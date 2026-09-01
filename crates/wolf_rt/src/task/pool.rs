//! The work-stealing pool (s32 Targets 2/6) — plain OS threads.
//!
//! Per-worker Chase–Lev deques plus a LIFO slot for spawn locality, a
//! global injector queue, parked-worker wakeup via `std::thread`
//! park/unpark (futex-backed on linux — the contract's "futex or
//! equivalent"). Default size = logical cores; grows under **blocking
//! compensation** (Loom's strategy at thread level): runtime-owned
//! blocking primitives wrap themselves in [`blocking`], which marks
//! the worker blocked, and when unblocked workers drop below target
//! parallelism the pool spins up a replacement thread (up to a cap);
//! replacements retire lazily when idle. Raw FFI that blocks is
//! invisible to this machinery — documented loudly in mod.rs; the
//! stdlib rule (s35/s38) is that all std blocking routes through
//! runtime-owned primitives.
//!
//! There is deliberately **no time-slicing machinery of our own**:
//! tasks run on real threads and the kernel preempts (see mod.rs for
//! the Go-preemption road not taken). A task holds its worker until
//! it returns; blocked tasks hold their thread (`[conc.task.order]`)
//! and compensation keeps the pool's parallelism unobservable.
//!
//! Worker threads run on the pooled, lazily-committed stacks of
//! stack.rs (unix; elsewhere `std::thread` with the same reserve —
//! the recorded windows delta, where the s36 chaos hooks must later
//! be able to inject commit failure in our own guard walker).
//!
//! The pool, like every static here, is **lazily initialized on first
//! spawn** (Target 1): a wolf binary that never spawns never creates
//! a thread, installs a handler, or touches a queue.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::Thread;

use super::deque::{Deque, Steal};
use super::hooks::{SchedEvent, SchedRng, sched_point};
use super::scope::{ExitReason, ScopeInner, TaskState};

/// A raw pointer that crosses the spawn boundary by `move` semantics
/// — ownership of the environment transfers to the task (the region
/// checker proved the move on the wolf side).
pub struct SendPtr(pub *mut core::ffi::c_void);
// SAFETY: ownership transfer is the contract of spawn (D14 `move`).
unsafe impl Send for SendPtr {}

/// A task body: a Rust closure (test/scaffolding surface) or a
/// compiled-wolf entry point with its moved environment.
pub enum Body {
    /// Rust-side body; returns its exit reason.
    Rust(Box<dyn FnOnce(&TaskCtx) -> ExitReason + Send + 'static>),
    /// Compiled entry: `0` = normal, nonzero = error tag (`[abi.err]`
    /// discipline: the tag rides the first integer register).
    C {
        /// The spawned function, post-lowering.
        entry: unsafe extern "C" fn(*mut core::ffi::c_void) -> i64,
        /// The moved closure environment.
        env: SendPtr,
    },
}

/// One spawned task.
pub struct Task {
    pub id: u64,
    pub name: Arc<str>,
    pub scope: Arc<ScopeInner>,
    pub body: Body,
}

/// What a running Rust task sees of itself.
pub struct TaskCtx {
    pub(crate) scope: Arc<ScopeInner>,
}

impl TaskCtx {
    /// Cooperative cancel-check (a `checkpoint()` safe point).
    pub fn is_cancelled(&self) -> bool {
        let c = self.scope.is_cancelled();
        sched_point(SchedEvent::CancelCheck {
            scope: self.scope.id(),
            cancelled: c,
        });
        c
    }

