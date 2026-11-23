//! wolf-lang#1175: on linux-gnu, a large region buffer freed after an
//! earlier large free gives its pages back.
//!
//! glibc maps an allocation at or above its mmap threshold (128 KiB at
//! start) and unmaps it when it is freed. The first free of such a mapped
//! block raises the threshold to that block's size (glibc's dynamic
//! threshold, mallopt(3)), so every later block of that size comes from the
//! heap, and the heap keeps it resident after it is freed. A region's buffer
//! of that size is one host block (its own chunk) and the region's end frees
//! it, so a program that ends one large region and then another kept the
//! second region's buffer resident for good.
//!
//! The witness reads its own anonymous memory, resident plus swapped, from
//! `/proc/self/smaps_rollup` (the kernel counts it from the page tables when
//! it is read; the instrument `region_copyout_lanes.rs` uses), once to warm
//! the reading's own storage, then before the first of two 2 MiB regions in
//! turn and after the second. Each region reserves its buffer, writes every
//! byte of it, and ends. The rows, on native and release:
//!
//! - after the second region the witness holds what it held before the
//!   first, within two pages;
//! - the same binary read twice agrees with itself within two pages (A/A);
//! - with the user's own threshold above the buffer's size and a trim
//!   threshold that keeps the heap (`MALLOC_MMAP_THRESHOLD_=4194304`,
//!   `MALLOC_TRIM_THRESHOLD_=67108864`) both buffers come from the heap and
//!   the heap keeps them, as the user asked: the runtime leaves the user's
//!   threshold alone.
//!
//! linux-gnu only: the threshold is glibc's. Elsewhere each row says SKIP by
//! name and asserts nothing.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const WITNESS: &str = r#"
fn field(text: str, key: str) -> str {
    for line in text.lines() {
        if line.starts_with(key) {
            return ((line.strip_prefix(key) else "").trim().strip_suffix("kB") else "?").trim()
        }
    }
    "?"
}

fn mark(tag: str) {
    region probe {
        let sm = fs_read_text("/proc/self/smaps_rollup") else ""
        print("mark {tag} anon={field(sm, "Anonymous:")} swap={field(sm, "Swap:")}")
    }
}

fn big(n: int) -> int {
    var kept = 0
    region work {
        var buf = List[byte]()
        (mut buf).reserve(n)
        var i = 0
        while i < n {
            (mut buf).push(7 as byte)
            i += 1
        }
        kept = buf.len
    }
    kept
}

fn main() -> int {
    mark("warm")
    mark("before")
    let a = big(2097152)
    let b = big(2097152)
    mark("after")
    print("done {a + b}")
    0
}
"#;

/// The environment names that set glibc's malloc policy, removed from every
/// run so the host's own setting cannot decide a row.
const MALLOC_VARS: [&str; 6] = [
    "MALLOC_MMAP_THRESHOLD_",
    "MALLOC_TRIM_THRESHOLD_",
    "MALLOC_TOP_PAD_",
    "MALLOC_MMAP_MAX_",
    "MALLOC_ARENA_MAX",
    "GLIBC_TUNABLES",
];

fn on_glibc() -> bool {
    cfg!(all(target_os = "linux", target_env = "gnu"))
}

/// Build the witness for one row and tier, into a directory of its own (the
/// rows run on threads of their own).
fn build(row: &str, tier: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("wolf-1175-{}-{row}-{tier}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("witness dir");
    let src = dir.join("main.lu");
    std::fs::write(&src, WITNESS).expect("witness source");
    let exe = dir.join("witness");
    let mut cmd = Command::new(wolf());
    cmd.arg("build").arg(&src).arg("-o").arg(&exe);
    if tier == "release" {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf build runs");
    assert!(
        out.status.success(),
        "the {tier} witness builds: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    exe
}

/// One run: the anonymous kB the witness holds before its first large region
/// and after its second.
fn run(exe: &Path, env: &[(&str, &str)]) -> (u64, u64) {
    let mut cmd = Command::new(exe);
    for k in MALLOC_VARS {
        cmd.env_remove(k);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("the witness runs");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success() && text.contains("done 4194304"),
        "the witness finished: {text} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let read = |tag: &str| -> u64 {
        let line = text
            .lines()
            .find(|l| l.starts_with(&format!("mark {tag} ")))
            .unwrap_or_else(|| panic!("no `mark {tag}` line: {text}"));
        let word = |key: &str| -> u64 {
            line.split_whitespace()
                .find_map(|w| w.strip_prefix(key))
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| panic!("an instrument that cannot read is not a pass: {line}"))
        };
        word("anon=") + word("swap=")
    };
    (read("before"), read("after"))
}

/// Two pages, in kB: the margin a reading of whole pages needs.
fn margin_kib() -> u64 {
    #[cfg(unix)]
    {
        // SAFETY: sysconf is always callable.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as u64;
        2 * page / 1024
    }
    #[cfg(not(unix))]
    {
        8
    }
}

#[test]
fn a_second_large_region_gives_its_pages_back() {
    if !on_glibc() {
        eprintln!(
            "SKIP a_second_large_region_gives_its_pages_back: linux-gnu only (the threshold is glibc's)"
        );
        return;
    }
    let m = margin_kib();
    for tier in ["native", "release"] {
        let exe = build("pages", tier);
        let (b1, a1) = run(&exe, &[]);
        let (b2, a2) = run(&exe, &[]);
        let (g1, g2) = (a1.saturating_sub(b1), a2.saturating_sub(b2));
        eprintln!(
            "REPORT large_region_resident {tier}: before {b1} kB after {a1} kB (grew {g1}); again {b2} -> {a2} (grew {g2}); margin {m} kB"
        );
        assert!(
            g1.abs_diff(g2) <= m,
            "#1175 {tier}: the same binary read twice grew {g1} and {g2} kB (A/A)"
        );
        assert!(
            g1 <= m,
            "#1175 {tier}: after two 2 MiB regions in turn the witness holds {g1} kB more than before the first (margin {m} kB)"
        );
    }
}

#[test]
fn the_users_threshold_is_left_alone() {
    if !on_glibc() {
        eprintln!(
            "SKIP the_users_threshold_is_left_alone: linux-gnu only (the threshold is glibc's)"
        );
        return;
    }
    let exe = build("user", "native");
    let (b, a) = run(
        &exe,
        &[
            ("MALLOC_MMAP_THRESHOLD_", "4194304"),
            ("MALLOC_TRIM_THRESHOLD_", "67108864"),
        ],
    );
    let grew = a.saturating_sub(b);
    eprintln!(
        "REPORT large_region_resident user thresholds mmap 4194304 trim 67108864: before {b} kB after {a} kB (grew {grew})"
    );
    assert!(
        grew >= 1024,
        "#1175: with the user's MALLOC_MMAP_THRESHOLD_=4194304 and MALLOC_TRIM_THRESHOLD_=67108864 the 2 MiB buffers come from the heap and stay, but the witness grew only {grew} kB: the runtime overrode the user's threshold"
    );
}
