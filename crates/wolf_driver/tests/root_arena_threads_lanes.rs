//! s222 (wolf-lang#635, #421) — strings made on one thread and read on
//! another, through the process root, on every machine that runs them.
//!
//! s222 gave the native root arena a bump window per thread (the
//! design note in `wolf_rt::str`): the lock is taken to refill a window,
//! never per allocation. What a program can observe must not move:
//! a `str` materialized by a `par` worker or a scoped task is read by
//! the spawner after the worker is done and after the spawner has
//! itself allocated thousands of strings since — if two threads were
//! ever handed overlapping bytes, or a window's bytes were reclaimed,
//! the checksums below move. Both programs are deterministic in their
//! output whatever the schedule (the hashes are order-independent sums
//! or per-index), so every machine owes the same bytes.
//!
//! - native and release: the answer below, byte for byte (the release
//!   tier links the same runtime with its own codegen).
//! - checked: these shapes were outside the checked machine until s226
//!   (`par` was "this List method" and a `scope` "structured
//!   concurrency in checked execution (C1 deferred)"). It answers now,
//!   and must answer the same bytes; a refusal for any other reason
//!   fails the gate.
//! - lupin: the oracle, and the race detector (`[conc.mm.race.3]`): it
//!   must answer the same bytes and never `trap(race)`.
//!
//! The concurrency rows of s222's report run this file under
//! `taskset -c 0-3` and on all cores.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
    unsupported: String,
}

fn parse_obs(bytes: &[u8], stderr: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    let s = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: s("verdict"),
        stdout: s("stdout_inline"),
        version: s("impl_version"),
        unsupported: rec["x-unsupported-construct"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| String::from_utf8_lossy(stderr).into_owned()),
    }
}

/// One wolfgang machine. `None`: the host cannot run the native lane
/// (the s59 skip pattern, printed), never a silent pass.
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if flag != "--checked" && lane_exit::environment_refusal(&out, &format!("wolf {flag}")) {
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    Some(parse_obs(&out.stdout, &out.stderr, &format!("wolf {flag}")))
}

/// The sibling lupin, found as `pairing.rs` finds it.
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

fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle and race-detector leg of wolf-lang#635's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the compiled machines are \
             still held to the expected answer; only the oracle leg is absent"
        );
        return None;
    };
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    Some(parse_obs(&out.stdout, &out.stderr, "lupin"))
}

/// Write `src` as `main.lu` in a fresh directory of its own.
fn program(name: &str, src: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("s222-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("main.lu");
    std::fs::write(&p, src).unwrap();
    p
}

fn every_machine(name: &str, src: &str, want: &str, checked_reason: &str) {
    let entry = program(name, src);
    for flag in ["--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want),
            "{name}: wolf {flag} ({}) — a string crossed threads wrong",
            obs.version
        );
    }
    if let Some(obs) = lane(&entry, "--checked") {
        if obs.verdict == "unsupported" {
            assert!(
                obs.unsupported.contains(checked_reason),
                "{name}: checked is unsupported for a reason other than the recorded one: {}",
                obs.unsupported
            );
            eprintln!(
                "REPORT: {name} checked unsupported: {}",
                obs.unsupported.trim()
            );
        } else {
            assert_eq!(
                (obs.verdict.as_str(), obs.stdout.as_str()),
                ("exit(0)", want),
                "{name}: checked now runs this shape and must answer the same bytes"
            );
        }
    }
    if let Some(obs) = lupin_says(&entry) {
        assert_ne!(
            obs.verdict, "trap(race)",
            "{name}: lupin {} saw a race",
            obs.version
        );
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want),
            "{name}: lupin {}",
            obs.version
        );
    }
    let _ = std::fs::remove_dir_all(entry.parent().unwrap());
}

/// `par` workers (pool threads) materialize 12,800 strings in the
/// root; the spawner churns 20,000 more of its own, then re-derives
/// every worker's list serially and compares.
const PAR_STRINGS: &str = r#"fn piece(i: int) -> List[str] {
    var out = List[str]()
    var k = 0
    while k < 200 {
        let s = "w{i}-{k}-" + "ab".repeat(k % 7 + 1)
        (mut out).push(s.upper())
        k = k + 1
    }
    out
}

fn sum(xs: List[str]) -> int {
    var h = 0
    for s in xs {
        for b in s.bytes() { h = (h * 7 + (b as int)) % 1000003 }
    }
    h
}

fn main() -> !int {
    var ids = List[int]()
    var i = 0
    while i < 64 {
        (mut ids).push(i)
        i = i + 1
    }
    let got = ids.par(piece)
    var churn = List[str]()
    var j = 0
    while j < 20000 {
        (mut churn).push("c{j}" + "x".repeat(j % 13))
        j = j + 1
    }
    var bad = 0
    var total = 0
    for k in 0..64 {
        let have = sum(got[k])
        if have != sum(piece(k)) { bad = bad + 1 }
        total = (total + have) % 1000000007
    }
    print("bad {bad} total {total} churn {churn.len}")
    0
}
"#;

/// Eight scoped tasks send 2,400 strings they materialized over a
/// channel; the spawner reads them after the scope has joined and
/// after allocating 20,000 strings of its own.
const SCOPE_CHANNEL: &str = r#"fn make(t: int, ch: channel[str]) {
    var k = 0
    while k < 300 {
        ch.send("t{t}-k{k}-" + "z".repeat(k % 11)) else {}
        k = k + 1
    }
}

fn main() -> !int {
    let ch = channel[str](4096)
    scope s {
        for t in 0..8 {
            s.spawn(fn() { make(t, ch) })
        }
    }
    ch.close()
    var churn = 0
    var j = 0
    while j < 20000 {
        let c = "c{j}" + "y".repeat(j % 7)
        churn = churn + c.bytes().len
        j = j + 1
    }
    var n = 0
    var h = 0
    for m in ch {
        n = n + 1
        for b in m.bytes() { h = (h + (b as int)) % 1000003 }
    }
    print("n {n} h {h} churn {churn}")
    0
}
"#;

#[test]
fn par_workers_strings_read_back_on_the_spawner() {
    every_machine(
        "par-strings",
        PAR_STRINGS,
        "bad 0 total 33242710 churn 20000\n",
        "this List method",
    );
}

#[test]
fn scoped_tasks_strings_cross_a_channel() {
    every_machine(
        "scope-channel",
        SCOPE_CHANNEL,
        "n 2400 h 654442 churn 168887\n",
        "structured concurrency",
    );
}
