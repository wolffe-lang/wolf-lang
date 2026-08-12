//! Value-range analysis on e-SSA/π-node ranges, with the X3 claw-back
//! clients (s42 target 4 + amendment 3).
//!
//! # The e-SSA/π form, without materialization
//!
//! Classic ABCD materializes a π node per branch edge; WIR's block
//! parameters make that redundant for ANALYSIS: a refinement from
//! `br (icmp.CC a b), bT, bF` holds throughout every block dominated
//! by the taken edge whenever the edge target has that edge as its
//! only entry. The analysis therefore keys refinements by
//! (edge-target block, value) and intersects them along a query
//! block's dominator chain — the π environment IS the dominator path.
//! Ranges seed from `iconst` payloads, `range` FACTS (s26's channel:
//! the input, not a re-derivation — D2), checked-op postconditions,
//! and induction lower bounds (a header parameter whose back-edge
//! argument is a positive-step checked increment never descends below
//! its entry value).
//!
//! # Clients
//!
//! - **Check elimination** (X3's claw-back): an overflow check whose
//!   operand ranges prove the result in-bounds rewrites to the wrap
//!   form (identical value when no overflow is possible; the trap arm
//!   was unreachable). Signed and unsigned families both. Division
//!   checks have no unchecked twin in the closed op set and stay.
//! - **Branch folding by ranges**: a `br` whose comparison is decided
//!   by refined ranges becomes a `jmp` — a bounds check dominated by
//!   a proving comparison dies here, trap arm and all.
//! - **Loop-level check versioning** (amendment 3, demand-driven —
//!   the ≥80% backstop): an innermost, call-free loop with remaining
//!   eliminable-class checks gets a guarded fast copy; the guard
//!   bounds the loop-invariant limit so the SAME π machinery proves
//!   the fast copy's checks away on the next analysis round. The slow
//!   copy keeps every check — behavior is identical on both paths,
//!   only the proven-impossible traps are gone from the hot one.
//!   Loops containing calls never version (schedule points, spec/07).
//!
//! The hot-loop elimination RATE is measured here (`loop_checks_seen`
//! / `loop_checks_eliminated`) — the empirical defense of
//! checked-in-release, reported per commit.

use std::collections::{HashMap, HashSet};

use crate::facts::FactKind;
use crate::ir::{Aux, Block, FuncId, Function, Inst, Module, Value, ValueDef};
use crate::ops::{IntCc, Opcode};
use crate::verify::VerifyError;

use super::analysis::{self, Doms};
use super::{ModView, OptStats, Thresholds, run_managed};

type Range = (i128, i128);

pub(crate) fn run(
    m: &mut Module,
    fid: FuncId,
    verify_each: bool,
    th: &Thresholds,
    stats: &mut OptStats,
) -> Result<bool, VerifyError> {
    run_managed(m, fid, "rangeopt", verify_each, |f, view, _ctx| {
        let mut changed = false;
        // Round 1: eliminate what the current ranges prove; count the
        // hot-loop candidates while we are at it.
        let (c, remaining) = eliminate_round(f, view, stats, true);
        changed |= c;
        // Demand-driven versioning: only for candidates the direct
        // round could not prove.
        let twins = version_loops(f, view, th, stats, &remaining);
        if !twins.is_empty() {
            changed = true;
            // Round 2: the guards are dominating branches now; the
            // same machinery proves the fast copies' checks.
            let (c, _) = eliminate_round(f, view, stats, false);
            changed |= c;
            // Metric honesty: an original whose FAST twin lost its
            // check counts as eliminated on the hot path.
            for (_, twin) in twins {
                if !overflow_candidate(f.insts[twin].op) {
                    stats.loop_checks_eliminated += 1;
                }
            }
        }
        changed
    })
}

// ------------------------------------------------------------ ranges ----

struct RangeCx<'a> {
    f: &'a Function,
    view: &'a ModView<'a>,
    doms: &'a Doms,
    /// Refinements per single-entry branch target: (block, value) → range.
    pi: HashMap<(Block, Value), Range>,
    /// Range per value at its DEF site (memoized; π information from
    /// the def block's dominator chain propagates through arithmetic).
    base: HashMap<Value, Range>,
    /// Hypothetical overrides (versioning what-if queries).
    hypo: HashMap<Value, Range>,
    /// Recursion guard.
    visiting: HashSet<Value>,
    /// Instruction placement (batch-computed; the egg discipline).
    place: HashMap<Inst, Block>,
}

fn type_bounds(cx: &RangeCx, v: Value) -> Option<Range> {
    cx.view.types.int_bounds(cx.f.value_ty(v))
}

impl<'a> RangeCx<'a> {
    fn new(f: &'a Function, view: &'a ModView, doms: &'a Doms, cfg: &analysis::Cfg) -> RangeCx<'a> {
        Self::with_hypo(f, view, doms, cfg, HashMap::new())
    }

    /// A context with hypothetical overrides installed BEFORE π
    /// seeding (refinements are computed from operand ranges, so the
    /// what-if must be visible to them — versioning's demand check).
    fn with_hypo(
        f: &'a Function,
        view: &'a ModView,
        doms: &'a Doms,
        cfg: &analysis::Cfg,
        hypo: HashMap<Value, Range>,
    ) -> RangeCx<'a> {
        let mut place = HashMap::new();
        for &b in &cfg.rpo {
            for &i in &f.blocks[b].insts {
                place.insert(i, b);
            }
        }
        let mut cx = RangeCx {
            f,
            view,
            doms,
            pi: HashMap::new(),
            base: HashMap::new(),
            hypo,
            visiting: HashSet::new(),
            place,
        };
        cx.seed_facts();
        cx.seed_pi(cfg);
        cx
    }

