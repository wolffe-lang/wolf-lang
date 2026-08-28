//! Release backend (Tier-R): WIR → LLVM IR → object (c09, s41–s45).
//!
//! Sits behind [`wolf_backend::Backend`] — the same seam the Cranelift
//! debug tier sits behind; the driver branches on capabilities, never
//! identity (D1). The two tiers share WIR-in and the `wolf_backend`
//! layout/ABI plans; they diverge at instruction selection.
//!
//! # The rented-backend seam (s41 recorded delta)
//!
//! D1 rents LLVM. The contract recommended `inkwell`; this
//! implementation instead emits **textual LLVM IR** and drives the
//! system `clang` (`clang -x ir -c -O2`) to object bytes. Recorded
//! rationale, by velocity per the contract's own rule:
//! - D33 (no build scripts, dependency-thin) is preserved exactly —
//!   zero new crate dependencies; `llvm-sys`/`inkwell` both carry
//!   build scripts and hard-pin LLVM majors.
//! - The toolchain requirement is *one binary already required by the
//!   link step's ecosystem*: `clang` ≥ [`CLANG_MIN_MAJOR`] on PATH
//!   (hosts with LLVM newer than any binding crate supports — e.g.
//!   LLVM 22 — work day one).
//! - Textual IR is the *reviewable artifact* the snapshot story wants:
//!   per-fact goldens diff as plain text, and byte-identical output
//!   for identical input is trivially auditable.
//!
//! Everything above this crate is inkwell-compatible: swapping the
//! text emitter for a builder API touches only this crate (the s28
//! seam holds).
//!
//! # LLVM version pin + bump policy
//!
//! [`CLANG_MIN_MAJOR`] is the single pinned floor. Bumping it (or the
//! CI-provisioned LLVM major) happens in its own commit, and only
//! after a clean LONG fuzz run of the metadata differential rig
//! (`tests/fact_fuzz.rs` with `WOLF_FACT_FUZZ_N` raised) — the
//! Rust-noalias lesson, mechanized as policy.
//!
//! # The fact-disposition ledger — the RELEASE column (s28's table)
//!
//! The debug tier's ledger (wolf_codegen_clif) records what Tier-F
//! *erases*; this column records what Tier-R *exploits*:
//!
//! | WIR carrier            | Disposition at LLVM tier                  |
//! |------------------------|-------------------------------------------|
//! | `noalias a b`          | USED: `mut`/`read` param modes lower to    |
//! |                        | `noalias`(/`readonly`) `noundef` argument  |
//! |                        | attributes; cross-region disjointness      |
//! |                        | rides the scope metadata below             |
//! | `deref p N`            | USED: `dereferenceable(N)` at the def      |
//! |                        | site; allocation call sites carry it as a  |
//! |                        | return attribute                           |
//! | `range v lo..=hi`      | USED: `!range` on integer loads; checked-  |
//! |                        | arith postconditions ride the overflow-    |
//! |                        | checked CFG itself (no metadata needed)    |
//! | `region p rN`          | USED: one `!alias.scope` domain per        |
//! |                        | function, one scope per region CLASS (s80: |
//! |                        | every same-role `region.foreign` root      |
//! |                        | shares one); every load/store lists all    |
//! |                        | other classes in `!noalias`, and every     |
//! |                        | CALL lists the classes it provably cannot  |
//! |                        | reach (s83 — interprocedural-strength      |
//! |                        | disjointness, reports/01 fact 2)           |
//! | `frozen p`             | USED: `!invariant.load` on loads through   |
//! |                        | `sync.freeze` tokens / frozen pointers     |
//! | allocation alignment   | USED: `align 16` on `region.alloc`/        |
//! | (report 10 delta 2)    | `stack.alloc` pointers; natural `align`    |
//! |                        | on every scalar access                     |
//! | effect tokens          | ERASED to program order + the scope        |
//! | (`mem.rN`, `io`)       | metadata (the order was the semantics;     |
//! |                        | the scopes are the licence LLVM needs)     |
//! | trap kinds             | KEPT: cold out-of-line blocks calling      |
//! |                        | `__wolf_rt_trap(kind)` + `unreachable`;    |
//! |                        | cold `!prof` weights on every trap and     |
//! |                        | error-row arc (report 10 delta 3);         |
//! |                        | verdicts match the debug tier exactly (X3  |
//! |                        | is profile-independent)                    |
//! | call purity bits       | NOT YET: WIR carries no purity facts at    |
//! |                        | v0 — `memory(...)`/`willreturn` land when  |
//! |                        | s42's analyses mint them                   |
//!
//! Metadata is the BONUS channel (D2): our own mid-end (s42) exploits
//! the same facts in WIR, so LLVM dropping any of this costs speed,
//! never correctness. *Wrong* metadata miscompiles — which is why
//! [`EmitOptions::strip_facts`] exists and the differential fuzz rig
//! compares stripped vs annotated lowerings at -O0/-O2/-O3
//! (`tests/fact_fuzz.rs`; the s44 sentinel lane reuses it).
//!
//! # No unwinding (D30)
//!
//! Every define and declare is `nounwind`; there is no code path in
//! this crate that can emit `invoke`, `landingpad`, or a personality —
//! and a CI test greps every emitted module to hold it that way.

