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
