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

## Status: TWO EXCEPTIONS SPENT (human ruling, 2026-08-21, run seven)

The drafts below were staged at s103 (posture ruled 2026-08-21:
drafted in advance, spent deliberately). At run seven — geomean
1.095x, the loss set exactly these two, nothing undiagnosed anywhere
in the suite — the human ruled: spend them. Marker lines follow each
entry; the gate counts 2 of the cap of 3. Revisit conditions stand:
b3's at any sound discharge for input-derived values or an X3
amendment; word_count's expires the day sc14 lands.

- exception: b3_churn
- exception: word_count

> **b3_churn** — checked adds on input-derived values.
> *Root cause:* the request loop's arithmetic operates on values that
> arrive from the request itself (`id + j` and the size-mask sums);
> they lower to `iadd.chk`, and no sound discharge exists — s99's
> facts rightly cannot bound values the program did not compute (D44
> second addendum: no range fact without a proof).
> *Decision that renounced the win:* X3 — checked arithmetic in every
> profile, release included.
> *Measured cost:* callgrind A/B against a wrapping-typed variant of
> the same kernel (same sink): `_Wmain` 844,129 → 678,122 Ir —
> **83 Ir/request, 19.7% of `_Wmain`, 8.3% of program**. Spending
> this exception explains ~0.115x → ~0.125x and NO further: the
> kernel's remaining gap is design cost and midend candidates, both
> in the ledger's s103 account, and this entry does not cover them.
> *Revisit condition:* container element facts or an input-range
> annotation surface ever lets the checks fold (s100-family, HUMAN),
> or the midend candidates land and the X3 share becomes the majority
> of what remains.

> **word_count** — boundary-computing iteration vs a boundary-free hand scan.
> *Root cause (corrected 2026-08-22, superseding the stale List[str]
> materialization story — that bug was fixed by s84, per the kernel's
> own header; the wc diagnosis lane proved the truth three ways):*
> `words()` yields zero-copy views (D25), so the fused loop (s84,
> `[mem.str.view]`) locates every word's start and end — a
> data-dependent branch per byte, mispredicting on the word rhythm
> (callgrind: 4.5 branches/byte, 972k mispredicts vs naive C's 29 at
> EQUAL instruction counts). Naive C counts state transitions without
> ever locating a boundary; clang compiles it branchless. The program
> discards the views and keeps only the count; it pays for generality
> it does not use.
> *Decision that renounced the win:* D25 — `words()` is a view
> iterator, not a counting primitive; boundary computation is its
> contract.
> *Measured cost:* 0.886x ritual (stable, runs 4–9). The same
> algorithm byte-shaped in wolf: 0.965x (control, 2026-08-22);
> d1_utf8_validate's 1.39x WIN bounds wolf's byte-scanning as
> competitive. Checked adds ~1.1% of Ir — not the mechanism.
> *Revisit condition:* a midend transform degrading a boundary-yielding
> iterator with unused views into a counting automaton (a new
> transform class, unowned), or a kernel-thesis amendment. The former
> "expires when sc14 lands" condition is void — the kernel imports no
> std and an std `each_word` walks the same boundaries.
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
