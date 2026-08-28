//! s35's D15 lazy-lifecycle check — the no-spawn family widened to
//! io: a wolf program that performs no io carries NO reactor (no
//! thread, no epoll state), even once the task pool is up; the FIRST
//! io wait brings up exactly one reactor thread, lazily.
//! `harness = false` because the assertions count OS threads, so the
//! harness itself may not spawn any.

use std::time::{Duration, Instant};
use wolf_rt::{net, reactor, task};

#[cfg(target_os = "linux")]
fn os_thread_count() -> usize {
    std::fs::read_dir("/proc/self/task").map_or(1, |d| d.count())
}

/// macOS: `proc_pidinfo(PROC_PIDTASKINFO)` — the task's thread count
/// straight from the kernel (no /proc here), s59.
#[cfg(target_os = "macos")]
fn os_thread_count() -> usize {
    // SAFETY: zeroed out-struct of the exact size the call contracts.
    unsafe {
        let mut ti: libc::proc_taskinfo = std::mem::zeroed();
        let sz = size_of::<libc::proc_taskinfo>() as i32;
        let n = libc::proc_pidinfo(
            std::process::id() as i32,
            libc::PROC_PIDTASKINFO,
            0,
            (&raw mut ti).cast(),
            sz,
        );
        if n == sz {
            ti.pti_threadnum as usize
        } else {
            1
        }
    }
}

pub fn main() {
    // Before ANY runtime use: no reactor, no pool, one thread.
    assert!(!reactor::initialized(), "reactor up before any io");
    assert_eq!(os_thread_count(), 1, "threads exist before any runtime use");

    // Task machinery WITHOUT io: the pool comes up; the reactor must
    // not (io is its own lazy tier — D15 pay-for-what-you-use).
    let r = task::scope("compute", |s| {
        for i in 0..4 {
            s.spawn(&format!("t{i}"), |_| task::ExitReason::Normal);
        }
    });
    assert!(r.is_ok());
    assert!(task::initialized(), "pool did not come up");
    assert!(!reactor::initialized(), "task use initialized the reactor");

    // A net table, a listener, even a port query are not io WAITS:
    // the reactor stays down until something parks.
    let mut t = net::NetTable::new();
    let srv = t.listen("127.0.0.1:0").expect("listen");
    let _port = t.port(srv).expect("port");
    assert!(!reactor::initialized(), "listen initialized the reactor");

    let before = os_thread_count();

    // The FIRST wait brings the reactor up: exactly one new thread.
    let out = reactor::sleep_until(Instant::now() + Duration::from_millis(5));
    assert_eq!(out, reactor::IoWait::TimedOut);
    assert!(
        reactor::initialized(),
        "the first wait did not initialize the reactor"
    );
    assert_eq!(
        os_thread_count(),
        before + 1,
        "reactor lifecycle: expected exactly one reactor thread"
    );
    println!("no_io: ok (reactor lazy; one thread after first wait)");
}
