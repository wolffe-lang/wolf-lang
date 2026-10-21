//! s254 (wolf-lang#834; pelt sh22, `notes/sh22-evidence/witness/cwdregion`)
//! — `os_cwd()`'s answer dies with the region it is asked in.
//!
//! pelt's `shape-rss` method, as a row: the witness
//! (`fixtures/cwd_region/main.lu`, sh22's program) runs N and 2N turns of
//! `region r { let c = os_cwd() …; t += c.len }`, and the turn's cost is
//! what the extra N turns add to the peak resident set, divided by N — the
//! OS's own high-water mark of exactly this child (`wait4(2)`'s
//! `ru_maxrss`: kibibytes on linux, bytes on macOS), never a ledger. Two
//! controls beside it: a `str` of the same length built by interpolation
//! in the same region (the region gives it back), and the loop alone.
//!
//! **Red at trunk** (`9da7bdcf`, 0.2.34): the runtime copied the name with
//! `ambient_copy`, into the PROCESS ROOT whatever region was open, so each
//! turn kept the name rounded up to the root arena's 16-byte step — 32 B
//! a turn for kasumi's 28-byte lane directory, 48 B for pelt's 39-byte one
//! (sh22). The bound is 4 B a turn on both tiers, which a 16-byte step
//! can never meet; the controls read 0. Every row prints a `REPORT` line.
//! linux and macOS (the placement is one function on every host; windows
//! has no `wait4`, and the row is not asked there).

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn report(line: &str) {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("cwd_region_lanes")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

/// The witness built for one tier; a compile error fails the gate.
fn build(dir: &Path, release: bool) -> PathBuf {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cwd_region/main.lu");
    let tier = if release { "release" } else { "native" };
    let exe = dir.join(format!("cwdregion-{tier}"));
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&exe)
        .arg("--no-cache");
    if release {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf build runs");
    assert!(
        out.status.success(),
        "wolf build ({tier}) refused the witness: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    exe
}

/// One run: the program's stdout and the child's peak resident set in
/// bytes, from `wait4(2)` on exactly this child.
fn peak(exe: &Path, dir: &Path, mode: &str, size: &str) -> (String, u64) {
    let mut child = Command::new(exe)
        .args([mode, size])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the witness");
    let mut stdout = String::new();
    {
        use std::io::Read as _;
        let mut out = child.stdout.take().expect("piped stdout");
        out.read_to_string(&mut stdout).expect("read stdout");
    }
    let (status, bytes) = reap_with_peak(child);
    assert!(
        libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
        "the witness {} {mode} {size} did not exit 0 (status {status})",
        exe.display()
    );
    (stdout, bytes)
}

/// Reap `child` with `wait4(2)`: its raw status and its peak resident set
/// in bytes (`ru_maxrss`: kibibytes on linux, bytes on macOS).
fn reap_with_peak(child: std::process::Child) -> (libc::c_int, u64) {
    let pid = child.id() as libc::pid_t;
    let mut status: libc::c_int = 0;
    // SAFETY: a zeroed `rusage` is a valid out-parameter; `wait4` fills it
    // for the child this test spawned and has not waited for.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: plain FFI on a pid this process spawned.
    let reaped = unsafe { libc::wait4(pid, &mut status, 0, &mut ru) };
    assert_eq!(reaped, pid, "wait4 reaps exactly the child");
    let maxrss = ru.ru_maxrss as u64;
    let bytes = if cfg!(target_os = "macos") {
        maxrss
    } else {
        maxrss * 1024
    };
    (status, bytes)
}

/// N = 200,000 turns and 2N = 400,000: the bytes a turn keeps.
const N: i64 = 200_000;

#[test]
fn a_cwd_asked_in_a_region_keeps_nothing_a_turn() {
    let dir = scratch("rss");
    let cwd_len = dir.to_str().expect("utf-8 scratch").len() as i64;
    let mut failures = Vec::new();
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let exe = build(&dir, release);
        for mode in ["cwd", "interp", "none"] {
            let (out1, p1) = peak(&exe, &dir, mode, "xx");
            let (out2, p2) = peak(&exe, &dir, mode, "xxxx");
            let per_turn = (p2 as i64 - p1 as i64) / N;
            report(&format!(
                "REPORT cwdregion {tier} {mode} (cwd {cwd_len} bytes): {N} turns {p1} B, \
                 {} turns {p2} B, {per_turn} B a turn (t={}, {})",
                2 * N,
                out1.trim(),
                out2.trim()
            ));
            if mode == "cwd" {
                assert_eq!(
                    out2.trim(),
                    (2 * N * cwd_len).to_string(),
                    "the {tier} witness asked the name every turn"
                );
            }
            if per_turn > 4 {
                failures.push(format!("{tier} {mode}: {per_turn} B a turn"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "a region gave back less than it was given: {failures:?} (the bound is 4 B a turn; \
         a name kept in the root costs its length rounded up to 16)"
    );
}
