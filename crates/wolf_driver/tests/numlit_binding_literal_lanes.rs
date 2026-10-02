//! s202 (wolf-lang#458) — a literal written as an unannotated binding's
//! value takes its type from its own context, never from a later use.
//! It has none (`[type.numlit.propagate]`: adoption does not cross a
//! binding), so it is an `i32` (`[type.numlit.default]`) and must fit.
//! A later use still decides the binding's VALUE type
//! (`[type.numlit.value]`: `let n = 0` then `take_int(n)` is an `int`).
//! Until s202 every wolfgang lane let a later `int` use type the literal
//! too and ran `922337203685477580` as 64-bit; lupin 0.1.43 trapped
//! (`trap(overflow)`, "outside `i32`, the binding's type"). E0415 at
//! compile time and lupin's overflow trap are one answer, as the
//! clause's note records for an unused binding.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` reads one lane; this gate asks checked, native,
//! release and lupin.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
    }
}

/// One wolfgang lane. `None` means the host cannot run the native lane
/// (the s59 skip pattern), never a silent pass.
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) && flag != "--checked" {
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

/// The sibling lupin, found exactly as `pairing.rs` finds it: `LUPIN`
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

/// lupin's observation, or `None` when this box has no sibling.
/// `WOLF_PAIRING_REQUIRE_SIBLING` is the linux CI job saying "one was
/// arranged here" (r10/#253): there an absent sibling is a failed
/// fetch, and a gate that answers broken plumbing with a green skip is
/// not a gate.
fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of wolf-lang#458's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still compared against the EXPECTED answer here; only the oracle \
             leg is absent"
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

const ISSUE: &str = "wolf-lang#458";

fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.exists(), "corpus row missing: {}", p.display());
    p
}

/// Every wolfgang lane refuses the row with E0415; lupin, when present,
/// traps it as an overflow of the binding's `i32`.
fn every_lane_refuses_the_literal(entry: &Path) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict,
            "fail(E0415)",
            "the {flag} verdict on {} ({ISSUE}: the literal takes `i32` at the binding)",
            entry.display()
        );
    }
    if let Some(lupin) = lupin_says(entry) {
        assert_eq!(
            lupin.verdict,
            "trap(overflow)",
            "lupin {}'s verdict on {}",
            lupin.version,
            entry.display()
        );
    }
}

/// Every lane, lupin included, runs the row and prints `want`.
fn every_lane_runs(entry: &Path, want: &str) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(obs.verdict, "exit(0)", "the {flag} verdict on {}", entry.display());
        assert_eq!(obs.stdout, want, "the {flag} stdout on {}", entry.display());
    }
    if let Some(lupin) = lupin_says(entry) {
        assert_eq!(lupin.verdict, "exit(0)", "lupin's verdict on {}", entry.display());
        assert_eq!(lupin.stdout, want, "lupin's stdout on {}", entry.display());
    }
}

/// The issue's witness. Red at trunk 12a56b22: every wolfgang lane
/// answers `exit(0)` `922337203685477580`.
#[test]
fn the_witness_literal_takes_i32_at_the_binding() {
    every_lane_refuses_the_literal(&corpus("typecheck/numlit_binding_literal.lu"));
}

/// `[type.numlit.value]`'s own `take_int(big)`. Red at trunk 12a56b22:
/// every wolfgang lane answers `exit(0)` `5000000001`.
#[test]
fn a_later_call_does_not_type_the_literal() {
    every_lane_refuses_the_literal(&corpus("typecheck/numlit_binding_literal_call.lu"));
}

/// The literal inside the initializer's arithmetic term. Red at trunk
/// 12a56b22: every wolfgang lane answers `exit(0)` `ran`.
#[test]
fn a_literal_in_the_initializer_term_takes_i32() {
    every_lane_refuses_the_literal(&corpus("typecheck/numlit_binding_literal_term.lu"));
}

/// The controls: a later use still types the binding; annotated,
/// argument-position and value-beside literals keep their context.
/// Green at trunk.
#[test]
fn a_later_use_still_types_the_binding() {
    every_lane_runs(
        &corpus("typecheck/numlit_binding_value_later_use.lu"),
        "1 60 3 922337203685477580 5000000001 12\n",
    );
}
