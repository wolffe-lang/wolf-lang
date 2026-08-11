//! WIR function → CLIF translation (s28).
//!
//! The easy 80% is structural: WIR and CLIF are both phi-free
//! block-parameter SSA (same lineage), so blocks, params and branches
//! map 1:1 for scalars. The engineered 20%:
//!
//! - **Tokens erase to program order** — a `mem.rN`/`io`-typed value
//!   becomes nothing; instruction order within a block IS the memory
//!   order Cranelift sees.
//! - **Checked arithmetic** (X3) lowers to explicit overflow checks
//!   branching to per-kind trap blocks that CALL `__wolf_rt_trap`, so
//!   the trap identity survives to the process boundary (verdict
//!   `trap(overflow)`, not "SIGILL somewhere"). Narrow widths compute
//!   widened in i64 and range-check; i64 uses Cranelift's native
//!   `*_overflow` ops.
//! - **Aggregates and error unions** have no CLIF type: every
//!   `{…}`/`eu{…}` value is an explicit stack slot addressed via
//!   `wolf_backend::layout`. Aggregate-typed BLOCK PARAMETERS own a
//!   slot; every entering edge copies into it (loads before stores, so
//!   swapping edges can't clobber); `br` edges with aggregate args go
//!   through per-edge trampoline blocks.
//! - **Call boundaries execute `wolf_backend::abi` plans** (s29):
//!   classification lives THERE, this file is mechanical. Small
//!   aggregates scalarize into register units; MEMORY-class values go
//!   by pointer (wolf) or byval/sret (C membrane); `c.*` callees are
//!   unmangled libc imports under the SysV plan; error-union returns
//!   keep the discriminant in the first INTEGER return register.
//! - **Region ops** call the `wolf_rt` shims; `stack.alloc` is a real
//!   stack slot.

use std::collections::HashMap;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    AbiParam, BlockArg, InstBuilder, MemFlagsData, Signature, SourceLoc, StackSlot, StackSlotData,
    StackSlotKind, TrapCode, Value as CValue, types as ctypes,
};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module as _;
use cranelift_object::ObjectModule;

use wolf_backend::abi::{self, Conv, ParamPass, RegClass, RetPass, Unit};
use wolf_backend::layout::{self, Layout};
use wolf_backend::{BackendError, DebugTy};
use wolf_wir::ir::{Aux, Block as WBlock, Function as WFunction, Module as WModule, SigId};
use wolf_wir::ops::{FloatCc, IntCc, Opcode, TrapKind};
use wolf_wir::types::{TypeData, TypeId};

/// The runtime symbols compiled code may reference (the s28/s29 symbol
/// contract with `wolf_rt`): name, param count, has-result.
/// `__wolf_rt_main_err` is the eu-main exit path (s29): tag id, name
/// length, and the tag's name bytes packed little-endian into four
/// i64 words (32-byte cap) — reports `error: <name>` on stdout and
/// exits 1, the documented D30 process behavior for a `main` that
/// returns an error value.
pub const RT_SYMBOLS: [(&str, usize, bool); 6] = [
    ("__wolf_rt_trap", 1, false),
    ("__wolf_rt_region_new", 0, true),
    ("__wolf_rt_region_alloc", 2, true),
    ("__wolf_rt_region_free", 1, false),
    ("__wolf_rt_region_freeze", 1, false),
    ("__wolf_rt_main_err", 6, false),
];

/// How one WIR parameter crosses the call boundary (the mechanical
/// execution of [`wolf_backend::abi::ParamPass`] — classification
/// lives THERE; this enum only adds what plan execution needs).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Slot {
    /// Passed directly as one CLIF value.
    Direct(cranelift_codegen::ir::Type),
    /// Aggregate scalarized into ≤ 2 eightbyte register units.
    Split(Vec<Unit>),
    /// Aggregate by pointer (wolf-native MEMORY class).
    Ref,
    /// Aggregate byval on the stack (C MEMORY class), by size.
    CStruct(u32),
    /// Effect token: erased, crosses as nothing.
    Token,
}

/// How one WIR result crosses the call boundary.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum RSlot {
    /// Returned directly as one CLIF value.
    Direct(cranelift_codegen::ir::Type),
    /// Aggregate in ≤ 2 return registers (eu: tag is unit 0).
    Split(Vec<Unit>),
    /// Caller-allocated out-slot of `size` bytes. Wolf: plain pointer
    /// param (trailing), plus an `i64` discriminant return when
    /// `disc_in_reg` (big error unions). C: a true psABI sret param
    /// (first; Cranelift returns the pointer in `%rax` itself).
    Sret {
        disc_in_reg: bool,
        c: bool,
        size: u32,
    },
    /// Effect token: erased.
    Token,
}

pub(crate) struct SigInfo {
    pub params: Vec<Slot>,
    pub rets: Vec<RSlot>,
    pub clif: Signature,
    /// The out-slot pointer params sit FIRST in the CLIF signature
    /// (C convention) rather than trailing (wolf-native).
    pub sret_first: bool,
}

fn scalar_clif_ty(m: &WModule, ty: TypeId) -> Option<cranelift_codegen::ir::Type> {
    Some(match m.types.get(ty) {
        TypeData::I8 | TypeData::Bool => ctypes::I8,
        TypeData::I16 => ctypes::I16,
        TypeData::I32 => ctypes::I32,
        TypeData::I64 | TypeData::Ptr => ctypes::I64,
        TypeData::F32 => ctypes::F32,
        TypeData::F64 => ctypes::F64,
        TypeData::Mem(_) | TypeData::Io | TypeData::Agg(_) | TypeData::Eu { .. } => return None,
    })
}

fn is_agg(m: &WModule, ty: TypeId) -> bool {
    matches!(m.types.get(ty), TypeData::Agg(_) | TypeData::Eu { .. })
}

/// The CLIF value type carrying one eightbyte unit: INTEGER units ride
/// `i64` (partial eightbytes are chunk-assembled), SSE units ride
/// `f32`/`f64` by covered width.
fn unit_clif_ty(u: &Unit) -> cranelift_codegen::ir::Type {
    match u.class {
        RegClass::Int => ctypes::I64,
        RegClass::Sse => {
            if u.bytes <= 4 {
                ctypes::F32
            } else {
                ctypes::F64
            }
        }
    }
}

/// Build the CLIF signature + slot map for one WIR signature under one
/// convention, executing the shared plan from [`wolf_backend::abi`]
/// (where ALL classification lives — this function is mechanical).
pub(crate) fn sig_info(m: &WModule, sig: SigId, conv: Conv, cc: CallConv) -> SigInfo {
    let data = &m.sigs[sig];
    let plan = abi::plan_sig(&m.types, data, conv);
    let mut clif = Signature::new(cc);
    let sret_first = conv == Conv::C;
    let mut rets = Vec::with_capacity(data.results.len());
    for (pass, &r) in plan.rets.iter().zip(data.results.iter()) {
        match pass {
            RetPass::Token => rets.push(RSlot::Token),
            RetPass::Direct(ty) => {
                let t = scalar_clif_ty(m, *ty).expect("scalar result");
                clif.returns.push(AbiParam::new(t));
                rets.push(RSlot::Direct(t));
            }
            RetPass::Split(units) => {
                for u in units {
                    clif.returns.push(AbiParam::new(unit_clif_ty(u)));
                }
                rets.push(RSlot::Split(units.clone()));
            }
            RetPass::Sret { disc_in_reg } => {
                let size = layout::layout_of(&m.types, r).expect("sized result").size;
                if sret_first {
                    // psABI sret: pointer first (%rdi); Cranelift
                    // returns it in %rax itself — no explicit return.
                    clif.params.push(AbiParam::special(
                        ctypes::I64,
                        cranelift_codegen::ir::ArgumentPurpose::StructReturn,
                    ));
                } else if *disc_in_reg {
                    // Big error union: the discriminant STILL rides
                    // the first INTEGER return register ([abi.err]).
                    clif.returns.push(AbiParam::new(ctypes::I64));
                }
                rets.push(RSlot::Sret {
                    disc_in_reg: *disc_in_reg,
                    c: sret_first,
                    size,
                });
            }
        }
    }
    let mut params = Vec::with_capacity(data.params.len());
    for (pass, p) in plan.params.iter().zip(data.params.iter()) {
        match pass {
            ParamPass::Token => params.push(Slot::Token),
            ParamPass::Direct(ty) => {
                let t = scalar_clif_ty(m, *ty).expect("scalar param");
                clif.params.push(AbiParam::new(t));
                params.push(Slot::Direct(t));
            }
            ParamPass::Split(units) => {
                for u in units {
                    clif.params.push(AbiParam::new(unit_clif_ty(u)));
                }
                params.push(Slot::Split(units.clone()));
            }
            ParamPass::Memory => match conv {
                Conv::Wolf => {
                    clif.params.push(AbiParam::new(ctypes::I64));
                    params.push(Slot::Ref);
                }
                Conv::C => {
                    let size = layout::layout_of(&m.types, p.ty).expect("sized param").size;
                    clif.params.push(AbiParam::special(
                        ctypes::I64,
                        cranelift_codegen::ir::ArgumentPurpose::StructArgument(
                            size.next_multiple_of(8),
                        ),
                    ));
                    params.push(Slot::CStruct(size));
                }
            },
        }
    }
    // Wolf-native out-slot pointers trail the declared params, in
    // result order (a PLAIN pointer param — no C sret obligations).
    if !sret_first {
        for r in &rets {
            if matches!(r, RSlot::Sret { .. }) {
                clif.params.push(AbiParam::new(ctypes::I64));
            }
        }
    }
    SigInfo {
        params,
        rets,
        clif,
        sret_first,
    }
}

