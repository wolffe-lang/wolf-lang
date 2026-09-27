//! s183 (wolf-lang#452, ruled by the maintainer 2026-09-26: **index
//! first, then value**) — in `xs[idx()] = val()` and `m[key()] =
//! val()` the place's operands are evaluated before the right-hand
//! side, left to right as `[mem.model.order]` reads, and the element is
//! located after it (`[mem.model.place.rhs]`). Native, release and
//! lupin already did this; the checked machine ran the right-hand side
//! first and moved.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every `phase: run` entry on the NATIVE
//! lane, which was right before the fix; the defect was the CHECKED
//! lane's order, which no corpus row can see. Every case here asserts
//! that the lanes agree AND agree on the ruled answer — the order of
//! the side effects is the stdout.

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
             (LUPIN={}) — the oracle leg of #452's gate did not run",
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

/// Every wolfgang lane prints `want`; lupin, when present, prints
/// `want` too, or — for a version named in `lupin_pre_mirror` — the
/// measured answer of a known lupin defect, pinned by version so a
/// newer lupin that still differs reds by name (s180's design).
fn every_lane_says(entry: &Path, want: &str, lupin_pre_mirror: &[(&str, &str)]) {
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
        "the CHECKED lane's order on {} (wolf-lang#452: index first, then value)",
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
            "the {flag} lane's order on {}",
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
        let expect = lupin_pre_mirror
            .iter()
            .find(|(v, _)| *v == lupin.version)
            .map(|(_, s)| *s)
            .unwrap_or(want);
        assert_eq!(
            lupin.stdout,
            expect,
            "lupin {}'s answer on {}",
            lupin.version,
            entry.display()
        );
    }
}

/// s182's control, verbatim: `xs[idx()] = val()` prints `idx val 7`.
/// Red at trunk a201c518 on the checked lane (`val idx 7`).
#[test]
fn a_list_store_evaluates_its_index_before_its_value() {
    every_lane_says(&corpus("ctl_store_order.lu"), "idx\nval\n7\n", &[]);
}

/// `m[key()] = val()`, then `m[key()] = grow(mut m)`: the key first,
/// and the insert lands in the rehashed map.
#[test]
fn a_map_store_evaluates_its_key_before_its_value() {
    every_lane_says(
        &corpus("ctl_store_order_map.lu"),
        "key\nval\n7 1\nkey\ngrow\n7 65\n",
        &[],
    );
}

/// A field after the index, a store through a `mut` parameter, and
/// `xs[x()] = grow(mut xs)`: index, then value, then the element of
/// the grown list.
#[test]
fn the_index_runs_first_under_a_field_a_mut_parameter_and_a_growing_value() {
    every_lane_says(
        &corpus("ctl_store_order_nested.lu"),
        "c\nval\n7\np\nval\n7\nx\ngrow\n7 65\n",
        &[],
    );
}

/// Two and three index levels: every operand once, outermost first,
/// then the value. lupin 0.1.40 evaluates each operand but the last
/// twice in a store (wolf-interp#145, filed by s183); 0.1.41 is pinned
/// with it because the issue was filed after is56, the lane cutting
/// 0.1.41, launched — a lupin that fixes it drops its row here.
#[test]
fn every_index_operand_runs_once_outermost_first_before_the_value() {
    const LUPIN_145: &str = "i\ni\nj\nval\n7\na\nb\na\nb\nc\nval\n9\n";
    every_lane_says(
        &corpus("ctl_store_order_nested_index.lu"),
        "i\nj\nval\n7\na\nb\nc\nval\n9\n",
        &[("0.1.40", LUPIN_145), ("0.1.41", LUPIN_145)],
    );
}

/// The index's VALUE is taken first: `ys[i] = bump(mut i)` stores at
/// the old `i`. Red at trunk on the checked lane (`0 5 1`).
#[test]
fn the_index_value_is_taken_before_the_value_changes_it() {
    every_lane_says(&corpus("ctl_store_order_captured.lu"), "5 0 1\n", &[]);
}

/// `xs[idx()] += val()`: index, value, then read-combine-write.
#[test]
fn a_compound_store_evaluates_its_index_before_its_value() {
    every_lane_says(&corpus("ctl_store_order_compound.lu"), "idx\nval\n8\n", &[]);
}

/// `p[idx()] = val()` through a raw pointer: the same order.
#[test]
fn a_raw_pointer_store_evaluates_its_index_before_its_value() {
    every_lane_says(&corpus("ctl_store_order_raw.lu"), "idx\nval\n9\n", &[]);
}
