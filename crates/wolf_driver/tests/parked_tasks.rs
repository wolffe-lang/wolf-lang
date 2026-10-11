//! s231 (wolf-lang#713) — a program that parks one task per connection.
//!
//! A task parked in a runtime wait (`net_read`, `net_accept`, a channel
//! `recv`) holds its OS thread (`[conc.task.order]`), and the pool adds a
//! worker for each one so the program keeps running. Until s231 that
//! compensation stopped at a fixed cap, `max(8 t, t + 4)` workers with
//! `t = max(2, cpus)`: 16 on one or two cpus, 32 on four, 128 on sixteen.
//! Once every worker held a parked task, a task that would have woken
//! them sat in a queue nothing took, and the program stopped.
//!
//! Measured at trunk 6143943d on kasumi (16 cpus), the issue's witness
//! (`fixtures/parked_tasks/waitwolf`, `loop 0 N` holds 2N+1 tasks):
//! `taskset -c 0` passes N = 7 and stalls at 8, `-c 0-3` passes 15 and
//! stalls at 16, all cores pass 63 and stall at 64 — `2N + 1 > cap`
//! exactly, on the native and release tiers alike. A gdb stop at N = 8
//! on one cpu showed all 16 workers inside a runtime wait (8 in
//! `net_read`, 8 in a channel recv) and the eighth server task queued.
//! lupin 0.1.50 and the checked machine have no such pool and answer.
//!
//! The gates:
//! - the witness completes, native and release, on every cpu set this
//!   host can pin: `loop 0 64` under `taskset -c 0` and `loop 0 1000`
//!   under `-c 0-3` and on all cores (linux); all cores elsewhere;
//! - every tier answers it alike at N = 64: the checked machine
//!   (`wolf conform-run --checked --json`; `wolf run --checked` is the
//!   native build) and lupin print the line the compiled tiers print;
//! - the soak: 10,000 tasks parked in a channel recv, in `net_read` and
//!   in `net_accept` (2,000 on macOS, whose kernel caps a process at
//!   4,096 threads) are held and released, every one answering; on
//!   linux the parked process is sampled from /proc: its threads, its
//!   RSS (reported per parked task) and the cpu it burns while every
//!   task is parked, which must be near zero — a parked task waits for
//!   an event, never a timer (trunk's waits woke every 5 ms).
//!
//! A stalled run is killed at its deadline and FAILS the gate; there is
//! no skip on linux (`taskset` is part of the host) and none for a
//! missing lupin under `WOLF_PAIRING_REQUIRE_SIBLING`.

mod lane_exit;

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

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

/// The witness's last line for `loop 0 n`.
fn want(n: usize) -> String {
    format!(
        "waitwolf loop: {n} connections at once, {n} requests answered, {n} answered right, \
         {n} ended as they should"
    )
}

