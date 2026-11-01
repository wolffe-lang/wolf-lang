//! s272 (wolf-lang#686, #921, #922, #976, #975) — the OS surface
//! boreutils and yew met: no record of a file system (`df`, `stat -f`),
//! no change of root (`chroot`), no scheduling priority (`nice`), no
//! `ONLCR` in the terminal's mode word (yew's REPL), and no read-only
//! query of one signal's action (`env --ignore-signal`).
//! `[os.fs.statfs]`, `[os.proc.chroot]`, `[os.proc.priority]`,
//! `[os.term.mode]`, `[os.signal.action.one]`.
//!
//! Each witness runs on the checked machine and lupin under `conform-run`
//! and as native and release binaries, and every machine's answer is
//! printed, so a `--nocapture` log is the table. Three kinds:
//!
//! - EVERY HOST (windows too): the corpus rows of the five calls
//!   (`corpus/fs/statfs_rows.lu`, `corpus/os/chroot_rows.lu`,
//!   `priority_rows.lu`, `signal_action_rows.lu`, `term_lf_rows.lu`), each
//!   held to its host's lines. A row that changes something (a nice value
//!   raised, a disposition set and set back) changes only the process the
//!   test started.
//! - MEASURED: `fs_statfs`'s words against GNU `stat -f` (linux: ext4,
//!   tmpfs, overlay, proc — whatever this host mounts) and `/bin/df -k`
//!   (macOS), the free counts bracketed by the oracle's read before and
//!   after the program's, in the same second; the flags word against the
//!   mount's options (`/proc/self/mountinfo`).
//! - LIVE: `ONLCR` on a pseudo-terminal the test opens, started on and
//!   off (unix); the change of root and the nice value in a throwaway
//!   `--network none` container (linux), once as a user and once as root
//!   inside it — s242's engine (`WOLF_JOBWAIT_ENGINE`, else docker then
//!   podman; `WOLF_JOBWAIT_IMAGE`, default ubuntu:24.04), and
//!   `WOLF_JOBWAIT_REQUIRE_CONTAINER=1` (CI's linux job) makes a missing
//!   engine a failure, never a skip. boreutils' `tools/roottest` is the
//!   shape (read, not moved).
//!
//! **Nothing here signals a process, and nothing changes a root, a nice
//! value or a disposition outside a process the test started.**
//!
//! **lupin 0.1.58**, the pairing, predates every new name: each is pinned
//! here by version, exactly as measured ("does not resolve"); a newer lupin
//! is held to the mirror (wolf-interp's issue, filed by this lane).

mod lane_exit;
mod test_cpus;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A `REPORT` line for the CI log even when the test passes.
fn report(line: &str) {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

/// The pairing at this lane, which predates the mirror of the five names.
const PRE_MIRROR_LUPIN: &[&str] = &["0.1.58"];

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    reason: String,
    version: String,
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
        reason: {
            let r = field("x-unsupported");
            if r.is_empty() {
                field("x-unsupported-construct")
            } else {
                r
            }
        },
        version: field("impl_version"),
    }
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/os_surface")
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

/// The stdout a corpus row's own header declares.
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

/// A fresh working directory of the test's own.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("os_surface_lanes")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

fn run_in(mut cmd: Command, dir: &Path) -> Output {
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.output().expect("spawn")
}

/// The native or release binary of `entry`, as `dir/<stem>-<tier>`. A
/// compile error FAILS the gate (it is the defect at a pre-fix head,
/// never an environment); an environment that cannot build the tier
/// skips loudly.
fn build(entry: &Path, dir: &Path, stem: &str, release: bool) -> Option<PathBuf> {
    let tier = if release { "release" } else { "native" };
    let exe = dir.join(format!("{stem}-{tier}{}", std::env::consts::EXE_SUFFIX));
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
            "SKIP: environment cannot build the {tier} tier: {}",
            stderr.trim()
        );
        return None;
    }
    assert!(out.status.success(), "wolf build failed: {stderr}");
    Some(exe)
}

/// The sibling lupin, found exactly as `pairing.rs` finds it, or a loud
/// skip — a failure under `WOLF_PAIRING_REQUIRE_SIBLING`.
fn lupin_or_skip() -> Option<PathBuf> {
    let found = if let Some(p) = std::env::var_os("LUPIN") {
        let p = PathBuf::from(p);
        p.is_file().then_some(p)
    } else {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut hit = None;
        while dir.pop() {
            let candidate = dir.join("../wolf-interp/target/release/lupin");
            if candidate.is_file() {
                hit = Some(candidate);
                break;
            }
        }
        hit
    };
    if found.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of s272's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    found
}

/// What lupin is held to for one witness.
enum Lupin<'a> {
    /// 0.1.58 stops at a name it does not resolve (one of these); a newer
    /// lupin mirrors the compiler.
    Unresolved(&'a [&'a str]),
    /// Every lupin refuses by name at one of these calls: the program
    /// meets a call lupin refuses by design (no `tcsetattr`, ruling #57 R8)
    /// before any new name.
    Refuses(&'a [&'a str]),
}

