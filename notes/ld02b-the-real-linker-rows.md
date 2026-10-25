# ld02b — the real-linker rows and the same-head runtime

Materialised from wolf-lang#961 (head be461db0, merged 2026-10-25) at the merge; the evidence sits in `notes/ld02b/`.

Footprint: cross-cutting

## Prediction
One shared native/release `compile_native -> link_objects` call, then Unix/Windows platform implementations; predicted 16 new rows and no CC compatibility flag. The call-site and compatibility predictions held. There are 16 Linux integration rows plus one selection unit row; non-Linux has a named unsupported-host refusal row. Root-manifest fixtures cover exit, printed output and linked assembly on both tiers.

## Forbidden
No external linker's, libc's, kernel's or shell's source was read. Ligature's own source is the permitted input. GNU tools are black-box controls. No PAX or ligature changes, platform default changes, merge, tag, release, label, issue action or Mac build. `cargo fmt` alone ran on the Mac. Windows, non-Linux explicit linking and foreign targets remain refused by name; the release clang IR-to-object path is unchanged.

## Inputs, verified
- wolf-lang base `253d80ab`; ligature `3f206413`, built inside the isolated kasumi container with complete wolf 0.2.35 and bundled std `2386522e`.
- Wolf archive SHA256 `4cc4045bbbb605c07835bc4f32685953c784eeef4cd3139a48b457dfc90dad1a`; lupin archive `ad703550ac4787e8e4bb26989d591a731410274d7332f2b4e8b9a64c878822cc`, each read twice in `notes/ld02b/archive-digests.log`.
- Built ligature SHA256 `90b25789b8523bea5f4ff5c7c85ba1dd2b5459d6450c113ac9583096fab62b57`; matching ld01 musl runtime `9d859b9e6b5319f5a6f609e73ba0d71981d2dfa489882648ac9f454cd9db739c`, both read twice; sysroot hashes are retained in `notes/ld02b/seam-superseded.log`.
- R-ld02-2 recommendation A implemented: CLI `--linker` > nonempty `WOLF_LD` > unchanged platform path; selected failure is final. R-ld02-4 recommendation A implemented: explicit cc-shaped selection is native-Linux-only; retain macOS signing/debug behavior and the distinct Windows `WOLF_LINKER` COFF path unchanged. These are recommendations, not lane rulings.

## Evidence index
- `be461db0`: fixes the stale-runtime input behind run `40215921361` (`__wolf_rt_main_start` undefined): real links now share this head's `cargo xtask dist-rt` runtime with static-entry, reuse `ci/fetch-entry-bundle` for verified musl members, and print both digests twice; seven input/wiring tests, corrupt-cache refusal, planner tests, fmt and macOS linker-selection rows (3/3) pass. Linux real-link validation remains CI-owned.
- Continuation `ce8c05d5`: rebased onto trunk `c1aca5f3`, retaining s267 CI changes; ported the #966 hollow-cache guard to `ci/build-test-ligature` (default re-clone, caller-named tree refusal); local fmt, macOS `linker_select` (3/3), six cache-guard fixtures and CI planner tests pass.
- `834cf3e1`: pre-change prediction.
- `67b84d4c`: selection, process argv/errors/exits, no-fallback and real-tool witnesses.
- `ef422453`: spec, CLI help, Unreleased entry and dedicated CI refusal step.
- `d0038cd0`: selected links bypass lld discovery; release selection witness.
- `0152e7e8`: README-only refresh; README read against changed commands and build/install claims.
- `17ea51a0`: require host ELF64 little-endian executable/PIE with file-backed executable entry; reject stale, malformed, foreign and relocatable output.
- `e266c0f9`: independently pinned real-linker builder and fixture preflight.
- `fceea2f7`: root assembly object order and trap aliases on both tiers.
- `d90f975a`: release module name assertion (`00-c00.o`, not native `00-root.o`). First red retained in `notes/ld02b/check-first-red.log`.
- `c4f0ba42`: evidence and bounded container/gauntlet scripts under `notes/ld02b/`.

`notes/ld02b/check.log` at `d90f975a`: fmt, workspace/all-targets clippy, 15 process integration rows, one pure selection row and the CI planner tests all exit 0. The expensive real-ligature row was explicitly filtered from that fast pass; it is strict in the full gauntlet. An earlier source-head run passed 13 process rows and the first real ligature exit/native build, own-reader verification and execution, then was stopped (exit 143) because its source was superseded. No complete real-tool green is claimed from that run.

