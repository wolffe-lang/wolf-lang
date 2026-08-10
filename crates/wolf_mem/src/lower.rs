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

use std::collections::HashMap;

use wolf_ast::{
    Arg, AssignStmt, Block as AstBlock, CallExpr, CastExpr, DeferStmt, ElseExpr, ExprStmt,
    FieldInit, ForExpr, GreenNode, IfExpr, LetDecl, MatchExpr, MemberExpr, ParamMode, ParenExpr,
    PathExpr, PrefixExpr, RangeExpr, ReturnExpr, StringExpr, StructLit, SyntaxKind, TupleExpr,
    VarDecl, WhileExpr, is_pattern_kind,
};
use wolf_diag::{Applicability, Diagnostic, Suggestion, codes};
use wolf_span::Span;

use wolf_sema::check::CallSig;
use wolf_sema::sig::{ItemSig, ParamSig, SigTables};
use wolf_sema::types::{Prim, TyId, TyKind, TypeTable, render};
use wolf_sema::{NotYet, Package, TypedBody};

use crate::cfg::{Block, BlockId, CallSurface, Cfg, Local, LocalId, Stmt};
use crate::place::{Base, Place, PlaceId, Proj};

type R<T> = Result<T, NotYet>;

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

/// One lowered body: the CFG plus the context-free diagnostics found
/// on the way.
pub struct Lowered {
    pub cfg: Cfg,
    pub diags: Vec<Diagnostic>,
}

