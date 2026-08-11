//! `when (a, b)` whole-set acquisition (s33) — the runtime half of
//! the language construct (03 Q6; s27's lowering will emit calls to
//! the entry points here).
//!
//! # The mechanism (`[conc.when.order]`, the BoC/Verona cown design)
//!
//! Every sync cell carries a stable global acquisition id assigned at
//! creation — ONE total order over all sync objects in the process.
//! `when` sorts its operand set by id and acquires in that order,
//! releasing in reverse at block exit; `when (a, b)` and `when (b, a)`
//! perform identical acquisitions. A task blocked mid-set holds only
//! objects earlier in the canonical order than the one it awaits, so
//! no cycle of `when` acquisitions can form — deadlock freedom by
//! construction (`[conc.when.nodeadlock]`).
//!
//! Nesting is a compile error (E1103, `[conc.when.nonest]` — the
//! checker's job when c05 typing lands); the runtime debug-asserts
//! the same invariant (defense in depth — unsafe code could smuggle a
//! lock in). *Dynamic* self-acquisition through a call — acquiring a
//! cell the task already holds — can never complete and is
//! `trap(deadlock)` (`[conc.deadlock.self]`): the Rust surface
//! reports it as a value for tests, the C surface fires
//! `__wolf_rt_trap(DEADLOCK)`.
//!
//! Each per-object acquisition is a schedule point (kind `acquire`,
//! spec/07; `idx`/`set_len` tie one `when`'s steps together so the
//! recorder folds them into one whole-set event with the set's ids),
//! and `[conc.mm.hb.mutex]`'s release→acquire edge counts per object
//! — carried here by each cell's own mutex. Blocked acquisition parks
//! with blocking compensation and is a cancellation point
//! (`[conc.cancel.points]`): a cancelled mid-set acquire releases its
//! prefix in reverse and surfaces the cancellation value.

use core::ffi::c_void;
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use super::hooks::{SchedEvent, sched_point};
use super::pool::{blocking, current_scope};

/// Creation-order acquisition ids — the canonical total order
/// (`[conc.when.order]`: "creation order; a recorded decision").
static NEXT_SYNC: AtomicU64 = AtomicU64::new(1);

/// Per-thread holder token (tasks run to completion on one thread —
/// s32's model — so the thread token identifies the acquiring task).
static NEXT_HOLDER: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static HOLDER: Cell<u64> = const { Cell::new(0) };
    /// Depth of `when` bodies on this task's stack — the nesting
    /// debug-assert's evidence.
    static WHEN_DEPTH: Cell<u32> = const { Cell::new(0) };
}

fn holder_token() -> u64 {
    HOLDER.with(|h| {
        if h.get() == 0 {
            h.set(NEXT_HOLDER.fetch_add(1, SeqCst));
        }
        h.get()
    })
}

/// Why a whole-set acquisition did not complete. Values, never
/// unwinds (D30); the C surface maps `SelfAcquire` to
/// `trap(deadlock)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhenErr {
    /// Cancellation surfaced while blocked mid-set; the acquired
    /// prefix was released in reverse.
    Cancelled,
    /// The task already holds one of the operands
    /// (`[conc.deadlock.self]`: can never complete).
    SelfAcquire,
}

/// A sync cell — the runtime object behind the `sync` wrapper types
/// (`Mutex` et al. wrap it in s37/s38). Guards one payload word with
/// exclusive access while held (`[conc.when.body]`; layout freedom
/// for real payloads stays with codegen, same one-word contract as
/// channels).
pub struct SyncCell {
    id: u64,
    st: Mutex<u64>, // holder token; 0 = free
    cv: Condvar,
    payload: Cell<u64>,
}

// SAFETY: `payload` is read/written only between acquire and release
// of `st`'s holder discipline — exclusive access is the `when` body's
// contract (`[conc.when.body]`), so the Cell never sees concurrent
// use.
unsafe impl Send for SyncCell {}
// SAFETY: as above — cross-thread access is serialized by holding.
unsafe impl Sync for SyncCell {}

