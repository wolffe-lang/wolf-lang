//! std-module resolution through the driver (F-0001, issue #1):
//! `--std-root <dir>` (or the `WOLF_STD` environment variable) roots
//! `use std.X` at `<dir>/X/`; flag beats env; neither configured keeps
//! the prelude-stub `std`. Fixtures are self-contained temp trees —
//! never a sibling checkout.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// Build a small std tree + one program under a fresh subdirectory of
/// the target tmpdir: `<case>/std/fs/fs.lu`, `<case>/std/net/http/h.lu`
/// (the nested shape, wolf-std layout: the tree is the namespace), and
/// `<case>/pkg/main.lu` with the given source. Returns (std root,
/// entry file).
fn fixture(case: &str, main_src: &str) -> (PathBuf, PathBuf) {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&base);
    let std_root = base.join("std");
    std::fs::create_dir_all(std_root.join("fs")).unwrap();
    std::fs::create_dir_all(std_root.join("net/http")).unwrap();
    std::fs::write(
        std_root.join("fs/fs.lu"),
        "pub fn read_text(p: str) -> str { p }\n",
    )
    .unwrap();
    std::fs::write(
        std_root.join("net/http/h.lu"),
        "pub fn get(u: str) -> str { u }\n",
    )
    .unwrap();
    let pkg = base.join("pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    let entry = pkg.join("main.lu");
    std::fs::write(&entry, main_src).unwrap();
    (std_root, entry)
}

/// Run `wolf conform-run --phase=resolve` with the given extra args and
/// env, returning the observation record's (phase_reached, verdict).
fn resolve_verdict(entry: &Path, extra: &[&str], env: &[(&str, &str)]) -> (String, String) {
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--phase=resolve")
        .args(extra)
        .env_remove("WOLF_STD");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("wolf runs");
    let record: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout is one observation record");
    (
        record["phase_reached"].as_str().unwrap_or("").to_string(),
        record["verdict"].as_str().unwrap_or("").to_string(),
    )
}

const USE_FS: &str = "use std.fs\nfn main() -> !int {\n    fs.read_text(\"x\")\n    0\n}\n";
const USE_NESTED: &str = "use std.net.http\nfn main() -> !int {\n    http.get(\"x\")\n    0\n}\n";

#[test]
fn std_root_flag_resolves_std_modules() {
    let (std_root, entry) = fixture("flag_fs", USE_FS);
    let (phase, verdict) =
        resolve_verdict(&entry, &["--std-root", std_root.to_str().unwrap()], &[]);
    assert_eq!((phase.as_str(), verdict.as_str()), ("resolve", "pass"));
}

#[test]
fn std_root_flag_resolves_nested_dirs() {
    let (std_root, entry) = fixture("flag_nested", USE_NESTED);
    let eq_form = format!("--std-root={}", std_root.display());
    let (phase, verdict) = resolve_verdict(&entry, &[&eq_form], &[]);
    assert_eq!((phase.as_str(), verdict.as_str()), ("resolve", "pass"));
}

#[test]
fn wolf_std_env_is_the_fallback() {
    let (std_root, entry) = fixture("env_nested", USE_NESTED);
    let (phase, verdict) =
        resolve_verdict(&entry, &[], &[("WOLF_STD", std_root.to_str().unwrap())]);
    assert_eq!((phase.as_str(), verdict.as_str()), ("resolve", "pass"));
}

#[test]
fn flag_beats_env() {
    // A bogus WOLF_STD must not matter when the flag names a real
    // root: precedence is decided before validation.
    let (std_root, entry) = fixture("flag_beats_env", USE_NESTED);
    let (phase, verdict) = resolve_verdict(
        &entry,
        &["--std-root", std_root.to_str().unwrap()],
        &[("WOLF_STD", "/definitely/not/a/std/root")],
    );
    assert_eq!((phase.as_str(), verdict.as_str()), ("resolve", "pass"));
}

#[test]
fn without_std_root_the_stub_behavior_holds() {
    // The stub keeps answering `use std.fs` (its one module) and keeps
    // fencing everything else — exactly the pre-F-0001 behavior.
    let (_std_root, entry) = fixture("no_root_fs", USE_FS);
    let (phase, verdict) = resolve_verdict(&entry, &[], &[]);
    assert_eq!((phase.as_str(), verdict.as_str()), ("resolve", "pass"));

    let (_std_root, entry) = fixture("no_root_nested", USE_NESTED);
    let (phase, verdict) = resolve_verdict(&entry, &[], &[]);
    assert_eq!(phase, "resolve");
    assert_eq!(verdict, "fail(E0301)");
}

#[test]
fn bad_std_root_is_a_loud_error() {
    let (_std_root, entry) = fixture("bad_root", USE_FS);
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(&entry)
        .arg("--phase=resolve")
        .args(["--std-root", "/definitely/not/a/std/root"])
        .env_remove("WOLF_STD")
        .output()
        .expect("wolf runs");
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("std root"), "loud, named error: {err}");
}

/// The stderr of `wolf conform-run --phase=resolve` under the given
/// extra args and env — the diagnostics a reader actually sees.
fn resolve_stderr(entry: &Path, extra: &[&str], env: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--phase=resolve")
        .args(extra)
        .env_remove("WOLF_STD");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("wolf runs");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// #251: a packaged wolf ships no standard library, and nothing told
/// the reader one existed. The resolver knows `WOLF_STD` is unset at
/// exactly the moment the reader needs to hear it, so the miss says so.
#[test]
fn a_std_miss_without_a_root_names_wolf_std() {
    let (_std_root, entry) = fixture("no_root_names_mechanism", USE_NESTED);
    let err = resolve_stderr(&entry, &[], &[]);
    assert!(err.contains("E0301"), "the miss is still reported:\n{err}");
    assert!(
        err.contains("WOLF_STD") && err.contains("--std-root") && err.contains("wolf-std"),
        "the note names the mechanism and the library:\n{err}"
    );
}

/// …and only then. With a root configured the sentence would be false,
/// and a reader who already pointed wolf at a std tree does not need it.
#[test]
fn a_std_miss_with_a_root_does_not_name_wolf_std() {
    // A real std root that simply has no `std.list` in it.
    let (std_root, entry) = fixture(
        "root_set_missing_module",
        "use std.list\nfn main() -> !int {\n    0\n}\n",
    );
    let err = resolve_stderr(&entry, &["--std-root", std_root.to_str().unwrap()], &[]);
    assert!(err.contains("E0301"), "the miss is still reported:\n{err}");
    assert!(
        !err.contains("WOLF_STD"),
        "no root-is-unset note when a root IS set:\n{err}"
    );
}
