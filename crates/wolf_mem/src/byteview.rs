//! The byte-view **lend** (s89, wolf-lang#86) — what a `List[int]`
//! parameter may do with bytes it was handed as a view rather than a
//! copy.
//!
//! # Why this lives in the memory checker
//!
//! s77 made `s.bytes()` the receiver's own `{ptr, len}` and lowered it
//! in place wherever it was consumed — iteration, indexing, and the
//! `len`/`count`/`is_empty`/`get`/`first`/`last` queries. Every other
//! position materialized, and an ARGUMENT is another position, so
//! `std.bytes`' nine functions took a heap copy per call for bytes they
//! only ever read. The fix is not a new type: it is the argument the
//! region checker already makes for regions, one scale down. A view
//! passed to a callee is **lent** for the call — the callee may read
//! it, and it may not outlive the call — and "may not outlive" is a
//! property of the callee's body that this module decides, once, for
//! both consumers ([`crate::check_package`] for the diagnostic, and
//! `wolf_wir::lower` for the lowering).
//!
//! # The three answers, and why only one of them is load-bearing
//!
//! [`Lend::Lendable`] is the only verdict that changes code
//! generation, and it is deliberately the narrow one: EVERY use of the
//! parameter must be one of s77's seven read positions, or a re-lend
//! into another parameter that is itself `Lendable`. Anything this
//! module has not proved is [`Lend::Opaque`] — the caller materializes,
//! which is bit-for-bit the pre-s89 behaviour and therefore always
//! sound. [`Lend::Escapes`] is the subset of "not lendable" where the
//! body PROVABLY makes the value outlive the call (it is returned, or
//! assigned away, or handed on `take`/`mut`); that one materializes
//! too and is W1004, so the copy the analysis PROVED necessary is said
//! once instead of silently costing (s92; through s91 it was E1015, a
//! refusal).
//!
//! Misclassifying an escape as opaque is a missed diagnostic, never
//! unsafety: soundness rests entirely on `Lendable`'s whitelist.
//!
//! # The fix ladder
//!
//! A lend the callee wants to keep is spelled by binding first — `let
//! bs = s.bytes()` materializes (a `let` is not a lend position, and
//! s77's comment already said so), and the bound list may then be
//! passed anywhere. That is the W1004 note.

use std::collections::HashMap;

use wolf_ast::{
    Arg, BracketApply, CallExpr, FnDecl, ForExpr, GreenNode, MemberExpr, ParenExpr, SyntaxKind,
};
use wolf_sema::Package;
use wolf_sema::sig::{FnSig, ItemSig, SigTables};
use wolf_sema::types::{Prim, TyKind, TypeTable};
use wolf_span::Span;

/// How long a re-lend chain this analysis will follow before answering
/// `Opaque` (the caller materializes, which is always correct). Cycles
/// are handled by the in-progress stack rather than by this cap; the
/// cap bounds work on a deep acyclic chain.
const MAX_DEPTH: usize = 16;

/// What a callee does with a `List[int]` parameter offered as a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lend {
    /// Every use is a read s77 models: the view may cross the call.
    Lendable,
    /// A use provably outlives the call. The span is that use.
    Escapes(Span),
    /// Some use is outside the modelled surface: materialize (this is
    /// the pre-s89 behaviour, and it is never wrong — only slower).
    Opaque,
}

/// Is this the type a byte view has — `List[int]`?
pub fn is_byte_list(table: &TypeTable, ty: wolf_sema::types::TyId) -> bool {
    match table.kind(ty) {
        TyKind::List(elem) => matches!(table.kind(*elem), TyKind::Prim(Prim::Int)),
        _ => false,
    }
}

/// `<str>.bytes()` — the syntactic view, with its `str` receiver.
///
/// `expr_ty` answers the checked type of a span; the receiver must be a
/// `str` for this to be a view at all (a `List` receiver named `bytes`
/// is somebody else's method). A `copy`/`move` prefix is NOT stripped:
/// `copy s.bytes()` is the spelling that asks for the materialization
/// on purpose.
pub fn view_recv<'t>(
    e: &'t GreenNode,
    src: &[u8],
    expr_ty: &dyn Fn(Span) -> Option<TyKind>,
) -> Option<&'t GreenNode> {
    if e.kind != SyntaxKind::CallExpr {
        return None;
    }
    let d = CallExpr::cast(e)?;
    let m = MemberExpr::cast(d.callee()?)?;
    if member_name(m, src).as_deref() != Some("bytes") {
        return None;
    }
    let base = m.base()?;
    let recv = unparen(base);
    match expr_ty(recv.span) {
        Some(TyKind::Prim(Prim::Str)) => Some(recv),
        _ => None,
    }
}

