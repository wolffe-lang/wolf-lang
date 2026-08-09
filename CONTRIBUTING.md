# Contributing to wolf

## Commits
- Commit often, in chunks: one logical change per commit — never `git add -A`.
- Messages: terse, imperative, <250 chars. No coauthor lines, no
  generated-with trailers.
- Tests land in the same commit as the code they test; benchmarks land with
  the perf claims they prove.

## Quality bars
- Tests are first-class; CI is first-class. `cargo xtask ci` must be green
  before pushing (fmt-check, clippy -D warnings, tests, deps-check).
- The crate dependency direction is locked (see `Cargo.toml` and
  `cargo xtask deps-check`):
  `span ← diag ← {lex, ast} ← parse ← sema ← mem ← wir ← codegen_* ← driver`;
  `wolf_rt` may depend on `wolf_span` at most.
- Every diagnostic gets a reviewed snapshot. Every corpus surface change
  updates `corpus/` in the same commit as the spec change — the corpus is
  never allowed to drift from the locked surface (s02 discipline; review
  checklist item).
- Platform-agnostic by default: code must not assume linux/x86-64; the CI
  matrix (linux x86-64/aarch64, macOS aarch64, windows x86-64, freebsd
  cross-build) is the arbiter.

## Testing conventions
- Property tests use proptest; case count follows `PROPTEST_CASES` (small
  in PR CI, large in nightly CI). Exemplar: `crates/wolf_span/tests/`.
- Snapshots use insta (`cargo insta review` to update deliberately);
  exemplar: `xtask/tests/directive_snapshots.rs`.
- Benchmarks: `cargo xtask bench --track=<runtime|compile>` emits JSONL to
  `bench-results/`; `cargo xtask bench diff <base> <cand> [--gate]` is the
  variance-aware comparison (median + 3×MAD noise floor, 2% practical
  floor, N≥10 runs). Reference kernels live in `bench/kernels/`.
- Fuzz targets live in `fuzz/`; `cargo xtask fuzz-smoke` builds them where
  cargo-fuzz (nightly) is available.

## Toolchain
- Pinned in `rust-toolchain.toml` (rustup/CI) and `rust-version`
  (everyone). Bump deliberately, in a dedicated commit, CI-green.

## Where the plan lives
- `.docs/` (untracked): sprint plan, decision log, research reports. The
  sprint files under `.docs/sprints/` are the implementation contract; the
  decision log `.docs/planning/02-decisions.md` is the design authority.
