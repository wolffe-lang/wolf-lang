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
//! # Where that theorem stops: foreign roots (s80, wolf-lang#83)
//!
//! "No token ⇒ no effect" holds for a region whose ROOT this function
//! owns or received: `region.new` and `stack.alloc` mint storage
//! nobody else can name, and a `mem.rN` entry parameter is a region
//! the caller lent — in both cases the only way to reach the memory is
//! through a token, and tokens are linear.
//!
//! It is FALSE for `region.foreign`. A foreign root names storage the
//! RUNTIME owns, and any function can mint its own root over it: a
//! callee writes a caller's container buffers while holding none of the
//! caller's tokens (`@stencil` in the kernel suite does exactly this).
//! The token is not exhaustive across a call boundary, so a call KILLS
//! every availability entry keyed on a foreign region's token. That
//! costs the cross-call CSE for container traffic only; every other
//! region keeps the full theorem.
//!
//! s78 declined the matching LLVM call-site fact for this reason and
//! recorded the hazard; s80 audited it, found no source-level dynamic
//! witness (see the note in `docs/backlog.md` on what blocks one today,
//! and why that is a lowering accident rather than a theorem), and
//! enforces the conservatism here so nothing rests on the accident.
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

use std::collections::{HashMap, HashSet};

use crate::ir::{Aux, Block, FuncId, Function, Inst, Module, Value};
use crate::ops::{ForeignRole, Opcode};
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
        let foreign = foreign_roles(f, view);
        let mut changed = false;
        changed |= rle_and_forward(f, view, &foreign, stats);
        changed |= dse_dying_regions(f, view, ctx, stats);
        changed
    })
}

/// The regions this function roots with `region.foreign`, each with
/// its role (s80). These are the regions whose tokens are NOT
/// exhaustive; every other region's is, so this map is exactly the
/// scope of both conservatism rules:
///
/// 1. a CALL may write foreign storage while consuming none of its
///    tokens (the callee mints its own root);
/// 2. two roots with the SAME role name the same class of storage, so
///    a store through one is a write the other's token does not
///    version — region identity is not the aliasing unit here, the
///    role is (the same rule the LLVM tier's scope classes follow).
pub(crate) fn foreign_roles(f: &Function, view: &ModView) -> HashMap<u32, ForeignRole> {
    let mut out = HashMap::new();
    for &b in &f.layout {
        for &inst in &f.blocks[b].insts {
            if f.insts[inst].op != Opcode::RegionForeign {
                continue;
            }
            if let Some(&tok) = f.vpool.get(f.insts[inst].results).first()
                && let TypeData::Mem(r) = view.types.get(f.value_ty(tok))
                && let Aux::Int(code) = f.insts[inst].aux
                && let Some(role) = ForeignRole::from_code(code)
            {
                out.insert(crate::entity::EntityRef::as_u32(*r), role);
            }
        }
    }
    out
}

/// Which foreign roles a set of blocks can write WITHOUT versioning any
/// foreign token those blocks carry (s80). A call writes every role (it
/// mints its own roots); a store through a foreign token writes that
/// role for every OTHER root of it. Shared with `licm`, which asks the
/// same question about a loop body.
pub(crate) fn blocked_roles<'a>(
    f: &Function,
    view: &ModView,
    foreign: &HashMap<u32, ForeignRole>,
    blocks: impl IntoIterator<Item = &'a crate::ir::Block>,
) -> HashSet<ForeignRole> {
    let mut out = HashSet::new();
    if foreign.is_empty() {
        return out;
    }
    for &b in blocks {
        for &i in &f.blocks[b].insts {
            match f.insts[i].op {
                Opcode::Call => {
                    out.extend(ForeignRole::ALL);
                }
                Opcode::Store => {
                    let args = f.vpool.get(f.insts[i].args);
                    if let Some(&tok) = args.get(2)
                        && let Some(role) = token_role(f, view, foreign, tok)
                    {
                        out.insert(role);
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// `v`'s foreign role, if `v` is a token naming a foreign region.
pub(crate) fn token_role(
    f: &Function,
    view: &ModView,
    foreign: &HashMap<u32, ForeignRole>,
    v: Value,
) -> Option<ForeignRole> {
    match view.types.get(f.value_ty(v)) {
        TypeData::Mem(r) => foreign.get(&crate::entity::EntityRef::as_u32(*r)).copied(),
        _ => None,
    }
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
fn rle_and_forward(
    f: &mut Function,
    view: &ModView,
    foreign: &HashMap<u32, ForeignRole>,
    stats: &mut OptStats,
) -> bool {
    let cfg = analysis::cfg(f);
    let doms = analysis::dominators(&cfg);
    // The kill below is a LINEAR scan in RPO, and a back edge is not
    // linear: a call in a loop body runs before the header's second
    // visit, so an entry minted in the preheader is stale inside the
    // loop even though nothing killed it on the way there (s80 —
    // `licm`'s witness caught this in `memopt` first). Entering a loop
    // header therefore kills whatever that loop's body can write
    // without versioning a token; entries minted INSIDE the body are
    // re-minted every iteration and handled by the linear scan.
    let mut header_kill: HashMap<Block, HashSet<ForeignRole>> = HashMap::new();
    if !foreign.is_empty() {
        let loops = analysis::loops(f, &cfg, &doms);
        for l in &loops.loops {
            let roles = blocked_roles(f, view, foreign, l.blocks.iter());
            header_kill.entry(l.header).or_default().extend(roles);
        }
    }
    let mut avail: HashMap<(Value, Value, TypeId), (Value, Block, Src)> = HashMap::new();
    let mut repl: HashMap<Value, Value> = HashMap::new();
    let mut changed = false;
    for &b in &cfg.rpo {
        if let Some(roles) = header_kill.get(&b)
            && !roles.is_empty()
        {
            avail.retain(|&(tok, _, _), _| match token_role(f, view, foreign, tok) {
                Some(role) => !roles.contains(&role),
                None => true,
            });
        }
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
                    let (v, p, tok) = (args[0], args[1], args[2]);
                    let ty = f.value_ty(v);
                    let m2 = f.vpool.get(f.insts[inst].results)[0];
                    // A store into foreign storage is a write EVERY
                    // same-role foreign root can see (s80), and the
                    // other roots' chains do not version against it —
                    // so their entries die here, by hand. For a local
                    // region the version keying below is the whole
                    // story.
                    if let Some(role) = token_role(f, view, foreign, tok) {
                        avail.retain(|&(t, _, _), _| token_role(f, view, foreign, t) != Some(role));
                    }
                    // The stored value is available at the successor
                    // version; the consumed version's entries go stale
                    // by keying (no future op can name it).
                    avail.insert((m2, p, ty), (v, b, Src::Store));
                }
                // A call is opaque to FOREIGN storage regardless of
                // which tokens it consumes (s80): the callee mints its
                // own root over the same runtime memory. Drop every
                // entry keyed on a foreign token; local regions keep
                // the token theorem and ride across untouched.
                Opcode::Call if !foreign.is_empty() => {
                    avail.retain(|&(tok, _, _), _| token_role(f, view, foreign, tok).is_none());
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
                // A foreign region's storage OUTLIVES the frame (s75:
                // the runtime owns it), so its stores are observable
                // exactly like a caller's — an entry root, never dying.
                Opcode::RegionForeign => {
                    if let Some(&tok) = results.first()
                        && let Some(r) = region_of(view, f.value_ty(tok))
                    {
                        regions.entry(r).or_default().entry_root = true;
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
