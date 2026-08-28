//! s125 — the trap names its site: one witness, all three lanes.
//!
//! A native trap reports TWO stderr lines: the first is the parsed
//! machine contract (`wolf-trap: <kind>` — byte-identical to what it
//! has been since s28; both harness parsers take the whole remainder
//! as the kind), the second is new and additive —
//! `  at <file>:<line>:<col>` — pointing at the trap SITE (the
//! statement whose check fired), never the enclosing function.
//!
//! The witness program puts the trapping expression AT the statement
//! head (a tail expression), so statement-grain and expression-grain
//! coincide and the asserted line:col is the true trap expression's
//! own position. Asserted on:
//! - the debug tier (`wolf build`, Cranelift): exact two-line stderr;
//! - the release tier (`wolf build --release`, LLVM): the same bytes;
//! - the checked lane (`conform-run --checked --json`): the recorded
//!   `x-trap-span` resolves to the SAME line:col;
//! - both native conform-run lanes: the `trap(bounds)` verdict is
//!   UNCHANGED — the harness tolerates (ignores) the site line.
//!
//! Off-target the lanes skip loudly (the s28 posture); the linux CI
//! lane proves them.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The witness source. Line 6 (1-based), column 5: the tail expression
/// `xs[7]` is its own statement, so the statement's span — what the
/// trap check carries — IS the trapping expression's position.
const SRC: &str = "//! check: run(exit=trap(bounds))\n\
                   //! phase: run\n\
                   fn main() -> !int {\n    \
                       var xs = List[int]()\n    \
                       (mut xs).push(1)\n    \
                       xs[7]\n\
                   }\n";
const TRAP_LINE: u64 = 6;
const TRAP_COL: u64 = 5;

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

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// Build with the given tier and run the binary; returns stderr and
/// the exit code, or `None` when the environment cannot build this
/// tier (loud skip — the linux CI lane provisions both).
fn build_and_run(dir: &Path, release: bool) -> Option<(String, Option<i32>)> {
    let src = dir.join("trap_site.lu");
    std::fs::write(&src, SRC).expect("write witness");
    let exe = dir.join("witness");
    let mut cmd = Command::new(wolf());
    cmd.current_dir(dir)
        .args(["build", "trap_site.lu", "-o"])
        .arg(&exe);
    if release {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf runs");
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        assert!(
            msg.contains("cannot compile this yet") || msg.contains("not found"),
            "wolf build failed for a non-environment reason:\n{msg}"
        );
        eprintln!(
            "SKIP: environment cannot build (release={release}): {}",
            msg.trim()
        );
        return None;
    }
    let run = Command::new(&exe).output().expect("witness runs");
    Some((
        String::from_utf8_lossy(&run.stderr).into_owned(),
        run.status.code(),
    ))
}

/// The two-line report: first line byte-identical to the pre-s125
/// contract, second line the site, nothing else.
fn assert_site_report(stderr: &str, code: Option<i32>, tier: &str) {
    assert_eq!(code, Some(134), "{tier}: trap exit code");
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("wolf-trap: bounds"),
        "{tier}: the first line is the parsed ABI, byte-identical:\n{stderr}"
    );
    let line2 = lines.get(1).copied().unwrap_or_default();
    assert!(
        line2.starts_with("  at "),
        "{tier}: the site line's shape is `  at <file>:<line>:<col>`:\n{stderr}"
    );
    assert!(
        line2.ends_with(&format!("trap_site.lu:{TRAP_LINE}:{TRAP_COL}")),
        "{tier}: the site line names the trap expression (its display \
         path may carry a `./`):\n{stderr}"
    );
    assert_eq!(
        lines.len(),
        2,
        "{tier}: exactly two report lines:\n{stderr}"
    );
}

#[test]
fn native_trap_names_its_site_debug_tier() {
    ensure_rt_staticlib();
    let dir = scratch("trap_site_debug");
    if let Some((stderr, code)) = build_and_run(&dir, false) {
        assert_site_report(&stderr, code, "debug");
    }
}

#[test]
fn native_trap_names_its_site_release_tier() {
    ensure_rt_staticlib();
    let dir = scratch("trap_site_release");
    if let Some((stderr, code)) = build_and_run(&dir, true) {
        assert_site_report(&stderr, code, "release");
    }
}

