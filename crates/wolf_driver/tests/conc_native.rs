//! s73 + s86 acceptance — native concurrency, end to end, both tiers.
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
//! s86 adds three things and one boundary:
//! - the RELEASE tier (wir → LLVM -O2 → clang) runs the same files at
//!   the same verdicts. Until s86 it refused `func.addr` — the
//!   compiled task-entry pointer — and that single opcode took every
//!   spawn-bearing program off the tier;
//! - a spawn UNDER A LOOP compiles, each reach getting its own capture
//!   record from the scope's arena;
//! - X12's ten-run seed stability is asserted on both tiers.
//!
//! The boundary: a PROC spawned in a loop still refuses, by name, and
//! a test holds it there — s87 owns that half.
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
    native_opt(file, seed, false)
}

/// As [`native`], with the s42 WIR mid-end forced on (`WOLF_MIDEND=1`
/// is the debug-tier override the driver reads; the release tier runs
/// it unconditionally). The optimizer must be behaviorally invisible.
fn native_opt(file: &str, seed: Option<u64>, midend: bool) -> Option<Obs> {
    native_tier(file, seed, midend, false)
}

/// As [`native_opt`], on either tier: `--native` alone is the Cranelift
/// debug tier, `--native --release` the LLVM one (s86 — before it, the
/// release tier refused every conc program by name at `func.addr`).
fn native_tier(file: &str, seed: Option<u64>, midend: bool, release: bool) -> Option<Obs> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(root.join(file))
        .arg("--native")
        .arg("--json");
    if release {
        cmd.arg("--release");
    }
    if midend {
        cmd.env("WOLF_MIDEND", "1");
    }
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

/// Issue #64: two selects with timeout arms in one body. The second
/// select's `-2` timeout sentinel is minted in a dispatch-chain block
/// that does not dominate the join, so GVN hash-consing it onto the
/// first select's definition ICE'd the verifier (`dominance: %N is not
/// dominated`). Lowering now scopes dispatch-chain constants like any
/// other arm-local value; this is the native regression witness beside
/// the WIR golden in `wolf_wir`'s `lower_corpus`.
#[test]
fn two_sequential_selects_with_timeouts_run() {
    ensure_rt_staticlib();
    pinned("corpus/conc/select_two_timeouts.lu", "exit(0)", "");
}

// ---- s42: the mid-end is behaviorally invisible ----------------------

