//! The bounded inventory calls (wolf-std#156): a directory cursor that
//! admits each entry against the caller's limits before it copies a name,
//! no-follow metadata and opens of one validated byte component at a held
//! directory, and a link read with an explicit maximum. `[os.fs.cursor]`,
//! `[os.fs.component]`.
//!
//! Each row's program runs on the checked machine under `conform-run` and as
//! native and release binaries; every machine's stdout is held to the row's
//! `want`, and every machine's answer is printed, so a `--nocapture` log is
//! the table. A wrong answer prints `FAIL <row> …` on a line of its own
//! before the test fails, so a planted break is red BY NAME in a record.
//!
//! The memory row (linux) reads each native program's own peak resident
//! size (`VmHWM`, per address space, so the test process's size never
//! reaches it) and holds the cursor's admission to a flat peak where the
//! whole-listing call grows with the directory.
//!
//! Filesystem facts are set and observed by this file through std, never
//! through the Wolf calls under test. Temporary HOME and cwd belong to each
//! row.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A wrong answer, BY NAME, at column 0, then the test fails.
fn fail(row: &str, why: &str) -> ! {
    eprintln!("FAIL {row} {why}");
    panic!("FAIL {row} {why}");
}

fn scratch(row: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("inventory_fs")
        .join(row);
    if p.exists() {
        std::fs::remove_dir_all(&p).unwrap();
    }
    std::fs::create_dir_all(p.join("home")).unwrap();
    p
}

fn command(program: &Path, dir: &Path) -> Command {
    let mut c = Command::new(program);
    c.current_dir(dir)
        .env("HOME", dir.join("home"))
        .env("SHELL", "/bin/sh");
    c
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace('\r', "")
}

/// The helpers every row's program shares: each call's answer as a word, a
/// row's tag by name.
const HELPERS: &str = r#"
fn name(s: str) -> List[byte] {
    var b = List[byte]()
    for x in s.bytes() {
        (mut b).push(x)
    }
    b
}

fn repeat(x: byte, n: int) -> List[byte] {
    var b = List[byte]()
    var i = 0
    while i < n {
        (mut b).push(x)
        i = i + 1
    }
    b
}

fn cursor(dir: int, entries: int, bytes: int, longest: int, scan: int) -> int {
    fs_dir_cursor_open(dir, entries, bytes, longest, scan) else |e| {
        return match e {
            budget => -1,
            denied => -2,
            invalid => -3,
            io => -4,
            not_found => -5,
            unsupported => -6,
        }
    }
}

fn next_row(c: int) -> str {
    let r = fs_dir_cursor_next(c) else |e| {
        return match e {
            budget => "budget",
            eof => "eof",
            io => "io",
        }
    }
    let n = str_from_utf8(r[1..]) else "?"
    "{r[0] as int}:{n}"
}

fn close_row(c: int) -> str {
    fs_dir_cursor_close(c) else |e| {
        return match e {
            io => "io",
        }
    }
    "ok"
}

fn lstat_tag(dir: int, n: List[byte]) -> str {
    let r = fs_lstat_component(dir, n) else |e| {
        return match e {
            denied => "denied",
            invalid => "invalid",
            io => "io",
            not_found => "not_found",
            unsupported => "unsupported",
        }
    }
    "kind{r[0]}"
}

fn open_tag(dir: int, n: List[byte], open: int, dev: int, ino: int) -> str {
    let fd = fs_open_component(dir, n, open, dev, ino) else |e| {
        return match e {
            changed => "changed",
            denied => "denied",
            invalid => "invalid",
            io => "io",
            not_found => "not_found",
            unsupported => "unsupported",
        }
    }
    fs_close(fd) else |_| {}
    "ok"
}

fn link_tag(dir: int, n: List[byte], max: int, dev: int, ino: int) -> str {
    let t = fs_read_link_component(dir, n, max, dev, ino) else |e| {
        return match e {
            budget => "budget",
            changed => "changed",
            denied => "denied",
            invalid => "invalid",
            io => "io",
            not_found => "not_found",
            unsupported => "unsupported",
        }
    }
    let s = str_from_utf8(t) else "?"
    "{t.len}:{s}"
}
"#;

/// The row's program: the helpers (when the row calls the new names), then
/// `main` around `body`.
fn program(dir: &Path, stem: &str, helpers: bool, body: &str) -> PathBuf {
    let entry = dir.join(format!("{stem}.lu"));
    let helpers = if helpers { HELPERS } else { "" };
    std::fs::write(
        &entry,
        format!("{helpers}\nfn main() -> !int {{\n{body}\n    0\n}}\n"),
    )
    .unwrap();
    entry
}

