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
//! # Loop-carried parameters (s85)
//!
//! D44 held X3 on the argument that checked arithmetic's measured cost
//! was an optimizer gap rather than a semantics problem, and named the
//! gap: a loop that sums bounded values could not prove its
//! accumulator safe, because an accumulator is not an induction
//! variable — its step is a value, not a constant, so the
//! monotonicity rule above says nothing about it. A header parameter
//! is therefore bounded by the MEET of three independent rules, each
//! sound alone:
//!
//! - **monotonicity**, as above, and now CHECKED-only. A `wrap` form
//!   carries no trap and therefore no floor, and `wrapping[T]`
//!   arithmetic lowers to the same opcode this pass mints — the two
//!   are indistinguishable at the opcode, so the stronger reading was
//!   a claim about programs it had not seen.
//! - **trip-scaled accumulation**: `v ± d` once per iteration, with
//!   `d` bounded and the iteration count bounded, bounds `v` by
//!   `entry + trip · extreme(d)`. This is the rule the summation
//!   needed. It also recovers the wrap case the first rule gave up,
//!   honestly: when the whole computed span fits the type, no
//!   intermediate could have wrapped, and that is the same sentence
//!   read as an induction.
//! - **join**: a parameter IS one of its incoming arguments, so a
//!   back-edge argument bounded by its own defining op — a mask, a
//!   remainder — bounds the parameter. `acc = (acc + x) & 0xFFFFF` is
//!   the checksum idiom, and the mask is what makes it finite.
//!
//! The trip bound itself is read off any constant-step induction
//! variable of the same header: it starts at or above its entry floor,
//! advances by exactly `s > 0`, and sits at or below the guard's
//! refinement where the advance is computed. Where nothing bounds the
//! guard's limit, the bound does not close — and that is the demand
//! the versioning client below answers.
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
//! - **Branch folding by RELATION** (s75): intervals cannot prove
//!   `i <u n` from `i <s n`, because the fact is relational and an
//!   interval domain forgets relations the moment it abstracts. So the
//!   π seeding keeps the comparison itself alongside the ranges it
//!   implies: the condition that held on the edge into a single-entry
//!   block, over the same operand PAIR, decides a later comparison
//!   over that pair whenever the two orderings agree (same domain, or
//!   both operands provably non-negative — and the interval channel
//!   supplies exactly that non-negativity). This is what discharges
//!   `l[i]`'s bounds check inside `while i < l.len`, which is the
//!   shape the container work of s75 made common.
//! - **Branch folding by relation, AFFINE** (s78, wolf-lang#82's
//!   sibling finding): a stencil's guard is `i < len - 1` and its
//!   indices are `i - 1`, `i`, `i + 1` — every one an affine offset
//!   away from the pair the guard related, so the same-pair rule
//!   proves none of them. The channel therefore decomposes both sides
//!   of every comparison into `base + constant` and reasons about the
//!   DIFFERENCE of the two bases: a known `bx + p CC by + q` pins an
//!   interval on `d = bx - by`, and a query `bx + k1 cc by + k2` is
//!   decided by that interval. A decomposition step is admitted only
//!   when the machine result IS the mathematical one: a `chk` op has
//!   already trapped otherwise (and a use is dominated by its def),
//!   and a `wrap` op is admitted only when the interval channel
//!   proves it cannot wrap. The same decomposition runs BACKWARD at π
//!   seeding time — refining `len - 1` refines `len` — which is how an
//!   opaque loaded length becomes non-negative inside the loop, the
//!   precondition for crossing between the signed and unsigned
//!   orderings at all.
//! - **Loop-level check versioning** (amendment 3, demand-driven —
//!   the ≥80% backstop): an innermost, call-free loop with remaining
//!   eliminable-class checks gets a guarded fast copy; the guard
//!   bounds the loop-invariant limit so the SAME π machinery proves
//!   the fast copy's checks away on the next analysis round. The slow
//!   copy keeps every check — behavior is identical on both paths,
//!   only the proven-impossible traps are gone from the hot one.
//!   Loops containing calls never version (schedule points, spec/07).
//!   With s85's trip-scaled rule underneath it, this client now fires
//!   for REDUCTIONS as well as scaled indices: `n <= K` above the loop
//!   bounds the trip count, which bounds the sum. The fast copy is not
//!   unchecked arithmetic — it is arithmetic whose check was
//!   discharged once, outside — and the distinction is the whole
//!   difference between an optimization and a change of semantics.
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

