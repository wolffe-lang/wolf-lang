//! Procs and supervision (s34) — Armstrong's model without the copy
//! tax (D13; report 03 §"Wolf's answer").
//!
//! # The model
//!
//! A proc is a **failure domain**: an unforgeable generational id, a
//! name, a root task scope, an owned region set, and its mailbox
//! channels (`[conc.proc.1]`). Procs interact by message only — the
//! id is opaque, no memory handle crosses this API, so the post-v1
//! OS-process backend fits behind the same surface (03 Q1; the COPL
//! checklist, Armstrong thesis ~L956–976). The registry, the root
//! supervisor scope, and the region-ledger hooks are all lazily
//! initialized on the first proc spawn: a program that never spawns a
//! proc links none of this (D15 — the driver's `--gc-sections` test
//! bans the `__wolf_rt_proc_*` symbols from no-proc binaries).
//!
//! # Exit reasons and the two teardown laws
//!
//! `[conc.proc.exit]`'s closed set — `normal(value) | error(value) |
//! killed | cancelled` — is [`ProcExit`]. Reasons are VALUES (D30),
//! delivered to monitors over ordinary s33 channels, so
//! `[conc.mm.hb.proc]`'s edge rides the channel's own send↔recv
//! synchronization. A trapped/panicked proc task becomes
//! `error(value)` with the reserved [`TRAP_ERROR_TAG`] — a proc's
//! trap is an exit reason, never process death (unless the root
//! domain: `[conc.proc.root]`).
//!
//! The two teardown laws, side by side (D14's signature distinction,
//! litmus-pinned by `corpus/conc/proc_kill_defers.lu` /
//! `proc_cancel_defers.lu`):
//!
//! - **Kill** (`[conc.proc.kill]`, in order): (1) the task tree is
//!   cancelled *without running further user code* — blocking points
//!   raise the [`pool::KilledToken`] teardown instead of returning a
//!   value, so pending `defer`/`errdefer` DO NOT run; (2) external
//!   frames are waited out or fenced (`[conc.ffi.kill]` — v0 has no
//!   FFI inside procs, so this step is vacuous and documented);
//!   (3) the region ledger bulk-frees; (4) exit reasons deliver.
//! - **Cancel** (`[conc.proc.cancel]`): cooperative delivery at
//!   `[conc.cancel.points]`; blocking points return the cancellation
//!   VALUE and ordinary returns run the defers
//!   (`[conc.cancel.defer]`). A proc that completes its value despite
//!   cancellation keeps `normal(value)`.
//!
//! On the crash/normal path the order is deliver-then-free (sprint
//! Target 5: monitors never observe a freed region's data — reasons
//! are values and cross-proc data was moved or frozen, D14); on the
//! kill path the order is free-then-deliver, exactly
//! `[conc.proc.kill]`'s (3)-(4).
//!
//! # Region ownership & the ledger (Target 2)
//!
//! Every region created inside a proc is owned by it: the region
//! entry points report create/free through the fn-pointer seam in
//! native.rs (installed here, at registry init), and ownership moves
//! at the transfer seams — `__wolf_rt_region_transfer` (send side)
//! detaches from the donor, [`__wolf_rt_region_adopt`] (recv side,
//! frozen ahead of codegen consumption) attaches to the receiver.
//! Bulk-free on exit walks the ledger, not the objects (D10 Tier-1
//! wholesale free). Frozen `imm` data never enters the ledger —
//! freezing is s42's shared tier; the v0 ledger tracks region arenas
//! only.
//!
//! # link / monitor / supervision (Targets 4–5)
//!
//! `link(a, b)` couples fates symmetrically and idempotently per pair
//! (`[conc.proc.link.pair]`); an abnormal exit (`error`/`killed`)
//! delivers a kill to the partner, unless the partner traps exits
//! ([`set_trap_exit`]) — then the reason arrives as an ordinary
//! message. `monitor(p)` returns a channel that delivers the exit
//! reason as a value — asymmetric observation, observer outlives
//! observed (late monitors on an exited proc get the recorded reason
//! immediately). The **root supervisor's domain is the process**
//! (`[conc.proc.root]`, D16 escape 2): linking to [`ROOT_DOMAIN`]
//! couples a proc to it, and the root domain's abnormal death runs
//! the killed-proc sequence for every live proc and terminates the
//! process with the nonzero, implementation-specified
//! [`ROOT_DEATH_EXIT`]. Supervision *strategies* are library code
//! over these two primitives (Armstrong's posture): the minimal
//! restart-capable [`supervise`] loop ships here (restart, N-in-window
//! give-up, escalate by returning [`Supervised::GaveUp`] to the
//! caller — one level up, ultimately the root supervisor); richer
//! strategy trees are s39 stdlib. Flat beats deep (AXD301).
//!
//! # Handlers: atomic and non-blocking (Target 3)
//!
//! Mailboxes are ordinary s33 channels and a proc's receive loop is
//! `select` — no selective receive, ever (03 Q3). A handler runs to
//! completion without interleaving on proc state and may not block:
//! statically the checker's job (c05), dynamically a debug-assert at
//! every runtime-owned blocking point (`pool::blocking` checks
//! [`in_handler`] — defense in depth). Long work is spawned onto the
//! proc's scope; its completion is another message.
//!
//! # Codegen/sema seams (honest refusals)
//!
//! The `__wolf_rt_proc_*` C surface below is frozen ahead of
//! consumption (the s28/s32/s33 pattern). Nothing emits calls to it
//! yet: typecheck still refuses every concurrency construct with the
//! "concurrency typing (c05)" NotYet (spawn-with-closure gates
//! frontend-side), so the conc corpus tier sits at `phase: resolve`
//! and the litmuses execute under lupin. Kill teardown for COMPILED
//! task bodies is likewise deferred to the codegen sprint that lowers
//! the no-defer teardown branch: C ABI frames are never unwound
//! (`[conc.cancel.c]`), so the C entry points suppress the token and
//! surface the cancelled status value instead — documented, not
//! silent.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::chan::{Arm, Chan, select};
use super::hooks::{SchedEvent, sched_point};
use super::pool::{self, Body, TaskCtx};
use super::scope::{ExitReason, ScopeInner};
use crate::native;

/// The root supervisor's domain id — the process itself
/// (`[conc.proc.root]`; `[conc.task.root]`). Never a real proc id
/// (real ids carry a nonzero generation in the high word).
pub const ROOT_DOMAIN: u64 = 0;

/// Process exit status when the root supervisor domain dies
/// abnormally (`[conc.proc.root]`): nonzero and
/// implementation-specified — conforming tools compare the outcome
/// class, never this number (`[conf.trap.exit]` discipline).
pub const ROOT_DEATH_EXIT: i32 = 121;

/// The reserved `error(value)` tag of a trapped/panicked proc task —
/// a trap becomes an exit reason at the proc boundary, and this is
/// the value it carries until row payloads land (s39; D30 notes the
/// debug-build return trace joins it there).
pub const TRAP_ERROR_TAG: i64 = -1;

/// A proc's exit reason — `[conc.proc.exit]`'s closed set, as a
/// value (D30). The variant order is the wire `kind` code (0..=4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcExit {
    /// Completed with its value.
    Normal {
        /// The completion value (i64 register, `[abi.err]` shape).
        value: i64,
    },
    /// An error value crossed the proc boundary; panicked Rust-side
    /// tasks carry [`TRAP_ERROR_TAG`].
    Error {
        /// The error row tag (D30).
        tag: i64,
    },
    /// Kill teardown (`[conc.proc.kill]`) — defers did not run.
    Killed,
    /// Structured cancellation reached the proc and it did not
    /// complete a value (`[conc.proc.cancel]`) — defers ran.
    Cancelled,
    /// A trap fired on a task inside the proc and was contained at
    /// the boundary (D68, s132): the proc died by the killed-proc
    /// sequence (`[conc.proc.kill]` — no further user code, regions
    /// bulk-freed BEFORE delivery) and the trap's kind code rides as
    /// the reason's payload — reasons are values, never unwinding
    /// (`[conc.proc.exit]`). The kind codes are
    /// [`native::trap_code`]'s closed vocabulary (no new trap kind —
    /// that is D68's point).
    Fault {
        /// The trap kind code ([`native::trap_code`]).
        kind: i32,
    },
}

impl ProcExit {
    /// The reason-class code (hook payloads, the wire word's low
    /// byte): 0 normal, 1 error, 2 killed, 3 cancelled, 4 fault.
    pub fn kind(self) -> u8 {
        match self {
            ProcExit::Normal { .. } => 0,
            ProcExit::Error { .. } => 1,
            ProcExit::Killed => 2,
            ProcExit::Cancelled => 3,
            ProcExit::Fault { .. } => 4,
        }
    }

