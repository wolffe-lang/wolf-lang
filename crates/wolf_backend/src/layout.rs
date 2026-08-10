//! Aggregate layout v0 — the language's layout contract, owned by the
//! backend INTERFACE so every backend (Cranelift now, Tier-F at c12)
//! and the s29 ABI lowering compute identical offsets. Never a
//! Cranelift-private detail.
//!
//! Rules (v0, natural/C-like):
//! - scalars: size = natural width, align = size (`bool` is 1 byte,
//!   `ptr` is 8 — the M1 target is 64-bit).
//! - `{a, b, …}` aggregates: fields in declaration order, each at the
//!   next multiple of its alignment; aggregate alignment is the max
//!   field alignment; size rounds up to the alignment.
//! - `eu{OK, S…}` error unions: laid out exactly as the aggregate
//!   `{i64 tag, OK?, S…}` (tag first; `[abi.err.repr]`'s register-pair
//!   contract is s29's — memory layout is this).
//! - effect tokens (`mem.rN`, `io`): NO layout — they erase at codegen
//!   and asking is a caller bug ([`layout_of`] returns `None`).
//!
//! s27's flat `mut`-spill layout (packed 8-byte scalar slots) agrees
//! with these rules for the all-`i64` aggregates the corpus spills
//! today; c06's spilled-aggregate completion replaces that special
//! case with calls into this module.

use wolf_wir::types::{TypeData, TypeId, TypeInterner};

/// Size and alignment of one type, in bytes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Layout {
    pub size: u32,
    pub align: u32,
}

/// Layout of a type; `None` for effect tokens (they have no bytes).
pub fn layout_of(types: &TypeInterner, ty: TypeId) -> Option<Layout> {
    let l = match types.get(ty) {
        TypeData::I8 | TypeData::Bool => Layout { size: 1, align: 1 },
        TypeData::I16 => Layout { size: 2, align: 2 },
        TypeData::I32 | TypeData::F32 => Layout { size: 4, align: 4 },
        TypeData::I64 | TypeData::F64 | TypeData::Ptr => Layout { size: 8, align: 8 },
        TypeData::Mem(_) | TypeData::Io => return None,
        TypeData::Agg(fields) => fields_layout(types, fields)?.0,
        TypeData::Eu { .. } => fields_layout(types, &eu_fields(types, ty))?.0,
    };
    Some(l)
}

/// The equivalent field list of an error union: `[i64 tag, OK?, S…]`.
pub fn eu_fields(types: &TypeInterner, ty: TypeId) -> Vec<TypeId> {
    let TypeData::Eu { ok, slots } = types.get(ty) else {
        panic!("eu_fields on a non-eu type");
    };
    let mut fields = vec![wolf_wir::types::I64];
    if let Some(okt) = ok {
        fields.push(*okt);
    }
    fields.extend(slots.iter().copied());
    fields
}

/// Byte offset of the ok payload within an `eu` (None when the ok half
/// is unit). The tag is always at offset 0.
pub fn eu_ok_offset(types: &TypeInterner, ty: TypeId) -> Option<u32> {
    let TypeData::Eu { ok, .. } = types.get(ty) else {
        panic!("eu_ok_offset on a non-eu type");
    };
    ok.as_ref()?;
    field_offsets(types, &eu_fields(types, ty)).map(|offs| offs[1])
}

/// Byte offsets of the error-payload slots within an `eu`, in slot
/// order.
pub fn eu_slot_offset(types: &TypeInterner, ty: TypeId) -> Vec<u32> {
    let TypeData::Eu { ok, .. } = types.get(ty) else {
        panic!("eu_slot_offset on a non-eu type");
    };
    let skip = 1 + usize::from(ok.is_some());
    field_offsets(types, &eu_fields(types, ty))
        .map(|offs| offs[skip..].to_vec())
        .unwrap_or_default()
}

/// Offsets of each field of an aggregate field list, or `None` if any
/// field is a token (unrepresentable in memory).
pub fn field_offsets(types: &TypeInterner, fields: &[TypeId]) -> Option<Vec<u32>> {
    fields_layout(types, fields).map(|(_, offs)| offs)
}

fn fields_layout(types: &TypeInterner, fields: &[TypeId]) -> Option<(Layout, Vec<u32>)> {
    let mut offs = Vec::with_capacity(fields.len());
    let mut size = 0u32;
    let mut align = 1u32;
    for &f in fields {
        let l = layout_of(types, f)?;
        size = size.next_multiple_of(l.align);
        offs.push(size);
        size += l.size;
        align = align.max(l.align);
    }
    Some((
        Layout {
            size: size.next_multiple_of(align),
            align,
        },
        offs,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wolf_wir::types::{self, TypeInterner};

    #[test]
    fn scalar_and_agg_layout() {
        let mut it = TypeInterner::new();
        assert_eq!(
            layout_of(&it, types::I64),
            Some(Layout { size: 8, align: 8 })
        );
        assert_eq!(
            layout_of(&it, types::BOOL),
            Some(Layout { size: 1, align: 1 })
        );
        // {i8, i64, i32} — padding to natural alignment, size rounds up.
        let a = it.intern(TypeData::Agg(vec![types::I8, types::I64, types::I32]));
        assert_eq!(layout_of(&it, a), Some(Layout { size: 24, align: 8 }));
        let TypeData::Agg(fields) = it.get(a).clone() else {
            unreachable!()
        };
        assert_eq!(field_offsets(&it, &fields), Some(vec![0, 8, 16]));
        // The corpus's all-i64 triple: packed 0/8/16, 24 bytes — agrees
        // with s27's flat spill layout.
        let t = it.intern(TypeData::Agg(vec![types::I64, types::I64, types::I64]));
        assert_eq!(layout_of(&it, t), Some(Layout { size: 24, align: 8 }));
    }

    #[test]
    fn eu_layout_tag_first() {
        let mut it = TypeInterner::new();
        let e = it.eu(Some(types::I64), vec![types::I64]);
        assert_eq!(layout_of(&it, e), Some(Layout { size: 24, align: 8 }));
        assert_eq!(eu_ok_offset(&it, e), Some(8));
        assert_eq!(eu_slot_offset(&it, e), vec![16]);
        let unit = it.eu(None, vec![]);
        assert_eq!(layout_of(&it, unit), Some(Layout { size: 8, align: 8 }));
        assert_eq!(eu_ok_offset(&it, unit), None);
    }

    #[test]
    fn tokens_have_no_layout() {
        use wolf_wir::entity::EntityRef;
        let mut it = TypeInterner::new();
        let m = it.mem(wolf_wir::types::RegionId::new(0));
        assert_eq!(layout_of(&it, m), None);
        assert_eq!(layout_of(&it, types::IO), None);
    }
}
