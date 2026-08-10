//! The memory checker (s18–s23) — the language's soul.
//!
//! Contract: Tier-0 MVS exclusivity (loan sets over a checker-internal,
//! WIR-shaped check CFG — never exported; WIR builds from typed HIR),
//! region inference and the region checker (D10), Perceus-style `shared`
//! insertion and generational `handle` pools (X5), the unsafe tier with
//! Tree-Borrows-shaped provenance (D11), and the miri-lite UB checker
//! (s23). Checker facts flow to WIR as verified fact annotations.
//!
//! # Implemented today (s18 — Tier-0 exclusivity)
//!
//! - **The check CFG** ([`cfg`]): per-function basic blocks of effect
//!   statements over field-granular *places* ([`place`],
//!   `[mem.model.path.disjoint]`), explicit edges for control flow,
//!   `?` error edges (D30), and X3 trap edges. Checker-internal by
//!   contract; its textual dump is snapshot-pinned.
//! - **Moves** ([`moves`]): forward maybe-uninitialized dataflow —
//!   use-after-move / use-of-uninit (E1001) with move-site
//!   provenance, partial moves per field, re-initialization revives
//!   (whole or per-field residue).
//! - **Call-site mode agreement** (E1007, in [`lower`]): `f(mut x)` /
//!   `f(take x)` must match the declaration (X1) — the c03 handoff,
//!   decided to fire here with exclusivity, one diagnostic in one
//!   place. `mut` needs a place, not a temporary (E1009).
//! - **Exclusivity** ([`excl`], E1002): within one call, a `mut`
//!   place has no other live access path (`[mem.tier0.excl]`);
//!   disjoint fields go `mut` together; `Copy` reads complete at
//!   argument evaluation, which is what keeps the two-phase
//!   `xs.push(xs.len)` shape legal.
//! - **View sets** (E1008): `fn norm(mut self.{x, y})` narrows the
//!   receiver's exclusive footprint at call sites and pins the callee
//!   to the declared fields (`[mem.tier0.excl.3]`).
//! - **Local loan dataflow** ([`loans`]): NLL-grade per-function loan
//!   sets — gen at borrows, killed by base overwrite and by borrower
//!   last-use (backward liveness), two-phase reservation. The
//!   surface (`&x`) refuses in sema until the region campaign, so the
//!   engine is exercised by its unit tests (RFC 2094's problem cases,
//!   the Polonius case accepted).
//! - **Fact export stubs** ([`facts`]): `mut` → noalias +
//!   dereferenceable, `read` → frozen-for-call, whole-value move
//!   sites — the s26 schema, snapshot-tested now.
//! - **Region inference** ([`regions`], s19): every heap allocation
//!   (struct literal, constructor, non-`Copy` call result) is
//!   attributed to the ambient region (`[mem.region.create.3]`, D12)
//!   with Cyclone-rule signature defaults — fresh generalized
//!   parameter regions, results in `ρ_caller`, zero annotations in
//!   the common shape. Conflicting placement demands are E1004;
//!   region-local data outliving its region's wholesale free is
//!   E1010. Escape analysis records stack-promotion facts (regions
//!   whose create/free pair is frame-local; individual allocations
//!   that never escape) for c05/s26 — facts only, no codegen.
//! - **The region checker** ([`lower`]-integrated, s20): `freeze`
//!   consumes the affine value and promotes the owned subtree to
//!   `imm` — writes through any path into frozen data are E1012,
//!   frozen sites are exempt from co-location (`[mem.region.edge.imm]`)
//!   and outlive every frame; transfer/freeze of an open region is
//!   E1005 (`[mem.region.freeze.3]` — the open window pins the
//!   handle, `mut`-lending included); the multiopen open set is
//!   checked as an **antichain in the region forest**
//!   (`[mem.region.multiopen]`, E1011 — iso parent edges recorded at
//!   embedding stores, `in h.child { }` resolves one-step field
//!   paths); a call returning `region` mints a fresh identity (the
//!   scheme-carrying interface's shape — wolf_sema renders and hashes
//!   the derived Cyclone scheme into every heap-reaching fn sig).
//! - **Honest refusals**: `shared`/`handle` (s21), the unsafe tier
//!   (s22), closures/concurrency (c05), region identity through
//!   conflicting rebinds or multi-step paths (s21) return [`NotYet`]
//!   — the conform-run `mem` rung completes only when every body was
//!   actually checked.

use wolf_ast::{GreenNode, SyntaxKind, is_expr_kind};
use wolf_diag::Diagnostic;
use wolf_sema::sig::{ItemSig, SigTables};
use wolf_sema::types::{TyId, TyKind, TypeTable};
use wolf_sema::{BodyRef, BodyResult, NotYet, Package, Typecheck, TypedBody};
use wolf_span::Span;

pub mod cfg;
mod excl;
pub mod facts;
pub mod loans;
mod lower;
mod moves;
pub mod place;
pub mod regions;

