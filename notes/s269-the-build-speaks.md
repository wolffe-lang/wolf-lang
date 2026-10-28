# s269 — the build speaks

_Materialised from the body of wolf-lang#991 at the merge (`9ee6092b`)._

## Forbidden

No language, spec, runtime, optimization, CI, pin or gate changes; no package installation, issue changes, label, merge, tag or release. All Linux execution is in the lane's rootless container on kasumi CPUs8–11 under the shared per-run flock. No attribution trailers. The temporary baseline instrumentation was removed from compiler sources before the prediction commit; reproduction inputs remain evidence only.


R-s269-3 A: the reporter is driver-owned, but truthful live pass events touch wolf_wir, which the audit classifies as codegen. Hooks only observe begin/end; pass order, arguments and results are unchanged. The coordinator owns the final-head full-matrix label.

## Inputs, verified

- Measured baseline `5adf3d91` (v0.2.36); rebased prediction `efb1d5b0` still precedes implementation. Fresh yew trunk0ccb2c67661a2dd789b5ce86180d592370a79c31; its actual port entry is src/yew.lu. Both tiers use --no-cache --deny-warnings, not a headless reduction. Final branch is rebased once onto `43a12d3e`; verification and coverage separation are below.
- Bundled STD-PIN78b9d3cbc41d51db44d18c40b6694403644d1c83, fresh checkout, identity read twice. This is the compiler's bundled std, not yew's older archived toolchain pin. Measurements describe this exact rig, not a reproduction of an older CI41-minute timing.
- localhost/s267-gauntlet image96098506abd0395aecffbf52a0ec765b459ccf4fa150742fd2721fbe84597a28; Rust1.97.1, optimized measurement compiler with debug info disabled; Wolf native/release name compilation tiers. CARGO_INCREMENTAL=0, -j4, target inside checkout, lane under20GB. Foreign UID0 pid1, UID1000 runner, zero SigBlk/SigIgn, disposition-neutral CPU shim, pids-limit16384/nproc32768 and session-neutral launcher verified in notes/s269-evidence/preflight.log.
- Paired lupin0.1.58, pin a5378e3: release archive34f52792bb59bc7565e0edc1235cd47d9a6ad13d5e6528993627f2e699b62193 read twice from the API and hashed twice after download; executable47c646a61b9fd0dfe7d4565a74e22428a259eb54642f6d0d6ef3c1c0abe01ada hashed twice. Read-only real ligature90b25789b8523bea5f4ff5c7c85ba1dd2b5459d6450c113ac9583096fab62b57, clef0fde34eb9e7f6fecaabee17ed069eb009a7bfbfd157312efeeb604f76217cbc1 plus its census tree, and musl runtime3d6f821ae70ebb40d106f31c6a95ee46c0c3626d9bf230f43c1f0b8c7c26eb24/sysroot are staged for strict gauntlet rows; executable/archive hashes are distinct identities.

## Prediction

Complete pre-edit per-unit/per-phase tables, including every std module, codegen cluster and individual midend pass, are in notes/s269-evidence/prediction.txt; original rows in before-native.tsv and before-release.tsv. Source-unit/body elapsed contributions overlap when parallel; nested package rows are inclusive, not additional wall time. Unattributable global work stays package work rather than an invented per-module allocation.

Baseline package wall seconds:

| Phase | Native | Release |
|---|---:|---:|
| Load + resolve |0.295|0.294|
| Typecheck |0.306|0.307|
| Mem |1.433|1.418|
| WIR lowering |22.213|22.018|
| Midend |skipped|823.978|
| Codegen preparation |2.339|0.938|
| Codegen |26.917|45.277|
| Link |0.325|0.276|
| Total |53.828|894.506|

**Native is dominated by codegen26.917s, then WIR lowering22.213s. Release is dominated by the midend823.978s (92.1%); LICM342.403s (38.3% of total) narrowly exceeds hdrprom340.442s (38.1%). Together they consume682.845s (76.3%). Inline is103.256s; rangeopt including revisits is11.548s (1.29%).** This sample does not assign the15-minute build to the range pass or its versioner; #711/#983 remain open, not disproved for other versions/inputs.

The permanent --timings rerun measures native52.239s (codegen25.876,WIR21.551) and release893.084s (midend824.420; LICM344.573,hdrprom342.457,rangeopt11.464). A single pair is not a speedup claim. Both complete yew binaries are byte-identical before/after: native SHA256bd932dd38fa255f3ee97a429ef2ea214297d7d8ad3896db827ddfa05c6ef6bbc; release SHA256e6c9d2a016112d55b69bf97e5388b8b3ef5d57d581d5caa78fcdd6b1d7b6c2b4. Full printed tables and cmp results are after-native.log/after-release.log; s269-after-summary.json preserves aggregations.

**There are no two dominant yew functions/loops in these pass rows.** Each pass has4993 function rows. Largest LICM rows: edit.ed_run0.206623s/3 visits, flcli.repl_main0.150544s/3; largest hdrprom rows: edit.ed_run0.203480s/3, text.file_write_atomic0.140482s/3. Each top-two sum is only~0.10% of its pass total. The concrete next profiling seam is whole-module memopt::write_sets(m), called on every function visit at crates/wolf_wir/src/midend/licm.rs:87 and hdrprom.rs:83. These timers include that work and pass verification; they do not establish its internal share or identify a culpable source loop. No speculative optimization here.

