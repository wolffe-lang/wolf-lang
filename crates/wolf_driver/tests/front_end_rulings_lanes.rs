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
