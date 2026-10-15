//! s239 (wolf-lang#750, #751) — a park at the descriptor limit, on every
//! machine. `[os.net.reactor]`, `[os.fs.error]`.
//!
//! Until s239 the native runtime made its readiness set (`epoll_create1`
//! and an `eventfd` on linux, a `kqueue` on macOS) at the first call that
//! parked, and ASSERTED that the host gave it. A process that had used
//! its descriptors up before it first parked ended there: a panic in a
//! function that cannot unwind, SIGABRT, after the program's own lines
//! had run (lobo 0.1.2 and 0.1.3 could be ended by a client this way,
//! lobo#90). `os_signal_wait_fd` did the same with its wake pipe. Now a
//! program that names a parking net call makes the set before `main`,
//! and a refusal there is one stderr line and exit 71; a park asks the
//! host for nothing it can refuse with an abort.
//!
//! Every program here runs with ITS OWN descriptor limit lowered
//! (`setrlimit` in the child between fork and exec), never the test
//! runner's and never the host's. Machines: the native and release
//! binaries `wolf build` made — plain, and on linux again under
//! `taskset -c 0-3` (wolf-lang#570) — the checked machine under
//! `conform-run --checked` and lupin under `conform-run`.
//!
//! lupin 0.1.52 (the pairing) predates the mirror: its net family does
//! not set `os_error()` (#751's lupin half) and it does not resolve
//! `os_signal_wait_fd` (s234's R6, refused by name). Its answers are
//! pinned by version and release commit as pre-mirror (s180's design),
//! never widened.

#![cfg(unix)]

mod lane_exit;

use std::io::{BufRead as _, BufReader};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The descriptor limit the at-limit witnesses run under: room for the
/// standard streams, the loader, a listener and a connected pair, and
/// a table the program then fills itself.
const LIMIT: u64 = 32;

/// The lupin releases that predate the mirror, each with the commit its
/// release archive reports (0.1.52's archive, digest f5194b74…, reports
/// `025428b`, measured on kasumi).
const PRE_MIRROR_LUPIN: &[(&str, Option<&str>)] = &[("0.1.52", Some("025428b"))];

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/reactor")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// A working directory of the test's own: the witnesses write a scratch
/// file relative to their cwd.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("reactor_lanes")
        .join(test);
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

