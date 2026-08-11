//! The deterministic test scheduler (s36 Phase B) — spec/07's engine.
//!
//! # The model
//!
//! Every tracked task runs one at a time under a **baton**: a task
//! executes only while it holds the baton, releases it at every
//! runtime-owned blocking point (`pool::blocking`) and at task exit,
//! and re-acquires it — via a scheduler *grant* — before continuing.
//! At each grant the scheduler picks the next runnable context from
//! the seeded PRNG (or a replay stream): that pick IS spec/07's `pick`
//! kind, and the recorded stream of `(index, kind, subject, choice)`
//! tuples is `sched-ev/1` (the c15 flight-recorder foundation).
//!
//! Tracking is **scope-rooted**: [`run`] installs a det-domain root
//! scope on the harness thread; only tasks whose scope chain roots
//! there participate. Everything else in the process (other tests'
//! tasks, pool plumbing) runs untracked and unrecorded — which is also
//! why `park`/`unpark`/`steal` never appear in a recording: under the
//! test scheduler they collapse into `pick` (spec/07 §1).
//!
//! # Grants and the quiet window
//!
//! A grant fires when no tracked context is running and at least one
//! is requesting. When some context sits blocked in a primitive (or a
//! spawned task has not yet reached a worker), a causally-pending
//! wakeup may still be in flight — those travel condvar notifies and
//! 5ms poll backstops — so the grant additionally waits for the
//! scheduler state to be **quiet** for [`QUIET`] (> the longest poll
//! period). This is quiescence-with-settle (the CHESS/Coyote posture):
//! practically deterministic, enforced by the run-twice-and-diff CI
//! property rather than a proof. Known gap, documented in spec/07: a
//! contended `when` cell hand-off is claimed by whichever waiter polls
//! first, and proc trees parent under the root domain (outside the det
//! domain) — both recorded for the c07 closeout.
//!
//! # Virtual time
//!
//! Timer arms never really sleep: a registered deadline fires only
//! when the whole det domain is otherwise idle (no runnable context,
//! no pending spawn, quiet elapsed) — earliest `(duration, arm order)`
//! first. A 10-second timeout in a det test completes in milliseconds,
//! deterministically.
//!
//! # Recording, seeds, replay
//!
//! `sched-ev/1` numbering and the seed split live in [`super::hooks`]
//! (`kind_code`, `seed_spec`) so the driver can share them; this
//! module produces [`Recording`]s, serializes them (versioned,
//! append-only per `[sched.stable]`), and replays them from a simple
//! seed (bit 62 clear), a packed mixed-radix seed (bit 62 set,
//! rendered as a `w1-` token), or an explicit `ev:` stream.

use std::collections::HashMap;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::hooks::{SchedEvent, SchedRng, kind_code, seed_spec};
use super::scope::ScopeInner;

/// Settle window before a grant while wakeups may be in flight; must
/// exceed the longest runtime poll backstop (5ms) with margin.
const QUIET: Duration = Duration::from_millis(12);

/// Poll period for threads waiting on the scheduler itself.
const SCHED_POLL: Duration = Duration::from_millis(2);

/// Waiting this long with no state transition and no grantable work
/// is a deadlock in the det domain — fail loudly with the task dump.
const STALL: Duration = Duration::from_secs(5);

/// One recorded schedule event — the `sched-ev/1` tuple (spec/07 §2).
/// c15's flight recorder extends this tuple, never replaces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    /// Position in the stream (0-based, dense).
    pub index: u64,
    /// `sched-ev/1` kind code ([`kind_code`]).
    pub kind: u8,
    /// Kind-specific subject, ids normalized to first-seen order per
    /// id space (so a recording replays across processes).
    pub subject: u64,
    /// The decision at decision-carrying kinds (`pick`: index into
    /// the sorted ready set; `select.arm`: index into the ready arms;
    /// `proc.exit`: the reason class; `acquire`: `idx<<16|set_len`).
    pub choice: u32,
}

/// A finished det run's event stream plus the per-decision radixes
/// (alternative counts) needed to pack it into a `w1-` seed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recording {
    /// The full stream, in order.
    pub records: Vec<Record>,
    /// `(choice, radix)` per DECISION point, in stream order — the
    /// mixed-radix digits of `[sched.seed]`'s packed encoding.
    pub decisions: Vec<(u32, u32)>,
}

