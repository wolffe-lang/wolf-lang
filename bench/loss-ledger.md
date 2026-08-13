# The T1 loss ledger (s44)

Every kernel wolf loses, diagnosed. The contract's classification:

- **(a)** the fact exists in WIR but was not encoded or used (an s41
  metadata or s42 pass gap);
- **(b)** the fact does not exist yet (a checker or s26 lowering gap — files
  into the c04/c05 backlog);
- **(c)** LLVM dropped or ignored our metadata (grow the s41 fuzz corpus;
  consider the mid-end taking the rewrite over);
- **(d)** the win requires UB we renounced, or a deliberate semantic.

(a)–(c) get fixed in the fact pipeline with the kernel as the regression
test. Never by special-casing a benchmark shape. (d) becomes an entry in
`bench/bench-exceptions.md`.

Measured on 2026-08-12, `cargo xtask bench --track=t1 --runs=7`, clang
22.1.8, rustc stable, linux x86-64. `perf` was unavailable on this host, so
instruction counts are absent; `llvm-profdata` refused the profile
(version skew against clang 22), so the PGO scrutiny lane is absent. Both
are host facts, recorded so nobody reads their absence as a result.

## The headline

**M2 is NOT declared.** Suite geomean vs naive `clang -O3` is **0.030x** —
wolf is ~33x slower across the suite, and 12 of 13 kernels lose by more than
10%. One kernel (`e2_checksum`) ties. The layout-noise floor is 2.2–9.3% per
kernel, two orders of magnitude below the effects being reported, so
layout bias explains none of this.

Per family, because a geomean must never launder a family-shaped loss:

| family | geomean vs naive C | reading |
|---|---|---|
| A aliasing | **0.004x** | ~240x slower |
| B arena | **0.031x** | ~32x slower |
| C layout | **0.032x** | ~31x slower |
| D strings | **0.012x** | ~83x slower |
| E arithmetic | **0.573x** | ~1.7x slower — the only family in the same order of magnitude |

Scrutiny lane, ungated (report-10 amendment 5): geomean vs expert C
**0.0092x**, vs `clang -O3 -march=native` **0.0227x**, vs `rustc -O`
**0.0283x**. The gated comparison is the *most* favourable of the four. That
is what the scrutiny lane is for.

## G1 — there is no vectorizable bulk container (families A, B, C)

**Classification (b), and it is the whole story for 10 of the 12 losses.**

`List[T]` is wolf's only bulk container at this surface, and every element
access is an **out-of-line call into the runtime**: `xs[i]` lowers to
`__wolf_rt_list_read(hdr, idx, out_slot)` through a caller stack slot with a
bounds check inside the callee (`wolf_wir::lower::lower_list_method`,
`wolf_rt::list`). There is no data pointer, no fixed-size array type in the
language, and `stack.alloc` takes constant sizes only. Measured cost of one
element touch: **44–92 ns**, against 0.18–1.05 ns for the same loop in C.

The consequence is worth stating precisely, because it is easy to
misdiagnose: **the aliasing facts are correct, complete, and worthless
here.** The backend does emit `noalias`/`readonly`/`dereferenceable` on the
pointer parameters and one `!alias.scope` per region; the mid-end does run.
None of it can help, because the loop body is a call to an opaque function,
not a memory access LLVM can move or widen. The vectorization witnesses say
so without ambiguity: naive C vectorizes 1–2 loops per A-family kernel,
wolf vectorizes **zero** in every kernel of the suite.

So family A does not measure aliasing today, family C does not measure
layout, and family B does not measure allocation strategy. All three measure
the container. `list_alloc` is the cleanest illustration: wolf's region-built
structure costs 62.5 ns/node against naive C's **malloc-and-free per node**
at 25.4 ns/node — wolf's arena is 2.5x slower than the malloc discipline it
was supposed to beat — while the hand-rolled expert-C arena is 2.27 ns/node.
The region machinery is not what is slow; the 10000 `push` calls are.

**Fix:** a bulk container with inlinable element access — a fixed-size array
type, or a `List` whose indexing lowers to `ptr.off` + `load`/`store` with
the bounds check in the caller (the `ptr.off` opcode, the `!range` channel
and the trap blocks all already exist in the LLVM tier; nothing new is
needed below WIR). Until that lands, families A, B and C cannot be scored
against C on their own theses, and no amount of mid-end work will move them.
Files into c04/c05 as the top-priority M2 blocker.

## G2 — string and byte views go through the runtime too (family D)

**Classification (b), same shape as G1.** `bytes()`, `words()` and range
slicing are genuinely zero-copy — no allocation, no copy — but each step is
a runtime shim call, so a byte scan costs 58–75 ns/byte against C's
0.63–0.91 ns/byte. D's thesis (zero-copy views beat copying) is not what the
numbers are testing; iteration overhead is. Fix rides G1: an inlinable byte
cursor.

## G3 — checked arithmetic blocks the vectorizer (family E)

**Classification (d) — a deliberate semantic — with an (a) tail.**

This is the only family where the comparison is actually about codegen, and
the result is the most useful number this sprint produced:

