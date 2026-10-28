//! Base-only plant gate: live controls must never reach non-terminal stderr.
use std::path::Path;
use std::process::Command;

#[test]
fn non_tty_never_receives_live_controls_even_with_immediate_heartbeat() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("s269-notty");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.lu"), "fn main() -> !int { 0 }\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .env_remove("WOLF_STD")
        .env("WOLF_STATUS_AFTER", "0")
        .env("TERM", "xterm")
        .arg("build")
        .arg(dir.join("main.lu"))
        .arg("-o")
        .arg(dir.join("output"))
        .arg("--emit=wir")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(!out.stderr.contains(&b'\r'));
    assert!(!out.stderr.contains(&0x1b));
    assert!(!String::from_utf8_lossy(&out.stderr).contains("Building ["));
}

// No reporter implementation is present: this branch is the baseline,
// the independent gate, and the deliberately unconditional live write only.
