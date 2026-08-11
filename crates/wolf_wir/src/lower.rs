//! Typed HIR → WIR lowering (s25–s27): sema's typed AST in, verified
//! SSA out, in one forward walk through [`crate::build::FuncBuilder`].
//!
//! s25 laid the scalar/control base (Braun SSA, checked arithmetic,
//! `if`/`while`/`loop`); s26 added the memory story (regions,
//! aggregates by value, pointer-shaped `mut`, facts). s27 finishes the
//! CONTROL story — the organizing invariant is D30's: **error
//! propagation is ordinary control flow.** No landing pads, no unwind
//! tables, no implicit edges:
//!
//! - **Error unions**: `!T` with a non-empty sealed row lowers to the
//!   `eu{…}` pair type; tags are module-interned `i64`s (0 = ok), so
//!   row widening is the identity on tag values and `?` is `eu.is_err`
//!   plus `br` — the err edge forms the propagated value FIRST, runs
//!   the errdefer chain (`[mem.model.order]`), then `ret`s. Empty-row
//!   unions still lower as their ok type (the error case is
//!   uninhabited).
//! - **`defer`/`errdefer`**: build-time only. Each scope keeps its
//!   entries (typed AST fragments + a visibility fence); every exit
//!   edge — fall-through, `return`, `?`-err, `break`/`continue`
//!   crossing the scope — re-lowers the applicable chain in LIFO order
//!   at the exit site. `region name { }`'s wholesale free is an
//!   ordinary (outermost) entry of its scope (X4), so early exits free
//!   on every edge and the verifier's per-path token linearity proves
//!   it independently.
//! - **`match`**: single-scrutinee decision trees — one discriminant
//!   read, shared tag tests in arm order, guards as ordinary branches
//!   re-entering at the next candidate, bindings as ordinary locals.
//!   Exhaustiveness is sema's theorem (s17): exhaustive matches lower
//!   with NO default edge — the final arm is entered unconditionally.
//! - **`for`**: ranges lower structurally (the closed builtin family,
//!   `[mem.iter.range]`); other iterables await the `Iter[T]` drive
//!   loop's std surface (`[mem.iter.for]`, c06/std).
//! - **Methods**: inherent methods lower as ordinary functions named
//!   `Type.method`; receiver modes reuse the s26 machinery (`read` by
//!   value, `mut` pointer-shaped with flat-aggregate spills, `take` by
//!   value).
//!
//! Everything still missing refuses with an honest [`NotYet`] naming
//! its owner — strings/data segments, `List`/`Pool` runtime shapes,
//! trait dispatch tables, closures and concurrency are c06/c05+. A
//! body is either fully lowered or refused, never half-guessed (the
//! conservatism-ledger contract).

use std::collections::HashMap;

use wolf_ast::{
    Arg, AssignStmt, Block as AstBlock, BreakExpr, CallExpr, CastExpr, ConstDecl, DeferStmt,
    ElseExpr, ExprStmt, ForExpr, GreenNode, IfExpr, LetDecl, LoopExpr, MatchArm, MatchExpr,
    ParamMode, ParenExpr, PrefixExpr, RangeExpr, ReturnExpr, StringExpr, SyntaxKind, TryExpr,
    VarDecl, WhileExpr,
};
use wolf_sema::check::{CallSig, CastKind, Dispatch};
use wolf_sema::sig::{FnSig, ItemSig, SigTables};
use wolf_sema::types::{Prim, TyId, TyKind, TypeTable};
use wolf_sema::{BodyResult, NotYet, Package, Typecheck, TypedBody};
use wolf_span::Span;

use crate::build::{FuncBuilder, InsOut, Stats, Var};
use crate::entity::EntityRef;
use crate::facts::{DerefSize, FactData, FactKind, Just, Theorem};
use crate::ir::{Aux, Block, ExtFunc, Mode, Module, Param, SigId};
use crate::ops::{FloatCc, IntCc, Opcode, TrapKind};
use crate::types::{self, RegionId, TypeId};

type R<T> = Result<T, NotYet>;

/// The result of lowering one package.
pub struct Build {
    /// The WIR module: one function per lowered body, in body order.
    pub module: Module,
    /// Honest refusals, in body order. Empty ⇔ every checked body
    /// lowered (the `wir` rung's completion condition).
    pub not_yet: Vec<NotYet>,
    /// Summed peephole counters (`--zstats`, the `wir-build` bench).
    pub stats: Stats,
}

/// Lower every checked body of a package. `tc` must come from
/// [`wolf_sema::typecheck_package`] over the same `pkg`; callers gate
/// on `tc.not_yet`/`mem` cleanliness for the rung verdict — this
/// function lowers whatever is lowerable and refuses the rest.
pub fn lower_package(pkg: &Package, tc: &Typecheck) -> Build {
    let mut module = Module::new();
    let mut not_yet = Vec::new();
    let mut stats = Stats::default();
    // Callee resolution: fn name → its unique (module, sig); names
    // declared in more than one module refuse at the call site.
    let mut fns: HashMap<&str, Vec<(usize, &FnSig)>> = HashMap::new();
    for (m, items) in tc.sigs.modules.iter().enumerate() {
        for (name, sig) in items {
            if let ItemSig::Fn(f) = sig {
                fns.entry(name.as_str()).or_default().push((m, f));
            }
        }
    }
    let mut sig_cache: HashMap<String, SigId> = HashMap::new();
    for outcome in &tc.bodies {
        let BodyResult::Checked(tb) = &outcome.result else {
            continue;
        };
        let body = &outcome.body;
        match lower_body(pkg, &tc.sigs, tb, body, &fns, &mut module, &mut sig_cache) {
            Ok(Some(s)) => stats.add(s),
            Ok(None) => {}
            Err(nyc) => not_yet.push(nyc),
        }
    }
    Build {
        module,
        not_yet,
        stats,
    }
}

/// Lower one checked body. `Ok(None)`: nothing to lower (bodyless
/// item, broken tree — not a refusal).
#[allow(clippy::too_many_arguments)]
fn lower_body(
    pkg: &Package,
    sigs: &SigTables,
    tb: &TypedBody,
    body: &wolf_sema::BodyRef,
    fns: &HashMap<&str, Vec<(usize, &FnSig)>>,
    module: &mut Module,
    sig_cache: &mut HashMap<String, SigId>,
) -> R<Option<Stats>> {
    let root = &pkg.files[body.file].parse.root;
    let Some(node) = root.nodes().filter(|n| n.kind.is_item()).nth(body.decl) else {
        return Ok(None);
    };
    let span = node.span;
    // Resolve the fn node, signature, and WIR name: plain items
    // directly; impl members (s27 methods) through the impl tables —
    // an inherent method lowers as an ordinary function named
    // `Type.method` (matching sema's diagnostic label, which is also
    // the call sites' callee string).
    let (fn_node, fsig, wir_name): (&GreenNode, &FnSig, String) = match body.member {
        None => {
            match node.kind {
                SyntaxKind::FnDecl => {}
                SyntaxKind::LetDecl | SyntaxKind::VarDecl | SyntaxKind::ConstDecl => {
                    return Err(refuse("item-initializer lowering (globals, c06)", span));
                }
                _ => return Ok(None),
            }
            let Some(ItemSig::Fn(fsig)) = sigs.get(body.module, &body.name) else {
                return Ok(None);
            };
            // WIR names are module-path qualified (issue #26): two
            // modules may declare the same item name, and the mangled
            // symbol hash must fold the full path — `std.list.len`
            // and `std.str.len` are distinct functions.
            (node, fsig, qualify(sigs, body.module, &body.name))
        }
        Some(mi) => {
            if node.kind != SyntaxKind::ImplDecl {
                // Trait default bodies check against the trait's own
                // archetype; they lower per impl once dispatch tables
                // land.
                return Err(refuse(
                    "trait default-body lowering (dispatch tables, c06)",
                    span,
                ));
            }
            let Some(imp) = sigs
                .impls
                .iter()
                .find(|i| i.file == body.file && i.decl == body.decl)
            else {
                return Ok(None);
            };
            if imp.trait_ref.is_some() {
                return Err(refuse(
                    "trait-impl method lowering (dispatch tables, c06)",
                    span,
                ));
            }
            if !imp.generics.is_empty() {
                return Err(refuse("generic-impl lowering (monomorphization)", span));
            }
            let Some(m) = imp.methods.iter().find(|m| m.member == mi) else {
                return Ok(None);
            };
            let TyKind::Nominal { name: tyname, .. } = sigs.table.kind(imp.self_ty) else {
                return Err(refuse("methods on non-nominal self types", span));
            };
            let Some(mnode) = node.nodes().filter(|n| n.kind.is_item()).nth(mi) else {
                return Ok(None);
            };
            if mnode.kind != SyntaxKind::FnDecl {
                return Ok(None); // associated consts have no body to lower
            }
            (mnode, &m.sig, format!("{tyname}.{}", m.name))
        }
    };
    let span = fn_node.span;
    let d = wolf_ast::FnDecl::cast(fn_node).expect("kind");
    let Some(block) = d.body() else {
        return Ok(None);
    };
    if !fsig.generics.is_empty() {
        return Err(refuse("generic-function lowering (monomorphization)", span));
    }
    if fsig.comptime {
        return Err(refuse(
            "comptime-function lowering (D29 CTFE owns these)",
            span,
        ));
    }
    // The WIR signature (modes carried; s26 attaches the fact slots).
    let sig = wir_fn_sig(module, sig_cache, sigs, &wir_name, fsig, span)?;
    let mut b = FuncBuilder::new(module, wir_name, sig);
    // s30: spans thread from the typed HIR into WIR (the lossless s07
    // chain) — the file once per function, then a per-statement span
    // cursor the builder stamps on every appended instruction.
    b.func.src_file = Some(span.file.index() as u32);
    b.set_span(fsig.name_span.lo, fsig.name_span.hi);
    let mut lowerer = Lowerer {
        src: &pkg.files[body.file].raw.src,
        table: &tb.table,
        sig_table: &sigs.table,
        sigs,
        calls: tb.calls.iter().map(|(s, c)| (*s, c)).collect(),
        expr_tys: tb.exprs.iter().map(|(s, t)| (*s, *t)).collect(),
        local_tys: tb.locals.iter().map(|(_, s, t)| (*s, *t)).collect(),
        casts: tb
            .casts
            .iter()
            .map(|(s, f, t, k)| (*s, (*f, *t, *k)))
            .collect(),
        fns,
        dispatch: tb.dispatch.iter().map(|(s, d)| (*s, d)).collect(),
        matches: tb.matches.iter().copied().collect(),
        scopes: Vec::new(),
        visible: None,
        loops: Vec::new(),
        fn_eu: None,
        fn_tail: None,
        callees: HashMap::new(),
        straight_line: false,
        b: &mut b,
    };
    lowerer.lower_fn(fsig, block)?;
    let stats = b.stats;
    let func = b.finish();
    module.add_func(func);
    Ok(Some(stats))
}

fn refuse(construct: &'static str, span: Span) -> NotYet {
    NotYet { construct, span }
}

/// The WIR shape of `str` (s31): a `{ptr, i64}` fat pair — bytes
/// pointer + byte length, exactly the s30 type-DIE mapping.
fn str_ty(it: &mut types::TypeInterner) -> TypeId {
    it.intern(types::TypeData::Agg(vec![types::PTR, types::I64]))
}

/// One decoded segment of a string episode: literal bytes (escape set
/// per the reference interpreter's `eval_string` — `\n \t \r \\ \" \{
/// \} \0`, unknown escapes drop the backslash) or an interpolation
/// hole's expression with its format-spec node, if the hole carries
/// one (s38).
enum StrSeg<'t> {
    Lit(Vec<u8>),
    Hole {
        expr: &'t GreenNode,
        spec: Option<&'t GreenNode>,
    },
}

/// One classified print segment (s31 print path; s38 adds the packed
/// format spec — `0` means none — and the float segment). Spec-less
/// stdout segments keep the frozen `__wolf_rt_print_*` symbols;
/// everything else flows through the stream-parameterized
/// `__wolf_rt_write_*` family.
enum PrintSeg {
    /// Literal bytes → module data.
    Lit(Vec<u8>),
    /// A str value ({ptr, len}).
    Str { v: Value, spec: i64 },
    /// An integer value; widened to i64 per signedness.
    Int { v: Value, unsigned: bool, spec: i64 },
    /// A bool value.
    Bool { v: Value, spec: i64 },
    /// An `f64` value (s38: reference semantics exist — the checked
    /// executor renders the same bytes).
    F64 { v: Value, spec: i64 },
}

/// The module-path-qualified WIR name of item `name` in `module`
/// (issue #26): `geometry.area` for a child module, the bare name in
/// the package root (whose dotted path is empty — `main` stays
/// `main`). The mangled symbol hashes the WIR name, so the full module
/// path lands in every `_W…$hash` symbol.
fn qualify(sigs: &SigTables, module: usize, name: &str) -> String {
    match sigs.module_names.get(module).map(String::as_str) {
        Some("") | None => name.to_string(),
        Some(path) => format!("{path}.{name}"),
    }
}

/// Map one sema type to a WIR type. `Ok(None)` is the unit/never
/// "no value" case (zero-field aggregates included); unsupported types
/// refuse. `sigs` resolves nominal ADAPTER types (`type X = distinct
/// B` — layout identity, D28) to their base scalar and struct nominals
/// to by-value aggregates (s26; field types live in the signature
/// table). Unsigned prims ride the same-width scalar — signedness is
/// an op property, not a type property (the s26 op-set decision,
/// recorded in [`crate::ops`]).
fn wir_ty(
    it: &mut types::TypeInterner,
    table: &TypeTable,
    sigs: &SigTables,
    id: TyId,
    span: Span,
) -> R<Option<TypeId>> {
    wir_ty_depth(it, table, sigs, id, span, 0)
}

fn wir_ty_depth(
    it: &mut types::TypeInterner,
    table: &TypeTable,
    sigs: &SigTables,
    id: TyId,
    span: Span,
    depth: u32,
) -> R<Option<TypeId>> {
    if depth > 32 {
        return Err(refuse("deeply nested aggregate types", span));
    }
    match table.kind(id) {
        TyKind::Unit | TyKind::Never => Ok(None),
        TyKind::Prim(p) => match p {
            Prim::Bool => Ok(Some(types::BOOL)),
            Prim::I8 | Prim::U8 => Ok(Some(types::I8)),
            Prim::I16 | Prim::U16 => Ok(Some(types::I16)),
            Prim::I32 | Prim::U32 => Ok(Some(types::I32)),
            Prim::I64 | Prim::Int | Prim::U64 | Prim::Uint => Ok(Some(types::I64)),
            Prim::F32 => Ok(Some(types::F32)),
            Prim::F64 => Ok(Some(types::F64)),
            // `str` is a {ptr, len} pair (s31, the s30 DIE mapping made
            // operational): the bytes live in module data (literals) or
            // wherever a runtime str points; the value is the fat pair.
            Prim::Str => Ok(Some(str_ty(it))),
            Prim::Byte => Err(refuse("byte lowering (runtime byte views, c08)", span)),
        },
        TyKind::Wrapping(inner) => match wir_ty_depth(it, table, sigs, *inner, span, depth + 1)? {
            Some(t) if types_is_int(t) => Ok(Some(t)),
            _ => Err(refuse("wrapping over a non-integer type", span)),
        },
        TyKind::Distinct(inner) => wir_ty_depth(it, table, sigs, *inner, span, depth + 1),
        TyKind::ErrUnion(ok, row) => {
            if row_is_empty(table, *row) {
                wir_ty_depth(it, table, sigs, *ok, span, depth + 1)
            } else {
                // A fallible type with tags: the eu pair (s27). The ok
                // half maps as usual; the row's payloads unify into
                // positional slots.
                let okw = wir_ty_depth(it, table, sigs, *ok, span, depth + 1)?;
                let slots = row_slot_tys(it, table, sigs, *row, span, depth)?;
                Ok(Some(it.eu(okw, slots)))
            }
        }
        TyKind::Row { .. } => {
            // A caught error value (`else |err|` binding, match
            // scrutinee): the tag alone for payload-free rows, else
            // tag + slots as a by-value aggregate mirroring the enum
            // representation.
            let slots = row_slot_tys(it, table, sigs, id, span, depth)?;
            if slots.is_empty() {
                Ok(Some(types::I64))
            } else {
                let mut fields = vec![types::I64];
                fields.extend(slots);
                Ok(Some(it.intern(types::TypeData::Agg(fields))))
            }
        }
        TyKind::Nominal { module, name } => {
            // Adapter types are scalars in disguise (layout identity);
            // struct nominals are by-value aggregates (s26); enums are
            // tag scalars or tag+slots aggregates (s27). Field/variant
            // types live in the SIGNATURE table.
            match sigs.get(*module as usize, name) {
                Some(ItemSig::Distinct { base, .. }) => {
                    wir_ty_depth(it, &sigs.table, sigs, *base, span, depth + 1)
                }
                Some(ItemSig::Struct(ss)) if !ss.generic => {
                    let mut fields = Vec::with_capacity(ss.fields.len());
                    for f in &ss.fields {
                        match wir_ty_depth(it, &sigs.table, sigs, f.ty, span, depth + 1)? {
                            Some(t) => fields.push(t),
                            None => {
                                return Err(refuse("unit-typed struct fields", span));
                            }
                        }
                    }
                    if fields.is_empty() {
                        return Ok(None); // a zero-field struct is unit-shaped
                    }
                    Ok(Some(it.intern(types::TypeData::Agg(fields))))
                }
                Some(ItemSig::Enum {
                    generic: false,
                    variants,
                    ..
                }) => {
                    // Enum values: the variant tag (declaration index,
                    // i64) alone when payload-free, else tag + the
                    // position-unified payload slots.
                    let mut slots: Vec<TypeId> = Vec::new();
                    for v in variants {
                        for (i, &p) in v.payload.iter().enumerate() {
                            let Some(w) = wir_ty_depth(it, &sigs.table, sigs, p, span, depth + 1)?
                            else {
                                return Err(refuse("unit-typed enum payloads", span));
                            };
                            match slots.get(i) {
                                None => slots.push(w),
                                Some(&s) if s == w => {}
                                Some(_) => {
                                    return Err(refuse(
                                        "enum payload slots with conflicting types across \
                                         variants (spilled union layout, c06)",
                                        span,
                                    ));
                                }
                            }
                        }
                    }
                    if slots.is_empty() {
                        Ok(Some(types::I64))
                    } else {
                        let mut fields = vec![types::I64];
                        fields.extend(slots);
                        Ok(Some(it.intern(types::TypeData::Agg(fields))))
                    }
                }
                _ => Err(refuse("generic-nominal lowering (monomorphization)", span)),
            }
        }
        TyKind::Tuple(elems) => {
            let mut fields = Vec::with_capacity(elems.len());
            for &e in &elems.clone() {
                match wir_ty_depth(it, table, sigs, e, span, depth + 1)? {
                    Some(t) => fields.push(t),
                    None => return Err(refuse("unit-typed tuple elements", span)),
                }
            }
            if fields.is_empty() {
                return Ok(None);
            }
            Ok(Some(it.intern(types::TypeData::Agg(fields))))
        }
        TyKind::RegionTy => Err(refuse(
            "first-class region values beyond local bindings (c05)",
            span,
        )),
        TyKind::Range(_) => Err(refuse(
            "range VALUES outside `for` headers (owned `Iter[int]` ranges, c06/std)",
            span,
        )),
        TyKind::Shared(_)
        | TyKind::Weak(_)
        | TyKind::Handle(_)
        | TyKind::List(_)
        | TyKind::Pool(_) => Err(refuse(
            "shared-tier surface lowering (rc receivers + runtime cells, c06)",
            span,
        )),
        // Raw pointers are opaque `ptr` VALUES (s29 — the C membrane
        // hands them out and takes them back). The raw-tier OPS over
        // them (deref, index, arithmetic, casts) keep their s26
        // refusals at the expression sites.
        TyKind::Ptr(_) => Ok(Some(types::PTR)),
        _ => Err(refuse("this type in WIR lowering", span)),
    }
}

/// The position-unified error-payload slot types of a row (s27): slot
/// `i` is the type every tag's `i`-th payload agrees on; disagreement
/// refuses (spilled union layout is c06's). Open tails are admitted —
/// unknown tags carry no payloads we could name; generic tails refuse.
fn row_slot_tys(
    it: &mut types::TypeInterner,
    table: &TypeTable,
    sigs: &SigTables,
    row: TyId,
    span: Span,
    depth: u32,
) -> R<Vec<TypeId>> {
    let TyKind::Row { tags, tail } = table.kind(row) else {
        return Err(refuse("a non-row error channel (generic rows)", span));
    };
    if let Some(t) = tail
        && !matches!(table.kind(*t), TyKind::OpenTail)
    {
        return Err(refuse("generic row tails (monomorphization)", span));
    }
    let tags = tags.clone();
    let mut slots: Vec<TypeId> = Vec::new();
    for (_, payloads) in &tags {
        for (i, &p) in payloads.iter().enumerate() {
            let Some(w) = wir_ty_depth(it, table, sigs, p, span, depth + 1)? else {
                return Err(refuse("unit-typed tag payloads", span));
            };
            match slots.get(i) {
                None => slots.push(w),
                Some(&s) if s == w => {}
                Some(_) => {
                    return Err(refuse(
                        "row payload slots with conflicting types across tags \
                         (spilled union layout, c06)",
                        span,
                    ));
                }
            }
        }
    }
    Ok(slots)
}

/// Is the sema type an unsigned integer (through wrappers)? Decides
/// `u*.chk` op and `icmp.u*` condition selection.
fn sema_unsigned(table: &TypeTable, id: TyId) -> bool {
    match table.kind(id) {
        TyKind::Prim(Prim::Uint | Prim::U8 | Prim::U16 | Prim::U32 | Prim::U64) => true,
        TyKind::Wrapping(t) | TyKind::Distinct(t) => sema_unsigned(table, *t),
        _ => false,
    }
}

/// Byte size of a WIR scalar (the v0 layout used by `stack.alloc`
/// slots and `deref` facts; aggregate layout finalization is c06's).
fn scalar_size(t: TypeId) -> Option<u64> {
    match t {
        types::I8 | types::BOOL => Some(1),
        types::I16 => Some(2),
        types::I32 | types::F32 => Some(4),
        types::I64 | types::F64 | types::PTR => Some(8),
        _ => None,
    }
}

/// Byte size of a FLAT value: a scalar, or an aggregate of flat values
/// packed sequentially with no padding — the v0 SPILL layout `mut`
/// arguments and receivers use (s27; c06 finalizes real layout, and
/// the spill slots are private to one call, so only self-consistency
/// matters).
fn flat_size(it: &types::TypeInterner, t: TypeId) -> Option<u64> {
    if let Some(s) = scalar_size(t) {
        return Some(s);
    }
    match it.get(t) {
        types::TypeData::Agg(fields) => {
            let mut sum = 0u64;
            for f in fields.clone() {
                sum += flat_size(it, f)?;
            }
            Some(sum)
        }
        _ => None,
    }
}

/// The packed byte offset of each field of a flat aggregate.
fn flat_offsets(it: &types::TypeInterner, fields: &[TypeId]) -> Option<Vec<u64>> {
    let mut out = Vec::with_capacity(fields.len());
    let mut off = 0u64;
    for &f in fields {
        out.push(off);
        off += flat_size(it, f)?;
    }
    Some(out)
}

fn types_is_int(t: TypeId) -> bool {
    matches!(t, types::I8 | types::I16 | types::I32 | types::I64)
}

fn row_is_empty(table: &TypeTable, row: TyId) -> bool {
    matches!(
        table.kind(row),
        TyKind::Row { tags, tail } if tags.is_empty() && tail.is_none()
    )
}

/// Build (and cache) the WIR signature for a fn item.
///
/// `mut` parameters are POINTER-SHAPED (s26): each lowers to a `mut
/// ptr` parameter immediately followed by that slot's `mem.rK` token
/// parameter — formal regions numbered r0, r1, … in `mut`-param order.
/// Call sites bind formals to actual regions (the caller's spill
/// slots); the verifier substitutes consistently. Only scalar `mut`
/// params lower today (aggregate field addressing needs c06 layout).
fn wir_fn_sig(
    module: &mut Module,
    cache: &mut HashMap<String, SigId>,
    sigs: &SigTables,
    name: &str,
    fsig: &FnSig,
    span: Span,
) -> R<SigId> {
    if let Some(&sig) = cache.get(name) {
        return Ok(sig);
    }
    let sig = wir_sig_of(module, sigs, fsig, span)?;
    cache.insert(name.to_string(), sig);
    Ok(sig)
}

