//! Shared function analyses for the mid-end (s42): reachable-RPO CFG,
//! dominators, natural loops, constant lookup, and use rewriting.
//!
//! Deferred invariant maintenance (the egg discipline, reports/04
//! carry #7): none of these structures is repaired incrementally.
//! A pass computes what it needs at entry, mutates freely, and the
//! pass boundary rebuilds the function in batch (`compact`) — CFG,
//! value numbering, and layout come back canonical there, never
//! mid-pass.

use std::collections::{HashMap, HashSet};

use crate::ir::{Aux, Block, Function, Inst, Value, ValueDef};
use crate::ops::Opcode;
use crate::print::successors;
use crate::types::TypeInterner;

/// Reachable control flow: RPO order and predecessor lists.
pub(crate) struct Cfg {
    /// Reachable blocks in reverse postorder; entry first.
    pub rpo: Vec<Block>,
    pub preds: HashMap<Block, Vec<Block>>,
    pub reachable: HashSet<Block>,
}

pub(crate) fn cfg(f: &Function) -> Cfg {
    let mut post: Vec<Block> = Vec::new();
    let mut seen: HashSet<Block> = HashSet::new();
    if let Some(entry) = f.entry() {
        let mut stack: Vec<(Block, usize)> = vec![(entry, 0)];
        seen.insert(entry);
        while let Some((b, i)) = stack.last().copied() {
            let mut succs = successors(f, b);
            succs.reverse();
            if i < succs.len() {
                stack.last_mut().expect("nonempty").1 += 1;
                let s = succs[i];
                if f.blocks.contains(s) && seen.insert(s) {
                    stack.push((s, 0));
                }
            } else {
                post.push(b);
                stack.pop();
            }
        }
    }
    post.reverse();
    let mut preds: HashMap<Block, Vec<Block>> = HashMap::new();
    for &b in &post {
        preds.entry(b).or_default();
        for s in successors(f, b) {
            if seen.contains(&s) {
                preds.entry(s).or_default().push(b);
            }
        }
    }
    Cfg {
        rpo: post,
        preds,
        reachable: seen,
    }
}

/// Immediate dominators (Cooper–Harvey–Kennedy over RPO).
pub(crate) struct Doms {
    idom: HashMap<Block, Block>,
}

impl Doms {
    pub fn idom(&self, b: Block) -> Option<Block> {
        let d = *self.idom.get(&b)?;
        if d == b { None } else { Some(d) }
    }

    /// Does `a` dominate `b`? (Reflexive.)
    pub fn dominates(&self, a: Block, b: Block) -> bool {
        let mut cur = b;
        loop {
            if cur == a {
                return true;
            }
            match self.idom.get(&cur) {
                Some(&d) if d != cur => cur = d,
                _ => return false,
            }
        }
    }
}

pub(crate) fn dominators(cfg: &Cfg) -> Doms {
    let mut order: HashMap<Block, usize> = HashMap::new();
    for (i, &b) in cfg.rpo.iter().enumerate() {
        order.insert(b, i);
    }
    let mut idom: HashMap<Block, Block> = HashMap::new();
    let Some(&entry) = cfg.rpo.first() else {
        return Doms { idom };
    };
    idom.insert(entry, entry);
    let intersect = |idom: &HashMap<Block, Block>, mut a: Block, mut b: Block| -> Block {
        while a != b {
            while order[&a] > order[&b] {
                a = idom[&a];
            }
            while order[&b] > order[&a] {
                b = idom[&b];
            }
        }
        a
    };
    let mut changed = true;
    while changed {
        changed = false;
        for &b in cfg.rpo.iter().skip(1) {
            let mut new_idom: Option<Block> = None;
            for &p in &cfg.preds[&b] {
                if !idom.contains_key(&p) {
                    continue;
                }
                new_idom = Some(match new_idom {
                    None => p,
                    Some(cur) => intersect(&idom, cur, p),
                });
            }
            if let Some(ni) = new_idom
                && idom.get(&b) != Some(&ni)
            {
                idom.insert(b, ni);
                changed = true;
            }
        }
    }
    Doms { idom }
}

/// One natural loop: all back edges to one header, bodies merged.
pub(crate) struct NaturalLoop {
    pub header: Block,
    pub blocks: HashSet<Block>,
}

pub(crate) struct Loops {
    /// Innermost-first (post-order by containment is not guaranteed;
    /// consumers filter by block-set size when nesting matters).
    pub loops: Vec<NaturalLoop>,
    /// Loop nesting depth per block (0 = not in any loop).
    pub depth: HashMap<Block, u32>,
}