Failed-tool plant: draft #959, head `ba8281d6`, partial dispatch `40181908874`, `lint-test / explicit linker selection and refusals`, expected row `linux::failed_selected_linker_is_final`. Foreign-output plant: draft #960, head `0114fa49`, partial dispatch `40181915607`, same job/step, expected row `linux::foreign_selected_output_is_refused`. The production guard is broken in each branch, not the fixture. Both runs were read once as `in_progress`; the orchestrator must retain their actual red rows and close the drafts. No CI-red claim yet.

The full kasumi gauntlet is running at executable-source head `d90f975a` (the final commit adds only `notes/ld02b/` evidence). PID `2839291`; `/home/mfwolffe/lanes/ld02b/evidence/final.pid`, `final.log`, `final.exit`; details in `gauntlet.log` and `gauntlet.exit`. All eleven post-bench commands run individually afterwards, with every exit printed in `final.log` and logs under `evidence/after-bench/`. `final.exit` is absent until completion. Known baseline wolf-lang#952 is the startup row 23 on this Arch userland, named, never waived; this run must establish its own reading and every other step must be exit 0.

Container: CPUs 4–7 under the shared flock; pids 16384, nproc 32768, memory 12 GB, core limit 0, foreign pid 1, isolated PID namespace, no host PID namespace, incremental off, Rust dev/test debug info off. Lane size was 1.8 GB at launch. The lock is released between gauntlet and post-bench containers. Real ligature and clef were built inside the container, not on the host.

## Done-when
- [x] CLI/environment/default precedence, space/quote paths, absent and exit-37 tools, no cc fallback, malformed/foreign/class/kind/entry/stale outputs.
- [x] Default and empty-environment CC compatibility; native/release/run paths; object-only build does not link; assembly objects and trap aliases retain order.
- [x] Spec, CLI help, CHANGELOG, README and CI protocol/refusal gate.
- [x] Fast local checks at final executable source.
- [ ] Complete strict real-ligature suite: all three root-manifest programs, native and release, own-reader verification and stdout/stderr/status. Running inside the full gauntlet, not yet green.
- [ ] Complete `cargo xtask ci`, name #952 without waiver, and obtain exit 0 from each of the eleven post-bench commands.
- [ ] Both CI plants observed red by the orchestrator; final-head PR/full CI and audit accepted. No label added by this lane.

The matching musl bundle is now public (ligature#13): release `link-bundle-2026-10-25`, SHA256 `6f99a2794da54b2dd85f06713d736b27862a7cd0da9e28bd940d975a4603a1d1`. CI caches the archive by digest, verifies cached and downloaded bytes twice, refuses mismatch, and includes a deliberately corrupt-cache refusal witness. CI builds pinned ligature and supplies the runtime/sysroot to the full gauntlet. The real row no longer skips when its tool is absent: missing provisioning is a test failure. Its six expensive links execute once in the full gauntlet rather than twice; the earlier protocol-only step explicitly filters that row. Linux job budget is 150 minutes for those links plus the existing gauntlet. Hosted Windows/non-Linux refusal execution awaits those CI platforms. No new issue is filed; ligature#13 records the published bundle; ligature#12 tracks linker performance. The note's five sections are carried in this PR body under the session's report-file restriction; no wrapper refusal is claimed.

Prune only after evidence review: exact Mac plant worktrees `/Users/mfwolffe/lanes/ld02b/plant-failed` and `/Users/mfwolffe/lanes/ld02b/plant-foreign`; after the local job finishes, generated kasumi `/home/mfwolffe/lanes/ld02b/wolf-lang/target`, `/home/mfwolffe/lanes/ld02b/scratch`, `/home/mfwolffe/lanes/ld02b/cargo-home` and built tool targets. Retain evidence, scripts, input pins and the main worktrees until merge. Never prune another lane's paths; use `podman unshare` for container-owned generated files.

Final handoff: head `c4f0ba42ba92887cbe39803a9371497251290f84`, PR-tier run `40182759083` first read as in progress by the audit. The audit ended `AUDIT FAIL (footprint: cross-cutting)`: CI has not completed, no green tier exists, and the orchestrator-owned full-matrix label is absent. Its worktree-present finding was then resolved by explicitly removing the three clean Mac ld02b worktrees listed above; the remote active checkout is retained. The spec change is the expected WARN. No merge readiness is claimed.

Published-bundle follow-up: `2d55f22b` requires real-linker provisioning; `ab2e5fab` adds the corrupt-cache witness and prevents running the six links twice. Final head is `ab2e5fabe81c410ba472eb69cb94619752d3864b`. The existing kasumi job at `d90f975a` is untouched and still owns its original source/evidence; the new provisioning and missing-tool requirement await final-head CI. The prior audit/run status above belongs to `c4f0ba42`, not this new head. Two pushes were needed because the first local editing script failed before applying the timeout/deduplication correction; no successful validation of that failed edit is claimed.



