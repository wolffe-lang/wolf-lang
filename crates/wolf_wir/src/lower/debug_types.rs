//! Freeze source types before WIR erases names, variants and pointees.
use super::*;
use crate::debug::{
    DebugDiscriminant as Discr, DebugEncoding as Enc, DebugKind as Kind, DebugMember as Member,
    DebugType, DebugTypeData as Type, DebugVariant as Variant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Storage {
    Native,
    Flat,
    Raw,
}

/// One immutable signature universe, owned by the lowering unit rather than a
/// body (specialization body tables are temporary). No table-local id or table
/// address escapes into this root cache. Layout ids remain private to debugging.
pub(super) struct Context {
    types: types::TypeInterner,
    roots: HashMap<(Bound, Storage, bool), DebugType>,
    byte_view: Option<DebugType>,
}

impl Context {
    pub(super) fn new() -> Self {
        Self {
            types: types::TypeInterner::new(),
            roots: HashMap::new(),
            byte_view: None,
        }
    }

    pub(super) fn binding_type(
        &mut self,
        table: &TypeTable,
        sigs: &SigTables,
        id: TyId,
        span: Span,
        reference: bool,
    ) -> Option<DebugType> {
        let storage = if reference {
            Storage::Flat
        } else {
            Storage::Native
        };
        self.source_type(table, sigs, id, span, storage, reference)
    }

    fn source_type(
        &mut self,
        table: &TypeTable,
        sigs: &SigTables,
        id: TyId,
        span: Span,
        storage: Storage,
        reference: bool,
    ) -> Option<DebugType> {
        let enabled = cache_enabled();
        let key = if enabled {
            root_key(table, id).map(|ty| (ty, storage, reference))
        } else {
            None
        };
        if let Some(ty) = key.as_ref().and_then(|key| self.roots.get(key)) {
            return Some(ty.clone());
        }
        // The test-only reference path recreates the old per-binding interner
        // as well as disabling both caches. Production always uses the unit's.
        let mut fresh = (!enabled).then(types::TypeInterner::new);
        let ty = Describer {
            types: fresh.as_mut().unwrap_or(&mut self.types),
            sigs,
            span,
            active: Vec::new(),
            memo: enabled.then(HashMap::new),
        }
        .describe(table, id, storage, None, 0)
        .map(|ty| {
            if reference {
                Type::new(format!("mut {}", ty.name), 8, Kind::Reference(ty))
            } else {
                ty
            }
        });
        // A refusal must not be reused at a different source span.
        if let (Some(key), Some(ty)) = (key, &ty) {
            self.roots.insert(key, ty.clone());
        }
        ty
    }

    pub(super) fn byte_view(&mut self) -> DebugType {
        if !cache_enabled() {
            return byte_view();
        }
        self.byte_view.get_or_insert_with(byte_view).clone()
    }
}

/// Bound freezes these concrete shapes without retaining any table-local id.
/// Other shapes (notably anonymous Distinct, projections, and unresolved types)
/// keep the uncached root path. Bound's catch-all Leaf is NOT a general key.
/// Bound the walk itself: a compact structural DAG must not explode while we
/// merely decide whether to memoize it. Nominal definitions are not unfolded.
fn root_key(table: &TypeTable, id: TyId) -> Option<Bound> {
    fn closed(table: &TypeTable, id: TyId, depth: u32, left: &mut usize) -> bool {
        if depth > 32 || *left == 0 {
            return false;
        }
        *left -= 1;
        let mut child = |id| closed(table, id, depth + 1, left);
        match table.kind(id) {
            TyKind::Unit | TyKind::Never | TyKind::Prim(_) => true,
            TyKind::Wrapping(t)
            | TyKind::Ptr(t)
            | TyKind::List(t)
            | TyKind::Pool(t)
            | TyKind::Range(t) => child(*t),
            TyKind::Map(a, b) | TyKind::ErrUnion(a, b) => child(*a) && child(*b),
            TyKind::Tuple(ts) | TyKind::Nominal { args: ts, .. } => ts.iter().all(|t| child(*t)),
            TyKind::Fn(args, ret) => args.iter().all(|t| child(*t)) && child(*ret),
            TyKind::Row { tags, tail } => {
                tags.iter()
                    .all(|(_, payload)| payload.iter().all(|t| child(*t)))
                    && tail.is_none_or(|t| child(t))
            }
            _ => false,
        }
    }
    closed(table, id, 0, &mut 4096).then(|| freeze(table, id))
}

#[cfg(test)]
thread_local! {
    static UNCACHED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn cache_enabled() -> bool {
    #[cfg(test)]
    {
        !UNCACHED.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        true
    }
}

#[cfg(test)]
pub(super) fn source_type(
    table: &TypeTable,
    sigs: &SigTables,
    id: TyId,
    span: Span,
    storage: Storage,
) -> Option<DebugType> {
    Context::new().source_type(table, sigs, id, span, storage, false)
}

/// This memo dies at the end of ONE root traversal. Its borrowed table identities
/// cannot survive a temporary specialization table. Frame stack addresses are
/// deliberately not identities: the full chain is copied, including parents.
#[derive(Clone, PartialEq, Eq, Hash)]
struct FrameKey {
    names: Vec<String>,
    table: *const TypeTable,
    args: Vec<TyId>,
    parent: Option<Box<FrameKey>>,
}

impl FrameKey {
    fn of(frame: &RigidFrame<'_>) -> Self {
        Self {
            names: frame.names.to_vec(),
            table: frame.table,
            args: frame.args.to_vec(),
            parent: frame.parent.map(|parent| Box::new(Self::of(parent))),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct MemoKey {
    table: *const TypeTable,
    id: TyId,
    storage: Storage,
    depth: u32,
    active: Vec<(u32, String, Storage)>,
    frame: Option<FrameKey>,
}

struct Describer<'a> {
    types: &'a mut types::TypeInterner,
    sigs: &'a SigTables,
    span: Span,
    active: Vec<(u32, String, Storage)>,
    memo: Option<HashMap<MemoKey, Option<DebugType>>>,
}

fn byte_view() -> DebugType {
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
        if let Some(key) = &nominal
            && self.active.contains(key)
        {
            return Some(opaque(&format!("opaque recursive {}", key.1)));
        }
        let key = self.memo.as_ref().map(|_| MemoKey {
            table,
            id,
            storage,
            depth,
            active: self.active.clone(),
            frame: frame.map(FrameKey::of),
        });
        if let Some(cached) = key.as_ref().and_then(|key| self.memo.as_ref()?.get(key)) {
            return cached.clone();
        }
        if let Some(key) = &nominal {
            self.active.push(key.clone());
        }
        let result = self.describe_inner(table, id, storage, frame, depth);
        if nominal.is_some() {
            self.active.pop();
        }
        if let (Some(key), Some(memo)) = (key, &mut self.memo) {
            memo.insert(key, result.clone());
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
        let wire = wir_ty_frame(self.types, table, sigs, id, self.span, 0, frame).ok()??;
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
                flat_size(self.types, wire)?,
                flat_offsets(self.types, &fields)?,
            )
        } else if storage == Storage::Raw {
            let raw = raw_lay(self.types, Some((table, sigs, id)), wire, 0)?;
            (
                raw.size,
                raw.fields.into_iter().map(|(offset, _)| offset).collect(),
            )
        } else {
            let size = natural_layout(self.types, wire)?.0;
            let mut offset = 0u64;
            let mut offsets = Vec::new();
            for field in &fields {
                let (size, align) = natural_layout(self.types, *field)?;
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
                let ew =
                    wir_ty_frame(self.types, table, sigs, *elem, self.span, 0, frame).ok()??;
                std::sync::Arc::make_mut(&mut element).size = list_stride_of(self.types, ew)?;
                return Some(pointer(self.header(&name, element, false)));
            }
            TyKind::Map(key, value) => {
                let kw =
                    wir_ty_frame(self.types, table, sigs, *key, self.span, 0, frame).ok()??;
                let vw =
                    wir_ty_frame(self.types, table, sigs, *value, self.span, 0, frame).ok()??;
                let key = self.describe(table, *key, Storage::Flat, frame, depth + 1)?;
                let value = self.describe(table, *value, Storage::Flat, frame, depth + 1)?;
                let pair = self.types.intern(types::TypeData::Agg(vec![kw, vw]));
                let entry = Type::new(
                    format!("{name} entry"),
                    list_stride_of(self.types, pair)?,
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
        // A list's header is `{data, len, cap, region}` since
        // wolf-lang#1177; a map's keeps its entry stride before the region.
        let words: &[&str] = if map {
            &["len", "cap", "elem"]
        } else {
            &["len", "cap"]
        };
        for (i, name) in words.iter().enumerate() {
            fields.push(member(*name, (i as u64 + 1) * 8, int()));
        }
        let region = if map { 32 } else { 24 };
        fields.push(member("region", region, pointer(opaque("opaque region"))));
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
        Type::new(name, if map { 88 } else { 32 }, Kind::Struct(fields))
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

    fn uncached<T>(f: impl FnOnce() -> T) -> T {
        struct Restore(bool);
        impl Drop for Restore {
            fn drop(&mut self) {
                UNCACHED.with(|flag| flag.set(self.0));
            }
        }
        let _restore = Restore(UNCACHED.with(|flag| flag.replace(true)));
        f()
    }

    fn compare_description(table: &TypeTable, sigs: &SigTables, id: TyId, span: Span) {
        let mut context = Context::new();
        for storage in [Storage::Native, Storage::Flat, Storage::Raw] {
            let actual = context
                .source_type(table, sigs, id, span, storage, false)
                .unwrap();
            let expected = uncached(|| source_type(table, sigs, id, span, storage)).unwrap();
            assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
        }
    }

    #[test]
    fn root_keys_cross_tables_without_reusing_local_ids() {
        let (sigs, span) = setup();
        let mut left = TypeTable::new();
        let mut right = TypeTable::new();
        let int = left.intern(TyKind::Prim(Prim::Int));
        let byte = right.intern(TyKind::Prim(Prim::Byte));
        let a = left.intern(TyKind::Tuple(vec![int]));
        let b = right.intern(TyKind::Tuple(vec![byte]));
        assert_eq!(a, b, "the fixture must collide in table-local ids");
        let mut context = Context::new();
        let x = context
            .source_type(&left, &sigs, a, span, Storage::Native, false)
            .unwrap();
        let y = context
            .source_type(&right, &sigs, b, span, Storage::Native, false)
            .unwrap();
        assert_ne!(x, y);
        assert_eq!((x.size, y.size), (8, 1));
        let other_int = right.intern(TyKind::Prim(Prim::Int));
        let same = right.intern(TyKind::Tuple(vec![other_int]));
        assert_ne!(a, same);
        let shared = context
            .source_type(&right, &sigs, same, span, Storage::Native, false)
            .unwrap();
        assert!(std::sync::Arc::ptr_eq(&x, &shared));
        // These forms contain table-local references that Bound::Leaf does not
        // freeze; they must never silently become persistent keys.
        let anonymous = left.intern(TyKind::Distinct(int));
        assert!(root_key(&left, anonymous).is_none());
        println!("PASS debug-description-table-identity");
    }

    #[test]
    fn recursive_memo_keeps_depth_and_active_nominals() {
        let (mut sigs, span) = setup();
        let a = sigs.table.intern(TyKind::Nominal {
            module: 0,
            name: "A".into(),
            args: vec![],
        });
        let b = sigs.table.intern(TyKind::Nominal {
            module: 0,
            name: "B".into(),
            args: vec![],
        });
        let x = sigs.table.intern(TyKind::Nominal {
            module: 0,
            name: "X".into(),
            args: vec![],
        });
        let pa = sigs.table.intern(TyKind::Ptr(a));
        let pb = sigs.table.intern(TyKind::Ptr(b));
        let px = sigs.table.intern(TyKind::Ptr(x));
        let structure = |fields: Vec<(&str, TyId)>| {
            ItemSig::Struct(StructSig {
                generic: false,
                generics: vec![],
                name_span: span,
                fields: fields
                    .into_iter()
                    .map(|(name, ty)| FieldSig {
                        name: name.into(),
                        ty,
                        span,
                    })
                    .collect(),
                repr_c: true,
                packed: false,
                align: None,
            })
        };
        sigs.modules = vec![BTreeMap::from([
            ("A".into(), structure(vec![("x", px)])),
            ("B".into(), structure(vec![("x", px)])),
            ("X".into(), structure(vec![("a", pa), ("b", pb)])),
        ])];
        // X is reached at the SAME depth with A active on one path and B on
        // the other. A key that omits the active stack changes its opaque gaps.
        let root = sigs.table.intern(TyKind::Tuple(vec![pa, pb]));
        compare_description(&sigs.table, &sigs, root, span);
        let mut table = TypeTable::new();
        let byte = table.intern(TyKind::Prim(Prim::Byte));
        let shallow = table.intern(TyKind::Ptr(byte));
        let mut deep = shallow;
        for _ in 0..32 {
            deep = table.intern(TyKind::Ptr(deep));
        }
        let root = table.intern(TyKind::Tuple(vec![shallow, deep]));
        compare_description(&table, &sigs, root, span);
        let expected =
            uncached(|| source_type(&table, &sigs, root, span, Storage::Native)).unwrap();
        assert!(format!("{expected:?}").contains("opaque recursive"));
        println!("PASS debug-description-recursion-context");
    }

    #[test]
    fn recursive_memo_keeps_the_complete_rigid_frame_chain() {
        let (mut sigs, span) = setup();
        let rigid = sigs.table.intern(TyKind::Rigid("T".into()));
        let mut table = TypeTable::new();
        let int = table.intern(TyKind::Prim(Prim::Int));
        let byte = table.intern(TyKind::Prim(Prim::Byte));
        let names = vec!["T".to_string()];
        let outer_a_args = [int];
        let outer_b_args = [byte];
        let outer_a = RigidFrame {
            names: &names,
            table: &table,
            args: &outer_a_args,
            parent: None,
        };
        let outer_b = RigidFrame {
            names: &names,
            table: &table,
            args: &outer_b_args,
            parent: None,
        };
        let inner_names = vec!["U".to_string()];
        let inner_args = [byte];
        let inner_a = RigidFrame {
            names: &inner_names,
            table: &table,
            args: &inner_args,
            parent: Some(&outer_a),
        };
        let inner_b = RigidFrame {
            names: &inner_names,
            table: &table,
            args: &inner_args,
            parent: Some(&outer_b),
        };
        let mut types = types::TypeInterner::new();
        let mut describer = Describer {
            types: &mut types,
            sigs: &sigs,
            span,
            active: vec![],
            memo: Some(HashMap::new()),
        };
        let a = describer
            .describe(&sigs.table, rigid, Storage::Native, Some(&inner_a), 0)
            .unwrap();
        let b = describer
            .describe(&sigs.table, rigid, Storage::Native, Some(&inner_b), 0)
            .unwrap();
        assert_eq!((a.name.as_str(), a.size), ("int", 8));
        assert_eq!((b.name.as_str(), b.size), ("byte", 1));
        println!("PASS debug-description-rigid-context");
    }

    #[test]
    fn cached_lowering_preserves_wir_and_debug_binding_bytes() {
        use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};
        let sources = [
            "struct Pair { small: i8, wide: int }\n\
             struct Node { value: int, next: List[Node] }\n\
             fn views(mut value: Pair, raw: *Pair, xs: List[Pair], pairs: Map[int, Pair], optional: int ! {none}, node: Node) -> int { let pointer = raw\n0 }\n\
             fn carry[T](value: T) -> T { let local = value\nlocal }\n\
             fn main() -> int { carry(3 as u8)\ncarry(4 as byte)\n0 }\n",
            "fn total(bs: List[byte]) -> int { var n = 0\nfor b in bs { n = n + b }\nn + bs.len + bs[0] as int }\n\
             fn main() -> !int { total(\"wolf\".bytes()) - 563 }\n",
        ];
        for (index, source) in sources.iter().enumerate() {
            let mut loader = MemoryLoader::new("debug_fidelity");
            loader.add_file(&[], "main.lu", source);
            let resolved = resolve_package_with(&mut loader, &AliasTable::default(), true).unwrap();
            assert!(
                resolved
                    .diagnostics
                    .iter()
                    .all(|d| d.severity != wolf_diag::Severity::Error)
            );
            let checked = typecheck_package_with(&resolved.package, true);
            assert!(!checked.has_errors(), "{:?}", checked.diagnostics);
            assert!(checked.not_yet.is_empty());
            let memory = wolf_mem::check_package(&resolved.package, &checked);
            assert!(memory.not_yet.is_empty());
            assert!(
                memory
                    .diagnostics
                    .iter()
                    .all(|d| d.severity != wolf_diag::Severity::Error),
                "{:?}",
                memory.diagnostics
            );
            let reference = uncached(|| lower_package(&resolved.package, &checked));
            let cached = lower_package(&resolved.package, &checked);
            assert!(reference.not_yet.is_empty(), "{:?}", reference.not_yet);
            assert!(cached.not_yet.is_empty(), "{:?}", cached.not_yet);
            crate::verify_module(&reference.module).unwrap();
            crate::verify_module(&cached.module).unwrap();
            assert_eq!(
                crate::print_module(&cached.module),
                crate::print_module(&reference.module)
            );
            let dump = |build: &Build| {
                build
                    .module
                    .funcs
                    .values()
                    .map(|f| format!("{} {:?} {:?}\n", f.name, f.src_file, f.debug_vars))
                    .collect::<String>()
            };
            let want = dump(&reference);
            assert!(want.contains("DebugTypeData"));
            if index == 0 {
                assert!(want.contains("mut Pair"));
                assert!(want.contains("opaque recursive"));
                assert!(want.contains("carry.mono"));
            } else {
                assert!(want.contains("borrowed List[byte] view"));
                assert!(
                    reference
                        .module
                        .funcs
                        .values()
                        .any(|f| f.debug_vars.iter().any(|v| !v.pieces.is_empty()))
                );
            }
            let got = dump(&cached);
            if got != want {
                eprintln!("FAIL debug-description-byte-identity");
            }
            assert_eq!(got, want);
        }
        println!("PASS debug-description-byte-identity");
    }

    #[test]
    fn every_scalar_retains_source_name_size_and_encoding() {
        let (sigs, span) = setup();
        let mut table = TypeTable::new();
        let mut context = Context::new();
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
            let ty = context
                .source_type(&table, &sigs, id, span, Storage::Native, false)
                .unwrap();
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
        let mut context = Context::new();
        let native = context
            .source_type(&table, &sigs, left, span, Storage::Native, false)
            .unwrap();
        let other = context
            .source_type(&table, &sigs, right, span, Storage::Native, false)
            .unwrap();
        assert_ne!(native, other, "module identity is not structural shape");
        let flat = context
            .source_type(&table, &sigs, left, span, Storage::Flat, false)
            .unwrap();
        let Kind::Struct(n) = &native.kind else {
            panic!("structure")
        };
        let Kind::Struct(f) = &flat.kind else {
            panic!("structure")
        };
        assert_eq!((native.size, n[1].offset), (16, 8));
        assert_eq!((flat.size, f[1].offset), (9, 1));
        let list = table.intern(TyKind::List(left));
        // The same flat Pair is reached at depth two through List and Tuple.
        // Padding a list element must not mutate the memo's packed Pair view.
        let tuple = table.intern(TyKind::Tuple(vec![left]));
        let both = table.intern(TyKind::Tuple(vec![list, tuple]));
        compare_description(&table, &sigs, both, span);
        let header = context
            .source_type(&table, &sigs, list, span, Storage::Native, false)
            .unwrap();
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