    /// Block at a runtime-owned point until the scope is cancelled.
    pub fn wait_cancelled(&self) {
        self.scope.wait_cancelled();
    }
}

struct WorkerSlot {
    occupied: AtomicBool,
    thread: Mutex<Option<Thread>>,
    deque: Deque<Task>,
    lifo: AtomicPtr<Task>,
}

struct Pool {
    target: usize,
    cap: usize,
    slots: Vec<WorkerSlot>,
    injector: Mutex<VecDeque<Box<Task>>>,
    /// Parked worker ids, most-recently-parked first.
    idle: Mutex<Vec<usize>>,
    /// Racy mirror of `idle.len()` — the spawn path's lock-free
    /// nobody-to-wake fast path.
    idle_count: AtomicUsize,
    /// Live workers not blocked in a runtime-owned primitive.
    unblocked: AtomicUsize,
    /// Live worker threads.
    running: AtomicUsize,
}

static POOL: OnceLock<Pool> = OnceLock::new();

thread_local! {
    static WORKER_ID: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
    /// Innermost scope on this thread: the running task's scope,
    /// plus nested `scope { }` blocks pushed by the owner.
    static SCOPE_STACK: RefCell<Vec<Arc<ScopeInner>>> =
        const { RefCell::new(Vec::new()) };
    /// True while a `Body::Rust` task body runs on this thread — the
    /// only frames kill teardown may unwind (s34). C-ABI frames are
    /// never unwound (`[conc.cancel.c]`'s discipline extended to
    /// kill: compiled tasks get kill delivery when codegen lowers the
    /// no-defer teardown branch — the honest s34 refusal).
    static IN_RUST_TASK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The running task's (id, spawn scope) — the trap-containment
    /// path's handle for retiring a task that will never return
    /// through [`run_task`] (D68, s132). Set around the body; taken
    /// by [`finish_current_task`] exactly once.
    static CURRENT_TASK: RefCell<Option<(u64, Arc<ScopeInner>)>> =
        const { RefCell::new(None) };
}

/// The kill-teardown unwind payload (s34, `[conc.proc.kill]` step 1):
/// blocking points in a killed scope terminate their task by raising
/// this token instead of returning a value, so NO further user code —
/// and therefore no `defer` — runs on the killed path (contrast
/// cancellation, which surfaces as a value and lets ordinary returns
/// run the defers: `[conc.cancel.defer]` vs `[conc.proc.kill]`, D14's
/// signature distinction). [`run_task`] catches it at the task
/// boundary and records [`ExitReason::Killed`].
pub(crate) struct KilledToken;

/// Raise kill teardown if `scope` sits in a killed proc tree and the
/// current frame stack is unwindable (a Rust task body). Callers must
/// hold NO locks: the token skips their cleanup (that is its job for
/// user code, but runtime state must already be consistent — waiter
/// cells committed, prefixes released).
pub(crate) fn kill_teardown_check(scope: &ScopeInner) {
    if scope.is_killed() && IN_RUST_TASK.with(std::cell::Cell::get) {
        std::panic::panic_any(KilledToken);
    }
}

/// Run `f` with kill-teardown unwinding suppressed: the frames below
/// are C ABI (never unwound — `[conc.cancel.c]` extended to kill), so
/// blocking points inside surface the cancelled status VALUE instead
/// of the token. The compiled-body teardown branch is the codegen
/// sprint's; this keeps the C proc entry sound until then.
pub(crate) fn suppress_kill_unwind<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            IN_RUST_TASK.with(|c| c.set(self.0));
        }
    }
    let prev = IN_RUST_TASK.with(|c| c.replace(false));
    let _g = Restore(prev);
    f()
}

/// The innermost scope on the calling thread (spawn parenting, C
/// checkpoint surface).
pub fn current_scope() -> Option<Arc<ScopeInner>> {
    SCOPE_STACK.with(|s| s.borrow().last().cloned())
}

/// Retire the calling thread's running task with `reason` — the
/// trap-containment path's half of [`run_task`]'s completion (D68,
/// s132): the task's frames never resume, so its `child_done` must
/// come from here for the scope join to quiesce. Idempotent by
/// construction (the TLS slot is taken); a thread with no running
/// task (the harness root) is a no-op.
pub(crate) fn finish_current_task(reason: ExitReason) {
    if let Some((id, scope)) = CURRENT_TASK.with(|c| c.borrow_mut().take()) {
        scope.child_done(id, reason);
    }
}

