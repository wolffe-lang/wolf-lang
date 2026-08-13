//! `FuncBuilder` — Braun on-the-fly SSA construction with
//! peephole-at-construction (s25, D1).
//!
//! One forward walk builds verified WIR directly: no CFG-then-SSA
//! staging, no phase that materializes non-SSA IR, no post-construction
//! cleanup pass — if output looks dirty, the peephole rule set is
//! incomplete, not the pipeline (s25 non-target).
//!
//! - **Braun SSA** (ssa-book; Braun et al.): `def_var`/`use_var` over
//!   numbered variables; a use with no local def recurses into
//!   predecessors; an **unsealed** block (loop header mid-construction)
//!   gets a placeholder block parameter resolved at
//!   [`FuncBuilder::seal_block`]. The trivial-φ test runs verbatim on
//!   block-parameter argument lists, and removal cascades to dependent
//!   Braun-created parameters.
//! - **Semi-pruned filtering**: variables the caller marks single-block
//!   ([`FuncBuilder::mark_single_block`], from a cheap AST pre-scan)
//!   live in a per-block slot and bypass the global Braun maps.
//! - **The peephole gauntlet** (Click §5): every [`FuncBuilder::ins`]
//!   speculatively appends at an arena mark, then folds constants
//!   (checked ops fold only in range; a provable overflow folds to
//!   `trap` — X3 runtime semantics exactly, the interpreter oracle's
//!   contract), applies the data-driven identity table
//!   ([`crate::peephole_rules`]), and hash-conses pure ops through a
//!   scope-stacked GVN table. Any hit rolls the arena back to the mark
//!   (s24 LIFO delete): speculation is free — no tombstones, no id
//!   leaks.
//! - **Effect tokens as Braun variables**: the builder tracks one
//!   current `mem` token per region and one `io` token as ordinary
//!   variables, so token merges at joins become block parameters and
//!   loop-carried tokens need zero extra machinery. Store→load
//!   forwarding and redundant-load GVN work through the token chain,
//!   never an alias pass.
//!
//! Construction is deterministic: hash maps are lookup-only, every
//! iterated structure is a `Vec`, and two identical builder runs print
//! byte-identically (tested; the D8 hashing input).

use crate::entity::EntityRef;
use crate::ir::{Aux, Block, BlockCall, ExtFunc, Function, Inst, Value, ValueDef};
use crate::ops::{IntCc, Opcode, TrapKind};
use crate::peephole_rules::{self, Rewrite};
use crate::types::{RegionId, TypeData, TypeId, TypeInterner};
use std::collections::{HashMap, HashSet};

/// A Braun variable: the builder-side identity of one source-level
/// binding (or one effect-token chain). Minted by
/// [`FuncBuilder::declare_var`]; carries no name — callers keep the
/// mapping.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Var(u32);

/// Peephole hit counters (the Click claim, measured not assumed —
/// dumped under `--zstats` and the `wir-build` bench metric).
#[derive(Clone, Copy, Default, Debug)]
pub struct Stats {
    /// Instructions that survived the gauntlet (terminators included).
    pub insts: u64,
    /// All-constant folds (including checked-op folds to `trap`).
    pub fold: u64,
    /// Identity-table rewrites plus `br const → jmp` folds.
    pub identity: u64,
    /// GVN hash-consing hits.
    pub gvn: u64,
    /// Store→load forwards through the token chain.
    pub forward: u64,
}

impl Stats {
    pub fn add(&mut self, other: Stats) {
        self.insts += other.insts;
        self.fold += other.fold;
        self.identity += other.identity;
        self.gvn += other.gvn;
        self.forward += other.forward;
    }
}

/// What one [`FuncBuilder::ins`] produced.
#[derive(Clone, Debug)]
pub enum InsOut {
    /// The instruction's result values (possibly rewritten to existing
    /// values by the peephole; empty for result-less ops).
    Vals(Vec<Value>),
    /// The instruction provably traps (checked-arith overflow, divide
    /// by a constant zero): the builder emitted `trap` and the block
    /// is filled — the caller's control flow diverged.
    Trapped,
}

impl InsOut {
    /// The single result of a one-result op. Panics on `Trapped` or
    /// arity mismatch — use only where the op cannot fold to a trap.
    pub fn one(self) -> Value {
        match self {
            InsOut::Vals(v) if v.len() == 1 => v[0],
            other => panic!("expected exactly one result, got {other:?}"),
        }
    }

    /// The single result, or `None` when the op folded to a trap.
    pub fn value(self) -> Option<Value> {
        match self {
            InsOut::Vals(v) if v.len() == 1 => Some(v[0]),
            InsOut::Trapped => None,
            other => panic!("expected one result, got {other:?}"),
        }
    }
}

/// GVN key: opcode + result type + canonicalized operands + the
/// non-edge aux payload. Only pure ops are ever keyed; a load's token
/// rides as an ordinary operand, which makes redundant loads hash-equal
/// exactly when address AND token agree (Click's insight: effects in
/// use-def means GVN needs no side-table of "may touch memory").
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct GvnKey {
    op: u16,
    ty: u32,
    args: Vec<u32>,
    aux: (u8, u64),
}

/// Per-block Braun bookkeeping.
#[derive(Clone, Default)]
struct BlockInfo {
    sealed: bool,
    filled: bool,
    /// Placeholder params awaiting `seal_block`: (variable, param value).
    incomplete: Vec<(Var, Value)>,
    /// Terminator instructions that branch here, one entry per
    /// predecessor terminator (a two-armed `br` into the same block
    /// still appears once — its edges are rewritten together).
    preds: Vec<Inst>,
}

/// The s25 builder. Owns the function under construction and a cursor;
/// the module (type interner + signatures) is borrowed for the whole
/// build.
pub struct FuncBuilder<'m> {
    pub module: &'m mut crate::ir::Module,
    pub func: Function,
    cur: Block,
    blocks: Vec<BlockInfo>,
    /// Global Braun map: (var, block) → definition.
    defs: HashMap<(u32, u32), Value>,
    var_ty: Vec<TypeId>,
    /// Semi-pruned: vars proven single-block by the caller's pre-scan.
    single_block: Vec<bool>,
    /// The per-block fast slot for single-block vars (cleared on every
    /// cursor move).
    local_slot: HashMap<u32, Value>,
    /// Params minted by Braun resolution — the only ones the trivial-φ
    /// cascade may remove (caller-created merge params keep their
    /// handles valid).
    braun_params: HashSet<Value>,
    /// Every value the trivial-φ test has replaced, old → new (s74,
    /// #66). `replace_value` rewrites the *stored* uses, but a removal's
    /// cascade can remove the very value an outer removal just chose as
    /// its replacement, leaving that outer frame holding a handle to a
    /// parameter that no longer exists. This is the forwarding chain
    /// that turns such a handle back into a live value.
    replaced: HashMap<Value, Value>,
    /// Scoped GVN: value table + the key list per scope.
    gvn: HashMap<GvnKey, Value>,
    gvn_scopes: Vec<Vec<GvnKey>>,
    /// Effect-token variables, minted on demand.
    mem_tokens: HashMap<u32, Var>,
    io_token: Option<Var>,
    /// The next fresh function-scoped region id (entry-token regions
    /// from the signature are pre-claimed; `region.new`/`stack.alloc`
    /// mint from here — one token root per region, verified).
    next_region: u32,
    pub stats: Stats,
}

