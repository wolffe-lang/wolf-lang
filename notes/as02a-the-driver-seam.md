# as02a — the assembler driver seam (wolf-lang#957, merged at `9f2cff44`)

This note is the PR body of wolf-lang#957, materialised at the merge by the orchestrator (the lane's wrapper could not write Markdown). The evidence files are under `notes/as02a/`.

Footprint: cross-cutting

## Prediction
Two driver call sites; x86-64 gets `--64`, not every 64-bit architecture; 16 new named tests. The two call sites and protocol prediction held. Linux now has 16 integration rows plus one selection unit row (the extra is stale-output rejection); Windows additionally compiles a named hosted-assembly refusal row.

## Forbidden
No other assembler/disassembler/opcode-database source was read. GNU as is a black-box test tool. No PAX source was changed, no default was flipped, no release/merge/label was performed. The linker and release clang IR code-generation path are unchanged. The checked machine still refuses assembly calls by name; the exported-symbol roster remains text-derived from `.globl`/`.global`, not macro expansion or included files. The witness rosters are `kw_seven`, the second listed source’s `another`, and the freestanding stub’s `wolf_trap`, `kw_outb`, `kw_big`.

## Inputs, verified
- wolf-lang base `e65b89e5`; clef `996a3ccdf160923757c4ce6aa474efdffa2135b3`.
- Clef built inside the kasumi container using wolf 0.2.35 archive SHA256 `4cc4045bbbb605c07835bc4f32685953c784eeef4cd3139a48b457dfc90dad1a`, read twice by clef's pinned fetcher. Lupin 0.1.56 archive `ad703550ac4787e8e4bb26989d591a731410274d7332f2b4e8b9a64c878822cc`, read twice.
- Built clef SHA256 `6ce3230dd4f7352f3251cff6dcf79308954031129a0fc28e59d635a5710750e0`, read twice.
- R-as02-1 recommendation A implemented: CLI > nonempty WOLF_AS > legacy driver; failure is final. R-as02-4 recommendation A implemented: --64 is x86-64-specific. These are recommendations, not lane rulings.

## Evidence index
- `07c5db4c`: pre-change prediction in `notes/as02a/prediction.txt`.
- `c822acc4`: selection, output verification, refusal tests and pinned real-clef CI setup.
- `4a6fd361`: spec, command help and Unreleased entry.
- `802ce0af`: README refresh; README read against the changed surface.
- `917a3cc3`: all 43 pinned x86-64 corpus sources through root manifests, both tiers.
- `41d00072`: no-op tools cannot reuse stale output; quoted executable path witness.
- `171c8698`: clippy ordering correction.
- `2c686a99`: place the clef builder under classified `ci/`.
- `a99c25d5`: fix the absolute-link requirement caught by `readme::tests::shipped_readme_has_no_relative_links`.
- `9f2cff44`: bank strict logs, measurements and the bounded container recipe under `notes/as02a/`. Final head `9f2cff44c945697903e52ea7e5fdf05539b207ae`.
- kasumi `/home/mfwolffe/lanes/as02a/evidence/check2.log`: fmt, clippy, 16 strict integration rows, one unit selection row, six existing assembly-boundary rows and CI planner tests all exit 0. `/home/mfwolffe/lanes/as02a/evidence/seam3.log` records 43/43 corpus sources on native and release, real-clef fresh package execution returning 7, and checked refusal `a call into assembly kw_seven`.
- Failed-tool control: draft #955, head `a494b442`, partial dispatch `40174784840`, target row `linux::failed_selected_assembler_is_final`.
- Foreign-output control: draft #956, head `3229792a`, partial dispatch `40174790345`, target row `linux::coff_selected_output_is_refused`.
- Each dispatch was read once immediately: both `in_progress`. CI redness is not yet claimed; the orchestrator must retain the named red rows and close the drafts.
- The first full-debug attempt exceeded the 10 GB lane budget and was stopped; its target was pruned. The rerun disables Rust dev/test debug information, keeps `CARGO_INCREMENTAL=0`, and occupied 8.2 GB. CPUs 4-7, memory 12 GB, pids 16384, nproc 32768, core limit 0, foreign pid 1, no host PID namespace.
- The first complete workspace test stage stopped on the README relative link (retained in `notes/as02a/readme-red.log`); `c332ca05` fixes it. All eleven post-benchmark commands at that run returned 0: spec-extract, conformance, print-gate, diag-catalog, doc-catalog, fmt-lu, fmt-fuzz, audit-surface, dist-smoke, differ-self, lane-coverage. The overall run remains red, not waived.
- A complete implementation-head rerun is now active at `76183685`: kasumi `/home/mfwolffe/lanes/as02a/evidence/final.pid`, `final.log`, `final.exit`; detailed `gauntlet.log`/`gauntlet.exit`, followed by every post-bench step in `after-bench/` and each exit in `final.log`. The final marker does not exist until completion. This is a long-job handoff, not a green gauntlet claim. While it ran, trunk advanced by notes-only `568dfda4`; the branch was rebased to `9f2cff44`. `git diff --name-only 76183685 9f2cff44` lists only eight `notes/s266*` paths: no compiler, tests, spec, CLI, README, CI or assembler-builder change. The running gauntlet therefore covers identical executable inputs; its log correctly retains its original head.
- Startup row 23 is the known wolf-lang#952 baseline failure, not waived. The final rerun must establish its actual reading and every other step must be exit 0.

## Done-when
- [x] Selection, path quoting, absent/nonzero tools, malformed/COFF/Mach-O/wrong-machine/class outputs, and stale output tested.
- [x] Real clef builds all 43 x86-64 census sources through root manifests on both tiers; fresh packages link and return 7 on both tiers.
- [x] Checked assembly remains unsupported; freestanding unknown exports remain E1306. Foreign target names are refused. Hosted Windows has a dedicated refusal test (execution awaits Windows CI). Clef's AArch64 Mach-O census row remains explicitly outside scope.
- [x] Spec, CLI help, CHANGELOG and README updated.
- [x] All eleven post-bench steps exit 0 on the first run; README red retained and corrected.
- [ ] Complete source-identical gauntlet and post-bench rerun, naming #952 without waiver.
- [ ] Both intentional controls observed red by the orchestrator.
- [ ] Audit and final-head PR/full CI green; orchestrator adds the label, watches, merges and closes #937.

The five-section report is carried here; a Markdown lane note was not written under this session's report-file restriction. No new issue is requested at this point. No labels added by the lane.

Final-head README repair check: all four `readme::tests` pass, exit 0, in kasumi `evidence/final.log` before the complete gauntlet. Historical head IDs inside retained logs and measurements identify the exact original runs, not the rebased implementation commits above.

Final handoff: CI run `40176744880` was read once as `pending` at `9f2cff44`. `bash wolf/tools/lane-audit.sh wolf-lang as02a 957 /Users/mfwolffe/lanes/as02a/wolf-lang` ended `AUDIT FAIL (footprint: cross-cutting)`: only pending/no-green-tier CI and the intentionally absent full-matrix label remain. Fast-forward ancestry, report sections, citations, clean trailer policy and pruning passed; the expected spec change is a WARN. The Mac worktree has been removed. The active kasumi job PID is `2333055`; retain its logs and inspect `evidence/final.exit`, the bench reading and every subsequent exit before claiming completion. No merge readiness is claimed.

