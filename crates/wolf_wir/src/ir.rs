//! The WIR data model: a [`Module`] of [`Function`]s, each a flat set
//! of arenas plus a block layout order. Blocks carry typed BLOCK
//! PARAMETERS and branches pass argument lists — no φ instruction
//! exists anywhere in this IR (D1). Function parameters are the entry
//! block's parameters.
//!
//! All per-entity data lives in side tables reached by u32 indices;
//! a value knows its def (instruction result or block param) via its
//! [`ValueData`]. The mutation surface here is deliberately RAW —
//! append-only plus LIFO rollback ([`Function::mark`] /
//! [`Function::truncate_to_mark`]); s25's builder (Braun on-the-fly
//! SSA, peephole-at-construction) wraps it.

use crate::entity::{EntityList, EntityRef, ListPool, Mark, PrimaryMap, entity_id};
use crate::facts::{FactData, FactId};
use crate::ops::{FloatCc, IntCc, Opcode, TrapKind};
use crate::types::{TypeId, TypeInterner};

entity_id!(
    /// A function in a module.
    pub struct FuncId, "fn"
);
entity_id!(
    /// A basic block within a function.
    pub struct Block, "b"
);
entity_id!(
    /// An instruction within a function.
    pub struct Inst, "inst"
);
entity_id!(
    /// An SSA value: a block parameter or an instruction result.
    pub struct Value, "%"
);
entity_id!(
    /// A signature (interned per module).
    pub struct SigId, "sig"
);
entity_id!(
    /// A callee referenced by this function (name + signature).
    pub struct ExtFunc, "f"
);

/// Parameter passing mode — the language's call-site modes, carried by
/// every WIR signature so backends and the verifier see them (c04's
/// theorems about `mut`/`read`/`take` params attach here).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Plain by-value parameter (no keyword in text).
    Val,
    /// `mut` — exclusive for the call: noalias + dereferenceable.
    Mut,
    /// `read` — immutable for the call: Frozen.
    Read,
    /// `take` — ownership transfers to the callee.
    Take,
}

impl Mode {
    pub fn keyword(self) -> Option<&'static str> {
        match self {
            Mode::Val => None,
            Mode::Mut => Some("mut"),
            Mode::Read => Some("read"),
            Mode::Take => Some("take"),
        }
    }
}

/// One signature parameter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Param {
    pub ty: TypeId,
    pub mode: Mode,
}

impl Param {
    pub fn val(ty: TypeId) -> Param {
        Param {
            ty,
            mode: Mode::Val,
        }
    }
}

/// A function signature: mode-carrying params (token params are
/// ordinary params of `mem.rN`/`io` type) and result types. Calls
/// consume their token args and mint one successor token per token
/// param, appended after the declared results in param order.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SigData {
    pub params: Vec<Param>,
    pub results: Vec<TypeId>,
}

/// A callee: resolved by name at link/lower time; the signature is the
/// contract the verifier checks call sites against.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExtFuncData {
    pub name: String,
    pub sig: SigId,
}

/// Non-operand instruction payload. Small and closed: everything else
/// about an instruction is its opcode, operands, and result values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Aux {
    None,
    /// `iconst` payload (sign-extended to i64) / `agg.get` field index.
    Int(i64),
    /// `fconst` payload as raw IEEE-754 bits (byte-stable dumps).
    FloatBits(u64),
    /// `bconst` payload.
    Bool(bool),
    /// `icmp` condition.
    IntCc(IntCc),
    /// `fcmp` condition.
    FloatCc(FloatCc),
    /// `ptr.off` element size in bytes.
    Scale(u64),
    /// `call` callee.
    Callee(ExtFunc),
    /// `jmp` target.
    Jump(BlockCall),
    /// `br` targets (then, else).
    Br(BlockCall, BlockCall),
    /// `trap` kind — which check this trap reports (s28; `Assert`
    /// prints as bare `trap`, the pre-s28 form).
    Trap(TrapKind),
}