impl Recording {
    /// Serialize to the frozen `sched-ev/1` text form: a version
    /// header line, then one `index kind subject choice` line per
    /// record. Append-only per `[sched.stable]`; the schema-compat
    /// test pins these bytes.
    pub fn serialize(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::from("sched-ev/1\n");
        for r in &self.records {
            let _ = writeln!(out, "{} {} {} {}", r.index, r.kind, r.subject, r.choice);
        }
        out
    }

    /// Parse the serialized form back (round-trip half of the schema
    /// test). `None` on any deviation from the frozen format.
    pub fn parse(text: &str) -> Option<Recording> {
        let mut lines = text.lines();
        if lines.next()? != "sched-ev/1" {
            return None;
        }
        let mut records = Vec::new();
        for line in lines {
            let mut it = line.split_ascii_whitespace();
            records.push(Record {
                index: it.next()?.parse().ok()?,
                kind: it.next()?.parse().ok()?,
                subject: it.next()?.parse().ok()?,
                choice: it.next()?.parse().ok()?,
            });
            if it.next().is_some() {
                return None;
            }
        }
        Some(Recording {
            records,
            decisions: Vec::new(),
        })
    }

    /// The last `k` records — the ring-buffer dump surfaced in
    /// exploration failure output.
    pub fn last(&self, k: usize) -> &[Record] {
        let n = self.records.len();
        &self.records[n.saturating_sub(k)..]
    }

    /// Pack the run's decisions into a bit-62 seed (`[sched.seed]`'s
    /// high namespace) when they fit; the `w1-` token renders it.
    pub fn packed_seed(&self) -> Option<u64> {
        seed_spec::encode_packed(&self.decisions)
    }

    /// The explicit decision stream (`ev:` form).
    pub fn stream(&self) -> Vec<u32> {
        self.decisions.iter().map(|&(c, _)| c).collect()
    }
}

/// Where decisions come from.
#[derive(Debug, Clone)]
pub enum Source {
    /// Simple seed (bit 62 clear): every decision draws from a
    /// [`SchedRng`] stream seeded here.
    Seed(u64),
    /// Packed mixed-radix schedule (bit 62 set, minus the flag):
    /// decode `choice = v % radix; v /= radix` per decision; an
    /// exhausted value keeps choosing 0.
    Packed(u64),
    /// Explicit decision stream (`ev:`): one choice per decision,
    /// taken modulo the live radix; exhausted → 0.
    Stream(Vec<u32>),
}

enum Draw {
    Rng(SchedRng),
    Packed(u64),
    Stream(Vec<u32>, usize),
}

impl Draw {
    fn below(&mut self, n: u32) -> u32 {
        debug_assert!(n > 0);
        match self {
            Draw::Rng(rng) => rng.below(n as usize) as u32,
            Draw::Packed(v) => {
                let c = (*v % u64::from(n)) as u32;
                *v /= u64::from(n);
                c
            }
            Draw::Stream(s, pos) => {
                let c = s.get(*pos).copied().unwrap_or(0) % n;
                *pos += 1;
                c
            }
        }
    }
}

struct Engine {
    /// The det-domain root scope id: tasks rooting here are tracked.
    root_scope: u64,
    draw: Draw,
    records: Vec<Record>,
    decisions: Vec<(u32, u32)>,
    next_index: u64,
    /// First-seen id normalization: (space, raw) → dense id.
    norm: HashMap<(u8, u64), u64>,
    norm_next: HashMap<u8, u64>,
    /// The context currently allowed to run.
    baton: Option<u64>,
    /// Contexts waiting for a grant (ctx ids).
    requesters: Vec<u64>,
    /// Tracked contexts currently blocked inside a runtime primitive.
    blocked: usize,
    /// Tracked tasks spawned but not yet entered on a worker.
    pending_spawn: usize,
    /// Last scheduler-state transition (the quiet clock).
    last_change: Instant,
    /// Registered virtual timers.
    timers: Vec<Timer>,
    next_timer: u64,
    timers_armed: u64,
}

struct Timer {
    id: u64,
    /// Fire order: earliest (duration, arm sequence) first.
    dur: Duration,
    seq: u64,
    fired: bool,
}

struct Sched {
    st: Mutex<Option<Engine>>,
    cv: Condvar,
}

fn sched() -> &'static Sched {
    static S: OnceLock<Sched> = OnceLock::new();
    S.get_or_init(|| Sched {
        st: Mutex::new(None),
        cv: Condvar::new(),
    })
}

