//! s184 (wolf-lang#464) — `[mem.tier0.mode.mut]`: a `mut` parameter is
//! initialized at every return of the callee. A path that may leave it,
//! or any place under it (a field, an element, a map value), moved-out
//! is E1001 on the checked, native and release lanes; a store back
//! before the return makes it legal again.
//!
//! Before s184 every refused row here compiled with only W1002 (a wrong
//! "never written") and native and release printed `2`: the caller's
//! place still named the buffer the callee moved out and grew — the
//! #460 aliasing, reached through a parameter. The checked machine
//! printed `2` on the whole and map shapes and trapped on the field and
//! element shapes. So each refused case asserts the verdict AND that
//! the only diagnostic is E1001 (W1002 stands down beside it).
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs a `phase: run` entry on the NATIVE lane
//! only and checks a `fail(..)` entry's phase, so it cannot see the
//! checked machine's answer, nor lupin's.
//!
//! lupin's column is wolf-interp's. Its dynamic reading of the sentence
//! is `[mem.tier0.move.2]`'s: the caller's read of a place the callee
//! left moved-out traps `use-after-move`. lupin 0.1.40 does that on
//! the field shape only; the whole and one-path shapes print the
//! original `1` (wolffe-lang/wolf-interp#146), the element shape is
//! wolffe-lang/wolf-interp#141 and the map shape #144. For the versions
//! pinned per case those cases assert the MEASURED answer; any later
//! version must trap, so a pin bump that carries lupin forward without
//! the mirror goes red here by name (s180's design).

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

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

/// Every wolfgang lane answers `wolfgang`; lupin answers `pre` at one of
/// `pre_versions` and `ruled` at any other.
fn every_lane_pinned(
    name: &str,
    pre_versions: &[&str],
    wolfgang: Want<'_>,
    pre: Want<'_>,
    ruled: Want<'_>,
) {
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
    if pre_versions.contains(&version.as_str()) {
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

/// lupin releases measured before the mirror of this clause
/// (wolffe-lang/wolf-interp#146): a whole-parameter move is not
/// seen by the caller.
const PRE_MIRROR_LUPIN: &[&str] = &["0.1.40"];

/// lupin releases whose index read ignores a moved element's mark
/// (wolffe-lang/wolf-interp#141).
const PRE_ELEM_TRAP_LUPIN: &[&str] = &["0.1.40"];

/// lupin releases whose `Map` read copies a non-`Copy` value out rather
/// than moving it (wolffe-lang/wolf-interp#144); 0.1.41 is is56's head
/// as eg01b measured it.
const PRE_MAP_MOVE_LUPIN: &[&str] = &["0.1.40", "0.1.41"];

const fn refused() -> Want<'static> {
    verdict("fail(E1001)")
}

/// A refused row's only diagnostic is E1001 on every wolfgang lane —
/// the wrong W1002 "never written" is gone beside it.
fn only_e1001(name: &str) {
    let entry = corpus(name);
    for flag in ["--checked", "--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            assert_eq!(
                obs.codes,
                vec!["E1001".to_string()],
                "the {flag} diagnostics on {}",
                entry.display()
            );
        }
    }
}

// ------------------------------------------------ the refused shapes --

/// The whole parameter moved out and never stored back — #464's row.
#[test]
fn a_whole_mut_parameter_left_moved_out_is_refused() {
    every_lane_pinned(
        "mut_param_moveout_whole.lu",
        PRE_MIRROR_LUPIN,
        refused(),
        runs("1\n"),
        verdict("trap(use-after-move)"),
    );
    only_e1001("mut_param_moveout_whole.lu");
}

/// A field of the parameter moved out and never stored back.
#[test]
fn a_field_of_a_mut_parameter_left_moved_out_is_refused() {
    let trap = verdict("trap(use-after-move)");
    every_lane_pinned(
        "mut_param_moveout_field.lu",
        PRE_MIRROR_LUPIN,
        refused(),
        trap,
        trap,
    );
    only_e1001("mut_param_moveout_field.lu");
}

/// An element of the parameter moved out and never stored back.
#[test]
fn an_element_of_a_mut_parameter_left_moved_out_is_refused() {
    every_lane_pinned(
        "mut_param_moveout_elem.lu",
        PRE_ELEM_TRAP_LUPIN,
        refused(),
        runs("1\n"),
        verdict("trap(use-after-move)"),
    );
    only_e1001("mut_param_moveout_elem.lu");
}

/// A map value of the parameter read out and never stored back.
#[test]
fn a_map_value_of_a_mut_parameter_left_read_out_is_refused() {
    every_lane_pinned(
        "mut_param_moveout_map.lu",
        PRE_MAP_MOVE_LUPIN,
        refused(),
        runs("1\n"),
        verdict("trap(use-after-move)"),
    );
    only_e1001("mut_param_moveout_map.lu");
}

/// Stored back on one path only: the other path returns it moved-out.
#[test]
fn a_mut_parameter_stored_back_on_one_path_only_is_refused() {
    every_lane_pinned(
        "mut_param_moveout_one_path.lu",
        PRE_MIRROR_LUPIN,
        refused(),
        runs("1\n"),
        verdict("trap(use-after-move)"),
    );
    only_e1001("mut_param_moveout_one_path.lu");
}

// ----------------------------------------- the store back revives it --

/// `xs = t` before the return.
#[test]
fn a_whole_mut_parameter_stored_back_runs() {
    let want = runs("2\n");
    every_lane_pinned("mut_param_restore_whole.lu", &[], want, want, want);
}

/// `s.tags = t` before the return.
#[test]
fn a_field_stored_back_runs() {
    let want = runs("2\n");
    every_lane_pinned("mut_param_restore_field.lu", &[], want, want, want);
}

/// `xs[0] = t` before the return (item 3: the same literal).
#[test]
fn an_element_stored_back_through_the_same_literal_runs() {
    let want = runs("2\n");
    every_lane_pinned("mut_param_restore_elem.lu", &[], want, want, want);
}

/// `m[k] = take v` before the return (R3 over the `str` key `k`).
#[test]
fn a_map_value_stored_back_through_the_same_key_runs() {
    let want = runs("2\n");
    every_lane_pinned("mut_param_restore_map.lu", &[], want, want, want);
}