/// How a WIR value exists in CLIF.
#[derive(Clone, Copy, Debug)]
enum Repr {
    /// One CLIF value (ints, floats, bools as i8, ptrs as i64).
    Scalar(CValue),
    /// An aggregate/error-union: the address of its bytes.
    Addr(CValue),
    /// An effect token: nothing.
    Token,
}

struct Tx<'a, 'b> {
    b: FunctionBuilder<'b>,
    m: &'a WModule,
    f: &'a WFunction,
    /// The convention THIS function is defined under (wolf-native for
    /// module functions; C for `export`ed membranes).
    conv: Conv,
    om: &'a mut ObjectModule,
    funcs: &'a HashMap<String, FuncEntry>,
    rt: &'a mut HashMap<&'static str, cranelift_module::FuncId>,
    /// C membrane imports declared so far (`c.*` callee → func id).
    imports: &'a mut HashMap<String, cranelift_module::FuncId>,
    blocks: HashMap<WBlock, cranelift_codegen::ir::Block>,
    vals: HashMap<wolf_wir::ir::Value, Repr>,
    /// Slots owned by aggregate-typed block params of non-entry blocks.
    param_slots: HashMap<(WBlock, usize), cranelift_codegen::ir::StackSlot>,
    /// sret addresses (entry params), in aggregate-result order.
    sret: Vec<CValue>,
    /// Lazily created per-kind trap blocks.
    trap_blocks: HashMap<i32, cranelift_codegen::ir::Block>,
    /// FuncRefs imported into this function, by callee name, with the
    /// convention their calls lower under.
    fref_cache: HashMap<String, (cranelift_codegen::ir::FuncRef, Conv)>,
    /// s30 debug: WIR value → indices into `debug_slots` (a value may
    /// carry several names — GVN dedup; a name may have many values —
    /// rebindings, all stored to ONE slot).
    debug_names: HashMap<wolf_wir::ir::Value, Vec<usize>>,
    /// s30 debug: one frame slot per named scalar binding —
    /// (name, scalar shape, slot, is-param). The debug tier forces
    /// named locals addressable (s28 decision) so `DW_OP_fbreg`
    /// locations are whole-function truths.
    debug_slots: Vec<(String, DebugTy, StackSlot, bool)>,
}

/// One declared function the translator can call.
pub(crate) struct FuncEntry {
    pub fid: cranelift_module::FuncId,
    pub sig: SigId,
}

fn ice(msg: impl Into<String>) -> BackendError {
    BackendError::Internal(msg.into())
}

fn nyi(msg: impl Into<String>) -> BackendError {
    BackendError::Unsupported(msg.into())
}

/// Translate one function. Returns the named-variable frame slots the
/// debug tier spilled (s30): the caller maps them to frame-pointer-
/// relative offsets once the frame layout is known.
#[allow(clippy::too_many_arguments)]
pub(crate) fn translate_function(
    b: FunctionBuilder<'_>,
    m: &WModule,
    f: &WFunction,
    conv: Conv,
    si: &SigInfo,
    om: &mut ObjectModule,
    funcs: &HashMap<String, FuncEntry>,
    rt: &mut HashMap<&'static str, cranelift_module::FuncId>,
    imports: &mut HashMap<String, cranelift_module::FuncId>,
) -> Result<Vec<(String, DebugTy, StackSlot, bool)>, BackendError> {
    let mut tx = Tx {
        b,
        m,
        f,
        conv,
        om,
        funcs,
        rt,
        imports,
        blocks: HashMap::new(),
        vals: HashMap::new(),
        param_slots: HashMap::new(),
        sret: Vec::new(),
        trap_blocks: HashMap::new(),
        fref_cache: HashMap::new(),
        debug_names: HashMap::new(),
        debug_slots: Vec::new(),
    };
    tx.prepare_debug_vars();
    tx.run(si)?;
    let cfg = tx.om.isa().frontend_config();
    tx.b.finalize(cfg);
    Ok(tx.debug_slots)
}

impl<'a, 'b> Tx<'a, 'b> {
    /// Build the named-binding tables (s30): one frame slot per named
    /// SCALAR binding (aggregates already live in addressable slots;
    /// their DIEs are s31+), shared by every rebinding of the name.
    /// Shadowing with a DIFFERENT scalar shape keeps the first
    /// binding's slot and shape — later shadows of another type are
    /// not stored (v0 honesty: one DIE per name).
    fn prepare_debug_vars(&mut self) {
        let mut by_name: HashMap<&str, usize> = HashMap::new();
        for dv in &self.f.debug_vars {
            let ty = self.f.value_ty(dv.val);
            let Some(dty) = debug_ty(self.m, ty) else {
                continue; // aggregate/token-shaped: no scalar slot at v0
            };
            let idx = match by_name.get(dv.name.as_str()) {
                Some(&i) => {
                    if self.debug_slots[i].1 != dty {
                        continue; // shadow of a different shape
                    }
                    if dv.param {
                        self.debug_slots[i].3 = true;
                    }
                    i
                }
                None => {
                    let slot = self.b.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    self.debug_slots
                        .push((dv.name.clone(), dty, slot, dv.param));
                    by_name.insert(dv.name.as_str(), self.debug_slots.len() - 1);
                    self.debug_slots.len() - 1
                }
            };
            self.debug_names.entry(dv.val).or_default().push(idx);
        }
    }

    /// If `wv` is a named scalar binding, store its value into the
    /// name's frame slot at the current point (the definition site) —
    /// the debug tier's forced spill.
    fn debug_spill(&mut self, wv: wolf_wir::ir::Value) {
        let Some(idxs) = self.debug_names.get(&wv).cloned() else {
            return;
        };
        let Some(&Repr::Scalar(cv)) = self.vals.get(&wv) else {
            return;
        };
        for i in idxs {
            let slot = self.debug_slots[i].2;
            let addr = self.b.ins().stack_addr(ctypes::I64, slot, 0);
            self.b.ins().store(MemFlagsData::trusted(), cv, addr, 0);
        }
    }

