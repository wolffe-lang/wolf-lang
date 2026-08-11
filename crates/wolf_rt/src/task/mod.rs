//! The task layer (s32) — scoped spawn onto a work-stealing pool of
//! plain OS threads with pooled, lazily-committed native stacks.
//!
//! # The model (D13, report 03 §"Wolf's answer" made code)
//!
//! Tasks are closures run to completion on worker threads. C calls
//! are plain calls, the kernel preempts, and nothing leaks by
//! construction: the ONLY spawn is scoped (D16), and scope exit joins
//! every child (`[conc.task.join]`). There are no green threads, no
//! stackless tier, no async/await, and none of the machinery green
//! threads drag in: the entire Go non-cooperative-preemption apparatus
//! — safe-point loops, signal preemption, conservative frame scanning
//! (`refs/repos/go-proposals/design/24543-non-cooperative-preemption.md`)
//! — is deliberately absent, the road not taken (report 03 §"Needs
//! work" 1). Stacks never move, so FFI pointers into stack memory are
//! legal exactly as in C (contrast Go's register-calling and
//! cgo-pointer rules, `40724-register-calling.md` /
//! `12416-cgo-pointers.md` — the whole reason D13 rejected green
//! threads).
//!
//! Blocking discipline: runtime-owned blocking primitives route
//! through [`pool::blocking`], which drives blocking compensation
//! (Target 6). **Raw FFI that blocks is invisible to compensation** —
//! a C call that sleeps holds its worker and the pool cannot tell;
//! the stdlib rule (s35/s38) is that all std blocking routes through
//! runtime-owned primitives, and `[conc.cancel.c]` already promises C
//! frames are never unwound or interrupted.
//!
//! Every scheduling decision routes through the hooks.rs seam
//! (Target 7; the s36 Phase A doc gates this sprint's merge — see
//! hooks.rs for exactly how much of X12 lands here), and all runtime
//! nondeterminism draws from the seam's seeded PRNG.
//!
//! # The symbol contract (codegen-facing, frozen like s28's)
//!
//! Compiled wolf code drives the s32 task layer through five entry
//! points (s33 adds the channel/select family in chan.rs —
//! `__wolf_rt_chan_new/clone/free/send/send_region/recv/close`,
//! `__wolf_rt_select` — and the `when` family in when.rs —
//! `__wolf_rt_sync_new/free/payload`,
//! `__wolf_rt_when_acquire/release`; s34 adds the proc family in
//! proc.rs — `__wolf_rt_proc_spawn/self/monitor/link/kill/cancel`,
//! `__wolf_rt_region_adopt` — frozen the same way, ahead of
//! consumption):
//!
//! - [`__wolf_rt_scope_new`] — open a scope; the returned handle IS
//!   the language's scope value (D16: scope handles are values; the
//!   checker keeps them inside their region, the runtime object is
//!   the capability).
//! - [`__wolf_rt_scope_spawn`] — schedule `entry(env)` under the
//!   scope. `env` crosses by `move` (D14): ownership transfers to the
//!   task; the region checker proved the move statically.
//! - [`__wolf_rt_scope_join_free`] — scope exit: join all children,
//!   free the scope, return 0 or the first failure's error tag (the
//!   re-raise of `[conc.task.fail]`; codegen branches into the `?`
//!   error path on nonzero).
//! - [`__wolf_rt_task_checkpoint`] — the explicit `checkpoint()`
//!   cancel poll (`[conc.cancel.points]`).
//! - [`__wolf_rt_region_transfer`] — a region value crossing a spawn
//!   boundary (`sync.transfer`'s runtime half; v0 identity, the seam
//!   where the checked profile retags and s36 records).
//!
//! WIR does not lower `scope`/`spawn`/`channel`/`select`/`when` yet
//! (typecheck refuses all concurrency constructs with the
//! "concurrency typing (c05)" NotYet, closures included; the conc
//! corpus tier sits at `phase: resolve`), so nothing emits calls to
//! any of these symbols yet — the surfaces are frozen here first,
//! exactly as s28 froze the trap contract before s31 consumed it
//! fully.
//!
//! A binary that never spawns pays for none of this: pool, registry,
//! signal handler, and stack store are all lazily initialized on
//! first use, and the driver's `--gc-sections` link drops the symbols
//! outright (Target 1's CI test holds both halves).

