//! Simplify (s42 target 4, amendment 4): constant folding, the ONE
//! declarative identity table, dominator-scoped GVN, branch folding,
//! trivial block-parameter removal, and trap-free DCE — applied
//! EAGERLY (the aegraph evidence: eager application loses ~0.1%
//! against full e-graph extraction, X11 owns revisiting that), to a
//! FIXED iteration budget (D4: reproducible builds).
//!
//! The rule table is [`crate::peephole_rules::RULES`] — the same rows
//! the construction-time peephole applies. This pass exists because
//! inlining and constant propagation expose redexes construction
//! never saw; the discipline is that a rewrite is a ROW, and both
//! consumers interpret the same table.
//!
//! X3 throughout: checked ops fold to their trap kind on provable
//! overflow (the trap IS the outcome); identity rows are trap-free by
//! table construction; GVN dedup of checked ops preserves the
//! surviving check; DCE never removes a live check (`is_removable`
//! excludes the checked family — the range pass must prove a check
//! away before it can die).

use std::collections::{HashMap, HashSet};

use crate::ir::{Aux, Block, FuncId, Function, Inst, Module, Value};
use crate::ops::{IntCc, Opcode, TrapKind};
use crate::peephole_rules::{self, Rewrite};
use crate::types::{TypeId, TypeInterner};
use crate::verify::{Invalidation, PassCtx, VerifyError};

use super::analysis::{self, is_removable};
use super::{OptStats, Thresholds, run_managed};

pub(crate) fn run(
    m: &mut Module,
    fid: FuncId,
    verify_each: bool,
    th: &Thresholds,
    stats: &mut OptStats,
) -> Result<bool, VerifyError> {
    let iters = th.simplify_iters;
    run_managed(m, fid, "simplify", verify_each, |f, view, ctx| {
        let mut changed = false;
        for _ in 0..iters {
            let mut round = false;
            round |= gvn_fold_round(f, view.types, ctx, stats);
            round |= trivial_params(f, ctx, stats);
            round |= dce(f, ctx, stats);
            changed |= round;
            if !round {
                break;
            }
        }
        changed
    })
}

/// GVN key: opcode + resolved operands + payload + result types (the
/// types disambiguate `iconst.i32 0` from `iconst.i64 0` and the
/// conversion family).
#[derive(PartialEq, Eq, Hash)]
struct Key {
    op: Opcode,
    args: Vec<Value>,
    aux: AuxKey,
    result_tys: Vec<TypeId>,
}

/// `Aux` with edge payloads excluded (terminators are never GVN'd).
#[derive(PartialEq, Eq, Hash)]
enum AuxKey {
    None,
    Int(i64),
    FloatBits(u64),
    Bool(bool),
    IntCc(IntCc),
    FloatCc(crate::ops::FloatCc),
    Scale(u64),
    Data(u32),
    Callee(String),
}

fn aux_key(f: &Function, aux: Aux) -> Option<AuxKey> {
    Some(match aux {
        Aux::None => AuxKey::None,
        Aux::Int(n) => AuxKey::Int(n),
        Aux::FloatBits(b) => AuxKey::FloatBits(b),
        Aux::Bool(b) => AuxKey::Bool(b),
        Aux::IntCc(cc) => AuxKey::IntCc(cc),
        Aux::FloatCc(cc) => AuxKey::FloatCc(cc),
        Aux::Scale(s) => AuxKey::Scale(s),
        Aux::Data(d) => AuxKey::Data(d),
        // `func.addr` payload: keyed by callee NAME (ExtFunc ids are
        // function-local but names are the identity).
        Aux::Callee(ef) => AuxKey::Callee(f.ext_funcs[ef].name.clone()),
        Aux::Jump(..) | Aux::Br(..) | Aux::Trap(..) => return None,
    })
}

