//! s39 acceptance — `wolf test` end-to-end through the `wolf` binary:
//! discovery (`*_test.lu`, `test_*` fns, black-box `main` fallback),
//! verdict mapping (assert failure = `trap(assert)`), exit-code
//! discipline (0 all pass; 1 any failure or unsupported; 2 usage),
//! `--filter`/`--list`/`--fail-fast`, the s67 warning posture, the
//! reserved X12 flags, and the wolf-test/0 JSON schema conformance
//! (`docs/test-json.md` is the written contract this test pins).
//!
//! Everything runs on the checked lane — no `cc`, no `libwolf_rt.a`,
//! every host.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const PASSING: &str = "fn test_math() {\n    assert(1 + 1 == 2)\n}\n\n\
                       fn test_rows() -> !int {\n    let x = 41\n    \
                       assert(x + 1 == 42)\n    0\n}\n";
const FAILING: &str = "fn test_holds() {\n    assert(true)\n}\n\n\
                       fn test_breaks() {\n    assert(false, \"deliberate\")\n}\n";
const MAIN_ONLY: &str = "fn main() -> !int {\n    assert(2 + 2 == 4)\n    0\n}\n";
/// A body whose dead `match` arm fires the E0802 warning inside a
/// passing test.
const WARNY: &str = "fn test_warny() {\n    var x = 0\n    match x {\n        \
                     0 => { x = 1 }\n        _ => { x = 2 }\n        1 => { x = 3 }\n    }\n    \
                     assert(x == 1)\n}\n";

fn fixture(case: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("testcmd-{case}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir fixture");
    for (name, src) in files {
        std::fs::write(dir.join(name), src).expect("write fixture");
    }
    dir
}

/// `wolf test` with flags: (exit code, stdout, stderr).
fn run_test(dir: &Path, extra: &[&str]) -> (i32, String, String) {
    let out = Command::new(wolf())
        .arg("test")
        .arg(dir)
        .args(extra)
        .output()
        .expect("run wolf test");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// ------------------------------------------------------- discovery --

#[test]
fn discovers_test_files_and_fns_in_order() {
    let dir = fixture(
        "discover",
        &[
            ("alpha_test.lu", PASSING),
            ("beta_test.lu", MAIN_ONLY),
            ("not_a_test.lu", "fn helper() -> int {\n    3\n}\n"),
        ],
    );
    let (code, out, _err) = run_test(&dir, &[]);
    assert_eq!(code, 0, "all green:\n{out}");
    // Files sorted; fns in declaration order; non-`_test.lu` ignored;
    // a main-only file runs black-box as one test.
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines[0].contains("alpha_test.lu::test_math") && lines[0].ends_with("ok"));
    assert!(lines[1].contains("alpha_test.lu::test_rows") && lines[1].ends_with("ok"));
    assert!(lines[2].contains("beta_test.lu::main") && lines[2].ends_with("ok"));
    assert!(out.contains("3 passed; 0 failed"));
    assert!(!out.contains("not_a_test"), "non-test files never run");
}

#[test]
fn list_prints_without_running() {
    let dir = fixture("list", &[("alpha_test.lu", PASSING)]);
    let (code, out, _err) = run_test(&dir, &["--list"]);
    assert_eq!(code, 0);
    assert!(out.contains("alpha_test.lu::test_math"));
    assert!(out.contains("alpha_test.lu::test_rows"));
    assert!(!out.contains("ok"), "--list runs nothing");
}

// ------------------------------------------- verdicts + exit codes --

#[test]
fn assert_failure_is_trap_assert_and_exit_1() {
    let dir = fixture("failing", &[("f_test.lu", FAILING)]);
    let (code, out, _err) = run_test(&dir, &[]);
    assert_eq!(code, 1, "a failing test fails the run:\n{out}");
    assert!(out.contains("test_holds ... ok"));
    assert!(out.contains("test_breaks ... FAILED (trap(assert))"));
    assert!(out.contains("1 passed; 1 failed"));
}

#[test]
fn filter_selects_by_substring() {
    let dir = fixture("filter", &[("f_test.lu", FAILING)]);
    let (code, out, _err) = run_test(&dir, &["--filter=holds"]);
    assert_eq!(code, 0, "the failing test is filtered out:\n{out}");
    assert!(out.contains("1 passed; 0 failed; 0 rejected; 0 unsupported; 1 filtered out"));
}

#[test]
fn fail_fast_stops_at_the_first_failure() {
    let dir = fixture(
        "failfast",
        &[("a_test.lu", FAILING), ("z_test.lu", PASSING)],
    );
    let (code, out, _err) = run_test(&dir, &["--fail-fast"]);
    assert_eq!(code, 1);
    assert!(out.contains("stopped early"));
    assert!(!out.contains("z_test"), "later files never ran:\n{out}");
}

