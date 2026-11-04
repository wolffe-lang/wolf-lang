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
