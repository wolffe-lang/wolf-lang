//! The annotating queries (s134): signature help, semantic tokens,
//! inlay hints — s133's binding table read three more ways.
//!
//! Every answer comes from what the resolver and the checker already
//! recorded: [`wolf_sema::Resolution::refs`] (the lexical half — every
//! name bound, uses and binders alike), [`wolf_sema::TypedBody`]'s
//! `member_refs` (fields, variants, methods), `calls` (every resolved
//! call site's declared parameter surface, keyed by the call
//! expression's span) and `locals` (every local binding's inferred
//! type). Nothing here re-resolves or searches text; a name the
//! compiler never bound gets no token, a call it never resolved gets no
//! signature, a binding it never typed gets no hint — honestly, never a
//! guess.
//!
//! # Signature help
//!
//! The innermost call whose argument list contains the cursor answers
//! its callee's declared parameters (`name: type`, the call-site mode
//! spelled where one is declared), the active parameter counted by the
//! commas before the cursor, the return type when the callee is a
//! declared item, and its `///` doc comment — the same doc model hover
//! and `wolf doc` read. A method's receiver is not a parameter the call
//! site spells inside the parentheses, so it is not one the label
//! spells either. A variant constructor's payload types are its
//! parameters; a fn-typed value's parameters have types and no names.
//!
//! # Semantic tokens
//!
//! Keywords from the parse tree; every other token from the binding
//! table's target: a `Local` is a `parameter` when its binder sits in
//! a parameter list and a `variable` otherwise (`readonly` unless the
//! binder is `var`'s); an `Item` is what its kind says (`function`, or
//! `type` for structs, enums, aliases and traits, `variable` for
//! consts and globals); a module, a std path and the `c` namespace are
//! `namespace`; a prelude name is a `function` where it is called and a
//! `variable` elsewhere; a builtin type and `Self` are `type`; a member
//! is `property`, `enumMember` or `function` by what its declaration
//! is. The `declaration` modifier marks a binder's own token. Tokens
//! are emitted in source order and never overlap.
//!
//! # Inlay hints
//!
//! Two classes, both answered and both filterable by the server (the
//! client's configuration decides what shows): the inferred type of a
//! `let`/`var` binder that carries no ascription (`: int`, after the
//! name), and the parameter name before a positional argument whose
//! expression is not already that name (`side:` before `3`, nothing
//! before `side`). Only calls the checker resolved to a declaration
//! carry names; a fn-typed value has none to offer.

use std::path::Path;

use wolf_ast::{Child, GreenNode, GreenToken, SyntaxKind};
use wolf_sema::sig::{FnSig, ItemSig, ParamSig};
use wolf_sema::{BodyResult, ItemKind, RefTarget, Resolution, Typecheck, TypedBody};
use wolf_span::Span;

use crate::host::{Cancelled, Snapshot};
use crate::queries::{PackageAnalysis, file_index};

// ------------------------------------------------------------- results --

/// What `signatureHelp` answers for a cursor inside an argument list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureHelpResult {
    /// The rendered signature: `area(side: int) -> int`.
    pub label: String,
    /// Each parameter's label as a byte range within `label`, in
    /// declaration order (the receiver excluded).
    pub params: Vec<(u32, u32)>,
    /// The parameter the cursor is in, by commas before it — `None`
    /// when the callee declares none.
    pub active_parameter: Option<u32>,
    /// The callee's `///` doc comment, when it has one.
    pub doc: Option<String>,
}

/// A semantic-token type — the closed set this server emits, named
/// as the protocol's standard legend names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemKind {
    Namespace,
    Type,
    Parameter,
    Variable,
    Property,
    EnumMember,
    Function,
    Keyword,
}

impl SemKind {
    /// The protocol's standard name.
    pub fn as_str(self) -> &'static str {
        match self {
            SemKind::Namespace => "namespace",
            SemKind::Type => "type",
            SemKind::Parameter => "parameter",
            SemKind::Variable => "variable",
            SemKind::Property => "property",
            SemKind::EnumMember => "enumMember",
            SemKind::Function => "function",
            SemKind::Keyword => "keyword",
        }
    }
}

