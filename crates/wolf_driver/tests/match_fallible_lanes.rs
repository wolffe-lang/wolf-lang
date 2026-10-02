//! s197 (wolf-lang#497, ruling #21) — `match` over a fallible value
//! (`[type.row.match]`). The scrutinee has type `T ! {row}`; an arm
//! naming a tag of the row is a row arm, any other pattern is a value
//! pattern over `T`, `_` covers both halves, and the match must cover
//! every tag and the whole of `T`. Before this lane every wolfgang
//! lane refused the form as `unsupported` and lupin 0.1.43 ran it —
//! taking the first bare-tag arm for a VALUE too (`match look(m, "a")
//! { none => -1, v => v }` answered -1 on a hit), which is why its
//! answers are pinned here by version as pre-mirror (is67 moves
//! lupin), never widened.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every `phase: run` entry on the NATIVE
//! lane; the checked machine's dispatch (`eval_match`) and the release
//! tier's lowering are what this gate watches. Every case asserts that
//! the lanes agree AND agree on the right answer.

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
             (LUPIN={}) — the oracle leg of #497's gate did not run",
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
/// `want` too, or — for a version named in `lupin_pre_mirror` — the
/// measured answer of a known lupin defect, pinned by version so a
/// newer lupin that still differs reds by name (s180's design).
fn every_lane_says(entry: &Path, want: &str, lupin_pre_mirror: &[(&str, &str)]) {
    let checked = lane(entry, "--checked").expect("the checked lane always runs");
    assert_eq!(
        checked.verdict,
        "exit(0)",
        "checked verdict on {} (wolf-lang#497: `match` over a fallible value)",
        entry.display()
    );
    assert_eq!(
        checked.stdout,
        want,
        "the CHECKED lane on {} (wolf-lang#497: `match` over a fallible value)",
        entry.display()
    );
    for flag in ["--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            continue;
        };
        assert_eq!(
            obs.verdict,
            "exit(0)",
            "{flag} verdict on {} (wolf-lang#497)",
            entry.display()
        );
        assert_eq!(obs.stdout, want, "the {flag} lane on {}", entry.display());
    }
    if let Some(lupin) = lupin_says(entry) {
        let pinned = lupin_pre_mirror
            .iter()
            .find(|(v, _)| *v == lupin.version)
            .map(|(_, s)| *s);
        assert_eq!(
            lupin.verdict,
            "exit(0)",
            "lupin verdict on {}",
            entry.display()
        );
        assert_eq!(
            lupin.stdout,
            pinned.unwrap_or(want),
            "lupin {}'s answer on {}",
            lupin.version,
            entry.display()
        );
    }
}

/// Every wolfgang lane refuses the row with `code`; lupin, when
/// present, refuses it with the same code, or — for a version named
/// in `lupin_pre_mirror` — answers its measured pre-mirror verdict.
fn every_lane_refuses(entry: &Path, code: &str, lupin_pre_mirror: &[(&str, &str)]) {
    let want = format!("fail({code})");
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            continue;
        };
        assert_eq!(
            obs.verdict,
            want,
            "the {flag} lane on {} (wolf-lang#497: `match` over a fallible value is \
             refused by name here, never run)",
            entry.display()
        );
    }
    if let Some(lupin) = lupin_says(entry) {
        let pinned = lupin_pre_mirror
            .iter()
            .find(|(v, _)| *v == lupin.version)
            .map(|(_, s)| *s);
        assert_eq!(
            lupin.verdict,
            pinned.unwrap_or(want.as_str()),
            "lupin {}'s verdict on {}",
            lupin.version,
            entry.display()
        );
    }
}

/// The issue's shape: a bare row arm and a value binding, over the
/// call and over a bound `!int`. Red at trunk cdde128a on every
/// wolfgang lane (`unsupported`). lupin 0.1.43 takes the `none` arm
/// for a value too.
#[test]
fn a_bare_tag_arm_and_a_value_binding() {
    every_lane_says(
        &corpus("match_row_bare_tag.lu"),
        "look zz\n-1\nlook a\n6\nlook zz\n-2\nlook a\n1\n",
        &[("0.1.43", "look zz\n-1\nlook a\n-1\nlook zz\n-2\nlook a\n-2\n")],
    );
}

