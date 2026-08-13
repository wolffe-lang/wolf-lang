# The T1 loss ledger (s44, re-measured s75)

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
are host facts, recorded so nobody reads their absence as a result. The s75
re-run is on the same host, same command, same day as the s44 baseline it
is compared against.

## The headline

**M2 is NOT declared.** Suite geomean vs naive `clang -O3` is **0.191x** —
wolf is ~5x slower across the suite, and 9 of 13 kernels lose by more than
10%. Three kernels WIN (`alias_daxpy` 1.047x, `aos_dot` 1.497x,
`c2_ecs_sweep` 2.907x) and one ties (`e2_checksum`). The layout-noise floor
is 1.9–8.9% per kernel, still far below the effects being reported, so
layout bias explains none of this.

Against s44, which measured this suite before `List` element access became
memory the optimizer can see (#77, closed by s75):

| family | s44 | s75 | reading |
|---|---|---|---|
| A aliasing | 0.004x | **0.405x** | ~100x faster; `alias_daxpy` now wins |
| B arena | 0.031x | **0.049x** | barely moved — see G5, the real B gap |
| C layout | 0.032x | **2.086x** | the only family that WINS its thesis |
| D strings | 0.012x | **0.015x** | barely moved — see G2, unchanged |
| E arithmetic | 0.573x | **0.587x** | unchanged, as expected: no container |
| **suite** | **0.030x** | **0.191x** | 6.4x, from one lowering change |

The number that moved is the one G1 named. Element access cost **44–92 ns**
at s44; it is **0.24–1.00 ns** now, against C's 0.17–1.00 — the same order
of magnitude as C for the first time. What did NOT move is everything G1
was not: family D goes through `str` shims (G2), family B allocates
outside its own region (G5), family E pays for checked arithmetic (G3,
D44).

Scrutiny lane, ungated (report-10 amendment 5): geomean vs expert C
**0.199x**, vs `clang -O3 -march=native` **0.130x**, vs `rustc -O`
**0.192x**. The gated comparison is still the most favourable of the four.
Two individual results are worth naming because they invert: `alias_daxpy`
beats **expert** C (1.057x — hand-`restrict`ed, which is the point of
family A), and `c2_ecs_sweep` beats `rustc -O` by 2.96x.

## G1 — CLOSED at s75: the container is no longer a call

**Was classification (b), and it was the whole story for 10 of the 12
s44 losses.**

At s44 every element access was an out-of-line call:
`xs[i]` lowered to `__wolf_rt_list_read(hdr, idx, out_slot)` through a
caller stack slot, with the bounds check inside the callee. LLVM could not
vectorize across it, could not see the memory, and every `noalias`, scope
and `!range` fact the compiler correctly emitted had nothing to license.

s75 lowers indexing, iteration and assignment to `ptr.off` + `load`/`store`
through two token-rooted regions (`region.foreign`: one for container
headers, one for element buffers — always separate allocations, so the
disjointness is a theorem). What remains in `wolf_rt` is what genuinely
needs it: `list_new` mints a header, `list_push` grows a buffer. The
bounds check moved into the caller, where the range analysis can see it.

The witnesses say it plainly. `alias_daxpy` vectorizes (1 loop, floor
ratcheted in `bench/gates.json`) — the first vectorized loop this suite has
ever produced from wolf. Family C now measures layout: `c2_ecs_sweep`'s SoA
idiom beats naive C's AoS cache-line tax 2.9x, which is the number the
family was written to produce.

## G6 — the bounds checks that survive are the shape of the next gap

**Classification (a), new at s75.**

Bounds checks stay (X3's sibling: eliminating a check because it is
provable is optimization; eliminating it because it is inconvenient is
not). What s75 added is a **relational** π channel in `rangeopt`: the
comparison that held on the edge into a single-entry block decides a later
comparison over the same operand PAIR. That is what discharges `xs[i]`
inside `while i < xs.len` — intervals cannot, because the fact is a
relation and an interval domain forgets relations by construction.

Where it stops is `a2_stencil1d` (0.368x, and it vectorizes 0 loops
against naive C's 2). Its loop is `while i < last` with `last = src.len - 1`,
indexing `src[i-1]`, `src[i]`, `src[i+1]`, `out[i]`. Every one of those is
an **affine offset** away from the pair the guard related, and the same-pair
rule proves none of them. Four surviving guards means four early exits in
the loop body, and LoopVectorize will not take a loop with more than one.

The fix is a bounded affine extension of the same channel — decide
`i + c1 <u n + c2` from `i <s n` when the offsets are constants and the
arithmetic provably does not wrap — plus the loop-versioning client
generalized from "a limit against a constant K" to "a limit against another
loop-invariant value", which is what `out.len >= src.len` needs. Both are
`rangeopt` work with `a2_stencil1d` as the regression test.

## G7 — buffer disjointness is a checker theorem nobody spends

**Classification (a), new at s75, and it is family A's remaining loss.**

`a5_hoist_call` (0.172x) is the clean illustration. Its loop reads `src[0]`,
calls an opaque function, and reads `src[0]` again. The two loads are now
real loads — but LLVM cannot CSE them across the call, because the call
might write the buffer. wolf KNOWS it cannot: `src` is a `read` parameter
and the c04 mode theorems prove it disjoint from everything the callee can
reach. The fact exists in WIR (`FactKind::Noalias`, `Just::Theorem(ExclMut)`)
and the LLVM tier spends it — on the *parameter pointer*. It cannot spend it
on the element buffer, because that pointer is LOADED from the header, and
the noalias channel at v0 reaches LLVM only through parameter attributes and
region scopes.

s75 deliberately did not fake this: containers share one buffer region, so
no `!noalias` is claimed between two containers, because `let b = a` copies
a header pointer and two `List` values may genuinely share one buffer.
Closing it means propagating a proved-disjoint pair from container values to
their buffer pointers and giving those pointers their own alias scopes. It
is filed in `docs/backlog.md`.

The metadata-drop sentinel is the other half of this story. Family A,
`stripped / annotated − 1`: **+0.0% on all three kernels** — the two lanes
produced identical medians on every run, i.e. the difference is below the
resolution of wolf's millisecond clock. At s44 this read +1.1% and the
explanation was "with the loop body reduced to an opaque runtime call there
is nothing for `noalias` to license". The body is no longer a call, and the
reading has not improved, which points at G7 rather than at G1: the channel
still has nothing to license on the pointers that matter.

## G2 — HALF-CLOSED at s77: the byte view lands, the compare does not

**Was classification (b). The view half is closed; what remains is a
different call, and it is now measured rather than inferred.**

At s75 `bytes()` did not return a view: it called `__wolf_rt_str_bytes`,
which **materialized a whole `List[int]` — eight bytes of heap per input
byte** — and `d2_substr_search`'s range slicing was a `__wolf_rt_str_get`
shim call per comparison. Family D was 0.015x and a byte cost 34–74 ns
against C's 0.61–0.93.

s77 makes `bytes()` a view over the receiver's own `{ptr, len}` pair (the
same two words every zero-copy subslice already was), with `ptr.off` at
stride 1 + `load.i8` + `zext` element access and the bounds check in the
caller — s75's machinery at the stride bytes actually have. `s[a..b]` and
`s.get(a..b)` stop calling the runtime too: `[mem.str.get]`'s domain is two
unsigned compares plus one guarded byte probe per endpoint, and the result
is address arithmetic.

Re-measured 2026-08-13, same host and command as the s75 run
(`--track=t1 --kernels=d1_utf8_validate,d2_substr_search,word_count
--runs=7`; `perf` and `llvm-profdata` still unavailable on this host):

| kernel | s75 | s77 | ns/byte (wolf vs naive C) | reading |
|---|---|---|---|---|
| `d1_utf8_validate` | in the 34–74 ns band | **1.125x WIN** | 0.615 vs 0.751 | the byte walk is now a compiled loop |
| `d2_substr_search` | 0.014x | **0.014x** | 45.8 vs 0.64 | the slice is free; `==` is the whole cost |
| `word_count` | 0.016x | **0.016x** | 57.1 vs 0.90 | `words()` still materializes |
| **family D** | **0.015x** | **0.062x** | | one kernel wins its thesis |

**d1 wins its thesis.** A structural UTF-8 scan over a byte view beats
naive `clang -O3`, and the same run has it beating `clang -O3
-march=native` (0.719) and `rustc -O` (0.620) too. That is the first
family-D win the suite has produced, and it comes from the same lowering
change family A got at s75. Two independent 7-run measurements put it at
1.125x and 1.179x against per-kernel floors of 8.3% and 1.0% — the effect
is real, its second digit is not.

The IR-volume ratchet moved the RIGHT way this time, which is worth a line
because s75's did not: the corpus figure is **57.8% of the naive lowering
(n=105), against 58.4% at s75**, and the kernel figure is unchanged at
87.2%. An inline domain test is more instructions than a call, but a call
also carried a stack slot, an out-parameter store and a reload, and the
slot is what dominated.

**d2's residual is `__wolf_rt_str_eq`, and the A/B says so.** With the
slice inlined, the loop body holds exactly one call left — the equality.
Replacing `hay[i..i+5] == needle` with a byte-view compare (same kernel,
same sizes, LLVM tier) drops the cost from **45.8 ns/byte to 0.91** — 50x,
i.e. ~0.70x against naive C. So the slice is not what costs; a cross-crate
call for a five-byte compare is. Classification **(b)**: the fix is to stop
handing five bytes to an opaque shim — a length guard plus an inline byte
compare for short operands, or a route to `memcmp` so LLVM can see and
specialize it. `d2_substr_search` is its regression test. Filed, not fixed
here: s77's contract is views, and `==` is not a view.

**word_count is unchanged, and the cause is now located.** `words()` still
builds a `List[str]` — one materialization pass over 72 000 word views per
call — so the kernel measures an allocation, exactly as `bytes()` did. A
lazy `words()` cannot be inlined the way `bytes()` was: `split_whitespace`
is Unicode `White_Space`, so the scan needs a real character predicate, not
a byte test. It wants the D28 iterator protocol plus a runtime classifier
entry, which is a sprint, not a patch. Until then family D's geomean is
carried by one kernel and the ledger says so.

## G5 — `List` allocates outside the region it lives in (family B)

**Classification (b), new diagnosis at s75.** Family B is 0.049x, and
`b3_churn` is 0.004x — 720 ns per request against naive C's 2.67.

The kernel is `region scratch { var buf = List[Req](); … }`: sixteen pushes
into a scratch region, then wholesale free. Except the `List` is not in the
scratch region. `wolf_rt::list` allocates headers and buffers with
`ambient_alloc`, which is the **process root region** at this tier (s40's
design note says so; `[mem.region.create.3]` is satisfied by construction,
just not usefully). So `region scratch { }` frees nothing the container
used, every request leaks into a bump arena that only grows, and the
family's entire thesis — arena bump-allocate and wholesale free versus
malloc/free discipline — is not being exercised at all.

`list_alloc` says the same thing more quietly: 36.0 ns/node against naive
C's **malloc-and-free per node** at 22.9, and expert C's hand-rolled arena
at 2.13. The region machinery is not what is slow; the `push` calls and
their ambient allocation are.

The fix is to give `List` the ambient region it is lexically in — the
runtime needs a current-region handle at the allocation site, and lowering
already knows which region that is. Until it lands, family B cannot be
scored on its own thesis, and no amount of element-access work will move it
(s75 moved it 0.031x → 0.049x, which is the container-read half and nothing
else).

## G3 — checked arithmetic blocks the vectorizer (family E)

**Classification (d) — a deliberate semantic — with an (a) tail.**
Unchanged by s75, as expected: family E touches no container.

| kernel | wolf checked | wolf wrapping | naive C | verdict |
|---|---|---|---|---|
| `e1_sum_reduce` | 0.3675 ns/op | **0.1425** | 0.1460 | checked LOSS 0.397x; **wrapping TIES clang** (2.4% apart, floor 6.3%) |
| `e2_checksum` | 0.7700 | 0.7650 | 0.7833 | **TIE** at 1.017x |
| `e3_index_arith` | 0.1150 | 0.1150 | 0.0576 | LOSS 0.501x, and the checks are NOT the cause |

**X3 tracker: +37.4% geomean across family E** (`e1` +157.9%, `e2` +0.7%,
`e3` +0.0%), against s44's +38.3% — the same number, re-measured after G1,
which is what D44 asked for ("Re-measure after G1; this clause may be
re-triggered then"). It is re-triggered at the same magnitude, and the
finding is the same one D44 already ruled on: the cost is **bimodal**, not
uniform. Latency-bound loops pay nothing; the one vectorizable reduction
pays 1.6x, because the check is what stops the vectorizer. Nothing in the
s75 measurement disturbs D44's reasoning, so D44 stands and this is a
confirmation, not a new decision.

`e3` keeps the (a) tail: checked and wrapping lanes are bit-identical in
time and neither vectorizes, while naive C's masked scaled-index loop does.
A genuine s42 canonical-loop gap; the kernel is its regression test.

## G4 — trivially-false checks survive the mid-end into LLVM IR

**Classification (a), small and cheap, still open.** The emitted IR still
carries guards against constant divisors (`icmp eq i64 4096, 0` and
`icmp eq i64 4096, -1`). LLVM folds them, so they cost no run time, but they
are pure IR volume against issue #70's budget. A small s42 peephole.

## Issue #70's two metrics, re-measured

1. **LLVM IR instruction count ≤ 50% of the naive s41 lowering
   (geomean):** **NOT MET.** 58.4% across the corpus (n=103 run entries),
   87.2% across the 13 hot kernels — against s44's 57.7% / 87.5%. The
   corpus figure moved 0.7 points the WRONG way, and the cause is s75
   itself: direct element access emits address arithmetic and a load where
   a call used to be one instruction, and D43's line brackets add two calls
   per `print` statement. Both are volume the mid-end cannot remove and
   should not want to. The ratchets in `bench/gates.json` (90% kernels /
   60% corpus) still hold; the contract target does not, and the honest
   reading is that #70's metric and #77's fix pull in opposite directions
   for one instruction per access.
2. **LLVM's share of Tier-R build wall time ≤ 50%:** **MET**, unchanged.

## D43 — `print` is line-atomic, measured

Implemented in `wolf_rt` (both tiers inherit it): lowering brackets a print
statement's segment calls with `__wolf_rt_print_begin` /
`__wolf_rt_print_end`, the segments accumulate in a thread-local line
buffer, and the stream lock is taken once for the whole line instead of once
per segment.

Measured A/B on the same host, a six-segment interpolated line, 200 000
lines, median of 5 runs: **1470 ns/line per-segment → 380 ns/line
line-atomic, a 3.9x speedup.** D43 predicted 3–4x from 1.69 µs → 0.43 µs;
this is the same effect at the same size. Tearing is gone by construction,
not by luck: a whole statement is one `write_all`.

The book's exercise 30-5 (which teaches the tearing) is retired by this
row, per D43. Depth is counted rather than assumed to be one, and every
interpolation hole is evaluated before the first byte is written, so no trap
can strand a half-buffered line.

## Kernels the contract names that are not implemented

`a3-stencil2d`, `a4-matvec`, `b1-tree-build-walk`, `b2-json-dom`,
`b4-pointer-chase`, `c1-particles`, `c3-niche-filter`, `d4-format`. At s44
these were deferred because G1 dominated every array-shaped result. G1 is
closed, so the array-shaped ones (`a3`, `a4`, `c1`, `c3`, `b4`) would now
measure their own theses and are the first thing to add. The string- and
allocation-shaped ones (`b1`, `b2`, `d4`) should wait for G2 and G5, for the
same reason the others waited for G1.

## Family F cannot be measured at all

Unchanged. The release tier refuses `Opcode::FuncAddr`, so no wolf task,
channel or proc program compiles through Tier-R. s33's expectation of M2
evidence from this suite is blocked, not pending.
