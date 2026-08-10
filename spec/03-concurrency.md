# Wolf Language Specification — 03: Concurrency

Status: normative, v0 (sprint s05). Anchors `[conc.*]` are stable. The
runtime (s32–s36) implements §2–§5; the interpreter (is06) executes them
and the explorer (is07) enumerates §5's events. Written in the Go memory
model document's shape — advice first, formalism beneath — because that
document's restraint is the correct register for memory models
(`.docs/refs/specs/go-memory-model.html`;
`.docs/refs/papers/boehm-threads-library.pdf` is why this is language
core and not a library README).

---

## §1 The memory model `[conc.mm]`

### The advice `[conc.mm.advice]`

Safe wolf code cannot data-race: sharing requires `move` (region
transfer), `freeze` (`imm`), or a `sync` wrapper, and everything else
does not typecheck (D14). If a program has no `unsafe` and no FFI, every
execution is sequentially consistent and this section's remainder is not
needed to reason about it. Stop reading here.

### The formal core `[conc.mm.hb]`

*Happens-before* is the least partial order consistent with:

- `[conc.mm.hb.po]` **Program order** within one task.
- `[conc.mm.hb.spawn]` `s.spawn(f)` happens-before the first action of
  the spawned task; every action of a child task happens-before the
  scope-exit join in the parent (`[conc.task.join]`).
- `[conc.mm.hb.chan]` The *k*-th send on a channel happens-before the
  *k*-th receive completes; for unbuffered channels the receive also
  happens-before the send *returns* (rendezvous).
- `[conc.mm.hb.mutex]` The *n*-th release of a `Mutex` (or `when` block
  exit) happens-before the *n+1*-th acquisition.
- `[conc.mm.hb.freeze]` `freeze r` happens-before every cross-task read
  of the resulting `imm` data.
- `[conc.mm.hb.move]` A region `move` sent through any channel publishes
  the entire transferred graph: every prior write into the region
  happens-before every access by the receiver.
- `[conc.mm.hb.proc]` A proc's exit (normal or otherwise) happens-before
  the delivery of its exit reason to any monitor.

`[conc.mm.drf]` **DRF-SC guarantee:** an execution containing no data
race (two conflicting accesses unordered by happens-before, at least one
a write) has sequentially-consistent semantics. Safe code satisfies the
premise by construction.

### Atomics `[conc.mm.atomic]`

- `[conc.mm.atomic.sc]` `std.sync` atomics are sequentially consistent
  by default.
- `[conc.mm.atomic.relaxed]` Relaxed and acquire/release orderings exist
  only in the unsafe tier (Boehm's measurement: the fully-fenced cost is
  small; the reasoning cost of weak orderings is not). Their semantics
  follow the happens-before edges they document; no out-of-thin-air
  values in any execution.

### Races in unsafe/FFI `[conc.mm.race]`

- `[conc.mm.race.1]` A data race on non-atomic memory (reachable only
  via Tier 3 or FFI) is UB — s04 §7 row **C1**, whose licensing this
  clause completes: sync-free stretches permit store motion, load/store
  combining, and register promotion of memory the model proves
  unshared.