R-s269-1 implemented:30s idle threshold,60s heartbeat cadence, documented WOLF_STATUS_AFTER seconds override. Compiler/tool output resets the initial deadline; phase changes do not. A fast nonTTY compilation adds zero bytes; a heartbeat-bearing compilation gets a completion line. TERM=dumb never receives controls. TTY updates fit a single line, reserving space for terminal-echoed ^C, and clear before diagnostics, at completion/refusal and on interrupt. Signal handling preserves inherited non-default dispositions.

R-s269-2 implemented: build/run/test compilation only; run/test execution and fmt/fix/doc do not carry a live compilation session. Quiet suppresses status/text timing output, not diagnostics/test results/program output; explicit JSON still writes its requested file. --timings prints scoped elapsed contributions on stderr; --timings=json:<file> writes documented wolf-build-timings/1 records with deterministic row order and per-function detail. No timing thresholds were added to gates.

## Evidence index

All short paths below are under notes/s269-evidence/.

- preflight.log:5 fake-clock cases,6 integration cases, existing exact-empty-stderr snapshot, all-target driver/span clippy -Dwarnings. Covers thresholds/cadence/output reset/final/quiet/dumb, JSON parsing/cache reuse/failure records/file-write errors/test JSON stdout, slow nonTTY WOLF_STATUS_AFTER=1 heartbeat + final, run reporting, and actual chatty-linker stream preservation/idle reset, including quiet.
- Real40-column controlling PTY success, refusal, TERM=dumb and SIGINT0x03 all pass in preflight.log. focus3.log retains the genuine wrapped-status failure:39 characters plus echoed ^C wrapped before cleanup. Reserving3 columns fixed it. preflight.log.take1 retains a separate harness race (PTY EOF before waitpid observes exit); bounded EOF polling fixed that, then the full focused runner passed. Neither failed attempt is called green.
- differ-checked.log:1228 files,0 divergences,752/752 run coverage. differ-native.log:3 divergences,757/757; differ-release.log:1 divergence,757/757, of1166 run entries. Native differences are atomic_ub_uaf exits134vs36, unsafe_ub_uaf152vs235, volatile_ub_uaf93vs154; release volatile_ub_uaf228vs16. Each source explicitly declares compiled use-after-free behavior undefined and promises only WIR. ub-replay.log verifies all six before/after compiled pairs byte-identical, identical checked ub(mem.ub)/P1 records, five native/release executions per compiler, and empty stdout/stderr throughout. The arbitrary executed-program exits are not compile errors. Real nonzero differ statuses remain disclosed; no row is silently excluded or compared only by totals.
- Heartbeat plant draft#986, head83056dde: existing outcome_surface::a_fine_program_passes_on_the_default_lane seen red at outcome_surface.rs107, exit101, plant-heartbeat.log, with premature heartbeat contaminating the exact-empty-stderr snapshot. Targeted lint-test dispatch40758136461 and PR40758110694 were read immediately in_progress; coordinator verifies CI's named row, no lane waiting.
- Live-line plant draft#987, head219b75f0: non_tty_never_receives_live_controls_even_with_immediate_heartbeat seen red at status_lanes.rs27, exit101, plant-live.log.take2 (CR assertion). Both plants are independent base-plus-break branches; the second adds its independent baseline-compatible gate, never reporter implementation. Initial local fmt blockage was corrected by a formatting-only plant commit, not accepted as intended evidence. New targeted dispatch40760452481 and PR40760423509 were read immediately in_progress; initial-head40758247695/40758233453 are not relied on. The coordinator verified the same deliberate break red in CI's cap_ffi rows; the local log is the named nonTTY gate. Draft #987 remains for coordinator cleanup at the merge. Corresponding fix gates are green in preflight.log.
- startup-baseline.log: the required Arch image still has #952's startup budget mismatch. Baseline and candidate release hello binaries, stdout and syscall-name sequences are identical; both wolf-minus-C=23, against budget12. No budget/runtime/gate changed. Recommended named baseline disposition if it is the sole final gauntlet failure; actual gauntlet status remains authoritative, not presumed green.
- **The single full cargo xtask ci at772bb1a3efebf890194dfab8e2b2b430eb87ba14 ended RED at test**, exit1: xtask readme::tests::shipped_readme_has_no_relative_links rejected the relative docs/cli.md link introduced by the README refresh. The first run's /home/mfwolffe/lanes/s269/evidence/gauntlet.log records fmt, fmt-fixpoint, clippy and rt-none completed before test; no step after test ran. Those four steps are unaffected by the one-line README-only correction. The absolute-URL fix is local commit e2b147473072badd0ef47f58c723c2446ff7af57; cargo test -p xtask --lib passes76/76 in the same container (/home/mfwolffe/lanes/s269/evidence/readme-lib.log).
- **The whole test step and every subsequent gauntlet step completed individually, in original order, at pre-rebase corrected head e2b147473072badd0ef47f58c723c2446ff7af57**, ending2026-10-28T21:55:55Z; no second full gauntlet was invoked. Actual continuation aggregate remains1: test,fuzz-check,deps-check,corpus,abi-check,debug-check,midend-rate,spec-extract,conformance,print-gate,diag-catalog,doc-catalog,fmt-lu,fmt-fuzz,audit-surface,dist-smoke,differ-self,lane-coverage all exited0; bench-gates alone exited1, only for startup58 minus C35 =23 against recorded12. IR-volume694 recorded/722 measured/28 new,0 unmeasured and0 regressed; all vector witnesses pass. This is the standing wolf-lang#952 Arch-container disposition, **never waived and not monolithic green**; the PR's Ubuntu CI decides. Each command's actual log/exit is under /home/mfwolffe/lanes/s269/evidence/continuation/; aggregate marker /home/mfwolffe/lanes/s269/evidence/continuation.exit, progress /home/mfwolffe/lanes/s269/evidence/continuation-container.log. Final source-recheck0 proves the tested files stayed unchanged.
- **Strict skips counted:** the complete corrected workspace test step emitted57 loud SKIP messages:54 unavailable nested-container cases,2 lldb cases (macOS CI owns those),1 macOS/aarch64 clang-drift witness. debug-check reran the same debug_native suite and repeated the2 lldb skips:59 emitted skip messages across the continuation, not59 distinct unavailable cases. No other step emitted SKIP. These remain explicit local coverage limitations for applicable CI hosts, not silently executed rows. Raw records are /home/mfwolffe/lanes/s269/evidence/continuation/test.log and /home/mfwolffe/lanes/s269/evidence/continuation/debug-check.log.
- **One conflict-free rebase onto43a12d3e** after wolf-lang#985: final head9ee6092bfb440023722f0f24884ff4ff2791098c. All53 lane-owned paths have identical git blob IDs and modes before/after rebase. The only old-head-to-new-head changes are upstream .github/workflows/ci.yml, .github/workflows/release.yml, ci/release-publish-test.py and notes/r41-release-id.txt. Local gauntlet plus continuation at e2b14747 covers this lane's unchanged files; final-head labelled CI covers #985's upstream lines. The held push was made once with an explicit force-with-lease. CI40777590137 was read once immediately, queued at9ee6092bfb440023722f0f24884ff4ff2791098c. Full-matrix remains on; the lane never labels or waits on CI.

