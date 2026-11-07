//! s285 (wolf-lang#1058, `[gram.pat.lit]`): a literal pattern takes its
//! scrutinee's type, and one that type cannot hold is E0415 at the
//! literal on the checked machine, native and release. Before, native and
//! release wrapped the arm constant into the scrutinee's width and matched
//! a different number (`300` over a `u8` matched 44), while the checked
//! machine and lupin never matched, and no tier said anything.
//!
//! lupin is the fourth machine. The pinned lupin (0.1.59) has no E0415 for
//! a pattern and accepts a signed literal pattern; its mirror is the
//! wolf-interp change of the same name. Until the pin moves, lupin is held
//! to the same spans whenever its record carries the code, and otherwise to
//! the never-matching answer each row is written for, never the wrapped one.
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
/// `code` at the same spans, or (the pinned 0.1.59) answers `lupin_else`.
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

/// wolf-lang#1058's program, the first red: native and release answered
/// `exit(0)` (the wrapped arm), the checked machine and lupin `exit(2)`.
#[test]
fn an_arm_literal_outside_a_u8_is_e0415_on_every_tier() {
    refused(
        "typecheck/numlit_pattern_wraps.lu",
        "E0415",
        &[(537, 540)],
        &[
            "error[E0415]: `300` does not fit `u8`",
            "u8 holds 0..=255",
            "[gram.pat.lit]",
        ],
        "exit(2)",
    );
}

/// Every integer primitive's max+1, and `wrapping[u8]`, `wrapping[i8]`.
#[test]
fn every_primitives_max_plus_one_is_e0415() {
    refused(
        "typecheck/numlit_pattern_edges.lu",
        "E0415",
        &[
            (702, 705),
            (786, 791),
            (872, 882),
            (963, 982),
            (1063, 1082),
            (1161, 1164),
            (1245, 1250),
            (1331, 1341),
            (1422, 1442),
            (1525, 1545),
            (1643, 1646),
            (1744, 1747),
        ],
        &[
            "`128` does not fit `i8`",
            "`32768` does not fit `i16`",
            "`2147483648` does not fit `i32`",
            "`9223372036854775808` does not fit `i64`",
            "`9223372036854775808` does not fit `int`",
            "`256` does not fit `u8`",
            "`65536` does not fit `u16`",
            "`4294967296` does not fit `u32`",
            "`18446744073709551616` does not fit `u64`",
            "`18446744073709551616` does not fit `uint`",
            "i8 holds -128..=127",
            "uint holds 0..=18446744073709551615",
        ],
        "exit(0)",
    );
}

/// A tuple element, a payload, a struct field, an or-pattern's
/// alternative, after `@`, a statement `match`, hex and `_` spellings.
#[test]
fn a_literal_outside_its_type_is_e0415_in_every_position() {
    refused(
        "typecheck/numlit_pattern_positions.lu",
        "E0415",
        &[
            (714, 717),
            (806, 809),
            (898, 901),
            (999, 1002),
            (1088, 1091),
            (1203, 1206),
            (1301, 1306),
            (1392, 1398),
        ],
        &["`300` does not fit `u8`", "`65536` does not fit `u16`"],
        "exit(0)",
    );
}

/// Each range end is a literal pattern: a high end, the exclusive end one
/// past the maximum (whose note names `0..=255`), both ends, an `i8` range.
#[test]
fn a_range_end_outside_its_type_is_e0415() {
    refused(
        "typecheck/numlit_pattern_range.lu",
        "E0415",
        &[(749, 752), (842, 845), (928, 931), (934, 937), (1029, 1032)],
        &[
            "`300` does not fit `u8`",
            "`256` does not fit `u8`",
            "`200` does not fit `i8`",
            "0..=255",
        ],
        "exit(0)",
    );
}

/// A literal scrutinee and an unannotated binding: the shared literal type
/// defaults to `i32`, and the pattern must fit it.
#[test]
fn a_literal_scrutinee_holds_its_pattern_to_i32() {
    refused(
        "typecheck/numlit_pattern_default.lu",
        "E0415",
        &[(627, 637), (739, 749)],
        &["`3000000000` does not fit `i32`", "[type.numlit.default]"],
        "exit(0)",
    );
}

/// No literal pattern carries a sign: each signed literal is E0207 by
/// name, at the literal, in every position. lupin 0.1.59 runs the file
/// (and matches `-1` against an `i8` holding -1): `exit(2)`.
#[test]
fn a_signed_literal_pattern_is_refused_by_name() {
    refused(
        "grammar/match_pattern_signed_literal.lu",
        "E0207",
        &[(607, 609), (671, 675), (742, 744), (810, 812)],
        &[
            "error[E0207]: a literal pattern has no sign",
            "n if n == -1",
        ],
        "exit(2)",
    );
}

/// The in-range edge of every primitive matches on all four machines; the
/// checked machine missed every `u64` arm above `i64::MAX`, and native
/// refused the decimal ones.
#[test]
fn every_primitives_edge_literal_matches_on_every_tier() {
    let row = "rows/match_literal_pattern_edges.lu";
    let want = "i8 i16 i32 i64 int u8 u16 u32 u64 uint\n0 x 2p63 wrap nested\n";
    for (machine, record, _) in compiled(row) {
        assert_eq!(record["verdict"], "exit(0)", "{machine} {row}: {record}");
        assert_eq!(record["stdout_inline"], want, "{machine} {row}: {record}");
    }
    if let Some(record) = lupin(row) {
        assert_eq!(record["verdict"], "exit(0)", "lupin {row}: {record}");
        assert_eq!(record["stdout_inline"], want, "lupin {row}: {record}");
    }
}
