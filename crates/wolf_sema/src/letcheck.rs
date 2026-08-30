//! E0410 — `let` is immutable — at the RESOLVE rung (s29, closing
//! DIV-2026-010).
//!
//! How a place was *bound* is lexical knowledge: no types are needed to
//! know that `x = 2` assigns to a `let`. The interpreter has always
//! enforced this rule on its resolve rung (its deepest static rung),
//! while wolfc emitted the same diagnostic from the type checker — same
//! code, same span, different `phase_reached`, a differential verdict
//! mismatch on every E0410 witness. The emission now lives here, in the
//! phase that owns binding structure; the type checker no longer
//! reports it.
//!
//! Scope discipline, pinned by `corpus/typecheck/let_shadow_var_ok.lu`:
//! the innermost binding of a name decides (a `var` shadowing a `let`
//! assigns fine, and vice versa fails); a second `let x = …` *shadows*
//! rather than assigns; parameters are not `let` bindings (their
//! mutability is the mode system's business); pattern bindings in
//! `match`/`for`/`else |e|` arms are not `let` bindings either.
//! Item-level `let` globals are immutable exactly like locals
//! (`[gram.item.let]` — item-level and statement-level share the
//! grammar).

use wolf_ast::{
    AssignStmt, ClosureExpr, ElseExpr, FnDecl, ForExpr, GreenNode, LetDecl, MatchArm, MatchExpr,
    ParenExpr, PathExpr, SyntaxKind, VarDecl,
};
use wolf_diag::{Applicability, Diagnostic, Diagnostics, Suggestion, codes};
use wolf_span::Span;

use crate::graph::{Package, pattern_names};

/// One lexical binding the walk knows about: its name and, when it was
/// bound with `let`, the keyword's span (E0410's secondary label and
/// fix-it anchor). `None` means assignable as far as this rule cares.
type Bind = (String, Option<Span>);

/// Check every function body of one file against the `let`-immutability
/// rule, reporting into `sink`. `globals` are the module's item-level
/// `let`/`var` bindings (visible in every body of the module).
pub(crate) fn check_file(pkg: &Package, file: usize, globals: &[Bind], sink: &mut Diagnostics) {
    let src = &pkg.files[file].raw.src;
    let root = &pkg.files[file].parse.root;
    let mut walk = Walk {
        src,
        scopes: vec![globals.to_vec()],
        sink,
    };
    for item in root.nodes().filter(|n| n.kind.is_item()) {
        walk.item(item);
    }
}

/// The module's item-level `let`/`var` bindings, across all its files.
pub(crate) fn module_globals(pkg: &Package, module: usize) -> Vec<Bind> {
    let mut out = Vec::new();
    for &fi in &pkg.modules[module].files {
        let src = &pkg.files[fi].raw.src;
        for item in pkg.files[fi].parse.root.nodes() {
            let (binders, let_kw) = match item.kind {
                SyntaxKind::LetDecl => (
                    LetDecl::cast(item).map(|d| d.binders()).unwrap_or_default(),
                    item.tokens()
                        .find(|t| t.kind == SyntaxKind::LetKw)
                        .map(|t| t.span),
                ),
                SyntaxKind::VarDecl => (
                    VarDecl::cast(item).map(|d| d.binders()).unwrap_or_default(),
                    None,
                ),
                _ => continue,
            };
            for pat in binders.iter().filter_map(|b| b.pattern) {
                for (name, _) in pattern_names(pat, src) {
                    out.push((name, let_kw));
                }
            }
        }
    }
    out
}

struct Walk<'a> {
    src: &'a [u8],
    /// Innermost scope last; within a scope, the latest entry wins
    /// (shadowing inside one block).
    scopes: Vec<Vec<Bind>>,
    sink: &'a mut Diagnostics,
}