mod emit;
pub mod fuzzgen;

use std::collections::{HashMap, HashSet};
use std::process::Command;

use wolf_backend::{
    Backend, BackendError, Capabilities, DebugSink, DwarfFidelity, Linkage, ObjectProduct,
    SymbolInfo,
};
use wolf_wir::ir::{FuncId, Function, Module as WirModule, SigId};

pub use emit::{BranchWeights, EmitOptions};

/// Minimum `clang` major this backend accepts — the one pinned place
/// (bump policy in the crate docs).
pub const CLANG_MIN_MAJOR: u32 = 15;

/// The target datalayout/triple this tier emits (s41 is linux/x86-64
/// only, like the debug tier at s28; the matrix is c13).
const TARGET_TRIPLE: &str = "x86_64-unknown-linux-gnu";
const DATALAYOUT: &str =
    "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128";

/// The LLVM implementation of the backend trait.
pub struct LlvmBackend {
    cx: emit::ModuleCx,
    /// WIR name → declared entry: how call sites resolve.
    funcs: HashMap<String, emit::FuncEntry>,
    /// Symbols DEFINED in this module (they never get `declare`s).
    defined: HashSet<String>,
    /// Finished `define`s, in definition order.
    func_irs: Vec<(String, String)>,
    /// Data globals defined through [`Backend::define_data`].
    data_globals: Vec<String>,
    symbols: Vec<SymbolInfo>,
    clang: String,
    /// The optimization level handed to clang (`-O2` per the contract;
    /// s42 shrinks what we hand the pipeline, s44 measures it).
    opt: &'static str,
}

impl LlvmBackend {
    /// A release backend for the host target with the default fact
    /// emission. s41 is linux/x86-64 only; anything else — including a
    /// missing or too-old system LLVM — is an HONEST refusal naming
    /// the requirement (D33: system LLVM is a documented toolchain
    /// requirement, provisioned by the CI lane, never a build script).
    pub fn new() -> Result<LlvmBackend, BackendError> {
        Self::with_options(EmitOptions::default())
    }

