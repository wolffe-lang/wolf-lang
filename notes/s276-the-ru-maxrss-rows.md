# s276 — the ru_maxrss rows

Materialised from PR #1017's body at the merge (`d82ecc05`); the lane handed its note back in the PR body.

## Forbidden

Footprint: module

Tests and lane-local evidence only. No runtime, codegen, loader, spec, CI, pin or measurement-bound change. No labels, merge, tag, release, issue closure, attribution trailer, Mac build or maintainer-data writes. Deliberate defects exist only in draft plant #1015. The original repetition proof ran on hasu; the rebased-head checks and re-cut plant verification run on kasumi under a 2 GiB cgroup cap.

## Inputs, verified

wolf-lang#1008; #631 and all comments; s274's one-process measurement note. Original proof: base `bb728d66`, head `64723e93df8a89febf5cdd7aee7c500a3b6612ce`. Current rebased head: `d82ecc05ff9a28ba302bd6a6ec9d22370e4eecd6`, on trunk `7659deb5`, after s272 merged. Branch `s276`; worktree `/Users/mfwolffe/lanes/s276/repo`. Rebase was conflict-free; range-diff reports all seven patches identical. The 200/20 measurements below are explicitly from the original proof head, not a new repetition campaign at the rebased head.

- `278b24d8`: prediction before every change.
- `be98b0ca`: identical rebased Linux baseline/plant runner; the original measured baseline was `92fc161d`, a tests/evidence-only descendant of `bb728d66`.
- `9f3e0da4`: state cwd's floor and exact guarded leak.
- `1120dfb3`: name outer-write's existing no-region sensitivity control.
- `9c9521e2`: distinguish quadratic map snapshots from sub-floor growth.
- `e90368c1`: state checked-budget's measurement limits.
- `d82ecc05`: fail-fast final-head gauntlet and 200/20 proof.
- Re-cut plant `86566402` (#1015), directly on `d82ecc05`; original measured plant `0bcac44f`: root-place cwd answers (#834); remove outer-write's turn block (#839); materialize map entries per get (#972); retain an uncharged `Vec<Value>` per consumed byte view (#308).

hasu: Linux 7.2.4 x86-64, rootless `localhost/r41-toolchain:1`, 12 GiB, foreign root pid 1, lane uid 1000, neutral taskset shim, Rust 1.97.1 (2026-07-14 build). Both block locks held by one long flock each, emptier block chosen per submission. Final proof: block A, CPUs 0–3 inside the container; 2026-11-02 18:51:21–19:47:31Z. Competing element-places suite completed 720 successful iterations with no failed-load sentinel; CPUs 0–3 measured 94.20%, 94.75%, 93.10%, 93.46% busy.

Cargo registry, lupin archive and shim were copied into this lane's directory, never mounted from another lane. Lupin 0.1.58 archive SHA256 read twice: `34f52792bb59bc7565e0edc1235cd47d9a6ad13d5e6528993627f2e699b62193`. Original-proof compiler SHA256 `c4425f45349300621c1a659960907a60b72ad528f11acc76d2c4ef76d0dc8160`; runtime archive SHA256 `581169b170b4a43d8ce641c693bd0f130f58111879fa81084457a18934831850`. README read, current; no install/build/count/feature statement moved.

## Prediction

`notes/s276-prediction.txt` at `c64a4cd8` predicted that the harness floor would mask small readings but not necessarily the regressions these rows guard, at their existing input sizes. It predicted each original-regression plant would fail 20/20 and each positive row would pass 200/200 under load. These predictions held.

The important qualification: the roughly 3 MB floor describes the native witnesses here, not every checked process. Checked map reads about 21–23 MB, checked byte-view reduction 18–19 MiB, and the doubling-budget control 417–418 MiB. All four regression shapes clear their observed floors decisively; none requires an instrument replacement under contract item 2. Equal flat readings still do not establish absence of arbitrary smaller leaks. Comments now state that limitation instead of promising a child-only peak without qualification.

The prediction also proposed a separate doubling-budget implementation plant. That was not implemented: the four requested leak shapes are covered by six named tests. The independent doubling-refusal sibling instead ran in all 200 positive repetitions and the full suite. No new sensitivity claim is made for bypassing its byte-budget check.

## Evidence index

### Original baseline, proof head and each original-regression plant

Original baseline is `92fc161d` (compiler/runtime unchanged from `bb728d66`), original proof head `64723e93`, original plant `0bcac44f`. Each original-proof-head named test passed **200/200**; each of the six named plant tests failed **20/20**, at its intended memory assertion. Independent review of all 120 raw plant logs found no wrong assertion, compile refusal or crash substituted for the memory failure. All suites and all 1,520 proof executions had **0 SKIP** lines.

| Guarded shape / lane | Baseline pilot | Original-proof-head range, 200 runs | Plant range, 20 runs | Unchanged bound / decision |
|---|---|---|---|---|
| cwd root-retained answer, native + release, 200k→400k turns | 2,674,688→2,674,688 B; 0 B/turn | 0 B/turn on both tiers | 45–49 B/turn on both tiers; larger peak 20,783,104–21,291,008 B | ≤4 B/turn; **A: retain** |
| outer-write missing turn region, native, 100k→200k turns | 2732→2732 KiB; 0 B/turn | 0 B/turn | 27–34 B/turn; larger peak 7744–8176 KiB | ≤4 B/turn; **A: retain** |
| outer-write missing turn region, release | 2696→2696 KiB; 0 B/turn | 0 B/turn | 28–36 B/turn; larger peak 7808–8216 KiB | ≤4 B/turn; **A: retain** |
| retained map snapshot per get, native, 2000→4000 entries | 2,838,528→2,838,528 B; 0 B/entry | 0 B/entry | 144,019–144,437 B/entry; larger peak 386,224,128–386,666,496 B | ≤1024 B/entry, peak ≤64 MiB; **A: retain** |
| retained map snapshot per get, release | same baseline pair; 0 B/entry | 0 B/entry | 144,050–144,310 B/entry; larger peak 386,322,432–386,682,880 B | same bounds; **A: retain** |
| retained map snapshot per get, checked, 500→1000 entries | 22,327,296→21,733,376 B; −1187 B/entry | −1908…1564 B/entry | 492,019–493,289 B/entry; larger peak 348,057,600–348,692,480 B | ≤16 KiB/entry, peak ≤128 MiB; **A: retain** |
| uncharged retained consumed-byte lists, checked, n=1024→8192 | 19→19 MiB | 18–19 MiB at either size | 45–46→1821–1822 MiB | big peak <200 MiB, growth ≤64 MiB; **A: retain** |

The plants exceed the floor itself, not merely a zero-difference assertion. Cwd's small/large planted peaks were about 11–12/21 MB versus a flat head floor of 2.49–2.85 MB; outer-write about 4.5–5.1/7.7–8.2 MiB versus about 2.5–3.0 MiB; map native/release about 98/386 MB versus 2.69–3.11 MB. Checked map rises from about 102 to 348 MB, and byte-view reduction from 45–46 to 1821–1822 MiB. No original-regression plant remained below the floor, so **B is selected for no row**.

The existing outer-write no-region control remained above its 4 B/turn bound in all 200 runs: native 27–36, release 27–37 B/turn. The independent doubling-budget sibling passed 200/200, refused with `shadow-memory budget exhausted`, and peaked at 417–418 MiB, below its unchanged 1536 MiB ceiling.

### Artifacts and named reds

All records below are under `/home/mfwolffe/lanes/s276/records/` on hasu:

- `pilot.log`, `pilot-base-<row>.log`, `pilot-plant-<row>.log`: baseline and planted readings, full assertions, source builds. `pilot.exit = 0`.
- `suite-{cwd_region_lanes,region_outer_write_lanes,map_pairs_walk_lanes,checked_budget}.log`: all **35 tests** pass, 0 SKIP; lupin required for the applicable oracle columns.
- `proof-rows.log`: 1400 head exit-0 records and 120 plant exit-101 records; `proof-head-<row>-{1..200}.log` and `proof-plant-<row>-{1..20}.log` retain full output.
- Named reds, each 20/20: `a_cwd_asked_in_a_region_keeps_nothing_a_turn`; `the_turn_region_runs_flat_on_the_native_tier`; `the_turn_region_runs_flat_on_the_release_tier`; `a_string_keyed_map_walked_by_get_keeps_a_linear_resident_set_natively`; `a_string_keyed_map_walked_by_get_keeps_a_linear_resident_set_on_the_checked_machine`; `the_reduction_runs_flat_on_the_checked_tier`.
- `cpu-before.txt`, `cpu-after.txt`, `load-count = 720`; no `load-failed.log`.
- `final.log`, `final.exit = 0`, `gauntlet.exit = 0`: original-proof-head fail-fast gauntlet. Committed scripts: `notes/s276-evidence/{inside,pilot,job,gauntlet}.sh`.

Every original-proof-head END line, directly read from `final.log`:

```text
END checkout exit=0
END build exit=0
END fmt exit=0
END clippy exit=0
END suite exit=0
END proof exit=0
```

### Rebase receipts at the current head

Main head **`d82ecc05`** is on trunk **`7659deb5`**, without conflicts or additional changes. Range-diff reports all seven lane patches identical:

```text
c64a4cd8 = 278b24d8  prediction
92fc161d = be98b0ca  characterization runner
86209125 = 9f3e0da4  cwd guarantee
398cbb65 = 1120dfb3  outer-write control
62b36e5d = 9c9521e2  map guarantee
ba4b1b96 = e90368c1  checked-budget guarantee
64723e93 = d82ecc05  fail-fast gauntlet
0bcac44f = 86566402  re-cut four-shape plant patch
```

Kasumi checks ran at the exact main head on slot **4–7**, one long `flock -w 21600`, taskset inside `localhost/r41-toolchain:1`, container stdin closed, foreign root pid 1. `memory.max = 2147483648` and memory+swap capped at 2 GiB for every build and witness. SIGBLK/SIGIGN were zero. Recorded 2026-11-03 03:47:40–03:50:02Z in `/home/mfwolffe/lanes/s276/records/rebase.log`; `rebase.exit = 0`.

All 28 driver binary unit tests and all 35 tests across the four integration targets pass, 0 SKIP. Cwd and outer-region readings: 0 B/turn on native/release; outer no-region controls: 31/32 B/turn. Map: 0 B/entry native/release, −335 checked. Checked-budget reduction: 19→19 MiB; doubling refusal: 418 MiB. These are new-head checks, separate from the earlier 200/20 repetition proof.

Every rebased-main END line:

```text
END checkout-cap exit=0
END build exit=0
END fmt exit=0
END clippy exit=0
END driver-unit exit=0
END cwd_region_lanes exit=0
END region_outer_write_lanes exit=0
END map_pairs_walk_lanes exit=0
END checked_budget exit=0
END clean-tree exit=0
```

All transferred inputs were verified before use:
- source bundle SHA256 `35c47dc7bc59ed0609e4370a5af3bf3e189c3aef1e159ac773cff46e3083147e`;
- 6180 Cargo files match the SHA256 manifest, whose own SHA256 is `6e504947b188f6ad65dc4b2e560b5280c3b2dd9fa8179119404140d835494553`;
- taskset shim SHA256 `90cc72a168b53850a27615eb9ae64e79074490ebe32cd4516b9e73ffaebe963d`;
- lupin archive SHA256 `34f52792bb59bc7565e0edc1235cd47d9a6ad13d5e6528993627f2e699b62193`.

Rebased compiler SHA256 `b282aeba178ab48925ac85fad7a29b57d14eda088b3a7fc5a29f137a822dd4f1`; runtime SHA256 `c5ae3b2b17d4c334a6c80d88e1fe6761ff816eec6f69e187e96d764c73530eaa`. Main-check cgroup peak reached its 2 GiB cap, with `oom = 0`, `oom_kill = 0`; its three `max` events caused no failure. Lane disk footprint before build-output pruning: 1,087,940 KiB, after: 344,960 KiB, below 3 GiB.

Current main CI run **41574793674** was in progress on its one-shot read. The one-shot plant query still returned the historical run **41493234038 at `0bcac44f`**; no run at the re-cut head was visible in that response. This historical ID is not presented as current-head evidence; no retry or CI wait was made.

**Merge gate: module, local evidence. No label or labelled CI run is required for main #1017.** The existing plant PR stays draft/open by request; its automatic CI is not a merge gate. The orchestrator audits the local records, hand-checks them, merges and handles #1008/#1015. No audit was run by this lane after this rebase.

### Fresh local named reds at the re-cut plant head

Re-cut plant **`86566402`** was also built and run locally on kasumi slot 4–7, under the same **2 GiB cgroup cap**, one long flock, taskset inside and container stdin closed. Source delta range-diff is identical to the original four-shape plant. Incremental plant bundle SHA256: `103982e14656fac0ec6e44b0def01ed070b67a0c7698dafddd28709d4d4bc65a`.

`/home/mfwolffe/lanes/s276/records/recut-plant.log` and `recut-plant.exit = 0` record fresh failures at the intended memory assertions, all exit 101 and **0 SKIP**. Individual raw logs are `recut-plant-<row>.log` in that directory:

| Named red at `86566402` | Fresh measurement | Unchanged bound |
|---|---|---|
| `a_cwd_asked_in_a_region_keeps_nothing_a_turn` | native/release 63 B/turn; peaks about 27 MB at 400k turns | ≤4 B/turn |
| `the_turn_region_runs_flat_on_the_native_tier` | 30 B/turn | ≤4 B/turn |
| `the_turn_region_runs_flat_on_the_release_tier` | 29 B/turn | ≤4 B/turn |
| `a_string_keyed_map_walked_by_get_keeps_a_linear_resident_set_natively` | native 144191, release 144068 B/entry | ≤1024 B/entry |
| `a_string_keyed_map_walked_by_get_keeps_a_linear_resident_set_on_the_checked_machine` | 492240 B/entry | ≤16384 B/entry |
| `the_reduction_runs_flat_on_the_checked_tier` | 47→1823 MiB | big peak <200 MiB; growth ≤64 MiB |

The new planted cwd is 52 bytes long (original plant: 43), so root-arena rounding is now 64 rather than 48 bytes per answer; the observed 63 B/turn is consistent with that shape, not a widened bound. These are one fresh invocation per named test at the re-cut head, not a claim that the historical 20 repetitions were repeated there. They confirm **R-s276-1 A for every row** after the rebase.

Every fresh plant-verification END line (the wrapper expects and validates named test exit 101):

```text
END plant-checkout-cap exit=0
END plant-build exit=0
END plant-test-build exit=0
END plant-named-reds exit=0
```

Fresh plant cgroup: 2 GiB peak, `oom = 0`, `oom_kill = 0`. Lane footprint before pruning its target: 1,137,936 KiB; after: 483,980 KiB. Both main/plant target trees were pruned, keeping the lane below 3 GiB. One initial container checkout precondition failed because a worktree referenced an unavailable host-absolute Git path; no witness ran in that attempt. Its separate `recut-plant-setup-failed.log` is retained. A self-contained clone corrected the precondition; the successful record is `recut-plant.log` above.

## Done-when

R-list, recommendations implemented:

- **R-s276-1:** per row, (A) retain the instrument after proving original-regression sensitivity; (B) replace it with in-witness Anonymous+Swap, bounds unchanged. **Recommend A for cwd, outer-write, map-pairs (native/release and checked), and checked-budget's byte-view reduction.** The table gives each decision and measured separation. Every original shape clears the floor and fails 20/20; no row qualifies for B. Measurements and bounds remain unchanged; only their guarantees and evidence are corrected.
- **R-s276-2:** (A) cover all seven named memory tests across the four files; (B) one test per filename. **Recommend A**, implemented: all seven positive rows 200/200, all six original-leak plant rows red 20/20. The independent doubling-budget control has no additional implementation plant.

Test targets:
- [x] committed prediction before changes; off-line draft plant #1015
- [x] original baseline and each original-regression plant measured on Linux
- [x] `cargo fmt --all --check`
- [x] clippy: all four test targets, `-D warnings`
- [x] all four full test suites: 35 pass, 0 SKIP
- [x] each of seven named memory tests: 200/200 green under four-CPU load
- [x] each of six named plant tests: 20/20 red at the intended memory assertion
- [x] original-proof-head gauntlet green; every END line quoted
- [x] README read, current; `bash -n` evidence scripts; `git diff --check`
- [x] original-proof-head audit: **`AUDIT CLEAN (footprint: module)`**, exit 0; `/Users/mfwolffe/lanes/s276/audit-final.log`; original CI **41493931407** succeeded at `64723e93` (`tier pr`, 3/9 jobs). These are historical, not rebased-head evidence.
- [x] rebased-head capped kasumi checks: 28 driver unit tests plus 35 integration tests pass; 0 SKIP; every END line below
- [x] each branch force-pushed exactly once, with explicit old-head leases; both branch CI IDs queried once; no CI wait
- [x] re-cut plant locally red by all six names at `86566402`, under the 2 GiB cap
- [ ] orchestrator audit and hand check at the rebased head; not run by this lane, as requested
- [x] main PR ready for review after all local evidence was recorded; plant remains draft

Issue #1008 remains open. Recommend its closure by the orchestrator at merge: each row meets the contract's demonstrated-sensitivity alternative. No issue to file from these measurements. Plant #1015 remains draft, never merge. No issue or PR was closed by this lane.

