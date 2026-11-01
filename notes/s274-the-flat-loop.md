# s274 — the flat loop: #631's row reads one process, exactly

Footprint: module

Refs wolf-lang#631 (the row), #612 (the leak it guards), #674 (closed as #631's duplicate; it asked for the one-process reading). Tests only: `crates/wolf_driver/tests/region_copyout_lanes.rs` and this note's evidence. No runtime, codegen, CI or pin change; no CHANGELOG entry (the contract).

## Forbidden

No runtime change (item 1 found no runtime growth: the flat loop is flat inside one process). No spec, CI, pin or shared-tool change. No attribution trailer, merge, tag, release, label, issue filed or closed. kasumi untouched. The planning repo, `~/.claude`, the maintainer's data and other lanes' directories only read; s273's cargo registry and taskset shim, and r41's lupin 0.1.58 archive, copied into `~/lanes/s274/` (reuse by copy), never mounted. Every row run on hasu in rootless podman `localhost/r41-toolchain:1`, `--memory 12g`, a foreign pid 1, both lock names held for the batch by one long `flock -w 21600` each. Every build from a commit (each job asserts its sha and a clean tree). No Mac build: the Mac only edited, committed and ran the checked machine (`wolf conform-run --checked`) on the witness sources. No helper agents.

## Inputs, verified

- Base: wolf-lang trunk `7332772f` (`7332772f2e96b5d949b7d8a5c5583c81b815a8ae`). Branch `s274`, Mac worktree `~/lanes/s274/wolf-lang`; the plant `s274-plant-leak` in `~/lanes/s274/plant`.
- hasu (Linux 7.2.4, x86-64), rootless podman `localhost/r41-toolchain:1`, rustc 1.97.1 (`8bab26f4f`), as uid 1000 under a root pid 1 (SigBlk/SigIgn 0), s273's taskset shim copied. Block B (CPUs 6–11) for batch 1, block A (CPUs 0–5) for batches 2 and 3, picked by waiter count (the 21:10Z rule) from batch 2 on. Every job checks out its commit in a clone of its own (`~/lanes/s274/wolf-lang` for batch 1, `wolf-lang-b2` for batches 2 and 3), asserts the sha and a clean tree, and runs the scripts as committed (`git show <sha>:…`).
- Tools at trunk, batch 1: `target/debug/wolf` sha256 `cb71dce6…`, `libwolf_rt.a` `581169b1…`. In batch 2's clone `wolf` reads `a23b3e54…` (another checkout path) and `libwolf_rt.a` the same `581169b1…`; at trunk and at the head `wolf` and `libwolf_rt.a` are **identical** (`S274_TOOLS`), so the rows differ only in the test binary.
- The row's witness, cut from the test file at trunk by `probe.py`: `row/flat/main.lu` sha256 `470bcba9…` is the file the row writes (`cmp` against `target/tmp/region_copyout_lanes/copy_int/main.lu`: equal) and s273's witness; its release executable `839a7caa…` is s273's too.
- lupin 0.1.58 for the suite's oracle legs: archive sha256 `34f52792…` (read twice), copied from `~/lanes/r41/inputs/lupin058`.
- The load: `cargo test -p wolf_driver --test element_places_lanes`, its test binary run directly in a loop on the rig's CPUs (67 tests, ~3 s an iteration, every iteration green). CPUs 0–3 read 98–99 % busy under batch 2's load4 and 92–96 % for CPUs 6–11 under batch 1's load6 (`notes/s274-evidence/logs/cpu-busy-2026-11-01T2111Z.txt`). Batches 1 and 2 overlapped from 21:09Z to 21:20Z, each on its own block.
- README read, current (it names no row, count or instrument this lane moves).

## Prediction

`da0b9e69` records the prediction before any change (`notes/s274-evidence/prediction.txt`). What held and what did not:

- P1 held: trunk's b − a centred on 0 (medians −12 … +10 kB) with a spread of −176 … +184 kB in 4 kB steps; clipped medians 0.0–0.5 B/turn, worst 9.4 (predicted 5–8: a little worse); red 14–28 % on every rig (predicted 10–30 %), 118 of 510 runs, no rig clean.
- P2 held: the leak 1,289.6–1,307.2 B/turn between two processes, 1,298.6 exactly in one, both tiers; the control the same.
- P3 held: kept-`str` 23.6–41.0 B/turn (32.8 in one process), the control 1,290.2–1,307.0; neither reddened.
- P4 held in its numbers and missed its mechanism. In one process the flat loop moves +4 kB of anonymous memory (0.2 B/turn, every process, every rig; the prediction's bound was ≤ 8 kB) and +0 kB of VmHWM (quieter than predicted). But the noise is not the kernel's counter batching: VmRSS and smaps `Rss` agree within a page. It is the file-backed pages each process maps (176–192 kB of spread), against 8 kB of anonymous spread.
- P5 held: `live_region_bytes()` cannot see the process root.
- P6 held, with one change: no warm-up reading (the first reading at 10,000 turns of a fresh process read 0 kB by 20,000, so a warm-up buys nothing). The head's flat row reads 0.00 B/turn, not the predicted ≤ 0.4.
- P7 held: 0 of 370 red at the head (220 on load4), the plant 32 of 32 at 1,298.84 B/turn, trunk 28 of 110 (25 %) on the same rig.
- P8 held in its outcome and missed its reason: no `ru_maxrss` sibling reddened, but because each reads the harness's floor, not because its bound has margin over the witness's noise.

## Evidence index

### 1. Measured at trunk, before any change (batch 1, block B, `7332772f`)

The row itself, its test binary run directly, 100 times on each rig (`S274_ROW`). Each run measures `release=false` (native) and then `release=true` and stops at the first red leg, so a release sample exists only where native was green. The statistic is `growth_per_turn`'s: max(0, b − a) × 1024 / 20,000 B/turn, with a and b the VmHWM of a 20,000-turn and a 40,000-turn process.

| rig | red runs | native B/turn: median, p95, worst | release B/turn: median, p95, worst | b − a, kB: worst both ways |
|---|---|---|---|---|
| rest6 (CPUs 6–11) | **24 / 100** | 0.2, 5.3, 7.4 (n 100) | 0.0, 5.2, 6.3 (n 87) | −164 … +144 |
| load6 | **14 / 100** | 0.0, 4.5, 7.2 (n 100) | 0.0, 4.6, 8.6 (n 92) | −176 … +168 |
| rest4 (CPUs 6–9) | **26 / 100** | 0.5, 7.0, 8.4 (n 100) | 0.0, 5.1, 8.4 (n 84) | −172 … +164 |
| load4 | **26 / 100** | 0.0, 4.9, 7.4 (n 100) | 0.2, 5.9, 7.6 (n 89) | −168 … +148 |
| load4, batch 2 (CPUs 0–3), trunk interleaved with the head | **28 / 110** | 0.0, 5.8, 7.8 (n 110) | 0.4, 5.4, 9.4 (n 95) | −140 … +184 |

Trunk's row is red in **118 of 510 runs (23 %)**, on every rig; no rig is clean, and the load does not make it worse (load6 was the quietest).

The witnesses, cut from the test file (`probe.py`) and built once per tier with trunk's `wolf`, 100 processes per variant, tier and rig at 20,000 turns and 100 at 40,000 (30 for `str` and `ctl`); each process reads itself at turns 10,000 and 20,000 and at its end (`S274_PROBE`). Per rig the numbers below did not move between rest6, load6, rest4 and load4; the ranges are over all four.

| witness | the row's two-process statistic (B/turn, clipped) | one process, 20,000 → 40,000 turns: anonymous + swap | one process: VmHWM |
|---|---|---|---|
| flat (`STEP_COPY_INT`, the row) | medians 0.0–0.7, worst 9.0; b − a −176 … +176 kB | **+4 kB = 0.2 B/turn, every process** (spread 0) | +0 kB, every process |
| leak (#612's shape: the flat step with `copy region` undone) | 1,289.6 … 1,307.2 | **+25,364 kB = 1,298.6 B/turn, every process** | +25,364 kB |
| str (`STEP_COPY_STR`, one `str` kept per turn) | 23.6 … 41.0 | **+640 kB = 32.8 B/turn, every process** | +644 kB |
| ctl (`STEP_NO_REGION`) | 1,290.2 … 1,307.0 | **+25,364 kB = 1,298.6 B/turn** | +25,364 kB |

**What the noise is made of.** At the same turn (20,000), the flat witness's VmHWM differs from process to process by 180–200 kB on every rig and tier, and so does its smaps_rollup `Rss` (VmRSS and `Rss` agree within one page at all 10,400 readings: on hasu the RSS counters are not the noise). Split by kind, the **anonymous** memory differs by **8 kB** (192–204 kB) on every rig and tier, and the **file-backed** pages (the executable and its libraries, mapped in ahead of use) by **176–192 kB**. So the row's noise is the file-backed pages each process happens to map, nothing the loop allocates, and it is per process: inside one process the reading does not move. The prediction's guess at the kernel's per-CPU counter batching was wrong for this host (P4 missed its mechanism; its numbers held).

**The runtime's accounting** (`live_region_bytes()`, `crates/wolf_rt/src/native.rs`) counts the chunks of named regions only, "neither the process-root arena nor the retired-chunk pool": #612's leak lands in the root arena, so it would read the plant as flat. A root counter would be a runtime change (out of bounds); none was needed.

**The runtime does not grow**: the flat loop reads +0 kB of VmHWM and +4 kB of anonymous memory (one page, every time) over 20,000 turns of one process, so the contract's stop condition (runtime growth) did not arise.

### 2. The fix — R-s274-1 E (`17c2bd22`, comments `8a544687`)

`crates/wolf_driver/tests/region_copyout_lanes.rs` only. The witness takes `[turns, first]` and reads itself twice, at turn 20,000 inside the loop and at 40,000 after it: `mark <turn> anon=<kB> swap=<kB> hwm=<kB>`, from `/proc/self/smaps_rollup` and `/proc/self/status`, inside a `region probe { }` so the reading keeps nothing. The harness builds each tier once, runs it once, takes `(anon + swap)` at 40,000 minus at 20,000, × 1024 / 20,000, signed, and prints both readings and VmHWM's beside them. A reading the witness could not take (`?`) fails the row, never passes it. The three bounds are unchanged: the flat row < 4 B/turn, kept-`str` < 64, the control > 1,000. Each row now builds once and runs once per tier where it built twice and ran twice. `8a544687` changes comments only (the mechanism above, measured); no non-comment line differs from `17c2bd22`.

**Margins, both ways, on every rig** (the head's own witness, batch 2): the flat row reads **0.00 B/turn** in all 370 runs on both tiers (anonymous 196–204 kB at both readings), against the 4 B/turn bound: 78 kB of headroom over a reading that did not move by one page. Above the bound, a one-`str`-per-turn leak reads 32.8 (8× the bound) and #612's leak 1,298.84 (325×). Under the old statistic the same flat loop reached 9.4 B/turn, 2.3× over the bound.

### 3. The proof (batch 2, block A, the head `17c2bd22`, the plant `7923b85d`, trunk's row from the same clone)

`notes/s274-evidence/logs/proof-17c2bd22.log`, the rows `proof-rows-17c2bd22.log`:

| rig | head runs, red | head B/turn (both tiers) | plant runs, red | plant B/turn | trunk runs, red |
|---|---|---|---|---|---|
| load4 (CPUs 0–3 with the load) | 220, **0** | 0.00 every run | 20, **20** | 1,298.84 every run | 110, **28** (25 %) |
| rest6 (CPUs 0–5) | 50, 0 | 0.00 | 4, 4 | 1,298.84 | — |
| load6 | 50, 0 | 0.00 | 4, 4 | 1,298.84 | — |
| rest4 | 50, 0 | 0.00 | 4, 4 | 1,298.84 | — |

`S274_PROOF head red 0/370, plant red 32/32, trunk (load4) red 28/110`. The plant's row stops at its native leg (red), so its release leg is not reached by the row; the probe's `leak` witness reads release at 1,298.6 too (section 1). In-process VmHWM reads +0 kB in all 740 head legs.

Batch 3 repeats the gauntlet and the proof at `8a544687`, the final test file (job `proof3`, `/home/mfwolffe/lanes/s274/records/proof3.log`, exit marker `proof3.exit`); its result is in the PR body.

### 4. The siblings

Every row in `wolf_driver`'s tests that bounds a resident-set reading (grep for `VmHWM`, `VmRSS`, `smaps`, `ru_maxrss`; the other `/proc/self/status` readers read signal masks and CPU lists):

| row | reading | shape | bound | red on noise? |
|---|---|---|---|---|
| `region_copyout_lanes::a_loop_keeping_a_scalar_is_flat` | the witness's own VmHWM, two processes | 20k / 40k turns | < 4 B/turn (78 kB) | **yes**: #631, 23 % here; **fixed** (R-s274-1) |
| `region_copyout_lanes::the_loop_frees_its_scratch_keeping_only_its_result` | the same | the same | < 64 B/turn | no (the row: 26.0–39.5 in 80 runs; the probe 23.6–41.0) against 64; moved with the helper (R-s274-2): now 32.8 exactly |
| `region_copyout_lanes::the_control_grows_without_a_region` | the same | the same | > 1,000 B/turn | no (1,291.9–1,306.4 in 80 runs); moved with the helper: now 1,298.6 exactly |
| `cwd_region_lanes::a_cwd_asked_in_a_region_keeps_nothing_a_turn` | `wait4` `ru_maxrss` | 200k / 400k turns | ≤ 4 B/turn (integer) | no: 0 B a turn in all 40 runs (rest6, load4), every mode and tier: the floor below |
| `region_outer_write_lanes::the_turn_region_runs_flat_on_the_{native,release}_tier` | `ru_maxrss` | 100k / 200k turns | ≤ 4 B/turn; the control > 4 | no: region 0 B a turn in all 80 legs, the control 28–37 B a turn: the floor below |
| `map_pairs_walk_lanes::…_natively` | `ru_maxrss` | 2,000 / 4,000 entries | ≤ 1,024 B/entry, 64 MiB ceiling | no: 0 B an entry in all 80 legs: the floor below |
| `map_pairs_walk_lanes::…_on_the_checked_machine` | `ru_maxrss` of `wolf conform-run` | 500 / 1,000 entries | ≤ 16 KiB/entry | no (not run here; 8 MB of margin) |
| `checked_budget::the_reduction_runs_flat_on_the_checked_tier` | `ru_maxrss` | n 1,024 / 8,192 | +64 MiB, < 200 MiB | no (MiB margins) |
| `soak_residue::a_million_cycles_leave_resident_memory_flat` | VmRSS read from outside, one process | warm → end | 1–2 MiB per leg | no (one process already) |
| `parked_tasks::ten_thousand_parked_tasks_hold_and_release` | VmRSS per parked task | 100 / M tasks | < 128 KiB a task | no (~1 GiB margin) |

**Has any reddened?** CI: the failed jobs of the last 80 failed `ci.yml` runs and of every re-run attempt in the last 300 runs (90 runs, 190 failed jobs, 2026-10-20 to 11-01; `notes/s274-evidence/scripts/ci-scan.sh`, `logs/ci-scan-*.txt`): the scalar row twice (s250's PR run 39618268643 and trunk's push 41282097347, both 5 B/turn), and s273's map-walk plant twice (its deliberate 148 KB/entry); no other bounded-memory row. Every `*.log`/`*.txt` under hasu's and the Mac's `~/lanes`: the scalar row only (and s273's plant). So only the scalar row reddened, and only it is fixed.

**A finding about the `ru_maxrss` rows** (listed, not changed: R-s274-3). On Linux their two readings are **equal in every run**: 400 of 400 pairs (cwd_region's six readings per run, outer-write's region mode and the map walk, rest6 and load4, `S274_SIBLINE`), while the value itself moves between runs, 2.57–2.84 MB for cwd_region. A child's `ru_maxrss` starts from the high-water mark of the address space its exec replaced, which is the test harness's own (copied by fork, shared by vfork); batch 3's floor experiment (`floor-inner.sh`) tests that directly. So these rows cannot go red on noise, and they read 0 for any growth that keeps the child under the harness's own peak (~2.6–3.0 MB here): s273's 81 B/entry map walk (measured on macOS, where it reads) reads 0 here. Only outer-write's no-region control, at ~3 MB of growth, rises above the floor. The leaks they were written for were far above that floor (cwd: 32 B/turn × 200,000 = 6.4 MB; the map walk's 148 KB/entry), so each still caught its own bug, but their stated bounds (4 B/turn, 1 KiB/entry) promise more resolution than they have on Linux. The same method (the witness reads itself in one process) would give it; recommended as a follow-on with an issue to file.

### 5. The plant

`s274-plant-leak` = `17c2bd22` + #612's leak back (`7923b85d`: the scalar step's `copy region scratch { build(i).len }` undone to `build(i).len`), draft **#1005**. Red by name on hasu, 32 of 32: `region_copyout_lanes::a_loop_keeping_a_scalar_is_flat` run alone (`--exact`) exits 101 at its native leg, reading 1,298.84 B/turn every time (anonymous 25,492–25,500 kB at 20,000 turns → 50,860–50,868 kB at 40,000, +25,368 kB; VmHWM the same +25,368 kB), so its assertion is "copy region scalar loop grew 1298.8 B/turn (release=false), want flat (wolf-lang#612)". Batch 3 keeps one plant run whole (`S274_RAW plant:`) for the message itself. Its CI run, `tier pr` **41350548615**, read once after the push (in progress); the orchestrator watches it. The plant's instrument is the head's: `8a544687` differs from `17c2bd22` in comments only.

### The gauntlet

Scoped to a tests-only change (the contract: no full gauntlet; `tier pr` and the labelled run are the verdict), batch 2 at `17c2bd22`, block A, every step with its `END` line, stopping at the first nonzero (`notes/s274-evidence/scripts/hasu/proof-inner.sh`):

```
END trunk exit=0     (trunk's row and the load, built in the batch's clone)
END head exit=0      (the clone at 17c2bd22, clean)
END build exit=0     (wolf and libwolf_rt.a identical to trunk's)
END fmt exit=0       (cargo fmt --all --check)
END clippy exit=0    (cargo clippy -p wolf_driver --test region_copyout_lanes -- -D warnings)
END suite exit=0     (the whole region_copyout_lanes binary, lupin 0.1.58 required, 4 CPUs: 10 passed, 0 SKIP)
END plant exit=0     (the plant's test binary, from its commit)
END proof exit=0     (S274_PROOF head red 0/370, plant red 32/32, trunk (load4) red 28/110)
```

Batch 3 at `8a544687`: in the PR body. The final head adds only `notes/` after `8a544687` (this note and its logs), so no row can differ between them.

### Rig and records

Scripts: `notes/s274-evidence/scripts/hasu/` (`container.sh` holds both lock names with one long `flock -w 21600` each; `measure-inner.sh` batch 1; `proof-inner.sh` batch 2; `batch3-inner.sh` and `floor-inner.sh` batch 3; `probe.py` cuts the witnesses), `scripts/analyze.py` (every distribution above, `logs/analysis-*.txt`), `scripts/ci-scan.sh`. Logs: `logs/m1-trunk-7332772f.log.xz` (batch 1, whole), `logs/proof-17c2bd22.log` and `logs/proof-rows-17c2bd22.log` (batch 2). On hasu: `/home/mfwolffe/lanes/s274/records/` (`m1`, `proof`, `proof3`: `.log`, `.pid`, `.exit`).

## Done-when

Recommended rulings, each implemented:

- **R-s274-1 — the statistic.** Options from #631 and the contract, each judged on item 1's data:
  (A) the median of several runs per point: the two-process difference spans −176 … +184 kB, so a median of five per point still crosses the 78 kB bound now and then, at five times the runs;
  (B) more turns, still two processes: 200,000 → 400,000 turns divides the same ±180 kB to ±0.9 B/turn, with each run ten times longer and the leak's witness at ~520 MB;
  (C) a 16 B/turn bound: the worst flat reading was 9.4 B/turn, so the margin is under 2×, and a one-`str`-per-turn leak (32.8 B/turn) would sit only 2× over it;
  (D) the runtime's own accounting, `live_region_bytes()`: it counts named regions' chunks, not the process root where #612's leak lands, so the plant would read 0;
  (E) one process, two readings of what a leak grows, its anonymous memory (smaps_rollup `Anonymous` + `Swap`), at 20,000 and 40,000 turns; the bound stays 4 B/turn.
  **Recommend E** (implemented, `17c2bd22`; comments `8a544687`). Its margin, both ways: the flat loop reads 0.00 B/turn (370 runs, both tiers, four rigs) under a 4 B/turn bound; a one-`str` leak reads 32.8 (8× over) and #612's leak 1,298.84 (325× over). Either half alone would have fixed hasu (two processes' anonymous memory differs by 8 kB; one process's VmHWM does not move); together they do not rest on how a kernel counts file pages.
- **R-s274-2 — the file's two siblings** share `growth_per_turn`. (A) they move with it (implemented); (B) keep a second, two-process helper for them. **Recommend A**: one witness, one instrument, and the control still proves the instrument can see growth.
- **R-s274-3 — the `ru_maxrss` siblings** (`cwd_region_lanes`, `region_outer_write_lanes`, `map_pairs_walk_lanes`, `checked_budget`) never reddened, but on Linux they read a floor. (A) list them here and file a follow-on (recommended, nothing changed); (B) move them to the one-process reading in this lane. **Recommend A**: the contract's item 4 fixes only rows that reddened, and these four files have their own witnesses.
- **R-s274-4 — the plant's shape.** (A) the witness's `copy region` undone (#612's leak, implemented); (B) the runtime's region free made a no-op. **Recommend A**: s270b's `--dump=regions` shows the row's `scratch` promoted to the stack (create and free elided), so B never reaches this witness.
- **R-s274-5 — VmHWM.** (A) printed beside the reading, not asserted (implemented); (B) assert on in-process VmHWM. **Recommend A**: in one process VmHWM is flat on hasu too, but it counts file-backed pages, which a process maps in ahead of use; anonymous memory is what a leak grows.

Issues: wolf-lang#631 closes at the merge (the orchestrator's). #674 is already closed as its duplicate. **To file** (R-s274-3): "wolf_driver: the `ru_maxrss` bounded-memory rows (cwd_region_lanes, region_outer_write_lanes, map_pairs_walk_lanes, checked_budget) read the test harness's own peak on Linux; growth under ~2.6 MB reads 0", with section 4's numbers, the method (the witness reads its own smaps_rollup `Anonymous` in one process) and its file list. Nothing filed or closed here.

Test targets:

- [x] `cargo test -p wolf_driver --test region_copyout_lanes`: all 10 rows, lupin 0.1.58 required, 0 SKIP (hasu block A, 4 CPUs, at `17c2bd22`)
- [x] `a_loop_keeping_a_scalar_is_flat` at the head: 370 runs on four rigs, 0 red (220 under load on 4 CPUs)
- [x] the plant `s274-plant-leak`: the same row red 32 of 32 on hasu
- [x] trunk's row on the same rigs: red 118 of 510
- [x] `cargo fmt --all --check`; `cargo clippy -p wolf_driver --test region_copyout_lanes -- -D warnings` (rustc 1.97.1)
- [ ] batch 3 at `8a544687`, the final test file: the gauntlet and the proof again (job `proof3`, queued on block A at the hand-back)
- [ ] `tier pr` at the head (CI; the orchestrator's watch)
- [ ] the plant's `tier pr` 41350548615 red by name (CI; the orchestrator's watch)
