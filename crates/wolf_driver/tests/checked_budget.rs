//! s153 (wolf-lang#308) — the checked tier's resident set is bounded by
//! its budgets (`[exec.checked.budget]`), witnessed on sc43's
//! twelve-line reduction at a size that used to cost gibibytes.
//!
//! The shape: `rest.bytes()[0]` over a shrinking `rest`. A consumed
//! `bytes()` is a view (`[mem.str.view]`) and allocates nothing on any
//! lane, but between s136 and s153 the checked machine charged it zero
//! and STILL materialized a list per call that it never reclaimed —
//! n + (n-1) + … values retained under a step budget that could not
//! see them. Measured with `/usr/bin/time -l`, wolf 0.2.10, macOS
//! aarch64, before the fix:
//!
//! | n    | peak RSS   |
//! |------|------------|
//! | 2048 |   131 MiB  |
//! | 4096 |   485 MiB  |
//! | 8192 |  1867 MiB  |
//!
//! ×3.7 per doubling. The machine now hands the consumer the octets and
//! mints nothing, so the same walk is flat: this test runs n = 8192 as a
//! subprocess and asserts the peak under 200 MiB, and that doubling the
//! input three times from n = 1024 moves the peak by less than a
//! constant — the property, not just the number.
//!
//! The second test is the other half of the rule: the byte budget is a
//! ceiling the tier reaches BEFORE the host does. A `str` doubled by
//! `+` forty times would be a tebibyte; the machine charges each build
//! to its cumulative byte budget and refuses — `unsupported:
//! shadow-memory budget exhausted` — with the resident set bounded by a
//! few copies of the largest string it did build.
//!
//! How peak is measured, per host — the OS's own high-water mark of the
//! child process (the number `/usr/bin/time -l` prints), never the
//! machine's ledger:
//! - unix: `wait4(2)`'s `ru_maxrss` for exactly this child — bytes on
//!   macOS, kibibytes on linux and the BSDs;
//! - windows: `K32GetProcessMemoryInfo`'s `PeakWorkingSetSize` on the
//!   child's handle, read after it exits and before the handle closes.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const MIB: u64 = 1 << 20;

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// sc43's reduction, verbatim in shape: a literal of `n` bytes walked
/// one octet at a time through the view.
fn reduction(n: usize) -> String {
    format!(
        "//! check: run(exit=0)\n\
         //! phase: run\n\
         \n\
         fn main() -> int {{\n    \
             var rest = \"{}\"\n    \
             var n = 0\n    \
             while rest.len > 0 {{\n        \
                 let c = rest.bytes()[0] as int\n        \
                 if c < 0 {{ return 1 }}\n        \
                 rest = rest[1..]\n        \
                 n = n + 1\n    \
             }}\n    \
             if n == {n} {{ 0 }} else {{ 1 }}\n\
         }}\n",
        "a".repeat(n)
    )
}

/// A `str` doubled by `+` forty times: 2^40 bytes if nothing stops it.
const DOUBLING: &str = "//! check: run(exit=0)\n\
                        //! phase: run\n\
                        \n\
                        fn main() -> int {\n    \
                            var s = \"a\"\n    \
                            var i = 0\n    \
                            while i < 40 {\n        \
                                s = s + s\n        \
                                i = i + 1\n    \
                            }\n    \
                            if s.len > 0 { 0 } else { 1 }\n\
                        }\n";

struct Run {
    stdout: String,
    stderr: String,
    code: Option<i32>,
    peak_bytes: u64,
}

fn run_checked(path: &Path) -> Run {
    let mut child = Command::new(wolf())
        .args(["conform-run", "--checked", "--json"])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wolf");
    let mut out = child.stdout.take().expect("piped stdout");
    let mut err = child.stderr.take().expect("piped stderr");
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        err.read_to_string(&mut s).ok();
        s
    });
    let mut stdout = String::new();
    out.read_to_string(&mut stdout).expect("read stdout");
    let stderr = err_reader.join().expect("stderr reader");
    let (code, peak_bytes) = reap_with_peak(child);
    Run {
        stdout,
        stderr,
        code,
        peak_bytes,
    }
}

