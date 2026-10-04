//! The clause's layout (kw08, K4 — STATUS #31): `[abi.layout.c]`,
//! `[abi.layout.packed]` and `[abi.layout.align]`, computed once from
//! the struct signatures and never from codegen. Two readers share it:
//! the comptime queries (`size_of`, `align_of`, `offset_of`,
//! `[abi.layout.query]`) and the raw tier's pointee layout in WIR
//! lowering, so what a program asks at compile time is what a raw
//! pointer reads and writes at run time, and what C reads.
//!
//! The rules, for the 64-bit targets wolf has (`[abi.c.targets]`; SysV
//! x86-64, AAPCS64 and win64 agree on every one):
//! - a scalar is its natural size, aligned to its size (`bool`, `u8`,
//!   `byte` 1; `u16` 2; `u32`, `f32`, `char` 4; `u64`, `int`, `uint`,
//!   `f64` 8); a raw pointer `*T` is 8, aligned 8;
//! - a `#[repr(c)]` struct places its fields in declaration order, each
//!   at the next offset its alignment allows; its alignment is its
//!   strictest field's and its size is rounded up to that alignment;
//! - `#[repr(c, packed)]`: every field at the next byte, alignment 1,
//!   no padding anywhere (gcc and clang's `__attribute__((packed))`);
//! - `#[repr(c, align(N))]`: the C layout, then the alignment raised to
//!   at least `N` and the size rounded up to it (`aligned(N)`).
//!
//! Every other type — a struct without `#[repr(c)]`, an enum, a tuple,
//! `str`, a container, a generic — has the native layout, which
//! `[abi.native.layout]` leaves to codegen: it has no answer here
//! ([`NoCLayout`], which the queries report as E0708).

use crate::sig::{ItemSig, SigTables, StructSig};
use crate::types::{Prim, TyId, TyKind, TypeTable};

/// A type's layout under the clause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CLayout {
    pub size: u64,
    pub align: u64,
    /// A struct's fields in declaration order (empty for a scalar).
    pub fields: Vec<FieldLayout>,
}

/// One field of a [`CLayout`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldLayout {
    pub name: String,
    pub offset: u64,
    pub layout: CLayout,
}

/// Why a type has no clause layout: `what` names it as source spells
/// it (the type itself, or the field whose type is native-layout).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoCLayout {
    pub what: String,
}

impl CLayout {
    fn scalar(n: u64) -> CLayout {
        CLayout {
            size: n,
            align: n,
            fields: Vec::new(),
        }
    }

    /// The field named `name`, if this is a struct that has one.
    pub fn field(&self, name: &str) -> Option<&FieldLayout> {
        self.fields.iter().find(|f| f.name == name)
    }
}

/// A primitive's size (= its alignment); `None` for `str`, a pair
/// with no single C spelling.
pub fn prim_size(p: Prim) -> Option<u64> {
    Some(match p {
        Prim::Bool | Prim::Byte | Prim::I8 | Prim::U8 => 1,
        Prim::I16 | Prim::U16 => 2,
        // `char` is 4 bytes (D58: a 32-bit scalar, i32-shaped in WIR).
        Prim::I32 | Prim::U32 | Prim::F32 | Prim::Char => 4,
        Prim::I64 | Prim::U64 | Prim::Int | Prim::Uint | Prim::F64 => 8,
        Prim::Str => return None,
    })
}

/// The clause's layout of `ty` (in `table`), or why it has none.
pub fn c_layout(sigs: &SigTables, table: &TypeTable, ty: TyId) -> Result<CLayout, NoCLayout> {
    layout_in(sigs, table, ty, 0)
}

/// The clause's layout of the struct item `name` in `module`.
pub fn struct_c_layout(sigs: &SigTables, module: usize, name: &str) -> Result<CLayout, NoCLayout> {
    match sigs.get(module, name) {
        Some(ItemSig::Struct(ss)) => struct_layout(sigs, name, ss, 0),
        Some(ItemSig::Distinct { base, .. }) => layout_in(sigs, &sigs.table, *base, 1),
        _ => Err(NoCLayout {
            what: format!("`{name}`"),
        }),
    }
}

