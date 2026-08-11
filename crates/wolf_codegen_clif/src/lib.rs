//! Interim debug backend: WIR → Cranelift → relocatable ELF objects
//! (s28–s31). Sits behind [`wolf_backend::Backend`]; DELETED at c12
//! closeout when the owned Tier-F backend takes the same trait (D1 —
//! this crate is scaffolding, and everything above the interface crate
//! must not notice the swap).
//!
//! # The fact-erasure table (s28 acceptance)
//!
//! WIR facts are PROVEN semantics (D2): dropping them is always sound,
//! exploiting them is optional. This debug tier drops nearly all of
//! them — the table below is the measure of what Tier-R (c09) has to
//! exploit. Disposition of every WIR fact kind and the token spine at
//! this tier:
//!
//! | WIR carrier            | Disposition at Cranelift tier            |
//! |------------------------|------------------------------------------|
//! | `noalias a b`          | ERASED (no aliasing hints in CLIF)       |
//! | `deref p N`            | PARTIALLY USED: licenses `MemFlags::     |
//! |                        | trusted()` (non-trapping, aligned) on    |
//! |                        | every load/store — the verifier already  |
//! |                        | proved dereferenceability                |
//! | `range v lo..=hi`      | ERASED (the bounds-check-elision seam    |
//! |                        | stays in WIR where the verifier proves   |
//! |                        | it; nothing to elide until `bounds.br`)  |
//! | `region p rN`          | ERASED (provenance never reaches CLIF)   |
//! | `frozen m`             | ERASED (rematerialization is s42's)      |
//! | effect tokens          | ERASED to program order: instruction     |
//! | (`mem.rN`, `io`)       | order within a block IS the memory       |
//! |                        | order; tokens become nothing             |
//! | trap kinds             | KEPT: every trap reaches                 |
//! |                        | `__wolf_rt_trap(kind)` with its identity |
//! |                        | — verdicts must match the interpreter's  |
//!
//! # The v0 runtime model
//!
//! Region ops lower to calls into `wolf_rt`'s native shims
//! (`__wolf_rt_region_new/alloc/free/freeze`); `stack.alloc` becomes a
//! real stack slot. Checked arithmetic lowers to explicit compare
//! sequences that branch to per-kind trap blocks calling
//! `__wolf_rt_trap` — a CALL, not a hardware trap, so the trap KIND
//! survives to the process boundary without signal-handler machinery
//! (recorded s28 delta from the contract's `trapnz` sketch; the
//! trap-info table `_W.traps` is still emitted for s31's runtime).
//! Aggregates and error unions live in explicit stack slots addressed
//! through `wolf_backend::layout` — locals stay addressable (no SROA
//! heroics; s30's debugger story depends on it).

mod translate;

use std::collections::HashMap;

use cranelift_codegen::ir::UserFuncName;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, Module as _};
use cranelift_object::{ObjectBuilder, ObjectModule};

use wolf_backend::{
    Backend, BackendError, Capabilities, DebugSink, DwarfFidelity, Linkage, ObjectProduct,
    SymbolInfo, mangle,
};
use wolf_wir::entity::EntityRef;
use wolf_wir::ir::{Aux, FuncId, Function, Module as WirModule, SigId};
use wolf_wir::ops::Opcode;
use wolf_wir::types;

pub use translate::RT_SYMBOLS;

/// The trap-info table's symbol: `[count][code u8, name bytes, NUL]…`
/// — which check fired, by code, for s31's runtime reporting.
pub const TRAP_TABLE_SYMBOL: &str = "_W.traps";

/// The Cranelift implementation of the backend trait.
pub struct ClifBackend {
    module: ObjectModule,
    /// WIR name → declared entry: how call sites resolve.
    funcs: HashMap<String, translate::FuncEntry>,
    /// Runtime shims declared so far (`__wolf_rt_*`).
    rt: HashMap<&'static str, cranelift_module::FuncId>,
    /// C membrane imports declared so far (`c.*` callee names, s29).
    imports: HashMap<String, cranelift_module::FuncId>,
    symbols: Vec<SymbolInfo>,
    /// CLIF text of every defined function, in definition order — the
    /// golden-snapshot surface (lowering changes are reviewed diffs).
    clif_texts: Vec<(String, String)>,
    fb_ctx: FunctionBuilderContext,
}

