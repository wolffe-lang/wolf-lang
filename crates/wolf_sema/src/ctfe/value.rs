//! The interned comptime value arena (s16, Target 1).
//!
//! One arena holds every value comptime evaluation produces: scalars,
//! aggregates, comptime slices, functions, and **types** — hash-consed
//! like the type table, so structural equality is id equality and every
//! value carries a **content hash** (blake3 over a canonical encoding).
//! The hashes are the determinism surface: same program + target ⇒
//! bit-identical hashes on every host (the cross-arch differential gate
//! consumes exactly these), and they key the memoization table so a
//! cached comptime result invalidates exactly when its inputs change
//! (the D7 incremental spine).
//!
//! Nothing in here references the type checker's arenas: the engine
//! boundary is `evaluate(fn, arg values) → value`, with no HIR types
//! leaking into results — a future WIR-based evaluator can replace the
//! walker without touching this vocabulary.
//!
//! Determinism rules baked in at this level:
//! - no addresses are observable (ids are arena ordinals, and ordinals
//!   never escape into results — only content hashes do);
//! - floats are stored as bits, and every arithmetic result is
//!   NaN-canonicalized before interning (x86-64 and aarch64 disagree
//!   on default NaN payloads; wolf does not);
//! - aggregate fields keep declaration order; nothing iterates a hash
//!   map on the way to a hash.

use std::collections::HashMap;

use crate::types::Prim;

/// Index of an interned value. Equality of ids is equality of values
/// within one arena.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ValId(u32);

impl ValId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A self-contained comptime *type* value — the inhabitants of the
/// `type` kind (D29). Deliberately independent of [`crate::types`]'s
/// interned table: a type value is data, comparable and hashable on
/// its own, convertible to and from checker types only at the engine
/// boundary.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum CtType {
    Unit,
    Prim(Prim),
    Tuple(Vec<CtType>),
    /// A nominal struct/enum by its semantic identity.
    Nominal {
        module: u32,
        name: String,
    },
    /// A `typebuild`-synthesized struct: fields in declaration order.
    Synth {
        fields: Vec<(String, CtType)>,
    },
    /// The `type` type itself.
    TypeTy,
    /// A form outside the comptime mirror, kept by rendering (equal
    /// only to the identical rendering — never guessed about).
    Opaque(String),
}

impl CtType {
    /// Render as source would spell it (diagnostics + hashes).
    pub fn render(&self) -> String {
        match self {
            CtType::Unit => "()".to_string(),
            CtType::Prim(p) => p.name().to_string(),
            CtType::Tuple(ts) => {
                let parts: Vec<String> = ts.iter().map(CtType::render).collect();
                format!("({})", parts.join(", "))
            }
            CtType::Nominal { name, .. } => name.clone(),
            CtType::Synth { fields } => {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|(n, t)| format!("{n}: {}", t.render()))
                    .collect();
                format!("struct {{ {} }}", parts.join(", "))
            }
            CtType::TypeTy => "type".to_string(),
            CtType::Opaque(s) => s.clone(),
        }
    }
}

/// One interned value's structure. Children are [`ValId`]s into the
/// same arena.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ValueKind {
    Unit,
    Bool(bool),
    /// An integer at its declared width (target-parameterized
    /// semantics: `int`/`uint` are 64-bit on every tier-1 target).
    /// The value is stored wide; the width bounds every operation.
    Int {
        v: i128,
        w: Prim,
    },
    /// A float stored as bits at its width (`f32` occupies the low 32
    /// bits). Bits, not `f64`: NaN payloads must hash identically
    /// across hosts.
    Float {
        bits: u64,
        w: Prim,
    },
    Str(String),
    Tuple(Vec<ValId>),
    /// A comptime slice (reflection lists, `typebuild` inputs).
    Slice(Vec<ValId>),
    /// A struct value: fields in declaration order.
    Struct {
        module: u32,
        name: String,
        fields: Vec<(String, ValId)>,
    },
    /// A named function as a value.
    Fn {
        module: u32,
        name: String,
    },
    /// A type as a value (D29).
    Type(CtType),
}

/// The hash-consing arena with per-value content hashes and a logical
/// heap gauge (cells) that the D33 heap cap reads.
#[derive(Debug, Default)]
pub struct ValueArena {
    kinds: Vec<ValueKind>,
    map: HashMap<ValueKind, ValId>,
    hashes: Vec<[u8; 32]>,
    /// Cells charged so far. Interning a *new* value charges its
    /// shallow cost; re-interning an existing structure is free
    /// (sharing is not allocation).
    pub cells_used: u64,
}