/// How many constant-offset steps [`RangeCx::affine_of`] walks. Two is
/// enough for every shape the container work produces (`len - 1`,
/// `i + 1`); the bound is what keeps the analysis D4-budgeted.
const AFFINE_DEPTH: usize = 4;

/// Stand-in for ±∞ in difference intervals — far outside any integer
/// type's range, and far enough from i128's own bounds that the
/// interval algebra cannot overflow.
const INF: i128 = 1i128 << 100;

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
    /// The RELATION that held on the edge into a single-entry target,
    /// as `(cc, a, b)` already oriented to the taken edge (s75). The
    /// interval channel above is a projection of this; keeping the
    /// pair is what lets a later comparison over the same two values
    /// be decided outright.
    rel: HashMap<Block, Vec<(IntCc, Value, Value)>>,
    /// Range per value at its DEF site (memoized; π information from
    /// the def block's dominator chain propagates through arithmetic).
    base: HashMap<Value, Range>,
    /// Affine decomposition memo (s78): `v == affine[v].0 +
    /// affine[v].1` as MATHEMATICAL integers on every execution that
    /// reaches v's definition.
    affine: HashMap<Value, (Value, i128)>,
    /// Hypothetical overrides (versioning what-if queries).
    hypo: HashMap<Value, Range>,
    /// Recursion guard.
    visiting: HashSet<Value>,
    /// Instruction placement (batch-computed; the egg discipline).
    place: HashMap<Inst, Block>,
    /// Trip-count bounds per loop header (s85), memoized: how many
    /// times the header's back edges can be traversed.
    trip: HashMap<Block, Option<i128>>,
    /// Recursion guard for the above (a trip query asks for the
    /// induction variable's range, which asks for the trip count).
    trip_busy: HashSet<Block>,
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
            rel: HashMap::new(),
            base: HashMap::new(),
            affine: HashMap::new(),
            hypo,
            visiting: HashSet::new(),
            place,
            trip: HashMap::new(),
            trip_busy: HashSet::new(),
        };
        cx.seed_facts();
        // The fact seeds are the only memo entries that predate the π
        // environment legitimately; everything else cached while
        // BUILDING that environment saw a partial one, because seeding
        // an edge queries the operand ranges at the branch. Snapshot
        // the seeds, seed π, then drop the derived memos so every
        // client query re-derives against the finished environment.
        // (Without this a trip bound computed mid-seed reads a loop
        // counter as unbounded and memoizes the answer — s85.)
        let seeded = cx.base.clone();
        cx.seed_pi(cfg);
        cx.base = seeded;
        cx.affine.clear();
        cx.trip.clear();
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
        // The relation itself, kept whole: the interval projection
        // below loses it, and it is the only thing that can prove a
        // later comparison over the same pair.
        self.rel.entry(target).or_default().push((cc, a, b));
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
        // Collected first, applied below: an affine subject refines its
        // BASE too (s78), and computing the decomposition needs `self`.
        let mut puts: Vec<(Value, Range)> = Vec::new();
        let mut put = |v: Value, r: Range| puts.push((v, r));
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
        // Backward through the affine channel: refining `len - 1`
        // refines `len` by the same interval, shifted. The identity is
        // exact (see `affine_of`) and the subject's def dominates this
        // branch, so the shifted refinement holds wherever the edge's
        // does.
        for k in 0..puts.len() {
            let (v, r) = puts[k];
            let (base, off) = self.affine_of(v);
            if base != v && off != 0 {
                puts.push((base, (r.0.saturating_sub(off), r.1.saturating_sub(off))));
            }
        }
        for (v, r) in puts {
            let e = self.pi.entry((target, v)).or_insert((i128::MIN, i128::MAX));
            e.0 = e.0.max(r.0);
            e.1 = e.1.min(r.1);
        }
    }

    /// Decompose `v` into `base + offset` over constant-offset add/sub
    /// chains, where the equality holds over the MATHEMATICAL integers
    /// on every execution that reaches v's definition.
    ///
    /// A `chk` step is free: had the mathematical result not been
    /// representable the op would have trapped, and a use is dominated
    /// by its def — so on any execution that observes `v`, no wrap
    /// happened. A `wrap` step (which this very pass mints, so refusing
    /// it would make the analysis weaker on its own second round) is
    /// admitted only when the interval channel proves the mathematical
    /// result in range at the def site. Everything else stops the walk:
    /// `(v, 0)` is always a true decomposition.
    fn affine_of(&mut self, v: Value) -> (Value, i128) {
        if let Some(&r) = self.affine.get(&v) {
            return r;
        }
        let bits = self.view.types.int_bits(self.f.value_ty(v));
        let mut base = v;
        let mut off: i128 = 0;
        for _ in 0..AFFINE_DEPTH {
            let Some(inst) = analysis::def_inst(self.f, base) else {
                break;
            };
            let sign: i128 = match self.f.insts[inst].op {
                Opcode::IaddChk | Opcode::IaddWrap => 1,
                Opcode::IsubChk | Opcode::IsubWrap => -1,
                _ => break,
            };
            let args = self.f.vpool.get(self.f.insts[inst].args);
            let Some(c) = analysis::const_int(self.f, args[1]) else {
                break;
            };
            // One width throughout: an offset means nothing across a
            // truncation, and the operand type is the result type here.
            if self.view.types.int_bits(self.f.value_ty(args[0])) != bits {
                break;
            }
            if matches!(self.f.insts[inst].op, Opcode::IaddWrap | Opcode::IsubWrap)
                && !self.wrap_is_exact(inst, args[0], sign * c as i128)
            {
                break;
            }
            base = args[0];
            off += sign * c as i128;
        }
        let r = (base, off);
        self.affine.insert(v, r);
        r
    }

    /// Does `x + delta` provably stay inside its type at `inst`'s
    /// block? (The wrap-form admission test for [`Self::affine_of`].)
    fn wrap_is_exact(&mut self, inst: Inst, x: Value, delta: i128) -> bool {
        let Some((tlo, thi)) = self.view.types.int_bounds(self.f.value_ty(x)) else {
            return false;
        };
        let r = match self.place.get(&inst).copied() {
            Some(b) => self.range_at(x, b),
            None => self.def_range(x),
        };
        let Some(r) = r else { return false };
        r.0 + delta >= tlo && r.1 + delta <= thi
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

    /// Every (block, argument) pair flowing into parameter `idx` of
    /// `b`, split into entry edges and back edges.
    fn incoming(&self, b: Block, idx: u16) -> (Vec<Value>, Vec<(Block, Value)>) {
        let mut entry_args: Vec<Value> = Vec::new();
        let mut back_args: Vec<(Block, Value)> = Vec::new();
        for &pb in &self.f.layout {
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
                    back_args.push((pb, arg));
                } else {
                    entry_args.push(arg);
                }
            }
        }
        (entry_args, back_args)
    }

    /// A loop-carried parameter's range, as the MEET of three
    /// independent over-approximations (each sound on its own, so
    /// intersecting them is sound):
    ///
    /// - **monotonicity** — a checked constant-step increment never
    ///   descends below its entry floor, because the descent it would
    ///   need is the wrap a checked op traps on instead;
    /// - **trip-scaled accumulation** (s85) — a parameter incremented
    ///   once per iteration by a BOUNDED (not necessarily constant)
    ///   delta cannot travel further than the trip count times that
    ///   delta's extreme. This is the rule a summation needs: `acc =
    ///   acc + (i & 1023)` over a loop of 100000 lands in
    ///   `0..=102_300_000`, and the accumulator's own overflow check
    ///   dies on the ranges rather than on a promise;
    /// - **join** — the parameter is one of its incoming arguments, so
    ///   an argument whose defining op bounds it outright (a mask, a
    ///   remainder) bounds the parameter too.
    fn param_range(&mut self, v: Value, b: Block, idx: u16, tb: Range) -> Range {
        let (entry_args, back_args) = self.incoming(b, idx);
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
        let mut r = tb;
        let mut meet = |x: Option<Range>| {
            if let Some(x) = x {
                r = (r.0.max(x.0), r.1.min(x.1));
            }
        };
        let mono = self.mono_range(v, &back_args, elo, ehi, tb);
        meet(mono);
        let acc = self.accum_range(v, b, &back_args, elo, ehi, tb);
        meet(acc);
        let join = self.join_range(&back_args, elo, ehi);
        meet(join);
        if r.0 > r.1 { tb } else { r }
    }

    /// Monotone constant-step induction. **Checked forms only**: a
    /// checked op traps rather than wrapping, so "never descends below
    /// the entry floor" is a theorem about it. A wrap form carries no
    /// such postcondition — user `wrapping[T]` arithmetic lowers to the
    /// same opcode — and monotonicity claimed from one would be a
    /// proof this pass has no right to. The bounded-trip rule below
    /// recovers the wrap case honestly, when it can.
    fn mono_range(
        &mut self,
        v: Value,
        back: &[(Block, Value)],
        elo: i128,
        ehi: i128,
        tb: Range,
    ) -> Option<Range> {
        let mut pos = true;
        let mut neg = true;
        for &(_, a) in back {
            let ai = analysis::def_inst(self.f, a)?;
            let op = self.f.insts[ai].op;
            if !matches!(op, Opcode::IaddChk | Opcode::IsubChk) {
                return None;
            }
            let args = self.f.vpool.get(self.f.insts[ai].args);
            if args[0] != v {
                return None;
            }
            let step = analysis::const_int(self.f, args[1])?;
            let step = if op == Opcode::IsubChk { -step } else { step };
            if step <= 0 {
                pos = false;
            }
            if step >= 0 {
                neg = false;
            }
        }
        if pos {
            Some((elo, tb.1))
        } else if neg {
            Some((tb.0, ehi))
        } else {
            None
        }
    }

    /// The trip-scaled accumulator bound (s85, D44's first mechanism).
    ///
    /// Every back edge feeds `v ± d` where `d` is bounded but need not
    /// be constant, so after `k` iterations `v = entry + Σ d_i` with
    /// `k ≤ trip`. The bound is `entry + trip · extreme(d)` on each
    /// side. For CHECKED increments the machine value equals that
    /// mathematical one on every execution that reaches the use (an
    /// intermediate that did not fit would have trapped first). For
    /// WRAP increments there is no such postcondition, so the rule is
    /// admitted only when the whole computed span fits the type — in
    /// which case no intermediate could have wrapped, which is the
    /// same statement read as an induction.
    fn accum_range(
        &mut self,
        v: Value,
        header: Block,
        back: &[(Block, Value)],
        elo: i128,
        ehi: i128,
        tb: Range,
    ) -> Option<Range> {
        let trip = self.trip_bound(header)?;
        let mut dlo = i128::MAX;
        let mut dhi = i128::MIN;
        let mut wrapping = false;
        for &(_, a) in back {
            let ai = analysis::def_inst(self.f, a)?;
            // Unsigned checked ops are deliberately absent: their trap
            // boundary is `u64::MAX`, so their result can be negative
            // read as a signed interval, and this whole channel is
            // signed.
            let sign: i128 = match self.f.insts[ai].op {
                Opcode::IaddChk => 1,
                Opcode::IsubChk => -1,
                Opcode::IaddWrap => {
                    wrapping = true;
                    1
                }
                Opcode::IsubWrap => {
                    wrapping = true;
                    -1
                }
                _ => return None,
            };
            let args = self.f.vpool.get(self.f.insts[ai].args);
            if args[0] != v {
                return None;
            }
            // The delta AT THE INCREMENT'S OWN BLOCK: the loop guard's
            // π refinement is what bounds a delta that is itself the
            // induction variable (`acc = acc + i`).
            let d = match self.place.get(&ai).copied() {
                Some(b) => self.range_at(args[1], b)?,
                None => self.def_range(args[1])?,
            };
            let (clo, chi) = if sign > 0 { (d.0, d.1) } else { (-d.1, -d.0) };
            dlo = dlo.min(clo);
            dhi = dhi.max(chi);
        }
        let lo = elo.checked_add(trip.checked_mul(dlo.min(0))?)?;
        let hi = ehi.checked_add(trip.checked_mul(dhi.max(0))?)?;
        if lo > hi {
            return None;
        }
        if wrapping && (lo < tb.0 || hi > tb.1) {
            return None;
        }
        Some((lo, hi))
    }

    /// The join rule: a parameter takes one of its incoming arguments'
    /// values, so an argument bounded by its own defining op bounds the
    /// parameter. `acc = (acc + x) & 0xFFFFF` is the shape — the mask
    /// is what makes a checksum accumulator finite.
    fn join_range(&mut self, back: &[(Block, Value)], elo: i128, ehi: i128) -> Option<Range> {
        let mut lo = elo;
        let mut hi = ehi;
        for &(_, a) in back {
            let r = self.selfbound(a)?;
            lo = lo.min(r.0);
            hi = hi.max(r.1);
        }
        Some((lo, hi))
    }

    /// The range of a value whose defining op bounds it WITHOUT
    /// consulting operand ranges. The restriction is the point: a
    /// general query here would recurse through the very parameter
    /// being computed, hit the cycle guard, and MEMOIZE type bounds for
    /// a loop-carried value that later rounds could have done better on.
    fn selfbound(&mut self, v: Value) -> Option<Range> {
        let inst = analysis::def_inst(self.f, v)?;
        if !matches!(
            self.f.insts[inst].op,
            Opcode::Iconst | Opcode::Band | Opcode::IremChk | Opcode::UremChk | Opcode::Lshr
        ) {
            return None;
        }
        self.def_range(v)
    }

    /// An upper bound on how many times the back edges of the loop
    /// headed at `h` can be traversed (s85), or `None` when none is
    /// derivable.
    fn trip_bound(&mut self, h: Block) -> Option<i128> {
        if let Some(&t) = self.trip.get(&h) {
            return t;
        }
        // A trip query asks for the induction variable's range, which
        // asks for the trip count. The guard cuts that cycle: the
        // inner query answers from the π environment alone, which is
        // exactly the information the bound is built from.
        if !self.trip_busy.insert(h) {
            return None;
        }
        let mut best: Option<i128> = None;
        for (i, j) in self.f.block_params(h).into_iter().enumerate() {
            if let Some(t) = self.trip_from_iv(h, j, i as u16) {
                best = Some(best.map_or(t, |b: i128| b.min(t)));
            }
        }
        self.trip_busy.remove(&h);
        self.trip.insert(h, best);
        best
    }

    /// The trip bound read off one induction variable: `j` starts at or
    /// above `elo`, advances by exactly `s > 0` per traversal, and sits
    /// at or below `hi` at the point the advance is computed — so the
    /// traversal count cannot exceed `(hi - elo) / s + 1`.
    ///
    /// `hi` comes from the π environment at the back edge's own block,
    /// which is where a `while j < n` guard has already refined it.
    fn trip_from_iv(&mut self, h: Block, j: Value, idx: u16) -> Option<i128> {
        let (entry, back) = self.incoming(h, idx);
        if entry.is_empty() || back.is_empty() {
            return None;
        }
        let jtb = type_bounds(self, j)?;
        let mut elo = i128::MAX;
        for &a in &entry {
            elo = elo.min(self.def_range(a)?.0);
        }
        let mut step: Option<i128> = None;
        let mut hi = i128::MIN;
        for &(from, a) in &back {
            let ai = analysis::def_inst(self.f, a)?;
            let op = self.f.insts[ai].op;
            let sign: i128 = match op {
                Opcode::IaddChk | Opcode::IaddWrap => 1,
                Opcode::IsubChk | Opcode::IsubWrap => -1,
                _ => return None,
            };
            let args = self.f.vpool.get(self.f.insts[ai].args);
            if args[0] != j {
                return None;
            }
            let s = sign * analysis::const_int(self.f, args[1])? as i128;
            if s <= 0 {
                return None;
            }
            if *step.get_or_insert(s) != s {
                return None;
            }
            let r = self.pi_at(j, from)?;
            // A wrap-form advance only counts if it provably does not
            // wrap: a wrapped step restarts the variable at the bottom
            // of the type and the count below would be a fiction.
            if matches!(op, Opcode::IaddWrap | Opcode::IsubWrap) && r.1.checked_add(s)? > jtb.1 {
                return None;
            }
            hi = hi.max(r.1);
        }
        let s = step?;
        if elo == i128::MAX || hi == i128::MIN {
            return None;
        }
        let span = hi.checked_sub(elo)?;
        if span < 0 {
            return Some(0);
        }
        Some(span / s + 1)
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
            // Wrap forms: no trap means no free postcondition, so the
            // interval is the mathematical one only when it provably
            // fits. It often does — this pass mints wrap forms out of
            // checks it proved, and refusing to read them back would
            // make the analysis weaker on its own second round.
            Opcode::IaddWrap | Opcode::IsubWrap | Opcode::ImulWrap => {
                let op = self.f.insts[inst].op;
                let (Some(a), Some(b)) = (ar(self, 0), ar(self, 1)) else {
                    return tb;
                };
                let cand = match op {
                    Opcode::IaddWrap => (a.0 + b.0, a.1 + b.1),
                    Opcode::IsubWrap => (a.0 - b.1, a.1 - b.0),
                    _ => {
                        let c = [a.0 * b.0, a.0 * b.1, a.1 * b.0, a.1 * b.1];
                        (
                            c.iter().copied().min().expect("nonempty"),
                            c.iter().copied().max().expect("nonempty"),
                        )
                    }
                };
                if cand.0 >= tb.0 && cand.1 <= tb.1 {
                    cand
                } else {
                    tb
                }
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

    /// The π-ONLY view of `v` inside `block`: type bounds intersected
    /// with the refinements on `block`'s dominator chain, without ever
    /// consulting `v`'s own definition.
    ///
    /// [`Self::trip_bound`] needs exactly this. The upper bound it
    /// wants is the loop guard's, and asking [`Self::range_at`] for it
    /// would recurse into the induction variable's definition — whose
    /// own range is one of the things the trip bound is computed FOR.
    /// The cycle guard would answer that query with type bounds and
    /// then memoize the answer, so the ordering of the first query
    /// would decide the precision of every later one.
    fn pi_at(&mut self, v: Value, block: Block) -> Option<Range> {
        let mut r = match self.hypo.get(&v) {
            Some(&h) => h,
            None => type_bounds(self, v)?,
        };
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

    /// Is `icmp.cc a, b` decided by a comparison over the SAME pair
    /// that held on the way into `block` (s75)? Walks the dominator
    /// chain, newest first, and stops at the first relation that
    /// decides — a relation that does not decide is no reason to stop
    /// looking at the ones above it.
    fn decide_rel(&mut self, cc: IntCc, a: Value, b: Value, block: Block) -> Option<bool> {
        // Crossing between the signed and unsigned orderings is sound
        // exactly when both operands sit in the non-negative half,
        // where the two agree. The interval channel answers that, and
        // in the shape this exists for it answers YES: `i` is an
        // induction variable from 0, and the very edge that carries
        // `i < n` refines `n` to at least `i`'s floor plus one.
        let mut nonneg: Option<bool> = None;
        let mut cur = Some(block);
        while let Some(bb) = cur {
            if let Some(known) = self.rel.get(&bb).cloned() {
                for (kcc, ka, kb) in known {
                    let oriented = if (ka, kb) == (a, b) {
                        kcc
                    } else if (ka, kb) == (b, a) {
                        swap_cc(kcc)
                    } else {
                        continue;
                    };
                    let (kmask, kdom) = cc_mask(oriented);
                    let (qmask, qdom) = cc_mask(cc);
                    let same_domain = match (kdom, qdom) {
                        (None, _) | (_, None) => true,
                        (Some(x), Some(y)) => x == y,
                    };
                    if !same_domain {
                        let both_nonneg = *nonneg.get_or_insert_with(|| {
                            let ra = self.range_at(a, block);
                            let rb = self.range_at(b, block);
                            matches!((ra, rb), (Some(ra), Some(rb)) if ra.0 >= 0 && rb.0 >= 0)
                        });
                        if !both_nonneg {
                            continue;
                        }
                    }
                    if kmask & !qmask == 0 {
                        return Some(true);
                    }
                    if kmask & qmask == 0 {
                        return Some(false);
                    }
                }
            }
            cur = self.doms.idom(bb);
        }
        None
    }

    /// The affine generalization (s78): decide `a cc b` from known
    /// relations over the same pair of affine BASES, by intervals on
    /// the bases' difference.
    ///
    /// `a = ba + ka` and `b = bb + kb` reduce the query to
    /// `d = ba - bb  cc  kb - ka`; a known relation over the same two
    /// bases reduces the same way to an interval on `d`. Intervals from
    /// every dominating relation intersect, and the query is decided
    /// when the surviving interval sits entirely inside — or entirely
    /// outside — the query's admitted set. Crossing between the signed
    /// and unsigned orderings needs both sides of the relation being
    /// crossed non-negative, exactly as the same-pair rule does.
    fn decide_rel_affine(&mut self, cc: IntCc, a: Value, b: Value, block: Block) -> Option<bool> {
        let (ba, ka) = self.affine_of(a);
        let (bb, kb) = self.affine_of(b);
        if ba == bb {
            return None; // same base: pure arithmetic, the intervals own it
        }
        let qcc = self.signed_view(cc, a, b, block)?;
        let target = kb.checked_sub(ka)?;
        let mut d: Range = (-INF, INF);
        let mut cur = Some(block);
        while let Some(at) = cur {
            for (kcc, x, y) in self.rel.get(&at).cloned().unwrap_or_default() {
                let (bx, kx) = self.affine_of(x);
                let (by, ky) = self.affine_of(y);
                // Orient the known relation onto (ba - bb).
                let (kcc, p, q) = if (bx, by) == (ba, bb) {
                    (kcc, kx, ky)
                } else if (bx, by) == (bb, ba) {
                    (swap_cc(kcc), ky, kx)
                } else {
                    continue;
                };
                let Some(kcc) = self.signed_view(kcc, x, y, block) else {
                    continue;
                };
                let Some(c) = q.checked_sub(p) else { continue };
                let Some(known) = diff_interval(kcc, c) else {
                    continue;
                };
                d = (d.0.max(known.0), d.1.min(known.1));
                if d.0 > d.1 {
                    return None; // contradiction: claim nothing, ever
                }
                if let Some(v) = decide_cc(qcc, d, (target, target)) {
                    return Some(v);
                }
            }
            cur = self.doms.idom(at);
        }
        None
    }

    /// `cc` read in the SIGNED ordering, or `None` when it cannot be:
    /// an unsigned condition agrees with the signed one exactly on the
    /// non-negative half, and that is what the interval channel is
    /// asked for here.
    fn signed_view(&mut self, cc: IntCc, a: Value, b: Value, block: Block) -> Option<IntCc> {
        let signed = match cc {
            IntCc::Ult => IntCc::Slt,
            IntCc::Ule => IntCc::Sle,
            IntCc::Ugt => IntCc::Sgt,
            IntCc::Uge => IntCc::Sge,
            other => return Some(other),
        };
        let ra = self.range_at(a, block)?;
        let rb = self.range_at(b, block)?;
        (ra.0 >= 0 && rb.0 >= 0).then_some(signed)
    }
}

/// The interval a relation `d CC c` pins on `d` (signed conditions
/// only — [`RangeCx::signed_view`] is the gate).
fn diff_interval(cc: IntCc, c: i128) -> Option<Range> {
    Some(match cc {
        IntCc::Slt => (-INF, c - 1),
        IntCc::Sle => (-INF, c),
        IntCc::Sgt => (c + 1, INF),
        IntCc::Sge => (c, INF),
        IntCc::Eq => (c, c),
        // `!=` admits two intervals; an interval domain cannot hold it.
        _ => return None,
    })
}

// ---------------------------------------------- the ordering algebra ----

const ORD_LT: u8 = 1;
const ORD_EQ: u8 = 2;
const ORD_GT: u8 = 4;

/// The outcomes a condition admits, and the ordering it reads them
/// in (`None` for equality, which both orderings agree on).
fn cc_mask(cc: IntCc) -> (u8, Option<bool>) {
    match cc {
        IntCc::Eq => (ORD_EQ, None),
        IntCc::Ne => (ORD_LT | ORD_GT, None),
        IntCc::Slt => (ORD_LT, Some(true)),
        IntCc::Sle => (ORD_LT | ORD_EQ, Some(true)),
        IntCc::Sgt => (ORD_GT, Some(true)),
        IntCc::Sge => (ORD_GT | ORD_EQ, Some(true)),
        IntCc::Ult => (ORD_LT, Some(false)),
        IntCc::Ule => (ORD_LT | ORD_EQ, Some(false)),
        IntCc::Ugt => (ORD_GT, Some(false)),
        IntCc::Uge => (ORD_GT | ORD_EQ, Some(false)),
    }
}

/// The same condition read with its operands exchanged.
fn swap_cc(cc: IntCc) -> IntCc {
    match cc {
        IntCc::Eq => IntCc::Eq,
        IntCc::Ne => IntCc::Ne,
        IntCc::Slt => IntCc::Sgt,
        IntCc::Sle => IntCc::Sge,
        IntCc::Sgt => IntCc::Slt,
        IntCc::Sge => IntCc::Sle,
        IntCc::Ult => IntCc::Ugt,
        IntCc::Ule => IntCc::Uge,
        IntCc::Ugt => IntCc::Ult,
        IntCc::Uge => IntCc::Ule,
    }
}

/// Is this `br` the guard of a bounds check — the false arm being a
/// block that does nothing but `trap.bounds`? Structural, so it stays
/// true however the check was spelled.
fn bounds_guard(f: &Function, inst: Inst) -> bool {
    let Aux::Br(_, e) = f.insts[inst].aux else {
        return false;
    };
    let insts = &f.blocks[e.block].insts;
    match insts.first() {
        Some(&i) => matches!(
            (f.insts[i].op, f.insts[i].aux),
            (Opcode::Trap, Aux::Trap(crate::ops::TrapKind::Bounds))
        ),
        None => false,
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
                let is_bounds = bounds_guard(f, inst);
                if count_metrics && is_bounds {
                    stats.bounds_checks_seen += 1;
                }
                let cond = f.vpool.get(f.insts[inst].args)[0];
                if let Some(ci) = analysis::def_inst(f, cond)
                    && f.insts[ci].op == Opcode::Icmp
                    && let Aux::IntCc(cc) = f.insts[ci].aux
                {
                    let cargs = f.vpool.get(f.insts[ci].args);
                    let (x, y) = (cargs[0], cargs[1]);
                    let by_range = match (cx.range_at(x, b), cx.range_at(y, b)) {
                        (Some(ra), Some(rb)) => decide_cc(cc, ra, rb),
                        _ => None,
                    };
                    // Intervals first (cheap, and they subsume the
                    // relational answer when they can give one), then
                    // the relation over the same pair, then the same
                    // relation read through affine offsets (s78).
                    let decided = by_range
                        .or_else(|| cx.decide_rel(cc, x, y, b))
                        .or_else(|| cx.decide_rel_affine(cc, x, y, b));
                    if let Some(decided) = decided {
                        branch_jmps.push((inst, decided));
                        if count_metrics && is_bounds && decided {
                            stats.bounds_checks_eliminated += 1;
                        }
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