    /// `range` facts are input (D2: verified semantics, not hints).
    fn seed_facts(&mut self) {
        for fd in self.f.facts.values() {
            if let FactKind::Range(v, lo, hi) = fd.kind {
                let cur = self.base.get(&v).copied();
                let (clo, chi) = cur.unwrap_or((i128::MIN, i128::MAX));
                self.base.insert(v, (clo.max(lo), chi.min(hi)));
            }
        }
    }

    /// π refinements: for each conditional branch, each edge target
    /// with that edge as its only entry refines the compared values.
    fn seed_pi(&mut self, cfg: &analysis::Cfg) {
        for &b in &cfg.rpo {
            let Some(&term) = self.f.blocks[b].insts.last() else {
                continue;
            };
            let Aux::Br(t, e) = self.f.insts[term].aux else {
                continue;
            };
            let cond = self.f.vpool.get(self.f.insts[term].args)[0];
            let Some(ci) = analysis::def_inst(self.f, cond) else {
                continue;
            };
            if self.f.insts[ci].op != Opcode::Icmp {
                continue;
            }
            let Aux::IntCc(cc) = self.f.insts[ci].aux else {
                continue;
            };
            let cargs = self.f.vpool.get(self.f.insts[ci].args);
            let (a, bb) = (cargs[0], cargs[1]);
            for (edge, holds) in [(t, true), (e, false)] {
                let target = edge.block;
                if target == b {
                    continue; // self-loop edge: the param is a NEW instance
                }
                let single_entry = cfg
                    .preds
                    .get(&target)
                    .is_some_and(|p| p.len() == 1 && p[0] == b)
                    && t.block != e.block;
                if !single_entry {
                    continue;
                }
                self.refine_edge(b, target, cc, holds, a, bb);
            }
        }
    }

    fn refine_edge(
        &mut self,
        at: Block,
        target: Block,
        cc: IntCc,
        holds: bool,
        a: Value,
        b: Value,
    ) {
        // Effective condition on the taken edge.
        let cc = if holds {
            cc
        } else {
            match cc {
                IntCc::Eq => IntCc::Ne,
                IntCc::Ne => IntCc::Eq,
                IntCc::Slt => IntCc::Sge,
                IntCc::Sle => IntCc::Sgt,
                IntCc::Sgt => IntCc::Sle,
                IntCc::Sge => IntCc::Slt,
                IntCc::Ult => IntCc::Uge,
                IntCc::Ule => IntCc::Ugt,
                IntCc::Ugt => IntCc::Ule,
                IntCc::Uge => IntCc::Ult,
            }
        };
        // Operand ranges AS OBSERVED AT THE BRANCH (dominating π
        // entries are already seeded — RPO order): a guard like
        // `n <= K` upstream tightens what `i < n` proves here.
        let ra = self.range_at(a, at);
        let rb = self.range_at(b, at);
        let (Some(ra), Some(rb)) = (ra, rb) else {
            return;
        };
        // Unsigned conditions refine in the signed domain only when
        // both sides are known non-negative.
        let nonneg = ra.0 >= 0 && rb.0 >= 0;
        let mut put = |v: Value, r: Range| {
            let e = self.pi.entry((target, v)).or_insert((i128::MIN, i128::MAX));
            e.0 = e.0.max(r.0);
            e.1 = e.1.min(r.1);
        };
        match cc {
            IntCc::Eq => {
                put(a, rb);
                put(b, ra);
            }
            IntCc::Ne => {}
            IntCc::Slt => {
                put(a, (i128::MIN, rb.1 - 1));
                put(b, (ra.0 + 1, i128::MAX));
            }
            IntCc::Sle => {
                put(a, (i128::MIN, rb.1));
                put(b, (ra.0, i128::MAX));
            }
            IntCc::Sgt => {
                put(a, (rb.0 + 1, i128::MAX));
                put(b, (i128::MIN, ra.1 - 1));
            }
            IntCc::Sge => {
                put(a, (rb.0, i128::MAX));
                put(b, (i128::MIN, ra.1));
            }
            IntCc::Ult if nonneg => {
                put(a, (i128::MIN, rb.1 - 1));
                put(b, (ra.0 + 1, i128::MAX));
            }
            IntCc::Ule if nonneg => {
                put(a, (i128::MIN, rb.1));
                put(b, (ra.0, i128::MAX));
            }
            IntCc::Ugt if nonneg => {
                put(a, (rb.0 + 1, i128::MAX));
                put(b, (i128::MIN, ra.1 - 1));
            }
            IntCc::Uge if nonneg => {
                put(a, (rb.0, i128::MAX));
                put(b, (i128::MIN, ra.1));
            }
            _ => {}
        }
    }

