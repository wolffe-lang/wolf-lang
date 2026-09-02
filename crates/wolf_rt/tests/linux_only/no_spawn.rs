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

#[path = "thread_count.rs"]
mod thread_count;
use thread_count::os_thread_count;

pub fn main() {
    // Before ANY runtime use: exactly the main thread, no pool. On
    // windows the process may already carry threads that are not
    // ours (the loader's thread pool, an injected DLL's) — the claim
    // there is that WOLF seats none until the first spawn, measured
    // as a delta against whatever the host started us with.
    let base = os_thread_count();
    #[cfg(not(windows))]
    assert_eq!(base, 1, "background threads exist before first spawn");
    assert!(!task::initialized(), "pool initialized before first spawn");

    // A scope WITHOUT spawns must not bring up the pool either
    // (join of an empty scope is a no-op, not a scheduler use).
    let r = task::scope("empty", |_| ());
    assert!(r.is_ok());
    assert!(!task::initialized(), "an empty scope initialized the pool");
    assert_eq!(os_thread_count(), base, "an empty scope created threads");

    // First spawn: the pool comes up, lazily, now.
    let r = task::scope("first", |s| {
        s.spawn("t", |_| task::ExitReason::Normal);
    });
    assert!(r.is_ok());
    assert!(task::initialized(), "spawn did not initialize the pool");
    assert!(os_thread_count() > base, "spawn created no worker threads");

    let (target, running, _) = task::counters();
    assert!(running >= 1 && running <= (target * 8).max(target + 4));
    println!("no_spawn: ok (target={target}, running={running})");
}
