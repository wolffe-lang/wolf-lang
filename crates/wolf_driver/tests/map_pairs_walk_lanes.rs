//! s273 (wolf-lang#972, #943) — the two memory blowups, as rows on every
//! machine the gauntlet runs.
//!
//! **#972, the resident set.** `fixtures/map_pairs_walk/main.lu` is the
//! issue's program (N string-key inserts into a `Map[str, int]`, then N
//! lookups) with wolf-std's `map.set`/`map.get` bodies over a local `Eq`.
//! `get` walks `m.pairs()`, and at trunk `b16353e8` every walk copied the
//! whole entry buffer into a fresh list in the caller's region, so N gets
//! kept N² × 24 bytes: 2,356 MB at N = 10,000 on nomad, against 2.2 MB for
//! the same program in C (`notes/s273-evidence/`). A walk whose body never
//! names the map now reads the map's own entries. The row is the cost of
//! an entry: what N more entries add to the peak resident set, divided by
//! N — `wait4(2)`'s `ru_maxrss` (kibibytes on linux, bytes on macOS),
//! never a ledger. On Linux the reading has the harness's high-water
//! floor (#1008), so it cannot resolve arbitrary sub-floor growth. The
//! regression guarded here is the retained N² entry snapshots, not a
//! small linear leak; s276's off-line plant materializes `pairs()` per get.
//! The bound is 1 KiB an entry natively (≈ 90 B measured after, ≈ 140 KB at
//! N = 2,000 before) and 16 KiB on the checked machine (its `Value`s and
//! the compiler beside them; ≈ 400 KB before), with the peak itself under
//! a ceiling. linux and macOS (windows has no `wait4`, and the row is not
//! asked there).
//!
//! **The corpus rows on every machine.** `cargo xtask corpus` runs a
//! `phase: run` row natively; this gate runs s273's three rows on the
//! checked machine, native and release, and asserts one answer:
//! `map_pairs_walk_view.lu` (the walk charges nothing), `_snapshot.lu`
//! (a body that writes the map still sees the walk's snapshot) and
//! `list_reset_reuse_ledger.lu` (#943: a reset and refilled list does not
//! grow its region; the checked machine charged every refill until
//! s273). lupin's column: 0.1.58 answers the first two as the compilers
//! do and charges the refill as the checked machine did (wolf-interp
//! owes the mirror); an unlisted release must mirror. Beside them,
//! `fixtures/reuse_ledger/main.lu` holds #943's neighbours (`clear` and
//! `pop` on a list, `clear` and `remove` on a map), whose kept storage is
//! each machine's own rather than the clause's, to one answer.

