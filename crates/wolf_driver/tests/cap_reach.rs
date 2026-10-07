//! s217 (wolf-lang#615) — a package's capabilities are what its code
//! REACHES, not only what it imports.
//!
//! Each fixture under `tests/fixtures/pkg/reach/<case>/` is a root
//! `app` that declares nothing and a dependency `pad` (or a mini std)
//! whose code reaches one capability. Until s217 a `caps=[]` dependency
//! called `fs_read_text` with no diagnostic, `wolf audit` printed
//! `effective: []`, and `wolf audit --ci` passed — even on an
//! undeclared `use std.net`, which only the build refused.
//!
//! The witnesses, red at trunk `85de08ff` (every build 0, every audit
//! 0) and green here:
//! - one host builtin per capability-carrying sandbox category (fs,
//!   net, exec, env), each refused E1504 naming the builtin and its site;
//! - a builtin reached only through the dependency's own helper, one
//!   module down;
//! - `wolf audit --ci` failing on an undeclared capability with no
//!   build and no `wolf.sum`;
//! - std modules charged to their importer for what THEIR code
//!   reaches (`std.process` → `exec`, `std.os` → `env`);
//! - and the honest cases unchanged: a fn the package names like a
//!   builtin is its own, and a declared capability builds.
//!
//! Every refusal is a front-end stop, so the refusal rows run on every
//! host. The honest rows build: a build that stops with any `error[`
//! fails the test, and only a diagnostic-free exit 2 (no linker) skips,
//! loudly — exit 2 alone is never a skip (wolf-lang#550).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// Copy one reach fixture (and the mini std beside it) into a fresh
/// tmp dir; the fixtures stay pristine.
fn stage(case: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pkg/reach");
    let dest = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("cap_reach_{case}"));
    let _ = std::fs::remove_dir_all(&dest);
    copy_tree(&src.join(case), &dest.join(case));
    copy_tree(&src.join("std"), &dest.join("std"));
    dest.join(case)
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("read fixture dir").flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_tree(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).expect("copy fixture file");
        }
    }
}

