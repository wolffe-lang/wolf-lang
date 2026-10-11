//! The work-stealing pool (s32 Targets 2/6) — plain OS threads.
//!
//! Per-worker Chase–Lev deques plus a LIFO slot for spawn locality, a
//! global injector queue, parked-worker wakeup via `std::thread`
//! park/unpark (futex-backed on linux — the contract's "futex or
//! equivalent"). Default size = logical cores; grows under **blocking
//! compensation** (Loom's strategy at thread level): runtime-owned
//! blocking primitives wrap themselves in [`blocking`], which marks
//! the worker blocked, and when unblocked workers drop below target
//! parallelism the pool spins up a replacement thread; replacements
//! retire lazily when idle. Since s231 (wolf-lang#713) there is no cap
//! on replacements: the pool holds `target` unblocked workers plus one
//! thread for every task parked in a runtime-owned wait, because a
//! parked task holds its thread and the task that would wake it may be
//! the next one queued. Before s231 compensation stopped at
//! `max(8 t, t + 4)` workers, and a program that parked that many
//! tasks at once (one per connection) stopped. The slot table grows in
//! chunks; the operating system's thread limit is the last word, and a
//! thread it refuses is reported once on stderr. Raw FFI that blocks is
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
//! stack.rs (unix) or on the kernel's own reserve-and-guard stacks
//! (windows, stack_win.rs — s60b: the reservation is `CreateThread`'s,
//! the guard walker ntdll's, the overflow reporter a vectored
//! exception handler; the span is not ours to pool, so idle trim and
//! the s36 commit-failure injection are the named s60c delta there).
//!
//! The pool, like every static here, is **lazily initialized on first
//! spawn** (Target 1): a wolf binary that never spawns never creates
//! a thread, installs a handler, or touches a queue.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
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

impl WorkerSlot {
    fn new() -> WorkerSlot {
        WorkerSlot {
            occupied: AtomicBool::new(false),
            thread: Mutex::new(None),
            deque: Deque::new(),
            lifo: AtomicPtr::new(std::ptr::null_mut()),
        }
    }
}

/// Worker slots per chunk past the first ([`Pool::slot`]).
const CHUNK: usize = 128;
/// Chunks past the first: 8,192 x 128 = 1,048,576 more slots than the
/// first chunk's, far past any host's thread limit (kasumi: 254,378).
const MAX_CHUNKS: usize = 8_192;

struct Pool {
    target: usize,
    /// The first chunk's size: s32's fixed table, `max(8 t, t + 4)`
    /// slots, made at start. The steal probe's random draw ranges over
    /// these exactly as it always has.
    base: usize,
    /// The first chunk, `base` slots.
    core: Box<[WorkerSlot]>,
    /// Further chunks of [`CHUNK`] slots each, made on demand (s231)
    /// and never freed (the pool is a static). A null entry is a chunk
    /// not yet made; a slot below `made` always has its chunk.
    more: Box<[AtomicPtr<WorkerSlot>]>,
    /// Slots ever handed out: every slot below it is occupied or in
    /// `free`. Published after the slot's chunk exists.
    made: AtomicUsize,
    /// Vacated slots, lowest first — the one taken next, as the old
    /// first-free scan took it. Also the lock that serializes growth.
    free: Mutex<BinaryHeap<Reverse<usize>>>,
    /// The operating system refused a worker thread once: reported.
    refused: AtomicBool,
    /// Idle extras asleep with no timeout (they had no surplus to
    /// retire from); a blocking exit that makes a surplus wakes one.
    sleepers: AtomicUsize,
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

impl Pool {
    /// Slot `i`, which must be below `made` (or a core slot).
    fn slot(&self, i: usize) -> &WorkerSlot {
        if i < self.base {
            return &self.core[i];
        }
        let j = i - self.base;
        let chunk = self.more[j / CHUNK].load(SeqCst);
        assert!(!chunk.is_null(), "worker slot {i} has no chunk");
        // SAFETY: a non-null chunk is a leaked `Box<[WorkerSlot]>` of
        // CHUNK slots (`claim_slot`), never freed.
        unsafe { &*chunk.add(j % CHUNK) }
    }

    /// Slots handed out so far (every one of them exists).
    fn made(&self) -> usize {
        self.made.load(SeqCst)
    }