/// Serializes det runs process-wide (the engine is a global).
static DET_SERIAL: Mutex<()> = Mutex::new(());

/// Lock-free "an engine is live" gate: with no det run active, the
/// per-spawn/per-task seams must not touch the engine mutex at all
/// (the rest of the test suite runs concurrently and unserialized).
static ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

thread_local! {
    /// This thread's tracked context id, while participating.
    static CTX: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
    /// Inside a `block_enter`..`block_exit` window.
    static IN_BLOCK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Events emitted while not holding the baton (blocked-side
    /// resolutions); flushed, in order, at the next grant.
    static BUFFER: std::cell::RefCell<Vec<SchedEvent>> = const { std::cell::RefCell::new(Vec::new()) };
    /// The last det choice drawn, paired with the next select.arm
    /// event's record.
    static LAST_CHOICE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// A timeout duration armed by `select`, consumed by the wait
    /// side's timer registration.
    static ARMED: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

// ---- the harness ---------------------------------------------------------

/// Run `f` under the deterministic scheduler with decisions from
/// `source`; returns the recording. The calling thread is context 0;
/// scopes opened inside `f` root in the det domain and their tasks are
/// serialized under the baton. Concurrent det runs are serialized.
pub fn run(source: Source, f: impl FnOnce()) -> Recording {
    let _serial = DET_SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let domain = ScopeInner::new("det-domain", None);
    {
        let mut st = sched().st.lock().unwrap();
        assert!(st.is_none(), "det engine already active");
        *st = Some(Engine {
            root_scope: domain.id(),
            draw: match source {
                Source::Seed(s) => Draw::Rng(SchedRng::from_seed(s)),
                Source::Packed(v) => Draw::Packed(v),
                Source::Stream(s) => Draw::Stream(s, 0),
            },
            records: Vec::new(),
            decisions: Vec::new(),
            next_index: 0,
            norm: HashMap::new(),
            norm_next: HashMap::new(),
            baton: Some(0), // the harness context starts granted
            requesters: Vec::new(),
            blocked: 0,
            pending_spawn: 0,
            last_change: Instant::now(),
            timers: Vec::new(),
            next_timer: 0,
            timers_armed: 0,
        });
        ACTIVE.store(true, std::sync::atomic::Ordering::Release);
    }
    CTX.with(|c| c.set(Some(0)));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        super::pool::with_scope(&domain, f)
    }));
    CTX.with(|c| c.set(None));
    IN_BLOCK.with(|c| c.set(false));
    BUFFER.with(|b| b.borrow_mut().clear());
    ARMED.with(|c| c.set(None));
    let rec = {
        let mut st = sched().st.lock().unwrap();
        ACTIVE.store(false, std::sync::atomic::Ordering::Release);
        let e = st.take().expect("det engine vanished mid-run");
        Recording {
            records: e.records,
            decisions: e.decisions,
        }
    };
    sched().cv.notify_all();
    match r {
        Ok(()) => rec,
        Err(p) => std::panic::resume_unwind(p),
    }
}

/// Parse a schedule spec: decimal seed, `w1-` packed token, or
/// `ev:c0,c1,…` explicit stream (the `--replay` argument forms).
pub fn parse_spec(spec: &str) -> Option<Source> {
    if let Some(list) = spec.strip_prefix("ev:") {
        let choices: Option<Vec<u32>> = list
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.trim().parse().ok())
            .collect();
        return Some(Source::Stream(choices?));
    }
    if let Some(v) = seed_spec::parse_token(spec) {
        return Some(Source::Packed(v & !seed_spec::PACKED_BIT));
    }
    let n: u64 = spec.parse().ok()?;
    Some(if n & seed_spec::PACKED_BIT != 0 {
        Source::Packed(n & !seed_spec::PACKED_BIT)
    } else {
        Source::Seed(n)
    })
}

// ---- id-space normalization ----------------------------------------------

mod space {
    pub const TASK: u8 = 1;
    pub const SCOPE: u8 = 2;
    pub const CHAN: u8 = 3;
    pub const SYNC: u8 = 4;
    pub const PROC: u8 = 5;
    pub const IO: u8 = 6;
}

