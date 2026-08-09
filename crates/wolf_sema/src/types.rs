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
        }
    }

    /// Is this one of the integer types (the `{integer}` literal kind)?
    pub fn is_integer(self) -> bool {
        matches!(
            self,
            Prim::Byte
                | Prim::Int
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
    /// `!T` — the error channel. The row side is OPAQUE at s13: no
    /// tags, no composition; the row engine is s15 (D30).
    ErrUnion(TyId),
    /// A range expression's builtin type (closed family — `for` iterates
    /// it without any trait machinery; D25).
    Range(TyId),
    /// A nominal struct/enum: (module index, item name). Field/variant
    /// structure lives in the signature tables, not here.
    Nominal {
        module: u32,
        name: String,
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
    /// `dyn Trait` by rendered path (s14 owns the semantics).
    Dyn(String),
    /// The `region` type (X4).
    RegionTy,
    /// The `type` type (comptime, D29).
    TypeTy,
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
        TyKind::ErrUnion(t) => format!("!{}", render(table, *t, resolve)),
        TyKind::Range(t) => format!("range[{}]", render(table, *t, resolve)),
        TyKind::Nominal { name, .. } => name.clone(),
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
        TyKind::Dyn(p) => format!("dyn {p}"),
        TyKind::RegionTy => "region".to_string(),
        TyKind::TypeTy => "type".to_string(),
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
        (TyKind::ErrUnion(x), TyKind::ErrUnion(y)) => {
            diff(table, *x, *y, resolve, out, "the success type");
        }
        (TyKind::Range(x), TyKind::Range(y)) => {
            diff(table, *x, *y, resolve, out, "the element type");
        }
        _ => leaf(out),
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
        let e = t.intern(TyKind::ErrUnion(int));
        assert_eq!(render(&t, e, &no_vars), "!int");
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
