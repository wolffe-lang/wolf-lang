//! s226 — structured concurrency on the checked machine (C1).
//!
//! spec/03 §2–§3 executed on [`super::Machine`]: scopes and their
//! joins, spawned tasks, procs, channels that block, `select`, `when`,
//! `par`, and a race detector over the memory two tasks can reach.
//!
//! # The task model (`[exec.checked.task]`)
//!
//! A task is one evaluator stack. A tree-walk that must sleep in the
//! middle of an expression needs a call stack to sleep on, so each task
//! owns an OS thread — and **at most one task runs at a time**: the
//! [`Machine`] itself is the baton. A task that leaves the cpu moves
//! the whole machine into the [`Hub`]'s one slot, opens the next task's
//! [`Gate`] and waits on its own; what stays behind on its thread is a
//! blank machine nobody reads. There is no parallelism, no lock around
//! machine state, and no `unsafe`.
//!
//! # Scheduling (`[exec.checked.sched]`)
//!
//! The scheduler is lupin's (wolf-interp `src/eval/sched.rs`), ported
//! as plain data: a FIFO ready queue; a task runs until it reaches a
//! blocking point (a scope's join, a channel operation that cannot
//! complete, a `select` with no ready arm, a held `Mutex`, a proc join,
//! a sleep) and is never preempted; a spawn makes the child ready and
//! the parent keeps running. A decision is taken where more than one
//! task is ready and where more than one `select` arm is ready. With no
//! seed, or seed 0, every decision takes the first candidate; any other
//! seed below 2^62 draws from xorshift64 started at `seed | 1`; a seed
//! with bit 62 set and bit 63 clear is a packed schedule — its low 62
//! bits are the choices, mixed radix, least significant first
//! (`[sched.seed]`). One seed is one interleaving: the machine explores
//! none, so every outcome it shows is one a real schedule could show.
//!
//! # The race rule (`[exec.checked.race]`)
//!
//! Every task carries a vector clock. Happens-before is
//! `[conc.mm.hb]`'s: a spawn (the child starts from its parent's
//! clock), a scope's join (every child into the owner), the k-th send
//! into the k-th receive (the message carries the sender's clock, and
//! on a rendezvous channel the receive goes back into the sender), a
//! `Mutex` release into the next acquisition, a proc's exit into each
//! delivery of its reason, and — every order being run as `seq_cst`
//! (`[conc.mm.atomic.raw.5]`) — each atomic operation through its
//! location's clock. Two accesses to overlapping bytes of one raw
//! allocation (or one pool slot, or one module `var`) by two tasks, at
//! least one a write, with no order between them, are a data race:
//! `trap(race)` at `[conc.mm.race.3]`, at the second access. Two atomic
//! accesses never race; an atomic and a plain one do.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};

use super::*;

/// `W` of `[conc.task.par.chunk]`: fixed, as lupin's is, so one seed is
/// one chunk count on every host.
pub(super) const PAR_WORKERS: usize = 4;

/// A task thread's stack: the machine's own (`CHECKED_STACK_BYTES`),
/// so the call-depth budget answers before the host stack on a task
/// exactly as it does on `main`.
pub(super) const TASK_STACK_BYTES: usize = CHECKED_STACK_BYTES;

/// The status of a run whose root domain was killed
/// (`[conc.proc.root]`: nonzero, implementation-specified; this is the
/// native runtime's number).
pub(super) const ROOT_KILLED_STATUS: u8 = 121;

pub(super) type TaskId = usize;

// --------------------------------------------------------------- the baton --

/// The gate one task's thread waits on. Open means "the machine is in
/// the slot for you".
#[derive(Default)]
pub(super) struct Gate {
    open: Mutex<bool>,
    cv: Condvar,
}

impl Gate {
    fn open(&self) {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        *open = true;
        self.cv.notify_one();
    }

    fn wait(&self) {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        while !*open {
            open = self.cv.wait(open).unwrap_or_else(|e| e.into_inner());
        }
        *open = false;
    }
}

/// Where the machine rests between two threads.
pub(super) struct Hub<'t> {
    slot: Mutex<Option<Machine<'t>>>,
    /// Every gate handed out, so a host panic can release each waiter.
    gates: Mutex<Vec<Arc<Gate>>>,
    /// A thread of this run panicked: nobody will hand the machine on.
    abandoned: AtomicBool,
    panic: Mutex<Option<Box<dyn std::any::Any + Send>>>,
}

/// What a thread unwinds with when the run was abandoned under it.
struct Abandoned;

impl<'t> Hub<'t> {
    pub(super) fn new() -> Arc<Hub<'t>> {
        Arc::new(Hub {
            slot: Mutex::new(None),
            gates: Mutex::new(Vec::new()),
            abandoned: AtomicBool::new(false),
            panic: Mutex::new(None),
        })
    }

    fn gate(&self) -> Arc<Gate> {
        let gate = Arc::new(Gate::default());
        self.gates
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(gate.clone());
        gate
    }

    /// A thread panicked: keep the first payload and wake every waiter,
    /// each of which unwinds its own thread. The run's caller sees the
    /// panic, as it did when the machine had one thread.
    pub(super) fn abandon(&self, payload: Box<dyn std::any::Any + Send>) {
        if !payload.is::<Abandoned>() {
            let mut slot = self.panic.lock().unwrap_or_else(|e| e.into_inner());
            if slot.is_none() {
                *slot = Some(payload);
            }
        }
        self.abandoned.store(true, Ordering::SeqCst);
        // The machine holds the channel the thread service waits on:
        // drop it wherever it rests, or the run never ends.
        drop(self.slot.lock().unwrap_or_else(|e| e.into_inner()).take());
        for gate in self.gates.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            gate.open();
        }
    }

    /// Rests the machine in the slot. Under an abandoned run nobody
    /// will take it, so it is dropped here instead.
    fn rest(&self, machine: Machine<'t>) {
        *self.slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(machine);
        if self.abandoned.load(Ordering::SeqCst) {
            drop(self.slot.lock().unwrap_or_else(|e| e.into_inner()).take());
        }
    }

    /// Takes the machine back; `None` under an abandoned run.
    fn wake(&self) -> Option<Machine<'t>> {
        let machine = self.slot.lock().unwrap_or_else(|e| e.into_inner()).take();
        if self.abandoned.load(Ordering::SeqCst) {
            return None;
        }
        machine
    }

    pub(super) fn take_panic(&self) -> Option<Box<dyn std::any::Any + Send>> {
        self.panic.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

/// A request for one task thread, served by the thread that holds the
/// `std::thread::scope` ([`super::run_checked_fn`]).
pub(super) struct SpawnReq<'t> {
    pub(super) name: String,
    pub(super) run: Box<dyn FnOnce() + Send + 't>,
    pub(super) reply: mpsc::SyncSender<bool>,
}

// ------------------------------------------------------------------- state --

/// Why the scheduler woke a parked task.
#[derive(Debug, Clone)]
pub(super) enum Wake {
    /// The blocked operation completed with this value.
    Ok(Value),
    /// The channel closed under the blocked operation.
    Closed,
    /// Cancellation reached this blocking point (`[conc.cancel.points]`).
    Cancelled,
    /// The task's proc was killed, or the run is over.
    Killed,
    /// A `select` arm committed: its index, and the value for a receive.
    Arm(usize, Option<Value>),
    /// Every task is blocked and no timer is pending
    /// (`[conc.deadlock.def]`); the roster rides along.
    Deadlock(String),
}

/// How a task ended.
#[derive(Debug, Clone)]
pub(super) enum TaskEnd {
    Value(Value),
    /// An error value: a failure for `[conc.task.fail]`.
    Error(Value),
    /// A trap: re-raised at the scope's exit, or contained by a proc.
    Trapped(TrapInfo),
    Cancelled,
    Killed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskState {
    Ready,
    Running,
    Blocked,
    Done,
}

/// What a task runs.
#[derive(Debug, Clone)]
pub(super) enum Entry {
    /// A spawned closure, by arena index.
    Closure(usize),
    /// A named fn: a proc's body, or `s.spawn(f)` of a fn item.
    Call { body: usize, args: Vec<Value> },
    /// One chunk of a `par`: indices `lo..hi` of job `job`.
    ParChunk { job: usize, lo: usize, hi: usize },
}

/// The state a task owns while it is not running: its frames and what
/// rides beside them.
#[derive(Default)]
struct TaskCtx<'t> {
    frames: Vec<Frame<'t>>,
    self_tys: Vec<Option<String>>,
    frame_rigids: Vec<HashMap<String, String>>,
    pending_self_ty: Option<String>,
    pending_rigids: Option<HashMap<String, String>>,
    ambient: Vec<usize>,
    in_defer: bool,
    in_atomic: bool,
    last_os_error: i64,
    when_held: Vec<usize>,
}

struct Tcb<'t> {
    name: String,
    state: TaskState,
    proc: usize,
    scope: Option<usize>,
    gate: Option<Arc<Gate>>,
    /// The task's thread exists (it is made at the first schedule).
    started: bool,
    cancelled: bool,
    killed: bool,
    wake: Option<Wake>,
    ctx: Option<TaskCtx<'t>>,
    vc: Vec<u64>,
    entry: Option<Entry>,
}

#[derive(Default)]
struct ScopeState {
    owner: TaskId,
    children: Vec<TaskId>,
    /// The owner is parked at the scope's exit.
    joining: bool,
    /// Failures in schedule order (`[conc.task.fail]`).
    failures: Vec<TaskEnd>,
}

