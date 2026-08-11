//! The backend interface (s28) — the PERMANENT artifact of c06.
//!
//! WIR in, relocatable objects out. `wolf_codegen_clif` (Cranelift, the
//! interim debug backend) and the owned Tier-F backend (c12) both
//! implement [`Backend`]; the driver programs against this trait and
//! branches on [`Capabilities`], never on backend identity (D1: the
//! Cranelift crate is scaffolding and is DELETED at c12 closeout — if
//! replacing it touches anything above this crate, s28 failed).
//!
//! This crate also owns [`layout`] — the aggregate layout rules shared
//! by every backend — and [`abi`] (s29): the call-boundary plans every
//! backend executes mechanically (wolf-native v0 + the SysV C
//! membrane). Both are language contracts, never a Cranelift-private
//! detail.
//!
//! Nothing here may depend on Cranelift — `cargo xtask deps-check`
//! holds the edge (`wir ← backend ← codegen_clif ← driver`).

pub mod abi;
pub mod layout;

use wolf_wir::ir::{FuncId, Function, Module, SigId};

/// How a symbol participates in linking.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Linkage {
    /// Defined here, visible to the linker (entry points, exports).
    Export,
    /// Defined here, module-internal.
    Local,
    /// Declared here, defined elsewhere (runtime shims, other objects).
    Import,
}

/// One entry of the finished object's symbol table.
#[derive(Clone, Debug)]
pub struct SymbolInfo {
    pub name: String,
    pub linkage: Linkage,
    /// True for functions, false for data.
    pub is_function: bool,
}

/// The finished product: relocatable object bytes plus the symbols the
/// backend defined or imported (the driver's link step consumes both).
#[derive(Clone, Debug)]
pub struct ObjectProduct {
    /// Relocatable ELF object bytes (linux/x86-64 in s28; D35's wider
    /// matrix is c13).
    pub bytes: Vec<u8>,
    pub symbols: Vec<SymbolInfo>,
}

/// A backend error: unsupported construct (honest refusal — the
/// conservatism ledger extends into codegen) or an internal fault.
#[derive(Clone, Debug)]
pub enum BackendError {
    /// The backend cannot compile this yet. Never a silent miscompile:
    /// the driver reports the construct and keeps the file's phase
    /// honest.
    Unsupported(String),
    /// A backend invariant broke — a compiler bug (ICE), never a
    /// verdict.
    Internal(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::Unsupported(s) => write!(f, "unsupported: {s}"),
            BackendError::Internal(s) => write!(f, "internal: {s}"),
        }
    }
}

impl std::error::Error for BackendError {}

/// What a backend can do — the driver branches on THESE, never on the
/// backend's name. Cranelift reports `false`/line-level; the owned
/// Tier-F backend (c12) flips them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Capabilities {
    /// In-place function patching (resident recompilation). Cranelift
    /// structurally cannot (report 04 §Needs-work); Tier-F can.
    pub supports_in_place_patching: bool,
    /// Debug-info fidelity this backend sustains.
    pub dwarf_fidelity: DwarfFidelity,
}

/// Debug-info fidelity tiers ([`Capabilities::dwarf_fidelity`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DwarfFidelity {
    /// No debug info.
    None,
    /// Line tables only (Cranelift's tier in s28; s30 rides this).
    Lines,
    /// Full variable-location fidelity (Tier-F's DWARF-first design).
    Full,
}

/// Where debug information flows OUT of a backend — part of the trait
/// from day one so Tier-F's DWARF-first design is not bolted on (s30
/// consumes; today's WIR carries no per-instruction spans yet, so the
/// hooks fire with what exists: function boundaries).
pub trait DebugSink {
    /// A function's definition begins (name is the pre-mangling WIR
    /// name; `symbol` the linker-visible one).
    fn function(&mut self, _func: FuncId, _name: &str, _symbol: &str) {}
    /// A source location for the instruction range starting at
    /// `code_offset` within the current function (s30: WIR spans flow
    /// through here into line tables).
    fn srcloc(&mut self, _code_offset: u32, _span_lo: u32, _span_hi: u32) {}
}

/// A [`DebugSink`] that drops everything (the default).
#[derive(Default)]
pub struct NullDebugSink;

impl DebugSink for NullDebugSink {}

/// The backend contract: WIR in, objects out.
///
/// Call order: `declare_function` for every function (so calls resolve
/// regardless of definition order), then `define_function` /
/// `define_data` in any order, then `finish` exactly once. `module`
/// carries the type interner and signature table every call needs;
/// `name` is the WIR-level name call sites reference, `symbol` the
/// linker-visible one (the caller decides mangling via [`mangle`] —
/// entry shims export unmangled symbols through the same path).
pub trait Backend {
    fn capabilities(&self) -> Capabilities;

    /// Declare a function ahead of definition. `sig` indexes the
    /// module's signature table.
    fn declare_function(
        &mut self,
        module: &Module,
        id: FuncId,
        name: &str,
        symbol: &str,
        sig: SigId,
        linkage: Linkage,
    ) -> Result<(), BackendError>;