/// What a fold decided.
enum Fold {
    None,
    Int(i64),
    Bool(bool),
    /// Replace the result with an existing value.
    Value(Value),
    /// X3: the op provably traps — the block ends here, with the kind.
    Trap(TrapKind),
}

/// One eager rewrite round over the dominator-tree preorder.
fn gvn_fold_round(
    f: &mut Function,
    types: &TypeInterner,
    ctx: &mut PassCtx,
    stats: &mut OptStats,
) -> bool {
    let cfg = analysis::cfg(f);
    let doms = analysis::dominators(&cfg);
    // Dominator-tree preorder: RPO filtered to a DFS over idom children
    // (RPO itself is a valid preorder for dominance-scoped tables:
    // every dominator precedes its dominated blocks in RPO).
    let order = cfg.rpo.clone();
    let mut table: HashMap<Key, Vec<Value>> = HashMap::new();
    let mut owner: HashMap<Value, Block> = HashMap::new(); // first result -> defining block
    let mut repl: HashMap<Value, Value> = HashMap::new();
    let mut changed = false;

    for &b in &order {
        let insts = f.blocks[b].insts.clone();
        let mut kept: Vec<Inst> = Vec::with_capacity(insts.len());
        let mut trap: Option<TrapKind> = None;
        for inst in insts {
            // Resolve operands through the replacement map, in place.
            let args_list = f.insts[inst].args;
            for (i, v) in f.vpool.get(args_list).into_iter().enumerate() {
                let r = analysis::resolve(&repl, v);
                if r != v {
                    f.vpool.set(args_list, i, r);
                    changed = true;
                }
            }
            match f.insts[inst].aux {
                Aux::Jump(bc) => resolve_edge(f, bc, &repl, &mut changed),
                Aux::Br(t, e) => {
                    resolve_edge(f, t, &repl, &mut changed);
                    resolve_edge(f, e, &repl, &mut changed);
                }
                _ => {}
            }
            let op = f.insts[inst].op;
            if op.is_terminator() {
                if fold_branch(f, inst) {
                    stats.branch_folds += 1;
                    changed = true;
                }
                kept.push(inst);
                continue;
            }
            // (1) Constant folding — X3 rules identical to the
            // construction-time gauntlet.
            match fold_inst(f, types, inst) {
                Fold::Int(n) => {
                    morph_const(f, inst, Aux::Int(n), Opcode::Iconst);
                    stats.folds += 1;
                    changed = true;
                    kept.push(inst);
                    continue;
                }
                Fold::Bool(v) => {
                    morph_const(f, inst, Aux::Bool(v), Opcode::Bconst);
                    stats.folds += 1;
                    changed = true;
                    kept.push(inst);
                    continue;
                }
                Fold::Value(v) => {
                    let res = f.vpool.get(f.insts[inst].results);
                    repl.insert(res[0], v);
                    retire_value_facts(f, res[0], ctx);
                    stats.folds += 1;
                    changed = true;
                    continue; // inst dropped
                }
                Fold::Trap(kind) => {
                    trap = Some(kind);
                    stats.folds += 1;
                    changed = true;
                    break; // the block ends here
                }
                Fold::None => {}
            }
            // (2) The declarative identity table (one table, eager).
            let args = f.vpool.get(f.insts[inst].args);
            if args.len() == 2 {
                let same = args[0] == args[1];
                let lc = analysis::const_int(f, args[0]);
                let rc = analysis::const_int(f, args[1]);
                if let Some(rw) = peephole_rules::match_rules(op, same, lc, rc) {
                    let res = f.vpool.get(f.insts[inst].results);
                    match rw {
                        Rewrite::Lhs => {
                            repl.insert(res[0], args[0]);
                            retire_value_facts(f, res[0], ctx);
                            stats.rule_hits += 1;
                            changed = true;
                            continue;
                        }
                        Rewrite::Rhs => {
                            repl.insert(res[0], args[1]);
                            retire_value_facts(f, res[0], ctx);
                            stats.rule_hits += 1;
                            changed = true;
                            continue;
                        }
                        Rewrite::ConstInt(n) => {
                            morph_const(f, inst, Aux::Int(n), Opcode::Iconst);
                            stats.rule_hits += 1;
                            changed = true;
                            kept.push(inst);
                            continue;
                        }
                        Rewrite::ConstBool(v) => {
                            morph_const(f, inst, Aux::Bool(v), Opcode::Bconst);
                            stats.rule_hits += 1;
                            changed = true;
                            kept.push(inst);
                            continue;
                        }
                    }
                }
                // The same-operand `icmp` family rides beside the table.
                if op == Opcode::Icmp
                    && same
                    && let Aux::IntCc(cc) = f.insts[inst].aux
                {
                    morph_const(
                        f,
                        inst,
                        Aux::Bool(peephole_rules::icmp_same(cc)),
                        Opcode::Bconst,
                    );
                    stats.rule_hits += 1;
                    changed = true;
                    kept.push(inst);
                    continue;
                }
            }
            // (3) Dominator-scoped value numbering over pure ops.
            let op = f.insts[inst].op; // may have been morphed above
            if analysis::is_gvn_pure(op)
                && let Some(ak) = aux_key(f, f.insts[inst].aux)
            {
                let res = f.vpool.get(f.insts[inst].results);
                let key = Key {
                    op,
                    args: f.vpool.get(f.insts[inst].args),
                    aux: ak,
                    result_tys: res.iter().map(|&v| f.value_ty(v)).collect(),
                };
                if let Some(prev) = table.get(&key) {
                    // A hit only counts when the previous definition
                    // dominates this use site (RPO order guarantees
                    // dominators come first, not that every earlier
                    // block dominates).
                    let pb = owner[&prev[0]];
                    if doms.dominates(pb, b) {
                        for (&old, &new) in res.iter().zip(prev) {
                            repl.insert(old, new);
                            retire_value_facts(f, old, ctx);
                        }
                        stats.gvn_hits += 1;
                        changed = true;
                        continue;
                    }
                }
                table.insert(key, res.clone());
                owner.insert(res[0], b);
            }
            kept.push(inst);
        }
        f.blocks[b].insts = kept;
        if let Some(kind) = trap {
            f.append_inst(b, Opcode::Trap, &[], &[], Aux::Trap(kind));
        }
    }
    if !repl.is_empty() {
        analysis::replace_uses(f, &repl);
    }
    changed
}

