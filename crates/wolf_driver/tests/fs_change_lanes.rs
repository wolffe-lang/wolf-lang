//! s230 (wolf-lang#346, #698, #699, #700, #702, #704) — the calls that
//! change a file without writing its bytes, and the umask, on every
//! machine. `[os.fs.mode]`, `[os.fs.create.mode]`, `[os.fs.symlink]`,
//! `[os.fs.link]`, `[os.fs.times]`, `[os.fs.truncate]`, `[os.fs.umask]`.
//!
//! Before this lane no machine resolved a name for any of them (E0301 on
//! the checked machine, native and release; "does not resolve" on
//! lupin), so `chmod`, `touch`, `ln`, `mkdir -m` and `cp -p` could not
//! be written (bu21's census). Each witness runs on the checked machine
//! and lupin under `conform-run` and as native and release binaries, and
//! every machine's standard output is printed, so a `--nocapture` log is
//! the four-machine table:
//!
//! - the six corpus rows, on every host (windows included): the shape
//!   rows, a hard link, a length, the times, and the mode, moded
//!   creation, symbolic link and mask with `unsupported` asserted on its
//!   side in the program;
//! - the fixtures under `fixtures/fs_change/`, on unix: modes under a
//!   known mask, both kinds of link read back through s218's calls, a
//!   link's own times, the `denied` row of every path call, and the mask
//!   itself. After each, the test reads the host with std — a second
//!   oracle that does not go through any machine under test.
//!
//! **The umask.** It is one word per PROCESS. No test here sets it in
//! this process: `modes.lu` is started through `sh` with `umask 027`,
//! and `umask.lu` sets its own from inside the machine under test, which
//! is a child process each time — so nothing a test does to a mask can
//! reach a test running beside it, and no lock is needed. Every file a
//! test changes is under a scratch directory it made; none asks who it
//! runs as (`denied.lu` asks the host whether a locked directory binds
//! it, and asserts on both answers).
//!
//! lupin 0.1.50 (the 0.2.27 pairing) predated the mirror — it resolved
//! none of the eight names — and was pinned by version and release
//! commit as pre-mirror (s180's design). 0.1.51 (the 0.2.28 pairing, r33)
//! carries it, measured on the published archive, so the pin is dropped
//! and every lupin must carry the mirror:
//! every row as the compiler's, except the two it refuses BY NAME for
//! want of a host call it can make without `unsafe` (`os_umask`, and
//! `fs_set_times` on a link itself).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The lupin releases that predate the mirror, with the commit each
/// archive reports. Empty since the 0.2.28 pairing: 0.1.51 (r33) carries
/// wolf-interp's s230, measured; 0.1.50 (`32b970d`) did not.
const PRE_MIRROR_LUPIN: &[(&str, Option<&str>)] = &[];

fn pre_mirror(lupin: &Obs) -> bool {
    PRE_MIRROR_LUPIN
        .iter()
        .any(|(v, c)| *v == lupin.version && c.is_none_or(|c| lupin.commit.starts_with(c)))
}

/// The verdict a pre-mirror lupin gives a program calling a name it
/// does not resolve.
const PRE: (&str, &str) = ("unsupported", "");

/// What lupin (with the mirror) is expected to say of a witness.
#[derive(Clone, Copy)]
enum Lupin {
    /// Every row, as the compiler's machines.
    Mirrors,
    /// Refused by name: the verdict is `unsupported` and the reason
    /// names this call.
    Refuses(&'static str),
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    reason: String,
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
    let field = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: field("verdict"),
        stdout: field("stdout_inline"),
        reason: field("x-unsupported"),
        version: field("impl_version"),
        commit: field("commit"),
    }
}

/// Give the owner every bit on `p` and all under it, so a tree a
/// stopped run left locked (`denied.lu`, `umask.lu`) can be removed.
/// Links are not followed.
#[cfg(unix)]
fn unlock(p: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let Ok(md) = std::fs::symlink_metadata(p) else {
        return;
    };
    if !md.is_dir() {
        return;
    }
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700));
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            unlock(&e.path());
        }
    }
}

#[cfg(not(unix))]
fn unlock(_p: &Path) {}

/// A fresh working directory of the test's own, holding `target/`.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("fs_change_lanes")
        .join(test);
    if d.exists() {
        unlock(&d);
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(d.join("target")).expect("scratch dir");
    d
}

/// `program args…`, started under `mask` when one is given: through
/// `sh`, which sets its own mask and then becomes the program — the
/// mask is the child's from its first instruction and never this
/// process's.
fn command(program: &Path, mask: Option<&str>) -> Command {
    match mask {
        None => Command::new(program),
        Some(m) => {
            let mut c = Command::new("/bin/sh");
            c.arg("-c")
                .arg(format!("umask {m}; exec \"$@\""))
                .arg("sh")
                .arg(program);
            c
        }
    }
}