impl ClifBackend {
    /// A backend for the host target. s28 is linux/x86-64 only (M1;
    /// D35's matrix is c13) — anything else is an honest refusal.
    pub fn new() -> Result<ClifBackend, BackendError> {
        let triple = target_lexicon::Triple::host();
        if triple.architecture != target_lexicon::Architecture::X86_64
            || triple.operating_system != target_lexicon::OperatingSystem::Linux
        {
            return Err(BackendError::Unsupported(format!(
                "native codegen targets linux/x86-64 only in s28 (host: {triple})"
            )));
        }
        let mut flags = settings::builder();
        // Position-independent objects: host toolchains link PIE by
        // default.
        flags
            .set("is_pic", "true")
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        let isa = cranelift_codegen::isa::lookup(triple)
            .map_err(|e| BackendError::Internal(e.to_string()))?
            .finish(settings::Flags::new(flags))
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        let builder = ObjectBuilder::new(isa, "wolf", cranelift_module::default_libcall_names())
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        Ok(ClifBackend {
            module: ObjectModule::new(builder),
            funcs: HashMap::new(),
            rt: HashMap::new(),
            imports: HashMap::new(),
            symbols: Vec::new(),
            clif_texts: Vec::new(),
            fb_ctx: FunctionBuilderContext::new(),
        })
    }

    /// The CLIF text of every function defined so far (snapshots).
    pub fn clif_texts(&self) -> &[(String, String)] {
        &self.clif_texts
    }
}

fn clif_linkage(l: Linkage) -> cranelift_module::Linkage {
    match l {
        Linkage::Export => cranelift_module::Linkage::Export,
        Linkage::Local => cranelift_module::Linkage::Local,
        Linkage::Import => cranelift_module::Linkage::Import,
    }
}

impl Backend for ClifBackend {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_in_place_patching: false,
            dwarf_fidelity: DwarfFidelity::Lines,
        }
    }

    fn declare_function(
        &mut self,
        module: &WirModule,
        id: FuncId,
        name: &str,
        symbol: &str,
        sig: SigId,
        linkage: Linkage,
    ) -> Result<(), BackendError> {
        // `export` functions are C membranes: declared under the SysV
        // plan (s29, D19). Everything else is wolf-native.
        let conv = if module.funcs.get(id).is_some_and(|f| f.export) {
            wolf_backend::abi::Conv::C
        } else {
            wolf_backend::abi::Conv::Wolf
        };
        let si = translate::sig_info(module, sig, conv, self.module.isa().default_call_conv());
        let fid = self
            .module
            .declare_function(symbol, clif_linkage(linkage), &si.clif)
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        self.funcs
            .insert(name.to_string(), translate::FuncEntry { fid, sig });
        self.symbols.push(SymbolInfo {
            name: symbol.to_string(),
            linkage,
            is_function: true,
        });
        Ok(())
    }

    fn define_function(
        &mut self,
        module: &WirModule,
        id: FuncId,
        func: &Function,
        debug: &mut dyn DebugSink,
    ) -> Result<(), BackendError> {
        let Some(entry) = self.funcs.get(&func.name) else {
            return Err(BackendError::Internal(format!(
                "function `{}` defined before being declared",
                func.name
            )));
        };
        let (fid, sig) = (entry.fid, entry.sig);
        let conv = if func.export {
            wolf_backend::abi::Conv::C
        } else {
            wolf_backend::abi::Conv::Wolf
        };
        let si = translate::sig_info(module, sig, conv, self.module.isa().default_call_conv());
        let mut ctx = self.module.make_context();
        ctx.func.signature = si.clif.clone();
        ctx.func.name = UserFuncName::user(0, id.as_u32());
        {
            let builder = FunctionBuilder::new(&mut ctx.func, &mut self.fb_ctx);
            translate::translate_function(
                builder,
                module,
                func,
                conv,
                &si,
                &mut self.module,
                &self.funcs,
                &mut self.rt,
                &mut self.imports,
            )?;
        }
        let symbol = self
            .module
            .declarations()
            .get_function_decl(fid)
            .linkage_name(fid)
            .into_owned();
        debug.function(id, &func.name, &symbol);
        self.clif_texts
            .push((func.name.clone(), ctx.func.display().to_string()));
        self.module
            .define_function(fid, &mut ctx)
            .map_err(|e| BackendError::Internal(format!("{}: {e}", func.name)))?;
        Ok(())
    }

    fn define_data(
        &mut self,
        name: &str,
        bytes: &[u8],
        linkage: Linkage,
    ) -> Result<(), BackendError> {
        let did = self
            .module
            .declare_data(name, clif_linkage(linkage), false, false)
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        let mut desc = DataDescription::new();
        desc.define(bytes.to_vec().into_boxed_slice());
        self.module
            .define_data(did, &desc)
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        self.symbols.push(SymbolInfo {
            name: name.to_string(),
            linkage,
            is_function: false,
        });
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<ObjectProduct, BackendError> {
        let product = self.module.finish();
        let bytes = product
            .emit()
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        Ok(ObjectProduct {
            bytes,
            symbols: self.symbols,
        })
    }
}

