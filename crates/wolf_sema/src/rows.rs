//! Inferred-row sealing (s15, Target 3 — the Zig-trap fix).
//!
//! A module-private function may write `-> !T` and let the compiler
//! infer its error row. Unlike Zig — where an inferred error set makes
//! the function *generic*, breaking recursion, function pointers, and
//! target-independence — wolf resolves the row to a **concrete tag
//! set at signature elaboration**, so an inferred-row function is an
//! ordinary first-class value with a concrete type.
//!
//! The engine is a cycle-aware fixpoint over each module's call graph
//! (legal because row inference never crosses module boundaries, s12:
//! private functions are invisible elsewhere). Each pass runs the body
//! checker in *collection* mode ([`crate::check::collect_body_rows`]):
//! tags raised into the function's own (marked) row and rows
//! propagated into it by `?`/return are absorbed rather than
//! width-checked, diagnostics discarded — the final per-body check
//! re-runs against the sealed row and owns every report. Rows grow
//! monotonically in a finite tag universe (the tags syntactically
//! present in the module's bodies), so divergence is impossible;
//! recursion and mutual recursion converge like anything else.
//!
//! Exported (`pub`/`pub(pkg)`) signatures must state their row: an
//! inferred row there is E0605, with a fix-it stating the sealed row
//! the body implies (or dropping the `!` when the body cannot fail).

use wolf_diag::{Applicability, Diagnostic, Suggestion, codes};
use wolf_span::Span;

use crate::check::{BodyRef, collect_body_rows};
use crate::graph::{Package, Vis};
use crate::sig::{ItemSig, SigTables};
use crate::types::{TyId, TyKind, TypeTable, render};

/// One function whose row is inferred.
struct Inferred {
    module: usize,
    name: String,
    /// The ok type (in `sigs.table`).
    inner: TyId,
    file: usize,
    decl: usize,
    vis: Vis,
    name_span: Span,
    ret_span: Option<Span>,
}

/// Seal every inferred row in `sigs` to its concrete tag set, record
/// the sealed facts for `wolf interface`, and reject inferred rows on
/// exported items (E0605).
pub(crate) fn seal(pkg: &Package, sigs: &mut SigTables) {
    let mut inferred: Vec<Inferred> = Vec::new();
    for &m in &pkg.topo {
        let mut items: Vec<&crate::graph::Item> = pkg.tables[m].items.iter().collect();
        items.sort_by_key(|i| (i.file, i.decl));
        for item in items {
            let Some(ItemSig::Fn(f)) = sigs.get(m, &item.name) else {
                continue;
            };
            if let TyKind::ErrUnion(inner, row) = sigs.table.kind(f.ret)
                && matches!(sigs.table.kind(*row), TyKind::InferredRow { .. })
            {
                inferred.push(Inferred {
                    module: m,
                    name: item.name.clone(),
                    inner: *inner,
                    file: item.file,
                    decl: item.decl,
                    vis: item.vis,
                    name_span: item.name_span,
                    ret_span: f.ret_span,
                });
            }
        }
    }
    if inferred.is_empty() {
        return;
    }

    // The fixpoint: rows grow monotonically until stable.
    let mut rows: Vec<Vec<(String, Vec<TyId>)>> = vec![Vec::new(); inferred.len()];
    let mut iterations = 0usize;
    loop {
        let mut changed = false;
        for target in 0..inferred.len() {
            // Present every inferred fn at its current partial row —
            // except the target, which keeps its marker so the
            // collector absorbs instead of width-checking.
            for (i, inf) in inferred.iter().enumerate() {
                let row_ty = if i == target {
                    sigs.table.intern(TyKind::InferredRow {
                        module: inf.module as u32,
                        name: inf.name.clone(),
                    })
                } else {
                    sigs.table.row(rows[i].clone(), None)
                };
                set_ret_row(sigs, inf, row_ty);
            }
            let inf = &inferred[target];
            let body = BodyRef {
                module: inf.module,
                file: inf.file,
                name: inf.name.clone(),
                decl: inf.decl,
                member: None,
            };
            let (tags, body_table) = collect_body_rows(pkg, sigs, &body);
            for (name, payload) in tags {
                if rows[target].iter().any(|(n, _)| *n == name) {
                    continue; // first payload wins; the final check
                    // reports any conflict (E0606)
                }
                let moved: Vec<TyId> = payload
                    .into_iter()
                    .map(|t| transfer(&body_table, &mut sigs.table, t))
                    .collect();
                rows[target].push((name, moved));
                changed = true;
            }
        }
        iterations += 1;
        if !changed || iterations > 64 {
            debug_assert!(iterations <= 64, "row sealing fixpoint runaway");
            break;
        }
    }

    // Finalize: write the sealed concrete rows, record them for
    // `wolf interface`, and report exported inferred rows (E0605).
    for (i, inf) in inferred.iter().enumerate() {
        let row_ty = sigs.table.row(rows[i].clone(), None);
        set_ret_row(sigs, inf, row_ty);
        let sealed_ret = sigs.table.err_union(inf.inner, row_ty);
        let rendered = format!(
            "fn {} -> {}",
            inf.name,
            render_sealed(&sigs.table, inf.inner, row_ty)
        );
        sigs.sealed.push((inf.module, inf.name.clone(), rendered));
        if inf.vis != Vis::Private {
            report_pub_inferred(sigs, inf, sealed_ret, row_ty);
        }
    }
}

