//! The check CFG (s18 Target 1) — basic blocks of *effect statements*
//! over places, with explicit edges for `if`/`match`/loops, explicit
//! error-union edges (`?` is a branch, D30), and explicit trap edges
//! for checked arithmetic (X3).
//!
//! # Invariants
//!
//! - **Checker-internal.** This CFG is used only by `wolf_mem`. WIR is
//!   built from sema's typed HIR (s25); checker facts flow to WIR as
//!   fact annotations, never as this CFG.
//! - **Effects, not values.** Statements record what an expression
//!   *does to places* — reads, moves, initializations, borrows, call
//!   argument surfaces — in left-to-right evaluation order
//!   (`[mem.model.order]`). Temporaries never appear: a value with no
//!   place needs no tracking.
//! - **Deterministic.** Blocks, statements, locals, places, and loans
//!   are created in walk order; two lowerings of the same body are
//!   byte-identical (the dump snapshots pin this).
//! - **Trap edges end executions.** A block containing a checked
//!   arithmetic site carries a `trap` successor edge; nothing is
//!   analyzed past a trap — traps abort, `defer`s do not run.
//! - **`defer`/`errdefer` are lowered at their exits.** A scope's
//!   pending defers are re-lowered (LIFO) on every path that leaves
//!   the scope: normal exit and `return` get `defer`s, error edges
//!   (`?`) get `errdefer`s and `defer`s interleaved in reverse
//!   declaration order. The duplication is sound: each exit is a
//!   distinct path.

use wolf_span::Span;

use crate::place::{PlaceId, PlaceTable};

/// A region variable (s19). Regions are type-level facts with zero
/// runtime representation (`[mem.region.create.4]`): the table exists
/// so every allocation in the dump is attributable to one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionId(pub u32);

/// What introduced a region variable. The crate invariant s20 stands
/// on: distinct `Scope`/`Value` regions that never unify denote
/// provably distinct regions (`[mem.region.create.2]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// The function's implicit caller-region parameter — the ambient
    /// default (D12, `[mem.region.create.3]`).
    Caller,
    /// Module state (`ρ_static`): item initializers allocate here.
    Static,
    /// A fresh, generalized parameter region (the Cyclone default:
    /// one per heap-reaching parameter position).
    Param,
    /// A syntactic `region name { … }` block (create+open+scope+free).
    Scope,
    /// A first-class `region(…)` value (X4).
    Value,
}

/// A region's allocation strategy (`[mem.region.create.1]`): cost,
/// never safety. Parsed and carried here; `rc` semantics are s21's,
/// `pool` internals are s21's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Strategy {
    Arena,
    Rc,
    /// The rendered element type.
    Pool(String),
}

#[derive(Debug, Clone)]
pub struct Region {
    /// `caller`, `static`, the parameter's name, the block's name
    /// (`<region>` for anonymous forms).
    pub name: String,
    pub kind: RegionKind,
    pub strategy: Strategy,
    /// The introduction site (function name span for `Caller`).
    pub span: Span,
}

/// An allocation site (s19). There is no `new` keyword: allocation
/// sites are struct literals, constructor calls, and (later)
/// collection constructors and closures (`[mem.model.alloc]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SiteId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteKind {
    /// A literal construction in this body (struct literal, enum
    /// constructor): the per-site stack-promotion candidate.
    Lit,
    /// A non-`Copy` call result: allocated by the callee into its
    /// caller's region — this function's ambient at the call site.
    CallResult,
    /// The pseudo-site of a non-`Copy` parameter: data received in
    /// that parameter's generalized region.
    Param,
}

#[derive(Debug, Clone)]
pub struct AllocSite {
    pub span: Span,
    /// Rendered type (dump/debug surface only).
    pub ty: String,
    /// The inferred region: the ambient region at the site
    /// (`[mem.region.create.3]`).
    pub region: RegionId,
    pub kind: SiteKind,
}

/// A function-local binding (parameters first, then bindings in walk
/// order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalId(pub u32);