/// A branch edge: target block plus the arguments passed to its params.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BlockCall {
    pub block: Block,
    pub args: EntityList<Value>,
}

/// An instruction: opcode + operand list + results + auxiliary payload.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InstData {
    pub op: Opcode,
    /// Value operands (branch-edge arguments live in `aux`, not here).
    pub args: EntityList<Value>,
    /// Result values, in order. Terminators have none; most ops one;
    /// `call` may have several (declared results + successor tokens).
    pub results: EntityList<Value>,
    pub aux: Aux,
}

/// A source span attached to an instruction (s30): byte offsets into
/// the function's source file ([`Function::src_file`]). Debug aux, NOT
/// part of the canonical textual format — like fact spans and tag
/// names, spans do not survive print → parse (the D8 hash input is the
/// printed form). Dropping them is always sound; line tables (s30
/// DWARF) consume them through the backend's `DebugSink`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SrcSpan {
    pub lo: u32,
    pub hi: u32,
}

/// One named source binding's SSA definition (s30 debug aux): `name`
/// currently holds `val`. A name may appear many times — once per
/// (re)binding or assignment — and one value may carry several names
/// (GVN dedup). Like [`SrcSpan`]s, these are non-canonical: the
/// printer ignores them, backends read them to emit variable DIEs.
#[derive(Clone, Debug)]
pub struct DebugVarData {
    pub name: String,
    pub val: Value,
    /// True for function parameters (DW_TAG_formal_parameter).
    pub param: bool,
}

/// Where a value comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueDef {
    /// Result `index` of an instruction.
    Result(Inst, u16),
    /// Parameter `index` of a block.
    Param(Block, u16),
}

/// A value: its type and its def site.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ValueData {
    pub ty: TypeId,
    pub def: ValueDef,
}

/// A basic block: typed parameters plus its instructions in order.
#[derive(Clone, Debug, Default)]
pub struct BlockData {
    pub params: EntityList<Value>,
    pub insts: Vec<Inst>,
}

/// A rollback point across all of a function's arenas (LIFO delete;
/// s25 speculative construction).
#[derive(Clone, Copy, Debug)]
pub struct FuncMark {
    blocks: Mark,
    insts: Mark,
    values: Mark,
    facts: Mark,
    ext_funcs: Mark,
    vpool: Mark,
    layout_len: u32,
}

/// A function: arenas + side tables + block layout order. `layout[0]`
/// is the entry block; its parameters are the function's parameters
/// and must match the signature.
#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub sig: SigId,
    /// C membrane export (s29, D19): the function is defined under the
    /// C convention with an UNMANGLED linker symbol (its WIR name) and
    /// external linkage. Textually `export fn @name…`. Everything else
    /// stays `wolf-abi-0` internal.
    pub export: bool,
    pub blocks: PrimaryMap<Block, BlockData>,
    pub insts: PrimaryMap<Inst, InstData>,
    pub values: PrimaryMap<Value, ValueData>,
    pub facts: PrimaryMap<FactId, FactData>,
    pub ext_funcs: PrimaryMap<ExtFunc, ExtFuncData>,
    /// Block order for printing/canonical numbering. Entry first.
    pub layout: Vec<Block>,
    /// The one packed pool for every value list in this function
    /// (operands, results, block params, branch args).
    pub vpool: ListPool<Value>,
    /// s30 debug aux: the source file (a `wolf_span::FileId` index)
    /// this function's spans point into. `None` for synthetic
    /// functions (entry shims, hand-built fixtures) — no line table.
    pub src_file: Option<u32>,
    /// s30 debug aux: the span every subsequently appended instruction
    /// records ([`Function::append_inst`] reads it). Lowering sets it
    /// per statement; synthesized code (defer chains, trap checks)
    /// inherits the enclosing statement's span.
    pub span_cursor: Option<SrcSpan>,
    /// Per-instruction source spans, parallel to `insts` (same length
    /// always; rolled back together). Non-canonical (see [`SrcSpan`]).
    srcspans: Vec<Option<SrcSpan>>,
    /// Named-binding definitions for variable DIEs (s30). Rolled back
    /// with the value arena; rewritten by the builder's value
    /// replacement. Non-canonical.
    pub debug_vars: Vec<DebugVarData>,
}