/// The lend analysis over a package's function bodies.
pub struct Lender<'a> {
    pkg: &'a Package,
    sigs: &'a SigTables,
    /// Declaration-name span → the `FnDecl` node and its file index.
    /// Built once; a signature is identified by its name span, which is
    /// what every call site already carries (`CallSig::decl_span`).
    decls: HashMap<Span, (&'a GreenNode, usize)>,
    /// Function name → every signature declaring it. Both consumers ask
    /// per call site, so the lookup is indexed rather than scanned.
    by_name: HashMap<&'a str, Vec<&'a FnSig>>,
}

impl<'a> Lender<'a> {
    pub fn new(pkg: &'a Package, sigs: &'a SigTables) -> Lender<'a> {
        let mut decls = HashMap::new();
        for (fi, file) in pkg.files.iter().enumerate() {
            index_decls(&file.parse.root, fi, &mut decls);
        }
        let mut by_name: HashMap<&str, Vec<&FnSig>> = HashMap::new();
        for module in &sigs.modules {
            for (n, sig) in module {
                if let ItemSig::Fn(f) = sig {
                    by_name.entry(n.as_str()).or_default().push(f);
                }
            }
        }
        Lender {
            pkg,
            sigs,
            decls,
            by_name,
        }
    }

    /// May parameter `ix` of `fsig` receive a byte view?
    pub fn param(&self, fsig: &FnSig, ix: usize) -> Lend {
        self.param_at(fsig, ix, &[])
    }

    /// `stack`: the (declaration, parameter) pairs whose verdict is
    /// already being computed further up. Re-entering one is a CYCLE —
    /// a recursive byte walker re-lending its own view — and the answer
    /// there is `Lendable`, the greatest fixed point. That is sound
    /// because the property is a whitelist over the body's own uses: a
    /// cycle adds no use, so it can turn nothing into an escape, and
    /// assuming the loop lends never admits a use the whitelist would
    /// have rejected. (Assuming `Opaque` instead would only lose the
    /// optimization on every self-recursive walk, which is most of
    /// them.)
    fn param_at(&self, fsig: &FnSig, ix: usize, stack: &[(Span, usize)]) -> Lend {
        if stack.contains(&(fsig.name_span, ix)) {
            return Lend::Lendable;
        }
        if stack.len() >= MAX_DEPTH {
            return Lend::Opaque;
        }
        let Some(p) = fsig.params.get(ix) else {
            return Lend::Opaque;
        };
        // A view is read-only and word-shaped: only the default `read`
        // mode can take one. `mut` would need a place to write back to
        // and `take` is an ownership transfer — both are escapes by
        // construction, and both are refused at the declaration rather
        // than diagnosed per call site.
        if p.mode.is_some() || !is_byte_list(&self.sigs.table, p.ty) {
            return Lend::Opaque;
        }
        let Some((decl, file)) = self.decls.get(&fsig.name_span) else {
            return Lend::Opaque;
        };
        let Some(d) = FnDecl::cast(decl) else {
            return Lend::Opaque;
        };
        let Some(body) = d.body() else {
            return Lend::Opaque;
        };
        let src = &self.pkg.files[*file].raw.src;
        let name = p.name.as_str();
        // Shadowing is a refusal, not a puzzle: if anything in the body
        // re-binds the parameter's name, occurrences below that point
        // mean a different value and the whitelist would be reading the
        // wrong ones.
        if rebinds(body.syntax(), src, name) {
            return Lend::Opaque;
        }
        let mut inner: Vec<(Span, usize)> = stack.to_vec();
        inner.push((fsig.name_span, ix));
        let mut scan = Scan {
            lender: self,
            src,
            name,
            stack: &inner,
            escape: None,
            opaque: false,
        };
        let tail = body.trailing_expr().map(|t| t.span);
        scan.block(body.syntax(), tail);
        match (scan.escape, scan.opaque) {
            (Some(s), _) => Lend::Escapes(s),
            (None, true) => Lend::Opaque,
            (None, false) => Lend::Lendable,
        }
    }

    /// The unique `(module, sig)` a bare callee name resolves to, or
    /// `None` when the name is a builtin, ambiguous, or not a function.
    /// A builtin is deliberately not lendable: `str_from_utf8`, `print`
    /// and friends consume a real list, and modelling each one is the
    /// std facade's job rather than this analysis's.
    pub(crate) fn callee_sig(&self, name: &str, decl_span: Option<Span>) -> Option<&'a FnSig> {
        let cands = self.by_name.get(name)?;
        match decl_span {
            // The declaration locus disambiguates two modules declaring
            // one name (issue #26), exactly as it does at lowering.
            Some(ds) => cands.iter().copied().find(|f| f.name_span == ds),
            None => match cands.as_slice() {
                [one] => Some(one),
                _ => None,
            },
        }
    }
}

