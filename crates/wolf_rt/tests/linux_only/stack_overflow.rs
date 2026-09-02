//! s32 Target 3 acceptance — hitting the guard page is a
//! deterministic fault whose diagnostic NAMES the task. Runs the
//! overflow in a subprocess (re-exec of this binary) and asserts the
//! trap-discipline exit (134) plus the named report on stderr — the
//! same two facts on every host the task layer runs on (the unix
//! signal handler; on windows the vectored exception handler that
//! answers `STATUS_STACK_OVERFLOW`, s60b). `harness = false` so the
//! child mode is a plain main.

const CHILD_ENV: &str = "WOLF_RT_TEST_OVERFLOW_CHILD";

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn overflow_forever(depth: usize) -> usize {
    // Touch the frame page-by-page in address order so the descent
    // cannot leap the guard (rustc's stack probes help; this makes
    // the test not depend on them).
    let mut buf = [0u8; 2048];
    let mut i = 0;
    while i < buf.len() {
        // SAFETY: in-bounds volatile writes into our own frame.
        unsafe { std::ptr::write_volatile(buf.as_mut_ptr().add(i), depth as u8) };
        i += 512;
    }
    let below = std::hint::black_box(overflow_forever(depth + 1));
    below + buf[0] as usize
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub fn main() {
    if std::env::var(CHILD_ENV).is_ok() {
        // Child: overflow inside a named task. Never returns 0.
        let _ = wolf_rt::task::scope("overflow-scope", |s| {
            s.spawn("deep-recursor", |_| {
                std::hint::black_box(overflow_forever(0));
                wolf_rt::task::ExitReason::Normal
            });
        });
        std::process::exit(0);
    }

    let me = std::env::current_exe().expect("current_exe");
    let out = std::process::Command::new(me)
        .env(CHILD_ENV, "1")
        .output()
        .expect("spawn child");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(134),
        "expected the trap-discipline exit; stderr: {stderr}"
    );
    assert!(
        stderr.contains("stack overflow in task 'deep-recursor'"),
        "diagnostic must name the task; stderr: {stderr}"
    );
    println!("stack_overflow: ok (named deterministic fault)");
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn main() {
    // The guard reporter is the task layer's: the subprocess
    // assertion runs where the layer runs — SIGSEGV on linux, SIGBUS
    // on macOS, the vectored handler on windows (s60b). Elsewhere the
    // gate is closed and there is nothing to assert.
    println!("stack_overflow: SKIP (no task layer on this host)");
}