fn layout_in(
    sigs: &SigTables,
    table: &TypeTable,
    ty: TyId,
    depth: u32,
) -> Result<CLayout, NoCLayout> {
    let none = || NoCLayout {
        what: format!("`{}`", crate::types::render(table, ty, &|_| Err("_"))),
    };
    if depth > 32 {
        return Err(none());
    }
    match table.kind(ty) {
        TyKind::Prim(p) => prim_size(*p).map(CLayout::scalar).ok_or_else(none),
        TyKind::Ptr(_) => Ok(CLayout::scalar(8)),
        TyKind::Wrapping(inner) | TyKind::Distinct(inner) => {
            layout_in(sigs, table, *inner, depth + 1)
        }
        TyKind::Nominal { module, name, args } if args.is_empty() => {
            match sigs.get(*module as usize, name) {
                Some(ItemSig::Struct(ss)) => struct_layout(sigs, name, ss, depth + 1),
                Some(ItemSig::Distinct { base, .. }) => {
                    layout_in(sigs, &sigs.table, *base, depth + 1)
                }
                _ => Err(none()),
            }
        }
        _ => Err(none()),
    }
}

fn struct_layout(
    sigs: &SigTables,
    name: &str,
    ss: &StructSig,
    depth: u32,
) -> Result<CLayout, NoCLayout> {
    if !ss.repr_c || ss.generic {
        return Err(NoCLayout {
            what: format!("`{name}`"),
        });
    }
    let mut fields = Vec::with_capacity(ss.fields.len());
    let mut off = 0u64;
    let mut align = 1u64;
    for f in &ss.fields {
        let l = layout_in(sigs, &sigs.table, f.ty, depth + 1).map_err(|e| NoCLayout {
            what: format!("{} (the field `{name}.{}`)", e.what, f.name),
        })?;
        if !ss.packed {
            off = off.div_ceil(l.align) * l.align;
            align = align.max(l.align);
        }
        fields.push(FieldLayout {
            name: f.name.clone(),
            offset: off,
            layout: l.clone(),
        });
        off += l.size;
    }
    if let Some(n) = ss.align {
        align = align.max(n);
    }
    Ok(CLayout {
        size: off.div_ceil(align) * align,
        align,
        fields,
    })
}

/// The `#[repr(c, align(N))]` struct `ty` is or holds at any depth, by
/// name.
fn aligned_inside(sigs: &SigTables, table: &TypeTable, ty: TyId, depth: u32) -> Option<String> {
    if depth > 32 {
        return None;
    }
    match table.kind(ty) {
        TyKind::Wrapping(inner) | TyKind::Distinct(inner) => {
            aligned_inside(sigs, table, *inner, depth + 1)
        }
        TyKind::Nominal { module, name, args } if args.is_empty() => {
            match sigs.get(*module as usize, name) {
                Some(ItemSig::Struct(ss)) if !ss.generic => {
                    if ss.align.is_some() {
                        return Some(name.clone());
                    }
                    ss.fields
                        .iter()
                        .find_map(|f| aligned_inside(sigs, &sigs.table, f.ty, depth + 1))
                }
                Some(ItemSig::Distinct { base, .. }) => {
                    aligned_inside(sigs, &sigs.table, *base, depth + 1)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// E0820 (kw08, `[abi.layout.packed]`): a packed struct's field may not
/// be, or hold, an `align(N)` struct. The C compilers disagree on that
/// layout by target: gcc and clang on SysV and Apple targets place the
/// aligned member at the next byte (`struct { u8; A16; u8 }` packed is
/// size 18, the member at 1), while clang for the MSVC ABI keeps its
/// alignment (size 48, the member at 16; CI run 37177942003, windows) —
/// so no layout is "the C one", and the shape is refused by name rather
/// than laid out to agree with half the targets.
pub fn check_aligned_in_packed(sigs: &SigTables) -> Vec<wolf_diag::Diagnostic> {
    let mut out = Vec::new();
    for items in &sigs.modules {
        for (name, sig) in items {
            let ItemSig::Struct(ss) = sig else { continue };
            if !ss.packed {
                continue;
            }
            for f in &ss.fields {
                let Some(inner) = aligned_inside(sigs, &sigs.table, f.ty, 0) else {
                    continue;
                };
                out.push(
                    wolf_diag::Diagnostic::error(
                        wolf_diag::codes::E0820,
                        f.span,
                        format!(
                            "the packed struct `{name}` holds the aligned struct `{inner}` in \
                             its field `{}`",
                            f.name
                        ),
                    )
                    .with_label("no single C layout for this field")
                    .with_note(
                        "C compilers disagree on an aligned struct inside a packed one: gcc \
                         and clang on SysV and Apple targets place it at the next byte, the \
                         MSVC ABI keeps its alignment ([abi.layout.packed]). Pack the outer \
                         struct without the aligned one inside it, or drop `packed`.",
                    ),
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_are_their_size() {
        assert_eq!(prim_size(Prim::U8), Some(1));
        assert_eq!(prim_size(Prim::U16), Some(2));
        assert_eq!(prim_size(Prim::Char), Some(4));
        assert_eq!(prim_size(Prim::Int), Some(8));
        assert_eq!(prim_size(Prim::Str), None);
    }
}