    /// True for the reasons that propagate over links
    /// (`[conc.proc.2]`: ABNORMAL exit kills the partner). `normal`
    /// completion and orderly cancellation do not propagate; a
    /// contained trap does (a faulted partner is a dead partner).
    pub fn is_abnormal(self) -> bool {
        matches!(
            self,
            ProcExit::Error { .. } | ProcExit::Killed | ProcExit::Fault { .. }
        )
    }

    /// Pack the reason into one channel word: kind in the low byte,
    /// payload (value, tag, or trap kind) in the upper 56 bits — the
    /// v0 wire shape monitors receive over their s33 channel (payloads
    /// outside ±2^55 truncate; row-value payloads replace this
    /// packing when s39 lands them). The word shape is pinned by
    /// snapshot below.
    pub fn encode(self) -> u64 {
        let (kind, payload) = match self {
            ProcExit::Normal { value } => (0u64, value),
            ProcExit::Error { tag } => (1, tag),
            ProcExit::Killed => (2, 0),
            ProcExit::Cancelled => (3, 0),
            ProcExit::Fault { kind } => (4, i64::from(kind)),
        };
        ((payload as u64) << 8) | kind
    }

    /// Unpack [`Self::encode`]'s word (arithmetic shift restores the
    /// payload's sign). Unknown kinds decode as `error(TRAP_ERROR_TAG)`
    /// — a forged word is a fault value, never UB.
    pub fn decode(word: u64) -> ProcExit {
        let payload = (word as i64) >> 8;
        match word & 0xFF {
            0 => ProcExit::Normal { value: payload },
            1 => ProcExit::Error { tag: payload },
            2 => ProcExit::Killed,
            3 => ProcExit::Cancelled,
            4 => ProcExit::Fault {
                kind: payload as i32,
            },
            _ => ProcExit::Error {
                tag: TRAP_ERROR_TAG,
            },
        }
    }
}

/// What a proc body hands back (the Rust scaffolding surface; the C
/// surface maps `0 → Value(0)`, nonzero → `Fail` per `[abi.err]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcOutcome {
    /// Completed with a value — reason `normal(value)`.
    Value(i64),
    /// Completed with an error value — reason `error(tag)`.
    Fail {
        /// The error row tag (D30).
        tag: i64,
    },
    /// The body acknowledged cancellation and stopped without a value
    /// — reason `cancelled` (`[conc.proc.cancel]`).
    Cancelled,
}

/// Why a proc operation did not apply: the id is stale (generational
/// ids fault cleanly — X5 spirit; the C surface maps this to
/// `trap(stale-handle)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcErr {
    /// No live or exited proc behind this id.
    Stale,
}

// ---- registry -------------------------------------------------------------

enum SlotState {
    Live(Arc<Proc>),
    /// Tombstone: the recorded reason serves late monitors (observer
    /// outlives observed). v0 never reuses slots — procs are coarse;
    /// slot recycling (with a generation bump) is post-v0.
    Exited(ProcExit),
}

struct Registry {
    slots: Vec<SlotState>,
    /// Proc-root scope id → proc id (scope-chain attribution:
    /// `current_proc` walks to the root scope and looks it up here).
    scope_to_proc: HashMap<u64, u64>,
    /// Region handle → owning proc id (the free/transfer side of the
    /// ledger; the per-proc set below is the bulk-free walk).
    region_owner: HashMap<usize, u64>,
    /// Proc id → owned region handles.
    proc_regions: HashMap<u64, HashSet<usize>>,
}

static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

/// The root supervisor scope (D16 escape 2): process-lifetime, never
/// joined; reaper tasks and daemon-shaped procs hang under it, so the
/// structured dump enumerates them (`[conc.task.root]` — there is no
/// detached proc).
static ROOT_SCOPE: OnceLock<Arc<ScopeInner>> = OnceLock::new();

static LEDGER_HOOKS: native::RegionLedgerHooks = native::RegionLedgerHooks {
    on_new: ledger_on_new,
    on_free: ledger_on_free,
};

fn registry() -> &'static Mutex<Registry> {
    REGISTRY.get_or_init(|| {
        // First proc: install the region-ledger seam (native.rs calls
        // through it from now on) — the D15 lazy-init moment — and the
        // trap-containment seam beside it (D68, s132): from the first
        // proc on, a trap on a task inside a proc dies at the proc
        // boundary instead of taking the process.
        native::install_region_ledger_hooks(&LEDGER_HOOKS);
        native::install_trap_containment(contain_trap);
        Mutex::new(Registry {
            slots: Vec::new(),
            scope_to_proc: HashMap::new(),
            region_owner: HashMap::new(),
            proc_regions: HashMap::new(),
        })
    })
}

fn root_scope() -> &'static Arc<ScopeInner> {
    ROOT_SCOPE.get_or_init(|| ScopeInner::new("root-supervisor", None))
}

/// Pack (generation, index) — generation in the high word so id 0 is
/// never a real proc (it is [`ROOT_DOMAIN`]). v0 pins generation 1
/// (slots are not reused); the shape is the contract.
fn pack_id(generation: u32, index: u32) -> u64 {
    (u64::from(generation) << 32) | u64::from(index)
}

fn slot_index(id: u64) -> Option<usize> {
    // v0: generation must be exactly 1 (no reuse yet); anything else
    // is a stale or forged id.
    if id >> 32 != 1 {
        return None;
    }
    Some((id & 0xFFFF_FFFF) as usize)
}

fn live_proc(id: u64) -> Result<Arc<Proc>, ProcErr> {
    let reg = registry().lock().unwrap();
    match slot_index(id).and_then(|i| reg.slots.get(i)) {
        Some(SlotState::Live(p)) => Ok(p.clone()),
        Some(SlotState::Exited(_)) => Err(ProcErr::Stale),
        None => Err(ProcErr::Stale),
    }
}

// ---- the proc object -------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Running,
    Exited(ProcExit),
}

struct ProcSt {
    phase: Phase,
    /// Kill requested (`[conc.proc.kill]` step 1 begun) — decides the
    /// reason ahead of anything the tree reports.
    kill_requested: bool,
    /// A trap was contained at this proc's boundary (D68, s132): the
    /// trap kind code. The FIRST decided teardown wins: a fault only
    /// records into a proc that is still running and not already
    /// being killed, and once recorded it decides the reason ahead of
    /// `kill_requested`.
    fault: Option<i32>,
    /// The body completed with this value (reason `normal(value)`).
    value: Option<i64>,
    /// The body stopped by acknowledging cancellation.
    main_cancelled: bool,
    /// Linked partner ids (idempotent per pair,
    /// `[conc.proc.link.pair]`); may contain [`ROOT_DOMAIN`].
    links: Vec<u64>,
    /// Monitor channels awaiting the exit reason.
    monitors: Vec<Arc<Chan>>,
    /// Trap-exit mailbox: link deaths arrive as messages instead of
    /// kills (the acceptance's "trap converts to a message").
    trap_exit: Option<Arc<Chan>>,
}

/// One supervised failure domain (D13). Runtime-internal: the public
/// surface is the id-keyed functions below — no memory handle crosses
/// the proc API.
struct Proc {
    id: u64,
    scope: Arc<ScopeInner>,
    st: Mutex<ProcSt>,
    cv: Condvar,
}

// ---- spawn ------------------------------------------------------------------

/// Spawn a proc under the root supervisor (`[conc.task.root]`):
/// `body` runs as the proc-main task of a fresh, parentless scope —
/// the failure domain's boundary — and a reaper task under the root
/// supervisor waits out the tree, decides the exit reason, and runs
/// the delivery/free sequence. Returns the proc's opaque id.
pub fn spawn_proc(name: &str, body: impl FnOnce(&TaskCtx) -> ProcOutcome + Send + 'static) -> u64 {
    // The proc's root task scope: parentless — enclosing-scope
    // cancellation must NOT reach into a foreign failure domain.
    let scope = ScopeInner::new(&format!("proc:{name}"), None);
    let proc = {
        let mut reg = registry().lock().unwrap();
        let index = u32::try_from(reg.slots.len()).expect("proc slots exhausted");
        let id = pack_id(1, index);
        let proc = Arc::new(Proc {
            id,
            scope: scope.clone(),
            st: Mutex::new(ProcSt {
                phase: Phase::Running,
                kill_requested: false,
                fault: None,
                value: None,
                main_cancelled: false,
                links: Vec::new(),
                monitors: Vec::new(),
                trap_exit: None,
            }),
            cv: Condvar::new(),
        });
        reg.slots.push(SlotState::Live(proc.clone()));
        reg.scope_to_proc.insert(scope.id(), id);
        reg.proc_regions.insert(id, HashSet::new());
        proc
    };
    sched_point(SchedEvent::ProcSpawn { proc: proc.id });

    // proc-main FIRST (the scope must be non-quiescent before the
    // reaper's join can observe it), then the reaper.
    let main_proc = proc.clone();
    pool::spawn_task(
        &scope,
        &format!("proc-main:{name}"),
        Body::Rust(Box::new(move |ctx| match body(ctx) {
            ProcOutcome::Value(v) => {
                main_proc.st.lock().unwrap().value = Some(v);
                ExitReason::Normal
            }
            ProcOutcome::Fail { tag } => ExitReason::Error { tag },
            ProcOutcome::Cancelled => {
                main_proc.st.lock().unwrap().main_cancelled = true;
                ExitReason::Cancelled
            }
        })),
    );
    let reaper_proc = proc.clone();
    pool::spawn_task(
        root_scope(),
        &format!("proc-reaper:{name}"),
        Body::Rust(Box::new(move |_| {
            reap(&reaper_proc);
            ExitReason::Normal
        })),
    );
    proc.id
}

