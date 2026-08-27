//! Loop-carried header promotion (s110 — the c24/s103 midend residue,
//! named in the ledger's push account: ~15% of b3's `_Wmain` is list
//! header traffic).
//!
//! The shape: a hot loop re-loads a container header's fields
//! (data/len/cap) every iteration and stores `len` back every
//! iteration, because the header lives in Foreign(Header) storage
//! whose token is never exhaustive (s80) — `memopt`'s availability
//! dies at the backedge and `licm` refuses the hoist. The C twin
//! keeps all three in registers. This pass promotes provably-safe
//! header cells to loop-carried SSA values: field loads hoist to the
//! preheader, in-loop loads fold to the carried value, in-loop field
//! stores are DEFERRED (the carried value updates instead), and the
//! memory image is re-synchronized by a flush store at every escape
//! point — before any call that can observe the header, and on every
//! loop exit.
//!
//! # The license (proofs the facts machinery already mints — nothing
//! # is hoped)
//!
//! Promotion of region `c`'s cells fires only when EVERY in-loop
//! access that could touch `c`'s storage is placed:
//!
//! - **Element stores** are discharged by the region split itself:
//!   the List protocol threads header and buffer memory on separate
//!   region chains with distinct [`ForeignRole`]s (the s104 pair
//!   scopes, mechanized as WIR roles). A store on a Buffer-role (or
//!   any other-region) chain provably cannot touch Header-role cells.
//!   An access on ANOTHER same-role chain has no such theorem — two
//!   Header-role roots name the same storage class (s80's rule) — and
//!   bails.
//! - **Header-chain ops** must be loads/stores of a single invariant
//!   base pointer at constant, non-overlapping 8-byte offsets — the
//!   header-field shape. Anything else on the chain (a variant
//!   address, an alloc, a freeze) bails.
//! - **Calls** are discharged three ways, in strength order: a callee
//!   whose s102 write set ([`super::memopt::write_sets`]) is
//!   `Params([])` writes no foreign storage and rides over; a call
//!   that cannot reach an EXHAUSTIVE local region without its token
//!   ([`super::memopt::exhaustive_regions`], the s83 theorem) rides
//!   over; a call that versions the `c` chain (takes and mints a `c`
//!   token — the growth-capable `__wolf_rt_list_push` shape) becomes
//!   a BOUNDARY: dirty cells flush before it, every cell reloads
//!   after it, so the fast path between boundaries carries the header
//!   in registers while the growth call keeps its full view. Any
//!   other call — external without the token, `call.ind`, rc traffic
//!   against a non-exhaustive region — is unproven and bails.
//!
//! Unproven = no promotion, and every bail is counted loudly
//! (`header_bail_alias` / `header_bail_call` / `header_bail_shape` in
//! [`OptStats`]): the stats are the evidence surface, per the c24
//! law.
//!
//! # Deferral vs traps
//!
//! A deferred `len` store means the memory image is stale between the
//! store site and the next flush. Checked arithmetic in the loop can
//! trap in that window. The mid-end's model already answers this: a
//! trap is a control effect, not a memory observer — `memopt`'s DSE
//! counts loads and calls as observers and deletes stores a trap
//! would "see", and the write-set walker classifies checked ops as
//! harmless ("a trap is a control effect, not a store"). The verdict
//! and its timing are untouched — no check is added, moved, or
//! removed (X3 intact); only the header cell's store is deferred to
//! the escape points a running program can observe it from.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ir::{Aux, Block, FuncId, Function, Inst, Module, Value, ValueData, ValueDef};
use crate::ops::{ForeignRole, Opcode};
use crate::types::{TypeData, TypeId};
use crate::verify::{Invalidation, PassCtx, VerifyError};

use super::{ModView, OptStats, Thresholds, analysis, run_managed};

