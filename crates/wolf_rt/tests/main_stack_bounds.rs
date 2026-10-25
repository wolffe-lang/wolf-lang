//! s265 (wolf-lang#918): the main thread's stack guard that the entry
//! shim registers (`__wolf_rt_stack_start` → `enter_main_thread`) is
//! read from `/proc/self/maps` and `RLIMIT_STACK` directly; this holds
//! it to the bounds `pthread_getattr_np(3)` reports for the same thread,
//! which is what s256 registered. Linux only; `harness = false` so the
//! check runs on the process's main thread, never a libtest worker.
//!
//! The parent checks itself, then re-executes this binary under each
//! case that moves the answer: a smaller and a larger stack limit, one
//! that is not a whole number of pages, no
//! limit at all (the host then lays mappings out the legacy way, so the
//! mapping beneath the stack is the floor), and a 1 MiB environment
//! (the stack's top moves away from the frame). A limit the host will
//! not grant is a loud SKIP of that case, never a silent pass.

#[cfg(target_os = "linux")]
fn main() {
    use std::os::unix::process::CommandExt as _;
    use std::process::Command;
    if std::env::args().any(|a| a == "--child") {
        std::process::exit(if check("child") { 0 } else { 1 });
    }
    let mut ok = check("parent");
    let me = std::env::current_exe().expect("current_exe");
    let cases: [(&str, Option<libc::rlim_t>, usize); 5] = [
        ("stack limit 4 MiB", Some(4 << 20), 0),
        ("stack limit 8191 KiB (not a page multiple)", Some(8191 << 10), 0),
        ("stack limit 64 MiB", Some(64 << 20), 0),
        ("no stack limit", Some(libc::RLIM_INFINITY), 0),
        ("a 1 MiB environment", None, 1 << 20),
    ];
    for (name, limit, env) in cases {
        // SAFETY: a plain read of our own limit.
        let hard = unsafe {
            let mut r: libc::rlimit = std::mem::zeroed();
            libc::getrlimit(libc::RLIMIT_STACK, &mut r);
            r.rlim_max
        };
        if let Some(l) = limit
            && hard != libc::RLIM_INFINITY
            && l > hard
        {
            eprintln!("main_stack_bounds: SKIP {name}: the hard limit is {hard}");
            continue;
        }
        let mut cmd = Command::new(&me);
        cmd.arg("--child");
        // One environment string may not pass 128 KiB (execve(2)'s
        // per-string limit), so the pad is several.
        for i in 0..env.div_ceil(120_000) {
            cmd.env(format!("S265_PAD_{i}"), "x".repeat(120_000));
        }
        if let Some(l) = limit {
            // SAFETY: setrlimit(2) is a single system call in the child
            // between fork and exec; nothing allocates.
            unsafe {
                cmd.pre_exec(move || {
                    let mut r: libc::rlimit = std::mem::zeroed();
                    libc::getrlimit(libc::RLIMIT_STACK, &mut r);
                    r.rlim_cur = l;
                    if libc::setrlimit(libc::RLIMIT_STACK, &r) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        let st = cmd.status().expect("spawn the child");
        eprintln!("main_stack_bounds: {name}: {st}");
        ok &= st.success();
    }
    assert!(ok, "the cheap main-thread guard differs from pthread_getattr_np's");
    println!("main_stack_bounds: ok");
}

#[cfg(target_os = "linux")]
fn check(who: &str) -> bool {
    let (cheap, pthread) = wolf_rt::task::main_guard_both();
    eprintln!("main_stack_bounds: {who}: cheap {cheap:x?} pthread {pthread:x?}");
    cheap.is_some() && cheap == pthread
}

#[cfg(not(target_os = "linux"))]
fn main() {}
