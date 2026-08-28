//! The s30 DWARF v0 builder — a [`DebugSink`] that turns the
//! backend's per-function debug stream into `.debug_*` sections via
//! gimli's write API (D6).
//!
//! This module is the PERMANENT half of the debug story: Cranelift
//! emits no DWARF itself (cg_clif precedent) — the backend reports
//! machine facts (code offsets, frame-slot offsets, sizes) through the
//! `DebugSink` seam, and THIS builder owns every DWARF decision, so
//! the c12 backend swap does not touch it.
//!
//! v0 coverage (the contract's honesty bar):
//! - **Line tables**: one sequence per function, statement-grain rows
//!   from WIR spans (byte-exact s07 chain), `is_stmt` on every row,
//!   `prologue_end` on the first — breakpoints land after frame setup.
//! - **Subprogram DIEs**: name (the WIR name — `main`, `geometry.area`;
//!   `break main` works by DW_AT_name), linkage name (mangled symbol),
//!   low/high pc, decl file/line, frame base = the frame pointer.
//! - **Variables**: scalar params and lets as
//!   `DW_TAG_formal_parameter`/`DW_TAG_variable` with whole-function
//!   `DW_OP_fbreg` locations (the s28 forced-spill decision; no
//!   live-range fidelity until s58's Tier-F).
//! - **Types**: the closed scalar set ([`DebugTy`]) as
//!   `DW_TAG_base_type`; aggregates/error unions are s31+ (recorded
//!   deltas, not silent gaps).
//!
//! Language code: `DW_LANG_C11`, deliberately NOT the contract's
//! `DW_LANG_lo_user` — a recorded s30 delta: gdb refuses to consume
//! subprogram/variable DIEs from a CU whose language it cannot map
//! (tested against gdb 17: `print x` and `break main` both dead under
//! `lo_user`, alive under C11). C-family semantics are exactly what
//! raw scalar DIEs need; the private constant returns when wolf ships
//! a debugger plugin or DWARF assigns a code (s54/s58).

use std::collections::HashMap;

use gimli::constants;
use gimli::write::{
    Address, AttributeValue, DwarfUnit, EndianVec, Expression, LineProgram, LineString, Relocation,
    RelocationTarget, Sections, UnitEntryId,
};
use gimli::{Encoding, Format, LittleEndian};

use crate::{DebugReloc, DebugRelocTarget, DebugSection, DebugSink, DebugTy};
use wolf_wir::entity::EntityRef;
use wolf_wir::ir::FuncId;

/// One source file the builder can map byte offsets into: display
/// path plus its line-start table (byte offset of each line's first
/// byte — `wolf_span::LineIndex` shape, passed as plain data so this
/// crate stays span-agnostic).
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: String,
    pub line_starts: Vec<u32>,
}

impl SourceFile {
    /// 1-based (line, column) of a byte offset. Public since s125: the
    /// trap-site rendering (`__wolf_rt_trap_at`'s immediates) resolves
    /// through the same table the line program does, so the site a
    /// trap names and the line a debugger shows can never disagree.
    pub fn line_col(&self, offset: u32) -> (u64, u64) {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(l) => l,
            Err(i) => i.saturating_sub(1),
        };
        let col = offset - self.line_starts.get(line).copied().unwrap_or(0);
        (line as u64 + 1, col as u64 + 1)
    }
}

#[derive(Clone, Debug)]
struct VarRec {
    name: String,
    ty: DebugTy,
    fbreg: i32,
    param: bool,
}

#[derive(Clone, Debug)]
struct FnRec {
    id: FuncId,
    name: String,
    /// The mangled linker symbol. NOT emitted as DW_AT_linkage_name at
    /// v0 (see the subprogram builder) — kept so s54's demangler story
    /// can flip it on without reshaping the sink.
    #[allow(dead_code)]
    symbol: String,
    /// (file index, decl span lo).
    span: Option<(u32, u32)>,
    /// (code offset, span lo) in offset order.
    locs: Vec<(u32, u32)>,
    vars: Vec<VarRec>,
    size: u32,
}

