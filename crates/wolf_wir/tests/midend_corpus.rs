//! s42 corpus-level mid-end gates:
//!
//! - **The elimination-rate acceptance** (X3's empirical defense,
//!   amendment 3's ≥80% backstop): over the checked-arith kernel tier
//!   (`corpus/kernels/`), at least 80% of overflow checks inside
//!   natural loops are statically eliminated — directly by π ranges
//!   or by the loop-versioning fast copy. The rate is printed so the
//!   per-commit bench lane can scrape it.
//! - **Conc conservatism, structurally**: optimizing a spawn-bearing
//!   corpus module keeps every schedulable entry's runtime-seam
//!   (`__wolf_rt_*`) sequence identical in count and order, and holds
//!   every seam-touching region back from promotion and coalescing —
//!   schedule points are barriers, and no pass may add, drop, or
//!   reorder them. Its behavioral half (same seed ⇒ same verdict and
//!   stdout with the mid-end on) is `conc_native.rs`'s
//!   `mid_end_does_not_change_seeded_conc_behavior`, which needs the
//!   native rung.
//! - **The full-corpus mid-end sweep**: every lowerable corpus module
//!   optimizes with verify-each-pass ON and stays print→parse→print
//!   canonical — the optimizer's own round-trip discipline.

use std::path::{Path, PathBuf};

use wolf_sema::{AliasTable, DiskLoader, Resolution, resolve_package_with, typecheck_package_with};
use wolf_wir::midend::{Options, optimize_module};
use wolf_wir::{lower_package, parse_module, print_module, verify_module};

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

fn corpus_root() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    p.canonicalize().unwrap_or(p)
}

fn resolve(entry: &Path) -> Option<Resolution> {
    let mut sm = wolf_span::SourceMap::new();
    let mut loader =
        DiskLoader::from_entry(entry, &mut sm, Box::new(|src: &[u8]| is_member_file(src)))?;
    let res = resolve_package_with(&mut loader, &AliasTable::default(), true).ok()?;
    if res
        .diagnostics
        .iter()
        .any(|d| d.severity == wolf_diag::Severity::Error)
    {
        return None;
    }
    Some(res)
}

/// Lower one mem-clean entry to a verified module.
fn lower(entry: &Path) -> Option<wolf_wir::Module> {
    let res = resolve(entry)?;
    let tc = typecheck_package_with(&res.package, true);
    if !tc.not_yet.is_empty() || tc.has_errors() {
        return None;
    }
    let mem = wolf_mem::check_package(&res.package, &tc);
    if !mem.not_yet.is_empty()
        || mem
            .diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error)
    {
        return None;
    }
    let build = lower_package(&res.package, &tc);
    if !build.not_yet.is_empty() {
        return None;
    }
    verify_module(&build.module).expect("lowered module verifies");
    Some(build.module)
}

fn opts() -> Options {
    Options {
        verify_each: true,
        ..Options::default()
    }
}

/// The s42 acceptance gate: ≥80% of hot-loop overflow checks on the
/// kernel tier are statically eliminated. Reported, then gated.
#[test]
fn kernel_tier_elimination_rate_is_at_least_80_percent() {
    let dir = corpus_root().join("kernels");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("kernel tier present")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "lu"))
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "kernel tier has files");
    let mut seen = 0usize;
    let mut eliminated = 0usize;
    for entry in &entries {
        let mut module = lower(entry)
            .unwrap_or_else(|| panic!("kernel {} must reach the wir rung", entry.display()));
        let stats = optimize_module(&mut module, &opts()).expect("pipeline green");
        eprintln!(
            "kernel {}: {}/{} hot-loop checks eliminated, {} versioned",
            entry.file_name().unwrap_or_default().to_string_lossy(),
            stats.loop_checks_eliminated,
            stats.loop_checks_seen,
            stats.loops_versioned
        );
        seen += stats.loop_checks_seen;
        eliminated += stats.loop_checks_eliminated;
    }
    assert!(seen > 0, "the tier exercises checks in loops");
    let rate = eliminated as f64 / seen as f64;
    eprintln!(
        "kernel tier elimination rate: {eliminated}/{seen} = {:.1}%",
        rate * 100.0
    );
    assert!(
        rate >= 0.80,
        "the X3 claw-back acceptance: {eliminated}/{seen} = {:.1}% < 80%",
        rate * 100.0
    );
}

