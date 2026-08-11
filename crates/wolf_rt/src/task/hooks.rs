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

/// The phase of a blocking channel edge: `Block` when the operation
/// parks (spec/07's decision point — which task runs next), `Commit`
/// when the k-th send↔recv pairing lands (the `[conc.mm.hb.chan]`
/// edge the recorder pairs per channel, per k). The sprint inventory's
/// send-block / send-commit / recv-block / recv-commit factor as
/// kind (`chan.send` / `chan.recv`) × this phase — the KIND vocabulary
/// stays exactly spec/07's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChanPhase {
    /// The op found no partner/space and is about to park.
    Block,
    /// The op committed (paired, buffered, or drained).
    Commit,
}

/// A schedule-point event — the s32+s33 slice of the spec/07 taxonomy
/// (`[sched.point.set]`).
///
/// Discriminants and layout are UNSTABLE (numbering is s36 Phase B's;
/// see the module doc). The s33 widening carries exactly the spec/07
/// kinds: `chan.send`/`chan.recv` ([`SchedEvent::ChanSend`] /
/// [`SchedEvent::ChanRecv`]), `select.arm` ([`SchedEvent::SelectArm`]),
/// the appended `acquire` and `chan.close` kinds
/// ([`SchedEvent::Acquire`] / [`SchedEvent::ChanClose`] — appended to
/// the doc per `[sched.stable]`'s append rule; `acquire` is
/// sched-ev/0 kind 5's native twin, `[conc.when.order]`), and the
/// RESERVED `timer.fire` kind, activated by s33's timeout arms
/// (`[sched.stable]`: implementing a reserved kind activates its
/// name; s35's timer wheel inherits it). The s34 widening appends
/// the proc kinds per `[sched.stable]`'s append rule: `proc.spawn`,
/// `proc.kill`, `proc.exit` ([`SchedEvent::ProcSpawn`] /
/// [`SchedEvent::ProcKill`] / [`SchedEvent::ProcExit`] — sched-ev/0
/// kind 8's native twin; monitor/link DELIVERIES ride the existing
/// `chan.send` edges, so delivery order is already recorded). The s35
/// widening appends `io.arrive` ([`SchedEvent::IoArrive`] — the
/// reactor's completion-arrival decision, appended per
/// `[sched.stable]`'s append rule: the net module's reserved
/// "completion-arrival appends its own kind" note, activated) and
/// gives the activated `timer.fire` kind a second producer, the
/// reactor's timer wheel (same kind — `[sched.stable]`: the wheel
/// inherits the name s33 activated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// A region moved across a spawn/send boundary (`sync.transfer`
    /// seam; s33's moving sends route through it too).
    RegionTransfer,
    /// kind `chan.send` — a send edge on channel `chan`.
    ChanSend { chan: u64, phase: ChanPhase },
    /// kind `chan.recv` — a receive edge on channel `chan`.
    ChanRecv { chan: u64, phase: ChanPhase },
    /// kind `chan.close` — `close` wakes every waiter (an
    /// interleaving-visible decision; sprint inventory's `close`).
    ChanClose { chan: u64 },
    /// kind `select.arm` — the committed arm among the ready set
    /// (`[conc.select.fair]`: the choice is the seeded PRNG's;
    /// `ready` is the ready-arm bitmask, recorded with the choice).
    SelectArm { chosen: u32, ready: u64 },
    /// kind `acquire` — one sync object acquired, in canonical order
    /// (`[conc.when.order]`). `idx`/`set_len` tie the steps of one
    /// `when` whole-set acquisition together: the recorder folds
    /// them into one event carrying the set's ids (sprint inventory's
    /// when-acquire(set)).
    Acquire { obj: u64, idx: u32, set_len: u32 },
    /// kind `timer.fire` — a timeout arm's deadline fired (s33
    /// activates the reserved kind; virtual under the s36 test
    /// scheduler, monotonic in production — `[conc.select.timeout]`).
    /// s35's reactor timer wheel emits the same kind when a parked io
    /// wait's deadline fires (the wheel inherits the name).
    TimerFire,
    /// kind `io.arrive` — a pending io completion was delivered to
    /// its parked waiter (s35's reactor). WHICH pending completion is
    /// delivered next, and when, is the interleaving decision — the
    /// s36 `--chaos` delay/reorder seam; `token` is the reactor's
    /// submission token, the subject id.
    IoArrive { token: u64 },
    /// kind `proc.spawn` — a proc came up under the root supervisor
    /// (`[conc.task.root]`; `proc` is the packed generational id).
    ProcSpawn { proc: u64 },
    /// kind `proc.kill` — kill teardown was requested for `proc`
    /// (`[conc.proc.kill]` step 1 begins here; the s36 `--chaos`
    /// kill-injection point).
    ProcKill { proc: u64 },
    /// kind `proc.exit` — `proc`'s exit reason was determined
    /// (sched-ev/0 kind 8's native twin; `kind` is the reason class:
    /// 0 normal, 1 error, 2 killed, 3 cancelled — `[conc.proc.exit]`).
    /// Monitor/link deliveries then ride ordinary `chan.send` edges.
    ProcExit { proc: u64, kind: u8 },
}

