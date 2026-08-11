//! The bidirectional body checker (s13, Targets 2, 4, 5).
//!
//! Two judgments — check (`e ⇐ T`) and synthesize (`e ⇒ T`) — on the
//! Pfenning recipe: intro forms check, elim forms synthesize, and the
//! only annotations are item signatures (D27). Inside bodies the DK
//! "complete and easy" discipline runs on a union-find of existential
//! variables with levels ([`crate::unify`]); locals are monomorphic
//! (no let-generalization — a local helper that needs polymorphism
//! becomes a named item).
//!
//! Every constraint carries provenance (D22): a source span plus a
//! machine-readable [`Reason`], threaded through unification so a
//! mismatch reports *where the requirement came from* as a because
//! chain, with structural type diffs for large types and one concrete
//! hint per message.
//!
//! **The ledger-honesty contract:** constructs not yet typeable
//! return [`BodyResult::NotYetCheckable`] — never a guess. After s17
//! that set is: region/concurrency expressions (c04/c05), the unsafe
//! tier (c04/c10), std generic data and builtin-type methods (s05),
//! indexing/slicing (`e[…]`, `^n`), and the small c14-backlog tail
//! (loop values, nested items, bound methods as values).
//! String-interpolation holes are accepted at any sized
//! primitive/`str` type; full format-spec validation is s16 (D26).
//!
//! Since s17 the surface is closed: full patterns with Maranget
//! exhaustiveness (`exhaust`, E0801/E0802), method resolution with
//! the X1 receiver-mode law (inherent → in-scope traits; E0803/E0804/
//! E0807), enum variants, trait default bodies against archetype
//! `Self`, the closed coercion set ([`crate::coerce`]) and completed
//! `as` casts (E0805), and open error rows (`! {A, ..}`).
//!
//! Since s15 the error channel types for real: `!T` carries a
//! structural row, ok-injection generalizes to row-typed contexts,
//! `?` propagates by width check + injection re-tagging (E0602/E0606),
//! `else`/`else |err|` default and handle, `errdefer` is
//! fallible-only, and each `?`/`else`/raise site is a trace point
//! recorded in [`TypedBody`] (the s32 hook). Width subtyping lives in
//! the CHECKING direction here ([`Checker::expect_unify`],
//! [`Checker::row_subset`]) — never inside the unifier.
//!
//! Checking is `(&SigTables, body) → BodyResult` with no shared
//! mutable state — each body is an independent inference problem
//! (Target 5; parallelized in [`crate::typecheck`]).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use wolf_ast::{
    Arg, ArgList, AssignStmt, BinExpr, Block, BracketApply, CallExpr, CastExpr, ClosureExpr,
    ConstDecl, DeferStmt, ExprStmt, FieldInit, FnDecl, ForExpr, FreezeExpr, FromEndExpr, GreenNode,
    IfExpr, InBlock, LetDecl, MatchArm, MatchExpr, MemberExpr, ParenExpr, PathExpr, PrefixExpr,
    RangeExpr, RegionBlock, RegionValue, ReturnExpr, StringExpr, StructLit, SyntaxKind, TupleExpr,
    UnsafeBlock, VarDecl, WhileExpr, is_expr_kind, is_type_kind,
};

use wolf_diag::{Applicability, Diagnostic, Suggestion, codes};
use wolf_span::Span;

use crate::exhaust;
use crate::graph::{BindTarget, Package};
use crate::prelude;
use crate::sig::{FnSig, GenericSig, ItemSig, Lower, ParamSig, SigTables, StructSig, bindings_for};
use crate::traits::{self, TraitRef};
use crate::types::{MetaTy, Prim, TyId, TyKind, TypeTable, diff, render, subst};
use crate::unify::{NumKind, UnifyErr, VarStore, join, unify};

// ------------------------------------------------------------ contract --

/// Why a body cannot be checked yet: the construct's name and where it
/// sits. Returned instead of guessing (the ledger-honesty contract).
#[derive(Debug, Clone)]
pub struct NotYet {
    pub construct: &'static str,
    pub span: Span,
}

/// The per-body result contract (s13).
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // Checked is the overwhelmingly common case
pub enum BodyResult {
    /// Fully typed: the body's own table and the recorded types.
    Checked(TypedBody),
    /// A construct outside the s13-checkable set was encountered.
    NotYetCheckable(NotYet),
    /// The body is checkable and wrong.
    Errors(Vec<Diagnostic>),
}

/// How one `recv.method(args)` call dispatches (s17): resolution is
/// fixed and recorded — c04 reads receivers/modes from here, c05
/// lowers the call (static and `dyn` calls resolve identically by
/// name; the strategy is invisible at this layer, D27).
#[derive(Debug, Clone)]
pub enum Dispatch {
    /// An inherent-impl method of the receiver's own type.
    Inherent { ty: String, method: String },
    /// A trait method, through the coherence-unique impl (or the
    /// witness table when `dyn_call`).
    Trait {
        module: usize,
        name: String,
        method: String,
        dyn_call: bool,
    },
}

/// The declared mode surface of one resolved call site (s18/c04):
/// everything `wolf_mem`'s call-site mode agreement and exclusivity
/// checking need, recorded here so the resolution logic lives in one
/// place. Keyed by the call expression's span in [`TypedBody::calls`].
#[derive(Debug, Clone)]
pub struct CallSig {
    /// The callee label as diagnostics render it (`f`, `P.norm`).
    pub callee: String,
    /// The declaration locus (the "declared here" secondary), when
    /// the callee is a declared item (fn-typed values have none).
    pub decl_span: Option<Span>,
    /// `params[0]` is the receiver: the call site spells it as
    /// `recv.m(…)` / `(mut recv).m(…)`, not in the argument list.
    pub has_self: bool,
    /// Enum-variant construction (`Color.Rgb(1, 2, 3)`): arguments
    /// are payload values (moved in), and call-site modes are not
    /// part of the surface.
    pub ctor: bool,
    /// The callee's declared parameters (modes, spans, view sets).
    /// For fn-typed callees these are synthesized mode-`read`
    /// entries — a `fn` type cannot declare `mut`/`take`.
    pub params: Vec<crate::sig::ParamSig>,
    /// s22 — a call through the `import c` namespace (`c.malloc(…)`):
    /// unsafe-tier by D11, gated to `unsafe` blocks by wolf_mem
    /// (E1301) and recorded as an attribution fact for s23/c10.
    pub c_call: bool,
}

/// Which rule admitted an `as` cast (s17). `Numeric` lowers to a real
/// conversion in c05; `Adapter` and `Identity` are layout no-ops (the
/// D28 layout-identity fact).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastKind {
    Numeric,
    Adapter,
    Identity,
    /// s22 — a raw-tier pointer bridge: `*T ↔ *U` (free retyping,
    /// same tag), integer→pointer (provenance resolved among exposed
    /// tags, `[mem.prov.expose]`), pointer→integer (exposes the tag),
    /// or region→pointer (the region's backing base, for FFI and
    /// allocator work). Typing admits the cast anywhere; `wolf_mem`
    /// gates it to `unsafe` blocks (E1301) and records the expose
    /// fact.
    Raw,
}

/// The typed HIR of one body — minimal but real: every recorded
/// expression and local binding with its final (defaulted) type, plus
/// the body's own interned table (per-function independence).
#[derive(Debug)]
pub struct TypedBody {
    pub table: TypeTable,
    /// (span, type) per checked expression, in visit order.
    pub exprs: Vec<(Span, TyId)>,
    /// (name, span, type) per local binding.
    pub locals: Vec<(String, Span, TyId)>,
    /// Every `defer`/`errdefer` in declaration order (s15): `true` =
    /// `errdefer` (error-path-only). s27 lowers the strict-LIFO
    /// interleave from exactly this record; no unwinding anywhere.
    pub cleanups: Vec<(Span, bool)>,
    /// Error-trace hook points (s15 → s32): every `?` propagation,
    /// `else` observation, and error-tag injection site, in visit
    /// order. Debug builds write one trace entry per point ([abi.err.
    /// trace]); the runtime buffer is s32's.
    pub trace_points: Vec<Span>,
    /// Comptime call sites (s16, D29): every call to a `comptime fn`
    /// or reflection intrinsic in this (runtime) body, in visit
    /// order. The package-level comptime pass evaluates exactly
    /// these ([`crate::ctfe::run_package`]).
    pub comptime_calls: Vec<Span>,
    /// Warnings from an otherwise-clean body (E0802 unreachable arms):
    /// the body still counts as `Checked` — warnings never fail a
    /// rung — but every one is surfaced and snapshot-reviewed.
    pub warnings: Vec<Diagnostic>,
    /// Every method-call dispatch decision, in visit order (s17).
    pub dispatch: Vec<(Span, Dispatch)>,
    /// Every `as` cast: (site, source type, target type, kind).
    pub casts: Vec<(Span, TyId, TyId, CastKind)>,
    /// Every coercion the checker inserted, in visit order (s17): the
    /// closed set of [`crate::coerce`], nothing else — c04 extends the
    /// table (tier adjustments) without reopening the checker.
    pub coercions: Vec<(Span, crate::coerce::Coercion)>,
    /// Per-`match` facts: (span, exhaustive-without-default). s27's
    /// decision trees start from exactly this annotation.
    pub matches: Vec<(Span, bool)>,
    /// Every resolved call site's declared mode surface, in visit
    /// order (s18): direct calls, method calls (`has_self`),
    /// qualified trait calls, fn-typed-value calls, and enum-variant
    /// construction (`ctor`). `wolf_mem` reads argument modes and
    /// view sets from here — never by re-resolving.
    pub calls: Vec<(Span, CallSig)>,
}

impl TypedBody {
    /// Render a local's final type (test/tooling surface).
    pub fn local_type(&self, name: &str) -> Option<String> {
        self.locals
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, _, t)| render(&self.table, *t, &|_| Err("_")))
    }
}

/// One body to check: a function body, an item initializer, or (s14)
/// an impl member body.
#[derive(Debug, Clone)]
pub struct BodyRef {
    pub module: usize,
    pub file: usize,
    /// Item name (diagnostics + bench identity).
    pub name: String,
    /// Ordinal among the file root's item nodes.
    pub decl: usize,
    /// For impl member bodies: the member's ordinal among the impl's
    /// item nodes (`None` for top-level bodies).
    pub member: Option<usize>,
}

// ---------------------------------------------------------- provenance --

/// Machine-readable constraint provenance (D22): *why* a type was
/// required. Rendered as the because chain of a mismatch.
#[derive(Debug, Clone)]
pub enum Reason {
    ReturnOfFn(String),
    ArgOfCall {
        callee: String,
        index: usize,
    },
    IfCondition,
    WhileCondition,
    MatchGuard,
    LetAnnotation(String),
    OpOperands(String),
    AssignTo(String),
    StructField(String),
    ForRange,
    Pattern,
    /// An `if` without `else` produces `()`.
    BareIf,
    /// Loop bodies produce `()`.
    LoopBody,
    ClosureBody,
    GlobalInit(String),
    /// An error tag's declared payload type (s15).
    TagPayload {
        tag: String,
        index: usize,
    },
    /// The `else` fallback produces the ok half of the fallible value.
    ElseFallback,
    /// A method call's receiver (s17).
    Receiver(String),
    /// The target of `in r { … }` must be a region value (X4, s19).
    InTarget,
    /// The operand of `freeze` must be a region value (s20,
    /// `[mem.region.freeze.1]`) — or a region block, which types as
    /// its body's value.
    FreezeTarget,
}

impl Reason {
    /// "…, but {phrase} `T`" — the requirement rendered mid-sentence.
    fn phrase(&self) -> String {
        match self {
            Reason::ReturnOfFn(f) => format!("`{f}` must return"),
            Reason::ArgOfCall { callee, index } => {
                format!(
                    "`{callee}` needs its {} argument to be",
                    ordinal(*index + 1)
                )
            }
            Reason::IfCondition => "an `if` condition must be".to_string(),
            Reason::WhileCondition => "a `while` condition must be".to_string(),
            Reason::MatchGuard => "a match guard must be".to_string(),
            Reason::LetAnnotation(n) => format!("`{n}` is declared as"),
            Reason::OpOperands(op) => format!("the other side of `{op}` makes it"),
            Reason::AssignTo(p) => format!("`{p}` is"),
            Reason::StructField(n) => format!("the field `{n}` is"),
            Reason::ForRange => "the range's start makes this".to_string(),
            Reason::Pattern => "the pattern requires".to_string(),
            Reason::BareIf => "an `if` without `else` produces".to_string(),
            Reason::LoopBody => "a loop body produces".to_string(),
            Reason::ClosureBody => "the closure's context needs".to_string(),
            Reason::GlobalInit(n) => format!("`{n}` is declared as"),
            Reason::TagPayload { tag, index } => {
                format!(
                    "the tag `{tag}` declares its {} payload as",
                    ordinal(*index + 1)
                )
            }
            Reason::ElseFallback => "the `else` fallback must produce".to_string(),
            Reason::Receiver(m) => format!("`{m}`'s receiver must be"),
            Reason::InTarget => "the target of `in` must be".to_string(),
            Reason::FreezeTarget => "the target of `freeze` must be".to_string(),
        }
    }

    /// The secondary-span label of the because locus.
    fn because_label(&self) -> &'static str {
        match self {
            Reason::ReturnOfFn(_) => "the return type is declared here",
            Reason::ArgOfCall { .. } => "the parameter is declared here",
            Reason::IfCondition | Reason::WhileCondition | Reason::MatchGuard => {
                "the condition starts here"
            }
            Reason::LetAnnotation(_) => "the annotation is here",
            Reason::OpOperands(_) => "this operand fixed the type",
            Reason::AssignTo(_) => "the assignment target is here",
            Reason::StructField(_) => "the field is declared here",
            Reason::ForRange => "the start endpoint is here",
            Reason::Pattern => "the pattern is here",
            Reason::BareIf => "there is no `else` branch on this `if`",
            Reason::LoopBody => "the loop starts here",
            Reason::ClosureBody => "the closure starts here",
            Reason::GlobalInit(_) => "the annotation is here",
            Reason::TagPayload { .. } => "the row is declared here",
            Reason::ElseFallback => "the fallible expression is here",
            Reason::Receiver(_) => "the method is declared here",
            Reason::InTarget => "the `in` block starts here",
            Reason::FreezeTarget => "the `freeze` is here",
        }
    }
}

fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// A checking-mode expectation: the required type plus its provenance.
#[derive(Debug, Clone)]
struct Expect {
    ty: TyId,
    reason: Reason,
    /// The because locus (`None` when the reason is self-evident at
    /// the primary span).
    because: Option<Span>,
}

// -------------------------------------------------------------- checker --

type R<T> = Result<T, NotYet>;

struct LoopCtx {
    saw_break: bool,
}

/// The impl-search depth rail for blanket-impl chains.
const SAT_DEPTH: u32 = 8;

/// Why a type must satisfy a trait (s14 obligations, discharged at
/// body end once inputs are solved).
#[derive(Debug, Clone)]
enum OblOrigin {
    /// Instantiating `callee`'s generic parameter `param`.
    Instantiation { callee: String, param: String },
    /// A qualified call `Trait.method(…)` constrains its `Self`.
    Qualified { method: String },
}

#[derive(Debug, Clone)]
struct Obligation {
    ty: TyId,
    tr: TraitRef,
    /// Primary span: the call-site argument (or call) to blame.
    span: Span,
    /// Where the bound was declared (the secondary).
    bound_span: Option<Span>,
    origin: OblOrigin,
}

struct Checker<'a> {
    sigs: &'a SigTables,
    module: usize,
    file: usize,
    lo: Lower<'a>,
    vars: VarStore,
    /// Lexical scopes: `(name, type)`. Binding *immutability* (E0410)
    /// is the resolve rung's law since s29 (DIV-2026-010) — this
    /// checker no longer tracks `let` provenance.
    scopes: Vec<Vec<(String, TyId)>>,
    diags: Vec<Diagnostic>,
    exprs: Vec<(Span, TyId)>,
    locals: Vec<(String, Span, TyId)>,
    /// (return type, fn name, because span) of the enclosing function.
    ret: Option<(TyId, String, Span)>,
    generics: Vec<String>,
    /// Rigid name → declared bounds (the archetype facts, D28).
    bounds: traits::Bounds,
    /// Rigid name → (declaration span, has any bound) for the
    /// add-this-bound suggestion.
    generic_info: BTreeMap<String, (Span, bool)>,
    /// The impl self type when checking an impl member body.
    self_ty: Option<TyId>,
    /// Bound obligations, discharged after defaulting.
    obligations: Vec<Obligation>,
    /// Satisfaction cache keyed on (type, trait) — the s14 contract.
    sat_cache: HashMap<(TyId, usize, String), bool>,
    level: u32,
    in_closure: bool,
    loops: Vec<LoopCtx>,
    /// `defer`/`errdefer` sites in declaration order (s15 → s27).
    cleanups: Vec<(Span, bool)>,
    /// `?` / `else` / tag-injection trace hook points (s15 → s32).
    trace_points: Vec<Span>,
    /// Comptime call sites in visit order (s16 → the ctfe pass).
    comptime_calls: Vec<Span>,
    /// Inside a `comptime fn` body: its own calls evaluate only when
    /// the evaluator walks them — never registered as root sites.
    in_comptime_fn: bool,
    /// Row-sealing mode (s15): tags raised or propagated by this body
    /// are absorbed here instead of width-checked; diagnostics are
    /// discarded by the caller ([`collect_body_rows`]).
    collect: Option<RowSink>,
    /// Where an extend-the-row fix-it inserts, for this function.
    row_fix: Option<RowFix>,
    /// Method-dispatch decisions in visit order (s17).
    dispatch: Vec<(Span, Dispatch)>,
    /// `as` casts in visit order (s17).
    casts: Vec<(Span, TyId, TyId, CastKind)>,
    /// Inserted coercions in visit order (s17).
    coercions: Vec<(Span, crate::coerce::Coercion)>,
    /// Every `match` in the body, checked for exhaustiveness and
    /// reachability at body end, once types are final (s17).
    match_recs: Vec<MatchRec>,
    /// Per-match exhaustiveness facts, filled by [`Checker::check_matches`].
    match_facts: Vec<(Span, bool)>,
    /// Resolved call-site mode surfaces in visit order (s18).
    calls: Vec<(Span, CallSig)>,
    /// Every tag name spelled in an error row of the item under check
    /// — the resolver's lowercase-deferral set, recomputed here so a
    /// lowercase name it deferred is refused honestly instead of
    /// silently ([`crate::resolve::lexical_row_tags`], #4).
    row_tags: BTreeSet<String>,
}

/// One `match` recorded for the body-end exhaustiveness pass (s17):
/// patterns already lowered to the engine's IR during typing, the
/// scrutinee zonked when the pass runs.
struct MatchRec {
    /// The `match` keyword-to-brace span (E0801's primary).
    span: Span,
    scrut: TyId,
    /// (lowered pattern, has-guard, pattern span) per arm.
    arms: Vec<(exhaust::Pat, bool, Span)>,
}

/// The sink of a row-collection pass (s15 sealing): every tag the
/// body raises or propagates, with payload types in the body's table.
#[derive(Debug, Default)]
pub(crate) struct RowSink {
    pub tags: Vec<(String, Vec<TyId>)>,
}

impl RowSink {
    fn add(&mut self, name: &str, payload: Vec<TyId>) {
        if !self.tags.iter().any(|(n, _)| n == name) {
            self.tags.push((name.to_string(), payload));
        }
    }
}

/// How to machine-extend the enclosing function's error row.
#[derive(Clone, Copy, Debug)]
enum RowFix {
    /// An explicit `! {…}` exists: insert `, Tag` before its `}`.
    ExtendRow { insert_at: Span },
    /// A `-> T` with no row: append ` ! {Tag}` after the return type.
    AddRow { insert_at: Span },
}

/// What a width check found lacking: the tags the receiving row does
/// not include (name + payload types, for exact rendering), payload
/// conflicts on shared tags `(tag, found, wanted)`, and an abstract
/// tail that blocked the check.
#[derive(Debug, Default)]
struct RowLack {
    missing: Vec<(String, Vec<TyId>)>,
    conflicts: Vec<(String, String, String)>,
    abstract_tail: Option<String>,
    /// The value's row is open (`..`) but the receiving row is closed
    /// (s17): the unlisted remainder has nowhere to go.
    open_into_closed: bool,
}

impl RowLack {
    fn is_empty(&self) -> bool {
        self.missing.is_empty()
            && self.conflicts.is_empty()
            && self.abstract_tail.is_none()
            && !self.open_into_closed
    }
}

/// The extend-the-row fix-it site of a signature, if one exists. An
/// inferred (`-> !T`) row absorbs everything, so it never needs one.
fn row_fix_of(sig: &FnSig, table: &TypeTable) -> Option<RowFix> {
    if let Some(rs) = sig.row_span {
        // Insert just before the row's closing `}`.
        let at = rs.hi.saturating_sub(1);
        return Some(RowFix::ExtendRow {
            insert_at: Span::new(rs.file, at, at),
        });
    }
    let ret_span = sig.ret_span?;
    match table.kind(sig.ret) {
        // `-> !T` (inferred or member empty-row): nothing to extend.
        TyKind::ErrUnion(..) => None,
        _ => Some(RowFix::AddRow {
            insert_at: Span::new(ret_span.file, ret_span.hi, ret_span.hi),
        }),
    }
}

/// Check one body against the elaborated signatures. Pure in
/// `(&Package, &SigTables, &BodyRef)` — no shared mutable state.
pub fn check_body(pkg: &Package, sigs: &SigTables, body: &BodyRef) -> BodyResult {
    let outer = pkg.files[body.file]
        .parse
        .root
        .nodes()
        .filter(|n| n.kind.is_item())
        .nth(body.decl)
        .expect("body decl index valid");
    let node = match body.member {
        None => outer,
        Some(mi) => outer
            .nodes()
            .filter(|n| n.kind.is_item())
            .nth(mi)
            .expect("impl member index valid"),
    };
    let mut c = Checker {
        sigs,
        module: body.module,
        file: body.file,
        lo: Lower::new(pkg, sigs.table.clone()),
        vars: VarStore::new(),
        scopes: Vec::new(),
        diags: Vec::new(),
        exprs: Vec::new(),
        locals: Vec::new(),
        ret: None,
        generics: Vec::new(),
        bounds: traits::Bounds::new(),
        generic_info: BTreeMap::new(),
        self_ty: None,
        obligations: Vec::new(),
        sat_cache: HashMap::new(),
        level: 0,
        in_closure: false,
        loops: Vec::new(),
        cleanups: Vec::new(),
        trace_points: Vec::new(),
        comptime_calls: Vec::new(),
        in_comptime_fn: false,
        collect: None,
        row_fix: None,
        dispatch: Vec::new(),
        casts: Vec::new(),
        coercions: Vec::new(),
        match_recs: Vec::new(),
        match_facts: Vec::new(),
        calls: Vec::new(),
        row_tags: BTreeSet::new(),
    };
    let outcome = match body.member {
        None => c.run(node, body),
        Some(mi) if outer.kind == SyntaxKind::TraitDecl => {
            c.run_trait_member(outer, node, body, mi)
        }
        Some(mi) => c.run_impl_member(node, body, mi),
    };
    match outcome {
        Err(nyc) => BodyResult::NotYetCheckable(nyc),
        Ok(()) => {
            c.finish_defaulting();
            c.discharge_obligations();
            // Exhaustiveness/reachability runs once types are final
            // (s17): E0801 errors, E0802 warnings.
            c.check_matches();
            // Cascade suppression: diagnostics inside parse-wrecked
            // regions stay quiet (the s10 contract).
            let mut sink = wolf_diag::Diagnostics::new();
            for &region in &c.lo.pkg.files[body.file].parse.error_regions {
                sink.suppress(region);
            }
            let mut diags = Vec::new();
            for d in c.diags {
                sink.push(d);
            }
            diags.extend(sink.into_vec());
            wolf_diag::sort_diagnostics(&mut diags);
            // Warnings never fail a body: an error-free body checks,
            // carrying its warnings in the typed HIR.
            let has_errors = diags
                .iter()
                .any(|d| d.severity == wolf_diag::Severity::Error);
            if !has_errors {
                // Zonk the recorded types to their final solutions.
                let exprs = c
                    .exprs
                    .iter()
                    .map(|&(s, t)| (s, zonk(&mut c.lo.table, &c.vars, t)))
                    .collect();
                let locals = c
                    .locals
                    .iter()
                    .map(|(n, s, t)| (n.clone(), *s, zonk(&mut c.lo.table, &c.vars, *t)))
                    .collect();
                let casts = c
                    .casts
                    .iter()
                    .map(|&(s, f, t, k)| {
                        (
                            s,
                            zonk(&mut c.lo.table, &c.vars, f),
                            zonk(&mut c.lo.table, &c.vars, t),
                            k,
                        )
                    })
                    .collect();
                BodyResult::Checked(TypedBody {
                    table: c.lo.table,
                    exprs,
                    locals,
                    cleanups: c.cleanups,
                    trace_points: c.trace_points,
                    comptime_calls: c.comptime_calls,
                    warnings: diags,
                    dispatch: c.dispatch,
                    casts,
                    coercions: c.coercions,
                    matches: c.match_facts,
                    calls: c.calls,
                })
            } else {
                BodyResult::Errors(diags)
            }
        }
    }
}

/// Row-collection entry (s15 sealing): run the checker over `body` in
/// absorb mode — tags raised into the function's own (marked) row and
/// rows propagated into it by `?`/return are recorded instead of
/// width-checked. Diagnostics are discarded: the final per-body check
/// re-runs against the sealed row and owns all reports. A
/// NotYetCheckable refusal yields whatever was collected up to it (the
/// refusal already stops the file's rung). Returns the tags (payload
/// types zonked and defaulted) plus the table they live in.
pub(crate) fn collect_body_rows(
    pkg: &Package,
    sigs: &SigTables,
    body: &BodyRef,
) -> (Vec<(String, Vec<TyId>)>, TypeTable) {
    let node = pkg.files[body.file]
        .parse
        .root
        .nodes()
        .filter(|n| n.kind.is_item())
        .nth(body.decl)
        .expect("body decl index valid");
    let mut c = Checker {
        sigs,
        module: body.module,
        file: body.file,
        lo: Lower::new(pkg, sigs.table.clone()),
        vars: VarStore::new(),
        scopes: Vec::new(),
        diags: Vec::new(),
        exprs: Vec::new(),
        locals: Vec::new(),
        ret: None,
        generics: Vec::new(),
        bounds: traits::Bounds::new(),
        generic_info: BTreeMap::new(),
        self_ty: None,
        obligations: Vec::new(),
        sat_cache: HashMap::new(),
        level: 0,
        in_closure: false,
        loops: Vec::new(),
        cleanups: Vec::new(),
        trace_points: Vec::new(),
        comptime_calls: Vec::new(),
        in_comptime_fn: false,
        collect: Some(RowSink::default()),
        row_fix: None,
        dispatch: Vec::new(),
        casts: Vec::new(),
        coercions: Vec::new(),
        match_recs: Vec::new(),
        match_facts: Vec::new(),
        calls: Vec::new(),
        row_tags: BTreeSet::new(),
    };
    let _ = c.run(node, body);
    // Solve what the body pinned, then default the rest so payload
    // types leave as concrete as they will ever be.
    c.finish_defaulting();
    let sink = c.collect.take().unwrap_or_default();
    let tags: Vec<(String, Vec<TyId>)> = sink
        .tags
        .into_iter()
        .map(|(n, p)| {
            (
                n,
                p.into_iter()
                    .map(|t| zonk(&mut c.lo.table, &c.vars, t))
                    .collect(),
            )
        })
        .collect();
    (tags, c.lo.table)
}

/// Deep-resolve every solved variable in `ty`; unresolved vars remain.
fn zonk(table: &mut TypeTable, vars: &VarStore, ty: TyId) -> TyId {
    let resolved = vars.shallow(table, ty);
    match table.kind(resolved).clone() {
        TyKind::Wrapping(t) => {
            let z = zonk(table, vars, t);
            table.intern(TyKind::Wrapping(z))
        }
        TyKind::ErrUnion(t, row) => {
            let z = zonk(table, vars, t);
            let r = zonk(table, vars, row);
            table.intern(TyKind::ErrUnion(z, r))
        }
        TyKind::Row { tags, tail } => {
            let ztags: Vec<(String, Vec<TyId>)> = tags
                .into_iter()
                .map(|(n, p)| (n, p.into_iter().map(|t| zonk(table, vars, t)).collect()))
                .collect();
            let ztail = tail.map(|t| zonk(table, vars, t));
            // A tail solved to a concrete row folds into the host.
            match ztail.map(|t| table.kind(t).clone()) {
                Some(TyKind::Row {
                    tags: tt,
                    tail: inner,
                }) => {
                    let mut merged = ztags;
                    merged.extend(tt);
                    table.row(merged, inner)
                }
                _ => table.row(ztags, ztail),
            }
        }
        TyKind::Range(t) => {
            let z = zonk(table, vars, t);
            table.intern(TyKind::Range(z))
        }
        // The Tier-2 wrappers and the s21 builtin containers zonk
        // through — a `shared T`/`List[T]` whose element carries a
        // solved var must not reach `TypedBody` half-resolved.
        TyKind::Ptr(t) => {
            let z = zonk(table, vars, t);
            table.intern(TyKind::Ptr(z))
        }
        TyKind::Shared(t) => {
            let z = zonk(table, vars, t);
            table.intern(TyKind::Shared(z))
        }
        TyKind::Handle(t) => {
            let z = zonk(table, vars, t);
            table.intern(TyKind::Handle(z))
        }
        TyKind::Weak(t) => {
            let z = zonk(table, vars, t);
            table.intern(TyKind::Weak(z))
        }
        TyKind::Distinct(t) => {
            let z = zonk(table, vars, t);
            table.intern(TyKind::Distinct(z))
        }
        TyKind::List(t) => {
            let z = zonk(table, vars, t);
            table.intern(TyKind::List(z))
        }
        TyKind::Pool(t) => {
            let z = zonk(table, vars, t);
            table.intern(TyKind::Pool(z))
        }
        TyKind::Proj(base, name) => {
            let z = zonk(table, vars, base);
            table.intern(TyKind::Proj(z, name))
        }
        TyKind::Tuple(ts) => {
            let z: Vec<TyId> = ts.into_iter().map(|t| zonk(table, vars, t)).collect();
            table.intern(TyKind::Tuple(z))
        }
        TyKind::Fn(ps, r) => {
            let zp: Vec<TyId> = ps.into_iter().map(|t| zonk(table, vars, t)).collect();
            let zr = zonk(table, vars, r);
            table.intern(TyKind::Fn(zp, zr))
        }
        _ => resolved,
    }
}

impl<'a> Checker<'a> {
    // ------------------------------------------------------ plumbing ---

