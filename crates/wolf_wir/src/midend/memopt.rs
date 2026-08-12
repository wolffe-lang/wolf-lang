//! Region-aware memory optimization (s42 target 3 — the moat pass's
//! analysis half). The AA oracle's strongest answers are STRUCTURAL
//! here: effect tokens are per-region operands, so a load keyed by its
//! (token-version, address) pair is redundant exactly when the same
//! pair is available from a dominator — and a call that does not
//! consume a region's token PROVABLY cannot touch that region, so
//! availability rides across opaque calls for free. That is the
//! "holy grail" hoist (`read`-param and frozen loads CSE across
//! calls) falling out of the token discipline rather than a points-to
//! analysis; most may-alias queries never reach an oracle because the
//! chains never meet (reports/01 fact 2, mechanized).
//!
//! Clients shipped here:
//! - redundant-load elimination and store-to-load forwarding,
//!   dominator-scoped, keyed by (token version, address, type);
//! - dead-store elimination into DYING regions: a function-local
//!   region (rooted at `region.new`/`stack.alloc`, never an entry
//!   parameter — that memory is caller-visible) whose token chain
//!   feeds no load and no call is write-only; every store into it is
//!   unobservable and dies, `region.free` acting as bulk-dead-store.
//!
//! Store motion via token reordering is deliberately NOT here (v0
//! delta, recorded): reordering against calls would need schedule-
//! point reasoning (spec/07) — conservatism first.

use std::collections::HashMap;

use crate::ir::{Aux, Block, FuncId, Function, Inst, Module, Value};
use crate::ops::Opcode;
use crate::types::{TypeData, TypeId};
use crate::verify::{Invalidation, PassCtx, VerifyError};

use super::analysis;
use super::{ModView, OptStats, run_managed};

pub(crate) fn run(
    m: &mut Module,
    fid: FuncId,
    verify_each: bool,
    stats: &mut OptStats,
) -> Result<bool, VerifyError> {
    run_managed(m, fid, "memopt", verify_each, |f, view, ctx| {
        let mut changed = false;
        changed |= rle_and_forward(f, stats);
        changed |= dse_dying_regions(f, view, ctx, stats);
        changed
    })
}

/// Where an availability entry came from (metrics only).
#[derive(Clone, Copy)]
enum Src {
    Load,
    Store,
}

/// Redundant-load elimination + store-to-load forwarding, dominator-
/// scoped: an entry only fires when its defining block dominates the
/// use site (RPO guarantees dominators are visited first).
fn rle_and_forward(f: &mut Function, stats: &mut OptStats) -> bool {
    let cfg = analysis::cfg(f);
    let doms = analysis::dominators(&cfg);
    let mut avail: HashMap<(Value, Value, TypeId), (Value, Block, Src)> = HashMap::new();
    let mut repl: HashMap<Value, Value> = HashMap::new();
    let mut changed = false;
    for &b in &cfg.rpo {
        let insts = f.blocks[b].insts.clone();
        let mut kept: Vec<Inst> = Vec::with_capacity(insts.len());
        for inst in insts {
            // Resolve operands eagerly so keys are canonical.
            let args_list = f.insts[inst].args;
            for (i, v) in f.vpool.get(args_list).into_iter().enumerate() {
                let r = analysis::resolve(&repl, v);
                if r != v {
                    f.vpool.set(args_list, i, r);
                }
            }
            match f.insts[inst].aux {
                Aux::Jump(bc) => resolve_edge(f, bc, &repl),
                Aux::Br(t, e) => {
                    resolve_edge(f, t, &repl);
                    resolve_edge(f, e, &repl);
                }
                _ => {}
            }
            let op = f.insts[inst].op;
            match op {
                Opcode::Load => {
                    let args = f.vpool.get(f.insts[inst].args);
                    let (p, tok) = (args[0], args[1]);
                    let res = f.vpool.get(f.insts[inst].results)[0];
                    let ty = f.value_ty(res);
                    if let Some(&(val, owner, src)) = avail.get(&(tok, p, ty))
                        && doms.dominates(owner, b)
                    {
                        repl.insert(res, val);
                        match src {
                            Src::Load => stats.loads_eliminated += 1,
                            Src::Store => stats.stores_forwarded += 1,
                        }
                        changed = true;
                        continue; // load dropped
                    }
                    avail.insert((tok, p, ty), (res, b, Src::Load));
                }
                Opcode::Store => {
                    let args = f.vpool.get(f.insts[inst].args);
                    let (v, p, _tok) = (args[0], args[1], args[2]);
                    let ty = f.value_ty(v);
                    let m2 = f.vpool.get(f.insts[inst].results)[0];
                    // The stored value is available at the successor
                    // version; the consumed version's entries go stale
                    // by keying (no future op can name it).
                    avail.insert((m2, p, ty), (v, b, Src::Store));
                }
                _ => {}
            }
            kept.push(inst);
        }
        f.blocks[b].insts = kept;
    }
    if !repl.is_empty() {
        analysis::replace_uses(f, &repl);
    }
    changed
}

