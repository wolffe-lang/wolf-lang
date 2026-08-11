//! Typed channels and `select` (s33) — the synchronization vocabulary
//! of the `sync` verb (D13/D14).
//!
//! # The model
//!
//! `Channel[T]` comes in exactly two flavors: rendezvous (capacity 0)
//! and bounded (capacity N). **No unbounded flavor** — Go's own
//! unbounded-queue proposal documents buffered channels as a poor
//! universal queue; channels here are synchronization vocabulary, real
//! queues are c08 stdlib work. The runtime never copies and never
//! inspects payloads: a payload is ONE WORD — a moved region's pointer
//! (the Verona iso-transfer: one word moves an arbitrary cyclic graph),
//! a frozen `imm` reference, or a plain value — and layout freedom
//! stays with codegen (`[conc.chan.move]`, `[conc.chan.imm]`).
//!
//! Close semantics (`[conc.chan.close]`, D30): `close` is explicit and
//! idempotent; buffered items drain; recv on drained-closed yields the
//! closed error VALUE; send on closed is an error value, never a
//! panic. A drained-closed channel makes a `select` recv arm READY
//! (`[conc.select.closed]`, Go's posture): an arm that could never
//! deliver must not block forever.
//!
//! # Memory-model obligations (spec 03 §1)
//!
//! The k-th send synchronizes-before the k-th receive completes
//! (`[conc.mm.hb.chan]`); close synchronizes-before a recv observing
//! closed; a moving send publishes the region's every prior write
//! (`[conc.mm.hb.move]`). Implementation: every edge passes through
//! the channel's mutex and the waiter cell's mutex — SC synchronization
//! throughout (Boehm's atomics posture; no relaxed orderings anywhere
//! in this file).
//!
//! # Blocking and cancellation
//!
//! Blocked send/recv/select park through [`pool::blocking`] (s32's
//! compensation) and are cancellation points (`[conc.cancel.points]`):
//! cancellation surfaces as a value at the blocking point, and a
//! committed operation is never torn — the commit is a single atomic
//! state-machine transition on the waiter cell, so a send either
//! happened before the cancel (peer observes the value) or not at all.
//!
//! # `select` (`[conc.select]`)
//!
//! Readiness is evaluated with every involved channel locked, in
//! canonical id order (two overlapping selects cannot deadlock — the
//! same whole-set ordering argument as `when`, `[conc.when.order]`).
//! Among simultaneously-ready arms the choice draws from the seeded
//! PRNG ([`SchedRng::for_select`]) — never ambient entropy; this is
//! load-bearing for `--replay=SEED` (`[conc.select.fair]`). A blocked
//! select registers ONE waiter cell on every arm; exactly-one-commit
//! is the cell's atomic state machine. Timeout arms need no timer
//! wheel yet (pay-for-what-you-use, D15: the deadline rides the
//! waiter's own timed wait; s35's wheel takes over when I/O arms
//! land); a fired deadline is a `timer.fire` seam event.
//!
//! Every operation routes through the spec/07 seam — see hooks.rs for
//! the exact kind inventory. Seam events may be emitted while channel
//! locks are held: observers record, they never re-enter the channel
//! layer (the test-hook contract).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::hooks::{ChanPhase, SchedEvent, SchedRng, sched_point};
use super::pool::{blocking, current_scope};

/// Why a channel operation did not deliver: the channel is closed
/// (`[conc.chan.close]`'s error value) or structured cancellation
/// reached the blocking point (`[conc.cancel.points]`). Both are
/// VALUES (D30) — never faults, never unwinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChanErr {
    /// The channel is closed (and, for recv, drained).
    Closed,
    /// Cancellation surfaced at the blocking point.
    Cancelled,
}

/// What a waiter cell committed with. One cell serves plain send,
/// plain recv, and every arm of a `select`; the arm index rides
/// alongside in the cell state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Committed {
    /// A recv completed with the peer's payload word.
    RecvOk(u64),
    /// A send's payload was taken (paired or buffered by a peer).
    SentOk,
    /// The channel closed under the blocked op.
    Closed,
    /// The timeout arm's deadline fired.
    Timeout,
    /// Cancellation surfaced.
    Cancelled,
}

/// Sentinel arm index for self-commits (timeout / cancellation).
const ARM_NONE: u32 = u32::MAX;

/// The blocked-operation cell: registered on every involved channel,
/// committed EXACTLY ONCE — by a peer (under that channel's lock), by
/// the deadline, or by cancellation. The single `committed` slot under
/// one mutex is the "shared atomic state machine" of the contract:
/// exactly-one-commit, never torn.
struct WaitCell {
    st: Mutex<Option<(u32, Committed)>>,
    cv: Condvar,
}

impl WaitCell {
    fn new() -> Arc<WaitCell> {
        Arc::new(WaitCell {
            st: Mutex::new(None),
            cv: Condvar::new(),
        })
    }

    /// Commit `(arm, c)` if nothing committed yet; the one winner
    /// returns true.
    fn try_commit(&self, arm: u32, c: Committed) -> bool {
        let mut st = self.st.lock().unwrap();
        if st.is_some() {
            return false;
        }
        *st = Some((arm, c));
        self.cv.notify_all();
        true
    }

    /// A racing readiness probe (never authoritative — the commit is).
    fn is_committed(&self) -> bool {
        self.st.lock().unwrap().is_some()
    }