/// The checked lane records the SAME site: `x-trap-span`'s start
/// resolves to the identical line:col through the identical
/// line-start arithmetic.
#[test]
fn checked_lane_records_the_same_site() {
    let dir = scratch("trap_site_checked");
    let src = dir.join("trap_site.lu");
    std::fs::write(&src, SRC).expect("write witness");
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(&src)
        .args(["--checked", "--json"])
        .output()
        .expect("wolf runs");
    assert!(
        out.status.success(),
        "conform-run --checked failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("record parses");
    assert_eq!(rec["verdict"].as_str(), Some("trap(bounds)"));
    let lo = rec["x-trap-span"][0].as_u64().expect("x-trap-span") as usize;
    // Resolve lo to 1-based line:col over the source bytes.
    let pre = &SRC.as_bytes()[..lo];
    let line = pre.iter().filter(|&&b| b == b'\n').count() as u64 + 1;
    let col = (lo - pre.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1)) as u64 + 1;
    assert_eq!(
        (line, col),
        (TRAP_LINE, TRAP_COL),
        "the checked lane's trap span starts at the same site the native line names"
    );
}

/// A line-shifting edit must move the reported site even when every
/// body hash is unchanged (the release tier's cluster cache keys
/// span-free canonical WIR — s125 added the site component so a
/// cached object can never serve stale coordinates).
#[test]
fn cached_release_rebuild_reports_the_shifted_site() {
    ensure_rt_staticlib();
    let dir = scratch("trap_site_cache");
    let src = dir.join("trap_site.lu");
    std::fs::write(&src, SRC).expect("write witness");
    let exe = dir.join("witness");
    // Two cached builds: the original, then the same program shifted
    // one line down by a comment line (bodies identical, spans moved).
    let build = |expect_line: u64| {
        let out = Command::new(wolf())
            .current_dir(&dir)
            .args(["build", "trap_site.lu", "--release", "-o"])
            .arg(&exe)
            .output()
            .expect("wolf runs");
        if !out.status.success() {
            let msg = String::from_utf8_lossy(&out.stderr);
            assert!(
                msg.contains("cannot compile this yet") || msg.contains("not found"),
                "wolf build failed for a non-environment reason:\n{msg}"
            );
            eprintln!(
                "SKIP: environment cannot build the release tier: {}",
                msg.trim()
            );
            return false;
        }
        let run = Command::new(&exe).output().expect("witness runs");
        let stderr = String::from_utf8_lossy(&run.stderr);
        let line2 = stderr.lines().nth(1).unwrap_or_default();
        assert!(
            line2.ends_with(&format!("trap_site.lu:{expect_line}:{TRAP_COL}")),
            "cached rebuild must resolve the shifted site (expected line \
             {expect_line}):\n{stderr}"
        );
        true
    };
    if !build(TRAP_LINE) {
        return;
    }
    std::fs::write(
        &src,
        format!("// shifted one line (s125 cache-key witness)\n{SRC}"),
    )
    .expect("rewrite witness");
    build(TRAP_LINE + 1);
}

/// The harness parsers tolerate (and ignore) the site line: the
/// conform-run verdict on both native lanes is the unchanged
/// `trap(bounds)` — kind recovered from the first line alone.
#[test]
fn conform_run_verdicts_are_unchanged_by_the_site_line() {
    ensure_rt_staticlib();
    for flag in ["--native", "--release"] {
        let dir = scratch(&format!("trap_site_verdict{flag}"));
        let src = dir.join("trap_site.lu");
        std::fs::write(&src, SRC).expect("write witness");
        let out = Command::new(wolf())
            .arg("conform-run")
            .arg(&src)
            .args([flag, "--json"])
            .output()
            .expect("wolf runs");
        if out.status.code() == Some(2) {
            eprintln!(
                "SKIP: environment cannot run {flag}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            continue;
        }
        assert!(
            out.status.success(),
            "conform-run {flag} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("record parses");
        assert_eq!(
            rec["verdict"].as_str(),
            Some("trap(bounds)"),
            "{flag}: the site line must not disturb the trap verdict"
        );
    }
}
