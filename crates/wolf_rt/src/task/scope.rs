//! Scopes (s32 Targets 4/5/8) — the structured-concurrency unit.
//!
//! `scope { spawn f() }`: scope exit joins all children; a scope
//! cannot be exited while a child runs (`[conc.task.join]`). There is
//! **no detached spawn anywhere in this crate** — not private, not
//! "for the stdlib" (D16; the Go/Kotlin leak class is structurally
//! impossible). Scope handles are values: the runtime object here is
//! the capability the checker (c04) keeps inside its region.
//!
//! Failure semantics (`[conc.task.fail]`, `[conc.task.fail.owner]`):
//! a child completing with an error value — or, in the Rust test
//! surface, a panic, which becomes an exit *reason*, never an unwind
//! across the scheduler (D30) — cancels its siblings and the blocked
//! owner, and re-raises at the scope exit after the join. Multiple
//! failures surface the first in schedule order; the rest attach as
//! context. Cancellation is cooperative: observed only at
//! runtime-owned blocking points (`[conc.cancel.points]` — here:
//! join and [`ScopeInner::wait_cancelled`]; s33 adds channel ops),
//! and a cancelled task's own defers run (`[conc.cancel.defer]` —
//! compiled code's lowering does that by ordinary returns; D14's
//! cancelled-task-defers-run rule is why cancellation surfaces as a
//! *value* from blocking points, never as a teardown).
//!
//! Observability floor (Target 8): every task carries a name
//! (spawn-site default) and its scope id; scopes register in a global
//! (lazily created) registry so [`dump`] can emit the structured
//! scope-tree dump s54's pretty-printers consume.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};

use super::hooks::{SchedEvent, sched_point};
use super::pool;

/// How a task or scope run ended. This shape is pinned by snapshot
/// (the sibling-cancel acceptance test); extending it is a reviewed
/// change — s34's proc exit reasons (`[conc.proc.exit]`) build on it
/// (`Killed` is that reviewed extension: the kill-teardown outcome of
/// `[conc.proc.kill]`, distinct from `Cancelled` because the two run
/// different defer laws — D14's signature distinction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    /// Completed with its value.
    Normal,
    /// Completed with an error value; the tag travels (D30).
    Error {
        /// The error row tag, as codegen passes it (i64 register).
        tag: i64,
    },
    /// A Rust-side task panicked: caught at the task boundary and
    /// carried as a reason — panics never unwind across the scheduler.
    Panicked,
    /// Structured cancellation reached the task at a blocking point.
    Cancelled,
    /// Kill teardown reached the task at a blocking point: the task
    /// terminated WITHOUT running further user code
    /// (`[conc.proc.kill]` step 1 — contrast `Cancelled`, whose
    /// blocking point returns a value and lets defers run).
    Killed,
}

impl ExitReason {
    /// Failures fail the scope; `Cancelled` and `Killed` are
    /// consequences, not causes, and `Normal` is success.
    fn is_failure(&self) -> bool {
        matches!(self, ExitReason::Error { .. } | ExitReason::Panicked)
    }
}

/// Dump-visible task state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// Spawned, not yet picked up by a worker.
    Queued,
    /// Running on a worker.
    Running,
    /// Blocked at a runtime-owned blocking point.
    Blocked,
}

/// Task-id hasher: ids are sequential u64s, SipHash buys nothing and
/// sits on the sub-microsecond spawn path — a Fibonacci multiply
/// spreads them fine.
#[derive(Default)]
struct IdHasher(u64);

impl std::hash::Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

type IdMap<V> = HashMap<u64, V, std::hash::BuildHasherDefault<IdHasher>>;

struct ScopeState {
    /// Children spawned and not yet finished (run + cancellation
    /// drain included) — join waits for zero.
    live: usize,
    /// First failure in schedule order (first recorded here wins).
    first_fail: Option<ExitReason>,
    /// The rest, attached as context (`[conc.task.fail]`).
    context: Vec<ExitReason>,
    /// Live children for the dump: id → (name, state).
    tasks: IdMap<(std::sync::Arc<str>, TaskState)>,
}