#[test]
fn compile_error_fails_the_run() {
    let dir = fixture(
        "broken",
        &[("b_test.lu", "fn test_nope() {\n    frobnicate()\n}\n")],
    );
    let (code, out, err) = run_test(&dir, &[]);
    assert_eq!(code, 1, "a test file that does not compile fails the run");
    // #157, s169: REJECTED, not FAILED. The file never ran, so it
    // found nothing — "your test found a bug" and "your module does
    // not build" get different words and different columns.
    assert!(out.contains("REJECTED (does not compile)"), "{out}");
    assert!(!out.contains("FAILED"), "and not the old word:\n{out}");
    assert!(out.contains("0 failed; 1 rejected"), "{out}");
    assert!(
        err.contains("error["),
        "the diagnostic renders on stderr:\n{err}"
    );
}

/// #75, s169: `wolf test` honours `--error-limit`. `build` and `run`
/// have capped their reports since s63; this surface printed every
/// diagnostic a wrecked test module could produce, which is exactly
/// the terminal-scrolling the cap exists to prevent.
#[test]
fn test_reports_are_capped_by_error_limit() {
    // Forty unresolved names: one file, forty errors.
    let mut src = String::from("fn test_wreck() {\n");
    for i in 0..40 {
        src.push_str(&format!("    frobnicate{i}()\n"));
    }
    src.push_str("}\n");
    let dir = fixture("error-limit", &[("e_test.lu", &src)]);

    let (code, _out, capped) = run_test(&dir, &[]);
    assert_eq!(code, 1, "the file does not compile");
    assert!(
        capped.contains("more diagnostic"),
        "the default cap holds back the tail and says so:\n{capped}"
    );
    let shown = capped.matches("error[").count();
    assert!(shown < 40, "capped at {shown} of 40:\n{capped}");

    // `--error-limit=0` prints everything, exactly as on `build`.
    let (_, _out, all) = run_test(&dir, &["--error-limit=0"]);
    assert!(
        !all.contains("more diagnostic"),
        "nothing is held back at limit 0:\n{all}"
    );
    assert!(all.matches("error[").count() > shown, "more than the cap");

    // And a malformed value is a usage error, as everywhere else.
    let (code, _, err) = run_test(&dir, &["--error-limit=lots"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("--error-limit needs a count"), "{err}");
}

#[test]
fn usage_errors_exit_2() {
    let dir = fixture("usage", &[("u_test.lu", PASSING)]);
    let (code, _out, err) = run_test(&dir, &["--no-such-flag"]);
    assert_eq!(code, 2);
    assert!(err.contains("unknown flag"));
}

// ------------------------------------------------- warnings (s67) --

#[test]
fn warnings_surface_but_do_not_fail_a_green_run() {
    let dir = fixture("warny", &[("w_test.lu", WARNY)]);
    let (code, out, err) = run_test(&dir, &[]);
    assert_eq!(code, 0, "warnings never fail a green run:\n{err}");
    assert!(out.contains("test_warny ... ok"));
    assert!(
        err.contains("warning[E0802]"),
        "the warning renders:\n{err}"
    );
}

#[test]
fn deny_warnings_promotes_and_fails_the_run() {
    let dir = fixture("warny-deny", &[("w_test.lu", WARNY)]);
    let (code, out, err) = run_test(&dir, &["--deny-warnings"]);
    assert_eq!(code, 1, "a denied warning fails the run:\n{out}");
    assert!(out.contains("REJECTED (does not compile)"), "{out}");
    assert!(
        err.contains("error[E0802]"),
        "the promotion renders as an error:\n{err}"
    );
}

// ------------------- the X12 flags (s36; spec/07 [sched.flags]) --

/// `--schedules=N` explores: runs each test N times under derived
/// seeds, prints the root seed, and a stable suite stays green.
#[test]
fn schedules_explores_and_prints_root_seed() {
    let dir = fixture("x12-explore", &[("d_test.lu", PASSING)]);
    let (code, out, err) = run_test(&dir, &["--schedules=3", "--json"]);
    assert_eq!(code, 0, "a verdict-stable suite is green:\n{out}\n{err}");
    assert!(
        err.contains("root seed"),
        "exploration prints its root seed:\n{err}"
    );
    assert!(
        out.contains("[3 run(s), seed-blind: the checked machine]"),
        "the test detail says what actually ran:\n{out}"
    );
}

/// s170 (wolf-lang#159's X12 row): "a tool should not print a
/// reproduction command for a run it cannot reproduce."
///
/// `--schedules=N` used to open with "reproduce any run with
/// --replay=SEED" before a single schedule had been explored — and a
/// `test_*` fn runs on the checked machine, serially and seed-blind
/// ([sched.seed]), so for the bodies the book cares about the promise
/// was simply false. The banner no longer makes it, and the summary
/// says how many tests ran without a seed.
#[test]
fn schedules_promises_no_replay_it_cannot_honour() {
    let dir = fixture("x12-honesty", &[("d_test.lu", PASSING)]);
    let (code, out, err) = run_test(&dir, &["--schedules=3"]);
    assert_eq!(code, 0, "a verdict-stable suite is green:\n{out}\n{err}");
    assert!(
        err.contains("root seed"),
        "exploration still prints its root seed:\n{err}"
    );
    assert!(
        !err.contains("reproduce any run with --replay=SEED"),
        "the blanket promise is gone from the banner:\n{err}"
    );
    assert!(
        err.contains("ran without one and carry no replay line"),
        "the summary names the tests that never explored:\n{err}"
    );
}

/// The machine surface carries the same two counts, so a CI job can
/// assert on them rather than on prose.
#[test]
fn schedules_json_summary_carries_the_exploration_counts() {
    let dir = fixture("x12-counts", &[("d_test.lu", PASSING)]);
    let (code, out, err) = run_test(&dir, &["--schedules=3", "--json"]);
    assert_eq!(code, 0, "{out}\n{err}");
    let summary = out
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v["event"] == "summary")
        .expect("a summary event");
    assert_eq!(
        summary["unexplored"], 2,
        "both of the fixture's tests ran seed-blind on the checked machine, \
         and seed-blind repetition is not exploration:\n{out}"
    );
    assert_eq!(summary["explored"], 0, "{out}");
    assert!(
        !out.contains("schedule(s)"),
        "a seed-blind repetition is not called a schedule:\n{out}"
    );
}