    /// Context-free range of a value (its def, seeded facts, induction
    /// lower bounds; operands queried at the def's own block so π
    /// information propagates through arithmetic).
    fn def_range(&mut self, v: Value) -> Option<Range> {
        if let Some(&r) = self.hypo.get(&v) {
            return Some(r);
        }
        if let Some(&r) = self.base.get(&v) {
            return Some(r);
        }
        let tb = type_bounds(self, v)?;
        if !self.visiting.insert(v) {
            return Some(tb); // cycle (loop-carried): type bounds
        }
        let r = self.def_range_inner(v, tb);
        self.visiting.remove(&v);
        let r = (r.0.max(tb.0), r.1.min(tb.1));
        self.base.insert(v, r);
        Some(r)
    }

    fn def_range_inner(&mut self, v: Value, tb: Range) -> Range {
        match self.f.values[v].def {
            ValueDef::Param(b, i) => self.param_range(v, b, i, tb),
            ValueDef::Result(inst, _) => self.result_range(v, inst, tb),
        }
    }

    /// Induction seeding: a header parameter whose back-edge argument
    /// is `iadd.chk(param, positive const)` (or the wrap form this
    /// pass itself proved) cannot descend below its entry argument's
    /// lower bound; symmetrically for negative steps and upper bounds.
    fn param_range(&mut self, v: Value, b: Block, idx: u16, tb: Range) -> Range {
        // Collect incoming (pred, arg) pairs from the whole layout.
        let mut entry_args: Vec<Value> = Vec::new();
        let mut back_args: Vec<Value> = Vec::new();
        let blocks: Vec<Block> = self.f.layout.clone();
        for pb in blocks {
            let Some(&term) = self.f.blocks[pb].insts.last() else {
                continue;
            };
            let mut edges = Vec::new();
            match self.f.insts[term].aux {
                Aux::Jump(bc) => edges.push(bc),
                Aux::Br(t, e) => {
                    edges.push(t);
                    edges.push(e);
                }
                _ => {}
            }
            for bc in edges {
                if bc.block != b {
                    continue;
                }
                let args = self.f.vpool.get(bc.args);
                let Some(&arg) = args.get(idx as usize) else {
                    continue;
                };
                if self.doms.dominates(b, pb) {
                    back_args.push(arg);
                } else {
                    entry_args.push(arg);
                }
            }
        }
        if entry_args.is_empty() {
            return tb;
        }
        // Entry bound: meet of entry arguments' ranges.
        let mut elo = i128::MAX;
        let mut ehi = i128::MIN;
        for &a in &entry_args {
            let Some(r) = self.def_range(a) else {
                return tb;
            };
            elo = elo.min(r.0);
            ehi = ehi.max(r.1);
        }
        // No back edges: an ordinary join/pass-through parameter IS
        // the merge of its incoming arguments.
        if back_args.is_empty() {
            return (elo, ehi);
        }
        // Monotonicity: every back-edge argument is a checked (or
        // proven-wrap-free) constant-step increment of the parameter.
        let mut pos = true;
        let mut neg = true;
        for &a in &back_args {
            let Some(ai) = analysis::def_inst(self.f, a) else {
                return tb;
            };
            let op = self.f.insts[ai].op;
            if !matches!(op, Opcode::IaddChk | Opcode::IaddWrap | Opcode::IsubChk) {
                return tb;
            }
            let args = self.f.vpool.get(self.f.insts[ai].args);
            if args[0] != v {
                return tb;
            }
            let Some(step) = analysis::const_int(self.f, args[1]) else {
                return tb;
            };
            // The wrap form is only monotone if this pass proved it
            // trap-free, which it did before rewriting — accept it
            // (the rewrite preserved values on all executions).
            let step = if op == Opcode::IsubChk { -step } else { step };
            if step < 0 {
                pos = false;
            }
            if step > 0 {
                neg = false;
            }
            if step == 0 {
                pos = false;
                neg = false;
            }
        }
        if pos {
            (elo, tb.1)
        } else if neg {
            (tb.0, ehi)
        } else {
            tb
        }
    }