/// The DWARF v0 builder. Feed it as the `DebugSink` of a whole-module
/// compile, then [`DwarfBuilder::finish`] the collected stream into
/// [`DebugSection`]s for [`crate::Backend::add_debug_sections`].
pub struct DwarfBuilder {
    /// `wolf_span::FileId` index → source file.
    files: HashMap<u32, SourceFile>,
    comp_dir: String,
    producer: String,
    fns: Vec<FnRec>,
}

impl DwarfBuilder {
    /// `files` maps `wolf_span::FileId` indices (as the WIR carries
    /// them) to their paths and line tables; `comp_dir` is the
    /// compilation working directory (DW_AT_comp_dir).
    pub fn new(comp_dir: impl Into<String>, files: HashMap<u32, SourceFile>) -> DwarfBuilder {
        DwarfBuilder {
            files,
            comp_dir: comp_dir.into(),
            producer: format!("wolf v0 (wolfgang {})", env!("CARGO_PKG_VERSION")),
            fns: Vec::new(),
        }
    }

    fn cur(&mut self) -> Option<&mut FnRec> {
        self.fns.last_mut()
    }

    /// Serialize everything collected so far into `.debug_*` sections
    /// with symbol/section relocations. Deterministic for a fixed
    /// input stream.
    pub fn finish(&self) -> Result<Vec<DebugSection>, String> {
        self.build().map_err(|e| e.to_string())
    }

