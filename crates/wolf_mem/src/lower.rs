//! Typed-body → check-CFG lowering (s18).
//!
//! The walker consumes sema's typed HIR surface — the AST plus
//! [`TypedBody`]'s side tables (`locals` for binding types, `calls`
//! for every resolved call's declared mode surface) — and produces the
//! effect CFG of [`crate::cfg`]. Everything it cannot lower soundly
//! refuses with an honest [`NotYet`], never a guess: regions are
//! s19–s20, `shared`/`handle` are s21, the unsafe tier is s22,
//! closures/concurrency are c05.
//!
//! Context-free rules are checked *during* lowering, because they need
//! no dataflow: call-site mode agreement (E1007, the c03 handoff
//! decided here — one diagnostic, one place), `mut`-needs-a-place
//! (E1009), and the callee-side view-set footprint (E1008,
//! `[mem.tier0.excl.3]`). Path-sensitive rules (E1001, E1002) run as
//! separate passes over the finished CFG.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use wolf_ast::FreezeExpr;

use wolf_ast::{
    Arg, AssignStmt, Block as AstBlock, CallExpr, CastExpr, DeferStmt, ElseExpr, ExprStmt,
    FieldInit, ForExpr, GreenNode, IfExpr, InBlock, MatchExpr, MemberExpr, ParamMode, ParenExpr,
    PathExpr, PrefixExpr, RangeExpr, RegionBlock, RegionValue, ReturnExpr, StringExpr, StructLit,
    SyntaxKind, TupleExpr, WhileExpr, is_pattern_kind,
};
use wolf_diag::{Applicability, Diagnostic, Suggestion, codes};
use wolf_span::Span;

use wolf_sema::check::{CallSig, CastKind};
use wolf_sema::sig::{ItemSig, ParamSig, SigTables};
use wolf_sema::types::{Prim, TyId, TyKind, TypeTable, render};
use wolf_sema::{NotYet, Package, TypedBody};

use crate::cfg::{
    AllocSite, Block, BlockId, CallSurface, Cfg, Local, LocalId, Region, RegionId, RegionKind,
    SiteId, SiteKind, Stmt, Strategy,
};
use crate::place::{Base, Place, PlaceId, Proj};
use crate::regions::{Binding, RegionSummary, RegionTable, Unify};

type R<T> = Result<T, NotYet>;

/// A value's region provenance, threaded through evaluation (s19):
/// which allocation sites the value may contain (a may-set — never
/// strong-updated, so joins are unions and the analysis stays sound
/// over branches), and which first-class region it denotes, when one
/// is statically known.
#[derive(Debug, Clone, Default)]
pub(crate) struct Val {
    /// Sorted, deduped site ids.
    sites: Vec<SiteId>,
    /// `Some(r)`: this expression denotes region `r` (X4 value flow).
    region: Option<RegionId>,
    /// The span that produced the first site (diagnostic anchor).
    origin: Option<Span>,
    /// s20: fields of this value that hold a region value with known
    /// identity (`Holder { child: move c }` records `("child", c)`),
    /// so `in h.child { }` can resolve the iso edge it opens through.
    region_fields: Vec<(String, RegionId)>,
    /// s98 (D47): this value BORROWS the named places. A dyn pair's
    /// data half borrows its cast operand's place (one entry); a
    /// capturing closure's env borrows every captured place (s105 —
    /// the same design, plural). Set only on the producing
    /// expression's own value; the borrow is claimed where the value
    /// lands — a binding grows shared loans, a call argument joins
    /// the read surface, anything else refuses by name at body end.
    borrowed: Vec<PlaceId>,
}

impl Val {
    fn none() -> Val {
        Val::default()
    }

    fn site(id: SiteId, span: Span) -> Val {
        Val {
            sites: vec![id],
            region: None,
            origin: Some(span),
            region_fields: Vec::new(),
            borrowed: Vec::new(),
        }
    }

    fn merge(&mut self, other: Val) {
        for s in other.sites {
            if let Err(i) = self.sites.binary_search(&s) {
                self.sites.insert(i, s);
            }
        }
        if self.origin.is_none() {
            self.origin = other.origin;
        }
        // Two different region values merging (branch arms) lose
        // static identity.
        self.region = match (self.region, other.region) {
            (Some(a), Some(b)) if a == b => Some(a),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            _ => None,
        };
        self.region_fields.extend(other.region_fields);
    }
}

/// Does this signature-table type mention a generic parameter
/// anywhere (s94)? Fields that do — beyond the bare-parameter case —
/// need substitution this module's immutable view cannot intern, so
/// their struct answers opaquely and the walk refuses by name.
fn mentions_rigid(table: &TypeTable, ty: TyId) -> bool {
    match table.kind(ty) {
        TyKind::Rigid(_) => true,
        TyKind::Wrapping(t)
        | TyKind::Range(t)
        | TyKind::Ptr(t)
        | TyKind::Shared(t)
        | TyKind::Handle(t)
        | TyKind::Weak(t)
        | TyKind::Distinct(t)
        | TyKind::List(t)
        | TyKind::Pool(t)
        | TyKind::Chan(t)
        | TyKind::Mutex(t) => mentions_rigid(table, *t),
        TyKind::Tuple(ts) => ts.iter().any(|t| mentions_rigid(table, *t)),
        TyKind::Fn(ps, r) => {
            ps.iter().any(|t| mentions_rigid(table, *t)) || mentions_rigid(table, *r)
        }
        TyKind::ErrUnion(a, b) => mentions_rigid(table, *a) || mentions_rigid(table, *b),
        TyKind::Row { tags, tail } => {
            tags.iter()
                .any(|(_, ps)| ps.iter().any(|t| mentions_rigid(table, *t)))
                || tail.is_some_and(|t| mentions_rigid(table, t))
        }
        TyKind::Nominal { args, .. } => args.iter().any(|t| mentions_rigid(table, *t)),
        TyKind::Proj(t, _) => mentions_rigid(table, *t),
        _ => false,
    }
}

/// A type reference that knows its interner: sema keeps signature
/// types in [`SigTables::table`] and body types in the body's own
/// table, and place typing crosses between them at `Nominal` fields.
#[derive(Clone, Copy)]
pub(crate) struct Ty<'a> {
    table: &'a TypeTable,
    id: TyId,
}

impl<'a> Ty<'a> {
    fn kind(&self) -> &'a TyKind {
        self.table.kind(self.id)
    }
}

/// Is the type `Copy` — implicit copy on what would otherwise move
/// (`[mem.tier0.move.3]`, POD-shaped only)? The set mirrors the
/// reference interpreter's dynamic judgement exactly: scalars, `str`
/// (immutable views, D25), ranges, fn values, `handle`, raw pointers.
/// Aggregates — structs, enums, tuples, rows — are not.
fn is_copy(t: Ty<'_>, depth: u32) -> bool {
    if depth > 32 {
        return false;
    }
    match t.kind() {
        TyKind::Error | TyKind::Never | TyKind::Unit | TyKind::Prim(_) => true,
        TyKind::Wrapping(_)
        | TyKind::Fn(..)
        | TyKind::Range(_)
        | TyKind::Handle(_)
        | TyKind::Ptr(_)
        | TyKind::TypeTy => true,
        // s73 — the conc capability handles: a channel/scope/sync
        // value copied to another task crosses as a new reference to
        // the SHARED runtime object (the interior is runtime-
        // synchronized); proc ids and exit reasons are plain words
        // ([conc.proc.1], [conc.proc.exit]).
        TyKind::Chan(_)
        | TyKind::Mutex(_)
        | TyKind::TaskScope
        | TyKind::Proc
        | TyKind::ExitReason => true,
        TyKind::Distinct(inner) => is_copy(
            Ty {
                table: t.table,
                id: *inner,
            },
            depth + 1,
        ),
        _ => false,
    }
}

/// One lowered body: the CFG, the context-free diagnostics found on
/// the way, and the region inference record (s19).
pub struct Lowered {
    pub cfg: Cfg,
    pub diags: Vec<Diagnostic>,
    pub regions: RegionSummary,
    /// s21 — `shared`/`weak` cell allocation sites with the static
    /// atomicity bit (true = a freeze/transfer point reaches the
    /// cell; every other cell is provably thread-exclusive).
    pub rc_cells: Vec<(SiteId, bool)>,
}

struct Scope<'t> {
    names: Vec<(String, LocalId)>,
    /// Scope-exit obligations in declaration order: `defer`s and s21
    /// RC drops interleave in ONE LIFO sequence ([mem.shared.drop.1]
    /// — a destructor-carrying value has an implicit use at scope
    /// exit, LIFO with `defer`/`errdefer`).
    cleanup: Vec<Cleanup<'t>>,
    /// `locals.len()` at scope entry: locals below this mark outlive
    /// the scope (the region-close escape sweep's boundary).
    locals_mark: usize,
}

/// One scope-exit cleanup obligation.
#[derive(Clone, Copy)]
enum Cleanup<'t> {
    /// `defer` / `errdefer` (bool = errdefer).
    Defer(&'t GreenNode, bool),
    /// s21: a `shared`/`weak`-typed local's recorded RC drop
    /// (drop-if-live; the c05 lowering owns the real decrement).
    DropLocal(LocalId),
}

struct LoopFrame {
    break_to: BlockId,
    continue_to: BlockId,
    scope_depth: usize,
}

pub(crate) struct Lowerer<'t> {
    pkg: &'t Package,
    sigs: &'t SigTables,
    tb: &'t TypedBody,
    module: usize,
    file: usize,
    src: &'t [u8],
    /// span → CallSig, from [`TypedBody::calls`].
    calls: HashMap<Span, &'t CallSig>,
    /// binding-ident span → body-table type, from [`TypedBody::locals`].
    local_tys: HashMap<Span, TyId>,
    /// span → body-table type, from [`TypedBody::exprs`] (checked-op
    /// classification only).
    expr_tys: HashMap<Span, TyId>,

    blocks: Vec<Block>,
    locals: Vec<Local>,
    /// Per-local type (walker-side; the CFG keeps only a rendering).
    tys: Vec<Option<Ty<'t>>>,
    places: crate::place::PlaceTable,
    cur: BlockId,
    exit: BlockId,
    scopes: Vec<Scope<'t>>,
    loops: Vec<LoopFrame>,
    /// Callee-side view-set context: (`self` local, viewed fields,
    /// declaration span).
    view: Option<(LocalId, Vec<String>, Span)>,
    /// Re-entrancy guard: while defers are being lowered at an exit,
    /// nested error edges do not re-lower them.
    in_defer: bool,
    diags: Vec<Diagnostic>,

    // ------------------------------------------ region inference ----
    /// Region variables + unification state (s19).
    rt: RegionTable,
    /// The ambient-region stack: `[0]` is the caller (or static, for
    /// item initializers); `region name {` / `in r {` push.
    ambient: Vec<RegionId>,
    /// Allocation sites, walk order.
    sites: Vec<AllocSite>,
    /// Per site: why the value may leave the frame (`None`: in-frame).
    site_escape: Vec<Option<&'static str>>,
    /// May-hold: root local -> the sites its value may contain, each
    /// with the span where it flowed in (diagnostic anchor).
    holds: BTreeMap<u32, BTreeMap<SiteId, Span>>,
    /// Region-typed locals with statically-known identity (`None`:
    /// bound on conflicting paths — `in` on it refuses).
    region_local: HashMap<u32, Option<RegionId>>,
    /// Per region: the region value moved away (returned, sent,
    /// stored) — its free is no longer this frame's.
    moved_region: Vec<bool>,
    /// Per region: an escape/placement error involved it (no
    /// promotion).
    tainted_region: Vec<bool>,
    /// s105: the region's handle was LENT to a callee (passed as an
    /// argument or receiver). A callee may allocate through it (D12's
    /// ambient law), so promotion's "nothing ever lands here and the
    /// handle never leaves the frame" proof no longer holds — the
    /// region keeps its runtime create/free and W1001 stays quiet.
    lent_region: Vec<bool>,
    /// Frame-local clean regions: the promotion fact.
    promoted: Vec<RegionId>,
    /// Any placement demand failed (E1004/E1010 fired).
    conflicted: bool,
    /// Move sites that are binding patterns (s128 #173): E1001 skips
    /// the `copy`-at-the-move fix-it there (`copy` is not pattern
    /// grammar).
    pattern_moves: Vec<Span>,
    /// `ρ_static`, once demanded (fn bodies; item initializers use it
    /// as their ambient).
    static_region: Option<RegionId>,

    // ------------------------------------------ region checker (s20) ----
    /// The open set as a stack of (region, open-site) pairs — every
    /// `region name {` / `in r {` entry, popped at its exit. The
    /// ambient stack minus the caller/static base. Openness is
    /// depth-counted: the same region may appear twice
    /// (`[mem.region.open.1]`).
    open_stack: Vec<(RegionId, Span)>,
    /// The region forest's iso edges: child region -> the region of
    /// the aggregate it was embedded into (`[mem.region.edge.iso]`).
    /// Keyed by raw region index (rigid regions never merge).
    region_parent: HashMap<u32, RegionId>,
    /// Regions consumed by `freeze`: their data is `imm` forever
    /// (`[mem.region.freeze.1]`), keyed by raw index, value = the
    /// freeze site (diagnostic anchor).
    frozen_region: HashMap<u32, Span>,
    /// Allocation sites promoted to `imm` by a freeze: writes through
    /// any path holding one are E1012; stores of one are exempt from
    /// co-location (`[mem.region.edge.imm]`).
    frozen_site: BTreeMap<SiteId, Span>,
    /// Field paths with statically-known region identity:
    /// `(root local, field name) -> region` (`h.child` after
    /// `let h = Holder { child: move c }`).
    region_field: HashMap<(u32, String), RegionId>,

    // ------------------------------------------ the shared tier (s21) ----
    /// Allocation sites owned by a `shared` cell (the cell itself and
    /// the payload it captured): RC frees them, not a region, so every
    /// region demand exempts them exactly like `imm` data — the
    /// `[mem.region.edge]` Tier-2 column's ✅.
    shared_site: BTreeSet<SiteId>,
    /// The cell sites proper (creation + `clone`/`downgrade`
    /// results), in mint order — the fact record's spine.
    rc_cells: Vec<SiteId>,

    // ------------------------------------- iteration claims (s72, D40) ----
    /// The places the enclosing `for` loops iterate, innermost last —
    /// each holds a read claim for its loop's extent
    /// (`[mem.iter.excl]`); mut uses of a conflicting path inside are
    /// E1013 at their emission sites.
    iter_claims: Vec<(PlaceId, Span)>,

    // ------------------------------------------ the unsafe tier (s22) ----
    /// `unsafe { }` nesting depth: the ring the raw-tier operations
    /// demand (E1301 outside; `[mem.unsafe.scope]`).
    unsafe_depth: usize,
    /// span → (source ty, target ty, kind), from [`TypedBody::casts`]
    /// — raw pointer bridges gate on the ring and emit expose facts.
    casts: HashMap<Span, (TyId, TyId, CastKind)>,

    // -------------------------------------------- dyn pairs (s98, D47) ----
    /// First-class loans this body created (dyn casts claimed by a
    /// binding); the NLL engine ([`crate::loans`]) scopes them by the
    /// borrower's liveness.
    loans: Vec<crate::cfg::Loan>,
    /// Borrowing expressions (dyn casts, capturing closures) produced
    /// but not yet CLAIMED (bound or passed). One left at body end is
    /// a value in a position the conservative reading does not admit
    /// — refuse by name, never let it flow somewhere the borrow
    /// cannot be seen. The bool: the entry is a closure (it names its
    /// own refusal).
    unclaimed_pairs: Vec<(Span, bool)>,
}

impl<'t> Lowerer<'t> {
    fn text(&self, span: Span) -> String {
        String::from_utf8_lossy(&self.src[span.lo as usize..span.hi as usize]).into_owned()
    }