fn judge_lupin(obs: &Obs, name: &str, want: &str, lupin: &Lupin<'_>) {
    eprintln!(
        "== lupin {} on {name}: {} {}\n{}",
        obs.version, obs.verdict, obs.reason, obs.stdout
    );
    let pre = PRE_MIRROR_LUPIN.contains(&obs.version.as_str());
    let refuses = |calls: &[&str], unresolved: bool| {
        assert_eq!(
            obs.verdict, "unsupported",
            "lupin {} refuses {name} by name: {obs:?}",
            obs.version
        );
        assert!(
            calls.iter().any(|c| obs.reason.contains(&format!("`{c}`"))),
            "lupin {}'s refusal of {name} names one of {calls:?}: {}",
            obs.version,
            obs.reason
        );
        if unresolved {
            assert!(
                obs.reason.contains("does not resolve"),
                "lupin {} (pre-mirror) does not resolve {calls:?}: {}",
                obs.version,
                obs.reason
            );
        }
    };
    match *lupin {
        Lupin::Unresolved(calls) => {
            if pre {
                refuses(calls, true);
            } else {
                assert_eq!(
                    (obs.verdict.as_str(), obs.stdout.as_str()),
                    ("exit(0)", want),
                    "lupin {} on {name}: {obs:?}",
                    obs.version
                );
            }
        }
        Lupin::Refuses(calls) => refuses(calls, false),
    }
}

/// The checked machine's record of `entry`, run in `dir`.
fn checked_in(entry: &Path, dir: &Path) -> Obs {
    let name = entry.file_name().unwrap().to_string_lossy().to_string();
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--checked")
        .arg("--json");
    let out = run_in(cmd, dir);
    let obs = parse_obs(&out.stdout, "the observation");
    eprintln!(
        "== checked on {name}: {} {}\n{}{}",
        obs.verdict,
        obs.reason,
        obs.stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    obs
}

/// `entry` on every machine, ON THE HOST, in `dir`: the checked machine
/// and lupin under `conform-run`, the native and release binaries run
/// directly. Each stdout is held to `want`.
fn every_lane_answers_on_the_host(entry: &Path, dir: &Path, want: &str, lupin: Lupin<'_>) {
    let name = entry.file_name().unwrap().to_string_lossy().to_string();
    let checked = checked_in(entry, dir);
    assert_eq!(
        (checked.verdict.as_str(), checked.stdout.as_str()),
        ("exit(0)", want),
        "the CHECKED machine on {name}: {checked:?}"
    );
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(entry, dir, "a", release) else {
            continue;
        };
        let out = run_in(Command::new(&exe), dir);
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        eprintln!(
            "== {tier} on {name}: exit {:?}\n{stdout}{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(0), "the {tier} binary on {name}");
        assert_eq!(stdout, want, "the {tier} binary on {name}");
    }
    if let Some(l) = lupin_or_skip() {
        let mut cmd = Command::new(&l);
        cmd.arg("conform-run").arg(entry).arg("--json");
        let out = run_in(cmd, dir);
        judge_lupin(
            &parse_obs(&out.stdout, "lupin's observation"),
            &name,
            want,
            &lupin,
        );
    }
}

/// Whether the test runs as the super-user (a row a user meets is then a
/// different row, and the test says so rather than guessing).
fn as_root() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: geteuid cannot fail.
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

// ------------------------------------------------------ every host --

/// #686: the record's rows that hold on every host — twelve words, the
/// space words answered, free within total and available within free, a
/// file reads its file system's record, a missing path `not_found` with
/// the host's number beside it. Red at `7332772f`: E0301.
#[test]
fn the_file_system_record_on_every_host() {
    let dir = scratch("statfs_rows");
    let entry = corpus("fs/statfs_rows.lu");
    let want = declared_stdout(&entry);
    every_lane_answers_on_the_host(&entry, &dir, &want, Lupin::Unresolved(&["fs_statfs"]));
}

/// #921: the rows of `os_chroot` a user meets — linux looks the path up
/// first (`not_found`, 2), macOS asks for the privilege first (`denied`,
/// 1, measured on nomad), windows has no root to change; the issue's
/// witness `os_chroot(".")` is `denied` (1) on both unixes. Red at
/// `7332772f`: E0301.
#[test]
fn the_root_rows_on_every_host() {
    if as_root() {
        eprintln!("SKIP: the_root_rows_on_every_host: the test runs as root — these are a user's rows");
        return;
    }
    let dir = scratch("chroot_rows");
    let entry = corpus("os/chroot_rows.lu");
    let want = if cfg!(target_os = "macos") {
        "missing=denied\nmissing_number=1\ndot=denied\ndot_number=1\n".to_string()
    } else if cfg!(windows) {
        "missing=unsupported\nmissing_number=0\ndot=unsupported\ndot_number=0\n".to_string()
    } else {
        declared_stdout(&entry)
    };
    every_lane_answers_on_the_host(&entry, &dir, &want, Lupin::Unresolved(&["os_chroot"]));
}

/// #922: the nice value as a user meets it — read in range, the issue's
/// witness `os_setpriority(0)` ok and moving nothing, a raise ok, a far
/// raise clamped to the top, a lowering `denied` with EACCES and its text.
/// Red at `7332772f`: E0301.
#[test]
fn the_priority_rows_on_every_host() {
    if as_root() {
        eprintln!("SKIP: the_priority_rows_on_every_host: the test runs as root — these are a user's rows");
        return;
    }
    let dir = scratch("priority_rows");
    let entry = corpus("os/priority_rows.lu");
    let want = if cfg!(windows) {
        "read=unsupported\nzero=unsupported\n".to_string()
    } else {
        declared_stdout(&entry)
    };
    every_lane_answers_on_the_host(
        &entry,
        &dir,
        &want,
        Lupin::Unresolved(&["os_getpriority", "os_setpriority"]),
    );
}

/// #975: one signal's action, read and never changed — the shape rows,
/// TERMINATE by bit and by number the default, KILL and STOP the host's
/// (linux: the default; Darwin: `io` with EINVAL, 22), nothing moved by a
/// run of queries, an ignore 1, a listen 2. Red at `7332772f`: E0301.
#[test]
fn the_signal_action_rows_on_every_host() {
    let dir = scratch("action_rows");
    let entry = corpus("os/signal_action_rows.lu");
    let want = if cfg!(target_os = "macos") {
        declared_stdout(&entry)
            .replace("kill=0\nkill_number=0", "kill=io\nkill_number=22")
            .replace("stop=0\nstop_number=0", "stop=io\nstop_number=22")
    } else if cfg!(windows) {
        "zero=invalid\ntwo_bits=invalid\npast_table=invalid\nfar_host_number: true\nterm=unsupported\n"
            .to_string()
    } else {
        declared_stdout(&entry)
    };
    every_lane_answers_on_the_host(
        &entry,
        &dir,
        &want,
        Lupin::Unresolved(&["os_signal_action_one"]),
    );
}

/// #976: bit 24 (`RAW_LF`) is a mode bit — `io` on a handle that is not a
/// terminal for the set and the restore registration (windows:
/// `unsupported`), and bit 25 still `invalid`. Red at `7332772f`: bit 24
/// was `invalid`.
#[test]
fn the_mode_word_carries_onlcr_rows() {
    let dir = scratch("term_lf_rows");
    let entry = corpus("os/term_lf_rows.lu");
    let want = if cfg!(windows) {
        "lf_bit_on_a_file=unsupported\nlf_bit_restore_on_a_file=unsupported\npast_lf_bit=invalid\npast_lf_bit_restore=invalid\n"
            .to_string()
    } else {
        declared_stdout(&entry)
    };
    every_lane_answers_on_the_host(&entry, &dir, &want, Lupin::Refuses(&["os_term_set_mode"]));
}

/// #975, R-s272-9: the two fault meanings the runtime's stack guard holds
/// a handler for answer the PROGRAM's choice for a sent one on the
/// compiled tiers (0, 1 after an ignore, 0 after the default), never the
/// guard's handler (2); the checked machine refuses them by name, the
/// `wolf` process's own guard. Red at `7332772f`: E0301.
#[cfg(unix)]
#[test]
fn the_fault_meanings_read_the_programs_choice() {
    let dir = scratch("action_faults");
    let entry = fixture("action_faults.lu");
    let want = "segv=0\nsegv_ignored=1\nsegv_back=0\nbus=0\nbus_ignored=1\nbus_back=0\n";
    let checked = checked_in(&entry, &dir);
    assert_eq!(
        checked.verdict, "unsupported",
        "the checked machine refuses the fault meanings by name: {checked:?}"
    );
    assert!(
        checked.reason.contains("os_signal_action_one"),
        "the checked machine's refusal names the call: {checked:?}"
    );
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, &dir, "a", release) else {
            continue;
        };
        let out = run_in(Command::new(&exe), &dir);
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        eprintln!("== {tier} on action_faults.lu: exit {:?}\n{stdout}", out.status.code());
        assert_eq!(out.status.code(), Some(0), "the {tier} binary");
        assert_eq!(stdout, want, "the {tier} binary");
    }
    if let Some(l) = lupin_or_skip() {
        let mut cmd = Command::new(&l);
        cmd.arg("conform-run").arg(&entry).arg("--json");
        let out = run_in(cmd, &dir);
        judge_lupin(
            &parse_obs(&out.stdout, "lupin's observation"),
            "action_faults.lu",
            want,
            &Lupin::Unresolved(&["os_signal_action_one"]),
        );
    }
}

