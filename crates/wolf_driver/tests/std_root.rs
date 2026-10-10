//! std-module resolution through the driver (F-0001, issue #1):
//! `--std-root <dir>` (or the `WOLF_STD` environment variable) roots
//! `use std.X` at `<dir>/X/`; flag beats env; neither configured keeps
//! the prelude-stub `std`. Fixtures are self-contained temp trees —
//! never a sibling checkout.

mod lane_exit;

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

// ---------------------------------------------------------------------
// Ruling #29 (wolf-lang#415, s204): the default std root is a `std`
// directory beside the running `wolf` binary — where the release archive
// stages the pinned wolf-std — and every configured source still
// overrides it, in this order: `--std-root`, `WOLF_STD`, a `wolf.pkg`
// `std` path dependency, the default. Each test below reaches one tree
// by a module only that tree has, so the answer names the root that won.

/// An "installed" wolf: the test binary, hard-linked (copied where a link
/// is refused) into `<case>/bin/` with a `std/` beside it holding
/// `std.besidemark`. `std` as given: `Some(true)` a directory, `Some(false)`
/// a plain FILE named `std`, `None` nothing beside. Returns the binary.
fn installed_wolf(case: &str, std_beside: Option<bool>) -> PathBuf {
    let bin = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(case)
        .join("bin");
    let _ = std::fs::remove_dir_all(&bin);
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join(Path::new(wolf()).file_name().unwrap());
    if std::fs::hard_link(wolf(), &exe).is_err() {
        std::fs::copy(wolf(), &exe).unwrap();
    }
    match std_beside {
        Some(true) => {
            std::fs::create_dir_all(bin.join("std/besidemark")).unwrap();
            std::fs::write(
                bin.join("std/besidemark/b.lu"),
                "pub fn mark() -> int { 29 }\n",
            )
            .unwrap();
        }
        Some(false) => std::fs::write(bin.join("std"), "not a tree\n").unwrap(),
        None => {}
    }
    exe
}

/// A std tree holding exactly one module, `std.<mark>`, at `<case>/<name>`.
fn marked_root(case: &str, name: &str, mark: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case).join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(mark)).unwrap();
    std::fs::write(root.join(mark).join("m.lu"), "pub fn mark() -> int { 1 }\n").unwrap();
    root
}

/// A package `<case>/pkg/main.lu` using `std.<mark>`; with `manifest_std`
/// it carries a `wolf.pkg` whose `std` path dependency is that tree.
fn marked_entry(case: &str, mark: &str, manifest_std: Option<&Path>) -> PathBuf {
    let pkg = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(case)
        .join("pkg");
    let _ = std::fs::remove_dir_all(&pkg);
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(
        pkg.join("main.lu"),
        format!("use std.{mark}\nfn main() -> int {{\n    {mark}.mark()\n}}\n"),
    )
    .unwrap();
    if let Some(root) = manifest_std {
        std::fs::write(
            pkg.join("wolf.pkg"),
            format!(
                "pkg {{\n    name: \"s204/prec\",\n    version: \"0.1.0\",\n    edition: \"1\",\n\n    deps: {{\n        std: {{ path: \"{}\" }},\n    }},\n}}\n",
                root.display().to_string().replace('\\', "/")
            ),
        )
        .unwrap();
    }
    pkg.join("main.lu")
}