/// Append the C-entry shim to a WIR module: `main(argc, argv)` — in
/// truth a niladic CLIF function — that calls wolf's `@main`. A plain
/// `() -> i64` main truncates its result to the C exit code. An
/// error-union main (`!int`, s29 `[abi.err]`) branches on the
/// discriminant: the ok value becomes the exit code; an error value
/// becomes the documented D30 process behavior — `error: <tag name>`
/// on stdout and exit 1, via `__wolf_rt_main_err` (the tag's name
/// bytes ride as packed immediates selected by a compile-time tag
/// dispatch: tag names are module-static, so no runtime name table is
/// needed). The returned id must be declared with the UNMANGLED
/// symbol `main` ([`compile_module`] does).
pub fn add_entry_shim(m: &mut WirModule) -> Result<FuncId, BackendError> {
    let Some((_, entry)) = m.funcs.iter().find(|(_, f)| f.name == "main") else {
        return Err(BackendError::Unsupported(
            "no `main` function to build an executable around".to_string(),
        ));
    };
    let entry_sig = entry.sig;
    let refuse = |sd: &wolf_wir::ir::SigData| {
        Err(BackendError::Unsupported(format!(
            "entry signature is not `() -> i64` or `() -> !i64` (found {} param(s), \
             {} result(s))",
            sd.params.len(),
            sd.results.len()
        )))
    };
    let sd = m.sigs[entry_sig].clone();
    if !sd.params.is_empty() || sd.results.len() != 1 {
        return refuse(&sd);
    }
    let res_ty = sd.results[0];
    let eu_ok = match m.types.get(res_ty).clone() {
        wolf_wir::types::TypeData::I64 => None,
        wolf_wir::types::TypeData::Eu { ok, .. } => {
            if ok.is_some_and(|t| t != types::I64) {
                return refuse(&sd);
            }
            Some(ok)
        }
        _ => return refuse(&sd),
    };
    let tags = m.tags.clone();
    let err_sig = m.make_sig(vec![wolf_wir::ir::Param::val(types::I64); 6], vec![]);
    let shim_sig = m.make_sig(vec![], vec![types::I32]);
    let mut f = Function::new("__wolf_main_shim", shim_sig);
    let b0 = f.make_block(&[]);
    let callee = f.import_func("main", entry_sig);
    let (_, res) = f.append_inst(b0, Opcode::Call, &[], &[res_ty], Aux::Callee(callee));

    let Some(eu_ok) = eu_ok else {
        // Plain `() -> i64`: truncate and return.
        let (_, tr) = f.append_inst(b0, Opcode::Itrunc, &[res[0]], &[types::I32], Aux::None);
        f.append_inst(b0, Opcode::Ret, &[tr[0]], &[], Aux::None);
        return Ok(m.add_func(f));
    };

    // Error-union main: branch on the discriminant.
    let b_ok = f.make_block(&[]);
    let b_err = f.make_block(&[]);
    let (_, is_err) = f.append_inst(b0, Opcode::EuIsErr, &[res[0]], &[types::BOOL], Aux::None);
    let then_edge = f.block_call(b_err, &[]);
    let else_edge = f.block_call(b_ok, &[]);
    f.append_inst(
        b0,
        Opcode::Br,
        &[is_err[0]],
        &[],
        Aux::Br(then_edge, else_edge),
    );
    // Ok path: the payload (or 0 for a unit ok) is the exit code.
    let code = if eu_ok.is_some() {
        let (_, ok) = f.append_inst(b_ok, Opcode::EuOk, &[res[0]], &[types::I64], Aux::None);
        let (_, tr) = f.append_inst(b_ok, Opcode::Itrunc, &[ok[0]], &[types::I32], Aux::None);
        tr[0]
    } else {
        let (_, z) = f.append_inst(b_ok, Opcode::Iconst, &[], &[types::I32], Aux::Int(0));
        z[0]
    };
    f.append_inst(b_ok, Opcode::Ret, &[code], &[], Aux::None);
    // Err path: compile-time dispatch over the module's interned tags
    // (tag id k = index + 1) hands the tag's NAME to the runtime.
    let (_, tagv) = f.append_inst(b_err, Opcode::EuErr, &[res[0]], &[types::I64], Aux::None);
    let err_callee = f.import_func("__wolf_rt_main_err", err_sig);
    let report = |f: &mut Function, block, tag_val, name: Option<&str>| {
        let mut words = [0i64; 4];
        let mut len = 0i64;
        if let Some(name) = name {
            let bytes = name.as_bytes();
            len = bytes.len().min(32) as i64;
            for (j, &b) in bytes.iter().take(32).enumerate() {
                words[j / 8] |= (b as i64) << ((j % 8) * 8);
            }
        }
        let mut args = vec![tag_val];
        for imm in std::iter::once(len).chain(words) {
            let (_, v) = f.append_inst(block, Opcode::Iconst, &[], &[types::I64], Aux::Int(imm));
            args.push(v[0]);
        }
        f.append_inst(block, Opcode::Call, &args, &[], Aux::Callee(err_callee));
        // `__wolf_rt_main_err` exits; this return is the honest CFG
        // tail (exit 1 if the runtime ever returned).
        let (_, one) = f.append_inst(block, Opcode::Iconst, &[], &[types::I32], Aux::Int(1));
        f.append_inst(block, Opcode::Ret, &[one[0]], &[], Aux::None);
    };
    let mut cur = b_err;
    for (i, name) in tags.iter().enumerate() {
        let id = (i + 1) as i64;
        let b_hit = f.make_block(&[]);
        let b_next = f.make_block(&[]);
        let (_, k) = f.append_inst(cur, Opcode::Iconst, &[], &[types::I64], Aux::Int(id));
        let (_, eq) = f.append_inst(
            cur,
            Opcode::Icmp,
            &[tagv[0], k[0]],
            &[types::BOOL],
            Aux::IntCc(wolf_wir::ops::IntCc::Eq),
        );
        let hit_edge = f.block_call(b_hit, &[]);
        let next_edge = f.block_call(b_next, &[]);
        f.append_inst(cur, Opcode::Br, &[eq[0]], &[], Aux::Br(hit_edge, next_edge));
        report(&mut f, b_hit, tagv[0], Some(name));
        cur = b_next;
    }
    // Unknown tag (defensive): report the numeric id alone.
    report(&mut f, cur, tagv[0], None);
    Ok(m.add_func(f))
}

