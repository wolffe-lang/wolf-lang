//! The survey lens ([`wolf_wir::lower_package_survey`], `cargo xtask
//! peel`): statement-level refusal collection behind the fail-fast
//! ledger. Two properties hold forever:
//!
//! - **The lens changes nothing.** The survey [`Build`] is bit-for-bit
//!   the fail-fast one — same `not_yet`, same module — because a
//!   surveyed body's verdict is pinned to its first reason and its
//!   garbage function is never added.
//! - **The lens is a superset.** Every ledger reason appears in the
//!   survey list as a `follow_on: false` entry; what the survey adds
//!   beyond that is what fail-fast masks.
//!
//! Survey output is NEVER snapshotted: it is a lens for contract
//! authors (the c19 closeout's lesson — a refusal names the first
//! reason a body stops, not the only one), not a gate.

use std::path::{Path, PathBuf};

use wolf_sema::{AliasTable, DiskLoader, Resolution, resolve_package_with, typecheck_package_with};

/// Mirrors `lower_corpus.rs` (integration tests cannot share a
/// harness module).
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

fn resolve(entry: &Path) -> Resolution {
    let mut sm = wolf_span::SourceMap::new();
    let mut loader =
        DiskLoader::from_entry(entry, &mut sm, Box::new(|src: &[u8]| is_member_file(src)))
            .expect("entry loads");
    let res = resolve_package_with(&mut loader, &AliasTable::default(), true).expect("resolves");
    assert!(
        !res.diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error),
        "{}: resolve errors",
        entry.display()
    );
    res
}

/// Both lowerings of one entry, with the two properties asserted.
fn both(entry: &Path) -> (wolf_wir::Build, Vec<wolf_wir::SurveyReason>) {
    let res = resolve(entry);
    let tc = typecheck_package_with(&res.package, true);
    let fail_fast = wolf_wir::lower_package(&res.package, &tc);
    let (build, reasons) = wolf_wir::lower_package_survey(&res.package, &tc);
    // The lens changes nothing.
    let key = |b: &wolf_wir::Build| {
        b.not_yet
            .iter()
            .map(|n| (n.construct.to_string(), n.span.lo, n.span.hi))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        key(&fail_fast),
        key(&build),
        "{}: survey changed the ledger",
        entry.display()
    );
    let names = |m: &wolf_wir::Module| {
        let mut v: Vec<String> = m.funcs.iter().map(|(_, f)| f.name.clone()).collect();
        v.sort();
        v
    };
    assert_eq!(
        names(&fail_fast.module),
        names(&build.module),
        "{}: survey changed the module",
        entry.display()
    );
    // The lens is a superset.
    for n in &fail_fast.not_yet {
        assert!(
            reasons
                .iter()
                .any(|r| !r.follow_on && r.construct == n.construct && r.span == n.span),
            "{}: ledger reason `{}` @{}..{} missing from the survey",
            entry.display(),
            n.construct,
            n.span.lo,
            n.span.hi
        );
    }
    (build, reasons)
}

/// The fixture: one function, two INDEPENDENT refusal-provoking
/// statements (a bare fn name passed as a value — the hof_tail shape),
/// a clean `print` between them proving the lens lowered on.
/// Fail-fast reports one reason for `main`; the survey reports both,
/// in source order, the second marked follow-on.
#[test]
fn the_survey_collects_what_fail_fast_masks() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/survey_two_reasons.lu");
    let (build, reasons) = both(&p);
    let in_main: Vec<_> = reasons.iter().filter(|r| r.fn_name == "main").collect();
    assert!(
        in_main.len() >= 2,
        "expected the masked second reason, got {in_main:#?}"
    );
    // The first is the ledger's reason for `main`, exactly.
    let ledger_main = build
        .not_yet
        .iter()
        .find(|n| n.construct.contains("module-item reads"))
        .expect("main's ledger reason");
    assert_eq!(in_main[0].construct, ledger_main.construct);
    assert_eq!(in_main[0].span, ledger_main.span);
    assert!(!in_main[0].follow_on);
    // The second is the SAME construct at a LATER span — the masked
    // independent statement — and is marked a lead, not a verdict.
    assert_eq!(in_main[1].construct, in_main[0].construct);
    assert!(in_main[1].span.lo > in_main[0].span.hi);
    assert!(in_main[1].follow_on);
}

fn corpus_root() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    p.canonicalize().unwrap_or(p)
}

/// The properties over real refusing corpus entries — the files the
/// c21 contracts were written against.
#[test]
fn survey_is_a_superset_and_changes_nothing_on_refusing_corpus_files() {
    for name in [
        "rows/hof_tail.lu",
        "traits/dyn_ok.lu",
        "memory/prov_holy_grail.lu",
        "rows/inferred_private.lu",
        "comptime/norm_linear.lu",
    ] {
        let (build, _) = both(&corpus_root().join(name));
        assert!(
            !build.not_yet.is_empty(),
            "{name}: expected a refusing entry (update this list when it stops refusing)"
        );
    }
}
