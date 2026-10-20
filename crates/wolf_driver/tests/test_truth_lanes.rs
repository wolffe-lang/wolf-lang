//! s250 (wolf-lang#817, #810; yew yw02 §3, G23, G29) — `wolf test` tells
//! the truth about where a test ran and what it printed.
//!
//! **#817, the native fallback named and counted.** A `main` test the
//! checked machine refuses (its 20M-step budget, a construct outside its
//! surface) runs once natively. Until s250 the row read `ok` and the only
//! trace was `detail: "exit(0) [native]"` in `--json`; yw02 read ten of
//! twenty files green that the checked machine never finished. The row
//! now names the refusal (`ok [native: step budget exhausted]`), the
//! summary counts it (`1 passed, 1 of them native`), `--json` keeps the
//! detail and gains a `native` key and count, and the exit status is the
//! one it was: reporting, not a new failure. `--no-native-fallback` makes
//! the refusal the failure a checked-only suite wants.
//!
//! **#810, `--nocapture` and the child role.** `--nocapture` shows a
//! test's standard output and standard error as the test writes them, a
//! passing test's `SKIP:` lines included (#571's evidence rule): the
//! checked machine echoes each write, a native run's pipes are pumped.
//! Under `--json` the echo goes to stderr and every test event carries
//! its output. And `os_exe()`: on the checked machine it named the `wolf`
//! binary, so a test re-spawning the program under test started the
//! compiler; under `wolf test` the checked machine now refuses it by name
//! (`[os.proc.exe]`), so a `main` test runs natively, where `os_exe()` is
//! the test's own executable, and a `test_*` fn is `unsupported`. Outside
//! `wolf test` nothing changes (`conform-run --checked` still answers the
//! `wolf` binary).
//!
//! Red at this file's commit (at 0.2.33: kasumi `~/lanes/s250/ev/w0-0233.log`):
//! the budget row read `ok` with no mark, no count, no `native` key;
//! `--nocapture` and `--no-native-fallback` were unknown flags (exit 2);
//! the self-spawning test FAILED (`self: child exited 2`, the child being
//! `wolf`), and the `test_*` fn calling `os_exe()` passed.
//!
//! The native rows need the native tier: a host that cannot build it
//! skips them loudly (the s59 pattern, `lane_exit::environment_refusal`),
//! on both streams, and `WOLF_TEST_NATIVE_REQUIRE=1` (CI's linux and
//! macOS gauntlets) makes that skip a failure; the checked rows run
//! everywhere.

mod lane_exit;