// ------------------------------------------- measured: the record --

/// One record as twelve words, by path, from the fixture's output.
fn records_of(stdout: &str) -> Vec<(String, Vec<i64>)> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let Some((p, rest)) = line.rsplit_once(": ").or_else(|| line.split_once(':')) else {
            continue;
        };
        let words: Vec<i64> = rest
            .split_whitespace()
            .filter_map(|w| w.parse().ok())
            .collect();
        out.push((p.trim().to_string(), words));
    }
    out
}

/// One machine's read of the record: its stdout.
type Reader = Box<dyn Fn() -> String>;

/// The machines that read the record, each a closure over the paths
/// file in `dir`: the checked machine, the native and release binaries.
fn record_machines(dir: &Path) -> Vec<(String, Reader)> {
    let entry = fixture("statfs_words.lu");
    let mut rows: Vec<(String, Reader)> = Vec::new();
    {
        let entry = entry.clone();
        let dir = dir.to_path_buf();
        rows.push((
            "checked".into(),
            Box::new(move || {
                let obs = checked_in(&entry, &dir);
                assert_eq!(obs.verdict, "exit(0)", "the checked machine reads the record: {obs:?}");
                obs.stdout
            }),
        ));
    }
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, dir, "words", release) else {
            continue;
        };
        let dir = dir.to_path_buf();
        rows.push((
            tier.into(),
            Box::new(move || {
                let out = run_in(Command::new(&exe), &dir);
                assert_eq!(out.status.code(), Some(0), "the binary reads the record");
                String::from_utf8_lossy(&out.stdout).to_string()
            }),
        ));
    }
    rows
}