    /// Park (with blocking compensation) until committed. Polls the
    /// current scope for cancellation (`[conc.cancel.points]`) and
    /// honors `deadline` (a timeout arm) — both resolve by
    /// self-committing through the same exactly-once gate, so a peer
    /// commit that wins the race is honored and never torn.
    fn wait(&self, deadline: Option<Instant>) -> (u32, Committed) {
        blocking(|| {
            let scope = current_scope();
            let mut st = self.st.lock().unwrap();
            loop {
                if let Some(c) = *st {
                    return c;
                }
                if scope.as_ref().is_some_and(|s| s.is_cancelled()) {
                    *st = Some((ARM_NONE, Committed::Cancelled));
                    sched_point(SchedEvent::CancelCheck {
                        scope: scope.as_ref().map_or(0, |s| s.id()),
                        cancelled: true,
                    });
                    return st.unwrap();
                }
                if let Some(d) = deadline
                    && Instant::now() >= d
                {
                    sched_point(SchedEvent::TimerFire);
                    *st = Some((ARM_NONE, Committed::Timeout));
                    return st.unwrap();
                }
                // 5ms poll backstop covers enclosing-scope
                // cancellation (the same posture as Gate/
                // wait_cancelled); real wakeups arrive via notify.
                let mut dur = Duration::from_millis(5);
                if let Some(d) = deadline {
                    dur = dur.min(d.saturating_duration_since(Instant::now()));
                }
                let (g, _) = self.cv.wait_timeout(st, dur).unwrap();
                st = g;
            }
        })
    }
}

/// A registered blocked operation: the shared cell, which arm of its
/// select it is (0 for plain ops), and — for send waiters — the
/// payload word the peer will take.
struct Waiter {
    cell: Arc<WaitCell>,
    arm: u32,
    val: u64,
}

struct ChanSt {
    buf: VecDeque<u64>,
    closed: bool,
    /// Blocked senders in arrival order, payload word attached.
    senders: VecDeque<Waiter>,
    /// Blocked receivers in arrival order.
    recvers: VecDeque<Waiter>,
}

impl ChanSt {
    /// Hand `v` straight to a blocked receiver; dead (already
    /// committed) waiters are skipped and dropped.
    fn deliver_to_recver(&mut self, v: u64) -> bool {
        while let Some(w) = self.recvers.pop_front() {
            if w.cell.try_commit(w.arm, Committed::RecvOk(v)) {
                return true;
            }
        }
        false
    }

    /// Take a blocked sender's payload, completing its send.
    fn take_from_sender(&mut self) -> Option<u64> {
        while let Some(w) = self.senders.pop_front() {
            if w.cell.try_commit(w.arm, Committed::SentOk) {
                return Some(w.val);
            }
        }
        None
    }

    fn has_live_sender(&self) -> bool {
        self.senders.iter().any(|w| !w.cell.is_committed())
    }

    fn has_live_recver(&self) -> bool {
        self.recvers.iter().any(|w| !w.cell.is_committed())
    }
}

static NEXT_CHAN: AtomicU64 = AtomicU64::new(1);

/// A runtime channel — the untyped word-payload object behind
/// `channel[T](n)` (`[conc.chan.type]` typing is the checker's;
/// `[conc.chan.buf]`: `cap == 0` is rendezvous, `cap >= 1` buffers).
pub struct Chan {
    id: u64,
    cap: usize,
    st: Mutex<ChanSt>,
}

impl Chan {
    /// Create a channel: `cap == 0` rendezvous, else bounded.
    pub fn new(cap: usize) -> Arc<Chan> {
        Arc::new(Chan {
            id: NEXT_CHAN.fetch_add(1, SeqCst),
            cap,
            st: Mutex::new(ChanSt {
                buf: VecDeque::new(),
                closed: false,
                senders: VecDeque::new(),
                recvers: VecDeque::new(),
            }),
        })
    }