#[cfg(unix)]
fn reap_with_peak(child: Child) -> (Option<i32>, u64) {
    let pid = child.id() as libc::pid_t;
    let mut status: libc::c_int = 0;
    // SAFETY: a zeroed `rusage` is a valid out-parameter; `wait4` fills
    // it for the reaped child.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: plain FFI on a pid this process spawned and has not yet
    // waited for (both pipes were read to EOF above, so the child has
    // exited or is about to).
    let reaped = unsafe { libc::wait4(pid, &mut status, 0, &mut ru) };
    assert_eq!(reaped, pid, "wait4 reaps exactly the child");
    let code = if libc::WIFEXITED(status) {
        Some(libc::WEXITSTATUS(status))
    } else {
        None
    };
    // ru_maxrss: bytes on macOS, kibibytes everywhere else unix.
    let maxrss = ru.ru_maxrss as u64;
    let bytes = if cfg!(target_os = "macos") {
        maxrss
    } else {
        maxrss * 1024
    };
    (code, bytes)
}

#[cfg(windows)]
fn reap_with_peak(mut child: Child) -> (Option<i32>, u64) {
    use std::os::windows::io::AsRawHandle;
    // PROCESS_MEMORY_COUNTERS, as psapi.h lays it out.
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn K32GetProcessMemoryInfo(
            process: *mut core::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }
    let status = child.wait().expect("wait");
    let mut pmc = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    // SAFETY: the handle is the child's own, still open (the `Child`
    // closes it on drop, after this), and `pmc` is a correctly sized
    // out-parameter.
    let ok = unsafe { K32GetProcessMemoryInfo(child.as_raw_handle() as *mut _, &mut pmc, pmc.cb) };
    assert_ne!(
        ok, 0,
        "K32GetProcessMemoryInfo answers for an exited child whose handle is open"
    );
    (status.code(), pmc.peak_working_set_size as u64)
}

fn verdict(stdout: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("conform-run --json emits one record; got {e}: {stdout:?}"));
    v.get("verdict")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

#[test]
fn the_reduction_runs_flat_on_the_checked_tier() {
    let dir = scratch("checked_budget_walk");
    let small = dir.join("walk_1024.lu");
    let big = dir.join("walk_8192.lu");
    std::fs::write(&small, reduction(1024)).expect("write");
    std::fs::write(&big, reduction(8192)).expect("write");

    let s = run_checked(&small);
    let b = run_checked(&big);
    for (name, r) in [("n=1024", &s), ("n=8192", &b)] {
        assert_eq!(r.code, Some(0), "{name}: conform-run exits 0\n{}", r.stderr);
        assert_eq!(
            verdict(&r.stdout),
            "exit(0)",
            "{name}: the walk completes\n{}",
            r.stderr
        );
    }
    let (sp, bp) = (s.peak_bytes / MIB, b.peak_bytes / MIB);
    eprintln!("checked_budget: peak RSS n=1024 {sp} MiB, n=8192 {bp} MiB");
    assert!(
        b.peak_bytes < 200 * MIB,
        "n=8192 peaked at {bp} MiB — the ceiling is 200 MiB (it was 1867 MiB before s153, \
         wolf-lang#308)"
    );
    assert!(
        b.peak_bytes <= s.peak_bytes + 64 * MIB,
        "three doublings moved the peak from {sp} MiB to {bp} MiB — the walk must be flat \
         (before s153 it was x3.7 per doubling)"
    );
}

#[test]
fn a_str_doubled_by_plus_meets_the_byte_budget_before_the_host_does() {
    let dir = scratch("checked_budget_doubling");
    let path = dir.join("doubling.lu");
    std::fs::write(&path, DOUBLING).expect("write");
    let r = run_checked(&path);
    let peak = r.peak_bytes / MIB;
    eprintln!(
        "checked_budget: doubling peak RSS {peak} MiB, stderr: {}",
        r.stderr.trim()
    );
    assert_eq!(
        r.code,
        Some(0),
        "an honest refusal is a recorded observation\n{}",
        r.stderr
    );
    assert_eq!(
        verdict(&r.stdout),
        "unsupported",
        "the byte budget refuses the build, never a verdict\n{}",
        r.stderr
    );
    assert!(
        r.stderr.contains("shadow-memory budget exhausted"),
        "the refusal names the budget it hit: {}",
        r.stderr
    );
    // The largest str the budget admits is 2^27 bytes: the cumulative
    // charge passes 256 MiB on the next build, and `+` charges BEFORE
    // it builds, so the refused 2^28 result is never allocated. At the
    // refusing step the machine holds the place and its two operand
    // copies (3 x 128 MiB) plus the previous step's transients — a few
    // hundred MiB, against a tebibyte if nothing stopped it.
    // Measured 774 MiB on macOS aarch64 (debug) at s153 — six copies
    // of the admitted 128 MiB, allocator slack included; the ceiling
    // leaves room for a different allocator, not for a different law.
    assert!(
        r.peak_bytes < 1536 * MIB,
        "the refusal arrived at {peak} MiB — the ceiling is 1.5 GiB (unbounded before s153)"
    );
}
