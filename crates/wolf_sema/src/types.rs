//! The interned type table (s13, Target 3).
//!
//! Types are hash-consed into a flat arena: structurally equal types
//! intern to the same [`TyId`], so equality fast-paths on index
//! comparison (`wolf_wir`-style idioms). The table is per-unit-of-work:
//! signature elaboration builds a base table, and each body checker
//! clones it — body checking never shares mutable state (Target 5).
//!
//! Interning is append-only. Speculative checking (snapshot/rollback,
//! [`crate::unify`]) never needs to un-intern: a type minted during a
//! rolled-back trial is merely unreferenced, which cannot change any
//! equality answer — hash-consing is idempotent.

use std::collections::HashMap;

/// Index of an interned type in a [`TypeTable`]. Equality of ids is
/// equality of types within one table (hash-consing invariant).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct TyId(u32);

impl TyId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The built-in primitive scalar types (spec 02's closed inventory).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Prim {
    Bool,
    Str,
    /// An octet (D72, s135): `0..=255`, 1 byte, alignment 1 — the
    /// element type of every byte buffer. Deliberately NOT an integer
    /// type, `char`'s posture (`[type.byte]`): no numeric-literal
    /// adoption, no closed arithmetic (a byte operand widens to `int`
    /// and the result is `int`, `[type.byte.op]`); the bridges are the
    /// two spelled casts (`byte as int` widens, `int as byte` keeps
    /// the low eight bits — `[type.byte.cast]`).
    Byte,
    Int,
    Uint,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    /// A Unicode scalar value (D58, s121): `0..=0x10FFFF` excluding
    /// the surrogate gap `0xD800..=0xDFFF`. Deliberately NOT an
    /// integer type — no arithmetic, no numeric-literal adoption; the
    /// only numeric bridges are the spelled casts (`char as int`
    /// total, `int as char` trapping per D56's family).
    Char,
}

impl Prim {
    /// Parse a builtin type name (must agree with
    /// [`crate::prelude::BUILTIN_TYPES`]; `wrapping` is a constructor,
    /// not a prim).
    pub fn from_name(name: &str) -> Option<Prim> {
        Some(match name {
            "bool" => Prim::Bool,
            "str" => Prim::Str,
            "byte" => Prim::Byte,
            "int" => Prim::Int,
            "uint" => Prim::Uint,
            "i8" => Prim::I8,
            "i16" => Prim::I16,
            "i32" => Prim::I32,
            "i64" => Prim::I64,
            "u8" => Prim::U8,
            "u16" => Prim::U16,
            "u32" => Prim::U32,
            "u64" => Prim::U64,
            "f32" => Prim::F32,
            "f64" => Prim::F64,
            "char" => Prim::Char,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Prim::Bool => "bool",
            Prim::Str => "str",
            Prim::Byte => "byte",
            Prim::Int => "int",
            Prim::Uint => "uint",
            Prim::I8 => "i8",
            Prim::I16 => "i16",
            Prim::I32 => "i32",
            Prim::I64 => "i64",
            Prim::U8 => "u8",
            Prim::U16 => "u16",
            Prim::U32 => "u32",
            Prim::U64 => "u64",
            Prim::F32 => "f32",
            Prim::F64 => "f64",
            Prim::Char => "char",
        }
    }

    /// Is this one of the integer types (the `{integer}` literal kind)?
    /// `byte` is not one (D72): a literal adopts no byte, and byte
    /// arithmetic is `int`'s — see [`Prim::Byte`].
    pub fn is_integer(self) -> bool {
        matches!(
            self,
            Prim::Int
                | Prim::Uint
                | Prim::I8
                | Prim::I16
                | Prim::I32
                | Prim::I64
                | Prim::U8
                | Prim::U16
                | Prim::U32
                | Prim::U64
        )
    }

    /// Is this one of the float types (the `{float}` literal kind)?
    pub fn is_float(self) -> bool {
        matches!(self, Prim::F32 | Prim::F64)
    }
}

/// The closed family of comptime reflection types (s16, D29). Small on
/// purpose: reflection consumes and produces *semantic* values — types,
/// fields, variants, traits — never syntax (the D29 amendment).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum MetaTy {
    /// `typeinfo(T)` — kind, name, fields, variants, implemented
    /// traits. Field offsets stay unresolved until c05 lays types out.
    TypeInfo,
    /// `typeinfo(T).fields` — the field list (`.len` today; indexing
    /// arrives with s17 brackets).
    FieldList,
    /// One field description: `.name`, `.ty`.
    Field,
    /// `typeinfo(T).variants` — the enum variant list.
    VariantList,
    /// `typeinfo(T).traits` — the implemented-trait name list.
    TraitList,
}

