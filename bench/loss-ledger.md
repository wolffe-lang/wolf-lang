# The T1 loss ledger (s44, re-measured s75, RE-MEASURED WHOLE s79)

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

Measured on 2026-08-13, `cargo xtask bench --track=t1 --runs=7`, TWICE and
independently (runs A and B below), clang 22.1.8, rustc 1.97.1, linux
x86-64. `perf` was unavailable on this host, so instruction counts are
absent; `llvm-profdata` refused the profile (version skew against clang
22), so the PGO scrutiny lane is absent. **The host was not quiet**: other
agents were building on the same machine at load 4–6 throughout, which is
why several layout floors below are 15–48% where s75 measured 2–9%. Those
floors are upper bounds that include host noise, not layout alone; M2 is
decided on the dedicated runner (protocol §7), and this is not it. All
three facts are recorded so nobody reads them as results.

## The headline

**M2 is NOT declared.** Suite geomean vs naive `clang -O3` is **0.476x**
(run A) / **0.456x** (run B) — wolf is ~2.1x slower across the suite. Six
kernels WIN or TIE (`list_alloc` 2.53x, `c2_ecs_sweep` 1.90/1.65x,
`d1_utf8_validate` 1.49/1.14x, `aos_dot` 1.05x, `alias_daxpy` 1.02/0.98x,
`e2_checksum` 1.03/0.99x) and seven lose by more than 10%.

**Nothing in the compiler changed at s79.** The suite moved from 0.191x to
0.476x because the rig was measuring the wrong things, in three ways that
four sprints had found and none of which the rig itself could see:

| fault | what it was worth |
|---|---|
| the wolf lanes linked `target/debug/libwolf_rt.a`, i.e. an **-O0 runtime** | up to **12.8x** on a single kernel (`word_count`); 5.4x `b3_churn`, 5.0x `d2_substr_search`, 2.8x `list_alloc`, 1.0x on the kernels that never call the runtime |
| `b3_churn/ref.c`'s malloc was **deleted by clang** — the naive baseline executed no allocation | the baseline went 2.6 → 7.2 ns/op, i.e. 2.9x slower and now real. Together with the runtime fix on the wolf side (5.4x), `b3_churn` moves 0.003x → 0.043x |
| the wolf lanes were **1–6 ms under a 1 ms clock** (#84) | `alias_daxpy` and `c2_ecs_sweep` were ONE TICK; family C's headline 2.086x was a 1–3 bucket reading and is 1.36x when measured |

Per family, across all four sprints and both s79 runs:

| family | s44 | s75/s77 | **s79 A / B** | reading |
|---|---|---|---|---|
| A aliasing | 0.004x | 0.405x | **0.366 / 0.366** | LOWER than s75: the s75 number was 1–2 clock ticks |
| B arena | 0.031x | 0.054x | **0.330 / 0.330** | 6x, and none of it is the compiler: the -O0 runtime and a deleted malloc |
| C layout | 0.032x | 2.086x | **1.410 / 1.321** | still the family that wins its thesis, at a third less than the bucket-reading claimed |
| D strings | 0.012x | 0.062x | **0.316 / 0.302** | 5x, all of it the release runtime; `d1` still wins |
| E arithmetic | 0.573x | 0.587x | **0.578 / 0.527** | unchanged, as expected: no container, no runtime |
| **suite** | **0.030x** | **0.191x** | **0.476 / 0.456** | |

Scrutiny lane, ungated (report-10 amendment 5), same two runs: geomean vs
expert C **0.421 / 0.418**, vs `rustc -O` **0.522 / 0.509**, vs `clang -O3
-march=native` **0.361 / 0.342**. The gated comparison remains the most
favourable of the four, which is worth repeating every time it is quoted.
Individual results that invert: `list_alloc` beats naive C by 2.5x while
losing to **expert** C's hand-rolled arena by 4.7x; `c2_ecs_sweep` beats
`rustc -O` by 1.8x and loses to expert C's SoA by 1.8x; `alias_daxpy` is
now a TIE against naive C and a 1.7x LOSS against `-march=native`.

### The three faults, and why the rig could not see them

Worth stating plainly, because it is the finding that outlives these
numbers: **every one of the three was found by a sprint doing something
else** — s76 found the runtime and the malloc while chasing family B, s78
found the clock while chasing the sentinel. The rig had no instrument
pointed at itself. s79 adds three:

- the report names the runtime it linked (`runtime_build` record, and the
  label in every wolf lane's `config`);
- every wolf lane publishes `clock_quantum_frac` and `wall_ms`, so a
  measurement that has outgrown its clock is a number in the report
  instead of a silent widening of the bucket;
- the folded-workload guard now also runs on the **wolf** lane, which is
  the half that could flatter us and the half that was missing;
- `bench-gates` checks that a baseline still makes the calls its thesis
  needs (below).

The second fault gets the instrument it can have: a kernel declares
`baseline_calls` in the manifest and `cargo xtask bench-gates` fails when
the compiled naive binary stops referencing them (verified against the
broken `b3_churn` binary, which it flags, and the fixed one, which it
passes). That catches deleted allocations — the general case, a deleted
loop, leaves no missing symbol, so s79 also audited all 19 C sources by
hand (scaling test plus disassembly; method and results in
`bench/protocol.md` §11) and that half remains a manual procedure. The
manual audit found the one deleted allocation and one kernel —
`a5_hoist_call` — whose baseline has never executed the algorithm it
names, in **either** lane.

## The full table, four comparison lanes, floors applied

Run A (run B in brackets where the verdict differs). `ns/op` is the L0
median; `floor` is the layout-noise floor, the max over the wolf and
naive-C lanes, and **a win inside its floor is a TIE**. `tick` is one
millisecond of `time_now_ms` as a fraction of the wolf lane's measured
wall — the resolution every wolf number here is read through.

| kernel | F | wolf ns/op | naive C | vs naive | verdict | vs expert C | vs rustc -O | vs -march=native | floor | tick |
|---|---|---|---|---|---|---|---|---|---|---|
| `alias_daxpy` | A | 0.3174 | 0.3229 | 1.017 [0.979] | TIE | 0.988 | 1.113 | 0.584 | 3.2% | 0.74% |
| `a2_stencil1d` | A | 1.6128 | 0.4595 | 0.285 [0.306] | LOSS | 0.289 | 0.302 | 0.201 | 17.9% | 0.46% |
| `a5_hoist_call` | A | 1.5167 | 0.2571 | 0.169 [0.163] | LOSS | 0.168 | 0.236 | 0.094 | 28.6% | 0.51% |
| `list_alloc` | B | 11.000 | 27.796 | 2.527 [2.536] | **WIN** | 0.211 | 0.269 | 2.510 | 7.6% | 0.76% |
| `b3_churn` | B | 173.42 | 7.4815 | 0.043 [0.043] | LOSS | — | 0.447 | 0.043 | 4.6% | 0.73% |
| `aos_dot` | C | 1.1647 | 1.2219 | 1.049 [1.055] | WIN [TIE] | 0.974 | 1.070 | 1.008 | 4.2% | 0.51% |
| `c2_ecs_sweep` | C | 0.6000 | 1.1375 | 1.896 [1.653] | **WIN** | 0.559 | 1.846 | 1.821 | 6.5% | 0.98% |
| `word_count` | D | 5.0982 | 0.9765 | 0.192 [0.187] | LOSS | — | 0.333 | 0.156 | 6.7% | 0.74% |
| `d1_utf8_validate` | D | 0.5788 | 0.8605 | 1.487 [1.136] | WIN [TIE] | — | 1.177 | 1.386 | 8.6% | 1.27% |
| `d2_substr_search` | D | 5.8931 | 0.6523 | 0.111 [0.130] | LOSS | — | 0.111 | 0.112 | 25.0% | 0.91% |
| `e1_sum_reduce` | E | 0.4219 | 0.1541 | 0.365 [0.347] | LOSS | — | 0.448 | 0.170 | 3.8% | 0.74% |
| `e2_checksum` | E | 0.8524 | 0.8759 | 1.028 [0.986] | TIE | — | 1.102 | 1.003 | 3.4% | 0.56% |
| `e3_index_arith` | E | 0.1290 | 0.0664 | 0.515 [0.426] | LOSS | — | folded | 0.200 | 7.3% | 0.56% |

| family | vs naive C | vs expert C | vs rustc -O | vs -march=native |
|---|---|---|---|---|
| A aliasing | 0.366 / 0.366 | 0.364 / 0.360 | 0.430 / 0.441 | 0.222 / 0.219 |
| B arena | 0.330 / 0.330 | 0.211 / 0.214 | 0.347 / 0.365 | 0.327 / 0.333 |
| C layout | 1.410 / 1.321 | 0.738 / 0.730 | 1.405 / 1.331 | 1.355 / 1.302 |
| D strings | 0.316 / 0.302 | — | 0.352 / 0.321 | 0.289 / 0.265 |
| E arithmetic | 0.578 / 0.527 | — | 0.702 / 0.670 | 0.324 / 0.286 |
| **SUITE** | **0.476 / 0.456** | **0.421 / 0.418** | **0.522 / 0.509** | **0.361 / 0.342** |

Notes a reader should not have to dig for. `e3_index_arith`'s Rust lane
folds its workload (9.7e-6 ns/op) and contributes no comparison, as it has
since s44 — the guard catches it, so the rustc geomeans are over 12
kernels, not 13. The expert-C column exists for six kernels only.
`a5_hoist_call`'s table row above predates its #97 redesign (2026-08-20)
and measured a folded loop, not the kernel's thesis; the redesigned
kernel's numbers live in G7's closing subsection. `alias_daxpy` losing 1.7x to `-march=native`
while tying naive C is the honest shape of family A on this host: the
gated lane is the friendliest of the four, every time.

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

### s79 corrections to the two family-A/C claims above

**`c2_ecs_sweep`'s 2.9x was a one-tick reading.** At s75's sizes the wolf
lane ran **1 ms** — one tick of `time_now_ms`, so the measurement had
100% quantisation and 2.907x carried no significant digits. Re-measured
over 240–250 ms: **1.896x / 1.653x**, and `aos_dot` **1.049x / 1.055x**
against s75's 1.497x. Family C still wins its thesis, and it wins it by
about a third less than the number that has been quoted since s75.

**Family A's naive-C lane is not naive, and the audit says so.** In
`alias_daxpy` and `a2_stencil1d` the hot function is `static` and both
buffers are file-scope arrays, so clang proves disjointness by inlining
and vectorizes with no runtime alias check (checked in the disassembly:
`movapd`/`mulpd`/`addpd` with no guard). The gated lane therefore already
has the fact the family says naive C lacks — `a2`'s naive and expert
binaries land 0.25% apart. This is a baseline STRONGER than its label,
which is the conservative direction, so it stays as it is; but it means
family A's 0.366x is "wolf vs clang-with-the-fact", not "wolf vs a
compiler that had to assume overlap", and the thesis is only tested on a
shape where clang cannot see both buffers at once. Nobody had checked.

## G6 — the bounds checks that survive are the shape of the next gap

**Classification (a), new at s75.**

Bounds checks stay (X3's sibling: eliminating a check because it is
provable is optimization; eliminating it because it is inconvenient is
not). What s75 added is a **relational** π channel in `rangeopt`: the
comparison that held on the edge into a single-entry block decides a later
comparison over the same operand PAIR. That is what discharges `xs[i]`
inside `while i < xs.len` — intervals cannot, because the fact is a
relation and an interval domain forgets relations by construction.

Where it stops is `a2_stencil1d` (**0.285 / 0.306x re-measured at s79**,
against s75's 0.368x — the s75 reading was two clock ticks wide, so the
"regression" is the earlier number being unreadable, not a change in the
compiler; it vectorizes 0 loops
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

**Landed 2026-08-21 (#98), half of it.** The affine half (s78) had already
discharged the three `src` guards; what remained was one bounds guard on
`out[i]` and two element-add overflow checks. The versioning client now
takes bounds guards as candidates at all (a br to a trap arm made every
guarded loop "multi-exit" and versioning never fired — the trap arm is the
check's own failure arm, not an exit), asks for the missing relation as
REAL guards at the loop's door (`0 <= out.len`, `last <= out.len`, a
paramless chain so the memory token is consumed once per path), and the
relational channel chains the two base pairs through one intermediate —
the difference-bound step, deliberately bounded at a single hop. The fast
copy's bounds check folds; the slow copy keeps everything. Measured on
this host loaded, ratios only: **0.299x → 0.331x** vs naive C; IR volume
266 → 309 instructions, the one kernel that moved (`kernel_ceiling_history`
in gates.json carries the decomposition). The two overflow checks stay —
an opaque `List[int]` element is unbounded intra-procedurally, exactly
D44's second-addendum caveat — and they are what still blocks
vectorization (rustc sits at C parity here because its release adds wrap).
Closing them needs element-range facts from the container layer or
interprocedural value-range summaries; neither exists, and a pre-scan
guard would change the guard's complexity class. e3/d2 were checked for
free movement: none (0.483x, 0.827x — unchanged within noise).

## G7 — buffer disjointness is a checker theorem nobody spends

> **SUPERSEDED 2026-08-21 (#115):** the closing numbers below predate
> the quiet-host M2 retry, where the redesigned kernel's C lanes ran
> ~9,000x slower than measured here (naive 5720 ns/op, expert 3246 —
> wolf 1.433 and rustc 0.336 both reproduce). The 4040x "WIN" it now
> renders is an artifact; quote suite numbers ex-a5 until #115 lands a
> third design. The one trustworthy a5 signal: wolf vs rustc 0.234x.


**Classification (a), new at s75, and it is family A's remaining loss.**

### s79 correction: `a5_hoist_call` was never measuring this

**The illustration below is not what the kernel compiles to, and had not
been for any of its five measurements.** In both lanes the "opaque call"
is solved and deleted: clang -O3 folds the recursion's closed form, proves
the callee touches no memory, hoists both loads and vectorizes; wolf's
release tier inlines `probe` and `opaque`, folds the same recursion and
hoists the same load (checked in the disassembly of both binaries —
neither timed region contains a call). The reported 0.172x, and s79's
0.169 / 0.163x, are one folded arithmetic loop against another: clang's
vectorized, wolf's a scalar chain of checked adds with a `jo` per
operation. It is a real number about a real difference; it is not about
CSE across calls.

It cannot be fixed on one side. Three same-TU repairs were tried and
failed (a run-time recursion depth, a `volatile` function pointer, a
published global alias); moving the callee into its own translation unit
does restore the call and the reload, measured at 0.219 → 1.31 ns/op —
**and it was backed out**, because wolf has no counterpart: whole-program
single module, no `noinline`, and Tier-R refuses `func.addr`, so there is
no indirect call either. Fixing only the C lane would have turned this
loss into a win manufactured by unequal work.

What would actually test the thesis is a kernel where the callee
**writes** memory the caller must assume it might alias — then wolf's
`read` mode is the only thing that licenses the hoist, and G7 is exactly
why wolf would fail it today (containers share one buffer region, so no
`!noalias` is claimed between two containers). That kernel exists now — the
closing subsection below.

### #97 closed (2026-08-20): the callee writes memory, and the kernel measures its thesis

The redesign is the one #97 prescribed. `bump` stores through a second
container (`mut scratch`) between the two `src[0]` loads; naive C's
pointers are laundered through volatile globals (the b3 escape idiom),
so the store may alias `src` and clang must reload — and store — every
iteration; expert C asserts `restrict` over the same laundered
pointers; rustc's `&`/`&mut` and wolf's `read`/`mut` prove the
disjointness from the signature. All four lanes agree on the sink, so
every lane executes the same arithmetic; only what the compiler may
prove differs. The pre-redesign gate floor `c_naive_min: 1` flips to
0 deliberately: a naive lane that vectorizes this kernel again has
optimized the aliasing hazard away and stopped measuring the thesis.

Measured at the redesign (best-of-5 per lane, i7-10870H, **loaded
host — sibling build lanes running; relative mechanism claims only,
no geomean update from these numbers**):

| lane | ns/op | vs naive |
|---|---|---|
| naive C | 0.616 | 1.000 |
| expert C (`restrict`) | 0.354 | 1.740 |
| rustc -O | 0.368 | 1.673 |
| wolf --release | 1.450 | 0.425 |

Three facts these numbers pin. (1) **The aliasing tax is real and now
measured**: naive C loses 1.74x to expert C purely for lack of
provenance, and the disassembly shows the mechanism (load src → store
scratch → RELOAD src, per iteration). (2) **rustc lands at expert
parity from types alone** — which is precisely the property wolf
claims; the kernel now rewards it. (3) **wolf gets the CSE but not
the rest**: the disassembly shows ONE `src[0]` element load per
iteration, reordered above the scratch store and used twice — the
cross-call CSE this section predicted wolf would FAIL is actually
delivered (the noalias channel reached the element loads) — but the
load is not hoisted out of the loop, the scratch value is not
register-promoted, and the loop pays two container-header loads, two
bounds checks and four `jo` traps per iteration. wolf's 0.42x is
family A's honest residue (this G7 gap plus checked-arith), where the
old 0.16x was an artifact.

The s79 audit blocks in `ref.c`/`kernel.lu` are condensed into the
new headers; the full history stays here.

The original s75 diagnosis, kept because the FACT-side analysis is still
correct — it is the kernel that was wrong, not the gap:

`a5_hoist_call` (0.172x) was meant to be the clean illustration. Its loop reads `src[0]`,
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

### The metadata sentinel, now that it can be read (#84)

At s44 the sentinel read +1.1%; at s75 and s78 it read **+0.0% on all
three family-A kernels**, and the ledger explained that as "the channel
has nothing to license". That explanation was unfalsifiable: the kernels
ran 1–6 ms against a 1 ms clock, so a bucket was 17–100% wide and both
lanes landed in the same one. #84 is closed by sizing family A to 200–360
ms; one tick is now **0.50–0.77%** of the measurement, and the harness
publishes that number (`clock_quantum_frac`) beside every reading so this
cannot silently come back.

What the readable sentinel says, over two independent 7-run measurements:

| kernel | run A | run B | tick |
|---|---|---|---|
| `alias_daxpy` | +3.85% | +0.00% | 0.69–0.77% |
| `a2_stencil1d` | −8.72% | −1.18% | 0.50–0.60% |
| `a5_hoist_call` | +7.14% | +5.77% | 0.55–0.64% |

The bucket is no longer the limit; the **host** is. Run-to-run spread is
5–9 points, which is larger than any bonus the channel plausibly carries,
and `a2_stencil1d` reads NEGATIVE in both runs (the stripped build
measuring faster). Two honest readings of that: on a machine at load 4–6
this lane cannot resolve the metadata bonus at all, and it needs the
dedicated runner before anything is claimed from it; and the s44/s75
conclusion — that the channel has little to license while the pointers
that matter are loaded from a header (G7) — is still consistent with the
data but is no longer *evidenced* by it. The sentinel's job is to price
channel decay per commit, and it can only start doing that job on a quiet
machine. That is now the only thing standing between it and a number.

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

### s79 re-measurement of family D: the residual was half runtime build

Every number in the s77 table above was taken against an **-O0
`libwolf_rt.a`**, and family D is the family that calls the runtime most.
Same kernels, same host, release runtime, sizes recalibrated:

| kernel | s77 (debug rt) | **s79 A / B** | ns/byte wolf vs naive C (A) |
|---|---|---|---|
| `d1_utf8_validate` | 1.125x | **1.487 / 1.136** | 0.579 vs 0.861 |
| `d2_substr_search` | 0.014x | **0.111 / 0.130** | 5.89 vs 0.65 |
| `word_count` | 0.016x | **0.192 / 0.187** | 5.10 vs 0.98 |
| **family D** | **0.062x** | **0.316 / 0.302** | |

The A/B on the runtime alone, at fixed sizes: **`word_count` 12.8x,
`d2_substr_search` 5.0x, `d1_utf8_validate` 1.09x**. That ordering is the
diagnosis restated as a measurement — `d1`'s loop is compiled wolf code
and barely notices the runtime's build, while `word_count` and `d2` spend
their time inside `wolf_rt` and were being scored on `-O0` versions of
`words()` and `str_eq`. The two diagnoses stand (`words()` materializes,
`==` is an opaque cross-crate call); their *prices* were overstated by
5–13x and are only now honest numbers. `d1` remains the family's one win,
1.49x and 1.14x across two runs against floors of 8.6% and 48% — the
second run's floor is host noise, and the honest summary is that `d1`
wins by somewhere between "clearly" and "not measurably" on a loaded
machine.

### s84 — G2 CLOSED: `words()` stops materializing, and family D reaches 1.000x

s81 inlined `==` and s84 made `words()`/`lines()`/`split()` lazy
(`[mem.str.view]`): in a `for` head they walk the receiver's own bytes
and yield subslices of it, so there is no `List[str]`, no push per
element, and — with the `White_Space` predicate inline — no call in the
loop at all. Same host and command as the s77/s79 runs
(`--track=t1 --kernels=d1_utf8_validate,d2_substr_search,word_count
--runs=7`, release runtime; `perf` and `llvm-profdata` still
unavailable), single run:

| kernel | s77 | s79 | **s84** | ns/byte wolf vs naive C |
|---|---|---|---|---|
| `d1_utf8_validate` | 1.125x | 1.487 / 1.136 | **1.227x WIN** | 0.674 vs 0.827 |
| `d2_substr_search` | 0.014x | 0.111 / 0.130 | **0.871x LOSS** | 0.804 vs 0.700 |
| `word_count` | 0.016x | 0.192 / 0.187 | **0.935x LOSS** | 1.162 vs 1.087 |
| **family D** | **0.062x** | **0.316 / 0.302** | **1.000x** | |

**`word_count` is the number this sprint existed for: 0.192x → 0.935x**,
5.47 ns/byte → 1.162, an A/B on the same host in the same session (the
lazy lowering switched off and back on, everything else fixed). It now
beats `rustc -O`'s `split_whitespace()` lane by **1.50x** and sits
inside 7% of a hand-written two-state byte scan, against a 6.45% layout
floor — which is to say the remaining gap is at the edge of what this
rig can resolve. What it does NOT beat is `clang -O3 -march=native`
(0.649x): the C scan vectorizes and the wolf loop does not, which is
the next question rather than this one.

`d2_substr_search`'s move (0.111 → 0.871) is s81's equality inlining
finally showing up in a family-D measurement; the ledger had not been
re-run since. G2's two named residuals — "the compare is a call" and
"`words()` materializes" — are both gone, and family D is the first
family to reach the M2 primary bar exactly.

The D rows in the full table above are the s79 run and are now stale;
they are left as-is rather than half-updated, because a three-kernel
Run A cannot be spliced into a thirteen-kernel A/B without lying about
which run each number came from. The next full-suite sweep supersedes
them.

The IR-volume ratchets both moved the right way despite the inline
predicate being more instructions than a call: **kernel suite 87.7% of
naive (ratchet 90.0%), corpus 57.8% (ratchet 60.0%)** — the
materializing path carried a runtime call, a header, a growable buffer
and a push per element, and that is more IR than a decision tree.

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

### s76 update — the placement is fixed; the SCORE barely moves, and here is why

Containers now allocate in the ambient region at their site, header and
buffer both, and a `region` block reclaims every byte they used (the
deterministic witness is `crates/wolf_rt/tests/region_containers.rs`, on
the runtime's own live-region-bytes ledger). Family B's thesis is finally
the thing being measured. The number, though:

| | family B geomean | `b3_churn` vs naive C |
|---|---|---|
| s75 (leaking) | 0.049x | 0.004x |
| s76 (placed) | 0.054x | 0.003x |

Two measurement facts have to be on the table before that table means
anything.

1. **The rig links an UNOPTIMIZED runtime.** `cargo xtask bench` builds the
   wolf lane with `target/debug/wolf`, which links `target/debug/
   libwolf_rt.a` — `wolf_rt` compiled `-O0`. For a kernel whose whole body
   is runtime calls, that is most of the absolute number. A/B under the rig:
   965 → 850 ns/op, **12% faster**. The same A/B against a release
   `libwolf_rt.a`: **310 → 180 ns/op, 1.7x faster**. Placement is a win in
   both, and the ratio the suite prints understates it. Fixing the rig to
   link the release runtime is its own (small) job, and until it does,
   family B's absolute ns/op are not the language's numbers.
2. **`c_naive` is 2.9 ns/op because clang deleted the allocation.** `ref.c`
   mallocs a 16-entry buffer that provably does not escape `handle()`, so
   LLVM elides the malloc/free pair outright. The "naive malloc/free per
   request" baseline this kernel was written to beat is not being executed.
   A ratio against it is a ratio against nothing, which is why the kernel
   reads 0.003x whether the leak is there or not. The kernel needs a sink
   that defeats the elision (`ref.c` should escape the buffer) before family
   B has a scoreable opponent.

En route, s76 found and fixed a real cost the leak had been hiding: a
region's first chunk was 16 KiB of `calloc`, which a scratch region using
430 bytes paid in full, every request. Once the container actually landed in
the region that made `b3_churn` **3x slower** than the leaking version
(310 → 920 ns/op with a release runtime). Region chunks now follow a
geometric ladder from 1 KiB to a 1 MiB cap, which is where the 1.7x comes
from. `list_alloc` moved 0.893x → 0.946x across runs on a 3.7–11.5% layout
floor, i.e. inside the noise; it needs a re-run with more samples.

**Still open for family B:** the rig's `-O0` runtime, `ref.c`'s elided
allocation, and `push` still being an opaque call per element (the
allocation is now cheap; the CALL is what is left). G5's placement half is
closed; its throughput half is a measurement-rig problem first and an
inlining problem second.

### s79 — both measurement problems are fixed, and s76's diagnosis was right

The two items s76 filed are closed. The rig links the release runtime
(worth **5.4x on `b3_churn` and 2.8x on `list_alloc`** at fixed sizes),
and `b3_churn/ref.c` escapes its buffer to a volatile sink so clang can no
longer delete the allocation — the naive baseline goes 2.6 → 7.2 ns/op
with an unchanged sink and two `malloc`/`free` call sites back in the
disassembly. Family B, re-measured twice:

| | family B | `list_alloc` | `b3_churn` |
|---|---|---|---|
| s75 (leaking, -O0 rt, elided malloc) | 0.049x | 0.893–0.946x | 0.004x |
| s76 (placed, same rig faults) | 0.054x | 0.946x | 0.003x |
| **s79 A / B** | **0.330 / 0.330** | **2.527 / 2.536x WIN** | **0.043 / 0.043x** |

**`list_alloc` is now the suite's largest win: 11.0 ns/node against naive
C's 27.8 for malloc-and-free per node.** That is family B's thesis —
region bump-allocate and wholesale free versus malloc/free discipline —
paying for the first time, and it only became visible once the runtime
doing the bump was compiled with optimizations. Against **expert** C's
hand-rolled arena it is still a 4.7x loss (0.211 / 0.214), which is the
honest ceiling on the claim: wolf's regions beat the allocation discipline
most C has, and do not yet beat the one an expert writes.

**`b3_churn` is still 0.043x, and that is now a real number about a real
gap.** 173 ns per request against a naive baseline that genuinely mallocs
and frees, at 7.5. The region create/free pair is not the cost — 16
`push` calls into a fresh 1 KiB chunk are, plus a region setup per
request. This is the inlining half of G5 that s76 named, with the
measurement fog removed: no leak, no elided baseline, no -O0 runtime, and
a 40x gap that belongs to `push` being an opaque call per element.

### 2026-08-20 — the 40x apportioned: an instruction profile, a control, and one pure waste

Measured after the M2 quiet-host run put `b3_churn` at 0.046x (#89).
Conditions: i7-10870H, LOADED host (sibling lanes running), so every
number here is a ratio or an instruction count, not a wall-clock claim.
Instrument: callgrind over 2 000 requests, plus a no-region control
variant (the same kernel with `region scratch { }` replaced by a bare
block, built from scratch — not committed; the shipped kernel is
unchanged).

**The instruction split, per request (~1 805 Ir total vs C's ~150):**

| where | share | what it is |
|---|---|---|
| `__wolf_rt_list_push` self | 20.5% | 16 out-of-line calls, growth branch each |
| kernel body (`_Wmain`) | 18.4% | loop, reads, checked adds |
| `__memcpy_avx` | 11.0% | 16 per-element stack-slot copies + 2 growth copies |
| malloc + free + calloc | 10.9% | `Box<Region>` + chunk vec, 2 allocs + 2 frees per request |
| `__memset_avx2` | 6.1% | zeroing a 1 KiB chunk for a 264-byte need |
| `__wolf_rt_region_alloc` + growth | ~5% | bump path + `RawVec` machinery |

**The control confirms s79's call**: dropping the region pair makes the
kernel 1.65x SLOWER (145 → 240 ns/op same host, ratio stable across
runs), because ambient chunks ladder toward `CHUNK_MAX` = 1 MiB and
every new chunk is `vec![0u8; cap]` — zeroed in full. The region pair
is not the cost; it is the cheaper of the two paths through the same
heap-backed chunk machinery.

**The kernel's own comment is wrong about the runtime.** "The region
create/free pair is a pointer bump and a pointer reset" describes the
design (reports/01 fact 5), not `native.rs`: `region_new` heap-allocates
a `Box<Region>`, the first `region_alloc` heap-allocates AND ZEROES a
1 KiB chunk (`vec![0u8; cap]` — the zeroing is an artifact of safe-Rust
construction; a bump allocator never needs zeroed memory), and
`region_free` walks and frees every chunk. Per request that is two
mallocs, two frees, one memset and two atomics before any element
lands, against naive C's single tcache-hit `malloc(256)`/`free`.

**Fix shapes, ranked by the profile — all out of this diagnosis's
scope, none of them kernel work:**
1. *Push stops being a call* (the G5 inlining half, now 31.5% of Ir
   with its copies): an inline cap-check fast path that calls out only
   on growth — lowering work, the exact shape G1 took for reads.
2. *Chunks are pooled, not malloc'd* (~17% of Ir, more of the cycles):
   a freelist of retired chunks in `wolf_rt`, no `vec![0u8]` zeroing,
   lazy `Box`. This is what would make the kernel's comment TRUE.
3. The body's checked adds are family E's item, not B's.

The kernel measures its thesis correctly today; it is the runtime that
does not yet do what the thesis says. No gates move on this entry — it
is a diagnosis, not a fix.

### #113 — both fix shapes landed, and the split says they hit

Fixes 1 and 2 above are in (the inline push fast path in lowering; the
pooled, non-zeroing `MaybeUninit` chunks in `wolf_rt`, with the s76
determinism aid kept in DEBUG builds by name — see `native::new_chunk`
— and the same de-zeroing applied to the root arena the control had
convicted). Conditions: same i7-10870H, LOADED host (load 1.5–2.7,
sibling session live), best-of-runs, ratios and instruction counts
only.

| | before | after | |
|---|---|---|---|
| `b3_churn` wolf | 141.8 ns/op | **70.9** | 2.0x; **0.047x → 0.094x** vs naive C, now ahead of rustc's 74.8 |
| `list_alloc` wolf | 9.0 ns/node | **3.83** | 2.35x; the family-thesis WIN goes **2.54x → 6.55x**, closing on rustc 2.54 / expert 2.13 |

Callgrind, 2 000 requests, same protocol as the diagnosis:
`list_push` self 20.5% → **6.8%** — 14 of 16 pushes never leave
`_Wmain`; the two growth pushes still call out, as designed.
`__memcpy` 11.0% → 3.9% (growth copies and slow-path spills only).
**The malloc+free+calloc rows and the `__memset` row are gone from
the table**; in their place the pools cost ~3.1% of Ir in TLS
traffic. `_Wmain` is now 37.6% of the profile (was 18.4%) — the
kernel body finally dominates its own benchmark.

What is left of the 0.094x: the body's checked adds (family E's
item, not B's), the bump path and retire walk (`region_alloc` 5.9%,
`region_free` 3.7%), and the two growth calls. One scrutiny flag,
recorded not chased: the ungated PGO lane reads b3 at −10.1% against
an 8.5% floor on this loaded host — re-read it on a quiet host
before believing it.

En route the fast path caught a latent layout fact: lowering's block
CREATION order was never execution order (a `region` block pre-creates
its exit block, so exit code lives in an early-id block), and only the
printer's RPO re-sort hid it. `Builder::finish` now hands out
canonical reachable-RPO layout, so the mid-end seam litmus compares
canonical against canonical — one order everywhere.

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

### s79 re-measurement — the same finding, at last with resolution

Family E never touches a container or the runtime, so it is the family the
three faults could not distort, and it is the one that moved least:
**0.578 / 0.527** against s75's 0.587x. The X3 tracker is
**+37.1% / +43.3% geomean** against s44's +38.3% and s75's +37.4%.

| kernel | wolf checked (A) | naive C (A) | vs naive A / B | X3 A / B |
|---|---|---|---|---|
| `e1_sum_reduce` | 0.4219 ns/op | 0.1541 | 0.365 / 0.347 | **+164.7% / +170.6%** |
| `e2_checksum` | 0.8524 | 0.8759 | 1.028 / 0.986 TIE | −2.2% / −1.2% |
| `e3_index_arith` | 0.1290 | 0.0664 | 0.515 / 0.426 | −0.6% / +10.1% |

Bimodal, exactly as D44 ruled: one vectorizable reduction pays 1.65–1.71x
for the check, and the two latency-bound loops pay nothing measurable
(both readings sit inside their floors, and `e3`'s +10.1% in run B against
−0.6% in run A is the host, not the semantics). This is the third
consecutive re-measurement to re-trigger the D2/X3 revisit clause at the
same magnitude and reach the same conclusion, and the first one where the
family-E numbers are known not to be clock artefacts. D44 stands.

### s85 re-measurement — the optimizer gap was an optimizer gap

D44's ruling was a claim about the compiler, not about the language:
the cost is "a mid-end problem (range analysis plus loop versioning
already in s42's pass list), not a semantics problem". s85 tested it
by building the two mechanisms D44 named, and the claim survives.

Both lanes re-measured back to back on one host, 7 runs, the only
difference between them being the compiler:

| kernel | checked | wrapping | X3 delta | pre-s85 X3 | vs naive C |
|---|---|---|---|---|---|
| `e1_sum_reduce` | 0.1500 ns/op | 0.1500 | **+0.0%** | +177.0% | 0.366x → **0.971x** |
| `e2_checksum` | 0.8214 | 0.8571 | −4.2% | −0.3% | 1.007x → 0.996x |
| `e3_index_arith` | 0.1250 | 0.1290 | −3.1% | −1.9% | 0.500x → 0.517x |
| **family E geomean** | | | **−2.4%** | **+39.4%** | |

The bimodality is gone because its one mode is gone. `e1`'s
accumulator is now bounded by the trip count times the masked
increment, so its check folds and LLVM vectorizes the reduction; the
two latency-bound loops are where they always were. A −2.4% geomean
means checked and wrapping compile to the same loop and the residual
is host noise, so the honest statement is **X3's measured cost on
family E is zero, inside the floor** — comfortably under the ~2–3%
D44 set as its revisit threshold. **The D2/X3 revisit clause is not
re-triggered.**

Two cautions on the number, so it is not read as more than it is.
First, family E is three kernels: this is not a claim that checked
arithmetic is free everywhere, only that it is free on everything this
suite can currently see. Second, the win rides on ranges that close:
where nothing bounds an element (`List[int]`) or an entry value
(`e2`'s opaque seed), no proof is available, the check stays, and the
program pays for it. `corpus/kernels/hot_sum_reduce.lu` carries both
halves — the `u8` reduction that discharges and the `int` reduction
that does not — so the boundary is a regression test rather than a
recollection.

`e3` keeps its loss at 0.517x and it is still not an X3 loss: the
checked and wrapping lanes agree to within 3%, so the gap is the (a)
tail already recorded above, not the semantics.

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
2. **LLVM's share of Tier-R build wall time ≤ 50%:** **MET, and flattered
   by a factor of ~1.8.** The harness reports **19.9%** (geomean, n=13,
   against s75's figure) — but `t_total` is a build by `target/debug/wolf`,
   a **debug-built front end**, and a slow front end makes LLVM's share
   look small. This is fault 1 wearing a different hat, and s79 measured
   it rather than guessing: same kernels, same IR, best of three, driver
   built `--release`:

   | kernel | share (debug driver) | share (release driver) |
   |---|---|---|
   | `alias_daxpy` | 0.292 | **0.553** |
   | `list_alloc` | 0.213 | 0.315 |
   | `e1_sum_reduce` | 0.191 | 0.319 |
   | `d1_utf8_validate` | 0.191 | 0.328 |
   | `c2_ecs_sweep` | 0.200 | 0.333 |

   Geomean over those five: **0.20 debug / 0.36 release**. The budget still
   holds — 36% ≤ 50% — but one kernel is already over it, and the number
   the nightly publishes is a lower bound, not the figure the posture is
   claimed on. The harness now says so in the record's `config` and on
   stderr. Making the rig build a release driver for this metric is a
   CI-time decision (several minutes per nightly) and is left as the
   follow-up rather than taken unilaterally here.

### s79 re-measurement of metric 1

**Still NOT MET, and essentially unmoved: 58.2% across the corpus (n=107,
10 files refused by Tier-R), 86.8% across the 13 kernels**, against s77's
57.8% / 87.2% and s44's 57.7% / 87.5%. The corpus sample grew by four
files (the s78/s79 corpus additions), which accounts for the 0.4-point
drift on its own. Nothing in this sprint touched lowering, and nothing in
the numbers suggests otherwise. The `bench/gates.json` ratchets (90%
kernels / 60% corpus) still hold with room.

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

## s45 — what PGO is worth on this suite

Three full `--runs=7` measurements on 2026-08-14, same host, clang 22.1.8,
release runtime. `perf` and `llvm-profdata` unavailable as before. **Train
and measure are different inputs and are named separately:** the lane
trains at `ops_wolf/8` sweeps and is measured at the full `ops_wolf`,
mirroring the clang PGO lane. That separation is weak — the suite's only
input knob is the sweep count, so both inputs run the same code paths at
different trip counts, which is the direction that FLATTERS PGO. Every
number below is an upper bound, and it is read through the kernel's layout
floor (protocol §4: a win inside the floor is a TIE).

**Run 1 was a different compiler**, and its story is the finding. It ran
with the profile-driven inline bonus at 64 WIR instructions. Result:
`word_count`'s hot 155-instruction `count_words` inlined into `main`, both
of the program's records went stale (2/2 matched before the pass, 0/2
after), the `!prof` channel fell silent on exactly the code the profile was
collected for, and the kernel measured **−28.67%**. `alias_daxpy` read
−5.92% against a 2.14% floor. Two losses outside their floors, no
reproducible win. That is a PGO-caused regression, and the contract's
acceptance says fix or neutralize it: it is neutralized. The profile's
mid-end consumers (inline budget, cluster affinity) ship at **neutral
defaults** in `Thresholds`, implemented and tested but off, because a
record is keyed by the post-mid-end body hash and **any mid-end knob that
acts on hotness destroys the record of every body it changes**. What ships
live is the channel that does not move the bodies: measured `!prof` branch
weights through s41.

Runs 2 and 3 measure that compiler. `pgo_speedup` = median(L0) default /
median(L0) profiled; `> 1` is a PGO win.

| kernel | run 2 | run 3 | verdict |
|---|---|---|---|
| `alias_daxpy` | +0.77% (floor 4.65%) | −6.49% (floor 15.28%) | TIE, TIE |
| `a2_stencil1d` | +2.67% (floor 2.16%) | −3.35% (floor 7.88%) | WIN then TIE — **does not replicate** |
| `a5_hoist_call` | +5.44% (floor 7.64%) | +3.25% (floor 9.40%) | TIE, TIE |
| `list_alloc` | −2.26% (floor 6.82%) | +3.45% (floor 4.17%) | TIE, TIE |
| `b3_churn` | +1.34% (floor 7.48%) | +3.14% (floor 7.60%) | TIE, TIE |
| `aos_dot` | +0.00% (floor 15.38%) | −0.50% (floor 3.84%) | TIE, TIE |
| `c2_ecs_sweep` | −0.84% (floor 9.86%) | +2.59% (floor 10.71%) | TIE, TIE |
| **`word_count`** | **+57.14% (floor 6.55%)** | **+57.14% (floor 3.03%)** | **WIN, replicated exactly** |
| `d1_utf8_validate` | +31.82% (floor 5.66%) | +6.02% (floor 17.47%) | WIN then TIE — **does not replicate** |
| `d2_substr_search` | +0.00% (floor 6.25%) | −10.61% (floor 46.83%) | TIE, TIE |
| `e1_sum_reduce` | +0.00% (floor 3.60%) | +5.32% (floor 79.52%) | TIE, TIE |
| `e2_checksum` | +0.93% (floor 4.13%) | −0.51% (floor 3.58%) | TIE, TIE |
| `e3_index_arith` | −1.03% (floor 7.63%) | +6.25% (floor 8.18%) | TIE, TIE |

**The finding: one reproducible win, and it is in family D.** `word_count`
gives +57.14% in both runs — the same figure twice, on a kernel whose
reading is quantised by a 4.76% clock tick, with the two runs' floors
differing by 2x. Everything else is inside its floor. Two kernels
(`a2_stencil1d`, `d1_utf8_validate`) cleared their floor in one run and not
the other, which is what a floor is for: **a single-run win that does not
replicate is not a win.** No kernel regresses outside its floor in either
run, so the contract's ">2% regression is a heuristic bug" clause is
satisfied at these settings.

Read the floors themselves with suspicion: they range 2–47% across runs of
the same kernel on this host, so they are measuring host noise as well as
layout. M2 is decided on the dedicated runner and this is not it.

### Open, for the closeout

- **The keying tension is real and unresolved.** Post-mid-end content-hash
  keying (D8/s43, locked, and right for staleness) makes profile-guided
  mid-end transformation self-invalidating. Two exits exist and neither is
  a heuristic tweak: per-EDGE counters (so weights survive a body change
  the way block counts do not), or a second, pre-mid-end key carried
  alongside the D8 one. Both are container-visible and want a version bump.
- **Block counts are not edge counts.** The emitter only weights branches
  whose successors have a single predecessor, where the two coincide; a
  join reached from several places carries the sum and is left unweighted.
  That silently drops the channel on the most interesting shapes.
- **The train/measure separation wants a second input per kernel**, not a
  smaller sweep count of the same one.

## Kernels the contract names that are not implemented

`a3-stencil2d`, `a4-matvec`, `b1-tree-build-walk`, `b2-json-dom`,
`b4-pointer-chase`, `c1-particles`, `c3-niche-filter`, `d4-format`. At s44
these were deferred because G1 dominated every array-shaped result. G1 is
closed, so the array-shaped ones (`a3`, `a4`, `c1`, `c3`, `b4`) would now
measure their own theses and are the first thing to add. The string- and
allocation-shaped ones (`b1`, `b2`, `d4`) should wait for G2 and G5, for the
same reason the others waited for G1.

## Family F — UNBLOCKED at s86 (was: cannot be measured at all)

Through s85 this section read: "the release tier refuses
`Opcode::FuncAddr`, so no wolf task, channel or proc program compiles
through Tier-R. s33's expectation of M2 evidence from this suite is
blocked, not pending."

s86 lowered `func.addr` (a module function's address is a link-time
constant, exactly like `data.addr`), and the block is gone: every conc
corpus file now compiles and runs through Tier-R at the debug tier's
verdict and stdout. Family F kernels are writable. They are still not
WRITTEN — that is the next sprint's work, not this one's — but the
reason they were absent no longer exists.

**First measurement, and it is not flattering.** With the release tier
able to compile them, the 14 conc corpus programs entered the
IR-volume gate's population for the first time, and they measure
**geomean 0.900** of naive lowering against **0.582** for the other
117 run-phase entries (measured 2026-08-16 with the gate's own
`--emit=llvm-ir` on/off procedure). The mid-end barely shrinks a
scheduler-bound body, which is unsurprising — those bodies are mostly
opaque `__wolf_rt_*` calls with nothing to fold — but two entries are
worth naming:

| file | ratio | note |
|------|-------|------|
| `conc/proc_link.lu` | **1.385** | the mid-end makes it BIGGER |
| `conc/proc_kill_defers.lu` | 1.008 | net-neutral |
| the other 12 | 0.815–1.000 | no measurable win |

`proc_link` growing by 38% is a real finding and a new open item: a
pass (inlining is the first suspect, since the synthesized task bodies
are small and single-caller) is expanding a body the mid-end cannot
then re-shrink. Classification **(a)** — the facts exist, the pass
policy is wrong for this shape. It costs nothing today because Tier-R
conc has no benchmark riding on it yet; it will cost something the
moment family F is written, which is the argument for writing it.

The corpus IR-volume ratchet moved 0.600 → 0.630 for this population
change, with both sub-geomeans recorded in `bench/gates.json` so the
gate cannot be read as having gotten weaker for the code it was
already watching: the non-conc population is 0.582, unchanged.