fn resolve_edge(f: &mut Function, bc: crate::ir::BlockCall, repl: &HashMap<Value, Value>) {
    for (i, v) in f.vpool.get(bc.args).into_iter().enumerate() {
        let r = analysis::resolve(repl, v);
        if r != v {
            f.vpool.set(bc.args, i, r);
        }
    }
}

/// Dead-store elimination into dying regions. Per region: find the
/// chain root; if it is a local allocation and NO load reads the
/// region's tokens and NO call consumes one, every store on the chain
/// is unobservable — delete them all, splicing each consumed token to
/// its successor's uses.
fn dse_dying_regions(
    f: &mut Function,
    view: &ModView,
    ctx: &mut PassCtx,
    stats: &mut OptStats,
) -> bool {
    // Region id -> (has local root, observed by load/call, params seen).
    #[derive(Default)]
    struct RegionUse {
        local_root: bool,
        entry_root: bool,
        observed: bool,
        frozen: bool,
        stores: Vec<Inst>,
    }
    let mut regions: HashMap<u32, RegionUse> = HashMap::new();
    let region_of = |view: &ModView, ty: TypeId| -> Option<u32> {
        match view.types.get(ty) {
            TypeData::Mem(r) => Some(crate::entity::EntityRef::as_u32(*r)),
            _ => None,
        }
    };
    if let Some(entry) = f.entry() {
        for &p in &f.block_params(entry) {
            if let Some(r) = region_of(view, f.value_ty(p)) {
                regions.entry(r).or_default().entry_root = true;
            }
        }
    }
    let blocks: Vec<Block> = f.layout.clone();
    for &b in &blocks {
        for &inst in &f.blocks[b].insts {
            let op = f.insts[inst].op;
            let args = f.vpool.get(f.insts[inst].args);
            let results = f.vpool.get(f.insts[inst].results);
            match op {
                Opcode::RegionNew | Opcode::StackAlloc => {
                    if let Some(&tok) = results.get(1)
                        && let Some(r) = region_of(view, f.value_ty(tok))
                    {
                        regions.entry(r).or_default().local_root = true;
                    }
                }
                Opcode::Load => {
                    if let Some(r) = region_of(view, f.value_ty(args[1])) {
                        regions.entry(r).or_default().observed = true;
                    }
                }
                Opcode::Call => {
                    for &a in &args {
                        if let Some(r) = region_of(view, f.value_ty(a)) {
                            regions.entry(r).or_default().observed = true;
                        }
                    }
                }
                Opcode::SyncFreeze | Opcode::RcDup | Opcode::RcDrop => {
                    // Frozen/shared chains: leave untouched (freezing
                    // implies later reads; rc implies shared owners).
                    for &a in &args {
                        if let Some(r) = region_of(view, f.value_ty(a)) {
                            regions.entry(r).or_default().frozen = true;
                        }
                    }
                }
                Opcode::Store => {
                    if let Some(r) = region_of(view, f.value_ty(args[2])) {
                        regions.entry(r).or_default().stores.push(inst);
                    }
                }
                _ => {}
            }
        }
    }
    let mut dying: Vec<u32> = regions
        .iter()
        .filter(|(_, u)| {
            u.local_root && !u.entry_root && !u.observed && !u.frozen && !u.stores.is_empty()
        })
        .map(|(&r, _)| r)
        // Tokenless read seams (`__wolf_rt_print_str`-class calls take
        // raw pointers): a region whose pointer ESCAPED is readable
        // without its token — not dying, stores stay.
        .filter(|&r| !analysis::region_pointers(f, view.types, r).escapes)
        .collect();
    dying.sort();
    if dying.is_empty() {
        return false;
    }
    let mut dead: Vec<Inst> = Vec::new();
    let mut repl: HashMap<Value, Value> = HashMap::new();
    for r in dying {
        for &st in &regions[&r].stores {
            let args = f.vpool.get(f.insts[st].args);
            let consumed = args[2];
            let succ = f.vpool.get(f.insts[st].results)[0];
            repl.insert(succ, consumed);
            for id in super::facts_on(f, succ) {
                ctx.invalidate(id, Invalidation::ValueDeleted);
            }
            dead.push(st);
            stats.dead_stores += 1;
        }
    }
    for &b in &blocks {
        f.blocks[b].insts.retain(|i| !dead.contains(i));
    }
    analysis::replace_uses(f, &repl);
    true
}
