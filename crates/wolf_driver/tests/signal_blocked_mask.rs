//! s188 (wolf-lang#483) — a program answers the signals it arms even
//! when it inherits them blocked.
//!
//! A process inherits its signal mask across `fork` and `exec`. lobo,
//! started from fish with `ssh -f … &` on kasumi, inherited SIGINT and
//! SIGQUIT blocked (`SigBlk 0x6` on every thread), so the `sigaction`
//! handler `os_signal_listen(QUIT)` installed could never run:
//! `kill -QUIT` stayed pending forever (`ShdPnd 0x4`) and the graceful
//! quit never happened. nginx sets its own mask and answers whatever it
//! inherits. The runtime now unblocks each signal a program arms, on
//! the one thread it owns for the program's whole life (the drain
//! thread, `wolf-signal`), and leaves every other bit of every thread's
//! mask as inherited.
//!
//! The harness is the launcher: `sigprocmask` in the forked child,
//! before `exec`, blocks the set, exactly as an inheriting shell would.
//! Each run records the process's and every thread's `SigBlk` where the
//! host exposes them (`/proc` on linux) and prints them, so a log says
//! what mask each run saw. A run that does not answer within its
//! deadline is killed by its own pid and fails with the pending set.
//!
//! Unix only: windows has no signal masks (`[os.signal.platform]`
//! delivers console control events there).

#![cfg(unix)]

mod lane_exit;

use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The native tiers link `libwolf_rt.a` found next to `wolf`; `cargo
/// test` alone does not refresh it, and a stale runtime would test the
/// code before the fix (`conc_native.rs`'s helper, same reason).
fn ensure_rt_staticlib() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .expect("cargo builds wolf_rt");
        assert!(status.success(), "wolf_rt staticlib build failed");
    });
}

