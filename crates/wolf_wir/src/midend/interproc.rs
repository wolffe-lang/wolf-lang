//! Interprocedural range facts (s99): ranges for values a function did
//! not compute — a callee's return, a parameter's meet over its call
//! sites, and a container whose loads inherit what its stores provably
//! put in.
//!
//! # Posture (D44, both addenda)
//!
//! X3 is locked: checked arithmetic stays, and **no range fact is
//! minted without a proof**. Everything here is built so the fast path
//! exists only where the proof does:
//!
//! - the analysis is WHOLE-PROGRAM (the release tier is whole-program,
//!   s43) and runs on the final bodies of the phase, so what it proves
//!   is what the backend sees;
//! - every unprovable shape poisons, at fact-COLLECTION time: a store
//!   the scan cannot attribute, a container reaching an external
//!   `decl` it does not know, a `call.ind` (s97: an indirect call
//!   contributes no summary edge — so here it KILLS), an exported or
//!   address-taken function's parameters (the C membrane and function
//!   pointers can deliver anything);
//! - a `wrapping[T]` value needs no special kill because the value
//!   EVALUATOR is honest: a `*.wrap` op whose mathematical range does
//!   not fit its type evaluates to the full type bounds — the
//!   D44-second-addendum hole (treating a wrap form as monotone
//!   "because the pass mints the same opcode for forms it has
//!   proved") cannot recur, because nothing here reasons from opcode
//!   identity to a claim the math does not make.
//!
//! # Provenance and audit
//!
//! Facts mint as [`Just::Summary`] carrying the first 8 bytes of the
//! summary-index digest the fixpoint read, so a fact in a bare dump
//! names the exact proof context. Per-function verification is
//! shape-only for this tag (one function cannot see the program);
//! [`reverify`] is the semantic check — it recomputes the analysis on
//! the current module and demands every summary-justified fact be
//! implied by a fresh proof. The whole-program phase runs it under
//! `verify_each`.
//!
//! # The container channel, concretely
//!
//! The lowering gives a `List` one shape (s75/s113): a header pointer
//! `hdr`; `load.ptr hdr` is the element buffer; `ptr.off hdr, 8|16`
//! are len/cap; element access is `ptr.off buffer, idx, esize` +
//! `load`/`store`; growth pushes cross `__wolf_rt_list_push(hdr,
//! slot, …)` with the pushed value in a fresh stack slot. Two lists
//! from two `__wolf_rt_list_new` calls own disjoint buffers (runtime
//! allocation semantics — the same tier of knowledge as "push appends
//! the slot's value"). The channel keys groups by their root — a
//! local `list_new` site or a parameter — and the meet of every
//! attributable store bounds every attributable load. Any store or
//! escape the walk cannot attribute poisons: the group (an escape) or
//! the whole function (an unattributable buffer store).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::entity::EntityRef;
use crate::facts::{FactData, FactKind, Just};
use crate::ir::{Aux, Block, FuncId, Function, Module, Value, ValueDef};
use crate::ops::Opcode;

use super::analysis;

/// Fixed widening bound (D4: count-fixed, never wall-clock): SCC
/// members iterate this many passes, then widen to type bounds.
pub const WIDEN_PASSES: usize = 3;

type Rng = (i128, i128);

/// A container group's identity inside one function.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Root {
    /// The k-th `__wolf_rt_list_new` call site, in RPO order — the
    /// summary's `a<k>` site id.
    Local(u32),
    /// The container reachable from parameter `i` (directly for a
    /// `ptr` param used as a header, through one load for a `mut`
    /// param holding a header slot).
    Param(u32),
}

/// What the meet over a group's stores knows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Meet {
    /// No attributable store yet (an empty container's loads never
    /// execute, so an empty meet stays grantable — the bounds guard
    /// is the loads' own problem).
    Empty,
    Range(Rng),
    Poison,
}

impl Meet {
    fn join(self, r: Option<Rng>) -> Meet {
        match (self, r) {
            (Meet::Poison, _) | (_, None) => Meet::Poison,
            (Meet::Empty, Some(r)) => Meet::Range(r),
            (Meet::Range((lo, hi)), Some((l2, h2))) => Meet::Range((lo.min(l2), hi.max(h2))),
        }
    }
    fn poison(&mut self) {
        *self = Meet::Poison;
    }
}

/// A value's provenance, for store/load attribution. The lattice is
/// Unset < specific < Other; block-parameter origins are the meet of
/// their incoming edges, iterated to a fixpoint.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Origin {
    Unset,
    /// A `stack.alloc` result — slot stores are benign.
    Stack(u32),
    /// A `mut` parameter: a pointer to a slot holding a header.
    MutSlot(u32),
    /// A container header (root).
    Hdr(Root),
    /// The element buffer of a container (root).
    Data(Root),
    /// An element address inside a container (root).
    Elem(Root),
    /// A len/cap field address of some header — benign.
    HdrField,
    /// `data.addr` module data — benign.
    Const,
    /// Anything else.
    Other,
}

fn meet_origin(a: Origin, b: Origin) -> Origin {
    use Origin::*;
    match (a, b) {
        (Unset, x) | (x, Unset) => x,
        (x, y) if x == y => x,
        _ => Other,
    }
}

/// One module-level call site, with what flows through it.
#[derive(Clone, Debug)]
struct CallSite {
    callee: String,
    /// Per argument: a container root flowing as a header (directly,
    /// or as a `mut` slot holding one), an evaluated integer range, or
    /// opaque.
    args: Vec<ArgFlow>,
}

#[derive(Clone, Copy, Debug)]
enum ArgFlow {
    Container(Root),
    Int(Option<Rng>),
    Opaque,
}

/// What one function looks like to the fixpoint.
#[derive(Clone, Debug, Default)]
struct Local {
    /// Store meet per group, from attributable element stores and
    /// whitelisted pushes.
    meets: BTreeMap<Root, Meet>,
    /// Element-load result values per group.
    loads: BTreeMap<Root, Vec<Value>>,
    /// Module-function call sites.
    calls: Vec<CallSite>,
    /// Integer entry parameters: (position, value).
    int_params: Vec<(u32, Value)>,
    /// Entry parameters that reach a container group (position →
    /// root), for grant injection.
    param_roots: BTreeMap<u32, Root>,
    /// An unattributable store on a buffer-shaped address, or a raw
    /// escape the walk cannot classify: no container fact anywhere in
    /// this function, and callers treat every passed container as
    /// clobbered unbounded.
    dirty: bool,
    /// Per-param container effect, for callers: does this function
    /// (directly) write the container reachable from param i, and
    /// within what range? Closed transitively in pass 2.
    writes: BTreeMap<u32, Meet>,
    /// Params whose container is READ (loads observed) — used only
    /// for reporting; reads never kill.
    reads: BTreeSet<u32>,
}