/// Each word against the oracle's two reads around it: a word the host
/// keeps still (`fixed`) equals both; a count that moves while the host
/// writes (`moving`) lies between them, in the same second.
fn bracketed(before: i64, got: i64, after: i64) -> bool {
    got >= before.min(after) && got <= before.max(after)
}

/// The paths the record is measured on: the test's own directory (the
/// checkout's file system: ext4 on hasu and CI, APFS on macOS), the root,
/// and the host's other kinds where it mounts them.
fn measured_paths(dir: &Path) -> Vec<String> {
    let mut ps = vec![dir.display().to_string(), "/".to_string()];
    for p in ["/dev/shm", "/proc", "/tmp", "/dev", "/System/Volumes/Data"] {
        if Path::new(p).exists() {
            ps.push(p.to_string());
        }
    }
    ps
}

/// linux: GNU `stat -f`'s words for `p` — block size, fundamental block
/// size, total, free, available, files, free files, name max, type (hex),
/// id (hex) — or `None` when there is no GNU `stat`.
#[cfg(target_os = "linux")]
fn gnu_words(p: &str) -> Option<Vec<i64>> {
    let out = Command::new("stat")
        .args(["-f", "-c", "%s %S %b %f %a %c %d %l %t %i", p])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let t = String::from_utf8_lossy(&out.stdout);
    let f: Vec<&str> = t.split_whitespace().collect();
    if f.len() != 10 {
        return None;
    }
    let mut w: Vec<i64> = f[..8].iter().map(|s| s.parse::<u64>().unwrap() as i64).collect();
    w.push(u64::from_str_radix(f[8], 16).unwrap() as i64);
    w.push(u64::from_str_radix(f[9], 16).unwrap() as i64);
    Some(w)
}

/// linux: the mount that holds `p`, from `/proc/self/mountinfo` (the last
/// line whose mount point is the longest prefix of `p` wins, as the
/// kernel's overmounts do): (read-only, nosuid).
#[cfg(target_os = "linux")]
fn mount_flags(p: &str) -> (bool, bool) {
    let canon = std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p));
    let canon = canon.display().to_string();
    let info = std::fs::read_to_string("/proc/self/mountinfo").expect("mountinfo");
    let mut best: Option<(usize, bool, bool)> = None;
    for line in info.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 {
            continue;
        }
        let mp = f[4].replace("\\040", " ");
        let under = canon == mp || mp == "/" || canon.starts_with(&format!("{mp}/"));
        if !under {
            continue;
        }
        let opts: Vec<&str> = f[5].split(',').collect();
        let dash = f.iter().position(|x| *x == "-").unwrap_or(f.len() - 1);
        let sup: Vec<&str> = f.get(dash + 3).map_or(Vec::new(), |s| s.split(',').collect());
        let ro = opts.contains(&"ro") || sup.contains(&"ro");
        let nosuid = opts.contains(&"nosuid");
        if best.is_none_or(|(n, _, _)| mp.len() >= n) {
            best = Some((mp.len(), ro, nosuid));
        }
    }
    best.map_or((false, false), |(_, ro, ns)| (ro, ns))
}

