//! The lived lint corpus (s68) — the first wave's analysis passes.
//!
//! Every lint here is mined from a recorded scar: a case where wolf
//! let someone write something legal that was wrong or confusing. The
//! triage table in `docs/lint-triage.md` cites each one; the corpus
//! fixtures under `corpus/lints/` witness each one. Two passes live in
//! this module:
//!
//! - [`check_file`] — the **resolve-rung** pass (syntax + lexical
//!   binding structure, letcheck-style): W0304 prelude shadowing,
//!   W0305 row-tag collisions, W0306 bare prefix-operator statements,
//!   W0307 comparisons binding to an `else` fallback, W0308 `mut`
//!   arguments inside interpolations, W0309 interpolation-shaped
//!   braces in raw literals, W0602 anonymous `pub` rows, W1101 writes
//!   to captured copies inside `spawn` closures, W1102 captures of
//!   bindings assigned after closure creation, W1302 reassigned
//!   `assume noalias` operands. These need no types, so an
//!   implementation with resolution alone can carry them
//!   (shared-analysis per the conformance posture).
//! - [`check_typed_body`] — the **typecheck-rung** pass over a
//!   [`TypedBody`]: W0601 discarded fallible results, W0401 literals
//!   outside a cast target's range, W0402 `0.0 - x` negation.
//!   (W0801, the capitalized-binder pattern lint, lives in the
//!   checker itself, where pattern resolution decides bind-vs-test.)
//!
//! Cost discipline (D5): each pass is one walk over trees the phase
//! already holds, allocation-light, and firing nothing on clean code.

use std::collections::BTreeSet;

use wolf_ast::{
    Arg, AssignStmt, AssumeStmt, Block, CallExpr, CastExpr, ClosureExpr, ConstDecl, ElseExpr,
    EnumDecl, ExprStmt, FnDecl, GreenNode, GreenToken, LetDecl, MemberExpr, ParamMode, RowEntry,
    StructDecl, SyntaxKind, TraitDecl, TypeDecl, VarDecl,
};
use wolf_diag::{Applicability, Diagnostic, Diagnostics, Suggestion, codes};
use wolf_span::Span;

use crate::check::{BodyRef, TypedBody};
use crate::graph::{Package, pattern_names};
use crate::prelude;
use crate::types::{Prim, TyKind, render};

// ===================================================== resolve rung ==

/// Run the resolve-rung wave over one file of `module`, reporting into
/// `sink` (cascade suppression applies: warnings inside parse-error
/// regions are dropped like every other resolve diagnostic).
pub(crate) fn check_file(pkg: &Package, module: usize, file: usize, sink: &mut Diagnostics) {
    // The std tree is exempt wholesale: its wrappers deliberately
    // shadow the builtin spellings they delegate to, and a user
    // cannot act on a warning inside a library file.
    if pkg.modules[module].path.first().is_some_and(|s| s == "std") {
        return;
    }
    let src = &pkg.files[file].raw.src;
    let root = &pkg.files[file].parse.root;
    let mut w = Wave {
        pkg,
        module,
        src,
        sink,
        scopes: vec![Vec::new()],
        closures: Vec::new(),
        assumes: Vec::new(),
        interp_depth: 0,
    };
    for item in root.nodes().filter(|n| n.kind.is_item()) {
        w.item(item);
    }
}

/// One lexical binding: name, declaration span, declared with `var`.
type Bind = (String, Span, bool);

/// A closure's capture record: where the closure sits, and every
/// enclosing binding it references (name + the binding's decl span).
struct Capture {
    span: Span,
    frees: Vec<(String, Span)>,
}

struct Wave<'a> {
    pkg: &'a Package,
    module: usize,
    src: &'a [u8],
    sink: &'a mut Diagnostics,
    /// Innermost scope last; per-function stacks reset at `func`.
    scopes: Vec<Vec<Bind>>,
    /// Closures seen so far in the current function (W1102).
    closures: Vec<Capture>,
    /// `assume noalias` operands in the current function: operand
    /// name, its span, the whole statement's span (W1302).
    assumes: Vec<(String, Span, Span)>,
    /// How many `Interp` nodes enclose the walk (W0308).
    interp_depth: u32,
}

