//! s245 (wolf-lang#778) — a standard descriptor the program was started
//! without stays closed as the program sees it, on both compiled tiers.
//! `[os.signal.hub]`, `[os.net.reactor]`, `[os.fs.std]`.
//!
//! 0.2.31 made the signal hub (two pipes) before `main` in a program that
//! names a call which makes it (s243, ruling #80), and the readiness set
//! (epoll and an eventfd, a kqueue on macOS) in one that names a call
//! that can park (s239). Two faults met there. The host hands out the
//! lowest free descriptor, so a program started `prog >&-` found the
//! hub's pipe on 1: `fs_fstat(1)` answered ok and its output went into
//! the runtime's pipe. And the dev tier, which runs no mid-end, decided
//! "names" over every function the module held, so `use std.signal`
//! alone made the hub there, while the release tier, after its
//! dead-function walk, made none. boreutils' `cat` and `wc` with `>&-`
//! went red on the dev tier at 0.2.31 (bu26, boreutils `792ad31`
//! `notes/bu26/probes/fd1s.lu`, here byte for byte as `stdfd/fd1s.lu`).
//!
//! Every program runs with 0, 1 and 2 closed in turn: on unix closed in
//! the child between fork and exec, on windows started with that
//! standard handle null (the parent's handle set to null for the spawn,
//! under a lock, and put back). The programs write their report to a
//! file, so a run with 2 closed is read too. On linux they also run
//! under `taskset -c 0-3` (wolf-lang#570).
//!
//! The `use std.…` programs build against `fixtures/stdfd_std/`, a
//! stand-in for the bundled std's `std.signal` and `std.net` cut to the
//! shape the fault lives in (wolf-lang's CI has no wolf-std).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture(name: &str) -> PathBuf {
    let p = fixtures().join("stdfd").join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// A directory of the test's own for its builds and reports.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("stdfd_lanes")
        .join(test);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

#[derive(Clone, Copy, Debug)]
enum Tier {
    Dev,
    Release,
}

/// The runtime library the binaries link, built from this tree first
/// (`wolf build` links the `libwolf_rt.a` beside `wolf`; a test build of
/// the driver alone leaves whatever staticlib an earlier build made).
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

/// The binary `wolf build` makes of `entry` on `tier`, against the
/// stand-in std. A compile error FAILS the gate; `None` is a host that
/// cannot build the tier (loud).
fn build(entry: &Path, dir: &Path, tier: Tier) -> Option<PathBuf> {
    let stem = entry.file_stem().unwrap().to_string_lossy();
    let exe = dir.join(format!(
        "{stem}-{}{}",
        match tier {
            Tier::Dev => "dev",
            Tier::Release => "release",
        },
        std::env::consts::EXE_SUFFIX
    ));
    ensure_rt_staticlib();
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(entry)
        .arg("-o")
        .arg(&exe)
        .arg("--no-cache")
        .arg("--std-root")
        .arg(fixtures().join("stdfd_std"))
        .env_remove("WOLF_STD");
    if let Tier::Release = tier {
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
        // The one tier a host may not have: windows' release lane, dark by
        // design (s60c). Anywhere else a refusal FAILS the gate, so a green
        // run on linux and macOS is both tiers run, and on windows the dev
        // tier (wolf-lang#571: a skip line is hidden when the test passes).
        assert!(
            cfg!(windows) && matches!(tier, Tier::Release),
            "the {tier:?} tier refused to build on a host that has it: {}",
            stderr.trim()
        );
        eprintln!(
            "SKIP: environment cannot build the {tier:?} tier: {}",
            stderr.trim()
        );
        return None;
    }
    assert!(out.status.success(), "wolf build failed: {stderr}");
    Some(exe)
}

/// The cpu sets the binaries run on: all cores, and on linux four
/// (`taskset -c 0-3`, wolf-lang#570) — never skipped there.
fn cpu_sets() -> Vec<Option<&'static str>> {
    if cfg!(target_os = "linux") {
        let ok = Command::new("taskset")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        assert!(
            ok,
            "`taskset` (util-linux) is part of this gate's linux host: the programs must run \
             on four cpus as well as all cores, never skip them"
        );
        vec![None, Some("0-3")]
    } else {
        vec![None]
    }
}

fn command(exe: &Path, set: Option<&str>) -> Command {
    match set {
        Some(cpus) => {
            let mut c = Command::new("taskset");
            c.args(["-c", cpus]).arg(exe);
            c
        }
        None => Command::new(exe),
    }
}

