# Wolf Spec — 07: Schedule Points (s36 Phase A + Phase B)

Status: normative for hook SHAPE and vocabulary; `sched-ev/1` numbering
is ASSIGNED (s36 Phase B, §1.1) and append-only from here per
`[sched.stable]`. `sched-ev/0` (the reference interpreter's stream) is
stable and unchanged. This document gates c07 implementation merges
(s36's Phase A contract): every runtime seam matches this shape or does
not merge.

## 1. The taxonomy `[sched.point.set]`

A schedule point is a runtime-owned decision that can alter observable
interleaving. The closed v1 set:

- `spawn` — a task becomes runnable (parent continues vs child runs is
  a decision only under the test scheduler).
- `join` — a scope waits; which pending task runs next is a decision.
- `park` / `unpark` — a worker blocks or wakes (production-only detail;
  under the test scheduler these collapse into `pick`).
- `chan.send` / `chan.recv` — blocking channel edges (s33's
  implemented inventory: each edge carries a block/commit phase as
  subject detail — the kind vocabulary is exactly this list).
- `select.arm` — the committed arm among ready arms (the ready set
  rides as subject detail, per `[conc.det.events]` kind 4).
- `pick` — the generic "which runnable task next" decision; every other
  point reduces to at most one `pick` plus its own identity.
- `timer.fire` — activated by s33's timeout arms per `[sched.stable]`
  (reserved-kind activation); s35's timer wheel inherits the name.
- `chan.close` — `close` wakes every blocked waiter, an
  interleaving-visible decision. (Appended 2026-08-11 by s33 per
  `[sched.stable]`'s append rule — the sprint inventory's `close`
  event; sched-ev/0 has no counterpart, and cross-version comparison
  is by verdict, so the append is safe.)
- `acquire` — one sync-object acquisition in `when`'s canonical
  order; a whole-set acquisition's steps carry (index, set-size)
  subject detail so the recorder folds them into one event with the
  set's ids. (Appended 2026-08-11 by s33: `[conc.when.order]` and
  sched-ev/0 kind 5 already name `acquire`; this list omitted it only
  because s32 had no sync objects.)
- `proc.spawn` — a proc comes up under the root supervisor
  (`[conc.task.root]`; subject: the packed generational proc id).
- `proc.kill` — kill teardown is requested for a proc
  (`[conc.proc.kill]` step 1 begins; this is also `--chaos`'s s36
  kill-injection point).
- `proc.exit` — a proc's exit reason is determined (sched-ev/0 kind
  8's native twin; subject carries the proc id and the reason class
  per `[conc.proc.exit]`). Monitor and link DELIVERIES ride ordinary
  `chan.send` edges, so delivery order is already recorded without a
  separate kind. (All three appended 2026-08-11 by s34 per
  `[sched.stable]`'s append rule — the module's "proc events join in
  s34" reservation, activated; cross-version comparison stays by
  verdict.)
- `io.arrive` — a pending io completion is delivered to its parked
  waiter (s35's reactor; subject: the submission token). WHICH
  pending completion is delivered next, and when, is the
  interleaving decision; s36's `--chaos` delay/reorder injection
  lands on this seam, and the simulated reactor implements it.
  `timer.fire` gains the reactor's timer wheel as a second producer
  — same activated kind, inherited per this section's rule; a fired
  io deadline is a `timer.fire` event. (Appended 2026-08-11 by s35
  per `[sched.stable]`'s append rule — the net module's reserved
  "completion-arrival appends its own kind" note, activated;
  sched-ev/0 has no counterpart, and cross-version comparison is by
  verdict, so the append is safe.)

### 1.1 `sched-ev/1` numbering `[sched.ev1]` (Phase B, assigned 2026-08-11)

Append-only; new kinds take the next number, nothing ever renumbers.
Kind 0 is `pick` — the grant decision itself, emitted by the test
scheduler, never by a seam site.

| # | kind | subject (ids normalized to first-seen order per space) | choice |
|---|------|--------------------------------------------------------|--------|
| 0 | `pick` | granted task id | index into the sorted ready set |
| 1 | `spawn` | task id | 0 |
| 2 | `join` | scope id | 0 |
| 3 | `park` | — (never recorded: collapses into `pick`) | — |
| 4 | `unpark` | — (collapses into `pick`) | — |
| 5 | `steal` | — (collapses into `pick`) | — |
| 6 | `cancel.check` | `scope id << 1 \| cancelled` | 0 |
| 7 | `region.transfer` | 0 | 0 |
| 8 | `chan.send` | `chan id << 1 \| commit` (0 = block phase) | 0 |
| 9 | `chan.recv` | `chan id << 1 \| commit` | 0 |
| 10 | `chan.close` | `chan id << 1` | 0 |
| 11 | `select.arm` | ready-arm bitmask | index into the ready arms |
| 12 | `acquire` | sync id | `idx << 16 \| set_len` (detail, not a decision) |
| 13 | `timer.fire` | 0 | 0 |
| 14 | `io.arrive` | submission token | 0 |
| 15 | `proc.spawn` | proc id | 0 |
| 16 | `proc.kill` | proc id | 0 |
| 17 | `proc.exit` | proc id | reason class (`[conc.proc.exit]`) |

`cancel.check` with `cancelled = false` records only from an explicit
`checkpoint()` under the scheduler's grant; the runtime's poll-backstop
probes are timing noise, not decisions, and never enter the stream.

Serialized form (frozen v1; c15's flight recorder reads it, and any
change bumps the version): a `sched-ev/1` header line, then one
`index kind subject choice` line per record, decimal, space-separated.

## 2. The hook shape `[sched.point.hook]`

One seam, one site per decision: `sched_point(kind, subject) ->
Decision`. Release builds compile the seam to the production choice
with zero cost (D15/D5 — bench-verified); test builds route to the
pluggable scheduler. Recording emits `(index, kind, subject, choice)`
tuples — the v2 flight-recorder schema (c15) extends this tuple, never
replaces it. s32's implemented seam (event slice + cfg(test) observer,
numbering deferred) conforms.