impl Wave<'_> {
    fn text(&self, span: Span) -> String {
        String::from_utf8_lossy(&self.src[span.lo as usize..span.hi as usize]).into_owned()
    }

    fn warn(&mut self, d: Diagnostic) {
        self.sink.push(d);
    }

    // ------------------------------------------------------ items --

    fn item(&mut self, node: &GreenNode) {
        match node.kind {
            SyntaxKind::FnDecl => self.func(node, /*free*/ true),
            SyntaxKind::TraitDecl | SyntaxKind::ImplDecl => {
                if node.kind == SyntaxKind::TraitDecl
                    && let Some(t) = TraitDecl::cast(node).and_then(|d| d.name())
                {
                    self.shadow_check(t.span, "trait");
                }
                // Method names are not ambient (they resolve through a
                // receiver), so members skip the shadow check.
                for m in node.nodes().filter(|n| n.kind == SyntaxKind::FnDecl) {
                    self.func(m, /*free*/ false);
                }
            }
            SyntaxKind::StructDecl => {
                if let Some(t) = StructDecl::cast(node).and_then(|d| d.name()) {
                    self.shadow_check(t.span, "struct");
                }
            }
            SyntaxKind::EnumDecl => {
                if let Some(t) = EnumDecl::cast(node).and_then(|d| d.name()) {
                    self.shadow_check(t.span, "enum");
                }
            }
            SyntaxKind::TypeDecl => {
                if let Some(t) = TypeDecl::cast(node).and_then(|d| d.name()) {
                    self.shadow_check(t.span, "type alias");
                }
            }
            SyntaxKind::LetDecl | SyntaxKind::VarDecl | SyntaxKind::ConstDecl => {
                self.binding_decl(node, /*declare*/ false);
            }
            _ => {}
        }
    }

    /// W0304 at a declared name's span.
    fn shadow_check(&mut self, span: Span, what: &str) {
        let name = self.text(span);
        if !prelude::shadow_hazard(&name) {
            return;
        }
        let kind = if prelude::is_builtin_type(&name) {
            "built-in type"
        } else {
            "prelude"
        };
        self.warn(
            Diagnostic::warning(
                codes::W0304,
                span,
                format!("this {what} shadows the {kind} name `{name}`"),
            )
            .with_label("shadows an ambient name")
            .with_note(format!(
                "`{name}` resolves without an import in every file; this declaration \
                 silently wins over it for the whole module (file boundaries create no \
                 scopes). Pick another name so both stay reachable."
            )),
        );
    }

    /// A `let`/`var`/`const` at any level: W0304 on its names, and
    /// (when `declare`) push the bindings for the capture lints.
    fn binding_decl(&mut self, node: &GreenNode, declare: bool) {
        let (pat, is_var, what) = match node.kind {
            SyntaxKind::LetDecl => (
                LetDecl::cast(node).and_then(|d| d.pattern()),
                false,
                "`let`",
            ),
            SyntaxKind::VarDecl => (VarDecl::cast(node).and_then(|d| d.pattern()), true, "`var`"),
            SyntaxKind::ConstDecl => {
                if let Some(t) = ConstDecl::cast(node).and_then(|d| d.name()) {
                    self.shadow_check(t.span, "`const` binding");
                    let name = self.text(t.span);
                    if declare && let Some(scope) = self.scopes.last_mut() {
                        scope.push((name, t.span, false));
                    }
                }
                return;
            }
            _ => return,
        };
        if let Some(pat) = pat {
            for (name, span) in pattern_names(pat, self.src) {
                if prelude::shadow_hazard(&name) {
                    let kind = if prelude::is_builtin_type(&name) {
                        "built-in type"
                    } else {
                        "prelude"
                    };
                    self.warn(
                        Diagnostic::warning(
                            codes::W0304,
                            span,
                            format!("this {what} binding shadows the {kind} name `{name}`"),
                        )
                        .with_label("shadows an ambient name")
                        .with_note(format!(
                            "every later use of `{name}` in this scope means the binding, \
                             not the ambient name — silently. Rename the binding."
                        )),
                    );
                }
                if declare && let Some(scope) = self.scopes.last_mut() {
                    scope.push((name, span, is_var));
                }
            }
        }
    }

    // -------------------------------------------------- functions --

    fn func(&mut self, node: &GreenNode, free: bool) {
        let Some(d) = FnDecl::cast(node) else { return };
        if free {
            if let Some(t) = d.name() {
                self.shadow_check(t.span, "function");
            }
            self.pub_row_check(&d);
        }
        self.row_collision_check(node, &d);
        // Fresh per-function lexical state.
        let saved_closures = std::mem::take(&mut self.closures);
        let saved_assumes = std::mem::take(&mut self.assumes);
        self.scopes.push(Vec::new());
        if let Some(params) = d.params() {
            for p in params.params() {
                if let Some(t) = p.name() {
                    let name = self.text(t.span);
                    if free {
                        self.shadow_check(t.span, "parameter");
                    }
                    if let Some(scope) = self.scopes.last_mut() {
                        scope.push((name, t.span, false));
                    }
                }
            }
        }
        if let Some(body) = d.body() {
            self.block(body.syntax());
        }
        self.scopes.pop();
        self.closures = saved_closures;
        self.assumes = saved_assumes;
    }

    /// W0305 — a declared row tag that shares its name with a module
    /// item, an import, or a prelude name.
    fn row_collision_check(&mut self, node: &GreenNode, d: &FnDecl<'_>) {
        // Every row entry in the signature (return row; param rows).
        let mut entries: Vec<(String, Span)> = Vec::new();
        collect_row_entries(
            node,
            d.body().map(|b| b.syntax().span),
            self.src,
            &mut entries,
        );
        for (tag, span) in entries {
            let clash = if self.pkg.tables[self.module]
                .get(&tag)
                .is_some_and(|it| self.text(span) == it.name)
            {
                Some("an item of this module")
            } else if self.pkg.modules[self.module]
                .bindings
                .iter()
                .flatten()
                .any(|b| b.name == tag)
            {
                Some("an import")
            } else if prelude::in_prelude(&tag) {
                Some("a prelude name")
            } else {
                None
            };
            if let Some(what) = clash {
                self.warn(
                    Diagnostic::warning(
                        codes::W0305,
                        span,
                        format!("the row tag `{tag}` shares its name with {what}"),
                    )
                    .with_label("one word, two meanings")
                    .with_note(
                        "in raise position this word is the tag; everywhere else it is \
                         the thing it collides with — programs with this collision have \
                         returned a module as a value. Rename the tag."
                            .to_string(),
                    ),
                );
            }
        }
    }

    /// W0602 — an exported signature spelling a multi-tag row inline.
    fn pub_row_check(&mut self, d: &FnDecl<'_>) {
        if d.visibility().is_none() {
            return;
        }
        let Some(row) = d.ret_ty().and_then(|r| r.error_row()) else {
            return;
        };
        let tags: Vec<String> = row
            .entries()
            .filter_map(|e| e.path().map(|p| self.text(p.syntax().span)))
            .collect();
        if tags.len() < 2 {
            return; // a single-tag row is not worth naming
        }
        self.warn(
            Diagnostic::warning(
                codes::W0602,
                row.syntax().span,
                format!(
                    "this `pub` signature spells a {}-tag error row anonymously",
                    tags.len()
                ),
            )
            .with_label("this exact tag set is now API")
            .with_note(
                "every caller depends on this row, and every later tag is a visible \
                 change at every boundary; keep the row small — one tag per failure a \
                 caller can act on — and reuse one spelling everywhere it flows."
                    .to_string(),
            ),
        );
    }

    // ------------------------------------------------- statements --

    fn block(&mut self, node: &GreenNode) {
        self.scopes.push(Vec::new());
        let stmts: Vec<&GreenNode> = node
            .nodes()
            .filter(|n| wolf_ast::is_stmt_kind(n.kind))
            .collect();
        let last = stmts.len().saturating_sub(1);
        for (i, s) in stmts.iter().enumerate() {
            match s.kind {
                SyntaxKind::ExprStmt => {
                    // W0306 — a bare prefix-operator statement (the
                    // broken-continuation shape); the block's trailing
                    // value position is exempt.
                    if i != last
                        && let Some(e) = ExprStmt::cast(s).and_then(|x| x.expr())
                        && e.kind == SyntaxKind::PrefixExpr
                        && let Some(op) = e.tokens().next()
                        && matches!(
                            op.kind,
                            SyntaxKind::Minus
                                | SyntaxKind::Not
                                | SyntaxKind::Amp
                                | SyntaxKind::Star
                        )
                    {
                        let opt = self.text(op.span);
                        self.warn(
                            Diagnostic::warning(
                                codes::W0306,
                                s.span,
                                format!("this statement is only `{opt}` applied to a value"),
                            )
                            .with_label("computes a value and discards it")
                            .with_note(
                                "if this was meant to continue the line above, join the \
                                 lines — a newline ended that statement; standalone, the \
                                 result goes nowhere."
                                    .to_string(),
                            ),
                        );
                    }
                    self.expr_children(s);
                }
                SyntaxKind::AssignStmt => {
                    self.assign(s);
                    self.expr_children(s);
                }
                SyntaxKind::AssumeStmt => {
                    if let Some(a) = AssumeStmt::cast(s) {
                        for op in a.exprs() {
                            if op.kind == SyntaxKind::PathExpr {
                                let name = self.text(op.span);
                                self.assumes.push((name, op.span, s.span));
                            }
                        }
                    }
                }
                SyntaxKind::LetDecl | SyntaxKind::VarDecl | SyntaxKind::ConstDecl => {
                    self.expr_children(s);
                    self.binding_decl(s, /*declare*/ true);
                }
                _ => self.expr_children(s),
            }
        }
        self.scopes.pop();
    }

    /// W1102 (assignment after a capture) and W1302 (assignment to an
    /// `assume noalias` operand), then normal descent.
    fn assign(&mut self, s: &GreenNode) {
        let Some(a) = AssignStmt::cast(s) else { return };
        let Some(place) = a.place() else { return };
        // Whole-name assignment only: `p[0] = …` / `p.f = …` write
        // *through* the binding, which is a different fact.
        if place.kind != SyntaxKind::PathExpr {
            return;
        }
        let Some(root) = place_root(place, self.src) else {
            return;
        };
        // W1302 — the assumption's operand no longer holds the pointer
        // it asserted about.
        if let Some((_, _, assume_span)) = self
            .assumes
            .iter()
            .find(|(n, _, a)| *n == root.0 && a.hi <= s.span.lo)
            .cloned()
        {
            self.warn(
                Diagnostic::warning(
                    codes::W1302,
                    s.span,
                    format!(
                        "`{}` was an `assume noalias` operand, and this reassigns it",
                        root.0
                    ),
                )
                .with_label("the assumption no longer describes this name")
                .with_secondary(assume_span, "the assumption was stated here")
                .with_note(
                    "the no-alias license was granted for the pointer this name held \
                     then; state the assumption after the last assignment of its \
                     operands."
                        .to_string(),
                ),
            );
        }
        // W1102 — a closure created earlier captured this binding.
        if let Some((_, decl_span, is_var)) = self.lookup(&root.0)
            && is_var
        {
            let captured: Vec<Span> = self
                .closures
                .iter()
                .filter(|c| {
                    c.span.hi <= s.span.lo
                        && c.frees.iter().any(|(n, d)| *n == root.0 && *d == decl_span)
                })
                .map(|c| c.span)
                .collect();
            if let Some(&closure_span) = captured.first() {
                self.warn(
                    Diagnostic::warning(
                        codes::W1102,
                        s.span,
                        format!(
                            "the closure above captured `{}` by value, so it will not \
                             see this assignment",
                            root.0
                        ),
                    )
                    .with_label("invisible to the closure")
                    .with_secondary(closure_span, "captured by value when this was created")
                    .with_note(
                        "closures copy their captures at creation; create the closure \
                         after the last assignment, or pass the value as a call \
                         argument instead."
                            .to_string(),
                    ),
                );
            }
        }
    }

    fn lookup(&self, name: &str) -> Option<(String, Span, bool)> {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| s.iter().rev().find(|(n, _, _)| n == name))
            .cloned()
    }

    // ----------------------------------------------- expressions --

    /// Walk every child expression of `node` for the expression-level
    /// lints, descending into nested blocks via `block`.
    fn expr_children(&mut self, node: &GreenNode) {
        for child in node.nodes() {
            self.expr(child);
        }
    }

    fn expr(&mut self, e: &GreenNode) {
        match e.kind {
            SyntaxKind::Block => {
                self.block(e);
                return;
            }
            SyntaxKind::ClosureExpr => {
                self.closure(e);
                return;
            }
            SyntaxKind::ElseExpr => self.else_chain(e),
            SyntaxKind::Interp => {
                self.interp_depth += 1;
                self.expr_children(e);
                self.interp_depth -= 1;
                return;
            }
            SyntaxKind::Arg => self.mut_arg_in_interp(e),
            SyntaxKind::StringExpr => self.raw_braces(e),
            SyntaxKind::CallExpr => self.spawn_call(e),
            SyntaxKind::LetDecl | SyntaxKind::VarDecl | SyntaxKind::ConstDecl => {
                // A declaration in expression context (match arms,
                // if-bodies arrive via Block; this is belt and braces).
                self.binding_decl(e, true);
            }
            _ => {}
        }
        self.expr_children(e);
    }

    /// W0307 — a comparison operator in fallback position.
    fn else_chain(&mut self, e: &GreenNode) {
        let Some(x) = ElseExpr::cast(e) else { return };
        let Some(fb) = x.fallback() else { return };
        if fb.kind != SyntaxKind::BinExpr {
            return;
        }
        let Some(op) = fb.tokens().find(|t| {
            matches!(
                t.kind,
                SyntaxKind::EqEq
                    | SyntaxKind::NotEq
                    | SyntaxKind::Lt
                    | SyntaxKind::Gt
                    | SyntaxKind::LtEq
                    | SyntaxKind::GtEq
                    | SyntaxKind::Spaceship
            )
        }) else {
            return;
        };
        let opt = self.text(op.span);
        // The probable intent: default first, compare second.
        let lhs_hi = fb.nodes().next().map(|n| n.span.hi).unwrap_or(op.span.lo);
        let sugg = Suggestion::new(
            "to default first and compare second, group the `else`",
            vec![
                (
                    Span::new(e.span.file, e.span.lo, e.span.lo),
                    "(".to_string(),
                ),
                (Span::new(e.span.file, lhs_hi, lhs_hi), ")".to_string()),
            ],
            Applicability::Maybe,
        );
        self.warn(
            Diagnostic::warning(
                codes::W0307,
                op.span,
                format!("this `{opt}` applies to the fallback only"),
            )
            .with_label("compared before the `else` defaults")
            .with_note(
                "`else` binds loosest, so the fallback is the whole operator chain \
                 after it; parenthesize the reading you mean."
                    .to_string(),
            )
            .with_suggestion(sugg),
        );
    }

    /// W0308 — `{f(mut x)}` inside a string.
    fn mut_arg_in_interp(&mut self, e: &GreenNode) {
        if self.interp_depth == 0 {
            return;
        }
        if Arg::cast(e).is_some_and(|a| a.mode() == Some(ParamMode::Mut)) {
            self.warn(
                Diagnostic::warning(
                    codes::W0308,
                    e.span,
                    "this interpolation mutates its argument".to_string(),
                )
                .with_label("a write, hidden inside formatting")
                .with_note(
                    "interpolations read as pure formatting; hoist the call onto \
                     its own line and interpolate the result."
                        .to_string(),
                ),
            );
        }
    }

    /// W0309 — `r"{who}"`.
    fn raw_braces(&mut self, e: &GreenNode) {
        let Some(open) = e.tokens().find(|t| t.kind == SyntaxKind::StrBegin) else {
            return;
        };
        let open_text = self.text(open.span);
        if !(open_text.starts_with("r\"") || open_text.starts_with("r#")) {
            return;
        }
        for frag in e.tokens().filter(|t| t.kind == SyntaxKind::StrFragment) {
            let text = self.text(frag.span);
            if let Some((lo, hi, name)) = interp_shaped(&text) {
                let span = Span::new(
                    frag.span.file,
                    frag.span.lo + lo as u32,
                    frag.span.lo + hi as u32,
                );
                self.warn(
                    Diagnostic::warning(
                        codes::W0309,
                        span,
                        format!("`{{{name}}}` in a raw literal is just braces and letters"),
                    )
                    .with_label("raw literals interpolate nothing")
                    .with_note(
                        "drop the `r` prefix to interpolate, or keep it if these exact \
                         bytes are wanted — the two readings are one keystroke apart."
                            .to_string(),
                    ),
                );
                return; // one report per literal is enough
            }
        }
    }

    /// W1101 — `s.spawn(fn() { captured = … })`.
    fn spawn_call(&mut self, e: &GreenNode) {
        let Some(call) = CallExpr::cast(e) else {
            return;
        };
        let is_spawn = call
            .callee()
            .and_then(MemberExpr::cast)
            .and_then(|m| m.member())
            .is_some_and(|t| self.text(t.span) == "spawn");
        if !is_spawn {
            return;
        }
        let Some(args) = call.args() else { return };
        for arg in args.args() {
            let Some(v) = arg.value() else { continue };
            if v.kind != SyntaxKind::ClosureExpr {
                continue;
            }
            let Some(cl) = ClosureExpr::cast(v) else {
                continue;
            };
            let locals = closure_locals(&cl, self.src);
            // `when (a, b) { a += … }` bodies are the sync-type
            // exception ([conc.when.body]): the set is acquired, so
            // writes inside are synchronized, not lost.
            let when_bodies: Vec<Span> = descendants(v)
                .filter(|n| n.kind == SyntaxKind::WhenExpr)
                .map(|n| n.span)
                .collect();
            for a in descendants(v).filter_map(AssignStmt::cast) {
                let sp = a.syntax().span;
                if when_bodies.iter().any(|w| sp.lo >= w.lo && sp.hi <= w.hi) {
                    continue;
                }
                let Some(place) = a.place() else { continue };
                if place.kind != SyntaxKind::PathExpr {
                    continue; // writes through a projection: E1101's turf
                }
                let Some((name, span)) = place_root(place, self.src) else {
                    continue;
                };
                if locals.contains(&name) || self.lookup(&name).is_none() {
                    // Closure-local, or not an enclosing-function
                    // binding at all (module state is not a capture).
                    continue;
                }
                // A write to something the task captured: task-local.
                self.warn(
                    Diagnostic::warning(
                        codes::W1101,
                        span,
                        format!("this write to `{name}` stays inside the task"),
                    )
                    .with_label("lands on the task's own copy")
                    .with_secondary(v.span, "the closure captured it at spawn")
                    .with_note(
                        "task captures copy (or move); the enclosing binding never \
                         sees this assignment. Send the result over a channel, or \
                         return it through the scope's join."
                            .to_string(),
                    ),
                );
            }
        }
    }

    /// A closure for W1102: record its free variables, then walk its
    /// body with the closure's own scope in place.
    fn closure(&mut self, e: &GreenNode) {
        let Some(cl) = ClosureExpr::cast(e) else {
            return;
        };
        let locals = closure_locals(&cl, self.src);
        let mut frees: Vec<(String, Span)> = Vec::new();
        for p in descendants(e).filter(|n| n.kind == SyntaxKind::PathExpr) {
            let name = self.text(p.span);
            if locals.contains(&name) {
                continue;
            }
            if let Some((n, decl, is_var)) = self.lookup(&name)
                && is_var
                && !frees.iter().any(|(fname, fd)| *fname == n && *fd == decl)
            {
                frees.push((n, decl));
            }
        }
        if !frees.is_empty() {
            self.closures.push(Capture {
                span: e.span,
                frees,
            });
        }
        // Walk the body for nested lints (fresh scope; params only —
        // precise closure-local scoping is the checker's job, and the
        // wave only needs names not to leak outward).
        self.scopes.push(Vec::new());
        if let Some(scope) = self.scopes.last_mut() {
            for name in &locals {
                scope.push((name.clone(), e.span, false));
            }
        }
        if let Some(body) = cl.body() {
            self.expr(body);
        }
        self.scopes.pop();
    }
}