/// Overwrite one inferred fn's return row in the signature tables.
fn set_ret_row(sigs: &mut SigTables, inf: &Inferred, row_ty: TyId) {
    let ret = sigs.table.err_union(inf.inner, row_ty);
    if let Some(ItemSig::Fn(f)) = sigs
        .modules
        .get_mut(inf.module)
        .and_then(|m| m.get_mut(&inf.name))
    {
        f.ret = ret;
    }
}

/// Render `inner ! {row}` with the row always spelled out (the whole
/// point of `wolf interface` showing sealed rows).
fn render_sealed(table: &TypeTable, inner: TyId, row: TyId) -> String {
    let no_vars = |_: u32| -> Result<TyId, &'static str> { Err("_") };
    format!(
        "{} ! {}",
        render(table, inner, &no_vars),
        crate::types::render_row(table, row, &no_vars)
    )
}

/// E0605 — an exported signature must state its row; the fix-it writes
/// the sealed row the body implies (or drops the `!` when the body
/// cannot fail at all).
fn report_pub_inferred(sigs: &mut SigTables, inf: &Inferred, _sealed_ret: TyId, row_ty: TyId) {
    let no_vars = |_: u32| -> Result<TyId, &'static str> { Err("_") };
    let inner_str = render(&sigs.table, inf.inner, &no_vars);
    let empty = matches!(
        sigs.table.kind(row_ty),
        TyKind::Row { tags, .. } if tags.is_empty()
    );
    let row_str = crate::types::render_row(&sigs.table, row_ty, &no_vars);
    let vis_kw = if inf.vis == Vis::Pub {
        "pub"
    } else {
        "pub(pkg)"
    };
    let mut d = Diagnostic::error(
        codes::E0605,
        inf.name_span,
        format!(
            "`{}` is `{vis_kw}`, so it must state its error row",
            inf.name
        ),
    )
    .with_label("exported with an inferred row");
    if let Some(rs) = inf.ret_span {
        d = d.with_secondary(rs, "the row is inferred here");
        let suggestion = if empty {
            Suggestion::new(
                format!("the body cannot fail — drop the `!`: `-> {inner_str}`"),
                vec![(rs, format!("-> {inner_str}"))],
                Applicability::Maybe,
            )
        } else {
            Suggestion::new(
                format!("state the sealed row: `-> {inner_str} ! {row_str}`"),
                vec![(rs, format!("-> {inner_str} ! {row_str}"))],
                Applicability::Maybe,
            )
        };
        d = d.with_suggestion(suggestion);
    }
    d = d.with_note(
        "inferred rows are private-only (D30): an exported signature is a \
         contract other modules rebuild against, so its failure set is stated, \
         never derived from a body that can drift.",
    );
    sigs.diagnostics.push(d);
}