mod chan;
mod deque;
mod hooks;
mod pool;
mod proc;
mod scope;
#[cfg(unix)]
mod stack;
mod when;

pub use chan::{
    __wolf_rt_chan_clone, __wolf_rt_chan_close, __wolf_rt_chan_free, __wolf_rt_chan_new,
    __wolf_rt_chan_recv, __wolf_rt_chan_send, __wolf_rt_chan_send_region, __wolf_rt_select, Arm,
    Chan, ChanErr, Selected, WolfSelectArm, select, select_verdict, select_with,
};
pub use hooks::{ChanPhase, SchedEvent, SchedRng};
pub use pool::{Body, SendPtr, TaskCtx, blocking, counters, current_scope, initialized};
pub use proc::{
    __wolf_rt_proc_cancel, __wolf_rt_proc_kill, __wolf_rt_proc_link, __wolf_rt_proc_monitor,
    __wolf_rt_proc_self, __wolf_rt_proc_spawn, __wolf_rt_region_adopt, ProcErr, ProcExit,
    ProcOutcome, ROOT_DEATH_EXIT, ROOT_DOMAIN, RestartPolicy, Supervised, TRAP_ERROR_TAG, cancel,
    current_proc, kill, ledger_live, link, monitor, proc_ledger, set_trap_exit, spawn_proc,
    supervise, with_handler,
};
pub use scope::{ExitReason, ScopeInner, TaskState, dump};
pub use when::{
    __wolf_rt_sync_free, __wolf_rt_sync_new, __wolf_rt_sync_payload, __wolf_rt_when_acquire,
    __wolf_rt_when_release, SyncCell, WhenErr, when, when_acquire, when_release,
};

use std::sync::Arc;

/// Open a scope, run `f` with its handle, join at exit
/// (`[conc.task.join]`), re-raise the first failure
/// (`[conc.task.fail]`). The Rust scaffolding twin of the compiled
/// `scope { }` lowering; tests and later crate-internal users
/// (s33–s35) drive the runtime through it.
pub fn scope<R>(name: &str, f: impl FnOnce(&ScopeHandle) -> R) -> Result<R, ExitReason> {
    let inner = ScopeInner::new(name, current_scope().as_ref());
    let handle = ScopeHandle {
        inner: inner.clone(),
    };
    let r = pool::with_scope(&inner, || f(&handle));
    match inner.join() {
        None => Ok(r),
        Some(reason) => Err(reason),
    }
}

/// A scope handle — the capability value (D16). `Clone` hands the
/// capability to callees, never lifetime beyond the region (the
/// checker's law on the wolf side; on the Rust side the `Arc` keeps
/// the object alive but `scope()` still joins before returning).
#[derive(Clone)]
pub struct ScopeHandle {
    inner: Arc<ScopeInner>,
}

impl ScopeHandle {
    /// Spawn a named task into this scope — the only spawn (D16).
    pub fn spawn(&self, name: &str, body: impl FnOnce(&TaskCtx) -> ExitReason + Send + 'static) {
        pool::spawn_task(&self.inner, name, Body::Rust(Box::new(body)));
    }

    /// The runtime scope object (s33+ primitives hang off it).
    pub fn inner(&self) -> &Arc<ScopeInner> {
        &self.inner
    }
}

/// A runtime-owned gate: tasks block in [`Gate::wait`] until
/// [`Gate::open`]. Predates the s33 channel layer as the v0 blocking
/// point; kept as the dependency-free primitive the pool/cancellation
/// tests block through.
pub struct Gate {
    state: std::sync::Mutex<bool>,
    cv: std::sync::Condvar,
}

impl Default for Gate {
    fn default() -> Self {
        Gate::new()
    }
}

impl Gate {
    pub fn new() -> Gate {
        Gate {
            state: std::sync::Mutex::new(false),
            cv: std::sync::Condvar::new(),
        }
    }