/// #686: every word `fs_statfs` answers equals GNU `stat -f`'s (linux) —
/// the block size, the fragment size, the totals, name max, type and id
/// exactly, the free counts between GNU's read before and after the
/// program's — and the flags word the mount's `ro` and `nosuid`; on
/// macOS the 1024-blocks, used, available, iused and ifree `/bin/df -k`
/// prints, and name max `getconf NAME_MAX`'s. Every machine reads the
/// same record. The plant (`s272-plant-statfs`: `free` reported as
/// `available`) is red HERE by name on any file system that keeps blocks
/// back for the super-user (ext4's 5%). Red at `7332772f`: E0301.
#[test]
fn fs_statfs_matches_gnu_stat_f() {
    let dir = scratch("statfs_gnu");
    let paths = measured_paths(&dir);
    std::fs::write(dir.join("s272.paths"), paths.join("\n") + "\n").expect("paths written");
    report(&format!("REPORT statfs paths: {paths:?}"));
    #[cfg(target_os = "linux")]
    {
        let gnu_present = Command::new("stat")
            .arg("--version")
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("GNU coreutils"));
        if !gnu_present {
            assert!(
                std::env::var_os("WOLF_STATFS_REQUIRE_GNU").is_none(),
                "WOLF_STATFS_REQUIRE_GNU is set and no GNU stat is on PATH"
            );
            eprintln!("SKIP: fs_statfs_matches_gnu_stat_f: no GNU stat on PATH — the oracle is absent");
            return;
        }
        for (machine, read) in record_machines(&dir) {
            let mut ok = false;
            let mut last = String::new();
            for attempt in 0..3 {
                let before: Vec<Option<Vec<i64>>> = paths.iter().map(|p| gnu_words(p)).collect();
                let got = records_of(&read());
                let after: Vec<Option<Vec<i64>>> = paths.iter().map(|p| gnu_words(p)).collect();
                let mut bad = Vec::new();
                for (i, p) in paths.iter().enumerate() {
                    let (Some(b), Some(a)) = (&before[i], &after[i]) else {
                        eprintln!("SKIP: {p}: GNU stat -f did not answer");
                        continue;
                    };
                    let Some((_, w)) = got.iter().find(|(q, _)| q == p) else {
                        bad.push(format!("{p}: no record from {machine}"));
                        continue;
                    };
                    assert_eq!(w.len(), 12, "{machine}: twelve words for {p}: {w:?}");
                    report(&format!(
                        "REPORT {machine} {p}: wolf {w:?} gnu-before {b:?} gnu-after {a:?}"
                    ));
                    // GNU's order: %s %S %b %f %a %c %d %l %t %i; the record's
                    // words 1..8, 10, 11 (word 9 is the flags).
                    let at = [1usize, 2, 3, 4, 5, 6, 7, 8, 10, 11];
                    let names = [
                        "block_size",
                        "fragment_size",
                        "blocks",
                        "free",
                        "available",
                        "files",
                        "free_files",
                        "name_max",
                        "type",
                        "fsid",
                    ];
                    let moving = [3usize, 4, 6];
                    for (k, &wi) in at.iter().enumerate() {
                        let fine = if moving.contains(&k) {
                            bracketed(b[k], w[wi], a[k])
                        } else {
                            w[wi] == b[k] && w[wi] == a[k]
                        };
                        if !fine {
                            bad.push(format!(
                                "{p}: {} wolf={} gnu-before={} gnu-after={}",
                                names[k], w[wi], b[k], a[k]
                            ));
                        }
                    }
                    let (ro, nosuid) = mount_flags(p);
                    let want_flags = i64::from(ro) | (i64::from(nosuid) << 1);
                    if w[9] != want_flags {
                        bad.push(format!("{p}: flags wolf={} mount={want_flags}", w[9]));
                    }
                    if w[0] != 0xfff {
                        bad.push(format!("{p}: have wolf={:#x} want 0xfff", w[0]));
                    }
                }
                if bad.is_empty() {
                    ok = true;
                    break;
                }
                last = bad.join("\n");
                eprintln!("{machine}: attempt {attempt} outside GNU's reads:\n{last}");
            }
            assert!(ok, "{machine}: fs_statfs against GNU stat -f:\n{last}");
        }
    }
    #[cfg(target_os = "macos")]
    {
        // macOS `df -k`: Filesystem 1024-blocks Used Available Capacity
        // iused ifree %iused Mounted-on.
        let df = |p: &str| -> Option<Vec<i64>> {
            let out = Command::new("/bin/df").args(["-k", p]).output().ok()?;
            let t = String::from_utf8_lossy(&out.stdout).to_string();
            let line = t.lines().nth(1)?;
            let f: Vec<&str> = line.split_whitespace().collect();
            let num = |i: usize| f.get(i).and_then(|s| s.parse::<i64>().ok());
            Some(vec![num(1)?, num(2)?, num(3)?, num(5)?, num(6)?])
        };
        let name_max = |p: &str| -> Option<i64> {
            let out = Command::new("getconf").args(["NAME_MAX", p]).output().ok()?;
            String::from_utf8_lossy(&out.stdout).trim().parse().ok()
        };
        for (machine, read) in record_machines(&dir) {
            let mut ok = false;
            let mut last = String::new();
            for attempt in 0..3 {
                let before: Vec<Option<Vec<i64>>> = paths.iter().map(|p| df(p)).collect();
                let got = records_of(&read());
                let after: Vec<Option<Vec<i64>>> = paths.iter().map(|p| df(p)).collect();
                let mut bad = Vec::new();
                for (i, p) in paths.iter().enumerate() {
                    let (Some(b), Some(a)) = (&before[i], &after[i]) else {
                        eprintln!("SKIP: {p}: df -k did not answer");
                        continue;
                    };
                    let Some((_, w)) = got.iter().find(|(q, _)| q == p) else {
                        bad.push(format!("{p}: no record from {machine}"));
                        continue;
                    };
                    assert_eq!(w.len(), 12, "{machine}: twelve words for {p}: {w:?}");
                    let k = |n: i64| n * w[2] / 1024;
                    let mine = [k(w[3]), k(w[3] - w[4]), k(w[5]), w[6] - w[7], w[7]];
                    report(&format!(
                        "REPORT {machine} {p}: wolf {w:?} as df {mine:?} df-before {b:?} df-after {a:?}"
                    ));
                    let names = ["1024-blocks", "used", "available", "iused", "ifree"];
                    for (j, name) in names.iter().enumerate() {
                        let fine = if j == 0 {
                            mine[j] == b[j] && mine[j] == a[j]
                        } else {
                            bracketed(b[j], mine[j], a[j])
                        };
                        if !fine {
                            bad.push(format!(
                                "{p}: {name} wolf={} df-before={} df-after={}",
                                mine[j], b[j], a[j]
                            ));
                        }
                    }
                    if let Some(n) = name_max(p) {
                        if w[8] != n {
                            bad.push(format!("{p}: name_max wolf={} getconf={n}", w[8]));
                        }
                    }
                    if w[0] != 0xfff {
                        bad.push(format!("{p}: have wolf={:#x} want 0xfff", w[0]));
                    }
                }
                if bad.is_empty() {
                    ok = true;
                    break;
                }
                last = bad.join("\n");
                eprintln!("{machine}: attempt {attempt} outside df's reads:\n{last}");
            }
            assert!(ok, "{machine}: fs_statfs against df -k:\n{last}");
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = record_machines;
        let _ = records_of;
        let _ = bracketed;
        eprintln!("SKIP: fs_statfs_matches_gnu_stat_f: no oracle on this host (windows: the corpus row)");
    }
}