    /// [`LlvmBackend::new`] with explicit emission options (the fuzz
    /// rig's metadata-stripped control lane).
    pub fn with_options(opts: EmitOptions) -> Result<LlvmBackend, BackendError> {
        if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            return Err(BackendError::Unsupported(
                "the release tier targets linux/x86-64 only in s41 (c13 owns the \
                 matrix; the debug tier crossed to macOS/aarch64 at s59 — the \
                 release tier follows as its own c13 sprint)"
                    .to_string(),
            ));
        }
        let clang = clang_binary();
        match clang_major(&clang) {
            Some(v) if v >= CLANG_MIN_MAJOR => {}
            Some(v) => {
                return Err(BackendError::Unsupported(format!(
                    "`{clang}` is LLVM {v}; the release tier requires clang >= {CLANG_MIN_MAJOR} \
                     (system LLVM is a toolchain requirement — see wolf_codegen_llvm docs)"
                )));
            }
            None => {
                return Err(BackendError::Unsupported(format!(
                    "`{clang}` not found — the release tier requires clang >= {CLANG_MIN_MAJOR} \
                     on PATH (or WOLF_CLANG); system LLVM is a documented toolchain \
                     requirement, never a build script (D33)"
                )));
            }
        }
        Ok(LlvmBackend {
            cx: emit::ModuleCx::new(opts),
            funcs: HashMap::new(),
            defined: HashSet::new(),
            func_irs: Vec::new(),
            data_globals: Vec::new(),
            symbols: Vec::new(),
            clang,
            opt: "-O2",
        })
    }

    /// The complete module IR assembled from everything defined so far
    /// — the `--emit=llvm-ir` surface and the snapshot/fuzz substrate.
    /// Pure text: byte-identical input yields byte-identical IR.
    pub fn module_ir(&self) -> String {
        let mut out = String::new();
        out.push_str("; wolf release tier (s41) - WIR -> LLVM IR\n");
        out.push_str("source_filename = \"wolf\"\n");
        out.push_str(&format!("target datalayout = \"{DATALAYOUT}\"\n"));
        out.push_str(&format!("target triple = \"{TARGET_TRIPLE}\"\n\n"));
        for g in &self.cx.globals {
            out.push_str(g);
            out.push('\n');
        }
        for g in &self.data_globals {
            out.push_str(g);
            out.push('\n');
        }
        if !self.cx.globals.is_empty() || !self.data_globals.is_empty() {
            out.push('\n');
        }
        for (_, ir) in &self.func_irs {
            out.push_str(ir);
            out.push('\n');
        }
        for (sym, decl) in &self.cx.decls {
            if !self.defined.contains(sym) {
                out.push_str(decl);
                out.push('\n');
            }
        }
        for d in &self.cx.intrinsics {
            out.push_str(d);
            out.push('\n');
        }
        if !self.cx.meta.is_empty() {
            out.push('\n');
            for (i, m) in self.cx.meta.iter().enumerate() {
                out.push_str(&format!("!{i} = {m}\n"));
            }
        }
        out
    }

    /// The IR of every defined function, in definition order (the
    /// golden-snapshot surface, mirroring the debug tier's
    /// `clif_texts`).
    pub fn ir_texts(&self) -> &[(String, String)] {
        &self.func_irs
    }
}

/// The clang binary: `WOLF_CLANG` wins, `clang` on PATH otherwise.
fn clang_binary() -> String {
    std::env::var("WOLF_CLANG")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "clang".to_string())
}

/// Probe `clang --version` for the major version.
fn clang_major(clang: &str) -> Option<u32> {
    let out = Command::new(clang).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // "... clang version NN.N.N ..." (possibly prefixed by a distro).
    let idx = text.find("clang version ")?;
    let rest = &text[idx + "clang version ".len()..];
    let major: String = rest.chars().take_while(char::is_ascii_digit).collect();
    major.parse().ok()
}

