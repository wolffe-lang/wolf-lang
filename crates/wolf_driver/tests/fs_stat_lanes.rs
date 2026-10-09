//! s218 (wolf-lang#625, #626, #536) — the full stat record, `lstat`,
//! `readlink` and the unsorted typed listing, on every machine.
//! `[os.fs.stat]`, `[os.fs.readlink]`, `[os.fs.readdir]`.
//!
//! Before this lane every path call followed a symbolic link and
//! nothing read the link itself; the whole stat surface was `[kind,
//! size, modified_ms]`; and `fs_read_dir` sorted, failed on one
//! non-UTF-8 name and gave no type — so `ls -l`, `-F`, `-t`, `-S`, `-R`,
//! `-i` and `-U` could not be written (bu18). The witnesses are the
//! fixtures under `fixtures/fs_stat/`, run on the checked machine and
//! lupin under `conform-run` and as native and release binaries:
//!
//! - `portable.lu` on every host (windows included): the words every
//!   host answers, on files the program makes;
//! - `links.lu`, `record.lu`, `entries.lu` on unix, over a tree this
//!   test makes first (links need the host's `symlink`; the language has
//!   none): a link against its target, a dangling link, readlink, every
//!   word of the record against `stat(1)` (linux) and the host's own
//!   metadata, a modification time set to the nanosecond read back
//!   exactly, the listing in the host's order with kinds, and on linux a
//!   name that is not UTF-8 listed whole.
//!
//! lupin 0.1.48 and 0.1.49 (the pairing since v0.2.26) predate the mirror
//! and resolve none of the four names: its answers are pinned by version and release commit
//! as pre-mirror (s180's design), never widened.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The lupin releases that predate the mirror, with the commit each
/// release archive reports: 0.1.48 (r30, `531bf05`) and 0.1.49 (r31, the
/// pairing since v0.2.26, `f516a5f`); wolf-interp's s218 is unmerged at
/// both.
const PRE_MIRROR_LUPIN: &[(&str, Option<&str>)] =
    &[("0.1.48", Some("531bf05")), ("0.1.49", Some("f516a5f"))];

fn pre_mirror(lupin: &Obs) -> bool {
    PRE_MIRROR_LUPIN
        .iter()
        .any(|(v, c)| *v == lupin.version && c.is_none_or(|c| lupin.commit.starts_with(c)))
}

/// The verdict a pre-mirror lupin gives a program calling a name it
/// does not resolve (measured at trunk; `red-trunk-*.log`).
const PRE: (&str, &str) = ("unsupported", "");

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
    commit: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let text = String::from_utf8_lossy(bytes).replace('\r', "");
    let rec: serde_json::Value = text
        .lines()
        .rev()
        .find_map(|l| {
            l.find('{')
                .and_then(|i| serde_json::from_str(l[i..].trim()).ok())
        })
        .unwrap_or_else(|| panic!("{what} record parses: {text}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        commit: rec["commit"].as_str().unwrap_or("").to_string(),
    }
}

/// A fresh working directory of the test's own, holding `target/`.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("fs_stat_lanes")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(d.join("target")).expect("scratch dir");
    d
}

fn run_in(mut cmd: Command, dir: &Path) -> Output {
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.output().expect("spawn")
}

fn conform(entry: &Path, dir: &Path, flag: &str) -> Option<Obs> {
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run").arg(entry).arg(flag).arg("--json");
    let out = run_in(cmd, dir);
    if flag != "--checked" && lane_exit::environment_refusal(&out, &format!("wolf {flag}")) {
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, "the observation"))
}

/// The native or release binary of `entry`. A compile error FAILS the
/// gate (it is the defect at a pre-fix head, never an environment).
fn build(entry: &Path, dir: &Path, release: bool) -> Option<PathBuf> {
    let exe = dir.join(format!(
        "a-{}{}",
        if release { "release" } else { "native" },
        std::env::consts::EXE_SUFFIX
    ));
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(entry)
        .arg("-o")
        .arg(&exe)
        .arg("--no-cache");
    if release {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf build runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("error["),
        "wolf build {} refused the program (a compile error is the defect, never a skip):\n{stderr}",
        entry.display()
    );
    if lane_exit::environment_refusal(&out, "wolf build") {
        eprintln!(
            "SKIP: environment cannot build the {} tier: {}",
            if release { "release" } else { "native" },
            stderr.trim()
        );
        return None;
    }
    assert!(out.status.success(), "wolf build failed: {stderr}");
    Some(exe)
}

