//! s177 (wolf-lang#444) — `move <place>` empties that place, and the
//! checked lane, the native lane and lupin all say so.
//!
//! `[mem.tier0.move.2]`: use of a moved-from place is E1001 statically
//! and `trap(use-after-move)` dynamically — the `[conf.trap.map]`
//! pairing. wolfgang refuses at compile time, so the wolfgang verdict
//! is `fail(E1001)` where lupin's is the trap; that is agreement, not
//! divergence, and this file spells the mapping out so a future reader
//! does not "fix" one side into the other.
//!
//! Why this needs a test of its own rather than the corpus alone
//! (s171's lesson, wave-45): `cargo xtask corpus` runs every
//! `phase: run` entry on the NATIVE lane, and `lane-coverage` counts
//! which entries the checked lane EXECUTES, not what it answers. A
//! corpus witness therefore cannot see a checked-lane divergence at
//! all. The two corpus files pin the shape and feed the lupin pairing;
//! THIS file is what fails if one lane regresses alone.
//!
//! Every case asserts the lanes agree AND that they agree on the right
//! verdict — cross-lane equality alone passed for every one of these
//! programs at trunk 75e2aa8d, when all three lanes were wrong
//! together (#444: `exit(0)`, printing the moved value).

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
    if out.status.code() == Some(2) && flag == "--native" {
        eprintln!(
            "SKIP: environment cannot run the native lane: {}",
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
///
/// `WOLF_PAIRING_REQUIRE_SIBLING` is the linux CI job saying "one was
/// arranged here" (r10/#253): there, an absent sibling is the fetch
/// having failed, and a gate that answers broken plumbing with a green
/// skip is not a gate.
fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of #444's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still compared against the EXPECTED verdict here, so this file \
             keeps its teeth; only the oracle leg is absent"
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

fn program(case: &str, src: &str) -> PathBuf {
    // One program per directory: file boundaries create no scopes
    // (D32), so two `main`s in one directory are E0302.
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, src).expect("write");
    entry
}

/// The use after the `move` is refused by both wolfgang lanes with
/// E1001, and trapped by lupin — `[conf.trap.map]`'s two spellings of
/// one rule. `printed` is what lupin gets out before the trap, which
/// is also what every wolfgang lane printed at trunk while accepting
/// the program.
fn every_lane_refuses(entry: &Path, printed: &str) {
    let checked = lane(entry, "--checked").expect("the checked lane always runs");
    assert_eq!(
        checked.verdict,
        "fail(E1001)",
        "the CHECKED lane accepts a use after `move` on {} (#444)",
        entry.display()
    );
    if let Some(native) = lane(entry, "--native") {
        assert_eq!(
            native.verdict,
            "fail(E1001)",
            "the NATIVE lane accepts a use after `move` on {} (#444)",
            entry.display()
        );
        assert_eq!(
            checked.verdict,
            native.verdict,
            "the wolfgang lanes disagree on {}",
            entry.display()
        );
    }
    if let Some(lupin) = lupin_says(entry) {
        assert_eq!(
            lupin.verdict,
            "trap(use-after-move)",
            "lupin is the oracle for {} and did not trap",
            entry.display()
        );
        assert_eq!(
            lupin.stdout, printed,
            "lupin trapped at a different point in the program than expected"
        );
    }
}

/// The program runs, on every lane, with the same output. The
/// over-refusal guard: a fix for #444 that also refuses this one has
/// broken wolf-book ch07 §7.2.
fn every_lane_runs(entry: &Path, want: &str) {
    let checked = lane(entry, "--checked").expect("the checked lane always runs");
    assert_eq!(
        checked.verdict,
        "exit(0)",
        "the CHECKED lane refuses a legal program on {}",
        entry.display()
    );
    assert_eq!(
        checked.stdout,
        want,
        "checked stdout on {}",
        entry.display()
    );
    if let Some(native) = lane(entry, "--native") {
        assert_eq!(
            native.verdict,
            "exit(0)",
            "the NATIVE lane refuses a legal program on {}",
            entry.display()
        );
        assert_eq!(native.stdout, want, "native stdout on {}", entry.display());
    }
    if let Some(lupin) = lupin_says(entry) {
        assert_eq!(
            lupin.verdict,
            "exit(0)",
            "lupin refuses a legal program on {}",
            entry.display()
        );
        assert_eq!(lupin.stdout, want, "lupin stdout on {}", entry.display());
    }
}

const BOOK_HEAD: &str = "\
struct Meta { author: str, words: int }
struct Doc { title: str, meta: Meta }

fn main() -> !int {
    var d = Doc { title: \"regions\", meta: Meta { author: \"ada\", words: 900 } }
    let who = move d.meta.author
    print(\"{who} wrote it\")
    print(\"{d.title}, {d.meta.words} words\")
";

/// The maintainer's witness: the book's example plus one line that
/// reads the moved leaf. `corpus/memory/move_field_use_after.lu`.
#[test]
fn reading_a_moved_field_is_refused_on_every_lane() {
    let src = format!("{BOOK_HEAD}    print(\"{{d.meta.author}} wrote it\")\n    0\n}}\n");
    let entry = program("s177_move_field_use_after", &src);
    every_lane_refuses(&entry, "ada wrote it\nregions, 900 words\n");
}

/// The book's own example, untouched: reads only the siblings.
/// `corpus/memory/move_field_siblings_ok.lu`.
#[test]
fn moving_a_field_leaves_its_siblings_live_on_every_lane() {
    let src = format!("{BOOK_HEAD}    0\n}}\n");
    let entry = program("s177_move_field_siblings_ok", &src);
    every_lane_runs(&entry, "ada wrote it\nregions, 900 words\n");
}

/// The generalization #444's own text did not reach: the leaf is a
/// `str` and `str` is `Copy`, so the defect was never about the field
/// path. A depth-0 `Copy` binding has it too, and had it at trunk on
/// all three lanes.
#[test]
fn moving_a_copy_binding_empties_it_on_every_lane() {
    let entry = program(
        "s177_move_copy_binding",
        "fn main() -> !int {\n    \
         var s = \"ada\"\n    \
         let t = move s\n    \
         print(\"{t}\")\n    \
         print(\"{s}\")\n    \
         0\n}\n",
    );
    every_lane_refuses(&entry, "ada\n");
}

/// …and an `int`, the most Copy-shaped place there is. Nothing about
/// `move` consults the type.
#[test]
fn moving_an_int_binding_empties_it_on_every_lane() {
    let entry = program(
        "s177_move_int_binding",
        "fn main() -> !int {\n    \
         var n = 7\n    \
         let m = move n\n    \
         print(\"{m} {n}\")\n    \
         0\n}\n",
    );
    every_lane_refuses(&entry, "");
}

/// The control that was already right, and stays right: a non-`Copy`
/// field. If this ever fails with the three above passing, the fix has
/// been replaced by something that only special-cases `Copy`.
#[test]
fn moving_a_struct_field_is_still_refused_on_every_lane() {
    let entry = program(
        "s177_move_struct_field",
        "struct Meta { author: str, words: int }\n\
         struct Doc { title: str, meta: Meta }\n\n\
         fn main() -> !int {\n    \
         var d = Doc { title: \"regions\", meta: Meta { author: \"ada\", words: 900 } }\n    \
         let m = move d.meta\n    \
         print(\"{m.author}\")\n    \
         print(\"{d.meta.author} wrote it\")\n    \
         0\n}\n",
    );
    every_lane_refuses(&entry, "ada\n");
}
