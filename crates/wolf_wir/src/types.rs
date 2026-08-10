//! WIR types v0 (s24): machine scalars, `ptr` (provenance rides in
//! facts, s26), the `mem`/`io` effect-token types, and small aggregates
//! by value list. In-crate interner; sema types map onto these in s25.

use crate::entity::{PrimaryMap, entity_id};
use std::collections::HashMap;

entity_id!(
    /// An interned WIR type.
    pub struct TypeId, "ty"
);

entity_id!(
    /// A function-scoped region identity. `mem.r0` is the effect token
    /// for region r0; real region semantics land in s26 — s24 carries
    /// the identity so tokens and region facts name the same thing.
    pub struct RegionId, "r"
);

/// The structure of a WIR type.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TypeData {
    /// 8-bit signed integer.
    I8,
    /// 16-bit signed integer.
    I16,
    /// 32-bit signed integer.
    I32,
    /// 64-bit signed integer.
    I64,
    /// 32-bit IEEE-754 float.
    F32,
    /// 64-bit IEEE-754 float.
    F64,
    /// Boolean (compare results, branch conditions).
    Bool,
    /// An untyped pointer. Aliasing, dereferenceability and region
    /// provenance are FACTS about ptr values, never part of the type.
    Ptr,
    /// The memory effect token for one region: loads read it, stores
    /// and calls consume it and produce a successor. The token chain
    /// is a spine, not a DAG (verified: linear consumption).
    Mem(RegionId),
    /// The external-effect token (I/O, FFI). Same linearity as `Mem`.
    Io,
    /// A small aggregate passed by value list (field types in order).
    Agg(Vec<TypeId>),
    /// An error union `!T` (s27, D30): a two-value pair — an `i64` tag
    /// (0 = ok; nonzero = a module-interned error tag) plus the ok
    /// payload, with optional error-payload slots. Never a heap box;
    /// c06's ABI expands it to the register-pair contract
    /// (`[abi.err.repr]`). Built and split ONLY by the `eu.*` ops.
    Eu {
        /// The ok payload's type; `None` when the ok half is unit.
        ok: Option<TypeId>,
        /// Error-payload slots, position-unified across the row's
        /// tags (payload-free rows have none).
        slots: Vec<TypeId>,
    },
}

/// Deduplicating type interner. Scalars are pre-interned so their
/// `TypeId`s are compile-time constants.
#[derive(Clone, Debug)]
pub struct TypeInterner {
    types: PrimaryMap<TypeId, TypeData>,
    dedup: HashMap<TypeData, TypeId>,
}

/// Pre-interned scalar `TypeId`s (order fixed by `TypeInterner::new`).
pub const I8: TypeId = TypeId::from_const(0);
pub const I16: TypeId = TypeId::from_const(1);
pub const I32: TypeId = TypeId::from_const(2);
pub const I64: TypeId = TypeId::from_const(3);
pub const F32: TypeId = TypeId::from_const(4);
pub const F64: TypeId = TypeId::from_const(5);
pub const BOOL: TypeId = TypeId::from_const(6);
pub const PTR: TypeId = TypeId::from_const(7);
pub const IO: TypeId = TypeId::from_const(8);

impl TypeId {
    const fn from_const(i: u32) -> TypeId {
        TypeId(i)
    }
}

impl Default for TypeInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeInterner {
    pub fn new() -> Self {
        let mut it = TypeInterner {
            types: PrimaryMap::new(),
            dedup: HashMap::new(),
        };
        for t in [
            TypeData::I8,
            TypeData::I16,
            TypeData::I32,
            TypeData::I64,
            TypeData::F32,
            TypeData::F64,
            TypeData::Bool,
            TypeData::Ptr,
            TypeData::Io,
        ] {
            it.intern(t);
        }
        debug_assert_eq!(it.dedup[&TypeData::Io], IO);
        it
    }

    pub fn intern(&mut self, data: TypeData) -> TypeId {
        if let Some(&id) = self.dedup.get(&data) {
            return id;
        }
        let id = self.types.push(data.clone());
        self.dedup.insert(data, id);
        id
    }

