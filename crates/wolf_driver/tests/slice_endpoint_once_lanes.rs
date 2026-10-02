//! s187 (wolf-lang#479) — a slice's endpoints run exactly once,
//! wherever the slice stands: bound by `let`, as the base of a member
//! read (`xs[a()..b()].len`), inside an interpolation hole, as a `read`
//! argument, as a method receiver, as a `for` iterable, nested under
//! another index or slice, over an indexed element, and over a `str`.
//! `[mem.model.order]` evaluates every operand once.
//!
//! The checked machine's place lookup evaluated a bracket's operands
//! before learning the index was a range, answered "not a place", and
//! every caller then evaluated the slice again: the endpoints ran two
//! or three times. Native, release and lupin 0.1.42 ran them once
//! (lupin 0.1.42 runs an indexed base's OUTER index twice or three
//! times, wolffe-lang/wolf-interp#157; it was pinned by version until
//! the 0.1.43 pairing, r25, and 0.1.43 runs it once).
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every `phase: run` entry on the NATIVE
//! lane, which was right before the fix; the defect was the CHECKED
//! lane's, which no corpus row can see. Every case asserts that the
//! lanes agree AND agree on the right answer — the side effects are
//! the stdout.

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
             (LUPIN={}) — the oracle leg of #479's gate did not run",
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
        "the CHECKED lane on {} (wolf-lang#479: each slice endpoint runs once)",
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
        assert_eq!(obs.stdout, want, "the {flag} lane on {}", entry.display());
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

/// is60's c14/c18/c19: `let s = xs[lo()..hi()]` (the control),
/// `let n = xs[lo()..hi()].len`, and `"{xs[lo()..hi()].len}"`. Red at
/// trunk c2401f05 on the checked lane (`len` twice, `hole` three times).
#[test]
fn a_member_read_of_a_slice_runs_its_endpoints_once() {
    every_lane_says(
        &corpus("ctl_slice_endpoints.lu"),
        "let\nlo\nhi\n2\nlen\nlo\nhi\n2\nhole\nlo\nhi\n2\n",
        &[],
    );
}

/// A slice as a `read` argument, as the receiver of `count` and
/// `is_empty`, and as a `for` iterable. Red at trunk on the checked
/// lane (twice in each position).
#[test]
fn a_slice_argument_receiver_or_iterable_runs_its_endpoints_once() {
    every_lane_says(
        &corpus("ctl_slice_endpoints_call.lu"),
        "arg\nlo\nhi\n50\ncount\nlo\nhi\n2\nis_empty\nlo\nhi\nfalse\nfor\nlo\nhi\n50\n",
        &[],
    );
}

/// `xs[lo()..hi()][0]` and `xs[lo()..hi()][0..1].len`. Red at trunk on
/// the checked lane (twice, three times).
#[test]
fn a_nested_slice_runs_its_inner_endpoints_once() {
    every_lane_says(
        &corpus("ctl_slice_endpoints_nested.lu"),
        "index\nlo\nhi\n20\nslice\nlo\nhi\n1\n",
        &[],
    );
}

/// `g[gi()][lo()..hi()]` bound, as a member base, and in a hole: the
/// index, then both endpoints, once each. Red at trunk on the checked
/// lane. lupin 0.1.42 ran `gi` twice, three times and twice (measured;
/// pinned until the 0.1.43 pairing, r25, which runs it once).
#[test]
fn a_slice_of_an_indexed_element_runs_the_index_and_endpoints_once() {
    every_lane_says(
        &corpus("ctl_slice_endpoints_indexed_base.lu"),
        "let\ngi\nlo\nhi\n2\nlen\ngi\nlo\nhi\n2\nhole\ngi\nlo\nhi\n2\n",
        &[],
    );
}

/// `s[lo()..hi()]` over a `str`, bound, as a member base, and in a
/// hole. Red at trunk on the checked lane.
#[test]
fn a_str_slice_runs_its_endpoints_once() {
    every_lane_says(
        &corpus("ctl_slice_endpoints_str.lu"),
        "let\nlo\nhi\n2\nlen\nlo\nhi\n2\nhole\nlo\nhi\n2\n",
        &[],
    );
}
