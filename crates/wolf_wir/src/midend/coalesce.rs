//! Bump-allocation coalescing (amendment 2): consecutive constant-size
//! allocations from ONE region fuse into a single `region.alloc`, the
//! followers becoming `ptr.off`s at 16-aligned offsets — one runtime
//! bump instead of k (frees need no accounting: the region frees
//! wholesale, riding the region effect token as ever).
//!
//! Adjacency is judged on the TOKEN CHAIN inside one block: the next
//! allocation must consume the previous one's token directly, with no
//! CALL anywhere between them in program order. The token gives the
//! semantic barrier (anything consuming the chain breaks the run);
//! the no-call rule is the schedule-point conservatism (spec/07):
//! every runtime call is a potential scheduling decision, and the
//! allocator's observable state must not change across one. A spawn
//! is a call — no coalescing across spawn edges, by construction.
//!
//! The fused chunks keep the runtime's 16-byte alignment contract
//! (report 10 delta 2): offsets are multiples of 16.

use std::collections::HashMap;

use crate::entity::EntityRef;
use crate::facts::{FactData, FactKind, Just};
use crate::ir::{Aux, FuncId, Inst, Module, Value};
use crate::ops::Opcode;
use crate::types::TypeData;
use crate::verify::{Invalidation, VerifyError};

use super::analysis;
use super::{OptStats, Thresholds, run_managed};