impl SyncCell {
    /// Create a cell; its acquisition id is the next in the one
    /// canonical order.
    pub fn new(payload: u64) -> Arc<SyncCell> {
        Arc::new(SyncCell {
            id: NEXT_SYNC.fetch_add(1, SeqCst),
            st: Mutex::new(0),
            cv: Condvar::new(),
            payload: Cell::new(payload),
        })
    }

    /// The stable acquisition id (canonical order; hook payloads).
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Acquire this one cell for `me`; step `idx` of a `set_len`-set
    /// acquisition. Blocks (with compensation) under contention; a
    /// cancellation point.
    fn acquire(&self, me: u64, idx: u32, set_len: u32) -> Result<(), WhenErr> {
        let mut holder = self.st.lock().unwrap();
        if *holder == me {
            return Err(WhenErr::SelfAcquire);
        }
        if *holder == 0 {
            *holder = me;
            sched_point(SchedEvent::Acquire {
                obj: self.id,
                idx,
                set_len,
            });
            return Ok(());
        }
        drop(holder);
        blocking(|| {
            let scope = current_scope();
            let mut holder = self.st.lock().unwrap();
            loop {
                if *holder == 0 {
                    *holder = me;
                    sched_point(SchedEvent::Acquire {
                        obj: self.id,
                        idx,
                        set_len,
                    });
                    return Ok(());
                }
                if scope.as_ref().is_some_and(|s| s.is_cancelled()) {
                    sched_point(SchedEvent::CancelCheck {
                        scope: scope.as_ref().map_or(0, |s| s.id()),
                        cancelled: true,
                    });
                    return Err(WhenErr::Cancelled);
                }
                // 5ms poll backstop for enclosing-scope cancellation;
                // real wakeups arrive via the release's notify.
                let (g, _) = self
                    .cv
                    .wait_timeout(holder, Duration::from_millis(5))
                    .unwrap();
                holder = g;
            }
        })
    }

    /// Release (must hold). The release→next-acquire edge is
    /// `[conc.mm.hb.mutex]`'s, carried by this mutex.
    fn release(&self, me: u64) {
        let mut holder = self.st.lock().unwrap();
        debug_assert_eq!(*holder, me, "sync cell released by a non-holder");
        *holder = 0;
        self.cv.notify_all();
    }

    /// The guarded payload word — valid to touch only while held
    /// (`[conc.when.body]`'s exclusive access).
    pub fn payload_ptr(&self) -> *mut u64 {
        self.payload.as_ptr()
    }
}

/// Sorted canonical acquisition order for a set (indices into
/// `cells`); `Err(SelfAcquire)` on a duplicate operand — acquiring
/// the same cell twice in one set can never complete.
fn canonical_order(cells: &[&SyncCell]) -> Result<Vec<usize>, WhenErr> {
    let mut order: Vec<usize> = (0..cells.len()).collect();
    order.sort_by_key(|&i| cells[i].id);
    if order.windows(2).any(|w| cells[w[0]].id == cells[w[1]].id) {
        return Err(WhenErr::SelfAcquire);
    }
    Ok(order)
}

/// Acquire the ENTIRE ordered set — the `when` entry. On `Ok`, the
/// caller runs the body with exclusive access to every payload and
/// must call [`when_release`] with the same set at block exit.
///
/// Debug builds assert the no-nesting invariant (`[conc.when.nonest]`
/// is E1103 statically; this is the defense-in-depth twin).
pub fn when_acquire(cells: &[&SyncCell]) -> Result<(), WhenErr> {
    WHEN_DEPTH.with(|d| {
        debug_assert_eq!(
            d.get(),
            0,
            "nested acquisition inside a `when` body ([conc.when.nonest]): \
             the compiler forbids this lexically; something smuggled a lock in"
        );
    });
    let order = canonical_order(cells)?;
    let me = holder_token();
    let set_len = order.len() as u32;
    let mut held: Vec<usize> = Vec::with_capacity(order.len());
    for (k, &i) in order.iter().enumerate() {
        match cells[i].acquire(me, k as u32, set_len) {
            Ok(()) => held.push(i),
            Err(e) => {
                // Release the prefix in reverse; the operation
                // resolves without the set (never torn — peers only
                // ever saw fully-released cells).
                for &j in held.iter().rev() {
                    cells[j].release(me);
                }
                return Err(e);
            }
        }
    }
    WHEN_DEPTH.with(|d| d.set(d.get() + 1));
    Ok(())
}