- `[conc.mm.race.2]` Bounded outcome (Go's posture, adopted): a racy
  execution may not fabricate out-of-thin-air values, and the safe
  tier's memory safety is not voided wholesale — corruption is limited
  to memory reachable from the racing accesses' provenance.
- `[conc.mm.race.3]` Implementations may detect a race and halt with
  trap kind `race` (`--checked` builds; the schedule explorer).

## §2 Tasks `[conc.task]`

- `[conc.task.scope]` `scope name? { … }` opens a structured-concurrency
  scope. Scope handles are ordinary values (D16): a function that spawns
  into its caller's scope takes the handle as a parameter — lifetime
  extension is visible at every call site. No detached spawn exists in
  the language or standard library.
- `[conc.task.spawn]` `s.spawn(closure)` schedules a task. The closure's
  captures obey D14: `Copy` values copy; `imm` shares; everything else
  must `move` (a captured region transfers). Capturing a `mut` borrow of
  enclosing state is a compile error (E1101) unless the state is a
  `sync` type.
- `[conc.task.join]` Scope exit joins all children: the block does not
  complete until every spawned task has completed or been cancelled and
  finished its cancellation.
- `[conc.task.fail]` A child completing with an error value or fault
  cancels its siblings (`[conc.cancel]`) and re-raises at the scope
  exit; multiple failures surface the first in schedule order (a
  recorded decision, `[conc.det.events]`), the rest attach as context.
- `[conc.task.order]` Spawn confers no ordering beyond
  `[conc.mm.hb.spawn]`; tasks may run in any interleaving consistent
  with happens-before. Implementations schedule on OS threads; a blocked
  task holds its thread; pool compensation is unobservable.
- `[conc.task.name]` Tasks and procs carry names (spawn-site default,
  user-overridable) surfaced in the structured dump — the dump's
  *contents* are implementation-specified; its *existence* is contract.
- `[conc.task.par]` `xs.par(f)` and the parallel iterator family are
  defined by desugaring to `scope { … spawn … }` — they add no
  semantics, only shape.
- `[conc.task.root]` The process runs under a root supervisor scope of
  process lifetime; `spawn proc` targets it (or a nested supervisor).
  Daemon-shaped work is therefore named, supervised, and enumerable —
  never detached.
- `[conc.task.fail.owner]` `[conc.task.fail]`'s cancellation reaches
  the scope **owner** too: when a child fails, an owner blocked at a
  `[conc.cancel.points]` blocking point it entered inside the scope's
  extent is cancelled exactly like a sibling — the scope is the
  cancellation unit, owner included (the Trio posture, adopted
  2026-08-10 after is06's machine deadlocked `procs.lu`'s second
  scope without it — finding S-4). The failure still re-raises at the
  scope exit, after the join (`[conc.task.join]`).

## §3 Procs, channels, select, cancellation `[conc.proc]`

### Procs `[conc.proc.model]`

- `[conc.proc.1]` A proc is a failure domain owning its regions. At v1
  procs are in-process (03 Q1); every clause here is worded so an
  OS-process backend also satisfies it — nothing may assume shared
  address-space visibility into a proc beyond its channels.
- `[conc.proc.2]` `w.link()` couples fates symmetrically: either side's
  abnormal exit kills the other. `w.monitor()` delivers the exit reason
  asynchronously to the monitor's channel.
- `[conc.proc.exit]` Exit reasons, closed set: `normal(value)`,
  `error(value)` (an error value crossed the proc boundary),
  `killed` (supervisor/link), `cancelled` (structured cancellation
  reached the proc). Reasons are values (D30) — never unwinding.
- `[conc.proc.kill]` **Killed-proc sequence, in order** (the decided
  rule): (1) the proc's task tree is cancelled *without running any
  further user code* — pending `defer`/`errdefer` in the killed proc
  **do not run**; (2) external frames are waited out or fenced (§4);
  (3) the proc's regions bulk-free; (4) exit reasons deliver.
  Consequence, stated normatively: resources shared *across* proc
  boundaries must be owned by channels/supervisors (release-on-exit
  messages), never by defers inside the proc. Contrast
  `[conc.cancel.defer]`.
- `[conc.proc.cancel]` `w.cancel()` delivers **structured
  cancellation** to a proc: its task tree is cancelled cooperatively
  at `[conc.cancel.points]` blocking points, and its defers run
  (`[conc.cancel.defer]` — contrast `[conc.proc.kill]`, which skips
  them). A proc that exits because this cancellation reached it exits
  with reason `cancelled`; one that completes its value despite it
  keeps `normal(value)`. (Appended 2026-08-10: is06 found
  `[conc.proc.exit]`'s `cancelled` label unreachable from the pinned
  surface — finding S-6; this clause is the delivery mechanism.)
- `[conc.proc.link.pair]` `a.link(b)` couples two procs symmetrically
  — the two-proc spelling is06 found missing (finding S-7);
  `w.link()` is `w.link(<the calling task's proc>)`. Linking is
  idempotent per pair; delivery is `[conc.proc.2]`'s.
- `[conc.proc.root]` The **root supervisor's domain is the process**:
  `main`'s task tree runs in it, and `w.link()` called from `main`
  couples `w` to it. The root domain's abnormal death — a linked
  partner's abnormal exit, or a fault escaping `main` — runs the
  killed-proc sequence (`[conc.proc.kill]`) for every live proc and
  terminates the process with a nonzero, implementation-specified
  status (`[conf.trap.exit]` discipline: conforming tools compare the
  outcome class, never the number). (Appended 2026-08-10, finding
  S-7's second half: the machine reported the root kill
  `unsupported`; it is now specified.)

### Channels `[conc.chan]`

- `[conc.chan.type]` `channel[T](n)` requires T sendable: `Copy`, `imm`,
  a region value (moved on send), or a `sync` type. Anything else is a
  compile error (E1102) pointing at D14's three verbs.
- `[conc.chan.buf]` Capacity `n ≥ 1` buffers; `n = 0` is rendezvous.
  Sends on a full channel and receives on an empty one block (they are
  cancellation points and recorded events).
- `[conc.chan.close]` `close` makes further sends return an error value
  (never UB, never a fault); buffered items drain; receives on a
  drained-closed channel return the closed error. Iterating a channel
  (`for v in ch`) ends at drained-close.
- `[conc.chan.mailbox]` Procs communicate exclusively via typed channels
  + `select` (03 Q3): there is no selective receive; a proc's message
  handlers are atomic and non-blocking (a handler that must block
  spawns/awaits inside its own task tree instead).
- `[conc.chan.move]` Sending a region value **is its affine move**
  (`[mem.region.freeze.2]`): the send transfers the whole owned
  subtree to the receiver and publishes every prior write into it
  (`[conc.mm.hb.move]`). The transferred subtree must be **closed**
  (`[mem.region.freeze.3]`: sending an open region is a compile
  error, E1005; dynamically `region-fault`) and **disconnected** — at
  the send, no path from the sender's still-reachable graph leads
  into the transferred region other than the moved value itself.
  Static checkers get this from the forest invariant plus affine
  moves; dynamic machines re-check it at the send. (Appended
  2026-08-10, finding S-2 — the clause the sprint contract named.)
- `[conc.chan.staleuse]` After a moving send, the donor's binding is
  moved-from: any later use through it is `[mem.tier0.move.2]` —
  compile error E1001, dynamically `trap(use-after-move)`. The
  staleness is the *sender's* fault, reported at the use site, never
  the receiver's. (Appended 2026-08-10, finding S-2: the
  sender-stale-use fault the machine implements now has its clause.)
