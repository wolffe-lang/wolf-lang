//! Freeze source types before WIR erases names, variants and pointees.
use super::*;
use crate::debug::{
    DebugDiscriminant as Discr, DebugEncoding as Enc, DebugKind as Kind, DebugMember as Member,
    DebugType, DebugTypeData as Type, DebugVariant as Variant,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Storage {
    Native,
    Flat,
    Raw,
}

pub(super) fn source_type(
    table: &TypeTable,
    sigs: &SigTables,
    id: TyId,
    span: Span,
    storage: Storage,
) -> Option<DebugType> {
    // A private layout interner avoids changing canonical WIR type numbering or
    // code generation merely because a debugger needs a List's element type.
    Describer {
        types: types::TypeInterner::new(),
        sigs,
        span,
    }
    .describe(table, id, storage, None, 0)
}

struct Describer<'a> {
    types: types::TypeInterner,
    sigs: &'a SigTables,
    span: Span,
}

fn int() -> DebugType {
    Type::new("int", 8, Kind::Base(Enc::Signed))
}
fn opaque(name: &str) -> DebugType {
    Type::new(name, 0, Kind::Opaque)
}
fn pointer(ty: DebugType) -> DebugType {
    Type::new(format!("*{}", ty.name), 8, Kind::Pointer(ty))
}
fn member(name: impl Into<String>, offset: u64, ty: DebugType) -> Member {
    Member {
        name: name.into(),
        offset,
        ty,
    }
}

