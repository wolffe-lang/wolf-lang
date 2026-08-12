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

use std::collections::HashSet;

use crate::ir::{Aux, Block, FuncId, Inst, Module, Value, ValueDef};
use crate::ops::Opcode;
use crate::verify::VerifyError;

use super::analysis;
use super::{OptStats, run_managed};

pub(crate) fn run(
    m: &mut Module,
    fid: FuncId,
    verify_each: bool,
    stats: &mut OptStats,
) -> Result<bool, VerifyError> {
    run_managed(m, fid, "licm", verify_each, |f, _view, _ctx| {
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