- `[conc.chan.imm]` `imm` data sends **by reference**: no move, no
  copy, and the sender's access survives the send
  (`[mem.region.edge.imm]`, `[conc.mm.hb.freeze]`). `Copy` values
  copy at the send. Only region values change hands on a send.

### Select `[conc.select]`

- `[conc.select.ready]` `select` evaluates readiness of its arms;
  exactly one ready arm's body runs. With no ready arm, `select` blocks
  (cancellation point).
- `[conc.select.fair]` Among simultaneously-ready arms the choice is
  **pseudo-random, seeded by the scheduler** — a recorded decision
  (`[conc.det.events]`), never wall-clock incidental. This wording is
  load-bearing: replay reproduces the same choices from the seed.
- `[conc.select.timeout]` `timeout(d)` arms become ready when the
  scheduler's clock (virtual under test, monotonic in production)
  reaches the deadline — timer fire is a recorded event.
- `[conc.select.io]` Completion-based I/O (X6) surfaces exclusively as
  select-able completion operations; no clause anywhere may assume
  readiness polling. An I/O completion's delivery is a recorded event.
- `[conc.select.closed]` A **drained-closed** channel makes its
  receive arm **ready** (`[conc.select.ready]`): the arm runs and
  receives the closed error value (`[conc.chan.close]`'s error, an
  ordinary error value). Go's posture, adopted 2026-08-10 from is06's
  machine (finding S-8): a `select` that blocked forever on a channel
  that can never deliver would contradict `[conc.chan.close]`'s
  never-a-fault discipline.