/// Raise this process's open-file limit as far as the host allows; the
/// programs run below inherit it. `loop 0 1000` holds 2,001 sockets and
/// the read soak 20,001.
fn raise_fd_limit() -> u64 {
    #[cfg(unix)]
    {
        let mut r = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: a valid out-pointer for getrlimit/setrlimit.
        unsafe {
            libc::getrlimit(libc::RLIMIT_NOFILE, &mut r);
            let ceiling: libc::rlim_t = if cfg!(target_os = "macos") {
                10_240
            } else {
                1 << 20
            };
            let want = r.rlim_max.min(ceiling);
            if want > r.rlim_cur {
                let raised = libc::rlimit {
                    rlim_cur: want,
                    rlim_max: r.rlim_max,
                };
                libc::setrlimit(libc::RLIMIT_NOFILE, &raised);
            }
            libc::getrlimit(libc::RLIMIT_NOFILE, &mut r);
        }
        r.rlim_cur as u64
    }
    #[cfg(not(unix))]
    {
        u64::MAX
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/parked_tasks")
        .join(name)
        .join("main.lu")
}

/// Copy `src` into its own scratch directory as `main.lu` (one module
/// per directory, D32), replacing `edit.0` with `edit.1` when given.
fn stage(src: &Path, dir_name: &str, edit: Option<(&str, &str)>) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("parked_tasks")
        .join(dir_name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let mut body = std::fs::read_to_string(src).expect("read fixture");
    if let Some((from, to)) = edit {
        assert!(body.contains(from), "fixture edit anchor missing: {from}");
        body = body.replacen(from, to, 1);
    }
    let entry = dir.join("main.lu");
    std::fs::write(&entry, body).expect("write fixture");
    entry
}

/// Build a fixture on `tier` ("native" or "release"). `None` only where
/// the host refuses the tier by name (windows release, the s59 skip).
fn build(name: &str, tier: &str) -> Option<PathBuf> {
    ensure_rt_staticlib();
    let entry = stage(&fixture(name), &format!("{name}_{tier}"), None);
    let dir = entry.parent().expect("staged dir").to_path_buf();
    let exe = dir.join(if cfg!(windows) { "w.exe" } else { "w" });
    let mut cmd = Command::new(wolf());
    cmd.current_dir(&dir)
        .arg("build")
        .arg("main.lu")
        .arg("-o")
        .arg(&exe);
    if tier == "release" {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf runs");
    if cfg!(windows) && lane_exit::environment_refusal(&out, &format!("wolf build ({tier})")) {
        eprintln!(
            "SKIP: this host cannot build the {tier} tier: {}",
            text(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success() && exe.is_file(),
        "wolf build {name} ({tier}) must build (exit {:?}) — a refusal is the gate failing:\n{}",
        out.status.code(),
        text(&out.stderr)
    );
    Some(exe)
}

/// The cpu sets this host can pin to: `None` is all cores.
fn cpu_sets() -> Vec<Option<&'static str>> {
    if cfg!(target_os = "linux") {
        let ok = Command::new("taskset")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        assert!(
            ok,
            "`taskset` (util-linux) is part of this gate's linux host: the witness must run \
             on 1 and 4 cpus as well as all cores, never skip them"
        );
        vec![Some("0"), Some("0-3"), None]
    } else {
        eprintln!(
            "NOTE: no cpu-affinity tool on this host; the witness runs on all cores only \
             (the 1- and 4-cpu sets are the linux host's)"
        );
        vec![None]
    }
}

fn label(set: Option<&str>) -> String {
    set.map_or("all cores".to_string(), |c| format!("taskset -c {c}"))
}

fn command(exe: &Path, set: Option<&str>, args: &[&str]) -> Command {
    let mut cmd = match set {
        Some(cpus) => {
            let mut c = Command::new("taskset");
            c.args(["-c", cpus]).arg(exe);
            c
        }
        None => Command::new(exe),
    };
    cmd.args(args);
    cmd
}

struct Run {
    stdout: String,
    stderr: String,
    code: Option<i32>,
    elapsed: Duration,
    stalled: bool,
}

/// Wait for `child` until `limit`; past it the child is killed (by its
/// own handle) and the run is a stall.
fn finish(mut child: Child, t0: Instant, limit: Duration) -> Run {
    let mut stalled = false;
    loop {
        if child.try_wait().expect("try_wait").is_some() {
            break;
        }
        if t0.elapsed() > limit {
            stalled = true;
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let status = child.wait().expect("wait");
    let elapsed = t0.elapsed();
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut stdout);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut stderr);
    }
    Run {
        stdout,
        stderr,
        code: if stalled { None } else { status.code() },
        elapsed,
        stalled,
    }
}

fn run_bounded(exe: &Path, set: Option<&str>, args: &[&str], limit: Duration) -> Run {
    let t0 = Instant::now();
    let child = command(exe, set, args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the witness starts");
    finish(child, t0, limit)
}

fn last_line(s: &str) -> &str {
    s.trim_end().lines().last().unwrap_or("")
}

/// The witness's N for a cpu set: 64 on one cpu (four times its old
/// limit of 7), 1,000 on four cpus and on all cores. Off linux the fd
/// limit decides: 1,000 where it holds 2,001 sockets with room, else 64;
/// windows runs 256.
fn loop_n(set: Option<&str>, fds: u64) -> usize {
    if cfg!(windows) {
        // WSAPoll rebuilds its whole armed list on every wake (s60b).
        return 256;
    }
    match set {
        Some("0") => 64,
        _ if fds >= 4_096 => 1_000,
        _ => 64,
    }
}

/// #713: the witness completes on every cpu set, native and release.
#[test]
fn the_witness_completes_on_every_cpu_set() {
    let fds = raise_fd_limit();
    eprintln!("REPORT open-file limit for the runs: {fds}");
    for tier in ["native", "release"] {
        let Some(exe) = build("waitwolf", tier) else {
            continue;
        };
        for set in cpu_sets() {
            let n = loop_n(set, fds);
            let ns = n.to_string();
            let r = run_bounded(&exe, set, &["loop", "0", &ns], Duration::from_secs(45));
            eprintln!(
                "REPORT waitwolf {tier} {} loop 0 {n}: {:?} exit {:?} in {:?}{}",
                label(set),
                last_line(&r.stdout),
                r.code,
                r.elapsed,
                if r.stalled { " — STALLED, killed" } else { "" }
            );
            assert!(
                !r.stalled,
                "#713: `loop 0 {n}` ({tier}, {}) made no progress in {:?} — the parked tasks \
                 hold every worker the pool will start. stdout: {:?} stderr: {:?}",
                label(set),
                r.elapsed,
                r.stdout,
                r.stderr
            );
            assert_eq!(
                (last_line(&r.stdout), r.code),
                (want(n).as_str(), Some(0)),
                "waitwolf {tier} {} loop 0 {n}; stderr: {}",
                label(set),
                r.stderr
            );
        }
    }
}

/// The sibling lupin, found exactly as `pairing.rs` finds it.
fn sibling_lupin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LUPIN") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while dir.pop() {
        let candidate = dir.join("../wolf-interp/target/release/lupin");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// One observation record's verdict and stdout, from `cmd`'s run.
fn record(mut cmd: Command, what: &str, limit: Duration) -> (String, String) {
    let t0 = Instant::now();
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the observer starts");
    let r = finish(child, t0, limit);
    assert!(
        !r.stalled,
        "{what} made no progress in {:?}; stderr: {}",
        r.elapsed, r.stderr
    );
    let rec: serde_json::Value = serde_json::from_str(&r.stdout)
        .unwrap_or_else(|e| panic!("{what} record parses: {e}; stderr: {}", r.stderr));
    let s = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    eprintln!(
        "REPORT {what}: {} {:?} in {:?}",
        s("verdict"),
        last_line(&s("stdout_inline")),
        r.elapsed
    );
    (s("verdict"), s("stdout_inline"))
}

/// Item 4: at N = 64 every tier prints the same line — the checked
/// machine and lupin (which never had the pool's cap) and the two
/// compiled tiers (which stalled there on up to 16 cpus at trunk).
#[test]
fn every_tier_answers_the_witness_alike() {
    raise_fd_limit();
    let n = 64;
    let entry = stage(
        &fixture("waitwolf"),
        "waitwolf_fixed64",
        // conform-run and lupin take no program arguments.
        Some((
            "    let args = env_args()",
            "    let args = [\"loop\", \"0\", \"64\"]",
        )),
    );
    let mut checked = Command::new(wolf());
    checked
        .arg("conform-run")
        .arg(&entry)
        .arg("--checked")
        .arg("--json");
    let (verdict, stdout) = record(checked, "checked machine", Duration::from_secs(120));
    assert_eq!(
        (verdict.as_str(), last_line(&stdout)),
        ("exit(0)", want(n).as_str()),
        "the checked machine's answer"
    );
    match sibling_lupin() {
        Some(lupin) => {
            let mut cmd = Command::new(lupin);
            cmd.arg("conform-run").arg(&entry).arg("--json");
            let (verdict, stdout) = record(cmd, "lupin", Duration::from_secs(120));
            assert_eq!(
                (verdict.as_str(), last_line(&stdout)),
                ("exit(0)", want(n).as_str()),
                "lupin's answer"
            );
        }
        None => {
            assert!(
                std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
                "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
                 (set LUPIN, or build ../wolf-interp)"
            );
            eprintln!("SKIP: no sibling lupin — the lupin half of this gate did not run");
        }
    }
    for tier in ["native", "release"] {
        let Some(exe) = build("waitwolf", tier) else {
            continue;
        };
        let r = run_bounded(&exe, None, &["loop", "0", "64"], Duration::from_secs(45));
        eprintln!(
            "REPORT {tier} all cores loop 0 64: {:?} exit {:?} in {:?}{}",
            last_line(&r.stdout),
            r.code,
            r.elapsed,
            if r.stalled { " — STALLED, killed" } else { "" }
        );
        assert!(
            !r.stalled,
            "#713: `loop 0 64` on {tier} made no progress in {:?}",
            r.elapsed
        );
        assert_eq!(
            (last_line(&r.stdout), r.code),
            (want(n).as_str(), Some(0)),
            "the {tier} answer; stderr: {}",
            r.stderr
        );
    }
}

/// What /proc says about a running process (linux).
#[derive(Debug, Clone, Copy, Default)]
struct Sample {
    threads: u64,
    rss_kib: u64,
    cpu_ticks: u64,
}

#[cfg(target_os = "linux")]
fn sample(pid: u32) -> Option<Sample> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let field = |k: &str| {
        status
            .lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
    };
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Fields after the parenthesised command name: utime is the 14th
    // field overall, stime the 15th.
    let rest = &stat[stat.rfind(')')? + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    let ticks = f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?;
    Some(Sample {
        threads: field("Threads:")?,
        rss_kib: field("VmRSS:")?,
        cpu_ticks: ticks,
    })
}

#[cfg(not(target_os = "linux"))]
fn sample(_pid: u32) -> Option<Sample> {
    None
}

fn clock_ticks() -> u64 {
    #[cfg(unix)]
    {
        // SAFETY: sysconf has no preconditions.
        let t = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        if t > 0 { t as u64 } else { 100 }
    }
    #[cfg(not(unix))]
    {
        100
    }
}

struct Soak {
    run: Run,
    /// While every task sat parked: the process, then its cpu over the
    /// window as a fraction of one cpu.
    parked: Option<(Sample, f64)>,
}

/// Run `park MODE M HOLD` (the soak fixture); once it says `parked:`,
/// wait for the threads to settle, then sample twice `window` apart.
fn soak(exe: &Path, set: Option<&str>, mode: &str, m: usize, hold_ms: u64) -> Soak {
    let ms = m.to_string();
    let hs = hold_ms.to_string();
    let t0 = Instant::now();
    let mut child = command(exe, set, &[mode, &ms, &hs])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the soak starts");
    let pid = child.id();
    let out = child.stdout.take().expect("stdout piped");
    let (tx, rx) = mpsc::channel::<String>();
    let reader = std::thread::spawn(move || {
        let mut all = String::new();
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            all.push_str(&line);
            all.push('\n');
            let _ = tx.send(line);
        }
        all
    });
    let mut parked = None;
    if rx
        .recv_timeout(Duration::from_secs(60))
        .is_ok_and(|l| l.starts_with("parked:"))
    {
        // The program holds its tasks for `hold_ms` from this line.
        let deadline = Instant::now() + Duration::from_millis(hold_ms);
        // Settle: every task has a thread (or the pool stops growing).
        let mut last = 0;
        let settle_until = Instant::now() + Duration::from_millis(hold_ms / 2);
        while Instant::now() < settle_until {
            std::thread::sleep(Duration::from_millis(200));
            let Some(s) = sample(pid) else { break };
            if s.threads as usize > m || (s.threads == last && s.threads > 0) {
                break;
            }
            last = s.threads;
        }
        let window = Duration::from_millis(1500);
        if Instant::now() + window < deadline
            && let Some(a) = sample(pid)
        {
            std::thread::sleep(window);
            if let Some(b) = sample(pid) {
                let cpu = (b.cpu_ticks.saturating_sub(a.cpu_ticks)) as f64
                    / clock_ticks() as f64
                    / window.as_secs_f64();
                parked = Some((b, cpu));
            }
        }
    }
    let mut run = finish(
        child,
        t0,
        Duration::from_millis(hold_ms) + Duration::from_secs(90),
    );
    run.stdout = reader.join().unwrap_or_default();
    Soak { run, parked }
}

/// The soak: 10,000 parked tasks (2,000 on macOS), each way of parking,
/// held and released; memory per parked task and the cpu of the parked
/// process reported, the cpu bounded.
#[cfg(unix)]
#[test]
fn ten_thousand_parked_tasks_hold_and_release() {
    let fds = raise_fd_limit();
    let m: usize = if cfg!(target_os = "macos") {
        2_000
    } else {
        10_000
    };
    // `accept` peaks at three sockets a task (listener, client, server).
    assert!(
        fds >= (3 * m + 64) as u64,
        "the soak needs {} open files and the host allows {fds}",
        3 * m + 64
    );
    let Some(exe) = build("park", "native") else {
        return;
    };
    let mut runs: Vec<(&str, Option<&str>)> =
        vec![("chan", None), ("read", None), ("accept", None)];
    if cfg!(target_os = "linux") {
        runs.push(("chan", Some("0")));
    }
    for (mode, set) in runs {
        // The same mode at 100 tasks is the baseline the per-task
        // figure is taken against.
        let base = soak(&exe, set, mode, 100, 4_000);
        let s = soak(&exe, set, mode, m, 9_000);
        let per_task = match (base.parked, s.parked) {
            (Some((b, _)), Some((p, _))) => {
                Some((p.rss_kib.saturating_sub(b.rss_kib)) as f64 / (m - 100) as f64)
            }
            _ => None,
        };
        eprintln!(
            "REPORT soak {mode} {} M={m}: {:?} exit {:?} in {:?}{}; parked: {:?}; \
             baseline(100): {:?}; RSS per parked task {} KiB",
            label(set),
            last_line(&s.run.stdout),
            s.run.code,
            s.run.elapsed,
            if s.run.stalled { " — STALLED, killed" } else { "" },
            s.parked,
            base.parked,
            per_task.map_or("n/a".to_string(), |k| format!("{k:.1}"))
        );
        let done = format!("released: {mode} {m} parked, {m} answered");
        assert!(
            !s.run.stalled,
            "#713: {m} tasks parked in {mode} ({}) never all answered in {:?}; stdout: {:?}",
            label(set),
            s.run.elapsed,
            s.run.stdout
        );
        assert_eq!(
            (last_line(&s.run.stdout), s.run.code),
            (done.as_str(), Some(0)),
            "soak {mode} {}; stderr: {}",
            label(set),
            s.run.stderr
        );
        if cfg!(target_os = "linux") {
            let (p, cpu) = s
                .parked
                .unwrap_or_else(|| panic!("soak {mode}: the parked process was never sampled"));
            assert!(
                p.threads as usize > m,
                "soak {mode}: {} threads while {m} tasks were parked — the pool did not \
                 hold one for each",
                p.threads
            );
            // A parked task waits for an event. At trunk every parked
            // wait woke each 5 ms to poll for cancellation: 2,000,000
            // wakeups a second at this size.
            assert!(
                cpu < 0.25,
                "soak {mode}: the parked process burned {:.0}% of a cpu with every task \
                 parked — a parked task must not wake on a timer",
                cpu * 100.0
            );
            let k = per_task.expect("both runs sampled");
            assert!(
                k < 128.0,
                "soak {mode}: {k:.1} KiB of RSS per parked task"
            );
        }
    }
}