/// One classified token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemToken {
    pub span: Span,
    pub kind: SemKind,
    /// This token is the binder / declaration itself.
    pub declaration: bool,
    /// A `let` (not `var`) binding, or a `const`.
    pub readonly: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintKind {
    /// An inferred type after a binder: `: int`.
    Type,
    /// A parameter name before an argument: `side:`.
    Parameter,
}

/// One inlay hint, anchored at a byte offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlayHint {
    pub offset: u32,
    pub label: String,
    pub kind: HintKind,
}

// --------------------------------------------------------- the queries --

impl Snapshot {
    /// Signature help (s134): the declared parameter surface of the
    /// innermost call whose argument list contains `offset`.
    /// `Ok(None)` when the cursor is in no argument list, or the call
    /// never resolved.
    pub fn signature_help(
        &self,
        entry: &Path,
        offset: u32,
    ) -> Result<Option<SignatureHelpResult>, Cancelled> {
        self.guard(|| self.signature_help_impl(entry, offset))
    }

    /// Semantic tokens (s134) for one file, in source order, never
    /// overlapping. `Ok(None)` when the file never reached resolution.
    pub fn semantic_tokens(&self, path: &Path) -> Result<Option<Vec<SemToken>>, Cancelled> {
        self.guard(|| self.semantic_tokens_impl(path))
    }

    /// Inlay hints (s134) for the byte range `[lo, hi)` of one file.
    /// `Ok(None)` when the file never reached typecheck.
    pub fn inlay_hints(
        &self,
        path: &Path,
        lo: u32,
        hi: u32,
    ) -> Result<Option<Vec<InlayHint>>, Cancelled> {
        self.guard(|| self.inlay_hints_impl(path, lo, hi))
    }

    // ---------------------------------------------------------- innards --

    fn signature_help_impl(&self, entry: &Path, offset: u32) -> Option<SignatureHelpResult> {
        let a = self.analysis(entry)?;
        let res = a.resolution.as_ref()?;
        let file_idx = file_index(res, entry)?;
        let unit = &res.package.files[file_idx];
        let (call, args) = call_at(&unit.parse.root, offset)?;
        let tc = a.typecheck.as_ref()?;
        let tb = body_containing(tc, file_idx, call.span.lo)?;
        let (_, sig) = tb.calls.iter().find(|(s, _)| *s == call.span)?;
        // Commas among the argument list's own tokens, before the cursor.
        let commas = args
            .tokens()
            .filter(|t| t.kind == SyntaxKind::Comma && t.span.hi <= offset)
            .count() as u32;
        let shown: Vec<&ParamSig> = sig.params.iter().skip(usize::from(sig.has_self)).collect();
        // A declared callee's parameter types live in the signature
        // table; a fn-typed value's were synthesized from the body's.
        let table = if sig.decl_span.is_some() {
            &tc.sigs.table
        } else {
            &tb.table
        };
        let render = |t| wolf_sema::types::render(table, t, &|_| Err("_"));
        let mut label = format!("{}(", sig.callee);
        let mut params = Vec::with_capacity(shown.len());
        for (i, p) in shown.iter().enumerate() {
            if i > 0 {
                label.push_str(", ");
            }
            let start = label.len() as u32;
            match p.mode {
                Some(wolf_ast::ParamMode::Mut) => label.push_str("mut "),
                Some(wolf_ast::ParamMode::Take) => label.push_str("take "),
                _ => {}
            }
            if !p.name.is_empty() {
                label.push_str(&p.name);
                label.push_str(": ");
            }
            label.push_str(&render(p.ty));
            params.push((start, label.len() as u32));
        }
        label.push(')');
        let decl = sig.decl_span.and_then(|d| fn_sig_at(tc, d));
        if let Some(f) = decl
            && !sig.ctor
        {
            label.push_str(" -> ");
            label.push_str(&render(f.ret));
        }
        let doc = sig.decl_span.and_then(|d| doc_at(res, d));
        let active_parameter = if shown.is_empty() {
            None
        } else {
            Some(commas.min(shown.len() as u32 - 1))
        };
        Some(SignatureHelpResult {
            label,
            params,
            active_parameter,
            doc,
        })
    }