// ------------------------------------------------ live: ONLCR (unix) --

#[cfg(unix)]
mod pty {
    use std::io::Read as _;
    use std::os::fd::{AsRawFd as _, FromRawFd as _};
    use std::os::unix::fs::OpenOptionsExt as _;
    use std::os::unix::process::CommandExt as _;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// One run on a fresh pseudo-terminal, its controlling terminal, the
    /// slave started with `ONLCR` as `onlcr` says (output processing on).
    /// Answers the raw transcript, the exit code and `ONLCR` after.
    pub fn run(argv: &[String], dir: &Path, onlcr: bool) -> (Vec<u8>, Option<i32>, bool) {
        // SAFETY: the pty calls on descriptors this test owns; the child's
        // hook calls only async-signal-safe functions.
        unsafe {
            let m = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
            assert!(m >= 0, "posix_openpt");
            assert_eq!(libc::grantpt(m), 0, "grantpt");
            assert_eq!(libc::unlockpt(m), 0, "unlockpt");
            let name = std::ffi::CStr::from_ptr(libc::ptsname(m))
                .to_string_lossy()
                .into_owned();
            let master = std::fs::File::from_raw_fd(m);
            let slave = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NOCTTY)
                .open(&name)
                .expect("open the pty's slave");
            let mut t: libc::termios = std::mem::zeroed();
            assert_eq!(libc::tcgetattr(slave.as_raw_fd(), &mut t), 0, "tcgetattr");
            t.c_oflag |= libc::OPOST;
            if onlcr {
                t.c_oflag |= libc::ONLCR;
            } else {
                t.c_oflag &= !libc::ONLCR;
            }
            assert_eq!(
                libc::tcsetattr(slave.as_raw_fd(), libc::TCSANOW, &t),
                0,
                "tcsetattr"
            );
            let w = libc::winsize {
                ws_row: 24,
                ws_col: 80,
                ws_xpixel: 0,
                ws_ypixel: 0,
            };
            assert_eq!(libc::ioctl(m, libc::TIOCSWINSZ, &w), 0, "TIOCSWINSZ");
            let mut cmd = Command::new(&argv[0]);
            cmd.args(&argv[1..])
                .current_dir(dir)
                .stdin(Stdio::from(slave.try_clone().expect("dup")))
                .stdout(Stdio::from(slave.try_clone().expect("dup")))
                .stderr(Stdio::from(slave.try_clone().expect("dup")));
            cmd.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
            let mut child = cmd.spawn().expect("spawn under the pty");
            let buf = Arc::new(Mutex::new(Vec::new()));
            let sink = buf.clone();
            let mut rd = master.try_clone().expect("dup master");
            let reader = std::thread::spawn(move || {
                let mut b = [0u8; 4096];
                loop {
                    match rd.read(&mut b) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => sink.lock().unwrap().extend_from_slice(&b[..n]),
                    }
                }
            });
            let end = Instant::now() + Duration::from_secs(120);
            let code = loop {
                match child.try_wait().expect("try_wait") {
                    Some(st) => break st.code(),
                    None if Instant::now() > end => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break None;
                    }
                    None => std::thread::sleep(Duration::from_millis(20)),
                }
            };
            std::thread::sleep(Duration::from_millis(200));
            let mut after: libc::termios = std::mem::zeroed();
            assert_eq!(
                libc::tcgetattr(slave.as_raw_fd(), &mut after),
                0,
                "tcgetattr after"
            );
            let onlcr_after = after.c_oflag & libc::ONLCR != 0;
            drop(slave);
            drop(master);
            let _ = reader.join();
            let bytes = buf.lock().unwrap().clone();
            (bytes, code, onlcr_after)
        }
    }
}

