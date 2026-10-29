# s271 — make test, fmt, fix and nightly failures truthful

_Materialised from the body of wolf-lang#997 at the merge (`5b553088`)._


Refs #898, #970, #958, #506, #857, #616, #947. No issue is closed by this PR.

## Forbidden

Spec untouched; no language change, third-party implementation source, attribution trailer, merge, tag, release or label. Aspen was read-only. Fresh hasu clone; all execution inside the rootless container on block A (0–5 under its flock), 12 GiB, foreign uid-0 pid 1 and uid-1000 tests. No host signal witnesses, alternate block or host, or maintainer-service changes.

## Inputs, verified

Started at `5adf3d91` (v0.2.36), rebased onto `d2753b48`; final candidate `0190d2b406e9c79b245cabd14b31389cdd882a82`. After s270b merged it was rebased once more, onto `a246580b`, without conflicts. All twelve commits are patch-identical (`git range-diff`): `9dbe1502`→`4e0b888e`, `fa788b85`→`dfaef3b5`, `94e32740`→`a568b84e`, `f624b5ed`→`f3fcc691`, `fde73f81`→`30722481`, `a064c503`→`15d9d5e1`, `3081fd63`→`6b73bc1f`, `8b25fac3`→`4642e20d`, `7a92f6d6`→`37424da5`, `00016388`→`a65307b5`, `0190d2b4`→`374c416c`, `a9bd986c`→`11a82baa`. The only file both lanes touch is `docs/diagnostics.md`: s270b added the #948 fixture paths to E1010's line, s271 added the E1016 section, and the two merged untouched. `cargo xtask diag-catalog --check` is green at the rebased head. Final head **`5b55308811da980150fa090f63ca5916060c6146`**. Runs and plants below that name `0190d2b4` or `a9bd986c` ran at those pre-rebase heads. Rebase retained both CI checks, build-progress instrumentation and quiet/timing help. Mac worktree: `~/lanes/s271/wolf-lang`; fresh hasu clone: `/home/mfwolffe/lanes/s271/wolf-lang`.

`localhost/r41-toolchain:1`: Rust 1.97.1, clang/LLD 18.1.3, strace; musl and none targets verified. `candidate2.log` reads the following inputs twice before use:

- lupin 0.1.58, pin `a5378e3`; archive SHA256 `34f52792bb59bc7565e0edc1235cd47d9a6ad13d5e6528993627f2e699b62193`.
- Bootstrap wolf 0.2.35, pin `cf0c13d`; complete archive SHA256 `4cc4045bbbb605c07835bc4f32685953c784eeef4cd3139a48b457dfc90dad1a`.
- wolf-std `78b9d3cbc41d51db44d18c40b6694403644d1c83`; clef `996a3ccdf160923757c4ce6aa474efdffa2135b3`; ligature `3f20641375d0493b5c62f802f435d624de172ede`.
- Lane-owned musl bundle SHA256 `6f99a2794da54b2dd85f06713d736b27862a7cd0da9e28bd940d975a4603a1d1`, all eight members verified twice. The borrowed cache failed its checksum and was left untouched.
- Head-built musl runtime SHA256 `17132d79da53b2d11237340c9133ce20b2b4893bf299bb1c4442ee7697dd70da` (46,928,434 bytes), never the bundle's old runtime. Clef and ligature executable hashes are also read twice in the log.

README read, current. No pin or version bump.

## Prediction

`9dbe1502` records the prediction before implementation. The setup, assertion record, parser refusal, unapplied fixes and coded lend refusal predictions held. #857 was already repaired by `0eb9bef9`: 50,000 bounded cases found no remaining drift, so this pass adds regression coverage rather than claiming a new layout repair. The first nightly dispatch exposed a toolchain prerequisite mistake; the correction installs musl for the checkout-selected Rust, not the action's newer stable.

## Evidence index

