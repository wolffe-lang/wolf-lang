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
    /// Signature diagnostics + body errors, deterministically sorted.
    /// Meaningful for the rung verdict only when `not_yet` is empty.
    pub diagnostics: Vec<Diagnostic>,
    /// Every NotYetCheckable refusal, in body order.
    pub not_yet: Vec<NotYet>,
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
    // Impl/trait member bodies exist but are s14/s17 constructs: they
    // are NotYetCheckable wholesale, recorded so the ledger stays
    // honest about the rung.
    for nyc in member_bodies_not_yet(pkg) {
        not_yet.push(nyc);
    }
    for (body, result) in tasks.into_iter().zip(results) {
        match &result {
            BodyResult::Errors(ds) => diagnostics.extend(ds.iter().cloned()),
            BodyResult::NotYetCheckable(nyc) => not_yet.push(nyc.clone()),
            BodyResult::Checked(_) => {}
        }
        bodies.push(BodyOutcome { body, result });
    }
    wolf_diag::sort_diagnostics(&mut diagnostics);
    Typecheck {
        sigs,
        bodies,
        diagnostics,
        not_yet,
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
                });
            }
        }
    }
    // Deterministic order: file, then declaration.
    out.sort_by_key(|b| (b.file, b.decl));
    out
}

/// `impl`/`trait` member bodies: method receivers and trait machinery
/// are s14/s17 — each such body is one honest NotYetCheckable.
fn member_bodies_not_yet(pkg: &Package) -> Vec<NotYet> {
    let mut out = Vec::new();
    for unit in &pkg.files {
        for node in unit.parse.root.nodes().filter(|n| {
            matches!(
                n.kind,
                wolf_ast::SyntaxKind::ImplDecl | wolf_ast::SyntaxKind::TraitDecl
            )
        }) {
            for member in node.nodes().filter(|n| n.kind.is_item()) {
                if member.kind == wolf_ast::SyntaxKind::FnDecl
                    && let Some(d) = wolf_ast::FnDecl::cast(member)
                    && d.body().is_some()
                {
                    out.push(NotYet {
                        construct: "impl/trait member bodies (s14/s17)",
                        span: member.span,
                    });
                }
            }
        }
    }
    out
}
