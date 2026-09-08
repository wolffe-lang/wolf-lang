//! The front door, from outside the process (#250).
//!
//! `wolf --help` used to be one line of twenty-three pipe-separated
//! verbs, printed to stderr, exiting 2 — a stranger's first command
//! after `--version` answered with an error and no next step, and
//! `wolf build --help` did not exist at all.

use std::process::{Command, Output};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn run(args: &[&str]) -> Output {
    Command::new(wolf()).args(args).output().expect("wolf runs")
}

fn stdout(args: &[&str]) -> String {
    let out = run(args);
    assert_eq!(
        out.status.code(),
        Some(0),
        "`wolf {}` succeeds; stderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Asking for help is not an error, and the answer goes to stdout so it
/// can be piped into a pager.
#[test]
fn help_exits_zero_on_stdout() {
    for form in [&["--help"][..], &["-h"][..], &["help"][..]] {
        let out = run(form);
        assert_eq!(out.status.code(), Some(0), "`wolf {}` exits 0", form[0]);
        assert!(
            !out.stdout.is_empty(),
            "`wolf {}` answers on stdout",
            form[0]
        );
        assert!(
            out.stderr.is_empty(),
            "`wolf {}` says nothing on stderr",
            form[0]
        );
    }
}

/// The four things the one-line usage never said.
#[test]
fn help_answers_a_stranger() {
    let h = stdout(&["--help"]);
    assert!(
        h.contains("compiled systems language"),
        "what wolf is:\n{h}"
    );
    assert!(h.contains("wolf run hello.lu"), "a first program:\n{h}");
    assert!(h.contains("github.com/wolffe-lang/wolf-lang"), "docs:\n{h}");
    assert!(
        h.contains("wolf <command> --help"),
        "per-command help:\n{h}"
    );
    // The two adjacent gaps filed with #250 and #251.
    assert!(
        h.contains("E0302"),
        "the directory-is-a-module ambush:\n{h}"
    );
    assert!(
        h.contains("WOLF_STD"),
        "how to reach a standard library:\n{h}"
    );
}

/// Every verb the overview lists answers `--help` itself, both ways of
/// asking. This is the gap the issue named: there was no `wolf build
/// --help` for any of the twenty-three.
#[test]
fn every_listed_verb_has_its_own_help() {
    let overview = stdout(&["--help"]);
    let verbs: Vec<String> = overview
        .lines()
        .skip_while(|l| !l.ends_with("code:"))
        .take_while(|l| !l.starts_with("On its own:"))
        .filter(|l| l.starts_with("  ") && !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect();
    assert!(verbs.len() >= 20, "the overview lists the verbs: {verbs:?}");
    for v in &verbs {
        let flag = stdout(&[v, "--help"]);
        let word = stdout(&["help", v]);
        assert_eq!(flag, word, "`wolf {v} --help` and `wolf help {v}` agree");
        assert!(flag.starts_with(&format!("wolf {v} — ")), "{v}:\n{flag}");
        assert!(
            flag.contains(&format!("usage: wolf {v}")),
            "{v} shows its usage:\n{flag}"
        );
    }
}

/// A wrong flag prints the SAME usage line `--help` shows, because both
/// read the one table — the property that keeps them from drifting.
#[test]
fn a_usage_error_and_help_agree() {
    let out = run(&["tree", "--bogus-flag"]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("usage: wolf tree"), "{err}");
    assert!(
        err.contains("`wolf tree --help`"),
        "and points onward: {err}"
    );
    assert!(stdout(&["tree", "--help"]).contains(err.lines().next().unwrap()));
}

/// `wolf run prog.lu --help` is the PROGRAM's `--help`, not wolf's:
/// everything after the entry file is the program's argv, and a
/// toolchain that eats it is a worse bug than the one this fixed.
#[test]
fn run_does_not_steal_the_programs_help_flag() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("help_surface_argv");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("main.lu");
    std::fs::write(&entry, "fn main() {\n    print(\"ran\\n\")\n}\n").unwrap();
    let out = Command::new(wolf())
        .arg("run")
        .arg(&entry)
        .arg("--help")
        .env_remove("WOLF_STD")
        .output()
        .expect("wolf runs");
    let so = String::from_utf8_lossy(&out.stdout);
    assert!(
        so.contains("ran"),
        "the program ran, not wolf's help:\n{so}"
    );
}

/// An unknown verb is still an error — but now it says which word was
/// wrong and shows the reader where to go.
#[test]
fn an_unknown_verb_is_an_error_with_a_way_out() {
    let out = run(&["biuld"]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("`biuld` is not a wolf command"), "{err}");
    assert!(
        err.contains("did you mean `wolf build`?"),
        "the near-miss: {err}"
    );
    assert!(
        err.contains("`wolf --help`"),
        "and where the list is: {err}"
    );
    // Nothing at all is not a typo — that reader is at the front door.
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(2));
    let so = String::from_utf8_lossy(&out.stdout);
    assert!(
        so.contains("wolf run hello.lu"),
        "bare wolf opens the door:\n{so}"
    );
}

/// There is a man page to install, and it is generated from the same
/// table, so it cannot describe a command that no longer exists.
#[test]
fn there_is_a_man_page() {
    let m = stdout(&["--man"]);
    assert!(m.starts_with(".TH WOLF 1 "), "roff, section 1:\n{m}");
    for section in [
        ".SH NAME",
        ".SH SYNOPSIS",
        ".SH COMMANDS",
        ".SH EXIT STATUS",
    ] {
        assert!(m.contains(section), "the man page has {section}");
    }
    assert!(m.contains(".B wolf build\n"), "and documents the verbs");
    assert!(m.contains("WOLF_STD"), "and the environment");
}

/// There are completions to install, for the three shells the house
/// packages target.
#[test]
fn there_are_completions_for_three_shells() {
    for shell in ["bash", "zsh", "fish"] {
        let s = stdout(&["--completions", shell]);
        assert!(s.contains("wolf"), "{shell} completion mentions wolf");
        assert!(
            s.contains("build") && s.contains("conform-run"),
            "{shell} names the verbs"
        );
    }
    let out = run(&["--completions", "tcsh"]);
    assert_eq!(out.status.code(), Some(2), "an unknown shell is refused");
    let out = run(&["--completions"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a missing shell name is refused"
    );
}
