//! s114 — signal RECEPTION (#126), cross-lane.
//!
//! The os_time_native discipline over the signal surface: the
//! deterministic sequential loopback (listen→raise→wait) runs on the
//! checked AND native lanes and must produce the IDENTICAL verdict and
//! stdout — the checked machine models signals as a pure in-machine
//! queue, the native lane delivers the real SIGHUP through the reactor's
//! task layer, and the OUTPUT is causally pinned either way. The
//! concurrent supervisor shape is native-only (the checked lane refuses
//! `spawn` by name, C1-deferred), pinned as an honest non-parity case.
//!
//! Off-target the whole file compiles away (native codegen is
//! linux/x86-64 only at this tier).

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::path::Path;
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

struct Obs {
    verdict: String,
    stdout: String,
}

fn lane(case: &str, src: &str, flag: &str) -> Option<Obs> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join(format!("{case}.lu"));
    std::fs::write(&entry, src).expect("write fixture");
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(&entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) && flag == "--native" {
        eprintln!(
            "SKIP: environment cannot run the native lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {case}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    })
}

/// The sequential loopback agrees across lanes: listen for RELOAD (=1),
/// self-deliver it, wait, read back the meaning. Ten native runs pin
/// stability (real signals, but the wait is causally after the raise).
#[test]
fn loopback_agrees_across_lanes_and_is_stable() {
    let src = r#"
fn main() -> !int {
    os_signal_listen(1)?
    os_signal_raise(1)?
    let m = os_signal_wait(1)?
    print("m={m}")
    if m == 1 { 0 } else { 1 }
}
"#;
    let checked = lane("s114_loopback", src, "--checked").expect("checked always runs");
    assert_eq!(checked.verdict, "exit(0)", "checked verdict");
    assert_eq!(checked.stdout, "m=1\n", "checked stdout");
    for _ in 0..10 {
        let Some(native) = lane("s114_loopback", src, "--native") else {
            return; // environment cannot run the native lane
        };
        assert_eq!(native.verdict, checked.verdict, "cross-lane verdict");
        assert_eq!(native.stdout, checked.stdout, "cross-lane stdout");
    }
}

/// A wait for a meaning that was never raised/listened is `io` at once
/// (an empty/unmapped set can never arrive) — never a hang. The `else`
/// makes the row observable; both lanes agree.
#[test]
fn empty_set_is_io_both_lanes() {
    let src = r#"
fn main() -> !int {
    let m = os_signal_wait(0) else |_| { print("io"); 0 }
    print("done {m}")
    0
}
"#;
    let checked = lane("s114_empty", src, "--checked").expect("checked always runs");
    assert_eq!(checked.verdict, "exit(0)");
    assert_eq!(checked.stdout, "io\ndone 0\n");
    if let Some(native) = lane("s114_empty", src, "--native") {
        assert_eq!(native.verdict, checked.verdict);
        assert_eq!(native.stdout, checked.stdout);
    }
}

/// The wws supervisor shape is native-only: the checked lane refuses
/// `spawn` by name (structured concurrency, C1-deferred) — an honest
/// `unsupported`, not a fake pass — while the native lane delivers the
/// real SIGUSR2 to the parked supervisor.
#[test]
fn supervisor_shape_native_runs_checked_refuses() {
    let src = r#"
fn main() -> !int {
    os_signal_listen(8)?
    var got = 0
    scope s {
        s.spawn(fn() { os_signal_raise(8) })
        got = os_signal_wait(8)?
    }
    print("got={got}")
    if got == 8 { 0 } else { 1 }
}
"#;
    let checked = lane("s114_supervisor", src, "--checked").expect("checked always runs");
    assert_eq!(
        checked.verdict, "unsupported",
        "checked must refuse spawn by name"
    );
    if let Some(native) = lane("s114_supervisor", src, "--native") {
        assert_eq!(native.verdict, "exit(0)", "native delivers the signal");
        assert_eq!(native.stdout, "got=8\n");
    }
}
