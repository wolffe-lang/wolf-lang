//! LICM (s42 target 5): loop-invariant loads and pure trap-free ops
//! hoist to the preheader.
//!
//! A load hoists when its address AND its token are defined outside
//! the loop. An outside-defined token is the whole analysis (the fact
//! made it a lookup): if anything in the loop stored to that region,
//! the loop body would consume the token and mint successors — the
//! outside definition could not be the load's operand. Frozen tokens
//! (`sync.freeze` results, never invalidated) are the special case
//! that needs no case: they satisfy the same test forever.
//!
//! # The one region where that argument fails (s80, wolf-lang#83)
//!
//! "Anything that wrote this region would have consumed the token"
//! assumes the token names every writer. A `region.foreign` root does
//! not: it names storage the RUNTIME owns, and a callee mints its own
//! root over the same bytes rather than receiving one (`@stencil`).
//! So a call inside the loop can write foreign memory while leaving the
//! loop's foreign token defined outside it, and the load would hoist
//! over a write it cannot see.
//!
//! The rule: a load on a FOREIGN token does not hoist out of a loop
//! that contains a call.
//!
//! # The same argument, one door over (s83, wolf-lang#92)
//!
//! "The runtime owns it" is not the only way memory becomes reachable
//! without its token. A LOCAL region whose pointer ESCAPED is reachable
//! the same way — tokenless runtime seams take raw pointers, and a call
//! in the loop body can write through one while the loop's token sits
//! outside, untouched. `memopt`'s `dse` has guarded escape since s42;
//! nothing here did.
//!
//! So the rule generalizes: a load does not hoist out of a loop
//! containing a call unless its region's token is EXHAUSTIVE
//! ([`super::memopt::exhaustive_regions`] — rooted here or lent, no
//! pointer escaped, no handle handed out). Foreign roots are never
//! exhaustive, so the s80 rule is the special case rather than the
//! whole story. Regions that ARE exhaustive are untouched, which is the
//! token discipline's dividend and why the conservatism stays narrow.
//!
//! Speculation is safe by construction: safe-tier WIR loads cannot
//! trap (a live token is structural validity — reports/01), so a load
//! that ran conditionally may run unconditionally in the preheader.
//! Checked arithmetic, by the same coin, does NOT hoist (X3: running
//! a maybe-skipped check unconditionally could ADD a trap — verdicts
//! are sacred); the range pass rewrites proven checks to wrap forms,
//! which hoist here like any pure op.
//!
//! Induction-variable simplification beyond the canonical header-
//! parameter shape is deliberately absent — LLVM's indvars stays
//! rented (non-target); the wrap rewrites are what unlock it.
//!
//! # The license the theorems always offered (s102, #98's a5 half)
//!
//! Both conservatisms above answer "could ANYTHING in this loop write
//! the storage this load reads?" with role- and call-granularity. But
//! the c04 mode theorems are finer: a callee whose WRITE SET is
//! provably confined to the object graphs of its `mut` parameters
//! ([`super::memopt::write_sets`], #91 absorbed exactly this far)
//! cannot write a load whose chain roots at a DIFFERENT entry
//! parameter — `mut` is exclusive, which is the theorem G7 records as
//! "buffer disjointness is a checker theorem nobody spends". So a
//! foreign-token load with a param-rooted chain is LICENSED past a
//! call (or a same-role store) when every foreign write in the loop —
//! direct or through a known callee — lands through a different,
//! `excl.mut`-backed entry parameter. Anything unprovable (an external
//! or indirect callee, rc traffic, an unrooted chain) licenses
//! nothing, and the negative witnesses pin that.

use std::collections::HashSet;

use crate::ir::{Aux, Block, FuncId, Inst, Module, Value, ValueDef};
use crate::ops::{ForeignRole, Opcode};
use crate::verify::VerifyError;

use super::analysis;
use super::{OptStats, run_managed};

