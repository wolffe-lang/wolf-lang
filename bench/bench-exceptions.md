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

## Status at s85: still EMPTY — and the one candidate is now withdrawn.

The `e1_sum_reduce` exception drafted below was never filed, on the
stated condition that it should wait until "s42 proves the
accumulator's range (making the check foldable)". s85 did that: a
loop-carried accumulator is bounded by the trip count times its
per-iteration increment, and where the trip count is opaque the
versioner hoists the bound test. On this host, re-measured back to
back with the pre-s85 compiler:

| kernel | checked | wrapping | X3 delta | was |
|---|---|---|---|---|
| `e1_sum_reduce` | 0.1500 | 0.1500 | **+0.0%** | +177.0% |
| `e2_checksum` | 0.8214 | 0.8571 | −4.2% | −0.3% |
| `e3_index_arith` | 0.1250 | 0.1290 | −3.1% | −1.9% |
| **family E geomean** | | | **−2.4%** | **+39.4%** |

`e1` goes from 0.366x to 0.971x against naive `clang -O3`, inside its
own noise floor. The revisit condition on the draft below is met, so
the template stays a template: there is no longer a measured cost to
except. What X3 costs on this suite is now zero to within the floor,
and the remaining `e3` loss (0.517x) is not an X3 cost — its checked
and wrapping lanes agree — but a separate gap already in the ledger.

## Status at s79: still EMPTY. No exceptions are claimed.

The s44 reasoning below is unchanged in substance and its numbers have
been re-measured. What moved: **seven** of thirteen kernels now lose by
more than 10% rather than twelve, the suite geomean is 0.476x rather than
0.030x, and `e1_sum_reduce`'s renunciation costs **+164.7%** rather than
+159.6% (same shape, measured against a clock that can now resolve it).
Neither of the two conclusions changes — three exceptions cannot rescue a
0.476x geomean, and X3's cost is still bimodal (+164.7% on `e1`, −2.2% on
`e2`, −0.6% on `e3`, the last two inside their floors, i.e. free).

## The s44 status, kept for the record: EMPTY. No exceptions are claimed.

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
   geomean ≥ 1.00 and the suite sat at 0.030x (0.476x at s79). Filing exceptions would
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