## Done-when

Commits:
- `efb1d5b0`: measured prediction before implementation.
- `0b1b32b5`: observational events, driver reporter, options and tests.
- `07ffef4d`: help, CLI page, timing schema and Unreleased entry.
- `b61f1c22`: README-only refresh.
- `92e5dc15`: actual tool-output and bounded PTY exit-race regressions.
- `6778e5d4`: CLI prose correction.
- `fe205bcd`: full printed yew timings and focused evidence.
- `c317941a`: classified differential, independent plants and gauntlet runner.
- `9ee6092b`: README-only installed-link correction (pre-rebase e2b14747).

Test checklist:
- [x] Baseline tables both tiers; dominant phases named; all-unit table in prediction.
- [x] Permanent --timings yew tables; both emitted binaries byte-identical.
- [x] Fake-clock, nonTTY, JSON/cache/error and build/run/test stream rows.
- [x] Controlling PTY success/refusal/0x03/dumb, including narrow-terminal cleanup.
- [x] Checked/native/release differential, every differing row inspected and proven byte-identical UB nondeterminism.
- [x] Both independent plants red locally by name; targeted CI IDs read immediately, no wait/label.
- [x] README read and refreshed in its own commit; no install/version/count claim changed.
- [x] All local gauntlet steps covered by first run plus ordered continuation; actual local aggregate remains red only at unwaived Arch startup #952; strict57 workspace skips plus2 repeated debug-check skips counted above.
- [x] One conflict-free rebase onto43a12d3e; all53 lane-owned file blobs/modes unchanged; one held force-with-lease push.
- [x] Existing full-matrix label preserved at final head; no lane label action.
- [ ] Coordinator: green required tier at final head before merge; plant cleanup remains coordinator-owned.
- [x] Rebased-head CI read once immediately:40777590137, queued at9ee6092bfb440023722f0f24884ff4ff2791098c; no CI waiting.
- [x] The three lane-owned Linux target directories were pruned after all jobs exited; lane size8.1GB to999MB, sources/baseline/candidate binaries/raw evidence retained. The three pushed clean Mac worktrees were removed by explicit path, without force.
- [x] Audit run exactly once from the org directory. Exact last line: `AUDIT FAIL (footprint: cross-cutting)`. All source/footprint/ancestry/attribution/sections/sha/worktree checks pass; its only failures are CI40777590137 still in_progress and no completed green tier at this head. The audit was not rerun; merge awaits the coordinator's green required tier.

No issue filed or closed. Follow-on profiling recommendation: quantify repeated whole-module write_sets work inside LICM/hdrprom before changing those passes. The measurement instrument, not a guessed source loop or an optimization change, is this PR's answer.