pub(crate) fn loops(f: &Function, cfg: &Cfg, doms: &Doms) -> Loops {
    let mut by_header: HashMap<Block, HashSet<Block>> = HashMap::new();
    for &b in &cfg.rpo {
        for s in successors(f, b) {
            if cfg.reachable.contains(&s) && doms.dominates(s, b) {
                // b -> s is a back edge; collect the natural loop.
                let body = by_header.entry(s).or_default();
                body.insert(s);
                let mut stack = vec![b];
                while let Some(x) = stack.pop() {
                    if body.insert(x) {
                        for &p in cfg.preds.get(&x).map(|v| v.as_slice()).unwrap_or(&[]) {
                            stack.push(p);
                        }
                    }
                }
            }
        }
    }
    let mut loops: Vec<NaturalLoop> = by_header
        .into_iter()
        .map(|(header, blocks)| NaturalLoop { header, blocks })
        .collect();
    // Deterministic order: by header id; innermost (smallest) first on ties.
    loops.sort_by_key(|l| (l.blocks.len(), l.header));
    let mut depth: HashMap<Block, u32> = HashMap::new();
    for l in &loops {
        for &b in &l.blocks {
            *depth.entry(b).or_insert(0) += 1;
        }
    }
    Loops { loops, depth }
}

// ------------------------------------------------------------ values ----

/// The instruction defining `v`, if it is an instruction result.
pub(crate) fn def_inst(f: &Function, v: Value) -> Option<Inst> {
    match f.values[v].def {
        ValueDef::Result(i, _) => Some(i),
        ValueDef::Param(..) => None,
    }
}

/// The `iconst` payload of `v`, if that is its definition.
pub(crate) fn const_int(f: &Function, v: Value) -> Option<i64> {
    let i = def_inst(f, v)?;
    match (f.insts[i].op, f.insts[i].aux) {
        (Opcode::Iconst, Aux::Int(n)) => Some(n),
        _ => None,
    }
}

/// The `bconst` payload of `v`, if that is its definition.
pub(crate) fn const_bool(f: &Function, v: Value) -> Option<bool> {
    let i = def_inst(f, v)?;
    match (f.insts[i].op, f.insts[i].aux) {
        (Opcode::Bconst, Aux::Bool(b)) => Some(b),
        _ => None,
    }
}

/// Resolve a replacement map transitively (a→b, b→c ⇒ a→c).
pub(crate) fn resolve(map: &HashMap<Value, Value>, mut v: Value) -> Value {
    let mut hops = 0;
    while let Some(&n) = map.get(&v) {
        v = n;
        hops += 1;
        debug_assert!(hops < 1_000_000, "replacement cycle");
    }
    v
}