impl Engine {
    fn norm_id(&mut self, sp: u8, raw: u64) -> u64 {
        if let Some(&d) = self.norm.get(&(sp, raw)) {
            return d;
        }
        let next = self.norm_next.entry(sp).or_insert(0);
        let d = *next;
        *next += 1;
        self.norm.insert((sp, raw), d);
        d
    }

    /// Record one event; `None` for kinds the det stream drops
    /// (pool plumbing collapses into `pick`; see module doc).
    fn record_event(&mut self, ev: &SchedEvent, choice: u32) {
        let kind = kind_code(ev);
        let subject = match *ev {
            SchedEvent::Park { .. } | SchedEvent::Unpark { .. } | SchedEvent::Steal { .. } => {
                return; // collapsed into pick (spec/07 §1)
            }
            SchedEvent::Spawn { task, .. } => self.norm_id(space::TASK, task),
            SchedEvent::Join { scope } => self.norm_id(space::SCOPE, scope),
            SchedEvent::CancelCheck { scope, cancelled } => {
                (self.norm_id(space::SCOPE, scope) << 1) | u64::from(cancelled)
            }
            SchedEvent::RegionTransfer | SchedEvent::TimerFire => 0,
            SchedEvent::ChanSend { chan, phase } | SchedEvent::ChanRecv { chan, phase } => {
                (self.norm_id(space::CHAN, chan) << 1)
                    | u64::from(phase == super::hooks::ChanPhase::Commit)
            }
            SchedEvent::ChanClose { chan } => self.norm_id(space::CHAN, chan) << 1,
            SchedEvent::SelectArm { ready, .. } => ready,
            SchedEvent::Acquire { obj, .. } => self.norm_id(space::SYNC, obj),
            SchedEvent::IoArrive { token } => self.norm_id(space::IO, token),
            SchedEvent::ProcSpawn { proc }
            | SchedEvent::ProcKill { proc }
            | SchedEvent::ProcExit { proc, .. } => self.norm_id(space::PROC, proc),
        };
        let choice = match *ev {
            SchedEvent::Acquire { idx, set_len, .. } => (idx << 16) | set_len,
            SchedEvent::ProcExit { kind, .. } => u32::from(kind),
            SchedEvent::SelectArm { .. } => choice,
            _ => 0,
        };
        let index = self.next_index;
        self.next_index += 1;
        self.records.push(Record {
            index,
            kind,
            subject,
            choice,
        });
    }

    fn touch(&mut self) {
        self.last_change = Instant::now();
    }

    /// Grant the baton if the domain is ready for a decision; fire a
    /// virtual timer instead when the whole domain is idle.
    fn try_grant(&mut self) -> bool {
        if self.baton.is_some() {
            return false;
        }
        // In-flight spawns must land on workers before any decision:
        // the requester set at a grant is then a deterministic
        // function of the schedule so far, not of worker latency.
        if self.pending_spawn > 0 {
            return false;
        }
        let quiet = self.last_change.elapsed() >= QUIET;
        if self.requesters.is_empty() {
            // Fully idle with parked deadline waiters: virtual time
            // advances — earliest (duration, arm order) fires.
            if quiet
                && self.blocked > 0
                && let Some(t) = self
                    .timers
                    .iter_mut()
                    .filter(|t| !t.fired)
                    .min_by_key(|t| (t.dur, t.seq))
            {
                t.fired = true;
                self.touch();
            }
            return false;
        }
        let fast = self.blocked == 0;
        if !(fast || quiet) {
            return false;
        }
        self.requesters.sort_unstable();
        let n = self.requesters.len() as u32;
        let idx = self.draw.below(n);
        self.decisions.push((idx, n));
        let ctx = self.requesters.remove(idx as usize);
        let subject = self.norm_id(space::TASK, ctx);
        let index = self.next_index;
        self.next_index += 1;
        self.records.push(Record {
            index,
            kind: 0, // pick
            subject,
            choice: idx,
        });
        self.baton = Some(ctx);
        self.touch();
        true
    }
}

// ---- hooks (called from the runtime seams) -------------------------------

/// Is this scope inside the live det domain? Lock-free `false` when
/// no engine is running — the common case for every other test in
/// the process.
pub(crate) fn is_tracked(scope: &std::sync::Arc<ScopeInner>) -> bool {
    if !ACTIVE.load(std::sync::atomic::Ordering::Acquire) {
        return false;
    }
    let s = sched();
    let st = s.st.lock().unwrap();
    match st.as_ref() {
        Some(e) => scope.root().id() == e.root_scope,
        None => false,
    }
}

