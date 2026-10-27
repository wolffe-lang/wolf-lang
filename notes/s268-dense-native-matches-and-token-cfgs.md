# s268 — Lower dense native matches and guard token CFG admission

_Materialised from the body of wolf-lang#982 at the merge (`7d1d9604`)._


## Forbidden

No language surface, spec, runtime, CI, pin, or loader changes. No package installation, issue creation/closure, label, merge, tag, or release. No third-party implementation source read: the yew sources are the project's owned witnesses; the IR and assembly are generated outputs. All Linux builds/runs use the lane's rootless container on kasumi CPUs4-7 under the shared flock. Git operations are on the Mac worktrees only. No attribution trailers.

The original contract said `module` for this codegen change; the coordinator corrected it to `cross-cutting` after the audit's path map was checked. The orchestrator owns the final-head full-matrix label and green tier-full merge gate.

## Inputs, verified

- Base `ed120cb9`; prediction/witness commit `31524103` precedes the first codegen edit. `bench/dispatch/{dense-match,token-leaf}/main.lu` are verbatim yw08 reductions. `bench/dispatch/run.sh` runs each tier three times with `--no-cache`.
- kasumi x86-64, CPUs4-7; image `localhost/s267-gauntlet:latest`, digest `96098506abd0395aecffbf52a0ec765b459ccf4fa150742fd2721fbe84597a28`; Rust1.97.1, clang23.1.3, GCC16.2.1. CARGO_INCREMENTAL=0. No external CARGO_TARGET_DIR for xtask ci. Foreign pid1, pids-limit16384, nproc32768, disposition-neutral taskset shim; SigBlk/SigIgn0 verified. See `notes/s268-evidence/environment.log` and scripts/.
- Saved baseline compiler SHA256 `436f98e9e3afa6c59d68c2134c7cdd52cd3a1979dbfd5f8072c5b9e49f881752`; runtime `0b65bd9d449c2780f47d268595777a6177b52aef6e640ddbef590a083f117827`. These are Rust debug builds; native/release below name the Wolf compilation tiers.
- Paired lupin0.1.56 executable `991d54cf28e4f182b339d9eeb4dba7b2df3ad2618acb3bb53bddca42856b4bb9`, read twice. Std pin2386522e. Matching staged musl runtime `3d6f821ae70ebb40d106f31c6a95ee46c0c3626d9bf230f43c1f0b8c7c26eb24` and its sysroot mounted read-only. Pinned ligature3f206413 executable `90b25789b8523bea5f4ff5c7c85ba1dd2b5459d6450c113ac9583096fab62b57` and clef executable `0fde34eb9e7f6fecaabee17ed069eb009a7bfbfd157312efeeb604f76217cbc1` plus its census tree, read-only; digests read twice. No tool installed.
- README read, current: no stated language capability, version, module count or installation claim moved.

## Prediction

The complete pre-edit prediction and all three baseline readings are committed in `bench/dispatch/prediction.txt`.

**R-s268-1: recommend and implement >=8 distinct cases, inclusive span <=4 times case count.** The native backend recognizes pure integer equality chains on one SSA value, then emits an unsigned i64 bounds check before narrowing to br_table's index. Holes/out-of-range values reach the original default; duplicate literals retain the first arm. Narrow-width constants are canonicalized to signed i64 for this density calculation; unsigned windows crossing that sign boundary may conservatively retain a chain. Bypassed comparisons cannot escape their block; argument-carrying edges and effects keep the ordinary branch path. Integer literals needed by another block are rematerialized at function entry in deterministic order, so other predecessors remain valid. WIR and release codegen are unchanged.

Dense witness, ns/op, arm order **0 / 30 / 59**; each cell is the witness's best of ten, three independent executions:

| Tier | Before runs1/2/3 | After runs1/2/3 |
|---|---|---|
| Native | 1.673/4.322/7.267; 1.391/4.293/7.159; 1.374/4.319/7.282 | 1.732/1.924/1.700; 1.700/1.889/1.701; 1.701/1.888/1.702 |
| Release | 1.374/1.571/1.543; 1.357/1.543/1.535; 1.353/1.525/1.542 | 1.372/1.510/1.511; 1.322/1.510/1.510; 1.322/1.511/1.510 |