    /// Stable channel id (hook payloads; select's canonical lock
    /// order).
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Send one payload word. Blocks when the buffer is full (or, at
    /// capacity 0, until a receiver pairs); a blocking send is a
    /// cancellation point. `Err(Closed)` is the send-on-closed error
    /// value (`[conc.chan.close]`).
    pub fn send(&self, v: u64) -> Result<(), ChanErr> {
        let mut st = self.st.lock().unwrap();
        if st.closed {
            return Err(ChanErr::Closed);
        }
        if st.deliver_to_recver(v) {
            sched_point(SchedEvent::ChanSend {
                chan: self.id,
                phase: ChanPhase::Commit,
            });
            return Ok(());
        }
        if st.buf.len() < self.cap {
            st.buf.push_back(v);
            sched_point(SchedEvent::ChanSend {
                chan: self.id,
                phase: ChanPhase::Commit,
            });
            return Ok(());
        }
        let cell = WaitCell::new();
        st.senders.push_back(Waiter {
            cell: cell.clone(),
            arm: 0,
            val: v,
        });
        sched_point(SchedEvent::ChanSend {
            chan: self.id,
            phase: ChanPhase::Block,
        });
        drop(st);
        let (_, c) = cell.wait(None);
        match c {
            Committed::SentOk => {
                sched_point(SchedEvent::ChanSend {
                    chan: self.id,
                    phase: ChanPhase::Commit,
                });
                Ok(())
            }
            Committed::Closed => Err(ChanErr::Closed),
            Committed::Cancelled => {
                self.remove_waiters_of(&cell);
                // Kill teardown (s34, `[conc.proc.kill]`): with the
                // channel state consistent again, a killed scope's
                // task terminates HERE — the cancelled error value is
                // never returned, so no defer runs on the kill path.
                kill_teardown_check_current();
                Err(ChanErr::Cancelled)
            }
            Committed::RecvOk(_) | Committed::Timeout => unreachable!("send cell"),
        }
    }

    /// Send a region value: the affine move (`[conc.chan.move]`).
    /// Routes the handle through s32's `__wolf_rt_region_transfer`
    /// seam (the checked profile's retag point, s36's recorded
    /// ownership transfer), then moves the one pointer word. The
    /// donor's loss of access is `[conc.chan.staleuse]`: statically
    /// E1001, dynamically `trap(use-after-move)` — enforced by the
    /// checker / checked-profile retag (s54), not by this word move.
    ///
    /// # Safety
    ///
    /// `region` must be a live region handle; per the move rule the
    /// caller may not touch it after a successful send.
    pub unsafe fn send_region(&self, region: *mut core::ffi::c_void) -> Result<(), ChanErr> {
        // SAFETY: caller contract — live handle, moved here.
        let moved = unsafe { super::__wolf_rt_region_transfer(region) };
        self.send(moved as u64)
    }

    /// Receive one payload word. Drains buffered items ahead of the
    /// closed error (`[conc.chan.close]`); blocks when empty (a
    /// cancellation point); `Err(Closed)` on drained-closed.
    pub fn recv(&self) -> Result<u64, ChanErr> {
        let mut st = self.st.lock().unwrap();
        if let Some(v) = st.buf.pop_front() {
            // Backfill from a blocked sender: its payload takes the
            // freed slot, its send commits (FIFO preserved).
            if let Some(v2) = st.take_from_sender() {
                st.buf.push_back(v2);
            }
            sched_point(SchedEvent::ChanRecv {
                chan: self.id,
                phase: ChanPhase::Commit,
            });
            return Ok(v);
        }
        if let Some(v) = st.take_from_sender() {
            // Rendezvous pairing (cap 0 — or a racing sender).
            sched_point(SchedEvent::ChanRecv {
                chan: self.id,
                phase: ChanPhase::Commit,
            });
            return Ok(v);
        }
        if st.closed {
            return Err(ChanErr::Closed);
        }
        let cell = WaitCell::new();
        st.recvers.push_back(Waiter {
            cell: cell.clone(),
            arm: 0,
            val: 0,
        });
        sched_point(SchedEvent::ChanRecv {
            chan: self.id,
            phase: ChanPhase::Block,
        });
        drop(st);
        let (_, c) = cell.wait(None);
        match c {
            Committed::RecvOk(v) => {
                sched_point(SchedEvent::ChanRecv {
                    chan: self.id,
                    phase: ChanPhase::Commit,
                });
                Ok(v)
            }
            Committed::Closed => Err(ChanErr::Closed),
            Committed::Cancelled => {
                self.remove_waiters_of(&cell);
                // Kill teardown — see the twin comment in `send`.
                kill_teardown_check_current();
                Err(ChanErr::Cancelled)
            }
            Committed::SentOk | Committed::Timeout => unreachable!("recv cell"),
        }
    }

    /// Close the channel (idempotent). Every blocked waiter resolves
    /// with the closed error value; buffered items stay for draining
    /// (`[conc.chan.close]`). Close synchronizes-before any recv that
    /// observes it (the channel mutex carries the edge).
    pub fn close(&self) {
        let mut st = self.st.lock().unwrap();
        if st.closed {
            return;
        }
        st.closed = true;
        sched_point(SchedEvent::ChanClose { chan: self.id });
        while let Some(w) = st.senders.pop_front() {
            w.cell.try_commit(w.arm, Committed::Closed);
        }
        while let Some(w) = st.recvers.pop_front() {
            w.cell.try_commit(w.arm, Committed::Closed);
        }
    }

    /// True once closed (test/introspection surface).
    pub fn is_closed(&self) -> bool {
        self.st.lock().unwrap().closed
    }

    /// Drop every queue entry belonging to `cell` (a resolved select
    /// or a cancelled op cleans up after itself; peers also skip and
    /// drop dead entries lazily).
    fn remove_waiters_of(&self, cell: &Arc<WaitCell>) {
        let mut st = self.st.lock().unwrap();
        st.senders.retain(|w| !Arc::ptr_eq(&w.cell, cell));
        st.recvers.retain(|w| !Arc::ptr_eq(&w.cell, cell));
    }
}

// ---- select --------------------------------------------------------------

/// One `select` arm over a channel (timeout and `else` arms are
/// parameters of [`select`] itself — codegen folds multiple `after`
/// arms to their minimum deadline).
pub enum Arm<'a> {
    /// `recv ch` — ready when a value (or drained-close) can deliver.
    Recv(&'a Chan),
    /// `send ch <- v` — ready when the payload can commit (or closed).
    Send(&'a Chan, u64),
}

impl Arm<'_> {
    fn chan(&self) -> &Chan {
        match self {
            Arm::Recv(c) | Arm::Send(c, _) => c,
        }
    }
}

/// A `select`'s outcome. Closed-channel arms COMMIT (with the closed
/// error value) rather than block forever — `[conc.select.closed]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selected {
    /// A recv arm delivered a value.
    Recv { arm: usize, value: u64 },
    /// A recv arm observed drained-close (the closed error value).
    RecvClosed { arm: usize },
    /// A send arm committed its payload.
    Sent { arm: usize },
    /// A send arm observed closed (the send error value).
    SendClosed { arm: usize },
    /// The timeout arm fired (`[conc.select.timeout]`).
    Timeout,
    /// No arm was ready and the immediate `else` arm ran.
    Else,
    /// Cancellation surfaced at the blocking point.
    Cancelled,
}

/// Process-wide select decision index: the n-th select's tie-break
/// stream derives from (root seed, n) alone — replayable from the
/// seed, stable per decision (`[conc.select.fair]`).
static SELECT_SEQ: AtomicU64 = AtomicU64::new(0);

/// `select` over `arms`, with an optional timeout arm and an optional
/// immediate `else` arm (`has_else`: non-blocking poll). Exactly one
/// ready arm commits; with none ready (and no `else`) the task parks
/// with one waiter cell registered on every arm — a cancellation
/// point.
pub fn select(arms: &[Arm], timeout: Option<Duration>, has_else: bool) -> Selected {
    let mut rng = SchedRng::for_select(SELECT_SEQ.fetch_add(1, SeqCst));
    select_with(&mut rng, arms, timeout, has_else)
}

