//! The s73 native-lowering ABI shims — the additions compiled wolf
//! code needs beyond the frozen s32–s34 surfaces, in their own module
//! (the sprint's footprint rule: new shims land here, the frozen
//! files stay frozen).
//!
//! # The compiled task-return protocol (the s32 extension, reviewed)
//!
//! s32 froze `entry(env) -> i64` as "0 = normal, nonzero = error tag".
//! Native lowering needs a third outcome: a task that stopped because
//! structured cancellation reached it (`[conc.proc.cancel]` /
//! `[conc.task.fail]`'s sibling-cancel) is `Cancelled`, not `Error` —
//! the two feed different exit reasons (a cancelled proc must report
//! reason `cancelled`, and a cancelled sibling is a consequence, not
//! a failure that would win `first_fail`). [`CANCEL_TAG`] is the
//! reserved return value: task/proc entry shims return it when the
//! body's error is the `cancelled` row tag (or after kill teardown),
//! and `pool::run_task` / [`__wolf_rt_proc_spawn_outcome`] map it to
//! the `Cancelled` reason. Real error tags are WIR module tag ids
//! (≥ 1) — the negative space is the runtime's to reserve
//! (`TRAP_ERROR_TAG` = -1 took it first).
//!
//! # The kill-teardown branch (`[conc.proc.kill]`, the c07 handoff)
//!
//! C ABI frames are never unwound (`[conc.cancel.c]`), so kill
//! teardown for compiled bodies is a LOWERING discipline, not an
//! unwind: at every runtime-owned blocking point that returns the
//! cancelled status, and on every error-propagation edge, compiled
//! code calls [`__wolf_rt_task_killed`] — and when it answers 1,
//! returns straight out of the frame WITHOUT running `defer`s or
//! `else` handlers (the no-defer teardown branch codegen owed s34).
//! What lands vs. what stays refused is the s73 ledger's:
//! blocking-point and error-edge teardown land; kill delivery to a
//! COMPUTE-BOUND native body still awaits the checkpoint story
//! (`--checked` back-edge polls, c07's recorded refusal), and a
//! killed task inside a row-less intermediate frame resumes that one
//! frame until its next blocking point or row edge (recorded gap,
//! same class).

use core::ffi::c_void;

use super::pool;
use super::proc::{ProcOutcome, spawn_proc};

/// The reserved compiled-task return value meaning "structured
/// cancellation stopped this body" (`ExitReason::Cancelled` /
/// `ProcExit::Cancelled`, never an error tag). WIR error tags are
/// ≥ 1; -1 is `TRAP_ERROR_TAG`.
pub const CANCEL_TAG: i64 = -2;

/// The kill-teardown poll for compiled bodies: 1 when the calling
/// task's scope sits in a killed tree (`[conc.proc.kill]` step 1 —
/// the caller must return without running further user code), else 0.
/// The cancel twin is the status value the blocking op already
/// returned; this only separates kill from polite cancel.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_task_killed() -> i8 {
    match pool::current_scope() {
        Some(s) => i8::from(s.is_killed()),
        None => 0,
    }
}

/// Spawn `entry(env)` as a proc under the root supervisor — the
/// compiled twin of `__wolf_rt_proc_spawn` with the three-outcome
/// return protocol: `0` → `normal(0)`, [`CANCEL_TAG`] → `cancelled`,
/// any other value → `error(tag)`. (The frozen s34 entry keeps its
/// two-outcome contract for the surfaces that froze against it.)
///
/// # Safety
///
/// `entry` must be callable with `env` (which moves — D14); `name`
/// must address `name_len` readable bytes when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_proc_spawn_outcome(
    entry: unsafe extern "C" fn(*mut c_void) -> i64,
    env: *mut c_void,
    name: *const u8,
    name_len: i64,
) -> u64 {
    let name = super::name_from_raw(name, name_len);
    let name = if name.is_empty() {
        "proc".to_string()
    } else {
        name
    };
    let env = pool::SendPtr(env);
    spawn_proc(&name, move |_| {
        let moved_env = env;
        // C ABI frames below: suppress the kill-teardown unwind for
        // their whole extent (`[conc.cancel.c]`); the compiled body
        // carries its own teardown branch.
        let tag = pool::suppress_kill_unwind(|| {
            // SAFETY: caller contract — lowered proc body + moved env.
            unsafe { entry(moved_env.0) }
        });
        match tag {
            0 => ProcOutcome::Value(0),
            CANCEL_TAG => ProcOutcome::Cancelled,
            tag => ProcOutcome::Fail { tag },
        }
    })
}