/// The uncached signature build (shared by definitions and call-site
/// imports so both see the same `mut` → (ptr, token) expansion).
fn wir_sig_of(module: &mut Module, sigs: &SigTables, fsig: &FnSig, span: Span) -> R<SigId> {
    let mut params = Vec::with_capacity(fsig.params.len());
    let mut next_formal = 0u32;
    for p in &fsig.params {
        let Some(ty) = wir_ty(&mut module.types, &sigs.table, sigs, p.ty, p.span)? else {
            return Err(refuse("unit-typed parameters", p.span));
        };
        match p.mode {
            None => params.push(Param {
                ty,
                mode: Mode::Val,
            }),
            Some(ParamMode::Take) => params.push(Param {
                ty,
                mode: Mode::Take,
            }),
            Some(ParamMode::Mut) => {
                if flat_size(&module.types, ty).is_none() {
                    return Err(refuse(
                        "`mut` parameters of non-flat types (spill layout, c06)",
                        p.span,
                    ));
                }
                let formal = crate::types::RegionId::new(next_formal);
                next_formal += 1;
                let tok = module.types.mem(formal);
                params.push(Param {
                    ty: types::PTR,
                    mode: Mode::Mut,
                });
                params.push(Param {
                    ty: tok,
                    mode: Mode::Val,
                });
            }
        }
    }
    let results = match wir_ty(&mut module.types, &sigs.table, sigs, fsig.ret, span)? {
        Some(t) => vec![t],
        None => vec![],
    };
    Ok(module.make_sig(params, results))
}

/// Control-flow result of lowering one expression/statement.
#[derive(Clone, Copy, Debug)]
enum Flow {
    /// Evaluation fell through with a value (`None` = unit).
    Val(Option<Value>),
    /// Control diverged (return/break/continue/provable trap): the
    /// current block is filled; stop emitting on this path.
    Diverged,
}

use crate::ir::Value;

macro_rules! flow_val {
    ($e:expr) => {
        match $e? {
            Flow::Val(v) => v,
            Flow::Diverged => return Ok(Flow::Diverged),
        }
    };
}

/// One name binding in scope.
#[derive(Clone, Copy)]
enum LocalBind {
    Val {
        var: Var,
        /// The binding is `wrapping[T]`-typed (compound assignment
        /// selects `.wrap` ops). Computed at bind time against the
        /// binding's OWN interner — sig and body tables never mix.
        wrapping: bool,
        /// Unsigned-typed binding (compound assignment selects the
        /// `u*.chk` family — the s26 op-set decision).
        unsigned: bool,
        wir_ty: TypeId,
    },
    /// A `mut`-mode parameter: pointer-shaped (s26). Reads load, writes
    /// store — both through the slot region's token chain.
    MutRef {
        ptr: Value,
        region: RegionId,
        elem: TypeId,
        wrapping: bool,
        unsigned: bool,
    },
    /// A region binding (`region r { }` name, `let r = region(...)`,
    /// `let f = freeze r`): compile-time identity + runtime handle.
    Region {
        region: RegionId,
        handle: Value,
        frozen: bool,
    },
    /// A unit-typed binding (no runtime value).
    Unit,
}

/// One `defer`/`errdefer` entry: the typed AST fragment, re-lowered at
/// every applicable exit edge (LIFO), plus a visibility fence so the
/// re-lowering resolves names exactly as the declaration site saw them
/// (captured SSA is read through Braun variables, so values are the
/// CURRENT ones at the exit — `[mem.model.order]`: defers run as the
/// frames return).
#[derive(Clone, Copy)]
struct DeferEntry<'t> {
    expr: &'t GreenNode,
    errdefer: bool,
    /// (scope index, binds visible in that scope) at declaration.
    fence: (usize, usize),
}

/// One lexical scope: its bindings, its cleanup entries, and — for
/// `region name { }` sugar — the region to free wholesale on every
/// exit edge (X4: the free is the scope's outermost cleanup entry).
#[derive(Default)]
struct ScopeFrame<'t> {
    binds: Vec<(String, LocalBind)>,
    defers: Vec<DeferEntry<'t>>,
    region: Option<(RegionId, Value)>,
}

struct LoopFrame {
    /// Jump target of `continue`: the header for `while`/`loop`, the
    /// (lazily created) increment latch for `for`.
    continue_to: ContinueTo,
    exit: Option<Block>,
    /// The exit block's value parameter, minted by the first
    /// break-with-value (`let x = loop { break 5 }`).
    exit_param: Option<Value>,
    /// Scope depth at loop entry: `break`/`continue` unwind the defer
    /// chains of every deeper scope before jumping.
    depth: usize,
}

#[derive(Clone, Copy)]
enum ContinueTo {
    Block(Block),
    /// A `for` loop's latch, created on first use so a body that
    /// always diverges leaves no unreachable block behind.
    ForLatch(Option<Block>),
}

/// What a `match` scrutinee's discriminant ranges over (s27).
enum MatchDomain {
    /// An enum: variant name → (declaration index, payload arity).
    Enum(Vec<(String, i64, usize)>),
    /// An error row: tag name → payload arity (ids are module-interned
    /// at test-emission time; open rows admit unlisted tags, which can
    /// only be matched by a rest arm).
    Row(Vec<(String, usize)>),
    /// Integer/bool literals.
    Scalar,
}

/// One lowered pattern's shape.
enum PatShape {
    /// Matches anything; `Some(name)` binds the whole scrutinee.
    Irrefutable(Option<String>),
    /// Discriminant equals one of these constants (or-alternatives);
    /// payload bindings — (slot index, name) — only for the
    /// single-alternative form.
    Tests(Vec<i64>, Vec<(usize, String)>),
    /// Bool literal (the discriminant IS the condition).
    BoolTest(bool),
}

/// How one `mut` argument reaches the callee (s26).
enum MutArg {
    /// A local place spills to a fresh stack slot; the callee gets the
    /// slot and the place reloads on return.
    Spill {
        cur: Value,
        size: u64,
        writeback: WriteBackShape,
    },
    /// A `mut` parameter re-lent onward: same pointer, same region.
    Relend { ptr: Value, region: RegionId },
}

/// What to restore from a spill slot after the call.
enum WriteBackShape {
    /// A whole local: reload and redefine.
    Var { var: Var, ty: TypeId },
    /// A (possibly nested) field path of a by-value aggregate local:
    /// reload the leaf and rebuild the aggregates around it.
    Field {
        var: Var,
        path: Vec<usize>,
        fty: TypeId,
    },
}

struct WriteBack {
    shape: WriteBackShape,
    slot: Value,
    region: RegionId,
    span: Span,
}

impl WriteBackShape {
    fn filled(self, slot: Value, region: RegionId, span: Span) -> WriteBack {
        WriteBack {
            shape: self,
            slot,
            region,
            span,
        }
    }
}

struct Lowerer<'t, 'b, 'm> {
    src: &'t [u8],
    /// The body's own type table.
    table: &'t TypeTable,
    /// The signature table's interner (param/return types live there).
    sig_table: &'t TypeTable,
    /// The whole signature set (adapter-type base resolution).
    sigs: &'t SigTables,
    calls: HashMap<Span, &'t CallSig>,
    expr_tys: HashMap<Span, TyId>,
    local_tys: HashMap<Span, TyId>,
    casts: HashMap<Span, (TyId, TyId, CastKind)>,
    fns: &'t HashMap<&'t str, Vec<(usize, &'t FnSig)>>,
    /// Method dispatch decisions by call span (s17/s27).
    dispatch: HashMap<Span, &'t Dispatch>,
    /// Per-`match` exhaustiveness facts by span (s17).
    matches: HashMap<Span, bool>,
    scopes: Vec<ScopeFrame<'t>>,
    /// In-force visibility fence while re-lowering a defer entry at an
    /// exit site: (scope index, binds visible there, stack depth when
    /// installed — the defer body's own deeper scopes stay visible).
    visible: Option<(usize, usize, usize)>,
    loops: Vec<LoopFrame>,
    /// The function's fallible shape: the eu WIR type of its return
    /// (None when the return is not a tagged error union).
    fn_eu: Option<TypeId>,
    /// The function body's trailing-expression span when the function
    /// is fallible: the tail routes through `emit_return` so errdefer
    /// applicability is decided uniformly.
    fn_tail: Option<Span>,
    /// Per-function callee import cache.
    callees: HashMap<String, ExtFunc>,
    /// Semi-pruned pre-scan verdict: a function whose body contains no
    /// control construct is one block, so every variable is
    /// single-block and bypasses the global Braun maps.
    straight_line: bool,
    b: &'b mut FuncBuilder<'m>,
}

impl<'t, 'b, 'm> Lowerer<'t, 'b, 'm> {
    fn text(&self, span: Span) -> String {
        String::from_utf8_lossy(&self.src[span.lo as usize..span.hi as usize]).into_owned()
    }

    fn expr_sema_ty(&self, span: Span) -> Option<TyId> {
        self.expr_tys.get(&span).copied()
    }

    fn lower_fn(&mut self, fsig: &FnSig, block: AstBlock<'t>) -> R<()> {
        self.straight_line = !contains_control(block.syntax());
        // The fallible shape (s27): a tagged-row return means every
        // return site produces the eu pair, and the body tail routes
        // through `emit_return`.
        if let TyKind::ErrUnion(_, row) = self.sig_table.kind(fsig.ret)
            && !row_is_empty(self.sig_table, *row)
        {
            let eu = wir_ty(
                &mut self.b.module.types,
                self.sig_table,
                self.sigs,
                fsig.ret,
                fsig.name_span,
            )?
            .expect("a tagged error union is a value");
            self.fn_eu = Some(eu);
            self.fn_tail = block.trailing_expr().map(|e| e.span);
        }
        // Prologue: signature params are the entry block's params —
        // with each `mut` param expanded to (ptr, mem token) pairs.
        let entry_params = self.b.block_params(self.b.current_block());
        self.scopes.push(ScopeFrame::default());
        let mut wir_idx = 0usize;
        let mut mut_ptrs: Vec<(Value, TypeId)> = Vec::new();
        for p in &fsig.params {
            let Some(wty) = wir_ty(
                &mut self.b.module.types,
                self.sig_table,
                self.sigs,
                p.ty,
                p.span,
            )?
            else {
                unreachable!("unit params refused at sig build");
            };
            let wrapping = matches!(self.sig_table.kind(p.ty), TyKind::Wrapping(_));
            let unsigned = sema_unsigned(self.sig_table, p.ty);
            let bind = if p.mode == Some(ParamMode::Mut) {
                let ptr = entry_params[wir_idx];
                let tok = entry_params[wir_idx + 1];
                wir_idx += 2;
                let types::TypeData::Mem(region) =
                    *self.b.module.types.get(self.b.func.value_ty(tok))
                else {
                    unreachable!("mut params carry their token param");
                };
                mut_ptrs.push((ptr, wty));
                LocalBind::MutRef {
                    ptr,
                    region,
                    elem: wty,
                    wrapping,
                    unsigned,
                }
            } else {
                let val = entry_params[wir_idx];
                wir_idx += 1;
                let var = self.b.declare_var(wty);
                if self.straight_line {
                    self.b.mark_single_block(var);
                }
                self.b.def_var(var, val);
                // s30 debug aux: the parameter's name and entry value
                // (DW_TAG_formal_parameter rides this).
                self.b.func.add_debug_var(p.name.clone(), val, true);
                LocalBind::Val {
                    var,
                    wrapping,
                    unsigned,
                    wir_ty: wty,
                }
            };
            self.scopes
                .last_mut()
                .expect("scope")
                .binds
                .push((p.name.clone(), bind));
        }
        // The c04 handoff, at last (s26): the exclusivity theorem the
        // memory checker proved for each `mut` parameter becomes entry
        // facts — dereferenceable for the element, and pairwise noalias
        // (the hand-`restrict`-C killer). The wir rung only runs on
        // mem-clean packages, so the citation always has its theorem.
        for &(ptr, elem) in &mut_ptrs {
            let size = flat_size(&self.b.module.types, elem).expect("mut params are flat");
            self.b.func.add_fact(FactData::new(
                FactKind::Deref(ptr, DerefSize::Const(size)),
                Just::Theorem(Theorem::ExclMut),
            ));
        }
        for (i, &(a, _)) in mut_ptrs.iter().enumerate() {
            for &(b, _) in &mut_ptrs[i + 1..] {
                self.b.func.add_fact(FactData::new(
                    FactKind::Noalias(a, b),
                    Just::Theorem(Theorem::ExclMut),
                ));
            }
        }
        let ret = wir_ty(
            &mut self.b.module.types,
            self.sig_table,
            self.sigs,
            fsig.ret,
            fsig.name_span,
        )?;
        match self.lower_block(block, ret.is_some())? {
            Flow::Diverged => {}
            Flow::Val(v) => match (ret, v) {
                (Some(_), Some(val)) => self.b.ins_ret(&[val]),
                (Some(eu), None) if self.fn_eu == Some(eu) => {
                    // A unit-ok fallible fn falling through: the
                    // implicit ok return wraps nothing.
                    let ok = self.b.ins_eu_make_ok(eu, None);
                    self.b.ins_ret(&[ok]);
                }
                (Some(_), None) => {
                    // Typed return with a unit trailing value: the
                    // checker guarantees this cannot happen on a
                    // fall-through path of a value-returning fn.
                    return Err(refuse(
                        "fall-through without a return value",
                        fsig.name_span,
                    ));
                }
                (None, _) => self.b.ins_ret(&[]),
            },
        }
        self.scopes.pop();
        Ok(())
    }

    // ---------------------------------------------------- blocks ----

    fn lower_block(&mut self, block: AstBlock<'t>, want_value: bool) -> R<Flow> {
        self.lower_block_in(block, want_value, None)
    }

    /// Lower a block in a fresh scope; `region` attaches the X4 sugar's
    /// wholesale free as the scope's outermost cleanup entry.
    fn lower_block_in(
        &mut self,
        block: AstBlock<'t>,
        want_value: bool,
        region: Option<(RegionId, Value)>,
    ) -> R<Flow> {
        self.scopes.push(ScopeFrame {
            region,
            ..ScopeFrame::default()
        });
        let last_value = if want_value {
            block.trailing_expr().map(|e| e.span)
        } else {
            None
        };
        let mut out: Option<Value> = None;
        for stmt in block.statements() {
            let flow = self.lower_stmt(stmt, last_value, &mut out);
            match flow {
                Ok(Flow::Val(_)) => {}
                Ok(Flow::Diverged) => {
                    self.scopes.pop();
                    return Ok(Flow::Diverged);
                }
                Err(e) => {
                    self.scopes.pop();
                    return Err(e);
                }
            }
        }
        // Normal fall-through: this scope's own cleanup chain (the
        // trailing value is already an SSA value — formed before the
        // defers run, [mem.model.order]).
        let si = self.scopes.len() - 1;
        let flowing = self.run_one_scope_exit(si, false);
        self.scopes.pop();
        if !flowing? {
            return Ok(Flow::Diverged);
        }
        Ok(Flow::Val(out))
    }

    // ------------------------------------------- exit edges (s27) ----

