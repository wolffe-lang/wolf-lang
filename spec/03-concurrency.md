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
- `[conc.proc.handle]` The two handles have **type names**, in the
  prelude: a scope handle is `Scope`, and a proc handle is `Proc[T]`
  where `T` is the completion value its join collects
  (`[conc.proc.join]`). `fn fan_out(s: Scope)` is how
  `[conc.task.scope]`'s "takes the handle as a parameter" is written,
  and `fn watch(p: Proc[int])` its proc twin. The lowercase `scope`
  and `proc` are the block and spawn KEYWORDS and are not admitted in
  type position; a program that writes one there is E0206, whose help
  names the capitalised spelling. (Added 2026-09-18, s170 — BACKLOG
  B21's ruling on wolf-lang#316: prelude names with a signature
  elaboration, not keywords in type position the way `region` is.)
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
  task holds its thread; pool compensation is unobservable **to the
  program** — no value, ordering or exit depends on it — **and
  observable to the host.** **The cost, stated** (measured, not
  derived): on wolf's native tier on linux, compensation is a timed
  wait that re-arms, armed once per parked blocking wait, so each task
  parked in a kernel wait with no deadline — `os_signal_wait`, an
  unbudgeted `net_accept` — costs **~196 `futex` calls a second, every
  one an error return**, for as long as it stays parked, and a process
  with two such waits pays two clocks. Four measurements by lobo on
  ubuntu-latest (4 vcpus) under `strace -c -f`, 8 s idle windows:
  ws30 (wolf 0.2.9 and `fc07cc5`, runs 34553773533, 34554207566),
  ws31 (0.2.11, run 34598620069), ws32 (0.2.11, run 34603180650,
  6,273 calls over four hands, and run 34603229602 for the per-wait
  increment). It is under 0.1% of a serving hand's CPU, and it is a
  count a server choosing between a poll and a park is owed. (The
  number written 2026-09-15 by s163 — wolf-lang#302 item 2; whether a
  deadline-less park can cost nothing is item 1, still open.)
- `[conc.task.name]` Tasks and procs carry names (spawn-site default,
  user-overridable) surfaced in the structured dump — the dump's
  *contents* are implementation-specified; its *existence* is contract.
- `[conc.task.par]` `xs.par(f)` is defined by desugaring to `scope { …
  spawn … }` — it adds no semantics, only shape — and it is the whole
  parallel iterator family this edition (a parallel `filter` or `fold`
  is not ruled). `xs` is a `List[T]` read, never moved; `f` is a fn
  value of type `fn(T) -> U` or `fn(T) -> U ! E`; the value is
  `List[U]`, or `List[U] ! E` when `f` carries a row. It is a builtin
  method (`[type.comb.builtin]`). The desugar, with `n = xs.len`, `k`
  the chunk count of `[conc.task.par.chunk]`, and chunk `c` the index
  range `lo(c)..hi(c)`:

  ```text
  var out = <n slots of U>
  scope par {
      for c in 0..k {
          par.spawn(fn() {
              for i in lo(c)..hi(c) { out[i] = f(xs[i])? }
          })
      }
  }
  out
  ```

  The two things this text does that a program cannot spell — every
  task reads the one `xs`, and every task writes `out` — are what the
  desugar is licensed to do: the reads share a read claim held for the
  whole expression (`[mem.iter.excl]`), the writes go to pairwise
  disjoint slots, and no slot is read before the scope joins, so no
  execution of the desugar has a data race (`[conc.mm.drf]`).
  (Completed 2026-09-15 by s166, wolf-lang#390 option 1. The clause was
  one sentence from s05 to this date; neither machine implemented it,
  and E1101's note stopped recommending it at 0.2.13 because nothing
  did.)
- `[conc.task.par.order]` **Results are in input order, whatever the
  schedule.** `out[i]` is `f(xs[i])` for every `i`, so `xs.par(f)`
  and `xs.map(f)` are the same `List` for every total `f` that
  performs no recorded event (`[conc.task.par.det]`) — the schedule decides when
  each slot is filled and never which. Cost: none beyond the desugar;
  order is a property of the slots, not a sort.
- `[conc.task.par.fail]` **Failure is `[conc.task.fail]`'s.** A chunk
  stops at its own first failure — an error value `f` raises, or a
  fault — and that failure cancels the sibling chunks
  (`[conc.cancel]`: each observes it at its next blocking point, or
  runs to its end) and re-raises at the `par`, after the join. Of
  several failures the first in schedule order surfaces and the rest
  attach as context. The partially filled `out` is never observable: a
  failed `par` has no value. So which later elements `f` ran on before
  the cancellation reached them is schedule-dependent, and is
  observable only through what `f` did besides return — a send, a
  `sync` write, output — each of which is a recorded event already. A
  fault in a root-domain `par` is process death, as the same fault in a
  serial loop is (`[conc.proc.root]`). Cost: nothing on the success
  path; a failure costs the cancellation `[conc.cancel]` already
  prices.