/// A native or release binary of `entry`, or the row's failure by name (a
/// compile error is a red, never a skip).
fn build(row: &str, entry: &Path, dir: &Path, stem: &str, release: bool) -> PathBuf {
    let tier = if release { "release" } else { "native" };
    let exe = dir.join(format!("{stem}-{tier}"));
    let mut c = command(Path::new(env!("CARGO_BIN_EXE_wolf")), dir);
    c.arg("build").arg(entry).arg("--no-cache").arg("-o").arg(&exe);
    if release {
        c.arg("--release");
    }
    let o = c.output().unwrap();
    if !o.status.success() {
        fail(
            row,
            &format!(
                "build ({tier}) exit {:?}\nstdout={}\nstderr={}",
                o.status.code(),
                text(&o.stdout),
                text(&o.stderr)
            ),
        );
    }
    exe
}

/// The checked machine's stdout for `entry`, held to exit 0.
fn checked(row: &str, entry: &Path, dir: &Path) -> String {
    let o = command(Path::new(env!("CARGO_BIN_EXE_wolf")), dir)
        .args(["conform-run", "--json", "--checked"])
        .arg(entry)
        .output()
        .unwrap();
    let out = text(&o.stdout);
    let rec: Option<serde_json::Value> = out.lines().rev().find_map(|l| {
        l.find('{')
            .and_then(|i| serde_json::from_str(l[i..].trim()).ok())
    });
    let Some(rec) = rec else {
        fail(
            row,
            &format!("checked: no record\nstdout={out}\nstderr={}", text(&o.stderr)),
        );
    };
    let verdict = rec["verdict"].as_str().unwrap_or("").to_string();
    let stdout = rec["stdout_inline"].as_str().unwrap_or("").to_string();
    eprintln!("== checked {row}: {verdict}\n{stdout}");
    if verdict != "exit(0)" {
        fail(
            row,
            &format!("checked verdict {verdict}: {rec}\nstderr={}", text(&o.stderr)),
        );
    }
    stdout
}

fn binary(row: &str, exe: &Path, dir: &Path, tier: &str) -> Output {
    let o = command(exe, dir).output().unwrap();
    eprintln!(
        "== {tier} {row}: exit {:?}\n{}{}",
        o.status.code(),
        text(&o.stdout),
        text(&o.stderr)
    );
    if o.status.code() != Some(0) {
        fail(row, &format!("{tier} exit {:?}", o.status.code()));
    }
    o
}

/// `body` on every machine in `dir`, each stdout held to `want`; `setup`
/// runs before each machine, so a row that changes the tree starts over.
fn every_machine(row: &str, dir: &Path, body: &str, want: &str, setup: &dyn Fn(&Path)) {
    every_machine_with(row, dir, true, body, want, setup);
}

fn every_machine_with(
    row: &str,
    dir: &Path,
    helpers: bool,
    body: &str,
    want: &str,
    setup: &dyn Fn(&Path),
) {
    let entry = program(dir, "main", helpers, body);
    let native = build(row, &entry, dir, "inventory", false);
    let release = build(row, &entry, dir, "inventory", true);
    setup(dir);
    let got = checked(row, &entry, dir);
    if got != want {
        fail(row, &format!("checked stdout:\n{got}\nwant:\n{want}"));
    }
    for (tier, exe) in [("native", &native), ("release", &release)] {
        setup(dir);
        let got = text(&binary(row, exe, dir, tier).stdout);
        if got != want {
            fail(row, &format!("{tier} stdout:\n{got}\nwant:\n{want}"));
        }
    }
    eprintln!("PASS {row} checked native release");
}

/// A fresh `source` directory holding `files` (name, bytes).
fn fresh(dir: &Path, files: &[(&[u8], &[u8])]) {
    use std::os::unix::ffi::OsStrExt as _;
    let s = dir.join("source");
    if s.exists() {
        std::fs::remove_dir_all(&s).unwrap();
    }
    std::fs::create_dir(&s).unwrap();
    for (name, bytes) in files {
        std::fs::write(s.join(std::ffi::OsStr::from_bytes(name)), bytes).unwrap();
    }
}

fn mkfifo(p: &Path) {
    use std::os::unix::ffi::OsStrExt as _;
    let c = std::ffi::CString::new(p.as_os_str().as_bytes()).unwrap();
    // SAFETY: a NUL-terminated path this test owns.
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0, "mkfifo {p:?}");
}

