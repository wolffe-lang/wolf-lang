//! s171 (wolf-lang#400) — a call through a fn-typed parameter goes to
//! the parameter, on every lane.
//!
//! `[conf.resolve.ambient]` resolves a bare name in call position in
//! the order every other bare-name reference resolves in, innermost
//! lexical binding FIRST. The checked machine instead re-derived the
//! callee body from the spelled name — `decl_span` is `None` for a
//! fn-typed value, so the lookup fell through to a name-only net over
//! every module and found a top-level fn that happened to share the
//! parameter's name.
//!
//! Why this needs a test of its own rather than a corpus file: the
//! corpus gate (`cargo xtask corpus`) executes every `phase: run`
//! entry on the NATIVE lane, and native was always right. The
//! `lane-coverage` gate counts which entries the checked lane
//! executes, not what it answers. So the corpus witnesses document
//! the shape and feed the lupin pairing, and THIS file is what fails
//! if the checked machine regresses.
//!
//! Every case asserts the two lanes agree AND that they agree on the
//! right answer — a cross-lane equality alone would have passed if
//! both lanes were wrong together.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

struct Obs {
    verdict: String,
    stdout: String,
}

/// Run one entry file on one lane. `None` means the host cannot run
/// the native lane (the s59 skip pattern), never a silent pass.
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
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    })
}

fn case_dir(case: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// Both lanes agree, and agree on `want`.
fn both_lanes_say(entry: &Path, want: &str) {
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
        "the CHECKED lane called the wrong body on {} (#400)",
        entry.display()
    );
    let Some(native) = lane(entry, "--native") else {
        return;
    };
    assert_eq!(native.verdict, "exit(0)", "native verdict");
    assert_eq!(
        native.stdout,
        want,
        "the native lane disagrees with the expectation on {}",
        entry.display()
    );
    assert_eq!(
        checked.stdout,
        native.stdout,
        "the lanes disagree on {}",
        entry.display()
    );
}

/// The issue's own witness. `apply`'s parameter is named after the
/// module's `fn le`; the argument is `ge`. The second line is the
/// other half of the rule — the module's own `le` is still reachable
/// as a VALUE, so the fix is not "a parameter name hides the item".
#[test]
fn a_fn_typed_parameter_beats_a_top_level_fn_of_the_same_name() {
    let dir = case_dir("s171_same_module");
    let entry = dir.join("main.lu");
    std::fs::write(
        &entry,
        r#"
fn le(a: int, b: int) -> bool { a <= b }

fn ge(a: int, b: int) -> bool { a >= b }

fn apply(le: fn(int, int) -> bool, x: int, y: int) -> bool {
    le(x, y)
}

fn main() -> !int {
    print("{apply(ge, 1, 2)}")
    print("{apply(le, 1, 2)}")
    0
}
"#,
    )
    .expect("write fixture");
    both_lanes_say(&entry, "false\ntrue\n");
}

/// The shape sc50 hit through `std.list.sort_by`, and the reason a
/// caller cannot defend against the bug: the LIBRARY picks the
/// parameter's name. The old name-only net searched every module and
/// took the smallest body index, so the library called the CALLER's
/// unrelated function.
#[test]
fn a_fn_typed_parameter_beats_a_top_level_fn_across_a_module_boundary() {
    let dir = case_dir("s171_cross_module");
    std::fs::create_dir_all(dir.join("cmp")).expect("mkdir cmp");
    std::fs::write(
        dir.join("cmp/c.lu"),
        r#"//! member: true

pub fn apply(le: fn(int, int) -> bool, x: int, y: int) -> bool {
    le(x, y)
}

pub fn apply_swapped(le: fn(int, int) -> bool, x: int, y: int) -> bool {
    le(y, x)
}
"#,
    )
    .expect("write lib");
    let entry = dir.join("main.lu");
    std::fs::write(
        &entry,
        r#"
use cmp

fn le(a: int, b: int) -> bool { a <= b }

fn ge(a: int, b: int) -> bool { a >= b }

fn main() -> !int {
    print("{cmp.apply(ge, 1, 2)}")
    print("{cmp.apply_swapped(ge, 1, 2)}")
    0
}
"#,
    )
    .expect("write fixture");
    both_lanes_say(&entry, "false\ntrue\n");
}

/// The sharpest form: the top-level `le` takes ONE argument, so a
/// machine that dispatches to it is calling an arity-1 body with two
/// arguments. Before the fix the checked lane did exactly that and
/// still printed `exit(0)` — no arity check stood between the wrong
/// resolution and the wrong answer, which is why this class is worse
/// than a crash.
#[test]
fn the_parameter_wins_even_when_the_top_level_fn_has_another_arity() {
    let dir = case_dir("s171_arity");
    let entry = dir.join("main.lu");
    std::fs::write(
        &entry,
        r#"
fn le(a: int) -> bool { true }

fn ge(a: int, b: int) -> bool { a >= b }

fn apply(le: fn(int, int) -> bool, x: int, y: int) -> bool {
    le(x, y)
}

fn main() -> !int {
    print("{apply(ge, 1, 2)}")
    0
}
"#,
    )
    .expect("write fixture");
    both_lanes_say(&entry, "false\n");
}

/// The guard on the fix: a call to a DECLARED fn must still resolve to
/// the declaration even when a local of the same name is in scope but
/// holds no fn — and a local declared LATER must not capture a call
/// that precedes it. Without this, "consult the binding first" would
/// be a new way to call the wrong thing.
#[test]
fn a_declared_call_is_unaffected_by_a_later_local_of_the_same_name() {
    let dir = case_dir("s171_declared_still_wins");
    let entry = dir.join("main.lu");
    std::fs::write(
        &entry,
        r#"
fn tag() -> int { 7 }

fn main() -> !int {
    print("{tag()}")
    let tag = 99
    print("{tag}")
    0
}
"#,
    )
    .expect("write fixture");
    both_lanes_say(&entry, "7\n99\n");
}