    /// Open the gate; all waiters return `true`.
    pub fn open(&self) {
        *self.state.lock().unwrap() = true;
        self.cv.notify_all();
    }

    /// Block (with compensation) until open — or until the task's
    /// scope is cancelled, returning `false` (`[conc.cancel.points]`:
    /// cancellation surfaces as a value at the blocking point).
    pub fn wait(&self, ctx: &TaskCtx) -> bool {
        let opened = blocking(|| {
            let mut opened = self.state.lock().unwrap();
            loop {
                if *opened {
                    return true;
                }
                if ctx.is_cancelled() {
                    return false;
                }
                let (g, _) = self
                    .cv
                    .wait_timeout(opened, std::time::Duration::from_millis(5))
                    .unwrap();
                opened = g;
            }
        });
        if !opened {
            // Kill teardown (s34): a killed scope terminates the task
            // here; a merely-cancelled one sees `false` and runs its
            // defers by ordinary returns.
            pool::kill_teardown_check(&ctx.scope);
        }
        opened
    }
}

// ---- the C entry surface -------------------------------------------------

use core::ffi::c_void;

pub(crate) fn name_from_raw(ptr: *const u8, len: i64) -> String {
    if ptr.is_null() || len <= 0 {
        return String::new();
    }
    // SAFETY: caller contract — `ptr` addresses `len` readable bytes
    // (codegen passes rodata spawn-site names).
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len as usize) };
    String::from_utf8_lossy(bytes).into_owned()
}

/// Open a scope (`scope { }` entry). `name`/`len` may be null/0 (an
/// unnamed scope). Returns the scope capability handle.
///
/// # Safety
///
/// `name` must address `len` readable bytes when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_scope_new(name: *const u8, len: i64) -> *mut c_void {
    let name = name_from_raw(name, len);
    let inner = ScopeInner::new(&name, current_scope().as_ref());
    Arc::into_raw(inner).cast_mut().cast()
}

/// Spawn `entry(env)` under `scope`. `env` moves: the task owns it
/// from this call on (D14 `move`; the checker proved the transfer).
/// `entry` returns 0 for normal completion or an error tag
/// (`[abi.err]` first-register discipline) to fail the scope.
///
/// # Safety
///
/// `scope` must be a live handle from [`__wolf_rt_scope_new`];
/// `entry` must be callable with `env`; `name` as in scope_new.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_scope_spawn(
    scope: *mut c_void,
    entry: unsafe extern "C" fn(*mut c_void) -> i64,
    env: *mut c_void,
    name: *const u8,
    name_len: i64,
) {
    // SAFETY: borrow the caller's handle without consuming it.
    let inner = unsafe { std::mem::ManuallyDrop::new(Arc::from_raw(scope.cast::<ScopeInner>())) };
    let name = name_from_raw(name, name_len);
    let name = if name.is_empty() {
        "task".to_string()
    } else {
        name
    };
    pool::spawn_task(
        &inner,
        &name,
        Body::C {
            entry,
            env: SendPtr(env),
        },
    );
}

/// Scope exit: join all children (`[conc.task.join]`), free the
/// handle, return 0 on success or the first failure's error tag —
/// the `[conc.task.fail]` re-raise (a Rust-side panic reason reports
/// tag -1; compiled tasks trap the process before reaching here).
///
/// # Safety
///
/// `scope` must be a live handle from [`__wolf_rt_scope_new`]; it is
/// dead after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_scope_join_free(scope: *mut c_void) -> i64 {
    // SAFETY: consumes the caller's handle.
    let inner = unsafe { Arc::from_raw(scope.cast::<ScopeInner>()) };
    match inner.join() {
        None => 0,
        Some(ExitReason::Error { tag }) => tag,
        Some(_) => -1,
    }
}

/// The explicit `checkpoint()` cancel poll (`[conc.cancel.points]`):
/// 1 when the calling task's scope has been cancelled, else 0.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_task_checkpoint() -> i8 {
    match current_scope() {
        Some(s) => {
            let c = s.is_cancelled();
            hooks::sched_point(SchedEvent::CancelCheck {
                scope: s.id(),
                cancelled: c,
            });
            i8::from(c)
        }
        None => 0,
    }
}

