//! Per-worker Chase–Lev work-stealing deque (s32 Target 2).
//!
//! The classic single-owner / multi-thief deque (Chase & Lev, SPAA'05;
//! orderings per Lê/Pop/Cohen/Nardelli PPoPP'13, taken conservatively
//! at `SeqCst` — the debug tier buys obviousness, c09 may relax with a
//! model checker in hand). Two v0 simplifications, both on the safe
//! side:
//!
//! - **Fixed capacity, no resize.** A full deque refuses the push and
//!   the caller overflows to the global injector. This removes the
//!   grow-and-reclaim hazard (the published algorithm's only memory-
//!   management subtlety) entirely.
//! - **Owner never blocks in here.** Pop and steal are wait-free /
//!   lock-free respectively; parking policy lives in the pool.
//!
//! Elements are raw `*mut T` owned pointers: a successful `push`
//! transfers ownership in; a successful `pop`/`steal` transfers it
//! out. The pool is the only client and drains queues before drop.

use std::sync::atomic::{AtomicIsize, AtomicPtr, Ordering::SeqCst, fence};

/// Capacity per worker. Overflow spills to the injector, so this caps
/// locality, not concurrency. 256 tasks of locality per worker is
/// plenty for the debug tier.
const CAP: usize = 256;
const MASK: isize = (CAP as isize) - 1;

/// The deque. `bottom` is the owner's end, `top` the thieves' end.
pub struct Deque<T> {
    top: AtomicIsize,
    bottom: AtomicIsize,
    slots: Box<[AtomicPtr<T>]>,
}

/// Result of a steal attempt.
pub enum Steal<T> {
    /// Nothing to take.
    Empty,
    /// Lost a race; caller may retry.
    Retry,
    /// Took the element.
    Taken(*mut T),
}

impl<T> Default for Deque<T> {
    fn default() -> Self {
        Deque::new()
    }
}

impl<T> Deque<T> {
    pub fn new() -> Deque<T> {
        let slots = (0..CAP)
            .map(|_| AtomicPtr::new(std::ptr::null_mut()))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Deque {
            top: AtomicIsize::new(0),
            bottom: AtomicIsize::new(0),
            slots,
        }
    }

    /// Owner-only. `Err` gives the element back: the deque is full and
    /// the caller must route it to the injector.
    pub fn push(&self, item: *mut T) -> Result<(), *mut T> {
        let b = self.bottom.load(SeqCst);
        let t = self.top.load(SeqCst);
        if b - t >= CAP as isize {
            return Err(item);
        }
        self.slots[(b & MASK) as usize].store(item, SeqCst);
        fence(SeqCst);
        self.bottom.store(b + 1, SeqCst);
        Ok(())
    }

    /// Owner-only LIFO pop.
    pub fn pop(&self) -> Option<*mut T> {
        let b = self.bottom.load(SeqCst) - 1;
        self.bottom.store(b, SeqCst);
        fence(SeqCst);
        let t = self.top.load(SeqCst);
        if t > b {
            // Empty: restore.
            self.bottom.store(b + 1, SeqCst);
            return None;
        }
        let item = self.slots[(b & MASK) as usize].load(SeqCst);
        if t == b {
            // Last element: race the thieves for it.
            let won = self.top.compare_exchange(t, t + 1, SeqCst, SeqCst).is_ok();
            self.bottom.store(b + 1, SeqCst);
            return won.then_some(item);
        }
        Some(item)
    }

    /// Thief-side FIFO steal.
    pub fn steal(&self) -> Steal<T> {
        let t = self.top.load(SeqCst);
        fence(SeqCst);
        let b = self.bottom.load(SeqCst);
        if t >= b {
            return Steal::Empty;
        }
        let item = self.slots[(t & MASK) as usize].load(SeqCst);
        if self.top.compare_exchange(t, t + 1, SeqCst, SeqCst).is_ok() {
            Steal::Taken(item)
        } else {
            Steal::Retry
        }
    }