/// s170 (wolf-lang#159's X12 row), the half this test used to assert
/// backwards: a `test_*` fn that FAILS under `--schedules=N` gets no
/// replay seed, because nothing about it was seeded.
///
/// It used to get one — `replay: wolf test --replay=<derived seed>` —
/// and running that command re-ran the same serial checked execution
/// the seed had no part in. The X12 contract's "every finding carries
/// its replay line" is a claim about findings the schedule explorer
/// made; the native lane's replay line is pinned by
/// `decimal_replay_states_its_native_scope` and by `conc_native.rs`.
#[test]
fn schedules_failure_on_the_checked_machine_offers_no_seed() {
    let dir = fixture("x12-fail", &[("f_test.lu", FAILING)]);
    let (code, out, _err) = run_test(&dir, &["--schedules=2"]);
    assert_eq!(code, 1, "the failing test still fails:\n{out}");
    assert!(
        out.contains("test_breaks"),
        "the failure is still reported by name:\n{out}"
    );
    assert!(
        !out.contains("--replay="),
        "a seed-blind run offers no reproduction seed:\n{out}"
    );
}

/// `--replay` accepts every schedule spelling (`[sched.seed]`):
/// decimal seed, `w1-` token, explicit `ev:` stream — and rejects
/// nonsense with usage exit 2.
#[test]
fn replay_accepts_every_schedule_spelling() {
    let dir = fixture("x12-replay", &[("d_test.lu", PASSING)]);
    // The `w1-` token decoder shares the task layer's linux-only
    // posture; elsewhere the spelling is a usage error like any other
    // unparseable schedule, so only linux exercises it here.
    #[cfg(target_os = "linux")]
    let specs = ["42", "4611686018427387916", "w1-5", "ev:0,1,0"];
    #[cfg(not(target_os = "linux"))]
    let specs = ["42", "4611686018427387916", "ev:0,1,0"];
    for spec in specs {
        let (code, _out, err) = run_test(&dir, &[&format!("--replay={spec}")]);
        assert_eq!(code, 0, "`--replay={spec}` runs:\n{err}");
        assert!(
            err.contains("replaying schedule"),
            "the replay banner names the schedule:\n{err}"
        );
    }
    let (code, _out, err) = run_test(&dir, &["--replay=nonsense"]);
    assert_eq!(code, 2, "a malformed schedule is a usage error");
    assert!(
        err.contains("sched.seed"),
        "the refusal cites the spec:\n{err}"
    );
}