/// #976: on a terminal started with `ONLCR` on and with it off, the
/// program reads `RAW_LF` as the terminal has it, turns output processing
/// on WITH the newline mapping, and the wire reads `ONLCR-A CR LF
/// ONLCR-B` in both; when it ends — after `os_term_set_mode` back, and by
/// the runtime's restore at the return from `main` — the terminal's
/// `ONLCR` is what it was at start. The checked machine and both compiled
/// tiers (yew's `onlcr/probe.py` shape, re-made here). Red at `7332772f`:
/// the set of a mode clearing `RAW_LF` was `invalid` (bit 24), and the
/// mapping could not be turned on.
#[cfg(unix)]
#[test]
fn onlcr_on_a_terminal_started_with_it_on_and_off() {
    let dir = scratch("onlcr");
    let entry = fixture("onlcr.lu");
    let mut machines: Vec<(String, Vec<String>)> = vec![(
        "checked".into(),
        vec![
            wolf().to_string(),
            "conform-run".into(),
            entry.display().to_string(),
            "--checked".into(),
            "--json".into(),
        ],
    )];
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        if let Some(exe) = build(&entry, &dir, "onlcr", release) {
            machines.push((tier.into(), vec![exe.display().to_string()]));
        }
    }
    let mut bad = Vec::new();
    for (machine, argv) in &machines {
        for onlcr in [true, false] {
            for how in ["set", "register"] {
                std::fs::write(dir.join("s272.how"), how).expect("how written");
                let (bytes, code, after) = pty::run(argv, &dir, onlcr);
                let text = String::from_utf8_lossy(&bytes).to_string();
                eprintln!(
                    "== {machine} onlcr={onlcr} how={how}: exit {code:?}, ONLCR after {after}\n{text:?}"
                );
                let start_bit = if onlcr { "raw_lf_at_start=0" } else { "raw_lf_at_start=1" };
                let mut why = Vec::new();
                if code != Some(0) {
                    why.push(format!("exit {code:?}"));
                }
                if !text.contains(start_bit) {
                    why.push(format!("no `{start_bit}`"));
                }
                if !text.contains("raw_lf_after_set=0") {
                    why.push("no `raw_lf_after_set=0`".into());
                }
                if !bytes.windows(17).any(|w| w == b"ONLCR-A\r\nONLCR-B") {
                    why.push("the wire does not read ONLCR-A CR LF ONLCR-B".into());
                }
                if after != onlcr {
                    why.push(format!("ONLCR after is {after}, at start {onlcr}"));
                }
                if !why.is_empty() {
                    bad.push(format!("{machine} onlcr={onlcr} how={how}: {}", why.join("; ")));
                }
            }
        }
    }
    assert!(bad.is_empty(), "ONLCR rows:\n{}", bad.join("\n"));
}

// --------------------------------- live: root and priority (linux) --

#[cfg(target_os = "linux")]
fn image() -> String {
    std::env::var("WOLF_JOBWAIT_IMAGE").unwrap_or_else(|_| "docker.io/library/ubuntu:24.04".into())
}

/// The engine that runs a throwaway container here, and whether it is
/// rootless (its root holds no capability in the host's namespace), or a
/// loud skip — a failure under `WOLF_JOBWAIT_REQUIRE_CONTAINER`.
#[cfg(target_os = "linux")]
fn engine(test: &str) -> Option<(String, bool)> {
    let required = std::env::var_os("WOLF_JOBWAIT_REQUIRE_CONTAINER").is_some();
    let candidates: Vec<String> = match std::env::var("WOLF_JOBWAIT_ENGINE") {
        Ok(e) => vec![e],
        Err(_) => vec!["docker".into(), "podman".into()],
    };
    let mut why = Vec::new();
    for e in &candidates {
        let out = Command::new(e)
            .args(["run", "--rm", "--network", "none"])
            .arg(image())
            .args(["sh", "-c", "echo $$"])
            .stdin(Stdio::null())
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let rootless = if e.contains("podman") {
                    Command::new(e)
                        .args(["info", "--format", "{{.Host.Security.Rootless}}"])
                        .output()
                        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
                } else {
                    Command::new(e)
                        .args(["info", "--format", "{{.SecurityOptions}}"])
                        .output()
                        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("rootless"))
                };
                report(&format!("REPORT {test}: engine {e}, rootless {rootless}, image {}", image()));
                return Some((e.clone(), rootless));
            }
            Ok(o) => why.push(format!(
                "{e}: {}",
                String::from_utf8_lossy(&o.stderr).lines().next().unwrap_or("")
            )),
            Err(err) => why.push(format!("{e}: {err}")),
        }
    }
    assert!(
        !required,
        "WOLF_JOBWAIT_REQUIRE_CONTAINER is set and no engine ran {} — {test} did not run: {why:?}",
        image()
    );
    eprintln!(
        "SKIP: {test}: no container engine ran {} ({why:?}) — the live witnesses did not run",
        image()
    );
    None
}

/// `script` in a throwaway container: no network, the test's directory at
/// `/w` READ-ONLY and its working directory, the fixtures and the `wolf`
/// binary read-only, `HOME=/tmp`, as `user` (`uid:gid`), with `caps`
/// added.
#[cfg(target_os = "linux")]
fn in_container(engine: &str, dir: &Path, user: &str, caps: &[&str], script: &str) -> Output {
    let mut c = Command::new(engine);
    c.args(["run", "--rm", "--network", "none", "--user", user, "-e", "HOME=/tmp"]);
    for cap in caps {
        c.args(["--cap-add", cap]);
    }
    c.arg("-v").arg(format!("{}:/w:ro", dir.display()));
    c.arg("-v")
        .arg(format!("{}:/fx:ro", fixture("onlcr.lu").parent().unwrap().display()));
    c.arg("-v").arg(format!("{}:/machines/wolf:ro", wolf()));
    c.args(["-w", "/w"]).arg(image()).args(["sh", "-c", script]);
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c.output().expect("the engine runs")
}

/// The machines a container case runs, each as a shell command: the
/// checked machine under `conform-run` (its stdout from the record), and
/// the native and release binaries in the test's directory — plain, and
/// under `taskset` on four of the allocation where there are four.
#[cfg(target_os = "linux")]
fn container_machines(entry_name: &str, stem: &str, dir: &Path, test: &str) -> Vec<(String, String, bool)> {
    let entry = fixture(entry_name);
    let mut rows = vec![(
        "checked".to_string(),
        format!("/machines/wolf conform-run /fx/{entry_name} --checked --json"),
        true,
    )];
    let four = test_cpus::subset(4, test);
    if four.is_none() {
        eprintln!("SKIP: {test}: fewer than four cpus — the pinned rows did not run");
    }
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, dir, stem, release) else {
            continue;
        };
        let name = exe.file_name().unwrap().to_string_lossy().to_string();
        rows.push((tier.to_string(), format!("/w/{name}"), false));
        if let Some(list) = &four {
            rows.push((
                format!("{tier} taskset -c {list}"),
                format!("taskset -c {list} /w/{name}"),
                false,
            ));
        }
    }
    rows
}

