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
//! **The ledger-honesty contract:** constructs s13 cannot type yet
//! return [`BodyResult::NotYetCheckable`] — never a guess. That set
//! includes method calls, generic instantiation, `?`, `else`
//! defaulting, trait anything, region/concurrency expressions, and the
//! unsafe tier. String-interpolation holes are accepted at any sized
//! primitive/`str` type; full format-spec validation is s16 (D26).
//! `!T` supports implicit ok-injection in check mode; the row side
//! stays opaque until s15.
//!
//! Checking is `(&SigTables, body) → BodyResult` with no shared
//! mutable state — each body is an independent inference problem
//! (Target 5; parallelized in [`crate::typecheck`]).

use wolf_ast::{
    Arg, ArgList, AssignStmt, BinExpr, Block, CallExpr, CastExpr, ClosureExpr, ConstDecl,
    DeferStmt, ExprStmt, FieldInit, FnDecl, ForExpr, GreenNode, IfExpr, LetDecl, MatchArm,
    MatchExpr, MemberExpr, ParenExpr, PathExpr, PrefixExpr, RangeExpr, ReturnExpr, StringExpr,
    StructLit, SyntaxKind, TupleExpr, VarDecl, WhileExpr, is_expr_kind, is_type_kind,
};
use wolf_diag::{Applicability, Diagnostic, Suggestion, codes};
use wolf_span::Span;

use crate::graph::{BindTarget, Package};
use crate::prelude;
use crate::sig::{FnSig, ItemSig, Lower, SigTables, StructSig, bindings_for};
use crate::types::{Prim, TyId, TyKind, TypeTable, diff, render};
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
pub enum BodyResult {
    /// Fully typed: the body's own table and the recorded types.
    Checked(TypedBody),
    /// A construct outside the s13-checkable set was encountered.
    NotYetCheckable(NotYet),
    /// The body is checkable and wrong.
    Errors(Vec<Diagnostic>),
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

/// One body to check: a function body or an item initializer.
#[derive(Debug, Clone)]
pub struct BodyRef {
    pub module: usize,
    pub file: usize,
    /// Item name (diagnostics + bench identity).
    pub name: String,
    /// Ordinal among the file root's item nodes.
    pub decl: usize,
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

struct Checker<'a> {
    sigs: &'a SigTables,
    module: usize,
    file: usize,
    lo: Lower<'a>,
    vars: VarStore,
    scopes: Vec<Vec<(String, TyId)>>,
    diags: Vec<Diagnostic>,
    exprs: Vec<(Span, TyId)>,
    locals: Vec<(String, Span, TyId)>,
    /// (return type, fn name, because span) of the enclosing function.
    ret: Option<(TyId, String, Span)>,
    generics: Vec<String>,
    level: u32,
    in_closure: bool,
    loops: Vec<LoopCtx>,
}

/// Check one body against the elaborated signatures. Pure in
/// `(&Package, &SigTables, &BodyRef)` — no shared mutable state.
pub fn check_body(pkg: &Package, sigs: &SigTables, body: &BodyRef) -> BodyResult {
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
        level: 0,
        in_closure: false,
        loops: Vec::new(),
    };
    let outcome = c.run(node, body);
    match outcome {
        Err(nyc) => BodyResult::NotYetCheckable(nyc),
        Ok(()) => {
            c.finish_defaulting();
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
            if diags.is_empty() {
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
                BodyResult::Checked(TypedBody {
                    table: c.lo.table,
                    exprs,
                    locals,
                })
            } else {
                BodyResult::Errors(diags)
            }
        }
    }
}

