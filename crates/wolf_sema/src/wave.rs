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
//! The idiom arbiter (the second wave) adds two more layers:
//!
//! - **convention lints** in [`check_file`], mechanizing the house API
//!   conventions where they generalize: W0310 `get_` prefixes, W0311
//!   predicate names that do not answer `bool`, W0312 `as_` views
//!   that consume, W0313 undocumented `pub` items, W0603 row-tag
//!   case/payload discipline, W0604 a `get` that cannot miss, W1002
//!   `mut` parameters never written (with the drop-the-`mut` fix-it),
//!   W1003 `take` parameters returned unchanged;
//! - [`check_package`] — the **package-shape** pass over the resolved
//!   module graph (D32 lived experience): W0314 one-item modules,
//!   W0315 `pub(pkg)` items nothing else in the package uses, W0316
//!   modules importing their own ancestor (the cyclic-adjacent shape
//!   short of the hard error).
//!
//! Cost discipline (D5): each pass is one walk over trees the phase
//! already holds, allocation-light, and firing nothing on clean code.

use std::collections::BTreeSet;

use wolf_ast::{
    Arg, AssignStmt, AssumeStmt, Block, CallExpr, CastExpr, ClosureExpr, ConstDecl, ElseExpr,
    EnumDecl, ExprStmt, FnDecl, GreenNode, GreenToken, LetDecl, MemberExpr, ParamMode, ParenExpr,
    RowEntry, StructDecl, SyntaxKind, TraitDecl, TypeDecl, VarDecl, Visibility,
};
use wolf_diag::{Applicability, Diagnostic, Diagnostics, Suggestion, codes};
use wolf_span::Span;