    /// Lower the LIFO cleanup chain for every scope deeper than
    /// `down_to` (scopes `down_to..` unwind, innermost first) at the
    /// CURRENT emission point — one call per exit edge. `errdefer`
    /// entries run only on error edges. Returns false when a cleanup
    /// provably diverged (the edge is already terminated).
    fn run_exits(&mut self, down_to: usize, err_path: bool) -> R<bool> {
        for si in (down_to..self.scopes.len()).rev() {
            if !self.run_one_scope_exit(si, err_path)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn run_one_scope_exit(&mut self, si: usize, err_path: bool) -> R<bool> {
        let entries = self.scopes[si].defers.clone();
        for ent in entries.iter().rev() {
            if ent.errdefer && !err_path {
                continue;
            }
            // Re-lower the fragment behind a visibility fence (names
            // resolve as the declaration saw them) with the loop stack
            // masked (a `break` inside a defer never targets a loop
            // outside it).
            let saved_vis = self.visible;
            let saved_loops = std::mem::take(&mut self.loops);
            self.visible = Some((ent.fence.0, ent.fence.1, self.scopes.len()));
            let r = self.lower_expr(ent.expr);
            self.visible = saved_vis;
            self.loops = saved_loops;
            match r? {
                Flow::Val(_) => {}
                Flow::Diverged => return Ok(false),
            }
        }
        if let Some((region, handle)) = self.scopes[si].region {
            // The X4 free point: LIFO-outermost in its scope, present
            // on EVERY exit edge; the verifier's per-path token
            // linearity independently proves no edge misses or
            // doubles it.
            self.b.ins_region_free(region, handle);
        }
        Ok(true)
    }

    /// Emit one return: run the full cleanup chain, then `ret`. For a
    /// fallible function returning a DYNAMIC union (not provably ok or
    /// err at build time) with errdefer entries pending, the exit
    /// forks on `eu.is_err` so the errdefer chain runs exactly on the
    /// error edge.
    fn emit_return(&mut self, v: Option<Value>) -> R<()> {
        let vals: Vec<Value> = v.into_iter().collect();
        if let Some(val) = v
            && self.fn_eu.is_some()
        {
            let def_op = match self.b.func.values[val].def {
                crate::ir::ValueDef::Result(di, _) => Some(self.b.func.insts[di].op),
                _ => None,
            };
            let has_errdefers = self
                .scopes
                .iter()
                .any(|s| s.defers.iter().any(|d| d.errdefer));
            let err_path = match def_op {
                Some(Opcode::EuMakeErr) => true,
                Some(Opcode::EuMakeOk) => false,
                _ if !has_errdefers => false,
                _ => {
                    // Dynamic: fork the exit.
                    let is_err = self.b.ins_eu_is_err(val);
                    if let Some(c) = self.b.as_bool_const(is_err) {
                        if self.run_exits(0, c)? {
                            self.b.ins_ret(&vals);
                        }
                        return Ok(());
                    }
                    let err_bb = self.b.create_block();
                    let ok_bb = self.b.create_block();
                    self.b.ins_br(is_err, err_bb, &[], ok_bb, &[]);
                    self.b.seal_block(err_bb);
                    self.b.seal_block(ok_bb);
                    self.b.switch_to_block(err_bb);
                    self.b.gvn_push_scope();
                    let flowing = self.run_exits(0, true);
                    self.b.gvn_pop_scope();
                    if flowing? {
                        self.b.ins_ret(&vals);
                    }
                    self.b.switch_to_block(ok_bb);
                    self.b.gvn_push_scope();
                    let flowing = self.run_exits(0, false);
                    self.b.gvn_pop_scope();
                    if flowing? {
                        self.b.ins_ret(&vals);
                    }
                    return Ok(());
                }
            };
            if self.run_exits(0, err_path)? {
                self.b.ins_ret(&vals);
            }
            return Ok(());
        }
        if self.run_exits(0, false)? {
            self.b.ins_ret(&vals);
        }
        Ok(())
    }

    fn lower_stmt(
        &mut self,
        stmt: &'t GreenNode,
        last_value: Option<Span>,
        out: &mut Option<Value>,
    ) -> R<Flow> {
        // s30: statement-grain span threading — every instruction this
        // statement expands into (checks, `?` branches, defer chains
        // re-lowered at its exits) inherits the statement's span, so
        // line tables step statements and never land on synthesized
        // code.
        self.b.set_span(stmt.span.lo, stmt.span.hi);
        match stmt.kind {
            SyntaxKind::ExprStmt => {
                let d = ExprStmt::cast(stmt).expect("kind");
                if let Some(e) = d.expr() {
                    let wanted = Some(e.span) == last_value;
                    // A fallible function's body tail is a return site:
                    // route it through `emit_return` so ok-wrapping and
                    // errdefer applicability are decided uniformly.
                    if wanted
                        && self.fn_tail == Some(e.span)
                        && let Some(eu) = self.fn_eu
                    {
                        let v = flow_val!(self.lower_fallible_expr(e, eu));
                        self.emit_return(v)?;
                        return Ok(Flow::Diverged);
                    }
                    let v = flow_val!(self.lower_expr_w(e, wanted));
                    if wanted {
                        *out = v;
                    }
                }
                Ok(Flow::Val(None))
            }
            SyntaxKind::LetDecl => {
                let d = LetDecl::cast(stmt).expect("kind");
                self.lower_binding(d.pattern(), d.init(), stmt.span)
            }
            SyntaxKind::VarDecl => {
                let d = VarDecl::cast(stmt).expect("kind");
                self.lower_binding(d.pattern(), d.init(), stmt.span)
            }
            SyntaxKind::ConstDecl => {
                let d = ConstDecl::cast(stmt).expect("kind");
                let name = d.name().map(|t| t.span);
                self.lower_binding_named(name, d.init(), stmt.span)
            }
            SyntaxKind::AssignStmt => self.lower_assign(stmt),
            SyntaxKind::DeferStmt => {
                // Registration only — zero code here. The fragment
                // re-lowers at every applicable exit edge (LIFO).
                let d = DeferStmt::cast(stmt).expect("kind");
                if let Some(expr) = d.expr() {
                    let si = self.scopes.len() - 1;
                    let fence = (si, self.scopes[si].binds.len());
                    self.scopes[si].defers.push(DeferEntry {
                        expr,
                        errdefer: d.is_errdefer(),
                        fence,
                    });
                }
                Ok(Flow::Val(None))
            }
            SyntaxKind::AssumeStmt => Err(refuse(
                "assume noalias (unsafe-tier WIR ops, deferred from s26 — see closeout)",
                stmt.span,
            )),
            k if k.is_item() => Err(refuse("nested item declarations", stmt.span)),
            _ => Ok(Flow::Val(None)),
        }
    }

    fn lower_binding(
        &mut self,
        pat: Option<&'t GreenNode>,
        init: Option<&'t GreenNode>,
        span: Span,
    ) -> R<Flow> {
        let Some(pat) = pat else {
            return Ok(Flow::Val(None));
        };
        match pat.kind {
            SyntaxKind::IdentPat => self.lower_binding_named(Some(pat.span), init, span),
            SyntaxKind::WildcardPat => {
                if let Some(e) = init {
                    flow_val!(self.lower_expr(e));
                }
                Ok(Flow::Val(None))
            }
            _ => Err(refuse(
                "destructuring bindings (tuple/struct patterns, c06)",
                pat.span,
            )),
        }
    }

    fn lower_binding_named(
        &mut self,
        name_span: Option<Span>,
        init: Option<&'t GreenNode>,
        span: Span,
    ) -> R<Flow> {
        let Some(init) = init else {
            return Err(refuse(
                "uninitialized bindings (definite-assignment lowering)",
                span,
            ));
        };
        // Region-typed bindings are compile-time identities plus a
        // runtime handle — not ordinary SSA values (X4's value form,
        // local-binding shape; general first-class flow is c05).
        if let Some(bind) = self.lower_region_init(init)? {
            if let Some(name_span) = name_span {
                let name = self.text(name_span);
                self.scopes
                    .last_mut()
                    .expect("scope")
                    .binds
                    .push((name, bind));
            }
            return Ok(Flow::Val(None));
        }
        let v = flow_val!(self.lower_expr(init));
        let Some(name_span) = name_span else {
            return Ok(Flow::Val(None));
        };
        let name = self.text(name_span);
        let sema_ty = self
            .local_tys
            .get(&name_span)
            .copied()
            .or_else(|| self.expr_sema_ty(init.span));
        let Some(sema_ty) = sema_ty else {
            return Err(refuse("a binding without a recorded type", span));
        };
        let wty = wir_ty(
            &mut self.b.module.types,
            self.table,
            self.sigs,
            sema_ty,
            span,
        )?;
        let bind = match (wty, v) {
            (Some(wty), Some(val)) => {
                let var = self.b.declare_var(wty);
                if self.straight_line {
                    self.b.mark_single_block(var);
                }
                self.b.def_var(var, val);
                // s30 debug aux: `let`/`var`/`const` binding.
                self.b.func.add_debug_var(name.clone(), val, false);
                LocalBind::Val {
                    var,
                    wrapping: matches!(self.table.kind(sema_ty), TyKind::Wrapping(_)),
                    unsigned: sema_unsigned(self.table, sema_ty),
                    wir_ty: wty,
                }
            }
            (None, _) => LocalBind::Unit,
            (Some(_), None) => {
                return Err(refuse("a typed binding of a valueless expression", span));
            }
        };
        self.scopes
            .last_mut()
            .expect("scope")
            .binds
            .push((name, bind));
        Ok(Flow::Val(None))
    }

    /// `Some(bind)` when `init` is a region-producing form: the value
    /// form `region(strategy?)`, or `freeze r` over a region binding.
    fn lower_region_init(&mut self, init: &'t GreenNode) -> R<Option<LocalBind>> {
        match init.kind {
            SyntaxKind::RegionValue => {
                // Strategy selection (arena/rc/pool) is runtime-handle
                // configuration; the s28 lowering to wolf_rt carries it.
                let (region, handle) = self.b.ins_region_new();
                Ok(Some(LocalBind::Region {
                    region,
                    handle,
                    frozen: false,
                }))
            }
            SyntaxKind::FreezeExpr => {
                let d = wolf_ast::FreezeExpr::cast(init).expect("kind");
                let Some(operand) = d.expr() else {
                    return Err(refuse("freeze without an operand", init.span));
                };
                let (region, handle) = self.expect_region(operand)?;
                let frozen_tok = self.b.ins_sync_freeze(region, handle);
                // Deep immutability is a fact about the handle from the
                // freeze point on (op-justified; the verifier checks the
                // citation is a sync.freeze).
                self.b.func.add_fact(FactData::new(
                    FactKind::Frozen(handle),
                    Just::Op(frozen_tok),
                ));
                self.mark_frozen(operand);
                Ok(Some(LocalBind::Region {
                    region,
                    handle,
                    frozen: true,
                }))
            }
            _ => Ok(None),
        }
    }

    /// Resolve an expression that must name a region binding.
    fn expect_region(&mut self, e: &'t GreenNode) -> R<(RegionId, Value)> {
        if e.kind != SyntaxKind::PathExpr {
            return Err(refuse(
                "region operands beyond named bindings (c05)",
                e.span,
            ));
        }
        let name = self.text(e.span);
        match self.lookup(&name) {
            Some(LocalBind::Region { region, handle, .. }) => Ok((region, handle)),
            _ => Err(refuse(
                "region operands beyond named bindings (c05)",
                e.span,
            )),
        }
    }

    /// Mark the named region binding frozen (freeze consumed the affine
    /// mutable capability; scope-end bookkeeping must not free it — RC
    /// owns frozen regions, X4).
    fn mark_frozen(&mut self, e: &'t GreenNode) {
        if e.kind != SyntaxKind::PathExpr {
            return;
        }
        let name = self.text(e.span);
        let mut frozen_region: Option<RegionId> = None;
        'outer: for scope in self.scopes.iter_mut().rev() {
            for (n, b) in scope.binds.iter_mut().rev() {
                if *n == name
                    && let LocalBind::Region { frozen, region, .. } = b
                {
                    *frozen = true;
                    frozen_region = Some(*region);
                    break 'outer;
                }
            }
        }
        // A frozen region is RC-owned (X4): scope-end bookkeeping must
        // not free it — clear any pending wholesale-free entry.
        if let Some(r) = frozen_region {
            for scope in self.scopes.iter_mut() {
                if matches!(scope.region, Some((sr, _)) if sr == r) {
                    scope.region = None;
                }
            }
        }
    }

    /// Name lookup, honoring the defer visibility fence: while a defer
    /// fragment re-lowers at an exit site, scopes between its
    /// declaration point and the exit are invisible (its own nested
    /// scopes, pushed at or beyond the install depth, stay visible).
    fn lookup(&self, name: &str) -> Option<LocalBind> {
        for (i, scope) in self.scopes.iter().enumerate().rev() {
            let limit = match self.visible {
                Some((fs, fb, depth)) if i < depth => {
                    if i > fs {
                        continue; // hidden: between the fence and the exit
                    }
                    if i == fs { fb } else { scope.binds.len() }
                }
                _ => scope.binds.len(),
            };
            for (n, b) in scope.binds[..limit].iter().rev() {
                if n == name {
                    return Some(*b);
                }
            }
        }
        None
    }

    fn compound_bin(op: SyntaxKind) -> Option<SyntaxKind> {
        Some(match op {
            SyntaxKind::PlusEq => SyntaxKind::Plus,
            SyntaxKind::MinusEq => SyntaxKind::Minus,
            SyntaxKind::StarEq => SyntaxKind::Star,
            SyntaxKind::SlashEq => SyntaxKind::Slash,
            SyntaxKind::PercentEq => SyntaxKind::Percent,
            SyntaxKind::AmpEq => SyntaxKind::Amp,
            SyntaxKind::PipeEq => SyntaxKind::Pipe,
            SyntaxKind::CaretEq => SyntaxKind::Caret,
            SyntaxKind::ShlEq => SyntaxKind::Shl,
            SyntaxKind::ShrEq => SyntaxKind::Shr,
            _ => return None,
        })
    }

    fn lower_assign(&mut self, stmt: &'t GreenNode) -> R<Flow> {
        let d = AssignStmt::cast(stmt).expect("kind");
        let Some(place) = d.place() else {
            return Ok(Flow::Val(None));
        };
        if place.kind == SyntaxKind::MemberExpr {
            return self.lower_member_assign(d, place, stmt.span);
        }
        if place.kind != SyntaxKind::PathExpr {
            return Err(refuse("assignment through nested places (c06)", place.span));
        }
        let name = self.text(place.span);
        let bind = match self.lookup(&name) {
            Some(b) => b,
            None => {
                return Err(refuse(
                    "assignment to a non-local name (globals, c06)",
                    place.span,
                ));
            }
        };
        match bind {
            LocalBind::Unit => {
                // Unit-typed assignment: evaluate for effect.
                if let Some(e) = d.value() {
                    flow_val!(self.lower_expr(e));
                }
                Ok(Flow::Val(None))
            }
            LocalBind::Region { .. } => Err(refuse(
                "region rebinding (c05 identity backlog)",
                place.span,
            )),
            LocalBind::Val {
                var,
                wrapping,
                unsigned,
                wir_ty: wty,
            } => {
                let Some(value_expr) = d.value() else {
                    return Ok(Flow::Val(None));
                };
                let rhs = flow_val!(self.lower_expr(value_expr));
                let Some(rhs) = rhs else {
                    return Err(refuse(
                        "assignment of a valueless expression",
                        value_expr.span,
                    ));
                };
                let op = d.op().map(|t| t.kind).unwrap_or(SyntaxKind::Eq);
                let newval = if op == SyntaxKind::Eq {
                    rhs
                } else {
                    let cur = self.b.use_var(var);
                    let Some(bin) = Self::compound_bin(op) else {
                        return Err(refuse("this compound assignment operator", stmt.span));
                    };
                    match self.arith(bin, cur, rhs, wrapping, unsigned, wty, stmt.span)? {
                        Some(v) => v,
                        None => return Ok(Flow::Diverged),
                    }
                };
                self.b.def_var(var, newval);
                // s30 debug aux: the rebinding is the name's current
                // definition from here on.
                self.b.func.add_debug_var(name.clone(), newval, false);
                Ok(Flow::Val(None))
            }
            // s25's honest refusal, repaid: a write through a `mut`
            // parameter is a store through its pointer, ordered by the
            // slot region's token chain.
            LocalBind::MutRef {
                ptr,
                region,
                elem,
                wrapping,
                unsigned,
            } => {
                let Some(value_expr) = d.value() else {
                    return Ok(Flow::Val(None));
                };
                let rhs = flow_val!(self.lower_expr(value_expr));
                let Some(rhs) = rhs else {
                    return Err(refuse(
                        "assignment of a valueless expression",
                        value_expr.span,
                    ));
                };
                let op = d.op().map(|t| t.kind).unwrap_or(SyntaxKind::Eq);
                let newval = if op == SyntaxKind::Eq {
                    rhs
                } else {
                    let cur = self.b.ins_load(elem, ptr, region);
                    let Some(bin) = Self::compound_bin(op) else {
                        return Err(refuse("this compound assignment operator", stmt.span));
                    };
                    match self.arith(bin, cur, rhs, wrapping, unsigned, elem, stmt.span)? {
                        Some(v) => v,
                        None => return Ok(Flow::Diverged),
                    }
                };
                self.b.ins_store(newval, ptr, region);
                Ok(Flow::Val(None))
            }
        }
    }

    /// `x.f = v` / `x.f += v` where `x` is a local by-value aggregate:
    /// rebuild the aggregate with the field replaced (registers, not
    /// memory — the promotion story; deeper paths are s27).
    fn lower_member_assign(
        &mut self,
        d: AssignStmt<'t>,
        place: &'t GreenNode,
        span: Span,
    ) -> R<Flow> {
        let m = wolf_ast::MemberExpr::cast(place).expect("kind");
        let Some(base) = m.base() else {
            return Ok(Flow::Val(None));
        };
        if base.kind != SyntaxKind::PathExpr {
            return Err(refuse("assignment through nested places (c06)", place.span));
        }
        let name = self.text(base.span);
        // `self.x = v` through a `mut` receiver: a store at the
        // field's packed offset (s27 methods).
        if let Some(LocalBind::MutRef {
            ptr, region, elem, ..
        }) = self.lookup(&name)
        {
            return self.lower_mut_member_assign(d, m, base, ptr, region, elem, span);
        }
        let Some(LocalBind::Val {
            var,
            wir_ty: agg_ty,
            ..
        }) = self.lookup(&name)
        else {
            return Err(refuse("assignment through nested places (c06)", place.span));
        };
        let Some(base_sema) = self.expr_sema_ty(base.span) else {
            return Err(refuse("a member write without a recorded type", place.span));
        };
        let (index, wrapping, unsigned) = self.member_index(base_sema, m, place.span)?;
        let types::TypeData::Agg(fields) = self.b.module.types.get(agg_ty).clone() else {
            return Err(refuse("member writes on non-aggregates", place.span));
        };
        let Some(&fty) = fields.get(index) else {
            return Err(refuse("a member the aggregate does not carry", place.span));
        };
        let Some(value_expr) = d.value() else {
            return Ok(Flow::Val(None));
        };
        let rhs = flow_val!(self.lower_expr(value_expr));
        let Some(rhs) = rhs else {
            return Err(refuse(
                "assignment of a valueless expression",
                value_expr.span,
            ));
        };
        let cur_agg = self.b.use_var(var);
        let op = d.op().map(|t| t.kind).unwrap_or(SyntaxKind::Eq);
        let newfield = if op == SyntaxKind::Eq {
            rhs
        } else {
            let cur = self
                .b
                .ins(Opcode::AggGet, &[cur_agg], &[fty], Aux::Int(index as i64))
                .one();
            let Some(bin) = Self::compound_bin(op) else {
                return Err(refuse("this compound assignment operator", span));
            };
            match self.arith(bin, cur, rhs, wrapping, unsigned, fty, span)? {
                Some(v) => v,
                None => return Ok(Flow::Diverged),
            }
        };
        let mut parts = Vec::with_capacity(fields.len());
        for (k, &kt) in fields.iter().enumerate() {
            if k == index {
                parts.push(newfield);
            } else {
                parts.push(
                    self.b
                        .ins(Opcode::AggGet, &[cur_agg], &[kt], Aux::Int(k as i64))
                        .one(),
                );
            }
        }
        let rebuilt = self
            .b
            .ins(Opcode::AggMake, &parts, &[agg_ty], Aux::None)
            .one();
        self.b.def_var(var, rebuilt);
        Ok(Flow::Val(None))
    }

    /// `self.f = v` / `self.f += v` where `self` is a pointer-shaped
    /// `mut` receiver: a store at the field's packed offset, through
    /// the slot region's token chain.
    #[allow(clippy::too_many_arguments)]
    fn lower_mut_member_assign(
        &mut self,
        d: AssignStmt<'t>,
        m: wolf_ast::MemberExpr<'t>,
        base: &'t GreenNode,
        ptr: Value,
        region: RegionId,
        elem: TypeId,
        span: Span,
    ) -> R<Flow> {
        let Some(base_sema) = self.expr_sema_ty(base.span) else {
            return Err(refuse("a member write without a recorded type", span));
        };
        let (index, wrapping, unsigned) = self.member_index(base_sema, m, span)?;
        let types::TypeData::Agg(fields) = self.b.module.types.get(elem).clone() else {
            return Err(refuse("member writes on non-aggregate receivers", span));
        };
        let Some(offs) = flat_offsets(&self.b.module.types, &fields) else {
            return Err(refuse("member writes on non-flat receivers", span));
        };
        let Some(&fty) = fields.get(index) else {
            return Err(refuse("a member the receiver does not carry", span));
        };
        let Some(value_expr) = d.value() else {
            return Ok(Flow::Val(None));
        };
        let rhs = flow_val!(self.lower_expr(value_expr));
        let Some(rhs) = rhs else {
            return Err(refuse(
                "assignment of a valueless expression",
                value_expr.span,
            ));
        };
        let addr = self.field_addr(ptr, offs[index]);
        let op = d.op().map(|t| t.kind).unwrap_or(SyntaxKind::Eq);
        let newval = if op == SyntaxKind::Eq {
            rhs
        } else {
            let cur = self.load_flat(fty, addr, region, span)?;
            let Some(bin) = Self::compound_bin(op) else {
                return Err(refuse("this compound assignment operator", span));
            };
            match self.arith(bin, cur, rhs, wrapping, unsigned, fty, span)? {
                Some(v) => v,
                None => return Ok(Flow::Diverged),
            }
        };
        self.store_flat(newval, addr, region, span)?;
        Ok(Flow::Val(None))
    }

    /// Resolve a member access against the base's SEMA type: field
    /// index in declared order plus the field's wrapping/unsigned
    /// classification (struct fields live in the signature table,
    /// tuple elements in the body table).
    fn member_index(
        &self,
        base_sema: TyId,
        m: wolf_ast::MemberExpr<'t>,
        span: Span,
    ) -> R<(usize, bool, bool)> {
        let Some(member) = m.member() else {
            return Err(refuse("a member access without a member", span));
        };
        let mname = self.text(member.span);
        // Unwrap adapters to the underlying nominal/tuple.
        let mut ty = base_sema;
        let mut table = self.table;
        for _ in 0..32 {
            match table.kind(ty) {
                TyKind::Distinct(inner) => ty = *inner,
                _ => break,
            }
        }
        loop {
            match table.kind(ty) {
                TyKind::Nominal { module, name } => match self.sigs.get(*module as usize, name) {
                    Some(ItemSig::Struct(ss)) => {
                        let Some(idx) = ss.fields.iter().position(|f| f.name == mname) else {
                            return Err(refuse("a member the struct does not declare", span));
                        };
                        let fty = ss.fields[idx].ty;
                        let wrapping = matches!(self.sig_table.kind(fty), TyKind::Wrapping(_));
                        let unsigned = sema_unsigned(self.sig_table, fty);
                        return Ok((idx, wrapping, unsigned));
                    }
                    Some(ItemSig::Distinct { base, .. }) => {
                        ty = *base;
                        table = self.sig_table;
                    }
                    _ => return Err(refuse("member access on this type (c06/std)", span)),
                },
                TyKind::Tuple(elems) => {
                    let Ok(idx) = mname.parse::<usize>() else {
                        return Err(refuse("named members on tuples", span));
                    };
                    let Some(&fty) = elems.get(idx) else {
                        return Err(refuse("a tuple index out of range", span));
                    };
                    let wrapping = matches!(table.kind(fty), TyKind::Wrapping(_));
                    let unsigned = sema_unsigned(table, fty);
                    return Ok((idx, wrapping, unsigned));
                }
                _ => return Err(refuse("member access on this type (c06/std)", span)),
            }
        }
    }

    // ----------------------------------------------- expressions ----

    fn lower_expr(&mut self, e: &'t GreenNode) -> R<Flow> {
        self.lower_expr_w(e, true)
    }

    /// `want`: the surrounding context consumes the value. Only block
    /// and if lowering care — an else-if chain inherits the demand even
    /// when sema recorded no type for the nested if node.
    fn lower_expr_w(&mut self, e: &'t GreenNode, want: bool) -> R<Flow> {
        match e.kind {
            SyntaxKind::LiteralExpr => self.lower_literal(e),
            SyntaxKind::ParenExpr => match ParenExpr::cast(e).and_then(|p| p.expr()) {
                Some(inner) => self.lower_expr(inner),
                None => Ok(Flow::Val(None)),
            },
            SyntaxKind::PathExpr => {
                let name = self.text(e.span);
                match self.lookup(&name) {
                    Some(LocalBind::Val { var, .. }) => Ok(Flow::Val(Some(self.b.use_var(var)))),
                    // A read of a `mut` parameter is a load through the
                    // slot region's token chain (store→load forwarding
                    // keeps straight-line reads free).
                    Some(LocalBind::MutRef {
                        ptr, region, elem, ..
                    }) => Ok(Flow::Val(Some(
                        self.read_mut_ref(ptr, region, elem, e.span)?,
                    ))),
                    Some(LocalBind::Region { .. }) => Err(refuse(
                        "first-class region values beyond local bindings (c05)",
                        e.span,
                    )),
                    Some(LocalBind::Unit) => Ok(Flow::Val(None)),
                    None => {
                        // An unresolved bare name whose recorded type
                        // is a tagged union is an error-tag RAISE
                        // (sema typed it by injection, D30): the value
                        // is just the eu pair — errdefer applicability
                        // is decided where it leaves the function.
                        if let Some(eu) = self.raise_target(e.span)? {
                            let id = self.b.module.tag_id(&name);
                            let tag = self.b.iconst(types::I64, id);
                            return Ok(Flow::Val(Some(self.b.ins_eu_make_err(eu, tag, &[]))));
                        }
                        Err(refuse("module-item reads (globals, c06)", e.span))
                    }
                }
            }
            SyntaxKind::Block => {
                let d = AstBlock::cast(e).expect("kind");
                self.lower_block(d, want)
            }
            SyntaxKind::PrefixExpr => self.lower_prefix(e),
            SyntaxKind::BinExpr => self.lower_bin(e),
            SyntaxKind::CastExpr => self.lower_cast(e),
            SyntaxKind::IfExpr => self.lower_if(e, want),
            SyntaxKind::WhileExpr => self.lower_while(e),
            SyntaxKind::LoopExpr => self.lower_loop(e),
            SyntaxKind::CallExpr => self.lower_call(e),
            SyntaxKind::ReturnExpr => {
                let d = ReturnExpr::cast(e).expect("kind");
                let v = match (d.value(), self.fn_eu) {
                    (Some(x), Some(eu)) => match self.lower_fallible_expr(x, eu)? {
                        Flow::Val(v) => v,
                        Flow::Diverged => return Ok(Flow::Diverged),
                    },
                    (Some(x), None) => flow_val!(self.lower_expr(x)),
                    (None, Some(eu)) => Some(self.b.ins_eu_make_ok(eu, None)),
                    (None, None) => None,
                };
                self.emit_return(v)?;
                Ok(Flow::Diverged)
            }
            SyntaxKind::BreakExpr => {
                let d = BreakExpr::cast(e).expect("kind");
                if self.loops.is_empty() {
                    return Err(refuse("break outside a loop", e.span));
                }
                // The carried value (if any) is formed BEFORE the
                // defer chains of the scopes being left run.
                let val = match d.value() {
                    Some(x) => flow_val!(self.lower_expr(x)),
                    None => None,
                };
                let depth = self.loops.last().expect("frame").depth;
                if !self.run_exits(depth, false)? {
                    return Ok(Flow::Diverged);
                }
                // The exit block is created lazily so a break-less
                // `loop` has no unreachable exit.
                let exit = match self.loops.last().expect("frame").exit {
                    Some(x) => x,
                    None => {
                        let x = self.b.create_block();
                        self.loops.last_mut().expect("frame").exit = Some(x);
                        x
                    }
                };
                let param = self.loops.last().expect("frame").exit_param;
                match (val, param) {
                    (Some(v), None) => {
                        let ty = self.b.func.value_ty(v);
                        let p = self.b.add_block_param(exit, ty);
                        self.loops.last_mut().expect("frame").exit_param = Some(p);
                        self.b.ins_jmp(exit, &[v]);
                    }
                    (Some(v), Some(_)) => self.b.ins_jmp(exit, &[v]),
                    (None, None) => self.b.ins_jmp(exit, &[]),
                    (None, Some(_)) => {
                        return Err(refuse(
                            "mixed break arities in one loop (checker contract)",
                            e.span,
                        ));
                    }
                }
                Ok(Flow::Diverged)
            }
            SyntaxKind::ContinueExpr => {
                let Some(frame) = self.loops.last() else {
                    return Err(refuse("continue outside a loop", e.span));
                };
                let depth = frame.depth;
                if !self.run_exits(depth, false)? {
                    return Ok(Flow::Diverged);
                }
                let target = self.continue_target();
                self.b.ins_jmp(target, &[]);
                Ok(Flow::Diverged)
            }
            SyntaxKind::TryExpr => self.lower_try(e),
            SyntaxKind::ElseExpr => self.lower_else(e, want),
            SyntaxKind::MatchExpr => self.lower_match(e, want),
            SyntaxKind::ForExpr => self.lower_for(e),
            SyntaxKind::StringExpr => self.lower_string(e),
            SyntaxKind::StructLit => self.lower_struct_lit(e),
            SyntaxKind::TupleExpr => self.lower_tuple(e),
            SyntaxKind::MemberExpr => self.lower_member(e),
            SyntaxKind::BracketApply => Err(refuse(
                "indexing lowering (List/Pool runtime shapes, c06)",
                e.span,
            )),
            SyntaxKind::RangeExpr | SyntaxKind::FromEndExpr => Err(refuse(
                "range VALUES outside `for` headers (owned `Iter[int]` ranges, c06/std)",
                e.span,
            )),
            SyntaxKind::RegionBlock => self.lower_region_block(e, want),
            SyntaxKind::InBlock => self.lower_in_block(e, want),
            SyntaxKind::RegionValue => Err(refuse(
                "first-class region values beyond local bindings (c05)",
                e.span,
            )),
            SyntaxKind::FreezeExpr => {
                // Statement-position freeze: emit the freeze point; the
                // (region-typed) result is not a runtime value.
                if self.lower_region_init(e)?.is_some() {
                    Ok(Flow::Val(None))
                } else {
                    Err(refuse(
                        "first-class region values beyond local bindings (c05)",
                        e.span,
                    ))
                }
            }
            SyntaxKind::UnsafeBlock => {
                // The unsafe RING is static discipline — E1301 and the
                // s22 attribution facts, enforced by wolf_mem before
                // lowering ever runs. At WIR grain an unsafe block is
                // a block (s29: the C membrane's call sites live here);
                // the raw-tier OPS inside keep their own refusals.
                match wolf_ast::UnsafeBlock::cast(e).and_then(|d| d.body()) {
                    Some(b) => self.lower_block(b, want),
                    None => Err(refuse("an unsafe block without a body", e.span)),
                }
            }
            SyntaxKind::BorrowExpr => Err(refuse(
                "unsafe-tier WIR ops (deferred from s26 — see closeout)",
                e.span,
            )),
            SyntaxKind::ClosureExpr => Err(refuse("closure lowering (c05)", e.span)),
            SyntaxKind::ScopeExpr
            | SyntaxKind::SelectExpr
            | SyntaxKind::WhenExpr
            | SyntaxKind::SpawnExpr => Err(refuse("concurrency lowering (c05)", e.span)),
            SyntaxKind::InlineC | SyntaxKind::AsmExpr => Err(refuse("inline C/asm (c10)", e.span)),
            _ => Err(refuse("this expression shape in WIR lowering", e.span)),
        }
    }

    // ------------------------------------------- aggregates (s26) ----

    /// `Point { x: 1, y: 2 }` — by-value aggregate construction:
    /// initializers evaluate in SOURCE order, `agg.make` takes fields
    /// in DECLARED order (registers, never memory — small-aggregate
    /// promotion is the default story; c06 finalizes spilled layout).
    fn lower_struct_lit(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = wolf_ast::StructLit::cast(e).expect("kind");
        let Some(sema_ty) = self.expr_sema_ty(e.span) else {
            return Err(refuse("a struct literal without a recorded type", e.span));
        };
        // Resolve the struct's declared field order.
        let mut ty = sema_ty;
        let table = self.table;
        for _ in 0..32 {
            match table.kind(ty) {
                TyKind::Distinct(inner) => ty = *inner,
                _ => break,
            }
        }
        let TyKind::Nominal { module, name } = table.kind(ty) else {
            return Err(refuse("struct literals of this type (c06)", e.span));
        };
        let Some(ItemSig::Struct(ss)) = self.sigs.get(*module as usize, name) else {
            return Err(refuse("struct literals of this type (c06)", e.span));
        };
        let declared: Vec<String> = ss.fields.iter().map(|f| f.name.clone()).collect();
        // Source-order evaluation.
        let mut by_name: Vec<(String, Value)> = Vec::new();
        for fi in d.fields() {
            let Some(name_tok) = fi.name() else { continue };
            let fname = self.text(name_tok.span);
            let value = match fi.value() {
                Some(v) => flow_val!(self.lower_expr(v)),
                // `{ x }` field-init shorthand reads the same name.
                None => match self.lookup(&fname) {
                    Some(LocalBind::Val { var, .. }) => Some(self.b.use_var(var)),
                    Some(LocalBind::MutRef {
                        ptr, region, elem, ..
                    }) => Some(self.read_mut_ref(ptr, region, elem, fi.syntax().span)?),
                    _ => {
                        return Err(refuse("this field-init shorthand", fi.syntax().span));
                    }
                },
            };
            let Some(value) = value else {
                return Err(refuse("unit-typed struct fields", fi.syntax().span));
            };
            by_name.push((fname, value));
        }
        let Some(wty) = wir_ty(
            &mut self.b.module.types,
            self.table,
            self.sigs,
            sema_ty,
            e.span,
        )?
        else {
            // Zero-field struct: unit-shaped, no value.
            return Ok(Flow::Val(None));
        };
        let mut parts = Vec::with_capacity(declared.len());
        for fname in &declared {
            let Some((_, v)) = by_name.iter().find(|(n, _)| n == fname) else {
                return Err(refuse(
                    "struct literals with defaulted fields (c06)",
                    e.span,
                ));
            };
            parts.push(*v);
        }
        Ok(Flow::Val(Some(
            self.b.ins(Opcode::AggMake, &parts, &[wty], Aux::None).one(),
        )))
    }

    /// `(a, b, c)` — by-value tuple construction.
    fn lower_tuple(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = wolf_ast::TupleExpr::cast(e).expect("kind");
        let mut parts = Vec::new();
        for el in d.elems() {
            let Some(v) = flow_val!(self.lower_expr(el)) else {
                return Err(refuse("unit-typed tuple elements", el.span));
            };
            parts.push(v);
        }
        if parts.is_empty() {
            return Ok(Flow::Val(None));
        }
        let Some(sema_ty) = self.expr_sema_ty(e.span) else {
            return Err(refuse("a tuple without a recorded type", e.span));
        };
        let Some(wty) = wir_ty(
            &mut self.b.module.types,
            self.table,
            self.sigs,
            sema_ty,
            e.span,
        )?
        else {
            return Ok(Flow::Val(None));
        };
        Ok(Flow::Val(Some(
            self.b.ins(Opcode::AggMake, &parts, &[wty], Aux::None).one(),
        )))
    }

    /// `x.f` / `t.0` — by-value field extraction (`agg.get`).
    fn lower_member(&mut self, e: &'t GreenNode) -> R<Flow> {
        let m = wolf_ast::MemberExpr::cast(e).expect("kind");
        let Some(base) = m.base() else {
            return Ok(Flow::Val(None));
        };
        let Some(base_sema) = self.expr_sema_ty(base.span) else {
            return Err(refuse("a member access without a recorded type", e.span));
        };
        let (index, ..) = self.member_index(base_sema, m, e.span)?;
        let Some(agg) = flow_val!(self.lower_expr(base)) else {
            return Err(refuse("member access on a valueless expression", e.span));
        };
        let agg_ty = self.b.func.value_ty(agg);
        let types::TypeData::Agg(fields) = self.b.module.types.get(agg_ty).clone() else {
            return Err(refuse("member access on non-aggregates (c06/std)", e.span));
        };
        let Some(&fty) = fields.get(index) else {
            return Err(refuse("a member the aggregate does not carry", e.span));
        };
        Ok(Flow::Val(Some(
            self.b
                .ins(Opcode::AggGet, &[agg], &[fty], Aux::Int(index as i64))
                .one(),
        )))
    }

    // ----------------------------------------------- regions (s26) ----

    /// `region name? { body }` — X4 block sugar: `region.new`, the
    /// body with the name bound, `region.free` wholesale at the end
    /// (the s19 frame-local free point). Early exits across the
    /// boundary need s27's defer machinery and refuse honestly.
    fn lower_region_block(&mut self, e: &'t GreenNode, want: bool) -> R<Flow> {
        let d = wolf_ast::RegionBlock::cast(e).expect("kind");
        let Some(body) = d.body() else {
            return Ok(Flow::Val(None));
        };
        let (region, handle) = self.b.ins_region_new();
        // The name binding lives in a thin wrapper scope; the
        // wholesale free rides the BODY scope's cleanup chain (X4: the
        // sugar's free is an ordinary defer entry, so every exit edge
        // — fall-through, `return`, `?`-err, `break`/`continue`
        // crossing the boundary — frees in the right LIFO position;
        // [mem.region.intra.2]).
        self.scopes.push(ScopeFrame::default());
        if let Some(name_tok) = d.name() {
            let name = self.text(name_tok.span);
            self.scopes.last_mut().expect("scope").binds.push((
                name,
                LocalBind::Region {
                    region,
                    handle,
                    frozen: false,
                },
            ));
        }
        let out = self.lower_block_in(body, want, Some((region, handle)));
        self.scopes.pop();
        out
    }

    /// `in r { body }` — open a region value for ambient placement.
    /// Allocation sites that target the ambient region arrive with the
    /// s27 container surface; scalar/aggregate work is register-
    /// resident, so opening is compile-time bookkeeping today.
    fn lower_in_block(&mut self, e: &'t GreenNode, want: bool) -> R<Flow> {
        let d = wolf_ast::InBlock::cast(e).expect("kind");
        let Some(region_expr) = d.region() else {
            return Err(refuse("an `in` block without a region", e.span));
        };
        let (_region, _handle) = self.expect_region(region_expr)?;
        let Some(body) = d.body() else {
            return Ok(Flow::Val(None));
        };
        self.lower_block(body, want)
    }

    fn lower_literal(&mut self, e: &'t GreenNode) -> R<Flow> {
        let text = self.text(e.span);
        if text == "true" {
            return Ok(Flow::Val(Some(self.b.bconst(true))));
        }
        if text == "false" {
            return Ok(Flow::Val(Some(self.b.bconst(false))));
        }
        let Some(sema_ty) = self.expr_sema_ty(e.span) else {
            return Err(refuse("a literal without a recorded type", e.span));
        };
        let Some(wty) = wir_ty(
            &mut self.b.module.types,
            self.table,
            self.sigs,
            sema_ty,
            e.span,
        )?
        else {
            return Err(refuse("a unit-typed literal", e.span));
        };
        if wty == types::F32 || wty == types::F64 {
            let Some(v) = parse_float_literal(&text) else {
                return Err(refuse("this float literal shape", e.span));
            };
            let bits = if wty == types::F32 {
                (v as f32).to_bits() as u64
            } else {
                v.to_bits()
            };
            return Ok(Flow::Val(Some(self.b.fconst(wty, bits))));
        }
        if sema_unsigned(self.table, sema_ty) {
            // Unsigned literals are BIT PATTERNS on the same-width
            // scalar (the s26 decision): parse as u64, wrap to the
            // width's signed payload (`255` as u8 prints `iconst.i8 -1`
            // — bit-honest, byte-stable).
            let Some(bits) = parse_uint_literal(&text) else {
                return Err(refuse("this literal shape in WIR lowering", e.span));
            };
            let Some(width) = self.b.module.types.int_bits(wty) else {
                return Err(refuse("a non-integer unsigned literal", e.span));
            };
            if width < 64 && bits >= (1u64 << width) {
                return Err(refuse(
                    "an unsigned literal beyond its type's width",
                    e.span,
                ));
            }
            let payload = wrap_bits(bits, width);
            return Ok(Flow::Val(Some(self.b.iconst(wty, payload))));
        }
        let Some(n) = parse_int_literal(&text) else {
            return Err(refuse("this literal shape in WIR lowering", e.span));
        };
        Ok(Flow::Val(Some(self.b.iconst(wty, n))))
    }

    fn lower_prefix(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = PrefixExpr::cast(e).expect("kind");
        let Some(operand) = d.operand() else {
            return Ok(Flow::Val(None));
        };
        match d.op().map(|t| t.kind) {
            Some(SyntaxKind::Minus) => {
                let v = flow_val!(self.lower_expr(operand));
                let Some(v) = v else {
                    return Err(refuse("negation of a valueless expression", e.span));
                };
                let ty = self.b.func.value_ty(v);
                if ty == types::F32 || ty == types::F64 {
                    return Ok(Flow::Val(Some(
                        self.b.ins(Opcode::Fneg, &[v], &[ty], Aux::None).one(),
                    )));
                }
                // A constant operand folds directly (no dead zero
                // const left behind by the speculative 0 - x shape).
                if let Some(n) = self.b.as_int_const(v) {
                    self.b.stats.fold += 1;
                    let neg = -(n as i128);
                    let (lo, hi) = self
                        .b
                        .module
                        .types
                        .int_bounds(ty)
                        .expect("negation folds on integers");
                    return if neg < lo || neg > hi {
                        // Negating MIN traps in checked arithmetic (X3).
                        self.b.ins_trap(TrapKind::Overflow);
                        Ok(Flow::Diverged)
                    } else {
                        Ok(Flow::Val(Some(self.b.iconst(ty, neg as i64))))
                    };
                }
                let zero = self.b.iconst(ty, 0);
                match self.b.ins(Opcode::IsubChk, &[zero, v], &[ty], Aux::None) {
                    InsOut::Vals(r) => Ok(Flow::Val(Some(r[0]))),
                    InsOut::Trapped => Ok(Flow::Diverged),
                }
            }
            Some(SyntaxKind::Not) => {
                let v = flow_val!(self.lower_expr(operand));
                let Some(v) = v else {
                    return Err(refuse("`!` of a valueless expression", e.span));
                };
                if let Some(c) = self.b.as_bool_const(v) {
                    self.b.stats.fold += 1;
                    return Ok(Flow::Val(Some(self.b.bconst(!c))));
                }
                // No boolean-not op exists: route through a two-edge
                // branch into one merge param (br const → jmp keeps
                // this clean when the operand later folds).
                let t = self.b.bconst(true);
                let f = self.b.bconst(false);
                let merge = self.b.create_block();
                let out = self.b.add_block_param(merge, types::BOOL);
                self.b.ins_br(v, merge, &[f], merge, &[t]);
                self.b.seal_block(merge);
                self.b.switch_to_block(merge);
                Ok(Flow::Val(Some(out)))
            }
            Some(SyntaxKind::CopyKw) | Some(SyntaxKind::MoveKw) => self.lower_expr(operand),
            Some(SyntaxKind::SharedKw) => Err(refuse(
                "shared-cell surface lowering (rc.* receivers, s27)",
                e.span,
            )),
            Some(SyntaxKind::Amp) | Some(SyntaxKind::Star) => Err(refuse(
                "borrow/deref lowering (unsafe-tier WIR ops, deferred from s26 — see closeout)",
                e.span,
            )),
            _ => self.lower_expr(operand),
        }
    }

    fn lower_bin(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = wolf_ast::BinExpr::cast(e).expect("kind");
        let op = d.op().map(|t| t.kind);
        if matches!(op, Some(SyntaxKind::AmpAmp | SyntaxKind::PipePipe)) {
            return self.lower_short_circuit(d, op == Some(SyntaxKind::AmpAmp), e.span);
        }
        let lhs = match d.lhs() {
            Some(l) => flow_val!(self.lower_expr(l)),
            None => None,
        };
        let rhs = match d.rhs() {
            Some(r) => flow_val!(self.lower_expr(r)),
            None => None,
        };
        let (Some(a), Some(bv)) = (lhs, rhs) else {
            return Err(refuse("operator on valueless operands", e.span));
        };
        let Some(op) = op else {
            return Ok(Flow::Val(Some(a)));
        };
        match op {
            SyntaxKind::Plus
            | SyntaxKind::Minus
            | SyntaxKind::Star
            | SyntaxKind::Slash
            | SyntaxKind::Percent
            | SyntaxKind::Amp
            | SyntaxKind::Pipe
            | SyntaxKind::Caret
            | SyntaxKind::Shl
            | SyntaxKind::Shr => {
                let Some(sema_ty) = self.expr_sema_ty(e.span) else {
                    return Err(refuse("an operator without a recorded type", e.span));
                };
                let Some(wty) = wir_ty(
                    &mut self.b.module.types,
                    self.table,
                    self.sigs,
                    sema_ty,
                    e.span,
                )?
                else {
                    return Err(refuse("a unit-typed operator", e.span));
                };
                let wrapping = matches!(self.table.kind(sema_ty), TyKind::Wrapping(_));
                let unsigned = sema_unsigned(self.table, sema_ty);
                match self.arith(op, a, bv, wrapping, unsigned, wty, e.span)? {
                    Some(v) => Ok(Flow::Val(Some(v))),
                    None => Ok(Flow::Diverged),
                }
            }
            SyntaxKind::EqEq
            | SyntaxKind::NotEq
            | SyntaxKind::Lt
            | SyntaxKind::Gt
            | SyntaxKind::LtEq
            | SyntaxKind::GtEq => {
                let ty = self.b.func.value_ty(a);
                if ty == types::F32 || ty == types::F64 {
                    let cc = match op {
                        SyntaxKind::EqEq => FloatCc::Eq,
                        SyntaxKind::NotEq => FloatCc::Ne,
                        SyntaxKind::Lt => FloatCc::Lt,
                        SyntaxKind::Gt => FloatCc::Gt,
                        SyntaxKind::LtEq => FloatCc::Le,
                        _ => FloatCc::Ge,
                    };
                    return Ok(Flow::Val(Some(
                        self.b
                            .ins(Opcode::Fcmp, &[a, bv], &[types::BOOL], Aux::FloatCc(cc))
                            .one(),
                    )));
                }
                if !types_is_int(ty) {
                    return Err(refuse(
                        "comparison outside integers/floats (str/enum compares, c06/std)",
                        e.span,
                    ));
                }
                // Order is a property of the OPERANDS' sema type:
                // unsigned types compare with the `u*` conditions.
                let unsigned = d
                    .lhs()
                    .and_then(|l| self.expr_sema_ty(l.span))
                    .map(|t| sema_unsigned(self.table, t))
                    .unwrap_or(false);
                let cc = match (op, unsigned) {
                    (SyntaxKind::EqEq, _) => IntCc::Eq,
                    (SyntaxKind::NotEq, _) => IntCc::Ne,
                    (SyntaxKind::Lt, false) => IntCc::Slt,
                    (SyntaxKind::Gt, false) => IntCc::Sgt,
                    (SyntaxKind::LtEq, false) => IntCc::Sle,
                    (_, false) => IntCc::Sge,
                    (SyntaxKind::Lt, true) => IntCc::Ult,
                    (SyntaxKind::Gt, true) => IntCc::Ugt,
                    (SyntaxKind::LtEq, true) => IntCc::Ule,
                    (_, true) => IntCc::Uge,
                };
                Ok(Flow::Val(Some(
                    self.b
                        .ins(Opcode::Icmp, &[a, bv], &[types::BOOL], Aux::IntCc(cc))
                        .one(),
                )))
            }
            _ => Err(refuse("this operator in WIR lowering", e.span)),
        }
    }

    /// Arithmetic/bitwise emission: X3 checked by default, `.wrap` for
    /// `wrapping[T]`-typed expressions, the `u*.chk` family for
    /// unsigned types (the s26 op-set decision), float ops for floats.
    /// `None` means the op provably trapped.
    #[allow(clippy::too_many_arguments)]
    fn arith(
        &mut self,
        op: SyntaxKind,
        a: Value,
        b: Value,
        wrapping: bool,
        unsigned: bool,
        wty: TypeId,
        span: Span,
    ) -> R<Option<Value>> {
        if wty == types::F32 || wty == types::F64 {
            let fop = match op {
                SyntaxKind::Plus => Opcode::Fadd,
                SyntaxKind::Minus => Opcode::Fsub,
                SyntaxKind::Star => Opcode::Fmul,
                SyntaxKind::Slash => Opcode::Fdiv,
                _ => return Err(refuse("this float operator (no frem op)", span)),
            };
            return Ok(Some(self.b.ins(fop, &[a, b], &[wty], Aux::None).one()));
        }
        // Wrapping arithmetic is sign-agnostic (two's complement), so
        // `wrapping[uN]` shares the `.wrap` ops with the signed forms.
        let iop = match (op, wrapping, unsigned) {
            (SyntaxKind::Plus, false, false) => Opcode::IaddChk,
            (SyntaxKind::Minus, false, false) => Opcode::IsubChk,
            (SyntaxKind::Star, false, false) => Opcode::ImulChk,
            (SyntaxKind::Slash, false, false) => Opcode::IdivChk,
            (SyntaxKind::Percent, false, false) => Opcode::IremChk,
            (SyntaxKind::Plus, false, true) => Opcode::UaddChk,
            (SyntaxKind::Minus, false, true) => Opcode::UsubChk,
            (SyntaxKind::Star, false, true) => Opcode::UmulChk,
            (SyntaxKind::Slash, false, true) => Opcode::UdivChk,
            (SyntaxKind::Percent, false, true) => Opcode::UremChk,
            (SyntaxKind::Plus, true, _) => Opcode::IaddWrap,
            (SyntaxKind::Minus, true, _) => Opcode::IsubWrap,
            (SyntaxKind::Star, true, _) => Opcode::ImulWrap,
            (SyntaxKind::Slash | SyntaxKind::Percent, true, _) => {
                return Err(refuse("wrapping division (no idiv.wrap op)", span));
            }
            (SyntaxKind::Amp, ..) => Opcode::Band,
            (SyntaxKind::Pipe, ..) => Opcode::Bor,
            (SyntaxKind::Caret, ..) => Opcode::Bxor,
            (SyntaxKind::Shl, ..) => Opcode::Shl,
            (SyntaxKind::Shr, ..) => {
                if unsigned {
                    Opcode::Lshr
                } else {
                    Opcode::Ashr
                }
            }
            _ => return Err(refuse("this operator in WIR lowering", span)),
        };
        let out = match self.b.ins(iop, &[a, b], &[wty], Aux::None) {
            InsOut::Vals(r) => r[0],
            InsOut::Trapped => return Ok(None),
        };
        // X3's claw-back, the locally-checkable slice: a no-trap
        // remainder by a positive constant carries its postcondition
        // range as a verified fact (`: op` — the verifier re-derives
        // it from the defining op; peephole rewrites keep it sound,
        // since the fact rides the SURVIVING value's own def).
        if matches!(iop, Opcode::IremChk | Opcode::UremChk)
            && let Some(c) = self.b.as_int_const(b)
            && c > 0
            && matches!(
                self.b.func.values[out].def,
                crate::ir::ValueDef::Result(di, 0)
                    if matches!(
                        self.b.func.insts[di].op,
                        Opcode::IremChk | Opcode::UremChk | Opcode::Iconst
                    )
            )
        {
            let (lo, hi) = if iop == Opcode::UremChk {
                (0, c as i128 - 1)
            } else {
                (-(c as i128 - 1), c as i128 - 1)
            };
            let kind = FactKind::Range(out, lo, hi);
            let dup = self.b.func.facts.values().any(|fd| fd.kind == kind);
            if !dup {
                self.b.func.add_fact(FactData::new(kind, Just::DefOp));
            }
        }
        Ok(Some(out))
    }

    fn lower_short_circuit(
        &mut self,
        d: wolf_ast::BinExpr<'t>,
        is_and: bool,
        span: Span,
    ) -> R<Flow> {
        let lhs = match d.lhs() {
            Some(l) => flow_val!(self.lower_expr(l)),
            None => None,
        };
        let Some(lhs) = lhs else {
            return Err(refuse("logic on a valueless operand", span));
        };
        let Some(rhs_expr) = d.rhs() else {
            return Ok(Flow::Val(Some(lhs)));
        };
        if let Some(c) = self.b.as_bool_const(lhs) {
            // Branch-on-const at the walker: the not-taken side is
            // never lowered (dead code costs nothing, and no
            // unreachable block is created).
            self.b.stats.identity += 1;
            if c == is_and {
                return self.lower_expr(rhs_expr);
            }
            return Ok(Flow::Val(Some(self.b.bconst(!is_and))));
        }
        let short = self.b.bconst(!is_and);
        let rhs_bb = self.b.create_block();
        let merge = self.b.create_block();
        let out = self.b.add_block_param(merge, types::BOOL);
        if is_and {
            self.b.ins_br(lhs, rhs_bb, &[], merge, &[short]);
        } else {
            self.b.ins_br(lhs, merge, &[short], rhs_bb, &[]);
        }
        self.b.seal_block(rhs_bb);
        self.b.switch_to_block(rhs_bb);
        self.b.gvn_push_scope();
        let rhs = self.lower_expr(rhs_expr);
        let diverged = match rhs {
            Ok(Flow::Val(Some(v))) => {
                self.b.ins_jmp(merge, &[v]);
                false
            }
            Ok(Flow::Val(None)) => {
                self.b.gvn_pop_scope();
                return Err(refuse("logic on a valueless operand", span));
            }
            Ok(Flow::Diverged) => true,
            Err(e) => {
                self.b.gvn_pop_scope();
                return Err(e);
            }
        };
        let _ = diverged;
        self.b.gvn_pop_scope();
        self.b.seal_block(merge);
        self.b.switch_to_block(merge);
        Ok(Flow::Val(Some(out)))
    }

    fn lower_cast(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = CastExpr::cast(e).expect("kind");
        let Some(inner) = d.expr() else {
            return Ok(Flow::Val(None));
        };
        let v = flow_val!(self.lower_expr(inner));
        let Some((from, to, kind)) = self.casts.get(&e.span).copied() else {
            // No recorded cast: sema treated it as identity.
            return Ok(Flow::Val(v));
        };
        match kind {
            CastKind::Identity | CastKind::Adapter => Ok(Flow::Val(v)),
            CastKind::Numeric => {
                let Some(v) = v else {
                    return Err(refuse("cast of a valueless expression", e.span));
                };
                let src = wir_ty(
                    &mut self.b.module.types,
                    self.table,
                    self.sigs,
                    from,
                    e.span,
                )?;
                let dst = wir_ty(&mut self.b.module.types, self.table, self.sigs, to, e.span)?;
                let (Some(src), Some(dst)) = (src, dst) else {
                    return Err(refuse("cast on unit types", e.span));
                };
                if src == dst {
                    return Ok(Flow::Val(Some(v)));
                }
                match (int_bits(src), int_bits(dst)) {
                    (Some(fb), Some(tb)) if tb > fb => {
                        // Widening extends by the SOURCE's signedness
                        // (unsigned zero-extends — the s26 decision).
                        let op = if sema_unsigned(self.table, from) {
                            Opcode::Zext
                        } else {
                            Opcode::Sext
                        };
                        Ok(Flow::Val(Some(
                            self.b.ins(op, &[v], &[dst], Aux::None).one(),
                        )))
                    }
                    (Some(_), Some(_)) => Err(refuse(
                        "narrowing numeric casts (range-check semantics, s27)",
                        e.span,
                    )),
                    _ => Err(refuse("int↔float casts (no conversion op yet)", e.span)),
                }
            }
            CastKind::Raw => Err(refuse(
                "raw-pointer casts (unsafe-tier WIR ops, deferred from s26 — see closeout)",
                e.span,
            )),
        }
    }

    fn lower_if(&mut self, e: &'t GreenNode, want: bool) -> R<Flow> {
        let d = IfExpr::cast(e).expect("kind");
        let cond = match d.condition() {
            Some(c) => flow_val!(self.lower_expr(c)),
            None => None,
        };
        let Some(cond) = cond else {
            return Err(refuse("an if without a condition value", e.span));
        };
        // Does this if produce a value? Sema's recorded type decides
        // when present; else-if chains inherit the caller's demand
        // (their nested node may carry no recorded type of its own).
        let want_v = match self.expr_sema_ty(e.span) {
            Some(t) => {
                wir_ty(&mut self.b.module.types, self.table, self.sigs, t, e.span)?.is_some()
            }
            None => want,
        };
        // A fallible-typed if (its recorded type is a tagged union):
        // arms may mix raises and plain ok values; both coerce into
        // the eu pair at the merge (ok-injection is `eu.make.ok`).
        let merge_eu = self.eu_ty_of_span(e.span)?;
        if let Some(c) = self.b.as_bool_const(cond) {
            // Branch-on-const: lower only the taken arm.
            self.b.stats.identity += 1;
            let flow = if c {
                match d.then_block() {
                    Some(tb) => self.lower_block(tb, want_v),
                    None => Ok(Flow::Val(None)),
                }
            } else {
                match d.else_branch() {
                    Some(el) if el.kind == SyntaxKind::Block => {
                        self.lower_block(AstBlock::cast(el).expect("kind"), want_v)
                    }
                    Some(el) => self.lower_expr_w(el, want_v),
                    None => Ok(Flow::Val(None)),
                }
            }?;
            return match flow {
                Flow::Val(v) if want_v => Ok(Flow::Val(self.arm_to_merge(v, merge_eu, e.span)?)),
                f => Ok(f),
            };
        }
        let then_bb = self.b.create_block();
        let else_bb = self.b.create_block();
        self.b.ins_br(cond, then_bb, &[], else_bb, &[]);
        self.b.seal_block(then_bb);
        self.b.seal_block(else_bb);
        // Then arm.
        self.b.switch_to_block(then_bb);
        self.b.gvn_push_scope();
        let then_flow = match d.then_block() {
            Some(tb) => self.lower_block(tb, want_v),
            None => Ok(Flow::Val(None)),
        };
        let then_out = match then_flow {
            Ok(f) => f,
            Err(e) => {
                self.b.gvn_pop_scope();
                return Err(e);
            }
        };
        let then_end = self.b.current_block();
        self.b.gvn_pop_scope();
        // Else arm.
        self.b.switch_to_block(else_bb);
        self.b.gvn_push_scope();
        let else_flow = match d.else_branch() {
            Some(el) if el.kind == SyntaxKind::Block => {
                self.lower_block(AstBlock::cast(el).expect("kind"), want_v)
            }
            Some(el) => self.lower_expr_w(el, want_v),
            None => Ok(Flow::Val(None)),
        };
        let else_out = match else_flow {
            Ok(f) => f,
            Err(e) => {
                self.b.gvn_pop_scope();
                return Err(e);
            }
        };
        let else_end = self.b.current_block();
        self.b.gvn_pop_scope();
        // Join.
        match (then_out, else_out) {
            (Flow::Diverged, Flow::Diverged) => Ok(Flow::Diverged),
            (Flow::Val(v), Flow::Diverged) => {
                // Continue in the then arm's end block.
                self.b.switch_to_block(then_end);
                let v = if want_v {
                    self.arm_to_merge(v, merge_eu, e.span)?
                } else {
                    v
                };
                Ok(Flow::Val(v))
            }
            (Flow::Diverged, Flow::Val(v)) => {
                self.b.switch_to_block(else_end);
                let v = if want_v {
                    self.arm_to_merge(v, merge_eu, e.span)?
                } else {
                    v
                };
                Ok(Flow::Val(v))
            }
            (Flow::Val(tv), Flow::Val(ev)) => {
                // Coerce each arm's value into a fallible merge type
                // in that arm's own end block (may branch for row
                // widening).
                self.b.switch_to_block(then_end);
                let tv = if want_v {
                    self.arm_to_merge(tv, merge_eu, e.span)?
                } else {
                    tv
                };
                let then_end = self.b.current_block();
                self.b.switch_to_block(else_end);
                let ev = if want_v {
                    self.arm_to_merge(ev, merge_eu, e.span)?
                } else {
                    ev
                };
                let else_end = self.b.current_block();
                let merge = self.b.create_block();
                // The result rides a merge parameter, typed off the
                // arm value; equal arm values need no parameter (the
                // trivial-param test, applied before the param exists).
                let (param, targs, eargs): (Option<Value>, Vec<Value>, Vec<Value>) =
                    match (want_v, tv, ev) {
                        (true, Some(a), Some(b)) if a != b => {
                            let ty = self.b.func.value_ty(a);
                            let p = self.b.add_block_param(merge, ty);
                            (Some(p), vec![a], vec![b])
                        }
                        (true, Some(a), Some(_)) => (Some(a), vec![], vec![]),
                        _ => (None, vec![], vec![]),
                    };
                self.b.switch_to_block(then_end);
                self.b.ins_jmp(merge, &targs);
                self.b.switch_to_block(else_end);
                self.b.ins_jmp(merge, &eargs);
                self.b.seal_block(merge);
                self.b.switch_to_block(merge);
                Ok(Flow::Val(param))
            }
        }
    }

    fn lower_while(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = WhileExpr::cast(e).expect("kind");
        let header = self.b.create_block();
        self.b.ins_jmp(header, &[]);
        self.b.switch_to_block(header);
        self.b.gvn_push_scope();
        self.loops.push(LoopFrame {
            continue_to: ContinueTo::Block(header),
            exit: None,
            exit_param: None,
            depth: self.scopes.len(),
        });
        let out = self.lower_while_inner(d, header);
        let frame = self.loops.pop().expect("frame");
        self.b.gvn_pop_scope();
        out?;
        self.finish_loop(header, frame)
    }

    fn lower_while_inner(&mut self, d: WhileExpr<'t>, header: Block) -> R<()> {
        let cond = match d.condition() {
            Some(c) => match self.lower_expr(c)? {
                Flow::Val(Some(v)) => v,
                Flow::Val(None) => {
                    return Err(refuse("a while without a condition value", d.syntax().span));
                }
                // The condition itself diverged: no loop edges beyond
                // it; the header chain is already terminated.
                Flow::Diverged => return Ok(()),
            },
            None => return Err(refuse("a while without a condition", d.syntax().span)),
        };
        match self.b.as_bool_const(cond) {
            Some(false) => {
                // The loop never runs: fall straight to the exit; the
                // body is dead code and never lowered.
                self.b.stats.identity += 1;
                let exit = self.b.create_block();
                self.loops.last_mut().expect("frame").exit = Some(exit);
                self.b.ins_jmp(exit, &[]);
            }
            Some(true) => {
                self.b.stats.identity += 1;
                let body = self.b.create_block();
                self.b.ins_jmp(body, &[]);
                self.b.seal_block(body);
                self.b.switch_to_block(body);
                self.lower_loop_body(d.body(), header)?;
            }
            None => {
                let body = self.b.create_block();
                let exit = self.b.create_block();
                self.loops.last_mut().expect("frame").exit = Some(exit);
                self.b.ins_br(cond, body, &[], exit, &[]);
                self.b.seal_block(body);
                self.b.switch_to_block(body);
                self.lower_loop_body(d.body(), header)?;
            }
        }
        Ok(())
    }

    fn lower_loop(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = LoopExpr::cast(e).expect("kind");
        let header = self.b.create_block();
        self.b.ins_jmp(header, &[]);
        self.b.switch_to_block(header);
        self.b.gvn_push_scope();
        self.loops.push(LoopFrame {
            continue_to: ContinueTo::Block(header),
            exit: None,
            exit_param: None,
            depth: self.scopes.len(),
        });
        let out = self.lower_loop_body(d.body(), header);
        let frame = self.loops.pop().expect("frame");
        self.b.gvn_pop_scope();
        out?;
        self.finish_loop(header, frame)
    }

    fn lower_loop_body(&mut self, body: Option<AstBlock<'t>>, header: Block) -> R<()> {
        self.b.gvn_push_scope();
        let flow = match body {
            Some(bl) => self.lower_block(bl, false),
            None => Ok(Flow::Val(None)),
        };
        let flow = match flow {
            Ok(f) => f,
            Err(e) => {
                self.b.gvn_pop_scope();
                return Err(e);
            }
        };
        if let Flow::Val(_) = flow {
            // The back edge.
            self.b.ins_jmp(header, &[]);
        }
        self.b.gvn_pop_scope();
        Ok(())
    }

    fn finish_loop(&mut self, header: Block, frame: LoopFrame) -> R<Flow> {
        self.b.seal_block(header);
        match frame.exit {
            Some(exit) => {
                self.b.seal_block(exit);
                self.b.switch_to_block(exit);
                // `loop { break v }` results ride the exit parameter.
                Ok(Flow::Val(frame.exit_param))
            }
            None => Ok(Flow::Diverged),
        }
    }

    /// The current loop's `continue` target; a `for` loop's latch is
    /// created on first demand (no unreachable blocks otherwise).
    fn continue_target(&mut self) -> Block {
        match self.loops.last().expect("frame").continue_to {
            ContinueTo::Block(b) => b,
            ContinueTo::ForLatch(Some(b)) => b,
            ContinueTo::ForLatch(None) => {
                let b = self.b.create_block();
                if let ContinueTo::ForLatch(l) =
                    &mut self.loops.last_mut().expect("frame").continue_to
                {
                    *l = Some(b);
                }
                b
            }
        }
    }

    // --------------------------------------------------- calls ----

    fn lower_call(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = CallExpr::cast(e).expect("kind");
        let cs: Option<&CallSig> = self.calls.get(&e.span).copied();
        // Builtins without a signature: assert / print.
        let callee_text = d.callee().map(|c| self.text(c.span)).unwrap_or_default();
        // The s38 io/fs builtin tier executes on the checked lane;
        // native lowering owes it the row-returning call ABI and (for
        // the read side) allocating str materialization — an honest
        // refusal either way, with the tier named.
        if matches!(
            callee_text.as_str(),
            "read_line"
                | "fs_read_text"
                | "fs_write_text"
                | "fs_open"
                | "fs_create"
                | "fs_read"
                | "fs_write"
                | "fs_close"
                | "fs_remove"
                | "fs_exists"
        ) {
            return Err(refuse(
                "io/fs builtins in native lowering (checked lane only at s38)",
                e.span,
            ));
        }
        // The s39 net builtin tier mirrors the fs posture: checked
        // lane executes, native lowering owes the row-returning call
        // ABI + str materialization — an honest refusal, tier named.
        if matches!(
            callee_text.as_str(),
            "net_listen"
                | "net_port"
                | "net_accept"
                | "net_connect"
                | "net_read"
                | "net_write"
                | "net_close"
        ) {
            return Err(refuse(
                "net builtins in native lowering (checked lane only at s39)",
                e.span,
            ));
        }
        if cs.is_none() {
            match callee_text.as_str() {
                "assert" => return self.lower_assert(d),
                "print" | "print_raw" | "eprint" | "eprint_raw" => {
                    let stream = if callee_text.starts_with('e') { 2 } else { 1 };
                    return self.lower_print(d, callee_text.ends_with("print"), stream);
                }
                _ => {
                    // A call-shaped tag RAISE (`Io(9)` under a row
                    // that declares Io with payloads): the callee is a
                    // bare unresolved name and sema recorded the
                    // enclosing union type on this call.
                    if let Some(callee) = d.callee()
                        && callee.kind == SyntaxKind::PathExpr
                        && self.lookup(&callee_text).is_none()
                        && let Some(eu) = self.raise_target(e.span)?
                    {
                        let mut payloads = Vec::new();
                        for a in d.args().into_iter().flat_map(|l| l.args()) {
                            let Some(vexpr) = Arg::value(a) else { continue };
                            let Some(v) = flow_val!(self.lower_expr(vexpr)) else {
                                return Err(refuse("unit-typed tag payloads", vexpr.span));
                            };
                            payloads.push(v);
                        }
                        let id = self.b.module.tag_id(&callee_text);
                        let tag = self.b.iconst(types::I64, id);
                        return Ok(Flow::Val(Some(self.b.ins_eu_make_err(eu, tag, &payloads))));
                    }
                    return Err(refuse("calls outside the resolved surface", e.span));
                }
            }
        }
        let cs = cs.expect("checked");
        if cs.c_call {
            return self.lower_c_call(d, cs, e);
        }
        if cs.ctor {
            return self.lower_ctor(d, cs, e);
        }
        if cs.has_self {
            return self.lower_method_call(d, cs, e);
        }
        if cs.decl_span.is_none() {
            return Err(refuse("indirect calls through fn values (c05)", e.span));
        }
        // Resolve the callee to its package fn. Names declared in more
        // than one module disambiguate by the declaration locus sema
        // recorded on the call (issue #26): `decl_span` names exactly
        // one signature.
        let Some(cands) = self.fns.get(cs.callee.as_str()) else {
            return Err(refuse("calls into unresolvable bodies", e.span));
        };
        let (callee_module, callee_sig) = if cands.len() == 1 {
            cands[0]
        } else {
            let hits: Vec<&(usize, &FnSig)> = cands
                .iter()
                .filter(|(_, f)| Some(f.name_span) == cs.decl_span)
                .collect();
            match hits.as_slice() {
                [one] => **one,
                _ => {
                    return Err(refuse(
                        "a same-named callee without a unique declaration locus",
                        e.span,
                    ));
                }
            }
        };
        if !callee_sig.generics.is_empty() {
            return Err(refuse("generic-function calls (monomorphization)", e.span));
        }
        if callee_sig.comptime {
            return Err(refuse("comptime calls (D29 CTFE owns these)", e.span));
        }
        // Arguments under their declared modes. A `mut` argument is
        // s25's refusal repaid: the local spills to a `stack.alloc`
        // slot (its own one-slot region — stack provenance, the s19
        // promotion landing pad), the callee gets (ptr, token), and the
        // local reloads on return. Re-lending a `mut` PARAMETER passes
        // its pointer and region straight through — no copy, and the
        // exclusivity theorem survives the hop.
        let mut args = Vec::new();
        let mut formal_regions: HashMap<u32, RegionId> = HashMap::new();
        let mut next_formal = 0u32;
        let mut writebacks: Vec<WriteBack> = Vec::new();
        let mut spilled_slots: Vec<Value> = Vec::new();
        for (i, a) in d.args().into_iter().flat_map(|l| l.args()).enumerate() {
            let mode = cs.params.get(i).and_then(|p| p.mode);
            let Some(vexpr) = Arg::value(a) else { continue };
            if mode == Some(ParamMode::Mut) {
                let formal = next_formal;
                next_formal += 1;
                match self.lower_mut_arg(vexpr)? {
                    MutArg::Spill {
                        cur,
                        size,
                        writeback,
                    } => {
                        let (slot_region, slot) = self.b.ins_stack_alloc(size);
                        // The spill slot's facts: stack provenance and
                        // its full extent, both op-derived (the s19
                        // promotion fact, realized).
                        self.b.func.add_fact(FactData::new(
                            FactKind::Region(slot, slot_region),
                            Just::DefOp,
                        ));
                        self.b.func.add_fact(FactData::new(
                            FactKind::Deref(slot, DerefSize::Const(size)),
                            Just::DefOp,
                        ));
                        self.store_flat(cur, slot, slot_region, vexpr.span)?;
                        formal_regions.insert(formal, slot_region);
                        args.push(slot);
                        spilled_slots.push(slot);
                        writebacks.push(writeback.filled(slot, slot_region, vexpr.span));
                    }
                    MutArg::Relend { ptr, region } => {
                        formal_regions.insert(formal, region);
                        args.push(ptr);
                    }
                }
                continue;
            }
            let v = flow_val!(self.lower_expr(vexpr));
            let Some(v) = v else {
                return Err(refuse("unit-typed arguments", vexpr.span));
            };
            args.push(v);
        }
        // The call-site disjointness theorem, made explicit: distinct
        // `mut` places at one call never alias ([mem.model.path.
        // disjoint] — field-granular; the checker proved it, or the
        // call would not be mem-clean).
        for (i, &a) in spilled_slots.iter().enumerate() {
            for &b in &spilled_slots[i + 1..] {
                self.b.func.add_fact(FactData::new(
                    FactKind::Noalias(a, b),
                    Just::Theorem(Theorem::ExclField),
                ));
            }
        }
        // Import the callee under its module-qualified WIR name (per-
        // function cache; the shared sig build keeps the mut-expansion
        // identical to the definition's).
        let callee_name = qualify(self.sigs, callee_module, &cs.callee);
        let ext = match self.callees.get(&callee_name) {
            Some(&ext) => ext,
            None => {
                let sig = wir_sig_of(self.b.module, self.sigs, callee_sig, e.span)?;
                let ext = self.b.func.import_func(callee_name.clone(), sig);
                self.callees.insert(callee_name, ext);
                ext
            }
        };
        let results = self.b.ins_call_regions(ext, &args, &formal_regions);
        self.run_writebacks(writebacks)?;
        Ok(Flow::Val(results.first().copied()))
    }

    /// A call through the `c.` membrane (s29): the is04-modelled five
    /// lower to WIR calls whose callee names KEEP the `c.` namespace —
    /// the membrane is nominal, and the backend maps `c.X` to the
    /// unmangled libc symbol under the SysV plan (D19). Signatures are
    /// fixed here, mirroring sema's typing (`uint`/`int` → `i64`,
    /// `*u8` → `ptr`).
    ///
    /// v0 threads NO io token through these calls: WIR never reorders
    /// calls (block instruction order is program order at every
    /// backend), and the raw loads/stores an io spine would have to
    /// order against do not lower yet (s26 deferral). The token
    /// threading joins when `print`/raw-deref lowering lands (c06 —
    /// recorded in the campaign closeout).
    fn lower_c_call(&mut self, d: CallExpr<'t>, cs: &CallSig, e: &'t GreenNode) -> R<Flow> {
        use crate::ir::Param;
        let (param_tys, ret): (Vec<TypeId>, Option<TypeId>) = match cs.callee.as_str() {
            "c.malloc" => (vec![types::I64], Some(types::PTR)),
            "c.calloc" => (vec![types::I64, types::I64], Some(types::PTR)),
            "c.free" => (vec![types::PTR], None),
            "c.memset" => (vec![types::PTR, types::I64, types::I64], Some(types::PTR)),
            "c.memcpy" => (vec![types::PTR, types::PTR, types::I64], Some(types::PTR)),
            // Sema refuses beyond the modelled set before lowering
            // runs; this is the defensive twin (c10's importer).
            _ => {
                return Err(refuse(
                    "imported C beyond the modelled intrinsic set (c10)",
                    e.span,
                ));
            }
        };
        let mut args = Vec::new();
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            let Some(vexpr) = Arg::value(a) else { continue };
            let Some(v) = flow_val!(self.lower_expr(vexpr)) else {
                return Err(refuse("unit-typed arguments", vexpr.span));
            };
            args.push(v);
        }
        if args.len() != param_tys.len() {
            return Err(refuse("a C call with the wrong arity", e.span));
        }
        let ext = match self.callees.get(&cs.callee) {
            Some(&ext) => ext,
            None => {
                let params = param_tys.iter().map(|&ty| Param::val(ty)).collect();
                let sig = self.b.module.make_sig(params, ret.into_iter().collect());
                let ext = self.b.func.import_func(cs.callee.clone(), sig);
                self.callees.insert(cs.callee.clone(), ext);
                ext
            }
        };
        let results = self.b.ins_call(ext, &args);
        Ok(Flow::Val(results.first().copied()))
    }

