//! The C module interface artifact — what crosses the importer
//! interface.
//!
//! This is the **permanent deliverable** of s46. The libclang-shaped
//! worker behind it is scaffolding to be burned (D17: the embedded C
//! frontend of c15 replaces it *behind this interface, unchanged*), so
//! the artifact is specified as if the worker did not exist: a decl
//! table, a target-parameterized type encoding, a macro table, per-decl
//! status with the refusal that produced it, and spans back into the
//! original headers so a diagnostic can point at the line.
//!
//! Nothing wolf-shaped lives here. The artifact describes C; the
//! compiler's opinions about how C becomes wolf live in [`crate::map`]
//! and can change without re-importing anything.

use crate::ctype::{CTypeArena, CTypeId, EnumDef, Record, TargetInfo};
use crate::refuse::{Demotion, Refusal, Status};

/// The serialized format's version. **Bumping this is a deliberate
/// commit with a migration note**, and it is part of the cache key, so
/// a bump invalidates every cached artifact rather than misreading one.
pub const FORMAT_VERSION: u32 = 1;

/// A location in an original header: an index into
/// [`Artifact::files`], plus line and column (1-based).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SourceLoc {
    pub file: u32,
    pub line: u32,
    pub col: u32,
}

/// C's linkage classes (c23-n3220 §6.2.2). The distinction is not
/// pedantry: an internal-linkage entity has **no link-time symbol**, so
/// importing it as a callable function would produce a program that
/// compiles and fails to link.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Linkage {
    /// A symbol the linker will find.
    External,
    /// `static` at file scope — a value or a type, never a symbol.
    Internal,
    /// Typedefs, tags, enum constants: no linkage at all.
    None,
}

impl Linkage {
    pub fn tag(self) -> &'static str {
        match self {
            Linkage::External => "external",
            Linkage::Internal => "internal",
            Linkage::None => "none",
        }
    }

    pub fn from_tag(t: &str) -> Option<Linkage> {
        Some(match t {
            "external" => Linkage::External,
            "internal" => Linkage::Internal,
            "none" => Linkage::None,
            _ => return None,
        })
    }
}

/// What kind of thing a declaration is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DeclKind {
    /// A function. `ty` is a [`CType::Func`](crate::ctype::CType::Func).
    Func {
        ty: CTypeId,
        /// `static inline`, or `inline` with no external definition:
        /// the body is in the header and there is no symbol to link.
        /// The worker is expected to emit an out-of-line companion (see
        /// [`Artifact::shims`]); until it does, the declaration is
        /// extern-only.
        inline_only: bool,
    },
    /// A global object (`extern int errno;`).
    Object { ty: CTypeId },
    /// A `typedef`.
    Typedef { ty: CTypeId },
    /// A struct/union/enum tag.
    Tag { ty: CTypeId },
    /// An enumerator, with the value the target resolved it to.
    EnumConst { ty: CTypeId, value: i128 },
}

impl DeclKind {
    pub fn tag(&self) -> &'static str {
        match self {
            DeclKind::Func { .. } => "fn",
            DeclKind::Object { .. } => "object",
            DeclKind::Typedef { .. } => "typedef",
            DeclKind::Tag { .. } => "tag",
            DeclKind::EnumConst { .. } => "enumconst",
        }
    }

    pub fn ty(&self) -> CTypeId {
        match self {
            DeclKind::Func { ty, .. }
            | DeclKind::Object { ty }
            | DeclKind::Typedef { ty }
            | DeclKind::Tag { ty }
            | DeclKind::EnumConst { ty, .. } => *ty,
        }
    }
}

/// One imported declaration.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Decl {
    /// The C name (`malloc`), not a wolf spelling.
    pub name: String,
    /// How this name is spelled inside the `c` namespace. Usually the
    /// same as `name`; `struct_stat` when the tag `stat` collides with
    /// the function `stat` (c23-n3220 §6.2.3 — C's separate name spaces
    /// have to become one wolf namespace somehow, and renaming
    /// *visibly* beats picking a winner).
    pub wolf_name: String,
    pub kind: DeclKind,
    pub linkage: Linkage,
    pub loc: SourceLoc,
    pub status: Status,
}