fn resolve_edge(
    f: &mut Function,
    bc: crate::ir::BlockCall,
    repl: &HashMap<Value, Value>,
    changed: &mut bool,
) {
    for (i, v) in f.vpool.get(bc.args).into_iter().enumerate() {
        let r = analysis::resolve(repl, v);
        if r != v {
            f.vpool.set(bc.args, i, r);
            *changed = true;
        }
    }
}

/// Facts on a value the pass is retiring in favor of a replacement:
/// the definition goes away, so the fact goes with it (the replacement
/// value carries its own facts). Honest invalidation, not a drop.
fn retire_value_facts(f: &Function, v: Value, ctx: &mut PassCtx) {
    for id in super::facts_on(f, v) {
        ctx.invalidate(id, Invalidation::ValueDeleted);
    }
}

/// Rewrite an instruction into a constant in place (same result value,
/// same type — folding never changes types).
fn morph_const(f: &mut Function, inst: Inst, aux: Aux, op: Opcode) {
    let empty = f.vpool.intern(&[]);
    let data = &mut f.insts[inst];
    data.op = op;
    data.args = empty;
    data.aux = aux;
}

/// `br` on a constant → `jmp` down the taken edge; `br` with two
/// identical edges → `jmp`.
fn fold_branch(f: &mut Function, inst: Inst) -> bool {
    let Aux::Br(t, e) = f.insts[inst].aux else {
        return false;
    };
    let args = f.vpool.get(f.insts[inst].args);
    let taken = match analysis::const_bool(f, args[0]) {
        Some(true) => t,
        Some(false) => e,
        None => {
            if t == e {
                t
            } else {
                return false;
            }
        }
    };
    let empty = f.vpool.intern(&[]);
    let data = &mut f.insts[inst];
    data.op = Opcode::Jmp;
    data.args = empty;
    data.aux = Aux::Jump(taken);
    true
}

