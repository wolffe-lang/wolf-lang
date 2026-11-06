//! The binder warning and the `select` refusal through the `wolf` binary
//! (`[gram.pat.binder]`): `wolf fix` applies the lowercase rename and
//! lists the qualified test as the alternative, a second run finds nothing,
//! `--deny-warnings` fails the build at W0801 while `#[allow(w0801)]` keeps
//! it, a `select` arm's variant is a static rejection, and `--explain`
//! carries both codes' new paragraphs.
//!
//! Everything stops before codegen (`--emit=wir`), so it runs on every host.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const BINDS: &str = "enum Kind { Drink, Food, Gratuity }\n\n\
    fn main() -> !int {\n    let (Drink, n) = (Kind.Food, 3)\n    \
    let s = match Drink {\n        Kind.Food => \"food\",\n        _ => \"other\",\n    }\n    \
    print(\"{s} {n}\")\n    0\n}\n";

const SELECTS: &str = "enum Kind { Drink, Food, Gratuity }\n\n\
    fn main() -> !int {\n    let ch = channel[Kind](1)\n    ch.send(Kind.Food)?\n    \
    var got = 0\n    select {\n        Drink from ch => { got = 1 },\n        \
    timeout(1.s) => { got = 2 },\n    }\n    got - 1\n}\n";

fn fixture(case: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("binder_variants")
        .join(case);
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
        .env_remove("WOLF_STD")
        .output()
        .expect("run wolf build");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `wolf fix` with extra flags: (exit code, stdout + stderr).
fn fix(dir: &Path, extra: &[&str]) -> (i32, String) {
    let out = Command::new(wolf())
        .arg("fix")
        .arg(dir.join("main.lu"))
        .args(extra)
        .env_remove("WOLF_STD")
        .output()
        .expect("run wolf fix");
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn the_warning_renders_with_both_fixes_and_the_build_succeeds() {
    let dir = fixture("render", BINDS);
    let (code, err) = build_wir(&dir, &[]);
    assert_eq!(code, 0, "a warning never fails a default build:\n{err}");
    assert!(
        err.contains("warning[W0801]: `Drink` binds a new name here"),
        "{err}"
    );
    assert!(err.contains("`Drink` is declared here, in `Kind`"), "{err}");
    assert!(err.contains("bind it under a lowercase name: `drink`"), "{err}");
    assert!(err.contains("test the variant instead: `Kind.Drink`"), "{err}");
}

#[test]
fn fix_applies_the_rename_and_lists_the_test_as_the_alternative() {
    let dir = fixture("fix", BINDS);
    let (code, out) = fix(&dir, &[]);
    assert_eq!(code, 1, "the dry run signals a pending fix:\n{out}");
    assert!(out.contains("W0801"), "{out}");
    assert!(
        out.contains("machine-applicable: alternative to the selected fix"),
        "the qualified test is listed, not applied:\n{out}"
    );
    let (code, out) = fix(&dir, &["--apply"]);
    assert_eq!(code, 0, "apply succeeds:\n{out}");
    let after = std::fs::read_to_string(dir.join("main.lu")).expect("read");
    assert!(after.contains("let (drink, n) = (Kind.Food, 3)"), "{after}");
    assert!(after.contains("match drink {"), "the use is renamed too:\n{after}");
    assert!(!after.contains("Drink,"), "{after}");
    let (code, out) = fix(&dir, &[]);
    assert_eq!(code, 0, "nothing is left to fix:\n{out}");
    let (code, err) = build_wir(&dir, &["--deny-warnings"]);
    assert_eq!(code, 0, "the renamed program is warning-clean:\n{err}");
}

#[test]
fn deny_warnings_fails_at_the_binder_and_allow_keeps_it() {
    let dir = fixture("deny", BINDS);
    let (code, err) = build_wir(&dir, &["--deny-warnings"]);
    assert_eq!(code, 2, "a denied warning fails the build:\n{err}");
    assert!(err.contains("error[W0801]"), "promoted to an error:\n{err}");
    let allowed = BINDS.replacen("fn main", "#[allow(w0801)]\nfn main", 1);
    let dir = fixture("deny-allowed", &allowed);
    let (code, err) = build_wir(&dir, &["--deny-warnings"]);
    assert_eq!(code, 0, "the program's own allow survives the deny:\n{err}");
    assert!(!err.contains("W0801"), "{err}");
}

#[test]
fn a_select_arm_that_spells_a_variant_is_a_static_rejection() {
    let dir = fixture("select", SELECTS);
    let (code, err) = build_wir(&dir, &[]);
    assert_eq!(code, 2, "a static rejection:\n{err}");
    assert!(
        err.contains("error[E0806]: `Drink` is the variant `Kind.Drink`"),
        "{err}"
    );
    assert!(err.contains("a receive cannot fail to match"), "{err}");
    let (code, out) = fix(&dir, &["--apply"]);
    assert_eq!(code, 0, "the lowercase receive is the fix:\n{out}");
    let after = std::fs::read_to_string(dir.join("main.lu")).expect("read");
    assert!(after.contains("drink from ch =>"), "{after}");
    let (code, err) = build_wir(&dir, &[]);
    assert_eq!(code, 0, "the fixed program builds:\n{err}");
}

#[test]
fn explain_carries_the_binder_and_select_paragraphs() {
    let explain = |code: &str| {
        let out = Command::new(wolf())
            .args(["--explain", code])
            .output()
            .expect("run wolf --explain");
        assert!(out.status.success(), "{code}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let w = explain("W0801");
    assert!(w.contains("binder position"), "{w}");
    assert!(w.contains("`Kind.Drink`"), "{w}");
    let e = explain("E0806");
    assert!(e.contains("`select` arm"), "{e}");
}
