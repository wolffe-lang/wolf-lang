//! s60b — a stack overflow inside a spawned task reports in wolf's
//! voice and exits with the trap-discipline number, on every host the
//! task layer runs on: the guard-page reporter (a SIGSEGV/SIGBUS
//! handler on linux/macOS, a vectored exception handler answering
//! `STATUS_STACK_OVERFLOW` on windows) prints
//! `wolf-rt: stack overflow in task '<name>'` and exits 134. Before
//! s60b a windows binary died as `0xC00000FD` with no words — the s60a
//! ledger's named gap, now a measured claim.
//!
//! The overflow is genuine recursion in COMPILED wolf code (no runtime
//! test hook): a non-tail call that can never return, run inside a
//! spawned task so the report names it. Hosts the native tier refuses
//! skip loudly (the s59 pattern).

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// Unbounded non-tail recursion inside a task. The `+ 1` after the
/// call keeps the frame live (no tail-call shape to optimize away);
/// the depth parameter keeps the compiler from proving anything about
/// the call's result.
const OVERFLOW_IN_TASK: &str = "\
fn deep(n: int) -> int {
    if n < 0 { 0 } else { deep(n + 1) + 1 }
}

fn main() -> int {
    scope s {
        s.spawn(fn() {
            let x = deep(0)
            print(\"unreachable: {x}\")
        })
    }
    0
}
";

fn build_fixture(case: &str, src: &str) -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("prog.lu");
    std::fs::write(&entry, src).expect("write fixture");
    let exe = dir.join(format!("prog{}", std::env::consts::EXE_SUFFIX));
    let out = Command::new(wolf())
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("wolf runs");
    match out.status.code() {
        Some(0) => Some(exe),
        Some(2) => {
            eprintln!(
                "SKIP: environment cannot link native binaries: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            None
        }
        other => panic!(
            "wolf build failed (exit {other:?}): {}",
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

#[test]
fn stack_overflow_in_a_task_reports_in_wolfs_voice_and_exits_134() {
    let Some(exe) = build_fixture("stack_overflow_task", OVERFLOW_IN_TASK) else {
        return;
    };
    let run = Command::new(&exe).output().expect("run");
    let stderr = String::from_utf8_lossy(&run.stderr);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        run.status.code(),
        Some(134),
        "the trap-discipline exit (D70: one number on every native host); \
         stderr: {stderr} stdout: {stdout}"
    );
    // The task's name is the lowering's entry symbol for the spawned
    // closure (`main.task0.entry` today) — the report names it.
    assert!(
        stderr.contains("wolf-rt: stack overflow in task 'main.task0.entry'"),
        "the report names the task; stderr: {stderr}"
    );
    assert!(
        !stdout.contains("unreachable"),
        "the task ran past its overflow: {stdout}"
    );
}