fn run_in(mut cmd: Command, dir: &Path) -> Output {
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.output().expect("spawn")
}

fn conform(entry: &Path, dir: &Path, flag: &str, mask: Option<&str>) -> Option<Obs> {
    let mut cmd = command(Path::new(wolf()), mask);
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

fn lupin_says(entry: &Path, dir: &Path, mask: Option<&str>) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of s230's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        return None;
    };
    let mut cmd = command(&lupin, mask);
    cmd.arg("conform-run").arg(entry).arg("--json");
    let out = run_in(cmd, dir);
    Some(parse_obs(&out.stdout, "lupin's observation"))
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fs_change")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn corpus(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(name);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// The stdout a corpus row's own header declares — the one truth the
/// file states for every host (`check: run(exit=0, stdout="…")`).
fn declared_stdout(entry: &Path) -> String {
    let text = std::fs::read_to_string(entry).expect("corpus row reads");
    let line = text
        .lines()
        .find(|l| l.starts_with("//! check: run("))
        .unwrap_or_else(|| panic!("{} declares a run", entry.display()));
    let start = line.find("stdout=\"").expect("a declared stdout") + "stdout=\"".len();
    let end = line.rfind("\")").expect("the directive closes");
    line[start..end].replace("\\n", "\n") + "\n"
}

/// `entry` on every machine in `dir`: the checked machine and lupin
/// under `conform-run`, the native and release binaries run directly,
/// each under `mask` when one is given. `check` judges each stdout;
/// `after` runs once each machine has finished, on what it left in
/// `dir`.
fn every_lane_runs(
    entry: &Path,
    dir: &Path,
    mask: Option<&str>,
    lupin: Lupin,
    check: &dyn Fn(&str, &str),
    after: &dyn Fn(&str),
) {
    let name = entry.file_name().unwrap().to_string_lossy().to_string();
    let checked = conform(entry, dir, "--checked", mask).expect("the checked lane always runs");
    eprintln!(
        "== checked on {name}: {}\n{}",
        checked.verdict, checked.stdout
    );
    assert_eq!(
        checked.verdict, "exit(0)",
        "the CHECKED lane on {name}: {checked:?}"
    );
    check(&checked.stdout, &format!("checked on {name}"));
    after(&format!("checked on {name}"));
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(entry, dir, release) else {
            continue;
        };
        let out = run_in(command(&exe, mask), dir);
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        eprintln!(
            "== {tier} on {name}: exit {:?}\n{stdout}",
            out.status.code()
        );
        assert_eq!(
            out.status.code(),
            Some(0),
            "the {tier} binary on {name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        check(&stdout, &format!("{tier} on {name}"));
        after(&format!("{tier} on {name}"));
    }
    if let Some(obs) = lupin_says(entry, dir, mask) {
        eprintln!(
            "== lupin {} ({}) on {name}: {} {}\n{}",
            obs.version, obs.commit, obs.verdict, obs.reason, obs.stdout
        );
        if pre_mirror(&obs) {
            assert_eq!(
                (obs.verdict.as_str(), obs.stdout.as_str()),
                PRE,
                "lupin {} at {} (pre-mirror, pinned by version) on {name}",
                obs.version,
                obs.commit
            );
            return;
        }
        match lupin {
            Lupin::Mirrors => {
                assert_eq!(
                    obs.verdict, "exit(0)",
                    "lupin {} on {name}: {obs:?}",
                    obs.version
                );
                check(&obs.stdout, &format!("lupin {} on {name}", obs.version));
                after(&format!("lupin {} on {name}", obs.version));
            }
            Lupin::Refuses(call) => {
                assert_eq!(
                    obs.verdict, "unsupported",
                    "lupin {} refuses {name} by name: {obs:?}",
                    obs.version
                );
                assert!(
                    obs.reason.contains(call),
                    "lupin {}'s refusal of {name} names `{call}`: {}",
                    obs.version,
                    obs.reason
                );
            }
        }
    }
}

/// A corpus row on every machine: each stdout is the one the row's own
/// header declares.
fn the_corpus_row_runs(row: &str, test: &str, lupin: Lupin) {
    let dir = scratch(test);
    let entry = corpus(row);
    let want = declared_stdout(&entry);
    every_lane_runs(
        &entry,
        &dir,
        None,
        lupin,
        &|got, what| assert_eq!(got, want, "{what} (s230)"),
        &|_| {},
    );
}

// ------------------------------------------------- every host --

/// The shape rows: `invalid` before the host on every machine and every
/// host, a forged handle `io`, no host number left behind, nothing made.
/// Red at trunk: E0301 (no name resolves).
#[test]
fn the_shape_rows_on_every_host() {
    the_corpus_row_runs("fs/change_rows.lu", "shape", Lupin::Mirrors);
}

