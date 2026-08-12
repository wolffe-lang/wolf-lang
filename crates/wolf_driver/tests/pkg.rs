//! s51 acceptance — the package manager through the driver.
//!
//! - The dogfood: a fixture project depending on a second fixture
//!   package (`path:` source) builds AND runs end-to-end.
//! - The D33 witness: a dependency carrying a build.rs-analog is
//!   refused with E1503 before its content is trusted for anything.
//! - The I13 witness: undeclared `std.net` use fails E1504; the
//!   declared twin builds — with std itself resolved as a `path:`
//!   dependency from the manifest (the s51 acceptance story).
//! - The verbs: `wolf add/rm/update/audit/tree/why` — manifest edits
//!   land whole, the ledger is byte-stable, and a dependency that
//!   acquires a capability fails `wolf audit --ci` BEFORE the ledger
//!   is refreshed.
//! - Integrity: a git dependency pins into `wolf.sum`; a tampered
//!   store tree fails E1506 (skips loudly when `git` is absent).
//!
//! Native codegen is linux/x86-64 in c06; off-target the file
//! compiles away (the refusal tests would run anywhere, but one
//! honest gate beats two).

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pkg")
}

/// Copy a fixture tree into a fresh tmp case dir (fixtures stay
/// pristine; builds may drop `.lu-cache`/`wolf.sum` in the copy).
fn stage(case: &str, fixture: &str) -> PathBuf {
    let dest = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dest);
    copy_tree(&fixtures().join(fixture), &dest);
    dest
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

/// Run `wolf` with args in `dir`, WOLF_STD scrubbed (fixtures carry
/// their own std when they need one).
fn run_wolf(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(wolf());
    cmd.args(args).current_dir(dir).env_remove("WOLF_STD");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("wolf runs")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Exit 2 from a build = environment (no cc/rt lib): skip loudly,
/// exactly like the other native suites.
fn env_skip(out: &Output, what: &str) -> bool {
    if out.status.code() == Some(2) {
        eprintln!("SKIP {what}: {}", stderr(out).trim());
        return true;
    }
    false
}

// ------------------------------------------------------------ dogfood ----

#[test]
fn dogfood_path_dep_builds_and_runs() {
    let dir = stage("pkg_dogfood_run", "dogfood");
    let out = run_wolf(&dir, &["run", "app/main.lu"], &[]);
    if env_skip(&out, "dogfood") {
        return;
    }
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr:\n{}\nstdout:\n{}",
        stderr(&out),
        stdout(&out)
    );
    assert_eq!(stdout(&out), "twist(20) = 41\n");
}

#[test]
fn dogfood_dep_diagnostics_use_pkg_scheme() {
    // D7: a dependency's on-disk location never leaks — break util,
    // and the error's file display is pkg://util/…, not a tmp path.
    let dir = stage("pkg_dogfood_display", "dogfood");
    std::fs::write(
        dir.join("util/util.lu"),
        "pub fn twist(x: int) -> int {\n    2 * x + undefined_name\n}\n",
    )
    .expect("break util");
    let out = run_wolf(&dir, &["build", "app/main.lu"], &[]);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("pkg://util/util.lu"), "{err}");
    assert!(
        !err.contains("pkg_dogfood_display"),
        "location leaked:\n{err}"
    );
}

// ---------------------------------------------------------------- D33 ----

#[test]
fn hostile_build_script_dep_is_refused_e1503() {
    let dir = stage("pkg_hostile", "hostile");
    let out = run_wolf(&dir, &["build", "app/main.lu"], &[]);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("E1503"), "{err}");
    assert!(err.contains("no build scripts, ever"), "{err}");
    // Refused at the manifest, before the dep's code is in the build.
    assert!(err.contains("E1505"), "the dep entry is named too:\n{err}");
}

// ---------------------------------------------------------------- I13 ----

