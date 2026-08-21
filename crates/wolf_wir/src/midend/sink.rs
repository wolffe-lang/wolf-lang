//! Region-bounded allocation sinking / stack promotion (amendment 1 —
//! the moat pass). A whole small region whose lifetime is dominated
//! and whose contents provably never escape promotes to the frame:
//!
//! ```text
//! %h, %m0 = region.new            %h, %m0 = stack.alloc TOTAL
//! %p, %m1 = region.alloc %h,N,%m0     %p = ptr.off %h, OFF, 1
//! ...                          ⇒  ...
//! region.free %h, %mk             (free deleted — frame lifetime)
//! ```
//!
//! Every runtime call disappears from the region's lifecycle; the
//! backend's entry-block alloca discipline means a promoted region
//! inside a loop reuses ONE frame slab per activation (exactly the b3
//! request-churn shape; b1's unbounded tree region must NOT promote —
//! the size gate holds it back).
//!
//! Applicability (all checked, conservatively):
//! - the handle's only uses are `region.alloc` and exactly one
//!   `region.free`; no freeze, no rc, no call ever sees the handle;
//! - every allocation size is a constant; the 16-aligned total fits
//!   the table's `sink_max_bytes`;
//! - no pointer into the region ESCAPES (calls — including every
//!   spawn seam, which is how "no sinking across spawn edges" holds —
//!   stores-as-value, returns, aggregates, block args);
//! - no call consumes the region's tokens (tokenless read seams are
//!   covered by the escape rule; token-carrying calls by this one);
//! - each `region.alloc` executes at most once per `region.new`
//!   activation (same innermost loop, dominated by the root — the
//!   static-offset licence);
//! - the `region.free` exists (dominated lifetime, no leak-on-branch
//!   shapes in v0) and is unique.
//!
//! Fact custody: `region`/`deref` facts about promoted pointers are
//! REWRITTEN to the stack region and re-justified against the slab
//! allocation (`: op %slab`); the old region's facts are invalidated
//! as region-retired (`RegionFreed` — the region ceased to exist).

use std::collections::{HashMap, HashSet};

use crate::entity::EntityRef;
use crate::facts::{FactData, FactKind, Just};
use crate::ir::{Aux, Block, FuncId, Function, Inst, Module, Value};
use crate::ops::Opcode;
use crate::types::{RegionId, TypeData, TypeId};
use crate::verify::{Invalidation, PassCtx, VerifyError};

use super::analysis;
use super::{OptStats, Thresholds, run_managed};

/// (region.new inst, its block, the free, laid-out allocs, total,
/// region, the s76 ambient enter/leave calls to retire with it).
type Proto = (Inst, Block, Inst, Vec<(Inst, u64)>, u64, u32, Vec<Inst>);

/// One promotable region, fully analyzed.
struct Candidate {
    new_inst: Inst,
    new_block: Block,
    free_inst: Inst,
    /// (alloc inst, byte offset in the slab).
    allocs: Vec<(Inst, u64)>,
    total: u64,
    /// The retired region.
    old_region: u32,
    /// s76: the `__wolf_rt_region_ambient_enter`/`leave` calls that
    /// opened this region for container placement. They retire with the
    /// region — which `analyze_region` only permits after proving the
    /// region's extent CANNOT have allocated through the ambient slot
    /// (see its `no_ambient_allocation` gate). A promoted region's
    /// handle is a frame pointer, not a `Region *`, so leaving an enter
    /// behind would hand `wolf_rt` a stack address to bump-allocate in.
    ambient: Vec<Inst>,
    /// Pre-interned `mem.rFRESH` for the slab's token chain.
    new_mem_ty: TypeId,
    new_region: RegionId,
}

/// The s76 ambient-region seam's symbol names (see
/// `wolf_rt::native`'s ambient-region note).
const AMBIENT_ENTER: &str = "__wolf_rt_region_ambient_enter";
const AMBIENT_LEAVE: &str = "__wolf_rt_region_ambient_leave";

