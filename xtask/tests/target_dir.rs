//! wolf-lang#1187: xtask's steps find the compiler where cargo put it.
//!
//! With `CARGO_TARGET_DIR` set, cargo builds the driver somewhere other
//! than `target/`, and the steps used to run the fixed `target/debug/wolf`:
//! `xtask corpus` then skipped every file it could not start and passed with
//! "phase execution pending a non-stub driver", having run nothing. This row
//! does what the issue's reproduction does: a clean tree (a worktree of
//! `HEAD`, so no `target/` of its own) and three real steps against one
//! relocated target directory, built once and shared; each step is held to
//! having run the compiler cargo built there.
//!
//! **What it costs, stated:** one debug build of the driver and runtime into
//! the relocated directory (printed as a `REPORT` line), reused by later runs
//! on the same host; then the corpus step on four fixed files (`SUBSET`: the
//! bug is a step that ran nothing, and four files prove it ran something),
//! and fmt-lu and audit-surface whole. Each step's seconds are printed.
//! **Where it runs:** on linux, where CI's `lint + test + corpus` job runs it
//! inside `cargo xtask ci`. The macOS and windows `build + test` legs would
//! pay the same build again on the slowest and scarcest runners for the same
//! answer, so they skip it, loudly; `WOLF_XTASK_TARGET_DIR_ROW=1` runs it on
//! any host.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Instant;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits in the workspace")
        .to_path_buf()
}

fn loud(line: &str) {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

fn runs_here() -> bool {
    cfg!(target_os = "linux")
        || std::env::var_os("WOLF_XTASK_TARGET_DIR_ROW").is_some_and(|v| v == "1")
}

/// A worktree of `HEAD` at `path`, removed when dropped.
struct Tree(PathBuf);

impl Tree {
    fn new(path: PathBuf) -> Tree {
        let _ = Command::new("git")
            .arg("-C")
            .arg(workspace())
            .args(["worktree", "remove", "--force"])
            .arg(&path)
            .output();
        let _ = std::fs::remove_dir_all(&path);
        let ok = Command::new("git")
            .arg("-C")
            .arg(workspace())
            .args(["worktree", "add", "--detach"])
            .arg(&path)
            .arg("HEAD")
            .stdout(Stdio::null())
            .status()
            .expect("git runs");
        assert!(ok.success(), "a worktree of HEAD at {}", path.display());
        Tree(path)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = Command::new("git")
            .arg("-C")
            .arg(workspace())
            .args(["worktree", "remove", "--force"])
            .arg(&self.0)
            .output();
    }
}

/// The corpus files the row runs: two native programs (one with a stdout
/// check, one with a trap), a resolve-phase program and an enforced rule.
const SUBSET: [&str; 4] = [
    "corpus/hello.lu",
    "corpus/overflow.lu",
    "corpus/errors.lu",
    "corpus/grammar/attr_unknown.lu",
];

/// `xtask <step> <args>` from the clean tree with `CARGO_TARGET_DIR` at `dir`.
fn xtask(step: &str, args: &[String], tree: &Path, dir: &Path) -> (Output, u64) {
    let start = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg(step)
        .args(args)
        .current_dir(tree)
        .env("CARGO_TARGET_DIR", dir)
        .stdin(Stdio::null())
        .output()
        .expect("xtask runs");
    (out, start.elapsed().as_secs())
}

#[test]
fn the_steps_run_the_compiler_cargo_built_under_a_relocated_target() {
    if !runs_here() {
        loud(
            "SKIP: the relocated-target row runs on linux (set WOLF_XTASK_TARGET_DIR_ROW=1 to run it here)",
        );
        return;
    }
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let tree = Tree::new(tmp.join("relocated-tree"));
    let dir = tmp.join("relocated-target");
    std::fs::create_dir_all(&dir).expect("relocated target dir");
    assert!(
        !tree.0.join("target").exists(),
        "the clean tree has no target directory of its own"
    );
    let wolf = dir
        .join("debug")
        .join(format!("wolf{}", std::env::consts::EXE_SUFFIX));

    let start = Instant::now();
    let built = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args(["build", "-p", "wolf_driver", "-p", "wolf_rt", "--quiet"])
        .current_dir(&tree.0)
        .env("CARGO_TARGET_DIR", &dir)
        .status()
        .expect("cargo build runs");
    assert!(built.success(), "the driver builds into {}", dir.display());
    loud(&format!(
        "REPORT the relocated target's driver build took {} s ({})",
        start.elapsed().as_secs(),
        dir.display()
    ));

    let subset: Vec<String> = SUBSET.iter().map(|f| format!("--file={f}")).collect();
    let (out, secs) = xtask("corpus", &subset, &tree.0, &dir);
    let err = String::from_utf8_lossy(&out.stderr);
    for line in err
        .lines()
        .filter(|l| l.starts_with("corpus: ") && !l.contains(".lu"))
    {
        loud(&format!("REPORT {line}"));
    }
    loud(&format!("REPORT xtask corpus took {secs} s"));
    assert!(
        !err.contains("phase execution pending a non-stub driver"),
        "xtask corpus under a relocated target directory ran nothing: it printed \
         `phase execution pending a non-stub driver` (wolf-lang#1187)"
    );
    assert!(
        err.contains("phase ledger enforced via conform-run"),
        "xtask corpus did not enforce the phase ledger:\n{err}"
    );
    let running = format!("corpus: running {}", wolf.display());
    assert!(
        err.contains(&running),
        "xtask corpus did not run the compiler cargo built ({running}):\n{err}"
    );
    let executed = err
        .lines()
        .find_map(|l| {
            l.strip_prefix("corpus: ")
                .and_then(|r| r.split_once(" file(s) executed by "))
                .and_then(|(n, _)| n.parse::<usize>().ok())
        })
        .expect("xtask corpus says how many files it executed");
    assert_eq!(
        executed,
        SUBSET.len(),
        "xtask corpus executed {executed} file(s), not the {} it was given:\n{err}",
        SUBSET.len()
    );
    let summary = format!(
        "corpus: {} file(s), 0 bad — phase ledger enforced via conform-run",
        SUBSET.len()
    );
    assert!(
        err.contains(&summary),
        "xtask corpus ran other than the four files ({summary}):\n{err}"
    );
    assert!(out.status.success(), "xtask corpus failed:\n{err}");

    for step in ["fmt-lu", "audit-surface"] {
        let (out, secs) = xtask(step, &[], &tree.0, &dir);
        loud(&format!("REPORT xtask {step} took {secs} s"));
        assert!(
            out.status.success(),
            "xtask {step} under a relocated target directory:\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
