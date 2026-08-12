//! Inlining (s42 target 2): bottom-up over the module call graph, all
//! thresholds in the ONE tunable table, callee-simplify-before-inline
//! by pipeline construction (amendment 4's CGSCC order — a function is
//! fully optimized in its own visit before any caller looks at it).
//!
//! Fact preservation under inlining is the correctness crux (the
//! historical LLVM restrict-scope failure this design routes around):
//! - region identity lives in TYPES, so the rewrite is mechanical:
//!   a callee's formal regions (its token parameters) bind to the
//!   caller's actual regions at the call site; callee-internal regions
//!   (its own `region.new`/`stack.alloc`) mint FRESH caller region ids;
//! - noalias and range facts are timeless value claims and copy over
//!   with their justifications;
//! - theorem-justified deref/frozen facts are CALL-SCOPED claims (the
//!   c04 theorem held for the call's duration) and are conservatively
//!   dropped — op-justified ones re-derive structurally and copy.
//!
//! The verifier re-checks every imported fact after the splice.
//!
//! Error-union calls inline like any call — values, no unwinding (D30).
//!
//! v0 restriction (recorded for the closeout; s43's summaries extend
//! this): callees with effect-token parameters inline only when their
//! body is a single block (the final token version per chain is then
//! syntactically evident); multi-block pure/eu callees inline freely
//! with a join continuation. Recursive and SCC-internal calls never
//! inline.
//!
//! s43 adds a [`Scope`]: which callees this run may CONSIDER at all.
//! The whole-program pipeline uses it twice — the module phase sees
//! only same-module callees (thin-LTO's per-module optimization), the
//! cross-cluster phase sees its cluster's members plus the bodies the
//! summary-driven import decision brought in. [`Scope::all`] is the
//! s42 behaviour verbatim (every module function), which is what the
//! plain [`super::optimize_module`] pipeline still passes.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::entity::EntityRef;
use crate::facts::{FactKind, Just};
use crate::ir::{Aux, Block, ExtFunc, FuncId, Function, Inst, Module, Value};
use crate::ops::Opcode;
use crate::types::{RegionId, TypeData, TypeId};
use crate::verify::VerifyError;

use super::analysis;
use super::summary::Homes;
use super::{OptStats, Thresholds, run_managed};

/// Which callees one inline run may consider, and how to account for
/// the ones it accepts.
#[derive(Clone, Copy, Default)]
pub(crate) struct Scope<'a> {
    /// Callee names the run may consider; `None` = every module
    /// function (the s42 whole-module behaviour).
    pub allow: Option<&'a BTreeSet<String>>,
    /// Names imported from ANOTHER cluster: accepting one is a
    /// cross-cluster inline (the s43 evidence counter).
    pub imported: Option<&'a BTreeSet<String>>,
    /// Home modules, for the cross-module counter.
    pub homes: Option<&'a Homes>,
}

impl Scope<'_> {
    /// Every module function is a candidate — s42's posture.
    pub(crate) fn all() -> Scope<'static> {
        Scope::default()
    }

    fn allows(&self, name: &str) -> bool {
        self.allow.is_none_or(|a| a.contains(name))
    }
}

/// Bottom-up (callees-first) traversal order of the module call graph.
/// Deterministic: DFS post-order from each function in id order.
pub(crate) fn bottom_up_order(m: &Module) -> Vec<FuncId> {
    let by_name: HashMap<&str, FuncId> = m
        .funcs
        .iter()
        .map(|(id, f)| (f.name.as_str(), id))
        .collect();
    let mut order = Vec::new();
    let mut seen: HashSet<FuncId> = HashSet::new();
    for root in m.funcs.keys() {
        // Iterative DFS, post-order emission.
        let mut stack: Vec<(FuncId, usize)> = Vec::new();
        if !seen.contains(&root) {
            seen.insert(root);
            stack.push((root, 0));
        }
        while let Some(&(fid, i)) = stack.last() {
            let callees = callee_ids(m, fid, &by_name);
            if i < callees.len() {
                stack.last_mut().expect("nonempty").1 += 1;
                let c = callees[i];
                if seen.insert(c) {
                    stack.push((c, 0));
                }
            } else {
                order.push(fid);
                stack.pop();
            }
        }
    }
    order
}