/// Names bound inside a closure: parameters plus every `let`/`var`
/// pattern in its body (flat approximation — good enough to decide
/// "captured vs local" for a warning).
fn closure_locals(cl: &ClosureExpr<'_>, src: &[u8]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let Some(params) = cl.params() {
        for p in params.params() {
            if let Some(t) = p.name() {
                out.insert(text_of(src, t));
            }
        }
    }
    if let Some(body) = cl.body() {
        for d in descendants(body).filter(|n| {
            matches!(
                n.kind,
                SyntaxKind::LetDecl | SyntaxKind::VarDecl | SyntaxKind::ConstDecl
            )
        }) {
            let pat = match d.kind {
                SyntaxKind::LetDecl => LetDecl::cast(d).and_then(|x| x.pattern()),
                SyntaxKind::VarDecl => VarDecl::cast(d).and_then(|x| x.pattern()),
                _ => {
                    if let Some(t) = ConstDecl::cast(d).and_then(|x| x.name()) {
                        out.insert(text_of(src, t));
                    }
                    None
                }
            };
            if let Some(pat) = pat {
                for (name, _) in pattern_names(pat, src) {
                    out.insert(name);
                }
            }
        }
        // `for` patterns and match-arm binders count as locals too.
        for p in descendants(body)
            .filter(|n| matches!(n.kind, SyntaxKind::IdentPat | SyntaxKind::BindingPat))
        {
            for (name, _) in pattern_names(p, src) {
                out.insert(name);
            }
        }
    }
    out
}