#[derive(Debug, Clone)]
pub struct Local {
    pub name: String,
    /// The binding-site span.
    pub span: Span,
    /// Rendered type (dump/debug surface only).
    pub ty: String,
    pub is_copy: bool,
    /// `Some` for parameters, with the declared mode (`None` inside
    /// the option = `read`).
    pub param_mode: Option<Option<wolf_ast::ParamMode>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LoanId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoanKind {
    Shared,
    Unique,
}

/// A first-class local borrow (loan gen site). The dataflow engine
/// ([`crate::loans`]) is exercised by unit tests until `&`/`&mut`
/// expressions become typeable (they refuse in sema today — 01 Q4's
/// surface arrives with the region campaign's later sprints).
#[derive(Debug, Clone)]
pub struct Loan {
    pub place: PlaceId,
    pub kind: LoanKind,
    /// The local the borrow is bound to; its last use (backward
    /// liveness) ends the loan — NLL-grade, not lexical.
    pub borrower: LocalId,
    pub origin: Span,
    /// Two-phase (`v.push(v.len())` shape): reserved — conflicting
    /// with writes only — until its `Activate` statement runs.
    pub two_phase: bool,
}

/// One call's argument surface: the places lent or taken for the
/// call's extent, checked pairwise for exclusivity
/// (`[mem.tier0.excl]`) at this statement.
#[derive(Debug, Clone)]
pub struct CallSurface {
    pub callee: String,
    pub span: Span,
    /// `mut` arguments/receiver: exclusive for the whole call. A view
    /// set (`mut self.{x, y}`) narrows the receiver into one entry
    /// per viewed field.
    pub mut_args: Vec<(PlaceId, Span)>,
    /// Non-`Copy` `read`-mode place arguments: immutably lent for the
    /// whole call (`Copy` places were copied at evaluation and appear
    /// as `Read` statements instead).
    pub read_args: Vec<(PlaceId, Span)>,
    /// `take` arguments: already moved by their evaluation-order
    /// `Move` statements; listed here for pairwise conflicts.
    pub take_args: Vec<(PlaceId, Span)>,
    /// s22 — a call through the `import c` namespace: unsafe-tier by
    /// D11, always emitted (even with an empty argument surface) as
    /// the FFI attribution point ([mem.boundary.ffi]).
    pub c_call: bool,
}

/// One effect statement.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// Non-consuming use: a `Copy` use, an explicit `copy`, a
    /// condition, a `read`-mode copy at argument evaluation.
    Read { place: PlaceId, span: Span },
    /// Consuming use: the place becomes uninitialized
    /// (`[mem.tier0.move.1]`).
    Move { place: PlaceId, span: Span },
    /// Whole-place (re)initialization: revives
    /// (`[mem.tier0.move.4]`).
    Init { place: PlaceId, span: Span },
    /// Read-and-write in place (compound assignment): the place must
    /// be initialized and stays so.
    Mutate { place: PlaceId, span: Span },
    /// A local declared without an initializer: uninitialized, with
    /// the declaration as provenance.
    Uninit { place: PlaceId, span: Span },
    /// A call's argument surface (pairwise exclusivity).
    Call(CallSurface),
    /// Loan gen (first-class borrow).
    Borrow { loan: LoanId, span: Span },
    /// A use through a loan's borrower (access *through* the loan is
    /// exempt from that loan's own conflicts).
    UseBorrower { local: LocalId, span: Span },
    /// Two-phase activation: the loan's first write-capable use.
    Activate { loan: LoanId, span: Span },
    /// A checked arithmetic site: the containing block carries a trap
    /// edge (X3).
    CheckedOp { span: Span },
    /// A heap allocation, attributed to its inferred region — the "no
    /// hidden allocations" law (s19): every allocation is visible in
    /// the dump.
    Alloc { site: SiteId, span: Span },
    /// A region becomes the ambient region (`region name {` /
    /// `in r {` entry). Openness is depth-counted
    /// (`[mem.region.open.1]`).
    RegionOpen { region: RegionId, span: Span },
    /// The matching exit. For `Scope` regions the sugar-block exit is
    /// also the wholesale free (`[mem.region.intra.2]`); `Value`
    /// regions free at their binding scope's end unless moved away.
    RegionClose { region: RegionId, span: Span },
    /// s21 — a recorded RC increment on a `shared` cell (Perceus dup:
    /// genuine fan-out only, today exactly the explicit `clone()`
    /// sites). The insertion *plan*: c05/s26 lower these; reuse/fusion
    /// analysis refines them later. Not a use for the moves pass.
    Dup { place: PlaceId, span: Span },
    /// s21 — a recorded RC decrement/drop of a `shared`/`weak`-typed
    /// local at scope exit, LIFO with `defer`s (`[mem.shared.drop.1]`
    /// — the declaration span is the anchor). Conditional semantics
    /// (drop-if-live, Rust-drop-flag-shaped) so re-drops of moved-away
    /// locals never fire; drop-at-last-use migration is recorded in
    /// the facts, applied in c05.
    Drop { place: PlaceId, span: Span },
    /// s21 — a generational check at a `pool[h]` slot access
    /// (`[mem.shared.handle.3]`): the containing block carries a trap
    /// edge (`stale-handle`, X5's deterministic-fault contract —
    /// dynamic by design, no static half). Emitted as a checked-op
    /// fact so s42 can hoist/dedup within provably-unfreed windows.
    HandleCheck { place: PlaceId, span: Span },
    // ------------------------------------------- the unsafe tier (s22) --
    // Raw-tier statements carry *rendered* pointer expressions, not
    // places: raw memory has no place discipline by design (the rules
    // are simpler than the safe tier's, D11), and these statements
    // exist for UB *attribution* — s23's miri-lite and the WIR trace
    // every dynamic verdict back to one of them. The pointer value's
    // own read is a separate `Read` statement when it is a place.
    /// `unsafe {` — ring 1 opens (audit + attribution scope).
    UnsafeEnter { span: Span },
    /// The matching `}` — the ring closes; values crossing out
    /// re-enter the safe tier's invariants.
    UnsafeExit { span: Span },
    /// A read through a raw pointer (`p[i]`, deref): can hit [mem.ub]
    /// P1/P3/P4/L1/L2 dynamically — no static approximation exists.
    RawRead { ptr: String, span: Span },
    /// A write through a raw pointer: additionally P2 (Frozen) and T2.
    RawWrite { ptr: String, span: Span },
    /// `assume noalias p, q` — the asserted-UB fact ([mem.unsafe.raw.2]):
    /// licenses O5; violation is P5, checked dynamically by s23/is04.
    Assume { ops: String, span: Span },
    /// A provenance-affecting bridge ([mem.prov.expose]): `dir` is one
    /// of `ptr->ptr`, `ptr->int` (exposes the tag), `int->ptr`
    /// (exposed/wildcard resolution), `region->ptr` (the region's
    /// backing base).
    Expose {
        what: String,
        dir: &'static str,
        span: Span,
    },
    /// Re-entry door 1 — `borrow r from p` ([mem.unsafe.door]): the
    /// claim is UB-if-false at the *door* (P6); s23 checks range +
    /// liveness + tag compatibility dynamically.
    Door {
        region: String,
        ptr: String,
        span: Span,
    },
    /// A strict-provenance op (`addr`, `with_addr`, `expose`,
    /// `with_exposed`) on a raw pointer — RFC 3559's shape; deriving
    /// is never an access (creation-is-not-a-use, Tree Borrows).
    ProvOp { op: String, ptr: String, span: Span },
}