#![cfg(unix)]

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn report(line: &str) {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("map_pairs_walk_lanes")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map_pairs_walk/main.lu")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The fixture built for one tier, or `None` (a loud SKIP) when the host
/// cannot run that lane; a compile error or an ICE fails the gate.
fn build(dir: &Path, release: bool) -> Option<PathBuf> {
    let tier = if release { "release" } else { "native" };
    let exe = dir.join(format!("mapwalk-{tier}"));
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(fixture())
        .arg("-o")
        .arg(&exe)
        .arg("--no-cache");
    if release {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf build runs");
    if lane_exit::environment_refusal(&out, &format!("wolf build ({tier})")) {
        eprintln!(
            "SKIP: environment cannot run the {tier} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "wolf build ({tier}) refused the fixture: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(exe)
}

/// Reap `child` with `wait4(2)`: its raw status and its peak resident set
/// in bytes (`ru_maxrss`: kibibytes on linux, bytes on macOS).
fn reap_with_peak(child: std::process::Child) -> (libc::c_int, u64) {
    let pid = child.id() as libc::pid_t;
    let mut status: libc::c_int = 0;
    // SAFETY: a zeroed `rusage` is a valid out-parameter; `wait4` fills it
    // for the child this test spawned and has not waited for.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: plain FFI on a pid this process spawned.
    let reaped = unsafe { libc::wait4(pid, &mut status, 0, &mut ru) };
    assert_eq!(reaped, pid, "wait4 reaps exactly the child");
    let maxrss = ru.ru_maxrss as u64;
    let bytes = if cfg!(target_os = "macos") {
        maxrss
    } else {
        maxrss * 1024
    };
    (status, bytes)
}

/// One run of `cmd`: its stdout and its peak resident set in bytes.
fn peak(mut cmd: Command, what: &str) -> (String, u64) {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the witness");
    let mut stdout = String::new();
    {
        use std::io::Read as _;
        let mut out = child.stdout.take().expect("piped stdout");
        out.read_to_string(&mut stdout).expect("read stdout");
    }
    let (status, bytes) = reap_with_peak(child);
    assert!(
        libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
        "{what} did not exit 0 (status {status})"
    );
    (stdout, bytes)
}

/// The bytes an entry adds to the peak between `n` and `2n` entries.
fn per_entry(p1: u64, p2: u64, n: u64) -> i64 {
    (p2 as i64 - p1 as i64) / n as i64
}

const MIB: u64 = 1 << 20;

#[test]
fn a_string_keyed_map_walked_by_get_keeps_a_linear_resident_set_natively() {
    let dir = scratch("native");
    const N: u64 = 2_000;
    let mut failures = Vec::new();
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&dir, release) else {
            continue;
        };
        let run = |n: u64| {
            let mut cmd = Command::new(&exe);
            cmd.arg(n.to_string());
            peak(cmd, &format!("{tier} mapwalk {n}"))
        };
        let (out1, p1) = run(N);
        let (out2, p2) = run(2 * N);
        assert_eq!(out1.trim(), N.to_string(), "{tier}: every key found");
        assert_eq!(out2.trim(), (2 * N).to_string(), "{tier}: every key found");
        let cost = per_entry(p1, p2, N);
        report(&format!(
            "REPORT mapwalk {tier}: {N} entries {p1} B, {} entries {p2} B, {cost} B an entry",
            2 * N
        ));
        if cost > 1024 {
            failures.push(format!(
                "{tier}: {cost} B an entry (bound 1024): the walk in `get` is copying the map"
            ));
        }
        if p2 > 64 * MIB {
            failures.push(format!(
                "{tier}: peak {p2} B at {} entries (ceiling {} B)",
                2 * N,
                64 * MIB
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_string_keyed_map_walked_by_get_keeps_a_linear_resident_set_on_the_checked_machine() {
    const N: u64 = 500;
    let run = |n: u64| {
        let mut cmd = Command::new(wolf());
        cmd.args(["conform-run", "--json", "--checked"])
            .arg(fixture())
            .arg(format!("--argv-hex={}", hex(b"mapwalk")))
            .arg(format!("--argv-hex={}", hex(n.to_string().as_bytes())));
        let (out, p) = peak(cmd, &format!("checked mapwalk {n}"));
        let rec: serde_json::Value = serde_json::from_str(&out).expect("one record");
        assert_eq!(rec["verdict"], "exit(0)", "checked mapwalk {n}: {rec}");
        assert_eq!(
            rec["stdout_inline"].as_str().map(str::trim),
            Some(n.to_string().as_str()),
            "checked mapwalk {n}: every key found"
        );
        p
    };
    let p1 = run(N);
    let p2 = run(2 * N);
    let cost = per_entry(p1, p2, N);
    report(&format!(
        "REPORT mapwalk checked: {N} entries {p1} B, {} entries {p2} B, {cost} B an entry",
        2 * N
    ));
    assert!(
        cost <= 16 * 1024,
        "checked: {cost} B an entry (bound 16 KiB): the walk in `get` is minting a list per call"
    );
    assert!(
        p2 <= 128 * MIB,
        "checked: peak {p2} B at {} entries (ceiling {} B)",
        2 * N,
        128 * MIB
    );
}

// ------------------------------------------------- the corpus rows -------

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
    commit: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        commit: rec["commit"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    }
}

fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) && flag != "--checked" {
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, "the observation"))
}

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
             (LUPIN={}) — the lupin column of s273's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the three wolfgang \
             lanes are still asserted; only lupin's column is absent"
        );
        return None;
    };
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    Some(parse_obs(&out.stdout, "lupin's observation"))
}

/// wolf-interp owes #943's mirror: the published records that still
/// charge a refilled slot twice (measured: 0.1.58 prints `8 false true`
/// on `list_reset_reuse_ledger.lu`). An unlisted release must mirror.
fn refill_before_mirror(version: &str, commit: &str) -> bool {
    // r43: the 0.1.60 candidate at its release head keeps the forward pin
    // (`8 false true`, wolf-interp's retirement-checked witness); r44: so
    // does the 0.1.61 candidate at its release head.
    [
        ("0.1.58", "6240efe"),
        ("0.1.59", "668fb75"),
        ("0.1.60", "17bd522"),
        ("0.1.61", "a10cbe5"),
    ]
    .iter()
    .any(|(v, c)| *v == version && commit.starts_with(c))
}