/// One past each limit refuses as `budget` and consumes the record; exactly
/// at a limit admits; the scan limit is sticky; `eof` only at the end.
#[test]
fn fs_directory_entry_and_byte_limit_one_past() {
    let row = "fs_directory_entry_and_byte_limit_one_past";
    let d = scratch(row);
    let body = r#"
    let root = fs_open_mode("source", 48)?
    let c = cursor(root, 1, 8, 8, 8)
    let first = next_row(c)
    print("entries: {first == "0:a" || first == "0:b"} {next_row(c)} {next_row(c)} {close_row(c)}")
    let x = cursor(root, 2, 2, 1, 8)
    let x1 = next_row(x)
    let x2 = next_row(x)
    print("at_limit: {(x1 == "0:a" && x2 == "0:b") || (x1 == "0:b" && x2 == "0:a")} {next_row(x)} {close_row(x)}")
    let y = cursor(root, 2, 1, 1, 8)
    let y1 = next_row(y)
    print("bytes: {y1 == "0:a" || y1 == "0:b"} {next_row(y)} {next_row(y)} {close_row(y)}")
    let w = cursor(root, 8, 8, 0, 8)
    print("longest: {next_row(w)} {next_row(w)} {next_row(w)} {close_row(w)}")
    let s = cursor(root, 8, 8, 8, 0)
    print("scan: {next_row(s)} {next_row(s)} {close_row(s)}")
    let z = cursor(root, 0, 8, 8, 8)
    print("none: {next_row(z)} {next_row(z)} {next_row(z)} {close_row(z)}")
    fs_close(root)?
"#;
    let want = "entries: true budget eof ok\n\
                at_limit: true eof ok\n\
                bytes: true budget eof ok\n\
                longest: budget budget eof ok\n\
                scan: budget budget ok\n\
                none: budget budget eof ok\n";
    every_machine(row, &d, body, want, &|d| fresh(d, &[(b"a", b"x"), (b"b", b"y")]));
}

/// Two cursors on one held directory read independently; a name that is
/// not UTF-8 arrives as its bytes; a closed, a forged and a twice-closed
/// cursor are `io`.
#[test]
fn fs_cursor_independent_offsets_and_raw_names() {
    let row = "fs_cursor_independent_offsets_and_raw_names";
    let d = scratch(row);
    let body = r#"
    let root = fs_open_mode("source", 48)?
    let a = cursor(root, 1, 1, 1, 8)
    let b = cursor(root, 1, 1, 1, 8)
    let x = fs_dir_cursor_next(a)?
    let y = fs_dir_cursor_next(b)?
    print("a: {x.len} {x[0] as int} {x[1] as int}")
    print("b: {y.len} {y[0] as int} {y[1] as int}")
    print("ends: {next_row(a)} {next_row(b)}")
    print("closed: {close_row(a)} {next_row(a)} {close_row(a)}")
    print("forged: {next_row(999999)} {close_row(999999)} {next_row(0)}")
    print("other: {next_row(b)} {close_row(b)}")
    fs_close(root)?
"#;
    let want = "a: 2 0 255\n\
                b: 2 0 255\n\
                ends: eof eof\n\
                closed: ok io io\n\
                forged: io io io\n\
                other: eof ok\n";
    every_machine(row, &d, body, want, &|d| fresh(d, &[(&[255], b"raw")]));
}

/// A limit below zero and a handle that is no directory are `invalid`, a
/// forged handle `io`; the sixty-fifth live cursor is `budget`, and closing
/// one makes room.
#[test]
fn fs_cursor_shape_refusals_and_live_ceiling() {
    let row = "fs_cursor_shape_refusals_and_live_ceiling";
    let d = scratch(row);
    let body = r#"
    let root = fs_open_mode("source", 48)?
    let f = fs_open("source/a")?
    print("negative: {cursor(root, -1, 8, 8, 8)} {cursor(root, 8, -1, 8, 8)} {cursor(root, 8, 8, -1, 8)} {cursor(root, 8, 8, 8, -1)}")
    print("file: {cursor(f, 8, 8, 8, 8)} forged: {cursor(999999, 8, 8, 8, 8)}")
    var held = List[int]()
    var refused = 0
    var i = 0
    while i < 65 {
        let c = cursor(root, 1, 1, 1, 1)
        if c > 0 {
            (mut held).push(c)
        } else {
            refused = c
        }
        i = i + 1
    }
    print("ceiling: {held.len} {refused}")
    let first = held[0]
    print("room: {close_row(first)} {cursor(root, 1, 1, 1, 1) > 0}")
    fs_close(f)?
    fs_close(root)?
"#;
    let want = "negative: -3 -3 -3 -3\n\
                file: -3 forged: -4\n\
                ceiling: 64 -1\n\
                room: ok true\n";
    every_machine(row, &d, body, want, &|d| fresh(d, &[(b"a", b"x")]));
}