fn callee_ids(m: &Module, fid: FuncId, by_name: &HashMap<&str, FuncId>) -> Vec<FuncId> {
    let f = &m.funcs[fid];
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for &b in &f.layout {
        for &inst in &f.blocks[b].insts {
            if f.insts[inst].op != Opcode::Call {
                continue;
            }
            let Aux::Callee(ef) = f.insts[inst].aux else {
                continue;
            };
            if let Some(&cid) = by_name.get(f.ext_funcs[ef].name.as_str())
                && cid != fid
                && seen.insert(cid)
            {
                out.push(cid);
            }
        }
    }
    out
}

/// One prepared call-site splice.
struct Plan {
    block: Block,
    call: Inst,
    callee: Function,
    /// Callee TypeId → caller TypeId (token types re-regioned).
    ty_map: HashMap<TypeId, TypeId>,
    /// Callee RegionId → caller RegionId (for region facts).
    region_map: HashMap<RegionId, RegionId>,
}

pub(crate) fn run(
    m: &mut Module,
    fid: FuncId,
    verify_each: bool,
    th: &Thresholds,
    stats: &mut OptStats,
    scope: Scope<'_>,
) -> Result<bool, VerifyError> {
    // ---- phase 1: decide (immutable module) ------------------------------
    let by_name: HashMap<String, FuncId> =
        m.funcs.iter().map(|(id, f)| (f.name.clone(), id)).collect();
    // Module-wide call-site counts (the single-use bonus).
    let mut site_count: HashMap<FuncId, u32> = HashMap::new();
    for (_, f) in m.funcs.iter() {
        for &b in &f.layout {
            for &inst in &f.blocks[b].insts {
                if f.insts[inst].op == Opcode::Call
                    && let Aux::Callee(ef) = f.insts[inst].aux
                    && let Some(&cid) = by_name.get(&f.ext_funcs[ef].name)
                {
                    *site_count.entry(cid).or_insert(0) += 1;
                }
            }
        }
    }
    let caller = &m.funcs[fid];
    let cfg = analysis::cfg(caller);
    let doms = analysis::dominators(&cfg);
    let loops = analysis::loops(caller, &cfg, &doms);
    let mut accepted: Vec<(Block, Inst, FuncId)> = Vec::new();
    for &b in &cfg.rpo {
        for &inst in &caller.blocks[b].insts {
            if caller.insts[inst].op != Opcode::Call {
                continue;
            }
            let Aux::Callee(ef) = caller.insts[inst].aux else {
                continue;
            };
            let name = &caller.ext_funcs[ef].name;
            let Some(&cid) = by_name.get(name) else {
                continue; // external — including every __wolf_rt_* seam
            };
            if cid == fid {
                continue; // no self-inlining
            }
            if !scope.allows(name) {
                continue; // outside this run's cluster/module horizon
            }
            let callee = &m.funcs[cid];
            if decide(m, callee, caller, inst, &loops, b, site_count[&cid], th) {
                accepted.push((b, inst, cid));
            }
        }
    }
    if accepted.is_empty() {
        return Ok(false);
    }
    // Whole-program accounting (s43): a call the module phase could
    // not even see is the win the summaries bought.
    let mut cross_module = 0usize;
    let mut cross_cluster = 0usize;
    {
        let caller_home = scope
            .homes
            .map(|h| h.home_of(&m.funcs[fid].name).to_string());
        for &(_, _, cid) in &accepted {
            let callee_name = &m.funcs[cid].name;
            if let (Some(home), Some(homes)) = (&caller_home, scope.homes)
                && homes.home_of(callee_name) != home.as_str()
            {
                cross_module += 1;
            }
            if scope.imported.is_some_and(|i| i.contains(callee_name)) {
                cross_cluster += 1;
            }
        }
    }
    // ---- phase 1.5: prepare (needs &mut m.types) --------------------------
    // Fresh caller region ids: allocated past everything the caller or
    // any accepted callee copy mentions.
    let mut next_region = {
        let mut mx = 0u32;
        let bump = |f: &Function, types: &crate::types::TypeInterner| {
            let mut m = 0u32;
            for v in f.values.values() {
                if let TypeData::Mem(r) = types.get(v.ty) {
                    m = m.max(r.as_u32() + 1);
                }
            }
            for fd in f.facts.values() {
                if let FactKind::Region(_, r) = fd.kind {
                    m = m.max(r.as_u32() + 1);
                }
            }
            m
        };
        mx = mx.max(bump(&m.funcs[fid], &m.types));
        for &(_, _, cid) in &accepted {
            mx = mx.max(bump(&m.funcs[cid], &m.types));
        }
        mx
    };
    let mut plans: Vec<Plan> = Vec::new();
    for (block, call, cid) in accepted {
        let callee = m.funcs[cid].clone();
        // Formal-region binding: the callee's entry token params bind
        // to the regions of the caller's token arguments, pairwise in
        // signature order.
        let mut region_map: HashMap<RegionId, RegionId> = HashMap::new();
        {
            let caller = &m.funcs[fid];
            let entry = callee.entry().expect("verified callee");
            let cargs = caller.vpool.get(caller.insts[call].args);
            for (&p, &a) in callee.block_params(entry).iter().zip(&cargs) {
                if let (TypeData::Mem(rf), TypeData::Mem(ra)) = (
                    m.types.get(callee.value_ty(p)).clone(),
                    m.types.get(caller.value_ty(a)).clone(),
                ) {
                    region_map.insert(rf, ra);
                }
            }
        }
        // Callee-internal regions: fresh ids, in first-appearance order.
        let mut callee_regions: Vec<RegionId> = Vec::new();
        for v in callee.values.values() {
            if let TypeData::Mem(r) = m.types.get(v.ty)
                && !callee_regions.contains(r)
            {
                callee_regions.push(*r);
            }
        }
        for r in callee_regions {
            region_map.entry(r).or_insert_with(|| {
                let nr = RegionId::new(next_region);
                next_region += 1;
                nr
            });
        }
        // Pre-intern the remapped token types.
        let mut ty_map: HashMap<TypeId, TypeId> = HashMap::new();
        for v in callee.values.values() {
            if let TypeData::Mem(r) = m.types.get(v.ty).clone() {
                let nt = m.types.mem(region_map[&r]);
                ty_map.insert(v.ty, nt);
            }
        }
        plans.push(Plan {
            block,
            call,
            callee,
            ty_map,
            region_map,
        });
    }
    // Apply in reverse site order so positions inside one block stay
    // valid (a later splice never moves an earlier site).
    plans.sort_by_key(|p| (p.block, position_of(&m.funcs[fid], p.block, p.call)));
    plans.reverse();
    let n = plans.len();
    let changed = run_managed(m, fid, "inline", verify_each, move |f, view, _ctx| {
        for plan in &plans {
            splice(f, view, plan);
        }
        true
    })?;
    if changed {
        stats.inlined_calls += n;
        stats.cross_module_inlined += cross_module;
        stats.cross_cluster_inlined += cross_cluster;
    }
    Ok(changed)
}