fn text_of(src: &[u8], t: &GreenToken) -> String {
    String::from_utf8_lossy(&src[t.span.lo as usize..t.span.hi as usize]).into_owned()
}

/// The leftmost identifier of a place expression (`a`, `a.b`, `a[i]`).
fn place_root(mut e: &GreenNode, src: &[u8]) -> Option<(String, Span)> {
    loop {
        match e.kind {
            SyntaxKind::PathExpr => {
                let t = e.tokens().find(|t| t.kind == SyntaxKind::Ident)?;
                return Some((
                    String::from_utf8_lossy(&src[t.span.lo as usize..t.span.hi as usize])
                        .into_owned(),
                    t.span,
                ));
            }
            SyntaxKind::MemberExpr | SyntaxKind::BracketApply | SyntaxKind::ParenExpr => {
                e = e.nodes().next()?;
            }
            _ => return None,
        }
    }
}

/// Every node beneath `node`, depth-first (excluding `node` itself).
fn descendants(node: &GreenNode) -> impl Iterator<Item = &GreenNode> {
    let mut stack: Vec<&GreenNode> = node.nodes().collect();
    stack.reverse();
    std::iter::from_fn(move || {
        let n = stack.pop()?;
        let mut kids: Vec<&GreenNode> = n.nodes().collect();
        kids.reverse();
        stack.extend(kids);
        Some(n)
    })
}