#[test]
fn undeclared_capability_fails_e1504() {
    let dir = stage("pkg_caps_bad", "caps");
    let out = run_wolf(&dir, &["build", "app/main.lu"], &[]);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("E1504"), "{err}");
    assert!(err.contains("`net` capability"), "{err}");
}

#[test]
fn declared_capability_builds_with_std_as_path_dep() {
    // The acceptance story: std by path in the manifest, no
    // --std-root flag, and the program builds and runs.
    let dir = stage("pkg_caps_ok", "caps");
    let out = run_wolf(&dir, &["run", "app-ok/main.lu"], &[]);
    if env_skip(&out, "caps-ok") {
        return;
    }
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(stdout(&out), "localhost\n");
}

// -------------------------------------------------------------- verbs ----

#[test]
fn add_rm_round_trip_with_lockfile() {
    let dir = stage("pkg_add_rm", "dogfood");
    let app = dir.join("app");
    // Start from a manifest-less project: wolf add creates one.
    std::fs::remove_file(app.join("wolf.pkg")).expect("rm manifest");
    let out = run_wolf(
        &dir,
        &["add", "util", "--path", "../util", "--dir", "app"],
        &[],
    );
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    let manifest = std::fs::read_to_string(app.join("wolf.pkg")).expect("manifest exists");
    assert!(
        manifest.contains("util: { path: \"../util\" }"),
        "{manifest}"
    );
    let sum = std::fs::read_to_string(app.join("wolf.sum")).expect("ledger written");
    assert!(sum.contains("util 0.2.0 - caps=-"), "{sum}");
    // Byte-stable: update with nothing changed rewrites identically.
    let out = run_wolf(&dir, &["update", "--dir", "app"], &[]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(app.join("wolf.sum")).expect("ledger"),
        sum
    );
    // And the project builds (the created manifest is real).
    let out = run_wolf(&dir, &["run", "app/main.lu"], &[]);
    if !env_skip(&out, "add-built") {
        assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
        assert_eq!(stdout(&out), "twist(20) = 41\n");
    }
    // rm: the entry leaves the manifest, the ledger empties away.
    let out = run_wolf(&dir, &["rm", "util", "--dir", "app"], &[]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    let manifest = std::fs::read_to_string(app.join("wolf.pkg")).expect("manifest");
    assert!(!manifest.contains("util:"), "{manifest}");
    assert!(!app.join("wolf.sum").exists(), "empty ledger file lingers");
    // Adding a duplicate alias is refused.
    let out = run_wolf(
        &dir,
        &["add", "util", "--path", "../util", "--dir", "app"],
        &[],
    );
    assert_eq!(out.status.code(), Some(0));
    let out = run_wolf(
        &dir,
        &["add", "util", "--path", "../util", "--dir", "app"],
        &[],
    );
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    assert!(
        stderr(&out).contains("already a dependency"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn audit_tree_and_ci_gate_on_capability_acquisition() {
    let dir = stage("pkg_audit", "dogfood");
    let app = dir.join("app");
    // Record today's world in the ledger.
    let out = run_wolf(&dir, &["update", "--dir", "app"], &[]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    // The tree witness.
    let out = run_wolf(&dir, &["audit", "--dir", "app"], &[]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    let tree = stdout(&out);
    assert!(tree.contains("capability tree (I13)"), "{tree}");
    assert!(tree.contains("demo/app 0.1.0 (root) caps=[]"), "{tree}");
    assert!(tree.contains("└── util 0.2.0 caps=[]"), "{tree}");
    assert!(tree.contains("effective: []"), "{tree}");
    // The upgrade demo: util acquires `exec`. The audit surfaces it
    // and --ci refuses — BEFORE any verb rewrites the ledger.
    std::fs::write(
        dir.join("util/wolf.pkg"),
        "pkg {\n    name:    \"demo/util\",\n    version: \"0.3.0\",\n    edition: \"1\",\n\n    capabilities: [exec],\n}\n",
    )
    .expect("upgrade util");
    let out = run_wolf(&dir, &["audit", "--ci", "--dir", "app"], &[]);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("ACQUIRES capability `exec`"), "{err}");
    // The ledger did not move.
    let sum = std::fs::read_to_string(app.join("wolf.sum")).expect("ledger");
    assert!(sum.contains("util 0.2.0"), "{sum}");
    // `wolf update` accepts the world and re-records it.
    let out = run_wolf(&dir, &["update", "--dir", "app"], &[]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    let sum = std::fs::read_to_string(app.join("wolf.sum")).expect("ledger");
    assert!(sum.contains("util 0.3.0 - caps=exec"), "{sum}");
    let out = run_wolf(&dir, &["audit", "--ci", "--dir", "app"], &[]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
}

#[test]
fn tree_and_why_explain_the_graph() {
    let dir = stage("pkg_why", "dogfood");
    let out = run_wolf(&dir, &["tree", "--dir", "app"], &[]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert!(stdout(&out).contains("util 0.2.0"), "{}", stdout(&out));
    let out = run_wolf(&dir, &["why", "util", "--dir", "app"], &[]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(stdout(&out), "demo/app (root) -> util 0.2.0\n");
    let out = run_wolf(&dir, &["why", "ghost", "--dir", "app"], &[]);
    assert_eq!(out.status.code(), Some(1));
}

// ---------------------------------------------------------- integrity ----

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn git_dep_pins_and_tamper_fails_e1506() {
    if !git_available() {
        eprintln!("SKIP git integrity: no git on PATH");
        return;
    }
    let dir = stage("pkg_git", "dogfood");
    let store = dir.join("store");
    let store_env: &[(&str, &str)] = &[("WOLF_STORE", store.to_str().unwrap())];
    // A local git repo holding the util package, tagged.
    let repo = dir.join("util-repo");
    copy_tree(&dir.join("util"), &repo);
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.org",
            ])
            .args(args)
            .current_dir(&repo)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "--quiet"]);
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "util fixture"]);
    git(&["tag", "v0.2.0"]);
    let url = format!("file://{}", repo.display());

    // add --git: fetches into the store, pins into wolf.sum.
    let app = dir.join("app");
    std::fs::remove_file(app.join("wolf.pkg")).expect("fresh app");
    let out = run_wolf(
        &dir,
        &[
            "add", "gutil", "--git", &url, "--tag", "v0.2.0", "--dir", "app",
        ],
        store_env,
    );
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    let sum = std::fs::read_to_string(app.join("wolf.sum")).expect("ledger");
    assert!(sum.contains("gutil 0.2.0 b3:"), "{sum}");
    // The program builds offline from the store (no re-fetch path).
    std::fs::write(
        app.join("main.lu"),
        "use gutil\n\nfn main() -> !int {\n    let t = gutil.twist(20)\n    print(\"{t}\")\n    0\n}\n",
    )
    .expect("rewrite main");
    let out = run_wolf(&dir, &["run", "app/main.lu"], store_env);
    if !env_skip(&out, "git-built") {
        assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
        assert_eq!(stdout(&out), "41\n");
    }
    // Tamper: edit the store tree under its address. The next build
    // re-derives the hash and refuses with E1506.
    let hash = sum
        .lines()
        .find(|l| l.starts_with("gutil"))
        .and_then(|l| l.split_whitespace().nth(2))
        .expect("hash field")
        .trim_start_matches("b3:");
    let victim = store.join(hash).join("util.lu");
    let mut code = std::fs::read_to_string(&victim).expect("store tree");
    code = code.replace("2 * x + 1", "0 * x");
    std::fs::write(&victim, code).expect("tamper");
    let out = run_wolf(&dir, &["build", "app/main.lu"], store_env);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    assert!(stderr(&out).contains("E1506"), "{}", stderr(&out));
}