    fn result_range(&mut self, v: Value, inst: Inst, tb: Range) -> Range {
        let data = &self.f.insts[inst];
        let args = self.f.vpool.get(data.args);
        // Operand ranges as observed AT THE DEF SITE: π refinements on
        // the def block's dominator chain flow through arithmetic
        // (this is what makes the π environment e-SSA-strength without
        // materialized π nodes).
        let home = self.place.get(&inst).copied();
        let ar = |cx: &mut Self, i: usize| match home {
            Some(b) => cx.range_at(args[i], b),
            None => cx.def_range(args[i]),
        };
        match data.op {
            Opcode::Iconst => match data.aux {
                Aux::Int(n) => (n as i128, n as i128),
                _ => tb,
            },
            // Checked ops: on every execution that continues, the
            // mathematical result was representable — the interval
            // arithmetic meet with type bounds is exact.
            Opcode::IaddChk | Opcode::UaddChk => {
                let (Some(a), Some(b)) = (ar(self, 0), ar(self, 1)) else {
                    return tb;
                };
                (a.0 + b.0, a.1 + b.1)
            }
            Opcode::IsubChk | Opcode::UsubChk => {
                let (Some(a), Some(b)) = (ar(self, 0), ar(self, 1)) else {
                    return tb;
                };
                (a.0 - b.1, a.1 - b.0)
            }
            Opcode::ImulChk | Opcode::UmulChk => {
                let (Some(a), Some(b)) = (ar(self, 0), ar(self, 1)) else {
                    return tb;
                };
                let c = [a.0 * b.0, a.0 * b.1, a.1 * b.0, a.1 * b.1];
                (
                    c.iter().copied().min().expect("nonempty"),
                    c.iter().copied().max().expect("nonempty"),
                )
            }
            Opcode::IremChk => match analysis::const_int(self.f, args[1]) {
                Some(c) if c > 0 => (-(c as i128 - 1), c as i128 - 1),
                _ => tb,
            },
            Opcode::UremChk => match analysis::const_int(self.f, args[1]) {
                Some(c) if c > 0 => (0, c as i128 - 1),
                _ => tb,
            },
            Opcode::UdivChk => {
                // Unsigned divide by a positive constant shrinks
                // non-negative dividends.
                let (Some(a), Some(_)) = (ar(self, 0), ar(self, 1)) else {
                    return tb;
                };
                match analysis::const_int(self.f, args[1]) {
                    Some(c) if c > 0 && a.0 >= 0 => (0, a.1 / c as i128),
                    _ => tb,
                }
            }
            Opcode::IaddSat => {
                let (Some(a), Some(b)) = (ar(self, 0), ar(self, 1)) else {
                    return tb;
                };
                ((a.0 + b.0).clamp(tb.0, tb.1), (a.1 + b.1).clamp(tb.0, tb.1))
            }
            Opcode::IsubSat => {
                let (Some(a), Some(b)) = (ar(self, 0), ar(self, 1)) else {
                    return tb;
                };
                ((a.0 - b.1).clamp(tb.0, tb.1), (a.1 - b.0).clamp(tb.0, tb.1))
            }
            Opcode::Band => {
                // x & mask with a non-negative constant mask.
                let mask = analysis::const_int(self.f, args[0])
                    .or_else(|| analysis::const_int(self.f, args[1]));
                match mask {
                    Some(m) if m >= 0 => (0, m as i128),
                    _ => tb,
                }
            }
            Opcode::Lshr => match analysis::const_int(self.f, args[1]) {
                Some(sh) if sh > 0 => {
                    let bits = self.view.types.int_bits(self.f.value_ty(v)).unwrap_or(64);
                    let sh = (sh as u32) & (bits - 1);
                    if sh == 0 { tb } else { (0, tb.1 >> sh) }
                }
                _ => tb,
            },
            Opcode::Sext => ar(self, 0).unwrap_or(tb),
            Opcode::Zext => {
                let Some(a) = ar(self, 0) else { return tb };
                if a.0 >= 0 {
                    a
                } else {
                    let sbits = self
                        .view
                        .types
                        .int_bits(self.f.value_ty(args[0]))
                        .unwrap_or(64);
                    (0, (1i128 << sbits) - 1)
                }
            }
            Opcode::Itrunc => {
                let Some(a) = ar(self, 0) else { return tb };
                if a.0 >= tb.0 && a.1 <= tb.1 { a } else { tb }
            }
            _ => tb,
        }
    }

    /// Range of `v` as observed inside `block`: def range intersected
    /// with every π refinement on the dominator chain.
    fn range_at(&mut self, v: Value, block: Block) -> Option<Range> {
        let mut r = self.def_range(v)?;
        let mut b = block;
        loop {
            if let Some(&(plo, phi)) = self.pi.get(&(b, v)) {
                r = (r.0.max(plo), r.1.min(phi));
            }
            match self.doms.idom(b) {
                Some(d) => b = d,
                None => break,
            }
        }
        Some(r)
    }
}

// ----------------------------------------------------- elimination ----

fn overflow_candidate(op: Opcode) -> bool {
    matches!(
        op,
        Opcode::IaddChk
            | Opcode::IsubChk
            | Opcode::ImulChk
            | Opcode::UaddChk
            | Opcode::UsubChk
            | Opcode::UmulChk
    )
}

fn wrap_form(op: Opcode) -> Opcode {
    match op {
        Opcode::IaddChk | Opcode::UaddChk => Opcode::IaddWrap,
        Opcode::IsubChk | Opcode::UsubChk => Opcode::IsubWrap,
        Opcode::ImulChk | Opcode::UmulChk => Opcode::ImulWrap,
        other => other,
    }
}

