//! The checked machine agrees with native, release and lupin on five
//! programs it used to answer differently: one wrong answer and four
//! refusals of programs every other machine runs.
//!
//! - wolf-lang#1050: a fallible call in an interpolation hole is read
//!   (`[type.interp.union]`); the checked machine propagated its row and
//!   exited 1.
//! - wolf-lang#981: a nested call in a generic call's argument list took
//!   the callee's type bindings (`[type.generic.bind]`).
//! - wolf-lang#402: `T` bound through a parameter typed `List[T]`
//!   (`[type.generic.bind]`) stayed unbound under `<`.
//! - wolf-lang#828: the value a call returned was never applied
//!   (`[type.fn.value]`).
//! - wolf-lang#882: a `match` spent its scrutinee, so a `_` arm could not
//!   read it again (`[mem.tier0.move.1]`); std.json's renderer has the
//!   shape.
//!
//! Each row runs on the checked machine, native and release with one
//! stdout (one `stdout_sha256`), and on lupin too when it is present
//! (`LUPIN`, or a sibling wolf-interp build).
mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MACHINES: [&str; 3] = ["--checked", "--native", "--release"];

fn corpus(row: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(row)
}

/// The row's fixture std root (a `std` directory beside it), passed as
/// xtask's corpus walk passes it.
fn std_root(row: &str) -> Option<PathBuf> {
    corpus(row)
        .parent()
        .map(|d| d.join("std"))
        .filter(|d| d.is_dir())
}

fn record(out: &Output, row: &str, machine: &str) -> serde_json::Value {
    assert!(
        out.status.success(),
        "{machine} {row}: {}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{machine} {row}: {e}: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

/// One compiled machine's record, or `None` when the host cannot run
/// that tier (a loud SKIP, refused under `WOLF_TEST_NATIVE_REQUIRE`).
fn conform(row: &str, machine: &str) -> Option<serde_json::Value> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wolf"));
    cmd.args(["conform-run", "--json", machine]);
    if let Some(root) = std_root(row) {
        cmd.arg("--std-root").arg(root);
    }
    let out = cmd.arg(corpus(row)).output().expect("run wolf");
    if machine != "--checked" && lane_exit::environment_refusal(&out, machine) {
        assert!(
            std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
            "required {machine} unavailable: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        eprintln!(
            "SKIP: {machine} {row}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        return None;
    }
    Some(record(&out, row, machine))
}

fn lupin() -> Option<PathBuf> {
    let found = std::env::var_os("LUPIN").map(PathBuf::from).or_else(|| {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("../wolf-interp/target/release/lupin");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    });
    if found.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "required lupin missing"
        );
    }
    found
}

fn lupin_record(row: &str) -> Option<serde_json::Value> {
    let Some(lupin) = lupin() else {
        eprintln!("SKIP: lupin {row}: set LUPIN to run the fourth machine");
        return None;
    };
    let mut cmd = Command::new(lupin);
    cmd.args(["conform-run", "--json"]);
    if let Some(root) = std_root(row) {
        cmd.arg("--std-root").arg(root);
    }
    let out = cmd.arg(corpus(row)).output().expect("run lupin");
    Some(record(&out, row, "lupin"))
}

/// Exit 0 with `stdout` on every compiled machine, no diagnostic, one
/// `stdout_sha256` among them, and lupin's answer the same.
fn runs(row: &str, stdout: &str) {
    let mut shas: Vec<String> = Vec::new();
    for machine in MACHINES {
        let Some(rec) = conform(row, machine) else {
            continue;
        };
        assert_eq!(rec["verdict"], "exit(0)", "{machine} {row}: {rec}");
        assert_eq!(rec["stdout_inline"], stdout, "{machine} {row}: {rec}");
        let errors = rec["diagnostics"].as_array().map(Vec::len).unwrap_or(0);
        assert_eq!(errors, 0, "{machine} {row}: {rec}");
        shas.push(rec["stdout_sha256"].as_str().unwrap_or("").to_string());
    }
    shas.dedup();
    assert!(shas.len() <= 1, "{row}: the machines disagree: {shas:?}");
    if let Some(rec) = lupin_record(row) {
        assert_eq!(rec["verdict"], "exit(0)", "lupin {row}: {rec}");
        assert_eq!(rec["stdout_inline"], stdout, "lupin {row}: {rec}");
    }
}

#[test]
fn interp_hole_fallible_call() {
    runs(
        "strings/interp_hole_fallible_call.lu",
        "outside\noutside\n3\nnear outside\n",
    );
}

#[test]
fn dispatch_nested_call_arg() {
    runs("traits/dispatch_nested_call_arg.lu", "nested 7\nbound 14\n");
}

#[test]
fn ord_bound_list_elems() {
    runs("methods/ord_bound_list_elems.lu", "true\nfalse\ntrue\n");
}

#[test]
fn closure_returned_call() {
    runs("typecheck/closure_returned_call.lu", "14\n9\n12\n");
}

#[test]
fn match_wildcard_reads_scrutinee() {
    runs("memory/match_wildcard_reads_scrutinee.lu", "1\n0\n12\n1\n");
}