/// A second name is the same file, the call never replaces, and a
/// missing source or parent is `not_found`.
#[test]
fn a_hard_link_on_every_host() {
    the_corpus_row_runs("fs/hard_link.lu", "hard_link", Lupin::Mirrors);
}

/// Shorter, longer with zeros, the offset unmoved; a closed handle and
/// a read-only one are `io`.
#[test]
fn a_length_on_every_host() {
    the_corpus_row_runs("fs/truncate.lu", "truncate", Lupin::Mirrors);
}

/// Both times, one kept, "now", a directory, before the epoch, a
/// missing path — read back through `fs_stat`.
#[test]
fn the_times_on_every_host() {
    the_corpus_row_runs("fs/set_times.lu", "set_times", Lupin::Mirrors);
}

/// A mode set, a mode at creation and a symbolic link: served and read
/// back, or `unsupported` with nothing made (windows) — one stdout.
#[test]
fn a_mode_and_a_symlink_on_every_host() {
    the_corpus_row_runs("fs/mode_symlink.lu", "mode_symlink", Lupin::Mirrors);
}

/// The mask swapped and put back, with masks that keep the owner's
/// bits; `unsupported` on windows. lupin has no host call for it and
/// refuses by name.
#[test]
fn the_mask_on_every_host() {
    the_corpus_row_runs("os/umask.lu", "umask_row", Lupin::Refuses("os_umask"));
}

// ------------------------------------------------------- unix --