/// **Conc conservatism, structurally** (s73 seam, spec/07).
///
/// Every `__wolf_rt_*` call is a potential schedule point, and a spawn
/// edge is a call. Optimizing a spawn/channel/select module must leave
/// the schedule-point skeleton exactly as lowering built it:
///
/// 1. **The seam sequence is invariant** — same seams, same count, same
///    order, nothing added, dropped, duplicated, or reordered. The
///    comparison drops the HOST FUNCTION name because inlining a direct
///    call legitimately relocates a seam's host (a task body inlines
///    into its own entry shim) without moving it past anything; the
///    call's *position in the global order* is what conservatism owns.
///    No other pass can move a `call` at all: LICM hoists only
///    `is_removable` ops (a call never is), simplify's DCE and GVN
///    cannot delete or hash-cons one, and memopt touches loads/stores.
/// 2. **Nothing promotes or coalesces around a seam** — every region in
///    these modules has its handle or its pointers reach a runtime call
///    (`chan_send_region`, `region_adopt`, `scope_spawn`), so the
///    sink pass's escape rule and the coalesce pass's no-call-between
///    rule must hold every one of them back: both counters read zero.
///    The positive controls that keep this non-vacuous are the unit
///    fixtures (`sink_promotes_small_region_in_loop`,
///    `coalesce_fuses_adjacent_allocs`) and their paired barrier tests
///    (`sink_rejects_unbounded_and_escaping`,
///    `coalesce_never_crosses_a_call`) in `midend_passes.rs`.
///
/// The behavioral half of this litmus — the same file running to the
/// same verdict and stdout under a fixed `--seed` with the mid-end on —
/// lives in `wolf_driver`'s `conc_native.rs`
/// (`mid_end_does_not_change_seeded_conc_behavior`), because it needs
/// the native rung.
#[test]
fn conc_seams_survive_optimization_in_order() {
    let candidates = [
        "conc/select_two_timeouts.lu",
        "conc/message_passing.lu",
        "conc/select_seeded.lu",
        "conc/cancel_sibling.lu",
        "conc/freeze_publish.lu",
    ];
    let mut tested = 0;
    for rel in candidates {
        let entry = corpus_root().join(rel);
        let Some(mut module) = lower(&entry) else {
            continue; // pre-wir rungs own it today
        };
        let before = seam_skeleton(&module);
        let total: usize = before.iter().map(|(_, s)| s.len()).sum();
        assert!(
            total > 0,
            "{rel} exercises runtime seams before optimization"
        );
        let stats = optimize_module(&mut module, &opts()).expect("pipeline green");
        let after = seam_skeleton(&module);
        assert_eq!(
            before.iter().map(|(r, _)| r).collect::<Vec<_>>(),
            after.iter().map(|(r, _)| r).collect::<Vec<_>>(),
            "{rel}: the set of schedulable entries changed — a task entry \
             was added or dropped"
        );
        for ((root, b), (_, a)) in before.iter().zip(&after) {
            assert_eq!(
                b, a,
                "{rel}: entry `{root}` — the schedule-point sequence changed \
                 (a seam was added, dropped, duplicated, or reordered)"
            );
        }
        assert_eq!(
            (stats.regions_promoted, stats.allocs_coalesced),
            (0, 0),
            "{rel}: a region whose handle or pointers reach a schedule point \
             was promoted or coalesced — the escape/no-call guards leaked"
        );
        eprintln!(
            "conc conservatism {rel}: {} entr(ies), {total} seam call(s) invariant, \
             0 promoted, 0 coalesced ({} inlined call site(s), {} -> {} insts)",
            after.len(),
            stats.inlined_calls,
            stats.insts_before,
            stats.insts_after
        );
        tested += 1;
    }
    assert!(
        tested >= 3,
        "the conc tier reaches the wir rung ({tested} module(s))"
    );
}