    /// A string episode in VALUE position (s31): a literal-only string
    /// becomes a `{ptr, len}` pair over module data. An interpolated
    /// string in value position needs allocation to materialize —
    /// refused until the str runtime tier (c08); `print` consumes
    /// interpolation directly as segment writes ([`Self::lower_print`]).
    fn lower_string(&mut self, e: &'t GreenNode) -> R<Flow> {
        let mut bytes: Vec<u8> = Vec::new();
        for seg in self.string_segments(e) {
            match seg {
                StrSeg::Lit(b) => bytes.extend_from_slice(&b),
                StrSeg::Hole { .. } => {
                    return Err(refuse(
                        "interpolated strings in value position (allocating \
                         materialization, c08)",
                        e.span,
                    ));
                }
            }
        }
        Ok(Flow::Val(Some(self.str_value(&bytes))))
    }

    /// Build the `{ptr, len}` value of a byte-literal string: intern
    /// the bytes as module data, take their address, pair with the
    /// length. Zero-length literals intern one NUL byte (a zero-size
    /// data symbol is degenerate) but keep len 0.
    fn str_value(&mut self, bytes: &[u8]) -> Value {
        let idx = self
            .b
            .module
            .intern_data(if bytes.is_empty() { &[0u8] } else { bytes });
        let p = self.b.ins_data_addr(idx);
        let len = self.b.iconst(types::I64, bytes.len() as i64);
        let sty = str_ty(self.b.types());
        self.b
            .ins(Opcode::AggMake, &[p, len], &[sty], Aux::None)
            .one()
    }