fn wrap_to(v: i128, bits: u32) -> i64 {
    if bits >= 64 {
        return v as i64;
    }
    let m = 1i128 << bits;
    let mut r = v.rem_euclid(m);
    if r >= m / 2 {
        r -= m;
    }
    r as i64
}

/// Constant folding — the same X3 semantics as the construction-time
/// gauntlet in [`crate::build`]: checked ops fold to their trap kind
/// on provable overflow; float identities are never speculated.
fn fold_inst(f: &Function, types: &TypeInterner, inst: Inst) -> Fold {
    let data = &f.insts[inst];
    let args = f.vpool.get(data.args);
    let results = f.vpool.get(data.results);
    let cst = |v: Value| analysis::const_int(f, v);
    let ints = || -> Option<(i64, i64)> { Some((cst(args[0])?, cst(args[1])?)) };
    let bits_of = |v: Value| types.int_bits(f.value_ty(v));
    let bounds_of = |v: Value| types.int_bounds(f.value_ty(v));
    match data.op {
        Opcode::IaddChk | Opcode::IsubChk | Opcode::ImulChk => {
            let Some((a, b)) = ints() else {
                return Fold::None;
            };
            let r = match data.op {
                Opcode::IaddChk => (a as i128) + (b as i128),
                Opcode::IsubChk => (a as i128) - (b as i128),
                _ => (a as i128) * (b as i128),
            };
            let (lo, hi) = bounds_of(results[0]).expect("int op");
            if r < lo || r > hi {
                Fold::Trap(TrapKind::Overflow)
            } else {
                Fold::Int(r as i64)
            }
        }
        Opcode::IdivChk | Opcode::IremChk => {
            if cst(args[1]) == Some(0) {
                return Fold::Trap(TrapKind::DivZero);
            }
            let Some((a, b)) = ints() else {
                return Fold::None;
            };
            let (lo, _) = bounds_of(results[0]).expect("int op");
            if (a as i128) == lo && b == -1 {
                return Fold::Trap(TrapKind::Overflow);
            }
            Fold::Int(if data.op == Opcode::IdivChk {
                a / b
            } else {
                a % b
            })
        }
        Opcode::UaddChk | Opcode::UsubChk | Opcode::UmulChk | Opcode::UdivChk | Opcode::UremChk => {
            if matches!(data.op, Opcode::UdivChk | Opcode::UremChk) && cst(args[1]) == Some(0) {
                return Fold::Trap(TrapKind::DivZero);
            }
            let Some((a, b)) = ints() else {
                return Fold::None;
            };
            let bits = bits_of(results[0]).expect("int op");
            let mask: u128 = if bits >= 64 {
                u64::MAX as u128
            } else {
                (1u128 << bits) - 1
            };
            let (ua, ub) = ((a as u64 as u128) & mask, (b as u64 as u128) & mask);
            let r: u128 = match data.op {
                Opcode::UaddChk => ua + ub,
                Opcode::UsubChk => {
                    if ua < ub {
                        return Fold::Trap(TrapKind::Overflow);
                    }
                    ua - ub
                }
                Opcode::UmulChk => ua * ub,
                _ => {
                    if ub == 0 {
                        return Fold::Trap(TrapKind::DivZero);
                    }
                    if data.op == Opcode::UdivChk {
                        ua / ub
                    } else {
                        ua % ub
                    }
                }
            };
            if r > mask {
                Fold::Trap(TrapKind::Overflow)
            } else {
                Fold::Int(wrap_to(r as i128, bits))
            }
        }
        Opcode::IaddWrap | Opcode::IsubWrap | Opcode::ImulWrap => {
            let Some((a, b)) = ints() else {
                return Fold::None;
            };
            let bits = bits_of(results[0]).expect("int op");
            let r = match data.op {
                Opcode::IaddWrap => (a as i128).wrapping_add(b as i128),
                Opcode::IsubWrap => (a as i128).wrapping_sub(b as i128),
                _ => (a as i128).wrapping_mul(b as i128),
            };
            Fold::Int(wrap_to(r, bits))
        }
        Opcode::IaddSat | Opcode::IsubSat | Opcode::ImulSat => {
            let Some((a, b)) = ints() else {
                return Fold::None;
            };
            let (lo, hi) = bounds_of(results[0]).expect("int op");
            let r = match data.op {
                Opcode::IaddSat => (a as i128) + (b as i128),
                Opcode::IsubSat => (a as i128) - (b as i128),
                _ => (a as i128) * (b as i128),
            };
            Fold::Int(r.clamp(lo, hi) as i64)
        }
        Opcode::Band | Opcode::Bor | Opcode::Bxor => {
            let Some((a, b)) = ints() else {
                return Fold::None;
            };
            Fold::Int(match data.op {
                Opcode::Band => a & b,
                Opcode::Bor => a | b,
                _ => a ^ b,
            })
        }
        Opcode::Shl | Opcode::Lshr | Opcode::Ashr => {
            let Some((a, b)) = ints() else {
                return Fold::None;
            };
            let bits = bits_of(results[0]).expect("int op");
            let sh = (b as u32) & (bits - 1);
            Fold::Int(match data.op {
                Opcode::Shl => wrap_to((a as i128) << sh, bits),
                Opcode::Ashr => a >> sh,
                _ => {
                    let mask = if bits >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << bits) - 1
                    };
                    (((a as u64) & mask) >> sh) as i64
                }
            })
        }
        Opcode::Icmp => {
            let Some((a, b)) = ints() else {
                return Fold::None;
            };
            let Aux::IntCc(cc) = data.aux else {
                return Fold::None;
            };
            let (ua, ub) = (a as u64, b as u64);
            Fold::Bool(match cc {
                IntCc::Eq => a == b,
                IntCc::Ne => a != b,
                IntCc::Slt => a < b,
                IntCc::Sle => a <= b,
                IntCc::Sgt => a > b,
                IntCc::Sge => a >= b,
                IntCc::Ult => ua < ub,
                IntCc::Ule => ua <= ub,
                IntCc::Ugt => ua > ub,
                IntCc::Uge => ua >= ub,
            })
        }
        Opcode::Sext => {
            // `iconst` payloads are sign-extended already.
            match cst(args[0]) {
                Some(n) => Fold::Int(n),
                None => Fold::None,
            }
        }
        Opcode::Zext => {
            let n = cst(args[0]);
            let Some(n) = n else { return Fold::None };
            let sbits = bits_of(args[0]).expect("int op");
            let mask = if sbits >= 64 {
                u64::MAX
            } else {
                (1u64 << sbits) - 1
            };
            Fold::Int(((n as u64) & mask) as i64)
        }
        Opcode::Itrunc => {
            let Some(n) = cst(args[0]) else {
                return Fold::None;
            };
            let bits = bits_of(results[0]).expect("int op");
            Fold::Int(wrap_to(n as i128, bits))
        }
        Opcode::AggGet => {
            // `agg.get.K` over a visible `agg.make` → the field value.
            let Aux::Int(k) = data.aux else {
                return Fold::None;
            };
            let Some(src) = analysis::def_inst(f, args[0]) else {
                return Fold::None;
            };
            if f.insts[src].op != Opcode::AggMake {
                return Fold::None;
            }
            let fields = f.vpool.get(f.insts[src].args);
            match fields.get(k as usize) {
                Some(&v) => Fold::Value(v),
                None => Fold::None,
            }
        }
        Opcode::EuIsErr => {
            let Some(src) = analysis::def_inst(f, args[0]) else {
                return Fold::None;
            };
            match f.insts[src].op {
                Opcode::EuMakeOk => Fold::Bool(false),
                Opcode::EuMakeErr => Fold::Bool(true),
                _ => Fold::None,
            }
        }
        Opcode::EuOk => {
            let Some(src) = analysis::def_inst(f, args[0]) else {
                return Fold::None;
            };
            if f.insts[src].op != Opcode::EuMakeOk {
                return Fold::None;
            }
            match f.vpool.get(f.insts[src].args).first() {
                Some(&v) => Fold::Value(v),
                None => Fold::None,
            }
        }
        Opcode::EuErr => {
            let Some(src) = analysis::def_inst(f, args[0]) else {
                return Fold::None;
            };
            if f.insts[src].op != Opcode::EuMakeErr {
                return Fold::None;
            }
            let mk = f.vpool.get(f.insts[src].args);
            match data.aux {
                Aux::None => Fold::Value(mk[0]),
                Aux::Int(k) => match mk.get(k as usize + 1) {
                    Some(&v) => Fold::Value(v),
                    None => Fold::None,
                },
                _ => Fold::None,
            }
        }
        _ => Fold::None,
    }
}