#[test]
fn the_refill_pin_names_only_the_measured_release_record() {
    assert!(refill_before_mirror("0.1.58", "6240efe"));
    assert!(!refill_before_mirror("0.1.58", "unknown"));
    assert!(!refill_before_mirror("0.1.59", "6240efe"));
    assert!(refill_before_mirror("0.1.59", "668fb75"));
    assert!(!refill_before_mirror("0.1.59", "unknown"));
    assert!(!refill_before_mirror("0.1.58", "668fb75"));
    assert!(refill_before_mirror("0.1.60", "17bd522"));
    assert!(!refill_before_mirror("0.1.60", "668fb75"));
    assert!(!refill_before_mirror("0.1.60", "unknown"));
    assert!(refill_before_mirror("0.1.61", "a10cbe5"));
    assert!(!refill_before_mirror("0.1.61", "17bd522"));
    assert!(!refill_before_mirror("0.1.61", "unknown"));
}

fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// A run row: exit(0) and `want` on the checked machine, native and
/// release; lupin's column as measured (`lupin_pre`: what a listed
/// pre-mirror record answers instead).
fn run_row(rel: &str, want: &str, lupin_pre: Option<&str>, why: &str) {
    let entry = corpus(rel);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        report(&format!(
            "REPORT {rel} {flag}: {} {:?}",
            obs.verdict, obs.stdout
        ));
        assert_eq!(obs.verdict, "exit(0)", "{rel} on {flag}: {why}");
        assert_eq!(obs.stdout, want, "{rel} on {flag}: {why}");
    }
    if let Some(l) = lupin_says(&entry) {
        report(&format!(
            "REPORT {rel} lupin {} {}: {} {:?}",
            l.version, l.commit, l.verdict, l.stdout
        ));
        assert_eq!(l.verdict, "exit(0)", "{rel}: lupin's verdict");
        match lupin_pre {
            Some(pre) if refill_before_mirror(&l.version, &l.commit) => {
                assert_eq!(l.stdout, pre, "{rel}: the measured pre-mirror lupin");
            }
            _ => assert_eq!(l.stdout, want, "{rel}: lupin's answer"),
        }
    }
}

#[test]
fn a_walk_that_never_names_its_map_charges_nothing_on_every_machine() {
    run_row(
        "memory/map_pairs_walk_view.lu",
        "64 201600 7 true true\n",
        None,
        "a pairs() walk whose body never names the map mints no list (#972)",
    );
}

#[test]
fn a_walk_whose_body_writes_the_map_sees_its_snapshot_on_every_machine() {
    run_row(
        "memory/map_pairs_walk_snapshot.lu",
        "abc 6 3 20 4\n",
        None,
        "a body that names the map walks the materialized snapshot",
    );
}

#[test]
fn a_reset_and_refilled_list_does_not_grow_its_region_on_every_machine() {
    run_row(
        "memory/list_reset_reuse_ledger.lu",
        "8 true true\n",
        Some("8 false true\n"),
        "[type.list.reset]: the next pushes reuse the buffer (#943)",
    );
}

/// #943's neighbours (`fixtures/reuse_ledger/main.lu`): a `List` refilled
/// after `reset`, `clear` and `pop`, a `Map` after `clear` and `remove`,
/// each flat on the ledger, and a push or insert past the high-water mark
/// charging — one answer on every machine. At the base the checked
/// machine printed `false false false true false false true 1616` (every
/// refill charged again); native and release printed this line.
#[test]
fn a_refilled_container_charges_only_past_its_storage_on_every_machine() {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reuse_ledger/main.lu");
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        report(&format!(
            "REPORT reuse_ledger {flag}: {} {:?}",
            obs.verdict, obs.stdout
        ));
        assert_eq!(obs.verdict, "exit(0)", "reuse_ledger on {flag}");
        assert_eq!(
            obs.stdout, "true true true true true true true 1616\n",
            "reuse_ledger on {flag}: a refill reuses the storage, growth charges"
        );
    }
}