pub(crate) fn run(
    m: &mut Module,
    fid: FuncId,
    verify_each: bool,
    stats: &mut OptStats,
) -> Result<bool, VerifyError> {
    let wsets = super::memopt::write_sets(m);
    run_managed(m, fid, "licm", verify_each, |f, view, _ctx| {
        let foreign = super::memopt::foreign_roles(f, view);
        let exhaustive = super::memopt::exhaustive_regions(f, view);
        let cfg = analysis::cfg(f);
        let doms = analysis::dominators(&cfg);
        let loops = analysis::loops(f, &cfg, &doms);
        let mut changed = false;
        // Outermost-first (larger loops first) so hoisted code can
        // cascade outward across the fixpoint iterations of the
        // pipeline (one round per pass run — D4 budgets).
        let mut order: Vec<usize> = (0..loops.loops.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(loops.loops[i].blocks.len()));
        for li in order {
            let l = &loops.loops[li];
            // Preheader: the unique out-of-loop predecessor of the
            // header, jumping straight at it.
            let Some(preds) = cfg.preds.get(&l.header) else {
                continue;
            };
            let outside: Vec<Block> = preds
                .iter()
                .copied()
                .filter(|p| !l.blocks.contains(p))
                .collect();
            let [ph] = outside[..] else { continue };
            let Some(&term) = f.blocks[ph].insts.last() else {
                continue;
            };
            if !matches!(f.insts[term].aux, Aux::Jump(_)) {
                continue; // a br into the header: no clean landing pad
            }
            let defined_in_loop = |f: &crate::ir::Function, v: Value| -> bool {
                match f.values[v].def {
                    ValueDef::Param(b, _) => l.blocks.contains(&b),
                    ValueDef::Result(i, _) => f
                        .layout
                        .iter()
                        .any(|&b| l.blocks.contains(&b) && f.blocks[b].insts.contains(&i)),
                }
            };
            // What in this loop can write foreign storage WITHOUT
            // versioning a foreign token the loop carries (s80):
            // - any call (the callee mints its own root over the same
            //   runtime memory and never names the caller's token);
            // - a store through some OTHER same-role foreign root,
            //   whose chain is a different chain entirely.
            // Either one blocks a foreign-token load from hoisting,
            // however loop-invariant that token looks. Local regions
            // are untouched: there the token really is exhaustive.
            let blocked: HashSet<ForeignRole> =
                super::memopt::blocked_roles(f, view, &foreign, l.blocks.iter());
            // The #92 half (s83): the same argument, for a LOCAL region
            // whose pointer escaped. A tokenless seam takes a raw
            // pointer and can write through it, so a call in the body
            // is a write the loop's outside-defined token never sees —
            // exactly the foreign case, with escape standing in for
            // "the runtime owns it".
            let body_has_call = l
                .blocks
                .iter()
                .any(|&b| f.blocks[b].insts.iter().any(|&i| f.insts[i].op.is_call()));
            // One sweep per loop: collect hoistable instructions in
            // deterministic block order.
            let mut hoist: Vec<(Block, Inst)> = Vec::new();
            let mut hoisted_vals: HashSet<Value> = HashSet::new();
            for &b in &cfg.rpo {
                if !l.blocks.contains(&b) {
                    continue;
                }
                for &inst in &f.blocks[b].insts {
                    let op = f.insts[inst].op;
                    let invariant_ok = match op {
                        Opcode::Load => true,
                        _ => analysis::is_removable(op) && op != Opcode::Load,
                    };
                    if !invariant_ok || op.is_terminator() {
                        continue;
                    }
                    let args = f.vpool.get(f.insts[inst].args);
                    if op == Opcode::Load
                        && let Some(&tok) = args.get(1)
                        && let Some(role) = super::memopt::token_role(f, view, &foreign, tok)
                        && blocked.contains(&role)
                        && !load_licensed(f, view, &foreign, &l.blocks, inst, &wsets)
                    {
                        continue;
                    }
                    if op == Opcode::Load
                        && body_has_call
                        && let Some(&tok) = args.get(1)
                        && let crate::types::TypeData::Mem(r) = view.types.get(f.value_ty(tok))
                        && !exhaustive.contains(&crate::entity::EntityRef::as_u32(*r))
                        && !(super::memopt::token_role(f, view, &foreign, tok).is_some()
                            && load_licensed(f, view, &foreign, &l.blocks, inst, &wsets))
                    {
                        continue;
                    }
                    let all_invariant = args
                        .iter()
                        .all(|&a| !defined_in_loop(f, a) || hoisted_vals.contains(&a));
                    if !all_invariant {
                        continue;
                    }
                    hoist.push((b, inst));
                    for r in f.vpool.get(f.insts[inst].results) {
                        hoisted_vals.insert(r);
                    }
                }
            }
            if hoist.is_empty() {
                continue;
            }
            // Move: insert before the preheader's terminator, in the
            // order collected (defs before uses by construction).
            let tpos = f.blocks[ph].insts.len() - 1;
            for (k, &(b, inst)) in hoist.iter().enumerate() {
                f.blocks[b].insts.retain(|&i| i != inst);
                f.blocks[ph].insts.insert(tpos + k, inst);
                match f.insts[inst].op {
                    Opcode::Load => stats.loads_hoisted += 1,
                    _ => stats.invariants_hoisted += 1,
                }
            }
            changed = true;
        }
        changed
    })
}