/// One channel. A buffered message carries its sender's clock
/// (`[conc.mm.hb.chan]`, `[conc.mm.hb.move]`).
struct Chan {
    cap: usize,
    buf: VecDeque<(Value, Vec<u64>)>,
    closed: bool,
    senders: VecDeque<(TaskId, Value)>,
    receivers: VecDeque<TaskId>,
    /// Tasks blocked in a `select` with an arm here: `(task, arm)`.
    selecters: Vec<(TaskId, usize)>,
}

struct Mx {
    value: Value,
    holder: Option<TaskId>,
    waiters: VecDeque<TaskId>,
    vc: Vec<u64>,
}

struct Proc {
    root: TaskId,
    /// The proc's own region (`[conc.proc.1]`); `None` for the root
    /// domain, whose region is the run's.
    region: Option<usize>,
    monitors: Vec<usize>,
    links: Vec<usize>,
    /// The exit reason, once the proc has one (`[conc.proc.exit]`).
    exited: Option<Value>,
}

struct Timer {
    deadline: u128,
    seq: u64,
    task: TaskId,
    arm: usize,
}

struct RealSleep {
    due: std::time::Instant,
    seq: u64,
    task: TaskId,
}

/// A proc exit whose reason is not published yet: the regions free
/// first (`[conc.proc.kill]` step 3 before step 4).
struct PendingExit {
    message: Value,
    vc: Vec<u64>,
    monitors: Vec<usize>,
}

struct Access {
    task: TaskId,
    tick: u64,
    lo: usize,
    hi: usize,
    write: bool,
}

/// The memory two tasks can both reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum RaceKey {
    Alloc(usize),
    Pool(usize, usize),
    Static(usize),
}

/// One `par`'s shared half: the fn value, the list it reads, and the
/// slots its chunks fill.
struct ParJob {
    f: Value,
    items: Vec<Value>,
    slots: Vec<Option<Value>>,
    span: Span,
}

enum Chooser {
    Seeded { seed: u64, rng: u64 },
    Packed { digits: u64 },
}

/// The scheduler: plain data inside the machine.
pub(super) struct Sched<'t> {
    tasks: Vec<Tcb<'t>>,
    ready: VecDeque<TaskId>,
    scopes: Vec<ScopeState>,
    chans: Vec<Chan>,
    mutexes: Vec<Mx>,
    procs: Vec<Proc>,
    timers: Vec<Timer>,
    timer_seq: u64,
    sleeps: Vec<RealSleep>,
    sleep_seq: u64,
    /// Virtual nanoseconds (`[conc.select.timeout]`).
    clock: u128,
    chooser: Chooser,
    /// The running task.
    current: TaskId,
    /// A second task has existed: the race detector's fast path.
    pub(super) concurrent: bool,
    /// The run is ending; the root wakes each task itself.
    shutdown: bool,
    /// A stop raised on a task's thread that ends the whole run (a UB
    /// finding, a refusal, a budget, `os_exit`): the root raises it.
    fatal: Option<Stop>,
    pending: Vec<PendingExit>,
    accesses: BTreeMap<RaceKey, Vec<Access>>,
    atomic_accesses: BTreeMap<RaceKey, Vec<Access>>,
    atomic_clocks: BTreeMap<(RaceKey, usize), Vec<u64>>,
    par_jobs: Vec<ParJob>,
}

/// One constructed closure.
pub(super) struct ClosureInst<'t> {
    /// The body whose typed tables the closure's spans live in.
    pub(super) body: usize,
    pub(super) node: &'t GreenNode,
    /// Captures, copied at construction.
    pub(super) captures: Vec<(String, Value)>,
    pub(super) self_ty: Option<String>,
    pub(super) rigids: HashMap<String, String>,
}

fn merge_vc(into: &mut Vec<u64>, from: &[u64]) {
    if into.len() < from.len() {
        into.resize(from.len(), 0);
    }
    for (slot, value) in into.iter_mut().zip(from) {
        *slot = (*slot).max(*value);
    }
}

fn tag(name: &str) -> Value {
    Value::ErrTag {
        tag: name.to_string(),
        payload: Vec::new(),
    }
}

/// The first remembered access that conflicts with this one and that
/// `my_vc` does not order.
fn unordered_conflict(
    history: &[Access],
    me: TaskId,
    my_vc: &[u64],
    lo: usize,
    hi: usize,
    write: bool,
) -> Option<(TaskId, bool)> {
    history
        .iter()
        .find(|prior| {
            prior.task != me
                && (prior.write || write)
                && prior.lo < hi
                && prior.hi > lo
                && my_vc.get(prior.task).copied().unwrap_or(0) < prior.tick
        })
        .map(|prior| (prior.task, prior.write))
}

/// Remember an access, keeping one entry per (task, range, kind): the
/// latest tick of a task on a range orders after its earlier ones, so
/// it conflicts whenever any of them would.
fn remember(history: &mut Vec<Access>, task: TaskId, tick: u64, lo: usize, hi: usize, write: bool) {
    match history
        .iter_mut()
        .find(|a| a.task == task && a.lo == lo && a.hi == hi && a.write == write)
    {
        Some(entry) => entry.tick = tick,
        None => history.push(Access {
            task,
            tick,
            lo,
            hi,
            write,
        }),
    }
}

