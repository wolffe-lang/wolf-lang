# Wolf Spec — 07: Schedule Points (s36 Phase A)

Status: normative for hook SHAPE and vocabulary; event numbering for the
native runtime (`sched-ev/1`) is reserved-unstable until s36 Phase B
assigns it. `sched-ev/0` (the reference interpreter's stream) is stable
and unchanged. This document gates c07 implementation merges (s36's
Phase A contract): every runtime seam matches this shape or does not
merge.

## 1. The taxonomy `[sched.point.set]`

A schedule point is a runtime-owned decision that can alter observable
interleaving. The closed v1 set:

- `spawn` — a task becomes runnable (parent continues vs child runs is
  a decision only under the test scheduler).
- `join` — a scope waits; which pending task runs next is a decision.
- `park` / `unpark` — a worker blocks or wakes (production-only detail;
  under the test scheduler these collapse into `pick`).
- `chan.send` / `chan.recv` — blocking channel edges (s33 widens with
  its hook inventory, which must match this doc).
- `select.arm` — the committed arm among ready arms.
- `pick` — the generic "which runnable task next" decision; every other
  point reduces to at most one `pick` plus its own identity.
- `timer.fire` — RESERVED (s35's inventory names it; unimplemented
  points stay named here so numbering never shifts).

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
`sched-ev/0`; `sched-ev/1` adopts the same split with its own radix
table at Phase B. `--schedule=ev:<stream>` replays an explicit decision
stream; `[proto.seed.equal]` comparisons remain one-sided as specced.

## 4. Stability contract `[sched.stable]`

Within one `sched-ev` version: same program, same seed, same decision
stream, byte-identical observable output. Adding a NEW point kind
appends to the taxonomy (never renumbers); implementing a RESERVED kind
activates its name. Cross-version (0 vs 1) comparison is by verdict,
never by stream.