impl MetaTy {
    pub fn name(self) -> &'static str {
        match self {
            MetaTy::TypeInfo => "TypeInfo",
            MetaTy::FieldList => "FieldList",
            MetaTy::Field => "Field",
            MetaTy::VariantList => "VariantList",
            MetaTy::TraitList => "TraitList",
        }
    }
}

/// One interned type's structure. Children are [`TyId`]s into the same
/// table, so the arena is flat and cycles are unrepresentable (nominal
/// types refer to their definition by name, never by structure).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TyKind {
    /// A broken tree types as `<error>` and unifies with everything
    /// silently (D22 — no cascades off a wreck).
    Error,
    /// The type of `return`/`break`/diverging expressions; unifies with
    /// everything (bottom).
    Never,
    /// The unit type: blocks without a value, `fn` without `->`.
    Unit,
    Prim(Prim),
    /// `wrapping[T]` over an integer prim (X3: intended overflow is a
    /// type, never a mode).
    Wrapping(TyId),
    Tuple(Vec<TyId>),
    /// `fn(params) -> ret`.
    Fn(Vec<TyId>, TyId),
    /// `!T` / `T ! {row}` — the error channel (D30, s15): the ok type
    /// plus the row. The row position holds a [`TyKind::Row`], a
    /// [`TyKind::Rigid`] row variable (a generic signature's tail), a
    /// [`TyKind::Var`] (instantiation), or — transiently, inside
    /// signature elaboration only — a [`TyKind::InferredRow`] marker.
    ErrUnion(TyId, TyId),
    /// A structural error row: payload-carrying tags, canonically
    /// sorted by tag name (wolf rows are *sets* — duplicate labels are
    /// rejected at elaboration, E0601), plus an optional polymorphic
    /// tail (`{A, B | e}` internally; the surface spells tails as a
    /// row entry naming a generic parameter). The row also serves as
    /// the type of a caught error value (`else |err| …`).
    Row {
        tags: Vec<(String, Vec<TyId>)>,
        tail: Option<TyId>,
    },
    /// The pre-sealing marker of a `-> !T` inferred row (private
    /// items only). Replaced by the sealed concrete [`TyKind::Row`]
    /// during signature elaboration's cycle-aware fixpoint
    /// ([`crate::rows`]); it never survives into body checking.
    InferredRow {
        module: u32,
        name: String,
    },
    /// The `..` open-row marker (s17, D30): appears only as a
    /// [`TyKind::Row`] tail. `! {A, B, ..}` declares *at least* these
    /// tags — raises of new tags are admitted, any row widens into it,
    /// and a `match` over the row value must bind a rest (`_` or a
    /// name). The Roc growing-tag-union bar: adding a tag upstream
    /// never breaks a rest-arm consumer.
    OpenTail,
    /// A range expression's builtin type (closed family — `for` iterates
    /// it without any trait machinery; D25).
    Range(TyId),
    /// A nominal struct/enum: (module index, item name). Field/variant
    /// structure lives in the signature tables, not here.
    Nominal {
        module: u32,
        name: String,
        /// Type arguments, in declaration order (s94): empty for a
        /// non-generic nominal, one `TyId` per parameter for an
        /// elaborated application (`Map[str, int]`). Const-generic
        /// arguments do not land here — they stay opaque
        /// ([`TyKind::Unsupported`]) until value arguments exist in
        /// the table.
        args: Vec<TyId>,
    },
    /// A rigid (universal) type variable — a generic item's parameter
    /// inside its own body. Unifies only with itself; the s14 trait
    /// engine instantiates these.
    Rigid(String),
    /// An existential inference variable, per body ([`crate::unify`]).
    Var(u32),
    /// `*T` (unsafe tier). Elaborates structurally; operations are not
    /// yet checkable.
    Ptr(TyId),
    /// `shared T` / `handle T` / `weak T` / `distinct T` (memory tiers,
    /// c04). Structural placeholders: equal only to themselves.
    Shared(TyId),
    Handle(TyId),
    Weak(TyId),
    Distinct(TyId),
    /// The two prelude containers the memory-tier corpus rests on
    /// (s21): `List[T]` (growable sequence, `bounds`-trapping index)
    /// and `Pool[T]` (generational slab behind `handle T`, X5). Typed
    /// as builtins exactly like `Shared`/`Handle` — *not* the generic
    /// std surface (s16 generic data / s37 std own that); every other
    /// generic instantiation still lands in [`TyKind::Unsupported`].
    List(TyId),
    Pool(TyId),
    /// The concurrency surface's builtin types (spec 03, D14). Typed
    /// as builtins exactly like the containers above — the real std
    /// sync surface subsumes these later. `Chan` is `channel[T](n)`'s
    /// value ([conc.chan.type]); `Mutex` is the `sync` wrapper the
    /// `when` construct acquires ([conc.when.body]); `TaskScope` is
    /// the handle a `scope name { … }` block binds (D16: scope
    /// handles are ordinary values, passable as parameters).
    Chan(TyId),
    Mutex(TyId),
    TaskScope,
    /// A proc handle — `spawn proc f(args)`'s value ([conc.proc.1]:
    /// an unforgeable generational id; no memory handle crosses the
    /// proc API). Methods: `monitor`/`kill`/`cancel`/`link` (s73).
    Proc,
    /// A proc exit reason ([conc.proc.exit]'s closed set as a value,
    /// D30) — what a monitor channel delivers; `is_normal`/
    /// `is_error`/`is_killed`/`is_cancelled` observe the class.
    ExitReason,
    /// `dyn Trait` with the trait resolved to its defining module
    /// (s14). Dyn-safety is checked where the type is written; witness
    /// layout is c05's.
    Dyn {
        module: u32,
        name: String,
    },
    /// An associated-type projection `Base.Name` (s14). Bases are
    /// rigid inside generic bodies; concrete bases normalize away via
    /// the impl's rewrite rules ([`crate::rewrite`]). Equality is
    /// textual-after-normalization — no equality solver (D28).
    Proj(TyId, String),
    /// The `region` type (X4).
    RegionTy,
    /// The `type` type (comptime, D29).
    TypeTy,
    /// A comptime-only reflection value's type (s16, D29): what
    /// `typeinfo(T)` and its projections type as. These kinds exist
    /// only during semantic analysis — reflection values never reach
    /// runtime code, and no runtime type ever unifies with one.
    Meta(MetaTy),
    /// A type form s13 elaborates only as an opaque token (generic
    /// instantiations like `List[int]`). Carries its rendered source
    /// form; equal only to the identical rendering. Any *operation* on
    /// a value of this type is NotYetCheckable — never guessed.
    Unsupported(String),
}