impl<'t> Sched<'t> {
    /// `seed` as `[sched.seed]` reads it (0: every decision takes the
    /// first candidate).
    pub(super) fn new(seed: u64) -> Sched<'t> {
        const PACKED_TAG: u64 = 1 << 62;
        let chooser = if seed & PACKED_TAG != 0 && seed >> 63 == 0 {
            Chooser::Packed {
                digits: seed & (PACKED_TAG - 1),
            }
        } else {
            Chooser::Seeded {
                seed,
                rng: seed | 1,
            }
        };
        Sched {
            tasks: vec![Tcb {
                name: "main".to_string(),
                state: TaskState::Running,
                proc: 0,
                scope: None,
                gate: None,
                started: true,
                cancelled: false,
                killed: false,
                wake: None,
                ctx: None,
                vc: vec![1],
                entry: None,
            }],
            ready: VecDeque::new(),
            scopes: Vec::new(),
            chans: Vec::new(),
            mutexes: Vec::new(),
            procs: vec![Proc {
                root: 0,
                region: None,
                monitors: Vec::new(),
                links: Vec::new(),
                exited: None,
            }],
            timers: Vec::new(),
            timer_seq: 0,
            sleeps: Vec::new(),
            sleep_seq: 0,
            clock: 0,
            chooser,
            current: 0,
            concurrent: false,
            shutdown: false,
            fatal: None,
            pending: Vec::new(),
            accesses: BTreeMap::new(),
            atomic_accesses: BTreeMap::new(),
            atomic_clocks: BTreeMap::new(),
            par_jobs: Vec::new(),
        }
    }

    /// One decision among `n` candidates (`[conc.det.seed]`,
    /// `[conc.select.fair]`).
    fn decide(&mut self, n: usize) -> usize {
        match &mut self.chooser {
            Chooser::Seeded { seed, rng } => {
                if *seed == 0 || n <= 1 {
                    0
                } else {
                    let mut x = *rng;
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    *rng = x;
                    (x % n as u64) as usize
                }
            }
            Chooser::Packed { digits } => {
                if n <= 1 {
                    0
                } else {
                    let digit = (*digits % n as u64) as usize;
                    *digits /= n as u64;
                    digit
                }
            }
        }
    }

    fn tick(&mut self, task: TaskId) -> u64 {
        let len = self.tasks.len();
        let vc = &mut self.tasks[task].vc;
        if vc.len() < len {
            vc.resize(len, 0);
        }
        vc[task] += 1;
        vc[task]
    }

    /// Joins `from` into `task`'s clock: a happens-before edge.
    fn hb_edge(&mut self, from: &[u64], task: TaskId) {
        merge_vc(&mut self.tasks[task].vc, from);
        self.tick(task);
    }

    fn make_ready(&mut self, task: TaskId, wake: Wake) {
        let tcb = &mut self.tasks[task];
        if tcb.state == TaskState::Done {
            return;
        }
        tcb.wake = Some(wake);
        if tcb.state == TaskState::Blocked {
            tcb.state = TaskState::Ready;
            self.ready.push_back(task);
        }
    }

    /// Takes a task off every waiter list: it is being woken by one of
    /// its registrations and must not be woken twice.
    fn deregister(&mut self, task: TaskId) {
        for chan in &mut self.chans {
            chan.receivers.retain(|t| *t != task);
            chan.senders.retain(|(t, _)| *t != task);
            chan.selecters.retain(|(t, _)| *t != task);
        }
        for mx in &mut self.mutexes {
            mx.waiters.retain(|t| *t != task);
        }
        self.timers.retain(|timer| timer.task != task);
        self.sleeps.retain(|sleep| sleep.task != task);
    }

    fn promote_due_sleeps(&mut self) {
        if self.sleeps.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        let (mut due, keep): (Vec<RealSleep>, Vec<RealSleep>) =
            self.sleeps.drain(..).partition(|s| s.due <= now);
        self.sleeps = keep;
        due.sort_by_key(|s| s.seq);
        for sleep in due {
            if self.tasks[sleep.task].state == TaskState::Blocked {
                self.make_ready(sleep.task, Wake::Ok(Value::Unit));
            }
        }
    }

    /// The next task to run, advancing virtual time if it must; `None`
    /// when nothing can ever run again.
    fn choose_next(&mut self) -> Option<TaskId> {
        loop {
            self.promote_due_sleeps();
            if !self.ready.is_empty() {
                let index = self.decide(self.ready.len());
                return self.ready.remove(index);
            }
            if self.advance_clock() {
                continue;
            }
            let earliest = self.sleeps.iter().map(|s| s.due).min()?;
            let now = std::time::Instant::now();
            if earliest > now {
                std::thread::sleep(earliest - now);
            }
        }
    }

    /// Jumps the virtual clock to the earliest timer and fires what is
    /// due (`[conc.select.timeout]`).
    fn advance_clock(&mut self) -> bool {
        let Some(earliest) = self.timers.iter().map(|t| t.deadline).min() else {
            return false;
        };
        self.clock = self.clock.max(earliest);
        let clock = self.clock;
        let (mut due, keep): (Vec<Timer>, Vec<Timer>) =
            self.timers.drain(..).partition(|t| t.deadline <= clock);
        self.timers = keep;
        due.sort_by_key(|t| (t.deadline, t.seq));
        let fired = !due.is_empty();
        for timer in due {
            if self.tasks[timer.task].state == TaskState::Blocked {
                self.deregister(timer.task);
                self.make_ready(timer.task, Wake::Arm(timer.arm, None));
            }
        }
        fired
    }

    fn blocked_roster(&self) -> String {
        self.tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.state == TaskState::Blocked)
            .map(|(id, t)| format!("`{}` (task {id})", t.name))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// `[conc.deadlock.def]`: every blocked task wakes with the verdict.
    fn declare_deadlock(&mut self) {
        let roster = self.blocked_roster();
        let blocked: Vec<TaskId> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.state == TaskState::Blocked)
            .map(|(id, _)| id)
            .collect();
        for task in blocked {
            self.deregister(task);
            self.make_ready(task, Wake::Deadlock(roster.clone()));
        }
    }

    // -- channels -------------------------------------------------------

    pub(super) fn new_chan(&mut self, cap: usize) -> usize {
        self.chans.push(Chan {
            cap,
            buf: VecDeque::new(),
            closed: false,
            senders: VecDeque::new(),
            receivers: VecDeque::new(),
            selecters: Vec::new(),
        });
        self.chans.len() - 1
    }

    /// After a change on `chan`, complete whatever can complete now.
    fn settle_chan(&mut self, chan: usize) {
        loop {
            while self.chans[chan].buf.len() < self.chans[chan].cap
                && !self.chans[chan].senders.is_empty()
            {
                let (sender, value) = self.chans[chan].senders.pop_front().expect("non-empty");
                let vc = self.tasks[sender].vc.clone();
                // A send is a release: tick past the published clock.
                self.tick(sender);
                self.chans[chan].buf.push_back((value, vc));
                self.make_ready(sender, Wake::Ok(Value::Unit));
            }
            let receiver = self.chans[chan].receivers.front().copied();
            let selecter = self.chans[chan].selecters.first().copied();
            let has_buf = !self.chans[chan].buf.is_empty();
            let has_sender = !self.chans[chan].senders.is_empty();
            if !has_buf && !has_sender {
                if self.chans[chan].closed {
                    while let Some(receiver) = self.chans[chan].receivers.pop_front() {
                        self.make_ready(receiver, Wake::Closed);
                    }
                    let selecters = std::mem::take(&mut self.chans[chan].selecters);
                    for (task, arm) in selecters {
                        self.deregister(task);
                        self.make_ready(task, Wake::Arm(arm, Some(tag("closed"))));
                    }
                }
                return;
            }
            let Some((task, arm)) = receiver
                .map(|t| (t, None))
                .or(selecter.map(|(t, a)| (t, Some(a))))
            else {
                return;
            };
            let value = self.take_message(chan, task);
            self.deregister(task);
            match arm {
                None => self.make_ready(task, Wake::Ok(value)),
                Some(arm) => self.make_ready(task, Wake::Arm(arm, Some(value))),
            }
        }
    }

    /// Hands the next deliverable message to `receiver`: the buffer
    /// first, else a blocked sender's rendezvous value. The k-th send
    /// happens-before the k-th receive completes, and on a rendezvous
    /// the receive happens-before the send returns (`[conc.mm.hb.chan]`).
    fn take_message(&mut self, chan: usize, receiver: TaskId) -> Value {
        if let Some((value, vc)) = self.chans[chan].buf.pop_front() {
            self.hb_edge(&vc, receiver);
            return value;
        }
        let (sender, value) = self.chans[chan]
            .senders
            .pop_front()
            .expect("caller checked a sender exists");
        let vc = self.tasks[sender].vc.clone();
        self.tick(sender);
        self.hb_edge(&vc, receiver);
        let back = self.tasks[receiver].vc.clone();
        self.tick(receiver);
        self.hb_edge(&back, sender);
        self.make_ready(sender, Wake::Ok(Value::Unit));
        value
    }

    // -- cancellation, kill, exit ---------------------------------------

    /// Cooperative cancellation (`[conc.cancel.points]`): delivered now
    /// to a parked task, at its next blocking point otherwise, and down
    /// into every scope the task owns.
    fn cancel_task(&mut self, task: TaskId) {
        let tcb = &mut self.tasks[task];
        if tcb.state == TaskState::Done || tcb.cancelled || tcb.killed {
            return;
        }
        tcb.cancelled = true;
        if tcb.state == TaskState::Blocked {
            self.deregister(task);
            self.make_ready(task, Wake::Cancelled);
        }
        let owned: Vec<TaskId> = self
            .scopes
            .iter()
            .filter(|scope| scope.owner == task)
            .flat_map(|scope| scope.children.iter().copied())
            .collect();
        for child in owned {
            self.cancel_task(child);
        }
    }

    /// Records a proc's exit: the reason parks until the caller has
    /// freed the returned regions, and an abnormal exit takes every
    /// linked proc with it (`[conc.proc.2]`).
    fn deliver_exit(&mut self, proc: usize, reason: Value) -> Vec<usize> {
        if self.procs[proc].exited.is_some() {
            return Vec::new();
        }
        let abnormal = !matches!(&reason, Value::ErrTag { tag, .. } if tag == "normal");
        self.procs[proc].exited = Some(reason.clone());
        let root = self.procs[proc].root;
        self.pending.push(PendingExit {
            message: reason,
            vc: self.tasks[root].vc.clone(),
            monitors: self.procs[proc].monitors.clone(),
        });
        let mut regions: Vec<usize> = self.procs[proc].region.into_iter().collect();
        if abnormal {
            for linked in self.procs[proc].links.clone() {
                regions.extend(self.kill_proc(linked));
            }
        }
        regions
    }

    /// Step 4 of the killed-proc sequence: publish the parked reasons.
    fn publish_pending(&mut self) {
        for exit in std::mem::take(&mut self.pending) {
            for chan in exit.monitors {
                self.chans[chan]
                    .buf
                    .push_back((exit.message.clone(), exit.vc.clone()));
                self.settle_chan(chan);
            }
        }
    }

    /// `[conc.proc.kill]` steps 1 and 4; the returned regions are step 3.
    fn kill_proc(&mut self, proc: usize) -> Vec<usize> {
        if self.procs[proc].exited.is_some() {
            return Vec::new();
        }
        let members: Vec<TaskId> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.proc == proc && t.state != TaskState::Done)
            .map(|(id, _)| id)
            .collect();
        for task in members {
            self.tasks[task].killed = true;
            match self.tasks[task].state {
                TaskState::Blocked => {
                    self.deregister(task);
                    self.make_ready(task, Wake::Killed);
                }
                // A wake already pending is superseded: no further user
                // code runs.
                TaskState::Ready => {
                    self.deregister(task);
                    self.tasks[task].wake = Some(Wake::Killed);
                }
                TaskState::Running | TaskState::Done => {}
            }
        }
        self.deliver_exit(proc, tag("killed"))
    }

    fn all_done(&self, scope: usize) -> bool {
        self.scopes[scope]
            .children
            .iter()
            .all(|child| self.tasks[*child].state == TaskState::Done)
    }

    /// A task's execution finished: record it, settle its scope and its
    /// proc. Returns the regions of an exiting proc, to free.
    fn end_task(&mut self, me: TaskId, end: TaskEnd) -> Vec<usize> {
        let cancelled = self.tasks[me].cancelled;
        let killed = self.tasks[me].killed;
        // A cancelled task ending with an error finished its
        // cancellation (`[conc.task.fail]`: not a new failure); a killed
        // one ran no further user code.
        let end = match end {
            TaskEnd::Error(_) | TaskEnd::Value(_) if killed => TaskEnd::Killed,
            TaskEnd::Error(_) if cancelled => TaskEnd::Cancelled,
            other => other,
        };
        self.tasks[me].state = TaskState::Done;
        self.tasks[me].ctx = None;
        if let Some(scope) = self.tasks[me].scope {
            if matches!(end, TaskEnd::Error(_) | TaskEnd::Trapped(_)) {
                self.scopes[scope].failures.push(end.clone());
                let siblings: Vec<TaskId> = self.scopes[scope]
                    .children
                    .iter()
                    .copied()
                    .filter(|sibling| *sibling != me)
                    .collect();
                for sibling in siblings {
                    self.cancel_task(sibling);
                }
                // `[conc.task.fail.owner]`: an owner blocked inside the
                // scope's extent is cancelled like a sibling; one at
                // the join keeps joining.
                let owner = self.scopes[scope].owner;
                if !self.scopes[scope].joining && self.tasks[owner].state == TaskState::Blocked {
                    self.cancel_task(owner);
                }
            }
            let owner = self.scopes[scope].owner;
            if self.scopes[scope].joining
                && self.all_done(scope)
                && self.tasks[owner].state == TaskState::Blocked
            {
                self.make_ready(owner, Wake::Ok(Value::Unit));
            }
        }
        let proc = self.tasks[me].proc;
        if proc != 0 && self.procs[proc].root == me {
            let reason = match &end {
                TaskEnd::Value(value) => Value::ErrTag {
                    tag: "normal".to_string(),
                    payload: vec![value.clone()],
                },
                TaskEnd::Error(err) => Value::ErrTag {
                    tag: "error".to_string(),
                    payload: vec![err.clone()],
                },
                // `[conc.proc.exit]`: a contained trap is `fault(kind)`.
                TaskEnd::Trapped(trap) => Value::ErrTag {
                    tag: "fault".to_string(),
                    payload: vec![Value::Str(trap.kind.to_string())],
                },
                TaskEnd::Cancelled => tag("cancelled"),
                TaskEnd::Killed => tag("killed"),
            };
            return self.deliver_exit(proc, reason);
        }
        Vec::new()
    }

    fn spawn_task(
        &mut self,
        parent: TaskId,
        scope: Option<usize>,
        proc: usize,
        name: String,
        entry: Entry,
        ambient: usize,
    ) -> TaskId {
        let id = self.tasks.len();
        let parent_vc = self.tasks[parent].vc.clone();
        self.tick(parent);
        self.tasks.push(Tcb {
            name,
            state: TaskState::Ready,
            proc,
            scope,
            gate: None,
            started: false,
            cancelled: false,
            killed: false,
            wake: Some(Wake::Ok(Value::Unit)),
            ctx: Some(TaskCtx {
                ambient: vec![ambient],
                ..TaskCtx::default()
            }),
            vc: parent_vc,
            entry: Some(entry),
        });
        self.tick(id);
        self.concurrent = true;
        if let Some(scope) = scope {
            self.scopes[scope].children.push(id);
        }
        self.ready.push_back(id);
        id
    }
}