/// `cmd` with standard descriptor `k` closed in the child (between fork
/// and exec, after the other two are wired), the others piped.
#[cfg(unix)]
fn run_closed(mut cmd: Command, k: i32) -> Output {
    use std::os::unix::process::CommandExt as _;
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: between fork and exec the closure makes one system call.
    unsafe {
        cmd.pre_exec(move || {
            libc::close(k);
            Ok(())
        });
    }
    let child = cmd.spawn().expect("the program starts");
    child.wait_with_output().expect("the program is reaped")
}

/// `cmd` started with standard handle `k` null, the others piped. A
/// spawn hands a child the parent's own handle for an inherited stream,
/// and a null one as null; so the parent's handle is set to null for the
/// spawn alone, under a lock no other spawn here takes, and put back.
#[cfg(windows)]
fn run_closed(mut cmd: Command, k: i32) -> Output {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(which: u32) -> *mut c_void;
        fn SetStdHandle(which: u32, handle: *mut c_void) -> i32;
    }
    // STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE: (DWORD)-10,
    // -11, -12.
    const WHICH: [u32; 3] = [-10i32 as u32, -11i32 as u32, -12i32 as u32];
    static SPAWN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let which = WHICH[k as usize];
    let pipe = |i: i32| {
        if i == k {
            Stdio::inherit()
        } else {
            Stdio::piped()
        }
    };
    cmd.stdin(pipe(0)).stdout(pipe(1)).stderr(pipe(2));
    let child = {
        let _g = SPAWN.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: the process's own standard handle, read, set to null
        // for one spawn and restored before the lock is released.
        let saved = unsafe { GetStdHandle(which) };
        // SAFETY: as above.
        let nulled = unsafe { SetStdHandle(which, std::ptr::null_mut()) };
        assert_ne!(nulled, 0, "SetStdHandle");
        let child = cmd.spawn();
        // SAFETY: as above.
        let back = unsafe { SetStdHandle(which, saved) };
        assert_ne!(back, 0, "SetStdHandle back");
        child.expect("the program starts")
    };
    child.wait_with_output().expect("the program is reaped")
}

/// One run's report.
struct Run {
    what: String,
    report: String,
}

impl Run {
    /// The report line that starts with `key ` (its rest), or "".
    fn line(&self, key: &str) -> String {
        self.report
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{key} ")))
            .unwrap_or("")
            .to_string()
    }

    /// The descriptors `/proc/self/fd` named at 3 and above (linux).
    fn above_std(&self) -> Vec<u32> {
        self.line("links")
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .filter(|&n| n > 2)
            .collect()
    }
}