/// Row entries in a function's *signature* (body excluded): the tag
/// text and its span.
fn collect_row_entries(
    node: &GreenNode,
    body_span: Option<Span>,
    src: &[u8],
    out: &mut Vec<(String, Span)>,
) {
    for row in descendants(node).filter(|n| n.kind == SyntaxKind::ErrorRow) {
        if body_span.is_some_and(|b| row.span.lo >= b.lo && row.span.hi <= b.hi) {
            continue;
        }
        for entry in row.nodes().filter_map(RowEntry::cast) {
            if let Some(p) = entry.path() {
                let span = p.syntax().span;
                let tag =
                    String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned();
                // Dotted tags (`io.Error`) name a foreign row; the
                // collision lint concerns bare names only.
                if !tag.contains('.') {
                    out.push((tag, span));
                }
            }
        }
    }
}

/// `{ident}` (optionally `{ident:spec}`) inside raw-literal text:
/// byte offsets of the braced group and the identifier inside it.
fn interp_shaped(text: &str) -> Option<(usize, usize, String)> {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            let start = i;
            let mut j = i + 1;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            let ident_end = j;
            // allow a `:spec` tail
            while j < b.len() && b[j] != b'}' && b[j] != b'{' {
                j += 1;
            }
            if ident_end > i + 1 && j < b.len() && b[j] == b'}' && b[i + 1].is_ascii_alphabetic() {
                return Some((start, j + 1, text[i + 1..ident_end].to_string()));
            }
            i = j;
        }
        i += 1;
    }
    None
}

