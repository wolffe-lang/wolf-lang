//! wolf-lang#1115 and #1076: storage that escaped the memory checker,
//! refused statically on every compiled machine.
//!
//! Each refusal row is answered by the checked machine, native and
//! release with the same code at the same span: the front end is shared,
//! so a machine that disagrees is a defect, never a posture. Each control
//! runs on all three with one stdout (one `stdout_sha256`), and on lupin
//! too when it is present. lupin is the oracle for #1115's dynamic
//! counterpart: it traps `region-fault` where the compiled tiers used to
//! read freed bytes.
mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MACHINES: [&str; 3] = ["--checked", "--native", "--release"];

fn corpus(row: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(row)
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
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .args(["conform-run", "--json", machine])
        .arg(corpus(row))
        .output()
        .expect("run wolf");
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
    let out = Command::new(lupin)
        .args(["conform-run", "--json"])
        .arg(corpus(row))
        .output()
        .expect("run lupin");
    Some(record(&out, row, "lupin"))
}

/// The byte span of `marker`'s `skip..skip+len` slice in the row, where
/// `marker` occurs exactly once.
fn span_of(row: &str, marker: &str, skip: usize, len: usize) -> (u64, u64) {
    let src = std::fs::read_to_string(corpus(row)).expect("read row");
    let at: Vec<usize> = src.match_indices(marker).map(|(i, _)| i).collect();
    assert_eq!(
        at.len(),
        1,
        "{row}: `{marker}` must occur once, found {at:?}"
    );
    let lo = (at[0] + skip) as u64;
    (lo, lo + len as u64)
}

/// The refusal: the verdict names `code`, and an error diagnostic with
/// that code sits at exactly `span`, on every compiled machine.
fn refused(row: &str, code: &str, span: (u64, u64)) {
    for machine in MACHINES {
        let Some(rec) = conform(row, machine) else {
            continue;
        };
        assert_eq!(
            rec["verdict"],
            format!("fail({code})"),
            "{machine} {row}: {rec}"
        );
        let at: Vec<(u64, u64)> = rec["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d["code"] == code)
            .map(|d| {
                (
                    d["span"][0].as_u64().unwrap_or(u64::MAX),
                    d["span"][1].as_u64().unwrap_or(u64::MAX),
                )
            })
            .collect();
        assert_eq!(at, vec![span], "{machine} {row}: {code} spans: {rec}");
    }
}

/// The control: exit 0 with `stdout` on every compiled machine, one
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

/// lupin's dynamic answer for a payload read after its region is freed.
fn lupin_traps_region_fault(row: &str) {
    if let Some(rec) = lupin_record(row) {
        assert_eq!(rec["verdict"], "trap(region-fault)", "lupin {row}: {rec}");
    }
}

const INTACT: &str =
    "intact 0: bound 3 exceeded by the request, which asked for more rows than the table holds\n";

// #1115: an error payload built in a `region` block, propagated by `?`.

#[test]
fn a_payload_built_in_a_region_block_cannot_leave_it_by_try() {
    let row = "memory/region_try_payload_escape.lu";
    refused(row, "E1010", span_of(row, "inner(3)?", 0, 9));
    lupin_traps_region_fault(row);
}

#[test]
fn a_payload_bound_in_the_block_cannot_leave_it_by_try() {
    let row = "memory/region_try_payload_bound.lu";
    refused(row, "E1010", span_of(row, "res?", 0, 4));
    lupin_traps_region_fault(row);
}

#[test]
fn a_payload_beside_a_list_ok_value_is_judged_alone() {
    let row = "memory/region_try_payload_okheap.lu";
    refused(row, "E1010", span_of(row, "make(3)?", 0, 8));
    lupin_traps_region_fault(row);
}

#[test]
fn a_literal_payload_leaves_a_region_block_by_try() {
    runs("memory/region_try_payload_literal.lu", INTACT);
}

#[test]
fn a_try_before_the_region_block_is_legal() {
    runs("memory/region_try_payload_outside.lu", INTACT);
}

#[test]
fn a_list_ok_value_in_the_block_does_not_refuse_a_literal_payload() {
    runs("memory/region_try_payload_okheap_literal.lu", INTACT);
}

// #1076: a closure parameter's mode.

#[test]
fn a_closures_mode_less_parameter_cannot_be_assigned() {
    let row = "typecheck/closure_param_read_assign.lu";
    refused(row, "E1014", span_of(row, "a = a + 1", 0, 1));
}

#[test]
fn a_closures_mut_parameter_can_be_assigned() {
    runs("typecheck/closure_param_mut_assign.lu", "2\n");
}