/// Forget every det thread-local — a thread leaving a torn-down
/// engine (harness panic path) must not haunt the next run.
fn clear_thread_state() {
    CTX.with(|c| c.set(None));
    IN_BLOCK.with(|c| c.set(false));
    BUFFER.with(|b| b.borrow_mut().clear());
    ARMED.with(|c| c.set(None));
}

/// Park the calling thread until granted; runs `try_grant` each poll
/// so grants fire even with no dedicated scheduler thread. `false`
/// when the engine tore down instead of granting.
fn wait_for_baton(my: u64) -> bool {
    let s = sched();
    let mut st = s.st.lock().unwrap();
    let mut stall = Instant::now();
    loop {
        let Some(e) = st.as_mut() else {
            return false; // engine tore down (panic path)
        };
        if e.baton == Some(my) {
            drop(st);
            flush_buffer();
            return true;
        }
        if e.try_grant() {
            s.cv.notify_all();
            stall = Instant::now();
            continue;
        }
        if e.baton.is_none() && stall.elapsed() >= STALL {
            let dump = super::scope::dump();
            panic!("det scheduler stalled (deadlock in the det domain?)\n{dump}");
        }
        let (g, _) = s.cv.wait_timeout(st, SCHED_POLL).unwrap();
        st = g;
    }
}

/// Move the thread's buffered (blocked-side) events into the stream —
/// called exactly when the baton lands.
fn flush_buffer() {
    let evs: Vec<SchedEvent> = BUFFER.with(|b| b.borrow_mut().drain(..).collect());
    if evs.is_empty() {
        return;
    }
    let mut st = sched().st.lock().unwrap();
    if let Some(e) = st.as_mut() {
        for ev in &evs {
            e.record_event(ev, 0);
        }
    }
}

/// A tracked spawn is on its way to a worker (pairs with
/// [`ctx_enter`]); keeps the fast-grant path honest about in-flight
/// tasks.
pub(crate) fn note_spawn(scope: &std::sync::Arc<ScopeInner>) {
    if !is_tracked(scope) {
        return;
    }
    let s = sched();
    let mut st = s.st.lock().unwrap();
    if let Some(e) = st.as_mut() {
        e.pending_spawn += 1;
        e.touch();
    }
}

/// A worker picked up tracked task `id`: join the det domain and wait
/// for the first grant. No-op for untracked tasks.
pub(crate) fn ctx_enter(id: u64, scope: &std::sync::Arc<ScopeInner>) {
    if !is_tracked(scope) {
        return;
    }
    {
        let s = sched();
        let mut st = s.st.lock().unwrap();
        let Some(e) = st.as_mut() else { return };
        e.pending_spawn = e.pending_spawn.saturating_sub(1);
        e.requesters.push(id);
        e.touch();
        s.cv.notify_all();
    }
    CTX.with(|c| c.set(Some(id)));
    if !wait_for_baton(id) {
        clear_thread_state();
    }
}

/// The tracked task finished: release the baton for the next grant.
pub(crate) fn ctx_exit() {
    let Some(my) = CTX.with(std::cell::Cell::get) else {
        return;
    };
    CTX.with(|c| c.set(None));
    let s = sched();
    let mut st = s.st.lock().unwrap();
    if let Some(e) = st.as_mut() {
        debug_assert_eq!(e.baton, Some(my), "ctx_exit without the baton");
        e.baton = None;
        e.touch();
        e.try_grant();
    }
    s.cv.notify_all();
}

/// Entering a runtime-owned blocking point (`pool::blocking`): the
/// baton frees while the primitive parks this thread.
pub(crate) fn block_enter() {
    let Some(my) = CTX.with(std::cell::Cell::get) else {
        return;
    };
    if IN_BLOCK.with(std::cell::Cell::get) {
        return; // nested blocking() — outermost owns the transition
    }
    IN_BLOCK.with(|c| c.set(true));
    let s = sched();
    let mut st = s.st.lock().unwrap();
    if let Some(e) = st.as_mut() {
        debug_assert_eq!(e.baton, Some(my), "block_enter without the baton");
        e.baton = None;
        e.blocked += 1;
        e.touch();
        e.try_grant();
    }
    s.cv.notify_all();
}