/// Park the calling worker forever after a contained trap (D68,
/// s132). The frames below this call are compiled wolf frames plus
/// the trap path's own — never unwound (`[abi.native.nounwind]`),
/// never resumed. The worker leaves the unblocked count exactly the
/// way [`blocking`] does (LIFO slot flushed to stealable ground, the
/// pool compensated), minus `blocking`'s handler debug-assert: a trap
/// inside a proc message handler is precisely a case this path must
/// contain, not assert on. The thread and its stack's high water are
/// the measured containment cost — the same class as a task parked on
/// a channel nobody sends to, and the pool already compensates for
/// those.
pub(crate) fn park_after_contained_trap() -> ! {
    if let Some(w) = WORKER_ID.with(std::cell::Cell::get) {
        let p = pool();
        let slot = &p.slots[w];
        let held = slot.lifo.swap(std::ptr::null_mut(), SeqCst);
        if !held.is_null() {
            match slot.deque.push(held) {
                Ok(()) => {}
                Err(back) => {
                    // SAFETY: pointer from Box::into_raw in spawn_task.
                    let boxed = unsafe { Box::from_raw(back) };
                    p.injector.lock().unwrap().push_back(boxed);
                }
            }
            wake_one(p);
        }
        p.unblocked.fetch_sub(1, SeqCst);
        compensate(p);
    }
    // det (s36): the baton frees for good — this thread never takes
    // another grant (no-op outside the det domain).
    super::hooks::det_block_enter();
    loop {
        std::thread::park();
    }
}

/// Run `f` with `scope` as the innermost scope on this thread.
pub fn with_scope<R>(scope: &Arc<ScopeInner>, f: impl FnOnce() -> R) -> R {
    SCOPE_STACK.with(|s| s.borrow_mut().push(scope.clone()));
    let r = f();
    SCOPE_STACK.with(|s| {
        s.borrow_mut().pop();
    });
    r
}

fn pool() -> &'static Pool {
    POOL.get_or_init(|| {
        #[cfg(unix)]
        super::stack::install_overflow_handler();
        let target = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .max(2);
        let cap = (target * 8).max(target + 4);
        let slots = (0..cap)
            .map(|_| WorkerSlot {
                occupied: AtomicBool::new(false),
                thread: Mutex::new(None),
                deque: Deque::new(),
                lifo: AtomicPtr::new(std::ptr::null_mut()),
            })
            .collect();
        Pool {
            target,
            cap,
            slots,
            injector: Mutex::new(VecDeque::new()),
            idle: Mutex::new(Vec::new()),
            idle_count: AtomicUsize::new(0),
            unblocked: AtomicUsize::new(0),
            running: AtomicUsize::new(0),
        }
    })
}

/// Spawn a task under `scope` (the ONLY spawn — D16; every caller
/// holds a scope). Sub-microsecond path: id + queue push + wake.
pub fn spawn_task(scope: &Arc<ScopeInner>, name: &str, body: Body) {
    let p = pool();
    ensure_workers(p);
    let name: Arc<str> = name.into();
    let id = scope.add_child(&name);
    sched_point(SchedEvent::Spawn {
        task: id,
        scope: scope.id(),
    });
    // det (s36): an in-flight tracked task holds the fast-grant path
    // open until a worker enters it.
    super::hooks::det_note_spawn(scope);
    let task = Box::new(Task {
        id,
        name,
        scope: scope.clone(),
        body,
    });
    match WORKER_ID.with(std::cell::Cell::get) {
        Some(w) => {
            // LIFO slot for locality; the displaced task goes to the
            // deque (or spills to the injector when full).
            let slot = &p.slots[w];
            let old = slot.lifo.swap(Box::into_raw(task), SeqCst);
            if !old.is_null()
                && let Err(back) = slot.deque.push(old)
            {
                // SAFETY: `back` came out of Box::into_raw above.
                let boxed = unsafe { Box::from_raw(back) };
                p.injector.lock().unwrap().push_back(boxed);
            }
        }
        None => p.injector.lock().unwrap().push_back(task),
    }
    wake_one(p);
}

/// First use: bring up the core workers.
fn ensure_workers(p: &'static Pool) {
    if p.running.load(SeqCst) >= p.target {
        return;
    }
    for slot in 0..p.target {
        if !p.slots[slot].occupied.swap(true, SeqCst) {
            p.running.fetch_add(1, SeqCst);
            p.unblocked.fetch_add(1, SeqCst);
            start_worker(p, slot, false);
        }
    }
}

