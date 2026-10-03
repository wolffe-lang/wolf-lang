//! s202 (wolf-lang#393) — a `!T` bound without `?` when the row is
//! statically empty. `wir_ty` lowers such a `!T` as its plain ok value,
//! and `else` and `?` already honoured that; the interpolation hole
//! did not (it asked `eu.ok` of a plain i64 and native and release
//! PANICKED, `build.rs:1210`, in every release since 0.2.14), and
//! `match` refused the scrutinee as `unsupported`. The checked machine
//! and lupin always ran these programs. `[type.interp.union]`: the
//! hole renders the ok payload.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every `phase: run` entry on the NATIVE
//! lane only. This gate asks checked, native, release and lupin, and a
//! panic on any lane fails it (a non-zero exit with no record).

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
             (LUPIN={}) — the oracle leg of wolf-lang#393's gate did not run",
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

const ISSUE: &str = "wolf-lang#393";

/// The corpus row itself is the program: one truth per file.
fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.exists(), "corpus row missing: {}", p.display());
    p
}

/// Every wolfgang lane answers `verdict` with `stdout`; lupin, when
/// present, answers the same verdict with `stdout` or with one of
/// `lupin_also` (an answer the clause allows besides the compiler's) —
/// or, for a version named in `lupin_pre_mirror`, the measured verdict
/// of a known lupin defect, pinned by version so a newer lupin that
/// still differs reds by name (s180's design).
fn every_lane_says(
    entry: &Path,
    verdict: &str,
    stdout: &str,
    lupin_also: &[&str],
    lupin_pre_mirror: &[(&str, &str)],
) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict,
            verdict,
            "the {flag} verdict on {} ({ISSUE})",
            entry.display()
        );
        assert_eq!(
            obs.stdout,
            stdout,
            "the {flag} stdout on {} ({ISSUE})",
            entry.display()
        );
    }
    if let Some(lupin) = lupin_says(entry) {
        if let Some((_, measured)) = lupin_pre_mirror.iter().find(|(v, _)| *v == lupin.version) {
            assert_eq!(
                lupin.verdict,
                *measured,
                "lupin {} (pre-mirror) verdict on {}",
                lupin.version,
                entry.display()
            );
            return;
        }
        assert_eq!(
            lupin.verdict,
            verdict,
            "lupin {}'s verdict on {}",
            lupin.version,
            entry.display()
        );
        assert!(
            lupin.stdout == stdout || lupin_also.contains(&lupin.stdout.as_str()),
            "lupin {}'s stdout on {}: {:?}, want {:?} or one of {:?}",
            lupin.version,
            entry.display(),
            lupin.stdout,
            stdout,
            lupin_also
        );
    }
}

/// The issue's program, its `{ f() }` twin and a `-> int` caller.
/// Red at trunk 12a56b22: native and release panic (exit 101).
#[test]
fn a_bound_empty_row_value_renders_its_ok_half() {
    every_lane_says(
        &corpus("rows/eu_bind_empty_row.lu"),
        "exit(0)",
        "42\n42\n42\n",
        &[],
        &[],
    );
}

/// `match`, `else` and a wider return over the bound value. Red at
/// trunk 12a56b22: native and release answer `unsupported` on the
/// `match`. lupin 0.1.44 was pinned pre-mirror (r26, the 0.2.21
/// pairing; wolffe-lang/wolf-interp#176): its row-match reader took
/// `f`'s inferred `-> !int` row as open and refused the one value arm,
/// `fail(E0801)`, where `[gram]`'s inferred private row is empty and
/// every other machine — lupin 0.1.43 included — prints `43 42 42`.
/// The pin was dropped at the 0.1.45 pairing (r27): 0.1.45 carries
/// is69's inferred row and prints `43 42 42`.
#[test]
fn a_bound_empty_row_value_matches_elses_and_widens() {
    every_lane_says(
        &corpus("rows/eu_bind_empty_row_handled.lu"),
        "exit(0)",
        "43 42 42\n",
        &[],
        &[],
    );
}

/// The unit twin: `!()` with an empty row is valueless on native and
/// release, and the hole renders `()`. Red at trunk 12a56b22:
/// `unsupported` ("a valueless union in interpolation").
#[test]
fn a_bound_empty_row_unit_renders_unit() {
    every_lane_says(
        &corpus("rows/eu_bind_empty_row_unit.lu"),
        "exit(0)",
        "g\n()\n",
        &[],
        &[],
    );
}