/// The exit sequence — runs on the reaper task once per proc.
fn reap(proc: &Arc<Proc>) {
    // Wait out the task tree (`[conc.task.join]` — the crash path's
    // "once the tree is quiescent"). First failure in schedule order
    // is the crash reason.
    let first_fail = proc.scope.join();

    // Decide the reason (`[conc.proc.exit]`) — but do NOT publish it
    // yet: publication (phase + tombstone) is what lets a racing
    // `monitor()` deliver immediately, and on the kill-ordered path
    // the reason must not be observable before the ledger frees
    // (`[conc.proc.kill]` (3)-(4); `[mem.region.cap.3]` — s132 found
    // the early `Phase::Exited` write let an early monitor read
    // `fault(...)` with the breaching charge still live).
    let reason = {
        let st = proc.st.lock().unwrap();
        if let Some(kind) = st.fault {
            // A contained trap (D68): the fault decided the teardown
            // before any kill that raced in behind it — the reason
            // carries the trap kind to the join as a value.
            ProcExit::Fault { kind }
        } else if st.kill_requested {
            ProcExit::Killed
        } else {
            match first_fail {
                Some(ExitReason::Error { tag }) => ProcExit::Error { tag },
                // A Rust-side panic becomes an exit reason at the
                // proc boundary — not process death (unless root).
                Some(_) => ProcExit::Error {
                    tag: TRAP_ERROR_TAG,
                },
                None => match (st.value, st.main_cancelled) {
                    (Some(value), _) => ProcExit::Normal { value },
                    (None, true) => ProcExit::Cancelled,
                    // Cancellation delivered and every task drained
                    // without reporting a value.
                    (None, false) if proc.scope.is_cancelled() => ProcExit::Cancelled,
                    (None, false) => ProcExit::Normal { value: 0 },
                },
            }
        }
    };
    sched_point(SchedEvent::ProcExit {
        proc: proc.id,
        kind: reason.kind(),
    });

    // Take the region ledger (both maps) — the slot stays Live and
    // the phase stays Running until `publish`, so every observer
    // still queues behind the teardown instead of reading around it.
    let owned: Vec<usize> = {
        let mut reg = registry().lock().unwrap();
        reg.scope_to_proc.remove(&proc.scope.id());
        let owned = reg
            .proc_regions
            .remove(&proc.id)
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<usize>>();
        for h in &owned {
            reg.region_owner.remove(h);
        }
        owned
    };

    // Publish the reason: registry tombstone (late monitors read it),
    // then phase + cv (kill()/monitor() waiters), then the registered
    // monitors/links are taken for delivery. Ordered AROUND the
    // bulk-free per teardown law below.
    let publish = |reason: ProcExit| -> (Vec<Arc<Chan>>, Vec<u64>) {
        {
            let mut reg = registry().lock().unwrap();
            if let Some(i) = slot_index(proc.id) {
                reg.slots[i] = SlotState::Exited(reason);
            }
        }
        let mut st = proc.st.lock().unwrap();
        st.phase = Phase::Exited(reason);
        proc.cv.notify_all();
        (
            std::mem::take(&mut st.monitors),
            std::mem::take(&mut st.links),
        )
    };

    // `[conc.ffi.kill]` step 2 is vacuous in v0 (no FFI runs inside
    // procs yet); the wait-out-or-fence obligation lands with c10.
    if matches!(reason, ProcExit::Killed | ProcExit::Fault { .. }) {
        // Kill order (`[conc.proc.kill]`): (3) bulk-free, (4) deliver.
        // A contained trap dies by the same sequence (D68, s132) — and
        // free-BEFORE-deliver is the bound half of the cap contract
        // (`[mem.region.cap.3]`): by the time `fault(alloc-contract)`
        // reaches the join, the breaching proc's charge is already
        // reclaimed wholesale, so a supervisor that admits work on the
        // reason never races the memory it was promised back. The
        // reason publishes AFTER the free, so even a monitor that
        // races the reaper cannot read it early.
        bulk_free(&owned);
        let (monitors, links) = publish(reason);
        deliver(reason, &monitors, &links);
    } else {
        // Crash/normal order (sprint Target 5): deliver, then free —
        // a monitor never observes a freed region's data (reasons are
        // values; cross-proc data moved or froze, D14).
        let (monitors, links) = publish(reason);
        deliver(reason, &monitors, &links);
        bulk_free(&owned);
    }
}

/// Wholesale-free the ledger's regions (D10 Tier-1): the walk is over
/// the ledger, never the object graph — a cyclic 100 MB region costs
/// one arena free.
fn bulk_free(owned: &[usize]) {
    for &h in owned {
        // SAFETY: ledger invariant — every recorded handle is a live
        // region the quiescent tree can no longer touch; each is
        // freed exactly once (the ledger entry was removed above, so
        // the free hook finds nothing to double-account).
        unsafe { native::__wolf_rt_region_free(h as *mut core::ffi::c_void) };
    }
}

/// Deliver `reason` to every monitor, then walk the links
/// (`[conc.proc.2]`): abnormal reasons kill the partner — unless it
/// traps exits, then the reason arrives as a message. Deliveries are
/// non-blocking sends (a full mailbox never wedges a reaper).
fn deliver(reason: ProcExit, monitors: &[Arc<Chan>], links: &[u64]) {
    let word = reason.encode();
    for m in monitors {
        // Monitor channels are runtime-created with capacity 1 and
        // receive exactly one delivery — but never block on a
        // misbehaving receiver: the else-arm drops the delivery
        // rather than wedging the reaper.
        let _ = select(&[Arm::Send(m, word)], None, true);
    }
    if !reason.is_abnormal() {
        return;
    }
    for &partner in links {
        if partner == ROOT_DOMAIN {
            root_died(reason);
            continue;
        }
        let Ok(p) = live_proc(partner) else {
            continue; // already down
        };
        let trap = p.st.lock().unwrap().trap_exit.clone();
        match trap {
            // Trap converts the kill to a message (acceptance:
            // "unless trapped; trap converts to a message").
            Some(mb) => {
                let _ = select(&[Arm::Send(&mb, word)], None, true);
            }
            None => kill_mark(&p),
        }
    }
}

/// Begin kill teardown for `p` (`[conc.proc.kill]` step 1): mark the
/// proc and its scope killed and wake every blocking point. The
/// reaper completes steps 3–4.
fn kill_mark(p: &Arc<Proc>) {
    sched_point(SchedEvent::ProcKill { proc: p.id });
    {
        let mut st = p.st.lock().unwrap();
        if matches!(st.phase, Phase::Exited(_)) {
            return; // late kill: the reason is already decided
        }
        st.kill_requested = true;
    }
    p.scope.kill();
}

/// D68's containment (s132, the [`native::install_trap_containment`]
/// seam): a trap fired on the calling thread. If the calling task
/// runs inside a proc, the proc is the failure domain
/// (`[conc.proc.1]`): record `fault(kind)` — the first decided
/// teardown wins — begin the killed-proc sequence for the rest of the
/// tree (`[conc.proc.kill]`: no further user code, so no `defer`
/// below the boundary runs), retire the trapping task so the reaper's
/// join can quiesce, and PARK this thread for good. Parking is the
/// no-unwinding law made mechanism: compiled wolf frames are never
/// unwound (`[abi.native.nounwind]`) and never resumed either — the
/// measured containment cost is one held worker thread (and its
/// stack's high water) per contained trap, which the pool compensates
/// for like any indefinitely blocked task. RETURNS (and the process-
/// exit trap path proceeds unchanged) when the task runs in the root
/// domain — `main`'s tree, or a plain task outside any proc
/// (`[conc.proc.root]`: the root domain's trap is process death).
fn contain_trap(kind: i32) {
    let Some(pid) = current_proc() else { return };
    let Ok(p) = live_proc(pid) else { return };
    {
        let mut st = p.st.lock().unwrap();
        if !matches!(st.phase, Phase::Exited(_)) && !st.kill_requested && st.fault.is_none() {
            st.fault = Some(kind);
        }
    }
    kill_mark(&p);
    pool::finish_current_task(ExitReason::Killed);
    pool::park_after_contained_trap();
}