/// The blocking primitive returned: rejoin the ready set and wait for
/// a grant before touching user-observable state again.
pub(crate) fn block_exit() {
    let Some(my) = CTX.with(std::cell::Cell::get) else {
        return;
    };
    if !IN_BLOCK.with(std::cell::Cell::get) {
        return;
    }
    IN_BLOCK.with(|c| c.set(false));
    {
        let s = sched();
        let mut st = s.st.lock().unwrap();
        let Some(e) = st.as_mut() else {
            clear_thread_state();
            return;
        };
        e.blocked = e.blocked.saturating_sub(1);
        e.requesters.push(my);
        e.touch();
        s.cv.notify_all();
    }
    if !wait_for_baton(my) {
        clear_thread_state();
    }
}

/// Seam event sink ([`super::hooks::sched_point`]'s det half): batoned
/// events record directly; blocked-side resolutions buffer until the
/// next grant so the stream stays scheduler-ordered.
pub(crate) fn on_event(ev: &SchedEvent) {
    let Some(my) = CTX.with(std::cell::Cell::get) else {
        return;
    };
    let s = sched();
    let mut st = s.st.lock().unwrap();
    let Some(e) = st.as_mut() else { return };
    if e.baton == Some(my) {
        let choice = LAST_CHOICE.with(std::cell::Cell::get);
        e.record_event(ev, choice);
    } else {
        // Poll-backstop cancel probes that resolved nothing are
        // timing noise, not decisions — drop them.
        if let SchedEvent::CancelCheck {
            cancelled: false, ..
        } = ev
        {
            return;
        }
        drop(st);
        BUFFER.with(|b| b.borrow_mut().push(*ev));
    }
}

/// A det-owned choice among `n` alternatives (select tie-breaks).
/// `None` when the calling thread is outside the det domain — the
/// caller falls back to its production PRNG.
pub(crate) fn choose(n: usize) -> Option<usize> {
    let _my = CTX.with(std::cell::Cell::get)?;
    let s = sched();
    let mut st = s.st.lock().unwrap();
    let e = st.as_mut()?;
    let n = u32::try_from(n).ok().filter(|&n| n > 0)?;
    let c = e.draw.below(n);
    e.decisions.push((c, n));
    LAST_CHOICE.with(|l| l.set(c));
    Some(c as usize)
}

/// `select` armed a timeout: stash the duration for the wait side.
pub(crate) fn arm_timer(dur: Duration) {
    if CTX.with(std::cell::Cell::get).is_some() {
        ARMED.with(|c| c.set(Some(dur)));
    }
}

/// Register a virtual timer for a parking wait (det domain only).
pub(crate) fn timer_register(deadline: Option<Instant>) -> Option<u64> {
    CTX.with(std::cell::Cell::get)?;
    deadline?;
    let dur = ARMED
        .with(|c| c.take())
        .or_else(|| deadline.map(|d| d.saturating_duration_since(Instant::now())))?;
    let s = sched();
    let mut st = s.st.lock().unwrap();
    let e = st.as_mut()?;
    let id = e.next_timer;
    e.next_timer += 1;
    let seq = e.timers_armed;
    e.timers_armed += 1;
    e.timers.push(Timer {
        id,
        dur,
        seq,
        fired: false,
    });
    Some(id)
}

/// Has the scheduler fired this virtual timer? (Replaces the real
/// clock comparison in det mode — no real sleeping.)
pub(crate) fn timer_fired(id: u64) -> bool {
    let s = sched();
    let mut st = s.st.lock().unwrap();
    let Some(e) = st.as_mut() else { return false };
    // Idle evaluation rides the same poll (fires timers when the
    // domain is fully quiescent).
    e.try_grant();
    e.timers.iter().any(|t| t.id == id && t.fired)
}

/// Deregister a virtual timer (wait resolved).
pub(crate) fn timer_done(id: u64) {
    let s = sched();
    let mut st = s.st.lock().unwrap();
    if let Some(e) = st.as_mut() {
        e.timers.retain(|t| t.id != id);
    }
}

/// One poll-loop tick from a parked waiter: drives grant/fire
/// evaluation so decisions land even when every thread is parked.
pub(crate) fn poll_tick() {
    if CTX.with(std::cell::Cell::get).is_none() {
        return;
    }
    let s = sched();
    let mut st = s.st.lock().unwrap();
    if let Some(e) = st.as_mut()
        && e.try_grant()
    {
        s.cv.notify_all();
    }
}

