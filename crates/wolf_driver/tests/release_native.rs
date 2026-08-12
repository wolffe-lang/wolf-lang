//! s41 acceptance — the release tier at exit-parity with the debug
//! tier over the ENTIRE run-phase corpus.
//!
//! Every `phase: run` corpus file executes through `conform-run` on
//! both native lanes — `--native` (wir → Cranelift → cc, the debug
//! tier) and `--release` (wir → LLVM -O2 → clang, the s41 tier) — and
//! must produce the IDENTICAL verdict and stdout hash. This is the
//! release column of the corpus ledger: checked-lane semantics are
//! preserved bit-for-bit (X3 is profile-independent — checked
//! arithmetic traps identically in release; only the speed changes).
//!
//! Off-target (or without clang/cc) the lanes skip loudly; the linux
//! CI lane provisions both, so a silent skip there is a lane bug.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

fn collect_lu(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    items.sort();
    for p in items {
        if p.is_dir() {
            collect_lu(&p, out);
        } else if p.extension().is_some_and(|e| e == "lu") {
            out.push(p);
        }
    }
}

/// Header scan: `phase: run` files that are not `member: true`.
fn is_run_entry(src: &str) -> bool {
    let mut run = false;
    for line in src.lines() {
        let Some(rest) = line.trim_start().strip_prefix("//!") else {
            break;
        };
        let rest = rest.trim();
        if rest.strip_prefix("phase:").map(str::trim) == Some("run") {
            run = true;
        }
        if rest.strip_prefix("member:").map(str::trim) == Some("true") {
            return false;
        }
    }
    run
}

/// One conform-run lane: (verdict, stdout sha). `None` only for
/// environment exit-2 (no cc / no clang / no rt staticlib) — loudly.
fn lane(file: &Path, flag: &str) -> Option<(String, String)> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(file)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) {
        eprintln!(
            "SKIP: environment cannot run {flag} on {}: {}",
            file.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {}: {}",
        file.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    Some((
        rec["verdict"].as_str().unwrap_or("").to_string(),
        rec["stdout_sha256"].as_str().unwrap_or("").to_string(),
    ))
}

/// The whole run-phase corpus, both tiers, one behavior.
#[test]
fn release_tier_matches_debug_tier_on_the_corpus() {
    let root = corpus_root();
    assert!(root.is_dir(), "corpus directory exists");
    let mut files = Vec::new();
    collect_lu(&root, &mut files);
    let runnable: Vec<PathBuf> = files
        .into_iter()
        .filter(|f| {
            std::fs::read_to_string(f)
                .map(|s| is_run_entry(&s))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        runnable.len() >= 50,
        "the run-phase corpus should be substantial (found {})",
        runnable.len()
    );
    let mut checked = 0usize;
    let mut divergences = Vec::new();
    for f in &runnable {
        let Some(debug) = lane(f, "--native") else {
            return; // environment skip (loud)
        };
        let Some(release) = lane(f, "--release") else {
            return;
        };
        checked += 1;
        if debug != release {
            divergences.push(format!(
                "{}: debug={debug:?} release={release:?}",
                f.display()
            ));
        }
    }
    assert!(
        divergences.is_empty(),
        "release tier diverges from debug tier on {} of {checked} file(s):\n{}",
        divergences.len(),
        divergences.join("\n")
    );
    eprintln!("release parity: {checked} corpus file(s), identical verdict + stdout sha");
}
