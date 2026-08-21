//! Loop-region CSE (s102, e3's mechanism): two IDENTICAL pure
//! constant-shape loops executed in sequence compute identical exit
//! values, so the second loop's results are the first's — GVN, at the
//! granularity of a loop region.
//!
//! The shape this exists for is e3_index_arith after inlining unrolls
//! a constant-depth recursion: two copies of the same reduction loop,
//! same entry values, same trip governor, same pure body — and clang
//! runs one copy, replacing the other with its exit value (exit-value
//! replacement wins a pass-ordering race there; see the G8 addendum).
//! Final-value replacement — folding BOTH copies to a compile-time
//! constant — was the contract's first-listed option and is
//! deliberately NOT built: with the kernel's constant trip count it
//! would fold the loop the benchmark exists to time, the exact
//! failure class #115 documents (a lane executing no loop measured
//! against one that executes it). Region CSE lands the same 2x with
//! no budget hazard and no kernel-breaking overshoot, and it
//! generalizes: recursion unrolling and macro-shaped duplication
//! produce identical sequential loops in real programs.
//!
//! # Matching, deliberately narrow (v1)
//!
//! Two natural loops match when:
//! - each is {header, body} with a Jump-entry preheader, the body has
//!   no block params, the header's `br` sends an argless edge to the
//!   body and an ARGLESS edge out (loop-closed values flow through
//!   header params only — block-param dominance gives that for free);
//! - the first loop's exit target dominates the second's preheader
//!   (STRICTLY sequential: alternatives from loop versioning sit in
//!   sibling branch arms and never dominate each other, so fast/slow
//!   twins can never CSE);
//! - entry args are pairwise the SAME value (GVN has already merged
//!   equal constants), and every instruction matches in lockstep
//!   under the value map — same opcode, same aux, same types, args
//!   mapped or shared invariants;
//! - every instruction is pure and NON-MEMORY: [`analysis::
//!   is_removable`] minus `load` — a load is removable when dead but
//!   memory may change between the two loops, so a loop that so much
//!   as reads memory never matches.
//!
//! One merge per pass run (D4: a fixed count of rewrites per visit,
//! not a fixpoint); the dominator tree is computed once and a rewrite
//! invalidates it for the blocks it orphans.

use std::collections::HashMap;

use crate::ir::{Aux, Block, BlockCall, FuncId, Inst, Module, Value, ValueDef};
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
    let mut merged = false;
    let changed = run_managed(m, fid, "loopcse", verify_each, |f, _view, _ctx| {
        let cfg = analysis::cfg(f);
        let doms = analysis::dominators(&cfg);
        let loops = analysis::loops(f, &cfg, &doms);
        // Canonical candidates, in deterministic header order.
        let mut cands: Vec<Shape> = Vec::new();
        for l in &loops.loops {
            if let Some(shape) = shape_of(f, &cfg, l) {
                cands.push(shape);
            }
        }
        cands.sort_by_key(|s| {
            cfg.rpo
                .iter()
                .position(|&b| b == s.header)
                .unwrap_or(usize::MAX)
        });
        for i in 0..cands.len() {
            for j in 0..cands.len() {
                if i == j {
                    continue;
                }
                let (a, b) = (&cands[i], &cands[j]);
                // Sequential: A finished before B begins.
                if !doms.dominates(a.exit_target, b.preheader) {
                    continue;
                }
                if let Some(map) = loops_match(f, a, b) {
                    rewrite(f, b, &map);
                    merged = true;
                    return true; // one merge per run (D4)
                }
            }
        }
        false
    })?;
    if merged {
        stats.loops_cse += 1;
    }
    Ok(changed)
}

/// The canonical single-body loop shape, or `None`.
struct Shape {
    header: Block,
    body: Block,
    preheader: Block,
    exit_target: Block,
    /// Entry args the preheader passes to the header params.
    entry_args: Vec<Value>,
    /// Header params, in order.
    params: Vec<Value>,
}