/// Is this foreign-token load licensed to hoist past everything in
/// `blocks` (s102)? True iff the load's address chain roots at an
/// entry parameter and EVERY foreign write in the blocks — a direct
/// store or a call with a known write set — lands through a different
/// entry parameter carrying the `excl.mut` theorem. See the module
/// doc; the default is No.
fn load_licensed(
    f: &crate::ir::Function,
    view: &super::ModView,
    foreign: &std::collections::HashMap<u32, crate::ops::ForeignRole>,
    blocks: &HashSet<Block>,
    load: Inst,
    wsets: &std::collections::HashMap<String, super::memopt::WriteSet>,
) -> bool {
    let addr = f.vpool.get(f.insts[load].args)[0];
    let Some(root) = super::memopt::chain_root(f, view, foreign, addr) else {
        return false;
    };
    // One writer check, shared by the store and call arms: the write
    // chain must root at a DIFFERENT entry param that carries the
    // exclusivity theorem.
    let disjoint_writer = |wv: Value| -> bool {
        match super::memopt::chain_root(f, view, foreign, wv) {
            Some(wroot) => wroot != root && super::memopt::param_excl_mut(f, wroot),
            None => false,
        }
    };
    for &b in blocks {
        for &i in &f.blocks[b].insts {
            let args = f.vpool.get(f.insts[i].args);
            match f.insts[i].op {
                Opcode::Store => {
                    let Some(&tok) = args.get(2) else {
                        return false;
                    };
                    if super::memopt::token_role(f, view, foreign, tok).is_none() {
                        continue; // a local-region store: different memory
                    }
                    if !disjoint_writer(args[1]) {
                        return false;
                    }
                }
                Opcode::Call => {
                    let Aux::Callee(ef) = f.insts[i].aux else {
                        return false;
                    };
                    match wsets.get(f.ext_funcs[ef].name.as_str()) {
                        Some(super::memopt::WriteSet::Params(ps)) => {
                            for &pi in ps {
                                let Some(&arg) = args.get(pi) else {
                                    return false;
                                };
                                if !disjoint_writer(arg) {
                                    return false;
                                }
                            }
                        }
                        _ => return false, // external, indirect, unknown
                    }
                }
                Opcode::CallInd | Opcode::RcDup | Opcode::RcDrop => return false,
                _ => {}
            }
        }
    }
    true
}
