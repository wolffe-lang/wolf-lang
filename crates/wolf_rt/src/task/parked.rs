//! Parked waits, woken by cancellation (s231, wolf-lang#713).
//!
//! A task parked in a runtime-owned wait — a reactor wait (`net_read`,
//! `net_accept`, …), a channel operation, a `when` acquisition — must
//! leave it when its scope is cancelled or its proc killed
//! (`[conc.cancel.points]`, `[conc.proc.kill]`). Until s231 every such
//! wait woke each 5 ms to look: a parked task cost ~200 wakeups a
//! second for as long as it stayed parked (`[conc.task.order]`'s cost
//! paragraph measured ~196), so 10,000 parked tasks cost two million.
//!
//! Now a wait REGISTERS while it parks, and [`wake_all`] — called by
//! every cancellation ([`super::scope::ScopeInner::cancel`], which kill
//! goes through) — nudges each registered wait to re-check. A parked
//! task then costs nothing until an event names it: its completion, its
//! deadline, or a cancellation somewhere in the process. Cancellations
//! are rare and a nudge is a lock and a notify, so waking every parked
//! wait (not only those under the cancelled scope, which would need the
//! scope tree walked downwards) is the simple, correct choice.
//!
//! The protocol that makes a lost wakeup impossible: the waiter
//! registers BEFORE it takes its own lock and checks for cancellation;
//! the canceller sets the flag BEFORE it calls [`wake_all`], and each
//! [`Wake::wake`] takes the waiter's lock before it notifies. So either
//! the waiter's check sees the flag, or the waiter is already in its
//! condvar wait when the notify comes.
//!
//! Lock order: a registry shard, then a waiter's own lock (inside
//! `wake`). A waiter never holds its own lock while it registers or
//! deregisters — [`Parked`] is created before that lock is taken and
//! dropped after it is released.
//!
//! The det domain (sched-test builds) keeps its timed waits: its
//! scheduler is driven by parked waiters polling ([`super::hooks`]).

use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::{Mutex, OnceLock};

/// A parked wait that can be nudged to re-check its condition.
pub(crate) trait Wake: Sync {
    /// Take the wait's own lock and notify its waiter.
    fn wake(&self);
}

/// A registered wait, by address. The pointee outlives the entry:
/// [`Parked`] borrows it and removes the entry (under the shard lock)
/// before the borrow ends.
struct Entry(*const (dyn Wake + 'static));

// SAFETY: the pointee is `Sync` (the trait bound) and lives until the
// entry is removed; the entry only crosses threads inside the registry,
// and is dereferenced only under its shard's lock.
unsafe impl Send for Entry {}

const SHARDS: usize = 16;

struct Registry {
    shards: [Mutex<HashMap<u64, Entry>>; SHARDS],
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(0);

fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| Registry {
        shards: std::array::from_fn(|_| Mutex::new(HashMap::new())),
    })
}

/// A wait's registration; dropping it deregisters.
pub(crate) struct Parked<'a> {
    id: u64,
    _wait: PhantomData<&'a dyn Wake>,
}

/// Register `w` for cancellation nudges while its waiter is parked. Hold
/// the result across the wait; take the wait's own lock only after this
/// returns, and drop the result only after releasing that lock.
pub(crate) fn park_on<'a>(w: &'a (dyn Wake + 'a)) -> Parked<'a> {
    let id = NEXT.fetch_add(1, SeqCst);
    let p: *const (dyn Wake + 'a) = w;
    // SAFETY: only the lifetime is erased. `Parked<'a>` removes the
    // entry before `'a` ends, and the pointer is only dereferenced
    // under the shard lock that removal takes.
    let p: *const (dyn Wake + 'static) = unsafe { std::mem::transmute(p) };
    registry().shards[id as usize % SHARDS]
        .lock()
        .unwrap()
        .insert(id, Entry(p));
    Parked {
        id,
        _wait: PhantomData,
    }
}

impl Drop for Parked<'_> {
    fn drop(&mut self) {
        registry().shards[self.id as usize % SHARDS]
            .lock()
            .unwrap()
            .remove(&self.id);
    }
}

/// Nudge every parked wait to re-check (a cancellation or kill was just
/// recorded). A process that never parked a wait never builds the
/// registry.
pub(crate) fn wake_all() {
    let Some(reg) = REGISTRY.get() else {
        return;
    };
    for shard in &reg.shards {
        let entries = shard.lock().unwrap();
        for e in entries.values() {
            // SAFETY: registered entries are live (see `Entry`), and the
            // shard lock held here excludes their removal.
            unsafe { (*e.0).wake() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct Count(AtomicUsize);

    impl Wake for Count {
        fn wake(&self) {
            self.0.fetch_add(1, SeqCst);
        }
    }

    /// A registered wait is nudged by every `wake_all` while it is
    /// registered, and never after its registration drops.
    #[test]
    fn wake_all_nudges_exactly_the_registered_waits() {
        let a = Count(AtomicUsize::new(0));
        let b = Count(AtomicUsize::new(0));
        let pa = park_on(&a);
        {
            let _pb = park_on(&b);
            wake_all();
        }
        wake_all();
        drop(pa);
        wake_all();
        // Other tests in this binary may cancel scopes concurrently, so
        // the counts are lower bounds on the nudges while registered
        // and exact on the absence of any after.
        let (na, nb) = (a.0.load(SeqCst), b.0.load(SeqCst));
        assert!(na >= 2, "a was registered across two wake_all calls: {na}");
        assert!(nb >= 1, "b was registered across one wake_all call: {nb}");
        let (na2, nb2) = (na, nb);
        wake_all();
        assert_eq!(
            (a.0.load(SeqCst), b.0.load(SeqCst)),
            (na2, nb2),
            "a deregistered wait is never nudged"
        );
    }
}