/// Is the check provably trap-free given operand ranges at its block?
fn provable(cx: &mut RangeCx, view: &ModView, f: &Function, inst: Inst, b: Block) -> bool {
    let op = f.insts[inst].op;
    let args = f.vpool.get(f.insts[inst].args);
    let res = f.vpool.get(f.insts[inst].results)[0];
    let Some((tlo, thi)) = view.types.int_bounds(f.value_ty(res)) else {
        return false;
    };
    let bits = view.types.int_bits(f.value_ty(res)).unwrap_or(64);
    let umax: i128 = if bits >= 64 {
        u64::MAX as i128
    } else {
        (1i128 << bits) - 1
    };
    let (Some(a), Some(bb)) = (cx.range_at(args[0], b), cx.range_at(args[1], b)) else {
        return false;
    };
    match op {
        Opcode::IaddChk => a.0 + bb.0 >= tlo && a.1 + bb.1 <= thi,
        Opcode::IsubChk => a.0 - bb.1 >= tlo && a.1 - bb.0 <= thi,
        Opcode::ImulChk => {
            let c = [a.0 * bb.0, a.0 * bb.1, a.1 * bb.0, a.1 * bb.1];
            c.iter().all(|&x| x >= tlo && x <= thi)
        }
        // Unsigned: the signed ranges must pin both operands into the
        // non-negative half so signed and unsigned agree, then the
        // unsigned bound is checked.
        Opcode::UaddChk => a.0 >= 0 && bb.0 >= 0 && a.1 + bb.1 <= umax,
        Opcode::UsubChk => a.0 >= 0 && bb.0 >= 0 && a.0 >= bb.1,
        Opcode::UmulChk => a.0 >= 0 && bb.0 >= 0 && a.1 * bb.1 <= umax,
        _ => false,
    }
}

/// One elimination round. Returns (changed, unproven candidates in
/// call-free innermost loops → slot for their future clone twin).
fn eliminate_round(
    f: &mut Function,
    view: &ModView,
    stats: &mut OptStats,
    count_metrics: bool,
) -> (bool, HashMap<Inst, Option<Inst>>) {
    let cfg = analysis::cfg(f);
    let doms = analysis::dominators(&cfg);
    let loops = analysis::loops(f, &cfg, &doms);
    let mut cx = RangeCx::new(f, view, &doms, &cfg);
    let mut rewrites: Vec<Inst> = Vec::new();
    let mut branch_jmps: Vec<(Inst, bool)> = Vec::new();
    let mut remaining: HashMap<Inst, Option<Inst>> = HashMap::new();
    for &b in &cfg.rpo {
        let in_loop = loops.depth.get(&b).copied().unwrap_or(0) > 0;
        for &inst in &f.blocks[b].insts {
            let op = f.insts[inst].op;
            if overflow_candidate(op) {
                let ok = provable(&mut cx, view, f, inst, b);
                if count_metrics && in_loop {
                    stats.loop_checks_seen += 1;
                }
                if ok {
                    rewrites.push(inst);
                    if count_metrics && in_loop {
                        stats.loop_checks_eliminated += 1;
                    }
                } else if in_loop {
                    remaining.insert(inst, None);
                }
            } else if op == Opcode::Br {
                // Range-decided branches: the dominating comparison
                // proves this one.
                let cond = f.vpool.get(f.insts[inst].args)[0];
                if let Some(ci) = analysis::def_inst(f, cond)
                    && f.insts[ci].op == Opcode::Icmp
                    && let Aux::IntCc(cc) = f.insts[ci].aux
                {
                    let cargs = f.vpool.get(f.insts[ci].args);
                    if let (Some(ra), Some(rb)) =
                        (cx.range_at(cargs[0], b), cx.range_at(cargs[1], b))
                        && let Some(decided) = decide_cc(cc, ra, rb)
                    {
                        branch_jmps.push((inst, decided));
                    }
                }
            }
        }
    }
    let changed = !rewrites.is_empty() || !branch_jmps.is_empty();
    for inst in rewrites {
        let op = f.insts[inst].op;
        f.insts[inst].op = wrap_form(op);
        stats.checks_rewritten += 1;
    }
    for (inst, taken) in branch_jmps {
        let Aux::Br(t, e) = f.insts[inst].aux else {
            continue;
        };
        let edge = if taken { t } else { e };
        let empty = f.vpool.intern(&[]);
        let data = &mut f.insts[inst];
        data.op = Opcode::Jmp;
        data.args = empty;
        data.aux = Aux::Jump(edge);
        stats.branch_folds += 1;
    }
    (changed, remaining)
}

/// Ranges decide a comparison when the intervals are disjoint or
/// nested appropriately.
fn decide_cc(cc: IntCc, a: Range, b: Range) -> Option<bool> {
    let signed = |lt: bool| -> Option<bool> {
        if a.1 < b.0 {
            Some(lt)
        } else if a.0 >= b.1 && !lt && a.0 > b.1 {
            Some(!lt)
        } else {
            None
        }
    };
    match cc {
        IntCc::Slt => {
            if a.1 < b.0 {
                Some(true)
            } else if a.0 >= b.1 {
                Some(false)
            } else {
                None
            }
        }
        IntCc::Sle => {
            if a.1 <= b.0 {
                Some(true)
            } else if a.0 > b.1 {
                Some(false)
            } else {
                None
            }
        }
        IntCc::Sgt => decide_cc(IntCc::Slt, b, a),
        IntCc::Sge => decide_cc(IntCc::Sle, b, a),
        IntCc::Eq => {
            if a.0 == a.1 && b.0 == b.1 && a.0 == b.0 {
                Some(true)
            } else if a.1 < b.0 || b.1 < a.0 {
                Some(false)
            } else {
                None
            }
        }
        IntCc::Ne => decide_cc(IntCc::Eq, a, b).map(|x| !x),
        // Unsigned versions: only when both are known non-negative.
        IntCc::Ult if a.0 >= 0 && b.0 >= 0 => decide_cc(IntCc::Slt, a, b),
        IntCc::Ule if a.0 >= 0 && b.0 >= 0 => decide_cc(IntCc::Sle, a, b),
        IntCc::Ugt if a.0 >= 0 && b.0 >= 0 => decide_cc(IntCc::Sgt, a, b),
        IntCc::Uge if a.0 >= 0 && b.0 >= 0 => decide_cc(IntCc::Sge, a, b),
        _ => {
            let _ = signed;
            None
        }
    }
}

