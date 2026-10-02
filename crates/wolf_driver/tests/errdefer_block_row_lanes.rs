//! s196 (wolf-lang#499, ruling #20) — a block whose value is an error
//! has left on the error path, so its `errdefer` runs, as a function's
//! does at its tail (`[mem.model.order]`). Native and release ran the
//! block's `defer` and skipped its `errdefer`; the checked machine and
//! lupin 0.1.43 ran both. Measured at trunk cdde128a (kasumi,
//! `~/lanes/s196/evidence/red-trunk-cdde128a-rows.log`).
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every `phase: run` entry on the NATIVE
//! lane only, and the fix is in the WIR lowering both native and
//! release share — a row alone cannot show the release lane moved.
//! Every case asserts that the lanes agree AND agree on the right
//! answer; the `errdefer …` lines are the stdout.

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

/// lupin's observation, or `None` when this box has no sibling
/// (`WOLF_PAIRING_REQUIRE_SIBLING` turns an absent sibling into a
/// failure, r10/#253).
fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of #499's gate did not run",
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

/// The corpus row itself is the program: one truth per file.
fn corpus(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/rows")
        .join(name);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// Every wolfgang lane prints `want`; lupin, when present, prints
/// `want` too (lupin 0.1.43 already agrees; no pre-mirror pin).
fn every_lane_says(entry: &Path, want: &str) {
    let checked = lane(entry, "--checked").expect("the checked lane always runs");
    assert_eq!(
        checked.verdict,
        "exit(0)",
        "checked verdict on {}",
        entry.display()
    );
    assert_eq!(
        checked.stdout,
        want,
        "the CHECKED lane on {} (wolf-lang#499)",
        entry.display()
    );
    for flag in ["--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            continue;
        };
        assert_eq!(
            obs.verdict,
            "exit(0)",
            "{flag} verdict on {}",
            entry.display()
        );
        assert_eq!(
            obs.stdout,
            want,
            "the {flag} lane on {} (wolf-lang#499: a block whose value is an error runs its \
             errdefer)",
            entry.display()
        );
    }
    if let Some(lupin) = lupin_says(entry) {
        assert_eq!(
            lupin.verdict,
            "exit(0)",
            "lupin verdict on {}",
            entry.display()
        );
        assert_eq!(
            lupin.stdout,
            want,
            "lupin {}'s answer on {}",
            lupin.version,
            entry.display()
        );
    }
}

/// The issue's shape: `let b = { errdefer …; defer …; look(m, "zz") } else 0`,
/// with the function-tail twin in the same file. Red at trunk cdde128a
/// on native and release (`errdefer blk` missing).
#[test]
fn a_block_whose_value_is_a_row_runs_its_errdefer() {
    every_lane_says(
        &corpus("errdefer_block_row.lu"),
        "look zz\ndefer blk\nerrdefer blk\n0\nlook a\ndefer fn\n5\nlook zz\ndefer fn\nerrdefer fn\n9\n",
    );
}

/// Nested blocks: the inner block's row is the outer block's value, so
/// both scopes leave on the error path, inner first, each LIFO; and a
/// block as a function's tail runs its own errdefer before the
/// function's. Red at trunk on native and release (four lines missing).
#[test]
fn nested_blocks_each_run_their_errdefer() {
    every_lane_says(
        &corpus("errdefer_block_nested.lu"),
        "look a\ndefer inner\ndefer outer\n5\nlook zz\ndefer inner\nerrdefer inner\ndefer outer\n\
errdefer outer\n-1\nlook a\ndefer blk\ndefer fn\n5\nlook zz\ndefer blk\nerrdefer blk\ndefer fn\n\
errdefer fn\n9\n",
    );
}

/// A block inside a loop whose `else` breaks: the block's errdefer runs
/// on the failing turn before the handler's `break`. Red at trunk on
/// native and release (`errdefer blk zz` missing).
#[test]
fn a_block_in_a_loop_runs_its_errdefer_before_the_break() {
    every_lane_says(
        &corpus("errdefer_block_loop_break.lu"),
        "look a\ndefer blk a\ntotal 5\nlook b\ndefer blk b\ntotal 12\nlook zz\ndefer blk zz\n\
errdefer blk zz\ndone 12\n",
    );
}

/// The control: a function whose tail value is a row, and one that
/// returns a tag early. Green at trunk on all four machines.
#[test]
fn a_function_tail_runs_its_errdefer() {
    every_lane_says(
        &corpus("errdefer_fn_tail_control.lu"),
        "look a\ndefer fn\n5\nlook zz\ndefer fn\nerrdefer fn\n9\nearly\ndefer fn\nerrdefer fn\n9\n",
    );
}