/// A region value crossing a spawn boundary (move semantics across
/// threads — `sync.transfer`'s runtime half). v0: identity; the
/// happens-before edge is `[conc.mm.hb.move]`, delivered by the
/// scheduler's own synchronization. This is the seam where the
/// checked profile retags the region's granules (quarantine, s54)
/// and s36 records the ownership transfer.
///
/// # Safety
///
/// `handle` must be a live region handle; the caller may not touch
/// it after the transfer (the checker's move rule).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_transfer(handle: *mut c_void) -> *mut c_void {
    hooks::sched_point(SchedEvent::RegionTransfer);
    // s34 ledger half: ownership leaves the donor proc at the move;
    // the receiver attaches via `__wolf_rt_region_adopt` (in-flight
    // regions belong to the channel — `[conc.chan.move]`).
    proc::region_transferred(handle as usize);
    handle
}

/// Write the structured task dump (Target 8) to stderr: live scopes,
/// their live tasks, names and states, grouped by scope tree. s54's
/// debugger pretty-printers call this.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_dump_tasks() {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(dump().as_bytes());
    let _ = err.flush();
}

// ---- acceptance tests ----------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

    #[test]
    fn scope_joins_all_children() {
        let hits = Arc::new(AtomicUsize::new(0));
        let r = scope("joins", |s| {
            for i in 0..64 {
                let hits = hits.clone();
                s.spawn(&format!("t{i}"), move |_| {
                    hits.fetch_add(1, SeqCst);
                    ExitReason::Normal
                });
            }
        });
        assert!(r.is_ok());
        // The join happened: every child ran before scope() returned.
        assert_eq!(hits.load(SeqCst), 64);
    }

    #[test]
    fn nested_scopes_join_inside_out() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let r = scope("outer", |s| {
            let log2 = log.clone();
            s.spawn("child", move |_| {
                let inner_log = log2.clone();
                let r = scope("inner", |s2| {
                    let l = inner_log.clone();
                    s2.spawn("grandchild", move |_| {
                        l.lock().unwrap().push("grandchild");
                        ExitReason::Normal
                    });
                });
                assert!(r.is_ok());
                log2.lock().unwrap().push("child-after-inner");
                ExitReason::Normal
            });
        });
        assert!(r.is_ok());
        assert_eq!(
            *log.lock().unwrap(),
            vec!["grandchild", "child-after-inner"]
        );
    }

    /// The sibling-cancel acceptance (Target 5 / `[conc.task.fail]` /
    /// `[conc.task.fail.owner]`'s runtime half): one child errors,
    /// the sibling blocked at a runtime-owned point observes
    /// cancellation, the parent join returns the error. The
    /// exit-reason shape is snapshotted below.
    #[test]
    fn sibling_cancel_and_reraise() {
        let sibling_saw_cancel = Arc::new(AtomicUsize::new(0));
        let saw = sibling_saw_cancel.clone();
        let r = scope("race", |s| {
            s.spawn("blocked-sibling", move |ctx| {
                // Blocks "forever" (no channel yet — the v0 blocking
                // point); completes only via cancellation.
                ctx.wait_cancelled();
                saw.fetch_add(1, SeqCst);
                ExitReason::Cancelled
            });
            s.spawn("failer", |_| ExitReason::Error { tag: 7 });
        });
        assert_eq!(r, Err(ExitReason::Error { tag: 7 }));
        assert_eq!(sibling_saw_cancel.load(SeqCst), 1);
    }

    /// The exit-reason shape, pinned (acceptance: "snapshot the
    /// exit-reason shape"). Changing this is a reviewed change —
    /// `Killed` is s34's reviewed extension (the kill-teardown
    /// outcome, `[conc.proc.kill]`).
    #[test]
    fn exit_reason_shape_snapshot() {
        let shapes = [
            format!("{:?}", ExitReason::Normal),
            format!("{:?}", ExitReason::Error { tag: 7 }),
            format!("{:?}", ExitReason::Panicked),
            format!("{:?}", ExitReason::Cancelled),
            format!("{:?}", ExitReason::Killed),
        ];
        assert_eq!(
            shapes,
            [
                "Normal",
                "Error { tag: 7 }",
                "Panicked",
                "Cancelled",
                "Killed",
            ]
        );
    }

    /// Failure context: first failure in schedule order wins, the
    /// rest attach as context (`[conc.task.fail]`).
    #[test]
    fn first_failure_wins_rest_attach() {
        let gate = Arc::new(Gate::new());
        let g2 = gate.clone();
        let r = scope("multi-fail", |s| {
            let g = gate.clone();
            s.spawn("second", move |ctx| {
                // Held at the gate until cancellation (wait returns
                // false), THEN fails — strictly after "first".
                if !g.wait(ctx) {
                    return ExitReason::Error { tag: 2 };
                }
                ExitReason::Normal
            });
            s.spawn("first", |_| ExitReason::Error { tag: 1 });
        });
        drop(g2);
        assert_eq!(r, Err(ExitReason::Error { tag: 1 }));
    }

    /// A panicking Rust task becomes an exit REASON (D30) — no unwind
    /// escapes the scheduler, the scope fails, siblings cancel.
    #[test]
    fn panic_becomes_exit_reason() {
        let r = scope("panicky", |s| {
            s.spawn("boom", |_| panic!("caught at the task boundary"));
        });
        assert_eq!(r, Err(ExitReason::Panicked));
    }

    /// Blocking compensation (Target 6): block `target` tasks in a
    /// runtime-owned primitive, then require compute throughput —
    /// replacement workers must appear or the compute tasks starve
    /// (the test deadlocks and times out without compensation).
    #[test]
    fn blocking_compensation_recovers_throughput() {
        let (target, ..) = {
            // Force pool init so counters are live.
            let _ = scope("warm", |s| s.spawn("t", |_| ExitReason::Normal));
            counters()
        };
        let gate = Arc::new(Gate::new());
        let done = Arc::new(AtomicUsize::new(0));
        let r = scope("compensate", |s| {
            for i in 0..target {
                let g = gate.clone();
                s.spawn(&format!("blocker{i}"), move |ctx| {
                    g.wait(ctx);
                    ExitReason::Normal
                });
            }
            for i in 0..(target * 2) {
                let done = done.clone();
                s.spawn(&format!("compute{i}"), move |_| {
                    done.fetch_add(1, SeqCst);
                    ExitReason::Normal
                });
            }
            // Wait (bounded) for the compute tasks while all core
            // workers sit blocked at the gate.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            while done.load(SeqCst) < target * 2 {
                assert!(
                    std::time::Instant::now() < deadline,
                    "compute tasks starved: compensation failed \
                     (counters: {:?})",
                    counters()
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            gate.open();
        });
        assert!(r.is_ok());
        assert_eq!(done.load(SeqCst), target * 2);
    }

    /// 10k queued tasks are descriptors, not stacks (Target 3's
    /// laziness, testable half): live worker threads stay within the
    /// pool cap while 10k tasks flow through.
    #[test]
    fn ten_k_tasks_bounded_threads() {
        let n = Arc::new(AtomicUsize::new(0));
        let r = scope("10k", |s| {
            for i in 0..10_000 {
                let n = n.clone();
                s.spawn(&format!("t{i}"), move |_| {
                    n.fetch_add(1, SeqCst);
                    ExitReason::Normal
                });
            }
        });
        assert!(r.is_ok());
        assert_eq!(n.load(SeqCst), 10_000);
        let (target, running, _) = counters();
        assert!(
            running <= (target * 8).max(target + 4),
            "worker threads exceeded the cap: {running}"
        );
    }

    /// The dump exists and groups tasks under their scope
    /// (`[conc.task.name]`: existence is contract).
    #[test]
    fn dump_names_scopes_and_tasks() {
        let gate = Arc::new(Gate::new());
        let seen = Arc::new(std::sync::Mutex::new(String::new()));
        let g2 = gate.clone();
        let seen2 = seen.clone();
        let r = scope("dumped", |s| {
            let g = g2.clone();
            s.spawn("held", move |ctx| {
                g.wait(ctx);
                ExitReason::Normal
            });
            // Give the task a moment to be visible, then dump.
            std::thread::sleep(std::time::Duration::from_millis(20));
            *seen2.lock().unwrap() = dump();
            gate.open();
        });
        assert!(r.is_ok());
        let text = seen.lock().unwrap().clone();
        assert!(text.contains("'dumped'"), "dump: {text}");
        assert!(text.contains("'held'"), "dump: {text}");
    }

    /// The seam sees the decisions (Target 7): spawn, join, and the
    /// steal/park/unpark family flow through sched_point in test
    /// builds.
    #[test]
    fn sched_seam_observes_events() {
        use std::sync::Mutex;
        let _serial = hooks::test_hook::SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
        hooks::test_hook::set_test_hook(Some(Box::new(|ev| {
            let tag = match ev {
                SchedEvent::Spawn { .. } => "spawn",
                SchedEvent::Park { .. } => "park",
                SchedEvent::Unpark { .. } => "unpark",
                SchedEvent::Steal { .. } => "steal",
                SchedEvent::Join { .. } => "join",
                SchedEvent::CancelCheck { .. } => "cancel-check",
                SchedEvent::RegionTransfer => "region-transfer",
                SchedEvent::ChanSend { .. } => "chan-send",
                SchedEvent::ChanRecv { .. } => "chan-recv",
                SchedEvent::ChanClose { .. } => "chan-close",
                SchedEvent::SelectArm { .. } => "select-arm",
                SchedEvent::Acquire { .. } => "acquire",
                SchedEvent::TimerFire => "timer",
                SchedEvent::ProcSpawn { .. } => "proc-spawn",
                SchedEvent::ProcKill { .. } => "proc-kill",
                SchedEvent::ProcExit { .. } => "proc-exit",
            };
            SEEN.lock().unwrap().push(tag);
        })));
        let r = scope("seamed", |s| {
            for i in 0..8 {
                s.spawn(&format!("t{i}"), |_| ExitReason::Normal);
            }
        });
        hooks::test_hook::set_test_hook(None);
        assert!(r.is_ok());
        let seen = SEEN.lock().unwrap();
        assert!(seen.contains(&"spawn"), "no spawn events");
        assert!(seen.contains(&"join"), "no join events");
    }

    /// The C symbol surface end to end: scope_new → spawn → join_free
    /// carries the error tag; the env moves.
    #[test]
    fn c_surface_round_trip() {
        unsafe extern "C" fn ok_body(env: *mut c_void) -> i64 {
            // SAFETY: env is the Box<AtomicUsize> raw from the test.
            let n = unsafe { Box::from_raw(env.cast::<AtomicUsize>()) };
            n.fetch_add(1, SeqCst);
            Box::leak(n); // observed below via the Arc twin
            0
        }
        unsafe extern "C" fn err_body(_env: *mut c_void) -> i64 {
            9
        }
        // SAFETY: entry points used per their documented contract.
        unsafe {
            let name = b"c-scope";
            let sc = __wolf_rt_scope_new(name.as_ptr(), name.len() as i64);
            let counter = Box::into_raw(Box::new(AtomicUsize::new(0)));
            __wolf_rt_scope_spawn(sc, ok_body, counter.cast(), b"ok".as_ptr(), 2);
            __wolf_rt_scope_spawn(sc, err_body, std::ptr::null_mut(), b"err".as_ptr(), 3);
            let tag = __wolf_rt_scope_join_free(sc);
            assert_eq!(tag, 9);
            let counter = Box::from_raw(counter);
            assert_eq!(counter.load(SeqCst), 1);
        }
    }

    /// Region transfer is identity in v0 and fires the seam event.
    #[test]
    fn region_transfer_identity() {
        let h = crate::native::__wolf_rt_region_new();
        // SAFETY: live handle, transferred then freed once.
        unsafe {
            let h2 = __wolf_rt_region_transfer(h);
            assert_eq!(h, h2);
            crate::native::__wolf_rt_region_free(h2);
        }
    }
}