    // ---------------------------------------------------- blocks ----

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(Block::default());
        id
    }

    fn goto(&mut self, from: BlockId, to: BlockId) {
        let b = &mut self.blocks[from.0 as usize];
        if !b.succs.contains(&to) {
            b.succs.push(to);
        }
    }

    fn push(&mut self, s: Stmt) {
        self.blocks[self.cur.0 as usize].stmts.push(s);
    }

    // ---------------------------------------------------- locals ----

    fn declare(&mut self, name: &str, span: Span, ty: Option<Ty<'t>>) -> LocalId {
        let id = LocalId(self.locals.len() as u32);
        let rendered = ty
            .map(|t| render(t.table, t.id, &|_| Err("_")))
            .unwrap_or_else(|| "?".to_string());
        self.locals.push(Local {
            name: name.to_string(),
            span,
            ty: rendered,
            is_copy: ty.map(|t| is_copy(t, 0)).unwrap_or(false),
            param_mode: None,
        });
        self.tys.push(ty);
        if let Some(scope) = self.scopes.last_mut() {
            scope.names.push((name.to_string(), id));
        }
        id
    }

    fn lookup(&self, name: &str) -> Option<LocalId> {
        for scope in self.scopes.iter().rev() {
            for (n, id) in scope.names.iter().rev() {
                if n == name {
                    return Some(*id);
                }
            }
        }
        None
    }

    fn push_scope(&mut self) {
        let mark = self.locals.len();
        self.scopes.push(Scope {
            names: Vec::new(),
            cleanup: Vec::new(),
            locals_mark: mark,
        });
    }

    // --------------------------------------------------- regions ----

    fn new_region(
        &mut self,
        name: &str,
        kind: RegionKind,
        strategy: Strategy,
        span: Span,
    ) -> RegionId {
        let id = self.rt.new_region(Region {
            name: name.to_string(),
            kind,
            strategy,
            span,
        });
        self.moved_region.push(false);
        self.tainted_region.push(false);
        self.lent_region.push(false);
        id
    }

    /// The current ambient region (`[mem.region.create.3]`).
    fn ambient(&self) -> RegionId {
        *self.ambient.last().expect("ambient stack never empty")
    }

    /// Record an allocation at the ambient region and emit its CFG
    /// statement — the no-hidden-allocations law: every allocation is
    /// attributable in the dump.
    fn alloc_site(&mut self, ty: String, kind: SiteKind, span: Span) -> SiteId {
        let id = SiteId(self.sites.len() as u32);
        self.sites.push(AllocSite {
            span,
            ty,
            region: self.ambient(),
            kind,
        });
        self.site_escape.push(None);
        if kind != SiteKind::Param {
            self.push(Stmt::Alloc { site: id, span });
        }
        id
    }

    fn mark_escape(&mut self, site: SiteId, why: &'static str) {
        let slot = &mut self.site_escape[site.0 as usize];
        if slot.is_none() {
            *slot = Some(why);
        }
    }

    /// s105: a region binding's handle crossing a call boundary — see
    /// `lent_region`.
    fn mark_region_lent(&mut self, place: PlaceId) {
        let pl = self.places.get(place);
        if !pl.proj.is_empty() {
            return;
        }
        let Base::Local(l) = pl.base else { return };
        if let Some(Some(rid)) = self.region_local.get(&l).copied() {
            self.lent_region[rid.0 as usize] = true;
        }
    }

    fn mark_val_escape(&mut self, val: &Val, why: &'static str) {
        for &s in &val.sites {
            self.mark_escape(s, why);
        }
    }

    /// Sites a place's value may contain (keyed by the root local; a
    /// field projection conservatively shares the whole value's set).
    fn sites_of_place(&self, place: PlaceId) -> Vec<(SiteId, Span)> {
        match self.places.get(place).base {
            Base::Local(l) => self
                .holds
                .get(&l)
                .map(|m| m.iter().map(|(&s, &sp)| (s, sp)).collect())
                .unwrap_or_default(),
            Base::Global(..) => Vec::new(),
        }
    }

    fn val_of_place(&self, place: PlaceId, span: Span) -> Val {
        let mut sites: Vec<SiteId> = self
            .sites_of_place(place)
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        sites.sort_unstable();
        sites.dedup();
        let region = match self.places.get(place).base {
            Base::Local(l) if self.places.get(place).proj.is_empty() => {
                self.region_local.get(&l).copied().flatten()
            }
            _ => None,
        };
        Val {
            sites,
            region,
            origin: Some(span),
            region_fields: Vec::new(),
            borrowed: Vec::new(),
        }
    }

    /// s98: claim a produced dyn pair — its cast span leaves the
    /// unclaimed list because the borrow found a home (a binding's
    /// loan or a call's read surface).
    fn claim_dyn(&mut self, val: &Val) {
        if val.borrowed.is_empty() {
            return;
        }
        if let Some(origin) = val.origin
            && let Some(i) = self.unclaimed_pairs.iter().position(|&(s, _)| s == origin)
        {
            self.unclaimed_pairs.remove(i);
        }
    }

    fn hold(&mut self, local: u32, val: &Val, span: Span) {
        if val.sites.is_empty() {
            return;
        }
        let entry = self.holds.entry(local).or_default();
        for &s in &val.sites {
            entry.entry(s).or_insert(span);
        }
    }

    /// Render a region for diagnostics — introductions, never
    /// internal variable names.
    fn show_region(&mut self, id: RegionId) -> String {
        match self.rt.binding(id) {
            Some(Binding::Caller) => "the caller's region".to_string(),
            Some(Binding::Static) => "the static region (module state)".to_string(),
            Some(Binding::Local(r)) => {
                let r = &self.rt.regions[r.0 as usize];
                format!("region `{}`", r.name)
            }
            None => {
                let r = &self.rt.regions[id.0 as usize];
                format!("`{}`'s region", r.name)
            }
        }
    }

    /// A region's introduction span (for the "created here"
    /// secondary).
    fn region_span(&mut self, id: RegionId) -> Option<(String, Span)> {
        match self.rt.binding(id) {
            Some(Binding::Local(r)) => {
                let r = &self.rt.regions[r.0 as usize];
                Some((r.name.clone(), r.span))
            }
            _ => None,
        }
    }

    /// The checked type of an expression, when sema recorded one.
    fn expr_ty(&self, span: Span) -> Option<Ty<'t>> {
        self.expr_tys.get(&span).map(|&id| Ty {
            table: &self.tb.table,
            id,
        })
    }

    /// The rendered type of a checked expression (dump surface only).
    fn rendered_expr_ty(&self, span: Span) -> String {
        self.expr_tys
            .get(&span)
            .map(|&id| render(&self.tb.table, id, &|_| Err("_")))
            .unwrap_or_else(|| "?".to_string())
    }

    /// The `ρ_static` variable, created on first demand (module state).
    fn static_region(&mut self, span: Span) -> RegionId {
        if let Some(id) = self.static_region {
            return id;
        }
        let id = self.new_region("static", RegionKind::Static, Strategy::Arena, span);
        self.static_region = Some(id);
        id
    }

    /// The shared fix-ladder note (Target 6): allocation-site
    /// vocabulary, never lifetimes or internal variable names.
    fn ladder() -> &'static str {
        "to keep the value, allocate it where it must live: build it outside the \
         region block, or aim the allocation at a longer-lived region explicitly \
         (`let r = region()` … `in r { … }`); widening the region block to cover \
         every use also works. Two keep-alive alternatives change the ownership \
         instead: `freeze` the region (immutable forever) or make the value a \
         `shared` cell (reference-counted, never dangles)."
    }

    /// One report per site: a site whose placement already conflicted
    /// stays quiet in later demands (no cascades off one bug).
    fn already_conflicted(&self, site: SiteId) -> bool {
        self.site_escape[site.0 as usize] == Some("conflicting placement")
    }

    /// An embedding store: moving a value into an aggregate demands
    /// co-location — wolf has no safe cross-region references outside
    /// `iso`/`imm` (which are s20), so the `[mem.region.edge]` table's
    /// ❌ column reports here as E1004.
    fn demand_store(&mut self, val: &Val, target: RegionId, container: Option<Span>, span: Span) {
        for &s in &val.sites {
            if self.already_conflicted(s) {
                continue;
            }
            // Frozen (`imm`) data may be referenced from anywhere,
            // forever ([mem.region.edge.imm]): no co-location demand.
            if self.frozen_site.contains_key(&s) {
                continue;
            }
            // Tier-2 `shared` cells are the sanctioned cross-region
            // edge (s21, X5: never dangles) — no co-location demand.
            if self.shared_site.contains(&s) {
                continue;
            }
            let sr = self.sites[s.0 as usize].region;
            // Render both sides before unifying: after a merge they
            // resolve identically.
            let from = self.show_region(sr);
            let into = self.show_region(target);
            match self.rt.unify(target, sr) {
                Unify::Ok => {}
                Unify::ParamsMerged => {
                    self.conflicted = true;
                    self.mark_escape(s, "conflicting placement");
                    let alloc = self.sites[s.0 as usize].span;
                    let mut d = Diagnostic::error(
                        codes::E1004,
                        span,
                        format!(
                            "this stores a value from {from} into {into} — \
                             parameter regions are independent by default"
                        ),
                    )
                    .with_label("the store that would tie the two regions together")
                    .with_secondary(alloc, format!("the value arrives here, in {from}"));
                    if let Some(c) = container {
                        d = d.with_secondary(c, format!("the container lives in {into}"));
                    }
                    d = d.with_note(
                        "each parameter's data lives in its own (generalized) region — \
                         the Cyclone default that keeps signatures annotation-free. A \
                         signature surface for declaring two parameters share a region \
                         is planned; today, `copy` the value into its container's \
                         region, or restructure so the allocation happens alongside \
                         the container.",
                    );
                    self.diags.push(d);
                    return;
                }
                Unify::Conflict(a, b) => {
                    self.conflicted = true;
                    self.mark_escape(s, "conflicting placement");
                    for side in [a, b] {
                        if let Binding::Local(r) = side {
                            self.tainted_region[r.0 as usize] = true;
                        }
                    }
                    let alloc = self.sites[s.0 as usize].span;
                    let mut d = Diagnostic::error(
                        codes::E1004,
                        span,
                        format!(
                            "this value is allocated in {from}, but it is being \
                             stored into a value living in {into}"
                        ),
                    )
                    .with_label("the two regions must be one")
                    .with_secondary(alloc, format!("allocated here, into {from}"));
                    if let Some(c) = container {
                        d = d.with_secondary(
                            c,
                            format!("the container is allocated here, into {into}"),
                        );
                    }
                    if let Some((name, intro)) = self.region_span(sr) {
                        d = d.with_secondary(intro, format!("region `{name}` is created here"));
                    }
                    d = d.with_note(Self::ladder());
                    self.diags.push(d);
                    return;
                }
            }
        }
    }

    /// A value leaves the frame (returned, or the function's trailing
    /// value): its region must outlive the caller — `ρ_static` always
    /// does, flexible parameter regions join `ρ_caller` (the default
    /// effect), and a frame-local region is an escape (E1010).
    fn demand_outlives_frame(&mut self, val: &Val, span: Span) {
        for &s in &val.sites {
            if self.already_conflicted(s) {
                continue;
            }
            // Frozen data outlives everything ([mem.region.freeze.1]:
            // immutable forever — returning it is always legal).
            if self.frozen_site.contains_key(&s) {
                self.mark_escape(s, "returned");
                continue;
            }
            // A `shared` cell outlives any frame while a strong count
            // holds it (s21: RC frees it, never a region).
            if self.shared_site.contains(&s) {
                self.mark_escape(s, "returned");
                continue;
            }
            let sr = self.sites[s.0 as usize].region;
            match self.rt.binding(sr) {
                Some(Binding::Static) | Some(Binding::Caller) => {
                    self.mark_escape(s, "returned");
                }
                None => {
                    // Fresh parameter region: the default scheme's
                    // "result is in the caller's region" binds it.
                    let caller = self.ambient[0];
                    let _ = self.rt.unify(caller, sr);
                    self.mark_escape(s, "returned");
                }
                Some(Binding::Local(rid)) => {
                    self.conflicted = true;
                    self.tainted_region[rid.0 as usize] = true;
                    self.mark_escape(s, "outlives its region");
                    let region = self.show_region(sr);
                    let alloc = self.sites[s.0 as usize].span;
                    let mut d = Diagnostic::error(
                        codes::E1010,
                        span,
                        format!(
                            "this value is allocated in {region}, which is freed \
                             before the caller could ever use it"
                        ),
                    )
                    .with_label("the value would outlive its region")
                    .with_secondary(alloc, format!("allocated here, into {region}"));
                    if let Some((name, intro)) = self.region_span(sr) {
                        d = d.with_secondary(
                            intro,
                            format!(
                                "region `{name}` is created here and freed with its \
                                 scope — everything in it is freed wholesale"
                            ),
                        );
                    }
                    d = d.with_note(Self::ladder());
                    self.diags.push(d);
                }
            }
        }
        // A region value leaving the frame is a legal affine move
        // (X4: region values outlive lexical scopes by design); its
        // free is simply no longer ours.
        if let Some(r) = val.region {
            self.moved_region[r.0 as usize] = true;
        }
    }

    /// A store into module state: the target is `ρ_static`.
    fn demand_static(&mut self, val: &Val, span: Span) {
        for &s in &val.sites {
            // Frozen data may live anywhere, forever
            // ([mem.region.edge.imm]).
            if self.frozen_site.contains_key(&s) {
                self.mark_escape(s, "module state");
                continue;
            }
            // `shared` cells: RC-owned, storable anywhere (s21).
            if self.shared_site.contains(&s) {
                self.mark_escape(s, "module state");
                continue;
            }
            let sr = self.sites[s.0 as usize].region;
            match self.rt.binding(sr) {
                Some(Binding::Static) => {
                    self.mark_escape(s, "module state");
                }
                None => {
                    let st = self.static_region(span);
                    let _ = self.rt.unify(st, sr);
                    self.mark_escape(s, "module state");
                }
                Some(Binding::Caller) | Some(Binding::Local(_)) => {
                    self.conflicted = true;
                    self.mark_escape(s, "module state");
                    let region = self.show_region(sr);
                    if let Some(Binding::Local(rid)) = self.rt.binding(sr) {
                        self.tainted_region[rid.0 as usize] = true;
                    }
                    let alloc = self.sites[s.0 as usize].span;
                    let mut d = Diagnostic::error(
                        codes::E1010,
                        span,
                        format!(
                            "module state outlives every call, but this value is \
                             allocated in {region}, which does not"
                        ),
                    )
                    .with_label("stored into module state here")
                    .with_secondary(alloc, format!("allocated here, into {region}"));
                    d = d.with_note(Self::ladder());
                    self.diags.push(d);
                }
            }
        }
    }

    /// `[mem.region.intra.2]`: the region dies as a unit at its
    /// close. Anything declared outside it (or the closing block's
    /// own value) still holding its data is E1010; a clean,
    /// frame-local, unmoved region is the promotion fact (Target 5).
    fn sweep_region_close(
        &mut self,
        rid: RegionId,
        outer_mark: usize,
        close_span: Span,
        val: &mut Val,
    ) {
        // The block's own value escaping the dying region.
        let mut kept = Vec::new();
        for &s in &val.sites.clone() {
            // s21: shared cells survive their creation region — RC
            // owns them (X5: `shared` never dangles).
            if self.shared_site.contains(&s) {
                kept.push(s);
                continue;
            }
            if self.rt.same(self.sites[s.0 as usize].region, rid) {
                if !self.already_conflicted(s) {
                    self.report_close_escape(
                        rid,
                        s,
                        val.origin.unwrap_or(close_span),
                        close_span,
                        None,
                    );
                }
            } else {
                kept.push(s);
            }
        }
        val.sites = kept;
        // Bindings that outlive the region.
        let entries: Vec<(u32, SiteId, Span)> = self
            .holds
            .iter()
            .filter(|(l, _)| (**l as usize) < outer_mark)
            .flat_map(|(&l, m)| m.iter().map(move |(&s, &sp)| (l, s, sp)))
            .collect();
        for (l, s, sp) in entries {
            if self.shared_site.contains(&s) {
                continue; // s21: RC-owned, outlives the region freely
            }
            if self.rt.same(self.sites[s.0 as usize].region, rid) && !self.already_conflicted(s) {
                let name = self.locals[l as usize].name.clone();
                self.report_close_escape(rid, s, sp, close_span, Some(name));
            }
        }
        if !self.tainted_region[rid.0 as usize]
            && !self.moved_region[rid.0 as usize]
            && !self.lent_region[rid.0 as usize]
        {
            self.promoted.push(rid);
        }
    }

    fn report_close_escape(
        &mut self,
        rid: RegionId,
        site: SiteId,
        escape_span: Span,
        close_span: Span,
        binding: Option<String>,
    ) {
        self.conflicted = true;
        self.tainted_region[rid.0 as usize] = true;
        self.mark_escape(site, "outlives its region");
        let region = self.show_region(rid);
        let alloc = self.sites[site.0 as usize].span;
        let message = match &binding {
            Some(name) => format!(
                "`{name}` still holds a value allocated in {region} when the \
                 region is freed"
            ),
            None => format!(
                "this value is allocated in {region}, which is freed when the \
                 block ends"
            ),
        };
        let mut d = Diagnostic::error(codes::E1010, escape_span, message)
            .with_label(match &binding {
                Some(_) => "the value flows out of the region here",
                None => "the block's value would outlive the region",
            })
            .with_secondary(alloc, format!("allocated here, into {region}"));
        if let Some((name, intro)) = self.region_span(rid) {
            d = d.with_secondary(intro, format!("region `{name}` is created here"));
        }
        d = d.with_secondary(
            close_span,
            "the region is freed here — everything in it is freed wholesale, as one unit",
        );
        d = d.with_note(Self::ladder());
        self.diags.push(d);
    }

    // ------------------------------------------ region checker (s20) ----

    /// The iso parent of a region, unification-aware (`region_parent`
    /// keys are raw indices; rigid regions never merge, but lookups
    /// resolve through the table to stay honest about `same`).
    fn parent_of(&mut self, r: RegionId) -> Option<RegionId> {
        // Sorted keys (F-0048 class): when several merged regions
        // carry parent edges, the SMALLEST matching index answers —
        // a stable choice, never a hash order's.
        let mut keys: Vec<u32> = self.region_parent.keys().copied().collect();
        keys.sort_unstable();
        for k in keys {
            if self.rt.same(RegionId(k), r) {
                return self.region_parent.get(&k).copied();
            }
        }
        None
    }

    /// Is `anc` an ancestor of `r` in the region forest (owner,
    /// transitively via iso edges — `[mem.region.edge.iso]`)?
    fn is_ancestor(&mut self, anc: RegionId, r: RegionId) -> bool {
        let mut cur = r;
        for _ in 0..64 {
            let Some(p) = self.parent_of(cur) else {
                return false;
            };
            if self.rt.same(p, anc) {
                return true;
            }
            cur = p;
        }
        false
    }

    /// `[mem.region.multiopen]`: the open set must be an antichain in
    /// the region forest. Opening a region while an ancestor or
    /// descendant is open is E1011, naming both open sites.
    /// Re-opening an already-open region is idempotent
    /// (`[mem.region.open.1]`); sibling co-opens are always legal.
    fn check_open_antichain(&mut self, rid: RegionId, span: Span) {
        let opens: Vec<(RegionId, Span)> = self.open_stack.clone();
        for (o, osite) in opens {
            if self.rt.same(o, rid) {
                continue; // depth-counted re-open
            }
            let other_is_ancestor = self.is_ancestor(o, rid);
            let other_is_descendant = self.is_ancestor(rid, o);
            if !other_is_ancestor && !other_is_descendant {
                continue; // siblings: disjoint footprints, co-open away
            }
            self.conflicted = true;
            let this = self.show_region(rid);
            let other = self.show_region(o);
            let relation = if other_is_ancestor {
                format!("{other} owns {this}, so its open window already reaches this data")
            } else {
                format!(
                    "{this} owns {other}, so this open window would reach data that is already open"
                )
            };
            self.diags.push(
                Diagnostic::error(
                    codes::E1011,
                    span,
                    format!("this would open {this} while {other} is still open"),
                )
                .with_label("the second open window starts here")
                .with_secondary(osite, format!("{other} is opened here, and is still open"))
                .with_note(format!(
                    "{relation}. Two regions may be open at once only when neither owns \
                     the other (they are siblings in the region forest) — close the \
                     first block before opening this one, or open the child through \
                     its own scope after the owner's window ends."
                )),
            );
            return;
        }
    }

    /// `[mem.region.freeze.3]` (E1005): a region moves or freezes as a
    /// *closed* subtree only. Fires when the region itself — or an
    /// open region it owns transitively — is in the open set. Returns
    /// whether it reported.
    fn check_transfer_closed(&mut self, rid: RegionId, span: Span) -> bool {
        let opens: Vec<(RegionId, Span)> = self.open_stack.clone();
        for (o, osite) in opens {
            let pinned = self.rt.same(o, rid) || self.is_ancestor(rid, o);
            if !pinned {
                continue;
            }
            self.conflicted = true;
            self.tainted_region[rid.0 as usize] = true;
            let region = self.show_region(rid);
            let (which, why) = if self.rt.same(o, rid) {
                (
                    format!("{region} is open here"),
                    format!("{region} is open, so its handle cannot move or freeze"),
                )
            } else {
                let child = self.show_region(o);
                (
                    format!("{child} — owned by {region} — is open here"),
                    format!(
                        "{region} contains an open child region, so it cannot move \
                         or freeze"
                    ),
                )
            };
            self.diags.push(
                Diagnostic::error(codes::E1005, span, why)
                    .with_label("the transfer happens inside the open window")
                    .with_secondary(osite, format!("{which}, until its block ends"))
                    .with_note(
                        "a region transfers or freezes as a closed subtree only — the \
                         open window pins its handle. End the `region`/`in` block \
                         first, or move the value before opening the region.",
                    ),
            );
            return true;
        }
        false
    }

    /// Mark a region — and everything it owns — frozen: its data is
    /// `imm` forever (`[mem.region.freeze.1]`), never freed (frozen
    /// is its end state, not a leak), referencable from anywhere
    /// (`[mem.region.edge.imm]`), and rejects writes (E1012).
    fn freeze_region(&mut self, rid: RegionId, span: Span) {
        self.frozen_region.entry(rid.0).or_insert(span);
        self.moved_region[rid.0 as usize] = true;
        let children: Vec<u32> = (0..self.rt.regions.len() as u32)
            .filter(|&i| i != rid.0 && self.is_ancestor(rid, RegionId(i)))
            .collect();
        for c in children {
            self.frozen_region.entry(c).or_insert(span);
            self.moved_region[c as usize] = true;
        }
        for i in 0..self.sites.len() {
            let sr = self.sites[i].region;
            if self.rt.same(sr, rid) || self.is_ancestor(rid, sr) {
                self.frozen_site.entry(SiteId(i as u32)).or_insert(span);
            }
        }
    }

    /// `Some(freeze site)` when a place's value is (or contains)
    /// frozen data, or the place is a frozen region value itself.
    fn frozen_at(&mut self, place: PlaceId) -> Option<Span> {
        for (s, _) in self.sites_of_place(place) {
            if let Some(f) = self.frozen_site.get(&s) {
                return Some(*f);
            }
        }
        if let Base::Local(l) = self.places.get(place).base
            && let Some(Some(rid)) = self.region_local.get(&l)
            && let Some(f) = self.frozen_region.get(&rid.0)
        {
            return Some(*f);
        }
        None
    }

    /// Lending a region value `mut` while its window is open is the
    /// same pin as moving it (`[mem.region.freeze.3]`'s spirit: the
    /// callee could move or freeze the handle we are standing
    /// inside) — E1005 through the shared reporter.
    fn check_region_lend(&mut self, place: PlaceId, span: Span) {
        if self.places.get(place).proj.is_empty()
            && let Base::Local(l) = self.places.get(place).base
            && let Some(Some(rid)) = self.region_local.get(&l).copied()
        {
            self.check_transfer_closed(rid, span);
        }
    }

    /// A write reaching frozen data — E1012 (`[mem.region.freeze.1]`:
    /// deep, forever; every path is checked, because immutability is
    /// a fact about the data, not about one binding).
    fn check_frozen_write(&mut self, place: PlaceId, span: Span, verb: &str) {
        let Some(fspan) = self.frozen_at(place) else {
            return;
        };
        self.conflicted = true;
        let shown = self.show_place_now(place);
        self.diags.push(
            Diagnostic::error(
                codes::E1012,
                span,
                format!("`{shown}` is frozen, so it cannot be {verb}"),
            )
            .with_label("this needs the data to be mutable")
            .with_secondary(
                fspan,
                "the freeze happens here — the promotion to `imm` is deep and permanent",
            )
            .with_note(
                "`freeze` promotes the whole graph to `imm`: shareable from anywhere, \
                 forever, and never writable again. Build the value completely before \
                 freezing it, or keep a mutable copy (`copy`) alongside the frozen one.",
            ),
        );
    }

    /// A write reaching a `read`-mode parameter — E1014 (s72, D39:
    /// `[mem.tier0.mode.read]` — immutable for the whole call, and the
    /// immutability is deep, so projections count and so does lending
    /// the binding `mut` onward). The caller-side half of the mode
    /// rules always held; this is the callee's.
    fn check_read_param_write(&mut self, place: PlaceId, span: Span, verb: &str) {
        let Base::Local(l) = self.places.get(place).base else {
            return;
        };
        if self.locals[l as usize].param_mode != Some(None) {
            return;
        }
        let name = self.locals[l as usize].name.clone();
        let decl = self.locals[l as usize].span;
        let shown = self.show_place_now(place);
        self.diags.push(
            Diagnostic::error(
                codes::E1014,
                span,
                format!("`{shown}` is `read` for the whole call, so it cannot be {verb}"),
            )
            .with_label("write through a `read` parameter")
            .with_secondary(
                decl,
                format!("`{name}` is declared without a mode — that spells `read`, immutable for the call"),
            )
            .with_note(
                "a `read` parameter is the caller's value, lent immutably \
                 [mem.tier0.mode.read]. Declare it `mut` if this function's purpose is \
                 to change it (call sites then spell the mutation), `take` it if the \
                 function consumes it, or mutate this function's own `copy`.",
            ),
        );
    }

    /// A mutating use of a place the enclosing `for` iterates — E1013
    /// (s72, D40: `[mem.iter.excl]` — the loop holds a read claim on
    /// its container for the loop's whole extent). Returns whether it
    /// reported, so moves can recover as reads instead of cascading
    /// into the old E1001 reads-as-moves accident (#15).
    fn check_iter_claim(&mut self, place: PlaceId, span: Span, verb: &str) -> bool {
        let hit = self
            .iter_claims
            .iter()
            .rev()
            .find(|&&(claimed, _)| self.places.overlap(claimed, place))
            .copied();
        let Some((claimed, head)) = hit else {
            return false;
        };
        let shown = self.show_place_now(place);
        let container = self.show_place_now(claimed);
        self.diags.push(
            Diagnostic::error(
                codes::E1013,
                span,
                format!("`{shown}` is being iterated, so it cannot be {verb}"),
            )
            .with_label("the container changes under the loop")
            .with_secondary(
                head,
                format!("the `for` loop holds `{container}` from here, for its whole extent"),
            )
            .with_note(
                "iterating holds a read claim on the container for the loop's extent \
                 [mem.iter.excl]. Collect the changes into a second list and apply them \
                 after the loop — or use an index loop (`var i = 0` … `while i < xs.len`), \
                 whose condition re-reads the length every pass.",
            ),
        );
        true
    }

    /// E1002 for a `Copy` read overlapping an EARLIER `mut` argument
    /// of the same call (s72, D39). The non-`Copy` half of the rule is
    /// pairwise over the call surface in [`crate::excl`]; the `Copy`
    /// half lives here because it is order-sensitive — the read is an
    /// instant, not a loan, so only a claim already active when it
    /// evaluates conflicts.
    fn check_copy_read_after_mut(
        &mut self,
        place: PlaceId,
        span: Span,
        arg_muts: &[(PlaceId, Span)],
    ) {
        let Some(&(m, mspan)) = arg_muts
            .iter()
            .find(|&&(m, _)| self.places.overlap(m, place))
        else {
            return;
        };
        let (a, b) = (self.show_place_now(m), self.show_place_now(place));
        let relation = if self.places.covers(m, place) {
            format!(
                "`{b}` is inside `{a}` — a path and its prefix conflict [mem.model.path.disjoint]."
            )
        } else {
            format!("`{a}` and `{b}` can reach the same memory.")
        };
        self.diags.push(
            Diagnostic::error(
                codes::E1002,
                span,
                format!("`{b}` is read for this call after `{a}` goes `mut` in it"),
            )
            .with_label("read of a place being mutated")
            .with_secondary(mspan, format!("`{a}` is passed `mut` here"))
            .with_note(format!(
                "{relation} Read the value into a local before the call, or pass \
                 disjoint fields."
            )),
        );
    }

    // ---------------------------------------------------- places ----

    /// Struct field types of a place type, for sibling interning and
    /// member typing.
    fn fields_of(&self, t: Ty<'t>) -> Option<Vec<(String, Ty<'t>)>> {
        match t.kind() {
            TyKind::Nominal { module, name, args } => {
                match self.sigs.get(*module as usize, name)? {
                    ItemSig::Struct(ss) => {
                        // s94: an applied generic struct's fields answer
                        // with the ARGUMENT type where the declaration
                        // wrote the bare parameter (`v: T` under `Box[
                        // List[int]]` is the List — region content the
                        // walks must see, copyness the moves must see).
                        // A field mentioning a parameter inside a
                        // compound type would need interning this
                        // immutable view cannot do: the whole struct
                        // answers `None`, and the member walk's own
                        // refusal names the place. A bare generic use
                        // (arity mismatch) also answers `None`.
                        if ss.generic && args.len() != ss.generics.len() {
                            return None;
                        }
                        let mut out = Vec::with_capacity(ss.fields.len());
                        for f in &ss.fields {
                            let fty = match self.sigs.table.kind(f.ty) {
                                TyKind::Rigid(r) => {
                                    let i = ss.generics.iter().position(|g| g == r)?;
                                    Ty {
                                        table: t.table,
                                        id: args[i],
                                    }
                                }
                                _ if mentions_rigid(&self.sigs.table, f.ty) => return None,
                                _ => Ty {
                                    table: &self.sigs.table,
                                    id: f.ty,
                                },
                            };
                            out.push((f.name.clone(), fty));
                        }
                        Some(out)
                    }
                    _ => None,
                }
            }
            TyKind::Tuple(elems) => Some(
                elems
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        (
                            i.to_string(),
                            Ty {
                                table: t.table,
                                id: *id,
                            },
                        )
                    })
                    .collect(),
            ),
            _ => None,
        }
    }

    /// Resolve an expression to a place, if it names one. Interns the
    /// place *and its field siblings at every projection step*, so the
    /// move analysis can expand partial re-initializations over the
    /// interned universe.
    fn as_place(&mut self, e: &'t GreenNode) -> Option<(PlaceId, Option<Ty<'t>>)> {
        match e.kind {
            SyntaxKind::ParenExpr => {
                let inner = ParenExpr::cast(e)?.expr()?;
                self.as_place(inner)
            }
            SyntaxKind::PathExpr => {
                let t = PathExpr::cast(e)?.ident()?;
                let name = self.text(t.span);
                if let Some(local) = self.lookup(&name) {
                    let ty = self.tys[local.0 as usize];
                    let place = Place {
                        base: Base::Local(local.0),
                        proj: Vec::new(),
                    };
                    let copy = ty.map(|t| is_copy(t, 0)).unwrap_or(false);
                    return Some((self.places.intern(place, copy), ty));
                }
                // A module-level `let`/`var`/`const` item: a place for
                // mode agreement and same-call exclusivity; moves are
                // not tracked on module state (later campaign).
                if let Some(ItemSig::Global(g)) = self.sigs.get(self.module, &name) {
                    let ty = g.ty.map(|id| Ty {
                        table: &self.sigs.table,
                        id,
                    });
                    let place = Place {
                        base: Base::Global(self.module as u32, name),
                        proj: Vec::new(),
                    };
                    let copy = ty.map(|t| is_copy(t, 0)).unwrap_or(false);
                    return Some((self.places.intern(place, copy), ty));
                }
                None
            }
            SyntaxKind::MemberExpr => {
                let m = MemberExpr::cast(e)?;
                let base = m.base()?;
                let member = m.member()?;
                let (base_id, base_ty) = self.as_place(base)?;
                let fname = self.text(member.span);
                let base_place = self.places.get(base_id).clone();
                // Intern every sibling field at this step (typed), so
                // whole-value moves can expand into per-field residue.
                let mut field_ty = None;
                if let Some(bt) = base_ty
                    && let Some(fields) = self.fields_of(bt)
                {
                    for (name, fty) in &fields {
                        let mut proj = base_place.proj.clone();
                        proj.push(Proj::Field(name.clone()));
                        let sibling = Place {
                            base: base_place.base.clone(),
                            proj,
                        };
                        self.places.intern(sibling, is_copy(*fty, 0));
                        if *name == fname {
                            field_ty = Some(*fty);
                        }
                    }
                }
                let mut proj = base_place.proj;
                proj.push(Proj::Field(fname));
                let place = Place {
                    base: base_place.base,
                    proj,
                };
                // Builtin members (`xs.len`) are not in `fields_of`;
                // the checker's recorded type answers for them, so a
                // copy read never masquerades as a move (#6).
                let field_ty = field_ty.or_else(|| self.expr_ty(e.span));
                let copy = field_ty.map(|t| is_copy(t, 0)).unwrap_or(false);
                Some((self.places.intern(place, copy), field_ty))
            }
            // s21 — container element access: `pool[h]` / `xs[i]`.
            // The container is the place base ([mem.shared.handle.3]);
            // every element is one `Opaque` place. Index operands
            // evaluate here (left-to-right law) and the access emits
            // its check: a generational `handle-check` on pools (trap
            // `stale-handle`, X5) or a bounds `checked-op` on lists
            // (trap `bounds`, D25). A refusal inside the index returns
            // `None`; the caller's value fallback re-raises it.
            SyntaxKind::BracketApply => {
                let b = wolf_ast::BracketApply::cast(e)?;
                let recv = b.callee()?;
                let (base_id, base_ty) = self.as_place(recv)?;
                let container = base_ty.map(|t| t.kind().clone());
                if !matches!(container, Some(TyKind::Pool(_) | TyKind::List(_))) {
                    return None; // not a container place (s17 surface)
                }
                for a in b.args().into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = wolf_ast::Arg::value(a)
                        && wolf_ast::is_expr_kind(v.kind)
                        && self.eval_value(v).is_err()
                    {
                        return None;
                    }
                }
                let base_place = self.places.get(base_id).clone();
                let mut proj = base_place.proj;
                proj.push(Proj::Opaque);
                let place = Place {
                    base: base_place.base,
                    proj,
                };
                let elem_ty = self.expr_ty(e.span);
                let copy = elem_ty.map(|t| is_copy(t, 0)).unwrap_or(false);
                let pid = self.places.intern(place, copy);
                match container {
                    Some(TyKind::Pool(_)) => {
                        self.push(Stmt::HandleCheck {
                            place: pid,
                            span: e.span,
                        });
                        self.blocks[self.cur.0 as usize].trap = true;
                    }
                    _ => {
                        self.push(Stmt::CheckedOp { span: e.span });
                        self.blocks[self.cur.0 as usize].trap = true;
                    }
                }
                Some((pid, elem_ty))
            }
            _ => None,
        }
    }

    /// The callee-side view-set footprint (`[mem.tier0.excl.3]`,
    /// E1008): inside `fn m(mut self.{x, y})`, every touch of `self`
    /// must stay inside the view.
    fn check_view(&mut self, place: PlaceId, span: Span) {
        let Some((self_local, fields, decl)) = self.view.clone() else {
            return;
        };
        let p = self.places.get(place);
        if p.base != Base::Local(self_local.0) {
            return;
        }
        let outside = match p.proj.first() {
            None => true, // `self` whole
            Some(Proj::Field(f)) => !fields.contains(f),
            Some(Proj::Opaque) => true,
        };
        if outside {
            let shown = self.show_place_now(place);
            let view = fields.join(", ");
            self.diags.push(
                Diagnostic::error(
                    codes::E1008,
                    span,
                    format!(
                        "this method declares a view of `self.{{{view}}}`, but touches `{shown}`"
                    ),
                )
                .with_label("outside the declared view")
                .with_secondary(decl, "the view set is declared here")
                .with_note(
                    "callers rely on the view: they may use the other fields while this \
                     method runs. Add the field to the view set, or take plain `mut self`.",
                ),
            );
        }
    }

    fn show_place_now(&self, id: PlaceId) -> String {
        let place = self.places.get(id);
        let mut out = match &place.base {
            Base::Local(l) => self.locals[*l as usize].name.clone(),
            Base::Global(_, name) => name.clone(),
        };
        for step in &place.proj {
            match step {
                Proj::Field(f) => {
                    out.push('.');
                    out.push_str(f);
                }
                Proj::Opaque => out.push_str("[_]"),
            }
        }
        out
    }

    // ------------------------------------------------- place uses ----

    fn emit_read(&mut self, place: PlaceId, span: Span) {
        self.check_view(place, span);
        self.push(Stmt::Read { place, span });
    }

    fn emit_move(&mut self, place: PlaceId, span: Span) {
        self.check_view(place, span);
        // Module-level state is never move-tracked (s18): a global
        // read in value position is a read.
        if matches!(self.places.get(place).base, Base::Global(..)) {
            self.push(Stmt::Read { place, span });
            return;
        }
        // Moving the iterated container out from under its loop is
        // E1013; recover as a read so the one true error is not
        // followed by an E1001 echo on the loop's back edge.
        if self.check_iter_claim(place, span, "moved away") {
            self.push(Stmt::Read { place, span });
            return;
        }
        // Moving a region value whole: affine transfer — its free is
        // no longer this frame's ([mem.region.freeze.2]). Moving it
        // while its window is open is E1005 (s20,
        // [mem.region.freeze.3]): the open pins the handle.
        if self.places.get(place).proj.is_empty()
            && let Base::Local(l) = self.places.get(place).base
            && let Some(Some(rid)) = self.region_local.get(&l).copied()
        {
            self.check_transfer_closed(rid, span);
            self.moved_region[rid.0 as usize] = true;
        }
        self.push(Stmt::Move { place, span });
    }

    fn emit_init(&mut self, place: PlaceId, span: Span) {
        self.check_view(place, span);
        // A whole-place (re)binding replaces the value; a projected
        // init writes *into* the value — frozen data rejects the
        // latter (s20, [mem.region.freeze.1]). A `read` parameter
        // rejects both (s72, D39: the binding holds the caller's
        // value for the whole call).
        let projected = !self.places.get(place).proj.is_empty();
        if projected {
            self.check_frozen_write(place, span, "assigned through");
        }
        self.check_read_param_write(
            place,
            span,
            if projected {
                "assigned through"
            } else {
                "assigned"
            },
        );
        self.check_iter_claim(place, span, "assigned");
        self.push(Stmt::Init { place, span });
    }

    fn emit_mutate(&mut self, place: PlaceId, span: Span) {
        self.check_view(place, span);
        self.check_frozen_write(place, span, "modified");
        self.check_read_param_write(place, span, "modified");
        self.check_iter_claim(place, span, "modified");
        self.push(Stmt::Mutate { place, span });
    }

    /// Use a place in value position: `Copy` copies, otherwise the
    /// value moves out (`[mem.tier0.move.1]`).
    fn use_value(&mut self, place: PlaceId, span: Span) {
        if self.places.is_copy(place) {
            self.emit_read(place, span);
        } else {
            self.emit_move(place, span);
        }
    }

    // ------------------------------------------- the unsafe tier (s22) ----

    fn in_unsafe(&self) -> bool {
        self.unsafe_depth > 0
    }

    /// E1301 — the missing-ring error. States the ring, never
    /// moralizes: the raw tier is simpler, not scarier (D11).
    fn require_unsafe(&mut self, what: &str, span: Span) {
        if self.in_unsafe() {
            return;
        }
        self.diags.push(
            Diagnostic::error(
                codes::E1301,
                span,
                format!("{what} needs an `unsafe` block"),
            )
            .with_label("raw-tier operation in safe code")
            .with_note(
                "raw pointers are inert data anywhere — only the tier's operations \
                 need the ring. Wrap this in `unsafe { }` and state the invariant in \
                 a `# Safety:` comment; the rules inside are simpler than the safe \
                 tier's, not stricter.",
            ),
        );
    }

    /// W1301 — the `# Safety:` style lint ([mem.boundary.doc],
    /// advisory): an `unsafe` block should state the invariant it
    /// maintains, either in the contiguous comment lines immediately
    /// above it or inside the block itself.
    fn lint_safety_comment(&mut self, span: Span) {
        let block = String::from_utf8_lossy(&self.src[span.lo as usize..span.hi as usize]);
        if block.contains("# Safety") {
            return;
        }
        // Contiguous `//` lines immediately above the block (the
        // partial line holding `unsafe` itself is cut first).
        let head = String::from_utf8_lossy(&self.src[..span.lo as usize]);
        let head = &head[..head.rfind('\n').unwrap_or(0)];
        for line in head.lines().rev() {
            let t = line.trim();
            if t.is_empty() || !t.starts_with("//") {
                break;
            }
            if t.contains("# Safety") {
                return;
            }
        }
        let kw = Span::new(span.file, span.lo, (span.lo + 6).min(span.hi));
        self.diags.push(
            Diagnostic::warning(
                codes::W1301,
                kw,
                "this `unsafe` block does not state its invariant".to_string(),
            )
            .with_label("add a `# Safety:` comment")
            .with_note(
                "the block discharges a proof obligation the checker cannot; write \
                 the invariant down for the auditors who will read this ring \
                 (`// # Safety: …` above the block).",
            ),
        );
    }

    /// `assume noalias p, q` ([mem.unsafe.raw.2]): operands must be
    /// raw pointers (E1304) inside the ring (E1301); the fact licenses
    /// O5 and its violation is P5 — checked dynamically by s23/is04,
    /// never approximated here.
    fn lower_assume(&mut self, stmt: &'t GreenNode) -> R<()> {
        self.require_unsafe("`assume noalias`", stmt.span);
        let d = wolf_ast::AssumeStmt::cast(stmt).expect("kind");
        let mut ops: Vec<String> = Vec::new();
        for op in d.exprs() {
            if let Some((place, _)) = self.as_place(op) {
                self.emit_read(place, op.span);
            } else {
                self.eval_value(op)?;
            }
            let is_ptr = matches!(
                self.expr_ty(op.span).map(|t| t.kind().clone()),
                Some(TyKind::Ptr(_) | TyKind::Error | TyKind::Never)
            );
            if !is_ptr {
                let shown = self.rendered_expr_ty(op.span);
                self.diags.push(
                    Diagnostic::error(
                        codes::E1304,
                        op.span,
                        format!(
                            "`assume noalias` takes raw pointers, but this operand is `{shown}`"
                        ),
                    )
                    .with_label("not a raw pointer")
                    .with_note(
                        "safe values already carry stronger, checked aliasing facts \
                         (`mut` is exclusive, `read` is frozen) — `assume` exists only \
                         for `*T` values the checker cannot see through.",
                    ),
                );
            }
            ops.push(self.text(op.span));
        }
        self.push(Stmt::Assume {
            ops: ops.join(", "),
            span: stmt.span,
        });
        Ok(())
    }

    /// `borrow r from p` — re-entry door 1 ([mem.unsafe.door]).
    /// Static requirements, checked locally (D11: each op's rule is
    /// short and local): the ring (E1301), a `region` left operand and
    /// a `*T` right operand (E1305). The claim itself — p addresses
    /// r's live allocation, correctly typed — is the P6 obligation,
    /// dynamic by design.
    fn eval_borrow_from(&mut self, e: &'t GreenNode) -> R<Val> {
        self.require_unsafe("the `borrow … from …` door", e.span);
        let d = wolf_ast::BorrowExpr::cast(e).expect("kind");
        let mut door = ("<region>".to_string(), "<ptr>".to_string());
        if let Some(r) = d.borrowed() {
            if let Some((place, _)) = self.as_place(r) {
                self.emit_read(place, r.span);
            } else {
                self.eval_value(r)?;
            }
            let is_region = matches!(
                self.expr_ty(r.span).map(|t| t.kind().clone()),
                Some(TyKind::RegionTy | TyKind::Error | TyKind::Never)
            );
            if !is_region {
                let shown = self.rendered_expr_ty(r.span);
                self.door_misuse(r.span, "a `region` value", &shown);
            }
            door.0 = self.text(r.span);
        }
        if let Some(p) = d.source() {
            if let Some((place, _)) = self.as_place(p) {
                self.emit_read(place, p.span);
            } else {
                self.eval_value(p)?;
            }
            let is_ptr = matches!(
                self.expr_ty(p.span).map(|t| t.kind().clone()),
                Some(TyKind::Ptr(_) | TyKind::Error | TyKind::Never)
            );
            if !is_ptr {
                let shown = self.rendered_expr_ty(p.span);
                self.door_misuse(p.span, "a raw pointer (`*T`)", &shown);
            }
            door.1 = self.text(p.span);
        }
        self.push(Stmt::Door {
            region: door.0,
            ptr: door.1,
            span: e.span,
        });
        Ok(Val::none())
    }

    fn door_misuse(&mut self, span: Span, wanted: &str, got: &str) {
        self.diags.push(
            Diagnostic::error(
                codes::E1305,
                span,
                format!("`borrow … from …` needs {wanted} here, but this is `{got}`"),
            )
            .with_label("the door's claim has nothing to check against")
            .with_note(
                "door 1 asserts \"this pointer addresses that region's live \
                 allocation\" — it needs the region on the left and the raw pointer \
                 on the right. The other door is a checked `handle`, which \
                 re-validates its generation at every access.",
            ),
        );
    }

    /// A raw-pointer element access `p[i]` ([mem.unsafe.raw.1]):
    /// C-shaped, no bounds, no validity — statically only the ring is
    /// required; the access is recorded for s23/is04 attribution
    /// (P1/P3/P4/L1/L2, writes additionally P2/T2).
    fn raw_index(&mut self, e: &'t GreenNode, write: bool) -> R<()> {
        let verb = if write {
            "a raw pointer write"
        } else {
            "a raw pointer read"
        };
        self.require_unsafe(verb, e.span);
        let b = wolf_ast::BracketApply::cast(e).expect("kind");
        let mut ptr = "<ptr>".to_string();
        if let Some(recv) = b.callee() {
            if let Some((place, _)) = self.as_place(recv) {
                self.emit_read(place, recv.span);
            } else {
                self.eval_value(recv)?;
            }
            ptr = self.text(recv.span);
        }
        for a in b.args().into_iter().flat_map(|l| l.args()) {
            if let Some(v) = wolf_ast::Arg::value(a)
                && wolf_ast::is_expr_kind(v.kind)
            {
                if let Some((place, _)) = self.as_place(v) {
                    self.emit_read(place, v.span);
                } else {
                    self.eval_value(v)?;
                }
            }
        }
        if write {
            self.push(Stmt::RawWrite { ptr, span: e.span });
        } else {
            self.push(Stmt::RawRead { ptr, span: e.span });
        }
        Ok(())
    }

    /// Is `e` a `p[i]` access through a raw pointer? (The bracket
    /// place machinery owns containers; the raw tier owns this.)
    fn is_raw_index(&self, e: &GreenNode) -> bool {
        if e.kind != SyntaxKind::BracketApply {
            return false;
        }
        wolf_ast::BracketApply::cast(e)
            .and_then(|b| b.callee())
            .and_then(|recv| self.expr_ty(recv.span))
            .is_some_and(|t| matches!(t.kind(), TyKind::Ptr(_)))
    }

    // ---------------------------------------------------- defers ----

    /// Lower pending defers for scopes `[depth..]`, innermost first,
    /// each scope's defers in reverse declaration order (LIFO).
    /// `error_path` includes `errdefer`s.
    fn emit_defers(&mut self, depth: usize, error_path: bool) -> R<()> {
        if self.in_defer {
            return Ok(());
        }
        self.in_defer = true;
        let mut pending: Vec<Cleanup<'t>> = Vec::new();
        for scope in self.scopes[depth..].iter().rev() {
            for c in scope.cleanup.iter().rev() {
                if let Cleanup::Defer(_, is_err) = c
                    && *is_err
                    && !error_path
                {
                    continue;
                }
                pending.push(*c);
            }
        }
        let mut result = Ok(());
        for c in pending {
            match c {
                Cleanup::Defer(expr, _) => {
                    if let Err(nyc) = self.eval_value(expr) {
                        result = Err(nyc);
                        break;
                    }
                }
                // s21: the recorded RC drop, LIFO with the defers
                // ([mem.shared.drop.1]); drop-if-live semantics, so a
                // moved-away local's drop never re-fires. The
                // declaration span is the anchor.
                Cleanup::DropLocal(l) => {
                    let span = self.locals[l.0 as usize].span;
                    let place = self.places.intern(
                        Place {
                            base: Base::Local(l.0),
                            proj: Vec::new(),
                        },
                        false,
                    );
                    self.push(Stmt::Drop { place, span });
                }
            }
        }
        self.in_defer = false;
        result
    }

    // ------------------------------------------------ expressions ----

    /// Evaluate an expression for its value: a place moves (or
    /// copies); everything else recurses structurally in evaluation
    /// order (`[mem.model.order]`). Returns the value's region
    /// provenance (s19).
    fn eval_value(&mut self, e: &'t GreenNode) -> R<Val> {
        match e.kind {
            SyntaxKind::LiteralExpr => Ok(Val::none()),
            SyntaxKind::PathExpr | SyntaxKind::MemberExpr | SyntaxKind::BracketApply => {
                // s22: `p[i]` through a raw pointer is a raw-tier
                // access, not a container place.
                if self.is_raw_index(e) {
                    self.raw_index(e, false)?;
                    return Ok(Val::none());
                }
                if let Some((place, _)) = self.as_place(e) {
                    // A `Copy` use duplicates a region-free scalar:
                    // no site flows (this is what keeps a Copy field
                    // read from pinning its parent's region).
                    let val = if self.places.is_copy(place) {
                        Val::none()
                    } else {
                        self.val_of_place(place, e.span)
                    };
                    self.use_value(place, e.span);
                    return Ok(val);
                }
                // A field of a temporary: evaluate the base; the
                // projection itself has no place effects.
                if e.kind == SyntaxKind::MemberExpr
                    && let Some(base) = MemberExpr::cast(e).and_then(|m| m.base())
                {
                    return self.eval_value(base);
                }
                // s21: a bracket access that did not resolve to a
                // container place (temporary receiver, or a refusal
                // inside the index that `as_place` could not carry) —
                // re-evaluate the constituents so the refusal
                // surfaces; a temporary container's element read has
                // no further place effects.
                if e.kind == SyntaxKind::BracketApply
                    && let Some(b) = wolf_ast::BracketApply::cast(e)
                {
                    let mut val = Val::none();
                    if let Some(r) = b.callee() {
                        val.merge(self.eval_value(r)?);
                    }
                    for a in b.args().into_iter().flat_map(|l| l.args()) {
                        if let Some(v) = wolf_ast::Arg::value(a)
                            && wolf_ast::is_expr_kind(v.kind)
                        {
                            val.merge(self.eval_value(v)?);
                        }
                    }
                    return Ok(val);
                }
                Ok(Val::none()) // item reference (fn value, enum type, …)
            }
            SyntaxKind::StringExpr => {
                let d = StringExpr::cast(e).expect("kind");
                for i in d.interps() {
                    if let Some(hole) = i.expr() {
                        // Interpolation formats the value: a read.
                        if let Some((place, _)) = self.as_place(hole) {
                            self.emit_read(place, hole.span);
                        } else {
                            self.eval_value(hole)?;
                        }
                    }
                }
                Ok(Val::none())
            }
            SyntaxKind::ParenExpr => match ParenExpr::cast(e).and_then(|p| p.expr()) {
                Some(inner) => self.eval_value(inner),
                None => Ok(Val::none()),
            },
            SyntaxKind::TupleExpr => {
                // Tuples aggregate by value; they are not allocation
                // sites of their own (`[mem.model.alloc]` names
                // struct literals, collection ctors, closures).
                let mut val = Val::none();
                for elem in TupleExpr::cast(e).expect("kind").elems() {
                    val.merge(self.eval_value(elem)?);
                }
                Ok(val)
            }
            SyntaxKind::Block => {
                let b = AstBlock::cast(e).expect("kind");
                self.walk_block(b, true)
            }
            SyntaxKind::PrefixExpr => self.eval_prefix(e),
            SyntaxKind::BinExpr => self.eval_bin(e),
            SyntaxKind::CastExpr => {
                let inner = CastExpr::cast(e).and_then(|c| c.expr());
                // s98 (D47) — a dyn cast: the pair's data half
                // borrows the operand PLACE. Deriving the pair READS
                // the place (never moves it — same Tree Borrows rule
                // as the raw bridge below); the borrow is claimed
                // where the pair lands. Provenance flows: the pair
                // holds whatever the place held, so the region story
                // (escape, free-before-use) rides the existing site
                // machinery unchanged.
                if let Some((_src_t, _tgt_t, CastKind::Unsize)) = self.casts.get(&e.span).copied() {
                    let Some(x) = inner else {
                        return Ok(Val::none());
                    };
                    let Some((place, _)) = self.as_place(x) else {
                        // Sema admits only place-shaped operands
                        // (E0810); a shape its syntax passes and this
                        // checker cannot name is the conservative
                        // reading's residue, recorded for D47.
                        return Err(NotYet {
                            construct: "a dyn cast of a place this checker cannot name (D47's conservative reading)",
                            span: e.span,
                        });
                    };
                    self.emit_read(place, e.span);
                    let mut val = self.val_of_place(place, e.span);
                    val.borrowed = vec![place];
                    self.unclaimed_pairs.push((e.span, false));
                    return Ok(val);
                }
                // s22 — a raw pointer bridge ([mem.prov.expose]):
                // ring-gated; the operand is *read*, never moved —
                // deriving a pointer is not a use (Tree Borrows), and
                // `r as *u8` must leave the region value live.
                if let Some((src_t, tgt_t, CastKind::Raw)) = self.casts.get(&e.span).copied() {
                    self.require_unsafe("a pointer cast", e.span);
                    if let Some(x) = inner {
                        if let Some((place, _)) = self.as_place(x) {
                            self.emit_read(place, x.span);
                        } else {
                            self.eval_value(x)?;
                        }
                        let kind_of = |id: TyId| self.tb.table.kind(id).clone();
                        let dir = match (kind_of(src_t), kind_of(tgt_t)) {
                            (TyKind::Ptr(_), TyKind::Ptr(_)) => "ptr->ptr",
                            (TyKind::Ptr(_), _) => "ptr->int",
                            (TyKind::RegionTy, _) => "region->ptr",
                            _ => "int->ptr",
                        };
                        let what = self.text(x.span);
                        self.push(Stmt::Expose {
                            what,
                            dir,
                            span: e.span,
                        });
                    }
                    return Ok(Val::none());
                }
                match inner {
                    Some(inner) => self.eval_value(inner),
                    None => Ok(Val::none()),
                }
            }
            SyntaxKind::RangeExpr => {
                // s37: `^n` endpoints (D25 end-relative slicing) read
                // their inner offset expression; the marker itself has
                // no place effects.
                for end in RangeExpr::cast(e).expect("kind").endpoints() {
                    if end.kind == SyntaxKind::FromEndExpr {
                        if let Some(inner) = wolf_ast::FromEndExpr::cast(end).and_then(|f| f.expr())
                        {
                            self.eval_value(inner)?;
                        }
                    } else {
                        self.eval_value(end)?;
                    }
                }
                Ok(Val::none())
            }
            SyntaxKind::TryExpr => {
                let d = wolf_ast::TryExpr::cast(e).expect("kind");
                let val = match d.expr() {
                    Some(inner) => self.eval_value(inner)?,
                    None => Val::none(),
                };
                // `?` is a branch (D30): the error edge leaves the
                // function, running errdefers + defers (LIFO).
                let err = self.new_block();
                let ok = self.new_block();
                self.goto(self.cur, ok);
                self.goto(self.cur, err);
                let resume = self.cur;
                self.cur = err;
                self.emit_defers(0, true)?;
                let exit = self.exit;
                self.goto(self.cur, exit);
                self.cur = resume;
                self.cur = ok;
                Ok(val)
            }
            SyntaxKind::CallExpr => self.eval_call(e),
            SyntaxKind::StructLit => {
                let d = StructLit::cast(e).expect("kind");
                let mut parts: Vec<(Option<String>, Val, Span)> = Vec::new();
                for f in d.fields() {
                    if let Some(v) = FieldInit::value(f) {
                        // Field initializers consume their values.
                        let fv = self.eval_value(v)?;
                        let fname = f.name().map(|t| self.text(t.span));
                        parts.push((fname, fv, v.span));
                    }
                }
                // The construction is an allocation site in the
                // ambient region ([mem.region.create.3]); embedded
                // values must be co-located.
                let ty = self.rendered_expr_ty(e.span);
                let site = self.alloc_site(ty, SiteKind::Lit, e.span);
                let region = self.sites[site.0 as usize].region;
                let mut val = Val::site(site, e.span);
                for (fname, fv, sp) in parts {
                    if let Some(r) = fv.region {
                        // A region value moved into a field: the iso
                        // edge ([mem.region.edge.iso]) — the aggregate
                        // becomes the owner (the parent edge in the
                        // region forest), the free is no longer this
                        // frame's, and the field path keeps the
                        // identity so `in h.child { }` resolves.
                        self.moved_region[r.0 as usize] = true;
                        self.region_parent.insert(r.0, region);
                        if let Some(f) = fname {
                            val.region_fields.push((f, r));
                        }
                    }
                    self.demand_store(&fv, region, Some(e.span), sp);
                    let fields = std::mem::take(&mut val.region_fields);
                    val.merge(fv);
                    val.region_fields = fields;
                }
                val.region = None;
                Ok(val)
            }
            SyntaxKind::IfExpr => self.eval_if(e),
            SyntaxKind::MatchExpr => self.eval_match(e),
            SyntaxKind::WhileExpr => self.eval_while(e),
            SyntaxKind::ForExpr => self.eval_for(e),
            SyntaxKind::LoopExpr => self.eval_loop(e),
            SyntaxKind::ElseExpr => self.eval_else(e),
            SyntaxKind::ReturnExpr => {
                let d = ReturnExpr::cast(e).expect("kind");
                if let Some(v) = d.value() {
                    let val = self.eval_value(v)?;
                    self.demand_outlives_frame(&val, v.span);
                }
                self.emit_defers(0, false)?;
                let exit = self.exit;
                self.goto(self.cur, exit);
                let dead = self.new_block();
                self.cur = dead;
                Ok(Val::none())
            }
            SyntaxKind::BreakExpr => {
                let Some(frame) = self.loops.last() else {
                    return Ok(Val::none()); // parse-adjacent wreckage
                };
                let (target, depth) = (frame.break_to, frame.scope_depth);
                self.emit_defers(depth, false)?;
                self.goto(self.cur, target);
                let dead = self.new_block();
                self.cur = dead;
                Ok(Val::none())
            }
            SyntaxKind::ContinueExpr => {
                let Some(frame) = self.loops.last() else {
                    return Ok(Val::none());
                };
                let (target, depth) = (frame.continue_to, frame.scope_depth);
                self.emit_defers(depth, false)?;
                self.goto(self.cur, target);
                let dead = self.new_block();
                self.cur = dead;
                Ok(Val::none())
            }
            SyntaxKind::ClosureExpr => self.eval_closure(e),
            SyntaxKind::FromEndExpr => Err(NotYet {
                construct: "end-relative `^n` slicing places (the std surface)",
                span: e.span,
            }),
            SyntaxKind::RegionBlock => self.eval_region_block(e),
            SyntaxKind::RegionValue => self.eval_region_value(e),
            SyntaxKind::InBlock => self.eval_in_block(e),
            SyntaxKind::FreezeExpr => self.eval_freeze(e),
            // s22 — ring 1: the block's value crosses back into the
            // safe tier, where every safe-tier invariant re-applies
            // (the re-entry contract: what comes out is an ordinary,
            // fully-typed value — doors and handle checks vouched for
            // it inside).
            SyntaxKind::UnsafeBlock => {
                let d = wolf_ast::UnsafeBlock::cast(e).expect("kind");
                self.lint_safety_comment(e.span);
                let kw = Span::new(e.span.file, e.span.lo, (e.span.lo + 6).min(e.span.hi));
                self.push(Stmt::UnsafeEnter { span: kw });
                self.unsafe_depth += 1;
                let val = match d.body() {
                    Some(b) => self.walk_block(b, true),
                    None => Ok(Val::none()),
                };
                self.unsafe_depth -= 1;
                let val = val?;
                self.push(Stmt::UnsafeExit {
                    span: end_span(e.span),
                });
                Ok(val)
            }
            // Inline C and asm have no pinned semantics (c10: s48/s50)
            // — only the rule that they are unsafe-tier.
            SyntaxKind::InlineC | SyntaxKind::AsmExpr => Err(NotYet {
                construct: "inline C / asm (unsafe tier)",
                span: e.span,
            }),
            // s73 — the conc surface. The single-threaded check CFG
            // models each construct conservatively: task closures run
            // "once, here" (captures are by-copy reads per S-10;
            // E1101 already rejected cross-task mutation), select
            // arms are alternative branches, `when` bodies run under
            // payload rebinds. Interleaving soundness is the type
            // system's (E1101/E1102/E1103) + the runtime's; this
            // tier checks moves/exclusivity/loans per task body.
            SyntaxKind::ScopeExpr => self.eval_conc_scope(e),
            SyntaxKind::SelectExpr => self.eval_select(e),
            SyntaxKind::WhenExpr => self.eval_when(e),
            SyntaxKind::SpawnExpr => self.eval_spawn(e),
            SyntaxKind::BorrowExpr => self.eval_borrow_from(e),
            _ => Err(NotYet {
                construct: "this expression shape (memory tier)",
                span: e.span,
            }),
        }
    }

    // ---------------------------------------------- region forms ----

    /// `region(strategy?)` — a first-class region value: created, not
    /// opened; the ambient is untouched until `in` (X4).
    fn eval_region_value(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = RegionValue::cast(e).expect("kind");
        let strategy = self.parse_strategy(d.strategy());
        let rid = self.new_region("<region>", RegionKind::Value, strategy, e.span);
        Ok(Val {
            sites: Vec::new(),
            region: Some(rid),
            origin: Some(e.span),
            region_fields: Vec::new(),
            borrowed: Vec::new(),
        })
    }

    /// `region name? (: strategy)? { … }` — create + open + scope +
    /// free (`[mem.region.create.1]`): the block body runs with the
    /// region ambient; the exit frees it wholesale (unless the affine
    /// value was moved away — then the free is no longer ours, and
    /// s20 owns the rest of that story).
    fn eval_region_block(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = RegionBlock::cast(e).expect("kind");
        let strategy = self.parse_strategy(d.strategy());
        let name_tok = d.name();
        let name = name_tok
            .map(|t| self.text(t.span))
            .unwrap_or_else(|| "<region>".to_string());
        let intro = name_tok.map(|t| t.span).unwrap_or(e.span);
        let rid = self.new_region(&name, RegionKind::Scope, strategy, intro);
        let outer_mark = self.locals.len();
        self.push(Stmt::RegionOpen {
            region: rid,
            span: intro,
        });
        self.check_open_antichain(rid, intro);
        self.open_stack.push((rid, intro));
        self.ambient.push(rid);
        self.push_scope();
        if let Some(t) = name_tok {
            // The sugar name is an affine region-typed binding,
            // usable by `in name { }` (and, illegally until s20's
            // E1005, by `move`).
            let ty = self.local_tys.get(&t.span).map(|&id| Ty {
                table: &self.tb.table,
                id,
            });
            let local = self.declare(&name, t.span, ty);
            let place = self.places.intern(
                Place {
                    base: Base::Local(local.0),
                    proj: Vec::new(),
                },
                false,
            );
            self.push(Stmt::Init {
                place,
                span: t.span,
            });
            self.region_local.insert(local.0, Some(rid));
        }
        let mut val = match d.body() {
            Some(b) => self.walk_block(b, true)?,
            None => Val::none(),
        };
        let close_span = end_span(e.span);
        self.close_scope(close_span)?;
        self.ambient.pop();
        self.open_stack.pop();
        if !self.moved_region[rid.0 as usize] {
            self.sweep_region_close(rid, outer_mark, close_span, &mut val);
            self.push(Stmt::RegionClose {
                region: rid,
                span: close_span,
            });
        }
        val.region = None;
        Ok(val)
    }

    /// `in r { … }` — rebind the ambient region for the block
    /// (`[mem.region.create.3]`). Re-entering an already-open region
    /// is idempotent — openness is depth-counted
    /// (`[mem.region.open.1]`). Not a free: no escape sweep here.
    /// Since s20, `r` may also be a one-step field path (`in h.child`)
    /// — opening a region through the iso edge that stashes it; the
    /// antichain check (`[mem.region.multiopen]`) decides whether the
    /// open is legal, and a frozen region refuses to open at all.
    fn eval_in_block(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = InBlock::cast(e).expect("kind");
        let rid = match d.region() {
            Some(rexpr) => {
                let known = match self.as_place(rexpr) {
                    Some((place, _)) if self.places.get(place).proj.is_empty() => {
                        self.emit_read(place, rexpr.span);
                        match self.places.get(place).base {
                            Base::Local(l) => self.region_local.get(&l).copied().flatten(),
                            Base::Global(..) => None,
                        }
                    }
                    Some((place, _)) => {
                        self.emit_read(place, rexpr.span);
                        let p = self.places.get(place);
                        match (&p.base, p.proj.as_slice()) {
                            (Base::Local(l), [Proj::Field(f)]) => {
                                self.region_field.get(&(*l, f.clone())).copied()
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                };
                match known {
                    Some(rid) => rid,
                    None => {
                        return Err(NotYet {
                            construct: "region flow beyond bindings and one-step iso fields",
                            span: rexpr.span,
                        });
                    }
                }
            }
            None => self.ambient(),
        };
        // A frozen region never reopens: its window closed forever
        // when `freeze` promoted it ([mem.region.freeze.1]).
        if let Some(&fspan) = self.frozen_region.get(&rid.0) {
            self.conflicted = true;
            let region = self.show_region(rid);
            self.diags.push(
                Diagnostic::error(
                    codes::E1012,
                    e.span,
                    format!("{region} is frozen, so it cannot be opened for mutation"),
                )
                .with_label("`in` opens a region for writing")
                .with_secondary(
                    fspan,
                    "the freeze happens here — the promotion to `imm` is deep and permanent",
                )
                .with_note(
                    "`freeze` promotes the whole graph to `imm`: readable and shareable \
                     from anywhere, forever, and never writable again. Do the mutation \
                     before the freeze, or build a fresh region and `copy` what you need.",
                ),
            );
        }
        self.push(Stmt::RegionOpen {
            region: rid,
            span: e.span,
        });
        self.check_open_antichain(rid, e.span);
        self.open_stack.push((rid, e.span));
        self.ambient.push(rid);
        let val = match d.body() {
            Some(b) => self.walk_block(b, true)?,
            None => Val::none(),
        };
        self.ambient.pop();
        self.open_stack.pop();
        self.push(Stmt::RegionClose {
            region: rid,
            span: end_span(e.span),
        });
        Ok(val)
    }

    /// `freeze r` / `freeze region { … }` — promotion to `imm` (s20,
    /// `[mem.region.freeze.1]`): consumes the affine region value (a
    /// move — E1005 fires at the chokepoint if the window is open,
    /// `[mem.region.freeze.3]`), then relabels the whole owned
    /// subtree frozen: never freed, never written, referencable from
    /// anywhere.
    fn eval_freeze(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = FreezeExpr::cast(e).expect("kind");
        let Some(operand) = d.expr() else {
            return Ok(Val::none());
        };
        if operand.kind == SyntaxKind::RegionBlock {
            return self.eval_frozen_block(operand, e.span);
        }
        let rid = match self.as_place(operand) {
            Some((place, _)) if self.places.get(place).proj.is_empty() => {
                let known = match self.places.get(place).base {
                    Base::Local(l) => self.region_local.get(&l).copied().flatten(),
                    Base::Global(..) => None,
                };
                // The freeze consumes the value (affine): the move
                // statement carries use-after-freeze to E1001, and
                // the open-window check to E1005.
                self.use_value(place, operand.span);
                known
            }
            _ => self.eval_value(operand)?.region,
        };
        let Some(rid) = rid else {
            return Err(NotYet {
                construct: "freezing a region whose identity is not statically known",
                span: operand.span,
            });
        };
        self.freeze_region(rid, e.span);
        Ok(Val {
            sites: Vec::new(),
            region: Some(rid),
            origin: Some(e.span),
            region_fields: Vec::new(),
            borrowed: Vec::new(),
        })
    }

    /// `freeze region { … }` — build-then-freeze: the block runs with
    /// the region ambient, and the closing brace promotes instead of
    /// freeing. Everything the block allocated — including its value
    /// and anything stashed in outer bindings — is `imm` from here on
    /// (`[mem.region.freeze.1]`; frozen is the region's end state,
    /// not a leak). No escape sweep: escaping frozen data is the
    /// point.
    fn eval_frozen_block(&mut self, e: &'t GreenNode, freeze_span: Span) -> R<Val> {
        let d = RegionBlock::cast(e).expect("kind");
        let strategy = self.parse_strategy(d.strategy());
        let name_tok = d.name();
        let name = name_tok
            .map(|t| self.text(t.span))
            .unwrap_or_else(|| "<region>".to_string());
        let intro = name_tok.map(|t| t.span).unwrap_or(e.span);
        let rid = self.new_region(&name, RegionKind::Scope, strategy, intro);
        self.push(Stmt::RegionOpen {
            region: rid,
            span: intro,
        });
        self.check_open_antichain(rid, intro);
        self.open_stack.push((rid, intro));
        self.ambient.push(rid);
        self.push_scope();
        let mut val = match d.body() {
            Some(b) => self.walk_block(b, true)?,
            None => Val::none(),
        };
        let close_span = end_span(e.span);
        self.close_scope(close_span)?;
        self.ambient.pop();
        self.open_stack.pop();
        self.push(Stmt::RegionClose {
            region: rid,
            span: close_span,
        });
        self.freeze_region(rid, freeze_span);
        val.region = None;
        Ok(val)
    }

    /// `: rc` / `: pool(T)` — parsed, carried; arena is the default
    /// (`[mem.region.create.1]`: strategy changes cost, never safety).
    fn parse_strategy(&self, strat: Option<&GreenNode>) -> Strategy {
        let Some(s) = strat else {
            return Strategy::Arena;
        };
        if let Some(ty) = s.nodes().next() {
            return Strategy::Pool(self.text(ty.span));
        }
        let word = s
            .tokens()
            .find(|t| t.kind == SyntaxKind::Ident)
            .map(|t| self.text(t.span));
        match word.as_deref() {
            Some("rc") => Strategy::Rc,
            _ => Strategy::Arena,
        }
    }

    fn eval_prefix(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = PrefixExpr::cast(e).expect("kind");
        let Some(operand) = d.operand() else {
            return Ok(Val::none());
        };
        match d.op().map(|t| t.kind) {
            Some(SyntaxKind::CopyKw) => {
                // `copy x`: an independent value from any type —
                // never a move ([mem.tier0.move.3]). The duplicate is
                // a fresh allocation in the ambient region: `copy`
                // deliberately breaks region linkage (the fix
                // ladder's first rung).
                if let Some((place, ty)) = self.as_place(operand) {
                    self.emit_read(place, operand.span);
                    if !self.places.is_copy(place) {
                        let rendered = ty
                            .map(|t| render(t.table, t.id, &|_| Err("_")))
                            .unwrap_or_else(|| "?".to_string());
                        let site = self.alloc_site(rendered, SiteKind::Lit, e.span);
                        return Ok(Val::site(site, e.span));
                    }
                    Ok(Val::none())
                } else {
                    self.eval_value(operand)
                }
            }
            Some(SyntaxKind::MoveKw) => {
                if let Some((place, _)) = self.as_place(operand) {
                    let val = if self.places.is_copy(place) {
                        Val::none()
                    } else {
                        self.val_of_place(place, operand.span)
                    };
                    self.use_value(place, operand.span);
                    Ok(val)
                } else {
                    self.eval_value(operand)
                }
            }
            Some(SyntaxKind::Amp) => Err(NotYet {
                construct: "first-class borrow expressions (typeable with the region campaign)",
                span: e.span,
            }),
            Some(SyntaxKind::Star) => Err(NotYet {
                construct: "the unsafe tier",
                span: e.span,
            }),
            Some(SyntaxKind::SharedKw) => {
                // `shared v` — the Tier-2 RC cell ([mem.shared.rc.1]).
                // The payload moves into the cell (MVS), and from
                // here on RC owns it: the cell and its payload are
                // exempt from every region demand, exactly like `imm`
                // data — the `[mem.region.edge]` Tier-2 column. The
                // cell itself is an allocation at the creation site
                // ([mem.model.alloc]).
                let operand_val = if let Some((place, _)) = self.as_place(operand) {
                    let v = if self.places.is_copy(place) {
                        Val::none()
                    } else {
                        self.val_of_place(place, operand.span)
                    };
                    self.use_value(place, operand.span);
                    v
                } else {
                    self.eval_value(operand)?
                };
                for &s in &operand_val.sites {
                    self.shared_site.insert(s);
                }
                let ty = self.rendered_expr_ty(e.span);
                let site = self.alloc_site(ty, SiteKind::Lit, e.span);
                self.shared_site.insert(site);
                self.rc_cells.push(site);
                Ok(Val::site(site, e.span))
            }
            _ => self.eval_value(operand),
        }
    }

    // ------------------------------------------- the conc surface ----
    // (s73 — see the dispatch comment: single-threaded conservative
    // models; the cross-task laws are typing's and the runtime's.)

    /// Declare + init one binder at `span`, typed from sema's locals.
    fn declare_init(&mut self, name: &str, span: Span) {
        let ty = self.local_tys.get(&span).map(|&id| Ty {
            table: &self.tb.table,
            id,
        });
        let local = self.declare(name, span, ty);
        let place = self.places.intern(
            Place {
                base: Base::Local(local.0),
                proj: Vec::new(),
            },
            self.locals[local.0 as usize].is_copy,
        );
        self.push(Stmt::Init { place, span });
    }

    /// `scope name? { … }` — the handle binds over the body; the
    /// block's value is the body's ([conc.task.scope]). The join is
    /// the runtime's fact; here the body simply runs.
    fn eval_conc_scope(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = wolf_ast::ScopeExpr::cast(e).expect("kind");
        self.push_scope();
        if let Some(name) = d.name() {
            let nm = self.text(name.span);
            self.declare_init(&nm, name.span);
        }
        let out = match d.body() {
            Some(b) => self.walk_block(b, true),
            None => Ok(Val::none()),
        };
        let out = out?;
        self.close_scope(end_span(e.span))?;
        Ok(out)
    }

    /// A closure: the body models as "runs once, here" — captured
    /// reads are reads, closure locals live in their own scope. The
    /// closure VALUE carries no sites, but it BORROWS every captured
    /// place (s105 — the s98 pair design, plural): sema's capture
    /// record names them, each becomes a shared borrow claimed where
    /// the value lands (a spawn argument's call surface, a binding's
    /// scoped loans), and writes under a live closure refuse through
    /// the same NLL engine dyn pairs use.
    fn eval_closure(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = wolf_ast::ClosureExpr::cast(e).expect("kind");
        // The capture record (keyed by this closure's span). Resolved
        // to places BEFORE the body walk, at the derivation point —
        // the borrow begins where the env is built.
        let cap_names: Vec<String> = self
            .tb
            .task_captures
            .iter()
            .find(|(sp, _)| *sp == e.span)
            .map(|(_, caps)| caps.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();
        let mut borrowed: Vec<PlaceId> = Vec::new();
        for name in &cap_names {
            let Some(local) = self.lookup(name) else {
                continue; // not a frame local (module state): no place
            };
            let place = self.places.intern(
                Place {
                    base: Base::Local(local.0),
                    proj: Vec::new(),
                },
                self.locals[local.0 as usize].is_copy,
            );
            self.emit_read(place, e.span);
            borrowed.push(place);
        }
        self.push_scope();
        if let Some(params) = d.params() {
            for p in params.params() {
                if let Some(n) = p.name() {
                    let nm = self.text(n.span);
                    self.declare_init(&nm, n.span);
                }
            }
        }
        let r = match d.body() {
            Some(b) if b.kind == SyntaxKind::Block => {
                let blk = AstBlock::cast(b).expect("kind");
                self.walk_block(blk, false).map(|_| ())
            }
            Some(b) => self.eval_value(b).map(|_| ()),
            None => Ok(()),
        };
        r?;
        self.close_scope(end_span(e.span))?;
        if borrowed.is_empty() {
            return Ok(Val::none());
        }
        self.unclaimed_pairs.push((e.span, true));
        Ok(Val {
            sites: Vec::new(),
            region: None,
            origin: Some(e.span),
            region_fields: Vec::new(),
            borrowed,
        })
    }

    /// `select { … }` — arms are alternative branches (exactly one
    /// commits, [conc.select.ready]): a fan/join diamond, one scope
    /// per arm with its binder.
    fn eval_select(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = wolf_ast::SelectExpr::cast(e).expect("kind");
        // Evaluate every arm's source/duration head in the fan block
        // (readiness is observed before any arm body runs).
        let arms: Vec<_> = d.arms().collect();
        for arm in &arms {
            let body = arm.body();
            let head = arm
                .syntax()
                .nodes()
                .filter(|n| wolf_ast::is_expr_kind(n.kind))
                .find(|n| body.is_none_or(|b| !std::ptr::eq(*n as *const _, b as *const _)));
            if let Some(h) = head {
                match self.as_place(h) {
                    Some((place, _)) => self.emit_read(place, h.span),
                    None => {
                        self.eval_value(h)?;
                    }
                }
            }
        }
        let fan = self.cur;
        let join = self.new_block();
        for arm in &arms {
            let arm_block = self.new_block();
            self.goto(fan, arm_block);
            self.cur = arm_block;
            self.push_scope();
            if let Some(pat) = arm.pattern() {
                let mut binds = Vec::new();
                collect_binding_spans(pat, &mut binds);
                for span in binds {
                    let name = self.text(span);
                    self.declare_init(&name, span);
                }
            }
            if let Some(b) = arm.body() {
                match b.kind {
                    SyntaxKind::Block => {
                        let blk = AstBlock::cast(b).expect("kind");
                        self.walk_block(blk, false)?;
                    }
                    _ => {
                        self.eval_value(b)?;
                    }
                }
            }
            self.close_scope(end_span(arm.syntax().span))?;
            self.goto(self.cur, join);
        }
        if arms.is_empty() {
            self.goto(fan, join);
        }
        self.cur = join;
        Ok(Val::none())
    }

    /// `when (a, b) { … }` — operands are read (acquisition borrows
    /// the cells), simple paths rebind to their payloads over the
    /// body ([conc.when.body]).
    fn eval_when(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = wolf_ast::WhenExpr::cast(e).expect("kind");
        self.push_scope();
        for op in d.operands() {
            match self.as_place(op) {
                Some((place, _)) => self.emit_read(place, op.span),
                None => {
                    self.eval_value(op)?;
                }
            }
            if op.kind == SyntaxKind::PathExpr
                && let Some(t) = wolf_ast::PathExpr::cast(op).and_then(|p| p.ident())
            {
                let name = self.text(t.span);
                self.declare_init(&name, t.span);
            }
        }
        let out = match d.body() {
            Some(b) => self.walk_block(b, true),
            None => Ok(Val::none()),
        };
        let out = out?;
        self.close_scope(end_span(e.span))?;
        Ok(out)
    }

    /// `spawn proc f(args)` — args evaluate as call arguments (moves
    /// out of places, D14); the handle value is a plain word.
    fn eval_spawn(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = wolf_ast::SpawnExpr::cast(e).expect("kind");
        if let Some(args) = d.args() {
            for a in args.args() {
                if let Some(v) = a.value() {
                    self.eval_value(v)?;
                }
            }
        }
        Ok(Val::none())
    }

    fn eval_bin(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = wolf_ast::BinExpr::cast(e).expect("kind");
        let (lhs, rhs) = (d.lhs(), d.rhs());
        let op = d.op().map(|t| t.kind);
        match op {
            // Short-circuit: the right side is control flow.
            Some(SyntaxKind::AmpAmp | SyntaxKind::PipePipe) => {
                if let Some(l) = lhs {
                    self.eval_value(l)?;
                }
                let rhs_block = self.new_block();
                let join = self.new_block();
                self.goto(self.cur, rhs_block);
                self.goto(self.cur, join);
                self.cur = rhs_block;
                if let Some(r) = rhs {
                    self.eval_value(r)?;
                }
                self.goto(self.cur, join);
                self.cur = join;
                Ok(Val::none())
            }
            _ => {
                if let Some(l) = lhs {
                    self.eval_value(l)?;
                }
                if let Some(r) = rhs {
                    self.eval_value(r)?;
                }
                if matches!(
                    op,
                    Some(
                        SyntaxKind::Plus
                            | SyntaxKind::Minus
                            | SyntaxKind::Star
                            | SyntaxKind::Slash
                            | SyntaxKind::Percent
                    )
                ) && self.is_checked_int_op(lhs, rhs)
                {
                    self.push(Stmt::CheckedOp { span: e.span });
                    self.blocks[self.cur.0 as usize].trap = true;
                }
                Ok(Val::none())
            }
        }
    }

    /// X3: `+ - * / %` on integer operands trap (overflow/div-zero);
    /// `wrapping[T]` and floats do not.
    fn is_checked_int_op(&self, lhs: Option<&GreenNode>, rhs: Option<&GreenNode>) -> bool {
        let is_int = |n: Option<&GreenNode>| {
            n.and_then(|n| self.expr_tys.get(&n.span))
                .map(|&id| {
                    matches!(
                        self.tb.table.kind(id),
                        TyKind::Prim(p) if p.is_integer() || *p == Prim::Byte
                    )
                })
                .unwrap_or(false)
        };
        is_int(lhs) || is_int(rhs)
    }

    fn eval_if(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = IfExpr::cast(e).expect("kind");
        if let Some(cond) = d.condition() {
            self.eval_value(cond)?;
        }
        let then_block = self.new_block();
        let join = self.new_block();
        self.goto(self.cur, then_block);
        let cond_block = self.cur;
        self.cur = then_block;
        let mut val = Val::none();
        if let Some(tb) = d.then_block() {
            val.merge(self.walk_block(tb, true)?);
        }
        self.goto(self.cur, join);
        match d.else_branch() {
            Some(else_node) => {
                let else_block = self.new_block();
                self.goto(cond_block, else_block);
                self.cur = else_block;
                match else_node.kind {
                    SyntaxKind::Block => {
                        let b = AstBlock::cast(else_node).expect("kind");
                        val.merge(self.walk_block(b, true)?);
                    }
                    _ => val.merge(self.eval_value(else_node)?),
                }
                self.goto(self.cur, join);
            }
            None => {
                self.goto(cond_block, join);
            }
        }
        self.cur = join;
        Ok(val)
    }

    fn eval_match(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = MatchExpr::cast(e).expect("kind");
        let scrut_place = match d.scrutinee() {
            Some(s) => match self.as_place(s) {
                Some((place, _)) => {
                    self.emit_read(place, s.span);
                    Some((place, s.span))
                }
                None => {
                    self.eval_value(s)?;
                    None
                }
            },
            None => None,
        };
        let scrut_val = scrut_place.map(|(p, sp)| self.val_of_place(p, sp));
        let fan = self.cur;
        let join = self.new_block();
        let mut out = Val::none();
        for arm in d.arms() {
            let arm_block = self.new_block();
            self.goto(fan, arm_block);
            self.cur = arm_block;
            self.push_scope();
            let mut moved_scrut = false;
            if let Some(pat) = arm.pattern() {
                let mut binds = Vec::new();
                collect_binding_spans(pat, &mut binds);
                for span in binds {
                    let name = self.text(span);
                    let ty = self.local_tys.get(&span).map(|&id| Ty {
                        table: &self.tb.table,
                        id,
                    });
                    let local = self.declare(&name, span, ty);
                    let place = self.places.intern(
                        Place {
                            base: Base::Local(local.0),
                            proj: Vec::new(),
                        },
                        self.locals[local.0 as usize].is_copy,
                    );
                    self.push(Stmt::Init { place, span });
                    // A binding's piece stays in the scrutinee's
                    // region: it inherits the site set.
                    if let Some(sv) = scrut_val.clone() {
                        self.hold(local.0, &sv, span);
                    }
                    // A non-`Copy` binding takes its piece out of a
                    // place scrutinee: field-granular payload paths
                    // are a non-target, so the whole place moves in
                    // this arm (conservative, matching the dynamic
                    // machine's whole-value binding).
                    if !self.locals[local.0 as usize].is_copy
                        && !moved_scrut
                        && let Some((sp, sspan)) = scrut_place
                    {
                        self.emit_move(sp, sspan);
                        moved_scrut = true;
                    }
                }
            }
            if let Some(guard) = arm.guard() {
                // The guard node wraps its condition expression (s27:
                // feeding the wrapper to the walker refused honestly —
                // now the condition itself evaluates).
                if let Some(cond) = guard.nodes().find(|n| wolf_ast::is_expr_kind(n.kind)) {
                    self.eval_value(cond)?;
                }
            }
            if let Some(body) = arm.body() {
                out.merge(self.eval_value(body)?);
            }
            self.close_scope(end_span(arm.syntax().span))?;
            self.goto(self.cur, join);
        }
        self.cur = join;
        Ok(out)
    }

    fn eval_while(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = WhileExpr::cast(e).expect("kind");
        let head = self.new_block();
        self.goto(self.cur, head);
        self.cur = head;
        if let Some(cond) = d.condition() {
            self.eval_value(cond)?;
        }
        let body = self.new_block();
        let exit = self.new_block();
        self.goto(self.cur, body);
        self.goto(self.cur, exit);
        self.cur = body;
        self.loops.push(LoopFrame {
            break_to: exit,
            continue_to: head,
            scope_depth: self.scopes.len(),
        });
        if let Some(b) = d.body() {
            self.walk_block(b, false)?;
        }
        self.loops.pop();
        self.goto(self.cur, head);
        self.cur = exit;
        Ok(Val::none())
    }

    fn eval_for(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = ForExpr::cast(e).expect("kind");
        // s72, D40 ([mem.iter.excl]): iterating a place is a READ
        // with a claim held for the loop's extent — never a move. The
        // container stays live behind the walk and after it (the
        // E1001 reads-as-moves accident of #15 died here by ruling);
        // mut uses inside the body are E1013's, checked at their own
        // emission sites against the claim stack. A `Copy` iterable
        // is copied at loop entry and carries no claim — the same
        // instant-read model as `Copy` call arguments.
        let mut claimed = false;
        if let Some(iter) = d.iterable() {
            match self.as_place(iter) {
                Some((place, _)) => {
                    self.emit_read(place, iter.span);
                    if !self.places.is_copy(place) {
                        self.iter_claims.push((place, iter.span));
                        claimed = true;
                    }
                }
                None => {
                    self.eval_value(iter)?;
                }
            }
        }
        let head = self.new_block();
        self.goto(self.cur, head);
        let body = self.new_block();
        let exit = self.new_block();
        self.goto(head, body);
        self.goto(head, exit);
        self.cur = body;
        self.push_scope();
        if let Some(pat) = d.pattern() {
            let mut binds = Vec::new();
            collect_binding_spans(pat, &mut binds);
            for span in binds {
                let name = self.text(span);
                let ty = self.local_tys.get(&span).map(|&id| Ty {
                    table: &self.tb.table,
                    id,
                });
                let local = self.declare(&name, span, ty);
                let place = self.places.intern(
                    Place {
                        base: Base::Local(local.0),
                        proj: Vec::new(),
                    },
                    self.locals[local.0 as usize].is_copy,
                );
                self.push(Stmt::Init { place, span });
            }
        }
        self.loops.push(LoopFrame {
            break_to: exit,
            continue_to: head,
            scope_depth: self.scopes.len(),
        });
        if let Some(b) = d.body() {
            self.walk_block(b, false)?;
        }
        self.loops.pop();
        if claimed {
            self.iter_claims.pop();
        }
        self.close_scope(end_span(e.span))?;
        self.goto(self.cur, head);
        self.cur = exit;
        Ok(Val::none())
    }

    fn eval_loop(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = wolf_ast::LoopExpr::cast(e).expect("kind");
        let head = self.new_block();
        let exit = self.new_block();
        self.goto(self.cur, head);
        self.cur = head;
        self.loops.push(LoopFrame {
            break_to: exit,
            continue_to: head,
            scope_depth: self.scopes.len(),
        });
        if let Some(b) = d.body() {
            self.walk_block(b, false)?;
        }
        self.loops.pop();
        self.goto(self.cur, head);
        self.cur = exit;
        Ok(Val::none())
    }

    fn eval_else(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = ElseExpr::cast(e).expect("kind");
        let mut out = Val::none();
        if let Some(s) = d.scrutinized() {
            out.merge(self.eval_value(s)?);
        }
        let fallback = self.new_block();
        let join = self.new_block();
        self.goto(self.cur, join); // ok path
        self.goto(self.cur, fallback);
        self.cur = fallback;
        self.push_scope();
        if let Some(pat) = d.handler_pattern() {
            let mut binds = Vec::new();
            collect_binding_spans(pat, &mut binds);
            for span in binds {
                let name = self.text(span);
                let ty = self.local_tys.get(&span).map(|&id| Ty {
                    table: &self.tb.table,
                    id,
                });
                let local = self.declare(&name, span, ty);
                let place = self.places.intern(
                    Place {
                        base: Base::Local(local.0),
                        proj: Vec::new(),
                    },
                    self.locals[local.0 as usize].is_copy,
                );
                self.push(Stmt::Init { place, span });
            }
        }
        if let Some(fb) = d.fallback() {
            out.merge(self.eval_value(fb)?);
        }
        self.close_scope(end_span(e.span))?;
        self.goto(self.cur, join);
        self.cur = join;
        Ok(out)
    }

    // ----------------------------------------------------- calls ----

    fn eval_call(&mut self, e: &'t GreenNode) -> R<Val> {
        let d = CallExpr::cast(e).expect("kind");
        let cs = self.calls.get(&e.span).copied();
        let mut surface = CallSurface {
            callee: cs.map(|c| c.callee.clone()).unwrap_or_else(|| {
                d.callee()
                    .map(|c| self.text(c.span))
                    .unwrap_or_else(|| "<call>".to_string())
            }),
            span: e.span,
            mut_args: Vec::new(),
            read_args: Vec::new(),
            take_args: Vec::new(),
            c_call: cs.map(|c| c.c_call).unwrap_or(false),
        };
        // s22: a call through the `import c` namespace is unsafe-tier
        // (D11) — the ring is required, and the call is always emitted
        // as the FFI attribution point ([mem.boundary.ffi]).
        if surface.c_call {
            self.require_unsafe(&format!("the C call `{}`", surface.callee), e.span);
        }
        // Sites whose data the callee receives by `take`/`mut`: it
        // may embed them in the result (the conservative carry).
        let mut carry: Vec<SiteId> = Vec::new();
        // The receiver, when the resolved callee takes `self` and the
        // call site spells `recv.method(…)`.
        let mut receiver_done = false;
        if let Some(cs) = cs
            && cs.has_self
            && let Some(callee) = d.callee()
            && callee.kind == SyntaxKind::MemberExpr
            && let Some(m) = MemberExpr::cast(callee)
            && let Some(base) = m.base()
        {
            let selfp = cs.params.first().cloned();
            let recv_expr = match ParenExpr::cast(base) {
                Some(p) if p.mode().is_some() => p.expr().unwrap_or(base),
                _ => base,
            };
            if let Some(selfp) = selfp {
                self.lower_receiver(recv_expr, base.span, &selfp, &mut surface, &mut carry)?;
            }
            // s21: `clone()` on a `shared` receiver is the recorded
            // dup — Perceus inserts RC ops at genuine fan-out only,
            // and explicit clones are exactly that set today.
            if cs.callee == "clone"
                && matches!(
                    self.expr_ty(recv_expr.span).map(|t| t.kind().clone()),
                    Some(TyKind::Shared(_))
                )
                && matches!(
                    recv_expr.kind,
                    SyntaxKind::PathExpr | SyntaxKind::MemberExpr
                )
                && let Some((place, _)) = self.as_place(recv_expr)
            {
                self.push(Stmt::Dup {
                    place,
                    span: e.span,
                });
            }
            // s22: strict-provenance ops on a raw-pointer receiver
            // (RFC 3559's shape) — ring-gated derivations, recorded
            // for attribution; deriving is never an access
            // (creation-is-not-a-use). `is_null` is a plain compare
            // and stays free.
            if matches!(
                self.expr_ty(recv_expr.span).map(|t| t.kind().clone()),
                Some(TyKind::Ptr(_))
            ) && matches!(
                cs.callee.as_str(),
                "addr" | "with_addr" | "expose" | "with_exposed"
            ) {
                self.require_unsafe(&format!("the provenance op `{}`", cs.callee), e.span);
                let ptr = self.text(recv_expr.span);
                if matches!(cs.callee.as_str(), "expose" | "with_exposed") {
                    let dir = if cs.callee == "expose" {
                        "ptr->int"
                    } else {
                        "int->ptr"
                    };
                    self.push(Stmt::Expose {
                        what: ptr.clone(),
                        dir,
                        span: e.span,
                    });
                }
                self.push(Stmt::ProvOp {
                    op: cs.callee.clone(),
                    ptr,
                    span: e.span,
                });
            }
            receiver_done = true;
        }
        // A constructor's callee is a type head (`Node`, or the s21
        // `List[handle Node]` bracket form), not a value — nothing to
        // evaluate.
        let is_ctor = cs.map(|c| c.ctor).unwrap_or(false);
        if !receiver_done
            && !is_ctor
            && let Some(callee) = d.callee()
        {
            // Callee expressions with effects: a fn-typed place is
            // read; a member path's base may be one.
            match callee.kind {
                SyntaxKind::PathExpr | SyntaxKind::MemberExpr => {
                    if let Some((place, _)) = self.as_place(callee) {
                        self.emit_read(place, callee.span);
                    }
                }
                _ => {
                    self.eval_value(callee)?;
                }
            }
        }
        // Arguments, left to right.
        let args: Vec<Arg<'_>> = d.args().into_iter().flat_map(|a| a.args()).collect();
        let offset = usize::from(cs.map(|c| c.has_self).unwrap_or(false));
        let mut ctor_parts: Vec<(Val, Span)> = Vec::new();
        // Spelled `mut` arguments seen so far, in evaluation order —
        // the claims a later `Copy` read of this call evaluates inside
        // (s72, D39). Receiver `mut`s are two-phase and stay out.
        let mut arg_muts: Vec<(PlaceId, Span)> = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let Some(v) = Arg::value(*arg) else { continue };
            let site_mode = Arg::mode(*arg);
            let declared = cs.and_then(|c| c.params.get(i + offset));
            match (cs, declared) {
                (Some(cs), Some(param)) if cs.ctor => {
                    if let Some(mode) = site_mode {
                        self.mode_mismatch(cs, param, *arg, v, Some(mode), None);
                    }
                    let pv = self.eval_value(v)?; // payloads move in
                    ctor_parts.push((pv, v.span));
                }
                (Some(cs), Some(param)) => {
                    self.lower_arg(
                        cs,
                        param,
                        *arg,
                        v,
                        site_mode,
                        &mut surface,
                        &mut carry,
                        &mut arg_muts,
                    )?;
                }
                _ => {
                    // No resolved signature (builtin `print`, host
                    // stubs, or error-suppressed): arguments are
                    // reads — never silent moves.
                    if let Some((place, _)) = self.as_place(v) {
                        self.emit_read(place, v.span);
                    } else {
                        self.eval_value(v)?;
                    }
                }
            }
        }
        if !surface.mut_args.is_empty()
            || !surface.read_args.is_empty()
            || !surface.take_args.is_empty()
        {
            for &(place, span) in surface
                .mut_args
                .iter()
                .chain(surface.read_args.iter())
                .chain(surface.take_args.iter())
            {
                self.check_view(place, span);
            }
        }
        // -------------------------------- region attribution (s19) --
        // Enum construction is an allocation site like a struct
        // literal; payloads must be co-located.
        if cs.map(|c| c.ctor).unwrap_or(false) {
            let ty = self.rendered_expr_ty(e.span);
            let site = self.alloc_site(ty, SiteKind::Lit, e.span);
            let region = self.sites[site.0 as usize].region;
            let mut val = Val::site(site, e.span);
            for (pv, sp) in ctor_parts {
                if let Some(r) = pv.region {
                    // The payload owns the region: an iso edge, with
                    // the constructed value as parent
                    // ([mem.region.edge.iso]).
                    self.moved_region[r.0 as usize] = true;
                    self.region_parent.insert(r.0, region);
                }
                self.demand_store(&pv, region, Some(e.span), sp);
                val.merge(pv);
            }
            val.region = None;
            return Ok(val);
        }
        // A real call: the callee's default scheme (Cyclone rules)
        // says its result — and anything it keeps — lands in *its*
        // caller's region, which is this function's ambient at the
        // call site ([mem.region.create.3], D12). A non-`Copy` result
        // is therefore an ambient allocation here; `mut` arguments
        // may legally be replaced with fresh ambient allocations, so
        // they gain the call site too.
        let ret_ty = self.expr_tys.get(&e.span).map(|&id| Ty {
            table: &self.tb.table,
            id,
        });
        // A region return counts behind an error row too (s73:
        // `ch.recv()` on a `channel[region]` is `region ! {closed,
        // cancelled}` — the ok half is the fresh region identity).
        let ret_region = match ret_ty.map(|t| t.kind().clone()) {
            Some(TyKind::RegionTy) => true,
            Some(TyKind::ErrUnion(ok, _)) => {
                matches!(ret_ty.map(|t| t.table.kind(ok)), Some(TyKind::RegionTy))
            }
            _ => false,
        };
        let ret_heap = !ret_region && ret_ty.map(|t| !is_copy(t, 0)).unwrap_or(false);
        let mut_targets: Vec<PlaceId> = surface
            .mut_args
            .iter()
            .map(|(p, _)| *p)
            .filter(|p| !self.places.is_copy(*p))
            .collect();
        let has_surface = !surface.mut_args.is_empty()
            || !surface.read_args.is_empty()
            || !surface.take_args.is_empty();
        let callee = surface.callee.clone();
        if has_surface || surface.c_call {
            self.push(Stmt::Call(surface));
        }
        let mut out = Val::none();
        if ret_heap || !mut_targets.is_empty() {
            let ty = if ret_heap {
                self.rendered_expr_ty(e.span)
            } else {
                format!("{callee}(..)")
            };
            let site = self.alloc_site(ty, SiteKind::CallResult, e.span);
            // s21: a call handing back a `shared`/`weak` cell
            // (`clone`, `downgrade`) — the result is RC-owned, not
            // region-owned.
            if matches!(
                ret_ty.map(|t| t.kind()),
                Some(TyKind::Shared(_) | TyKind::Weak(_))
            ) {
                self.shared_site.insert(site);
                self.rc_cells.push(site);
            }
            if ret_heap {
                out = Val::site(site, e.span);
                for s in carry {
                    if let Err(i) = out.sites.binary_search(&s) {
                        out.sites.insert(i, s);
                    }
                }
            }
            let call_val = Val::site(site, e.span);
            for place in mut_targets {
                if let Base::Local(l) = self.places.get(place).base {
                    self.hold(l, &call_val, e.span);
                }
            }
        }
        if ret_region {
            // A call returning a region value (s20, the
            // scheme-carrying interface's shape): the callee hands
            // over a region it created — a fresh, distinct identity
            // on this side, whose affine value is the unique handle.
            // Never promotion-eligible: its create happened in the
            // callee, so the create/free pair is not frame-local.
            let rid = self.new_region("<region>", RegionKind::Value, Strategy::Arena, e.span);
            self.tainted_region[rid.0 as usize] = true;
            out.region = Some(rid);
            out.origin = Some(e.span);
        }
        Ok(out)
    }

    /// `take`/`mut` hand the callee data it may keep or replace: the
    /// held sites escape this frame's certainty (no stack promotion)
    /// and are carried into the call's result set.
    fn escape_to_callee(&mut self, place: PlaceId, carry: &mut Vec<SiteId>) {
        self.mark_region_lent(place);
        for (s, _) in self.sites_of_place(place) {
            self.mark_escape(s, "passed to a call");
            carry.push(s);
        }
    }

    fn lower_receiver(
        &mut self,
        recv: &'t GreenNode,
        recv_span: Span,
        selfp: &ParamSig,
        surface: &mut CallSurface,
        carry: &mut Vec<SiteId>,
    ) -> R<()> {
        match selfp.mode {
            None => {
                // `read self`: immutably lent for the call.
                if let Some((place, _)) = self.as_place(recv) {
                    self.emit_read(place, recv_span);
                    self.mark_region_lent(place);
                    if !self.places.is_copy(place) {
                        surface.read_args.push((place, recv_span));
                    }
                } else {
                    let rv = self.eval_value(recv)?;
                    // s98: `(p as dyn T).m()` — the receiver pair's
                    // borrow is call-extent, same as an argument.
                    if !rv.borrowed.is_empty() {
                        for &b in &rv.borrowed {
                            surface.read_args.push((b, recv_span));
                        }
                        self.claim_dyn(&rv);
                    }
                }
            }
            Some(ParamMode::Mut) => match self.as_place(recv) {
                Some((place, ty)) => {
                    self.check_frozen_write(place, recv_span, "passed as `mut`");
                    self.check_read_param_write(place, recv_span, "lent `mut`");
                    self.check_iter_claim(place, recv_span, "lent `mut`");
                    self.check_region_lend(place, recv_span);
                    self.escape_to_callee(place, carry);
                    match &selfp.view {
                        Some(view) => {
                            // The view set narrows the exclusive
                            // footprint to the named fields
                            // ([mem.tier0.excl.3]).
                            let base_place = self.places.get(place).clone();
                            let fields = ty.and_then(|t| self.fields_of(t));
                            for fname in view {
                                let fty = fields.as_ref().and_then(|fs| {
                                    fs.iter().find(|(n, _)| n == fname).map(|(_, t)| *t)
                                });
                                let mut proj = base_place.proj.clone();
                                proj.push(Proj::Field(fname.clone()));
                                let id = self.places.intern(
                                    Place {
                                        base: base_place.base.clone(),
                                        proj,
                                    },
                                    fty.map(|t| is_copy(t, 0)).unwrap_or(false),
                                );
                                surface.mut_args.push((id, recv_span));
                            }
                        }
                        None => surface.mut_args.push((place, recv_span)),
                    }
                }
                None => self.mut_needs_place(recv_span),
            },
            Some(ParamMode::Take) => {
                if let Some((place, _)) = self.as_place(recv) {
                    self.escape_to_callee(place, carry);
                    self.emit_move(place, recv_span);
                    surface.take_args.push((place, recv_span));
                } else {
                    let val = self.eval_value(recv)?;
                    self.mark_val_escape(&val, "passed to a call");
                    carry.extend(val.sites);
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_arg(
        &mut self,
        cs: &CallSig,
        param: &ParamSig,
        arg: Arg<'t>,
        v: &'t GreenNode,
        site_mode: Option<ParamMode>,
        surface: &mut CallSurface,
        carry: &mut Vec<SiteId>,
        arg_muts: &mut Vec<(PlaceId, Span)>,
    ) -> R<()> {
        if site_mode != param.mode {
            self.mode_mismatch(cs, param, arg, v, site_mode, param.mode);
        }
        match param.mode {
            None => {
                if let Some(mode) = site_mode
                    && mode == ParamMode::Take
                {
                    // The site spelled `take` against a `read`
                    // parameter: already reported; keep read
                    // semantics (the callee does not consume).
                }
                if let Some((place, _)) = self.as_place(v) {
                    self.emit_read(place, v.span);
                    self.mark_region_lent(place);
                    if !self.places.is_copy(place) {
                        // Non-`Copy` read arguments are lent for the
                        // whole call; `Copy` ones were copied at
                        // evaluation (which is what keeps the
                        // two-phase `xs.push(xs.len)` shape legal).
                        surface.read_args.push((place, v.span));
                    } else {
                        // s72, D39 — the overlap rule's static half:
                        // a `Copy` read completes at its own
                        // evaluation, and left-to-right order
                        // ([mem.model.order]) puts that evaluation
                        // INSIDE any exclusive claim an earlier `mut`
                        // argument of this call already spelled —
                        // f(mut a, a.x) reads a place the call holds.
                        // Receiver claims stay two-phase (reserved
                        // until entry), which is what keeps
                        // xs.push(xs.len) legal; a read BEFORE the
                        // `mut` (f(a.x, mut a)) finished before the
                        // claim began and stays legal too.
                        self.check_copy_read_after_mut(place, v.span, arg_muts);
                    }
                } else {
                    let av = self.eval_value(v)?;
                    // s98: a dyn cast in argument position — the
                    // pair's borrow is call-extent: the place joins
                    // the read surface (immutably lent for the call)
                    // and dies with it.
                    if !av.borrowed.is_empty() {
                        for &b in &av.borrowed {
                            surface.read_args.push((b, v.span));
                        }
                        self.claim_dyn(&av);
                    }
                }
            }
            Some(ParamMode::Mut) => match self.as_place(v) {
                Some((place, _)) => {
                    self.check_frozen_write(place, v.span, "passed as `mut`");
                    self.check_read_param_write(place, v.span, "lent `mut`");
                    self.check_iter_claim(place, v.span, "lent `mut`");
                    self.check_region_lend(place, v.span);
                    self.escape_to_callee(place, carry);
                    surface.mut_args.push((place, v.span));
                    arg_muts.push((place, v.span));
                }
                None => {
                    self.mut_needs_place(v.span);
                    self.eval_value(v)?;
                }
            },
            Some(ParamMode::Take) => {
                if let Some((place, _)) = self.as_place(v) {
                    self.escape_to_callee(place, carry);
                    self.emit_move(place, v.span);
                    surface.take_args.push((place, v.span));
                } else {
                    // Consuming a temporary is fine — it never had
                    // another owner.
                    let val = self.eval_value(v)?;
                    self.mark_val_escape(&val, "passed to a call");
                    carry.extend(val.sites);
                }
            }
        }
        Ok(())
    }

    /// E1007 — call-site mode agreement (X1, the c03 handoff): the
    /// argument's spelling must equal the parameter's declaration.
    /// Points at the argument, never the callee.
    fn mode_mismatch(
        &mut self,
        cs: &CallSig,
        param: &ParamSig,
        arg: Arg<'t>,
        v: &GreenNode,
        site: Option<ParamMode>,
        declared: Option<ParamMode>,
    ) {
        let mode_word = |m: ParamMode| match m {
            ParamMode::Mut => "mut",
            ParamMode::Take => "take",
        };
        let pname = if param.name.is_empty() {
            "this parameter".to_string()
        } else {
            format!("`{}`", param.name)
        };
        let mode_token_span = arg
            .syntax()
            .tokens()
            .find(|t| matches!(t.kind, SyntaxKind::MutKw | SyntaxKind::TakeKw))
            .map(|t| t.span);
        let (message, suggestion) = match (site, declared) {
            (None, Some(m)) => (
                format!(
                    "`{}` declares {pname} as `{}`, but the call site does not say so",
                    cs.callee,
                    mode_word(m)
                ),
                Some(Suggestion::new(
                    format!("write the argument's mode: `{} …`", mode_word(m)),
                    vec![(
                        Span::new(v.span.file, v.span.lo, v.span.lo),
                        format!("{} ", mode_word(m)),
                    )],
                    Applicability::MachineApplicable,
                )),
            ),
            (Some(m), None) => (
                if cs.ctor {
                    format!(
                        "`{}` builds a value: its arguments carry no call-site mode",
                        cs.callee
                    )
                } else {
                    format!(
                        "`{}` takes {pname} as plain `read` — no mode is written for it",
                        cs.callee,
                    )
                },
                mode_token_span.map(|ts| {
                    Suggestion::new(
                        format!("remove the `{}`", mode_word(m)),
                        vec![(Span::new(ts.file, ts.lo, v.span.lo), String::new())],
                        Applicability::MachineApplicable,
                    )
                }),
            ),
            (Some(s), Some(d)) => (
                format!(
                    "`{}` declares {pname} as `{}`, but the call site says `{}`",
                    cs.callee,
                    mode_word(d),
                    mode_word(s)
                ),
                mode_token_span.map(|ts| {
                    Suggestion::new(
                        format!("write `{}`", mode_word(d)),
                        vec![(ts, mode_word(d).to_string())],
                        Applicability::MachineApplicable,
                    )
                }),
            ),
            (None, None) => return,
        };
        let mut diag = Diagnostic::error(codes::E1007, v.span, message).with_label("this argument");
        let decl = if param.name.is_empty() {
            cs.decl_span.unwrap_or(param.span)
        } else {
            param.span
        };
        diag = diag.with_secondary(decl, "the parameter is declared here");
        if let Some(s) = suggestion {
            diag = diag.with_suggestion(s);
        }
        self.diags.push(diag);
    }

    /// E1009 — `mut` lends a location; a temporary has none.
    fn mut_needs_place(&mut self, span: Span) {
        self.diags.push(
            Diagnostic::error(
                codes::E1009,
                span,
                "a `mut` argument must name a place — a variable or a field path",
            )
            .with_label("this is a temporary value")
            .with_note(
                "the callee's writes would vanish with the temporary. Bind the value \
                 first: `var t = …`, then pass `mut t`.",
            ),
        );
    }

    // ------------------------------------------------ statements ----

    fn walk_block(&mut self, b: AstBlock<'t>, want_value: bool) -> R<Val> {
        self.push_scope();
        let stmts: Vec<&GreenNode> = b.statements().collect();
        let last_value_stmt = if want_value {
            b.trailing_expr().map(|e| e.span)
        } else {
            None
        };
        let mut out = Val::none();
        for stmt in stmts {
            match stmt.kind {
                SyntaxKind::ExprStmt => {
                    let d = ExprStmt::cast(stmt).expect("kind");
                    if let Some(e) = d.expr() {
                        let is_value = Some(e.span) == last_value_stmt;
                        // Value or discarded, the expression's effects
                        // are the same: a place in value position
                        // moves out (into the block's value or into a
                        // dropped temporary). Only the trailing value
                        // flows onward.
                        let val = self.eval_value(e)?;
                        if is_value {
                            out = val;
                        }
                    }
                }
                SyntaxKind::LetDecl => self.lower_let(stmt)?,
                SyntaxKind::VarDecl => self.lower_var(stmt)?,
                SyntaxKind::ConstDecl => self.lower_const(stmt)?,
                SyntaxKind::AssignStmt => self.lower_assign(stmt)?,
                SyntaxKind::DeferStmt => {
                    let d = DeferStmt::cast(stmt).expect("kind");
                    if let Some(e) = d.expr() {
                        let is_err = d.is_errdefer();
                        self.scopes
                            .last_mut()
                            .expect("scope")
                            .cleanup
                            .push(Cleanup::Defer(e, is_err));
                    }
                }
                SyntaxKind::AssumeStmt => self.lower_assume(stmt)?,
                // #116b: a nested named fn — capture-free (the
                // checker refused captures by name), so its body is a
                // self-contained frame: params are its own locals,
                // walked in its own scope exactly as a capture-free
                // closure's body is. The NAME binds as an initialized
                // Copy local (a fn value is one code pointer).
                SyntaxKind::FnDecl => {
                    let d = wolf_ast::FnDecl::cast(stmt).expect("kind");
                    self.push_scope();
                    if let Some(params) = d.params() {
                        for p in params.params() {
                            if let Some(n) = p.name() {
                                let nm = self.text(n.span);
                                self.declare_init(&nm, n.span);
                            }
                        }
                    }
                    if let Some(b) = d.body() {
                        self.walk_block(b, false)?;
                    }
                    self.close_scope(end_span(stmt.span))?;
                    if let Some(n) = d.name() {
                        let nm = self.text(n.span);
                        self.declare_init(&nm, n.span);
                    }
                }
                k if k.is_item() => {
                    return Err(NotYet {
                        construct: "nested item declarations",
                        span: stmt.span,
                    });
                }
                _ => {}
            }
        }
        self.close_scope(end_span(b.syntax().span))?;
        Ok(out)
    }

    /// Emit the scope's own defers (normal path, LIFO), free the
    /// first-class regions whose values die with it (s19: a region
    /// value's last-use free is approximated by its binding scope's
    /// end), and pop it.
    fn close_scope(&mut self, close_span: Span) -> R<()> {
        let depth = self.scopes.len() - 1;
        let result = self.emit_defers(depth, false);
        let (mark, owned): (usize, Vec<LocalId>) = {
            let scope = self.scopes.last().expect("scope");
            (
                scope.locals_mark,
                scope.names.iter().map(|(_, id)| *id).collect(),
            )
        };
        for local in owned {
            let Some(Some(rid)) = self.region_local.get(&local.0).copied() else {
                continue;
            };
            if self.rt.regions[rid.0 as usize].kind != RegionKind::Value {
                continue;
            }
            if self.moved_region[rid.0 as usize] {
                continue; // the affine value left; the free is not ours
            }
            let mut dummy = Val::none();
            self.sweep_region_close(rid, mark, close_span, &mut dummy);
            self.push(Stmt::RegionClose {
                region: rid,
                span: close_span,
            });
            // One free per region: a second binding in an outer scope
            // cannot free it again.
            self.moved_region[rid.0 as usize] = true;
        }
        self.scopes.pop();
        result
    }

    fn bind_pattern_inits(&mut self, pat: &'t GreenNode, has_init: bool, val: &Val) {
        let mut binds = Vec::new();
        collect_binding_spans(pat, &mut binds);
        let single = binds.len() == 1;
        for span in binds {
            let name = self.text(span);
            let ty = self.local_tys.get(&span).map(|&id| Ty {
                table: &self.tb.table,
                id,
            });
            let local = self.declare(&name, span, ty);
            // s21: a `shared`/`weak` local carries an RC drop
            // obligation at its scope's exit, LIFO with the defers
            // ([mem.shared.drop.1]).
            if matches!(
                ty.map(|t| t.kind()),
                Some(TyKind::Shared(_) | TyKind::Weak(_))
            ) && let Some(scope) = self.scopes.last_mut()
            {
                scope.cleanup.push(Cleanup::DropLocal(local));
            }
            let place = self.places.intern(
                Place {
                    base: Base::Local(local.0),
                    proj: Vec::new(),
                },
                self.locals[local.0 as usize].is_copy,
            );
            if has_init {
                self.push(Stmt::Init { place, span });
                // The binding holds whatever the initializer held (a
                // destructuring conservatively gives every binding
                // the whole set).
                self.hold(local.0, val, span);
                // s98 (D47): the pair's data half borrows its place —
                // a SHARED loan, borrower = this binding. The NLL
                // engine scopes it by the binding's liveness and
                // refuses writes/moves of the place while the pair is
                // needed. A copy of the binding does not extend the
                // loan (recorded for a D47 addendum); the region story
                // rides the held sites regardless.
                if single && !val.borrowed.is_empty() {
                    for &borrowed in &val.borrowed {
                        let loan = crate::cfg::LoanId(self.loans.len() as u32);
                        self.loans.push(crate::cfg::Loan {
                            place: borrowed,
                            kind: crate::cfg::LoanKind::Shared,
                            borrower: local,
                            origin: span,
                            two_phase: false,
                        });
                        self.push(Stmt::Borrow { loan, span });
                    }
                    self.claim_dyn(val);
                }
                if single && let Some(rid) = val.region {
                    // `let r = region()`: the value region takes the
                    // binding's name (dump/diagnostic identity).
                    if self.rt.regions[rid.0 as usize].name == "<region>" {
                        self.rt.regions[rid.0 as usize].name = name.clone();
                        self.rt.regions[rid.0 as usize].span = span;
                    }
                    self.region_local.insert(local.0, Some(rid));
                }
                if single {
                    // `let h = Holder { child: move c }`: the field
                    // path `h.child` keeps the region identity (s20 —
                    // `in h.child { }` opens through the iso edge).
                    for (f, r) in &val.region_fields {
                        self.region_field.insert((local.0, f.clone()), *r);
                    }
                }
            } else {
                self.push(Stmt::Uninit { place, span });
            }
        }
    }

    fn lower_let(&mut self, stmt: &'t GreenNode) -> R<()> {
        self.lower_binders(stmt)
    }

    fn lower_var(&mut self, stmt: &'t GreenNode) -> R<()> {
        self.lower_binders(stmt)
    }

    /// Every binder of a `let`/`var`, in source order — a comma group
    /// is the sequence of single bindings (D63).
    fn lower_binders(&mut self, stmt: &'t GreenNode) -> R<()> {
        for b in wolf_ast::binding_binders(stmt) {
            // s128 (#173): a tuple pattern over a PLACE initializer
            // moves each bound element out of its own sub-place —
            // partial moves per the tier-0 discipline; `_` leaves its
            // element untouched.
            if let (Some(pat), Some(init)) = (b.pattern, b.init)
                && pat.kind == SyntaxKind::TuplePat
                && !self.is_raw_index(init)
                && let Some((place, ty)) = self.as_place(init)
            {
                self.bind_tuple_from_place(pat, place, ty);
                continue;
            }
            let (has_init, val) = match b.init {
                Some(init) => (true, self.eval_value(init)?),
                None => (false, Val::none()),
            };
            if let Some(pat) = b.pattern {
                self.bind_pattern_inits(pat, has_init, &val);
            }
        }
        Ok(())
    }

    /// `let (x, y) = p` where `p` names a place: every element place
    /// is interned first (the move analysis expands partial residue
    /// over the interned universe), then each BOUND element is
    /// consumed from its own sub-place — a copy for Copy elements, a
    /// move otherwise — and bound as a single binding. Nested tuple
    /// patterns recurse; `_` elements are interned but never touched.
    fn bind_tuple_from_place(
        &mut self,
        pat: &'t GreenNode,
        base: PlaceId,
        base_ty: Option<Ty<'t>>,
    ) {
        let subs: Vec<&'t GreenNode> = pat.nodes().filter(|n| is_pattern_kind(n.kind)).collect();
        let elems = base_ty.and_then(|t| self.fields_of(t));
        let base_place = self.places.get(base).clone();
        let mut eplaces = Vec::with_capacity(subs.len());
        let n = subs.len().max(elems.as_ref().map(|e| e.len()).unwrap_or(0));
        for i in 0..n {
            let ety = elems.as_ref().and_then(|e| e.get(i)).map(|(_, t)| *t);
            let mut proj = base_place.proj.clone();
            proj.push(Proj::Field(i.to_string()));
            let ep = self.places.intern(
                Place {
                    base: base_place.base.clone(),
                    proj,
                },
                ety.map(|t| is_copy(t, 0)).unwrap_or(false),
            );
            eplaces.push((ep, ety));
        }
        for (i, sub) in subs.iter().enumerate() {
            if sub.kind == SyntaxKind::WildcardPat {
                continue;
            }
            let Some(&(ep, ety)) = eplaces.get(i) else {
                continue;
            };
            if sub.kind == SyntaxKind::TuplePat {
                self.bind_tuple_from_place(sub, ep, ety);
                continue;
            }
            let val = if self.places.is_copy(ep) {
                Val::none()
            } else {
                self.val_of_place(ep, sub.span)
            };
            if !self.places.is_copy(ep) {
                self.pattern_moves.push(sub.span);
            }
            self.use_value(ep, sub.span);
            self.bind_pattern_inits(sub, true, &val);
        }
    }

    /// A local `const` binds like a `let` (its comptime evaluation is
    /// the s16 pass; here it is a binding with an initializer).
    fn lower_const(&mut self, stmt: &'t GreenNode) -> R<()> {
        let d = wolf_ast::ConstDecl::cast(stmt).expect("kind");
        let (has_init, val) = match d.init() {
            Some(init) => (true, self.eval_value(init)?),
            None => (false, Val::none()),
        };
        if let Some(name) = d.name() {
            let span = name.span;
            let text = self.text(span);
            let ty = self.local_tys.get(&span).map(|&id| Ty {
                table: &self.tb.table,
                id,
            });
            let local = self.declare(&text, span, ty);
            let place = self.places.intern(
                Place {
                    base: Base::Local(local.0),
                    proj: Vec::new(),
                },
                self.locals[local.0 as usize].is_copy,
            );
            if has_init {
                self.push(Stmt::Init { place, span });
                self.hold(local.0, &val, span);
            } else {
                self.push(Stmt::Uninit { place, span });
            }
        }
        Ok(())
    }

    fn lower_assign(&mut self, stmt: &'t GreenNode) -> R<()> {
        let d = AssignStmt::cast(stmt).expect("kind");
        let val = match d.value() {
            Some(v) => self.eval_value(v)?,
            None => Val::none(),
        };
        let Some(place_expr) = d.place() else {
            return Ok(());
        };
        // s22: `p[i] = v` / `p[i] op= v` through a raw pointer is a
        // raw-tier write (compound also reads), not a place effect.
        if self.is_raw_index(place_expr) {
            if d.op().map(|t| t.kind != SyntaxKind::Eq).unwrap_or(false) {
                self.raw_index(place_expr, false)?;
            }
            self.raw_index(place_expr, true)?;
            return Ok(());
        }
        let Some((place, _)) = self.as_place(place_expr) else {
            return Err(NotYet {
                construct: "assignment through this place shape",
                span: place_expr.span,
            });
        };
        let compound = d.op().map(|t| t.kind != SyntaxKind::Eq).unwrap_or(false);
        if compound {
            self.emit_mutate(place, place_expr.span);
            let arith = matches!(
                d.op().map(|t| t.kind),
                Some(
                    SyntaxKind::PlusEq
                        | SyntaxKind::MinusEq
                        | SyntaxKind::StarEq
                        | SyntaxKind::SlashEq
                        | SyntaxKind::PercentEq
                )
            );
            if arith && self.is_checked_int_op(Some(place_expr), None) {
                self.push(Stmt::CheckedOp { span: stmt.span });
                self.blocks[self.cur.0 as usize].trap = true;
            }
        } else {
            self.emit_init(place, place_expr.span);
            // ------------------------------- region flow (s19) ------
            match self.places.get(place).base.clone() {
                Base::Local(l) => {
                    let whole = self.places.get(place).proj.is_empty();
                    if !whole {
                        // An embedding store demands co-location with
                        // the container's own allocation(s).
                        if let Some(r) = val.region {
                            // A region value stored into a field: the
                            // iso edge ([mem.region.edge.iso]) — the
                            // container's region becomes the parent,
                            // the free is no longer this frame's, and
                            // the field path keeps the identity.
                            self.moved_region[r.0 as usize] = true;
                            if let Some(&(c, _)) = self.sites_of_place(place).first() {
                                let target = self.sites[c.0 as usize].region;
                                self.region_parent.insert(r.0, target);
                            }
                            if let [Proj::Field(f)] = self.places.get(place).proj.as_slice() {
                                self.region_field.insert((l, f.clone()), r);
                            }
                        }
                        let containers = self.sites_of_place(place);
                        if let Some(&(c, _)) = containers.first() {
                            let target = self.sites[c.0 as usize].region;
                            let cspan = self.sites[c.0 as usize].span;
                            self.demand_store(&val, target, Some(cspan), place_expr.span);
                        }
                    } else if let Some(rid) = val.region {
                        match self.region_local.get(&l).copied() {
                            Some(Some(prev)) if prev != rid => {
                                // Rebound to a different region on
                                // some path: identity is no longer
                                // static — `in` on it refuses.
                                self.region_local.insert(l, None);
                            }
                            _ => {
                                self.region_local.insert(l, Some(rid));
                            }
                        }
                    }
                    self.hold(l, &val, place_expr.span);
                }
                Base::Global(..) => {
                    // Module state outlives every frame: `ρ_static`.
                    self.demand_static(&val, place_expr.span);
                }
            }
        }
        Ok(())
    }
}

/// The closing-brace span of a braced construct (the "freed here"
/// anchor).
fn end_span(span: Span) -> Span {
    Span::new(span.file, span.hi.saturating_sub(1), span.hi)
}

/// Collect the binding-ident spans of a pattern, in source order
/// ([gram.pat]: wildcard, literal, ident, path-with-sub-patterns,
/// tuple, or, `@`-binding). The caller reads the names from source.
fn collect_binding_spans(pat: &GreenNode, out: &mut Vec<Span>) {
    if matches!(pat.kind, SyntaxKind::IdentPat | SyntaxKind::BindingPat)
        && let Some(t) = pat.tokens().find(|t| t.kind == SyntaxKind::Ident)
    {
        out.push(t.span);
    }
    for child in pat.nodes().filter(|n| is_pattern_kind(n.kind)) {
        collect_binding_spans(child, out);
    }
}

impl<'t> Lowerer<'t> {
    // -------------------------------------------------- entry points --

    pub(crate) fn new(
        pkg: &'t Package,
        sigs: &'t SigTables,
        tb: &'t TypedBody,
        module: usize,
        file: usize,
    ) -> Self {
        let src = &pkg.files[file].raw.src;
        let calls = tb.calls.iter().map(|(s, c)| (*s, c)).collect();
        let local_tys = tb.locals.iter().map(|(_, s, t)| (*s, *t)).collect();
        let expr_tys = tb.exprs.iter().map(|(s, t)| (*s, *t)).collect();
        let casts = tb
            .casts
            .iter()
            .map(|(s, src_t, tgt_t, k)| (*s, (*src_t, *tgt_t, *k)))
            .collect();
        // b0: entry, b1: exit.
        let blocks = vec![Block::default(), Block::default()];
        Lowerer {
            pkg,
            sigs,
            tb,
            module,
            file,
            src,
            calls,
            local_tys,
            expr_tys,
            blocks,
            locals: Vec::new(),
            tys: Vec::new(),
            places: crate::place::PlaceTable::new(),
            cur: BlockId(0),
            exit: BlockId(1),
            scopes: Vec::new(),
            loops: Vec::new(),
            view: None,
            in_defer: false,
            diags: Vec::new(),
            rt: RegionTable::default(),
            ambient: Vec::new(),
            sites: Vec::new(),
            site_escape: Vec::new(),
            holds: BTreeMap::new(),
            region_local: HashMap::new(),
            moved_region: Vec::new(),
            tainted_region: Vec::new(),
            lent_region: Vec::new(),
            promoted: Vec::new(),
            conflicted: false,
            pattern_moves: Default::default(),
            static_region: None,
            open_stack: Vec::new(),
            region_parent: HashMap::new(),
            frozen_region: HashMap::new(),
            frozen_site: BTreeMap::new(),
            shared_site: BTreeSet::new(),
            rc_cells: Vec::new(),
            region_field: HashMap::new(),
            iter_claims: Vec::new(),
            unsafe_depth: 0,
            loans: Vec::new(),
            unclaimed_pairs: Vec::new(),
            casts,
        }
    }

    /// Lower a function body. `params` come from the elaborated
    /// signature (modes and view sets included).
    pub(crate) fn lower_fn(
        mut self,
        name: &str,
        params: &[ParamSig],
        body: AstBlock<'t>,
    ) -> R<Lowered> {
        // The implicit caller-region parameter: the ambient default
        // (D12, [mem.region.create.3]).
        let caller = self.new_region(
            "caller",
            RegionKind::Caller,
            Strategy::Arena,
            end_span(body.syntax().span),
        );
        self.ambient.push(caller);
        self.push_scope();
        for p in params {
            let ty = Ty {
                table: &self.sigs.table,
                id: p.ty,
            };
            let local = self.declare(&p.name, p.span, Some(ty));
            self.locals[local.0 as usize].param_mode = Some(p.mode);
            if p.name == "self"
                && let Some(view) = &p.view
            {
                self.view = Some((local, view.clone(), p.span));
            }
            // Cyclone defaults: every heap-reaching parameter position
            // gets a fresh, generalized region variable — a C-shaped
            // signature costs zero annotations.
            match ty.kind() {
                TyKind::RegionTy => {
                    let rid = self.new_region(&p.name, RegionKind::Param, Strategy::Arena, p.span);
                    self.region_local.insert(local.0, Some(rid));
                }
                _ if !is_copy(ty, 0) => {
                    let rid = self.new_region(&p.name, RegionKind::Param, Strategy::Arena, p.span);
                    let rendered = render(ty.table, ty.id, &|_| Err("_"));
                    let sid = SiteId(self.sites.len() as u32);
                    self.sites.push(AllocSite {
                        span: p.span,
                        ty: rendered,
                        region: rid,
                        kind: SiteKind::Param,
                    });
                    self.site_escape.push(None);
                    self.holds.entry(local.0).or_default().insert(sid, p.span);
                }
                _ => {}
            }
        }
        let val = self.walk_block(body, true)?;
        // The trailing value is the function's result: the default
        // scheme's "result lands in the caller's region".
        let result_span = val.origin.unwrap_or_else(|| end_span(body.syntax().span));
        self.demand_outlives_frame(&val, result_span);
        self.close_scope(end_span(body.syntax().span))?; // the param scope
        self.check_unclaimed_dyn()?;
        let exit = self.exit;
        self.goto(self.cur, exit);
        Ok(self.finish(name))
    }

    /// Lower a module-level item initializer as a body with no
    /// parameters. Its ambient is `ρ_static`: module state allocates
    /// in the static region and lives forever.
    pub(crate) fn lower_init(mut self, name: &str, init: &'t GreenNode) -> R<Lowered> {
        let st = self.new_region("static", RegionKind::Static, Strategy::Arena, init.span);
        self.static_region = Some(st);
        self.ambient.push(st);
        self.push_scope();
        self.eval_value(init)?;
        self.close_scope(end_span(init.span))?;
        self.check_unclaimed_dyn()?;
        let exit = self.exit;
        self.goto(self.cur, exit);
        Ok(self.finish(name))
    }

    /// s98 (D47's conservative reading): a dyn pair may land in a
    /// binding (a scoped loan) or a call's argument/receiver surface
    /// (a call-extent lend) — nowhere else, because nowhere else can
    /// this checker SEE the borrow. A pair left unclaimed flowed into
    /// a return, an aggregate, a container, a match — each a real
    /// future shape, each needing the borrow story written first;
    /// refuse by name rather than let the pair outlive its place.
    fn check_unclaimed_dyn(&self) -> Result<(), NotYet> {
        match self.unclaimed_pairs.first() {
            None => Ok(()),
            Some(&(span, closure)) => Err(NotYet {
                construct: if closure {
                    "a capturing closure outside a binding or argument position \
                     (bind it or pass it)"
                } else {
                    "a dyn pair outside a binding or argument position (bind it or pass it)"
                },
                span,
            }),
        }
    }

    fn finish(self, name: &str) -> Lowered {
        let _ = (self.pkg, self.file);
        // s21: the static atomicity bit — non-atomic unless a freeze
        // (the one statically-visible sharing point checkable today;
        // cross-thread transfer refuses until c05/c07) reaches the
        // cell. Thread-exclusivity is structural, never a runtime
        // test.
        let rc_cells: Vec<(SiteId, bool)> = self
            .rc_cells
            .iter()
            .map(|&s| (s, self.frozen_site.contains_key(&s)))
            .collect();
        let regions = crate::regions::summarize(
            name,
            self.rt,
            &self.sites,
            &self.site_escape,
            &self.promoted,
            self.conflicted,
        );
        Lowered {
            cfg: Cfg {
                name: name.to_string(),
                blocks: self.blocks,
                locals: self.locals,
                places: self.places,
                loans: self.loans,
                regions: regions.regions.clone(),
                sites: self.sites,
                entry: BlockId(0),
                exit: BlockId(1),
                pattern_moves: self.pattern_moves,
            },
            diags: self.diags,
            regions,
            rc_cells,
        }
    }
}