fn position_of(f: &Function, b: Block, inst: Inst) -> usize {
    f.blocks[b]
        .insts
        .iter()
        .position(|&i| i == inst)
        .expect("call site in its block")
}

/// The size/benefit decision, every knob from the table.
#[allow(clippy::too_many_arguments)]
fn decide(
    m: &Module,
    callee: &Function,
    caller: &Function,
    call: Inst,
    loops: &analysis::Loops,
    site_block: Block,
    module_sites: u32,
    th: &Thresholds,
) -> bool {
    let size: usize = callee
        .layout
        .iter()
        .map(|&b| callee.blocks[b].insts.len())
        .sum();
    let size = size as u32;
    // Token-parameter callees: single-block bodies only (v0).
    let entry = callee.entry().expect("verified callee");
    let has_token_params = callee
        .block_params(entry)
        .iter()
        .any(|&p| m.types.is_token(callee.value_ty(p)));
    if has_token_params && callee.layout.len() > 1 {
        return false;
    }
    // Multi-result rets join at a continuation; token finals need the
    // single-block shape — both hold by the checks above.
    if size <= th.inline_always {
        return true;
    }
    let depth = loops.depth.get(&site_block).copied().unwrap_or(0);
    let const_args = caller
        .vpool
        .get(caller.insts[call].args)
        .into_iter()
        .filter(|&a| analysis::const_int(caller, a).is_some())
        .count() as u32;
    let read_params = m.sigs[callee.sig]
        .params
        .iter()
        .filter(|p| p.mode == crate::ir::Mode::Read)
        .count() as u32;
    let mut budget = th.inline_max
        + depth * th.inline_depth_bonus
        + (const_args + read_params) * th.inline_const_arg_bonus;
    if module_sites == 1 {
        budget = budget.max(th.inline_single_use);
    }
    size <= budget
}