/// The hash-consing arena. Cloning is cheap enough for per-body use
/// (tables are small); ids from a clone remain valid in the clone.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypeTable {
    kinds: Vec<TyKind>,
    map: HashMap<TyKind, TyId>,
}

impl TypeTable {
    pub fn new() -> TypeTable {
        TypeTable::default()
    }

    /// Intern `kind`, returning the existing id when the structure was
    /// seen before (structural equality ⇒ id equality).
    pub fn intern(&mut self, kind: TyKind) -> TyId {
        if let Some(&id) = self.map.get(&kind) {
            return id;
        }
        let id = TyId(u32::try_from(self.kinds.len()).expect("type table overflow"));
        self.kinds.push(kind.clone());
        self.map.insert(kind, id);
        id
    }

    pub fn kind(&self, id: TyId) -> &TyKind {
        &self.kinds[id.index()]
    }

    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    // Convenience constructors for the ubiquitous types.
    pub fn error(&mut self) -> TyId {
        self.intern(TyKind::Error)
    }

    pub fn never(&mut self) -> TyId {
        self.intern(TyKind::Never)
    }

    pub fn unit(&mut self) -> TyId {
        self.intern(TyKind::Unit)
    }

    pub fn prim(&mut self, p: Prim) -> TyId {
        self.intern(TyKind::Prim(p))
    }

    /// The empty (closed, tagless) row — the row of a fallible value
    /// that cannot actually fail.
    pub fn empty_row(&mut self) -> TyId {
        self.intern(TyKind::Row {
            tags: Vec::new(),
            tail: None,
        })
    }

