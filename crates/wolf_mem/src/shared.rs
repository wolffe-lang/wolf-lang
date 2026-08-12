//! s21 — the type-level acyclicity law of strong `shared` edges
//! (`[mem.shared.rc.2]`, E1006).
//!
//! `shared T` drops its payload when the last strong count drops; a
//! strong cycle would wait on itself forever, and wolf has no cycle
//! collector — by decision (01 Q3), not omission: acyclicity is what
//! lets every RC drop skip cycle detection forever (the licensed
//! recovery in `[mem.ub.defined]`). So the strong type-reachability
//! graph must be a DAG, checked here at type-definition sites: `T`
//! strongly-reaches `U` when a field/variant path of `T` embeds `U`
//! by value (including through the s21 containers `List`/`Pool`,
//! tuples, error unions and rows) or holds `shared U`. `weak`,
//! `handle`, raw pointers, and function types are not strong edges —
//! they are exactly the sanctioned back-edge vocabulary.
//!
//! A cycle is E1006 only when it crosses at least one `shared` edge:
//! a pure by-value embed cycle is an infinite type, sema's territory.
//! Generic instantiations the type system still holds opaque
//! (`TyKind::Unsupported`) refuse honestly when they could hide a
//! `shared` edge — passing would advance the ledger on a check
//! nobody ran.

use std::collections::BTreeSet;

use wolf_diag::{Diagnostic, codes};
use wolf_sema::NotYet;
use wolf_sema::sig::{ItemSig, SigTables};
use wolf_sema::types::{TyId, TyKind};
use wolf_span::Span;

/// A node of the type graph: (module index, item name).
type Node = (usize, String);

/// One strong edge, anchored at the field/variant that spells it.
struct Edge {
    from: Node,
    to: Node,
    /// The path into `to` crosses a `shared` wrapper.
    via_shared: bool,
    span: Span,
}

/// Check every type definition's strong `shared` reachability
/// ([mem.shared.rc.2]); E1006 per cycle-closing `shared` edge.
pub(crate) fn check(sigs: &SigTables, diags: &mut Vec<Diagnostic>, not_yet: &mut Vec<NotYet>) {
    let mut edges: Vec<Edge> = Vec::new();
    for (mi, module) in sigs.modules.iter().enumerate() {
        for (name, sig) in module {
            let from: Node = (mi, name.clone());
            match sig {
                ItemSig::Struct(ss) => {
                    for f in &ss.fields {
                        collect(sigs, f.ty, false, &from, f.span, &mut edges, not_yet, 0);
                    }
                }
                ItemSig::Enum { variants, .. } => {
                    for v in variants {
                        for &t in &v.payload {
                            collect(sigs, t, false, &from, v.span, &mut edges, not_yet, 0);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    // Every `shared` edge that closes a strong cycle is one E1006.
    for e in edges.iter().filter(|e| e.via_shared) {
        if let Some(path) = strong_path(&edges, &e.to, &e.from) {
            let mut cycle: Vec<&str> = vec![e.from.1.as_str()];
            cycle.extend(path.iter().map(|n| n.1.as_str()));
            if cycle.last() != Some(&e.from.1.as_str()) {
                cycle.push(e.from.1.as_str());
            }
            let loop_ = cycle.join(" → ");
            let target = &e.to.1;
            diags.push(
                Diagnostic::error(
                    codes::E1006,
                    e.span,
                    format!("`{}` holds a strong `shared` path back to itself", e.from.1),
                )
                .with_label(format!("this `shared` edge closes the cycle {loop_}"))
                .with_note(format!(
                    "strong `shared` references drop their target when the last \
                     count drops, so a strong cycle would keep itself alive forever \
                     — and wolf has no cycle collector ([mem.shared.rc.2]). Break \
                     the back-edge: make this field `weak {target}` (upgrade to \
                     reach the value without keeping it alive) or `handle {target}` \
                     (a generational index that faults if the target is gone). If \
                     the structure is genuinely cyclic, keep the whole graph in one \
                     region instead — intra-region cycles are safe and freed \
                     wholesale ([mem.region.intra.1]).",
                )),
            );
        }
    }
}

/// Strong edges out of one type expression. `via_shared` is sticky
/// once the walk crosses a `shared` wrapper.
#[allow(clippy::too_many_arguments)]
fn collect(
    sigs: &SigTables,
    ty: TyId,
    via_shared: bool,
    from: &Node,
    span: Span,
    edges: &mut Vec<Edge>,
    not_yet: &mut Vec<NotYet>,
    depth: u32,
) {
    if depth > 64 {
        return;
    }
    match sigs.table.kind(ty) {
        TyKind::Nominal { module, name } => edges.push(Edge {
            from: from.clone(),
            to: (*module as usize, name.clone()),
            via_shared,
            span,
        }),
        TyKind::Shared(t) => collect(sigs, *t, true, from, span, edges, not_yet, depth + 1),
        // By-value embedding: the wrapper owns its element strongly.
        TyKind::List(t)
        | TyKind::Pool(t)
        | TyKind::Wrapping(t)
        | TyKind::Distinct(t)
        | TyKind::Range(t) => collect(sigs, *t, via_shared, from, span, edges, not_yet, depth + 1),
        TyKind::Tuple(ts) => {
            for &t in ts {
                collect(sigs, t, via_shared, from, span, edges, not_yet, depth + 1);
            }
        }
        TyKind::ErrUnion(t, row) => {
            collect(sigs, *t, via_shared, from, span, edges, not_yet, depth + 1);
            collect(
                sigs,
                *row,
                via_shared,
                from,
                span,
                edges,
                not_yet,
                depth + 1,
            );
        }
        TyKind::Row { tags, .. } => {
            for (_, payload) in tags {
                for &t in payload {
                    collect(sigs, t, via_shared, from, span, edges, not_yet, depth + 1);
                }
            }
        }
        // An opaque generic instantiation could hide a `shared` edge:
        // refuse honestly rather than pass unchecked.
        TyKind::Unsupported(s) if s.contains("shared ") => not_yet.push(NotYet {
            construct: "`shared` acyclicity through opaque generic std types (generic data)",
            span,
        }),
        // `weak`/`handle`/`*T`/fn types: the sanctioned non-strong
        // edges. Everything else is a leaf.
        _ => {}
    }
}

/// A strong path `from → … → to` (empty when `from == to`), if one
/// exists. Deterministic DFS in edge order; returns the visited node
/// chain *including* `to`.
fn strong_path(edges: &[Edge], from: &Node, to: &Node) -> Option<Vec<Node>> {
    if from == to {
        return Some(vec![to.clone()]);
    }
    let mut seen: BTreeSet<Node> = BTreeSet::new();
    let mut path: Vec<Node> = Vec::new();
    fn dfs(
        edges: &[Edge],
        cur: &Node,
        to: &Node,
        seen: &mut BTreeSet<Node>,
        path: &mut Vec<Node>,
    ) -> bool {
        if !seen.insert(cur.clone()) {
            return false;
        }
        path.push(cur.clone());
        if cur == to {
            return true;
        }
        for e in edges.iter().filter(|e| &e.from == cur) {
            if dfs(edges, &e.to, to, seen, path) {
                return true;
            }
        }
        path.pop();
        false
    }
    if dfs(edges, from, to, &mut seen, &mut path) {
        Some(path)
    } else {
        None
    }
}
