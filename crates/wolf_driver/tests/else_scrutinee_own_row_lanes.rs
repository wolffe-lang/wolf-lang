//! s191 (wolf-lang#492, ruling #18) — an `else` handles its
//! scrutinee's own row and nothing else (`[type.row.else]`). A `?` that
//! fires inside the scrutinee — in a call argument, a `Map` index, a
//! nested `else`'s scrutinee, a block — and a `return` of a row from
//! inside it leave the enclosing function past the `else`, on every
//! machine. The checked machine's `else` matched every error flow,
//! propagating or not, so it caught the inner `?`, bound the fallback,
//! and ran on: a silent wrong answer. Native, release and lupin 0.1.42
//! propagated.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every `phase: run` entry on the NATIVE
//! lane; the defect was the CHECKED lane's, which no corpus row can
//! see. Every case asserts that the lanes agree AND agree on the right
//! answer — the side effects (`after`, `defer`, `errdefer`, `caught`)
//! are the stdout.

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
             (LUPIN={}) — the oracle leg of #492's gate did not run",
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
        "checked verdict on {}",
        entry.display()
    );
    assert_eq!(
        checked.stdout,
        want,
        "the CHECKED lane on {} (wolf-lang#492: an `else` handles its scrutinee's own row only)",
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

/// The issue's first shape: `look(m, key(ok)?) else 0`. Red at trunk
/// e619d14c on the checked lane (`after 0`, answered 0).
#[test]
fn a_propagating_call_argument_leaves_past_the_else() {
    every_lane_says(
        &corpus("else_try_call_arg.lu"),
        "key\nlook a\nafter 5\n5\nkey\n9\n",
        &[],
    );
}

/// The issue's second shape: `m[key(ok)?] else 0`, bound and as a tail.
/// Red at trunk on the checked lane.
#[test]
fn a_propagating_map_index_leaves_past_the_else() {
    every_lane_says(
        &corpus("else_try_index.lu"),
        "let\nkey\nafter 5\n5\nkey\n9\ntail\nkey\n5\nkey\n9\n",
        &[],
    );
}

/// Three nested `else`s over one `?`; the empty-map call is the
/// control, each `else` handling its own `none` in turn. Red at trunk
/// on the checked lane.
#[test]
fn a_propagating_try_passes_every_nested_else() {
    every_lane_says(
        &corpus("else_try_nested_else.lu"),
        "key\nlook a\nname 5\nlook a\nafter 5\n5\nkey\n9\nkey\nlook a\nname 0\nlook z\n\
after 7\n7\n",
        &[],
    );
}

/// `else |e|` (matching on the tag) and `else |none|`: the handler is
/// typed over the scrutinee's row and never sees the inner `?`'s tag.
/// Red at trunk on the checked lane.
#[test]
fn a_handler_never_sees_an_inner_try_row() {
    every_lane_says(
        &corpus("else_try_handler.lu"),
        "bind\nkey\nlook a\nafter 5\n5\nkey\n9\nkey\nlook a\ncaught none\nafter -1\n-1\n\
path\nkey\n9\nkey\nlook a\nafter -2\n-2\n",
        &[],
    );
}

/// A `?` inside a block within the scrutinee: the block's `errdefer`
/// and `defer` run, LIFO, and the row leaves the function. Red at
/// trunk on the checked lane.
#[test]
fn a_propagating_try_in_a_block_scrutinee_leaves_past_the_else() {
    every_lane_says(
        &corpus("else_try_block.lu"),
        "key\nlook a\ndefer\nafter 5\n5\nkey\nerrdefer\ndefer\n9\n",
        &[],
    );
}

/// `return parse` inside a block scrutinee returns the row from the
/// function; `return 3` is the value twin. Red at trunk on the checked
/// lane (the row case).
#[test]
fn a_returned_row_in_the_scrutinee_leaves_past_the_else() {
    every_lane_says(
        &corpus("else_return_tag.lu"),
        "tag\nblk\nlook a\nafter 5\n5\nblk\n9\nvalue\nblk\nlook a\nafter 5\n5\nblk\n3\n",
        &[],
    );
}

/// The control: a scrutinee's own failure — a plain call, a call whose
/// `?` argument did not fire, a callee that propagated inside its own
/// body, a block whose tail fails, a binding handler — is handled on
/// every machine. Green at trunk.
#[test]
fn an_else_handles_its_scrutinee_own_failure() {
    every_lane_says(
        &corpus("else_own_failure.lu"),
        "plain\nlook a\n5\nlook zz\n0\ntry\nkey\nlook a\n0\ncallee\nlook zz\n0\n\
block\nblk\nlook zz\n0\nhandler\nlook zz\ncaught none\n-1\n",
        &[],
    );
}