pub(crate) fn run(
    m: &mut Module,
    fid: FuncId,
    verify_each: bool,
    th: &Thresholds,
    stats: &mut OptStats,
) -> Result<bool, VerifyError> {
    let max = th.coalesce_max_bytes;
    let mut fused_total = 0usize;
    let changed = run_managed(m, fid, "coalesce", verify_each, |f, view, ctx| {
        let mut changed = false;
        let blocks: Vec<_> = f.layout.clone();
        for b in blocks {
            // Find runs: leader alloc + followers chained directly off
            // its token, same handle, const sizes, no call between.
            let insts = f.blocks[b].insts.clone();
            let mut i = 0usize;
            while i < insts.len() {
                let leader = insts[i];
                if !is_const_alloc(f, leader) {
                    i += 1;
                    continue;
                }
                let largs = f.vpool.get(f.insts[leader].args);
                let (handle, lsize) = (largs[0], analysis::const_int(f, largs[1]).expect("const"));
                let lres = f.vpool.get(f.insts[leader].results);
                let (lp, mut chain_tok) = (lres[0], lres[1]);
                let mut run: Vec<(Inst, i64)> = Vec::new();
                let mut total = (lsize as u64).div_ceil(16) * 16;
                let mut j = i + 1;
                while j < insts.len() {
                    let cand = insts[j];
                    let op = f.insts[cand].op;
                    if op == Opcode::Call {
                        break; // schedule point — hard barrier
                    }
                    let cargs = f.vpool.get(f.insts[cand].args);
                    if is_const_alloc(f, cand) && cargs[0] == handle && cargs[2] == chain_tok {
                        let sz = analysis::const_int(f, cargs[1]).expect("const");
                        let padded = (sz as u64).div_ceil(16) * 16;
                        if total + padded > max {
                            break;
                        }
                        run.push((cand, total as i64));
                        total += padded;
                        chain_tok = f.vpool.get(f.insts[cand].results)[1];
                        j += 1;
                        continue;
                    }
                    // Anything else that consumes the current chain
                    // token ends the run (stores, frees, freezes...).
                    if cargs.contains(&chain_tok) {
                        break;
                    }
                    j += 1;
                }
                if run.is_empty() {
                    i += 1;
                    continue;
                }
                // Fuse: grow the leader, rewrite followers.
                changed = true;
                fused_total += run.len();
                let region = match view.types.get(f.value_ty(chain_tok)) {
                    TypeData::Mem(r) => r.as_u32(),
                    _ => unreachable!("alloc token is a mem token"),
                };
                let _ = region;
                // Leader size := total. Mint the constant right before
                // the leader.
                let lpos = f.blocks[b]
                    .insts
                    .iter()
                    .position(|&x| x == leader)
                    .expect("pos");
                let (_, sz_vals) = f.append_inst(
                    b,
                    Opcode::Iconst,
                    &[],
                    &[crate::types::I64],
                    Aux::Int(total as i64),
                );
                let szv = sz_vals[0];
                let szi = f.blocks[b].insts.pop().expect("appended");
                f.blocks[b].insts.insert(lpos, szi);
                let largs_list = f.insts[leader].args;
                f.vpool.set(largs_list, 1, szv);
                // Followers: ptr.off at their run offsets; tokens
                // splice through.
                let mut repl: HashMap<Value, Value> = HashMap::new();
                for &(fa, off) in &run {
                    let fargs = f.vpool.get(f.insts[fa].args);
                    let fres = f.vpool.get(f.insts[fa].results);
                    let (p_old, m_out) = (fres[0], fres[1]);
                    let m_in = fargs[2];
                    let fpos = f.blocks[b]
                        .insts
                        .iter()
                        .position(|&x| x == fa)
                        .expect("pos");
                    let (_, off_vals) =
                        f.append_inst(b, Opcode::Iconst, &[], &[crate::types::I64], Aux::Int(off));
                    let offv = off_vals[0];
                    let offi = f.blocks[b].insts.pop().expect("appended");
                    let (_, p_vals) = f.append_inst(
                        b,
                        Opcode::PtrOff,
                        &[lp, offv],
                        &[crate::types::PTR],
                        Aux::Scale(1),
                    );
                    let p_new = p_vals[0];
                    let pi = f.blocks[b].insts.pop().expect("appended");
                    f.blocks[b].insts.remove(fpos);
                    f.blocks[b].insts.insert(fpos, pi);
                    f.blocks[b].insts.insert(fpos, offi);
                    repl.insert(p_old, p_new);
                    repl.insert(m_out, m_in);
                }
                analysis::replace_uses(f, &repl);
                // Fact custody: follower facts re-cite the leader.
                let ids: Vec<_> = f.facts.keys().collect();
                for id in ids {
                    let fd = f.facts[id];
                    match fd.kind {
                        FactKind::Region(p, r) if repl.contains_key(&p) => {
                            ctx.invalidate(id, Invalidation::ValueDeleted);
                            f.facts[id] = FactData {
                                kind: FactKind::Region(analysis::resolve(&repl, p), r),
                                just: Just::Op(lp),
                                span: fd.span,
                            };
                        }
                        FactKind::Deref(p, size) if repl.contains_key(&p) => {
                            ctx.invalidate(id, Invalidation::ValueDeleted);
                            f.facts[id] = FactData {
                                kind: FactKind::Deref(analysis::resolve(&repl, p), size),
                                just: Just::Op(lp),
                                span: fd.span,
                            };
                        }
                        FactKind::Noalias(x, y)
                            if repl.contains_key(&x) || repl.contains_key(&y) =>
                        {
                            ctx.invalidate(id, Invalidation::ValueDeleted);
                            f.facts[id] = FactData {
                                kind: FactKind::Noalias(
                                    analysis::resolve(&repl, x),
                                    analysis::resolve(&repl, y),
                                ),
                                just: fd.just,
                                span: fd.span,
                            };
                        }
                        _ => {}
                    }
                }
                // Restart scanning after the run (positions shifted).
                i = j;
            }
        }
        changed
    })?;
    if changed {
        stats.allocs_coalesced += fused_total;
    }
    Ok(changed)
}

fn is_const_alloc(f: &crate::ir::Function, inst: Inst) -> bool {
    if f.insts[inst].op != Opcode::RegionAlloc {
        return false;
    }
    let args = f.vpool.get(f.insts[inst].args);
    analysis::const_int(f, args[1]).is_some_and(|n| n >= 0)
}