    fn semantic_tokens_impl(&self, path: &Path) -> Option<Vec<SemToken>> {
        let a = self.analysis(path)?;
        let res = a.resolution.as_ref()?;
        let file_idx = file_index(res, path)?;
        let unit = &res.package.files[file_idx];
        let root = &unit.parse.root;
        let mut out = Vec::new();
        let mut parents: Vec<&GreenNode> = Vec::new();
        classify_tokens(&a, res, file_idx, root, &mut parents, &mut out);
        out.sort_by_key(|t| t.span.lo);
        Some(out)
    }

    fn inlay_hints_impl(&self, path: &Path, lo: u32, hi: u32) -> Option<Vec<InlayHint>> {
        let a = self.analysis(path)?;
        let res = a.resolution.as_ref()?;
        let tc = a.typecheck.as_ref()?;
        let file_idx = file_index(res, path)?;
        let unit = &res.package.files[file_idx];
        let src = &unit.raw.src;
        let root = &unit.parse.root;
        let mut out = Vec::new();
        // Inferred binder types.
        let mut decls = Vec::new();
        collect_kinds(
            root,
            &[SyntaxKind::LetDecl, SyntaxKind::VarDecl],
            &mut decls,
        );
        for decl in decls {
            for b in wolf_ast::binding_binders(decl) {
                if b.ty.is_some() || b.init.is_none() {
                    continue;
                }
                let Some(pat) = b.pattern else { continue };
                if pat.kind != SyntaxKind::IdentPat {
                    continue;
                }
                let Some(tok) = pat.child_token(SyntaxKind::Ident) else {
                    continue;
                };
                if tok.span.hi < lo || tok.span.hi > hi {
                    continue;
                }
                let Some(tb) = body_containing(tc, file_idx, tok.span.lo) else {
                    continue;
                };
                let Some((_, _, ty)) = tb.locals.iter().find(|(_, s, _)| *s == tok.span) else {
                    continue;
                };
                let text = wolf_sema::types::render(&tb.table, *ty, &|_| Err("_"));
                if text.contains("<error>") {
                    continue;
                }
                out.push(InlayHint {
                    offset: tok.span.hi,
                    label: format!(": {text}"),
                    kind: HintKind::Type,
                });
            }
        }
        // Parameter names at call sites.
        let mut calls = Vec::new();
        collect_kinds(root, &[SyntaxKind::CallExpr], &mut calls);
        for call in calls {
            if call.span.hi < lo || call.span.lo > hi {
                continue;
            }
            let Some(tb) = body_containing(tc, file_idx, call.span.lo) else {
                continue;
            };
            let Some((_, sig)) = tb.calls.iter().find(|(s, _)| *s == call.span) else {
                continue;
            };
            if sig.ctor || sig.c_call || sig.decl_span.is_none() {
                continue;
            }
            let Some(args) = wolf_ast::CallExpr::cast(call).and_then(|c| c.args()) else {
                continue;
            };
            let skip = usize::from(sig.has_self);
            for (i, arg) in args.args().enumerate() {
                let Some(param) = sig.params.get(i + skip) else {
                    break;
                };
                if param.name.is_empty() {
                    continue;
                }
                let Some(value) = arg.value() else { continue };
                if value.span.lo < lo || value.span.lo >= hi {
                    continue;
                }
                if is_bare_name(value, src, &param.name) {
                    continue;
                }
                out.push(InlayHint {
                    offset: value.span.lo,
                    label: format!("{}:", param.name),
                    kind: HintKind::Parameter,
                });
            }
        }
        out.sort_by_key(|h| (h.offset, h.kind == HintKind::Parameter));
        Some(out)
    }
}

// -------------------------------------------------------------- helpers --