    /// Decode one string episode into literal chunks and interpolation
    /// holes — the SAME algorithm as the reference interpreter's
    /// `eval_string` (naive quote strip, its escape set, hole spans =
    /// whole `Interp` nodes so format specs are consumed with the hole
    /// and ignored at v0).
    fn string_segments(&self, e: &'t GreenNode) -> Vec<StrSeg<'t>> {
        let d = StringExpr::cast(e).expect("kind");
        let raw = self.text(e.span);
        let base = e.span.lo;
        #[allow(clippy::type_complexity)]
        let mut holes: Vec<(u32, u32, Option<(&'t GreenNode, Option<&'t GreenNode>)>)> = Vec::new();
        for i in d.interps() {
            let ispan = i.syntax().span;
            holes.push((
                ispan.lo - base,
                ispan.hi - base,
                i.expr().map(|e| (e, i.format_spec())),
            ));
        }
        let bytes = raw.as_bytes();
        // `"""` multiline literals dedent by the closing delimiter's
        // column (D26) — hole-free only; `lower_print` refuses the
        // combination before segments are asked for. Byte-identical
        // with the checked executor's path.
        if bytes.starts_with(b"\"\"\"") && holes.iter().all(|(_, _, h)| h.is_none()) {
            let inner = &bytes[3..bytes.len().saturating_sub(3).max(3)];
            let lit = decode_escapes(&dedent_multiline(inner));
            if lit.is_empty() {
                return Vec::new();
            }
            return vec![StrSeg::Lit(lit)];
        }
        let (start, end) = if bytes.len() >= 2 {
            (1usize, bytes.len() - 1)
        } else {
            (0, bytes.len())
        };
        let mut segs: Vec<StrSeg<'t>> = Vec::new();
        let mut lit: Vec<u8> = Vec::new();
        let mut i = start;
        while i < end {
            if let Some(&(_, hi, hole)) = holes.iter().find(|(lo, _, _)| *lo as usize == i) {
                if let Some((expr, spec)) = hole {
                    if !lit.is_empty() {
                        segs.push(StrSeg::Lit(std::mem::take(&mut lit)));
                    }
                    segs.push(StrSeg::Hole { expr, spec });
                }
                i = hi as usize;
                continue;
            }
            let c = bytes[i];
            if c == b'\\' && i + 1 < end {
                // Code-point escapes (`\xNN`, `\u{…}`) — kept
                // byte-identical with the checked executor's decoder
                // (s37; a divergence here prints different bytes).
                if let Some((ch, consumed)) = decode_codepoint_escape(&bytes[i..end]) {
                    let mut buf = [0u8; 4];
                    lit.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                    i += consumed;
                    continue;
                }
                lit.push(match bytes[i + 1] {
                    b'n' => b'\n',
                    b't' => b'\t',
                    b'r' => b'\r',
                    b'0' => 0,
                    other => other, // \\ \" \{ \} and unknown: the char itself
                });
                i += 2;
                continue;
            }
            // `{{` / `}}` are literal braces ([gram.lex.str]).
            if (c == b'{' || c == b'}') && i + 1 < end && bytes[i + 1] == c {
                lit.push(c);
                i += 2;
                continue;
            }
            lit.push(c);
            i += 1;
        }
        if !lit.is_empty() {
            segs.push(StrSeg::Lit(lit));
        }
        segs
    }

    /// Classify one evaluated print value by its sema type: str values
    /// print as bytes, integers widen to i64 per signedness, bools
    /// print `true`/`false`, `f64` renders shortest-round-trip (s38 —
    /// the checked executor is the reference, byte for byte). `f32`
    /// still refuses (its rounding story is unruled). `spec` is the
    /// packed format spec, `0` for none.
    fn classify_print_value(&mut self, expr: &'t GreenNode, v: Value, spec: i64) -> R<PrintSeg> {
        let Some(&sema) = self.expr_tys.get(&expr.span) else {
            return Err(refuse("print of an untyped expression", expr.span));
        };
        let mut ty = sema;
        while let TyKind::Wrapping(inner) | TyKind::Distinct(inner) = self.table.kind(ty) {
            ty = *inner;
        }
        match self.table.kind(ty) {
            TyKind::Prim(Prim::Str) => Ok(PrintSeg::Str { v, spec }),
            TyKind::Prim(Prim::Bool) => Ok(PrintSeg::Bool { v, spec }),
            TyKind::Prim(Prim::F64) => Ok(PrintSeg::F64 { v, spec }),
            TyKind::Prim(Prim::F32) => Err(refuse(
                "`f32` print formatting (f64 is the s38 float)",
                expr.span,
            )),
            TyKind::Prim(_) => {
                let unsigned = sema_unsigned(self.table, ty);
                let spec = if unsigned && spec != 0 {
                    spec | wolf_sema::fmtspec::PACK_UNSIGNED
                } else {
                    spec
                };
                Ok(PrintSeg::Int { v, unsigned, spec })
            }
            _ => Err(refuse(
                "print of a non-primitive value (s16/D26)",
                expr.span,
            )),
        }
    }

    /// Parse a hole's format spec into its packed form (s38): the
    /// spec is comptime-known, sema already diagnosed malformed and
    /// mismatched ones (E0412/E0413), so lowering only refuses what
    /// has no pinned semantics — the computed spec (`{x:{w}}`).
    fn packed_spec(&mut self, spec: Option<&'t GreenNode>) -> R<i64> {
        let Some(node) = spec else { return Ok(0) };
        if node.nodes().any(|n| n.kind == SyntaxKind::Interp) {
            return Err(refuse("a computed format spec (s38 formatting)", node.span));
        }
        let text = self.text(node.span);
        let src = text.strip_prefix(':').unwrap_or(&text);
        match wolf_sema::fmtspec::parse(src) {
            Ok(parsed) if parsed.is_default() => Ok(0),
            Ok(parsed) => Ok(wolf_sema::fmtspec::pack(&parsed)),
            // Sema diagnosed E0412; nothing malformed lowers.
            Err(_) => Err(refuse(
                "a format spec outside the ruled grammar (E0412)",
                node.span,
            )),
        }
    }

    /// Import (once) and call a `__wolf_rt_print_*` shim. No io token
    /// threads through in v0: WIR never reorders calls — block
    /// instruction order IS program order at every backend (the s29
    /// posture; the io spine joins with a reordering tier, recorded in
    /// the campaign closeout).
    fn rt_print_call(&mut self, name: &'static str, params: &[TypeId], args: &[Value]) {
        let ext = match self.callees.get(name) {
            Some(&ext) => ext,
            None => {
                let sig = self
                    .b
                    .module
                    .make_sig(params.iter().map(|&t| Param::val(t)).collect(), vec![]);
                let ext = self.b.func.import_func(name, sig);
                self.callees.insert(name.to_string(), ext);
                ext
            }
        };
        self.b.ins_call(ext, args);
    }

    /// The v0 print path (s31, s38): `print(x)`/`eprint(x)` lower to
    /// per-segment runtime writes. All hole values are evaluated
    /// BEFORE any byte is written (the interpreter's order — a
    /// trapping hole emits no partial output); then literal chunks
    /// flow from module data and values through the typed shims.
    /// `print`/`eprint` append one `\n` segment; the `_raw` forms
    /// append nothing. Spec-less stdout segments keep the frozen s31
    /// `__wolf_rt_print_*` symbols; spec-carrying, stderr, and float
    /// segments flow through the s38 `__wolf_rt_write_*` family with
    /// the comptime-packed spec as an immediate (never a runtime
    /// parse).
    fn lower_print(&mut self, d: CallExpr<'t>, newline: bool, stream: i64) -> R<Flow> {
        let stdout = stream == 1;
        let mut outs: Vec<PrintSeg> = Vec::new();
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            let Some(vexpr) = Arg::value(a) else { continue };
            if vexpr.kind == SyntaxKind::StringExpr {
                // Dedent shifts every hole offset — refuse the
                // combination rather than print undedented text
                // (the checked executor refuses identically).
                if let Some(sd) = StringExpr::cast(vexpr)
                    && self.text(vexpr.span).starts_with("\"\"\"")
                    && sd.interps().any(|i| i.expr().is_some())
                {
                    return Err(refuse(
                        "interpolation inside a multiline string (s38 formatting)",
                        vexpr.span,
                    ));
                }
                for seg in self.string_segments(vexpr) {
                    match seg {
                        StrSeg::Lit(b) => outs.push(PrintSeg::Lit(b)),
                        StrSeg::Hole { expr: h, spec } => {
                            let packed = self.packed_spec(spec)?;
                            let Some(v) = flow_val!(self.lower_expr(h)) else {
                                return Err(refuse("unit-typed interpolation holes", h.span));
                            };
                            let seg = self.classify_print_value(h, v, packed)?;
                            outs.push(seg);
                        }
                    }
                }
            } else {
                let Some(v) = flow_val!(self.lower_expr(vexpr)) else {
                    return Err(refuse("unit-typed print arguments", vexpr.span));
                };
                let seg = self.classify_print_value(vexpr, v, 0)?;
                outs.push(seg);
            }
        }
        if newline {
            outs.push(PrintSeg::Lit(b"\n".to_vec()));
        }
        for out in outs {
            match out {
                PrintSeg::Lit(bytes) => {
                    if bytes.is_empty() {
                        continue;
                    }
                    let idx = self.b.module.intern_data(&bytes);
                    let p = self.b.ins_data_addr(idx);
                    let len = self.b.iconst(types::I64, bytes.len() as i64);
                    if stdout {
                        self.rt_print_call(
                            "__wolf_rt_print_str",
                            &[types::PTR, types::I64],
                            &[p, len],
                        );
                    } else {
                        let st = self.b.iconst(types::I64, stream);
                        let sp = self.b.iconst(types::I64, 0);
                        self.rt_print_call(
                            "__wolf_rt_write_str",
                            &[types::I64, types::PTR, types::I64, types::I64],
                            &[st, p, len, sp],
                        );
                    }
                }
                PrintSeg::Str { v, spec } => {
                    let p = self
                        .b
                        .ins(Opcode::AggGet, &[v], &[types::PTR], Aux::Int(0))
                        .one();
                    let len = self
                        .b
                        .ins(Opcode::AggGet, &[v], &[types::I64], Aux::Int(1))
                        .one();
                    if stdout && spec == 0 {
                        self.rt_print_call(
                            "__wolf_rt_print_str",
                            &[types::PTR, types::I64],
                            &[p, len],
                        );
                    } else {
                        let st = self.b.iconst(types::I64, stream);
                        let sp = self.b.iconst(types::I64, spec);
                        self.rt_print_call(
                            "__wolf_rt_write_str",
                            &[types::I64, types::PTR, types::I64, types::I64],
                            &[st, p, len, sp],
                        );
                    }
                }
                PrintSeg::Int { v, unsigned, spec } => {
                    let vty = self.b.func.value_ty(v);
                    let wide = if vty == types::I64 {
                        v
                    } else if unsigned {
                        self.b
                            .ins(Opcode::Zext, &[v], &[types::I64], Aux::None)
                            .one()
                    } else {
                        self.b
                            .ins(Opcode::Sext, &[v], &[types::I64], Aux::None)
                            .one()
                    };
                    if stdout && spec == 0 {
                        self.rt_print_call("__wolf_rt_print_i64", &[types::I64], &[wide]);
                    } else {
                        let st = self.b.iconst(types::I64, stream);
                        let sp = self.b.iconst(types::I64, spec);
                        self.rt_print_call(
                            "__wolf_rt_write_i64",
                            &[types::I64, types::I64, types::I64],
                            &[st, wide, sp],
                        );
                    }
                }
                PrintSeg::Bool { v, spec } => {
                    if stdout && spec == 0 {
                        self.rt_print_call("__wolf_rt_print_bool", &[types::BOOL], &[v]);
                    } else {
                        let st = self.b.iconst(types::I64, stream);
                        let sp = self.b.iconst(types::I64, spec);
                        self.rt_print_call(
                            "__wolf_rt_write_bool",
                            &[types::I64, types::BOOL, types::I64],
                            &[st, v, sp],
                        );
                    }
                }
                PrintSeg::F64 { v, spec } => {
                    let st = self.b.iconst(types::I64, stream);
                    let sp = self.b.iconst(types::I64, spec);
                    self.rt_print_call(
                        "__wolf_rt_write_f64",
                        &[types::I64, types::F64, types::I64],
                        &[st, v, sp],
                    );
                }
            }
        }
        Ok(Flow::Val(None))
    }