- `[conc.task.par.capture]` **`f` is checked as a spawned closure's
  body** (`[conc.task.spawn]`, D14): `Copy` captures copy, `imm` and
  `sync` captures share, a captured `List` or other value is read
  through the shared loan every spawned closure's captures take, and a
  write to captured enclosing state is **E1101**, reported at the
  write with the `par` as the spawn site. A named function or a
  capture-free closure captures nothing. A region value captured by
  `f` is refused, because one value cannot move into `k` tasks. The
  check is spawn's, applied once to `f` and valid for every chunk,
  because nothing spawn accepts for one task depends on how many tasks
  run the same body. Cost: none at run time — it is a refusal.
- `[conc.task.par.chunk]` **Chunking is contiguous, bounded by the
  workers, and unobservable.** The runtime splits `0..n` into `k =
  min(n, W)` contiguous chunks whose lengths differ by at most one,
  in index order, where `W ≥ 1` is the number of workers the runtime
  schedules tasks onto (implementation-specified; the native tier's
  pool starts one per logical core). A task per element is not a
  conforming desugar: a spawn costs a record and a queue operation,
  most `f` cost less, and `k ≤ W` bounds the overhead by the machine
  rather than by the list. `n == 0` spawns nothing and answers `[]`;
  an implementation may run a single chunk (`k == 1`) on the calling
  task with no spawn at all. `k` is **unobservable except through
  `[conc.det]`'s recorded events**: results are in input order
  (`[conc.task.par.order]`), `f`'s captures cannot carry a write
  between chunks (`[conc.task.par.capture]`), and the only trace `k`
  leaves is the number of `spawn` events under the `par`'s scope — and
  the chunks' own events, if `f` performs any. No clause lets a
  program ask for `k`, and a program whose printed bytes change with
  `W` observes it through a `sync` object or a channel, each of which
  records.
- `[conc.task.par.det]` **Determinism.** Under record/replay
  (`[conc.det.modes]`) `k` is part of the stream — its `spawn` events —
  so a replay reproduces the same chunks; the schedule explorer
  (`--schedules=N`) runs with a worker count fixed by its configuration
  and recorded with the seed, so one seed is one `k`, and enumerates the chunk tasks' interleavings as
  it enumerates any spawned task's. When `f` performs no recorded
  event (its captures are `Copy`, `imm` or fn values and it neither
  sends, acquires nor does I/O), every schedule and every `k` produce
  the identical `List` — the chunks' actions touch disjoint slots and
  so commute — and a DPOR reduction may identify them all
  (`[conc.det.dpor]`). The **checked** execution (`wolf conform-run
  --checked`) refuses `scope` and closures today, and so refuses `par`
  by the same name, unsupported rather than wrong; the native tiers
  run it. Cost: the recording costs the `spawn` events and nothing
  per element.
