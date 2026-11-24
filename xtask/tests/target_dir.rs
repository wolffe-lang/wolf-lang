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
//! A second row holds `xtask fmt-fuzz` to the release loop cargo built in the
//! same relocated directory (the follow-up for the `target/release` lookups),
//! and a third holds `xtask rt-none` to the freestanding runtime cargo built
//! when the directory moves by `build.target-dir`.
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

/// The rows share one relocated target directory and build into it, so they
/// run one at a time.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
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

/// The follow-up to wolf-lang#1187: `xtask fmt-fuzz` runs the loop cargo
/// built under the relocated target directory. It used to start the fixed
/// `target/release/examples/fmt_fuzz`, and a loop that could not be started
/// was reported as "findings above" — a finding nobody's input produced.
/// **What it costs, stated:** one release build of `wolf_fmt`'s example into
/// the relocated directory (27 s cold on a 2-job macOS build, printed here),
/// then 200 cases (under a second).
#[test]
fn fmt_fuzz_runs_the_loop_cargo_built_under_a_relocated_target() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    if !runs_here() {
        loud(
            "SKIP: the relocated-target fmt-fuzz row runs on linux (set WOLF_XTASK_TARGET_DIR_ROW=1 to run it here)",
        );
        return;
    }
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let tree = Tree::new(tmp.join("relocated-tree-fuzz"));
    let dir = tmp.join("relocated-target");
    std::fs::create_dir_all(&dir).expect("relocated target dir");
    let fuzz = dir
        .join("release")
        .join("examples")
        .join(format!("fmt_fuzz{}", std::env::consts::EXE_SUFFIX));

    let start = Instant::now();
    let built = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args([
            "build",
            "--release",
            "-p",
            "wolf_fmt",
            "--example",
            "fmt_fuzz",
            "--quiet",
        ])
        .current_dir(&tree.0)
        .env("CARGO_TARGET_DIR", &dir)
        .status()
        .expect("cargo build runs");
    assert!(built.success(), "the fuzzer builds into {}", dir.display());
    loud(&format!(
        "REPORT the relocated target's fuzzer build took {} s ({})",
        start.elapsed().as_secs(),
        dir.display()
    ));

    let args: Vec<String> = ["--cases=200", "--seed=63", "--allow-open"]
        .iter()
        .map(|a| a.to_string())
        .collect();
    let (out, secs) = xtask("fmt-fuzz", &args, &tree.0, &dir);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    for line in text.lines().filter(|l| l.starts_with("fmt-fuzz: ")) {
        loud(&format!("REPORT {line}"));
    }
    loud(&format!("REPORT xtask fmt-fuzz took {secs} s"));
    assert!(
        !text.contains("findings above"),
        "xtask fmt-fuzz under a relocated target directory reported findings from a loop it \
         never ran: it printed `findings above` (the follow-up to wolf-lang#1187):\n{text}"
    );
    let running = format!("fmt-fuzz: running {}", fuzz.display());
    assert!(
        text.contains(&running),
        "xtask fmt-fuzz did not run the loop cargo built ({running}):\n{text}"
    );
    assert!(
        text.contains("fmt-fuzz: 200 cases"),
        "the loop says it ran the 200 cases it was given:\n{text}"
    );
    assert!(
        out.status.success() && text.contains("fmt-fuzz: invariants held"),
        "xtask fmt-fuzz failed:\n{text}"
    );
}

/// The follow-up to wolf-lang#1187: `xtask rt-none` finds the freestanding
/// runtime where cargo built it when the target directory moves by
/// `build.target-dir`, here by its environment form `CARGO_BUILD_TARGET_DIR`
/// with `CARGO_TARGET_DIR` removed. It read `CARGO_TARGET_DIR` alone, so it
/// looked for the archive cargo had just built under `target/`.
/// **What it costs, stated:** one release build of `wolf_rt_none` (a `no_std`
/// crate with no dependencies) for `x86_64-unknown-none` into the relocated
/// directory, inside the step's printed seconds. A toolchain without that
/// target makes rt-none skip loudly, and the row with it; under
/// `WOLF_RT_NONE_REQUIRE=1` (every CI job) that is rt-none's own failure.
#[test]
fn rt_none_builds_where_cargo_puts_it_under_build_target_dir() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    if !runs_here() {
        loud(
            "SKIP: the relocated-target rt-none row runs on linux (set WOLF_XTASK_TARGET_DIR_ROW=1 to run it here)",
        );
        return;
    }
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let tree = Tree::new(tmp.join("relocated-tree-rt-none"));
    let dir = tmp.join("relocated-target");
    std::fs::create_dir_all(&dir).expect("relocated target dir");
    let lib = dir
        .join("x86_64-unknown-none")
        .join("release")
        .join("libwolf_rt_none.a");

    let start = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("rt-none")
        .current_dir(&tree.0)
        .env_remove("CARGO_TARGET_DIR")
        .env("CARGO_BUILD_TARGET_DIR", &dir)
        .stdin(Stdio::null())
        .output()
        .expect("xtask runs");
    let secs = start.elapsed().as_secs();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    for line in text.lines().filter(|l| l.starts_with("rt-none: ")) {
        loud(&format!("REPORT {line}"));
    }
    loud(&format!("REPORT xtask rt-none took {secs} s"));
    if text.contains("rt-none: SKIP") {
        loud(
            "SKIP: rt_none_builds_where_cargo_puts_it_under_build_target_dir: this toolchain has no x86_64-unknown-none target",
        );
        return;
    }
    assert!(
        !text.contains("did not produce"),
        "xtask rt-none under build.target-dir looked for the archive where cargo did not \
         put it: it printed `did not produce` (the follow-up to wolf-lang#1187):\n{text}"
    );
    let built = format!("rt-none: {}", lib.display());
    assert!(
        text.contains(&built),
        "xtask rt-none did not name the archive cargo built ({built}):\n{text}"
    );
    assert!(lib.is_file(), "the archive is at {}", lib.display());
    assert!(out.status.success(), "xtask rt-none failed:\n{text}");
}