    /// Approximate occupancy (racy; wakeup heuristics only).
    pub fn len_hint(&self) -> isize {
        (self.bottom.load(SeqCst) - self.top.load(SeqCst)).max(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn boxed(v: usize) -> *mut usize {
        Box::into_raw(Box::new(v))
    }
    unsafe fn unbox(p: *mut usize) -> usize {
        *unsafe { Box::from_raw(p) }
    }

    #[test]
    fn lifo_pop_fifo_steal() {
        let d: Deque<usize> = Deque::new();
        for v in 0..4 {
            d.push(boxed(v)).unwrap();
        }
        assert_eq!(unsafe { unbox(d.pop().unwrap()) }, 3);
        match d.steal() {
            Steal::Taken(p) => assert_eq!(unsafe { unbox(p) }, 0),
            _ => panic!("steal should take the oldest"),
        }
        assert_eq!(unsafe { unbox(d.pop().unwrap()) }, 2);
        assert_eq!(unsafe { unbox(d.pop().unwrap()) }, 1);
        assert!(d.pop().is_none());
        assert!(matches!(d.steal(), Steal::Empty));
    }

    #[test]
    fn full_deque_refuses() {
        let d: Deque<usize> = Deque::new();
        for v in 0..CAP {
            d.push(boxed(v)).unwrap();
        }
        let spill = boxed(999);
        let back = d.push(spill).unwrap_err();
        assert_eq!(back, spill);
        unsafe { unbox(back) };
        while let Some(p) = d.pop() {
            unsafe { unbox(p) };
        }
    }

    /// Owner pushes/pops while thieves hammer steal: every element is
    /// taken exactly once (counted), none lost, none duplicated.
    #[test]
    fn concurrent_steal_stress() {
        const N: usize = 20_000;
        const THIEVES: usize = 3;
        let d: Arc<Deque<usize>> = Arc::new(Deque::new());
        let taken = Arc::new(AtomicUsize::new(0));
        let sum = Arc::new(AtomicUsize::new(0));

        let mut thieves = Vec::new();
        for _ in 0..THIEVES {
            let d = d.clone();
            let taken = taken.clone();
            let sum = sum.clone();
            thieves.push(std::thread::spawn(move || {
                while taken.load(Ordering::SeqCst) < N {
                    match d.steal() {
                        Steal::Taken(p) => {
                            sum.fetch_add(unsafe { unbox(p) }, Ordering::SeqCst);
                            taken.fetch_add(1, Ordering::SeqCst);
                        }
                        Steal::Retry => std::hint::spin_loop(),
                        Steal::Empty => std::thread::yield_now(),
                    }
                }
            }));
        }

        let mut next = 0usize;
        while next < N {
            match d.push(boxed(next)) {
                Ok(()) => next += 1,
                Err(p) => {
                    // Full: act as owner-consumer for a moment.
                    unsafe { unbox(p) };
                    if let Some(q) = d.pop() {
                        sum.fetch_add(unsafe { unbox(q) }, Ordering::SeqCst);
                        taken.fetch_add(1, Ordering::SeqCst);
                    }
                    // Re-push the refused value next loop.
                    match d.push(boxed(next)) {
                        Ok(()) => next += 1,
                        Err(p) => {
                            unsafe { unbox(p) };
                        }
                    }
                }
            }
        }
        // Owner drains alongside the thieves.
        while taken.load(Ordering::SeqCst) < N {
            if let Some(p) = d.pop() {
                sum.fetch_add(unsafe { unbox(p) }, Ordering::SeqCst);
                taken.fetch_add(1, Ordering::SeqCst);
            } else {
                std::thread::yield_now();
            }
        }
        for th in thieves {
            th.join().unwrap();
        }
        assert_eq!(taken.load(Ordering::SeqCst), N);
        assert_eq!(sum.load(Ordering::SeqCst), N * (N - 1) / 2);
    }
}
