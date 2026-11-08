//! s286 (wolf-lang#1051): the arena limit the runtime settles is glibc's
//! own, and a limit the user set is left alone.
//!
//! glibc reads the CPU count for its arena limit the first time a thread
//! needs an arena past its test bound (`M_ARENA_TEST`, 8 on 64-bit), and
//! that read opens a file on the lowest free descriptor while the program
//! runs (#1051). The runtime now caps the arenas at the bound in the entry
//! shim's first call (`__wolf_rt_main_start`), so glibc never runs its
//! test, and sets glibc's documented limit, `8 * get_nprocs()`, when the
//! pool first starts. `M_ARENA_TEST` is not used once `M_ARENA_MAX` is
//! nonzero (mallopt(3)).
//!
//! Each child here holds `8 * get_nprocs() + 8` threads that have all
//! allocated at once, so every arena glibc allows is in use, and counts
//! them off `malloc_stats(3)`, which prints one `Arena N:` block per arena
//! on standard error:
//!
//!   control   no runtime call: glibc's own limit on this host.
//!   settled   the entry call, then the pool's start: must equal control.
//!   entry     the entry call alone, no pool: stops at the test bound, 8.
//!
//! and the skip: with `MALLOC_ARENA_MAX=2`, with
//! `GLIBC_TUNABLES=glibc.malloc.arena_max=2`, and with that item after
//! another, the settled child holds 2.
//!
//! `harness = false`: libtest's own threads would hold arenas of their own,
//! and the entry call is made once, before any thread, as the shim makes it.

use std::process::Command;
use std::sync::{Arc, Barrier};

unsafe extern "C" {
    /// get_nprocs(3): the processors online, the count glibc's limit uses.
    fn get_nprocs() -> libc::c_int;
}

/// glibc's documented test bound and per-CPU multiple: 8 where `long` is
/// 8 bytes, 2 where it is 4 (mallopt(3), the manual's
/// `glibc.malloc.arena_max`).
const PER_CPU: usize = if cfg!(target_pointer_width = "64") {
    8
} else {
    2
};

/// The environment names that set glibc's arena policy, removed from every
/// child so the host's own setting cannot decide a row.
const ARENA_VARS: [&str; 3] = ["MALLOC_ARENA_MAX", "MALLOC_ARENA_TEST", "GLIBC_TUNABLES"];

pub fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--child") {
        child(&args[2], args[3].parse().expect("a thread count"));
        return;
    }
    // SAFETY: a plain query.
    let n = usize::try_from(unsafe { get_nprocs() }).unwrap_or(1).max(1);
    let threads = PER_CPU * n + 8;
    let control = arenas("control", threads, &[]);
    let settled = arenas("settled", threads, &[]);
    let entry = arenas("entry", threads, &[]);
    eprintln!(
        "REPORT arena_limit: get_nprocs {n}, {threads} threads: control {control} \
         ({PER_CPU} * get_nprocs = {}), settled {settled}, entry alone {entry}",
        PER_CPU * n
    );
    assert_eq!(
        settled, control,
        "#1051: the limit the runtime settles is glibc's own on this host"
    );
    assert_eq!(
        entry, PER_CPU,
        "#1051: before any pool, the runtime caps the arenas at glibc's test bound"
    );
    for (name, value) in [
        ("MALLOC_ARENA_MAX", "2"),
        ("GLIBC_TUNABLES", "glibc.malloc.arena_max=2"),
        (
            "GLIBC_TUNABLES",
            "glibc.malloc.trim_threshold=131072:glibc.malloc.arena_max=2",
        ),
    ] {
        let got = arenas("settled", threads, &[(name, value)]);
        eprintln!("REPORT arena_limit: settled with {name}={value}: {got}");
        assert_eq!(
            got, 2,
            "#1051: the user's own limit ({name}={value}) is left alone"
        );
    }
    eprintln!("arena_limit: the settled limit is glibc's own and the user's limit stands");
}

/// The arenas a child of `mode` holds, counted off its `malloc_stats` report.
fn arenas(mode: &str, threads: usize, env: &[(&str, &str)]) -> usize {
    let me = std::env::current_exe().expect("current_exe");
    let mut cmd = Command::new(me);
    cmd.arg("--child").arg(mode).arg(threads.to_string());
    for k in ARENA_VARS {
        cmd.env_remove(k);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("the child runs");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "the {mode} child failed: {err}");
    let count = err.lines().filter(|l| l.starts_with("Arena ")).count();
    assert!(
        count > 0,
        "the {mode} child's malloc_stats named no arena: {err}"
    );
    count
}

/// Make the runtime's calls `mode` names, then hold `threads` threads that
/// have each allocated, report the arenas, and let them go.
fn child(mode: &str, threads: usize) {
    match mode {
        "control" => {}
        // SAFETY: the shim's first call, made once before any thread, with
        // no arguments and no environment array (both allowed null).
        "entry" => unsafe {
            wolf_rt::native::__wolf_rt_main_start(0, std::ptr::null(), std::ptr::null())
        },
        "settled" => {
            // SAFETY: as above.
            unsafe { wolf_rt::native::__wolf_rt_main_start(0, std::ptr::null(), std::ptr::null()) };
            let r = wolf_rt::task::scope("s286", |s| {
                s.spawn("w", |_| wolf_rt::task::ExitReason::Normal);
            });
            assert!(r.is_ok(), "the pool ran a task");
        }
        other => panic!("unknown child mode {other}"),
    }
    let arrived = Arc::new(Barrier::new(threads + 1));
    let release = Arc::new(Barrier::new(threads + 1));
    let held: Vec<_> = (0..threads)
        .map(|_| {
            let (arrived, release) = (arrived.clone(), release.clone());
            std::thread::spawn(move || {
                // Above the per-thread cache's largest size, so the request
                // goes to the thread's arena.
                let v: Vec<u8> = std::hint::black_box(Vec::with_capacity(4096));
                arrived.wait();
                release.wait();
                drop(v);
            })
        })
        .collect();
    arrived.wait();
    // SAFETY: a report on standard error.
    unsafe { libc::malloc_stats() };
    release.wait();
    for h in held {
        h.join().expect("a held thread ends");
    }
}