    /// Intern a row in canonical form: tags sorted by name. Callers
    /// reject duplicates before this point (E0601); a duplicate that
    /// slips through keeps its first payload.
    pub fn row(&mut self, mut tags: Vec<(String, Vec<TyId>)>, tail: Option<TyId>) -> TyId {
        tags.sort_by(|a, b| a.0.cmp(&b.0));
        tags.dedup_by(|a, b| a.0 == b.0);
        self.intern(TyKind::Row { tags, tail })
    }

    /// Intern `ok ! row`.
    pub fn err_union(&mut self, ok: TyId, row: TyId) -> TyId {
        self.intern(TyKind::ErrUnion(ok, row))
    }
}

/// Render one row tag as source would spell it: `Tag` or
/// `Tag(payload, …)`.
pub fn render_tag(
    table: &TypeTable,
    name: &str,
    payload: &[TyId],
    resolve: &dyn Fn(u32) -> Result<TyId, &'static str>,
) -> String {
    if payload.is_empty() {
        name.to_string()
    } else {
        let parts: Vec<String> = payload.iter().map(|t| render(table, *t, resolve)).collect();
        format!("{name}({})", parts.join(", "))
    }
}

/// Render a row: `{A, B(int) | e}`. The empty closed row renders `{}`.
pub fn render_row(
    table: &TypeTable,
    row: TyId,
    resolve: &dyn Fn(u32) -> Result<TyId, &'static str>,
) -> String {
    match table.kind(row) {
        TyKind::Row { tags, tail } => {
            let mut parts: Vec<String> = tags
                .iter()
                .map(|(n, p)| render_tag(table, n, p, resolve))
                .collect();
            if tail.is_some_and(|t| matches!(table.kind(t), TyKind::OpenTail)) {
                parts.sort();
                if parts.is_empty() {
                    return "{..}".to_string();
                }
                return format!("{{{}, ..}}", parts.join(", "));
            }
            let tail_str = tail.map(|t| render(table, t, resolve));
            match tail_str {
                Some(t) if parts.is_empty() => format!("{{{t}}}"),
                Some(t) => format!("{{{} | {t}}}", parts.join(", ")),
                None => {
                    if parts.is_empty() {
                        "{}".to_string()
                    } else {
                        parts.sort();
                        format!("{{{}}}", parts.join(", "))
                    }
                }
            }
        }
        TyKind::InferredRow { .. } => "{..inferred}".to_string(),
        _ => format!("{{{}}}", render(table, row, resolve)),
    }
}

/// The structural row delta (D22/E0602): tags of `a` absent from `b`,
/// tags of `b` absent from `a`, and shared tags whose payloads differ —
/// each rendered as source spells it. Never renders whole rows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RowDelta {
    pub only_in_a: Vec<String>,
    pub only_in_b: Vec<String>,
    pub payload_conflicts: Vec<String>,
}

impl RowDelta {
    pub fn is_empty(&self) -> bool {
        self.only_in_a.is_empty() && self.only_in_b.is_empty() && self.payload_conflicts.is_empty()
    }
}

/// Compute the [`RowDelta`] between two concrete rows. `None` when
/// either side is not a concrete `Row`.
pub fn row_delta(
    table: &TypeTable,
    a: TyId,
    b: TyId,
    resolve: &dyn Fn(u32) -> Result<TyId, &'static str>,
) -> Option<RowDelta> {
    let (TyKind::Row { tags: ta, .. }, TyKind::Row { tags: tb, .. }) =
        (table.kind(a).clone(), table.kind(b).clone())
    else {
        return None;
    };
    let mut delta = RowDelta::default();
    for (n, p) in &ta {
        match tb.iter().find(|(m, _)| m == n) {
            None => delta.only_in_a.push(render_tag(table, n, p, resolve)),
            Some((_, q)) if p != q => {
                delta
                    .payload_conflicts
                    .push(render_tag(table, n, p, resolve));
            }
            Some(_) => {}
        }
    }
    for (n, p) in &tb {
        if !ta.iter().any(|(m, _)| m == n) {
            delta.only_in_b.push(render_tag(table, n, p, resolve));
        }
    }
    Some(delta)
}

