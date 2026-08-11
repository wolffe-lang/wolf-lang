//! The schedule-point seam (s32 Target 7; X12) — every scheduler
//! decision flows through [`sched_point`], and every drop of runtime
//! nondeterminism draws from the runtime-owned seeded PRNG here.
//!
//! # The seam contract (and its gate)
//!
//! s36 Phase A owns the *design doc* for this seam: the closed event
//! taxonomy, the stable event numbering, the `sched_point(event) ->
//! Decision` ABI, the recording schema. **That doc gates s32's merge**
//! (both sprint files say so). What lands here is the s32 half:
//!
//! - every decision the s32 scheduler makes — spawn, park, unpark,
//!   steal (victim included), join, cancel-check, region transfer —
//!   already routes through the single [`sched_point`] call;
//! - the events carry no stable numbering (X12 reserves sched-ev
//!   numbers for the s36 doc; the enum is layout- and
//!   discriminant-unstable on purpose);
//! - normal builds compile the seam to nothing (`cfg(test)`-gated
//!   dispatch — the release staticlib contains no hook code at all;
//!   the s36 doc decides the final cfg-vs-fn-pointer strategy and owns
//!   the zero-cost bench obligation);
//! - test builds route events to a pluggable observer
//!   ([`set_test_hook`]) — the embryo of s36 Phase B's pluggable
//!   scheduler. The observer sees events; it cannot yet *decide*
//!   (Decision = proceed, always). Widening the return type to the
//!   doc's `Decision` is the s36 Phase B change, at this one site.
//!
//! # The PRNG ownership rule
//!
//! ALL runtime nondeterminism — today that is exactly steal-victim
//! choice — draws from [`SchedRng`], a SplitMix64 stream split per
//! worker from one runtime-owned root seed (`WOLF_SCHED_SEED` if set,
//! a fixed constant otherwise). No ambient entropy, no `rand()`
//! anywhere in `wolf_rt`: a seed plus the OS's timing is the complete
//! description of a schedule, and once s36 virtualizes time the seed
//! alone is.

/// A schedule-point event — s32's slice of the s36 taxonomy.
///
/// Discriminants and layout are UNSTABLE (numbering is the s36 doc's;
/// see the module doc). Channel/select/proc/io events join in s33–s35;
/// `TimerFire` is declared now so the s33 timer lands inside the seam
/// rather than around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // TimerFire: declared for s33, unreachable in s32.
pub enum SchedEvent {
    /// A task was made runnable under a scope.
    Spawn { task: u64, scope: u64 },
    /// A worker found no work and is about to sleep.
    Park { worker: usize },
    /// A sleeping worker is being woken for new work.
    Unpark { worker: usize },
    /// A steal attempt — the victim choice is the nondeterminism.
    Steal { thief: usize, victim: usize },
    /// A scope owner is about to block for its children.
    Join { scope: u64 },
    /// A cancellation poll at a runtime-owned blocking point.
    CancelCheck { scope: u64, cancelled: bool },
    /// A region moved across a spawn boundary (`sync.transfer` seam).
    RegionTransfer,
    /// Reserved: the s33 timer wheel fires through the seam.
    TimerFire,
}

/// The one seam. Inlines to nothing outside test builds.
#[inline(always)]
pub fn sched_point(event: SchedEvent) {
    #[cfg(test)]
    test_hook::dispatch(&event);
    #[cfg(not(test))]
    let _ = event;
}

#[cfg(test)]
pub mod test_hook {
    //! The pluggable observer for test builds (s36 Phase B's socket).
    use super::SchedEvent;
    use std::sync::{Mutex, OnceLock};

    type Hook = Box<dyn Fn(&SchedEvent) + Send + Sync>;
    static HOOK: OnceLock<Mutex<Option<Hook>>> = OnceLock::new();

    fn cell() -> &'static Mutex<Option<Hook>> {
        HOOK.get_or_init(|| Mutex::new(None))
    }

    /// Install (or clear) the process-wide event observer.
    pub fn set_test_hook(hook: Option<Hook>) {
        *cell().lock().unwrap() = hook;
    }

    pub(super) fn dispatch(event: &SchedEvent) {
        // Fast path: never initialized — no hook was ever installed.
        let Some(m) = HOOK.get() else { return };
        if let Some(h) = m.lock().unwrap().as_ref() {
            h(event);
        }
    }
}

/// The runtime-owned schedule PRNG: SplitMix64, split per worker from
/// the root seed. Deterministic given the seed; the only entropy the
/// scheduler ever consumes.
pub struct SchedRng(u64);

impl SchedRng {
    /// Split a per-worker stream off the root seed.
    pub fn for_worker(index: usize) -> SchedRng {
        // golden-ratio increment keeps sibling streams decorrelated.
        SchedRng(root_seed() ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    fn next_u64(&mut self) -> u64 {
        // SplitMix64 (Steele/Lea/Flood) — public-domain reference mix.
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-enough draw in `0..n` (n > 0).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// Root seed: `WOLF_SCHED_SEED` (decimal u64) or a fixed constant.
/// Never wall-clock, never ASLR, never `/dev/urandom` — the PRNG
/// ownership rule above.
fn root_seed() -> u64 {
    use std::sync::OnceLock;
    static SEED: OnceLock<u64> = OnceLock::new();
    *SEED.get_or_init(|| {
        std::env::var("WOLF_SCHED_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(SEED_DEFAULT)
    })
}

/// The fixed default root seed: arbitrary, stable across builds.
const SEED_DEFAULT: u64 = 0x5EED_00D5_32AA_F00D;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic_per_worker() {
        let mut a = SchedRng::for_worker(3);
        let mut b = SchedRng::for_worker(3);
        let mut c = SchedRng::for_worker(4);
        let sa: Vec<usize> = (0..16).map(|_| a.below(10)).collect();
        let sb: Vec<usize> = (0..16).map(|_| b.below(10)).collect();
        let sc: Vec<usize> = (0..16).map(|_| c.below(10)).collect();
        assert_eq!(sa, sb);
        assert_ne!(sa, sc);
    }
}
