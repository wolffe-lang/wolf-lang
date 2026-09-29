//! wolf-lang#471: `lane_exit::environment_refusal`, the test every
//! native/release gate asks before it reads an exit 2 as "this host
//! cannot run the lane". An environment refusal skips; an internal
//! compiler error (the driver's `wolf <command>: ICE: …` line, then
//! exit 2) fails the test. The stderr texts below are the driver's own
//! (`crates/wolf_driver/src/main.rs`).

mod lane_exit;

use lane_exit::environment_refusal;
use std::process::{ExitStatus, Output};

#[cfg(unix)]
fn status(code: i32) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(code << 8)
}

#[cfg(windows)]
fn status(code: i32) -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatus::from_raw(code as u32)
}

fn out(code: i32, stderr: &str) -> Output {
    Output {
        status: status(code),
        stdout: Vec::new(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

#[test]
fn an_exit_two_with_no_ice_is_an_environment_refusal() {
    for text in [
        "wolf conform-run: cannot run `cc`: No such file or directory (os error 2)",
        "wolf build: the release tier targets linux/x86-64 and macOS/aarch64",
        // `DEVICE` holds the letters, not the marker.
        "wolf build: write /tmp/x: No space left on DEVICE",
        "",
    ] {
        assert!(environment_refusal(&out(2, text), "test"), "{text:?}");
    }
}

#[test]
fn any_other_exit_is_not_a_refusal() {
    for code in [0, 1, 3, 65, 101] {
        assert!(!environment_refusal(
            &out(code, "wolf build: ICE: backend: x"),
            "test"
        ));
    }
}

#[test]
#[should_panic(expected = "internal compiler error")]
fn a_mid_end_ice_fails() {
    environment_refusal(
        &out(
            2,
            "wolf build: ICE: mid-end broke the module\ntoken-linearity",
        ),
        "conform-run --release",
    );
}

#[test]
#[should_panic(expected = "wolf build: ICE: backend: bad")]
fn a_backend_ice_fails_and_names_itself() {
    environment_refusal(&out(2, "wolf build: ICE: backend: bad"), "wolf build");
}

#[test]
#[should_panic(expected = "internal compiler error")]
fn a_conform_run_ice_fails() {
    environment_refusal(
        &out(
            2,
            "warning: x\nwolf conform-run: ICE: native binary died without an exit code",
        ),
        "conform-run --native",
    );
}