- `dfaef3b5`, `6b73bc1f` — #898: keep `cargo test --workspace`; add verified link inputs, head musl runtime, pinned ligature/clef and a job-only dispatch selector. Repaired job dispatch **40802774582**, head `0190d2b4`, was read once immediately with only `proptest-large` active. The orchestrator watches it; #898 still requires a green nightly after merge.
- `a65307b5` — #970/#958: message, source site and equality operands after FAILED and in optional JSON `assertion` data; checked, native and LLVM test executors. Nine real-binary rows cover both assertion forms, nocapture/JSON, effects evaluated once, primitive/nominal values, raw stderr, zero-cap regions and unchanged passing bytes. No new `assert_eq` builtin or function-name magic: helpers using `assert(a == b)` retain both operands.
- `a568b84e`, `37424da5` — #506: parser diagnostics and nonzero exit in both fmt modes. Final corpus census: **1,232 files; 1,189 exit 0 / 43 nonzero**. All **39** parser counterexamples retain every parser error heading; the other four refusals are declared relaid witnesses. `fmt-lu` independently verifies those parser refusals, not a broad directive exclusion.
- `4642e20d` — #857: aspen's if-argument fixture joins the regression tree consumed by `fmt-fixpoint`; 50,000 bounded cases pass. Existing layout code is unchanged.
- `f3fcc691` — #616: every unapplied suggestion lists site/applicability/reason; invalid files return 1, including after applying edits. The exact issue program reports E1002's maybe fix; rechecking exposes the legacy str-index fixture's real E0411 instead of turning it green.
- `30722481` — #947: **E1016**, normal source renderer, catalog/fix pattern and reviewed `ledger_diagnostic__lent_value_refusal.snap`; refusal still exits 4. Reporter subprocess and record-extension tests pin the code/site; unrelated refusals remain unchanged. Catalog: **193 codes / 34 warnings**, all with fixtures.
- `15d9d5e1` — focused `tool-details` CI gate and planner tests; `374c416c` — command help.
- `11a82baa` — the RT_SYMBOLS drift guard (`crates/wolf_driver/tests/str_list_fs_native.rs`, `the_runtime_symbol_table_covers_the_s40_families`) lists `a65307b5`'s two new runtime symbols, `__wolf_rt_assert_buf` and `__wolf_rt_assert_detail`, by name, and the count moves 255 → 257. At `0190d2b4` the guard failed at `:863` (`left: 257 right: 255`) in the local gauntlet, PR run 40805708803, labelled run 40806805769 and nightly job 40802774582. That was the only red in all four.
- `5b553088` — print-gate (`crates/wolf_driver/src/refusal_tests.rs:15`) refused the E1016 fixture's reason, which ended in `, s89)`. The fixture had copied the compiler's own text: both lent-view refusals in `crates/wolf_wir/src/lower.rs` carried the sprint id. They are the value-position refusal at `:5805` and the task-capture refusal at `:7499`, and E1016's note prints each to the reader. print-gate missed them because each literal continues onto a second line. The id is gone from both refusals, from the three fixtures (`refusal_tests.rs:15`, `crates/wolf_sema/tests/ledger_diagnostic.rs:15` and `:24`) and from the reviewed snapshot `ledger_diagnostic__lent_value_refusal.snap`. Nothing else matches on the old text.

Final-head targeted evidence: `/home/mfwolffe/lanes/s271/records/candidate2.log`, ending `S271_CANDIDATE_GREEN head=0190d2b406e9c79b245cabd14b31389cdd882a82` (exit 0). Includes strict clippy, assertion/fmt/fix/help/refusal tests, incoming four-machine and status/PTY tests, memory tests, WIR goldens, formatter sweep, census and catalog check. No tracked source change during the run.

Three separate draft plants, never in the fix line:

- **#993**, `fa4a5e76`: nightly **40778596902**, job 123134205381, named `every_root_manifest_program_links_with_real_ligature_on_both_tiers ... FAILED`, `real ligature is required`, 15 passed / 1 failed, exit 101. Earlier 40767579616 stopped at E0463 and is not counted as plant proof.
- **#995**, `b2ced01b`: partial **40802236578**, job 123212844751, both assertion-message and equality-value tests fail by name; 3 passed / 6 failed, exit 101. JSON details are null while `trap(assert)` survives.
- **#996**, `3787c3f2`: partial **40802650299**, job 123214167858, `multiline_if_argument_reaches_its_fixed_point ... FAILED`, 0 passed / 1 failed, exit 101. Pass one keeps `out.s(`; pass two moves the receiver. This rolls back the existing fix rather than inventing a residual v0.2.36 bug.