    /// Reload spilled places after a call: the callee's writes are
    /// visible through the slot (the call minted the slot region's
    /// successor token, so these loads are ordered after it).
    fn run_writebacks(&mut self, writebacks: Vec<WriteBack>) -> R<()> {
        for wb in writebacks {
            let WriteBack {
                shape,
                slot,
                region,
                span,
            } = wb;
            match shape {
                WriteBackShape::Var { var, ty } => {
                    let back = self.load_flat(ty, slot, region, span)?;
                    self.b.def_var(var, back);
                }
                WriteBackShape::Field { var, path, fty } => {
                    let back = self.load_flat(fty, slot, region, span)?;
                    let cur_agg = self.b.use_var(var);
                    let rebuilt = self.rebuild_at(cur_agg, &path, back);
                    self.b.def_var(var, rebuilt);
                }
            }
        }
        Ok(())
    }

    /// Rebuild an aggregate value with the leaf at `path` replaced.
    fn rebuild_at(&mut self, agg: Value, path: &[usize], newv: Value) -> Value {
        let Some((&idx, rest)) = path.split_first() else {
            return newv;
        };
        let aty = self.b.func.value_ty(agg);
        let types::TypeData::Agg(fields) = self.b.module.types.get(aty).clone() else {
            unreachable!("field writeback path over a non-aggregate");
        };
        let mut parts = Vec::with_capacity(fields.len());
        for (k, &kt) in fields.iter().enumerate() {
            if k == idx && rest.is_empty() {
                parts.push(newv);
                continue;
            }
            let cur = self
                .b
                .ins(Opcode::AggGet, &[agg], &[kt], Aux::Int(k as i64))
                .one();
            if k == idx {
                parts.push(self.rebuild_at(cur, rest, newv));
            } else {
                parts.push(cur);
            }
        }
        self.b.ins(Opcode::AggMake, &parts, &[aty], Aux::None).one()
    }

    /// Resolve a (possibly nested) member chain over a by-value local:
    /// the base variable, the field index path (outer to inner), and
    /// the leaf's WIR type.
    fn resolve_member_chain(&mut self, e: &'t GreenNode) -> R<(Var, Vec<usize>, TypeId)> {
        let mut members: Vec<wolf_ast::MemberExpr<'t>> = Vec::new();
        let mut cur = e;
        while cur.kind == SyntaxKind::MemberExpr {
            let m = wolf_ast::MemberExpr::cast(cur).expect("kind");
            let Some(base) = m.base() else {
                return Err(refuse("a member chain without a base", e.span));
            };
            members.push(m);
            cur = base;
        }
        if cur.kind != SyntaxKind::PathExpr {
            return Err(refuse("`mut` places beyond local paths", e.span));
        }
        let name = self.text(cur.span);
        let Some(LocalBind::Val {
            var,
            wir_ty: mut wt,
            ..
        }) = self.lookup(&name)
        else {
            return Err(refuse(
                "`mut` places beyond local by-value bindings",
                e.span,
            ));
        };
        members.reverse(); // innermost (closest to the base) first
        let mut path = Vec::with_capacity(members.len());
        for m in members {
            let base = m.base().expect("checked above");
            let Some(base_sema) = self.expr_sema_ty(base.span) else {
                return Err(refuse("a member place without a recorded type", e.span));
            };
            let (index, ..) = self.member_index(base_sema, m, e.span)?;
            let types::TypeData::Agg(fields) = self.b.module.types.get(wt).clone() else {
                return Err(refuse("member places over non-aggregates", e.span));
            };
            let Some(&fty) = fields.get(index) else {
                return Err(refuse("a member the aggregate does not carry", e.span));
            };
            path.push(index);
            wt = fty;
        }
        Ok((var, path, wt))
    }

    /// Classify one `mut` argument: a whole flat local or a flat field
    /// path of a by-value aggregate local spills; a re-lent `mut`
    /// parameter passes through.
    fn lower_mut_arg(&mut self, vexpr: &'t GreenNode) -> R<MutArg> {
        match vexpr.kind {
            SyntaxKind::PathExpr => {
                let name = self.text(vexpr.span);
                match self.lookup(&name) {
                    Some(LocalBind::Val {
                        var, wir_ty: wty, ..
                    }) => {
                        let Some(size) = flat_size(&self.b.module.types, wty) else {
                            return Err(refuse(
                                "`mut` arguments of non-flat types (spill layout, c06)",
                                vexpr.span,
                            ));
                        };
                        let cur = self.b.use_var(var);
                        Ok(MutArg::Spill {
                            cur,
                            size,
                            writeback: WriteBackShape::Var { var, ty: wty },
                        })
                    }
                    Some(LocalBind::MutRef { ptr, region, .. }) => {
                        Ok(MutArg::Relend { ptr, region })
                    }
                    _ => Err(refuse(
                        "`mut` arguments beyond local places (c06)",
                        vexpr.span,
                    )),
                }
            }
            SyntaxKind::MemberExpr => {
                let (var, path, fty) = self.resolve_member_chain(vexpr)?;
                let Some(size) = flat_size(&self.b.module.types, fty) else {
                    return Err(refuse(
                        "`mut` arguments of non-flat types (spill layout, c06)",
                        vexpr.span,
                    ));
                };
                let mut cur = self.b.use_var(var);
                for &idx in &path {
                    let aty = self.b.func.value_ty(cur);
                    let types::TypeData::Agg(fields) = self.b.module.types.get(aty).clone() else {
                        return Err(refuse("`mut` fields of non-aggregates", vexpr.span));
                    };
                    cur = self
                        .b
                        .ins(Opcode::AggGet, &[cur], &[fields[idx]], Aux::Int(idx as i64))
                        .one();
                }
                Ok(MutArg::Spill {
                    cur,
                    size,
                    writeback: WriteBackShape::Field { var, path, fty },
                })
            }
            _ => Err(refuse(
                "`mut` arguments beyond local places (c06)",
                vexpr.span,
            )),
        }
    }