- `[conc.task.par.cost]` **The cost, stated.** One allocation of `n`
  slots in the ambient region of the `par` expression — exactly `map`'s
  (`[type.comb.set]`) — and no copy of `xs`. `k` task spawns, each
  `[conc.task.spawn]`'s price (a capture record in the scope's region
  and a pool enqueue), and one join. Per element, nothing beyond the
  call of `f`. A `copy` inside `f` is deep (`[mem.tier0.move.3]`): one
  allocation and a buffer copy for every `List` or `Map` it reaches,
  charged where the task allocates, so a `par` whose `f` copies a
  container pays that per element and not per list. What `f` allocates lands where a task's allocations land:
  tasks run with the process root as their ambient region, and the
  native root arena serializes allocation behind one lock, so an `f`
  that allocates on every call contends there and scales worse than
  one that computes. Speedup is not promised: a list shorter than `W`,
  or an `f` cheaper than a spawn, can make `xs.par(f)` slower than
  `xs.map(f)`, and both are conforming.
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
  address-space visibility into a proc beyond its channels. "Owning
  its regions" includes the **ambient** one the proc's body allocates
  into: `[mem.region.proc]` (s160, wolf-lang#355) reads that here, so
  a value built inside a proc and sent out of it is E1010, not a
  payload — the shape below rules the send, and that clause rules
  where the bytes live.
- `[conc.proc.2]` `w.link()` couples fates symmetrically: either side's
  abnormal exit kills the other. `w.monitor()` delivers the exit reason
  asynchronously to the monitor's channel.
- `[conc.proc.exit]` Exit reasons, closed set: `normal(value)`,
  `error(value)` (an error value crossed the proc boundary),
  `killed` (supervisor/link), `cancelled` (structured cancellation
  reached the proc), `fault(kind)` (a trap fired on a task inside the
  proc and was contained at the boundary — `kind` is one name from
  `[conf.trap.set]`'s closed vocabulary, so containment adds **no**
  new trap kind). Reasons are values (D30) — never unwinding.
  (Amended 2026-09-01, s132 — D68, the ruled fourth shape: the trap
  fires at its site, the proc is the failure domain that absorbs it.)
  The fault mapping: a contained trap runs the killed-proc sequence
  (`[conc.proc.kill]` — no further user code, regions bulk-free
  BEFORE delivery), `fault` is abnormal for links (`[conc.proc.2]`:
  a faulted partner is a dead partner), and the join observes the
  class as `is_fault()` — with `is_alloc_contract()` naming the one
  kind the region-cap contract pins (`[mem.region.cap.3]`,
  wolf-lang#187: the budget breach lobo's per-request 503 matches
  on). In the root domain a trap remains process death
  (`[conc.proc.root]`, `[conf.trap.exit]`).
- `[conc.proc.join]` `p.join()` blocks the calling task until `p`
  exits and yields its result: `T ! {error, killed, cancelled, fault}`,
  where the ok half is `[conc.proc.exit]`'s `normal(value)` read as a
  VALUE and each row tag is one abnormal class read as a tag. This is
  the proc's talk-back channel: before it, a proc could report only
  its exit class and its stdout. The join is a blocking point, so
  cancellation of the JOINING task surfaces at it
  (`[conc.cancel.points]`); it is not a second delivery of the exit
  reason, but the same one `w.monitor()` carries, read
  synchronously — a join after the proc has already exited answers
  immediately and never blocks. The row is payload-free at v1: an
  `error(tag)`'s tag rides the monitor channel, and a caller that
  needs it reads the reason there. (Added 2026-09-18, s170 — BACKLOG
  B21's ruling on wolf-lang#110: a typed result the supervisor
  collects at the join, NOT a channel handle as a proc parameter.)
- `[conc.proc.arg]` A `spawn proc` argument crosses into a domain
  that outlives the spawner, so it is **moved** — with one exception:
  FROZEN data is **shared, not moved**. `[mem.region.freeze.1]` makes
  frozen data immutable forever and shareable across threads and
  `[conc.chan.imm]` crosses `imm` by reference with no move, so
  neither reason to move an argument — a later write by the spawner,
  a free before the proc reads — can arise. One frozen snapshot may
  therefore be handed to any number of procs. (Added 2026-09-18,
  s170, wolf-lang#312: the mem tier moved it, so the same two calls
  compiled as plain calls and refused as spawns.)
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
  outcome class, never the number). A corpus witness spells that
  class `run(exit=nonzero)` (`[conf.directive.check]`; witness
  `conc/proc_link_root_death.lu`, where wolf's native tier exits 121
  and lupin 0.1.36 exits 1). (Appended 2026-08-10, finding S-7's
  second half: the machine reported the root kill `unsupported`; it is
  now specified. The spelling sentence, 2026-09-15, s163 —
  wolf-lang#371.)

### Channels `[conc.chan]`

- `[conc.chan.type]` `channel[T](n)` requires T sendable: `Copy`, `imm`,
  a region value (moved on send), or a `sync` type. Anything else is a
  compile error (E1102) pointing at D14's three verbs.
- `[conc.chan.payload]` **A payload is any value the type system can
  move, and the channel owns it in flight.** `[conc.chan.type]`'s four
  kinds are payloads; so is a struct, an enum or a tuple whose every
  field is a payload other than a region value — a region crosses on
  its own (`[conc.chan.move]`), never inside another value, because a
  copied handle would be two owners. A `send` copies the payload into
  the channel (the `[conc.chan.imm]` reading: `Copy` and by-value data
  copy at the send, and the sender's binding is untouched); the
  channel owns that copy while it is buffered or parked with a blocked
  sender; `recv` — a `for` over the channel, a `select` arm — hands it
  to the receiver, who owns it from then on. A payload leaves the
  sending frame, so it must outlive it exactly as a returned value
  must: a payload built in a frame-local region is E1010 at the send
  (witness `corpus/conc/chan_payload_escape.lu`) — and a proc's own
  ambient region is frame-local to the proc (`[mem.region.proc]`,
  witness `corpus/conc/chan_payload_escape_proc.lu`). A `str` payload is
  its view: the bytes stay where they were built and cross by
  reference (`[conc.chan.imm]`). The cost, stated: a payload of
  one machine word or less (an `int`, a `bool`, a `char`, any integer
  width) crosses in the word; anything wider — a `str`, a struct, a
  tuple, a float, a handle — crosses in a heap box the sender fills
  and the receiver empties, one allocation and two copies per message.
  A send that fails (`closed`, `cancelled`) frees its box before it
  answers, and a channel freed while boxes are still in it frees them
  with itself. (Ruled 2026-09-10, s150 — wolf-lang#268's largest
  family: the native tier carried one word since s39 and E1102 refused
  every aggregate, which held chapter 12 §12.1's `channel[Doc]` on one
  machine. Witness `corpus/conc/chan_struct_payload.lu`.)
- `[conc.chan.buf]` Capacity `n ≥ 1` buffers; `n = 0` is rendezvous.
  Sends on a full channel and receives on an empty one block (they are
  cancellation points and recorded events).
- `[conc.chan.default]` `channel[T]()` — no capacity argument — is the
  rendezvous channel: the default is `n = 0` (`[conc.chan.buf]`), by
  specification and not by omission. `channel[T]()` and
  `channel[T](0)` are the same channel: a send blocks until a receiver
  meets it. **The cost, stated:** a rendezvous channel reserves no
  buffer — its record is the channel's lock and its two waiter queues —
  and every send is one handoff through that lock, parking the sender
  until a receiver arrives; a capacity is spelled when a program wants
  the other trade. Witness: `conc/chan_default_rendezvous.lu`, whose
  receiver announces itself before it receives, so its line precedes
  the sender's on both machines, and `channel[int](1)` reverses the
  two. (Ruled 2026-09-11, BACKLOG B24 — wolf-lang#155's ch12 row: the
  default is specified as rendezvous; written by s163. Appended
  2026-08-11 as a draft from the bs06 ledger's spec-gap row: the
  reference machine defaulted to rendezvous with no clause behind it;
  this clause adopts that behavior as normative rather than repairing
  it. Rationale: rendezvous is the synchronization-first default —
  every send meets its receive, so the `[conc.mm.hb.chan]` edge pairs
  the parties directly and no message waits in a buffer nobody chose;
  a capacity is a throughput decision, and decisions are spelled.
  Go's precedent, deliberately shared: `make(chan T)` is unbuffered.)
- `[conc.chan.close]` `close` makes further sends return an error value
  (never UB, never a fault); buffered items drain; receives on a
  drained-closed channel return the closed error. Iterating a channel
  (`for v in ch`) ends at drained-close. The closed error is the
  payload-free row tag **`closed`**, and the cancellation that surfaces
  as an error value at a channel's blocking points
  (`[conc.cancel.points]`) is **`cancelled`**: lowercase, the house
  pact `[mem.str.parse]` restated — a payload-free mark is a lowercase
  word, CapCase names a payload's type (W0603). `recv`'s row is
  `T ! {closed, cancelled}`, and `send`'s is `() ! {closed, cancelled}`
  — the same two tags, since a send blocked on a full buffer is a
  blocking point too (`[conc.chan.buf]`, `[conc.cancel.points]`). A
  send whose value nobody reads — a loop body, an else-less `if` — is
  `[type.unit.discard]`'s warned discard, never a mismatch; `?` hands
  the failure to the enclosing row, a task's to its scope
  (`[type.unit.consume]`). (Typed 2026-09-10, wolf-lang#275, s146: the
  compiler had `send` as `()` for two releases after this clause said
  otherwise, and the fix waited on `[type.unit]` being written;
  `corpus/conc/chan_send_closed_row.lu` witnesses the send side, the
  file below the receive side.) (Spelled 2026-09-09, wolf-lang#273: the
  clause named the value and not its tag, the compiler's row said
  `closed` and the interpreter minted `Closed`; since `{err}` renders a
  caught row by its tag's name (`[type.interp.row]`) the spelling is
  observable, so it is one. The interpreter and the book's chapter 12
  follow in the same wave; `corpus/conc/chan_closed_row.lu` witnesses
  the receive side.)
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
  is07 reports per schedule. The checked tier (`[exec.checked]`) is
  one of the deterministic modes, and it runs exactly one task: it
  refuses `spawn`, `scope`, `select` and `when` by name before they
  run. So a channel operation that would block there — a send on a
  full or rendezvous channel, a receive or a `for` on an empty open
  one — blocks every live task, and the tier answers `trap(deadlock)`
  at that operation. It costs nothing to detect: with one task the
  check is the block itself. The native tier is permitted not to
  detect it and, at 0.2.14, waits. (s161, wolf-lang#342: the checked
  tier served no channel method at all before; lupin 0.1.36 answers
  the same `trap(deadlock)` on the same four shapes. Witnesses:
  `corpus/conc/chan_root_task.lu` for the operations that complete,
  and `ubcheck`'s
  `a_blocking_channel_operation_on_the_only_task_is_deadlock` for the
  four that block, which the corpus cannot carry because the native
  tier would wait on them.)
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
