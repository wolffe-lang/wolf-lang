//! Bounded filesystem inventory, native/release and the real checked machine.
//! A compile error is a red, not a skip. Filesystem observations are independent
//! of the Wolf wrappers; temporary HOME and cwd belong to each test.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(row: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join("inventory_fs").join(row);
    if p.exists() { std::fs::remove_dir_all(&p).unwrap(); }
    std::fs::create_dir_all(p.join("home")).unwrap();
    p
}

fn command(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_wolf"));
    c.current_dir(dir).env("HOME", dir.join("home")).env("SHELL", "/bin/sh");
    c
}

fn require(row: &str, o: &Output) {
    if !o.status.success() {
        eprintln!("FAIL {row}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    }
    assert!(o.status.success(), "FAIL {row}");
}

fn run(row: &str, dir: &Path, body: &str) {
    let entry = dir.join("main.lu");
    std::fs::write(&entry, format!("fn name(s: str) -> List[byte] {{\n    var b = List[byte]()\n    for x in s.bytes() {{ (mut b).push(x) }}\n    b\n}}\nfn main() -> !int {{\n{body}\n    0\n}}\n")).unwrap();
    let mut checked = command(dir);
    let o = checked.args(["conform-run", "--json", "--checked"]).arg(&entry).output().unwrap();
    require(row, &o);
    let text = String::from_utf8_lossy(&o.stdout);
    let rec: serde_json::Value = text.lines().rev().find_map(|l| serde_json::from_str(l).ok()).expect("JSON observation");
    assert_eq!(rec["verdict"], "exit(0)", "FAIL {row}: checked {rec}");
    for release in [false, true] {
        let exe = dir.join(if release { "inventory-release" } else { "inventory-native" });
        let mut c = command(dir);
        c.arg("build").arg(&entry).arg("--no-cache").arg("-o").arg(&exe);
        if release { c.arg("--release"); }
        let o = c.output().unwrap();
        require(row, &o);
        let o = Command::new(&exe).current_dir(dir).env("HOME", dir.join("home"))
            .env("SHELL", "/bin/sh").output().unwrap();
        require(row, &o);
    }
    eprintln!("PASS {row} checked native release");
}

#[cfg(unix)]
#[test]
fn fs_directory_entry_and_byte_limit_one_past() {
    let row = "fs_directory_entry_and_byte_limit_one_past";
    let d = scratch(row);
    std::fs::create_dir(d.join("source")).unwrap();
    std::fs::write(d.join("source/a"), b"x").unwrap();
    std::fs::write(d.join("source/b"), b"y").unwrap();
    run(row, &d, r#"
    let root = fs_open_mode("source", 48)?
    let c = fs_dir_cursor_open(root, 1, 8, 8, 8)?
    let a = fs_dir_cursor_next(c)?
    if a.len != 2 || a[0] != 0 as byte { return 11 }
    var hit = false
    let b = fs_dir_cursor_next(c) else |e| {
        match e { budget => { hit = true }, _ => {} }
        List[byte]()
    }
    if !hit || b.len != 0 { return 12 }
    fs_dir_cursor_close(c)?
    let small = fs_dir_cursor_open(root, 8, 0, 8, 8)?
    var bytes_hit = false
    let z = fs_dir_cursor_next(small) else |e| {
        match e { budget => { bytes_hit = true }, _ => {} }
        List[byte]()
    }
    if !bytes_hit || z.len != 0 { return 13 }
    fs_dir_cursor_close(small)?
    fs_close(root)?
"#);
}

#[cfg(unix)]
#[test]
fn fs_cursor_independent_offsets_and_raw_names() {
    use std::os::unix::ffi::OsStringExt as _;
    let row = "fs_cursor_independent_offsets_and_raw_names";
    let d = scratch(row);
    std::fs::create_dir(d.join("source")).unwrap();
    std::fs::write(d.join("source").join(std::ffi::OsString::from_vec(vec![255])), b"raw").unwrap();
    run(row, &d, r#"
    let root = fs_open_mode("source", 48)?
    let a = fs_dir_cursor_open(root, 1, 1, 1, 8)?
    let b = fs_dir_cursor_open(root, 1, 1, 1, 8)?
    let x = fs_dir_cursor_next(a)?
    let y = fs_dir_cursor_next(b)?
    if x.len != 2 || y.len != 2 || x[1] != 255 as byte || y[1] != 255 as byte { return 21 }
    var eof_seen = false
    let end = fs_dir_cursor_next(a) else |e| {
        match e { eof => { eof_seen = true }, _ => {} }
        List[byte]()
    }
    if !eof_seen || end.len != 0 { return 22 }
    fs_dir_cursor_close(a)?
    fs_dir_cursor_close(b)?
    fs_close(root)?
"#);
}

#[cfg(unix)]
#[test]
fn fs_descriptor_parent_replacement_metadata_truth() {
    let row = "fs_descriptor_parent_replacement_metadata_truth";
    let d = scratch(row);
    std::fs::create_dir(d.join("source")).unwrap();
    std::fs::create_dir(d.join("outside")).unwrap();
    std::fs::write(d.join("source/f"), b"inside").unwrap();
    std::fs::write(d.join("outside/f"), b"outside").unwrap();
    run(row, &d, r#"
    let root = fs_open_mode("source", 48)?
    let n = name("f")
    let before = fs_lstat_component(root, n)?
    fs_rename("source", "moved")?
    fs_symlink("outside", "source")?
    let f = fs_open_component(root, n, 0, before[9], before[10])?
    let text = fs_read(f, 8)?
    if text != "inside" { return 31 }
    fs_close(f)?
    var bad = false
    let escaped = fs_lstat_component(root, name("../outside/f")) else |e| {
        match e { invalid => { bad = true }, _ => {} }
        List[int]()
    }
    if !bad || escaped.len != 0 { return 32 }
    fs_close(root)?
"#);
}

#[cfg(unix)]
#[test]
fn fs_readlink_max_plus_one() {
    use std::os::unix::fs::symlink;
    let row = "fs_readlink_max_plus_one";
    let d = scratch(row);
    std::fs::create_dir(d.join("source")).unwrap();
    symlink("abcd", d.join("source/link")).unwrap();
    run(row, &d, r#"
    let root = fs_open_mode("source", 48)?
    let n = name("link")
    let s = fs_lstat_component(root, n)?
    let exact = fs_read_link_component(root, n, 4, s[9], s[10])?
    if exact.len != 4 || exact[3] != 100 as byte { return 41 }
    var bounded = false
    let over = fs_read_link_component(root, n, 3, s[9], s[10]) else |e| {
        match e { budget => { bounded = true }, _ => {} }
        List[byte]()
    }
    if !bounded || over.len != 0 { return 42 }
    fs_close(root)?
"#);
}

#[cfg(unix)]
#[test]
fn fs_existing_info_metadata_truth() {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    let row = "fs_existing_info_metadata_truth";
    let d = scratch(row);
    let p = d.join("truth");
    std::fs::write(&p, b"truth").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o640)).unwrap();
    let f = std::fs::File::options().write(true).open(&p).unwrap();
    f.set_times(std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH
        + std::time::Duration::new(1_700_000_000, 123_456_789))).unwrap();
    let m = std::fs::metadata(&p).unwrap();
    run(row, &d, &format!(r#"
    let s = fs_lstat("truth")?
    if s[0] != 0 || s[1] != 5 || s[4] != 416 || s[6] != {uid} || s[7] != {gid} {{ return 51 }}
    if s[9] != {dev} || s[10] != {ino} || s[14] != {sec} || s[15] != {ns} {{ return 52 }}
    let f = fs_open("truth")?
    let h = fs_fstat_record(f)?
    if h[9] != s[9] || h[10] != s[10] || h[15] != s[15] {{ return 53 }}
    fs_close(f)?
"#, uid=m.uid(), gid=m.gid(), dev=m.dev() as i64, ino=m.ino() as i64, sec=m.mtime(), ns=m.mtime_nsec()));
}