fn wake_one(p: &Pool) {
    if p.idle_count.load(SeqCst) == 0 {
        return;
    }
    let woken = {
        let mut idle = p.idle.lock().unwrap();
        let w = idle.pop();
        p.idle_count.store(idle.len(), SeqCst);
        w
    };
    if let Some(w) = woken {
        sched_point(SchedEvent::Unpark { worker: w });
        if let Some(t) = p.slots[w].thread.lock().unwrap().as_ref() {
            t.unpark();
        }
    }
}

/// Wrap a runtime-owned blocking wait: mark the worker blocked so the
/// pool can compensate (Target 6). Non-worker threads pass through.
///
/// Handler atomicity (s34, `[conc.chan.mailbox]`): proc message
/// handlers are atomic and NON-BLOCKING — a handler that must block
/// spawns the work onto its proc's scope instead. The checker rejects
/// what it can see statically; this debug-assert is the defense in
/// depth at every runtime-owned blocking point (release builds
/// compile it out).
pub fn blocking<R>(f: impl FnOnce() -> R) -> R {
    debug_assert!(
        !super::proc::in_handler(),
        "blocking primitive called inside a proc message handler \
         ([conc.chan.mailbox]: handlers are atomic and non-blocking — \
         spawn the work onto the proc's scope and complete via a message)"
    );
    let Some(w) = WORKER_ID.with(std::cell::Cell::get) else {
        // det (s36): the harness root context blocks off-pool.
        super::hooks::det_block_enter();
        let r = f();
        super::hooks::det_block_exit();
        return r;
    };
    let p = pool();
    // The LIFO slot is owner-private: flush it to stealable ground
    // before blocking, or a child spawned just before a nested join
    // would deadlock its own parent.
    let slot = &p.slots[w];
    let held = slot.lifo.swap(std::ptr::null_mut(), SeqCst);
    if !held.is_null() {
        match slot.deque.push(held) {
            Ok(()) => {}
            Err(back) => {
                // SAFETY: pointer from Box::into_raw in spawn_task.
                let boxed = unsafe { Box::from_raw(back) };
                p.injector.lock().unwrap().push_back(boxed);
            }
        }
        wake_one(p);
    }
    p.unblocked.fetch_sub(1, SeqCst);
    compensate(p);
    // det (s36): the baton frees while the primitive parks; the
    // return edge re-requests a grant before user-observable state
    // moves again. Both no-op outside the det domain. The worker
    // stays counted blocked through the grant wait, so compensation
    // keeps covering it.
    super::hooks::det_block_enter();
    let r = f();
    super::hooks::det_block_exit();
    p.unblocked.fetch_add(1, SeqCst);
    r
}

/// Live unblocked workers dipped below target: add a replacement
/// thread, up to the cap.
fn compensate(p: &'static Pool) {
    if p.unblocked.load(SeqCst) >= p.target {
        return;
    }
    if p.running.load(SeqCst) >= p.cap {
        return;
    }
    for slot in p.target..p.cap {
        if !p.slots[slot].occupied.swap(true, SeqCst) {
            p.running.fetch_add(1, SeqCst);
            p.unblocked.fetch_add(1, SeqCst);
            start_worker(p, slot, true);
            return;
        }
    }
}

// ---- the worker ----------------------------------------------------------