pub(crate) fn run(
    m: &mut Module,
    fid: FuncId,
    verify_each: bool,
    th: &Thresholds,
    stats: &mut OptStats,
) -> Result<bool, VerifyError> {
    // ---- analyze (immutable) ---------------------------------------------
    let mut next_region = {
        let f = &m.funcs[fid];
        let mut mx = 0u32;
        for v in f.values.values() {
            if let TypeData::Mem(r) = m.types.get(v.ty) {
                mx = mx.max(r.as_u32() + 1);
            }
        }
        for fd in f.facts.values() {
            if let FactKind::Region(_, r) = fd.kind {
                mx = mx.max(r.as_u32() + 1);
            }
        }
        mx
    };
    let mut proto: Vec<Proto> = Vec::new();
    {
        let f = &m.funcs[fid];
        let view = super::ModView {
            types: &m.types,
            sigs: &m.sigs,
        };
        let cfg = analysis::cfg(f);
        let doms = analysis::dominators(&cfg);
        let loops = analysis::loops(f, &cfg, &doms);
        let innermost = |b: Block| -> Option<Block> {
            loops
                .loops
                .iter()
                .filter(|l| l.blocks.contains(&b))
                .min_by_key(|l| l.blocks.len())
                .map(|l| l.header)
        };
        for &b in &cfg.rpo {
            for &inst in &f.blocks[b].insts {
                if f.insts[inst].op != Opcode::RegionNew {
                    continue;
                }
                let results = f.vpool.get(f.insts[inst].results);
                let (h, tok) = (results[0], results[1]);
                let TypeData::Mem(r) = view.types.get(f.value_ty(tok)) else {
                    continue;
                };
                let region = r.as_u32();
                if let Some(c) =
                    analyze_region(f, &view, &cfg, &doms, innermost, inst, b, h, region, th)
                {
                    proto.push(c);
                }
            }
        }
    }
    if proto.is_empty() {
        return Ok(false);
    }
    // ---- prepare types (needs &mut m.types) --------------------------------
    let mut cands: Vec<Candidate> = Vec::new();
    for (new_inst, new_block, free_inst, allocs, total, old_region, ambient) in proto {
        let new_region = RegionId::new(next_region);
        next_region += 1;
        let new_mem_ty = m.types.mem(new_region);
        cands.push(Candidate {
            new_inst,
            new_block,
            free_inst,
            allocs,
            total,
            old_region,
            ambient,
            new_mem_ty,
            new_region,
        });
    }
    let n_regions = cands.len();
    let n_allocs: usize = cands.iter().map(|c| c.allocs.len()).sum();
    let changed = run_managed(m, fid, "sink", verify_each, move |f, view, ctx| {
        for c in &cands {
            promote(f, view, ctx, c);
        }
        true
    })?;
    if changed {
        stats.regions_promoted += n_regions;
        stats.allocs_promoted += n_allocs;
    }
    Ok(changed)
}

/// The s76 ambient bracket around `handle`: the
/// `__wolf_rt_region_ambient_enter(%handle)` calls plus every
/// `__wolf_rt_region_ambient_leave(%saved)` that consumes one of their
/// results. Two passes, so RPO order never decides whether a leave is
/// recognized.
fn ambient_bracket(f: &Function, cfg: &analysis::Cfg, handle: Value) -> Vec<Inst> {
    let named = |inst: Inst, want: &str| -> bool {
        matches!(f.insts[inst].aux, Aux::Callee(ef) if f.ext_funcs[ef].name == want)
    };
    let mut out: Vec<Inst> = Vec::new();
    let mut saved: Vec<Value> = Vec::new();
    for &b in &cfg.rpo {
        for &inst in &f.blocks[b].insts {
            if named(inst, AMBIENT_ENTER) && f.vpool.get(f.insts[inst].args).contains(&handle) {
                out.push(inst);
                saved.extend(f.vpool.get(f.insts[inst].results).iter().copied());
            }
        }
    }
    if out.is_empty() {
        return out;
    }
    for &b in &cfg.rpo {
        for &inst in &f.blocks[b].insts {
            if named(inst, AMBIENT_LEAVE)
                && f.vpool
                    .get(f.insts[inst].args)
                    .iter()
                    .any(|a| saved.contains(a))
            {
                out.push(inst);
            }
        }
    }
    out
}