impl Function {
    pub fn new(name: impl Into<String>, sig: SigId) -> Function {
        Function {
            name: name.into(),
            sig,
            export: false,
            blocks: PrimaryMap::new(),
            insts: PrimaryMap::new(),
            values: PrimaryMap::new(),
            facts: PrimaryMap::new(),
            ext_funcs: PrimaryMap::new(),
            layout: Vec::new(),
            vpool: ListPool::new(),
            src_file: None,
            span_cursor: None,
            srcspans: Vec::new(),
            debug_vars: Vec::new(),
        }
    }

    /// The source span recorded for `inst`, if any (s30 debug aux).
    pub fn srcspan(&self, inst: Inst) -> Option<SrcSpan> {
        self.srcspans.get(inst.index()).copied().flatten()
    }

    /// Record a named binding's current SSA definition (s30 debug
    /// aux; see [`DebugVarData`]).
    pub fn add_debug_var(&mut self, name: impl Into<String>, val: Value, param: bool) {
        self.debug_vars.push(DebugVarData {
            name: name.into(),
            val,
            param,
        });
    }

    pub fn entry(&self) -> Option<Block> {
        self.layout.first().copied()
    }

    /// Create a block with the given parameter types, append it to the
    /// layout, and mint its parameter values.
    pub fn make_block(&mut self, param_tys: &[TypeId]) -> Block {
        let block = self.blocks.next_key();
        let mut params = Vec::with_capacity(param_tys.len());
        for (i, &ty) in param_tys.iter().enumerate() {
            params.push(self.values.push(ValueData {
                ty,
                def: ValueDef::Param(block, i as u16),
            }));
        }
        let params = self.vpool.intern(&params);
        let b = self.blocks.push(BlockData {
            params,
            insts: Vec::new(),
        });
        debug_assert_eq!(b, block);
        self.layout.push(block);
        block
    }

    pub fn block_params(&self, block: Block) -> Vec<Value> {
        self.vpool.get(self.blocks[block].params)
    }

    /// Append an instruction to a block, minting one result value per
    /// entry in `result_tys`. Returns the instruction and its results.
    pub fn append_inst(
        &mut self,
        block: Block,
        op: Opcode,
        args: &[Value],
        result_tys: &[TypeId],
        aux: Aux,
    ) -> (Inst, Vec<Value>) {
        let inst = self.insts.next_key();
        let mut results = Vec::with_capacity(result_tys.len());
        for (i, &ty) in result_tys.iter().enumerate() {
            results.push(self.values.push(ValueData {
                ty,
                def: ValueDef::Result(inst, i as u16),
            }));
        }
        let args = self.vpool.intern(args);
        let rlist = self.vpool.intern(&results);
        let i = self.insts.push(InstData {
            op,
            args,
            results: rlist,
            aux,
        });
        debug_assert_eq!(i, inst);
        self.srcspans.push(self.span_cursor);
        self.blocks[block].insts.push(inst);
        (inst, results)
    }

    /// Declare a callee for use by `call` instructions in this function.
    pub fn import_func(&mut self, name: impl Into<String>, sig: SigId) -> ExtFunc {
        self.ext_funcs.push(ExtFuncData {
            name: name.into(),
            sig,
        })
    }

    /// Attach a fact. Facts are entities: printed, hashed, verified —
    /// never droppable metadata (D2).
    pub fn add_fact(&mut self, fact: FactData) -> FactId {
        self.facts.push(fact)
    }