/// The root supervisor domain died abnormally (`[conc.proc.root]`):
/// run the killed-proc sequence for every live proc, then terminate
/// the process with the nonzero [`ROOT_DEATH_EXIT`]. Never returns
/// (except for the racing second caller, whose reaper simply finishes
/// while the first exits the process).
fn root_died(reason: ProcExit) {
    static DYING: AtomicBool = AtomicBool::new(false);
    if DYING.swap(true, SeqCst) {
        return; // one exit sequence is already running
    }
    eprintln!(
        "wolf-proc: root supervisor domain died ({}): killing live procs",
        match reason.kind() {
            0 => "normal",
            1 => "error",
            2 => "killed",
            3 => "cancelled",
            _ => "fault",
        }
    );
    let live: Vec<Arc<Proc>> = {
        let reg = registry().lock().unwrap();
        reg.slots
            .iter()
            .filter_map(|s| match s {
                SlotState::Live(p) => Some(p.clone()),
                SlotState::Exited(_) => None,
            })
            .collect()
    };
    for p in &live {
        kill_mark(p);
    }
    for p in &live {
        wait_exited(p);
    }
    std::process::exit(ROOT_DEATH_EXIT);
}

/// Block (with compensation) until `p`'s reaper records the reason.
fn wait_exited(p: &Arc<Proc>) -> ProcExit {
    pool::blocking(|| {
        let mut st = p.st.lock().unwrap();
        loop {
            if let Phase::Exited(r) = st.phase {
                return r;
            }
            st = p.cv.wait(st).unwrap();
        }
    })
}

// ---- the public verbs -------------------------------------------------------

/// Kill `proc` (`[conc.proc.kill]`): teardown without user code, then
/// bulk-free, then delivery. Blocks until the sequence completes —
/// except for self-kill, which returns and lets the teardown reach
/// the caller at its next blocking point. Late kills on an exited
/// proc are no-ops (the reason is already decided).
pub fn kill(proc: u64) -> Result<(), ProcErr> {
    let p = match live_proc(proc) {
        Ok(p) => p,
        // A recorded exit is not stale — kill-after-exit is a no-op.
        Err(e) => return exited_reason(proc).map(|_| ()).map_err(|_| e),
    };
    kill_mark(&p);
    if current_proc() == Some(proc) {
        return Ok(()); // self-kill: the unwind reaches us shortly
    }
    wait_exited(&p);
    Ok(())
}

/// Deliver structured cancellation to `proc` (`[conc.proc.cancel]`):
/// cooperative at blocking points, defers run. Returns immediately;
/// the exit reason (cancelled — or normal(value) if the body still
/// completes) arrives at monitors as usual.
pub fn cancel(proc: u64) -> Result<(), ProcErr> {
    let p = match live_proc(proc) {
        Ok(p) => p,
        Err(e) => return exited_reason(proc).map(|_| ()).map_err(|_| e),
    };
    p.scope.cancel();
    Ok(())
}

/// Monitor `proc`: returns a fresh capacity-1 channel that delivers
/// the exit reason as an encoded word ([`ProcExit::decode`]) —
/// failure detection with reason, asymmetric. Monitoring an already
/// exited proc delivers immediately (the observer always learns the
/// fate).
pub fn monitor(proc: u64) -> Result<Arc<Chan>, ProcErr> {
    let ch = Chan::new(1);
    match live_proc(proc) {
        Ok(p) => {
            let mut st = p.st.lock().unwrap();
            if let Phase::Exited(r) = st.phase {
                drop(st);
                let _ = select(&[Arm::Send(&ch, r.encode())], None, true);
            } else {
                st.monitors.push(ch.clone());
            }
            Ok(ch)
        }
        Err(_) => {
            let r = exited_reason(proc)?;
            let _ = select(&[Arm::Send(&ch, r.encode())], None, true);
            Ok(ch)
        }
    }
}

/// Link `a` and `b` symmetrically and idempotently per pair
/// (`[conc.proc.link.pair]`; `w.link()`'s one-arg spelling passes the
/// calling task's domain — [`current_proc`] or [`ROOT_DOMAIN`]).
/// Either side's ABNORMAL exit kills the other (`[conc.proc.2]`),
/// which the partner may trap ([`set_trap_exit`]). Linking to an
/// already-abnormally-exited proc propagates immediately (the Erlang
/// posture: no lost exit signals).
pub fn link(a: u64, b: u64) -> Result<(), ProcErr> {
    if a == b {
        return Ok(());
    }
    // Resolve both sides first; a stale id is the caller's fault.
    let ra = resolve(a)?;
    let rb = resolve(b)?;
    // Dead-partner propagation, both directions (an exited-normal
    // partner just never fires — linking to it is a quiet no-op).
    if let Side::Exited(reason) = ra
        && reason.is_abnormal()
    {
        propagate_link_death(reason, b);
        return Ok(());
    }
    if let Side::Exited(reason) = rb
        && reason.is_abnormal()
    {
        propagate_link_death(reason, a);
        return Ok(());
    }
    if let Side::Live(p) = &ra {
        let mut st = p.st.lock().unwrap();
        if !st.links.contains(&b) {
            st.links.push(b);
        }
    }
    if let Side::Live(p) = &rb {
        let mut st = p.st.lock().unwrap();
        if !st.links.contains(&a) {
            st.links.push(a);
        }
    }
    Ok(())
}

/// One side of a link: the root domain, a live proc, or a recorded
/// exit.
enum Side {
    Root,
    Live(Arc<Proc>),
    Exited(ProcExit),
}

fn resolve(id: u64) -> Result<Side, ProcErr> {
    if id == ROOT_DOMAIN {
        return Ok(Side::Root);
    }
    match live_proc(id) {
        Ok(p) => Ok(Side::Live(p)),
        Err(_) => exited_reason(id).map(Side::Exited),
    }
}

fn propagate_link_death(reason: ProcExit, target: u64) {
    if target == ROOT_DOMAIN {
        root_died(reason);
        return;
    }
    if let Ok(p) = live_proc(target) {
        let trap = p.st.lock().unwrap().trap_exit.clone();
        match trap {
            Some(mb) => {
                let _ = select(&[Arm::Send(&mb, reason.encode())], None, true);
            }
            None => kill_mark(&p),
        }
    }
}

/// Trap exits for `proc`: linked partners' deaths arrive on `mailbox`
/// as encoded reason words instead of kills. Supervisors are exactly
/// this plus a receive loop (Armstrong: supervisors are just code
/// over links).
pub fn set_trap_exit(proc: u64, mailbox: Arc<Chan>) -> Result<(), ProcErr> {
    let p = live_proc(proc)?;
    p.st.lock().unwrap().trap_exit = Some(mailbox);
    Ok(())
}

/// The recorded exit reason of a proc that already exited.
fn exited_reason(id: u64) -> Result<ProcExit, ProcErr> {
    let reg = registry().lock().unwrap();
    match slot_index(id).and_then(|i| reg.slots.get(i)) {
        Some(SlotState::Exited(r)) => Ok(*r),
        _ => Err(ProcErr::Stale),
    }
}

/// The calling task's proc, if it runs inside one (scope-chain
/// attribution to the proc-root scope). `None` means the root domain.
pub fn current_proc() -> Option<u64> {
    let scope = pool::current_scope()?;
    let root = scope.root();
    let reg = REGISTRY.get()?;
    reg.lock().unwrap().scope_to_proc.get(&root.id()).copied()
}

// ---- the region ledger ------------------------------------------------------

fn ledger_on_new(handle: usize) {
    let Some(pid) = current_proc() else { return };
    let mut reg = registry().lock().unwrap();
    reg.region_owner.insert(handle, pid);
    if let Some(set) = reg.proc_regions.get_mut(&pid) {
        set.insert(handle);
    }
}

fn ledger_on_free(handle: usize, _bytes: usize) {
    let Some(reg) = REGISTRY.get() else { return };
    let mut reg = reg.lock().unwrap();
    if let Some(pid) = reg.region_owner.remove(&handle)
        && let Some(set) = reg.proc_regions.get_mut(&pid)
    {
        set.remove(&handle);
    }
}

/// Ownership left the donor at a moving send
/// (`__wolf_rt_region_transfer`'s ledger half: "the ledger moves with
/// the word"). Until the receiver adopts it the region is in flight —
/// owned by the channel, exactly the resource posture
/// `[conc.proc.kill]` prescribes for cross-proc resources.
pub(crate) fn region_transferred(handle: usize) {
    let Some(reg) = REGISTRY.get() else { return };
    let mut reg = reg.lock().unwrap();
    if let Some(pid) = reg.region_owner.remove(&handle)
        && let Some(set) = reg.proc_regions.get_mut(&pid)
    {
        set.remove(&handle);
    }
}

