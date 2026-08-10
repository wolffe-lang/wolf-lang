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
