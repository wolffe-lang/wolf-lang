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
    Backend, BackendError, Capabilities, DebugRelocTarget, DebugSection, DebugSink, DwarfFidelity,
    Linkage, ObjectProduct, SymbolInfo, mangle,
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
    /// WIR FuncId index → object function id (s30: debug relocations
    /// name functions by WIR id; this map resolves them to symbols).
    by_id: HashMap<u32, cranelift_module::FuncId>,
    /// Runtime shims declared so far (`__wolf_rt_*`).
    rt: HashMap<&'static str, cranelift_module::FuncId>,
    /// C membrane imports declared so far (`c.*` callee names, s29).
    imports: HashMap<String, cranelift_module::FuncId>,
    /// Module data blobs defined so far (s31 str/data): WIR data index
    /// → object data id, lazily defined on first `data.addr`.
    data_ids: HashMap<u32, cranelift_module::DataId>,
    /// Source files trap sites resolve against (s125): FileId index →
    /// path + line starts. Empty (the default) keeps every trap
    /// site-less — the pre-s125 one-line report.
    site_files: HashMap<u32, wolf_backend::dwarf::SourceFile>,
    /// Per-file rodata path symbols (`_W.site.<idx>`) defined so far,
    /// lazily on the first trap site in that file.
    site_file_data: HashMap<u32, cranelift_module::DataId>,
    symbols: Vec<SymbolInfo>,
    /// CLIF text of every defined function, in definition order — the
    /// golden-snapshot surface (lowering changes are reviewed diffs).
    clif_texts: Vec<(String, String)>,
    /// DWARF sections handed over via `add_debug_sections`, embedded
    /// into the object at `finish` (s30).
    debug_sections: Vec<DebugSection>,
    fb_ctx: FunctionBuilderContext,
}

