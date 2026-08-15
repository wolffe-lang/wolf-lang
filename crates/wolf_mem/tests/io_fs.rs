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

/// A path spelled safely inside a wolf string literal: forward slashes
/// everywhere (Windows accepts them at the API; backslashes would read
/// as escape sequences — `C:\Users` began with "there is no \U
/// escape in wolf", seven times).
fn lit(p: &std::path::Path) -> String {
    p.display().to_string().replace('\\', "/")
}
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
        p = lit(&path)
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
        p = lit(&path)
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
        p = lit(&path)
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
        p = lit(&path)
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
        p = lit(&path)
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
        p = lit(&path)
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
        p = lit(&path),
        q = lit(&dir.join("gone.txt"))
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
        p = lit(&path)
    );
    let out = run(&src);
    assert!(matches!(out.verdict, Verdict::Exit(1)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "error: utf8\n");
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------ s90 (#51/#52): fs v1 --
//
// Bytes, directories, modes, metadata, rename. These run on EVERY
// tier-1 target (the checked machine is the portable lane), so the
// fixtures avoid anything platform-shaped: no permission changes, no
// device boundaries, no byte sequences in filenames. `lit` keeps
// windows paths spellable inside a wolf literal.

/// #52's complaint, answered: appending no longer reads the file.
/// The witness is a file whose existing byte no text reader can
/// decode — if the append had gone through `read_text` + concat, this
/// would raise `utf8` instead of appending.
#[test]
fn fs_append_mode_does_not_read_the_file() {
    let dir = scratch("append");
    let path = dir.join("log.bin");
    std::fs::write(&path, [0x80u8]).expect("fixture");
    let src = format!(
        "fn main() -> !int {{\n\
         let p = \"{p}\"\n\
         let fd = fs_open_mode(p, 2)?\n\
         fs_write(fd, \"one\\n\")?\n\
         fs_close(fd)?\n\
         let again = fs_open_mode(p, 2)?\n\
         fs_write(again, \"two\\n\")?\n\
         fs_close(again)?\n\
         print(\"size={{fs_size(p)?}}\")\n\
         0\n\
         }}\n",
        p = lit(&path)
    );
    let out = run(&src);
    assert!(matches!(out.verdict, Verdict::Exit(0)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "size=9\n");
    // Every original byte still there, both appends at the end.
    assert_eq!(std::fs::read(&path).expect("read back"), b"\x80one\ntwo\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The five modes, including the two rows only the moded open can
/// raise: `invalid` (a mode outside the set) and `exists` (mode 4
/// losing the exclusive-create race).
#[test]
fn fs_open_mode_covers_the_set_and_its_rows() {
    let dir = scratch("modes");
    let path = dir.join("m.txt");
    let src = format!(
        "fn main() -> !int {{\n\
         let p = \"{p}\"\n\
         let missing = fs_open_mode(p, 0) else |_| 0 - 1\n\
         let fresh = fs_open_mode(p, 4)?\n\
         fs_close(fresh)?\n\
         let raced = fs_open_mode(p, 4) else |_| 0 - 2\n\
         let w = fs_open_mode(p, 1)?\n\
         fs_write(w, \"abcd\")?\n\
         fs_close(w)?\n\
         let rw = fs_open_mode(p, 3)?\n\
         fs_close(rw)?\n\
         let bad = fs_open_mode(p, 99) else |_| 0 - 3\n\
         print(\"{{missing}} {{raced}} {{bad}} kept={{fs_read_text(p)?}}\")\n\
         0\n\
         }}\n",
        p = lit(&path)
    );
    let out = run(&src);
    assert!(matches!(out.verdict, Verdict::Exit(0)), "{:?}", out.verdict);
    // Mode 3 does NOT truncate: "abcd" survives it.
    assert_eq!(out.stdout, "-1 -2 -3 kept=abcd\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Each new row's TAG identity, one propagation per program (a `?`
/// out of `main` is the one place a tag's name reaches stdout).
#[test]
fn fs_new_rows_carry_their_tags() {
    let dir = scratch("tags");
    let existing = dir.join("already");
    std::fs::create_dir(&existing).expect("fixture");
    let file = dir.join("f.bin");
    std::fs::write(&file, b"x").expect("fixture");
    for (src, tag) in [
        // A mode outside the set never touches the filesystem.
        (
            format!("let fd = fs_open_mode(\"{p}\", 42)?\n", p = lit(&file)),
            "invalid",
        ),
        // A byte list holding something that is not a byte.
        (
            format!(
                "var b = List[int]()\n\
                 (mut b).push(300)\n\
                 fs_write_bytes(\"{p}\", b)?\n",
                p = lit(&file)
            ),
            "invalid",
        ),
        // Strict create over an existing directory.
        (
            format!("fs_create_dir(\"{p}\")?\n", p = lit(&existing)),
            "exists",
        ),
        // A missing parent, single-level.
        (
            format!(
                "fs_create_dir(\"{p}\")?\n",
                p = lit(&dir.join("nope/deeper"))
            ),
            "not_found",
        ),
        // A listing of something that is not there.
        (
            format!(
                "let ns = fs_read_dir(\"{p}\")?\n",
                p = lit(&dir.join("nope"))
            ),
            "not_found",
        ),
        // A move whose source is gone.
        (
            format!(
                "fs_rename(\"{a}\", \"{b}\")?\n",
                a = lit(&dir.join("nope")),
                b = lit(&dir.join("dest"))
            ),
            "not_found",
        ),
        // Size of nothing.
        (
            format!("let n = fs_size(\"{p}\")?\n", p = lit(&dir.join("nope"))),
            "not_found",
        ),
        // A non-empty directory is `io` — the platforms name the
        // errno differently and the response is the same either way.
        (format!("fs_remove_dir(\"{p}\")?\n", p = lit(&dir)), "io"),
        // The byte reader's end-of-file, over a handle.
        (
            format!(
                "let fd = fs_open_mode(\"{p}\", 0)?\n\
                 let first = fs_read_chunk(fd, 64)?\n\
                 let second = fs_read_chunk(fd, 64)?\n",
                p = lit(&file)
            ),
            "eof",
        ),
    ] {
        let program = format!("fn main() -> !int {{\n{src}0\n}}\n");
        let out = run(&program);
        assert!(
            matches!(out.verdict, Verdict::Exit(1)),
            "{tag}: {:?}",
            out.verdict
        );
        assert_eq!(out.stdout, format!("error: {tag}\n"), "tag for:\n{program}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// #51's byte level: a file with a lone `0x80` in it is the test.
#[test]
fn fs_byte_io_carries_what_text_io_refuses() {
    let dir = scratch("bytes");
    let path = dir.join("bin.dat");
    let copy = dir.join("copy.dat");
    let src = format!(
        "fn main() -> !int {{\n\
         let p = \"{p}\"\n\
         let q = \"{q}\"\n\
         var b = List[int]()\n\
         (mut b).push(128)\n\
         (mut b).push(0)\n\
         (mut b).push(255)\n\
         (mut b).push(65)\n\
         fs_write_bytes(p, b)?\n\
         let refused = fs_read_text(p) else |_| \"text refused\"\n\
         let back = fs_read_bytes(p)?\n\
         print(\"{{refused}} n={{back.len}} {{back[0]}} {{back[2]}}\")\n\
         fs_write_bytes(q, back)?\n\
         let fd = fs_open_mode(q, 0)?\n\
         let head = fs_read_chunk(fd, 2)?\n\
         let tail = fs_read_chunk(fd, 64)?\n\
         fs_close(fd)?\n\
         print(\"copy={{fs_size(q)?}} head={{head.len}} tail={{tail.len}} {{tail[1]}}\")\n\
         0\n\
         }}\n",
        p = lit(&path),
        q = lit(&copy)
    );
    let out = run(&src);
    assert!(matches!(out.verdict, Verdict::Exit(0)), "{:?}", out.verdict);
    assert_eq!(
        out.stdout,
        "text refused n=4 128 255\ncopy=4 head=2 tail=2 65\n"
    );
    // The copy is byte-identical — `copy_file` stops being a text op.
    assert_eq!(
        std::fs::read(&copy).expect("copy"),
        [0x80, 0x00, 0xff, 0x41]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The SORTING decision, asserted. The entries are created in an
/// order chosen to disagree with byte order, so a listing that merely
/// echoed the filesystem would have to be lucky to pass.
#[test]
fn fs_read_dir_is_sorted_and_lists_names() {
    let dir = scratch("readdir");
    for n in ["zebra.txt", "alpha.txt", "Mid.txt", "beta"] {
        std::fs::write(dir.join(n), b"x").expect("fixture");
    }
    std::fs::create_dir(dir.join("sub")).expect("fixture");
    let src = format!(
        "fn main() -> !int {{\n\
         let names = fs_read_dir(\"{p}\")?\n\
         for n in names {{\n\
         print(\"{{n}}\")\n\
         }}\n\
         print(\"n={{names.len}}\")\n\
         0\n\
         }}\n",
        p = lit(&dir)
    );
    let out = run(&src);
    assert!(matches!(out.verdict, Verdict::Exit(0)), "{:?}", out.verdict);
    // Byte order (uppercase first), names not paths, no `.`/`..`.
    assert_eq!(
        out.stdout,
        "Mid.txt\nalpha.txt\nbeta\nsub\nzebra.txt\nn=5\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fs_dirs_and_metadata_say_what_exists() {
    let dir = scratch("dirs");
    let src = format!(
        "fn main() -> !int {{\n\
         let root = \"{p}\"\n\
         let a = \"{p}/a\"\n\
         let b = \"{p}/a/b\"\n\
         let leaf = \"{p}/a/b/leaf.txt\"\n\
         let nope = \"{p}/nope\"\n\
         fs_create_dir_all(b)?\n\
         fs_create_dir_all(b)?\n\
         fs_write_text(leaf, \"12345\")?\n\
         print(\"dir={{fs_is_dir(a)}} file={{fs_is_file(a)}}\")\n\
         print(\"dir={{fs_is_dir(leaf)}} file={{fs_is_file(leaf)}}\")\n\
         print(\"gone_dir={{fs_is_dir(nope)}} gone_file={{fs_is_file(nope)}}\")\n\
         print(\"size={{fs_size(leaf)?}}\")\n\
         let m = fs_modified_ms(leaf)?\n\
         print(\"recent={{m > 1600000000000}}\")\n\
         fs_remove_dir_all(a)?\n\
         print(\"unmade={{!fs_exists(a)}} root={{fs_is_dir(root)}}\")\n\
         0\n\
         }}\n",
        p = lit(&dir)
    );
    let out = run(&src);
    assert!(matches!(out.verdict, Verdict::Exit(0)), "{:?}", out.verdict);
    assert_eq!(
        out.stdout,
        "dir=true file=false\n\
         dir=false file=true\n\
         gone_dir=false gone_file=false\n\
         size=5\n\
         recent=true\n\
         unmade=true root=true\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `fs_rename` moves the entry without reading it — which is what
/// makes `std.fs.move_file` stop being copy-then-remove. It does NOT
/// promise atomicity (see `wolf_rt::fs`), and no test here pretends
/// otherwise: what is asserted is the effect.
#[test]
fn fs_rename_moves_without_reading() {
    let dir = scratch("rename");
    let from = dir.join("from.bin");
    std::fs::write(&from, [0x80u8, 0xff]).expect("fixture");
    let to = dir.join("to.bin");
    let src = format!(
        "fn main() -> !int {{\n\
         let a = \"{a}\"\n\
         let b = \"{b}\"\n\
         fs_rename(a, b)?\n\
         print(\"moved={{fs_is_file(b)}} src={{fs_exists(a)}}\")\n\
         0\n\
         }}\n",
        a = lit(&from),
        b = lit(&to)
    );
    let out = run(&src);
    assert!(matches!(out.verdict, Verdict::Exit(0)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "moved=true src=false\n");
    // Bytes no text path could have carried survived the move.
    assert_eq!(std::fs::read(&to).expect("moved"), [0x80, 0xff]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// #69: the rig's own binary. The point is a path that can be
/// SPAWNED, so the assertion is that it names an existing file.
#[test]
fn os_exe_names_an_existing_file() {
    assert_stdout(
        "fn main() -> !int {\n\
         let exe = os_exe()?\n\
         print(\"file={fs_is_file(exe)} empty={exe.len == 0}\")\n\
         0\n\
         }\n",
        "file=true empty=false\n",
    );
}