fn worker_main(p: &'static Pool, slot: usize, extra: bool) {
    WORKER_ID.with(|c| c.set(Some(slot)));
    *p.slots[slot].thread.lock().unwrap() = Some(std::thread::current());
    let mut rng = SchedRng::for_worker(slot);
    loop {
        if let Some(task) = find_task(p, slot, &mut rng) {
            run_task(task);
            continue;
        }
        // Out of work: advertise idle, then re-check (no lost wakeup:
        // enqueue happens-before wake_one, which pops us and unparks).
        {
            let mut idle = p.idle.lock().unwrap();
            if !idle.contains(&slot) {
                idle.push(slot);
            }
            p.idle_count.store(idle.len(), SeqCst);
        }
        if have_visible_work(p) {
            let mut idle = p.idle.lock().unwrap();
            if let Some(pos) = idle.iter().position(|&w| w == slot) {
                idle.remove(pos);
            }
            p.idle_count.store(idle.len(), SeqCst);
            continue;
        }
        sched_point(SchedEvent::Park { worker: slot });
        #[cfg(unix)]
        trim_own_stack(p, slot);
        if extra {
            std::thread::park_timeout(std::time::Duration::from_millis(50));
            let mut idle = p.idle.lock().unwrap();
            let still_idle = idle.iter().position(|&w| w == slot);
            if let Some(pos) = still_idle
                && !have_visible_work(p)
                && p.running.load(SeqCst) > p.target
            {
                // Retire lazily: leave the pool.
                idle.remove(pos);
                p.idle_count.store(idle.len(), SeqCst);
                drop(idle);
                *p.slots[slot].thread.lock().unwrap() = None;
                p.slots[slot].occupied.store(false, SeqCst);
                p.running.fetch_sub(1, SeqCst);
                p.unblocked.fetch_sub(1, SeqCst);
                return;
            }
        } else {
            std::thread::park();
        }
    }
}

fn have_visible_work(p: &Pool) -> bool {
    // NOTE: LIFO slots are deliberately invisible here — they are
    // owner-private (only their worker can claim them), so counting
    // them would spin the whole pool on work nobody else can take.
    if !p.injector.lock().unwrap().is_empty() {
        return true;
    }
    p.slots
        .iter()
        .any(|s| s.occupied.load(SeqCst) && s.deque.len_hint() > 0)
}

fn find_task(p: &Pool, slot: usize, rng: &mut SchedRng) -> Option<Box<Task>> {
    // 1. LIFO slot (spawn locality).
    let own = &p.slots[slot];
    let t = own.lifo.swap(std::ptr::null_mut(), SeqCst);
    if !t.is_null() {
        // SAFETY: pointer from Box::into_raw, sole taker via swap.
        return Some(unsafe { Box::from_raw(t) });
    }
    // 2. Own deque.
    if let Some(t) = own.deque.pop() {
        // SAFETY: pointer from Box::into_raw, popped once.
        return Some(unsafe { Box::from_raw(t) });
    }
    // 3. Injector.
    if let Some(t) = p.injector.lock().unwrap().pop_front() {
        return Some(t);
    }
    // 4. Steal — victim choice draws from the seeded PRNG (the one
    //    nondeterminism the s32 scheduler owns).
    for _ in 0..(4 * p.cap) {
        let victim = rng.below(p.cap);
        if victim == slot || !p.slots[victim].occupied.load(SeqCst) {
            continue;
        }
        sched_point(SchedEvent::Steal {
            thief: slot,
            victim,
        });
        match p.slots[victim].deque.steal() {
            // SAFETY: pointer from Box::into_raw, CAS-claimed once.
            Steal::Taken(t) => return Some(unsafe { Box::from_raw(t) }),
            Steal::Retry => std::hint::spin_loop(),
            Steal::Empty => {}
        }
    }
    None
}

