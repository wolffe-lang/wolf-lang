//! wolf-lang#1082: on Linux, the hosted link asks cc for lld
//! (`-fuse-ld=lld`) whenever an `ld.lld` is on PATH, and it did so
//! silently, so the bytes of a binary depended on whether lld happened to
//! be installed and nothing said why. The choice is unchanged; `wolf build`
//! now names the lld it picked, once, as a note, and `wolf build --help`
//! (and so `wolf.1`) documents the selection and the override.
//!
//! The linking rows need a Linux host with an `ld.lld` on PATH (every CI
//! Linux job installs one, as does the gauntlet's image). Elsewhere they
//! SKIP by name; `WOLF_LLD_NOTE_REQUIRE` turns a missing lld into a red.
mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const NOTE: &str = "wolf build: note: linking through ";

fn wolf() -> Command {
    Command::new(env!("CARGO_BIN_EXE_wolf"))
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// `ld.lld` as PATH resolves it, the way the link's probe finds it.
fn lld_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("ld.lld"))
        .find(|p| p.is_file())
}

/// A hello program in its own directory; `None` (a named SKIP) where the
/// row cannot run.
fn staged(case: &str) -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        eprintln!("SKIP {case}: the lld routing is Linux's (macOS links through Apple ld, Windows its own)");
        return None;
    }
    if lld_on_path().is_none() {
        assert!(
            std::env::var_os("WOLF_LLD_NOTE_REQUIRE").is_none(),
            "{case}: WOLF_LLD_NOTE_REQUIRE is set and no ld.lld is on PATH"
        );
        eprintln!("SKIP {case}: no ld.lld on PATH");
        return None;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("s289_{case}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(
        dir.join("main.lu"),
        "fn main() -> int {\n    print(\"hello\")\n    0\n}\n",
    )
    .expect("write main.lu");
    Some(dir)
}

fn build(dir: &Path, extra: &[&str], env: &[(&str, &str)]) -> Option<Output> {
    let mut cmd = wolf();
    cmd.arg("build")
        .args(extra)
        .args(["main.lu", "--no-cache", "-o", "hello"])
        .current_dir(dir)
        .env_remove("WOLF_LD");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run wolf build");
    if lane_exit::environment_refusal(&out, "build") {
        eprintln!("SKIP: build: {}", stderr(&out));
        return None;
    }
    Some(out)
}

/// The help names the selection and the override (and `wolf.1` is made
/// from the same text).
#[test]
fn build_help_names_the_lld_selection_and_its_override() {
    let out = wolf().args(["build", "--help"]).output().expect("run wolf");
    let text = String::from_utf8_lossy(&out.stdout);
    for want in ["ld.lld", "-fuse-ld=lld", "WOLF_LD=cc"] {
        assert!(text.contains(want), "`wolf build --help` lacks {want:?}:\n{text}");
    }
}

/// With an `ld.lld` on PATH, the build says which one, once, with its
/// version line and the override.
#[test]
fn a_build_that_picks_lld_from_path_says_which_once() {
    let Some(dir) = staged("lld_note") else {
        return;
    };
    let Some(out) = build(&dir, &[], &[]) else {
        return;
    };
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(0), "{err}");
    let notes: Vec<&str> = err.lines().filter(|l| l.starts_with(NOTE)).collect();
    assert_eq!(notes.len(), 1, "one note:\n{err}");
    let lld = lld_on_path().expect("checked by staged");
    assert!(
        notes[0].contains(&lld.display().to_string()),
        "the note names {}:\n{err}",
        lld.display()
    );
    let version = Command::new(&lld)
        .arg("--version")
        .output()
        .expect("ld.lld --version");
    let line = String::from_utf8_lossy(&version.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    assert!(notes[0].contains(&line), "the note carries {line:?}:\n{err}");
    assert!(notes[0].contains("WOLF_LD=cc"), "{err}");
}

/// `WOLF_LD=cc` keeps the system linker, and there is nothing to note.
#[test]
fn wolf_ld_cc_links_without_the_note() {
    let Some(dir) = staged("lld_note_wolf_ld") else {
        return;
    };
    let Some(out) = build(&dir, &[], &[("WOLF_LD", "cc")]) else {
        return;
    };
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert!(!err.contains(NOTE), "{err}");
}

/// An object-only build links nothing, so it notes nothing.
#[test]
fn an_object_only_build_notes_nothing() {
    let Some(dir) = staged("lld_note_obj") else {
        return;
    };
    let mut cmd = wolf();
    let out = cmd
        .args(["build", "--emit=obj", "main.lu", "-o", "hello.o"])
        .current_dir(&dir)
        .env_remove("WOLF_LD")
        .output()
        .expect("run wolf build");
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert!(!err.contains(NOTE), "{err}");
}