/// The held directory answers for its own entries after its path is moved
/// and replaced by a link to another tree: no read leaves the root. Every
/// name that is not one component is `invalid` before the host; identity
/// that differs is `changed`; a link and a fifo are never opened.
#[test]
fn fs_descriptor_parent_replacement_metadata_truth() {
    let row = "fs_descriptor_parent_replacement_metadata_truth";
    let d = scratch(row);
    let body = r#"
    let root = fs_open_mode("source", 48)?
    let n = name("f")
    let before = fs_lstat_component(root, n)?
    let ld = fs_lstat_component(root, name("l"))?
    let pd = fs_lstat_component(root, name("p"))?
    let dd = fs_lstat_component(root, name("d"))?
    fs_rename("source", "moved")?
    fs_symlink("outside", "source")?
    let f = fs_open_component(root, n, 0, before[9], before[10])?
    let got = fs_read(f, 16)?
    print("read: {got}")
    fs_close(f)?
    var nul = name("a")
    (mut nul).push(0 as byte)
    print("shapes: {lstat_tag(root, name(""))} {lstat_tag(root, name("."))} {lstat_tag(root, name(".."))} {lstat_tag(root, name("../outside/f"))} {lstat_tag(root, name("d/x"))} {lstat_tag(root, nul)} {lstat_tag(root, repeat(120 as byte, 256))}")
    print("kinds: {lstat_tag(root, n)} {lstat_tag(root, name("l"))} {lstat_tag(root, name("p"))} {lstat_tag(root, name("d"))} {lstat_tag(root, repeat(120 as byte, 255))}")
    print("changed: {open_tag(root, n, 0, before[9], before[10] + 1)} {open_tag(root, n, 0, before[9] + 1, before[10])}")
    print("never: {open_tag(root, name("l"), 0, ld[9], ld[10])} {open_tag(root, name("p"), 0, pd[9], pd[10])}")
    print("dir: {open_tag(root, name("d"), 32, dd[9], dd[10])} {open_tag(root, name("d"), 48, dd[9], dd[10])}")
    print("words: {open_tag(root, n, 1, before[9], before[10])} {open_tag(root, n, 64, before[9], before[10])} {open_tag(root, n, 5, before[9], before[10])}")
    print("missing: {open_tag(root, name("zz"), 0, 1, 1)} {open_tag(999999, n, 0, before[9], before[10])}")
    fs_close(root)?
"#;
    let want = "read: inside\n\
                shapes: invalid invalid invalid invalid invalid invalid invalid\n\
                kinds: kind0 kind3 kind4 kind1 not_found\n\
                changed: changed changed\n\
                never: invalid invalid\n\
                dir: ok ok\n\
                words: invalid invalid ok\n\
                missing: not_found io\n";
    let setup = |d: &Path| {
        for p in ["source", "moved", "outside"] {
            let p = d.join(p);
            if p.is_symlink() {
                std::fs::remove_file(&p).unwrap();
            } else if p.exists() {
                std::fs::remove_dir_all(&p).unwrap();
            }
        }
        fresh(d, &[(b"f", b"inside")]);
        std::fs::create_dir(d.join("outside")).unwrap();
        std::fs::write(d.join("outside/f"), b"outside").unwrap();
        std::fs::create_dir(d.join("source/d")).unwrap();
        std::os::unix::fs::symlink("f", d.join("source/l")).unwrap();
        mkfifo(&d.join("source/p"));
    };
    every_machine(row, &d, body, want, &setup);
}

