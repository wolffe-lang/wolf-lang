//! s35's D15 lazy-lifecycle check — the no-spawn family widened to
//! io: a wolf program that performs no io carries NO reactor (no
//! thread, no epoll state), even once the task pool is up; the FIRST
//! io wait brings up exactly one reactor thread, lazily.
//! `harness = false` because the assertions count OS threads, so the
//! harness itself may not spawn any.

use std::time::{Duration, Instant};
use wolf_rt::{net, reactor, task};

#[path = "thread_count.rs"]
mod thread_count;
use thread_count::os_thread_count;

pub fn main() {
    // Before ANY runtime use: no reactor, no pool, one thread — on
    // windows, no thread of OURS: the loader seats threads that are
    // not wolf's (the no_spawn twin's measured finding, s60b), so the
    // claim there is a delta, taken below at each lifecycle edge.
    assert!(!reactor::initialized(), "reactor up before any io");
    #[cfg(not(windows))]
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
