# s280 — the hashed Map: an index behind the same entry points (#1003, #894)

Footprint: cross-cutting

The runtime's `Map` gets a deterministic open-addressing index over its insertion-ordered entry buffer. The seven `__wolf_rt_map_*` entry points keep their signatures. No clause in `spec/` moves, and codegen does not change. The final-head gauntlet record (g3, at the head rebased onto 0.2.37, every `END` line from every take) and the plant record are in the PR body. This file is the note's five sections.

## Forbidden

- No `spec/` change, no new crate dependency, no codegen change. The diff touches `crates/wolf_rt/src/map.rs` (shared by `wolf_rt_none`), two test files, two test fixtures, `CHANGELOG.md` and `notes/`.
- Clean room: no libc, kernel or other language runtime's hash-table source. The design is textbook open addressing with linear probing. The hash is FNV-1a with its published constants, then the published 64-bit finalizer constants of MurmurHash3's fmix64.
- The r42 release branch and its PRs were not touched. The lane lands after 0.2.37, from trunk.
- Hosts: nothing in `/tmp` or `/var/folders`, and TMPDIR stays under the lane's directory. No commit was signed on hasu. Every memory-growing witness ran on hasu under `prlimit --as=3221225472` inside a 12 GiB container. The Mac ran only `cargo test -p wolf_rt` unit rows (at most 258 entries each) behind the Mac build lock.
- No label, merge, tag or issue closure, and no attribution trailer. #1003 stays open at this merge (its std half is wolf-std#109); the orchestrator reads #894 at the merge.

## Inputs, verified

- Base: wolf-lang trunk `e2757a7bd702d3b21f06396291010d2d1cb6101d` (0.2.37), rebased once from `5f1f1c949582f53121e4047bc83c86f23ff72240` (R-s280-3). Branch `s280`.
- Measurements: hasu block A (CPUs 0–5), both block-A lock names, image `localhost/r41-toolchain:1`, `--memory 12g`, `taskset` inside, `CARGO_INCREMENTAL=0`. hasu is an Intel i7-12700KF; Rust 1.97.1; Ubuntu cc 13.3.0 at -O2 for the C bar.
- At the rebased head: std pin `5b3a59942b2ee69b99682544341ce8a12c984e9e` and lupin 0.1.59 (pin `b16353e8ae616ff3a35bd27adc56b727c50df345`), archive SHA256 `12121909c289ac8ff4a138de8faabdb86b72df97988832b39047d5f312ec3a18`, the digest wolf-interp's v0.1.59 release publishes. Each gauntlet prep step reads both twice and checks them against `STD-PIN` and `PAIRING`. The measurements before the rebase used std `78b9d3cbc41d51db44d18c40b6694403644d1c83` and lupin 0.1.58 (archive `34f52792bb59bc7565e0edc1235cd47d9a6ad13d5e6528993627f2e699b62193`).
- The new pin's `map.get` still walks `pairs()` (wolf-std `std/map/map.lu:241` at `5b3a59942b2ee69b99682544341ce8a12c984e9e`). sc78's index read is on wolf-std's trunk, not in this pin.
- The header layout, unchanged by the rebase: 88 bytes. The `ListHdr` prefix is `data` 0, `len` 8, `cap` 16, `elem` 24 and `region` 32; then `key_kind` 40, `key_size` 48, `val_off` 56, `val_size` 64, `index` 72 and `index_cap` 80.
- #1003's witness (`notes/s273-evidence/witness/w972.lu`, `c972.c`) in its modes `issue` (std `map.get`), `index` (`m[k]`), `setonly` and `intmap`, plus the ledger modes `region`, `rindex` and `rset`.
- `crates/wolf_rt/src/map.rs`, `spec/10-types.md` `[type.map]` and `spec/02-memory-model.md` `[mem.region.account.1]`, read whole before the change.
- README read, current: it states nothing about maps or their cost.

## Prediction

`notes/s280-evidence/prediction.txt` (`8b0e8765`) was committed with the base measurements before any test or runtime change. The full matrix is in `baseline.log`.

| N | mode | native s | release s | checked s | C s |
|---|---|---|---|---|---|
| 100,000 | issue | 41.57 | 16.35 | 45.58 (unsupported) | 0.01 |
| 100,000 | index | 16.12 | 16.15 | 19.52 | |
| 236,000 | issue | 169.68 | 67.97 | 134.44 (unsupported) | 0.04 |
| 236,000 | index | 62.36 | 63.21 | 116.15 | |

Base native index 50k → 100k: 4.73×, three paired runs.

