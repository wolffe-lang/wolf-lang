//! s38 — the io/fs builtin tier under checked execution: stderr
//! writers, the stdin `read_line`, and the `fs_*` family (THE Phase-B
//! unblock — wolf-std's stdc02 builds `std.fs`/`std.io` over exactly
//! this surface). Errors are D30 payload rows, never traps: a missing
//! file is `not_found`, a text-decode failure is `utf8`, an exhausted
//! stdin is `eof` — each one handleable with `else`/`match`, each one
//! deterministic.
//!
//! Filesystem tests run against real files under a per-test temp
//! directory (the checked machine performs REAL host operations; only
//! the comptime sandbox refuses them, D33).

use wolf_mem::ubcheck::{self, Budget, Verdict};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

/// Statically clean ladder, then checked execution with a stdin
/// buffer. Panics on refusal.
fn run_with_input(src: &str, stdin: &str) -> ubcheck::RunOutcome {
    let mut ml = MemoryLoader::new("iofs");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "input resolves clean: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        tc.not_yet.is_empty(),
        "input typechecks fully: {:?}",
        tc.not_yet
    );
    assert!(
        !tc.has_errors(),
        "input typechecks clean: {:?}",
        tc.diagnostics
    );
    let mem = wolf_mem::check_package(&res.package, &tc);
    assert!(
        mem.not_yet.is_empty(),
        "input stays inside the mem surface: {:?}",
        mem.not_yet
    );
    ubcheck::run_checked_with_input(&res.package, &tc, Budget::default(), stdin)
        .expect("the program is within the executable surface")
}

fn run(src: &str) -> ubcheck::RunOutcome {
    run_with_input(src, "")
}

fn assert_stdout(src: &str, expected: &str) {
    let out = run(src);
    match out.verdict {
        Verdict::Exit(0) => {}
        other => panic!("expected exit(0), got {other:?} (stdout: {:?})", out.stdout),
    }
    assert_eq!(out.stdout, expected, "stdout");
}