pub use facts::FnFacts;
pub use lower::Lowered;
pub use regions::RegionSummary;

/// The package-level result of the s18–s19 memory checker.
#[derive(Debug, Default)]
pub struct MemCheck {
    /// E1xxx diagnostics, deterministically sorted. Meaningful for
    /// the rung verdict only when `not_yet` is empty (the same
    /// conservatism contract as the typecheck rung: never report a
    /// half-checked file).
    pub diagnostics: Vec<Diagnostic>,
    /// Every honest refusal, in body order.
    pub not_yet: Vec<NotYet>,
    /// Per-function fact summaries (s26's input schema).
    pub facts: Vec<FnFacts>,
    /// Per-function region inference records (s19): the reviewable
    /// dump surface behind `wolf conform-run --dump=regions`.
    pub regions: Vec<RegionSummary>,
}

/// Check every fully-typed body of the package. `tc` must come from
/// [`wolf_sema::typecheck_package`] over the same `pkg`.
pub fn check_package(pkg: &Package, tc: &Typecheck) -> MemCheck {
    let mut out = MemCheck::default();
    // Tier guard: types from later sprints refuse the whole rung
    // honestly (a `shared` field is exactly the E1006 surface s21
    // owns — passing the file now would advance the ledger on a
    // check nobody ran).
    tier_guard(&tc.sigs, &mut out.not_yet);
    for outcome in &tc.bodies {
        let BodyResult::Checked(tb) = &outcome.result else {
            continue;
        };
        body_tier_guard(tb, &mut out.not_yet);
        match lower_body(pkg, &tc.sigs, tb, &outcome.body) {
            None => {}
            Some(Err(nyc)) => out.not_yet.push(nyc),
            Some(Ok(lowered)) => {
                out.diagnostics.extend(lowered.diags);
                moves::check(&lowered.cfg, &mut out.diagnostics);
                excl::check(&lowered.cfg, &mut out.diagnostics);
                loans::check(&lowered.cfg, &mut out.diagnostics);
                out.facts
                    .push(facts::collect(&lowered.cfg, &lowered.regions));
                out.regions.push(lowered.regions);
            }
        }
    }
    wolf_diag::sort_diagnostics(&mut out.diagnostics);
    out
}

/// Run every path-sensitive pass over one check CFG (the moves, call
/// exclusivity, and loan analyses) and return its diagnostics. The
/// fuzz/property surface: deterministic for a given CFG, never
/// panicking.
pub fn check_cfg(cfg: &cfg::Cfg) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    moves::check(cfg, &mut diags);
    excl::check(cfg, &mut diags);
    loans::check(cfg, &mut diags);
    diags
}

/// The check-CFG dumps of every lowerable body, in body order (the
/// s01 IR-dump snapshot family).
pub fn dump_package(pkg: &Package, tc: &Typecheck) -> String {
    let mut out = String::new();
    for outcome in &tc.bodies {
        let BodyResult::Checked(tb) = &outcome.result else {
            continue;
        };
        if let Some(Ok(lowered)) = lower_body(pkg, &tc.sigs, tb, &outcome.body) {
            out.push_str(&lowered.cfg.dump());
            out.push('\n');
        }
    }
    out
}

/// The region-inference dumps of every lowerable body, in body order
/// (the `--dump=regions` debug surface, snapshot-pinned).
pub fn dump_regions_package(pkg: &Package, tc: &Typecheck) -> String {
    let mut out = String::new();
    for outcome in &tc.bodies {
        let BodyResult::Checked(tb) = &outcome.result else {
            continue;
        };
        if let Some(Ok(lowered)) = lower_body(pkg, &tc.sigs, tb, &outcome.body) {
            out.push_str(&lowered.regions.render());
            out.push('\n');
        }
    }
    out
}

/// Lower one checked body. `None`: the item has no lowerable body
/// (extern fn, broken tree).
fn lower_body(
    pkg: &Package,
    sigs: &SigTables,
    tb: &TypedBody,
    body: &BodyRef,
) -> Option<Result<Lowered, NotYet>> {
    let root = &pkg.files[body.file].parse.root;
    let node = root.nodes().filter(|n| n.kind.is_item()).nth(body.decl)?;
    let (node, outer) = match body.member {
        None => (node, None),
        Some(mi) => {
            let inner = node.nodes().filter(|n| n.kind.is_item()).nth(mi)?;
            (inner, Some(node))
        }
    };
    let lowerer = lower::Lowerer::new(pkg, sigs, tb, body.module, body.file);
    match node.kind {
        SyntaxKind::FnDecl => {
            let d = wolf_ast::FnDecl::cast(node)?;
            let block = d.body()?;
            let params = fn_params(pkg, sigs, body, outer)?;
            Some(lowerer.lower_fn(&body.name, params, block))
        }
        SyntaxKind::ConstDecl | SyntaxKind::LetDecl | SyntaxKind::VarDecl => {
            let init = node.nodes().find(|n| is_expr_kind(n.kind))?;
            Some(lowerer.lower_init(&body.name, init))
        }
        _ => None,
    }
}