/// The checked body of the item containing `offset` in file `file_idx`.
fn body_containing(tc: &Typecheck, file_idx: usize, offset: u32) -> Option<&TypedBody> {
    // Bodies are keyed by (file, decl ordinal, member ordinal); the
    // tree walk that maps an offset to those coordinates is
    // `queries::decl_at`, which needs the root — so instead read the
    // coordinates off the outcomes whose body spans contain `offset`.
    // A `BodyRef` carries no span, so match on the file and take the
    // innermost body whose recorded expressions or locals surround the
    // offset; a body with no recorded span at all cannot be answered.
    let mut best: Option<(&TypedBody, u32)> = None;
    for o in &tc.bodies {
        if o.body.file != file_idx {
            continue;
        }
        let BodyResult::Checked(tb) = &o.result else {
            continue;
        };
        let extent = tb
            .exprs
            .iter()
            .map(|(s, _)| (s.lo, s.hi))
            .chain(tb.locals.iter().map(|(_, s, _)| (s.lo, s.hi)))
            .chain(tb.calls.iter().map(|(s, _)| (s.lo, s.hi)))
            .fold(None, |acc: Option<(u32, u32)>, (lo, hi)| match acc {
                None => Some((lo, hi)),
                Some((a, b)) => Some((a.min(lo), b.max(hi))),
            });
        let Some((lo, hi)) = extent else { continue };
        if lo <= offset && offset <= hi {
            let width = hi - lo;
            if best.is_none_or(|(_, w)| width < w) {
                best = Some((tb, width));
            }
        }
    }
    best.map(|(tb, _)| tb)
}

/// The innermost call expression whose argument list contains
/// `offset` — after its `(`, and before its `)` when it has one.
fn call_at(node: &GreenNode, offset: u32) -> Option<(&GreenNode, &GreenNode)> {
    let mut found = None;
    for child in node.nodes() {
        if child.span.lo <= offset
            && offset <= child.span.hi
            && let Some(hit) = call_at(child, offset)
        {
            found = Some(hit);
        }
    }
    if found.is_some() {
        return found;
    }
    if node.kind != SyntaxKind::CallExpr {
        return None;
    }
    let args = node.child_node(SyntaxKind::ArgList)?;
    let opened = args.child_token(SyntaxKind::LParen)?;
    if offset <= opened.span.lo {
        return None;
    }
    if let Some(close) = args.child_token(SyntaxKind::RParen)
        && offset > close.span.lo
    {
        return None;
    }
    if offset > args.span.hi {
        return None;
    }
    Some((node, args))
}

/// The declared signature whose name token is `decl` — a module fn,
/// an impl method, or a trait method.
fn fn_sig_at(tc: &Typecheck, decl: Span) -> Option<&FnSig> {
    for m in &tc.sigs.modules {
        for sig in m.values() {
            if let ItemSig::Fn(f) = sig
                && f.name_span == decl
            {
                return Some(f);
            }
        }
    }
    for i in &tc.sigs.impls {
        for m in &i.methods {
            if m.name_span == decl {
                return Some(&m.sig);
            }
        }
    }
    for t in tc.sigs.traits.values() {
        for m in &t.methods {
            if m.name_span == decl {
                return Some(&m.sig);
            }
        }
    }
    None
}

/// The `///` comment above the item or member whose name token is
/// `decl` — ONE doc model (s53): what hover and `wolf doc` read.
fn doc_at(res: &Resolution, decl: Span) -> Option<String> {
    let unit = res.package.files.iter().find(|u| u.raw.file == decl.file)?;
    let item = item_with_name(&unit.parse.root, decl)?;
    crate::docs::first_token(item)
        .and_then(|t| crate::docs::outer_doc(t, &unit.raw.src))
        .map(|d| d.text)
}

