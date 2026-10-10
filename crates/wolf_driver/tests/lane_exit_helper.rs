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

/// wolf-lang#620: pkg.rs's diamond read this exact stderr (kasumi, GNU
/// ld via collect2; hasu's binutils 2.46 prints the same line) as a
/// skip. A link error is the compiler's defect, never the host's.
#[test]
#[should_panic(expected = "failed to LINK")]
fn a_gnu_ld_undefined_reference_fails() {
    environment_refusal(
        &out(
            2,
            "ld: /tmp/wolf-link-1/02-root.o: in function `main':\n\
             main.lu:6:(.text+0x2f): undefined reference to `go'\n\
             collect2: error: ld returned 1 exit status\n\
             wolf run: `cc` failed linking ./.lu-cache/bin/main",
        ),
        "diamond-built",
    );
}

#[test]
#[should_panic(expected = "undefined symbol: go")]
fn an_lld_undefined_symbol_fails_and_names_itself() {
    environment_refusal(
        &out(2, "ld.lld: error: undefined symbol: go\ncollect2: error: ld returned 1 exit status"),
        "diamond-built",
    );
}

#[test]
fn every_linker_spelling_of_a_symbol_defect_fails() {
    for text in [
        "Undefined symbols for architecture arm64:\n  \"_go\", referenced from:",
        "duplicate symbol '_go' in:\n    a.o\n    b.o",
        "ld: b.o: multiple definition of `go'; a.o: first defined here",
        "ld.lld: error: duplicate symbol: go",
    ] {
        let r = std::panic::catch_unwind(|| environment_refusal(&out(2, text), "test"));
        assert!(r.is_err(), "{text:?} was read as an environment refusal");
    }
}

/// A host that cannot link at all still skips: no symbol is named.
#[test]
fn a_missing_linker_or_runtime_is_still_a_refusal() {
    for text in [
        "wolf run: cannot run `cc`: No such file or directory (os error 2)",
        "/usr/bin/ld: cannot find -lwolf_rt: No such file or directory\n\
         collect2: error: ld returned 1 exit status",
        "wolf build: libwolf_rt.a not found beside the compiler",
    ] {
        assert!(environment_refusal(&out(2, text), "test"), "{text:?}");
    }
}