All sinks are1000000/31000000/60000000. Native median arm59/arm0 is1.000, meeting <=1.3. Arm59 improves ~76.6%; **arm0 costs ~22.3% more**, the table's overhead versus the formerly first comparison. Release stays flat. A later post-dist functional rerun of the committed runner is also retained (`unlocked.log`), not substituted for this first valid after set: native0/30/59 medians2.101/1.888/2.078ns, release1.322/1.133/1.133ns; the position-dependence bar still passes. Absolute timing/layout variation is not a correctness claim.

Two measurement misses are explicit. First, development compilers sharing +dev.unknown reused cached objects; those readings were discarded, and the permanent runner now forces --no-cache. Second, the conservative recognizer initially left arm0 outside the table because yew's default reuses literal1 from the next test. Entry rematerialization fixed that, and a permanent gate requires the complete60-entry table, not a59-entry suffix. No timing threshold gates correctness.

**R-s268-2: recommend and retain zero multi-block token admissions until token finals are generalized.** The small witness's calls/inline ratio per1000 is native1274/1293/1275 and release1020/1025/1027 before; native1395/1273/1272 and release1070/1020/1019 after, sink1289 throughout. The production inliner is unchanged. Its decide guard is not merely a missed size heuristic: splice scans only entry instructions to reconstruct token finals and its multi-return join omits token results. The dedicated single-return plant produces TokenOrder (entry token read after consumption); the multi-return plant hits the2!=1 assertion. A proposed positive size bound would admit an unsound transformation, so none is adopted.