/// The item (top-level or impl/trait member) whose name token is `name`.
fn item_with_name(root: &GreenNode, name: Span) -> Option<&GreenNode> {
    for item in root.nodes().filter(|n| n.kind.is_item()) {
        if !(item.span.lo <= name.lo && name.hi <= item.span.hi) {
            continue;
        }
        if matches!(item.kind, SyntaxKind::ImplDecl | SyntaxKind::TraitDecl)
            && let Some(m) = item_with_name(item, name)
        {
            return Some(m);
        }
        if item.tokens().any(|t| t.span == name) {
            return Some(item);
        }
    }
    None
}

fn collect_kinds<'a>(node: &'a GreenNode, kinds: &[SyntaxKind], out: &mut Vec<&'a GreenNode>) {
    if kinds.contains(&node.kind) {
        out.push(node);
    }
    for child in node.nodes() {
        collect_kinds(child, kinds, out);
    }
}

/// Is `value` a bare identifier spelled exactly `name`?
fn is_bare_name(value: &GreenNode, src: &[u8], name: &str) -> bool {
    if value.kind != SyntaxKind::PathExpr {
        return false;
    }
    let toks: Vec<&GreenToken> = value.tokens().collect();
    toks.len() == 1 && value.nodes().next().is_none() && toks[0].text(src) == name.as_bytes()
}

/// Walk every token, classifying identifiers through the binding
/// table and keywords through their kinds.
fn classify_tokens<'a>(
    a: &PackageAnalysis,
    res: &Resolution,
    file_idx: usize,
    node: &'a GreenNode,
    parents: &mut Vec<&'a GreenNode>,
    out: &mut Vec<SemToken>,
) {
    parents.push(node);
    for child in &node.children {
        match child {
            Child::Node(n) => classify_tokens(a, res, file_idx, n, parents, out),
            Child::Token(t) => {
                if let Some(tok) = classify_token(a, res, file_idx, t, node, parents) {
                    out.push(tok);
                }
            }
        }
    }
    parents.pop();
}

fn classify_token(
    a: &PackageAnalysis,
    res: &Resolution,
    file_idx: usize,
    t: &GreenToken,
    parent: &GreenNode,
    parents: &[&GreenNode],
) -> Option<SemToken> {
    if t.span.lo == t.span.hi {
        return None;
    }
    if is_keyword_kind(t.kind) {
        return Some(SemToken {
            span: t.span,
            kind: SemKind::Keyword,
            declaration: false,
            readonly: false,
        });
    }
    if t.kind != SyntaxKind::Ident {
        return None;
    }
    let unit = &res.package.files[file_idx];
    // The lexical half.
    if let Some(r) = res.refs[file_idx].iter().find(|r| r.span == t.span) {
        return Some(match &r.target {
            RefTarget::Local(decl) => {
                let (kind, readonly) = local_kind(&unit.parse.root, *decl);
                SemToken {
                    span: t.span,
                    kind,
                    declaration: *decl == t.span,
                    readonly,
                }
            }
            RefTarget::Item { module, name } => {
                let item = res.package.tables[*module].get(name)?;
                let (kind, readonly) = match item.kind {
                    ItemKind::Fn => (SemKind::Function, false),
                    ItemKind::Struct | ItemKind::Enum | ItemKind::Type | ItemKind::Trait => {
                        (SemKind::Type, false)
                    }
                    ItemKind::Const | ItemKind::Let => (SemKind::Variable, true),
                    ItemKind::Var => (SemKind::Variable, false),
                };
                SemToken {
                    span: t.span,
                    kind,
                    declaration: item.name_span == t.span,
                    readonly,
                }
            }
            RefTarget::Module(_) | RefTarget::Std | RefTarget::CNamespace => SemToken {
                span: t.span,
                kind: SemKind::Namespace,
                declaration: false,
                readonly: false,
            },
            RefTarget::Prelude(_) => SemToken {
                span: t.span,
                kind: if in_type_position(parents) {
                    SemKind::Type
                } else if is_callee(parent, parents) {
                    SemKind::Function
                } else {
                    SemKind::Variable
                },
                declaration: false,
                readonly: false,
            },
            RefTarget::BuiltinType(_) | RefTarget::SelfKw => SemToken {
                span: t.span,
                kind: SemKind::Type,
                declaration: false,
                readonly: false,
            },
        });
    }
    // The type-dependent half: member uses, then member declarations.
    let tc = a.typecheck.as_ref()?;
    let decl = tc
        .bodies
        .iter()
        .filter(|o| o.body.file == file_idx)
        .find_map(|o| match &o.result {
            BodyResult::Checked(tb) => tb
                .member_refs
                .iter()
                .find(|(u, _)| *u == t.span)
                .map(|(_, d)| *d),
            _ => None,
        })
        .or_else(|| member_kind(&tc.sigs, t.span).map(|_| t.span))?;
    let kind = member_kind(&tc.sigs, decl)?;
    Some(SemToken {
        span: t.span,
        kind,
        declaration: decl == t.span,
        readonly: false,
    })
}