    /// Intern a branch edge for `Aux::Jump`/`Aux::Br`.
    pub fn block_call(&mut self, block: Block, args: &[Value]) -> BlockCall {
        BlockCall {
            block,
            args: self.vpool.intern(args),
        }
    }

    /// The type of a value.
    pub fn value_ty(&self, v: Value) -> TypeId {
        self.values[v].ty
    }

    // ---- LIFO delete (truncate-to-mark; Click §5, for s25) -------------

    /// Snapshot every arena length.
    pub fn mark(&self) -> FuncMark {
        FuncMark {
            blocks: self.blocks.mark(),
            insts: self.insts.mark(),
            values: self.values.mark(),
            facts: self.facts.mark(),
            ext_funcs: self.ext_funcs.mark(),
            vpool: self.vpool.mark(),
            layout_len: self.layout.len() as u32,
        }
    }

    /// Roll every arena back to `mark`, restoring table state exactly:
    /// entities minted since the mark vanish, including instructions
    /// appended to blocks that predate it.
    pub fn truncate_to_mark(&mut self, mark: FuncMark) {
        let first_dead_inst = Inst::new(mark.insts.0);
        self.blocks.truncate_to(mark.blocks);
        for block in self.blocks.keys().collect::<Vec<_>>() {
            let insts = &mut self.blocks[block].insts;
            insts.retain(|&i| i < first_dead_inst);
        }
        self.insts.truncate_to(mark.insts);
        self.srcspans.truncate(mark.insts.0 as usize);
        let first_dead_value = Value::new(mark.values.0);
        self.debug_vars.retain(|dv| dv.val < first_dead_value);
        self.values.truncate_to(mark.values);
        self.facts.truncate_to(mark.facts);
        self.ext_funcs.truncate_to(mark.ext_funcs);
        self.vpool.truncate_to(mark.vpool);
        self.layout.truncate(mark.layout_len as usize);
    }
}

/// A module: the type interner, interned signatures, external decls,
/// and functions in definition order.
#[derive(Clone, Debug)]
pub struct Module {
    pub types: TypeInterner,
    pub sigs: PrimaryMap<SigId, SigData>,
    /// External declarations (`decl @name(...)`), in declaration order.
    /// The canonical printer emits the sorted union of these and every
    /// callee any function imports.
    pub decls: Vec<(String, SigId)>,
    pub funcs: PrimaryMap<FuncId, Function>,
    /// Module-interned error-tag names (s27): tag id = index + 1
    /// (id 0 is the ok discriminant, never a tag). GLOBAL interning is
    /// what makes row widening the identity on tag values — D30's
    /// "injection re-tagging" costs nothing. Interned in lowering
    /// visit order (deterministic); compiler-internal, so the textual
    /// format carries only the `i64` ids and round-trip does not
    /// preserve the names (like fact spans).
    pub tags: Vec<String>,
}

impl Default for Module {
    fn default() -> Self {
        Self::new()
    }
}

impl Module {
    pub fn new() -> Module {
        Module {
            types: TypeInterner::new(),
            sigs: PrimaryMap::new(),
            decls: Vec::new(),
            funcs: PrimaryMap::new(),
            tags: Vec::new(),
        }
    }

    /// Intern an error-tag name; ids start at 1 (0 = ok).
    pub fn tag_id(&mut self, name: &str) -> i64 {
        if let Some(i) = self.tags.iter().position(|t| t == name) {
            return (i + 1) as i64;
        }
        self.tags.push(name.to_string());
        self.tags.len() as i64
    }

    /// Record an external declaration.
    pub fn add_decl(&mut self, name: impl Into<String>, sig: SigId) {
        self.decls.push((name.into(), sig));
    }

    pub fn make_sig(&mut self, params: Vec<Param>, results: Vec<TypeId>) -> SigId {
        self.sigs.push(SigData { params, results })
    }

    pub fn add_func(&mut self, func: Function) -> FuncId {
        self.funcs.push(func)
    }
}
