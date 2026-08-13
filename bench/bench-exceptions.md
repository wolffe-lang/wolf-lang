# Documented benchmark exceptions

A kernel losing to naive `clang -O3` by more than 10% fails the M2 primary
gate **unless** it appears here. An exception is only ever for contract
class **(d)**: the win requires undefined behaviour wolf renounced, or a
semantic wolf chose on purpose. A missing container, an unencoded fact or a
dropped metadata channel is a **bug to fix**, never an exception — those
live in `bench/loss-ledger.md`.

Each entry states the kernel, the root cause, the decision that renounced
the win, the measured cost, and the condition under which it is revisited.

**Cap: 3 exceptions.** The cap is enforced by
`xtask::t1::evaluate_gate`, and exceptions are reviewed again at s64.

The gate reads this file, so each entry needs the marker line
`- exception: <kernel-directory-name>` for the kernel to count as excepted.

## Status at s44: EMPTY. No exceptions are claimed.

The ledger exists (the contract requires it to, even empty) and it is
deliberately unused, because claiming an exception right now would be
dishonest in both directions.

Twelve of thirteen kernels lose by more than 10%, and **eleven of them are
class (b) or (a)** — a missing bulk container (gap G1), string iteration
through runtime shims (G2), a canonical-loop gap on `e3` (G3's tail),
trivially-false checks surviving into LLVM IR (G4). Those are the fact
pipeline's work, not exceptions.

The one genuine class-(d) candidate is **`e1_sum_reduce`**: checked
arithmetic (X3) lowers a reduction to `llvm.sadd.with.overflow.i64`, LLVM
will not vectorize that, and the identical loop over `wrapping[int]` both
vectorizes and lands within noise of `clang -O3` (0.1425 vs 0.1403 ns/op,
floor 3.4%). Measured cost of the renunciation on this kernel: **+159.6%**.
That is a real, deliberate, well-understood trade.

It is still not filed as an exception, for two reasons:

1. **Three exceptions cannot rescue this suite.** The primary gate needs a
   geomean ≥ 1.00 and the suite sits at 0.030x. Filing exceptions would
   change nothing about the verdict and would put three "expected loss"
   stamps on the record before the numbers they excuse have been separated
   from G1's shadow.
2. **X3's cost is bimodal, and the ledger should record it once it is
   understood, not once it is measured.** The same renunciation costs
   +159.6% on `e1`, +1.9% on `e2` and +0.0% on `e3`. What deserves an
   exception is a *shape* — "checked reduction over a vectorizable loop" —
   and naming that shape properly needs the loop-versioning and
   range-recovery work to have had its say on it first. The breach is
   already recorded where it belongs: the **D2/X3 revisit clause** is
   triggered by the +38.3% family-E geomean, with a decision-log entry and
   the numbers attached, and a human decides.

When an exception is eventually filed it should look like this (kept here as
the template, commented out of the machine-readable set by having no marker
line):

> **e1_sum_reduce** — checked-arith reduction.
> *Root cause:* the accumulate step lowers to
> `llvm.sadd.with.overflow.i64` with a cold trap edge; LoopVectorize will
> not vectorize an overflow-intrinsic reduction.
> *Decision that renounced the win:* X3 — checked arithmetic in every
> profile, release included.
> *Measured cost:* +159.6% vs the `wrapping[int]` variant of the same loop,
> which ties `clang -O3` within the layout-noise floor.
> *Revisit condition:* s42 proves the accumulator's range (making the check
> foldable), or a vectorizable checked-reduction lowering is found (a
> widened add plus one saturating check per vector, trap on the reduced
> flag).
