//! The D5 compile-time-track bench (s13): bodies-checked-per-second
//! over the corpus.
//!
//! Resolves every corpus entry file once (parsing and resolution are
//! not measured), then times N full type-checking passes over all
//! packages and prints one JSON line:
//!
//! ```text
//! {"bodies_per_sec": 12345.6, "bodies": 42}
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
        eprintln!("bodies_bench: no corpus entries under {}", corpus.display());
        std::process::exit(1);
    }
    // Resolve once per entry; only clean-resolving packages are timed
    // (a package that fails resolution never reaches the checker).
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
    // Warm-up + body count.
    let mut bodies = 0usize;
    for res in &packages {
        let tc = typecheck_package_with(&res.package, false);
        bodies += tc.bodies.len();
    }
    // Timed passes: enough iterations for a stable wall-clock read.
    const ITERS: usize = 40;
    let t = Instant::now();
    for _ in 0..ITERS {
        for res in &packages {
            let tc = typecheck_package_with(&res.package, false);
            std::hint::black_box(tc.bodies.len());
        }
    }
    let elapsed = t.elapsed().as_secs_f64();
    let checked = (bodies * ITERS) as f64;
    let bps = if elapsed > 0.0 {
        checked / elapsed
    } else {
        0.0
    };
    // D9 (s17): single-body re-check latency — the per-body path an
    // editor edit takes (signatures cached, one body re-checked).
    // Measured on the largest checked body in the corpus.
    let mut best: Option<(usize, wolf_sema::BodyRef)> = None;
    let mut best_n = 0usize;
    for (pi, res) in packages.iter().enumerate() {
        let tc = typecheck_package_with(&res.package, false);
        for b in &tc.bodies {
            if let wolf_sema::BodyResult::Checked(tb) = &b.result
                && tb.exprs.len() > best_n
            {
                best_n = tb.exprs.len();
                best = Some((pi, b.body.clone()));
            }
        }
    }
    let recheck_us = match best {
        Some((pi, body)) => {
            let pkg = &packages[pi].package;
            let sigs = wolf_sema::build_sigs(pkg);
            const RECHECKS: usize = 200;
            let t = Instant::now();
            for _ in 0..RECHECKS {
                std::hint::black_box(wolf_sema::check_body(pkg, &sigs, &body));
            }
            t.elapsed().as_secs_f64() * 1e6 / RECHECKS as f64
        }
        None => 0.0,
    };
    println!(
        "{{\"bodies_per_sec\": {bps:.1}, \"bodies\": {bodies}, \
         \"single_body_recheck_us\": {recheck_us:.1}}}"
    );
}
