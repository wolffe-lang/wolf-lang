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
        active: Vec::new(),
    }
    .describe(table, id, storage, None, 0)
}

struct Describer<'a> {
    types: types::TypeInterner,
    sigs: &'a SigTables,
    span: Span,
    active: Vec<(u32, String, Storage)>,
}

pub(super) fn byte_view() -> DebugType {
    Type::new(
        "borrowed List[byte] view",
        16,
        Kind::Struct(vec![
            member(
                "data",
                0,
                pointer(Type::new("byte", 1, Kind::Base(Enc::UnsignedChar))),
            ),
            member("len", 8, int()),
        ]),
    )
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
        let nominal = match table.kind(id) {
            TyKind::Nominal { module, .. } => Some((*module, mono_spelling(table, id), storage)),
            _ => None,
        };
        if let Some(key) = &nominal {
            if self.active.contains(key) {
                return Some(opaque(&format!("opaque recursive {}", key.1)));
            }
            self.active.push(key.clone());
        }
        let result = self.describe_inner(table, id, storage, frame, depth);
        if nominal.is_some() {
            self.active.pop();
        }
        result
    }

    fn describe_inner(
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
                Prim::U8 | Prim::U16 | Prim::U32 | Prim::U64 | Prim::Uint => Enc::Unsigned,
                Prim::Char => Enc::Utf,
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
        let mut result = Type::new(name, size, kind);
        if let TyKind::Nominal { module, .. } = table.kind(id) {
            let identity = (*module, result.name.clone());
            std::sync::Arc::make_mut(&mut result).identity = Some(identity);
        }
        Some(result)
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
            // The hashed index: a runtime-private slot table (dense entry
            // index + 1, zero vacant), named opaque, and its slot count.
            fields.push(member("index", 72, pointer(opaque("opaque map index"))));
            fields.push(member("index_cap", 80, int()));
        }
        Type::new(name, if map { 88 } else { 40 }, Kind::Struct(fields))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use wolf_sema::sig::{FieldSig, StructSig};

    fn setup() -> (SigTables, Span) {
        let mut files = wolf_span::SourceMap::new();
        let span = Span::new(files.intern(std::path::Path::new("debug-types.lu")), 0, 0);
        (
            SigTables {
                table: TypeTable::new(),
                modules: Vec::new(),
                module_names: Vec::new(),
                traits: BTreeMap::new(),
                impls: Vec::new(),
                diagnostics: Vec::new(),
                sealed: Vec::new(),
                origins: Default::default(),
            },
            span,
        )
    }

    #[test]
    fn every_scalar_retains_source_name_size_and_encoding() {
        let (sigs, span) = setup();
        let mut table = TypeTable::new();
        for (prim, size, enc) in [
            (Prim::I8, 1, Enc::Signed),
            (Prim::U8, 1, Enc::Unsigned),
            (Prim::I16, 2, Enc::Signed),
            (Prim::U16, 2, Enc::Unsigned),
            (Prim::I32, 4, Enc::Signed),
            (Prim::U32, 4, Enc::Unsigned),
            (Prim::I64, 8, Enc::Signed),
            (Prim::U64, 8, Enc::Unsigned),
            (Prim::Int, 8, Enc::Signed),
            (Prim::Uint, 8, Enc::Unsigned),
            (Prim::Bool, 1, Enc::Boolean),
            (Prim::Byte, 1, Enc::UnsignedChar),
            (Prim::F32, 4, Enc::Float),
            (Prim::F64, 8, Enc::Float),
            (Prim::Char, 4, Enc::Utf),
        ] {
            let id = table.intern(TyKind::Prim(prim));
            let ty = source_type(&table, &sigs, id, span, Storage::Native).unwrap();
            assert_eq!(
                (ty.name.as_str(), ty.size, &ty.kind),
                (prim.name(), size, &Kind::Base(enc))
            );
        }
    }

    #[test]
    fn equal_shaped_nominals_and_storage_views_remain_distinct() {
        let (mut sigs, span) = setup();
        let byte = sigs.table.intern(TyKind::Prim(Prim::U8));
        let wide = sigs.table.intern(TyKind::Prim(Prim::U64));
        let definition = ItemSig::Struct(StructSig {
            generic: false,
            generics: Vec::new(),
            name_span: span,
            fields: vec![
                FieldSig {
                    name: "small".into(),
                    ty: byte,
                    span,
                },
                FieldSig {
                    name: "wide".into(),
                    ty: wide,
                    span,
                },
            ],
            repr_c: true,
            packed: false,
            align: None,
        });
        sigs.modules = vec![
            BTreeMap::from([("Pair".into(), definition.clone())]),
            BTreeMap::from([("Pair".into(), definition)]),
        ];
        let mut table = TypeTable::new();
        let left = table.intern(TyKind::Nominal {
            module: 0,
            name: "Pair".into(),
            args: Vec::new(),
        });
        let right = table.intern(TyKind::Nominal {
            module: 1,
            name: "Pair".into(),
            args: Vec::new(),
        });
        let native = source_type(&table, &sigs, left, span, Storage::Native).unwrap();
        let other = source_type(&table, &sigs, right, span, Storage::Native).unwrap();
        assert_ne!(native, other, "module identity is not structural shape");
        let flat = source_type(&table, &sigs, left, span, Storage::Flat).unwrap();
        let Kind::Struct(n) = &native.kind else {
            panic!("structure")
        };
        let Kind::Struct(f) = &flat.kind else {
            panic!("structure")
        };
        assert_eq!((native.size, n[1].offset), (16, 8));
        assert_eq!((flat.size, f[1].offset), (9, 1));
        let list = table.intern(TyKind::List(left));
        let header = source_type(&table, &sigs, list, span, Storage::Native).unwrap();
        let Kind::Pointer(header) = &header.kind else {
            panic!("header pointer")
        };
        let Kind::Struct(fields) = &header.kind else {
            panic!("header structure")
        };
        let Kind::Pointer(element) = &fields[0].ty.kind else {
            panic!("element pointer")
        };
        let Kind::Struct(fields) = &element.kind else {
            panic!("element structure")
        };
        assert_eq!(
            (element.size, fields[1].offset),
            (16, 1),
            "packed fields, padded element stride"
        );
    }
}