use crate::check::{BodyRef, TypedBody};
use crate::graph::{BindTarget, Package, Vis, pattern_names};
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
        // W0313 — a `pub` item (not `pub(pkg)`) with no `///` above it.
        // Impl blocks are not named exports and stay out of scope.
        if node.kind != SyntaxKind::ImplDecl {
            self.pub_doc_check(node);
        }
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
        self.convention_checks(&d);
        self.mode_discipline(&d, free);
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
    /// item, an import, or a prelude name — and W0603, the tag-shape
    /// discipline over the same entries: marks are lowercase bare
    /// words, payload-carrying tags are CapCase, and `none` never
    /// carries a payload.
    fn row_collision_check(&mut self, node: &GreenNode, d: &FnDecl<'_>) {
        // Every row entry in the signature (return row; param rows).
        let mut entries: Vec<(String, Span, usize)> = Vec::new();
        collect_row_entries(
            node,
            d.body().map(|b| b.syntax().span),
            self.src,
            &mut entries,
        );
        // Row *variables* are generic parameters, not tags: `! {E}` in
        // a signature whose generics declare `E` instantiates per call
        // site and owes the tag-shape rules nothing. Single uppercase
        // letters get the same benefit (the row-variable convention,
        // for members whose generics live on an enclosing impl).
        let generics: BTreeSet<String> = d
            .generics()
            .map(|g| {
                g.params()
                    .filter_map(|p| p.name())
                    .map(|t| self.text(t.span))
                    .collect()
            })
            .unwrap_or_default();
        for (tag, span, payload) in &entries {
            if generics.contains(tag)
                || (tag.len() == 1 && tag.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
            {
                continue;
            }
            let capitalized = tag.chars().next().is_some_and(|c| c.is_ascii_uppercase());
            let shape = if tag == "none" && *payload > 0 {
                Some((
                    "`none` carries a payload here".to_string(),
                    "absence is not an error",
                    "\"there is nothing here\" and \"this went wrong, here is how\" are \
                     different answers on purpose: `none` stays payload-free, and a \
                     failure that has something to say gets its own CapCase tag.",
                ))
            } else if *payload > 0 && !capitalized {
                Some((
                    format!("the tag `{tag}` carries a payload but is spelled like a mark"),
                    "lowercase reads as nothing-to-destructure",
                    "payload-carrying tags are CapCase and name their payload type \
                     (`Parse(ParseErr)`); lowercase bare words are reserved for \
                     payload-free marks.",
                ))
            } else if *payload == 0 && capitalized {
                Some((
                    format!("the mark `{tag}` is spelled CapCase"),
                    "reads as if there were data to destructure",
                    "payload-free marks are lowercase bare words (`none`, `eof`, \
                     `parse`); CapCase is the reader's signal that a payload waits \
                     inside.",
                ))
            } else {
                None
            };
            if let Some((msg, label, note)) = shape {
                self.warn(
                    Diagnostic::warning(codes::W0603, *span, msg)
                        .with_label(label)
                        .with_note(note.to_string()),
                );
            }
        }
        for (tag, span, _) in entries {
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

    // ----------------------------------------- the idiom arbiter --

    /// W0313 — a `pub` item with no `///` above it. `pub(pkg)` is
    /// package-internal and exempt; the doc obligation is the export's.
    fn pub_doc_check(&mut self, node: &GreenNode) {
        let Some(vis) = node.nodes().find_map(Visibility::cast) else {
            return;
        };
        if vis.is_pkg() {
            return;
        }
        let documented = first_token(node).is_some_and(|t| {
            t.leading.iter().any(|sp| {
                self.src[sp.lo as usize..sp.hi as usize]
                    .strip_prefix(b"///")
                    .is_some()
            })
        });
        if documented {
            return;
        }
        self.warn(
            Diagnostic::warning(
                codes::W0313,
                vis.syntax().span,
                "this `pub` item has no doc comment".to_string(),
            )
            .with_label("exported, but undocumented")
            .with_note(
                "an exported item carries a `///` contract: what it computes, what \
                 each row tag means, when it traps. If it is not worth documenting, \
                 it is rarely worth exporting."
                    .to_string(),
            ),
        );
    }

    /// The naming conventions that generalize mechanically: W0310
    /// (`get_` prefix), W0311 (predicate names answer `bool`), W0312
    /// (`as_` views borrow), W0604 (bare `get` is checked access).
    fn convention_checks(&mut self, d: &FnDecl<'_>) {
        let Some(t) = d.name() else { return };
        let name = self.text(t.span);
        if let Some(noun) = name.strip_prefix("get_")
            && !noun.is_empty()
        {
            self.warn(
                Diagnostic::warning(
                    codes::W0310,
                    t.span,
                    format!("`{name}` wears the `get_` prefix"),
                )
                .with_label("the prefix says nothing")
                .with_note(format!(
                    "every function gets something — name it after the noun it \
                     fetches (`{noun}`), or, for checked access that can miss, \
                     bare `get` with an absence row."
                )),
            );
        }
        if name
            .strip_prefix("is_")
            .or_else(|| name.strip_prefix("has_"))
            .is_some_and(|rest| !rest.is_empty())
        {
            let answers_bool = d
                .ret_ty()
                .and_then(|r| r.ty())
                .is_some_and(|ty| self.text(ty.span) == "bool");
            if !answers_bool {
                self.warn(
                    Diagnostic::warning(
                        codes::W0311,
                        t.span,
                        format!("`{name}` reads as a predicate but does not answer `bool`"),
                    )
                    .with_label("a question that is not one")
                    .with_note(
                        "`is_`/`has_` names promise a yes-or-no answer; return `bool`, \
                         or rename the function after what it produces."
                            .to_string(),
                    ),
                );
            }
        }
        if name
            .strip_prefix("as_")
            .is_some_and(|rest| !rest.is_empty())
            && let Some(params) = d.params()
            && let Some(p) = params.params().find(|p| p.mode().is_some())
        {
            let verb = match p.mode() {
                Some(ParamMode::Take) => "consumes",
                _ => "mutates",
            };
            self.warn(
                Diagnostic::warning(
                    codes::W0312,
                    p.syntax().span,
                    format!(
                        "`{name}` reads as a borrowed view, but this parameter {verb} its operand"
                    ),
                )
                .with_label("a view must borrow")
                .with_note(
                    "`as_x` (and bare nouns) are views over an untouched operand; \
                     `to_x` is the spelling that builds a new value. Rename the \
                     function, or give the parameter the read default."
                        .to_string(),
                ),
            );
        }
        if name == "get" && !d.ret_ty().is_some_and(|r| r.error_row().is_some()) {
            self.warn(
                Diagnostic::warning(
                    codes::W0604,
                    t.span,
                    "this `get` declares no absence row".to_string(),
                )
                .with_label("checked access that cannot miss")
                .with_note(
                    "bare `get` is the checked-access spelling: it answers `T ! {none}` \
                     and callers handle the miss. Give it the absence row, or name it \
                     after what it computes."
                        .to_string(),
                ),
            );
        }
    }

    /// Mode discipline (X1): W1002 — a `mut` parameter the body never
    /// writes; W1003 — a `take` parameter returned unchanged. Both
    /// carry drop-the-mode fix-its; W1002's is machine-applicable when
    /// every call site is provably rewritable (private free function,
    /// name used only as a call), and honest `Maybe` otherwise.
    fn mode_discipline(&mut self, d: &FnDecl<'_>, free: bool) {
        let Some(body) = d.body() else { return };
        let body_node = body.syntax();
        let Some(params) = d.params() else { return };
        // Shadowing makes the flat write scan unreliable: a body that
        // rebinds the name anywhere keeps the lint silent.
        let rebound = rebound_names(body_node, self.src);
        for (idx, p) in params.params().enumerate() {
            let Some(mode) = p.mode() else { continue };
            let name_tok = if p.is_self() {
                p.syntax().child_token(SyntaxKind::SelfKw)
            } else {
                p.name()
            };
            let Some(name_tok) = name_tok else { continue };
            let name = self.text(name_tok.span);
            if rebound.contains(&name) {
                continue;
            }
            let Some(mode_tok) = p
                .syntax()
                .tokens()
                .find(|t| matches!(t.kind, SyntaxKind::MutKw | SyntaxKind::TakeKw))
            else {
                continue;
            };
            if body_writes(body_node, &name, self.src) {
                continue;
            }
            let drop_decl = (
                Span::new(mode_tok.span.file, mode_tok.span.lo, name_tok.span.lo),
                String::new(),
            );
            match mode {
                ParamMode::Mut => {
                    let callsites = if free && d.visibility().is_none() {
                        d.name()
                            .and_then(|f| self.callsite_mut_edits(&self.text(f.span), f.span, idx))
                    } else {
                        None
                    };
                    let sugg = match callsites {
                        Some(mut edits) => {
                            edits.insert(0, drop_decl);
                            Suggestion::new(
                                "drop the `mut` here and at every call site — the \
                                 parameter is never written",
                                edits,
                                Applicability::MachineApplicable,
                            )
                        }
                        None => Suggestion::new(
                            "drop the `mut` (call sites passing `mut` must drop \
                             theirs too)",
                            vec![drop_decl],
                            Applicability::Maybe,
                        ),
                    };
                    self.warn(
                        Diagnostic::warning(
                            codes::W1002,
                            mode_tok.span,
                            format!("`{name}` is `mut`, and the body never writes it"),
                        )
                        .with_label("writeback nothing uses")
                        .with_note(
                            "every call site surrenders exclusive access for a write \
                             that never happens; the read default is the honest mode."
                                .to_string(),
                        )
                        .with_suggestion(sugg),
                    );
                }
                ParamMode::Take => {
                    if !returns_bare(body_node, &name, self.src) {
                        continue;
                    }
                    self.warn(
                        Diagnostic::warning(
                            codes::W1003,
                            mode_tok.span,
                            format!("`{name}` is taken, never touched, and returned"),
                        )
                        .with_label("consumption that consumes nothing")
                        .with_note(
                            "the caller gives the value up only to receive it back; \
                             if callers could reasonably keep it, the signature is \
                             wrong."
                                .to_string(),
                        )
                        .with_suggestion(Suggestion::new(
                            "drop the `take` (call sites drop theirs and keep their \
                             binding; owned payloads may then need a real transform)",
                            vec![drop_decl],
                            Applicability::Maybe,
                        )),
                    );
                }
            }
        }
    }

    /// The call-site half of W1002's machine-applicable fix: every
    /// mention of `fname` across the module must be its declaration or
    /// the callee of a plain call — then the edits removing each call's
    /// `mut` at `arg_index` are returned. Any other use (a value
    /// position, a dotted path, a shadowing risk) refuses with `None`
    /// and the fix stays `Maybe`.
    fn callsite_mut_edits(
        &self,
        fname: &str,
        decl_span: Span,
        arg_index: usize,
    ) -> Option<Vec<(Span, String)>> {
        let mut edits = Vec::new();
        for &fi in &self.pkg.modules[self.module].files {
            let src = &self.pkg.files[fi].raw.src;
            let root = &self.pkg.files[fi].parse.root;
            let mut callee_idents: Vec<Span> = Vec::new();
            for call in descendants(root).filter_map(CallExpr::cast) {
                let Some(callee) = call.callee() else {
                    continue;
                };
                if callee.kind != SyntaxKind::PathExpr {
                    continue;
                }
                let idents: Vec<&GreenToken> = callee
                    .tokens()
                    .filter(|t| t.kind == SyntaxKind::Ident)
                    .collect();
                // A bare name, not a dotted path.
                let [ident] = idents[..] else { continue };
                if ident.text(src) != fname.as_bytes() {
                    continue;
                }
                callee_idents.push(ident.span);
                let Some(args) = call.args() else { continue };
                let Some(arg) = args.args().nth(arg_index) else {
                    continue;
                };
                if arg.mode() == Some(ParamMode::Mut) {
                    let mt = arg
                        .syntax()
                        .tokens()
                        .find(|t| t.kind == SyntaxKind::MutKw)?;
                    let v = arg.value()?;
                    edits.push((
                        Span::new(mt.span.file, mt.span.lo, v.span.lo),
                        String::new(),
                    ));
                }
            }
            // Every other mention refuses the machine-applicable claim.
            let mut ok = true;
            all_tokens(root, &mut |t| {
                if t.kind == SyntaxKind::Ident
                    && t.text(src) == fname.as_bytes()
                    && t.span != decl_span
                    && !callee_idents.contains(&t.span)
                {
                    ok = false;
                }
            });
            if !ok {
                return None;
            }
        }
        Some(edits)
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

/// The leftmost identifier of a place expression (`a`, `a.b`, `a[i]`,
/// `self.f`).
fn place_root(mut e: &GreenNode, src: &[u8]) -> Option<(String, Span)> {
    loop {
        match e.kind {
            SyntaxKind::PathExpr => {
                let t = e
                    .tokens()
                    .find(|t| matches!(t.kind, SyntaxKind::Ident | SyntaxKind::SelfKw))?;
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

/// The first token of `node` in document order (for leading-trivia
/// questions like "is there a `///` above this declaration?").
fn first_token(node: &GreenNode) -> Option<&GreenToken> {
    for child in &node.children {
        match child {
            wolf_ast::Child::Token(t) => return Some(t),
            wolf_ast::Child::Node(n) => {
                if let Some(t) = first_token(n) {
                    return Some(t);
                }
            }
        }
    }
    None
}

/// Visit every token beneath `node`, depth-first.
fn all_tokens(node: &GreenNode, f: &mut impl FnMut(&GreenToken)) {
    for child in &node.children {
        match child {
            wolf_ast::Child::Token(t) => f(t),
            wolf_ast::Child::Node(n) => all_tokens(n, f),
        }
    }
}

/// Every name bound *inside* `body` — `let`/`var`/`const` patterns,
/// match/`for` binders, closure parameters. A parameter whose name is
/// in this set is shadowed somewhere, and the flat write scan cannot
/// tell the two apart.
fn rebound_names(body: &GreenNode, src: &[u8]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for d in descendants(body) {
        match d.kind {
            SyntaxKind::LetDecl => {
                if let Some(pat) = LetDecl::cast(d).and_then(|x| x.pattern()) {
                    out.extend(pattern_names(pat, src).into_iter().map(|(n, _)| n));
                }
            }
            SyntaxKind::VarDecl => {
                if let Some(pat) = VarDecl::cast(d).and_then(|x| x.pattern()) {
                    out.extend(pattern_names(pat, src).into_iter().map(|(n, _)| n));
                }
            }
            SyntaxKind::ConstDecl => {
                if let Some(t) = ConstDecl::cast(d).and_then(|x| x.name()) {
                    out.insert(text_of(src, t));
                }
            }
            SyntaxKind::IdentPat | SyntaxKind::BindingPat => {
                out.extend(pattern_names(d, src).into_iter().map(|(n, _)| n));
            }
            SyntaxKind::ClosureExpr => {
                if let Some(cl) = ClosureExpr::cast(d)
                    && let Some(params) = cl.params()
                {
                    for p in params.params() {
                        if let Some(t) = p.name() {
                            out.insert(text_of(src, t));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Does `body` ever write `name`? Writes are assignments whose place
/// roots at the name (projections included — `p.f = v` mutates `p`),
/// and `mut`/`take` positions handing it onward: call arguments and
/// moded receivers.
fn body_writes(body: &GreenNode, name: &str, src: &[u8]) -> bool {
    for n in descendants(body) {
        let hit = match n.kind {
            SyntaxKind::AssignStmt => AssignStmt::cast(n)
                .and_then(|a| a.place())
                .and_then(|p| place_root(p, src))
                .is_some_and(|(root, _)| root == name),
            SyntaxKind::Arg => Arg::cast(n).is_some_and(|a| {
                a.mode().is_some()
                    && a.value()
                        .and_then(|v| place_root(v, src))
                        .is_some_and(|(root, _)| root == name)
            }),
            SyntaxKind::ParenExpr => ParenExpr::cast(n).is_some_and(|p| {
                p.mode().is_some()
                    && p.expr()
                        .and_then(|v| place_root(v, src))
                        .is_some_and(|(root, _)| root == name)
            }),
            _ => false,
        };
        if hit {
            return true;
        }
    }
    false
}

/// Does the function hand `name` back unchanged — a `return name`, or
/// the body block's trailing value being the bare name?
fn returns_bare(body: &GreenNode, name: &str, src: &[u8]) -> bool {
    let bare = |e: &GreenNode| {
        e.kind == SyntaxKind::PathExpr
            && String::from_utf8_lossy(&src[e.span.lo as usize..e.span.hi as usize]) == name
    };
    for n in descendants(body).filter(|n| n.kind == SyntaxKind::ReturnExpr) {
        if n.nodes().next().is_some_and(bare) {
            return true;
        }
    }
    let last = body
        .nodes()
        .filter(|n| wolf_ast::is_stmt_kind(n.kind))
        .last();
    last.and_then(ExprStmt::cast)
        .and_then(|x| x.expr())
        .is_some_and(bare)
}

/// Every node beneath `node`, depth-first (excluding `node` itself).
pub(crate) fn descendants(node: &GreenNode) -> impl Iterator<Item = &GreenNode> {
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
/// text, its span, and how many payload types it declares.
fn collect_row_entries(
    node: &GreenNode,
    body_span: Option<Span>,
    src: &[u8],
    out: &mut Vec<(String, Span, usize)>,
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
                // shape and collision lints concern bare names only.
                if !tag.contains('.') {
                    out.push((tag, span, entry.payload().count()));
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

// ==================================================== package shape ==

/// The package-shape pass (the idiom arbiter's structure lints), run
/// once over the resolved module graph: W0314 one-item modules, W0315
/// `pub(pkg)` items nothing else in the package uses, W0316 modules
/// importing their own ancestor. The std tree is exempt exactly as in
/// [`check_file`].
pub(crate) fn check_package(pkg: &Package, sink: &mut Diagnostics) {
    let is_std = |m: usize| pkg.modules[m].path.first().is_some_and(|s| s == "std");
    // Ident inventory per module (W0315's conservative "used" test: any
    // mention of the name anywhere in another module counts).
    let idents: Vec<Option<BTreeSet<String>>> = (0..pkg.modules.len())
        .map(|m| {
            if is_std(m) {
                return None;
            }
            let mut set = BTreeSet::new();
            for &fi in &pkg.modules[m].files {
                let src = &pkg.files[fi].raw.src;
                all_tokens(&pkg.files[fi].parse.root, &mut |t| {
                    if t.kind == SyntaxKind::Ident {
                        set.insert(text_of(src, t));
                    }
                });
            }
            Some(set)
        })
        .collect();
    for m in 0..pkg.modules.len() {
        if is_std(m) {
            continue;
        }
        let path = &pkg.modules[m].path;
        // W0314 — a non-root module holding exactly one item. Namespace
        // parents (modules with child modules) are structure, not
        // ceremony, and stay silent.
        if !path.is_empty() {
            let is_parent = pkg.modules.iter().enumerate().any(|(o, om)| {
                o != m && om.path.len() > path.len() && om.path[..path.len()] == path[..]
            });
            if !is_parent && pkg.tables[m].items.len() == 1 {
                let item = &pkg.tables[m].items[0];
                sink.push(
                    Diagnostic::warning(
                        codes::W0314,
                        item.name_span,
                        format!("{} holds exactly one item", pkg.modules[m].display_name()),
                    )
                    .with_label("a directory of ceremony around it")
                    .with_note(
                        "fold the item into the module that uses it, or grow the \
                         module into the family its name promises; a deliberate seam \
                         says so with `#[allow(w0314)]` and a reason."
                            .to_string(),
                    ),
                );
            }
        }
        // W0316 — an import reaching up into an ancestor module.
        for file_bindings in &pkg.modules[m].bindings {
            for b in file_bindings {
                let target = match &b.target {
                    BindTarget::PkgModule(i) => Some(*i),
                    BindTarget::Item { module, .. } => Some(*module),
                    _ => None,
                };
                if let Some(t) = target
                    && !is_std(t)
                    && pkg.modules[t].path.len() < path.len()
                    && path.starts_with(&pkg.modules[t].path)
                {
                    sink.push(
                        Diagnostic::warning(
                            codes::W0316,
                            b.name_span,
                            format!(
                                "{} imports {}, its own ancestor",
                                pkg.modules[m].display_name(),
                                pkg.modules[t].display_name()
                            ),
                        )
                        .with_label("one edit away from an import cycle")
                        .with_note(
                            "the ancestor owns this module structurally, and this \
                             import couples the two the other way; move the shared \
                             items down (or into a sibling) so the dependency runs \
                             parent to child."
                                .to_string(),
                        ),
                    );
                }
            }
        }
        // W0315 — a `pub(pkg)` item no other loaded module mentions.
        for item in &pkg.tables[m].items {
            if item.vis != Vis::Pkg {
                continue;
            }
            let used = idents
                .iter()
                .enumerate()
                .any(|(o, set)| o != m && set.as_ref().is_some_and(|s| s.contains(&item.name)));
            if !used {
                sink.push(
                    Diagnostic::warning(
                        codes::W0315,
                        item.name_span,
                        format!(
                            "nothing else in the package uses the `pub(pkg)` item `{}`",
                            item.name
                        ),
                    )
                    .with_label("package visibility, unearned")
                    .with_note(
                        "the widened visibility claims another module needs this; \
                         make the item private, and widen it back the day one does."
                            .to_string(),
                    ),
                );
            }
        }
    }
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
pub(crate) fn literal_value(e: Option<&GreenNode>, src: &[u8]) -> Option<i128> {
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
pub(crate) fn int_range(p: Prim) -> Option<(i128, i128)> {
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
