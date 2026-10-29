# s270b — the large region value (#948)

Lane s270b, crawl 54, Claude Opus 5.5 (moved from s270 after a provider content flag cut that lane
on this item). Contract `wolf/sprints/compiler/s270b-the-large-region-value.md`. Branch `s270b`
off trunk `5adf3d91` (v0.2.36). One sentence: **no machine mishandled a large value — the
compiler's memory check admitted a `push` the spec refuses, and native and release then read a
freed arena.**

## Forbidden

No spec edit (`spec/` untouched; the spec answered the question, so no ruling is asked). No
runtime, codegen, loader, CI or pin change: the fix is one rule in `wolf_mem`, the memory check
every compiling machine shares. No merge, tag, release, label, issue filed or closed. No third-party
source read: yew is the maintainer's own (read-only, a fresh clone at `bf7ab4a3`); the runtime and
compiler read are this repo's. Builds and runs on hasu block A only, every run under
`flock /home/mfwolffe/lanes/hasu-block-A.lock` taken per run (no lock held idle; every wait behind
s270's and the other lanes' containers printed a line a minute), in rootless podman with `localhost/r41-toolchain:1`,
`--memory 12g --pids-limit 16384 --ulimit nproc=32768:32768`, a foreign pid 1 (`--init`, the test
command dropped to uid 1000) and r41's disposition-neutral taskset shim. No `CARGO_TARGET_DIR`
outside a checkout; `CARGO_INCREMENTAL=0`. Git and `gh` on the Mac worktree only. Nothing of the
maintainer's touched. No attribution trailers.

## Inputs, verified

- wolf-lang trunk `5adf3d91f75b3545ff8d38e5b6693b0b9f367a05` (v0.2.36), a fresh clone on hasu at
  `/home/mfwolffe/lanes/s270b/wolf-lang`; the base compiler for every before/after is a second
  checkout of that sha (`/home/mfwolffe/lanes/s270b/base`), dev-built in the same image.