/// Splice one prepared callee copy over its call site.
fn splice(f: &mut Function, view: &super::ModView, plan: &Plan) {
    let callee = &plan.callee;
    let b = plan.block;
    let pos = position_of(f, b, plan.call);
    let call_args = f.vpool.get(f.insts[plan.call].args);
    let call_results = f.vpool.get(f.insts[plan.call].results);
    let call_span = f.srcspan(plan.call);
    f.span_cursor = call_span; // inlined code inherits the call span (s30)

    // Continuation: the tail of the caller block after the call.
    let tail: Vec<Inst> = f.blocks[b].insts[pos + 1..].to_vec();
    f.blocks[b].insts.truncate(pos);

    let remap_ty = |t: TypeId| plan.ty_map.get(&t).copied().unwrap_or(t);

    // Create the callee's blocks in the caller (params included; the
    // entry copy receives the call arguments via a jmp).
    let mut bmap: HashMap<Block, Block> = HashMap::new();
    let mut vmap: HashMap<Value, Value> = HashMap::new();
    for &cb in &callee.layout {
        let ptys: Vec<TypeId> = callee
            .block_params(cb)
            .iter()
            .map(|&p| remap_ty(callee.value_ty(p)))
            .collect();
        let nb = f.make_block(&ptys);
        for (&old, &new) in callee.block_params(cb).iter().zip(&f.block_params(nb)) {
            vmap.insert(old, new);
        }
        bmap.insert(cb, nb);
    }

    // The continuation block. Multi-ret callees pass their results as
    // continuation parameters; single-ret callees map results directly.
    let ret_sites: Vec<Inst> = callee
        .layout
        .iter()
        .flat_map(|&cb| callee.blocks[cb].insts.iter().copied())
        .filter(|&i| callee.insts[i].op == Opcode::Ret)
        .collect();
    let multi_ret = ret_sites.len() > 1;
    let declared: usize = view.sigs[callee.sig].results.len();
    let cont = if multi_ret {
        let ptys: Vec<TypeId> = call_results[..declared]
            .iter()
            .map(|&v| f.value_ty(v))
            .collect();
        f.make_block(&ptys)
    } else {
        f.make_block(&[])
    };
    f.blocks[cont].insts = tail;

    // Callee ext-func imports, deduplicated into the caller.
    let mut ef_cache: HashMap<u32, ExtFunc> = HashMap::new();

    // Copy instructions.
    let mut final_repl: HashMap<Value, Value> = HashMap::new();
    for &cb in &callee.layout {
        let nb = bmap[&cb];
        for &ci in &callee.blocks[cb].insts {
            let data = callee.insts[ci];
            let args: Vec<Value> = callee
                .vpool
                .get(data.args)
                .into_iter()
                .map(|v| vmap[&v])
                .collect();
            if data.op == Opcode::Ret {
                // ret → jmp cont(results...).
                if multi_ret {
                    let edge = f.block_call(cont, &args[..declared.min(args.len())]);
                    f.append_inst(nb, Opcode::Jmp, &[], &[], Aux::Jump(edge));
                } else {
                    // Map the call's declared results to the returned
                    // values; token results to the final chain values.
                    for (&res, &val) in call_results.iter().zip(&args) {
                        final_repl.insert(res, val);
                    }
                    let edge = f.block_call(cont, &[]);
                    f.append_inst(nb, Opcode::Jmp, &[], &[], Aux::Jump(edge));
                }
                continue;
            }
            let result_tys: Vec<TypeId> = callee
                .vpool
                .get(data.results)
                .into_iter()
                .map(|v| remap_ty(callee.value_ty(v)))
                .collect();
            let aux = match data.aux {
                Aux::Jump(bc) => {
                    let eargs: Vec<Value> = callee
                        .vpool
                        .get(bc.args)
                        .into_iter()
                        .map(|v| vmap[&v])
                        .collect();
                    Aux::Jump(f.block_call(bmap[&bc.block], &eargs))
                }
                Aux::Br(t, e) => {
                    let nt = {
                        let eargs: Vec<Value> = callee
                            .vpool
                            .get(t.args)
                            .into_iter()
                            .map(|v| vmap[&v])
                            .collect();
                        f.block_call(bmap[&t.block], &eargs)
                    };
                    let ne = {
                        let eargs: Vec<Value> = callee
                            .vpool
                            .get(e.args)
                            .into_iter()
                            .map(|v| vmap[&v])
                            .collect();
                        f.block_call(bmap[&e.block], &eargs)
                    };
                    Aux::Br(nt, ne)
                }
                Aux::Callee(ef) => {
                    let nef = *ef_cache.entry(ef.as_u32()).or_insert_with(|| {
                        let efd = &callee.ext_funcs[ef];
                        f.import_func(efd.name.clone(), efd.sig)
                    });
                    Aux::Callee(nef)
                }
                other => other,
            };
            let (_, results) = f.append_inst(nb, data.op, &args, &result_tys, aux);
            for (old, new) in callee.vpool.get(data.results).into_iter().zip(results) {
                vmap.insert(old, new);
            }
        }
    }

    // Token results of the call: the final version of each chain in a
    // single-block callee (the only token-carrying shape we accept) is
    // the last def of that token type in the block — or the entry
    // param itself when the chain was never consumed.
    if call_results.len() > declared {
        let entry = callee.entry().expect("verified callee");
        let mut current: HashMap<TypeId, Value> = HashMap::new();
        let mut token_params: Vec<Value> = Vec::new();
        for &p in &callee.block_params(entry) {
            let t = callee.value_ty(p);
            if view.types.is_token(remap_ty(t)) {
                current.insert(t, p);
                token_params.push(p);
            }
        }
        for &ci in &callee.blocks[entry].insts {
            for r in callee.vpool.get(callee.insts[ci].results) {
                let t = callee.value_ty(r);
                if current.contains_key(&t) {
                    current.insert(t, r);
                }
            }
        }
        for (i, &tok_res) in call_results[declared..].iter().enumerate() {
            let p = token_params[i];
            let fin = current[&callee.value_ty(p)];
            final_repl.insert(tok_res, vmap[&fin]);
        }
    }

    // Rewire the caller block into the entry copy.
    let entry_copy = bmap[&callee.entry().expect("verified callee")];
    let edge = f.block_call(entry_copy, &call_args);
    f.append_inst(b, Opcode::Jmp, &[], &[], Aux::Jump(edge));

    // Callee facts, per the custody policy.
    for fd in callee.facts.values() {
        let keep = match (fd.kind, fd.just) {
            (FactKind::Noalias(..), _) => true,
            (FactKind::Range(..), _) => true,
            (FactKind::Region(..), _) => true,
            (FactKind::Deref(..) | FactKind::Frozen(..), Just::DefOp | Just::Op(_)) => true,
            (FactKind::Deref(..) | FactKind::Frozen(..), Just::Theorem(_)) => false,
        };
        if !keep {
            continue;
        }
        let Some(mut nfd) = super::compact::remap_fact(fd, &vmap) else {
            continue;
        };
        if let FactKind::Region(v, r) = nfd.kind {
            nfd.kind = FactKind::Region(v, plan.region_map[&r]);
        }
        f.add_fact(nfd);
    }

    // Replace the call's result values everywhere (the tail moved to
    // `cont`, and dominated uses elsewhere see the same map). For
    // multi-ret callees the declared results come from cont's params.
    if multi_ret {
        for (i, &res) in call_results[..declared].iter().enumerate() {
            final_repl.insert(res, f.block_params(cont)[i]);
        }
        debug_assert_eq!(
            call_results.len(),
            declared,
            "multi-ret token callees are rejected at decide()"
        );
    }
    analysis::replace_uses(f, &final_repl);
    f.span_cursor = None;
    // The call instruction itself was dropped with the block truncate.
}