/// `wolf build [--release]` the program into its own directory. `None`
/// (a loud SKIP) only when the host cannot build that tier.
fn build(case: &str, src: &str, release: bool) -> Option<PathBuf> {
    ensure_rt_staticlib();
    let tier = if release { "release" } else { "native" };
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("s188")
        .join(format!("{case}-{tier}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join(format!("{case}.lu"));
    std::fs::write(&entry, src).expect("write fixture");
    let exe = dir.join(case);
    let mut cmd = Command::new(wolf());
    cmd.arg("build").arg(&entry);
    if release {
        cmd.arg("--release");
    }
    let out = cmd.arg("-o").arg(&exe).output().expect("wolf runs");
    if out.status.success() {
        return Some(exe);
    }
    if lane_exit::environment_refusal(&out, &format!("wolf build ({tier})")) {
        eprintln!(
            "SKIP: this host cannot build the {tier} tier: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    panic!(
        "wolf build ({tier}) failed on {case}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// One run of a built program under an inherited mask.
struct Run {
    child: Child,
    lines: Receiver<String>,
    what: String,
}

/// Start `exe` with exactly `blocked` in its signal mask. The mask is
/// set in the forked child before `exec` (`sigprocmask` is
/// async-signal-safe), so the program inherits it the way a job
/// inherits its shell's.
fn launch(exe: &Path, blocked: &[libc::c_int], what: &str) -> Run {
    let set: Vec<libc::c_int> = blocked.to_vec();
    let mut cmd = Command::new(exe);
    cmd.stdout(Stdio::piped()).stderr(Stdio::inherit());
    // SAFETY: the closure runs in the forked child before exec and calls
    // only async-signal-safe functions on a stack-local sigset.
    unsafe {
        cmd.pre_exec(move || {
            let mut s: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut s);
            for &sig in &set {
                libc::sigaddset(&mut s, sig);
            }
            if libc::sigprocmask(libc::SIG_SETMASK, &s, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = cmd.spawn().expect("the program starts");
    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    Run {
        child,
        lines: rx,
        what: what.to_string(),
    }
}

/// The process's pending and blocked sets and each thread's mask, as
/// `/proc` reports them (linux); empty elsewhere.
fn masks(pid: u32) -> String {
    let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
        return String::new();
    };
    let mut out: Vec<String> = status
        .lines()
        .filter(|l| l.starts_with("SigPnd") || l.starts_with("ShdPnd") || l.starts_with("SigBlk"))
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    for (name, blk) in thread_masks(pid) {
        out.push(format!("thread {name} SigBlk {blk:016x}"));
    }
    out.join("; ")
}

/// Each thread's name and blocked set (linux `/proc`); empty elsewhere.
fn thread_masks(pid: u32) -> Vec<(String, u64)> {
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
        return Vec::new();
    };
    let mut v = Vec::new();
    for t in tasks.flatten() {
        let p = t.path();
        let name = std::fs::read_to_string(p.join("comm"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let status = std::fs::read_to_string(p.join("status")).unwrap_or_default();
        let blk = status
            .lines()
            .find_map(|l| l.strip_prefix("SigBlk:"))
            .and_then(|h| u64::from_str_radix(h.trim(), 16).ok());
        if let Some(blk) = blk {
            v.push((name, blk));
        }
    }
    v
}

/// The process-wide pending set (linux `ShdPnd`), when readable.
fn shared_pending(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status
        .lines()
        .find_map(|l| l.strip_prefix("ShdPnd:"))
        .and_then(|h| u64::from_str_radix(h.trim(), 16).ok())
}

/// The `/proc` mask bit for a signal number.
fn bit(sig: libc::c_int) -> u64 {
    1u64 << (sig - 1)
}

impl Run {
    /// The next line, or a failure that kills the program (its own pid)
    /// and names what was pending.
    fn expect_line(&mut self, want: &str, within: Duration) {
        match self.lines.recv_timeout(within) {
            Ok(line) => assert_eq!(line, want, "{}: stdout", self.what),
            Err(RecvTimeoutError::Timeout) => self.fail(&format!(
                "no `{want}` within {within:?} — a signal the program armed never reached it \
                 (wolf-lang#483)"
            )),
            Err(RecvTimeoutError::Disconnected) => {
                let status = self.child.wait().expect("reap");
                panic!("{}: stdout closed before `{want}` ({status})", self.what);
            }
        }
    }

    fn send(&self, sig: libc::c_int) {
        let pid = libc::pid_t::try_from(self.child.id()).expect("pid fits");
        // SAFETY: a signal to our own child, by its pid.
        let rc = unsafe { libc::kill(pid, sig) };
        assert_eq!(rc, 0, "{}: kill({sig})", self.what);
    }

    /// The program exits 0 within `within`.
    fn expect_exit_zero(&mut self, within: Duration) {
        let deadline = Instant::now() + within;
        loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                assert!(status.success(), "{}: exit {status}", self.what);
                return;
            }
            if Instant::now() >= deadline {
                self.fail(&format!("still running {within:?} after its last signal"));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn fail(&mut self, why: &str) -> ! {
        let state = masks(self.child.id());
        let _ = self.child.kill();
        let _ = self.child.wait();
        panic!("{}: {why}; {state}", self.what);
    }
}

const READY: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(20);

/// Listen for QUIT, say so, wait for it.
const QUIT_ONLY: &str = r#"
fn main() -> !int {
    os_signal_listen(4)?
    print("ready")
    let m = os_signal_wait(4)?
    print("got={m}")
    0
}
"#;

/// Listen for all four meanings, then report four arrivals in order.
const ALL_FOUR: &str = r#"
fn main() -> !int {
    os_signal_listen(15)?
    print("ready")
    var i = 0
    while i < 4 {
        let m = os_signal_wait(15)?
        print("m={m}")
        i = i + 1
    }
    0
}
"#;

/// The issue's shape, with an unarmed signal beside it. The program
/// arms QUIT and inherits SIGHUP, SIGINT and SIGQUIT blocked. SIGHUP
/// is sent first and must stay pending (the program never armed it,
/// and its default action would end the process); then `kill -QUIT` is
/// answered. On linux the masks are read after `ready`: the drain
/// thread alone has QUIT unblocked, and SIGHUP and SIGINT stay blocked
/// on every thread.
fn blocked_quit_answered(release: bool) {
    let Some(exe) = build("blocked_quit", QUIT_ONLY, release) else {
        return;
    };
    let tier = if release { "release" } else { "native" };

    // Control: a clean mask answers QUIT (unchanged behaviour).
    let mut clean = launch(&exe, &[], &format!("{tier}, clean mask"));
    clean.expect_line("ready", READY);
    eprintln!("{}: {}", clean.what, masks(clean.child.id()));
    clean.send(libc::SIGQUIT);
    clean.expect_line("got=4", ANSWER);
    clean.expect_exit_zero(ANSWER);

    let blocked = [libc::SIGHUP, libc::SIGINT, libc::SIGQUIT];
    let mut run = launch(&exe, &blocked, &format!("{tier}, HUP+INT+QUIT blocked"));
    run.expect_line("ready", READY);
    let pid = run.child.id();
    eprintln!("{} after ready: {}", run.what, masks(pid));
    // Read now, asserted after the run: the answer is the issue, the
    // masks say why.
    let threads = thread_masks(pid);
    run.send(libc::SIGHUP);
    std::thread::sleep(Duration::from_millis(200));
    if let Some(pending) = shared_pending(pid) {
        assert_eq!(
            pending & bit(libc::SIGHUP),
            bit(libc::SIGHUP),
            "{}: the unarmed SIGHUP stays pending, never delivered",
            run.what
        );
    }
    run.send(libc::SIGQUIT);
    run.expect_line("got=4", ANSWER);
    run.expect_exit_zero(ANSWER);
    if !threads.is_empty() {
        let drain: Vec<_> = threads.iter().filter(|(n, _)| n == "wolf-signal").collect();
        assert_eq!(
            drain.len(),
            1,
            "{}: one drain thread: {threads:?}",
            run.what
        );
        let armed = bit(libc::SIGQUIT);
        let unarmed = bit(libc::SIGHUP) | bit(libc::SIGINT);
        for (name, blk) in &threads {
            assert_eq!(
                blk & unarmed,
                unarmed,
                "{}: thread {name} lost an unarmed signal from its inherited mask ({blk:016x})",
                run.what
            );
            let want = if name == "wolf-signal" { 0 } else { armed };
            assert_eq!(
                blk & armed,
                want,
                "{}: thread {name}'s QUIT bit ({blk:016x}) — only the drain thread unblocks it",
                run.what
            );
        }
    }
}

#[test]
fn a_blocked_quit_is_answered_once_armed_native() {
    blocked_quit_answered(false);
}

#[test]
fn a_blocked_quit_is_answered_once_armed_release() {
    blocked_quit_answered(true);
}

/// Every meaning a program arms is unblocked, not only QUIT: all four
/// signals and SIGINT inherited blocked, all four armed, each sent in
/// turn and each reported. SIGINT, never armed, stays blocked on every
/// thread (linux).
#[test]
fn every_armed_meaning_arrives_through_a_blocked_mask() {
    let Some(exe) = build("all_four", ALL_FOUR, false) else {
        return;
    };
    let blocked = [
        libc::SIGHUP,
        libc::SIGINT,
        libc::SIGQUIT,
        libc::SIGTERM,
        libc::SIGUSR2,
    ];
    let mut run = launch(&exe, &blocked, "native, HUP+INT+QUIT+TERM+USR2 blocked");
    run.expect_line("ready", READY);
    let pid = run.child.id();
    eprintln!("{} after ready: {}", run.what, masks(pid));
    let threads = thread_masks(pid);
    for (sig, m) in [
        (libc::SIGHUP, 1),
        (libc::SIGTERM, 2),
        (libc::SIGQUIT, 4),
        (libc::SIGUSR2, 8),
    ] {
        run.send(sig);
        run.expect_line(&format!("m={m}"), ANSWER);
    }
    run.expect_exit_zero(ANSWER);
    let armed = bit(libc::SIGHUP) | bit(libc::SIGQUIT) | bit(libc::SIGTERM) | bit(libc::SIGUSR2);
    for (name, blk) in &threads {
        assert_ne!(
            blk & bit(libc::SIGINT),
            0,
            "{}: thread {name} lost SIGINT, which the program never armed ({blk:016x})",
            run.what
        );
        let want = if name == "wolf-signal" { 0 } else { armed };
        assert_eq!(
            blk & armed,
            want,
            "{}: thread {name}'s armed bits ({blk:016x}) — only the drain thread unblocks them",
            run.what
        );
    }
}