/// The one seam. Inlines to nothing outside test builds (`test` or
/// the `sched-test` feature — the debug-tier runtime's hook switch;
/// release staticlibs carry no hook code, D15/D5 bench-verified).
#[inline(always)]
pub fn sched_point(event: SchedEvent) {
    #[cfg(any(test, feature = "sched-test"))]
    {
        super::det::on_event(&event);
        test_hook::dispatch(&event);
    }
    #[cfg(not(any(test, feature = "sched-test")))]
    let _ = event;
}

/// `sched-ev/1` kind numbering (s36 Phase B, spec/07 §1.1) —
/// append-only per `[sched.stable]`: new kinds take the next number,
/// nothing renumbers. Kind 0 is `pick` (the grant decision itself,
/// emitted by the det scheduler, not by a seam site).
pub fn kind_code(ev: &SchedEvent) -> u8 {
    match ev {
        SchedEvent::Spawn { .. } => 1,
        SchedEvent::Join { .. } => 2,
        SchedEvent::Park { .. } => 3,
        SchedEvent::Unpark { .. } => 4,
        SchedEvent::Steal { .. } => 5,
        SchedEvent::CancelCheck { .. } => 6,
        SchedEvent::RegionTransfer => 7,
        SchedEvent::ChanSend { .. } => 8,
        SchedEvent::ChanRecv { .. } => 9,
        SchedEvent::ChanClose { .. } => 10,
        SchedEvent::SelectArm { .. } => 11,
        SchedEvent::Acquire { .. } => 12,
        SchedEvent::TimerFire => 13,
        SchedEvent::IoArrive { .. } => 14,
        SchedEvent::ProcSpawn { .. } => 15,
        SchedEvent::ProcKill { .. } => 16,
        SchedEvent::ProcExit { .. } => 17,
    }
}

/// `[sched.seed]`'s namespace split and the `w1-` schedule token —
/// shared with the driver's `--replay` parser (pure encoding; unused
/// symbols drop from release links).
pub mod seed_spec {
    /// Bit 62: set = packed mixed-radix schedule, clear = simple seed.
    pub const PACKED_BIT: u64 = 1 << 62;

    /// Crockford-ish base32 (lowercase, no i/l/o/u) for `w1-` tokens.
    const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

    /// Pack `(choice, radix)` decisions into a bit-62 seed
    /// (little-endian mixed radix: the FIRST decision is the lowest
    /// digit). `None` when the schedule does not fit below 2^62.
    pub fn encode_packed(decisions: &[(u32, u32)]) -> Option<u64> {
        let mut v: u64 = 0;
        let mut scale: u64 = 1;
        for &(c, r) in decisions {
            debug_assert!(r > 0 && c < r);
            v = v.checked_add(scale.checked_mul(u64::from(c))?)?;
            scale = scale.checked_mul(u64::from(r.max(1)))?;
            if v >= PACKED_BIT || scale > PACKED_BIT {
                return None;
            }
        }
        Some(v | PACKED_BIT)
    }

    /// Render a packed seed as the short `w1-` token bs07 asked for
    /// (19-digit decimal seeds are hostile; 13 base32 digits are not).
    pub fn format_token(seed: u64) -> String {
        let mut v = seed & !PACKED_BIT;
        let mut out = String::from("w1-");
        let mut digits = Vec::new();
        loop {
            digits.push(ALPHABET[(v % 32) as usize] as char);
            v /= 32;
            if v == 0 {
                break;
            }
        }
        out.extend(digits.iter().rev());
        out
    }

    /// Parse a `w1-` token back to its packed seed (bit 62 set).
    pub fn parse_token(s: &str) -> Option<u64> {
        let body = s.strip_prefix("w1-")?;
        if body.is_empty() || body.len() > 13 {
            return None;
        }
        let mut v: u64 = 0;
        for b in body.bytes() {
            let d = ALPHABET.iter().position(|&a| a == b)? as u64;
            v = v.checked_mul(32)?.checked_add(d)?;
        }
        if v >= PACKED_BIT {
            return None;
        }
        Some(v | PACKED_BIT)
    }
}