    /// Intern the `mem.rN` token type for a region.
    pub fn mem(&mut self, region: RegionId) -> TypeId {
        self.intern(TypeData::Mem(region))
    }

    /// Intern an error-union type (s27).
    pub fn eu(&mut self, ok: Option<TypeId>, slots: Vec<TypeId>) -> TypeId {
        self.intern(TypeData::Eu { ok, slots })
    }

    pub fn get(&self, id: TypeId) -> &TypeData {
        &self.types[id]
    }

    /// True for `i8/i16/i32/i64`.
    pub fn is_int(&self, id: TypeId) -> bool {
        matches!(
            self.get(id),
            TypeData::I8 | TypeData::I16 | TypeData::I32 | TypeData::I64
        )
    }

    /// True for `f32/f64`.
    pub fn is_float(&self, id: TypeId) -> bool {
        matches!(self.get(id), TypeData::F32 | TypeData::F64)
    }

    /// True for `mem.rN` and `io` — the linearly-consumed effect tokens.
    pub fn is_token(&self, id: TypeId) -> bool {
        matches!(self.get(id), TypeData::Mem(_) | TypeData::Io)
    }

    /// Bit width of an integer type.
    pub fn int_bits(&self, id: TypeId) -> Option<u32> {
        match self.get(id) {
            TypeData::I8 => Some(8),
            TypeData::I16 => Some(16),
            TypeData::I32 => Some(32),
            TypeData::I64 => Some(64),
            _ => None,
        }
    }

    /// Inclusive value bounds of an integer type (signed).
    pub fn int_bounds(&self, id: TypeId) -> Option<(i128, i128)> {
        let bits = self.int_bits(id)?;
        let half = 1i128 << (bits - 1);
        Some((-half, half - 1))
    }

    /// Canonical textual name (`i64`, `mem.r0`, `{f64, i64}`, …).
    pub fn display(&self, id: TypeId) -> String {
        match self.get(id) {
            TypeData::I8 => "i8".into(),
            TypeData::I16 => "i16".into(),
            TypeData::I32 => "i32".into(),
            TypeData::I64 => "i64".into(),
            TypeData::F32 => "f32".into(),
            TypeData::F64 => "f64".into(),
            TypeData::Bool => "bool".into(),
            TypeData::Ptr => "ptr".into(),
            TypeData::Mem(r) => format!("mem.{r}"),
            TypeData::Io => "io".into(),
            TypeData::Agg(fields) => {
                let inner: Vec<String> = fields.iter().map(|&f| self.display(f)).collect();
                format!("{{{}}}", inner.join(", "))
            }
            TypeData::Eu { ok, slots } => {
                // `eu{OK}` / `eu{unit}` / `eu{OK, S0, S1}` — the first
                // position is the ok type (`unit` when absent), the
                // rest are error-payload slots.
                let mut parts = vec![match ok {
                    Some(t) => self.display(*t),
                    None => "unit".into(),
                }];
                for &s in slots {
                    parts.push(self.display(s));
                }
                format!("eu{{{}}}", parts.join(", "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityRef;

    #[test]
    fn scalars_are_pre_interned() {
        let mut it = TypeInterner::new();
        assert_eq!(it.intern(TypeData::I64), I64);
        assert_eq!(it.intern(TypeData::Ptr), PTR);
        assert_eq!(it.display(I64), "i64");
        assert_eq!(it.int_bounds(I8), Some((-128, 127)));
        assert!(it.is_token(IO));
    }

    #[test]
    fn mem_and_agg_dedup() {
        let mut it = TypeInterner::new();
        let m0 = it.mem(RegionId::new(0));
        assert_eq!(it.mem(RegionId::new(0)), m0);
        assert_ne!(it.mem(RegionId::new(1)), m0);
        assert_eq!(it.display(m0), "mem.r0");
        let a = it.intern(TypeData::Agg(vec![F64, I64]));
        assert_eq!(it.intern(TypeData::Agg(vec![F64, I64])), a);
        assert_eq!(it.display(a), "{f64, i64}");
        assert!(it.is_token(m0));
        assert!(!it.is_token(a));
    }
}