/// `cmd` run in `dir` with the CHILD's descriptor limit lowered to `n`
/// (the hard limit left alone): the test runner's own limit never moves.
fn limited(mut cmd: Command, dir: &Path, n: u64) -> Output {
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: between fork and exec the closure makes two system calls
    // and touches nothing else.
    unsafe {
        cmd.pre_exec(move || {
            let mut r = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::getrlimit(libc::RLIMIT_NOFILE, &mut r) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            r.rlim_cur = (n as libc::rlim_t).min(r.rlim_max);
            if libc::setrlimit(libc::RLIMIT_NOFILE, &r) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.output().expect("the witness runs")
}

/// The native or release binary of `entry`. A compile error FAILS the
/// gate; `None` is a host that cannot build the tier (loud).
fn build(entry: &Path, dir: &Path, release: bool) -> Option<PathBuf> {
    let stem = entry.file_stem().unwrap().to_string_lossy();
    let exe = dir.join(format!(
        "{stem}-{}",
        if release { "release" } else { "native" }
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

/// The cpu sets the compiled witnesses run on: all cores, and on linux
/// four (`taskset -c 0-3`, wolf-lang#570) — never skipped there.
fn cpu_sets() -> Vec<Option<&'static str>> {
    if cfg!(target_os = "linux") {
        let ok = Command::new("taskset")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        assert!(
            ok,
            "`taskset` (util-linux) is part of this gate's linux host: the witness must run \
             on four cpus as well as all cores, never skip them"
        );
        vec![None, Some("0-3")]
    } else {
        eprintln!("NOTE: no cpu-affinity tool on this host; the witnesses run on all cores only");
        vec![None]
    }
}

fn compiled(exe: &Path, set: Option<&str>) -> Command {
    match set {
        Some(cpus) => {
            let mut c = Command::new("taskset");
            c.args(["-c", cpus]).arg(exe);
            c
        }
        None => Command::new(exe),
    }
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

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
    commit: String,
}

fn parse_obs(out: &Output, what: &str) -> Obs {
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{what} record parses: {e}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        commit: rec["commit"].as_str().unwrap_or("").to_string(),
    }
}

/// The checked machine's record for `entry`, run under `n` descriptors.
fn checked(entry: &Path, dir: &Path, n: u64) -> Obs {
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--checked")
        .arg("--json");
    let out = limited(cmd, dir, n);
    assert!(
        out.status.success(),
        "conform-run --checked failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    parse_obs(&out, "the checked machine's")
}

/// lupin's record, or `None` when this box has no sibling.
/// `WOLF_PAIRING_REQUIRE_SIBLING` is the linux CI job saying one was
/// arranged here: an absent sibling is then a failure.
fn lupin(entry: &Path, dir: &Path, n: u64) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of #750's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        return None;
    };
    let mut cmd = Command::new(&lupin);
    cmd.arg("conform-run").arg(entry).arg("--json");
    let out = limited(cmd, dir, n);
    Some(parse_obs(&out, "lupin's"))
}

fn pre_mirror(o: &Obs) -> bool {
    PRE_MIRROR_LUPIN
        .iter()
        .any(|(v, c)| *v == o.version && c.is_none_or(|c| o.commit.starts_with(c)))
}

/// The line of `stdout` that starts with `key ` — its remainder.
fn line<'a>(stdout: &'a str, key: &str, what: &str) -> &'a str {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key} ")))
        .unwrap_or_else(|| panic!("{what}: no `{key}` line in:\n{stdout}"))
}

/// `full held=N errno=24`: the host refused an open with EMFILE after
/// at least one open.
fn assert_full(stdout: &str, what: &str) {
    let full = line(stdout, "full", what);
    let held: i64 = full
        .split_whitespace()
        .find_map(|w| w.strip_prefix("held="))
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("{what}: unreadable full line `{full}`"));
    assert!(held > 0, "{what}: the table was not filled: `{full}`");
    assert!(
        full.ends_with(&format!("errno={}", libc::EMFILE)),
        "{what}: the refusal was not EMFILE: `{full}`"
    );
}

/// `park_at_limit.lu`'s ruled answer: the park is the budget's
/// `timeout` (the set was made before `main`, so the park asks for
/// nothing), and again once the descriptors are back. The accept on an
/// empty queue at the limit is the HOST's: linux refuses the take
/// before it would block (`io`, EMFILE beside it — #751), macOS answers
/// would-block, so the take waits out its budget (`timeout`, 0) —
/// measured on the runner (`build + test (macos-latest)`, run
/// 38655502222, the native tier; checked and lupin held to it).
/// `accept_errno` is what the machine leaves in `os_error()` after
/// linux's refused accept.
fn assert_park(stdout: &str, accept_errno: i32, what: &str) {
    assert_full(stdout, what);
    let accept = if cfg!(target_os = "linux") {
        format!("row=io errno={accept_errno}")
    } else {
        "row=timeout errno=0".to_string()
    };
    assert_eq!(
        line(stdout, "accept", what),
        accept,
        "{what}: the accept at the limit"
    );
    assert_eq!(
        line(stdout, "park", what),
        "row=timeout errno=0",
        "{what}: the first park at the limit"
    );
    assert_eq!(
        line(stdout, "after", what),
        "row=timeout errno=0",
        "{what}: the park with descriptors back"
    );
}