/// [`select`] with the tie-break stream supplied — the seam the
/// fairness-under-seed acceptance drives with pinned seeds.
pub fn select_with(
    rng: &mut SchedRng,
    arms: &[Arm],
    timeout: Option<Duration>,
    has_else: bool,
) -> Selected {
    assert!(!arms.is_empty() || timeout.is_some() || has_else);
    assert!(arms.len() <= 64, "select supports at most 64 arms");
    let deadline = timeout.map(|d| Instant::now() + d);
    loop {
        // Lock every involved channel in canonical id order (dedup):
        // readiness evaluation and the committed op are atomic
        // against every peer, and two overlapping selects cannot
        // deadlock (the `when` whole-set ordering argument).
        let mut ids: Vec<(u64, &Chan)> = arms.iter().map(|a| (a.chan().id, a.chan())).collect();
        ids.sort_by_key(|(id, _)| *id);
        ids.dedup_by_key(|(id, _)| *id);
        let mut guards: Vec<(u64, MutexGuard<'_, ChanSt>)> = ids
            .iter()
            .map(|(id, c)| (*id, c.st.lock().unwrap()))
            .collect();
        fn st_of<'g>(guards: &'g mut [(u64, MutexGuard<'_, ChanSt>)], id: u64) -> &'g mut ChanSt {
            let i = guards.iter().position(|(g, _)| *g == id).unwrap();
            &mut guards[i].1
        }

        // Ready set (`[conc.select.ready]`; closed arms are ready —
        // `[conc.select.closed]`).
        let mut ready: Vec<usize> = Vec::new();
        for (i, arm) in arms.iter().enumerate() {
            let st = st_of(&mut guards, arm.chan().id);
            let r = match arm {
                Arm::Recv(_) => !st.buf.is_empty() || st.has_live_sender() || st.closed,
                Arm::Send(c, _) => st.closed || st.has_live_recver() || st.buf.len() < c.cap,
            };
            if r {
                ready.push(i);
            }
        }

        if !ready.is_empty() {
            let mask: u64 = ready.iter().fold(0, |m, &i| m | (1 << i));
            // The tie-break: seeded PRNG, never ambient entropy.
            let choice = ready[rng.below(ready.len())];
            let st = st_of(&mut guards, arms[choice].chan().id);
            let done = match &arms[choice] {
                Arm::Recv(c) => {
                    if let Some(v) = st.buf.pop_front() {
                        if let Some(v2) = st.take_from_sender() {
                            st.buf.push_back(v2);
                        }
                        Some((
                            c.id,
                            false,
                            Selected::Recv {
                                arm: choice,
                                value: v,
                            },
                        ))
                    } else if let Some(v) = st.take_from_sender() {
                        Some((
                            c.id,
                            false,
                            Selected::Recv {
                                arm: choice,
                                value: v,
                            },
                        ))
                    } else if st.closed {
                        Some((c.id, false, Selected::RecvClosed { arm: choice }))
                    } else {
                        None // readiness went stale (a waiter died) — re-poll
                    }
                }
                Arm::Send(c, v) => {
                    if st.closed {
                        Some((c.id, true, Selected::SendClosed { arm: choice }))
                    } else if st.deliver_to_recver(*v) {
                        Some((c.id, true, Selected::Sent { arm: choice }))
                    } else if st.buf.len() < c.cap {
                        st.buf.push_back(*v);
                        Some((c.id, true, Selected::Sent { arm: choice }))
                    } else {
                        None
                    }
                }
            };
            if let Some((chan, is_send, sel)) = done {
                if is_send {
                    sched_point(SchedEvent::ChanSend {
                        chan,
                        phase: ChanPhase::Commit,
                    });
                } else {
                    sched_point(SchedEvent::ChanRecv {
                        chan,
                        phase: ChanPhase::Commit,
                    });
                }
                sched_point(SchedEvent::SelectArm {
                    chosen: choice as u32,
                    ready: mask,
                });
                return sel;
            }
            continue; // stale readiness: drop guards, re-poll
        }

        if has_else {
            // The immediate arm: no ready arm, no decision to record.
            return Selected::Else;
        }

        // Block: ONE cell registered on every arm, exactly-one-commit
        // (the shared atomic state machine); registration happens with
        // all locks held, so no readiness can slip between poll and
        // park.
        let cell = WaitCell::new();
        for (i, arm) in arms.iter().enumerate() {
            let st = st_of(&mut guards, arm.chan().id);
            match arm {
                Arm::Recv(c) => {
                    st.recvers.push_back(Waiter {
                        cell: cell.clone(),
                        arm: i as u32,
                        val: 0,
                    });
                    sched_point(SchedEvent::ChanRecv {
                        chan: c.id,
                        phase: ChanPhase::Block,
                    });
                }
                Arm::Send(c, v) => {
                    st.senders.push_back(Waiter {
                        cell: cell.clone(),
                        arm: i as u32,
                        val: *v,
                    });
                    sched_point(SchedEvent::ChanSend {
                        chan: c.id,
                        phase: ChanPhase::Block,
                    });
                }
            }
        }
        drop(guards);

        let (arm, c) = cell.wait(deadline);
        // Clean our registrations off every involved channel.
        for (_, ch) in &ids {
            ch.remove_waiters_of(&cell);
        }
        let commit = |sel: Selected, chan: u64, is_send: bool| {
            if is_send {
                sched_point(SchedEvent::ChanSend {
                    chan,
                    phase: ChanPhase::Commit,
                });
            } else {
                sched_point(SchedEvent::ChanRecv {
                    chan,
                    phase: ChanPhase::Commit,
                });
            }
            sched_point(SchedEvent::SelectArm {
                chosen: arm,
                ready: 1u64 << arm,
            });
            sel
        };
        return match c {
            Committed::RecvOk(v) => commit(
                Selected::Recv {
                    arm: arm as usize,
                    value: v,
                },
                arms[arm as usize].chan().id,
                false,
            ),
            Committed::SentOk => commit(
                Selected::Sent { arm: arm as usize },
                arms[arm as usize].chan().id,
                true,
            ),
            Committed::Closed => match &arms[arm as usize] {
                Arm::Recv(ch) => commit(Selected::RecvClosed { arm: arm as usize }, ch.id, false),
                Arm::Send(ch, _) => commit(Selected::SendClosed { arm: arm as usize }, ch.id, true),
            },
            Committed::Timeout => Selected::Timeout,
            Committed::Cancelled => {
                // Kill teardown — registrations are already cleaned
                // off every arm above; see the twin comment in `send`.
                kill_teardown_check_current();
                Selected::Cancelled
            }
        };
    }
}

/// [`super::pool::kill_teardown_check`] against the calling task's
/// scope — the shared tail of every cancelled-resolution branch. Must
/// be called with no channel locks held.
fn kill_teardown_check_current() {
    if let Some(s) = current_scope() {
        super::pool::kill_teardown_check(&s);
    }
}

// ---- the C entry surface -------------------------------------------------
//
// Frozen ahead of consumption (the s32 lesson): codegen's channel
// lowering (the front end refuses `channel`/`select` until its sprint
// lands) will emit calls to these symbols; the surface is fixed here
// first. Status codes mirror `ChanErr`: 0 ok, 1 closed, 2 cancelled —
// error VALUES in the `[abi.err]` discipline, never traps.

use core::ffi::c_void;

const ST_OK: i32 = 0;
const ST_CLOSED: i32 = 1;
const ST_CANCELLED: i32 = 2;

fn status(r: Result<(), ChanErr>) -> i32 {
    match r {
        Ok(()) => ST_OK,
        Err(ChanErr::Closed) => ST_CLOSED,
        Err(ChanErr::Cancelled) => ST_CANCELLED,
    }
}

/// Create a channel: `cap == 0` rendezvous, `cap >= 1` bounded
/// (`[conc.chan.buf]`; there is no unbounded flavor). Returns the
/// channel handle. Negative `cap` is a codegen bug; clamped to 0.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_chan_new(cap: i64) -> *mut c_void {
    let cap = usize::try_from(cap).unwrap_or(0);
    Arc::into_raw(Chan::new(cap)).cast_mut().cast()
}

/// Clone the channel handle (a channel value copied to another task
/// crosses as a new reference; the object is shared).
///
/// # Safety
///
/// `ch` must be a live handle from [`__wolf_rt_chan_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_chan_clone(ch: *mut c_void) -> *mut c_void {
    // SAFETY: borrow without consuming, then bump the count.
    let arc = unsafe { std::mem::ManuallyDrop::new(Arc::from_raw(ch.cast::<Chan>())) };
    Arc::into_raw((*arc).clone()).cast_mut().cast()
}

/// Release one channel handle.
///
/// # Safety
///
/// `ch` must be a live handle; it is dead after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_chan_free(ch: *mut c_void) {
    // SAFETY: consumes the caller's reference.
    drop(unsafe { Arc::from_raw(ch.cast::<Chan>()) });
}