impl Describer<'_> {
    fn describe(
        &mut self,
        table: &TypeTable,
        id: TyId,
        storage: Storage,
        frame: Option<&RigidFrame<'_>>,
        depth: u32,
    ) -> Option<DebugType> {
        let sigs = self.sigs;
        let name = mono_spelling(table, id);
        if depth > 32 {
            return Some(opaque(&format!("opaque recursive {name}")));
        }
        if let TyKind::Rigid(n) = table.kind(id) {
            let (table, id, parent) = frame?.lookup(n)?;
            return self.describe(table, id, storage, parent, depth + 1);
        }
        let wire = wir_ty_frame(&mut self.types, table, sigs, id, self.span, 0, frame).ok()??;
        let fields = match self.types.get(wire) {
            types::TypeData::Agg(fields) => fields.clone(),
            types::TypeData::Eu { ok, slots } => {
                let mut fields = vec![types::I64];
                fields.extend(ok.iter().copied());
                fields.extend(slots.iter().copied());
                fields
            }
            _ => Vec::new(),
        };
        let (size, offsets) = if storage == Storage::Flat {
            (
                flat_size(&self.types, wire)?,
                flat_offsets(&self.types, &fields)?,
            )
        } else if storage == Storage::Raw {
            let raw = raw_lay(&self.types, Some((table, sigs, id)), wire, 0)?;
            (
                raw.size,
                raw.fields.into_iter().map(|(offset, _)| offset).collect(),
            )
        } else {
            let size = natural_layout(&self.types, wire)?.0;
            let mut offset = 0u64;
            let mut offsets = Vec::new();
            for field in &fields {
                let (size, align) = natural_layout(&self.types, *field)?;
                offset = offset.next_multiple_of(align);
                offsets.push(offset);
                offset += size;
            }
            (size, offsets)
        };
        let kind = match table.kind(id) {
            TyKind::Prim(Prim::Str) => Kind::Struct(vec![
                member(
                    "data",
                    0,
                    pointer(Type::new("byte", 1, Kind::Base(Enc::UnsignedChar))),
                ),
                member("len", 8, int()),
            ]),
            TyKind::Prim(p) => Kind::Base(match p {
                Prim::Bool => Enc::Boolean,
                Prim::F32 | Prim::F64 => Enc::Float,
                Prim::Byte => Enc::UnsignedChar,
                Prim::U8 | Prim::U16 | Prim::U32 | Prim::U64 | Prim::Uint | Prim::Char => {
                    Enc::Unsigned
                }
                _ => Enc::Signed,
            }),
            TyKind::Wrapping(inner) | TyKind::Distinct(inner) => {
                return self.describe(table, *inner, storage, frame, depth + 1);
            }
            TyKind::Ptr(inner) => Kind::Pointer(
                self.describe(table, *inner, Storage::Raw, frame, depth + 1)
                    .unwrap_or_else(|| opaque(&mono_spelling(table, *inner))),
            ),
            TyKind::Tuple(ts) => {
                let mut members = Vec::new();
                for (i, ty) in ts.iter().enumerate() {
                    members.push(member(
                        format!("_{i}"),
                        offsets[i],
                        self.describe(table, *ty, storage, frame, depth + 1)?,
                    ));
                }
                Kind::Struct(members)
            }
            TyKind::Nominal {
                module,
                name: item,
                args,
            } => match sigs.get(*module as usize, item)? {
                ItemSig::Distinct { base, .. } => {
                    return self.describe(&sigs.table, *base, storage, frame, depth + 1);
                }
                ItemSig::Struct(ss) => {
                    let inner = RigidFrame {
                        names: &ss.generics,
                        table,
                        args,
                        parent: frame,
                    };
                    let nested = if ss.generic { Some(&inner) } else { frame };
                    let mut members = Vec::new();
                    for (i, field) in ss.fields.iter().enumerate() {
                        members.push(member(
                            &field.name,
                            offsets[i],
                            self.describe(&sigs.table, field.ty, storage, nested, depth + 1)?,
                        ));
                    }
                    Kind::Struct(members)
                }
                ItemSig::Enum {
                    variants,
                    generic,
                    generics,
                    ..
                } => {
                    if variants.iter().all(|v| v.payload.is_empty()) {
                        Kind::Enum(
                            variants
                                .iter()
                                .enumerate()
                                .map(|(i, v)| (v.name.clone(), i as i64))
                                .collect(),
                        )
                    } else {
                        let inner = RigidFrame {
                            names: generics,
                            table,
                            args,
                            parent: frame,
                        };
                        let nested = if *generic { Some(&inner) } else { frame };
                        let mut cases = Vec::new();
                        for (tag, variant) in variants.iter().enumerate() {
                            let mut members = Vec::new();
                            for (i, ty) in variant.payload.iter().enumerate() {
                                // A union slot's typed view uses its natural layout,
                                // even when the enclosing aggregate is packed.
                                let view = if self.types.is_union(fields[i + 1]) {
                                    Storage::Native
                                } else {
                                    storage
                                };
                                members.push(member(
                                    format!("_{i}"),
                                    offsets[i + 1],
                                    self.describe(&sigs.table, *ty, view, nested, depth + 1)?,
                                ));
                            }
                            cases.push(Variant {
                                name: variant.name.clone(),
                                discriminant: Discr::Value(tag as i64),
                                members,
                            });
                        }
                        Kind::Variants(cases)
                    }
                }
                _ => Kind::Opaque,
            },
            TyKind::ErrUnion(ok, row) => {
                if row_is_empty(table, *row) {
                    return self.describe(table, *ok, storage, frame, depth + 1);
                }
                let has_ok = matches!(
                    self.types.get(wire),
                    types::TypeData::Eu { ok: Some(_), .. }
                );
                let success = if has_ok {
                    vec![member(
                        "value",
                        offsets[1],
                        self.describe(table, *ok, storage, frame, depth + 1)?,
                    )]
                } else {
                    Vec::new()
                };
                let mut cases = vec![Variant {
                    name: "ok".into(),
                    discriminant: Discr::Value(0),
                    members: success,
                }];
                let TyKind::Row { tags, tail } = table.kind(*row) else {
                    return None;
                };
                self.row_cases(
                    &mut cases,
                    table,
                    tags,
                    tail.is_some(),
                    &fields,
                    &offsets,
                    1 + usize::from(has_ok),
                    storage,
                    frame,
                    depth,
                )?;
                Kind::Variants(cases)
            }
            TyKind::Row { tags, tail } => {
                let mut cases = Vec::new();
                self.row_cases(
                    &mut cases,
                    table,
                    tags,
                    tail.is_some(),
                    &fields,
                    &offsets,
                    1,
                    storage,
                    frame,
                    depth,
                )?;
                Kind::Variants(cases)
            }
            TyKind::List(elem) => {
                let mut element = self.describe(table, *elem, Storage::Flat, frame, depth + 1)?;
                let ew = wir_ty_frame(&mut self.types, table, sigs, *elem, self.span, 0, frame)
                    .ok()??;
                std::sync::Arc::make_mut(&mut element).size = list_stride_of(&self.types, ew)?;
                return Some(pointer(self.header(&name, element, false)));
            }
            TyKind::Map(key, value) => {
                let kw = wir_ty_frame(&mut self.types, table, sigs, *key, self.span, 0, frame)
                    .ok()??;
                let vw = wir_ty_frame(&mut self.types, table, sigs, *value, self.span, 0, frame)
                    .ok()??;
                let key = self.describe(table, *key, Storage::Flat, frame, depth + 1)?;
                let value = self.describe(table, *value, Storage::Flat, frame, depth + 1)?;
                let pair = self.types.intern(types::TypeData::Agg(vec![kw, vw]));
                let entry = Type::new(
                    format!("{name} entry"),
                    list_stride_of(&self.types, pair)?,
                    Kind::Struct(vec![
                        member("key", 0, key.clone()),
                        member("value", key.size, value),
                    ]),
                );
                return Some(pointer(self.header(&name, entry, true)));
            }
            TyKind::Range(t) => Kind::Struct(vec![
                member(
                    "start",
                    offsets[0],
                    self.describe(table, *t, storage, frame, depth + 1)?,
                ),
                member(
                    "end",
                    offsets[1],
                    self.describe(table, *t, storage, frame, depth + 1)?,
                ),
            ]),
            // Runtime-private objects are named opaque pointees, not invented
            // fields inferred from their pointer-sized machine representation.
            _ if wire == types::PTR => return Some(pointer(opaque(&format!("opaque {name}")))),
            _ => Kind::Opaque,
        };
        Some(Type::new(name, size, kind))
    }

    fn header(&self, name: &str, element: DebugType, map: bool) -> DebugType {
        let mut fields = vec![member("data", 0, pointer(element))];
        for (i, name) in ["len", "cap", "elem"].iter().enumerate() {
            fields.push(member(*name, (i as u64 + 1) * 8, int()));
        }
        fields.push(member("region", 32, pointer(opaque("opaque region"))));
        if map {
            for (i, name) in ["key_kind", "key_size", "val_off", "val_size"]
                .iter()
                .enumerate()
            {
                fields.push(member(*name, 40 + i as u64 * 8, int()));
            }
        }
        Type::new(name, if map { 72 } else { 40 }, Kind::Struct(fields))
    }

    #[allow(clippy::too_many_arguments)]
    fn row_cases(
        &mut self,
        cases: &mut Vec<Variant>,
        table: &TypeTable,
        tags: &[(String, Vec<TyId>)],
        open: bool,
        fields: &[TypeId],
        offsets: &[u64],
        first: usize,
        storage: Storage,
        frame: Option<&RigidFrame<'_>>,
        depth: u32,
    ) -> Option<()> {
        for (tag, payload) in tags {
            let mut members = Vec::new();
            for (i, ty) in payload.iter().enumerate() {
                let view = if self.types.is_union(fields[first + i]) {
                    Storage::Native
                } else {
                    storage
                };
                members.push(member(
                    format!("_{i}"),
                    offsets[first + i],
                    self.describe(table, *ty, view, frame, depth + 1)?,
                ));
            }
            cases.push(Variant {
                name: tag.clone(),
                discriminant: Discr::ErrorTag(tag.clone()),
                members,
            });
        }
        if open {
            cases.push(Variant {
                name: "unknown_error".into(),
                discriminant: Discr::Default,
                members: Vec::new(),
            });
        }
        Some(())
    }
}