/// Every arm kind: a payload tag, a bare tag, a literal, a guarded
/// binding, the closing binding. Red at trunk on every wolfgang lane.
#[test]
fn every_arm_kind_over_one_fallible_value() {
    every_lane_says(
        &corpus("match_row_payload_tag.lu"),
        "eof\nbad x\ntwo\nlong 4\nlen 1\n",
        &[("0.1.43", "eof\nbad x\neof\neof\neof\n")],
    );
}

/// `_` covers what is left on both halves: the rest of the row and
/// every value, the value half alone, the row alone after a binding.
/// Red at trunk on every wolfgang lane.
#[test]
fn a_wildcard_covers_what_is_left_on_each_half() {
    every_lane_says(
        &corpus("match_row_wild_each_half.lu"),
        "0: closed | closed | some error\n1: other | io | some error\n\
2: other | bad 2 | some error\n3: other | value | value 30\n",
        &[(
            "0.1.43",
            "0: closed | closed | value closed\n1: other | io | value io\n\
2: other | bad 2 | value Bad(2)\n3: closed | closed | value 30\n",
        )],
    );
}

/// An enum value half beside a row arm, and a `match` nested in a
/// value arm. Red at trunk on every wolfgang lane.
#[test]
fn an_enum_value_half_and_a_nested_match() {
    every_lane_says(
        &corpus("match_row_nested.lu"),
        "none dot line 3\nlook a\nlook b\n12\nlook a\nlook zz\n5\nlook zz\n-1\n",
        &[(
            "0.1.43",
            "none dot line 3\nlook a\n-1\nlook a\n-1\nlook zz\n-1\n",
        )],
    );
}

/// The #492 shape on a `match`: a `?` that fires inside the scrutinee
/// leaves the function past the arms; the arms handle `look`'s own
/// row only. Red at trunk on every wolfgang lane; the falsifier for
/// the checked machine's dispatch.
#[test]
fn a_propagating_try_in_the_scrutinee_leaves_past_the_arms() {
    every_lane_says(
        &corpus("match_row_try_scrutinee.lu"),
        "key\nlook a\nafter 5\n5\nkey\nlook a\nafter -1\n-1\nkey\n9\n",
        &[(
            "0.1.43",
            "key\nlook a\nafter -1\n-1\nkey\nlook a\nafter -1\n-1\nkey\n9\n",
        )],
    );
}

/// The control: `else |e| match e { … }` and `match (… else 7)` keep
/// working beside the direct form. Green at trunk on four machines.
#[test]
fn the_else_handler_match_keeps_working() {
    every_lane_says(
        &corpus("match_row_else_control.lu"),
        "look zz\n-1\nlook zz\nseven\nlook a\n5\n",
        &[],
    );
}

/// A missing tag is E0801 on every wolfgang lane (`unsupported` at
/// trunk). lupin 0.1.43 runs the program and exits 2.
#[test]
fn a_missing_tag_is_e0801() {
    every_lane_refuses(
        &corpus("negative/match_row_missing_tag.lu"),
        "E0801",
        &[("0.1.43", "exit(2)")],
    );
}

/// The uncovered value half is E0801 on every wolfgang lane. lupin
/// 0.1.43 runs the program and exits 255.
#[test]
fn an_uncovered_value_half_is_e0801() {
    every_lane_refuses(
        &corpus("negative/match_row_missing_value.lu"),
        "E0801",
        &[("0.1.43", "exit(255)")],
    );
}

/// A tag that is also a variant of `T` is E0816 on every wolfgang
/// lane, never guessed. lupin 0.1.43 runs the program and exits 1.
#[test]
fn a_tag_that_is_also_a_variant_is_e0816() {
    every_lane_refuses(
        &corpus("negative/match_row_tag_variant_collision.lu"),
        "E0816",
        &[("0.1.43", "exit(1)")],
    );
}