/// The runtime scope object — the capability behind a `scope` block's
/// handle value.
pub struct ScopeInner {
    id: u64,
    name: Box<str>,
    parent: Option<Weak<ScopeInner>>,
    cancelled: AtomicBool,
    /// Kill teardown (s34, `[conc.proc.kill]`): a killed scope is also
    /// cancelled, but blocking points additionally refuse to return to
    /// user code (proc.rs's `kill_teardown_check`). The flag lives on
    /// the scope — not the proc — so the channel/when blocking paths
    /// need no proc-layer linkage (D15).
    killed: AtomicBool,
    state: Mutex<ScopeState>,
    cv: Condvar,
}

static NEXT_SCOPE: AtomicU64 = AtomicU64::new(1);
static NEXT_TASK: AtomicU64 = AtomicU64::new(1);

/// Global scope registry for the dump. Lazily created on first scope
/// (a binary that never opens a scope never touches it — D15).
static REGISTRY: OnceLock<Mutex<Vec<Weak<ScopeInner>>>> = OnceLock::new();

impl ScopeInner {
    /// Open a scope. `parent` is the creating task's scope, if any.
    pub fn new(name: &str, parent: Option<&Arc<ScopeInner>>) -> Arc<ScopeInner> {
        let scope = Arc::new(ScopeInner {
            id: NEXT_SCOPE.fetch_add(1, SeqCst),
            name: name.into(),
            parent: parent.map(Arc::downgrade),
            cancelled: AtomicBool::new(false),
            killed: AtomicBool::new(false),
            state: Mutex::new(ScopeState {
                live: 0,
                first_fail: None,
                context: Vec::new(),
                tasks: IdMap::default(),
            }),
            cv: Condvar::new(),
        });
        let reg = REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
        let mut reg = reg.lock().unwrap();
        reg.retain(|w| w.strong_count() > 0);
        reg.push(Arc::downgrade(&scope));
        scope
    }