/// Render `id` for humans. Unresolved inference variables render by
/// their literal kind (`{integer}`, `{float}`, `{number}`) or `_`;
/// `resolve` maps a var index to its solution (or `None`) and its
/// placeholder rendering.
pub fn render(
    table: &TypeTable,
    id: TyId,
    resolve: &dyn Fn(u32) -> Result<TyId, &'static str>,
) -> String {
    match table.kind(id) {
        TyKind::Error => "<error>".to_string(),
        TyKind::Never => "!".to_string(),
        TyKind::Unit => "()".to_string(),
        TyKind::Prim(p) => p.name().to_string(),
        TyKind::Wrapping(t) => format!("wrapping[{}]", render(table, *t, resolve)),
        TyKind::Tuple(ts) => {
            let parts: Vec<String> = ts.iter().map(|t| render(table, *t, resolve)).collect();
            format!("({})", parts.join(", "))
        }
        TyKind::Fn(params, ret) => {
            let parts: Vec<String> = params.iter().map(|t| render(table, *t, resolve)).collect();
            format!(
                "fn({}) -> {}",
                parts.join(", "),
                render(table, *ret, resolve)
            )
        }
        TyKind::ErrUnion(t, row) => {
            let ok = render(table, *t, resolve);
            match table.kind(*row) {
                // The bare/inferred spellings render as written.
                TyKind::Row { tags, tail: None } if tags.is_empty() => format!("!{ok}"),
                TyKind::InferredRow { .. } => format!("!{ok}"),
                _ => format!("{ok} ! {}", render_row(table, *row, resolve)),
            }
        }
        TyKind::Row { .. } => render_row(table, id, resolve),
        TyKind::InferredRow { .. } => "{..inferred}".to_string(),
        TyKind::OpenTail => "..".to_string(),
        TyKind::Range(t) => format!("range[{}]", render(table, *t, resolve)),
        TyKind::Nominal { name, args, .. } => {
            if args.is_empty() {
                name.clone()
            } else {
                let parts: Vec<String> = args.iter().map(|t| render(table, *t, resolve)).collect();
                format!("{name}[{}]", parts.join(", "))
            }
        }
        TyKind::Rigid(name) => name.clone(),
        TyKind::Var(v) => match resolve(*v) {
            Ok(t) => render(table, t, resolve),
            Err(placeholder) => placeholder.to_string(),
        },
        TyKind::Ptr(t) => format!("*{}", render(table, *t, resolve)),
        TyKind::Shared(t) => format!("shared {}", render(table, *t, resolve)),
        TyKind::Handle(t) => format!("handle {}", render(table, *t, resolve)),
        TyKind::Weak(t) => format!("weak {}", render(table, *t, resolve)),
        TyKind::Distinct(t) => format!("distinct {}", render(table, *t, resolve)),
        TyKind::List(t) => format!("List[{}]", render(table, *t, resolve)),
        TyKind::Pool(t) => format!("Pool[{}]", render(table, *t, resolve)),
        TyKind::Chan(t) => format!("channel[{}]", render(table, *t, resolve)),
        TyKind::Mutex(t) => format!("Mutex[{}]", render(table, *t, resolve)),
        TyKind::TaskScope => "scope".to_string(),
        TyKind::Proc => "proc".to_string(),
        TyKind::ExitReason => "ExitReason".to_string(),
        TyKind::Dyn { name, .. } => format!("dyn {name}"),
        TyKind::Proj(base, name) => format!("{}.{name}", render(table, *base, resolve)),
        TyKind::RegionTy => "region".to_string(),
        TyKind::TypeTy => "type".to_string(),
        TyKind::Meta(m) => m.name().to_string(),
        TyKind::Unsupported(s) => s.clone(),
    }
}