#[derive(Debug, Default)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub succs: Vec<BlockId>,
    /// X3: at least one statement can trap; executions may end here.
    pub trap: bool,
}

/// The per-function check CFG.
#[derive(Debug)]
pub struct Cfg {
    pub name: String,
    pub blocks: Vec<Block>,
    pub locals: Vec<Local>,
    pub places: PlaceTable,
    pub loans: Vec<Loan>,
    /// Region variables, walk order; `regions[0]` is always the
    /// caller/static ambient (s19).
    pub regions: Vec<Region>,
    /// Allocation sites, walk order (s19).
    pub sites: Vec<AllocSite>,
    pub entry: BlockId,
    /// The single normal/error exit (nothing is analyzed past it).
    pub exit: BlockId,
}

impl Cfg {
    pub fn block(&self, id: BlockId) -> &Block {
        &self.blocks[id.0 as usize]
    }

    pub fn local(&self, id: LocalId) -> &Local {
        &self.locals[id.0 as usize]
    }

    /// Render a place for diagnostics: `p`, `p.x`, `xs[_]`.
    pub fn show_place(&self, id: PlaceId) -> String {
        let place = self.places.get(id);
        let mut out = match &place.base {
            crate::place::Base::Local(l) => self.locals[*l as usize].name.clone(),
            crate::place::Base::Global(_, name) => name.clone(),
        };
        for step in &place.proj {
            match step {
                crate::place::Proj::Field(f) => {
                    out.push('.');
                    out.push_str(f);
                }
                crate::place::Proj::Opaque => out.push_str("[_]"),
            }
        }
        out
    }

    /// Render a region for dumps/diagnostics: `caller`, `static`,
    /// `param a`, `region tmp`.
    pub fn show_region(&self, id: RegionId) -> String {
        let r = &self.regions[id.0 as usize];
        match r.kind {
            RegionKind::Caller => "caller".to_string(),
            RegionKind::Static => "static".to_string(),
            RegionKind::Param => format!("param {}", r.name),
            RegionKind::Scope | RegionKind::Value => format!("region {}", r.name),
        }
    }