/// Can anything in the region's EXTENT have allocated through the
/// ambient slot? (s76 — only asked when the region carries an ambient
/// bracket.)
///
/// The extent is bounded by the enter and the leave, so every
/// instruction that can run inside it lies in a block reachable from
/// `new_block` that also reaches `free_block` (the leave sits just
/// inside the free, on the X4 cleanup chain). Within that set, an
/// opaque `call` is exactly the thing that might have asked
/// `wolf_rt` for container storage — the shims read the ambient slot,
/// and no summary tells this pass otherwise. So: no calls in the
/// extent besides the bracket itself.
///
/// Deliberately conservative, and deliberately on the OPTIMIZER's side
/// of the line: lowering emits the honest bracket, and a region is
/// promoted or elided only when the bracket is provably unobservable.
/// The cost of being wrong here is a dangling container; the cost of
/// being coarse is a `region.new`/`free` pair the mid-end could have
/// removed. Sharpening this with per-callee "allocates?" summaries is
/// the obvious follow-up.
fn no_ambient_allocation(
    f: &Function,
    cfg: &analysis::Cfg,
    new_block: Block,
    free_block: Block,
    ambient: &[Inst],
) -> bool {
    // Forward reachability from the region's root.
    let mut from_new: HashSet<Block> = HashSet::new();
    let mut work = vec![new_block];
    while let Some(b) = work.pop() {
        if !from_new.insert(b) {
            continue;
        }
        for s in crate::print::successors(f, b) {
            if cfg.reachable.contains(&s) {
                work.push(s);
            }
        }
    }
    // Backward reachability to the free.
    let mut to_free: HashSet<Block> = HashSet::new();
    let mut work = vec![free_block];
    while let Some(b) = work.pop() {
        if !to_free.insert(b) {
            continue;
        }
        for &p in cfg.preds.get(&b).into_iter().flatten() {
            work.push(p);
        }
    }
    for &b in &cfg.rpo {
        if !from_new.contains(&b) || !to_free.contains(&b) {
            continue;
        }
        for &inst in &f.blocks[b].insts {
            if f.insts[inst].op.is_call() && !ambient.contains(&inst) {
                return false;
            }
        }
    }
    true
}

