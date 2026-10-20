//! s251 (wolf-lang#806, #807) — a closure made in one object and
//! called in another, a named function of one file of a module read as
//! a value in another, and a one-line maker inlined across a cluster
//! cut: each builds and runs on BOTH tiers under EVERY partition.
//!
//! The six rows under `corpus/typecheck/` pin the shapes on every lane
//! (`release_native.rs` holds the two compiled tiers to one verdict);
//! this test pins the PARTITIONS, as `proc_cross_module.rs` does for
//! s134: the release tier under the whole-program phase (clusters —
//! where dedup merged a record's entry away, rows 1–4, and where the
//! import decision relocated a maker, row 6) and under
//! `WOLF_MIDEND=0` (per-module objects — where the synthetic `.fv`
//! shim rode the root module's object, row 5), and the native tier
//! (the same per-module objects, no mid-end). Each row's refusal at
//! 808ce11a is named in its header; `[abi.native.closure.object]` is
//! the clause. Hosts without a compiling lane skip loudly (the s59
//! pattern; an ICE or a link error is never a skip, wolf-lang#471).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// (row directory, the stdout every lane must print).
const ROWS: &[(&str, &str)] = &[
    ("closure_entry_to_module", "dumb\n"),
    ("closure_module_to_entry", "dumb\n"),
    ("closure_module_to_module", "dumb\n"),
    ("closure_captured_closure", "dumb\n"),
    ("fn_value_two_file_module", "dumb\n"),
    ("closure_inlined_one_liner", "dumb\n613069\n"),
];

fn witness(dir: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/typecheck")
        .join(dir)
        .join("main.lu")
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

/// One conform-run of `dir` on `lane` (`--native` or `--release`);
/// `midend` selects the whole-program phase or the per-module
/// partition (`WOLF_MIDEND=0`). `None` only on an environment refusal
/// (exit 2 naming no ICE and no symbol), reported loudly.
fn lane(dir: &str, flag: &str, midend: bool) -> Option<serde_json::Value> {
    ensure_rt_staticlib();
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(witness(dir))
        .args(["--json", flag]);
    if !midend {
        cmd.env("WOLF_MIDEND", "0");
    }
    let out = cmd.output().expect("wolf runs");
    let what = format!("wolf conform-run {flag} ({dir})");
    if lane_exit::environment_refusal(&out, &what) {
        eprintln!(
            "SKIP: environment cannot run {what}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "{what} exited {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(serde_json::from_slice(&out.stdout).expect("a record"))
}

fn check(dir: &str, stdout: &str, which: &str, r: &serde_json::Value) {
    assert_eq!(
        r["verdict"], "exit(0)",
        "{dir} under {which}: the program must run; record {r}"
    );
    assert_eq!(
        r["stdout_inline"], stdout,
        "{dir} under {which}: the native tier's output"
    );
    assert!(
        r.get("x-unsupported-construct").is_none(),
        "{dir} under {which}: no refusal rode the record"
    );
}

/// The release tier, clusters (the whole-program phase): rows 1–4 were
/// dedup's refusal here, row 6 the import decision's.
#[test]
fn the_six_rows_run_on_release_under_clusters() {
    for (dir, stdout) in ROWS {
        let Some(r) = lane(dir, "--release", true) else {
            return;
        };
        check(dir, stdout, "release, whole-program clusters", &r);
    }
}

/// The release tier, per-module objects (`WOLF_MIDEND=0`): row 5 was
/// the partition's refusal here (the `.fv` shim in the root's object).
#[test]
fn the_six_rows_run_on_release_under_the_per_module_partition() {
    for (dir, stdout) in ROWS {
        let Some(r) = lane(dir, "--release", false) else {
            return;
        };
        check(dir, stdout, "release, per-module partition (WOLF_MIDEND=0)", &r);
    }
}

/// The native tier (per-module objects, no mid-end): row 5 was its
/// refusal; the other five were green at 808ce11a and stay so.
#[test]
fn the_six_rows_run_on_the_native_tier() {
    for (dir, stdout) in ROWS {
        let Some(r) = lane(dir, "--native", true) else {
            return;
        };
        check(dir, stdout, "native", &r);
    }
}

/// The checked machine runs the same six programs to the same output.
#[test]
fn the_checked_machine_runs_the_six_rows() {
    for (dir, stdout) in ROWS {
        let out = Command::new(wolf())
            .arg("conform-run")
            .arg(witness(dir))
            .args(["--json", "--checked"])
            .output()
            .expect("wolf runs");
        assert!(out.status.success(), "{dir}: conform-run --checked exited {:?}", out.status.code());
        let r: serde_json::Value = serde_json::from_slice(&out.stdout).expect("a record");
        assert_eq!(r["phase_reached"], "run", "{dir}: record {r}");
        check(dir, stdout, "checked", &r);
    }
}
