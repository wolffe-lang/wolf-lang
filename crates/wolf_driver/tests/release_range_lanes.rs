//! wolf-lang#1205 and #1207: the release tier's interprocedural range
//! facts, held to the answers every other machine gives.
//!
//! The release tier's whole-program mid-end mints range facts on element
//! loads from the stores it can attribute to each list. Two holes let it
//! prove a range from the wrong stores:
//! - a value its provenance scan had no rule for (the list `m[k] else [0]`
//!   or `xs.get(i) else [0]` hands out through `eu.ok`, a `copy`'s result)
//!   stayed at the lattice's bottom, and a join skipped it, so the joined
//!   list was taken to be the fresh literal on the other edge;
//! - a list stored in a struct field or tuple element of a stack slot
//!   handed `mut` to a call was not seen as reaching the call, because only
//!   the slot's base was looked at.
//!
//! A release build then printed a stale or fallback element, dropped a
//! bounds trap, and read past a list's end. Each row's header is what the
//! checked machine, native and lupin answer; this gate holds every machine
//! to it, release included, and requires every compiled tier to run.
//! `release_native.rs` compares release with native over the run-phase
//! corpus; this gate also holds both to the header and to lupin, so a
//! fault the two native tiers share is caught too.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The corpus rows.
const ROWS: [&str; 6] = [
    "release_range_map_walk",
    "release_range_map_get",
    "release_range_join_literal",
    "release_range_read_past_end",
    "release_range_read_far",
    "release_range_slot_field",
];

const MACHINES: [&str; 3] = ["--checked", "--native", "--release"];

struct Obs {
    verdict: String,
    stdout: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    }
}

/// What `conform-run` says when the program it built died by a signal.
/// Before the fix, the far read's release binary did: that is the row's
/// answer on that machine, recorded beside the others so the gate names
/// every row; any other internal error still fails the gate.
const DIED: &str = "ICE: native binary died without an exit code";

/// One machine's observation of `entry`; `None` where the host cannot run
/// a compiled tier (a loud skip, refused under `WOLF_TEST_NATIVE_REQUIRE`).
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) && String::from_utf8_lossy(&out.stderr).contains(DIED) {
        return Some(Obs {
            verdict: "died without an exit code".to_string(),
            stdout: String::new(),
        });
    }
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) && flag != "--checked" {
        assert!(
            std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
            "required tier {flag} unavailable: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, "the observation"))
}

/// The sibling lupin, found as the other lane gates find it: `LUPIN`
/// first, then a `wolf-interp` checkout beside an ancestor.
fn sibling_lupin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LUPIN") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while dir.pop() {
        let candidate = dir.join("../wolf-interp/target/release/lupin");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn lupin(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found"
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN); the machines still answer the header"
        );
        return None;
    };
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    Some(parse_obs(&out.stdout, "lupin's observation"))
}

/// A row's header as the verdict and stdout every machine must answer:
/// `run(exit=0, stdout="…")` or `run(exit=trap(…))`.
fn expected(src: &str) -> (String, String) {
    let line = src
        .lines()
        .find(|l| l.starts_with("//! check: run(exit="))
        .expect("a run(exit=…) header");
    if let Some(body) = line.strip_prefix("//! check: run(exit=0, stdout=\"") {
        let body = body.trim_end_matches("\")");
        return (
            "exit(0)".to_string(),
            body.replace("\\n", "\n").replace("\\\"", "\""),
        );
    }
    let verdict = line
        .trim_start_matches("//! check: run(exit=")
        .strip_suffix(')')
        .expect("a closed run(exit=…) header");
    (verdict.to_string(), String::new())
}

fn corpus(row: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/memory")
        .join(format!("{row}.lu"))
}

#[test]
fn every_row_answers_its_header_on_every_machine() {
    let mut wrong = Vec::new();
    for row in ROWS {
        let entry = corpus(row);
        let src = std::fs::read_to_string(&entry).expect("the row exists");
        let (verdict, stdout) = expected(&src);
        for m in MACHINES {
            let Some(o) = lane(&entry, m) else {
                continue;
            };
            if o.verdict != verdict || o.stdout != stdout {
                wrong.push(format!(
                    "{row} {m}: {} {:?}, the header {} {:?}",
                    o.verdict, o.stdout, verdict, stdout
                ));
            }
        }
        if let Some(o) = lupin(&entry)
            && (o.verdict != verdict || o.stdout != stdout)
        {
            wrong.push(format!(
                "{row} lupin: {} {:?}, the header {} {:?}",
                o.verdict, o.stdout, verdict, stdout
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
