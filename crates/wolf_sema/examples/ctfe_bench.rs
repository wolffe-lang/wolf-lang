//! The D5 compile-time-track CTFE bench (s16): comptime evaluations
//! per second and the memoization hit rate over the corpus.
//!
//! Resolves every corpus entry file once (parsing and resolution are
//! not measured), then times N full type-checking passes — each of
//! which runs the package-level comptime pass — and prints one JSON
//! line:
//!
//! ```text
//! {"ctfe_evals": 42, "ctfe_memo_hit_rate": 0.61, "ctfe_evals_per_sec": 1234.5}
//! ```
//!
//! `cargo xtask bench` wires this into the bench record stream.

use std::path::{Path, PathBuf};
use std::time::Instant;

use wolf_sema::{AliasTable, DiskLoader, Resolution, resolve_package_with, typecheck_package_with};

/// Corpus-header check: is this a `member: true` file (compiled through
/// its entry, never an entry itself)?
fn is_member_file(src: &[u8]) -> bool {
    let text = String::from_utf8_lossy(src);
    for line in text.lines() {
        let Some(rest) = line.trim_start().strip_prefix("//!") else {
            break;
        };
        if let Some(v) = rest.trim().strip_prefix("member:")
            && v.trim() == "true"
        {
            return true;
        }
    }
    false
}

fn collect_entries(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    items.sort();
    for p in items {
        if p.is_dir() {
            collect_entries(&p, out);
        } else if p.extension().is_some_and(|e| e == "lu")
            && std::fs::read(&p).is_ok_and(|src| !is_member_file(&src))
        {
            out.push(p);
        }
    }
}

fn main() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let corpus = corpus.canonicalize().unwrap_or(corpus);
    let mut entries = Vec::new();
    collect_entries(&corpus, &mut entries);
    if entries.is_empty() {
        eprintln!("ctfe_bench: no corpus entries under {}", corpus.display());
        std::process::exit(1);
    }
    // Resolve once per entry; only clean-resolving packages reach the
    // checker (and through it, the comptime pass).
    let mut packages: Vec<Resolution> = Vec::new();
    for entry in &entries {
        let mut sm = wolf_span::SourceMap::new();
        let Some(mut loader) =
            DiskLoader::from_entry(entry, &mut sm, Box::new(|src: &[u8]| is_member_file(src)))
        else {
            continue;
        };
        let Ok(res) = resolve_package_with(&mut loader, &AliasTable::default(), false) else {
            continue;
        };
        let has_errors = res
            .diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error);
        if !has_errors {
            packages.push(res);
        }
    }
    // Warm-up pass: the per-pass eval count and the memo hit rate
    // (each pass runs a fresh engine, so warm-up equals steady state).
    let mut evals = 0u64;
    let mut hits = 0u64;
    let mut misses = 0u64;
    for res in &packages {
        let tc = typecheck_package_with(&res.package, false);
        evals += tc.ctfe.evals;
        hits += tc.ctfe.memo_hits;
        misses += tc.ctfe.memo_misses;
    }
    let hit_rate = if hits + misses == 0 {
        0.0
    } else {
        hits as f64 / (hits + misses) as f64
    };
    // Timed passes. The comptime corpus deliberately includes budget
    // exhaustion (a full fuel budget burned per pass), so a handful of
    // iterations is a stable read without minutes of spinning.
    const ITERS: usize = 5;
    let t = Instant::now();
    for _ in 0..ITERS {
        for res in &packages {
            let tc = typecheck_package_with(&res.package, false);
            std::hint::black_box(tc.ctfe.evals);
        }
    }
    let elapsed = t.elapsed().as_secs_f64();
    let timed = (evals * ITERS as u64) as f64;
    let eps = if elapsed > 0.0 { timed / elapsed } else { 0.0 };
    println!(
        "{{\"ctfe_evals\": {evals}, \"ctfe_memo_hit_rate\": {hit_rate:.4}, \
         \"ctfe_evals_per_sec\": {eps:.1}}}"
    );
}
