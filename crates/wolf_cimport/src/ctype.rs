//! The target-parameterized C type encoding.
//!
//! What crosses the importer interface is **C types with the target's
//! answers already substituted in** — not wolf types. `long` is not
//! "long", it is "a signed integer of 64 bits" once the triple says so;
//! a struct is a size, an alignment and a list of field offsets, not a
//! layout algorithm to re-run. Two reasons:
//!
//! 1. The worker has the target's layout facts (it is the thing that
//!    read the headers for that triple); the compiler does not, and
//!    should never be a second, disagreeing implementation of C's
//!    layout rules.
//! 2. The C-type → wolf-type mapping stays compiler-side (see
//!    [`crate::map`]) so it can change — new niches, a better pointer
//!    story — **without re-importing a single header**. The artifact
//!    is about C; only the compiler has opinions about wolf.
//!
//! Types live in a [`CTypeArena`] and are named by [`CTypeId`]. The
//! arena is append-only and its indices are part of the serialized
//! format, so a worker must emit them in a deterministic order.

use std::collections::BTreeMap;

/// A handle into a [`CTypeArena`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct CTypeId(pub u32);

/// A handle into [`Artifact::records`](crate::artifact::Artifact::records).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct RecordId(pub u32);

/// A handle into [`Artifact::enums`](crate::artifact::Artifact::enums).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct EnumId(pub u32);

/// Byte order. Recorded because bitfield *bit* offsets are only
/// meaningful alongside it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Endian {
    Little,
    Big,
}

impl Endian {
    pub fn tag(self) -> &'static str {
        match self {
            Endian::Little => "little",
            Endian::Big => "big",
        }
    }
}

/// The target's answers to the questions C leaves open. Every width is
/// in bits and already decided — the compiler never consults a table
/// keyed on a triple, it reads this.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TargetInfo {
    pub triple: String,
    pub pointer_bits: u16,
    pub pointer_align_bits: u16,
    /// `char`'s signedness: the arm/ppc split that silently changes
    /// program meaning.
    pub char_signed: bool,
    pub short_bits: u16,
    pub int_bits: u16,
    pub long_bits: u16,
    pub long_long_bits: u16,
    pub wchar_bits: u16,
    pub wchar_signed: bool,
    /// `size_t` / `ptrdiff_t` width (equal to `pointer_bits` on every
    /// target we support, recorded anyway — the day it is not, we want
    /// the artifact to say so rather than the compiler to assume).
    pub size_t_bits: u16,
    pub endian: Endian,
}

impl TargetInfo {
    /// linux/x86-64 — the s46 acceptance target and the default of
    /// `wolf c-import` when `--target` is not given.
    pub fn x86_64_linux() -> TargetInfo {
        TargetInfo {
            triple: "x86_64-unknown-linux-gnu".to_string(),
            pointer_bits: 64,
            pointer_align_bits: 64,
            char_signed: true,
            short_bits: 16,
            int_bits: 32,
            long_bits: 64,
            long_long_bits: 64,
            wchar_bits: 32,
            wchar_signed: true,
            size_t_bits: 64,
            endian: Endian::Little,
        }
    }

    /// The other tier-1 triples the importer knows how to *parameterize*
    /// (s47 owns the header bundles that make them useful).
    pub fn for_triple(triple: &str) -> Option<TargetInfo> {
        let mut t = TargetInfo::x86_64_linux();
        t.triple = triple.to_string();
        match triple {
            "x86_64-unknown-linux-gnu" | "x86_64-unknown-freebsd" => {}
            "aarch64-unknown-linux-gnu" => {
                // The signedness split that bites: `char` is unsigned on
                // aarch64 Linux, and a program that assumed otherwise
                // compiles either way and behaves differently.
                t.char_signed = false;
                t.wchar_signed = false;
            }
            "aarch64-apple-darwin" => {
                t.char_signed = false;
                t.wchar_bits = 32;
            }
            "x86_64-pc-windows-msvc" | "x86_64-pc-windows-gnu" => {
                // LLP64: `long` stays 32 bits. Getting this wrong is the
                // classic cross-compilation miscompile.
                t.long_bits = 32;
                t.wchar_bits = 16;
                t.wchar_signed = false;
            }
            _ => return None,
        }
        Some(t)
    }
}

/// C's type qualifiers, as they matter at the seam.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Qual {
    pub is_const: bool,
    /// Honored: a volatile access lowers to a volatile op, never gets
    /// folded away.
    pub is_volatile: bool,
    /// Consumed as an aliasing assertion at the boundary, never
    /// silently believed inside wolf.
    pub is_restrict: bool,
}

impl Qual {
    pub fn is_none(self) -> bool {
        !self.is_const && !self.is_volatile && !self.is_restrict
    }