    fn build(&self) -> gimli::write::Result<Vec<DebugSection>> {
        let encoding = Encoding {
            format: Format::Dwarf32,
            version: 4,
            address_size: 8,
        };
        let mut dwarf = DwarfUnit::new(encoding);

        // ---- line program ------------------------------------------------
        let comp_dir_str = if self.comp_dir.is_empty() {
            ".".to_string()
        } else {
            self.comp_dir.clone()
        };
        // The primary file: the first function's file, else a stub.
        let primary = self
            .fns
            .iter()
            .find_map(|f| f.span.and_then(|(file, _)| self.files.get(&file)))
            .map(|f| f.path.clone())
            .unwrap_or_else(|| "<no-source>".to_string());
        let mut line_program = LineProgram::new(
            encoding,
            gimli::LineEncoding::default(),
            LineString::String(comp_dir_str.clone().into_bytes()),
            None,
            LineString::String(primary.clone().into_bytes()),
            None,
        );
        // Register every referenced file once; remember its id.
        let dir = line_program.default_directory();
        let mut file_ids: HashMap<u32, gimli::write::FileId> = HashMap::new();
        let mut file_indices: Vec<u32> = self.files.keys().copied().collect();
        file_indices.sort_unstable();
        for fi in file_indices {
            let sf = &self.files[&fi];
            let id =
                line_program.add_file(LineString::String(sf.path.clone().into_bytes()), dir, None);
            file_ids.insert(fi, id);
        }

        // One sequence per function with source info, addressed by the
        // function's symbol (the linker resolves final addresses).
        for f in &self.fns {
            let Some((file_idx, decl_lo)) = f.span else {
                continue;
            };
            let (Some(&file_id), Some(sf)) = (file_ids.get(&file_idx), self.files.get(&file_idx))
            else {
                continue;
            };
            if f.locs.is_empty() || f.size == 0 {
                continue;
            }
            line_program.begin_sequence(Some(Address::Symbol {
                symbol: f.id.index(),
                addend: 0,
            }));
            // The sequence must cover the function ENTRY: a gap at
            // low_pc (prologue + debug-slot spills carry no srcloc)
            // makes gdb ignore the line table for breakpoint placement
            // and fall back to raw prologue analysis — landing before
            // parameters are readable. Synthesize a decl-line row at
            // offset 0; `prologue_end` stays on the first REAL row, so
            // `break fn` lands after frame setup with params live.
            if f.locs[0].0 > 0 {
                let (line, column) = sf.line_col(decl_lo);
                let row = line_program.row();
                row.address_offset = 0;
                row.file = file_id;
                row.line = line;
                row.column = column;
                row.is_statement = false;
                row.prologue_end = false;
                line_program.generate_row();
            }
            let mut first = true;
            for &(off, lo) in &f.locs {
                let (line, column) = sf.line_col(lo);
                let row = line_program.row();
                row.address_offset = u64::from(off);
                row.file = file_id;
                row.line = line;
                row.column = column;
                // Statement-grain spans: every distinct row IS a
                // statement boundary (the lowering contract).
                row.is_statement = true;
                row.prologue_end = first;
                first = false;
                line_program.generate_row();
            }
            line_program.end_sequence(u64::from(f.size));
        }
        dwarf.unit.line_program = line_program;

        // ---- DIEs --------------------------------------------------------
        let root = dwarf.unit.root();
        {
            let cu = dwarf.unit.get_mut(root);
            cu.set(
                constants::DW_AT_producer,
                AttributeValue::String(self.producer.clone().into_bytes()),
            );
            // No assigned DWARF language for wolf yet: the contract's
            // private user constant.
            cu.set(
                constants::DW_AT_language,
                AttributeValue::Language(constants::DW_LANG_C11),
            );
            cu.set(
                constants::DW_AT_name,
                AttributeValue::String(primary.into_bytes()),
            );
            cu.set(
                constants::DW_AT_comp_dir,
                AttributeValue::String(comp_dir_str.into_bytes()),
            );
        }

        // Base-type DIEs, one per scalar shape used.
        let mut base_types: HashMap<DebugTy, UnitEntryId> = HashMap::new();
        let mut base_ty = |dwarf: &mut DwarfUnit, ty: DebugTy| -> UnitEntryId {
            if let Some(&id) = base_types.get(&ty) {
                return id;
            }
            let (name, enc, size) = match ty {
                DebugTy::I8 => ("i8", constants::DW_ATE_signed, 1),
                DebugTy::I16 => ("i16", constants::DW_ATE_signed, 2),
                DebugTy::I32 => ("i32", constants::DW_ATE_signed, 4),
                DebugTy::I64 => ("int", constants::DW_ATE_signed, 8),
                DebugTy::F32 => ("f32", constants::DW_ATE_float, 4),
                DebugTy::F64 => ("f64", constants::DW_ATE_float, 8),
                DebugTy::Bool => ("bool", constants::DW_ATE_boolean, 1),
                DebugTy::Ptr => ("ptr", constants::DW_ATE_address, 8),
            };
            let id = dwarf.unit.add(root, constants::DW_TAG_base_type);
            let die = dwarf.unit.get_mut(id);
            die.set(
                constants::DW_AT_name,
                AttributeValue::String(name.as_bytes().to_vec()),
            );
            die.set(constants::DW_AT_encoding, AttributeValue::Encoding(enc));
            die.set(constants::DW_AT_byte_size, AttributeValue::Udata(size));
            base_types.insert(ty, id);
            id
        };

        for f in &self.fns {
            let Some((file_idx, lo)) = f.span else {
                continue; // synthetic (entry shim): no DIE
            };
            let sub = dwarf.unit.add(root, constants::DW_TAG_subprogram);
            {
                let die = dwarf.unit.get_mut(sub);
                die.set(
                    constants::DW_AT_name,
                    AttributeValue::String(f.name.clone().into_bytes()),
                );
                // Deliberately NO DW_AT_linkage_name at v0: debuggers
                // that see one adopt it as the function's search/
                // display name (physname), and no debugger knows how
                // to demangle `_W…$hash` yet — `break main` must find
                // the wolf `main` by its source name. The mangled
                // symbol still lives in the ELF symtab; a wolf
                // demangler + linkage names return together (s54).
                die.set(
                    constants::DW_AT_low_pc,
                    AttributeValue::Address(Address::Symbol {
                        symbol: f.id.index(),
                        addend: 0,
                    }),
                );
                die.set(
                    constants::DW_AT_high_pc,
                    AttributeValue::Udata(u64::from(f.size)),
                );
                if let (Some(&fid), Some(sf)) = (file_ids.get(&file_idx), self.files.get(&file_idx))
                {
                    let (line, _) = sf.line_col(lo);
                    die.set(
                        constants::DW_AT_decl_file,
                        AttributeValue::FileIndex(Some(fid)),
                    );
                    die.set(constants::DW_AT_decl_line, AttributeValue::Udata(line));
                }
                // Debug tier: frame pointers on, frame base = the
                // host's frame-pointer register — DWARF reg 6 (%rbp)
                // on x86-64, reg 29 (x29) on aarch64. Host-cfg is the
                // honest spelling here: this builder describes code
                // the clif backend generated for the HOST (its own
                // gate enforces that), so the two can never disagree.
                let fp_reg = if cfg!(target_arch = "aarch64") {
                    gimli::Register(29)
                } else {
                    gimli::Register(6)
                };
                let mut fb = Expression::new();
                fb.op_reg(fp_reg);
                die.set(constants::DW_AT_frame_base, AttributeValue::Exprloc(fb));
            }
            for v in &f.vars {
                let tid = base_ty(&mut dwarf, v.ty);
                let tag = if v.param {
                    constants::DW_TAG_formal_parameter
                } else {
                    constants::DW_TAG_variable
                };
                let var = dwarf.unit.add(sub, tag);
                let die = dwarf.unit.get_mut(var);
                die.set(
                    constants::DW_AT_name,
                    AttributeValue::String(v.name.clone().into_bytes()),
                );
                die.set(constants::DW_AT_type, AttributeValue::UnitRef(tid));
                let mut loc = Expression::new();
                loc.op_fbreg(i64::from(v.fbreg));
                die.set(constants::DW_AT_location, AttributeValue::Exprloc(loc));
            }
        }

        // ---- serialize ---------------------------------------------------
        let mut sections = Sections::new(RelocSection::default());
        dwarf.write(&mut sections)?;
        let mut out = Vec::new();
        sections.for_each(|id, sec| -> gimli::write::Result<()> {
            if sec.writer.slice().is_empty() {
                return Ok(());
            }
            let relocs = sec
                .relocs
                .iter()
                .map(|r| DebugReloc {
                    offset: r.offset as u32,
                    size: r.size,
                    target: match r.target {
                        RelocationTarget::Symbol(s) => {
                            DebugRelocTarget::Func(FuncId::new(s as u32))
                        }
                        RelocationTarget::Section(sid) => DebugRelocTarget::Section(sid.name()),
                    },
                    addend: r.addend,
                })
                .collect();
            out.push(DebugSection {
                name: id.name(),
                data: sec.writer.slice().to_vec(),
                relocs,
            });
            Ok(())
        })?;
        Ok(out)
    }
}

