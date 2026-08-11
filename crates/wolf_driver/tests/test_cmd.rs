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
    assert!(out.contains("1 passed; 0 failed; 0 unsupported; 1 filtered out"));
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
    assert!(out.contains("FAILED (does not compile)"));
    assert!(
        err.contains("error["),
        "the diagnostic renders on stderr:\n{err}"
    );
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
    assert!(out.contains("FAILED (does not compile)"));
    assert!(
        err.contains("error[E0802]"),
        "the promotion renders as an error:\n{err}"
    );
}

// -------------------------------------------- the X12 flags (s36) --

#[test]
fn determinism_flags_refuse_helpfully_until_s36() {
    let dir = fixture("x12", &[("d_test.lu", PASSING)]);
    for flag in ["--schedules=16", "--replay=42", "--chaos"] {
        let (code, _out, err) = run_test(&dir, &[flag]);
        assert_eq!(code, 2, "`{flag}` is reserved, not silently ignored");
        assert!(
            err.contains("s36") && err.contains("--replay=SEED"),
            "the refusal names the scheduler and the replay contract:\n{err}"
        );
    }
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
    assert_eq!(code, 0, "corpus/test runs green under wolf test:\n{out}");
    assert!(out.contains("assert_test.lu::test_arithmetic_holds ... ok"));
}