/// The program's answer from a container case: the record's stdout for
/// the checked machine, the process's own for a binary.
#[cfg(target_os = "linux")]
fn answer_of(out: &Output, record: bool, what: &str) -> String {
    if record {
        let obs = parse_obs(&out.stdout, what);
        assert_eq!(obs.verdict, "exit(0)", "{what}: {obs:?}");
        obs.stdout
    } else {
        assert_eq!(out.status.code(), Some(0), "{what}: {out:?}");
        String::from_utf8_lossy(&out.stdout).replace('\r', "")
    }
}

/// #921 and #922 in throwaway containers, once as a user (4242) and once
/// as root inside the container, on every machine: as root the change of
/// root takes (`/` is the jail after `os_chdir("/")`, the old `/w` gone)
/// and as a user it is `denied` (EPERM, 1); a missing path is `not_found`
/// and a file `io` (ENOTDIR, 20) for both. A raise of the nice value is
/// ok for both; a lowering is `denied` (EACCES, 13) for the user and for
/// root without CAP_SYS_NICE in the host's namespace, and ok for root
/// granted it (`--cap-add SYS_NICE` under a rootful engine; a rootless
/// engine's root holds it in its own namespace only, so it stays
/// `denied` there) — and GNU `nice -n -1` prints the same host text in the
/// same container. Red at `7332772f`: E0301.
#[cfg(target_os = "linux")]
#[test]
fn chroot_and_priority_in_a_container_as_root_and_as_a_user() {
    let test = "root_and_priority";
    let Some((engine, rootless)) = engine(test) else {
        return;
    };
    let dir = scratch(test);
    std::fs::create_dir_all(dir.join("jail")).expect("the jail");
    std::fs::write(dir.join("jail/s272-marker"), "marker\n").expect("the marker");
    let mut bad = Vec::new();
    let roots = container_machines("chroot_case.lu", "chroot", &dir, test);
    for (who, user) in [("user", "4242:4242"), ("root", "0:0")] {
        let want = if who == "root" {
            "euid=0\nmissing=not_found number=2\nfile=io number=20\njail=ok number=0\ncwd_unchanged: true\nmarker_at_root: true\nold_root_gone: true\n".to_string()
        } else {
            "euid=4242\nmissing=not_found number=2\nfile=io number=20\njail=denied number=1\n".to_string()
        };
        for (machine, cmd, record) in &roots {
            let out = in_container(&engine, &dir, user, &[], cmd);
            let got = answer_of(&out, *record, &format!("chroot {who} {machine}"));
            eprintln!("== chroot as {who}, {machine}:\n{got}{}", String::from_utf8_lossy(&out.stderr));
            if got != want {
                bad.push(format!("chroot as {who}, {machine}: got {got:?}, want {want:?}"));
            }
        }
    }
    let prios = container_machines("priority_case.lu", "prio", &dir, test);
    let cases: [(&str, &str, &[&str]); 3] = [
        ("user", "4242:4242", &[]),
        ("root", "0:0", &[]),
        ("root+SYS_NICE", "0:0", &["SYS_NICE"]),
    ];
    for (who, user, caps) in cases {
        let lowers = who == "root+SYS_NICE" && !rootless;
        let want = if lowers {
            "start=0\nraise=ok\nraised=2\nlower=ok number=0 text=\nafter=1\n"
        } else {
            "start=0\nraise=ok\nraised=2\nlower=denied number=13 text=Permission denied\nafter=2\n"
        };
        for (machine, cmd, record) in &prios {
            let out = in_container(&engine, &dir, user, caps, cmd);
            let got = answer_of(&out, *record, &format!("priority {who} {machine}"));
            eprintln!("== priority as {who}, {machine}:\n{got}{}", String::from_utf8_lossy(&out.stderr));
            if got != want {
                bad.push(format!("priority as {who}, {machine}: got {got:?}, want {want:?}"));
            }
        }
        let gnu = in_container(
            &engine,
            &dir,
            user,
            caps,
            "nice -n -1 true; echo status=$?",
        );
        let text = String::from_utf8_lossy(&gnu.stderr).to_string();
        let status = String::from_utf8_lossy(&gnu.stdout).to_string();
        report(&format!("REPORT GNU nice -n -1 as {who}: {text:?} {status:?}"));
        let gnu_says = text
            .lines()
            .find_map(|l| l.split_once("cannot set niceness: ").map(|(_, t)| t.trim().to_string()));
        if lowers {
            if gnu_says.is_some() {
                bad.push(format!("GNU nice as {who} refused: {text:?}"));
            }
        } else if gnu_says.as_deref() != Some("Permission denied") {
            bad.push(format!("GNU nice as {who}: {text:?}, the row says `Permission denied`"));
        }
    }
    assert!(bad.is_empty(), "container rows:\n{}", bad.join("\n"));
}
