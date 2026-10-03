//! s207 (wolf-lang#541) — a row nobody consumes is discarded on every
//! machine. `[type.unit.discard]`: "A `!T` tail in a unit context is a
//! discard, warned (W0601), never a mismatch. The value's row is lost —
//! and, when T is not `()`, the value with it"; W0601 says the same of
//! a non-trailing `!T` statement. Every lane warned. Native, release
//! and lupin then discarded; the checked machine propagated the row (or
//! kept the value), so `main` left with `error: bad` where the others
//! printed the next line. The checked machine now discards at every
//! position sema records as a unit-context discard and at every
//! non-trailing `!T` statement. A `return boom()` in a unit fn — "the
//! operand of a `return` in one" — also stopped native and release with
//! an internal error; it lowers as the discard now.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every entry on the NATIVE lane only, and
//! native was right; the defect was on the checked lane.

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
    codes: Vec<String>,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        codes: rec["diagnostics"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|d| d["code"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
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
             (LUPIN={}) — the oracle leg of wolf-lang#541's gate did not run",
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

const ISSUE: &str = "wolf-lang#541";

/// The corpus row itself is the program: one truth per file.
fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.exists(), "corpus row missing: {}", p.display());
    p
}

/// A lupin answer measured on a release that predates a mirror, pinned
/// by version so a newer lupin that still differs reds by name (s180's
/// design). is68's build reports the same version as the archive, so a
/// pinned version may also give the ruled answer.
type Pin = (&'static str, &'static str, &'static str);

/// Every wolfgang lane answers `verdict` with `stdout` and warns W0601
/// when `warns`; lupin answers the same, or what `pins` measured for its
/// version.
fn every_lane_says(entry: &Path, verdict: &str, stdout: &str, warns: bool, pins: &[Pin]) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            (verdict, stdout),
            "the {flag} answer on {} ({ISSUE})",
            entry.display()
        );
        assert_eq!(
            obs.codes.iter().any(|c| c == "W0601"),
            warns,
            "the {flag} W0601 on {}: {:?}",
            entry.display(),
            obs.codes
        );
    }
    if let Some(lupin) = lupin_says(entry) {
        let got = (lupin.verdict.as_str(), lupin.stdout.as_str());
        if got == (verdict, stdout) {
            return;
        }
        assert!(
            pins.iter()
                .any(|(v, pv, ps)| *v == lupin.version && (*pv, *ps) == got),
            "lupin {}'s answer on {}: {:?}, want {:?} (pins {:?})",
            lupin.version,
            entry.display(),
            got,
            (verdict, stdout),
            pins
        );
    }
}

/// A raising statement, an `int ! {none}` statement, an else-less
/// `if`, an else-if chain, the tails of `for`, `while` and `loop`
/// bodies, a `match` arm in a loop body. Red at trunk dfcc2f13: checked
/// answers `exit(1)`, `error: bad`.
#[test]
fn a_discarded_row_in_main_is_discarded_on_every_lane() {
    every_lane_says(
        &corpus("rows/unit_discard_raise_stmts.lu"),
        "exit(0)",
        "a\nb\nc\nd\ne 3\nf 3\ng 3\nother\nh\n",
        true,
        &[],
    );
}

/// A unit fn's tail and its statement, a unit method's tail, a unit fn
/// with a `defer`. Red at trunk dfcc2f13: checked answers `exit(1)`.
#[test]
fn a_unit_fn_returns_unit_when_its_tail_raises() {
    every_lane_says(
        &corpus("rows/unit_discard_raise_unit_fn.lu"),
        "exit(0)",
        "in f\nafter f\nafter g\nafter go\ndefer ran\nafter h\n",
        true,
        &[],
    );
}

/// A unit `main` whose tail raises exits 0. Red at trunk dfcc2f13:
/// checked answers `exit(1)`, `hi\nerror: bad`, as lupin 0.1.44 does
/// (is68 mirrored it, wolf-interp#103).
#[test]
fn a_unit_main_whose_tail_raises_exits_zero() {
    every_lane_says(
        &corpus("rows/unit_discard_unit_main.lu"),
        "exit(0)",
        "hi\n",
        true,
        &[("0.1.44", "exit(1)", "hi\nerror: bad\n")],
    );
}

/// An else-less `if`'s value is `()` whether its tail succeeds or
/// raises. Red at trunk dfcc2f13: checked prints `3 none`. lupin 0.1.44
/// prints `3 none` and is68's `() none` (wolf-interp#179).
#[test]
fn an_else_less_if_is_unit_valued_whatever_its_tail() {
    every_lane_says(
        &corpus("rows/unit_discard_if_value.lu"),
        "exit(0)",
        "() ()\n",
        true,
        &[
            ("0.1.44", "exit(0)", "3 none\n"),
            ("0.1.44", "exit(0)", "() none\n"),
        ],
    );
}

/// An else-less `if` as a STATEMENT in a fallible fn: the row stays
/// inside `g`. Red at trunk dfcc2f13: checked prints `0` (the caller's
/// `else`).
#[test]
fn an_else_less_if_statement_in_a_fallible_fn_discards() {
    every_lane_says(
        &corpus("rows/unit_discard_fallible_body_stmt.lu"),
        "exit(0)",
        "after if\n7\n",
        true,
        &[],
    );
}

/// `return boom()` in a unit fn. Red at trunk dfcc2f13: native and
/// release stop with an internal error, checked answers `exit(1)`.
#[test]
fn a_unit_fns_return_operand_is_discarded() {
    every_lane_says(
        &corpus("rows/unit_discard_return_operand.lu"),
        "exit(0)",
        "after\n",
        true,
        &[],
    );
}

/// The control: `?` consumes the row, so it still leaves the loop body
/// and the fn on every lane. Green at trunk dfcc2f13, and it must stay
/// green.
#[test]
fn a_propagated_row_still_leaves() {
    every_lane_says(
        &corpus("rows/unit_discard_try_leaves.lu"),
        "exit(0)",
        "raised\n",
        false,
        &[],
    );
}