struct Scope<'t> {
    names: Vec<(String, LocalId)>,
    defers: Vec<(&'t GreenNode, bool)>,
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

    // ---------------------------------------------------- places ----

    /// Struct field types of a place type, for sibling interning and
    /// member typing.
    fn fields_of(&self, t: Ty<'t>) -> Option<Vec<(String, Ty<'t>)>> {
        match t.kind() {
            TyKind::Nominal { module, name } => match self.sigs.get(*module as usize, name)? {
                ItemSig::Struct(ss) => Some(
                    ss.fields
                        .iter()
                        .map(|f| {
                            (
                                f.name.clone(),
                                Ty {
                                    table: &self.sigs.table,
                                    id: f.ty,
                                },
                            )
                        })
                        .collect(),
                ),
                _ => None,
            },
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
                let copy = field_ty.map(|t| is_copy(t, 0)).unwrap_or(false);
                Some((self.places.intern(place, copy), field_ty))
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
        self.push(Stmt::Move { place, span });
    }

    fn emit_init(&mut self, place: PlaceId, span: Span) {
        self.check_view(place, span);
        self.push(Stmt::Init { place, span });
    }

    fn emit_mutate(&mut self, place: PlaceId, span: Span) {
        self.check_view(place, span);
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

    // ---------------------------------------------------- defers ----

    /// Lower pending defers for scopes `[depth..]`, innermost first,
    /// each scope's defers in reverse declaration order (LIFO).
    /// `error_path` includes `errdefer`s.
    fn emit_defers(&mut self, depth: usize, error_path: bool) -> R<()> {
        if self.in_defer {
            return Ok(());
        }
        self.in_defer = true;
        let mut pending: Vec<&'t GreenNode> = Vec::new();
        for scope in self.scopes[depth..].iter().rev() {
            for (expr, is_err) in scope.defers.iter().rev() {
                if *is_err && !error_path {
                    continue;
                }
                pending.push(expr);
            }
        }
        let mut result = Ok(());
        for expr in pending {
            if let Err(nyc) = self.eval_value(expr) {
                result = Err(nyc);
                break;
            }
        }
        self.in_defer = false;
        result
    }

    // ------------------------------------------------ expressions ----

    /// Evaluate an expression for its value: a place moves (or
    /// copies); everything else recurses structurally in evaluation
    /// order (`[mem.model.order]`).
    fn eval_value(&mut self, e: &'t GreenNode) -> R<()> {
        match e.kind {
            SyntaxKind::LiteralExpr => Ok(()),
            SyntaxKind::PathExpr | SyntaxKind::MemberExpr => {
                if let Some((place, _)) = self.as_place(e) {
                    self.use_value(place, e.span);
                    return Ok(());
                }
                // A field of a temporary: evaluate the base; the
                // projection itself has no place effects.
                if e.kind == SyntaxKind::MemberExpr
                    && let Some(base) = MemberExpr::cast(e).and_then(|m| m.base())
                {
                    return self.eval_value(base);
                }
                Ok(()) // item reference (fn value, enum type, …)
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
                Ok(())
            }
            SyntaxKind::ParenExpr => match ParenExpr::cast(e).and_then(|p| p.expr()) {
                Some(inner) => self.eval_value(inner),
                None => Ok(()),
            },
            SyntaxKind::TupleExpr => {
                for elem in TupleExpr::cast(e).expect("kind").elems() {
                    self.eval_value(elem)?;
                }
                Ok(())
            }
            SyntaxKind::Block => {
                let b = AstBlock::cast(e).expect("kind");
                self.walk_block(b, true)
            }
            SyntaxKind::PrefixExpr => self.eval_prefix(e),
            SyntaxKind::BinExpr => self.eval_bin(e),
            SyntaxKind::CastExpr => match CastExpr::cast(e).and_then(|c| c.expr()) {
                Some(inner) => self.eval_value(inner),
                None => Ok(()),
            },
            SyntaxKind::RangeExpr => {
                for end in RangeExpr::cast(e).expect("kind").endpoints() {
                    self.eval_value(end)?;
                }
                Ok(())
            }
            SyntaxKind::TryExpr => {
                let d = wolf_ast::TryExpr::cast(e).expect("kind");
                if let Some(inner) = d.expr() {
                    self.eval_value(inner)?;
                }
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
                Ok(())
            }
            SyntaxKind::CallExpr => self.eval_call(e),
            SyntaxKind::StructLit => {
                let d = StructLit::cast(e).expect("kind");
                for f in d.fields() {
                    if let Some(v) = FieldInit::value(f) {
                        // Field initializers consume their values.
                        self.eval_value(v)?;
                    }
                }
                Ok(())
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
                    self.eval_value(v)?;
                }
                self.emit_defers(0, false)?;
                let exit = self.exit;
                self.goto(self.cur, exit);
                let dead = self.new_block();
                self.cur = dead;
                Ok(())
            }
            SyntaxKind::BreakExpr => {
                let Some(frame) = self.loops.last() else {
                    return Ok(()); // parse-adjacent wreckage
                };
                let (target, depth) = (frame.break_to, frame.scope_depth);
                self.emit_defers(depth, false)?;
                self.goto(self.cur, target);
                let dead = self.new_block();
                self.cur = dead;
                Ok(())
            }
            SyntaxKind::ContinueExpr => {
                let Some(frame) = self.loops.last() else {
                    return Ok(());
                };
                let (target, depth) = (frame.continue_to, frame.scope_depth);
                self.emit_defers(depth, false)?;
                self.goto(self.cur, target);
                let dead = self.new_block();
                self.cur = dead;
                Ok(())
            }
            SyntaxKind::ClosureExpr => Err(NotYet {
                construct: "closure capture analysis (c05 tasks)",
                span: e.span,
            }),
            SyntaxKind::BracketApply | SyntaxKind::FromEndExpr => Err(NotYet {
                construct: "indexing/slicing places (s05 std surface)",
                span: e.span,
            }),
            SyntaxKind::RegionBlock
            | SyntaxKind::RegionValue
            | SyntaxKind::InBlock
            | SyntaxKind::FreezeExpr => Err(NotYet {
                construct: "region checking (s19–s20)",
                span: e.span,
            }),
            SyntaxKind::UnsafeBlock | SyntaxKind::InlineC | SyntaxKind::AsmExpr => Err(NotYet {
                construct: "the unsafe tier (s22)",
                span: e.span,
            }),
            SyntaxKind::ScopeExpr
            | SyntaxKind::SelectExpr
            | SyntaxKind::WhenExpr
            | SyntaxKind::SpawnExpr => Err(NotYet {
                construct: "structured concurrency (c05)",
                span: e.span,
            }),
            SyntaxKind::BorrowExpr => Err(NotYet {
                construct: "the unsafe tier's re-entry doors (s22)",
                span: e.span,
            }),
            _ => Err(NotYet {
                construct: "this expression shape (memory tier)",
                span: e.span,
            }),
        }
    }

    fn eval_prefix(&mut self, e: &'t GreenNode) -> R<()> {
        let d = PrefixExpr::cast(e).expect("kind");
        let Some(operand) = d.operand() else {
            return Ok(());
        };
        match d.op().map(|t| t.kind) {
            Some(SyntaxKind::CopyKw) => {
                // `copy x`: an independent value from any type —
                // never a move ([mem.tier0.move.3]).
                if let Some((place, _)) = self.as_place(operand) {
                    self.emit_read(place, operand.span);
                    Ok(())
                } else {
                    self.eval_value(operand)
                }
            }
            Some(SyntaxKind::MoveKw) => {
                if let Some((place, _)) = self.as_place(operand) {
                    self.use_value(place, operand.span);
                    Ok(())
                } else {
                    self.eval_value(operand)
                }
            }
            Some(SyntaxKind::Amp) => Err(NotYet {
                construct: "first-class borrow expressions (typeable with the region campaign)",
                span: e.span,
            }),
            Some(SyntaxKind::Star) => Err(NotYet {
                construct: "the unsafe tier (s22)",
                span: e.span,
            }),
            Some(SyntaxKind::SharedKw) => Err(NotYet {
                construct: "`shared` allocation (s21)",
                span: e.span,
            }),
            _ => self.eval_value(operand),
        }
    }

    fn eval_bin(&mut self, e: &'t GreenNode) -> R<()> {
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
                Ok(())
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
                Ok(())
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

    fn eval_if(&mut self, e: &'t GreenNode) -> R<()> {
        let d = IfExpr::cast(e).expect("kind");
        if let Some(cond) = d.condition() {
            self.eval_value(cond)?;
        }
        let then_block = self.new_block();
        let join = self.new_block();
        self.goto(self.cur, then_block);
        let cond_block = self.cur;
        self.cur = then_block;
        if let Some(tb) = d.then_block() {
            self.walk_block(tb, true)?;
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
                        self.walk_block(b, true)?;
                    }
                    _ => self.eval_value(else_node)?,
                }
                self.goto(self.cur, join);
            }
            None => {
                self.goto(cond_block, join);
            }
        }
        self.cur = join;
        Ok(())
    }

    fn eval_match(&mut self, e: &'t GreenNode) -> R<()> {
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
        let fan = self.cur;
        let join = self.new_block();
        for arm in d.arms() {
            let arm_block = self.new_block();
            self.goto(fan, arm_block);
            self.cur = arm_block;
            self.scopes.push(Scope {
                names: Vec::new(),
                defers: Vec::new(),
            });
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
                self.eval_value(guard)?;
            }
            if let Some(body) = arm.body() {
                self.eval_value(body)?;
            }
            self.close_scope()?;
            self.goto(self.cur, join);
        }
        self.cur = join;
        Ok(())
    }

    fn eval_while(&mut self, e: &'t GreenNode) -> R<()> {
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
        Ok(())
    }

    fn eval_for(&mut self, e: &'t GreenNode) -> R<()> {
        let d = ForExpr::cast(e).expect("kind");
        if let Some(iter) = d.iterable() {
            match iter.kind {
                SyntaxKind::RangeExpr => self.eval_value(iter)?,
                _ => self.eval_value(iter)?,
            }
        }
        let head = self.new_block();
        self.goto(self.cur, head);
        let body = self.new_block();
        let exit = self.new_block();
        self.goto(head, body);
        self.goto(head, exit);
        self.cur = body;
        self.scopes.push(Scope {
            names: Vec::new(),
            defers: Vec::new(),
        });
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
        self.close_scope()?;
        self.goto(self.cur, head);
        self.cur = exit;
        Ok(())
    }

    fn eval_loop(&mut self, e: &'t GreenNode) -> R<()> {
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
        Ok(())
    }

    fn eval_else(&mut self, e: &'t GreenNode) -> R<()> {
        let d = ElseExpr::cast(e).expect("kind");
        if let Some(s) = d.scrutinized() {
            self.eval_value(s)?;
        }
        let fallback = self.new_block();
        let join = self.new_block();
        self.goto(self.cur, join); // ok path
        self.goto(self.cur, fallback);
        self.cur = fallback;
        self.scopes.push(Scope {
            names: Vec::new(),
            defers: Vec::new(),
        });
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
            self.eval_value(fb)?;
        }
        self.close_scope()?;
        self.goto(self.cur, join);
        self.cur = join;
        Ok(())
    }

    // ----------------------------------------------------- calls ----

    fn eval_call(&mut self, e: &'t GreenNode) -> R<()> {
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
        };
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
                self.lower_receiver(recv_expr, base.span, &selfp, &mut surface)?;
            }
            receiver_done = true;
        }
        if !receiver_done && let Some(callee) = d.callee() {
            // Callee expressions with effects: a fn-typed place is
            // read; a member path's base may be one.
            match callee.kind {
                SyntaxKind::PathExpr | SyntaxKind::MemberExpr => {
                    if let Some((place, _)) = self.as_place(callee) {
                        self.emit_read(place, callee.span);
                    }
                }
                _ => self.eval_value(callee)?,
            }
        }
        // Arguments, left to right.
        let args: Vec<Arg<'_>> = d.args().into_iter().flat_map(|a| a.args()).collect();
        let offset = usize::from(cs.map(|c| c.has_self).unwrap_or(false));
        for (i, arg) in args.iter().enumerate() {
            let Some(v) = Arg::value(*arg) else { continue };
            let site_mode = Arg::mode(*arg);
            let declared = cs.and_then(|c| c.params.get(i + offset));
            match (cs, declared) {
                (Some(cs), Some(param)) if cs.ctor => {
                    if let Some(mode) = site_mode {
                        self.mode_mismatch(cs, param, *arg, v, Some(mode), None);
                    }
                    self.eval_value(v)?; // payloads move in
                }
                (Some(cs), Some(param)) => {
                    self.lower_arg(cs, param, *arg, v, site_mode, &mut surface)?;
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
            self.push(Stmt::Call(surface));
        }
        Ok(())
    }

    fn lower_receiver(
        &mut self,
        recv: &'t GreenNode,
        recv_span: Span,
        selfp: &ParamSig,
        surface: &mut CallSurface,
    ) -> R<()> {
        match selfp.mode {
            None => {
                // `read self`: immutably lent for the call.
                if let Some((place, _)) = self.as_place(recv) {
                    self.emit_read(place, recv_span);
                    if !self.places.is_copy(place) {
                        surface.read_args.push((place, recv_span));
                    }
                } else {
                    self.eval_value(recv)?;
                }
            }
            Some(ParamMode::Mut) => match self.as_place(recv) {
                Some((place, ty)) => {
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
                    self.emit_move(place, recv_span);
                    surface.take_args.push((place, recv_span));
                } else {
                    self.eval_value(recv)?;
                }
            }
        }
        Ok(())
    }

    fn lower_arg(
        &mut self,
        cs: &CallSig,
        param: &ParamSig,
        arg: Arg<'t>,
        v: &'t GreenNode,
        site_mode: Option<ParamMode>,
        surface: &mut CallSurface,
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
                    if !self.places.is_copy(place) {
                        // Non-`Copy` read arguments are lent for the
                        // whole call; `Copy` ones were copied at
                        // evaluation (which is what keeps the
                        // two-phase `xs.push(xs.len)` shape legal).
                        surface.read_args.push((place, v.span));
                    }
                } else {
                    self.eval_value(v)?;
                }
            }
            Some(ParamMode::Mut) => match self.as_place(v) {
                Some((place, _)) => surface.mut_args.push((place, v.span)),
                None => {
                    self.mut_needs_place(v.span);
                    self.eval_value(v)?;
                }
            },
            Some(ParamMode::Take) => {
                if let Some((place, _)) = self.as_place(v) {
                    self.emit_move(place, v.span);
                    surface.take_args.push((place, v.span));
                } else {
                    // Consuming a temporary is fine — it never had
                    // another owner.
                    self.eval_value(v)?;
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

    fn walk_block(&mut self, b: AstBlock<'t>, want_value: bool) -> R<()> {
        self.scopes.push(Scope {
            names: Vec::new(),
            defers: Vec::new(),
        });
        let stmts: Vec<&GreenNode> = b.statements().collect();
        let last_value_stmt = if want_value {
            b.trailing_expr().map(|e| e.span)
        } else {
            None
        };
        for stmt in stmts {
            match stmt.kind {
                SyntaxKind::ExprStmt => {
                    let d = ExprStmt::cast(stmt).expect("kind");
                    if let Some(e) = d.expr() {
                        let _is_value = Some(e.span) == last_value_stmt;
                        // Value or discarded, the expression's effects
                        // are the same: a place in value position
                        // moves out (into the block's value or into a
                        // dropped temporary).
                        self.eval_value(e)?;
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
                            .defers
                            .push((e, is_err));
                    }
                }
                SyntaxKind::AssumeStmt => {
                    return Err(NotYet {
                        construct: "the unsafe tier (s22)",
                        span: stmt.span,
                    });
                }
                k if k.is_item() => {
                    return Err(NotYet {
                        construct: "nested item declarations (c14 backlog)",
                        span: stmt.span,
                    });
                }
                _ => {}
            }
        }
        self.close_scope()
    }

    /// Emit the scope's own defers (normal path, LIFO) and pop it.
    fn close_scope(&mut self) -> R<()> {
        let depth = self.scopes.len() - 1;
        let result = self.emit_defers(depth, false);
        self.scopes.pop();
        result
    }

    fn bind_pattern_inits(&mut self, pat: &'t GreenNode, has_init: bool) {
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
            if has_init {
                self.push(Stmt::Init { place, span });
            } else {
                self.push(Stmt::Uninit { place, span });
            }
        }
    }

    fn lower_let(&mut self, stmt: &'t GreenNode) -> R<()> {
        let d = LetDecl::cast(stmt).expect("kind");
        let has_init = match d.init() {
            Some(init) => {
                self.eval_value(init)?;
                true
            }
            None => false,
        };
        if let Some(pat) = d.pattern() {
            self.bind_pattern_inits(pat, has_init);
        }
        Ok(())
    }

    fn lower_var(&mut self, stmt: &'t GreenNode) -> R<()> {
        let d = VarDecl::cast(stmt).expect("kind");
        let has_init = match d.init() {
            Some(init) => {
                self.eval_value(init)?;
                true
            }
            None => false,
        };
        if let Some(pat) = d.pattern() {
            self.bind_pattern_inits(pat, has_init);
        }
        Ok(())
    }

    /// A local `const` binds like a `let` (its comptime evaluation is
    /// the s16 pass; here it is a binding with an initializer).
    fn lower_const(&mut self, stmt: &'t GreenNode) -> R<()> {
        let d = wolf_ast::ConstDecl::cast(stmt).expect("kind");
        let has_init = match d.init() {
            Some(init) => {
                self.eval_value(init)?;
                true
            }
            None => false,
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
            } else {
                self.push(Stmt::Uninit { place, span });
            }
        }
        Ok(())
    }

    fn lower_assign(&mut self, stmt: &'t GreenNode) -> R<()> {
        let d = AssignStmt::cast(stmt).expect("kind");
        if let Some(v) = d.value() {
            self.eval_value(v)?;
        }
        let Some(place_expr) = d.place() else {
            return Ok(());
        };
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
        }
        Ok(())
    }
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
        self.scopes.push(Scope {
            names: Vec::new(),
            defers: Vec::new(),
        });
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
        }
        self.walk_block(body, true)?;
        self.close_scope()?; // the param scope (no defers of its own)
        let exit = self.exit;
        self.goto(self.cur, exit);
        Ok(self.finish(name))
    }

    /// Lower a module-level item initializer as a body with no
    /// parameters.
    pub(crate) fn lower_init(mut self, name: &str, init: &'t GreenNode) -> R<Lowered> {
        self.scopes.push(Scope {
            names: Vec::new(),
            defers: Vec::new(),
        });
        self.eval_value(init)?;
        self.close_scope()?;
        let exit = self.exit;
        self.goto(self.cur, exit);
        Ok(self.finish(name))
    }

    fn finish(self, name: &str) -> Lowered {
        let _ = (self.pkg, self.file);
        Lowered {
            cfg: Cfg {
                name: name.to_string(),
                blocks: self.blocks,
                locals: self.locals,
                places: self.places,
                loans: Vec::new(),
                entry: BlockId(0),
                exit: BlockId(1),
            },
            diags: self.diags,
        }
    }
}