/// Release the whole set in reverse canonical order — the `when`
/// block exit.
pub fn when_release(cells: &[&SyncCell]) {
    let order = match canonical_order(cells) {
        Ok(o) => o,
        Err(_) => unreachable!("released a set that could not have been acquired"),
    };
    let me = holder_token();
    for &i in order.iter().rev() {
        cells[i].release(me);
    }
    WHEN_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
}

/// Run `body` holding the whole set — the Rust scaffolding twin of
/// the compiled `when (a, b) { }` lowering (tests and s34+ drive the
/// runtime through it). Panic-safe: a Rust-side panic in the body
/// releases the set on the way to the task boundary (compiled code
/// has no unwinding — D30 — so the raw entry points need no guard).
pub fn when<R>(cells: &[&SyncCell], body: impl FnOnce() -> R) -> Result<R, WhenErr> {
    when_acquire(cells)?;
    struct Release<'a, 'c>(&'a [&'c SyncCell]);
    impl Drop for Release<'_, '_> {
        fn drop(&mut self) {
            when_release(self.0);
        }
    }
    let guard = Release(cells);
    let r = body();
    drop(guard);
    Ok(r)
}

// ---- the C entry surface -------------------------------------------------
//
// Frozen ahead of consumption (the s32 lesson): s27's `when` lowering
// emits calls here once c05 typing lands.

/// Create a sync cell guarding `payload`. Returns the cell handle.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_sync_new(payload: u64) -> *mut c_void {
    Arc::into_raw(SyncCell::new(payload)).cast_mut().cast()
}

/// Release one cell handle.
///
/// # Safety
///
/// `cell` must be a live handle from [`__wolf_rt_sync_new`]; dead
/// after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_sync_free(cell: *mut c_void) {
    // SAFETY: consumes the caller's reference.
    drop(unsafe { Arc::from_raw(cell.cast::<SyncCell>()) });
}

/// The guarded payload word; valid to dereference only between
/// `when_acquire` and `when_release` of a set containing this cell.
///
/// # Safety
///
/// `cell` must be a live cell handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_sync_payload(cell: *mut c_void) -> *mut u64 {
    // SAFETY: borrow the caller's handle.
    let cell = unsafe { std::mem::ManuallyDrop::new(Arc::from_raw(cell.cast::<SyncCell>())) };
    cell.payload_ptr()
}

/// Acquire the whole set (`when` entry): 0 ok, 2 cancelled.
/// Self-acquisition (`[conc.deadlock.self]`) does not return — it is
/// `trap(deadlock)`.
///
/// # Safety
///
/// `cells` must address `n` live cell handles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_when_acquire(cells: *const *mut c_void, n: i64) -> i32 {
    // SAFETY: caller contract — n live handles, borrowed only.
    let (_arcs, refs) = unsafe { borrow_cells(cells, n) };
    match when_acquire(&refs) {
        Ok(()) => 0,
        Err(WhenErr::Cancelled) => 2,
        Err(WhenErr::SelfAcquire) => {
            crate::native::__wolf_rt_trap(crate::native::trap_code::DEADLOCK)
        }
    }
}