/// Full applicability analysis for one `region.new`.
#[allow(clippy::too_many_arguments)]
fn analyze_region(
    f: &Function,
    view: &super::ModView,
    cfg: &analysis::Cfg,
    doms: &analysis::Doms,
    innermost: impl Fn(Block) -> Option<Block>,
    new_inst: Inst,
    new_block: Block,
    handle: Value,
    region: u32,
    th: &Thresholds,
) -> Option<Proto> {
    // Handle uses: allocs + exactly one free, nothing else. Token
    // consumers on the chain: only those same ops, plus loads/stores.
    //
    // s76 adds one more permitted handle use: the ambient enter/leave
    // pair. It is not an escape (the runtime stores the handle in a
    // thread slot for the extent and drops it at the leave), but it DOES
    // make the region reachable to `wolf_rt`'s container allocator, so
    // `no_ambient_allocation` below has to clear the extent before this
    // region may be promoted or elided.
    let mut allocs: Vec<(Inst, Block)> = Vec::new();
    let mut frees: Vec<Inst> = Vec::new();
    let ambient = ambient_bracket(f, cfg, handle);
    for &b in &cfg.rpo {
        for &inst in &f.blocks[b].insts {
            if ambient.contains(&inst) {
                continue;
            }
            let op = f.insts[inst].op;
            let args = f.vpool.get(f.insts[inst].args);
            let touches_handle = args.contains(&handle);
            let touches_chain = args.iter().any(|&a| {
                matches!(view.types.get(f.value_ty(a)), TypeData::Mem(r) if r.as_u32() == region)
            });
            match op {
                Opcode::RegionAlloc if touches_handle => allocs.push((inst, b)),
                Opcode::RegionFree if touches_handle => frees.push(inst),
                Opcode::Load | Opcode::Store => {
                    // Loads/stores through the chain are the region's
                    // ordinary life; storing the HANDLE anywhere is an
                    // escape.
                    if op == Opcode::Store && args[0] == handle {
                        return None;
                    }
                }
                _ if touches_handle || touches_chain => return None,
                _ => {}
            }
            // Handle in a branch edge: conservative escape.
            match f.insts[inst].aux {
                Aux::Jump(bc) => {
                    if f.vpool.get(bc.args).contains(&handle) {
                        return None;
                    }
                }
                Aux::Br(t, e) => {
                    for bc in [t, e] {
                        if f.vpool.get(bc.args).contains(&handle) {
                            return None;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if frees.len() != 1 {
        return None;
    }
    let free_block = f
        .layout
        .iter()
        .copied()
        .find(|&b| f.blocks[b].insts.contains(&frees[0]))?;
    // s76: an ambient bracket means `wolf_rt` could have bump-allocated
    // container storage in this region behind the pass's back. Retiring
    // the region — as a stack slab or as nothing at all — is only sound
    // once the extent is cleared of every opaque call.
    if !ambient.is_empty() && !no_ambient_allocation(f, cfg, new_block, free_block, &ambient) {
        return None;
    }
    // The EMPTY region (scalarized scratch: `region r { ... }` whose
    // contents promoted to SSA) elides entirely — no slab, no ops.
    // Chain hygiene: nothing but the free may even READ the chain.
    if allocs.is_empty() {
        for &b in &cfg.rpo {
            for &inst in &f.blocks[b].insts {
                if inst == new_inst || inst == frees[0] || ambient.contains(&inst) {
                    continue;
                }
                let touches = f.vpool.get(f.insts[inst].args).iter().any(|&a| {
                    matches!(view.types.get(f.value_ty(a)), TypeData::Mem(r) if r.as_u32() == region)
                });
                if touches {
                    return None;
                }
            }
        }
        return Some((
            new_inst,
            new_block,
            frees[0],
            Vec::new(),
            0,
            region,
            ambient,
        ));
    }
    // Pointer escape (covers tokenless read seams and spawn edges).
    if analysis::region_pointers(f, view.types, region).escapes {
        return None;
    }
    // Constant sizes, 16-aligned chunk layout, budget.
    let mut laid: Vec<(Inst, u64)> = Vec::new();
    let mut total: u64 = 0;
    let home = innermost(new_block);
    for (inst, b) in allocs {
        // Once per activation: the alloc sits in the same innermost
        // loop as the region.new and is dominated by it.
        if innermost(b) != home || !doms.dominates(new_block, b) {
            return None;
        }
        let args = f.vpool.get(f.insts[inst].args);
        let size = analysis::const_int(f, args[1])?;
        if size < 0 {
            return None;
        }
        laid.push((inst, total));
        total += (size as u64).div_ceil(16) * 16;
    }
    if total == 0 || total > th.sink_max_bytes {
        return None;
    }
    // The free must be dominated by the region.new (a lifetime, not a
    // maybe): structural given tokens, but hold it explicitly.
    if !doms.dominates(new_block, free_block) {
        return None;
    }
    Some((new_inst, new_block, frees[0], laid, total, region, ambient))
}

/// Apply one promotion.
fn promote(f: &mut Function, view: &super::ModView, ctx: &mut PassCtx, c: &Candidate) {
    // s76: the ambient bracket retires with the region in BOTH shapes —
    // elision (the region ceases to exist) and promotion (the handle
    // becomes a frame pointer, which `wolf_rt` must never be handed as
    // a region). `analyze_region` proved the extent allocates nothing
    // through the slot, so dropping the bracket is observationally
    // nothing.
    if !c.ambient.is_empty() {
        for &b in &f.layout.clone() {
            f.blocks[b].insts.retain(|i| !c.ambient.contains(i));
        }
    }
    if c.allocs.is_empty() {
        // Empty-region elision: the region never held anything — the
        // `region.new`/`region.free` pair disappears outright.
        for &b in &f.layout.clone() {
            f.blocks[b]
                .insts
                .retain(|&i| i != c.new_inst && i != c.free_inst);
        }
        let _ = ctx;
        return;
    }
    let results = f.vpool.get(f.insts[c.new_inst].results);
    let (slab, tok0) = (results[0], results[1]);
    // region.new -> stack.alloc TOTAL (same result values: %h becomes
    // the slab pointer, %m0 the stack region's root token).
    let pos = f.blocks[c.new_block]
        .insts
        .iter()
        .position(|&i| i == c.new_inst)
        .expect("region.new in its block");
    let (_, sz_vals) = f.append_inst(
        c.new_block,
        Opcode::Iconst,
        &[],
        &[crate::types::I64],
        Aux::Int(c.total as i64),
    );
    let sz = sz_vals[0];
    let szi = f.blocks[c.new_block].insts.pop().expect("just appended");
    f.blocks[c.new_block].insts.insert(pos, szi);
    {
        let args = f.vpool.intern(&[sz]);
        let data = &mut f.insts[c.new_inst];
        data.op = Opcode::StackAlloc;
        data.args = args;
        data.aux = Aux::None;
    }
    // Retype the whole old token chain to the fresh stack region.
    let vals: Vec<Value> = f.values.keys().collect();
    for v in vals {
        if let TypeData::Mem(r) = view.types.get(f.value_ty(v))
            && r.as_u32() == c.old_region
        {
            f.values[v].ty = c.new_mem_ty;
        }
    }
    let _ = tok0;
    // Each alloc -> ptr.off %slab, OFF; token successor spliced out.
    let mut repl: HashMap<Value, Value> = HashMap::new();
    for &(alloc, off) in &c.allocs {
        let ab = f
            .layout
            .iter()
            .copied()
            .find(|&b| f.blocks[b].insts.contains(&alloc))
            .expect("alloc placed");
        let apos = f.blocks[ab]
            .insts
            .iter()
            .position(|&i| i == alloc)
            .expect("pos");
        let aargs = f.vpool.get(f.insts[alloc].args);
        let ares = f.vpool.get(f.insts[alloc].results);
        let (p_old, m_out) = (ares[0], ares[1]);
        let m_in = aargs[2];
        // OFF constant + ptr.off, inserted at the alloc's position.
        let (_, off_vals) = f.append_inst(
            ab,
            Opcode::Iconst,
            &[],
            &[crate::types::I64],
            Aux::Int(off as i64),
        );
        let offv = off_vals[0];
        let offi = f.blocks[ab].insts.pop().expect("appended");
        let (_, p_vals) = f.append_inst(
            ab,
            Opcode::PtrOff,
            &[slab, offv],
            &[crate::types::PTR],
            Aux::Scale(1),
        );
        let p_new = p_vals[0];
        let pi = f.blocks[ab].insts.pop().expect("appended");
        f.blocks[ab].insts.remove(apos);
        f.blocks[ab].insts.insert(apos, pi);
        f.blocks[ab].insts.insert(apos, offi);
        repl.insert(p_old, p_new);
        repl.insert(m_out, m_in);
    }
    // Delete the free (frame lifetime).
    for &b in &f.layout.clone() {
        f.blocks[b].insts.retain(|&i| i != c.free_inst);
    }
    analysis::replace_uses(f, &repl);
    // Fact custody: rewrite region/deref facts onto the slab; retire
    // what cannot be rewritten. The old region id is dead either way.
    let ids: Vec<_> = f.facts.keys().collect();
    for id in ids {
        let fd = f.facts[id];
        match fd.kind {
            FactKind::Region(p, r) if r.as_u32() == c.old_region => {
                let np = analysis::resolve(&repl, p);
                ctx.invalidate(id, Invalidation::RegionFreed);
                f.facts[id] = FactData {
                    kind: FactKind::Region(np, c.new_region),
                    just: Just::Op(slab),
                    span: fd.span,
                };
            }
            FactKind::Deref(p, size) if repl.contains_key(&p) => {
                let np = analysis::resolve(&repl, p);
                ctx.invalidate(id, Invalidation::RegionFreed);
                f.facts[id] = FactData {
                    kind: FactKind::Deref(np, size),
                    just: Just::Op(slab),
                    span: fd.span,
                };
            }
            FactKind::Noalias(a, b) if repl.contains_key(&a) || repl.contains_key(&b) => {
                let (na, nb) = (analysis::resolve(&repl, a), analysis::resolve(&repl, b));
                ctx.invalidate(id, Invalidation::RegionFreed);
                f.facts[id] = FactData {
                    kind: FactKind::Noalias(na, nb),
                    just: fd.just,
                    span: fd.span,
                };
            }
            _ => {}
        }
    }
}
