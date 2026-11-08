//! s287: the front-end rulings on the second compiler's spec silences,
//! each row on the checked machine, native and release, and on the pinned
//! lupin as the fourth machine (s285's shape, `literal_pattern_lanes.rs`).
//!
//! Every rule here is a front-end refusal, so one fix covers all three
//! compiled tiers. lupin's mirror is the wolf-interp change of the same
//! name; until the pin moves, lupin is held to the same spans whenever its
//! record carries the code, and otherwise to the answer the pinned release
//! gives today, which each test names.
mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn entry(row: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(row)
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

/// The compiled tiers' records and rendered stderr for `row`; a tier the
/// host cannot run is a loud SKIP, refused when `WOLF_TEST_NATIVE_REQUIRE`
/// is set.
fn compiled(row: &str) -> Vec<(&'static str, serde_json::Value, String)> {
    let mut runs = Vec::new();
    for machine in ["--checked", "--native", "--release"] {
        let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
            .args(["conform-run", "--json", machine])
            .arg(entry(row))
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
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        runs.push((machine, record_of(&out, row, machine), stderr));
    }
    runs
}

fn lupin(row: &str) -> Option<serde_json::Value> {
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
        return None;
    };
    let out = Command::new(lupin)
        .args(["conform-run", "--json"])
        .arg(entry(row))
        .output()
        .expect("run lupin");
    Some(record_of(&out, row, "lupin"))
}