It predicted:
- The first five header words stay the `ListHdr` prefix, and two words are appended (the index pointer and its slot count).
- FNV-1a over exactly the equality bytes, then an integer avalanche. Linear probing stores entry index + 1, with 0 meaning vacant, in a power-of-two table at most half full.
- `remove` shifts entries and rebuilds the index in the retained table. `clear` zeroes the slots and keeps both buffers. Neither allocates or charges. `copy` builds an independent index in the ambient region.
- Header charge +16 B. Index slots cost 16–32 B per peak entry beyond the initial table, and less than 64 B per peak entry including abandoned tables, so memory stays linear.
- Native ledger deltas of 524,176 / 4,194,192 / 8,388,496 B at 10k / 100k / 236k. Checked ledgers unchanged.
- Every answer, every `pairs()` order, the checked machine and lupin agreement unchanged.
- std's `map.get` still walks `pairs()`, so `issue` mode stays quadratic, and it is reported, not gated (R-s280-1).

What held: every line above. The measured ledger deltas equal the predicted ones exactly at all nine (mode, N) points. All 12 checked observations and 33 native/release/C answers are identical to the base (`check-observations.py`, `observation-comparison.log`).

## Evidence index

All under `notes/s280-evidence/`.

| file | what it shows |
|---|---|
| `prediction.txt` | the base table and the design, before any change |
| `baseline.log` | the raw base matrix: 55 `END … exit=0` |
| `red-59eff973.log` | hasu at the gates' commit: both timing gates red by name (doubling 4.7105 against bound 2.5; 100k at 988.8043× same-run C against bound 20). The semantic row passed there too (lupin unset in that take), as an invariant must. |
| `unit-red-b299667b.log` | Mac: six indexed unit rows red by name before the index (`indexed_integer_collisions_survive_rehash`, `indexed_string_collisions_compare_contents_after_growth`, `indexed_remove_patches_survivors_without_reordering`, `indexed_clear_reuses_storage_and_forgets_old_keys`, `indexed_copy_has_independent_storage_and_grows_from_exact_capacity`, `indexed_growth_charges_abandoned_tables_to_birth_region`) |
| `unit-red-slot-error.log` | the first red attempt, void: two removal slots were inferred as i32, fixed in `b299667b`. Kept, not replaced. |
| `unit-green-65de7724.log` | Mac: all 13 hosted map unit rows green with the index |
| `head-7d0539c9.log` | hasu at the code head: 61 `END … exit=0` (runtime, freestanding, index gates, memory regressions, the after matrix) |
| `after.txt`, `after-table.txt` | the after matrix and its reading |
| `check-observations.py`, `observation-comparison.log` | answers identical to the base; all nine ledger deltas exact |
| `corpus-advance.txt`, `corpus-proof-5f1f1c94.log` | R-s280-2: why the first full gauntlet went red at `corpus`, and the base proof |

Measured after, same host and image, three interleaved samples (`after.txt`):

| gate | base | head | bound |
|---|---|---|---|
| native time(100k) / time(50k) | 4.7105× | 1.9730× | 2.5× |
| native 100k / same-run C | 988.8043× | 1.2591× | 20× |
| native 236k / same-run C | not reached | 1.4212× | 20× |

- Native 100k `index` median: 15.993704380 s → 0.020333984 s.
- Native peak KiB at 10k / 100k / 236k: base 2704 / 9984 / 20532, head 3144 / 14496 / 29028, C 2468 / 8796 / 18552. That is about 129 B per entry from 10k to 100k, still linear.
- `issue` mode (std `map.get`, the pinned std) is still quadratic: native 100k 41.57 → 34.05 s, 236k 169.68 → 135.92 s. Inserts got cheap; the lookup walk did not. That is wolf-std#109's half of the work.
- The checked machine is unchanged, and no checked speedup is claimed.

The rows, one per constraint (contract item 5). The six marked *red first* failed by name at `b299667b`, before the index existed. The three marked *with the index* drive its internals or its allocator hook, so they could not be written against the base:
- `remove` keeps order with the index patched: `map::tests::indexed_remove_patches_survivors_without_reordering` (red first).
- Growth across a rehash: `indexed_integer_collisions_survive_rehash` (red first).
- A full table: `full_index_miss_is_bounded_and_next_insert_grows` (with the index, `65de7724`).
- Colliding keys, by contents: `indexed_string_collisions_compare_contents_after_growth` (red first). `deterministic_hash_vectors_and_empty_binary_strings` (with the index) pins the int–int, str–str and str–int slot collisions; a str key and an int key collide only across typed maps, since the language never mixes key types in one map.
- Region accounting after growth: `indexed_growth_charges_abandoned_tables_to_birth_region` (red first; exact birth-region bytes, and the ambient child is charged nothing).
- `clear`: `indexed_clear_reuses_storage_and_forgets_old_keys` (red first).
- `copy`: `indexed_copy_has_independent_storage_and_grows_from_exact_capacity` (red first).
- Freestanding: `wolf_rt_none`'s `hook_contract::map_index_growth_removal_and_copy_obey_the_allocator_hook` (with the index, `bb1c0f72`).
- Machines: `map_index_lanes::indexed_map_observations_agree_with_checked_and_lupin` (checked, native, release and lupin 0.1.58 print the same line).
- Gates: `native_string_index_doubling_costs_at_most_two_point_five` and `native_string_index_stays_within_twenty_times_same_run_c` (both red first, at `59eff973`).