## Done-when

Recommended rulings, each implemented:

- **R-s271-1:** supply setup, do not narrow nightly coverage.
- **R-s271-2:** retain the already-correct formatter layout and bank aspen's shape; no speculative style change.
- **R-s271-3:** add assertion code only to test executables; render comparands in reporter storage, outside program-region budgets; preserve ordinary/freestanding ABI and passing output. No new language builtin.
- **R-s271-4:** report the backend's recorded site, including an inlined caller site; do not reconstruct or invent coordinates. E1016 adds an explanation without changing exit-4 classification.

- [x] Every issue has its measured change or explicit already-fixed finding; all three plants red by name.
- [x] Targeted gates green at the pushed head; README read; no issue filed or closed.
- [x] One final-head `cargo xtask ci` job launched with `setsid -f`, pid **572607**, at 02:22:14Z; initially waiting for block A. Log: `/home/mfwolffe/lanes/s271/records/gauntlet.log`; exit marker: `/home/mfwolffe/lanes/s271/records/gauntlet.exit`; pid file: `/home/mfwolffe/lanes/s271/records/gauntlet.pid`. No gauntlet success is claimed before the marker.
- [x] PR-tier run **40805708803**, head `0190d2b4`, read once immediately: in progress. No CI wait.
- [x] Gauntlet at `0190d2b4`: exit **1** at step `test`, with only the RT_SYMBOLS guard failing (`records/gauntlet.log`, `gauntlet.exit`). Fixed by `a9bd986c`.
- [x] At `a9bd986c`: the guard test alone was green (`records/fixer-test.log`, exit 0). The chained gauntlet (`records/gauntlet2.log`) passed every step through `print-gate`, then went red there at 04:42:45Z: `refusal_tests.rs:15: reader-facing string names s89`. Fixed by `5b553088`.
- [x] Rebased onto `a246580b` (above). At the rebased head, alone in hasu's block-A container (`records/fixer3-test.log`, exit 0): `cargo fmt --all --check`, `cargo xtask print-gate`, `cargo xtask diag-catalog --check` (193 codes / 34 warnings, in sync), `the_runtime_symbol_table_covers_the_s40_families`, `refusal_tests::a_lend_refusal_keeps_exit_four_and_has_a_code_and_site`, and `ledger_diagnostic`, each `END … exit=0`. A first try at the fix (`81e14030`, never pushed) left two lines that rustfmt joins; gauntlet3 stopped at `fmt` on it (`records/gauntlet3.log`), and the commit was amended before the push.
- [ ] Full `cargo xtask ci` at **`5b55308811da980150fa090f63ca5916060c6146`**, chained to start only after those tests exited 0, under `setsid -f`: pid **2105016** (`/home/mfwolffe/lanes/s271/records/gauntlet4.pid`), log `/home/mfwolffe/lanes/s271/records/gauntlet4.log`, exit marker `/home/mfwolffe/lanes/s271/records/gauntlet4.exit`. No success is claimed before the marker.
- [x] PR run at `a9bd986c`: **40814880350**, read once: in progress. No CI wait.
- [x] PR run at the rebased head `5b553088`: **40820388261**, read once: in progress. No CI wait.
- [ ] Issue to file (orchestrator): print-gate reads one source line at a time, so it misses a literal that continues onto the next line. At `5b553088`, 13 reader-facing strings still name schedule ids that way: `wolf_wir/src/lower.rs:2592, 4919, 5163, 5259, 7472, 7531, 7709` (`c25 closeout`), `:9405, 25285` (`c06/std`), `wolf_driver/src/test_cmd.rs:362-363` (`s35`, `c07-closeout`), `wolf_driver/src/main.rs:2747` (`s47`, `s60`), `:3237` (`c13`).
- [x] Lane audit run exactly once, directly: **`AUDIT FAIL (footprint: cross-cutting)`**. The only failures are in-progress head CI/no completed tier and the absent `full-matrix` label (reserved for the orchestrator). Fast-forwardability, head, footprint, five sections, all cited shas, spec exclusion and attribution checks passed.
- [ ] The orchestrator labels `full-matrix`, watches full CI and the local gauntlet, then decides merge/closure. This lane labels nothing.



