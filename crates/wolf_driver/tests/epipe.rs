//! wolf-lang#282 — the toolchain's own text into a closed pipe.
//!
//! `wolf --version | head -1` is a pipeline every shell around the
//! project writes (the book's rig, the tap's test, lobo's stamp check):
//! the reader closes the pipe after one line, the second line's write
//! answers EPIPE, and `println!` — Rust ignores SIGPIPE at startup, so
//! the error is a return value — panicked with `failed printing to
//! stdout: Broken pipe` on stderr. The driver's `emit` treats EPIPE on
//! the `--version`, `--help`, `--explain`, `--man` and `--completions`
//! paths as a quiet exit 0 and every other write failure as the failure
//! it is; this test is the pipe, closed before the child can write.

use std::process::{Command, Stdio};

/// Run `wolf <args>` with its stdout piped and the reading end closed
/// BEFORE the child has written anything — from the child's first
/// byte on, every write is EPIPE.
fn into_a_closed_pipe(args: &[&str]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("wolf spawns");
    drop(child.stdout.take());
    child.wait_with_output().expect("wolf exits")
}

#[test]
fn the_toolchains_own_text_exits_quietly_on_a_closed_pipe() {
    let paths: [&[&str]; 6] = [
        &["--version"],
        &["--help"],
        &["help", "build"],
        &["--explain", "E1001"],
        &["--man"],
        &["--completions", "bash"],
    ];
    for args in paths {
        let out = into_a_closed_pipe(args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "wolf {} into a closed pipe: exit {:?}, stderr:\n{stderr}",
            args.join(" "),
            out.status.code()
        );
        assert!(
            stderr.is_empty(),
            "wolf {} into a closed pipe wrote to stderr:\n{stderr}",
            args.join(" ")
        );
    }
}

/// The reader that stays open sees the whole text — the quiet exit is
/// for EPIPE, not a license to print less.
#[test]
fn the_open_pipe_still_gets_both_version_lines() {
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("--version")
        .output()
        .expect("wolf runs");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert_eq!(text.lines().count(), 2, "two stamped lines:\n{text}");
    assert!(text.starts_with("wolf "), "{text}");
    assert!(
        text.lines()
            .nth(1)
            .is_some_and(|l| l.starts_with("paired with lupin ")),
        "{text}"
    );
}
