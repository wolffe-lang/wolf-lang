//! s73 acceptance — native concurrency, end to end.
//!
//! The conc corpus tier's run files execute NATIVELY (wir → Cranelift
//! → cc → the s32–s36 runtime) at the exit/stdout expectations the
//! reference machine pinned (the lupin exit-parity discipline: the
//! `check:` headers ARE the vendored verdicts). The checked lane
//! stays an honest `unsupported` for concurrency (C1 deferred — the
//! `[proto.cmp]` rule keeps that a non-divergence), so parity here is
//! native-vs-pinned, plus verdict STABILITY under `--seed`
//! ([sched.stable]'s CI property: same seed ⇒ same verdict and
//! stdout; different seeds ⇒ same VERDICT for these fixtures).
//!
//! Off-target the whole file compiles away (native codegen is
//! linux/x86-64 only at this tier).

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::path::Path;
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

struct Obs {
    verdict: String,
    stdout: String,
    seeded: bool,
}

/// One native conform-run over a corpus file, optionally seeded.
/// `None` (with a loud SKIP) only for environment failures.
fn native(file: &str, seed: Option<u64>) -> Option<Obs> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(root.join(file))
        .arg("--native")
        .arg("--json");
    if let Some(s) = seed {
        cmd.arg(format!("--seed={s}"));
    }
    let out = cmd.output().expect("wolf runs");
    if out.status.code() == Some(2) {
        eprintln!(
            "SKIP: environment cannot run the native lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run --native failed on {file}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        seeded: rec["seeded"].as_bool().unwrap_or(false),
    })
}

/// The pinned expectation, seed-stable: two different seeds and one
/// repeat of the first — verdict identical everywhere, stdout
/// identical for the SAME seed (and, for these deterministic
/// fixtures, across seeds too).
fn pinned(file: &str, want_verdict: &str, want_stdout: &str) {
    let Some(a) = native(file, Some(1)) else {
        return;
    };
    assert!(a.seeded, "{file}: the record must report seeded=true");
    assert_eq!(a.verdict, want_verdict, "{file}: verdict under seed 1");
    assert_eq!(a.stdout, want_stdout, "{file}: stdout under seed 1");
    let b = native(file, Some(2)).expect("environment already proved");
    assert_eq!(b.verdict, want_verdict, "{file}: verdict under seed 2");
    assert_eq!(b.stdout, want_stdout, "{file}: stdout under seed 2");
    let a2 = native(file, Some(1)).expect("environment already proved");
    assert_eq!(a2.verdict, a.verdict, "{file}: seed 1 replays its verdict");
    assert_eq!(a2.stdout, a.stdout, "{file}: seed 1 replays its stdout");
}

// ---- scope/spawn/channel/select/when ---------------------------------


/// The native lanes link `libwolf_rt.a` found next to the `wolf`
/// binary; `cargo test` alone does not produce the staticlib on a
/// fresh target (CI's release-parity lane silently skipped for
/// exactly this reason). Build it once, deterministically.
fn ensure_rt_staticlib() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let status = std::process::Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .expect("cargo builds wolf_rt");
        assert!(status.success(), "wolf_rt staticlib build failed");
    });
}

#[test]
fn select_two_ready_arms_is_conforming_either_way() {
    ensure_rt_staticlib();
    pinned("corpus/conc/select_seeded.lu", "exit(0)", "");
}

#[test]
fn when_multi_acquires_whole_sets() {
    ensure_rt_staticlib();
    pinned("corpus/conc/when_multi.lu", "exit(0)", "");
}

#[test]
fn freeze_publishes_across_tasks() {
    ensure_rt_staticlib();
    pinned("corpus/conc/freeze_publish.lu", "exit(0)", "");
}

#[test]
fn message_passing_moves_the_region() {
    ensure_rt_staticlib();
    pinned("corpus/conc/message_passing.lu", "exit(0)", "");
}

#[test]
fn failing_child_cancels_sibling_and_reraises() {
    ensure_rt_staticlib();
    pinned("corpus/conc/cancel_sibling.lu", "exit(0)", "");
}

// ---- procs: the kill-vs-cancel law (D14's signature distinction) -----

#[test]
fn killed_proc_skips_defers() {
    ensure_rt_staticlib();
    pinned("corpus/conc/proc_kill_defers.lu", "exit(0)", "released");
}

#[test]
fn cancelled_proc_runs_defers() {
    ensure_rt_staticlib();
    pinned(
        "corpus/conc/proc_cancel_defers.lu",
        "exit(0)",
        "defer-ran\nreleased",
    );
}

#[test]
fn linked_procs_share_fate() {
    ensure_rt_staticlib();
    pinned("corpus/conc/proc_link.lu", "exit(0)", "both-down");
}

#[test]
fn the_kitchen_sink_procs_file_runs() {
    ensure_rt_staticlib();
    pinned("corpus/procs.lu", "exit(0)", "");
}

// ---- the dogfood witness (X12 end to end) ----------------------------

#[test]
fn wolf_test_schedules_explores_a_native_body() {
    ensure_rt_staticlib();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = Command::new(wolf())
        .arg("test")
        .arg(root.join("corpus/test/conc_schedules_test.lu"))
        .arg("--schedules=4")
        .arg("--json")
        .env("WOLF_SCHED_SEED", "7")
        .output()
        .expect("wolf test runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if stderr.contains("cannot") && out.status.code() == Some(2) {
        eprintln!("SKIP: environment cannot run the native test lane");
        return;
    }
    assert!(
        stdout.contains("[native, 4 schedule(s)]"),
        "the native schedule lane must run: {stdout} {stderr}"
    );
    assert!(
        out.status.success(),
        "exploration must pass verdict-stably: {stdout} {stderr}"
    );
    // Replay: one decimal seed, one native run.
    let out = Command::new(wolf())
        .arg("test")
        .arg(root.join("corpus/test/conc_schedules_test.lu"))
        .arg("--replay=42")
        .arg("--json")
        .output()
        .expect("wolf test runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[native, 1 schedule(s)]"),
        "replay must run the native lane once: {stdout}"
    );
    assert!(out.status.success(), "replay must pass: {stdout}");
}
