//! s245 (wolf-lang#777) — a write larger than the socket's buffers,
//! beside the task that reads it, on the checked machine.
//!
//! main connects to a listener whose spawned task accepts and reads, then
//! writes 8 MiB. lupin 0.1.54 and the native tier answer `wrote 8388608
//! bytes to a task` in about 0.1 s; at 0.2.31 the checked machine
//! (`conform-run --json --checked`) was killed at 30 s, for each of
//! `net_write`, `net_write_bytes`, `net_writev` and `net_writev_head`.
//! Each was one blocking `write(2)`/`writev(2)` of everything, made on the
//! thread that holds the baton; the scheduler never preempts
//! (`[exec.checked.sched]`), so once the socket's buffers were full the
//! write waited for a reader that was a task of the same machine and
//! never ran. s244's `net_wait` had the same shape (#720). The writes now
//! park beside other tasks as a read does (`[exec.checked.task]`).
//!
//! The gates run `wolf conform-run --checked --json` (the checked
//! machine's own answer; `wolf run --checked` is the native build) under
//! a deadline: a run past it is killed by its own child handle and FAILS
//! the gate. Each program's answer is held to lupin's and to the native
//! tier's. The deadline program holds the write budget to the same rows
//! beside a task that never reads and with no task at all.

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
        .join("tests/fixtures/checked_net_write")
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
        "#777: {what} made no progress in {elapsed:?} and was killed; stderr: {stderr}"
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
/// machine also under three seeds. s246 (wolf-lang#780): a fill
/// fixture's last line, `filled N MiB`, is how far that machine wrote
/// before its reader's byte came back — a host's and a machine's own, so
/// it is read and reported (`REPORT … filled`), held to at least the 8
/// MiB of the first write, and every line before it held to `want`.
fn every_machine_answers(name: &str, want: &str) {
    let entry = fixture(name);
    let ok = |r: &Rec, what: &str| {
        let (head, filled) = match r.stdout.rfind("filled ") {
            Some(at) if r.stdout.ends_with(" MiB\n") => r.stdout.split_at(at),
            _ => (r.stdout.as_str(), ""),
        };
        assert_eq!(
            (r.verdict.as_str(), head),
            ("exit(0)", want),
            "{name}: {what}"
        );
        if !filled.is_empty() {
            let mib: u64 = filled
                .trim_start_matches("filled ")
                .trim_end_matches(" MiB\n")
                .parse()
                .unwrap_or_else(|_| panic!("{name}: {what}: the fill line reads: {filled:?}"));
            eprintln!("REPORT {name}: {what}: filled {mib} MiB before the reader's byte came back");
            assert!(
                mib >= 8,
                "{name}: {what}: the fill counts the first 8 MiB: {mib}"
            );
        }
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

/// What a write fixture says before its `filled` line: s245's line for
/// the 8 MiB write (`wrote 8388608 bytes to a task`), and, s246
/// (wolf-lang#780), that the reader ran that the reader ran (sent its byte back)
/// before the last write returned, which on the checked machine is a
/// write that blocked and handed the baton on, and that it read every
/// byte. Red with s245's hand-over removed: the checked machine is killed
/// at 30 s on every host (the fill makes even windows' loopback full).
const FILLED: &str = "wrote 8388608 bytes to a task\n\
                      the reader ran before the writes ended: true, and read every byte: true\n";

/// s244's witness (its 8 MiB write, then the s246 fill): `net_write`.
#[test]
fn a_big_write_beside_its_reader_hands_the_cpu_on() {
    every_machine_answers("write", FILLED);
}

/// The byte twin: `net_write_bytes`.
#[test]
fn a_big_byte_write_beside_its_reader_hands_the_cpu_on() {
    every_machine_answers("write_bytes", FILLED);
}

/// The gather: `net_writev`, two 4 MiB parts.
#[test]
fn a_big_gathered_write_beside_its_reader_hands_the_cpu_on() {
    every_machine_answers("writev", FILLED);
}

/// The gather with a `str` head: `net_writev_head`.
#[test]
fn a_big_headed_write_beside_its_reader_hands_the_cpu_on() {
    every_machine_answers("writev_head", FILLED);
}

/// An armed deadline still ends a write nobody reads, beside a task that
/// never reads and with no task left: `io` both times, on every machine.
#[test]
fn the_write_deadline_still_decides_beside_other_tasks() {
    every_machine_answers("deadline", "beside io, alone io\n");
}