impl Decl {
    /// Is this declaration usable at a wolf call site at all?
    pub fn usable(&self) -> bool {
        self.status.usable()
    }
}

/// A C constant value an object-like macro resolved to.
#[derive(Clone, PartialEq, Debug)]
pub enum ConstValue {
    Int {
        value: i128,
        ty: CTypeId,
    },
    Float {
        value: f64,
        ty: CTypeId,
    },
    /// A string literal, with its bytes (no NUL — the NUL is C's, and
    /// the mapping adds it).
    Str(Vec<u8>),
    Char {
        value: i64,
        ty: CTypeId,
    },
}

/// A macro, as the artifact records it. Macros stay **alive**: an
/// object-like macro that parses as a constant expression gets a value
/// here, and a function-like macro keeps its parameters and its token
/// sequence so the worker can re-expand it in the original TU context
/// at a wolf call site.
#[derive(Clone, PartialEq, Debug)]
pub enum MacroKind {
    Object {
        /// `Some` when the expansion is a C constant expression.
        value: Option<ConstValue>,
        /// The raw token sequence, always kept — the value is a
        /// convenience, the tokens are the truth.
        tokens: Vec<String>,
    },
    Function {
        params: Vec<String>,
        variadic: bool,
        tokens: Vec<String>,
    },
}

impl MacroKind {
    pub fn tag(&self) -> &'static str {
        match self {
            MacroKind::Object { .. } => "object",
            MacroKind::Function { .. } => "function",
        }
    }

    pub fn tokens(&self) -> &[String] {
        match self {
            MacroKind::Object { tokens, .. } | MacroKind::Function { tokens, .. } => tokens,
        }
    }
}

/// One macro definition.
#[derive(Clone, PartialEq, Debug)]
pub struct MacroDef {
    pub name: String,
    pub kind: MacroKind,
    pub loc: SourceLoc,
    pub status: Status,
}

/// A companion object the worker wants built and linked: out-of-line
/// definitions for `static inline` functions that were actually used.
/// This is the hole bindgen never closed — a header-only `inline` has
/// no symbol, so a binding generator that only reads declarations
/// produces a link error at the end of a long build.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ShimRequest {
    /// The C function the shim gives a body to.
    pub function: String,
    /// The generated C source. Compiled by the same toolchain that
    /// answered the import, never by a build script the package
    /// authored (D33: the recipe is declarative, the compiler drives).
    pub source: String,
}

/// A C module interface artifact: one import request's whole answer.
#[derive(Clone, PartialEq, Debug)]
pub struct Artifact {
    pub format_version: u32,
    /// Who produced this, and at what version — part of the cache key,
    /// so swapping the worker re-imports rather than trusting a stale
    /// answer.
    pub importer: String,
    /// The target every size, alignment and width in here was resolved
    /// for.
    pub target: TargetInfo,
    /// The header names as requested, in order.
    pub headers: Vec<String>,
    /// Header paths [`SourceLoc::file`] indexes into.
    pub files: Vec<String>,
    pub types: CTypeArena,
    pub records: Vec<Record>,
    pub enums: Vec<EnumDef>,
    /// Sorted by `wolf_name` — the artifact is byte-deterministic, and
    /// declaration order in a header is not a thing we want to depend
    /// on.
    pub decls: Vec<Decl>,
    /// Sorted by name.
    pub macros: Vec<MacroDef>,
    pub shims: Vec<ShimRequest>,
}

impl Artifact {
    /// An empty artifact for `target`, at the current format version.
    pub fn new(importer: impl Into<String>, target: TargetInfo) -> Artifact {
        Artifact {
            format_version: FORMAT_VERSION,
            importer: importer.into(),
            target,
            headers: Vec::new(),
            files: Vec::new(),
            types: CTypeArena::new(),
            records: Vec::new(),
            enums: Vec::new(),
            decls: Vec::new(),
            macros: Vec::new(),
            shims: Vec::new(),
        }
    }

    /// Put the artifact in canonical order. Two workers that saw the
    /// same headers must produce the same bytes, or the conformance
    /// suite compares noise.
    pub fn canonicalize(&mut self) {
        self.decls.sort_by(|a, b| a.wolf_name.cmp(&b.wolf_name));
        self.macros.sort_by(|a, b| a.name.cmp(&b.name));
        self.shims.sort_by(|a, b| a.function.cmp(&b.function));
    }

