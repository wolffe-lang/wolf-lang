//! s270: reduced compiler regressions compared against independent answers on
//! all four machines. A compiler ICE or language refusal is never a skip.
mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn assert_answer(out: &Output, row: &str, machine: &str, stdout: &str) {
    assert!(
        out.status.success(),
        "{machine} {row}: {}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let record: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{machine} {row}: {e}: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    assert_eq!(record["verdict"], "exit(0)", "{machine} {row}: {record}");
    assert_eq!(record["stdout_inline"], stdout, "{machine} {row}: {record}");
}

fn four_machines(row: &str, stdout: &str) {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(row);
    for machine in ["--checked", "--native", "--release"] {
        let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
            .args(["conform-run", "--json", machine])
            .arg(&entry)
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
            continue;
        }
        assert_answer(&out, row, machine, stdout);
    }
    let lupin = std::env::var_os("LUPIN").map(PathBuf::from).or_else(|| {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("../wolf-interp/target/release/lupin");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    });
    let Some(lupin) = lupin else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "required lupin missing"
        );
        eprintln!("SKIP: lupin {row}: set LUPIN to run the fourth machine");
        return;
    };
    let out = Command::new(lupin)
        .args(["conform-run", "--json"])
        .arg(entry)
        .output()
        .expect("run lupin");
    assert_answer(&out, row, "lupin", stdout);
}

#[test]
fn bare_enum_variant_initializes_its_unused_string_slot() {
    four_machines(
        "typecheck/enum_str_bare_choice.lu",
        "T.Text(x)\nT.Nothing\n",
    );
}

#[test]
fn mixed_error_payload_display_projects_the_active_tag() {
    four_machines("rows/mixed_payload_display.lu", "A(0)\nB(p)\nA(0)\nB(p)\n");
}

#[test]
fn a_relayed_mut_list_keeps_its_callees_pushes() {
    four_machines("memory/mut_relay_push.lu", "6\n2\n7\n");
}

#[test]
fn a_mut_list_replaced_by_its_callee_is_the_new_list() {
    four_machines("memory/mut_param_replaced.lu", "6\n");
}

// s281 (wolf-lang#1043): a bare payload-less variant nested in a payload
// pattern tests its variant on every machine. The first four rows were red
// on native and release before the fix (the arm took every value of the
// outer tag); the last five are controls that were green everywhere.

#[test]
fn a_bare_variant_nested_in_a_payload_tests_its_variant() {
    four_machines(
        "grammar/match_nested_bare_variant.lu",
        "other\ndrink\nother\n",
    );
}

#[test]
fn a_bare_variant_in_either_payload_slot_tests_that_slot() {
    four_machines(
        "grammar/match_nested_bare_variant_fields.lu",
        "275\n1\n30\n200\n0\n30\n3\n",
    );
}

#[test]
fn a_guarded_arm_tests_its_nested_bare_variant_first() {
    four_machines(
        "grammar/match_nested_bare_variant_guard.lu",
        "other\nbig drink\ndrink\n",
    );
}

#[test]
fn a_bare_variant_in_a_row_tags_payload_tests_its_variant() {
    four_machines(
        "rows/match_row_nested_bare_variant.lu",
        "other\ndrink\nvalue 5\n8\n7\n",
    );
}

#[test]
fn a_bare_variant_two_levels_down_tests_its_variant() {
    four_machines(
        "grammar/match_nested_bare_variant_deep.lu",
        "other\ndrink\nnope\n",
    );
}

#[test]
fn bare_variants_in_a_payload_or_pattern_test_the_slot() {
    four_machines(
        "grammar/match_nested_bare_variant_or.lu",
        "other\nconsumable\nconsumable\n",
    );
}

#[test]
fn a_qualified_nested_variant_tests_the_slot() {
    four_machines(
        "grammar/match_nested_qualified_variant.lu",
        "other\ndrink\n",
    );
}

#[test]
fn a_nested_name_that_is_no_variant_still_binds() {
    four_machines("grammar/match_nested_name_binds.lu", "food\ndrink\nnone\n");
}

#[test]
fn a_bare_variant_as_a_struct_fields_pattern_tests_the_field() {
    four_machines(
        "grammar/match_nested_struct_field_variant.lu",
        "other 1\ndrink\nother 3\ndrink 4\n",
    );
}

// s282: a capitalized bare name that spells a variant of an enum in scope,
// in a binder position, still binds a new name on every machine and warns
// W0801 at the name. The compiled tiers share the front end, so each reports
// the same `{code, span}` set; lupin answers the same verdict and output, and
// its W0801 spans must agree whenever its record carries W0801 (the pinned
// 0.1.59 runs no binder analysis, so its array omits the code).

fn spans_of(record: &serde_json::Value, code: &str) -> Vec<(u64, u64)> {
    let mut spans: Vec<(u64, u64)> = record["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|w| w["code"] == code)
        .map(|w| {
            (
                w["span"][0].as_u64().unwrap_or(u64::MAX),
                w["span"][1].as_u64().unwrap_or(u64::MAX),
            )
        })
        .collect();
    spans.sort();
    spans
}

fn record_of(out: &Output, row: &str, machine: &str) -> serde_json::Value {
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

fn lupin_binary() -> Option<PathBuf> {
    let lupin = std::env::var_os("LUPIN").map(PathBuf::from).or_else(|| {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("../wolf-interp/target/release/lupin");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    });
    if lupin.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "required lupin missing"
        );
    }
    lupin
}