    /// The dump spelling (`""` when unqualified).
    pub fn spelling(self) -> String {
        let mut s = String::new();
        for (on, word) in [
            (self.is_const, "const"),
            (self.is_volatile, "volatile"),
            (self.is_restrict, "restrict"),
        ] {
            if on {
                if !s.is_empty() {
                    s.push(' ');
                }
                s.push_str(word);
            }
        }
        s
    }
}

/// Which C spelling an integer came from. The *width* is already
/// resolved; this is kept only so diagnostics can say `unsigned long`
/// instead of `an unsigned 64-bit integer`, and so a future ABI rule
/// keyed on the spelling has it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IntSpelling {
    Char,
    Short,
    Int,
    Long,
    LongLong,
    SizeT,
    WcharT,
    /// A spelling the worker did not classify (a typedef chain it
    /// flattened, an extension type).
    Other,
}

impl IntSpelling {
    pub fn tag(self) -> &'static str {
        match self {
            IntSpelling::Char => "char",
            IntSpelling::Short => "short",
            IntSpelling::Int => "int",
            IntSpelling::Long => "long",
            IntSpelling::LongLong => "longlong",
            IntSpelling::SizeT => "size_t",
            IntSpelling::WcharT => "wchar_t",
            IntSpelling::Other => "other",
        }
    }

    pub fn from_tag(t: &str) -> Option<IntSpelling> {
        Some(match t {
            "char" => IntSpelling::Char,
            "short" => IntSpelling::Short,
            "int" => IntSpelling::Int,
            "long" => IntSpelling::Long,
            "longlong" => IntSpelling::LongLong,
            "size_t" => IntSpelling::SizeT,
            "wchar_t" => IntSpelling::WcharT,
            "other" => IntSpelling::Other,
            _ => return None,
        })
    }
}

/// One C type, with the target's answers substituted in.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CType {
    Void,
    /// `_Bool`.
    Bool,
    Int {
        bits: u16,
        signed: bool,
        spelling: IntSpelling,
    },
    Float {
        bits: u16,
    },
    Ptr {
        pointee: CTypeId,
        /// Qualifiers **on the pointee** (`const char *` is a pointer
        /// to a const char).
        qual: Qual,
    },
    Array {
        elem: CTypeId,
        /// `None` = `[]` (incomplete or a parameter that decayed).
        len: Option<u64>,
    },
    Func {
        ret: CTypeId,
        params: Vec<CTypeId>,
        variadic: bool,
    },
    Record(RecordId),
    Enum(EnumId),
    /// A type the worker refused. Carries the *tag* of the refusal so
    /// anything built out of it can say what it depends on; the full
    /// refusal lives on the declaration.
    Refused(String),
}

/// struct or union.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecordKind {
    Struct,
    Union,
}

impl RecordKind {
    pub fn tag(self) -> &'static str {
        match self {
            RecordKind::Struct => "struct",
            RecordKind::Union => "union",
        }
    }
}

/// One field, at its resolved offset. **Bit** offsets throughout, so a
/// bitfield and an ordinary field are the same shape.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Field {
    pub name: String,
    pub ty: CTypeId,
    pub offset_bits: u64,
    /// `Some(n)` for a bitfield of width `n`.
    pub bit_width: Option<u32>,
}

/// A struct or union, laid out for the target.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Record {
    /// The tag name (`""` for an anonymous record).
    pub name: String,
    pub kind: RecordKind,
    /// `None` = incomplete (declared, never defined).
    pub size_bytes: Option<u64>,
    pub align_bytes: Option<u32>,
    pub fields: Vec<Field>,
    /// The fields are not to be read: the record imported as opaque.
    /// (A union always lands here — see
    /// [`Refusal::UnionActiveMember`](crate::refuse::Refusal::UnionActiveMember).)
    pub opaque: bool,
}

/// An enum, with its constants' resolved values.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EnumDef {
    pub name: String,
    /// The integer type the target chose for it.
    pub underlying: CTypeId,
    /// `(name, value)`, in declaration order.
    pub constants: Vec<(String, i128)>,
}

/// The append-only type arena. Interning is by structural equality, so
/// two workers that saw the same header emit the same arena — which is
/// what makes the conformance dumps comparable at all.
#[derive(Clone, Default, Debug)]
pub struct CTypeArena {
    types: Vec<CType>,
    intern: BTreeMap<String, CTypeId>,
}

/// Two arenas are equal when they hold the same types in the same
/// order. The intern index is derived state — a decoded arena and a
/// built one can disagree about it while describing identical C, and
/// the artifact round-trip test would fail on a difference that means
/// nothing.
impl PartialEq for CTypeArena {
    fn eq(&self, other: &Self) -> bool {
        self.types == other.types
    }
}