    pub fn decl(&self, wolf_name: &str) -> Option<&Decl> {
        self.decls
            .binary_search_by(|d| d.wolf_name.as_str().cmp(wolf_name))
            .ok()
            .map(|i| &self.decls[i])
    }

    pub fn macro_(&self, name: &str) -> Option<&MacroDef> {
        self.macros
            .binary_search_by(|m| m.name.as_str().cmp(name))
            .ok()
            .map(|i| &self.macros[i])
    }

    /// Every refusal in the artifact, as `(what, demotion, refusal)`.
    /// `wolf audit` counts these per dependency: an import that refuses
    /// half a header is a fact about a dependency, not a private
    /// detail of the build.
    pub fn refusals(&self) -> Vec<(&str, Demotion, &Refusal)> {
        let mut out = Vec::new();
        for d in &self.decls {
            if let Status::Refused { demotion, refusal } = &d.status {
                out.push((d.wolf_name.as_str(), *demotion, refusal));
            }
        }
        for m in &self.macros {
            if let Status::Refused { demotion, refusal } = &m.status {
                out.push((m.name.as_str(), *demotion, refusal));
            }
        }
        out
    }

    /// `(ok, refused)` counts — the headline `wolf c-import` prints and
    /// `wolf audit` diffs.
    pub fn tally(&self) -> (usize, usize) {
        let total = self.decls.len() + self.macros.len();
        let refused = self.refusals().len();
        (total - refused, refused)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctype::{CType, IntSpelling, Qual};

    fn tiny() -> Artifact {
        let t = TargetInfo::x86_64_linux();
        let mut a = Artifact::new("test", t.clone());
        a.headers.push("stdlib.h".to_string());
        a.files.push("/usr/include/stdlib.h".to_string());
        let vp = a.types.void_ptr();
        let sz = a.types.size_t(&t);
        let malloc = a.types.intern(CType::Func {
            ret: vp,
            params: vec![sz],
            variadic: false,
        });
        a.decls.push(Decl {
            name: "malloc".into(),
            wolf_name: "malloc".into(),
            kind: DeclKind::Func {
                ty: malloc,
                inline_only: false,
            },
            linkage: Linkage::External,
            loc: SourceLoc {
                file: 0,
                line: 540,
                col: 14,
            },
            status: Status::Ok,
        });
        a.decls.push(Decl {
            name: "atof".into(),
            wolf_name: "atof".into(),
            kind: DeclKind::Func {
                ty: malloc,
                inline_only: false,
            },
            linkage: Linkage::External,
            loc: SourceLoc::default(),
            status: Status::refuse(Refusal::LongDouble),
        });
        let int_ = a.types.intern(CType::Int {
            bits: 32,
            signed: true,
            spelling: IntSpelling::Int,
        });
        a.macros.push(MacroDef {
            name: "EOF".into(),
            kind: MacroKind::Object {
                value: Some(ConstValue::Int {
                    value: -1,
                    ty: int_,
                }),
                tokens: vec!["-".into(), "1".into()],
            },
            loc: SourceLoc::default(),
            status: Status::Ok,
        });
        let _ = a.types.ptr_to(vp, Qual::default());
        a.canonicalize();
        a
    }

    #[test]
    fn lookup_needs_canonical_order() {
        let a = tiny();
        assert!(a.decl("malloc").is_some());
        assert!(a.decl("atof").is_some());
        assert!(a.decl("nope").is_none());
        assert!(a.macro_("EOF").is_some());
    }

    /// The tally is what `wolf audit` diffs across an upgrade: a
    /// dependency whose imports start refusing more is a visible event.
    #[test]
    fn refusals_are_counted_not_hidden() {
        let a = tiny();
        let (ok, refused) = a.tally();
        assert_eq!((ok, refused), (2, 1));
        let r = a.refusals();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0, "atof");
        assert_eq!(r[0].2.tag(), "long-double");
    }

    /// One cursed declaration must not cost its siblings anything.
    #[test]
    fn a_refused_decl_leaves_its_siblings_usable() {
        let a = tiny();
        assert!(a.decl("malloc").expect("present").usable());
        assert!(!a.decl("atof").expect("present").usable());
    }
}
