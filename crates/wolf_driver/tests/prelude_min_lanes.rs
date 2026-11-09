//! s289, the diagnostics sweep's third change (wolf-lang#708): the
//! prelude's `min` on the checked machine, native and release, and on the
//! pinned lupin as the fourth machine (the first change's shape,
//! `diagnostics_sweep_lanes.rs`).
//!
//! `[type.min]`: `min(a, b)` takes two values of one integer type and
//! answers the smaller, compared signed or unsigned as `<` compares that
//! type, each operand evaluated once, left to right. Until this change
//! every compiled tier declined `min` as "a prelude helper with no
//! signature yet". `[type.min.zip]`: the free `zip` has no meaning, so a
//! call is declined with that reason and a module's own `zip` draws no
//! W0304.
//!
//! lupin 0.1.60's `min` takes any number of integers of any widths, so the
//! arity, mixed-type and literal-fit rows run there. Until its mirror
//! narrows `min` to the clause, lupin is held to the code and spans
//! whenever its record carries the code, and otherwise to the answer the
//! pinned release gives today, which each test names.
mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn entry(row: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(row)
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/prelude_min")
        .join(name)
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

/// The compiled tiers' records and rendered stderr for the program at
/// `path`; a tier the host cannot run is a loud SKIP, refused when
/// `WOLF_TEST_NATIVE_REQUIRE` is set.
fn compiled_at(path: &Path, row: &str) -> Vec<(&'static str, serde_json::Value, String)> {
    let mut runs = Vec::new();
    for machine in ["--checked", "--native", "--release"] {
        let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
            .args(["conform-run", "--json", machine])
            .arg(path)
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

fn compiled(row: &str) -> Vec<(&'static str, serde_json::Value, String)> {
    compiled_at(&entry(row), row)
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

fn lupin_at(path: &Path, row: &str) -> Option<serde_json::Value> {
    let Some(lupin) = lupin_binary() else {
        eprintln!("SKIP: lupin {row}: set LUPIN to run the fourth machine");
        return None;
    };
    let out = Command::new(lupin)
        .args(["conform-run", "--json"])
        .arg(path)
        .output()
        .expect("run lupin");
    Some(record_of(&out, row, "lupin"))
}

fn lupin(row: &str) -> Option<serde_json::Value> {
    lupin_at(&entry(row), row)
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

/// The codes of every warning in `record`.
fn warnings(record: &serde_json::Value) -> Vec<String> {
    record["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["severity"] == "warning")
        .map(|d| d["code"].as_str().unwrap_or("?").to_string())
        .collect()
}

/// The words the compiler declined `min` with until this change, which no
/// row of this file may render.
const DECLINE: &str = "a prelude helper with no signature yet";

/// Every compiled tier runs `row` to exit 0 printing exactly `stdout`,
/// warning-clean, and so does lupin.
fn runs(row: &str, stdout: &str) {
    for (machine, record, stderr) in compiled(row) {
        assert_eq!(
            record["verdict"], "exit(0)",
            "{machine} {row}: {record}\n{stderr}"
        );
        assert_eq!(record["stdout_inline"], stdout, "{machine} {row}: {record}");
        assert_eq!(
            warnings(&record),
            Vec::<String>::new(),
            "{machine} {row}: {record}"
        );
        assert!(!stderr.contains(DECLINE), "{machine} {row}: {stderr}");
    }
    let Some(record) = lupin(row) else {
        return;
    };
    assert_eq!(record["verdict"], "exit(0)", "lupin {row}: {record}");
    assert_eq!(record["stdout_inline"], stdout, "lupin {row}: {record}");
}

/// Every compiled tier refuses `row` by `code` at exactly `span` and at
/// nothing else, its rendering says each of `has`, and none declines it.
/// lupin carries `code` at the same span, or (the pinned release) answers
/// `lupin_else`.
fn refused(row: &str, code: &str, span: (u64, u64), has: &[&str], lupin_else: &str) {
    let want = vec![(code.to_string(), span)];
    for (machine, record, stderr) in compiled(row) {
        assert_eq!(
            record["verdict"],
            format!("fail({code})"),
            "{machine} {row}: {record}\n{stderr}"
        );
        assert_eq!(errors(&record), want, "{machine} {row}: spans: {record}");
        for text in has {
            assert!(
                stderr.contains(text),
                "{machine} {row}: the rendering lacks {text:?}:\n{stderr}"
            );
        }
        assert!(!stderr.contains(DECLINE), "{machine} {row}: {stderr}");
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

// ------------------------------------------------------------ `[type.min]` --

/// Literal operands, equal operands, and `i64`'s two ends in both orders.
#[test]
fn min_answers_the_smaller_signed_value() {
    runs(
        "typecheck/min_signed.lu",
        "3 -5 7\n-9223372036854775808 -9223372036854775808 0\n",
    );
}

/// `u8` both ways, `u64`'s top value on either side, and `min` as a `u16`
/// function's tail: an unsigned type compares unsigned.
#[test]
fn min_compares_an_unsigned_type_unsigned() {
    runs("typecheck/min_unsigned.lu", "7 7\n1 1\n9\n");
}

/// Each operand once, left to right.
#[test]
fn min_evaluates_each_operand_once_left_to_right() {
    runs("typecheck/min_evaluates_once.lu", "tick 2\ntick 1\n1\n");
}

/// One operand: E0402 at the call. lupin 0.1.60 prints `5`.
#[test]
fn min_with_one_operand_is_e0402() {
    refused(
        "typecheck/min_one_argument.lu",
        "E0402",
        (246, 252),
        &["error[E0402]: `min` takes 2 arguments, but this call passes 1"],
        "exit(0)",
    );
}

/// Three operands: E0402 at the call. lupin 0.1.60 prints `1`.
#[test]
fn min_with_three_operands_is_e0402() {
    refused(
        "typecheck/min_three_arguments.lu",
        "E0402",
        (269, 281),
        &["error[E0402]: `min` takes 2 arguments, but this call passes 3"],
        "exit(0)",
    );
}

/// A `u8` and an `i64`: E0401 at the second operand. lupin 0.1.60 prints
/// `3`.
#[test]
fn min_over_two_integer_types_is_e0401() {
    refused(
        "typecheck/min_mixed_types.lu",
        "E0401",
        (361, 362),
        &["error[E0401]", "`u8`", "`i64`"],
        "exit(0)",
    );
}

/// A `str` operand: E0401 at the first operand, naming the integer family.
/// lupin 0.1.60 declines it.
#[test]
fn min_over_str_is_e0401() {
    refused(
        "typecheck/min_str.lu",
        "E0401",
        (314, 317),
        &["error[E0401]", "`min` takes integers", "`str`"],
        "unsupported",
    );
}

/// A literal beside a `u8` takes `u8`: `300` is E0415. lupin 0.1.60
/// prints `3`.
#[test]
fn a_literal_beside_a_u8_operand_must_fit_u8() {
    refused(
        "typecheck/min_literal_fit.lu",
        "E0415",
        (344, 347),
        &["error[E0415]", "`300` does not fit `u8`"],
        "exit(0)",
    );
}

/// The module's own `min` wins its name and keeps its W0304: the control.
#[test]
fn a_modules_own_min_keeps_its_w0304_and_runs() {
    let row = "lints/shadow_prelude_call.lu";
    for (machine, record, _) in compiled(row) {
        assert_eq!(record["verdict"], "exit(0)", "{machine} {row}: {record}");
    }
    let path = fixture("own_min/main.lu");
    for (machine, record, stderr) in compiled_at(&path, "own_min") {
        assert_eq!(record["verdict"], "exit(0)", "{machine} own_min: {record}");
        assert_eq!(
            record["stdout_inline"], "4\n",
            "{machine} own_min: {record}"
        );
        assert_eq!(
            warnings(&record),
            vec!["W0304".to_string()],
            "{machine} own_min: {stderr}"
        );
    }
}

// -------------------------------------------------------- `[type.min.zip]` --

/// The free `zip`, called: declined on every compiled tier with the
/// clause's reason, `xs.zip(ys)` named; lupin declines it too.
#[test]
fn the_free_zip_is_declined_with_its_reason() {
    let path = fixture("zip_call/main.lu");
    for (machine, record, stderr) in compiled_at(&path, "zip_call") {
        assert_eq!(
            record["verdict"], "unsupported",
            "{machine} zip_call: {record}"
        );
        let construct = record["x-unsupported-construct"].as_str().unwrap_or("");
        assert!(
            construct.contains("`zip`") && construct.contains("xs.zip(ys)"),
            "{machine} zip_call: {record}"
        );
        assert!(!stderr.contains(DECLINE), "{machine} zip_call: {stderr}");
    }
    if let Some(record) = lupin_at(&path, "zip_call") {
        assert_eq!(record["verdict"], "unsupported", "lupin zip_call: {record}");
    }
}

/// A module's own `zip` shadows nothing a program can use: no W0304.
#[test]
fn a_modules_own_zip_draws_no_w0304() {
    runs("lints/zip_own.lu", "12\n");
}
