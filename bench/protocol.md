# The T1 comparison protocol (s44, the M2 gate)

This is the in-repo protocol document the s44 contract requires. It says
exactly how "wolf beats `clang -O3`" is measured, which numbers are allowed
to fail a build, and which of the benchmarking crimes we have and have not
closed. If you want to refute the M2 claim, everything you need is here and
in `bench/kernels/manifest.json`; if a step below is wrong, the claim is
wrong, and we would rather you found it than a reader of a blog post.

Run it:

```
cargo xtask bench --track=t1 --runs=10 --out=bench-results/t1.jsonl
cargo xtask bench --track=irvolume --out=bench-results/irvolume.jsonl
cargo xtask bench gate bench-results/t1.jsonl      # the M2 verdict
cargo xtask bench-gates                            # the deterministic gates
```

## 1. The suite

Five families, thirteen kernels, declared in `bench/kernels/manifest.json`.
Each kernel directory holds:

| file | role |
|---|---|
| `kernel.lu` | the wolf implementation — idiomatic, no `unsafe`, no hand-tuning a normal user would not write |
| `ref.c` | **naive C**: what people actually write. This is the GATED comparison |
| `expert.c` | hand-`restrict`/hand-SoA C: what experts write (families A and C). Report-only |
| `ref.rs` | Rust `-O`. Report-only |
| `wrapping.lu` | family E only: the same kernel over `wrapping[int]`. The checked-vs-wrapping delta IS the X3 overhead number |

Families and their theses:

- **A — aliasing** (`alias_daxpy`, `a2_stencil1d`, `a5_hoist_call`): wolf's
  `mut`/`read` param modes prove disjointness that naive C needs
  hand-`restrict` for (reports/01 facts 1–2).
- **B — arena vs malloc** (`list_alloc`, `b3_churn`): region bump
  allocation and wholesale free vs malloc/free discipline (fact 5).
- **C — layout** (`aos_dot`, `c2_ecs_sweep`): **layout-free internal ABI +
  SoA-as-idiom vs C's fixed AoS ABI.** This wording is report-10 amendment
  4, the family-C honesty fix: wolf does **not** choose your layout at v1.
  What it gives you is an internal ABI with no C struct-layout obligation,
  so the SoA idiom costs nothing at the boundary. Compiler-chosen layout is
  a post-v1 campaign on I9's hooks, not a benchmark footnote.
- **D — strings** (`word_count`, `d1_utf8_validate`, `d2_substr_search`):
  `str` views (D24/D25) against hand-written byte scans.
- **E — checked arithmetic** (`e1_sum_reduce`, `e2_checksum`,
  `e3_index_arith`): X3's empirical defence, priced.

Not implemented at s44, and why (the full accounting is in
`bench/loss-ledger.md`): `a3-stencil2d`, `a4-matvec`, `b1-tree-build-walk`,
`b2-json-dom`, `b4-pointer-chase`, `c1-particles`, `c3-niche-filter`,
`d4-format` are shapes of kernels already present, and they would add
measurement time without adding a distinct finding while the container gap
(G1 in the ledger) dominates every array-shaped result. **Family F
(concurrency) cannot be measured at all**: the release tier refuses
`func.addr`, so no wolf task or channel program compiles through Tier-R
(`wolf_codegen_llvm::emit` — "the release tier does not lower concurrency
yet"). s33's M2 evidence is therefore blocked, not pending.

## 2. Toolchains and flags

- wolf: `wolf build kernel.lu -o bin --release`. **No per-kernel flags, no
  PGO** (D4: measurements are on default whole-program release builds).
  Internally that is WIR → s42 mid-end → s43 whole-program → LLVM IR →
  `clang -x ir -c -O2 -fPIC`. The `-O2` is the backend's own hardcoded
  level, not a benchmark choice.
- naive C: `clang -O3 ref.c` (the gate).
- expert C: `clang -O3 expert.c` (report-only).
- Rust: `rustc -O --edition=2024 ref.rs` (report-only).
- Scrutiny lane, **ungated**, report-10 amendment 5: `clang -O3
  -march=native ref.c` and PGO'd `clang -O3 ref.c` (instrument → train on
  1/8 of the sweeps → `llvm-profdata merge` → `-fprofile-instr-use`).
- Exact compiler version strings land in every record's `config` field.

Every lane's version, flags and machine facts ride in the report JSON. A
number without its toolchain is not a measurement.

## 3. Measurement

- **Self-timed regions.** Each kernel times its own hot loop and prints
  `{"ns":…,"ops":…,"sink":…}`; the harness divides to get ns per unit of
  work. Process startup is therefore outside every measurement.
- **wolf's clock is milliseconds** (`time_now_ms` is the only monotonic
  clock the language exposes). Kernels are sized so the wolf lane spends
  150–400 ms under the clock, i.e. quantisation is ≤ 0.7% — below the 2%
  practical floor. When a finer clock exists, sizes can shrink.
