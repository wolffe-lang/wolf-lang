# s270 — the four wrong machines: #940, #950, #735 (#948 moved to s270b)

## Forbidden

No language surface, spec, runtime, CI, pin or loader change; `spec/` untouched and no ruling asked: every fix makes the native and release tiers agree with the checked machine and lupin on programs the spec already defines. No issue filed or closed, no label, merge, tag or release; nothing installed. No third-party source read: pack is the maintainer's own (read-only clone; `5567c24` = `0c0359a^`, `ed1a328` = `7fd0fae^`, pk00's probes from `62a49f0`). All Linux work ran on hasu block A (cores 0-5, `flock /home/mfwolffe/lanes/hasu-block-A.lock` taken per run, never held idle) in rootless podman `localhost/r41-toolchain:1`: foreign pid 1, the test command dropped to uid 1000, `--memory 12g --pids-limit 16384 --ulimit nproc=32768:32768`, the disposition-neutral taskset shim. Git and `gh` from the Mac worktree only. No attribution trailers.

**#948 is not in this branch.** It moved to lane s270b, on its own branch; nothing here touches it. The prediction file (`f8197af3`) still carries its #948 paragraph as written before the move.

**Who did what.** The first agent on s270 (the contract's GPT-6-Astra lane, stopped twice by a provider content filter) wrote the prediction (`f8197af3`), the #950 fix and row (`c379e82b`), the #735 fix and row (`81df7573`) and `crates/wolf_driver/tests/four_machine_regressions.rs`, and took pack baselines at `7fd0fae^` that came back green on every tier (Prediction says why that row cannot see the fault). The second agent (Claude Opus 5.5) took the branch over at `81df7573`, re-ran those rows on all four machines (green; and red at the base in the differ), reproduced and bisected #940, reduced it, fixed the pass, added its rows, formatted the inherited test file, ran the differ, opened the plants and ran the gauntlet.

**Footprint.** The contract says `module`, but lane-audit's wolf-lang path map names `crates/wolf_wir/` codegen, and all three fixes live there (`lower.rs`, `midend/interproc.rs`). The PR therefore declares `cross-cutting` (s268's precedent); the orchestrator owns the final-head `full-matrix` label and the green `tier full`.

Footprint: cross-cutting

## Inputs, verified