/// A fresh per-test scratch directory (std::env::temp_dir is the
/// platform-honest location; nothing lands in the repo).
fn scratch(test: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("wolf-s38-iofs-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

// -------------------------------------------------------------- io v0 --

#[test]
fn eprint_writes_the_stderr_channel() {
    let out = run("fn main() -> !int {\n\
         let code = 3\n\
         eprint(\"warn: code {code}\")\n\
         eprint_raw(\"no newline\")\n\
         print(\"stdout untouched\")\n\
         0\n\
         }\n");
    assert!(matches!(out.verdict, Verdict::Exit(0)));
    assert_eq!(out.stderr, "warn: code 3\nno newline");
    assert_eq!(out.stdout, "stdout untouched\n");
}

#[test]
fn eprint_honors_format_specs() {
    // The fmt machinery is channel-agnostic: one rendering, two
    // streams.
    let out = run("fn main() -> !int {\n\
         let n = 42\n\
         eprint(\"[{n:>6}]\")\n\
         0\n\
         }\n");
    assert_eq!(out.stderr, "[    42]\n");
}

#[test]
fn read_line_consumes_lines_then_eof() {
    let out = run_with_input(
        "fn main() -> !int {\n\
         let a = read_line() else |_| \"<eof>\"\n\
         let b = read_line() else |_| \"<eof>\"\n\
         let c = read_line() else |_| \"<eof>\"\n\
         print(\"{a}|{b}|{c}\")\n\
         0\n\
         }\n",
        "wolf\npack\n",
    );
    assert_eq!(out.stdout, "wolf|pack|<eof>\n");
}

#[test]
fn read_line_default_stdin_is_empty() {
    // Conform-run supplies no stdin: `read_line` is deterministic
    // `eof`, never a hang.
    assert_stdout(
        "fn main() -> !int {\n\
         let line = read_line() else |_| \"nothing\"\n\
         print(\"{line}\")\n\
         0\n\
         }\n",
        "nothing\n",
    );
}

// -------------------------------------------------------------- fs v0 --

#[test]
fn fs_write_read_roundtrip() {
    let dir = scratch("roundtrip");
    let path = dir.join("note.txt");
    let src = format!(
        "fn main() -> !int {{\n\
         let path = \"{p}\"\n\
         fs_write_text(path, \"three wolves\\n\")?\n\
         let text = fs_read_text(path)?\n\
         print(\"read: {{text.trim()}}\")\n\
         fs_remove(path)?\n\
         if fs_exists(path) {{ 1 }} else {{ 0 }}\n\
         }}\n",
        p = path.display()
    );
    let out = run(&src);
    assert!(
        matches!(out.verdict, Verdict::Exit(0)),
        "verdict: {:?} stdout: {:?}",
        out.verdict,
        out.stdout
    );
    assert_eq!(out.stdout, "read: three wolves\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fs_missing_file_takes_the_else_path() {
    let dir = scratch("missing");
    let path = dir.join("no-such-file.txt");
    let src = format!(
        "fn main() -> !int {{\n\
         let text = fs_read_text(\"{p}\") else |_| \"fallback\"\n\
         print(\"{{text}}\")\n\
         0\n\
         }}\n",
        p = path.display()
    );
    // The error is a VALUE the caller handles — not a trap (D30).
    assert_eq!(run(&src).stdout, "fallback\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fs_error_row_propagates_out_of_main() {
    let dir = scratch("propagate");
    let path = dir.join("absent.txt");
    let src = format!(
        "fn main() -> !int {{\n\
         let text = fs_read_text(\"{p}\")?\n\
         print(\"{{text}}\")\n\
         0\n\
         }}\n",
        p = path.display()
    );
    let out = run(&src);
    // D30 process behavior: the tag on stdout, exit 1.
    assert!(matches!(out.verdict, Verdict::Exit(1)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "error: not_found\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fs_handles_open_write_read_close() {
    let dir = scratch("handles");
    let path = dir.join("log.txt");
    let src = format!(
        "fn main() -> !int {{\n\
         let path = \"{p}\"\n\
         let out = fs_create(path)?\n\
         fs_write(out, \"alpha\")?\n\
         fs_write(out, \" beta\")?\n\
         fs_close(out)?\n\
         let f = fs_open(path)?\n\
         let text = fs_read(f, 64)?\n\
         fs_close(f)?\n\
         print(\"{{text}}\")\n\
         fs_remove(path)?\n\
         0\n\
         }}\n",
        p = path.display()
    );
    let out = run(&src);
    assert!(
        matches!(out.verdict, Verdict::Exit(0)),
        "verdict: {:?} stdout: {:?}",
        out.verdict,
        out.stdout
    );
    assert_eq!(out.stdout, "alpha beta\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fs_read_at_end_is_the_eof_row() {
    let dir = scratch("eof");
    let path = dir.join("tiny.txt");
    let src = format!(
        "fn main() -> !int {{\n\
         let path = \"{p}\"\n\
         fs_write_text(path, \"x\")?\n\
         let f = fs_open(path)?\n\
         let first = fs_read(f, 8)?\n\
         print(\"{{first}}\")\n\
         let second = fs_read(f, 8)?\n\
         print(\"{{second}}\")\n\
         0\n\
         }}\n",
        p = path.display()
    );
    let out = run(&src);
    // The first read drains the file; the second raises `eof`, which
    // propagates out of `main` as the D30 process outcome.
    assert!(matches!(out.verdict, Verdict::Exit(1)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "x\nerror: eof\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fs_read_after_close_is_the_io_row() {
    let dir = scratch("closed");
    let path = dir.join("tiny.txt");
    let src = format!(
        "fn main() -> !int {{\n\
         let path = \"{p}\"\n\
         fs_write_text(path, \"x\")?\n\
         let f = fs_open(path)?\n\
         fs_close(f)?\n\
         let text = fs_read(f, 8) else |_| \"closed-fd handled\"\n\
         print(\"{{text}}\")\n\
         0\n\
         }}\n",
        p = path.display()
    );
    // A forged or closed fd is the `io` row — checkable, never a trap.
    assert_eq!(run(&src).stdout, "closed-fd handled\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fs_exists_is_total() {
    let dir = scratch("exists");
    let path = dir.join("here.txt");
    std::fs::write(&path, b"present").expect("fixture");
    let src = format!(
        "fn main() -> !int {{\n\
         let yes = fs_exists(\"{p}\")\n\
         let no = fs_exists(\"{q}\")\n\
         if yes && !no {{ 0 }} else {{ 1 }}\n\
         }}\n",
        p = path.display(),
        q = dir.join("gone.txt").display()
    );
    assert!(matches!(run(&src).verdict, Verdict::Exit(0)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fs_utf8_failure_is_the_utf8_row() {
    let dir = scratch("utf8");
    let path = dir.join("bin.dat");
    std::fs::write(&path, [0xff, 0xfe, 0x00]).expect("fixture");
    let src = format!(
        "fn main() -> !int {{\n\
         let text = fs_read_text(\"{p}\")?\n\
         print(\"{{text}}\")\n\
         0\n\
         }}\n",
        p = path.display()
    );
    let out = run(&src);
    assert!(matches!(out.verdict, Verdict::Exit(1)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "error: utf8\n");
    let _ = std::fs::remove_dir_all(&dir);
}