/// `[os.net.reactor]` R1/R3, `[os.fs.error]` (#751): no machine ends the
/// process at a park at the descriptor limit, and every wolfgang machine
/// answers the same rows and numbers.
#[test]
fn a_park_at_the_descriptor_limit_does_not_end_the_process() {
    let dir = scratch("park");
    let entry = fixture("park_at_limit.lu");
    for release in [false, true] {
        let Some(exe) = build(&entry, &dir, release) else {
            continue;
        };
        for set in cpu_sets() {
            let what = format!(
                "{} ({})",
                if release { "release" } else { "native" },
                set.map_or("all cores".to_string(), |c| format!("taskset -c {c}"))
            );
            let out = limited(compiled(&exe, set), &dir, LIMIT);
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert_eq!(
                out.status.code(),
                Some(0),
                "{what}: the process did not finish (until s239: SIGABRT at the first park, \
                 `epoll_create1 failed`)\nstdout:\n{stdout}\nstderr:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_park(&stdout, libc::EMFILE, &what);
            println!("{what}: {}", stdout.replace('\n', " | "));
        }
    }
    let c = checked(&entry, &dir, LIMIT);
    assert_eq!(c.verdict, "exit(0)", "checked: {c:?}");
    assert_park(&c.stdout, libc::EMFILE, "checked");
    println!("checked: {}", c.stdout.replace('\n', " | "));
    if let Some(l) = lupin(&entry, &dir, LIMIT) {
        assert_eq!(l.verdict, "exit(0)", "lupin: {l:?}");
        // Pre-mirror: lupin's net family leaves the number alone.
        let n = if pre_mirror(&l) { 0 } else { libc::EMFILE };
        assert_park(&l.stdout, n, &format!("lupin {}", l.version));
        println!(
            "lupin {} ({}): {}",
            l.version,
            l.commit,
            l.stdout.replace('\n', " | ")
        );
    }
}

/// The signal wait at the limit: `io`, never an abort; once the
/// descriptors are back, the file it waits on has a byte (0).
fn assert_wait(stdout: &str, what: &str) {
    assert_full(stdout, what);
    assert_eq!(
        line(stdout, "wait", what),
        "row=io",
        "{what}: the wait at the limit"
    );
    assert_eq!(
        line(stdout, "after", what),
        "row=answered value=0",
        "{what}: the wait with descriptors back"
    );
}

/// `[os.net.reactor]` R4: `os_signal_wait_fd` with one descriptor free
/// (its duplicate takes it, its pipe cannot be had) answers `io`.
#[test]
fn a_signal_wait_at_the_descriptor_limit_is_io() {
    let dir = scratch("wait_fd");
    let entry = fixture("wait_fd_at_limit.lu");
    for release in [false, true] {
        let Some(exe) = build(&entry, &dir, release) else {
            continue;
        };
        for set in cpu_sets() {
            let what = format!(
                "{} ({})",
                if release { "release" } else { "native" },
                set.map_or("all cores".to_string(), |c| format!("taskset -c {c}"))
            );
            let out = limited(compiled(&exe, set), &dir, LIMIT);
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert_eq!(
                out.status.code(),
                Some(0),
                "{what}: the process did not finish (until s239: SIGABRT, `signal self-pipe \
                 failed`)\nstdout:\n{stdout}\nstderr:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_wait(&stdout, &what);
            println!("{what}: {}", stdout.replace('\n', " | "));
        }
    }
    let c = checked(&entry, &dir, LIMIT);
    assert_eq!(c.verdict, "exit(0)", "checked: {c:?}");
    assert_wait(&c.stdout, "checked");
    println!("checked: {}", c.stdout.replace('\n', " | "));
    if let Some(l) = lupin(&entry, &dir, LIMIT) {
        if pre_mirror(&l) {
            assert_eq!(
                l.verdict, "unsupported",
                "lupin {} (pre-mirror): {l:?}",
                l.version
            );
        } else {
            assert_eq!(l.verdict, "exit(0)", "lupin: {l:?}");
            assert_wait(&l.stdout, &format!("lupin {}", l.version));
        }
        println!("lupin {} ({}): {}", l.version, l.commit, l.verdict);
    }
}

/// `[os.net.reactor]` R2: a program that names a parking call and whose
/// host refuses the readiness set never reaches `main` — one line on
/// stderr, exit 71 — while a program that names none starts under the
/// same limit. Linux: four descriptors is the standard streams and one
/// more, so the set (two descriptors there) cannot be had and the
/// loader (which opens and closes one at a time) still can. macOS's set
/// is ONE descriptor (a kqueue) and its loader's needs are not measured
/// here: the refusal is linux's row.
#[test]
fn a_program_that_cannot_make_its_readiness_set_does_not_start() {
    if !cfg!(target_os = "linux") {
        eprintln!(
            "NOTE: the start refusal is measured on linux (macOS's set is one kqueue \
             descriptor; its loader's needs at a four-descriptor limit are not measured)"
        );
        return;
    }
    let dir = scratch("start");
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(park) = build(&fixture("names_park.lu"), &dir, release) else {
            continue;
        };
        let Some(none) = build(&fixture("no_park.lu"), &dir, release) else {
            continue;
        };
        let out = limited(Command::new(&park), &dir, 4);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            out.status.code(),
            Some(71),
            "{tier}: names a park, four descriptors: {:?} stderr: {stderr}",
            out.status
        );
        assert!(
            out.stdout.is_empty(),
            "{tier}: `main` ran: {:?}",
            out.stdout
        );
        let lines: Vec<&str> = stderr.lines().collect();
        assert_eq!(lines.len(), 1, "{tier}: one stderr line: {stderr}");
        assert!(
            lines[0].starts_with("wolf: the runtime could not make its readiness set (")
                && lines[0].ends_with("); the program did not start")
                && lines[0].contains(&format!("(os error {})", libc::EMFILE)),
            "{tier}: the line: {stderr}"
        );
        let out = limited(Command::new(&none), &dir, 4);
        assert_eq!(
            (
                out.status.code(),
                String::from_utf8_lossy(&out.stdout).trim()
            ),
            (Some(0), "ready"),
            "{tier}: names no park, four descriptors: stderr {}",
            String::from_utf8_lossy(&out.stderr)
        );
        println!(
            "{tier}: names a park -> 71, `{}`; names none -> 0",
            lines[0]
        );
    }
}