/// Which root won, as `wolf build <entry> --emit=obj` with WOLF_STD
/// scrubbed unless given: `"pass"` when the object was built,
/// `"fail(E0301)"` when the import missed. `build` is the verb that reads
/// all four sources — `conform-run` is the conformance protocol and never
/// reads a `wolf.pkg` — and an object needs no runtime library beside the
/// linked test binary. `None` (a loud SKIP) where this host refuses the
/// native tier by name.
fn verdict_with(exe: &Path, entry: &Path, extra: &[&str], env: &[(&str, &str)]) -> Option<String> {
    // A package emits one object per module, `prec.<module>.o`; the std
    // module's object is the evidence a std tree answered.
    let dir = entry.parent().unwrap();
    let std_objs = |dir: &Path| -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("prec.std."))
            .count()
    };
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.file_name().to_string_lossy().starts_with("prec.") {
            std::fs::remove_file(e.path()).unwrap();
        }
    }
    let obj = dir.join("prec.o");
    let mut cmd = Command::new(exe);
    cmd.arg("build")
        .arg(entry)
        .arg("--emit=obj")
        .arg("-o")
        .arg(&obj)
        .args(extra)
        .env_remove("WOLF_STD");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("wolf runs");
    let err = String::from_utf8_lossy(&out.stderr);
    if out.status.success() && std_objs(dir) == 1 {
        return Some("pass".to_string());
    }
    if err.contains("error[E0301]") {
        return Some("fail(E0301)".to_string());
    }
    if lane_exit::environment_refusal(&out, "std root precedence") {
        eprintln!("SKIP std root precedence: {}", err.trim());
        return None;
    }
    panic!(
        "wolf build answered neither an object nor E0301 ({}):\n{err}",
        out.status
    );
}

/// One precedence row: `None` was a loud SKIP; otherwise the verdict is
/// the one the order predicts.
fn expect(got: Option<String>, want: &str, what: &str) {
    if let Some(got) = got {
        assert_eq!(got, want, "{what}");
    }
}

/// The precondition every older test in this file rests on: the cargo
/// build's own `wolf` has no `std` beside it, so it still has no default.
#[test]
fn the_cargo_built_wolf_has_no_std_beside_it() {
    let dir = Path::new(wolf()).parent().unwrap();
    assert!(
        !dir.join("std").exists(),
        "{} has a std beside it; every no-root test here would read the default",
        dir.display()
    );
}

/// Order 0: nothing configured — the std beside the binary answers.
/// Red at trunk ac0ac498: fail(E0301), the stub's miss.
#[test]
fn the_std_beside_the_binary_is_the_default_root() {
    let exe = installed_wolf("prec_default", Some(true));
    let entry = marked_entry("prec_default", "besidemark", None);
    expect(verdict_with(&exe, &entry, &[], &[]), "pass", "");
}

/// …and it runs: the checked machine executes the default tree's code.
#[test]
fn a_program_runs_against_the_default_root() {
    let exe = installed_wolf("prec_default_run", Some(true));
    let entry = marked_entry("prec_default_run", "besidemark", None);
    let out = Command::new(&exe)
        .args([
            "conform-run",
            entry.to_str().unwrap(),
            "--checked",
            "--json",
        ])
        .env_remove("WOLF_STD")
        .output()
        .expect("wolf runs");
    let record: serde_json::Value = serde_json::from_slice(&out.stdout).expect("record");
    assert_eq!(record["verdict"], "exit(29)", "{record}");
}

/// Order 1: a `wolf.pkg` `std` path dependency beats the default.
#[test]
fn a_manifest_std_beats_the_default() {
    let exe = installed_wolf("prec_pkg", Some(true));
    let root = marked_root("prec_pkg", "pkgstd", "pkgmark");
    let entry = marked_entry("prec_pkg", "pkgmark", Some(&root));
    expect(verdict_with(&exe, &entry, &[], &[]), "pass", "");
    let entry = marked_entry("prec_pkg", "besidemark", Some(&root));
    expect(verdict_with(&exe, &entry, &[], &[]), "fail(E0301)", "");
}

/// Order 2: `WOLF_STD` beats the manifest and the default.
#[test]
fn wolf_std_beats_the_manifest_and_the_default() {
    let exe = installed_wolf("prec_env", Some(true));
    let env_root = marked_root("prec_env", "envstd", "envmark");
    let pkg_root = marked_root("prec_env", "pkgstd", "pkgmark");
    let env = [("WOLF_STD", env_root.to_str().unwrap())];
    let entry = marked_entry("prec_env", "envmark", Some(&pkg_root));
    expect(verdict_with(&exe, &entry, &[], &env), "pass", "");
    for loser in ["pkgmark", "besidemark"] {
        let entry = marked_entry("prec_env", loser, Some(&pkg_root));
        expect(verdict_with(&exe, &entry, &[], &env), "fail(E0301)", loser);
    }
}

