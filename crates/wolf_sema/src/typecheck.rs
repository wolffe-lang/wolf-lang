//! Package-level type checking (s13): elaborate signatures once, then
//! check every body independently and in parallel.
//!
//! Body checking is `(&SigTables, body) → BodyResult` with no shared
//! mutable state (Target 5): rayon fans out over the bodies by
//! default; `WOLF_SEMA_SINGLE_THREAD=1` forces the sequential
//! fallback, and both orders produce identical output (results are
//! collected positionally, diagnostics re-sorted deterministically).
//!
//! The conform-run contract (the corpus ledger's rung):
//! - every body `Checked` and error-free ⇒ the file completes the
//!   `typecheck` rung;
//! - ANY body `NotYetCheckable` ⇒ the rung was *not* completed — the
//!   driver reports `phase_reached=resolve` + `unsupported` and the
//!   partial type diagnostics are withheld (conservatism: never report
//!   half-checked errors as the file's verdict);
//! - type errors with no NotYetCheckable bodies ⇒ `typecheck` +
//!   `fail(E04xx)`.

use rayon::prelude::*;
use wolf_diag::Diagnostic;

use crate::check::{BodyRef, BodyResult, NotYet, check_body};
use crate::graph::Package;
use crate::resolve::Resolution;
use crate::sig::{SigTables, build_sigs};

/// One body's outcome, tagged with its identity.
#[derive(Debug)]
pub struct BodyOutcome {
    pub body: BodyRef,
    pub result: BodyResult,
}

/// The package-level result of the s13 checker.
#[derive(Debug)]
pub struct Typecheck {
    pub sigs: SigTables,
    pub bodies: Vec<BodyOutcome>,
    /// Signature diagnostics + body errors + comptime faults (s16),
    /// deterministically sorted. Meaningful for the rung verdict only
    /// when `not_yet` is empty.
    pub diagnostics: Vec<Diagnostic>,
    /// Every NotYetCheckable refusal, in body order — checker
    /// refusals first, then comptime-evaluator gaps (s16).
    pub not_yet: Vec<NotYet>,
    /// Comptime evaluation counters (s16): memo hit rate rides the
    /// D5 bench stream.
    pub ctfe: crate::ctfe::CtfeStats,
}

impl Typecheck {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error)
    }

    /// Did every body check clean (the typecheck rung completes)?
    pub fn fully_checked(&self) -> bool {
        self.not_yet.is_empty() && !self.has_errors()
    }
}

/// Type-check a resolved package, honoring
/// [`crate::resolve::SINGLE_THREAD_ENV`].
pub fn typecheck_package(res: &Resolution) -> Typecheck {
    let single = std::env::var(crate::resolve::SINGLE_THREAD_ENV).is_ok_and(|v| v == "1");
    typecheck_package_with(&res.package, single)
}

/// [`typecheck_package`] with the threading choice made by the caller.
pub fn typecheck_package_with(pkg: &Package, single_thread: bool) -> Typecheck {
    let sigs = build_sigs(pkg);
    let tasks = collect_bodies(pkg);
    let run = |b: &BodyRef| check_body(pkg, &sigs, b);
    let results: Vec<BodyResult> = if single_thread {
        tasks.iter().map(run).collect()
    } else {
        tasks.par_iter().map(run).collect()
    };
    let mut bodies = Vec::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut not_yet = Vec::new();
    // Signature diagnostics ride along, cascade-suppressed per file.
    let mut sink = wolf_diag::Diagnostics::new();
    for unit in &pkg.files {
        for &region in &unit.parse.error_regions {
            sink.suppress(region);
        }
    }
    for d in &sigs.diagnostics {
        sink.push(d.clone());
    }
    diagnostics.extend(sink.into_vec());
    for (body, result) in tasks.into_iter().zip(results) {
        match &result {
            BodyResult::Errors(ds) => diagnostics.extend(ds.iter().cloned()),
            BodyResult::NotYetCheckable(nyc) => not_yet.push(nyc.clone()),
            // Warnings from clean bodies (E0802 et al.) surface too — a
            // checked body is not a silent one. The s68 typed lint wave
            // (discarded `!T` results, unfitting literal casts, `0.0 -
            // x`) runs over the recorded types here.
            BodyResult::Checked(tb) => {
                diagnostics.extend(tb.warnings.iter().cloned());
                diagnostics.extend(crate::wave::check_typed_body(pkg, &body, tb));
            }
        }
        bodies.push(BodyOutcome { body, result });
    }
    // The comptime pass (s16): every registered comptime call site in
    // a checked body evaluates under its budget; faults are catalog
    // diagnostics, engine gaps are honest NotYet refusals.
    let ctfe_pass = crate::ctfe::run_package(pkg, &sigs, &bodies);
    diagnostics.extend(ctfe_pass.diagnostics);
    not_yet.extend(ctfe_pass.not_yet);
    // The fold reaches the lane (s71): patch each successfully
    // evaluated site's value into its body, where both executing
    // lanes read it as an ordinary constant.
    for (oi, span, fold) in ctfe_pass.folds {
        if let BodyResult::Checked(tb) = &mut bodies[oi].result {
            tb.comptime_folds.push((span, fold));
        }
    }
    wolf_diag::sort_diagnostics(&mut diagnostics);
    // Note-grouping (the teach note renders once per code, not once per
    // spawn) is NOT done here any more: stripping notes from the
    // diagnostic values also stripped them from the JSON stream and the
    // LSP, which want every note on every item. `HumanReporter` groups
    // at render time instead, which also covers the warnings this pass
    // never sees.
    Typecheck {
        sigs,
        bodies,
        diagnostics,
        not_yet,
        ctfe: ctfe_pass.stats,
    }
}

