//! s162 — #146's stale half, from source: the release tier's overlap
//! guard must be built from the len CURRENT at the loop entry.
//!
//! `shape` grows `x` by in-place `push` and then writes `x[i]` from
//! `c[i]` twice around the store. The indexed loop versions with an
//! overlap guard; at trunk 30731a6 the guard's extent for `x` was the
//! len load ABOVE the first push (it dominates the guard, so no
//! verifier objects), i.e. 0, and an empty extent passes the guard
//! whatever the buffers are.
//!
//! The two lists share storage here only because `copy` of a List is
//! the operand itself on the native tiers (#384 — the second of #93's
//! "accidents" is gone), so `shape(copy c, c)` aliases. On trunk:
//! `--native` printed `sum 72 72` (the aliasing semantics, no fast
//! path), `--release` printed `sum 64 64` (the fast loop under a false
//! noalias fact, clang forwarding `c[i]` across the store to `x[i]`).
//! The checked lane deep-copies and traps `c[0]` on the empty list,
//! which is #384's to reconcile, so this test holds the release tier
//! to the NATIVE tier only — a pin that stays true after #384 lands
//! (both native tiers then trap too).
//!
//! Skips follow the s59 posture: environment exit-2 and the release
//! tier's named host refusal are loud skips, never verdicts.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const SRC: &str = "fn shape(x0: List[int], c: List[int]) -> List[int] {\n\
\x20   var x = x0\n\
\x20   (mut x).push(0)\n\
\x20   var i: int = 1\n\
\x20   while i < 8 {\n\
\x20       (mut x).push(i)\n\
\x20       i = i + 1\n\
\x20   }\n\
\x20   i = 0\n\
\x20   while i < 8 {\n\
\x20       x[i] = c[i] + 1\n\
\x20       x[i] = x[i] + c[i]\n\
\x20       i = i + 1\n\
\x20   }\n\
\x20   x\n\
}\n\
\n\
fn total(xs: List[int]) -> int {\n\
\x20   var s: int = 0\n\
\x20   for v in xs {\n\
\x20       s = s + v\n\
\x20   }\n\
\x20   s\n\
}\n\
\n\
fn main() -> int {\n\
\x20   var c = List[int]()\n\
\x20   let out = shape(copy c, c)\n\
\x20   var d = List[int]()\n\
\x20   let out2 = shape(copy d, d)\n\
\x20   print(\"sum {total(out)} {total(out2)}\")\n\
\x20   0\n\
}\n";

fn fixture() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("release_overlap_guard");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("prog.lu"), SRC).expect("write witness");
    dir
}

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

/// One conform-run lane: (verdict, stdout), `None` on a loud skip.
fn lane(src: &Path, flag: &str) -> Option<(String, String)> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(src)
        .args([flag, "--json"])
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) {
        eprintln!(
            "SKIP: environment cannot run {flag}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("record parses");
    if rec["verdict"] == "unsupported"
        && String::from_utf8_lossy(&out.stderr).contains("release tier targets")
    {
        eprintln!("SKIP: the release tier refuses this host");
        return None;
    }
    Some((
        rec["verdict"].as_str().unwrap_or("").to_string(),
        rec["stdout_inline"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| rec["stdout_sha256"].as_str().unwrap_or("").to_string()),
    ))
}

#[test]
fn release_overlap_guard_agrees_with_native_on_a_grown_list() {
    ensure_rt_staticlib();
    let dir = fixture();
    let src = dir.join("prog.lu");
    let Some(native) = lane(&src, "--native") else {
        return;
    };
    let Some(release) = lane(&src, "--release") else {
        return;
    };
    assert_eq!(
        release, native,
        "wolf-lang#146: the release tier's versioned loop disagrees with the \
         native tier — an overlap guard built from a stale len lets the fast \
         path run over storage the guard should have seen overlap"
    );
}