/// Deep-resolve every solved variable in `ty`; unresolved vars remain.
fn zonk(table: &mut TypeTable, vars: &VarStore, ty: TyId) -> TyId {
    let resolved = vars.shallow(table, ty);
    match table.kind(resolved).clone() {
        TyKind::Wrapping(t) => {
            let z = zonk(table, vars, t);
            table.intern(TyKind::Wrapping(z))
        }
        TyKind::ErrUnion(t) => {
            let z = zonk(table, vars, t);
            table.intern(TyKind::ErrUnion(z))
        }
        TyKind::Range(t) => {
            let z = zonk(table, vars, t);
            table.intern(TyKind::Range(z))
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

    // ------------------------------------------------------- entry ----

    fn run(&mut self, node: &GreenNode, body: &BodyRef) -> R<()> {
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
        if sig.bounded {
            return Err(NotYet {
                construct: "a body with bounded generics (s14)",
                span: node.span,
            });
        }
        let d = FnDecl::cast(node).expect("kind");
        self.generics = sig.generics.clone();
        let because = sig.ret_span.unwrap_or(sig.name_span);
        self.ret = Some((sig.ret, body.name.clone(), because));
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

    /// `actual ⇐ exp`, with the `!T` ok-injection trial: in check mode
    /// a `T` is accepted where `!T` is expected (D30). The trial runs
    /// on a unifier snapshot — the s14/s17 speculative machinery,
    /// exercised from day one.
    fn expect_unify(&mut self, span: Span, actual: TyId, exp: &Expect) {
        let expected = self.shallow(exp.ty);
        if let TyKind::ErrUnion(inner) = self.lo.table.kind(expected).clone() {
            let snap = self.vars.snapshot();
            if unify(&mut self.lo.table, &mut self.vars, actual, expected).is_ok() {
                return;
            }
            self.vars.rollback(snap);
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
        }
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
                if let Some(e) = DeferStmt::cast(s).and_then(|d| d.expr()) {
                    self.synth_expr(e)?;
                }
                Ok(())
            }
            SyntaxKind::AssumeStmt => Err(NotYet {
                construct: "`assume` (unsafe tier, c04)",
                span: s.span,
            }),
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
            // Nested items in statement position are s14+ (a nested fn
            // is an item with a signature; wiring its resolution scope
            // into checking waits for the trait sprint's restructuring
            // of item environments).
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
        self.bind_pattern(pat, ty)?;
        Ok(())
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
                let ty = self
                    .lo
                    .lower_type(self.module, self.file, &self.generics.clone(), a);
                let exp = Expect {
                    ty,
                    reason: Reason::LetAnnotation(name.unwrap_or_else(|| "this binding".into())),
                    because: Some(a.span),
                };
                self.check_expr(i, &exp)?;
                Ok(ty)
            }
            (Some(a), None) => {
                Ok(self
                    .lo
                    .lower_type(self.module, self.file, &self.generics.clone(), a))
            }
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
            _ => Err(NotYet {
                construct: "a refutable pattern in a binding (s17)",
                span: pat.span,
            }),
        }
    }

    fn check_assign(&mut self, s: &GreenNode) -> R<()> {
        let Some(a) = AssignStmt::cast(s) else {
            return Ok(());
        };
        let Some(place) = a.place() else {
            return Ok(());
        };
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
            _ => {
                // The `!T` tag forms: a deferred (capitalized,
                // unresolved) name checks against an opaque row (D30).
                if let TyKind::ErrUnion(_) = self.kind_of(exp.ty) {
                    if let Some(span) = self.deferred_tag(e) {
                        self.record(span, exp.ty);
                        return Ok(());
                    }
                    if e.kind == SyntaxKind::CallExpr
                        && let Some(c) = CallExpr::cast(e)
                        && let Some(callee) = c.callee()
                        && self.deferred_tag(callee).is_some()
                    {
                        // Tag payloads synthesize; the row is opaque.
                        if let Some(args) = c.args() {
                            self.synth_args_loosely(args)?;
                        }
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
            // ---- outside the s13-checkable set: honest refusals ----
            SyntaxKind::TryExpr => Err(NotYet {
                construct: "`?` propagation (s15 rows)",
                span: e.span,
            }),
            SyntaxKind::ElseExpr => Err(NotYet {
                construct: "`else` defaulting (s15 rows)",
                span: e.span,
            }),
            SyntaxKind::BracketApply => Err(NotYet {
                construct: "indexing / generic application (s17)",
                span: e.span,
            }),
            SyntaxKind::FromEndExpr => Err(NotYet {
                construct: "end-relative `^n` position (s17)",
                span: e.span,
            }),
            SyntaxKind::RegionBlock
            | SyntaxKind::RegionValue
            | SyntaxKind::InBlock
            | SyntaxKind::FreezeExpr => Err(NotYet {
                construct: "region typing (c04)",
                span: e.span,
            }),
            SyntaxKind::ScopeExpr
            | SyntaxKind::SelectExpr
            | SyntaxKind::WhenExpr
            | SyntaxKind::SpawnExpr => Err(NotYet {
                construct: "concurrency typing (c05)",
                span: e.span,
            }),
            SyntaxKind::UnsafeBlock
            | SyntaxKind::InlineC
            | SyntaxKind::AsmExpr
            | SyntaxKind::BorrowExpr => Err(NotYet {
                construct: "unsafe-tier typing (c04/c10)",
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
    /// accepted at any sized primitive / `str` type; full format-spec
    /// validation (alignment, precision, argument kinds) is s16 (D26).
    fn synth_string(&mut self, e: &GreenNode) -> R<TyId> {
        let d = StringExpr::cast(e).expect("kind");
        for i in d.interps() {
            if let Some(expr) = i.expr() {
                let t = self.synth_expr(expr)?;
                self.hole_ok(expr.span, t)?;
            }
            if let Some(spec) = i.format_spec() {
                // A spec's own `{…}` holes (e.g. interpolated widths).
                for nested in spec.nodes().filter_map(wolf_ast::Interp::cast) {
                    if let Some(expr) = nested.expr() {
                        let t = self.synth_expr(expr)?;
                        self.hole_ok(expr.span, t)?;
                    }
                }
            }
        }
        Ok(self.lo.table.prim(Prim::Str))
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
    /// only against an opaque `!T` row and refuse elsewhere.
    fn deferred_tag(&self, e: &GreenNode) -> Option<Span> {
        if e.kind != SyntaxKind::PathExpr {
            return None;
        }
        let t = PathExpr::cast(e)?.ident()?;
        let name = self.text(t.span);
        if !name.chars().next().is_some_and(char::is_uppercase) {
            return None;
        }
        if self.lookup_local(&name).is_some()
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

    fn synth_path(&mut self, e: &GreenNode) -> R<TyId> {
        let Some(t) = PathExpr::cast(e).and_then(|p| p.ident()) else {
            return Ok(self.error_ty());
        };
        let name = self.text(t.span);
        if name == "self" {
            return Err(NotYet {
                construct: "`self` receivers (s17 methods)",
                span: e.span,
            });
        }
        if let Some(ty) = self.lookup_local(&name) {
            return Ok(ty);
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
            return Err(NotYet {
                construct: "a type used as a value (comptime, s16)",
                span: e.span,
            });
        }
        if self.deferred_tag(e).is_some() {
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
                        construct: "a generic function value (s14 instantiation)",
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
            Some(ItemSig::Struct(_) | ItemSig::Enum { .. } | ItemSig::Alias { .. }) => {
                Err(NotYet {
                    construct: "a type used as a value (comptime, s16)",
                    span,
                })
            }
            Some(ItemSig::Trait) => Err(NotYet {
                construct: "trait machinery (s14)",
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
                if unify(&mut self.lo.table, &mut self.vars, t, bool_).is_err() {
                    self.report_bad_operand(operand.span, "!", "`bool`", t);
                }
                Ok(bool_)
            }
            Some(SyntaxKind::Minus) => {
                let t = self.synth_expr(operand)?;
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
            Some(SyntaxKind::SharedKw) => Err(NotYet {
                construct: "`shared` allocation typing (c04)",
                span: e.span,
            }),
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

    /// `==`/`!=` compare the primitive family in s13; everything else
    /// waits for trait-based equality (s14).
    fn equatable(&mut self, span: Span, ty: TyId) -> R<()> {
        match self.kind_of(ty) {
            TyKind::Prim(_)
            | TyKind::Wrapping(_)
            | TyKind::Var(_)
            | TyKind::Error
            | TyKind::Never => Ok(()),
            _ => Err(NotYet {
                construct: "`==` on non-primitive types (s14 traits)",
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
            Some(t) => self
                .lo
                .lower_type(self.module, self.file, &self.generics.clone(), t),
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
            return Ok(target);
        }
        if src_ty == target {
            return Ok(target);
        }
        Err(NotYet {
            construct: "this `as` conversion (s17 coercions)",
            span: e.span,
        })
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

    // ---------------------------------------------------------- calls --

    fn synth_args_loosely(&mut self, args: ArgList<'_>) -> R<()> {
        for a in args.args() {
            if let Some(v) = Arg::value(a) {
                if is_type_kind(v.kind) {
                    return Err(NotYet {
                        construct: "a type-shaped argument (s14/s16 generics)",
                        span: v.span,
                    });
                }
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
                    // print/print_raw builtin signature.
                    if name == "print" || name == "print_raw" {
                        return self.call_print(&name, e, d.args());
                    }
                    if let Some((module, item)) = self.named_fn_target(&name) {
                        return self.call_named(&item, module, callee.span, e, d.args());
                    }
                    if self.deferred_tag(callee).is_some() {
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
            && let Some((module, item)) = self.namespace_member(m)
        {
            return self.call_named(&item, module, callee.span, e, d.args());
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

    fn call_named(
        &mut self,
        name: &str,
        module: usize,
        callee_span: Span,
        e: &GreenNode,
        args: Option<ArgList<'_>>,
    ) -> R<TyId> {
        match self.sigs.get(module, name).cloned() {
            Some(ItemSig::Fn(sig)) => {
                if !sig.generics.is_empty() {
                    return Err(NotYet {
                        construct: "calling a generic function (s14 instantiation)",
                        span: callee_span,
                    });
                }
                self.call_by_sig(name, &sig, e, args)
            }
            Some(ItemSig::Struct(_) | ItemSig::Enum { .. } | ItemSig::Alias { .. }) => {
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
            Some(ItemSig::Trait) => Err(NotYet {
                construct: "trait machinery (s14)",
                span: callee_span,
            }),
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
        for (i, arg) in arg_nodes.iter().enumerate() {
            let Some(v) = Arg::value(*arg) else { continue };
            if is_type_kind(v.kind) {
                return Err(NotYet {
                    construct: "a type-shaped argument (s14/s16 generics)",
                    span: v.span,
                });
            }
            match sig.params.get(i) {
                Some(p) => {
                    let exp = Expect {
                        ty: p.ty,
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
        Ok(sig.ret)
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
                for (i, arg) in arg_nodes.iter().enumerate() {
                    let Some(v) = Arg::value(*arg) else { continue };
                    if is_type_kind(v.kind) {
                        return Err(NotYet {
                            construct: "a type-shaped argument (s14/s16 generics)",
                            span: v.span,
                        });
                    }
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
                    if is_type_kind(v.kind) {
                        return Err(NotYet {
                            construct: "a type-shaped argument (s14/s16 generics)",
                            span: v.span,
                        });
                    }
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

    // ------------------------------------------------------- members ---

    fn synth_member(&mut self, e: &GreenNode) -> R<TyId> {
        let d = MemberExpr::cast(e).expect("kind");
        if let Some((module, item)) = self.namespace_member(d) {
            return self.item_value_ty(module, &item, e.span);
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
                                construct: "fields of a generic struct (s14)",
                                span: e.span,
                            });
                        }
                        let mname = self.text(member.span);
                        match s.fields.iter().find(|f| f.name == mname) {
                            Some(f) => Ok(f.ty),
                            None => {
                                self.unknown_field(&name, &s, member.span, &mname);
                                Ok(self.error_ty())
                            }
                        }
                    }
                    Some(ItemSig::Enum { .. }) => Err(NotYet {
                        construct: "enum variants and members (s17)",
                        span: e.span,
                    }),
                    _ => Err(NotYet {
                        construct: "member access on this type (s17)",
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
                        construct: "method calls (s17)",
                        span: e.span,
                    }),
                }
            }
            TyKind::Error | TyKind::Never => Ok(self.error_ty()),
            TyKind::Var(_) => Err(NotYet {
                construct: "member access on an inferred type (s17)",
                span: e.span,
            }),
            _ => Err(NotYet {
                construct: "methods/members on this type (s17)",
                span: e.span,
            }),
        }
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
                construct: "a generic struct literal (s14 inference of arguments)",
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
    /// each arm against `exp` otherwise. s13 patterns: irrefutable +
    /// literal only; exhaustiveness is s17.
    fn check_match(&mut self, e: &GreenNode, exp: Option<&Expect>) -> R<TyId> {
        let d = MatchExpr::cast(e).expect("kind");
        let scrut_ty = match d.scrutinee() {
            Some(s) => self.synth_expr(s)?,
            None => self.error_ty(),
        };
        let mut joined: Option<(TyId, Span)> = None;
        for arm in d.arms() {
            self.push_scope();
            if let Some(p) = arm.pattern() {
                self.match_pattern(p, scrut_ty)?;
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
        match exp {
            Some(exp) => Ok(exp.ty),
            None => Ok(joined
                .map(|(t, _)| t)
                .unwrap_or_else(|| self.lo.table.never())),
        }
    }

    /// s13 match patterns: wildcard, binding, literal, and tuples of
    /// those. Everything else (variants, or-patterns, ranges) is s17.
    fn match_pattern(&mut self, pat: &GreenNode, scrut: TyId) -> R<()> {
        match pat.kind {
            SyntaxKind::WildcardPat
            | SyntaxKind::IdentPat
            | SyntaxKind::BindingPat
            | SyntaxKind::TuplePat => self.bind_pattern(pat, scrut),
            SyntaxKind::LiteralPat => {
                let lit_ty = match pat.tokens().next().map(|t| t.kind) {
                    Some(SyntaxKind::Int) => self.fresh(NumKind::Integer, pat.span),
                    Some(SyntaxKind::Float) => self.fresh(NumKind::Float, pat.span),
                    Some(SyntaxKind::TrueKw | SyntaxKind::FalseKw) => {
                        self.lo.table.prim(Prim::Bool)
                    }
                    _ => self.error_ty(),
                };
                let exp = Expect {
                    ty: scrut,
                    reason: Reason::Pattern,
                    because: Some(pat.span),
                };
                self.expect_unify(pat.span, lit_ty, &exp);
                Ok(())
            }
            _ => Err(NotYet {
                construct: "this pattern form (s17 exhaustiveness/variants)",
                span: pat.span,
            }),
        }
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
                    TyKind::Error | TyKind::Never => self.error_ty(),
                    _ => {
                        return Err(NotYet {
                            construct: "the iteration protocol (s14 traits)",
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
                        let ann =
                            self.lo
                                .lower_type(self.module, self.file, &self.generics.clone(), t);
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
            TyKind::ErrUnion(inner) => {
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
                Some(t) => self
                    .lo
                    .lower_type(self.module, self.file, &self.generics.clone(), t),
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
fn single_ident(pat: &GreenNode, src: &[u8]) -> Option<String> {
    if pat.kind == SyntaxKind::IdentPat {
        let t = pat.child_token(SyntaxKind::Ident)?;
        return Some(String::from_utf8_lossy(t.text(src)).into_owned());
    }
    None
}