## 3. Seeds and replay `[sched.seed]`

Resolves S-9. A seed selects a schedule deterministically. The seed
namespace is split at bit 62: low = simple seeds (human-typed, PRNG
schedule selection); high = packed mixed-radix encoded schedules (the
explorer's minimal counterexamples replay from the seed alone). The
interpreter's implemented encoding is adopted as normative for
`sched-ev/0`.

`sched-ev/1` (Phase B, assigned): the radix at each decision point is
its live alternative count — a `pick`'s ready-set size, a
`select.arm`'s ready-arm count — discovered during execution, never
tabulated. Packing is little-endian mixed radix (the first decision is
the lowest digit): decode `choice = v % radix; v /= radix` per
decision, an exhausted value keeps choosing 0; encode is the inverse
and fits iff the product of radixes stays below 2^62. A packed seed
renders as the schedule token `w1-<base32>` (lowercase Crockford-style
alphabet `0-9abcdefghjkmnpqrstvwxyz`, ≤ 13 digits) — the short,
quotable form the bs07 field feedback asked for in place of 19-digit
decimals; decimal and token forms are interchangeable everywhere a
seed is accepted. `--replay=ev:<stream>` replays an explicit decision
stream; `[proto.seed.equal]` comparisons remain one-sided as specced.

## 4. Stability contract `[sched.stable]`

Within one `sched-ev` version: same program, same seed, same decision
stream, byte-identical observable output. Adding a NEW point kind
appends to the taxonomy (never renumbers); implementing a RESERVED kind
activates its name. Cross-version (0 vs 1) comparison is by verdict,
never by stream — the CI-checkable conformance property between the
native sampler and lupin's exhaustive explorer (is07) is VERDICT
STABILITY: every seed's verdict for a program agrees, and the native
sampled schedule set is a subset of the explorer's frontier.

## 5. The flag surface `[sched.flags]` (X12 naming — decided 2026-08-11)

The X12 naming decision, closed on bs07's ch17 field evidence (the
three ledger observations: `--seed`/`--schedule` overlap confusingly;
`--explore` lives on a different subcommand than the workflow it
belongs to; 19-digit counterexample seeds are unquotable):

- **`--schedules=N`** — exploration, on the SAME subcommand as normal
  test running (`wolf test`; `wolf run` when it grows the surface).
  Runs each test N times under derived seeds (root seed printed;
  per-test seeds split from it); a failure prints its seed, the event
  trail, and a copy-pasteable `--replay=` line. The plural reads as
  the activity it is — resolves observation (b).
- **`--replay=<seed | w1-token | ev:stream>`** — the ONE reproduction
  verb. There is no separate `--seed` and no seed-or-stream
  `--schedule`: choosing a fresh schedule is `--schedules=N`'s job,
  reproducing a found one is `--replay`'s, and `--replay` accepts
  every schedule spelling (decimal seed, `w1-` token, explicit `ev:`
  stream — the editable form). Resolves observation (a).
- **`w1-` schedule tokens** (§3) — packed counterexample seeds print
  as ≤ 13 base32 digits, never 19-digit decimals. Resolves (c).
- **`--chaos`** — the name is kept (X12/D23); the injection engine is
  NOT in the s36 slice. It lands on the taxonomy's named seams
  (`io.arrive` delay/reorder in the simulated reactor, `proc.kill`
  injection, short reads/errors at the s35 submit/deliver edge),
  seeded and replayable by the same mechanism. Owner: the c07
  campaign closeout carries it as a named handoff.

`lupin` keeps its `sched-ev/0` surface; cross-tool comparison is by
verdict (§4), so the interpreter's flag alignment is an ic-track
decision, not forced here.

## 6. The v1 test scheduler, honestly `[sched.engine]`

The native engine (wolf_rt `task::det`) serializes tracked tasks under
a baton with grants at the runtime-owned seams; decisions draw from
the seed. Facts a consumer must know:

- **Practical determinism**: grants wait for quiescence plus a settle
  window (poll backstops bound wakeup latency), the CHESS/Coyote
  posture. The guarantee is enforced by CI's run-twice-and-diff, not
  by proof.
- Subject ids are normalized to first-seen order per id space, so a
  recording replays across processes.
- `park`/`unpark`/`steal` collapse into `pick` (§1.1) — pool plumbing
  is not schedule semantics.
- Virtual time: timer arms fire at whole-domain idleness, earliest
  `(duration, arm order)` first; no real sleeping.
- Known v1 gaps (c07 closeout ledger): a contended `when` cell
  hand-off is claimed by poll order, not by a recorded decision; proc
  trees parent under the root domain, outside the det tracking
  domain; real I/O under the det scheduler is unvirtualized (the
  simulated reactor's chaos seam lands with `--chaos`).
