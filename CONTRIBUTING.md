# Contributing to wolf

## Commits
- Commit often, in chunks: one logical change per commit. Never `git add -A`.
- Messages: terse, imperative, under 250 characters.
- A count in a subject line (`anchors 452 -> 453`) is advisory; the
  registry is the count. Predict against `spec/anchors.json` (and the
  corpus, the catalog) read at the commit, never against a message
  (F-0100's rule — wolf-lang#309 has an instance in each direction).
- Tests land in the same commit as the code they test. Benchmarks land with
  the perf claims they prove.

## Quality bars
- `cargo xtask ci` must be green before pushing. It runs `cargo fmt --check`,
  `clippy -D warnings`, and the workspace tests, then every verify lane in
  turn (`deps-check`, `corpus`, `abi-check`, `debug-check`, and the rest).
  The step list is the `ci()` table in `xtask/src/main.rs`; a lane added
  there is a lane you have to pass.
- The crate dependency direction is locked (see `Cargo.toml` and
  `cargo xtask deps-check`):
  `span ← diag ← {lex, ast} ← parse ← sema ← mem ← wir ← backend ← codegen_* ← driver`;
  `wolf_rt` may depend on `wolf_span` at most.
- Every diagnostic gets a reviewed snapshot. A corpus surface change updates
  `corpus/` in the same commit as the spec change, so the corpus never drifts
  from the locked surface (s02 discipline; review checklist item).
- Diagnostic messages follow `crates/wolf_diag/VOICE.md`, and a PR that adds
  or changes one quotes the guide in review. Every code needs a registry
  entry with a real explanation and at least one snapshot fixture;
  `cargo xtask diag-catalog` gates that. Compiler-phase crates never print
  (`cargo xtask print-gate`).
- Platform-agnostic by default: code must not assume linux/x86-64. The CI
  matrix (linux x86-64/aarch64, macOS aarch64, windows x86-64, freebsd
  cross-build) is the arbiter.

## CI tiers and the merge rule (tl21)
The org has 20 concurrent CI jobs for every repository, 5 of them macOS.
`.github/workflows/ci.yml` runs in tiers, chosen by its `plan` job
(`ci/tiers.py`, held by `ci/tiers-test.py`); the run's last job is named
for the tier and is red unless the plan succeeded, every job it chose
succeeded and every other job was skipped.

| run | tier | jobs |
|---|---|---|
| a pull request's push | `tier pr` | linux lint + test + corpus (the whole `cargo xtask ci`), the fuzz smoke |
| push to `trunk`, the nightly (02:07 UTC), `gh workflow run ci`, a pull request carrying **`full-matrix`** | `tier full` | those, the macOS native gauntlet's three shards, build + test on macOS and windows, the freebsd cross-check |
| a pull request that changes **only prose** | `tier noop` | none |

- **A PR merges only with a green `tier full` at its head.** `tier pr` is
  not the merge gate: it never ran macOS, windows or freebsd, and
  `lane-audit.sh` fails a head that has no green `tier full`.
- Open the PR **without** the label (`opened` and `labeled` together start
  two full runs, and the second cancels the first). Push until the head is
  the one to merge, then `gh pr edit <N> --add-label full-matrix` and wait.
  A push after that runs full again, so label last.
- A new push cancels the same branch's older run of the same tier. Trunk
  pushes, the nightly and dispatches are never cancelled.
- Prose is narrow and decided by path: `CHANGELOG.md`, `CONTRIBUTING.md`,
  `docs/*.md` and `docs/release/*.md`. **Not prose:** anything under
  `spec/` (`anchors.json` and `prelude.json` included), `README.md`, and
  the generated `docs/diagnostics.md`, `docs/warnings.md` and `docs/api/`;
  gates read all of them. A changed path no rule of `ci/tiers.py` names
  runs the full tier; add the rule in the same PR. A prose-only PR still
  needs the label's run to merge.
- A planted break needs one job: `gh workflow run ci --ref <branch> -f
  jobs=<job id>` (`tier partial`, never a merge gate).

## Testing conventions
- Property tests use proptest. Case count follows `PROPTEST_CASES`, which is
  small in PR CI and large in nightly CI. Exemplar: `crates/wolf_span/tests/`.
- Snapshots use insta. Run `cargo insta review` to update one deliberately;
  exemplar: `xtask/tests/directive_snapshots.rs`.
- Benchmarks: `cargo xtask bench --track=<runtime|compile>` emits JSONL to
  `bench-results/`. `cargo xtask bench diff <base> <cand> [--gate]` is the
  variance-aware comparison (median + 3×MAD noise floor, 2% practical floor,
  N≥10 runs). Reference kernels live in `bench/kernels/`.
- Fuzz targets live in `fuzz/`. `cargo xtask fuzz-smoke` builds them where
  cargo-fuzz (nightly) is available, and skips loudly where it is not.

## Toolchain
- Pinned in `rust-toolchain.toml` (rustup/CI) and `rust-version` (everyone).
  Bump it in its own commit, with CI green.

## Where the plan lives
- `.docs/` (untracked): sprint plan, decision log, research reports. The
  sprint files under `.docs/sprints/` are the implementation contract; the
  decision log `.docs/planning/02-decisions.md` is the design authority.
