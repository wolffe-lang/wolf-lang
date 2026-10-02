//! s202 (wolf-lang#425) — the mid-end's loop versioner cloned a loop
//! that held a token ROOT. Inlining a callee that reads a lent
//! `List` puts the callee's `region.foreign` roots at the call site,
//! inside the caller's loop; versioning copied them, and `--release`
//! stopped on `wir verify error [region-root] ... has two token
//! roots`, an ICE, on boreutils `cut -n`'s walk. A loop holding a
//! root is no longer versioned. Before, such a loop never compiled at
//! all, so no program that built changes.
//!
//! The release lane is the one that runs the mid-end; the ICE is an
//! exit 2 that names `ICE`, which `lane_exit` turns into a failure,
//! never a skip (wolf-lang#471).

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
             (LUPIN={}) — the oracle leg of wolf-lang#425's gate did not run",
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

const ISSUE: &str = "wolf-lang#425";

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
/// `lupin_also` (an answer the clause allows besides the compiler's).
fn every_lane_says(entry: &Path, verdict: &str, stdout: &str, lupin_also: &[&str]) {
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

/// The reduction, one module. Red at trunk 12a56b22: `--release` is an
/// ICE (`[region-root]`).
#[test]
fn a_loop_holding_an_inlined_root_builds_on_release() {
    every_lane_says(
        &corpus("memory/versioned_loop_inlined_root.lu"),
        "exit(0)",
        "4\n",
        &[],
    );
}

/// The issue's shape, the ring in its own module. Red at trunk
/// 12a56b22: `--release` is an ICE (`[region-root]`).
#[test]
fn a_loop_holding_a_cross_module_root_builds_on_release() {
    every_lane_says(
        &corpus("memory/versioned_loop_cross_module/main.lu"),
        "exit(0)",
        "4\n",
        &[],
    );
}