// ------------------------------------------------------- the machine side --

/// A task thread's whole life.
fn task_thread<'t>(hub: Arc<Hub<'t>>, gate: Arc<Gate>, id: TaskId) {
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        gate.wait();
        let Some(mut machine) = hub.wake() else {
            return;
        };
        machine.run_task(id);
    }));
    if let Err(payload) = run {
        hub.abandon(payload);
    }
}

impl<'t> Machine<'t> {
    pub(super) fn me(&self) -> TaskId {
        self.sched.current
    }

    fn swap_ctx(&mut self, ctx: &mut TaskCtx<'t>) {
        std::mem::swap(&mut self.frames, &mut ctx.frames);
        std::mem::swap(&mut self.self_tys, &mut ctx.self_tys);
        std::mem::swap(&mut self.frame_rigids, &mut ctx.frame_rigids);
        std::mem::swap(&mut self.pending_self_ty, &mut ctx.pending_self_ty);
        std::mem::swap(&mut self.pending_rigids, &mut ctx.pending_rigids);
        std::mem::swap(&mut self.ambient, &mut ctx.ambient);
        std::mem::swap(&mut self.in_defer, &mut ctx.in_defer);
        std::mem::swap(&mut self.in_atomic, &mut ctx.in_atomic);
        std::mem::swap(&mut self.last_os_error, &mut ctx.last_os_error);
        std::mem::swap(&mut self.when_held, &mut ctx.when_held);
    }

    fn save_ctx(&mut self, task: TaskId) {
        let mut ctx = TaskCtx::default();
        self.swap_ctx(&mut ctx);
        self.sched.tasks[task].ctx = Some(ctx);
    }

    fn load_ctx(&mut self, task: TaskId) {
        let mut ctx = self.sched.tasks[task].ctx.take().unwrap_or_default();
        self.swap_ctx(&mut ctx);
    }

    fn gate_of(&mut self, task: TaskId) -> E<Arc<Gate>> {
        if let Some(gate) = &self.sched.tasks[task].gate {
            return Ok(gate.clone());
        }
        let Some(hub) = &self.hub else {
            return self.no_threads();
        };
        let gate = hub.gate();
        self.sched.tasks[task].gate = Some(gate.clone());
        Ok(gate)
    }

    fn no_threads<T>(&self) -> E<T> {
        self.refuse(
            "a second task on a host with no thread to give it",
            self.pkg.files[0].parse.root.span,
        )
    }

    /// Makes `task`'s thread, parked at its gate, if it has none yet.
    fn start_thread(&mut self, task: TaskId) -> E<()> {
        if self.sched.tasks[task].started {
            return Ok(());
        }
        let gate = self.gate_of(task)?;
        let (Some(hub), Some(spawner)) = (self.hub.clone(), self.spawner.as_ref()) else {
            return self.no_threads();
        };
        let (reply, answer) = mpsc::sync_channel(1);
        let name = format!("wolf-checked-{}", self.sched.tasks[task].name);
        let request = SpawnReq {
            name,
            run: Box::new(move || task_thread(hub, gate, task)),
            reply,
        };
        if spawner.send(request).is_err() || answer.recv() != Ok(true) {
            return self.no_threads();
        }
        self.sched.tasks[task].started = true;
        Ok(())
    }

    /// Hands the machine to `next` and sleeps until it comes back.
    fn switch_to(&mut self, next: TaskId) -> E<()> {
        let me = self.me();
        self.start_thread(next)?;
        let my_gate = self.gate_of(me)?;
        let next_gate = self.gate_of(next)?;
        let Some(hub) = self.hub.clone() else {
            return self.no_threads();
        };
        self.save_ctx(me);
        self.load_ctx(next);
        self.sched.current = next;
        self.sched.tasks[next].state = TaskState::Running;
        let real = std::mem::replace(self, Machine::blank(self.pkg, self.tc));
        hub.rest(real);
        next_gate.open();
        my_gate.wait();
        match hub.wake() {
            Some(machine) => {
                *self = machine;
                Ok(())
            }
            // A thread of this run panicked: unwind this one too,
            // quietly; the run's caller re-raises the first panic.
            None => std::panic::resume_unwind(Box::new(Abandoned)),
        }
    }

    /// Gives the machine away for good: this task is done.
    fn give(&mut self, mut next: TaskId) {
        if self.start_thread(next).is_err() {
            // No thread for the next task: the run cannot go on. The
            // root always has its thread, and raises the refusal.
            self.sched.fatal.get_or_insert(Stop::Refuse(NotYet {
                construct: "a second task on a host with no thread to give it",
                span: self.pkg.files[0].parse.root.span,
            }));
            self.sched.shutdown = true;
            next = 0;
        }
        let (Some(hub), Ok(next_gate)) = (self.hub.clone(), self.gate_of(next)) else {
            return;
        };
        let mut gone = TaskCtx::default();
        self.swap_ctx(&mut gone);
        self.load_ctx(next);
        self.sched.current = next;
        self.sched.tasks[next].state = TaskState::Running;
        let real = std::mem::replace(self, Machine::blank(self.pkg, self.tc));
        hub.rest(real);
        next_gate.open();
    }

    fn pick_next(&mut self) -> Option<TaskId> {
        match self.sched.choose_next() {
            Some(next) => Some(next),
            None => {
                self.sched.declare_deadlock();
                self.sched.choose_next()
            }
        }
    }

    /// Parks the running task until a decision picks it again.
    fn park(&mut self) -> E<Wake> {
        let me = self.me();
        if self.sched.tasks[me].wake.is_none() {
            self.sched.tasks[me].state = TaskState::Blocked;
        } else {
            self.sched.tasks[me].state = TaskState::Ready;
            self.sched.ready.push_back(me);
        }
        let next = self
            .pick_next()
            .expect("a parked task is woken by the deadlock it completes");
        if next == me {
            self.sched.tasks[me].state = TaskState::Running;
        } else {
            self.switch_to(next)?;
        }
        if me == 0
            && let Some(stop) = self.sched.fatal.take()
        {
            return Err(stop);
        }
        Ok(self.sched.tasks[me]
            .wake
            .take()
            .expect("a scheduled task carries its wake reason"))
    }

    /// The running task's entry to a blocking point: a kill or a
    /// cancellation already requested is delivered here.
    fn blocking_point(&mut self) -> E<Option<Flow>> {
        let me = self.me();
        if self.sched.tasks[me].killed {
            return Err(Stop::Killed);
        }
        if self.sched.tasks[me].cancelled {
            return Ok(Some(raise(tag("cancelled"))));
        }
        Ok(None)
    }

    /// `[conc.deadlock.trap]`: the roster goes to stderr (its existence
    /// is contract, its contents implementation-specified).
    fn deadlock_trap<T>(&mut self, roster: &str, span: Span) -> E<T> {
        self.stderr.extend_from_slice(
            format!(
                "wolf: deadlock: every live task is blocked and no timer is pending: {roster}\n"
            )
            .as_bytes(),
        );
        self.trap("deadlock", "conc.deadlock.trap", span)
    }

    /// A wake that is not the operation's own answer.
    fn wake_stop<T>(&mut self, wake: Wake, span: Span) -> E<T> {
        match wake {
            Wake::Killed => Err(Stop::Killed),
            Wake::Deadlock(roster) => self.deadlock_trap(&roster, span),
            _ => self.refuse("a wake this blocking point does not take", span),
        }
    }

    // -- a task's life ----------------------------------------------------

    fn run_task(&mut self, id: TaskId) {
        let wake = self.sched.tasks[id].wake.take();
        let killed = matches!(wake, Some(Wake::Killed)) || self.sched.tasks[id].killed;
        let result = if killed {
            Err(Stop::Killed)
        } else {
            match self.sched.tasks[id].entry.take() {
                Some(Entry::Closure(cid)) => self.call_closure(cid, Vec::new(), true),
                Some(Entry::Call { body, args }) => self.call_body(body, args),
                Some(Entry::ParChunk { job, lo, hi }) => self.par_chunk(job, lo, hi),
                None => Ok(Value::Unit),
            }
        };
        let end = match result {
            Ok(v @ Value::ErrTag { .. }) => TaskEnd::Error(v),
            Ok(v) => TaskEnd::Value(v),
            Err(Stop::Trap(trap)) => TaskEnd::Trapped(trap),
            Err(Stop::Killed) => TaskEnd::Killed,
            // A UB finding, a refusal, a budget or `os_exit`: the run
            // ends with it, wherever it was raised. The root is parked
            // and takes it from here.
            Err(stop) => {
                self.sched.fatal.get_or_insert(stop);
                self.sched.shutdown = true;
                self.sched.tasks[id].state = TaskState::Done;
                self.give(0);
                return;
            }
        };
        let regions = self.sched.end_task(id, end);
        self.free_proc_regions(regions);
        let next = if self.sched.shutdown {
            Some(0)
        } else {
            self.pick_next()
        };
        // Nothing left to run can only mean the root is done, and then
        // the root is the one tearing down — it is parked, waiting.
        self.give(next.unwrap_or(0));
    }

