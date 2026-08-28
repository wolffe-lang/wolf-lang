//! s106 — the net builtin tier, cross-lane (c26's first crossing,
//! wolf-lang#118).
//!
//! The os_time_native discipline over the net corpus: every witness
//! runs `conform-run` on the checked lane (the reference executor)
//! AND both native tiers (`--native` = wir → Cranelift → cc,
//! `--release` = wir → LLVM -O2 → clang) and must produce the
//! IDENTICAL verdict and stdout — byte-equal, per file. The
//! acceptance's ten-run clause is asserted here for the echo
//! roundtrip on both native tiers; the spawn-accept witness rides
//! conc_native.rs's X12 ten-run suite (the checked lane refuses
//! structured concurrency wholesale — C1 deferred — so its honest
//! verdict there is `unsupported`, pinned below).
//!
//! lupin (wolf-interp) is the differential's fourth lane and runs
//! through `cargo xtask differ`; at 0.1.13 it does not resolve the
//! net builtins at all (`unsupported@resolve` — a skip under
//! [proto.cmp], never a divergence), so the byte-equality asserted
//! here is the three wolfgang lanes'.
//!
//! Discipline: loopback + port 0 throughout, inherited from the
//! corpus files themselves. Hosts the native tier refuses skip loudly
//! at runtime (the s59 pattern).

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn corpus(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/net")
        .join(file)
}

struct Obs {
    verdict: String,
    stdout: String,
}

/// One conform-run lane over a corpus file. `None` (with a loud SKIP)
/// only for environment failures (exit 2: no cc/clang, no rt
/// staticlib); refusals stay visible as `unsupported` verdicts.
fn lane(file: &str, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(corpus(file))
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) && flag != "--checked" {
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {file}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    // The release tier refuses this HOST by name (linux/x86-64 only
    // until its own c13 sprint): a loud skip, not a verdict (s59).
    if flag == "--release"
        && rec["verdict"] == "unsupported"
        && String::from_utf8_lossy(&out.stderr).contains("release tier targets linux/x86-64")
    {
        eprintln!("SKIP: the release tier refuses this host");
        return None;
    }
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    })
}

/// Run all three lanes; assert the expected verdict/stdout on checked
/// and byte-identical observations on both native tiers.
fn parity(file: &str, want_verdict: &str, want_stdout: &str) {
    let checked = lane(file, "--checked").expect("checked lane always runs");
    assert_eq!(
        checked.verdict, want_verdict,
        "{file}: checked verdict (stdout {:?})",
        checked.stdout
    );
    assert_eq!(checked.stdout, want_stdout, "{file}: checked stdout");
    for flag in ["--native", "--release"] {
        let Some(native) = lane(file, flag) else {
            return;
        };
        assert_eq!(
            native.verdict, checked.verdict,
            "{file}: {flag} verdict diverges from checked"
        );
        assert_eq!(
            native.stdout, checked.stdout,
            "{file}: {flag} stdout diverges from checked"
        );
    }
}

/// The s39 litmus crosses: listen/port/connect/write/accept/read/
/// close agree on all three lanes, byte for byte.
#[test]
fn echo_roundtrip_agrees_on_every_lane() {
    parity("echo_roundtrip.lu", "exit(0)", "got: ping\nreply: pong\n");
}

/// The fail-pin: `refused` must be the tag on every lane — handled
/// by `else`, then propagated as the documented process outcome.
#[test]
fn refused_row_agrees_on_every_lane() {
    parity("refused_row.lu", "exit(1)", "handled: 0\nerror: refused\n");
}

/// The s106 timeout witness: `net_deadline` arms the budget and a
/// read against a silent peer times out — the tag declared at s39,
/// witnessed on every lane that can express it (#45's builtin half).
#[test]
fn read_deadline_timeout_agrees_on_every_lane() {
    parity("read_deadline.lu", "exit(1)", "error: timeout\n");
}

/// The acceptance clause, verbatim: the echo roundtrip runs TEN times
/// on each native tier, every observation byte-equal to the checked
/// lane's.
#[test]
fn echo_roundtrip_is_ten_run_stable_on_both_tiers() {
    let checked = lane("echo_roundtrip.lu", "--checked").expect("checked lane always runs");
    assert_eq!(checked.verdict, "exit(0)");
    for flag in ["--native", "--release"] {
        for run in 0..10 {
            let Some(obs) = lane("echo_roundtrip.lu", flag) else {
                return;
            };
            assert_eq!(
                obs.verdict, checked.verdict,
                "echo_roundtrip {flag} run {run}: verdict drifted from checked"
            );
            assert_eq!(
                obs.stdout, checked.stdout,
                "echo_roundtrip {flag} run {run}: stdout drifted from checked"
            );
        }
    }
}

/// Blocking honesty: the spawn-accept witness runs on both native
/// tiers (a parked accept neither deadlocks the dial nor starves the
/// scheduler — the ten-run half lives in conc_native.rs), while the
/// checked lane's honest verdict is `unsupported` (C1: structured
/// concurrency is deferred there wholesale — a skip, never a
/// divergence).
#[test]
fn spawn_accept_runs_native_and_checked_refuses_honestly() {
    let checked = lane("spawn_accept.lu", "--checked").expect("checked lane always runs");
    assert_eq!(
        checked.verdict, "unsupported",
        "checked refuses structured concurrency (C1 deferred) — if this ran, pin its parity here"
    );
    for flag in ["--native", "--release"] {
        let Some(obs) = lane("spawn_accept.lu", flag) else {
            return;
        };
        assert_eq!(obs.verdict, "exit(0)", "spawn_accept {flag} verdict");
        assert_eq!(obs.stdout, "echo: howl\n", "spawn_accept {flag} stdout");
    }
}