/// Release the whole set in reverse canonical order (`when` exit).
///
/// # Safety
///
/// `cells`/`n` exactly as passed to the matching successful
/// [`__wolf_rt_when_acquire`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_when_release(cells: *const *mut c_void, n: i64) {
    // SAFETY: caller contract — the acquired set, borrowed only.
    let (_arcs, refs) = unsafe { borrow_cells(cells, n) };
    when_release(&refs);
}

/// Borrow `n` cell handles without consuming them.
///
/// # Safety
///
/// `cells` must address `n` live cell handles.
unsafe fn borrow_cells<'a>(
    cells: *const *mut c_void,
    n: i64,
) -> (
    Vec<std::mem::ManuallyDrop<Arc<SyncCell>>>,
    Vec<&'a SyncCell>,
) {
    // SAFETY: caller contract.
    let ptrs = unsafe { std::slice::from_raw_parts(cells, usize::try_from(n).unwrap_or(0)) };
    let arcs: Vec<std::mem::ManuallyDrop<Arc<SyncCell>>> = ptrs
        .iter()
        // SAFETY: live handles per caller contract; borrowed only.
        .map(|&p| unsafe { std::mem::ManuallyDrop::new(Arc::from_raw(p.cast::<SyncCell>())) })
        .collect();
    let refs: Vec<&'a SyncCell> = arcs
        .iter()
        // SAFETY: the heap object is stable and the caller's own
        // handles (plus the ManuallyDrop borrows above) keep it live
        // for the call's duration.
        .map(|a| unsafe { &*Arc::as_ptr(&**a) })
        .collect();
    (arcs, refs)
}