/// Braun-style trivial block-parameter removal: a non-entry parameter
/// whose incoming arguments are all one value (or the parameter
/// itself, on self edges) IS that value.
fn trivial_params(f: &mut Function, ctx: &mut PassCtx, stats: &mut OptStats) -> bool {
    let cfg = analysis::cfg(f);
    let mut changed = false;
    loop {
        // (block, param index) -> unique incoming value, if trivial.
        let mut incoming: HashMap<(Block, usize), HashSet<Value>> = HashMap::new();
        for &b in &cfg.rpo {
            let Some(&term) = f.blocks[b].insts.last() else {
                continue;
            };
            let mut edges = Vec::new();
            match f.insts[term].aux {
                Aux::Jump(bc) => edges.push(bc),
                Aux::Br(t, e) => {
                    edges.push(t);
                    edges.push(e);
                }
                _ => {}
            }
            for bc in edges {
                if !cfg.reachable.contains(&bc.block) {
                    continue;
                }
                for (i, v) in f.vpool.get(bc.args).into_iter().enumerate() {
                    incoming.entry((bc.block, i)).or_default().insert(v);
                }
            }
        }
        let mut repl: HashMap<Value, Value> = HashMap::new();
        let mut drop_param: Vec<(Block, usize)> = Vec::new();
        for &b in &cfg.rpo {
            if Some(b) == f.entry() {
                continue;
            }
            let params = f.block_params(b);
            for (i, &p) in params.iter().enumerate() {
                let Some(vals) = incoming.get(&(b, i)) else {
                    continue;
                };
                let mut others: Vec<Value> = vals.iter().copied().filter(|&v| v != p).collect();
                others.sort();
                others.dedup();
                if others.len() == 1 && !vals.is_empty() {
                    repl.insert(p, others[0]);
                    drop_param.push((b, i));
                }
            }
        }
        if repl.is_empty() {
            return changed;
        }
        // Remove the parameters and shrink every incoming edge.
        drop_param.sort();
        let mut drop_by_block: HashMap<Block, Vec<usize>> = HashMap::new();
        for (b, i) in drop_param {
            drop_by_block.entry(b).or_default().push(i);
        }
        for (&b, idxs) in &drop_by_block {
            let params = f.block_params(b);
            let kept: Vec<Value> = params
                .iter()
                .enumerate()
                .filter(|(i, _)| !idxs.contains(i))
                .map(|(_, &v)| v)
                .collect();
            f.blocks[b].params = f.vpool.intern(&kept);
            for (i, &p) in params.iter().enumerate() {
                if idxs.contains(&i) {
                    stats.trivial_params += 1;
                    for id in super::facts_on(f, p) {
                        ctx.invalidate(id, Invalidation::ValueDeleted);
                    }
                }
                let _ = p;
            }
        }
        // Rebuild every edge into the shrunken blocks.
        let blocks: Vec<Block> = f.layout.clone();
        for b in blocks {
            let Some(&term) = f.blocks[b].insts.last() else {
                continue;
            };
            let rebuild = |f: &mut Function, bc: crate::ir::BlockCall| -> crate::ir::BlockCall {
                match drop_by_block.get(&bc.block) {
                    None => bc,
                    Some(idxs) => {
                        let kept: Vec<Value> = f
                            .vpool
                            .get(bc.args)
                            .into_iter()
                            .enumerate()
                            .filter(|(i, _)| !idxs.contains(i))
                            .map(|(_, v)| v)
                            .collect();
                        f.block_call(bc.block, &kept)
                    }
                }
            };
            match f.insts[term].aux {
                Aux::Jump(bc) => {
                    let nbc = rebuild(f, bc);
                    f.insts[term].aux = Aux::Jump(nbc);
                }
                Aux::Br(t, e) => {
                    let nt = rebuild(f, t);
                    let ne = rebuild(f, e);
                    f.insts[term].aux = Aux::Br(nt, ne);
                }
                _ => {}
            }
        }
        analysis::replace_uses(f, &repl);
        changed = true;
        // A removed parameter can make another trivial; loop.
    }
}

