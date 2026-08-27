//! Pass-boundary compaction — the batch half of deferred invariant
//! maintenance (s42 target 1, the egg discipline).
//!
//! Passes mutate in place and may leave garbage behind: instructions
//! dropped from blocks, blocks made unreachable by branch folding,
//! values with no remaining definition. The arenas are append-only
//! (LIFO rollback is construction-time machinery, not pass machinery),
//! so the repair is a REBUILD: reachable blocks in RPO, instructions
//! in layout order, values renumbered densely, facts remapped in
//! order. The result is exactly what the verifier and the canonical
//! printer want — `check_layout`'s block/layout bijection holds again,
//! and unreachable code is gone rather than lingering unverifiable.
//!
//! Facts whose operands did not survive (their defining code was
//! removed) are DROPPED here and reported to the manager, which treats
//! them as value-deleted invalidations — the one legitimate loss the
//! D2 contract names. Everything else is copied bit-for-bit modulo
//! the value renumbering.

use std::collections::HashMap;

use crate::facts::{DerefSize, FactData, FactKind, Just};
use crate::ir::{Aux, Function, Value};

use super::analysis;

/// The value renumbering and fact disposition of one compaction.
pub(crate) struct CompactOut {
    pub func: Function,
    pub vmap: HashMap<Value, Value>,
}

/// Rebuild `f` reachable-only, canonical-order, densely numbered.
pub(crate) fn compact(f: &Function) -> CompactOut {
    let cfg = analysis::cfg(f);
    let mut nf = Function::new(f.name.clone(), f.sig);
    nf.export = f.export;
    nf.src_file = f.src_file;
    // c28: the constant-time contract survives every pass ([ct.attr.carry]).
    nf.consttime = f.consttime.clone();
    // Callee table: copied wholesale so `ExtFunc` ids stay stable and
    // `Aux::Callee` payloads copy verbatim.
    for ef in f.ext_funcs.values() {
        nf.ext_funcs.push(ef.clone());
    }
    let mut vmap: HashMap<Value, Value> = HashMap::new();
    // Blocks first (branches may point forward or backward).
    let mut bmap = HashMap::new();
    for &b in &cfg.rpo {
        let params = f.vpool.get(f.blocks[b].params);
        let tys: Vec<_> = params.iter().map(|&v| f.value_ty(v)).collect();
        let nb = nf.make_block(&tys);
        for (&old, &new) in params.iter().zip(&nf.block_params(nb)) {
            vmap.insert(old, new);
        }
        bmap.insert(b, nb);
    }
    // Instructions, in order.
    for &b in &cfg.rpo {
        let nb = bmap[&b];
        for &inst in &f.blocks[b].insts {
            let data = f.insts[inst];
            let args: Vec<Value> = f
                .vpool
                .get(data.args)
                .into_iter()
                .map(|v| *vmap.get(&v).expect("operand defined in reachable code"))
                .collect();
            let result_tys: Vec<_> = f
                .vpool
                .get(data.results)
                .into_iter()
                .map(|v| f.value_ty(v))
                .collect();
            let aux = match data.aux {
                Aux::Jump(bc) => {
                    let eargs: Vec<Value> = f
                        .vpool
                        .get(bc.args)
                        .into_iter()
                        .map(|v| *vmap.get(&v).expect("edge arg defined"))
                        .collect();
                    Aux::Jump(nf.block_call(bmap[&bc.block], &eargs))
                }
                Aux::Br(t, e) => {
                    let mut edges = [t, e].into_iter().map(|bc| {
                        let eargs: Vec<Value> = f
                            .vpool
                            .get(bc.args)
                            .into_iter()
                            .map(|v| *vmap.get(&v).expect("edge arg defined"))
                            .collect();
                        nf.block_call(bmap[&bc.block], &eargs)
                    });
                    let nt = edges.next().expect("then edge");
                    let ne = edges.next().expect("else edge");
                    Aux::Br(nt, ne)
                }
                other => other,
            };
            nf.span_cursor = f.srcspan(inst);
            let (_, results) = nf.append_inst(nb, data.op, &args, &result_tys, aux);
            for (old, new) in f.vpool.get(data.results).into_iter().zip(results) {
                vmap.insert(old, new);
            }
        }
    }
    nf.span_cursor = None;
    // Facts, remapped in order; a fact any of whose operands died is
    // dropped — the manager audits that as a value-deleted loss.
    for fd in f.facts.values() {
        if let Some(nfd) = remap_fact(fd, &vmap) {
            nf.add_fact(nfd);
        }
    }
    // Debug aux: named bindings whose values survived.
    for dv in &f.debug_vars {
        if let Some(&nv) = vmap.get(&dv.val) {
            nf.add_debug_var(dv.name.clone(), nv, dv.param);
        }
    }
    CompactOut { func: nf, vmap }
}

/// Remap one fact through the value map; `None` if any operand died.
pub(crate) fn remap_fact(fd: &FactData, vmap: &HashMap<Value, Value>) -> Option<FactData> {
    let mv = |v: Value| vmap.get(&v).copied();
    let kind = match fd.kind {
        FactKind::Noalias(a, b) => FactKind::Noalias(mv(a)?, mv(b)?),
        FactKind::Deref(v, DerefSize::Const(n)) => FactKind::Deref(mv(v)?, DerefSize::Const(n)),
        FactKind::Deref(v, DerefSize::Scaled { elem, count }) => FactKind::Deref(
            mv(v)?,
            DerefSize::Scaled {
                elem,
                count: mv(count)?,
            },
        ),
        FactKind::Range(v, lo, hi) => FactKind::Range(mv(v)?, lo, hi),
        FactKind::Region(v, r) => FactKind::Region(mv(v)?, r),
        FactKind::Frozen(v) => FactKind::Frozen(mv(v)?),
    };
    let just = match fd.just {
        Just::Op(v) => Just::Op(mv(v)?),
        // A guard justification names a value the same way `: op` does
        // — an unmapped guard would point a caller-side fact at a
        // stale value, so it remaps or the fact drops.
        Just::Guard(v) => Just::Guard(mv(v)?),
        other => other,
    };
    Some(FactData {
        kind,
        just,
        span: fd.span,
    })
}