### Rulings

- **R-s280-1, the bounds (recommended, implemented).** Native string-key inserts plus `m[k]` lookups: the median of three interleaved time(100k)/time(50k) pairs is at most 2.5. The 100k and 236k workloads cost at most 20× the same run's C, three interleaved samples each. The gates measure the runtime's `index` mode. `issue` mode (std's `map.get`) is reported, not gated, because it walks `pairs()` whatever the runtime does (wolf-std#109). Runtime unit rows also bound equality comparisons (at most 16 per probe), so the algorithm is guarded without a clock.
- **R-s280-2, the gauntlet's std (accepted).** The first full gauntlet (g1, `9973f2b3`) was green through `test` and red at `corpus`. Two `resolve` fixtures reached `wir`, because the harness exported `WOLF_STD` globally and `conform-run` falls back to it. The base shows exactly those two with the export, and 0 bad without it. The harness now unsets `WOLF_STD` and shows that in each take's record (`env-std-*` steps). `WOLF_DIST_STD_SRC` stays, and so does the explicit `WOLF_STD` the measurement scripts set. Neither corpus header changes. The whole gauntlet was taken again (g2, green at the pre-rebase head), and again at the rebased head (g3).

- **R-s280-3, the rebase onto 0.2.37 (recommended, implemented).** The branch was rebased once onto `e2757a7b`. `git range-diff` shows 13 of its 14 patches identical. The changelog commit was re-cut: the release opened a new Unreleased section, and a plain replay had put the entry inside 0.2.37's. The runtime, the lane's tests and its fixtures are byte-identical to the measured code head. Options: (a) keep the measurements taken before the rebase, and take the whole gauntlet and the plant again at the rebased head with the new pins; (b) also measure the whole matrix again, which costs about 2 h of a block and measures nothing the diff can change. (a) is implemented. The evidence logs keep the names of the commits they were measured at.

### Commits

- `5eac30b1` Predict the hashed map and record its baseline (before the rebase: `8b0e8765`)
- `e3d523a6` Gate indexed map operations and birth-region charges (before the rebase: `2dc88948`)
- `64512315` Gate map index scaling and same-run C cost (before the rebase: `59eff973`)
- `f1308534` Size map removal test slots to the runtime ABI (before the rebase: `b299667b`)
- `60b3826f` Record six named map unit failures before indexing (before the rebase: `94394034`)
- `d531f947` Index ordered maps with deterministic open addressing (before the rebase: `65de7724`)
- `67ff9ec3` Exercise map index ownership through freestanding hooks (before the rebase: `bb1c0f72`)
- `5d693878` Record the indexed map unit pass on arm64 (before the rebase: `7d0539c9`)
- `c5fc2858` Describe retained index capacity by peak entry count (before the rebase: `a6e03a48`)
- `6dd87e63` Record indexed map timings and exact ledger deltas (before the rebase: `9973f2b3`)
- `1dc14b61` Record the base corpus with and without a global WOLF_STD (before the rebase: `39ba337a`)
- `b3a08795` Explain g1's corpus red as the harness's global WOLF_STD (before the rebase: `aaffe856`)
- `09b55fb2` Changelog the hashed Map under Unreleased (before the rebase: `3aca7a54`)
- `a0976c74` Write the s280 note (before the rebase: `04fc83b6`)

## Done-when

- [x] Base matrix measured and committed before any change (item 1).
- [x] The index under every #1003 constraint, in `wolf_rt` and `wolf_rt_none` (item 2).
- [x] Order, answers, checked machine, lupin agreement and memory rows unchanged; bytes per entry stated (item 3).
- [x] Both gates red at the base by name, then green (item 4).
- [x] One row per constraint (item 5): six red first; three written with the index because they drive its internals.
- [ ] The full `cargo xtask ci` gauntlet green at the final head, in takes of at most 2 h, aggregated (record in the PR body).
- [ ] One plant, local: `s280-plant-*`, one commit, one file, one hunk, red by name in its record (in the PR body).
- [ ] Tier full green under `full-matrix` (the orchestrator labels and watches).
- [ ] At the merge: #1003 stays open until the pinned std carries wolf-std#109 (sc78), because its own `issue` program goes linear only then. The orchestrator reads #894's scope at the merge and handles it by hand.

Test targets:
- [ ] `cargo test -p wolf_rt --lib map::tests::` (13 rows)
- [ ] `cargo test -p wolf_rt_none --test hook_contract`
- [ ] `cargo test -p wolf_driver --test map_index_lanes` (3 rows, Linux)
- [ ] `cargo test -p wolf_driver --test map_pairs_walk_lanes` (7 existing memory rows, bounds unchanged)
- [ ] `cargo xtask ci`: build, gates and coverage shards
- [ ] CI tier full
