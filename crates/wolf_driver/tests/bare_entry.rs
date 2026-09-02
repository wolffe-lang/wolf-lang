//! #206 — a bare entry name means the same thing to every verb.
//!
//! `Path::new("hello.lu").parent()` is `Some("")`, not `None`, so the
//! package root the loader derived from a bare relative name was the
//! empty path and `read_dir` on it failed: `wolf conform-run
//! hello.lu` answered "the package root has no wolf source files"
//! while `wolf conform-run ./hello.lu` ran the program. The anchoring
//! used to live in ONE CLI parser (`parse_build_cli`, hence
//! build/run/fmt working), so `test`, `interface`, `doc` and
//! `conform-run` all refused; it lives in `wolf_sema::anchor_entry`
//! now, which every loader entry goes through.
//!
//! It mattered because on windows the native tier was refusing the
//! host by name until s60a, which made `wolf conform-run <file>
//! --checked` the only way to execute a wolf program with the
//! published toolchain — one missing `./` between a learner and their
//! first program.
//!
//! Everything here runs from the fixture directory with the entry
//! spelled bare, and nothing needs a linker except the halves that
//! say so.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The linking half needs `libwolf_rt.a` NEXT TO the `wolf` binary —
/// the two-artifact install (s28). `cargo test` does not build the
/// staticlib, and the linux gauntlet lane found that out: this suite
/// was green on a developer box that had built it and red in CI that
/// had not. Same shape as `trap_site.rs`'s.
fn ensure_rt_staticlib() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .expect("cargo builds wolf_rt");
        assert!(status.success(), "wolf_rt staticlib build failed");
    });
}

fn fixture(case: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("bare-entry-{case}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for (name, src) in files {
        std::fs::write(dir.join(name), src).expect("write fixture");
    }
    dir
}

/// Run `wolf` FROM `dir`, so the entry argument is spelled exactly as
/// given — a bare name has an empty parent only when it is relative.
fn wolf_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(wolf())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run wolf")
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or(-1)
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

const HELLO: &str = "fn main() -> !int {\n    print(\"hello, wolf\")\n    0\n}\n";

/// The issue's own reproduction: the bare name is accepted, and the
/// record is byte-identical to the `./`-spelled one — the entry has a
/// single reading, not two that happen to agree.
#[test]
fn conform_run_takes_a_bare_name() {
    let dir = fixture("conform", &[("hello.lu", HELLO)]);

    let bare = wolf_in(&dir, &["conform-run", "hello.lu", "--checked", "--json"]);
    assert_eq!(
        code(&bare),
        0,
        "bare name refused by conform-run: {}",
        stderr(&bare)
    );

    let dotted = wolf_in(&dir, &["conform-run", "./hello.lu", "--checked", "--json"]);
    assert_eq!(code(&dotted), 0, "./ spelling refused: {}", stderr(&dotted));

    assert_eq!(
        String::from_utf8_lossy(&bare.stdout),
        String::from_utf8_lossy(&dotted.stdout),
        "`hello.lu` and `./hello.lu` must produce the same record"
    );
    let record = String::from_utf8_lossy(&bare.stdout);
    assert!(
        record.contains("\"verdict\":\"exit(0)\""),
        "the program did not run: {record}"
    );
    assert!(
        record.contains("hello, wolf"),
        "the program's stdout is missing: {record}"
    );
}