/// The sibling lupin, found exactly as `pairing.rs` finds it.
fn sibling_lupin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LUPIN") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while dir.pop() {
        let candidate = dir.join("../wolf-interp/target/release/lupin");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn lupin_says(entry: &Path, dir: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of s218's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        return None;
    };
    let mut cmd = Command::new(&lupin);
    cmd.arg("conform-run").arg(entry).arg("--json");
    let out = run_in(cmd, dir);
    Some(parse_obs(&out.stdout, "lupin's observation"))
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fs_stat")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// `name` on every machine in `dir`: the checked machine and lupin under
/// `conform-run`, the native and release binaries run directly; `check`
/// judges each stdout (computed after the run, so an oracle that reads
/// the host sees what the program saw).
fn every_lane_runs(name: &str, dir: &Path, check: &dyn Fn(&str, &str)) {
    let entry = fixture(name);
    let checked = conform(&entry, dir, "--checked").expect("the checked lane always runs");
    assert_eq!(
        checked.verdict, "exit(0)",
        "the CHECKED lane on {name}: {checked:?}"
    );
    check(&checked.stdout, &format!("checked on {name}"));
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, dir, release) else {
            continue;
        };
        let out = run_in(Command::new(&exe), dir);
        assert_eq!(
            out.status.code(),
            Some(0),
            "the {tier} binary on {name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        check(
            &String::from_utf8_lossy(&out.stdout),
            &format!("{tier} on {name}"),
        );
    }
    if let Some(lupin) = lupin_says(&entry, dir) {
        if pre_mirror(&lupin) {
            assert_eq!(
                (lupin.verdict.as_str(), lupin.stdout.as_str()),
                PRE,
                "lupin {} at {} (pre-mirror, pinned by version) on {name}",
                lupin.version,
                lupin.commit
            );
        } else {
            assert_eq!(
                lupin.verdict, "exit(0)",
                "lupin {} on {name}: {lupin:?}",
                lupin.version
            );
            check(&lupin.stdout, &format!("lupin {} on {name}", lupin.version));
        }
    }
}

/// The unix tree the link fixtures read: `t/a` (5 bytes, its mtime and
/// atime set to 2020-01-01T05:00:00.123456789Z), `t/d/`, `t/ln -> a`,
/// `t/lnd -> d`, `t/dangling -> nope`, `t/zz`, `t/B`, created in that
/// order (which is not byte order); on linux `u/ok` and `u/bad\xff`.
#[cfg(unix)]
fn link_tree(test: &str) -> PathBuf {
    use std::os::unix::fs::symlink;
    let dir = scratch(test);
    let t = dir.join("t");
    std::fs::create_dir_all(t.join("d")).unwrap();
    std::fs::write(t.join("a"), b"12345").unwrap();
    symlink("a", t.join("ln")).unwrap();
    symlink("d", t.join("lnd")).unwrap();
    symlink("nope", t.join("dangling")).unwrap();
    std::fs::write(t.join("zz"), b"").unwrap();
    std::fs::write(t.join("B"), b"").unwrap();
    let when = std::time::UNIX_EPOCH + std::time::Duration::new(1_577_854_800, 123_456_789);
    let f = std::fs::File::options()
        .write(true)
        .open(t.join("a"))
        .unwrap();
    f.set_times(
        std::fs::FileTimes::new()
            .set_modified(when)
            .set_accessed(when),
    )
    .unwrap();
    drop(f);
    if cfg!(target_os = "linux") {
        use std::os::unix::ffi::OsStrExt as _;
        let u = dir.join("u");
        std::fs::create_dir_all(&u).unwrap();
        std::fs::write(u.join("ok"), b"").unwrap();
        std::fs::write(u.join(std::ffi::OsStr::from_bytes(b"bad\xff")), b"").unwrap();
    }
    dir
}