    /// Take a free slot for a new worker and mark it occupied: the
    /// lowest vacated one, else a fresh one (making its chunk). `None`
    /// only past the table's ceiling.
    fn claim_slot(&self) -> Option<usize> {
        let mut free = self.free.lock().unwrap();
        while let Some(Reverse(i)) = free.pop() {
            // A vacated core slot may have been retaken by
            // `ensure_workers` since it was listed.
            if !self.slot(i).occupied.swap(true, SeqCst) {
                return Some(i);
            }
        }
        let i = self.made.load(SeqCst);
        if i >= self.base {
            let j = i - self.base;
            if j / CHUNK >= MAX_CHUNKS {
                return None;
            }
            if self.more[j / CHUNK].load(SeqCst).is_null() {
                let chunk: Box<[WorkerSlot]> = (0..CHUNK).map(|_| WorkerSlot::new()).collect();
                self.more[j / CHUNK].store(Box::leak(chunk).as_mut_ptr(), SeqCst);
            }
        }
        self.slot(i).occupied.store(true, SeqCst);
        self.made.store(i + 1, SeqCst);
        Some(i)
    }

    /// Give a slot back: its worker has left the pool.
    fn vacate(&self, i: usize) {
        let mut free = self.free.lock().unwrap();
        *self.slot(i).thread.lock().unwrap() = None;
        self.slot(i).occupied.store(false, SeqCst);
        free.push(Reverse(i));
    }
}

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
/// never resumed.
///
/// The THREAD stays (its frames cannot be discarded), but since s228
/// (wolf-lang#662) it stops being a WORKER: it hands its queued work
/// to the injector and gives its pool slot back before it parks, so
/// the pool starts a replacement in that slot. Until s228 the parked
/// thread kept the slot — counted as a blocked worker for ever — and
/// the pool had a fixed table of slots (eight per core): after that
/// many contained traps every slot was a parked thread, nothing could
/// run a task, and the process stopped answering (hasu, 20 cpus:
/// between 120 and 250 traps; a 2-cpu host had 16 slots). Since s231
/// the table grows, but a slot held for ever is still a slot lost.
///
/// What a contained trap still costs is the parked thread itself: its
/// kernel record and the stack pages its dead frames occupy. The
/// thread's reusable buffers are released here; the stack below the
/// trap is trimmed.
pub(crate) fn park_after_contained_trap() -> ! {
    if let Some(w) = WORKER_ID.with(std::cell::Cell::get) {
        let p = pool();
        let slot = p.slot(w);
        // Everything this worker alone could reach goes where any
        // worker can take it: the LIFO slot and its own deque (thieves
        // skip an unoccupied slot).
        let held = slot.lifo.swap(std::ptr::null_mut(), SeqCst);
        if !held.is_null() {
            // SAFETY: pointer from Box::into_raw in spawn_task.
            let boxed = unsafe { Box::from_raw(held) };
            p.injector.lock().unwrap().push_back(boxed);
        }
        while let Some(t) = slot.deque.pop() {
            // SAFETY: pointer from Box::into_raw, popped once.
            let boxed = unsafe { Box::from_raw(t) };
            p.injector.lock().unwrap().push_back(boxed);
        }
        // Leave the pool: no longer a worker, the slot is free.
        WORKER_ID.with(|c| c.set(None));
        p.vacate(w);
        p.running.fetch_sub(1, SeqCst);
        p.unblocked.fetch_sub(1, SeqCst);
        compensate(p);
        wake_one(p);
        #[cfg(unix)]
        trim_own_stack(p, w);
    }
    crate::scratch::release_thread();
    crate::native::release_thread_pools();
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
        super::stack::install_overflow_handler();
        let target = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .max(2);
        let base = (target * 8).max(target + 4);
        let core = (0..base).map(|_| WorkerSlot::new()).collect();
        let more = (0..MAX_CHUNKS)
            .map(|_| AtomicPtr::new(std::ptr::null_mut()))
            .collect();
        Pool {
            target,
            base,
            core,
            more,
            // The core workers take 0..target (`ensure_workers`).
            made: AtomicUsize::new(target),
            free: Mutex::new(BinaryHeap::new()),
            refused: AtomicBool::new(false),
            sleepers: AtomicUsize::new(0),
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
/// The pool's target parallelism — `[conc.task.par.chunk]`'s `W`,
/// the worker count `par` bounds its chunk count by (s166). One per
/// logical core, at least two.
pub fn worker_target() -> usize {
    pool().target
}

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
            let slot = p.slot(w);
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
        if !p.slot(slot).occupied.swap(true, SeqCst) {
            p.running.fetch_add(1, SeqCst);
            p.unblocked.fetch_add(1, SeqCst);
            start_or_report(p, slot, false);
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
        if let Some(t) = p.slot(w).thread.lock().unwrap().as_ref() {
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
    compensated(f)
}

/// [`blocking`]'s compensation alone, for a wait the program asked for
/// by name — `time_sleep_ms`, `net_wait` (s231: until then they held
/// their worker with no compensation at all). They are not runtime
/// blocking primitives in [`blocking`]'s handler sense, so its handler
/// assert is not applied to them.
pub(crate) fn compensated<R>(f: impl FnOnce() -> R) -> R {
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
    let slot = p.slot(w);
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
    if p.unblocked.fetch_add(1, SeqCst) + 1 > p.target {
        nudge_sleeping_extra(p);
    }
    r
}

/// A surplus of unblocked workers exists: wake one idle extra that is
/// asleep with no timeout, so it can retire. It stays on the idle list
/// (retirement requires that), so this only unparks it.
fn nudge_sleeping_extra(p: &Pool) {
    if p.sleepers.load(SeqCst) == 0 {
        return;
    }
    let extra = {
        let idle = p.idle.lock().unwrap();
        idle.iter().rev().copied().find(|&w| w >= p.target)
    };
    if let Some(w) = extra
        && let Some(t) = p.slot(w).thread.lock().unwrap().as_ref()
    {
        t.unpark();
    }
}

/// Live unblocked workers dipped below target: add a replacement
/// thread. Any free slot serves (s228): a core slot is free only when a
/// contained trap vacated it ([`park_after_contained_trap`]), and its
/// replacement is a core worker like the one it replaces — it never
/// retires. No cap (s231, wolf-lang#713): a parked task holds its
/// thread, so every one parked needs a worker beside the `target`
/// unblocked ones, or the task that would wake them never runs. The
/// count is bounded by the tasks parked at once; the operating system
/// bounds it after that ([`start_or_report`]).
fn compensate(p: &'static Pool) {
    if p.unblocked.load(SeqCst) >= p.target {
        return;
    }
    // PLANT (s231, reverted next): the old cap, `max(8 t, t + 4)`.
    if p.running.load(SeqCst) >= p.base {
        return;
    }
    let Some(slot) = p.claim_slot() else {
        report_refusal(p, "the worker table is full");
        return;
    };
    p.running.fetch_add(1, SeqCst);
    p.unblocked.fetch_add(1, SeqCst);
    start_or_report(p, slot, slot >= p.target);
}

/// Start a worker in the occupied `slot`; when the operating system
/// refuses the thread, give the slot back, take the counts down and say
/// so once. The process keeps running on the workers it has: a queued
/// task waits until a parked one returns (the pre-s231 stall, now named
/// where it happens).
fn start_or_report(p: &'static Pool, slot: usize, extra: bool) {
    if let Err(e) = start_worker(p, slot, extra) {
        p.vacate(slot);
        p.running.fetch_sub(1, SeqCst);
        p.unblocked.fetch_sub(1, SeqCst);
        report_refusal(p, &e);
    }
}

fn report_refusal(p: &Pool, why: &str) {
    if !p.refused.swap(true, SeqCst) {
        let running = p.running.load(SeqCst);
        let unblocked = p.unblocked.load(SeqCst);
        eprintln!(
            "wolf: the runtime could not start another worker thread ({why}); {running} \
             are running, {} of them held by tasks parked in waits. A task queued now \
             waits until a parked task returns. (Reported once.)",
            running.saturating_sub(unblocked)
        );
    }
}

// ---- the worker ----------------------------------------------------------

fn worker_main(p: &'static Pool, slot: usize, extra: bool) {
    WORKER_ID.with(|c| c.set(Some(slot)));
    *p.slot(slot).thread.lock().unwrap() = Some(std::thread::current());
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
            if p.unblocked.load(SeqCst) > p.target {
                // Surplus: wait out the retire window, then leave.
                std::thread::park_timeout(std::time::Duration::from_millis(50));
            } else {
                // No surplus, so this extra cannot retire: sleep until
                // work or a surplus wakes it (s231). Until s231 it woke
                // every 50 ms regardless, and each wake scans every
                // slot — with 10,000 tasks parked, sixteen idle extras
                // cost 28 % of a cpu. `sleepers` before the re-check, so
                // a surplus made after the re-check sees this sleeper
                // ([`nudge_sleeping_extra`]); park's token covers an
                // unpark that lands before the park.
                p.sleepers.fetch_add(1, SeqCst);
                if p.unblocked.load(SeqCst) <= p.target {
                    std::thread::park();
                }
                p.sleepers.fetch_sub(1, SeqCst);
            }
            let mut idle = p.idle.lock().unwrap();
            let still_idle = idle.iter().position(|&w| w == slot);
            // Retire only out of SURPLUS unblocked parallelism, and take
            // the unblocked count down in the same step (wolf-lang#570,
            // s210). 0.2.23 retired while `running > target`, and
            // `running` counts blocked workers: with `target` workers
            // blocked, every idle extra left the pool, nothing was left
            // to run new work, and nothing compensates on spawn. A
            // spawned proc then never ran. The compare-and-swap keeps a
            // concurrent `blocking` entry's own decrement and
            // compensation decision consistent with this one.
            if let Some(pos) = still_idle
                && !have_visible_work(p)
                && p.unblocked
                    .fetch_update(SeqCst, SeqCst, |u| (u > p.target).then(|| u - 1))
                    .is_ok()
            {
                // Retire lazily: leave the pool.
                idle.remove(pos);
                p.idle_count.store(idle.len(), SeqCst);
                drop(idle);
                p.vacate(slot);
                p.running.fetch_sub(1, SeqCst);
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
    (0..p.made()).any(|i| {
        let s = p.slot(i);
        s.occupied.load(SeqCst) && s.deque.len_hint() > 0
    })
}

fn find_task(p: &Pool, slot: usize, rng: &mut SchedRng) -> Option<Box<Task>> {
    // 1. LIFO slot (spawn locality).
    let own = p.slot(slot);
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
    //    nondeterminism the s32 scheduler owns), over the first chunk
    //    exactly as before s231.
    for _ in 0..(4 * p.base) {
        let victim = rng.below(p.base);
        if let Some(t) = try_steal(p, slot, victim) {
            return Some(t);
        }
    }
    // 5. Past the first chunk (s231: the pool grew because tasks
    //    parked, and a parked worker's deque holds what it spawned
    //    before it parked): one pass from a random start, so a thief
    //    finds any stealable task in time linear in the workers.
    let made = p.made();
    if made > p.base {
        let span = made - p.base;
        let start = rng.below(span);
        for k in 0..span {
            if let Some(t) = try_steal(p, slot, p.base + (start + k) % span) {
                return Some(t);
            }
        }
    }
    None
}

fn try_steal(p: &Pool, slot: usize, victim: usize) -> Option<Box<Task>> {
    if victim == slot || victim >= p.made() || !p.slot(victim).occupied.load(SeqCst) {
        return None;
    }
    sched_point(SchedEvent::Steal {
        thief: slot,
        victim,
    });
    match p.slot(victim).deque.steal() {
        // SAFETY: pointer from Box::into_raw, CAS-claimed once.
        Steal::Taken(t) => Some(unsafe { Box::from_raw(t) }),
        Steal::Retry => {
            std::hint::spin_loop();
            None
        }
        Steal::Empty => None,
    }
}

#[allow(clippy::boxed_local)] // tasks travel the queues as raw Boxes
fn run_task(task: Box<Task>) {
    let Task {
        id,
        name,
        scope,
        body,
    } = *task;
    super::stack::set_fault_label(&name);
    // s200 (#407, `[os.fs.error]`): workers are reused, and `os_error()` is
    // the TASK's — a task starts with no code, never its predecessor's.
    crate::fs::os_error_clear();
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
    /// mapping or pthread_create fails; `Err` when that fails too (the
    /// operating system's thread or memory limit — the caller reports
    /// it, s231).
    pub fn spawn_worker(run: Box<dyn FnOnce() + Send>) -> Result<(), String> {
        let Some(stack) = take_stack() else {
            return fallback(run, "no stack could be mapped");
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
                let why = format!("pthread_create: {}", std::io::Error::from_raw_os_error(rc));
                return fallback(run, &why);
            }
            // The pthread_t is NOT detached and NOT joined here: the
            // worker parks it with its stack on exit (park_retired)
            // and the next take_stack joins it.
        }
        Ok(())
    }

    /// The plain `std::thread` worker, when the pooled stack could not
    /// be had; `Err` names both refusals.
    fn fallback(run: Box<dyn FnOnce() + Send>, first: &str) -> Result<(), String> {
        std::thread::Builder::new()
            .spawn(run)
            .map(drop)
            .map_err(|e| format!("{first}; std::thread: {e}"))
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

#[cfg(windows)]
mod os {
    //! Win32 workers (s60b). `std::thread::Builder::stack_size` is
    //! `CreateThread(…, STACK_SIZE_PARAM_IS_A_RESERVATION)`: the
    //! kernel reserves the span and commits on fault behind its
    //! `PAGE_GUARD` page (stack_win.rs's module doc says why the span
    //! is the kernel's, not ours). The reserve honors `WOLF_TASK_STACK`
    //! exactly as the unix mapping does; the worker's first act is to
    //! hold back the stack guarantee the overflow report runs in.
    use super::super::stack;

    pub fn spawn_worker(run: Box<dyn FnOnce() + Send>) -> Result<(), String> {
        std::thread::Builder::new()
            .name("wolf-worker".into())
            .stack_size(stack::reserve_size())
            .spawn(move || {
                stack::enter();
                run()
            })
            .map(drop)
            .map_err(|e| format!("CreateThread: {e}"))
    }
}

fn start_worker(p: &'static Pool, slot: usize, extra: bool) -> Result<(), String> {
    let run: Box<dyn FnOnce() + Send> = Box::new(move || worker_main(p, slot, extra));
    os::spawn_worker(run)
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
