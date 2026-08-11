//! s34 — `[conc.proc.root]`: the root supervisor's domain is the
//! process. A proc linked to the root domain exits abnormally ⇒ the
//! root domain dies ⇒ the killed-proc sequence runs for every live
//! proc (their regions bulk-free, their defers do NOT run) and the
//! process terminates with a NONZERO, implementation-specified
//! status. The death path calls `process::exit`, so it must run in a
//! child process: `harness = false` and a re-exec of this same binary
//! under `WOLF_RT_PROC_ROOT_CHILD=1`.

use std::process::Command;
use std::time::Duration;

use wolf_rt::task::{Chan, ProcOutcome, ROOT_DEATH_EXIT, ROOT_DOMAIN, link, spawn_proc};

const CHILD_ENV: &str = "WOLF_RT_PROC_ROOT_CHILD";

fn child() -> ! {
    // A daemon-shaped proc, alive and blocked, with an owned region:
    // the root death must run ITS killed-proc sequence too — no
    // further user code, so the marker after its recv never prints.
    let _daemon = spawn_proc("daemon", |_| {
        let h = wolf_rt::native::__wolf_rt_region_new();
        // SAFETY: fresh live handle; the proc ledger owns it and the
        // root-death sequence bulk-frees it.
        unsafe {
            wolf_rt::native::__wolf_rt_region_alloc(h, 4096);
        }
        let ch = Chan::new(0);
        let _ = ch.recv();
        eprintln!("daemon-defer-ran"); // must NOT appear: kill skips user code
        ProcOutcome::Value(0)
    });
    std::thread::sleep(Duration::from_millis(50)); // let the daemon park

    // The doomed proc is fate-coupled to the root domain (`w.link()`
    // from main — `[conc.proc.root]`) and exits abnormally.
    let doomed = spawn_proc("doomed", |_| ProcOutcome::Fail { tag: 9 });
    link(doomed, ROOT_DOMAIN).expect("live proc links");

    // The root death sequence must terminate this process; if it does
    // not, exit 0 so the parent sees the wrong class and fails loud.
    std::thread::sleep(Duration::from_secs(20));
    std::process::exit(0)
}

pub fn main() {
    if std::env::var(CHILD_ENV).is_ok() {
        child();
    }
    let exe = std::env::current_exe().expect("own path");
    let out = Command::new(exe)
        .env(CHILD_ENV, "1")
        .output()
        .expect("child runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Outcome class first (`[conf.trap.exit]` discipline: nonzero is
    // the contract)...
    assert!(
        !out.status.success(),
        "root-domain death must exit nonzero; stderr: {stderr}"
    );
    // ...then the implementation's own pinned status (a regression
    // guard on OUR constant, not spec).
    assert_eq!(out.status.code(), Some(ROOT_DEATH_EXIT), "stderr: {stderr}");
    assert!(
        stderr.contains("root supervisor domain died"),
        "missing the root-death report; stderr: {stderr}"
    );
    assert!(
        !stderr.contains("daemon-defer-ran"),
        "a killed proc ran user code past its blocking point ([conc.proc.kill]); \
         stderr: {stderr}"
    );
    println!("proc_root: ok (root death exits {ROOT_DEATH_EXIT}, killed procs skip defers)");
}