// ------------------------------------------------------ versioning ----

/// Doc anchor for the metric (referenced from the crate root).
pub const HOT_LOOP_NOTE: &str =
    "hot-loop overflow checks: eliminated / seen, the s42 acceptance rate";

/// Demand-driven loop versioning (amendment 3). For each innermost,
/// call-free, single-entry, single-exit-target loop with unproven
/// checks: derive a bound K on the loop-invariant guard limit `n`
/// such that `n <= K` makes the checks provable, clone the loop
/// behind an `icmp.sle n, K` guard, and let the next elimination
/// round prove the fast copy (the guard is an ordinary dominating
/// branch — the π machinery needs nothing new). Returns
/// (original candidate, fast twin) pairs for metric accounting.
///
/// Block-parameter SSA gives loop-closed form for free: live-outs
/// flow through the exit target's parameters, and the clone's exit
/// edges feed the same parameters. Loops with any other out-of-loop
/// use of a loop-defined value are rejected. Loops containing calls
/// never version (schedule points, spec/07 — spawn edges included).
fn version_loops(
    f: &mut Function,
    view: &ModView,
    th: &Thresholds,
    stats: &mut OptStats,
    remaining: &HashMap<Inst, Option<Inst>>,
) -> Vec<(Inst, Inst)> {
    if remaining.is_empty() {
        return Vec::new();
    }
    let cfg = analysis::cfg(f);
    let doms = analysis::dominators(&cfg);
    let loops = analysis::loops(f, &cfg, &doms);
    let mut twins: Vec<(Inst, Inst)> = Vec::new();
    // Innermost loops: contain no other loop's header besides their own.
    let mut plans: Vec<(usize, Vec<Inst>)> = Vec::new();
    for (li, l) in loops.loops.iter().enumerate() {
        let innermost = !loops.loops.iter().enumerate().any(|(oj, o)| {
            oj != li && l.blocks.contains(&o.header) && o.blocks.len() < l.blocks.len()
        });
        if !innermost {
            continue;
        }
        let size: usize = l.blocks.iter().map(|&b| f.blocks[b].insts.len()).sum();
        if size as u32 > th.version_max_insts {
            continue;
        }
        let cands: Vec<Inst> = l
            .blocks
            .iter()
            .flat_map(|&b| f.blocks[b].insts.iter().copied())
            .filter(|i| remaining.contains_key(i))
            .collect();
        if (cands.len() as u32) < th.version_min_checks {
            continue;
        }
        // Call-free (schedule-point conservatism).
        let has_call = l.blocks.iter().any(|&b| {
            f.blocks[b]
                .insts
                .iter()
                .any(|&i| f.insts[i].op == Opcode::Call)
        });
        if has_call {
            continue;
        }
        plans.push((li, cands));
    }
    for (li, cands) in plans {
        let l = &loops.loops[li];
        if let Some(pairs) = version_one(f, view, &cfg, &doms, l, &cands) {
            stats.loops_versioned += 1;
            twins.extend(pairs);
        }
    }
    twins
}