pub(crate) fn run(
    m: &mut Module,
    fid: FuncId,
    verify_each: bool,
    th: &Thresholds,
    stats: &mut OptStats,
) -> Result<bool, VerifyError> {
    let wsets = super::memopt::write_sets(m);
    run_managed(m, fid, "hdrprom", verify_each, |f, view, ctx| {
        let foreign = super::memopt::foreign_roles(f, view);
        let exhaustive = super::memopt::exhaustive_regions(f, view);
        let cfg = analysis::cfg(f);
        let doms = analysis::dominators(&cfg);
        let loops = analysis::loops(f, &cfg, &doms);
        let mut changed = false;
        // Innermost-first (the loops vector is size-sorted): a promoted
        // inner loop's preheader loads land inside the outer loop and
        // keep their invariant shape for the outer visit.
        for l in &loops.loops {
            changed |= try_loop(
                f,
                view,
                ctx,
                th,
                &wsets,
                &foreign,
                &exhaustive,
                &cfg,
                l,
                stats,
            );
        }
        changed
    })
}

/// One promotable header cell: an invariant field address of the base.
struct Cell {
    /// Canonical address value (defined outside the loop).
    addr: Value,
    /// Cell type (8-byte scalar).
    ty: TypeId,
    /// Any in-loop store targets this cell (flush needed at escapes).
    dirty: bool,
}

/// What the license concluded about one in-loop call.
#[derive(Clone, Copy, PartialEq)]
enum CallKind {
    /// Provably cannot touch the candidate cells — no action.
    Ride,
    /// Versions the candidate chain: flush dirty cells before, reload
    /// every cell after. Arg/result positions of the `c` token.
    Boundary { tok_arg: usize, tok_res: usize },
}