    /// Lower one verified WIR function to code.
    fn define_function(
        &mut self,
        module: &Module,
        id: FuncId,
        func: &Function,
        debug: &mut dyn DebugSink,
    ) -> Result<(), BackendError>;

    /// Emit a data symbol (v0: the trap-info table; c06's data section
    /// for str/globals rides this same entry point).
    fn define_data(
        &mut self,
        name: &str,
        bytes: &[u8],
        linkage: Linkage,
    ) -> Result<(), BackendError>;

    /// Finish the module: all declared functions must be defined.
    fn finish(self: Box<Self>) -> Result<ObjectProduct, BackendError>;
}

/// Symbol mangling v0 (s28): `_W` + the WIR item name (module-path
/// qualified names keep their interior dots — ELF allows them) + `$` +
/// a 16-hex hash of the name, the rendered signature, and the native
/// convention version ([`abi::CONVENTION_VERSION`], s29): a convention
/// bump changes every symbol, so mixed-convention objects fail to LINK
/// rather than silently miscompile (D7). Deliberately NOT C-linkable:
/// the FFI membrane is explicit (`extern "c"`/`export` symbols are
/// emitted unmangled). Deterministic across runs — the hash is FNV-1a
/// over stable inputs, no interner indices.
pub fn mangle(module: &Module, name: &str, sig: SigId) -> String {
    mangle_versioned(module, name, sig, abi::CONVENTION_VERSION)
}

/// [`mangle`] with the convention version explicit (the D7 invariant's
/// test seam — production callers use [`mangle`]).
pub fn mangle_versioned(module: &Module, name: &str, sig: SigId, version: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    eat(version.as_bytes());
    eat(name.as_bytes());
    let data = &module.sigs[sig];
    for p in &data.params {
        eat(module.types.display(p.ty).as_bytes());
        eat(p.mode.keyword().unwrap_or("val").as_bytes());
    }
    for &r in &data.results {
        eat(module.types.display(r).as_bytes());
    }
    format!("_W{name}${h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use wolf_wir::ir::Param;
    use wolf_wir::types;

    /// The abstraction proof (contract Target 1): a second backend that
    /// counts and emits nothing must compile against the trait.
    struct NullBackend {
        declared: Vec<String>,
        defined: usize,
    }

    impl Backend for NullBackend {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                supports_in_place_patching: false,
                dwarf_fidelity: DwarfFidelity::None,
            }
        }

        fn declare_function(
            &mut self,
            _module: &Module,
            _id: FuncId,
            name: &str,
            _symbol: &str,
            _sig: SigId,
            _linkage: Linkage,
        ) -> Result<(), BackendError> {
            self.declared.push(name.to_string());
            Ok(())
        }

        fn define_function(
            &mut self,
            _module: &Module,
            _id: FuncId,
            _func: &Function,
            _debug: &mut dyn DebugSink,
        ) -> Result<(), BackendError> {
            self.defined += 1;
            Ok(())
        }

        fn define_data(
            &mut self,
            _name: &str,
            _bytes: &[u8],
            _linkage: Linkage,
        ) -> Result<(), BackendError> {
            Ok(())
        }

        fn finish(self: Box<Self>) -> Result<ObjectProduct, BackendError> {
            Ok(ObjectProduct {
                bytes: Vec::new(),
                symbols: Vec::new(),
            })
        }
    }

    #[test]
    fn null_backend_implements_the_trait() {
        let mut m = Module::new();
        let sig = m.make_sig(vec![Param::val(types::I64)], vec![types::I64]);
        let f = Function::new("id", sig);
        let fid = m.add_func(f);
        let mut b: Box<dyn Backend> = Box::new(NullBackend {
            declared: Vec::new(),
            defined: 0,
        });
        let sym = mangle(&m, "id", sig);
        b.declare_function(&m, fid, "id", &sym, sig, Linkage::Export)
            .unwrap();
        b.define_function(&m, fid, &m.funcs[fid], &mut NullDebugSink)
            .unwrap();
        let product = b.finish().unwrap();
        assert!(product.bytes.is_empty());
    }

    #[test]
    fn mangling_is_deterministic_and_distinguishes_sigs() {
        let mut m = Module::new();
        let s1 = m.make_sig(vec![Param::val(types::I64)], vec![types::I64]);
        let s2 = m.make_sig(vec![Param::val(types::I32)], vec![types::I64]);
        let a = mangle(&m, "f", s1);
        assert_eq!(a, mangle(&m, "f", s1));
        assert!(a.starts_with("_Wf$"));
        let mut seen = HashSet::new();
        assert!(seen.insert(mangle(&m, "f", s1)));
        assert!(seen.insert(mangle(&m, "f", s2)));
        assert!(seen.insert(mangle(&m, "g", s1)));
        // Dotted method names survive (ELF permits interior dots).
        assert!(mangle(&m, "Counter.bump", s1).starts_with("_WCounter.bump$"));
    }
}
