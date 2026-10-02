//! s189 (wolf-lang#481) — an index operand that leaves early runs
//! exactly once on every machine, and its flow leaves as it does from
//! any other operand: a propagating `?` in an index (bound, in a hole,
//! as an argument, as a member base, under every reader of a place), in
//! a slice's endpoint and in its base index, in either index of
//! `g[a()?][b()?]`, a `return`, `break` or `continue` inside an index
//! operand, an `else` in one, and a store target `xs[idx()?] = v()`.
//! `[mem.model.order]` evaluates every operand once;
//! `[mem.model.place.rhs]` runs a store's index before its value.
//!
//! The checked machine's place lookup evaluated the operand, dropped its
//! flow as "not a place", and every caller then evaluated the whole
//! expression again: the operand ran two, three or six times, a
//! `continue` re-ran it and then trapped on a receiver the fallback had
//! moved, and a store or `mut` argument was refused as "not a place".
//! Native, release and lupin 0.1.42 run each operand once (lupin 0.1.42
//! runs some receivers' operands twice, wolffe-lang/wolf-interp#157 and
//! #162; pinned by version until the 0.1.43 pairing, r25 — 0.1.43 runs
//! them once).
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every `phase: run` entry on the NATIVE
//! lane; the defect was the CHECKED lane's, which no corpus row can
//! see. Every case asserts that the lanes agree AND agree on the right
//! answer — the side effects are the stdout.

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
             (LUPIN={}) — the oracle leg of #481's gate did not run",
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
        "the CHECKED lane on {} (wolf-lang#481: each operand runs once, and its flow leaves)",
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

/// The issue's own shape, `let v = xs[idx()?]`, then the same index in
/// a hole, as a `read` argument and as a member base (`rows[idx()?].len`).
/// Red at trunk 87e105b1 on the checked lane (twice, then three times).
#[test]
fn a_propagating_index_operand_runs_once() {
    every_lane_says(
        &corpus("ctl_index_try_once.lu"),
        "let\nidx\n20\nidx\n9\nhole\nidx\n20\n0\nidx\n9\narg\nidx\n20\n0\nidx\n9\n\
member\nidx\n2\nidx\n9\n",
        &[],
    );
}

/// The same `?` under `copy`, a method receiver, a `for` iterable, an
/// element store's right-hand side, a byte view, a `str` member and a
/// `print` argument. Red at trunk on the checked lane (three times, six
/// under a byte view). lupin 0.1.42 pinned (wolf-interp#162).
#[test]
fn a_propagating_index_operand_runs_once_under_every_reader() {
    every_lane_says(
        &corpus("ctl_index_try_once_receivers.lu"),
        "copy\nidx\n9\ncount\nidx\n9\nfor\nidx\n9\nelemcopy\nidx\n5\nidx\n9\n\
push\nidx\n3\nidx\n9\nbytes\nidx\n9\nbytesfor\nidx\n9\nstrlen\nidx\n9\n\
print\nidx\ncde\n0\nidx\n9\n",
        &[],
    );
}

/// `xs[lo()?..hi()].len` (a control: s187 keeps a slice out of the place
/// lookup, so it was already once) and `g[gi()?][lo()?..hi()]` (red at
/// trunk on the checked lane: `gi` three times). lupin 0.1.42 pinned
/// (wolf-interp#157).
#[test]
fn a_propagating_slice_endpoint_or_base_index_runs_once() {
    every_lane_says(
        &corpus("ctl_slice_try_once.lu"),
        "endpoint\nlo\nhi\n2\nlo\n9\nbase\ngi\nlo\nhi\n2\ngi\n9\n",
        &[],
    );
}

/// `g[a()?][b()?]`: when `a` propagates, `b` never runs; when `b` does,
/// `a` does not run again. Red at trunk on the checked lane (`a` three
/// times; `a` and `b` twice).
#[test]
fn a_nested_index_runs_each_operand_once() {
    every_lane_says(
        &corpus("ctl_nested_index_try_once.lu"),
        "both\na\nb\n6\nouter\na\n9\ninner\na\nb\n9\n",
        &[],
    );
}

/// `return`, `break` and `continue` inside an index operand. Red at
/// trunk on the checked lane: twice each, and under `continue` the next
/// iteration trapped (use-after-move) on the list the fallback moved.
#[test]
fn a_return_break_or_continue_in_an_index_runs_once() {
    every_lane_says(
        &corpus("ctl_index_return_break_once.lu"),
        "return\nidx\n20\nidx\n9\nbreak\nidx\n10\nidx\ndone\ncontinue\nidx\n10\nidx\n\
idx\n30\ndone\n",
        &[],
    );
}

/// `xs[idx() else 0]` (a control: the row is handled inside the operand)
/// and `xs[idx() else { return 9 }]` (red at trunk on the checked lane:
/// twice).
#[test]
fn an_else_in_an_index_runs_once() {
    every_lane_says(
        &corpus("ctl_index_else_once.lu"),
        "value\nidx\n20\nidx\n10\nreturn\nidx\n20\nidx\n9\n",
        &[],
    );
}

/// A store target whose index propagates (`xs[idx()?] = v()`, `+=`,
/// `g[a()?][0] = v()`) and a `mut` argument over one: the index runs
/// once, the right-hand side never, nothing is stored. Red at trunk on
/// the checked lane (`unsupported`: "assignment through this place
/// shape").
#[test]
fn a_store_target_whose_index_propagates_runs_it_once() {
    every_lane_says(
        &corpus("ctl_store_index_try_once.lu"),
        "store\nidx\nv\n[10, 7, 30]\nidx\n9\n[10, 7, 30]\ncompound\nidx\n[10, 8, 30]\nidx\n9\n\
[10, 8, 30]\nnested\na\nv\n[[1, 2], [7, 6]]\na\n9\n[[1, 2], [7, 6]]\nmut\nidx\n\
[10, 9, 30]\nidx\n9\n[10, 9, 30]\n",
        &[],
    );
}