impl Backend for LlvmBackend {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_in_place_patching: false,
            // Release-tier debug info is a later story (s58 territory);
            // v0 is honest: none. The driver's DWARF builder gates on
            // this, so no dead sections are built.
            dwarf_fidelity: DwarfFidelity::None,
        }
    }

    fn source_files(
        &mut self,
        files: std::collections::HashMap<u32, wolf_backend::dwarf::SourceFile>,
    ) {
        self.cx.site_files = files;
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
        let _ = sig;
        let export = module.funcs.get(id).is_some_and(|f| f.export);
        self.funcs.insert(
            name.to_string(),
            emit::FuncEntry {
                symbol: symbol.to_string(),
                export,
            },
        );
        if linkage != Linkage::Import {
            self.defined.insert(symbol.to_string());
        }
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
        _id: FuncId,
        func: &Function,
        _debug: &mut dyn DebugSink,
    ) -> Result<(), BackendError> {
        let Some(entry) = self.funcs.get(&func.name) else {
            return Err(BackendError::Internal(format!(
                "function `{}` defined before being declared",
                func.name
            )));
        };
        let symbol = entry.symbol.clone();
        let linkage = if self
            .symbols
            .iter()
            .any(|s| s.name == symbol && s.linkage == Linkage::Local)
        {
            "internal "
        } else {
            ""
        };
        let ir = emit::emit_function(
            &mut self.cx,
            module,
            func,
            &symbol,
            linkage,
            &self.funcs,
            &self.defined,
        )?;
        self.func_irs.push((func.name.clone(), ir));
        Ok(())
    }

    fn define_data(
        &mut self,
        name: &str,
        bytes: &[u8],
        linkage: Linkage,
    ) -> Result<(), BackendError> {
        let vis = match linkage {
            Linkage::Export => "",
            Linkage::Local => "internal ",
            Linkage::Import => {
                return Err(BackendError::Internal(
                    "define_data with Import linkage".to_string(),
                ));
            }
        };
        self.data_globals.push(format!(
            "@\"{name}\" = {vis}constant [{} x i8] {}, align 8",
            bytes.len().max(1),
            data_const(bytes),
        ));
        self.defined.insert(name.to_string());
        self.symbols.push(SymbolInfo {
            name: name.to_string(),
            linkage,
            is_function: false,
        });
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<ObjectProduct, BackendError> {
        let ir = self.module_ir();
        let dir = std::env::temp_dir().join(format!(
            "wolf-llvm-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir)
            .map_err(|e| BackendError::Internal(format!("create {}: {e}", dir.display())))?;
        let ll = dir.join("wolf.ll");
        let obj = dir.join("wolf.o");
        std::fs::write(&ll, &ir)
            .map_err(|e| BackendError::Internal(format!("write {}: {e}", ll.display())))?;
        // -x ir: the module carries its own triple/datalayout;
        // -fPIC: host toolchains link PIE by default (matches the
        // debug tier's is_pic).
        let out = Command::new(&self.clang)
            .arg("-x")
            .arg("ir")
            .arg(&ll)
            .arg("-c")
            .arg(self.opt)
            .arg("-fPIC")
            .arg("-Wno-override-module")
            .arg("-o")
            .arg(&obj)
            .output()
            .map_err(|e| BackendError::Internal(format!("cannot run `{}`: {e}", self.clang)))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let _ = std::fs::remove_dir_all(&dir);
            return Err(BackendError::Internal(format!(
                "clang rejected the emitted IR (a lowering bug, never a verdict):\n{stderr}"
            )));
        }
        let bytes = std::fs::read(&obj)
            .map_err(|e| BackendError::Internal(format!("read {}: {e}", obj.display())))?;
        let _ = std::fs::remove_dir_all(&dir);
        Ok(ObjectProduct {
            bytes,
            symbols: self.symbols,
        })
    }
}

/// Render bytes for a data global.
fn data_const(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "zeroinitializer".to_string();
    }
    let mut s = String::with_capacity(bytes.len() + 4);
    s.push_str("c\"");
    for &b in bytes {
        match b {
            b'"' | b'\\' => {
                s.push_str(&format!("\\{b:02X}"));
            }
            0x20..=0x7e => s.push(b as char),
            _ => {
                s.push_str(&format!("\\{b:02X}"));
            }
        }
    }
    s.push('"');
    s
}
