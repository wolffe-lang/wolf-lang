//! s67 acceptance — the warning system end-to-end through the `wolf`
//! binary: levels (`--deny-warnings`, `--allow`, per-family), the
//! `#[allow]` attribute, the `lints.*` manifest stub, the additive
//! `warnings` array in conform-run records, and `wolf fix`
//! (dry-run/apply/idempotent).
//!
//! Everything here stops before codegen (`--emit=wir`), so the tests
//! run on every host — no `cc`, no `libwolf_rt.a`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A body whose dead `match` arm fires the E0802 warning.
const WARNY: &str = "fn main() -> !int {\n    var x = 0\n    match x {\n        0 => { x = 1 }\n        _ => { x = 2 }\n        1 => { x = 3 }\n    }\n    if x > 0 { 0 } else { 1 }\n}\n";
/// The same body with the arm allowed at the item.
const ALLOWED: &str = "#[allow(e0802)]\nfn main() -> !int {\n    var x = 0\n    match x {\n        0 => { x = 1 }\n        _ => { x = 2 }\n        1 => { x = 3 }\n    }\n    if x > 0 { 0 } else { 1 }\n}\n";

fn fixture(case: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir fixture");
    std::fs::write(dir.join("main.lu"), src).expect("write main");
    dir
}

/// `wolf build --emit=wir` with extra flags: (exit code, stderr).
fn build_wir(dir: &Path, extra: &[&str]) -> (i32, String) {
    let out = Command::new(wolf())
        .arg("build")
        .arg(dir.join("main.lu"))
        .arg("-o")
        .arg(dir.join("out.wir"))
        .arg("--emit=wir")
        .arg("--no-cache")
        .args(extra)
        .output()
        .expect("run wolf build");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn warnings_report_but_do_not_fail_the_build() {
    let dir = fixture("warn-default", WARNY);
    let (code, err) = build_wir(&dir, &[]);
    assert_eq!(code, 0, "warnings never fail a default build:\n{err}");
    assert!(err.contains("warning[E0802]"), "warning renders:\n{err}");
}

#[test]
fn deny_warnings_promotes_and_fails() {
    let dir = fixture("warn-deny", WARNY);
    let (code, err) = build_wir(&dir, &["--deny-warnings"]);
    assert_eq!(code, 1, "denied warning fails the build:\n{err}");
    assert!(err.contains("error[E0802]"), "promoted to error:\n{err}");
    assert!(
        err.contains("promoted by the lint configuration"),
        "the note names the rule:\n{err}"
    );
}

#[test]
fn per_code_and_per_family_levels_layer_by_specificity() {
    let dir = fixture("warn-levels", WARNY);
    // allow silences.
    let (code, err) = build_wir(&dir, &["--allow", "E0802"]);
    assert_eq!(code, 0);
    assert!(!err.contains("E0802"), "allowed warning is silent:\n{err}");
    // family allow under --deny-warnings: specificity wins.
    let (code, err) = build_wir(&dir, &["--deny-warnings", "--allow", "E08xx"]);
    assert_eq!(code, 0, "family allow beats blanket deny:\n{err}");
    // unknown code on the flag is a CLI error.
    let (code, _) = build_wir(&dir, &["--deny", "W9999"]);
    assert_eq!(code, 2, "unregistered code on a flag exits 2");
}

#[test]
fn allow_attribute_is_item_granular_and_beats_deny() {
    let dir = fixture("warn-attr", ALLOWED);
    let (code, err) = build_wir(&dir, &["--deny-warnings"]);
    assert_eq!(code, 0, "source allow wins over CLI deny:\n{err}");
    assert!(!err.contains("E0802"), "suppressed entirely:\n{err}");
}

#[test]
fn manifest_lints_apply_and_cli_overrides() {
    let dir = fixture("warn-manifest", WARNY);
    std::fs::write(dir.join("wolf.pkg"), "lints.allow = E0802\n").expect("write manifest");
    let (code, err) = build_wir(&dir, &[]);
    assert_eq!(code, 0);
    assert!(!err.contains("E0802"), "manifest allow silences:\n{err}");
    let (code, err) = build_wir(&dir, &["--warn", "E0802"]);
    assert_eq!(code, 0);
    assert!(
        err.contains("warning[E0802]"),
        "CLI overrides manifest:\n{err}"
    );
    // A malformed manifest is an environment error, never ignored.
    std::fs::write(dir.join("wolf.pkg"), "lints.forbid = E0802\n").expect("rewrite manifest");
    let (code, _) = build_wir(&dir, &[]);
    assert_eq!(code, 2, "malformed lints table refuses loudly");
}

#[test]
fn conform_run_record_carries_the_warnings_array() {
    let dir = fixture("warn-record", WARNY);
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(dir.join("main.lu"))
        .arg("--json")
        .output()
        .expect("run conform-run");
    assert!(out.status.success());
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    let warns = rec["warnings"].as_array().expect("warnings array present");
    assert_eq!(warns.len(), 1);
    assert_eq!(warns[0]["code"], "E0802");
    assert!(warns[0]["span"].as_array().is_some_and(|s| s.len() == 2));

    // The allowed variant records an EMPTY array — the attribute is
    // part of the program, honored by the conformance surface.
    let dir = fixture("warn-record-allowed", ALLOWED);
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(dir.join("main.lu"))
        .arg("--json")
        .output()
        .expect("run conform-run");
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    assert_eq!(rec["warnings"].as_array().map(Vec::len), Some(0));
    assert_eq!(rec["diagnostics"].as_array().map(Vec::len), Some(0));
}

#[test]
fn fix_applies_machine_applicable_edits_idempotently() {
    let dir = fixture(
        "fix-cycle",
        "use nowhere_needed\n\nfn main() -> !int {\n    let s = \"abc\"\n    let b = s[-1]\n    0\n}\n",
    );
    std::fs::create_dir_all(dir.join("nowhere_needed")).expect("mkdir module");
    std::fs::write(
        dir.join("nowhere_needed/m.lu"),
        "//! member: true\n\npub fn f() -> int {\n    1\n}\n",
    )
    .expect("write module");
    let run_fix = |extra: &[&str]| {
        let out = Command::new(wolf())
            .arg("fix")
            .arg(dir.join("main.lu"))
            .args(extra)
            .output()
            .expect("run wolf fix");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    // Dry-run: reports pending fixes, exits 1, writes nothing.
    let before = std::fs::read_to_string(dir.join("main.lu")).expect("read");
    let (code, out, _) = run_fix(&[]);
    assert_eq!(code, 1, "dry-run signals pending fixes");
    assert!(out.contains("E0305"), "unused-import fix planned:\n{out}");
    assert!(out.contains("E0209"), "negative-index fix planned:\n{out}");
    assert_eq!(
        before,
        std::fs::read_to_string(dir.join("main.lu")).expect("read"),
        "dry-run writes nothing"
    );
    // Apply: both fixes land.
    let (code, _, err) = run_fix(&["--apply"]);
    assert_eq!(code, 0, "apply succeeds:\n{err}");
    let after = std::fs::read_to_string(dir.join("main.lu")).expect("read");
    assert!(!after.contains("use nowhere_needed"), "import deleted");
    assert!(after.contains("s[^1]"), "index rewritten to `^`:\n{after}");
    // Idempotent: nothing left to do.
    let (code, _, err) = run_fix(&[]);
    assert_eq!(code, 0, "second run is clean");
    assert!(err.contains("nothing to fix"), "{err}");
}