/// Read the guarded payload word of a sync cell. Valid only while the
/// calling task holds the cell's set (`[conc.when.body]` — the same
/// contract as `__wolf_rt_sync_payload`, as a call so lowered code
/// needs no foreign-memory tokens).
///
/// # Safety
///
/// `cell` must be a live handle from `__wolf_rt_sync_new`, held by
/// the calling task.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_sync_get(cell: *mut c_void) -> u64 {
    // SAFETY: caller contract — live handle, held.
    unsafe { *super::__wolf_rt_sync_payload(cell) }
}

/// Write the guarded payload word of a sync cell. Same contract as
/// [`__wolf_rt_sync_get`].
///
/// # Safety
///
/// As [`__wolf_rt_sync_get`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_sync_set(cell: *mut c_void, val: u64) {
    // SAFETY: caller contract — live handle, held.
    unsafe { *super::__wolf_rt_sync_payload(cell) = val }
}

#[cfg(test)]
mod tests {
    use super::super::proc::{ProcExit, monitor};
    use super::super::{ExitReason, scope};
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

    /// The three-outcome protocol end to end: 0, error tag, and the
    /// cancel sentinel each produce their reason class.
    #[test]
    fn proc_spawn_outcome_maps_the_protocol() {
        unsafe extern "C" fn ok_body(_env: *mut c_void) -> i64 {
            0
        }
        unsafe extern "C" fn err_body(_env: *mut c_void) -> i64 {
            7
        }
        unsafe extern "C" fn cancelled_body(_env: *mut c_void) -> i64 {
            CANCEL_TAG
        }
        for (body, want) in [
            (
                ok_body as unsafe extern "C" fn(*mut c_void) -> i64,
                ProcExit::Normal { value: 0 },
            ),
            (err_body, ProcExit::Error { tag: 7 }),
            (cancelled_body, ProcExit::Cancelled),
        ] {
            // SAFETY: entry points used per their documented contract.
            let id = unsafe {
                __wolf_rt_proc_spawn_outcome(body, std::ptr::null_mut(), b"t".as_ptr(), 1)
            };
            let m = monitor(id).expect("fresh proc");
            let got = ProcExit::decode(m.recv().expect("one delivery"));
            assert_eq!(got, want);
        }
    }

    /// A compiled task returning [`CANCEL_TAG`] records `Cancelled`,
    /// never a scope failure (the pool-mapping half of the protocol).
    #[test]
    fn cancel_tag_is_not_a_scope_failure() {
        unsafe extern "C" fn cancelled_body(env: *mut c_void) -> i64 {
            // SAFETY: env is the AtomicUsize raw from the test.
            unsafe { &*env.cast::<AtomicUsize>() }.fetch_add(1, SeqCst);
            CANCEL_TAG
        }
        let ran = AtomicUsize::new(0);
        let r = scope("cancel-tag", |s| {
            let raw = std::sync::Arc::into_raw(s.inner().clone());
            let env = std::ptr::from_ref(&ran).cast_mut().cast();
            // Drive through the C spawn surface so run_task's mapping
            // is the thing under test.
            unsafe {
                super::super::__wolf_rt_scope_spawn(
                    raw.cast_mut().cast(),
                    cancelled_body,
                    env,
                    b"c".as_ptr(),
                    1,
                );
                // scope_spawn borrows; rebalance the clone above.
                drop(std::sync::Arc::from_raw(raw));
            }
        });
        // Not an error: the scope joins clean.
        assert_eq!(r, Ok(()));
        assert_eq!(ran.load(SeqCst), 1);
    }

    /// The killed poll answers from the calling task's scope chain.
    #[test]
    fn task_killed_polls_the_scope_chain() {
        assert_eq!(__wolf_rt_task_killed(), 0, "root context is never killed");
        let saw = std::sync::Arc::new(AtomicUsize::new(0));
        let saw2 = saw.clone();
        let r = scope("killed-poll", |s| {
            let inner = s.inner().clone();
            s.spawn("probe", move |_| {
                let before = __wolf_rt_task_killed();
                inner.kill();
                let after = __wolf_rt_task_killed();
                saw2.store((before as usize) << 1 | after as usize, SeqCst);
                ExitReason::Cancelled
            });
        });
        // The scope was killed from inside; join sees no failure.
        assert!(r.is_ok() || matches!(r, Err(ExitReason::Cancelled)));
        assert_eq!(saw.load(SeqCst), 0b01, "0 before kill, 1 after");
    }
}