// ------------------------------------------------- every host --

/// The words every host answers, on files the program makes: kind, size
/// and modified_ms agree with `fs_fstat` and the path calls, stat and
/// lstat agree on a file, words 0..3 are answered, a directory is kind 1,
/// readlink of a file is `invalid`, the listing has both entries with
/// their kinds. Red at trunk: E0301 (the four names do not resolve);
/// lupin 0.1.48 `unsupported`.
#[test]
fn the_portable_words_on_every_host() {
    let dir = scratch("portable");
    let want = "words=20 kind=0 size=6 as_fstat=true\n\
as_paths=true stat_is_lstat=true have0123=true\ndir_kind=1\n\
read_link(file)=invalid read_link(missing)=not_found stat(missing)=not_found\n\
entries=2 file_kind0=true sub_kind1=true\ncleaned=true\n";
    every_lane_runs("portable.lu", &dir, &|got, what| {
        assert_eq!(got, want, "{what} (s218)");
    });
}

// ------------------------------------------------------- unix --

/// A link against its target: `fs_lstat` answers the link (kind 3, its
/// size the target's length), `fs_stat` the target (a file of 5, a
/// directory), a dangling link is `not_found` to `fs_stat` only, readlink
/// hands back `a`, `d`, `nope`, and is `invalid` on a file; `fs_stat` of
/// the link has the target's device and inode, `fs_lstat` its own.
#[cfg(unix)]
#[test]
fn a_link_against_its_target() {
    let dir = link_tree("links");
    let want = "a: lstat kind=0 size=5 | stat kind=0 size=5 | link invalid\n\
ln: lstat kind=3 size=1 | stat kind=0 size=5 | link a\n\
lnd: lstat kind=3 size=1 | stat kind=1 | link d\n\
dangling: lstat kind=3 size=4 | stat not_found | link nope\n\
stat(ln) is a: true; lstat(ln) is not a: true\n\
missing: lstat not_found | link not_found; a file: link invalid\n";
    every_lane_runs("links.lu", &dir, &|got, what| {
        assert_eq!(got, want, "{what} (s218)");
    });
}

/// The words of the host's own record for `p` (lstat when `link`), as
/// the fixture prints them, from std's metadata — the second oracle.
#[cfg(unix)]
fn host_words(p: &Path, link: bool) -> Vec<i64> {
    use std::os::unix::fs::MetadataExt as _;
    let md = if link {
        std::fs::symlink_metadata(p)
    } else {
        std::fs::metadata(p)
    }
    .unwrap();
    let kind = if md.file_type().is_symlink() { 3 } else { 0 };
    let ms = md.mtime() * 1000 + md.mtime_nsec() / 1_000_000;
    vec![
        kind,
        md.len() as i64,
        ms,
        md.mode() as i64 & 0o7777,
        md.nlink() as i64,
        i64::from(md.uid()),
        i64::from(md.gid()),
        md.blocks() as i64,
        md.dev() as i64,
        md.ino() as i64,
        md.rdev() as i64,
        md.atime(),
        md.atime_nsec(),
        md.mtime(),
        md.mtime_nsec(),
        md.ctime(),
        md.ctime_nsec(),
    ]
}