/// The spans of every error diagnostic in `record`, in order, with its code.
fn errors(record: &serde_json::Value) -> Vec<(String, (u64, u64))> {
    let mut found: Vec<(String, (u64, u64))> = record["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["severity"] != "warning")
        .map(|d| {
            (
                d["code"].as_str().unwrap_or("?").to_string(),
                (
                    d["span"][0].as_u64().unwrap_or(u64::MAX),
                    d["span"][1].as_u64().unwrap_or(u64::MAX),
                ),
            )
        })
        .collect();
    found.sort_by_key(|(_, s)| *s);
    found
}

/// Every compiled tier refuses `row` by `code` at exactly `spans` and at
/// nothing else, and its rendering says each of `texts`. lupin carries
/// `code` at the same spans, or (the pinned release) answers `lupin_else`.
fn refused(row: &str, code: &str, spans: &[(u64, u64)], texts: &[&str], lupin_else: &str) {
    let want: Vec<(String, (u64, u64))> = spans.iter().map(|s| (code.to_string(), *s)).collect();
    for (machine, record, stderr) in compiled(row) {
        assert_eq!(
            record["verdict"],
            format!("fail({code})"),
            "{machine} {row}: {record}"
        );
        assert_eq!(errors(&record), want, "{machine} {row}: spans: {record}");
        for text in texts {
            assert!(
                stderr.contains(text),
                "{machine} {row}: the rendering lacks {text:?}:\n{stderr}"
            );
        }
    }
    let Some(record) = lupin(row) else {
        return;
    };
    if record["verdict"] == format!("fail({code})") {
        assert_eq!(errors(&record), want, "lupin {row}: spans: {record}");
    } else {
        assert_eq!(record["verdict"], lupin_else, "lupin {row}: {record}");
    }
}

// ------------------------------------------- wolf-lang#1064, alias cycles --

/// An alias that names itself is E0610 at the inner `E` with no row naming
/// it. The compiled tiers answered `exit(0)`; lupin already refused it.
#[test]
fn an_unused_alias_that_names_itself_is_e0610() {
    refused(
        "rows/negative/error_alias_cycle_unused.lu",
        "E0610",
        &[(441, 442)],
        &[
            "error[E0610]: the error-set alias `E` names itself",
            "[type.err.alias.cycle]",
        ],
        "fail(E0610)",
    );
}

/// Two aliases that name each other, unused: one E0610, where the loop
/// closes in declaration order (`A` inside `B`'s row).
#[test]
fn an_unused_two_alias_loop_is_one_e0610() {
    refused(
        "rows/negative/error_alias_cycle_pair_unused.lu",
        "E0610",
        &[(348, 349)],
        &["the loop is A -> B -> A"],
        "fail(E0610)",
    );
}

/// The rest of an unused alias's own row is checked at its declaration: a
/// tag spelled twice is E0601 at the second. lupin 0.1.60 has no rule for a
/// row's own repeated tag (wolf-lang#1065) and runs it.
#[test]
fn an_unused_alias_that_spells_a_tag_twice_is_e0601() {
    refused(
        "rows/negative/error_alias_own_dup_unused.lu",
        "E0601",
        &[(440, 441)],
        &["the tag `x` appears twice in this error row"],
        "exit(0)",
    );
}

/// The used loop keeps its one report, at its span (`A` inside `B`'s row).
#[test]
fn a_used_alias_loop_keeps_one_e0610() {
    refused(
        "rows/negative/error_alias_cycle.lu",
        "E0610",
        &[(434, 435)],
        &["the loop is A -> B -> A"],
        "fail(E0610)",
    );
}

/// Every compiled tier and lupin run `row` to `exit(want)`.
fn runs(row: &str, want: u8) {
    let verdict = format!("exit({want})");
    for (machine, record, _) in compiled(row) {
        assert_eq!(record["verdict"], verdict, "{machine} {row}: {record}");
    }
    if let Some(record) = lupin(row) {
        assert_eq!(record["verdict"], verdict, "lupin {row}: {record}");
    }
}

// ----------------------------- wolf-lang#1059, a nullary arm binds nothing --

/// The arm's `Done` tests `E.Done`, so `Done = 9` assigns the `let`: E0410
/// at the assignment, on every tier. Every machine wrote the `let`
/// (`exit(9)`); lupin 0.1.60 still does.
#[test]
fn assigning_a_let_through_a_nullary_arm_is_e0410() {
    refused(
        "typecheck/nullary_arm_let_assign.lu",
        "E0410",
        &[(688, 692)],
        &[
            "error[E0410]: `Done` is bound with `let`, so it cannot be assigned again",
            "this pattern tests `Done` and binds nothing",
            "[gram.pat.nullary.scope]",
        ],
        "exit(9)",
    );
}

/// The control: under `var Done` the arm writes the `var`.
#[test]
fn a_nullary_arm_writes_an_outer_var() {
    runs("typecheck/nullary_arm_var_assign.lu", 9);
}

/// A row tag tested in an arm binds nothing either: E0410 at `gone = 9`.
#[test]
fn assigning_a_let_through_a_row_tag_arm_is_e0410() {
    refused(
        "typecheck/nullary_arm_tag_assign.lu",
        "E0410",
        &[(623, 627)],
        &["`gone` is bound with `let`, so it cannot be assigned again"],
        "exit(9)",
    );
}

/// A field of the `let`, written through the arm: E0410 over the place.
#[test]
fn assigning_a_lets_field_through_a_nullary_arm_is_e0410() {
    refused(
        "typecheck/nullary_arm_field_assign.lu",
        "E0410",
        &[(485, 491)],
        &["`Done` is bound with `let`, so its fields cannot be assigned either"],
        "exit(9)",
    );
}

// ----------------- wolf-lang#1060, a variant or a field declared twice --

/// The second `A` of one enum is E0302 at its declaration. Every machine
/// accepted it (`exit(3)`); lupin 0.1.60 still does.
#[test]
fn a_variant_declared_twice_is_e0302() {
    refused(
        "resolve/enum_variant_twice.lu",
        "E0302",
        &[(348, 349)],
        &[
            "error[E0302]: the variant `A` is declared twice in this enum",
            "first declared here",
            "[gram.item.type.names]",
        ],
        "exit(3)",
    );
}

/// With two payloads, the second variant could never be built: E0302.
#[test]
fn a_variant_declared_twice_with_two_payloads_is_e0302() {
    refused(
        "resolve/enum_variant_twice_payload.lu",
        "E0302",
        &[(289, 290)],
        &["the variant `A` is declared twice in this enum"],
        "exit(5)",
    );
}

/// A struct field declared twice is E0302 at the second, used or not.
#[test]
fn a_struct_field_declared_twice_is_e0302() {
    refused(
        "resolve/struct_field_twice.lu",
        "E0302",
        &[(269, 270)],
        &["error[E0302]: the field `a` is declared twice in this struct"],
        "exit(0)",
    );
}

/// The general `type … = enum {…}` and `= struct {…}` spellings: two E0302s.
#[test]
fn the_general_type_spellings_refuse_a_member_declared_twice() {
    refused(
        "resolve/type_decl_member_twice.lu",
        "E0302",
        &[(316, 317), (355, 356)],
        &[
            "the variant `A` is declared twice in this enum",
            "the field `a` is declared twice in this struct",
        ],
        "exit(0)",
    );
}

/// Names compare as spelled: `a` and `A` are two variants.
#[test]
fn variant_names_differing_in_case_are_two_variants() {
    runs("resolve/enum_variant_case_distinct.lu", 2);
}

// ------------ wolf-lang#1062 and #1073, a name bound twice in one pattern --

/// `P { a: x, b: x }`: E0827 at the second `x`. The later binding won on
/// every machine (`exit(2)`); lupin 0.1.60 still keeps it.
#[test]
fn a_struct_pattern_that_binds_a_name_twice_is_e0827() {
    refused(
        "typecheck/pattern_name_twice_struct.lu",
        "E0827",
        &[(376, 377)],
        &[
            "error[E0827]: `x` is bound twice in this pattern",
            "first bound here",
            "[gram.pat.names]",
        ],
        "exit(2)",
    );
}

/// At two types, the same: E0827, not a type error.
#[test]
fn a_name_bound_twice_at_two_types_is_e0827() {
    refused(
        "typecheck/pattern_name_twice_types.lu",
        "E0827",
        &[(333, 334)],
        &["`x` is bound twice in this pattern"],
        "exit(2)",
    );
}

/// A binder pattern: `let (x, x)`.
#[test]
fn a_let_pattern_that_binds_a_name_twice_is_e0827() {
    refused(
        "typecheck/pattern_name_twice_let.lu",
        "E0827",
        &[(243, 244)],
        &["`x` is bound twice in this pattern"],
        "exit(2)",
    );
}

/// Both sides of `@`.
#[test]
fn a_name_on_both_sides_of_at_is_e0827() {
    refused(
        "typecheck/pattern_name_twice_at.lu",
        "E0827",
        &[(214, 215)],
        &["`x` is bound twice in this pattern"],
        "exit(5)",
    );
}

/// At depth: `(x, (y, x))`.
#[test]
fn a_nested_name_bound_twice_is_e0827() {
    refused(
        "typecheck/pattern_name_twice_nested.lu",
        "E0827",
        &[(246, 247)],
        &["`x` is bound twice in this pattern"],
        "exit(5)",
    );
}

/// A `for` header.
#[test]
fn a_for_header_that_binds_a_name_twice_is_e0827() {
    refused(
        "typecheck/pattern_name_twice_for.lu",
        "E0827",
        &[(251, 252)],
        &["`x` is bound twice in this pattern"],
        "exit(2)",
    );
}

/// wolf-lang#1073: a parameter declared twice.
#[test]
fn a_parameter_declared_twice_is_e0827() {
    refused(
        "typecheck/param_name_twice.lu",
        "E0827",
        &[(230, 231)],
        &["error[E0827]: `a` is bound twice in this parameter list"],
        "exit(2)",
    );
}

/// A closure's parameter declared twice.
#[test]
fn a_closure_parameter_declared_twice_is_e0827() {
    refused(
        "typecheck/param_name_twice_closure.lu",
        "E0827",
        &[(232, 233)],
        &["`a` is bound twice in this parameter list"],
        "exit(2)",
    );
}

/// A comma group is several patterns: the second `a` shadows the first.
#[test]
fn a_comma_group_rebinds_by_shadowing() {
    runs("typecheck/let_group_shadow.lu", 2);
}

/// Two tests of one variant bind nothing, so nothing is bound twice.
#[test]
fn two_tests_of_one_variant_are_not_a_name_bound_twice() {
    runs("typecheck/pattern_variant_tests_twice.lu", 4);
}