    fn pkg(&self) -> &'a Package {
        self.lo.pkg
    }

    fn src(&self) -> &[u8] {
        &self.pkg().files[self.file].raw.src
    }

    fn text(&self, span: Span) -> String {
        let src = self.src();
        String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned()
    }

    fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn bind(&mut self, name: String, span: Span, ty: TyId) {
        self.locals.push((name.clone(), span, ty));
        if let Some(top) = self.scopes.last_mut() {
            top.push((name, ty));
        }
    }

    /// The innermost binding of `name`. Shadowing-correct by
    /// construction: the first match in reverse scope order decides.
    fn lookup_local(&self, name: &str) -> Option<TyId> {
        for scope in self.scopes.iter().rev() {
            for (n, t) in scope.iter().rev() {
                if n == name {
                    return Some(*t);
                }
            }
        }
        None
    }

    fn error_ty(&mut self) -> TyId {
        self.lo.table.error()
    }

    fn record(&mut self, span: Span, ty: TyId) -> TyId {
        self.exprs.push((span, ty));
        ty
    }

    /// Render with unresolved vars shown by their literal kind.
    fn show(&self, ty: TyId) -> String {
        let vars = &self.vars;
        render(&self.lo.table, ty, &|v| match vars.probe(v) {
            Some(t) => Ok(t),
            None => Err(vars.kind_of(v).placeholder()),
        })
    }

    fn fresh(&mut self, kind: NumKind, origin: Span) -> TyId {
        self.vars
            .fresh(&mut self.lo.table, self.level, kind, origin)
    }

    fn shallow(&self, ty: TyId) -> TyId {
        self.vars.shallow(&self.lo.table, ty)
    }

    fn kind_of(&self, ty: TyId) -> TyKind {
        self.lo.table.kind(self.shallow(ty)).clone()
    }

    /// Lower a body-level type node: rigid names in scope, `Self`
    /// substituted by the impl self type, concrete projections
    /// normalized through the package's impls.
    fn lower_ty(&mut self, node: &GreenNode) -> TyId {
        let names = self.generics.clone();
        let mut with_self = names;
        if self.self_ty.is_some() && !with_self.iter().any(|n| n == "Self") {
            with_self.push("Self".to_string());
        }
        let t = self.lo.lower_type(self.module, self.file, &with_self, node);
        let t = match self.self_ty {
            Some(st) => {
                let map: BTreeMap<String, TyId> = [("Self".to_string(), st)].into();
                subst(&mut self.lo.table, t, &map)
            }
            None => t,
        };
        traits::normalize_projections(self.sigs, &mut self.lo.table, &mut self.vars, t)
    }

    // ------------------------------------------------------- entry ----

    fn run(&mut self, node: &GreenNode, body: &BodyRef) -> R<()> {
        self.row_tags = crate::resolve::lexical_row_tags(node, self.src());
        match node.kind {
            SyntaxKind::FnDecl => self.run_fn(node, body),
            SyntaxKind::ConstDecl | SyntaxKind::LetDecl | SyntaxKind::VarDecl => {
                self.run_global(node, body)
            }
            _ => Err(NotYet {
                construct: "this item's body",
                span: node.span,
            }),
        }
    }

    fn run_fn(&mut self, node: &GreenNode, body: &BodyRef) -> R<()> {
        let sig = match self.sigs.get(self.module, &body.name) {
            Some(ItemSig::Fn(f)) => f.clone(),
            _ => {
                return Err(NotYet {
                    construct: "a body without an elaborated signature",
                    span: node.span,
                });
            }
        };
        let d = FnDecl::cast(node).expect("kind");
        self.in_comptime_fn = sig.comptime;
        self.install_generics(&sig.generics);
        self.validate_sig_projections(&sig);
        let because = sig.ret_span.unwrap_or(sig.name_span);
        self.ret = Some((sig.ret, body.name.clone(), because));
        self.row_fix = row_fix_of(&sig, &self.lo.table);
        self.push_scope();
        for p in &sig.params {
            self.bind(p.name.clone(), p.span, p.ty);
        }
        let Some(block) = d.body() else {
            self.pop_scope();
            return Ok(()); // extern/bodyless: nothing to check
        };
        let exp = Expect {
            ty: sig.ret,
            reason: Reason::ReturnOfFn(body.name.clone()),
            because: Some(because),
        };
        self.check_block(block, &exp)?;
        self.pop_scope();
        Ok(())
    }

    /// An impl member body (s14): the body of a method or associated
    /// const inside an `impl` block. `Self` is the impl's self type;
    /// the archetype facts are the impl's bounds plus the method's.
    fn run_impl_member(&mut self, node: &GreenNode, body: &BodyRef, member: usize) -> R<()> {
        self.row_tags = crate::resolve::lexical_row_tags(node, self.src());
        let Some(imp) = self
            .sigs
            .impls
            .iter()
            .find(|i| i.file == body.file && i.decl == body.decl)
        else {
            return Err(NotYet {
                construct: "an impl member without an elaborated header",
                span: node.span,
            });
        };
        self.self_ty = Some(imp.self_ty);
        self.install_generics(&imp.generics.clone());
        match node.kind {
            SyntaxKind::FnDecl => {
                let Some(m) = imp.methods.iter().find(|m| m.member == member).cloned() else {
                    return Err(NotYet {
                        construct: "an impl member without an elaborated signature",
                        span: node.span,
                    });
                };
                let d = FnDecl::cast(node).expect("kind");
                self.in_comptime_fn = m.sig.comptime;
                self.install_generics(&m.sig.generics);
                self.validate_sig_projections(&m.sig);
                let because = m.sig.ret_span.unwrap_or(m.sig.name_span);
                self.ret = Some((m.sig.ret, m.name.clone(), because));
                self.row_fix = row_fix_of(&m.sig, &self.lo.table);
                self.push_scope();
                for p in &m.sig.params {
                    self.bind(p.name.clone(), p.span, p.ty);
                }
                let Some(block) = d.body() else {
                    self.pop_scope();
                    return Ok(());
                };
                let exp = Expect {
                    ty: m.sig.ret,
                    reason: Reason::ReturnOfFn(m.name.clone()),
                    because: Some(because),
                };
                self.check_block(block, &exp)?;
                self.pop_scope();
                Ok(())
            }
            SyntaxKind::ConstDecl => {
                let c = imp.consts.iter().find(|c| c.member == member).cloned();
                let d = ConstDecl::cast(node);
                let init = d.and_then(|c| c.init());
                if let Some(init) = init {
                    self.push_scope();
                    match c {
                        Some(c) => {
                            let exp = Expect {
                                ty: c.ty,
                                reason: Reason::GlobalInit(c.name.clone()),
                                because: Some(c.name_span),
                            };
                            self.check_expr(init, &exp)?;
                        }
                        None => {
                            self.synth_expr(init)?;
                        }
                    }
                    self.pop_scope();
                }
                Ok(())
            }
            _ => Err(NotYet {
                construct: "this impl member's body",
                span: node.span,
            }),
        }
    }

    /// A trait member's *default* body (s17): checked once against the
    /// trait's own archetype — `Self` is rigid with exactly this trait
    /// as its bound (the golden rule, D28), so everything the body
    /// does must be provable from the trait's own surface.
    fn run_trait_member(
        &mut self,
        outer: &GreenNode,
        node: &GreenNode,
        body: &BodyRef,
        _member: usize,
    ) -> R<()> {
        self.row_tags = crate::resolve::lexical_row_tags(node, self.src());
        let Some(tname_tok) = wolf_ast::TraitDecl::cast(outer).and_then(|d| d.name()) else {
            return Err(NotYet {
                construct: "a trait member without a named trait",
                span: node.span,
            });
        };
        let tname = self.text(tname_tok.span);
        let tr = TraitRef {
            module: self.module,
            name: tname,
        };
        let Some(td) = self.sigs.traits.get(&tr).cloned() else {
            return Err(NotYet {
                construct: "a trait member without an elaborated trait",
                span: node.span,
            });
        };
        if !td.params.is_empty() {
            return Err(NotYet {
                construct: "default bodies of parameterized traits (c14 backlog)",
                span: node.span,
            });
        }
        self.install_generics(&[GenericSig {
            name: "Self".to_string(),
            span: td.name_span,
            bounds: vec![crate::sig::BoundRef {
                module: tr.module,
                name: tr.name.clone(),
                span: td.name_span,
            }],
        }]);
        // An associated const's default initializer.
        if node.kind == SyntaxKind::ConstDecl {
            let c = td
                .assoc_consts
                .iter()
                .find(|c| c.name == body.name)
                .cloned();
            let init = ConstDecl::cast(node).and_then(|c| c.init());
            if let Some(init) = init {
                self.push_scope();
                match c {
                    Some(c) => {
                        let exp = Expect {
                            ty: c.ty,
                            reason: Reason::GlobalInit(c.name.clone()),
                            because: Some(c.name_span),
                        };
                        self.check_expr(init, &exp)?;
                    }
                    None => {
                        self.synth_expr(init)?;
                    }
                }
                self.pop_scope();
            }
            return Ok(());
        }
        let Some(m) = td.methods.iter().find(|m| m.name == body.name).cloned() else {
            return Err(NotYet {
                construct: "a trait member without an elaborated signature",
                span: node.span,
            });
        };
        let d = FnDecl::cast(node).expect("kind");
        self.in_comptime_fn = m.sig.comptime;
        self.install_generics(&m.sig.generics);
        self.validate_sig_projections(&m.sig);
        let because = m.sig.ret_span.unwrap_or(m.sig.name_span);
        self.ret = Some((m.sig.ret, m.name.clone(), because));
        self.row_fix = row_fix_of(&m.sig, &self.lo.table);
        self.push_scope();
        for p in &m.sig.params {
            self.bind(p.name.clone(), p.span, p.ty);
        }
        let Some(block) = d.body() else {
            self.pop_scope();
            return Ok(());
        };
        let exp = Expect {
            ty: m.sig.ret,
            reason: Reason::ReturnOfFn(m.name.clone()),
            because: Some(because),
        };
        self.check_block(block, &exp)?;
        self.pop_scope();
        Ok(())
    }

    /// Bring a generic parameter list into scope: rigid names, bound
    /// facts, and the add-this-bound spans.
    fn install_generics(&mut self, gens: &[GenericSig]) {
        for g in gens {
            if !self.generics.contains(&g.name) {
                self.generics.push(g.name.clone());
            }
            self.bounds.insert(g.name.clone(), g.bounds.clone());
            self.generic_info
                .insert(g.name.clone(), (g.span, !g.bounds.is_empty()));
        }
    }

    /// Golden-rule validation of the signature itself: every
    /// projection `T.Item` written in the signature must be provable
    /// from `T`'s bounds (a bound trait declaring `Item`).
    fn validate_sig_projections(&mut self, sig: &FnSig) {
        let mut sites: Vec<(TyId, Span)> = sig.params.iter().map(|p| (p.ty, p.span)).collect();
        sites.push((sig.ret, sig.ret_span.unwrap_or(sig.name_span)));
        for (ty, span) in sites {
            self.validate_projections_in(ty, span);
        }
    }

    fn validate_projections_in(&mut self, ty: TyId, span: Span) {
        match self.lo.table.kind(ty).clone() {
            TyKind::Proj(base, name) => {
                if let TyKind::Rigid(param) = self.lo.table.kind(base).clone()
                    && self.assoc_type_provider(&param, &name).is_none()
                {
                    self.golden_rule_missing(
                        span,
                        &param,
                        &format!("an associated type `{name}`"),
                        self.suggest_assoc_provider(&name),
                    );
                }
                self.validate_projections_in(base, span);
            }
            TyKind::Wrapping(t)
            | TyKind::Range(t)
            | TyKind::Ptr(t)
            | TyKind::Shared(t)
            | TyKind::Handle(t)
            | TyKind::Weak(t)
            | TyKind::Distinct(t)
            | TyKind::List(t)
            | TyKind::Pool(t) => self.validate_projections_in(t, span),
            TyKind::ErrUnion(t, row) => {
                self.validate_projections_in(t, span);
                self.validate_projections_in(row, span);
            }
            TyKind::Row { tags, tail } => {
                for (_, payload) in tags {
                    for t in payload {
                        self.validate_projections_in(t, span);
                    }
                }
                if let Some(t) = tail {
                    self.validate_projections_in(t, span);
                }
            }
            TyKind::Tuple(ts) => {
                for t in ts {
                    self.validate_projections_in(t, span);
                }
            }
            TyKind::Fn(ps, r) => {
                for t in ps {
                    self.validate_projections_in(t, span);
                }
                self.validate_projections_in(r, span);
            }
            _ => {}
        }
    }

    /// The bound trait of `param` that declares associated type
    /// `name`, if exactly one does.
    fn assoc_type_provider(&self, param: &str, name: &str) -> Option<TraitRef> {
        let bs = self.bounds.get(param)?;
        let mut hit = None;
        for b in bs {
            let tr = TraitRef {
                module: b.module,
                name: b.name.clone(),
            };
            if let Some(td) = self.sigs.traits.get(&tr)
                && td.assoc_types.iter().any(|a| a.name == name)
            {
                if hit.is_some() {
                    return None; // ambiguous: not provable as written
                }
                hit = Some(tr);
            }
        }
        hit
    }

    /// A trait (anywhere in the package) declaring associated type
    /// `name` — suggestion fodder for the add-this-bound hint.
    fn suggest_assoc_provider(&self, name: &str) -> Option<String> {
        let mut hits = self
            .sigs
            .traits
            .values()
            .filter(|td| td.assoc_types.iter().any(|a| a.name == name))
            .map(|td| td.name.clone());
        let first = hits.next()?;
        if hits.next().is_some() {
            None
        } else {
            Some(first)
        }
    }

    fn run_global(&mut self, node: &GreenNode, body: &BodyRef) -> R<()> {
        let sig_ty = match self.sigs.get(self.module, &body.name) {
            Some(ItemSig::Global(g)) => g.ty,
            _ => None,
        };
        let init = node.nodes().find(|n| is_expr_kind(n.kind));
        let ann_span = node
            .nodes()
            .find(|n| is_type_kind(n.kind))
            .map(|n| n.span)
            .unwrap_or(node.span);
        if let Some(init) = init {
            match sig_ty {
                Some(ty) => {
                    let exp = Expect {
                        ty,
                        reason: Reason::GlobalInit(body.name.clone()),
                        because: Some(ann_span),
                    };
                    self.push_scope();
                    self.check_expr(init, &exp)?;
                    self.pop_scope();
                }
                None => {
                    // Missing annotation is E0407 (already reported by
                    // signature elaboration); still walk the body.
                    self.push_scope();
                    self.synth_expr(init)?;
                    self.pop_scope();
                }
            }
        }
        Ok(())
    }

    // ---------------------------------------------- expectations ------

    /// `actual ⇐ exp`, with the `!T` rules of the checking direction
    /// (D30/s15): ok-injection (`T ⇐ !T`), and **width-only row
    /// subtyping** at this boundary — a narrower fallible value flows
    /// into a wider row. Subtyping lives HERE, never inside the
    /// unifier. The trials run on a unifier snapshot — the s14/s17
    /// speculative machinery, exercised from day one.
    fn expect_unify(&mut self, span: Span, actual: TyId, exp: &Expect) {
        let expected = self.shallow(exp.ty);
        if let TyKind::ErrUnion(inner, erow) = self.lo.table.kind(expected).clone() {
            let snap = self.vars.snapshot();
            if unify(&mut self.lo.table, &mut self.vars, actual, expected).is_ok() {
                return;
            }
            self.vars.rollback(snap);
            let act = self.shallow(actual);
            if let TyKind::ErrUnion(a_ok, a_row) = self.lo.table.kind(act).clone() {
                // Width at the checking boundary: the value's row must
                // fit inside the expected row (tags-missing named
                // exactly; E0602/E0606).
                let inner_exp = Expect {
                    ty: inner,
                    reason: exp.reason.clone(),
                    because: exp.because,
                };
                if let Err(e) = unify(&mut self.lo.table, &mut self.vars, a_ok, inner) {
                    self.report_unify_err(span, a_ok, &inner_exp, e);
                }
                self.require_row_widening(span, a_row, erow, exp.because);
                return;
            }
            let inner_exp = Expect {
                ty: inner,
                reason: exp.reason.clone(),
                because: exp.because,
            };
            if let Err(e) = unify(&mut self.lo.table, &mut self.vars, actual, inner) {
                self.report_unify_err(span, actual, &inner_exp, e);
            }
            return;
        }
        if let Err(e) = unify(&mut self.lo.table, &mut self.vars, actual, exp.ty) {
            self.report_unify_err(span, actual, exp, e);
        }
    }

    fn report_unify_err(&mut self, span: Span, actual: TyId, exp: &Expect, err: UnifyErr) {
        match err {
            UnifyErr::Occurs { var, ty } => self.report_occurs(span, var, ty),
            UnifyErr::Mismatch => self.report_mismatch(span, actual, exp),
            UnifyErr::NeedsWitness => self.report_needs_witness(span, actual, exp),
        }
    }

    /// E0707 — const-generic equality beyond the linear line: the
    /// forms may be equal, but the compiler will not guess, and the
    /// line is documented right here (s16, report 02 Needs-work #8).
    fn report_needs_witness(&mut self, span: Span, actual: TyId, exp: &Expect) {
        let a = self.show(actual);
        let e = self.show(exp.ty);
        let mut d = Diagnostic::error(
            codes::E0707,
            span,
            format!("`{a}` and `{e}` may be equal, but proving it needs a witness"),
        )
        .with_label("these const expressions differ beyond linear arithmetic");
        if let Some(because) = exp.because {
            d = d.with_secondary(because, exp.reason.because_label());
        }
        d = d
            .with_note(
                "const-expression equality is decided in three steps, and the line \
                 is fixed: (1) closed expressions evaluate and compare by value; \
                 (2) `+`/`-` arithmetic over generic parameters compares by ring \
                 normalization, so `N + 1` equals `1 + N`; (3) anything beyond — \
                 `*`, `/`, `%`, shifts, bit operators — needs an explicit witness. \
                 This pair sits at step 3.",
            )
            .with_note(
                "state the equality where the reader can see it: a comptime \
                 `assert` on the sizes involved, or rewrite both spellings into \
                 the same `+`/`-` form.",
            );
        self.diags.push(d);
    }

    /// E0404 — render the cycle.
    fn report_occurs(&mut self, span: Span, var: TyId, ty: TyId) {
        let inner = self.show(ty);
        let _ = var;
        self.diags.push(
            Diagnostic::error(
                codes::E0404,
                span,
                "this would need an infinite type".to_string(),
            )
            .with_label(format!(
                "call it `t`: `t` would have to be `{inner}` with `t` itself \
                 inside — the substitution never terminates"
            ))
            .with_note(
                "no finite type satisfies a cycle like `t = fn(t) -> …`; this \
                 usually means a function is applied to itself — name the \
                 intended type explicitly to break the cycle."
                    .to_string(),
            ),
        );
    }

    /// E0401 — expected/actual with the because chain, a structural
    /// diff for large types, and one concrete hint.
    fn report_mismatch(&mut self, span: Span, actual: TyId, exp: &Expect) {
        let a = self.show(actual);
        let e = self.show(exp.ty);
        let mut d = Diagnostic::error(
            codes::E0401,
            span,
            format!("this is `{a}`, but {} `{e}`", exp.reason.phrase()),
        )
        .with_label(format!("found `{a}` here"));
        if let Some(because) = exp.because {
            d = d.with_secondary(because, exp.reason.because_label());
        }
        d = d.with_note(format!("expected `{e}`, found `{a}`"));
        // Structural diff: same constructor, differing parts named.
        let mut parts = Vec::new();
        let vars = &self.vars;
        diff(
            &self.lo.table,
            exp.ty,
            actual,
            &|v| match vars.probe(v) {
                Some(t) => Ok(t),
                None => Err(vars.kind_of(v).placeholder()),
            },
            &mut parts,
            "top",
        );
        if !parts.is_empty() && !parts.iter().any(|p| p.starts_with("top:")) {
            for p in parts.iter().take(3) {
                d = d.with_note(format!("the types differ in {p}"));
            }
        }
        if let Some(hint) = self.mismatch_hint(exp.ty, actual) {
            d = d.with_note(hint);
        }
        self.diags.push(d);
    }

    /// One concrete hint, when a classic applies.
    fn mismatch_hint(&self, expected: TyId, actual: TyId) -> Option<String> {
        let e = self.kind_of(expected);
        let a = self.kind_of(actual);
        let numeric = |k: &TyKind| match k {
            TyKind::Prim(p) => p.is_integer() || p.is_float(),
            TyKind::Wrapping(_) => true,
            TyKind::Var(v) => matches!(
                self.vars.kind_of(*v),
                NumKind::Integer | NumKind::Float | NumKind::Num
            ),
            _ => false,
        };
        // The argument-vs-return classic: the function itself where its
        // result was meant.
        if let TyKind::Fn(params, ret) = &a
            && params.is_empty()
            && self.shallow(*ret) == self.shallow(expected)
        {
            return Some(
                "this is the function itself; add `()` to call it and use its result.".to_string(),
            );
        }
        match (&e, &a) {
            (TyKind::Prim(Prim::Bool), _) if numeric(&a) => {
                Some("wolf has no truthiness: write the comparison out, e.g. `x != 0`.".to_string())
            }
            (TyKind::Prim(Prim::Str), _) if numeric(&a) => Some(
                "build strings with interpolation: \"{value}\" formats any primitive.".to_string(),
            ),
            (_, _) if numeric(&e) && numeric(&a) => Some(format!(
                "wolf never converts numbers implicitly; convert explicitly, \
                 e.g. `x as {}`.",
                self.show(expected)
            )),
            _ => None,
        }
    }

    /// E0409 — an operator applied to a type outside its family.
    fn report_bad_operand(&mut self, span: Span, op: &str, needs: &str, found: TyId) {
        let f = self.show(found);
        let mut d = Diagnostic::error(
            codes::E0409,
            span,
            format!("`{op}` cannot be applied to `{f}`"),
        )
        .with_label(format!("this is `{f}`"))
        .with_note(format!("`{op}` works on {needs}."));
        if op == "&&" || op == "||" || op == "!" {
            d = d.with_note(
                "wolf has no truthiness: write the comparison out, e.g. `x != 0`.".to_string(),
            );
        }
        if op == "+" && matches!(self.kind_of(found), TyKind::Prim(Prim::Str)) {
            d = d.with_note(
                "join strings with interpolation instead: \"{first}{second}\".".to_string(),
            );
        }
        self.diags.push(d);
    }

    // ---------------------------------------------- error rows (s15) ---

    /// The enclosing function's error row, if it is fallible.
    fn caller_row(&self) -> Option<TyId> {
        let (ret, _, _) = self.ret.as_ref()?;
        match self.kind_of(*ret) {
            TyKind::ErrUnion(_, row) => Some(row),
            _ => None,
        }
    }

    /// Render one tag as source spells it (`Io(IoError)`).
    fn tag_str(&self, name: &str, payload: &[TyId]) -> String {
        let vars = &self.vars;
        crate::types::render_tag(&self.lo.table, name, payload, &|v| match vars.probe(v) {
            Some(t) => Ok(t),
            None => Err(vars.kind_of(v).placeholder()),
        })
    }

    /// Normalize a row for width checking: resolve solved tails and
    /// fold them into the host row.
    fn resolve_row(&mut self, row: TyId) -> TyId {
        let head = self.shallow(row);
        zonk(&mut self.lo.table, &self.vars, head)
    }

    /// Width-only row subtyping (`sub ⊆ sup`), CHECKING direction only
    /// — never inside the unifier. Payloads of shared tags unify
    /// pointwise (they are invariant).
    fn row_subset(&mut self, sub: TyId, sup: TyId) -> Result<(), RowLack> {
        let sub = self.resolve_row(sub);
        let sup = self.resolve_row(sup);
        if sub == sup {
            return Ok(());
        }
        let ks = self.lo.table.kind(sub).clone();
        let kp = self.lo.table.kind(sup).clone();
        match (ks, kp) {
            (TyKind::Error | TyKind::Never, _) | (_, TyKind::Error | TyKind::Never) => Ok(()),
            // Markers appear only for the fn being collected: its own
            // row flowing into itself (recursion) is vacuously fine.
            (TyKind::InferredRow { .. }, _) | (_, TyKind::InferredRow { .. }) => Ok(()),
            // An unsolved row variable at a boundary assumes the
            // boundary's row (checking direction pins it).
            (TyKind::Var(_), _) => {
                let _ = unify(&mut self.lo.table, &mut self.vars, sub, sup);
                Ok(())
            }
            (_, TyKind::Var(_)) => {
                let _ = unify(&mut self.lo.table, &mut self.vars, sup, sub);
                Ok(())
            }
            (TyKind::Rigid(a), TyKind::Rigid(b)) if a == b => Ok(()),
            (TyKind::Rigid(a), TyKind::Row { tail: Some(t), .. }) if matches!(self.kind_of(t), TyKind::Rigid(b) if b == a) => {
                Ok(())
            }
            (TyKind::Rigid(a), _) => Err(RowLack {
                abstract_tail: Some(a),
                ..RowLack::default()
            }),
            (
                TyKind::Row {
                    tags: ta,
                    tail: tla,
                },
                TyKind::Row {
                    tags: tb,
                    tail: tlb,
                },
            ) => {
                let mut lack = RowLack::default();
                let sup_open = tlb.is_some_and(|t| matches!(self.kind_of(t), TyKind::OpenTail));
                let sub_open = tla.is_some_and(|t| matches!(self.kind_of(t), TyKind::OpenTail));
                if sub_open && !sup_open {
                    lack.open_into_closed = true;
                }
                for (n, pa) in &ta {
                    match tb.iter().find(|(m, _)| m == n) {
                        // An open receiving row admits every tag; only
                        // shared-tag payloads can still disagree.
                        None if sup_open => {}
                        None => lack.missing.push((n.clone(), pa.clone())),
                        Some((_, pb)) => {
                            if pa.len() != pb.len() {
                                lack.conflicts.push((
                                    n.clone(),
                                    self.tag_str(n, pa),
                                    self.tag_str(n, pb),
                                ));
                                continue;
                            }
                            let snap = self.vars.snapshot();
                            let mut ok = true;
                            for (x, y) in pa.iter().zip(pb.iter()) {
                                if unify(&mut self.lo.table, &mut self.vars, *x, *y).is_err() {
                                    ok = false;
                                    break;
                                }
                            }
                            if !ok {
                                self.vars.rollback(snap);
                                lack.conflicts.push((
                                    n.clone(),
                                    self.tag_str(n, pa),
                                    self.tag_str(n, pb),
                                ));
                            }
                        }
                    }
                }
                match tla.map(|t| self.kind_of(t)) {
                    None => {}
                    Some(TyKind::OpenTail) => {} // handled by sub_open above
                    Some(TyKind::Rigid(e)) => {
                        let sup_has_same_tail = tlb
                            .is_some_and(|t| matches!(self.kind_of(t), TyKind::Rigid(f) if f == e));
                        if !sup_has_same_tail {
                            lack.abstract_tail = Some(e);
                        }
                    }
                    Some(TyKind::Var(_)) => {
                        // Pin the open remainder to what the boundary
                        // still allows: sup's tags not already in sub.
                        let remainder: Vec<(String, Vec<TyId>)> = tb
                            .iter()
                            .filter(|(m, _)| !ta.iter().any(|(n, _)| n == m))
                            .cloned()
                            .collect();
                        let rem = self.lo.table.row(remainder, tlb);
                        let tail = tla.expect("tail present");
                        let _ = unify(&mut self.lo.table, &mut self.vars, tail, rem);
                    }
                    Some(_) => {}
                }
                if lack.is_empty() { Ok(()) } else { Err(lack) }
            }
            (
                TyKind::Row {
                    tags,
                    tail: Some(t),
                },
                TyKind::Rigid(b),
            ) if tags.is_empty() && matches!(self.kind_of(t), TyKind::Rigid(e) if e == b) => Ok(()),
            (TyKind::Row { tags, .. }, TyKind::Rigid(b)) => {
                let mut lack = RowLack {
                    abstract_tail: Some(b),
                    ..RowLack::default()
                };
                lack.missing
                    .extend(tags.iter().map(|(n, p)| (n.clone(), p.clone())));
                Err(lack)
            }
            _ => Ok(()),
        }
    }

    /// `sub ⊆ sup` at a boundary, reporting E0602/E0606 on failure —
    /// or absorbing `sub` into the collection sink when `sup` is this
    /// function's own inferred-row marker (sealing mode).
    fn require_row_widening(&mut self, span: Span, sub: TyId, sup: TyId, because: Option<Span>) {
        if matches!(self.kind_of(sup), TyKind::InferredRow { .. }) && self.collect.is_some() {
            self.absorb_row(sub);
            return;
        }
        match self.row_subset(sub, sup) {
            Ok(()) => {}
            Err(lack) => self.report_row_lack(span, lack, because),
        }
    }

    /// Record every tag of `row` into the collection sink.
    fn absorb_row(&mut self, row: TyId) {
        let row = self.resolve_row(row);
        if let TyKind::Row { tags, .. } = self.lo.table.kind(row).clone()
            && let Some(sink) = self.collect.as_mut()
        {
            for (n, p) in tags {
                sink.add(&n, p);
            }
        }
    }

    /// E0602 (tags the receiving row lacks — named exactly, with the
    /// signature-extending fix-it) and E0606 (payload conflicts on
    /// shared tags). Never renders whole rows.
    fn report_row_lack(&mut self, span: Span, lack: RowLack, because: Option<Span>) {
        if self.collect.is_some() {
            return; // collection passes never report
        }
        let fn_name = self
            .ret
            .as_ref()
            .map(|(_, n, _)| n.clone())
            .unwrap_or_else(|| "this function".to_string());
        if !lack.missing.is_empty() {
            let rendered: Vec<String> = lack
                .missing
                .iter()
                .map(|(n, p)| self.tag_str(n, p))
                .collect();
            let listed = rendered
                .iter()
                .map(|t| format!("`{t}`"))
                .collect::<Vec<_>>()
                .join(", ");
            let mut d = Diagnostic::error(
                codes::E0602,
                span,
                format!(
                    "this can also fail with {listed}, which `{fn_name}`'s row does not include"
                ),
            )
            .with_label(if rendered.len() == 1 {
                "the missing tag escapes here"
            } else {
                "the missing tags escape here"
            });
            if let Some(b) = because {
                d = d.with_secondary(b, "the receiving row is declared here");
            }
            d = d.with_note(
                "rows compose by union: `?` re-tags errors into the wider row by \
                 injection — there is no conversion to write, only tags to admit.",
            );
            match self.row_fix {
                Some(RowFix::ExtendRow { insert_at }) => {
                    d = d.with_suggestion(Suggestion::new(
                        format!("extend the row with {listed}"),
                        vec![(insert_at, format!(", {}", rendered.join(", ")))],
                        Applicability::Maybe,
                    ));
                }
                Some(RowFix::AddRow { insert_at }) => {
                    d = d.with_suggestion(Suggestion::new(
                        format!("declare the row: `! {{{}}}`", rendered.join(", ")),
                        vec![(insert_at, format!(" ! {{{}}}", rendered.join(", ")))],
                        Applicability::Maybe,
                    ));
                }
                None => {}
            }
            d = d.with_row_diff(wolf_diag::RowDiff {
                missing: rendered,
                extra: Vec::new(),
            });
            self.diags.push(d);
        }
        for (tag, found, wanted) in &lack.conflicts {
            let mut d = Diagnostic::error(
                codes::E0606,
                span,
                format!(
                    "the tag `{tag}` carries `{found}` here, but the receiving row \
                     declares `{wanted}`"
                ),
            )
            .with_label("the payloads disagree");
            if let Some(b) = because {
                d = d.with_secondary(b, "the receiving row is declared here");
            }
            d = d.with_note(
                "tag names are structural, so a shared name is the same tag — its \
                 payload types must agree everywhere it appears; align them, or \
                 use two different tag names.",
            );
            self.diags.push(d);
        }
        if lack.open_into_closed {
            let mut d = Diagnostic::error(
                codes::E0602,
                span,
                format!("this value's error row is open (`..`), but `{fn_name}`'s row is closed"),
            )
            .with_label("the `..` admits tags the receiving row does not");
            if let Some(b) = because {
                d = d.with_secondary(b, "the receiving row is declared here");
            }
            d = d.with_note(
                "an open row declares *at least* its listed tags; only another \
                 open row can absorb the unlisted remainder.",
            );
            if let Some(RowFix::ExtendRow { insert_at }) = self.row_fix {
                d = d.with_suggestion(Suggestion::new(
                    "open the receiving row: `..`".to_string(),
                    vec![(insert_at, ", ..".to_string())],
                    Applicability::Maybe,
                ));
            }
            self.diags.push(d);
        }
        if let Some(e) = &lack.abstract_tail {
            let mut d = Diagnostic::error(
                codes::E0602,
                span,
                format!(
                    "these errors flow into the abstract row `{e}`, which cannot be \
                     assumed to include them"
                ),
            )
            .with_label("abstract row boundary");
            if let Some(b) = because {
                d = d.with_secondary(b, "the receiving row is declared here");
            }
            d = d.with_note(format!(
                "`{e}` is a row variable — the caller chooses its tags, so a body \
                 can only propagate `{e}` itself; give the receiving row the same \
                 tail, or make the tags concrete on both sides."
            ));
            self.diags.push(d);
        }
    }

    /// Inject an error tag into `row` (D30): a bit-level re-tag, never
    /// a conversion. In sealing mode the tag is absorbed instead.
    fn inject_tag(
        &mut self,
        span: Span,
        tag: &str,
        args: &[&GreenNode],
        row: TyId,
        because: Option<Span>,
    ) -> R<()> {
        self.trace_points.push(span);
        let row = self.resolve_row(row);
        match self.lo.table.kind(row).clone() {
            TyKind::InferredRow { .. } => {
                let mut ptys = Vec::new();
                for a in args {
                    ptys.push(self.synth_expr(a)?);
                }
                if let Some(sink) = self.collect.as_mut() {
                    sink.add(tag, ptys);
                }
                Ok(())
            }
            TyKind::Row { tags, tail } => {
                match tags.iter().find(|(n, _)| n == tag) {
                    Some((_, payload)) => {
                        let payload = payload.clone();
                        if args.len() != payload.len() {
                            let found =
                                format!("{tag}({})", if args.is_empty() { "" } else { "…" });
                            let declared = self.tag_str(tag, &payload);
                            let mut d = Diagnostic::error(
                                codes::E0606,
                                span,
                                format!(
                                    "the tag `{tag}` carries {} payload{}, but this raise \
                                     gives it {}",
                                    payload.len(),
                                    if payload.len() == 1 { "" } else { "s" },
                                    args.len()
                                ),
                            )
                            .with_label(format!("raised as `{found}`"))
                            .with_note(format!(
                                "the row declares `{declared}`; a tag's payload list is \
                                 part of its identity — match it exactly."
                            ));
                            if let Some(b) = because {
                                d = d.with_secondary(b, "the row is declared here");
                            }
                            self.diags.push(d);
                            for a in args {
                                self.synth_expr(a)?;
                            }
                            return Ok(());
                        }
                        for (i, (a, p)) in args.iter().zip(payload.iter()).enumerate() {
                            let exp = Expect {
                                ty: *p,
                                reason: Reason::TagPayload {
                                    tag: tag.to_string(),
                                    index: i,
                                },
                                because,
                            };
                            self.check_expr(a, &exp)?;
                        }
                        Ok(())
                    }
                    None => {
                        let mut ptys = Vec::new();
                        for a in args {
                            ptys.push(self.synth_expr(a)?);
                        }
                        // With an abstract tail, the one report is the
                        // abstract-row story; otherwise it is the
                        // missing tag with its fix-it.
                        let mut lack = RowLack::default();
                        match tail.map(|t| self.kind_of(t)) {
                            // An open row admits raises of new tags —
                            // the Roc growing-tag-union story (s17).
                            Some(TyKind::OpenTail) => return Ok(()),
                            Some(TyKind::Rigid(e)) => lack.abstract_tail = Some(e),
                            _ => lack.missing.push((tag.to_string(), ptys)),
                        }
                        self.report_row_lack(span, lack, because);
                        Ok(())
                    }
                }
            }
            TyKind::Rigid(e) => {
                for a in args {
                    self.synth_expr(a)?;
                }
                let lack = RowLack {
                    abstract_tail: Some(e),
                    ..RowLack::default()
                };
                self.report_row_lack(span, lack, because);
                Ok(())
            }
            TyKind::Var(_) => {
                // An open row variable (a closure checked against a
                // row-polymorphic signature): the raise pins it to the
                // closed row containing exactly this tag.
                let mut ptys = Vec::new();
                for a in args {
                    ptys.push(self.synth_expr(a)?);
                }
                let closed = self.lo.table.row(vec![(tag.to_string(), ptys)], None);
                let _ = unify(&mut self.lo.table, &mut self.vars, row, closed);
                Ok(())
            }
            _ => {
                for a in args {
                    self.synth_expr(a)?;
                }
                Ok(())
            }
        }
    }

    // ---------------------------------------------- the golden rule ----

    /// The rigid (archetype) name of `ty`, if it is one.
    fn rigid_name(&self, ty: TyId) -> Option<String> {
        match self.kind_of(ty) {
            TyKind::Rigid(n) => Some(n),
            _ => None,
        }
    }

    /// E0501 — a generic body uses a capability its bounds do not
    /// provide (definition-site, D28). `capability` is mid-sentence
    /// ("`+`", "the method `show` of `Show`"); `add_bound` names a
    /// trait that would grant it, wired to a machine edit when the
    /// parameter has no bounds yet.
    fn golden_rule_missing(
        &mut self,
        span: Span,
        param: &str,
        capability: &str,
        add_bound: Option<String>,
    ) {
        let mut d = Diagnostic::error(
            codes::E0501,
            span,
            format!("the bounds on `{param}` do not provide {capability}"),
        )
        .with_label(format!("`{param}` could be any type here"));
        if let Some((decl, has_bounds)) = self.generic_info.get(param).copied() {
            d = d.with_secondary(decl, format!("`{param}` is declared here"));
            if let Some(tr) = add_bound {
                if has_bounds {
                    d = d.with_note(format!(
                        "add `+ {tr}` to `{param}`'s bounds to make this provable."
                    ));
                } else {
                    let insert = Span::new(decl.file, decl.hi, decl.hi);
                    d = d.with_suggestion(Suggestion::new(
                        format!("add the bound: `{param}: {tr}`"),
                        vec![(insert, format!(": {tr}"))],
                        Applicability::Maybe,
                    ));
                }
            }
        }
        d = d.with_note(
            "generic bodies are checked once against their bounds (the golden \
             rule, D28): everything a body does with a parameter must be provable \
             from the bounds alone, so instantiation can never fail inside it.",
        );
        self.diags.push(d);
    }

    /// E0501 for operator/comparison capabilities on archetypes: no
    /// bound can grant these yet (operator traits are later), so the
    /// message says so instead of hinting a bound.
    fn golden_rule_op(&mut self, span: Span, param: &str, op: &str) {
        self.diags.push(
            Diagnostic::error(
                codes::E0501,
                span,
                format!("the bounds on `{param}` say nothing about `{op}`"),
            )
            .with_label(format!("`{param}` could be any type here"))
            .with_note(
                "bounds grant trait members only, and no trait covers this \
                 operator yet (operator traits are a later sprint); take a \
                 concrete type here, or dispatch through a trait method.",
            ),
        );
    }

    /// Record a bound obligation, discharged after defaulting.
    fn obligate(
        &mut self,
        ty: TyId,
        tr: TraitRef,
        span: Span,
        bound_span: Option<Span>,
        origin: OblOrigin,
    ) {
        self.obligations.push(Obligation {
            ty,
            tr,
            span,
            bound_span,
            origin,
        });
    }

    /// The (type, trait) satisfaction query, cached (s14 contract).
    fn trait_satisfied(&mut self, ty: TyId, tr: &TraitRef) -> bool {
        let key = (ty, tr.module, tr.name.clone());
        if let Some(&hit) = self.sat_cache.get(&key) {
            return hit;
        }
        let ok = traits::satisfies(
            self.sigs,
            &mut self.lo.table,
            &mut self.vars,
            &self.bounds,
            ty,
            tr.module,
            &tr.name,
            SAT_DEPTH,
        );
        self.sat_cache.insert(key, ok);
        ok
    }

    /// Discharge every recorded obligation. Runs after defaulting, so
    /// inputs are as solved as they will ever be: archetypes answer
    /// from bounds (E0501, definition-site); concrete types answer by
    /// impl search (E0502, call-site — the errors name the unmet
    /// bound and never enter the callee).
    fn discharge_obligations(&mut self) {
        let obls = std::mem::take(&mut self.obligations);
        for o in obls {
            let t = zonk(&mut self.lo.table, &self.vars, o.ty);
            match self.lo.table.kind(t).clone() {
                // Unsolved: either defaulting already reported E0405,
                // or another error owns this body — one root cause,
                // one diagnostic.
                TyKind::Var(_) => continue,
                TyKind::Rigid(param) => {
                    if !self.trait_satisfied(t, &o.tr) {
                        let capability = match &o.origin {
                            OblOrigin::Instantiation { callee, param: gp } => {
                                format!("`{}`, which `{callee}` requires of `{gp}`", o.tr.name)
                            }
                            OblOrigin::Qualified { method } => {
                                format!("`{}` (needed to call `{}.{method}`)", o.tr.name, o.tr.name)
                            }
                        };
                        let mut sub = None;
                        if self.generic_info.contains_key(&param) {
                            sub = Some(o.tr.name.clone());
                        }
                        self.golden_rule_missing(o.span, &param, &capability, sub);
                        if let Some(bs) = o.bound_span
                            && let Some(d) = self.diags.last_mut()
                        {
                            *d = d
                                .clone()
                                .with_secondary(bs, "the required bound is declared here");
                        }
                    }
                }
                _ => {
                    if !self.trait_satisfied(t, &o.tr) {
                        let shown = self.show(t);
                        let what = match &o.origin {
                            OblOrigin::Instantiation { callee, param } => format!(
                                "`{shown}` does not implement `{}`, which `{callee}` requires of `{param}`",
                                o.tr.name
                            ),
                            OblOrigin::Qualified { method } => format!(
                                "`{shown}` does not implement `{}`, so `{}.{method}` cannot take it",
                                o.tr.name, o.tr.name
                            ),
                        };
                        let mut d = Diagnostic::error(codes::E0502, o.span, what)
                            .with_label(format!("this is `{shown}`"));
                        if let Some(bs) = o.bound_span {
                            d = d.with_secondary(bs, "the bound is declared here");
                        }
                        d = d.with_note(format!(
                            "write `impl {} for {shown}` in the trait's module or the \
                             type's module — or, when both are foreign, adapt: \
                             `type Local = distinct {shown}` and implement the trait \
                             for the adapter.",
                            o.tr.name
                        ));
                        self.diags.push(d);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------- blocks ----

    fn split_block<'n>(&self, b: Block<'n>) -> (Vec<&'n GreenNode>, Option<&'n GreenNode>) {
        let stmts: Vec<&GreenNode> = b.statements().collect();
        let trailing = b.trailing_expr();
        let take = if trailing.is_some() {
            stmts.len().saturating_sub(1)
        } else {
            stmts.len()
        };
        (stmts[..take].to_vec(), trailing)
    }

    fn check_block(&mut self, b: Block<'_>, exp: &Expect) -> R<()> {
        self.push_scope();
        let (stmts, trailing) = self.split_block(b);
        for s in stmts {
            self.check_stmt(s)?;
        }
        match trailing {
            Some(t) => self.check_expr(t, exp)?,
            None => {
                let unit = self.lo.table.unit();
                self.expect_unify(b.syntax().span, unit, exp);
            }
        }
        self.pop_scope();
        Ok(())
    }

    fn synth_block(&mut self, b: Block<'_>) -> R<TyId> {
        self.push_scope();
        let (stmts, trailing) = self.split_block(b);
        for s in stmts {
            self.check_stmt(s)?;
        }
        let ty = match trailing {
            Some(t) => self.synth_expr(t)?,
            None => self.lo.table.unit(),
        };
        self.pop_scope();
        Ok(ty)
    }

    /// The span best naming a block's value (for branch diagnostics).
    fn block_value_span(&self, b: Block<'_>) -> Span {
        b.trailing_expr().map(|e| e.span).unwrap_or(b.syntax().span)
    }

    // ----------------------------------------------- regions (X4) ----

    /// `region(strategy?)` — a first-class region value (s19). The
    /// strategy (arena default, `rc`, `pool(T)`) changes cost, never
    /// safety (`[mem.region.create.1]`), so the type is plain
    /// `region`; the memory checker reads the strategy from syntax.
    fn synth_region_value(&mut self, e: &GreenNode) -> R<TyId> {
        let d = RegionValue::cast(e).expect("kind");
        self.check_region_strategy(d.strategy());
        Ok(self.lo.table.intern(TyKind::RegionTy))
    }

    /// `region name? (: strategy)? { … }` — the block sugar:
    /// create + open + scope + free (`[mem.region.create.1]`). The
    /// name binds a `region`-typed value over the body; the block's
    /// value is the body's value (allocation placement is the memory
    /// checker's question, not typing's).
    fn synth_region_block(&mut self, e: &GreenNode) -> R<TyId> {
        let d = RegionBlock::cast(e).expect("kind");
        self.check_region_strategy(d.strategy());
        self.push_scope();
        if let Some(name) = d.name() {
            let region = self.lo.table.intern(TyKind::RegionTy);
            let nm = self.text(name.span);
            self.bind(nm, name.span, region);
        }
        let ty = match d.body() {
            Some(b) => self.synth_block(b)?,
            None => self.error_ty(),
        };
        self.pop_scope();
        Ok(ty)
    }

    /// `in r { … }` — rebind the ambient region over the block
    /// (`[mem.region.create.3]`): the target must be a region value;
    /// the block's value is the `in` expression's value.
    fn synth_in_block(&mut self, e: &GreenNode) -> R<TyId> {
        let d = InBlock::cast(e).expect("kind");
        if let Some(r) = d.region() {
            let region = self.lo.table.intern(TyKind::RegionTy);
            let exp = Expect {
                ty: region,
                reason: Reason::InTarget,
                because: Some(e.span),
            };
            self.check_expr(r, &exp)?;
        }
        match d.body() {
            Some(b) => self.synth_block(b),
            None => Ok(self.error_ty()),
        }
    }

    /// `freeze r` — promotion to `imm` (s20, `[mem.region.freeze.1]`).
    /// Typing only: `freeze region { … }` yields the block's value
    /// (now deeply immutable — the memory checker enforces that);
    /// `freeze r` on a region value yields `region` (the state change
    /// is the memory checker's fact, not a new type).
    fn synth_freeze(&mut self, e: &GreenNode) -> R<TyId> {
        let d = FreezeExpr::cast(e).expect("kind");
        let Some(operand) = d.expr() else {
            return Ok(self.error_ty());
        };
        if operand.kind == SyntaxKind::RegionBlock {
            return self.synth_region_block(operand);
        }
        let region = self.lo.table.intern(TyKind::RegionTy);
        let exp = Expect {
            ty: region,
            reason: Reason::FreezeTarget,
            because: Some(e.span),
        };
        self.check_expr(operand, &exp)?;
        Ok(region)
    }

    // ----------------------------------------- the unsafe tier (s22) ----

    /// `unsafe { … }` — the tier-3 ring. Typing-wise it is a plain
    /// block (its value is the body's value); the boundary rules —
    /// which operations demand the ring, what re-enters the safe
    /// world — are wolf_mem's (E1301/E1304/E1305), so typing stays
    /// permissive and the escape hatch stays *simpler* than the tier
    /// being escaped (the anti-Stacked-Borrows posture, D11).
    fn synth_unsafe(&mut self, e: &GreenNode) -> R<TyId> {
        let d = UnsafeBlock::cast(e).expect("kind");
        match d.body() {
            Some(b) => self.synth_block(b),
            None => Ok(self.error_ty()),
        }
    }

    /// `borrow r from p` — re-entry door 1 ([mem.unsafe.door]): the
    /// claim "p points into region r's live allocation" yields the
    /// pointee as a safe value governed by r's rules. Operands
    /// synthesize *loosely*: wrong operand shapes are the door-misuse
    /// catalog error (E1305, wolf_mem), never a bare type mismatch —
    /// one door, one diagnostic.
    fn synth_borrow_from(&mut self, e: &GreenNode) -> R<TyId> {
        let d = wolf_ast::BorrowExpr::cast(e).expect("kind");
        if let Some(r) = d.borrowed() {
            self.synth_expr(r)?;
        }
        let src_ty = match d.source() {
            Some(p) => self.synth_expr(p)?,
            None => self.error_ty(),
        };
        match self.kind_of(src_ty) {
            TyKind::Ptr(t) => Ok(t),
            _ => Ok(self.error_ty()),
        }
    }

    /// Validate a parsed `: rc` / `: pool(T)` strategy node: the pool
    /// element type elaborates (validation only); the strategy itself
    /// is a cost fact carried to the memory checker, not a type.
    fn check_region_strategy(&mut self, strat: Option<&GreenNode>) {
        if let Some(s) = strat
            && let Some(ty_node) = s.nodes().find(|n| is_type_kind(n.kind))
        {
            let _ = self.lower_ty(ty_node);
        }
    }

    // --------------------------------------------------- statements ----

    fn check_stmt(&mut self, s: &GreenNode) -> R<()> {
        match s.kind {
            SyntaxKind::ExprStmt => {
                if let Some(e) = ExprStmt::cast(s).and_then(|x| x.expr()) {
                    self.synth_expr(e)?;
                }
                Ok(())
            }
            SyntaxKind::AssignStmt => self.check_assign(s),
            SyntaxKind::DeferStmt => {
                let d = DeferStmt::cast(s);
                let is_err = d.is_some_and(|d| d.is_errdefer());
                // `errdefer` runs only on the error path, so it needs
                // one to exist (E0607). Capture rules match `defer`.
                if is_err && self.caller_row().is_none() && self.collect.is_none() {
                    let fn_name = self
                        .ret
                        .as_ref()
                        .map(|(_, n, _)| n.clone())
                        .unwrap_or_else(|| "this function".to_string());
                    let kw = s
                        .tokens()
                        .find(|t| t.kind == SyntaxKind::ErrdeferKw)
                        .map(|t| t.span)
                        .unwrap_or(s.span);
                    let mut diag = Diagnostic::error(
                        codes::E0607,
                        kw,
                        format!(
                            "`errdefer` runs only on the error path, but `{fn_name}` cannot fail"
                        ),
                    )
                    .with_label("this cleanup could never run");
                    if let Some((_, _, because)) = self.ret.as_ref() {
                        diag = diag.with_secondary(*because, "the signature declares no error row");
                    }
                    diag = diag
                        .with_note(
                            "use plain `defer` for cleanup that runs on every exit; keep \
                             `errdefer` only in a function with an error row.",
                        )
                        .with_suggestion(Suggestion::new(
                            "run the cleanup on every exit: `defer`",
                            vec![(kw, "defer".to_string())],
                            Applicability::Maybe,
                        ));
                    self.diags.push(diag);
                }
                if let Some(e) = d.and_then(|d| d.expr()) {
                    self.synth_expr(e)?;
                }
                // Declaration order recorded; s27 lowers the strict
                // LIFO interleave of `defer`/`errdefer` from this.
                self.cleanups.push((s.span, is_err));
                Ok(())
            }
            // s22: `assume noalias p, q` — operands synthesize
            // loosely; "not raw pointers" is the malformed-assume
            // catalog error (E1304, wolf_mem), and the `unsafe` ring
            // requirement is E1301 there too.
            SyntaxKind::AssumeStmt => {
                if let Some(a) = wolf_ast::AssumeStmt::cast(s) {
                    for op in a.exprs() {
                        self.synth_expr(op)?;
                    }
                }
                Ok(())
            }
            SyntaxKind::LetDecl => self
                .check_binding_stmt(LetDecl::cast(s).map(|d| (d.pattern(), d.ty(), d.init())), s),
            SyntaxKind::VarDecl => self
                .check_binding_stmt(VarDecl::cast(s).map(|d| (d.pattern(), d.ty(), d.init())), s),
            SyntaxKind::ConstDecl => {
                let d = ConstDecl::cast(s);
                let name = d.and_then(|c| c.name());
                let ann = d.and_then(|c| c.ty());
                let init = d.and_then(|c| c.init());
                let ty = self.binding_init(ann, init, name.map(|t| self.text(t.span)), s.span)?;
                if let Some(n) = name {
                    let nm = self.text(n.span);
                    self.bind(nm, n.span, ty);
                }
                Ok(())
            }
            // Nested items in statement position wait for s17's
            // restructuring of item environments (a nested fn is an
            // item with a signature).
            k if k.is_item() => Err(NotYet {
                construct: "a nested item declaration",
                span: s.span,
            }),
            _ => Ok(()),
        }
    }

    #[allow(clippy::type_complexity)]
    fn check_binding_stmt(
        &mut self,
        parts: Option<(Option<&GreenNode>, Option<&GreenNode>, Option<&GreenNode>)>,
        stmt: &GreenNode,
    ) -> R<()> {
        let Some((pat, ann, init)) = parts else {
            return Ok(());
        };
        let Some(pat) = pat else {
            // A recovered wreck without a pattern: walk the pieces so
            // real errors inside them surface, bind nothing, mint no
            // variable (nothing could ever pin it).
            if let Some(i) = init {
                self.synth_expr(i)?;
            }
            return Ok(());
        };
        let name = single_ident(pat, self.src());
        let ty = self.binding_init(ann, init, name, stmt.span)?;
        self.bind_pattern(pat, ty)
    }

    /// The shared `annotation? = init?` typing of let/var/const.
    fn binding_init(
        &mut self,
        ann: Option<&GreenNode>,
        init: Option<&GreenNode>,
        name: Option<String>,
        stmt_span: Span,
    ) -> R<TyId> {
        match (ann, init) {
            (Some(a), Some(i)) => {
                let ty = self.lower_ty(a);
                let exp = Expect {
                    ty,
                    reason: Reason::LetAnnotation(name.unwrap_or_else(|| "this binding".into())),
                    because: Some(a.span),
                };
                self.check_expr(i, &exp)?;
                Ok(ty)
            }
            (Some(a), None) => Ok(self.lower_ty(a)),
            (None, Some(i)) => self.synth_expr(i),
            // Neither annotation nor initializer: definite-assignment
            // analysis is c04; type-wise this is a fresh existential
            // (E0405 if no later use pins it).
            (None, None) => Ok(self.fresh(NumKind::Any, stmt_span)),
        }
    }

    fn bind_pattern(&mut self, pat: &GreenNode, ty: TyId) -> R<()> {
        match pat.kind {
            SyntaxKind::IdentPat => {
                if let Some(t) = pat.child_token(SyntaxKind::Ident) {
                    let name = self.text(t.span);
                    self.bind(name, t.span, ty);
                }
                Ok(())
            }
            SyntaxKind::WildcardPat => Ok(()),
            SyntaxKind::BindingPat => {
                if let Some(t) = pat.child_token(SyntaxKind::Ident) {
                    let name = self.text(t.span);
                    self.bind(name, t.span, ty);
                }
                for sub in pat.nodes().filter(|n| wolf_ast::is_pattern_kind(n.kind)) {
                    self.bind_pattern(sub, ty)?;
                }
                Ok(())
            }
            SyntaxKind::TuplePat => {
                let subs: Vec<&GreenNode> = pat
                    .nodes()
                    .filter(|n| wolf_ast::is_pattern_kind(n.kind))
                    .collect();
                let elem_tys: Vec<TyId> = match self.kind_of(ty) {
                    TyKind::Tuple(ts) if ts.len() == subs.len() => ts,
                    TyKind::Error => vec![self.error_ty(); subs.len()],
                    TyKind::Var(_) => {
                        let fresh: Vec<TyId> = subs
                            .iter()
                            .map(|s| self.fresh(NumKind::Any, s.span))
                            .collect();
                        let tup = self.lo.table.intern(TyKind::Tuple(fresh.clone()));
                        let exp = Expect {
                            ty: tup,
                            reason: Reason::Pattern,
                            because: Some(pat.span),
                        };
                        self.expect_unify(pat.span, ty, &exp);
                        fresh
                    }
                    _ => {
                        let shown = self.show(ty);
                        self.diags.push(
                            Diagnostic::error(
                                codes::E0401,
                                pat.span,
                                format!("this pattern unpacks a tuple, but the value is `{shown}`"),
                            )
                            .with_label("tuple pattern here")
                            .with_note(format!("expected a tuple, found `{shown}`")),
                        );
                        vec![self.error_ty(); subs.len()]
                    }
                };
                for (sub, t) in subs.into_iter().zip(elem_tys) {
                    self.bind_pattern(sub, t)?;
                }
                Ok(())
            }
            _ => {
                // E0806 — a refutable pattern where matching cannot
                // fail (s17): `let`/`var`/`for`/params/`else |p|`.
                self.diags.push(
                    Diagnostic::error(
                        codes::E0806,
                        pat.span,
                        "this pattern can fail to match, but a binding cannot".to_string(),
                    )
                    .with_label("refutable pattern")
                    .with_note(
                        "`let`, `var`, `for`, and parameters bind every value \
                         unconditionally; test and destructure with `match` \
                         instead: `match value { Pattern(x) => …, _ => … }`.",
                    ),
                );
                Ok(())
            }
        }
    }

    fn check_assign(&mut self, s: &GreenNode) -> R<()> {
        let Some(a) = AssignStmt::cast(s) else {
            return Ok(());
        };
        let Some(place) = a.place() else {
            return Ok(());
        };
        // E0410 — `let` is immutable — is the RESOLVE rung's law
        // (s29, DIV-2026-010): binding structure is lexical, so the
        // rejection fires before this checker ever runs.
        let place_ty = self.place_type(place)?;
        let place_text = self.text(place.span);
        let op = a.op().map(|t| t.kind);
        if let Some(op_kind) = op
            && op_kind != SyntaxKind::Eq
        {
            let op_text = a.op().map(|t| self.text(t.span)).unwrap_or_default();
            let (kind, needs) = match op_kind {
                SyntaxKind::PlusEq
                | SyntaxKind::MinusEq
                | SyntaxKind::StarEq
                | SyntaxKind::SlashEq
                | SyntaxKind::PercentEq => (NumKind::Num, "numbers"),
                _ => (NumKind::Integer, "integer types"),
            };
            let probe = self.fresh(kind, place.span);
            if unify(&mut self.lo.table, &mut self.vars, place_ty, probe).is_err() {
                self.report_bad_operand(place.span, &op_text, needs, place_ty);
            }
        }
        if let Some(v) = a.value() {
            let exp = Expect {
                ty: place_ty,
                reason: Reason::AssignTo(place_text),
                because: Some(place.span),
            };
            self.check_expr(v, &exp)?;
        }
        Ok(())
    }

    /// The type of an assignable place. Mutability/exclusivity are
    /// c04's checks; this is typing only.
    fn place_type(&mut self, place: &GreenNode) -> R<TyId> {
        match place.kind {
            SyntaxKind::PathExpr => {
                let Some(t) = PathExpr::cast(place).and_then(|p| p.ident()) else {
                    return Ok(self.error_ty());
                };
                let name = self.text(t.span);
                if let Some(ty) = self.lookup_local(&name) {
                    return Ok(ty);
                }
                match self.sigs.get(self.module, &name) {
                    Some(ItemSig::Global(g)) => match g.ty {
                        Some(ty) => Ok(ty),
                        None => Err(NotYet {
                            construct: "assignment to an unannotated global",
                            span: place.span,
                        }),
                    },
                    _ => Ok(self.error_ty()), // resolution already spoke
                }
            }
            SyntaxKind::MemberExpr => self.synth_member(place),
            SyntaxKind::ParenExpr => match ParenExpr::cast(place).and_then(|p| p.expr()) {
                Some(inner) => self.place_type(inner),
                None => Ok(self.error_ty()),
            },
            // s22 — a raw-pointer write target `p[i] = v`: the one
            // bracket place the raw tier adds (container-element
            // assignment stays s17's). wolf_mem gates the write to
            // `unsafe` blocks (E1301).
            SyntaxKind::BracketApply
                if BracketApply::cast(place)
                    .and_then(|b| b.callee())
                    .is_some_and(|recv| {
                        // A cheap pre-probe would re-synth the
                        // receiver; instead, peek the recorded type
                        // when the receiver is a plain local path.
                        PathExpr::cast(recv)
                            .and_then(|p| p.ident())
                            .map(|t| self.text(t.span))
                            .and_then(|n| self.lookup_local(&n))
                            .is_some_and(|ty| matches!(self.kind_of(ty), TyKind::Ptr(_)))
                    }) =>
            {
                self.synth_bracket(place)
            }
            _ => Err(NotYet {
                construct: "assignment through this place (s17)",
                span: place.span,
            }),
        }
    }

    // -------------------------------------------------- expressions ----

    fn check_expr(&mut self, e: &GreenNode, exp: &Expect) -> R<()> {
        match e.kind {
            SyntaxKind::ParenExpr => {
                if let Some(inner) = ParenExpr::cast(e).and_then(|p| p.expr()) {
                    return self.check_expr(inner, exp);
                }
                Ok(())
            }
            SyntaxKind::Block => {
                let b = Block::cast(e).expect("kind");
                self.record(e.span, exp.ty);
                self.check_block(b, exp)
            }
            // s22: `unsafe { … }` in checking position checks its body
            // against the expectation — coercions (`int` ⇒ `!int` at a
            // fn tail) work exactly as for a plain block.
            SyntaxKind::UnsafeBlock => {
                let d = UnsafeBlock::cast(e).expect("kind");
                self.record(e.span, exp.ty);
                match d.body() {
                    Some(b) => self.check_block(b, exp),
                    None => Ok(()),
                }
            }
            SyntaxKind::IfExpr => {
                self.record(e.span, exp.ty);
                self.check_if(e, exp)
            }
            SyntaxKind::MatchExpr => {
                self.record(e.span, exp.ty);
                self.check_match(e, Some(exp)).map(|_| ())
            }
            SyntaxKind::ClosureExpr => self.check_closure(e, exp),
            SyntaxKind::TupleExpr => self.check_tuple(e, exp),
            // Divergence checks itself; `Never ⇐ T` always holds.
            SyntaxKind::ReturnExpr | SyntaxKind::BreakExpr | SyntaxKind::ContinueExpr => {
                self.synth_expr(e).map(|_| ())
            }
            SyntaxKind::ElseExpr => {
                self.record(e.span, exp.ty);
                self.synth_else(e, Some(exp)).map(|_| ())
            }
            _ => {
                // The `!T` tag forms (D30/s15): a deferred
                // (capitalized, unresolved) name checks against the
                // expected row by membership — payloads pointwise,
                // missing tags named exactly (E0602).
                if let TyKind::ErrUnion(_, row) = self.kind_of(exp.ty) {
                    if let Some(span) = self.deferred_tag(e, Some(row)) {
                        let name = self.text(span);
                        self.inject_tag(span, &name, &[], row, exp.because)?;
                        self.record(span, exp.ty);
                        return Ok(());
                    }
                    if e.kind == SyntaxKind::CallExpr
                        && let Some(c) = CallExpr::cast(e)
                        && let Some(callee) = c.callee()
                        && let Some(cspan) = self.deferred_tag(callee, Some(row))
                    {
                        let name = self.text(cspan);
                        let mut args: Vec<&GreenNode> = Vec::new();
                        for a in c.args().into_iter().flat_map(|a| a.args()) {
                            if let Some(v) = Arg::value(a) {
                                args.push(v);
                            }
                        }
                        self.inject_tag(e.span, &name, &args, row, exp.because)?;
                        self.record(e.span, exp.ty);
                        return Ok(());
                    }
                }
                let t = self.synth_expr(e)?;
                self.expect_unify(e.span, t, exp);
                Ok(())
            }
        }
    }

    fn synth_expr(&mut self, e: &GreenNode) -> R<TyId> {
        let ty = self.synth_expr_inner(e)?;
        Ok(self.record(e.span, ty))
    }

    fn synth_expr_inner(&mut self, e: &GreenNode) -> R<TyId> {
        // A type-shaped node in value position (call arguments like
        // `typeinfo(*T)`): types are comptime values of kind `type`
        // (D29, s16). The node elaborates for validation only.
        if is_type_kind(e.kind) {
            let _ = self.lower_ty(e);
            return Ok(self.lo.table.intern(TyKind::TypeTy));
        }
        match e.kind {
            SyntaxKind::LiteralExpr => Ok(self.synth_literal(e)),
            SyntaxKind::StringExpr => self.synth_string(e),
            SyntaxKind::PathExpr => self.synth_path(e),
            SyntaxKind::ParenExpr => match ParenExpr::cast(e).and_then(|p| p.expr()) {
                Some(inner) => self.synth_expr(inner),
                None => Ok(self.error_ty()),
            },
            SyntaxKind::TupleExpr => {
                let d = TupleExpr::cast(e).expect("kind");
                let mut tys = Vec::new();
                for elem in d.elems() {
                    tys.push(self.synth_expr(elem)?);
                }
                Ok(self.lo.table.intern(TyKind::Tuple(tys)))
            }
            SyntaxKind::Block => self.synth_block(Block::cast(e).expect("kind")),
            SyntaxKind::PrefixExpr => self.synth_prefix(e),
            SyntaxKind::BinExpr => self.synth_bin(e),
            SyntaxKind::CastExpr => self.synth_cast(e),
            SyntaxKind::RangeExpr => self.synth_range(e),
            SyntaxKind::CallExpr => self.synth_call(e),
            SyntaxKind::MemberExpr => self.synth_member(e),
            SyntaxKind::StructLit => self.synth_struct_lit(e),
            SyntaxKind::IfExpr => self.synth_if(e),
            SyntaxKind::MatchExpr => self.check_match(e, None),
            SyntaxKind::ForExpr => self.synth_for(e),
            SyntaxKind::WhileExpr => self.synth_while(e),
            SyntaxKind::LoopExpr => self.synth_loop(e),
            SyntaxKind::ClosureExpr => self.synth_closure(e),
            SyntaxKind::ReturnExpr => self.synth_return(e),
            SyntaxKind::BreakExpr => self.synth_break(e),
            SyntaxKind::ContinueExpr => Ok(self.lo.table.never()),
            SyntaxKind::TryExpr => self.synth_try(e),
            SyntaxKind::ElseExpr => self.synth_else(e, None),
            // ---- outside the s13-checkable set: honest refusals ----
            SyntaxKind::BracketApply => self.synth_bracket(e),
            SyntaxKind::FromEndExpr => Err(NotYet {
                construct: "end-relative `^n` position (s17)",
                span: e.span,
            }),
            // ---- the region surface (X4, s19): the three creation /
            // ambient forms type here; inference and the region rules
            // live in wolf_mem. `freeze` changes what a region *is*
            // (deep imm promotion) and is s20's.
            SyntaxKind::RegionBlock => self.synth_region_block(e),
            SyntaxKind::RegionValue => self.synth_region_value(e),
            SyntaxKind::InBlock => self.synth_in_block(e),
            SyntaxKind::FreezeExpr => self.synth_freeze(e),
            SyntaxKind::ScopeExpr
            | SyntaxKind::SelectExpr
            | SyntaxKind::WhenExpr
            | SyntaxKind::SpawnExpr => Err(NotYet {
                construct: "concurrency typing (c05)",
                span: e.span,
            }),
            // ---- the unsafe tier (s22): `unsafe { }` types as its
            // body's value inside a fully safe signature
            // ([mem.unsafe.scope]); `borrow r from p` is re-entry
            // door 1 ([mem.unsafe.door]). The ring *rules* (E13xx)
            // are wolf_mem's — typing is permissive by design.
            SyntaxKind::UnsafeBlock => self.synth_unsafe(e),
            SyntaxKind::BorrowExpr => self.synth_borrow_from(e),
            // Inline C and asm have no pinned semantics yet (c10:
            // s48/s50) — only the rule that they are unsafe-tier.
            SyntaxKind::InlineC | SyntaxKind::AsmExpr => Err(NotYet {
                construct: "inline C / asm typing (c10)",
                span: e.span,
            }),
            // Broken trees type as `<error>`, silently (D22).
            _ => Ok(self.error_ty()),
        }
    }

    // ---------------------------------------------------- leaf forms ---

    /// Literals are polymorphic over the closed kinds `{integer}` /
    /// `{float}`, checked against context first; `i32`/`f64` default
    /// by rule at body end (never a solver).
    fn synth_literal(&mut self, e: &GreenNode) -> TyId {
        let Some(t) = e.tokens().next() else {
            return self.error_ty();
        };
        match t.kind {
            SyntaxKind::Int => self.fresh(NumKind::Integer, e.span),
            SyntaxKind::Float => self.fresh(NumKind::Float, e.span),
            SyntaxKind::TrueKw | SyntaxKind::FalseKw => self.lo.table.prim(Prim::Bool),
            _ => self.error_ty(),
        }
    }

    /// Whole strings are `str`. Interpolation holes synthesize and are
    /// accepted at any sized primitive / `str` type. Format specs are
    /// comptime-known (the grammar admits no computed spec except the
    /// nested-hole form, which stays an honest lane refusal), so they
    /// parse and validate HERE: a malformed spec is E0412 and a spec
    /// that does not fit the hole's type is E0413 — at the span inside
    /// the user's literal, never a runtime surprise (s38, D26, #28).
    fn synth_string(&mut self, e: &GreenNode) -> R<TyId> {
        let d = StringExpr::cast(e).expect("kind");
        for i in d.interps() {
            let mut hole_ty = None;
            if let Some(expr) = i.expr() {
                let t = self.synth_expr(expr)?;
                self.hole_ok(expr.span, t)?;
                hole_ty = Some(t);
            }
            if let Some(spec) = i.format_spec() {
                let computed = spec.nodes().any(|n| n.kind == SyntaxKind::Interp);
                // A spec's own `{…}` holes (e.g. interpolated widths).
                for nested in spec.nodes().filter_map(wolf_ast::Interp::cast) {
                    if let Some(expr) = nested.expr() {
                        let t = self.synth_expr(expr)?;
                        self.hole_ok(expr.span, t)?;
                    }
                }
                if !computed {
                    self.check_format_spec(spec, hole_ty);
                }
            }
        }
        Ok(self.lo.table.prim(Prim::Str))
    }

    /// Parse and validate one non-computed format spec against its
    /// hole's type. Diagnoses; never fails the body.
    fn check_format_spec(&mut self, spec: &GreenNode, hole_ty: Option<TyId>) {
        let text = self.text(spec.span);
        let src = text.strip_prefix(':').unwrap_or(&text);
        let parsed = match crate::fmtspec::parse(src) {
            Ok(p) => p,
            Err(err) => {
                self.diags.push(
                    Diagnostic::error(codes::E0412, spec.span, err.message())
                        .with_label("in this format spec"),
                );
                return;
            }
        };
        let Some(class) = hole_ty.and_then(|t| self.hole_class(t)) else {
            // The hole's numeric class is genuinely open (a generic
            // parameter, an error type) — the lanes stay honest about
            // anything that slips this far.
            return;
        };
        if let Err(mm) = crate::fmtspec::validate(&parsed, class) {
            self.diags.push(
                Diagnostic::error(codes::E0413, spec.span, mm.message())
                    .with_label("in this format spec"),
            );
        }
    }

    /// The format-spec class of a hole type, when it is resolved
    /// enough to have one. Unresolved numeric literals classify by
    /// their `NumKind` (an integer literal is an integer hole even
    /// before defaulting picks its width).
    fn hole_class(&self, ty: TyId) -> Option<crate::fmtspec::HoleClass> {
        use crate::fmtspec::HoleClass;
        match self.kind_of(ty) {
            TyKind::Prim(Prim::Str) => Some(HoleClass::Str),
            TyKind::Prim(Prim::Bool) => Some(HoleClass::Bool),
            TyKind::Prim(Prim::F32 | Prim::F64) => Some(HoleClass::Float),
            TyKind::Prim(_) | TyKind::Wrapping(_) => Some(HoleClass::Int),
            TyKind::Var(v) => match self.vars.kind_of(v) {
                NumKind::Integer => Some(HoleClass::Int),
                NumKind::Float => Some(HoleClass::Float),
                _ => None,
            },
            _ => None,
        }
    }

    fn hole_ok(&mut self, span: Span, ty: TyId) -> R<()> {
        match self.kind_of(ty) {
            TyKind::Prim(_)
            | TyKind::Wrapping(_)
            | TyKind::Error
            | TyKind::Never
            | TyKind::Var(_) => Ok(()),
            _ => Err(NotYet {
                construct: "string interpolation of a non-primitive value (s16/D26)",
                span,
            }),
        }
    }

    /// A capitalized name that resolves nowhere is a candidate
    /// error-row tag (D30) — resolution deferred it to us; we accept it
    /// only against an opaque `!T` row and refuse elsewhere. A
    /// *lowercase* name is a candidate only when the expected `row`
    /// declares it (#4) — or, with no expectation in sight, when the
    /// enclosing item spells it in some error row ([`Self::row_tags`]),
    /// which routes it to the honest refusal instead of silence.
    fn deferred_tag(&self, e: &GreenNode, row: Option<TyId>) -> Option<Span> {
        if e.kind != SyntaxKind::PathExpr {
            return None;
        }
        let t = PathExpr::cast(e)?.ident()?;
        let name = self.text(t.span);
        if !name.chars().next().is_some_and(char::is_uppercase) {
            let candidate = match row {
                Some(r) => self.row_declares(r, &name),
                None => self.row_tags.contains(&name),
            };
            if !candidate {
                return None;
            }
        }
        // s37 / wolf-lang#30 — a raise-position bare identifier
        // resolves against the *declared row first*: when the
        // expected row declares this tag, the tag wins over module
        // items, imports, prelude names and builtin types. Without
        // this, `return hex` inside module `hex` under `! {hex}`
        // silently returned the module value and the caller's `else`
        // never fired (the silent-wrong class). Locals and generics
        // still shadow — they are the tightest scope, and a collision
        // there is visible in the same function.
        if let Some(r) = row
            && self.row_declares(r, &name)
            && self.lookup_local(&name).is_none()
            && !self.generics.contains(&name)
            && name != "Self"
        {
            return Some(t.span);
        }
        if self.lookup_local(&name).is_some()
            || self.generics.contains(&name)
            || name == "Self"
            || self.pkg().tables[self.module].get(&name).is_some()
            || bindings_for(self.pkg(), self.module, self.file)
                .iter()
                .any(|b| b.name == name)
            || prelude::in_prelude(&name)
            || prelude::is_builtin_type(&name)
        {
            return None;
        }
        Some(t.span)
    }

    /// Does this row type declare the tag?
    fn row_declares(&self, row: TyId, name: &str) -> bool {
        match self.kind_of(row) {
            TyKind::Row { tags, .. } => tags.iter().any(|(n, _)| n == name),
            _ => false,
        }
    }

    fn synth_path(&mut self, e: &GreenNode) -> R<TyId> {
        let Some(t) = PathExpr::cast(e).and_then(|p| p.ident()) else {
            return Ok(self.error_ty());
        };
        let name = self.text(t.span);
        if let Some(ty) = self.lookup_local(&name) {
            return Ok(ty);
        }
        if name == "self" {
            // Bound as a local inside impl member bodies (s14); bare
            // elsewhere it is s17's receiver machinery.
            return Err(NotYet {
                construct: "`self` receivers (s17 methods)",
                span: e.span,
            });
        }
        if self.generics.contains(&name) || name == "Self" {
            // Types are comptime values (D29, s16): a generic
            // parameter in expression position is a `type`-kinded
            // value.
            return Ok(self.lo.table.intern(TyKind::TypeTy));
        }
        // File-scoped imports next (the resolver's order).
        for b in bindings_for(self.pkg(), self.module, self.file) {
            if b.name == name {
                return match &b.target {
                    BindTarget::Item { module, name } => {
                        self.item_value_ty(*module, &name.clone(), e.span)
                    }
                    BindTarget::PkgModule(_) | BindTarget::StdModule(_) => Err(NotYet {
                        construct: "a module namespace used as a value",
                        span: e.span,
                    }),
                    BindTarget::StdItem => Err(NotYet {
                        construct: "a std stub item (s05 std surface)",
                        span: e.span,
                    }),
                    BindTarget::CNamespace => Err(NotYet {
                        construct: "the `c` namespace (c10 FFI)",
                        span: e.span,
                    }),
                    BindTarget::Poisoned => Ok(self.error_ty()),
                };
            }
        }
        if self.pkg().tables[self.module].get(&name).is_some() {
            return self.item_value_ty(self.module, &name, e.span);
        }
        if name == "print" || name == "print_raw" {
            let str_ = self.lo.table.prim(Prim::Str);
            let unit = self.lo.table.unit();
            return Ok(self.lo.table.intern(TyKind::Fn(vec![str_], unit)));
        }
        if prelude::in_prelude(&name) {
            return Err(NotYet {
                construct: "a std/prelude stub without a signature (s05)",
                span: e.span,
            });
        }
        if prelude::is_builtin_type(&name) {
            // A builtin type name is a `type`-kinded comptime value.
            return Ok(self.lo.table.intern(TyKind::TypeTy));
        }
        if self.deferred_tag(e, None).is_some() {
            return Err(NotYet {
                construct: "an error-row tag outside `!T` context (s15)",
                span: e.span,
            });
        }
        // Unresolved: resolution already reported E0301.
        Ok(self.error_ty())
    }

    /// A module item used as a value.
    fn item_value_ty(&mut self, module: usize, name: &str, span: Span) -> R<TyId> {
        match self.sigs.get(module, name) {
            Some(ItemSig::Fn(f)) => {
                if !f.generics.is_empty() {
                    return Err(NotYet {
                        construct: "a generic function used as a value (s16 comptime)",
                        span,
                    });
                }
                let params: Vec<TyId> = f.params.iter().map(|p| p.ty).collect();
                let ret = f.ret;
                Ok(self.lo.table.intern(TyKind::Fn(params, ret)))
            }
            Some(ItemSig::Global(g)) => match g.ty {
                Some(t) => Ok(t),
                None => Err(NotYet {
                    construct: "an item without a declared type (E0407 upstream)",
                    span,
                }),
            },
            Some(
                ItemSig::Struct(_)
                | ItemSig::Enum { .. }
                | ItemSig::Alias { .. }
                | ItemSig::Distinct { .. },
            ) => {
                // Types are comptime values (D29, s16).
                Ok(self.lo.table.intern(TyKind::TypeTy))
            }
            Some(ItemSig::Trait) => Err(NotYet {
                construct: "a trait used as a value (comptime, s16)",
                span,
            }),
            None => Ok(self.error_ty()),
        }
    }

    // ------------------------------------------------------ operators --

    fn synth_prefix(&mut self, e: &GreenNode) -> R<TyId> {
        let d = PrefixExpr::cast(e).expect("kind");
        let op = d.op().map(|t| t.kind);
        let Some(operand) = d.operand() else {
            return Ok(self.error_ty());
        };
        match op {
            Some(SyntaxKind::Not) => {
                let t = self.synth_expr(operand)?;
                let bool_ = self.lo.table.prim(Prim::Bool);
                if let Some(n) = self.rigid_name(t) {
                    self.golden_rule_op(operand.span, &n, "!");
                } else if unify(&mut self.lo.table, &mut self.vars, t, bool_).is_err() {
                    self.report_bad_operand(operand.span, "!", "`bool`", t);
                }
                Ok(bool_)
            }
            Some(SyntaxKind::Minus) => {
                let t = self.synth_expr(operand)?;
                if let Some(n) = self.rigid_name(t) {
                    self.golden_rule_op(operand.span, &n, "-");
                    return Ok(self.error_ty());
                }
                let probe = self.fresh(NumKind::Num, e.span);
                if unify(&mut self.lo.table, &mut self.vars, t, probe).is_err() {
                    self.report_bad_operand(operand.span, "-", "numbers", t);
                }
                Ok(t)
            }
            Some(SyntaxKind::MoveKw | SyntaxKind::CopyKw) => self.synth_expr(operand),
            Some(SyntaxKind::Amp) => Err(NotYet {
                construct: "borrow expressions (c04)",
                span: e.span,
            }),
            Some(SyntaxKind::Star) => Err(NotYet {
                construct: "raw-pointer dereference (unsafe tier)",
                span: e.span,
            }),
            // s21: prefix `shared` builds the Tier-2 RC cell from an
            // owned value ([mem.shared.rc.1]); the cell's type wraps
            // the payload's.
            Some(SyntaxKind::SharedKw) => {
                let t = self.synth_expr(operand)?;
                Ok(self.lo.table.intern(TyKind::Shared(t)))
            }
            _ => Ok(self.error_ty()),
        }
    }

    fn synth_bin(&mut self, e: &GreenNode) -> R<TyId> {
        let d = BinExpr::cast(e).expect("kind");
        let (Some(lhs), Some(rhs)) = (d.lhs(), d.rhs()) else {
            // Broken operand: type as <error>, no cascade.
            if let Some(one) = d.lhs().or(d.rhs()) {
                self.synth_expr(one)?;
            }
            return Ok(self.error_ty());
        };
        let op_kind = d.op().map(|t| t.kind);
        let op_text = d.op().map(|t| self.text(t.span)).unwrap_or_default();
        match op_kind {
            Some(SyntaxKind::AmpAmp | SyntaxKind::PipePipe) => {
                let bool_ = self.lo.table.prim(Prim::Bool);
                let mut reported = false;
                for side in [lhs, rhs] {
                    let t = self.synth_expr(side)?;
                    if let Some(n) = self.rigid_name(t) {
                        if !reported {
                            self.golden_rule_op(side.span, &n, &op_text);
                            reported = true;
                        }
                        continue;
                    }
                    if unify(&mut self.lo.table, &mut self.vars, t, bool_).is_err() && !reported {
                        // One report per operator — the second operand's
                        // echo is the same root cause.
                        self.report_bad_operand(side.span, &op_text, "`bool`", t);
                        reported = true;
                    }
                }
                Ok(bool_)
            }
            Some(SyntaxKind::EqEq | SyntaxKind::NotEq) => {
                let lt = self.synth_expr(lhs)?;
                let rt = self.synth_expr(rhs)?;
                self.equatable(lhs.span, lt)?;
                let exp = Expect {
                    ty: lt,
                    reason: Reason::OpOperands(op_text),
                    because: Some(lhs.span),
                };
                self.expect_unify(rhs.span, rt, &exp);
                Ok(self.lo.table.prim(Prim::Bool))
            }
            Some(
                SyntaxKind::Lt
                | SyntaxKind::Gt
                | SyntaxKind::LtEq
                | SyntaxKind::GtEq
                | SyntaxKind::Spaceship,
            ) => {
                let lt = self.synth_expr(lhs)?;
                let rt = self.synth_expr(rhs)?;
                if let Some(n) = self.rigid_name(lt) {
                    self.golden_rule_op(lhs.span, &n, &op_text);
                    return Ok(self.lo.table.prim(Prim::Bool));
                }
                // `str` orders too: byte-lexicographic, total, defined
                // (the #7 ruling — lupin's byte order, no collation).
                if matches!(self.kind_of(lt), TyKind::Prim(Prim::Str)) {
                    let exp = Expect {
                        ty: lt,
                        reason: Reason::OpOperands(op_text),
                        because: Some(lhs.span),
                    };
                    self.expect_unify(rhs.span, rt, &exp);
                    return Ok(if op_kind == Some(SyntaxKind::Spaceship) {
                        self.lo.table.prim(Prim::Int)
                    } else {
                        self.lo.table.prim(Prim::Bool)
                    });
                }
                let probe = self.fresh(NumKind::Num, lhs.span);
                if unify(&mut self.lo.table, &mut self.vars, lt, probe).is_err() {
                    self.report_bad_operand(lhs.span, &op_text, "numbers", lt);
                }
                let exp = Expect {
                    ty: lt,
                    reason: Reason::OpOperands(op_text.clone()),
                    because: Some(lhs.span),
                };
                self.expect_unify(rhs.span, rt, &exp);
                if op_kind == Some(SyntaxKind::Spaceship) {
                    // `<=>` three-way compare: `int` (spec 02 owns the
                    // final ordering-type answer; int is the v0 read).
                    Ok(self.lo.table.prim(Prim::Int))
                } else {
                    Ok(self.lo.table.prim(Prim::Bool))
                }
            }
            Some(
                SyntaxKind::Plus
                | SyntaxKind::Minus
                | SyntaxKind::Star
                | SyntaxKind::Slash
                | SyntaxKind::Percent,
            ) => {
                // Checked arithmetic (X3): typing assumes nothing about
                // wrapping — `wrapping[T]` is just another number type.
                let lt = self.synth_expr(lhs)?;
                let rt = self.synth_expr(rhs)?;
                if let Some(n) = self.rigid_name(lt) {
                    self.golden_rule_op(lhs.span, &n, &op_text);
                    return Ok(self.error_ty());
                }
                let probe = self.fresh(NumKind::Num, lhs.span);
                if unify(&mut self.lo.table, &mut self.vars, lt, probe).is_err() {
                    self.report_bad_operand(lhs.span, &op_text, "numbers", lt);
                }
                let exp = Expect {
                    ty: lt,
                    reason: Reason::OpOperands(op_text),
                    because: Some(lhs.span),
                };
                self.expect_unify(rhs.span, rt, &exp);
                Ok(lt)
            }
            Some(
                SyntaxKind::Amp
                | SyntaxKind::Pipe
                | SyntaxKind::Caret
                | SyntaxKind::Shl
                | SyntaxKind::Shr,
            ) => {
                let lt = self.synth_expr(lhs)?;
                let rt = self.synth_expr(rhs)?;
                if let Some(n) = self.rigid_name(lt) {
                    self.golden_rule_op(lhs.span, &n, &op_text);
                    return Ok(self.error_ty());
                }
                let probe = self.fresh(NumKind::Integer, lhs.span);
                if unify(&mut self.lo.table, &mut self.vars, lt, probe).is_err() {
                    self.report_bad_operand(lhs.span, &op_text, "integer types", lt);
                }
                let exp = Expect {
                    ty: lt,
                    reason: Reason::OpOperands(op_text),
                    because: Some(lhs.span),
                };
                self.expect_unify(rhs.span, rt, &exp);
                Ok(lt)
            }
            _ => {
                self.synth_expr(lhs)?;
                self.synth_expr(rhs)?;
                Ok(self.error_ty())
            }
        }
    }

    /// `==`/`!=` compare the primitive family; archetypes answer from
    /// their bounds (no trait covers `==` yet — E0501, the golden
    /// rule); nominal equality waits for the operator traits (s17).
    fn equatable(&mut self, span: Span, ty: TyId) -> R<()> {
        match self.kind_of(ty) {
            TyKind::Prim(_)
            | TyKind::Wrapping(_)
            | TyKind::Var(_)
            | TyKind::Error
            | TyKind::Never => Ok(()),
            TyKind::Rigid(n) => {
                self.golden_rule_op(span, &n, "==");
                Ok(())
            }
            _ => Err(NotYet {
                construct: "`==` on non-primitive types (s17 operator traits)",
                span,
            }),
        }
    }

    fn synth_cast(&mut self, e: &GreenNode) -> R<TyId> {
        let d = CastExpr::cast(e).expect("kind");
        let src_ty = match d.expr() {
            Some(x) => self.synth_expr(x)?,
            None => self.error_ty(),
        };
        let target = match d.ty() {
            Some(t) => self.lower_ty(t),
            None => self.error_ty(),
        };
        let numeric = |k: &TyKind, vars: &VarStore| match k {
            TyKind::Prim(p) => p.is_integer() || p.is_float(),
            TyKind::Wrapping(_) => true,
            TyKind::Var(v) => matches!(
                vars.kind_of(*v),
                NumKind::Integer | NumKind::Float | NumKind::Num | NumKind::Any
            ),
            TyKind::Error | TyKind::Never => true,
            _ => false,
        };
        let sk = self.kind_of(src_ty);
        let tk = self.kind_of(target);
        if numeric(&sk, &self.vars) && numeric(&tk, &self.vars) {
            // A numeric cast: constrain an inference-var source to a
            // number, result is the target.
            if let TyKind::Var(_) = sk {
                let probe = self.fresh(NumKind::Num, e.span);
                let _ = unify(&mut self.lo.table, &mut self.vars, src_ty, probe);
            }
            self.casts.push((e.span, src_ty, target, CastKind::Numeric));
            return Ok(target);
        }
        if src_ty == target {
            self.casts
                .push((e.span, src_ty, target, CastKind::Identity));
            return Ok(target);
        }
        // Adapter casts (D28): between `type X = distinct B` and its
        // base `B`, `as` is free and bidirectional — same layout (the
        // recorded layout-identity fact; a no-op at c05 lowering).
        let src_res = self.shallow(src_ty);
        let tgt_res = self.shallow(target);
        for (adapter, other) in [(src_res, target), (tgt_res, src_ty)] {
            if let Some(bb) = self.distinct_base(adapter) {
                let snap = self.vars.snapshot();
                if unify(&mut self.lo.table, &mut self.vars, other, bb).is_ok() {
                    self.casts.push((e.span, src_ty, target, CastKind::Adapter));
                    return Ok(target);
                }
                self.vars.rollback(snap);
            }
        }
        // Raw-tier pointer bridges (s22, `[mem.prov.expose]`): typing
        // admits them anywhere — the pointer is inert data — and
        // `wolf_mem` gates the *operation* to `unsafe` blocks (E1301).
        //   *T as *U          free retyping, same tag
        //   int as *T         wildcard/exposed provenance (RFC 3559)
        //   *T as int         exposes the tag, yields the address
        //   region as *T      the region's backing base pointer
        let src_ptr = matches!(sk, TyKind::Ptr(_));
        let tgt_ptr = matches!(tk, TyKind::Ptr(_));
        if src_ptr || tgt_ptr {
            let int_side = |k: &TyKind, vars: &VarStore| match k {
                TyKind::Prim(p) => p.is_integer(),
                TyKind::Var(v) => matches!(
                    vars.kind_of(*v),
                    NumKind::Integer | NumKind::Num | NumKind::Any
                ),
                _ => false,
            };
            let bridged = (src_ptr && tgt_ptr)
                || (src_ptr && int_side(&tk, &self.vars))
                || (tgt_ptr && int_side(&sk, &self.vars))
                || (tgt_ptr && matches!(sk, TyKind::RegionTy));
            if bridged {
                // An integer side that is still an inference var pins
                // to an integer (the address is data, never a float).
                for (side_ptr, side_ty) in [(src_ptr, target), (tgt_ptr, src_ty)] {
                    if !side_ptr {
                        continue;
                    }
                    if let TyKind::Var(_) = self.kind_of(side_ty) {
                        let probe = self.fresh(NumKind::Integer, e.span);
                        let _ = unify(&mut self.lo.table, &mut self.vars, side_ty, probe);
                    }
                }
                self.casts.push((e.span, src_ty, target, CastKind::Raw));
                return Ok(target);
            }
        }
        // E0805 — outside the closed cast set (s17). Typed as the
        // target so one report stands alone.
        let shown_s = self.show(src_ty);
        let shown_t = self.show(target);
        let is_bool = |k: &TyKind| matches!(k, TyKind::Prim(Prim::Bool));
        let is_str = |k: &TyKind| matches!(k, TyKind::Prim(Prim::Str));
        let mut d = Diagnostic::error(
            codes::E0805,
            e.span,
            format!("`{shown_s}` does not cast to `{shown_t}`"),
        )
        .with_label("outside the cast set");
        d = if is_bool(&sk) && numeric(&tk, &self.vars) {
            d.with_note(
                "there is no truthiness bridge: write the value out, e.g. \
                 `if b { 1 } else { 0 }`.",
            )
        } else if numeric(&sk, &self.vars) && is_bool(&tk) {
            d.with_note("compare instead: `x != 0` is the `bool` you meant.")
        } else if is_str(&tk) {
            d.with_note("build strings with interpolation: \"{value}\" formats any primitive.")
        } else if is_str(&sk) && numeric(&tk, &self.vars) {
            d.with_note(
                "parsing is an ordinary function, not a cast — reach for the \
                 conversion the standard library names.",
            )
        } else if self.distinct_base(src_res).is_some()
            && self.distinct_base(src_res) == self.distinct_base(tgt_res)
        {
            d.with_note(
                "the two adapters share a base but not an identity; cast through \
                 the base: `x as Base as Other`.",
            )
        } else {
            d.with_note(
                "`as` converts between numeric types, and between an adapter \
                 (`type X = distinct B`) and its base — nothing else; build the \
                 value with the operation that names it.",
            )
        };
        self.diags.push(d);
        Ok(target)
    }

    /// The recorded base of an adapter type (`type X = distinct B`).
    fn distinct_base(&self, ty: TyId) -> Option<TyId> {
        if let TyKind::Nominal { module, name } = self.lo.table.kind(ty)
            && let Some(ItemSig::Distinct { base, .. }) = self.sigs.get(*module as usize, name)
        {
            return Some(*base);
        }
        None
    }

    fn synth_range(&mut self, e: &GreenNode) -> R<TyId> {
        let d = RangeExpr::cast(e).expect("kind");
        let endpoints: Vec<&GreenNode> = d.endpoints().collect();
        if endpoints.len() != 2 || endpoints.iter().any(|n| n.kind == SyntaxKind::FromEndExpr) {
            return Err(NotYet {
                construct: "open-ended or end-relative ranges (s17 slicing)",
                span: e.span,
            });
        }
        // Ranges iterate integers (the builtin closed family, D25).
        let elem = self.fresh(NumKind::Integer, endpoints[0].span);
        let first_span = endpoints[0].span;
        for ep in endpoints {
            let t = self.synth_expr(ep)?;
            let exp = Expect {
                ty: elem,
                reason: Reason::ForRange,
                because: Some(first_span),
            };
            self.expect_unify(ep.span, t, &exp);
        }
        Ok(self.lo.table.intern(TyKind::Range(elem)))
    }

    // ---------------------------------------------- `?` / `else` (s15) --

    /// Postfix `?` (D30): unwrap the ok half, propagate the error into
    /// the enclosing function's row — a width check plus re-tagging by
    /// injection, never a conversion.
    fn synth_try(&mut self, e: &GreenNode) -> R<TyId> {
        let d = wolf_ast::TryExpr::cast(e).expect("kind");
        let Some(operand) = d.expr() else {
            return Ok(self.error_ty());
        };
        let t = self.synth_expr(operand)?;
        match self.kind_of(t) {
            TyKind::ErrUnion(ok, row) => {
                if self.in_closure {
                    return Err(NotYet {
                        construct: "`?` inside a closure (s17 closure rows)",
                        span: e.span,
                    });
                }
                // Every `?` is an error-trace point ([abi.err.trace];
                // the runtime buffer is s32).
                self.trace_points.push(e.span);
                match self.caller_row() {
                    Some(crow) => {
                        let because = self.ret.as_ref().map(|(_, _, b)| *b);
                        self.require_row_widening(e.span, row, crow, because);
                    }
                    None => self.report_nonfallible_try(e.span, row),
                }
                Ok(ok)
            }
            TyKind::Error | TyKind::Never => Ok(self.error_ty()),
            // A numeric-kinded variable is a number whatever it
            // solves to — infallible for sure. Only a fully unknown
            // type is honestly deferred.
            TyKind::Var(v) if matches!(self.vars.kind_of(v), NumKind::Any) => Err(NotYet {
                construct: "`?` on a value whose type is still being inferred",
                span: operand.span,
            }),
            _ => {
                let shown = self.show(t);
                self.diags.push(
                    Diagnostic::error(
                        codes::E0603,
                        e.span,
                        format!("`?` needs a fallible operand, but this is `{shown}`"),
                    )
                    .with_label(format!("`{shown}` cannot fail"))
                    .with_note(
                        "`?` unwraps a `!T` value, propagating its error; this value \
                         has no error row — delete the `?`, or check the callee's \
                         signature if you expected it to be fallible.",
                    ),
                );
                Ok(t)
            }
        }
    }

    /// E0604 — `?` in a function whose signature admits no errors.
    fn report_nonfallible_try(&mut self, span: Span, callee_row: TyId) {
        if self.collect.is_some() {
            return;
        }
        let fn_name = self
            .ret
            .as_ref()
            .map(|(_, n, _)| n.clone())
            .unwrap_or_else(|| "this function".to_string());
        let mut d = Diagnostic::error(
            codes::E0604,
            span,
            format!("`?` propagates an error, but `{fn_name}` cannot fail"),
        )
        .with_label("the error would have nowhere to go");
        if let Some((_, _, because)) = self.ret.as_ref() {
            d = d.with_secondary(*because, "the signature declares no error row");
        }
        // When the callee's tags are concrete, offer the exact row.
        let row = self.resolve_row(callee_row);
        if let TyKind::Row { tags, tail: None } = self.lo.table.kind(row).clone()
            && !tags.is_empty()
            && let Some(RowFix::AddRow { insert_at }) = self.row_fix
        {
            let rendered: Vec<String> = tags.iter().map(|(n, p)| self.tag_str(n, p)).collect();
            d = d.with_suggestion(Suggestion::new(
                format!("make `{fn_name}` fallible: `! {{{}}}`", rendered.join(", ")),
                vec![(insert_at, format!(" ! {{{}}}", rendered.join(", ")))],
                Applicability::Maybe,
            ));
        }
        d = d.with_note(
            "errors travel in the declared row, never a side channel: make the \
             function fallible (`-> !T`, or an explicit row), or handle the \
             error here with `else`.",
        );
        self.diags.push(d);
    }

    /// Postfix `else` (D30): defaulting (`expr else fallback`) and the
    /// handler form (`expr else |err| body`, the handler binding the
    /// row value). The result is the fallible value's ok type.
    fn synth_else(&mut self, e: &GreenNode, exp: Option<&Expect>) -> R<TyId> {
        let d = wolf_ast::ElseExpr::cast(e).expect("kind");
        let Some(scrut) = d.scrutinized() else {
            return Ok(self.error_ty());
        };
        let t = self.synth_expr(scrut)?;
        match self.kind_of(t) {
            TyKind::ErrUnion(ok, row) => {
                // `else` observation is an error-trace point
                // ([abi.err.trace]).
                self.trace_points.push(e.span);
                self.push_scope();
                if let Some(pat) = d.handler_pattern() {
                    // The caught error's type is the row itself.
                    self.bind_pattern(pat, row)?;
                }
                if let Some(fb) = d.fallback() {
                    let fexp = Expect {
                        ty: ok,
                        reason: Reason::ElseFallback,
                        because: Some(scrut.span),
                    };
                    self.check_expr(fb, &fexp)?;
                }
                self.pop_scope();
                if let Some(exp) = exp {
                    self.expect_unify(e.span, ok, exp);
                }
                Ok(ok)
            }
            TyKind::Error | TyKind::Never => {
                if let Some(fb) = d.fallback() {
                    self.push_scope();
                    self.synth_expr(fb)?;
                    self.pop_scope();
                }
                Ok(self.error_ty())
            }
            TyKind::Var(v) if matches!(self.vars.kind_of(v), NumKind::Any) => Err(NotYet {
                construct: "`else` on a value whose type is still being inferred",
                span: scrut.span,
            }),
            _ => {
                let shown = self.show(t);
                self.diags.push(
                    Diagnostic::error(
                        codes::E0608,
                        e.span,
                        format!(
                            "`else` defaulting needs a fallible operand, but this is `{shown}`"
                        ),
                    )
                    .with_label(format!("`{shown}` cannot fail, so the `else` never fires"))
                    .with_note(
                        "postfix `else` substitutes a fallback for a `!T` value's \
                         error; this value has no error row — delete the `else`, or \
                         attach it to the fallible call itself.",
                    ),
                );
                if let Some(fb) = d.fallback() {
                    self.push_scope();
                    self.synth_expr(fb)?;
                    self.pop_scope();
                }
                if let Some(exp) = exp {
                    self.expect_unify(e.span, t, exp);
                }
                Ok(t)
            }
        }
    }

    // ------------------------------------- the Tier-2 surface (s21) ---

    /// `e[…]` — indexing on the s21 builtin containers. `List[T]`
    /// indexes by `int` (OOB is the `bounds` trap, never UB);
    /// `pool[h]` / `pool[mut h]` index by `handle T` and are the
    /// checked slot access of `[mem.shared.handle.3]` (staleness is
    /// the `stale-handle` trap — dynamic by contract, X5). Generic
    /// application in value position (`first[int]`) stays refused.
    fn synth_bracket(&mut self, e: &GreenNode) -> R<TyId> {
        let d = BracketApply::cast(e).expect("kind");
        let Some(recv) = d.callee() else {
            return Ok(self.error_ty());
        };
        let recv_ty = self.synth_expr(recv)?;
        let elem = match self.kind_of(recv_ty) {
            TyKind::List(elem) => {
                let int_ = self.lo.table.prim(Prim::Int);
                self.check_index_arg(d.args(), int_, "List index")?;
                elem
            }
            TyKind::Pool(elem) => {
                let h = self.lo.table.intern(TyKind::Handle(elem));
                self.check_index_arg(d.args(), h, "pool access")?;
                elem
            }
            // s22 — raw-pointer indexing `p[i]`: C-shaped element
            // access, no bounds, no validity ([mem.unsafe.raw.1]).
            // Typing admits it; wolf_mem gates the access to `unsafe`
            // blocks (E1301).
            TyKind::Ptr(elem) => {
                let int_ = self.lo.table.prim(Prim::Int);
                self.check_index_arg(d.args(), int_, "raw pointer index")?;
                elem
            }
            // s37 — `s[a..b]` byte-offset checked slicing (D25).
            TyKind::Prim(Prim::Str) => return self.str_bracket(e, d.args()),
            TyKind::Error | TyKind::Never => self.error_ty(),
            _ => {
                return Err(NotYet {
                    construct: "indexing / generic application (s17)",
                    span: e.span,
                });
            }
        };
        Ok(elem)
    }

    /// `s[a..b]` — zero-copy byte-offset slicing, checked (D25): the
    /// endpoints type as `int`, `^n` counts from the end, open ends
    /// default to the string's edges. **No `s[i]`** — a single index
    /// cannot honestly name "a character", so E0411 refuses with the
    /// real alternatives (slice / `bytes()` / `find` offsets), and
    /// the Python reflex `s[-1]` gets a fix-it to `s[^1]`.
    fn str_bracket(&mut self, e: &GreenNode, args: Option<ArgList<'_>>) -> R<TyId> {
        let str_ = self.lo.table.prim(Prim::Str);
        let int_ = self.lo.table.prim(Prim::Int);
        let arg_nodes: Vec<&GreenNode> = args
            .into_iter()
            .flat_map(|a| a.args())
            .filter_map(Arg::value)
            .collect();
        let [one] = arg_nodes.as_slice() else {
            return Err(NotYet {
                construct: "this `str` index shape (one range argument, s37)",
                span: e.span,
            });
        };
        if one.kind == SyntaxKind::RangeExpr {
            let d = RangeExpr::cast(one).expect("kind");
            for ep in d.endpoints() {
                let inner = if ep.kind == SyntaxKind::FromEndExpr {
                    FromEndExpr::cast(ep).and_then(|f| f.expr())
                } else {
                    Some(ep)
                };
                if let Some(x) = inner {
                    let exp = Expect {
                        ty: int_,
                        reason: Reason::ArgOfCall {
                            callee: "str slicing".to_string(),
                            index: 0,
                        },
                        because: None,
                    };
                    self.check_expr(x, &exp)?;
                }
            }
            return Ok(str_);
        }
        // `s[^n]` — a single end-relative position is still character
        // indexing; the slice forms are the honest spelling.
        if one.kind == SyntaxKind::FromEndExpr {
            if let Some(inner) = FromEndExpr::cast(one).and_then(|f| f.expr()) {
                let exp = Expect {
                    ty: int_,
                    reason: Reason::ArgOfCall {
                        callee: "str slicing".to_string(),
                        index: 0,
                    },
                    because: None,
                };
                self.check_expr(inner, &exp)?;
            }
            let digits = self.text(one.span);
            self.diags.push(
                Diagnostic::error(
                    codes::E0411,
                    e.span,
                    "`str` has no character indexing".to_string(),
                )
                .with_label(format!("no `s[{digits}]` on `str`"))
                .with_note(
                    "a single position cannot honestly name a character (D25); \
                     the end-relative slices are `s[^n..]` (last `n` bytes) and \
                     `s[..^n]` (all but the last `n`).",
                ),
            );
            return Ok(self.error_ty());
        }
        // `s[i]` — no character indexing exists, by decision (D25).
        self.synth_expr(one)?;
        self.diags.push(
            Diagnostic::error(
                codes::E0411,
                e.span,
                "`str` has no character indexing".to_string(),
            )
            .with_label("no `s[i]` on `str`")
            .with_note(
                "byte offsets are the honest currency (D25): slice with \
                 `s[a..b]` (or the recoverable `s.get(a..b)`), read bytes via \
                 `s.bytes()`, or use `find` — it returns byte offsets.",
            ),
        );
        Ok(self.error_ty())
    }

    /// The single index argument of a container `e[…]`, checked
    /// against the expected index type. The X1 `mut` marker
    /// (`pool[mut h]`) is legal surface; the exclusivity behind it is
    /// wolf_mem's. Type-only arguments (`channel[region]` shapes) are
    /// not indexing and refuse.
    fn check_index_arg(&mut self, args: Option<ArgList<'_>>, want: TyId, what: &str) -> R<()> {
        let arg_nodes: Vec<_> = args.into_iter().flat_map(|a| a.args()).collect();
        for (i, a) in arg_nodes.iter().enumerate() {
            let Some(v) = Arg::value(*a) else { continue };
            if is_type_kind(v.kind) {
                return Err(NotYet {
                    construct: "type arguments in bracket position (s17)",
                    span: v.span,
                });
            }
            if i == 0 && arg_nodes.len() == 1 {
                let exp = Expect {
                    ty: want,
                    reason: Reason::ArgOfCall {
                        callee: what.to_string(),
                        index: 0,
                    },
                    because: None,
                };
                self.check_expr(v, &exp)?;
            } else {
                self.synth_expr(v)?;
            }
        }
        Ok(())
    }

    /// `List[int]()` / `Pool[Node]()` — the container constructors
    /// (s21). Zero value arguments; the bracket carries one type
    /// argument. Recorded as a ctor call so the memory checker sites
    /// the container's allocation ([mem.region.create.3]).
    fn call_container_ctor(
        &mut self,
        name: &str,
        b: BracketApply<'_>,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        let ty_nodes: Vec<_> = b
            .args()
            .into_iter()
            .flat_map(|a| a.args())
            .filter_map(Arg::value)
            .collect();
        let elem = match ty_nodes.as_slice() {
            [one] => match self.type_from_bracket_arg(one) {
                Some(t) => t,
                None => {
                    return Err(NotYet {
                        construct: "this prelude container instantiation (s16 generic data)",
                        span: e.span,
                    });
                }
            },
            _ => {
                return Err(NotYet {
                    construct: "this prelude container instantiation (s16 generic data)",
                    span: e.span,
                });
            }
        };
        let arg_nodes: Vec<_> = args.into_iter().flat_map(|a| a.args()).collect();
        if !arg_nodes.is_empty() {
            self.wrong_arg_count(name, e.span, None, 0, arg_nodes.len());
            if let Some(a) = args {
                self.synth_args_loosely(a)?;
            }
        }
        self.calls.push((
            e.span,
            CallSig {
                callee: name.to_string(),
                decl_span: None,
                has_self: false,
                ctor: true,
                params: Vec::new(),
                c_call: false,
            },
        ));
        Ok(match name {
            "List" => self.lo.table.intern(TyKind::List(elem)),
            _ => self.lo.table.intern(TyKind::Pool(elem)),
        })
    }

    /// A bracket argument read as a *type* (D29: `e[…]` is one
    /// postfix shape, so `Pool[Node]`'s element parses as an
    /// expression — sema decides). Type-kind nodes lower directly
    /// (`List[handle Node]`); a bare path resolves as a type head
    /// (prims, generics, named items). Anything else: not a type.
    fn type_from_bracket_arg(&mut self, node: &GreenNode) -> Option<TyId> {
        if is_type_kind(node.kind) {
            return Some(self.lower_ty(node));
        }
        if node.kind != SyntaxKind::PathExpr {
            return None;
        }
        let t = PathExpr::cast(node)?.ident()?;
        let name = self.text(t.span);
        if self.lookup_local(&name).is_some() {
            return None;
        }
        if let Some(p) = Prim::from_name(&name) {
            return Some(self.lo.table.prim(p));
        }
        if self.generics.contains(&name) {
            return Some(self.lo.table.intern(TyKind::Rigid(name)));
        }
        match self
            .lo
            .resolve_type_head(self.module, self.file, &[(name, t.span)])
        {
            crate::sig::TypeHead::Item { module, name } => {
                Some(self.lo.named_item_type(module, &name, self.file, node))
            }
            _ => None,
        }
    }

    /// Methods on the Tier-2 builtins: `shared`/`weak` cells
    /// ([mem.shared.rc]) and the prelude containers. Typed stubs —
    /// exactly the surface the memory corpus rests on; the real std
    /// API (capacity, iteration adapters) is s37's. Receivers carry
    /// their honest X1 modes (#6): the mutators (`push`, `reserve`,
    /// `init`, `remove`) take `mut self`, so call sites spell
    /// `(mut xs).push(v)`; the readers stay bare.
    #[allow(clippy::too_many_arguments)]
    fn tier2_method_call(
        &mut self,
        base: &GreenNode,
        recv_ty: TyId,
        recv_mode: Option<wolf_ast::ParamMode>,
        member_span: Span,
        mname: &str,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        let p = |name: &str, ty: TyId| ParamSig {
            name: name.to_string(),
            ty,
            span: member_span,
            mode: None,
            view: None,
        };
        // `mut self` — the mutating receivers.
        let pm = |name: &str, ty: TyId| ParamSig {
            name: name.to_string(),
            ty,
            span: member_span,
            mode: Some(wolf_ast::ParamMode::Mut),
            view: None,
        };
        let (params, ret) = match (self.kind_of(recv_ty), mname) {
            // `[mem.shared.rc.1]` clones share ownership — the dup site.
            (TyKind::Shared(t), "clone") => {
                let r = self.lo.table.intern(TyKind::Shared(t));
                (vec![p("self", recv_ty)], r)
            }
            (TyKind::Shared(t), "downgrade") => {
                let r = self.lo.table.intern(TyKind::Weak(t));
                (vec![p("self", recv_ty)], r)
            }
            // `[mem.shared.rc.3]` upgrade is option-shaped: `T ! {gone}`
            // — the caller must handle the miss (`else`, `?`, `match`).
            (TyKind::Weak(t), "upgrade") => {
                let row = self
                    .lo
                    .table
                    .row(vec![("gone".to_string(), Vec::new())], None);
                let r = self.lo.table.intern(TyKind::ErrUnion(t, row));
                (vec![p("self", recv_ty)], r)
            }
            (TyKind::List(t), "push") => {
                let u = self.lo.table.unit();
                (vec![pm("self", recv_ty), p("value", t)], u)
            }
            // s37 — List method depth: the recoverable element reads
            // (`{none}` rows, the D25 posture — absence is a row, not
            // a sentinel or a trap), the emptiness probe, and the
            // drain/clear mutators.
            (TyKind::List(t), "pop") => {
                let row = self
                    .lo
                    .table
                    .row(vec![("none".to_string(), Vec::new())], None);
                let r = self.lo.table.intern(TyKind::ErrUnion(t, row));
                (vec![pm("self", recv_ty)], r)
            }
            (TyKind::List(t), "get") => {
                let int_ = self.lo.table.prim(Prim::Int);
                let row = self
                    .lo
                    .table
                    .row(vec![("none".to_string(), Vec::new())], None);
                let r = self.lo.table.intern(TyKind::ErrUnion(t, row));
                (vec![p("self", recv_ty), p("index", int_)], r)
            }
            (TyKind::List(t), "first" | "last") => {
                let row = self
                    .lo
                    .table
                    .row(vec![("none".to_string(), Vec::new())], None);
                let r = self.lo.table.intern(TyKind::ErrUnion(t, row));
                (vec![p("self", recv_ty)], r)
            }
            (TyKind::List(_), "is_empty") => {
                let b = self.lo.table.prim(Prim::Bool);
                (vec![p("self", recv_ty)], b)
            }
            (TyKind::List(_), "count") => {
                let int_ = self.lo.table.prim(Prim::Int);
                (vec![p("self", recv_ty)], int_)
            }
            (TyKind::List(_), "clear") => {
                let u = self.lo.table.unit();
                (vec![pm("self", recv_ty)], u)
            }
            // s37 — Pool observability (the wolf-lang#11 gap: no
            // length, no liveness probe): `len`/`is_empty` count live
            // slots; `alive` is the non-trapping handle probe.
            (TyKind::Pool(_), "len") => {
                let int_ = self.lo.table.prim(Prim::Int);
                (vec![p("self", recv_ty)], int_)
            }
            (TyKind::Pool(_), "is_empty") => {
                let b = self.lo.table.prim(Prim::Bool);
                (vec![p("self", recv_ty)], b)
            }
            (TyKind::Pool(t), "alive") => {
                let h = self.lo.table.intern(TyKind::Handle(t));
                let b = self.lo.table.prim(Prim::Bool);
                (vec![p("self", recv_ty), p("handle", h)], b)
            }
            // `[mem.shared.handle.1]` two-phase: reserve yields the
            // handle, init fills the slot. No null handles exist.
            (TyKind::Pool(t), "reserve") => {
                let r = self.lo.table.intern(TyKind::Handle(t));
                (vec![pm("self", recv_ty)], r)
            }
            (TyKind::Pool(t), "init") => {
                let h = self.lo.table.intern(TyKind::Handle(t));
                let u = self.lo.table.unit();
                (vec![pm("self", recv_ty), p("handle", h), p("value", t)], u)
            }
            (TyKind::Pool(t), "remove") => {
                let h = self.lo.table.intern(TyKind::Handle(t));
                let u = self.lo.table.unit();
                (vec![pm("self", recv_ty), p("handle", h)], u)
            }
            _ => {
                return Err(NotYet {
                    construct: "methods on generic std data (s05 std surface)",
                    span: e.span,
                });
            }
        };
        self.dispatch.push((
            e.span,
            Dispatch::Inherent {
                ty: self.show(recv_ty),
                method: mname.to_string(),
            },
        ));
        let sig = FnSig {
            params,
            ret,
            name_span: member_span,
            ret_span: None,
            row_span: None,
            generics: Vec::new(),
            comptime: false,
            trusted: None,
        };
        self.dispatch_method(
            mname,
            &sig,
            BTreeMap::new(),
            member_span,
            base,
            recv_ty,
            recv_mode,
            e,
            args,
        )
    }

    /// s37 — the builtin `str` method surface (D24/D25). Typed stubs
    /// like the Tier-2 containers': `str` is an immutable two-word
    /// UTF-8 slice, so every receiver is a plain read and every
    /// offset is a *byte* offset (D25 — the one honest currency).
    /// The load-bearing entry is `get`: `s.get(a..b) -> str ! {none}`
    /// — the recoverable slice, the boundary primitive wolf-std's
    /// filings named (wolf-lang#17): the row-shaped twin of the
    /// checked `s[a..b]` fault, and the one primitive a library
    /// cannot write for itself (writing it needs itself). Misses —
    /// out-of-range or split-code-point offsets, absent needles —
    /// are `{none}` rows, never traps. The views (`bytes`, `split`,
    /// `words`, `lines`) type as materialized `List`s at v0; the
    /// zero-copy iterator views arrive with the D28 protocol's
    /// builtin adoption (tracked in the s37 ledger). No `chars()`
    /// yet: it wants a `char` type, which is its own spec item.
    #[allow(clippy::too_many_arguments)]
    fn str_method_call(
        &mut self,
        base: &GreenNode,
        recv_ty: TyId,
        recv_mode: Option<wolf_ast::ParamMode>,
        member_span: Span,
        mname: &str,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        let p = |name: &str, ty: TyId| ParamSig {
            name: name.to_string(),
            ty,
            span: member_span,
            mode: None,
            view: None,
        };
        let str_ = self.lo.table.prim(Prim::Str);
        let int_ = self.lo.table.prim(Prim::Int);
        let bool_ = self.lo.table.prim(Prim::Bool);
        let none_of = |zelf: &mut Self, ok: TyId| {
            let row = zelf
                .lo
                .table
                .row(vec![("none".to_string(), Vec::new())], None);
            zelf.lo.table.intern(TyKind::ErrUnion(ok, row))
        };
        let (params, ret) = match mname {
            "is_empty" => (vec![p("self", recv_ty)], bool_),
            // The boundary primitive (D25's anti-landmine form).
            "get" => {
                let r = self.lo.table.intern(TyKind::Range(int_));
                let ret = none_of(self, str_);
                (vec![p("self", recv_ty), p("range", r)], ret)
            }
            // The byte view, materialized at v0 (D25 licenses byte
            // indexing on `bytes`; `b[i]` rides List indexing).
            "bytes" => {
                let ret = self.lo.table.intern(TyKind::List(int_));
                (vec![p("self", recv_ty)], ret)
            }
            "starts_with" | "ends_with" | "contains" => {
                (vec![p("self", recv_ty), p("needle", str_)], bool_)
            }
            // Byte offsets out; absence is a row, not a sentinel.
            "find" | "rfind" => {
                let ret = none_of(self, int_);
                (vec![p("self", recv_ty), p("needle", str_)], ret)
            }
            "count" => (vec![p("self", recv_ty), p("needle", str_)], int_),
            "split" => {
                let ret = self.lo.table.intern(TyKind::List(str_));
                (vec![p("self", recv_ty), p("sep", str_)], ret)
            }
            "words" | "lines" => {
                let ret = self.lo.table.intern(TyKind::List(str_));
                (vec![p("self", recv_ty)], ret)
            }
            "trim" | "trim_start" | "trim_end" | "lower" | "upper" => {
                (vec![p("self", recv_ty)], str_)
            }
            "strip_prefix" | "strip_suffix" => {
                let ret = none_of(self, str_);
                (vec![p("self", recv_ty), p("needle", str_)], ret)
            }
            "repeat" => (vec![p("self", recv_ty), p("count", int_)], str_),
            "replace" => (
                vec![p("self", recv_ty), p("from", str_), p("to", str_)],
                str_,
            ),
            _ => {
                return Err(NotYet {
                    construct: "this `str` method (outside the s37 builtin set)",
                    span: e.span,
                });
            }
        };
        self.dispatch.push((
            e.span,
            Dispatch::Inherent {
                ty: self.show(recv_ty),
                method: mname.to_string(),
            },
        ));
        let sig = FnSig {
            params,
            ret,
            name_span: member_span,
            ret_span: None,
            row_span: None,
            generics: Vec::new(),
            comptime: false,
            trusted: None,
        };
        self.dispatch_method(
            mname,
            &sig,
            BTreeMap::new(),
            member_span,
            base,
            recv_ty,
            recv_mode,
            e,
            args,
        )
    }

    /// s22 — the builtin method surface of `*T` ([mem.unsafe.raw],
    /// spec/02 §6). The strict-provenance ops exist so unsafe code can
    /// be written *without* wildcard provenance (RFC 3559's shape):
    /// `addr`/`with_addr` never expose; `expose`/`with_exposed` are
    /// the documented int-round-trip escape ([mem.prov.expose], same
    /// semantics as the `as` casts). `is_null` is a plain comparison.
    /// Typing admits them anywhere; wolf_mem gates the provenance ops
    /// to `unsafe` blocks (E1301).
    #[allow(clippy::too_many_arguments)]
    fn ptr_method_call(
        &mut self,
        base: &GreenNode,
        recv_ty: TyId,
        recv_mode: Option<wolf_ast::ParamMode>,
        member_span: Span,
        mname: &str,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        let p = |name: &str, ty: TyId| ParamSig {
            name: name.to_string(),
            ty,
            span: member_span,
            mode: None,
            view: None,
        };
        let uint_ = self.lo.table.prim(Prim::Uint);
        let bool_ = self.lo.table.prim(Prim::Bool);
        let (params, ret) = match mname {
            "is_null" => (vec![p("self", recv_ty)], bool_),
            // p.addr() — the address, provenance *not* exposed.
            "addr" => (vec![p("self", recv_ty)], uint_),
            // p.with_addr(a) — new address, same provenance.
            "with_addr" => (vec![p("self", recv_ty), p("addr", uint_)], recv_ty),
            // p.expose() / p.with_exposed(a) — the exposed round-trip.
            "expose" => (vec![p("self", recv_ty)], uint_),
            "with_exposed" => (vec![p("self", recv_ty), p("addr", uint_)], recv_ty),
            _ => {
                return Err(NotYet {
                    construct: "this raw-pointer operation (the s22 surface is \
                                is_null/addr/with_addr/expose/with_exposed)",
                    span: e.span,
                });
            }
        };
        self.dispatch.push((
            e.span,
            Dispatch::Inherent {
                ty: self.show(recv_ty),
                method: mname.to_string(),
            },
        ));
        let sig = FnSig {
            params,
            ret,
            name_span: member_span,
            ret_span: None,
            row_span: None,
            generics: Vec::new(),
            comptime: false,
            trusted: None,
        };
        self.dispatch_method(
            mname,
            &sig,
            BTreeMap::new(),
            member_span,
            base,
            recv_ty,
            recv_mode,
            e,
            args,
        )
    }

    // ---------------------------------------------------------- calls --

    fn synth_args_loosely(&mut self, args: ArgList<'_>) -> R<()> {
        for a in args.args() {
            if let Some(v) = Arg::value(a) {
                self.synth_expr(v)?;
            }
        }
        Ok(())
    }

    fn synth_call(&mut self, e: &GreenNode) -> R<TyId> {
        let d = CallExpr::cast(e).expect("kind");
        let Some(callee) = d.callee() else {
            return Ok(self.error_ty());
        };
        // Named-function calls: path or namespace-qualified path.
        if callee.kind == SyntaxKind::PathExpr {
            let t = PathExpr::cast(callee).and_then(|p| p.ident());
            if let Some(t) = t {
                let name = self.text(t.span);
                if self.lookup_local(&name).is_none() {
                    // print/print_raw builtin signature; the stderr
                    // writers type identically (s38 io v0).
                    if matches!(
                        name.as_str(),
                        "print" | "print_raw" | "eprint" | "eprint_raw"
                    ) {
                        return self.call_print(&name, e, d.args());
                    }
                    // Ambient host surfaces (s16): typed here, callable
                    // at runtime, categorically refused at comptime by
                    // the D33 sandbox.
                    if crate::ctfe::intrinsics::host_stub(&name).is_some() {
                        return self.call_host_stub(&name, e, d.args());
                    }
                    // The comptime intrinsics allowlist (D29/D33).
                    if let Some(i) = crate::ctfe::intrinsics::intrinsic(&name) {
                        return self.call_intrinsic(&name, i, e, d.args());
                    }
                    if let Some((module, item)) = self.named_fn_target(&name) {
                        return self.call_named(&item, module, callee.span, e, d.args());
                    }
                    if self.deferred_tag(callee, None).is_some() {
                        return Err(NotYet {
                            construct: "an error-row tag outside `!T` context (s15)",
                            span: callee.span,
                        });
                    }
                }
            }
        }
        if callee.kind == SyntaxKind::MemberExpr
            && let Some(m) = MemberExpr::cast(callee)
        {
            // `Trait.method(args)` / `ns.Trait.method(args)` — the s14
            // qualified call (always available; the disambiguation
            // E0803 points at).
            if let Some((tr, member_span, mname)) = self.qualified_trait_method(m) {
                return self.call_trait_method(&tr, callee.span, &mname, member_span, e, d.args());
            }
            // `c.malloc(…)` — a call through the `import c` namespace
            // (s22): the modelled intrinsic set types here; the full
            // header importer is c10's.
            if let Some((mname, member_span)) = self.c_namespace_member(m) {
                return self.c_call(&mname, member_span, e, d.args());
            }
            if let Some((module, item)) = self.namespace_member(m) {
                return self.call_named(&item, module, callee.span, e, d.args());
            }
            // `Type.Variant(args)` / `Type.assoc(args)` — enum
            // construction and inherent statics (s17).
            if let Some(res) = self.type_member_call(m, e, d.args())? {
                return Ok(res);
            }
            // `recv.method(args)` — the s17 receiver call.
            return self.method_call(m, e, d.args());
        }
        // `List[int]()` / `Pool[Node]()` — the s21 container ctors:
        // a bracket-applied prelude head in call position.
        if callee.kind == SyntaxKind::BracketApply
            && let Some(b) = BracketApply::cast(callee)
            && let Some(head) = b.callee()
            && head.kind == SyntaxKind::PathExpr
            && let Some(t) = PathExpr::cast(head).and_then(|pp| pp.ident())
        {
            let name = self.text(t.span);
            if self.lookup_local(&name).is_none() && matches!(name.as_str(), "List" | "Pool") {
                return self.call_container_ctor(&name, b, e, d.args());
            }
        }
        // Otherwise: call through the callee's type (closures, fn-typed
        // locals/fields, higher-order parameters).
        let callee_ty = self.synth_expr(callee)?;
        self.call_by_type(callee_ty, callee, e, d.args())
    }

    /// A bare name that resolves to a function item (module item or
    /// item import), if any.
    fn named_fn_target(&self, name: &str) -> Option<(usize, String)> {
        for b in bindings_for(self.pkg(), self.module, self.file) {
            if b.name == name {
                if let BindTarget::Item { module, name } = &b.target {
                    return Some((*module, name.clone()));
                }
                return None;
            }
        }
        if self.pkg().tables[self.module].get(name).is_some() {
            return Some((self.module, name.to_string()));
        }
        None
    }

    /// The trait a bare name resolves to (module item or item
    /// import), if it names one.
    fn trait_target(&self, name: &str) -> Option<TraitRef> {
        if self.lookup_local(name).is_some() {
            return None;
        }
        for b in bindings_for(self.pkg(), self.module, self.file) {
            if b.name == name {
                if let BindTarget::Item { module, name } = &b.target {
                    let tr = TraitRef {
                        module: *module,
                        name: name.clone(),
                    };
                    if self.sigs.traits.contains_key(&tr) {
                        return Some(tr);
                    }
                }
                return None;
            }
        }
        let tr = TraitRef {
            module: self.module,
            name: name.to_string(),
        };
        if self.sigs.traits.contains_key(&tr) {
            return Some(tr);
        }
        None
    }

    /// A callee of the shape `Trait.method` or `ns.Trait.method`:
    /// (trait, method-name span, method name).
    fn qualified_trait_method(&self, m: MemberExpr<'_>) -> Option<(TraitRef, Span, String)> {
        let member = m.member()?;
        let base = m.base()?;
        let tr = if base.kind == SyntaxKind::PathExpr {
            let t = PathExpr::cast(base)?.ident()?;
            self.trait_target(&self.text(t.span))?
        } else if base.kind == SyntaxKind::MemberExpr {
            let inner = MemberExpr::cast(base)?;
            let (module, name) = self.namespace_member(inner)?;
            let tr = TraitRef { module, name };
            if !self.sigs.traits.contains_key(&tr) {
                return None;
            }
            tr
        } else {
            return None;
        };
        Some((tr, member.span, self.text(member.span)))
    }

    /// The s14 qualified trait-method call: instantiate `Self` (and
    /// the method's own generics) as fresh existentials, let the
    /// *arguments* solve them (inputs select the impl; outputs never
    /// drive inference), and record the bound obligation.
    fn call_trait_method(
        &mut self,
        tr: &TraitRef,
        callee_span: Span,
        mname: &str,
        member_span: Span,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        let Some(td) = self.sigs.traits.get(tr) else {
            return Ok(self.error_ty());
        };
        if !td.params.is_empty() {
            return Err(NotYet {
                construct: "qualified calls on parameterized traits (bracket-argument surface)",
                span: callee_span,
            });
        }
        let Some(method) = td.method(mname).cloned() else {
            let names: Vec<&str> = td.methods.iter().map(|m| m.name.as_str()).collect();
            let mut d = Diagnostic::error(
                codes::E0403,
                member_span,
                format!("`{}` has no method named `{mname}`", tr.name),
            )
            .with_label("unknown trait method")
            .with_secondary(td.name_span, format!("`{}` is declared here", tr.name));
            if let Some(hit) = wolf_diag::suggest::best_match(mname, &names) {
                d = d.with_suggestion(Suggestion::new(
                    format!("did you mean `{hit}`?"),
                    vec![(member_span, hit.to_string())],
                    Applicability::Maybe,
                ));
            }
            self.diags.push(d);
            if let Some(a) = args {
                self.synth_args_loosely(a)?;
            }
            return Ok(self.error_ty());
        };
        let full_name = format!("{}.{mname}", tr.name);
        let mut map: BTreeMap<String, TyId> = BTreeMap::new();
        let self_var = self.fresh(NumKind::Any, callee_span);
        map.insert("Self".to_string(), self_var);
        for g in &method.sig.generics {
            let v = self.fresh(NumKind::Any, e.span);
            map.insert(g.name.clone(), v);
            for b in &g.bounds {
                self.obligate(
                    v,
                    TraitRef {
                        module: b.module,
                        name: b.name.clone(),
                    },
                    e.span,
                    Some(b.span),
                    OblOrigin::Instantiation {
                        callee: full_name.clone(),
                        param: g.name.clone(),
                    },
                );
            }
        }
        let arg_nodes: Vec<_> = args.into_iter().flat_map(|a| a.args()).collect();
        if arg_nodes.len() != method.sig.params.len() {
            self.wrong_arg_count(
                &full_name,
                e.span,
                Some(method.name_span),
                method.sig.params.len(),
                arg_nodes.len(),
            );
        }
        self.calls.push((
            e.span,
            CallSig {
                callee: full_name.clone(),
                decl_span: Some(method.name_span),
                has_self: false, // qualified: the receiver is argument 0
                ctor: false,
                params: method.sig.params.clone(),
                c_call: false,
            },
        ));
        // Blame span for the Self obligation: the argument that pins
        // `Self` (call-site errors point at the argument, D28).
        let mut self_blame = callee_span;
        for (i, arg) in arg_nodes.iter().enumerate() {
            let Some(v) = Arg::value(*arg) else { continue };
            match method.sig.params.get(i) {
                Some(p) => {
                    if self_blame == callee_span
                        && traits::mentions_rigid(&self.lo.table, p.ty, "Self")
                    {
                        self_blame = v.span;
                    }
                    let pty = subst(&mut self.lo.table, p.ty, &map);
                    let exp = Expect {
                        ty: pty,
                        reason: Reason::ArgOfCall {
                            callee: full_name.clone(),
                            index: i,
                        },
                        because: Some(p.span),
                    };
                    self.check_expr(v, &exp)?;
                }
                None => {
                    self.synth_expr(v)?;
                }
            }
        }
        self.obligate(
            self_var,
            tr.clone(),
            self_blame,
            Some(td.name_span),
            OblOrigin::Qualified {
                method: mname.to_string(),
            },
        );
        let ret = subst(&mut self.lo.table, method.sig.ret, &map);
        Ok(self.normalize(ret))
    }

    /// Normalize concrete projections (outputs) — after inputs are
    /// solved, never before.
    fn normalize(&mut self, ty: TyId) -> TyId {
        traits::normalize_projections(self.sigs, &mut self.lo.table, &mut self.vars, ty)
    }

    /// `ns.member` where `ns` is a file binding to a package module.
    fn namespace_member(&self, m: MemberExpr<'_>) -> Option<(usize, String)> {
        let base = m.base()?;
        let t = PathExpr::cast(base)?.ident()?;
        let name = self.text(t.span);
        if self.lookup_local(&name).is_some() {
            return None;
        }
        let b = bindings_for(self.pkg(), self.module, self.file)
            .iter()
            .find(|b| b.name == name)?;
        let BindTarget::PkgModule(module) = b.target else {
            return None;
        };
        let member = m.member()?;
        Some((module, self.text(member.span)))
    }

    /// `c.name` where `c` is the contextual namespace an `import c`
    /// declaration binds (s22; header contents are c10's).
    fn c_namespace_member(&self, m: MemberExpr<'_>) -> Option<(String, Span)> {
        let base = m.base()?;
        let t = PathExpr::cast(base)?.ident()?;
        let name = self.text(t.span);
        if self.lookup_local(&name).is_some() {
            return None;
        }
        let b = bindings_for(self.pkg(), self.module, self.file)
            .iter()
            .find(|b| b.name == name)?;
        if b.target != BindTarget::CNamespace {
            return None;
        }
        let member = m.member()?;
        Some((self.text(member.span), member.span))
    }

    /// A call through the `c` namespace (s22). The typed set is
    /// exactly the five allocator/memory intrinsics the is04 oracle
    /// models (`c.malloc`, `c.calloc`, `c.free`, `c.memset`,
    /// `c.memcpy` — its approximation contract §8); every other
    /// imported name waits for the c10 header importer, honestly.
    /// Results and pointer parameters are `*u8` — C's `void*`/`char*`
    /// floor — retyped by `as` where needed ([mem.unsafe.raw.1]).
    /// The `unsafe`-ring requirement is wolf_mem's E1301.
    fn c_call(
        &mut self,
        mname: &str,
        member_span: Span,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        let uint_ = self.lo.table.prim(Prim::Uint);
        let int_ = self.lo.table.prim(Prim::Int);
        let u8p = {
            let u8_ = self.lo.table.prim(Prim::U8);
            self.lo.table.intern(TyKind::Ptr(u8_))
        };
        let unit = self.lo.table.unit();
        let (param_tys, ret): (Vec<TyId>, TyId) = match mname {
            "malloc" => (vec![uint_], u8p),
            "calloc" => (vec![uint_, uint_], u8p),
            "free" => (vec![u8p], unit),
            "memset" => (vec![u8p, int_, uint_], u8p),
            "memcpy" => (vec![u8p, u8p, uint_], u8p),
            _ => {
                return Err(NotYet {
                    construct: "imported C beyond the modelled intrinsic set (c10's \
                                header importer)",
                    span: e.span,
                });
            }
        };
        let label = format!("c.{mname}");
        let params: Vec<ParamSig> = param_tys
            .iter()
            .enumerate()
            .map(|(i, &ty)| ParamSig {
                name: format!("a{i}"),
                ty,
                span: member_span,
                mode: None,
                view: None,
            })
            .collect();
        self.calls.push((
            e.span,
            CallSig {
                callee: label.clone(),
                decl_span: None,
                has_self: false,
                ctor: false,
                params,
                c_call: true,
            },
        ));
        self.call_fixed(&label, &param_tys, ret, e, args)
    }

    fn call_print(&mut self, name: &str, e: &GreenNode, args: Option<ArgList<'_>>) -> R<TyId> {
        let arg_nodes: Vec<_> = args.into_iter().flat_map(|a| a.args()).collect();
        if arg_nodes.len() != 1 {
            self.wrong_arg_count(name, e.span, None, 1, arg_nodes.len());
            if let Some(a) = args {
                self.synth_args_loosely(a)?;
            }
            return Ok(self.lo.table.unit());
        }
        if let Some(v) = Arg::value(arg_nodes[0]) {
            let str_ = self.lo.table.prim(Prim::Str);
            let exp = Expect {
                ty: str_,
                reason: Reason::ArgOfCall {
                    callee: name.to_string(),
                    index: 0,
                },
                because: None,
            };
            self.check_expr(v, &exp)?;
        }
        Ok(self.lo.table.unit())
    }

    /// Register a root comptime call site (s16). Sites inside a
    /// `comptime fn` body are the evaluator's to walk, and the
    /// row-collection pass never evaluates.
    fn note_comptime_call(&mut self, span: Span) {
        if !self.in_comptime_fn && self.collect.is_none() {
            self.comptime_calls.push(span);
        }
    }

    /// An ambient host stub's builtin signature (s16/s38). These type
    /// like `print`: real signatures so runtime code checks, with the
    /// comptime sandbox refusing every one of them by category.
    ///
    /// The s38 io/fs tier speaks D30: errors are payload rows, never
    /// sentinels or traps — `not_found`/`denied`/`io` for the
    /// filesystem family, `utf8` where text decoding can fail, `eof`
    /// where an end is an outcome. (`eof`/`utf8` are exactly the tags
    /// wolf-std's taxonomy reserved for the io tier.) Payload-carrying
    /// forms (`NotFound{path}`) are the recorded stdc02 retrofit.
    fn call_host_stub(&mut self, name: &str, e: &GreenNode, args: Option<ArgList<'_>>) -> R<TyId> {
        let str_ = self.lo.table.prim(Prim::Str);
        let int_ = self.lo.table.prim(Prim::Int);
        let bool_ = self.lo.table.prim(Prim::Bool);
        let unit = self.lo.table.unit();
        let rowed = |zelf: &mut Self, ok: TyId, tags: &[&str]| {
            let row = zelf.lo.table.row(
                tags.iter().map(|t| (t.to_string(), Vec::new())).collect(),
                None,
            );
            zelf.lo.table.intern(TyKind::ErrUnion(ok, row))
        };
        let (params, ret): (Vec<TyId>, TyId) = match name {
            "read_text" | "net_fetch" | "env_var" => (vec![str_], str_),
            "clock_ms" | "random_seed" => (Vec::new(), int_),
            "read_line" => (Vec::new(), rowed(self, str_, &["eof", "io", "utf8"])),
            "fs_read_text" => (
                vec![str_],
                rowed(self, str_, &["not_found", "denied", "io", "utf8"]),
            ),
            "fs_write_text" => (
                vec![str_, str_],
                rowed(self, unit, &["not_found", "denied", "io"]),
            ),
            "fs_open" => (
                vec![str_],
                rowed(self, int_, &["not_found", "denied", "io"]),
            ),
            "fs_create" => (vec![str_], rowed(self, int_, &["denied", "io"])),
            "fs_read" => (vec![int_, int_], rowed(self, str_, &["eof", "io", "utf8"])),
            "fs_write" => (vec![int_, str_], rowed(self, unit, &["io"])),
            "fs_close" => (vec![int_], rowed(self, unit, &["io"])),
            "fs_remove" => (
                vec![str_],
                rowed(self, unit, &["not_found", "denied", "io"]),
            ),
            "fs_exists" => (vec![str_], bool_),
            // The s39 net builtin tier (blocking TCP v0): the row
            // vocabulary is {refused, timeout, closed, io} — `closed`
            // is the peer's finish (the socket `eof`), `timeout` is
            // declared now so the vocabulary is stable when s35/s40
            // deadlines make it routinely reachable. A forged or
            // wrong-kind fd is `io`, never a trap.
            "net_listen" => (vec![str_], rowed(self, int_, &["io"])),
            "net_port" => (vec![int_], rowed(self, int_, &["io"])),
            "net_accept" => (vec![int_], rowed(self, int_, &["timeout", "io"])),
            "net_connect" => (vec![str_], rowed(self, int_, &["refused", "timeout", "io"])),
            "net_read" => (
                vec![int_, int_],
                rowed(self, str_, &["closed", "timeout", "utf8", "io"]),
            ),
            "net_write" => (vec![int_, str_], rowed(self, unit, &["closed", "io"])),
            "net_close" => (vec![int_], rowed(self, unit, &["io"])),
            _ => (Vec::new(), self.error_ty()),
        };
        self.call_fixed(name, &params, ret, e, args)
    }

    /// A comptime intrinsic call (the D33 allowlist): `typeinfo` /
    /// `reflect`, `typebuild`, `implements`, `size_of`, `assert`.
    fn call_intrinsic(
        &mut self,
        name: &str,
        i: crate::ctfe::intrinsics::Intrinsic,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        use crate::ctfe::intrinsics::Intrinsic as I;
        let type_ty = self.lo.table.intern(TyKind::TypeTy);
        let bool_ = self.lo.table.prim(Prim::Bool);
        let int_ = self.lo.table.prim(Prim::Int);
        let unit = self.lo.table.unit();
        match i {
            I::Assert => {
                // Runtime asserts stay runtime ([conf.trap.map]); the
                // evaluator owns comptime failures (E0710). One or two
                // arguments: the condition, and an optional `str`
                // message carried into the failure report (#9).
                let str_ = self.lo.table.prim(Prim::Str);
                let arg_nodes: Vec<_> = args.into_iter().flat_map(|a| a.args()).collect();
                if arg_nodes.is_empty() || arg_nodes.len() > 2 {
                    self.diags.push(
                        Diagnostic::error(
                            codes::E0402,
                            e.span,
                            format!(
                                "`{name}` takes 1 or 2 arguments, but this call passes {}",
                                arg_nodes.len()
                            ),
                        )
                        .with_label(format!(
                            "{} argument{} here",
                            arg_nodes.len(),
                            if arg_nodes.len() == 1 { "" } else { "s" }
                        ))
                        .with_note(
                            "`assert(condition)` checks the condition; \
                             `assert(condition, message)` adds a `str` shown on failure.",
                        ),
                    );
                }
                for (i, arg) in arg_nodes.iter().enumerate() {
                    let Some(v) = Arg::value(*arg) else { continue };
                    match i {
                        0 | 1 => {
                            let exp = Expect {
                                ty: if i == 0 { bool_ } else { str_ },
                                reason: Reason::ArgOfCall {
                                    callee: name.to_string(),
                                    index: i,
                                },
                                because: None,
                            };
                            self.check_expr(v, &exp)?;
                        }
                        _ => {
                            self.synth_expr(v)?;
                        }
                    }
                }
                Ok(unit)
            }
            I::TypeInfo => {
                self.note_comptime_call(e.span);
                let meta = self.lo.table.intern(TyKind::Meta(MetaTy::TypeInfo));
                self.call_fixed(name, &[type_ty], meta, e, args)
            }
            I::SizeOf => {
                self.note_comptime_call(e.span);
                self.call_fixed(name, &[type_ty], int_, e, args)
            }
            I::TypeBuild => {
                // The descriptor is a comptime aggregate; its shape is
                // the evaluator's contract, not a surface type.
                self.note_comptime_call(e.span);
                if let Some(a) = args {
                    self.synth_args_loosely(a)?;
                }
                Ok(type_ty)
            }
            I::Implements => {
                self.note_comptime_call(e.span);
                let arg_nodes: Vec<_> = args.into_iter().flat_map(|a| a.args()).collect();
                if arg_nodes.len() != 2 {
                    self.wrong_arg_count(name, e.span, None, 2, arg_nodes.len());
                    for a in &arg_nodes {
                        if let Some(v) = Arg::value(*a) {
                            self.synth_expr(v)?;
                        }
                    }
                    return Ok(bool_);
                }
                if let Some(v) = Arg::value(arg_nodes[0]) {
                    let exp = Expect {
                        ty: type_ty,
                        reason: Reason::ArgOfCall {
                            callee: name.to_string(),
                            index: 0,
                        },
                        because: None,
                    };
                    self.check_expr(v, &exp)?;
                }
                // The second argument names a trait — semantic value,
                // never syntax (the D29 amendment).
                let trait_ok = Arg::value(arg_nodes[1])
                    .filter(|v| v.kind == SyntaxKind::PathExpr)
                    .and_then(|v| PathExpr::cast(v).and_then(|p| p.ident()))
                    .map(|t| self.text(t.span))
                    .is_some_and(|n| self.trait_target(&n).is_some());
                if !trait_ok {
                    return Err(NotYet {
                        construct: "an `implements` argument that does not name a trait",
                        span: arg_nodes[1].syntax().span,
                    });
                }
                Ok(bool_)
            }
        }
    }

    /// Check a call against a fixed (non-generic) builtin signature.
    fn call_fixed(
        &mut self,
        name: &str,
        params: &[TyId],
        ret: TyId,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        let arg_nodes: Vec<_> = args.into_iter().flat_map(|a| a.args()).collect();
        if arg_nodes.len() != params.len() {
            self.wrong_arg_count(name, e.span, None, params.len(), arg_nodes.len());
        }
        for (i, arg) in arg_nodes.iter().enumerate() {
            let Some(v) = Arg::value(*arg) else { continue };
            match params.get(i) {
                Some(&p) => {
                    let exp = Expect {
                        ty: p,
                        reason: Reason::ArgOfCall {
                            callee: name.to_string(),
                            index: i,
                        },
                        because: None,
                    };
                    self.check_expr(v, &exp)?;
                }
                None => {
                    self.synth_expr(v)?;
                }
            }
        }
        Ok(ret)
    }

    fn call_named(
        &mut self,
        name: &str,
        module: usize,
        callee_span: Span,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        match self.sigs.get(module, name).cloned() {
            Some(ItemSig::Fn(sig)) => self.call_by_sig(name, &sig, e, args),
            Some(
                ItemSig::Struct(_)
                | ItemSig::Enum { .. }
                | ItemSig::Alias { .. }
                | ItemSig::Distinct { .. },
            ) => {
                self.not_callable(callee_span, &format!("the type `{name}`"));
                if let Some(a) = args {
                    self.synth_args_loosely(a)?;
                }
                Ok(self.error_ty())
            }
            Some(ItemSig::Global(g)) => {
                let ty = match g.ty {
                    Some(t) => t,
                    None => {
                        return Err(NotYet {
                            construct: "an item without a declared type (E0407 upstream)",
                            span: callee_span,
                        });
                    }
                };
                let callee = e
                    .nodes()
                    .find(|n| is_expr_kind(n.kind))
                    .expect("callee exists");
                self.call_by_type(ty, callee, e, args)
            }
            Some(ItemSig::Trait) => {
                self.not_callable(callee_span, &format!("the trait `{name}`"));
                if let Some(a) = args {
                    self.synth_args_loosely(a)?;
                }
                Ok(self.error_ty())
            }
            None => {
                if let Some(a) = args {
                    self.synth_args_loosely(a)?;
                }
                Ok(self.error_ty())
            }
        }
    }

    fn call_by_sig(
        &mut self,
        name: &str,
        sig: &FnSig,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        // A call to a `comptime fn` from runtime code is a root
        // comptime site: the ctfe pass evaluates it (D29, s16).
        if sig.comptime {
            self.note_comptime_call(e.span);
        }
        self.calls.push((
            e.span,
            CallSig {
                callee: name.to_string(),
                decl_span: Some(sig.name_span),
                has_self: false,
                ctor: false,
                params: sig.params.clone(),
                c_call: false,
            },
        ));
        // s14 instantiation: each generic parameter becomes a fresh
        // existential; the *arguments* solve them, and the solutions
        // are checked against the bounds only (never the callee's
        // body — the golden rule made that impossible to need).
        let mut map: BTreeMap<String, TyId> = BTreeMap::new();
        for g in &sig.generics {
            let v = self.fresh(NumKind::Any, e.span);
            map.insert(g.name.clone(), v);
        }
        let arg_nodes: Vec<_> = args.into_iter().flat_map(|a| a.args()).collect();
        if arg_nodes.len() != sig.params.len() {
            self.wrong_arg_count(
                name,
                e.span,
                Some(sig.name_span),
                sig.params.len(),
                arg_nodes.len(),
            );
        }
        // Call-site blame spans: the first argument whose declared
        // parameter mentions the generic parameter.
        let mut blames: BTreeMap<String, Span> = BTreeMap::new();
        for (i, arg) in arg_nodes.iter().enumerate() {
            let Some(v) = Arg::value(*arg) else { continue };
            match sig.params.get(i) {
                Some(p) => {
                    for g in &sig.generics {
                        if !blames.contains_key(&g.name)
                            && traits::mentions_rigid(&self.lo.table, p.ty, &g.name)
                        {
                            blames.insert(g.name.clone(), v.span);
                        }
                    }
                    let pty = if map.is_empty() {
                        p.ty
                    } else {
                        subst(&mut self.lo.table, p.ty, &map)
                    };
                    let exp = Expect {
                        ty: pty,
                        reason: Reason::ArgOfCall {
                            callee: name.to_string(),
                            index: i,
                        },
                        because: Some(p.span),
                    };
                    self.check_expr(v, &exp)?;
                }
                None => {
                    self.synth_expr(v)?;
                }
            }
        }
        for g in &sig.generics {
            let v = map[&g.name];
            let blame = blames.get(&g.name).copied().unwrap_or(e.span);
            for b in &g.bounds {
                self.obligate(
                    v,
                    TraitRef {
                        module: b.module,
                        name: b.name.clone(),
                    },
                    blame,
                    Some(b.span),
                    OblOrigin::Instantiation {
                        callee: name.to_string(),
                        param: g.name.clone(),
                    },
                );
            }
        }
        if map.is_empty() {
            Ok(sig.ret)
        } else {
            let ret = subst(&mut self.lo.table, sig.ret, &map);
            Ok(self.normalize(ret))
        }
    }

    fn call_by_type(
        &mut self,
        callee_ty: TyId,
        callee: &GreenNode,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        let arg_nodes: Vec<_> = args.into_iter().flat_map(|a| a.args()).collect();
        match self.kind_of(callee_ty) {
            TyKind::Fn(params, ret) => {
                if arg_nodes.len() != params.len() {
                    let name = self.text(callee.span);
                    self.wrong_arg_count(&name, e.span, None, params.len(), arg_nodes.len());
                }
                let callee_name = self.text(callee.span);
                self.calls.push((
                    e.span,
                    CallSig {
                        callee: callee_name.clone(),
                        decl_span: None,
                        has_self: false,
                        ctor: false,
                        // A `fn` type cannot declare modes: every
                        // parameter is `read` (s18 rejects call-site
                        // `mut`/`take` against these).
                        params: params
                            .iter()
                            .map(|&ty| crate::sig::ParamSig {
                                name: String::new(),
                                ty,
                                span: callee.span,
                                mode: None,
                                view: None,
                            })
                            .collect(),
                        c_call: false,
                    },
                ));
                for (i, arg) in arg_nodes.iter().enumerate() {
                    let Some(v) = Arg::value(*arg) else { continue };
                    match params.get(i) {
                        Some(&p) => {
                            let exp = Expect {
                                ty: p,
                                reason: Reason::ArgOfCall {
                                    callee: callee_name.clone(),
                                    index: i,
                                },
                                because: Some(callee.span),
                            };
                            self.check_expr(v, &exp)?;
                        }
                        None => {
                            self.synth_expr(v)?;
                        }
                    }
                }
                Ok(ret)
            }
            TyKind::Var(_) => {
                // DK-style: calling an unsolved existential shapes it
                // into a function type (closure params from context).
                // Arguments unify into the parameter slots *first*, so
                // the occurs check runs on the find-resolved form —
                // `g(g)` fails here with the cycle rendered (E0404).
                let mut params = Vec::new();
                for arg in &arg_nodes {
                    let Some(v) = Arg::value(*arg) else { continue };
                    let t = self.synth_expr(v)?;
                    let p = self.fresh(NumKind::Any, v.span);
                    let _ = unify(&mut self.lo.table, &mut self.vars, t, p);
                    params.push(p);
                }
                let ret = self.fresh(NumKind::Any, e.span);
                let fnty = self.lo.table.intern(TyKind::Fn(params, ret));
                if let Err(err) = unify(&mut self.lo.table, &mut self.vars, callee_ty, fnty) {
                    let exp = Expect {
                        ty: fnty,
                        reason: Reason::OpOperands("call".to_string()),
                        because: None,
                    };
                    self.report_unify_err(callee.span, callee_ty, &exp, err);
                    return Ok(self.error_ty());
                }
                Ok(ret)
            }
            TyKind::Error | TyKind::Never => {
                if let Some(a) = args {
                    self.synth_args_loosely(a)?;
                }
                Ok(self.error_ty())
            }
            TyKind::Rigid(n) => {
                // Golden rule: calling an archetype value needs a
                // capability no bound can grant yet.
                self.golden_rule_missing(callee.span, &n, "a callable interface", None);
                if let Some(a) = args {
                    self.synth_args_loosely(a)?;
                }
                Ok(self.error_ty())
            }
            _ => {
                let shown = self.show(callee_ty);
                self.not_callable(callee.span, &format!("`{shown}`"));
                if let Some(a) = args {
                    self.synth_args_loosely(a)?;
                }
                Ok(self.error_ty())
            }
        }
    }

    /// E0402.
    fn wrong_arg_count(
        &mut self,
        name: &str,
        span: Span,
        decl: Option<Span>,
        wants: usize,
        got: usize,
    ) {
        let mut d = Diagnostic::error(
            codes::E0402,
            span,
            format!(
                "`{name}` takes {wants} argument{}, but this call passes {got}",
                if wants == 1 { "" } else { "s" }
            ),
        )
        .with_label(format!(
            "{got} argument{} here",
            if got == 1 { "" } else { "s" }
        ));
        if let Some(decl) = decl {
            d = d.with_secondary(decl, format!("`{name}` is defined here"));
        }
        if got > wants {
            d = d.with_note("remove the extra arguments, or check the argument order.");
        } else {
            d = d.with_note("add the missing arguments — every parameter is required.");
        }
        self.diags.push(d);
    }

    /// E0406.
    fn not_callable(&mut self, span: Span, what: &str) {
        self.diags.push(
            Diagnostic::error(
                codes::E0406,
                span,
                format!("{what} is not a function, so it cannot be called"),
            )
            .with_label("called here")
            .with_note(
                "only functions and closures accept `(…)`; construct a struct \
                 with `Name { field: value }` braces instead.",
            ),
        );
    }

    // ---------------------------------------------- method calls (s17) --

    /// `Type.Member(args)`: enum-variant construction (`Color.Rgb(…)`)
    /// or an inherent associated function. `Ok(None)` ⇒ the callee is
    /// not a type member; fall through to receiver resolution.
    fn type_member_call(
        &mut self,
        m: MemberExpr<'_>,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<Option<TyId>> {
        let Some((module, tyname)) = self.type_target(m) else {
            return Ok(None);
        };
        let Some(member) = m.member() else {
            return Ok(None);
        };
        let mname = self.text(member.span);
        match self.sigs.get(module, &tyname).cloned() {
            Some(ItemSig::Enum {
                generic,
                variants,
                name_span,
            }) => {
                if generic {
                    return Err(NotYet {
                        construct: "a generic enum's variants (s16 generic data)",
                        span: e.span,
                    });
                }
                let Some(v) = variants.iter().find(|v| v.name == mname) else {
                    let names: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
                    let mut d = Diagnostic::error(
                        codes::E0403,
                        member.span,
                        format!("`{tyname}` has no variant named `{mname}`"),
                    )
                    .with_label("unknown variant")
                    .with_secondary(name_span, format!("`{tyname}` is defined here"));
                    if let Some(hit) = wolf_diag::suggest::best_match(&mname, &names) {
                        d = d.with_suggestion(Suggestion::new(
                            format!("did you mean `{hit}`?"),
                            vec![(member.span, hit.to_string())],
                            Applicability::Maybe,
                        ));
                    } else if !names.is_empty() {
                        d = d.with_note(format!("the variants are: {}", names.join(", ")));
                    }
                    self.diags.push(d);
                    if let Some(a) = args {
                        self.synth_args_loosely(a)?;
                    }
                    return Ok(Some(self.error_ty()));
                };
                let label = format!("{tyname}.{mname}");
                let payload = v.payload.clone();
                let v_span = v.span;
                let arg_nodes: Vec<_> = args.into_iter().flat_map(|a| a.args()).collect();
                if arg_nodes.len() != payload.len() {
                    self.wrong_arg_count(
                        &label,
                        e.span,
                        Some(v_span),
                        payload.len(),
                        arg_nodes.len(),
                    );
                }
                self.calls.push((
                    e.span,
                    CallSig {
                        callee: label.clone(),
                        decl_span: Some(v_span),
                        has_self: false,
                        ctor: true, // payloads move in; no call-site modes
                        params: payload
                            .iter()
                            .map(|&ty| crate::sig::ParamSig {
                                name: String::new(),
                                ty,
                                span: v_span,
                                mode: None,
                                view: None,
                            })
                            .collect(),
                        c_call: false,
                    },
                ));
                for (i, arg) in arg_nodes.iter().enumerate() {
                    let Some(val) = Arg::value(*arg) else {
                        continue;
                    };
                    match payload.get(i) {
                        Some(&p) => {
                            let exp = Expect {
                                ty: p,
                                reason: Reason::ArgOfCall {
                                    callee: label.clone(),
                                    index: i,
                                },
                                because: Some(v_span),
                            };
                            self.check_expr(val, &exp)?;
                        }
                        None => {
                            self.synth_expr(val)?;
                        }
                    }
                }
                Ok(Some(self.lo.table.intern(TyKind::Nominal {
                    module: module as u32,
                    name: tyname,
                })))
            }
            Some(ItemSig::Struct(_) | ItemSig::Distinct { .. }) => {
                let self_ty = self.lo.table.intern(TyKind::Nominal {
                    module: module as u32,
                    name: tyname.clone(),
                });
                // An inherent associated function (no `self`).
                if let Some(idx) = self.find_inherent_impl(self_ty, &mname, false) {
                    let imp = &self.sigs.impls[idx];
                    if !imp.generics.is_empty() {
                        return Err(NotYet {
                            construct: "associated functions of a generic impl (s16 generic data)",
                            span: e.span,
                        });
                    }
                    let method = imp
                        .methods
                        .iter()
                        .find(|mm| mm.name == mname && !mm.has_self)
                        .cloned()
                        .expect("found above");
                    let label = format!("{tyname}.{mname}");
                    return Ok(Some(self.call_by_sig(&label, &method.sig, e, args)?));
                }
                if self.find_inherent_impl(self_ty, &mname, true).is_some() {
                    self.diags.push(
                        Diagnostic::error(
                            codes::E0403,
                            member.span,
                            format!("`{mname}` is a method, so it is called on a value"),
                        )
                        .with_label("called on the type itself")
                        .with_note(format!(
                            "make a `{tyname}` first, then call `value.{mname}(…)` on it."
                        )),
                    );
                    if let Some(a) = args {
                        self.synth_args_loosely(a)?;
                    }
                    return Ok(Some(self.error_ty()));
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// The type item a member-expression base names, if any: a bare
    /// type name or `ns.Type`.
    fn type_target(&self, m: MemberExpr<'_>) -> Option<(usize, String)> {
        let base = m.base()?;
        let target = if base.kind == SyntaxKind::PathExpr {
            let t = PathExpr::cast(base)?.ident()?;
            let name = self.text(t.span);
            if self.lookup_local(&name).is_some() {
                return None;
            }
            self.named_struct_target(&name)?
        } else if base.kind == SyntaxKind::MemberExpr {
            self.namespace_member(MemberExpr::cast(base)?)?
        } else {
            return None;
        };
        match self.sigs.get(target.0, &target.1) {
            Some(ItemSig::Enum { .. } | ItemSig::Struct(_) | ItemSig::Distinct { .. }) => {
                Some(target)
            }
            _ => None,
        }
    }

    /// `recv.method(args)` — the fixed resolution order (s17, D27):
    /// (1) inherent methods of the receiver's type; (2) trait methods
    /// from traits *in scope*, coherence making the impl unique per
    /// trait; ambiguity across traits is E0803 (qualify), a method on
    /// an out-of-scope trait is E0807 (import). No fallback chains, no
    /// receiver auto-transform — the receiver is a place and its mode
    /// is explicit (X1/E0804).
    fn method_call(
        &mut self,
        m: MemberExpr<'_>,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        let (Some(base), Some(member)) = (m.base(), m.member()) else {
            if let Some(a) = args {
                self.synth_args_loosely(a)?;
            }
            return Ok(self.error_ty());
        };
        let mname = self.text(member.span);
        let recv_mode = if base.kind == SyntaxKind::ParenExpr {
            ParenExpr::cast(base).and_then(|p| p.mode())
        } else {
            None
        };
        let recv_ty = self.synth_expr(base)?;
        match self.kind_of(recv_ty) {
            TyKind::Error | TyKind::Never => {
                if let Some(a) = args {
                    self.synth_args_loosely(a)?;
                }
                Ok(self.error_ty())
            }
            TyKind::Var(_) => Err(NotYet {
                construct: "a method call on a value whose type is still being inferred",
                span: e.span,
            }),
            TyKind::Unsupported(_) => Err(NotYet {
                construct: "methods on generic std data (s05 std surface)",
                span: e.span,
            }),
            // The s21 Tier-2 builtins: shared/weak cells and the
            // prelude containers carry their typed stub methods.
            TyKind::Shared(_) | TyKind::Weak(_) | TyKind::List(_) | TyKind::Pool(_) => {
                self.tier2_method_call(base, recv_ty, recv_mode, member.span, &mname, e, args)
            }
            // s37 — the builtin `str` surface (D24/D25).
            TyKind::Prim(Prim::Str) => {
                self.str_method_call(base, recv_ty, recv_mode, member.span, &mname, e, args)
            }
            // s22 — the raw tier's builtin pointer methods (strict
            // provenance ops + the null probe).
            TyKind::Ptr(_) => {
                self.ptr_method_call(base, recv_ty, recv_mode, member.span, &mname, e, args)
            }
            TyKind::Rigid(param) => self.archetype_method_call(
                &param,
                base,
                recv_ty,
                recv_mode,
                member.span,
                &mname,
                e,
                args,
            ),
            TyKind::Dyn { module, name } => self.dyn_method_call(
                TraitRef {
                    module: module as usize,
                    name,
                },
                base,
                recv_ty,
                recv_mode,
                member.span,
                &mname,
                e,
                args,
            ),
            _ => self.concrete_method_call(base, recv_ty, recv_mode, member.span, &mname, e, args),
        }
    }

    /// Is `tr` in scope here — defined in this module, or imported?
    fn trait_in_scope(&self, tr: &TraitRef) -> bool {
        if tr.module == self.module {
            return true;
        }
        bindings_for(self.pkg(), self.module, self.file)
            .iter()
            .any(|b| {
                matches!(&b.target, BindTarget::Item { module, name }
                if *module == tr.module && *name == tr.name)
            })
    }

    /// The first inherent impl of `recv` carrying `mname` (with or
    /// without `self` per `want_self`). Trial-unified, rolled back.
    fn find_inherent_impl(&mut self, recv: TyId, mname: &str, want_self: bool) -> Option<usize> {
        for (i, imp) in self.sigs.impls.iter().enumerate() {
            if imp.trait_ref.is_some() {
                continue;
            }
            if !imp
                .methods
                .iter()
                .any(|mm| mm.name == mname && mm.has_self == want_self)
            {
                continue;
            }
            let snap = self.vars.snapshot();
            let ok =
                traits::impl_self_matches(self.sigs, &mut self.lo.table, &mut self.vars, i, recv);
            self.vars.rollback(snap);
            if ok {
                return Some(i);
            }
        }
        None
    }

    /// Commit an impl instantiation against the receiver: fresh vars
    /// for the impl's generics (bounds become obligations), self type
    /// unified with the receiver.
    fn instantiate_impl(&mut self, idx: usize, recv: TyId, span: Span) -> BTreeMap<String, TyId> {
        let generics = self.sigs.impls[idx].generics.clone();
        let self_ty = self.sigs.impls[idx].self_ty;
        let mut map = BTreeMap::new();
        for g in &generics {
            let v = self.fresh(NumKind::Any, span);
            map.insert(g.name.clone(), v);
            for b in &g.bounds {
                self.obligate(
                    v,
                    TraitRef {
                        module: b.module,
                        name: b.name.clone(),
                    },
                    span,
                    Some(b.span),
                    OblOrigin::Instantiation {
                        callee: "this impl".to_string(),
                        param: g.name.clone(),
                    },
                );
            }
        }
        let self_inst = subst(&mut self.lo.table, self_ty, &map);
        let _ = unify(&mut self.lo.table, &mut self.vars, self_inst, recv);
        map
    }

    /// Resolution step 2 for a concrete receiver.
    #[allow(clippy::too_many_arguments)]
    fn concrete_method_call(
        &mut self,
        base: &GreenNode,
        recv_ty: TyId,
        recv_mode: Option<wolf_ast::ParamMode>,
        member_span: Span,
        mname: &str,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        // (1) Inherent methods win — the type's own impl blocks.
        if let Some(idx) = self.find_inherent_impl(recv_ty, mname, true) {
            let map = self.instantiate_impl(idx, recv_ty, base.span);
            let method = self.sigs.impls[idx]
                .methods
                .iter()
                .find(|mm| mm.name == mname && mm.has_self)
                .cloned()
                .expect("found above");
            self.dispatch.push((
                e.span,
                Dispatch::Inherent {
                    ty: self.show(recv_ty),
                    method: mname.to_string(),
                },
            ));
            return self.dispatch_method(
                mname,
                &method.sig,
                map,
                method.name_span,
                base,
                recv_ty,
                recv_mode,
                e,
                args,
            );
        }
        // (2) Trait methods from impls whose trait the receiver's type
        // implements; scope decides visibility, coherence uniqueness.
        let mut hits: Vec<TraitRef> = Vec::new();
        let mut out_of_scope: Vec<TraitRef> = Vec::new();
        for i in 0..self.sigs.impls.len() {
            let Some(tr) = self.sigs.impls[i].trait_ref.clone() else {
                continue;
            };
            let Some(td) = self.sigs.traits.get(&tr) else {
                continue;
            };
            if !td.params.is_empty() {
                continue; // parameterized traits dispatch qualified (bracket surface)
            }
            if !td.method(mname).is_some_and(|mm| mm.has_self) {
                continue;
            }
            let snap = self.vars.snapshot();
            let ok = traits::impl_self_matches(
                self.sigs,
                &mut self.lo.table,
                &mut self.vars,
                i,
                recv_ty,
            );
            self.vars.rollback(snap);
            if !ok {
                continue;
            }
            let bucket = if self.trait_in_scope(&tr) {
                &mut hits
            } else {
                &mut out_of_scope
            };
            if !bucket.contains(&tr) {
                bucket.push(tr);
            }
        }
        match hits.len() {
            1 => {
                let tr = hits.remove(0);
                let method = self
                    .sigs
                    .traits
                    .get(&tr)
                    .and_then(|td| td.method(mname))
                    .cloned()
                    .expect("candidate has the method");
                let mut map = BTreeMap::new();
                map.insert("Self".to_string(), recv_ty);
                self.dispatch.push((
                    e.span,
                    Dispatch::Trait {
                        module: tr.module,
                        name: tr.name.clone(),
                        method: mname.to_string(),
                        dyn_call: false,
                    },
                ));
                self.dispatch_method(
                    mname,
                    &method.sig,
                    map,
                    method.name_span,
                    base,
                    recv_ty,
                    recv_mode,
                    e,
                    args,
                )
            }
            0 => {
                if !out_of_scope.is_empty() {
                    self.report_out_of_scope_trait(member_span, mname, &out_of_scope);
                    if let Some(a) = args {
                        self.synth_args_loosely(a)?;
                    }
                    return Ok(self.error_ty());
                }
                // A callable field? `p.f(x)` calls through the field's
                // type — fields and methods share no namespace magic,
                // but the classic typo deserves the right answer.
                if let TyKind::Nominal { module, name } = self.kind_of(recv_ty)
                    && let Some(ItemSig::Struct(s)) = self.sigs.get(module as usize, &name).cloned()
                    && let Some(f) = s.fields.iter().find(|f| f.name == mname)
                {
                    let m_node = e
                        .nodes()
                        .find(|n| n.kind == SyntaxKind::MemberExpr)
                        .unwrap_or(e);
                    return self.call_by_type(f.ty, m_node, e, args);
                }
                // Builtin receivers (str, int, ranges, …) grow their
                // methods with the standard library — an honest
                // refusal, not a typo report, until s05 lands.
                if !matches!(self.kind_of(recv_ty), TyKind::Nominal { .. }) {
                    return Err(NotYet {
                        construct: "methods on builtin types (s05 std surface)",
                        span: e.span,
                    });
                }
                self.report_unknown_method(recv_ty, member_span, mname);
                if let Some(a) = args {
                    self.synth_args_loosely(a)?;
                }
                Ok(self.error_ty())
            }
            _ => {
                self.report_method_ambiguity(base, member_span, mname, &hits, e, args)?;
                Ok(self.error_ty())
            }
        }
    }

    /// A method call on an archetype receiver: the bounds are the
    /// whole story (the golden rule, D28).
    #[allow(clippy::too_many_arguments)]
    fn archetype_method_call(
        &mut self,
        param: &str,
        base: &GreenNode,
        recv_ty: TyId,
        recv_mode: Option<wolf_ast::ParamMode>,
        member_span: Span,
        mname: &str,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        let bs = self.bounds.get(param).cloned().unwrap_or_default();
        let mut hits: Vec<TraitRef> = Vec::new();
        for b in &bs {
            let tr = TraitRef {
                module: b.module,
                name: b.name.clone(),
            };
            if let Some(td) = self.sigs.traits.get(&tr)
                && td.params.is_empty()
                && td.method(mname).is_some_and(|mm| mm.has_self)
                && !hits.contains(&tr)
            {
                hits.push(tr);
            }
        }
        match hits.len() {
            1 => {
                let tr = hits.remove(0);
                let method = self
                    .sigs
                    .traits
                    .get(&tr)
                    .and_then(|td| td.method(mname))
                    .cloned()
                    .expect("candidate has the method");
                let mut map = BTreeMap::new();
                map.insert("Self".to_string(), recv_ty);
                self.dispatch.push((
                    e.span,
                    Dispatch::Trait {
                        module: tr.module,
                        name: tr.name.clone(),
                        method: mname.to_string(),
                        dyn_call: false,
                    },
                ));
                self.dispatch_method(
                    mname,
                    &method.sig,
                    map,
                    method.name_span,
                    base,
                    recv_ty,
                    recv_mode,
                    e,
                    args,
                )
            }
            0 => {
                let provider = {
                    let mut ps = self
                        .sigs
                        .traits
                        .values()
                        .filter(|td| td.method(mname).is_some_and(|mm| mm.has_self))
                        .map(|td| td.name.clone());
                    let first = ps.next();
                    match (first, ps.next()) {
                        (Some(f), None) => Some(f),
                        _ => None,
                    }
                };
                self.golden_rule_missing(
                    member_span,
                    param,
                    &format!("a method `{mname}`"),
                    provider,
                );
                if let Some(a) = args {
                    self.synth_args_loosely(a)?;
                }
                Ok(self.error_ty())
            }
            _ => {
                self.report_method_ambiguity(base, member_span, mname, &hits, e, args)?;
                Ok(self.error_ty())
            }
        }
    }

    /// A `dyn Trait` receiver dispatches through its own trait's
    /// method set — resolution is identical to the static path (the
    /// strategy is invisible here; witness layout is c05's).
    #[allow(clippy::too_many_arguments)]
    fn dyn_method_call(
        &mut self,
        tr: TraitRef,
        base: &GreenNode,
        recv_ty: TyId,
        recv_mode: Option<wolf_ast::ParamMode>,
        member_span: Span,
        mname: &str,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        let Some(td) = self.sigs.traits.get(&tr) else {
            if let Some(a) = args {
                self.synth_args_loosely(a)?;
            }
            return Ok(self.error_ty());
        };
        let Some(method) = td.method(mname).filter(|mm| mm.has_self).cloned() else {
            let names: Vec<&str> = td
                .methods
                .iter()
                .filter(|mm| mm.has_self)
                .map(|mm| mm.name.as_str())
                .collect();
            let mut d = Diagnostic::error(
                codes::E0403,
                member_span,
                format!("`dyn {}` has no method named `{mname}`", tr.name),
            )
            .with_label("unknown method")
            .with_secondary(td.name_span, format!("`{}` is declared here", tr.name));
            if let Some(hit) = wolf_diag::suggest::best_match(mname, &names) {
                d = d.with_suggestion(Suggestion::new(
                    format!("did you mean `{hit}`?"),
                    vec![(member_span, hit.to_string())],
                    Applicability::Maybe,
                ));
            }
            self.diags.push(d);
            if let Some(a) = args {
                self.synth_args_loosely(a)?;
            }
            return Ok(self.error_ty());
        };
        let mut map = BTreeMap::new();
        map.insert("Self".to_string(), recv_ty);
        self.dispatch.push((
            e.span,
            Dispatch::Trait {
                module: tr.module,
                name: tr.name.clone(),
                method: mname.to_string(),
                dyn_call: true,
            },
        ));
        self.dispatch_method(
            mname,
            &method.sig,
            map,
            method.name_span,
            base,
            recv_ty,
            recv_mode,
            e,
            args,
        )
    }

    /// The shared tail of every dispatch: the X1 receiver-mode law,
    /// the receiver against the `self` parameter, arguments against
    /// the rest, generics instantiated argument-first.
    #[allow(clippy::too_many_arguments)]
    fn dispatch_method(
        &mut self,
        label: &str,
        sig: &FnSig,
        mut map: BTreeMap<String, TyId>,
        decl_span: Span,
        recv: &GreenNode,
        recv_ty: TyId,
        recv_mode: Option<wolf_ast::ParamMode>,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        for g in &sig.generics {
            if map.contains_key(&g.name) {
                continue;
            }
            let v = self.fresh(NumKind::Any, e.span);
            map.insert(g.name.clone(), v);
            for b in &g.bounds {
                self.obligate(
                    v,
                    TraitRef {
                        module: b.module,
                        name: b.name.clone(),
                    },
                    e.span,
                    Some(b.span),
                    OblOrigin::Instantiation {
                        callee: label.to_string(),
                        param: g.name.clone(),
                    },
                );
            }
        }
        let Some(selfp) = sig.params.first().cloned() else {
            if let Some(a) = args {
                self.synth_args_loosely(a)?;
            }
            return Ok(self.error_ty());
        };
        self.calls.push((
            e.span,
            CallSig {
                callee: label.to_string(),
                decl_span: Some(decl_span),
                has_self: true,
                ctor: false,
                params: sig.params.clone(),
                c_call: false,
            },
        ));
        self.receiver_mode_law(recv, recv_mode, selfp.mode, label, decl_span);
        let pty = subst(&mut self.lo.table, selfp.ty, &map);
        let exp = Expect {
            ty: pty,
            reason: Reason::Receiver(label.to_string()),
            because: Some(decl_span),
        };
        self.expect_unify(recv.span, recv_ty, &exp);
        let arg_nodes: Vec<_> = args.into_iter().flat_map(|a| a.args()).collect();
        let wants = sig.params.len().saturating_sub(1);
        if arg_nodes.len() != wants {
            self.wrong_arg_count(label, e.span, Some(decl_span), wants, arg_nodes.len());
        }
        for (i, arg) in arg_nodes.iter().enumerate() {
            let Some(v) = Arg::value(*arg) else { continue };
            match sig.params.get(i + 1) {
                Some(p) => {
                    let pty = subst(&mut self.lo.table, p.ty, &map);
                    let exp = Expect {
                        ty: pty,
                        reason: Reason::ArgOfCall {
                            callee: label.to_string(),
                            index: i,
                        },
                        because: Some(p.span),
                    };
                    self.check_expr(v, &exp)?;
                }
                None => {
                    self.synth_expr(v)?;
                }
            }
        }
        let ret = subst(&mut self.lo.table, sig.ret, &map);
        Ok(self.normalize(ret))
    }

    /// E0804 — the X1 receiver-mode law: the call site spells exactly
    /// the mode the method declares (`(mut p).norm()`), and no mode
    /// for `read self`. Syntax only; exclusivity itself is c04.
    fn receiver_mode_law(
        &mut self,
        recv: &GreenNode,
        given: Option<wolf_ast::ParamMode>,
        declared: Option<wolf_ast::ParamMode>,
        method: &str,
        decl_span: Span,
    ) {
        use wolf_ast::ParamMode;
        if given == declared {
            return;
        }
        let word = |m: ParamMode| match m {
            ParamMode::Mut => "mut",
            ParamMode::Take => "take",
        };
        match (declared, given) {
            (Some(dm), None) => {
                let kw = word(dm);
                let lo = Span::new(recv.span.file, recv.span.lo, recv.span.lo);
                let hi = Span::new(recv.span.file, recv.span.hi, recv.span.hi);
                self.diags.push(
                    Diagnostic::error(
                        codes::E0804,
                        recv.span,
                        format!(
                            "`{method}` takes its receiver `{kw}`, but the call site does not say so"
                        ),
                    )
                    .with_label(format!("write the mode here: `({kw} …)`"))
                    .with_secondary(decl_span, format!("`{method}` declares `{kw} self` here"))
                    .with_note(
                        "the receiver mirrors argument modes (X1): exclusive or \
                         consuming access is spelled where the reader can see it.",
                    )
                    .with_suggestion(Suggestion::new(
                        format!("spell the receiver mode: `({kw} …)`"),
                        vec![(lo, format!("({kw} ")), (hi, ")".to_string())],
                        Applicability::MachineApplicable,
                    )),
                );
            }
            (None, Some(gm)) => {
                let kw = word(gm);
                let inner = ParenExpr::cast(recv)
                    .and_then(|p| p.expr())
                    .map(|n| self.text(n.span))
                    .unwrap_or_default();
                let mut d = Diagnostic::error(
                    codes::E0804,
                    recv.span,
                    format!("`{method}` reads its receiver, so the call site writes no mode"),
                )
                .with_label(format!("`{kw}` marks nothing here"))
                .with_secondary(decl_span, format!("`{method}` is declared here"))
                .with_note(
                    "a bare receiver is the `read` mode — absence is the syntax \
                     ([mem.tier0.mode]).",
                );
                if !inner.is_empty() {
                    d = d.with_suggestion(Suggestion::new(
                        "drop the mode".to_string(),
                        vec![(recv.span, inner)],
                        Applicability::MachineApplicable,
                    ));
                }
                self.diags.push(d);
            }
            (Some(dm), Some(_)) => {
                let dkw = word(dm);
                let mode_tok = recv
                    .tokens()
                    .find(|t| matches!(t.kind, SyntaxKind::MutKw | SyntaxKind::TakeKw))
                    .map(|t| t.span)
                    .unwrap_or(recv.span);
                self.diags.push(
                    Diagnostic::error(
                        codes::E0804,
                        mode_tok,
                        format!(
                            "`{method}` takes its receiver `{dkw}`, but the call site says otherwise"
                        ),
                    )
                    .with_label("the modes disagree")
                    .with_secondary(decl_span, format!("`{method}` declares `{dkw} self` here"))
                    .with_suggestion(Suggestion::new(
                        format!("match the declaration: `{dkw}`"),
                        vec![(mode_tok, dkw.to_string())],
                        Applicability::MachineApplicable,
                    )),
                );
            }
            (None, None) => unreachable!("given == declared was handled"),
        }
    }

    /// E0803 — two or more in-scope traits provide the method for
    /// this receiver: no precedence exists; qualify.
    fn report_method_ambiguity(
        &mut self,
        base: &GreenNode,
        member_span: Span,
        mname: &str,
        hits: &[TraitRef],
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<()> {
        let names: Vec<String> = hits.iter().map(|t| t.name.clone()).collect();
        let listed = names
            .iter()
            .map(|n| format!("`{n}`"))
            .collect::<Vec<_>>()
            .join(" and ");
        let recv_text = self.text(base.span);
        let args_text = e
            .nodes()
            .find_map(ArgList::cast)
            .map(|a| {
                a.args()
                    .filter_map(Arg::value)
                    .map(|v| self.text(v.span))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let mut d = Diagnostic::error(
            codes::E0803,
            member_span,
            format!("both {listed} provide a `{mname}` for this receiver — say which you mean"),
        )
        .with_label("ambiguous method");
        for n in names.iter().take(2) {
            let qualified = if args_text.is_empty() {
                format!("{n}.{mname}({recv_text})")
            } else {
                format!("{n}.{mname}({recv_text}, {args_text})")
            };
            d = d.with_suggestion(Suggestion::new(
                format!("qualify through `{n}`"),
                vec![(e.span, qualified)],
                Applicability::Maybe,
            ));
        }
        d = d.with_note(
            "trait namespaces are isolated (D28): a silent winner would change \
             meaning when imports change, so the qualified form is the answer.",
        );
        self.diags.push(d);
        if let Some(a) = args {
            self.synth_args_loosely(a)?;
        }
        Ok(())
    }

    /// E0807 — the method exists, but only on traits never imported.
    fn report_out_of_scope_trait(&mut self, member_span: Span, mname: &str, hits: &[TraitRef]) {
        if let [tr] = hits {
            let module_path = self
                .sigs
                .module_names
                .get(tr.module)
                .cloned()
                .unwrap_or_default();
            let use_line = if module_path.is_empty() {
                format!("use {}", tr.name)
            } else {
                format!("use {module_path}.{}", tr.name)
            };
            self.diags.push(
                Diagnostic::error(
                    codes::E0807,
                    member_span,
                    format!(
                        "`{mname}` exists on the trait `{}`, but the trait is not in scope",
                        tr.name
                    ),
                )
                .with_label("resolves through no in-scope trait")
                .with_note(
                    "method calls resolve through the traits in scope, so a new \
                     dependency can never silently change what `.method()` means (D28).",
                )
                .with_suggestion(Suggestion::new(
                    format!("import the trait: `{use_line}`"),
                    vec![(Span::new(member_span.file, 0, 0), format!("{use_line}\n"))],
                    Applicability::Maybe,
                )),
            );
        } else {
            let names = hits
                .iter()
                .map(|t| format!("`{}`", t.name))
                .collect::<Vec<_>>()
                .join(", ");
            self.diags.push(
                Diagnostic::error(
                    codes::E0807,
                    member_span,
                    format!("`{mname}` exists on {names}, but none of them are in scope"),
                )
                .with_label("resolves through no in-scope trait")
                .with_note(format!(
                    "bring the one you mean into scope with `use`, then call it — \
                     qualified (`Trait.{mname}(…)`) or on the receiver."
                )),
            );
        }
    }

    /// E0403 — no method anywhere: typo suggestions over everything
    /// the receiver's type does offer.
    fn report_unknown_method(&mut self, recv_ty: TyId, member_span: Span, mname: &str) {
        let shown = self.show(recv_ty);
        let mut candidates: Vec<String> = Vec::new();
        for imp in &self.sigs.impls {
            // Cheap filter: exact self-type match covers the common case.
            if imp.self_ty == recv_ty || imp.trait_ref.is_some() {
                for mm in &imp.methods {
                    if imp.self_ty == recv_ty && !candidates.contains(&mm.name) {
                        candidates.push(mm.name.clone());
                    }
                }
            }
        }
        if let TyKind::Nominal { module, name } = self.kind_of(recv_ty)
            && let Some(ItemSig::Struct(s)) = self.sigs.get(module as usize, &name)
        {
            for f in &s.fields {
                if !candidates.contains(&f.name) {
                    candidates.push(f.name.clone());
                }
            }
        }
        let refs: Vec<&str> = candidates.iter().map(|s| s.as_str()).collect();
        let mut d = Diagnostic::error(
            codes::E0403,
            member_span,
            format!("`{shown}` has no method named `{mname}`"),
        )
        .with_label("unknown method");
        if let Some(hit) = wolf_diag::suggest::best_match(mname, &refs) {
            d = d.with_suggestion(Suggestion::new(
                format!("did you mean `{hit}`?"),
                vec![(member_span, hit.to_string())],
                Applicability::Maybe,
            ));
        } else if !candidates.is_empty() {
            d = d.with_note(format!("`{shown}` offers: {}", candidates.join(", ")));
        }
        self.diags.push(d);
    }

    // ------------------------------------------------------- members ---

    fn synth_member(&mut self, e: &GreenNode) -> R<TyId> {
        let d = MemberExpr::cast(e).expect("kind");
        if let Some((module, item)) = self.namespace_member(d) {
            return self.item_value_ty(module, &item, e.span);
        }
        // E0004 — `1.e5` parses as member access on an integer; the
        // float the user meant spells its fraction (s17, the c02
        // handoff item).
        if let (Some(base), Some(member)) = (d.base(), d.member())
            && base.kind == SyntaxKind::LiteralExpr
            && base
                .tokens()
                .next()
                .is_some_and(|t| t.kind == SyntaxKind::Int)
        {
            let mtext = self.text(member.span);
            if mtext.len() >= 2
                && mtext.starts_with('e')
                && mtext[1..].chars().all(|c| c.is_ascii_digit())
            {
                let btext = self.text(base.span);
                let fix = format!("{btext}.0{mtext}");
                self.diags.push(
                    Diagnostic::error(
                        codes::E0004,
                        e.span,
                        format!("`{btext}.{mtext}` reads as member access, not a float"),
                    )
                    .with_label(format!("integers have no member `{mtext}`"))
                    .with_note(
                        "a float literal needs digits on both sides of the dot \
                         ([gram.amb.intdot]).",
                    )
                    .with_suggestion(Suggestion::new(
                        format!("write the float out: `{fix}`"),
                        vec![(e.span, fix)],
                        Applicability::MachineApplicable,
                    )),
                );
                return Ok(self.fresh(NumKind::Float, e.span));
            }
        }
        // `Type.Member` in value position (s17): enum variant values.
        if let Some((module, tyname)) = self.type_target(d)
            && let Some(ItemSig::Enum {
                generic,
                variants,
                name_span,
            }) = self.sigs.get(module, &tyname).cloned()
        {
            let Some(member) = d.member() else {
                return Ok(self.error_ty());
            };
            let mname = self.text(member.span);
            if generic {
                return Err(NotYet {
                    construct: "a generic enum's variants (s16 generic data)",
                    span: e.span,
                });
            }
            return match variants.iter().find(|v| v.name == mname) {
                Some(v) if v.payload.is_empty() => Ok(self.lo.table.intern(TyKind::Nominal {
                    module: module as u32,
                    name: tyname,
                })),
                Some(v) => {
                    self.diags.push(
                        Diagnostic::error(
                            codes::E0402,
                            member.span,
                            format!(
                                "the variant `{mname}` carries {} value{}, but none are given",
                                v.payload.len(),
                                if v.payload.len() == 1 { "" } else { "s" }
                            ),
                        )
                        .with_label("payload-carrying variant")
                        .with_secondary(v.span, format!("`{mname}` is declared here"))
                        .with_note(format!(
                            "construct it with its payload: `{tyname}.{mname}({})`.",
                            vec!["_"; v.payload.len()].join(", ")
                        )),
                    );
                    Ok(self.error_ty())
                }
                None => {
                    let names: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
                    let mut diag = Diagnostic::error(
                        codes::E0403,
                        member.span,
                        format!("`{tyname}` has no variant named `{mname}`"),
                    )
                    .with_label("unknown variant")
                    .with_secondary(name_span, format!("`{tyname}` is defined here"));
                    if let Some(hit) = wolf_diag::suggest::best_match(&mname, &names) {
                        diag = diag.with_suggestion(Suggestion::new(
                            format!("did you mean `{hit}`?"),
                            vec![(member.span, hit.to_string())],
                            Applicability::Maybe,
                        ));
                    } else if !names.is_empty() {
                        diag = diag.with_note(format!("the variants are: {}", names.join(", ")));
                    }
                    self.diags.push(diag);
                    Ok(self.error_ty())
                }
            };
        }
        // s14: archetype member access (`T.N` — associated consts
        // through the bounds) and trait members outside call position.
        if let Some(base) = d.base()
            && base.kind == SyntaxKind::PathExpr
            && let Some(t) = PathExpr::cast(base).and_then(|p| p.ident())
        {
            let bname = self.text(t.span);
            if self.lookup_local(&bname).is_none() {
                if self.generics.contains(&bname) {
                    let Some(member) = d.member() else {
                        return Ok(self.error_ty());
                    };
                    let mname = self.text(member.span);
                    return self.rigid_member(&bname, member.span, &mname);
                }
                if self.trait_target(&bname).is_some() {
                    return Err(NotYet {
                        construct: "a trait method used as a value (s17)",
                        span: e.span,
                    });
                }
            }
        }
        let Some(base) = d.base() else {
            return Ok(self.error_ty());
        };
        let base_ty = self.synth_expr(base)?;
        let Some(member) = d.member() else {
            return Ok(self.error_ty());
        };
        match self.kind_of(base_ty) {
            TyKind::Nominal { module, name } => {
                match self.sigs.get(module as usize, &name).cloned() {
                    Some(ItemSig::Struct(s)) => {
                        if s.generic {
                            return Err(NotYet {
                                construct: "fields of a generic struct (s16 generic data)",
                                span: e.span,
                            });
                        }
                        let mname = self.text(member.span);
                        match s.fields.iter().find(|f| f.name == mname) {
                            Some(f) => Ok(f.ty),
                            None => {
                                // The field/method crossover classic:
                                // `p.len` when `len` is a method.
                                if self.find_inherent_impl(base_ty, &mname, true).is_some() {
                                    self.diags.push(
                                        Diagnostic::error(
                                            codes::E0403,
                                            member.span,
                                            format!("`{mname}` is a method, not a field"),
                                        )
                                        .with_label("no `()` call here")
                                        .with_note(
                                            format!(
                                                "call it: `.{mname}(…)` — `p.{mname}` is a \
                                             field access, `p.{mname}()` is a method call."
                                            ),
                                        ),
                                    );
                                    return Ok(self.error_ty());
                                }
                                self.unknown_field(&name, &s, member.span, &mname);
                                Ok(self.error_ty())
                            }
                        }
                    }
                    Some(ItemSig::Enum { .. }) => Err(NotYet {
                        construct: "a method or variant used as a value (c14 backlog)",
                        span: e.span,
                    }),
                    _ => Err(NotYet {
                        construct: "member access on this type (s05 std surface)",
                        span: e.span,
                    }),
                }
            }
            TyKind::Tuple(ts) => {
                let mname = self.text(member.span);
                match mname.parse::<usize>() {
                    Ok(i) if i < ts.len() => Ok(ts[i]),
                    Ok(i) => {
                        let shown = self.show(base_ty);
                        self.diags.push(
                            Diagnostic::error(
                                codes::E0403,
                                member.span,
                                format!("`{shown}` has no element {i}"),
                            )
                            .with_label(format!(
                                "the tuple has {} element{}",
                                ts.len(),
                                if ts.len() == 1 { "" } else { "s" }
                            )),
                        );
                        Ok(self.error_ty())
                    }
                    Err(_) => Err(NotYet {
                        construct: "a method used as a value (c14 backlog)",
                        span: e.span,
                    }),
                }
            }
            TyKind::Error | TyKind::Never => Ok(self.error_ty()),
            // s21: `xs.len` — the one field-shaped member the List
            // stub carries (`b.data.len`, `xs.push(xs.len)` shapes).
            TyKind::List(_) => {
                let mname = self.text(member.span);
                if mname == "len" {
                    Ok(self.lo.table.prim(Prim::Int))
                } else {
                    Err(NotYet {
                        construct: "members on generic std data (s05 std surface)",
                        span: e.span,
                    })
                }
            }
            // s37: `s.len` — byte length, O(1), documented honestly
            // (`"é".len == 2`; D24/D25). The one field-shaped member
            // `str` carries; everything else on `str` is a method.
            TyKind::Prim(Prim::Str) => {
                let mname = self.text(member.span);
                if mname == "len" {
                    Ok(self.lo.table.prim(Prim::Int))
                } else {
                    Err(NotYet {
                        construct: "this `str` member (the s37 surface is `len` + methods)",
                        span: e.span,
                    })
                }
            }
            // s21: a `shared T` cell is referentially transparent for
            // reads — `cfg.limit` reaches the payload's fields
            // ([mem.shared.rc.4]: RC is unobservable; the cell adds no
            // member namespace of its own).
            TyKind::Shared(inner) => match self.kind_of(inner) {
                TyKind::Nominal { module, name } => {
                    match self.sigs.get(module as usize, &name).cloned() {
                        Some(ItemSig::Struct(s)) if !s.generic => {
                            let mname = self.text(member.span);
                            match s.fields.iter().find(|f| f.name == mname) {
                                Some(f) => Ok(f.ty),
                                None => {
                                    self.unknown_field(&name, &s, member.span, &mname);
                                    Ok(self.error_ty())
                                }
                            }
                        }
                        _ => Err(NotYet {
                            construct: "member access on this type (s05 std surface)",
                            span: e.span,
                        }),
                    }
                }
                _ => Err(NotYet {
                    construct: "member access on this type (s05 std surface)",
                    span: e.span,
                }),
            },
            // Reflection values (s16): a closed member table per meta
            // kind — semantic values only, no syntax anywhere (D29).
            TyKind::Meta(m) => {
                let mname = self.text(member.span);
                match meta_member_ty(m, &mname) {
                    Some(k) => Ok(self.lo.table.intern(k)),
                    None => {
                        self.unknown_meta_member(m, member.span, &mname);
                        Ok(self.error_ty())
                    }
                }
            }
            TyKind::Var(_) => Err(NotYet {
                construct: "member access on a value whose type is still being inferred",
                span: e.span,
            }),
            _ => Err(NotYet {
                construct: "members on this type (s05 std surface)",
                span: e.span,
            }),
        }
    }

    /// E0403 for reflection values, with the closed member list.
    fn unknown_meta_member(&mut self, m: MetaTy, span: Span, member: &str) {
        let members = meta_members(m);
        let mut d = Diagnostic::error(
            codes::E0403,
            span,
            format!("`{}` has no member named `{member}`", m.name()),
        )
        .with_label("unknown reflection member");
        if let Some(hit) = wolf_diag::suggest::best_match(member, members) {
            d = d.with_suggestion(Suggestion::new(
                format!("did you mean `{hit}`?"),
                vec![(span, hit.to_string())],
                Applicability::Maybe,
            ));
        } else {
            d = d.with_note(format!("the members are: {}", members.join(", ")));
        }
        self.diags.push(d);
    }

    /// Member access on an archetype (`T.member`): associated consts
    /// resolve through the bounds; anything else is a golden-rule
    /// E0501 at the definition.
    fn rigid_member(&mut self, param: &str, span: Span, mname: &str) -> R<TyId> {
        let bs = self.bounds.get(param).cloned().unwrap_or_default();
        let mut const_hit: Option<TyId> = None;
        let mut const_hits = 0usize;
        let mut assoc_ty = false;
        let mut method = false;
        for b in &bs {
            let tr = TraitRef {
                module: b.module,
                name: b.name.clone(),
            };
            let Some(td) = self.sigs.traits.get(&tr) else {
                continue;
            };
            if let Some(c) = td.assoc_consts.iter().find(|c| c.name == mname) {
                const_hit = Some(c.ty);
                const_hits += 1;
            }
            if td.assoc_types.iter().any(|a| a.name == mname) {
                assoc_ty = true;
            }
            if td.method(mname).is_some() {
                method = true;
            }
        }
        if const_hits > 1 {
            self.diags.push(
                Diagnostic::error(
                    codes::E0501,
                    span,
                    format!(
                        "more than one bound on `{param}` provides a `{mname}` — the use is ambiguous"
                    ),
                )
                .with_label("ambiguous member")
                .with_note(
                    "trait namespaces are isolated, so two bounds may collide on a \
                     name; qualification through the trait arrives with s17 — until \
                     then, drop one of the colliding bounds here.",
                ),
            );
            return Ok(self.error_ty());
        }
        if let Some(cty) = const_hit {
            let rigid = self.lo.table.intern(TyKind::Rigid(param.to_string()));
            let map: BTreeMap<String, TyId> = [("Self".to_string(), rigid)].into();
            return Ok(subst(&mut self.lo.table, cty, &map));
        }
        if assoc_ty {
            return Err(NotYet {
                construct: "an associated type used as a value (comptime, s16)",
                span,
            });
        }
        if method {
            return Err(NotYet {
                construct: "a trait method used as a value (s17)",
                span,
            });
        }
        // No bound provides the member: definition-site E0501, with a
        // bound suggestion when exactly one trait declares it.
        let provider = {
            let mut hits = self
                .sigs
                .traits
                .values()
                .filter(|td| {
                    td.assoc_consts.iter().any(|c| c.name == mname) || td.method(mname).is_some()
                })
                .map(|td| td.name.clone());
            let first = hits.next();
            match (first, hits.next()) {
                (Some(f), None) => Some(f),
                _ => None,
            }
        };
        self.golden_rule_missing(span, param, &format!("a member `{mname}`"), provider);
        Ok(self.error_ty())
    }

    /// E0403 with typo detection against the struct's field names.
    fn unknown_field(&mut self, struct_name: &str, s: &StructSig, span: Span, field: &str) {
        let mut d = Diagnostic::error(
            codes::E0403,
            span,
            format!("`{struct_name}` has no field named `{field}`"),
        )
        .with_label("unknown field")
        .with_secondary(s.name_span, format!("`{struct_name}` is defined here"));
        let names: Vec<&str> = s.fields.iter().map(|f| f.name.as_str()).collect();
        if let Some(hit) = wolf_diag::suggest::best_match(field, &names) {
            d = d.with_suggestion(Suggestion::new(
                format!("did you mean `{hit}`?"),
                vec![(span, hit.to_string())],
                Applicability::Maybe,
            ));
        } else if !names.is_empty() {
            d = d.with_note(format!("the fields are: {}", names.join(", ")));
        }
        self.diags.push(d);
    }

    fn synth_struct_lit(&mut self, e: &GreenNode) -> R<TyId> {
        let d = StructLit::cast(e).expect("kind");
        let Some(path) = d.path() else {
            return Ok(self.error_ty());
        };
        // The struct: a bare name or `ns.Name`.
        let target = if path.kind == SyntaxKind::PathExpr {
            PathExpr::cast(path)
                .and_then(|p| p.ident())
                .map(|t| self.text(t.span))
                .and_then(|name| self.named_struct_target(&name))
        } else if path.kind == SyntaxKind::MemberExpr {
            MemberExpr::cast(path).and_then(|m| self.namespace_member(m))
        } else {
            None
        };
        let Some((module, name)) = target else {
            // Unresolved or non-struct head: resolution reported it, or
            // it is a construct we refuse rather than guess.
            return Err(NotYet {
                construct: "this struct-literal head (s17)",
                span: path.span,
            });
        };
        let Some(ItemSig::Struct(sig)) = self.sigs.get(module, &name).cloned() else {
            return Err(NotYet {
                construct: "a literal of a non-struct type (s17)",
                span: path.span,
            });
        };
        if sig.generic {
            return Err(NotYet {
                construct: "a generic struct literal (s16 generic data)",
                span: e.span,
            });
        }
        let mut seen: Vec<String> = Vec::new();
        for init in d.fields() {
            let Some(nt) = FieldInit::name(init) else {
                continue;
            };
            let fname = self.text(nt.span);
            if seen.contains(&fname) {
                self.diags.push(
                    Diagnostic::error(
                        codes::E0408,
                        nt.span,
                        format!("the field `{fname}` is initialized twice"),
                    )
                    .with_label("second initialization")
                    .with_note("each field is written exactly once in a struct literal."),
                );
                continue;
            }
            seen.push(fname.clone());
            let Some(field) = sig.fields.iter().find(|f| f.name == fname).cloned() else {
                self.unknown_field(&name, &sig, nt.span, &fname);
                if let Some(v) = init.value() {
                    self.synth_expr(v)?;
                }
                continue;
            };
            match init.value() {
                Some(v) => {
                    let exp = Expect {
                        ty: field.ty,
                        reason: Reason::StructField(fname.clone()),
                        because: Some(field.span),
                    };
                    self.check_expr(v, &exp)?;
                }
                None => {
                    // Shorthand `{ x }`: the local of the same name.
                    let ty = self.lookup_local(&fname).unwrap_or_else(|| self.error_ty());
                    let exp = Expect {
                        ty: field.ty,
                        reason: Reason::StructField(fname.clone()),
                        because: Some(field.span),
                    };
                    self.expect_unify(nt.span, ty, &exp);
                }
            }
        }
        let missing: Vec<&str> = sig
            .fields
            .iter()
            .filter(|f| !seen.contains(&f.name))
            .map(|f| f.name.as_str())
            .collect();
        if !missing.is_empty() {
            let list = missing
                .iter()
                .map(|m| format!("`{m}`"))
                .collect::<Vec<_>>()
                .join(", ");
            self.diags.push(
                Diagnostic::error(
                    codes::E0408,
                    e.span,
                    format!(
                        "this `{name}` literal is missing the field{} {list}",
                        if missing.len() == 1 { "" } else { "s" }
                    ),
                )
                .with_label("missing fields")
                .with_secondary(sig.name_span, format!("`{name}` is defined here"))
                .with_note("every field is written explicitly — wolf has no defaults yet."),
            );
        }
        Ok(self.lo.table.intern(TyKind::Nominal {
            module: module as u32,
            name,
        }))
    }

    fn named_struct_target(&self, name: &str) -> Option<(usize, String)> {
        for b in bindings_for(self.pkg(), self.module, self.file) {
            if b.name == name {
                if let BindTarget::Item { module, name } = &b.target {
                    return Some((*module, name.clone()));
                }
                return None;
            }
        }
        if self.pkg().tables[self.module].get(name).is_some() {
            return Some((self.module, name.to_string()));
        }
        None
    }

    // ------------------------------------------------------- control ----

    fn check_cond(&mut self, cond: &GreenNode, reason: Reason) -> R<()> {
        let bool_ = self.lo.table.prim(Prim::Bool);
        let exp = Expect {
            ty: bool_,
            reason,
            because: None,
        };
        self.check_expr(cond, &exp)
    }

    fn check_if(&mut self, e: &GreenNode, exp: &Expect) -> R<()> {
        let d = IfExpr::cast(e).expect("kind");
        if let Some(c) = d.condition() {
            self.check_cond(c, Reason::IfCondition)?;
        }
        if let Some(b) = d.then_block() {
            self.check_block(b, exp)?;
        }
        match d.else_branch() {
            Some(n) if n.kind == SyntaxKind::IfExpr => self.check_if(n, exp)?,
            Some(n) => {
                if let Some(b) = Block::cast(n) {
                    self.check_block(b, exp)?;
                }
            }
            None => {
                let unit = self.lo.table.unit();
                let bare = Expect {
                    ty: exp.ty,
                    reason: Reason::BareIf,
                    because: Some(e.span),
                };
                self.expect_unify(e.span, unit, &bare);
            }
        }
        Ok(())
    }

    /// Synthesis orientation (Candidate A): an `if` synthesizes from
    /// its branches; a mismatch is reported with Elm's honest framing —
    /// neither branch is "expected".
    fn synth_if(&mut self, e: &GreenNode) -> R<TyId> {
        let d = IfExpr::cast(e).expect("kind");
        if let Some(c) = d.condition() {
            self.check_cond(c, Reason::IfCondition)?;
        }
        let then_ty = match d.then_block() {
            Some(b) => {
                let t = self.synth_block(b)?;
                (t, self.block_value_span(b))
            }
            None => (self.error_ty(), e.span),
        };
        match d.else_branch() {
            None => {
                let unit = self.lo.table.unit();
                if join(&mut self.lo.table, &mut self.vars, then_ty.0, unit).is_err() {
                    let exp = Expect {
                        ty: unit,
                        reason: Reason::BareIf,
                        because: Some(e.span),
                    };
                    self.report_mismatch(then_ty.1, then_ty.0, &exp);
                }
                Ok(unit)
            }
            Some(n) => {
                let (else_ty, else_span) = if n.kind == SyntaxKind::IfExpr {
                    (self.synth_if(n)?, n.span)
                } else if let Some(b) = Block::cast(n) {
                    let t = self.synth_block(b)?;
                    (t, self.block_value_span(b))
                } else {
                    (self.error_ty(), n.span)
                };
                match join(&mut self.lo.table, &mut self.vars, then_ty.0, else_ty) {
                    Ok(t) => Ok(t),
                    Err(err) => {
                        self.branch_disagreement(
                            "if", then_ty.1, then_ty.0, else_span, else_ty, err,
                        );
                        Ok(self.error_ty())
                    }
                }
            }
        }
    }

    /// Elm's honest both-branches framing: neither side is "expected".
    fn branch_disagreement(
        &mut self,
        what: &str,
        first_span: Span,
        first_ty: TyId,
        second_span: Span,
        second_ty: TyId,
        err: UnifyErr,
    ) {
        if let UnifyErr::Occurs { var, ty } = err {
            self.report_occurs(second_span, var, ty);
            return;
        }
        let a = self.show(first_ty);
        let b = self.show(second_ty);
        let (noun, one, article) = if what == "if" {
            ("branches", "branch", "an")
        } else {
            ("arms", "arm", "a")
        };
        self.diags.push(
            Diagnostic::error(
                codes::E0401,
                second_span,
                format!("the {noun} of this `{what}` disagree about its type"),
            )
            .with_label(format!("this {one} is `{b}`"))
            .with_secondary(first_span, format!("but this one is `{a}`"))
            .with_note(format!(
                "{article} `{what}` used as a value produces one type; neither \
                 {one} is more \"right\" — make both `{a}`, or both `{b}`, or \
                 move the `{what}` into statement position."
            )),
        );
    }

    /// `match`: synthesizes from its arms when `exp` is `None`, checks
    /// each arm against `exp` otherwise. Patterns are the full [gram.
    /// pat] grammar (s17); each match is recorded for the body-end
    /// exhaustiveness pass ([`Checker::check_matches`]).
    fn check_match(&mut self, e: &GreenNode, exp: Option<&Expect>) -> R<TyId> {
        let d = MatchExpr::cast(e).expect("kind");
        let scrut_ty = match d.scrutinee() {
            Some(s) => self.synth_expr(s)?,
            None => self.error_ty(),
        };
        // Matching a fallible value directly is not a surface: `?`
        // unwraps it, `else |err|` hands you the row value to match.
        if let TyKind::ErrUnion(..) = self.kind_of(scrut_ty) {
            return Err(NotYet {
                construct: "`match` over a fallible value (unwrap with `?` or bind the error with `else |err|`)",
                span: e.span,
            });
        }
        let mut rec = MatchRec {
            span: e.span,
            scrut: scrut_ty,
            arms: Vec::new(),
        };
        let mut joined: Option<(TyId, Span)> = None;
        for arm in d.arms() {
            self.push_scope();
            if let Some(p) = arm.pattern() {
                let lowered = self.match_pattern(p, scrut_ty)?;
                rec.arms.push((lowered, arm.guard().is_some(), p.span));
            }
            if let Some(g) = arm.guard()
                && let Some(cond) = g.nodes().find(|n| is_expr_kind(n.kind))
            {
                self.check_cond(cond, Reason::MatchGuard)?;
            }
            if let Some(body) = MatchArm::body(arm) {
                match exp {
                    Some(exp) => self.check_expr(body, exp)?,
                    None => {
                        let t = self.synth_expr(body)?;
                        match joined {
                            None => joined = Some((t, body.span)),
                            Some((prev, prev_span)) => {
                                match join(&mut self.lo.table, &mut self.vars, prev, t) {
                                    Ok(j) => joined = Some((j, prev_span)),
                                    Err(err) => {
                                        self.branch_disagreement(
                                            "match", prev_span, prev, body.span, t, err,
                                        );
                                        joined = Some((self.error_ty(), prev_span));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            self.pop_scope();
        }
        self.match_recs.push(rec);
        match exp {
            Some(exp) => Ok(exp.ty),
            None => Ok(joined
                .map(|(t, _)| t)
                .unwrap_or_else(|| self.lo.table.never())),
        }
    }

    /// Type one match pattern against the scrutinee and lower it to
    /// the exhaustiveness IR (s17): wildcards, bindings, literals,
    /// enum variants and error-row tags with payloads, tuples,
    /// or-patterns, `@`-bindings.
    fn match_pattern(&mut self, pat: &GreenNode, scrut: TyId) -> R<exhaust::Pat> {
        use exhaust::{Ctor, Pat};
        match pat.kind {
            SyntaxKind::WildcardPat => Ok(Pat::Wild),
            SyntaxKind::IdentPat => {
                let Some(t) = pat.child_token(SyntaxKind::Ident) else {
                    return Ok(Pat::Wild);
                };
                let name = self.text(t.span);
                // Type-directed (#4): a bare name that spells one of
                // the scrutinee's cases — enum variant or row tag,
                // either letter case — is a test, not a binding
                // (`match err { gone => … }` over `T ! {gone}`). An
                // uppercase non-case still reports (E0403/E0602)
                // rather than silently binding.
                if let Some((cases, open)) = self.scrut_cases(scrut) {
                    if let Some((_, payload)) = cases.iter().find(|(n, _)| *n == name) {
                        if !payload.is_empty() {
                            self.pattern_shape_err(
                                t.span,
                                format!(
                                    "bare `{name}` names a variant that carries {} value{}",
                                    payload.len(),
                                    if payload.len() == 1 { "" } else { "s" }
                                ),
                                format!(
                                    "match its payload too: `{name}({})`",
                                    vec!["_"; payload.len()].join(", ")
                                ),
                            );
                        }
                        let arity = payload.len();
                        return Ok(Pat::Ctor {
                            ctor: Ctor::Named(name),
                            args: vec![Pat::Wild; arity],
                        });
                    }
                    if name.chars().next().is_some_and(char::is_uppercase) {
                        if open {
                            // An open row admits tags we cannot see.
                            return Ok(Pat::Ctor {
                                ctor: Ctor::Named(name),
                                args: Vec::new(),
                            });
                        }
                        self.unknown_case(scrut, t.span, &name);
                        return Ok(Pat::Wild);
                    }
                } else if name.chars().next().is_some_and(char::is_uppercase) {
                    // W0801 (s68): the scrutinee has no cases, so a
                    // capitalized bare name BINDS — the arm matches
                    // everything, and it reads as a variant test (the
                    // first-arm-always scar).
                    self.diags.push(
                        Diagnostic::warning(
                            codes::W0801,
                            t.span,
                            format!("`{name}` binds a new name here — it does not match a case"),
                        )
                        .with_label("matches every value")
                        .with_note(
                            "the scrutinee has no variants or tags, so a bare name is a \
                             binding; compare with a guard (`n if n == …`), or use a \
                             lowercase name so it reads as the binding it is."
                                .to_string(),
                        ),
                    );
                }
                self.bind(name, t.span, scrut);
                Ok(Pat::Wild)
            }
            SyntaxKind::BindingPat => {
                if let Some(t) = pat.child_token(SyntaxKind::Ident) {
                    let name = self.text(t.span);
                    self.bind(name, t.span, scrut);
                }
                match pat.nodes().find(|n| wolf_ast::is_pattern_kind(n.kind)) {
                    Some(sub) => self.match_pattern(sub, scrut),
                    None => Ok(Pat::Wild),
                }
            }
            SyntaxKind::OrPat => {
                let mut alts = Vec::new();
                for sub in pat.nodes().filter(|n| wolf_ast::is_pattern_kind(n.kind)) {
                    alts.push(self.match_pattern(sub, scrut)?);
                }
                Ok(Pat::Or(alts))
            }
            SyntaxKind::TuplePat => {
                let subs: Vec<&GreenNode> = pat
                    .nodes()
                    .filter(|n| wolf_ast::is_pattern_kind(n.kind))
                    .collect();
                let elem_tys: Vec<TyId> = match self.kind_of(scrut) {
                    TyKind::Tuple(ts) if ts.len() == subs.len() => ts,
                    TyKind::Error => vec![self.error_ty(); subs.len()],
                    TyKind::Var(_) => {
                        let fresh: Vec<TyId> = subs
                            .iter()
                            .map(|s| self.fresh(NumKind::Any, s.span))
                            .collect();
                        let tup = self.lo.table.intern(TyKind::Tuple(fresh.clone()));
                        let exp = Expect {
                            ty: tup,
                            reason: Reason::Pattern,
                            because: Some(pat.span),
                        };
                        self.expect_unify(pat.span, scrut, &exp);
                        fresh
                    }
                    TyKind::Tuple(ts) => {
                        self.pattern_shape_err(
                            pat.span,
                            format!(
                                "this tuple pattern binds {} piece{}, but the value has {}",
                                subs.len(),
                                if subs.len() == 1 { "" } else { "s" },
                                ts.len()
                            ),
                            format!(
                                "write one sub-pattern per element — `_` for the {} you skip",
                                if ts.len() > subs.len() {
                                    "ones"
                                } else {
                                    "none"
                                }
                            ),
                        );
                        vec![self.error_ty(); subs.len()]
                    }
                    _ => {
                        let shown = self.show(scrut);
                        self.pattern_shape_err(
                            pat.span,
                            format!("this pattern unpacks a tuple, but the value is `{shown}`"),
                            "match the value's own shape, or bind it to a name".to_string(),
                        );
                        vec![self.error_ty(); subs.len()]
                    }
                };
                let mut args = Vec::new();
                for (sub, t) in subs.into_iter().zip(elem_tys) {
                    args.push(self.match_pattern(sub, t)?);
                }
                Ok(Pat::Ctor {
                    ctor: Ctor::Tuple,
                    args,
                })
            }
            SyntaxKind::LiteralPat => self.literal_pattern(pat, scrut),
            SyntaxKind::PathPat => self.path_pattern(pat, scrut),
            _ => Ok(Pat::Wild), // broken tree: silence (D22)
        }
    }

    /// A literal pattern: types against the scrutinee, lowers to its
    /// constructor.
    fn literal_pattern(&mut self, pat: &GreenNode, scrut: TyId) -> R<exhaust::Pat> {
        use exhaust::{Ctor, Pat};
        // String literals arrive as a StringExpr node.
        if let Some(s) = pat.nodes().find(|n| n.kind == SyntaxKind::StringExpr) {
            if wolf_ast::StringExpr::cast(s).is_some_and(|d| d.interps().next().is_some()) {
                self.pattern_shape_err(
                    pat.span,
                    "an interpolated string cannot be a pattern".to_string(),
                    "patterns compare against fixed values; match a plain string \
                     literal, or compare with `==` in a guard"
                        .to_string(),
                );
                return Ok(Pat::Wild);
            }
            let str_ = self.lo.table.prim(Prim::Str);
            let exp = Expect {
                ty: scrut,
                reason: Reason::Pattern,
                because: Some(pat.span),
            };
            self.expect_unify(pat.span, str_, &exp);
            return Ok(Pat::Ctor {
                ctor: Ctor::Str(self.text(s.span)),
                args: Vec::new(),
            });
        }
        let tok = pat.tokens().next();
        let (lit_ty, ctor) = match tok.map(|t| t.kind) {
            Some(SyntaxKind::Int) => {
                let text = self.text(tok.expect("token").span);
                let ctor = match parse_int_text(&text) {
                    Some(v) => Ctor::Int(v),
                    // Unparseable spelling: a textual-identity ctor —
                    // never claims wildcard coverage.
                    None => Ctor::Named(text),
                };
                (self.fresh(NumKind::Integer, pat.span), Some(ctor))
            }
            Some(SyntaxKind::Float) => {
                let text = self.text(tok.expect("token").span);
                (
                    self.fresh(NumKind::Float, pat.span),
                    Some(Ctor::Float(text)),
                )
            }
            Some(SyntaxKind::TrueKw) => (self.lo.table.prim(Prim::Bool), Some(Ctor::Bool(true))),
            Some(SyntaxKind::FalseKw) => (self.lo.table.prim(Prim::Bool), Some(Ctor::Bool(false))),
            _ => (self.error_ty(), None),
        };
        let exp = Expect {
            ty: scrut,
            reason: Reason::Pattern,
            because: Some(pat.span),
        };
        self.expect_unify(pat.span, lit_ty, &exp);
        Ok(match ctor {
            Some(c) => Pat::Ctor {
                ctor: c,
                args: Vec::new(),
            },
            None => Pat::Wild,
        })
    }

    /// A path pattern `Tag(p, …)` / `io.Error(e)` / `Color.Rgb(…)`:
    /// an enum variant or an error-row tag with payload sub-patterns.
    fn path_pattern(&mut self, pat: &GreenNode, scrut: TyId) -> R<exhaust::Pat> {
        use exhaust::{Ctor, Pat};
        let segs: Vec<(String, Span)> = pat
            .nodes()
            .find_map(wolf_ast::Path::cast)
            .map(|p| p.segments().map(|t| (self.text(t.span), t.span)).collect())
            .unwrap_or_default();
        let subs: Vec<&GreenNode> = pat
            .nodes()
            .filter(|n| wolf_ast::is_pattern_kind(n.kind))
            .collect();
        let name_span = segs.last().map(|(_, s)| *s).unwrap_or(pat.span);
        let dotted = segs
            .iter()
            .map(|(n, _)| n.as_str())
            .collect::<Vec<_>>()
            .join(".");
        // Resolve the constructor against the scrutinee's shape.
        let payload: Option<(String, Vec<TyId>)> = match self.kind_of(scrut) {
            TyKind::Row { tags, tail } => match tags.iter().find(|(n, _)| *n == dotted) {
                Some((n, p)) => Some((n.clone(), p.clone())),
                None => {
                    let open = tail.is_some();
                    if open {
                        // An open row admits tags we cannot see;
                        // sub-patterns bind at unknown types.
                        Some((dotted.clone(), vec![self.error_ty(); subs.len()]))
                    } else {
                        self.unknown_case(scrut, name_span, &dotted);
                        None
                    }
                }
            },
            TyKind::Nominal { .. } => match self.scrut_variants(scrut) {
                Some(vs) => {
                    // `Rgb(…)` or `Color.Rgb(…)` — the variant is the
                    // last segment.
                    let vname = segs.last().map(|(n, _)| n.clone()).unwrap_or_default();
                    match vs.iter().find(|v| v.0 == vname) {
                        Some((n, p, _)) => Some((n.clone(), p.clone())),
                        None => {
                            self.unknown_case(scrut, name_span, &vname);
                            None
                        }
                    }
                }
                None => {
                    let shown = self.show(scrut);
                    self.pattern_shape_err(
                        pat.span,
                        format!("this pattern deconstructs a variant, but the value is `{shown}`"),
                        "only enum variants and error-row tags carry payloads to match".to_string(),
                    );
                    None
                }
            },
            TyKind::Error | TyKind::Never => None,
            _ => {
                let shown = self.show(scrut);
                self.pattern_shape_err(
                    pat.span,
                    format!("this pattern deconstructs a variant, but the value is `{shown}`"),
                    "only enum variants and error-row tags carry payloads to match".to_string(),
                );
                None
            }
        };
        let Some((cname, ptys)) = payload else {
            // Still walk the sub-patterns so their bindings exist.
            for sub in subs {
                let err = self.error_ty();
                self.match_pattern(sub, err)?;
            }
            return Ok(Pat::Wild);
        };
        if subs.len() != ptys.len() {
            self.pattern_shape_err(
                name_span,
                format!(
                    "`{cname}` carries {} value{}, but this pattern binds {}",
                    ptys.len(),
                    if ptys.len() == 1 { "" } else { "s" },
                    subs.len()
                ),
                format!(
                    "match the declared shape: `{cname}({})`",
                    vec!["_"; ptys.len()].join(", ")
                ),
            );
        }
        let mut args = Vec::new();
        for (i, sub) in subs.iter().enumerate() {
            let t = ptys.get(i).copied().unwrap_or_else(|| self.error_ty());
            args.push(self.match_pattern(sub, t)?);
        }
        args.resize(ptys.len(), Pat::Wild);
        Ok(Pat::Ctor {
            ctor: Ctor::Named(cname),
            args,
        })
    }

    /// The scrutinee's named cases — enum variants or row tags — as
    /// `(name, payload types)` plus whether the row is open. `None`
    /// when the type has no case shape at all.
    #[allow(clippy::type_complexity)]
    fn scrut_cases(&self, ty: TyId) -> Option<(Vec<(String, Vec<TyId>)>, bool)> {
        match self.kind_of(ty) {
            TyKind::Row { tags, tail } => Some((tags, tail.is_some())),
            _ => self
                .scrut_variants(ty)
                .map(|vs| (vs.into_iter().map(|(n, p, _)| (n, p)).collect(), false)),
        }
    }

    /// The scrutinee's enum variants, if it is a (non-generic) enum:
    /// (name, payload types, declaration span).
    fn scrut_variants(&self, ty: TyId) -> Option<Vec<(String, Vec<TyId>, Span)>> {
        if let TyKind::Nominal { module, name } = self.kind_of(ty)
            && let Some(ItemSig::Enum {
                variants,
                generic: false,
                ..
            }) = self.sigs.get(module as usize, &name)
        {
            return Some(
                variants
                    .iter()
                    .map(|v| (v.name.clone(), v.payload.clone(), v.span))
                    .collect(),
            );
        }
        None
    }

    /// E0808 — a pattern that cannot fit the value's shape.
    fn pattern_shape_err(&mut self, span: Span, what: String, next: String) {
        self.diags.push(
            Diagnostic::error(codes::E0808, span, what)
                .with_label("this pattern")
                .with_note(format!("{next}.")),
        );
    }

    /// An unknown variant/tag in a pattern: E0403 with suggestions
    /// over the scrutinee's cases (closed rows) — never a row dump.
    fn unknown_case(&mut self, scrut: TyId, span: Span, name: &str) {
        let (what, cases): (String, Vec<String>) = match self.kind_of(scrut) {
            TyKind::Row { tags, .. } => (
                "the value's error row".to_string(),
                tags.iter().map(|(n, _)| n.clone()).collect(),
            ),
            _ => match self.scrut_variants(scrut) {
                Some(vs) => (
                    format!("`{}`", self.show(scrut)),
                    vs.into_iter().map(|(n, _, _)| n).collect(),
                ),
                None => (format!("`{}`", self.show(scrut)), Vec::new()),
            },
        };
        let is_row = matches!(self.kind_of(scrut), TyKind::Row { .. });
        let code = if is_row { codes::E0602 } else { codes::E0403 };
        let mut d = Diagnostic::error(
            code,
            span,
            if is_row {
                format!("this arm matches the tag `{name}`, but {what} does not include it")
            } else {
                format!("{what} has no variant named `{name}`")
            },
        )
        .with_label(if is_row {
            "the arm could never fire"
        } else {
            "unknown variant"
        });
        let refs: Vec<&str> = cases.iter().map(|s| s.as_str()).collect();
        if let Some(hit) = wolf_diag::suggest::best_match(name, &refs) {
            d = d.with_suggestion(Suggestion::new(
                format!("did you mean `{hit}`?"),
                vec![(span, hit.to_string())],
                Applicability::Maybe,
            ));
        } else if !cases.is_empty() {
            d = d.with_note(format!("the cases are: {}", cases.join(", ")));
        }
        self.diags.push(d);
    }

    // ------------------------------------------- exhaustiveness (s17) --

    /// The body-end match pass: with types final, run the usefulness
    /// engine over every recorded `match` — non-exhaustive is E0801
    /// with concrete witnesses (≤3), a dead arm is E0802 (warning)
    /// citing the arm(s) that subsume it.
    fn check_matches(&mut self) {
        let recs = std::mem::take(&mut self.match_recs);
        for rec in recs {
            let scrut = zonk(&mut self.lo.table, &self.vars, rec.scrut);
            let Some(col) = self.col_ty(scrut, 0) else {
                // Error/unresolved scrutinee: another diagnostic owns
                // this body — one root cause, one report.
                self.match_facts.push((rec.span, false));
                continue;
            };
            let tys = [col];
            let mut prior: Vec<Vec<exhaust::Pat>> = Vec::new();
            let mut prior_spans: Vec<Span> = Vec::new();
            let mut any_guarded = false;
            for (p, guarded, span) in &rec.arms {
                if !exhaust::is_useful(&prior, std::slice::from_ref(p), &tys) {
                    // Find the minimal covering prefix; cite its last
                    // arm as the subsumer.
                    let mut cite = prior_spans.last().copied();
                    for k in 1..=prior.len() {
                        if !exhaust::is_useful(&prior[..k], std::slice::from_ref(p), &tys) {
                            cite = Some(prior_spans[k - 1]);
                            break;
                        }
                    }
                    let mut d = Diagnostic::warning(
                        codes::E0802,
                        *span,
                        "this arm can never match — the arms above already cover it".to_string(),
                    )
                    .with_label("unreachable arm");
                    if let Some(c) = cite {
                        d = d.with_secondary(c, "this arm already matches those values");
                    }
                    d = d.with_note(
                        "delete the arm, or reorder the arms so the more specific \
                         pattern comes first.",
                    );
                    self.diags.push(d);
                }
                if !*guarded {
                    prior.push(vec![p.clone()]);
                    prior_spans.push(*span);
                } else {
                    any_guarded = true;
                }
            }
            let ws = exhaust::witnesses(&prior, &tys, 3);
            let exhaustive = ws.is_empty();
            self.match_facts.push((rec.span, exhaustive));
            if !exhaustive {
                let rendered: Vec<String> = ws
                    .iter()
                    .map(|w| format!("`{}`", exhaust::render_pat(w)))
                    .collect();
                let mut d = Diagnostic::error(
                    codes::E0801,
                    rec.span,
                    format!("this `match` does not cover {}", rendered.join(", ")),
                )
                .with_label("not every value is matched");
                if any_guarded {
                    d = d.with_note(
                        "arms with `if` guards do not count toward coverage — a \
                         guard can be false.",
                    );
                }
                d = d.with_note(
                    "add arms for the missing cases, or end the `match` with a \
                     `_` arm to catch the rest deliberately.",
                );
                self.diags.push(d);
            }
        }
    }

    /// One column's constructor universe from a zonked type. `None`
    /// silences the pass (error/unresolved scrutinees).
    fn col_ty(&mut self, ty: TyId, depth: u32) -> Option<exhaust::ColTy> {
        use exhaust::ColTy;
        if depth > 8 {
            return Some(ColTy::Opaque);
        }
        let t = zonk(&mut self.lo.table, &self.vars, ty);
        Some(match self.lo.table.kind(t).clone() {
            TyKind::Error | TyKind::Never | TyKind::Var(_) => return None,
            TyKind::Prim(Prim::Bool) => ColTy::Bool,
            TyKind::Prim(Prim::Str) => ColTy::Str,
            TyKind::Prim(p) if p.is_integer() => ColTy::Int,
            TyKind::Prim(_) => ColTy::Float,
            TyKind::Wrapping(_) => ColTy::Int,
            TyKind::Tuple(ts) => {
                let mut cols = Vec::new();
                for t in ts {
                    cols.push(self.col_ty(t, depth + 1)?);
                }
                ColTy::Tuple(cols)
            }
            TyKind::Nominal { .. } => match self.scrut_variants(t) {
                Some(vs) => {
                    let mut out = Vec::new();
                    for (n, payload, _) in vs {
                        let mut cols = Vec::new();
                        for p in payload {
                            cols.push(self.col_ty(p, depth + 1)?);
                        }
                        out.push((n, cols));
                    }
                    ColTy::Enum(out)
                }
                None => ColTy::Opaque,
            },
            TyKind::Row { tags, tail } => {
                let mut out = Vec::new();
                for (n, payload) in tags {
                    let mut cols = Vec::new();
                    for p in payload {
                        cols.push(self.col_ty(p, depth + 1)?);
                    }
                    out.push((n, cols));
                }
                ColTy::Row {
                    tags: out,
                    open: tail.is_some(),
                }
            }
            _ => ColTy::Opaque,
        })
    }

    fn synth_for(&mut self, e: &GreenNode) -> R<TyId> {
        let d = ForExpr::cast(e).expect("kind");
        let elem = match d.iterable() {
            Some(it) if it.kind == SyntaxKind::RangeExpr => {
                let range_ty = self.synth_expr(it)?;
                match self.kind_of(range_ty) {
                    TyKind::Range(t) => t,
                    _ => self.error_ty(),
                }
            }
            Some(it) => {
                let t = self.synth_expr(it)?;
                match self.kind_of(t) {
                    TyKind::Range(elem) => elem,
                    // s21: `for x in xs` over the builtin List stub —
                    // elements in place order (the corpus region
                    // litmuses iterate lists; the general iteration
                    // protocol stays s17's).
                    TyKind::List(elem) => elem,
                    TyKind::Error | TyKind::Never => self.error_ty(),
                    _ => {
                        return Err(NotYet {
                            construct: "the iteration protocol (s17 for-trait wiring)",
                            span: it.span,
                        });
                    }
                }
            }
            None => self.error_ty(),
        };
        self.push_scope();
        if let Some(p) = d.pattern() {
            self.bind_pattern(p, elem)?;
        }
        if let Some(b) = d.body() {
            let unit = self.lo.table.unit();
            let exp = Expect {
                ty: unit,
                reason: Reason::LoopBody,
                because: None,
            };
            self.check_block(b, &exp)?;
        }
        self.pop_scope();
        Ok(self.lo.table.unit())
    }

    fn synth_while(&mut self, e: &GreenNode) -> R<TyId> {
        let d = WhileExpr::cast(e).expect("kind");
        if let Some(c) = d.condition() {
            self.check_cond(c, Reason::WhileCondition)?;
        }
        self.loops.push(LoopCtx { saw_break: false });
        if let Some(b) = d.body() {
            let unit = self.lo.table.unit();
            let exp = Expect {
                ty: unit,
                reason: Reason::LoopBody,
                because: None,
            };
            self.check_block(b, &exp)?;
        }
        self.loops.pop();
        Ok(self.lo.table.unit())
    }

    fn synth_loop(&mut self, e: &GreenNode) -> R<TyId> {
        let d = wolf_ast::LoopExpr::cast(e).expect("kind");
        self.loops.push(LoopCtx { saw_break: false });
        if let Some(b) = d.body() {
            let unit = self.lo.table.unit();
            let exp = Expect {
                ty: unit,
                reason: Reason::LoopBody,
                because: None,
            };
            self.check_block(b, &exp)?;
        }
        let ctx = self.loops.pop().expect("loop ctx");
        // `loop` without `break` never produces a value.
        Ok(if ctx.saw_break {
            self.lo.table.unit()
        } else {
            self.lo.table.never()
        })
    }

    fn synth_break(&mut self, e: &GreenNode) -> R<TyId> {
        let d = wolf_ast::BreakExpr::cast(e).expect("kind");
        if let Some(v) = d.value() {
            return Err(NotYet {
                construct: "`break` with a value (s17 loop values)",
                span: v.span,
            });
        }
        if let Some(ctx) = self.loops.last_mut() {
            ctx.saw_break = true;
        }
        Ok(self.lo.table.never())
    }

    fn synth_return(&mut self, e: &GreenNode) -> R<TyId> {
        if self.in_closure {
            return Err(NotYet {
                construct: "`return` inside a closure (s17 control typing)",
                span: e.span,
            });
        }
        let d = ReturnExpr::cast(e).expect("kind");
        let Some((ret, name, because)) = self.ret.clone() else {
            return Ok(self.lo.table.never());
        };
        let exp = Expect {
            ty: ret,
            reason: Reason::ReturnOfFn(name),
            because: Some(because),
        };
        match d.value() {
            Some(v) => self.check_expr(v, &exp)?,
            None => {
                let unit = self.lo.table.unit();
                self.expect_unify(e.span, unit, &exp);
            }
        }
        Ok(self.lo.table.never())
    }

    // ------------------------------------------------------ closures ---

    fn check_tuple(&mut self, e: &GreenNode, exp: &Expect) -> R<()> {
        let d = TupleExpr::cast(e).expect("kind");
        let elems: Vec<&GreenNode> = d.elems().collect();
        if let TyKind::Tuple(ts) = self.kind_of(exp.ty)
            && ts.len() == elems.len()
        {
            for (elem, &t) in elems.iter().zip(ts.iter()) {
                let sub = Expect {
                    ty: t,
                    reason: exp.reason.clone(),
                    because: exp.because,
                };
                self.check_expr(elem, &sub)?;
            }
            self.record(e.span, exp.ty);
            return Ok(());
        }
        let t = self.synth_expr(e)?;
        self.expect_unify(e.span, t, exp);
        Ok(())
    }

    /// Closure parameters take their types from checking context (the
    /// Pfenning recipe: closures are intro forms and check).
    fn check_closure(&mut self, e: &GreenNode, exp: &Expect) -> R<()> {
        let expected = self.shallow(exp.ty);
        match self.lo.table.kind(expected).clone() {
            TyKind::Fn(ptys, ret) => {
                let d = ClosureExpr::cast(e).expect("kind");
                let params: Vec<_> = d.params().into_iter().flat_map(|p| p.params()).collect();
                if params.len() != ptys.len() {
                    let shown = self.show(expected);
                    self.diags.push(
                        Diagnostic::error(
                            codes::E0401,
                            e.span,
                            format!(
                                "this closure takes {} parameter{}, but the context needs `{shown}`",
                                params.len(),
                                if params.len() == 1 { "" } else { "s" }
                            ),
                        )
                        .with_label("closure here")
                        .with_note(format!("expected `{shown}`")),
                    );
                    return Ok(());
                }
                self.push_scope();
                self.level += 1;
                for (p, &pty) in params.iter().zip(ptys.iter()) {
                    if let Some(t) = p.ty() {
                        let ann = self.lower_ty(t);
                        let name = p
                            .name()
                            .map(|n| self.text(n.span))
                            .unwrap_or_else(|| "_".to_string());
                        let exp2 = Expect {
                            ty: pty,
                            reason: Reason::LetAnnotation(name),
                            because: Some(t.span),
                        };
                        self.expect_unify(t.span, ann, &exp2);
                    }
                    if let Some(n) = p.name() {
                        let name = self.text(n.span);
                        self.bind(name, n.span, pty);
                    }
                }
                let was = self.in_closure;
                self.in_closure = true;
                if let Some(body) = d.body() {
                    let exp2 = Expect {
                        ty: ret,
                        reason: Reason::ClosureBody,
                        because: Some(e.span),
                    };
                    self.check_expr(body, &exp2)?;
                }
                self.in_closure = was;
                self.level -= 1;
                self.pop_scope();
                self.record(e.span, expected);
                Ok(())
            }
            TyKind::ErrUnion(inner, _) => {
                let sub = Expect {
                    ty: inner,
                    reason: exp.reason.clone(),
                    because: exp.because,
                };
                self.check_closure(e, &sub)
            }
            _ => {
                let t = self.synth_closure(e)?;
                self.record(e.span, t);
                self.expect_unify(e.span, t, exp);
                Ok(())
            }
        }
    }

    fn synth_closure(&mut self, e: &GreenNode) -> R<TyId> {
        let d = ClosureExpr::cast(e).expect("kind");
        let params: Vec<_> = d.params().into_iter().flat_map(|p| p.params()).collect();
        self.push_scope();
        self.level += 1;
        let mut ptys = Vec::new();
        for p in &params {
            let ty = match p.ty() {
                Some(t) => self.lower_ty(t),
                // Unannotated and uncontexted: a deeper-level
                // existential; if nothing pins it, E0405 at body end.
                None => {
                    let span = p.name().map(|n| n.span).unwrap_or(p.syntax().span);
                    self.fresh(NumKind::Any, span)
                }
            };
            if let Some(n) = p.name() {
                let name = self.text(n.span);
                self.bind(name, n.span, ty);
            }
            ptys.push(ty);
        }
        let was = self.in_closure;
        self.in_closure = true;
        let ret = match d.body() {
            Some(body) => self.synth_expr(body)?,
            None => self.lo.table.unit(),
        };
        self.in_closure = was;
        self.level -= 1;
        self.pop_scope();
        Ok(self.lo.table.intern(TyKind::Fn(ptys, ret)))
    }

    // ---------------------------------------------------- defaulting ---

    /// The defaulting *rule* (never a solver): at body end, unresolved
    /// `{integer}` vars become `i32`, `{float}` (and bare `{number}`)
    /// vars become `f64`/`i32`; a fully unconstrained var is E0405.
    fn finish_defaulting(&mut self) {
        // Cannot-infer is reported only in otherwise-clean bodies: when
        // a real error already fired, an unresolved var is almost
        // always its shadow, and one root cause means one diagnostic.
        let clean = self.diags.is_empty();
        for (v, kind, origin) in self.vars.unresolved_roots() {
            let var_ty = self.lo.table.intern(TyKind::Var(v));
            match kind {
                NumKind::Integer | NumKind::Num => {
                    let i32_ = self.lo.table.prim(Prim::I32);
                    let _ = unify(&mut self.lo.table, &mut self.vars, var_ty, i32_);
                }
                NumKind::Float => {
                    let f64_ = self.lo.table.prim(Prim::F64);
                    let _ = unify(&mut self.lo.table, &mut self.vars, var_ty, f64_);
                }
                NumKind::Any => {
                    if !clean {
                        continue;
                    }
                    self.diags.push(
                        Diagnostic::error(
                            codes::E0405,
                            origin,
                            "wolf cannot infer a type here".to_string(),
                        )
                        .with_label("no use of this pins its type down")
                        .with_note(
                            "closure parameters take their types from context; when \
                             there is no context, annotate: `fn(x: int) …`. Literals \
                             default (`i32`/`f64`) by rule, but nothing else does.",
                        ),
                    );
                }
            }
        }
    }
}

/// The single bound name of a simple pattern (for provenance wording).
/// The closed member table of each reflection kind (s16).
fn meta_members(m: MetaTy) -> &'static [&'static str] {
    match m {
        MetaTy::TypeInfo => &["kind", "name", "fields", "variants", "traits"],
        MetaTy::FieldList | MetaTy::VariantList | MetaTy::TraitList => &["len"],
        MetaTy::Field => &["name", "ty"],
    }
}

/// The type of one reflection member, if it exists.
fn meta_member_ty(m: MetaTy, member: &str) -> Option<TyKind> {
    Some(match (m, member) {
        (MetaTy::TypeInfo, "kind" | "name") => TyKind::Prim(Prim::Str),
        (MetaTy::TypeInfo, "fields") => TyKind::Meta(MetaTy::FieldList),
        (MetaTy::TypeInfo, "variants") => TyKind::Meta(MetaTy::VariantList),
        (MetaTy::TypeInfo, "traits") => TyKind::Meta(MetaTy::TraitList),
        (MetaTy::FieldList | MetaTy::VariantList | MetaTy::TraitList, "len") => {
            TyKind::Prim(Prim::Int)
        }
        (MetaTy::Field, "name") => TyKind::Prim(Prim::Str),
        (MetaTy::Field, "ty") => TyKind::TypeTy,
        _ => return None,
    })
}

/// Parse an integer literal's text (underscores, `0x`/`0o`/`0b`) into
/// the exhaustiveness engine's value identity.
fn parse_int_text(text: &str) -> Option<i128> {
    let t: String = text.chars().filter(|c| *c != '_').collect();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        i128::from_str_radix(h, 16).ok()
    } else if let Some(o) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
        i128::from_str_radix(o, 8).ok()
    } else if let Some(b) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        i128::from_str_radix(b, 2).ok()
    } else {
        t.parse().ok()
    }
}

fn single_ident(pat: &GreenNode, src: &[u8]) -> Option<String> {
    if pat.kind == SyntaxKind::IdentPat {
        let t = pat.child_token(SyntaxKind::Ident)?;
        return Some(String::from_utf8_lossy(t.text(src)).into_owned());
    }
    None
}