/// Every checkable-shaped body in the package: named function bodies
/// and item initializers, deduplicated by declaration.
fn collect_bodies(pkg: &Package) -> Vec<BodyRef> {
    let mut out = Vec::new();
    let mut seen: Vec<(usize, usize)> = Vec::new();
    for (m, table) in pkg.tables.iter().enumerate() {
        for item in &table.items {
            if seen.contains(&(item.file, item.decl)) {
                continue; // one body per declaration (tuple lets)
            }
            seen.push((item.file, item.decl));
            let node = crate::sig::item_node(pkg, item);
            let has_body = match node.kind {
                wolf_ast::SyntaxKind::FnDecl => wolf_ast::FnDecl::cast(node)
                    .and_then(|d| d.body())
                    .is_some(),
                wolf_ast::SyntaxKind::ConstDecl
                | wolf_ast::SyntaxKind::LetDecl
                | wolf_ast::SyntaxKind::VarDecl => {
                    node.nodes().any(|n| wolf_ast::is_expr_kind(n.kind))
                }
                _ => false,
            };
            if has_body {
                out.push(BodyRef {
                    module: m,
                    file: item.file,
                    name: item.name.clone(),
                    decl: item.decl,
                    member: None,
                });
            }
        }
    }
    // Impl member bodies (s14) and trait default bodies (s17):
    // methods and associated-const initializers check like any other
    // body — `Self` is the impl's self type, or the trait's own
    // archetype for a default body.
    for (m, md) in pkg.modules.iter().enumerate() {
        for &fi in &md.files {
            let root = &pkg.files[fi].parse.root;
            for (decl, node) in root.nodes().filter(|n| n.kind.is_item()).enumerate() {
                if !matches!(
                    node.kind,
                    wolf_ast::SyntaxKind::ImplDecl | wolf_ast::SyntaxKind::TraitDecl
                ) {
                    continue;
                }
                for (member, mnode) in node.nodes().filter(|n| n.kind.is_item()).enumerate() {
                    let (has_body, name) = match mnode.kind {
                        wolf_ast::SyntaxKind::FnDecl => {
                            let d = wolf_ast::FnDecl::cast(mnode);
                            (
                                d.and_then(|d| d.body()).is_some(),
                                d.and_then(|d| d.name())
                                    .map(|t| text(&pkg.files[fi].raw.src, t.span))
                                    .unwrap_or_default(),
                            )
                        }
                        wolf_ast::SyntaxKind::ConstDecl => {
                            let d = wolf_ast::ConstDecl::cast(mnode);
                            (
                                d.and_then(|d| d.init()).is_some(),
                                d.and_then(|d| d.name())
                                    .map(|t| text(&pkg.files[fi].raw.src, t.span))
                                    .unwrap_or_default(),
                            )
                        }
                        _ => (false, String::new()),
                    };
                    if has_body {
                        out.push(BodyRef {
                            module: m,
                            file: fi,
                            name,
                            decl,
                            member: Some(member),
                        });
                    }
                }
            }
        }
    }
    // Deterministic order: file, declaration, member.
    out.sort_by_key(|b| (b.file, b.decl, b.member));
    out
}

fn text(src: &[u8], span: wolf_span::Span) -> String {
    String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned()
}