/// Send one payload word: 0 ok, 1 closed, 2 cancelled.
///
/// # Safety
///
/// `ch` must be a live channel handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_chan_send(ch: *mut c_void, val: u64) -> i32 {
    // SAFETY: borrow the caller's handle.
    let ch = unsafe { std::mem::ManuallyDrop::new(Arc::from_raw(ch.cast::<Chan>())) };
    status(ch.send(val))
}

/// Send a region value — the affine move (`[conc.chan.move]`): routes
/// the handle through the `__wolf_rt_region_transfer` seam, then
/// moves the word. After 0 (ok) the donor may not touch the handle
/// (`[conc.chan.staleuse]`); on 1/2 the move did not happen and the
/// donor still owns it.
///
/// # Safety
///
/// `ch` a live channel handle; `region` a live region handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_chan_send_region(ch: *mut c_void, region: *mut c_void) -> i32 {
    // SAFETY: borrow the caller's handle; region per caller contract.
    let ch = unsafe { std::mem::ManuallyDrop::new(Arc::from_raw(ch.cast::<Chan>())) };
    status(unsafe { ch.send_region(region) })
}

/// Receive one payload word into `*out`: 0 ok, 1 closed (drained), 2
/// cancelled. `*out` is written only on 0.
///
/// # Safety
///
/// `ch` a live channel handle; `out` writable when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_chan_recv(ch: *mut c_void, out: *mut u64) -> i32 {
    // SAFETY: borrow the caller's handle.
    let ch = unsafe { std::mem::ManuallyDrop::new(Arc::from_raw(ch.cast::<Chan>())) };
    match ch.recv() {
        Ok(v) => {
            if !out.is_null() {
                // SAFETY: caller contract — writable out slot.
                unsafe { out.write(v) };
            }
            ST_OK
        }
        Err(ChanErr::Closed) => ST_CLOSED,
        Err(ChanErr::Cancelled) => ST_CANCELLED,
    }
}

/// Close the channel (idempotent; `[conc.chan.close]`).
///
/// # Safety
///
/// `ch` must be a live channel handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_chan_close(ch: *mut c_void) {
    // SAFETY: borrow the caller's handle.
    let ch = unsafe { std::mem::ManuallyDrop::new(Arc::from_raw(ch.cast::<Chan>())) };
    ch.close();
}

/// One lowered `select` arm.
#[repr(C)]
pub struct WolfSelectArm {
    /// The channel handle.
    pub chan: *mut c_void,
    /// 0 = recv, 1 = send.
    pub dir: i32,
    /// Padding (zero).
    pub _pad: i32,
    /// The payload word (send arms; ignored for recv).
    pub value: u64,
}

/// Select return space: committed arm index in `0..n`; or one of
/// these.
pub mod select_verdict {
    /// The timeout arm fired.
    pub const TIMEOUT: i64 = -2;
    /// No ready arm; the `else` arm ran.
    pub const ELSE: i64 = -3;
    /// Cancellation surfaced at the blocking point.
    pub const CANCELLED: i64 = -4;
}