/// Rewrite every use in the (reachable) layout through `map`:
/// instruction operands and branch-edge arguments. Facts are NOT
/// rewritten here — fact changes are pass decisions, recorded against
/// the pass context (D2).
pub(crate) fn replace_uses(f: &mut Function, map: &HashMap<Value, Value>) {
    if map.is_empty() {
        return;
    }
    let blocks: Vec<Block> = f.layout.clone();
    for b in blocks {
        let insts = f.blocks[b].insts.clone();
        for inst in insts {
            let args = f.insts[inst].args;
            for (i, v) in f.vpool.get(args).into_iter().enumerate() {
                let r = resolve(map, v);
                if r != v {
                    f.vpool.set(args, i, r);
                }
            }
            match f.insts[inst].aux {
                Aux::Jump(bc) => {
                    for (i, v) in f.vpool.get(bc.args).into_iter().enumerate() {
                        let r = resolve(map, v);
                        if r != v {
                            f.vpool.set(bc.args, i, r);
                        }
                    }
                }
                Aux::Br(t, e) => {
                    for bc in [t, e] {
                        for (i, v) in f.vpool.get(bc.args).into_iter().enumerate() {
                            let r = resolve(map, v);
                            if r != v {
                                f.vpool.set(bc.args, i, r);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

// ------------------------------------------------------------ op sets ----

/// Ops GVN may hash-cons: pure values, where two instructions with
/// identical operands and payload produce identical results — and, for
/// the checked family, identical trap behavior (deduplication keeps
/// the surviving check, so the verdict is unchanged; X3 holds).
pub(crate) fn is_gvn_pure(op: Opcode) -> bool {
    matches!(
        op,
        Opcode::Iconst
            | Opcode::Fconst
            | Opcode::Bconst
            | Opcode::IaddChk
            | Opcode::IsubChk
            | Opcode::ImulChk
            | Opcode::IdivChk
            | Opcode::IremChk
            | Opcode::IaddWrap
            | Opcode::IsubWrap
            | Opcode::ImulWrap
            | Opcode::IaddSat
            | Opcode::IsubSat
            | Opcode::ImulSat
            | Opcode::UaddChk
            | Opcode::UsubChk
            | Opcode::UmulChk
            | Opcode::UdivChk
            | Opcode::UremChk
            | Opcode::Band
            | Opcode::Bor
            | Opcode::Bxor
            | Opcode::Shl
            | Opcode::Lshr
            | Opcode::Ashr
            | Opcode::Fadd
            | Opcode::Fsub
            | Opcode::Fmul
            | Opcode::Fdiv
            | Opcode::Fneg
            | Opcode::Fma
            | Opcode::Icmp
            | Opcode::Fcmp
            | Opcode::Sext
            | Opcode::Zext
            | Opcode::Itrunc
            | Opcode::PtrOff
            | Opcode::AggMake
            | Opcode::AggGet
            | Opcode::DataAddr
            | Opcode::FuncAddr
            | Opcode::EuMakeOk
            | Opcode::EuMakeErr
            | Opcode::EuIsErr
            | Opcode::EuOk
            | Opcode::EuErr
    )
}

/// Ops DCE may remove when their results are unused: pure AND
/// trap-free. The checked family is deliberately ABSENT — removing a
/// dead `iadd.chk` would remove a potential trap, changing the
/// verdict (X3: traps are observable outcomes). Range-proven checks
/// are first rewritten to `*.wrap` by the range pass; the wrap form
/// then dies here.
pub(crate) fn is_removable(op: Opcode) -> bool {
    matches!(
        op,
        Opcode::Iconst
            | Opcode::Fconst
            | Opcode::Bconst
            | Opcode::IaddWrap
            | Opcode::IsubWrap
            | Opcode::ImulWrap
            | Opcode::IaddSat
            | Opcode::IsubSat
            | Opcode::ImulSat
            | Opcode::Band
            | Opcode::Bor
            | Opcode::Bxor
            | Opcode::Shl
            | Opcode::Lshr
            | Opcode::Ashr
            | Opcode::Fadd
            | Opcode::Fsub
            | Opcode::Fmul
            | Opcode::Fdiv
            | Opcode::Fneg
            | Opcode::Fma
            | Opcode::Icmp
            | Opcode::Fcmp
            | Opcode::Sext
            | Opcode::Zext
            | Opcode::Itrunc
            | Opcode::PtrOff
            | Opcode::AggMake
            | Opcode::AggGet
            | Opcode::DataAddr
            | Opcode::FuncAddr
            | Opcode::EuMakeOk
            | Opcode::EuMakeErr
            | Opcode::EuIsErr
            | Opcode::EuOk
            | Opcode::EuErr
            // A dead load is removable: safe-tier WIR loads cannot
            // trap (a live token is structural proof of validity),
            // and reading memory has no effect.
            | Opcode::Load
    )
}

/// Pointer provenance + escape for one region: the pointers minted by
/// its allocations (closed over `ptr.off`), and whether any of them
/// ESCAPES — is seen anywhere other than a load/store ADDRESS position
/// or a further `ptr.off` base. Escape is what makes a region
/// externally observable even without its token: runtime read shims
/// (`__wolf_rt_print_str`-class calls) take raw pointers with no token
/// operand, so a region whose pointer reached ANY call, store-value,
/// return, aggregate, or block argument must be treated as read (and,
/// for spawn seams, as crossing a schedule point — the conc
/// conservatism barrier).
pub(crate) struct RegionPtrs {
    pub escapes: bool,
}

pub(crate) fn region_pointers(f: &Function, types: &TypeInterner, region: u32) -> RegionPtrs {
    use crate::entity::EntityRef;
    use crate::types::TypeData;
    let mut ptrs: HashSet<Value> = HashSet::new();
    // Seed: allocation pointer results whose token result mints the
    // region; close over ptr.off in layout order (defs precede uses,
    // so one forward sweep suffices per SSA dominance).
    for &b in &f.layout {
        for &inst in &f.blocks[b].insts {
            let op = f.insts[inst].op;
            let results = f.vpool.get(f.insts[inst].results);
            match op {
                Opcode::RegionAlloc | Opcode::StackAlloc => {
                    if let Some(&tok) = results.get(1)
                        && let TypeData::Mem(r) = types.get(f.value_ty(tok))
                        && r.as_u32() == region
                    {
                        ptrs.insert(results[0]);
                    }
                }
                Opcode::PtrOff => {
                    let base = f.vpool.get(f.insts[inst].args)[0];
                    if ptrs.contains(&base) {
                        ptrs.insert(results[0]);
                    }
                }
                _ => {}
            }
        }
    }
    // Escape scan.
    let mut escapes = false;
    'scan: for &b in &f.layout {
        for &inst in &f.blocks[b].insts {
            let op = f.insts[inst].op;
            let args = f.vpool.get(f.insts[inst].args);
            let uses_ptr = |vs: &[Value]| vs.iter().any(|v| ptrs.contains(v));
            match op {
                Opcode::Load => {
                    // args = (addr, token): address position is safe.
                }
                Opcode::Store => {
                    // args = (value, addr, token): storing one of OUR
                    // pointers as the VALUE publishes the address.
                    if ptrs.contains(&args[0]) {
                        escapes = true;
                        break 'scan;
                    }
                }
                Opcode::PtrOff => {
                    // base propagation, handled in the seed sweep.
                }
                Opcode::Icmp => {
                    // Comparing addresses reveals nothing loadable.
                }
                _ => {
                    if uses_ptr(&args) {
                        escapes = true;
                        break 'scan;
                    }
                }
            }
            match f.insts[inst].aux {
                Aux::Jump(bc) => {
                    if f.vpool.get(bc.args).iter().any(|v| ptrs.contains(v)) {
                        escapes = true;
                        break 'scan;
                    }
                }
                Aux::Br(t, e) => {
                    for bc in [t, e] {
                        if f.vpool.get(bc.args).iter().any(|v| ptrs.contains(v)) {
                            escapes = true;
                            break 'scan;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    RegionPtrs { escapes }
}