/// Trap-free dead-code elimination: remove pure, non-trapping
/// instructions whose results are all unused. Never touches the
/// checked family (X3: a dead check still checks).
fn dce(f: &mut Function, ctx: &mut PassCtx, stats: &mut OptStats) -> bool {
    let mut uses: HashMap<Value, usize> = HashMap::new();
    let blocks: Vec<Block> = f.layout.clone();
    for &b in &blocks {
        for &inst in &f.blocks[b].insts {
            for v in f.vpool.get(f.insts[inst].args) {
                *uses.entry(v).or_insert(0) += 1;
            }
            match f.insts[inst].aux {
                Aux::Jump(bc) => {
                    for v in f.vpool.get(bc.args) {
                        *uses.entry(v).or_insert(0) += 1;
                    }
                }
                Aux::Br(t, e) => {
                    for bc in [t, e] {
                        for v in f.vpool.get(bc.args) {
                            *uses.entry(v).or_insert(0) += 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut changed = false;
    // Iterate to a local fixpoint: removing an instruction may kill
    // its operands' last uses.
    loop {
        let mut removed_any = false;
        for &b in blocks.iter().rev() {
            let insts = f.blocks[b].insts.clone();
            let mut kept = Vec::with_capacity(insts.len());
            for inst in insts {
                let op = f.insts[inst].op;
                let results = f.vpool.get(f.insts[inst].results);
                let dead = is_removable(op)
                    && !results.is_empty()
                    && results
                        .iter()
                        .all(|v| uses.get(v).copied().unwrap_or(0) == 0);
                if dead {
                    for v in f.vpool.get(f.insts[inst].args) {
                        if let Some(c) = uses.get_mut(&v) {
                            *c = c.saturating_sub(1);
                        }
                    }
                    for &v in &results {
                        for id in super::facts_on(f, v) {
                            ctx.invalidate(id, Invalidation::ValueDeleted);
                        }
                    }
                    stats.dce_removed += 1;
                    removed_any = true;
                    changed = true;
                } else {
                    kept.push(inst);
                }
            }
            f.blocks[b].insts = kept;
        }
        if !removed_any {
            break;
        }
    }
    changed
}