    fn run(&mut self, si: &SigInfo) -> Result<(), BackendError> {
        let Some(entry) = self.f.entry() else {
            return Err(ice("function has no entry block"));
        };
        // 1. Create every CLIF block; give non-entry blocks their
        // scalar params (aggregate params own slots instead).
        for &wb in &self.f.layout {
            let cb = self.b.create_block();
            self.blocks.insert(wb, cb);
        }
        let entry_cb = self.blocks[&entry];
        // Entry: CLIF params mirror the signature.
        self.b.switch_to_block(entry_cb);
        for abi in &si.clif.params {
            self.b.append_block_param(entry_cb, abi.value_type);
        }
        let cparams: Vec<CValue> = self.b.block_params(entry_cb).to_vec();
        let n_sret = si
            .rets
            .iter()
            .filter(|r| matches!(r, RSlot::Sret { .. }))
            .count();
        // C convention: out-slot pointers sit FIRST; wolf trails them.
        let mut next = if si.sret_first { n_sret } else { 0 };
        let wparams = self.f.block_params(entry);
        if wparams.len() != si.params.len() {
            return Err(ice("entry params do not match the signature"));
        }
        for (i, &wv) in wparams.iter().enumerate() {
            match &si.params[i] {
                Slot::Token => {
                    self.vals.insert(wv, Repr::Token);
                }
                Slot::Direct(_) => {
                    self.vals.insert(wv, Repr::Scalar(cparams[next]));
                    next += 1;
                }
                Slot::Split(units) => {
                    // Materialize the register units into a local slot
                    // (rounded to whole eightbytes — the store-side
                    // invariant of [`Tx::store_unit`]).
                    let units = units.clone();
                    let addr = self.units_slot_addr(&units);
                    for u in &units {
                        let v = cparams[next];
                        next += 1;
                        self.store_unit(addr, u, v);
                    }
                    self.vals.insert(wv, Repr::Addr(addr));
                }
                Slot::Ref | Slot::CStruct(_) => {
                    self.vals.insert(wv, Repr::Addr(cparams[next]));
                    next += 1;
                }
            }
        }
        let sret_base = if si.sret_first { 0 } else { next };
        for k in 0..n_sret {
            self.sret.push(cparams[sret_base + k]);
        }
        // Named parameters spill to their debug slots in the prologue
        // (s30): readable the moment a `break main`-style breakpoint
        // lands (prologue_end marks the first user row).
        for &wv in &wparams {
            self.debug_spill(wv);
        }
        // Non-entry blocks: scalar params become CLIF block params;
        // aggregate params own a stack slot filled by entering edges.
        for &wb in &self.f.layout {
            if wb == entry {
                continue;
            }
            let cb = self.blocks[&wb];
            for (i, wv) in self.f.block_params(wb).into_iter().enumerate() {
                let ty = self.f.value_ty(wv);
                if self.m.types.is_token(ty) {
                    self.vals.insert(wv, Repr::Token);
                } else if is_agg(self.m, ty) {
                    let slot = self.new_slot(self.layout_of(ty)?);
                    self.param_slots.insert((wb, i), slot);
                    // The address is minted at block entry below.
                } else {
                    let cty = scalar_clif_ty(self.m, ty).expect("scalar");
                    let cv = self.b.append_block_param(cb, cty);
                    self.vals.insert(wv, Repr::Scalar(cv));
                }
            }
        }
        // 2. Translate every block. The entry is first in layout and
        // the builder is ALREADY positioned there (step 1 may have
        // emitted param-materialization stores into it — switching,
        // even to the same block, would trip the fill-before-switch
        // rule).
        for bi in 0..self.f.layout.len() {
            let wb = self.f.layout[bi];
            let cb = self.blocks[&wb];
            if wb != entry {
                self.b.switch_to_block(cb);
            }
            // Aggregate block params: bind their slot addresses. Named
            // scalar params (merged rebindings, loop variables) re-
            // spill to their debug slots at block entry (s30).
            for (i, wv) in self.f.block_params(wb).into_iter().enumerate() {
                if let Some(&slot) = self.param_slots.get(&(wb, i)) {
                    let addr = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                    self.vals.insert(wv, Repr::Addr(addr));
                }
                if wb != entry {
                    self.debug_spill(wv);
                }
            }
            for ii in 0..self.f.blocks[wb].insts.len() {
                let inst = self.f.blocks[wb].insts[ii];
                // s30: the WIR span rides into the machine buffer as a
                // Cranelift srcloc (bits = span lo; the byte-exact s07
                // chain ends in .debug_line).
                let loc = match self.f.srcspan(inst) {
                    Some(sp) => SourceLoc::new(sp.lo),
                    None => SourceLoc::default(),
                };
                self.b.set_srcloc(loc);
                self.inst(inst)?;
                for rv in self.results_of(inst) {
                    self.debug_spill(rv);
                }
            }
        }
        self.fill_trap_blocks()?;
        self.b.seal_all_blocks();
        Ok(())
    }

    // ---- small helpers ---------------------------------------------------

    fn layout_of(&self, ty: TypeId) -> Result<Layout, BackendError> {
        layout::layout_of(&self.m.types, ty).ok_or_else(|| ice("layout of a token type"))
    }