    /// Enum-variant construction (`Color.Rgb(1, 2, 3)`, s27): the
    /// variant tag alone for payload-free enums, else tag + payloads
    /// with unfilled slots zeroed.
    fn lower_ctor(&mut self, d: CallExpr<'t>, cs: &CallSig, e: &'t GreenNode) -> R<Flow> {
        let Some(sema_ty) = self.expr_sema_ty(e.span) else {
            return Err(refuse("a constructor without a recorded type", e.span));
        };
        let mut ty = sema_ty;
        for _ in 0..32 {
            match self.table.kind(ty) {
                TyKind::Distinct(inner) => ty = *inner,
                _ => break,
            }
        }
        if matches!(
            self.table.kind(ty),
            TyKind::List(_) | TyKind::Pool(_) | TyKind::Shared(_)
        ) {
            return Err(refuse(
                "List/Pool/shared constructor lowering (runtime shapes, c06)",
                e.span,
            ));
        }
        let TyKind::Nominal { module, name } = self.table.kind(ty) else {
            return Err(refuse("constructing a non-enum type", e.span));
        };
        let Some(ItemSig::Enum { variants, .. }) = self.sigs.get(*module as usize, name) else {
            return Err(refuse("constructing a non-enum type", e.span));
        };
        let vname = cs.callee.rsplit('.').next().unwrap_or(&cs.callee);
        let Some(index) = variants.iter().position(|v| v.name == vname) else {
            return Err(refuse("a variant the enum does not declare", e.span));
        };
        let mut payloads = Vec::new();
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            let Some(vexpr) = Arg::value(a) else { continue };
            let Some(v) = flow_val!(self.lower_expr(vexpr)) else {
                return Err(refuse("unit-typed enum payloads", vexpr.span));
            };
            payloads.push(v);
        }
        let Some(wty) = wir_ty(
            &mut self.b.module.types,
            self.table,
            self.sigs,
            sema_ty,
            e.span,
        )?
        else {
            return Err(refuse("a unit-shaped enum", e.span));
        };
        let tag = self.b.iconst(types::I64, index as i64);
        if wty == types::I64 {
            // Payload-free enum: the tag IS the value.
            return Ok(Flow::Val(Some(tag)));
        }
        let types::TypeData::Agg(fields) = self.b.module.types.get(wty).clone() else {
            return Err(refuse("an enum without an aggregate shape", e.span));
        };
        let mut parts = vec![tag];
        parts.extend(payloads.iter().copied());
        for &fty in &fields[parts.len()..] {
            let z = self.zero_of(fty, e.span)?;
            parts.push(z);
        }
        Ok(Flow::Val(Some(
            self.b.ins(Opcode::AggMake, &parts, &[wty], Aux::None).one(),
        )))
    }

    /// An inherent method call (s27): the receiver marshals under its
    /// declared mode (read/take by value, `mut` pointer-shaped via the
    /// s26 spill machinery), then ordinary arguments; the callee is
    /// the mangled `Type.method` function.
    fn lower_method_call(&mut self, d: CallExpr<'t>, cs: &CallSig, e: &'t GreenNode) -> R<Flow> {
        let Some(disp) = self.dispatch.get(&e.span).copied() else {
            return Err(refuse("a method call without a dispatch record", e.span));
        };
        let Dispatch::Inherent { ty, method } = disp else {
            return Err(refuse(
                "trait-method call lowering (dispatch tables, c06)",
                e.span,
            ));
        };
        // The receiver place: `recv.m(…)` / `(mut recv).m(…)`.
        let Some(callee) = d.callee() else {
            return Err(refuse("a method call without a callee", e.span));
        };
        let m = wolf_ast::MemberExpr::cast(callee)
            .ok_or_else(|| refuse("a method call without a receiver", e.span))?;
        let Some(base) = m.base() else {
            return Err(refuse("a method call without a receiver", e.span));
        };
        let recv_place: &GreenNode = if base.kind == SyntaxKind::ParenExpr {
            ParenExpr::cast(base).and_then(|p| p.expr()).unwrap_or(base)
        } else {
            base
        };
        // The method's signature, from the unique inherent impl.
        let msig: &FnSig = self
            .sigs
            .impls
            .iter()
            .filter(|i| i.trait_ref.is_none())
            .find_map(|i| {
                let TyKind::Nominal { name, .. } = self.sig_table.kind(i.self_ty) else {
                    return None;
                };
                if name != ty {
                    return None;
                }
                i.methods
                    .iter()
                    .find(|mm| &mm.name == method)
                    .map(|mm| &mm.sig)
            })
            .ok_or_else(|| refuse("a method call without an elaborated impl", e.span))?;
        if !msig.generics.is_empty() {
            return Err(refuse("generic-method calls (monomorphization)", e.span));
        }
        if msig.comptime {
            return Err(refuse("comptime calls (D29 CTFE owns these)", e.span));
        }
        let mut args = Vec::new();
        let mut formal_regions: HashMap<u32, RegionId> = HashMap::new();
        let mut next_formal = 0u32;
        let mut writebacks: Vec<WriteBack> = Vec::new();
        let mut spilled_slots: Vec<Value> = Vec::new();
        // Receiver first, under its declared mode.
        let recv_mode = cs.params.first().and_then(|p| p.mode);
        match recv_mode {
            Some(ParamMode::Mut) => {
                let formal = next_formal;
                next_formal += 1;
                match self.lower_mut_arg(recv_place)? {
                    MutArg::Spill {
                        cur,
                        size,
                        writeback,
                    } => {
                        let (slot_region, slot) = self.b.ins_stack_alloc(size);
                        self.b.func.add_fact(FactData::new(
                            FactKind::Region(slot, slot_region),
                            Just::DefOp,
                        ));
                        self.b.func.add_fact(FactData::new(
                            FactKind::Deref(slot, DerefSize::Const(size)),
                            Just::DefOp,
                        ));
                        self.store_flat(cur, slot, slot_region, recv_place.span)?;
                        formal_regions.insert(formal, slot_region);
                        args.push(slot);
                        spilled_slots.push(slot);
                        writebacks.push(writeback.filled(slot, slot_region, recv_place.span));
                    }
                    MutArg::Relend { ptr, region } => {
                        formal_regions.insert(formal, region);
                        args.push(ptr);
                    }
                }
            }
            _ => {
                // read / take: the receiver travels by value.
                let Some(v) = flow_val!(self.lower_expr(recv_place)) else {
                    return Err(refuse("a valueless receiver", recv_place.span));
                };
                args.push(v);
            }
        }
        // Remaining arguments under their declared modes (params[0] is
        // the receiver).
        for (i, a) in d.args().into_iter().flat_map(|l| l.args()).enumerate() {
            let mode = cs.params.get(i + 1).and_then(|p| p.mode);
            let Some(vexpr) = Arg::value(a) else { continue };
            if mode == Some(ParamMode::Mut) {
                let formal = next_formal;
                next_formal += 1;
                match self.lower_mut_arg(vexpr)? {
                    MutArg::Spill {
                        cur,
                        size,
                        writeback,
                    } => {
                        let (slot_region, slot) = self.b.ins_stack_alloc(size);
                        self.b.func.add_fact(FactData::new(
                            FactKind::Region(slot, slot_region),
                            Just::DefOp,
                        ));
                        self.b.func.add_fact(FactData::new(
                            FactKind::Deref(slot, DerefSize::Const(size)),
                            Just::DefOp,
                        ));
                        self.store_flat(cur, slot, slot_region, vexpr.span)?;
                        formal_regions.insert(formal, slot_region);
                        args.push(slot);
                        spilled_slots.push(slot);
                        writebacks.push(writeback.filled(slot, slot_region, vexpr.span));
                    }
                    MutArg::Relend { ptr, region } => {
                        formal_regions.insert(formal, region);
                        args.push(ptr);
                    }
                }
                continue;
            }
            let v = flow_val!(self.lower_expr(vexpr));
            let Some(v) = v else {
                return Err(refuse("unit-typed arguments", vexpr.span));
            };
            args.push(v);
        }
        for (i, &a) in spilled_slots.iter().enumerate() {
            for &b in &spilled_slots[i + 1..] {
                self.b.func.add_fact(FactData::new(
                    FactKind::Noalias(a, b),
                    Just::Theorem(Theorem::ExclField),
                ));
            }
        }
        // The WIR callee is the mangled `Type.method` — sema's callee
        // label may be the bare method name, and bare names collide
        // across types.
        let callee_name = format!("{ty}.{method}");
        let ext = match self.callees.get(&callee_name) {
            Some(&ext) => ext,
            None => {
                let sig = wir_sig_of(self.b.module, self.sigs, msig, e.span)?;
                let ext = self.b.func.import_func(callee_name.clone(), sig);
                self.callees.insert(callee_name, ext);
                ext
            }
        };
        let results = self.b.ins_call_regions(ext, &args, &formal_regions);
        self.run_writebacks(writebacks)?;
        Ok(Flow::Val(results.first().copied()))
    }

    // ------------------------------------- error unions (s27, D30) ----

    /// The recorded type at `span` as a tagged-union WIR type, when it
    /// is one (the raise / fallible-merge target).
    fn eu_ty_of_span(&mut self, span: Span) -> R<Option<TypeId>> {
        let Some(t) = self.expr_sema_ty(span) else {
            return Ok(None);
        };
        if let TyKind::ErrUnion(_, row) = self.table.kind(t)
            && !row_is_empty(self.table, *row)
        {
            return wir_ty(&mut self.b.module.types, self.table, self.sigs, t, span);
        }
        Ok(None)
    }

    /// Alias with intent: is this span a tag-raise site (sema recorded
    /// the enclosing union type on it via `inject_tag`)?
    fn raise_target(&mut self, span: Span) -> R<Option<TypeId>> {
        self.eu_ty_of_span(span)
    }

    /// Lower an expression whose value must be `eu`-typed (a fallible
    /// return operand or fn tail): raises and calls already produce eu
    /// values through their own arms; a plain ok value is injected.
    fn lower_fallible_expr(&mut self, e: &'t GreenNode, eu: TypeId) -> R<Flow> {
        let v = match self.lower_expr(e)? {
            Flow::Val(v) => v,
            Flow::Diverged => return Ok(Flow::Diverged),
        };
        Ok(Flow::Val(self.arm_to_merge(v, Some(eu), e.span)?))
    }

    /// Coerce one merge-arm value into the fallible merge type:
    /// identity when already there, ok-injection (`eu.make.ok`)
    /// otherwise, widening for a narrower union.
    fn arm_to_merge(
        &mut self,
        v: Option<Value>,
        want_eu: Option<TypeId>,
        span: Span,
    ) -> R<Option<Value>> {
        let Some(eu) = want_eu else { return Ok(v) };
        let types::TypeData::Eu { ok, .. } = self.b.module.types.get(eu).clone() else {
            unreachable!("merge target is a union");
        };
        match v {
            None => {
                if ok.is_some() {
                    return Err(refuse(
                        "a valueless arm in a value-carrying fallible merge",
                        span,
                    ));
                }
                Ok(Some(self.b.ins_eu_make_ok(eu, None)))
            }
            Some(v) => {
                let vt = self.b.func.value_ty(v);
                if vt == eu {
                    return Ok(Some(v));
                }
                if matches!(self.b.module.types.get(vt), types::TypeData::Eu { .. }) {
                    return self.coerce_eu(v, eu, span).map(Some);
                }
                Ok(Some(self.b.ins_eu_make_ok(eu, Some(v))))
            }
        }
    }

    /// Widen a union value into a wider union type (RowWiden, s15):
    /// with module-interned tags the tag passes through unchanged; the
    /// ok and payload halves rebuild. Emits a diamond when the err bit
    /// is dynamic.
    fn coerce_eu(&mut self, v: Value, target: TypeId, span: Span) -> R<Value> {
        let src = self.b.func.value_ty(v);
        if src == target {
            return Ok(v);
        }
        let types::TypeData::Eu { ok: sok, .. } = self.b.module.types.get(src).clone() else {
            return Err(refuse("widening a non-union value", span));
        };
        let types::TypeData::Eu { ok: tok, .. } = self.b.module.types.get(target).clone() else {
            return Err(refuse("widening into a non-union type", span));
        };
        if sok != tok {
            return Err(refuse("row widening across differing ok types", span));
        }
        let is_err = self.b.ins_eu_is_err(v);
        if let Some(c) = self.b.as_bool_const(is_err) {
            return if c {
                self.propagate_err(v, target, span)
            } else {
                let okv = sok.map(|_| self.b.ins_eu_ok(v));
                Ok(self.b.ins_eu_make_ok(target, okv))
            };
        }
        let err_bb = self.b.create_block();
        let ok_bb = self.b.create_block();
        let merge = self.b.create_block();
        let out = self.b.add_block_param(merge, target);
        self.b.ins_br(is_err, err_bb, &[], ok_bb, &[]);
        self.b.seal_block(err_bb);
        self.b.seal_block(ok_bb);
        self.b.switch_to_block(err_bb);
        self.b.gvn_push_scope();
        let ev = self.propagate_err(v, target, span)?;
        self.b.ins_jmp(merge, &[ev]);
        self.b.gvn_pop_scope();
        self.b.switch_to_block(ok_bb);
        self.b.gvn_push_scope();
        let okv = sok.map(|_| self.b.ins_eu_ok(v));
        let rv = self.b.ins_eu_make_ok(target, okv);
        self.b.ins_jmp(merge, &[rv]);
        self.b.gvn_pop_scope();
        self.b.seal_block(merge);
        self.b.switch_to_block(merge);
        Ok(out)
    }

    /// Rebuild a union's error half at the target union type — D30's
    /// "injection re-tagging" is the identity on module-interned tags;
    /// only payload slots transfer (a prefix of the target's, by slot
    /// unification).
    fn propagate_err(&mut self, v: Value, target: TypeId, span: Span) -> R<Value> {
        let src = self.b.func.value_ty(v);
        if src == target {
            return Ok(v);
        }
        let types::TypeData::Eu { slots: sslots, .. } = self.b.module.types.get(src).clone() else {
            return Err(refuse("propagating a non-union value", span));
        };
        let types::TypeData::Eu { slots: tslots, .. } = self.b.module.types.get(target).clone()
        else {
            return Err(refuse("propagating into a non-union type", span));
        };
        if sslots.len() > tslots.len() || sslots[..] != tslots[..sslots.len()] {
            return Err(refuse(
                "row widening with non-prefix payload slots (spilled union layout, c06)",
                span,
            ));
        }
        let tag = self.b.ins_eu_err_tag(v);
        let payloads: Vec<Value> = (0..sslots.len())
            .map(|k| self.b.ins_eu_err_slot(v, k))
            .collect();
        Ok(self.b.ins_eu_make_err(target, tag, &payloads))
    }

    /// Postfix `?` (D30): `br eu.is_err, b_err, b_ok`. The ok path
    /// continues with the payload; the err path forms the propagated
    /// error FIRST, runs the errdefer+defer chains ([mem.model.order]),
    /// and `ret`s — ordinary control flow, no unwinding, ever.
    fn lower_try(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = TryExpr::cast(e).expect("kind");
        let Some(inner) = d.expr() else {
            return Ok(Flow::Val(None));
        };
        let v = flow_val!(self.lower_expr(inner));
        let Some(v) = v else {
            return Err(refuse("`?` on a valueless operand", e.span));
        };
        let vty = self.b.func.value_ty(v);
        let types::TypeData::Eu { ok, .. } = self.b.module.types.get(vty).clone() else {
            // The operand's row is statically empty: it lowered as its
            // plain ok type, and `?` is the identity.
            return Ok(Flow::Val(Some(v)));
        };
        let Some(fn_eu) = self.fn_eu else {
            return Err(refuse(
                "`?` outside a fallible fn (checker contract)",
                e.span,
            ));
        };
        let is_err = self.b.ins_eu_is_err(v);
        if let Some(c) = self.b.as_bool_const(is_err) {
            // A locally-built union: the split folds away entirely.
            if c {
                let out = self.propagate_err(v, fn_eu, e.span)?;
                if self.run_exits(0, true)? {
                    self.b.ins_ret(&[out]);
                }
                return Ok(Flow::Diverged);
            }
            let okv = ok.map(|_| self.b.ins_eu_ok(v));
            return Ok(Flow::Val(okv));
        }
        let err_bb = self.b.create_block();
        let ok_bb = self.b.create_block();
        self.b.ins_br(is_err, err_bb, &[], ok_bb, &[]);
        self.b.seal_block(err_bb);
        self.b.seal_block(ok_bb);
        self.b.switch_to_block(err_bb);
        self.b.gvn_push_scope();
        let out = self.propagate_err(v, fn_eu, e.span)?;
        let flowing = self.run_exits(0, true);
        self.b.gvn_pop_scope();
        if flowing? {
            self.b.ins_ret(&[out]);
        }
        self.b.switch_to_block(ok_bb);
        let okv = ok.map(|_| self.b.ins_eu_ok(v));
        Ok(Flow::Val(okv))
    }

    /// `expr else fallback` / `expr else |err| handler` (D30): branch
    /// on the err bit; the handler runs with the caught row value
    /// bound; both sides merge on the ok type.
    fn lower_else(&mut self, e: &'t GreenNode, want: bool) -> R<Flow> {
        let d = ElseExpr::cast(e).expect("kind");
        let Some(scrut) = d.scrutinized() else {
            return Ok(Flow::Val(None));
        };
        let v = flow_val!(self.lower_expr(scrut));
        let Some(v) = v else {
            return Err(refuse("`else` on a valueless operand", e.span));
        };
        let vty = self.b.func.value_ty(v);
        let types::TypeData::Eu { ok, slots } = self.b.module.types.get(vty).clone() else {
            // Statically-empty row: the fallback is dead code.
            return Ok(Flow::Val(Some(v)));
        };
        let Some(fb) = d.fallback() else {
            return Err(refuse("an `else` without a fallback", e.span));
        };
        let want_v = match self.expr_sema_ty(e.span) {
            Some(t) => {
                wir_ty(&mut self.b.module.types, self.table, self.sigs, t, e.span)?.is_some()
            }
            None => want,
        };
        let is_err = self.b.ins_eu_is_err(v);
        if let Some(c) = self.b.as_bool_const(is_err) {
            if !c {
                return Ok(Flow::Val(ok.map(|_| self.b.ins_eu_ok(v))));
            }
            return self.lower_else_handler(d, v, &slots, fb, want_v);
        }
        let err_bb = self.b.create_block();
        let ok_bb = self.b.create_block();
        self.b.ins_br(is_err, err_bb, &[], ok_bb, &[]);
        self.b.seal_block(err_bb);
        self.b.seal_block(ok_bb);
        self.b.switch_to_block(ok_bb);
        self.b.gvn_push_scope();
        let okv = ok.map(|_| self.b.ins_eu_ok(v));
        self.b.gvn_pop_scope();
        let ok_end = self.b.current_block();
        self.b.switch_to_block(err_bb);
        self.b.gvn_push_scope();
        let hres = self.lower_else_handler(d, v, &slots, fb, want_v);
        let hflow = match hres {
            Ok(f) => f,
            Err(x) => {
                self.b.gvn_pop_scope();
                return Err(x);
            }
        };
        let err_end = self.b.current_block();
        self.b.gvn_pop_scope();
        match hflow {
            Flow::Diverged => {
                // The handler always diverges (`else |_| { break }`,
                // the drive-loop shape): the ok value continues alone.
                self.b.switch_to_block(ok_end);
                Ok(Flow::Val(okv))
            }
            Flow::Val(hv) => {
                let merge = self.b.create_block();
                let (param, oargs, hargs): (Option<Value>, Vec<Value>, Vec<Value>) =
                    match (want_v, okv, hv) {
                        (true, Some(a), Some(b)) if a != b => {
                            let ty = self.b.func.value_ty(a);
                            let p = self.b.add_block_param(merge, ty);
                            (Some(p), vec![a], vec![b])
                        }
                        (true, Some(a), Some(_)) => (Some(a), vec![], vec![]),
                        _ => (None, vec![], vec![]),
                    };
                self.b.switch_to_block(ok_end);
                self.b.ins_jmp(merge, &oargs);
                self.b.switch_to_block(err_end);
                self.b.ins_jmp(merge, &hargs);
                self.b.seal_block(merge);
                self.b.switch_to_block(merge);
                Ok(Flow::Val(param))
            }
        }
    }

    /// The `else` handler side: bind `|err|` to the caught row value
    /// (tag alone, or tag + payload slots as an aggregate — the enum
    /// mirror), then lower the fallback.
    fn lower_else_handler(
        &mut self,
        d: ElseExpr<'t>,
        v: Value,
        slots: &[TypeId],
        fb: &'t GreenNode,
        want_v: bool,
    ) -> R<Flow> {
        self.scopes.push(ScopeFrame::default());
        if let Some(p) = d.handler_pattern() {
            match p.kind {
                SyntaxKind::IdentPat => {
                    let name = self.text(p.span);
                    let tag = self.b.ins_eu_err_tag(v);
                    let rv = if slots.is_empty() {
                        tag
                    } else {
                        let mut parts = vec![tag];
                        for k in 0..slots.len() {
                            parts.push(self.b.ins_eu_err_slot(v, k));
                        }
                        let mut fields = vec![types::I64];
                        fields.extend_from_slice(slots);
                        let aggt = self.b.module.types.intern(types::TypeData::Agg(fields));
                        self.b
                            .ins(Opcode::AggMake, &parts, &[aggt], Aux::None)
                            .one()
                    };
                    let rty = self.b.func.value_ty(rv);
                    let var = self.b.declare_var(rty);
                    self.b.def_var(var, rv);
                    self.scopes.last_mut().expect("scope").binds.push((
                        name,
                        LocalBind::Val {
                            var,
                            wrapping: false,
                            unsigned: false,
                            wir_ty: rty,
                        },
                    ));
                }
                SyntaxKind::WildcardPat => {}
                _ => {
                    self.scopes.pop();
                    return Err(refuse("destructuring `else` handler patterns", p.span));
                }
            }
        }
        let flow = self.lower_expr_w(fb, want_v);
        self.scopes.pop();
        flow
    }

    // ------------------------------------- match lowering (s27) ----

    /// What one scrutinee type's discriminant ranges over.
    fn match_domain(&mut self, scrut_sema: TyId, span: Span) -> R<MatchDomain> {
        let mut ty = scrut_sema;
        for _ in 0..32 {
            match self.table.kind(ty) {
                TyKind::Distinct(inner) => ty = *inner,
                _ => break,
            }
        }
        match self.table.kind(ty) {
            TyKind::Nominal { module, name } => match self.sigs.get(*module as usize, name) {
                Some(ItemSig::Enum { variants, .. }) => Ok(MatchDomain::Enum(
                    variants
                        .iter()
                        .enumerate()
                        .map(|(i, v)| (v.name.clone(), i as i64, v.payload.len()))
                        .collect(),
                )),
                _ => Err(refuse("match over this nominal type", span)),
            },
            TyKind::Row { tags, .. } => Ok(MatchDomain::Row(
                tags.iter().map(|(n, p)| (n.clone(), p.len())).collect(),
            )),
            TyKind::Prim(Prim::Bool) | TyKind::Prim(_) | TyKind::Wrapping(_) => {
                Ok(MatchDomain::Scalar)
            }
            _ => Err(refuse("match over this scrutinee type", span)),
        }
    }

    /// The tag constant a name tests for in `domain`, if it names one.
    /// Enum variants and row tags match by full dotted name or last
    /// segment (type-directed, case-blind — the #4 posture).
    fn domain_test(&mut self, domain: &MatchDomain, name: &str) -> Option<(i64, usize)> {
        let last = name.rsplit('.').next().unwrap_or(name);
        match domain {
            MatchDomain::Enum(vs) => vs
                .iter()
                .find(|(n, ..)| n == name || n == last)
                .map(|(_, i, a)| (*i, *a)),
            MatchDomain::Row(ts) => {
                ts.iter()
                    .find(|(n, _)| n == name || n == last)
                    .map(|(n, a)| {
                        let id = self.b.module.tag_id(n);
                        (id, *a)
                    })
            }
            MatchDomain::Scalar => None,
        }
    }

    /// Classify one arm pattern against the domain.
    fn pattern_shape(&mut self, pat: &'t GreenNode, domain: &MatchDomain) -> R<PatShape> {
        match pat.kind {
            SyntaxKind::WildcardPat => Ok(PatShape::Irrefutable(None)),
            SyntaxKind::IdentPat => {
                let name = self.text(pat.span);
                match self.domain_test(domain, &name) {
                    Some((c, _)) => Ok(PatShape::Tests(vec![c], vec![])),
                    None => Ok(PatShape::Irrefutable(Some(name))),
                }
            }
            SyntaxKind::LiteralPat => {
                let text = self.text(pat.span);
                if text == "true" {
                    return Ok(PatShape::BoolTest(true));
                }
                if text == "false" {
                    return Ok(PatShape::BoolTest(false));
                }
                match parse_int_literal(&text) {
                    Some(n) => Ok(PatShape::Tests(vec![n], vec![])),
                    None => Err(refuse("this literal pattern shape", pat.span)),
                }
            }
            SyntaxKind::PathPat => {
                // `Tag(subpats…)` / `Type.Variant(subpats…)`: the path
                // names the tag; payload subpatterns bind slots. The
                // path may be a nested node, so read it as the source
                // text before the payload parenthesis.
                let end = pat
                    .nodes()
                    .find(|n| wolf_ast::is_pattern_kind(n.kind))
                    .map(|n| n.span.lo)
                    .unwrap_or(pat.span.hi);
                let raw = String::from_utf8_lossy(&self.src[pat.span.lo as usize..end as usize])
                    .into_owned();
                let name: String = raw.trim_end().trim_end_matches('(').trim_end().to_string();
                let Some((c, arity)) = self.domain_test(domain, &name) else {
                    return Err(refuse(
                        "a pattern tag the scrutinee does not carry",
                        pat.span,
                    ));
                };
                let subs: Vec<&GreenNode> = pat
                    .nodes()
                    .filter(|n| wolf_ast::is_pattern_kind(n.kind))
                    .collect();
                if subs.len() != arity {
                    return Err(refuse(
                        "payload arity in a pattern (checker contract)",
                        pat.span,
                    ));
                }
                let mut binds = Vec::new();
                for (j, s) in subs.iter().enumerate() {
                    match s.kind {
                        SyntaxKind::IdentPat => binds.push((j, self.text(s.span))),
                        SyntaxKind::WildcardPat => {}
                        _ => {
                            return Err(refuse(
                                "nested refutable payload patterns (deep trees, c06)",
                                s.span,
                            ));
                        }
                    }
                }
                Ok(PatShape::Tests(vec![c], binds))
            }
            SyntaxKind::OrPat => {
                let mut consts = Vec::new();
                for alt in pat.nodes().filter(|n| wolf_ast::is_pattern_kind(n.kind)) {
                    match self.pattern_shape(alt, domain)? {
                        PatShape::Irrefutable(_) => return Ok(PatShape::Irrefutable(None)),
                        PatShape::Tests(cs, binds) => {
                            if !binds.is_empty() {
                                return Err(refuse(
                                    "or-patterns with payload bindings (join params, c06)",
                                    alt.span,
                                ));
                            }
                            consts.extend(cs);
                        }
                        PatShape::BoolTest(_) => {
                            return Err(refuse("or-patterns over bool", alt.span));
                        }
                    }
                }
                Ok(PatShape::Tests(consts, vec![]))
            }
            _ => Err(refuse(
                "this pattern shape in match lowering (tuple/@-bindings, c06)",
                pat.span,
            )),
        }
    }

    /// Compile a `match` to a decision chain over one discriminant
    /// read: shared tests in arm order, guards re-entering at the next
    /// candidate, no default edge when sema proved totality (s17).
    fn lower_match(&mut self, e: &'t GreenNode, want: bool) -> R<Flow> {
        let d = MatchExpr::cast(e).expect("kind");
        let Some(scrut_node) = d.scrutinee() else {
            return Ok(Flow::Val(None));
        };
        let Some(scrut_sema) = self.expr_sema_ty(scrut_node.span) else {
            return Err(refuse("a match without a recorded scrutinee type", e.span));
        };
        let sv = flow_val!(self.lower_expr(scrut_node));
        let Some(sv) = sv else {
            return Err(refuse("match on a valueless scrutinee", e.span));
        };
        let domain = self.match_domain(scrut_sema, e.span)?;
        // ONE discriminant read: payload-carrying enum/row values are
        // aggregates whose field 0 is the tag.
        let sv_ty = self.b.func.value_ty(sv);
        let disc = match (&domain, self.b.module.types.get(sv_ty).clone()) {
            (MatchDomain::Enum(_) | MatchDomain::Row(_), types::TypeData::Agg(fields)) => self
                .b
                .ins(Opcode::AggGet, &[sv], &[fields[0]], Aux::Int(0))
                .one(),
            _ => sv,
        };
        let want_v = match self.expr_sema_ty(e.span) {
            Some(t) => {
                wir_ty(&mut self.b.module.types, self.table, self.sigs, t, e.span)?.is_some()
            }
            None => want,
        };
        let merge_eu = self.eu_ty_of_span(e.span)?;
        let exhaustive = self.matches.get(&e.span).copied().unwrap_or(false);
        let arms: Vec<MatchArm> = d.arms().collect();
        let n = arms.len();
        // The merge point, created on the first arm that completes.
        let mut merge: Option<(Block, Option<Value>)> = None;
        let mut open = true; // the current block still needs a decision
        for (i, arm) in arms.iter().enumerate() {
            if !open {
                break; // an irrefutable arm ended the chain (E0802'd)
            }
            let Some(pat) = arm.pattern() else { continue };
            let shape = self.pattern_shape(pat, &domain)?;
            let is_last = i + 1 == n;
            match shape {
                PatShape::Irrefutable(bind) => {
                    self.enter_match_arm(
                        arm,
                        sv,
                        &[],
                        bind,
                        want_v,
                        merge_eu,
                        &mut merge,
                        None,
                        e.span,
                    )?;
                    open = false;
                }
                PatShape::BoolTest(b) => {
                    // A constant discriminant selects statically —
                    // untaken arms are never lowered (dead code costs
                    // nothing, the if-const posture).
                    if let Some(c) = self.b.as_bool_const(disc) {
                        self.b.stats.identity += 1;
                        if c != b {
                            continue;
                        }
                        if arm.guard().is_some() {
                            let next_bb = self.b.create_block();
                            self.enter_match_arm(
                                arm,
                                sv,
                                &[],
                                None,
                                want_v,
                                merge_eu,
                                &mut merge,
                                Some(next_bb),
                                e.span,
                            )?;
                            self.b.seal_block(next_bb);
                            self.b.switch_to_block(next_bb);
                        } else {
                            self.enter_match_arm(
                                arm,
                                sv,
                                &[],
                                None,
                                want_v,
                                merge_eu,
                                &mut merge,
                                None,
                                e.span,
                            )?;
                            open = false;
                        }
                        continue;
                    }
                    if is_last && exhaustive && arm.guard().is_none() {
                        self.enter_match_arm(
                            arm,
                            sv,
                            &[],
                            None,
                            want_v,
                            merge_eu,
                            &mut merge,
                            None,
                            e.span,
                        )?;
                        open = false;
                    } else {
                        let arm_bb = self.b.create_block();
                        let next_bb = self.b.create_block();
                        if b {
                            self.b.ins_br(disc, arm_bb, &[], next_bb, &[]);
                        } else {
                            self.b.ins_br(disc, next_bb, &[], arm_bb, &[]);
                        }
                        self.b.seal_block(arm_bb);
                        self.b.switch_to_block(arm_bb);
                        self.enter_match_arm(
                            arm,
                            sv,
                            &[],
                            None,
                            want_v,
                            merge_eu,
                            &mut merge,
                            Some(next_bb),
                            e.span,
                        )?;
                        self.b.seal_block(next_bb);
                        self.b.switch_to_block(next_bb);
                    }
                }
                PatShape::Tests(consts, binds) => {
                    if let Some(n) = self.b.as_int_const(disc) {
                        self.b.stats.identity += 1;
                        if !consts.contains(&n) {
                            continue;
                        }
                        if arm.guard().is_some() {
                            let next_bb = self.b.create_block();
                            self.enter_match_arm(
                                arm,
                                sv,
                                &binds,
                                None,
                                want_v,
                                merge_eu,
                                &mut merge,
                                Some(next_bb),
                                e.span,
                            )?;
                            self.b.seal_block(next_bb);
                            self.b.switch_to_block(next_bb);
                        } else {
                            self.enter_match_arm(
                                arm, sv, &binds, None, want_v, merge_eu, &mut merge, None, e.span,
                            )?;
                            open = false;
                        }
                        continue;
                    }
                    if is_last && exhaustive && arm.guard().is_none() {
                        // Sema's totality theorem: the final test is an
                        // unconditional edge — no default arm exists.
                        self.enter_match_arm(
                            arm, sv, &binds, None, want_v, merge_eu, &mut merge, None, e.span,
                        )?;
                        open = false;
                    } else {
                        let arm_bb = self.b.create_block();
                        let next_bb = self.b.create_block();
                        // Values born in the later chain blocks do not
                        // dominate the arm or the next candidate: keep
                        // them out of the enclosing GVN scope.
                        let mut chain_scopes = 0usize;
                        for (k, &c) in consts.iter().enumerate() {
                            let cv = self.b.iconst(types::I64, c);
                            let t = self
                                .b
                                .ins(
                                    Opcode::Icmp,
                                    &[disc, cv],
                                    &[types::BOOL],
                                    Aux::IntCc(IntCc::Eq),
                                )
                                .one();
                            if k + 1 == consts.len() {
                                self.b.ins_br(t, arm_bb, &[], next_bb, &[]);
                            } else {
                                let more = self.b.create_block();
                                self.b.ins_br(t, arm_bb, &[], more, &[]);
                                self.b.seal_block(more);
                                self.b.switch_to_block(more);
                                self.b.gvn_push_scope();
                                chain_scopes += 1;
                            }
                        }
                        for _ in 0..chain_scopes {
                            self.b.gvn_pop_scope();
                        }
                        self.b.seal_block(arm_bb);
                        self.b.switch_to_block(arm_bb);
                        self.enter_match_arm(
                            arm,
                            sv,
                            &binds,
                            None,
                            want_v,
                            merge_eu,
                            &mut merge,
                            Some(next_bb),
                            e.span,
                        )?;
                        self.b.seal_block(next_bb);
                        self.b.switch_to_block(next_bb);
                    }
                }
            }
        }
        if open {
            // A live residual edge (guards on the closing arms): sema
            // proved the value space covered, so this edge is
            // unreachable at runtime — the licensed trap.
            self.b.ins_trap(TrapKind::Assert);
        }
        match merge {
            Some((mb, param)) => {
                self.b.seal_block(mb);
                self.b.switch_to_block(mb);
                Ok(Flow::Val(param))
            }
            None => Ok(Flow::Diverged),
        }
    }

    /// Enter one arm: bind payloads (or the whole scrutinee), run the
    /// guard (failure re-enters the chain at `next_bb`), lower the
    /// body, and jump to the merge.
    #[allow(clippy::too_many_arguments)]
    fn enter_match_arm(
        &mut self,
        arm: &MatchArm<'t>,
        sv: Value,
        payload_binds: &[(usize, String)],
        whole_bind: Option<String>,
        want_v: bool,
        merge_eu: Option<TypeId>,
        merge: &mut Option<(Block, Option<Value>)>,
        next_bb: Option<Block>,
        span: Span,
    ) -> R<()> {
        self.scopes.push(ScopeFrame::default());
        self.b.gvn_push_scope();
        let result = self.enter_match_arm_inner(
            arm,
            sv,
            payload_binds,
            whole_bind,
            want_v,
            merge_eu,
            merge,
            next_bb,
            span,
        );
        self.b.gvn_pop_scope();
        self.scopes.pop();
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn enter_match_arm_inner(
        &mut self,
        arm: &MatchArm<'t>,
        sv: Value,
        payload_binds: &[(usize, String)],
        whole_bind: Option<String>,
        want_v: bool,
        merge_eu: Option<TypeId>,
        merge: &mut Option<(Block, Option<Value>)>,
        next_bb: Option<Block>,
        span: Span,
    ) -> R<()> {
        // Payload bindings: field 1+slot of the scrutinee aggregate,
        // licensed by the dominating tag test.
        let sv_ty = self.b.func.value_ty(sv);
        for (slot, name) in payload_binds {
            let types::TypeData::Agg(fields) = self.b.module.types.get(sv_ty).clone() else {
                return Err(refuse("payload bindings on a payload-free scrutinee", span));
            };
            let Some(&fty) = fields.get(slot + 1) else {
                return Err(refuse("a payload slot the scrutinee does not carry", span));
            };
            let pv = self
                .b
                .ins(Opcode::AggGet, &[sv], &[fty], Aux::Int((slot + 1) as i64))
                .one();
            let var = self.b.declare_var(fty);
            self.b.def_var(var, pv);
            self.scopes.last_mut().expect("scope").binds.push((
                name.clone(),
                LocalBind::Val {
                    var,
                    wrapping: false,
                    unsigned: false,
                    wir_ty: fty,
                },
            ));
        }
        if let Some(name) = whole_bind {
            let var = self.b.declare_var(sv_ty);
            self.b.def_var(var, sv);
            self.scopes.last_mut().expect("scope").binds.push((
                name,
                LocalBind::Val {
                    var,
                    wrapping: false,
                    unsigned: false,
                    wir_ty: sv_ty,
                },
            ));
        }
        // Guard: an ordinary branch; failure re-enters the decision
        // chain at the next candidate.
        if let Some(g) = arm.guard() {
            let Some(next) = next_bb else {
                return Err(refuse(
                    "a guard on the closing unconditional arm (coverage came from it)",
                    span,
                ));
            };
            let gexpr = g.nodes().find(|n| wolf_ast::is_expr_kind(n.kind));
            let Some(gexpr) = gexpr else {
                return Err(refuse("a guard without a condition", span));
            };
            match self.lower_expr(gexpr)? {
                Flow::Val(Some(gv)) => {
                    let body_bb = self.b.create_block();
                    self.b.ins_br(gv, body_bb, &[], next, &[]);
                    self.b.seal_block(body_bb);
                    self.b.switch_to_block(body_bb);
                }
                Flow::Val(None) => {
                    return Err(refuse("a guard without a condition value", span));
                }
                Flow::Diverged => return Ok(()),
            }
        }
        let Some(body) = arm.body() else {
            return Ok(());
        };
        let flow = self.lower_expr_w(body, want_v)?;
        match flow {
            Flow::Diverged => Ok(()),
            Flow::Val(v) => {
                let v = if want_v {
                    self.arm_to_merge(v, merge_eu, span)?
                } else {
                    v
                };
                let (mb, param) = match *merge {
                    Some((mb, param)) => (mb, param),
                    None => {
                        let mb = self.b.create_block();
                        let param = match (want_v, v) {
                            (true, Some(val)) => {
                                let ty = self.b.func.value_ty(val);
                                Some(self.b.add_block_param(mb, ty))
                            }
                            _ => None,
                        };
                        *merge = Some((mb, param));
                        (mb, param)
                    }
                };
                match (param, v) {
                    (Some(_), Some(val)) => self.b.ins_jmp(mb, &[val]),
                    (None, _) => self.b.ins_jmp(mb, &[]),
                    (Some(_), None) => {
                        return Err(refuse("a valueless arm in a valued match", span));
                    }
                }
                Ok(())
            }
        }
    }

    // --------------------------------------- for loops (s27, D25) ----

    /// `for pat in a..b { body }` — the closed builtin range family
    /// lowers structurally ([mem.iter.range]): bounds evaluated once,
    /// ascending +1 steps, checked arithmetic. Other iterables await
    /// the `Iter[T]` drive loop's std surface ([mem.iter.for]).
    fn lower_for(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = ForExpr::cast(e).expect("kind");
        let Some(iter) = d.iterable() else {
            return Ok(Flow::Val(None));
        };
        if iter.kind != SyntaxKind::RangeExpr {
            return Err(refuse(
                "`for` over non-range iterables (the `Iter[T]` drive loop — List/Pool adopt \
                 builtin-side, c06/std)",
                e.span,
            ));
        }
        let r = RangeExpr::cast(iter).expect("kind");
        let eps: Vec<&GreenNode> = r.endpoints().collect();
        if eps.len() != 2 || eps.iter().any(|n| n.kind == SyntaxKind::FromEndExpr) {
            return Err(refuse("`for` over open or end-relative ranges", iter.span));
        }
        let lo = flow_val!(self.lower_expr(eps[0]));
        let Some(lo) = lo else {
            return Err(refuse("a range without a start value", iter.span));
        };
        let hi = flow_val!(self.lower_expr(eps[1]));
        let Some(hi) = hi else {
            return Err(refuse("a range without an end value", iter.span));
        };
        let ity = self.b.func.value_ty(lo);
        if !types_is_int(ity) || self.b.func.value_ty(hi) != ity {
            return Err(refuse("non-integer `for` ranges", iter.span));
        }
        let unsigned = self
            .expr_sema_ty(eps[0].span)
            .map(|t| sema_unsigned(self.table, t))
            .unwrap_or(false);
        let bind_name = match d.pattern() {
            None => None,
            Some(p) if p.kind == SyntaxKind::IdentPat => Some(self.text(p.span)),
            Some(p) if p.kind == SyntaxKind::WildcardPat => None,
            Some(p) => {
                return Err(refuse(
                    "destructuring `for` patterns (tuple yields, c06/std)",
                    p.span,
                ));
            }
        };
        if r.is_inclusive() {
            self.lower_for_inclusive(d, lo, hi, ity, unsigned, bind_name)
        } else {
            self.lower_for_exclusive(d, lo, hi, ity, unsigned, bind_name)
        }
    }

    /// `a..b`: `header(i): br i < b, body, exit; latch: i+1 → header`.
    #[allow(clippy::too_many_arguments)]
    fn lower_for_exclusive(
        &mut self,
        d: ForExpr<'t>,
        lo: Value,
        hi: Value,
        ity: TypeId,
        unsigned: bool,
        bind_name: Option<String>,
    ) -> R<Flow> {
        let header = self.b.create_block();
        let iparam = self.b.add_block_param(header, ity);
        self.b.ins_jmp(header, &[lo]);
        self.b.switch_to_block(header);
        self.b.gvn_push_scope();
        let cc = if unsigned { IntCc::Ult } else { IntCc::Slt };
        let cond = self
            .b
            .ins(Opcode::Icmp, &[iparam, hi], &[types::BOOL], Aux::IntCc(cc))
            .one();
        let body_bb = self.b.create_block();
        let exit = self.b.create_block();
        self.b.ins_br(cond, body_bb, &[], exit, &[]);
        self.b.seal_block(body_bb);
        self.b.switch_to_block(body_bb);
        let frame = self.run_for_body(d, iparam, ity, unsigned, bind_name, Some(exit));
        let frame = match frame {
            Ok(f) => f,
            Err(x) => {
                self.b.gvn_pop_scope();
                return Err(x);
            }
        };
        // The latch: increment and loop (created only if some path
        // continues).
        if let ContinueTo::ForLatch(Some(latch)) = frame.continue_to {
            self.b.seal_block(latch);
            self.b.switch_to_block(latch);
            self.b.gvn_push_scope();
            let one = self.b.iconst(ity, 1);
            let op = if unsigned {
                Opcode::UaddChk
            } else {
                Opcode::IaddChk
            };
            match self.b.ins(op, &[iparam, one], &[ity], Aux::None) {
                InsOut::Vals(v) => self.b.ins_jmp(header, &[v[0]]),
                InsOut::Trapped => {}
            }
            self.b.gvn_pop_scope();
        }
        self.b.gvn_pop_scope();
        self.b.seal_block(header);
        self.b.seal_block(exit);
        self.b.switch_to_block(exit);
        Ok(Flow::Val(None))
    }

    /// `a..=b`: pre-test `a <= b`, body-first header, latch tests
    /// `i == b` before incrementing — `i + 1` never overflows.
    #[allow(clippy::too_many_arguments)]
    fn lower_for_inclusive(
        &mut self,
        d: ForExpr<'t>,
        lo: Value,
        hi: Value,
        ity: TypeId,
        unsigned: bool,
        bind_name: Option<String>,
    ) -> R<Flow> {
        // Constant bounds decide the pre-test statically — no folded
        // const left behind.
        let static_pre = match (self.b.as_int_const(lo), self.b.as_int_const(hi)) {
            (Some(a), Some(b)) if unsigned => Some((a as u64) <= (b as u64)),
            (Some(a), Some(b)) => Some(a <= b),
            _ => None,
        };
        if static_pre == Some(false) {
            // The loop provably never runs.
            self.b.stats.identity += 1;
            return Ok(Flow::Val(None));
        }
        let header = self.b.create_block();
        let iparam = self.b.add_block_param(header, ity);
        // With a constant-true pre-test the exit is created lazily
        // (breaks / the latch demand it) so no unreachable block
        // survives.
        let pre_exit = if static_pre == Some(true) {
            self.b.stats.identity += 1;
            self.b.ins_jmp(header, &[lo]);
            None
        } else {
            let pre_cc = if unsigned { IntCc::Ule } else { IntCc::Sle };
            let cond0 = self
                .b
                .ins(Opcode::Icmp, &[lo, hi], &[types::BOOL], Aux::IntCc(pre_cc))
                .one();
            let exit = self.b.create_block();
            self.b.ins_br(cond0, header, &[lo], exit, &[]);
            Some(exit)
        };
        self.b.switch_to_block(header);
        self.b.gvn_push_scope();
        let frame = self.run_for_body(d, iparam, ity, unsigned, bind_name, pre_exit);
        let mut frame = match frame {
            Ok(f) => f,
            Err(x) => {
                self.b.gvn_pop_scope();
                return Err(x);
            }
        };
        if let ContinueTo::ForLatch(Some(latch)) = frame.continue_to {
            self.b.seal_block(latch);
            self.b.switch_to_block(latch);
            self.b.gvn_push_scope();
            let done = self
                .b
                .ins(
                    Opcode::Icmp,
                    &[iparam, hi],
                    &[types::BOOL],
                    Aux::IntCc(IntCc::Eq),
                )
                .one();
            let exit = match frame.exit {
                Some(x) => x,
                None => {
                    let x = self.b.create_block();
                    frame.exit = Some(x);
                    x
                }
            };
            let inc_bb = self.b.create_block();
            self.b.ins_br(done, exit, &[], inc_bb, &[]);
            self.b.seal_block(inc_bb);
            self.b.switch_to_block(inc_bb);
            let one = self.b.iconst(ity, 1);
            let op = if unsigned {
                Opcode::UaddChk
            } else {
                Opcode::IaddChk
            };
            match self.b.ins(op, &[iparam, one], &[ity], Aux::None) {
                InsOut::Vals(v) => self.b.ins_jmp(header, &[v[0]]),
                InsOut::Trapped => {}
            }
            self.b.gvn_pop_scope();
        }
        self.b.gvn_pop_scope();
        self.b.seal_block(header);
        match frame.exit {
            Some(exit) => {
                self.b.seal_block(exit);
                self.b.switch_to_block(exit);
                Ok(Flow::Val(None))
            }
            None => Ok(Flow::Diverged),
        }
    }

    /// The shared `for`-body walk: bind the induction variable, push
    /// the loop frame (continue → latch, break → exit), lower the
    /// body, and emit the fall-through edge to the latch.
    fn run_for_body(
        &mut self,
        d: ForExpr<'t>,
        iparam: Value,
        ity: TypeId,
        unsigned: bool,
        bind_name: Option<String>,
        exit: Option<Block>,
    ) -> R<LoopFrame> {
        self.scopes.push(ScopeFrame::default());
        if let Some(name) = bind_name {
            let var = self.b.declare_var(ity);
            self.b.def_var(var, iparam);
            // s30 debug aux: the induction variable is the loop-carried
            // block param — `print i` in the body reads the live value.
            self.b.func.add_debug_var(name.clone(), iparam, false);
            self.scopes.last_mut().expect("scope").binds.push((
                name,
                LocalBind::Val {
                    var,
                    wrapping: false,
                    unsigned,
                    wir_ty: ity,
                },
            ));
        }
        self.loops.push(LoopFrame {
            continue_to: ContinueTo::ForLatch(None),
            exit,
            exit_param: None,
            depth: self.scopes.len(),
        });
        self.b.gvn_push_scope();
        let flow = match d.body() {
            Some(bl) => self.lower_block(bl, false),
            None => Ok(Flow::Val(None)),
        };
        let flow = match flow {
            Ok(f) => f,
            Err(x) => {
                self.b.gvn_pop_scope();
                self.loops.pop();
                self.scopes.pop();
                return Err(x);
            }
        };
        if let Flow::Val(_) = flow {
            let latch = self.continue_target();
            self.b.ins_jmp(latch, &[]);
        }
        self.b.gvn_pop_scope();
        let frame = self.loops.pop().expect("frame");
        self.scopes.pop();
        Ok(frame)
    }

    // -------------------------------- flat memory helpers (s27) ----

    /// `ptr + off` (byte offset) — the packed v0 spill layout.
    fn field_addr(&mut self, ptr: Value, off: u64) -> Value {
        if off == 0 {
            return ptr;
        }
        let i = self.b.iconst(types::I64, off as i64);
        self.b.ins_ptr_off(ptr, i, 1)
    }

    /// Load a flat value (scalar, or aggregate rebuilt field-wise).
    fn load_flat(&mut self, ty: TypeId, ptr: Value, region: RegionId, span: Span) -> R<Value> {
        if scalar_size(ty).is_some() {
            return Ok(self.b.ins_load(ty, ptr, region));
        }
        let types::TypeData::Agg(fields) = self.b.module.types.get(ty).clone() else {
            return Err(refuse("loading a non-flat type", span));
        };
        let Some(offs) = flat_offsets(&self.b.module.types, &fields) else {
            return Err(refuse("loading a non-flat aggregate", span));
        };
        let mut parts = Vec::with_capacity(fields.len());
        for (k, &fty) in fields.iter().enumerate() {
            let addr = self.field_addr(ptr, offs[k]);
            parts.push(self.load_flat(fty, addr, region, span)?);
        }
        Ok(self.b.ins(Opcode::AggMake, &parts, &[ty], Aux::None).one())
    }

    /// Store a flat value field-wise (scalar loads/stores only — the
    /// text format's typed mnemonics are scalar).
    fn store_flat(&mut self, val: Value, ptr: Value, region: RegionId, span: Span) -> R<()> {
        let ty = self.b.func.value_ty(val);
        if scalar_size(ty).is_some() {
            self.b.ins_store(val, ptr, region);
            return Ok(());
        }
        let types::TypeData::Agg(fields) = self.b.module.types.get(ty).clone() else {
            return Err(refuse("storing a non-flat type", span));
        };
        let Some(offs) = flat_offsets(&self.b.module.types, &fields) else {
            return Err(refuse("storing a non-flat aggregate", span));
        };
        for (k, &fty) in fields.iter().enumerate() {
            let part = self
                .b
                .ins(Opcode::AggGet, &[val], &[fty], Aux::Int(k as i64))
                .one();
            let addr = self.field_addr(ptr, offs[k]);
            self.store_flat(part, addr, region, span)?;
        }
        Ok(())
    }

    /// Read a whole `mut`-parameter value (scalars load directly;
    /// aggregates rebuild field-wise).
    fn read_mut_ref(&mut self, ptr: Value, region: RegionId, elem: TypeId, span: Span) -> R<Value> {
        if scalar_size(elem).is_some() {
            return Ok(self.b.ins_load(elem, ptr, region));
        }
        self.load_flat(elem, ptr, region, span)
    }

    /// The zero/default bit pattern of a flat type (enum payload slots
    /// a variant does not fill).
    fn zero_of(&mut self, t: TypeId, span: Span) -> R<Value> {
        if types_is_int(t) {
            return Ok(self.b.iconst(t, 0));
        }
        if t == types::BOOL {
            return Ok(self.b.bconst(false));
        }
        if t == types::F32 || t == types::F64 {
            return Ok(self.b.fconst(t, 0));
        }
        if let types::TypeData::Agg(fields) = self.b.module.types.get(t).clone() {
            let mut parts = Vec::with_capacity(fields.len());
            for &f in &fields {
                parts.push(self.zero_of(f, span)?);
            }
            return Ok(self.b.ins(Opcode::AggMake, &parts, &[t], Aux::None).one());
        }
        Err(refuse("a zero value of this type", span))
    }

    /// `assert(cond)` / `assert(cond, msg)` — the one user-raised trap
    /// (`[conf.trap.assert]`): `br cond, continue, trap`. A constant
    /// condition folds to nothing (true) or a plain `trap` (false) —
    /// X3 semantics exactly. The message renders once traps carry
    /// payloads (fmt, c06); an effect-free literal is dropped today.
    fn lower_assert(&mut self, d: CallExpr<'t>) -> R<Flow> {
        let mut args = d.args().into_iter().flat_map(|l| l.args());
        let Some(first) = args.next() else {
            return Ok(Flow::Val(None));
        };
        // The optional message: evaluated only on the failing path —
        // a literal has no effects, so dropping it is that evaluation.
        for extra in args {
            let Some(m) = Arg::value(extra) else { continue };
            if !matches!(m.kind, SyntaxKind::StringExpr | SyntaxKind::LiteralExpr) {
                return Err(refuse(
                    "assert messages with effects (trap payload rendering, c06)",
                    m.span,
                ));
            }
        }
        let Some(vexpr) = Arg::value(first) else {
            return Ok(Flow::Val(None));
        };
        let v = flow_val!(self.lower_expr(vexpr));
        let Some(v) = v else {
            return Err(refuse("assert of a valueless expression", vexpr.span));
        };
        match self.b.as_bool_const(v) {
            Some(true) => {
                self.b.stats.fold += 1;
            }
            Some(false) => {
                self.b.stats.fold += 1;
                self.b.ins_trap(TrapKind::Assert);
                return Ok(Flow::Diverged);
            }
            None => {
                let cont = self.b.create_block();
                let trap_bb = self.b.create_block();
                self.b.ins_br(v, cont, &[], trap_bb, &[]);
                self.b.seal_block(trap_bb);
                self.b.switch_to_block(trap_bb);
                self.b.ins_trap(TrapKind::Assert);
                self.b.seal_block(cont);
                self.b.switch_to_block(cont);
            }
        }
        Ok(Flow::Val(None))
    }
}