/// Order 3: `--std-root` beats `WOLF_STD`, the manifest and the default.
#[test]
fn the_flag_beats_everything() {
    let exe = installed_wolf("prec_flag", Some(true));
    let flag_root = marked_root("prec_flag", "flagstd", "flagmark");
    let env_root = marked_root("prec_flag", "envstd", "envmark");
    let pkg_root = marked_root("prec_flag", "pkgstd", "pkgmark");
    let flag = ["--std-root", flag_root.to_str().unwrap()];
    let env = [("WOLF_STD", env_root.to_str().unwrap())];
    let entry = marked_entry("prec_flag", "flagmark", Some(&pkg_root));
    expect(verdict_with(&exe, &entry, &flag, &env), "pass", "");
    for loser in ["envmark", "pkgmark", "besidemark"] {
        let entry = marked_entry("prec_flag", loser, Some(&pkg_root));
        expect(
            verdict_with(&exe, &entry, &flag, &env),
            "fail(E0301)",
            loser,
        );
    }
}

/// Order 4: nothing beside, nothing configured — the prelude stub, as
/// before the ruling (and #251's note still names the mechanism).
#[test]
fn nothing_beside_keeps_the_stub() {
    let exe = installed_wolf("prec_none", None);
    let entry = marked_entry("prec_none", "besidemark", None);
    expect(verdict_with(&exe, &entry, &[], &[]), "fail(E0301)", "");
}

/// A FILE named `std` beside the binary is not a root: the stub answers,
/// never an error about a tree nobody configured.
#[test]
fn a_std_file_beside_the_binary_is_not_a_root() {
    let exe = installed_wolf("prec_file", Some(false));
    let entry = marked_entry("prec_file", "besidemark", None);
    expect(verdict_with(&exe, &entry, &[], &[]), "fail(E0301)", "");
}

/// With the default in play the #251 note would be false, so a miss
/// against it reads as any configured root's miss: no "set WOLF_STD".
#[test]
fn a_miss_against_the_default_does_not_name_wolf_std() {
    let exe = installed_wolf("prec_default_miss", Some(true));
    let entry = marked_entry("prec_default_miss", "nosuchmark", None);
    let out = Command::new(&exe)
        .arg("conform-run")
        .arg(&entry)
        .arg("--phase=resolve")
        .env_remove("WOLF_STD")
        .output()
        .expect("wolf runs");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("E0301"), "the miss is still reported:\n{err}");
    assert!(!err.contains("WOLF_STD"), "no root-is-unset note:\n{err}");
}

/// A freestanding build gets no default (s204's census: the shipped std is
/// the hosted library, and its home modules stopped 27 of pax's 29 kernels):
/// `--target x86_64-unknown-none` with the std beside the binary is the
/// stub's E0301, and an explicit root still answers.
#[test]
fn a_freestanding_build_reads_no_default() {
    let exe = installed_wolf("prec_freestanding", Some(true));
    let pkg = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prec_freestanding/kpkg");
    let _ = std::fs::remove_dir_all(&pkg);
    std::fs::create_dir_all(&pkg).unwrap();
    let entry = pkg.join("k.lu");
    std::fs::write(
        &entry,
        "use std.besidemark\n\nexport fn kmain() -> int {\n    besidemark.mark()\n}\n",
    )
    .unwrap();
    let flag_root = exe.parent().unwrap().join("std");
    let build = |extra: &[&str]| {
        Command::new(&exe)
            .current_dir(&pkg)
            .args([
                "build",
                "k.lu",
                "--target",
                "x86_64-unknown-none",
                "--emit=obj",
                "-o",
                "k.o",
            ])
            .args(extra)
            .env_remove("WOLF_STD")
            .env_remove("WOLF_RT_NONE_LIB")
            .output()
            .expect("wolf runs")
    };
    let out = build(&[]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && err.contains("error[E0301]"),
        "a kernel must not read the shipped std by default ({}):\n{err}",
        out.status
    );
    let out = build(&["--std-root", flag_root.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "an explicit root still reaches a kernel ({}):\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}