/// Attach `handle` to the calling proc's ledger — the receive side of
/// a region move. Frozen ahead of consumption: codegen's recv
/// lowering will call this on region-typed payloads; the Rust
/// scaffolding calls it directly.
///
/// # Safety
///
/// `handle` must be a live region handle the caller just received
/// over a channel (it is in-flight: no ledger owns it).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_adopt(
    handle: *mut core::ffi::c_void,
) -> *mut core::ffi::c_void {
    ledger_on_new(handle as usize);
    handle
}

/// One proc's ledger: (region count, total bytes) — the per-proc
/// resource accounting Erlang never had (thesis ~L7235–7244);
/// s54's debugger and the s36 recorder read it here. An exited or
/// stale proc reads (0, 0): its ledger was bulk-freed.
pub fn proc_ledger(proc: u64) -> (u64, u64) {
    let Some(reg) = REGISTRY.get() else {
        return (0, 0);
    };
    let reg = reg.lock().unwrap();
    let Some(set) = reg.proc_regions.get(&proc) else {
        return (0, 0);
    };
    let count = set.len() as u64;
    let bytes: u64 = set
        .iter()
        // SAFETY: ledger invariant — recorded handles are live, and
        // the registry lock holds off every free (the free hook locks
        // it before the arena drops).
        .map(|&h| unsafe { native::region_bytes(h as *mut core::ffi::c_void) } as u64)
        .sum();
    (count, bytes)
}

/// Every proc's outstanding ledger, summed: (count, bytes). The
/// crash-cleanup acceptance drives this back to its baseline.
pub fn ledger_live() -> (u64, u64) {
    let Some(reg) = REGISTRY.get() else {
        return (0, 0);
    };
    let reg = reg.lock().unwrap();
    let count = reg.region_owner.len() as u64;
    let bytes: u64 = reg
        .region_owner
        .keys()
        // SAFETY: as in [`proc_ledger`].
        .map(|&h| unsafe { native::region_bytes(h as *mut core::ffi::c_void) } as u64)
        .sum();
    (count, bytes)
}

// ---- handler context (Target 3) ----------------------------------------------

thread_local! {
    static HANDLER_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// True while a proc message handler runs on this thread —
/// `pool::blocking`'s debug-assert reads it (handlers are atomic and
/// non-blocking, `[conc.chan.mailbox]`).
pub(crate) fn in_handler() -> bool {
    HANDLER_DEPTH.with(std::cell::Cell::get) > 0
}

/// Run one message handler atomically: `f` runs to completion without
/// interleaving on proc state, and any runtime-owned blocking
/// primitive inside it debug-asserts (the checker rejects the static
/// cases; this is the dynamic defense in depth). The receive-loop
/// lowering brackets handler bodies with this; long work is spawned
/// onto the proc's scope and completes via another message.
pub fn with_handler<R>(f: impl FnOnce() -> R) -> R {
    struct Depth;
    impl Drop for Depth {
        fn drop(&mut self) {
            HANDLER_DEPTH.with(|d| d.set(d.get() - 1));
        }
    }
    HANDLER_DEPTH.with(|d| d.set(d.get() + 1));
    let _g = Depth;
    f()
}

// ---- the minimal supervisor (Target 5) ----------------------------------------

/// Restart policy: at most `max_restarts` abnormal exits inside any
/// sliding `window` before the supervisor gives up.
#[derive(Debug, Clone, Copy)]
pub struct RestartPolicy {
    /// Abnormal exits tolerated within the window.
    pub max_restarts: u32,
    /// The sliding window.
    pub window: Duration,
}

/// What [`supervise`] concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Supervised {
    /// The child completed (normal or orderly-cancelled); its final
    /// reason.
    Done(ProcExit),
    /// N-times-in-window exhausted: the give-up ESCALATES by
    /// returning to the caller — one level up, ultimately the root
    /// supervisor (richer strategy trees are s39 stdlib; flat beats
    /// deep).
    GaveUp {
        /// The child's final abnormal reason.
        last: ProcExit,
        /// Restarts spent before giving up.
        restarts: u32,
    },
}

/// The minimal restart-capable supervisor: spawn the child via
/// `factory`, monitor it, restart on abnormal exit within
/// [`RestartPolicy`], give up and escalate past the budget. Runs on
/// the calling task (supervisors are just code over links/monitors).
pub fn supervise(
    name: &str,
    policy: RestartPolicy,
    mut factory: impl FnMut() -> Box<dyn FnOnce(&TaskCtx) -> ProcOutcome + Send>,
) -> Supervised {
    let mut restarts: Vec<Instant> = Vec::new();
    loop {
        let child = spawn_proc(name, factory());
        let m = monitor(child).expect("freshly spawned child");
        let reason = match m.recv() {
            Ok(word) => ProcExit::decode(word),
            // The supervisor itself was cancelled: stop supervising;
            // the child keeps its own fate.
            Err(_) => return Supervised::Done(ProcExit::Cancelled),
        };
        if !reason.is_abnormal() {
            return Supervised::Done(reason);
        }
        let now = Instant::now();
        restarts.retain(|t| now.duration_since(*t) < policy.window);
        if restarts.len() as u32 >= policy.max_restarts {
            return Supervised::GaveUp {
                last: reason,
                restarts: restarts.len() as u32,
            };
        }
        restarts.push(now);
    }
}

// ---- the C entry surface -------------------------------------------------
//
// Frozen ahead of consumption (the s28/s32/s33 pattern): codegen's
// proc lowering will emit calls to these symbols once c05 typing and
// the conc lowering land; the surface is fixed here first. Stale ids
// fault cleanly as trap(stale-handle) (X5). A no-proc binary carries
// none of these symbols (D15 — driver test).

use core::ffi::c_void;

/// Spawn `entry(env)` as a proc under the root supervisor; returns
/// the proc id. `entry` returns 0 for normal completion (value 0 in
/// v0 — typed completion values ride the s39 row work) or an error
/// tag (`[abi.err]`). Kill teardown for compiled bodies is deferred
/// to the codegen sprint that lowers the no-defer branch — C frames
/// are never unwound (`[conc.cancel.c]`), so blocking points inside
/// `entry` surface the cancelled status value instead.
///
/// # Safety
///
/// `entry` must be callable with `env` (which moves — D14); `name`
/// must address `name_len` readable bytes when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_proc_spawn(
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
        // Capture the SendPtr wrapper whole (disjoint capture would
        // otherwise grab the raw, non-Send field).
        let moved_env = env;
        // C ABI frames below: suppress the kill-teardown unwind for
        // their whole extent (`[conc.cancel.c]`).
        let tag = pool::suppress_kill_unwind(|| {
            // SAFETY: caller contract — lowered proc body + moved env.
            unsafe { entry(moved_env.0) }
        });
        if tag == 0 {
            ProcOutcome::Value(0)
        } else {
            ProcOutcome::Fail { tag }
        }
    })
}

/// The calling task's proc id, or [`ROOT_DOMAIN`] (0) in the root
/// domain.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_proc_self() -> u64 {
    current_proc().unwrap_or(ROOT_DOMAIN)
}

/// Monitor `proc`: returns a fresh channel handle delivering the
/// encoded exit reason word. Stale ids trap (stale-handle).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_proc_monitor(proc: u64) -> *mut c_void {
    match monitor(proc) {
        Ok(ch) => Arc::into_raw(ch).cast_mut().cast(),
        Err(ProcErr::Stale) => native::__wolf_rt_trap(native::trap_code::STALE_HANDLE),
    }
}

/// Link `a` and `b` (`b == 0` spells `w.link()`: the caller's domain,
/// proc or root — `[conc.proc.link.pair]`). Stale ids trap
/// (stale-handle).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_proc_link(a: u64, b: u64) {
    let b = if b == ROOT_DOMAIN {
        current_proc().unwrap_or(ROOT_DOMAIN)
    } else {
        b
    };
    if link(a, b).is_err() {
        native::__wolf_rt_trap(native::trap_code::STALE_HANDLE);
    }
}

/// Kill `proc` (`[conc.proc.kill]`). Stale ids trap (stale-handle);
/// late kills on an exited proc are no-ops.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_proc_kill(proc: u64) {
    if kill(proc).is_err() {
        native::__wolf_rt_trap(native::trap_code::STALE_HANDLE);
    }
}

/// Cancel `proc` (`[conc.proc.cancel]`). Stale ids trap.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_proc_cancel(proc: u64) {
    if cancel(proc).is_err() {
        native::__wolf_rt_trap(native::trap_code::STALE_HANDLE);
    }
}

