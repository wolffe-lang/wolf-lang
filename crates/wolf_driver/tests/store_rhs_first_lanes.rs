//! s182 (B123, s173's proposal 1, ruled B on 2026-09-24) — a store
//! evaluates its right-hand side FIRST, then mints its place
//! (`[mem.model.place.rhs]`), and every container's place lowering
//! owes the re-mint.
//!
//! Why a driver test beside the three corpus rows (s171's lesson,
//! wave 45): `cargo xtask corpus` runs every `phase: run` entry on the
//! NATIVE lane and `lane-coverage` counts which entries the checked
//! lane EXECUTES, not what it answers. The defect this clause exists
//! for was a silent CROSS-LANE wrong answer — under mint-once s173's
//! pool witness printed `1 65` on native against `7 65` on checked
//! with no diagnostic on either (wolf-lang#442 §4) — which no corpus
//! row can see. Every case here asserts the lanes agree AND agree on
//! the right answer.
//!
//! The pool case has two wolfgang lanes and a lupin that declines
//! `Pool` by name; the assertion on lupin there is that it says
//! `unsupported`, not a row and not a different number.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug)]
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
    if out.status.code() == Some(2) && flag != "--checked" {
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
             (LUPIN={}) — the oracle leg of [mem.model.place.rhs]'s gate did not run",
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
        .join("../../corpus/memory")
        .join(name);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// Every wolfgang lane runs and prints `want`; lupin, when present,
/// either does the same or (for `lupin_declines`) says `unsupported`.
fn every_lane_says(entry: &Path, want: &str, lupin_declines: bool) {
    let checked = lane(entry, "--checked").expect("the checked lane always runs");
    assert_eq!(checked.verdict, "exit(0)", "checked verdict on {}", entry.display());
    assert_eq!(checked.stdout, want, "the CHECKED lane's answer on {}", entry.display());
    for flag in ["--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            continue;
        };
        assert_eq!(obs.verdict, "exit(0)", "{flag} verdict on {}", entry.display());
        assert_eq!(obs.stdout, want, "the {flag} lane's answer on {}", entry.display());
    }
    if let Some(lupin) = lupin_says(entry) {
        if lupin_declines {
            assert_eq!(
                lupin.verdict,
                "unsupported",
                "lupin was expected to decline {} by name and answered otherwise — \
                 if it now serves the construct, drop the decline here and assert the answer",
                entry.display()
            );
        } else {
            assert_eq!(lupin.verdict, "exit(0)", "lupin verdict on {}", entry.display());
            assert_eq!(lupin.stdout, want, "lupin's answer on {}", entry.display());
        }
    }
}

/// `xs[0] = grow(mut xs)` and `cs[0].n = grow_cells(mut cs)`: the
/// right-hand side pushes 64 elements, moving the buffer under the
/// place. `corpus/memory/store_rhs_first_list.lu`.
#[test]
fn a_list_store_lands_after_its_right_hand_side_grew_the_list() {
    every_lane_says(&corpus("store_rhs_first_list.lu"), "7 65\n5 65\n", false);
}

/// `m["a"] = grow(mut m)`: the right-hand side inserts 64 keys,
/// rehashing under the place. `corpus/memory/store_rhs_first_map.lu`.
#[test]
fn a_map_store_lands_after_its_right_hand_side_grew_the_map() {
    every_lane_says(&corpus("store_rhs_first_map.lu"), "7 65\n", false);
}

/// s173's witness: `p[a].value = grow(mut p)` reserves 64 slots on the
/// right, moving the payload buffer. This is the case that was seen
/// wrong under mint-once (`1 65` native v `7 65` checked).
/// `corpus/memory/store_rhs_first_pool.lu`. lupin 0.1.38 declines
/// `Pool` by name.
#[test]
fn a_pool_store_lands_after_its_right_hand_side_grew_the_pool() {
    every_lane_says(&corpus("store_rhs_first_pool.lu"), "7 65\n", true);
}