fn shape_of(
    f: &crate::ir::Function,
    cfg: &analysis::Cfg,
    l: &analysis::NaturalLoop,
) -> Option<Shape> {
    if l.blocks.len() != 2 {
        return None;
    }
    let header = l.header;
    let &body = l.blocks.iter().find(|&&b| b != header)?;
    if !f.block_params(body).is_empty() {
        return None;
    }
    // Preheader: unique out-of-loop predecessor, jumping straight in.
    let preds = cfg.preds.get(&header)?;
    let outside: Vec<Block> = preds
        .iter()
        .copied()
        .filter(|p| !l.blocks.contains(p))
        .collect();
    let [preheader] = outside[..] else {
        return None;
    };
    let &pterm = f.blocks[preheader].insts.last()?;
    let Aux::Jump(entry) = f.insts[pterm].aux else {
        return None;
    };
    if entry.block != header {
        return None;
    }
    // Header terminator: br body-edge (argless), exit-edge (argless).
    let &hterm = f.blocks[header].insts.last()?;
    let Aux::Br(t, e) = f.insts[hterm].aux else {
        return None;
    };
    let (body_edge, exit_edge) = if t.block == body {
        (t, e)
    } else if e.block == body {
        (e, t)
    } else {
        return None;
    };
    if !f.vpool.get(body_edge.args).is_empty() || !f.vpool.get(exit_edge.args).is_empty() {
        return None;
    }
    if l.blocks.contains(&exit_edge.block) {
        return None;
    }
    // Body terminator: jmp back to the header.
    let &bterm = f.blocks[body].insts.last()?;
    let Aux::Jump(back) = f.insts[bterm].aux else {
        return None;
    };
    if back.block != header {
        return None;
    }
    Some(Shape {
        header,
        body,
        preheader,
        exit_target: exit_edge.block,
        entry_args: f.vpool.get(entry.args).to_vec(),
        params: f.block_params(header),
    })
}

/// Lockstep match of B against A; the returned map sends B's values
/// (params and results) to A's.
fn loops_match(f: &crate::ir::Function, a: &Shape, b: &Shape) -> Option<HashMap<Value, Value>> {
    if a.params.len() != b.params.len() || a.entry_args != b.entry_args {
        return None;
    }
    let mut map: HashMap<Value, Value> = HashMap::new();
    for (&pb, &pa) in b.params.iter().zip(&a.params) {
        if f.value_ty(pb) != f.value_ty(pa) {
            return None;
        }
        map.insert(pb, pa);
    }
    let in_loop = |shape: &Shape, v: Value| -> bool {
        match f.values[v].def {
            ValueDef::Param(blk, _) => blk == shape.header || blk == shape.body,
            ValueDef::Result(inst, _) => [shape.header, shape.body]
                .iter()
                .any(|&blk| f.blocks[blk].insts.contains(&inst)),
        }
    };
    for (&ba, &bb) in [(&a.header, &b.header), (&a.body, &b.body)]
        .iter()
        .map(|&(x, y)| (x, y))
    {
        let (ia, ib) = (&f.blocks[ba].insts, &f.blocks[bb].insts);
        if ia.len() != ib.len() {
            return None;
        }
        for (&insta, &instb) in ia.iter().zip(ib) {
            if !match_inst(f, a, b, insta, instb, &in_loop, &mut map) {
                return None;
            }
        }
    }
    Some(map)
}