/// The det poll period for waiters inside the domain (tighter than
/// the production 5ms backstop so the quiet window keeps its margin).
pub(crate) fn poll_period(default: Duration) -> Duration {
    if CTX.with(std::cell::Cell::get).is_some() {
        SCHED_POLL
    } else {
        default
    }
}

// ---- acceptance tests ----------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::{Arm, Chan, ExitReason, Selected, scope, select};
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// The shared litmus: two tasks race their ids into one channel;
    /// the receiver records arrival order. WHICH id arrives first is
    /// exactly one `pick` decision — the planted "ordering bug" is
    /// first == 2.
    fn race_litmus() -> Vec<u64> {
        let ch = Chan::new(2);
        let (a, b) = (ch.clone(), ch.clone());
        let r = scope("race", |s| {
            s.spawn("one", move |_| {
                a.send(1).unwrap();
                ExitReason::Normal
            });
            s.spawn("two", move |_| {
                b.send(2).unwrap();
                ExitReason::Normal
            });
        });
        assert!(r.is_ok());
        vec![ch.recv().unwrap(), ch.recv().unwrap()]
    }

    /// `[sched.stable]`'s CI property: same seed ⇒ byte-identical
    /// stream AND identical observable outcome, run twice and diffed.
    #[test]
    fn same_seed_same_stream_same_outcome() {
        let mut streams = Vec::new();
        let mut outcomes = Vec::new();
        for _ in 0..2 {
            let out = StdMutex::new(Vec::new());
            let rec = run(Source::Seed(7), || {
                let o = race_litmus();
                *out.lock().unwrap() = o;
            });
            streams.push(rec.serialize());
            outcomes.push(out.into_inner().unwrap());
        }
        assert_eq!(
            streams[0], streams[1],
            "seed 7 must replay byte-identically"
        );
        assert_eq!(outcomes[0], outcomes[1]);
    }

    /// Bug-finding + replay (the acceptance's planted message
    /// reorder): exploration over derived seeds finds the schedule
    /// where task "two" delivers first, and the failing seed replays
    /// to the identical failure and stream.
    #[test]
    fn exploration_finds_reorder_and_seed_replays_it() {
        let mut failing: Option<(u64, Vec<u64>, String)> = None;
        for seed in 0..64u64 {
            let out = StdMutex::new(Vec::new());
            let rec = run(Source::Seed(seed), || {
                let o = race_litmus();
                *out.lock().unwrap() = o;
            });
            let o = out.into_inner().unwrap();
            if o[0] == 2 {
                failing = Some((seed, o, rec.serialize()));
                break;
            }
        }
        let (seed, o, stream) = failing.expect("no seed in 0..64 reordered the sends");
        // Replay: same seed, same failure, same stream — forever.
        let out = StdMutex::new(Vec::new());
        let rec = run(Source::Seed(seed), || {
            let o2 = race_litmus();
            *out.lock().unwrap() = o2;
        });
        assert_eq!(out.into_inner().unwrap(), o);
        assert_eq!(rec.serialize(), stream);
    }

    /// `--schedule=ev:<stream>` replay: the recorded decision stream
    /// alone reproduces the run.
    #[test]
    fn explicit_stream_replays_the_run() {
        let out = StdMutex::new(Vec::new());
        let rec = run(Source::Seed(41), || {
            let o = race_litmus();
            *out.lock().unwrap() = o;
        });
        let first = out.into_inner().unwrap();
        let out2 = StdMutex::new(Vec::new());
        let rec2 = run(Source::Stream(rec.stream()), || {
            let o = race_litmus();
            *out2.lock().unwrap() = o;
        });
        assert_eq!(out2.into_inner().unwrap(), first);
        assert_eq!(rec2.serialize(), rec.serialize());
    }

    /// The bit-62 packed seed (`[sched.seed]`) and its `w1-` token:
    /// pack the recorded decisions, render, parse, replay — identical
    /// stream. The token is the short form bs07 asked for.
    #[test]
    fn packed_token_replays_the_run() {
        let rec = run(Source::Seed(3), || {
            let _o = race_litmus();
        });
        let seed = rec.packed_seed().expect("small litmus fits below 2^62");
        assert_ne!(seed & seed_spec::PACKED_BIT, 0);
        let token = seed_spec::format_token(seed);
        assert!(token.starts_with("w1-") && token.len() <= 16, "{token}");
        let parsed = seed_spec::parse_token(&token).expect("token parses");
        assert_eq!(parsed, seed);
        let Some(Source::Packed(v)) = parse_spec(&token) else {
            panic!("token must parse as a packed source");
        };
        let rec2 = run(Source::Packed(v), || {
            let _o = race_litmus();
        });
        assert_eq!(rec2.serialize(), rec.serialize());
    }

    /// Virtual time: a 10-second timeout arm fires without real
    /// sleeping once the det domain is idle, deterministically.
    #[test]
    fn virtual_timer_fires_without_sleeping() {
        let t0 = Instant::now();
        let rec = run(Source::Seed(1), || {
            let idle = Chan::new(0);
            assert_eq!(
                select(&[Arm::Recv(&idle)], Some(Duration::from_secs(10)), false),
                Selected::Timeout
            );
        });
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "virtual time must not really sleep"
        );
        assert!(
            rec.records.iter().any(|r| r.kind == 13),
            "timer.fire recorded: {rec:?}"
        );
    }

    /// Under the test scheduler park/unpark/steal collapse into
    /// `pick` (spec/07 §1): the plumbing kinds never appear.
    #[test]
    fn pool_plumbing_collapses_into_pick() {
        let rec = run(Source::Seed(9), || {
            let _o = race_litmus();
        });
        assert!(
            rec.records
                .iter()
                .all(|r| r.kind != 3 && r.kind != 4 && r.kind != 5),
            "park/unpark/steal leaked into the det stream: {rec:?}"
        );
        // And the stream is non-trivial: picks, spawns, chan edges.
        assert!(rec.records.iter().any(|r| r.kind == 0));
        assert!(rec.records.iter().any(|r| r.kind == 1));
        assert!(rec.records.iter().any(|r| r.kind == 8));
    }

    /// sched-ev/1 serialization round-trips, and the v1 bytes are
    /// FROZEN — c15's flight recorder reads this format; changing it
    /// bumps the version, never edits it (`[sched.stable]`).
    #[test]
    fn schema_round_trip_and_frozen_v1() {
        let rec = Recording {
            records: vec![
                Record {
                    index: 0,
                    kind: 0,
                    subject: 1,
                    choice: 0,
                },
                Record {
                    index: 1,
                    kind: 8,
                    subject: 3,
                    choice: 0,
                },
                Record {
                    index: 2,
                    kind: 11,
                    subject: 0b11,
                    choice: 1,
                },
            ],
            decisions: vec![(0, 2), (1, 2)],
        };
        let text = rec.serialize();
        assert_eq!(text, "sched-ev/1\n0 0 1 0\n1 8 3 0\n2 11 3 1\n");
        let back = Recording::parse(&text).expect("round-trip");
        assert_eq!(back.records, rec.records);
        assert!(Recording::parse("sched-ev/0\n").is_none());
        assert!(Recording::parse("sched-ev/1\n0 0 1\n").is_none());
    }

    /// The `--replay` argument grammar: decimal simple seed, decimal
    /// packed seed, `w1-` token, `ev:` stream.
    #[test]
    fn replay_spec_grammar() {
        assert!(matches!(parse_spec("42"), Some(Source::Seed(42))));
        let packed = (1u64 << 62) | 5;
        assert!(matches!(
            parse_spec(&packed.to_string()),
            Some(Source::Packed(5))
        ));
        let tok = seed_spec::format_token(packed);
        assert!(matches!(parse_spec(&tok), Some(Source::Packed(5))));
        match parse_spec("ev:0,3,1") {
            Some(Source::Stream(s)) => assert_eq!(s, vec![0, 3, 1]),
            other => panic!("bad stream parse: {other:?}"),
        }
        assert!(parse_spec("w1-").is_none());
        assert!(parse_spec("nonsense").is_none());
    }

    /// Ids are normalized to first-seen order, so a recording is
    /// stable no matter what else ran in the process first.
    #[test]
    fn subject_ids_are_normalized() {
        // Burn some global ids so raw ids are far from zero.
        for _ in 0..8 {
            let _ = Chan::new(1);
        }
        let rec = run(Source::Seed(2), || {
            let _o = race_litmus();
        });
        for r in rec.records {
            if r.kind == 8 || r.kind == 9 {
                assert!(r.subject >> 1 < 4, "chan ids not normalized: {r:?}");
            }
        }
    }
}