impl Eq for CTypeArena {}

impl CTypeArena {
    pub fn new() -> CTypeArena {
        CTypeArena::default()
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    pub fn get(&self, id: CTypeId) -> &CType {
        &self.types[id.0 as usize]
    }

    pub fn iter(&self) -> impl Iterator<Item = (CTypeId, &CType)> {
        self.types
            .iter()
            .enumerate()
            .map(|(i, t)| (CTypeId(i as u32), t))
    }

    /// Intern a type, returning its stable id.
    pub fn intern(&mut self, t: CType) -> CTypeId {
        let key = intern_key(&t);
        if let Some(&id) = self.intern.get(&key) {
            return id;
        }
        let id = CTypeId(self.types.len() as u32);
        self.types.push(t);
        self.intern.insert(key, id);
        id
    }

    /// Push without interning — used by the decoder, which is
    /// reconstructing an arena whose ids are already fixed.
    pub(crate) fn push_raw(&mut self, t: CType) -> CTypeId {
        let key = intern_key(&t);
        let id = CTypeId(self.types.len() as u32);
        self.types.push(t);
        self.intern.entry(key).or_insert(id);
        id
    }

    // ------------------------------------------------- conveniences --

    pub fn void(&mut self) -> CTypeId {
        self.intern(CType::Void)
    }

    /// `char` for this target (signedness included — the whole point).
    pub fn char_(&mut self, t: &TargetInfo) -> CTypeId {
        self.intern(CType::Int {
            bits: 8,
            signed: t.char_signed,
            spelling: IntSpelling::Char,
        })
    }

    pub fn int_(&mut self, t: &TargetInfo) -> CTypeId {
        self.intern(CType::Int {
            bits: t.int_bits,
            signed: true,
            spelling: IntSpelling::Int,
        })
    }

    pub fn size_t(&mut self, t: &TargetInfo) -> CTypeId {
        self.intern(CType::Int {
            bits: t.size_t_bits,
            signed: false,
            spelling: IntSpelling::SizeT,
        })
    }

    pub fn ptr_to(&mut self, pointee: CTypeId, qual: Qual) -> CTypeId {
        self.intern(CType::Ptr { pointee, qual })
    }

    /// `void *`.
    pub fn void_ptr(&mut self) -> CTypeId {
        let v = self.void();
        self.ptr_to(v, Qual::default())
    }

    /// The dump/diagnostic spelling of a type — C's own syntax, so a
    /// reader can find it in the header.
    pub fn spell(&self, id: CTypeId) -> String {
        self.spell_depth(id, 0)
    }

    fn spell_depth(&self, id: CTypeId, depth: u32) -> String {
        // Recursive C types (`struct s { struct s *next; }`) are normal;
        // the record arm stops the recursion by name, but a malformed
        // artifact could still cycle through a typedef, so cap it.
        if depth > 16 {
            return "…".to_string();
        }
        match self.get(id) {
            CType::Void => "void".to_string(),
            CType::Bool => "_Bool".to_string(),
            CType::Int {
                bits,
                signed,
                spelling,
            } => {
                let base = match spelling {
                    IntSpelling::Char => "char",
                    IntSpelling::Short => "short",
                    IntSpelling::Int => "int",
                    IntSpelling::Long => "long",
                    IntSpelling::LongLong => "long long",
                    IntSpelling::SizeT => return "size_t".to_string(),
                    IntSpelling::WcharT => return "wchar_t".to_string(),
                    IntSpelling::Other => {
                        return format!("{}int{bits}_t", if *signed { "" } else { "u" });
                    }
                };
                if *signed {
                    base.to_string()
                } else {
                    format!("unsigned {base}")
                }
            }
            CType::Float { bits } => match bits {
                32 => "float".to_string(),
                64 => "double".to_string(),
                other => format!("_Float{other}"),
            },
            CType::Ptr { pointee, qual } => {
                let q = qual.spelling();
                let inner = self.spell_depth(*pointee, depth + 1);
                if q.is_empty() {
                    format!("{inner} *")
                } else {
                    format!("{q} {inner} *")
                }
            }
            CType::Array { elem, len } => {
                let inner = self.spell_depth(*elem, depth + 1);
                match len {
                    Some(n) => format!("{inner}[{n}]"),
                    None => format!("{inner}[]"),
                }
            }
            CType::Func {
                ret,
                params,
                variadic,
            } => {
                let r = self.spell_depth(*ret, depth + 1);
                let mut ps: Vec<String> = params
                    .iter()
                    .map(|p| self.spell_depth(*p, depth + 1))
                    .collect();
                if *variadic {
                    ps.push("...".to_string());
                } else if ps.is_empty() {
                    ps.push("void".to_string());
                }
                format!("{r}({})", ps.join(", "))
            }
            // Records and enums spell by handle: the artifact's record
            // table is where the body lives, and inlining it here would
            // make a recursive type unprintable.
            CType::Record(r) => format!("record#{}", r.0),
            CType::Enum(e) => format!("enum#{}", e.0),
            CType::Refused(tag) => format!("<refused: {tag}>"),
        }
    }
}

/// The structural key a type interns under. Deterministic and total —
/// two arenas built from the same header in the same order agree.
fn intern_key(t: &CType) -> String {
    match t {
        CType::Void => "v".to_string(),
        CType::Bool => "b".to_string(),
        CType::Int {
            bits,
            signed,
            spelling,
        } => format!("i{bits}:{}:{}", *signed as u8, spelling.tag()),
        CType::Float { bits } => format!("f{bits}"),
        CType::Ptr { pointee, qual } => format!(
            "p{}:{}{}{}",
            pointee.0, qual.is_const as u8, qual.is_volatile as u8, qual.is_restrict as u8
        ),
        CType::Array { elem, len } => format!(
            "a{}:{}",
            elem.0,
            len.map(|n| n.to_string()).unwrap_or_else(|| "?".into())
        ),
        CType::Func {
            ret,
            params,
            variadic,
        } => {
            let ps: Vec<String> = params.iter().map(|p| p.0.to_string()).collect();
            format!("F{}({}){}", ret.0, ps.join(","), *variadic as u8)
        }
        CType::Record(r) => format!("R{}", r.0),
        CType::Enum(e) => format!("E{}", e.0),
        CType::Refused(tag) => format!("X{tag}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole reason widths are in the artifact: `long` is not one
    /// type. A windows import and a linux import of the same header
    /// must not produce the same encoding, or cross-compilation is a
    /// silent miscompile.
    #[test]
    fn long_is_not_one_type_across_targets() {
        let lin = TargetInfo::for_triple("x86_64-unknown-linux-gnu").expect("tier-1");
        let win = TargetInfo::for_triple("x86_64-pc-windows-msvc").expect("tier-1");
        assert_eq!(lin.long_bits, 64);
        assert_eq!(win.long_bits, 32, "LLP64: `long` stays 32 bits on windows");
    }

    /// `char`'s signedness is a target fact, and a wrong answer here is
    /// a program that compiles on both and behaves on one.
    #[test]
    fn char_signedness_follows_the_target() {
        let x86 = TargetInfo::for_triple("x86_64-unknown-linux-gnu").expect("tier-1");
        let arm = TargetInfo::for_triple("aarch64-unknown-linux-gnu").expect("tier-1");
        assert!(x86.char_signed);
        assert!(!arm.char_signed);

        let mut a = CTypeArena::new();
        let c_x86 = a.char_(&x86);
        let c_arm = a.char_(&arm);
        assert_ne!(
            c_x86, c_arm,
            "signed and unsigned char must not intern to one id"
        );
    }

    #[test]
    fn interning_is_structural_and_stable() {
        let t = TargetInfo::x86_64_linux();
        let mut a = CTypeArena::new();
        let v1 = a.void_ptr();
        let v2 = a.void_ptr();
        assert_eq!(v1, v2);
        let s1 = a.size_t(&t);
        let s2 = a.size_t(&t);
        assert_eq!(s1, s2);
        assert_ne!(v1, s1);
    }

    #[test]
    fn spelling_is_c_syntax() {
        let t = TargetInfo::x86_64_linux();
        let mut a = CTypeArena::new();
        let ch = a.char_(&t);
        let cp = a.ptr_to(
            ch,
            Qual {
                is_const: true,
                ..Qual::default()
            },
        );
        assert_eq!(a.spell(cp), "const char *");
        let vp = a.void_ptr();
        assert_eq!(a.spell(vp), "void *");
        let sz = a.size_t(&t);
        let f = a.intern(CType::Func {
            ret: vp,
            params: vec![sz],
            variadic: false,
        });
        assert_eq!(a.spell(f), "void *(size_t)");
        let novoid = a.intern(CType::Func {
            ret: sz,
            params: vec![],
            variadic: false,
        });
        assert_eq!(a.spell(novoid), "size_t(void)");
    }

    /// A recursive type is ordinary C; spelling one must terminate.
    #[test]
    fn recursive_types_spell_without_looping() {
        let mut a = CTypeArena::new();
        let r = a.intern(CType::Record(RecordId(0)));
        let p = a.ptr_to(r, Qual::default());
        assert_eq!(a.spell(p), "record#0 *");
    }

    #[test]
    fn unknown_triples_are_refused_not_guessed() {
        assert!(TargetInfo::for_triple("mips-unknown-linux-gnu").is_none());
    }
}