| kernel | wolf checked | wolf wrapping | naive C | verdict |
|---|---|---|---|---|
| `e1_sum_reduce` | 0.370 ns/op | **0.1425** | 0.1403 | checked LOSS 0.379x; **wrapping TIES clang** (1.6% apart, floor 3.4%) |
| `e2_checksum` | 0.790 | 0.775 | 0.778 | **TIE** at 0.985x |
| `e3_index_arith` | 0.115 | 0.115 | 0.0578 | LOSS 0.502x, and the checks are NOT the cause |

`e1` is the mechanism in one line: the reduction lowers to
`llvm.sadd.with.overflow.i64` with a cold trap edge, which LLVM will not
vectorize; the `wrapping[int]` variant of the identical loop **does**
vectorize (witness: 1 vectorized loop) and lands within noise of `clang
-O3`. So the release backend is competitive on a loop it is allowed to
vectorize — the cost is X3's, not the backend's.

**X3 tracker: +38.3% geomean across family E** (`e1` +159.6%, `e2` +1.9%,
`e3` +0.0%), against the ~2–3% revisit threshold. This is a breach by more
than an order of magnitude, so per the contract it does not fail M2 by
itself — it **triggers the D2/X3 revisit clause**: a decision-log entry with
these numbers, and a human decides. The shape of the finding matters as much
as the size: the cost is bimodal, not uniform. Where a loop is
latency-bound (`e2`'s serial multiply chain) the checks are free; where a
loop is a vectorizable reduction (`e1`) they cost 1.6x. A geomean over a
different kernel mix would produce a different "X3 overhead", so the honest
report is the distribution, not the single number.

`e3` carries the (a) tail: its checked and wrapping lanes are **bit-identical
in time** (x3 = +0.0%) and *neither* vectorizes, while naive C's masked
scaled-index loop does. Removing the checks is therefore not sufficient
there — something else in the emitted loop shape stops LoopVectorize
(candidates: the mask-fold reduction form, the phi/trampoline shapes at the
latch, absent `!llvm.loop` metadata). This is a genuine s42 canonical-loop
gap and the kernel is its regression test.

## G4 — trivially-false checks survive the mid-end into LLVM IR

**Classification (a), small and cheap to fix.** The emitted IR still carries
guards against constant divisors: `icmp eq i64 4096, 0` (div-by-zero) and
`icmp eq i64 4096, -1` (the INT_MIN/-1 pair) — 4 of them in
`e1_sum_reduce`'s module alone. LLVM folds them, so they cost no run time,
but they are pure IR volume and they inflate exactly the metric issue #70
budgets. Folding a checked div/rem against a constant divisor at WIR level
is a small s42 peephole.

## Issue #70's two metrics, measured

1. **LLVM IR instruction count handed to LLVM ≤ 50% of the naive s41
   lowering (geomean):** **NOT MET.** 57.7% across the corpus (n=99 run
   entries, 9 refused by Tier-R), 87.5% across the 13 hot kernels. The
   corpus figure independently corroborates s42's volunteered WIR-level
   58.2%, which is a good sign for the mid-end's self-reporting and a bad
   sign for the budget. G4 is part of the gap; the larger part is that the
   mid-end's wins are concentrated in shapes the kernels do not contain.
2. **LLVM's share of Tier-R build wall time ≤ 50%:** **MET**, at 20.9%
   geomean. The anti-rustc posture (reports/04: rustc lets LLVM eat 70–80%)
   holds with room to spare. Measured without instrumenting the driver:
   `t_total` is a real clean `--release` build and `t_llvm` is `clang -x ir
   -c -O2 -fPIC` on the exact module that build hands clang, with the same
   flags and the same binary — for these single-cluster kernels the two are
   the same invocation, so the ratio is not an estimate.

## The metadata-drop sentinel, first reading

Family A, `stripped / annotated − 1`: `alias_daxpy` **−1.5%**, `a2_stencil1d`
**+4.4%**, `a5_hoist_call` **+0.4%** — geomean **+1.1%**, inside the
per-kernel layout floors. **The bonus channel is worth approximately nothing
on this suite today**, and G1 says why: with the loop body reduced to an
opaque runtime call there is nothing for `noalias` to license. Recorded as
the baseline reading, not as a verdict on D3 — the sentinel becomes
informative the moment G1 lifts, and a *negative* reading then would be the
real alarm. (An earlier 3-run pass reported +20–29% here; at 7 runs it
collapses to +1.1%. The 3-run numbers were noise, and this line is the
reason the nightly lane runs 10.)

## Family F cannot be measured at all

Not a loss — a blocker. The release tier refuses `Opcode::FuncAddr` ("the
release tier does not lower concurrency yet"), so **no wolf task, channel or
proc program compiles through Tier-R**. `f1-channel-pingpong` and
`f2-par-map-scaling` cannot be built in the release profile, and s33's
expectation of M2 evidence from this suite is blocked until Tier-R lowers
concurrency. The debug tier runs those programs fine; comparing a Cranelift
build against `clang -O3` would be a benchmarking crime, so no number is
published.

## Kernels the contract names that are not implemented

`a3-stencil2d`, `a4-matvec`, `b1-tree-build-walk`, `b2-json-dom`,
`b4-pointer-chase`, `c1-particles`, `c3-niche-filter`, `d4-format`. Each is a
shape of a kernel already in the suite, and while G1 dominates every
array-shaped result they would add measurement time without adding a
finding. They are the first thing to add when G1 lifts — at which point the
suite grows to the contract's ~18 and the numbers mean what the contract
intended them to mean.
