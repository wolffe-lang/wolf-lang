//! s182 (wolf-lang#446) pinned `move xs[0]` on a `List[int]` then a
//! read of `xs[1]` as a documented conservatism divergence: `fail(E1001)`
//! on both wolfgang lanes (every element was one place, `Proj::Opaque`)
//! and `exit(0)` `1 2` on lupin, which models elements one by one. The
//! file asserted BOTH sides so that a change on either would be loud.
//!
//! eg01 (EGC's EG1) is that change, by ruling: `[mem.model.place.elem]`
//! item 1(a) makes two integer-literal indices whose values differ
//! distinct places, and the row now runs `1 2` on every machine. The
//! soundness half — a run-time index stays one place, the moved element
//! itself stays unreadable — is `element_places_lanes.rs`'s.
//!
//! Beside it, the half of #438 that already holds everywhere: a `Copy`
//! element stored through an index is its own copy on all three
//! machines (`corpus/memory/index_store_copy_elem.lu`).

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

fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of #446's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still compared against the EXPECTED verdict here; only the oracle \
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

fn corpus(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/memory")
        .join(name);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// The former conservatism row: every lane runs it and prints `1 2`.
/// `corpus/memory/elem_move_one_place.lu`.
#[test]
fn moving_one_element_leaves_its_sibling_readable_on_every_lane() {
    let entry = corpus("elem_move_one_place.lu");
    let want = "1 2\n";
    let checked = lane(&entry, "--checked").expect("the checked lane always runs");
    assert_eq!(
        checked.verdict, "exit(0)",
        "the CHECKED lane refuses `move xs[0]` then `xs[1]` — `[mem.model.place.elem]` \
         1(a) (EG1) has regressed to one place per container"
    );
    assert_eq!(checked.stdout, want, "the CHECKED lane's answer");
    for flag in ["--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            assert_eq!(obs.verdict, "exit(0)", "the {flag} lane disagrees with checked");
            assert_eq!(obs.stdout, want, "the {flag} lane's answer");
        }
    }
    if let Some(lupin) = lupin_says(&entry) {
        assert_eq!(lupin.verdict, "exit(0)", "lupin's verdict");
        assert_eq!(lupin.stdout, want, "lupin's answer");
    }
}

/// The `Copy` half of #438, true everywhere today.
/// `corpus/memory/index_store_copy_elem.lu`.
#[test]
fn a_copy_element_stored_by_index_is_its_own_copy_on_every_lane() {
    let entry = corpus("index_store_copy_elem.lu");
    let want = "7 9 wolf wolf 3 4\n";
    let checked = lane(&entry, "--checked").expect("the checked lane always runs");
    assert_eq!(checked.verdict, "exit(0)", "checked verdict");
    assert_eq!(checked.stdout, want, "the CHECKED lane's answer");
    if let Some(native) = lane(&entry, "--native") {
        assert_eq!(native.verdict, "exit(0)", "native verdict");
        assert_eq!(native.stdout, want, "the NATIVE lane's answer");
    }
    if let Some(lupin) = lupin_says(&entry) {
        assert_eq!(lupin.verdict, "exit(0)", "lupin verdict");
        assert_eq!(lupin.stdout, want, "lupin's answer");
    }
}