// ---------------------------------------------- W1004, the degraded lend ----

/// Walk one checked body's call sites and report every byte view lent
/// to a parameter the callee makes outlive the call ([mem.str.view.lend],
/// W1004).
///
/// Only PROVABLE escapes are reported. A parameter this analysis cannot
/// classify materializes at the call site instead, which is what the
/// language did before views crossed calls at all — and since s92 an
/// escape materializes too. The lowering never lends anything but a
/// `Lendable` parameter (`wolf_wir::lower`, the `view_mask`), so both
/// `Opaque` and `Escapes` compile by the pre-s89 copy; the difference
/// is that an escape is the one case the analysis has PROVED the copy
/// was needed, so it says so, once, and names the fix. Through s91 this
/// was E1015, a refusal; #107/#108 ruled that refusing a program with a
/// safe compilation is not this language's style.
pub fn check_body(
    lender: &Lender<'_>,
    pkg: &Package,
    tb: &wolf_sema::TypedBody,
    body: &wolf_sema::BodyRef,
    diags: &mut Vec<wolf_diag::Diagnostic>,
) {
    let root = &pkg.files[body.file].parse.root;
    let Some(node) = root.nodes().filter(|n| n.kind.is_item()).nth(body.decl) else {
        return;
    };
    let node = match body.member {
        None => node,
        Some(mi) => match node.nodes().filter(|n| n.kind.is_item()).nth(mi) {
            Some(inner) => inner,
            None => return,
        },
    };
    let src = &pkg.files[body.file].raw.src;
    let calls: HashMap<Span, &wolf_sema::check::CallSig> =
        tb.calls.iter().map(|(s, c)| (*s, c)).collect();
    let tys: HashMap<Span, wolf_sema::types::TyId> =
        tb.exprs.iter().map(|(s, t)| (*s, *t)).collect();
    let expr_ty = |sp: Span| -> Option<TyKind> {
        let mut id = *tys.get(&sp)?;
        // The same wrapper strip lowering applies before it decides a
        // view: `distinct str` is still a str's bytes.
        for _ in 0..16 {
            match tb.table.kind(id) {
                TyKind::Distinct(t) | TyKind::Wrapping(t) => id = *t,
                k => return Some(k.clone()),
            }
        }
        None
    };
    walk_calls(node, src, &calls, &expr_ty, lender, diags);
}

fn walk_calls(
    n: &GreenNode,
    src: &[u8],
    calls: &HashMap<Span, &wolf_sema::check::CallSig>,
    expr_ty: &dyn Fn(Span) -> Option<TyKind>,
    lender: &Lender<'_>,
    diags: &mut Vec<wolf_diag::Diagnostic>,
) {
    if n.kind == SyntaxKind::CallExpr
        && let Some(cs) = calls.get(&n.span)
        && !cs.has_self
        && !cs.ctor
        && !cs.c_call
        && let Some(d) = CallExpr::cast(n)
        && let Some(sig) = lender.callee_sig(&cs.callee, cs.decl_span)
    {
        for (i, a) in d.args().into_iter().flat_map(|l| l.args()).enumerate() {
            let Some(v) = Arg::value(a) else { continue };
            if a.mode().is_some() || view_recv(v, src, expr_ty).is_none() {
                continue;
            }
            if let Lend::Escapes(keep) = lender.param(sig, i) {
                diags.push(w1004(&cs.callee, sig, i, v.span, keep));
            }
        }
    }
    for c in n.nodes() {
        walk_calls(c, src, calls, expr_ty, lender, diags);
    }
}

fn w1004(callee: &str, sig: &FnSig, ix: usize, lend: Span, keep: Span) -> wolf_diag::Diagnostic {
    let pname = sig
        .params
        .get(ix)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| format!("#{ix}"));
    let mut d = wolf_diag::Diagnostic::warning(
        wolf_diag::codes::W1004,
        lend,
        format!(
            "these bytes were offered to `{callee}` as a LEND, but `{callee}` keeps them past \
             the call — so they were copied instead"
        ),
    )
    .with_label("a byte view of the string's own storage; materialized to a `List[int]` here")
    .with_secondary(keep, format!("`{pname}` outlives the call here"));
    if let Some(p) = sig.params.get(ix) {
        d = d.with_secondary(p.span, format!("`{pname}` is declared here"));
    }
    d.with_note(
        "the program means what it meant; it paid one copy the lend was meant to save. If \
         the copy is what you wanted, bind the bytes first — `let bs = s.bytes()` — and \
         pass `bs`; that costs exactly what this call costs and says so. If it is not, the \
         callee is the thing to change: a callee that only reads its parameter lends for \
         free. A LENT view costs nothing and lasts only for the call. (This shape was \
         refused as E1015 before the copy became the answer; that code is retired.)"
            .to_string(),
    )
}