// ---- det-engine shims -----------------------------------------------------
//
// The deterministic scheduler's seams, compiled to nothing outside
// hooked builds so pool/chan call sites stay free of cfg noise. Each
// shim forwards to `super::det` in test builds and inlines away in
// release (the D15/D5 zero-cost obligation, bench- and
// symbol-verified).

#[inline(always)]
#[allow(unused_variables)]
pub(crate) fn det_note_spawn(scope: &std::sync::Arc<super::scope::ScopeInner>) {
    #[cfg(any(test, feature = "sched-test"))]
    super::det::note_spawn(scope);
}

#[inline(always)]
pub(crate) fn det_ctx_exit() {
    #[cfg(any(test, feature = "sched-test"))]
    super::det::ctx_exit();
}

#[inline(always)]
pub(crate) fn det_block_enter() {
    #[cfg(any(test, feature = "sched-test"))]
    super::det::block_enter();
}

#[inline(always)]
pub(crate) fn det_block_exit() {
    #[cfg(any(test, feature = "sched-test"))]
    super::det::block_exit();
}

#[inline(always)]
#[allow(unused_variables)]
pub(crate) fn det_choose(n: usize) -> Option<usize> {
    #[cfg(any(test, feature = "sched-test"))]
    {
        super::det::choose(n)
    }
    #[cfg(not(any(test, feature = "sched-test")))]
    {
        None
    }
}

#[inline(always)]
#[allow(unused_variables)]
pub(crate) fn det_arm_timer(dur: std::time::Duration) {
    #[cfg(any(test, feature = "sched-test"))]
    super::det::arm_timer(dur);
}

#[inline(always)]
#[allow(unused_variables)]
pub(crate) fn det_timer_register(deadline: Option<std::time::Instant>) -> Option<u64> {
    #[cfg(any(test, feature = "sched-test"))]
    {
        super::det::timer_register(deadline)
    }
    #[cfg(not(any(test, feature = "sched-test")))]
    {
        None
    }
}

#[inline(always)]
#[allow(unused_variables)]
pub(crate) fn det_timer_fired(id: u64) -> bool {
    #[cfg(any(test, feature = "sched-test"))]
    {
        super::det::timer_fired(id)
    }
    #[cfg(not(any(test, feature = "sched-test")))]
    {
        false
    }
}

#[inline(always)]
#[allow(unused_variables)]
pub(crate) fn det_timer_done(id: Option<u64>) {
    #[cfg(any(test, feature = "sched-test"))]
    if let Some(id) = id {
        super::det::timer_done(id);
    }
}

#[inline(always)]
pub(crate) fn det_poll_tick() {
    #[cfg(any(test, feature = "sched-test"))]
    super::det::poll_tick();
}

#[inline(always)]
#[allow(unused_variables)]
pub(crate) fn det_poll_period(default: std::time::Duration) -> std::time::Duration {
    #[cfg(any(test, feature = "sched-test"))]
    {
        super::det::poll_period(default)
    }
    #[cfg(not(any(test, feature = "sched-test")))]
    {
        default
    }
}

#[cfg(any(test, feature = "sched-test"))]
pub mod test_hook {
    //! The pluggable observer for test builds (s36 Phase B's socket).
    use super::SchedEvent;
    use std::sync::{Mutex, OnceLock};

    type Hook = Box<dyn Fn(&SchedEvent) + Send + Sync>;
    static HOOK: OnceLock<Mutex<Option<Hook>>> = OnceLock::new();

    /// Serializes tests that install the process-wide observer (the
    /// hook is global; concurrent installers would drop each other's
    /// events). Lock it for the whole observed section.
    pub static SERIAL: Mutex<()> = Mutex::new(());

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

    /// Split the stream for the n-th `select` decision — the
    /// ready-arm tie-break (`[conc.select.fair]`; sprint rule:
    /// runtime-owned randomness, never ambient entropy). Derived from
    /// the root seed and the process-wide select index alone, so a
    /// seed reproduces every choice; domain-separated from the worker
    /// streams by a constant.
    pub fn for_select(seq: u64) -> SchedRng {
        SchedRng(root_seed() ^ 0x5E1E_C7A2_B00C_0FFE ^ seq.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    /// A stream from an explicit seed — the fairness-under-seed
    /// acceptance drives the select tie-break with pinned seeds.
    pub fn from_seed(seed: u64) -> SchedRng {
        SchedRng(seed)
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