/// Run `wolf` in `dir` with WOLF_STD scrubbed: the fixtures carry their
/// own std as a path dependency when they need one.
fn run_wolf(dir: &Path, args: &[&str]) -> Output {
    Command::new(wolf())
        .args(args)
        .current_dir(dir)
        .env_remove("WOLF_STD")
        .output()
        .expect("wolf runs")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn build(dir: &Path) -> Output {
    let exe = dir.join(if cfg!(windows) { "app.exe" } else { "app.out" });
    run_wolf(dir, &["build", "app/main.lu", "-o", exe.to_str().unwrap()])
}

/// A front-end refusal: exit 2 and an E1504 headline naming `head`.
fn assert_refused(out: &Output, head: &str) -> String {
    let err = stderr(out);
    assert_eq!(out.status.code(), Some(2), "stderr:\n{err}");
    assert!(
        err.lines()
            .any(|l| l.starts_with("error[E1504]:") && l.contains(head)),
        "expected an E1504 headline containing {head:?}:\n{err}"
    );
    err
}

/// An honest build: no diagnostic error of any kind. Exit 0, or a
/// diagnostic-free environment refusal (no linker), skipped loudly.
fn assert_builds(out: &Output, what: &str) {
    let err = stderr(out);
    assert!(
        !err.lines().any(|l| l.starts_with("error[")),
        "{what} must build with no error:\n{err}"
    );
    if out.status.code() == Some(0) {
        return;
    }
    if lane_exit::environment_refusal(out, what) {
        eprintln!("SKIP {what}: {}", err.trim());
        return;
    }
    panic!("{what}: exit {:?}\n{err}", out.status.code());
}

fn audit(dir: &Path, ci: bool) -> Output {
    let mut args = vec!["audit", "--dir", "app"];
    if ci {
        args.insert(1, "--ci");
    }
    run_wolf(dir, &args)
}

/// One `caps=[]` dependency reaching one capability through one host
/// builtin: the build is refused at the dependency's manifest, naming
/// the builtin and pointing at its call site; the audit derives the
/// capability from the code and `--ci` refuses with no build run and
/// no `wolf.sum` written.
fn builtin_witness(case: &str, builtin: &str, cap: &str, site: &str) {
    let dir = stage(case);
    let out = audit(&dir, true);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    let report = stdout(&out);
    assert!(
        report.contains(&format!("effective: [{cap}]\n")),
        "{report}"
    );
    assert!(
        report.contains(&format!(
            "  {cap}: pad — UNDECLARED: calls `{builtin}` at {site}\n"
        )),
        "{report}"
    );
    assert!(
        report.contains(&format!(
            "wolf audit: `pad` reaches capability `{cap}` without declaring it"
        )),
        "{report}"
    );
    assert!(
        stderr(&out).contains("undeclared capability use — refusing (--ci)"),
        "{}",
        stderr(&out)
    );
    // The audit read source only: nothing was built or recorded.
    assert!(
        !dir.join("app/wolf.sum").exists(),
        "the audit wrote a ledger"
    );
    assert!(
        !dir.join("app/.lu-cache").exists(),
        "the audit built something"
    );
    // Without --ci the same report, exit 0.
    let out = audit(&dir, false);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(stdout(&out), report);

    let out = build(&dir);
    let err = assert_refused(
        &out,
        &format!("dependency `pad` calls `{builtin}` but does not declare the `{cap}` capability"),
    );
    assert!(
        err.contains(&format!(" ::: {site}")),
        "the call site is shown:\n{err}"
    );
    assert!(
        err.contains(&format!("`{builtin}` reaches the `{cap}` capability here")),
        "{err}"
    );
}

#[test]
fn issue_615_fs_read_text_in_a_caps_free_dependency_is_e1504() {
    builtin_witness("fs", "fs_read_text", "fs", "pkg://pad/pad.lu:4:16");
}

#[test]
fn a_net_builtin_in_a_caps_free_dependency_is_e1504() {
    builtin_witness("net", "net_listen", "net", "pkg://pad/pad.lu:4:14");
}

#[test]
fn an_exec_builtin_in_a_caps_free_dependency_is_e1504() {
    builtin_witness("exec", "os_spawn", "exec", "pkg://pad/pad.lu:4:15");
}

#[test]
fn an_env_builtin_in_a_caps_free_dependency_is_e1504() {
    builtin_witness("env", "env_get", "env", "pkg://pad/pad.lu:4:16");
}

#[test]
fn a_builtin_behind_the_dependencys_own_helper_is_e1504() {
    // `pad.left` calls `inner.probe`, a `pub(pkg)` fn one module down,
    // which calls `fs_exists`: the dependency is charged however its
    // code reaches the builtin.
    builtin_witness("helper", "fs_exists", "fs", "pkg://pad/inner/inner.lu:3:8");
}

#[test]
fn a_fn_named_like_a_builtin_is_the_packages_own() {
    // The resolver binds `fs_read_text(s)` to pad's own declaration,
    // so nothing is charged — the derivation reads the resolver's
    // decision, never the spelling.
    let dir = stage("shadow");
    let out = audit(&dir, true);
    assert_eq!(out.status.code(), Some(0), "stdout:\n{}", stdout(&out));
    assert!(stdout(&out).contains("effective: []\n"), "{}", stdout(&out));
    assert_builds(&build(&dir), "shadow");
}

#[test]
fn a_declared_capability_builds_and_the_audit_gives_its_reason() {
    let dir = stage("declared");
    let out = audit(&dir, true);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    let report = stdout(&out);
    assert!(report.contains("└── pad 1.0.0 caps=[fs]\n"), "{report}");
    assert!(report.contains("effective: [fs]\n"), "{report}");
    assert!(
        report.contains("  fs: pad — declared; calls `fs_read_text` at pkg://pad/pad.lu:4:16\n"),
        "{report}"
    );
    assert_builds(&build(&dir), "declared");
}

#[test]
fn audit_ci_refuses_an_undeclared_std_net_import_without_a_build() {
    // #615's second gap: only the build refused this; the audit passed.
    let dir = stage("stdnet");
    let out = audit(&dir, true);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    let report = stdout(&out);
    assert!(report.contains("effective: [net]\n"), "{report}");
    assert!(
        report.contains(
            "  net: demo/app (root) — UNDECLARED: imports `std.net` at app/main.lu:3:1\n"
        ),
        "{report}"
    );
    assert!(!dir.join("app/wolf.sum").exists());
    assert_refused(
        &build(&dir),
        "this package uses `std.net` but does not declare the `net` capability",
    );
}

#[test]
fn a_std_module_charges_its_importer_for_what_its_code_reaches() {
    // `std.process` starts children (`os_spawn`) and imports `std.net`.
    // At trunk the build asked the ROOT for `net` (std modules fell
    // through to it) and never for `exec`.
    let dir = stage("stdproc");
    let err = assert_refused(
        &build(&dir),
        "this package uses `std.process` but does not declare the `exec` capability",
    );
    assert!(err.contains("through `os_spawn`"), "{err}");
    assert!(
        err.contains("this package uses `std.process` but does not declare the `net` capability"),
        "{err}"
    );
    let out = audit(&dir, true);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stdout(&out).contains("effective: [net, exec]\n"),
        "{}",
        stdout(&out)
    );

    // `std.os` reaches the core count (`env`); no facade rule names it.
    let dir = stage("stdos");
    assert_refused(
        &build(&dir),
        "this package uses `std.os` but does not declare the `env` capability",
    );
    let out = audit(&dir, true);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stdout(&out).contains("effective: [env]\n"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn an_audit_that_cannot_read_the_code_vouches_for_nothing() {
    // A manifest whose directory holds no wolf source (lobo's shape:
    // `wolf.pkg` at the repository root, the program under src/): the
    // audit shows the declarations, says it could not derive, and
    // `--ci` refuses rather than pass on what it never saw.
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cap_reach_nocode");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("app")).unwrap();
    std::fs::write(
        dir.join("app/wolf.pkg"),
        "pkg {\n    name:    \"demo/app\",\n    version: \"0.1.0\",\n    edition: \"1\",\n}\n",
    )
    .unwrap();
    let out = audit(&dir, false);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert!(
        stdout(&out).contains("wolf audit: cannot derive capabilities from the code:"),
        "{}",
        stdout(&out)
    );
    let out = audit(&dir, true);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    assert!(
        stderr(&out).contains("nothing is vouched for — refusing (--ci)"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn the_audit_covers_every_standalone_entry_beside_the_manifest() {
    // pax's shape: `kmain_*.lu` entries marked `member: false` beside
    // one `wolf.pkg`. The directory view loads none of them, so the
    // audit resolves each entry as its build would and unites them.
    let dir = stage("entries");
    let out = audit(&dir, true);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    let report = stdout(&out);
    assert!(report.contains("effective: [env]\n"), "{report}");
    assert!(
        report.contains(
            "  env: demo/entries (root) — UNDECLARED: calls `os_cpus` at app/cores.lu:6:13\n"
        ),
        "{report}"
    );
    let exe = dir.join(if cfg!(windows) { "q.exe" } else { "q.out" });
    assert_builds(
        &run_wolf(
            &dir,
            &["build", "app/quiet.lu", "-o", exe.to_str().unwrap()],
        ),
        "quiet",
    );
    assert_refused(
        &run_wolf(
            &dir,
            &["build", "app/cores.lu", "-o", exe.to_str().unwrap()],
        ),
        "this package calls `os_cpus` but does not declare the `env` capability",
    );
}
