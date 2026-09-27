//! eg00 (EGC, wolf-lang#446's campaign) — `[mem.model.place.elem]`'s
//! ten green witnesses, asserted on every machine.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs a `phase: run` entry on the NATIVE lane
//! only and checks a `fail(..)` entry's phase, so it cannot see the
//! checked machine's ANSWER, nor lupin's. Each case here asserts the
//! checked, native and release lanes give the row's verdict (and bytes)
//! and that lupin gives the clause's ruled verdict.
//!
//! The one row where lupin's answer is not the ruled one is
//! `elem_move_same_const_read.lu`: lupin 0.1.40 marks a moved element
//! but its index read never consults the mark and prints `1 1`
//! (wolffe-lang/wolf-interp#141). For the versions in
//! `PRE_MIRROR_LUPIN` that case asserts the MEASURED answer; any later
//! version must trap, so a pin bump that carries lupin forward without
//! the fix goes red here by name (s180's design).
//!
//! The one-place rows are the soundness bar the clause states: a
//! wolfgang lane that starts RUNNING one of them has made a run-time
//! index distinct without a proof rule — a regression, not progress.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// lupin releases that predate wolffe-lang/wolf-interp#141's fix.
const PRE_MIRROR_LUPIN: &[&str] = &["0.1.40"];

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    codes: Vec<String>,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        codes: rec["diagnostics"]
            .as_array()
            .map(|ds| {
                ds.iter()
                    .filter_map(|d| d["code"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// One wolfgang lane. `None` means the host cannot run a native lane
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

/// lupin's version and observation, or `None` when this box has no
/// sibling. `WOLF_PAIRING_REQUIRE_SIBLING` is the linux CI job saying
/// "one was arranged here" (r10/#253): there an absent sibling is a
/// failed fetch, and a gate that answers broken plumbing with a green
/// skip is not a gate.
fn lupin_says(entry: &Path) -> Option<(String, Obs)> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of [mem.model.place.elem]'s gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still compared against the RULED answer here; only the oracle \
             leg is absent"
        );
        return None;
    };
    let v = Command::new(&lupin)
        .arg("--version")
        .output()
        .expect("lupin --version runs");
    let version = String::from_utf8_lossy(&v.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string();
    assert!(!version.is_empty(), "lupin --version printed no version");
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    Some((version, parse_obs(&out.stdout, "lupin's observation")))
}

/// The corpus row itself is the program: one truth per file.
fn corpus(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/memory")
        .join(name);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// What one machine must answer: a verdict, and for a run the bytes.
#[derive(Clone, Copy)]
struct Want<'a> {
    verdict: &'a str,
    stdout: Option<&'a str>,
}

const fn runs(stdout: &str) -> Want<'_> {
    Want {
        verdict: "exit(0)",
        stdout: Some(stdout),
    }
}

const fn verdict(v: &str) -> Want<'_> {
    Want {
        verdict: v,
        stdout: None,
    }
}

fn check(machine: &str, entry: &Path, obs: &Obs, want: Want<'_>) {
    assert_eq!(
        obs.verdict,
        want.verdict,
        "the {machine} verdict on {} (diagnostics {:?})",
        entry.display(),
        obs.codes
    );
    if let Some(bytes) = want.stdout {
        assert_eq!(
            obs.stdout,
            bytes,
            "the {machine} answer on {}",
            entry.display()
        );
    }
}

/// Every wolfgang lane answers `wolfgang`; lupin answers `pre` at a
/// `PRE_MIRROR_LUPIN` version and `ruled` at any other.
fn every_lane(name: &str, wolfgang: Want<'_>, pre: Want<'_>, ruled: Want<'_>) {
    let entry = corpus(name);
    let checked = lane(&entry, "--checked").expect("the checked lane always runs");
    check("checked", &entry, &checked, wolfgang);
    for flag in ["--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            check(flag, &entry, &obs, wolfgang);
        }
    }
    let Some((version, lupin)) = lupin_says(&entry) else {
        return;
    };
    if PRE_MIRROR_LUPIN.contains(&version.as_str()) {
        check(
            &format!("lupin {version} (pre-mirror)"),
            &entry,
            &lupin,
            pre,
        );
    } else {
        check(&format!("lupin {version}"), &entry, &lupin, ruled);
    }
}

/// Item 1(d): `move t.0` leaves `t.1` readable — tuple positions are
/// field steps, distinct before the clause.
#[test]
fn a_tuple_position_moves_alone() {
    let want = runs("1 2\n");
    every_lane("elem_tuple_pos_move.lu", want, want, want);
}

/// Item 1(d), exclusivity: `bump2(mut t.0, mut t.1)` is two disjoint
/// claims.
#[test]
fn two_tuple_positions_go_mut_together() {
    let want = runs("2 12\n");
    every_lane("elem_tuple_pos_mut.lu", want, want, want);
}

/// Item 2: `move xs[i]` against a read of `xs[0]` — one place; lupin
/// sees `i = 1` and runs it (the conservatism direction).
#[test]
fn a_run_time_index_and_a_literal_are_one_place() {
    every_lane(
        "elem_dyn_move_const_read.lu",
        verdict("fail(E1001)"),
        runs("2 1\n"),
        runs("2 1\n"),
    );
}

/// Item 2: `add2(mut xs[i], mut xs[j])` — no rule proves `i != j`.
#[test]
fn two_run_time_indices_are_one_place() {
    every_lane(
        "elem_dyn_mut_pair.lu",
        verdict("fail(E1002)"),
        runs("2 12\n"),
        runs("2 12\n"),
    );
}

/// Item 2: the same literal twice is the same element on every
/// machine.
#[test]
fn the_same_literal_twice_is_one_element_everywhere() {
    let trap = verdict("trap(exclusivity)");
    every_lane(
        "elem_same_const_mut.lu",
        verdict("fail(E1002)"),
        trap,
        trap,
    );
}

/// Item 4, where R1 fails: `xs[i]` and `xs[k + 1]` over two locals —
/// and equal at run time, which lupin's trap shows.
#[test]
fn an_offset_through_another_local_stays_one_place() {
    let trap = verdict("trap(exclusivity)");
    every_lane(
        "elem_offset_other_local.lu",
        verdict("fail(E1002)"),
        trap,
        trap,
    );
}

/// Item 4, where R2 fails: a `0..3` loop index against `xs[0]`.
#[test]
fn a_loop_range_containing_the_literal_stays_one_place() {
    let trap = verdict("trap(exclusivity)");
    every_lane(
        "elem_loop_covers_const.lu",
        verdict("fail(E1002)"),
        trap,
        trap,
    );
}

/// Item 2: `Pool` handles are run-time values; lupin declines `Pool`.
#[test]
fn two_pool_handles_are_one_place() {
    let declines = verdict("unsupported");
    every_lane(
        "elem_pool_handles_one_place.lu",
        verdict("fail(E1002)"),
        declines,
        declines,
    );
}

/// The moved element itself stays unreadable: E1001 on wolfgang, and
/// `[mem.tier0.move.2]`'s trap on lupin once wolf-interp#141 is fixed.
#[test]
fn the_moved_element_itself_stays_unreadable() {
    every_lane(
        "elem_move_same_const_read.lu",
        verdict("fail(E1001)"),
        runs("1 1\n"),
        verdict("trap(use-after-move)"),
    );
}

/// Item 4's R3: out through `xs[i]`, back through `xs[i]`, read.
#[test]
fn a_store_through_the_same_index_revives_the_element() {
    let want = runs("1 3\n");
    every_lane("elem_same_index_revive.lu", want, want, want);
}