### `when` — whole-set acquisition `[conc.when]`

(Appended 2026-08-10. 03 Q6 decided `when` is a language construct;
the corpus exercised it with no clauses behind it — finding S-1.
is06's machine was the first executable evidence; these clauses adopt
its sound choices, per the approximation contract §10.5, and deviate
only where noted.)

- `[conc.when.order]` `when (a, b, …) { … }` acquires the **entire
  operand set** before the body runs, one object at a time in the
  **canonical order** — a single total order over all sync objects in
  the process (creation order; a recorded decision) — regardless of
  the order written at the site. `when (a, b)` and `when (b, a)`
  perform identical acquisitions. Each acquisition is an `acquire`
  event (`[conc.det.events]`), and `[conc.mm.hb.mutex]`'s edge counts
  per object.
- `[conc.when.nodeadlock]` **No lock-order deadlock, by
  construction:** every `when` acquires its whole set in the one
  canonical order, so a task blocked mid-set holds only objects
  earlier in that order than the one it awaits — no cycle of `when`
  acquisitions can form. This argument is exactly why
  `[conc.when.nonest]` forbids nesting: a nested `when` is
  incremental acquisition by another spelling.
- `[conc.when.body]` The body runs with **exclusive access** to every
  operand's payload. Operands named by simple paths rebind to their
  payloads inside the body (reads and writes go to the payload) and
  write back at release, in reverse canonical order; block exit is
  the release. Capturing or storing a payload path past the block is
  the same error surface as `[conc.task.spawn]`'s capture rule.
- `[conc.when.nonest]` A `when` lexically inside another `when` body
  is a **compile error (E1103)**. Reaching an acquisition of a sync
  object the task already holds *dynamically* (through a call) can
  never complete and is `trap(deadlock)` — `[conc.deadlock.self]`.
  (Deviation from the machine, with rationale: is06 trapped `assert`
  here only because no fitting kind existed; this amendment adds one,
  and `assert` was recorded as a stopgap, not a choice.)

### Deadlock `[conc.deadlock]`

