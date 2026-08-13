//! The gates test themselves (s44 acceptance: "CI regression gate
//! demonstrably fires on an injected 5% regression — test the gate itself
//! once").
//!
//! Two gates are exercised end to end through the real `xtask` binary:
//!
//! - `bench diff --gate`, the per-commit regression gate, against a
//!   synthetic 5% drop on a DETERMINISTIC metric. A gate nobody has ever
//!   seen fire is a gate nobody should trust; this is the receipt.
//! - `bench gate <t1.jsonl>`, the M2 verdict, against synthetic suites
//!   that should and should not declare M2 — including the case the
//!   contract cares most about, where the geomean clears 1.00 but one
//!   kernel loses badly and undocumented.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

fn scratch(name: &str) -> PathBuf {
    let dir = repo_root().join("target/bench-gate-tests");
    std::fs::create_dir_all(&dir).expect("mkdir scratch");
    dir.join(name)
}

/// Run the real binary from the repo root, the way CI does.
fn xtask(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("run xtask")
}

fn write(name: &str, lines: &[String]) -> PathBuf {
    let p = scratch(name);
    std::fs::write(&p, format!("{}\n", lines.join("\n"))).expect("write jsonl");
    p
}

/// One `bench diff`-shaped record on a deterministic (gating) metric.
fn hits(value: f64) -> String {
    serde_json::json!({
        "bench": "wir", "track": "compile", "lang": "rust",
        "metric": "wir_gvn_hits", "value": value, "unit": "hits",
        "commit": "test", "config": "synthetic", "style": "test"
    })
    .to_string()
}

#[test]
fn diff_gate_fires_on_an_injected_five_percent_regression() {
    // Ten samples with no spread: MAD is zero, so the 2% practical floor
    // is what a candidate must clear — 5% clears it.
    let base: Vec<String> = (0..10).map(|_| hits(1000.0)).collect();
    let cand: Vec<String> = (0..10).map(|_| hits(950.0)).collect();
    let b = write("gate-base.jsonl", &base);
    let c = write("gate-cand.jsonl", &cand);

    let out = xtask(&[
        "bench",
        "diff",
        b.to_str().expect("utf8"),
        c.to_str().expect("utf8"),
        "--gate",
    ]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "the gate did NOT fire on a 5% drop in a deterministic metric:\n{err}"
    );
    assert!(
        err.contains("REGRESSED"),
        "the gate fired without naming the regression:\n{err}"
    );

    // The same file against itself must pass, or the gate is just noise.
    let out = xtask(&[
        "bench",
        "diff",
        b.to_str().expect("utf8"),
        b.to_str().expect("utf8"),
        "--gate",
    ]);
    assert!(
        out.status.success(),
        "the gate fired on an identical candidate:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn diff_gate_does_not_fire_on_wall_derived_metrics() {
    // D5, the lesson from s31's first gate trip: wall-derived numbers are
    // reported, never gated, until a variance floor exists for them.
    let wall = |v: f64| {
        serde_json::json!({
            "bench": "nmod", "track": "compile", "lang": "wolf",
            "metric": "clean_build_wall_s", "value": v, "unit": "s",
            "commit": "test", "config": "synthetic", "style": "test"
        })
        .to_string()
    };
    let b = write(
        "gate-wall-base.jsonl",
        &(0..10).map(|_| wall(1.0)).collect::<Vec<_>>(),
    );
    let c = write(
        "gate-wall-cand.jsonl",
        &(0..10).map(|_| wall(2.0)).collect::<Vec<_>>(),
    );
    let out = xtask(&[
        "bench",
        "diff",
        b.to_str().expect("utf8"),
        c.to_str().expect("utf8"),
        "--gate",
    ]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "a doubled wall time failed the merge gate; wall-derived metrics are report-only:\n{err}"
    );
    assert!(
        err.contains("report-only"),
        "the 100% wall regression was not even reported:\n{err}"
    );
}

/// A `--track=t1` speedup record plus its kernel's layout floor.
fn t1_kernel(name: &str, family: &str, speedup: f64, floor: f64, verdict: &str) -> Vec<String> {
    vec![
        serde_json::json!({
            "bench": name, "track": "t1", "lang": "suite",
            "metric": "layout_noise_floor", "value": floor, "unit": "ratio",
            "commit": "test", "config": "synthetic", "style": "test",
            "family": family
        })
        .to_string(),
        serde_json::json!({
            "bench": name, "track": "t1", "lang": "wolf",
            "metric": "speedup_vs_c_naive", "value": speedup, "unit": "ratio",
            "commit": "test", "config": "synthetic", "style": "test",
            "family": family, "verdict": verdict
        })
        .to_string(),
    ]
}

#[test]
fn m2_gate_refuses_a_suite_that_loses() {
    let mut lines = Vec::new();
    lines.extend(t1_kernel("k1", "E", 0.40, 0.02, "LOSS"));
    lines.extend(t1_kernel("k2", "A", 0.05, 0.02, "LOSS"));
    let p = write("m2-losing.jsonl", &lines);
    let out = xtask(&["bench", "gate", p.to_str().expect("utf8")]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "M2 was declared on a losing suite:\n{err}"
    );
    assert!(err.contains("DOES NOT HOLD"), "{err}");
    assert!(err.contains("UNDOCUMENTED"), "{err}");
}

#[test]
fn m2_gate_refuses_a_geomean_that_hides_a_family_loss() {
    // geomean(4.0, 0.25) == 1.0: the geomean clause passes and the gate
    // must still refuse, because one kernel loses 300% undocumented.
    let mut lines = Vec::new();
    lines.extend(t1_kernel("fast", "E", 4.0, 0.02, "WIN"));
    lines.extend(t1_kernel("slow", "A", 0.25, 0.02, "LOSS"));
    let p = write("m2-hidden.jsonl", &lines);
    let out = xtask(&["bench", "gate", p.to_str().expect("utf8")]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("suite geomean: 1.000"),
        "expected the geomean clause to pass:\n{err}"
    );
    assert!(
        !out.status.success(),
        "a geomean of exactly 1.0 laundered a 4x family loss:\n{err}"
    );
}

#[test]
fn m2_gate_accepts_a_winning_suite() {
    let mut lines = Vec::new();
    lines.extend(t1_kernel("k1", "E", 1.30, 0.02, "WIN"));
    lines.extend(t1_kernel("k2", "A", 1.10, 0.02, "WIN"));
    lines.extend(t1_kernel("k3", "D", 1.01, 0.02, "TIE"));
    let p = write("m2-winning.jsonl", &lines);
    let out = xtask(&["bench", "gate", p.to_str().expect("utf8")]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "a winning suite failed the gate:\n{err}"
    );
    assert!(err.contains("HOLDS"), "{err}");
}

#[test]
fn m2_gate_rejects_a_report_whose_stored_verdict_disagrees() {
    // The report JSON and the decision procedure must not drift apart: a
    // stored "WIN" on a ratio inside the floor is a bug in one of them,
    // and the gate refuses to guess which.
    let lines = t1_kernel("k1", "E", 1.01, 0.10, "WIN");
    let p = write("m2-inconsistent.jsonl", &lines);
    let out = xtask(&["bench", "gate", p.to_str().expect("utf8")]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{err}");
    assert!(err.contains("diverged"), "{err}");
}

#[test]
fn m2_gate_needs_data() {
    let p = write("m2-empty.jsonl", &[hits(1.0)]);
    let out = xtask(&["bench", "gate", p.to_str().expect("utf8")]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no speedup_vs_c_naive records"), "{err}");
}