/// A target exactly at the maximum is answered whole; one byte past it is
/// `budget`, never a shorter target; a different identity is `changed`; a
/// name that is no link and a maximum outside the bound are `invalid`.
#[test]
fn fs_readlink_max_plus_one() {
    let row = "fs_readlink_max_plus_one";
    let d = scratch(row);
    let body = r#"
    let root = fs_open_mode("source", 48)?
    let n = name("link")
    let s = fs_lstat_component(root, n)?
    let r = fs_lstat_component(root, name("file"))?
    print("exact: {link_tag(root, n, 4, s[9], s[10])} {link_tag(root, n, 5, s[9], s[10])}")
    print("over: {link_tag(root, n, 3, s[9], s[10])} {link_tag(root, n, 0, s[9], s[10])}")
    print("changed: {link_tag(root, n, 4, s[9], s[10] + 1)}")
    print("shapes: {link_tag(root, name("file"), 8, r[9], r[10])} {link_tag(root, n, -1, s[9], s[10])} {link_tag(root, n, 65537, s[9], s[10])} {link_tag(root, name(".."), 8, s[9], s[10])}")
    print("missing: {link_tag(root, name("gone"), 8, s[9], s[10])}")
    fs_close(root)?
"#;
    let want = "exact: 4:abcd 4:abcd\n\
                over: budget budget\n\
                changed: changed\n\
                shapes: invalid invalid invalid invalid\n\
                missing: not_found\n";
    let setup = |d: &Path| {
        fresh(d, &[(b"file", b"regular")]);
        std::os::unix::fs::symlink("abcd", d.join("source/link")).unwrap();
    };
    every_machine(row, &d, body, want, &setup);
}

/// The cursor's kind is the held directory's no-follow answer for the
/// name, never the entry's type hint: a link to a directory is a link.
#[test]
fn fs_cursor_kinds_are_no_follow() {
    let row = "fs_cursor_kinds_are_no_follow";
    let d = scratch(row);
    let body = r#"
    let root = fs_open_mode("source", 48)?
    let c = cursor(root, 8, 64, 16, 16)
    var known = 0
    var other = 0
    var last = ""
    var i = 0
    while i < 8 {
        let r = next_row(c)
        if r == "eof" {
            last = r
            i = 8
        } else if r == "0:f" || r == "1:d" || r == "3:l" || r == "4:p" {
            known = known + 1
        } else {
            other = other + 1
        }
        i = i + 1
    }
    print("kinds: {known} {other} {last} {close_row(c)}")
    fs_close(root)?
"#;
    let want = "kinds: 4 0 eof ok\n";
    let setup = |d: &Path| {
        fresh(d, &[(b"f", b"x")]);
        std::fs::create_dir(d.join("source/d")).unwrap();
        std::os::unix::fs::symlink("d", d.join("source/l")).unwrap();
        mkfifo(&d.join("source/p"));
    };
    every_machine(row, &d, body, want, &setup);
}

/// The record the inventory reads (`fs_lstat`, `fs_fstat_record`) is the
/// host's, field by field: kind, size, mode, owner, identity and
/// nanosecond time. Already served at trunk; a regression row.
#[test]
fn fs_existing_info_metadata_truth() {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    let row = "fs_existing_info_metadata_truth";
    let d = scratch(row);
    let p = d.join("truth");
    std::fs::write(&p, b"truth").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o640)).unwrap();
    let f = std::fs::File::options().write(true).open(&p).unwrap();
    f.set_times(std::fs::FileTimes::new().set_modified(
        std::time::UNIX_EPOCH + std::time::Duration::new(1_700_000_000, 123_456_789),
    ))
    .unwrap();
    drop(f);
    let m = std::fs::metadata(&p).unwrap();
    let body = format!(
        r#"
    let s = fs_lstat("truth")?
    print("words: {{s[0] == 0}} {{s[1] == 5}} {{s[4] == 416}} {{s[6] == {uid}}} {{s[7] == {gid}}}")
    print("identity: {{s[9] == {dev}}} {{s[10] == {ino}}} {{s[14] == {sec}}} {{s[15] == {ns}}}")
    let f = fs_open("truth")?
    let h = fs_fstat_record(f)?
    print("held: {{h[9] == s[9]}} {{h[10] == s[10]}} {{h[15] == s[15]}}")
    fs_close(f)?
"#,
        uid = m.uid(),
        gid = m.gid(),
        dev = m.dev() as i64,
        ino = m.ino() as i64,
        sec = m.mtime(),
        ns = m.mtime_nsec()
    );
    let want = "words: true true true true true\n\
                identity: true true true true\n\
                held: true true true\n";
    every_machine_with(row, &d, false, &body, want, &|_| {});
}

/// `VmHWM` in KiB from a program's printed `/proc/self/status`.
#[cfg(target_os = "linux")]
fn peak_kib(row: &str, stdout: &str) -> u64 {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
        .unwrap_or_else(|| fail(row, &format!("no VmHWM in:\n{stdout}")))
}