// ==================================================== typecheck rung ==

/// The typed wave over one checked body: W0601 (discarded fallible
/// results), W0401 (literal outside the cast target's range), W0402
/// (`0.0 - x`). Reads the recorded types only — no re-inference.
pub(crate) fn check_typed_body(pkg: &Package, body: &BodyRef, tb: &TypedBody) -> Vec<Diagnostic> {
    let file = &pkg.files[body.file];
    if pkg.modules[body.module]
        .path
        .first()
        .is_some_and(|s| s == "std")
    {
        return Vec::new(); // library files: see `check_file`
    }
    let src = &file.raw.src;
    let outer = file
        .parse
        .root
        .nodes()
        .filter(|n| n.kind.is_item())
        .nth(body.decl);
    let Some(outer) = outer else {
        return Vec::new();
    };
    let node = match body.member {
        None => outer,
        Some(mi) => match outer.nodes().filter(|n| n.kind.is_item()).nth(mi) {
            Some(n) => n,
            None => return Vec::new(),
        },
    };
    let ty_at = |span: Span| tb.exprs.iter().find(|(s, _)| *s == span).map(|&(_, t)| t);
    let mut diags = Vec::new();
    let text =
        |span: Span| String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned();
    let rendered = |t| render(&tb.table, t, &|_| Err("_"));

    // W0601 — non-trailing expression statements of `!T` type.
    for block in descendants(node)
        .filter(|n| n.kind == SyntaxKind::Block)
        .filter_map(Block::cast)
    {
        let stmts: Vec<&GreenNode> = block.statements().collect();
        let last = stmts.len().saturating_sub(1);
        for (i, s) in stmts.iter().enumerate() {
            if i == last {
                continue; // the block's value position
            }
            let Some(e) = ExprStmt::cast(s).and_then(|x| x.expr()) else {
                continue;
            };
            let Some(t) = ty_at(e.span) else { continue };
            if let TyKind::ErrUnion(..) = tb.table.kind(t) {
                diags.push(
                    Diagnostic::warning(
                        codes::W0601,
                        e.span,
                        format!(
                            "this `{}` result is discarded, error row and all",
                            rendered(t)
                        ),
                    )
                    .with_label("a failure here vanishes silently")
                    .with_note(
                        "propagate it with `?`, handle it with `else`, or bind it \
                         away explicitly so the discard is visible."
                            .to_string(),
                    ),
                );
            }
        }
    }

    // W0401 — `<literal> as <narrow int>` that cannot fit.
    for &(span, _from, to, _kind) in &tb.casts {
        let Some(cast) = descendants(node)
            .find(|n| n.span == span && n.kind == SyntaxKind::CastExpr)
            .and_then(CastExpr::cast)
        else {
            continue;
        };
        let Some(value) = literal_value(cast.expr(), src) else {
            continue;
        };
        let TyKind::Prim(p) = tb.table.kind(to) else {
            continue;
        };
        let Some((lo, hi)) = int_range(*p) else {
            continue;
        };
        if value < lo || value > hi {
            diags.push(
                Diagnostic::warning(
                    codes::W0401,
                    span,
                    format!("`{value}` does not fit `{}`", p.name()),
                )
                .with_label(format!("{} holds {lo}..={hi}", p.name()))
                .with_note(
                    "the value is known here, and the target type cannot hold it — \
                     the conversion can never preserve it. Widen the target, or make \
                     intended wraparound part of the type with `wrapping`."
                        .to_string(),
                ),
            );
        }
    }

    // W0402 — `0.0 - x` where the subtraction is float-typed.
    for e in descendants(node).filter(|n| n.kind == SyntaxKind::BinExpr) {
        let is_minus = e.tokens().any(|t| t.kind == SyntaxKind::Minus);
        if !is_minus {
            continue;
        }
        let mut kids = e.nodes();
        let (Some(lhs), Some(rhs)) = (kids.next(), kids.next()) else {
            continue;
        };
        if lhs.kind != SyntaxKind::LiteralExpr || text(lhs.span) != "0.0" {
            continue;
        }
        let Some(t) = ty_at(e.span) else { continue };
        if !matches!(tb.table.kind(t), TyKind::Prim(p) if p.is_float()) {
            continue;
        }
        diags.push(
            Diagnostic::warning(
                codes::W0402,
                e.span,
                "`0.0 - x` is not negation".to_string(),
            )
            .with_label("erases the sign of `-0.0`")
            .with_note(
                "subtracting from zero maps `-0.0` to `+0.0`; unary minus negates \
                     every value, zeros included."
                    .to_string(),
            )
            .with_suggestion(Suggestion::new(
                "negate with unary minus",
                vec![(
                    Span::new(e.span.file, lhs.span.lo, rhs.span.lo),
                    "-".to_string(),
                )],
                Applicability::Maybe,
            )),
        );
    }

    diags
}