#[allow(clippy::too_many_arguments)]
fn match_inst(
    f: &crate::ir::Function,
    a: &Shape,
    b: &Shape,
    insta: Inst,
    instb: Inst,
    in_loop: &dyn Fn(&Shape, Value) -> bool,
    map: &mut HashMap<Value, Value>,
) -> bool {
    let (da, db) = (&f.insts[insta], &f.insts[instb]);
    if da.op != db.op {
        return false;
    }
    let terminator = da.op.is_terminator();
    // Purity: everything except the terminators must be removable and
    // NOT a load (memory can change between the loops).
    if !terminator && (!analysis::is_removable(da.op) || da.op == Opcode::Load) {
        return false;
    }
    // Aux: terminators carry edges (matched structurally by shape_of
    // for the header br / body jmp targets; the body jmp's ARGS are
    // matched below through the args list). Everything else must be
    // payload-identical.
    if !terminator && da.aux != db.aux {
        return false;
    }
    let (argsa, argsb) = (f.vpool.get(da.args), f.vpool.get(db.args));
    let (extra_a, extra_b) = if terminator {
        // The body jmp's next-iteration values ride the edge, not the
        // args list.
        let ea: Vec<Value> = match da.aux {
            Aux::Jump(e) => f.vpool.get(e.args).to_vec(),
            _ => Vec::new(),
        };
        let eb: Vec<Value> = match db.aux {
            Aux::Jump(e) => f.vpool.get(e.args).to_vec(),
            _ => Vec::new(),
        };
        (ea, eb)
    } else {
        (Vec::new(), Vec::new())
    };
    if argsa.len() != argsb.len() || extra_a.len() != extra_b.len() {
        return false;
    }
    for (va, vb) in argsa
        .iter()
        .copied()
        .zip(argsb.iter().copied())
        .chain(extra_a.iter().copied().zip(extra_b.iter().copied()))
    {
        let mapped_ok = match map.get(&vb) {
            Some(&m) => m == va,
            None => {
                // A shared invariant — the same value — or two EQUAL
                // CONSTANTS (lowering mints per-loop iconsts GVN may
                // not have merged across blocks); both must be defined
                // outside their loops.
                let invariant = !in_loop(a, va) && !in_loop(b, vb);
                invariant
                    && (va == vb
                        || (equal_consts(f, va, vb) && {
                            map.insert(vb, va);
                            true
                        }))
            }
        };
        if !mapped_ok {
            return false;
        }
    }
    // Results extend the map.
    let (ra, rb) = (f.vpool.get(da.results), f.vpool.get(db.results));
    if ra.len() != rb.len() {
        return false;
    }
    for (va, vb) in ra.iter().copied().zip(rb.iter().copied()) {
        if f.value_ty(va) != f.value_ty(vb) {
            return false;
        }
        map.insert(vb, va);
    }
    true
}

/// Bypass loop B: its preheader jumps straight to its exit target, and
/// every use of its values outside the loop reads A's instead.
fn rewrite(f: &mut crate::ir::Function, b: &Shape, map: &HashMap<Value, Value>) {
    // 1. Reroute the preheader.
    let &pterm = f.blocks[b.preheader].insts.last().expect("shape checked");
    let empty = f.vpool.intern(&[]);
    f.insts[pterm].aux = Aux::Jump(BlockCall {
        block: b.exit_target,
        args: empty,
    });
    // 2. Replace uses outside the loop.
    let loop_blocks = [b.header, b.body];
    let all_blocks: Vec<Block> = f.layout.clone();
    for blk in all_blocks {
        if loop_blocks.contains(&blk) {
            continue;
        }
        let insts: Vec<Inst> = f.blocks[blk].insts.clone();
        for inst in insts {
            let args = f.vpool.get(f.insts[inst].args).to_vec();
            if args.iter().any(|v| map.contains_key(v)) {
                let new: Vec<Value> = args.iter().map(|v| *map.get(v).unwrap_or(v)).collect();
                f.insts[inst].args = f.vpool.intern(&new);
            }
            let remap_edge = |f: &mut crate::ir::Function, e: BlockCall| -> BlockCall {
                let eargs = f.vpool.get(e.args).to_vec();
                if eargs.iter().any(|v| map.contains_key(v)) {
                    let new: Vec<Value> = eargs.iter().map(|v| *map.get(v).unwrap_or(v)).collect();
                    BlockCall {
                        block: e.block,
                        args: f.vpool.intern(&new),
                    }
                } else {
                    e
                }
            };
            match f.insts[inst].aux {
                Aux::Jump(e) => {
                    let ne = remap_edge(f, e);
                    f.insts[inst].aux = Aux::Jump(ne);
                }
                Aux::Br(t, e) => {
                    let (nt, ne) = (remap_edge(f, t), remap_edge(f, e));
                    f.insts[inst].aux = Aux::Br(nt, ne);
                }
                _ => {}
            }
        }
    }
    // Loop B is now unreachable; compaction at the pass boundary
    // removes it and audits its facts as value-deleted.
}

/// Two values that are both integer/float/bool constants with the
/// same payload and type.
fn equal_consts(f: &crate::ir::Function, a: Value, b: Value) -> bool {
    let const_of = |v: Value| -> Option<(Opcode, Aux, crate::types::TypeId)> {
        match f.values[v].def {
            ValueDef::Result(inst, _) => {
                let d = &f.insts[inst];
                matches!(d.op, Opcode::Iconst | Opcode::Fconst | Opcode::Bconst)
                    .then(|| (d.op, d.aux, f.value_ty(v)))
            }
            _ => None,
        }
    };
    match (const_of(a), const_of(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}