    fn new_slot(&mut self, l: Layout) -> cranelift_codegen::ir::StackSlot {
        let align_shift = l.align.trailing_zeros() as u8;
        self.b.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            l.size.max(1),
            align_shift,
        ))
    }

    fn scalar(&self, v: wolf_wir::ir::Value) -> Result<CValue, BackendError> {
        match self.vals.get(&v) {
            Some(Repr::Scalar(c)) => Ok(*c),
            other => Err(ice(format!("expected scalar repr, found {other:?}"))),
        }
    }

    fn addr(&self, v: wolf_wir::ir::Value) -> Result<CValue, BackendError> {
        match self.vals.get(&v) {
            Some(Repr::Addr(c)) => Ok(*c),
            other => Err(ice(format!("expected aggregate repr, found {other:?}"))),
        }
    }

    fn args_of(&self, inst: wolf_wir::ir::Inst) -> Vec<wolf_wir::ir::Value> {
        self.f.vpool.get(self.f.insts[inst].args)
    }

    fn results_of(&self, inst: wolf_wir::ir::Inst) -> Vec<wolf_wir::ir::Value> {
        self.f.vpool.get(self.f.insts[inst].results)
    }

    /// The (empty, cold) trap block for one kind. Bodies are emitted
    /// at the end of translation ([`Tx::fill_trap_blocks`]) — the
    /// frontend forbids switching away from an unfilled block, so they
    /// cannot be filled mid-instruction.
    fn trap_block(&mut self, code: i32) -> cranelift_codegen::ir::Block {
        if let Some(&tb) = self.trap_blocks.get(&code) {
            return tb;
        }
        let tb = self.b.create_block();
        self.b.set_cold_block(tb);
        self.trap_blocks.insert(code, tb);
        tb
    }

    /// Emit every pending trap block's body: call
    /// `__wolf_rt_trap(code)` (reports the kind, never returns), then
    /// a CLIF trap for the unreachable tail.
    fn fill_trap_blocks(&mut self) -> Result<(), BackendError> {
        for (code, tb) in std::mem::take(&mut self.trap_blocks) {
            self.b.switch_to_block(tb);
            let fref = self.rt_ref("__wolf_rt_trap")?;
            let c = self.b.ins().iconst(ctypes::I32, code as i64);
            self.b.ins().call(fref, &[c]);
            self.b
                .ins()
                .trap(TrapCode::user(code as u8).ok_or_else(|| ice("trap code 0"))?);
        }
        Ok(())
    }

    /// Branch to the `kind` trap block when `cond` is nonzero, and
    /// continue in a fresh block otherwise.
    fn trap_if(&mut self, cond: CValue, kind: TrapKind) -> Result<(), BackendError> {
        let code = trap_code(kind);
        let tb = self.trap_block(code);
        let cont = self.b.create_block();
        let empty: [BlockArg; 0] = [];
        self.b
            .ins()
            .brif(cond, tb, empty.iter(), cont, empty.iter());
        self.b.switch_to_block(cont);
        Ok(())
    }

    /// Declare (once) and import (per function) a runtime shim.
    fn rt_ref(
        &mut self,
        name: &'static str,
    ) -> Result<cranelift_codegen::ir::FuncRef, BackendError> {
        if let Some(&(fr, _)) = self.fref_cache.get(name) {
            return Ok(fr);
        }
        let fid = match self.rt.get(name) {
            Some(&fid) => fid,
            None => {
                let (_, nparams, has_ret) = RT_SYMBOLS
                    .iter()
                    .find(|(n, _, _)| *n == name)
                    .copied()
                    .ok_or_else(|| ice(format!("unknown runtime symbol {name}")))?;
                let mut sig = Signature::new(self.om.isa().default_call_conv());
                for i in 0..nparams {
                    // The trap code is i32; every other shim param is a
                    // pointer/size i64.
                    let ty = if name == "__wolf_rt_trap" && i == 0 {
                        ctypes::I32
                    } else {
                        ctypes::I64
                    };
                    sig.params.push(AbiParam::new(ty));
                }
                if has_ret {
                    sig.returns.push(AbiParam::new(ctypes::I64));
                }
                let fid = self
                    .om
                    .declare_function(name, cranelift_module::Linkage::Import, &sig)
                    .map_err(|e| ice(e.to_string()))?;
                self.rt.insert(name, fid);
                fid
            }
        };
        let fr = self.om.declare_func_in_func(fid, self.b.func);
        self.fref_cache.insert(name.to_string(), (fr, Conv::Wolf));
        Ok(fr)
    }

    /// A fresh slot big enough to hold `units` whole eightbytes (the
    /// [`Tx::store_unit`] invariant: full-width unit stores are always
    /// in bounds), returning its address.
    fn units_slot_addr(&mut self, units: &[Unit]) -> CValue {
        let slot = self.new_slot(Layout {
            size: (units.len() as u32).max(1) * 8,
            align: 8,
        });
        self.b.ins().stack_addr(ctypes::I64, slot, 0)
    }

    /// Store one register unit into `base + u.offset`. INTEGER units
    /// store the full eightbyte (callers guarantee a rounded slot —
    /// [`Tx::units_slot_addr`]); bytes beyond `u.bytes` are undefined
    /// slack, exactly like the register's high bytes.
    fn store_unit(&mut self, base: CValue, u: &Unit, v: CValue) {
        self.b
            .ins()
            .store(MemFlagsData::trusted(), v, base, u.offset as i32);
    }

    /// Load one register unit from `base + u.offset`. INTEGER units
    /// with partial coverage assemble from exact chunks — `base` may
    /// be an interior view of a larger aggregate, so no byte beyond
    /// the value's real extent is ever read.
    fn load_unit(&mut self, base: CValue, u: &Unit) -> CValue {
        let flags = MemFlagsData::trusted();
        match u.class {
            RegClass::Sse => {
                let ty = unit_clif_ty(u);
                self.b.ins().load(ty, flags, base, u.offset as i32)
            }
            RegClass::Int => {
                if u.bytes >= 8 {
                    return self.b.ins().load(ctypes::I64, flags, base, u.offset as i32);
                }
                let mut acc: Option<CValue> = None;
                let mut off = 0u32;
                let bytes = u.bytes as u32;
                for (chunk, ty) in [(4u32, ctypes::I32), (2, ctypes::I16), (1, ctypes::I8)] {
                    while bytes - off >= chunk {
                        let v = self.b.ins().load(ty, flags, base, (u.offset + off) as i32);
                        let wide = self.b.ins().uextend(ctypes::I64, v);
                        let shifted = if off == 0 {
                            wide
                        } else {
                            self.b.ins().ishl_imm_s(wide, (off * 8) as i64)
                        };
                        acc = Some(match acc {
                            None => shifted,
                            Some(a) => self.b.ins().bor(a, shifted),
                        });
                        off += chunk;
                    }
                }
                acc.unwrap_or_else(|| self.b.ins().iconst(ctypes::I64, 0))
            }
        }
    }

    /// Copy `size` bytes from `src` to `dst` (both addresses), in
    /// straight-line chunks. Sizes are small compile-time constants.
    fn copy_bytes(&mut self, dst: CValue, src: CValue, size: u32) {
        let flags = MemFlagsData::trusted();
        let mut off = 0u32;
        for (chunk, ty) in [
            (8u32, ctypes::I64),
            (4, ctypes::I32),
            (2, ctypes::I16),
            (1, ctypes::I8),
        ] {
            while size - off >= chunk {
                let v = self.b.ins().load(ty, flags, src, off as i32);
                self.b.ins().store(flags, v, dst, off as i32);
                off += chunk;
            }
        }
    }

    /// Load `size` bytes from `src` into chunk values (phase one of a
    /// clobber-safe copy).
    fn load_chunks(&mut self, src: CValue, size: u32) -> Vec<(u32, CValue)> {
        let flags = MemFlagsData::trusted();
        let mut out = Vec::new();
        let mut off = 0u32;
        for (chunk, ty) in [
            (8u32, ctypes::I64),
            (4, ctypes::I32),
            (2, ctypes::I16),
            (1, ctypes::I8),
        ] {
            while size - off >= chunk {
                out.push((off, self.b.ins().load(ty, flags, src, off as i32)));
                off += chunk;
            }
        }
        out
    }

    fn store_chunks(&mut self, dst: CValue, chunks: &[(u32, CValue)]) {
        let flags = MemFlagsData::trusted();
        for &(off, v) in chunks {
            self.b.ins().store(flags, v, dst, off as i32);
        }
    }

    // ---- edges -----------------------------------------------------------

    /// Prepare one branch edge: aggregate args are copied into the
    /// target's param slots (loads first, then stores — see module
    /// docs); scalar args become CLIF branch args.
    fn edge(
        &mut self,
        bc: wolf_wir::ir::BlockCall,
    ) -> Result<(cranelift_codegen::ir::Block, Vec<BlockArg>), BackendError> {
        let target = bc.block;
        let args = self.f.vpool.get(bc.args);
        let params = self.f.block_params(target);
        let mut scalars: Vec<BlockArg> = Vec::new();
        // Phase 1: load every aggregate arg's bytes.
        let mut copies: Vec<(cranelift_codegen::ir::StackSlot, Vec<(u32, CValue)>)> = Vec::new();
        for (i, (&av, &pv)) in args.iter().zip(params.iter()).enumerate() {
            let ty = self.f.value_ty(pv);
            if self.m.types.is_token(ty) {
                continue;
            }
            if is_agg(self.m, ty) {
                let slot = *self
                    .param_slots
                    .get(&(target, i))
                    .ok_or_else(|| ice("aggregate block param has no slot"))?;
                let src = self.addr(av)?;
                let size = self.layout_of(ty)?.size;
                let chunks = self.load_chunks(src, size);
                copies.push((slot, chunks));
            } else {
                scalars.push(BlockArg::Value(self.scalar(av)?));
            }
        }
        // Phase 2: store into the target slots.
        for (slot, chunks) in copies {
            let dst = self.b.ins().stack_addr(ctypes::I64, slot, 0);
            self.store_chunks(dst, &chunks);
        }
        Ok((self.blocks[&target], scalars))
    }

    // ---- instruction dispatch --------------------------------------------

    fn inst(&mut self, inst: wolf_wir::ir::Inst) -> Result<(), BackendError> {
        let data = self.f.insts[inst];
        let op = data.op;
        let args = self.args_of(inst);
        let results = self.results_of(inst);
        match op {
            Opcode::Iconst => {
                let Aux::Int(n) = data.aux else {
                    return Err(ice("iconst without payload"));
                };
                let ty = scalar_clif_ty(self.m, self.f.value_ty(results[0]))
                    .ok_or_else(|| ice("iconst of non-scalar"))?;
                let c = self.b.ins().iconst(ty, n);
                self.vals.insert(results[0], Repr::Scalar(c));
            }
            Opcode::Fconst => {
                let Aux::FloatBits(bits) = data.aux else {
                    return Err(ice("fconst without payload"));
                };
                let rty = self.f.value_ty(results[0]);
                let c = if matches!(self.m.types.get(rty), TypeData::F32) {
                    self.b
                        .ins()
                        .f32const(cranelift_codegen::ir::immediates::Ieee32::with_bits(
                            bits as u32,
                        ))
                } else {
                    self.b
                        .ins()
                        .f64const(cranelift_codegen::ir::immediates::Ieee64::with_bits(bits))
                };
                self.vals.insert(results[0], Repr::Scalar(c));
            }
            Opcode::Bconst => {
                let Aux::Bool(v) = data.aux else {
                    return Err(ice("bconst without payload"));
                };
                let c = self.b.ins().iconst(ctypes::I8, i64::from(v));
                self.vals.insert(results[0], Repr::Scalar(c));
            }

            // ---- checked / wrapping / saturating integer arithmetic ----
            Opcode::IaddChk
            | Opcode::IsubChk
            | Opcode::ImulChk
            | Opcode::UaddChk
            | Opcode::UsubChk
            | Opcode::UmulChk => {
                let r = self.checked_arith(op, args[0], args[1], results[0])?;
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::IdivChk | Opcode::IremChk | Opcode::UdivChk | Opcode::UremChk => {
                let r = self.checked_div(op, args[0], args[1], results[0])?;
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::IaddWrap | Opcode::IsubWrap | Opcode::ImulWrap => {
                let a = self.scalar(args[0])?;
                let b = self.scalar(args[1])?;
                let r = match op {
                    Opcode::IaddWrap => self.b.ins().iadd(a, b),
                    Opcode::IsubWrap => self.b.ins().isub(a, b),
                    _ => self.b.ins().imul(a, b),
                };
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::IaddSat | Opcode::IsubSat | Opcode::ImulSat => {
                let r = self.sat_arith(op, args[0], args[1], results[0])?;
                self.vals.insert(results[0], Repr::Scalar(r));
            }

            // ---- bit ops (shift amounts mask in CLIF, matching WIR) ----
            Opcode::Band | Opcode::Bor | Opcode::Bxor => {
                let a = self.scalar(args[0])?;
                let b = self.scalar(args[1])?;
                let r = match op {
                    Opcode::Band => self.b.ins().band(a, b),
                    Opcode::Bor => self.b.ins().bor(a, b),
                    _ => self.b.ins().bxor(a, b),
                };
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::Shl | Opcode::Lshr | Opcode::Ashr => {
                let a = self.scalar(args[0])?;
                let b = self.scalar(args[1])?;
                let r = match op {
                    Opcode::Shl => self.b.ins().ishl(a, b),
                    Opcode::Lshr => self.b.ins().ushr(a, b),
                    _ => self.b.ins().sshr(a, b),
                };
                self.vals.insert(results[0], Repr::Scalar(r));
            }

            // ---- floats (IEEE, no traps) ----
            Opcode::Fadd | Opcode::Fsub | Opcode::Fmul | Opcode::Fdiv => {
                let a = self.scalar(args[0])?;
                let b = self.scalar(args[1])?;
                let r = match op {
                    Opcode::Fadd => self.b.ins().fadd(a, b),
                    Opcode::Fsub => self.b.ins().fsub(a, b),
                    Opcode::Fmul => self.b.ins().fmul(a, b),
                    _ => self.b.ins().fdiv(a, b),
                };
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::Fneg => {
                let a = self.scalar(args[0])?;
                let r = self.b.ins().fneg(a);
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::Fma => {
                let (a, b, c) = (
                    self.scalar(args[0])?,
                    self.scalar(args[1])?,
                    self.scalar(args[2])?,
                );
                let r = self.b.ins().fma(a, b, c);
                self.vals.insert(results[0], Repr::Scalar(r));
            }

            // ---- compares ----
            Opcode::Icmp => {
                let Aux::IntCc(cc) = data.aux else {
                    return Err(ice("icmp without condition"));
                };
                let a = self.scalar(args[0])?;
                let b = self.scalar(args[1])?;
                let r = self.b.ins().icmp(int_cc(cc), a, b);
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::Fcmp => {
                let Aux::FloatCc(cc) = data.aux else {
                    return Err(ice("fcmp without condition"));
                };
                let a = self.scalar(args[0])?;
                let b = self.scalar(args[1])?;
                let r = self.b.ins().fcmp(float_cc(cc), a, b);
                self.vals.insert(results[0], Repr::Scalar(r));
            }

            // ---- conversions ----
            Opcode::Sext | Opcode::Zext | Opcode::Itrunc => {
                let a = self.scalar(args[0])?;
                let ty = scalar_clif_ty(self.m, self.f.value_ty(results[0]))
                    .ok_or_else(|| ice("conversion to non-scalar"))?;
                let r = match op {
                    Opcode::Sext => self.b.ins().sextend(ty, a),
                    Opcode::Zext => self.b.ins().uextend(ty, a),
                    _ => self.b.ins().ireduce(ty, a),
                };
                self.vals.insert(results[0], Repr::Scalar(r));
            }

            // ---- memory ----
            Opcode::PtrOff => {
                let Aux::Scale(scale) = data.aux else {
                    return Err(ice("ptr.off without scale"));
                };
                let p = self.scalar(args[0])?;
                let i = self.scalar(args[1])?;
                // The index is i64 in WIR ([mem] lowering); scale is a
                // positive immediate.
                let scaled = self.b.ins().imul_imm_s(i, scale as i64);
                let r = self.b.ins().iadd(p, scaled);
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::Load => {
                let p = self.scalar(args[0])?;
                let rty = self.f.value_ty(results[0]);
                if is_agg(self.m, rty) {
                    let l = self.layout_of(rty)?;
                    let slot = self.new_slot(l);
                    let dst = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                    self.copy_bytes(dst, p, l.size);
                    self.vals.insert(results[0], Repr::Addr(dst));
                } else {
                    let ty =
                        scalar_clif_ty(self.m, rty).ok_or_else(|| ice("load of token type"))?;
                    let r = self.b.ins().load(ty, MemFlagsData::trusted(), p, 0);
                    self.vals.insert(results[0], Repr::Scalar(r));
                }
            }
            Opcode::Store => {
                let vty = self.f.value_ty(args[0]);
                let p = self.scalar(args[1])?;
                if is_agg(self.m, vty) {
                    let src = self.addr(args[0])?;
                    let size = self.layout_of(vty)?.size;
                    self.copy_bytes(p, src, size);
                } else {
                    let v = self.scalar(args[0])?;
                    self.b.ins().store(MemFlagsData::trusted(), v, p, 0);
                }
                // The successor token.
                self.vals.insert(results[0], Repr::Token);
            }

            // ---- aggregates ----
            Opcode::AggMake => {
                let rty = self.f.value_ty(results[0]);
                let TypeData::Agg(fields) = self.m.types.get(rty).clone() else {
                    return Err(ice("agg.make of non-aggregate"));
                };
                let l = self.layout_of(rty)?;
                let offs = layout::field_offsets(&self.m.types, &fields)
                    .ok_or_else(|| ice("aggregate with token field"))?;
                let slot = self.new_slot(l);
                let dst = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                for ((&av, &fty), &off) in args.iter().zip(fields.iter()).zip(offs.iter()) {
                    self.store_field(dst, off, fty, av)?;
                }
                self.vals.insert(results[0], Repr::Addr(dst));
            }
            Opcode::AggGet => {
                let Aux::Int(k) = data.aux else {
                    return Err(ice("agg.get without index"));
                };
                let aty = self.f.value_ty(args[0]);
                let TypeData::Agg(fields) = self.m.types.get(aty).clone() else {
                    return Err(ice("agg.get of non-aggregate"));
                };
                let offs = layout::field_offsets(&self.m.types, &fields)
                    .ok_or_else(|| ice("aggregate with token field"))?;
                let base = self.addr(args[0])?;
                let off = offs[k as usize];
                let fty = fields[k as usize];
                let repr = self.load_field(base, off, fty)?;
                self.vals.insert(results[0], repr);
            }

            // ---- error unions (value ops; branching is plain br) ----
            Opcode::EuMakeOk => {
                let rty = self.f.value_ty(results[0]);
                let l = self.layout_of(rty)?;
                let slot = self.new_slot(l);
                let dst = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                let zero = self.b.ins().iconst(ctypes::I64, 0);
                self.b.ins().store(MemFlagsData::trusted(), zero, dst, 0);
                if let Some(&ok) = args.first() {
                    let off = layout::eu_ok_offset(&self.m.types, rty)
                        .ok_or_else(|| ice("eu.make.ok operand for a unit ok"))?;
                    self.store_field(dst, off, self.f.value_ty(ok), ok)?;
                }
                self.vals.insert(results[0], Repr::Addr(dst));
            }
            Opcode::EuMakeErr => {
                let rty = self.f.value_ty(results[0]);
                let l = self.layout_of(rty)?;
                let slot = self.new_slot(l);
                let dst = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                let tag = self.scalar(args[0])?;
                self.b.ins().store(MemFlagsData::trusted(), tag, dst, 0);
                let slot_offs = layout::eu_slot_offset(&self.m.types, rty);
                let TypeData::Eu { slots, .. } = self.m.types.get(rty).clone() else {
                    return Err(ice("eu.make.err of non-eu"));
                };
                for (i, &pv) in args.iter().skip(1).enumerate() {
                    self.store_field(dst, slot_offs[i], slots[i], pv)?;
                }
                self.vals.insert(results[0], Repr::Addr(dst));
            }
            Opcode::EuIsErr => {
                let base = self.addr(args[0])?;
                let tag = self
                    .b
                    .ins()
                    .load(ctypes::I64, MemFlagsData::trusted(), base, 0);
                let r = self.b.ins().icmp_imm_s(IntCC::NotEqual, tag, 0);
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::EuOk => {
                let ety = self.f.value_ty(args[0]);
                let base = self.addr(args[0])?;
                let off = layout::eu_ok_offset(&self.m.types, ety)
                    .ok_or_else(|| ice("eu.ok of a unit ok"))?;
                let TypeData::Eu { ok: Some(okt), .. } = self.m.types.get(ety).clone() else {
                    return Err(ice("eu.ok of non-eu"));
                };
                let repr = self.load_field(base, off, okt)?;
                self.vals.insert(results[0], repr);
            }
            Opcode::EuErr => {
                let ety = self.f.value_ty(args[0]);
                let base = self.addr(args[0])?;
                match data.aux {
                    Aux::Int(k) => {
                        let offs = layout::eu_slot_offset(&self.m.types, ety);
                        let TypeData::Eu { slots, .. } = self.m.types.get(ety).clone() else {
                            return Err(ice("eu.err of non-eu"));
                        };
                        let repr = self.load_field(base, offs[k as usize], slots[k as usize])?;
                        self.vals.insert(results[0], repr);
                    }
                    _ => {
                        let tag = self
                            .b
                            .ins()
                            .load(ctypes::I64, MemFlagsData::trusted(), base, 0);
                        self.vals.insert(results[0], Repr::Scalar(tag));
                    }
                }
            }

            // ---- calls ----
            Opcode::Call => {
                let Aux::Callee(ef) = data.aux else {
                    return Err(ice("call without callee"));
                };
                let callee = self.f.ext_funcs[ef].clone();
                self.lower_call(&callee.name, callee.sig, &args, &results)?;
            }

            // ---- the memory story (runtime shims) ----
            Opcode::RegionNew => {
                let fref = self.rt_ref("__wolf_rt_region_new")?;
                let call = self.b.ins().call(fref, &[]);
                let h = self.b.inst_results(call)[0];
                self.vals.insert(results[0], Repr::Scalar(h));
                self.vals.insert(results[1], Repr::Token);
            }
            Opcode::RegionAlloc => {
                let h = self.scalar(args[0])?;
                let size = self.scalar(args[1])?;
                let fref = self.rt_ref("__wolf_rt_region_alloc")?;
                let call = self.b.ins().call(fref, &[h, size]);
                let p = self.b.inst_results(call)[0];
                self.vals.insert(results[0], Repr::Scalar(p));
                self.vals.insert(results[1], Repr::Token);
            }
            Opcode::RegionFree => {
                let h = self.scalar(args[0])?;
                let fref = self.rt_ref("__wolf_rt_region_free")?;
                self.b.ins().call(fref, &[h]);
            }
            Opcode::SyncFreeze => {
                let h = self.scalar(args[0])?;
                let fref = self.rt_ref("__wolf_rt_region_freeze")?;
                self.b.ins().call(fref, &[h]);
                self.vals.insert(results[0], Repr::Token);
            }
            Opcode::StackAlloc => {
                // The size is an iconst by the op's contract.
                let size = self.const_of(args[0]).ok_or_else(|| {
                    nyi("stack.alloc with a non-constant size (not emitted by lowering)")
                })?;
                let size32 =
                    u32::try_from(size).map_err(|_| ice("stack.alloc size out of range"))?;
                let slot = self.new_slot(Layout {
                    size: size32,
                    align: 16,
                });
                let p = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                self.vals.insert(results[0], Repr::Scalar(p));
                self.vals.insert(results[1], Repr::Token);
            }
            Opcode::RcDup | Opcode::RcDrop => {
                return Err(nyi(
                    "shared-tier rc ops (runtime shape lands with the shared cells, s42)",
                ));
            }
            Opcode::SyncTransfer => {
                return Err(nyi("sync.transfer is reserved (concurrency campaign)"));
            }

            // ---- terminators ----
            Opcode::Jmp => {
                let Aux::Jump(bc) = data.aux else {
                    return Err(ice("jmp without edge"));
                };
                let (tb, cargs) = self.edge(bc)?;
                self.b.ins().jump(tb, cargs.iter());
            }
            Opcode::Br => {
                let Aux::Br(t, e) = data.aux else {
                    return Err(ice("br without edges"));
                };
                let cond = self.scalar(args[0])?;
                // Aggregate-carrying edges get per-edge trampolines so
                // the not-taken edge's copies never run.
                let needs_tramp = |tx: &Self, bc: wolf_wir::ir::BlockCall| {
                    let params = tx.f.block_params(bc.block);
                    params.iter().any(|&p| is_agg(tx.m, tx.f.value_ty(p)))
                };
                if needs_tramp(self, t) || needs_tramp(self, e) {
                    let tt = self.b.create_block();
                    let te = self.b.create_block();
                    let empty: [BlockArg; 0] = [];
                    self.b.ins().brif(cond, tt, empty.iter(), te, empty.iter());
                    for (tramp, bc) in [(tt, t), (te, e)] {
                        self.b.switch_to_block(tramp);
                        let (tb, cargs) = self.edge(bc)?;
                        self.b.ins().jump(tb, cargs.iter());
                    }
                } else {
                    let (tb, targs) = self.edge(t)?;
                    let (eb, eargs) = self.edge(e)?;
                    self.b.ins().brif(cond, tb, targs.iter(), eb, eargs.iter());
                }
            }
            Opcode::Ret => {
                let si = sig_info(
                    self.m,
                    self.f.sig,
                    self.conv,
                    self.om.isa().default_call_conv(),
                );
                let mut scalars: Vec<CValue> = Vec::new();
                let mut sret_i = 0usize;
                for (&rv, slot) in args.iter().zip(si.rets.iter()) {
                    match slot {
                        RSlot::Token => {}
                        RSlot::Direct(_) => scalars.push(self.scalar(rv)?),
                        RSlot::Split(units) => {
                            let src = self.addr(rv)?;
                            for u in units {
                                let v = self.load_unit(src, u);
                                scalars.push(v);
                            }
                        }
                        RSlot::Sret {
                            disc_in_reg,
                            c,
                            size,
                        } => {
                            let dst = self.sret[sret_i];
                            sret_i += 1;
                            let src = self.addr(rv)?;
                            self.copy_bytes(dst, src, *size);
                            if *disc_in_reg && !c {
                                // [abi.err.repr]: the discriminant
                                // rides the first INTEGER return
                                // register even for out-slot unions
                                // (tag is at offset 0 by layout).
                                let tag =
                                    self.b
                                        .ins()
                                        .load(ctypes::I64, MemFlagsData::trusted(), src, 0);
                                scalars.push(tag);
                            }
                        }
                    }
                }
                self.b.ins().return_(&scalars);
            }
            Opcode::Trap => {
                let kind = match data.aux {
                    Aux::Trap(k) => k,
                    _ => TrapKind::Assert,
                };
                let code = trap_code(kind);
                let fref = self.rt_ref("__wolf_rt_trap")?;
                let c = self.b.ins().iconst(ctypes::I32, code as i64);
                self.b.ins().call(fref, &[c]);
                self.b
                    .ins()
                    .trap(TrapCode::user(code as u8).ok_or_else(|| ice("trap code 0"))?);
            }
        }
        Ok(())
    }

    /// Constant payload of a value defined by `iconst`, if any.
    fn const_of(&self, v: wolf_wir::ir::Value) -> Option<i64> {
        match self.f.values[v].def {
            wolf_wir::ir::ValueDef::Result(inst, 0) if self.f.insts[inst].op == Opcode::Iconst => {
                match self.f.insts[inst].aux {
                    Aux::Int(n) => Some(n),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Store one field value (scalar or aggregate) at `base + off`.
    fn store_field(
        &mut self,
        base: CValue,
        off: u32,
        fty: TypeId,
        v: wolf_wir::ir::Value,
    ) -> Result<(), BackendError> {
        if is_agg(self.m, fty) {
            let src = self.addr(v)?;
            let size = self.layout_of(fty)?.size;
            let dst = self.b.ins().iadd_imm_s(base, off as i64);
            self.copy_bytes(dst, src, size);
        } else {
            let cv = self.scalar(v)?;
            self.b
                .ins()
                .store(MemFlagsData::trusted(), cv, base, off as i32);
        }
        Ok(())
    }

    /// Load one field (scalar loads a value; aggregate is a view into
    /// the parent's bytes — SSA immutability makes the shared address
    /// sound).
    fn load_field(&mut self, base: CValue, off: u32, fty: TypeId) -> Result<Repr, BackendError> {
        if is_agg(self.m, fty) {
            let p = self.b.ins().iadd_imm_s(base, off as i64);
            Ok(Repr::Addr(p))
        } else {
            let ty = scalar_clif_ty(self.m, fty).ok_or_else(|| ice("field of token type"))?;
            let v = self
                .b
                .ins()
                .load(ty, MemFlagsData::trusted(), base, off as i32);
            Ok(Repr::Scalar(v))
        }
    }

    /// Checked add/sub/mul, signed and unsigned (X3: trap on overflow,
    /// with the identity kept). Narrow widths widen to i64 and
    /// range-check; i64 uses the native overflow-flag ops.
    fn checked_arith(
        &mut self,
        op: Opcode,
        wa: wolf_wir::ir::Value,
        wb: wolf_wir::ir::Value,
        res: wolf_wir::ir::Value,
    ) -> Result<CValue, BackendError> {
        let ty = self.f.value_ty(res);
        let bits = self
            .m
            .types
            .int_bits(ty)
            .ok_or_else(|| ice("checked arith on non-int"))?;
        let a = self.scalar(wa)?;
        let b = self.scalar(wb)?;
        let signed = matches!(op, Opcode::IaddChk | Opcode::IsubChk | Opcode::ImulChk);
        if bits == 64 {
            let (r, o) = match op {
                Opcode::IaddChk => self.b.ins().sadd_overflow(a, b),
                Opcode::IsubChk => self.b.ins().ssub_overflow(a, b),
                Opcode::ImulChk => self.b.ins().smul_overflow(a, b),
                Opcode::UaddChk => self.b.ins().uadd_overflow(a, b),
                Opcode::UsubChk => self.b.ins().usub_overflow(a, b),
                _ => self.b.ins().umul_overflow(a, b),
            };
            self.trap_if(o, TrapKind::Overflow)?;
            return Ok(r);
        }
        // Narrow: widen, compute exactly, range-check, reduce.
        let (xa, xb) = if signed {
            (
                self.b.ins().sextend(ctypes::I64, a),
                self.b.ins().sextend(ctypes::I64, b),
            )
        } else {
            (
                self.b.ins().uextend(ctypes::I64, a),
                self.b.ins().uextend(ctypes::I64, b),
            )
        };
        let r = match op {
            Opcode::IaddChk | Opcode::UaddChk => self.b.ins().iadd(xa, xb),
            Opcode::IsubChk | Opcode::UsubChk => self.b.ins().isub(xa, xb),
            _ => self.b.ins().imul(xa, xb),
        };
        let ovf = if signed {
            let (lo, hi) = self
                .m
                .types
                .int_bounds(ty)
                .ok_or_else(|| ice("bounds of non-int"))?;
            let below = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, r, lo as i64);
            let above = self
                .b
                .ins()
                .icmp_imm_s(IntCC::SignedGreaterThan, r, hi as i64);
            self.b.ins().bor(below, above)
        } else {
            let mask = if bits >= 64 {
                -1i64
            } else {
                (1i64 << bits) - 1
            };
            // Unsigned: sub may go negative, add/mul may exceed the
            // mask — one signed range check covers both since the
            // widened exact result is in (i64::MIN, 2^63).
            let below = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, r, 0);
            let above = self.b.ins().icmp_imm_s(IntCC::SignedGreaterThan, r, mask);
            self.b.ins().bor(below, above)
        };
        self.trap_if(ovf, TrapKind::Overflow)?;
        Ok(self.b.ins().ireduce(clif_int(bits), r))
    }

    /// Checked division/remainder: explicit zero (and MIN/-1 for
    /// signed) checks branch to the trap blocks; the native op runs on
    /// the safe residue.
    fn checked_div(
        &mut self,
        op: Opcode,
        wa: wolf_wir::ir::Value,
        wb: wolf_wir::ir::Value,
        res: wolf_wir::ir::Value,
    ) -> Result<CValue, BackendError> {
        let ty = self.f.value_ty(res);
        let a = self.scalar(wa)?;
        let b = self.scalar(wb)?;
        let zero = self.b.ins().icmp_imm_s(IntCC::Equal, b, 0);
        self.trap_if(zero, TrapKind::DivZero)?;
        match op {
            Opcode::IdivChk | Opcode::IremChk => {
                let (lo, _) = self
                    .m
                    .types
                    .int_bounds(ty)
                    .ok_or_else(|| ice("div on non-int"))?;
                let is_min = self.b.ins().icmp_imm_s(IntCC::Equal, a, lo as i64);
                let is_m1 = self.b.ins().icmp_imm_s(IntCC::Equal, b, -1);
                let ovf = self.b.ins().band(is_min, is_m1);
                self.trap_if(ovf, TrapKind::Overflow)?;
                Ok(if op == Opcode::IdivChk {
                    self.b.ins().sdiv(a, b)
                } else {
                    self.b.ins().srem(a, b)
                })
            }
            _ => Ok(if op == Opcode::UdivChk {
                self.b.ins().udiv(a, b)
            } else {
                self.b.ins().urem(a, b)
            }),
        }
    }

    /// Saturating add/sub/mul (explicit source-level spelling; clamps,
    /// never traps).
    fn sat_arith(
        &mut self,
        op: Opcode,
        wa: wolf_wir::ir::Value,
        wb: wolf_wir::ir::Value,
        res: wolf_wir::ir::Value,
    ) -> Result<CValue, BackendError> {
        let ty = self.f.value_ty(res);
        let bits = self
            .m
            .types
            .int_bits(ty)
            .ok_or_else(|| ice("sat arith on non-int"))?;
        let a = self.scalar(wa)?;
        let b = self.scalar(wb)?;
        let (lo, hi) = self
            .m
            .types
            .int_bounds(ty)
            .ok_or_else(|| ice("bounds of non-int"))?;
        if bits < 64 {
            let xa = self.b.ins().sextend(ctypes::I64, a);
            let xb = self.b.ins().sextend(ctypes::I64, b);
            let r = match op {
                Opcode::IaddSat => self.b.ins().iadd(xa, xb),
                Opcode::IsubSat => self.b.ins().isub(xa, xb),
                _ => self.b.ins().imul(xa, xb),
            };
            let lov = self.b.ins().iconst(ctypes::I64, lo as i64);
            let hiv = self.b.ins().iconst(ctypes::I64, hi as i64);
            let r = self.b.ins().smax(r, lov);
            let r = self.b.ins().smin(r, hiv);
            return Ok(self.b.ins().ireduce(clif_int(bits), r));
        }
        let (r, o) = match op {
            Opcode::IaddSat => self.b.ins().sadd_overflow(a, b),
            Opcode::IsubSat => self.b.ins().ssub_overflow(a, b),
            _ => self.b.ins().smul_overflow(a, b),
        };
        // Overflow direction: for add/sub, a's sign picks the clamp;
        // for mul, the product's sign (sign(a) ^ sign(b)).
        let neg = match op {
            Opcode::ImulSat => {
                let sa = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, a, 0);
                let sb = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, b, 0);
                self.b.ins().bxor(sa, sb)
            }
            _ => self.b.ins().icmp_imm_s(IntCC::SignedLessThan, a, 0),
        };
        let lov = self.b.ins().iconst(ctypes::I64, lo as i64);
        let hiv = self.b.ins().iconst(ctypes::I64, hi as i64);
        let clamp = self.b.ins().select(neg, lov, hiv);
        Ok(self.b.ins().select(o, clamp, r))
    }

    /// Lower one call, executing the callee's ABI plan: tokens erased,
    /// small aggregates in registers, big ones through memory, error
    /// unions per `[abi.err.repr]`. The convention is the callee's:
    /// wolf-native for module functions and runtime shims, C SysV for
    /// the `c.*` import membrane.
    fn lower_call(
        &mut self,
        callee: &str,
        sig: SigId,
        args: &[wolf_wir::ir::Value],
        results: &[wolf_wir::ir::Value],
    ) -> Result<(), BackendError> {
        let cc = self.om.isa().default_call_conv();
        // Resolve the callee: module function (wolf conv), `c.*`
        // import (C membrane, unmangled libc symbol), or runtime shim.
        let (fref, conv) = if let Some(&(fr, conv)) = self.fref_cache.get(callee) {
            (fr, conv)
        } else if let Some(entry) = self.funcs.get(callee) {
            let fr = self.om.declare_func_in_func(entry.fid, self.b.func);
            self.fref_cache.insert(callee.to_string(), (fr, Conv::Wolf));
            (fr, Conv::Wolf)
        } else if let Some(symbol) = abi::c_import_symbol(callee) {
            // The explicit membrane (D19): the WIR name's `c.`
            // namespace IS the seam; the linker symbol is the plain C
            // name, declared with the SysV plan.
            let si_c = sig_info(self.m, sig, Conv::C, cc);
            let fid = match self.imports.get(callee) {
                Some(&fid) => fid,
                None => {
                    let fid = self
                        .om
                        .declare_function(symbol, cranelift_module::Linkage::Import, &si_c.clif)
                        .map_err(|e| ice(e.to_string()))?;
                    self.imports.insert(callee.to_string(), fid);
                    fid
                }
            };
            let fr = self.om.declare_func_in_func(fid, self.b.func);
            self.fref_cache.insert(callee.to_string(), (fr, Conv::C));
            (fr, Conv::C)
        } else if let Some((name, ..)) = RT_SYMBOLS.iter().find(|(n, ..)| n == &callee) {
            let fr = self.rt_ref(name)?;
            (fr, Conv::Wolf)
        } else {
            return Err(nyi(format!(
                "call to external `@{callee}` (hand-declared externs beyond the \
                 modelled c.* set are c10's header importer)"
            )));
        };
        let si = sig_info(self.m, sig, conv, cc);
        let mut cargs: Vec<CValue> = Vec::new();
        // Out-slots for memory-class results, in result order. C puts
        // the pointer FIRST in the argument list; wolf trails it.
        let rdata = self.m.sigs[sig].results.clone();
        let mut sret_addrs: Vec<CValue> = Vec::new();
        for (&rty, slot) in rdata.iter().zip(si.rets.iter()) {
            if matches!(slot, RSlot::Sret { .. }) {
                let l = self.layout_of(rty)?;
                let s = self.new_slot(l);
                let a = self.b.ins().stack_addr(ctypes::I64, s, 0);
                sret_addrs.push(a);
                if si.sret_first {
                    cargs.push(a);
                }
            }
        }
        for (&av, slot) in args.iter().zip(si.params.iter()) {
            match slot {
                Slot::Token => {}
                Slot::Direct(_) => cargs.push(self.scalar(av)?),
                Slot::Split(units) => {
                    let units = units.clone();
                    let base = self.addr(av)?;
                    for u in &units {
                        let v = self.load_unit(base, u);
                        cargs.push(v);
                    }
                }
                // C byval struct args take the ADDRESS as the CLIF
                // value; Cranelift performs the stack copy.
                Slot::Ref | Slot::CStruct(_) => cargs.push(self.addr(av)?),
            }
        }
        if !si.sret_first {
            cargs.extend(sret_addrs.iter().copied());
        }
        let call = self.b.ins().call(fref, &cargs);
        let rvals: Vec<CValue> = self.b.inst_results(call).to_vec();
        // Map declared results, then successor tokens (which trail the
        // declared results in WIR call results).
        let mut rv_i = 0usize;
        let mut sret_i = 0usize;
        let mut wr = results.iter();
        for slot in si.rets.iter() {
            let &rv = wr.next().ok_or_else(|| ice("call result arity"))?;
            match slot {
                RSlot::Token => {
                    self.vals.insert(rv, Repr::Token);
                }
                RSlot::Direct(_) => {
                    self.vals.insert(rv, Repr::Scalar(rvals[rv_i]));
                    rv_i += 1;
                }
                RSlot::Split(units) => {
                    // Materialize the returned units into a slot.
                    let units = units.clone();
                    let addr = self.units_slot_addr(&units);
                    for u in &units {
                        let v = rvals[rv_i];
                        rv_i += 1;
                        self.store_unit(addr, u, v);
                    }
                    self.vals.insert(rv, Repr::Addr(addr));
                }
                RSlot::Sret { disc_in_reg, c, .. } => {
                    if *disc_in_reg && !c {
                        // The register discriminant is advisory at
                        // this tier (the tag bytes are in the slot);
                        // consume its return position.
                        rv_i += 1;
                    }
                    self.vals.insert(rv, Repr::Addr(sret_addrs[sret_i]));
                    sret_i += 1;
                }
            }
        }
        for &rv in wr {
            // Successor tokens.
            self.vals.insert(rv, Repr::Token);
        }
        Ok(())
    }
}

/// The debug shape of a WIR scalar type (s30 variable DIEs); `None`
/// for aggregates, error unions, and tokens (no scalar DIE at v0).
fn debug_ty(m: &WModule, ty: TypeId) -> Option<DebugTy> {
    Some(match m.types.get(ty) {
        TypeData::Bool => DebugTy::Bool,
        TypeData::I8 => DebugTy::I8,
        TypeData::I16 => DebugTy::I16,
        TypeData::I32 => DebugTy::I32,
        TypeData::I64 => DebugTy::I64,
        TypeData::F32 => DebugTy::F32,
        TypeData::F64 => DebugTy::F64,
        TypeData::Ptr => DebugTy::Ptr,
        TypeData::Mem(_) | TypeData::Io | TypeData::Agg(_) | TypeData::Eu { .. } => return None,
    })
}

fn clif_int(bits: u32) -> cranelift_codegen::ir::Type {
    match bits {
        8 => ctypes::I8,
        16 => ctypes::I16,
        32 => ctypes::I32,
        _ => ctypes::I64,
    }
}

fn trap_code(kind: TrapKind) -> i32 {
    use wolf_rt::native::trap_code as tc;
    match kind {
        TrapKind::Assert => tc::ASSERT,
        TrapKind::Overflow => tc::OVERFLOW,
        TrapKind::DivZero => tc::DIV_ZERO,
        TrapKind::Bounds => tc::BOUNDS,
    }
}

fn int_cc(cc: IntCc) -> IntCC {
    match cc {
        IntCc::Eq => IntCC::Equal,
        IntCc::Ne => IntCC::NotEqual,
        IntCc::Slt => IntCC::SignedLessThan,
        IntCc::Sle => IntCC::SignedLessThanOrEqual,
        IntCc::Sgt => IntCC::SignedGreaterThan,
        IntCc::Sge => IntCC::SignedGreaterThanOrEqual,
        IntCc::Ult => IntCC::UnsignedLessThan,
        IntCc::Ule => IntCC::UnsignedLessThanOrEqual,
        IntCc::Ugt => IntCC::UnsignedGreaterThan,
        IntCc::Uge => IntCC::UnsignedGreaterThanOrEqual,
    }
}

fn float_cc(cc: FloatCc) -> FloatCC {
    // WIR float compares are ORDERED (NaN compares false) for
    // eq/lt/le/gt/ge; `ne` is IEEE-754 `!=` — the NEGATION of `eq`, so
    // it is UNORDERED (true when either operand is NaN). `nan != nan`
    // must be true natively exactly as in the interpreter (issue #22,
    // wolf-std F-0027: `x != x` is *the* portable NaN test).
    match cc {
        FloatCc::Eq => FloatCC::Equal,
        FloatCc::Ne => FloatCC::NotEqual,
        FloatCc::Lt => FloatCC::LessThan,
        FloatCc::Le => FloatCC::LessThanOrEqual,
        FloatCc::Gt => FloatCC::GreaterThan,
        FloatCc::Ge => FloatCC::GreaterThanOrEqual,
    }
}