/// `select` over `n` arms. `timeout_ns < 0` means no timeout arm;
/// `has_else != 0` adds the immediate arm. Returns the committed arm
/// index (recv value written to `*out_val`, per-arm status to
/// `*out_status`: 0 ok, 1 closed) or a [`select_verdict`] code.
///
/// # Safety
///
/// `arms` must address `n` valid descriptors whose channel handles
/// are live; `out_val`/`out_status` writable when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_select(
    arms: *const WolfSelectArm,
    n: i64,
    timeout_ns: i64,
    has_else: i8,
    out_val: *mut u64,
    out_status: *mut i32,
) -> i64 {
    // SAFETY: caller contract — n valid descriptors.
    let descs = unsafe { std::slice::from_raw_parts(arms, usize::try_from(n).unwrap_or(0)) };
    let chans: Vec<std::mem::ManuallyDrop<Arc<Chan>>> = descs
        .iter()
        // SAFETY: live handles per caller contract; borrowed only.
        .map(|d| unsafe { std::mem::ManuallyDrop::new(Arc::from_raw(d.chan.cast::<Chan>())) })
        .collect();
    let arms: Vec<Arm<'_>> = descs
        .iter()
        .zip(&chans)
        .map(|(d, c)| {
            if d.dir == 1 {
                Arm::Send(c, d.value)
            } else {
                Arm::Recv(c)
            }
        })
        .collect();
    let timeout = u64::try_from(timeout_ns).ok().map(Duration::from_nanos);
    let write = |p: *mut u64, v: u64| {
        if !p.is_null() {
            // SAFETY: caller contract — writable slot.
            unsafe { p.write(v) }
        }
    };
    let write_st = |p: *mut i32, v: i32| {
        if !p.is_null() {
            // SAFETY: caller contract — writable slot.
            unsafe { p.write(v) }
        }
    };
    match select(&arms, timeout, has_else != 0) {
        Selected::Recv { arm, value } => {
            write(out_val, value);
            write_st(out_status, ST_OK);
            arm as i64
        }
        Selected::Sent { arm } => {
            write_st(out_status, ST_OK);
            arm as i64
        }
        Selected::RecvClosed { arm } | Selected::SendClosed { arm } => {
            write_st(out_status, ST_CLOSED);
            arm as i64
        }
        Selected::Timeout => select_verdict::TIMEOUT,
        Selected::Else => select_verdict::ELSE,
        Selected::Cancelled => select_verdict::CANCELLED,
    }
}