/// The recursive body walk. `escape` wins over `opaque` (it is the
/// stronger, diagnosable answer); both stop the lend.
struct Scan<'a, 'b> {
    lender: &'b Lender<'a>,
    src: &'b [u8],
    name: &'b str,
    /// The in-progress `(declaration, parameter)` chain (see
    /// [`Lender::param_at`]).
    stack: &'b [(Span, usize)],
    escape: Option<Span>,
    opaque: bool,
}

impl Scan<'_, '_> {
    fn mark_escape(&mut self, span: Span) {
        if self.escape.is_none() {
            self.escape = Some(span);
        }
    }

    /// A block: every child, with the trailing expression carrying the
    /// escaping context (a fn body's tail IS its return).
    fn block(&mut self, n: &GreenNode, tail: Option<Span>) {
        for c in n.nodes() {
            let escaping = tail.is_some()
                && c.kind == SyntaxKind::ExprStmt
                && wolf_ast::ExprStmt::cast(c)
                    .and_then(|s| s.expr())
                    .map(|e| e.span)
                    == tail;
            self.node(c, escaping);
        }
    }

    /// One node. `escaping`: a bare occurrence of the parameter here
    /// leaves the call (a return, a tail, an assignment's right side).
    fn node(&mut self, n: &GreenNode, escaping: bool) {
        if self.escape.is_some() {
            return;
        }
        match n.kind {
            // --- the seven read positions ------------------------
            SyntaxKind::BracketApply => {
                let Some(d) = BracketApply::cast(n) else {
                    return self.children(n, false);
                };
                if d.callee().is_some_and(|r| self.is_param(r)) {
                    // `v[i]`: the index still gets walked.
                    for a in d.args().into_iter().flat_map(|l| l.args()) {
                        if let Some(v) = Arg::value(a) {
                            self.node(v, false);
                        }
                    }
                    return;
                }
                self.children(n, false);
            }
            SyntaxKind::MemberExpr => {
                let Some(m) = MemberExpr::cast(n) else {
                    return self.children(n, false);
                };
                if m.base().is_some_and(|b| self.is_param(b)) {
                    // `v.len` is the only field a view has; anything
                    // else is a shape this tier does not model.
                    if member_name(m, self.src).as_deref() != Some("len") {
                        self.opaque = true;
                    }
                    return;
                }
                self.children(n, false);
            }
            SyntaxKind::CallExpr => self.call(n, escaping),
            SyntaxKind::ForExpr => {
                let Some(d) = ForExpr::cast(n) else {
                    return self.children(n, false);
                };
                // The iterable is a read position; the body is not
                // (and the loop variable is a byte, not the view).
                for c in n.nodes() {
                    if d.iterable().is_some_and(|it| std::ptr::eq(it, c)) && self.is_param(c) {
                        continue;
                    }
                    self.node(c, false);
                }
            }
            // --- the escaping contexts ---------------------------
            SyntaxKind::ReturnExpr => self.children(n, true),
            SyntaxKind::AssignStmt => {
                // The right-hand side leaves this expression for a
                // place that may well outlive the call.
                let kids: Vec<&GreenNode> = n.nodes().collect();
                for (i, c) in kids.iter().enumerate() {
                    self.node(c, i + 1 == kids.len());
                }
            }
            // Tail-position pass-through: a value in the last position
            // of these is the enclosing expression's value.
            SyntaxKind::ParenExpr | SyntaxKind::ExprStmt => self.children(n, escaping),
            SyntaxKind::Block => {
                let tail = wolf_ast::Block::cast(n)
                    .and_then(|b| b.trailing_expr())
                    .map(|t| t.span);
                if escaping {
                    self.block(n, tail);
                } else {
                    self.children(n, false);
                }
            }
            SyntaxKind::IfExpr | SyntaxKind::MatchExpr | SyntaxKind::MatchArm => {
                self.children(n, escaping)
            }
            // --- a bare occurrence -------------------------------
            SyntaxKind::PathExpr => {
                if self.is_param(n) {
                    if escaping {
                        self.mark_escape(n.span);
                    } else {
                        self.opaque = true;
                    }
                }
            }
            _ => self.children(n, false),
        }
    }