/// The replay line's SCOPE is stated where a reader meets it (s74,
/// #65). A decimal `--replay` drives the real runtime, where the seed
/// pins the scheduler PRNG and not cross-task arrival order; the banner
/// says so rather than letting the flag imply a guarantee it cannot
/// keep. The `w1-`/`ev:` forms carry a decision stream, so they get no
/// such caveat.
#[test]
fn decimal_replay_states_its_native_scope() {
    let dir = fixture("x12-scope", &[("d_test.lu", PASSING)]);
    let (code, _out, err) = run_test(&dir, &["--replay=42"]);
    assert_eq!(code, 0, "the replay runs:\n{err}");
    assert!(
        err.contains("arrival order") && err.contains("sched.seed"),
        "a decimal replay states what the seed does not pin:\n{err}"
    );
    let (code, _out, err) = run_test(&dir, &["--replay=ev:0,1,0"]);
    assert_eq!(code, 0, "the stream replay runs:\n{err}");
    assert!(
        !err.contains("arrival order"),
        "a recorded decision stream needs no arrival-order caveat:\n{err}"
    );
}

/// `--schedules` and `--replay` are exclusive verbs.
#[test]
fn schedules_and_replay_are_exclusive() {
    let dir = fixture("x12-excl", &[("d_test.lu", PASSING)]);
    let (code, _out, err) = run_test(&dir, &["--schedules=2", "--replay=1"]);
    assert_eq!(code, 2);
    assert!(err.contains("exclusive"), "{err}");
}

/// `--chaos` keeps its decided name but the engine is parked: loud
/// refusal naming the seams and the owner, never a silent ignore.
#[test]
fn chaos_is_parked_with_owner_named() {
    let dir = fixture("x12-chaos", &[("d_test.lu", PASSING)]);
    let (code, _out, err) = run_test(&dir, &["--chaos"]);
    assert_eq!(code, 2);
    assert!(
        err.contains("c07") && err.contains("--replay="),
        "the refusal names the owner and the replay contract:\n{err}"
    );
}

// --------------------------------------- wolf-test/0 conformance --

/// The `--json` stream validates against the written schema
/// (docs/test-json.md): every line is one JSON object, every object
/// carries `schema: "wolf-test/0"` and a known `event`, required keys
/// are present per event, and exactly one `summary` comes last.
#[test]
fn json_stream_conforms_to_wolf_test_0() {
    let dir = fixture("json", &[("a_test.lu", PASSING), ("f_test.lu", FAILING)]);
    let (code, out, _err) = run_test(&dir, &["--json"]);
    assert_eq!(code, 1);
    let lines: Vec<serde_json::Value> = out
        .lines()
        .map(|l| serde_json::from_str(l).expect("every line is one JSON object"))
        .collect();
    assert!(!lines.is_empty());
    for v in &lines {
        assert_eq!(v["schema"], "wolf-test/0", "versioned from day one");
        match v["event"].as_str().expect("event key") {
            "suite" => {
                assert!(v["file"].is_string());
                assert!(v["tests"].is_u64());
            }
            "test" => {
                assert!(v["file"].is_string());
                assert!(v["name"].is_string());
                let status = v["status"].as_str().expect("status");
                assert!(matches!(status, "pass" | "fail" | "unsupported"));
                assert!(v["detail"].is_string());
                if status != "pass" {
                    assert!(v["stdout"].is_string(), "non-pass tests carry output");
                    assert!(v["stderr"].is_string());
                }
            }
            "summary" => {
                for k in ["passed", "failed", "unsupported", "filtered_out"] {
                    assert!(v[k].is_u64(), "summary carries `{k}`");
                }
                assert!(v["stopped_early"].is_boolean());
            }
            other => panic!("unknown event `{other}` — bump wolf-test/0 deliberately"),
        }
    }
    assert_eq!(
        lines.last().unwrap()["event"],
        "summary",
        "exactly one summary, last"
    );
    assert_eq!(lines.iter().filter(|v| v["event"] == "summary").count(), 1);
    let sum = lines.last().unwrap();
    assert_eq!(sum["passed"], 3);
    assert_eq!(sum["failed"], 1);
}

// ------------------------------------------------- the net corpus --

/// `wolf test` runs the repo's own test-tier corpus green — the
/// dogfooding seam (target 4): the std rig and the book runner
/// inherit exactly this invocation.
#[test]
fn test_tier_corpus_is_green() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/test");
    let (code, out, _err) = run_test(&corpus, &[]);
    // The conc witness (s73) runs natively, which the task layer
    // serves on linux (s28) and macOS (s59). On hosts the port has
    // not reached, the honest verdict is exit 1 with exactly that
    // file unsupported — a green run must mean everything ran.
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        assert_eq!(code, 0, "corpus/test runs green under wolf test:\n{out}");
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        assert_eq!(
            code, 1,
            "off the ported hosts the conc witness is unsupported:\n{out}"
        );
        assert!(
            out.contains("conc_schedules_test.lu"),
            "the only failure is the conc witness:\n{out}"
        );
    }
    assert!(out.contains("assert_test.lu::test_arithmetic_holds ... ok"));
}
