#![cfg(target_os = "linux")]
//! s32 Target 1 — "a wolf binary that never spawns is a C binary":
//! the runtime half. No scheduler state, no background threads exist
//! before the first spawn; they exist after. `harness = false`
//! because the libtest harness itself spawns threads, which would
//! poison the single-thread assertion.
//!
//! (The symbol half — a no-spawn corpus binary carries no scheduler
//! symbols — is wolf_driver/tests/no_spawn_binary.rs, over the
//! `--gc-sections` link.)

use wolf_rt::task;

#[cfg(target_os = "linux")]
fn os_thread_count() -> usize {
    std::fs::read_dir("/proc/self/task").map_or(1, |d| d.count())
}

#[cfg(not(target_os = "linux"))]
fn os_thread_count() -> usize {
    1 // No /proc equivalent asserted off-linux; the lazily-init
    // assertions below still run.
}

fn main() {
    // Before ANY runtime use: exactly the main thread, no pool.
    assert_eq!(
        os_thread_count(),
        1,
        "background threads exist before first spawn"
    );
    assert!(!task::initialized(), "pool initialized before first spawn");

    // A scope WITHOUT spawns must not bring up the pool either
    // (join of an empty scope is a no-op, not a scheduler use).
    let r = task::scope("empty", |_| ());
    assert!(r.is_ok());
    assert!(!task::initialized(), "an empty scope initialized the pool");
    assert_eq!(os_thread_count(), 1, "an empty scope created threads");

    // First spawn: the pool comes up, lazily, now.
    let r = task::scope("first", |s| {
        s.spawn("t", |_| task::ExitReason::Normal);
    });
    assert!(r.is_ok());
    assert!(task::initialized(), "spawn did not initialize the pool");
    assert!(os_thread_count() > 1, "spawn created no worker threads");

    let (target, running, _) = task::counters();
    assert!(running >= 1 && running <= (target * 8).max(target + 4));
    println!("no_spawn: ok (target={target}, running={running})");
}