    /// The deterministic textual dump (the s01 IR-dump snapshot
    /// family): byte-offset spans, walk-order ids.
    pub fn dump(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(out, "fn {}", self.name);
        for (i, l) in self.locals.iter().enumerate() {
            let mode = match l.param_mode {
                None => String::new(),
                Some(None) => " param(read)".to_string(),
                Some(Some(wolf_ast::ParamMode::Mut)) => " param(mut)".to_string(),
                Some(Some(wolf_ast::ParamMode::Take)) => " param(take)".to_string(),
            };
            let _ = writeln!(
                out,
                "  _{i} {}: {}{}{}",
                l.name,
                l.ty,
                if l.is_copy { " copy" } else { "" },
                mode,
            );
        }
        let sp = |s: Span| format!("@{}..{}", s.lo, s.hi);
        for (bi, b) in self.blocks.iter().enumerate() {
            let _ = writeln!(out, "  b{bi}:");
            for s in &b.stmts {
                let line = match s {
                    Stmt::Read { place, span } => {
                        format!("read {} {}", self.show_place(*place), sp(*span))
                    }
                    Stmt::Move { place, span } => {
                        format!("move {} {}", self.show_place(*place), sp(*span))
                    }
                    Stmt::Init { place, span } => {
                        format!("init {} {}", self.show_place(*place), sp(*span))
                    }
                    Stmt::Mutate { place, span } => {
                        format!("mutate {} {}", self.show_place(*place), sp(*span))
                    }
                    Stmt::Uninit { place, span } => {
                        format!("uninit {} {}", self.show_place(*place), sp(*span))
                    }
                    Stmt::Call(c) => {
                        let show = |args: &[(PlaceId, Span)], tag: &str| {
                            args.iter()
                                .map(|(p, _)| format!("{tag} {}", self.show_place(*p)))
                                .collect::<Vec<_>>()
                        };
                        let mut parts = show(&c.mut_args, "mut");
                        parts.extend(show(&c.read_args, "read"));
                        parts.extend(show(&c.take_args, "take"));
                        format!("call {} [{}] {}", c.callee, parts.join(", "), sp(c.span))
                    }
                    Stmt::Borrow { loan, span } => {
                        let l = &self.loans[loan.0 as usize];
                        format!(
                            "borrow{} l{} = &{}{} {}",
                            if l.two_phase { " (two-phase)" } else { "" },
                            loan.0,
                            if l.kind == LoanKind::Unique {
                                "mut "
                            } else {
                                ""
                            },
                            self.show_place(l.place),
                            sp(*span)
                        )
                    }
                    Stmt::UseBorrower { local, span } => {
                        format!("use-borrower _{} {}", local.0, sp(*span))
                    }
                    Stmt::Activate { loan, span } => {
                        format!("activate l{} {}", loan.0, sp(*span))
                    }
                    Stmt::CheckedOp { span } => format!("checked-op {}", sp(*span)),
                    Stmt::Alloc { site, span } => {
                        let s = &self.sites[site.0 as usize];
                        format!(
                            "alloc {} -> {} {}",
                            s.ty,
                            self.show_region(s.region),
                            sp(*span)
                        )
                    }
                    Stmt::RegionOpen { region, span } => {
                        format!("region-open {} {}", self.show_region(*region), sp(*span))
                    }
                    Stmt::RegionClose { region, span } => {
                        format!("region-close {} {}", self.show_region(*region), sp(*span))
                    }
                    Stmt::Dup { place, span } => {
                        format!("dup {} {}", self.show_place(*place), sp(*span))
                    }
                    Stmt::Drop { place, span } => {
                        format!("drop {} {}", self.show_place(*place), sp(*span))
                    }
                    Stmt::HandleCheck { place, span } => {
                        format!("handle-check {} {}", self.show_place(*place), sp(*span))
                    }
                    Stmt::UnsafeEnter { span } => format!("unsafe-enter {}", sp(*span)),
                    Stmt::UnsafeExit { span } => format!("unsafe-exit {}", sp(*span)),
                    Stmt::RawRead { ptr, span } => {
                        format!("raw-read {ptr} {}", sp(*span))
                    }
                    Stmt::RawWrite { ptr, span } => {
                        format!("raw-write {ptr} {}", sp(*span))
                    }
                    Stmt::Assume { ops, span } => {
                        format!("assume-noalias {ops} {}", sp(*span))
                    }
                    Stmt::Expose { what, dir, span } => {
                        format!("expose {what} ({dir}) {}", sp(*span))
                    }
                    Stmt::Door { region, ptr, span } => {
                        format!("door borrow {region} from {ptr} {}", sp(*span))
                    }
                    Stmt::ProvOp { op, ptr, span } => {
                        format!("prov {op} {ptr} {}", sp(*span))
                    }
                };
                let _ = writeln!(out, "    {line}");
            }
            let mut edges: Vec<String> = b.succs.iter().map(|s| format!("b{}", s.0)).collect();
            if b.trap {
                edges.push("trap".to_string());
            }
            if !edges.is_empty() {
                let _ = writeln!(out, "    -> {}", edges.join(", "));
            }
        }
        out
    }
}