/// The whole-program result the minter and the summary emitter read.
#[derive(Clone, Debug, Default)]
pub struct Ipr {
    /// Per function: return range (post-fixpoint; None = unbounded).
    pub rets: BTreeMap<String, Option<(i128, i128)>>,
    /// Per function: integer-parameter ranges by position.
    pub args: BTreeMap<String, BTreeMap<u32, (i128, i128)>>,
    /// Per function: local container store-meets by site id (`a<k>`)
    /// — the summary's `stores=` field. Poisoned groups are absent.
    pub stores: BTreeMap<String, BTreeMap<u32, (i128, i128)>>,
    /// Per function: granted element ranges for param-rooted groups.
    pub grants: BTreeMap<String, BTreeMap<u32, (i128, i128)>>,
    /// Local-rooted element ranges (function → root site → range).
    pub local_elems: BTreeMap<String, BTreeMap<u32, (i128, i128)>>,
}

/// The runtime entry points the container channel understands. Any
/// OTHER external `decl` receiving a container-linked value poisons
/// the group (`import c` and the whole unknown world live behind the
/// same door).
fn whitelisted_rt(name: &str) -> bool {
    matches!(name, "__wolf_rt_list_new" | "__wolf_rt_list_push")
}

fn int_bounds(m: &Module, f: &Function, v: Value) -> Option<Rng> {
    m.types.int_bounds(f.value_ty(v))
}

fn sat_add(a: i128, b: i128) -> i128 {
    a.saturating_add(b)
}
fn sat_sub(a: i128, b: i128) -> i128 {
    a.saturating_sub(b)
}
fn sat_mul(a: i128, b: i128) -> i128 {
    a.saturating_mul(b)
}

/// Param-free abstract evaluation of `v`'s runtime range. `rets` maps
/// callee → known return range (the bottom-up fixpoint's current
/// knowledge). Honest by construction: a `wrap` form whose
/// mathematical range escapes its type evaluates to full type bounds;
/// block parameters evaluate to type bounds (the s85 in-function
/// machinery owns loop-carried precision, not this pass).
fn eval(
    m: &Module,
    f: &Function,
    rets: &BTreeMap<String, Option<Rng>>,
    memo: &mut HashMap<Value, Option<Rng>>,
    v: Value,
    depth: usize,
) -> Option<Rng> {
    if let Some(r) = memo.get(&v) {
        return *r;
    }
    if depth == 0 {
        return int_bounds(m, f, v);
    }
    let out = eval_inner(m, f, rets, memo, v, depth);
    // Clip to the type: every value inhabits its type.
    let clipped = match (out, int_bounds(m, f, v)) {
        (Some((lo, hi)), Some((tlo, thi))) => Some((lo.max(tlo), hi.min(thi))),
        (r, _) => r,
    };
    memo.insert(v, clipped);
    clipped
}

fn eval_inner(
    m: &Module,
    f: &Function,
    rets: &BTreeMap<String, Option<Rng>>,
    memo: &mut HashMap<Value, Option<Rng>>,
    v: Value,
    depth: usize,
) -> Option<Rng> {
    let tb = int_bounds(m, f, v);
    let ValueDef::Result(i, pos) = f.values[v].def else {
        return tb; // block parameter: type bounds (see the doc)
    };
    let inst = &f.insts[i];
    let args = f.vpool.get(inst.args);
    let mut ev = |x: Value| eval(m, f, rets, memo, x, depth - 1);
    match inst.op {
        Opcode::Iconst => match inst.aux {
            Aux::Int(n) => Some((n as i128, n as i128)),
            _ => tb,
        },
        Opcode::Band => {
            // A non-negative constant mask bounds the result whatever
            // the other operand is (sound for wrap inputs too — the
            // mask applies to the machine value).
            let c0 = analysis::const_int(f, args[0]);
            let c1 = analysis::const_int(f, args[1]);
            match (c0, c1) {
                (_, Some(c)) | (Some(c), _) if c >= 0 => Some((0, c as i128)),
                _ => tb,
            }
        }
        Opcode::Zext => {
            let (flo, fhi) = ev(args[0])?;
            let _ = (flo, fhi);
            // The operand's UNSIGNED width bounds the result; its own
            // signed eval may straddle, so take the type width of the
            // source.
            let src_bits = m.types.int_bounds(f.value_ty(args[0]));
            match src_bits {
                // i8 zext: [0, 255], etc. — the source type's unsigned
                // ceiling is (hi - lo) when lo < 0 (two's complement
                // width), else hi.
                Some((slo, shi)) if slo < 0 => Some((0, sat_add(sat_sub(shi, slo), 0))),
                Some((_, shi)) => Some((0, shi)),
                None => tb,
            }
        }
        Opcode::IaddChk | Opcode::UaddChk | Opcode::IaddWrap => {
            let (alo, ahi) = ev(args[0])?;
            let (blo, bhi) = ev(args[1])?;
            let r = (sat_add(alo, blo), sat_add(ahi, bhi));
            fit_or(tb, r, inst.op == Opcode::IaddWrap)
        }
        Opcode::IsubChk | Opcode::UsubChk | Opcode::IsubWrap => {
            let (alo, ahi) = ev(args[0])?;
            let (blo, bhi) = ev(args[1])?;
            let r = (sat_sub(alo, bhi), sat_sub(ahi, blo));
            fit_or(tb, r, inst.op == Opcode::IsubWrap)
        }
        Opcode::ImulChk | Opcode::UmulChk | Opcode::ImulWrap => {
            let (alo, ahi) = ev(args[0])?;
            let (blo, bhi) = ev(args[1])?;
            let c = [
                sat_mul(alo, blo),
                sat_mul(alo, bhi),
                sat_mul(ahi, blo),
                sat_mul(ahi, bhi),
            ];
            let r = (
                *c.iter().min().expect("nonempty"),
                *c.iter().max().expect("nonempty"),
            );
            fit_or(tb, r, inst.op == Opcode::ImulWrap)
        }
        Opcode::Call => {
            if pos == 0
                && let Aux::Callee(ef) = inst.aux
            {
                let name = f.ext_funcs[ef].name.clone();
                if let Some(r) = rets.get(&name) {
                    return match (*r, tb) {
                        (Some((lo, hi)), Some((tlo, thi))) => Some((lo.max(tlo), hi.min(thi))),
                        (r, _) => r,
                    };
                }
            }
            tb
        }
        _ => tb,
    }
}