#[allow(clippy::too_many_arguments)]
fn try_loop(
    f: &mut Function,
    view: &ModView,
    ctx: &mut PassCtx,
    th: &Thresholds,
    wsets: &HashMap<String, super::memopt::WriteSet>,
    foreign: &HashMap<u32, ForeignRole>,
    exhaustive: &HashSet<u32>,
    cfg: &analysis::Cfg,
    l: &analysis::NaturalLoop,
    stats: &mut OptStats,
) -> bool {
    let region_of = |f: &Function, v: Value| -> Option<u32> {
        match view.types.get(f.value_ty(v)) {
            TypeData::Mem(r) => Some(crate::entity::EntityRef::as_u32(*r)),
            _ => None,
        }
    };
    let loop_insts: HashSet<Inst> = l
        .blocks
        .iter()
        .flat_map(|&b| f.blocks[b].insts.iter().copied())
        .collect();
    let defined_in_loop = |f: &Function, v: Value| -> bool {
        match f.values[v].def {
            ValueDef::Param(b, _) => l.blocks.contains(&b),
            ValueDef::Result(i, _) => loop_insts.contains(&i),
        }
    };
    // Deterministic in-loop block order.
    let loop_rpo: Vec<Block> = cfg
        .rpo
        .iter()
        .copied()
        .filter(|b| l.blocks.contains(b))
        .collect();

    // ---- survey: which regions have header-shaped loads here? ----------
    #[derive(Default)]
    struct Acc {
        loads: Vec<Inst>,
        stores: Vec<Inst>,
        /// A non-load/store/call/terminator op touches this region's
        /// tokens — not a header shape.
        other: bool,
    }
    let mut acc: BTreeMap<u32, Acc> = BTreeMap::new();
    let mut calls: Vec<Inst> = Vec::new();
    for &b in &loop_rpo {
        for &inst in &f.blocks[b].insts {
            let op = f.insts[inst].op;
            let args = f.vpool.get(f.insts[inst].args);
            match op {
                Opcode::Load => {
                    if let Some(&tok) = args.get(1)
                        && let Some(r) = region_of(f, tok)
                    {
                        acc.entry(r).or_default().loads.push(inst);
                    }
                }
                Opcode::Store => {
                    if let Some(&tok) = args.get(2)
                        && let Some(r) = region_of(f, tok)
                    {
                        acc.entry(r).or_default().stores.push(inst);
                    }
                }
                Opcode::Call | Opcode::CallInd => calls.push(inst),
                // Edges carry tokens as a matter of course (chain
                // threading); anything else touching a token makes the
                // region not-a-header for this loop.
                Opcode::Jmp | Opcode::Br => {}
                _ => {
                    for &v in args.iter().chain(f.vpool.get(f.insts[inst].results).iter()) {
                        if let Some(r) = region_of(f, v) {
                            acc.entry(r).or_default().other = true;
                        }
                    }
                }
            }
        }
    }

    let eight_byte = |ty: TypeId| ty == crate::types::I64 || ty == crate::types::PTR;
    // (base, byte offset) of a field address, on constant `ptr.off`s.
    let decompose = |f: &Function, mut v: Value| -> Option<(Value, i64)> {
        let mut off: i64 = 0;
        loop {
            let Some(i) = analysis::def_inst(f, v) else {
                return Some((v, off));
            };
            if f.insts[i].op != Opcode::PtrOff {
                return Some((v, off));
            }
            let args = f.vpool.get(f.insts[i].args);
            let idx = analysis::const_int(f, args[1])?;
            let Aux::Scale(s) = f.insts[i].aux else {
                return None;
            };
            off = idx.checked_mul(i64::try_from(s).ok()?)?.checked_add(off)?;
            v = args[0];
        }
    };

    let candidates: Vec<u32> = acc
        .iter()
        .filter(|(_, a)| {
            a.loads.iter().any(|&ld| {
                let addr = f.vpool.get(f.insts[ld].args)[0];
                !defined_in_loop(f, addr)
            })
        })
        .map(|(&r, _)| r)
        .collect();
    if candidates.is_empty() {
        return false;
    }

    let mut changed = false;
    'region: for c in candidates {
        let a = &acc[&c];
        if a.other {
            stats.header_bail_shape += 1;
            continue;
        }
        // ---- cells: single invariant base, constant 8-byte fields ----
        let mut base: Option<Value> = None;
        let mut cells: BTreeMap<i64, Cell> = BTreeMap::new();
        let mut folds: Vec<(Inst, i64)> = Vec::new(); // in-loop loads to fold
        let mut defers: Vec<(Inst, i64)> = Vec::new(); // in-loop stores to defer
        for (&inst, is_store) in a
            .loads
            .iter()
            .map(|i| (i, false))
            .chain(a.stores.iter().map(|i| (i, true)))
        {
            let args = f.vpool.get(f.insts[inst].args);
            let addr = args[if is_store { 1 } else { 0 }];
            let ty = if is_store {
                f.value_ty(args[0])
            } else {
                f.value_ty(f.vpool.get(f.insts[inst].results)[0])
            };
            let placed = (|| -> bool {
                if defined_in_loop(f, addr) || !eight_byte(ty) {
                    return false;
                }
                let Some((b, off)) = decompose(f, addr) else {
                    return false;
                };
                if *base.get_or_insert(b) != b {
                    return false;
                }
                match cells.get_mut(&off) {
                    Some(cell) => {
                        if cell.ty != ty {
                            return false;
                        }
                        cell.dirty |= is_store;
                    }
                    None => {
                        cells.insert(
                            off,
                            Cell {
                                addr,
                                ty,
                                dirty: is_store,
                            },
                        );
                    }
                }
                true
            })();
            if !placed {
                stats.header_bail_alias += 1;
                continue 'region;
            }
            if is_store {
                defers.push((inst, decompose(f, addr).expect("placed").1));
            } else {
                folds.push((inst, decompose(f, addr).expect("placed").1));
            }
        }
        // Non-overlap: 8-byte cells at offsets less than 8 apart alias.
        let offs: Vec<i64> = cells.keys().copied().collect();
        if offs.windows(2).any(|w| w[1] - w[0] < 8) {
            stats.header_bail_alias += 1;
            continue;
        }
        if (folds.len() + defers.len()) < th.hdrprom_min_ops as usize {
            continue; // legal but not worth carrying params for
        }
        // ---- same-role interference: another chain over the same
        // storage class has no disjointness theorem (s80) ----
        if let Some(&role) = foreign.get(&c) {
            for (&r2, a2) in &acc {
                if r2 != c
                    && foreign.get(&r2) == Some(&role)
                    && (!a2.loads.is_empty() || !a2.stores.is_empty() || a2.other)
                {
                    stats.header_bail_alias += 1;
                    continue 'region;
                }
            }
        }
        let local_exhaustive = foreign.get(&c).is_none() && exhaustive.contains(&c);
        // ---- calls: ride, boundary, or bail ----
        let mut call_kinds: HashMap<Inst, CallKind> = HashMap::new();
        for &ci in &calls {
            let args = f.vpool.get(f.insts[ci].args);
            let results = f.vpool.get(f.insts[ci].results);
            let targs: Vec<usize> = args
                .iter()
                .enumerate()
                .filter(|&(_, &v)| region_of(f, v) == Some(c))
                .map(|(i, _)| i)
                .collect();
            let tress: Vec<usize> = results
                .iter()
                .enumerate()
                .filter(|&(_, &v)| region_of(f, v) == Some(c))
                .map(|(i, _)| i)
                .collect();
            let kind = if !targs.is_empty() {
                // Versions the chain, or has no usable shape.
                let (&[tok_arg], &[tok_res]) = (&targs[..], &tress[..]) else {
                    stats.header_bail_shape += 1;
                    continue 'region;
                };
                CallKind::Boundary { tok_arg, tok_res }
            } else if local_exhaustive {
                // s83: no token, no effect — the exhaustive theorem.
                CallKind::Ride
            } else if f.insts[ci].op == Opcode::Call
                && let Aux::Callee(ef) = f.insts[ci].aux
                && matches!(
                    wsets.get(f.ext_funcs[ef].name.as_str()),
                    Some(super::memopt::WriteSet::Params(ps)) if ps.is_empty()
                )
            {
                // s102: the callee provably writes no foreign storage.
                CallKind::Ride
            } else {
                stats.header_bail_call += 1;
                continue 'region;
            };
            call_kinds.insert(ci, kind);
        }
        // rc traffic can reach non-exhaustive storage tokenlessly.
        if !local_exhaustive
            && loop_insts.iter().any(|&i| {
                matches!(
                    f.insts[i].op,
                    Opcode::RcDup | Opcode::RcDrop | Opcode::SyncFreeze
                )
            })
        {
            stats.header_bail_call += 1;
            continue;
        }
        let any_dirty = cells.values().any(|cl| cl.dirty);
        let has_boundary = call_kinds
            .values()
            .any(|k| matches!(k, CallKind::Boundary { .. }));

        // ---- structure: preheader, token threading, exits ----
        let Some(preds) = cfg.preds.get(&l.header) else {
            stats.header_bail_shape += 1;
            continue;
        };
        let outside: Vec<Block> = preds
            .iter()
            .copied()
            .filter(|p| !l.blocks.contains(p))
            .collect();
        let [ph] = outside[..] else {
            stats.header_bail_shape += 1;
            continue;
        };
        let Some(&ph_term) = f.blocks[ph].insts.last() else {
            stats.header_bail_shape += 1;
            continue;
        };
        let Aux::Jump(ph_edge) = f.insts[ph_term].aux else {
            stats.header_bail_shape += 1;
            continue;
        };
        // The token the loop enters with, and the header's `c` param
        // when the chain versions inside the loop.
        let hdr_params = f.block_params(l.header);
        let hdr_tok_params: Vec<usize> = hdr_params
            .iter()
            .enumerate()
            .filter(|&(_, &v)| region_of(f, v) == Some(c))
            .map(|(i, _)| i)
            .collect();
        let threaded = any_dirty || has_boundary;
        let (init_tok, hdr_tok) = if threaded {
            let [ix] = hdr_tok_params[..] else {
                stats.header_bail_shape += 1;
                continue;
            };
            (f.vpool.get(ph_edge.args)[ix], Some(hdr_params[ix]))
        } else {
            // Invariant chain: every load's token must be defined
            // outside the loop (one un-versioned token view).
            let mut toks: Vec<Value> = a
                .loads
                .iter()
                .map(|&ld| f.vpool.get(f.insts[ld].args)[1])
                .collect();
            toks.sort_by_key(|v| crate::entity::EntityRef::as_u32(*v));
            toks.dedup();
            let ok = toks.iter().all(|&t| !defined_in_loop(f, t));
            let [t] = toks[..] else {
                stats.header_bail_shape += 1;
                continue;
            };
            if !ok {
                stats.header_bail_shape += 1;
                continue;
            }
            (t, None)
        };

        // Dry-run token walk (threaded case): the applied chain must be
        // derivable — each store/boundary consumes the tracked token.
        let mut tok_out: HashMap<Block, Value> = HashMap::new();
        if threaded {
            let mut ok = true;
            for &b in &loop_rpo {
                let bparams = f.block_params(b);
                let btoks: Vec<Value> = bparams
                    .iter()
                    .copied()
                    .filter(|&v| region_of(f, v) == Some(c))
                    .collect();
                let mut cur = match btoks[..] {
                    [t] => t,
                    [] => {
                        // Single in-loop predecessor already walked.
                        let ps: Vec<Block> = cfg.preds[&b]
                            .iter()
                            .copied()
                            .filter(|p| l.blocks.contains(p))
                            .collect();
                        match ps[..] {
                            [p] if tok_out.contains_key(&p) => tok_out[&p],
                            _ => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                };
                for &inst in &f.blocks[b].insts {
                    let args = f.vpool.get(f.insts[inst].args);
                    match f.insts[inst].op {
                        Opcode::Store if region_of(f, args[2]) == Some(c) => {
                            if args[2] != cur {
                                ok = false;
                                break;
                            }
                            cur = f.vpool.get(f.insts[inst].results)[0];
                        }
                        Opcode::Call | Opcode::CallInd => {
                            if let Some(&CallKind::Boundary { tok_arg, tok_res }) =
                                call_kinds.get(&inst)
                            {
                                if args[tok_arg] != cur {
                                    ok = false;
                                    break;
                                }
                                cur = f.vpool.get(f.insts[inst].results)[tok_res];
                            }
                        }
                        _ => {}
                    }
                }
                if !ok {
                    break;
                }
                tok_out.insert(b, cur);
            }
            if !ok {
                stats.header_bail_shape += 1;
                continue;
            }
        }

        // Exits. With deferral, every exit target must be a private
        // landing pad (unique predecessor) so the flush dominates all
        // outside observers, and in-loop token values must not leak
        // anywhere the flush would invalidate.
        let mut exits: Vec<(Block, Block)> = Vec::new(); // (from, target)
        for &b in &loop_rpo {
            for s in crate::print::successors(f, b) {
                if !l.blocks.contains(&s) {
                    exits.push((b, s));
                }
            }
        }
        exits.sort_by_key(|&(b, s)| (b, s));
        exits.dedup();
        // Loads outside the loop that read a promoted cell through the
        // exiting token: replaced by the carried value (and deleted).
        let mut exit_folds: Vec<(Inst, Block, i64)> = Vec::new(); // (load, from-block, off)
        if any_dirty {
            let mut exit_of: HashMap<Block, Block> = HashMap::new(); // target -> from
            let mut ok = true;
            for &(b, s) in &exits {
                if cfg.preds.get(&s).map(|p| p.len()) != Some(1) || exit_of.insert(s, b).is_some() {
                    ok = false;
                    break;
                }
            }
            if !ok {
                stats.header_bail_shape += 1;
                continue;
            }
            // Every use of an in-loop `c` token outside the loop must
            // be a foldable cell load in the exit landing pad.
            let in_loop_toks: HashSet<Value> = {
                let mut s: HashSet<Value> = hdr_tok.into_iter().collect();
                for &b in &loop_rpo {
                    for &inst in &f.blocks[b].insts {
                        for v in f.vpool.get(f.insts[inst].results) {
                            if region_of(f, v) == Some(c) {
                                s.insert(v);
                            }
                        }
                    }
                    for &p in &f.block_params(b) {
                        if region_of(f, p) == Some(c) {
                            s.insert(p);
                        }
                    }
                }
                s
            };
            for &ob in &cfg.rpo {
                if l.blocks.contains(&ob) {
                    continue;
                }
                for &inst in &f.blocks[ob].insts {
                    let args = f.vpool.get(f.insts[inst].args);
                    let uses_tok = args.iter().any(|v| in_loop_toks.contains(v));
                    let edge_uses = match f.insts[inst].aux {
                        Aux::Jump(bc) => f
                            .vpool
                            .get(bc.args)
                            .iter()
                            .any(|v| in_loop_toks.contains(v)),
                        Aux::Br(t, e) => [t, e].iter().any(|bc| {
                            f.vpool
                                .get(bc.args)
                                .iter()
                                .any(|v| in_loop_toks.contains(v))
                        }),
                        _ => false,
                    };
                    if edge_uses {
                        ok = false;
                        break;
                    }
                    if !uses_tok {
                        continue;
                    }
                    let fold = (|| -> Option<(Inst, Block, i64)> {
                        if f.insts[inst].op != Opcode::Load {
                            return None;
                        }
                        let &from = exit_of.get(&ob)?;
                        if args[1] != tok_out[&from] {
                            return None;
                        }
                        let (b2, off) = decompose(f, args[0])?;
                        if Some(b2) != base || defined_in_loop(f, args[0]) {
                            return None;
                        }
                        let cell = cells.get(&off)?;
                        let lres = f.vpool.get(f.insts[inst].results)[0];
                        (cell.ty == f.value_ty(lres)).then_some((inst, from, off))
                    })();
                    match fold {
                        Some(x) => exit_folds.push(x),
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    break;
                }
            }
            if !ok {
                stats.header_bail_shape += 1;
                continue;
            }
        }

        // -------------------------------------------------- apply ----
        let cell_offs: Vec<i64> = cells.keys().copied().collect();
        let mut repl: HashMap<Value, Value> = HashMap::new();
        let mut dead: Vec<Inst> = Vec::new();
        let invalidate = |f: &Function, ctx: &mut PassCtx, v: Value| {
            for id in super::facts_on(f, v) {
                if f.facts[id].kind.subject() == v {
                    ctx.invalidate(id, Invalidation::ValueDeleted);
                }
            }
        };
        // Preheader loads, in cell order, before the terminator.
        let mut init: HashMap<i64, Value> = HashMap::new();
        for &off in &cell_offs {
            let cell = &cells[&off];
            let (_, res) = f.append_inst(
                ph,
                Opcode::Load,
                &[cell.addr, init_tok],
                &[cell.ty],
                Aux::None,
            );
            init.insert(off, res[0]);
        }
        // append_inst pushed after the terminator; restore order.
        let n = cell_offs.len();
        let insts = &mut f.blocks[ph].insts;
        let tpos = insts.len() - n - 1;
        let tail: Vec<Inst> = insts.split_off(tpos);
        let (term_v, loads_v) = tail.split_at(1);
        f.blocks[ph].insts.extend_from_slice(loads_v);
        f.blocks[ph].insts.extend_from_slice(term_v);

        // Param minting at merge points (header + in-loop joins).
        let needs_params = threaded;
        let mut block_cell_params: HashMap<Block, HashMap<i64, Value>> = HashMap::new();
        if needs_params {
            for &b in &loop_rpo {
                let multi = cfg.preds[&b].len() >= 2 || b == l.header;
                if !multi {
                    continue;
                }
                let old = f.block_params(b);
                let bix = old.len() as u16;
                let mut all = old.clone();
                let mut mine = HashMap::new();
                for (k, &off) in cell_offs.iter().enumerate() {
                    let p = f.values.push(ValueData {
                        ty: cells[&off].ty,
                        def: ValueDef::Param(b, bix + k as u16),
                    });
                    all.push(p);
                    mine.insert(off, p);
                }
                f.blocks[b].params = f.vpool.intern(&all);
                block_cell_params.insert(b, mine);
            }
        }

        // Forward rewrite walk.
        let mem_c_ty = f.value_ty(init_tok);
        let mut out_state: HashMap<Block, HashMap<i64, Value>> = HashMap::new();
        let mut out_tok_applied: HashMap<Block, Value> = HashMap::new();
        for &b in &loop_rpo {
            let mut state: HashMap<i64, Value> = match block_cell_params.get(&b) {
                Some(mine) => mine.clone(),
                None if b == l.header => init.clone(), // un-threaded: constant state
                None => {
                    let ps: Vec<Block> = cfg.preds[&b]
                        .iter()
                        .copied()
                        .filter(|p| l.blocks.contains(p))
                        .collect();
                    match ps[..] {
                        [p] if out_state.contains_key(&p) => out_state[&p].clone(),
                        _ => init.clone(), // un-threaded case only
                    }
                }
            };
            let mut cur_tok = if threaded {
                let bparams = f.block_params(b);
                bparams
                    .iter()
                    .copied()
                    .find(|&v| region_of(f, v) == Some(c))
                    .unwrap_or_else(|| {
                        let ps: Vec<Block> = cfg.preds[&b]
                            .iter()
                            .copied()
                            .filter(|p| l.blocks.contains(p))
                            .collect();
                        out_tok_applied[&ps[0]]
                    })
            } else {
                init_tok
            };
            let orig: Vec<Inst> = f.blocks[b].insts.clone();
            let mut order: Vec<Inst> = Vec::with_capacity(orig.len());
            for inst in orig {
                let op = f.insts[inst].op;
                let args = f.vpool.get(f.insts[inst].args);
                let is_c_load = op == Opcode::Load && region_of(f, args[1]) == Some(c);
                let is_c_store = op == Opcode::Store && region_of(f, args[2]) == Some(c);
                if is_c_load {
                    let off = folds
                        .iter()
                        .find(|&&(i, _)| i == inst)
                        .expect("licensed load")
                        .1;
                    let res = f.vpool.get(f.insts[inst].results)[0];
                    repl.insert(res, analysis::resolve(&repl, state[&off]));
                    invalidate(f, ctx, res);
                    dead.push(inst);
                    stats.header_loads_promoted += 1;
                    continue;
                }
                if is_c_store {
                    let off = defers
                        .iter()
                        .find(|&&(i, _)| i == inst)
                        .expect("licensed store")
                        .1;
                    let tok_res = f.vpool.get(f.insts[inst].results)[0];
                    repl.insert(tok_res, cur_tok);
                    invalidate(f, ctx, tok_res);
                    state.insert(off, analysis::resolve(&repl, args[0]));
                    dead.push(inst);
                    stats.header_stores_deferred += 1;
                    continue;
                }
                if let Some(&CallKind::Boundary { tok_arg, tok_res }) = call_kinds.get(&inst) {
                    // Flush dirty cells before the call.
                    for &off in &cell_offs {
                        if !cells[&off].dirty {
                            continue;
                        }
                        let v = analysis::resolve(&repl, state[&off]);
                        let (si, sres) = f.append_inst(
                            b,
                            Opcode::Store,
                            &[v, cells[&off].addr, cur_tok],
                            &[mem_c_ty],
                            Aux::None,
                        );
                        order.push(si);
                        cur_tok = sres[0];
                    }
                    // The call consumes the flushed chain.
                    let alist = f.insts[inst].args;
                    f.vpool.set(alist, tok_arg, cur_tok);
                    order.push(inst);
                    cur_tok = f.vpool.get(f.insts[inst].results)[tok_res];
                    // Reload every cell after it.
                    for &off in &cell_offs {
                        let (li, lres) = f.append_inst(
                            b,
                            Opcode::Load,
                            &[cells[&off].addr, cur_tok],
                            &[cells[&off].ty],
                            Aux::None,
                        );
                        order.push(li);
                        state.insert(off, lres[0]);
                    }
                    continue;
                }
                order.push(inst);
            }
            f.blocks[b].insts = order;
            out_state.insert(b, state);
            out_tok_applied.insert(b, cur_tok);
        }

        // Edge args into merge points.
        if needs_params {
            for &b in &loop_rpo {
                let Some(&term) = f.blocks[b].insts.last() else {
                    continue;
                };
                let extend = |f: &mut Function,
                              bc: crate::ir::BlockCall,
                              out: &HashMap<i64, Value>,
                              repl: &HashMap<Value, Value>|
                 -> crate::ir::BlockCall {
                    if !block_cell_params.contains_key(&bc.block) {
                        return bc;
                    }
                    let mut args = f.vpool.get(bc.args);
                    for off in cells.keys() {
                        args.push(analysis::resolve(repl, out[off]));
                    }
                    f.block_call(bc.block, &args)
                };
                match f.insts[term].aux {
                    Aux::Jump(bc) => {
                        let nbc = extend(f, bc, &out_state[&b], &repl);
                        f.insts[term].aux = Aux::Jump(nbc);
                    }
                    Aux::Br(t, e) => {
                        let nt = extend(f, t, &out_state[&b], &repl);
                        let ne = extend(f, e, &out_state[&b], &repl);
                        f.insts[term].aux = Aux::Br(nt, ne);
                    }
                    _ => {}
                }
            }
            // Preheader edge carries the initial values.
            let Aux::Jump(bc) = f.insts[ph_term].aux else {
                unreachable!("checked above")
            };
            let mut args = f.vpool.get(bc.args);
            for off in cells.keys() {
                args.push(init[off]);
            }
            let nbc = f.block_call(bc.block, &args);
            f.insts[ph_term].aux = Aux::Jump(nbc);
        }

        // Exit flushes + landing-pad folds.
        if any_dirty {
            for &(from, target) in &exits {
                let mut cur = analysis::resolve(&repl, out_tok_applied[&from]);
                let mut flushed: Vec<Inst> = Vec::new();
                for &off in &cell_offs {
                    if !cells[&off].dirty {
                        continue;
                    }
                    let v = analysis::resolve(&repl, out_state[&from][&off]);
                    let (si, sres) = f.append_inst(
                        target,
                        Opcode::Store,
                        &[v, cells[&off].addr, cur],
                        &[mem_c_ty],
                        Aux::None,
                    );
                    flushed.push(si);
                    cur = sres[0];
                }
                // append_inst put them at the end; the flush leads.
                let insts = &mut f.blocks[target].insts;
                insts.truncate(insts.len() - flushed.len());
                let old = std::mem::take(insts);
                let now = &mut f.blocks[target].insts;
                now.extend_from_slice(&flushed);
                now.extend(old);
            }
            for &(inst, from, off) in &exit_folds {
                let res = f.vpool.get(f.insts[inst].results)[0];
                repl.insert(res, analysis::resolve(&repl, out_state[&from][&off]));
                invalidate(f, ctx, res);
                dead.push(inst);
                stats.header_loads_promoted += 1;
            }
        }

        // Delete folded loads / deferred stores and rewrite uses.
        for &b in &f.layout.clone() {
            f.blocks[b].insts.retain(|i| !dead.contains(i));
        }
        analysis::replace_uses(f, &repl);
        stats.headers_promoted += 1;
        stats.header_cells += cells.len();
        changed = true;
        // One region per loop per run: the survey above is a snapshot
        // of the pre-rewrite body, and a second promotion in the same
        // loop would read it stale (D4: fixed budget, not a fixpoint).
        break;
    }
    changed
}