// ---- acceptance tests ----------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::hooks::test_hook;
    use super::super::{ExitReason, SchedEvent, scope};
    use super::*;

    /// The inversion acceptance (`[conc.when.nodeadlock]`): two tasks
    /// acquire `(a, b)` and `(b, a)` in a tight loop — deadlock-free
    /// by canonical-order construction, and the payload count proves
    /// exclusive access held throughout (`[conc.when.body]`).
    #[test]
    fn when_inversion_no_deadlock() {
        const N: u64 = 1_000_000;
        let a = SyncCell::new(0);
        let b = SyncCell::new(0);
        let (a1, b1) = (a.clone(), b.clone());
        let (a2, b2) = (a.clone(), b.clone());
        let r = scope("inversion", |s| {
            s.spawn("ab", move |_| {
                for _ in 0..N {
                    when(&[&a1, &b1], || {
                        // SAFETY: exclusive access while held.
                        unsafe { *a1.payload_ptr() += 1 };
                    })
                    .unwrap();
                }
                ExitReason::Normal
            });
            s.spawn("ba", move |_| {
                for _ in 0..N {
                    when(&[&b2, &a2], || {
                        // SAFETY: exclusive access while held.
                        unsafe { *a2.payload_ptr() += 1 };
                    })
                    .unwrap();
                }
                ExitReason::Normal
            });
        });
        assert!(r.is_ok());
        let total = when(&[&a, &b], || {
            // SAFETY: exclusive access while held.
            unsafe { *a.payload_ptr() }
        })
        .unwrap();
        assert_eq!(total, 2 * N);
    }

    /// `[conc.when.order]`: the acquisition order is canonical
    /// (creation order), regardless of the order written at the site
    /// — observed through the seam's `acquire` events.
    #[test]
    fn acquisition_follows_canonical_order() {
        let _serial = test_hook::SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let a = SyncCell::new(0); // created first: earlier id
        let b = SyncCell::new(0);
        let seen: Arc<Mutex<Vec<(u64, u32, u32)>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let (ida, idb) = (a.id(), b.id());
        test_hook::set_test_hook(Some(Box::new(move |ev| {
            if let SchedEvent::Acquire { obj, idx, set_len } = ev
                && (*obj == ida || *obj == idb)
            {
                sink.lock().unwrap().push((*obj, *idx, *set_len));
            }
        })));
        when(&[&b, &a], || ()).unwrap(); // written inverted
        test_hook::set_test_hook(None);
        assert_eq!(*seen.lock().unwrap(), vec![(ida, 0, 2), (idb, 1, 2)]);
    }

    /// `[conc.deadlock.self]`'s value surface: a set naming the same
    /// cell twice can never complete — `SelfAcquire` (the C surface
    /// maps it to `trap(deadlock)`), with nothing left held.
    #[test]
    fn self_acquisition_is_deadlock_value() {
        let a = SyncCell::new(0);
        let b = SyncCell::new(0);
        assert_eq!(when_acquire(&[&a, &a]), Err(WhenErr::SelfAcquire));
        // Nothing held: a fresh whole-set acquire succeeds.
        assert_eq!(when(&[&a, &b], || 5), Ok(5));
    }

    /// The nesting debug-assert (`[conc.when.nonest]`'s
    /// defense-in-depth twin): a hand-written nested acquisition
    /// fires it. Panics become exit reasons at the task boundary
    /// (D30), so the scope reports `Panicked`.
    #[test]
    #[cfg(debug_assertions)]
    fn nested_acquisition_debug_asserts() {
        let a = SyncCell::new(0);
        let b = SyncCell::new(0);
        let c = SyncCell::new(0);
        let r = scope("nested", |s| {
            s.spawn("smuggler", move |_| {
                let _ = when(&[&a, &b], || when(&[&c, &b], || ()));
                ExitReason::Normal
            });
        });
        assert_eq!(r, Err(ExitReason::Panicked));
    }

    /// Cancellation at a blocked `when` acquire
    /// (`[conc.cancel.points]`): the cancel value surfaces, the
    /// acquired prefix is released, the cells stay usable.
    #[test]
    fn cancellation_surfaces_at_blocked_acquire() {
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
        let a = SyncCell::new(0);
        let b = SyncCell::new(0);
        let saw = Arc::new(AtomicUsize::new(0));
        // Hold the set on the main thread, then cancel the blocked
        // acquirer via a failing sibling.
        when_acquire(&[&a, &b]).unwrap();
        let (a2, b2) = (a.clone(), b.clone());
        let saw2 = saw.clone();
        let r = scope("cancel-when", |s| {
            s.spawn("blocked", move |_| {
                assert_eq!(when_acquire(&[&a2, &b2]), Err(WhenErr::Cancelled));
                saw2.fetch_add(1, SeqCst);
                ExitReason::Cancelled
            });
            std::thread::sleep(std::time::Duration::from_millis(30));
            s.spawn("failer", |_| ExitReason::Error { tag: 7 });
        });
        assert_eq!(r, Err(ExitReason::Error { tag: 7 }));
        assert_eq!(saw.load(SeqCst), 1);
        when_release(&[&a, &b]);
        // Uncorrupted: the whole set acquires cleanly again.
        assert_eq!(when(&[&a, &b], || 1), Ok(1));
    }

    /// The C symbol surface: sync_new → when_acquire → payload →
    /// when_release → sync_free, whole-set semantics intact.
    #[test]
    fn c_surface_round_trip() {
        // SAFETY: entry points used per their documented contracts.
        unsafe {
            let a = __wolf_rt_sync_new(10);
            let b = __wolf_rt_sync_new(20);
            let set = [b, a]; // inverted on purpose
            assert_eq!(__wolf_rt_when_acquire(set.as_ptr(), 2), 0);
            let pa = __wolf_rt_sync_payload(a);
            let pb = __wolf_rt_sync_payload(b);
            assert_eq!((*pa, *pb), (10, 20));
            *pa += 1;
            __wolf_rt_when_release(set.as_ptr(), 2);
            assert_eq!(__wolf_rt_when_acquire(set.as_ptr(), 2), 0);
            assert_eq!(*__wolf_rt_sync_payload(a), 11);
            __wolf_rt_when_release(set.as_ptr(), 2);
            __wolf_rt_sync_free(a);
            __wolf_rt_sync_free(b);
        }
    }
}