/// Drive a backend over a whole WIR module: declare everything, define
/// everything, emit the trap-info table. Functions get mangled local
/// symbols; `entry_shim` (from [`add_entry_shim`]) exports the
/// unmangled `main`.
pub fn compile_module(
    backend: &mut dyn Backend,
    m: &WirModule,
    entry_shim: Option<FuncId>,
) -> Result<(), BackendError> {
    for (id, f) in m.funcs.iter() {
        let (symbol, linkage) = if Some(id) == entry_shim {
            ("main".to_string(), Linkage::Export)
        } else if f.export {
            // The C membrane (s29, D19): unmangled, externally
            // visible, defined under the SysV plan.
            (f.name.clone(), Linkage::Export)
        } else {
            (mangle(m, &f.name, f.sig), Linkage::Local)
        };
        backend.declare_function(m, id, &f.name, &symbol, f.sig, linkage)?;
    }
    for (id, f) in m.funcs.iter() {
        backend.define_function(m, id, f, &mut wolf_backend::NullDebugSink)?;
    }
    backend.define_data(TRAP_TABLE_SYMBOL, &trap_table(), Linkage::Local)?;
    Ok(())
}

/// The trap-info table bytes: `[count][code u8, name…, NUL]…`.
fn trap_table() -> Vec<u8> {
    use wolf_rt::native::trap_code as tc;
    let codes = [
        tc::OVERFLOW,
        tc::DIV_ZERO,
        tc::BOUNDS,
        tc::ASSERT,
        tc::USE_AFTER_MOVE,
        tc::EXCLUSIVITY,
        tc::REGION_FAULT,
        tc::STALE_HANDLE,
        tc::ALLOC_CONTRACT,
        tc::RACE,
        tc::UB,
    ];
    let mut out = vec![codes.len() as u8];
    for c in codes {
        out.push(c as u8);
        out.extend_from_slice(wolf_rt::native::trap_kind_name(c).as_bytes());
        out.push(0);
    }
    out
}