#[cfg(unix)]
fn host_mode(p: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::metadata(p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .mode()
        & 0o7777
}

/// Modes under the mask 0o027 (the numbers are decimal: 416 is 0o640,
/// 488 0o750, 384 0o600, 448 0o700, 493 0o755, 2505 0o4711, 420 0o644,
/// 1000 0o1750). A plain creation is 0o666 or 0o777 less the mask; a
/// moded one its own bits less the mask; an existing file keeps its
/// mode; `fs_set_mode` sets exactly its bits, set-id and sticky
/// included, and follows a link. Then std's own metadata, read by the
/// test, says the same of what each machine left.
#[cfg(unix)]
#[test]
fn modes_under_a_known_mask() {
    let dir = scratch("modes");
    let want = "plain: file=416 dir=488\n\
open_perm: ok private=384 ok exe=488 existing ok kept=384\n\
open_perm rows: taken=exists errno=17 read_absent=not_found errno=2\n\
create_dir_mode: ok sub=448 ok wide=488 again=exists orphan=not_found\n\
set_mode: 384 493 2505 0 420 sticky_dir=1000\n\
through a link: target=384 link_unchanged=true\n\
set_mode rows: dangling=not_found errno=2 missing=not_found errno=2\n";
    every_lane_runs(
        &fixture("modes.lu"),
        &dir,
        Some("027"),
        Lupin::Mirrors,
        &|got, what| assert_eq!(got, want, "{what} (s230)"),
        &|what| {
            let m = dir.join("m");
            let host: Vec<(&str, u32)> = ["plain", "d", "private", "exe", "sub", "wide"]
                .into_iter()
                .map(|n| (n, host_mode(&m.join(n))))
                .collect();
            assert_eq!(
                host,
                [
                    ("plain", 0o600),
                    ("d", 0o1750),
                    ("private", 0o600),
                    ("exe", 0o750),
                    ("sub", 0o700),
                    ("wide", 0o750),
                ],
                "{what}: the host's own metadata for what the program left"
            );
        },
    );
}

/// Both kinds of link, made by the program. Then the test reads the
/// host: the links store their targets verbatim, `b` is `a`'s inode,
/// and `hl` is a link, not a file.
#[cfg(unix)]
#[test]
fn links_made_and_read_back() {
    use std::os::unix::fs::MetadataExt as _;
    let dir = scratch("links");
    let want = "symlink: ok target=a kind=3 size=1 reads=12345\n\
dangling: ok target=../no/such/place kind=3 exists=false\n\
to a directory: ok is_dir=true\n\
symlink rows: over_link=exists errno=17 over_dangling=exists over_file=exists orphan=not_found errno=2\n\
link: ok same_file=true nlink=2,2\n\
link of a link: ok kind=3 target=a file_nlink=2\n\
link rows: taken=exists errno=17 missing=not_found errno=2 directory=denied errno=1\n";
    every_lane_runs(
        &fixture("links.lu"),
        &dir,
        None,
        Lupin::Mirrors,
        &|got, what| assert_eq!(got, want, "{what} (s230)"),
        &|what| {
            let l = dir.join("l");
            let target = |n: &str| std::fs::read_link(l.join(n)).unwrap();
            assert_eq!(target("ln"), Path::new("a"), "{what}");
            assert_eq!(target("dangling"), Path::new("../no/such/place"), "{what}");
            assert_eq!(
                target("hl"),
                Path::new("a"),
                "{what}: a link's link is a link"
            );
            let (a, b) = (
                std::fs::metadata(l.join("a")).unwrap(),
                std::fs::metadata(l.join("b")).unwrap(),
            );
            assert_eq!((a.dev(), a.ino()), (b.dev(), b.ino()), "{what}: one file");
            assert_eq!(a.nlink(), 2, "{what}: two names");
            assert!(
                !l.join("dd").exists(),
                "{what}: no second name for a directory"
            );
        },
    );
}

/// A link's own times against its target's. lupin refuses the first
/// call by name: it has no `utimensat` without `unsafe`.
#[cfg(unix)]
#[test]
fn a_links_own_times() {
    use std::os::unix::fs::MetadataExt as _;
    let dir = scratch("times_link");
    let want = "no follow: link atime=1000000000.1 mtime=1000000000.2 target mtime=800000000\n\
follow: target mtime=900000000.3 atime=800000000 link mtime=1000000000.2\n\
dangling: itself=ok mtime=700000000 followed=not_found errno=2\n";
    every_lane_runs(
        &fixture("times_link.lu"),
        &dir,
        None,
        Lupin::Refuses("fs_set_times"),
        &|got, what| assert_eq!(got, want, "{what} (s230)"),
        &|what| {
            let k = dir.join("k");
            let l = std::fs::symlink_metadata(k.join("ln")).unwrap();
            let a = std::fs::metadata(k.join("a")).unwrap();
            // The link's access time is not asked: following the link
            // (the program's second call does) is an access, and the
            // host may stamp it.
            assert_eq!(
                (l.mtime(), l.mtime_nsec()),
                (1_000_000_000, 2),
                "{what}: the link's own modification time"
            );
            assert_eq!(
                (a.atime(), a.mtime(), a.mtime_nsec()),
                (800_000_000, 900_000_000, 3),
                "{what}: the target's"
            );
        },
    );
}

/// The `denied` row of each of the six path calls, inside a directory
/// with no permission bits — where the host says that binds this
/// process; where it does not, each call is served. One stdout either
/// way; standard error (in the checked record's absence, the binaries')
/// says which was measured.
#[cfg(unix)]
#[test]
fn denied_rows_inside_a_locked_directory() {
    let dir = scratch("denied");
    let want = "denied rows hold: true; nothing made where it binds: true\n";
    every_lane_runs(
        &fixture("denied.lu"),
        &dir,
        None,
        Lupin::Mirrors,
        &|got, what| assert_eq!(got, want, "{what} (s230)"),
        &|what| {
            assert!(
                !dir.join("x").exists(),
                "{what}: the program unlocked and removed its tree"
            );
        },
    );
}

/// The mask set by the program (decimal again: 23 is 0o027, 63 0o077,
/// 191 0o277, 320 0o500): every creation after it loses its bits, a
/// child inherits it, and a parent made 0o500 is repaired with
/// `fs_set_mode` so a directory can be made inside it (bu21's
/// `mkdir -p` witness, wolf-lang#704). Each machine is a process of its
/// own, so the mask never reaches this one — asserted: the test's own
/// creation afterwards has the mode this process's mask always gave.
#[cfg(unix)]
#[test]
fn the_mask_a_program_sets() {
    let dir = scratch("umask");
    let probe = |n: &str| {
        let p = dir.join(n);
        std::fs::write(&p, b"").unwrap();
        host_mode(&p)
    };
    let before = probe("probe-before");
    let want = "under 0o027: file=416 dir=488\n\
under 0o077: replaced=23 file=384 dir=448\n\
a child inherits it: exit=0 umask=0077\n\
under 0o277: replaced=63 parent=320 repaired child=320\n\
restored: replaced=191\n";
    every_lane_runs(
        &fixture("umask.lu"),
        &dir,
        None,
        Lupin::Refuses("os_umask"),
        &|got, what| assert_eq!(got, want, "{what} (s230)"),
        &|what| {
            let u = dir.join("u");
            assert_eq!(host_mode(&u.join("f")), 0o640, "{what}");
            assert_eq!(host_mode(&u.join("g")), 0o600, "{what}");
            assert_eq!(host_mode(&u.join("e")), 0o700, "{what}");
            assert_eq!(host_mode(&u.join("p")), 0o700, "{what}");
            assert_eq!(host_mode(&u.join("p/q")), 0o500, "{what}");
            assert_eq!(
                probe("probe-after"),
                before,
                "{what}: the program's mask never reached the test process"
            );
        },
    );
}
