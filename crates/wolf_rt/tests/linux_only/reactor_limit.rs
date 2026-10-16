//! s239 (wolf-lang#750): the reactor at the descriptor limit. Until s239
//! the readiness set was made at the first wait and its creation was
//! ASSERTED, so a process that had used up its descriptors before it
//! ever parked ended there (`epoll_create1 failed`, a panic in a
//! function that cannot unwind). Now the making answers the host's
//! refusal: a wait meets `Unavailable(EMFILE)`, a net call its `io`
//! row, [`reactor::start`] the error — and the set is made by the first
//! call after descriptors come back.
//!
//!
//! s243 (wolf-lang#763): the signal hub likewise. Until s243 the first
//! `listen` or `wait` made its two pipes and its drain thread and
//! PANICKED at a refusal; now they answer `io`, [`signal::start`] the
//! error, and the hub is made by the first call after descriptors come
//! back.
//!
//! `harness = false`: the reactor is one per process and is made once,
//! so each case runs in a child of its own — this binary re-executed —
//! and the child lowers ITS OWN descriptor limit (never the test
//! runner's, never the host's) before it fills the table.

use std::process::Command;
use std::time::{Duration, Instant};
use wolf_rt::signal::{self, meaning, sig_code};
use wolf_rt::{net, reactor};

const CHILD: &str = "WOLF_RT_REACTOR_LIMIT_CASE";

pub fn main() {
    if let Ok(case) = std::env::var(CHILD) {
        match case.as_str() {
            "park" => park(),
            "start" => start(),
            "hub" => hub(),
            other => panic!("unknown case {other}"),
        }
        return;
    }
    let exe = std::env::current_exe().expect("this test's own binary");
    for case in ["park", "start", "hub"] {
        let out = Command::new(&exe)
            .env(CHILD, case)
            .output()
            .expect("re-execute this test");
        let so = String::from_utf8_lossy(&out.stdout);
        let se = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success() && so.contains(&format!("{case}: ok")),
            "case {case}: {:?}\nstdout:\n{so}\nstderr:\n{se}",
            out.status
        );
        print!("{so}");
    }
    println!(
        "reactor_limit: ok (no abort at the limit; the set and the hub are made once descriptors \
         return)"
    );
}

/// This process's own descriptor limit, lowered to `n` (the hard limit
/// left alone).
fn lower(n: libc::rlim_t) {
    let mut r = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: a live rlimit to fill and then hand back.
    unsafe {
        assert_eq!(libc::getrlimit(libc::RLIMIT_NOFILE, &mut r), 0);
        r.rlim_cur = n.min(r.rlim_max);
        assert_eq!(libc::setrlimit(libc::RLIMIT_NOFILE, &r), 0);
    }
}

/// Open the null device until the host refuses with EMFILE.
fn fill() -> Vec<std::fs::File> {
    let mut held = Vec::new();
    loop {
        match std::fs::File::open("/dev/null") {
            Ok(f) => held.push(f),
            Err(e) => {
                assert_eq!(e.raw_os_error(), Some(libc::EMFILE), "the refusal: {e}");
                return held;
            }
        }
    }
}

fn park() {
    lower(64);
    let mut t = net::NetTable::new();
    let srv = t.listen("127.0.0.1:0").expect("listen");
    let port = t.port(srv).expect("port");
    let cli = t.connect(&format!("127.0.0.1:{port}")).expect("connect");
    let _conn = t.accept(srv).expect("accept");
    t.set_deadline(cli, 50).expect("deadline");
    let held = fill();
    assert!(
        !reactor::initialized(),
        "the set was made before the first wait"
    );
    assert_eq!(
        reactor::sleep_until(Instant::now() + Duration::from_millis(5)),
        reactor::IoWait::Unavailable(libc::EMFILE),
        "a wait at the limit"
    );
    assert_eq!(
        t.read(cli, 16),
        Err("io"),
        "a read that must park, at the limit"
    );
    assert!(
        !reactor::initialized(),
        "a refused making left a reactor behind"
    );
    drop(held);
    assert_eq!(
        t.read(cli, 16),
        Err("timeout"),
        "the same read with descriptors back"
    );
    assert!(
        reactor::initialized(),
        "the first wait after the limit made the set"
    );
    println!("park: ok");
}

fn start() {
    lower(64);
    let held = fill();
    let e = reactor::start().expect_err("start at the limit");
    assert_eq!(e.raw_os_error(), Some(libc::EMFILE), "start's refusal: {e}");
    assert!(!reactor::initialized());
    drop(held);
    reactor::start().expect("start with descriptors back");
    assert!(reactor::initialized());
    reactor::start().expect("start again is a no-op");
    println!("start: ok");
}

/// s243 (#763): the signal hub at the limit, in a process that did not
/// start it (no compiled program is one: the entry shim starts it).
fn hub() {
    lower(64);
    let held = fill();
    assert_eq!(
        signal::listen(meaning::RELOAD),
        sig_code::IO,
        "a listen at the limit"
    );
    assert_eq!(
        signal::wait(meaning::RELOAD),
        -sig_code::IO,
        "a wait at the limit"
    );
    let e = signal::start().expect_err("the hub's start at the limit");
    assert_eq!(e.raw_os_error(), Some(libc::EMFILE), "start's refusal: {e}");
    drop(held);
    signal::start().expect("start with descriptors back");
    signal::start().expect("start again is a no-op");
    assert_eq!(
        signal::listen(meaning::RELOAD),
        sig_code::OK,
        "the listen with descriptors back"
    );
    assert_eq!(signal::raise(meaning::RELOAD), sig_code::OK, "the raise");
    assert_eq!(
        signal::wait(meaning::RELOAD),
        meaning::RELOAD,
        "the wait after the raise"
    );
    println!("hub: ok");
}
