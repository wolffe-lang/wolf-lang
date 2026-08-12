# Contributing to wolf

## Commits
- Commit often, in chunks: one logical change per commit. Never `git add -A`.
- Messages: terse, imperative, under 250 characters. No coauthor lines, no
  generated-with trailers.
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