/// The declared parameter list (modes and view sets) of a fn body:
/// module item, impl method, or trait default body.
fn fn_params<'a>(
    pkg: &'a Package,
    sigs: &'a SigTables,
    body: &BodyRef,
    outer: Option<&GreenNode>,
) -> Option<&'a [wolf_sema::sig::ParamSig]> {
    match outer {
        None => match sigs.get(body.module, &body.name)? {
            ItemSig::Fn(f) => Some(&f.params),
            _ => None,
        },
        Some(o) if o.kind == SyntaxKind::ImplDecl => {
            let imp = sigs
                .impls
                .iter()
                .find(|i| i.file == body.file && i.decl == body.decl)?;
            imp.methods
                .iter()
                .find(|m| m.name == body.name)
                .map(|m| m.sig.params.as_slice())
        }
        Some(o) if o.kind == SyntaxKind::TraitDecl => {
            let name_span = wolf_ast::TraitDecl::cast(o)?.name()?.span;
            let tname = text(pkg, body.file, name_span);
            let tr = wolf_sema::traits::TraitRef {
                module: body.module,
                name: tname,
            };
            sigs.traits
                .get(&tr)?
                .methods
                .iter()
                .find(|m| m.name == body.name)
                .map(|m| m.sig.params.as_slice())
        }
        _ => None,
    }
}

fn text(pkg: &Package, file: usize, span: Span) -> String {
    let src = &pkg.files[file].raw.src;
    String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned()
}

// ------------------------------------------------------ tier guards ----

/// Which later sprint owns a type, if any.
fn later_tier(table: &TypeTable, id: TyId, depth: u32) -> Option<&'static str> {
    if depth > 32 {
        return None;
    }
    match table.kind(id) {
        TyKind::Shared(_) | TyKind::Weak(_) => Some("`shared` reference counting (s21)"),
        TyKind::Handle(_) => Some("`handle` pools (s21)"),
        // `region`-typed values check here since s19.
        TyKind::Ptr(_) => Some("the unsafe tier (s22)"),
        TyKind::Wrapping(t) | TyKind::Distinct(t) | TyKind::Range(t) => {
            later_tier(table, *t, depth + 1)
        }
        TyKind::Tuple(elems) => elems.iter().find_map(|&t| later_tier(table, t, depth + 1)),
        TyKind::Fn(params, ret) => params
            .iter()
            .chain(std::iter::once(ret))
            .find_map(|&t| later_tier(table, t, depth + 1)),
        TyKind::ErrUnion(ok, row) => {
            later_tier(table, *ok, depth + 1).or_else(|| later_tier(table, *row, depth + 1))
        }
        TyKind::Row { tags, .. } => tags
            .iter()
            .flat_map(|(_, payload)| payload.iter())
            .find_map(|&t| later_tier(table, t, depth + 1)),
        _ => None,
    }
}

/// Signature-level guard: any item whose declared types belong to a
/// later sprint refuses the rung ('shared_cycle.lu' stays honest
/// until E1006 exists).
fn tier_guard(sigs: &SigTables, not_yet: &mut Vec<NotYet>) {
    for module in &sigs.modules {
        for sig in module.values() {
            match sig {
                ItemSig::Struct(ss) => {
                    for f in &ss.fields {
                        if let Some(construct) = later_tier(&sigs.table, f.ty, 0) {
                            not_yet.push(NotYet {
                                construct,
                                span: f.span,
                            });
                        }
                    }
                }
                ItemSig::Enum { variants, .. } => {
                    for v in variants {
                        for &t in &v.payload {
                            if let Some(construct) = later_tier(&sigs.table, t, 0) {
                                not_yet.push(NotYet {
                                    construct,
                                    span: v.span,
                                });
                            }
                        }
                    }
                }
                ItemSig::Fn(f) => {
                    for p in &f.params {
                        if let Some(construct) = later_tier(&sigs.table, p.ty, 0) {
                            not_yet.push(NotYet {
                                construct,
                                span: p.span,
                            });
                        }
                    }
                    if let Some(construct) = later_tier(&sigs.table, f.ret, 0) {
                        not_yet.push(NotYet {
                            construct,
                            span: f.name_span,
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

/// Body-level guard: any local whose (zonked) type belongs to a later
/// sprint.
fn body_tier_guard(tb: &TypedBody, not_yet: &mut Vec<NotYet>) {
    for (_, span, ty) in &tb.locals {
        if let Some(construct) = later_tier(&tb.table, *ty, 0) {
            not_yet.push(NotYet {
                construct,
                span: *span,
            });
        }
    }
}