/// The entry is still the ENTRY (D59) when it is spelled bare: a file
/// carrying a standalone-entry header opts every OTHER file out of the
/// module, and the named entry always joins its own. Anchoring the
/// root without anchoring the entry would have broken exactly this —
/// the loader compares the entry against the paths `read_dir` hands
/// back, and every corpus file has such a header.
#[test]
fn a_bare_headered_entry_is_still_the_entry() {
    let dir = fixture(
        "headered",
        &[(
            "case.lu",
            "//! check: run(exit=0, stdout=\"hello, wolf\")\n\
             //! phase: run\n\n\
             fn main() -> !int {\n    print(\"hello, wolf\")\n    0\n}\n",
        )],
    );
    let out = wolf_in(&dir, &["conform-run", "case.lu", "--checked", "--json"]);
    assert_eq!(
        code(&out),
        0,
        "a headered entry spelled bare was excluded from its own module: {}",
        stderr(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("\"verdict\":\"exit(0)\""),
        "record: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// A bare entry still forms its directory module: the sibling's
/// function is in scope with no `use` (the s124 rule), so the
/// anchoring did not turn the package into a single file.
#[test]
fn a_bare_entry_still_forms_its_module() {
    let dir = fixture(
        "sibling",
        &[
            ("a.lu", "fn main() -> !int {\n    helper()\n}\n"),
            ("b.lu", "fn helper() -> int {\n    42\n}\n"),
        ],
    );
    let out = wolf_in(&dir, &["build", "a.lu", "--emit=wir", "-o", "out.wir"]);
    assert_eq!(
        code(&out),
        0,
        "bare entry lost its sibling module: {}",
        stderr(&out)
    );
}

/// The verbs that used to disagree. `build`/`run`/`fmt` share the CLI
/// parser that carried the old one-place anchor and always worked;
/// `test`, `interface` and `doc` did not, and now do. `run` needs a
/// linker, so it stays with the linking half below.
#[test]
fn every_verb_agrees_about_a_bare_name() {
    let dir = fixture("verbs", &[("hello.lu", HELLO)]);
    let cases: &[&[&str]] = &[
        &["fmt", "--check", "hello.lu"],
        &["build", "hello.lu", "--emit=wir", "-o", "out.wir"],
        &["test", "hello.lu"],
        &["interface", "hello.lu"],
        &["doc", "hello.lu"],
        &["conform-run", "hello.lu", "--checked", "--json"],
    ];
    for args in cases {
        let out = wolf_in(&dir, args);
        assert_eq!(
            code(&out),
            0,
            "`wolf {}` refused a bare entry name: {}",
            args.join(" "),
            stderr(&out)
        );
    }
}

/// `wolf run hello.lu` — the learner's line, end to end, on a host
/// that can link. The native rung is the one a windows learner takes
/// after s60a, so it is worth executing rather than just lowering.
/// Where the environment cannot link, this SKIPS loudly by the
/// refusal's own words rather than reading an absent toolchain as a
/// bare-name regression (s59's rule).
#[test]
fn run_takes_a_bare_name() {
    ensure_rt_staticlib();
    let dir = fixture("run", &[("hello.lu", HELLO)]);
    let out = wolf_in(&dir, &["run", "hello.lu"]);
    if code(&out) != 0 {
        let msg = stderr(&out);
        assert!(
            msg.contains("not found") || msg.contains("cannot compile this yet"),
            "bare `wolf run` failed for a non-environment reason:\n{msg}"
        );
        eprintln!("SKIP: this environment cannot link: {}", msg.trim());
        return;
    }
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hello, wolf\n");
}

/// The unit of the fix, without a process: the empty parent reads as
/// `.`, everything else is untouched, and an absolute path keeps its
/// own root.
#[test]
fn anchor_entry_reads_an_empty_parent_as_dot() {
    use wolf_sema::anchor_entry;
    assert_eq!(
        anchor_entry(Path::new("hello.lu")),
        Path::new(".").join("hello.lu")
    );
    assert_eq!(
        anchor_entry(Path::new("./hello.lu")),
        PathBuf::from("./hello.lu")
    );
    assert_eq!(
        anchor_entry(Path::new("sub/hello.lu")),
        PathBuf::from("sub/hello.lu")
    );
    let abs = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hello.lu");
    assert_eq!(anchor_entry(&abs), abs);
    // The anchored path names the same directory the loader will read.
    assert_eq!(
        anchor_entry(Path::new("hello.lu")).parent(),
        Some(Path::new("."))
    );
}