/// `stat(1)` on linux: `%a %h %u %g %s %b %d %i %X %Y %Z` and the
/// fraction of `%x` and `%y` — mode (octal), links, uid, gid, size,
/// blocks, dev, ino, atime/mtime/ctime seconds, atime/mtime ns.
#[cfg(target_os = "linux")]
fn stat1(p: &Path, link: bool) -> Vec<i64> {
    let run = |fmt: &str| -> String {
        let mut c = Command::new("stat");
        if !link {
            c.arg("-L");
        }
        let out = c.arg("-c").arg(fmt).arg(p).output().expect("stat(1) runs");
        assert!(out.status.success(), "stat(1) on {}", p.display());
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    let mut v: Vec<i64> = run("%a %h %u %g %s %b %d %i %X %Y %Z")
        .split_whitespace()
        .enumerate()
        .map(|(i, w)| {
            if i == 0 {
                i64::from_str_radix(w, 8).unwrap()
            } else {
                w.parse().unwrap()
            }
        })
        .collect();
    for t in run("%x|%y").split('|') {
        // `2020-01-01 00:00:00.123456789 -0500`: the fraction.
        let frac = t.split('.').nth(1).unwrap().split(' ').next().unwrap();
        v.push(frac.parse().unwrap());
    }
    v
}

/// Every word of the record against the host: `stat(1)` on linux, std's
/// metadata everywhere. `t/a`'s mtime was set to the nanosecond and reads
/// back exactly (1577854800, 123456789); every unix word is answered
/// (`have` bits 0..17).
#[cfg(unix)]
#[test]
fn the_record_against_stat1() {
    let dir = link_tree("record");
    every_lane_runs("record.lu", &dir, &|got, what| {
        let lines: Vec<Vec<i64>> = got
            .lines()
            .map(|l| {
                l.split_whitespace()
                    .skip(1)
                    .map(|w| w.parse().unwrap())
                    .collect()
            })
            .collect();
        assert_eq!(lines.len(), 2, "{what}: two lines: {got}");
        for (rec, (path, link)) in lines.iter().zip([("t/a", false), ("t/ln", true)]) {
            assert_eq!(rec.len(), 20, "{what}: 20 words for {path}");
            let have = rec[3];
            assert_eq!(
                have & 0x3ffff,
                0x3ffff,
                "{what}: unix answers words 0..17 of {path}"
            );
            let mut mine = vec![rec[0], rec[1], rec[2]];
            mine.extend_from_slice(&rec[4..18]);
            let host = host_words(&dir.join(path), link);
            assert_eq!(mine, host, "{what}: {path} against the host's metadata");
            #[cfg(target_os = "linux")]
            {
                let s = stat1(&dir.join(path), link);
                // mode nlink uid gid size blocks dev ino atime mtime ctime, atime ns, mtime ns
                let want = vec![
                    rec[4], rec[5], rec[6], rec[7], rec[1], rec[8], rec[9], rec[10], rec[12],
                    rec[14], rec[16], rec[13], rec[15],
                ];
                assert_eq!(s, want, "{what}: {path} against stat(1)");
            }
        }
        assert_eq!(
            (lines[0][14], lines[0][15], lines[0][2]),
            (1_577_854_800, 123_456_789, 1_577_854_800_123),
            "{what}: the nanosecond mtime reads back exactly"
        );
        assert_eq!(lines[1][0], 3, "{what}: lstat of the link is kind 3");
    });
}

/// The listing in the host's own order, each entry with its own kind,
/// against std's `read_dir` of the same directory (the same host call,
/// the same order); on linux `u/` holds a non-UTF-8 name, listed whole
/// where `fs_read_dir` answers `utf8`.
#[cfg(unix)]
#[test]
fn the_listing_in_the_hosts_order() {
    use std::os::unix::ffi::OsStrExt as _;
    let dir = link_tree("entries");
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    let listing = |sub: &str| -> String {
        let rows: Vec<String> = std::fs::read_dir(dir.join(sub))
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                let ft = e.file_type().unwrap();
                let k = if ft.is_symlink() {
                    3
                } else if ft.is_dir() {
                    1
                } else {
                    0
                };
                format!("{k} {}", hex(e.file_name().as_bytes()))
            })
            .collect();
        format!("{sub}: {} entries\n{}\n", rows.len(), rows.join("\n"))
    };
    let mut want = listing("t");
    if cfg!(target_os = "linux") {
        want += &listing("u");
        want += "fs_read_dir(u): utf8\n";
    }
    eprintln!("the host's order:\n{want}");
    every_lane_runs("entries.lu", &dir, &|got, what| {
        assert_eq!(got, want, "{what} (s218)");
    });
}