/// `exe` run with `k` closed, reporting to a file of its own.
fn run(exe: &Path, set: Option<&str>, k: i32, dir: &Path) -> Run {
    let name = format!(
        "{}-{}-closed{k}.txt",
        exe.file_stem().unwrap().to_string_lossy(),
        set.map_or("all".to_string(), |s| format!("cpus{s}"))
    );
    let report_path = dir.join(name);
    let _ = std::fs::remove_file(&report_path);
    let mut cmd = command(exe, set);
    cmd.arg(&report_path);
    let out = run_closed(cmd, k);
    let what = format!(
        "{} on {}, descriptor {k} closed",
        exe.display(),
        set.map_or("all cores".to_string(), |s| format!("cpus {s}"))
    );
    assert!(
        out.status.success(),
        "{what}: exit {:?}; stderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let report =
        std::fs::read_to_string(&report_path).unwrap_or_else(|e| panic!("{what}: no report ({e})"));
    eprintln!("REPORT {what}: {report:?}");
    Run { what, report }
}

/// The three standard rows: the closed one reads closed, the other two
/// open (they are the runner's pipes).
fn assert_standard_rows(r: &Run, k: i32) {
    for fd in 0..3 {
        let want = if fd == k { "closed" } else { "open" };
        assert_eq!(
            r.line(&format!("fd{fd}")),
            want,
            "#778: {}: descriptor {fd} reads {:?}, want {want}",
            r.what,
            r.line(&format!("fd{fd}"))
        );
    }
}

/// Every program of `names`, built on both tiers, run with each of 0, 1
/// and 2 closed on every cpu set; `check` sees each run beside the
/// baseline's run of the same tier, set and closed descriptor.
fn every_run(test: &str, names: &[&str], check: impl Fn(&str, &Run, &Run, i32)) {
    let dir = scratch(test);
    for tier in [Tier::Dev, Tier::Release] {
        let Some(base) = build(&fixture("plain.lu"), &dir, tier) else {
            continue;
        };
        let exes: Vec<(&str, PathBuf)> = names
            .iter()
            .filter_map(|n| build(&fixture(&format!("{n}.lu")), &dir, tier).map(|e| (*n, e)))
            .collect();
        for set in cpu_sets() {
            for k in 0..3 {
                let b = run(&base, set, k, &dir);
                assert_standard_rows(&b, k);
                for (name, exe) in &exes {
                    let r = run(exe, set, k, &dir);
                    check(name, &r, &b, k);
                }
            }
        }
    }
}

/// bu26's probe itself, byte for byte. It reports on standard error, so
/// the runs with 0 and 1 closed are read: `fs_fstat(1)` answers `io`
/// with 1 closed and ok otherwise, and on linux no `/proc/self/fd` line
/// names the closed descriptor. With 2 closed it must still exit 0 (its
/// report goes nowhere).
#[test]
fn bu26s_probe_finds_a_closed_standard_descriptor_closed() {
    let dir = scratch("fd1s");
    for tier in [Tier::Dev, Tier::Release] {
        let Some(exe) = build(&fixture("fd1s.lu"), &dir, tier) else {
            continue;
        };
        for set in cpu_sets() {
            for k in 0..3 {
                let mut cmd = command(&exe, set);
                cmd.arg("unused");
                let out = run_closed(cmd, k);
                let what = format!("fd1s {tier:?} on {set:?}, descriptor {k} closed");
                assert!(out.status.success(), "{what}: exit {:?}", out.status);
                if k == 2 {
                    continue;
                }
                let err = String::from_utf8_lossy(&out.stderr);
                eprintln!("REPORT {what}: {err:?}");
                assert!(err.contains("meaning 2"), "{what}: {err}");
                let fstat = if k == 1 {
                    "fs_fstat(1): io"
                } else {
                    "fs_fstat(1): ok"
                };
                assert!(err.contains(fstat), "#778: {what}: want {fstat:?} in {err}");
                if cfg!(target_os = "linux") {
                    assert!(
                        !err.contains(&format!("fd {k}: open")),
                        "#778: {what}: the closed descriptor is open: {err}"
                    );
                }
            }
        }
    }
    panic!("s245 evidence plant: the REPORT lines above are this host's rows (not a failure of the gate)");
}

/// `use std.signal` and `use std.net`, reaching nothing that makes the
/// hub or the set: no tier makes either, so the closed descriptor stays
/// closed, and on linux the program holds exactly the baseline's
/// descriptors at 3 and above. 0.2.31's dev tier made the hub here.
#[test]
fn importing_signal_or_net_alone_makes_nothing_on_any_tier() {
    every_run(
        "imports",
        &["imports_signal", "imports_net"],
        |name, r, base, k| {
            assert_standard_rows(r, k);
            if cfg!(target_os = "linux") {
                assert_eq!(
                    r.above_std(),
                    base.above_std(),
                    "#778: {}: {name} holds descriptors the baseline does not (a hub or a set \
                     made for a call it never reaches)",
                    r.what
                );
            }
        },
    );
    panic!("s245 evidence plant: the REPORT lines above are this host's rows (not a failure of the gate)");
}

/// A program that reaches the listen, the raise and the wait: every tier
/// makes the hub before `main` (ruling #80), off the closed descriptor,
/// and the hub works where it is (the raise is heard by the wait). On
/// linux it holds more descriptors than the baseline: the hub's.
#[test]
fn a_reached_signal_hub_stays_off_a_closed_standard_descriptor() {
    every_run("hub", &["reaches_hub"], |_, r, base, k| {
        assert_standard_rows(r, k);
        assert_eq!(
            (
                r.line("listen").as_str(),
                r.line("raise").as_str(),
                r.line("wait").as_str()
            ),
            ("ok", "ok", "1"),
            "{}: the hub answers",
            r.what
        );
        if cfg!(target_os = "linux") {
            assert!(
                r.above_std().len() > base.above_std().len(),
                "{}: the hub was made before main ({:?} against {:?})",
                r.what,
                r.above_std(),
                base.above_std()
            );
        }
    });
    panic!("s245 evidence plant: the REPORT lines above are this host's rows (not a failure of the gate)");
}

/// The readiness set's twin: a program that reaches `net_accept` makes
/// the set before `main` (s239), off the closed descriptor.
#[test]
fn a_reached_readiness_set_stays_off_a_closed_standard_descriptor() {
    every_run("park", &["reaches_park"], |_, r, base, k| {
        assert_standard_rows(r, k);
        if cfg!(target_os = "linux") {
            assert!(
                r.above_std().len() > base.above_std().len(),
                "{}: the set was made before main ({:?} against {:?})",
                r.what,
                r.above_std(),
                base.above_std()
            );
        }
    });
    panic!("s245 evidence plant: the REPORT lines above are this host's rows (not a failure of the gate)");
}