// ---- acceptance tests ----------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::ChanErr;
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

    /// Receive one exit reason from a monitor channel, decoded.
    /// Bounded wait for a condition another thread makes true. Tests
    /// that used to sample once after a sleep were the load-flaky
    /// ones (#50): a red gate must mean a red pin, so this WAITS for
    /// the event and fails, naming it, only when it never arrives.
    fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !cond() {
            assert!(Instant::now() < deadline, "{what} never happened");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn recv_reason(m: &Arc<Chan>) -> ProcExit {
        ProcExit::decode(m.recv().expect("monitor delivers"))
    }

    /// The exit-reason taxonomy, pinned against `[conc.proc.exit]`'s
    /// s05 enumeration (acceptance: "reason taxonomy snapshot-tested")
    /// — and the v0 wire packing round-trips, sign included.
    #[test]
    fn exit_reason_taxonomy_and_packing_snapshot() {
        let reasons = [
            ProcExit::Normal { value: 7 },
            ProcExit::Error { tag: -42 },
            ProcExit::Killed,
            ProcExit::Cancelled,
            ProcExit::Fault {
                kind: native::trap_code::ALLOC_CONTRACT,
            },
        ];
        let shapes: Vec<String> = reasons.iter().map(|r| format!("{r:?}")).collect();
        assert_eq!(
            shapes,
            [
                "Normal { value: 7 }",
                "Error { tag: -42 }",
                "Killed",
                "Cancelled",
                "Fault { kind: 9 }",
            ]
        );
        for (i, r) in reasons.iter().enumerate() {
            assert_eq!(r.kind(), i as u8);
            assert_eq!(ProcExit::decode(r.encode()), *r);
        }
        // The packed words themselves are contract (low byte = kind).
        // fault(alloc-contract)'s word is the one the WIR reason
        // predicates compare against ([conc.proc.exit] mapping, D68):
        // trap kind 9 in the payload, reason class 4 in the low byte.
        assert_eq!(ProcExit::Normal { value: 7 }.encode(), (7 << 8));
        assert_eq!(ProcExit::Killed.encode(), 2);
        assert_eq!(
            ProcExit::Fault {
                kind: native::trap_code::ALLOC_CONTRACT
            }
            .encode(),
            (9 << 8) | 4
        );
    }

    /// Monitor delivers `normal(value)`; the observer outlives the
    /// observed, and a LATE monitor still learns the fate.
    #[test]
    fn monitor_normal_and_late_monitor() {
        let w = spawn_proc("worker", |_| ProcOutcome::Value(7));
        let m = monitor(w).unwrap();
        assert_eq!(recv_reason(&m), ProcExit::Normal { value: 7 });
        // Late monitor: the proc has exited; the tombstone delivers.
        let late = monitor(w).unwrap();
        assert_eq!(recv_reason(&late), ProcExit::Normal { value: 7 });
    }

    /// Crash-cleanup (acceptance): a proc plants regions, then
    /// crashes with an error value. The monitor receives the planted
    /// `error(tag)`, and the region ledger bulk-frees back to its
    /// baseline (walked by ledger, not by object graph).
    #[test]
    fn crash_frees_ledger_and_delivers_planted_error() {
        let (base_count, base_bytes) = ledger_live();
        let probed = Arc::new(Mutex::new((0u64, 0u64)));
        let probe = probed.clone();
        let w = spawn_proc("crasher", move |_| {
            for _ in 0..3 {
                let h = native::__wolf_rt_region_new();
                // SAFETY: fresh live handle; the proc ledger owns it
                // from here (bulk-free reclaims it on exit).
                unsafe {
                    native::__wolf_rt_region_alloc(h, 4096);
                }
            }
            *probe.lock().unwrap() = proc_ledger(current_proc().unwrap());
            ProcOutcome::Fail { tag: 42 }
        });
        let m = monitor(w).unwrap();
        assert_eq!(recv_reason(&m), ProcExit::Error { tag: 42 });
        // Ledger accounting saw all three regions and their bytes.
        let (count, bytes) = *probed.lock().unwrap();
        assert_eq!(count, 3);
        assert!(bytes >= 3 * 4096, "ledger bytes: {bytes}");
        // Crash path is deliver-then-free: THIS proc's ledger empties
        // after delivery, so wait for it. `ledger_live` is process-
        // global and a parallel sibling can hold regions, so the
        // global check is "back to at most baseline", waited for.
        wait_until("this proc's regions being freed", || {
            proc_ledger(w) == (0, 0)
        });
        wait_until("the global ledger returning to baseline", || {
            let (c, b) = ledger_live();
            c <= base_count && b <= base_bytes
        });
    }

    /// THE defer law, kill half (`[conc.proc.kill]`,
    /// `proc_kill_defers.lu`'s runtime twin): a killed proc's blocked
    /// task never runs another line of user code — the defer
    /// stand-in after the blocking point stays unreached — and its
    /// regions free wholesale BEFORE the reason delivers (kill
    /// ordering: free, then deliver).
    #[test]
    fn killed_proc_skips_defers_and_frees_regions() {
        let (base_count, _) = ledger_live();
        let defer_ran = Arc::new(AtomicBool::new(false));
        let blocked = Arc::new(Mutex::new(None::<Arc<Chan>>));
        let (dr, bl) = (defer_ran.clone(), blocked.clone());
        let w = spawn_proc("sleeper", move |_| {
            let h = native::__wolf_rt_region_new();
            // SAFETY: fresh live handle, proc-owned.
            unsafe {
                native::__wolf_rt_region_alloc(h, 1024);
            }
            let ch = Chan::new(0);
            *bl.lock().unwrap() = Some(ch.clone());
            let _ = ch.recv(); // killed here: recv never returns
            dr.store(true, SeqCst); // the defer stand-in — must NOT run
            ProcOutcome::Value(1)
        });
        // Wait until the proc is actually parked at its recv.
        let deadline = Instant::now() + Duration::from_secs(10);
        while blocked.lock().unwrap().is_none() {
            assert!(Instant::now() < deadline, "proc never reached its recv");
            std::thread::sleep(Duration::from_millis(2));
        }
        std::thread::sleep(Duration::from_millis(20)); // let it park
        let m = monitor(w).unwrap();
        kill(w).unwrap();
        assert_eq!(recv_reason(&m), ProcExit::Killed);
        // Kill order is free-then-deliver: by delivery THIS proc's
        // ledger is already empty — that is the ordering under test,
        // and it needs no wait.
        assert_eq!(proc_ledger(w), (0, 0));
        // `ledger_live` is process-global. Under a parallel `cargo
        // test` a sibling test's proc may hold a region at this
        // instant, so the global count is not a same-instant
        // assertion (#50: this line was the flaky one). Wait for it.
        wait_until("the global ledger returning to baseline", || {
            ledger_live().0 <= base_count
        });
        assert!(
            !defer_ran.load(SeqCst),
            "user code ran after the kill ([conc.proc.kill]: defers do not run)"
        );
    }

    /// THE defer law, cancel half (`[conc.proc.cancel]` /
    /// `[conc.cancel.defer]`, `proc_cancel_defers.lu`'s runtime
    /// twin): cancellation surfaces as a VALUE at the blocking point,
    /// the defer stand-in after it DOES run, and the reason is
    /// `cancelled`.
    #[test]
    fn cancelled_proc_runs_defers() {
        let defer_ran = Arc::new(AtomicBool::new(false));
        let blocked = Arc::new(Mutex::new(None::<Arc<Chan>>));
        let (dr, bl) = (defer_ran.clone(), blocked.clone());
        let w = spawn_proc("sleeper", move |_| {
            let ch = Chan::new(0);
            *bl.lock().unwrap() = Some(ch.clone());
            match ch.recv() {
                Err(ChanErr::Cancelled) => {
                    dr.store(true, SeqCst); // the defer stand-in — MUST run
                    ProcOutcome::Cancelled
                }
                other => panic!("expected the cancel value, got {other:?}"),
            }
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while blocked.lock().unwrap().is_none() {
            assert!(Instant::now() < deadline, "proc never reached its recv");
            std::thread::sleep(Duration::from_millis(2));
        }
        let m = monitor(w).unwrap();
        cancel(w).unwrap();
        assert_eq!(recv_reason(&m), ProcExit::Cancelled);
        assert!(
            defer_ran.load(SeqCst),
            "defers must run under cancellation ([conc.cancel.defer])"
        );
    }

    /// `[conc.proc.cancel]`'s second half: a proc that completes its
    /// value despite cancellation keeps `normal(value)`.
    #[test]
    fn cancelled_proc_completing_keeps_normal() {
        let blocked = Arc::new(Mutex::new(None::<Arc<Chan>>));
        let bl = blocked.clone();
        let w = spawn_proc("stubborn", move |_| {
            let ch = Chan::new(0);
            *bl.lock().unwrap() = Some(ch.clone());
            let _ = ch.recv(); // cancel surfaces here as a value
            ProcOutcome::Value(9) // ...and the body still completes
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while blocked.lock().unwrap().is_none() {
            assert!(Instant::now() < deadline, "proc never reached its recv");
            std::thread::sleep(Duration::from_millis(2));
        }
        let m = monitor(w).unwrap();
        cancel(w).unwrap();
        assert_eq!(recv_reason(&m), ProcExit::Normal { value: 9 });
    }

    /// A panicking proc task becomes `error(TRAP_ERROR_TAG)` — trap
    /// containment at the proc boundary, not process death.
    #[test]
    fn proc_trap_becomes_error_reason() {
        let w = spawn_proc("boom", |_| -> ProcOutcome {
            panic!("contained at the proc boundary")
        });
        let m = monitor(w).unwrap();
        assert_eq!(
            recv_reason(&m),
            ProcExit::Error {
                tag: TRAP_ERROR_TAG
            }
        );
    }

    /// D68's whole containment path, rt-level (s132, #187): a proc
    /// breaches its region cap — `__wolf_rt_region_alloc` fires
    /// `trap(alloc-contract)` at the site, the containment seam
    /// contains it at the proc boundary, and the monitor reads
    /// `fault(alloc-contract)` at the join. At-cap-exactly is not the
    /// breach (the 64-byte charge against cap 64 succeeds); the next
    /// charge is. Fault teardown is KILL-ordered ([mem.region.cap.3]):
    /// by delivery the breaching proc's ledger is already empty — the
    /// same no-wait assertion the kill test makes.
    #[test]
    fn region_cap_breach_contained_as_fault_at_join() {
        let w = spawn_proc("breacher", |_| {
            let h = native::__wolf_rt_region_new();
            // SAFETY: fresh live handle, proc-owned; set_cap precedes
            // any allocation (the lowering's creation-time contract).
            unsafe {
                native::__wolf_rt_region_set_cap(h, 64);
                let at_cap = native::__wolf_rt_region_alloc(h, 64);
                assert!(!at_cap.is_null(), "cap == charged is not a breach");
                // The next byte IS the breach: traps, never returns.
                native::__wolf_rt_region_alloc(h, 1);
            }
            unreachable!("the breach must trap at the allocating site");
        });
        let m = monitor(w).unwrap();
        assert_eq!(
            recv_reason(&m),
            ProcExit::Fault {
                kind: native::trap_code::ALLOC_CONTRACT
            }
        );
        // Free-then-deliver: the reclaimed charge precedes the reason.
        assert_eq!(proc_ledger(w), (0, 0));
    }

    /// The cap's domain half ([mem.region.cap.2]): a negative budget
    /// is the same allocation-contract violation, at the CREATING
    /// site — and inside a proc it is contained the same way.
    #[test]
    fn negative_cap_contained_as_fault() {
        let w = spawn_proc("neg-cap", |_| {
            let h = native::__wolf_rt_region_new();
            // SAFETY: fresh live handle.
            unsafe { native::__wolf_rt_region_set_cap(h, -1) };
            unreachable!("a negative cap must trap at the creating site");
        });
        let m = monitor(w).unwrap();
        assert_eq!(
            recv_reason(&m),
            ProcExit::Fault {
                kind: native::trap_code::ALLOC_CONTRACT
            }
        );
    }

    /// A faulted partner is a dead partner: `fault(kind)` propagates
    /// over links like every abnormal reason (`[conc.proc.2]`).
    #[test]
    fn fault_propagates_over_links() {
        let blocked = Arc::new(Mutex::new(None::<Arc<Chan>>));
        let bl = blocked.clone();
        let b = spawn_proc("partner", move |_| {
            let ch = Chan::new(0);
            *bl.lock().unwrap() = Some(ch.clone());
            let _ = ch.recv();
            ProcOutcome::Value(0)
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while blocked.lock().unwrap().is_none() {
            assert!(Instant::now() < deadline, "partner never parked");
            std::thread::sleep(Duration::from_millis(2));
        }
        let a = spawn_proc("faulter", |_| {
            let h = native::__wolf_rt_region_new();
            // SAFETY: fresh live handle.
            unsafe {
                native::__wolf_rt_region_set_cap(h, 0);
                // cap 0: every charge breaches ([mem.region.cap.2]).
                native::__wolf_rt_region_alloc(h, 1);
            }
            unreachable!("cap 0 must breach on the first charge");
        });
        let ma = monitor(a).unwrap();
        assert_eq!(
            recv_reason(&ma),
            ProcExit::Fault {
                kind: native::trap_code::ALLOC_CONTRACT
            }
        );
        let mb = monitor(b).unwrap();
        link(a, b).unwrap(); // a already faulted: propagates now
        assert_eq!(recv_reason(&mb), ProcExit::Killed);
    }

    /// Link test (acceptance, `[conc.proc.link.pair]`): killing one
    /// of a linked pair takes both down...
    #[test]
    fn link_pair_shares_fate() {
        let sleeper = |bl: Arc<Mutex<Option<Arc<Chan>>>>| {
            move |_: &TaskCtx| {
                let ch = Chan::new(0);
                *bl.lock().unwrap() = Some(ch.clone());
                let _ = ch.recv();
                ProcOutcome::Value(0)
            }
        };
        let (bla, blb) = (Arc::new(Mutex::new(None)), Arc::new(Mutex::new(None)));
        let a = spawn_proc("a", sleeper(bla.clone()));
        let b = spawn_proc("b", sleeper(blb.clone()));
        link(a, b).unwrap();
        link(a, b).unwrap(); // idempotent per pair
        let deadline = Instant::now() + Duration::from_secs(10);
        while bla.lock().unwrap().is_none() || blb.lock().unwrap().is_none() {
            assert!(Instant::now() < deadline, "procs never parked");
            std::thread::sleep(Duration::from_millis(2));
        }
        let m = monitor(b).unwrap();
        kill(a).unwrap();
        assert_eq!(recv_reason(&m), ProcExit::Killed);
    }

    /// ...unless trapped: trap converts the link kill to a message,
    /// and the trapping partner lives on to complete normally.
    #[test]
    fn link_trap_converts_kill_to_message() {
        let blocked = Arc::new(Mutex::new(None::<Arc<Chan>>));
        let bl = blocked.clone();
        let a = spawn_proc("doomed", move |_| {
            let ch = Chan::new(0);
            *bl.lock().unwrap() = Some(ch.clone());
            let _ = ch.recv();
            ProcOutcome::Value(0)
        });
        let mailbox = Chan::new(1);
        let mb = mailbox.clone();
        let b = spawn_proc("trapper", move |_| {
            // The supervisor shape: receive the partner's death as a
            // message, then finish normally.
            match mb.recv() {
                Ok(word) => match ProcExit::decode(word) {
                    ProcExit::Killed => ProcOutcome::Value(77),
                    other => ProcOutcome::Fail {
                        tag: i64::from(other.kind()),
                    },
                },
                Err(_) => ProcOutcome::Fail { tag: -9 },
            }
        });
        set_trap_exit(b, mailbox).unwrap();
        link(a, b).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while blocked.lock().unwrap().is_none() {
            assert!(Instant::now() < deadline, "proc never parked");
            std::thread::sleep(Duration::from_millis(2));
        }
        let m = monitor(b).unwrap();
        kill(a).unwrap();
        assert_eq!(recv_reason(&m), ProcExit::Normal { value: 77 });
    }

    /// Supervisor test (acceptance): a crash-looping child restarts
    /// N times within the window, then the give-up escalates to the
    /// caller; a child that recovers completes.
    #[test]
    fn supervisor_restarts_then_gives_up() {
        let policy = RestartPolicy {
            max_restarts: 3,
            window: Duration::from_secs(600),
        };
        let spawns = Arc::new(Mutex::new(0u32));
        let counter = spawns.clone();
        let verdict = supervise("crashloop", policy, move || {
            *counter.lock().unwrap() += 1;
            Box::new(|_: &TaskCtx| ProcOutcome::Fail { tag: 5 })
        });
        assert_eq!(
            verdict,
            Supervised::GaveUp {
                last: ProcExit::Error { tag: 5 },
                restarts: 3
            }
        );
        // Initial spawn + 3 restarts.
        assert_eq!(*spawns.lock().unwrap(), 4);

        // Recovery: fail once, then complete.
        let attempts = Arc::new(Mutex::new(0u32));
        let counter = attempts.clone();
        let verdict = supervise("flaky", policy, move || {
            let mut n = counter.lock().unwrap();
            *n += 1;
            let fail = *n == 1;
            Box::new(move |_: &TaskCtx| {
                if fail {
                    ProcOutcome::Fail { tag: 1 }
                } else {
                    ProcOutcome::Value(11)
                }
            })
        });
        assert_eq!(verdict, Supervised::Done(ProcExit::Normal { value: 11 }));
    }

    /// Handler atomicity (acceptance): a handler calling a blocking
    /// primitive trips the debug-assert (surfacing as the task's
    /// Panicked reason); the spawn-and-message pattern passes the
    /// same scenario.
    #[test]
    #[cfg(debug_assertions)]
    fn handler_blocking_debug_asserts_and_spawn_pattern_passes() {
        use super::super::scope;
        // The violation: block inside a handler.
        let r = scope("handler-violation", |s| {
            s.spawn("handler", |_| {
                let ch = Chan::new(0);
                with_handler(|| {
                    let _ = ch.recv(); // debug-asserts in blocking()
                });
                ExitReason::Normal
            });
        });
        assert_eq!(r, Err(ExitReason::Panicked));

        // The sanctioned shape: the handler spawns the blocking work
        // onto the scope; completion arrives as a message.
        let done = Chan::new(1);
        let tx = done.clone();
        let r = scope("handler-spawns", |s| {
            let s2 = s.clone();
            s.spawn("handler", move |_| {
                with_handler(|| {
                    // Atomic, non-blocking: spawn the long work.
                    let tx = tx.clone();
                    s2.spawn("worker", move |_| {
                        tx.send(1).unwrap();
                        ExitReason::Normal
                    });
                });
                ExitReason::Normal
            });
        });
        assert!(r.is_ok());
        assert_eq!(done.recv(), Ok(1));
    }

    /// Stale ids fault cleanly (X5 spirit): a forged id is an error
    /// value on the Rust surface (the C surface traps stale-handle).
    #[test]
    fn stale_ids_fault_cleanly() {
        let forged = pack_id(1, 0xFFFF_FF00);
        assert_eq!(kill(forged), Err(ProcErr::Stale));
        assert_eq!(cancel(forged), Err(ProcErr::Stale));
        assert!(monitor(forged).is_err());
        assert_eq!(link(forged, ROOT_DOMAIN), Err(ProcErr::Stale));
        // Wrong generation: same slot space, different word.
        let wrong_gen = 7u64 << 32;
        assert_eq!(kill(wrong_gen), Err(ProcErr::Stale));
    }

    /// Late kill on an exited proc is a no-op: the recorded reason
    /// stands.
    #[test]
    fn late_kill_is_noop() {
        let w = spawn_proc("done", |_| ProcOutcome::Value(3));
        let m = monitor(w).unwrap();
        assert_eq!(recv_reason(&m), ProcExit::Normal { value: 3 });
        kill(w).unwrap();
        let m2 = monitor(w).unwrap();
        assert_eq!(recv_reason(&m2), ProcExit::Normal { value: 3 });
    }

    /// The region ledger moves with the word (`[conc.chan.move]`
    /// ledger half): transfer detaches from the donor; adopt attaches
    /// to the receiver.
    #[test]
    fn ledger_moves_with_region_transfer() {
        let results = Chan::new(1);
        let tx = results.clone();
        let donor = spawn_proc("donor", move |_| {
            let h = native::__wolf_rt_region_new();
            // SAFETY: live handle, moved through the channel.
            unsafe {
                native::__wolf_rt_region_alloc(h, 512);
                tx.send_region(h).unwrap();
            }
            ProcOutcome::Value(0)
        });
        let dm = monitor(donor).unwrap();
        assert_eq!(recv_reason(&dm), ProcExit::Normal { value: 0 });
        // The donor exited; its ledger did NOT free the in-flight
        // region (ownership left at the send).
        let word = results.recv().unwrap();
        let h = pool::SendPtr(word as *mut core::ffi::c_void);
        let receiver = spawn_proc("receiver", move |_| {
            let moved = h;
            // SAFETY: the in-flight handle just received; adopt then
            // let proc exit bulk-free it.
            unsafe {
                __wolf_rt_region_adopt(moved.0);
            }
            let (count, _) = proc_ledger(current_proc().unwrap());
            ProcOutcome::Value(i64::try_from(count).unwrap())
        });
        let rm = monitor(receiver).unwrap();
        // The receiver's ledger counted the adopted region...
        assert_eq!(recv_reason(&rm), ProcExit::Normal { value: 1 });
        // ...and bulk-free on its exit reclaimed it (poll: crash path
        // frees after delivery).
        let deadline = Instant::now() + Duration::from_secs(10);
        while proc_ledger(receiver) != (0, 0) {
            assert!(Instant::now() < deadline, "adopted region never freed");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// The seam sees the proc kinds (spec/07 append): proc.spawn,
    /// proc.kill, proc.exit with the reason class.
    #[test]
    fn seam_observes_proc_events() {
        use super::super::hooks::test_hook;
        let _serial = test_hook::SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let seen: Arc<Mutex<Vec<SchedEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        test_hook::set_test_hook(Some(Box::new(move |ev| {
            if matches!(
                ev,
                SchedEvent::ProcSpawn { .. }
                    | SchedEvent::ProcKill { .. }
                    | SchedEvent::ProcExit { .. }
            ) {
                sink.lock().unwrap().push(*ev);
            }
        })));
        let blocked = Arc::new(Mutex::new(None::<Arc<Chan>>));
        let bl = blocked.clone();
        let w = spawn_proc("observed", move |_| {
            let ch = Chan::new(0);
            *bl.lock().unwrap() = Some(ch.clone());
            let _ = ch.recv();
            ProcOutcome::Value(0)
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while blocked.lock().unwrap().is_none() {
            assert!(Instant::now() < deadline, "proc never parked");
            std::thread::sleep(Duration::from_millis(2));
        }
        kill(w).unwrap();
        // The exit-reason determination is asynchronous to `kill`:
        // teardown reaches the blocked recv at its next poll (the 5ms
        // backstop), so wait for proc.exit before asserting — the
        // event's EXISTENCE is the contract, not its promptness.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if seen
                .lock()
                .unwrap()
                .contains(&SchedEvent::ProcExit { proc: w, kind: 2 })
            {
                break;
            }
            assert!(Instant::now() < deadline, "proc.exit never observed");
            std::thread::sleep(Duration::from_millis(2));
        }
        test_hook::set_test_hook(None);
        let seen = seen.lock().unwrap();
        assert!(seen.contains(&SchedEvent::ProcSpawn { proc: w }));
        assert!(seen.contains(&SchedEvent::ProcKill { proc: w }));
        assert!(seen.contains(&SchedEvent::ProcExit { proc: w, kind: 2 }));
    }

    /// The C symbol surface: spawn → self/monitor round trip; the
    /// entry's tag crosses as `error(tag)` per `[abi.err]`.
    #[test]
    fn c_surface_round_trip() {
        unsafe extern "C" fn ok_body(_env: *mut core::ffi::c_void) -> i64 {
            0
        }
        unsafe extern "C" fn err_body(_env: *mut core::ffi::c_void) -> i64 {
            13
        }
        // SAFETY: entry points used per their documented contracts.
        unsafe {
            let name = b"c-proc";
            let a = __wolf_rt_proc_spawn(ok_body, std::ptr::null_mut(), name.as_ptr(), 6);
            let ma = __wolf_rt_proc_monitor(a);
            let mut word = 0u64;
            assert_eq!(super::super::__wolf_rt_chan_recv(ma, &raw mut word), 0);
            assert_eq!(ProcExit::decode(word), ProcExit::Normal { value: 0 });
            super::super::__wolf_rt_chan_free(ma);

            let b = __wolf_rt_proc_spawn(err_body, std::ptr::null_mut(), name.as_ptr(), 6);
            let mb = __wolf_rt_proc_monitor(b);
            assert_eq!(super::super::__wolf_rt_chan_recv(mb, &raw mut word), 0);
            assert_eq!(ProcExit::decode(word), ProcExit::Error { tag: 13 });
            super::super::__wolf_rt_chan_free(mb);
        }
        // The root domain spells 0 outside any proc.
        assert_eq!(__wolf_rt_proc_self(), ROOT_DOMAIN);
    }

    /// `current_proc` attributes nested scopes to their proc, and the
    /// root domain to none.
    #[test]
    fn current_proc_attribution() {
        assert_eq!(current_proc(), None);
        let seen = Arc::new(Mutex::new((None, None)));
        let sink = seen.clone();
        let w = spawn_proc("attributed", move |_| {
            let outer = current_proc();
            let inner = Arc::new(Mutex::new(None));
            let i2 = inner.clone();
            let r = super::super::scope("nested", |s| {
                s.spawn("child", move |_| {
                    *i2.lock().unwrap() = current_proc();
                    ExitReason::Normal
                });
            });
            assert!(r.is_ok());
            *sink.lock().unwrap() = (outer, *inner.lock().unwrap());
            ProcOutcome::Value(0)
        });
        let m = monitor(w).unwrap();
        assert_eq!(recv_reason(&m), ProcExit::Normal { value: 0 });
        assert_eq!(*seen.lock().unwrap(), (Some(w), Some(w)));
    }
}