(Appended 2026-08-10, findings S-3/S-4: a language whose concurrency
is schedulable and explorable needs a stable spelling for "this
schedule deadlocks"; `unsupported` was honest and insufficient.)

- `[conc.deadlock.def]` An execution state in which **every live task
  is blocked** at a `[conc.cancel.points]` blocking point, with no
  pending timer and no in-flight I/O completion, is a **deadlock** —
  a *defined outcome*, not UB and not silent nontermination: a
  deterministic scheduler can detect it exactly.
- `[conc.deadlock.trap]` A detected deadlock terminates the process
  with trap kind `deadlock` (added to `[conf.trap.set]` by this
  amendment — the deliberate spec/05 revision that clause's closure
  demands), reporting the blocked-task roster (names and blocking
  points; contents implementation-specified, existence contract,
  `[conc.task.name]`'s discipline). Detection is **required** in
  deterministic test modes (record/replay, the is07 explorer) and
  permitted elsewhere. `trap(deadlock)` is the verdict spelling
  is07 reports per schedule.
- `[conc.deadlock.self]` Acquiring a sync object the acquiring task
  already holds can never complete: the same defined outcome,
  detected immediately — `trap(deadlock)`. (The lexical case is
  E1103, `[conc.when.nonest]`; this clause covers the through-a-call
  case no local check can see.)

### Cancellation `[conc.cancel]`

- `[conc.cancel.points]` Cancellation is cooperative, delivered at
  **runtime-owned blocking points**, closed set: channel send/receive,
  `select`, `Mutex`/`when` acquisition, I/O completion waits, timer
  waits, and explicit `checkpoint()`. `--checked` builds additionally
  poll at function entry and loop back-edges; release builds do not
  (the kernel preempts for scheduling, not cancellation).
- `[conc.cancel.defer]` A **cancelled task** runs its own frames'
  `defer`/`errdefer` as its frames unwind-by-return (no unwinding
  mechanism — cancellation surfaces as an error value at blocking
  points, and ordinary returns do the rest). Side-by-side with
  `[conc.proc.kill]`: cancellation is polite (defers run); kill is
  structural (regions free, defers don't).
- `[conc.cancel.c]` C frames are never unwound and never interrupted: a
  task blocked in an FFI call is cancelled at its next safe point after
  return (§4).

## §4 FFI safe points `[conc.ffi]`

- `[conc.ffi.points]` A safe point is any `[conc.cancel.points]`
  blocking point, plus (in `--checked` builds only) function entries and
  loop back-edges.
- `[conc.ffi.external]` Entering an `extern` C call moves the task to
  state *running-external*. The runtime may do nothing to a
  running-external task — no cancellation, no stack inspection, no
  migration — until the call returns or reaches a wolf-provided callback
  (which is a safe-point domain re-entry).
- `[conc.ffi.kill]` Proc kill with running-external members: region
  bulk-free (step 3 of `[conc.proc.kill]`) **waits out or fences**
  external frames — memory reachable by C per s04
  `[mem.boundary.ffi]` (handles / pinned `#[trusted]` regions) is not
  reclaimed while any external frame may hold it.

## §5 Determinism events `[conc.det]` (X12 — the unretrofittable contract)

Every scheduling-observable decision is a **recordable event** from a
closed taxonomy. This is semantics: the runtime's primitives are defined
as the things that emit these events.

- `[conc.det.events]` Event kinds, closed set (versioned as `sched-ev/0`):
  1. `spawn` (task created, parent scope, name)
  2. `steal` / `park` / `unpark` (scheduler placement)
  3. `chan` (send↔receive pairing, per channel, per k)
  4. `select` (chosen arm among the ready set — set recorded too)
  5. `acquire` (Mutex/`when` acquisition order per sync object)
  6. `timer` (fire order and virtual timestamps)
  7. `io` (completion delivery order)
  8. `procexit` (reason delivery order)
  9. `seed` (derivation for user-visible randomness — `std.random`
     draws from the schedule seed under test)
- `[conc.det.modes]` Conforming runtimes implement three modes:
  **record** (append events), **replay** (consume; any divergence is a
  hard fault naming the position and expected/actual event), **free**
  (neither). Test builds compile the hooks in; release builds may
  compile record/replay out but must keep primitive boundaries where the
  events *would* be (no fused fast paths that skip an event point).
- `[conc.det.flow]` All nondeterminism flows through runtime-owned
  primitives: stdlib concurrency paths may not call clocks, futexes, or
  OS randomness directly — s32–s36's enforcement hook, checked by
  review + the is07 explorer (an event the explorer cannot enumerate is
  a spec violation).
- `[conc.det.seed]` `--schedules=N` explores N seeds; `--replay=SEED`
  regenerates the identical event stream deterministically (the stream
  itself ships only in the v2 flight recorder; the taxonomy and ordering
  guarantees are fixed now).
- `[conc.det.chaos]` `--chaos` is defined as an event-stream rewrite
  (delaying `unpark`s, reordering `io` deliveries, injecting fault
  outcomes at blocking points) — chaos runs are therefore replayable by
  construction.
- `[conc.det.dpor]` Explorer obligation (is07): reductions (DPOR) must
  preserve recorded-event semantics — two executions identified by the
  reduction must produce identical event streams up to the reduction's
  proven-commutative reorderings.

---

Cross-references: s04 §7 C1 licensing lives in `[conc.mm.race.1]`.
Error-value mechanics at boundaries: spec 04 §4. Trap kind `race`:
`[conc.mm.race.3]`.