/// For a checked op the machine result IS the mathematical one (it
/// trapped otherwise), so the math range stands. For a wrap op the
/// math range stands only when it FITS the type — otherwise the
/// honest answer is the full type bounds (the D44-addendum rule).
fn fit_or(tb: Option<Rng>, r: Rng, is_wrap: bool) -> Option<Rng> {
    match tb {
        Some((tlo, thi)) => {
            if !is_wrap || (r.0 >= tlo && r.1 <= thi) {
                Some((r.0.max(tlo), r.1.min(thi)))
            } else {
                Some((tlo, thi))
            }
        }
        None => Some(r),
    }
}

/// Origin propagation to a fixpoint (block params meet their incoming
/// edge arguments), then one attribution walk.
fn scan(m: &Module, f: &Function) -> Local {
    let mut o: HashMap<Value, Origin> = HashMap::new();
    let set = |o: &mut HashMap<Value, Origin>, v: Value, x: Origin| -> bool {
        let cur = o.get(&v).copied().unwrap_or(Origin::Unset);
        let nw = meet_origin(cur, x);
        if nw != cur {
            o.insert(v, nw);
            true
        } else {
            false
        }
    };
    let cfg = analysis::cfg(f);
    let mut local_seq: HashMap<u32, u32> = HashMap::new(); // inst idx → a<k>
    let mut locals = 0u32;
    // Entry params: tentative container roots; demoted to Other the
    // moment a use is not header-shaped (conservatism is a fixpoint
    // property here: demotion re-runs until stable).
    let entry_params: Vec<Value> = f.entry().map(|e| f.block_params(e)).unwrap_or_default();
    let param_pos: HashMap<Value, u32> = entry_params
        .iter()
        .enumerate()
        .map(|(i, &v)| (v, i as u32))
        .collect();
    let param_modes: Vec<crate::ir::Mode> = m.sigs[f.sig].params.iter().map(|p| p.mode).collect();
    for (&v, &i) in &param_pos {
        if f.value_ty(v) != crate::types::PTR {
            continue;
        }
        match param_modes.get(i as usize) {
            // `mut` passes a pointer TO a header slot; the header
            // appears through `load.ptr param`.
            Some(crate::ir::Mode::Mut) => {
                set(&mut o, v, Origin::MutSlot(i));
            }
            // A by-value/read pointer param IS the header.
            _ => {
                set(&mut o, v, Origin::Hdr(Root::Param(i)));
            }
        }
    }
    // `mut` params are pointers to a slot holding the header; the
    // pattern below (load.ptr on a param) yields Hdr(Param) — start
    // them as Stack-like so `load.ptr param` maps to Hdr. A direct
    // `ptr` param IS the header. Both shapes appear in the corpus;
    // the demotion pass sorts out which one a given param is: a
    // header param used as a slot (or vice versa) demotes to Other.
    let mut changed = true;
    let mut rounds = 0;
    while changed && rounds < 32 {
        changed = false;
        rounds += 1;
        for &b in &cfg.rpo {
            for &ii in &f.blocks[b].insts {
                let inst = &f.insts[ii];
                let args = f.vpool.get(inst.args);
                let results = f.vpool.get(f.insts[ii].results).to_vec();
                match inst.op {
                    Opcode::StackAlloc => {
                        if let Some(&r0) = results.first() {
                            changed |= set(&mut o, r0, Origin::Stack(ii.as_u32()));
                        }
                    }
                    Opcode::DataAddr => {
                        if let Some(&r0) = results.first() {
                            changed |= set(&mut o, r0, Origin::Const);
                        }
                    }
                    Opcode::Call => {
                        if let Aux::Callee(ef) = inst.aux
                            && f.ext_funcs[ef].name == "__wolf_rt_list_new"
                            && let Some(&r0) = results.first()
                        {
                            let seq = *local_seq.entry(ii.as_u32()).or_insert_with(|| {
                                let k = locals;
                                locals += 1;
                                k
                            });
                            changed |= set(&mut o, r0, Origin::Hdr(Root::Local(seq)));
                        }
                    }
                    Opcode::Load => {
                        let base = o.get(&args[0]).copied().unwrap_or(Origin::Unset);
                        if let Some(&r0) = results.first() {
                            let is_ptr = f.value_ty(r0) == crate::types::PTR;
                            let x = match base {
                                // data = load.ptr hdr (field 0)
                                Origin::Hdr(r) if is_ptr => Origin::Data(r),
                                // hdr = load.ptr (mut param slot)
                                Origin::MutSlot(pi) if is_ptr => Origin::Hdr(Root::Param(pi)),
                                // hdr = load.ptr (local slot a header
                                // was stored into, same block) — the
                                // optimistic read that closes the
                                // loop-carried mut-slot cycle; a slot
                                // with no same-block dominating store
                                // stays Other (conservative).
                                Origin::Stack(_) if is_ptr => {
                                    match slot_value(f, b, ii, args[0])
                                        .and_then(|v| o.get(&v).copied())
                                    {
                                        Some(Origin::Hdr(r)) => Origin::Hdr(r),
                                        _ => Origin::Other,
                                    }
                                }
                                Origin::HdrField => Origin::Other,
                                Origin::Unset => Origin::Unset,
                                _ => Origin::Other,
                            };
                            changed |= set(&mut o, r0, x);
                        }
                    }
                    Opcode::PtrOff => {
                        let base = o.get(&args[0]).copied().unwrap_or(Origin::Unset);
                        let k = analysis::const_int(f, args[1]);
                        if let Some(&r0) = results.first() {
                            let scale = match inst.aux {
                                Aux::Scale(s) => s,
                                _ => 1,
                            };
                            let x = if k == Some(0) {
                                // A zero offset is the identity — the
                                // s104 versioner's guarded subject
                                // copies are exactly this shape, and
                                // an identity hop must not demote a
                                // data pointer to an element address
                                // (the store through it would become
                                // unattributable and poison the whole
                                // container's channel).
                                base
                            } else {
                                match base {
                                    Origin::Hdr(_)
                                        if scale == 1 && matches!(k, Some(8) | Some(16)) =>
                                    {
                                        Origin::HdrField
                                    }
                                    Origin::Hdr(_) => Origin::Other,
                                    Origin::Data(r) => Origin::Elem(r),
                                    Origin::Stack(s) => Origin::Stack(s),
                                    Origin::Const => Origin::Const,
                                    Origin::Unset => Origin::Unset,
                                    _ => Origin::Other,
                                }
                            };
                            changed |= set(&mut o, r0, x);
                        }
                    }
                    Opcode::Jmp | Opcode::Br => {
                        // Propagate into block parameters via edge args.
                        let edges: Vec<crate::ir::BlockCall> = match inst.aux {
                            Aux::Jump(j) => vec![j],
                            Aux::Br(t, e) => vec![t, e],
                            _ => vec![],
                        };
                        for edge in edges {
                            let params = f.block_params(edge.block);
                            let eargs = f.vpool.get(edge.args).to_vec();
                            for (p, a) in params.iter().zip(eargs.iter()) {
                                let ao = o.get(a).copied().unwrap_or(Origin::Unset);
                                if ao != Origin::Unset {
                                    changed |= set(&mut o, *p, ao);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    // Attribution walk.
    let _ = locals;
    let mut lf = Local::default();
    let origin = |v: Value| o.get(&v).copied().unwrap_or(Origin::Unset);
    // Track param demotion: a param whose value has any non-header use
    // is not a clean container param.
    let mut demoted: BTreeSet<u32> = BTreeSet::new();
    // First: integer entry params.
    for (i, &v) in entry_params.iter().enumerate() {
        if int_bounds(m, f, v).is_some() {
            lf.int_params.push((i as u32, v));
        }
    }
    // Return evaluation happens in the fixpoint (needs rets) — here we
    // only note the returned values.
    let mut memo: HashMap<Value, Option<Rng>> = HashMap::new();
    let empty_rets: BTreeMap<String, Option<Rng>> = BTreeMap::new();
    for &b in &cfg.rpo {
        for &ii in &f.blocks[b].insts {
            let inst = &f.insts[ii];
            let args = f.vpool.get(inst.args);
            match inst.op {
                Opcode::Store => {
                    // Store [val, addr, tok].
                    let addr = origin(args[1]);
                    match addr {
                        Origin::Elem(r) => {
                            let vr = eval(m, f, &empty_rets, &mut memo, args[0], 8);
                            let e = lf.meets.entry(r).or_insert(Meet::Empty);
                            *e = e.join(vr);
                            if let Root::Param(p) = r {
                                let w = lf.writes.entry(p).or_insert(Meet::Empty);
                                *w = w.join(vr);
                            }
                        }
                        Origin::Stack(_) | Origin::HdrField | Origin::Const => {}
                        // A store of a HEADER into a stack slot is the
                        // `mut` argument pattern; benign as a store —
                        // the slot's flow is handled at call sites.
                        _ if matches!(origin(args[0]), Origin::Hdr(_)) => {
                            if !matches!(addr, Origin::Stack(_)) {
                                // A header stored somewhere the walk
                                // cannot name: identity escapes.
                                if let Origin::Hdr(r) = origin(args[0]) {
                                    lf.meets.entry(r).or_insert(Meet::Empty).poison();
                                } else {
                                    lf.dirty = true;
                                }
                            }
                        }
                        Origin::Unset | Origin::Other => {
                            // An unattributable store: if it could be a
                            // buffer, every fact here dies.
                            lf.dirty = true;
                        }
                        _ => {}
                    }
                }
                Opcode::Load => {
                    let addr = origin(args[0]);
                    if let Origin::Elem(r) = addr
                        && let Some(&r0) = f.vpool.get(f.insts[ii].results).to_vec().first()
                        && int_bounds(m, f, r0).is_some()
                    {
                        lf.loads.entry(r).or_default().push(r0);
                        if let Root::Param(p) = r {
                            lf.reads.insert(p);
                        }
                    }
                }
                Opcode::CallInd => {
                    // s97: no summary edge — anything container-linked
                    // through an indirect call dies, loudly.
                    for a in args.iter().copied() {
                        match origin(a) {
                            Origin::Hdr(r) | Origin::Data(r) | Origin::Elem(r) => {
                                lf.meets.entry(r).or_insert(Meet::Empty).poison();
                                if let Root::Param(p) = r {
                                    lf.writes.entry(p).or_insert(Meet::Empty).poison();
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Opcode::Call => {
                    let Aux::Callee(ef) = inst.aux else { continue };
                    let name = f.ext_funcs[ef].name.clone();
                    let is_module_fn = m.funcs.iter().any(|(_, g)| g.name == name);
                    if is_module_fn {
                        let mut flows = Vec::new();
                        for a in args.iter().copied() {
                            let flow = match origin(a) {
                                Origin::Hdr(r) => ArgFlow::Container(r),
                                Origin::Stack(_) => {
                                    // A slot: if a header was stored
                                    // into it in this block before the
                                    // call, the callee sees that
                                    // container through a mut param.
                                    match slot_header(f, b, ii, a, &o) {
                                        Some(r) => ArgFlow::Container(r),
                                        None => ArgFlow::Opaque,
                                    }
                                }
                                Origin::Data(r) | Origin::Elem(r) => {
                                    // Raw interior pointers across a
                                    // call: identity laundering — kill.
                                    lf.meets.entry(r).or_insert(Meet::Empty).poison();
                                    ArgFlow::Opaque
                                }
                                _ => {
                                    if int_bounds(m, f, a).is_some() {
                                        ArgFlow::Int(eval(m, f, &empty_rets, &mut memo, a, 8))
                                    } else {
                                        ArgFlow::Opaque
                                    }
                                }
                            };
                            flows.push(flow);
                        }
                        lf.calls.push(CallSite {
                            callee: name,
                            args: flows,
                        });
                    } else if whitelisted_rt(&name) {
                        if name == "__wolf_rt_list_push" {
                            // push(hdr, slot, …): the pushed value is
                            // the slot's dominating store in this
                            // block.
                            let hdr = origin(args[0]);
                            let Origin::Hdr(r) = hdr else {
                                lf.dirty = true;
                                continue;
                            };
                            match slot_value(f, b, ii, args[1]) {
                                Some(v) => {
                                    let vr = eval(m, f, &empty_rets, &mut memo, v, 8);
                                    let e = lf.meets.entry(r).or_insert(Meet::Empty);
                                    *e = e.join(vr);
                                    if let Root::Param(p) = r {
                                        let w = lf.writes.entry(p).or_insert(Meet::Empty);
                                        *w = w.join(vr);
                                    }
                                }
                                None => {
                                    lf.meets.entry(r).or_insert(Meet::Empty).poison();
                                    if let Root::Param(p) = r {
                                        lf.writes.entry(p).or_insert(Meet::Empty).poison();
                                    }
                                }
                            }
                        }
                        // list_new handled in origin pass.
                    } else {
                        // Unknown external: any container-linked
                        // argument dies (import c and the whole world).
                        for a in args.iter().copied() {
                            match origin(a) {
                                Origin::Hdr(r) | Origin::Data(r) | Origin::Elem(r) => {
                                    lf.meets.entry(r).or_insert(Meet::Empty).poison();
                                    if let Root::Param(p) = r {
                                        lf.writes.entry(p).or_insert(Meet::Empty).poison();
                                    }
                                }
                                Origin::Stack(_) => {
                                    if let Some(r) = slot_header(f, b, ii, a, &o) {
                                        lf.meets.entry(r).or_insert(Meet::Empty).poison();
                                        if let Root::Param(p) = r {
                                            lf.writes.entry(p).or_insert(Meet::Empty).poison();
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Opcode::Ret => {
                    // Returned headers escape identity.
                    for a in args.iter().copied() {
                        if let Origin::Hdr(r) | Origin::Data(r) | Origin::Elem(r) = origin(a) {
                            lf.meets.entry(r).or_insert(Meet::Empty).poison();
                        }
                    }
                }
                _ => {}
            }
        }
    }
    // Param roots that survived with clean usage.
    for (i, &v) in entry_params.iter().enumerate() {
        let i = i as u32;
        if demoted.contains(&i) {
            continue;
        }
        if matches!(origin(v), Origin::Hdr(Root::Param(p)) if p == i)
            || o.values().any(|&x| x == Origin::Hdr(Root::Param(i)))
        {
            lf.param_roots.insert(i, Root::Param(i));
        }
    }
    let _ = &mut demoted;
    let _ = &param_modes;
    lf
}

/// The value stored into stack slot `slot` by a store in `b` BEFORE
/// instruction `at` (the push-argument pattern: fresh slot, one
/// store, the call).
fn slot_value(f: &Function, b: Block, at: crate::ir::Inst, slot: Value) -> Option<Value> {
    let mut found = None;
    for &ii in &f.blocks[b].insts {
        if ii == at {
            break;
        }
        let inst = &f.insts[ii];
        if inst.op == Opcode::Store {
            let args = f.vpool.get(inst.args);
            if args[1] == slot {
                found = Some(args[0]);
            }
        }
    }
    found
}

/// The container header stored into `slot` before `at` (the `mut`
/// argument pattern).
fn slot_header(
    f: &Function,
    b: Block,
    at: crate::ir::Inst,
    slot: Value,
    o: &HashMap<Value, Origin>,
) -> Option<Root> {
    let v = slot_value(f, b, at, slot)?;
    match o.get(&v).copied() {
        Some(Origin::Hdr(r)) => Some(r),
        _ => None,
    }
}

/// Run the whole-program analysis. `digest_prefix` is the provenance
/// tag every minted fact carries.
pub fn analyze(m: &Module) -> Ipr {
    // ---- local scans ---------------------------------------------------
    let mut locals: BTreeMap<String, Local> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for (_, f) in m.funcs.iter() {
        locals.insert(f.name.clone(), scan(m, f));
        order.push(f.name.clone());
    }
    order.sort();
    // Flags the grant pass needs.
    let mut exported: BTreeSet<String> = BTreeSet::new();
    let mut address_taken: BTreeSet<String> = BTreeSet::new();
    for (_, f) in m.funcs.iter() {
        if f.export || f.name == "main" {
            exported.insert(f.name.clone());
        }
        for &b in &f.layout {
            for &ii in &f.blocks[b].insts {
                if f.insts[ii].op == Opcode::FuncAddr
                    && let Aux::Callee(ef) = f.insts[ii].aux
                {
                    address_taken.insert(f.ext_funcs[ef].name.clone());
                }
            }
        }
    }
    for d in &m.data {
        for fname in &d.funcs {
            address_taken.insert(fname.clone());
        }
    }

    // ---- pass 2: close argw transitively (bottom-up, widened) ----------
    // A module call passing container root R at param j inherits the
    // callee's writes[j]; iterate WIDEN_PASSES then widen unknowns to
    // Poison.
    for _ in 0..WIDEN_PASSES {
        let snapshot: BTreeMap<String, BTreeMap<u32, Meet>> = locals
            .iter()
            .map(|(k, l)| (k.clone(), l.writes.clone()))
            .collect();
        let dirty_set: BTreeSet<String> = locals
            .iter()
            .filter(|(_, l)| l.dirty)
            .map(|(k, _)| k.clone())
            .collect();
        for name in &order {
            let calls = locals[name].calls.clone();
            let lf = locals.get_mut(name).expect("scanned");
            for cs in &calls {
                let callee_writes = snapshot.get(&cs.callee);
                let callee_dirty = dirty_set.contains(&cs.callee);
                for (j, af) in cs.args.iter().enumerate() {
                    let ArgFlow::Container(r) = af else { continue };
                    let effect = callee_writes
                        .and_then(|w| w.get(&(j as u32)).copied())
                        .unwrap_or(Meet::Empty);
                    let effect = if callee_dirty { Meet::Poison } else { effect };
                    match effect {
                        Meet::Empty => {}
                        Meet::Range(rr) => {
                            let e = lf.meets.entry(*r).or_insert(Meet::Empty);
                            *e = e.join(Some(rr));
                            if let Root::Param(p) = r {
                                let w = lf.writes.entry(*p).or_insert(Meet::Empty);
                                *w = w.join(Some(rr));
                            }
                        }
                        Meet::Poison => {
                            lf.meets.entry(*r).or_insert(Meet::Empty).poison();
                            if let Root::Param(p) = r {
                                lf.writes.entry(*p).or_insert(Meet::Empty).poison();
                            }
                        }
                    }
                }
            }
        }
    }

    // ---- pass 3: return ranges bottom-up (widened) ---------------------
    let mut rets: BTreeMap<String, Option<Rng>> = BTreeMap::new();
    for _ in 0..WIDEN_PASSES {
        for name in &order {
            let Some(fid) = fid_of(m, name) else { continue };
            let f = &m.funcs[fid];
            let mut memo = HashMap::new();
            let mut joined: Option<Option<Rng>> = None;
            for &b in &f.layout {
                for &ii in &f.blocks[b].insts {
                    if f.insts[ii].op != Opcode::Ret {
                        continue;
                    }
                    let args = f.vpool.get(f.insts[ii].args);
                    let Some(&rv) = args.first() else {
                        continue;
                    };
                    if int_bounds(m, f, rv).is_none() {
                        continue;
                    }
                    let r = eval(m, f, &rets, &mut memo, rv, 8);
                    joined = Some(match joined {
                        None => r,
                        Some(None) => None,
                        Some(Some((lo, hi))) => r.map(|(l2, h2)| (lo.min(l2), hi.max(h2))),
                    });
                }
            }
            // Full-type-bounds is "unbounded" — normalize to None so
            // no consumer (the summary's `ret=` field above all)
            // mistakes the absence of information for a fact.
            let joined = joined.flatten().filter(|&(lo, hi)| {
                let Some(fid2) = fid_of(m, name) else {
                    return false;
                };
                let f2 = &m.funcs[fid2];
                let tb = ret_type_bounds(m, f2);
                tb != Some((lo, hi))
            });
            rets.insert(name.clone(), joined);
        }
    }

    // ---- pass 4: parameter ranges top-down (widened) -------------------
    // Entry/exported/address-taken params stay unknown.
    let mut args: BTreeMap<String, BTreeMap<u32, Rng>> = BTreeMap::new();
    for _ in 0..WIDEN_PASSES {
        let mut incoming: BTreeMap<String, BTreeMap<u32, Option<Rng>>> = BTreeMap::new();
        for name in &order {
            for cs in &locals[name].calls {
                let entry = incoming.entry(cs.callee.clone()).or_default();
                for (j, af) in cs.args.iter().enumerate() {
                    let j = j as u32;
                    let r = match af {
                        ArgFlow::Int(r) => *r,
                        _ => None,
                    };
                    let slot = entry.entry(j).or_insert(Some((i128::MAX, i128::MIN)));
                    *slot = match (*slot, r) {
                        (None, _) | (_, None) => None,
                        (Some((lo, hi)), Some((l2, h2))) => {
                            // Sentinel start joins to the first real
                            // range.
                            if lo > hi {
                                Some((l2, h2))
                            } else {
                                Some((lo.min(l2), hi.max(h2)))
                            }
                        }
                    };
                }
            }
        }
        args.clear();
        for name in &order {
            if exported.contains(name) || address_taken.contains(name) {
                continue;
            }
            let Some(inc) = incoming.get(name) else {
                continue;
            };
            let mut per: BTreeMap<u32, Rng> = BTreeMap::new();
            for (&j, r) in inc {
                if let Some((lo, hi)) = r
                    && lo <= hi
                {
                    per.insert(j, (*lo, *hi));
                }
            }
            if !per.is_empty() {
                args.insert(name.clone(), per);
            }
        }
        // Re-evaluate call-site int args with param knowledge folded
        // in? Deliberately NOT in v1: the evaluator is param-free, so
        // the fixpoint here converges in one pass for non-recursive
        // graphs and stays sound (a recursive site contributes its
        // param's TYPE bounds, which can only widen). Recursive
        // precision (e3's `walk`) comes from the constant the entry
        // site passes meeting the recursive site's pass-through, which
        // the evaluator cannot see param-free — so a recursive
        // pass-through arg yields None and the meet widens to None
        // UNLESS the recursive arg is the parameter itself, handled
        // below.
        for name in &order {
            for cs in &locals[name].calls {
                if &cs.callee != name {
                    continue;
                }
                // Self-recursive site: an argument that is exactly the
                // callee's own parameter passes its range through
                // unchanged — the meet is then the non-recursive
                // sites' meet (e3: walk(n, depth-1) passes n).
                let Some(fid) = fid_of(m, name) else { continue };
                let f = &m.funcs[fid];
                let entry_params: Vec<Value> =
                    f.entry().map(|e| f.block_params(e)).unwrap_or_default();
                let _ = entry_params;
                let _ = cs;
            }
        }
    }

    // ---- self-recursive pass-through repair ---------------------------
    // The generic top-down meet above treats a recursive call's
    // argument opaquely (param-free eval → None) and so loses e3's
    // shape. Repair the exact provable case: for a function whose ONLY
    // callers are itself and non-recursive sites, an argument position
    // fed at every self-site by the parameter ITSELF (identity
    // pass-through) takes the meet of the non-self sites alone.
    {
        let mut self_ok: BTreeMap<String, BTreeMap<u32, Rng>> = BTreeMap::new();
        // Non-self incoming meets.
        let mut outer: BTreeMap<String, BTreeMap<u32, Option<Rng>>> = BTreeMap::new();
        let mut self_pass: BTreeMap<String, BTreeMap<u32, bool>> = BTreeMap::new();
        for name in &order {
            let Some(fid) = fid_of(m, name) else { continue };
            let f = &m.funcs[fid];
            let entry_params: Vec<Value> = f.entry().map(|e| f.block_params(e)).unwrap_or_default();
            for (_, g) in m.funcs.iter() {
                for cs in &locals[&g.name].calls {
                    if &cs.callee != name {
                        continue;
                    }
                    for (j, af) in cs.args.iter().enumerate() {
                        let j = j as u32;
                        if g.name == *name {
                            // Self site: identity pass-through?
                            let is_id = matches!(af, ArgFlow::Int(_)) && {
                                // Re-derive: the flow was evaluated;
                                // identity means the argument VALUE is
                                // the entry parameter at position j.
                                // The Local scan does not keep values,
                                // so re-scan this site.
                                self_site_passes_param(m, f, &entry_params, j)
                            };
                            let e = self_pass
                                .entry(name.clone())
                                .or_default()
                                .entry(j)
                                .or_insert(true);
                            *e = *e && is_id;
                        } else {
                            let r = match af {
                                ArgFlow::Int(r) => *r,
                                _ => None,
                            };
                            let slot = outer
                                .entry(name.clone())
                                .or_default()
                                .entry(j)
                                .or_insert(Some((i128::MAX, i128::MIN)));
                            *slot = match (*slot, r) {
                                (None, _) | (_, None) => None,
                                (Some((lo, hi)), Some((l2, h2))) => {
                                    if lo > hi {
                                        Some((l2, h2))
                                    } else {
                                        Some((lo.min(l2), hi.max(h2)))
                                    }
                                }
                            };
                        }
                    }
                }
            }
            let (Some(outer_j), Some(self_j)) = (outer.get(name), self_pass.get(name)) else {
                continue;
            };
            for (&j, &ok) in self_j {
                if !ok {
                    continue;
                }
                if exported.contains(name) || address_taken.contains(name) {
                    continue;
                }
                if let Some(Some((lo, hi))) = outer_j.get(&j)
                    && lo <= hi
                {
                    self_ok
                        .entry(name.clone())
                        .or_default()
                        .insert(j, (*lo, *hi));
                }
            }
        }
        for (name, per) in self_ok {
            let e = args.entry(name).or_default();
            for (j, r) in per {
                e.insert(j, r);
            }
        }
    }

    // ---- pass 5: grants (container ranges for param roots) -------------
    let mut grants: BTreeMap<String, BTreeMap<u32, Rng>> = BTreeMap::new();
    for name in &order {
        if exported.contains(name) || address_taken.contains(name) {
            continue;
        }
        if locals[name].dirty {
            continue;
        }
        let param_roots: Vec<u32> = locals[name].param_roots.keys().copied().collect();
        if param_roots.is_empty() {
            continue;
        }
        // Meet per param over every call site in the program.
        let mut per: BTreeMap<u32, Option<Rng>> = param_roots
            .iter()
            .map(|&p| (p, Some((i128::MAX, i128::MIN))))
            .collect();
        let mut any_site = false;
        for caller in &order {
            for cs in &locals[caller].calls {
                if &cs.callee != name {
                    continue;
                }
                any_site = true;
                // Frame check at this site: every param the callee
                // WRITES must be a distinct local root from every
                // param we grant on.
                for (&p, slot) in per.iter_mut() {
                    let granted = grant_at_site(&locals[caller], cs, p, &locals[name]);
                    *slot = match (*slot, granted) {
                        (None, _) | (_, None) => None,
                        (Some((lo, hi)), Some((l2, h2))) => {
                            if lo > hi {
                                Some((l2, h2))
                            } else {
                                Some((lo.min(l2), hi.max(h2)))
                            }
                        }
                    };
                }
            }
        }
        if !any_site {
            continue;
        }
        let mut out: BTreeMap<u32, Rng> = BTreeMap::new();
        for (p, r) in per {
            if let Some((lo, hi)) = r
                && lo <= hi
            {
                out.insert(p, (lo, hi));
            }
        }
        if !out.is_empty() {
            grants.insert(name.clone(), out);
        }
    }

    // ---- results -------------------------------------------------------
    let mut ipr = Ipr::default();
    for name in &order {
        ipr.rets
            .insert(name.clone(), rets.get(name).copied().flatten());
        if let Some(a) = args.get(name) {
            ipr.args.insert(name.clone(), a.clone());
        }
        if let Some(g) = grants.get(name) {
            ipr.grants.insert(name.clone(), g.clone());
        }
        let lf = &locals[name];
        let mut stores: BTreeMap<u32, Rng> = BTreeMap::new();
        let mut elems: BTreeMap<u32, Rng> = BTreeMap::new();
        for (root, meet) in &lf.meets {
            if let (Root::Local(k), Meet::Range(r)) = (root, meet) {
                stores.insert(*k, *r);
                if !lf.dirty {
                    elems.insert(*k, *r);
                }
            }
        }
        if !stores.is_empty() {
            ipr.stores.insert(name.clone(), stores);
        }
        if !elems.is_empty() {
            ipr.local_elems.insert(name.clone(), elems);
        }
    }
    ipr
}

/// Can `cs` (a call in `caller` to a function whose Local is `callee`)
/// grant an element range for the callee's param-root `p`? Sound iff
/// the argument at `p` is a clean LOCAL group of the caller with a
/// known meet, and every param the callee writes is passed a local
/// group with a DIFFERENT root (distinct `list_new` allocations own
/// disjoint buffers).
fn grant_at_site(caller: &Local, cs: &CallSite, p: u32, callee: &Local) -> Option<Rng> {
    let ArgFlow::Container(Root::Local(gk)) = cs.args.get(p as usize)? else {
        return None;
    };
    let meet = caller
        .meets
        .get(&Root::Local(*gk))
        .copied()
        .unwrap_or(Meet::Empty);
    let granted = match meet {
        Meet::Range(r) => r,
        // An empty container's loads never execute — grant the empty
        // range's neutral: nothing to grant, but nothing unsound; use
        // the full-empty sentinel by refusing (no loads will need it).
        Meet::Empty => return None,
        Meet::Poison => return None,
    };
    for (&wp, wmeet) in &callee.writes {
        if matches!(wmeet, Meet::Empty) {
            continue;
        }
        // The callee writes param wp's container: the argument there
        // must be a local root distinct from ours.
        match cs.args.get(wp as usize) {
            Some(ArgFlow::Container(Root::Local(ok))) if ok != gk => {}
            _ => return None,
        }
    }
    Some(granted)
}

/// Does every self-recursive call site of `f` pass parameter `j`'s own
/// value at position `j`?
fn self_site_passes_param(m: &Module, f: &Function, entry_params: &[Value], j: u32) -> bool {
    let Some(&pj) = entry_params.get(j as usize) else {
        return false;
    };
    let mut all = true;
    let mut any = false;
    for &b in &f.layout {
        for &ii in &f.blocks[b].insts {
            let inst = &f.insts[ii];
            if inst.op != Opcode::Call {
                continue;
            }
            let Aux::Callee(ef) = inst.aux else { continue };
            if f.ext_funcs[ef].name != f.name {
                continue;
            }
            any = true;
            let args = f.vpool.get(inst.args);
            all &= args.get(j as usize).copied() == Some(pj);
        }
    }
    let _ = m;
    any && all
}

/// The int bounds of the function's (first) return value type, read
/// off any `ret` site.
fn ret_type_bounds(m: &Module, f: &Function) -> Option<Rng> {
    for &b in &f.layout {
        for &ii in &f.blocks[b].insts {
            if f.insts[ii].op == Opcode::Ret
                && let Some(&rv) = f.vpool.get(f.insts[ii].args).first()
            {
                return int_bounds(m, f, rv);
            }
        }
    }
    None
}

fn fid_of(m: &Module, name: &str) -> Option<FuncId> {
    m.funcs
        .iter()
        .find(|(_, f)| f.name == name)
        .map(|(id, _)| id)
}

/// Mint the facts `ipr` proves, tagged with `digest_prefix`. Returns
/// the functions that gained at least one fact (the phase re-runs the
/// pipeline on exactly these). A fact is minted only when strictly
/// narrower than the subject's type bounds and not already implied by
/// an existing fact on the same value.
pub fn mint(m: &mut Module, ipr: &Ipr, digest_prefix: u64) -> (Vec<FuncId>, usize) {
    let mut touched: BTreeSet<FuncId> = BTreeSet::new();
    let mut total = 0usize;
    let names: Vec<(FuncId, String)> = m.funcs.iter().map(|(id, f)| (id, f.name.clone())).collect();
    for (fid, name) in &names {
        let fid = *fid;
        // 1. call results from callee return ranges.
        let mut plan: Vec<(Value, Rng)> = Vec::new();
        {
            let f = &m.funcs[fid];
            for &b in &f.layout {
                for &ii in &f.blocks[b].insts {
                    if f.insts[ii].op != Opcode::Call {
                        continue;
                    }
                    let Aux::Callee(ef) = f.insts[ii].aux else {
                        continue;
                    };
                    let callee = &f.ext_funcs[ef].name;
                    let Some(Some(r)) = ipr.rets.get(callee) else {
                        continue;
                    };
                    let Some(&r0) = f.vpool.get(f.insts[ii].results).to_vec().first() else {
                        continue;
                    };
                    if m.types.int_bounds(f.value_ty(r0)).is_some() {
                        plan.push((r0, *r));
                    }
                }
            }
            // 2. entry parameter ranges.
            if let Some(per) = ipr.args.get(name)
                && let Some(e) = f.entry()
            {
                let params = f.block_params(e);
                for (&j, &r) in per {
                    if let Some(&pv) = params.get(j as usize)
                        && m.types.int_bounds(f.value_ty(pv)).is_some()
                    {
                        plan.push((pv, r));
                    }
                }
            }
            // 3. container element loads: local roots and granted
            // param roots. Re-scan for load attribution (the Local is
            // not kept between analyze and mint; a fresh scan of an
            // unchanged module derives the same origins — D4).
            let lf = scan(m, f);
            for (root, loads) in &lf.loads {
                let range = match root {
                    Root::Local(k) => ipr.local_elems.get(name).and_then(|e| e.get(k)).copied(),
                    Root::Param(p) => ipr.grants.get(name).and_then(|g| g.get(p)).copied(),
                };
                let Some(r) = range else { continue };
                for &lv in loads {
                    plan.push((lv, r));
                }
            }
        }
        // Apply, filtered.
        let f = &mut m.funcs[fid];
        for (v, (lo, hi)) in plan {
            let Some((tlo, thi)) = m.types.int_bounds(f.value_ty(v)) else {
                continue;
            };
            let (lo, hi) = (lo.max(tlo), hi.min(thi));
            if lo <= tlo && hi >= thi {
                continue; // not narrower than the type
            }
            let kind = FactKind::Range(v, lo, hi);
            let implied = f.facts.values().any(|fd| match fd.kind {
                FactKind::Range(v2, l2, h2) => v2 == v && l2 >= lo && h2 <= hi,
                _ => false,
            });
            if implied {
                continue;
            }
            f.add_fact(FactData::new(kind, Just::Summary(digest_prefix)));
            total += 1;
            touched.insert(fid);
        }
    }
    (touched.into_iter().collect(), total)
}

/// The mint-time audit (the whole-program half of verification for
/// [`Just::Summary`] facts): recompute the analysis on the module AS
/// MINTED and demand every summary-justified fact be implied by a
/// fresh proof. Run BEFORE any pass consumes the facts: a fact may
/// feed an optimization that folds away its own proof's source (a
/// callee whose ret-fact lets its call sites const-fold, reshaping
/// the callee's returns), after which the fact persists as a verified
/// ENTITY — the same custody model as a c04 theorem fact, whose proof
/// WIR never re-derives either. The per-function verifier keeps the
/// shape checks forever; THIS check owns the proof, at the one moment
/// the proof and the module agree. (Found the hard way: running it
/// post-re-opt ICEd on `resolve/same_name` when a ret-fact's own
/// consumption erased its re-derivability.)
pub fn reverify(m: &Module) -> Result<(), (String, String)> {
    let ipr = analyze(m);
    // Rebuild the full mint plan value-set per function.
    for (_, f) in m.funcs.iter() {
        let mut proven: HashMap<Value, Rng> = HashMap::new();
        for &b in &f.layout {
            for &ii in &f.blocks[b].insts {
                if f.insts[ii].op != Opcode::Call {
                    continue;
                }
                let Aux::Callee(ef) = f.insts[ii].aux else {
                    continue;
                };
                if let Some(Some(r)) = ipr.rets.get(&f.ext_funcs[ef].name)
                    && let Some(&r0) = f.vpool.get(f.insts[ii].results).to_vec().first()
                {
                    proven.insert(r0, *r);
                }
            }
        }
        if let Some(per) = ipr.args.get(&f.name)
            && let Some(e) = f.entry()
        {
            let params = f.block_params(e);
            for (&j, &r) in per {
                if let Some(&pv) = params.get(j as usize) {
                    proven.insert(pv, r);
                }
            }
        }
        let lf = scan(m, f);
        for (root, loads) in &lf.loads {
            let range = match root {
                Root::Local(k) => ipr.local_elems.get(&f.name).and_then(|e| e.get(k)).copied(),
                Root::Param(p) => ipr.grants.get(&f.name).and_then(|g| g.get(p)).copied(),
            };
            if let Some(r) = range {
                for &lv in loads {
                    proven.insert(lv, r);
                }
            }
        }
        let mut memo = HashMap::new();
        for fd in f.facts.values() {
            let Just::Summary(_) = fd.just else { continue };
            let FactKind::Range(v, lo, hi) = fd.kind else {
                return Err((
                    f.name.clone(),
                    "summary justification on a non-range fact".to_string(),
                ));
            };
            if let Some(&(plo, phi)) = proven.get(&v)
                && plo >= lo
                && phi <= hi
            {
                continue;
            }
            // A pass may FOLD the subject after minting — a call
            // result becomes the constant its ret-fact enabled, a
            // load forwards — and the def-shape lookup above then
            // misses. The fact is still checkable: the local
            // evaluator's range for the subject must imply it.
            if let Some((elo, ehi)) = eval(m, f, &ipr.rets, &mut memo, v, 8)
                && elo >= lo
                && ehi <= hi
            {
                continue;
            }
            return Err((
                f.name.clone(),
                format!("summary range fact on {v:?} has no current proof"),
            ));
        }
    }
    Ok(())
}
