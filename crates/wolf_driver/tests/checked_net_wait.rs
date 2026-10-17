//! s244 (wolf-lang#720) — `net_wait` beside other tasks on the checked
//! machine.
//!
//! wolf-book exercise ex33-7 serves two spawned callers from one loop on
//! `net_wait`. lupin and both compiled tiers answer `answered 4 questions
//! on 2 connections` at once; the checked machine never finished
//! (bs65: killed at 120 s and 200 s on kasumi, hasu and nomad-1).
//!
//! The cause, measured at 0.2.31 on kasumi (`strace -f`): the machine's
//! one running thread sat in `poll([{fd=3, events=POLLIN}], 1, 2000)` on
//! the listener, timed out every 2.002 s, and no `connect(2)` was ever
//! made — the spawned callers were ready but never ran, because the
//! scheduler never preempts (`[exec.checked.sched]`) and the wait was no
//! blocking point: one poll(2) for the whole deadline, not the
//! millisecond-at-a-time park an accept or a read makes
//! (`[exec.checked.task]`). The loop got the empty answer and asked
//! again, for ever, at zero cpu. lupin's `net_wait` hands the baton on
//! between polls.
//!
//! The gates run `wolf conform-run --checked --json` (the checked
//! machine's own answer; `wolf run --checked` is the native build) under
//! a deadline: a run past it is killed by its own child handle and FAILS
//! the gate. Each program's answer is held to lupin's and to the native
//! tier's.

mod lane_exit;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// Far past any honest run of these programs (each answers in well
/// under a second on every tier), and far short of a hang.
const LIMIT: Duration = Duration::from_secs(30);

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/checked_net_wait")
        .join(name)
        .join("main.lu")
}

struct Rec {
    verdict: String,
    stdout: String,
    elapsed: Duration,
}

/// Wait for `child` until `LIMIT`; past it the child is killed by its
/// own handle and the gate fails with what it printed.
fn finish(mut child: Child, t0: Instant, what: &str) -> (String, String, Duration) {
    let mut stalled = false;
    loop {
        if child.try_wait().expect("try_wait").is_some() {
            break;
        }
        if t0.elapsed() > LIMIT {
            stalled = true;
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = child.wait();
    let elapsed = t0.elapsed();
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut stdout);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut stderr);
    }
    assert!(
        !stalled,
        "#720: {what} made no progress in {elapsed:?} and was killed; stderr: {stderr}"
    );
    (stdout, stderr, elapsed)
}

fn record(mut cmd: Command, what: &str) -> Rec {
    let t0 = Instant::now();
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the observer starts");
    let (stdout, stderr, elapsed) = finish(child, t0, what);
    let rec: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("{what} record parses: {e}; stderr: {stderr}"));
    let s = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    let r = Rec {
        verdict: s("verdict"),
        stdout: s("stdout_inline"),
        elapsed,
    };
    eprintln!(
        "REPORT {what}: {} {:?} in {:?}",
        r.verdict, r.stdout, r.elapsed
    );
    r
}

fn checked(entry: &Path, seed: Option<u64>) -> Rec {
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--checked")
        .arg("--json");
    if let Some(seed) = seed {
        cmd.arg(format!("--seed={seed}"));
    }
    let what = match seed {
        Some(n) => format!("checked machine (seed {n}) on {}", entry.display()),
        None => format!("checked machine on {}", entry.display()),
    };
    record(cmd, &what)
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

fn lupin(entry: &Path) -> Option<Rec> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (set LUPIN, or build ../wolf-interp)"
        );
        eprintln!("SKIP: no sibling lupin — the lupin half of this gate did not run");
        return None;
    };
    let mut cmd = Command::new(lupin);
    cmd.arg("conform-run").arg(entry).arg("--json");
    Some(record(cmd, &format!("lupin on {}", entry.display())))
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

/// The native tier's record; `None` is the s59 environment skip, named.
fn native(entry: &Path) -> Option<Rec> {
    ensure_rt_staticlib();
    let probe = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg("--native")
        .arg("--json")
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&probe, "wolf --native") {
        eprintln!(
            "SKIP: this host cannot run the native lane: {}",
            String::from_utf8_lossy(&probe.stderr).trim()
        );
        return None;
    }
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--native")
        .arg("--json");
    Some(record(cmd, &format!("native on {}", entry.display())))
}

/// Every machine gives `want` as an `exit(0)` record; the checked
/// machine also under three seeds.
fn every_machine_answers(name: &str, want: &str) {
    let entry = fixture(name);
    let ok = |r: &Rec, what: &str| {
        assert_eq!(
            (r.verdict.as_str(), r.stdout.as_str()),
            ("exit(0)", want),
            "{name}: {what}"
        );
    };
    ok(&checked(&entry, None), "the checked machine's answer");
    for seed in [1, 7, 42] {
        ok(
            &checked(&entry, Some(seed)),
            &format!("the checked machine under seed {seed}"),
        );
    }
    if let Some(l) = lupin(&entry) {
        ok(&l, "lupin's answer");
    }
    if let Some(n) = native(&entry) {
        ok(&n, "the native tier's answer");
    }
}

/// The reduced witness: one spawned caller, one wait loop. At 0.2.31 the
/// checked machine waited 2 s per pass for ever; now the caller runs in
/// the wait's first millisecond and the first wait answers the door.
#[test]
fn a_wait_beside_its_caller_hands_the_cpu_on() {
    every_machine_answers("door", "the door rang after 0 empty waits\n");
}

/// wolf-book ex33-7 itself, byte for byte (wolf-book `dfa6899`,
/// `principles/exercises/ch33/ex33-7.lu`, sha256 `405c617b…61cf96`).
#[test]
fn the_book_exercise_finishes_with_lupins_answer() {
    every_machine_answers("ex33_7", "answered 4 questions on 2 connections\n");
}

/// The deadline still decides: `0` asks once (and, as lupin's, hands
/// nothing on), a short deadline answers the empty list while the caller
/// sleeps, and the unbounded wait answers once the caller has dialled.
#[test]
fn the_deadline_still_decides_beside_other_tasks() {
    every_machine_answers("deadlines", "zero 0, short 0, unbounded 1\n");
}