// ---- acceptance tests ----------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::hooks::test_hook;
    use super::super::{ExitReason, scope};
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// `[conc.mm.hb.chan]`'s observable half: the k-th send pairs the
    /// k-th receive — FIFO through the buffer AND through blocked
    /// senders (backfill preserves order).
    #[test]
    fn kth_send_pairs_kth_recv() {
        let ch = Chan::new(4);
        let tx = ch.clone();
        let got = Arc::new(Mutex::new(Vec::new()));
        let sink = got.clone();
        let r = scope("kth", |s| {
            s.spawn("sender", move |_| {
                for k in 0..64u64 {
                    assert_eq!(tx.send(k), Ok(()));
                }
                ExitReason::Normal
            });
            let rx = ch.clone();
            s.spawn("recver", move |_| {
                for _ in 0..64 {
                    sink.lock().unwrap().push(rx.recv().unwrap());
                }
                ExitReason::Normal
            });
        });
        assert!(r.is_ok());
        assert_eq!(*got.lock().unwrap(), (0..64).collect::<Vec<u64>>());
    }

    /// Rendezvous (cap 0): a send completes only by pairing.
    #[test]
    fn rendezvous_pairs_directly() {
        let ch = Chan::new(0);
        let tx = ch.clone();
        let r = scope("rdv", |s| {
            s.spawn("sender", move |_| {
                for k in 10..20u64 {
                    assert_eq!(tx.send(k), Ok(()));
                }
                ExitReason::Normal
            });
            for k in 10..20u64 {
                assert_eq!(ch.recv(), Ok(k));
            }
        });
        assert!(r.is_ok());
    }

    /// `[conc.chan.close]`: buffered items drain, THEN the closed
    /// error value; send-on-closed is the error value; close is
    /// idempotent. Exit values snapshotted exactly.
    #[test]
    fn close_drains_then_yields_closed_error() {
        let ch = Chan::new(4);
        assert_eq!(ch.send(1), Ok(()));
        assert_eq!(ch.send(2), Ok(()));
        ch.close();
        ch.close(); // idempotent
        assert_eq!(ch.send(3), Err(ChanErr::Closed));
        assert_eq!(ch.recv(), Ok(1));
        assert_eq!(ch.recv(), Ok(2));
        assert_eq!(ch.recv(), Err(ChanErr::Closed));
        assert_eq!(ch.recv(), Err(ChanErr::Closed));
    }

    /// Close resolves BLOCKED ops with the error value — never a
    /// fault (D30): a parked sender (full buffer) and a parked
    /// receiver (empty channel) both come back with `Closed`.
    #[test]
    fn close_wakes_blocked_ops_with_error_value() {
        let full = Chan::new(1);
        assert_eq!(full.send(9), Ok(()));
        let empty = Chan::new(1);
        let tx = full.clone();
        let rx = empty.clone();
        let r = scope("close-wakes", |s| {
            s.spawn("blocked-sender", move |_| {
                assert_eq!(tx.send(10), Err(ChanErr::Closed));
                ExitReason::Normal
            });
            s.spawn("blocked-recver", move |_| {
                assert_eq!(rx.recv(), Err(ChanErr::Closed));
                ExitReason::Normal
            });
            std::thread::sleep(Duration::from_millis(30)); // let both park
            full.close();
            empty.close();
        });
        assert!(r.is_ok());
        // The drained item survives the close.
        assert_eq!(full.recv(), Ok(9));
        assert_eq!(full.recv(), Err(ChanErr::Closed));
    }

    /// The error-value shapes, pinned (acceptance: snapshot exit
    /// values). Changing these is a reviewed change.
    #[test]
    fn chan_shapes_snapshot() {
        let shapes = [
            format!("{:?}", ChanErr::Closed),
            format!("{:?}", ChanErr::Cancelled),
            format!("{:?}", Selected::Recv { arm: 1, value: 7 }),
            format!("{:?}", Selected::RecvClosed { arm: 0 }),
            format!("{:?}", Selected::Sent { arm: 2 }),
            format!("{:?}", Selected::SendClosed { arm: 0 }),
            format!("{:?}", Selected::Timeout),
            format!("{:?}", Selected::Else),
            format!("{:?}", Selected::Cancelled),
        ];
        assert_eq!(
            shapes,
            [
                "Closed",
                "Cancelled",
                "Recv { arm: 1, value: 7 }",
                "RecvClosed { arm: 0 }",
                "Sent { arm: 2 }",
                "SendClosed { arm: 0 }",
                "Timeout",
                "Else",
                "Cancelled",
            ]
        );
    }

    /// `[conc.select.fair]` under seed (acceptance): two ready arms —
    /// a fixed seed makes the identical choice across 100 runs;
    /// different seeds observe both arms.
    #[test]
    fn select_fairness_under_seed() {
        let run_once = |rng: &mut SchedRng| -> usize {
            let a = Chan::new(1);
            let b = Chan::new(1);
            a.send(1).unwrap();
            b.send(2).unwrap();
            match select_with(rng, &[Arm::Recv(&a), Arm::Recv(&b)], None, false) {
                Selected::Recv { arm, .. } => arm,
                other => panic!("expected a recv commit, got {other:?}"),
            }
        };
        let first = run_once(&mut SchedRng::from_seed(42));
        for _ in 0..99 {
            assert_eq!(run_once(&mut SchedRng::from_seed(42)), first);
        }
        let mut seen = [false; 2];
        for seed in 0..64 {
            seen[run_once(&mut SchedRng::from_seed(seed))] = true;
        }
        assert_eq!(seen, [true, true], "some arm never chosen across seeds");
    }

    /// A blocked select commits exactly one arm when a peer arrives.
    #[test]
    fn select_blocks_then_commits_one_arm() {
        let a = Chan::new(0);
        let b = Chan::new(0);
        let tx = b.clone();
        let r = scope("sel-block", |s| {
            let (a2, b2) = (a.clone(), b.clone());
            s.spawn("selector", move |_| {
                match select(&[Arm::Recv(&a2), Arm::Recv(&b2)], None, false) {
                    Selected::Recv { arm: 1, value: 77 } => ExitReason::Normal,
                    other => panic!("wrong commit: {other:?}"),
                }
            });
            std::thread::sleep(Duration::from_millis(30)); // let it park
            assert_eq!(tx.send(77), Ok(()));
        });
        assert!(r.is_ok());
    }

    /// Send arms: the arm with space (or a waiting receiver) commits;
    /// the payload is observable at the peer.
    #[test]
    fn select_send_arm_commits() {
        let full = Chan::new(1);
        full.send(1).unwrap();
        let open = Chan::new(1);
        match select(&[Arm::Send(&full, 8), Arm::Send(&open, 9)], None, false) {
            Selected::Sent { arm: 1 } => {}
            other => panic!("wrong commit: {other:?}"),
        }
        assert_eq!(open.recv(), Ok(9));
    }

    /// `[conc.select.closed]`: a drained-closed channel makes its
    /// recv arm READY — the select commits the closed error value
    /// instead of blocking forever. Send arms on closed channels
    /// likewise surface the send error.
    #[test]
    fn select_closed_arms_are_ready() {
        let dead = Chan::new(1);
        dead.close();
        let idle = Chan::new(0);
        match select(&[Arm::Recv(&idle), Arm::Recv(&dead)], None, false) {
            Selected::RecvClosed { arm: 1 } => {}
            other => panic!("expected the closed arm: {other:?}"),
        }
        match select(&[Arm::Send(&idle, 1), Arm::Send(&dead, 2)], None, false) {
            Selected::SendClosed { arm: 1 } => {}
            other => panic!("expected the closed send arm: {other:?}"),
        }
    }

    /// Timeout arms (`[conc.select.timeout]`): the deadline fires
    /// through the seam; `else` polls without blocking.
    #[test]
    fn select_timeout_and_else() {
        let idle = Chan::new(0);
        let t0 = Instant::now();
        assert_eq!(
            select(&[Arm::Recv(&idle)], Some(Duration::from_millis(25)), false),
            Selected::Timeout
        );
        assert!(t0.elapsed() >= Duration::from_millis(25));
        assert_eq!(select(&[Arm::Recv(&idle)], None, true), Selected::Else);
    }

    /// Close arriving while a select is PARKED resolves the arm with
    /// the closed error value.
    #[test]
    fn select_parked_wakes_on_close() {
        let ch = Chan::new(0);
        let closer = ch.clone();
        let r = scope("sel-close", |s| {
            let ch2 = ch.clone();
            s.spawn("selector", move |_| {
                match select(&[Arm::Recv(&ch2)], None, false) {
                    Selected::RecvClosed { arm: 0 } => ExitReason::Normal,
                    other => panic!("wrong commit: {other:?}"),
                }
            });
            std::thread::sleep(Duration::from_millis(30));
            closer.close();
        });
        assert!(r.is_ok());
    }

    /// Cancellation at a blocked SEND (`[conc.cancel.points]`): the
    /// cancel error surfaces, the peer state is uncorrupted — the
    /// send happened not at all (no phantom sender remains).
    #[test]
    fn cancellation_surfaces_at_blocked_send() {
        let ch = Chan::new(0);
        let saw = Arc::new(AtomicUsize::new(0));
        let saw2 = saw.clone();
        let tx = ch.clone();
        let r = scope("cancel-send", |s| {
            s.spawn("blocked", move |_| {
                assert_eq!(tx.send(5), Err(ChanErr::Cancelled));
                saw2.fetch_add(1, SeqCst);
                ExitReason::Cancelled
            });
            std::thread::sleep(Duration::from_millis(30));
            s.spawn("failer", |_| ExitReason::Error { tag: 7 });
        });
        assert_eq!(r, Err(ExitReason::Error { tag: 7 }));
        assert_eq!(saw.load(SeqCst), 1);
        // Peer state uncorrupted: no phantom sender to pair with.
        assert_eq!(select(&[Arm::Recv(&ch)], None, true), Selected::Else);
    }

    /// Cancellation at a blocked RECV — same discipline.
    #[test]
    fn cancellation_surfaces_at_blocked_recv() {
        let ch = Chan::new(0);
        let saw = Arc::new(AtomicUsize::new(0));
        let saw2 = saw.clone();
        let rx = ch.clone();
        let r = scope("cancel-recv", |s| {
            s.spawn("blocked", move |_| {
                assert_eq!(rx.recv(), Err(ChanErr::Cancelled));
                saw2.fetch_add(1, SeqCst);
                ExitReason::Cancelled
            });
            std::thread::sleep(Duration::from_millis(30));
            s.spawn("failer", |_| ExitReason::Error { tag: 7 });
        });
        assert_eq!(r, Err(ExitReason::Error { tag: 7 }));
        assert_eq!(saw.load(SeqCst), 1);
        // No phantom receiver: a send finds nobody (else arm runs).
        assert_eq!(select(&[Arm::Send(&ch, 1)], None, true), Selected::Else);
    }

    /// Cancellation at a blocked SELECT — the operation resolves with
    /// the cancel value; every registration is cleaned off every arm.
    #[test]
    fn cancellation_surfaces_at_blocked_select() {
        let a = Chan::new(0);
        let b = Chan::new(0);
        let saw = Arc::new(AtomicUsize::new(0));
        let saw2 = saw.clone();
        let (a2, b2) = (a.clone(), b.clone());
        let r = scope("cancel-select", |s| {
            s.spawn("blocked", move |_| {
                assert_eq!(
                    select(&[Arm::Recv(&a2), Arm::Send(&b2, 3)], None, false),
                    Selected::Cancelled
                );
                saw2.fetch_add(1, SeqCst);
                ExitReason::Cancelled
            });
            std::thread::sleep(Duration::from_millis(30));
            s.spawn("failer", |_| ExitReason::Error { tag: 7 });
        });
        assert_eq!(r, Err(ExitReason::Error { tag: 7 }));
        assert_eq!(saw.load(SeqCst), 1);
        assert_eq!(select(&[Arm::Send(&a, 1)], None, true), Selected::Else);
        assert_eq!(select(&[Arm::Recv(&b)], None, true), Selected::Else);
    }

    /// The seam sees the s33 inventory: chan.send/chan.recv (block +
    /// commit), chan.close, select.arm with the ready set, and
    /// timer.fire (spec/07's kinds, exactly).
    #[test]
    fn seam_observes_channel_events() {
        let _serial = test_hook::SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let seen: Arc<Mutex<Vec<SchedEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        test_hook::set_test_hook(Some(Box::new(move |ev| {
            sink.lock().unwrap().push(*ev);
        })));
        let ch = Chan::new(1);
        let id = ch.id();
        ch.send(1).unwrap();
        // Two ready arms -> a select.arm event with a 2-bit mask.
        let other = Chan::new(1);
        other.send(2).unwrap();
        let sel = select(&[Arm::Recv(&ch), Arm::Recv(&other)], None, false);
        assert!(matches!(sel, Selected::Recv { .. }));
        // A timeout arm fires timer.fire.
        let idle = Chan::new(0);
        assert_eq!(
            select(&[Arm::Recv(&idle)], Some(Duration::from_millis(5)), false),
            Selected::Timeout
        );
        ch.close();
        test_hook::set_test_hook(None);
        let seen = seen.lock().unwrap();
        assert!(seen.contains(&SchedEvent::ChanSend {
            chan: id,
            phase: ChanPhase::Commit
        }));
        assert!(
            seen.iter()
                .any(|e| matches!(e, SchedEvent::SelectArm { ready: 0b11, .. }))
        );
        assert!(seen.contains(&SchedEvent::TimerFire));
        assert!(seen.contains(&SchedEvent::ChanClose { chan: id }));
        assert!(seen.iter().any(|e| matches!(
            e,
            SchedEvent::ChanRecv {
                phase: ChanPhase::Commit,
                ..
            }
        )));
    }

    /// `[conc.chan.move]`: a region send routes the handle through
    /// the `region_transfer` seam and moves the one pointer word.
    #[test]
    fn send_region_moves_the_word() {
        let ch = Chan::new(1);
        let h = crate::native::__wolf_rt_region_new();
        // SAFETY: live handle, moved into the channel, received once,
        // freed once.
        unsafe {
            assert_eq!(ch.send_region(h), Ok(()));
            let got = ch.recv().unwrap() as *mut core::ffi::c_void;
            assert_eq!(got, h);
            crate::native::__wolf_rt_region_free(got);
        }
    }

    /// The C symbol surface end to end: chan new/send/recv/close/free
    /// and select over descriptors carry values and statuses.
    #[test]
    fn c_surface_round_trip() {
        // SAFETY: entry points used per their documented contracts.
        unsafe {
            let ch = __wolf_rt_chan_new(2);
            assert_eq!(__wolf_rt_chan_send(ch, 41), 0);
            let ch2 = __wolf_rt_chan_clone(ch);
            let mut out = 0u64;
            assert_eq!(__wolf_rt_chan_recv(ch2, &raw mut out), 0);
            assert_eq!(out, 41);
            // Select: one ready recv arm (buffered), one idle.
            assert_eq!(__wolf_rt_chan_send(ch, 42), 0);
            let idle = __wolf_rt_chan_new(0);
            let arms = [
                WolfSelectArm {
                    chan: idle,
                    dir: 0,
                    _pad: 0,
                    value: 0,
                },
                WolfSelectArm {
                    chan: ch,
                    dir: 0,
                    _pad: 0,
                    value: 0,
                },
            ];
            let mut status = -1i32;
            let arm = __wolf_rt_select(arms.as_ptr(), 2, -1, 0, &raw mut out, &raw mut status);
            assert_eq!((arm, status, out), (1, 0, 42));
            // Closed channel: send errors with status 1.
            __wolf_rt_chan_close(ch);
            assert_eq!(__wolf_rt_chan_send(ch, 1), 1);
            // Else verdict on an all-idle select.
            let arms = [WolfSelectArm {
                chan: idle,
                dir: 0,
                _pad: 0,
                value: 0,
            }];
            let arm = __wolf_rt_select(arms.as_ptr(), 1, -1, 1, &raw mut out, &raw mut status);
            assert_eq!(arm, select_verdict::ELSE);
            __wolf_rt_chan_free(ch2);
            __wolf_rt_chan_free(ch);
            __wolf_rt_chan_free(idle);
        }
    }
}