- **Paired, interleaved runs.** N ≥ 3 (nightly: 10) passes; within each
  pass the lane order is **rotated**, so no lane systematically owns the
  warm cache or the drift at the end of the sweep.
- **Median + MAD**, published per lane. Significance uses the s01/rustc-perf
  model already in `bench diff`: a delta must clear both 3×MAD and a 2%
  practical floor.
- **Full distributions.** Every individual sample is a record in the report
  JSON with its layout tag (report-10 amendment 5), not just median+MAD.
- **Instruction counts** via `perf stat` where `perf` exists, for stability
  beside wall time for honesty. `perf` is not available on every host; when
  it is missing the lane says so and the wall numbers stand alone.
- **Equal sweep counts across lanes.** The harness supports different
  counts per lane, but the suite sets them equal, because every kernel's
  correctness sink chains across sweeps and unequal counts would make the
  lanes compute different values. The consequence is honest and visible:
  where wolf is 100× slower, the C lane's sample is ~1 ms, so its MAD is
  published and a reader can judge its resolution for themselves.

### Correctness first

Every kernel prints a `sink`. Before a single timing sample is taken, all
lanes run once and their sinks are compared — exactly for integers, to a
relative tolerance of 1e-12 for floats (three compilers over IEEE
arithmetic are not one program twice). A divergence aborts the kernel. A
performance number from a lane that computes something else is worse than
no number at all.

### The folded-workload guard

A lane whose measured cost falls below 0.01 ns per unit of work did not run
the workload; it proved the answer and printed it. Such a lane contributes
no comparison, and the report records `folded_workload` instead. This is not
hypothetical: the first full run caught `e3_index_arith`'s Rust lane, where
rustc solved the closed form of a loop over a compile-time-constant bound
and "beat" wolf by 200000×. The Rust references now pass their loop inputs
through `std::hint::black_box`, and the guard stays as the backstop.

## 4. Layout-bias control (report-10 amendment 1, GATE-RELEVANT)

Stabilizer: once code/stack/heap layout is randomized, -O3 vs -O2 on SPEC
CPU2006 is statistically indistinguishable from noise. Mytkowicz: the size
of the environment block, or the order of object files at link time, is
enough on its own to flip a paper's conclusion; of 133 surveyed papers, none
adequately controlled measurement bias. Those effects are the same size as
the effect M2 claims. Without controlling them, M2 is refutable by
construction.

So the gated pair (wolf, naive C) is measured under four configurations:

| id | environment padding | ASLR |
|---|---|---|
| L0 | none | on |
| L1 | 4 KiB | on |
| L2 | 64 KiB | on |
| L3 | none | off (`setarch -R`) |

Every reported median comes from **L0**. The **layout-noise floor** is the
relative spread of the medians across configurations, `(max − min) / min`,
computed per lane; a kernel's floor is the **maximum** of the wolf and
naive-C floors — the conservative direction, because a bigger floor
swallows more claimed wins. **A win below the floor is a TIE, regardless of
MAD.** The floor never drops below the 2% practical floor.

Known limitation, stated rather than hidden: **link order is not
perturbed.** A single-object kernel has no link order to permute, and wolf's
driver links internally with no ordering knob. That axis is therefore
unexercised, so the reported floors may understate the true layout
sensitivity. Where `setarch` is unavailable, L3 is skipped and the floor
narrows further; the harness says so on stderr. Both are why the floor takes
the max across lanes instead of the wolf lane alone.

## 5. Vectorization witnesses (report-10 amendment 2)

For families A, C and E the harness stores LLVM optimization remarks per
kernel: `clang -O3 -Rpass=loop-vectorize` on `ref.c`, and `clang -x ir -O2
-Rpass=loop-vectorize` on the module wolf's backend actually emits (same
level the backend itself uses). The counts are deterministic, so **they
gate**: `bench/gates.json` records a floor per lane per kernel, and
`cargo xtask bench-gates` fails when a loop that used to vectorize stops.
A de-vectorized loop is thereby visible before any wall clock moves.

## 6. The metadata-drop sentinel (report-10 amendment 3)

A **permanent** lane, family A, one extra compile and run per commit:
`WOLF_STRIP_FACTS=1` lowers with every fact channel silenced — no
`noalias`/`readonly`/`dereferenceable`, no scoped-noalias, no `!range`,
`!invariant.load` or `!prof`, alignment claims dropped to 1. The reported
`metadata_bonus` is `stripped / annotated − 1`: what the bonus channel is
worth today. Metadata is a bonus (D2) — our own mid-end exploits the same
facts in WIR, so LLVM dropping it costs speed and never correctness.
Pricing it per commit is how channel decay on an LLVM bump shows up as a
number instead of as a mystery. Ungated, always published.

## 7. What gates, and what only reports (D5)

s31's first `bench diff --gate` trip was `max_rss +37.8%` on an xtask-only
commit, next to wall times "improving" 60%: no variance floor, no credible
gate. The rule that came out of it stands — **only deterministic metrics
gate merges** — and this sprint does not repeal it. It builds the floors so
the wall numbers become publishable, not so they become gates.