/// Re-intern a type from one table into another, structurally. Leftover
/// inference variables (payloads nothing pinned) transfer as `<error>`
/// — they unify with everything silently, and the final body check
/// still reports the body's own E0405 if one matters.
fn transfer(src: &TypeTable, dst: &mut TypeTable, ty: TyId) -> TyId {
    match src.kind(ty).clone() {
        TyKind::Error | TyKind::Var(_) => dst.error(),
        TyKind::Never => dst.never(),
        TyKind::Unit => dst.unit(),
        TyKind::Prim(p) => dst.prim(p),
        TyKind::Rigid(n) => dst.intern(TyKind::Rigid(n)),
        TyKind::Nominal { module, name } => dst.intern(TyKind::Nominal { module, name }),
        TyKind::Dyn { module, name } => dst.intern(TyKind::Dyn { module, name }),
        TyKind::RegionTy => dst.intern(TyKind::RegionTy),
        TyKind::TypeTy => dst.intern(TyKind::TypeTy),
        TyKind::Meta(m) => dst.intern(TyKind::Meta(m)),
        TyKind::Unsupported(s) => dst.intern(TyKind::Unsupported(s)),
        TyKind::InferredRow { module, name } => dst.intern(TyKind::InferredRow { module, name }),
        TyKind::OpenTail => dst.intern(TyKind::OpenTail),
        TyKind::Wrapping(t) => {
            let s = transfer(src, dst, t);
            dst.intern(TyKind::Wrapping(s))
        }
        TyKind::Range(t) => {
            let s = transfer(src, dst, t);
            dst.intern(TyKind::Range(s))
        }
        TyKind::Ptr(t) => {
            let s = transfer(src, dst, t);
            dst.intern(TyKind::Ptr(s))
        }
        TyKind::Shared(t) => {
            let s = transfer(src, dst, t);
            dst.intern(TyKind::Shared(s))
        }
        TyKind::Handle(t) => {
            let s = transfer(src, dst, t);
            dst.intern(TyKind::Handle(s))
        }
        TyKind::Weak(t) => {
            let s = transfer(src, dst, t);
            dst.intern(TyKind::Weak(s))
        }
        TyKind::Distinct(t) => {
            let s = transfer(src, dst, t);
            dst.intern(TyKind::Distinct(s))
        }
        TyKind::List(t) => {
            let s = transfer(src, dst, t);
            dst.intern(TyKind::List(s))
        }
        TyKind::Pool(t) => {
            let s = transfer(src, dst, t);
            dst.intern(TyKind::Pool(s))
        }
        TyKind::Chan(t) => {
            let s = transfer(src, dst, t);
            dst.intern(TyKind::Chan(s))
        }
        TyKind::Mutex(t) => {
            let s = transfer(src, dst, t);
            dst.intern(TyKind::Mutex(s))
        }
        TyKind::TaskScope => dst.intern(TyKind::TaskScope),
        TyKind::Proj(base, name) => {
            let s = transfer(src, dst, base);
            dst.intern(TyKind::Proj(s, name))
        }
        TyKind::Tuple(ts) => {
            let s: Vec<TyId> = ts.into_iter().map(|t| transfer(src, dst, t)).collect();
            dst.intern(TyKind::Tuple(s))
        }
        TyKind::Fn(ps, r) => {
            let sp: Vec<TyId> = ps.into_iter().map(|t| transfer(src, dst, t)).collect();
            let sr = transfer(src, dst, r);
            dst.intern(TyKind::Fn(sp, sr))
        }
        TyKind::ErrUnion(t, row) => {
            let s = transfer(src, dst, t);
            let r = transfer(src, dst, row);
            dst.intern(TyKind::ErrUnion(s, r))
        }
        TyKind::Row { tags, tail } => {
            let stags: Vec<(String, Vec<TyId>)> = tags
                .into_iter()
                .map(|(n, p)| (n, p.into_iter().map(|t| transfer(src, dst, t)).collect()))
                .collect();
            let stail = tail.map(|t| transfer(src, dst, t));
            dst.row(stags, stail)
        }
    }
}