impl ValueArena {
    pub fn new() -> ValueArena {
        ValueArena::default()
    }

    /// The shallow logical cost of a value, in cells.
    fn cost(kind: &ValueKind) -> u64 {
        match kind {
            ValueKind::Unit | ValueKind::Bool(_) => 1,
            ValueKind::Int { .. } | ValueKind::Float { .. } | ValueKind::Fn { .. } => 1,
            ValueKind::Str(s) => 1 + (s.len() as u64).div_ceil(8),
            ValueKind::Tuple(xs) | ValueKind::Slice(xs) => 1 + xs.len() as u64,
            ValueKind::Struct { fields, .. } => 1 + fields.len() as u64,
            ValueKind::Type(_) => 2,
        }
    }

    /// The canonical content encoding: a tag byte plus the children's
    /// content hashes (never their ids — ids are creation-order
    /// accidents; hashes are portable).
    fn encode(&self, kind: &ValueKind) -> Vec<u8> {
        let mut out = Vec::new();
        match kind {
            ValueKind::Unit => out.push(0),
            ValueKind::Bool(b) => {
                out.push(1);
                out.push(u8::from(*b));
            }
            ValueKind::Int { v, w } => {
                out.push(2);
                out.extend_from_slice(w.name().as_bytes());
                out.push(0);
                out.extend_from_slice(&v.to_le_bytes());
            }
            ValueKind::Float { bits, w } => {
                out.push(3);
                out.extend_from_slice(w.name().as_bytes());
                out.push(0);
                out.extend_from_slice(&bits.to_le_bytes());
            }
            ValueKind::Str(s) => {
                out.push(4);
                out.extend_from_slice(s.as_bytes());
            }
            ValueKind::Tuple(xs) => {
                out.push(5);
                for x in xs {
                    out.extend_from_slice(&self.hashes[x.index()]);
                }
            }
            ValueKind::Slice(xs) => {
                out.push(6);
                for x in xs {
                    out.extend_from_slice(&self.hashes[x.index()]);
                }
            }
            ValueKind::Struct {
                module,
                name,
                fields,
            } => {
                out.push(7);
                out.extend_from_slice(&module.to_le_bytes());
                out.extend_from_slice(name.as_bytes());
                out.push(0);
                for (fname, fv) in fields {
                    out.extend_from_slice(fname.as_bytes());
                    out.push(0);
                    out.extend_from_slice(&self.hashes[fv.index()]);
                }
            }
            ValueKind::Fn { module, name } => {
                out.push(8);
                out.extend_from_slice(&module.to_le_bytes());
                out.extend_from_slice(name.as_bytes());
            }
            ValueKind::Type(t) => {
                out.push(9);
                out.extend_from_slice(t.render().as_bytes());
            }
        }
        out
    }

    /// Intern `kind`; charges cells only when the structure is new.
    pub fn intern(&mut self, kind: ValueKind) -> ValId {
        if let Some(&id) = self.map.get(&kind) {
            return id;
        }
        let hash = *blake3::hash(&self.encode(&kind)).as_bytes();
        self.cells_used += Self::cost(&kind);
        let id = ValId(u32::try_from(self.kinds.len()).expect("value arena overflow"));
        self.kinds.push(kind.clone());
        self.hashes.push(hash);
        self.map.insert(kind, id);
        id
    }

    pub fn kind(&self, id: ValId) -> &ValueKind {
        &self.kinds[id.index()]
    }

    /// The value's content hash — the determinism and memoization
    /// surface.
    pub fn content_hash(&self, id: ValId) -> [u8; 32] {
        self.hashes[id.index()]
    }

    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    // Convenience constructors.
    pub fn unit(&mut self) -> ValId {
        self.intern(ValueKind::Unit)
    }

    pub fn bool_(&mut self, b: bool) -> ValId {
        self.intern(ValueKind::Bool(b))
    }

    pub fn int(&mut self, v: i128, w: Prim) -> ValId {
        self.intern(ValueKind::Int { v, w })
    }

    pub fn float(&mut self, bits: u64, w: Prim) -> ValId {
        self.intern(ValueKind::Float { bits, w })
    }

    pub fn str_(&mut self, s: impl Into<String>) -> ValId {
        self.intern(ValueKind::Str(s.into()))
    }