    /// `[conc.proc.kill]` step 3, then step 4: the regions bulk-free and
    /// only then do the parked reasons publish (`[mem.region.cap.3]`).
    fn free_proc_regions(&mut self, regions: Vec<usize>) {
        for rid in regions {
            if self.regions[rid].live {
                self.free_region(rid);
            }
        }
        self.sched.publish_pending();
    }

    /// The end of the run: every task that still has a thread is woken
    /// as killed, one at a time, and unwinds without user code
    /// (`[conc.task.root]`: the root supervisor reaps what is left).
    pub(super) fn teardown(&mut self) {
        self.sched.shutdown = true;
        self.sched.fatal = None;
        let mut id = 1;
        while id < self.sched.tasks.len() {
            let tcb = &mut self.sched.tasks[id];
            if tcb.state != TaskState::Done {
                if tcb.started {
                    tcb.killed = true;
                    tcb.wake = Some(Wake::Killed);
                    self.sched.ready.retain(|t| *t != id);
                    if self.switch_to(id).is_err() {
                        return;
                    }
                } else {
                    tcb.state = TaskState::Done;
                }
            }
            id += 1;
        }
    }

    // -- closures -----------------------------------------------------------

    /// A closure expression: the body, and every enclosing binding it
    /// names, copied now (`[gram.expr.closure]`).
    pub(super) fn eval_closure(&mut self, e: &'t GreenNode) -> E<Flow> {
        fn idents<'a>(node: &'a GreenNode, out: &mut Vec<&'a wolf_ast::GreenToken>) {
            out.extend(node.tokens().filter(|t| t.kind == SyntaxKind::Ident));
            for child in node.nodes() {
                idents(child, out);
            }
        }
        let mut toks = Vec::new();
        idents(e, &mut toks);
        let mut captures: Vec<(String, Value)> = Vec::new();
        for tok in toks {
            let name = self.text(tok.span);
            if captures.iter().any(|(n, _)| *n == name) {
                continue;
            }
            let Some((frame, local)) = self.lookup(&name) else {
                continue;
            };
            let v = match self.frames[frame].locals[local].clone() {
                // A `mut` parameter aliases its caller's place: the
                // closure copies what is there now.
                Value::Ref(place) => self.read_place(&place, tok.span)?,
                v => v,
            };
            captures.push((name, v));
        }
        self.charge_mem(16 + 8 * captures.len() as u64)?;
        let body = self.frames.last().expect("frame").body;
        self.closures.push(ClosureInst {
            body,
            node: e,
            captures,
            self_ty: self.self_tys.last().cloned().flatten(),
            rigids: self.frame_rigids.last().cloned().unwrap_or_default(),
        });
        Ok(Flow::Val(Value::Closure(self.closures.len() - 1)))
    }

    /// Calls a closure: a frame over the body's own typed tables, the
    /// captures and then the parameters bound in it. `owns` is a
    /// spawned task's call: a captured region moved into the task
    /// (`[conc.task.spawn]`), so the task frees it at its end.
    pub(super) fn call_closure(&mut self, cid: usize, args: Vec<Value>, owns: bool) -> E<Value> {
        if self.frames.len() > CALL_DEPTH_BUDGET {
            return Err(Stop::Budget("call depth budget exhausted"));
        }
        let inst = &self.closures[cid];
        let node = inst.node;
        let body = inst.body;
        let d = wolf_ast::ClosureExpr::cast(node).expect("closure node");
        let mut frame = Frame {
            body,
            locals: Vec::new(),
            scopes: vec![Scope {
                names: Vec::new(),
                cleanup: Vec::new(),
            }],
        };
        for (name, v) in &inst.captures {
            if owns && matches!(v, Value::Region(_)) {
                frame.scopes[0]
                    .cleanup
                    .push(Cleanup::FreeRegionLocal(frame.locals.len()));
            }
            frame.scopes[0]
                .names
                .push((name.clone(), frame.locals.len()));
            frame.locals.push(v.clone());
        }
        let self_ty = inst.self_ty.clone();
        let rigids = inst.rigids.clone();
        let src = &self.pkg.files[self.tc.bodies[body].body.file].raw.src;
        let names: Vec<String> = d
            .params()
            .into_iter()
            .flat_map(|p| p.params())
            .map(|p| match p.name() {
                Some(n) => String::from_utf8_lossy(&src[n.span.lo as usize..n.span.hi as usize])
                    .into_owned(),
                None => "_".to_string(),
            })
            .collect();
        for (i, v) in args.into_iter().enumerate() {
            let name = names.get(i).cloned().unwrap_or_else(|| format!("_{i}"));
            frame.scopes[0].names.push((name, frame.locals.len()));
            frame.locals.push(v);
        }
        self.frames.push(frame);
        self.self_tys.push(self_ty);
        self.frame_rigids.push(rigids);
        let result = match d.body() {
            Some(b) => match AstBlock::cast(b) {
                Some(block) => self.eval_block(block, true),
                None => self.eval(b),
            },
            None => Ok(Flow::Val(Value::Unit)),
        };
        let out = match result {
            Ok(Flow::Val(v)) | Ok(Flow::Return(v)) => self.exit_scopes_to(0, false).map(|()| v),
            Ok(Flow::Err(v, _)) => self.exit_scopes_to(0, true).map(|()| v),
            Ok(Flow::Break) | Ok(Flow::Continue) => Ok(Value::Unit),
            Err(e) => Err(e),
        };
        self.frames.pop();
        self.self_tys.pop();
        self.frame_rigids.pop();
        out
    }

    /// Calls a fn VALUE — a top-level fn or a closure — with evaluated
    /// arguments. A raised row comes back as its `ErrTag`.
    pub(super) fn call_fn_value(&mut self, f: &Value, args: Vec<Value>, span: Span) -> E<Value> {
        match f {
            Value::Fn(body) => self.call_body(*body, args),
            Value::Closure(cid) => self.call_closure(*cid, args, false),
            _ => self.refuse("a call through a value that is not a fn", span),
        }
    }

    // -- scopes and spawns ---------------------------------------------------

    /// `scope name? { … }` (`[conc.task.scope]`, `[conc.task.join]`,
    /// `[conc.task.fail]`).
    pub(super) fn eval_scope(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = wolf_ast::ScopeExpr::cast(e).expect("kind");
        let me = self.me();
        self.sched.scopes.push(ScopeState {
            owner: me,
            ..ScopeState::default()
        });
        let sid = self.sched.scopes.len() - 1;
        self.push_scope();
        if let Some(tok) = d.name() {
            let name = self.text(tok.span);
            self.declare(&name, Value::TaskScope(sid));
        }
        let body = match d.body() {
            Some(b) => self.eval_block(b, true),
            None => Ok(Flow::Val(Value::Unit)),
        };
        let body = match body {
            // The owner trapped inside its scope: structured teardown
            // still joins the children, then the trap goes on.
            Err(Stop::Trap(trap)) => {
                self.cancel_scope(sid);
                self.join_scope(sid, e.span)?;
                return Err(Stop::Trap(trap));
            }
            Err(stop) => return Err(stop),
            Ok(flow) => flow,
        };
        // The owner is leaving early (a `return`, a `?`, a `break`):
        // the children are cancelled before the join, so it terminates.
        if !matches!(body, Flow::Val(_) | Flow::Err(_, false)) {
            self.cancel_scope(sid);
        }
        let failures = self.join_scope(sid, e.span)?;
        self.close_scope(matches!(body, Flow::Err(..)))?;
        // `[conc.task.fail.owner]`: an owner cancelled with its failed
        // child finished its cancellation; the child's failure is what
        // re-raises.
        let cancelled = |v: &Value| matches!(v, Value::ErrTag { tag, .. } if tag == "cancelled");
        let body = match body {
            Flow::Err(ref v, _) | Flow::Return(ref v) if cancelled(v) && !failures.is_empty() => {
                Flow::Val(Value::Unit)
            }
            other => other,
        };
        if !matches!(body, Flow::Val(_) | Flow::Err(_, false)) {
            return Ok(body);
        }
        match failures.into_iter().next() {
            None => Ok(body),
            // The first failure in schedule order re-raises at the
            // scope's exit, into the enclosing function's row.
            Some(TaskEnd::Error(v)) => Ok(Flow::Err(v, true)),
            Some(TaskEnd::Trapped(trap)) => Err(Stop::Trap(trap)),
            Some(_) => Ok(body),
        }
    }

    fn cancel_scope(&mut self, sid: usize) {
        for child in self.sched.scopes[sid].children.clone() {
            self.sched.cancel_task(child);
        }
    }

    /// The scope-exit join: blocks until every child is done, then
    /// answers the failures in schedule order.
    fn join_scope(&mut self, sid: usize, span: Span) -> E<Vec<TaskEnd>> {
        let me = self.me();
        loop {
            if self.sched.tasks[me].killed {
                return Err(Stop::Killed);
            }
            if self.sched.all_done(sid) {
                self.sched.scopes[sid].joining = false;
                for child in self.sched.scopes[sid].children.clone() {
                    let vc = self.sched.tasks[child].vc.clone();
                    self.sched.hb_edge(&vc, me);
                }
                return Ok(std::mem::take(&mut self.sched.scopes[sid].failures));
            }
            self.sched.scopes[sid].joining = true;
            match self.park()? {
                // Cancellation cannot skip a join: the children were
                // cancelled with the owner, so this terminates.
                Wake::Ok(_) | Wake::Cancelled => {}
                other => return self.wake_stop(other, span),
            }
        }
    }

    /// The region a new task's allocations land in: its proc's own
    /// region, or the run's root (`[conc.task.par.cost]`).
    fn task_ambient(&self, proc: usize) -> usize {
        self.sched.procs[proc].region.unwrap_or(0)
    }

    /// `s.spawn(f)` (`[conc.task.spawn]`): the task is made ready and
    /// the spawner keeps running.
    pub(super) fn eval_spawn_task(
        &mut self,
        recv: &'t GreenNode,
        e: &'t GreenNode,
        args: Option<wolf_ast::ArgList<'t>>,
    ) -> E<Flow> {
        let scope = match found!(self.place_of(recv)) {
            Some(place) => self.read_place(&place, recv.span)?,
            None => val!(self.eval(recv)),
        };
        let Value::TaskScope(sid) = scope else {
            return self.refuse("a spawn on a value that is not a scope", e.span);
        };
        let Some(arg) = args.into_iter().flat_map(|l| l.args()).find_map(Arg::value) else {
            return self.refuse("a spawn without a task", e.span);
        };
        let f = val!(self.eval(arg));
        let entry = match f {
            Value::Closure(cid) => {
                let d = wolf_ast::ClosureExpr::cast(self.closures[cid].node).expect("closure");
                if d.params().into_iter().flat_map(|p| p.params()).count() != 0 {
                    return self.refuse("a spawned closure with parameters", arg.span);
                }
                // A captured region moves into the task (D14): the
                // spawner's binding is spent.
                if arg.kind == SyntaxKind::ClosureExpr {
                    let moved: Vec<String> = self.closures[cid]
                        .captures
                        .iter()
                        .filter(
                            |(_, v)| matches!(v, Value::Region(rid) if !self.regions[*rid].frozen),
                        )
                        .map(|(n, _)| n.clone())
                        .collect();
                    for name in moved {
                        if let Some((frame, local)) = self.lookup(&name)
                            && matches!(self.frames[frame].locals[local], Value::Region(_))
                        {
                            self.frames[frame].locals[local] = Value::Moved;
                        }
                    }
                }
                Entry::Closure(cid)
            }
            Value::Fn(body) => Entry::Call {
                body,
                args: Vec::new(),
            },
            _ => return self.refuse("a spawn of a value that is not a fn", arg.span),
        };
        let me = self.me();
        let proc = self.sched.tasks[me].proc;
        let ambient = self.task_ambient(proc);
        self.charge_mem(64)?;
        self.sched.spawn_task(
            me,
            Some(sid),
            proc,
            format!("task@{}", e.span.lo),
            entry,
            ambient,
        );
        Ok(Flow::Val(Value::Unit))
    }

    // -- par -------------------------------------------------------------------

    /// `xs.par(f)` (`[conc.task.par]`): `k = min(n, W)` contiguous
    /// chunks, one task each under a scope of their own, one join, the
    /// result in input order.
    pub(super) fn eval_par(&mut self, list: usize, f: Value, span: Span) -> E<Flow> {
        if let Value::Closure(cid) = &f
            && self.closures[*cid]
                .captures
                .iter()
                .any(|(_, v)| matches!(v, Value::Region(_)))
        {
            return self.refuse(
                "a `par` whose fn captures a region value ([conc.task.par.capture])",
                span,
            );
        }
        let items = self.lists[list].clone();
        let n = items.len();
        let k = n.min(PAR_WORKERS);
        let mut out = Vec::with_capacity(n);
        if k <= 1 {
            // One chunk runs on the calling task (`[conc.task.par.chunk]`).
            for item in items {
                match self.call_fn_value(&f, vec![item], span)? {
                    v @ Value::ErrTag { .. } => return Ok(raise(v)),
                    v => out.push(v),
                }
            }
        } else {
            let me = self.me();
            self.sched.scopes.push(ScopeState {
                owner: me,
                ..ScopeState::default()
            });
            let sid = self.sched.scopes.len() - 1;
            self.sched.par_jobs.push(ParJob {
                f,
                items,
                slots: vec![None; n],
                span,
            });
            let job = self.sched.par_jobs.len() - 1;
            let proc = self.sched.tasks[me].proc;
            let ambient = self.task_ambient(proc);
            let (base, extra) = (n / k, n % k);
            let mut lo = 0;
            for chunk in 0..k {
                let hi = lo + base + usize::from(chunk < extra);
                self.sched.spawn_task(
                    me,
                    Some(sid),
                    proc,
                    format!("par@{}#{chunk}", span.lo),
                    Entry::ParChunk { job, lo, hi },
                    ambient,
                );
                lo = hi;
            }
            let failures = self.join_scope(sid, span)?;
            let slots = std::mem::take(&mut self.sched.par_jobs[job].slots);
            self.sched.par_jobs[job].items = Vec::new();
            match failures.into_iter().next() {
                Some(TaskEnd::Error(v)) => return Ok(raise(v)),
                Some(TaskEnd::Trapped(trap)) => return Err(Stop::Trap(trap)),
                _ => {}
            }
            for slot in slots {
                match slot {
                    Some(v) => out.push(v),
                    None => return self.refuse("a `par` slot never filled", span),
                }
            }
        }
        let id = self.mint_list(out, span)?;
        Ok(Flow::Val(Value::List(id)))
    }

    /// One chunk: `for i in lo..hi { out[i] = f(xs[i])? }`.
    fn par_chunk(&mut self, job: usize, lo: usize, hi: usize) -> E<Value> {
        let f = self.sched.par_jobs[job].f.clone();
        let span = self.sched.par_jobs[job].span;
        // The chunk's frame: `call_fn_value` reads its caller's depth.
        for index in lo..hi {
            self.tick()?;
            let item = self.sched.par_jobs[job].items[index].clone();
            match self.call_fn_value(&f, vec![item], span)? {
                v @ Value::ErrTag { .. } => return Ok(v),
                v => self.sched.par_jobs[job].slots[index] = Some(v),
            }
        }
        Ok(Value::Unit)
    }

    // -- channels ----------------------------------------------------------------

    /// `ch.send(v)`: blocks on a full buffer or an unmet rendezvous
    /// (`[conc.chan.buf]`).
    pub(super) fn chan_send(&mut self, chan: usize, value: Value, span: Span) -> E<Flow> {
        if let Some(flow) = self.blocking_point()? {
            return Ok(flow);
        }
        let me = self.me();
        if self.sched.chans[chan].closed {
            return Ok(raise(tag("closed")));
        }
        self.sched.chans[chan].senders.push_back((me, value));
        self.sched.settle_chan(chan);
        let wake = match self.sched.tasks[me].wake.take() {
            Some(wake) => wake,
            None => self.park()?,
        };
        match wake {
            Wake::Ok(_) => Ok(Flow::Val(Value::Unit)),
            Wake::Closed => Ok(raise(tag("closed"))),
            Wake::Cancelled => Ok(raise(tag("cancelled"))),
            other => self.wake_stop(other, span),
        }
    }

    /// `ch.recv()`: blocks on an empty open channel.
    pub(super) fn chan_recv(&mut self, chan: usize, span: Span) -> E<Flow> {
        if let Some(flow) = self.blocking_point()? {
            return Ok(flow);
        }
        let me = self.me();
        self.sched.chans[chan].receivers.push_back(me);
        self.sched.settle_chan(chan);
        let wake = match self.sched.tasks[me].wake.take() {
            Some(wake) => wake,
            None => self.park()?,
        };
        match wake {
            Wake::Ok(v) => Ok(Flow::Val(v)),
            Wake::Closed => Ok(raise(tag("closed"))),
            Wake::Cancelled => Ok(raise(tag("cancelled"))),
            other => self.wake_stop(other, span),
        }
    }

    /// `ch.close()` (`[conc.chan.close]`).
    pub(super) fn chan_close(&mut self, chan: usize) {
        if self.sched.chans[chan].closed {
            return;
        }
        self.sched.chans[chan].closed = true;
        let senders = std::mem::take(&mut self.sched.chans[chan].senders);
        for (sender, _) in senders {
            self.sched.make_ready(sender, Wake::Closed);
        }
        self.sched.settle_chan(chan);
    }

    // -- select ----------------------------------------------------------------------

    /// `select { … }` (`[conc.select.ready]`, `[conc.select.fair]`,
    /// `[conc.select.timeout]`, `[conc.select.closed]`).
    pub(super) fn eval_select(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = wolf_ast::SelectExpr::cast(e).expect("kind");
        let arms: Vec<wolf_ast::SelectArm<'t>> = d.arms().collect();
        let mut chan_arms: Vec<(usize, usize)> = Vec::new();
        let mut timeout: Option<(usize, u128)> = None;
        for (index, arm) in arms.iter().enumerate() {
            let body = arm.body();
            let head = arm
                .syntax()
                .nodes()
                .filter(|n| wolf_ast::is_expr_kind(n.kind))
                .find(|n| body.is_none_or(|b| !std::ptr::eq(*n, b)));
            let Some(head) = head else {
                return self.refuse("a `select` arm without its source", arm.syntax().span);
            };
            if arm.is_timeout() {
                let Value::Int(nanos) = val!(self.eval(head)) else {
                    return self.refuse("a `timeout` that is not a duration", head.span);
                };
                let deadline = self.sched.clock.saturating_add(nanos.max(0) as u128);
                // Of several timeouts the earliest is the one that fires.
                if timeout.is_none_or(|(_, existing)| deadline < existing) {
                    timeout = Some((index, deadline));
                }
            } else {
                let source = match found!(self.place_of(head)) {
                    Some(place) => self.read_place(&place, head.span)?,
                    None => val!(self.eval(head)),
                };
                let Value::Chan(chan) = source else {
                    return self.refuse("a `select` arm over a non-channel", head.span);
                };
                chan_arms.push((index, chan));
            }
        }
        if let Some(flow) = self.blocking_point()? {
            // Cancellation at the `select`: the error value leaves the
            // function by ordinary return (`[conc.cancel.defer]`).
            return Ok(match flow {
                Flow::Err(v, _) => Flow::Err(v, true),
                other => other,
            });
        }
        let me = self.me();
        let mut ready: Vec<usize> = Vec::new();
        for (arm, chan) in &chan_arms {
            let c = &self.sched.chans[*chan];
            if !c.buf.is_empty() || !c.senders.is_empty() || c.closed {
                ready.push(*arm);
            }
        }
        if let Some((arm, deadline)) = timeout
            && self.sched.clock >= deadline
        {
            ready.push(arm);
        }
        let (arm, value) = if !ready.is_empty() {
            let arm = ready[self.sched.decide(ready.len())];
            match chan_arms.iter().find(|(a, _)| *a == arm) {
                None => (arm, None),
                Some((_, chan)) => {
                    let c = &self.sched.chans[*chan];
                    if c.buf.is_empty() && c.senders.is_empty() {
                        (arm, Some(tag("closed")))
                    } else {
                        (arm, Some(self.sched.take_message(*chan, me)))
                    }
                }
            }
        } else {
            for (arm, chan) in &chan_arms {
                self.sched.chans[*chan].selecters.push((me, *arm));
            }
            if let Some((arm, deadline)) = timeout {
                let seq = self.sched.timer_seq;
                self.sched.timer_seq += 1;
                self.sched.timers.push(Timer {
                    deadline,
                    seq,
                    task: me,
                    arm,
                });
            }
            match self.park()? {
                Wake::Arm(arm, value) => (arm, value),
                Wake::Cancelled => return Ok(Flow::Err(tag("cancelled"), true)),
                other => return self.wake_stop(other, e.span),
            }
        };
        let arm = arms[arm];
        self.push_scope();
        if let (Some(pat), Some(value)) = (arm.pattern(), value) {
            match pat.kind {
                SyntaxKind::WildcardPat => {}
                SyntaxKind::IdentPat => self.bind_pattern(pat, value)?,
                // `exit(reason) from m`: the reason binds.
                _ => {
                    if let Some(inner) = pat.nodes().find(|n| n.kind == SyntaxKind::IdentPat) {
                        self.bind_pattern(inner, value)?;
                    }
                }
            }
        }
        let out = match arm.body() {
            Some(b) => self.eval(b)?,
            None => Flow::Val(Value::Unit),
        };
        self.close_scope(matches!(out, Flow::Err(..)))?;
        Ok(match out {
            // A `select` is a statement shape and types as unit.
            Flow::Val(_) => Flow::Val(Value::Unit),
            other => other,
        })
    }

    // -- Mutex and `when` -------------------------------------------------------------

    pub(super) fn new_mutex(&mut self, value: Value) -> Value {
        self.sched.mutexes.push(Mx {
            value,
            holder: None,
            waiters: VecDeque::new(),
            vc: Vec::new(),
        });
        Value::Mutex(self.sched.mutexes.len() - 1)
    }

    /// Acquires one mutex of a `when` set: the payload, or the flow a
    /// cancellation delivered here raises.
    fn acquire(&mut self, mx: usize, span: Span) -> E<Result<Value, Flow>> {
        if let Some(flow) = self.blocking_point()? {
            return Ok(Err(flow));
        }
        let me = self.me();
        if self.sched.mutexes[mx].holder.is_none() {
            self.sched.mutexes[mx].holder = Some(me);
            let vc = self.sched.mutexes[mx].vc.clone();
            self.sched.hb_edge(&vc, me);
            return Ok(Ok(self.sched.mutexes[mx].value.clone()));
        }
        self.sched.mutexes[mx].waiters.push_back(me);
        match self.park()? {
            Wake::Ok(value) => Ok(Ok(value)),
            Wake::Cancelled => Ok(Err(raise(tag("cancelled")))),
            other => self.wake_stop(other, span),
        }
    }

    /// Releases a mutex, writing the payload back: the release
    /// happens-before the next acquisition (`[conc.mm.hb.mutex]`).
    fn release(&mut self, mx: usize, value: Value) {
        let me = self.me();
        self.sched.mutexes[mx].value = value;
        self.sched.mutexes[mx].holder = None;
        let vc = self.sched.tasks[me].vc.clone();
        merge_vc(&mut self.sched.mutexes[mx].vc, &vc);
        self.sched.tick(me);
        if let Some(next) = self.sched.mutexes[mx].waiters.pop_front() {
            self.sched.mutexes[mx].holder = Some(next);
            let vc = self.sched.mutexes[mx].vc.clone();
            self.sched.hb_edge(&vc, next);
            let value = self.sched.mutexes[mx].value.clone();
            self.sched.make_ready(next, Wake::Ok(value));
        }
    }

    /// `when (a, b, …) { … }` (`[conc.when.order]`, `[conc.when.body]`,
    /// `[conc.deadlock.self]`).
    pub(super) fn eval_when(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = wolf_ast::WhenExpr::cast(e).expect("kind");
        let mut set: Vec<(usize, Option<String>)> = Vec::new();
        for operand in d.operands() {
            let name = (operand.kind == SyntaxKind::PathExpr)
                .then(|| self.text(operand.span))
                .filter(|n| !n.contains('.'));
            let value = match found!(self.place_of(operand)) {
                Some(place) => self.read_place(&place, operand.span)?,
                None => val!(self.eval(operand)),
            };
            let Value::Mutex(id) = value else {
                return self.refuse("a `when` operand that is not a `Mutex`", operand.span);
            };
            set.push((id, name));
        }
        // The canonical order: creation order, at every site.
        set.sort_by_key(|(id, _)| *id);
        set.dedup_by_key(|(id, _)| *id);
        if set.iter().any(|(id, _)| self.when_held.contains(id)) {
            return self.trap("deadlock", "conc.deadlock.self", e.span);
        }
        let mut acquired: Vec<(usize, Option<String>, Value)> = Vec::new();
        for (id, name) in set {
            let got = match self.acquire(id, e.span) {
                Ok(got) => got,
                Err(stop) => {
                    for (id, _, payload) in acquired.into_iter().rev() {
                        self.release(id, payload);
                    }
                    return Err(stop);
                }
            };
            match got {
                Ok(payload) => acquired.push((id, name, payload)),
                Err(flow) => {
                    // Cancelled mid-set: hand back what was taken.
                    for (id, _, payload) in acquired.into_iter().rev() {
                        self.release(id, payload);
                    }
                    return Ok(match flow {
                        Flow::Err(v, _) => Flow::Err(v, true),
                        other => other,
                    });
                }
            }
        }
        for (id, _, _) in &acquired {
            self.when_held.push(*id);
        }
        self.push_scope();
        let mut slots: Vec<Option<usize>> = Vec::new();
        for (_, name, payload) in &acquired {
            slots.push(
                name.as_ref()
                    .map(|name| self.declare(name, payload.clone())),
            );
        }
        let frame = self.frames.len() - 1;
        let result = match d.body() {
            Some(b) => self.eval_block(b, true),
            None => Ok(Flow::Val(Value::Unit)),
        };
        // Write back and release in reverse canonical order.
        for ((id, _, payload), slot) in acquired.into_iter().zip(slots).rev() {
            let value = match slot {
                Some(slot) if result.is_ok() => self.frames[frame].locals[slot].clone(),
                _ => payload,
            };
            if let Some(at) = self.when_held.iter().rposition(|held| *held == id) {
                self.when_held.remove(at);
            }
            self.release(id, value);
        }
        let out = result?;
        self.close_scope(matches!(out, Flow::Err(..)))?;
        Ok(out)
    }

    // -- procs --------------------------------------------------------------------------

    /// `spawn proc f(args)` (`[conc.proc.1]`, `[conc.task.root]`): a
    /// failure domain under the root supervisor, owning a fresh region.
    pub(super) fn eval_spawn_proc(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = wolf_ast::SpawnExpr::cast(e).expect("kind");
        let Some(sig) = self.ctx().calls.get(&e.span).copied() else {
            return self.refuse("a proc spawn without a call record", e.span);
        };
        let module = self.tc.bodies[self.frames.last().expect("frame").body]
            .body
            .module;
        let body = sig
            .decl_span
            .and_then(|ds| self.fns_by_decl.get(&ds))
            .or_else(|| self.fns.get(&(module, sig.callee.clone())))
            .copied();
        let Some(body) = body else {
            return self.refuse("a proc spawn of an unresolvable callee", e.span);
        };
        let mut args = Vec::new();
        for (i, a) in d.args().into_iter().flat_map(|l| l.args()).enumerate() {
            let Some(v) = Arg::value(a) else { continue };
            // An argument crosses into a domain that outlives the
            // spawner: moved, except frozen data, which is shared
            // (`[conc.proc.arg]`).
            let mode = sig.params.get(i).and_then(|p| p.mode);
            args.push(val!(self.eval_arg(v, mode)));
        }
        self.charge_mem(64)?;
        let region = self.regions.len();
        self.regions.push(DynRegion {
            live: true,
            frozen: false,
            backing: None,
            span: e.span,
            charged: 0,
            cap: None,
        });
        let me = self.me();
        let proc = self.sched.procs.len();
        self.sched.procs.push(Proc {
            root: 0,
            region: Some(region),
            monitors: Vec::new(),
            links: Vec::new(),
            exited: None,
        });
        let root = self.sched.spawn_task(
            me,
            None,
            proc,
            format!("proc:{}", sig.callee),
            Entry::Call { body, args },
            region,
        );
        self.sched.procs[proc].root = root;
        Ok(Flow::Val(Value::Proc(proc)))
    }

    /// `w.monitor()`: a mailbox the exit reason arrives on; an
    /// already-exited proc delivers at once (`[conc.proc.2]`).
    fn proc_monitor(&mut self, proc: usize) -> usize {
        let chan = self.sched.new_chan(usize::MAX);
        self.sched.procs[proc].monitors.push(chan);
        if let Some(reason) = self.sched.procs[proc].exited.clone() {
            let root = self.sched.procs[proc].root;
            let vc = self.sched.tasks[root].vc.clone();
            self.sched.chans[chan].buf.push_back((reason, vc));
        }
        chan
    }

    /// The methods of a `Proc[T]` handle.
    pub(super) fn eval_proc_method(
        &mut self,
        method: &str,
        recv: &'t GreenNode,
        e: &'t GreenNode,
        args: Option<wolf_ast::ArgList<'t>>,
    ) -> E<Flow> {
        let handle = match found!(self.place_of(recv)) {
            Some(place) => self.read_place(&place, recv.span)?,
            None => val!(self.eval(recv)),
        };
        let Value::Proc(proc) = handle else {
            return self.refuse("a proc method on a value that is not a proc", e.span);
        };
        match method {
            "monitor" => Ok(Flow::Val(Value::Chan(self.proc_monitor(proc)))),
            // `[conc.proc.join]`: the same reason a monitor carries,
            // read synchronously — a monitor attached at the join.
            "join" => {
                let chan = self.proc_monitor(proc);
                let reason = match self.chan_recv(chan, e.span)? {
                    Flow::Val(v) => v,
                    other => return Ok(other),
                };
                match reason {
                    Value::ErrTag { tag, mut payload } if tag == "normal" => {
                        Ok(Flow::Val(payload.pop().unwrap_or(Value::Unit)))
                    }
                    // The row is payload-free at v1.
                    Value::ErrTag { tag, .. } => Ok(raise(Value::ErrTag {
                        tag,
                        payload: Vec::new(),
                    })),
                    _ => self.refuse("a join that received no exit reason", e.span),
                }
            }
            "kill" => {
                let regions = self.sched.kill_proc(proc);
                self.free_proc_regions(regions);
                self.root_death()?;
                Ok(Flow::Val(Value::Unit))
            }
            // `[conc.proc.cancel]`: cooperative, at blocking points;
            // defers run.
            "cancel" => {
                let members: Vec<TaskId> = self
                    .sched
                    .tasks
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.proc == proc && t.state != TaskState::Done)
                    .map(|(id, _)| id)
                    .collect();
                for task in members {
                    self.sched.cancel_task(task);
                }
                Ok(Flow::Val(Value::Unit))
            }
            // `a.link(b)`; `w.link()` is `w.link(<the caller's proc>)`
            // (`[conc.proc.link.pair]`). Idempotent per pair.
            "link" => {
                let other = match args.into_iter().flat_map(|l| l.args()).find_map(Arg::value) {
                    Some(v) => match val!(self.eval(v)) {
                        Value::Proc(other) => other,
                        _ => return self.refuse("a link to a value that is not a proc", v.span),
                    },
                    None => self.sched.tasks[self.me()].proc,
                };
                let procs = &mut self.sched.procs;
                if !procs[proc].links.contains(&other) {
                    procs[proc].links.push(other);
                }
                if !procs[other].links.contains(&proc) {
                    procs[other].links.push(proc);
                }
                // A partner already dead abnormally takes the other now.
                for (dead, live) in [(proc, other), (other, proc)] {
                    let abnormal = matches!(
                        &self.sched.procs[dead].exited,
                        Some(Value::ErrTag { tag, .. }) if tag != "normal"
                    );
                    if abnormal {
                        let regions = self.sched.kill_proc(live);
                        self.free_proc_regions(regions);
                    }
                }
                self.root_death()?;
                Ok(Flow::Val(Value::Unit))
            }
            _ => self.refuse("this proc method", e.span),
        }
    }

    /// `[conc.proc.root]`: the running task's own domain was killed
    /// under it (a linked partner died): it runs no further user code.
    fn root_death(&mut self) -> E<()> {
        if self.sched.tasks[self.me()].killed {
            return Err(Stop::Killed);
        }
        Ok(())
    }

    /// An exit reason's class predicates (`[conc.proc.exit]`).
    pub(super) fn exit_reason_is(&self, reason: &Value, method: &str) -> Option<bool> {
        let Value::ErrTag { tag, payload } = reason else {
            return None;
        };
        Some(match method {
            "is_normal" => tag == "normal",
            "is_error" => tag == "error",
            "is_killed" => tag == "killed",
            "is_cancelled" => tag == "cancelled",
            "is_fault" => tag == "fault",
            "is_alloc_contract" => {
                tag == "fault"
                    && matches!(payload.first(), Some(Value::Str(kind)) if kind == "alloc-contract")
            }
            _ => return None,
        })
    }

    // -- yields that are not channel operations --------------------------------------------

    /// `time_sleep_ms` beside other tasks: the sleeper parks, so its
    /// siblings run for the duration.
    pub(super) fn sleep_park(&mut self, ms: u64, span: Span) -> E<()> {
        if !self.sched.concurrent {
            std::thread::sleep(std::time::Duration::from_millis(ms));
            return Ok(());
        }
        let me = self.me();
        if self.sched.tasks[me].killed {
            return Err(Stop::Killed);
        }
        let seq = self.sched.sleep_seq;
        self.sched.sleep_seq += 1;
        self.sched.sleeps.push(RealSleep {
            due: std::time::Instant::now() + std::time::Duration::from_millis(ms),
            seq,
            task: me,
        });
        match self.park()? {
            Wake::Ok(_) | Wake::Cancelled => Ok(()),
            other => self.wake_stop(other, span),
        }
    }

    /// A host call that would block the whole machine (an accept, a
    /// read) polls instead and lets the other tasks run between polls.
    /// `false`: there is nobody else to wait for.
    pub(super) fn yield_now(&mut self, span: Span) -> E<bool> {
        let me = self.me();
        if self.sched.tasks[me].killed {
            return Err(Stop::Killed);
        }
        let others = self
            .sched
            .tasks
            .iter()
            .enumerate()
            .any(|(id, t)| id != me && t.state != TaskState::Done);
        if !others {
            return Ok(false);
        }
        self.sched.tasks[me].wake = Some(Wake::Ok(Value::Unit));
        match self.park()? {
            Wake::Ok(_) | Wake::Cancelled => Ok(true),
            other => self.wake_stop(other, span),
        }
    }

    // -- the race detector ---------------------------------------------------------------------

    /// One access against the happens-before order (`[conc.mm.race.3]`).
    pub(super) fn race_check(
        &mut self,
        key: RaceKey,
        lo: usize,
        hi: usize,
        write: bool,
        span: Span,
    ) -> E<()> {
        if !self.sched.concurrent {
            return Ok(());
        }
        let me = self.me();
        let atomic = self.in_atomic;
        let sched = &mut self.sched;
        if atomic && let Some(clock) = sched.atomic_clocks.get(&(key, lo)) {
            // Every order runs as seq_cst: the operation acquires its
            // location's clock first.
            merge_vc(&mut sched.tasks[me].vc, clock);
        }
        let my_vc = &sched.tasks[me].vc;
        // A plain access races a plain or an atomic one; an atomic
        // access races only a plain one.
        let mut found = sched
            .accesses
            .get(&key)
            .and_then(|history| unordered_conflict(history, me, my_vc, lo, hi, write));
        if found.is_none() && !atomic {
            found = sched
                .atomic_accesses
                .get(&key)
                .and_then(|history| unordered_conflict(history, me, my_vc, lo, hi, write));
        }
        if let Some((other, other_write)) = found {
            let what = match key {
                RaceKey::Alloc(id) => format!("allocation #{id}, bytes {lo}..{hi}"),
                RaceKey::Pool(pool, index) => format!("pool #{pool}, slot {index}"),
                RaceKey::Static(slot) => format!("module state #{slot}"),
            };
            let line = format!(
                "wolf: data race on {what}: this {}{} by `{}` (task {me}) and a{} {} by `{}` \
                 (task {other}) have no happens-before order ([conc.mm.hb])\n",
                if atomic { "atomic " } else { "" },
                if write { "write" } else { "read" },
                sched.tasks[me].name,
                if atomic { " plain" } else { "n earlier" },
                if other_write { "write" } else { "read" },
                sched.tasks[other].name,
            );
            self.stderr.extend_from_slice(line.as_bytes());
            return self.trap("race", "conc.mm.race.3", span);
        }
        let tick = my_vc.get(me).copied().unwrap_or(0);
        if atomic {
            remember(
                sched.atomic_accesses.entry(key).or_default(),
                me,
                tick,
                lo,
                hi,
                write,
            );
            let released = sched.tasks[me].vc.clone();
            merge_vc(sched.atomic_clocks.entry((key, lo)).or_default(), &released);
            sched.tick(me);
        } else {
            remember(
                sched.accesses.entry(key).or_default(),
                me,
                tick,
                lo,
                hi,
                write,
            );
        }
        Ok(())
    }
}