impl DebugSink for DwarfBuilder {
    fn function(&mut self, func: FuncId, name: &str, symbol: &str) {
        self.fns.push(FnRec {
            id: func,
            name: name.to_string(),
            symbol: symbol.to_string(),
            span: None,
            locs: Vec::new(),
            vars: Vec::new(),
            size: 0,
        });
    }

    fn function_span(&mut self, file: u32, lo: u32, _hi: u32) {
        if let Some(f) = self.cur() {
            f.span = Some((file, lo));
        }
    }

    fn var(&mut self, name: &str, ty: DebugTy, fbreg_offset: i32, param: bool) {
        let rec = VarRec {
            name: name.to_string(),
            ty,
            fbreg: fbreg_offset,
            param,
        };
        if let Some(f) = self.cur() {
            f.vars.push(rec);
        }
    }

    fn srcloc(&mut self, code_offset: u32, span_lo: u32, _span_hi: u32) {
        if let Some(f) = self.cur() {
            f.locs.push((code_offset, span_lo));
        }
    }

    fn function_size(&mut self, size: u32) {
        if let Some(f) = self.cur() {
            f.size = size;
        }
    }
}

/// A gimli writer that records relocations (the cg_clif pattern):
/// symbol targets become function-address relocations, section targets
/// become section-relative offsets — both resolved by the backend when
/// it embeds the sections in the object.
#[derive(Clone, Debug)]
struct RelocSection {
    writer: EndianVec<LittleEndian>,
    relocs: Vec<Relocation>,
}

