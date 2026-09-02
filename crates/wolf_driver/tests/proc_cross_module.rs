//! s134 (#219) — a proc may be spawned from any module, on the release
//! tier, under EVERY partition.
//!
//! lobo ws13 measured the shape: `spawn proc` in a non-entry module
//! built and ran natively and refused on `wolf build --release` with
//! `func.addr of @work.run.task0.entry outside this object's subset`
//! — #136's proc twin, one partition over. The s117 `refs=` edge keeps
//! a spawner and its entry shim in one CLUSTER; the per-module
//! partition (`WOLF_MIDEND=0`, the measurement mode lobo's gauntlet
//! runs in while #146 is open) never consulted it: the shim is
//! synthetic (`src_file = None`) and rides the ROOT module's object
//! while the spawner sits in its own. The debug tier imported the
//! symbol across objects (#116); the LLVM tier refused. Now both take
//! an out-of-subset referee's address by its mangled symbol.
//!
//! The corpus witness (`conc/proc_cross_module`) pins the shape on
//! every lane; this test pins the PARTITION that broke, because with
//! the whole-program phase on a thirty-line program is one cluster
//! and the refusal cannot fire. Hosts without a release tier skip
//! loudly (the s59 pattern).

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn witness() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/conc/proc_cross_module/main.lu")
}

fn ensure_rt_staticlib() {
    let bin_dir = Path::new(wolf()).parent().unwrap().to_path_buf();
    let lib = bin_dir.join("libwolf_rt.a");
    if lib.exists() {
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let st = Command::new("cargo")
        .args(["build", "-p", "wolf_rt", "--quiet"])
        .current_dir(&root)
        .status()
        .expect("cargo runs");
    assert!(st.success(), "building wolf_rt");
}

/// One release-lane conform-run, `midend` selecting the whole-program
/// phase (clusters) or the per-module partition (`WOLF_MIDEND=0`).
/// `None` only on an environment refusal (exit 2), reported loudly.
fn release_lane(midend: bool) -> Option<serde_json::Value> {
    ensure_rt_staticlib();
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(witness())
        .args(["--json", "--release"]);
    if !midend {
        cmd.env("WOLF_MIDEND", "0");
    }
    let out = cmd.output().expect("wolf runs");
    if out.status.code() == Some(2) {
        eprintln!(
            "SKIP: environment cannot run the release lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run exited {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(serde_json::from_slice(&out.stdout).expect("a record"))
}

#[test]
fn a_proc_spawned_from_a_leaf_module_runs_on_release_under_both_partitions() {
    for midend in [true, false] {
        let Some(r) = release_lane(midend) else {
            return;
        };
        let which = if midend {
            "whole-program clusters"
        } else {
            "per-module partition (WOLF_MIDEND=0)"
        };
        assert_eq!(
            r["verdict"], "exit(0)",
            "{which}: the release tier must run the cross-module proc; record {r}"
        );
        assert_eq!(
            r["stdout_inline"], "normal=0 breach=2\n",
            "{which}: the join's reasons (normal, then alloc-contract)"
        );
        assert!(
            r.get("x-unsupported-construct").is_none(),
            "{which}: no refusal rode the record"
        );
    }
}

/// The checked half of #219: `conform-run --checked` answered
/// `{"verdict":"unsupported","diagnostics":[]}` on every proc spawn
/// — by name on stderr, and NOTHING in the record a rig reads over a
/// pipe. The record now names the construct and its span as
/// extension keys (`[proto.record.ext]`); the verdict and the empty
/// `diagnostics` are unchanged (a refusal is not a fault in the
/// program, so it carries no E-code — the conservatism ledger's own
/// rule). The checked machine runs no structured concurrency at all
/// (C1 deferred); a proc is refused where every spawn is.
#[test]
fn the_checked_refusal_names_its_construct_in_the_record() {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(witness())
        .args(["--json", "--checked"])
        .output()
        .expect("wolf runs");
    assert!(out.status.success());
    let r: serde_json::Value = serde_json::from_slice(&out.stdout).expect("a record");
    assert_eq!(r["verdict"], "unsupported");
    assert_eq!(r["phase_reached"], "mem");
    assert_eq!(r["diagnostics"], serde_json::json!([]));
    let construct = r["x-unsupported-construct"]
        .as_str()
        .expect("the record names the refused construct");
    assert!(
        construct.contains("structured concurrency"),
        "the checked machine refuses the spawn by name: {construct}"
    );
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/conc/proc_cross_module/work/work.lu"),
    )
    .unwrap();
    let span = r["x-unsupported-span"]
        .as_array()
        .expect("a span rides too");
    let (lo, hi) = (
        span[0].as_u64().unwrap() as usize,
        span[1].as_u64().unwrap() as usize,
    );
    assert_eq!(
        &src[lo..hi],
        "spawn proc body(n, cap)",
        "the span is the spawn"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported — structured concurrency"),
        "stderr keeps speaking: {stderr}"
    );
}