/// A wide directory of `n` entries with 200-byte names under `dir/wide`.
#[cfg(target_os = "linux")]
fn wide(dir: &Path, n: usize) {
    let w = dir.join("wide");
    if w.exists() {
        std::fs::remove_dir_all(&w).unwrap();
    }
    std::fs::create_dir(&w).unwrap();
    for i in 0..n {
        let mut name = format!("{i:08}");
        name.push_str(&"w".repeat(192));
        std::fs::File::create(w.join(name)).unwrap();
    }
}

/// The cursor admits one entry and refuses the next before it copies a
/// name: its peak does not move between a narrow and a wide directory,
/// while the whole listing's grows with the directory (the fixture's
/// witness that the width is large enough to see).
#[cfg(target_os = "linux")]
#[test]
fn fs_directory_admission_before_allocation() {
    let row = "fs_directory_admission_before_allocation";
    let d = scratch(row);
    let status = r#"    let status = fs_read_text("/proc/self/status")?
    print(status)"#;
    let open = "    let root = fs_open_mode(\"wide\", 48)?";
    let programs = [
        ("base", false, format!("{open}\n{status}")),
        (
            "legacy",
            false,
            format!(
                "{open}\n    let all = fs_read_dir_entries_at(root)?\n    print(\"count: {{all.len}}\")\n{status}"
            ),
        ),
        (
            "admit",
            true,
            format!(
                "{open}\n    let c = cursor(root, 1, 255, 255, 8)\n    let one = next_row(c)\n    print(\"admit: {{one.len}} {{next_row(c)}}\")\n{status}"
            ),
        ),
    ];
    // The base and the whole listing first (served at trunk, so the
    // fixture's witness is measured before the cursor is built), then the
    // cursor's admission at the same two widths.
    let mut peaks = Vec::new();
    for (stem, helpers, body) in &programs {
        let entry = program(&d, stem, *helpers, body);
        let mut exes = Vec::new();
        for release in [false, true] {
            let tier = if release { "release" } else { "native" };
            exes.push((tier, build(row, &entry, &d, stem, release)));
        }
        for n in [2_000usize, 40_000] {
            wide(&d, n);
            for (tier, exe) in &exes {
                let out = text(&binary(row, exe, &d, tier).stdout);
                if *stem == "admit" && !out.contains("admit: 202 budget") {
                    fail(row, &format!("{tier} n={n}: {out}"));
                }
                if *stem == "legacy" && !out.contains(&format!("count: {n}")) {
                    fail(row, &format!("{tier} n={n}: {out}"));
                }
                let kib = peak_kib(row, &out);
                eprintln!("REPORT {row} {stem} {tier} n={n} VmHWM={kib} KiB");
                peaks.push((*stem, *tier, n, kib));
            }
        }
        if *stem == "legacy" {
            for tier in ["native", "release"] {
                let at = |n: usize| {
                    peaks
                        .iter()
                        .find(|p| p.0 == "legacy" && p.1 == tier && p.2 == n)
                        .map(|p| p.3)
                        .unwrap()
                };
                let grown = at(40_000).saturating_sub(at(2_000));
                eprintln!("REPORT {row} {tier}: the whole listing grew {grown} KiB from 2000 to 40000 entries");
                if grown < 4096 {
                    fail(
                        row,
                        &format!("{tier}: the whole listing grew only {grown} KiB: the fixture is too narrow to see"),
                    );
                }
            }
        }
    }
    let peak = |stem: &str, tier: &str, n: usize| {
        peaks
            .iter()
            .find(|p| p.0 == stem && p.1 == tier && p.2 == n)
            .map(|p| p.3)
            .unwrap()
    };
    for tier in ["native", "release"] {
        let grown = peak("legacy", tier, 40_000).saturating_sub(peak("legacy", tier, 2_000));
        let admit = peak("admit", tier, 40_000).saturating_sub(peak("admit", tier, 2_000));
        let over_base = peak("admit", tier, 40_000).saturating_sub(peak("base", tier, 40_000));
        if admit > 512 || over_base > 1024 {
            fail(
                row,
                &format!("{tier}: the cursor's peak moved {admit} KiB with the width, {over_base} KiB over the base"),
            );
        }
        eprintln!(
            "REPORT {row} {tier}: listing +{grown} KiB, cursor +{admit} KiB (+{over_base} over base)"
        );
    }
    let _ = std::fs::remove_dir_all(d.join("wide"));
    eprintln!("PASS {row} native release");
}