/// The structural type diff (D22): where two types share a constructor
/// but differ inside, name the differing parts instead of making the
/// reader eyeball two long renderings. Returns human path descriptions,
/// innermost mismatches only.
pub fn diff(
    table: &TypeTable,
    a: TyId,
    b: TyId,
    resolve: &dyn Fn(u32) -> Result<TyId, &'static str>,
    out: &mut Vec<String>,
    path: &str,
) {
    let deref = |t: TyId| -> TyId {
        if let TyKind::Var(v) = table.kind(t)
            && let Ok(sol) = resolve(*v)
        {
            return sol;
        }
        t
    };
    let (a, b) = (deref(a), deref(b));
    if a == b {
        return;
    }
    let leaf = |out: &mut Vec<String>| {
        out.push(format!(
            "{path}: `{}` vs `{}`",
            render(table, a, resolve),
            render(table, b, resolve)
        ));
    };
    match (table.kind(a), table.kind(b)) {
        (TyKind::Fn(pa, ra), TyKind::Fn(pb, rb)) => {
            if pa.len() != pb.len() {
                out.push(format!(
                    "{path}: one takes {} parameter{}, the other {}",
                    pa.len(),
                    if pa.len() == 1 { "" } else { "s" },
                    pb.len()
                ));
                return;
            }
            for (i, (x, y)) in pa.iter().zip(pb.iter()).enumerate() {
                diff(table, *x, *y, resolve, out, &format!("parameter {}", i + 1));
            }
            diff(table, *ra, *rb, resolve, out, "the return type");
        }
        (TyKind::Tuple(ta), TyKind::Tuple(tb)) => {
            if ta.len() != tb.len() {
                leaf(out);
                return;
            }
            for (i, (x, y)) in ta.iter().zip(tb.iter()).enumerate() {
                diff(table, *x, *y, resolve, out, &format!("element {}", i + 1));
            }
        }
        (TyKind::Wrapping(x), TyKind::Wrapping(y)) => {
            diff(table, *x, *y, resolve, out, "the wrapped type");
        }
        (TyKind::ErrUnion(x, rx), TyKind::ErrUnion(y, ry)) => {
            diff(table, *x, *y, resolve, out, "the success type");
            if rx != ry {
                // Row mismatches render tags-missing/tags-extra only —
                // never a dump of both full rows (D22, the Koka
                // degradation s15 exists to beat).
                match row_delta(table, *rx, *ry, resolve) {
                    Some(d) if !d.is_empty() => {
                        if !d.only_in_a.is_empty() {
                            out.push(format!(
                                "the error row: `{}` only on the first side",
                                d.only_in_a.join("`, `")
                            ));
                        }
                        if !d.only_in_b.is_empty() {
                            out.push(format!(
                                "the error row: `{}` only on the second side",
                                d.only_in_b.join("`, `")
                            ));
                        }
                        for c in &d.payload_conflicts {
                            out.push(format!(
                                "the error row: the shared tag `{c}` carries different payloads"
                            ));
                        }
                    }
                    Some(_) => {}
                    None => out.push(format!(
                        "the error row: `{}` vs `{}`",
                        render_row(table, *rx, resolve),
                        render_row(table, *ry, resolve)
                    )),
                }
            }
        }
        (TyKind::Range(x), TyKind::Range(y)) => {
            diff(table, *x, *y, resolve, out, "the element type");
        }
        _ => leaf(out),
    }
}

