//! The s25 compile-track bench: WIR instructions built per second over
//! the corpus, plus the peephole hit-rate counters (the Click §5 claim,
//! measured — never assumed).
//!
//! Front-end work (parse/resolve/typecheck) runs once and is NOT
//! measured; the timed region is `lower_package` alone. Prints one
//! JSON line:
//!
//! ```text
//! {"wir_insts_per_sec": 123456.7, "wir_insts": 42,
//!  "wir_fold_hits": 3, "wir_identity_hits": 1, "wir_gvn_hits": 2,
//!  "wir_instantiations_seen": 3, "wir_instantiations_lowered": 2,
//!  "wir_forward_hits": 0}
//! ```
//!
//! `cargo xtask bench --track=compile` wires this into the record
//! stream.

use std::path::{Path, PathBuf};
use std::time::Instant;

use wolf_sema::{AliasTable, DiskLoader, Resolution, resolve_package_with, typecheck_package_with};

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
        eprintln!(
            "wir_build_bench: no corpus entries under {}",
            corpus.display()
        );
        std::process::exit(1);
    }
    // Front-end once; keep only packages that resolve + typecheck fully
    // (the builder consumes checked bodies; refusals are fine — they
    // are part of the measured walk).
    let mut inputs: Vec<(Resolution, wolf_sema::Typecheck)> = Vec::new();
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
        if res
            .diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error)
        {
            continue;
        }
        let tc = typecheck_package_with(&res.package, false);
        if tc.has_errors() {
            continue;
        }
        inputs.push((res, tc));
    }
    // Warm-up + counter capture.
    let mut totals = wolf_wir::Stats::default();
    for (res, tc) in &inputs {
        let build = wolf_wir::lower_package(&res.package, tc);
        totals.add(build.stats);
    }
    // Timed passes.
    const ITERS: usize = 40;
    let t = Instant::now();
    for _ in 0..ITERS {
        for (res, tc) in &inputs {
            let build = wolf_wir::lower_package(&res.package, tc);
            std::hint::black_box(build.module.funcs.len());
        }
    }
    let elapsed = t.elapsed().as_secs_f64();
    let built = (totals.insts as usize * ITERS) as f64;
    let ips = if elapsed > 0.0 { built / elapsed } else { 0.0 };
    println!(
        "{{\"wir_insts_per_sec\": {ips:.1}, \"wir_insts\": {}, \
         \"wir_fold_hits\": {}, \"wir_identity_hits\": {}, \
         \"wir_gvn_hits\": {}, \"wir_forward_hits\": {}, \
         \"wir_instantiations_seen\": {}, \"wir_instantiations_lowered\": {}}}",
        totals.insts,
        totals.fold,
        totals.identity,
        totals.gvn,
        totals.forward,
        totals.instantiations_seen,
        totals.instantiations_lowered
    );
}