- Base `5adf3d91` (v0.2.36); branch `s270`.
- Compilers: the branch head and the base, built the same way (`cargo build --locked -p wolf_driver -p wolf_rt`, debug, `CARGO_INCREMENTAL=0`, Rust 1.97.1; the base from `git archive 5adf3d91`). wolf 0.2.35 from its release archive, sha256 `4cc4045bbbb605c07835bc4f32685953c784eeef4cd3139a48b457dfc90dad1a`, equal to pack's pin in `wolf-toolchain.toml`. lupin 0.1.58 is the fourth machine (`LUPIN`). Std: for the differ and the gauntlet, wolf-std at the dist pin `78b9d3cb` (`crates/wolf_driver/STD-PIN`), fresh clone; for pack, 0.2.35's bundled std (`2386522e`, pack's pin). The musl link bundle for the gauntlet is s267's (`dd09895a…`, copied from r41's inputs, equal by sha256 and by unpacked tree).
- The first agent's rows, re-run once at `81df7573` before anything was built on them: `four_machine_regressions` green on checked, native, release and lupin; the differ shows both rows red at the base (`enum_str_bare_choice` unsupported, `mixed_payload_display` no record, the ICE).
- README read, current: its one sentence this pass bears on ("The two tiers agree on every corpus program that runs") was false for the new rows before the fixes and holds after them.

## Prediction

Committed before any change (`f8197af3`, `notes/s270-prediction.txt`). Scored:

- **#950: hit.** The display lowering read each tag's payload straight off the union; projecting through the active tag fixed it (`c379e82b`).
- **#735: hit in kind, missed in place.** Not the enum join temporary: `zero_of` had no zero for a pointer slot, so the bare variant of an enum whose other variant carries a `str` was refused; `81df7573` gives the inactive slot a well-typed null. `zero_of` also feeds the killed-task teardown return (`kill_teardown_branch`), which leaves every frame without running user code, so that null is never read either; the differ moved no row for it.
- **#940: half.** "The unoptimized WIR remains correct" holds; "merges values across recursive calls" misses: recursion is not needed.

**#940, reproduced.** pk00's decoder probe placed in pack at `5567c24` (the recursive `feed`) answers `space: keys [7]` and `down: keys [7]` on `--release` with the mid-end on, on 0.2.35 and on the branch head alike; native and `WOLF_MIDEND=0` answer `[6]` and `[2]`. The contract's witness, pack's `--rows quit`, is green on every tier at `7fd0fae^` before any fix: the row types Down, Shift+Down, Space, `z`, then `q`, and a misread Down ends the game "by the quit key" with exit 0, which is all the row asserts. The `ctrl-c` row types the same keys then 0x03 and wants "ended by the interrupt", so it does see the fault: before the fix, release with the mid-end ends "by the quit key" there, at `7fd0fae^` (the loop-shaped `feed`) and at `0c0359a^` (the recursive one) alike, while `--selftest` passes. pack's switch to a loop never avoided the bug.

**Bisect, pass by pass.** A temporary skip knob (`scripts/s270c-knob.py`, never committed: applied to the hasu clone, the binary copied out, both files restored) skipped one mid-end pass or whole-program phase at a time: simplify, inline, memopt, rangeopt, sink, coalesce, licm, hdrprom, loopcse, dedup, the interprocedural range mint (step 2.5 of `optimize_whole_program`) and cross-cluster inlining. Only skipping the interprocedural mint makes the probe answer `[6]`/`[2]` and the reduced row print `6 7`. Skipping `rangeopt` alone does not, so the false fact is consumed downstream (the LLVM range channel), not made there.

**The transform.** The mint (s99, `midend/interproc.rs`) proves a range for container element loads from the meet of every store the program can attribute to that container. A `mut` parameter is a pointer to the caller's slot holding the list header (`Origin::MutSlot`). Handed straight on to another module function it was classified `ArgFlow::Opaque`, "no container", so the callee's writes never reached the caller. In the probe, `feed_all(mut d, bytes, now, mut keys)` relays `keys` to `feed`, which pushes every decoded key, and none of those pushes reached `show`. The only write left on `show`'s key list was `flush`'s `push(KEY_QUIT)`, so `show`'s element loads got `fact range 7..=7 : summary.…` (`940-probe-range-facts.txt`) and the release tier printed 7 for every key. In pack's live loop the same relay made every key the quit key, while `--selftest`, whose lists take another road, decoded the table right.

**Three more roads into the same channel, found while fixing it.** (a) A callee that replaces the list its `mut` parameter holds (`keys = fresh`): the caller kept reading its post-call reload of the slot as the list it had passed. Reduced as `corpus/memory/mut_param_replaced.lu`, release printed 7 where every other machine printed 6. (b) With relays carrying effects, pass 2 closes them one call hop per round for `WIDEN_PASSES` (3) rounds and then stopped, although its comment promised to "widen unknowns to Poison"; a chain of four or more relays would lose the writer again. (c) A `mut` slot reaching an indirect call or an unknown external was not killed. Function types cannot carry `mut` (E0206, `reduction/v8.lu.txt`), so (c) is defense in depth, pinned at the WIR level.

**What the reduction needed.** Three functions and `main`: a writer; a relay that takes `mut keys` and is multi-block (any loop or branch: a multi-block callee with a token parameter never inlines, the s42 v0 rule, so the relay's call survives into the whole-program phase); and a second writer pushing a constant. Not needed: recursion, the byte buffer, the decoder struct, a second module. With no constant writer the meet stays empty and nothing is minted (`v3`); a single-block relay inlines into its caller and the fault goes with it (`v5`). The row is `corpus/memory/mut_relay_push.lu`, written in pack's names.

**The fix** (`c7e83053`). A `mut` slot handed to a module call is `ArgFlow::Container(Param(p))`. A pointer stored into a `mut` slot kills that parameter's group (and the stored header's identity). An indirect or unknown call reaching a `mut` slot, or a stack slot holding a known header, kills it. Pass 2 runs its `WIDEN_PASSES` rounds, then widens whatever still moves to Poison until nothing moves: bounded by the number of entries, each widened at most once (D4: a count, never a clock). The widening is load-bearing: with it removed and the rest of the fix kept, the five-relay test reads `Some((7, 7))` (`940-green.log`).

## Evidence index

All under `notes/s270-evidence/`; host hasu block A, container `localhost/r41-toolchain:1`; recipes in `scripts/`.

- `940-probe.log`: the probe on the head and on 0.2.35, native / release / `WOLF_MIDEND=0`. `940-probe-range-facts.txt`: the minted `7..=7` fact on `show`'s element load and the mid-end's counters.
- `940-bisect.log`: the twelve skips, probe and reduced row each.
- `reduction/`: the eight variants (`.lu.txt`) and `variants.log`, every one on checked, native, release, `WOLF_MIDEND=0` and lupin. v1, v2, v4 and v6 miscompile; v3 and v5 do not; v7 is the replacement road; v8 is the E0206 refusal.
- `940-red.log`: the five new tests red before the fix (three WIR tests read `Some((7, 7))`; the two rows print `7\n7\n7\n` and `7\n` on `--release` after checked and native pass). `940-green.log`: the fix without the widening (the five-relay test still red), then the whole fix green: `interproc` 8/8, `four_machine_regressions` 4/4 with lupin required.
- `940-pack-pty.log`: pack at `7fd0fae^` and at `0c0359a^`, release with the mid-end on, `--rows quit,ctrl-c`. 0.2.35 and the unfixed head: `ctrl-c` FAILED, "ended by the quit key", `--selftest` ok. Fixed head: both rows ok.
- `differ/`: `cargo xtask differ` with impl A the head and impl B the base, each given the same tier by a logging proxy (`scripts/s270c-proxy.py`), plus a per-entry comparison (`summary.log`), at `0de8a1e0` (`a92d9314` after it adds blank lines to two rows and nothing else). 1,232 files, 1,170 entries per tier. Checked: 0 divergences, 0 moved. Native: 6 moved. Release: 8 moved. Every moved entry:

| Entry | Tier | Base → head | Why |
|---|---|---|---|
| `rows/mixed_payload_display.lu` | native, release | no record (ICE) → `exit(0)` `A(0)\nB(p)\n…` | #950, the new row |
| `typecheck/enum_str_bare_choice.lu` | native, release | unsupported → `exit(0)` `T.Text(x)\nT.Nothing\n` | #735, the new row |
| `memory/mut_relay_push.lu` | release | `7\n7\n7\n` → `6\n2\n7\n` | #940, the new row |
| `memory/mut_param_replaced.lu` | release | `7\n` → `6\n` | #940, the new row |
| `conc/atomic_ub_uaf.lu`, `memory/unsafe_ub_uaf.lu`, `memory/volatile_ub_uaf.lu` | native, release | exit code only | undefined freed-heap reads (their headers promise only `wir`); byte-identical binaries on head and base, exits scatter on both sides over five runs |
| `conc/atomic_race_plain.lu` | native, release | the count only | a data race on non-atomic memory, UB by its header (`[conc.mm.race.1]`, promises only that it compiles); byte-identical binaries, counts scatter on both sides |

  No other entry changes meaning; the replays are in `replays-and-plants.log`.
- `replays-and-plants.log`, plants: each plant branch checked out in the hasu clone and its tests run there.
  - #988 `s270-plant-940-relay` (`170812f9`): `interproc::a_relayed_mut_param_carries_the_callees_writes` and `a_mut_relay_chain_longer_than_the_widen_passes_still_closes` FAILED (`Some((7, 7))`); `four_machine_regressions::a_relayed_mut_list_keeps_its_callees_pushes` FAILED on `--release`.
  - #989 `s270-plant-950-payload` (`9e6141d7`): `four_machine_regressions::mixed_error_payload_display_projects_the_active_tag` FAILED, "--native hit an internal compiler error (exit 2) … lowered WIR failed verification".
  - #990 `s270-plant-735-zero` (`83e11e27`): `four_machine_regressions::bare_enum_variant_initializes_its_unused_string_slot` FAILED on `--native` (phase `mem`, the refusal).
- The gauntlet (`cargo xtask ci` at the final head, with `dist-rt` for musl, the pinned clef and ligature built first, every `*_REQUIRE` set as r41's rig sets them): see the PR body for its job, log and verdict.

## Done-when

Commits:
- `f8197af3`: prediction before the first change (first agent).
- `c379e82b`: #950, error display payloads projected through the active tag, row `corpus/rows/mixed_payload_display.lu` (first agent).
- `81df7573`: #735, inactive pointer-bearing enum payloads zeroed, row `corpus/typecheck/enum_str_bare_choice.lu` (first agent).
- `cc2249b4`: format the inherited four-machine test (`81df7573` was not fmt-clean).
- `c7e83053`: #940, `mut`-parameter writes carried through the interproc container channel, with three WIR-level tests.
- `0de8a1e0`: #940's two rows on all four machines.
- `a92d9314`: format the #950 and #735 corpus rows (blank lines only; `fmt-lu` refused both, `quick-gates.log`, `fmt-rows.log`).
- This note and its evidence.

Test checklist:
- [x] #940 reproduced on 0.2.35 and on the head; bisected to the interprocedural range mint; the transform named.
- [x] Five new #940 tests red before the fix, green after; the widening shown load-bearing.
- [x] pack's live loop through the pty: `ctrl-c` red before, `quit` and `ctrl-c` green after, at `7fd0fae^` and `0c0359a^`.
- [x] All four rows on checked, native, release and lupin (`four_machine_regressions`, lupin required).
- [x] fmt and all-target clippy (`-D warnings`) on `wolf_wir` and `wolf_driver`; `xtask corpus`, `fmt-lu`, `conformance`, `spec-extract --check`, `diag-catalog --check` before the gauntlet (`quick-gates.log`: `fmt-lu` caught the two inherited rows, fixed in `a92d9314`).
- [x] Differ, three tiers, head against base: every moved entry named above; the UB and race rows replayed.
- [x] Plants #988, #989, #990 red by name locally.
- [ ] Plants' PR runs, read once at the close.
- [ ] The gauntlet at the final head (PR body).
- [ ] The PR's CI run id, read once at open; lane-audit once.
- [ ] Orchestrator: final-head `full-matrix` label and green `tier full`.

Issue disposition (nothing closed here): #940 gets the fix, both rows and pack's live-loop witness green, so pack can drop `WOLF_MIDEND=0` (R-pk00-5) once a release carries `c7e83053`. #950 and #735 get their fixes and rows. #948 is s270b's.