- wolf 0.2.35 `wolf-0.2.35-x86_64-unknown-linux-gnu.tar.gz` sha256
  `4cc4045bbbb605c07835bc4f32685953c784eeef4cd3139a48b457dfc90dad1a`, lupin 0.1.58 (PAIRING's
  `lupin-version`, commit `6240efe`, pin `a5378e3`) `lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz`
  sha256 `34f52792bb59bc7565e0edc1235cd47d9a6ad13d5e6528993627f2e699b62193`: both digests read off
  the GitHub release assets and checked twice against the bytes (copied from s270's inputs).
- yew `bf7ab4a3472dc7bdf03e2090e7521703e5b790d6` (yew#70's sha, on yew trunk), a fresh clone;
  the witness `notes/yw07b-evidence/stdlib/scratch-once/main.lu` with `src/{fl,util,unicode}`,
  `diff -r` identical to the clone. Its std: 0.2.35's bundled one, on both compilers (as s270 did).
- wolf-std `78b9d3cbc41d51db44d18c40b6694403644d1c83` (STD-PIN); the s267 musl link bundle
  `dd09895a13d3ea4056db73d3e974f025a732391c38044094ea6fc8ed69fb54cc`, checked twice; the
  taskset shim sha256 `51d26e56c4740f8b6a5040490fc33339c772d9ee97f07abd1b495f9b82d18a41`, r41's bytes.
- s270's own measurements before the scope moved (read-only: `/home/mfwolffe/lanes/s270/logs/
  yew-versions.log`, `yew-035-threshold.log`): the issue's control changed two variables; at 0.2.35
  the byte axis shows no failure, the list axis passes at 16 and fails at 17. Reproduced below.
- README read, current: no statement it makes moved.

## Prediction

Committed before the first change as `cead26d0` (`notes/s270b-evidence/prediction.txt`): not a
large-value path, not a byte threshold, but the memory check — s255 put a builtin `push`'s call
site in the list's own region, and nothing stood for the element the push stores, the copy
wolf-lang#385 makes in the ambient region. Hits and misses at the end.

## 1. Four machines at trunk

yew's `scratch-once`, every cell from `conform-run --json` (records `records/repro-1.jsonl`):

| bytes, list elements | checked | native | release | lupin 0.1.58 |
|---|---|---|---|---|
| 1,024, 2 (the control) | exit(0) | trap(bounds) | trap(bounds) | trap(region-fault) |
| 1,024, 16 / 17 / 64 | exit(0) | trap(bounds) | trap(bounds) | trap(region-fault) |
| 100,000, 2 / 64 (the issue) | exit(0) | trap(bounds) | trap(bounds) | trap(region-fault) |

lupin names the read: "`vm.gc.slots[10].t` reaches into `native_scratch` (region #1), which was freed
wholesale; the value died with the region [mem.region.intra.2]". The same six at wolf 0.2.35's
release archive: checked exit(0); native and release exit(0) at 2 and 16 elements, trap(assert) at
17, SIGSEGV (no exit code) at 64, at either byte count — the issue's report.

## 2. The threshold, and why trunk traps `bounds`

**Bytes: no threshold.** At 2 list elements 1,024 and 100,000 bytes both run on 0.2.35 (s270's
bisection found no failure across 1,024..100,000; reproduced at both ends).

**List elements, wolf 0.2.35 native** (`records/sweep-1024.jsonl`, 1,024 bytes): 1–16 exit(0),
17–24 trap(assert) (yew's `fl/value.lu:169`, a slot reference that no longer validates), 25 and up
SIGSEGV. 17 is the count at which yew's `fl_list_push` reserves the slot's `vals` a third time (8,
16, 32 values of 24 bytes): one more buffer in the block that is freed. Where the answer turns from
right to `assert` to SIGSEGV is the arena's — which freed chunk comes back, and what lands on it —
not the language's.

**Trunk's `trap(bounds)` is the same cause, surfacing through the dev build's runtime.** A bisect
`v0.2.35..5adf3d91` on the dev-built compiler marked every step bad down to a docs-only commit
(`logs/bisect.log`): the good end was the release archive, not the source. Built the same way the
two releases agree (`records/build-profile.jsonl`): 0.2.35's own source dev-built traps `bounds` at
every count, and trunk built `--release` reproduces the archive's pattern exactly (exit(0) at 1, 2,
16; assert at 17, 24; SIGSEGV at 25, 64). A dev `wolf_rt` zeroes a pooled chunk when it hands it out
again (`take_chunk`, `cfg(debug_assertions)`), so — consistent with every reading above — the freed
block's slot headers read back as empty lists and the collector's copy loop (`fl/gc.lu:318`) indexes
past a length that just became 0. And with yew's allocator repaired (below) the base compiler —
trunk without this fix — runs the witness to the right answer on checked, native and release at
every size tried (1,024 bytes with 2, 16, 17, 33 and 64 elements; 100,000 bytes with 2 and 64), as
wolf 0.2.35 does (`records/yew-copy.jsonl`, 70 rows).

**The reduction** (`probes/e/`, base compiler): `e1_no_block_work.lu` is the smallest that differs —
a callee pushes `Slot { t: -1, bytes: List[int]() }` into the heap its caller owns, the call sits in
a `region` block, a second region takes the freed chunk back, and the read comes after: checked
`1 100 99 1000`, native and release SIGSEGV, lupin `region-fault`. What it needed: the block, the
store of a heap-holding element into the outer list (yew's "rooting" is that store: the slot table
is the root), the chunk's reuse (yew's collection is one: `e3_collect_reuse.lu` pushes into a slot
whose header the block held and traps `bounds` on the dev build), and the read after. Without the
reuse (`e2_no_reuse.lu`) every compiling machine prints the right answer from freed memory and only
lupin says so.

## 3. Which machine is wrong

The program is ill-formed, and the spec says so without a ruling:

- `[mem.region.edge.elem]` (wolf-lang#385): a non-`Copy` element passed plainly to `push` is COPIED
  into the container, at a cost of "one allocation for each list or map reached ... in the ambient
  region"; `[mem.region.create.3]` (D12): a callee's ambient is its caller's, so inside a block the
  copy is the block's.
- `[mem.region.outer-write]`: a store into an outer value from inside a block — "a `push` ... into
  such a container" — "is admitted when the stored value names no region ... or is written `copy e`
  at the store ... Every other store from inside the block into an outer value is refused as
  before"; through a callee, a `mut` argument gets the block's region unless the parameter is
  ambient-clean, and clean is "container stores of scalars or of `copy`s".
- `[mem.region.escape]`: such a value "held by a binding declared outside the region's block" is
  E1010; `[mem.region.intra.2]`: the block frees it wholesale.

yew's `fl_gc_alloc` pushes `fl_empty_slot()` — an `FlObj` holding seven `List`s — plainly into
`vm.gc.slots`, and `fl_str_new` and `fl_list_new` call it inside `region native_scratch { }` with
`vm` declared outside. That is the refused store. **lupin is right** (it traps `region-fault` at
the read: the dynamic counterpart E1010 maps to in the differ). **The compilers' memory check is
wrong** — `wolf_mem`, the front end the checked machine, native and release share, accepted it:
`--dump=mut-clean` at trunk says `fn fl_gc_alloc #0 vm: clean` (`records/mutclean.err`). The checked
machine's exit(0) follows from that acceptance — its lists are values in an arena that never frees
(`ubcheck`'s `lists`/`list_region` are for the ledger, not for lifetimes), so it cannot see a read
of a freed container; native and release did what an arena does with freed memory.

**The cause, in one sentence:** s255 attributed a builtin `push`'s call site to the list's own
region — right for the growth — and so dropped the only site that stood for the element the push
stores, the deep copy it makes in the ambient region, so a plain `push` of a `List` (or a struct
holding one) into an outer list was admitted inside a `region` block and a callee doing it was
judged ambient-clean.

## 4. The fix

`wolf_mem/src/lower.rs` (`51113ec0`): a builtin store's element passed plainly (not `take`, not a
`copy` aimed at an outer container) whose copy reaches the heap — `copy_reaches_heap`, the mirror of
`wolf_wir`'s predicate `deep_copy_in` asks — gives the receiver a second site, minted in the AMBIENT
region and held beside s255's growth site. The block's close then refuses it (E1010 naming the
push, with the clause's `copy e` repair); through a callee the parameter is unclean, and the call is
E1010 naming the callee's store. Only where s255 moved the site (a `List` receiver); `Pool.init`'s
receiver already held the ambient site. Measured scope at the base (`probes/d/`, `records/
probe-d-*.jsonl`, identical before and after): an index store and a `Map` store of a block-built
`List` were refused already (E1004 in the block, unclean through a callee); `push(copy …)` in the
block, `push(take x)` of an outer-built list, a push into a block-local list and a push of a struct
of a `str` and an `int` keep their answers. The new verdicts on the probes (`records/probe-fix1-*`):
`p1`–`p4` and `c1`–`c3` E1010 on all three compilers; `p5` (`push(copy empty())` in a callee) stays
clean and runs.

yew's witness at the head: E1010 at every size (`records/yew-head.jsonl`). The repair is the word
the report names, `(mut vm.gc.slots).push(copy fl_empty_slot())` (twice, `fl/gc.lu:123-124`):
with it the witness prints `native scratch constructors survive outside region and collection` on
checked, native and release at all seven sizes, on the head, the base and wolf 0.2.35 alike;
lupin 0.1.58 traps `region-fault` on it at every size (the `push(copy …)` through a `mut` parameter
it does not mirror yet, below).

## 5. Rows

| row | checked | native | release | lupin 0.1.58 |
|---|---|---|---|---|
| `memory/region_outer_write_push_heap_call.lu` (the reduced program) | fail(E1010) | fail(E1010) | fail(E1010) | trap(region-fault) — agreement |
| `memory/region_outer_write_push_heap.lu` (the store in the block) | fail(E1010) | fail(E1010) | fail(E1010) | trap(region-fault) — agreement |
| `memory/region_outer_write_push_plain_copy.lu` (an outer-built `List`, the copy in the block) | fail(E1010) | fail(E1010) | fail(E1010) | exit(0) — completeness |
| `memory/region_outer_write_push_copy_under.lu` (16 elements) | `4 1024 19 16 4 0 2000` | same | same | same |
| `memory/region_outer_write_push_copy_over.lu` (17 elements) | `4 1024 19 17 4 0 2000` | same | same | same |
| `memory/region_outer_write_push_copy_control.lu` (1,024 bytes, 2) | `4 1024 19 2 4 0 2000` | same | same | same |
| `memory/region_outer_write_push_copy_call.lu` (yew's callee, 100,000 bytes, 64) | `4 100000 101 64 4 0 2000` | same | same | trap(region-fault) — lupin owes |

At the base the three refusals ran: `_push_heap_call` checked `1 100 99 1000`, native and release
SIGSEGV; `_push_heap` checked `1 -1 100 99 2000`, SIGSEGV; `_push_plain_copy` checked and lupin
`1 100 99 2000`, native and release `1 1000 7777777 2000` (`records/rows2-base.jsonl`). The four run
rows give the same answer before and after: they guard the repaired path (a materialised copy in the
list's region, its growth across the doublings, read after the chunk's reuse and a collection).
`crates/wolf_driver/tests/region_outer_write_lanes.rs` runs every one on the checked machine, native
and release, asserts the reports' words, and pins lupin's column (`Lupin::ParamCopy` for the one
copy lupin 0.1.58 does not mirror: `push(copy …)` through a `mut` parameter; an unlisted lupin must
mirror).

## 6. The plant

Branch `s270b-plant-copy-in-list-region`, draft PR #994, built off the line: the code head `dc9eee84`
plus one deliberate break the fix never contains (`7abecf49`) — the copy a plain `push` makes is
minted in the list's own region, where s255 put the growth, instead of the ambient region: the model
that admitted #948. Run locally on hasu block A at `7abecf49` (`logs/plant.log`,
`logs/plant-outer-write.log`, `logs/plant-corpus.log`), red by name:

- `region_outer_write_lanes`: `test result: FAILED. 22 passed; 3 failed` — exactly the three #948
  refusals, each at `region_outer_write_lanes.rs:233` "`… on --checked`" (the row's verdict is not
  `fail(E1010)`): `a_callee_pushing_a_slot_it_builds_into_the_heap_is_refused_naming_its_store`,
  `a_plain_push_of_a_block_built_slot_into_an_outer_list_is_refused_naming_copy`,
  `a_plain_push_copies_an_outer_list_into_the_block_and_is_refused`. The four run rows and every
  s255 row stay green.
- `cargo xtask corpus`: `3 bad` — `memory/region_outer_write_push_heap.lu`, `_push_heap_call.lu` and
  `_push_plain_copy.lu`: "header claims phase `typecheck` but deepest passing phase is `wir`".
- rustfmt clean, so the plant's red is the rows', not a format step's.

The plant's CI run: TBD (read once).

## 7. The differ and the census

**The corpus, compiler control** (`cargo xtask differ` with the head as A and trunk `5adf3d91` as B,
both dev-built, each tier forced on both sides through `scripts/protocol-proxy.py`, r41's proxy
model; `logs/differ*.log`, raw exits kept — a named move is the claim, not zero divergence). 1,235
files. **Checked: three divergences, exactly the three new refusals** (A `fail(E1010)`, B `exit(0)`);
A executed 748 entries, B 751. **Native and release: the same three** (on two of them B's binary
dies, "conform-run exited nonzero") **plus the three undefined-behaviour probes** whose exit numbers
are reads of freed raw memory and move from run to run on any build (`conc/atomic_ub_uaf.lu`,
`memory/unsafe_ub_uaf.lu`, `memory/volatile_ub_uaf.lu`: native `125 vs 248`, `5 vs 77`, `234 vs 198`;
release `14 vs 215`, `217 vs 13`, `90 vs 79` — the same three s263's control named "native
undefined-behaviour probe", `notes/s263-evidence/differ-interpretation.txt`). No other entry changes
meaning; the four new run rows agree everywhere.

**Downstream, at the memory check** (`scripts/census.py`, `logs/census.log`): every `.lu` file of 24
repositories at their trunk heads (`records/census-heads.txt`: aspen, boreutils, clef, cub, den,
fugue, jobq, lair, ligature, lobo, lope, mk, muzzle, pack, pax, pelt, scent, spell, tomlq, trail,
wolf-book, wolf-lsp, wolf-std, yew), `conform-run --json --phase=mem` with the base and the head and
wolf-std at the STD-PIN: 2,912 files, 1,780 reaching the check. **Two verdicts move, and both are
this bug reported downstream before**:

- pax `notes/px72/region-witness.lu`: pass → fail(E1010). Its own header says "This unsafe variant
  must be rejected", and pax's `notes/px72-the-fsck.md` asks for it to be verified and filed: "accepted
  by wolf 0.2.35 although a region-local buffer escapes inside a newly constructed aggregate ... The
  initial checker crashed with that shape". It is `(mut h.nodes).push(Node { raw: copy raw })` inside
  a block through a `mut` parameter — the plain push's copy, in the block.
- yew `notes/yw07-evidence/reuse-row-move/main.lu`: pass → fail(E1010). It is **wolf-lang#919**
  (yew G50: "native and release terminate with SIGSEGV (139)", the checked machine prints PASS).
  #919 blamed `reserve()` inside the region; the runtime grows a list in its birth region, and what
  put the rows' lists in the block is `(mut pool.spare).push(row)` and `(mut pool.items).push(row)`
  — two plain pushes of a `Row` in `refill_inside`, called inside `region scratch { }`. Same cause.
  The repair is `take` at those two pushes (`probes/f/f2_reuse_row_move_take.lu`): no copy, so the
  rows' lists stay where the pool made them; at the head `refill_inside`'s `pool` is clean and the
  program prints `two-list row refill: PASS` on checked, native, release and lupin
  (`records/probe-f-head.jsonl`), while the unrepaired `f1` is E1010 on the three compilers.

Nothing else moved in any repository: no wolf-std module, no boreutils utility, no lobo, pelt, scent
or aspen file.

## 8. The gauntlet

**First run, at `822b236c` (red, kept):** chain2 on hasu block A, 2026-10-28T23:29Z–00:05Z
(`logs/chain2.log`, `logs/gauntlet-822b236c.log`, trimmed to the header and the failure). build,
dist-rt, clef and ligature green; `cargo xtask ci` exit 1 at `test`: `wolf_lex`
`corpus_snapshots::corpus_token_streams` — "snapshot assertion for
'memory__region_outer_write_push_copy_call' failed", a new corpus row with no token-stream
snapshot. Every suite that walks the corpus snapshots each row (`wolf_lex` `corpus_snapshots`,
`wolf_parse` `corpus_decls`, `wolf_wir` `lower_corpus`'s ledger); the seven rows had none. Then
`cargo test --workspace --no-fail-fast` once at `822b236c` with `INSTA_UPDATE=no`
(`logs/ws-test-822b236c.log`, 2026-10-29T00:44Z–01:15Z): **exactly three red binaries** —
`-p wolf_lex --test corpus_snapshots`, `-p wolf_parse --test corpus_decls`, `-p wolf_wir --test
lower_corpus` (`corpus_lowering_ledger`, whose diff is the seven rows' seven lines and nothing
else) — and every other test binary green (339 `test result: ok`). TBD: the snapshots, generated
after the rebase onto s270's merge, and the gauntlet at that head.

**Which tree each step ran on.** chain2's move to the pushed head printed git's refusal (lines 4–17
of `logs/chain2.log`): the hasu clone still held my working copies of `crates/wolf_mem/src/lower.rs`
and `crates/wolf_driver/tests/region_outer_write_lanes.rs` (modified) and the seven corpus rows
(untracked), copied there by `scp` for the focused checks before they were committed, so
`git checkout -B s270b FETCH_HEAD` aborted. The next commands did the move: `git reset --hard
FETCH_HEAD` (the branch was already `s270b`) replaced those nine paths with the committed bytes —
the same bytes: `lower.rs` and the test hashed `84c3c97d…` and `b087d5d0…` on both machines before
the commits, and the four run rows were committed from the copies `wolf fmt` wrote on hasu — and
`git clean -fdq -e target` left nothing untracked. The script then checked the head (`822b236c`,
the wanted sha) and printed an empty `git status`; the gauntlet's own header repeats both. The plant
ran in its own clone (`/home/mfwolffe/lanes/s270b/plant`, `7abecf49`), and probe-f used the head
compiler built from those same bytes, before the move. No step ran on a wrong tree; the refusal is
noise of the order the copies and the commits were made in.

## Hits and misses

- Hit: the cause (the memory check, the push's copy), no byte threshold, the checked machine's
  exit(0) as a consequence, lupin's `region-fault` as the agreement, `fl_gc_alloc` unclean after.
- Hit: trunk's `trap(bounds)` is the same cause; the repaired witness runs on every compiling
  machine at every size tried.
- Miss: I predicted 0.2.35 and 0.2.36 differ in how the cause surfaces. They do not: the difference
  is the dev build's runtime (pooled chunks zeroed), a fact of how the issue's 0.2.36 column was
  built, not of either release.
- Hit: the corpus differ moves exactly the three new refusals (and the three UB probes that move on
  every comparison).
- Half a miss: I predicted the census would move only yew. It moved one yew file and one pax file,
  and both are this bug as reported downstream before (pax px72's witness "must be rejected",
  yew G50 is wolf-lang#919); nothing else in 2,912 files.

## Issues (none filed or closed here)

- wolf-lang#948: this PR is its fix; the orchestrator closes it on the merge.
- wolf-lang#919: the same cause (section 7); its witness is E1010 at the head and runs on every
  machine with `push(take row)`. Recommended: close it with #948 after the merge, citing this note.
- To file, wolf-interp: lupin 0.1.58 traps `region-fault` on `push(copy …)` through a `mut`
  parameter (`region_outer_write_push_copy_call.lu`, and yew's repaired witness): the clause makes
  that copy in the list's own region. And it runs `region_outer_write_push_plain_copy.lu` to exit(0)
  where the clause refuses the store (the push's copy is made in the block).
- To file, yew: the scratch witnesses (`scratch-once`, `scratch-escape`) are E1010 now (G54); the
  repair is `push(copy fl_empty_slot())` at `fl/gc.lu:123-124`. G50's `reuse-row-move` likewise
  (`push(take row)`, #919). No other yew file moved in the census.
- pax: px72's `region-witness.lu` is refused now, as its header asked; nothing to file beyond
  telling px72's follow-on (its note asked for "verify and file separately").

## Evidence index

All under `notes/s270b-evidence/` unless named.

- `prediction.txt` — the prediction, committed `cead26d0` before the first change.
- `records/repro-1.jsonl`, `logs/repro-1.log` — yew's witness on all four machines at trunk and on
  wolf 0.2.35, six (bytes, elements) pairs.
- `records/sweep-1024.jsonl` — the list-length sweep on native, 0.2.35 and the base.
- `logs/bisect.log`, `records/bisect-steps.txt` — the dev-build bisect `v0.2.35..5adf3d91`.
- `records/build-profile.jsonl`, `logs/profile.log` — 0.2.35's source dev-built, trunk dev- and
  release-built, seven counts on native and release.
- `records/mutclean.err` — `--dump=mut-clean` of the witness at trunk.
- `probes/{a,b,c,d,e}/`, `records/probe-*.jsonl` — every probe on four machines (pre-fix files
  carry lupin's text record; later ones its JSON).
- `records/rows2-{base,head}.jsonl` — the seven rows on the base and the head.
- `records/yew-head.jsonl`, `records/yew-copy.jsonl`, `logs/yew-copy2.log` — the witness at the
  head, and repaired, at seven sizes on the head, the base and 0.2.35.
- `logs/census.log`, `records/census-heads.txt` — the downstream census (the full per-file record,
  1.1 MB, stays on hasu: `/home/mfwolffe/lanes/s270b/records/census.jsonl`).
- `logs/differ*.log`, `records/differ-*.exit` — the corpus control on three tiers.
- `probes/f/`, `records/probe-f-head.jsonl`, `logs/probe-f.log` — #919's witness and its `take` repair.
- `logs/plant*.log` — the plant's local red run; `logs/chain2.log`, `logs/gauntlet-822b236c.log` —
  the first gauntlet (red at the missing snapshots).
- `scripts/` — every script that produced the above, as run.

## Done-when

TBD