/// A compile-time-known integer literal value: `LiteralExpr` or a
/// minus-prefixed one. Handles `_` separators and `0x`/`0o`/`0b`.
fn literal_value(e: Option<&GreenNode>, src: &[u8]) -> Option<i128> {
    let e = e?;
    let (neg, lit) = match e.kind {
        SyntaxKind::LiteralExpr => (false, e),
        SyntaxKind::PrefixExpr => {
            let minus = e
                .tokens()
                .next()
                .is_some_and(|t| t.kind == SyntaxKind::Minus);
            let inner = e.nodes().next()?;
            if !minus || inner.kind != SyntaxKind::LiteralExpr {
                return None;
            }
            (true, inner)
        }
        _ => return None,
    };
    let t = lit.tokens().find(|t| t.kind == SyntaxKind::Int)?;
    let raw =
        String::from_utf8_lossy(&src[t.span.lo as usize..t.span.hi as usize]).replace('_', "");
    let value = if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        i128::from_str_radix(hex, 16).ok()?
    } else if let Some(oct) = raw.strip_prefix("0o").or_else(|| raw.strip_prefix("0O")) {
        i128::from_str_radix(oct, 8).ok()?
    } else if let Some(bin) = raw.strip_prefix("0b").or_else(|| raw.strip_prefix("0B")) {
        i128::from_str_radix(bin, 2).ok()?
    } else {
        raw.parse::<i128>().ok()?
    };
    Some(if neg { -value } else { value })
}

/// The inclusive range of an integer primitive; `None` for non-ints.
fn int_range(p: Prim) -> Option<(i128, i128)> {
    Some(match p {
        Prim::I8 => (i8::MIN as i128, i8::MAX as i128),
        Prim::I16 => (i16::MIN as i128, i16::MAX as i128),
        Prim::I32 => (i32::MIN as i128, i32::MAX as i128),
        Prim::I64 | Prim::Int => (i64::MIN as i128, i64::MAX as i128),
        Prim::U8 | Prim::Byte => (0, u8::MAX as i128),
        Prim::U16 => (0, u16::MAX as i128),
        Prim::U32 => (0, u32::MAX as i128),
        Prim::U64 | Prim::Uint => (0, u64::MAX as i128),
        _ => return None,
    })
}