/// The module's **schedule-point skeleton**: for every schedulable
/// entry (`main`, exported functions, and every `func.addr` target —
/// i.e. every task/proc entry the runtime can start), the sequence of
/// `__wolf_rt_*` seams that entry can reach, in program order, with
/// internal calls expanded INLINE at their call site.
///
/// Inline expansion is what makes this comparison honest: the mid-end
/// is allowed to inline a direct call (which relocates the callee's
/// seams into the caller and can retire the callee entirely), and it is
/// allowed to reorder the module's function table under dead-function
/// elimination. It is NOT allowed to change what a schedulable entry
/// does, in what order. Expanding calls quotients out exactly the first
/// freedom while keeping the second property sharp.
///
/// Recursion is cut with a `recurse(f)` marker rather than unrolled —
/// still an order-sensitive token, so a pass that changed a recursive
/// callee's seams would still be caught at that callee's own entry.
fn seam_skeleton(m: &wolf_wir::Module) -> Vec<(String, Vec<String>)> {
    let mut roots: Vec<String> = m
        .funcs
        .values()
        .filter(|f| f.export || f.name == "main")
        .map(|f| f.name.clone())
        .collect();
    for f in m.funcs.values() {
        for &b in &f.layout {
            for &inst in &f.blocks[b].insts {
                if f.insts[inst].op == wolf_wir::Opcode::FuncAddr
                    && let wolf_wir::Aux::Callee(ef) = f.insts[inst].aux
                {
                    roots.push(f.ext_funcs[ef].name.clone());
                }
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
        .into_iter()
        .map(|r| {
            let mut seams = Vec::new();
            expand(m, &r, &mut Vec::new(), &mut seams);
            (r, seams)
        })
        .collect()
}

fn expand(m: &wolf_wir::Module, name: &str, stack: &mut Vec<String>, out: &mut Vec<String>) {
    let Some(f) = m.funcs.values().find(|f| f.name == name) else {
        return; // not an internal function: an extern the module imports
    };
    if stack.iter().any(|s| s == name) {
        out.push(format!("recurse({name})"));
        return;
    }
    stack.push(name.to_string());
    for &b in &f.layout {
        for &inst in &f.blocks[b].insts {
            if f.insts[inst].op != wolf_wir::Opcode::Call {
                continue;
            }
            let wolf_wir::Aux::Callee(ef) = f.insts[inst].aux else {
                continue;
            };
            let callee = f.ext_funcs[ef].name.clone();
            if callee.starts_with("__wolf_rt_") {
                out.push(callee);
            } else {
                expand(m, &callee, stack, out);
            }
        }
    }
    stack.pop();
}

/// The optimizer's round-trip discipline over everything lowerable:
/// optimize with verify-each-pass, then print → parse → print must be
/// a fixpoint (the D8 hash input stays canonical after the mid-end).
#[test]
fn optimized_corpus_stays_canonical() {
    let root = corpus_root();
    let mut entries = Vec::new();
    collect_lu(&root, &mut entries);
    let mut optimized = 0;
    let (mut before, mut after) = (0usize, 0usize);
    let (mut seen, mut elim) = (0usize, 0usize);
    for entry in &entries {
        let Some(mut module) = lower(entry) else {
            continue;
        };
        let stats = optimize_module(&mut module, &opts())
            .unwrap_or_else(|e| panic!("mid-end broke {}:\n{e}", entry.display()));
        before += stats.insts_before;
        after += stats.insts_after;
        seen += stats.loop_checks_seen;
        elim += stats.loop_checks_eliminated;
        let printed = print_module(&module);
        let reparsed = parse_module(&printed)
            .unwrap_or_else(|e| panic!("optimized dump reparses {}: {e:?}", entry.display()));
        verify_module(&reparsed).expect("reparsed optimized module verifies");
        assert_eq!(
            print_module(&reparsed),
            printed,
            "print → parse → print fixpoint after the mid-end: {}",
            entry.display()
        );
        optimized += 1;
    }
    eprintln!("mid-end canonical round-trip: {optimized} corpus module(s)");
    // Report-only corpus-wide evidence (the contract's IR-volume budget
    // is stated against the LLVM instruction count the backend RECEIVES,
    // which s44's harness measures end to end; this is the WIR-level
    // proxy the mid-end itself can account for, plus the whole-corpus
    // elimination rate beside the kernel tier's gated one).
    eprintln!(
        "mid-end corpus IR volume: {before} -> {after} WIR inst(s) ({:.1}% of naive lowering)",
        100.0 * after as f64 / before.max(1) as f64
    );
    if seen > 0 {
        eprintln!(
            "mid-end corpus hot-loop checks: {elim}/{seen} eliminated ({:.1}%)",
            100.0 * elim as f64 / seen as f64
        );
    }
    assert!(optimized >= 50, "the sweep is substantial ({optimized})");
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
        } else if p.extension().is_some_and(|e| e == "lu")
            && std::fs::read(&p).is_ok_and(|src| !is_member_file(&src))
        {
            out.push(p);
        }
    }
}

/// s102's loop-region CSE witness, gated so the merge cannot rot:
/// `walk_twice.lu` is e3's shape (a constant-depth recursion the
/// inliner unrolls into two identical sequential pure loops), and the
/// pipeline must merge the second onto the first. The exit value is
/// pinned by the fixture's own exit code.
#[test]
fn kernel_walk_twice_merges_its_unrolled_loops() {
    let entry = corpus_root().join("kernels").join("walk_twice.lu");
    let mut module = lower(&entry).expect("walk_twice reaches the wir rung");
    let stats = optimize_module(&mut module, &opts()).expect("pipeline green");
    assert!(
        stats.loops_cse >= 1,
        "the unrolled twin loop merges (loops_cse = {}): {stats}",
        stats.loops_cse
    );
}