**Gating** (`cargo xtask bench-gates`, a step of `cargo xtask ci`):

| metric | why it can gate |
|---|---|
| LLVM IR instruction ratio (mid-end vs naive), kernels and corpus | same input, same count, any machine |
| vectorization witness counts per kernel per lane | remark counts are a function of the IR and the clang version |
| correctness sinks across all lanes | equality, not timing |
| `bench diff --gate` on `*_hits` / `*hit_rate` counters | unchanged from s31 |

**Report-only** (nightly, dedicated runner, published with distributions):
every `ns_per_op`, every speedup and geomean, the X3 overhead delta, the
metadata-bonus delta, LLVM's share of build wall time, `max_rss`, and all
`*_per_sec` throughput. These decide **M2** — over three consecutive
nightly runs on the dedicated runner — but they never fail a PR.

## 8. The M2 gate

Primary (contract §3), computed by `cargo xtask bench gate <t1.jsonl>`:

1. geomean speedup vs naive `clang -O3` across the suite ≥ **1.00**;
2. no individual kernel losing by more than **10%** unless covered by a
   documented exception in `bench/bench-exceptions.md`;
3. exceptions capped at **3**.

Clause 2 is the one that matters most: `evaluate_gate` is unit-tested
against a suite whose geomean is exactly 1.000 while one kernel loses 4×,
and it refuses. A geomean must never launder a family-shaped loss, which is
also why every report prints **per-family geomeans beside the suite
number**.

Secondary, reported and never gated: geomean vs expert C (target ≥ 0.95),
geomean vs `rustc -O` (target ≥ 1.00).

X3 tracker: the checked-vs-wrapping geomean across family E, against the
~2–3% revisit threshold. A breach does not fail M2 by itself — it triggers
the D2/X3 revisit clause: a decision-log entry with the numbers, and a human
decides.

`bench gate` re-derives every verdict from the raw ratio and the raw floor
and **refuses to run if a stored verdict disagrees with the re-derived
one**: the report JSON and the decision procedure are not allowed to drift
apart.

## 9. The benchmarking-crimes checklist (Heiser), answered

Adopted per report-10 amendment 5. Where we are still guilty, it says so.

| crime | status |
|---|---|
| **Selective benchmarking** — reporting only favourable subsets | Closed. Every kernel in the manifest is reported, per family, wins and losses alike; the M2 gate reads all of them. Kernels may be added post-M2; removing or weakening one requires the exception process. |
| **Improper baseline** — comparing against a straw man | Closed, and one was found: the file s01 shipped as `alias_daxpy/ref.c` was the **`restrict`** version, so the "naive C" gate was silently scored against expert C. It now lives in `expert.c`, and the naive lane is genuinely naive. |
| **Improper baseline** — comparing a hand loop to a tuned library | Closed by construction: `d1`/`d2` deliberately avoid `strstr`, `str::find` and `std::str::from_utf8`, all of which are hand-vectorized library routines. Every lane runs the same hand-written loop. |
| **Arithmetic mean of ratios** | Closed. Geomean everywhere ratios are averaged. |
| **Missing significance / no error bars** | Closed. Median + MAD, 3×MAD + 2% practical floor, full distributions in the report JSON. |
| **Missing layout-bias control** | Partly closed — the whole point of §4. Environment size and ASLR are perturbed; **link order is not**. Stated as a known limitation, mitigated by taking the max floor across lanes. |
| **Throughput/latency conflation** | Closed. Every kernel reports ns per unit of work; nothing is quoted as a rate. |
| **Unequal work between lanes** | Closed. Identical algorithm, identical inputs, identical sweep counts, and a sink comparison that fails the kernel if any lane computes something else. |
| **Downplaying the competition** | Closed by the scrutiny lane: `-march=native` and PGO'd clang are published beside the gated number, ungated, in the same report JSON. |
| **No steady-state detection** (vm-warmup) | Partly closed. AOT wolf is far less exposed than a VM, but "assume nothing, detect stability" is not implemented: we publish the full distribution and let a reader see multimodality rather than classifying it. Open item for s64. |
| **Unstated machine configuration** | Partly closed. Compiler versions and flags are recorded per record; frequency governor and SMT state are recorded by the runner, not by the harness — a dedicated-runner item, not a harness item. |
| **Cherry-picked N** | Closed. `--runs` is a parameter, its value rides the run, and nightly fixes it at 10. |

## 10. The honest iteration loop

Losing kernels are expected; laundering them is forbidden. Every loss is
diagnosed into one of four classes and recorded in `bench/loss-ledger.md`:
(a) the fact exists in WIR but was not encoded or used, (b) the fact does
not exist yet, (c) LLVM dropped our metadata, (d) the win requires UB we
renounced or a deliberate semantic. (a)–(c) are fixed in the fact pipeline
with the kernel as the regression test — never by special-casing a
benchmark shape. (d) becomes an entry in `bench/bench-exceptions.md`.