**Large-loop diagnosis (#978), with limits.** The exact yw08 hand-inlining recipe reproduces fib27: control medians111.518/110.169/107.667ms versus inline207.856/209.117/212.770ms, all asserted answers correct. This is a source-expansion experiment, not a compiler change in this PR.

| fl_vm_exec measure | Control | Hand-inlined |
|---|---:|---:|
| WIR instructions / blocks | 4747 /1417 | 6362 /1858 |
| Emitted pre-LLVM-optimization branches |1487|1850|
| Emitted pre-optimization trap calls |197|385|
| Final machine body bytes |22789|30045|
| Static disassembled instructions |4129|5549|
| Final trap-call sites |28|72|
| Stack reservation bytes |8824|8264|
| Static stack references |1014|1004|
| External tail jumps |0|0|

The helper calls really disappear from the final VM body, while checked-control-flow and body size grow substantially. Both bodies retain three bounded indirect table dispatches (their index/bounds/address sequences were inspected). **It is not a lost tail call. A larger stack frame or more static stack references is not the explanation either.** These counts establish expansion, not the dynamic cause of the full~1.9x cost: the container cannot enable perf cycles/instructions/cache events, and static counts cannot distinguish instruction-cache pressure from a different hot-path spill/branch mix. No claim that a particular cache threshold was crossed, and no speculative fix. Follow-on for #978: isolate read-helper versus stack-helper expansion and obtain dynamic hot-block/spill/cache measurements on an authorized counter-capable rig before changing the inlining budget.

The compiler's13-kernel suite was also measured: ten paired samples plus warmup on each of native/release, same release runtime, same manifest sweep counts, exact sinks. **All26 before/after binaries are byte-identical; zero regressions exceed max(2%,3×beforeMAD,3×afterMAD).** Full samples, medians and MADs are retained, rather than interpreting host drift as an optimization.

## Evidence index

All paths below are under `notes/s268-evidence/` unless named otherwise.

- `focused-rematerialized.log`: accepted after timings and execution checks; `quick.log`: final8 backend gates,2 token gates, three-tier source gate, all-target clippy -Dwarnings. Native execution includes every i8 input plus i16/i32/i64 boundaries, negative literals, holes, defaults and duplicates; source conformance covers checked/native/release.
- `kernels/{samples.jsonl,summary.json}`: all26 paired kernel distributions and byte-identity assertions. `scripts/kernels.py` reproduces them. These are before/after compiler comparisons, not a new C-oracle M2 claim.
- `yew-measurement.log`, `yew-ir.log`, `yew-machine.log`, `control-vm.asm`, `inline-vm.asm`, `yew/*-codegen.txt`, `yew/*-symbols.txt`, `yew/*-perf.txt`: VM timings, IR and final-machine attribution, including the counter limitation. The scripts retain the exact hand-inlining recipe. Full IR remains at kasumi `/home/mfwolffe/lanes/s268/evidence/yew/{control,inline}.ll`.
- `differ-{checked,native,release}.log`:1228 corpus files compared. Checked0 divergences; native3; release1. Run-rung execution is752/752 checked and757/757 on each compiled tier, of1166 run entries; unsupported ledger counts are unchanged.
- `uaf-replay.log`: every divergence inspected, replayed five times per compiler and compiled tier, and compared byte-for-byte. All stdout/stderr bytes are empty. The checked records are identical ub(mem.ub), rowP1.

| Divergent row | Original after vs before | Classification and replay proof |
|---|---|---|
| conc/atomic_ub_uaf.lu, native |Exit174 vs247|Pre-existing undefined freed-heap read; identical native binary SHA186c9456…; before exits240/116/102/199/182, after122/233/167/147/233. Release binaries also identical, consistently exit0.|
| memory/unsafe_ub_uaf.lu, native |Exit206 vs153|Pre-existing undefined freed-heap read; identical native binary SHAae5a5a0c…; before130/203/210/232/106, after254/143/245/222/181. Release binaries also identical, consistently exit0.|
| memory/volatile_ub_uaf.lu, native |Exit27 vs245|Pre-existing undefined freed-heap read; identical native binary SHAa5cb6974…; before200/123/209/157/197, after27/191/203/18/59.|
| memory/volatile_ub_uaf.lu, release |Exit138 vs171|Same pre-existing UB; identical release binary SHAed446210…; before2/242/51/71/191, after37/222/120/171/45. Here2 is the executed program's arbitrary exit, not a compiler failure.|

No row changes meaning because of this pass. The three source files themselves declare compiled behavior undefined and promise only the WIR phase; they are not silently excluded from the differ, whose nonzero statuses remain visible.

- Dense plant draft#979, head1f6eae14: local exit101 at dense_integer_chain_emits_one_bounded_table. CI40441766938 fails the same test and three related table gates in lint+test+corpus. `plant-chain.log` and `plant-chain-ci.log`.
- Token plant draft#980, headda2c3b09: local exit101 at both dedicated token-final gates. CI40441801083 fails earlier at existing aliased_stencil_takes_the_slow_loop with FactJust (guard provenance no longer tied to compared bases), before reaching the new tests. `plant-token.log` and `plant-token-ci.log`. Both CI runs were read completed, without waiting or dispatching another run.
- `gauntlet.log`: take1 failed at strict real-ligature provisioning, not codegen. `gauntlet2.log`: take2 supplied real binaries but omitted clef's sibling census tree. Take3 completed all six real-linker cells but two pinned parked_tasks cases could not spawn: PermissionDenied, not a timeout or scheduling assertion. The lane's later rsync reset the live bind-mounted taskset script to0644 after its initial chmod. The launcher now freezes a separate executable snapshot outside the synchronized scripts directory. These three rig failures are retained, not called green or attributed to #570. The repaired same-pin parked_tasks run passes4/4 (`parked-repair.log`). A subsequent gates run exposed the differ-only WOLF_STD leaking into the corpus phase ledger; unsetting it restored both intentional no-std refusals, without changing corpus or compiler files.
- **Local gauntlet green except #952's startup-call budget on the Arch image, identical at the baseline.** The corrected gates shard passes corpus, ABI, debug and midend checks; bench-gates passes all IR-volume/vectorization checks (694 recorded/722 measured paths,0 regressed), but reports wolf58 minus C35 =23 against budget12. The saved baseline and candidate emit byte-identical release hello binaries, SHA256 `4a89ca1b688519397a3bb3ee8b026123ef1f3d82547129e886af7872d86bde10`, identical stdout bytes `68656c6c6f2c20776f6c660a`, and identical58-call syscall-name sequences; both are23 over the same C twin. `startup-baseline.log` and `startup-traces/` prove it. The trace uses /dev/null stdout exactly like the gate; an initial piped-stdout probe omitted C's extra ioctl and was corrected, not used as the gate comparison. Raw address arguments naturally vary under ASLR. The coordinator approved this named baseline disposition; no budget, runtime or CI change.
- All remaining steps ran separately and exited0: workspace tests; fuzz-check; deps-check; spec-extract; conformance; print-gate; diag-catalog; doc-catalog; fmt-lu; fmt-fuzz; audit-surface; dist-smoke; differ-self; lane-coverage. `remaining-job.log` records each command's exit; `remaining/` contains every full log and marker. The workspace rerun filters only the expensive real-ligature test already proven green in all six cells in take3; it is not untested. There is no claim that a monolithic cargo xtask ci invocation returned0: its real nonzero logs are retained and the constituent steps above complete its coverage.
- The workspace emitted59 loud SKIP lines:54 unavailable nested-container rows, two release-profile witnesses not yet built, and three platform/tool rows (one macOS/aarch64 clang-drift witness and two lldb rows). After dist-smoke unlocked the release compiler, scope_handle_param was rerun4/4 with both compiled tiers observed (`unlocked.log`), clearing those two release-profile skips. The54 nested-container and3 platform/tool skips remain explicit local limitations for CI, not silent passes.
- `verified-source.sha256` pins the executable-source files tested; the later commits add evidence only. `scripts/` preserves the container, CPU shim, witness, differ, replay and measurement recipes.

## Done-when

Commits:
- `31524103`: commit prediction and reproduced witnesses before codegen edits.
- `c9016b60`: bounded dense native dispatch and correctness gates.
- `b0b6c72a`: pin conservative token-CFG admission with two safety gates.
- `cd60d930`: force cache-independent witness measurements.
- `1161326a`: measured evidence and reproduction scripts.
- `7d1d9604`: completed gauntlet constituents, repaired rig, unlocked release witnesses, and byte-identical #952 baseline proof.

Test checklist:
- [x] Dense shape, boundary, fallback and machine-execution gates.
- [x] Checked/native/release source conformance gate.
- [x] Token-final single/multi-return safety gates, both seen red on the plant (`notes/s268-evidence/plant-token.log`; CI run40441801083 independently fails the earlier stencil gate).
- [x] All-target clippy with warnings denied for changed compiler crates.
- [x] Three-tier corpus differ; each compiled-tier divergence inspected and proven byte-identical UB nondeterminism.
- [x] All13 compiler kernels, both tiers,10 paired samples and exact sinks.
- [x] Dense position-dependence bar; VM slowdown reproduced, attribution limitations named.
- [x] Both draft plants red locally and in CI by name.
- [x] Every local gauntlet step covered; only #952's baseline-identical Arch startup budget remains red, as disclosed above.
- [x] Final-head CI read once immediately after opening: run40454300636, in_progress at head7d1d96047cee4ea02ec33a3a07d4989cc9d6f735; no waiting.
- [x] Audit run once with the explicit worktree. Actual last line: `AUDIT FAIL (footprint: cross-cutting)`. It found pending CI/no green tier, missing orchestrator-owned full-matrix label, the still-present worktree, and a checklist red-claim citation missing in its own paragraph. That citation is corrected above after the single audit; the audit was not rerun. After confirming them clean and pushed, all three Mac worktrees were removed without force. The three lane-owned Linux target directories were pruned; source, raw evidence, baseline binaries and the tested compiler at `/home/mfwolffe/lanes/s268/candidate/wolf` remain.
- [ ] Orchestrator: final-head full-matrix label and green tier full before merge.

Issue disposition: #977 receives the native dense-dispatch fix; #978 receives the conservative admission answer, regression proof, measurements and bounded attribution, but remains open for token-final generalization and dynamic large-loop attribution. Nothing closed here. README current. The final hand-back supplies the exact head, run ID and audit's actual last line; no merge-readiness claim precedes those results.