fn int_bits(t: TypeId) -> Option<u32> {
    match t {
        types::I8 => Some(8),
        types::I16 => Some(16),
        types::I32 => Some(32),
        types::I64 => Some(64),
        _ => None,
    }
}

/// Does this subtree contain any construct that forces new blocks?
/// (The semi-pruned pre-scan: a function without any is one block, so
/// every variable is single-block.)
fn contains_control(node: &GreenNode) -> bool {
    fn is_control(k: SyntaxKind) -> bool {
        matches!(
            k,
            SyntaxKind::IfExpr
                | SyntaxKind::WhileExpr
                | SyntaxKind::LoopExpr
                | SyntaxKind::ForExpr
                | SyntaxKind::MatchExpr
                | SyntaxKind::ElseExpr
                | SyntaxKind::TryExpr
                | SyntaxKind::ReturnExpr
                | SyntaxKind::BreakExpr
                | SyntaxKind::ContinueExpr
                | SyntaxKind::CallExpr
                | SyntaxKind::ClosureExpr
                // Defers re-lower at exit edges (s27): their fragments
                // may live in blocks other than their declaration's.
                | SyntaxKind::DeferStmt
        )
    }
    fn walk(n: &GreenNode) -> bool {
        if is_control(n.kind) {
            return true;
        }
        // `!` and `&&`/`||` lower through branches too.
        if n.kind == SyntaxKind::PrefixExpr
            && n.tokens().next().is_some_and(|t| t.kind == SyntaxKind::Not)
        {
            return true;
        }
        if n.kind == SyntaxKind::BinExpr
            && n.tokens()
                .any(|t| matches!(t.kind, SyntaxKind::AmpAmp | SyntaxKind::PipePipe))
        {
            return true;
        }
        n.nodes().any(walk)
    }
    walk(node)
}

/// Integer literal: decimal with `_` separators, or `0x…` hex (bit
/// pattern, admitted up to 64 bits).
fn parse_int_literal(text: &str) -> Option<i64> {
    let t: String = text.chars().filter(|&c| c != '_').collect();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(hex, 16).ok().map(|v| v as i64);
    }
    t.parse::<i64>().ok()
}

fn parse_float_literal(text: &str) -> Option<f64> {
    let t: String = text.chars().filter(|&c| c != '_').collect();
    t.parse::<f64>().ok()
}

/// Unsigned literal: full u64 range, decimal or `0x…` hex.
fn parse_uint_literal(text: &str) -> Option<u64> {
    let t: String = text.chars().filter(|&c| c != '_').collect();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(hex, 16).ok();
    }
    t.parse::<u64>().ok()
}

/// Dedent a `"""` string's inner bytes by the closing delimiter's
/// column (D26). Byte-identical with the checked executor's
/// implementation (wolf_mem::ubcheck).
fn dedent_multiline(inner: &[u8]) -> Vec<u8> {
    let mut inner = inner;
    if inner.starts_with(b"\r\n") {
        inner = &inner[2..];
    } else if inner.first() == Some(&b'\n') {
        inner = &inner[1..];
    }
    let last_nl = inner.iter().rposition(|&b| b == b'\n');
    let (body, indent) = match last_nl {
        Some(i) => inner.split_at(i + 1),
        None => return inner.to_vec(),
    };
    if !indent.iter().all(|&b| b == b' ' || b == b'\t') {
        return inner.to_vec();
    }
    let mut out = Vec::with_capacity(body.len());
    let mut start = 0;
    while start < body.len() {
        let end = body[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| start + p + 1)
            .unwrap_or(body.len());
        let line = &body[start..end];
        let stripped = if line.starts_with(indent) {
            &line[indent.len()..]
        } else {
            line
        };
        out.extend_from_slice(stripped);
        start = end;
    }
    out
}

/// The escape decoder over a hole-free byte run — the multiline
/// path's helper. Byte-identical with the checked executor's.
fn decode_escapes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\\' && i + 1 < bytes.len() {
            if let Some((ch, consumed)) = decode_codepoint_escape(&bytes[i..]) {
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                i += consumed;
                continue;
            }
            out.push(match bytes[i + 1] {
                b'n' => b'\n',
                b't' => b'\t',
                b'r' => b'\r',
                b'0' => 0,
                other => other,
            });
            i += 2;
            continue;
        }
        // `{{` / `}}` are literal braces ([gram.lex.str]).
        if (c == b'{' || c == b'}') && bytes.get(i + 1) == Some(&c) {
            out.push(c);
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Decode a `\xNN` or `\u{…}` escape at the start of `bytes` (which
/// begins at the backslash). Returns the code point and the total
/// bytes consumed, or `None` for any other shape — the caller falls
/// back to the single-byte escape set. Byte-identical with the
/// checked executor's decoder (wolf_mem::ubcheck).
fn decode_codepoint_escape(bytes: &[u8]) -> Option<(char, usize)> {
    match bytes.get(1)? {
        b'x' => {
            let hex = bytes.get(2..4)?;
            let s = std::str::from_utf8(hex).ok()?;
            let n = u32::from_str_radix(s, 16).ok()?;
            Some((char::from_u32(n)?, 4))
        }
        b'u' => {
            if bytes.get(2) != Some(&b'{') {
                return None;
            }
            let close = bytes[3..].iter().position(|&b| b == b'}')?;
            let s = std::str::from_utf8(&bytes[3..3 + close]).ok()?;
            if s.is_empty() || s.len() > 6 {
                return None;
            }
            let n = u32::from_str_radix(s, 16).ok()?;
            Some((char::from_u32(n)?, 3 + close + 1))
        }
        _ => None,
    }
}

/// Sign-wrap a bit pattern into a `bits`-wide iconst payload.
fn wrap_bits(v: u64, bits: u32) -> i64 {
    if bits >= 64 {
        return v as i64;
    }
    let m = 1u64 << bits;
    let r = v & (m - 1);
    if r >= m / 2 {
        (r as i64) - (m as i64)
    } else {
        r as i64
    }
}