impl Walk<'_> {
    fn text(&self, span: Span) -> String {
        String::from_utf8_lossy(&self.src[span.lo as usize..span.hi as usize]).into_owned()
    }

    fn declare(&mut self, name: String, let_kw: Option<Span>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push((name, let_kw));
        }
    }

    fn declare_pattern(&mut self, pat: &GreenNode, let_kw: Option<Span>) {
        for (name, _) in pattern_names(pat, self.src) {
            self.declare(name, let_kw);
        }
    }

    /// The innermost binding's `let` provenance; `None` when the name
    /// is unbound or bound assignably.
    fn let_provenance(&self, name: &str) -> Option<Span> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.iter().rev().find(|(n, _)| n == name))
            .and_then(|(_, kw)| *kw)
    }

    /// Items: only function bodies execute assignments. Trait/impl
    /// members carry nested fns; other items hold no statements.
    fn item(&mut self, node: &GreenNode) {
        match node.kind {
            SyntaxKind::FnDecl => self.func(node),
            SyntaxKind::TraitDecl | SyntaxKind::ImplDecl => {
                for m in node.nodes().filter(|n| n.kind == SyntaxKind::FnDecl) {
                    self.func(m);
                }
            }
            _ => {}
        }
    }

    /// A function: params are assignable (a parameter is not a `let`
    /// binding); the body is one scope below them. Bodies see the
    /// globals (the walk's base scope) but never an enclosing fn's
    /// locals — a fresh wall for nested fns.
    fn func(&mut self, node: &GreenNode) {
        let Some(d) = FnDecl::cast(node) else { return };
        let Some(body) = d.body() else { return };
        let base = self.scopes.first().cloned().unwrap_or_default();
        let mut inner = Walk {
            src: self.src,
            scopes: vec![base, Vec::new()],
            sink: self.sink,
        };
        if let Some(params) = d.params() {
            for p in params.params() {
                if p.is_self() {
                    inner.declare("self".to_string(), None);
                } else if let Some(t) = p.name() {
                    let name =
                        String::from_utf8_lossy(&inner.src[t.span.lo as usize..t.span.hi as usize])
                            .into_owned();
                    inner.declare(name, None);
                }
            }
        }
        inner.expr(body.syntax());
    }

    /// One statement inside a block, in order.
    fn stmt(&mut self, s: &GreenNode) {
        match s.kind {
            SyntaxKind::LetDecl => {
                let let_kw = s
                    .tokens()
                    .find(|t| t.kind == SyntaxKind::LetKw)
                    .map(|t| t.span);
                if let Some(d) = LetDecl::cast(s) {
                    // Binders in order (D63): each initializer may read
                    // the names declared before it.
                    for b in d.binders() {
                        if let Some(init) = b.init {
                            self.expr(init);
                        }
                        if let Some(pat) = b.pattern {
                            self.declare_pattern(pat, let_kw);
                        }
                    }
                }
            }
            SyntaxKind::VarDecl => {
                if let Some(d) = VarDecl::cast(s) {
                    for b in d.binders() {
                        if let Some(init) = b.init {
                            self.expr(init);
                        }
                        if let Some(pat) = b.pattern {
                            self.declare_pattern(pat, None);
                        }
                    }
                }
            }
            SyntaxKind::ConstDecl => {
                // A `const` is not a `let`: assignment to it is a
                // different wrong (typing/place rules), not this one.
                for c in s.nodes() {
                    self.expr(c);
                }
                if let Some(t) = s.child_token(SyntaxKind::Ident) {
                    self.declare(self.text(t.span), None);
                }
            }
            SyntaxKind::AssignStmt => {
                let a = AssignStmt::cast(s);
                if let Some(v) = a.and_then(|a| a.value()) {
                    self.expr(v);
                }
                if let Some(place) = a.and_then(|a| a.place()) {
                    self.expr(place);
                    self.check_place(place);
                }
            }
            SyntaxKind::FnDecl => self.func(s),
            _ => self.expr(s),
        }
    }

    /// E0410 proper: the place of an assignment must not be a
    /// `let`-bound name. Fields, elements, and everything reached
    /// *through* a binding stay c04's exclusivity/freeze territory.
    fn check_place(&mut self, place: &GreenNode) {
        // Peel parens: `(x) = 2` assigns to `x`.
        let mut root = place;
        while root.kind == SyntaxKind::ParenExpr {
            match ParenExpr::cast(root).and_then(|p| p.expr()) {
                Some(inner) => root = inner,
                None => return,
            }
        }
        if root.kind != SyntaxKind::PathExpr {
            return;
        }
        let Some(t) = PathExpr::cast(root).and_then(|p| p.ident()) else {
            return;
        };
        let name = self.text(t.span);
        let Some(kw) = self.let_provenance(&name) else {
            return;
        };
        self.sink.push(
            Diagnostic::error(
                codes::E0410,
                root.span,
                format!("`{name}` is bound with `let`, so it cannot be assigned again"),
            )
            .with_label("this assignment needs a mutable binding")
            .with_secondary(kw, "the binding is made immutable here")
            .with_note(
                "`let` names a value once. Declare the binding with `var` \
                 to update it in place, or shadow it with a second \
                 `let` if the next value is really a new thing.",
            )
            .with_suggestion(Suggestion::new(
                "make the binding mutable: `var`",
                vec![(kw, "var".to_string())],
                Applicability::Maybe,
            )),
        );
    }

    /// Expressions, generically: scope-forming shapes get their scopes;
    /// everything else recurses in child order.
    fn expr(&mut self, e: &GreenNode) {
        match e.kind {
            SyntaxKind::Block => {
                self.scopes.push(Vec::new());
                for child in e.nodes() {
                    self.stmt(child);
                }
                self.scopes.pop();
            }
            SyntaxKind::ForExpr => {
                let d = ForExpr::cast(e);
                if let Some(iter) = d.and_then(|d| d.iterable()) {
                    self.expr(iter);
                }
                self.scopes.push(Vec::new());
                if let Some(pat) = d.and_then(|d| d.pattern()) {
                    self.declare_pattern(pat, None);
                }
                if let Some(body) = d.and_then(|d| d.body()) {
                    self.expr(body.syntax());
                }
                self.scopes.pop();
            }
            SyntaxKind::MatchExpr => {
                let d = MatchExpr::cast(e);
                if let Some(s) = d.and_then(|d| d.scrutinee()) {
                    self.expr(s);
                }
                for arm in e.nodes().filter_map(MatchArm::cast) {
                    self.scopes.push(Vec::new());
                    if let Some(pat) = arm.pattern() {
                        self.declare_pattern(pat, None);
                    }
                    if let Some(g) = arm.guard() {
                        self.expr(g);
                    }
                    if let Some(b) = arm.body() {
                        self.expr(b);
                    }
                    self.scopes.pop();
                }
            }
            SyntaxKind::ElseExpr => {
                let d = ElseExpr::cast(e);
                if let Some(s) = d.and_then(|d| d.scrutinized()) {
                    self.expr(s);
                }
                self.scopes.push(Vec::new());
                if let Some(pat) = d.and_then(|d| d.handler_pattern()) {
                    self.declare_pattern(pat, None);
                }
                if let Some(f) = d.and_then(|d| d.fallback()) {
                    self.expr(f);
                }
                self.scopes.pop();
            }
            SyntaxKind::WhenExpr => {
                // A `when` body addresses its operands through the
                // acquired cells (`[conc.when.body]`): `when (a, b)
                // { a += 10 }` assigns through the cell, not to the
                // binding — the operand names are assignable inside
                // the body whatever introduced them
                // (`corpus/conc/when_multi.lu` pins this).
                let d = wolf_ast::WhenExpr::cast(e);
                let mut names = Vec::new();
                if let Some(d) = d {
                    for op in d.operands() {
                        self.expr(op);
                        let mut place = op;
                        while place.kind == SyntaxKind::ParenExpr {
                            match ParenExpr::cast(place).and_then(|p| p.expr()) {
                                Some(inner) => place = inner,
                                None => break,
                            }
                        }
                        if place.kind == SyntaxKind::PathExpr
                            && let Some(t) = PathExpr::cast(place).and_then(|p| p.ident())
                        {
                            names.push(self.text(t.span));
                        }
                    }
                }
                self.scopes.push(Vec::new());
                for n in names {
                    self.declare(n, None);
                }
                if let Some(body) = d.and_then(|d| d.body()) {
                    self.expr(body.syntax());
                }
                self.scopes.pop();
            }
            SyntaxKind::ClosureExpr => {
                let d = ClosureExpr::cast(e);
                self.scopes.push(Vec::new());
                if let Some(params) = d.and_then(|d| d.params()) {
                    for p in params.params() {
                        if let Some(t) = p.name() {
                            self.declare(self.text(t.span), None);
                        }
                    }
                }
                if let Some(body) = d.and_then(|d| d.body()) {
                    self.expr(body);
                }
                self.scopes.pop();
            }
            SyntaxKind::FnDecl => self.func(e),
            // Patterns bind nothing assignable-through and hold no
            // assignments; skip them wholesale.
            k if wolf_ast::is_pattern_kind(k) => {}
            _ => {
                for child in e.nodes() {
                    self.expr(child);
                }
            }
        }
    }
}

// The unit tests live with the resolve suite
// (`tests/diagnostics.rs::e0410_*`) and the corpus witnesses
// (`corpus/typecheck/let_reassign.lu`, `let_compound_assign.lu`,
// `let_shadow_var_ok.lu`).