impl<'m> FuncBuilder<'m> {
    /// Start building `name` with signature `sig`. The entry block is
    /// created from the signature, sealed, and made current; its
    /// parameter values are available via [`FuncBuilder::block_params`]
    /// for the caller to `def_var`.
    pub fn new(
        module: &'m mut crate::ir::Module,
        name: impl Into<String>,
        sig: crate::ir::SigId,
    ) -> FuncBuilder<'m> {
        let mut func = Function::new(name, sig);
        let param_tys: Vec<TypeId> = module.sigs[sig].params.iter().map(|p| p.ty).collect();
        let entry = func.make_block(&param_tys);
        let entry_params = func.block_params(entry);
        let mut b = FuncBuilder {
            module,
            func,
            cur: entry,
            blocks: vec![BlockInfo {
                sealed: true,
                ..BlockInfo::default()
            }],
            defs: HashMap::new(),
            var_ty: Vec::new(),
            single_block: Vec::new(),
            local_slot: HashMap::new(),
            braun_params: HashSet::new(),
            replaced: HashMap::new(),
            gvn: HashMap::new(),
            gvn_scopes: vec![Vec::new()],
            mem_tokens: HashMap::new(),
            io_token: None,
            next_region: 0,
            stats: Stats::default(),
        };
        // Signature token params seed their chains; their regions are
        // pre-claimed so minted regions never collide.
        for (i, &ty) in param_tys.iter().enumerate() {
            match b.module.types.get(ty) {
                TypeData::Mem(r) => {
                    let r = *r;
                    b.next_region = b.next_region.max(r.as_u32() + 1);
                    b.def_mem(r, entry_params[i]);
                }
                TypeData::Io => b.def_io(entry_params[i]),
                _ => {}
            }
        }
        b
    }

    /// Finish: every block must be sealed and filled. Returns the
    /// function (the caller verifies it — the round-trip gate).
    pub fn finish(self) -> Function {
        for (i, info) in self.blocks.iter().enumerate() {
            debug_assert!(info.sealed, "block b{i} left unsealed at finish");
            debug_assert!(info.filled, "block b{i} left unfilled at finish");
            debug_assert!(
                info.incomplete.is_empty(),
                "block b{i} has unresolved placeholder params"
            );
        }
        self.func
    }

    pub fn types(&mut self) -> &mut TypeInterner {
        &mut self.module.types
    }

    /// Set the source span every subsequently appended instruction
    /// records (s30 debug aux). Lowering calls this once per source
    /// statement; everything a statement expands into — checks, defer
    /// re-lowers, `?` branches — inherits its span, so stepping never
    /// lands on a line the user didn't write.
    pub fn set_span(&mut self, lo: u32, hi: u32) {
        self.func.span_cursor = Some(crate::ir::SrcSpan { lo, hi });
    }

    // ------------------------------------------------------ blocks ----

    /// Create an empty, unsealed block (parameters appear on demand
    /// through Braun resolution, or explicitly via
    /// [`FuncBuilder::add_block_param`]).
    pub fn create_block(&mut self) -> Block {
        let b = self.func.make_block(&[]);
        debug_assert_eq!(b.index(), self.blocks.len());
        self.blocks.push(BlockInfo::default());
        b
    }

    /// Explicitly add a typed parameter to a not-yet-sealed block (a
    /// merge value the caller owns — never removed by the cascade).
    pub fn add_block_param(&mut self, block: Block, ty: TypeId) -> Value {
        debug_assert!(
            !self.blocks[block.index()].sealed || self.blocks[block.index()].preds.is_empty(),
            "params added after sealing would desync existing edges"
        );
        self.append_param(block, ty)
    }

    /// Move the cursor. The target must not be filled.
    pub fn switch_to_block(&mut self, block: Block) {
        debug_assert!(
            !self.blocks[block.index()].filled,
            "switch to a filled block"
        );
        self.cur = block;
        self.local_slot.clear();
    }

    pub fn current_block(&self) -> Block {
        self.cur
    }

    /// Is the block already terminated? (A fold-to-trap fills a block
    /// mid-walk; callers consult this before emitting more.)
    pub fn is_filled(&self, block: Block) -> bool {
        self.blocks[block.index()].filled
    }

    /// Declare that every predecessor of `block` is now known. Resolves
    /// placeholder parameters. Sealing twice is an ICE.
    pub fn seal_block(&mut self, block: Block) {
        assert!(
            !self.blocks[block.index()].sealed,
            "seal_block called twice"
        );
        self.blocks[block.index()].sealed = true;
        let pending = std::mem::take(&mut self.blocks[block.index()].incomplete);
        for (var, param) in pending {
            self.resolve_param(block, var, param);
        }
    }

    pub fn block_params(&self, block: Block) -> Vec<Value> {
        self.func.block_params(block)
    }

    // ---------------------------------------------------- variables ----

    /// Mint a variable of type `ty`.
    pub fn declare_var(&mut self, ty: TypeId) -> Var {
        let v = Var(self.var_ty.len() as u32);
        self.var_ty.push(ty);
        self.single_block.push(false);
        v
    }

    /// Semi-pruned marking: the caller's pre-scan proved every def and
    /// use of `var` lands in one block, so it bypasses the global maps.
    pub fn mark_single_block(&mut self, var: Var) {
        self.single_block[var.0 as usize] = true;
    }

    /// Set `var`'s current definition in the current block.
    pub fn def_var(&mut self, var: Var, val: Value) {
        if self.single_block[var.0 as usize] {
            self.local_slot.insert(var.0, val);
            return;
        }
        self.defs.insert((var.0, self.cur.as_u32()), val);
    }

    /// Read `var`'s current definition (Braun: local hit, else recurse
    /// through predecessors, else placeholder param on the unsealed
    /// frontier).
    pub fn use_var(&mut self, var: Var) -> Value {
        if self.single_block[var.0 as usize] {
            match self.local_slot.get(&var.0) {
                Some(&v) => return v,
                None => {
                    // The pre-scan promised a local def; fall through
                    // to the global path defensively in release.
                    debug_assert!(false, "single-block var used without a local def");
                }
            }
        }
        self.use_var_in(var, self.cur)
    }

    fn use_var_in(&mut self, var: Var, block: Block) -> Value {
        if let Some(&v) = self.defs.get(&(var.0, block.as_u32())) {
            return v;
        }
        let val = if !self.blocks[block.index()].sealed {
            // Incomplete CFG: install a placeholder parameter; resolve
            // at seal time.
            let param = self.append_param(block, self.var_ty[var.0 as usize]);
            self.braun_params.insert(param);
            self.blocks[block.index()].incomplete.push((var, param));
            param
        } else {
            let preds: Vec<Block> = self.pred_blocks(block);
            match preds.len() {
                0 => panic!(
                    "use of a variable with no definition on some path \
                     (mem-checked input cannot reach this)"
                ),
                1 => self.use_var_in(var, preds[0]),
                _ => {
                    // Break cycles: define the parameter first, then
                    // resolve it against all predecessors.
                    let param = self.append_param(block, self.var_ty[var.0 as usize]);
                    self.braun_params.insert(param);
                    self.defs.insert((var.0, block.as_u32()), param);
                    self.resolve_param(block, var, param)
                }
            }
        };
        self.defs.insert((var.0, block.as_u32()), val);
        val
    }

    /// Complete a (placeholder) parameter: compute the incoming value
    /// in every predecessor, append it to each incoming edge, then run
    /// the trivial-φ test.
    fn resolve_param(&mut self, block: Block, var: Var, param: Value) -> Value {
        let preds = self.blocks[block.index()].preds.clone();
        for inst in preds {
            let pb = self.inst_block(inst);
            let arg = self.use_var_in(var, pb);
            self.append_edge_arg(inst, block, arg);
        }
        self.try_remove_trivial(block, param)
    }

    /// The trivial-φ test, verbatim on block-parameter args: a param
    /// whose incoming args are all itself or one same value is replaced
    /// by that value; removal cascades to dependent Braun params.
    fn try_remove_trivial(&mut self, block: Block, param: Value) -> Value {
        let Some(index) = self.param_index(block, param) else {
            // Already removed by a cascade: hand back what it became,
            // never the dead handle (s74, #66).
            return self.forwarded(param);
        };
        let mut same: Option<Value> = None;
        for inst in self.blocks[block.index()].preds.clone() {
            for edge in self.edges_to(inst, block) {
                let args = self.func.vpool.get(edge.args);
                if index >= args.len() {
                    // A sibling placeholder of the block being sealed:
                    // its edge args are not appended yet. Skip — the
                    // seal loop tests it when its own resolution runs.
                    return param;
                }
                let arg = args[index];
                if arg == param || Some(arg) == same {
                    continue;
                }
                if same.is_some() {
                    return param; // two distinct incoming values: real merge
                }
                same = Some(arg);
            }
        }
        let Some(rep) = same else {
            // Only self-references: keep the param (unreachable-cycle
            // shapes; mem-checked inputs never build them).
            return param;
        };
        // Remove the param (and its arg slot on every incoming edge),
        // rewrite every use, then re-test Braun params that might have
        // become trivial (the cascade).
        self.remove_param(block, index);
        self.braun_params.remove(&param);
        self.replace_value(param, rep);
        let blocks: Vec<Block> = self.func.layout.clone();
        for b in blocks {
            if !self.blocks[b.index()].sealed {
                continue;
            }
            for p in self.func.block_params(b) {
                if self.braun_params.contains(&p) {
                    self.try_remove_trivial(b, p);
                }
            }
        }
        // The cascade above can remove `rep` itself — a sibling
        // parameter that only became trivial because THIS one went away
        // (s74, #66). Returning the pre-cascade handle hands the caller
        // a parameter that no longer exists, and `use_var_in` caches it,
        // so the next instruction built takes a phantom operand: the
        // shape that reached the printer as a value it could name and
        // not define. Follow the chain instead.
        self.forwarded(rep)
    }

    /// Chase a value through the trivial-φ replacements applied since it
    /// was chosen. Terminates: each link maps a removed parameter to a
    /// value chosen strictly earlier in the removal order.
    fn forwarded(&self, v: Value) -> Value {
        let mut cur = v;
        let mut guard = 0usize;
        while let Some(&next) = self.replaced.get(&cur) {
            if next == cur {
                break;
            }
            cur = next;
            guard += 1;
            debug_assert!(
                guard <= self.func.values.len(),
                "trivial-phi forwarding chain does not terminate"
            );
            if guard > self.func.values.len() {
                break;
            }
        }
        cur
    }

    /// Append a fresh parameter value to a block.
    fn append_param(&mut self, block: Block, ty: TypeId) -> Value {
        let mut params = self.func.vpool.get(self.func.blocks[block].params);
        let idx = params.len() as u16;
        let v = self.func.values.push(crate::ir::ValueData {
            ty,
            def: ValueDef::Param(block, idx),
        });
        params.push(v);
        self.func.blocks[block].params = self.func.vpool.intern(&params);
        v
    }

    fn param_index(&self, block: Block, param: Value) -> Option<usize> {
        self.func
            .block_params(block)
            .iter()
            .position(|&p| p == param)
    }

    /// Drop parameter `index` from `block` and the matching argument
    /// from every incoming edge; later params' def indices shift down.
    fn remove_param(&mut self, block: Block, index: usize) {
        let mut params = self.func.block_params(block);
        params.remove(index);
        for (i, &p) in params.iter().enumerate() {
            let ty = self.func.values[p].ty;
            self.func.values[p] = crate::ir::ValueData {
                ty,
                def: ValueDef::Param(block, i as u16),
            };
        }
        self.func.blocks[block].params = self.func.vpool.intern(&params);
        for inst in self.blocks[block.index()].preds.clone() {
            self.rewrite_edges(inst, |bc, args| {
                if bc.block == block {
                    let mut args = args;
                    args.remove(index);
                    Some(args)
                } else {
                    None
                }
            });
        }
    }

    /// Rewrite every use of `old` to `new`: instruction operands,
    /// branch-edge args, Braun maps, token slots, and GVN entries.
    fn replace_value(&mut self, old: Value, new: Value) {
        // The forwarding chain (s74, #66): rewriting stored uses is not
        // enough, because a handle can be live on the Rust call stack of
        // a nested removal. Record the edge so [`Self::forwarded`] can
        // repair such a handle.
        self.replaced.insert(old, new);
        let insts: Vec<Inst> = self.func.insts.keys().collect();
        for inst in insts {
            let args = self.func.vpool.get(self.func.insts[inst].args);
            if args.contains(&old) {
                let rewritten: Vec<Value> = args
                    .iter()
                    .map(|&a| if a == old { new } else { a })
                    .collect();
                self.func.insts[inst].args = self.func.vpool.intern(&rewritten);
            }
            self.rewrite_edges(inst, |_, args| {
                if args.contains(&old) {
                    Some(
                        args.into_iter()
                            .map(|a| if a == old { new } else { a })
                            .collect(),
                    )
                } else {
                    None
                }
            });
        }
        for v in self.defs.values_mut() {
            if *v == old {
                *v = new;
            }
        }
        for v in self.local_slot.values_mut() {
            if *v == old {
                *v = new;
            }
        }
        for info in &mut self.blocks {
            for (_, p) in &mut info.incomplete {
                if *p == old {
                    *p = new;
                }
            }
        }
        for v in self.gvn.values_mut() {
            if *v == old {
                *v = new;
            }
        }
        for dv in &mut self.func.debug_vars {
            if dv.val == old {
                dv.val = new;
            }
        }
    }

    /// Apply `f` to each branch edge of `inst`, replacing arg lists
    /// where it returns `Some`.
    fn rewrite_edges(
        &mut self,
        inst: Inst,
        mut f: impl FnMut(BlockCall, Vec<Value>) -> Option<Vec<Value>>,
    ) {
        let aux = self.func.insts[inst].aux;
        let new_aux = match aux {
            Aux::Jump(bc) => {
                let args = self.func.vpool.get(bc.args);
                match f(bc, args) {
                    Some(args) => Aux::Jump(BlockCall {
                        block: bc.block,
                        args: self.func.vpool.intern(&args),
                    }),
                    None => return,
                }
            }
            Aux::Br(t, e) => {
                let targs = self.func.vpool.get(t.args);
                let eargs = self.func.vpool.get(e.args);
                let nt = f(t, targs).map(|args| BlockCall {
                    block: t.block,
                    args: self.func.vpool.intern(&args),
                });
                let ne = f(e, eargs).map(|args| BlockCall {
                    block: e.block,
                    args: self.func.vpool.intern(&args),
                });
                if nt.is_none() && ne.is_none() {
                    return;
                }
                Aux::Br(nt.unwrap_or(t), ne.unwrap_or(e))
            }
            _ => return,
        };
        self.func.insts[inst].aux = new_aux;
    }

    fn append_edge_arg(&mut self, inst: Inst, target: Block, arg: Value) {
        self.rewrite_edges(inst, |bc, args| {
            if bc.block == target {
                let mut args = args;
                args.push(arg);
                Some(args)
            } else {
                None
            }
        });
    }

    fn edges_to(&self, inst: Inst, target: Block) -> Vec<BlockCall> {
        match self.func.insts[inst].aux {
            Aux::Jump(bc) if bc.block == target => vec![bc],
            Aux::Jump(_) => vec![],
            Aux::Br(t, e) => {
                let mut out = Vec::new();
                if t.block == target {
                    out.push(t);
                }
                if e.block == target {
                    out.push(e);
                }
                out
            }
            _ => vec![],
        }
    }

    fn pred_blocks(&self, block: Block) -> Vec<Block> {
        self.blocks[block.index()]
            .preds
            .iter()
            .map(|&i| self.inst_block(i))
            .collect()
    }

    fn inst_block(&self, inst: Inst) -> Block {
        // A terminator is the last instruction of exactly one block.
        for &b in &self.func.layout {
            if self.func.blocks[b].insts.last() == Some(&inst) {
                return b;
            }
        }
        panic!("terminator not found in any block");
    }

    // ------------------------------------------------- effect tokens ----

    fn token_var(&mut self, ty: TypeId) -> Var {
        let v = Var(self.var_ty.len() as u32);
        self.var_ty.push(ty);
        self.single_block.push(false);
        v
    }

    /// Seed / redefine region `r`'s current `mem` token.
    pub fn def_mem(&mut self, region: RegionId, tok: Value) {
        let v = self.mem_var(region);
        self.def_var(v, tok);
    }

    /// Region `r`'s current token (Braun-resolved: merges become block
    /// params, loop-carried tokens work like any variable).
    pub fn use_mem(&mut self, region: RegionId) -> Value {
        let v = self.mem_var(region);
        self.use_var(v)
    }

    fn mem_var(&mut self, region: RegionId) -> Var {
        if let Some(&v) = self.mem_tokens.get(&region.as_u32()) {
            return v;
        }
        let ty = self.module.types.mem(region);
        let v = self.token_var(ty);
        self.mem_tokens.insert(region.as_u32(), v);
        v
    }

    pub fn def_io(&mut self, tok: Value) {
        let v = self.io_var();
        self.def_var(v, tok);
    }

    pub fn use_io(&mut self) -> Value {
        let v = self.io_var();
        self.use_var(v)
    }

    fn io_var(&mut self) -> Var {
        if let Some(v) = self.io_token {
            return v;
        }
        let v = self.token_var(crate::types::IO);
        self.io_token = Some(v);
        v
    }

    // ------------------------------------------------- the gauntlet ----

    /// Append one instruction through the peephole gauntlet:
    /// speculative build at a mark → constant fold → identity table →
    /// GVN → LIFO delete on any hit.
    pub fn ins(&mut self, op: Opcode, args: &[Value], result_tys: &[TypeId], aux: Aux) -> InsOut {
        debug_assert!(
            !self.blocks[self.cur.index()].filled,
            "ins into a filled block"
        );
        debug_assert!(
            !op.is_terminator(),
            "terminators go through ins_jmp/ins_br/ins_ret/ins_trap"
        );
        // Operands must be LIVE definitions: a parameter handle that a
        // trivial-φ cascade retired is a phantom, and building with it
        // produces WIR that names a value nothing defines (s74, #66).
        // The forwarding chain makes that unreachable; this is the
        // assertion that keeps it so.
        debug_assert!(
            args.iter().all(|&a| match self.func.values[a].def {
                ValueDef::Param(db, di) => self.func.block_params(db).get(di as usize) == Some(&a),
                ValueDef::Result(..) => true,
            }),
            "an operand names a block parameter that was removed"
        );
        // 1. Speculative build at the mark (operands already exist —
        //    Braun guarantees inputs).
        let mark = self.func.mark();
        let (inst, results) = self.func.append_inst(self.cur, op, args, result_tys, aux);
        // 2. Fold.
        match self.try_fold(inst) {
            Fold::None => {}
            Fold::Const(c) => {
                self.func.truncate_to_mark(mark);
                self.stats.fold += 1;
                return InsOut::Vals(vec![self.emit_const(result_tys[0], c)]);
            }
            Fold::Trap(kind) => {
                self.func.truncate_to_mark(mark);
                self.stats.fold += 1;
                self.ins_trap(kind);
                return InsOut::Trapped;
            }
        }
        // 3. Identity rewrites (the data-driven table).
        if let Some(rw) = self.try_identity(inst) {
            self.func.truncate_to_mark(mark);
            self.stats.identity += 1;
            let out = match rw {
                Rewritten::Value(v) => v,
                Rewritten::ConstInt(n) => self.emit_const(result_tys[0], Const::Int(n)),
                Rewritten::ConstBool(b) => self.emit_const(result_tys[0], Const::Bool(b)),
            };
            return InsOut::Vals(vec![out]);
        }
        // 4. GVN via hash-consing at insert.
        if let Some(key) = self.gvn_key(inst) {
            if let Some(&hit) = self.gvn.get(&key) {
                self.func.truncate_to_mark(mark);
                self.stats.gvn += 1;
                return InsOut::Vals(vec![hit]);
            }
            self.gvn.insert(key.clone(), results[0]);
            self.gvn_scopes.last_mut().expect("scope stack").push(key);
        }
        self.stats.insts += 1;
        InsOut::Vals(results)
    }

    /// Push a GVN visibility scope. Callers bracket structured regions
    /// (branch arms, loop bodies): construction order follows the
    /// structured AST, so a scope stack aligned with block sealing
    /// gives dominance-safe visibility without a dominator tree.
    pub fn gvn_push_scope(&mut self) {
        self.gvn_scopes.push(Vec::new());
    }

    /// Pop a GVN scope, dropping every entry it added.
    pub fn gvn_pop_scope(&mut self) {
        let keys = self.gvn_scopes.pop().expect("gvn scope underflow");
        for key in keys {
            self.gvn.remove(&key);
        }
    }

    // -------------------------------------------------- constants ----

    pub fn iconst(&mut self, ty: TypeId, n: i64) -> Value {
        self.ins(Opcode::Iconst, &[], &[ty], Aux::Int(n)).one()
    }

    pub fn fconst(&mut self, ty: TypeId, bits: u64) -> Value {
        self.ins(Opcode::Fconst, &[], &[ty], Aux::FloatBits(bits))
            .one()
    }

    pub fn bconst(&mut self, b: bool) -> Value {
        self.ins(Opcode::Bconst, &[], &[crate::types::BOOL], Aux::Bool(b))
            .one()
    }

    fn emit_const(&mut self, ty: TypeId, c: Const) -> Value {
        match c {
            Const::Int(n) => self.iconst(ty, n),
            Const::Bool(b) => self.bconst(b),
        }
    }

    /// The constant behind a value, when its def is a const op.
    pub fn as_const(&self, v: Value) -> Option<Const> {
        let ValueDef::Result(inst, 0) = self.func.values[v].def else {
            return None;
        };
        match (self.func.insts[inst].op, self.func.insts[inst].aux) {
            (Opcode::Iconst, Aux::Int(n)) => Some(Const::Int(n)),
            (Opcode::Bconst, Aux::Bool(b)) => Some(Const::Bool(b)),
            _ => None,
        }
    }

    pub fn as_bool_const(&self, v: Value) -> Option<bool> {
        match self.as_const(v) {
            Some(Const::Bool(b)) => Some(b),
            _ => None,
        }
    }

    pub fn as_int_const(&self, v: Value) -> Option<i64> {
        match self.as_const(v) {
            Some(Const::Int(n)) => Some(n),
            _ => None,
        }
    }

    // ------------------------------------------------- memory ops ----

    /// `load.T addr` through region `r`'s current token, with
    /// store→load forwarding: a load whose token was defined by a store
    /// to the same address value forwards the stored value (through the
    /// token chain, never an alias pass).
    pub fn ins_load(&mut self, ty: TypeId, addr: Value, region: RegionId) -> Value {
        let tok = self.use_mem(region);
        if let ValueDef::Result(inst, 0) = self.func.values[tok].def
            && self.func.insts[inst].op == Opcode::Store
        {
            let sargs = self.func.vpool.get(self.func.insts[inst].args);
            if sargs[1] == addr && self.func.values[sargs[0]].ty == ty {
                self.stats.forward += 1;
                return sargs[0];
            }
        }
        self.ins(Opcode::Load, &[addr, tok], &[ty], Aux::None).one()
    }

    /// `store.T val, addr` through region `r`'s token chain (consumes
    /// the current token, defines the successor).
    pub fn ins_store(&mut self, val: Value, addr: Value, region: RegionId) {
        let tok = self.use_mem(region);
        let tok_ty = self.func.values[tok].ty;
        let out = self
            .ins(Opcode::Store, &[val, addr, tok], &[tok_ty], Aux::None)
            .one();
        self.def_mem(region, out);
    }

    /// `ptr.off base, index, scale`.
    /// `data.addr @sym` (s31): the address of module data entry `idx`
    /// (from [`crate::ir::Module::intern_data`]). Pure; GVN dedups it.
    pub fn ins_data_addr(&mut self, idx: u32) -> Value {
        self.ins(Opcode::DataAddr, &[], &[crate::types::PTR], Aux::Data(idx))
            .one()
    }

    /// `func.addr @f()` (s73): the address of a module function or
    /// known symbol as an opaque `ptr` — the compiled task-entry
    /// pointer. Pure; GVN dedups it.
    pub fn ins_func_addr(&mut self, callee: crate::ir::ExtFunc) -> Value {
        self.ins(
            Opcode::FuncAddr,
            &[],
            &[crate::types::PTR],
            Aux::Callee(callee),
        )
        .one()
    }

    pub fn ins_ptr_off(&mut self, base: Value, index: Value, scale: u64) -> Value {
        self.ins(
            Opcode::PtrOff,
            &[base, index],
            &[crate::types::PTR],
            Aux::Scale(scale),
        )
        .one()
    }

    // ------------------------------------------- the memory family ----

    /// Mint a fresh function-scoped region id.
    pub fn fresh_region(&mut self) -> RegionId {
        let r = RegionId::new(self.next_region);
        self.next_region += 1;
        r
    }

    /// `%h, %m = region.new` — create a fresh region; its token chain
    /// is seeded, its handle returned.
    pub fn ins_region_new(&mut self) -> (RegionId, Value) {
        let r = self.fresh_region();
        let tok_ty = self.module.types.mem(r);
        let out = self.ins(
            Opcode::RegionNew,
            &[],
            &[crate::types::PTR, tok_ty],
            Aux::None,
        );
        let InsOut::Vals(vals) = out else {
            unreachable!("region.new never folds");
        };
        self.def_mem(r, vals[1]);
        (r, vals[0])
    }

    /// `%p, %m2 = region.alloc h, size, m` — allocate `size` bytes in
    /// `region`; threads the region's token chain; returns the pointer.
    pub fn ins_region_alloc(&mut self, region: RegionId, handle: Value, size: Value) -> Value {
        let tok = self.use_mem(region);
        let tok_ty = self.func.values[tok].ty;
        let out = self.ins(
            Opcode::RegionAlloc,
            &[handle, size, tok],
            &[crate::types::PTR, tok_ty],
            Aux::None,
        );
        let InsOut::Vals(vals) = out else {
            unreachable!("region.alloc never folds");
        };
        self.def_mem(region, vals[1]);
        vals[0]
    }

    /// `region.free h, m` — wholesale free: consumes the region's
    /// current token and produces NO successor (no live token, no
    /// loads — the verifier's structural use-after-free rejection).
    pub fn ins_region_free(&mut self, region: RegionId, handle: Value) {
        let tok = self.use_mem(region);
        let out = self.ins(Opcode::RegionFree, &[handle, tok], &[], Aux::None);
        let InsOut::Vals(_) = out else {
            unreachable!("region.free never folds");
        };
    }

    /// `%m2 = rc.dup p, m` / `%m2 = rc.drop p, m` — token-threaded RC
    /// traffic (drops order against loads/stores).
    pub fn ins_rc(&mut self, dup: bool, ptr: Value, region: RegionId) {
        let tok = self.use_mem(region);
        let tok_ty = self.func.values[tok].ty;
        let op = if dup { Opcode::RcDup } else { Opcode::RcDrop };
        let out = self.ins(op, &[ptr, tok], &[tok_ty], Aux::None);
        let InsOut::Vals(vals) = out else {
            unreachable!("rc ops never fold");
        };
        self.def_mem(region, vals[0]);
    }

    /// `%f = sync.freeze h, m` — consume the region's mutable token and
    /// install the frozen (never-invalidated, never-consumable) token
    /// as its current one: subsequent loads read frozen data forever.
    pub fn ins_sync_freeze(&mut self, region: RegionId, handle: Value) -> Value {
        let tok = self.use_mem(region);
        let tok_ty = self.func.values[tok].ty;
        let out = self.ins(Opcode::SyncFreeze, &[handle, tok], &[tok_ty], Aux::None);
        let InsOut::Vals(vals) = out else {
            unreachable!("sync.freeze never folds");
        };
        self.def_mem(region, vals[0]);
        vals[0]
    }

    /// `%p, %m = stack.alloc size` — one frame slot with its own
    /// one-slot region (stack provenance; the s19 promotion landing
    /// pad). Returns the slot's region and pointer.
    pub fn ins_stack_alloc(&mut self, size_bytes: u64) -> (RegionId, Value) {
        let size = self.iconst(crate::types::I64, size_bytes as i64);
        let r = self.fresh_region();
        let tok_ty = self.module.types.mem(r);
        let out = self.ins(
            Opcode::StackAlloc,
            &[size],
            &[crate::types::PTR, tok_ty],
            Aux::None,
        );
        let InsOut::Vals(vals) = out else {
            unreachable!("stack.alloc never folds");
        };
        self.def_mem(r, vals[1]);
        (r, vals[0])
    }

    // ------------------------------------------- error unions (s27) ----

    /// `%e = eu.make.ok %v?` — wrap the ok payload (or nothing, for a
    /// unit ok half) into `eu_ty`.
    pub fn ins_eu_make_ok(&mut self, eu_ty: TypeId, v: Option<Value>) -> Value {
        let args: Vec<Value> = v.into_iter().collect();
        self.ins(Opcode::EuMakeOk, &args, &[eu_ty], Aux::None).one()
    }

    /// `%e = eu.make.err %tag, %payloads…` — wrap an error tag (and a
    /// prefix of the union's payload slots) into `eu_ty`.
    pub fn ins_eu_make_err(&mut self, eu_ty: TypeId, tag: Value, payloads: &[Value]) -> Value {
        let mut args = vec![tag];
        args.extend_from_slice(payloads);
        self.ins(Opcode::EuMakeErr, &args, &[eu_ty], Aux::None)
            .one()
    }

    /// `%b = eu.is_err %e`.
    pub fn ins_eu_is_err(&mut self, e: Value) -> Value {
        self.ins(Opcode::EuIsErr, &[e], &[crate::types::BOOL], Aux::None)
            .one()
    }

    /// `%v = eu.ok %e` — the union's ok payload (the union must have a
    /// non-unit ok half).
    pub fn ins_eu_ok(&mut self, e: Value) -> Value {
        let TypeData::Eu { ok: Some(t), .. } = *self.module.types.get(self.func.value_ty(e)) else {
            panic!("eu.ok on a non-eu or unit-ok value");
        };
        self.ins(Opcode::EuOk, &[e], &[t], Aux::None).one()
    }

    /// `%t = eu.err %e` — the union's error tag.
    pub fn ins_eu_err_tag(&mut self, e: Value) -> Value {
        self.ins(Opcode::EuErr, &[e], &[crate::types::I64], Aux::None)
            .one()
    }

    /// `%p = eu.err %e, K` — error-payload slot K.
    pub fn ins_eu_err_slot(&mut self, e: Value, k: usize) -> Value {
        let TypeData::Eu { ref slots, .. } = *self.module.types.get(self.func.value_ty(e)) else {
            panic!("eu.err on a non-eu value");
        };
        let slot = slots[k];
        self.ins(Opcode::EuErr, &[e], &[slot], Aux::Int(k as i64))
            .one()
    }

    // ------------------------------------------------------- calls ----

    /// `call @callee(args…)` with the mode-carrying signature: token
    /// parameters are filled from the current token chains (consumed),
    /// and each minted successor token re-defines its chain. `args`
    /// supplies the non-token arguments in order. Returns the declared
    /// results.
    pub fn ins_call(&mut self, callee: ExtFunc, args: &[Value]) -> Vec<Value> {
        self.ins_call_regions(callee, args, &HashMap::new())
    }

    /// `ins_call` with region-polymorphic binding (s26): a callee sig's
    /// `mem.rF` token params name FORMAL regions; `formal_regions` maps
    /// each formal (by index) to the caller's ACTUAL region — the
    /// spill-slot or arena the argument pointer lives in. Unbound
    /// formals fall back to the identity (same-numbered caller region:
    /// the s25 fixture behavior).
    pub fn ins_call_regions(
        &mut self,
        callee: ExtFunc,
        args: &[Value],
        formal_regions: &HashMap<u32, RegionId>,
    ) -> Vec<Value> {
        let sig = self.func.ext_funcs[callee].sig;
        let params = self.module.sigs[sig].params.clone();
        let declared: Vec<TypeId> = self.module.sigs[sig].results.clone();
        let mut full_args = Vec::with_capacity(params.len());
        // Token params in order: Some(actual region) for mem, None for io.
        let mut tokens: Vec<Option<RegionId>> = Vec::new();
        let mut next = 0usize;
        for p in &params {
            match self.module.types.get(p.ty).clone() {
                TypeData::Mem(f) => {
                    let actual = formal_regions.get(&f.as_u32()).copied().unwrap_or(f);
                    let tok = self.use_mem(actual);
                    full_args.push(tok);
                    tokens.push(Some(actual));
                }
                TypeData::Io => {
                    let tok = self.use_io();
                    full_args.push(tok);
                    tokens.push(None);
                }
                _ => {
                    full_args.push(args[next]);
                    next += 1;
                }
            }
        }
        debug_assert_eq!(next, args.len(), "call argument count mismatch");
        let n_declared = declared.len();
        let mut rtys = declared;
        for &region in &tokens {
            match region {
                Some(r) => {
                    let t = self.module.types.mem(r);
                    rtys.push(t);
                }
                None => rtys.push(crate::types::IO),
            }
        }
        let out = self.ins(Opcode::Call, &full_args, &rtys, Aux::Callee(callee));
        let InsOut::Vals(vals) = out else {
            unreachable!("calls never fold");
        };
        for (i, region) in tokens.iter().enumerate() {
            let succ = vals[n_declared + i];
            match region {
                Some(r) => self.def_mem(*r, succ),
                None => self.def_io(succ),
            }
        }
        vals[..n_declared].to_vec()
    }

    // ------------------------------------------------- terminators ----

    /// `jmp target(args…)`. Fills the current block.
    pub fn ins_jmp(&mut self, target: Block, args: &[Value]) {
        let edge = self.func.block_call(target, args);
        let (inst, _) = self
            .func
            .append_inst(self.cur, Opcode::Jmp, &[], &[], Aux::Jump(edge));
        self.stats.insts += 1;
        self.fill(inst, &[target]);
    }

    /// `br cond, then(args), else(args)` — with the `br const → jmp`
    /// peephole: a constant condition emits the taken edge only. (The
    /// not-taken block then has no predecessor; callers that pre-check
    /// [`FuncBuilder::as_bool_const`] avoid creating it at all.)
    pub fn ins_br(
        &mut self,
        cond: Value,
        then_block: Block,
        then_args: &[Value],
        else_block: Block,
        else_args: &[Value],
    ) {
        if let Some(b) = self.as_bool_const(cond) {
            self.stats.identity += 1;
            if b {
                self.ins_jmp(then_block, then_args);
            } else {
                self.ins_jmp(else_block, else_args);
            }
            return;
        }
        let t = self.func.block_call(then_block, then_args);
        let e = self.func.block_call(else_block, else_args);
        let (inst, _) = self
            .func
            .append_inst(self.cur, Opcode::Br, &[cond], &[], Aux::Br(t, e));
        self.stats.insts += 1;
        if then_block == else_block {
            self.fill(inst, &[then_block]);
        } else {
            self.fill(inst, &[then_block, else_block]);
        }
    }

    /// `ret vals…`. Fills the current block.
    pub fn ins_ret(&mut self, vals: &[Value]) {
        let (inst, _) = self
            .func
            .append_inst(self.cur, Opcode::Ret, vals, &[], Aux::None);
        self.stats.insts += 1;
        self.fill(inst, &[]);
    }

    /// `trap` with its kind (s28: which check fired — the identity
    /// conformance verdicts report). Fills the current block.
    pub fn ins_trap(&mut self, kind: TrapKind) {
        let (inst, _) = self
            .func
            .append_inst(self.cur, Opcode::Trap, &[], &[], Aux::Trap(kind));
        self.stats.insts += 1;
        self.fill(inst, &[]);
    }

    fn fill(&mut self, terminator: Inst, succs: &[Block]) {
        self.blocks[self.cur.index()].filled = true;
        self.local_slot.clear();
        for &s in succs {
            debug_assert!(
                !self.blocks[s.index()].sealed,
                "new predecessor for an already-sealed block"
            );
            self.blocks[s.index()].preds.push(terminator);
        }
    }

    // ----------------------------------------------------- folding ----

    fn try_fold(&self, inst: Inst) -> Fold {
        let data = &self.func.insts[inst];
        let args = self.func.vpool.get(data.args);
        let results = self.func.vpool.get(data.results);
        let cst = |v: Value| self.as_const(v);
        let ints = || -> Option<(i64, i64)> {
            match (cst(args[0])?, cst(args[1])?) {
                (Const::Int(a), Const::Int(b)) => Some((a, b)),
                _ => None,
            }
        };
        let bits_of = |v: Value| self.module.types.int_bits(self.func.values[v].ty);
        let bounds_of = |v: Value| self.module.types.int_bounds(self.func.values[v].ty);
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
                    // X3: a provable overflow IS the runtime trap.
                    Fold::Trap(TrapKind::Overflow)
                } else {
                    Fold::Const(Const::Int(r as i64))
                }
            }
            Opcode::IdivChk | Opcode::IremChk => {
                // A constant zero divisor traps regardless of the lhs.
                if let Some(Const::Int(0)) = cst(args[1]) {
                    return Fold::Trap(TrapKind::DivZero);
                }
                let Some((a, b)) = ints() else {
                    return Fold::None;
                };
                let (lo, _) = bounds_of(results[0]).expect("int op");
                if (a as i128) == lo && b == -1 {
                    return Fold::Trap(TrapKind::Overflow); // MIN / -1 overflows
                }
                let r = if data.op == Opcode::IdivChk {
                    a / b
                } else {
                    a % b
                };
                Fold::Const(Const::Int(r))
            }
            Opcode::UaddChk
            | Opcode::UsubChk
            | Opcode::UmulChk
            | Opcode::UdivChk
            | Opcode::UremChk => {
                // A constant zero divisor traps regardless of the lhs.
                if matches!(data.op, Opcode::UdivChk | Opcode::UremChk)
                    && matches!(cst(args[1]), Some(Const::Int(0)))
                {
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
                            return Fold::Trap(TrapKind::Overflow); // borrow
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
                    // Unsigned overflow IS the runtime trap (X3).
                    Fold::Trap(TrapKind::Overflow)
                } else {
                    Fold::Const(Const::Int(wrap_to(r as i128, bits)))
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
                Fold::Const(Const::Int(wrap_to(r, bits)))
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
                Fold::Const(Const::Int(r.clamp(lo, hi) as i64))
            }
            Opcode::Band | Opcode::Bor | Opcode::Bxor => {
                let Some((a, b)) = ints() else {
                    return Fold::None;
                };
                let r = match data.op {
                    Opcode::Band => a & b,
                    Opcode::Bor => a | b,
                    _ => a ^ b,
                };
                Fold::Const(Const::Int(r))
            }
            Opcode::Shl | Opcode::Lshr | Opcode::Ashr => {
                let Some((a, b)) = ints() else {
                    return Fold::None;
                };
                let bits = bits_of(results[0]).expect("int op");
                let sh = (b as u32) & (bits - 1);
                let r = match data.op {
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
                };
                Fold::Const(Const::Int(r))
            }
            Opcode::Icmp => {
                let Some((a, b)) = ints() else {
                    return Fold::None;
                };
                let Aux::IntCc(cc) = data.aux else {
                    return Fold::None;
                };
                let (ua, ub) = (a as u64, b as u64);
                let r = match cc {
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
                };
                Fold::Const(Const::Bool(r))
            }
            Opcode::Sext | Opcode::Zext | Opcode::Itrunc => {
                let Some(Const::Int(a)) = cst(args[0]) else {
                    return Fold::None;
                };
                let from_bits = bits_of(args[0]).expect("int conv");
                let to_bits = bits_of(results[0]).expect("int conv");
                let r = match data.op {
                    // iconst payloads are stored sign-extended already.
                    Opcode::Sext => a,
                    Opcode::Zext => {
                        let mask = if from_bits >= 64 {
                            u64::MAX
                        } else {
                            (1u64 << from_bits) - 1
                        };
                        ((a as u64) & mask) as i64
                    }
                    _ => wrap_to(a as i128, to_bits),
                };
                Fold::Const(Const::Int(r))
            }
            _ => Fold::None,
        }
    }

    // ---------------------------------------------- identity table ----

    fn try_identity(&self, inst: Inst) -> Option<Rewritten> {
        let data = &self.func.insts[inst];
        let args = self.func.vpool.get(data.args);
        // eu.* splits over a locally-built union fold to their
        // ingredients (s27): `?` on an in-function raise costs nothing.
        if matches!(data.op, Opcode::EuIsErr | Opcode::EuOk | Opcode::EuErr) {
            if let ValueDef::Result(mi, 0) = self.func.values[args[0]].def {
                let mop = self.func.insts[mi].op;
                let margs = self.func.vpool.get(self.func.insts[mi].args);
                match (data.op, mop) {
                    (Opcode::EuIsErr, Opcode::EuMakeOk) => {
                        return Some(Rewritten::ConstBool(false));
                    }
                    (Opcode::EuIsErr, Opcode::EuMakeErr) => {
                        return Some(Rewritten::ConstBool(true));
                    }
                    (Opcode::EuOk, Opcode::EuMakeOk) if !margs.is_empty() => {
                        return Some(Rewritten::Value(margs[0]));
                    }
                    (Opcode::EuErr, Opcode::EuMakeErr) => {
                        let idx = match data.aux {
                            Aux::None => 0,
                            Aux::Int(k) => k as usize + 1,
                            _ => return None,
                        };
                        if let Some(&v) = margs.get(idx) {
                            return Some(Rewritten::Value(v));
                        }
                    }
                    _ => {}
                }
            }
            return None;
        }
        if data.op == Opcode::Icmp && args.len() == 2 && args[0] == args[1] {
            let Aux::IntCc(cc) = data.aux else {
                return None;
            };
            return Some(Rewritten::ConstBool(peephole_rules::icmp_same(cc)));
        }
        if args.len() != 2 {
            return None;
        }
        let lhs_c = self.as_int_const(args[0]);
        let rhs_c = self.as_int_const(args[1]);
        let rw = peephole_rules::match_rules(data.op, args[0] == args[1], lhs_c, rhs_c)?;
        Some(match rw {
            Rewrite::Lhs => Rewritten::Value(args[0]),
            Rewrite::Rhs => Rewritten::Value(args[1]),
            Rewrite::ConstInt(n) => Rewritten::ConstInt(n),
            Rewrite::ConstBool(b) => Rewritten::ConstBool(b),
        })
    }

    // -------------------------------------------------------- GVN ----

    fn gvn_key(&self, inst: Inst) -> Option<GvnKey> {
        let data = &self.func.insts[inst];
        let results = self.func.vpool.get(data.results);
        if results.len() != 1 {
            return None;
        }
        // Pure ops only. Loads join: their token operand is in the key,
        // which is exactly redundant-load elimination (same address,
        // same token). Stores, calls, and terminators never enter.
        let pure = matches!(
            data.op,
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
                | Opcode::Load
                | Opcode::AggMake
                | Opcode::AggGet
                | Opcode::DataAddr
                | Opcode::FuncAddr
                | Opcode::EuMakeOk
                | Opcode::EuMakeErr
                | Opcode::EuIsErr
                | Opcode::EuOk
                | Opcode::EuErr
        );
        if !pure {
            return None;
        }
        let mut args: Vec<u32> = self
            .func
            .vpool
            .get(data.args)
            .iter()
            .map(|v| v.as_u32())
            .collect();
        let commutative = matches!(
            data.op,
            Opcode::IaddChk
                | Opcode::ImulChk
                | Opcode::IaddWrap
                | Opcode::ImulWrap
                | Opcode::IaddSat
                | Opcode::ImulSat
                | Opcode::UaddChk
                | Opcode::UmulChk
                | Opcode::Band
                | Opcode::Bor
                | Opcode::Bxor
                | Opcode::Fadd
                | Opcode::Fmul
        ) || matches!(
            (data.op, data.aux),
            (Opcode::Icmp, Aux::IntCc(IntCc::Eq)) | (Opcode::Icmp, Aux::IntCc(IntCc::Ne))
        );
        if commutative {
            args.sort_unstable();
        }
        let aux = match data.aux {
            Aux::None => (0u8, 0u64),
            Aux::Int(n) => (1, n as u64),
            Aux::FloatBits(b) => (2, b),
            Aux::Bool(b) => (3, b as u64),
            Aux::IntCc(cc) => (4, cc as u64),
            Aux::FloatCc(cc) => (5, cc as u64),
            Aux::Scale(s) => (6, s),
            Aux::Data(idx) => (7, idx as u64),
            // Edge- or callee-carrying ops (and terminators) never
            // reach here.
            Aux::Callee(_) | Aux::Jump(_) | Aux::Br(..) | Aux::Trap(_) => return None,
        };
        Some(GvnKey {
            op: data.op as u16,
            ty: self.func.values[results[0]].ty.as_u32(),
            args,
            aux,
        })
    }
}

/// A folded constant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Const {
    Int(i64),
    Bool(bool),
}

enum Fold {
    None,
    Const(Const),
    Trap(TrapKind),
}

enum Rewritten {
    Value(Value),
    ConstInt(i64),
    ConstBool(bool),
}

/// Two's-complement wrap of `v` into a `bits`-wide signed integer.
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