/// The compiled tiers' records for `row`, or `None` for a tier the host
/// cannot run (a SKIP, refused when `WOLF_TEST_NATIVE_REQUIRE` is set).
fn compiled_records(row: &str) -> Vec<(&'static str, serde_json::Value)> {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(row);
    let mut records = Vec::new();
    for machine in ["--checked", "--native", "--release"] {
        let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
            .args(["conform-run", "--json", machine])
            .arg(&entry)
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
            continue;
        }
        records.push((machine, record_of(&out, row, machine)));
    }
    records
}

/// Every machine binds and warns W0801 at exactly `spans`. `lupin_runs` is
/// false for a shape the pinned lupin still declines (named at the row).
fn binder_warns(row: &str, stdout: &str, spans: &[(u64, u64)], lupin_runs: bool) {
    for (machine, record) in compiled_records(row) {
        assert_eq!(record["verdict"], "exit(0)", "{machine} {row}: {record}");
        assert_eq!(record["stdout_inline"], stdout, "{machine} {row}: {record}");
        assert_eq!(
            spans_of(&record, "W0801"),
            spans,
            "{machine} {row}: W0801 spans: {record}"
        );
    }
    if !lupin_runs {
        return;
    }
    let Some(lupin) = lupin_binary() else {
        eprintln!("SKIP: lupin {row}: set LUPIN to run the fourth machine");
        return;
    };
    let entry = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(row);
    let out = Command::new(lupin)
        .args(["conform-run", "--json"])
        .arg(entry)
        .output()
        .expect("run lupin");
    let record = record_of(&out, row, "lupin");
    assert_eq!(record["verdict"], "exit(0)", "lupin {row}: {record}");
    assert_eq!(record["stdout_inline"], stdout, "lupin {row}: {record}");
    let lupin_spans = spans_of(&record, "W0801");
    if !lupin_spans.is_empty() {
        assert_eq!(lupin_spans, spans, "lupin {row}: W0801 spans: {record}");
    }
}

#[test]
fn let_and_var_binders_that_spell_a_variant_warn() {
    binder_warns(
        "lints/binder_variant_let.lu",
        "food 3\ntip 4\ndrink\ntip 2\ndrink 1 2\nfood 5 6\n7 8\n",
        &[
            (971, 976),
            (1067, 1071),
            (1169, 1177),
            (1246, 1251),
            (1354, 1358),
            (1442, 1450),
            (1546, 1550),
        ],
        true,
    );
}

#[test]
fn a_for_header_that_spells_a_variant_warns() {
    binder_warns(
        "lints/binder_variant_for.lu",
        "food1tip2\ndrinktip\n",
        &[(703, 708), (787, 791)],
        true,
    );
}

/// lupin 0.1.59 reads the handler's name as a variant test and refuses at
/// run ("the `else |pat|` pattern did not match the error value"); the
/// compiled tiers bind it, which is the rule. The fourth machine joins this
/// row when lupin's mirror ships.
#[test]
fn an_else_handler_name_that_spells_a_variant_warns() {
    binder_warns(
        "lints/binder_variant_else.lu",
        "7 5\n",
        &[(561, 566), (596, 600)],
        false,
    );
}

#[test]
fn an_item_imported_enums_variant_warns_in_a_binder() {
    binder_warns(
        "lints/binder_variant_imported/main.lu",
        "tip 3\n",
        &[(405, 410)],
        true,
    );
}

#[test]
fn binders_and_parameters_that_spell_no_test_stay_silent() {
    binder_warns(
        "lints/binder_variant_controls.lu",
        "food 3\n5 3\n8\n5\n",
        &[],
        true,
    );
}

/// A `select` arm's bare name that spells an in-scope variant is refused by
/// name on the compiled tiers (E0806 at the name). The pinned lupin refuses
/// the same program at run (`unsupported`, never a divergence); its own
/// static E0806 must agree on the span when it reports one.
#[test]
fn a_select_arm_that_spells_a_variant_is_refused() {
    let row = "conc/select_arm_bare_variant.lu";
    for (machine, record) in compiled_records(row) {
        assert_eq!(record["verdict"], "fail(E0806)", "{machine} {row}: {record}");
        assert_eq!(
            record["diagnostics"][0]["span"],
            serde_json::json!([652, 657]),
            "{machine} {row}: {record}"
        );
    }
    let Some(lupin) = lupin_binary() else {
        eprintln!("SKIP: lupin {row}: set LUPIN to run the fourth machine");
        return;
    };
    let entry = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(row);
    let out = Command::new(lupin)
        .args(["conform-run", "--json"])
        .arg(entry)
        .output()
        .expect("run lupin");
    let record = record_of(&out, row, "lupin");
    if record["verdict"] == "unsupported" {
        return;
    }
    assert_eq!(record["verdict"], "fail(E0806)", "lupin {row}: {record}");
    assert_eq!(
        record["diagnostics"][0]["span"],
        serde_json::json!([652, 657]),
        "lupin {row}: {record}"
    );
}

#[test]
fn a_select_arm_that_binds_a_lowercase_name_runs() {
    four_machines("conc/select_arm_lowercase_binds.lu", "food\n");
}
