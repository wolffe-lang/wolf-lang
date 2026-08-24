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
    Arg, AssignStmt, Block as AstBlock, BracketApply, BreakExpr, CallExpr, CastExpr, ConstDecl,
    DeferStmt, ElseExpr, ExprStmt, ForExpr, FromEndExpr, GreenNode, IfExpr, LetDecl, LoopExpr,
    MatchArm, MatchExpr, ParamMode, ParenExpr, PrefixExpr, RangeExpr, ReturnExpr, StringExpr,
    SyntaxKind, TryExpr, VarDecl, WhileExpr,
};
use wolf_mem::byteview::{Lend, Lender};
use wolf_sema::check::{CallSig, CastKind, Dispatch};
use wolf_sema::sig::{FnSig, ItemSig, SigTables};
use wolf_sema::types::{Prim, TyId, TyKind, TypeTable};
use wolf_sema::{BodyResult, Fold, NotYet, Package, Typecheck, TypedBody};
use wolf_span::Span;

use crate::build::{FuncBuilder, InsOut, Stats, Var};
use crate::entity::EntityRef;
use crate::facts::{DerefSize, FactData, FactKind, Just, Theorem};
use crate::ir::{Aux, Block, ExtFunc, Mode, Module, Param, SigId};
use crate::ops::{FloatCc, ForeignRole, IntCc, Opcode, TrapKind};
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

/// One reason collected by the survey lens ([`lower_package_survey`]).
///
/// ORDER MATTERS: reasons appear in lowering order — the source order
/// of the failure points within a body. The first reason of a body is
/// exactly what fail-fast lowering refuses with; it is reliable.
/// Everything after it was collected by SKIPPING the failed statement
/// and lowering on, so a later reason may be follow-on noise (a name
/// the skipped statement would have bound, a value it would have
/// produced). `follow_on` marks those: they are leads, not verdicts.
#[derive(Debug, Clone)]
pub struct SurveyReason {
    pub construct: &'static str,
    pub span: Span,
    /// The qualified name of the body the reason was found in (the
    /// spec name, for a worklist instance).
    pub fn_name: String,
    /// Not the first reason in its body — possibly noise from a
    /// skipped statement rather than an independent gap.
    pub follow_on: bool,
}

/// Lower every checked body of a package. `tc` must come from
/// [`wolf_sema::typecheck_package`] over the same `pkg`; callers gate
/// on `tc.not_yet`/`mem` cleanliness for the rung verdict — this
/// function lowers whatever is lowerable and refuses the rest.
pub fn lower_package(pkg: &Package, tc: &Typecheck) -> Build {
    lower_package_impl(pkg, tc, None)
}

/// [`lower_package`] with the survey lens on (the c19 closeout's
/// lesson made a tool): a refusal names the FIRST reason a body
/// stops, not the only one, so contract acceptance written against
/// the ledger can be written against a mask. Survey mode catches a
/// refusal at the statement that raised it, records it, skips the
/// statement, and lowers on — collecting what fail-fast masks.
///
/// A lens, never a gate. The returned [`Build`] is bit-for-bit what
/// [`lower_package`] returns — same `not_yet`, same module — because
/// a surveyed body's verdict is pinned to its first reason and its
/// (garbage) function is never added. Survey output is not
/// snapshotted anywhere; `wolf conform-run --dump=peel` prints it and
/// `cargo xtask peel` drives that over the corpus.
pub fn lower_package_survey(pkg: &Package, tc: &Typecheck) -> (Build, Vec<SurveyReason>) {
    let mut reasons = Vec::new();
    let build = lower_package_impl(pkg, tc, Some(&mut reasons));
    (build, reasons)
}

fn lower_package_impl(
    pkg: &Package,
    tc: &Typecheck,
    mut survey: Option<&mut Vec<SurveyReason>>,
) -> Build {
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
    // s89: the byte-view lend verdicts, computed once for the package
    // and shared with the memory checker's W1004 (`wolf_mem::byteview`
    // is the single authority — see this crate's Cargo comment).
    let lender = Lender::new(pkg, &tc.sigs);
    // s89: free-function bodies by their qualified WIR name, so a view
    // specialization requested at a call site can find the body to
    // lower a second time.
    let mut bodies: HashMap<String, (&TypedBody, &wolf_sema::BodyRef)> = HashMap::new();
    for outcome in &tc.bodies {
        let BodyResult::Checked(tb) = &outcome.result else {
            continue;
        };
        if outcome.body.member.is_none() {
            bodies.insert(
                qualify(&tc.sigs, outcome.body.module, &outcome.body.name),
                (tb, &outcome.body),
            );
        } else if let Some(mi) = outcome.body.member {
            // s94: method bodies join the worklist's map under the
            // mangled `Type.method` name, so a generic method's
            // instance can find its checked body the way a free fn's
            // does. s95: trait-impl methods join under
            // `Type.Trait.method`, and a trait's DEFAULT body joins
            // under the trait's own `Trait.method` — one entry per
            // method; the key's `Self` segment tells instances apart.
            if let Some(imp) = tc
                .sigs
                .impls
                .iter()
                .find(|i| i.file == outcome.body.file && i.decl == outcome.body.decl)
            {
                if let Some(m) = imp.methods.iter().find(|m| m.member == mi)
                    && let Some(tyname) = self_ty_key(&tc.sigs.table, imp.self_ty)
                {
                    let key = match &imp.trait_ref {
                        Some(tr) => format!("{tyname}.{}.{}", tr.name, m.name),
                        None => format!("{tyname}.{}", m.name),
                    };
                    bodies.insert(key, (tb, &outcome.body));
                }
            } else {
                let root = &pkg.files[outcome.body.file].parse.root;
                if let Some(node) = root
                    .nodes()
                    .filter(|n| n.kind.is_item())
                    .nth(outcome.body.decl)
                    && node.kind == SyntaxKind::TraitDecl
                    && let Some(tname) = wolf_ast::TraitDecl::cast(node).and_then(|t| t.name())
                    && let Some(mnode) = node.nodes().filter(|n| n.kind.is_item()).nth(mi)
                    && mnode.kind == SyntaxKind::FnDecl
                    && let Some(mtok) = wolf_ast::FnDecl::cast(mnode).and_then(|d| d.name())
                {
                    let src = &pkg.files[outcome.body.file].raw.src;
                    let tname = String::from_utf8_lossy(
                        &src[tname.span.lo as usize..tname.span.hi as usize],
                    );
                    let mname =
                        String::from_utf8_lossy(&src[mtok.span.lo as usize..mtok.span.hi as usize]);
                    bodies.insert(format!("{tname}.{mname}"), (tb, &outcome.body));
                }
            }
        }
    }
    let mut sig_cache: HashMap<String, SigId> = HashMap::new();
    let mut specs: Vec<SpecRequest> = Vec::new();
    let plain = SpecKey::view(0);
    for outcome in &tc.bodies {
        let BodyResult::Checked(tb) = &outcome.result else {
            continue;
        };
        let body = &outcome.body;
        let mut peeled: Vec<NotYet> = Vec::new();
        let sink = survey.is_some().then_some(&mut peeled);
        let r = lower_body(
            pkg,
            &tc.sigs,
            tb,
            body,
            &fns,
            &mut module,
            &mut sig_cache,
            &lender,
            &plain,
            &[],
            &mut specs,
            sink,
        );
        if let Some(out) = survey.as_deref_mut() {
            // A pre-body refusal never reaches the statement catch;
            // record it so the survey is a superset of the ledger.
            if peeled.is_empty()
                && let Err(ref nyc) = r
            {
                peeled.push(nyc.clone());
            }
            let fname = qualify(&tc.sigs, body.module, &body.name);
            for (i, n) in peeled.iter().enumerate() {
                out.push(SurveyReason {
                    construct: n.construct,
                    span: n.span,
                    fn_name: fname.clone(),
                    follow_on: i > 0,
                });
            }
        }
        match r {
            Ok(Some(s)) => stats.add(s),
            Ok(None) => {}
            Err(nyc) => not_yet.push(nyc),
        }
    }
    // The specialization worklist (s89, generalized by s93). A view-
    // taking clone may re-lend its view onward, and a monomorphic
    // instance may call another generic — so requests drain to
    // fixpoint; the `done` set makes the pass idempotent (N call sites
    // of one callee under one key emit ONE body). The 4096 guard is
    // s89's, and it is also the bound on polymorphic recursion (`f[T]`
    // calling `f[List[T]]` never reaches a fixpoint): hitting it is a
    // named refusal, not a hang.
    //
    // Reachability-driven exactly as s43 T2 said: only the (callee,
    // key) pairs some call site named are here. An uncalled generic
    // contributes no WIR — D8's release-tier rule, and `comptime fn`'s
    // precedent in `lower_body`.
    let mut done: std::collections::HashSet<(String, SpecKey)> = std::collections::HashSet::new();
    let mut named: HashMap<String, SpecKey> = HashMap::new();
    let mut guard = 0usize;
    while let Some(req) = specs.pop() {
        guard += 1;
        if guard > 4096 {
            let construct = if req.key.subst.is_empty() {
                "a byte-view specialization fixpoint"
            } else {
                "a monomorphization fixpoint (polymorphic recursion)"
            };
            not_yet.push(refuse(construct, req.span));
            if let Some(out) = survey.as_deref_mut() {
                out.push(SurveyReason {
                    construct,
                    span: req.span,
                    fn_name: req.name.clone(),
                    follow_on: false,
                });
            }
            break;
        }
        stats.instantiations_seen += u64::from(!req.key.subst.is_empty());
        if !done.insert((req.name.clone(), req.key.clone())) {
            continue;
        }
        // Two distinct keys must not share a mangled name (the name
        // squeeze in `mono_segment` can fold punctuation-only
        // differences); refuse by name rather than emit two bodies
        // under one symbol.
        let full = spec_name(&req.name, &req.key);
        if let Some(prev) = named.get(&full)
            && *prev != req.key
        {
            let construct = "two instantiations whose bindings mangle to one name";
            not_yet.push(refuse(construct, req.span));
            if let Some(out) = survey.as_deref_mut() {
                out.push(SurveyReason {
                    construct,
                    span: req.span,
                    fn_name: req.name.clone(),
                    follow_on: false,
                });
            }
            continue;
        }
        named.insert(full, req.key.clone());
        let Some(&(tb, body)) = bodies.get(&req.name) else {
            // The callee's body did not check, so there is nothing to
            // specialize; the call site's own lowering already refused.
            continue;
        };
        stats.instantiations_lowered += u64::from(!req.key.subst.is_empty());
        let mut peeled: Vec<NotYet> = Vec::new();
        let sink = survey.is_some().then_some(&mut peeled);
        let r = lower_body(
            pkg,
            &tc.sigs,
            tb,
            body,
            &fns,
            &mut module,
            &mut sig_cache,
            &lender,
            &req.key,
            &req.bindings,
            &mut specs,
            sink,
        );
        if let Some(out) = survey.as_deref_mut() {
            if peeled.is_empty()
                && let Err(ref nyc) = r
            {
                peeled.push(nyc.clone());
            }
            let fname = spec_name(&req.name, &req.key);
            for (i, n) in peeled.iter().enumerate() {
                out.push(SurveyReason {
                    construct: n.construct,
                    span: n.span,
                    fn_name: fname.clone(),
                    follow_on: i > 0,
                });
            }
        }
        match r {
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

/// What one specialization of a body is keyed on (s89 + s93).
///
/// s89: `mask` — the bitmask of parameters that arrive as `{ptr, len}`
/// byte views instead of `List` headers. s93: `subst` — the callee's
/// generic parameters bound to concrete types, in declaration order,
/// spelled canonically so the key (and the mangled name) is the same
/// however the call site wrote them. A mask-only key is exactly the
/// s89 request; a substitution-only key is one monomorphic instance;
/// both together is a byte-view clone OF an instance, and the drain
/// treats them uniformly — one worklist, not two.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SpecKey {
    mask: u32,
    /// (rigid name, canonical spelling of its binding). The spelling
    /// rather than a `TyId` because ids are per-table and this key
    /// crosses tables; the spelling is what the name carries anyway.
    subst: Vec<(String, String)>,
}

impl SpecKey {
    fn view(mask: u32) -> SpecKey {
        SpecKey {
            mask,
            subst: Vec::new(),
        }
    }
    fn is_plain(&self) -> bool {
        self.mask == 0 && self.subst.is_empty()
    }
}

/// A resolved static trait route (s95): the callee's base name, its
/// signature, and the bindings the instance applies.
type TraitRoute<'t> = (String, &'t FnSig, Vec<(String, Bound)>);

/// One call site's demand for a specialization of a callee (s89: a
/// byte-view clone; s93: a monomorphic instance; or both): the callee's
/// qualified WIR name, the key, and — for a substitution — the bindings
/// the substitution actually applies (the key holds their spelling).
struct SpecRequest {
    name: String,
    key: SpecKey,
    /// (rigid name, binding), table-independent — see [`Bound`]. Empty
    /// for a mask-only key.
    bindings: Vec<(String, Bound)>,
    span: Span,
}

/// The WIR name of a specialization. The suffixes ride the dotted-name
/// convention the textual format already parses (`@a.b.c`, the s27
/// method mangling) so a dump still round-trips. `base.bytesview.M` is
/// byte-for-byte the s89 spelling for a mask-only key, so every s89
/// snapshot is unchanged; a substitution appends `.mono.<spelling>`,
/// one segment per parameter, spelled the way [`mono_spelling`] does.
fn spec_name(base: &str, key: &SpecKey) -> String {
    let mut n = base.to_string();
    if key.mask != 0 {
        n.push_str(&format!(".bytesview.{}", key.mask));
    }
    for (_, sp) in &key.subst {
        n.push_str(".mono.");
        n.push_str(&mono_segment(sp));
    }
    n
}

/// The canonical spelling of a type binding: sema's own renderer. This
/// is what the KEY holds, so two bindings are the same instance exactly
/// when sema would print them the same way.
fn mono_spelling(table: &TypeTable, ty: TyId) -> String {
    wolf_sema::types::render(table, ty, &|_| Err("_"))
}

/// A type binding that has left its table (s93). Sema's `TyId`s are
/// per-table (each body has its own interner, the signatures another),
/// and a call site's binding — `T ↦ List[int]` — was interned in the
/// CALLER's body table, which the worklist drain never sees; the drain
/// re-lowers the CALLEE's body against clones of the callee's tables.
/// So a binding crosses as an owned structural tree, frozen from the
/// site's table and thawed into whichever table needs it. `Rigid` and
/// `Var` freeze as themselves: a Var cannot appear in a defaulted site
/// type, and a Rigid in a binding is an unbound parameter that
/// `wir_ty` refuses by name once thawed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Bound {
    Leaf(TyKind),
    Wrapping(Box<Bound>),
    Tuple(Vec<Bound>),
    Fn(Vec<Bound>, Box<Bound>),
    ErrUnion(Box<Bound>, Box<Bound>),
    Row {
        tags: Vec<(String, Vec<Bound>)>,
        tail: Option<Box<Bound>>,
    },
    Range(Box<Bound>),
    Ptr(Box<Bound>),
    /// s94: containers and applied nominals cross tables structurally
    /// — a `List(TyId)` leaf would smuggle a per-table id.
    List(Box<Bound>),
    Pool(Box<Bound>),
    Nominal {
        module: u32,
        name: String,
        args: Vec<Bound>,
    },
}

/// Does this declared type mention the trait receiver rigid `Self`?
/// (The qualified-call route uses it to find the impl-naming argument.)
fn mentions_self(table: &TypeTable, ty: TyId) -> bool {
    match table.kind(ty) {
        TyKind::Rigid(n) => n == "Self",
        TyKind::Proj(b, _) | TyKind::Wrapping(b) | TyKind::Distinct(b) | TyKind::Ptr(b) => {
            mentions_self(table, *b)
        }
        TyKind::List(t) | TyKind::Range(t) | TyKind::Chan(t) | TyKind::Mutex(t) => {
            mentions_self(table, *t)
        }
        TyKind::Tuple(ts) => ts.iter().any(|t| mentions_self(table, *t)),
        TyKind::Fn(ps, r) => {
            ps.iter().any(|t| mentions_self(table, *t)) || mentions_self(table, *r)
        }
        TyKind::ErrUnion(ok, row) => mentions_self(table, *ok) || mentions_self(table, *row),
        TyKind::Row { tags, tail } => {
            tags.iter()
                .any(|(_, ps)| ps.iter().any(|t| mentions_self(table, *t)))
                || tail.is_some_and(|t| mentions_self(table, t))
        }
        TyKind::Nominal { args, .. } => args.iter().any(|t| mentions_self(table, *t)),
        _ => false,
    }
}

fn freeze(table: &TypeTable, ty: TyId) -> Bound {
    match table.kind(ty).clone() {
        TyKind::Wrapping(t) => Bound::Wrapping(Box::new(freeze(table, t))),
        TyKind::List(t) => Bound::List(Box::new(freeze(table, t))),
        TyKind::Pool(t) => Bound::Pool(Box::new(freeze(table, t))),
        TyKind::Nominal { module, name, args } if !args.is_empty() => Bound::Nominal {
            module,
            name,
            args: args.into_iter().map(|t| freeze(table, t)).collect(),
        },
        TyKind::Tuple(ts) => Bound::Tuple(ts.into_iter().map(|t| freeze(table, t)).collect()),
        TyKind::Fn(ps, r) => Bound::Fn(
            ps.into_iter().map(|t| freeze(table, t)).collect(),
            Box::new(freeze(table, r)),
        ),
        TyKind::ErrUnion(a, b) => {
            Bound::ErrUnion(Box::new(freeze(table, a)), Box::new(freeze(table, b)))
        }
        TyKind::Row { tags, tail } => Bound::Row {
            tags: tags
                .into_iter()
                .map(|(n, ps)| (n, ps.into_iter().map(|t| freeze(table, t)).collect()))
                .collect(),
            tail: tail.map(|t| Box::new(freeze(table, t))),
        },
        TyKind::Range(t) => Bound::Range(Box::new(freeze(table, t))),
        TyKind::Ptr(t) => Bound::Ptr(Box::new(freeze(table, t))),
        leaf => Bound::Leaf(leaf),
    }
}

fn thaw(b: &Bound, into: &mut TypeTable) -> TyId {
    let k = match b {
        Bound::Leaf(k) => k.clone(),
        Bound::Wrapping(t) => TyKind::Wrapping(thaw(t, into)),
        Bound::Tuple(ts) => TyKind::Tuple(ts.iter().map(|t| thaw(t, into)).collect()),
        Bound::Fn(ps, r) => TyKind::Fn(ps.iter().map(|t| thaw(t, into)).collect(), thaw(r, into)),
        Bound::ErrUnion(a, b) => TyKind::ErrUnion(thaw(a, into), thaw(b, into)),
        Bound::Row { tags, tail } => {
            let tags = tags
                .iter()
                .map(|(n, ps)| (n.clone(), ps.iter().map(|t| thaw(t, into)).collect()))
                .collect();
            let tail = tail.as_ref().map(|t| thaw(t, into));
            // Rows intern through the table's canonicalizing ctor so a
            // thawed row equals a checker-minted one.
            return into.row(tags, tail);
        }
        Bound::Range(t) => TyKind::Range(thaw(t, into)),
        Bound::Ptr(t) => TyKind::Ptr(thaw(t, into)),
        Bound::List(t) => TyKind::List(thaw(t, into)),
        Bound::Pool(t) => TyKind::Pool(thaw(t, into)),
        Bound::Nominal { module, name, args } => TyKind::Nominal {
            module: *module,
            name: name.clone(),
            args: args.iter().map(|t| thaw(t, into)).collect(),
        },
    };
    into.intern(k)
}

/// One monomorphic instance's view of a checked body (s93): the
/// signature and body tables cloned (append-only interners — every id
/// the checker minted stays valid in the clone with the same kind) with
/// the bindings thawed into them and applied through `subst` to the
/// signature and to every expression/local type the lowerer will read.
/// A `Rigid` therefore never reaches `wir_ty`; if one does, the
/// substitution left a parameter unbound and `wir_ty` names it.
struct Instance {
    sig_tbl: TypeTable,
    fsig: FnSig,
    body_tbl: TypeTable,
    exprs: Vec<(Span, TyId)>,
    locals: Vec<(String, Span, TyId)>,
}

impl Instance {
    fn build(
        sigs: &SigTables,
        fsig: &FnSig,
        tb: &TypedBody,
        bindings: &[(String, Bound)],
    ) -> Instance {
        let mut st = sigs.table.clone();
        let map: std::collections::BTreeMap<String, TyId> = bindings
            .iter()
            .map(|(n, b)| (n.clone(), thaw(b, &mut st)))
            .collect();
        let mut f = fsig.clone();
        for p in &mut f.params {
            p.ty = wolf_sema::types::subst(&mut st, p.ty, &map);
        }
        f.ret = wolf_sema::types::subst(&mut st, f.ret, &map);
        // The body's own table holds the same rigids under the same
        // names (the checker checked the archetype once); the same
        // trees thaw HERE. `subst` is total — a name the map lacks
        // passes through as a Rigid, and `wir_ty` refuses it by name.
        let mut bt = tb.table.clone();
        let bmap: std::collections::BTreeMap<String, TyId> = bindings
            .iter()
            .map(|(n, b)| (n.clone(), thaw(b, &mut bt)))
            .collect();
        let exprs = tb
            .exprs
            .iter()
            .map(|(sp, t)| (*sp, wolf_sema::types::subst(&mut bt, *t, &bmap)))
            .collect();
        let locals = tb
            .locals
            .iter()
            .map(|(n, sp, t)| (n.clone(), *sp, wolf_sema::types::subst(&mut bt, *t, &bmap)))
            .collect();
        Instance {
            sig_tbl: st,
            fsig: f,
            body_tbl: bt,
            exprs,
            locals,
        }
    }
}

/// The spelling as one dotted-name SEGMENT for the mangled name. WIR
/// names are `[A-Za-z0-9_]+` segments joined by `.` (`parse.rs:
/// is_ident_cont`), so every other character becomes `_` and runs
/// collapse: `List[int]` → `List_int`, `(int, str)` → `int_str`,
/// `fn(int) -> int` → `fn_int_int`. Two bindings that differ only in
/// punctuation would share a name; the KEY still tells them apart (it
/// holds the raw spelling), and the drain in [`lower_package`] refuses
/// by name when two distinct keys mangle to one function name — a
/// visible refusal, never two bodies under one symbol.
fn mono_segment(spelling: &str) -> String {
    let mut out = String::with_capacity(spelling.len());
    let mut sep = false;
    for c in spelling.chars() {
        if c.is_ascii_alphanumeric() {
            if sep && !out.is_empty() {
                out.push('_');
            }
            sep = false;
            out.push(c);
        } else if c == '_' {
            sep = false;
            out.push('_');
        } else {
            sep = true;
        }
    }
    if out.is_empty() {
        // s94: a spelling with no identifier characters at all — the
        // empty row `{}` is the one that arises (a tail bound to
        // "nothing more") — must still be a parseable name segment,
        // or the dump's dotted name ends in `.` and does not
        // round-trip.
        out.push_str("empty");
    }
    out
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
    lender: &Lender<'_>,
    key: &SpecKey,
    bindings: &[(String, Bound)],
    specs: &mut Vec<SpecRequest>,
    mut survey: Option<&mut Vec<NotYet>>,
) -> R<Option<Stats>> {
    let view_mask = key.mask;
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
            if node.kind == SyntaxKind::TraitDecl {
                // s95: a trait DEFAULT body. Checked once against the
                // trait's own archetype (`Self` rigid); it lowers only
                // as an INSTANCE — `Self ↦ subject` — demanded by a
                // call site whose impl does not override the method.
                // The plain pass has no subject: archetype rule, no
                // WIR (the s93 shape).
                if bindings.is_empty() {
                    return Ok(None);
                }
                let Some(tname) = wolf_ast::TraitDecl::cast(node)
                    .and_then(|t| t.name())
                    .map(|t| {
                        String::from_utf8_lossy(
                            &pkg.files[body.file].raw.src[t.span.lo as usize..t.span.hi as usize],
                        )
                        .into_owned()
                    })
                else {
                    return Ok(None);
                };
                let tr = wolf_sema::traits::TraitRef {
                    module: body.module,
                    name: tname,
                };
                let Some(td) = sigs.traits.get(&tr) else {
                    return Ok(None);
                };
                let Some(mnode) = node.nodes().filter(|n| n.kind.is_item()).nth(mi) else {
                    return Ok(None);
                };
                if mnode.kind != SyntaxKind::FnDecl {
                    return Ok(None);
                }
                let d = wolf_ast::FnDecl::cast(mnode).expect("kind");
                let Some(mname_tok) = d.name() else {
                    return Ok(None);
                };
                let mname = String::from_utf8_lossy(
                    &pkg.files[body.file].raw.src
                        [mname_tok.span.lo as usize..mname_tok.span.hi as usize],
                )
                .into_owned();
                let Some(tm) = td.method(&mname) else {
                    return Ok(None);
                };
                // The base name is the TRAIT's — `Greeter.greet` — and
                // the key's `Self` segment names the subject:
                // `Greeter.greet.mono.P`. One spelling: a default body
                // is the trait's body monomorphized over `Self`.
                (mnode, &tm.sig, format!("{}.{mname}", tr.name))
            } else if node.kind == SyntaxKind::ImplDecl {
                let Some(imp) = sigs
                    .impls
                    .iter()
                    .find(|i| i.file == body.file && i.decl == body.decl)
                else {
                    return Ok(None);
                };
                if !imp.generics.is_empty() && bindings.is_empty() {
                    // s94: a generic impl's method archetype contributes
                    // no WIR — its instances are the WIR, exactly the
                    // free-fn rule below. The worklist hands an instance
                    // in with bindings covering the impl's rigids.
                    return Ok(None);
                }
                let Some(m) = imp.methods.iter().find(|m| m.member == mi) else {
                    return Ok(None);
                };
                let Some(tyname) = self_ty_key(&sigs.table, imp.self_ty) else {
                    return Err(refuse("methods on non-nominal self types", span));
                };
                let Some(mnode) = node.nodes().filter(|n| n.kind.is_item()).nth(mi) else {
                    return Ok(None);
                };
                if mnode.kind != SyntaxKind::FnDecl {
                    return Ok(None); // associated consts have no body to lower
                }
                // s95: a trait-impl method lowers like an inherent one;
                // trait-ness is name mangling only — `Point.Show.show`
                // (the s96 vtable slot name, too). A coherence-REJECTED
                // program can reach here with two impls of one trait
                // for one type; two bodies under one symbol is worse
                // than a named refusal (E0506 already owns the file).
                let wname = match &imp.trait_ref {
                    Some(tr) => format!("{tyname}.{}.{}", tr.name, m.name),
                    None => format!("{tyname}.{}", m.name),
                };
                if let Some(tr) = &imp.trait_ref
                    && bindings.is_empty()
                    && module.funcs.values().any(|f| f.name == wname)
                {
                    return Err(refuse_named(
                        format!(
                            "a second impl of `{}` for `{tyname}` (coherence rejected this program)",
                            tr.name
                        ),
                        span,
                    ));
                }
                (mnode, &m.sig, wname)
            } else {
                return Ok(None);
            }
        }
    };
    let span = fn_node.span;
    let d = wolf_ast::FnDecl::cast(fn_node).expect("kind");
    let Some(block) = d.body() else {
        return Ok(None);
    };
    // s93: a generic body lowers only as an INSTANCE — the worklist
    // hands one in as a substitution. Reached with none (the plain pass
    // over `tc.bodies`), the ARCHETYPE has nothing to lower: its
    // instances are the WIR, and an uncalled generic contributes none
    // at all — D8's release-tier rule, and exactly the shape of
    // `comptime fn` just below. Not a refusal: a refusal here would
    // mark every file that so much as declares a generic `refused` in
    // the ledger however cleanly its instances lowered, and the ledger
    // must record what did not lower, not what was never meant to.
    // What CAN still refuse by name is a call site that cannot bind a
    // parameter, and an instance whose body reaches a type the
    // substitution did not close — both below, both named.
    if !fsig.generics.is_empty() && bindings.is_empty() {
        return Ok(None);
    }
    if fsig.comptime {
        // A `comptime fn` is CTFE's alone (D29): its call sites fold
        // to constants (s71), so the definition simply does not lower
        // — and its presence no longer poisons the module's runtime
        // build.
        return Ok(None);
    }
    // s93: under a substitution, lower the body's INSTANCE — see
    // [`Instance`] for what is cloned and why a `Rigid` never reaches
    // `wir_ty`.
    let inst: Instance;
    let (sig_tbl, fsig, body_tbl, exprs, locals) = if bindings.is_empty() {
        (&sigs.table, fsig, &tb.table, &tb.exprs[..], &tb.locals[..])
    } else {
        inst = Instance::build(sigs, fsig, tb, bindings);
        (
            &inst.sig_tbl,
            &inst.fsig,
            &inst.body_tbl,
            &inst.exprs[..],
            &inst.locals[..],
        )
    };
    // s89: a byte-view clone is the same body under a different
    // parameter shape — one name, one signature, one entry binding per
    // masked parameter. The mask reached here only through a call site
    // whose callee the lend analysis proved `Lendable`. s93: an instance
    // is the same body under a substitution; the name carries both.
    let wir_name = if key.is_plain() {
        wir_name
    } else {
        spec_name(&wir_name, key)
    };
    // The WIR signature (modes carried; s26 attaches the fact slots).
    let sig = wir_fn_sig(
        module, sig_cache, sig_tbl, sigs, &wir_name, fsig, view_mask, span,
    )?;
    // s73: task bodies queued by spawn sites, synthesized post-pass.
    let mut pending: Vec<PendingTask<'_>> = Vec::new();
    let mut dyn_shims: Vec<DynShim> = Vec::new();
    let mut b = FuncBuilder::new(module, wir_name, sig);
    // s30: spans thread from the typed HIR into WIR (the lossless s07
    // chain) — the file once per function, then a per-statement span
    // cursor the builder stamps on every appended instruction.
    b.func.src_file = Some(span.file.index() as u32);
    b.set_span(fsig.name_span.lo, fsig.name_span.hi);
    let module_binds: HashMap<String, usize> =
        wolf_sema::module_bindings(pkg, body.module, body.file)
            .into_iter()
            .collect();
    let mut lowerer = Lowerer {
        src: &pkg.files[body.file].raw.src,
        module_binds: &module_binds,
        table: body_tbl,
        sig_table: sig_tbl,
        sigs,
        calls: tb.calls.iter().map(|(s, c)| (*s, c)).collect(),
        folds: tb.comptime_folds.iter().map(|(s, f)| (*s, f)).collect(),
        expr_tys: exprs.iter().map(|(s, t)| (*s, *t)).collect(),
        local_tys: locals.iter().map(|(_, s, t)| (*s, *t)).collect(),
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
        capture_binds: 0,
        task_captures: tb.task_captures.iter().map(|(s, c)| (*s, &c[..])).collect(),
        pending_tasks: &mut pending,
        foreign: None,
        lender,
        pending_specs: specs,
        pending_dyn_shims: &mut dyn_shims,
        survey: survey.as_deref_mut(),
        b: &mut b,
    };
    match lowerer.lower_fn(fsig, block, view_mask) {
        Ok(()) => {}
        Err(e) => return Err(body_verdict(survey.as_deref_mut(), e)),
    }
    // Survey: statements refused but the lens lowered on. The function
    // is not real WIR, so it is never added; the verdict is pinned to
    // the FIRST reason — exactly what fail-fast returns — so the
    // survey [`Build`] is bit-for-bit the fail-fast one.
    if let Some(sink) = survey.as_deref()
        && let Some(first) = sink.first()
    {
        return Err(first.clone());
    }
    let mut stats = b.stats;
    let func = b.finish();
    module.add_func(func);
    // s73: synthesize the queued task bodies and entry shims. A body
    // may queue further tasks (nested spawns) — drain to fixpoint.
    let mut guard = 0;
    while let Some(task) = pending.pop() {
        guard += 1;
        if guard > 256 {
            return Err(body_verdict(
                survey.as_deref_mut(),
                refuse("deeply nested task spawns", span),
            ));
        }
        if task.closure.is_some() {
            match lower_task_body(
                pkg,
                sigs,
                tb,
                body,
                fns,
                module,
                &task,
                &mut pending,
                lender,
                specs,
                &mut dyn_shims,
                survey.as_deref_mut(),
            ) {
                Ok(s) => stats.add(s),
                Err(e) => return Err(body_verdict(survey.as_deref_mut(), e)),
            }
        }
        // s105: a closure ENTRY is one function — no runtime shim.
        if matches!(task.kind, PendingKind::Task)
            && let Err(e) = build_task_shim(module, &task)
        {
            return Err(body_verdict(survey.as_deref_mut(), e));
        }
    }
    // s98: the dyn-slot shims this body's casts demanded. After the
    // task drain (a task body may cast); shims themselves never queue
    // more work.
    while let Some(shim) = dyn_shims.pop() {
        if let Err(e) = build_dyn_shim(module, &shim) {
            return Err(body_verdict(survey.as_deref_mut(), e));
        }
    }
    Ok(Some(stats))
}

/// Survey-aware error exit for [`lower_body`]'s post-statement stages
/// (prologue, signature, defers, the task drain): fail-fast returns
/// the error as-is; the survey records it and pins the body's verdict
/// to its first collected reason, which is what fail-fast would have
/// refused with.
fn body_verdict(survey: Option<&mut Vec<NotYet>>, e: NotYet) -> NotYet {
    match survey {
        None => e,
        Some(sink) => {
            sink.push(e);
            sink[0].clone()
        }
    }
}

/// Lower one queued closure task body (s73): an ordinary function
/// whose parameters are the closure's captures (by copy, S-10) and
/// whose fallible shape is the closure's raised row.
#[allow(clippy::too_many_arguments)]
fn lower_task_body<'t>(
    pkg: &'t Package,
    sigs: &'t SigTables,
    tb: &'t TypedBody,
    body: &wolf_sema::BodyRef,
    fns: &'t HashMap<&'t str, Vec<(usize, &'t FnSig)>>,
    module: &mut Module,
    task: &PendingTask<'t>,
    pending: &mut Vec<PendingTask<'t>>,
    lender: &'t Lender<'t>,
    specs: &mut Vec<SpecRequest>,
    dyn_shims: &mut Vec<DynShim>,
    survey: Option<&mut Vec<NotYet>>,
) -> R<Stats> {
    let closure = task.closure.expect("closure task");
    let mut b = FuncBuilder::new(module, task.body_name.clone(), task.body_sig);
    b.func.src_file = Some(task.span.file.index() as u32);
    b.set_span(task.span.lo, task.span.hi);
    let module_binds: HashMap<String, usize> =
        wolf_sema::module_bindings(pkg, body.module, body.file)
            .into_iter()
            .collect();
    let mut lo = Lowerer {
        src: &pkg.files[body.file].raw.src,
        module_binds: &module_binds,
        table: &tb.table,
        sig_table: &sigs.table,
        sigs,
        calls: tb.calls.iter().map(|(s, c)| (*s, c)).collect(),
        folds: tb.comptime_folds.iter().map(|(s, f)| (*s, f)).collect(),
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
        capture_binds: 0,
        task_captures: tb.task_captures.iter().map(|(s, c)| (*s, &c[..])).collect(),
        pending_tasks: pending,
        foreign: None,
        lender,
        pending_specs: specs,
        pending_dyn_shims: dyn_shims,
        survey,
        b: &mut b,
    };
    // The fallible shape: the body's result is an eu when the closure
    // raised (s73 closure rows).
    if let Some(ret) = task.body_ret
        && matches!(lo.b.module.types.get(ret), types::TypeData::Eu { .. })
    {
        lo.fn_eu = Some(ret);
    }
    // Prologue. Task kind: captures are the entry block's params,
    // bound by name (s73/s86 — the shim unpacked the env). Closure
    // kind (s105): the env RECORD pointer is the leading param;
    // captures flat-load from it through a foreign-role region (the
    // s98 dyn-shim read discipline: runtime-owned storage this body
    // never stores through), then the declared parameters bind.
    let entry_params = lo.b.block_params(lo.b.current_block());
    lo.scopes.push(ScopeFrame::default());
    lo.capture_binds = task.caps.len();
    match &task.kind {
        PendingKind::Task => {
            for (i, (name, sema)) in task.caps.iter().enumerate() {
                let wty = task.cap_wtys[i];
                let val = entry_params[i];
                let var = lo.b.declare_var(wty);
                lo.b.def_var(var, val);
                lo.b.func.add_debug_var(name.clone(), val, true);
                let bind = LocalBind::Val {
                    var,
                    wrapping: matches!(lo.table.kind(*sema), TyKind::Wrapping(_)),
                    unsigned: sema_unsigned(lo.table, *sema),
                    wir_ty: wty,
                };
                lo.scopes
                    .last_mut()
                    .expect("scope")
                    .binds
                    .push((name.clone(), bind));
            }
        }
        PendingKind::Closure { params } => {
            let has_env = !task.caps.is_empty();
            if has_env {
                let env = entry_params[0];
                let renv = lo.b.ins_region_foreign(ForeignRole::Header);
                for (i, (name, sema)) in task.caps.iter().enumerate() {
                    let wty = task.cap_wtys[i];
                    let off = task.cap_offs[i];
                    let addr = if off == 0 {
                        env
                    } else {
                        let k = lo.b.iconst(types::I64, off as i64);
                        lo.b.ins_ptr_off(env, k, 1)
                    };
                    let val = load_flat_raw(lo.b, wty, addr, renv, task.span)?;
                    let var = lo.b.declare_var(wty);
                    lo.b.def_var(var, val);
                    lo.b.func.add_debug_var(name.clone(), val, true);
                    let bind = LocalBind::Val {
                        var,
                        wrapping: matches!(lo.table.kind(*sema), TyKind::Wrapping(_)),
                        unsigned: sema_unsigned(lo.table, *sema),
                        wir_ty: wty,
                    };
                    lo.scopes
                        .last_mut()
                        .expect("scope")
                        .binds
                        .push((name.clone(), bind));
                }
            }
            let first = if has_env { 1 } else { 0 };
            for (j, (name, sema)) in params.iter().enumerate() {
                let val = entry_params[first + j];
                let wty = lo.b.func.value_ty(val);
                let var = lo.b.declare_var(wty);
                lo.b.def_var(var, val);
                lo.b.func.add_debug_var(name.clone(), val, true);
                let bind = LocalBind::Val {
                    var,
                    wrapping: matches!(lo.table.kind(lo.strip_sema(*sema)), TyKind::Wrapping(_)),
                    unsigned: sema_unsigned(lo.table, *sema),
                    wir_ty: wty,
                };
                lo.scopes
                    .last_mut()
                    .expect("scope")
                    .binds
                    .push((name.clone(), bind));
            }
        }
    }
    let Some(bnode) = callable_body(closure) else {
        return Err(refuse("a task closure without a body", closure.span));
    };
    if bnode.kind == SyntaxKind::Block {
        let blk = AstBlock::cast(bnode).expect("kind");
        if lo.fn_eu.is_some() {
            lo.fn_tail = blk.trailing_expr().map(|e| e.span);
        }
        match lo.lower_block(blk, task.body_ret.is_some())? {
            Flow::Diverged => {}
            Flow::Val(v) => {
                let out = lo.arm_to_merge_ret(v, task.body_ret, closure.span)?;
                let vals: Vec<Value> = out.into_iter().collect();
                lo.b.ins_ret(&vals);
            }
        }
    } else {
        // Single-expression body: `fn() expr`.
        match lo.lower_expr(bnode)? {
            Flow::Diverged => {}
            Flow::Val(v) => {
                let out = lo.arm_to_merge_ret(v, task.body_ret, closure.span)?;
                let vals: Vec<Value> = out.into_iter().collect();
                lo.b.ins_ret(&vals);
            }
        }
    }
    lo.scopes.pop();
    let stats = b.stats;
    let func = b.finish();
    module.add_func(func);
    Ok(stats)
}

/// Build one task entry shim (s73): `fn(env: ptr) -> i64` per the
/// frozen `__wolf_rt_scope_spawn`/`__wolf_rt_proc_spawn_outcome`
/// contract — unpack the env words, call the body function, and map
/// its outcome onto the task-return protocol: `0` normal, an error
/// tag, or the reserved cancel sentinel `-2` (`wolf_rt`'s
/// `CANCEL_TAG`) for a body whose error is the `cancelled` tag or
/// whose scope was killed (the no-defer teardown branch, the c07
/// handoff).
/// s98: build one dyn-slot shim — the erased-shape function a vtable
/// slot points at (`[abi.native.dyn]`). `(ptr, tail…)` in; the
/// receiver VALUE flat-loads from the data pointer (the exec
/// fixture's own shape: read through the pointer, then work); one
/// call to the real target; its result straight out. Module-wide
/// idempotent: two casts demanding one slot build one shim.
fn build_dyn_shim(module: &mut Module, shim: &DynShim) -> R<()> {
    if module.funcs.iter().any(|(_, f)| f.name == shim.name) {
        return Ok(());
    }
    let mut b = FuncBuilder::new(module, shim.name.clone(), shim.erased_sig);
    b.func.src_file = None; // synthetic: no line table
    let entry = b.current_block();
    let params = b.block_params(entry);
    let (data, tail) = params.split_first().expect("erased sig has a receiver");
    let tail: Vec<Value> = tail.to_vec();
    // The vtable data pointer reads through the foreign header region
    // (never stored through in any body this lowering emits).
    let region = b.ins_region_foreign(ForeignRole::Header);
    let recv = load_flat_raw(&mut b, shim.recv_ty, *data, region, shim.span)?;
    let ext = b.func.import_func(shim.target.clone(), shim.target_sig);
    let mut args = vec![recv];
    args.extend(tail);
    let rets = b.ins_call(ext, &args);
    b.ins_ret(&rets);
    let func = b.finish();
    module.add_func(func);
    Ok(())
}

fn build_task_shim(module: &mut Module, task: &PendingTask<'_>) -> R<()> {
    let cancelled_id = module.tag_id("cancelled");
    // Machine shape: (ptr) -> i64; the env token param erases.
    let env_formal = RegionId::new(0);
    let tok_ty = module.types.mem(env_formal);
    let sig = module.make_sig(
        vec![
            Param {
                ty: types::PTR,
                mode: Mode::Mut,
            },
            Param {
                ty: tok_ty,
                mode: Mode::Val,
            },
        ],
        vec![types::I64],
    );
    let mut b = FuncBuilder::new(module, task.shim_name.clone(), sig);
    b.func.src_file = None; // synthetic: no line table
    let entry = b.current_block();
    let params = b.block_params(entry);
    let (env, tok) = (params[0], params[1]);
    b.def_mem(env_formal, tok);
    // Unpack the env words.
    let mut args = Vec::with_capacity(task.cap_wtys.len());
    for (i, &wty) in task.cap_wtys.iter().enumerate() {
        let off = task.cap_offs[i];
        let addr = if off == 0 {
            env
        } else {
            let k = b.iconst(types::I64, off as i64);
            b.ins_ptr_off(env, k, 1)
        };
        args.push(load_flat_raw(&mut b, wty, addr, env_formal, task.span)?);
    }
    // Call the body.
    let body_ext = b.func.import_func(task.body_name.clone(), task.body_sig);
    let rets = b.ins_call(body_ext, &args);
    // The raw outcome tag: 0 for ok bodies, the eu tag for fallible
    // ones (module-interned, ≥ 1).
    let tag = match (task.body_ret, rets.first()) {
        (Some(rt), Some(&rv)) if matches!(module_types_get(&b, rt), types::TypeData::Eu { .. }) => {
            let is_err = b.ins_eu_is_err(rv);
            let err_bb = b.create_block();
            let ok_bb = b.create_block();
            let join = b.create_block();
            let out = b.add_block_param(join, types::I64);
            b.ins_br(is_err, err_bb, &[], ok_bb, &[]);
            b.seal_block(err_bb);
            b.seal_block(ok_bb);
            b.switch_to_block(err_bb);
            let t = b.ins_eu_err_tag(rv);
            b.ins_jmp(join, &[t]);
            b.switch_to_block(ok_bb);
            let z = b.iconst(types::I64, 0);
            b.ins_jmp(join, &[z]);
            b.seal_block(join);
            b.switch_to_block(join);
            out
        }
        _ => b.iconst(types::I64, 0),
    };
    // Map onto the protocol: killed → -2 (kill teardown reached the
    // task; [conc.proc.kill] step 1's compiled half), cancelled tag →
    // -2 (polite cancel — the reason class, not a failure), else the
    // tag itself.
    let killed_ext = {
        let s = b.module.make_sig(Vec::new(), vec![types::I8]);
        b.func.import_func("__wolf_rt_task_killed".to_string(), s)
    };
    let killed = b
        .ins_call(killed_ext, &[])
        .first()
        .copied()
        .expect("killed poll result");
    let kz = b.iconst(types::I8, 0);
    let was_killed = b
        .ins(
            Opcode::Icmp,
            &[killed, kz],
            &[types::BOOL],
            Aux::IntCc(IntCc::Ne),
        )
        .one();
    // killed → -2; else cancelled-tag → -2; else the tag (a branch
    // chain — WIR bools are not integer-op operands).
    let cancel_bb = b.create_block();
    let check_bb = b.create_block();
    let plain_bb = b.create_block();
    b.ins_br(was_killed, cancel_bb, &[], check_bb, &[]);
    b.seal_block(check_bb);
    b.switch_to_block(check_bb);
    let cid = b.iconst(types::I64, cancelled_id);
    let is_cancel = b
        .ins(
            Opcode::Icmp,
            &[tag, cid],
            &[types::BOOL],
            Aux::IntCc(IntCc::Eq),
        )
        .one();
    b.ins_br(is_cancel, cancel_bb, &[], plain_bb, &[]);
    b.seal_block(cancel_bb);
    b.seal_block(plain_bb);
    b.switch_to_block(cancel_bb);
    let minus2 = b.iconst(types::I64, -2);
    b.ins_ret(&[minus2]);
    b.switch_to_block(plain_bb);
    b.ins_ret(&[tag]);
    let func = b.finish();
    module.add_func(func);
    Ok(())
}

/// [`Lowerer::load_flat`]'s raw twin for shim building (no `Lowerer`
/// in scope): scalars load directly, aggregates rebuild field-wise.
fn load_flat_raw(
    b: &mut FuncBuilder<'_>,
    ty: TypeId,
    ptr: Value,
    region: RegionId,
    span: Span,
) -> R<Value> {
    if scalar_size(ty).is_some() {
        return Ok(b.ins_load(ty, ptr, region));
    }
    let types::TypeData::Agg(fields) = b.module.types.get(ty).clone() else {
        return Err(refuse("task captures without a flat layout", span));
    };
    let Some(offs) = flat_offsets(&b.module.types, &fields) else {
        return Err(refuse("task captures without a flat layout", span));
    };
    let mut parts = Vec::with_capacity(fields.len());
    for (k, &fty) in fields.iter().enumerate() {
        let addr = if offs[k] == 0 {
            ptr
        } else {
            let i = b.iconst(types::I64, offs[k] as i64);
            b.ins_ptr_off(ptr, i, 1)
        };
        parts.push(load_flat_raw(b, fty, addr, region, span)?);
    }
    Ok(b.ins(Opcode::AggMake, &parts, &[ty], Aux::None).one())
}

/// Borrow-friendly type peek while a `FuncBuilder` holds the module.
fn module_types_get(b: &FuncBuilder<'_>, t: TypeId) -> types::TypeData {
    b.module.types.get(t).clone()
}

fn refuse(construct: &'static str, span: Span) -> NotYet {
    NotYet { construct, span }
}

/// A refusal whose reason names something only known at lowering time
/// (s93: the rigid an instantiation left unbound). `NotYet.construct`
/// is `&'static str` by design — the ledger keys on it — so the text is
/// leaked. This is a cold, error-only path: one allocation per refusal
/// the ledger will print, never per lowered instruction.
fn refuse_named(text: String, span: Span) -> NotYet {
    let leaked: &'static str = Box::leak(text.into_boxed_str());
    refuse(leaked, span)
}

/// The two `wolf_rt::list::ListHdr` field offsets compiled code
/// addresses directly (s75). The header is `#[repr(C)] { data: *mut
/// u8, len: i64, cap: i64, elem: i64 }`, and these offsets are part
/// of the runtime ABI the moment element access stops going through
/// a shim: `wolf_rt::list`'s `header_offsets_are_the_lowering_abi`
/// test pins them against this pair by name.
const LIST_DATA_OFF: u64 = 0;
const LIST_LEN_OFF: u64 = 8;
const LIST_CAP_OFF: u64 = 16;

/// The longest operand `==` on `str` compares INLINE (s81). Above it,
/// lowering routes to `__wolf_rt_str_eq`, whose body is a `memcmp`.
///
/// The number is a measurement, not a taste: on this host's LLVM tier
/// the inline byte loop runs at ~1.45 ns/byte and `memcmp` at ~0.012
/// (4 KiB operands; the wolf kernel goes 1.45 → 0.024 ns/byte when the
/// route is taken, a 60x drop), while a cross-crate call costs a few ns
/// fixed. The crossover therefore sits in the low tens of bytes, and 64
/// is the round number above it that still keeps every dispatch-shaped
/// compare — `match` arms, tokens, keywords, `d2_substr_search`'s
/// five-byte needle — on the call-free path. A constant length folds
/// the test away entirely, so those sites emit no call even as dead
/// code.
///
/// The branch is not free on the short path, and it was measured there
/// too: `d2_substr_search` came out at 0.868x WITH the route and 0.789x
/// without, over 11 paired runs each against per-run layout floors of
/// ~10% — the same number twice, and the kernel's spread across three
/// independent runs (0.87–0.97) is wider than the difference. So the
/// route buys 60x where it applies and costs nothing measurable where
/// it does not.
const STR_EQ_INLINE_MAX: i64 = 64;

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
/// The `Self` segment of an impl-method name: a nominal's name, or —
/// #119, D49 — a primitive's spelling (`int.Ord.cmp`); the bridge's
/// substrate on builtins uses the same mangling grammar as nominals.
fn self_ty_key(table: &TypeTable, ty: TyId) -> Option<String> {
    match table.kind(ty) {
        TyKind::Nominal { name, .. } => Some(name.clone()),
        TyKind::Prim(p) => Some(p.name().to_string()),
        _ => None,
    }
}

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
/// A rigid-binding frame for layout (s94): one hop of generic-nominal
/// application. `Pair[T, int]`'s fields lower with a frame binding
/// `K ↦ (args' table, T's id)`; frames STACK, so an applied nominal in
/// a field position (`Outer[T] { inner: Pair[T, int] }`) resolves the
/// inner `T` through its parent. No interning: a rigid resolves by
/// hopping to the argument's own table and continuing there.
struct RigidFrame<'x> {
    names: &'x [String],
    table: &'x TypeTable,
    args: &'x [TyId],
    parent: Option<&'x RigidFrame<'x>>,
}

impl<'x> RigidFrame<'x> {
    fn lookup(&self, name: &str) -> Option<(&'x TypeTable, TyId, Option<&'x RigidFrame<'x>>)> {
        if let Some(i) = self.names.iter().position(|n| n == name) {
            return Some((self.table, self.args[i], self.parent));
        }
        self.parent.and_then(|p| p.lookup(name))
    }
}

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
    wir_ty_frame(it, table, sigs, id, span, depth, None)
}

fn wir_ty_frame(
    it: &mut types::TypeInterner,
    table: &TypeTable,
    sigs: &SigTables,
    id: TyId,
    span: Span,
    depth: u32,
    frame: Option<&RigidFrame<'_>>,
) -> R<Option<TypeId>> {
    if depth > 32 {
        // The cap is also the recursive-generic-nominal stop
        // (`Node[T] { next: Node[T] }` by value is infinite-size; a
        // cycle-safe layout needs indirection the language does not
        // spell yet).
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
        TyKind::Wrapping(inner) => {
            match wir_ty_frame(it, table, sigs, *inner, span, depth + 1, frame)? {
                Some(t) if types_is_int(t) => Ok(Some(t)),
                _ => Err(refuse("wrapping over a non-integer type", span)),
            }
        }
        TyKind::Distinct(inner) => wir_ty_frame(it, table, sigs, *inner, span, depth + 1, frame),
        TyKind::ErrUnion(ok, row) => {
            if row_is_empty(table, *row) {
                wir_ty_frame(it, table, sigs, *ok, span, depth + 1, frame)
            } else {
                // A fallible type with tags: the eu pair (s27). The ok
                // half maps as usual; the row's payloads unify into
                // positional slots.
                let okw = wir_ty_frame(it, table, sigs, *ok, span, depth + 1, frame)?;
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
        TyKind::Nominal { module, name, args } => {
            // Adapter types are scalars in disguise (layout identity);
            // struct nominals are by-value aggregates (s26); enums are
            // tag scalars or tag+slots aggregates (s27). Field/variant
            // types live in the SIGNATURE table.
            match sigs.get(*module as usize, name) {
                Some(ItemSig::Distinct { base, .. }) => {
                    wir_ty_frame(it, &sigs.table, sigs, *base, span, depth + 1, frame)
                }
                Some(ItemSig::Struct(ss)) if !ss.generic => {
                    let mut fields = Vec::with_capacity(ss.fields.len());
                    for f in &ss.fields {
                        match wir_ty_frame(it, &sigs.table, sigs, f.ty, span, depth + 1, frame)? {
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
                // s94: an APPLIED generic struct — `Pair[int, str]` —
                // lays out its fields under a rigid frame binding the
                // declaration's parameters to the application's
                // arguments (which live in the CURRENT table). Frames
                // stack, so `Outer[T] { inner: Pair[T, int] }` resolves
                // the inner `T` through the outer application. The
                // layout memo is the interner itself: identical field
                // lists intern to one `Agg`.
                Some(ItemSig::Struct(ss)) if ss.generics.len() == args.len() => {
                    let inner = RigidFrame {
                        names: &ss.generics,
                        table,
                        args,
                        parent: frame,
                    };
                    let mut fields = Vec::with_capacity(ss.fields.len());
                    for f in &ss.fields {
                        match wir_ty_frame(
                            it,
                            &sigs.table,
                            sigs,
                            f.ty,
                            span,
                            depth + 1,
                            Some(&inner),
                        )? {
                            Some(t) => fields.push(t),
                            None => {
                                return Err(refuse("unit-typed struct fields", span));
                            }
                        }
                    }
                    if fields.is_empty() {
                        return Ok(None);
                    }
                    Ok(Some(it.intern(types::TypeData::Agg(fields))))
                }
                Some(ItemSig::Enum {
                    generic,
                    generics,
                    variants,
                    ..
                }) if !*generic || generics.len() == args.len() => {
                    // s94: an applied generic enum lays out its
                    // payload slots under the same rigid frame as
                    // struct fields; a bare generic enum still falls
                    // through to the named refusal.
                    let inner = RigidFrame {
                        names: generics,
                        table,
                        args,
                        parent: frame,
                    };
                    let frame = if *generic { Some(&inner) } else { frame };
                    // Enum values: the variant tag (declaration index,
                    // i64) alone when payload-free, else tag + the
                    // position-unified payload slots.
                    let mut slots: Vec<TypeId> = Vec::new();
                    for v in variants {
                        for (i, &p) in v.payload.iter().enumerate() {
                            let Some(w) =
                                wir_ty_frame(it, &sigs.table, sigs, p, span, depth + 1, frame)?
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
                match wir_ty_frame(it, table, sigs, e, span, depth + 1, frame)? {
                    Some(t) => fields.push(t),
                    None => return Err(refuse("unit-typed tuple elements", span)),
                }
            }
            if fields.is_empty() {
                return Ok(None);
            }
            Ok(Some(it.intern(types::TypeData::Agg(fields))))
        }
        // s105: a region VALUE is its runtime handle — the header
        // pointer the runtime already keys region identity on (#113's
        // pooling work; the s73 channel crossing pinned the shape).
        // Value positions only: a region INSIDE another value (a
        // struct field, a tuple slot, an enum payload, an error-union
        // half) is a handle extent tracking cannot follow through the
        // aggregate boundary — refused by name, the c25 stop.
        TyKind::RegionTy => {
            if depth == 0 {
                Ok(Some(types::PTR))
            } else {
                Err(refuse(
                    "a region inside another value (extent tracking stops at the \
                     aggregate boundary — c25 closeout)",
                    span,
                ))
            }
        }
        // s73 — the conc capability handles: opaque runtime pointers
        // (channels, sync cells, task scopes); proc ids and exit
        // reasons are plain words ([conc.proc.1], [conc.proc.exit]).
        TyKind::Chan(_) | TyKind::Mutex(_) | TyKind::TaskScope => Ok(Some(types::PTR)),
        TyKind::Proc | TyKind::ExitReason => Ok(Some(types::I64)),
        TyKind::Range(_) => Err(refuse(
            "range VALUES outside `for` headers (owned `Iter[int]` ranges, c06/std)",
            span,
        )),
        // A `List[T]` VALUE is one pointer to its runtime header
        // (s40, `wolf_rt::list`); element shapes are checked at the
        // operation sites, so the handle lowers for any element.
        TyKind::List(_) => Ok(Some(types::PTR)),
        TyKind::Shared(_) | TyKind::Weak(_) | TyKind::Handle(_) | TyKind::Pool(_) => Err(refuse(
            "shared-tier surface lowering (rc receivers + runtime cells, c06)",
            span,
        )),
        // Raw pointers are opaque `ptr` VALUES (s29 — the C membrane
        // hands them out and takes them back). The raw-tier OPS over
        // them (deref, index, arithmetic, casts) keep their s26
        // refusals at the expression sites.
        TyKind::Ptr(_) => Ok(Some(types::PTR)),
        // s95: a fn-typed VALUE is one code pointer — `func.addr` puts
        // it there, and the call through it is its own construct (the
        // c05 refusal at the call site, until an indirect call lands).
        TyKind::Fn(_, _) => Ok(Some(types::PTR)),
        // s93: a rigid here is a generic parameter no substitution
        // bound — unless a rigid FRAME binds it (s94: a generic
        // nominal's field under an application). Resolution hops to
        // the argument's own table and continues under the parent
        // frame; only an unbound rigid refuses, named.
        TyKind::Rigid(name) => {
            if let Some(f) = frame
                && let Some((atable, aid, parent)) = f.lookup(name)
            {
                return wir_ty_frame(it, atable, sigs, aid, span, depth + 1, parent);
            }
            Err(refuse_named(
                format!("a generic parameter `{name}` outside any instantiation"),
                span,
            ))
        }
        // s93: an associated-type projection whose base the substitution
        // made concrete (`T.Item` under `T ↦ int`) normalizes through the
        // impl's rewrite rules — which need the impl instantiated, and
        // that is s94's generic-impl work under c06's dispatch tables.
        // Name it, so the ledger says what is missing rather than "a
        // type".
        // s95: a projection whose base IS concrete normalizes through
        // the coherence-unique impl's rewrite rules right here — the
        // rule's target lives in the signature table, so resolution
        // hops tables exactly as the rigid FRAME arm does. What still
        // refuses: a base no impl covers (the message below), a
        // GENERIC impl's rule (its target mentions rigids the base's
        // arguments would have to bind — named), and an assoc name two
        // impls both rewrite (coherence allows it across traits).
        TyKind::Proj(base, name) => {
            if let TyKind::Nominal { name: head, .. } = table.kind(*base) {
                let mut hits = sigs.impls.iter().filter(|i| {
                    i.rewrites.contains_key(name)
                        && matches!(
                            sigs.table.kind(i.self_ty),
                            TyKind::Nominal { name: h, .. } if h == head
                        )
                });
                match (hits.next(), hits.next()) {
                    (Some(i), None) => {
                        if !i.generics.is_empty() {
                            return Err(refuse_named(
                                format!(
                                    "an associated type on a generic impl (`.{name}` binds \
                                     through the instantiation)"
                                ),
                                span,
                            ));
                        }
                        let target = i.rewrites[name];
                        return wir_ty_frame(it, &sigs.table, sigs, target, span, depth + 1, None);
                    }
                    (Some(_), Some(_)) => {
                        return Err(refuse_named(
                            format!("an associated type two impls rewrite (`.{name}`)"),
                            span,
                        ));
                    }
                    (None, _) => {}
                }
            }
            Err(refuse_named(
                format!("an associated-type projection `.{name}` (needs the impl instantiated)"),
                span,
            ))
        }
        // s96: a trait object is the two-word pair (data, vtable) —
        // `[abi.native.dyn]`. The data half points at the erased value
        // and carries the region obligations the checker already
        // assigned it; the vtable half points at static slots in the
        // trait's `DynReport.methods` order (sema's canonical record —
        // the interface serializes exactly that list, so a slot index
        // is a cross-module fact). The layout lands here; TABLES
        // cannot yet be demanded — nothing constructs a dyn value
        // (no coercion, cast, or unification admits concrete → dyn),
        // so the pair only ever arrives as a parameter today. The
        // construction rule is a surface decision filed for the
        // human, not decided silently (s96's stop).
        TyKind::Dyn { .. } => {
            // s98 (D47's conservative reading): the pair exists as a
            // local, a parameter, or an argument — TOP-LEVEL positions
            // only (depth 0). Behind any other type — a struct field,
            // a generic argument, a row payload, a fallible return —
            // the pair could outlive the place its data half borrows
            // through a shape the mem tier cannot see; refuse by name
            // until the borrow story for that shape is written.
            if depth > 0 {
                return Err(refuse(
                    "a `dyn` inside another type (the pair stays a local or an argument for now)",
                    span,
                ));
            }
            Ok(Some(it.intern(types::TypeData::Agg(vec![
                types::PTR,
                types::PTR,
            ]))))
        }
        // s94 elaborated applied generic nominals, so what still
        // reaches lowering as an opaque token is the residue the
        // checker leaves by design: const-generic VALUE arguments
        // (`Buf[2 + 2]` — values have no home in the type table yet)
        // and generic aliases. Name the token and the reason.
        TyKind::Unsupported(spelling) => Err(refuse_named(
            format!(
                "a generic application left opaque by the checker (`{spelling}`: const-generic values and generic aliases do not elaborate yet)"
            ),
            span,
        )),
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

/// The strictest natural alignment any scalar inside a flat type
/// needs (s75: what an element stride has to tile at).
fn flat_align(it: &types::TypeInterner, t: TypeId) -> u64 {
    if let Some(s) = scalar_size(t) {
        return s;
    }
    match it.get(t) {
        types::TypeData::Agg(fields) => fields
            .clone()
            .into_iter()
            .map(|f| flat_align(it, f))
            .max()
            .unwrap_or(1),
        _ => 0,
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
#[allow(clippy::too_many_arguments)]
fn wir_fn_sig(
    module: &mut Module,
    cache: &mut HashMap<String, SigId>,
    table: &TypeTable,
    sigs: &SigTables,
    name: &str,
    fsig: &FnSig,
    view_mask: u32,
    span: Span,
) -> R<SigId> {
    if let Some(&sig) = cache.get(name) {
        return Ok(sig);
    }
    let sig = wir_sig_of(module, table, sigs, fsig, view_mask, span)?;
    cache.insert(name.to_string(), sig);
    Ok(sig)
}

/// The uncached signature build (shared by definitions and call-site
/// imports so both see the same `mut` → (ptr, token) expansion).
///
/// s89: a parameter in `view_mask` arrives as a byte VIEW — the two
/// words a `str` is — instead of a `List` header, so it expands to
/// `(ptr, i64)`. Both the definition and the call-site import build
/// through here with the same mask, which is what keeps the two sides
/// of a specialized call in agreement.
/// `table` is the table `fsig`'s ids live in: `sigs.table` for a plain
/// or byte-view build, the substituted clone for a monomorphic
/// instance (s93) — a superset of `sigs.table`, so nominal field ids
/// resolved through `sigs` stay valid either way.
fn wir_sig_of(
    module: &mut Module,
    table: &TypeTable,
    sigs: &SigTables,
    fsig: &FnSig,
    view_mask: u32,
    span: Span,
) -> R<SigId> {
    let mut params = Vec::with_capacity(fsig.params.len());
    let mut next_formal = 0u32;
    for (i, p) in fsig.params.iter().enumerate() {
        if view_mask & (1u32 << i) != 0 {
            params.push(Param {
                ty: types::PTR,
                mode: Mode::Val,
            });
            params.push(Param {
                ty: types::I64,
                mode: Mode::Val,
            });
            continue;
        }
        let Some(ty) = wir_ty(&mut module.types, table, sigs, p.ty, p.span)? else {
            return Err(refuse("unit-typed parameters", p.span));
        };
        // s105: a region parameter is the arrived handle, read-only —
        // `mut`/`take` would promise writeback or consumption of an
        // identity this frame does not own.
        if matches!(table.kind(p.ty), TyKind::RegionTy) && p.mode.is_some() {
            return Err(refuse(
                "a moded region parameter (a region arrives read-only — c25 closeout)",
                p.span,
            ));
        }
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
    // s98 (D47): a declared `dyn` RETURN would hand the pair down a
    // frame while its data half borrows a place in the returning one;
    // the borrow story for that crossing is unwritten. Refuse at the
    // signature, so every body of the fn refuses the same way.
    if matches!(table.kind(fsig.ret), TyKind::Dyn { .. }) {
        return Err(refuse(
            "a `dyn` return (the pair does not cross a return — bind it in the caller)",
            span,
        ));
    }
    let results = match wir_ty(&mut module.types, table, sigs, fsig.ret, span)? {
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
        /// This frame created the region (`region.new` seeded its token
        /// chain here). A region that ARRIVED as a value — a parameter,
        /// a call result, a channel receive — has its chain with its
        /// creator: opening through the handle works, but freeze/free
        /// would need a token this frame does not hold (s105).
        owned: bool,
    },
    /// s89 — a `List[int]` parameter that arrived as a byte VIEW: the
    /// receiver's own `{ptr, len}`, exactly what `s.bytes()` is at the
    /// call site. Entry-block values, so they dominate every use; the
    /// binding is read-only by construction (there is no store path in
    /// this file that takes one) and every use it can have is one of
    /// s77's seven consuming positions — the lend analysis proved that
    /// before the caller was allowed to pass one.
    BytesView { ptr: Value, len: Value },
    /// s105: a capturing closure bound by `let` — the two-word pair
    /// (entry fn ptr via `func.addr`, env record ptr), the s96/s98
    /// aggregate with the vtable slot replaced by a direct entry. The
    /// pair stays in its frame: a call reads both halves (two
    /// `agg.get`s and a `call.ind`, env leading); any other read
    /// refuses by name.
    Closure { pair: Value },
    /// A unit-typed binding (no runtime value).
    Unit,
    /// A `when`-body payload rebind (s73, [conc.when.body]): reads and
    /// writes go through the held cell's runtime accessors
    /// (`__wolf_rt_sync_get`/`__wolf_rt_sync_set`) — valid exactly
    /// while the set is held, which is the rebind's lexical extent.
    SyncPayload { cell: Value },
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
    /// s76: this scope opened an AMBIENT region — the saved previous
    /// ambient handle, restored on every exit edge ahead of the free
    /// (`[mem.region.create.3]`; see [`Lower::open_ambient`]).
    ambient_prev: Option<Value>,
    /// s73: a held `when` set to release on every exit edge —
    /// (cells array pointer, its slot region, set length).
    when_release: Option<(Value, RegionId, i64)>,
    /// s73: an open task scope's handle — join+free on every exit
    /// edge crossing it ([conc.task.join]); the fall-through exit
    /// clears this and re-raises the tag itself.
    conc_scope: Option<Value>,
    /// s86: this task scope owns an ENV ARENA — the region capture
    /// records are bump-allocated in when a spawn sits under a loop
    /// (a frame slot would be one buffer shared by every iteration).
    /// It is the same handle as [`ScopeFrame::region`]; the field
    /// exists so `pack_task_env` can find the *task* scope's arena
    /// without mistaking an enclosing `region { }` for it.
    task_env: Option<(RegionId, Value)>,
    /// s86: the loop-stack depth when this task scope opened. A spawn
    /// needs the arena exactly when it runs at a GREATER depth — one
    /// frame slot cannot serve two live tasks, and a loop is the only
    /// way to reach the same spawn site twice before the join.
    loops_at_open: usize,
}

/// Strip a `move`/`copy` prefix off an argument expression.
fn strip_move(e: &GreenNode) -> &GreenNode {
    if e.kind == SyntaxKind::PrefixExpr
        && let Some(d) = PrefixExpr::cast(e)
        && matches!(
            d.op().map(|t| t.kind),
            Some(SyntaxKind::MoveKw | SyntaxKind::CopyKw)
        )
        && let Some(op) = d.operand()
    {
        return op;
    }
    e
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

/// s89 — where the byte view being consumed comes from.
#[derive(Clone, Copy)]
enum ViewSrc<'t> {
    /// `<str>.bytes()`: the receiver still needs lowering.
    Recv(&'t GreenNode),
    /// A parameter lent a view: its two entry words, already in hand.
    Bound(Value, Value),
}

/// A `for` head that walks a `str` LAZILY (s84, `[mem.str.view]`):
/// `words()`, `lines()` and `split(sep)` yield subslices of the
/// receiver's own storage, so the loop allocates nothing. The
/// `List[str]` these methods are typed as is built only where a
/// first-class list value is actually needed.
#[derive(Clone, Copy)]
enum StrIter<'t> {
    Words {
        recv: &'t GreenNode,
    },
    Lines {
        recv: &'t GreenNode,
    },
    Split {
        recv: &'t GreenNode,
        sep: &'t GreenNode,
    },
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
    /// `str`: literal arms dispatch by equality (#54, v0) — a chain of
    /// `__wolf_rt_str_eq` tests in arm order.
    Str,
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
    /// Str literal arm: the scrutinee equals one of these cooked byte
    /// strings (or-alternatives). Dispatch-by-equality (#54, v0).
    StrTests(Vec<Vec<u8>>),
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
    /// Folded comptime call sites by span (s71): the site lowers to
    /// this constant; the comptime callee itself never lowers.
    folds: HashMap<Span, &'t Fold>,
    expr_tys: HashMap<Span, TyId>,
    local_tys: HashMap<Span, TyId>,
    casts: HashMap<Span, (TyId, TyId, CastKind)>,
    fns: &'t HashMap<&'t str, Vec<(usize, &'t FnSig)>>,
    /// This body's module-namespace bindings (bound name → package
    /// module), for the qualified fn-value read (#116): `txt.is_wolf`
    /// in value position names a module member, and the base has no
    /// recorded type precisely because sema typed the member THROUGH
    /// the namespace.
    module_binds: &'b HashMap<String, usize>,
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
    /// s105: the leading binds of scope 0 are CAPTURES (this Lowerer
    /// is a closure/task body). Writes, `mut`/`take` lends of a name
    /// that resolves to one refuse by name — borrow-only closures:
    /// the env holds copies, and only read-only captures keep the
    /// copy unobservable (the mem tier's shared loans are the other
    /// half of that argument).
    capture_binds: usize,
    /// s73: spawn-site capture sets by method-call span (sema's
    /// conc-lowering handoff — the closure environment layout input).
    task_captures: HashMap<Span, &'t [wolf_sema::TaskCapture]>,
    /// s73: task bodies queued for synthesis after this function
    /// finishes (a `FuncBuilder` borrows the module exclusively, so
    /// nested functions build in a post-pass worklist).
    pending_tasks: &'b mut Vec<PendingTask<'t>>,
    /// s75: the function's runtime-storage regions, minted on the
    /// first container touch — `(headers, element buffers)`. TWO, not
    /// one, and the split is a theorem rather than a convenience: a
    /// `wolf_rt` list header and a list element buffer are always
    /// separate allocations (`new_list` mints the header, `push_raw`
    /// mints and regrows the buffer, and `data` never points into the
    /// header), so no store through one can be a store through the
    /// other. That is what lets a `len` load leave a loop whose body
    /// writes elements. WITHIN each region nothing is claimed: two
    /// lists may share a buffer.
    foreign: Option<(RegionId, RegionId)>,
    /// s89: the shared byte-view lend verdicts (`wolf_mem::byteview`).
    lender: &'t Lender<'t>,
    /// s89: view specializations this body's call sites demanded, drained
    /// to fixpoint by [`lower_package`].
    pending_specs: &'b mut Vec<SpecRequest>,
    /// s98: dyn-cast sites queue one erased-shape shim per vtable slot
    /// (`{target}.dynshim`: load the receiver from the data pointer,
    /// call the target); [`lower_body`] builds them after the task
    /// drain. Shim bodies contain no casts, so the queue never feeds
    /// itself.
    pending_dyn_shims: &'b mut Vec<DynShim>,
    /// The survey lens ([`lower_package_survey`]): when on, a
    /// statement-level refusal is recorded here and lowering continues
    /// with the next statement instead of aborting the body.
    survey: Option<&'b mut Vec<NotYet>>,
    b: &'b mut FuncBuilder<'m>,
}

/// One queued dyn-slot shim (s98): the erased-shape function a vtable
/// slot points at. `(ptr, tail…)` in, the receiver VALUE flat-loaded
/// from the data pointer, one call to the real target, its result out.
/// The shim is the table's problem, never the call site's
/// (`[abi.native.dyn]`).
struct DynShim {
    /// The shim's WIR name: `{target}.dynshim`.
    name: String,
    /// The real method body the slot dispatches to.
    target: String,
    target_sig: SigId,
    /// The erased signature (`ptr` receiver + declared tail).
    erased_sig: SigId,
    /// The receiver's WIR layout (what flat-loads from the data ptr).
    recv_ty: TypeId,
    span: Span,
}

/// What a queued body IS (s105): a spawn task (the s73/s86 shape —
/// body fn + runtime entry shim), or a closure VALUE's entry fn (one
/// function: `(env?, params…) -> ret`, captures loaded from the env
/// record in its own prologue; no runtime shim).
#[derive(Clone)]
enum PendingKind {
    Task,
    /// The closure's declared parameters (name, sema type), in order.
    Closure {
        params: Vec<(String, TyId)>,
    },
}

/// One queued task body (s73): the entry shim `fn(env) -> i64` plus —
/// for closure tasks — the body function lowered from the closure with
/// its captures as parameters. Proc spawns of named functions reuse
/// the already-lowered callee as the body. s105: also one queued
/// closure ENTRY (see [`PendingKind`]).
struct PendingTask<'t> {
    /// The entry shim's WIR name (what `func.addr` names).
    shim_name: String,
    /// The body function's WIR name.
    body_name: String,
    /// The body's WIR signature (params = env words, one result when
    /// `body_ret` is set).
    body_sig: SigId,
    /// The closure to lower as the body (None: `body_name` is an
    /// already-lowered named function — the proc-spawn form).
    closure: Option<&'t GreenNode>,
    /// Env-slot names + sema types, in layout order (closure captures
    /// or evaluated spawn arguments).
    caps: Vec<(String, TyId)>,
    /// WIR types of the env slots, in order.
    cap_wtys: Vec<TypeId>,
    /// Byte offsets of the env slots (8-aligned).
    cap_offs: Vec<u64>,
    /// The body's result WIR type, if any (an eu for fallible bodies).
    body_ret: Option<TypeId>,
    /// The declaration span (diagnostics + line tables).
    span: Span,
    kind: PendingKind,
}

impl<'t, 'b, 'm> Lowerer<'t, 'b, 'm> {
    fn text(&self, span: Span) -> String {
        String::from_utf8_lossy(&self.src[span.lo as usize..span.hi as usize]).into_owned()
    }

    fn expr_sema_ty(&self, span: Span) -> Option<TyId> {
        self.expr_tys.get(&span).copied()
    }

    fn lower_fn(&mut self, fsig: &FnSig, block: AstBlock<'t>, view_mask: u32) -> R<()> {
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
        for (pi, p) in fsig.params.iter().enumerate() {
            // s89: a lent byte view is two entry words, bound as the
            // view itself — no header, no allocation, nothing to
            // materialize on entry.
            if view_mask & (1u32 << pi) != 0 {
                let ptr = entry_params[wir_idx];
                let len = entry_params[wir_idx + 1];
                wir_idx += 2;
                self.b.func.add_debug_var(p.name.clone(), ptr, true);
                self.scopes
                    .last_mut()
                    .expect("scope")
                    .binds
                    .push((p.name.clone(), LocalBind::BytesView { ptr, len }));
                continue;
            }
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
            // s105: a region parameter binds as a region — identity +
            // arrived handle (the s73 adopted shape: the token chain
            // is the creator's; opening through the handle works,
            // freeze/free refuse by name).
            if matches!(self.sig_table.kind(p.ty), TyKind::RegionTy) {
                let val = entry_params[wir_idx];
                wir_idx += 1;
                self.b.func.add_debug_var(p.name.clone(), val, true);
                let region = self.b.fresh_region();
                self.scopes.last_mut().expect("scope").binds.push((
                    p.name.clone(),
                    LocalBind::Region {
                        region,
                        handle: val,
                        frozen: false,
                        owned: false,
                    },
                ));
                continue;
            }
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
        self.lower_block_in(block, want_value, None, None)
    }

    /// Lower a block in a fresh scope; `region` attaches the X4 sugar's
    /// wholesale free as the scope's outermost cleanup entry, and
    /// `ambient_prev` (s76) attaches the ambient-region restore just
    /// inside it.
    fn lower_block_in(
        &mut self,
        block: AstBlock<'t>,
        want_value: bool,
        region: Option<(RegionId, Value)>,
        ambient_prev: Option<Value>,
    ) -> R<Flow> {
        self.scopes.push(ScopeFrame {
            region,
            ambient_prev,
            ..ScopeFrame::default()
        });
        let last_value = if want_value {
            block.trailing_expr().map(|e| e.span)
        } else {
            None
        };
        let mut out: Option<Value> = None;
        for stmt in block.statements() {
            let scopes_depth = self.scopes.len();
            let loops_depth = self.loops.len();
            let saved_visible = self.visible;
            let flow = self.lower_stmt(stmt, last_value, &mut out);
            match flow {
                Ok(Flow::Val(_)) => {}
                Ok(Flow::Diverged) => {
                    self.scopes.pop();
                    return Ok(Flow::Diverged);
                }
                Err(e) => {
                    // The survey lens: record the reason, restore the
                    // frame stacks the failed statement may have left
                    // half-pushed, and lower ON — the next statement's
                    // reason is what fail-fast masks. The function is
                    // garbage from here (skipped values, dangling
                    // blocks); [`lower_body`] never adds it.
                    if let Some(sink) = self.survey.as_deref_mut() {
                        sink.push(e);
                        self.scopes.truncate(scopes_depth);
                        self.loops.truncate(loops_depth);
                        self.visible = saved_visible;
                        continue;
                    }
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
        // s73: a held `when` set releases on every exit edge, after
        // the body's defers ([conc.when.body]'s extent).
        if let Some((slot, region, n)) = self.scopes[si].when_release {
            let nv = self.b.iconst(types::I64, n);
            self.call_with_slot_token("__wolf_rt_when_release", &[slot, nv], region, None);
        }
        // s73: an exit edge crossing an open task scope joins first
        // ([conc.task.join]); the tag is dropped on non-fall-through
        // edges (the explicit exit's value wins — recorded rule).
        if let Some(handle) = self.scopes[si].conc_scope {
            self.rt_call("__wolf_rt_scope_join_free", &[handle], Some(types::I64));
        }
        // s76: close the ambient region BEFORE the free — after this
        // point allocations belong to the enclosing region again, and
        // the ambient slot never names a freed arena.
        if let Some(prev) = self.scopes[si].ambient_prev {
            self.rt_call("__wolf_rt_region_ambient_leave", &[prev], None);
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
            // #116b: a nested named fn — a capture-free fn value with
            // a name. The entry lifts through s105's closure queue
            // (the checker enforced zero captures and recorded the fn
            // type at this decl's span); the name binds like a `let`
            // of the s95 `func.addr` value.
            SyntaxKind::FnDecl => {
                let d = wolf_ast::FnDecl::cast(stmt).expect("kind");
                let v = self.queue_closure_entry(stmt, Vec::new(), None)?;
                let Some(name_span) = d.name().map(|t| t.span) else {
                    return Ok(Flow::Val(None));
                };
                let Some(sema_ty) = self.expr_sema_ty(stmt.span) else {
                    return Err(refuse("a nested fn without a recorded type", stmt.span));
                };
                let name = self.text(name_span);
                let Some(wty) = self.wir_value_ty(sema_ty, stmt.span)? else {
                    return Err(refuse("a valueless nested fn", stmt.span));
                };
                let var = self.b.declare_var(wty);
                if self.straight_line {
                    self.b.mark_single_block(var);
                }
                self.b.def_var(var, v);
                self.b.func.add_debug_var(name.clone(), v, false);
                self.scopes.last_mut().expect("scope").binds.push((
                    name,
                    LocalBind::Val {
                        var,
                        wrapping: false,
                        unsigned: false,
                        wir_ty: wty,
                    },
                ));
                Ok(Flow::Val(None))
            }
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
        // s105: a CAPTURING closure bound by `let`/`var` — build the
        // pair here, where the binding claims the borrow (the mem
        // tier's loan already did the same one rung earlier).
        // Capture-free closures fall through: they are ordinary s95
        // fn values.
        if init.kind == SyntaxKind::ClosureExpr
            && let Some(name_span) = name_span
            && !self.closure_captures(init.span).is_empty()
        {
            let pair = self.lower_closure_pair(init)?;
            let name = self.text(name_span);
            self.scopes
                .last_mut()
                .expect("scope")
                .binds
                .push((name, LocalBind::Closure { pair }));
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
        // s73/s105: a region value arriving as a VALUE (a recv'd
        // region, a call result, a rebound handle): bind identity +
        // handle. The arriving arena's token chain is its creator's
        // (runtime free via proc ledger / process exit, or the
        // creating frame's free point); no local chain roots here —
        // `owned: false`, so freeze/free refuse by name.
        if matches!(self.table.kind(self.strip_sema(sema_ty)), TyKind::RegionTy) {
            let Some(handle) = v else {
                return Err(refuse("a region binding without a handle value", span));
            };
            let region = self.b.fresh_region();
            self.scopes.last_mut().expect("scope").binds.push((
                name,
                LocalBind::Region {
                    region,
                    handle,
                    frozen: false,
                    owned: false,
                },
            ));
            return Ok(Flow::Val(None));
        }
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
                    owned: true,
                }))
            }
            SyntaxKind::FreezeExpr => {
                let d = wolf_ast::FreezeExpr::cast(init).expect("kind");
                let Some(operand) = d.expr() else {
                    return Err(refuse("freeze without an operand", init.span));
                };
                if operand.kind == SyntaxKind::RegionBlock {
                    // `let x = freeze region { … }` — build-then-
                    // freeze yields the block's VALUE, not a region
                    // binding; the expr path lowers it (s73).
                    return Ok(None);
                }
                let (region, handle, owned) = self.expect_region(operand)?;
                // s105: freezing consumes the mutable token — a chain
                // only the creating frame holds. An arrived handle
                // (parameter, call result, recv) refuses by name.
                if !owned {
                    return Err(refuse(
                        "freezing a region this frame did not create (the token \
                         chain lives with the creator — c25 closeout)",
                        operand.span,
                    ));
                }
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
                    owned: true,
                }))
            }
            _ => Ok(None),
        }
    }

    /// Resolve an expression that must name a region binding. The
    /// third element: this frame created the region (freeze/free may
    /// consume its token chain).
    fn expect_region(&mut self, e: &'t GreenNode) -> R<(RegionId, Value, bool)> {
        if e.kind != SyntaxKind::PathExpr {
            return Err(refuse(
                "region operands beyond named bindings (c05)",
                e.span,
            ));
        }
        let name = self.text(e.span);
        match self.lookup(&name) {
            Some(LocalBind::Region {
                region,
                handle,
                owned,
                ..
            }) => Ok((region, handle, owned)),
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
        self.lookup_at(name).map(|(_, _, b)| b)
    }

    /// [`Self::lookup`] with the resolution point: (scope index, bind
    /// index, bind). The capture classifier's input (s105).
    fn lookup_at(&self, name: &str) -> Option<(usize, usize, LocalBind)> {
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
            for (j, (n, b)) in scope.binds[..limit].iter().enumerate().rev() {
                if n == name {
                    return Some((i, j, *b));
                }
            }
        }
        None
    }

    /// s105: does `name` resolve to a CAPTURE of the closure/task body
    /// this Lowerer is building? Shadowing-correct: a closure-local
    /// rebinding of the same name resolves deeper and answers false.
    fn resolves_to_capture(&self, name: &str) -> bool {
        self.capture_binds > 0
            && matches!(self.lookup_at(name), Some((0, j, _)) if j < self.capture_binds)
    }

    /// s105: refuse a write/`mut`/`take` position whose place ROOT is
    /// a capture — borrow-only closures. `what` names the spelling.
    fn check_capture_write(&self, e: &'t GreenNode, what: &'static str) -> R<()> {
        if self.capture_binds == 0 {
            return Ok(());
        }
        let mut cur = e;
        loop {
            match cur.kind {
                SyntaxKind::MemberExpr => {
                    let Some(base) = wolf_ast::MemberExpr::cast(cur).and_then(|m| m.base()) else {
                        return Ok(());
                    };
                    cur = base;
                }
                SyntaxKind::BracketApply => {
                    let Some(base) = BracketApply::cast(cur).and_then(|b| b.callee()) else {
                        return Ok(());
                    };
                    cur = base;
                }
                SyntaxKind::ParenExpr => {
                    let Some(inner) = ParenExpr::cast(cur).and_then(|p| p.expr()) else {
                        return Ok(());
                    };
                    cur = inner;
                }
                SyntaxKind::PathExpr => break,
                _ => return Ok(()),
            }
        }
        let root = self.text(cur.span);
        if self.resolves_to_capture(&root) {
            return Err(refuse_named(
                format!(
                    "a closure {what} its captured binding `{root}` (captures are \
                     read-only — borrow-only closures, c25 closeout)"
                ),
                e.span,
            ));
        }
        Ok(())
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
        self.check_capture_write(place, "writing")?;
        if place.kind == SyntaxKind::MemberExpr {
            return self.lower_member_assign(d, place, stmt.span);
        }
        if place.kind == SyntaxKind::BracketApply {
            return self.lower_index_assign(d, place, stmt.span);
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
            // s105: the pair is claimed by ITS binding for its whole
            // extent (the mem tier scoped the loans to it); rebinding
            // would need loan transfer nothing models yet.
            LocalBind::Closure { .. } => Err(refuse(
                "reassigning a closure binding (the pair stays with its `let` — \
                 c25 closeout)",
                place.span,
            )),
            // s89: a lent view is read-only — the lend analysis admits
            // no assignment to the parameter, so reaching here would be
            // the two halves disagreeing. Refusing keeps that
            // disagreement an honest `NotYet` rather than a write into
            // a caller's string.
            LocalBind::BytesView { .. } => Err(refuse(
                "assignment to a lent `bytes()` view (a byte view is read-only, s77/s89)",
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
                    let cur = self.read_mut_ref(ptr, region, elem, stmt.span)?;
                    let Some(bin) = Self::compound_bin(op) else {
                        return Err(refuse("this compound assignment operator", stmt.span));
                    };
                    match self.arith(bin, cur, rhs, wrapping, unsigned, elem, stmt.span)? {
                        Some(v) => v,
                        None => return Ok(Flow::Diverged),
                    }
                };
                // Field-wise, not one aggregate store (s74, #67): the
                // read side already rebuilds flat aggregates field by
                // field, and `store` is scalar-only by the WIR's own
                // rule — a `mut str` / `mut` struct parameter took the
                // asymmetric path and ICE'd the verifier.
                self.store_flat(newval, ptr, region, stmt.span)?;
                Ok(Flow::Val(None))
            }
            // s73: a `when` payload write — through the held cell's
            // accessor ([conc.when.body]; the checker keeps these
            // inside the body's extent).
            LocalBind::SyncPayload { cell } => {
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
                    // The payload computes at its own width (checked
                    // arithmetic included), then widens back to the
                    // wire word.
                    let wty = self.b.func.value_ty(rhs);
                    let w = self
                        .rt_call("__wolf_rt_sync_get", &[cell], Some(types::I64))
                        .expect("payload word");
                    let cur = self.narrow_from_wire(w, wty, stmt.span)?;
                    let Some(bin) = Self::compound_bin(op) else {
                        return Err(refuse("this compound assignment operator", stmt.span));
                    };
                    match self.arith(bin, cur, rhs, false, false, wty, stmt.span)? {
                        Some(v) => v,
                        None => return Ok(Flow::Diverged),
                    }
                };
                let w = self.widen_to_wire(newval, stmt.span)?;
                self.rt_call("__wolf_rt_sync_set", &[cell, w], None);
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
                TyKind::Nominal { module, name, .. } => match self.sigs.get(*module as usize, name)
                {
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
                    // s105: a region read as a VALUE is its runtime
                    // handle — the s73 channel crossing's shape in
                    // every value position (passed, returned,
                    // rebound). Positions extent tracking cannot
                    // follow refuse at the TYPE (a region inside
                    // another value) or at the OP (container
                    // elements), by name.
                    Some(LocalBind::Region { handle, .. }) => Ok(Flow::Val(Some(handle))),
                    // s105: the pair does not leave its frame — the
                    // fn-value ABI is one word, and the env borrows
                    // places of THIS frame. Calls go through the
                    // binding; any other read refuses by name.
                    Some(LocalBind::Closure { .. }) => Err(refuse(
                        "a capturing closure read as a value (the pair stays in its \
                         frame; call it by name — c25 closeout)",
                        e.span,
                    )),
                    // s89: a lent view has no first-class WIR value —
                    // that is the invariant s77 set and this sprint
                    // kept. Every position the lend analysis admits is
                    // handled before the name is read as a value, so
                    // this arm is the disagreement detector, not a
                    // path a `Lendable` body can take.
                    Some(LocalBind::BytesView { .. }) => Err(refuse(
                        "a lent `bytes()` view in a value position (bind it with `let` \
                         to materialize, s89)",
                        e.span,
                    )),
                    // s73: a `when` payload read — through the held
                    // cell's accessor ([conc.when.body]), narrowed to
                    // the payload's own width.
                    Some(LocalBind::SyncPayload { cell }) => {
                        let w = self
                            .rt_call("__wolf_rt_sync_get", &[cell], Some(types::I64))
                            .expect("payload word");
                        let wty = match self.expr_sema_ty(e.span) {
                            Some(t) => self.wir_value_ty(t, e.span)?.unwrap_or(types::I64),
                            None => types::I64,
                        };
                        Ok(Flow::Val(Some(self.narrow_from_wire(w, wty, e.span)?)))
                    }
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
                        // s95: a module-level FN read as a VALUE —
                        // `func.addr` (s86 built the emission for task
                        // entries; a bare fn name in value position is
                        // the same pointer). What still refuses here
                        // is actual module STATE — `let`/`var` items —
                        // and the message now says so.
                        if let Some(cands) = self.fns.get(name.as_str()) {
                            let (fmodule, fsig) = match cands.as_slice() {
                                [one] => *one,
                                _ => {
                                    return Err(refuse_named(
                                        format!(
                                            "a same-named function read without a unique \
                                             declaration locus (`{name}`)"
                                        ),
                                        e.span,
                                    ));
                                }
                            };
                            if !fsig.generics.is_empty() {
                                return Err(refuse_named(
                                    format!(
                                        "a generic function as a value (`{name}` has no \
                                         instantiation at the read)"
                                    ),
                                    e.span,
                                ));
                            }
                            if fsig.comptime {
                                return Err(refuse_named(
                                    format!("a comptime fn as a runtime value (`{name}`)"),
                                    e.span,
                                ));
                            }
                            let qname = qualify(self.sigs, fmodule, &name);
                            let ext = match self.callees.get(&qname) {
                                Some(&ext) => ext,
                                None => {
                                    let sig = wir_sig_of(
                                        self.b.module,
                                        self.sig_table,
                                        self.sigs,
                                        fsig,
                                        0,
                                        e.span,
                                    )?;
                                    let ext = self.b.func.import_func(qname.clone(), sig);
                                    self.callees.insert(qname, ext);
                                    ext
                                }
                            };
                            return Ok(Flow::Val(Some(self.b.ins_func_addr(ext))));
                        }
                        Err(refuse(
                            "module-item reads (mutable module state, c06)",
                            e.span,
                        ))
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
            SyntaxKind::BracketApply => self.lower_index(e),
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
                // `freeze region { … }` in value position yields the
                // block's value with the region promoted to imm
                // ([mem.region.freeze.1]; never freed — s73).
                if let Some(operand) = wolf_ast::FreezeExpr::cast(e).and_then(|d| d.expr())
                    && operand.kind == SyntaxKind::RegionBlock
                {
                    return self.lower_frozen_block(operand, want);
                }
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
            // s105: a closure VALUE. Capture-free closures lambda-lift
            // to a synthesized module fn — the value is one code
            // pointer (`func.addr`), indistinguishable from the s95
            // bare-fn read, so it stores, passes, returns and calls
            // through every s97 path. A CAPTURING closure is the
            // two-word pair, admitted only where a binding claims it
            // (the mem tier's loan discipline mirrors this): anywhere
            // else refuses by name.
            SyntaxKind::ClosureExpr => {
                if self.closure_captures(e.span).is_empty() {
                    let v = self.queue_closure_entry(e, Vec::new(), None)?;
                    Ok(Flow::Val(Some(v)))
                } else {
                    Err(refuse(
                        "a capturing closure outside a `let` binding (the env \
                         borrows its captures; bind it, then call it — c25 \
                         closeout)",
                        e.span,
                    ))
                }
            }
            // The conc surface (s73): native lowering onto the
            // s32–s36 runtime ABI.
            SyntaxKind::ScopeExpr => self.lower_conc_scope(e, want),
            SyntaxKind::SelectExpr => self.lower_select_expr(e),
            SyntaxKind::WhenExpr => self.lower_when_expr(e, want),
            SyntaxKind::SpawnExpr => self.lower_proc_spawn(e),
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
        let TyKind::Nominal { module, name, .. } = table.kind(ty) else {
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
        // s73: duration members on integer literals — scale into the
        // int nanosecond currency ([conc.select.timeout]'s v0 unit;
        // sema admitted literal bases only).
        if base.kind == SyntaxKind::LiteralExpr
            && let Some(mtext) = m.member().map(|t| self.text(t.span))
            && let Some(mult) = match mtext.as_str() {
                "s" => Some(1_000_000_000i64),
                "ms" => Some(1_000_000),
                "us" => Some(1_000),
                "ns" => Some(1),
                _ => None,
            }
        {
            let Some(v) = flow_val!(self.lower_expr(base)) else {
                return Err(refuse("a valueless duration base", base.span));
            };
            let k = self.b.iconst(types::I64, mult);
            let scaled = self.arith(SyntaxKind::Star, v, k, false, false, types::I64, e.span)?;
            return Ok(match scaled {
                Some(s) => Flow::Val(Some(s)),
                None => Flow::Diverged,
            });
        }
        // #116/#23: a member whose BASE is a namespace, not a value.
        // The base span has no recorded type precisely because sema
        // typed the member THROUGH the namespace (synth_member's
        // module and type-member paths) — so the recorded type of the
        // WHOLE member expression says what this is:
        //   module.fn      -> the s95 fn-value read, qualified
        //   Enum.Variant   -> payload-free construction (the tag)
        // A base that is a real value always carries a recorded type,
        // so this branch cannot shadow field extraction.
        if self.expr_sema_ty(base.span).is_none()
            && base.kind == SyntaxKind::PathExpr
            && let Some(bt) = wolf_ast::PathExpr::cast(base).and_then(|pp| pp.ident())
            && let Some(mtok) = m.member()
            && let Some(whole) = self.expr_sema_ty(e.span)
        {
            let bname = self.text(bt.span);
            let mname = self.text(mtok.span);
            if matches!(self.table.kind(whole), TyKind::Fn(..))
                && let Some(&fmodule) = self.module_binds.get(&bname)
                && let Some(ItemSig::Fn(fsig)) = self.sigs.get(fmodule, &mname)
            {
                if !fsig.generics.is_empty() {
                    return Err(refuse_named(
                        format!(
                            "a generic function as a value (`{bname}.{mname}` has no                              instantiation at the read)"
                        ),
                        e.span,
                    ));
                }
                if fsig.comptime {
                    return Err(refuse_named(
                        format!("a comptime fn as a runtime value (`{bname}.{mname}`)"),
                        e.span,
                    ));
                }
                let qname = qualify(self.sigs, fmodule, &mname);
                let ext = match self.callees.get(&qname) {
                    Some(&ext) => ext,
                    None => {
                        let sig =
                            wir_sig_of(self.b.module, self.sig_table, self.sigs, fsig, 0, e.span)?;
                        let ext = self.b.func.import_func(qname.clone(), sig);
                        self.callees.insert(qname, ext);
                        ext
                    }
                };
                return Ok(Flow::Val(Some(self.b.ins_func_addr(ext))));
            }
            if let TyKind::Nominal { module, name, .. } = self.table.kind(whole).clone()
                && let Some(ItemSig::Enum { variants, .. }) = self.sigs.get(module as usize, &name)
                && let Some(index) = variants.iter().position(|v| v.name == mname)
            {
                if !variants[index].payload.is_empty() {
                    return Err(refuse_named(
                        format!(
                            "a payload-carrying variant as a bare value                              (`{name}.{mname}` wants its payload applied)"
                        ),
                        e.span,
                    ));
                }
                return self.enum_value(whole, index, &[], e.span);
            }
        }
        let Some(base_sema) = self.expr_sema_ty(base.span) else {
            return Err(refuse("a member access without a recorded type", e.span));
        };
        // s40: the builtin `len` members — `s.len` is the byte length
        // half of the pair (D25), `l.len` reads the runtime header.
        if m.member().map(|t| self.text(t.span)).as_deref() == Some("len") {
            // s77: `<str>.bytes().len` is the receiver's length half.
            if let Some(src) = self.view_src(base) {
                let Some((_, n)) = self.lower_view(src)? else {
                    return Ok(Flow::Diverged);
                };
                return Ok(Flow::Val(Some(n)));
            }
            match self.table.kind(self.strip_sema(base_sema)) {
                TyKind::Prim(Prim::Str) => {
                    let Some(sv) = flow_val!(self.lower_expr(base)) else {
                        return Err(refuse("a valueless str receiver", base.span));
                    };
                    let (_, l) = self.str_parts(sv);
                    return Ok(Flow::Val(Some(l)));
                }
                TyKind::List(_) => {
                    let Some(hdr) = flow_val!(self.lower_expr(base)) else {
                        return Err(refuse("a valueless List receiver", base.span));
                    };
                    return Ok(Flow::Val(Some(self.list_len_of(hdr))));
                }
                _ => {}
            }
        }
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

    /// Make `handle` the AMBIENT region for the construct being lowered
    /// (s76, wolf-lang#81), returning the saved previous handle to hand
    /// to [`ScopeFrame::ambient_prev`].
    ///
    /// `[mem.region.create.3]` places an allocation in the ambient
    /// region at its site, and D12 makes a callee allocate into its
    /// CALLER's region — so "ambient" is a property of the dynamic call
    /// stack, exactly as `wolf_mem`'s checker models it with its own
    /// ambient stack. The runtime therefore keeps a per-thread slot and
    /// lowering brackets it: the enter is emitted at the point the
    /// region opens (so it dominates the whole body), the leave rides
    /// the X4 cleanup chain (so every exit edge — fall-through,
    /// `return`, `?`-err, `break`/`continue` crossing the boundary —
    /// restores it, in LIFO order, ahead of the free).
    ///
    /// Scalars and flat aggregates are register-resident and do not
    /// care; what this buys is the CONTAINERS — `wolf_rt::list` reads
    /// the slot, so `region scratch { … }` around container work now
    /// frees the bytes the container used instead of leaking them into
    /// the process root.
    fn open_ambient(&mut self, handle: Value) -> Option<Value> {
        self.rt_call(
            "__wolf_rt_region_ambient_enter",
            &[handle],
            Some(types::PTR),
        )
    }

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
        // s76: the block's region becomes the ambient one for its body
        // — `[mem.region.create.3]`, and the reason `region scratch { }`
        // now reclaims what a container inside it allocated.
        let ambient_prev = self.open_ambient(handle);
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
                    owned: true,
                },
            ));
        }
        let out = self.lower_block_in(body, want, Some((region, handle)), ambient_prev);
        self.scopes.pop();
        out
    }

    /// `in r { body }` — open a region value for ambient placement
    /// ([mem.region.create.3]). s76 makes this REAL for containers: the
    /// body runs with `r` as the thread's ambient region, so a `List`
    /// built inside (here or in anything it calls — D12) lands in `r`
    /// and dies with `r`, not with the process. Scalar and flat
    /// aggregate work is register-resident and unaffected.
    fn lower_in_block(&mut self, e: &'t GreenNode, want: bool) -> R<Flow> {
        let d = wolf_ast::InBlock::cast(e).expect("kind");
        let Some(region_expr) = d.region() else {
            return Err(refuse("an `in` block without a region", e.span));
        };
        let (_region, handle, _owned) = self.expect_region(region_expr)?;
        let Some(body) = d.body() else {
            return Ok(Flow::Val(None));
        };
        // Opening is not owning: `in` never frees `r` (the region value's
        // owner does), so only the ambient restore rides the chain.
        let ambient_prev = self.open_ambient(handle);
        self.lower_block_in(body, want, None, ambient_prev)
    }

    // ------------------------------------- the conc surface (s73) ----
    //
    // scope/spawn/channel/select/when/proc lower onto the frozen
    // s32–s36 runtime ABI: plain calls to `__wolf_rt_*` symbols, task
    // bodies as synthesized functions behind `func.addr` entry
    // pointers, and the kill-vs-cancel law as the status-2 teardown
    // branch at every blocking point (c07's handoff — see
    // `kill_teardown_branch`).

    /// Merge a task body's trailing value into its declared result
    /// (ok-injection for fallible bodies; identity otherwise).
    fn arm_to_merge_ret(
        &mut self,
        v: Option<Value>,
        ret: Option<TypeId>,
        span: Span,
    ) -> R<Option<Value>> {
        let Some(rt) = ret else { return Ok(None) };
        if matches!(self.b.module.types.get(rt), types::TypeData::Eu { .. }) {
            return self.arm_to_merge(v, Some(rt), span);
        }
        match v {
            Some(v) => Ok(Some(v)),
            None => Err(refuse("a valueless task body with a typed result", span)),
        }
    }

    /// A sema type as a WIR VALUE type, with the conc special case:
    /// region values are opaque `ptr` handles when they cross a
    /// channel (`[conc.chan.move]`); everywhere else `wir_ty` rules.
    fn wir_value_ty(&mut self, sema: TyId, span: Span) -> R<Option<TypeId>> {
        if matches!(self.table.kind(self.strip_sema(sema)), TyKind::RegionTy) {
            return Ok(Some(types::PTR));
        }
        wir_ty(&mut self.b.module.types, self.table, self.sigs, sema, span)
    }

    /// The status-2 teardown branch (`[conc.proc.kill]` step 1 for
    /// compiled bodies — the c07 refusal repaid at blocking points):
    /// when the calling task's scope is KILLED, return straight out of
    /// this frame — no defers, no handlers, no user code. Region
    /// bookkeeping is skipped too (tokens are at-most-once per path;
    /// the proc ledger bulk-frees, `[conc.proc.kill]` step 3). Merely
    /// cancelled tasks fall through — cancellation is a VALUE
    /// ([conc.cancel.points]) and the caller's else/`?` handles it,
    /// defers running by ordinary returns ([conc.cancel.defer]).
    fn kill_teardown_branch(&mut self, span: Span) -> R<()> {
        let killed = self
            .rt_call("__wolf_rt_task_killed", &[], Some(types::I8))
            .expect("killed poll result");
        let kz = self.b.iconst(types::I8, 0);
        let was_killed = self
            .b
            .ins(
                Opcode::Icmp,
                &[killed, kz],
                &[types::BOOL],
                Aux::IntCc(IntCc::Ne),
            )
            .one();
        let dead_bb = self.b.create_block();
        let cont_bb = self.b.create_block();
        self.b.ins_br(was_killed, dead_bb, &[], cont_bb, &[]);
        self.b.seal_block(dead_bb);
        self.b.seal_block(cont_bb);
        self.b.switch_to_block(dead_bb);
        self.b.gvn_push_scope();
        if let Some(eu) = self.fn_eu {
            let cid = self.b.module.tag_id("cancelled");
            let tag = self.b.iconst(types::I64, cid);
            let out = self.b.ins_eu_make_err(eu, tag, &[]);
            self.b.ins_ret(&[out]);
        } else {
            let results = self.b.module.sigs[self.b.func.sig].results.clone();
            match results.as_slice() {
                [] => {
                    self.b.ins_ret(&[]);
                }
                [rt] => {
                    let rt = *rt;
                    let z = self.zero_of(rt, span)?;
                    self.b.ins_ret(&[z]);
                }
                _ => return Err(refuse("multi-result teardown returns", span)),
            }
        }
        self.b.gvn_pop_scope();
        self.b.switch_to_block(cont_bb);
        Ok(())
    }

    /// Cancellation surfaced at a blocking point with no row to carry
    /// it (statement-position select/when in a row-less fn): propagate
    /// through the enclosing row when one exists, else trap — the
    /// checker's shapes make this path unreachable in v0 programs.
    fn cancel_escape(&mut self, span: Span) -> R<()> {
        if let Some(eu) = self.fn_eu {
            let cid = self.b.module.tag_id("cancelled");
            let tag = self.b.iconst(types::I64, cid);
            let out = self.b.ins_eu_make_err(eu, tag, &[]);
            if self.run_exits(0, true)? {
                self.b.ins_ret(&[out]);
            }
        } else {
            let _ = span;
            self.b.ins_trap(TrapKind::Assert);
        }
        Ok(())
    }

    /// `scope name? { … }` — `[conc.task.scope]`: open the runtime
    /// scope, run the body with the handle bound, join at every exit
    /// edge (the handle rides the scope frame so early exits join
    /// too), and re-raise the first failure at the fall-through exit
    /// (`[conc.task.fail]`).
    fn lower_conc_scope(&mut self, e: &'t GreenNode, want: bool) -> R<Flow> {
        let d = wolf_ast::ScopeExpr::cast(e).expect("kind");
        let Some(body) = d.body() else {
            return Ok(Flow::Val(None));
        };
        let name = d.name().map(|t| self.text(t.span)).unwrap_or_default();
        let (np, nl) = self.name_bytes(&name);
        let handle = self
            .rt_call("__wolf_rt_scope_new", &[np, nl], Some(types::PTR))
            .expect("scope handle");
        // The handle binds in a wrapper frame; join+free rides the
        // frame so `return`/`?` edges inside the body join first.
        self.scopes.push(ScopeFrame::default());
        if let Some(name_tok) = d.name() {
            let nm = self.text(name_tok.span);
            let var = self.b.declare_var(types::PTR);
            self.b.def_var(var, handle);
            self.scopes.last_mut().expect("scope").binds.push((
                nm,
                LocalBind::Val {
                    var,
                    wrapping: false,
                    unsigned: false,
                    wir_ty: types::PTR,
                },
            ));
        }
        {
            let depth = self.loops.len();
            let f = self.scopes.last_mut().expect("scope");
            f.conc_scope = Some(handle);
            f.loops_at_open = depth;
        }
        // s86: the scope's ENV ARENA, minted only when the body spawns
        // from inside a loop. It rides `ScopeFrame::region`, so the X4
        // cleanup chain frees it on every early-exit edge — and the
        // chain already orders the join AHEAD of the free, which is
        // the whole safety argument: no task can still be reading a
        // capture record when its arena dies.
        if spawns_under_a_loop(self.src, body.syntax()) {
            let (r, h) = self.b.ins_region_new();
            let f = self.scopes.last_mut().expect("scope");
            f.region = Some((r, h));
            f.task_env = Some((r, h));
        }
        let out = self.lower_block(body, want);
        let flow = match out {
            Ok(f) => f,
            Err(x) => {
                self.scopes.pop();
                return Err(x);
            }
        };
        // Fall-through: join here, then re-raise a failing child's
        // tag into the enclosing row ([conc.task.fail] — the same
        // width discipline as `?`; sema checked it at the spawn).
        let env_arena = {
            let f = self.scopes.last_mut().expect("scope");
            f.conc_scope = None;
            f.task_env = None;
            f.region.take()
        };
        self.scopes.pop();
        if matches!(flow, Flow::Diverged) {
            return Ok(Flow::Diverged);
        }
        let tag = self
            .rt_call("__wolf_rt_scope_join_free", &[handle], Some(types::I64))
            .expect("join tag");
        // Joined: every child is finished, so the arena its capture
        // records live in is dead. Freed HERE — ahead of the re-raise
        // fork — so both the failing and the clean edge free it once.
        if let Some((r, h)) = env_arena {
            self.b.ins_region_free(r, h);
        }
        let z = self.b.iconst(types::I64, 0);
        let failed = self
            .b
            .ins(
                Opcode::Icmp,
                &[tag, z],
                &[types::BOOL],
                Aux::IntCc(IntCc::Ne),
            )
            .one();
        if let Some(eu) = self.fn_eu {
            let err_bb = self.b.create_block();
            let cont_bb = self.b.create_block();
            self.b.ins_br(failed, err_bb, &[], cont_bb, &[]);
            self.b.seal_block(err_bb);
            self.b.seal_block(cont_bb);
            self.b.switch_to_block(err_bb);
            self.b.gvn_push_scope();
            let out = self.b.ins_eu_make_err(eu, tag, &[]);
            let flowing = self.run_exits(0, true);
            self.b.gvn_pop_scope();
            if flowing? {
                self.b.ins_ret(&[out]);
            }
            self.b.switch_to_block(cont_bb);
        } else {
            // No row to re-raise into: the spawn typing already
            // required one for fallible children, so a nonzero tag
            // here is a can't-happen (Rust-side panic tags aside).
            let clean = self
                .b
                .ins(
                    Opcode::Icmp,
                    &[tag, z],
                    &[types::BOOL],
                    Aux::IntCc(IntCc::Eq),
                )
                .one();
            if self.trap_unless(clean, TrapKind::Assert) {
                return Ok(Flow::Diverged);
            }
        }
        match flow {
            Flow::Val(v) => Ok(Flow::Val(v)),
            Flow::Diverged => Ok(Flow::Diverged),
        }
    }

    /// `freeze region { … }` in value position (s73): run the block
    /// with the region's name bound, promote at the closing brace
    /// (`sync.freeze`; the arena is imm forever, never freed — X4's
    /// RC-owned end state), and yield the block's value.
    fn lower_frozen_block(&mut self, e: &'t GreenNode, want: bool) -> R<Flow> {
        let d = wolf_ast::RegionBlock::cast(e).expect("kind");
        let Some(body) = d.body() else {
            return Ok(Flow::Val(None));
        };
        let (region, handle) = self.b.ins_region_new();
        // s76: the block body's ambient region, same as `region { }` —
        // the difference is the ending (freeze, never free), so the
        // arena is immutable forever and a container in it outlives the
        // block legally ([mem.region.freeze.1]).
        let ambient_prev = self.open_ambient(handle);
        self.scopes.push(ScopeFrame::default());
        if let Some(name_tok) = d.name() {
            let name = self.text(name_tok.span);
            self.scopes.last_mut().expect("scope").binds.push((
                name,
                LocalBind::Region {
                    region,
                    handle,
                    frozen: false,
                    owned: true,
                },
            ));
        }
        let out = self.lower_block_in(body, want, None, ambient_prev);
        self.scopes.pop();
        let flow = out?;
        if matches!(flow, Flow::Diverged) {
            return Ok(Flow::Diverged);
        }
        let frozen_tok = self.b.ins_sync_freeze(region, handle);
        self.b.func.add_fact(FactData::new(
            FactKind::Frozen(handle),
            Just::Op(frozen_tok),
        ));
        Ok(flow)
    }

    /// Rodata (ptr, len) for a runtime name argument.
    fn name_bytes(&mut self, name: &str) -> (Value, Value) {
        let bytes: &[u8] = if name.is_empty() {
            b"task"
        } else {
            name.as_bytes()
        };
        let idx = self.b.module.intern_data(bytes);
        let p = self.b.ins_data_addr(idx);
        let l = self.b.iconst(types::I64, bytes.len() as i64);
        (p, l)
    }

    /// `s.spawn(fn() { … })` — [conc.task.spawn]: pack the capture
    /// set (sema's s73 handoff) into a stack env, queue the body +
    /// entry shim, and hand the runtime `entry(env)` through the
    /// frozen five-parameter spawn.
    fn lower_scope_spawn(
        &mut self,
        d: CallExpr<'t>,
        recv: &'t GreenNode,
        e: &'t GreenNode,
    ) -> R<Flow> {
        let scope_h = flow_val!(self.lower_expr(recv));
        let Some(scope_h) = scope_h else {
            return Err(refuse("a spawn without a scope handle", recv.span));
        };
        let Some(closure) = d
            .args()
            .into_iter()
            .flat_map(|l| l.args())
            .find_map(Arg::value)
            .filter(|n| n.kind == SyntaxKind::ClosureExpr)
        else {
            return Err(refuse(
                "spawn of a non-closure task (fn values, c05)",
                e.span,
            ));
        };
        let caps: Vec<(String, TyId)> = self
            .task_captures
            .get(&e.span)
            .map(|cs| cs.iter().map(|c| (c.name.clone(), c.ty)).collect())
            .unwrap_or_default();
        let arena = self.task_env_arena(recv, e.span)?;
        let (env, env_region, layout) = self.pack_task_env(&caps, arena, e.span, false)?;
        let task_no = self.pending_tasks.len();
        let base = self.b.func.name.clone();
        let body_name = format!("{base}.task{task_no}");
        let shim_name = format!("{base}.task{task_no}.entry");
        let body_ret = self.task_body_ret(closure)?;
        let body_sig = {
            let params: Vec<Param> = layout.0.iter().map(|&t| Param::val(t)).collect();
            let results: Vec<TypeId> = body_ret.into_iter().collect();
            self.b.module.make_sig(params, results)
        };
        self.pending_tasks.push(PendingTask {
            shim_name: shim_name.clone(),
            body_name,
            body_sig,
            closure: Some(closure),
            caps,
            cap_wtys: layout.0,
            cap_offs: layout.1,
            body_ret,
            span: e.span,
            kind: PendingKind::Task,
        });
        // The entry pointer: func.addr of the (post-pass) shim.
        let shim_sig = self.task_shim_sig();
        let shim_ext = self.rt_like_import(&shim_name, shim_sig);
        let entry = self.b.ins_func_addr(shim_ext);
        let (np, nl) = self.name_bytes(&shim_name);
        self.call_with_slot_token(
            "__wolf_rt_scope_spawn",
            &[scope_h, entry, env, np, nl],
            env_region,
            None,
        );
        Ok(Flow::Val(None))
    }

    /// The shim's WIR signature: `(env: ptr, mem.r0) -> i64`.
    fn task_shim_sig(&mut self) -> SigId {
        let formal = RegionId::new(0);
        let tok = self.b.module.types.mem(formal);
        self.b.module.make_sig(
            vec![
                Param {
                    ty: types::PTR,
                    mode: Mode::Mut,
                },
                Param {
                    ty: tok,
                    mode: Mode::Val,
                },
            ],
            vec![types::I64],
        )
    }

    /// Import a module-local synthesized function by (name, sig) —
    /// the `func.addr` target (verify_module holds one sig per name).
    fn rt_like_import(&mut self, name: &str, sig: SigId) -> ExtFunc {
        match self.callees.get(name) {
            Some(&ext) => ext,
            None => {
                let ext = self.b.func.import_func(name.to_string(), sig);
                self.callees.insert(name.to_string(), ext);
                ext
            }
        }
    }

    /// The declared result of a task closure body: sema recorded the
    /// closure's `Fn(…, ret)` at its span. Fallible closures produce
    /// a payload-free eu (payload rows refuse — the s39 row work).
    fn task_body_ret(&mut self, closure: &'t GreenNode) -> R<Option<TypeId>> {
        let Some(cty) = self.expr_sema_ty(closure.span) else {
            return Err(refuse(
                "a task closure without a recorded type",
                closure.span,
            ));
        };
        let TyKind::Fn(_, ret) = self.table.kind(self.strip_sema(cty)) else {
            return Err(refuse("a task closure without a fn type", closure.span));
        };
        let ret = *ret;
        match self.table.kind(self.strip_sema(ret)) {
            TyKind::ErrUnion(_, row) if !row_is_empty(self.table, *row) => {
                let eu = wir_ty(
                    &mut self.b.module.types,
                    self.table,
                    self.sigs,
                    ret,
                    closure.span,
                )?
                .expect("a tagged union is a value");
                let types::TypeData::Eu { slots, .. } = self.b.module.types.get(eu) else {
                    unreachable!("eu shape");
                };
                if !slots.is_empty() {
                    return Err(refuse("task error payloads (s39 typed rows)", closure.span));
                }
                Ok(Some(eu))
            }
            _ => self.wir_value_ty(ret, closure.span),
        }
    }

    /// s105: sema's capture record for the closure at `span` (empty
    /// when the closure captures nothing — or, defensively, when the
    /// record is missing; the entry build re-checks).
    fn closure_captures(&self, span: Span) -> Vec<(String, TyId)> {
        self.task_captures
            .get(&span)
            .map(|cs| cs.iter().map(|c| (c.name.clone(), c.ty)).collect())
            .unwrap_or_default()
    }

    /// s105: queue a closure's ENTRY function and return its address
    /// (`func.addr`). Capture-free: the entry's signature IS the
    /// closure's fn type — a true s95 fn value. Capturing: the env
    /// record pointer leads, and the caller wraps the address into
    /// the pair.
    fn queue_closure_entry(
        &mut self,
        e: &'t GreenNode,
        caps: Vec<(String, TyId)>,
        cap_layout: Option<(Vec<TypeId>, Vec<u64>)>,
    ) -> R<Value> {
        let Some(cty) = self.expr_sema_ty(e.span) else {
            return Err(refuse("a closure without a recorded type", e.span));
        };
        let TyKind::Fn(ptys, _) = self.table.kind(self.strip_sema(cty)).clone() else {
            return Err(refuse("a closure without a fn type", e.span));
        };
        let pnames: Vec<String> = callable_params(e)
            .into_iter()
            .flat_map(|l| l.params())
            .map(|p| {
                p.name()
                    .map(|n| self.text(n.span))
                    .unwrap_or_else(|| "_".to_string())
            })
            .collect();
        if pnames.len() != ptys.len() {
            return Err(refuse(
                "a closure whose parameter list disagrees with its type",
                e.span,
            ));
        }
        let params: Vec<(String, TyId)> = pnames.into_iter().zip(ptys).collect();
        let body_ret = self.task_body_ret(e)?;
        let mut sigparams: Vec<Param> = Vec::new();
        if !caps.is_empty() {
            sigparams.push(Param {
                ty: types::PTR,
                mode: Mode::Val,
            });
        }
        for (_, sema) in &params {
            if matches!(self.table.kind(self.strip_sema(*sema)), TyKind::RegionTy) {
                return Err(refuse(
                    "a region parameter on a closure (c25 closeout)",
                    e.span,
                ));
            }
            let Some(w) = self.wir_value_ty(*sema, e.span)? else {
                return Err(refuse("unit-typed closure parameters", e.span));
            };
            sigparams.push(Param {
                ty: w,
                mode: Mode::Val,
            });
        }
        let results: Vec<TypeId> = body_ret.into_iter().collect();
        let entry_sig = self.b.module.make_sig(sigparams, results);
        let n = self.pending_tasks.len();
        let base = self.b.func.name.clone();
        let name = format!("{base}.cls{n}");
        let (cap_wtys, cap_offs) = cap_layout.unwrap_or_default();
        self.pending_tasks.push(PendingTask {
            shim_name: name.clone(),
            body_name: name.clone(),
            body_sig: entry_sig,
            closure: Some(e),
            caps,
            cap_wtys,
            cap_offs,
            body_ret,
            span: e.span,
            kind: PendingKind::Closure { params },
        });
        let ext = self.rt_like_import(&name, entry_sig);
        Ok(self.b.ins_func_addr(ext))
    }

    /// s105: build a CAPTURING closure's pair at its `let` binding —
    /// env record packed in a frame slot (values read now, the s86
    /// packing; the mem tier's loans make copy-vs-borrow
    /// unobservable), entry queued, the two words joined.
    fn lower_closure_pair(&mut self, e: &'t GreenNode) -> R<Value> {
        let caps = self.closure_captures(e.span);
        let (slot, _region, layout) = self.pack_task_env(&caps, None, e.span, true)?;
        let entry = self.queue_closure_entry(e, caps, Some(layout))?;
        let pair_ty = self
            .b
            .module
            .types
            .intern(types::TypeData::Agg(vec![types::PTR, types::PTR]));
        Ok(self
            .b
            .ins(Opcode::AggMake, &[entry, slot], &[pair_ty], Aux::None)
            .one())
    }

    /// s105: a call through a closure binding's pair — the dyn
    /// dispatch shape with the vtable slot replaced by a direct
    /// entry: two `agg.get`s and s97's `call.ind`, env leading. The
    /// sig comes from the closure's recorded fn TYPE, exactly as
    /// `lower_indirect_call` builds it, plus the leading env `ptr`.
    fn lower_closure_call(
        &mut self,
        pair: Value,
        d: CallExpr<'t>,
        cs: &CallSig,
        e: &'t GreenNode,
    ) -> R<Flow> {
        let Some(callee) = d.callee() else {
            return Err(refuse("calls outside the resolved surface", e.span));
        };
        let Some(fn_ty) = self.expr_sema_ty(callee.span) else {
            return Err(refuse(
                "an indirect callee without a recorded type",
                callee.span,
            ));
        };
        let TyKind::Fn(param_tys, ret_ty) = self.table.kind(self.strip_sema(fn_ty)).clone() else {
            return Err(refuse(
                "an indirect callee that is not fn-typed",
                callee.span,
            ));
        };
        if cs.params.iter().any(|p| p.mode.is_some()) {
            return Err(refuse(
                "an indirect callee with parameter modes (fn types carry none)",
                e.span,
            ));
        }
        let mut args = Vec::new();
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            let Some(vexpr) = Arg::value(a) else { continue };
            let Some(v) = flow_val!(self.lower_expr(vexpr)) else {
                return Err(refuse("unit-typed arguments", vexpr.span));
            };
            args.push(v);
        }
        let table = self.table;
        let mut params = vec![Param {
            ty: types::PTR,
            mode: Mode::Val,
        }];
        for &pt in &param_tys {
            let Some(w) = wir_ty(&mut self.b.module.types, table, self.sigs, pt, e.span)? else {
                return Err(refuse("unit-typed parameters", e.span));
            };
            params.push(Param {
                ty: w,
                mode: Mode::Val,
            });
        }
        let results = match wir_ty(&mut self.b.module.types, table, self.sigs, ret_ty, e.span)? {
            Some(t) => vec![t],
            None => vec![],
        };
        let sig = self.b.module.make_sig(params, results);
        let entry = self
            .b
            .ins(Opcode::AggGet, &[pair], &[types::PTR], Aux::Int(0))
            .one();
        let env = self
            .b
            .ins(Opcode::AggGet, &[pair], &[types::PTR], Aux::Int(1))
            .one();
        let mut cargs = vec![env];
        cargs.extend(args);
        Ok(Flow::Val(self.b.ins_call_ind(entry, sig, &cargs)))
    }

    /// Where THIS spawn's capture record must live (s86).
    ///
    /// `Ok(None)` — a frame slot is sound: the spawn site is reached at
    /// most once before the scope joins, and the join dominates the
    /// frame's death. `Ok(Some(arena))` — the site sits under a loop
    /// opened after its scope, so every reach needs its own record.
    /// `Err` — it sits under such a loop and the owning scope has no
    /// arena, which happens only when the receiver is not the scope
    /// frame we are standing in (a spawn into an ENCLOSING scope from
    /// inside a nested one). Refused by name; never guessed.
    fn task_env_arena(&self, recv: &'t GreenNode, span: Span) -> R<Option<(RegionId, Value)>> {
        // By NAME, not by handle value: inside a loop the handle reads
        // back as a block parameter, so `Value` identity is not the
        // scope's identity — the binding is.
        let owner = if recv.kind == SyntaxKind::PathExpr {
            let name = self.text(recv.span);
            self.scopes
                .iter()
                .rev()
                .find(|f| f.conc_scope.is_some() && f.binds.iter().any(|(n, _)| *n == name))
        } else {
            None
        };
        let Some(owner) = owner else {
            // The handle came from somewhere other than a named scope
            // frame in this function (a scope value passed in, say).
            // Sound only outside a loop.
            return if self.loops.is_empty() {
                Ok(None)
            } else {
                Err(refuse(
                    "a task spawned in a loop through a scope handle this function did not open",
                    span,
                ))
            };
        };
        if self.loops.len() <= owner.loops_at_open {
            return Ok(None);
        }
        match owner.task_env {
            Some(a) => Ok(Some(a)),
            None => Err(refuse(
                "a task spawned in a loop into an enclosing scope (its arena is not in scope)",
                span,
            )),
        }
    }

    /// Evaluate + pack values into a fresh env record; returns
    /// (env ptr, its region, (wir types, offsets)).
    #[allow(clippy::type_complexity)]
    fn pack_task_env(
        &mut self,
        caps: &[(String, TyId)],
        arena: Option<(RegionId, Value)>,
        span: Span,
        closure: bool,
    ) -> R<(Value, RegionId, (Vec<TypeId>, Vec<u64>))> {
        let mut wtys = Vec::with_capacity(caps.len());
        let mut offs = Vec::with_capacity(caps.len());
        let mut size = 0u64;
        for (name, sema) in caps {
            let Some(wty) = self.wir_value_ty(*sema, span)? else {
                return Err(refuse(
                    if closure {
                        "unit-typed closure captures"
                    } else {
                        "unit-typed task captures"
                    },
                    span,
                ));
            };
            if matches!(self.table.kind(self.strip_sema(*sema)), TyKind::RegionTy) {
                let _ = name;
                return Err(refuse(
                    if closure {
                        "a region captured by a closure (open it in the enclosing \
                         frame — c25 closeout)"
                    } else {
                        "a region captured by a task (send it through a channel — [conc.chan.move])"
                    },
                    span,
                ));
            }
            let Some(sz) = flat_size(&self.b.module.types, wty) else {
                return Err(refuse(
                    if closure {
                        "closure captures without a flat layout"
                    } else {
                        "task captures without a flat layout"
                    },
                    span,
                ));
            };
            wtys.push(wty);
            offs.push(size);
            size += sz.next_multiple_of(8);
        }
        // s86: where the capture record lives. A frame slot when the
        // spawn is straight-line (the scope joins before the frame
        // dies); the task scope's arena when it sits under a loop, so
        // every iteration hands its task a record of its own.
        let (region, slot) = match arena {
            Some((r, h)) => {
                let n = self.b.iconst(types::I64, size.max(8) as i64);
                let p = self.b.ins_region_alloc(r, h, n);
                self.b
                    .func
                    .add_fact(FactData::new(FactKind::Region(p, r), Just::DefOp));
                self.b.func.add_fact(FactData::new(
                    FactKind::Deref(p, DerefSize::Const(size.max(8))),
                    Just::DefOp,
                ));
                (r, p)
            }
            None => self.rt_slot(size.max(8)),
        };
        for (i, (name, _)) in caps.iter().enumerate() {
            let v = match self.lookup(name) {
                Some(LocalBind::Val { var, .. }) => self.b.use_var(var),
                Some(LocalBind::MutRef {
                    ptr, region, elem, ..
                }) => self.read_mut_ref(ptr, region, elem, span)?,
                Some(LocalBind::SyncPayload { cell }) => self
                    .rt_call("__wolf_rt_sync_get", &[cell], Some(types::I64))
                    .expect("payload word"),
                Some(LocalBind::Region { .. }) => {
                    return Err(refuse(
                        if closure {
                            "a region captured by a closure (open it in the enclosing \
                             frame — c25 closeout)"
                        } else {
                            "a region captured by a task (send it through a channel — \
                             [conc.chan.move])"
                        },
                        span,
                    ));
                }
                // s105: a capturing closure's pair stays in its frame
                // — an env record holding another env's pair would put
                // borrowed places behind two boundaries no tracker
                // follows.
                Some(LocalBind::Closure { .. }) => {
                    return Err(refuse(
                        "a capturing closure captured by value (the pair stays in \
                         its frame — c25 closeout)",
                        span,
                    ));
                }
                // s89: a lent view cannot be captured — the capture
                // outlives the call the lend is scoped to (S-10 copies
                // captures into the task's environment).
                Some(LocalBind::BytesView { .. }) => {
                    return Err(refuse(
                        "a lent `bytes()` view captured by a task (bind it with `let` \
                         to materialize, s89)",
                        span,
                    ));
                }
                Some(LocalBind::Unit) | None => {
                    return Err(refuse("an unresolvable task capture", span));
                }
            };
            let addr = self.field_addr(slot, offs[i]);
            self.store_flat(v, addr, region, span)?;
        }
        Ok((slot, region, (wtys, offs)))
    }

    /// A runtime call whose pointer arguments live in one stack slot
    /// region: the trailing (erased) token param orders the caller's
    /// stores/loads against the call, at the frozen ABI's positions.
    fn call_with_slot_token(
        &mut self,
        name: &'static str,
        args: &[Value],
        slot_region: RegionId,
        result: Option<TypeId>,
    ) -> Option<Value> {
        let mut params: Vec<Param> = args
            .iter()
            .map(|&a| Param::val(self.b.func.value_ty(a)))
            .collect();
        let formal = RegionId::new(0);
        let tok = self.b.module.types.mem(formal);
        params.push(Param {
            ty: tok,
            mode: Mode::Val,
        });
        let ext = self.rt_import(name, params, result.into_iter().collect());
        let mut formal_regions = HashMap::new();
        formal_regions.insert(0u32, slot_region);
        self.b
            .ins_call_regions(ext, args, &formal_regions)
            .first()
            .copied()
    }

    /// One-word channel payload conversions: widen to the wire i64.
    fn widen_to_wire(&mut self, v: Value, span: Span) -> R<Value> {
        let vt = self.b.func.value_ty(v);
        if vt == types::I64 {
            return Ok(v);
        }
        if vt == types::BOOL || types_is_int(vt) {
            let op = if vt == types::BOOL {
                Opcode::Zext
            } else {
                Opcode::Sext
            };
            return Ok(self.b.ins(op, &[v], &[types::I64], Aux::None).one());
        }
        Err(refuse(
            "channel payloads beyond one word (s39 std sync)",
            span,
        ))
    }

    /// The wire word back to the element type.
    fn narrow_from_wire(&mut self, w: Value, elem_wty: TypeId, span: Span) -> R<Value> {
        if elem_wty == types::I64 {
            return Ok(w);
        }
        if elem_wty == types::BOOL {
            let z = self.b.iconst(types::I64, 0);
            return Ok(self
                .b
                .ins(Opcode::Icmp, &[w, z], &[types::BOOL], Aux::IntCc(IntCc::Ne))
                .one());
        }
        if types_is_int(elem_wty) {
            return Ok(self
                .b
                .ins(Opcode::Itrunc, &[w], &[elem_wty], Aux::None)
                .one());
        }
        Err(refuse(
            "channel payloads beyond one word (s39 std sync)",
            span,
        ))
    }

    /// Channel methods — send/recv/close over the s33 runtime seam.
    fn lower_chan_method(
        &mut self,
        d: CallExpr<'t>,
        recv: &'t GreenNode,
        elem: TyId,
        mname: &str,
        e: &'t GreenNode,
    ) -> R<Flow> {
        let ch = flow_val!(self.lower_expr(recv));
        let Some(ch) = ch else {
            return Err(refuse("a channel op without a handle", recv.span));
        };
        let elem_is_region = matches!(self.table.kind(self.strip_sema(elem)), TyKind::RegionTy);
        match mname {
            "close" => {
                self.rt_call("__wolf_rt_chan_close", &[ch], None);
                Ok(Flow::Val(None))
            }
            "send" => {
                let Some(arg) = d
                    .args()
                    .into_iter()
                    .flat_map(|l| l.args())
                    .find_map(Arg::value)
                else {
                    return Err(refuse("a send without a payload", e.span));
                };
                let status = if elem_is_region {
                    // `send(move r)` — the affine move
                    // ([conc.chan.move]): the handle crosses; the
                    // donor's binding is moved-from (wolf_mem proved
                    // it; the runtime routes the transfer seam).
                    let operand = strip_move(arg);
                    let (_rid, handle, _owned) = self.expect_region(operand)?;
                    self.rt_call(
                        "__wolf_rt_chan_send_region",
                        &[ch, handle],
                        Some(types::I32),
                    )
                } else {
                    let v = flow_val!(self.lower_expr(arg));
                    let Some(v) = v else {
                        return Err(refuse("a unit-typed send payload", arg.span));
                    };
                    let w = self.widen_to_wire(v, arg.span)?;
                    self.rt_call("__wolf_rt_chan_send", &[ch, w], Some(types::I32))
                }
                .expect("send status");
                // Status 2 (cancelled): the kill teardown branch; a
                // polite cancel (and 1, closed-send) falls through —
                // send types as unit at v0, so the error value has no
                // carrier yet (the s39 row work; ledgered).
                let two = self.b.iconst(types::I32, 2);
                let cancelled = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[status, two],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Eq),
                    )
                    .one();
                let cbb = self.b.create_block();
                let cont = self.b.create_block();
                self.b.ins_br(cancelled, cbb, &[], cont, &[]);
                self.b.seal_block(cbb);
                self.b.switch_to_block(cbb);
                self.b.gvn_push_scope();
                self.kill_teardown_branch(e.span)?;
                self.b.ins_jmp(cont, &[]);
                self.b.gvn_pop_scope();
                self.b.seal_block(cont);
                self.b.switch_to_block(cont);
                Ok(Flow::Val(None))
            }
            "recv" => {
                let (region, slot) = self.rt_slot(8);
                let status = self
                    .rt_call_slot("__wolf_rt_chan_recv", &[ch], slot, region, Some(types::I32))
                    .expect("recv status");
                // The `!T {closed, cancelled}` union.
                let eu = if elem_is_region {
                    let it = &mut self.b.module.types;
                    it.eu(Some(types::PTR), Vec::new())
                } else {
                    self.eu_ty_of(e.span)?
                };
                let v = self.chan_status_join(status, eu, e.span, |lo| {
                    let types::TypeData::Eu { ok, .. } = lo.b.module.types.get(eu).clone() else {
                        unreachable!("recv eu");
                    };
                    let Some(okt) = ok else {
                        return Ok(None);
                    };
                    if elem_is_region {
                        // The wire word IS the handle; adopt on
                        // receipt ([conc.chan.move]'s ledger half).
                        let h = lo.b.ins_load(types::PTR, slot, region);
                        let adopted = lo
                            .rt_call("__wolf_rt_region_adopt", &[h], Some(types::PTR))
                            .expect("adopted handle");
                        Ok(Some(adopted))
                    } else {
                        let w = lo.b.ins_load(types::I64, slot, region);
                        Ok(Some(lo.narrow_from_wire(w, okt, e.span)?))
                    }
                })?;
                Ok(Flow::Val(Some(v)))
            }
            _ => Err(refuse("this channel method (s39 std sync)", e.span)),
        }
    }

    /// Join a chan-op status into its `!T {closed, cancelled}` union:
    /// 0 → ok (via `mk_ok`), 1 → `closed`, 2 → the kill-teardown
    /// check, then `cancelled` as a value ([conc.cancel.points]).
    fn chan_status_join(
        &mut self,
        status: Value,
        eu: TypeId,
        span: Span,
        mk_ok: impl FnOnce(&mut Self) -> R<Option<Value>>,
    ) -> R<Value> {
        let ok_bb = self.b.create_block();
        let closed_bb = self.b.create_block();
        let cancel_bb = self.b.create_block();
        let merge = self.b.create_block();
        let out = self.b.add_block_param(merge, eu);
        let zero = self.b.iconst(types::I32, 0);
        let is_ok = self
            .b
            .ins(
                Opcode::Icmp,
                &[status, zero],
                &[types::BOOL],
                Aux::IntCc(IntCc::Eq),
            )
            .one();
        let rest = self.b.create_block();
        self.b.ins_br(is_ok, ok_bb, &[], rest, &[]);
        self.b.seal_block(ok_bb);
        self.b.seal_block(rest);
        self.b.switch_to_block(rest);
        self.b.gvn_push_scope();
        let one = self.b.iconst(types::I32, 1);
        let is_closed = self
            .b
            .ins(
                Opcode::Icmp,
                &[status, one],
                &[types::BOOL],
                Aux::IntCc(IntCc::Eq),
            )
            .one();
        self.b.ins_br(is_closed, closed_bb, &[], cancel_bb, &[]);
        self.b.gvn_pop_scope();
        self.b.seal_block(closed_bb);
        self.b.seal_block(cancel_bb);
        self.b.switch_to_block(ok_bb);
        self.b.gvn_push_scope();
        let okv = mk_ok(self)?;
        let ov = self.b.ins_eu_make_ok(eu, okv);
        self.b.ins_jmp(merge, &[ov]);
        self.b.gvn_pop_scope();
        self.b.switch_to_block(closed_bb);
        self.b.gvn_push_scope();
        let cid = self.b.module.tag_id("closed");
        let ctag = self.b.iconst(types::I64, cid);
        let cv = self.b.ins_eu_make_err(eu, ctag, &[]);
        self.b.ins_jmp(merge, &[cv]);
        self.b.gvn_pop_scope();
        self.b.switch_to_block(cancel_bb);
        self.b.gvn_push_scope();
        self.kill_teardown_branch(span)?;
        let xid = self.b.module.tag_id("cancelled");
        let xtag = self.b.iconst(types::I64, xid);
        let xv = self.b.ins_eu_make_err(eu, xtag, &[]);
        self.b.ins_jmp(merge, &[xv]);
        self.b.gvn_pop_scope();
        self.b.seal_block(merge);
        self.b.switch_to_block(merge);
        Ok(out)
    }

    /// Proc verbs (s73 — [conc.proc.model]): monitor/kill/cancel/link
    /// over the s34 registry, ids as plain words.
    fn lower_proc_method(
        &mut self,
        d: CallExpr<'t>,
        recv: &'t GreenNode,
        mname: &str,
        e: &'t GreenNode,
    ) -> R<Flow> {
        let id = flow_val!(self.lower_expr(recv));
        let Some(id) = id else {
            return Err(refuse("a proc op without a handle", recv.span));
        };
        match mname {
            "monitor" => Ok(Flow::Val(Some(
                self.rt_call("__wolf_rt_proc_monitor", &[id], Some(types::PTR))
                    .expect("monitor channel"),
            ))),
            "kill" => {
                self.rt_call("__wolf_rt_proc_kill", &[id], None);
                Ok(Flow::Val(None))
            }
            "cancel" => {
                self.rt_call("__wolf_rt_proc_cancel", &[id], None);
                Ok(Flow::Val(None))
            }
            "link" => {
                let Some(arg) = d
                    .args()
                    .into_iter()
                    .flat_map(|l| l.args())
                    .find_map(Arg::value)
                else {
                    return Err(refuse("a link without a partner", e.span));
                };
                let other = flow_val!(self.lower_expr(arg));
                let Some(other) = other else {
                    return Err(refuse("a link without a partner value", arg.span));
                };
                self.rt_call("__wolf_rt_proc_link", &[id, other], None);
                Ok(Flow::Val(None))
            }
            _ => Err(refuse("this proc method (s39 supervisors)", e.span)),
        }
    }

    /// Exit-reason class predicates ([conc.proc.exit]): the wire word
    /// carries the class in its low byte.
    fn lower_reason_method(
        &mut self,
        recv: &'t GreenNode,
        mname: &str,
        e: &'t GreenNode,
    ) -> R<Flow> {
        let word = flow_val!(self.lower_expr(recv));
        let Some(word) = word else {
            return Err(refuse("a reason predicate without a value", recv.span));
        };
        let class = match mname {
            "is_normal" => 0,
            "is_error" => 1,
            "is_killed" => 2,
            "is_cancelled" => 3,
            _ => return Err(refuse("this exit-reason method (s39)", e.span)),
        };
        let mask = self.b.iconst(types::I64, 0xFF);
        let kind = self
            .b
            .ins(Opcode::Band, &[word, mask], &[types::I64], Aux::None)
            .one();
        let want = self.b.iconst(types::I64, class);
        Ok(Flow::Val(Some(
            self.b
                .ins(
                    Opcode::Icmp,
                    &[kind, want],
                    &[types::BOOL],
                    Aux::IntCc(IntCc::Eq),
                )
                .one(),
        )))
    }

    /// `spawn proc f(args)` — [conc.task.root]: pack the evaluated
    /// arguments into an env, synthesize the entry shim over the
    /// (already-lowered) named callee, and spawn under the root
    /// supervisor with the three-outcome return protocol.
    ///
    /// The env is a frame slot here and the PROC'S OWN COPY after the
    /// spawn returns (`[abi.native.procenv]`, s87). s86 gave a task's
    /// capture record a home the task's `scope` owns and joins before
    /// freeing; a proc has no such extent — it is a failure domain
    /// under the root supervisor and outlives its spawner by design —
    /// so no extent at this site is entitled to keep the record alive,
    /// and the runtime copies it into the proc's frame before handing
    /// the id back. That is why a spawn under a loop is sound with ONE
    /// slot per site: the slot is free for reuse the instant the call
    /// returns. (Until s87 this refused "a proc spawned in a loop";
    /// `corpus/conc/proc_spawn_loop.lu` is the witness that it runs.)
    fn lower_proc_spawn(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = wolf_ast::SpawnExpr::cast(e).expect("kind");
        let Some(cs) = self.calls.get(&e.span).copied() else {
            return Err(refuse("a proc spawn without a call record", e.span));
        };
        let Some(cands) = self.fns.get(cs.callee.as_str()) else {
            return Err(refuse("a proc spawn of an unresolvable callee", e.span));
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
                        "a same-named proc callee without a unique declaration locus",
                        e.span,
                    ));
                }
            }
        };
        if !callee_sig.generics.is_empty() || callee_sig.comptime {
            return Err(refuse("generic/comptime proc bodies", e.span));
        }
        if callee_sig
            .params
            .iter()
            .any(|p| p.mode == Some(ParamMode::Mut))
        {
            return Err(refuse("`mut` parameters on proc bodies", e.span));
        }
        // Evaluate the arguments left to right and pack them.
        let mut wtys = Vec::new();
        let mut offs = Vec::new();
        let mut vals = Vec::new();
        let mut size = 0u64;
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            let Some(vexpr) = Arg::value(a) else { continue };
            let Some(v) = flow_val!(self.lower_expr(vexpr)) else {
                return Err(refuse("unit-typed proc-spawn arguments", vexpr.span));
            };
            let wty = self.b.func.value_ty(v);
            let Some(sz) = flat_size(&self.b.module.types, wty) else {
                return Err(refuse(
                    "proc-spawn arguments without a flat layout",
                    vexpr.span,
                ));
            };
            wtys.push(wty);
            offs.push(size);
            vals.push(v);
            size += sz.next_multiple_of(8);
        }
        let (env_region, env) = self.rt_slot(size.max(8));
        for (i, &v) in vals.iter().enumerate() {
            let addr = self.field_addr(env, offs[i]);
            self.store_flat(v, addr, env_region, e.span)?;
        }
        // The body: the callee's ordinary lowered function.
        let body_name = qualify(self.sigs, callee_module, &cs.callee);
        let body_sig = wir_sig_of(
            self.b.module,
            self.sig_table,
            self.sigs,
            callee_sig,
            0,
            e.span,
        )?;
        let task_no = self.pending_tasks.len();
        let base = self.b.func.name.clone();
        let shim_name = format!("{base}.task{task_no}.entry");
        // The declared result, mapped exactly as the sig build did.
        let body_ret = self.b.module.sigs[body_sig].results.first().copied();
        self.pending_tasks.push(PendingTask {
            shim_name: shim_name.clone(),
            body_name,
            body_sig,
            closure: None,
            caps: Vec::new(),
            cap_wtys: wtys,
            cap_offs: offs,
            body_ret,
            span: e.span,
            kind: PendingKind::Task,
        });
        let shim_sig = self.task_shim_sig();
        let shim_ext = self.rt_like_import(&shim_name, shim_sig);
        let entry = self.b.ins_func_addr(shim_ext);
        let (np, nl) = self.name_bytes(&cs.callee);
        // The byte count the runtime copies: the packed size, which
        // is zero for an argument-less body (the slot is still minted
        // at 8 bytes; the runtime copies nothing and passes null).
        let env_len = self.b.iconst(types::I64, size as i64);
        let id = self
            .call_with_slot_token(
                "__wolf_rt_proc_spawn_outcome",
                &[entry, env, env_len, np, nl],
                env_region,
                Some(types::I64),
            )
            .expect("proc id");
        Ok(Flow::Val(Some(id)))
    }

    /// `select { … }` — readiness choice over the s33 pick seam: the
    /// arms array + out-params live in one stack slot; the committed
    /// arm's body runs; TIMEOUT runs the timeout arm; CANCELLED takes
    /// the teardown/escape path ([conc.select.*]).
    fn lower_select_expr(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = wolf_ast::SelectExpr::cast(e).expect("kind");
        struct ChanArm<'t> {
            pat: Option<&'t GreenNode>,
            body: Option<&'t GreenNode>,
            elem: Option<TyId>,
        }
        let mut chan_arms: Vec<ChanArm<'t>> = Vec::new();
        let mut timeout_arm: Option<(&'t GreenNode, Option<&'t GreenNode>)> = None;
        let mut heads: Vec<&'t GreenNode> = Vec::new();
        for arm in d.arms() {
            let body = arm.body();
            let head = arm
                .syntax()
                .nodes()
                .filter(|n| wolf_ast::is_expr_kind(n.kind))
                .find(|n| body.is_none_or(|b| !std::ptr::eq(*n as *const _, b as *const _)));
            if arm.is_timeout() {
                let Some(dur) = head else {
                    return Err(refuse(
                        "a timeout arm without a duration",
                        arm.syntax().span,
                    ));
                };
                timeout_arm = Some((dur, body));
            } else {
                let Some(src) = head else {
                    return Err(refuse("a select arm without a source", arm.syntax().span));
                };
                let elem = self.expr_sema_ty(src.span).and_then(|t| {
                    match self.table.kind(self.strip_sema(t)) {
                        TyKind::Chan(el) => Some(*el),
                        _ => None,
                    }
                });
                heads.push(src);
                chan_arms.push(ChanArm {
                    pat: arm.pattern(),
                    body,
                    elem,
                });
            }
        }
        let n = chan_arms.len();
        // One slot: [arms: 24n][out_val: 8][out_status: 8].
        let arms_bytes = 24u64 * n as u64;
        let (region, slot) = self.rt_slot(arms_bytes + 16);
        for (i, src) in heads.iter().enumerate() {
            let ch = flow_val!(self.lower_expr(src));
            let Some(ch) = ch else {
                return Err(refuse("a select source without a value", src.span));
            };
            let base = self.field_addr(slot, 24 * i as u64);
            self.b.ins_store(ch, base, region);
            let dir_addr = self.field_addr(slot, 24 * i as u64 + 8);
            let zero64 = self.b.iconst(types::I64, 0);
            self.b.ins_store(zero64, dir_addr, region); // dir=0 recv, pad=0
            let val_addr = self.field_addr(slot, 24 * i as u64 + 16);
            self.b.ins_store(zero64, val_addr, region);
        }
        let timeout_ns = match timeout_arm {
            Some((dur, _)) => {
                let v = flow_val!(self.lower_expr(dur));
                let Some(v) = v else {
                    return Err(refuse("a unit-typed timeout duration", dur.span));
                };
                self.widen_to_wire(v, dur.span)?
            }
            None => self.b.iconst(types::I64, -1),
        };
        let nv = self.b.iconst(types::I64, n as i64);
        let has_else = self.b.iconst(types::I8, 0);
        let out_val = self.field_addr(slot, arms_bytes);
        let out_status = self.field_addr(slot, arms_bytes + 8);
        let arms_ptr = slot;
        let picked = self
            .call_with_slot_token(
                "__wolf_rt_select",
                &[arms_ptr, nv, timeout_ns, has_else, out_val, out_status],
                region,
                Some(types::I64),
            )
            .expect("select verdict");
        // Dispatch: 0..n arm bodies, -2 timeout, -4 cancelled.
        let join = self.b.create_block();
        let mut cursor = self.b.current_block();
        let mut arm_blocks = Vec::with_capacity(n);
        // The dispatch chain's constants (arm indices, the -2 timeout
        // sentinel) and comparisons are minted in CHAIN blocks that do
        // not dominate the join or anything after the select — scope
        // every chain step so none of it enters GVN visibility (#64:
        // the second select's sentinel was hash-consed onto the
        // first's, the #40 if-join cross-arm dominance class at s73's
        // select shape).
        for i in 0..n {
            let hit = self.b.create_block();
            let next = self.b.create_block();
            self.b.switch_to_block(cursor);
            self.b.gvn_push_scope();
            let iv = self.b.iconst(types::I64, i as i64);
            let is_i = self
                .b
                .ins(
                    Opcode::Icmp,
                    &[picked, iv],
                    &[types::BOOL],
                    Aux::IntCc(IntCc::Eq),
                )
                .one();
            self.b.ins_br(is_i, hit, &[], next, &[]);
            self.b.gvn_pop_scope();
            self.b.seal_block(hit);
            self.b.seal_block(next);
            arm_blocks.push(hit);
            cursor = next;
        }
        // The residue: timeout, else cancelled/other.
        self.b.switch_to_block(cursor);
        let timeout_bb = self.b.create_block();
        let other_bb = self.b.create_block();
        self.b.gvn_push_scope();
        let tv = self.b.iconst(types::I64, -2);
        let is_t = self
            .b
            .ins(
                Opcode::Icmp,
                &[picked, tv],
                &[types::BOOL],
                Aux::IntCc(IntCc::Eq),
            )
            .one();
        self.b.ins_br(is_t, timeout_bb, &[], other_bb, &[]);
        self.b.gvn_pop_scope();
        self.b.seal_block(timeout_bb);
        self.b.seal_block(other_bb);
        // other: CANCELLED (or an unexpected verdict) — teardown
        // check, then the escape path.
        self.b.switch_to_block(other_bb);
        self.b.gvn_push_scope();
        self.kill_teardown_branch(e.span)?;
        self.cancel_escape(e.span)?;
        self.b.gvn_pop_scope();
        // timeout arm body.
        self.b.switch_to_block(timeout_bb);
        self.b.gvn_push_scope();
        match timeout_arm {
            Some((_, body)) => {
                if self.lower_arm_body(body)? {
                    self.b.ins_jmp(join, &[]);
                }
            }
            None => {
                // No timeout arm: the runtime cannot report TIMEOUT.
                self.b.ins_trap(TrapKind::Assert);
            }
        }
        self.b.gvn_pop_scope();
        // Channel arm bodies.
        for (i, arm) in chan_arms.iter().enumerate() {
            self.b.switch_to_block(arm_blocks[i]);
            self.b.gvn_push_scope();
            self.scopes.push(ScopeFrame::default());
            // Bind the arm pattern from out_val (a committed recv).
            if let Some(pat) = arm.pat {
                match pat.kind {
                    SyntaxKind::IdentPat => {
                        let name = self.text(pat.span);
                        let Some(elem) = arm.elem else {
                            return Err(refuse("a select arm without an element type", pat.span));
                        };
                        let Some(ewty) = self.wir_value_ty(elem, pat.span)? else {
                            return Err(refuse("unit-typed select payloads", pat.span));
                        };
                        let w = self.b.ins_load(types::I64, out_val, region);
                        let v = self.narrow_from_wire(w, ewty, pat.span)?;
                        let var = self.b.declare_var(ewty);
                        self.b.def_var(var, v);
                        self.scopes.last_mut().expect("scope").binds.push((
                            name,
                            LocalBind::Val {
                                var,
                                wrapping: false,
                                unsigned: false,
                                wir_ty: ewty,
                            },
                        ));
                    }
                    SyntaxKind::WildcardPat => {}
                    SyntaxKind::PathPat => {
                        // `exit(reason) from m`: bind the payload
                        // ident to the reason word.
                        if let Some(one) = pat.nodes().find(|nn| nn.kind == SyntaxKind::IdentPat) {
                            let name = self.text(one.span);
                            let w = self.b.ins_load(types::I64, out_val, region);
                            let var = self.b.declare_var(types::I64);
                            self.b.def_var(var, w);
                            self.scopes.last_mut().expect("scope").binds.push((
                                name,
                                LocalBind::Val {
                                    var,
                                    wrapping: false,
                                    unsigned: false,
                                    wir_ty: types::I64,
                                },
                            ));
                        }
                    }
                    _ => {
                        return Err(refuse("this select-arm pattern in lowering", pat.span));
                    }
                }
            }
            let flowed = self.lower_arm_body(arm.body)?;
            let si = self.scopes.len() - 1;
            let still = if flowed {
                self.run_one_scope_exit(si, false)?
            } else {
                false
            };
            self.scopes.pop();
            if still {
                self.b.ins_jmp(join, &[]);
            }
            self.b.gvn_pop_scope();
        }
        self.b.seal_block(join);
        self.b.switch_to_block(join);
        Ok(Flow::Val(None))
    }

    /// Lower a select arm's body (block or expression); true when the
    /// path still flows.
    fn lower_arm_body(&mut self, body: Option<&'t GreenNode>) -> R<bool> {
        match body {
            Some(b) if b.kind == SyntaxKind::Block => {
                let blk = AstBlock::cast(b).expect("kind");
                Ok(!matches!(self.lower_block(blk, false)?, Flow::Diverged))
            }
            Some(b) => Ok(!matches!(self.lower_expr_w(b, false)?, Flow::Diverged)),
            None => Ok(true),
        }
    }

    /// `when (a, b) { … }` — whole-set acquisition over the s33 seam
    /// ([conc.when.order]): cells array in a slot, acquire, payload
    /// rebinds over the body, release on every exit edge.
    fn lower_when_expr(&mut self, e: &'t GreenNode, want: bool) -> R<Flow> {
        let d = wolf_ast::WhenExpr::cast(e).expect("kind");
        let ops: Vec<&GreenNode> = d.operands().collect();
        let n = ops.len() as i64;
        let (region, slot) = self.rt_slot((ops.len() as u64 * 8).max(8));
        let mut cells = Vec::with_capacity(ops.len());
        for (i, op) in ops.iter().enumerate() {
            let cell = flow_val!(self.lower_expr(op));
            let Some(cell) = cell else {
                return Err(refuse("a `when` operand without a value", op.span));
            };
            let addr = self.field_addr(slot, 8 * i as u64);
            self.b.ins_store(cell, addr, region);
            cells.push(cell);
        }
        let nv = self.b.iconst(types::I64, n);
        let status = self
            .call_with_slot_token(
                "__wolf_rt_when_acquire",
                &[slot, nv],
                region,
                Some(types::I32),
            )
            .expect("acquire status");
        // Status 2: cancelled mid-set — teardown check, then escape.
        let two = self.b.iconst(types::I32, 2);
        let cancelled = self
            .b
            .ins(
                Opcode::Icmp,
                &[status, two],
                &[types::BOOL],
                Aux::IntCc(IntCc::Eq),
            )
            .one();
        let cbb = self.b.create_block();
        let body_bb = self.b.create_block();
        self.b.ins_br(cancelled, cbb, &[], body_bb, &[]);
        self.b.seal_block(cbb);
        self.b.seal_block(body_bb);
        self.b.switch_to_block(cbb);
        self.b.gvn_push_scope();
        self.kill_teardown_branch(e.span)?;
        self.cancel_escape(e.span)?;
        self.b.gvn_pop_scope();
        self.b.switch_to_block(body_bb);
        // The body, with payload rebinds and the release on every
        // exit edge (the scope frame's when_release).
        self.scopes.push(ScopeFrame {
            when_release: Some((slot, region, n)),
            ..ScopeFrame::default()
        });
        for (i, op) in ops.iter().enumerate() {
            if op.kind == SyntaxKind::PathExpr {
                let name = self.text(op.span);
                self.scopes
                    .last_mut()
                    .expect("scope")
                    .binds
                    .push((name, LocalBind::SyncPayload { cell: cells[i] }));
            }
        }
        let out = match d.body() {
            Some(b) => self.lower_block(b, want),
            None => Ok(Flow::Val(None)),
        };
        let flow = match out {
            Ok(f) => f,
            Err(x) => {
                self.scopes.pop();
                return Err(x);
            }
        };
        if matches!(flow, Flow::Diverged) {
            self.scopes.pop();
            return Ok(Flow::Diverged);
        }
        // Fall-through: release now (the frame's entry), then pop.
        let si = self.scopes.len() - 1;
        let flowing = self.run_one_scope_exit(si, false);
        self.scopes.pop();
        if !flowing? {
            return Ok(Flow::Diverged);
        }
        Ok(flow)
    }

    /// `for v in ch { … }` — drain to drained-close
    /// ([conc.chan.close]); cancellation ends the iteration after the
    /// teardown check ([conc.cancel.points]).
    fn lower_for_chan(
        &mut self,
        d: ForExpr<'t>,
        iter: &'t GreenNode,
        elem: TyId,
        span: Span,
    ) -> R<Flow> {
        let ch = flow_val!(self.lower_expr(iter));
        let Some(ch) = ch else {
            return Err(refuse("a channel iteration without a handle", iter.span));
        };
        let Some(ewty) = self.wir_value_ty(elem, span)? else {
            return Err(refuse("unit-typed channel elements", span));
        };
        let bind_name = match d.pattern() {
            None => None,
            Some(p) if p.kind == SyntaxKind::IdentPat => Some(self.text(p.span)),
            Some(p) if p.kind == SyntaxKind::WildcardPat => None,
            Some(p) => {
                return Err(refuse("destructuring channel `for` patterns", p.span));
            }
        };
        let (region, slot) = self.rt_slot(8);
        let header = self.b.create_block();
        let body_bb = self.b.create_block();
        let exit_bb = self.b.create_block();
        self.b.ins_jmp(header, &[]);
        self.b.switch_to_block(header);
        // The loop's GVN scope (s74, #66): values born in the header and
        // the body do not dominate what follows the loop, so they must
        // not be reusable there. `for` over a range and over a `List`
        // both scope their loops this way; this one did not, and a
        // constant minted inside a channel loop's body was hash-consed
        // into a later block it does not dominate.
        self.b.gvn_push_scope();
        let status = self
            .rt_call_slot("__wolf_rt_chan_recv", &[ch], slot, region, Some(types::I32))
            .expect("recv status");
        let zero = self.b.iconst(types::I32, 0);
        let is_ok = self
            .b
            .ins(
                Opcode::Icmp,
                &[status, zero],
                &[types::BOOL],
                Aux::IntCc(IntCc::Eq),
            )
            .one();
        let notok_bb = self.b.create_block();
        self.b.ins_br(is_ok, body_bb, &[], notok_bb, &[]);
        self.b.seal_block(body_bb);
        self.b.seal_block(notok_bb);
        // Not ok: 1 closed → exit; 2 cancelled → teardown check, then
        // exit (iteration ends; the frame's defers run by ordinary
        // control flow — [conc.cancel.defer]).
        self.b.switch_to_block(notok_bb);
        self.b.gvn_push_scope();
        let two = self.b.iconst(types::I32, 2);
        let is_cancel = self
            .b
            .ins(
                Opcode::Icmp,
                &[status, two],
                &[types::BOOL],
                Aux::IntCc(IntCc::Eq),
            )
            .one();
        let cancel_bb = self.b.create_block();
        self.b.ins_br(is_cancel, cancel_bb, &[], exit_bb, &[]);
        self.b.seal_block(cancel_bb);
        self.b.switch_to_block(cancel_bb);
        self.kill_teardown_branch(span)?;
        self.b.ins_jmp(exit_bb, &[]);
        self.b.gvn_pop_scope();
        // The body.
        self.b.switch_to_block(body_bb);
        self.scopes.push(ScopeFrame::default());
        if let Some(name) = bind_name {
            let w = self.b.ins_load(types::I64, slot, region);
            let v = self.narrow_from_wire(w, ewty, span)?;
            let var = self.b.declare_var(ewty);
            self.b.def_var(var, v);
            self.scopes.last_mut().expect("scope").binds.push((
                name,
                LocalBind::Val {
                    var,
                    wrapping: false,
                    unsigned: false,
                    wir_ty: ewty,
                },
            ));
        }
        self.loops.push(LoopFrame {
            continue_to: ContinueTo::Block(header),
            exit: Some(exit_bb),
            exit_param: None,
            depth: self.scopes.len(),
        });
        let body_flow = match d.body() {
            Some(b) => self.lower_block(b, false),
            None => Ok(Flow::Val(None)),
        };
        self.loops.pop();
        let body_flow = match body_flow {
            Ok(f) => f,
            Err(x) => {
                self.scopes.pop();
                self.b.gvn_pop_scope();
                return Err(x);
            }
        };
        self.scopes.pop();
        if !matches!(body_flow, Flow::Diverged) {
            self.b.ins_jmp(header, &[]);
        }
        self.b.gvn_pop_scope();
        self.b.seal_block(header);
        self.b.seal_block(exit_bb);
        self.b.switch_to_block(exit_bb);
        Ok(Flow::Val(None))
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
                // s40: str comparison — byte equality and the
                // byte-lexicographic order ([mem.str.order]). Both are
                // INLINE: s81 did equality ([`Self::str_eq_inline`]),
                // s84 the relational family ([`Self::str_cmp_inline`],
                // wolf-lang#94). `__wolf_rt_str_cmp` survives as the
                // long-operand escape and the FFI entry, and the
                // checked executor's `Ord` on bytes stays the reference
                // both spellings answer to.
                let lhs_is_str = d
                    .lhs()
                    .and_then(|l| self.expr_sema_ty(l.span))
                    .map(|t| matches!(self.table.kind(self.strip_sema(t)), TyKind::Prim(Prim::Str)))
                    .unwrap_or(false);
                if lhs_is_str {
                    let (ap, al) = self.str_parts(a);
                    let (bp, bl) = self.str_parts(bv);
                    if matches!(op, SyntaxKind::EqEq | SyntaxKind::NotEq) {
                        let want_eq = op == SyntaxKind::EqEq;
                        return Ok(Flow::Val(Some(self.str_eq_inline(ap, al, bp, bl, want_eq))));
                    }
                    let cc = match op {
                        SyntaxKind::Lt => IntCc::Slt,
                        SyntaxKind::Gt => IntCc::Sgt,
                        SyntaxKind::LtEq => IntCc::Sle,
                        _ => IntCc::Sge,
                    };
                    return Ok(Flow::Val(Some(self.str_cmp_inline(ap, al, bp, bl, cc))));
                }
                // s88 (wolf-lang#100): `bool` equality is one `icmp` on
                // the i8-shaped flag — nothing about comparing two
                // one-bit values needs a wider gate than the integers
                // already get. Ordering never arrives here: typecheck
                // refuses `<`/`>`/`<=`/`>=` on `bool` (the relational
                // arm of `synth_bin` unifies against a numeric probe),
                // so the arm below is the honest floor, not a policy.
                if ty == types::BOOL {
                    let cc = match op {
                        SyntaxKind::EqEq => IntCc::Eq,
                        SyntaxKind::NotEq => IntCc::Ne,
                        _ => return Err(refuse("ordering comparison on `bool`", e.span)),
                    };
                    return Ok(Flow::Val(Some(
                        self.b
                            .ins(Opcode::Icmp, &[a, bv], &[types::BOOL], Aux::IntCc(cc))
                            .one(),
                    )));
                }
                if !types_is_int(ty) {
                    return Err(refuse(
                        "comparison outside integers, floats, `bool` and `str` \
                         (enum compares, c06/std)",
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
            CastKind::Unsize => self.lower_dyn_cast(e, v, from, to),
        }
    }

    /// s98 (D47): `place as dyn Trait` — build the pair.
    ///
    /// The DATA half is the place's spilled identity: the operand
    /// value flat-stores into a `stack.alloc` slot, the same
    /// convention `mut` arguments already use (s26) — wolf locals are
    /// SSA values, and the slot is the only memory identity a local
    /// has. The mem tier's shared loan forbids writes to the place
    /// while the pair is live, so slot-vs-place is unobservable; the
    /// slot is frame-lived and the pair cannot leave the frame (the
    /// depth-0 rule, the return guard), so the pointer never dangles.
    ///
    /// The VTABLE half is a content-interned fn-pointer table: one
    /// slot per method in the trait's `DynReport` canonical order,
    /// each pointing at an erased-shape shim over the s95 instance
    /// (`[abi.native.dyn]`: the shim is the table's problem, never
    /// the call site's). Two casts of one (trait, impl) pair share
    /// ONE table (D8's discipline applied to data).
    fn lower_dyn_cast(
        &mut self,
        e: &'t GreenNode,
        v: Option<Value>,
        from: TyId,
        to: TyId,
    ) -> R<Flow> {
        let Some(v) = v else {
            return Err(refuse("a dyn cast of a unit value", e.span));
        };
        let sigs = self.sigs;
        let TyKind::Dyn {
            module: tmod,
            name: tname,
        } = self.table.kind(self.strip_sema(to)).clone()
        else {
            return Err(refuse("a dyn cast whose target is not a dyn", e.span));
        };
        let tr = wolf_sema::traits::TraitRef {
            module: tmod as usize,
            name: tname.clone(),
        };
        let Some(td) = sigs.traits.get(&tr) else {
            return Err(refuse("a dyn cast on an unelaborated trait", e.span));
        };
        if !td.dyn_report.safe() {
            return Err(refuse_named(
                format!("a dyn cast to `{tname}`, which is not dyn-safe (object safety owns this)"),
                e.span,
            ));
        }
        let src = self.strip_sema(from);
        let TyKind::Nominal { name: head, .. } = self.table.kind(src).clone() else {
            return Err(refuse_named(
                format!("a dyn cast of a non-nominal type to `{tname}`"),
                e.span,
            ));
        };
        let Some(imp) = sigs.impls.iter().find(|i| {
            i.trait_ref.as_ref() == Some(&tr)
                && matches!(
                    sigs.table.kind(i.self_ty),
                    TyKind::Nominal { name: n, .. } if n == &head
                )
        }) else {
            return Err(refuse_named(
                format!("a dyn cast without a coherent `{tname}` impl for `{head}`"),
                e.span,
            ));
        };
        if !imp.generics.is_empty() {
            return Err(refuse_named(
                format!("a generic `{tname}` impl behind a dyn cast (instantiate-at-cast)"),
                e.span,
            ));
        }
        // The data half: spill the operand into its slot.
        let Some(src_w) = wir_ty(&mut self.b.module.types, self.table, sigs, from, e.span)? else {
            return Err(refuse("a dyn cast of a unit-typed value", e.span));
        };
        let Some(size) = flat_size(&self.b.module.types, src_w) else {
            return Err(refuse(
                "a dyn cast of a value without a flat layout",
                e.span,
            ));
        };
        let (slot_region, slot) = self.b.ins_stack_alloc(size);
        self.b.func.add_fact(FactData::new(
            FactKind::Region(slot, slot_region),
            Just::DefOp,
        ));
        self.b.func.add_fact(FactData::new(
            FactKind::Deref(slot, DerefSize::Const(size)),
            Just::DefOp,
        ));
        self.store_flat(v, slot, slot_region, e.span)?;
        // The vtable half: one shim per canonical slot.
        let mut slot_fns = Vec::with_capacity(td.dyn_report.methods.len());
        for m in &td.dyn_report.methods {
            let Some(tm) = td.method(m).filter(|mm| mm.has_self) else {
                return Err(refuse("a dyn slot without a receiver method", e.span));
            };
            // The erased shape is the TRAIT's declaration; the same
            // guards the dyn CALL path holds (a slot no call site
            // could reach honestly is a slot this table refuses).
            if let Some(mode) = tm.sig.params.first().and_then(|p| p.mode) {
                let kw = match mode {
                    ParamMode::Mut => "mut",
                    ParamMode::Take => "take",
                };
                return Err(refuse_named(
                    format!(
                        "a `{kw} self` method behind a dyn cast (the erased receiver is a pointer)"
                    ),
                    e.span,
                ));
            }
            for p in tm.sig.params.iter().skip(1) {
                if p.mode.is_some() {
                    return Err(refuse_named(
                        format!("a moded parameter behind a dyn `{tname}.{m}` slot"),
                        e.span,
                    ));
                }
                if mentions_self(self.sig_table, p.ty) {
                    return Err(refuse_named(
                        "a `Self`-typed parameter behind a dyn slot (object safety owns this)"
                            .to_string(),
                        e.span,
                    ));
                }
            }
            if mentions_self(self.sig_table, tm.sig.ret) {
                return Err(refuse_named(
                    "a `Self`-typed return behind a dyn slot (object safety owns this)".to_string(),
                    e.span,
                ));
            }
            // The slot's real target: the impl's own body, or the
            // trait's default monomorphized over `Self ↦ head` (the
            // s95 mangling, exactly).
            let (target, target_sig) = match imp.methods.iter().find(|mm| mm.name == *m) {
                Some(mm) => {
                    if !mm.sig.generics.is_empty() {
                        return Err(refuse_named(
                            format!("a generic method `{m}` behind a dyn slot"),
                            e.span,
                        ));
                    }
                    let name = format!("{head}.{tname}.{m}");
                    let sig = wir_sig_of(self.b.module, self.sig_table, sigs, &mm.sig, 0, e.span)?;
                    (name, sig)
                }
                None => {
                    let base = format!("{tname}.{m}");
                    let bound = freeze(self.table, src);
                    let mut scratch = TypeTable::new();
                    let t = thaw(&bound, &mut scratch);
                    let key = SpecKey {
                        mask: 0,
                        subst: vec![("Self".to_string(), mono_spelling(&scratch, t))],
                    };
                    let full = spec_name(&base, &key);
                    self.pending_specs.push(SpecRequest {
                        name: base,
                        key,
                        bindings: vec![("Self".to_string(), bound.clone())],
                        span: e.span,
                    });
                    let mut st = self.sig_table.clone();
                    let map: std::collections::BTreeMap<String, TyId> =
                        std::iter::once(("Self".to_string(), thaw(&bound, &mut st))).collect();
                    let mut f = tm.sig.clone();
                    for p in &mut f.params {
                        p.ty = wolf_sema::types::subst(&mut st, p.ty, &map);
                    }
                    f.ret = wolf_sema::types::subst(&mut st, f.ret, &map);
                    let sig = wir_sig_of(self.b.module, &st, sigs, &f, 0, e.span)?;
                    (full, sig)
                }
            };
            // The erased signature: ptr receiver + the declared tail.
            let mut params = vec![Param {
                ty: types::PTR,
                mode: Mode::Val,
            }];
            for p in tm.sig.params.iter().skip(1) {
                let Some(w) = wir_ty(&mut self.b.module.types, self.sig_table, sigs, p.ty, e.span)?
                else {
                    return Err(refuse("unit-typed parameters behind a dyn slot", e.span));
                };
                params.push(Param {
                    ty: w,
                    mode: Mode::Val,
                });
            }
            let results = match wir_ty(
                &mut self.b.module.types,
                self.sig_table,
                sigs,
                tm.sig.ret,
                e.span,
            )? {
                Some(t) => vec![t],
                None => vec![],
            };
            let erased_sig = self.b.module.make_sig(params, results);
            let shim = format!("{target}.dynshim");
            self.pending_dyn_shims.push(DynShim {
                name: shim.clone(),
                target,
                target_sig,
                erased_sig,
                recv_ty: src_w,
                span: e.span,
            });
            slot_fns.push(shim);
        }
        let hint = format!("vt.{head}.{tname}");
        let (idx, fresh) = self.b.module.intern_fn_table(&hint, &slot_fns);
        self.b.stats.vtables_demanded += 1;
        if fresh {
            self.b.stats.vtables_unique += 1;
        }
        let vt = self.b.ins_data_addr(idx);
        let pair_ty = self
            .b
            .module
            .types
            .intern(types::TypeData::Agg(vec![types::PTR, types::PTR]));
        let pair = self
            .b
            .ins(Opcode::AggMake, &[slot, vt], &[pair_ty], Aux::None)
            .one();
        Ok(Flow::Val(Some(pair)))
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
            // s73: region-typed results are ptr handles (a recv'd
            // region unwrapping) — `wir_value_ty`'s rule.
            Some(t) => self.wir_value_ty(t, e.span)?.is_some(),
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
                // widening). Each coercion gets its OWN GVN scope:
                // the arm-end blocks are siblings, and a coercion op
                // with equal keys on both sides (a NULLARY
                // `eu.make.ok` for unit arms — s40's fs family made
                // these real; an injected tag iconst) must not
                // GVN-share across them, or the "equal arm values
                // need no parameter" test below sees one value that
                // dominates neither edge.
                self.b.switch_to_block(then_end);
                self.b.gvn_push_scope();
                let tv = if want_v {
                    self.arm_to_merge(tv, merge_eu, e.span)
                } else {
                    Ok(tv)
                };
                self.b.gvn_pop_scope();
                let tv = tv?;
                let then_end = self.b.current_block();
                self.b.switch_to_block(else_end);
                self.b.gvn_push_scope();
                let ev = if want_v {
                    self.arm_to_merge(ev, merge_eu, e.span)
                } else {
                    Ok(ev)
                };
                self.b.gvn_pop_scope();
                let ev = ev?;
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

    /// Materialize a folded comptime value (s71) as ordinary constants
    /// at the call site's checked type — the fold reaches the lane.
    fn lower_fold(&mut self, f: &'t Fold, e: &'t GreenNode) -> R<Flow> {
        match f {
            Fold::Unit => Ok(Flow::Val(None)),
            Fold::Bool(b) => Ok(Flow::Val(Some(self.b.bconst(*b)))),
            Fold::Str(s) => Ok(Flow::Val(Some(self.str_value(s.as_bytes())))),
            Fold::Int(v) => {
                let Some(sema_ty) = self.expr_sema_ty(e.span) else {
                    return Err(refuse("a comptime fold without a recorded type", e.span));
                };
                let Some(wty) = wir_ty(
                    &mut self.b.module.types,
                    self.table,
                    self.sigs,
                    sema_ty,
                    e.span,
                )?
                else {
                    return Err(refuse("a unit-typed integer fold", e.span));
                };
                if sema_unsigned(self.table, sema_ty) {
                    let Some(width) = self.b.module.types.int_bits(wty) else {
                        return Err(refuse("a non-integer unsigned fold", e.span));
                    };
                    let payload = wrap_bits(*v as u64, width);
                    return Ok(Flow::Val(Some(self.b.iconst(wty, payload))));
                }
                Ok(Flow::Val(Some(self.b.iconst(wty, *v as i64))))
            }
            Fold::Float(v) => {
                let Some(sema_ty) = self.expr_sema_ty(e.span) else {
                    return Err(refuse("a comptime fold without a recorded type", e.span));
                };
                let Some(wty) = wir_ty(
                    &mut self.b.module.types,
                    self.table,
                    self.sigs,
                    sema_ty,
                    e.span,
                )?
                else {
                    return Err(refuse("a unit-typed float fold", e.span));
                };
                let bits = if wty == types::F32 {
                    (*v as f32).to_bits() as u64
                } else {
                    v.to_bits()
                };
                Ok(Flow::Val(Some(self.b.fconst(wty, bits))))
            }
        }
    }

    fn lower_call(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = CallExpr::cast(e).expect("kind");
        // A folded comptime call site (s71) IS its value: emit the
        // constant and never look at the callee or arguments (a type
        // argument has no runtime lowering at all).
        if let Some(f) = self.folds.get(&e.span) {
            let f = *f;
            return self.lower_fold(f, e);
        }
        let cs: Option<&CallSig> = self.calls.get(&e.span).copied();
        // Builtins without a signature: assert / print.
        let callee_text = d.callee().map(|c| self.text(c.span)).unwrap_or_default();
        // The s38 io/fs builtin tier, natively (s40): rows ride the
        // s29 eu shape, text results materialize through the ambient
        // region (`wolf_rt::fs`).
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
                // The s90 surface (#51/#52) lands on BOTH lanes in the
                // sprint that introduces it: a std.fs that can only
                // list a directory under the checked executor is not a
                // std.fs.
                | "fs_open_mode"
                | "fs_read_bytes"
                | "fs_write_bytes"
                | "fs_read_chunk"
                | "fs_write_chunk"
                | "fs_read_dir"
                | "fs_create_dir"
                | "fs_create_dir_all"
                | "fs_remove_dir"
                | "fs_remove_dir_all"
                | "fs_rename"
                | "fs_is_file"
                | "fs_is_dir"
                | "fs_size"
                | "fs_modified_ms"
        ) {
            return self.lower_fs_builtin(&callee_text, d, e);
        }
        // The s39 net builtin tier, natively (s106 — c26's first
        // crossing): one `wolf_rt::net` shim per call over the process
        // NetTable, codes to row tags, str results through out slots —
        // the fs pattern, family for family. `net_deadline` arms the
        // s35 reactor budget, making the `timeout` tag reachable
        // natively (#45's builtin half).
        if matches!(
            callee_text.as_str(),
            "net_listen"
                | "net_port"
                | "net_accept"
                | "net_connect"
                | "net_read"
                | "net_write"
                | "net_close"
                | "net_deadline"
        ) {
            return self.lower_net_builtin(&callee_text, d, e);
        }
        // The s40 os/env and time builtin tiers, natively: same eu
        // shape as fs — codes become row tags, str results
        // materialize through the ambient region (`wolf_rt::{os,time}`
        // mirror the checked `os_builtin`/`time_builtin` entry for
        // entry).
        if matches!(
            callee_text.as_str(),
            "env_args"
                | "env_get"
                | "env_set"
                | "env_vars"
                | "os_cwd"
                | "os_exe"
                | "os_exit"
                | "time_now_ms"
                | "time_unix_ms"
                | "time_sleep_ms"
        ) {
            return self.lower_os_time_builtin(&callee_text, d, e);
        }
        // The s40 process trio, natively (s107 — c26's LAST crossing;
        // #118 closes): the argv `List[str]` header crosses as one
        // pointer to `wolf_rt::os`'s ChildTable shims, codes to row
        // tags, the exit code back through an out word. Wait reaps;
        // kill never tombstones — the zombie discipline is the
        // runtime's module doc.
        if matches!(callee_text.as_str(), "os_spawn" | "os_wait" | "os_kill") {
            return self.lower_process_builtin(&callee_text, d, e);
        }
        // The s40 json builtin tier, natively (s107): the reference
        // parser stays the checked lane's (`wolf_mem::json`);
        // `wolf_rt::json` is its hand mirror (the locked graph keeps
        // the reference out of the runtime's reach — D15) and the
        // driver's json_parity test pins the two. With this arm the
        // last checked-lane-only refusal leaves the lowering.
        if matches!(
            callee_text.as_str(),
            "json_valid" | "json_get" | "json_type" | "json_len"
        ) {
            return self.lower_json_builtin(&callee_text, d, e);
        }
        // s81 (#58): the validating byte source. Unlike json it lands on
        // BOTH lanes in the sprint that introduces it — a border post
        // only wolf-std's checked tests can cross would not be a border
        // post.
        if callee_text == "str_from_utf8" {
            return self.lower_str_from_utf8(d, e);
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
        // s95: a qualified `Trait.method(v)` carries the same dispatch
        // record a method call does (the checker writes it; s18 says
        // read, never re-derive). Without the record this path falls
        // through to the free-fn map and refuses there, as before.
        if let Some(&disp) = self.dispatch.get(&e.span)
            && let Dispatch::Trait {
                module,
                name,
                method,
                dyn_call,
            } = disp
        {
            if *dyn_call {
                // s96: the receiver is the leading `Self` argument
                // (the trait method's params[0]); everything after it
                // is an ordinary argument.
                let args: Vec<_> = d.args().into_iter().flat_map(|l| l.args()).collect();
                let Some(recv) = args.first().copied().and_then(Arg::value) else {
                    return Err(refuse("a dyn call without a receiver argument", e.span));
                };
                let rest: Vec<_> = args[1..].to_vec();
                return self.lower_dyn_trait_call(recv, rest, *module, name, method, e);
            }
            return self.lower_qualified_trait_call(d, cs, *module, name, method, e);
        }
        if cs.decl_span.is_none() {
            return self.lower_indirect_call(d, cs, e);
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
        // s93: a generic callee is called as an INSTANCE. Bind each of
        // its parameters from what the checker already decided at this
        // site — the argument expressions' final types against the
        // declared parameter types, and the call's own type against
        // the declared return — then lower the call against the
        // instance's substituted signature. Every rigid must bind; one
        // the site never constrains (used only in a position the
        // checker defaulted away) refuses by name.
        let bindings: Vec<(String, Bound)> = if callee_sig.generics.is_empty() {
            Vec::new()
        } else {
            self.bind_generics(callee_sig, d, e)?
        };
        if callee_sig.comptime {
            // Folded sites returned above (s71); reaching here means
            // the evaluated value has no scalar/str runtime shape yet.
            return Err(refuse(
                "a comptime fold without a runtime value shape (aggregates)",
                e.span,
            ));
        }
        // Arguments under their declared modes.
        //
        // s89 (#86): which arguments cross as a byte VIEW rather than a
        // materialized `List[int]`. Two conditions, both necessary: the
        // argument is a view here (`s.bytes()`, or a view this function
        // was itself lent), and the memory checker's lend analysis
        // proved the callee's parameter `Lendable` — every use inside
        // one of s77's seven read positions. `Opaque` AND `Escapes`
        // materialize, bit-for-bit the pre-s89 behaviour — an escape
        // is W1004 at `mem` (s92; E1015 refused it through s91) and
        // reaches lowering as an ordinary by-value argument.
        let mut view_mask = 0u32;
        for (i, a) in d.args().into_iter().flat_map(|l| l.args()).enumerate() {
            let Some(vexpr) = Arg::value(a) else { continue };
            if i >= 32 || a.mode().is_some() {
                continue;
            }
            if self.view_src(vexpr).is_some() && self.lender.param(callee_sig, i) == Lend::Lendable
            {
                view_mask |= 1u32 << i;
            }
        }
        // The `mut` argument is s25's refusal repaid: the local spills
        // to a `stack.alloc` slot (its own one-slot region — stack
        // provenance, the s19 promotion landing pad), the callee gets
        // (ptr, token), and the local reloads on return. Re-lending a
        // `mut` PARAMETER passes its pointer and region straight
        // through — no copy, and the exclusivity theorem survives the
        // hop.
        let mut args = Vec::new();
        let mut formal_regions: HashMap<u32, RegionId> = HashMap::new();
        let mut next_formal = 0u32;
        let mut writebacks: Vec<WriteBack> = Vec::new();
        let mut spilled_slots: Vec<Value> = Vec::new();
        for (i, a) in d.args().into_iter().flat_map(|l| l.args()).enumerate() {
            let mode = cs.params.get(i).and_then(|p| p.mode);
            let Some(vexpr) = Arg::value(a) else { continue };
            if view_mask & (1u32 << i) != 0 {
                let src = self.view_src(vexpr).expect("decided above");
                let Some((ptr, len)) = self.lower_view(src)? else {
                    return Ok(Flow::Diverged);
                };
                args.push(ptr);
                args.push(len);
                continue;
            }
            if mode == Some(ParamMode::Take) {
                self.check_capture_write(vexpr, "consuming (`take`)")?;
            }
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
        let base_name = qualify(self.sigs, callee_module, &cs.callee);
        // A specialized call names the CLONE/INSTANCE, and queues it.
        // The request is idempotent — `lower_package` emits one body per
        // (callee, key) however many call sites ask for it (s89 for the
        // mask, s93 for the substitution; one worklist).
        let key = SpecKey {
            mask: view_mask,
            subst: bindings
                .iter()
                .map(|(n, b)| {
                    // Spell through a scratch table: the tree is
                    // table-free, and the spelling must be the same
                    // wherever it is thawed.
                    let mut scratch = TypeTable::new();
                    let t = thaw(b, &mut scratch);
                    (n.clone(), mono_spelling(&scratch, t))
                })
                .collect(),
        };
        let callee_name = if key.is_plain() {
            base_name
        } else {
            let full = spec_name(&base_name, &key);
            self.pending_specs.push(SpecRequest {
                name: base_name,
                key,
                bindings: bindings.clone(),
                span: e.span,
            });
            full
        };
        let ext = match self.callees.get(&callee_name) {
            Some(&ext) => ext,
            None => {
                // The import's signature must be the INSTANCE's, built
                // from the same substituted table the definition will
                // build from, so both sides of the call agree.
                let sig = if bindings.is_empty() {
                    wir_sig_of(
                        self.b.module,
                        self.sig_table,
                        self.sigs,
                        callee_sig,
                        view_mask,
                        e.span,
                    )?
                } else {
                    let mut st = self.sig_table.clone();
                    let map: std::collections::BTreeMap<String, TyId> = bindings
                        .iter()
                        .map(|(n, b)| (n.clone(), thaw(b, &mut st)))
                        .collect();
                    let mut f = callee_sig.clone();
                    for p in &mut f.params {
                        p.ty = wolf_sema::types::subst(&mut st, p.ty, &map);
                    }
                    f.ret = wolf_sema::types::subst(&mut st, f.ret, &map);
                    wir_sig_of(self.b.module, &st, self.sigs, &f, view_mask, e.span)?
                };
                let ext = self.b.func.import_func(callee_name.clone(), sig);
                self.callees.insert(callee_name, ext);
                ext
            }
        };
        let results = self.b.ins_call_regions(ext, &args, &formal_regions);
        self.run_writebacks(writebacks)?;
        Ok(Flow::Val(results.first().copied()))
    }

    /// s97 (#112): a call whose callee is an EXPRESSION — a fn-typed
    /// parameter, local, or read module fn. The demand side is s95's
    /// (the READ already instantiated whatever the value names); this
    /// site lowers the callee to its ptr, the arguments by value, and
    /// calls through the value with the sig built from the callee's
    /// recorded fn TYPE — already substituted in an instance body, so
    /// a `Rigid` never reaches `wir_ty` here. Fn types carry no modes
    /// and no region tokens (target 4): the sig is by-value token-free
    /// by construction, and the eu row rides in the return type, so
    /// `?`/`else` at this site marshal exactly as at a direct call.
    fn lower_indirect_call(&mut self, d: CallExpr<'t>, cs: &CallSig, e: &'t GreenNode) -> R<Flow> {
        let Some(callee) = d.callee() else {
            return Err(refuse("calls outside the resolved surface", e.span));
        };
        // s105: a call through a CLOSURE binding routes through the
        // pair (env leading); everything else is s97's plain fn-value
        // call.
        if callee.kind == SyntaxKind::PathExpr {
            let name = self.text(callee.span);
            if let Some(LocalBind::Closure { pair }) = self.lookup(&name) {
                return self.lower_closure_call(pair, d, cs, e);
            }
        }
        let Some(fn_ty) = self.expr_sema_ty(callee.span) else {
            return Err(refuse(
                "an indirect callee without a recorded type",
                callee.span,
            ));
        };
        let TyKind::Fn(param_tys, ret_ty) = self.table.kind(fn_ty).clone() else {
            return Err(refuse(
                "an indirect callee that is not fn-typed",
                callee.span,
            ));
        };
        // Belt and suspenders for target 4: sema synthesizes MODELESS
        // params for fn-typed callees today (`call_by_type`), and s18
        // rejects call-site `mut`/`take` against them. If either ever
        // changes, refuse by name rather than guess a convention.
        if cs.params.iter().any(|p| p.mode.is_some()) {
            return Err(refuse(
                "an indirect callee with parameter modes (fn types carry none)",
                e.span,
            ));
        }
        let Some(fv) = flow_val!(self.lower_expr(callee)) else {
            return Err(refuse("unit-typed callees", callee.span));
        };
        let mut args = Vec::new();
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            let Some(vexpr) = Arg::value(a) else { continue };
            let Some(v) = flow_val!(self.lower_expr(vexpr)) else {
                return Err(refuse("unit-typed arguments", vexpr.span));
            };
            args.push(v);
        }
        let table = self.table;
        let mut params = Vec::with_capacity(param_tys.len());
        for &pt in &param_tys {
            let Some(w) = wir_ty(&mut self.b.module.types, table, self.sigs, pt, e.span)? else {
                return Err(refuse("unit-typed parameters", e.span));
            };
            params.push(Param {
                ty: w,
                mode: Mode::Val,
            });
        }
        let results = match wir_ty(&mut self.b.module.types, table, self.sigs, ret_ty, e.span)? {
            Some(t) => vec![t],
            None => vec![],
        };
        let sig = self.b.module.make_sig(params, results);
        Ok(Flow::Val(self.b.ins_call_ind(fv, sig, &args)))
    }

    /// s93: recover a generic callee's bindings at one call site from
    /// what the checker already decided — each argument expression's
    /// final type against the declared parameter type, and the call
    /// expression's own type against the declared return. The walk is
    /// structural over the two tables (declared types live in the
    /// signature table, site types in this body's table); a `Rigid` on
    /// the declared side binds to the site type, frozen; a second
    /// sighting of the same rigid must freeze identically. Anything
    /// left unbound is a refusal BY NAME: dictionary passing is not a
    /// fallback here, and no `<error>` reaches WIR.
    ///
    /// Deliberately not sema's unifier: at a checked call the bindings
    /// are already ground (`TypedBody.exprs` holds defaulted types), so
    /// a one-way structural match is all there is to do.
    fn bind_generics(
        &self,
        callee_sig: &FnSig,
        d: CallExpr<'t>,
        e: &'t GreenNode,
    ) -> R<Vec<(String, Bound)>> {
        let mut map: std::collections::BTreeMap<String, Bound> = std::collections::BTreeMap::new();
        let args: Vec<_> = d.args().into_iter().flat_map(|l| l.args()).collect();
        for (i, p) in callee_sig.params.iter().enumerate() {
            let Some(a) = args.get(i) else { break };
            let Some(vexpr) = Arg::value(*a) else {
                continue;
            };
            let Some(site) = self.expr_sema_ty(vexpr.span) else {
                continue;
            };
            self.match_binding(p.ty, site, &mut map, e.span)?;
        }
        if let Some(site_ret) = self.expr_sema_ty(e.span) {
            self.match_binding(callee_sig.ret, site_ret, &mut map, e.span)?;
        }
        let mut out = Vec::with_capacity(callee_sig.generics.len());
        for g in &callee_sig.generics {
            let Some(b) = map.remove(&g.name) else {
                return Err(refuse_named(
                    format!(
                        "an instantiation with an unbound parameter (`{}` is not fixed by this call)",
                        g.name
                    ),
                    e.span,
                ));
            };
            out.push((g.name.clone(), b));
        }
        Ok(out)
    }

    /// [`bind_generics`] for a method call (s94): the receiver's
    /// site type binds the IMPL's rigids (its declared type is the
    /// impl self type — `Pair[K, V]` — so a `Pair[int, str]` receiver
    /// binds both), the argument types bind the method's own, and the
    /// call's recorded type binds through the return. Every rigid of
    /// both scopes must bind, or the site refuses naming the one that
    /// did not.
    /// s95: a qualified `Trait.method(args)` — no receiver expression,
    /// the `Self`-pinning ARGUMENT names the impl. Same routing as the
    /// method-call form; the arguments marshal as ordinary values (a
    /// `mut` parameter through this surface refuses by name until a
    /// witness needs it).
    /// s96: a dyn-method call — the dispatch record says `dyn_call`,
    /// so there is no static callee to name. The receiver is the
    /// two-word pair (`[abi.native.dyn]`); dispatch loads the slot the
    /// trait's `DynReport` names (sema's canonical order — the
    /// interface serializes that list, so a slot index is a
    /// cross-module fact) and calls indirect with the ERASED
    /// signature: the receiver crosses as the DATA pointer, every
    /// other parameter and the return exactly as declared — dyn-safety
    /// pinned them `Self`-free at resolve (E0508/09/10 are the
    /// witnesses), and the guards below refuse by name if that ever
    /// slips. s97's `call.ind` is the whole call path; s96 builds no
    /// call machinery of its own.
    fn lower_dyn_trait_call(
        &mut self,
        recv: &'t GreenNode,
        args: Vec<Arg<'t>>,
        tmodule: usize,
        tname: &str,
        method: &str,
        e: &'t GreenNode,
    ) -> R<Flow> {
        let tr = wolf_sema::traits::TraitRef {
            module: tmodule,
            name: tname.to_string(),
        };
        let Some(td) = self.sigs.traits.get(&tr) else {
            return Err(refuse("a dyn call on an unelaborated trait", e.span));
        };
        // Belt and suspenders: sema refuses dyn-unsafe traits at the
        // `dyn` spelling; a call reaching here without a safe report
        // would dispatch into a table sema never shaped.
        if !td.dyn_report.safe() {
            return Err(refuse_named(
                format!("a dyn call on `{tname}`, which is not dyn-safe (object safety owns this)"),
                e.span,
            ));
        }
        let Some(slot) = td.dyn_report.methods.iter().position(|m| m == method) else {
            return Err(refuse_named(
                format!(
                    "a dyn call to `{tname}.{method}`, which is not in the dyn-safe method set"
                ),
                e.span,
            ));
        };
        let Some(tm) = td.method(method).filter(|mm| mm.has_self) else {
            return Err(refuse("a dyn call without a receiver method", e.span));
        };
        let msig = &tm.sig;
        // The erased convention passes the DATA pointer as the
        // receiver, by value. A `mut`/`take` receiver would promise
        // writeback or consumption through erasure — no witness needs
        // either; refuse by name rather than guess a convention.
        if let Some(mode) = msig.params.first().and_then(|p| p.mode) {
            let kw = match mode {
                ParamMode::Mut => "mut",
                ParamMode::Take => "take",
            };
            return Err(refuse_named(
                format!("a `{kw} self` method through `dyn` (the erased receiver is a pointer)"),
                e.span,
            ));
        }
        for p in msig.params.iter().skip(1) {
            if p.mode.is_some() {
                return Err(refuse_named(
                    format!("a moded parameter through a dyn `{tname}.{method}` call"),
                    e.span,
                ));
            }
            if mentions_self(self.sig_table, p.ty) {
                return Err(refuse_named(
                    "a `Self`-typed parameter through `dyn` (object safety owns this)".to_string(),
                    e.span,
                ));
            }
        }
        if mentions_self(self.sig_table, msig.ret) {
            return Err(refuse_named(
                "a `Self`-typed return through `dyn` (object safety owns this)".to_string(),
                e.span,
            ));
        }
        // Receiver first, then ordinary arguments.
        let Some(pair) = flow_val!(self.lower_expr(recv)) else {
            return Err(refuse("unit-typed dyn receivers", recv.span));
        };
        let mut vals = Vec::new();
        for a in &args {
            let Some(vexpr) = Arg::value(*a) else {
                continue;
            };
            let Some(v) = flow_val!(self.lower_expr(vexpr)) else {
                return Err(refuse("unit-typed arguments", vexpr.span));
            };
            vals.push(v);
        }
        // The dispatch chain: pair → (data, vtable) → slot → call.ind.
        let data = self
            .b
            .ins(Opcode::AggGet, &[pair], &[types::PTR], Aux::Int(0))
            .one();
        let vt = self
            .b
            .ins(Opcode::AggGet, &[pair], &[types::PTR], Aux::Int(1))
            .one();
        let slot_addr = self.field_addr(vt, (slot as u64) * 8);
        // A vtable is immutable static data; its loads thread the
        // header foreign token (never stored through in any body this
        // lowering emits, so no forwarding hazard).
        let r = self.foreign_hdr_region();
        let fp = self.b.ins_load(types::PTR, slot_addr, r);
        // The erased signature, from the trait's own declaration: PTR
        // receiver + the declared tail, all by value, token-free.
        let table = self.sig_table;
        let mut params = vec![Param {
            ty: types::PTR,
            mode: Mode::Val,
        }];
        for p in msig.params.iter().skip(1) {
            let Some(w) = wir_ty(&mut self.b.module.types, table, self.sigs, p.ty, e.span)? else {
                return Err(refuse("unit-typed parameters", e.span));
            };
            params.push(Param {
                ty: w,
                mode: Mode::Val,
            });
        }
        let results = match wir_ty(&mut self.b.module.types, table, self.sigs, msig.ret, e.span)? {
            Some(t) => vec![t],
            None => vec![],
        };
        let sig = self.b.module.make_sig(params, results);
        let mut cargs = vec![data];
        cargs.extend(vals);
        Ok(Flow::Val(self.b.ins_call_ind(fp, sig, &cargs)))
    }

    fn lower_qualified_trait_call(
        &mut self,
        d: CallExpr<'t>,
        cs: &CallSig,
        tmodule: usize,
        tname: &'t str,
        method: &'t str,
        e: &'t GreenNode,
    ) -> R<Flow> {
        let tr = wolf_sema::traits::TraitRef {
            module: tmodule,
            name: tname.to_string(),
        };
        let Some(td) = self.sigs.traits.get(&tr) else {
            return Err(refuse("a qualified call on an unelaborated trait", e.span));
        };
        let Some(tm) = td.method(method) else {
            return Err(refuse("a qualified call on an undeclared method", e.span));
        };
        let args: Vec<_> = d.args().into_iter().flat_map(|l| l.args()).collect();
        // The Self-pinning argument: the first whose DECLARED type
        // mentions `Self` (sema blamed the same argument, D28).
        let mut head: Option<String> = None;
        for (i, p) in tm.sig.params.iter().enumerate() {
            if !mentions_self(self.sig_table, p.ty) {
                continue;
            }
            if let Some(a) = args.get(i)
                && let Some(vexpr) = Arg::value(*a)
                && let Some(site) = self.expr_sema_ty(vexpr.span)
                && let Some(name) = self_ty_key(self.table, self.strip_sema(site))
            {
                head = Some(name);
            }
            break;
        }
        let Some(head) = head else {
            return Err(refuse_named(
                format!("a `{tname}.{method}` call whose `Self` argument is not nominal"),
                e.span,
            ));
        };
        let imp = self
            .sigs
            .impls
            .iter()
            .find(|i| {
                i.trait_ref.as_ref() == Some(&tr)
                    && self_ty_key(self.sig_table, i.self_ty).as_deref() == Some(head.as_str())
            })
            .ok_or_else(|| {
                refuse_named(
                    format!("a `{tname}` call without a coherent impl for `{head}`"),
                    e.span,
                )
            })?;
        let (base_name, msig): (String, &FnSig) =
            match imp.methods.iter().find(|mm| mm.name == *method) {
                Some(m) => (format!("{head}.{tname}.{method}"), &m.sig),
                None => (format!("{tname}.{method}"), &tm.sig),
            };
        let overridden = !std::ptr::eq(msig, &tm.sig as *const _);
        // Bindings: the impl route needs its rigids (and the method's);
        // the default route needs `Self` (and the method's).
        let required: Vec<String> = if overridden {
            imp.generics
                .iter()
                .chain(msig.generics.iter())
                .map(|g| g.name.clone())
                .collect()
        } else {
            std::iter::once("Self".to_string())
                .chain(msig.generics.iter().map(|g| g.name.clone()))
                .collect()
        };
        let mut map: std::collections::BTreeMap<String, Bound> = std::collections::BTreeMap::new();
        for (i, p) in msig.params.iter().enumerate() {
            if let Some(a) = args.get(i)
                && let Some(vexpr) = Arg::value(*a)
                && let Some(site) = self.expr_sema_ty(vexpr.span)
            {
                self.match_binding(p.ty, site, &mut map, e.span)?;
            }
        }
        if let Some(site_ret) = self.expr_sema_ty(e.span) {
            self.match_binding(msig.ret, site_ret, &mut map, e.span)?;
        }
        let mut bindings: Vec<(String, Bound)> = Vec::with_capacity(required.len());
        for name in &required {
            let Some(b) = map.remove(name.as_str()) else {
                return Err(refuse_named(
                    format!(
                        "an instantiation with an unbound parameter (`{name}` is not fixed by this call)"
                    ),
                    e.span,
                ));
            };
            bindings.push((name.clone(), b));
        }
        if msig.comptime {
            return Err(refuse(
                "comptime method calls (D29 CTFE owns these)",
                e.span,
            ));
        }
        // Arguments: ordinary values, declared modes read/take only.
        for p in &msig.params {
            if matches!(p.mode, Some(ParamMode::Mut)) {
                return Err(refuse_named(
                    format!("a `mut` parameter through a qualified `{tname}.{method}` call"),
                    e.span,
                ));
            }
        }
        let _ = cs;
        let mut vals = Vec::new();
        for a in &args {
            let Some(vexpr) = Arg::value(*a) else {
                continue;
            };
            let Some(v) = flow_val!(self.lower_expr(vexpr)) else {
                return Err(refuse("unit-typed arguments", vexpr.span));
            };
            vals.push(v);
        }
        let callee_name = if bindings.is_empty() {
            base_name
        } else {
            let key = SpecKey {
                mask: 0,
                subst: bindings
                    .iter()
                    .map(|(n, b)| {
                        let mut scratch = TypeTable::new();
                        let t = thaw(b, &mut scratch);
                        (n.clone(), mono_spelling(&scratch, t))
                    })
                    .collect(),
            };
            let full = spec_name(&base_name, &key);
            self.pending_specs.push(SpecRequest {
                name: base_name,
                key,
                bindings: bindings.clone(),
                span: e.span,
            });
            full
        };
        let ext = match self.callees.get(&callee_name) {
            Some(&ext) => ext,
            None => {
                let sig = if bindings.is_empty() {
                    wir_sig_of(self.b.module, self.sig_table, self.sigs, msig, 0, e.span)?
                } else {
                    let mut st = self.sig_table.clone();
                    let map: std::collections::BTreeMap<String, TyId> = bindings
                        .iter()
                        .map(|(n, b)| (n.clone(), thaw(b, &mut st)))
                        .collect();
                    let mut f = msig.clone();
                    for p in &mut f.params {
                        p.ty = wolf_sema::types::subst(&mut st, p.ty, &map);
                    }
                    f.ret = wolf_sema::types::subst(&mut st, f.ret, &map);
                    wir_sig_of(self.b.module, &st, self.sigs, &f, 0, e.span)?
                };
                let ext = self.b.func.import_func(callee_name.clone(), sig);
                self.callees.insert(callee_name, ext);
                ext
            }
        };
        let results = self.b.ins_call_regions(ext, &vals, &HashMap::new());
        Ok(Flow::Val(results.first().copied()))
    }

    /// s95: resolve a STATIC trait-method call — the record names the
    /// trait, the receiver's head names the impl (coherence made it
    /// unique), and the route yields the callee's base name, signature
    /// and bindings. An overridden method is the impl's, mangled
    /// `Type.Trait.method`; a defaulted one is the trait's own body as
    /// an instance, `Trait.method` + a `Self` binding in the key.
    fn route_trait_static(
        &self,
        tmodule: usize,
        tname: &'t str,
        method: &'t str,
        head: &str,
        d: CallExpr<'t>,
        e: &'t GreenNode,
    ) -> R<TraitRoute<'t>> {
        let tr = wolf_sema::traits::TraitRef {
            module: tmodule,
            name: tname.to_string(),
        };
        let imp = self
            .sigs
            .impls
            .iter()
            .find(|i| {
                i.trait_ref.as_ref() == Some(&tr)
                    && matches!(
                        self.sig_table.kind(i.self_ty),
                        TyKind::Nominal { name, .. } if name == head
                    )
            })
            .ok_or_else(|| {
                refuse_named(
                    format!("a `{tname}` call without a coherent impl for `{head}`"),
                    e.span,
                )
            })?;
        if let Some(m) = imp.methods.iter().find(|mm| mm.name == *method) {
            let bindings = if imp.generics.is_empty() && m.sig.generics.is_empty() {
                Vec::new()
            } else {
                self.bind_method_generics(imp, &m.sig, d, e)?
            };
            return Ok((format!("{head}.{tname}.{method}"), &m.sig, bindings));
        }
        // Defaulted: the trait's body, `Self ↦ subject`. The body
        // mentions `Self` (and the method's own rigids) only — the
        // impl's rigids never appear in it, so they are not bound.
        let tm = self
            .sigs
            .traits
            .get(&tr)
            .and_then(|td| td.method(method))
            .ok_or_else(|| {
                refuse_named(
                    format!("a `{tname}.{method}` call without a declared method"),
                    e.span,
                )
            })?;
        let bindings = self.bind_trait_default(&tm.sig, d, e)?;
        Ok((format!("{tname}.{method}"), &tm.sig, bindings))
    }

    /// The default-body binder: `Self` from the receiver (or the
    /// Self-mentioning argument), the method's own rigids from the
    /// argument types — [`Self::bind_method_generics`]'s shape with
    /// `Self` in the required set.
    fn bind_trait_default(
        &self,
        msig: &FnSig,
        d: CallExpr<'t>,
        e: &'t GreenNode,
    ) -> R<Vec<(String, Bound)>> {
        let mut map: std::collections::BTreeMap<String, Bound> = std::collections::BTreeMap::new();
        if let Some(base) = d
            .callee()
            .and_then(wolf_ast::MemberExpr::cast)
            .and_then(|m| m.base())
            && let Some(p0) = msig.params.first()
            && let Some(site) = self.expr_sema_ty(base.span)
        {
            self.match_binding(p0.ty, site, &mut map, e.span)?;
        }
        let args: Vec<_> = d.args().into_iter().flat_map(|l| l.args()).collect();
        for (i, p) in msig.params.iter().skip(1).enumerate() {
            let Some(a) = args.get(i) else { break };
            let Some(vexpr) = Arg::value(*a) else {
                continue;
            };
            let Some(site) = self.expr_sema_ty(vexpr.span) else {
                continue;
            };
            self.match_binding(p.ty, site, &mut map, e.span)?;
        }
        if let Some(site_ret) = self.expr_sema_ty(e.span) {
            self.match_binding(msig.ret, site_ret, &mut map, e.span)?;
        }
        let mut out = Vec::with_capacity(1 + msig.generics.len());
        for name in std::iter::once("Self").chain(msig.generics.iter().map(|g| g.name.as_str())) {
            let Some(b) = map.remove(name) else {
                return Err(refuse_named(
                    format!(
                        "an instantiation with an unbound parameter (`{name}` is not fixed by this call)"
                    ),
                    e.span,
                ));
            };
            out.push((name.to_string(), b));
        }
        Ok(out)
    }

    fn bind_method_generics(
        &self,
        imp: &wolf_sema::traits::ImplDef,
        msig: &FnSig,
        d: CallExpr<'t>,
        e: &'t GreenNode,
    ) -> R<Vec<(String, Bound)>> {
        let mut map: std::collections::BTreeMap<String, Bound> = std::collections::BTreeMap::new();
        // Receiver: the callee is `base.method`, and params[0] is the
        // declared receiver.
        if let Some(base) = d
            .callee()
            .and_then(wolf_ast::MemberExpr::cast)
            .and_then(|m| m.base())
            && let Some(p0) = msig.params.first()
            && let Some(site) = self.expr_sema_ty(base.span)
        {
            self.match_binding(p0.ty, site, &mut map, e.span)?;
        }
        let args: Vec<_> = d.args().into_iter().flat_map(|l| l.args()).collect();
        for (i, p) in msig.params.iter().skip(1).enumerate() {
            let Some(a) = args.get(i) else { break };
            let Some(vexpr) = Arg::value(*a) else {
                continue;
            };
            let Some(site) = self.expr_sema_ty(vexpr.span) else {
                continue;
            };
            self.match_binding(p.ty, site, &mut map, e.span)?;
        }
        if let Some(site_ret) = self.expr_sema_ty(e.span) {
            self.match_binding(msig.ret, site_ret, &mut map, e.span)?;
        }
        let mut out = Vec::with_capacity(imp.generics.len() + msig.generics.len());
        for g in imp.generics.iter().chain(msig.generics.iter()) {
            let Some(b) = map.remove(&g.name) else {
                return Err(refuse_named(
                    format!(
                        "an instantiation with an unbound parameter (`{}` is not fixed by this call)",
                        g.name
                    ),
                    e.span,
                ));
            };
            out.push((g.name.clone(), b));
        }
        Ok(out)
    }

    /// One structural step of [`bind_generics`]: `decl` in the
    /// signature table against `site` in this body's table.
    fn match_binding(
        &self,
        decl: TyId,
        site: TyId,
        map: &mut std::collections::BTreeMap<String, Bound>,
        span: Span,
    ) -> R<()> {
        let dk = self.sig_table.kind(decl).clone();
        if let TyKind::Rigid(name) = dk {
            let b = freeze(self.table, site);
            match map.get(&name) {
                None => {
                    map.insert(name, b);
                }
                Some(prev) if *prev == b => {}
                Some(_) => {
                    return Err(refuse_named(
                        format!("an instantiation binding `{name}` two ways at one call"),
                        span,
                    ));
                }
            }
            return Ok(());
        }
        let sk = self.table.kind(site).clone();
        match (dk, sk) {
            (TyKind::Wrapping(a), TyKind::Wrapping(b))
            | (TyKind::Range(a), TyKind::Range(b))
            | (TyKind::List(a), TyKind::List(b))
            | (TyKind::Pool(a), TyKind::Pool(b))
            | (TyKind::Ptr(a), TyKind::Ptr(b)) => self.match_binding(a, b, map, span),
            // s94: an applied nominal matches argument-wise — a
            // `Pair[K, V]` receiver declaration against a
            // `Pair[int, str]` site binds both rigids.
            (
                TyKind::Nominal {
                    module: dm,
                    name: dn,
                    args: da,
                },
                TyKind::Nominal {
                    module: sm,
                    name: sn,
                    args: sa,
                },
            ) if dm == sm && dn == sn && da.len() == sa.len() => {
                for (a, b) in da.into_iter().zip(sa) {
                    self.match_binding(a, b, map, span)?;
                }
                Ok(())
            }
            (TyKind::Tuple(xs), TyKind::Tuple(ys)) if xs.len() == ys.len() => {
                for (a, b) in xs.into_iter().zip(ys) {
                    self.match_binding(a, b, map, span)?;
                }
                Ok(())
            }
            (TyKind::Fn(xs, xr), TyKind::Fn(ys, yr)) if xs.len() == ys.len() => {
                for (a, b) in xs.into_iter().zip(ys) {
                    self.match_binding(a, b, map, span)?;
                }
                self.match_binding(xr, yr, map, span)
            }
            (TyKind::ErrUnion(a, ar), TyKind::ErrUnion(b, br)) => {
                self.match_binding(a, b, map, span)?;
                self.match_binding(ar, br, map, span)
            }
            // A declared row with a rigid tail against a site row: the
            // tail binds to whatever the site row has beyond the
            // declared tags — `subst`'s row-merge arm is the inverse.
            (
                TyKind::Row {
                    tags: dtags,
                    tail: Some(dtail),
                },
                TyKind::Row {
                    tags: stags,
                    tail: stail,
                },
            ) if matches!(self.sig_table.kind(dtail), TyKind::Rigid(_)) => {
                for (dn, dps) in &dtags {
                    if let Some((_, sps)) = stags.iter().find(|(n, _)| n == dn) {
                        for (a, b) in dps.iter().zip(sps) {
                            self.match_binding(*a, *b, map, span)?;
                        }
                    }
                }
                let rest: Vec<(String, Vec<TyId>)> = stags
                    .into_iter()
                    .filter(|(n, _)| !dtags.iter().any(|(dn, _)| dn == n))
                    .collect();
                // The tail's binding is the site's residual row (its own
                // tail carried), frozen from this body's table.
                let residual = Bound::Row {
                    tags: rest
                        .into_iter()
                        .map(|(n, ps)| (n, ps.into_iter().map(|t| freeze(self.table, t)).collect()))
                        .collect(),
                    tail: stail.map(|t| Box::new(freeze(self.table, t))),
                };
                let TyKind::Rigid(name) = self.sig_table.kind(dtail).clone() else {
                    unreachable!("guarded")
                };
                match map.get(&name) {
                    None => {
                        map.insert(name, residual);
                    }
                    Some(prev) if *prev == residual => {}
                    Some(_) => {
                        return Err(refuse_named(
                            format!("an instantiation binding row `{name}` two ways at one call"),
                            span,
                        ));
                    }
                }
                Ok(())
            }
            // Ground on both sides, or shapes the site cannot inform:
            // nothing to bind here.
            _ => Ok(()),
        }
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

    // ------------------------------- s40: native str/List/fs tier ----
    //
    // The runtime shapes live in `wolf_rt::{str, list, fs}`; the
    // region-backed strbuf materialization DESIGN NOTE is that str
    // module's crate docs — a materialized str is an allocation like
    // any other, landing in the ambient region per
    // [mem.region.create.3] (realized as the process root region at
    // this tier). Semantics per shim are the checked executor's,
    // function for function; misses come back as codes and LOWERING
    // picks the spelling — `{none}` row (`get`, `find`, `pop`) or
    // `trap(bounds)` (`s[a..b]`, `l[i]`) — so verdict identity is
    // decided here, where the language rules live, never in the
    // runtime.

    /// The sema type behind adapter wrappers (`distinct`, `wrapping`).
    /// s105: a region ELEMENT in a container is the named stop — the
    /// handle would leave extent tracking's sight (a `List[region]`
    /// cell has no binding the mem tier's region law can follow).
    /// Channels are the one deliberate exception ([conc.chan.move]).
    fn refuse_region_elem(&self, elem: TyId, span: Span) -> R<()> {
        if matches!(self.table.kind(self.strip_sema(elem)), TyKind::RegionTy) {
            return Err(refuse(
                "a region element in a container (extent tracking stops at the handle — c25 closeout)",
                span,
            ));
        }
        Ok(())
    }

    fn strip_sema(&self, mut ty: TyId) -> TyId {
        for _ in 0..32 {
            match self.table.kind(ty) {
                TyKind::Distinct(i) | TyKind::Wrapping(i) => ty = *i,
                _ => break,
            }
        }
        ty
    }

    /// Import (once) a runtime shim under an explicit WIR signature.
    /// One shape per symbol, by construction of the call sites.
    fn rt_import(
        &mut self,
        name: &'static str,
        params: Vec<Param>,
        results: Vec<TypeId>,
    ) -> ExtFunc {
        match self.callees.get(name) {
            Some(&ext) => ext,
            None => {
                let sig = self.b.module.make_sig(params, results);
                let ext = self.b.func.import_func(name, sig);
                self.callees.insert(name.to_string(), ext);
                ext
            }
        }
    }

    /// Call a slotless runtime shim; `result` is its scalar result
    /// type, if any.
    fn rt_call(
        &mut self,
        name: &'static str,
        args: &[Value],
        result: Option<TypeId>,
    ) -> Option<Value> {
        let params: Vec<Param> = args
            .iter()
            .map(|&a| Param::val(self.b.func.value_ty(a)))
            .collect();
        let ext = self.rt_import(name, params, result.into_iter().collect());
        self.b.ins_call(ext, args).first().copied()
    }

    /// A fresh `size`-byte stack slot for runtime out/elem traffic,
    /// with its provenance facts (the s19 pattern the `mut`-arg spill
    /// established).
    fn rt_slot(&mut self, size: u64) -> (RegionId, Value) {
        let (region, slot) = self.b.ins_stack_alloc(size);
        self.b
            .func
            .add_fact(FactData::new(FactKind::Region(slot, region), Just::DefOp));
        self.b.func.add_fact(FactData::new(
            FactKind::Deref(slot, DerefSize::Const(size)),
            Just::DefOp,
        ));
        (region, slot)
    }

    /// Call a runtime shim whose LAST machine parameter is a slot
    /// pointer the runtime reads or writes: the shim's WIR signature
    /// carries a `mem.r0` token bound to the slot's region, so the
    /// call orders against the caller's stores and reloads through the
    /// ordinary token chain. `result` as in [`Self::rt_call`].
    fn rt_call_slot(
        &mut self,
        name: &'static str,
        args: &[Value],
        slot: Value,
        slot_region: RegionId,
        result: Option<TypeId>,
    ) -> Option<Value> {
        let mut params: Vec<Param> = args
            .iter()
            .map(|&a| Param::val(self.b.func.value_ty(a)))
            .collect();
        params.push(Param::val(types::PTR));
        let formal = RegionId::new(0);
        let tok = self.b.module.types.mem(formal);
        params.push(Param {
            ty: tok,
            mode: Mode::Val,
        });
        let ext = self.rt_import(name, params, result.into_iter().collect());
        let mut call_args = args.to_vec();
        call_args.push(slot);
        let mut formal_regions = HashMap::new();
        formal_regions.insert(0u32, slot_region);
        self.b
            .ins_call_regions(ext, &call_args, &formal_regions)
            .first()
            .copied()
    }

    /// [`Self::rt_call_slot`] for a shim that also mutates FOREIGN
    /// storage (s75: `list_new` mints a header, `list_push` writes the
    /// header AND may reallocate the buffer). Trailing formal tokens
    /// bind both foreign regions, so no `data`/`len` the caller
    /// already loaded survives the call — at WIR by the token chain,
    /// at LLVM by the call being opaque.
    fn rt_call_foreign(
        &mut self,
        name: &'static str,
        args: &[Value],
        slot: Option<(Value, RegionId)>,
        result: Option<TypeId>,
    ) -> Option<Value> {
        let (hdrs, bufs) = self.foreign_regions();
        let mut params: Vec<Param> = args
            .iter()
            .map(|&a| Param::val(self.b.func.value_ty(a)))
            .collect();
        let mut call_args = args.to_vec();
        let mut formal_regions = HashMap::new();
        let mut next_formal = 0u32;
        if let Some((slot_ptr, slot_region)) = slot {
            params.push(Param::val(types::PTR));
            call_args.push(slot_ptr);
            let tok = self.b.module.types.mem(RegionId::new(next_formal));
            params.push(Param {
                ty: tok,
                mode: Mode::Val,
            });
            formal_regions.insert(next_formal, slot_region);
            next_formal += 1;
        }
        for actual in [hdrs, bufs] {
            let tok = self.b.module.types.mem(RegionId::new(next_formal));
            params.push(Param {
                ty: tok,
                mode: Mode::Val,
            });
            formal_regions.insert(next_formal, actual);
            next_formal += 1;
        }
        let ext = self.rt_import(name, params, result.into_iter().collect());
        self.b
            .ins_call_regions(ext, &call_args, &formal_regions)
            .first()
            .copied()
    }

    /// The `{ptr, len}` halves of a str value.
    fn str_parts(&mut self, v: Value) -> (Value, Value) {
        let p = self
            .b
            .ins(Opcode::AggGet, &[v], &[types::PTR], Aux::Int(0))
            .one();
        let l = self
            .b
            .ins(Opcode::AggGet, &[v], &[types::I64], Aux::Int(1))
            .one();
        (p, l)
    }

    /// Reload the `{ptr, len}` pair a shim wrote through `slot`.
    fn load_str_slot(&mut self, slot: Value, region: RegionId, span: Span) -> R<Value> {
        let sty = str_ty(self.b.types());
        self.load_flat(sty, slot, region, span)
    }

    /// `i64 != 0` as a WIR bool.
    fn nonzero(&mut self, v: Value) -> Value {
        let z = self.b.iconst(types::I64, 0);
        self.b
            .ins(Opcode::Icmp, &[v, z], &[types::BOOL], Aux::IntCc(IntCc::Ne))
            .one()
    }

    /// Branch to a `kind` trap unless `hit` holds; continue otherwise.
    /// Returns `true` when the trap is PROVEN (the caller's path
    /// diverged); a proven pass emits nothing.
    fn trap_unless(&mut self, hit: Value, kind: TrapKind) -> bool {
        match self.b.as_bool_const(hit) {
            Some(true) => {
                self.b.stats.fold += 1;
                false
            }
            Some(false) => {
                self.b.stats.fold += 1;
                self.b.ins_trap(kind);
                true
            }
            None => {
                let cont = self.b.create_block();
                let trap_bb = self.b.create_block();
                self.b.ins_br(hit, cont, &[], trap_bb, &[]);
                self.b.seal_block(trap_bb);
                self.b.switch_to_block(trap_bb);
                self.b.ins_trap(kind);
                self.b.seal_block(cont);
                self.b.switch_to_block(cont);
                false
            }
        }
    }

    /// The eu WIR type recorded for expression `span` (the `!T` shape
    /// of a fallible builtin's result).
    fn eu_ty_of(&mut self, span: Span) -> R<TypeId> {
        let Some(sema) = self.expr_sema_ty(span) else {
            return Err(refuse("a fallible builtin without a recorded type", span));
        };
        match wir_ty(&mut self.b.module.types, self.table, self.sigs, sema, span)? {
            Some(t) if matches!(self.b.module.types.get(t), types::TypeData::Eu { .. }) => Ok(t),
            _ => Err(refuse("a fallible builtin without a union shape", span)),
        }
    }

    /// Declared tag names of the row on the `!T` type at `span`.
    fn row_tag_names(&self, span: Span) -> Vec<String> {
        let Some(sema) = self.expr_sema_ty(span) else {
            return Vec::new();
        };
        let TyKind::ErrUnion(_, row) = self.table.kind(self.strip_sema(sema)) else {
            return Vec::new();
        };
        let TyKind::Row { tags, .. } = self.table.kind(*row) else {
            return Vec::new();
        };
        tags.iter().map(|(n, _)| n.clone()).collect()
    }

    /// Join a hit flag into an eu value: `hit` takes the ok half from
    /// `mk_ok`, the miss path takes the payload-free tag + payloads
    /// from `mk_err`. Both closures run in their own blocks.
    fn eu_join(
        &mut self,
        eu_ty: TypeId,
        hit: Value,
        mk_ok: impl FnOnce(&mut Self) -> R<Option<Value>>,
        mk_err: impl FnOnce(&mut Self) -> R<Value>,
    ) -> R<Value> {
        // A DECIDED test needs no join. Building the losing arm anyway
        // would fill a block nothing branches to, and Braun would then
        // be asked for a definition on a path that does not exist —
        // the same latent shape `trap_unless` already folds. It stayed
        // latent while every `hit` came out of a runtime call; s75's
        // in-place `clear` + `pop` makes store→load forwarding decide
        // one, so it stops being latent here.
        match self.b.as_bool_const(hit) {
            Some(true) => {
                self.b.stats.fold += 1;
                let okv = mk_ok(self)?;
                return Ok(self.b.ins_eu_make_ok(eu_ty, okv));
            }
            Some(false) => {
                self.b.stats.fold += 1;
                let tag = mk_err(self)?;
                return Ok(self.b.ins_eu_make_err(eu_ty, tag, &[]));
            }
            None => {}
        }
        let hit_bb = self.b.create_block();
        let miss_bb = self.b.create_block();
        let merge = self.b.create_block();
        let out = self.b.add_block_param(merge, eu_ty);
        self.b.ins_br(hit, hit_bb, &[], miss_bb, &[]);
        self.b.seal_block(hit_bb);
        self.b.seal_block(miss_bb);
        self.b.switch_to_block(hit_bb);
        self.b.gvn_push_scope();
        let okv = mk_ok(self)?;
        let ev = self.b.ins_eu_make_ok(eu_ty, okv);
        self.b.ins_jmp(merge, &[ev]);
        self.b.gvn_pop_scope();
        self.b.switch_to_block(miss_bb);
        self.b.gvn_push_scope();
        let tag = mk_err(self)?;
        let ev = self.b.ins_eu_make_err(eu_ty, tag, &[]);
        self.b.ins_jmp(merge, &[ev]);
        self.b.gvn_pop_scope();
        self.b.seal_block(merge);
        self.b.switch_to_block(merge);
        Ok(out)
    }

    /// The payload-free `{none}` miss tag.
    fn none_tag(&mut self) -> Value {
        let id = self.b.module.tag_id("none");
        self.b.iconst(types::I64, id)
    }

    /// Map a runtime fs error code (nonzero, in `code`) to the
    /// module-interned tag id — a compile-time-dispatched branch chain
    /// mirroring the checked executor's `errtag`: `not_found`/`denied`
    /// coarsen to `io` when the row does not declare them; `utf8`/`eof`
    /// pass through.
    ///
    /// s90 adds three codes. `exists` (7) and `cross_device` (8) come
    /// out of `io::ErrorKind` like `not_found`/`denied`, so they take
    /// the SAME coarsening — that is what keeps the eu ABI's tag
    /// coarsening checked-parity as #40 established it. `invalid` (6)
    /// does not coarsen: it is never an `ErrorKind`, it is a caller
    /// mistake the runtime decides itself (a mode outside the set, a
    /// `List[int]` element that is not a byte), and only the builtins
    /// that declare it can produce it.
    fn fs_code_tag(&mut self, code: Value, declared: &[String]) -> Value {
        let io_id = self.b.module.tag_id("io");
        let merge = self.b.create_block();
        let out = self.b.add_block_param(merge, types::I64);
        let cases: Vec<(i64, i64)> = [
            (1, "not_found", true),
            (2, "denied", true),
            (4, "utf8", false),
            (5, "eof", false),
            (6, "invalid", false),
            (7, "exists", true),
            (8, "cross_device", true),
        ]
        .into_iter()
        .map(|(c, name, coarsen)| {
            let id = if !coarsen || declared.iter().any(|t| t == name) {
                self.b.module.tag_id(name)
            } else {
                io_id
            };
            (c, id)
        })
        .collect();
        for (c, id) in cases {
            let k = self.b.iconst(types::I64, c);
            let eq = self
                .b
                .ins(
                    Opcode::Icmp,
                    &[code, k],
                    &[types::BOOL],
                    Aux::IntCc(IntCc::Eq),
                )
                .one();
            let tag = self.b.iconst(types::I64, id);
            let next = self.b.create_block();
            self.b.ins_br(eq, merge, &[tag], next, &[]);
            self.b.seal_block(next);
            self.b.switch_to_block(next);
        }
        let io_tag = self.b.iconst(types::I64, io_id);
        self.b.ins_jmp(merge, &[io_tag]);
        self.b.seal_block(merge);
        self.b.switch_to_block(merge);
        out
    }

    /// Resolve a range node's endpoints against `len` — open sides
    /// default to the edges, `^n` counts from the end, `..=` bumps the
    /// upper bound — the SAME resolution as the checked executor's
    /// `eval_str_slice`, before any domain question is asked.
    fn range_endpoints(&mut self, rn: &'t GreenNode, len: Value) -> R<(Value, Value)> {
        let d = RangeExpr::cast(rn).ok_or_else(|| refuse("this index shape", rn.span))?;
        let dots = rn
            .tokens()
            .find(|t| matches!(t.kind, SyntaxKind::DotDot | SyntaxKind::DotDotEq))
            .map(|t| t.span.lo)
            .unwrap_or(rn.span.hi);
        let mut lo = self.b.iconst(types::I64, 0);
        let mut hi = len;
        for ep in d.endpoints() {
            let resolved = if ep.kind == SyntaxKind::FromEndExpr {
                let inner = FromEndExpr::cast(ep).and_then(|f| f.expr());
                let Some(inner) = inner else {
                    return Err(refuse("a bare `^` endpoint", ep.span));
                };
                let n = match self.lower_expr(inner)? {
                    Flow::Val(Some(v)) => v,
                    _ => return Err(refuse("a valueless `^n` endpoint", ep.span)),
                };
                // `len - n` cannot overflow: both are in-range i64s of
                // a real string; wrap semantics match the checked
                // subtraction.
                self.b
                    .ins(Opcode::IsubWrap, &[len, n], &[types::I64], Aux::None)
                    .one()
            } else {
                match self.lower_expr(ep)? {
                    Flow::Val(Some(v)) => v,
                    _ => return Err(refuse("a valueless slice endpoint", ep.span)),
                }
            };
            if ep.span.lo < dots {
                lo = resolved;
            } else {
                hi = resolved;
            }
        }
        if d.is_inclusive() {
            let one = self.b.iconst(types::I64, 1);
            hi = self
                .b
                .ins(Opcode::IaddWrap, &[hi, one], &[types::I64], Aux::None)
                .one();
        }
        Ok((lo, hi))
    }

    /// A string episode in VALUE position: a literal-only string
    /// becomes a `{ptr, len}` pair over module data (s31); an
    /// interpolated string MATERIALIZES through the region-backed
    /// strbuf (s40 — the c08-owed design, see `wolf_rt::str`): one
    /// `strbuf_new`, per-segment appends through the same packed-spec
    /// renderers the print path uses, one `strbuf_finish` into the
    /// ambient region.
    fn lower_string(&mut self, e: &'t GreenNode) -> R<Flow> {
        // Dedent shifts every hole offset — refuse the combination,
        // exactly as `lower_print` and the checked executor do.
        if let Some(sd) = StringExpr::cast(e)
            && self.text(e.span).starts_with("\"\"\"")
            && sd.interps().any(|i| i.expr().is_some())
        {
            return Err(refuse(
                "interpolation inside a multiline string (s38 formatting)",
                e.span,
            ));
        }
        let segs = self.string_segments(e);
        if !segs.iter().any(|s| matches!(s, StrSeg::Hole { .. })) {
            let mut bytes: Vec<u8> = Vec::new();
            for seg in segs {
                if let StrSeg::Lit(b) = seg {
                    bytes.extend_from_slice(&b);
                }
            }
            return Ok(Flow::Val(Some(self.str_value(&bytes))));
        }
        let buf = self
            .rt_call("__wolf_rt_strbuf_new", &[], Some(types::PTR))
            .expect("strbuf handle");
        for seg in segs {
            match seg {
                StrSeg::Lit(bytes) => {
                    if bytes.is_empty() {
                        continue;
                    }
                    let idx = self.b.module.intern_data(&bytes);
                    let p = self.b.ins_data_addr(idx);
                    let len = self.b.iconst(types::I64, bytes.len() as i64);
                    let sp = self.b.iconst(types::I64, 0);
                    self.rt_call("__wolf_rt_strbuf_str", &[buf, p, len, sp], None);
                }
                StrSeg::Hole { expr, spec } => {
                    let packed = self.packed_spec(spec)?;
                    let Some(v) = flow_val!(self.lower_expr(expr)) else {
                        return Err(refuse("unit-typed interpolation holes", expr.span));
                    };
                    match self.classify_print_value(expr, v, packed)? {
                        PrintSeg::Str { v, spec } => {
                            let (p, l) = self.str_parts(v);
                            let sp = self.b.iconst(types::I64, spec);
                            self.rt_call("__wolf_rt_strbuf_str", &[buf, p, l, sp], None);
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
                            let sp = self.b.iconst(types::I64, spec);
                            self.rt_call("__wolf_rt_strbuf_i64", &[buf, wide, sp], None);
                        }
                        PrintSeg::Bool { v, spec } => {
                            let sp = self.b.iconst(types::I64, spec);
                            self.rt_call("__wolf_rt_strbuf_bool", &[buf, v, sp], None);
                        }
                        PrintSeg::F64 { v, spec } => {
                            let sp = self.b.iconst(types::I64, spec);
                            self.rt_call("__wolf_rt_strbuf_f64", &[buf, v, sp], None);
                        }
                        PrintSeg::Lit(_) => unreachable!("holes classify as values"),
                    }
                }
            }
        }
        let (region, slot) = self.rt_slot(16);
        self.rt_call_slot("__wolf_rt_strbuf_finish", &[buf], slot, region, None);
        let v = self.load_str_slot(slot, region, e.span)?;
        Ok(Flow::Val(Some(v)))
    }

    /// `[mem.str.get]`'s domain, INLINE (s77) — the test
    /// `__wolf_rt_str_get` makes, spelled in WIR so a slice costs no
    /// call. Two halves, exactly the shim's:
    ///
    /// - **range**: `lo <=u hi <=u len`. Unsigned covers the sign case
    ///   in the same compare — a negative endpoint is a very large
    ///   unsigned one — so this is the shim's `a < 0 || b < a || b >
    ///   len` in two tests instead of three.
    /// - **code-point boundary**: an endpoint is a boundary when it is
    ///   the length or does NOT address a continuation byte, i.e.
    ///   `off == len || (byte[off] & 0xC0) != 0x80`. That is
    ///   `str::is_char_boundary` (offset 0 needs no case of its own: a
    ///   valid str never opens with a continuation byte), and
    ///   `wolf_rt::str`'s `char_boundary_rule_is_the_two_bit_test`
    ///   pins the equivalence against Rust's own predicate.
    ///
    /// The `off == len` guard is load-bearing, not an optimization:
    /// without it the boundary test would read one past the end.
    ///
    /// Returns the domain bool. The pair itself is address arithmetic
    /// the caller does ([`Self::str_subslice`]) — one representation
    /// for every zero-copy result, which is what the byte view shares.
    fn str_slice_domain(&mut self, sp: Value, sl: Value, lo: Value, hi: Value) -> Value {
        // The four tests, short-circuited left to right. `merge` is
        // minted on the FIRST test that is not already decided — a
        // fully-constant slice (`"wolf"[0..9]`) folds to one bool and
        // emits no control flow at all, and a block nothing branches
        // to is exactly the unreachable-block shape a folded edge left
        // behind once before.
        let mut merge: Option<(Block, Value)> = None;
        let mut dead = false;
        // The probe constants, minted in the block that DOMINATES every
        // block below and before the scopes open: a constant minted
        // inside a branch arm is visible only there, so both probes
        // would mint their own.
        let mask = self.b.iconst(types::I64, 0xC0);
        let cont = self.b.iconst(types::I64, 0x80);
        // Nothing computed inside the region dominates the join, so
        // nothing inside it stays GVN-visible after it: the address
        // arithmetic a boundary probe does is the SAME expression
        // `str_subslice` does next, and handing that definition to the
        // join is a dominance error (it was one).
        self.b.gvn_push_scope();
        for i in 0..4 {
            let ok = match i {
                // `lo <=u hi <=u len` — unsigned folds the sign case
                // into the same compare: a negative endpoint is a very
                // large unsigned one. The shim's three tests, in two.
                0 => self.b.ins(
                    Opcode::Icmp,
                    &[lo, hi],
                    &[types::BOOL],
                    Aux::IntCc(IntCc::Ule),
                ),
                1 => self.b.ins(
                    Opcode::Icmp,
                    &[hi, sl],
                    &[types::BOOL],
                    Aux::IntCc(IntCc::Ule),
                ),
                // Boundaries LAST, and that ordering is load-bearing:
                // the probe reads `sp[off]`, which is in bounds only
                // because the range half already holds on this path.
                2 => InsOut::Vals(vec![self.str_boundary_ok(sp, sl, lo, mask, cont)]),
                _ => InsOut::Vals(vec![self.str_boundary_ok(sp, sl, hi, mask, cont)]),
            }
            .one();
            match self.b.as_bool_const(ok) {
                Some(true) => continue,
                Some(false) => {
                    dead = true;
                    break;
                }
                None => {}
            }
            let (m, _) = *merge.get_or_insert_with(|| {
                let m = self.b.create_block();
                let p = self.b.add_block_param(m, types::BOOL);
                (m, p)
            });
            let miss = self.b.bconst(false);
            let next = self.b.create_block();
            self.b.ins_br(ok, next, &[], m, &[miss]);
            self.b.seal_block(next);
            self.b.switch_to_block(next);
        }
        let decided = self.b.bconst(!dead);
        let out = match merge {
            None => decided,
            Some((m, p)) => {
                self.b.ins_jmp(m, &[decided]);
                self.b.seal_block(m);
                self.b.switch_to_block(m);
                p
            }
        };
        self.b.gvn_pop_scope();
        out
    }

    /// One endpoint's code-point-boundary test: `off == len ||
    /// (sp[off] & 0xC0) != 0x80`. The `off == len` half is what keeps
    /// the probe from reading one past the end; when it is statically
    /// decided the load needs no branch of its own.
    fn str_boundary_ok(
        &mut self,
        sp: Value,
        sl: Value,
        off: Value,
        mask: Value,
        cont: Value,
    ) -> Value {
        let at_end = self
            .b
            .ins(
                Opcode::Icmp,
                &[off, sl],
                &[types::BOOL],
                Aux::IntCc(IntCc::Eq),
            )
            .one();
        let probe = |z: &mut Self| -> Value {
            let byte = z.bytes_load_at(sp, off);
            let bits =
                z.b.ins(Opcode::Band, &[byte, mask], &[types::I64], Aux::None)
                    .one();
            z.b.ins(
                Opcode::Icmp,
                &[bits, cont],
                &[types::BOOL],
                Aux::IntCc(IntCc::Ne),
            )
            .one()
        };
        match self.b.as_bool_const(at_end) {
            // The endpoint IS the length: a boundary, no load.
            Some(true) => self.b.bconst(true),
            // Provably interior: the load needs no guard.
            Some(false) => probe(self),
            None => {
                let probe_bb = self.b.create_block();
                let join = self.b.create_block();
                let out = self.b.add_block_param(join, types::BOOL);
                let yes = self.b.bconst(true);
                self.b.ins_br(at_end, join, &[yes], probe_bb, &[]);
                self.b.seal_block(probe_bb);
                self.b.switch_to_block(probe_bb);
                self.b.gvn_push_scope();
                let ok = probe(self);
                self.b.ins_jmp(join, &[ok]);
                self.b.gvn_pop_scope();
                self.b.seal_block(join);
                self.b.switch_to_block(join);
                out
            }
        }
    }

    /// The zero-copy subslice `s[lo..hi]` as a `{ptr, len}` pair —
    /// `ptr + lo` and `hi - lo`, the same two words the shim wrote
    /// through its out slot. Valid exactly when
    /// [`Self::str_slice_domain`] held; every caller establishes that
    /// first (a trap, or a `{none}` row).
    fn str_subslice(&mut self, sp: Value, lo: Value, hi: Value) -> Value {
        let p = self.b.ins_ptr_off(sp, lo, 1);
        let n = self
            .b
            .ins(Opcode::IsubWrap, &[hi, lo], &[types::I64], Aux::None)
            .one();
        let sty = str_ty(self.b.types());
        self.b
            .ins(Opcode::AggMake, &[p, n], &[sty], Aux::None)
            .one()
    }

    /// The s37 builtin `str` method set, natively (s40): every method
    /// is one runtime call into `wolf_rt::str` (algorithms shared with
    /// the checked executor by construction), plus the miss spelling
    /// decided here — `{none}` rows for `get`/`find`/`strip_*`, a
    /// `bounds` trap for negative `repeat`.
    fn lower_str_method(
        &mut self,
        d: CallExpr<'t>,
        recv_place: &'t GreenNode,
        mname: &str,
        e: &'t GreenNode,
    ) -> R<Flow> {
        let Some(sv) = flow_val!(self.lower_expr(recv_place)) else {
            return Err(refuse("a valueless str receiver", recv_place.span));
        };
        let (sp, sl) = self.str_parts(sv);
        let arg_exprs: Vec<&'t GreenNode> = d
            .args()
            .into_iter()
            .flat_map(|l| l.args())
            .filter_map(Arg::value)
            .collect();
        let arg_val = |zelf: &mut Self, i: usize| -> R<Value> {
            let Some(x) = arg_exprs.get(i) else {
                return Err(refuse("a str method with missing arguments", e.span));
            };
            match zelf.lower_expr(x)? {
                Flow::Val(Some(v)) => Ok(v),
                _ => Err(refuse("a valueless str method argument", x.span)),
            }
        };
        let needle = |zelf: &mut Self, i: usize| -> R<(Value, Value)> {
            let v = arg_val(zelf, i)?;
            Ok(zelf.str_parts(v))
        };
        match mname {
            "is_empty" => {
                let z = self.b.iconst(types::I64, 0);
                let r = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[sl, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Eq),
                    )
                    .one();
                Ok(Flow::Val(Some(r)))
            }
            "get" => {
                let Some(rn) = arg_exprs.first() else {
                    return Err(refuse("`get` without a range", e.span));
                };
                let (lo, hi) = self.range_endpoints(rn, sl)?;
                let eu = self.eu_ty_of(e.span)?;
                // s77: the domain inline, the pair by arithmetic — the
                // recoverable spelling of the SAME test `s[a..b]` traps
                // on ([mem.str.get]'s "same domain" law, now literally
                // one helper).
                let hit = self.str_slice_domain(sp, sl, lo, hi);
                let out = self.eu_join(
                    eu,
                    hit,
                    |z| Ok(Some(z.str_subslice(sp, lo, hi))),
                    |z| Ok(z.none_tag()),
                )?;
                Ok(Flow::Val(Some(out)))
            }
            // s77: `bytes()` in a position lowering consumes on the
            // spot is a VIEW (see the byte-view block below), and never
            // reaches this arm. Here it is the fallback — a view that
            // must become a first-class `List[int]` value (a binding, an
            // argument, a return) materializes exactly as it always
            // did, threading the foreign chain like `List[T]()`.
            "bytes" => {
                let r = self
                    .rt_call_foreign("__wolf_rt_str_bytes", &[sp, sl], None, Some(types::PTR))
                    .expect("list");
                Ok(Flow::Val(Some(r)))
            }
            "starts_with" | "ends_with" | "contains" => {
                let (np, nl) = needle(self, 0)?;
                let mode = match mname {
                    "starts_with" => 0,
                    "ends_with" => 1,
                    _ => 2,
                };
                let m = self.b.iconst(types::I64, mode);
                let rc = self
                    .rt_call(
                        "__wolf_rt_str_probe",
                        &[sp, sl, np, nl, m],
                        Some(types::I64),
                    )
                    .expect("rc");
                Ok(Flow::Val(Some(self.nonzero(rc))))
            }
            "find" | "rfind" => {
                let (np, nl) = needle(self, 0)?;
                let rev = self.b.iconst(types::I64, i64::from(mname == "rfind"));
                let rc = self
                    .rt_call(
                        "__wolf_rt_str_find",
                        &[sp, sl, np, nl, rev],
                        Some(types::I64),
                    )
                    .expect("rc");
                let z = self.b.iconst(types::I64, 0);
                let hit = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[rc, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Sge),
                    )
                    .one();
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(eu, hit, |_| Ok(Some(rc)), |z| Ok(z.none_tag()))?;
                Ok(Flow::Val(Some(out)))
            }
            "count" => {
                let (np, nl) = needle(self, 0)?;
                let rc = self
                    .rt_call("__wolf_rt_str_count", &[sp, sl, np, nl], Some(types::I64))
                    .expect("rc");
                Ok(Flow::Val(Some(rc)))
            }
            "split" | "words" | "lines" => {
                let (np, nl, mode) = if mname == "split" {
                    let (np, nl) = needle(self, 0)?;
                    (np, nl, 0)
                } else {
                    // No needle: pass the receiver's pointer with len 0
                    // (the shim ignores it in these modes).
                    let z = self.b.iconst(types::I64, 0);
                    (sp, z, if mname == "words" { 1 } else { 2 })
                };
                let m = self.b.iconst(types::I64, mode);
                let r = self
                    .rt_call_foreign(
                        "__wolf_rt_str_split",
                        &[sp, sl, np, nl, m],
                        None,
                        Some(types::PTR),
                    )
                    .expect("list");
                Ok(Flow::Val(Some(r)))
            }
            "trim" | "trim_start" | "trim_end" => {
                let mode = match mname {
                    "trim" => 0,
                    "trim_start" => 1,
                    _ => 2,
                };
                let m = self.b.iconst(types::I64, mode);
                let (region, slot) = self.rt_slot(16);
                self.rt_call_slot("__wolf_rt_str_trim", &[sp, sl, m], slot, region, None);
                Ok(Flow::Val(Some(self.load_str_slot(slot, region, e.span)?)))
            }
            "lower" | "upper" => {
                let up = self.b.iconst(types::I64, i64::from(mname == "upper"));
                let (region, slot) = self.rt_slot(16);
                self.rt_call_slot("__wolf_rt_str_case", &[sp, sl, up], slot, region, None);
                Ok(Flow::Val(Some(self.load_str_slot(slot, region, e.span)?)))
            }
            "strip_prefix" | "strip_suffix" => {
                let (np, nl) = needle(self, 0)?;
                let suffix = self
                    .b
                    .iconst(types::I64, i64::from(mname == "strip_suffix"));
                let (region, slot) = self.rt_slot(16);
                let rc = self
                    .rt_call_slot(
                        "__wolf_rt_str_strip",
                        &[sp, sl, np, nl, suffix],
                        slot,
                        region,
                        Some(types::I64),
                    )
                    .expect("rc");
                let hit = self.nonzero(rc);
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |z| Ok(Some(z.load_str_slot(slot, region, e.span)?)),
                    |z| Ok(z.none_tag()),
                )?;
                Ok(Flow::Val(Some(out)))
            }
            "repeat" => {
                let n = arg_val(self, 0)?;
                // A negative count is a caller contract violation:
                // the `assert` trap, ruled by [mem.str.repeat] (#57)
                // — not an out-of-range access.
                let z = self.b.iconst(types::I64, 0);
                let ok = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[n, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Sge),
                    )
                    .one();
                if self.trap_unless(ok, TrapKind::Assert) {
                    return Ok(Flow::Diverged);
                }
                let (region, slot) = self.rt_slot(16);
                self.rt_call_slot("__wolf_rt_str_repeat", &[sp, sl, n], slot, region, None);
                Ok(Flow::Val(Some(self.load_str_slot(slot, region, e.span)?)))
            }
            "replace" => {
                let (fp, fl) = needle(self, 0)?;
                let (tp, tl) = needle(self, 1)?;
                let (region, slot) = self.rt_slot(16);
                self.rt_call_slot(
                    "__wolf_rt_str_replace",
                    &[sp, sl, fp, fl, tp, tl],
                    slot,
                    region,
                    None,
                );
                Ok(Flow::Val(Some(self.load_str_slot(slot, region, e.span)?)))
            }
            _ => Err(refuse(
                "this `str` method (outside the s37 builtin set)",
                e.span,
            )),
        }
    }

    // -------------------------- List element access (s75, #77) ----
    //
    // Element traffic is `ptr.off` + `load`/`store` through the
    // function's one foreign region, NOT a runtime call: an opaque
    // call is a wall the vectorizer cannot see past, and s44 measured
    // the wall at 44-92 ns per element against C's 0.2-1.0. What
    // stays in `wolf_rt` is what genuinely needs it — allocating a
    // header (`list_new`) and growing a buffer (`list_push`). Both
    // thread the foreign region's token, so no cached `data`/`len`
    // survives a growth.
    //
    // The bounds check moves INTO the caller, where the range
    // analysis can see it (X3's sibling: eliminating a check because
    // it is provable is optimization, eliminating it because it is
    // inconvenient is not). One unsigned compare covers both ends —
    // a negative index is a very large unsigned one.

    /// The function's runtime-storage regions, minted on first touch
    /// and rooted at the entry block: `(headers, element buffers)`.
    fn foreign_regions(&mut self) -> (RegionId, RegionId) {
        match self.foreign {
            Some(rs) => rs,
            None => {
                let hdrs = self.b.ins_region_foreign(ForeignRole::Header);
                let bufs = self.b.ins_region_foreign(ForeignRole::Buffer);
                self.foreign = Some((hdrs, bufs));
                (hdrs, bufs)
            }
        }
    }

    fn foreign_hdr_region(&mut self) -> RegionId {
        self.foreign_regions().0
    }

    fn foreign_buf_region(&mut self) -> RegionId {
        self.foreign_regions().1
    }

    /// `%d = load.ptr %hdr` — a `List`'s element buffer.
    fn list_data(&mut self, hdr: Value) -> Value {
        let r = self.foreign_hdr_region();
        let addr = self.field_addr(hdr, LIST_DATA_OFF);
        self.b.ins_load(types::PTR, addr, r)
    }

    /// `%n = load.i64 %hdr+8` — a `List`'s live element count.
    fn list_len_of(&mut self, hdr: Value) -> Value {
        let r = self.foreign_hdr_region();
        let addr = self.field_addr(hdr, LIST_LEN_OFF);
        self.b.ins_load(types::I64, addr, r)
    }

    /// `%c = load.i64 %hdr+16` — a `List`'s element capacity.
    fn list_cap_of(&mut self, hdr: Value) -> Value {
        let r = self.foreign_hdr_region();
        let addr = self.field_addr(hdr, LIST_CAP_OFF);
        self.b.ins_load(types::I64, addr, r)
    }

    /// `store.i64 %n, %hdr+8` — publish a new element count.
    fn list_set_len(&mut self, hdr: Value, n: Value) {
        let r = self.foreign_hdr_region();
        let addr = self.field_addr(hdr, LIST_LEN_OFF);
        self.b.ins_store(n, addr, r);
    }

    /// `%p = ptr.off %data, %idx, esize` — element `idx`'s address.
    fn list_elem_addr(&mut self, data: Value, idx: Value, esize: u64) -> Value {
        self.b.ins_ptr_off(data, idx, esize)
    }

    /// The element stride, checked to TILE at the element's alignment.
    ///
    /// A shim copied elements byte-wise, so a packed layout that does
    /// not tile (`{i32, i64}` — 12 bytes, so every other element lands
    /// 4 bytes off) cost nothing but a memcpy. Compiled loads claim
    /// natural alignment, so the same layout would be a misaligned
    /// access. Rather than quietly emit one, refuse: the conservatism
    /// ledger is the honest home for a shape the packed v0 layout
    /// cannot address directly. (Every scalar element tiles, because
    /// its stride IS its alignment; so do the aggregates the corpus
    /// holds.)
    fn list_stride(&mut self, ewty: TypeId, span: Span) -> R<u64> {
        let Some(esize) = flat_size(&self.b.module.types, ewty) else {
            return Err(refuse("List elements without a flat layout", span));
        };
        let align = flat_align(&self.b.module.types, ewty);
        if align == 0 || esize % align != 0 {
            return Err(refuse(
                "List elements whose packed layout does not tile at their alignment",
                span,
            ));
        }
        Ok(esize)
    }

    /// `%b = icmp.ult %idx, %len` — the in-bounds test.
    fn list_in_bounds(&mut self, idx: Value, len: Value) -> Value {
        self.b
            .ins(
                Opcode::Icmp,
                &[idx, len],
                &[types::BOOL],
                Aux::IntCc(IntCc::Ult),
            )
            .one()
    }

    /// The trapping spelling of the same test (`l[i]`, `l[i] = v`).
    /// Returns `true` when the trap is PROVEN — the caller diverged.
    fn list_bounds_trap(&mut self, idx: Value, len: Value) -> bool {
        let ok = self.list_in_bounds(idx, len);
        self.trap_unless(ok, TrapKind::Bounds)
    }

    /// Load element `idx` (bounds already established by the caller).
    fn list_load_at(&mut self, hdr: Value, idx: Value, ewty: TypeId, span: Span) -> R<Value> {
        let esize = self.list_stride(ewty, span)?;
        let data = self.list_data(hdr);
        let p = self.list_elem_addr(data, idx, esize);
        let r = self.foreign_buf_region();
        self.load_flat(ewty, p, r, span)
    }

    /// Store `v` at element `idx` (bounds already established).
    fn list_store_at(&mut self, hdr: Value, idx: Value, v: Value, span: Span) -> R<()> {
        let vty = self.b.func.value_ty(v);
        let esize = self.list_stride(vty, span)?;
        let data = self.list_data(hdr);
        let p = self.list_elem_addr(data, idx, esize);
        let r = self.foreign_buf_region();
        self.store_flat(v, p, r, span)
    }

    /// The `List` method depth, natively (s40, rebuilt s75): the
    /// header pointer IS the value; element traffic is direct memory
    /// through the foreign region, and only allocation and growth
    /// remain `wolf_rt::list` calls. Recoverable reads
    /// (`pop`/`get`/`first`/`last`) are `{none}` rows.
    fn lower_list_method(
        &mut self,
        d: CallExpr<'t>,
        recv_place: &'t GreenNode,
        elem_sema: TyId,
        mname: &str,
        e: &'t GreenNode,
    ) -> R<Flow> {
        self.refuse_region_elem(elem_sema, e.span)?;
        // s105: a MUTATING method on a captured List — the env holds a
        // copy of the handle, so the write would land on the live
        // list while the reference semantics (S-10 deep copy at
        // capture) says it must not. Reads stay divergence-free
        // because the loan forbids writes while the closure lives;
        // writes from inside are the one spelling left, refused by
        // name (borrow-only closures).
        if matches!(mname, "push" | "pop" | "clear") {
            self.check_capture_write(recv_place, "mutating")?;
        }
        let Some(ewty) = wir_ty(
            &mut self.b.module.types,
            self.table,
            self.sigs,
            elem_sema,
            e.span,
        )?
        else {
            return Err(refuse("unit-typed List elements", e.span));
        };
        let Some(esize) = flat_size(&self.b.module.types, ewty) else {
            return Err(refuse("List elements without a flat layout", e.span));
        };
        let Some(hdr) = flow_val!(self.lower_expr(recv_place)) else {
            return Err(refuse("a valueless List receiver", recv_place.span));
        };
        let arg_exprs: Vec<&'t GreenNode> = d
            .args()
            .into_iter()
            .flat_map(|l| l.args())
            .filter_map(Arg::value)
            .collect();
        match mname {
            // Growth is the one thing the runtime still owns: a push
            // may reallocate, and the arena discipline lives there.
            // The COMMON case is not growth (#113: sixteen out-of-line
            // `list_push` calls and their slot spills were 31.5% of
            // `b3_churn`'s per-request instructions), so the in-bounds
            // append inlines — the G1 shape, applied to the write
            // side: load `len`/`cap`, and when there is room, store
            // the element at `data[len]` through the buffer region and
            // publish `len + 1` through the header region, exactly the
            // stores `pop`/`clear` already make in place. Only a full
            // buffer calls out. The value is lowered BEFORE the length
            // is read (it may itself push — the `get` discipline), and
            // a non-tiling element keeps the call-only path: a direct
            // store claims natural alignment the packed v0 layout
            // cannot promise there (the `list_stride` rule), while the
            // slot-spill call copies bytes and promises nothing.
            "push" => {
                let Some(vx) = arg_exprs.first() else {
                    return Err(refuse("`push` without a value", e.span));
                };
                let v = match self.lower_expr(vx)? {
                    Flow::Val(Some(v)) => v,
                    Flow::Val(None) => return Err(refuse("unit-typed List elements", vx.span)),
                    Flow::Diverged => return Ok(Flow::Diverged),
                };
                let align = flat_align(&self.b.module.types, ewty);
                let tiles = align != 0 && esize % align == 0;
                if !tiles {
                    let (region, slot) = self.rt_slot(esize);
                    self.store_flat(v, slot, region, vx.span)?;
                    self.rt_call_foreign("__wolf_rt_list_push", &[hdr], Some((slot, region)), None);
                    return Ok(Flow::Val(None));
                }
                let len = self.list_len_of(hdr);
                let cap = self.list_cap_of(hdr);
                let room = self.list_in_bounds(len, cap);
                let fast_bb = self.b.create_block();
                let slow_bb = self.b.create_block();
                let merge = self.b.create_block();
                self.b.ins_br(room, fast_bb, &[], slow_bb, &[]);
                self.b.seal_block(fast_bb);
                self.b.seal_block(slow_bb);
                self.b.switch_to_block(fast_bb);
                self.b.gvn_push_scope();
                let data = self.list_data(hdr);
                let p = self.list_elem_addr(data, len, esize);
                let r = self.foreign_buf_region();
                self.store_flat(v, p, r, vx.span)?;
                let one = self.b.iconst(types::I64, 1);
                let len1 = self
                    .b
                    .ins(Opcode::IaddWrap, &[len, one], &[types::I64], Aux::None)
                    .one();
                self.list_set_len(hdr, len1);
                self.b.ins_jmp(merge, &[]);
                self.b.gvn_pop_scope();
                self.b.switch_to_block(slow_bb);
                self.b.gvn_push_scope();
                let (region, slot) = self.rt_slot(esize);
                self.store_flat(v, slot, region, vx.span)?;
                self.rt_call_foreign("__wolf_rt_list_push", &[hdr], Some((slot, region)), None);
                self.b.ins_jmp(merge, &[]);
                self.b.gvn_pop_scope();
                self.b.seal_block(merge);
                self.b.switch_to_block(merge);
                Ok(Flow::Val(None))
            }
            // `pop` shrinks in place: publish `len - 1`, then read the
            // element that just left the live prefix. Same order the
            // shim used, so the observable result is unchanged.
            "pop" => {
                let n = self.list_len_of(hdr);
                let z = self.b.iconst(types::I64, 0);
                let hit = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[n, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Sgt),
                    )
                    .one();
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |z| {
                        let one = z.b.iconst(types::I64, 1);
                        let last =
                            z.b.ins(Opcode::IsubWrap, &[n, one], &[types::I64], Aux::None)
                                .one();
                        z.list_set_len(hdr, last);
                        Ok(Some(z.list_load_at(hdr, last, ewty, e.span)?))
                    },
                    |z| Ok(z.none_tag()),
                )?;
                Ok(Flow::Val(Some(out)))
            }
            "get" | "first" | "last" => {
                // The index expression runs BEFORE the length is read:
                // it may call something that pushes, and a length read
                // ahead of it would be stale.
                let given = match mname {
                    "get" => {
                        let Some(ix) = arg_exprs.first() else {
                            return Err(refuse("`get` without an index", e.span));
                        };
                        match self.lower_expr(ix)? {
                            Flow::Val(Some(v)) => Some(v),
                            _ => return Err(refuse("a valueless List index", ix.span)),
                        }
                    }
                    "first" => Some(self.b.iconst(types::I64, 0)),
                    _ => None,
                };
                let n = self.list_len_of(hdr);
                let idx = match given {
                    Some(v) => v,
                    // `last`: an empty list yields -1, which the
                    // unsigned in-bounds test rejects — the shim's
                    // `idx < 0` arm, spelled in one compare.
                    None => {
                        let one = self.b.iconst(types::I64, 1);
                        self.b
                            .ins(Opcode::IsubWrap, &[n, one], &[types::I64], Aux::None)
                            .one()
                    }
                };
                let hit = self.list_in_bounds(idx, n);
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |z| Ok(Some(z.list_load_at(hdr, idx, ewty, e.span)?)),
                    |z| Ok(z.none_tag()),
                )?;
                Ok(Flow::Val(Some(out)))
            }
            "is_empty" => {
                let n = self.list_len_of(hdr);
                let z = self.b.iconst(types::I64, 0);
                let r = self
                    .b
                    .ins(Opcode::Icmp, &[n, z], &[types::BOOL], Aux::IntCc(IntCc::Eq))
                    .one();
                Ok(Flow::Val(Some(r)))
            }
            "count" => Ok(Flow::Val(Some(self.list_len_of(hdr)))),
            "clear" => {
                let z = self.b.iconst(types::I64, 0);
                self.list_set_len(hdr, z);
                Ok(Flow::Val(None))
            }
            _ => Err(refuse("this List method (s05 std surface)", e.span)),
        }
    }

    /// The s38 io/fs builtin family, natively (s40): each call is one
    /// `wolf_rt::fs` shim; the returned code becomes the row value
    /// here (`fs_code_tag` mirrors the checked `errtag` coarsening),
    /// text results reload from the out slot as `{ptr, len}` pairs.
    fn lower_fs_builtin(&mut self, name: &str, d: CallExpr<'t>, e: &'t GreenNode) -> R<Flow> {
        let mut argv: Vec<Value> = Vec::new();
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            let Some(vx) = Arg::value(a) else { continue };
            match self.lower_expr(vx)? {
                Flow::Val(Some(v)) => argv.push(v),
                Flow::Val(None) => return Err(refuse("unit-typed fs arguments", vx.span)),
                Flow::Diverged => return Ok(Flow::Diverged),
            }
        }
        let arg = |i: usize| -> R<Value> {
            argv.get(i)
                .copied()
                .ok_or_else(|| refuse("an fs call with missing arguments", e.span))
        };
        let zero_eq = |zelf: &mut Self, rc: Value| -> Value {
            let z = zelf.b.iconst(types::I64, 0);
            zelf.b
                .ins(
                    Opcode::Icmp,
                    &[rc, z],
                    &[types::BOOL],
                    Aux::IntCc(IntCc::Eq),
                )
                .one()
        };
        match name {
            "fs_exists" => {
                let s = arg(0)?;
                let (p, l) = self.str_parts(s);
                let rc = self
                    .rt_call("__wolf_rt_fs_exists", &[p, l], Some(types::I64))
                    .expect("rc");
                Ok(Flow::Val(Some(self.nonzero(rc))))
            }
            // s90: the total predicates — `exists` finally says WHAT
            // exists. One shim, `want` selecting file/dir.
            "fs_is_file" | "fs_is_dir" => {
                let s = arg(0)?;
                let (p, l) = self.str_parts(s);
                let want = self.b.iconst(types::I64, i64::from(name == "fs_is_dir"));
                let rc = self
                    .rt_call("__wolf_rt_fs_is", &[p, l, want], Some(types::I64))
                    .expect("rc");
                Ok(Flow::Val(Some(self.nonzero(rc))))
            }
            "fs_read_text" | "read_line" | "fs_read" => {
                let (region, slot) = self.rt_slot(16);
                let rc = match name {
                    "fs_read_text" => {
                        let s = arg(0)?;
                        let (p, l) = self.str_parts(s);
                        self.rt_call_slot(
                            "__wolf_rt_fs_read_text",
                            &[p, l],
                            slot,
                            region,
                            Some(types::I64),
                        )
                    }
                    "read_line" => self.rt_call_slot(
                        "__wolf_rt_read_line",
                        &[],
                        slot,
                        region,
                        Some(types::I64),
                    ),
                    _ => {
                        let fd = arg(0)?;
                        let max = arg(1)?;
                        self.rt_call_slot(
                            "__wolf_rt_fs_read",
                            &[fd, max],
                            slot,
                            region,
                            Some(types::I64),
                        )
                    }
                }
                .expect("rc");
                let hit = zero_eq(self, rc);
                let eu = self.eu_ty_of(e.span)?;
                let declared = self.row_tag_names(e.span);
                let out = self.eu_join(
                    eu,
                    hit,
                    |z| Ok(Some(z.load_str_slot(slot, region, e.span)?)),
                    |z| Ok(z.fs_code_tag(rc, &declared)),
                )?;
                Ok(Flow::Val(Some(out)))
            }
            // s90: a list result (`List[int]` bytes, `List[str]` names)
            // or a single word (size, timestamp) rides an out slot the
            // same way a str pair does. The list-minting calls go
            // through `rt_call_foreign` — the shim allocates into the
            // container regions, so no `data`/`len` the caller loaded
            // may survive it.
            "fs_read_bytes" | "fs_read_chunk" | "fs_read_dir" => {
                let (region, slot) = self.rt_slot(8);
                let (sym, args): (&'static str, Vec<Value>) = match name {
                    "fs_read_chunk" => {
                        let fd = arg(0)?;
                        let max = arg(1)?;
                        ("__wolf_rt_fs_read_chunk", vec![fd, max])
                    }
                    _ => {
                        let s = arg(0)?;
                        let (p, l) = self.str_parts(s);
                        let sym = if name == "fs_read_bytes" {
                            "__wolf_rt_fs_read_bytes"
                        } else {
                            "__wolf_rt_fs_read_dir"
                        };
                        (sym, vec![p, l])
                    }
                };
                let rc = self
                    .rt_call_foreign(sym, &args, Some((slot, region)), Some(types::I64))
                    .expect("rc");
                let hit = zero_eq(self, rc);
                let eu = self.eu_ty_of(e.span)?;
                let declared = self.row_tag_names(e.span);
                let out = self.eu_join(
                    eu,
                    hit,
                    |z| Ok(Some(z.load_flat(types::PTR, slot, region, e.span)?)),
                    |z| Ok(z.fs_code_tag(rc, &declared)),
                )?;
                Ok(Flow::Val(Some(out)))
            }
            "fs_size" | "fs_modified_ms" => {
                let s = arg(0)?;
                let (p, l) = self.str_parts(s);
                let which = self
                    .b
                    .iconst(types::I64, i64::from(name == "fs_modified_ms"));
                let (region, slot) = self.rt_slot(8);
                let rc = self
                    .rt_call_slot(
                        "__wolf_rt_fs_stat",
                        &[p, l, which],
                        slot,
                        region,
                        Some(types::I64),
                    )
                    .expect("rc");
                let hit = zero_eq(self, rc);
                let eu = self.eu_ty_of(e.span)?;
                let declared = self.row_tag_names(e.span);
                let out = self.eu_join(
                    eu,
                    hit,
                    |z| Ok(Some(z.load_flat(types::I64, slot, region, e.span)?)),
                    |z| Ok(z.fs_code_tag(rc, &declared)),
                )?;
                Ok(Flow::Val(Some(out)))
            }
            "fs_write_text" | "fs_write" | "fs_close" | "fs_remove" | "fs_write_bytes"
            | "fs_write_chunk" | "fs_create_dir" | "fs_create_dir_all" | "fs_remove_dir"
            | "fs_remove_dir_all" | "fs_rename" => {
                let rc = match name {
                    "fs_write_text" => {
                        let path = arg(0)?;
                        let contents = arg(1)?;
                        let (pp, pl) = self.str_parts(path);
                        let (cp, cl) = self.str_parts(contents);
                        self.rt_call(
                            "__wolf_rt_fs_write_text",
                            &[pp, pl, cp, cl],
                            Some(types::I64),
                        )
                    }
                    "fs_write" => {
                        let fd = arg(0)?;
                        let s = arg(1)?;
                        let (sp, sl) = self.str_parts(s);
                        self.rt_call("__wolf_rt_fs_write", &[fd, sp, sl], Some(types::I64))
                    }
                    "fs_close" => {
                        let fd = arg(0)?;
                        self.rt_call("__wolf_rt_fs_close", &[fd], Some(types::I64))
                    }
                    // s90 byte writes: the `List[int]` argument is one
                    // header pointer, and the shim READS the caller's
                    // buffer — `rt_call_foreign`, like `str_from_utf8`.
                    "fs_write_bytes" => {
                        let path = arg(0)?;
                        let hdr = arg(1)?;
                        let (pp, pl) = self.str_parts(path);
                        self.rt_call_foreign(
                            "__wolf_rt_fs_write_bytes",
                            &[pp, pl, hdr],
                            None,
                            Some(types::I64),
                        )
                    }
                    "fs_write_chunk" => {
                        let fd = arg(0)?;
                        let hdr = arg(1)?;
                        self.rt_call_foreign(
                            "__wolf_rt_fs_write_chunk",
                            &[fd, hdr],
                            None,
                            Some(types::I64),
                        )
                    }
                    "fs_create_dir" | "fs_create_dir_all" => {
                        let path = arg(0)?;
                        let (pp, pl) = self.str_parts(path);
                        let all = self
                            .b
                            .iconst(types::I64, i64::from(name == "fs_create_dir_all"));
                        self.rt_call("__wolf_rt_fs_create_dir", &[pp, pl, all], Some(types::I64))
                    }
                    "fs_remove_dir" | "fs_remove_dir_all" => {
                        let path = arg(0)?;
                        let (pp, pl) = self.str_parts(path);
                        let all = self
                            .b
                            .iconst(types::I64, i64::from(name == "fs_remove_dir_all"));
                        self.rt_call("__wolf_rt_fs_remove_dir", &[pp, pl, all], Some(types::I64))
                    }
                    "fs_rename" => {
                        let from = arg(0)?;
                        let to = arg(1)?;
                        let (fp, fl) = self.str_parts(from);
                        let (tp, tl) = self.str_parts(to);
                        self.rt_call("__wolf_rt_fs_rename", &[fp, fl, tp, tl], Some(types::I64))
                    }
                    _ => {
                        let path = arg(0)?;
                        let (pp, pl) = self.str_parts(path);
                        self.rt_call("__wolf_rt_fs_remove", &[pp, pl], Some(types::I64))
                    }
                }
                .expect("rc");
                let hit = zero_eq(self, rc);
                let eu = self.eu_ty_of(e.span)?;
                let declared = self.row_tag_names(e.span);
                let out =
                    self.eu_join(eu, hit, |_| Ok(None), |z| Ok(z.fs_code_tag(rc, &declared)))?;
                Ok(Flow::Val(Some(out)))
            }
            // s90/#52: one moded open under three spellings.
            // `fs_open`/`fs_create` pass the mode constants they
            // always meant (0 read, 1 write+truncate); `fs_open_mode`
            // passes the caller's, and the runtime answers an unknown
            // one with `-invalid` before touching the filesystem.
            "fs_open" | "fs_create" | "fs_open_mode" => {
                let path = arg(0)?;
                let (pp, pl) = self.str_parts(path);
                let mode = match name {
                    "fs_open_mode" => arg(1)?,
                    _ => self.b.iconst(types::I64, i64::from(name == "fs_create")),
                };
                let rc = self
                    .rt_call("__wolf_rt_fs_open", &[pp, pl, mode], Some(types::I64))
                    .expect("rc");
                let z = self.b.iconst(types::I64, 0);
                let hit = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[rc, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Sge),
                    )
                    .one();
                let eu = self.eu_ty_of(e.span)?;
                let declared = self.row_tag_names(e.span);
                let out = self.eu_join(
                    eu,
                    hit,
                    |_| Ok(Some(rc)),
                    |zelf| {
                        // The failure code arrived negated.
                        let zz = zelf.b.iconst(types::I64, 0);
                        let code = zelf
                            .b
                            .ins(Opcode::IsubWrap, &[zz, rc], &[types::I64], Aux::None)
                            .one();
                        Ok(zelf.fs_code_tag(code, &declared))
                    },
                )?;
                Ok(Flow::Val(Some(out)))
            }
            _ => Err(refuse("this io/fs builtin", e.span)),
        }
    }

    /// The s39 net builtin family, natively (s106 — the crossing #118
    /// tracked): each call is one `wolf_rt::net` shim over the process
    /// NetTable; the returned code (`wolf_rt::net::net_code`) becomes
    /// the row value here through [`Self::code_tag_chain`], coarsened
    /// against the DECLARED row exactly as the checked executor's
    /// `coarse` — a tag the call's `!T` does not declare reports `io`.
    /// The read result reloads from the out slot as a `{ptr, len}`
    /// pair; fd results ride the `fs_open` convention (the handle
    /// >= 0, or the negated code).
    fn lower_net_builtin(&mut self, name: &str, d: CallExpr<'t>, e: &'t GreenNode) -> R<Flow> {
        let mut argv: Vec<Value> = Vec::new();
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            let Some(vx) = Arg::value(a) else { continue };
            match self.lower_expr(vx)? {
                Flow::Val(Some(v)) => argv.push(v),
                Flow::Val(None) => return Err(refuse("unit-typed net arguments", vx.span)),
                Flow::Diverged => return Ok(Flow::Diverged),
            }
        }
        let arg = |i: usize| -> R<Value> {
            argv.get(i)
                .copied()
                .ok_or_else(|| refuse("a net call with missing arguments", e.span))
        };
        // The wire codes, as (coarsened) row tags. `utf8` is only
        // `net_read`'s; on every other row it coarsens like the rest.
        let declared = self.row_tag_names(e.span);
        let tag_pairs: Vec<(i64, &str)> =
            [(1, "refused"), (2, "timeout"), (3, "closed"), (4, "utf8")]
                .into_iter()
                .map(|(c, t)| {
                    (
                        c,
                        if declared.iter().any(|d| d == t) {
                            t
                        } else {
                            "io"
                        },
                    )
                })
                .collect();
        let zero_eq = |zelf: &mut Self, rc: Value| -> Value {
            let z = zelf.b.iconst(types::I64, 0);
            zelf.b
                .ins(
                    Opcode::Icmp,
                    &[rc, z],
                    &[types::BOOL],
                    Aux::IntCc(IntCc::Eq),
                )
                .one()
        };
        match name {
            // The fd family: the handle (>= 0), or `-code`.
            "net_listen" | "net_connect" | "net_port" | "net_accept" => {
                let rc = match name {
                    "net_listen" | "net_connect" => {
                        let s = arg(0)?;
                        let (p, l) = self.str_parts(s);
                        let sym = if name == "net_listen" {
                            "__wolf_rt_net_listen"
                        } else {
                            "__wolf_rt_net_connect"
                        };
                        self.rt_call(sym, &[p, l], Some(types::I64))
                    }
                    "net_port" => {
                        let fd = arg(0)?;
                        self.rt_call("__wolf_rt_net_port", &[fd], Some(types::I64))
                    }
                    _ => {
                        let fd = arg(0)?;
                        self.rt_call("__wolf_rt_net_accept", &[fd], Some(types::I64))
                    }
                }
                .expect("rc");
                let z = self.b.iconst(types::I64, 0);
                let hit = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[rc, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Sge),
                    )
                    .one();
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |_| Ok(Some(rc)),
                    |zelf| {
                        // The failure code arrived negated.
                        let zz = zelf.b.iconst(types::I64, 0);
                        let code = zelf
                            .b
                            .ins(Opcode::IsubWrap, &[zz, rc], &[types::I64], Aux::None)
                            .one();
                        Ok(zelf.code_tag_chain(code, &tag_pairs, "io"))
                    },
                )?;
                Ok(Flow::Val(Some(out)))
            }
            "net_read" => {
                let fd = arg(0)?;
                let max = arg(1)?;
                let (region, slot) = self.rt_slot(16);
                let rc = self
                    .rt_call_slot(
                        "__wolf_rt_net_read",
                        &[fd, max],
                        slot,
                        region,
                        Some(types::I64),
                    )
                    .expect("rc");
                let hit = zero_eq(self, rc);
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |zelf| Ok(Some(zelf.load_str_slot(slot, region, e.span)?)),
                    |zelf| Ok(zelf.code_tag_chain(rc, &tag_pairs, "io")),
                )?;
                Ok(Flow::Val(Some(out)))
            }
            "net_write" | "net_close" | "net_deadline" => {
                let rc = match name {
                    "net_write" => {
                        let fd = arg(0)?;
                        let s = arg(1)?;
                        let (sp, sl) = self.str_parts(s);
                        self.rt_call("__wolf_rt_net_write", &[fd, sp, sl], Some(types::I64))
                    }
                    "net_close" => {
                        let fd = arg(0)?;
                        self.rt_call("__wolf_rt_net_close", &[fd], Some(types::I64))
                    }
                    _ => {
                        let fd = arg(0)?;
                        let ms = arg(1)?;
                        self.rt_call("__wolf_rt_net_deadline", &[fd, ms], Some(types::I64))
                    }
                }
                .expect("rc");
                let hit = zero_eq(self, rc);
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |_| Ok(None),
                    |zelf| Ok(zelf.code_tag_chain(rc, &tag_pairs, "io")),
                )?;
                Ok(Flow::Val(Some(out)))
            }
            _ => Err(refuse("this net builtin", e.span)),
        }
    }

    /// The s40 json builtin family, natively (s107 — the crossing that
    /// closes #118): each call is one `wolf_rt::json` shim over the
    /// hand-mirrored reference (`wolf_mem::json` stays the semantic
    /// authority; the driver's json_parity test pins the copies).
    /// Wire codes (`wolf_rt::json::json_code`: 1 parse, 2 missing,
    /// 3 kind) become row values through [`Self::code_tag_chain`];
    /// str results reload from the out slot; `json_len` rides the
    /// `fs_open` handle convention (the count >= 0, or the negated
    /// code — a json length is never negative). PURE: no capability
    /// tag, no process state, the one shim family with neither.
    fn lower_json_builtin(&mut self, name: &str, d: CallExpr<'t>, e: &'t GreenNode) -> R<Flow> {
        let mut argv: Vec<Value> = Vec::new();
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            let Some(vx) = Arg::value(a) else { continue };
            match self.lower_expr(vx)? {
                Flow::Val(Some(v)) => argv.push(v),
                Flow::Val(None) => return Err(refuse("unit-typed json arguments", vx.span)),
                Flow::Diverged => return Ok(Flow::Diverged),
            }
        }
        let arg = |i: usize| -> R<Value> {
            argv.get(i)
                .copied()
                .ok_or_else(|| refuse("a json call with missing arguments", e.span))
        };
        if name == "json_valid" {
            let s = arg(0)?;
            let (sp, sl) = self.str_parts(s);
            let rc = self
                .rt_call("__wolf_rt_json_valid", &[sp, sl], Some(types::I64))
                .expect("rc");
            return Ok(Flow::Val(Some(self.nonzero(rc))));
        }
        let s = arg(0)?;
        let path = arg(1)?;
        let (sp, sl) = self.str_parts(s);
        let (pp, pl) = self.str_parts(path);
        match name {
            "json_get" | "json_type" => {
                // The declared row is `{parse, missing}`; `kind` is
                // unreachable on these entries (a scalar step answers
                // `missing` in the walk), and any residue coarsens to
                // `parse` — the RFC-violation catch-all.
                let sym = if name == "json_get" {
                    "__wolf_rt_json_get"
                } else {
                    "__wolf_rt_json_type"
                };
                let (region, slot) = self.rt_slot(16);
                let rc = self
                    .rt_call_slot(sym, &[sp, sl, pp, pl], slot, region, Some(types::I64))
                    .expect("rc");
                let z = self.b.iconst(types::I64, 0);
                let hit = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[rc, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Eq),
                    )
                    .one();
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |zelf| Ok(Some(zelf.load_str_slot(slot, region, e.span)?)),
                    |zelf| Ok(zelf.code_tag_chain(rc, &[(2, "missing")], "parse")),
                )?;
                Ok(Flow::Val(Some(out)))
            }
            "json_len" => {
                let rc = self
                    .rt_call("__wolf_rt_json_len", &[sp, sl, pp, pl], Some(types::I64))
                    .expect("rc");
                let z = self.b.iconst(types::I64, 0);
                let hit = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[rc, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Sge),
                    )
                    .one();
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |_| Ok(Some(rc)),
                    |zelf| {
                        // The failure code arrived negated.
                        let zz = zelf.b.iconst(types::I64, 0);
                        let code = zelf
                            .b
                            .ins(Opcode::IsubWrap, &[zz, rc], &[types::I64], Aux::None)
                            .one();
                        Ok(zelf.code_tag_chain(code, &[(1, "parse"), (2, "missing")], "kind"))
                    },
                )?;
                Ok(Flow::Val(Some(out)))
            }
            _ => Err(refuse("this json builtin", e.span)),
        }
    }

    /// The s40 process trio, natively (s107): `wolf_rt::os`'s
    /// ChildTable shims over `std::process`. The argv `List[str]`
    /// crosses as its header pointer through [`Self::rt_call_foreign`]
    /// (the shim READS the caller's list buffer — the
    /// `fs_write_bytes` posture); the spawn handle rides the `fs_open`
    /// convention, the exit code comes back through an out word whose
    /// 0-code REAPS (the runtime's zombie discipline), and codes
    /// (`wolf_rt::os::proc_code`) become the declared rows —
    /// `{not_found, denied, io}` at spawn, `{signal, io}` at wait,
    /// `{io}` at kill.
    fn lower_process_builtin(&mut self, name: &str, d: CallExpr<'t>, e: &'t GreenNode) -> R<Flow> {
        let mut argv: Vec<Value> = Vec::new();
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            let Some(vx) = Arg::value(a) else { continue };
            match self.lower_expr(vx)? {
                Flow::Val(Some(v)) => argv.push(v),
                Flow::Val(None) => return Err(refuse("unit-typed process arguments", vx.span)),
                Flow::Diverged => return Ok(Flow::Diverged),
            }
        }
        let arg = |i: usize| -> R<Value> {
            argv.get(i)
                .copied()
                .ok_or_else(|| refuse("a process call with missing arguments", e.span))
        };
        match name {
            "os_spawn" => {
                let hdr = arg(0)?;
                let rc = self
                    .rt_call_foreign("__wolf_rt_os_spawn", &[hdr], None, Some(types::I64))
                    .expect("rc");
                let z = self.b.iconst(types::I64, 0);
                let hit = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[rc, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Sge),
                    )
                    .one();
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |_| Ok(Some(rc)),
                    |zelf| {
                        // The failure code arrived negated.
                        let zz = zelf.b.iconst(types::I64, 0);
                        let code = zelf
                            .b
                            .ins(Opcode::IsubWrap, &[zz, rc], &[types::I64], Aux::None)
                            .one();
                        Ok(zelf.code_tag_chain(code, &[(1, "not_found"), (2, "denied")], "io"))
                    },
                )?;
                Ok(Flow::Val(Some(out)))
            }
            "os_wait" => {
                let h = arg(0)?;
                let (region, slot) = self.rt_slot(8);
                let rc = self
                    .rt_call_slot("__wolf_rt_os_wait", &[h], slot, region, Some(types::I64))
                    .expect("rc");
                let z = self.b.iconst(types::I64, 0);
                let hit = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[rc, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Eq),
                    )
                    .one();
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |zelf| Ok(Some(zelf.load_flat(types::I64, slot, region, e.span)?)),
                    |zelf| Ok(zelf.code_tag_chain(rc, &[(3, "signal")], "io")),
                )?;
                Ok(Flow::Val(Some(out)))
            }
            "os_kill" => {
                let h = arg(0)?;
                let rc = self
                    .rt_call("__wolf_rt_os_kill", &[h], Some(types::I64))
                    .expect("rc");
                let z = self.b.iconst(types::I64, 0);
                let hit = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[rc, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Eq),
                    )
                    .one();
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |_| Ok(None),
                    |zelf| {
                        let id = zelf.b.module.tag_id("io");
                        Ok(zelf.b.iconst(types::I64, id))
                    },
                )?;
                Ok(Flow::Val(Some(out)))
            }
            _ => Err(refuse("this process builtin", e.span)),
        }
    }

    /// `str_from_utf8(b: List[int]) -> str ! {utf8}` (s81, wolf-lang#58)
    /// — the byte SOURCE s77 deliberately did not add, added the honest
    /// way.
    ///
    /// One call to `__wolf_rt_str_from_utf8` with the list header and a
    /// 16-byte out slot: code 0 hands back the materialized `{ptr, len}`
    /// pair, anything else becomes the `utf8` tag. The validation is the
    /// runtime's (`core::str::from_utf8`, the same reference the checked
    /// executor uses), the SPELLING of the failure is decided here — a
    /// row, never a trap — which is the same division of labour every
    /// other fallible builtin uses.
    ///
    /// The call goes through [`Self::rt_call_foreign`] because the shim
    /// READS foreign storage (the caller's list buffer) as well as
    /// writing the slot: no `data`/`len` the caller loaded may survive
    /// it.
    fn lower_str_from_utf8(&mut self, d: CallExpr<'t>, e: &'t GreenNode) -> R<Flow> {
        let mut argv: Vec<Value> = Vec::new();
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            let Some(vx) = Arg::value(a) else { continue };
            match self.lower_expr(vx)? {
                Flow::Val(Some(v)) => argv.push(v),
                Flow::Val(None) => return Err(refuse("a unit-typed byte list", vx.span)),
                Flow::Diverged => return Ok(Flow::Diverged),
            }
        }
        let Some(hdr) = argv.first().copied() else {
            return Err(refuse("`str_from_utf8` without its byte list", e.span));
        };
        let (region, slot) = self.rt_slot(16);
        let rc = self
            .rt_call_foreign(
                "__wolf_rt_str_from_utf8",
                &[hdr],
                Some((slot, region)),
                Some(types::I64),
            )
            .expect("rc");
        let z = self.b.iconst(types::I64, 0);
        let hit = self
            .b
            .ins(
                Opcode::Icmp,
                &[rc, z],
                &[types::BOOL],
                Aux::IntCc(IntCc::Eq),
            )
            .one();
        let eu = self.eu_ty_of(e.span)?;
        let out = self.eu_join(
            eu,
            hit,
            |zelf| Ok(Some(zelf.load_str_slot(slot, region, e.span)?)),
            |zelf| {
                let id = zelf.b.module.tag_id("utf8");
                Ok(zelf.b.iconst(types::I64, id))
            },
        )?;
        Ok(Flow::Val(Some(out)))
    }

    /// Map a runtime error code to a row tag through a branch chain:
    /// `pairs` are `(code, tag)` cases, `fallback` catches every other
    /// nonzero code — the compile-time-dispatched twin of
    /// [`Self::fs_code_tag`] for families whose codes are not the fs
    /// family's.
    fn code_tag_chain(&mut self, code: Value, pairs: &[(i64, &str)], fallback: &str) -> Value {
        let merge = self.b.create_block();
        let out = self.b.add_block_param(merge, types::I64);
        for (c, name) in pairs {
            let id = self.b.module.tag_id(name);
            let k = self.b.iconst(types::I64, *c);
            let eq = self
                .b
                .ins(
                    Opcode::Icmp,
                    &[code, k],
                    &[types::BOOL],
                    Aux::IntCc(IntCc::Eq),
                )
                .one();
            let tag = self.b.iconst(types::I64, id);
            let next = self.b.create_block();
            self.b.ins_br(eq, merge, &[tag], next, &[]);
            self.b.seal_block(next);
            self.b.switch_to_block(next);
        }
        let fb = self.b.module.tag_id(fallback);
        let fb_tag = self.b.iconst(types::I64, fb);
        self.b.ins_jmp(merge, &[fb_tag]);
        self.b.seal_block(merge);
        self.b.switch_to_block(merge);
        out
    }

    /// The s40 os/env and time builtin families, natively: one
    /// `wolf_rt::{os,time}` shim each, codes to row tags, str results
    /// through out slots — the fs pattern, family for family.
    /// `os_exit` calls the runtime exit and diverges (the block ends
    /// in an unreachable trap edge, the `[conf.trap.map]` residual
    /// spelling).
    fn lower_os_time_builtin(&mut self, name: &str, d: CallExpr<'t>, e: &'t GreenNode) -> R<Flow> {
        let mut argv: Vec<Value> = Vec::new();
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            let Some(vx) = Arg::value(a) else { continue };
            match self.lower_expr(vx)? {
                Flow::Val(Some(v)) => argv.push(v),
                Flow::Val(None) => return Err(refuse("unit-typed os/time arguments", vx.span)),
                Flow::Diverged => return Ok(Flow::Diverged),
            }
        }
        let arg = |i: usize| -> R<Value> {
            argv.get(i)
                .copied()
                .ok_or_else(|| refuse("an os/time call with missing arguments", e.span))
        };
        match name {
            "env_args" | "env_vars" => {
                let sym = if name == "env_args" {
                    "__wolf_rt_env_args"
                } else {
                    "__wolf_rt_env_vars"
                };
                let r = self.rt_call(sym, &[], Some(types::PTR)).expect("list");
                Ok(Flow::Val(Some(r)))
            }
            "env_get" => {
                let (np, nl) = {
                    let s = arg(0)?;
                    self.str_parts(s)
                };
                let (region, slot) = self.rt_slot(16);
                let rc = self
                    .rt_call_slot(
                        "__wolf_rt_env_get",
                        &[np, nl],
                        slot,
                        region,
                        Some(types::I64),
                    )
                    .expect("rc");
                let z = self.b.iconst(types::I64, 0);
                let hit = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[rc, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Eq),
                    )
                    .one();
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |z| Ok(Some(z.load_str_slot(slot, region, e.span)?)),
                    |z| Ok(z.code_tag_chain(rc, &[(2, "utf8")], "missing")),
                )?;
                Ok(Flow::Val(Some(out)))
            }
            "env_set" => {
                let (np, nl) = {
                    let s = arg(0)?;
                    self.str_parts(s)
                };
                let (vp, vl) = {
                    let s = arg(1)?;
                    self.str_parts(s)
                };
                let rc = self
                    .rt_call("__wolf_rt_env_set", &[np, nl, vp, vl], Some(types::I64))
                    .expect("rc");
                let z = self.b.iconst(types::I64, 0);
                let hit = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[rc, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Eq),
                    )
                    .one();
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |_| Ok(None),
                    |z| {
                        let id = z.b.module.tag_id("invalid");
                        Ok(z.b.iconst(types::I64, id))
                    },
                )?;
                Ok(Flow::Val(Some(out)))
            }
            "os_cwd" | "os_exe" => {
                let sym = if name == "os_cwd" {
                    "__wolf_rt_os_cwd"
                } else {
                    "__wolf_rt_os_exe"
                };
                let (region, slot) = self.rt_slot(16);
                let rc = self
                    .rt_call_slot(sym, &[], slot, region, Some(types::I64))
                    .expect("rc");
                let z = self.b.iconst(types::I64, 0);
                let hit = self
                    .b
                    .ins(
                        Opcode::Icmp,
                        &[rc, z],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Eq),
                    )
                    .one();
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |z| Ok(Some(z.load_str_slot(slot, region, e.span)?)),
                    |z| {
                        let id = z.b.module.tag_id("io");
                        Ok(z.b.iconst(types::I64, id))
                    },
                )?;
                Ok(Flow::Val(Some(out)))
            }
            "os_exit" => {
                let c = arg(0)?;
                self.rt_call("__wolf_rt_os_exit", &[c], None);
                // The shim never returns; the residual edge is the
                // sema-licensed unreachable spelling.
                self.b.ins_trap(TrapKind::Assert);
                Ok(Flow::Diverged)
            }
            "time_now_ms" | "time_unix_ms" => {
                let sym = if name == "time_now_ms" {
                    "__wolf_rt_time_now_ms"
                } else {
                    "__wolf_rt_time_unix_ms"
                };
                let r = self.rt_call(sym, &[], Some(types::I64)).expect("ms");
                Ok(Flow::Val(Some(r)))
            }
            "time_sleep_ms" => {
                let ms = arg(0)?;
                self.rt_call("__wolf_rt_time_sleep_ms", &[ms], None);
                Ok(Flow::Val(None))
            }
            _ => Err(refuse("this os/time builtin", e.span)),
        }
    }

    /// `s[range]` / `l[i]` (s40): the checked byte-slice and the
    /// bounds-trapping element read — same runtime entries as the
    /// recoverable forms, with the miss spelled `trap(bounds)`.
    fn lower_index(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = BracketApply::cast(e).ok_or_else(|| refuse("this index shape", e.span))?;
        let Some(recv) = d.callee() else {
            return Err(refuse("an index without a receiver", e.span));
        };
        let Some(base_sema) = self.expr_sema_ty(recv.span) else {
            return Err(refuse("an index without a recorded type", e.span));
        };
        // s77: `<str>.bytes()[i]` reads the receiver's storage directly
        // — s75's unsigned in-bounds test plus one `load.i8`, with no
        // list to materialize first.
        if let Some(src) = self.view_src(recv) {
            let Some((base, n)) = self.lower_view(src)? else {
                return Ok(Flow::Diverged);
            };
            let ix = d
                .args()
                .into_iter()
                .flat_map(|l| l.args())
                .filter_map(Arg::value)
                .next()
                .ok_or_else(|| refuse("an index without an operand", e.span))?;
            let Some(idx) = flow_val!(self.lower_expr(ix)) else {
                return Err(refuse("a valueless List index", ix.span));
            };
            if self.list_bounds_trap(idx, n) {
                return Ok(Flow::Diverged);
            }
            return Ok(Flow::Val(Some(self.bytes_load_at(base, idx))));
        }
        match self.table.kind(self.strip_sema(base_sema)) {
            TyKind::Prim(Prim::Str) => {
                let Some(sv) = flow_val!(self.lower_expr(recv)) else {
                    return Err(refuse("a valueless str receiver", recv.span));
                };
                let (sp, sl) = self.str_parts(sv);
                let rn = d
                    .args()
                    .into_iter()
                    .flat_map(|l| l.args())
                    .filter_map(Arg::value)
                    .find(|v| v.kind == SyntaxKind::RangeExpr)
                    .ok_or_else(|| refuse("str indexing outside range slices (D25)", e.span))?;
                let (lo, hi) = self.range_endpoints(rn, sl)?;
                // s77: the domain inline (no `str_get` call), the trap
                // spelling of the same test `get` reports as `{none}`.
                let hit = self.str_slice_domain(sp, sl, lo, hi);
                if self.trap_unless(hit, TrapKind::Bounds) {
                    return Ok(Flow::Diverged);
                }
                Ok(Flow::Val(Some(self.str_subslice(sp, lo, hi))))
            }
            TyKind::List(elem) => {
                let elem = *elem;
                self.refuse_region_elem(elem, e.span)?;
                let Some(ewty) = wir_ty(
                    &mut self.b.module.types,
                    self.table,
                    self.sigs,
                    elem,
                    e.span,
                )?
                else {
                    return Err(refuse("unit-typed List elements", e.span));
                };
                if flat_size(&self.b.module.types, ewty).is_none() {
                    return Err(refuse("List elements without a flat layout", e.span));
                }
                let Some(hdr) = flow_val!(self.lower_expr(recv)) else {
                    return Err(refuse("a valueless List receiver", recv.span));
                };
                let ix = d
                    .args()
                    .into_iter()
                    .flat_map(|l| l.args())
                    .filter_map(Arg::value)
                    .next()
                    .ok_or_else(|| refuse("an index without an operand", e.span))?;
                let Some(idx) = flow_val!(self.lower_expr(ix)) else {
                    return Err(refuse("a valueless List index", ix.span));
                };
                // s75: the check the caller owns, then a plain load.
                let n = self.list_len_of(hdr);
                if self.list_bounds_trap(idx, n) {
                    return Ok(Flow::Diverged);
                }
                Ok(Flow::Val(Some(self.list_load_at(hdr, idx, ewty, e.span)?)))
            }
            _ => Err(refuse(
                "indexing outside str/List (Pool/Map runtime shapes, c06/std)",
                e.span,
            )),
        }
    }

    /// `l[i] = v` (s40): the bounds-trapping element write.
    fn lower_index_assign(
        &mut self,
        d: AssignStmt<'t>,
        place: &'t GreenNode,
        span: Span,
    ) -> R<Flow> {
        let b = BracketApply::cast(place).ok_or_else(|| refuse("this index shape", span))?;
        let Some(recv) = b.callee() else {
            return Err(refuse("an index without a receiver", span));
        };
        let Some(base_sema) = self.expr_sema_ty(recv.span) else {
            return Err(refuse("an index without a recorded type", span));
        };
        // s77: a byte view has NO write path — `s.bytes()[i] = v` would
        // write a str's own bytes (rodata, for a literal) and could turn
        // a valid `str` into an invalid one. The refusal is the
        // enforcement; see the byte-view block.
        if self.view_src(recv).is_some() {
            return Err(refuse(
                "writing through a `bytes()` view (a byte view is read-only, s77)",
                span,
            ));
        }
        let TyKind::List(elem) = self.table.kind(self.strip_sema(base_sema)) else {
            return Err(refuse(
                "index writes outside List (raw-pointer writes c10; Pool/Map c06/std)",
                span,
            ));
        };
        let elem = *elem;
        self.refuse_region_elem(elem, span)?;
        let Some(ewty) = wir_ty(&mut self.b.module.types, self.table, self.sigs, elem, span)?
        else {
            return Err(refuse("unit-typed List elements", span));
        };
        if flat_size(&self.b.module.types, ewty).is_none() {
            return Err(refuse("List elements without a flat layout", span));
        }
        let op = d.op().map(|t| t.kind).unwrap_or(SyntaxKind::Eq);
        let Some(hdr) = flow_val!(self.lower_expr(recv)) else {
            return Err(refuse("a valueless List receiver", recv.span));
        };
        let ix = b
            .args()
            .into_iter()
            .flat_map(|l| l.args())
            .filter_map(Arg::value)
            .next()
            .ok_or_else(|| refuse("an index without an operand", span))?;
        let Some(idx) = flow_val!(self.lower_expr(ix)) else {
            return Err(refuse("a valueless List index", ix.span));
        };
        let Some(vx) = d.value() else {
            return Ok(Flow::Val(None));
        };
        let Some(rhs) = flow_val!(self.lower_expr(vx)) else {
            return Err(refuse("assignment of a valueless expression", vx.span));
        };
        // One bounds check for the whole statement: `l[i] op= v` reads
        // and writes the SAME element, so the read's check dominates
        // the write and a second one is provably redundant (GVN dedups
        // the compare; the branch folds). `l[i] = v` checks once by
        // construction.
        let n = self.list_len_of(hdr);
        if self.list_bounds_trap(idx, n) {
            return Ok(Flow::Diverged);
        }
        // `l[i] op= v` (#55): read-modify-write in place, X3-checked
        // at the element's sema type.
        let v = if op == SyntaxKind::Eq {
            rhs
        } else {
            let Some(bin) = Self::compound_bin(op) else {
                return Err(refuse("this compound assignment operator", span));
            };
            let cur = self.list_load_at(hdr, idx, ewty, span)?;
            let wrapping = matches!(self.table.kind(elem), TyKind::Wrapping(_));
            let unsigned = sema_unsigned(self.table, elem);
            match self.arith(bin, cur, rhs, wrapping, unsigned, ewty, span)? {
                Some(v) => v,
                None => return Ok(Flow::Diverged),
            }
        };
        self.list_store_at(hdr, idx, v, vx.span)?;
        Ok(Flow::Val(None))
    }

    /// `for pat in <List>` (s40, s75): a COUNTED loop over the backing
    /// storage — `len` once into the trip count, then `ptr.off` +
    /// `load` per iteration, each element bound by value. No iterator
    /// protocol, no call in the body, and no bounds check: the header
    /// test `i < n` IS the proof, so emitting one would be a check the
    /// lowering itself can discharge.
    fn lower_for_list(
        &mut self,
        d: ForExpr<'t>,
        iter: &'t GreenNode,
        elem_sema: TyId,
        span: Span,
    ) -> R<Flow> {
        let Some(ewty) = wir_ty(
            &mut self.b.module.types,
            self.table,
            self.sigs,
            elem_sema,
            span,
        )?
        else {
            return Err(refuse("unit-typed List elements", span));
        };
        if flat_size(&self.b.module.types, ewty).is_none() {
            return Err(refuse("List elements without a flat layout", span));
        }
        let Some(hdr) = flow_val!(self.lower_expr(iter)) else {
            return Err(refuse("a valueless List iterable", iter.span));
        };
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
        let n = self.list_len_of(hdr);
        let header = self.b.create_block();
        let iparam = self.b.add_block_param(header, types::I64);
        let zero = self.b.iconst(types::I64, 0);
        self.b.ins_jmp(header, &[zero]);
        self.b.switch_to_block(header);
        self.b.gvn_push_scope();
        let cond = self
            .b
            .ins(
                Opcode::Icmp,
                &[iparam, n],
                &[types::BOOL],
                Aux::IntCc(IntCc::Slt),
            )
            .one();
        let body_bb = self.b.create_block();
        let exit = self.b.create_block();
        self.b.ins_br(cond, body_bb, &[], exit, &[]);
        self.b.seal_block(body_bb);
        self.b.switch_to_block(body_bb);
        // The element: in-bounds by the header test, so no check. The
        // buffer pointer is re-read inside the body rather than
        // hoisted by hand — a body that cannot touch the list leaves
        // the load loop-invariant and LICM lifts it, and a body that
        // CAN would have invalidated a hoisted copy.
        let elem = self.list_load_at(hdr, iparam, ewty, span)?;
        let frame = self.run_for_body(d, elem, ewty, false, bind_name, Some(exit));
        let frame = match frame {
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
            let one = self.b.iconst(types::I64, 1);
            match self
                .b
                .ins(Opcode::IaddChk, &[iparam, one], &[types::I64], Aux::None)
            {
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

    // ------------------------------ the byte view (s77, #80) --------
    //
    // `s.bytes()` is a VIEW: the receiver's own `{ptr, len}` pair, the
    // same two words a `str` is and the same two words every zero-copy
    // subslice already was (`trim`, `get`, `strip_*`, the `split`
    // elements — the c08 design note promised one representation, and
    // this is it). No allocation, no header, no runtime call: element
    // `i` is `ptr.off` at stride 1 plus a `load.i8`, which is s75's
    // gep+load with the stride the bytes actually have. What it cost
    // before was eight heap bytes per input byte and a materializing
    // call (`__wolf_rt_str_bytes`), and s44/s75 measured family D at
    // 0.015x with that allocation inside every kernel.
    //
    // What the view CANNOT do, and how that is enforced:
    //
    // - It cannot be written, and three things say so independently.
    //   (1) There is no store path at all: the byte LOAD is the only
    //   access this block emits. (2) The surface already refuses to
    //   name a view as a mutable place — `s.bytes().push(1)` is E0804
    //   (`push` takes its receiver `mut`), `(mut s.bytes()).push(1)` is
    //   E1009 (a `mut` argument must name a place, and this is a
    //   temporary). (3) The guards below refuse the mutators anyway, so
    //   a future surface that DID admit those spellings cannot acquire
    //   a write path by default. A str's bytes are immutable at every
    //   tier — a literal's live in rodata — so a writable view would be
    //   both a soundness hole and a segfault.
    // - It cannot become a `str`. The view is bit-identical to a str
    //   pair, but nothing lowers a byte sequence BACK to a `str`: the
    //   only str-producing operations take a `str` receiver and go
    //   through `[mem.str.get]`'s boundary domain
    //   ([`Self::str_slice_domain`]). A `List[int] -> str` conversion
    //   would have to VALIDATE (wolf-std's `bytes.to_str`, still
    //   blocked, and this is why: it wants a checked primitive, not a
    //   cast).
    // - It cannot outlive its bytes any more than a subslice can: the
    //   view borrows exactly the storage `trim`/`split` elements
    //   already borrow, so there is one lifetime story here, not two.
    //
    // The view is never a first-class WIR value: lowering recognizes
    // `.bytes()` in the positions it consumes on the spot (iteration,
    // indexing, `len`/`count`/`is_empty`/`get`/`first`/`last`) and
    // reads the pair there. Every OTHER position — a `let` binding, an
    // argument, a return — still materializes through
    // `__wolf_rt_str_bytes`, bit-for-bit today's behavior, so no value
    // that a later consumer could mistake for a `str` (or for a `List`
    // header) ever escapes into the IR.

    /// The `str` receiver of a `<str>.bytes()` call — the syntactic
    /// recognition that decides a view, with no lowering side effects
    /// (the caller lowers the receiver only if it takes the view).
    fn bytes_view_recv(&self, e: &'t GreenNode) -> Option<&'t GreenNode> {
        if e.kind != SyntaxKind::CallExpr {
            return None;
        }
        let d = CallExpr::cast(e)?;
        let callee = d.callee()?;
        let m = wolf_ast::MemberExpr::cast(callee)?;
        if m.member().map(|t| self.text(t.span)).as_deref() != Some("bytes") {
            return None;
        }
        let base = m.base()?;
        let recv = if base.kind == SyntaxKind::ParenExpr {
            ParenExpr::cast(base).and_then(|p| p.expr()).unwrap_or(base)
        } else {
            base
        };
        let recv_sema = self.expr_sema_ty(recv.span)?;
        if !matches!(
            self.table.kind(self.strip_sema(recv_sema)),
            TyKind::Prim(Prim::Str)
        ) {
            return None;
        }
        Some(recv)
    }

    /// Lower the receiver of a recognized `.bytes()` call to its
    /// `{ptr, len}` halves — the view itself. `None` means the receiver
    /// DIVERGED (the caller's path is dead), never "not a view":
    /// [`Self::bytes_view_recv`] already decided that.
    fn lower_bytes_view(&mut self, recv: &'t GreenNode) -> R<Option<(Value, Value)>> {
        let sv = match self.lower_expr(recv)? {
            Flow::Val(Some(v)) => v,
            Flow::Val(None) => return Err(refuse("a valueless str receiver", recv.span)),
            Flow::Diverged => return Ok(None),
        };
        Ok(Some(self.str_parts(sv)))
    }

    /// s89 — where a byte view comes from in the position being lowered:
    /// the `.bytes()` call itself, or a PARAMETER that was lent one.
    /// The two are the same two words; the only difference is whether
    /// the receiver still needs lowering.
    fn view_src(&self, e: &'t GreenNode) -> Option<ViewSrc<'t>> {
        if let Some(recv) = self.bytes_view_recv(e) {
            return Some(ViewSrc::Recv(recv));
        }
        let e = if e.kind == SyntaxKind::ParenExpr {
            ParenExpr::cast(e).and_then(|p| p.expr()).unwrap_or(e)
        } else {
            e
        };
        if e.kind != SyntaxKind::PathExpr {
            return None;
        }
        match self.lookup(&self.text(e.span)) {
            Some(LocalBind::BytesView { ptr, len }) => Some(ViewSrc::Bound(ptr, len)),
            _ => None,
        }
    }

    /// The `{ptr, len}` of a recognized view. `None` = the receiver
    /// diverged (see [`Self::lower_bytes_view`]).
    fn lower_view(&mut self, src: ViewSrc<'t>) -> R<Option<(Value, Value)>> {
        match src {
            ViewSrc::Recv(recv) => self.lower_bytes_view(recv),
            ViewSrc::Bound(ptr, len) => Ok(Some((ptr, len))),
        }
    }

    /// Element `idx` of a byte view: `ptr.off` at stride 1, `load.i8`,
    /// widened to the `int` the checked lane's `Value::Int(b)` is. The
    /// zero-extension is what makes the byte unsigned — LLVM reads
    /// 0..=255 straight off the `zext`, so no range fact is needed to
    /// say what the op already says.
    ///
    /// The load rides the foreign BUFFER region, the same root s75's
    /// `List` element loads use. Not a third region on purpose: a str's
    /// bytes and a container's buffer are always distinct allocations
    /// at this tier, but "always distinct today" is not the theorem a
    /// separate region would claim (`region.foreign`'s own doc says
    /// two roots are a `!noalias` claim), and G7 is where earned
    /// disjointness lands.
    fn bytes_load_at(&mut self, base: Value, idx: Value) -> Value {
        let r = self.foreign_buf_region();
        let p = self.b.ins_ptr_off(base, idx, 1);
        let byte = self.b.ins_load(types::I8, p, r);
        self.b
            .ins(Opcode::Zext, &[byte], &[types::I64], Aux::None)
            .one()
    }

    /// `==` / `!=` on `str`, INLINE (s81, the last call out of family
    /// D's hot loop): a length guard, then a byte-at-a-time compare
    /// over the two operands' own storage — the s77 byte view's
    /// `ptr.off` + `load.i8`, read from two bases at once.
    /// `__wolf_rt_str_eq` was the one call left inside
    /// `d2_substr_search`'s loop after s77 inlined the slice; the A/B
    /// that replaced it with a byte compare measured 45.8 → 0.91
    /// ns/byte (50x, ~0.70x of C), so this is a shape with a number
    /// behind it, not a guess.
    ///
    /// `want_eq` picks which answer each exit carries, so `!=` costs
    /// exactly what `==` costs: the two constants swap places and no
    /// negation block is emitted (WIR has no boolean-not op).
    ///
    /// The trip count is the RIGHT operand's length on purpose. The
    /// guard has already proved the two lengths equal, and the right
    /// operand is the interned literal at every `match` arm and at
    /// nearly every source-level `==`, so the loop LLVM sees there has
    /// a CONSTANT bound and unrolls into a straight-line compare.
    ///
    /// Why not a word-at-a-time compare INLINE: WIR loads carry NATURAL
    /// alignment (`emit.rs` writes `align 8` for an `i64`), and a str's
    /// bytes have no alignment guarantee — every zero-copy subslice
    /// starts wherever its receiver's code points do. An unaligned wide
    /// load needs an alignment concept WIR does not have yet, so the
    /// wide path is the runtime's `memcmp`, taken past
    /// [`STR_EQ_INLINE_MAX`].
    ///
    /// There is no EMITTED pointer-identity shortcut: `icmp` is
    /// integer-only by the verifier's rule, so `ap == bp` would need a
    /// pointer compare added to the IR surface (verifier, both
    /// backends, the roundtrip fuzzer). The build-time half of it is
    /// free, though — when the two bases are the same WIR VALUE (two
    /// uses of one interned literal, or a str compared with itself),
    /// equality IS the length test, and that case returns without
    /// reading a byte. Empty-vs-empty needs neither: two zero lengths
    /// pass the guard and the loop runs zero times.
    ///
    /// The length guard can DECIDE at build time (`"wolf" == "wolves"`
    /// interns two constant lengths), and a decided guard is asked
    /// BEFORE its block exists: `ins_br` on a constant condition emits
    /// a plain jump, so a block created for the arm that lost would sit
    /// predecessorless — the shape Braun's `use_var` panics on when a
    /// later byte load walks its memory token back through it.
    fn str_eq_inline(
        &mut self,
        ap: Value,
        al: Value,
        bp: Value,
        bl: Value,
        want_eq: bool,
    ) -> Value {
        // Different lengths, different strings — one compare, and it is
        // the whole answer at a `match` arm whose literal is a
        // different width than the scrutinee.
        let len_eq = self
            .b
            .ins(
                Opcode::Icmp,
                &[al, bl],
                &[types::BOOL],
                Aux::IntCc(IntCc::Eq),
            )
            .one();
        if let Some(c) = self.b.as_bool_const(len_eq)
            && (!c || ap == bp)
        {
            // Decided outright: unequal lengths, or equal lengths over
            // the same storage.
            self.b.stats.fold += 1;
            return self.b.bconst(c == want_eq);
        }
        let merge = self.b.create_block();
        let out = self.b.add_block_param(merge, types::BOOL);
        // `yes` is what an EQUAL verdict carries out; `!=` swaps them.
        let yes = self.b.bconst(want_eq);
        let no = self.b.bconst(!want_eq);
        // Same storage, same bytes: equality is exactly the length
        // test, decided without a scan.
        if ap == bp {
            self.b.stats.fold += 1;
            self.b.ins_br(len_eq, merge, &[yes], merge, &[no]);
            self.b.seal_block(merge);
            self.b.switch_to_block(merge);
            return out;
        }
        if self.b.as_bool_const(len_eq).is_none() {
            let same_len = self.b.create_block();
            self.b.ins_br(len_eq, same_len, &[], merge, &[no]);
            self.b.seal_block(same_len);
            self.b.switch_to_block(same_len);
        }
        self.b.gvn_push_scope();
        // The guard has proved the two lengths equal, so the trip
        // count and the long test may read EITHER; take the one that
        // is a build-time constant, right operand first (the interned
        // literal at every `match` arm). The left arises when a
        // constant-width slice meets an unknown operand —
        // `hay[i..i + 5] == needle` — where the width folds to 5 (the
        // isub-over-iadd identity) and a constant bound is what lets
        // LLVM unroll the scan into the straight-line compare the s81
        // A/B measured. With neither constant the right operand
        // stands, as before.
        let bl = if self.b.as_int_const(bl).is_none() && self.b.as_int_const(al).is_some() {
            al
        } else {
            bl
        };
        // The long-operand route ([`STR_EQ_INLINE_MAX`]). The byte loop
        // runs at ~1.45 ns/byte and `memcmp` at ~0.012 (measured, s81:
        // 4 KiB operands, this host, LLVM tier), so past a few dozen
        // bytes the call is two orders of magnitude cheaper and the
        // honest lowering takes it. A KNOWN length settles the test
        // without emitting the constant at all — every `match` arm and
        // every literal compare — so the dispatch path this sprint
        // exists for emits no call, not even as dead code.
        let long = match self.b.as_int_const(bl) {
            Some(n) => self.b.bconst(n > STR_EQ_INLINE_MAX),
            None => {
                let k = self.b.iconst(types::I64, STR_EQ_INLINE_MAX);
                self.b
                    .ins(
                        Opcode::Icmp,
                        &[bl, k],
                        &[types::BOOL],
                        Aux::IntCc(IntCc::Sgt),
                    )
                    .one()
            }
        };
        match self.b.as_bool_const(long) {
            // Provably short: no call, no branch, straight to the scan.
            Some(false) => {}
            // Provably long: the scan is dead, so it is not built.
            Some(true) => {
                let rc = self
                    .rt_call("__wolf_rt_str_eq", &[ap, al, bp, bl], Some(types::I64))
                    .expect("rc");
                let hit = self.nonzero(rc);
                self.b.ins_br(hit, merge, &[yes], merge, &[no]);
                self.b.gvn_pop_scope();
                self.b.seal_block(merge);
                self.b.switch_to_block(merge);
                return out;
            }
            None => {
                let wide = self.b.create_block();
                let narrow = self.b.create_block();
                self.b.ins_br(long, wide, &[], narrow, &[]);
                self.b.seal_block(wide);
                self.b.switch_to_block(wide);
                self.b.gvn_push_scope();
                let rc = self
                    .rt_call("__wolf_rt_str_eq", &[ap, al, bp, bl], Some(types::I64))
                    .expect("rc");
                let hit = self.nonzero(rc);
                self.b.ins_br(hit, merge, &[yes], merge, &[no]);
                self.b.gvn_pop_scope();
                self.b.seal_block(narrow);
                self.b.switch_to_block(narrow);
            }
        }
        let zero = self.b.iconst(types::I64, 0);
        let header = self.b.create_block();
        let i = self.b.add_block_param(header, types::I64);
        self.b.ins_jmp(header, &[zero]);
        self.b.switch_to_block(header);
        let more = self
            .b
            .ins(
                Opcode::Icmp,
                &[i, bl],
                &[types::BOOL],
                Aux::IntCc(IntCc::Slt),
            )
            .one();
        let body = self.b.create_block();
        self.b.ins_br(more, body, &[], merge, &[yes]);
        self.b.seal_block(body);
        self.b.switch_to_block(body);
        let x = self.bytes_load_at(ap, i);
        let y = self.bytes_load_at(bp, i);
        let same = self
            .b
            .ins(Opcode::Icmp, &[x, y], &[types::BOOL], Aux::IntCc(IntCc::Eq))
            .one();
        let latch = self.b.create_block();
        self.b.ins_br(same, latch, &[], merge, &[no]);
        self.b.seal_block(latch);
        self.b.switch_to_block(latch);
        let one = self.b.iconst(types::I64, 1);
        // `.wrap`, not `.chk`: the index is bounded by a str's byte
        // length, which is an `i64` the allocator already handed out —
        // a checked add here would be a trap edge no execution reaches
        // and a block the mid-end would then have to prune.
        let next = self
            .b
            .ins(Opcode::IaddWrap, &[i, one], &[types::I64], Aux::None)
            .one();
        self.b.ins_jmp(header, &[next]);
        self.b.seal_block(header);
        self.b.gvn_pop_scope();
        self.b.seal_block(merge);
        self.b.switch_to_block(merge);
        out
    }

    /// `< <= > >=` on `str`, INLINE (s84, wolf-lang#94) —
    /// `[mem.str.order]`'s byte-lexicographic order without a call.
    /// s81 named equality only and left the relational family on
    /// `__wolf_rt_str_cmp`; this is the same machinery one step
    /// further, and the rest of that function's reasoning (why the byte
    /// loop and not a word-at-a-time compare, why the call above
    /// [`STR_EQ_INLINE_MAX`]) applies here unchanged.
    ///
    /// The shape is two verdicts and no three-way value, because WIR
    /// has no select and a `-1/0/1` intermediate would only be
    /// re-branched on:
    ///
    /// - the first position where the two strings DIFFER decides, by
    ///   an unsigned byte compare — and there the strict and non-strict
    ///   forms agree, so `<=` costs exactly what `<` costs;
    /// - a shared prefix all the way to `min(len)` leaves the LENGTHS
    ///   to decide, under the operator's own condition. "Shorter first
    ///   on a shared prefix" is not a special case here: it is what
    ///   `al < bl` says.
    ///
    /// Bytes reach the compare through `zext`, so they are 0..=255 and
    /// signed and unsigned compares agree on them — which is why the
    /// unsigned order `[mem.str.order]` demands needs no `u*`
    /// condition to express.
    ///
    /// Two build-time decisions cost nothing at run time: comparing a
    /// value with ITSELF (`ap == bp`) makes the shared prefix equal by
    /// construction, so the answer is the length compare with no bytes
    /// read; and two constant lengths settle `min` without emitting the
    /// diamond.
    fn str_cmp_inline(&mut self, ap: Value, al: Value, bp: Value, bl: Value, cc: IntCc) -> Value {
        let icmp = |z: &mut Self, cc: IntCc, a: Value, b: Value| {
            z.b.ins(Opcode::Icmp, &[a, b], &[types::BOOL], Aux::IntCc(cc))
                .one()
        };
        // Same storage, same shared prefix: only the lengths are left.
        if ap == bp {
            self.b.stats.fold += 1;
            return icmp(self, cc, al, bl);
        }
        // At a differing byte the strict form is the whole answer.
        let byte_cc = match cc {
            IntCc::Slt | IntCc::Sle => IntCc::Slt,
            _ => IntCc::Sgt,
        };
        // `min(al, bl)`: the length of the shared prefix to scan. Two
        // constant lengths — or one length VALUE used twice, which is
        // every `s.len`-derived compare — settle it without a diamond,
        // and the second case has to: a merge whose two edges carry one
        // value is the trivial phi the builder rejects.
        let m = match (self.b.as_int_const(al), self.b.as_int_const(bl)) {
            _ if al == bl => {
                self.b.stats.fold += 1;
                al
            }
            (Some(x), Some(y)) => {
                self.b.stats.fold += 1;
                self.b.iconst(types::I64, x.min(y))
            }
            _ => {
                let shorter = self.b.create_block();
                let mp = self.b.add_block_param(shorter, types::I64);
                let lt = icmp(self, IntCc::Slt, al, bl);
                // Both arms land in the same block, so a decided
                // condition still leaves it with a predecessor.
                self.b.ins_br(lt, shorter, &[al], shorter, &[bl]);
                self.b.seal_block(shorter);
                self.b.switch_to_block(shorter);
                mp
            }
        };
        let merge = self.b.create_block();
        let out = self.b.add_block_param(merge, types::BOOL);
        self.b.gvn_push_scope();
        // Past [`STR_EQ_INLINE_MAX`] the runtime's `memcmp`-backed
        // compare wins by the margin s81 measured for equality, and the
        // honest lowering takes it.
        let long = match self.b.as_int_const(m) {
            Some(n) => self.b.bconst(n > STR_EQ_INLINE_MAX),
            None => {
                let k = self.b.iconst(types::I64, STR_EQ_INLINE_MAX);
                icmp(self, IntCc::Sgt, m, k)
            }
        };
        let call_cmp = |z: &mut Self| {
            let rc = z
                .rt_call("__wolf_rt_str_cmp", &[ap, al, bp, bl], Some(types::I64))
                .expect("rc");
            let zero = z.b.iconst(types::I64, 0);
            let r = icmp(z, cc, rc, zero);
            z.b.ins_jmp(merge, &[r]);
        };
        match self.b.as_bool_const(long) {
            // Provably short: no call, no branch, straight to the scan.
            Some(false) => {}
            // Provably long: the scan is dead, so it is not built.
            Some(true) => {
                call_cmp(self);
                self.b.gvn_pop_scope();
                self.b.seal_block(merge);
                self.b.switch_to_block(merge);
                return out;
            }
            None => {
                let wide = self.b.create_block();
                let narrow = self.b.create_block();
                self.b.ins_br(long, wide, &[], narrow, &[]);
                self.b.seal_block(wide);
                self.b.switch_to_block(wide);
                self.b.gvn_push_scope();
                call_cmp(self);
                self.b.gvn_pop_scope();
                self.b.seal_block(narrow);
                self.b.switch_to_block(narrow);
            }
        }
        let zero = self.b.iconst(types::I64, 0);
        let one = self.b.iconst(types::I64, 1);
        let header = self.b.create_block();
        let i = self.b.add_block_param(header, types::I64);
        self.b.ins_jmp(header, &[zero]);
        self.b.switch_to_block(header);
        let more = icmp(self, IntCc::Slt, i, m);
        let body = self.b.create_block();
        let tail = self.b.create_block();
        self.b.ins_br(more, body, &[], tail, &[]);
        // The prefix ran out: the lengths decide, under the operator's
        // own condition.
        self.b.gvn_push_scope();
        self.b.seal_block(tail);
        self.b.switch_to_block(tail);
        let by_len = icmp(self, cc, al, bl);
        self.b.ins_jmp(merge, &[by_len]);
        self.b.gvn_pop_scope();
        self.b.seal_block(body);
        self.b.switch_to_block(body);
        let x = self.bytes_load_at(ap, i);
        let y = self.bytes_load_at(bp, i);
        let same = icmp(self, IntCc::Eq, x, y);
        let latch = self.b.create_block();
        let diff = self.b.create_block();
        self.b.ins_br(same, latch, &[], diff, &[]);
        self.b.gvn_push_scope();
        self.b.seal_block(diff);
        self.b.switch_to_block(diff);
        let by_byte = icmp(self, byte_cc, x, y);
        self.b.ins_jmp(merge, &[by_byte]);
        self.b.gvn_pop_scope();
        self.b.seal_block(latch);
        self.b.switch_to_block(latch);
        // `.wrap`: the index is bounded by a str's byte length, an
        // `i64` the allocator already handed out (the `str_eq_inline`
        // argument, verbatim).
        let next = self
            .b
            .ins(Opcode::IaddWrap, &[i, one], &[types::I64], Aux::None)
            .one();
        self.b.ins_jmp(header, &[next]);
        self.b.seal_block(header);
        self.b.gvn_pop_scope();
        self.b.seal_block(merge);
        self.b.switch_to_block(merge);
        out
    }

    /// `for b in <str>.bytes()` — a counted loop over the receiver's
    /// own bytes: `len` once into the trip count, then one `load.i8`
    /// per iteration. No iterator protocol, no allocation, no call, and
    /// no bounds check (the header test `i < n` IS the proof) — the
    /// `lower_for_list` shape at stride 1.
    fn lower_for_bytes(&mut self, d: ForExpr<'t>, base: Value, n: Value) -> R<Flow> {
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
        let header = self.b.create_block();
        let iparam = self.b.add_block_param(header, types::I64);
        let zero = self.b.iconst(types::I64, 0);
        self.b.ins_jmp(header, &[zero]);
        self.b.switch_to_block(header);
        self.b.gvn_push_scope();
        let cond = self
            .b
            .ins(
                Opcode::Icmp,
                &[iparam, n],
                &[types::BOOL],
                Aux::IntCc(IntCc::Slt),
            )
            .one();
        let body_bb = self.b.create_block();
        let exit = self.b.create_block();
        self.b.ins_br(cond, body_bb, &[], exit, &[]);
        self.b.seal_block(body_bb);
        self.b.switch_to_block(body_bb);
        let elem = self.bytes_load_at(base, iparam);
        let frame = self.run_for_body(d, elem, types::I64, false, bind_name, Some(exit));
        let frame = match frame {
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
            let one = self.b.iconst(types::I64, 1);
            match self
                .b
                .ins(Opcode::IaddChk, &[iparam, one], &[types::I64], Aux::None)
            {
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

    /// The byte-view spelling of the `List` methods lowering hands a
    /// view receiver: `len`/`count` is the pair's length half,
    /// `is_empty` one compare, `get`/`first`/`last` the s75 unsigned
    /// in-bounds test plus a byte load. The mutators get a refusal
    /// rather than an implementation: a view has no write path.
    fn lower_bytes_view_method(
        &mut self,
        d: CallExpr<'t>,
        base: Value,
        n: Value,
        mname: &str,
        e: &'t GreenNode,
    ) -> R<Flow> {
        match mname {
            "count" => Ok(Flow::Val(Some(n))),
            "is_empty" => {
                let z = self.b.iconst(types::I64, 0);
                Ok(Flow::Val(Some(
                    self.b
                        .ins(Opcode::Icmp, &[n, z], &[types::BOOL], Aux::IntCc(IntCc::Eq))
                        .one(),
                )))
            }
            "get" | "first" | "last" => {
                let given = match mname {
                    "get" => {
                        let ix = d
                            .args()
                            .into_iter()
                            .flat_map(|l| l.args())
                            .filter_map(Arg::value)
                            .next()
                            .ok_or_else(|| refuse("`get` without an index", e.span))?;
                        match self.lower_expr(ix)? {
                            Flow::Val(Some(v)) => Some(v),
                            _ => return Err(refuse("a valueless List index", ix.span)),
                        }
                    }
                    "first" => Some(self.b.iconst(types::I64, 0)),
                    _ => None,
                };
                let idx = match given {
                    Some(v) => v,
                    None => {
                        let one = self.b.iconst(types::I64, 1);
                        self.b
                            .ins(Opcode::IsubWrap, &[n, one], &[types::I64], Aux::None)
                            .one()
                    }
                };
                let hit = self.list_in_bounds(idx, n);
                let eu = self.eu_ty_of(e.span)?;
                let out = self.eu_join(
                    eu,
                    hit,
                    |z| Ok(Some(z.bytes_load_at(base, idx))),
                    |z| Ok(z.none_tag()),
                )?;
                Ok(Flow::Val(Some(out)))
            }
            // A view is READ-ONLY, and this is where that is enforced:
            // a mutator would write a str's bytes (rodata, for a
            // literal) and could forge an invalid `str` out of a valid
            // one. Refusing is the honest answer — the alternative is
            // materializing silently, which would make `push` write a
            // copy nobody can read.
            "push" | "pop" | "clear" => Err(refuse(
                "mutation through a `bytes()` view (a byte view is read-only, s77)",
                e.span,
            )),
            _ => Err(refuse("this List method (s05 std surface)", e.span)),
        }
    }

    // ------------------------- lazy str iteration (s84, #95) --------
    //
    // `words()`, `lines()` and `split()` are typed `List[str]`, and in
    // any position that needs a first-class list they still build one.
    // In a `for` head they build NOTHING: the loop walks the receiver's
    // own bytes and yields `{ptr, len}` pairs into it, exactly the s77
    // byte view one level up (`[mem.str.view]`). What that removes from
    // `word_count` is a `List[str]` header, a growable buffer, and one
    // 16-byte push per word — for a 1 MiB text, ~180k pushes.
    //
    // The three walks are the `wolf_rt::str` reference functions
    // (`words_of`, `lines_of`, and `find`-driven splitting) transcribed
    // into WIR, block for block, so the clause has one meaning and two
    // spellings rather than two behaviours.

    /// Which of the three lazy walks a `for` head named.
    ///
    /// `split` carries its separator expression; `words`/`lines` take
    /// no argument. Anything else — a wrong arity, a non-`str`
    /// receiver — is not recognized and falls through to the
    /// materializing `List` path, which is always correct.
    fn str_iter_recv(&self, e: &'t GreenNode) -> Option<StrIter<'t>> {
        if e.kind != SyntaxKind::CallExpr {
            return None;
        }
        let d = CallExpr::cast(e)?;
        let callee = d.callee()?;
        let m = wolf_ast::MemberExpr::cast(callee)?;
        let name = m.member().map(|t| self.text(t.span))?;
        let base = m.base()?;
        let recv = if base.kind == SyntaxKind::ParenExpr {
            ParenExpr::cast(base).and_then(|p| p.expr()).unwrap_or(base)
        } else {
            base
        };
        let recv_sema = self.expr_sema_ty(recv.span)?;
        if !matches!(
            self.table.kind(self.strip_sema(recv_sema)),
            TyKind::Prim(Prim::Str)
        ) {
            return None;
        }
        let args: Vec<&'t GreenNode> = d
            .args()
            .into_iter()
            .flat_map(|l| l.args())
            .filter_map(Arg::value)
            .collect();
        match (name.as_str(), args.len()) {
            ("words", 0) => Some(StrIter::Words { recv }),
            ("lines", 0) => Some(StrIter::Lines { recv }),
            ("split", 1) => Some(StrIter::Split { recv, sep: args[0] }),
            _ => None,
        }
    }

    /// `[mem.str.ws]`, inline: is a `White_Space` scalar encoded at
    /// `base + i`, and how wide is it? Answers `(bool, i64)` through a
    /// merge block, and leaves the builder in that merge.
    ///
    /// The shape is `wolf_rt::str::ws_at`'s, and the two are pinned
    /// equal over every scalar by that module's
    /// `ws_at_is_the_twenty_five_scalars`.
    ///
    /// **The hot path is the first three blocks.** A byte below 0x80 is
    /// a separator iff it is `0x20` or in `0x09..=0x0D`, which is two
    /// integer compares — `b == 0x20`, and `b - 9` unsigned-below-5 —
    /// and no memory access at all. That is the choice this sprint
    /// measured rather than assumed: over the `word_count` buffer the
    /// two compares ran at 0.491 ns/byte, a 256-byte lookup table at
    /// 1.003, and a 64-bit shift-mask at 0.513. The table loses because
    /// it turns a pure-ALU predicate into a dependent load per byte and
    /// stops the scan vectorizing, so the twenty-five scalars are
    /// spelled as tests, not indexed as data.
    ///
    /// Everything past `hi` is cold and never executes on ASCII text.
    /// Only four lead bytes can begin a separator (`C2`, `E1`, `E2`,
    /// `E3`), so two compares dismiss every other byte — including
    /// every continuation byte, which is what keeps the walks on
    /// code-point boundaries. Behind `E1/E2/E3` the three bytes are
    /// decoded to the actual scalar and tested as ONE 48-bit mask plus
    /// three equalities; a UTF-8 decode is cheaper here than nine
    /// byte-pattern branches, and it is what `[mem.str.ws]` is written
    /// in terms of.
    ///
    /// Reading `base[i+1]` and `base[i+2]` is in bounds because the
    /// lead byte says so: a `str` is valid UTF-8 by construction, so a
    /// `C2` has one continuation byte after it and an `E1/E2/E3` has
    /// two. The `b >= 0x80` test alone would NOT license the read — a
    /// str can end on a continuation byte — which is why the lead-byte
    /// tests come before the loads and not after.
    fn ws_at_inline(&mut self, base: Value, i: Value) -> (Value, Value) {
        // The scalars behind `E2`, as one mask over `cp - 0x2000`:
        // U+2000..U+200A (bits 0..10), U+2028, U+2029 (bits 40, 41) and
        // U+202F (bit 47). U+205F and the two singletons are equalities.
        const E2_MASK: i64 = (1 << 47) | (1 << 41) | (1 << 40) | 0x7FF;

        let merge = self.b.create_block();
        let ws = self.b.add_block_param(merge, types::BOOL);
        let width = self.b.add_block_param(merge, types::I64);
        let yes = self.b.bconst(true);
        let no = self.b.bconst(false);
        let one = self.b.iconst(types::I64, 1);
        let two = self.b.iconst(types::I64, 2);
        let three = self.b.iconst(types::I64, 3);
        let icmp = |z: &mut Self, cc: IntCc, a: Value, b: Value| {
            z.b.ins(Opcode::Icmp, &[a, b], &[types::BOOL], Aux::IntCc(cc))
                .one()
        };

        let b0 = self.bytes_load_at(base, i);
        let k80 = self.b.iconst(types::I64, 0x80);
        let hi = icmp(self, IntCc::Sge, b0, k80);
        let ascii = self.b.create_block();
        let multi = self.b.create_block();
        self.b.ins_br(hi, multi, &[], ascii, &[]);

        // Each arm below gets its OWN GVN scope, and that is load
        // bearing rather than tidy: the arms compute the same shapes at
        // different offsets (`i + 1` appears in the two-byte arm, the
        // three-byte arm and the callers' latches), and a value hashed
        // in one arm is not dominated by anything in a sibling — GVN
        // would hand it over and the verifier would reject the result.
        // Popping at each arm's end also keeps the merge continuation
        // clean, which is what lets a caller emit `i + 1` afterwards.

        // ---- ASCII: two compares, and this is where the time goes.
        self.b.gvn_push_scope();
        self.b.seal_block(ascii);
        self.b.switch_to_block(ascii);
        let k20 = self.b.iconst(types::I64, 0x20);
        let is_sp = icmp(self, IntCc::Eq, b0, k20);
        let ctl = self.b.create_block();
        self.b.ins_br(is_sp, merge, &[yes, one], ctl, &[]);
        self.b.seal_block(ctl);
        self.b.switch_to_block(ctl);
        let k9 = self.b.iconst(types::I64, 9);
        let d = self
            .b
            .ins(Opcode::IsubWrap, &[b0, k9], &[types::I64], Aux::None)
            .one();
        let k4 = self.b.iconst(types::I64, 4);
        let is_ctl = icmp(self, IntCc::Ule, d, k4);
        self.b.ins_jmp(merge, &[is_ctl, one]);
        self.b.gvn_pop_scope();

        // ---- non-ASCII: four lead bytes, everything else dismissed.
        self.b.gvn_push_scope();
        self.b.seal_block(multi);
        self.b.switch_to_block(multi);
        let kc2 = self.b.iconst(types::I64, 0xC2);
        let is_c2 = icmp(self, IntCc::Eq, b0, kc2);
        let two_bb = self.b.create_block();
        let e_bb = self.b.create_block();
        self.b.ins_br(is_c2, two_bb, &[], e_bb, &[]);

        // C2: U+0085 NEL and U+00A0 NO-BREAK SPACE.
        self.b.gvn_push_scope();
        self.b.seal_block(two_bb);
        self.b.switch_to_block(two_bb);
        let i1 = self
            .b
            .ins(Opcode::IaddWrap, &[i, one], &[types::I64], Aux::None)
            .one();
        let b1 = self.bytes_load_at(base, i1);
        let k85 = self.b.iconst(types::I64, 0x85);
        let is_nel = icmp(self, IntCc::Eq, b1, k85);
        let nbsp_bb = self.b.create_block();
        self.b.ins_br(is_nel, merge, &[yes, two], nbsp_bb, &[]);
        self.b.seal_block(nbsp_bb);
        self.b.switch_to_block(nbsp_bb);
        let ka0 = self.b.iconst(types::I64, 0xA0);
        let is_nbsp = icmp(self, IntCc::Eq, b1, ka0);
        self.b.ins_jmp(merge, &[is_nbsp, two]);
        self.b.gvn_pop_scope();

        // E1/E2/E3, or nothing at all.
        self.b.seal_block(e_bb);
        self.b.switch_to_block(e_bb);
        let ke1 = self.b.iconst(types::I64, 0xE1);
        let de = self
            .b
            .ins(Opcode::IsubWrap, &[b0, ke1], &[types::I64], Aux::None)
            .one();
        let k2 = self.b.iconst(types::I64, 2);
        let is_e = icmp(self, IntCc::Ule, de, k2);
        let three_bb = self.b.create_block();
        let none_bb = self.b.create_block();
        self.b.ins_br(is_e, three_bb, &[], none_bb, &[]);
        self.b.seal_block(none_bb);
        self.b.switch_to_block(none_bb);
        self.b.ins_jmp(merge, &[no, one]);

        // The three-byte forms, decoded to their scalar.
        self.b.gvn_push_scope();
        self.b.seal_block(three_bb);
        self.b.switch_to_block(three_bb);
        let i1b = self
            .b
            .ins(Opcode::IaddWrap, &[i, one], &[types::I64], Aux::None)
            .one();
        let i2v = self
            .b
            .ins(Opcode::IaddWrap, &[i, two], &[types::I64], Aux::None)
            .one();
        let c1 = self.bytes_load_at(base, i1b);
        let c2 = self.bytes_load_at(base, i2v);
        let k0f = self.b.iconst(types::I64, 0x0F);
        let k3f = self.b.iconst(types::I64, 0x3F);
        let k12 = self.b.iconst(types::I64, 12);
        let k6 = self.b.iconst(types::I64, 6);
        let hi4 = self
            .b
            .ins(Opcode::Band, &[b0, k0f], &[types::I64], Aux::None)
            .one();
        let hi4 = self
            .b
            .ins(Opcode::Shl, &[hi4, k12], &[types::I64], Aux::None)
            .one();
        let mid6 = self
            .b
            .ins(Opcode::Band, &[c1, k3f], &[types::I64], Aux::None)
            .one();
        let mid6 = self
            .b
            .ins(Opcode::Shl, &[mid6, k6], &[types::I64], Aux::None)
            .one();
        let lo6 = self
            .b
            .ins(Opcode::Band, &[c2, k3f], &[types::I64], Aux::None)
            .one();
        let cp = self
            .b
            .ins(Opcode::Bor, &[hi4, mid6], &[types::I64], Aux::None)
            .one();
        let cp = self
            .b
            .ins(Opcode::Bor, &[cp, lo6], &[types::I64], Aux::None)
            .one();
        let k2000 = self.b.iconst(types::I64, 0x2000);
        let off = self
            .b
            .ins(Opcode::IsubWrap, &[cp, k2000], &[types::I64], Aux::None)
            .one();
        let k2f = self.b.iconst(types::I64, 0x2F);
        // `off <=u 0x2F` also rejects every scalar BELOW U+2000: the
        // wrapping subtract makes those very large unsigned values.
        let in_win = icmp(self, IntCc::Ule, off, k2f);
        let mask_bb = self.b.create_block();
        let oth_bb = self.b.create_block();
        self.b.ins_br(in_win, mask_bb, &[], oth_bb, &[]);
        self.b.seal_block(mask_bb);
        self.b.switch_to_block(mask_bb);
        let mask = self.b.iconst(types::I64, E2_MASK);
        let sh = self
            .b
            .ins(Opcode::Lshr, &[mask, off], &[types::I64], Aux::None)
            .one();
        let bit = self
            .b
            .ins(Opcode::Band, &[sh, one], &[types::I64], Aux::None)
            .one();
        let in_set = self.nonzero(bit);
        self.b.ins_jmp(merge, &[in_set, three]);
        // U+1680, U+3000, U+205F — the three outside the window.
        self.b.seal_block(oth_bb);
        self.b.switch_to_block(oth_bb);
        let k1680 = self.b.iconst(types::I64, 0x1680);
        let is_ogham = icmp(self, IntCc::Eq, cp, k1680);
        let oth2 = self.b.create_block();
        self.b.ins_br(is_ogham, merge, &[yes, three], oth2, &[]);
        self.b.seal_block(oth2);
        self.b.switch_to_block(oth2);
        let k3000 = self.b.iconst(types::I64, 0x3000);
        let is_ideo = icmp(self, IntCc::Eq, cp, k3000);
        let oth3 = self.b.create_block();
        self.b.ins_br(is_ideo, merge, &[yes, three], oth3, &[]);
        self.b.seal_block(oth3);
        self.b.switch_to_block(oth3);
        let k205f = self.b.iconst(types::I64, 0x205F);
        let is_mmsp = icmp(self, IntCc::Eq, cp, k205f);
        self.b.ins_jmp(merge, &[is_mmsp, three]);
        self.b.gvn_pop_scope();
        self.b.gvn_pop_scope();

        self.b.seal_block(merge);
        self.b.switch_to_block(merge);
        (ws, width)
    }

    /// `for w in <str>.words()` — `[mem.str.words]` as two counted
    /// walks over the receiver's own bytes, and no allocation anywhere.
    ///
    /// ```text
    /// skip(i):   i < n ? [ws_at(i) ? skip(i + width) : scan(i)] : exit
    /// scan(j):   j < n ? [ws_at(j) ? emit(j) : scan(j + 1)] : emit(n)
    /// emit(e):   w = {base + i, e - i};  body;  skip(e)
    /// ```
    ///
    /// The two walks step DIFFERENTLY, and that is the whole safety
    /// argument. The skip walk steps by the separator's WIDTH, because
    /// stepping one byte would land inside a multi-byte space and open
    /// a word at a continuation byte — a `str` that splits a code
    /// point, which `[mem.str.get]` refuses to produce. The scan walk
    /// steps one byte, which is safe precisely because `ws_at` answers
    /// "no" for every continuation byte, so it can only ever stop on a
    /// boundary.
    ///
    /// A yielded word is never empty (`skip` has already run to a
    /// non-separator when `scan` starts), which is `[mem.str.words]`'s
    /// substance rather than an accident of the loop shape.
    fn lower_for_words(&mut self, d: ForExpr<'t>, base: Value, n: Value) -> R<Flow> {
        let bind_name = self.for_bind_name(d)?;
        let sty = str_ty(self.b.types());
        let exit = self.b.create_block();
        let zero = self.b.iconst(types::I64, 0);
        let one = self.b.iconst(types::I64, 1);

        let skip = self.b.create_block();
        let si = self.b.add_block_param(skip, types::I64);
        self.b.ins_jmp(skip, &[zero]);
        self.b.switch_to_block(skip);
        self.b.gvn_push_scope();
        let more = self
            .b
            .ins(
                Opcode::Icmp,
                &[si, n],
                &[types::BOOL],
                Aux::IntCc(IntCc::Slt),
            )
            .one();
        let probe = self.b.create_block();
        self.b.ins_br(more, probe, &[], exit, &[]);
        self.b.seal_block(probe);
        self.b.switch_to_block(probe);
        let (is_ws, width) = self.ws_at_inline(base, si);
        let adv = self.b.create_block();
        let word = self.b.create_block();
        self.b.ins_br(is_ws, adv, &[], word, &[]);
        // The advance block is a back edge: its own GVN scope, because
        // nothing downstream of `word` is dominated by it.
        self.b.gvn_push_scope();
        self.b.seal_block(adv);
        self.b.switch_to_block(adv);
        let next = self
            .b
            .ins(Opcode::IaddWrap, &[si, width], &[types::I64], Aux::None)
            .one();
        self.b.ins_jmp(skip, &[next]);
        self.b.gvn_pop_scope();

        // The word runs from `si` to the next separator (or the end).
        self.b.seal_block(word);
        self.b.switch_to_block(word);
        let scan = self.b.create_block();
        let ji = self.b.add_block_param(scan, types::I64);
        // `stop` takes NO parameter: both edges into it carry `ji`, the
        // scan header's own parameter, and a block param whose incoming
        // values are all one value is the trivial phi the builder's
        // postcondition rejects. `scan` dominates `stop`, so the value
        // is simply in scope.
        let stop = self.b.create_block();
        // The scan starts one byte PAST the word's first: `skip` has
        // already proved `si` in bounds and not a separator, so asking
        // again would be a whole predicate evaluation per word for an
        // answer we hold. Stepping into a multi-byte scalar's tail is
        // fine — continuation bytes are not separators, so the scan
        // runs through them exactly as it runs through any word byte.
        let first = self
            .b
            .ins(Opcode::IaddWrap, &[si, one], &[types::I64], Aux::None)
            .one();
        self.b.ins_jmp(scan, &[first]);
        self.b.switch_to_block(scan);
        self.b.gvn_push_scope();
        let inb = self
            .b
            .ins(
                Opcode::Icmp,
                &[ji, n],
                &[types::BOOL],
                Aux::IntCc(IntCc::Slt),
            )
            .one();
        let probe2 = self.b.create_block();
        self.b.ins_br(inb, probe2, &[], stop, &[]);
        self.b.seal_block(probe2);
        self.b.switch_to_block(probe2);
        let (is_ws2, _) = self.ws_at_inline(base, ji);
        let step = self.b.create_block();
        self.b.ins_br(is_ws2, stop, &[], step, &[]);
        self.b.gvn_push_scope();
        self.b.seal_block(step);
        self.b.switch_to_block(step);
        let nj = self
            .b
            .ins(Opcode::IaddWrap, &[ji, one], &[types::I64], Aux::None)
            .one();
        self.b.ins_jmp(scan, &[nj]);
        self.b.gvn_pop_scope();
        self.b.seal_block(scan);
        self.b.gvn_pop_scope();

        self.b.seal_block(stop);
        self.b.switch_to_block(stop);
        let elem = self.str_subslice(base, si, ji);
        let frame = match self.run_for_body(d, elem, sty, false, bind_name, Some(exit)) {
            Ok(f) => f,
            Err(x) => {
                self.b.gvn_pop_scope();
                return Err(x);
            }
        };
        if let ContinueTo::ForLatch(Some(latch)) = frame.continue_to {
            self.b.seal_block(latch);
            self.b.switch_to_block(latch);
            self.b.ins_jmp(skip, &[ji]);
        }
        self.b.gvn_pop_scope();
        self.b.seal_block(skip);
        self.b.seal_block(exit);
        self.b.switch_to_block(exit);
        Ok(Flow::Val(None))
    }

    /// `for l in <str>.lines()` — `[mem.str.lines]`: scan to the next
    /// LF, hand back everything before it, and absorb one CR that
    /// immediately preceded that LF.
    ///
    /// ```text
    /// head(i):  i < n ? find(i) : exit
    /// find(j):  j < n ? [b[j] == LF ? at(j) : find(j + 1)] : at(n)
    /// at(j):    e = (j < n && j > i && b[j-1] == CR) ? j - 1 : j
    /// emit(e):  l = {base + i, e - i};  body;  head(j + 1)
    /// ```
    ///
    /// `j < n` is exactly the "an LF terminated this line" test, and it
    /// is why `"a\r"` yields `"a\r"` while `"a\r\n"` yields `"a"`: a CR
    /// is part of the terminator only when a terminator followed it.
    /// The resume offset `j + 1` steps past the LF, and when `j == n`
    /// it steps past the end — which is how a trailing LF opens no
    /// final empty line and an empty receiver yields nothing at all.
    fn lower_for_lines(&mut self, d: ForExpr<'t>, base: Value, n: Value) -> R<Flow> {
        let bind_name = self.for_bind_name(d)?;
        let sty = str_ty(self.b.types());
        let exit = self.b.create_block();
        let zero = self.b.iconst(types::I64, 0);
        let one = self.b.iconst(types::I64, 1);
        let lf = self.b.iconst(types::I64, 0x0A);
        let cr = self.b.iconst(types::I64, 0x0D);
        let icmp = |z: &mut Self, cc: IntCc, a: Value, b: Value| {
            z.b.ins(Opcode::Icmp, &[a, b], &[types::BOOL], Aux::IntCc(cc))
                .one()
        };

        let head = self.b.create_block();
        let ip = self.b.add_block_param(head, types::I64);
        self.b.ins_jmp(head, &[zero]);
        self.b.switch_to_block(head);
        self.b.gvn_push_scope();
        let more = icmp(self, IntCc::Slt, ip, n);
        let find = self.b.create_block();
        let jp = self.b.add_block_param(find, types::I64);
        self.b.ins_br(more, find, &[ip], exit, &[]);
        self.b.switch_to_block(find);
        self.b.gvn_push_scope();
        let inb = icmp(self, IntCc::Slt, jp, n);
        let probe = self.b.create_block();
        // `at` takes no parameter: both edges carry `jp`, the find
        // header's own, and `find` dominates `at` — a param here would
        // be the trivial phi the builder's postcondition rejects.
        let at = self.b.create_block();
        let fp = jp;
        self.b.ins_br(inb, probe, &[], at, &[]);
        self.b.gvn_push_scope();
        self.b.seal_block(probe);
        self.b.switch_to_block(probe);
        let byte = self.bytes_load_at(base, jp);
        let is_lf = icmp(self, IntCc::Eq, byte, lf);
        let fstep = self.b.create_block();
        self.b.ins_br(is_lf, at, &[], fstep, &[]);
        self.b.seal_block(fstep);
        self.b.switch_to_block(fstep);
        let nj = self
            .b
            .ins(Opcode::IaddWrap, &[jp, one], &[types::I64], Aux::None)
            .one();
        self.b.ins_jmp(find, &[nj]);
        self.b.gvn_pop_scope();
        self.b.seal_block(find);
        self.b.gvn_pop_scope();

        // The end offset: `fp`, or one back when a CR sits under the LF.
        self.b.seal_block(at);
        self.b.switch_to_block(at);
        let emit = self.b.create_block();
        let eparam = self.b.add_block_param(emit, types::I64);
        let had_lf = icmp(self, IntCc::Slt, fp, n);
        let crchk = self.b.create_block();
        self.b.ins_br(had_lf, crchk, &[], emit, &[fp]);
        // The CR probe is a side arm: it does not dominate `emit`, and
        // it computes `fp - 1`, so it keeps its own GVN scope.
        self.b.gvn_push_scope();
        self.b.seal_block(crchk);
        self.b.switch_to_block(crchk);
        let nonempty = icmp(self, IntCc::Sgt, fp, ip);
        let crprobe = self.b.create_block();
        self.b.ins_br(nonempty, crprobe, &[], emit, &[fp]);
        self.b.seal_block(crprobe);
        self.b.switch_to_block(crprobe);
        let back = self
            .b
            .ins(Opcode::IsubWrap, &[fp, one], &[types::I64], Aux::None)
            .one();
        let prev = self.bytes_load_at(base, back);
        let is_cr = icmp(self, IntCc::Eq, prev, cr);
        self.b.ins_br(is_cr, emit, &[back], emit, &[fp]);
        self.b.gvn_pop_scope();

        self.b.seal_block(emit);
        self.b.switch_to_block(emit);
        let elem = self.str_subslice(base, ip, eparam);
        let frame = match self.run_for_body(d, elem, sty, false, bind_name, Some(exit)) {
            Ok(f) => f,
            Err(x) => {
                self.b.gvn_pop_scope();
                return Err(x);
            }
        };
        if let ContinueTo::ForLatch(Some(latch)) = frame.continue_to {
            self.b.seal_block(latch);
            self.b.switch_to_block(latch);
            let resume = self
                .b
                .ins(Opcode::IaddWrap, &[fp, one], &[types::I64], Aux::None)
                .one();
            self.b.ins_jmp(head, &[resume]);
        }
        self.b.gvn_pop_scope();
        self.b.seal_block(head);
        self.b.seal_block(exit);
        self.b.switch_to_block(exit);
        Ok(Flow::Val(None))
    }

    /// `for f in <str>.split(sep)` — `[mem.str.split]`: every field,
    /// empty ones included, driven by `__wolf_rt_str_find` over the
    /// unconsumed rest.
    ///
    /// ```text
    /// head(i):  sep empty ? emit(n, _, stop)
    ///                     : find(base + i, n - i, sep)
    ///                       hit < 0 ? emit(n, _, stop)
    ///                               : emit(i + hit, i + hit + |sep|, go)
    /// emit(e, k, c):  f = {base + i, e - i};  body;  c ? head(k) : exit
    /// ```
    ///
    /// The call stays, and stays deliberately: it is one call per
    /// FIELD, not per byte, and `wolf_rt`'s `find` is a real substring
    /// searcher — an inlined naive scan would be quadratic on the
    /// inputs a parser actually hands it. What the lazy shape removes
    /// is the `List[str]` the fields used to be pushed into, which is
    /// where the allocation was.
    ///
    /// A constant empty separator is answered without a loop at all:
    /// `[mem.str.empty]` makes it one field, the whole string, and
    /// emitting the loop would leave the search blocks with no
    /// predecessor once the guard folded.
    fn lower_for_split(
        &mut self,
        d: ForExpr<'t>,
        base: Value,
        n: Value,
        np: Value,
        nl: Value,
    ) -> R<Flow> {
        let bind_name = self.for_bind_name(d)?;
        let sty = str_ty(self.b.types());
        let exit = self.b.create_block();
        let zero = self.b.iconst(types::I64, 0);

        // `s.split("")` — one field, the whole string, no loop.
        if self.b.as_int_const(nl) == Some(0) {
            self.b.stats.fold += 1;
            let elem = self.str_subslice(base, zero, n);
            let frame = self.run_for_body(d, elem, sty, false, bind_name, Some(exit))?;
            if let ContinueTo::ForLatch(Some(latch)) = frame.continue_to {
                self.b.seal_block(latch);
                self.b.switch_to_block(latch);
                self.b.ins_jmp(exit, &[]);
            }
            self.b.seal_block(exit);
            self.b.switch_to_block(exit);
            return Ok(Flow::Val(None));
        }

        let go = self.b.bconst(true);
        let stop = self.b.bconst(false);
        let head = self.b.create_block();
        let ip = self.b.add_block_param(head, types::I64);
        self.b.ins_jmp(head, &[zero]);
        self.b.switch_to_block(head);
        self.b.gvn_push_scope();
        let emit = self.b.create_block();
        let eparam = self.b.add_block_param(emit, types::I64);
        let kparam = self.b.add_block_param(emit, types::I64);
        let cparam = self.b.add_block_param(emit, types::BOOL);
        let empty_sep = self
            .b
            .ins(
                Opcode::Icmp,
                &[nl, zero],
                &[types::BOOL],
                Aux::IntCc(IntCc::Eq),
            )
            .one();
        let search = self.b.create_block();
        self.b
            .ins_br(empty_sep, emit, &[n, zero, stop], search, &[]);
        // The search arm does not dominate `emit`: its own GVN scope.
        self.b.gvn_push_scope();
        self.b.seal_block(search);
        self.b.switch_to_block(search);
        let rest_p = self.b.ins_ptr_off(base, ip, 1);
        let rest_n = self
            .b
            .ins(Opcode::IsubWrap, &[n, ip], &[types::I64], Aux::None)
            .one();
        let fwd = self.b.iconst(types::I64, 0);
        let hit = self
            .rt_call(
                "__wolf_rt_str_find",
                &[rest_p, rest_n, np, nl, fwd],
                Some(types::I64),
            )
            .expect("rc");
        let miss = self
            .b
            .ins(
                Opcode::Icmp,
                &[hit, zero],
                &[types::BOOL],
                Aux::IntCc(IntCc::Slt),
            )
            .one();
        let mid = self.b.create_block();
        self.b.ins_br(miss, emit, &[n, zero, stop], mid, &[]);
        self.b.seal_block(mid);
        self.b.switch_to_block(mid);
        let end = self
            .b
            .ins(Opcode::IaddWrap, &[ip, hit], &[types::I64], Aux::None)
            .one();
        let resume = self
            .b
            .ins(Opcode::IaddWrap, &[end, nl], &[types::I64], Aux::None)
            .one();
        self.b.ins_jmp(emit, &[end, resume, go]);
        self.b.gvn_pop_scope();

        self.b.seal_block(emit);
        self.b.switch_to_block(emit);
        let elem = self.str_subslice(base, ip, eparam);
        let frame = match self.run_for_body(d, elem, sty, false, bind_name, Some(exit)) {
            Ok(f) => f,
            Err(x) => {
                self.b.gvn_pop_scope();
                return Err(x);
            }
        };
        if let ContinueTo::ForLatch(Some(latch)) = frame.continue_to {
            self.b.seal_block(latch);
            self.b.switch_to_block(latch);
            self.b.ins_br(cparam, head, &[kparam], exit, &[]);
        }
        self.b.gvn_pop_scope();
        self.b.seal_block(head);
        self.b.seal_block(exit);
        self.b.switch_to_block(exit);
        Ok(Flow::Val(None))
    }

    /// The `for` binding name, with the shared refusal for patterns the
    /// counted-loop shapes cannot destructure.
    fn for_bind_name(&self, d: ForExpr<'t>) -> R<Option<String>> {
        match d.pattern() {
            None => Ok(None),
            Some(p) if p.kind == SyntaxKind::IdentPat => Ok(Some(self.text(p.span))),
            Some(p) if p.kind == SyntaxKind::WildcardPat => Ok(None),
            Some(p) => Err(refuse(
                "destructuring `for` patterns (tuple yields, c06/std)",
                p.span,
            )),
        }
    }

    /// Build the `{ptr, len}` value of a byte-literal string: intern
    /// the bytes as module data, take their address, pair with the
    /// length. Zero-length literals intern one NUL byte (a zero-size
    /// data symbol is degenerate) but keep len 0.
    fn str_value(&mut self, bytes: &[u8]) -> Value {
        let (p, len) = self.str_literal_parts(bytes);
        let sty = str_ty(self.b.types());
        self.b
            .ins(Opcode::AggMake, &[p, len], &[sty], Aux::None)
            .one()
    }

    /// The `{ptr, len}` halves of a byte-literal string WITHOUT building
    /// the pair. `agg.get(agg.make(p, len), 1)` does not fold back to
    /// `len` at build time, so a caller that only wants the halves and
    /// goes through [`Self::str_value`] loses the length's constness —
    /// and with it the s81 threshold fold that keeps `match`-over-str
    /// dispatch call-free, and the constant trip count that lets LLVM
    /// unroll the compare.
    fn str_literal_parts(&mut self, bytes: &[u8]) -> (Value, Value) {
        let idx = self
            .b
            .module
            .intern_data(if bytes.is_empty() { &[0u8] } else { bytes });
        let p = self.b.ins_data_addr(idx);
        let len = self.b.iconst(types::I64, bytes.len() as i64);
        (p, len)
    }

    /// Cook a hole-free string EPISODE (a `StringLit` pattern node)
    /// into its runtime bytes: quote strip, the shared escape set,
    /// `"""` dedent — byte-identical with `string_segments`' literal
    /// walk and the checked executor's decoder (#54: str match arms
    /// compare these bytes at runtime).
    fn cooked_str_lit(&self, s: &'t GreenNode) -> Vec<u8> {
        let raw = self.text(s.span);
        let bytes = raw.as_bytes();
        if bytes.starts_with(b"\"\"\"") {
            let inner = &bytes[3..bytes.len().saturating_sub(3).max(3)];
            return decode_escapes(&dedent_multiline(inner));
        }
        // Raw literal (#76): the whole opening delimiter — `r"`,
        // `r#"`, … — strips, and the bytes between the fences ARE the
        // value ([gram.lex.str.raw]: no escapes, no interpolation).
        if let Some(inner) = raw_str_inner(bytes) {
            return inner.to_vec();
        }
        let inner = if bytes.len() >= 2 {
            &bytes[1..bytes.len() - 1]
        } else {
            bytes
        };
        decode_escapes(inner)
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
        // Raw literal (#76): strip the full `r#*"` delimiter pair and
        // take the inner bytes verbatim — no escapes, no interpolation
        // ([gram.lex.str.raw]; the lexer emits no `Interp` inside one,
        // so the holes list is empty here).
        if let Some(inner) = raw_str_inner(bytes) {
            if inner.is_empty() {
                return Vec::new();
            }
            return vec![StrSeg::Lit(inner.to_vec())];
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
        // D43: the statement's segments are ONE line. Every hole has
        // been evaluated by now, so nothing between the bracket calls
        // can trap and strand a half-written line — and the runtime
        // takes the stream lock once instead of once per segment.
        self.rt_print_call("__wolf_rt_print_begin", &[], &[]);
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
        let st = self.b.iconst(types::I64, stream);
        self.rt_print_call("__wolf_rt_print_end", &[types::I64], &[st]);
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
        self.check_capture_write(vexpr, "lending `mut`")?;
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
        // s40: `List[T]()` — the runtime header, sized by the
        // element's flat layout.
        if let TyKind::List(elem) = self.table.kind(ty) {
            let elem = *elem;
            self.refuse_region_elem(elem, e.span)?;
            let Some(ewty) = wir_ty(
                &mut self.b.module.types,
                self.table,
                self.sigs,
                elem,
                e.span,
            )?
            else {
                return Err(refuse("unit-typed List elements", e.span));
            };
            let Some(esize) = flat_size(&self.b.module.types, ewty) else {
                return Err(refuse("List elements without a flat layout", e.span));
            };
            let sz = self.b.iconst(types::I64, esize as i64);
            // Threads the foreign token: the fresh header is storage
            // compiled code will load from, so the chain must know.
            let hdr = self
                .rt_call_foreign("__wolf_rt_list_new", &[sz], None, Some(types::PTR))
                .expect("hdr");
            return Ok(Flow::Val(Some(hdr)));
        }
        // s73: `channel[T](n)` — the runtime channel; omitted capacity
        // is rendezvous ([conc.chan.default]).
        if let TyKind::Chan(_) = self.table.kind(ty) {
            let cap = match d
                .args()
                .into_iter()
                .flat_map(|l| l.args())
                .find_map(Arg::value)
            {
                Some(vexpr) => {
                    let Some(v) = flow_val!(self.lower_expr(vexpr)) else {
                        return Err(refuse("a unit-typed channel capacity", vexpr.span));
                    };
                    self.widen_to_wire(v, vexpr.span)?
                }
                None => self.b.iconst(types::I64, 0),
            };
            let h = self
                .rt_call("__wolf_rt_chan_new", &[cap], Some(types::PTR))
                .expect("chan handle");
            return Ok(Flow::Val(Some(h)));
        }
        // s73: `Mutex(v)` — a sync cell guarding one word
        // ([conc.when.body]).
        if let TyKind::Mutex(_) = self.table.kind(ty) {
            let init = match d
                .args()
                .into_iter()
                .flat_map(|l| l.args())
                .find_map(Arg::value)
            {
                Some(vexpr) => {
                    let Some(v) = flow_val!(self.lower_expr(vexpr)) else {
                        return Err(refuse("a unit-typed Mutex payload", vexpr.span));
                    };
                    self.widen_to_wire(v, vexpr.span)?
                }
                None => self.b.iconst(types::I64, 0),
            };
            let h = self
                .rt_call("__wolf_rt_sync_new", &[init], Some(types::PTR))
                .expect("sync handle");
            return Ok(Flow::Val(Some(h)));
        }
        if matches!(self.table.kind(ty), TyKind::Pool(_) | TyKind::Shared(_)) {
            return Err(refuse(
                "Pool/shared constructor lowering (runtime shapes, c06)",
                e.span,
            ));
        }
        let TyKind::Nominal { module, name, .. } = self.table.kind(ty) else {
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
        self.enum_value(sema_ty, index, &payloads, e.span)
    }

    /// The WIR value of an enum variant — shared by call-form
    /// construction ([`Self::lower_ctor`]) and the bare payload-free
    /// member form (`Hue.Red`, #23): tag alone for payload-free
    /// enums, else the aggregate with unfilled slots zeroed.
    fn enum_value(
        &mut self,
        sema_ty: TyId,
        index: usize,
        payloads: &[Value],
        span: Span,
    ) -> R<Flow> {
        let Some(wty) = wir_ty(
            &mut self.b.module.types,
            self.table,
            self.sigs,
            sema_ty,
            span,
        )?
        else {
            return Err(refuse("a unit-shaped enum", span));
        };
        let tag = self.b.iconst(types::I64, index as i64);
        if wty == types::I64 {
            // Payload-free enum: the tag IS the value.
            return Ok(Flow::Val(Some(tag)));
        }
        let types::TypeData::Agg(fields) = self.b.module.types.get(wty).clone() else {
            return Err(refuse("an enum without an aggregate shape", span));
        };
        let mut parts = vec![tag];
        parts.extend(payloads.iter().copied());
        for &fty in &fields[parts.len()..] {
            let z = self.zero_of(fty, span)?;
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
        // s95: static trait dispatch routes through the record the
        // checker already wrote — never re-derived (s18). s96: a dyn
        // record routes to the witness-table call — the receiver is
        // the member base, the args are the list.
        if let Dispatch::Trait {
            module,
            name,
            method,
            dyn_call: true,
        } = disp
        {
            let Some(callee) = d.callee() else {
                return Err(refuse("a method call without a callee", e.span));
            };
            let m = wolf_ast::MemberExpr::cast(callee)
                .ok_or_else(|| refuse("a method call without a receiver", e.span))?;
            let Some(base) = m.base() else {
                return Err(refuse("a method call without a receiver", e.span));
            };
            let args: Vec<_> = d.args().into_iter().flat_map(|l| l.args()).collect();
            return self.lower_dyn_trait_call(base, args, *module, name, method, e);
        }
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
        // s40: the builtin receivers — `str` (the s37 method set) and
        // `List` dispatch to the runtime tier, never to an impl.
        if let Some(recv_sema) = self.expr_sema_ty(recv_place.span) {
            let mname = m.member().map(|t| self.text(t.span)).unwrap_or_default();
            match self.table.kind(self.strip_sema(recv_sema)) {
                TyKind::Prim(Prim::Str) => {
                    return self.lower_str_method(d, recv_place, &mname, e);
                }
                TyKind::List(elem) => {
                    let elem = *elem;
                    // s77: `<str>.bytes().m(…)` answers from the view.
                    if let Some(src) = self.view_src(recv_place) {
                        let Some((base, n)) = self.lower_view(src)? else {
                            return Ok(Flow::Diverged);
                        };
                        return self.lower_bytes_view_method(d, base, n, &mname, e);
                    }
                    return self.lower_list_method(d, recv_place, elem, &mname, e);
                }
                // s73: the conc receivers dispatch to the runtime
                // seams, never to an impl.
                TyKind::Chan(elem) => {
                    let elem = *elem;
                    return self.lower_chan_method(d, recv_place, elem, &mname, e);
                }
                TyKind::TaskScope if mname == "spawn" => {
                    return self.lower_scope_spawn(d, recv_place, e);
                }
                TyKind::Proc => {
                    return self.lower_proc_method(d, recv_place, &mname, e);
                }
                TyKind::ExitReason => {
                    return self.lower_reason_method(recv_place, &mname, e);
                }
                _ => {}
            }
        }
        // The route: inherent methods through the type's own impl
        // (s27/s94); trait methods through the coherence-unique impl
        // the record names (s95) — overridden methods lower per impl,
        // a method the impl does not override runs the trait's DEFAULT
        // body as an instance (`Self ↦ subject`).
        let (base_name, msig, bindings): (String, &FnSig, Vec<(String, Bound)>) = match disp {
            Dispatch::Inherent { ty, method } => {
                // The method's signature, from the unique inherent impl
                // — and the impl itself (s94: a generic impl's rigids
                // bind from the receiver at this site).
                let (imp, msig): (&wolf_sema::traits::ImplDef, &FnSig) = self
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
                            .map(|mm| (i, &mm.sig))
                    })
                    .ok_or_else(|| refuse("a method call without an elaborated impl", e.span))?;
                // s94: a generic impl or a generic method is called as
                // an INSTANCE, the s93 free-fn rule — bind every rigid,
                // push the demand, call the instance.
                let bindings: Vec<(String, Bound)> =
                    if imp.generics.is_empty() && msig.generics.is_empty() {
                        Vec::new()
                    } else {
                        self.bind_method_generics(imp, msig, d, e)?
                    };
                (format!("{ty}.{method}"), msig, bindings)
            }
            Dispatch::Trait {
                module,
                name,
                method,
                ..
            } => {
                let Some(recv_ty) = self.expr_sema_ty(recv_place.span) else {
                    return Err(refuse(
                        "a trait method call without a typed receiver",
                        e.span,
                    ));
                };
                let stripped = self.strip_sema(recv_ty);
                let Some(head) = self_ty_key(self.table, stripped) else {
                    return Err(refuse_named(
                        format!("a `{name}` method on a non-nominal receiver"),
                        e.span,
                    ));
                };
                self.route_trait_static(*module, name, method, &head, d, e)?
            }
        };
        if msig.comptime {
            // Comptime method sites are not registered fold sites
            // (the s16 pass folds `CallExpr` spans only) — an honest
            // refusal until a consumer needs them.
            return Err(refuse(
                "comptime method calls (D29 CTFE owns these)",
                e.span,
            ));
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
                if recv_mode == Some(ParamMode::Take) {
                    self.check_capture_write(recv_place, "consuming (`take`)")?;
                }
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
            if mode == Some(ParamMode::Take) {
                self.check_capture_write(vexpr, "consuming (`take`)")?;
            }
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
        // The WIR callee is the mangled name the route chose — sema's
        // callee label may be the bare method name, and bare names
        // collide across types. Under a substitution the callee is the
        // INSTANCE: the s93 free-fn push, on the same worklist.
        let callee_name = if bindings.is_empty() {
            base_name
        } else {
            let key = SpecKey {
                mask: 0,
                subst: bindings
                    .iter()
                    .map(|(n, b)| {
                        let mut scratch = TypeTable::new();
                        let t = thaw(b, &mut scratch);
                        (n.clone(), mono_spelling(&scratch, t))
                    })
                    .collect(),
            };
            let full = spec_name(&base_name, &key);
            self.pending_specs.push(SpecRequest {
                name: base_name,
                key,
                bindings: bindings.clone(),
                span: e.span,
            });
            full
        };
        let ext = match self.callees.get(&callee_name) {
            Some(&ext) => ext,
            None => {
                let sig = if bindings.is_empty() {
                    wir_sig_of(self.b.module, self.sig_table, self.sigs, msig, 0, e.span)?
                } else {
                    // The import's signature is the instance's — the
                    // same substituted view the definition builds.
                    let mut st = self.sig_table.clone();
                    let map: std::collections::BTreeMap<String, TyId> = bindings
                        .iter()
                        .map(|(n, b)| (n.clone(), thaw(b, &mut st)))
                        .collect();
                    let mut f = msig.clone();
                    for p in &mut f.params {
                        p.ty = wolf_sema::types::subst(&mut st, p.ty, &map);
                    }
                    f.ret = wolf_sema::types::subst(&mut st, f.ret, &map);
                    wir_sig_of(self.b.module, &st, self.sigs, &f, 0, e.span)?
                };
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
            // s73: region-typed results are ptr handles (a recv'd
            // region unwrapping) — `wir_value_ty`'s rule.
            Some(t) => self.wir_value_ty(t, e.span)?.is_some(),
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
                SyntaxKind::PathPat => {
                    // `else |Tag(p)|` (s71, #43): sema proved the
                    // pattern covers the row entire (E0809), so no
                    // tag test is needed — the payload sub-patterns
                    // bind the eu slots directly.
                    let subs: Vec<&GreenNode> = p
                        .nodes()
                        .filter(|n| wolf_ast::is_pattern_kind(n.kind))
                        .collect();
                    if subs.len() != slots.len() {
                        self.scopes.pop();
                        return Err(refuse(
                            "payload arity in a handler pattern (checker contract)",
                            p.span,
                        ));
                    }
                    for (k, sub) in subs.iter().enumerate() {
                        match sub.kind {
                            SyntaxKind::IdentPat => {
                                let name = self.text(sub.span);
                                let pv = self.b.ins_eu_err_slot(v, k);
                                let fty = self.b.func.value_ty(pv);
                                let var = self.b.declare_var(fty);
                                self.b.def_var(var, pv);
                                self.scopes.last_mut().expect("scope").binds.push((
                                    name,
                                    LocalBind::Val {
                                        var,
                                        wrapping: false,
                                        unsigned: false,
                                        wir_ty: fty,
                                    },
                                ));
                            }
                            SyntaxKind::WildcardPat => {}
                            _ => {
                                self.scopes.pop();
                                return Err(refuse(
                                    "nested `else` handler payload patterns",
                                    sub.span,
                                ));
                            }
                        }
                    }
                }
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
            TyKind::Nominal { module, name, .. } => match self.sigs.get(*module as usize, name) {
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
            TyKind::Prim(Prim::Str) => Ok(MatchDomain::Str),
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
            MatchDomain::Scalar | MatchDomain::Str => None,
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
                // A str-literal arm (#54): the raw string episode is a
                // StringLit child; cook its bytes exactly as string
                // expressions cook (escapes, brace doubling, dedent).
                if let Some(s) = pat.nodes().find(|n| n.kind == SyntaxKind::StringLit) {
                    if s.tokens().any(|t| t.kind == SyntaxKind::InterpOpen) {
                        return Err(refuse("an interpolated string as a pattern", pat.span));
                    }
                    return Ok(PatShape::StrTests(vec![self.cooked_str_lit(s)]));
                }
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
                let mut strs: Vec<Vec<u8>> = Vec::new();
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
                        PatShape::StrTests(bs) => strs.extend(bs),
                        PatShape::BoolTest(_) => {
                            return Err(refuse("or-patterns over bool", alt.span));
                        }
                    }
                }
                if !strs.is_empty() {
                    if !consts.is_empty() {
                        return Err(refuse("mixed str/scalar or-patterns", pat.span));
                    }
                    return Ok(PatShape::StrTests(strs));
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
            // s73: region-typed results are ptr handles (a recv'd
            // region unwrapping) — `wir_value_ty`'s rule.
            Some(t) => self.wir_value_ty(t, e.span)?.is_some(),
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
                    // The arm constants live at the DISCRIMINANT's type,
                    // not `int`'s (s74, #67). A scalar scrutinee can be
                    // any integer width — a `for` induction variable, a
                    // narrow field, an enum tag — and `icmp` demands
                    // both operands share one type; emitting the test at
                    // a fixed i64 ICE'd the verifier. Sign-wrapping into
                    // that width is what literal *expressions* already
                    // do, so an unsigned pattern like `255` on a `u8`
                    // scrutinee becomes the same `-1` bit pattern the
                    // value carries — and the constant-fold path below
                    // compares against the same payload, which it did
                    // not before (it answered the wrong arm silently).
                    let dty = self.b.func.value_ty(disc);
                    let consts: Vec<i64> = match self.b.module.types.int_bits(dty) {
                        Some(bits) => consts.iter().map(|&c| wrap_bits(c as u64, bits)).collect(),
                        None => consts,
                    };
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
                            let cv = self.b.iconst(dty, c);
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
                PatShape::StrTests(cands) => {
                    // Dispatch-by-equality (#54, v0): each candidate is
                    // one INLINE str equality against the interned
                    // literal bytes (s81), chained in arm order. The
                    // literal's length is a constant here, so the
                    // length guard folds to a compare against a
                    // constant and the byte loop gets a constant trip
                    // count — a `match` on short literals unrolls.
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
                        continue;
                    }
                    let (sp, sl) = self.str_parts(sv);
                    let arm_bb = self.b.create_block();
                    let next_bb = self.b.create_block();
                    let mut chain_scopes = 0usize;
                    for (k, bytesc) in cands.iter().enumerate() {
                        let (cp, cl) = self.str_literal_parts(bytesc);
                        let t = self.str_eq_inline(sp, sl, cp, cl, true);
                        if k + 1 == cands.len() {
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
            // s77: `for b in <str>.bytes()` is a counted loop over the
            // receiver's own bytes — the view, never a materialized
            // list.
            if let Some(src) = self.view_src(iter) {
                let Some((base, n)) = self.lower_view(src)? else {
                    return Ok(Flow::Diverged);
                };
                return self.lower_for_bytes(d, base, n);
            }
            // s84: `for w in <str>.words()` — and the `lines`/`split`
            // spellings — walk the receiver's own bytes and yield
            // subslices of it ([mem.str.view]). No `List[str]`, no
            // push per element, no allocation at all.
            if let Some(kind) = self.str_iter_recv(iter) {
                let recv = match kind {
                    StrIter::Words { recv } | StrIter::Lines { recv } => recv,
                    StrIter::Split { recv, .. } => recv,
                };
                let sv = match self.lower_expr(recv)? {
                    Flow::Val(Some(v)) => v,
                    Flow::Val(None) => {
                        return Err(refuse("a valueless str receiver", recv.span));
                    }
                    Flow::Diverged => return Ok(Flow::Diverged),
                };
                let (base, n) = self.str_parts(sv);
                return match kind {
                    StrIter::Words { .. } => self.lower_for_words(d, base, n),
                    StrIter::Lines { .. } => self.lower_for_lines(d, base, n),
                    StrIter::Split { sep, .. } => {
                        let sepv = match self.lower_expr(sep)? {
                            Flow::Val(Some(v)) => v,
                            Flow::Val(None) => {
                                return Err(refuse("a valueless str separator", sep.span));
                            }
                            Flow::Diverged => return Ok(Flow::Diverged),
                        };
                        let (np, nl) = self.str_parts(sepv);
                        self.lower_for_split(d, base, n, np, nl)
                    }
                };
            }
            // s40: `for` over a List value (a `words()`/`lines()`/
            // `split()` result that had to MATERIALIZE, or any other
            // list) drives by index through the runtime.
            if let Some(it) = self.expr_sema_ty(iter.span)
                && let TyKind::List(elem) = self.table.kind(self.strip_sema(it))
            {
                let elem = *elem;
                self.refuse_region_elem(elem, e.span)?;
                return self.lower_for_list(d, iter, elem, e.span);
            }
            // s73: `for v in ch` drains to drained-close.
            if let Some(it) = self.expr_sema_ty(iter.span)
                && let TyKind::Chan(elem) = self.table.kind(self.strip_sema(it))
            {
                let elem = *elem;
                return self.lower_for_chan(d, iter, elem, e.span);
            }
            return Err(refuse(
                "`for` over non-range iterables (the `Iter[T]` drive loop — Pool adopts \
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

/// Is this call a task spawn — `<expr>.spawn(fn() { … })`?
///
/// A syntactic test on purpose: it runs before any lowering, so no
/// type is available. It over-approximates in exactly one direction
/// (some other `spawn` method taking a closure), and the cost of a
/// false positive is one unused arena, never a wrong program.
fn is_task_spawn_call(src: &[u8], n: &GreenNode) -> bool {
    if n.kind != SyntaxKind::CallExpr {
        return false;
    }
    let Some(d) = CallExpr::cast(n) else {
        return false;
    };
    let Some(callee) = d.callee() else {
        return false;
    };
    let Some(member) = wolf_ast::MemberExpr::cast(callee).and_then(|m| m.member()) else {
        return false;
    };
    let lo = member.span.lo as usize;
    let hi = member.span.hi as usize;
    if src.get(lo..hi) != Some(b"spawn") {
        return false;
    }
    d.args()
        .into_iter()
        .flat_map(|l| l.args())
        .filter_map(Arg::value)
        .any(|a| a.kind == SyntaxKind::ClosureExpr)
}

/// Does this scope body spawn a task from inside a loop?
///
/// The question decides where the capture records live (s86). Outside
/// a loop a spawn's env is a frame slot: the scope joins before the
/// frame dies, so the task's pointer is live for exactly as long as
/// the task is. Inside a loop that reasoning fails — one slot, N
/// tasks, all of them reading the last iteration's captures — so the
/// scope grows an arena and each iteration bump-allocates its own
/// record (contract target 2: "a capture record allocated in the
/// scope's region").
///
/// The scan does NOT stop at a nested `scope { }`: a spawn there may
/// still name THIS scope's handle, and the receiver decides which
/// arena it allocates in, not the lexical nesting. Over-minting costs
/// one unused arena; under-minting costs a refusal at the spawn.
fn spawns_under_a_loop(src: &[u8], body: &GreenNode) -> bool {
    fn any_spawn(src: &[u8], n: &GreenNode) -> bool {
        is_task_spawn_call(src, n) || n.nodes().any(|c| any_spawn(src, c))
    }
    fn walk(src: &[u8], n: &GreenNode) -> bool {
        if matches!(
            n.kind,
            SyntaxKind::WhileExpr | SyntaxKind::LoopExpr | SyntaxKind::ForExpr
        ) && any_spawn(src, n)
        {
            return true;
        }
        n.nodes().any(|c| walk(src, c))
    }
    walk(src, body)
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
                // s40: checked indexing (`s[a..b]`, `l[i]`) branches
                // to its `bounds` trap block.
                | SyntaxKind::BracketApply
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

/// The parameter list of a callable node — a `ClosureExpr` or a
/// nested `FnDecl` statement (#116b): the two shapes the s105 closure
/// queue lifts.
fn callable_params(e: &GreenNode) -> Option<wolf_ast::ParamList<'_>> {
    match e.kind {
        SyntaxKind::ClosureExpr => wolf_ast::ClosureExpr::cast(e).and_then(|d| d.params()),
        SyntaxKind::FnDecl => wolf_ast::FnDecl::cast(e).and_then(|d| d.params()),
        _ => None,
    }
}

/// The body node of a callable node (see [`callable_params`]).
fn callable_body(e: &GreenNode) -> Option<&GreenNode> {
    match e.kind {
        SyntaxKind::ClosureExpr => wolf_ast::ClosureExpr::cast(e).and_then(|d| d.body()),
        SyntaxKind::FnDecl => wolf_ast::FnDecl::cast(e)
            .and_then(|d| d.body())
            .map(|b| b.syntax()),
        _ => None,
    }
}

/// The inner bytes of a raw string literal's source text, or `None`
/// when the text is not raw-delimited. `r"…"`, `r#"…"#`, `r##"…"##` —
/// the whole opening delimiter (`r`, the `#` fence, the quote) and its
/// balancing close strip; what remains IS the value
/// ([gram.lex.str.raw]: no escapes, no interpolation). Byte-identical
/// with the checked executor's implementation (wolf_mem::ubcheck) —
/// #76 retired the naive first/last-byte quote strip that left the
/// opening `"` of `r"` in the value.
fn raw_str_inner(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.first() != Some(&b'r') {
        return None;
    }
    let hashes = bytes[1..].iter().take_while(|&&b| b == b'#').count();
    let open = 1 + hashes; // index of the opening `"`
    if bytes.get(open) != Some(&b'"') {
        return None;
    }
    let start = open + 1;
    let end = bytes.len().saturating_sub(1 + hashes).max(start);
    Some(&bytes[start..end])
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