/// **Conc conservatism, behaviorally** (the s42 litmus's other half —
/// the structural one is `wolf_wir`'s `midend_corpus.rs`).
///
/// Every spawn/channel/select corpus file, compiled through the WIR
/// mid-end and compiled without it, under the SAME `--seed`: identical
/// verdict, identical stdout. The optimizer sees the same schedule
/// points either way, so the seeded scheduler makes the same decisions
/// — a pass that sank, coalesced, or forwarded across a spawn edge or
/// a `__wolf_rt_*` schedule point would show up here as a divergence,
/// not as a crash somewhere downstream.
#[test]
fn mid_end_does_not_change_seeded_conc_behavior() {
    ensure_rt_staticlib();
    let files = [
        "corpus/conc/select_two_timeouts.lu",
        "corpus/conc/select_seeded.lu",
        "corpus/conc/message_passing.lu",
        "corpus/conc/cancel_sibling.lu",
        "corpus/conc/freeze_publish.lu",
        "corpus/conc/when_multi.lu",
        "corpus/conc/proc_cancel_defers.lu",
        "corpus/conc/proc_link.lu",
    ];
    let mut compared = 0;
    for file in files {
        for seed in [1u64, 2, 7] {
            let Some(plain) = native_opt(file, Some(seed), false) else {
                return; // environment cannot run the native lane
            };
            let opt = native_opt(file, Some(seed), true).expect("environment already proved");
            assert!(plain.seeded && opt.seeded, "{file}: both runs are seeded");
            assert_eq!(
                plain.verdict, opt.verdict,
                "{file} @ seed {seed}: the mid-end changed the verdict"
            );
            assert_eq!(
                plain.stdout, opt.stdout,
                "{file} @ seed {seed}: the mid-end changed stdout"
            );
            compared += 1;
        }
    }
    eprintln!("mid-end conc differential: {compared} seeded run pair(s) identical");
    assert_eq!(compared, 24, "every file × seed pair ran");
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

// ---- s86: a spawn under a loop ---------------------------------------

/// The fan-out shape, and the reason it was refused until s86.
///
/// The capture record used to be a frame slot. One slot, eight tasks:
/// each would have read whatever the LAST iteration stored, and the
/// witness would total 8 * 8^2 = 512. It totals 204 — the sum of the
/// squares — so every task got captures of its own. The total is
/// order-independent, so this asserts the CAPTURE, not the schedule.
#[test]
fn a_spawn_under_a_loop_gives_every_task_its_own_captures() {
    ensure_rt_staticlib();
    pinned("corpus/conc/spawn_fanout_loop.lu", "exit(0)", "204\n");
}

/// The arena is per SCOPE, so the shape survives the mid-end: region
/// promotion must not sink a record that escapes into the runtime, and
/// bump fusion must not merge two iterations' records into one.
#[test]
fn the_mid_end_does_not_merge_two_iterations_capture_records() {
    ensure_rt_staticlib();
    for seed in [1u64, 5, 9] {
        let Some(plain) = native_opt("corpus/conc/spawn_fanout_loop.lu", Some(seed), false) else {
            return;
        };
        let opt =
            native_opt("corpus/conc/spawn_fanout_loop.lu", Some(seed), true).expect("environment");
        assert_eq!(plain.verdict, "exit(0)", "seed {seed}: plain verdict");
        assert_eq!(opt.verdict, plain.verdict, "seed {seed}: mid-end verdict");
        assert_eq!(opt.stdout, plain.stdout, "seed {seed}: mid-end stdout");
        assert_eq!(opt.stdout, "204\n", "seed {seed}: the captures held");
    }
}

/// The other side of the same line, closed by s87: a PROC spawned in a
/// loop RUNS, on both native tiers.
///
/// s86's answer for tasks is the scope's arena; a proc has no scope.
/// s87's answer is the runtime's: `__wolf_rt_proc_spawn_outcome`
/// copies the argument record before it returns
/// (`[abi.native.procenv]`), so one frame slot per site is sound under
/// a loop — the slot is free the instant the id is back. Until s87
/// this test asserted the refusal and its owner; now it asserts the
/// run, seed-stable, the way every other conc witness does. (The
/// per-proc VALUE check is `wolf_rt`'s `proc_env_is_copied_at_spawn`,
/// where a body can be gated; a proc's only outputs are its exit class
/// and stdout.)
#[test]
fn a_proc_spawned_in_a_loop_runs_on_both_tiers() {
    ensure_rt_staticlib();
    pinned("corpus/conc/proc_spawn_loop.lu", "exit(0)", "");
    let Some(rel) = native_tier("corpus/conc/proc_spawn_loop.lu", Some(1), false, true) else {
        return;
    };
    assert_eq!(
        rel.verdict, "exit(0)",
        "release tier: a proc spawn in a loop runs"
    );
    assert_eq!(rel.stdout, "", "release tier: no output");
}

// ---- s86: the RELEASE tier runs concurrency ---------------------------

/// Every conc run-file, on the LLVM tier, at the verdict the debug
/// tier pins.
///
/// This is the sprint's headline: before s86 the release tier refused
/// `func.addr` — the compiled task-entry pointer — which took every
/// program containing a `spawn` with it. `conform-run --native
/// --release` recorded `phase_reached: wir, verdict: unsupported`,
/// which is exactly the symptom #104 reported (it read it as the whole
/// native story; it was the release half of it).
#[test]
fn the_release_tier_runs_the_conc_corpus() {
    ensure_rt_staticlib();
    let cases = [
        ("corpus/conc/message_passing.lu", "exit(0)", ""),
        ("corpus/conc/freeze_publish.lu", "exit(0)", ""),
        ("corpus/conc/cancel_sibling.lu", "exit(0)", ""),
        ("corpus/conc/when_multi.lu", "exit(0)", ""),
        ("corpus/conc/select_seeded.lu", "exit(0)", ""),
        ("corpus/conc/select_two_timeouts.lu", "exit(0)", ""),
        ("corpus/conc/select_single_arm_loop.lu", "exit(0)", ""),
        ("corpus/conc/spawn_fanout_loop.lu", "exit(0)", "204\n"),
        (
            "corpus/conc/chan_drain_after_inclusive_loop.lu",
            "exit(0)",
            "3 12\n",
        ),
        ("corpus/procs.lu", "exit(0)", ""),
    ];
    let mut ran = 0;
    for (file, verdict, stdout) in cases {
        let Some(o) = native_tier(file, Some(3), false, true) else {
            return; // no clang / no cc / no staticlib (loud)
        };
        assert_eq!(o.verdict, verdict, "{file}: release-tier verdict");
        assert_eq!(o.stdout, stdout, "{file}: release-tier stdout");
        ran += 1;
    }
    assert_eq!(ran, 10, "every release-tier conc case ran");
}

/// D14's signature distinction, on the release tier too.
///
/// The kill path skips defers and bulk-frees regions; the cancel path
/// runs them. c07 pinned the ORDER with runtime tests; s73 pinned it
/// natively on the debug tier; this pins it on the tier that actually
/// optimizes, where a mis-sunk defer or a hoisted free would show up
/// as the other file's stdout.
#[test]
fn the_defer_law_holds_on_the_release_tier() {
    ensure_rt_staticlib();
    let Some(killed) = native_tier("corpus/conc/proc_kill_defers.lu", Some(1), false, true) else {
        return;
    };
    assert_eq!(killed.verdict, "exit(0)");
    assert_eq!(
        killed.stdout, "released",
        "a KILLED proc's defers must not run — only the owner's release"
    );
    let cancelled = native_tier("corpus/conc/proc_cancel_defers.lu", Some(1), false, true)
        .expect("environment");
    assert_eq!(cancelled.verdict, "exit(0)");
    assert_eq!(
        cancelled.stdout, "defer-ran\nreleased",
        "a CANCELLED proc runs its defers, and before the reason lands"
    );
    let linked =
        native_tier("corpus/conc/proc_link.lu", Some(1), false, true).expect("environment");
    assert_eq!(linked.stdout, "both-down", "linked procs share fate");
}

// ---- s86: X12 determinism, ten runs -----------------------------------

/// X12's promise, stated as the sprint contract states it: the same
/// program, the same seed, the same output — ten runs.
///
/// Run on BOTH tiers, because determinism that only holds where the
/// optimizer is off is not a language property. The programs chosen
/// spawn, send, receive and select, so the scheduler makes real
/// decisions in every one of them.
#[test]
fn the_same_seed_gives_the_same_output_ten_times() {
    ensure_rt_staticlib();
    let files = [
        "corpus/conc/spawn_fanout_loop.lu",
        "corpus/conc/select_seeded.lu",
        "corpus/conc/message_passing.lu",
        "corpus/procs.lu",
    ];
    for release in [false, true] {
        for file in files {
            let Some(first) = native_tier(file, Some(1234), false, release) else {
                return;
            };
            assert!(first.seeded, "{file}: the record must report seeded=true");
            for run in 1..10 {
                let again = native_tier(file, Some(1234), false, release).expect("environment");
                assert_eq!(
                    again.verdict, first.verdict,
                    "{file} (release={release}) run {run}: verdict drifted under one seed"
                );
                assert_eq!(
                    again.stdout, first.stdout,
                    "{file} (release={release}) run {run}: stdout drifted under one seed"
                );
            }
            eprintln!(
                "X12: {file} (release={release}) — 10 runs at seed 1234, \
                 verdict {} stdout {:?}",
                first.verdict, first.stdout
            );
        }
    }
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