impl Default for RelocSection {
    fn default() -> Self {
        RelocSection {
            writer: EndianVec::new(LittleEndian),
            relocs: Vec::new(),
        }
    }
}

impl gimli::write::RelocateWriter for RelocSection {
    type Writer = EndianVec<LittleEndian>;

    fn writer(&self) -> &Self::Writer {
        &self.writer
    }

    fn writer_mut(&mut self) -> &mut Self::Writer {
        &mut self.writer
    }

    fn relocate(&mut self, relocation: Relocation) {
        self.relocs.push(relocation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_fn_builder() -> DwarfBuilder {
        let mut files = HashMap::new();
        files.insert(
            0,
            SourceFile {
                path: "hello.lu".to_string(),
                // Lines start at 0, 10, 20, 30.
                line_starts: vec![0, 10, 20, 30],
            },
        );
        let mut b = DwarfBuilder::new("/src", files);
        b.function(FuncId::new(0), "main", "_Wmain$0000000000000000");
        b.function_span(0, 10, 14);
        b.var("x", DebugTy::I64, -16, false);
        b.var("a", DebugTy::I64, -24, true);
        b.srcloc(0, 10, 14);
        b.srcloc(8, 21, 25);
        b.function_size(32);
        b
    }

    #[test]
    fn emits_line_and_info_sections_with_relocs() {
        let secs = one_fn_builder().finish().expect("dwarf builds");
        let names: Vec<&str> = secs.iter().map(|s| s.name).collect();
        assert!(names.contains(&".debug_line"), "sections: {names:?}");
        assert!(names.contains(&".debug_info"), "sections: {names:?}");
        assert!(names.contains(&".debug_abbrev"), "sections: {names:?}");
        // The line table addresses the function through its symbol —
        // exactly one sequence-start relocation.
        let line = secs.iter().find(|s| s.name == ".debug_line").unwrap();
        let sym_relocs = line
            .relocs
            .iter()
            .filter(|r| matches!(r.target, DebugRelocTarget::Func(_)))
            .count();
        assert_eq!(sym_relocs, 1, "one begin_sequence address");
        // .debug_info points at the function (low_pc) and its own
        // support sections.
        let info = secs.iter().find(|s| s.name == ".debug_info").unwrap();
        assert!(
            info.relocs
                .iter()
                .any(|r| matches!(r.target, DebugRelocTarget::Func(_))),
            "low_pc relocation present"
        );
    }

    #[test]
    fn line_col_is_one_based() {
        let sf = SourceFile {
            path: "x.lu".into(),
            line_starts: vec![0, 10, 20],
        };
        assert_eq!(sf.line_col(0), (1, 1));
        assert_eq!(sf.line_col(9), (1, 10));
        assert_eq!(sf.line_col(10), (2, 1));
        assert_eq!(sf.line_col(25), (3, 6));
    }

    #[test]
    fn deterministic_output() {
        let a = one_fn_builder().finish().unwrap();
        let b = one_fn_builder().finish().unwrap();
        let flat = |v: &[DebugSection]| -> Vec<(String, Vec<u8>)> {
            v.iter()
                .map(|s| (s.name.to_string(), s.data.clone()))
                .collect()
        };
        assert_eq!(flat(&a), flat(&b));
    }

    #[test]
    fn synthetic_functions_are_skipped() {
        let mut b = DwarfBuilder::new("/src", HashMap::new());
        b.function(FuncId::new(0), "__wolf_main_shim", "main");
        b.function_size(16);
        let secs = b.finish().expect("dwarf builds");
        // No file, no span: no line sequence, no subprogram — only the
        // bare CU skeleton.
        let line = secs.iter().find(|s| s.name == ".debug_line");
        if let Some(line) = line {
            assert!(
                line.relocs.is_empty(),
                "no sequences means no address relocations"
            );
        }
    }
}