fn is_keyword_kind(kind: SyntaxKind) -> bool {
    (SyntaxKind::AsKw..=SyntaxKind::WhileKw).contains(&kind) || kind == SyntaxKind::SelfKw
}

/// A `Local`'s token kind and readonly-ness, from where its binder
/// sits: a parameter list, a `var`, or anything else (`let`, a
/// pattern, a loop variable — all immutable bindings).
fn local_kind(root: &GreenNode, decl: Span) -> (SemKind, bool) {
    let mut chain: Vec<&GreenNode> = Vec::new();
    if !ancestors_of(root, decl, &mut chain) {
        return (SemKind::Variable, true);
    }
    // `chain` is outermost-first; the last is the token's own parent.
    for n in chain.iter().rev() {
        match n.kind {
            SyntaxKind::Param => return (SemKind::Parameter, false),
            SyntaxKind::VarDecl => return (SemKind::Variable, false),
            SyntaxKind::LetDecl => return (SemKind::Variable, true),
            SyntaxKind::Block | SyntaxKind::FnDecl => break,
            _ => {}
        }
    }
    (SemKind::Variable, true)
}

/// The node chain down to the node owning the token `span`.
fn ancestors_of<'a>(node: &'a GreenNode, span: Span, chain: &mut Vec<&'a GreenNode>) -> bool {
    if !(node.span.lo <= span.lo && span.hi <= node.span.hi) {
        return false;
    }
    chain.push(node);
    for child in &node.children {
        match child {
            Child::Token(t) if t.span == span => return true,
            Child::Node(n) => {
                if ancestors_of(n, span, chain) {
                    return true;
                }
            }
            Child::Token(_) => {}
        }
    }
    chain.pop();
    false
}

fn in_type_position(parents: &[&GreenNode]) -> bool {
    parents
        .iter()
        .rev()
        .take(3)
        .any(|n| wolf_ast::is_type_kind(n.kind))
}

/// Is `parent` (a path) the callee of the call above it?
fn is_callee(parent: &GreenNode, parents: &[&GreenNode]) -> bool {
    let Some(call) = parents.iter().rev().nth(1) else {
        return false;
    };
    if !matches!(call.kind, SyntaxKind::CallExpr | SyntaxKind::BracketApply) {
        return false;
    }
    call.nodes()
        .next()
        .is_some_and(|first| std::ptr::eq(first, parent))
}

/// What a member declaration's name token declares: a field, a
/// variant, a method (impl or trait).
fn member_kind(sigs: &wolf_sema::SigTables, decl: Span) -> Option<SemKind> {
    for m in &sigs.modules {
        for sig in m.values() {
            match sig {
                ItemSig::Struct(s) if s.fields.iter().any(|f| f.span == decl) => {
                    return Some(SemKind::Property);
                }
                ItemSig::Enum { variants, .. } if variants.iter().any(|v| v.span == decl) => {
                    return Some(SemKind::EnumMember);
                }
                _ => {}
            }
        }
    }
    if sigs
        .impls
        .iter()
        .any(|i| i.methods.iter().any(|m| m.name_span == decl))
        || sigs
            .traits
            .values()
            .any(|t| t.methods.iter().any(|m| m.name_span == decl))
    {
        return Some(SemKind::Function);
    }
    None
}