    /// Stable scope id (dump + hook payloads).
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Cancelled, directly or through an enclosing scope (the scope
    /// is the cancellation unit, owner included — Trio posture).
    pub fn is_cancelled(&self) -> bool {
        if self.cancelled.load(SeqCst) {
            return true;
        }
        match &self.parent {
            Some(p) => p.upgrade().is_some_and(|p| p.is_cancelled()),
            None => false,
        }
    }

    /// Deliver structured cancellation: siblings and the blocked
    /// owner observe it at their next blocking point.
    pub fn cancel(&self) {
        self.cancelled.store(true, SeqCst);
        // Wake everything blocked on this scope (join, wait points).
        let _g = self.state.lock().unwrap();
        self.cv.notify_all();
    }

    /// Deliver kill teardown (`[conc.proc.kill]` step 1): the scope is
    /// cancelled AND marked killed, so blocking points terminate their
    /// task without returning to user code (defers do not run). Only
    /// the proc layer calls this — there is no task-level kill in the
    /// language.
    pub fn kill(&self) {
        self.killed.store(true, SeqCst);
        self.cancel();
    }

    /// Killed, directly or through an enclosing scope (kill reaches
    /// the whole proc task tree, same chain walk as [`Self::is_cancelled`]).
    pub fn is_killed(&self) -> bool {
        if self.killed.load(SeqCst) {
            return true;
        }
        match &self.parent {
            Some(p) => p.upgrade().is_some_and(|p| p.is_killed()),
            None => false,
        }
    }

    /// The root of this scope's parent chain — the proc-root scope for
    /// scopes inside a proc (proc.rs's scope→proc attribution). A
    /// dropped mid-chain parent terminates the walk (v0: scopes
    /// outlive their tasks in every supported shape).
    pub fn root(self: &Arc<ScopeInner>) -> Arc<ScopeInner> {
        let mut cur = self.clone();
        while let Some(p) = cur.parent.as_ref().and_then(Weak::upgrade) {
            cur = p;
        }
        cur
    }

    /// Register a child about to be spawned; returns its task id.
    /// The name is shared with the task object (one allocation on the
    /// spawn path — it is on the sub-microsecond budget).
    pub fn add_child(&self, name: &std::sync::Arc<str>) -> u64 {
        let id = NEXT_TASK.fetch_add(1, SeqCst);
        let mut st = self.state.lock().unwrap();
        st.live += 1;
        st.tasks.insert(id, (name.clone(), TaskState::Queued));
        id
    }

    /// Dump-state transition for a live child.
    pub fn child_state(&self, id: u64, state: TaskState) {
        let mut st = self.state.lock().unwrap();
        if let Some(t) = st.tasks.get_mut(&id) {
            t.1 = state;
        }
    }

    /// A child finished with `reason`. Failures cancel the scope
    /// (`[conc.task.fail]`); the last child out wakes the join.
    pub fn child_done(&self, id: u64, reason: ExitReason) {
        let mut st = self.state.lock().unwrap();
        st.tasks.remove(&id);
        if reason.is_failure() {
            if st.first_fail.is_none() {
                st.first_fail = Some(reason);
                drop(st);
                self.cancel();
                st = self.state.lock().unwrap();
            } else {
                st.context.push(reason);
            }
        } else if matches!(reason, ExitReason::Cancelled | ExitReason::Killed) {
            st.context.push(reason);
        }
        st.live -= 1;
        if st.live == 0 {
            self.cv.notify_all();
        }
    }

    /// Scope-exit join (`[conc.task.join]`): block until every child
    /// has completed or finished its cancellation, then re-raise the
    /// first failure. The wait is a runtime-owned blocking point with
    /// blocking compensation (Target 6).
    pub fn join(&self) -> Option<ExitReason> {
        sched_point(SchedEvent::Join { scope: self.id });
        pool::blocking(|| {
            let mut st = self.state.lock().unwrap();
            while st.live > 0 {
                st = self.cv.wait(st).unwrap();
            }
        });
        self.state.lock().unwrap().first_fail.take()
    }

    /// A runtime-owned blocking point that completes only when the
    /// scope is cancelled — predates s33's channels as the "blocked
    /// forever" stand-in (the sibling-cancel acceptance still uses
    /// it). Fires a cancel-check event on every wakeup.
    pub fn wait_cancelled(&self) {
        pool::blocking(|| {
            let mut st = self.state.lock().unwrap();
            loop {
                let cancelled = self.is_cancelled();
                sched_point(SchedEvent::CancelCheck {
                    scope: self.id,
                    cancelled,
                });
                if cancelled {
                    return;
                }
                // Timed wait: enclosing-scope cancellation notifies a
                // *parent's* cv, not ours — poll covers the chain
                // (v0; s33's primitives register on the chain).
                let (g, _t) = self
                    .cv
                    .wait_timeout(st, std::time::Duration::from_millis(5))
                    .unwrap();
                st = g;
            }
        });
        // Kill teardown (s34): a killed scope's task terminates here
        // rather than resuming user code (`[conc.proc.kill]` step 1).
        pool::kill_teardown_check(self);
    }

    /// Failure context attached after the first (`[conc.task.fail]`).
    pub fn context(&self) -> Vec<ExitReason> {
        self.state.lock().unwrap().context.clone()
    }

    fn dump_into(&self, out: &mut String) {
        use std::fmt::Write as _;
        let st = self.state.lock().unwrap();
        let parent = self.parent.as_ref().and_then(|w| w.upgrade()).map(|p| p.id);
        let _ = write!(out, "scope #{} '{}'", self.id, self.name);
        if let Some(p) = parent {
            let _ = write!(out, " parent=#{p}");
        }
        let _ = writeln!(
            out,
            " live={} cancelled={}",
            st.live,
            self.cancelled.load(SeqCst)
        );
        let mut tasks: Vec<_> = st.tasks.iter().collect();
        tasks.sort_by_key(|(id, _)| **id);
        for (id, (name, state)) in tasks {
            let _ = writeln!(out, "  task #{id} '{name}' {state:?}");
        }
    }
}

/// Render the structured task dump (Target 8): every live scope with
/// its live tasks, grouped by scope, parents named. Contents are
/// implementation-specified; existence is contract
/// (`[conc.task.name]`).
pub fn dump() -> String {
    let mut out = String::from("wolf-rt tasks:\n");
    if let Some(reg) = REGISTRY.get() {
        let scopes: Vec<Arc<ScopeInner>> = {
            let reg = reg.lock().unwrap();
            reg.iter().filter_map(Weak::upgrade).collect()
        };
        for s in scopes {
            s.dump_into(&mut out);
        }
    }
    out
}