use std::io::BufRead as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A scratch copy of fixture directory `case` for the test `test`:
/// `wolf test` runs there, never inside the repository, and no two tests
/// (which run in parallel) share one.
fn fixture(case: &str, test: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/test_truth")
        .join(case);
    assert!(src.is_dir(), "fixture missing: {}", src.display());
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("test_truth_lanes")
        .join(format!("{case}-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    for e in std::fs::read_dir(&src).expect("fixture dir") {
        let p = e.expect("entry").path();
        std::fs::copy(&p, dir.join(p.file_name().expect("a name"))).expect("copy fixture");
    }
    dir
}

/// `wolf test <dir> <flags…>`: (exit code, stdout, stderr).
fn run_test(dir: &Path, flags: &[&str]) -> (i32, String, String) {
    let out = Command::new(wolf())
        .arg("test")
        .arg(dir)
        .args(flags)
        .output()
        .expect("run wolf test");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Can this host build the native tier? `false` (with a loud SKIP, on
/// both streams) only for an environment refusal — no `cc`, no runtime
/// library; a compile error or an ICE fails the test.
fn native_tier(dir: &Path, entry: &str) -> bool {
    let exe = dir.join(format!("probe{}", std::env::consts::EXE_SUFFIX));
    let out = Command::new(wolf())
        .arg("build")
        .arg(dir.join(entry))
        .arg("-o")
        .arg(&exe)
        .arg("--no-cache")
        .current_dir(dir)
        .output()
        .expect("wolf build runs");
    let _ = std::fs::remove_file(&exe);
    if lane_exit::environment_refusal(&out, "wolf build") {
        let why = String::from_utf8_lossy(&out.stderr);
        // CI's linux and macOS gauntlets build the runtime before these
        // run (`cargo test --workspace`), so a refusal there is a broken
        // job, never a quiet skip (#571).
        assert!(
            std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none_or(|v| v != "1"),
            "WOLF_TEST_NATIVE_REQUIRE=1 and the native tier cannot build: {}",
            why.trim()
        );
        println!("SKIP: the native tier cannot build here: {}", why.trim());
        eprintln!("SKIP: the native tier cannot build here: {}", why.trim());
        return false;
    }
    assert!(
        out.status.success(),
        "wolf build {entry} failed (a compile error is the defect, never a skip):\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    true
}

fn json_lines(out: &str) -> Vec<serde_json::Value> {
    out.lines()
        .map(|l| {
            serde_json::from_str(l).unwrap_or_else(|e| panic!("not one JSON object ({e}): {l}"))
        })
        .collect()
}

fn test_event<'a>(lines: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    lines
        .iter()
        .find(|v| v["event"] == "test" && v["name"] == name)
        .unwrap_or_else(|| panic!("no test event named {name}: {lines:?}"))
}

fn summary(lines: &[serde_json::Value]) -> &serde_json::Value {
    let last = lines.last().expect("a summary");
    assert_eq!(last["event"], "summary", "the summary is last");
    last
}

// ------------------------------------------- #817, the native fallback --

/// yw02's case: a `main` past the step budget passes natively, and the
/// row, the summary and the stderr note say so; the exit status is 0, as
/// it was.
#[test]
fn a_budget_refusal_runs_natively_and_the_row_says_so() {
    let dir = fixture(
        "budget",
        "a_budget_refusal_runs_natively_and_the_row_says_so",
    );
    if !native_tier(&dir, "budget_test.lu") {
        return;
    }
    let (code, out, err) = run_test(&dir, &[]);
    assert_eq!(code, 0, "a native pass is a pass:\n{out}\n{err}");
    assert!(
        out.contains("budget_test.lu::main ... ok [native: step budget exhausted]"),
        "the row names the fallback and the checked machine's refusal:\n{out}"
    );
    assert!(
        out.contains("wolf test: 1 passed, 1 of them native; 0 failed;"),
        "the summary counts the native pass:\n{out}"
    );
    assert!(
        err.contains("1 passing test(s) ran natively because the checked machine refused them"),
        "the note says what a native pass is:\n{err}"
    );
}

/// `--json` keeps the detail it had and gains the refusal and the count.
#[test]
fn a_budget_refusal_is_named_and_counted_in_json() {
    let dir = fixture("budget", "a_budget_refusal_is_named_and_counted_in_json");
    if !native_tier(&dir, "budget_test.lu") {
        return;
    }
    let (code, out, err) = run_test(&dir, &["--json"]);
    assert_eq!(code, 0, "{out}\n{err}");
    let lines = json_lines(&out);
    let ev = test_event(&lines, "main");
    assert_eq!(ev["status"], "pass");
    assert_eq!(
        ev["detail"], "exit(0) [native]",
        "the detail keeps its field"
    );
    assert_eq!(ev["native"], "step budget exhausted", "the refusal, named");
    let sum = summary(&lines);
    assert_eq!(sum["passed"], 1);
    assert_eq!(sum["native"], 1, "the count");
}

/// A checked run that never fell back says nothing new: no mark, no
/// count in the human summary (its line is the one consumers read), a
/// zero count in `--json`.
#[test]
fn a_checked_pass_carries_no_native_mark() {
    let dir = fixture("skip", "a_checked_pass_carries_no_native_mark");
    let (code, out, _) = run_test(&dir, &[]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("skip_test.lu::test_skips_loudly ... ok\n"),
        "{out}"
    );
    assert!(
        out.contains("wolf test: 1 passed; 0 failed; 0 rejected; 0 unsupported; 0 filtered out"),
        "the summary line is unchanged when nothing ran natively:\n{out}"
    );
    let (_, out, _) = run_test(&dir, &["--json"]);
    let lines = json_lines(&out);
    assert!(
        test_event(&lines, "test_skips_loudly")
            .get("native")
            .is_none()
    );
    assert_eq!(summary(&lines)["native"], 0);
}

/// `--no-native-fallback`: the refusal is the verdict, `unsupported`,
/// and the run fails. Nothing is built, so every host runs this.
#[test]
fn no_native_fallback_makes_the_refusal_a_failure() {
    let dir = fixture("budget", "no_native_fallback_makes_the_refusal_a_failure");
    let (code, out, err) = run_test(&dir, &["--no-native-fallback"]);
    assert_eq!(
        code, 1,
        "a refused test fails a checked-only run:\n{out}\n{err}"
    );
    assert!(
        out.contains(
            "budget_test.lu::main ... unsupported (step budget exhausted — no native fallback \
             (--no-native-fallback))"
        ),
        "{out}"
    );
    assert!(
        out.contains("wolf test: 0 passed; 0 failed; 0 rejected; 1 unsupported;"),
        "{out}"
    );
    let (code, out, _) = run_test(&dir, &["--no-native-fallback", "--json"]);
    assert_eq!(code, 1);
    let lines = json_lines(&out);
    let ev = test_event(&lines, "main");
    assert_eq!(ev["status"], "unsupported");
    assert!(ev.get("native").is_none(), "nothing ran natively: {ev}");
    assert_eq!(summary(&lines)["native"], 0);
}

// ------------------------------------------------ #810, the child role --

/// The test re-spawns itself through `os_exe()` and reads the child's 7:
/// the checked machine refuses `os_exe()` by name, the `main` runs
/// natively, and there `os_exe()` is the test's own binary.
#[test]
fn a_self_spawning_test_runs_natively_and_meets_its_own_child() {
    let dir = fixture(
        "self_spawn",
        "a_self_spawning_test_runs_natively_and_meets_its_own_child",
    );
    if !native_tier(&dir, "self_test.lu") {
        return;
    }
    let (code, out, err) = run_test(&dir, &[]);
    assert_eq!(code, 0, "the child is the test, not wolf:\n{out}\n{err}");
    assert!(
        out.contains("self_test.lu::main ... ok [native: `os_exe()` under `wolf test`: the checked machine runs inside the `wolf` binary, so it has no test executable to name ([os.proc.exe])]"),
        "the row names the os_exe refusal:\n{out}"
    );
    assert!(
        !err.contains("is not a wolf command"),
        "the compiler was never the child:\n{err}"
    );
    // The checked-or-nothing suite gets the refusal, by name.
    let (code, out, _) = run_test(&dir, &["--no-native-fallback"]);
    assert_eq!(code, 1);
    assert!(
        out.contains("self_test.lu::main ... unsupported (`os_exe()` under `wolf test`"),
        "{out}"
    );
}

/// A `test_*` fn runs only on the checked machine: `os_exe()` there is
/// `unsupported`, named, and the run fails — never a pass over the
/// `wolf` binary's path.
#[test]
fn os_exe_in_a_test_fn_is_refused_by_name() {
    let dir = fixture("exe_in_test_fn", "os_exe_in_a_test_fn_is_refused_by_name");
    let (code, out, _) = run_test(&dir, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains(
            "exe_test.lu::test_names_its_executable ... unsupported (`os_exe()` under `wolf test`"
        ),
        "{out}"
    );
}

/// Outside `wolf test` the checked machine's `os_exe()` is what it was:
/// the `wolf` binary ([os.proc.exe]).
#[test]
fn os_exe_outside_wolf_test_still_names_wolf() {
    let dir = fixture("exe_main", "os_exe_outside_wolf_test_still_names_wolf");
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(dir.join("exe_main.lu"))
        .arg("--checked")
        .arg("--json")
        .output()
        .expect("conform-run runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("a record");
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    let named = rec["stdout_inline"]
        .as_str()
        .expect("stdout")
        .trim()
        .to_string();
    let want = std::fs::canonicalize(wolf()).expect("wolf's path");
    assert_eq!(
        std::fs::canonicalize(&named).expect("os_exe names a file"),
        want,
        "conform-run --checked's os_exe() is the wolf binary"
    );
}

// --------------------------------------------- #810, --nocapture --

/// Captured, a passing test's SKIP lines are never shown; with
/// `--nocapture` each stream's line is on that stream.
#[test]
fn nocapture_shows_a_passing_tests_skip_lines() {
    let dir = fixture("skip", "nocapture_shows_a_passing_tests_skip_lines");
    let (code, out, err) = run_test(&dir, &[]);
    assert_eq!(code, 0);
    assert!(
        !out.contains("SKIP:") && !err.contains("SKIP:"),
        "captured: {out}\n{err}"
    );
    let (code, out, err) = run_test(&dir, &["--nocapture"]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(
        out.contains("SKIP: on stdout, from a passing test\n"),
        "stdout:\n{out}"
    );
    assert!(
        err.contains("SKIP: on stderr, from a passing test\n"),
        "stderr:\n{err}"
    );
    assert!(
        !out.contains("SKIP: on stderr"),
        "each stream on its own: {out}"
    );
    // The output comes before the row that reports the test.
    let at = out.find("SKIP: on stdout").expect("the line");
    let row = out.find("test_skips_loudly ... ok").expect("the row");
    assert!(at < row, "{out}");
}

/// `--json --nocapture`: stdout stays the event stream (every line one
/// object), the echo is on stderr, and a passing test's event carries
/// its output — the record a lane counts SKIP lines from.
#[test]
fn nocapture_under_json_keeps_stdout_the_event_stream() {
    let dir = fixture("skip", "nocapture_under_json_keeps_stdout_the_event_stream");
    let (code, out, err) = run_test(&dir, &["--json", "--nocapture"]);
    assert_eq!(code, 0, "{out}\n{err}");
    let lines = json_lines(&out);
    let ev = test_event(&lines, "test_skips_loudly");
    assert_eq!(ev["status"], "pass");
    assert_eq!(ev["stdout"], "SKIP: on stdout, from a passing test\n");
    assert_eq!(ev["stderr"], "SKIP: on stderr, from a passing test\n");
    assert!(
        err.contains("SKIP: on stdout, from a passing test\n"),
        "{err}"
    );
    assert!(
        err.contains("SKIP: on stderr, from a passing test\n"),
        "{err}"
    );
}

/// The test's output is shown AS IT RUNS: the test prints a line and
/// waits for a file the runner makes only after reading that line from
/// `wolf test`'s stdout, so it passes only when its output arrived
/// before it ended. Without `--nocapture` it would wait out its minute
/// and fail.
fn streams_as_it_runs(case: &str, row: &str) {
    let dir = fixture(case, "streams");
    let go = dir.join("go");
    let mut child = Command::new(wolf())
        .arg("test")
        .arg(&dir)
        .arg("--nocapture")
        .env("S250_GO", &go)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("wolf test starts");
    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let reader = std::thread::spawn(move || {
        let mut all = String::new();
        for line in std::io::BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let _ = tx.send(line.clone());
            all.push_str(&line);
            all.push('\n');
        }
        all
    });
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::BufReader::new(stderr), &mut s);
        s
    });
    let start = Instant::now();
    let mut seen_early = false;
    while start.elapsed() < Duration::from_secs(120) {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) if line.starts_with("early: on stdout") => {
                // The test is still running: it is waiting for us.
                assert!(
                    child.try_wait().expect("try_wait").is_none(),
                    "wolf test ended before the early line was read"
                );
                seen_early = true;
                break;
            }
            Ok(_) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    if seen_early {
        std::fs::write(&go, b"go").expect("the go file");
    }
    let status = child.wait().expect("wolf test ends");
    let out = reader.join().expect("stdout reader");
    let err = err_reader.join().expect("stderr reader");
    assert!(
        seen_early,
        "the early line never arrived while the test ran:\n{out}\n{err}"
    );
    assert_eq!(status.code(), Some(0), "{out}\n{err}");
    assert!(out.contains(row), "{out}");
    assert!(
        err.contains("SKIP: on stderr, before the test ends"),
        "{err}"
    );
}

#[test]
fn nocapture_streams_a_checked_test_as_it_runs() {
    streams_as_it_runs(
        "stream_checked",
        "stream_test.lu::test_streams_as_it_runs ... ok\n",
    );
}

#[test]
fn nocapture_streams_a_native_test_as_it_runs() {
    let dir = fixture(
        "stream_native",
        "nocapture_streams_a_native_test_as_it_runs",
    );
    if !native_tier(&dir, "stream_test.lu") {
        return;
    }
    streams_as_it_runs(
        "stream_native",
        "stream_test.lu::main ... ok [native: `os_exe()` under `wolf test`",
    );
}

/// `--help`'s usage line names both flags.
#[test]
fn the_usage_names_the_new_flags() {
    let (code, _, err) = run_test(Path::new("."), &["--no-such-flag"]);
    assert_eq!(code, 2);
    assert!(err.contains("[--nocapture]"), "{err}");
    assert!(err.contains("[--no-native-fallback]"), "{err}");
}