/// Version one loop; `None` when a structural precondition fails.
fn version_one(
    f: &mut Function,
    view: &ModView,
    cfg: &analysis::Cfg,
    doms: &Doms,
    l: &analysis::NaturalLoop,
    cands: &[Inst],
) -> Option<Vec<(Inst, Inst)>> {
    // Single entry: one outside predecessor of the header, ending in a
    // plain jmp (the clean guard landing pad).
    let outside: Vec<Block> = cfg
        .preds
        .get(&l.header)?
        .iter()
        .copied()
        .filter(|p| !l.blocks.contains(p))
        .collect();
    let [ph] = outside[..] else { return None };
    let &pterm = f.blocks[ph].insts.last()?;
    let Aux::Jump(entry_edge) = f.insts[pterm].aux else {
        return None;
    };
    if entry_edge.block != l.header {
        return None;
    }
    // Loop-shape discipline: ONE exit target, all of whose
    // predecessors are loop blocks. Loop-defined values used beyond
    // the loop are routed through fresh exit-target parameters below
    // (block-parameter SSA's loop-closed form, materialized on
    // demand) — the single exit target post-dominates both versions,
    // so the rewritten uses stay dominated.
    let defined_in_loop = |f: &Function, v: Value| -> bool {
        match f.values[v].def {
            ValueDef::Param(b, _) => l.blocks.contains(&b),
            ValueDef::Result(i, _) => f
                .layout
                .iter()
                .any(|&b| l.blocks.contains(&b) && f.blocks[b].insts.contains(&i)),
        }
    };
    let mut exits: Vec<Block> = Vec::new();
    for &b in &l.blocks {
        let Some(&term) = f.blocks[b].insts.last() else {
            continue;
        };
        let check = |bc: crate::ir::BlockCall, exits: &mut Vec<Block>| {
            if !l.blocks.contains(&bc.block) && !exits.contains(&bc.block) {
                exits.push(bc.block);
            }
        };
        match f.insts[term].aux {
            Aux::Jump(bc) => check(bc, &mut exits),
            Aux::Br(t, e) => {
                check(t, &mut exits);
                check(e, &mut exits);
            }
            _ => {}
        }
    }
    let [exit_bb] = exits[..] else { return None };
    if cfg
        .preds
        .get(&exit_bb)
        .is_some_and(|ps| ps.iter().any(|p| !l.blocks.contains(p)))
    {
        return None;
    }
    // Live-outs: loop-defined values used anywhere outside the loop.
    let mut live_out: Vec<Value> = Vec::new();
    for &b in &cfg.rpo {
        if l.blocks.contains(&b) {
            continue;
        }
        for &inst in &f.blocks[b].insts {
            let note = |v: Value, lo: &mut Vec<Value>| {
                if defined_in_loop(f, v) && !lo.contains(&v) {
                    lo.push(v);
                }
            };
            for v in f.vpool.get(f.insts[inst].args) {
                note(v, &mut live_out);
            }
            match f.insts[inst].aux {
                Aux::Jump(bc) => {
                    for v in f.vpool.get(bc.args) {
                        note(v, &mut live_out);
                    }
                }
                Aux::Br(t, e) => {
                    for bc in [t, e] {
                        for v in f.vpool.get(bc.args) {
                            note(v, &mut live_out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    live_out.sort();
    // The invariant limit: the header's own guard comparison, one side
    // defined outside the loop.
    let &hterm = f.blocks[l.header].insts.last()?;
    let Aux::Br(..) = f.insts[hterm].aux else {
        return None;
    };
    let cond = f.vpool.get(f.insts[hterm].args)[0];
    let ci = analysis::def_inst(f, cond)?;
    if f.insts[ci].op != Opcode::Icmp {
        return None;
    }
    let cargs = f.vpool.get(f.insts[ci].args);
    let n = *cargs.iter().find(|&&v| !defined_in_loop(f, v))?;
    let n_ty = f.value_ty(n);
    let (tlo, thi) = view.types.int_bounds(n_ty)?;
    // K search: a bounded power-of-two ladder of what-if queries
    // (demand-driven, iteration-count bounded — D4). Prefer the
    // LARGEST K proving every candidate; fall back to the largest
    // proving any. The what-if installs `n ∈ [type_lo, K]` BEFORE π
    // seeding so refinements see it.
    let mut best: Option<(i128, usize)> = None;
    let mut k_try = thi / 2;
    for _ in 0..48 {
        if k_try <= tlo.max(1) {
            break;
        }
        let hypo: HashMap<Value, Range> = [(n, (tlo, k_try))].into_iter().collect();
        let mut cx = RangeCx::with_hypo(f, view, doms, cfg, hypo);
        let proven = cands
            .iter()
            .filter(|&&c| {
                let b = f
                    .layout
                    .iter()
                    .copied()
                    .find(|&bb| f.blocks[bb].insts.contains(&c))
                    .expect("candidate placed");
                provable(&mut cx, view, f, c, b)
            })
            .count();
        if proven == cands.len() {
            best = Some((k_try, proven));
            break; // all proven at the largest K so far — take it
        }
        if proven > 0 && best.is_none_or(|(_, p)| proven > p) {
            best = Some((k_try, proven));
        }
        k_try /= 2;
    }
    let (k, _) = best?;
    // ---- loop-closed form: route live-outs through exit params -----------
    if !live_out.is_empty() {
        use crate::ir::ValueData;
        let old_params = f.block_params(exit_bb);
        let base = old_params.len() as u16;
        let mut new_params: Vec<Value> = Vec::new();
        for (i, &v) in live_out.iter().enumerate() {
            let p = f.values.push(ValueData {
                ty: f.value_ty(v),
                def: ValueDef::Param(exit_bb, base + i as u16),
            });
            new_params.push(p);
        }
        let mut all = old_params.clone();
        all.extend(new_params.iter().copied());
        f.blocks[exit_bb].params = f.vpool.intern(&all);
        // Extend every loop→exit edge with the live-out values.
        for &b in &l.blocks {
            let Some(&term) = f.blocks[b].insts.last() else {
                continue;
            };
            let extend = |f: &mut Function, bc: crate::ir::BlockCall| -> crate::ir::BlockCall {
                if bc.block != exit_bb {
                    return bc;
                }
                let mut args = f.vpool.get(bc.args);
                args.extend(live_out.iter().copied());
                f.block_call(exit_bb, &args)
            };
            match f.insts[term].aux {
                Aux::Jump(bc) => {
                    let nbc = extend(f, bc);
                    f.insts[term].aux = Aux::Jump(nbc);
                }
                Aux::Br(t, e) => {
                    let nt = extend(f, t);
                    let ne = extend(f, e);
                    f.insts[term].aux = Aux::Br(nt, ne);
                }
                _ => {}
            }
        }
        // Rewrite every OUTSIDE use to the corresponding exit param.
        let rewrite: HashMap<Value, Value> = live_out
            .iter()
            .copied()
            .zip(new_params.iter().copied())
            .collect();
        for &b in &cfg.rpo.clone() {
            if l.blocks.contains(&b) {
                continue;
            }
            for &inst in &f.blocks[b].insts.clone() {
                let args_list = f.insts[inst].args;
                for (i, v) in f.vpool.get(args_list).into_iter().enumerate() {
                    if let Some(&np) = rewrite.get(&v) {
                        f.vpool.set(args_list, i, np);
                    }
                }
                match f.insts[inst].aux {
                    Aux::Jump(bc) => {
                        for (i, v) in f.vpool.get(bc.args).into_iter().enumerate() {
                            if let Some(&np) = rewrite.get(&v) {
                                f.vpool.set(bc.args, i, np);
                            }
                        }
                    }
                    Aux::Br(t, e) => {
                        for bc in [t, e] {
                            for (i, v) in f.vpool.get(bc.args).into_iter().enumerate() {
                                if let Some(&np) = rewrite.get(&v) {
                                    f.vpool.set(bc.args, i, np);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    // ---- clone ----------------------------------------------------------
    let loop_blocks: Vec<Block> = cfg
        .rpo
        .iter()
        .copied()
        .filter(|b| l.blocks.contains(b))
        .collect();
    let mut bmap: HashMap<Block, Block> = HashMap::new();
    let mut vmap: HashMap<Value, Value> = HashMap::new();
    for &lb in &loop_blocks {
        let ptys: Vec<_> = f.block_params(lb).iter().map(|&p| f.value_ty(p)).collect();
        let nb = f.make_block(&ptys);
        for (&old, &new) in f.block_params(lb).iter().zip(&f.block_params(nb)) {
            vmap.insert(old, new);
        }
        bmap.insert(lb, nb);
    }
    let mut twin: Vec<(Inst, Inst)> = Vec::new();
    for &lb in &loop_blocks {
        let nb = bmap[&lb];
        let insts = f.blocks[lb].insts.clone();
        for inst in insts {
            let data = f.insts[inst];
            let args: Vec<Value> = f
                .vpool
                .get(data.args)
                .into_iter()
                .map(|v| vmap.get(&v).copied().unwrap_or(v))
                .collect();
            let rtys: Vec<_> = f
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
                        .map(|v| vmap.get(&v).copied().unwrap_or(v))
                        .collect();
                    let tgt = bmap.get(&bc.block).copied().unwrap_or(bc.block);
                    Aux::Jump(f.block_call(tgt, &eargs))
                }
                Aux::Br(t, e) => {
                    let mk = |bc: crate::ir::BlockCall, f: &mut Function| {
                        let eargs: Vec<Value> = f
                            .vpool
                            .get(bc.args)
                            .into_iter()
                            .map(|v| vmap.get(&v).copied().unwrap_or(v))
                            .collect();
                        let tgt = bmap.get(&bc.block).copied().unwrap_or(bc.block);
                        f.block_call(tgt, &eargs)
                    };
                    let nt = mk(t, f);
                    let ne = mk(e, f);
                    Aux::Br(nt, ne)
                }
                other => other,
            };
            f.span_cursor = f.srcspan(inst);
            let (ni, results) = f.append_inst(nb, data.op, &args, &rtys, aux);
            for (old, new) in f.vpool.get(data.results).into_iter().zip(results) {
                vmap.insert(old, new);
            }
            if cands.contains(&inst) {
                twin.push((inst, ni));
            }
        }
    }
    f.span_cursor = None;
    // ---- guard ------------------------------------------------------------
    // G: `%k = iconst K; %c = icmp.sle n, %k; br %c, FP(..), H(..)`.
    // FP is a pass-through FAST PREHEADER: it exists so the guard's
    // taken edge has a single-entry target — the π seeding point that
    // dominates the whole fast loop (headers have back edges and can
    // never be single-entry themselves).
    let htys: Vec<_> = f
        .block_params(l.header)
        .iter()
        .map(|&p| f.value_ty(p))
        .collect();
    let g = f.make_block(&htys);
    let gparams = f.block_params(g);
    let fp = f.make_block(&htys);
    let fpparams = f.block_params(fp);
    let fast_entry = f.block_call(bmap[&l.header], &fpparams);
    f.append_inst(fp, Opcode::Jmp, &[], &[], Aux::Jump(fast_entry));
    let (_, kv) = f.append_inst(g, Opcode::Iconst, &[], &[n_ty], Aux::Int(k as i64));
    let (_, cv) = f.append_inst(
        g,
        Opcode::Icmp,
        &[n, kv[0]],
        &[crate::types::BOOL],
        Aux::IntCc(IntCc::Sle),
    );
    let fast = f.block_call(fp, &gparams);
    let slow = f.block_call(l.header, &gparams);
    f.append_inst(g, Opcode::Br, &[cv[0]], &[], Aux::Br(fast, slow));
    // Retarget the entry edge at the guard.
    let entry_args = f.vpool.get(entry_edge.args);
    let g_edge = f.block_call(g, &entry_args);
    f.insts[pterm].aux = Aux::Jump(g_edge);
    Some(twin)
}