impl ClifBackend {
    /// A backend for the host target. s28 opened linux/x86-64 (M1);
    /// s59 widens to macOS/aarch64 (c13, D35's tier-1 matrix) —
    /// anything else is an honest refusal.
    pub fn new() -> Result<ClifBackend, BackendError> {
        let triple = target_lexicon::Triple::host();
        let supported = matches!(
            (&triple.architecture, &triple.operating_system),
            (
                target_lexicon::Architecture::X86_64,
                target_lexicon::OperatingSystem::Linux
            ) | (
                target_lexicon::Architecture::Aarch64(_),
                target_lexicon::OperatingSystem::Darwin(_)
            )
        );
        if !supported {
            return Err(BackendError::Environment(format!(
                "this host cannot run the native tier: native codegen targets \
                 linux/x86-64 and macOS/aarch64 (s28 + s59; the rest of D35's \
                 matrix is c13) — host: {triple}"
            )));
        }
        let mut flags = settings::builder();
        // Position-independent objects: host toolchains link PIE by
        // default (and arm64 Mach-O is PIC-only).
        flags
            .set("is_pic", "true")
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        // The debug tier keeps frame pointers (s30): frame base = %rbp
        // (x29 on arm64 — Apple MANDATES live frame pointers) for
        // DW_OP_fbreg variable locations, and debugger stack walks
        // work even where .eh_frame coverage is thin.
        flags
            .set("preserve_frame_pointers", "true")
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        let isa = cranelift_codegen::isa::lookup(triple)
            .map_err(|e| BackendError::Internal(e.to_string()))?
            .finish(settings::Flags::new(flags))
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        let mut builder =
            ObjectBuilder::new(isa, "wolf", cranelift_module::default_libcall_names())
                .map_err(|e| BackendError::Internal(e.to_string()))?;
        // NO cranelift-object .eh_frame (recorded s30 delta from the
        // contract): its absolute relocations produce DT_TEXTREL
        // warnings in PIE links, and with frame pointers preserved the
        // debugger's stack walks are already sound via the %rbp chain
        // (verified: gdb backtraces through wolf frames into libc).
        // PC-relative .eh_frame via gimli CFI is an s31/s58 follow-up;
        // wolf itself has no unwinding semantics (D30).
        builder.unwind_info(false);
        Ok(ClifBackend {
            module: ObjectModule::new(builder),
            funcs: HashMap::new(),
            by_id: HashMap::new(),
            rt: HashMap::new(),
            imports: HashMap::new(),
            data_ids: HashMap::new(),
            site_files: HashMap::new(),
            site_file_data: HashMap::new(),
            symbols: Vec::new(),
            clif_texts: Vec::new(),
            debug_sections: Vec::new(),
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

    fn source_files(&mut self, files: HashMap<u32, wolf_backend::dwarf::SourceFile>) {
        self.site_files = files;
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
        let si = translate::sig_info(module, sig, conv, self.module.isa().default_call_conv())?;
        let fid = self
            .module
            .declare_function(symbol, clif_linkage(linkage), &si.clif)
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        self.funcs
            .insert(name.to_string(), translate::FuncEntry { fid, sig });
        self.by_id.insert(id.as_u32(), fid);
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
        let si = translate::sig_info(module, sig, conv, self.module.isa().default_call_conv())?;
        let mut ctx = self.module.make_context();
        ctx.func.signature = si.clif.clone();
        ctx.func.name = UserFuncName::user(0, id.as_u32());
        let debug_slots;
        {
            let builder = FunctionBuilder::new(&mut ctx.func, &mut self.fb_ctx);
            debug_slots = translate::translate_function(
                builder,
                module,
                func,
                conv,
                &si,
                &mut self.module,
                &self.funcs,
                &mut self.rt,
                &mut self.imports,
                &mut self.data_ids,
                &self.site_files,
                &mut self.site_file_data,
            )?;
        }
        let symbol = self
            .module
            .declarations()
            .get_function_decl(fid)
            .linkage_name(fid)
            .into_owned();
        debug.function(id, &func.name, &symbol);
        // The function's source coordinates (s30): the file plus the
        // earliest span any of its instructions carries (the decl-site
        // span lowering seeds the cursor with). Synthetic functions
        // (entry shim, fixtures) have neither — no DIE, no line rows.
        if let Some(file) = func.src_file {
            let first_span = func
                .layout
                .iter()
                .flat_map(|&b| func.blocks[b].insts.iter())
                .find_map(|&i| func.srcspan(i));
            if let Some(sp) = first_span {
                debug.function_span(file, sp.lo, sp.hi);
            }
        }
        self.clif_texts
            .push((func.name.clone(), ctx.func.display().to_string()));
        self.module
            .define_function(fid, &mut ctx)
            .map_err(|e| BackendError::Internal(format!("{}: {e}", func.name)))?;
        // Post-compile debug facts: machine srclocs (the span rides in
        // the SourceLoc bits as the span's `lo`; `hi` recovered from
        // the WIR side table), frame-slot variable locations, and the
        // code size closing the line-table sequence.
        let compiled = ctx
            .compiled_code()
            .ok_or_else(|| BackendError::Internal("no compiled code after define".into()))?;
        let mut span_hi: HashMap<u32, u32> = HashMap::new();
        for &b in &func.layout {
            for &i in &func.blocks[b].insts {
                if let Some(sp) = func.srcspan(i) {
                    span_hi.entry(sp.lo).or_insert(sp.hi);
                }
            }
        }
        for loc in compiled.buffer.get_srclocs_sorted() {
            if loc.loc.is_default() {
                continue;
            }
            let lo = loc.loc.bits();
            let hi = span_hi.get(&lo).copied().unwrap_or(lo);
            debug.srcloc(loc.start, lo, hi);
        }
        if let Some(frame) = compiled.buffer.frame_layout() {
            let fp = frame.frame_to_fp_offset as i64;
            for (name, ty, slot, param) in &debug_slots {
                let off = frame.stackslots[*slot].offset as i64 - fp;
                let Ok(off) = i32::try_from(off) else {
                    continue;
                };
                debug.var(name, *ty, off, *param);
            }
        }
        debug.function_size(compiled.buffer.total_size());
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

    fn add_debug_sections(&mut self, sections: Vec<DebugSection>) -> Result<(), BackendError> {
        self.debug_sections = sections;
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<ObjectProduct, BackendError> {
        let mut product = self.module.finish();
        // Embed the DWARF sections (s30): section data first, then
        // relocations — function targets resolve through the WIR-id →
        // object-symbol map, section targets through the section's own
        // symbol. Absolute 64-bit (or 32-bit offset) relocations; the
        // system linker finalizes addresses.
        use cranelift_object::object::write::{Relocation, StandardSegment, Symbol, SymbolSection};
        use cranelift_object::object::{
            RelocationEncoding, RelocationFlags, RelocationKind, SymbolFlags, SymbolKind,
            SymbolScope,
        };
        // Section naming is format-aware (s59): ELF spells DWARF
        // sections `.debug_*`; Mach-O spells them `__debug_*` inside
        // the `__DWARF` segment (which `segment_name(Debug)` already
        // provides). The generic Absolute relocations below translate
        // to ARM64_RELOC_UNSIGNED there — verified on the emitted
        // objects.
        let is_macho = product.object.format() == cranelift_object::object::BinaryFormat::MachO;
        let sec_name = |name: &str| -> Vec<u8> {
            if is_macho {
                format!("__{}", name.trim_start_matches('.')).into_bytes()
            } else {
                name.as_bytes().to_vec()
            }
        };
        let mut sec_ids = HashMap::new();
        for sec in &self.debug_sections {
            let id = product.object.add_section(
                product.object.segment_name(StandardSegment::Debug).to_vec(),
                sec_name(sec.name),
                cranelift_object::object::SectionKind::Debug,
            );
            product.object.set_section_data(id, sec.data.clone(), 1);
            let sym = product.object.add_symbol(Symbol {
                name: sec_name(sec.name),
                value: 0,
                size: 0,
                kind: SymbolKind::Section,
                scope: SymbolScope::Compilation,
                weak: false,
                section: SymbolSection::Section(id),
                flags: SymbolFlags::None,
            });
            sec_ids.insert(sec.name, (id, sym));
        }
        // Mach-O (s59): dsymutil consumes debug relocations in the
        // APPLE ASSEMBLER convention — SECTION-BASED (non-extern)
        // entries whose embedded bytes hold the target's final object
        // address; it derives the covering symbol from that address,
        // validates against the debug map, and rewrites to the linked
        // address. Extern (symbol) relocations there are a trap: the
        // validate and rewrite passes disagree about whether the
        // embedded value is symbol-relative, so any nonzero function
        // offset lands wrong (measured both ways on this host's
        // dsymutil, not theorized). So on Mach-O function targets
        // relocate against their SECTION symbol and the emitted bytes
        // are PATCHED post-emit with the real object addresses; ELF
        // keeps plain symbol relocations for the system linker.
        let mut macho_patches: Vec<DebugPatch> = Vec::new();
        for sec in &self.debug_sections {
            let &(id, _) = sec_ids.get(sec.name).expect("just inserted");
            for r in &sec.relocs {
                let symbol = match r.target {
                    DebugRelocTarget::Func(fid) => {
                        let Some(&ofid) = self.by_id.get(&fid.as_u32()) else {
                            return Err(BackendError::Internal(format!(
                                "debug reloc names undeclared function {fid:?}"
                            )));
                        };
                        product.function_symbol(ofid)
                    }
                    DebugRelocTarget::Section(name) => match sec_ids.get(name) {
                        Some(&(_, sym)) => sym,
                        None => {
                            return Err(BackendError::Internal(format!(
                                "debug reloc names missing section {name}"
                            )));
                        }
                    },
                };
                let mut reloc_symbol = symbol;
                if is_macho {
                    // A function target embeds the symbol's final
                    // object address (looked up by name post-emit) and
                    // relocates against its SECTION symbol; a section
                    // target embeds the DWARF section-relative OFFSET
                    // — the addend alone (`None` marks it).
                    let sym_name = match r.target {
                        DebugRelocTarget::Func(_) => {
                            let sym = product.object.symbol(symbol);
                            let name = sym.name.clone();
                            if let cranelift_object::object::write::SymbolSection::Section(sid) =
                                sym.section
                            {
                                reloc_symbol = product.object.section_symbol(sid);
                            }
                            Some(name)
                        }
                        DebugRelocTarget::Section(_) => None,
                    };
                    macho_patches.push(DebugPatch {
                        section: String::from_utf8_lossy(&sec_name(sec.name)).into_owned(),
                        offset: r.offset,
                        size: r.size,
                        addend: r.addend,
                        symbol: sym_name,
                    });
                }
                product
                    .object
                    .add_relocation(
                        id,
                        Relocation {
                            offset: u64::from(r.offset),
                            symbol: reloc_symbol,
                            addend: r.addend,
                            flags: RelocationFlags::Generic {
                                kind: RelocationKind::Absolute,
                                encoding: RelocationEncoding::Generic,
                                size: r.size * 8,
                            },
                        },
                    )
                    .map_err(|e| BackendError::Internal(e.to_string()))?;
            }
        }
        let mut bytes = product
            .emit()
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        if !macho_patches.is_empty() {
            patch_macho_debug(&mut bytes, &macho_patches)?;
        }
        Ok(ObjectProduct {
            bytes,
            symbols: self.symbols,
        })
    }
}

/// One Mach-O debug-section fix-up (see `finish`): a `Some(symbol)`
/// target writes that symbol's final object address + addend at
/// `section[offset..offset+size]`; a `None` target writes the addend
/// alone (a DWARF section-relative offset, e.g. `DW_AT_stmt_list`).
struct DebugPatch {
    section: String,
    offset: u32,
    size: u8,
    addend: i64,
    symbol: Option<Vec<u8>>,
}

/// Resolve the collected Mach-O debug fix-ups into the emitted
/// object's bytes (see `finish` — the Apple convention embeds
/// resolved `__DWARF` addresses).
fn patch_macho_debug(bytes: &mut [u8], patches: &[DebugPatch]) -> Result<(), BackendError> {
    use object::read::{Object as _, ObjectSection as _, ObjectSymbol as _};
    let ice = |m: String| BackendError::Internal(m);
    let file = object::read::macho::MachOFile64::<object::Endianness, _>::parse(&*bytes)
        .map_err(|e| ice(format!("re-parse emitted Mach-O: {e}")))?;
    let mut sym_addrs: HashMap<Vec<u8>, u64> = HashMap::new();
    for sym in file.symbols() {
        sym_addrs.insert(sym.name_bytes().unwrap_or(b"").to_vec(), sym.address());
    }
    let mut writes: Vec<(usize, u8, u64)> = Vec::new();
    for p in patches {
        let sec = &p.section;
        let s = file
            .section_by_name_bytes(sec.as_bytes())
            .ok_or_else(|| ice(format!("emitted object lost section {sec}")))?;
        let (file_off, file_len) = s
            .file_range()
            .ok_or_else(|| ice(format!("section {sec} has no file data")))?;
        let value = match &p.symbol {
            Some(name) => sym_addrs
                .get(name)
                .copied()
                .ok_or_else(|| {
                    ice(format!(
                        "emitted object lost symbol {}",
                        String::from_utf8_lossy(name)
                    ))
                })?
                .wrapping_add(p.addend as u64),
            None => p.addend as u64,
        };
        let at = file_off + u64::from(p.offset);
        if u64::from(p.offset) + u64::from(p.size) > file_len {
            return Err(ice(format!("debug patch outside section {sec}")));
        }
        writes.push((at as usize, p.size, value));
    }
    // (Borrow of `bytes` through `file` ends here.)
    for (at, size, value) in writes {
        let le = value.to_le_bytes();
        bytes[at..at + size as usize].copy_from_slice(&le[..size as usize]);
    }
    Ok(())
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
    // The message speaks the SURFACE's type names (`int`, `!int`), not
    // the WIR's (`i64`, `!i64`) — a refusal naming types the programmer
    // cannot write is the compiler talking to itself (wolf-lang#103,
    // the discipline #39 asks for elsewhere).
    let refuse = |sd: &wolf_wir::ir::SigData| {
        Err(BackendError::Unsupported(format!(
            "entry signature is not `fn main()`, `fn main() -> int` or \
             `fn main() -> !int` (found {} param(s), {} result(s))",
            sd.params.len(),
            sd.results.len()
        )))
    };
    let sd = m.sigs[entry_sig].clone();
    if !sd.params.is_empty() || sd.results.len() > 1 {
        return refuse(&sd);
    }
    // s88 (wolf-lang#103): `fn main()` — no return at all — is the form
    // a newcomer writes first and the one every other lane already runs
    // (the checked rung and lupin both print and exit 0). The shim calls
    // it for its effects and hands the process a 0.
    if sd.results.is_empty() {
        let shim_sig = m.make_sig(vec![], vec![types::I32]);
        let mut f = Function::new("__wolf_main_shim", shim_sig);
        let b0 = f.make_block(&[]);
        let callee = f.import_func("main", entry_sig);
        f.append_inst(b0, Opcode::Call, &[], &[], Aux::Callee(callee));
        let (_, z) = f.append_inst(b0, Opcode::Iconst, &[], &[types::I32], Aux::Int(0));
        f.append_inst(b0, Opcode::Ret, &[z[0]], &[], Aux::None);
        return Ok(m.add_func(f));
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
/// unmangled `main`. `debug` receives the per-function debug stream
/// (s30) — pass [`wolf_backend::NullDebugSink`] to drop it.
pub fn compile_module(
    backend: &mut dyn Backend,
    m: &WirModule,
    entry_shim: Option<FuncId>,
    debug: &mut dyn DebugSink,
) -> Result<(), BackendError> {
    let all: Vec<FuncId> = m.funcs.keys().collect();
    compile_selected(backend, m, &all, entry_shim, true, false, debug)
}

/// [`compile_module`] restricted to a subset of the module's functions
/// — the s31 per-module object seam. Wolf functions are declared
/// `Export` when `cross_module` (other objects call them by mangled
/// symbol; calls to functions OUTSIDE the subset import the same
/// mangled names — see the translator's callee fallback), `Local` in a
/// single-object build. The trap-info table is emitted only where
/// `trap_table` (exactly one object per executable — the entry one).
pub fn compile_selected(
    backend: &mut dyn Backend,
    m: &WirModule,
    funcs: &[FuncId],
    entry_shim: Option<FuncId>,
    trap_table_here: bool,
    cross_module: bool,
    debug: &mut dyn DebugSink,
) -> Result<(), BackendError> {
    let wolf_linkage = if cross_module {
        Linkage::Export
    } else {
        Linkage::Local
    };
    for &id in funcs {
        let f = &m.funcs[id];
        let (symbol, linkage) = if Some(id) == entry_shim {
            ("main".to_string(), Linkage::Export)
        } else if f.export {
            // The C membrane (s29, D19): unmangled, externally
            // visible, defined under the SysV plan.
            (f.name.clone(), Linkage::Export)
        } else {
            (mangle(m, &f.name, f.sig), wolf_linkage)
        };
        backend.declare_function(m, id, &f.name, &symbol, f.sig, linkage)?;
    }
    for &id in funcs {
        backend.define_function(m, id, &m.funcs[id], debug)?;
    }
    if trap_table_here {
        backend.define_data(TRAP_TABLE_SYMBOL, &trap_table(), Linkage::Local)?;
    }
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