    fn children(&mut self, n: &GreenNode, escaping: bool) {
        for c in n.nodes() {
            self.node(c, escaping);
        }
    }

    /// A call: the method family on a view, a re-lend into another
    /// function, or neither.
    fn call(&mut self, n: &GreenNode, escaping: bool) {
        let Some(d) = CallExpr::cast(n) else {
            return self.children(n, false);
        };
        // `v.count()` and friends.
        if let Some(callee) = d.callee()
            && callee.kind == SyntaxKind::MemberExpr
            && let Some(m) = MemberExpr::cast(callee)
            && m.base().is_some_and(|b| self.is_param(b))
        {
            match member_name(m, self.src).as_deref() {
                Some("len" | "count" | "is_empty" | "get" | "first" | "last") => {}
                _ => self.opaque = true,
            }
            for a in d.args().into_iter().flat_map(|l| l.args()) {
                if let Some(v) = Arg::value(a) {
                    self.node(v, false);
                }
            }
            return;
        }
        // A plain call: an argument that IS the parameter re-lends it,
        // and the callee's own verdict decides.
        let callee_name = d
            .callee()
            .filter(|c| c.kind == SyntaxKind::PathExpr)
            .map(|c| text(self.src, c.span));
        for (i, a) in d.args().into_iter().flat_map(|l| l.args()).enumerate() {
            let Some(v) = Arg::value(a) else { continue };
            if !self.is_param(v) {
                self.node(v, false);
                continue;
            }
            // `f(mut v)` / `f(take v)` hand the callee something it may
            // keep or replace: an escape at the argument, spelled.
            if a.mode().is_some() {
                self.mark_escape(v.span);
                return;
            }
            let Some(name) = callee_name.as_deref() else {
                self.opaque = true;
                continue;
            };
            let Some(sig) = self.lender.callee_sig(name, None) else {
                self.opaque = true;
                continue;
            };
            match self.lender.param_at(sig, i, self.stack) {
                Lend::Lendable => {}
                Lend::Escapes(_) => self.mark_escape(v.span),
                Lend::Opaque => self.opaque = true,
            }
        }
        // A call in tail position returns its own result, not the
        // parameter — nothing further escapes through the callee node.
        let _ = escaping;
    }

    /// Is this expression exactly the parameter's name?
    fn is_param(&self, e: &GreenNode) -> bool {
        let e = unparen(e);
        e.kind == SyntaxKind::PathExpr && text(self.src, e.span) == self.name
    }
}

// ------------------------------------------------------------ helpers ----

fn text(src: &[u8], span: Span) -> String {
    String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize])
        .trim()
        .to_string()
}

fn member_name(m: MemberExpr<'_>, src: &[u8]) -> Option<String> {
    m.member().map(|t| text(src, t.span))
}

fn unparen(e: &GreenNode) -> &GreenNode {
    if e.kind == SyntaxKind::ParenExpr {
        ParenExpr::cast(e).and_then(|p| p.expr()).unwrap_or(e)
    } else {
        e
    }
}

/// Does anything in this subtree bind `name` again?
fn rebinds(n: &GreenNode, src: &[u8], name: &str) -> bool {
    let binder = matches!(
        n.kind,
        SyntaxKind::LetDecl
            | SyntaxKind::VarDecl
            | SyntaxKind::ConstDecl
            | SyntaxKind::IdentPat
            | SyntaxKind::Param
    );
    if binder
        && n.tokens()
            .any(|t| t.kind == SyntaxKind::Ident && text(src, t.span) == name)
    {
        return true;
    }
    n.nodes().any(|c| rebinds(c, src, name))
}

/// Index every `fn` declaration by its name span (module items and
/// inherent impl methods alike).
fn index_decls<'a>(
    root: &'a GreenNode,
    file: usize,
    out: &mut HashMap<Span, (&'a GreenNode, usize)>,
) {
    for item in root.nodes().filter(|n| n.kind.is_item()) {
        record(item, file, out);
        if item.kind == SyntaxKind::ImplDecl {
            for m in item.nodes().filter(|n| n.kind.is_item()) {
                record(m, file, out);
            }
        }
    }
}

fn record<'a>(item: &'a GreenNode, file: usize, out: &mut HashMap<Span, (&'a GreenNode, usize)>) {
    if item.kind != SyntaxKind::FnDecl {
        return;
    }
    if let Some(name) = FnDecl::cast(item).and_then(|d| d.name()) {
        out.insert(name.span, (item, file));
    }
}