/// Substitute rigid type variables by name (s14 instantiation: an
/// archetype's rigids become the call's inference variables; a trait
/// member's `Self` becomes the impl's self type). Structural and
/// total — unknown rigids pass through unchanged.
pub fn subst(
    table: &mut TypeTable,
    ty: TyId,
    map: &std::collections::BTreeMap<String, TyId>,
) -> TyId {
    match table.kind(ty).clone() {
        TyKind::Rigid(name) => map.get(&name).copied().unwrap_or(ty),
        TyKind::Nominal { module, name, args } if !args.is_empty() => {
            let sargs: Vec<TyId> = args.into_iter().map(|a| subst(table, a, map)).collect();
            table.intern(TyKind::Nominal {
                module,
                name,
                args: sargs,
            })
        }
        TyKind::Wrapping(t) => {
            let s = subst(table, t, map);
            table.intern(TyKind::Wrapping(s))
        }
        TyKind::ErrUnion(t, row) => {
            let s = subst(table, t, map);
            let r = subst(table, row, map);
            table.intern(TyKind::ErrUnion(s, r))
        }
        TyKind::Row { tags, tail } => {
            let stags: Vec<(String, Vec<TyId>)> = tags
                .into_iter()
                .map(|(n, p)| (n, p.into_iter().map(|t| subst(table, t, map)).collect()))
                .collect();
            let stail = tail.map(|t| subst(table, t, map));
            // A tail substituted to a concrete row merges into the
            // host row (row-variable instantiation is union).
            match stail.map(|t| table.kind(t).clone()) {
                Some(TyKind::Row {
                    tags: tt,
                    tail: inner,
                }) => {
                    let mut merged = stags;
                    merged.extend(tt);
                    table.row(merged, inner)
                }
                _ => table.row(stags, stail),
            }
        }
        TyKind::Range(t) => {
            let s = subst(table, t, map);
            table.intern(TyKind::Range(s))
        }
        TyKind::Ptr(t) => {
            let s = subst(table, t, map);
            table.intern(TyKind::Ptr(s))
        }
        TyKind::Shared(t) => {
            let s = subst(table, t, map);
            table.intern(TyKind::Shared(s))
        }
        TyKind::Handle(t) => {
            let s = subst(table, t, map);
            table.intern(TyKind::Handle(s))
        }
        TyKind::Weak(t) => {
            let s = subst(table, t, map);
            table.intern(TyKind::Weak(s))
        }
        TyKind::Distinct(t) => {
            let s = subst(table, t, map);
            table.intern(TyKind::Distinct(s))
        }
        TyKind::List(t) => {
            let s = subst(table, t, map);
            table.intern(TyKind::List(s))
        }
        TyKind::Pool(t) => {
            let s = subst(table, t, map);
            table.intern(TyKind::Pool(s))
        }
        TyKind::Chan(t) => {
            let s = subst(table, t, map);
            table.intern(TyKind::Chan(s))
        }
        TyKind::Mutex(t) => {
            let s = subst(table, t, map);
            table.intern(TyKind::Mutex(s))
        }
        TyKind::Proj(base, name) => {
            let s = subst(table, base, map);
            table.intern(TyKind::Proj(s, name))
        }
        TyKind::Tuple(ts) => {
            let s: Vec<TyId> = ts.into_iter().map(|t| subst(table, t, map)).collect();
            table.intern(TyKind::Tuple(s))
        }
        TyKind::Fn(params, ret) => {
            let ps: Vec<TyId> = params.into_iter().map(|t| subst(table, t, map)).collect();
            let r = subst(table, ret, map);
            table.intern(TyKind::Fn(ps, r))
        }
        _ => ty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_vars(_: u32) -> Result<TyId, &'static str> {
        Err("_")
    }

    #[test]
    fn interning_respects_structural_equality() {
        let mut t = TypeTable::new();
        let int = t.prim(Prim::Int);
        let a = t.intern(TyKind::Tuple(vec![int, int]));
        let b = t.intern(TyKind::Tuple(vec![int, int]));
        assert_eq!(a, b, "same structure, same id");
        let f1 = t.intern(TyKind::Fn(vec![int], int));
        let f2 = t.intern(TyKind::Fn(vec![int], int));
        assert_eq!(f1, f2);
        let str_ = t.prim(Prim::Str);
        let c = t.intern(TyKind::Tuple(vec![int, str_]));
        assert_ne!(a, c, "different structure, different id");
    }

    #[test]
    fn rendering_reads_like_source() {
        let mut t = TypeTable::new();
        let int = t.prim(Prim::Int);
        let f64_ = t.prim(Prim::F64);
        let f = t.intern(TyKind::Fn(vec![int, f64_], int));
        assert_eq!(render(&t, f, &no_vars), "fn(int, f64) -> int");
        let row = t.empty_row();
        let e = t.intern(TyKind::ErrUnion(int, row));
        assert_eq!(render(&t, e, &no_vars), "!int");
        let str_ = t.prim(Prim::Str);
        let tagged = t.row(
            vec![
                ("TooShort".to_string(), vec![]),
                ("BadDigit".to_string(), vec![str_]),
            ],
            None,
        );
        let e2 = t.err_union(int, tagged);
        assert_eq!(render(&t, e2, &no_vars), "int ! {BadDigit(str), TooShort}");
        let w = t.intern(TyKind::Wrapping(int));
        assert_eq!(render(&t, w, &no_vars), "wrapping[int]");
    }

    #[test]
    fn diff_names_the_differing_part_only() {
        let mut t = TypeTable::new();
        let int = t.prim(Prim::Int);
        let str_ = t.prim(Prim::Str);
        let bool_ = t.prim(Prim::Bool);
        let a = t.intern(TyKind::Fn(vec![int, str_], bool_));
        let b = t.intern(TyKind::Fn(vec![int, int], bool_));
        let mut out = Vec::new();
        diff(&t, a, b, &no_vars, &mut out, "the function type");
        assert_eq!(out, ["parameter 2: `str` vs `int`"]);
    }
}
