//! Cascade posture: a wrecked file gets a bounded report, not a
//! scrolling wall. One root cause should be reachable from the top of
//! the terminal, so the report caps and then says how many it held back
//! and how to see them (`--error-limit`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixture(case: &str, src: &[u8]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir fixture");
    std::fs::write(dir.join("main.lu"), src).expect("write main");
    dir
}

/// Deliberate garbage that makes the parser report a great deal: many
/// independent statement-level wrecks, one per line, so the count comes
/// from breadth rather than from one cascade.
fn wreckage(lines: usize) -> Vec<u8> {
    let mut src = String::from("fn main() -> !int {\n");
    for i in 0..lines {
        src.push_str(&format!("    let = {i} +\n"));
    }
    src.push_str("    0\n}\n");
    src.into_bytes()
}

/// (exit code, stderr) from `wolf build --emit=wir` with extra flags.
fn build(dir: &Path, extra: &[&str]) -> (i32, String) {
    let out = Command::new(wolf())
        .arg("build")
        .arg(dir.join("main.lu"))
        .arg("-o")
        .arg(dir.join("out.wir"))
        .arg("--emit=wir")
        .arg("--no-cache")
        .args(extra)
        .output()
        .expect("run wolf build");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn count_reports(stderr: &str) -> usize {
    stderr
        .lines()
        .filter(|l| l.starts_with("error[") || l.starts_with("warning["))
        .count()
}

#[test]
fn a_wrecked_file_reports_at_most_the_cap_and_says_how_many_it_held_back() {
    let dir = fixture("error-cap-default", &wreckage(200));
    let (code, err) = build(&dir, &[]);
    assert_eq!(code, 1, "the file must not compile:\n{err}");
    let shown = count_reports(&err);
    assert!(
        shown > 0 && shown <= 25,
        "expected a capped report, got {shown} diagnostics"
    );
    assert!(
        err.contains("not shown"),
        "a capped report must account for what it held back:\n{err}"
    );
    assert!(
        err.contains("--error-limit=0"),
        "the cap must name its own escape hatch:\n{err}"
    );
}

#[test]
fn error_limit_zero_prints_everything() {
    let dir = fixture("error-cap-unlimited", &wreckage(200));
    let (_, capped) = build(&dir, &[]);
    let (code, all) = build(&dir, &["--error-limit=0"]);
    assert_eq!(code, 1);
    assert!(
        count_reports(&all) > count_reports(&capped),
        "--error-limit=0 must lift the cap ({} vs {})",
        count_reports(&all),
        count_reports(&capped)
    );
    assert!(
        !all.contains("not shown"),
        "an uncapped report has nothing to account for:\n{all}"
    );
}

#[test]
fn a_small_error_list_arrives_whole_with_no_summary_line() {
    let dir = fixture("error-cap-small", &wreckage(2));
    let (code, err) = build(&dir, &[]);
    assert_eq!(code, 1);
    assert!(
        !err.contains("not shown"),
        "a report under the cap must not mention a cap:\n{err}"
    );
}

#[test]
fn a_bad_error_limit_is_a_usage_error() {
    let dir = fixture("error-cap-badflag", &wreckage(2));
    let (code, err) = build(&dir, &["--error-limit=lots"]);
    assert_eq!(code, 2, "a malformed flag is a usage error:\n{err}");
    assert!(err.contains("--error-limit"), "the message names the flag");
}