#[allow(clippy::boxed_local)] // tasks travel the queues as raw Boxes
fn run_task(task: Box<Task>) {
    let Task {
        id,
        name,
        scope,
        body,
    } = *task;
    #[cfg(unix)]
    super::stack::set_fault_label(&name);
    scope.child_state(id, TaskState::Running);
    // det (s36): a tracked task joins the det domain and waits for
    // its first grant before the body runs; the wait routes through
    // `blocking` so the pool compensates for the parked worker. The
    // trackedness check comes first — untracked tasks (and release
    // builds) must not pay `blocking`'s LIFO flush.
    #[cfg(any(test, feature = "sched-test"))]
    if super::det::is_tracked(&scope) {
        blocking(|| super::det::ctx_enter(id, &scope));
    }
    // s76: workers are REUSED, so a body that left the thread's ambient
    // region set would hand the next task a handle that is no longer
    // live. A task starts at the process root either way — a task's
    // allocations are not its spawner's region's (the region-transfer
    // seam is how a region crosses a spawn).
    // The trap-containment handle (D68, s132): while the body runs,
    // a contained trap retires the task through this slot instead of
    // through the return path below. Cleared after the body either
    // way — a task that completed normally must not be retirable.
    CURRENT_TASK.with(|c| *c.borrow_mut() = Some((id, scope.clone())));
    let reason = crate::native::with_root_ambient(|| {
        with_scope(&scope, || match body {
            Body::Rust(f) => {
                let ctx = TaskCtx {
                    scope: scope.clone(),
                };
                // Unwind-safe flag guard: kill teardown may only unwind
                // Rust task frames, and the flag must reset even when the
                // body unwinds (workers are pooled — TLS outlives tasks).
                struct RustTaskFlag;
                impl Drop for RustTaskFlag {
                    fn drop(&mut self) {
                        IN_RUST_TASK.with(|c| c.set(false));
                    }
                }
                // D30: a panic becomes an exit reason, never an unwind
                // across the scheduler. The kill-teardown token is the
                // one distinguished payload (`[conc.proc.kill]` step 1).
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    IN_RUST_TASK.with(|c| c.set(true));
                    let _guard = RustTaskFlag;
                    f(&ctx)
                })) {
                    Ok(reason) => reason,
                    Err(payload) if payload.is::<KilledToken>() => ExitReason::Killed,
                    Err(_) => ExitReason::Panicked,
                }
            }
            Body::C { entry, env } => {
                // SAFETY: codegen's contract — `entry` is the lowered
                // task body, `env` its moved environment.
                let tag = unsafe { entry(env.0) };
                if tag == 0 {
                    ExitReason::Normal
                } else if tag == super::conc_abi::CANCEL_TAG {
                    // The s73 protocol extension (reviewed): the reserved
                    // cancel sentinel is the compiled body saying
                    // "structured cancellation stopped me" — a
                    // consequence, never a scope failure (conc_abi.rs).
                    ExitReason::Cancelled
                } else {
                    ExitReason::Error { tag }
                }
            }
        })
    });
    #[cfg(unix)]
    super::stack::set_fault_label("");
    CURRENT_TASK.with(|c| *c.borrow_mut() = None);
    scope.child_done(id, reason);
    // det (s36): completion (child_done's join/cancel wakeups
    // included) happened under the baton; release it for the next
    // grant. No-op when this task never entered the det domain.
    super::hooks::det_ctx_exit();
}

// ---- worker thread creation ---------------------------------------------

#[cfg(unix)]
mod os {
    //! pthread workers on pooled stacks (stack.rs). A retiring worker
    //! cannot return its own stack while running on it, so it parks
    //! the pair (pthread_t, stack) in the store and the NEXT taker
    //! `pthread_join`s the dead thread before reusing the span.
    use super::super::stack::TaskStack;
    use std::sync::{Mutex, OnceLock};