/// Descriptors and threads of a running child, read from `/proc` once
/// it has printed `ready`.
fn at_rest(exe: &Path) -> (usize, usize) {
    let mut child = Command::new(exe)
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    let mut first = String::new();
    BufReader::new(child.stdout.as_mut().unwrap())
        .read_line(&mut first)
        .expect("the child's first line");
    assert_eq!(first.trim(), "ready");
    let pid = child.id();
    let fds = std::fs::read_dir(format!("/proc/{pid}/fd"))
        .expect("the child's descriptors")
        .count();
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).expect("status");
    let threads = status
        .lines()
        .find_map(|l| l.strip_prefix("Threads:"))
        .and_then(|n| n.trim().parse().ok())
        .expect("Threads");
    assert!(child.wait().expect("wait").success());
    (fds, threads)
}

/// `[os.net.reactor]` R1's cost, D15's rule: a program that names a
/// parking call carries the set (two descriptors on linux) and the
/// reactor thread from its start; a program that names none carries
/// neither, before `main` or after it.
#[test]
fn the_readiness_set_costs_a_program_only_when_it_names_a_park() {
    if !cfg!(target_os = "linux") {
        eprintln!("NOTE: the at-rest counts are read from linux's /proc");
        return;
    }
    let dir = scratch("cost");
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(park) = build(&fixture("names_park.lu"), &dir, release) else {
            continue;
        };
        let Some(none) = build(&fixture("no_park.lu"), &dir, release) else {
            continue;
        };
        let (pf, pt) = at_rest(&park);
        let (nf, nt) = at_rest(&none);
        assert_eq!(
            (pf, pt),
            (nf + 2, nt + 1),
            "{tier}: names a park (fds {pf}, threads {pt}) against names none (fds {nf}, \
             threads {nt})"
        );
        println!("{tier}: names a park fds={pf} threads={pt}; names none fds={nf} threads={nt}");
    }
}