    pub fn ty(&mut self, t: CtType) -> ValId {
        self.intern(ValueKind::Type(t))
    }

    /// Render a value for humans (diagnostics, tests).
    pub fn render(&self, id: ValId) -> String {
        match self.kind(id) {
            ValueKind::Unit => "()".to_string(),
            ValueKind::Bool(b) => b.to_string(),
            ValueKind::Int { v, .. } => v.to_string(),
            ValueKind::Float { bits, w } => {
                if *w == Prim::F32 {
                    format!("{}", f32::from_bits(*bits as u32))
                } else {
                    format!("{}", f64::from_bits(*bits))
                }
            }
            ValueKind::Str(s) => format!("{s:?}"),
            ValueKind::Tuple(xs) => {
                let parts: Vec<String> = xs.iter().map(|&x| self.render(x)).collect();
                format!("({})", parts.join(", "))
            }
            ValueKind::Slice(xs) => {
                let parts: Vec<String> = xs.iter().map(|&x| self.render(x)).collect();
                format!("[{}]", parts.join(", "))
            }
            ValueKind::Struct { name, fields, .. } => {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|(n, v)| format!("{n}: {}", self.render(*v)))
                    .collect();
                format!("{name} {{ {} }}", parts.join(", "))
            }
            ValueKind::Fn { name, .. } => format!("fn {name}"),
            ValueKind::Type(t) => t.render(),
        }
    }
}

/// The integer range of a width: `(min, max)` inclusive.
pub fn int_range(w: Prim) -> (i128, i128) {
    match w {
        Prim::I8 => (i8::MIN as i128, i8::MAX as i128),
        Prim::I16 => (i16::MIN as i128, i16::MAX as i128),
        Prim::I32 => (i32::MIN as i128, i32::MAX as i128),
        // `int` is 64-bit on every tier-1 target (target-parameterized
        // semantics, evaluated at the declared width).
        Prim::I64 | Prim::Int => (i64::MIN as i128, i64::MAX as i128),
        Prim::U8 | Prim::Byte => (0, u8::MAX as i128),
        Prim::U16 => (0, u16::MAX as i128),
        Prim::U32 => (0, u32::MAX as i128),
        Prim::U64 | Prim::Uint => (0, u64::MAX as i128),
        _ => (i64::MIN as i128, i64::MAX as i128),
    }
}

/// Canonical quiet NaN per width — every float operation result passes
/// through here so NaN payloads cannot leak host defaults (x86-64 and
/// aarch64 differ; wolf's comptime does not).
pub fn canon_f64(x: f64) -> u64 {
    if x.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        x.to_bits()
    }
}

pub fn canon_f32(x: f32) -> u32 {
    if x.is_nan() { 0x7fc0_0000 } else { x.to_bits() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning_is_structural_and_hashes_are_content() {
        let mut a = ValueArena::new();
        let x = a.int(42, Prim::Int);
        let y = a.int(42, Prim::Int);
        assert_eq!(x, y);
        let z = a.int(42, Prim::I32);
        assert_ne!(x, z, "width is part of the value");
        // Content hashes are creation-order independent: build the
        // same structures in a different order in a fresh arena.
        let t1 = {
            let s = a.str_("hi");
            a.intern(ValueKind::Tuple(vec![x, s]))
        };
        let mut b = ValueArena::new();
        let t2 = {
            let s = b.str_("hi");
            let x2 = b.int(42, Prim::Int);
            b.intern(ValueKind::Tuple(vec![x2, s]))
        };
        assert_eq!(a.content_hash(t1), b.content_hash(t2));
    }

    #[test]
    fn nan_canonicalization_is_width_aware() {
        let bits = canon_f64(f64::NAN);
        assert_eq!(bits, 0x7ff8_0000_0000_0000);
        assert_eq!(canon_f64(1.5), 1.5f64.to_bits());
        assert_eq!(canon_f32(f32::NAN), 0x7fc0_0000);
    }

    #[test]
    fn sharing_is_not_allocation() {
        let mut a = ValueArena::new();
        let before = a.cells_used;
        let x = a.int(7, Prim::Int);
        let after_first = a.cells_used;
        let y = a.int(7, Prim::Int);
        assert_eq!(x, y);
        assert_eq!(a.cells_used, after_first, "re-interning charges nothing");
        assert!(after_first > before);
    }
}
