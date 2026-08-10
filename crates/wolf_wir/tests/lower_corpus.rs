//! The s25 lowering gates over the real corpus:
//!
//! - **Golden WIR snapshots** for a representative subset (reviewed
//!   artifacts — the shape of the builder's output is a contract).
//! - **The round-trip gate**: every lowered function verifies, prints
//!   canonically, and reparses to a byte-identical fixpoint.
//! - **The determinism gate** (CI): two independent builder runs over
//!   every lowerable corpus entry print byte-identically (D8 hashing
//!   and snapshot stability depend on it).
//!
//! Only mem-clean files lower (the `wir` rung sits after `mem`); files
//! the lowerer refuses stay in the conservatism ledger and are listed
//! in a snapshot so refusal-surface changes are reviewed too.

use std::path::{Path, PathBuf};

use wolf_sema::{AliasTable, DiskLoader, Resolution, resolve_package_with, typecheck_package_with};
use wolf_wir::{lower_package, print_module, verify_module};

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

/// Run the ladder up to `mem`; `Some` only for fully-checked,
/// fully-clean packages (the wir rung's precondition).
fn mem_clean(entry: &Path) -> Option<(Resolution, wolf_sema::Typecheck)> {
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
    Some((res, tc))
}

/// Lower one entry: `Ok(canonical dump)` when every body lowered (and
/// verified, and reached print→parse→print fixpoint), `Err(refusals)`
/// otherwise.
fn lower_entry(entry: &Path) -> Option<Result<String, Vec<String>>> {
    let (res, tc) = mem_clean(entry)?;
    let build = lower_package(&res.package, &tc);
    if !build.not_yet.is_empty() {
        return Some(Err(build
            .not_yet
            .iter()
            .map(|n| n.construct.to_string())
            .collect()));
    }
    verify_module(&build.module).expect("lowered module verifies");
    let printed = print_module(&build.module);
    let reparsed = wolf_wir::parse_module(&printed).expect("canonical dump reparses");
    verify_module(&reparsed).expect("reparsed module verifies");
    assert_eq!(
        print_module(&reparsed),
        printed,
        "print → parse → print fixpoint for {}",
        entry.display()
    );
    Some(Ok(printed))
}

fn golden(name: &str) {
    let path = corpus_root().join(name);
    let out = match lower_entry(&path) {
        None => panic!("{name} is not mem-clean; golden inputs must reach the wir rung"),
        Some(Ok(dump)) => dump,
        Some(Err(refusals)) => panic!("{name} refused: {refusals:?}"),
    };
    insta::assert_snapshot!(name.replace('/', "_"), out);
}

// ---------------------------------------------------------- goldens ----

#[test]
fn golden_memory_div_zero() {
    golden("memory/div_zero.lu");
}

#[test]
fn golden_faults_div_zero_rem() {
    golden("faults/div_zero_rem.lu");
}

#[test]
fn golden_faults_overflow_add() {
    golden("faults/overflow_add.lu");
}

#[test]
fn golden_faults_assert_fails() {
    golden("faults/assert_fails.lu");
}

#[test]
fn golden_grammar_else_chain() {
    golden("grammar/else_chain.lu");
}

#[test]
fn golden_grammar_bang_not() {
    golden("grammar/bang_not.lu");
}

#[test]
fn golden_typecheck_cast_set() {
    golden("typecheck/cast_set.lu");
}

#[test]
fn golden_resolve_two_mod() {
    golden("resolve/two_mod/main.lu");
}

#[test]
fn golden_resolve_forward() {
    golden("resolve/forward/main.lu");
}

// ---- the s26 memory-surface goldens (fact-fidelity witnesses) ----

/// The freeze flow: `region(rc)` value form, `in r { }`, `sync.freeze`
/// with its op-justified `frozen` fact. The s26 demo artifact.
#[test]
fn golden_memory_region_freeze_ok() {
    golden("memory/region_freeze_ok.lu");
}

/// Field-granular `mut` args: stack.alloc spill slots with op-derived
/// region/deref facts and `excl.field` noalias edges — plus the
/// callee's `excl.mut` entry facts (the hand-`restrict`-C killer).
#[test]
fn golden_memory_excl_disjoint_ok() {
    golden("memory/excl_disjoint_ok.lu");
}

/// Sibling region values with interleaved use (the multiopen
/// antichain), lowered to region.new pairs.
#[test]
fn golden_memory_region_multiopen_values_ok() {
    golden("memory/region_multiopen_values_ok.lu");
}

/// Aggregate-heavy tree transform: struct literals and member reads as
/// by-value `agg.make`/`agg.get` (the register-promotion story).
#[test]
fn golden_memory_region_infer_tree_transform() {
    golden("memory/region_infer_tree_transform.lu");
}

/// The unsigned checked family (`u*.chk`) with the X3 claw-back: a
/// remainder by a positive constant carries its `range` postcondition
/// as a verified `: op` fact.
#[test]
fn golden_memory_checked_unsigned() {
    golden("memory/checked_unsigned.lu");
}

// ------------------------------------------------- the whole corpus ----

/// Which files lower today. Snapshot-pinned: every advance or retreat
/// of the lowering surface is a reviewed diff, exactly like the corpus
/// phase ledger it feeds.
#[test]
fn corpus_lowering_ledger() {
    let root = corpus_root();
    let mut entries = Vec::new();
    collect_entries(&root, &mut entries);
    assert!(!entries.is_empty(), "corpus present");
    let mut lines = Vec::new();
    for entry in &entries {
        let rel = entry
            .strip_prefix(&root)
            .unwrap_or(entry)
            .display()
            .to_string()
            .replace('\\', "/");
        match lower_entry(entry) {
            None => lines.push(format!("{rel}: not mem-clean (pre-wir rungs own it)")),
            Some(Ok(_)) => lines.push(format!("{rel}: lowers")),
            Some(Err(refusals)) => {
                lines.push(format!("{rel}: refused — {}", refusals.join("; ")));
            }
        }
    }
    insta::assert_snapshot!("corpus_lowering_ledger", lines.join("\n"));
}

/// The determinism gate: two independent runs, byte-identical dumps.
#[test]
fn corpus_dumps_are_deterministic() {
    let root = corpus_root();
    let mut entries = Vec::new();
    collect_entries(&root, &mut entries);
    for entry in &entries {
        let a = lower_entry(entry);
        let b = lower_entry(entry);
        match (a, b) {
            (Some(Ok(x)), Some(Ok(y))) => {
                assert_eq!(x, y, "byte-identical dumps for {}", entry.display());
            }
            (Some(Err(x)), Some(Err(y))) => assert_eq!(x, y),
            (None, None) => {}
            _ => panic!("nondeterministic lowering verdict for {}", entry.display()),
        }
    }
}