    // POSIX `pthread_attr_setstack` exists on macOS (pthread.h) but
    // the libc crate does not bind it for apple targets (it carries
    // only the deprecated setstackaddr/setstacksize pair) — declared
    // here directly, s59.
    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        fn pthread_attr_setstack(
            attr: *mut libc::pthread_attr_t,
            stackaddr: *mut libc::c_void,
            stacksize: libc::size_t,
        ) -> libc::c_int;
    }
    #[cfg(not(target_os = "macos"))]
    use libc::pthread_attr_setstack;

    struct Store {
        free: Vec<TaskStack>,
        retired: Vec<(libc::pthread_t, TaskStack)>,
    }

    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();

    fn store() -> &'static Mutex<Store> {
        STORE.get_or_init(|| {
            Mutex::new(Store {
                free: Vec::new(),
                retired: Vec::new(),
            })
        })
    }

    fn take_stack() -> Option<TaskStack> {
        let mut st = store().lock().unwrap();
        if let Some((thread, stack)) = st.retired.pop() {
            // SAFETY: the pthread_t was parked by its own thread on
            // exit and joined exactly once, here.
            unsafe { libc::pthread_join(thread, std::ptr::null_mut()) };
            return Some(stack);
        }
        if let Some(s) = st.free.pop() {
            return Some(s);
        }
        drop(st);
        TaskStack::map()
    }

    fn park_retired(stack: TaskStack) {
        // SAFETY: pthread_self of the exiting worker.
        let me = unsafe { libc::pthread_self() };
        store().lock().unwrap().retired.push((me, stack));
    }

    struct Boot {
        run: Box<dyn FnOnce() + Send>,
        stack: TaskStack,
    }

    extern "C" fn thread_entry(arg: *mut libc::c_void) -> *mut libc::c_void {
        // SAFETY: arg is the Box<Boot> leaked by spawn_worker.
        let boot = unsafe { Box::from_raw(arg.cast::<Boot>()) };
        boot.stack.enter(); // altstack for the overflow report
        let (lo, len) = boot.stack.span();
        super::WORKER_SPAN.with(|c| c.set(Some((lo, len))));
        (boot.run)();
        park_retired(boot.stack);
        std::ptr::null_mut()
    }

    /// Spawn `run` on a pooled stack. Falls back to `std::thread` if
    /// mapping or pthread_create fails (never silently loses a
    /// worker).
    pub fn spawn_worker(run: Box<dyn FnOnce() + Send>) {
        let Some(stack) = take_stack() else {
            std::thread::spawn(run);
            return;
        };
        let (lo, len) = (stack.stack_lo(), stack.stack_len());
        let boot = Box::into_raw(Box::new(Boot { run, stack }));
        // SAFETY: attr init/setstack/create with a valid attr and a
        // page-aligned span we own for the thread's whole life.
        unsafe {
            let mut attr: libc::pthread_attr_t = std::mem::zeroed();
            libc::pthread_attr_init(&mut attr);
            pthread_attr_setstack(&mut attr, lo.cast(), len);
            let mut tid: libc::pthread_t = std::mem::zeroed();
            let rc = libc::pthread_create(&mut tid, &attr, thread_entry, boot.cast());
            libc::pthread_attr_destroy(&mut attr);
            if rc != 0 {
                // Reclaim and fall back.
                let boot = Box::from_raw(boot);
                let Boot { run, stack } = *boot;
                store().lock().unwrap().free.push(stack);
                std::thread::spawn(run);
            }
            // The pthread_t is NOT detached and NOT joined here: the
            // worker parks it with its stack on exit (park_retired)
            // and the next take_stack joins it.
        }
    }
}

#[cfg(unix)]
fn trim_own_stack(_p: &Pool, _slot: usize) {
    // An idle worker releases committed pages beneath its parked
    // frame back to the OS (Target 3 recycling: pooled stacks do not
    // pin peak RSS). The local's address approximates SP.
    let probe = 0u8;
    let sp = std::ptr::from_ref(&probe) as usize;
    if let Some((lo, len)) = WORKER_SPAN.with(std::cell::Cell::get) {
        super::stack::trim_range(lo, len, sp);
    }
}

#[cfg(unix)]
thread_local! {
    /// The worker's own stack-proper bounds, recorded at thread entry
    /// (os::thread_entry), read by the idle trim.
    static WORKER_SPAN: std::cell::Cell<Option<(usize, usize)>> =
        const { std::cell::Cell::new(None) };
}

fn start_worker(p: &'static Pool, slot: usize, extra: bool) {
    let run: Box<dyn FnOnce() + Send> = Box::new(move || worker_main(p, slot, extra));
    #[cfg(unix)]
    os::spawn_worker(run);
    #[cfg(not(unix))]
    {
        // Recorded delta: no VirtualAlloc guard walker yet — std
        // stacks at the same reserve (see module doc).
        let _ = std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn(move || run());
    }
}

/// Test/diagnostic visibility: (target, running, unblocked).
pub fn counters() -> (usize, usize, usize) {
    match POOL.get() {
        Some(p) => (p.target, p.running.load(SeqCst), p.unblocked.load(SeqCst)),
        None => (0, 0, 0),
    }
}

/// True once the pool has ever been initialized (Target 1 tests).
pub fn initialized() -> bool {
    POOL.get().is_some()
}
