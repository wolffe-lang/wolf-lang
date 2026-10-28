//! s269: output compatibility, timing records and real terminal behaviour.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}
fn fixture(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("s269-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.lu"), "fn main() -> !int { 0 }\n").unwrap();
    dir
}
fn build(dir: &Path) -> Command {
    let mut c = Command::new(wolf());
    c.env_remove("WOLF_STD")
        .env_remove("WOLF_STATUS_AFTER")
        .env("TERM", "xterm")
        .arg("build")
        .arg(dir.join("main.lu"))
        .arg("-o")
        .arg(dir.join("output"));
    c
}
fn success(out: Output) -> Output {
    assert!(
        out.status.success(),
        "{:?}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    out
}
#[test]
fn fast_non_tty_build_has_no_reporter_bytes() {
    let dir = fixture("fast");
    let out = success(build(&dir).arg("--emit=wir").output().unwrap());
    assert_eq!(out.stdout, b"");
    assert_eq!(
        out.stderr, b"",
        "fast build stderr is an exact empty snapshot"
    );
}
#[test]
fn non_tty_never_receives_live_controls_even_with_immediate_heartbeat() {
    let dir = fixture("notty");
    let out = success(
        build(&dir)
            .arg("--emit=wir")
            .env("WOLF_STATUS_AFTER", "0")
            .output()
            .unwrap(),
    );
    assert!(!out.stderr.contains(&b'\r'));
    assert!(!out.stderr.contains(&0x1b));
    assert!(!String::from_utf8_lossy(&out.stderr).contains("Building ["));
}
#[test]
fn timing_json_is_versioned_and_cache_reuse_is_counted() {
    let dir = fixture("json");
    let path = dir.join("timings.json");
    for reused in [false, true] {
        let out = success(
            build(&dir)
                .arg("--emit=obj")
                .arg(format!("--timings=json:{}", path.display()))
                .output()
                .unwrap(),
        );
        assert_eq!(out.stdout, b"");
        assert_eq!(out.stderr, b"");
        let data: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(data["schema"], "wolf-build-timings/1");
        let b = &data["builds"][0];
        assert_eq!(b["tier"], "native");
        assert_eq!(b["success"], true);
        assert_eq!(b["units"], 1);
        assert_eq!(b["reused"], usize::from(reused));
        let rows = b["rows"].as_array().unwrap();
        for phase in [
            "load+resolve",
            "typecheck",
            "mem",
            "WIR lowering",
            "codegen",
        ] {
            assert!(
                rows.iter().any(|r| r["phase"] == phase),
                "missing {phase}: {rows:?}"
            );
        }
        assert!(
            rows.iter()
                .all(|r| r["seconds"].as_f64().unwrap() >= 0.0 && r["count"].as_u64().unwrap() > 0)
        );
    }
}
#[test]
fn timings_failure_and_quiet_keep_diagnostics_honest() {
    let dir = fixture("errors");
    let out = build(&dir)
        .arg("--emit=wir")
        .arg(format!(
            "--timings=json:{}/missing/report.json",
            dir.display()
        ))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("timings:"));
    let out = success(
        build(&dir)
            .args(["--emit=wir", "--timings", "-q"])
            .env("WOLF_STATUS_AFTER", "0")
            .output()
            .unwrap(),
    );
    assert_eq!(out.stderr, b"");
    std::fs::write(
        dir.join("main.lu"),
        "fn main() -> !int { missing_name() }\n",
    )
    .unwrap();
    let out = build(&dir)
        .args(["--emit=wir", "--quiet"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("E0301"));
    let path = dir.join("failure.json");
    let out = build(&dir)
        .args(["--emit=wir", "--quiet"])
        .arg(format!("--timings=json:{}", path.display()))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("E0301"));
    let data: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(data["builds"][0]["success"], false);
    assert!(!data["builds"][0]["rows"].as_array().unwrap().is_empty());
}
#[test]
fn test_command_keeps_json_stdout_and_records_each_compilation() {
    let dir = fixture("test");
    let path = dir.join("timings.json");
    std::fs::write(dir.join("a_test.lu"), "fn test_one() { assert(true) }\n").unwrap();
    std::fs::write(
        dir.join("b_test.lu"),
        "fn test_two() { assert(1 + 1 == 2) }\n",
    )
    .unwrap();
    let out = success(
        Command::new(wolf())
            .env_remove("WOLF_STD")
            .arg("test")
            .arg(&dir)
            .args(["--json", "--no-doc", "--quiet"])
            .arg(format!("--timings=json:{}", path.display()))
            .output()
            .unwrap(),
    );
    assert_eq!(out.stderr, b"");
    for line in String::from_utf8(out.stdout).unwrap().lines() {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["schema"], "wolf-test/0");
    }
    let data: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let builds = data["builds"].as_array().unwrap();
    assert_eq!(builds.len(), 2);
    assert!(
        builds
            .iter()
            .all(|b| b["command"] == "test" && b["tier"] == "checked" && b["success"] == true)
    );
}
#[cfg(target_os = "linux")]
#[test]
fn controlling_pty_cleanup_and_slow_non_tty_heartbeat() {
    let dir = fixture("pty");
    let out = Command::new("python3")
        .arg("-I")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/status/pty.py"))
        .arg(wolf())
        .arg(&dir)
        .output()
        .expect("python3 for real pty witness");
    assert!(
        out.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&out.stdout));
}
