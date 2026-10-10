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

mod lane_exit;

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
    if lane_exit::environment_refusal(&out, "wolf conform-run --release") {
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

/// The checked half. Until s226 the checked machine ran no structured
/// concurrency (C1 deferred) and refused this proc spawn by name, the
/// construct and its span riding the record as extension keys (#219,
/// `[proto.record.ext]`). It runs the proc now (`[exec.checked.task]`):
/// the cross-module spawn, the contained `alloc-contract` fault and
/// both joins, with the compiled tiers' answer and no refusal in the
/// record.
#[test]
fn the_checked_machine_runs_the_cross_module_proc() {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(witness())
        .args(["--json", "--checked"])
        .output()
        .expect("wolf runs");
    assert!(out.status.success());
    let r: serde_json::Value = serde_json::from_slice(&out.stdout).expect("a record");
    assert_eq!(r["verdict"], "exit(0)", "record {r}");
    assert_eq!(r["phase_reached"], "run");
    assert_eq!(r["stdout_inline"], "normal=0 breach=2\n");
    assert!(
        r.get("x-unsupported-construct").is_none(),
        "no refusal rides the record"
    );
}
