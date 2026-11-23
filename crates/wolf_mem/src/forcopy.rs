//! A `for` binding's own value (wolf-lang#1169) — when a `for` over a list
//! may bind the walked element itself, and when the binding must be a deep
//! copy of it.
//!
//! # Why
//!
//! `[gram.pat.let]`: a name a `for` header binds "holds its own value, moved
//! or copied out of what was matched, never a place in it, so no write to it
//! could reach the scrutinee"; `[mem.iter.for]` walks a copy of the iterable
//! and binds each yielded value with a `let`. A machine that binds the
//! element itself (the list's own header word, an `Rc` id in the checked
//! machine) is right exactly when nothing can write through the binding or
//! keep it past the iteration — then the element and a copy of it cannot be
//! told apart. Until wolf-lang#1169 every compiling machine bound the element
//! itself unconditionally, so `for row in rows { row[0] = 0 }` wrote `rows`,
//! and so did every way the element could leave the binding: a `let` or
//! `var` move, a struct built from it, a `mut` lend, a `take` argument, an
//! assignment out of the loop, a `return`.
//!
//! # The rule, and which way it errs
//!
//! [`needs_own_copy`] answers `false` (bind the element itself) only when it
//! can show the body READS the bound names and everything derived from them,
//! and `true` (deep-copy the element at the bind) for anything else,
//! including every shape it does not know. A wrong `true` costs one copy an
//! iteration, never an answer; a wrong `false` is the bug. Derived names are
//! the names bound, in the body, from an expression that spells a bound name:
//! a `let`/`var`, a `match` arm on it, a nested `for` over it, an `else`
//! handler (to a fixpoint, by spelling — never narrower than an alias).
//!
//! A use is a read when the value goes nowhere that keeps it: an operand, a
//! condition, an interpolation, an index or a field read, a plain call
//! argument (a `read` parameter is deeply immutable for the call, E1014, and
//! cannot be returned or moved out, E1002; a builtin that keeps a plain
//! argument copies it in, `[mem.region.edge.elem]`: `push` copies any plain
//! element, an index store copies a place), the iterable of a nested `for`,
//! the initializer of a `let` (the new name is derived and answers for
//! itself). A call that escapes keeps its plain arguments read when it is a
//! call by name to a declared function ([`declared_call`]): its answer cannot
//! hold a `read` argument (a `read` parameter moved anywhere that outlives the
//! call is E1002; a walk over one binds an element this predicate copies
//! before it leaves). Every other call (a method, builtin or declared; a
//! constructor; a fn value; a C call) passes its own position on to its plain
//! arguments. A pool's `init` keeps the value it is handed as it is, so its
//! plain arguments escape.
//! Everything else asks for the copy: a store through a derived
//! name (an index or field place rooted at one), `mut`/`take`/`move`/`&`/
//! `shared` on one, a plain assignment, a `return` or a `break` carrying one,
//! a closure, `spawn` or other construct that names one, `unsafe` among
//! them. A struct, tuple or list literal passes its own position to its
//! parts: one that escapes carries them out, one that is read (bound by
//! `let`, pushed, passed to a function) reads them. A `region`, `in` or
//! `scope` block is walked as the block it is: its region holds the block's
//! own allocations, never the walked element.
//!
//! A body that writes through the binding and lets nothing escape binds a
//! copy no use keeps past the iteration ([`OwnCopy::Local`]): a machine makes
//! it in a region of the iteration's own and frees that region when the
//! iteration ends. The writes are a store through a derived place and a
//! builtin's `mut` receiver whose answer goes nowhere and whose other
//! arguments are plain; a `mut` or `take` argument of any call, and a
//! receiver whose answer escapes (`last = (mut row).pop()`), are escapes. An
//! element type that may hold a `str` keeps its copy ([`reaches_str`]).
//!
//! A value whose type is a primitive (`int`, `bool`, `str`, …) is its own
//! copy wherever it goes (`[mem.tier0.move.3]`; a `str`'s bytes are
//! immutable), so an escape of one is a read. The type is the checker's, read
//! at the expression's span through the consumer's `expr_ty` (`ubcheck`'s
//! `expr_tys`, the lowering's `expr_sema_ty`); a span with no recorded type,
//! a generic (`Rigid`), an unsolved variable, a distinct type or any other
//! non-primitive answers "may reach the heap" and asks for the copy.
//!
//! Both machines call this one function — `ubcheck`'s `eval_for` and
//! `wolf_wir::lower`'s `lower_for_list` — and copy with their own deep copy
//! (`deep_copy` there, `deep_copy_in` here, which agree by construction,
//! wolf-lang#384), so the two can never disagree about which loops copy.

use std::collections::HashSet;

use wolf_ast::{
    Arg, AssignStmt, Block, BracketApply, BreakExpr, CallExpr, ElseExpr, ForExpr, GreenNode,
    MatchExpr, MemberExpr, ParenExpr, PrefixExpr, ReturnExpr, StructLit, SyntaxKind,
};
use wolf_sema::check::CallSig;
use wolf_sema::sig::{ItemSig, SigTables};
use wolf_sema::types::{Prim, TyId, TyKind, TypeTable};
use wolf_span::Span;

/// What a `for` loop binds each element to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OwnCopy {
    /// The element itself: the body only reads what the header binds.
    Element,
    /// A deep copy nothing keeps past its iteration: the body writes
    /// through the binding (a store, a builtin's `mut` receiver) and lets
    /// no part of it escape, and the element's type holds no `str`, so a
    /// machine may make the copy in a region of its own and free that
    /// region when the iteration ends.
    Local,
    /// A deep copy that may outlive its iteration.
    Kept,
}

/// Must the `for` loop `d` bind a deep copy of each element (`true`), or may
/// it bind the element itself (`false`)? `false` only when the body provably
/// only reads what the header binds; see the module documentation.
pub fn needs_own_copy(
    d: ForExpr<'_>,
    src: &[u8],
    expr_ty: &dyn Fn(Span) -> Option<TyKind>,
    declared: &dyn Fn(Span) -> bool,
) -> bool {
    own_copy(d, src, expr_ty, declared, true) != OwnCopy::Element
}

/// The binding the `for` loop `d` needs: the element itself, a copy freed
/// with its iteration, or a kept copy. Errs only toward the more owned
/// answer. `holds_str` is [`reaches_str`] of the element's type: a copy
/// that may hold a `str` is never freed with its iteration, because a
/// `str` is a primitive this walk lets go anywhere (`[mem.tier0.move.3]`),
/// and a `str` stored into the copy may have its bytes in the copy's own
/// region (a list's growth and its `str` pushes land in the region the
/// list was born in).
pub fn own_copy(
    d: ForExpr<'_>,
    src: &[u8],
    expr_ty: &dyn Fn(Span) -> Option<TyKind>,
    declared: &dyn Fn(Span) -> bool,
    holds_str: bool,
) -> OwnCopy {
    let Some(pat) = d.pattern() else {
        return OwnCopy::Element;
    };
    let mut names = HashSet::new();
    idents(pat, src, &mut names);
    if names.is_empty() {
        return OwnCopy::Element;
    }
    let Some(body) = d.body() else {
        return OwnCopy::Element;
    };
    let body = body.syntax();
    loop {
        let before = names.len();
        derive(body, src, &mut names);
        if names.len() == before {
            break;
        }
    }
    let mut w = Walk {
        src,
        names: &names,
        expr_ty,
        declared,
        wrote: false,
        escaped: false,
    };
    w.block(body, Pos::Read);
    match (w.wrote, w.escaped) {
        (_, true) => OwnCopy::Kept,
        (true, false) if holds_str => OwnCopy::Kept,
        (true, false) => OwnCopy::Local,
        (false, false) => OwnCopy::Element,
    }
}

/// Can a value of `ty` (its ids in `table`) hold a `str`, at any depth?
/// Errs toward yes: a generic or recursive shape, a fn value, a cell, a
/// pointer, a row or any kind this does not name answers yes.
pub fn reaches_str(sigs: &SigTables, table: &TypeTable, ty: TyId, depth: u32) -> bool {
    if depth > 16 {
        return true;
    }
    let inner = |t: TyId| reaches_str(sigs, table, t, depth + 1);
    let declared = |t: TyId| reaches_str(sigs, &sigs.table, t, depth + 1);
    match table.kind(ty) {
        TyKind::Never | TyKind::Unit => false,
        TyKind::Prim(p) => matches!(p, Prim::Str),
        TyKind::Wrapping(t)
        | TyKind::Distinct(t)
        | TyKind::List(t)
        | TyKind::Pool(t)
        | TyKind::Range(t) => inner(*t),
        TyKind::Map(k, v) => inner(*k) || inner(*v),
        TyKind::Tuple(es) => es.iter().any(|&e| inner(e)),
        TyKind::Nominal { module, name, .. } => match sigs.get(*module as usize, name) {
            Some(ItemSig::Struct(ss)) => {
                !ss.generics.is_empty() || ss.fields.iter().any(|f| declared(f.ty))
            }
            Some(ItemSig::Enum {
                generics, variants, ..
            }) => {
                !generics.is_empty()
                    || variants
                        .iter()
                        .flat_map(|v| v.payload.iter())
                        .any(|&p| declared(p))
            }
            Some(ItemSig::Distinct { base, .. }) => declared(*base),
            _ => true,
        },
        _ => true,
    }
}

/// Is `c` a call to a declared function, by name or as a qualified trait
/// method (`f(x)`, `m.f(x)`, `T.make(x)`, `Tr.m(x)`), whose body the checker
/// holds to the `read` parameter law (E1002)? Not a method call (`x.m()`,
/// builtin or declared: its receiver is a parameter of its own), a
/// constructor, a fn value (no declaration), a nested fn by its moded name,
/// a C or a variadic call.
pub fn declared_call(c: &CallSig) -> bool {
    !c.has_self && c.decl_span.is_some() && !c.ctor && !c.c_call && !c.variadic
}

/// wolf-lang#1177 (part 2): is `push`'s operand `e` FRESH — a value no
/// other place can reach once the push holds it — so the push may store it
/// as it is instead of deep-copying it? `[mem.region.edge.elem]` states the
/// copy's cost as paid once per plain push, a ceiling, and a push of a value
/// nobody else holds needs no copy to keep the list's element independent.
///
/// `true` only for, parentheses seen through:
/// - a struct, tuple or list literal: it owns its parts. A local, a field or
///   an element it names is moved into it (a later use is E1001 on every
///   compiling machine, a `use-after-move` trap on lupin);
/// - `copy e`, `copy region { … }` among them: its answer is a new value;
/// - a call to a declared function ([`declared_call`]) whose every argument
///   is a primitive (`expr_ty`): nothing it could hand back was reachable
///   before the call, since module state holds no heap value
///   (`[mem.static.3]`) and its arguments hold none. A declared call with a
///   heap argument is not fresh while wolf-lang#1184 stands: a function may
///   answer `xs.first() else [..]`, the caller's own element, and E1002 does
///   not see it.
///
/// `false` for everything else, which keeps the copy: a place, a method call
/// (`get` and `first` may hand out the receiver's own element), an `else`,
/// `if`, `match` or block expression (each may yield a place's value), a
/// constructor, a fn value's call, a C call. A wrong `false` costs one copy;
/// a wrong `true` is a second writable path to one value, so the list is
/// short on purpose. `declared` answers whether the call at a span is a
/// [`declared_call`], from the consumer's own call records; `expr_ty` is the
/// checker's type at a span, as [`needs_own_copy`] reads it.
pub fn fresh_operand(
    e: &GreenNode,
    declared: &dyn Fn(Span) -> bool,
    expr_ty: &dyn Fn(Span) -> Option<TyKind>,
) -> bool {
    match e.kind {
        SyntaxKind::ParenExpr => {
            // `(mut x)` and `(take x)` are modes, not values.
            !e.tokens()
                .any(|t| matches!(t.kind, SyntaxKind::MutKw | SyntaxKind::TakeKw))
                && ParenExpr::cast(e)
                    .and_then(|p| p.expr())
                    .is_some_and(|inner| fresh_operand(inner, declared, expr_ty))
        }
        SyntaxKind::StructLit | SyntaxKind::TupleExpr | SyntaxKind::ListLit => true,
        SyntaxKind::PrefixExpr => PrefixExpr::cast(e)
            .and_then(|p| p.op())
            .is_some_and(|t| t.kind == SyntaxKind::CopyKw),
        SyntaxKind::CallExpr => {
            declared(e.span)
                && CallExpr::cast(e).is_some_and(|d| {
                    d.args().into_iter().flat_map(|l| l.args()).all(|a| {
                        a.value().is_some_and(|v| {
                            matches!(
                                expr_ty(v.span),
                                Some(TyKind::Prim(_) | TyKind::Unit | TyKind::Never)
                            )
                        })
                    })
                })
        }
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pos {
    /// The value is read where it stands, or bound to a derived name.
    Read,
    /// The value goes somewhere that may keep it or write it.
    Escape,
}

struct Walk<'s, 'n, 'f> {
    src: &'s [u8],
    names: &'n HashSet<String>,
    expr_ty: &'f dyn Fn(Span) -> Option<TyKind>,
    /// Is the call at this span one to a declared function ([`declared_call`]
    /// of the checker's record there)?
    declared: &'f dyn Fn(Span) -> bool,
    /// A store through a derived name, or a `mut` lend of one.
    wrote: bool,
    /// Any other use that may keep or hand on a derived value.
    escaped: bool,
}

impl Walk<'_, '_, '_> {
    fn prim(&self, span: Span) -> bool {
        matches!(
            (self.expr_ty)(span),
            Some(TyKind::Prim(_) | TyKind::Unit | TyKind::Never)
        )
    }

    fn spells(&self, n: &GreenNode) -> bool {
        spells(n, self.src, self.names)
    }

    fn block(&mut self, n: &GreenNode, pos: Pos) {
        let Some(b) = Block::cast(n) else {
            return self.expr(n, pos);
        };
        let tail = b.trailing_expr().map(|e| e.span);
        for s in b.statements() {
            self.stmt(s, tail, pos);
        }
    }

    fn stmt(&mut self, s: &GreenNode, tail: Option<Span>, pos: Pos) {
        match s.kind {
            SyntaxKind::ExprStmt => {
                for e in s.nodes() {
                    let p = if Some(e.span) == tail { pos } else { Pos::Read };
                    self.expr(e, p);
                }
            }
            // The initializer is bound to a new name, which `derive` made
            // answer for itself.
            SyntaxKind::LetDecl | SyntaxKind::VarDecl => {
                for b in wolf_ast::binding_binders(s) {
                    if let Some(init) = b.init {
                        self.expr(init, Pos::Read);
                    }
                }
            }
            SyntaxKind::ConstDecl | SyntaxKind::AssumeStmt | SyntaxKind::DeferStmt => {
                for e in s.nodes() {
                    self.expr(e, Pos::Read);
                }
            }
            SyntaxKind::AssignStmt => self.assign(s),
            _ => {
                if self.spells(s) {
                    self.escaped = true;
                }
            }
        }
    }

    fn assign(&mut self, s: &GreenNode) {
        let Some(d) = AssignStmt::cast(s) else {
            self.escaped |= self.spells(s);
            return;
        };
        let compound = d.op().is_some_and(|t| t.kind != SyntaxKind::Eq);
        let mut index_store = false;
        if let Some(place) = d.place() {
            let place = unparen(place);
            if place.kind != SyntaxKind::PathExpr {
                // A store through a place: rooted at a derived name, it
                // writes that name's value.
                if root(place).is_some_and(|r| self.names.contains(&text(self.src, r.span))) {
                    self.wrote = true;
                }
                index_store = place.kind == SyntaxKind::BracketApply;
                self.place_parts(place);
            }
        }
        if let Some(v) = d.value() {
            // `xs[i] = v` copies a place `v` in (wolf-lang#438,
            // `[mem.region.edge.elem]`); a temporary, a field store and a
            // plain assignment keep the value itself.
            let copied = index_store && !d.takes() && is_place(unparen(v));
            let p = if compound || copied {
                Pos::Read
            } else {
                Pos::Escape
            };
            self.expr(v, p);
        }
    }

    /// The reads inside a store's place: its index expressions.
    fn place_parts(&mut self, place: &GreenNode) {
        match place.kind {
            SyntaxKind::BracketApply => {
                let b = BracketApply::cast(place).expect("kind");
                if let Some(c) = b.callee() {
                    self.place_parts(unparen(c));
                }
                for a in b.args().into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = a.value() {
                        self.expr(v, Pos::Read);
                    }
                }
            }
            SyntaxKind::MemberExpr => {
                if let Some(b) = MemberExpr::cast(place).and_then(|m| m.base()) {
                    self.place_parts(unparen(b));
                }
            }
            _ => {}
        }
    }

    fn expr(&mut self, e: &GreenNode, pos: Pos) {
        if self.escaped {
            return;
        }
        let pos = if pos == Pos::Escape && self.prim(e.span) {
            Pos::Read
        } else {
            pos
        };
        match e.kind {
            SyntaxKind::PathExpr => {
                if pos == Pos::Escape && self.names.contains(&text(self.src, e.span)) {
                    self.escaped = true;
                }
            }
            SyntaxKind::LiteralExpr | SyntaxKind::ContinueExpr => {}
            SyntaxKind::ParenExpr => {
                let moded = e
                    .tokens()
                    .any(|t| matches!(t.kind, SyntaxKind::MutKw | SyntaxKind::TakeKw));
                let p = if moded { Pos::Escape } else { pos };
                if let Some(inner) = ParenExpr::cast(e).and_then(|x| x.expr()) {
                    self.expr(inner, p);
                }
            }
            SyntaxKind::PrefixExpr => {
                let d = PrefixExpr::cast(e).expect("kind");
                let p = match d.op().map(|t| t.kind) {
                    Some(SyntaxKind::CopyKw) => Pos::Read,
                    Some(SyntaxKind::Minus | SyntaxKind::Not | SyntaxKind::Star) => Pos::Read,
                    _ => Pos::Escape,
                };
                if let Some(o) = d.operand() {
                    self.expr(o, p);
                }
            }
            SyntaxKind::BinExpr
            | SyntaxKind::CastExpr
            | SyntaxKind::RangeExpr
            | SyntaxKind::FromEndExpr
            | SyntaxKind::StringExpr
            | SyntaxKind::Interp => {
                for c in e.nodes() {
                    self.expr(c, Pos::Read);
                }
            }
            SyntaxKind::TryExpr => {
                for c in e.nodes() {
                    self.expr(c, pos);
                }
            }
            SyntaxKind::CallExpr => {
                let d = CallExpr::cast(e).expect("kind");
                // A plain argument is a `read` parameter's (E1014, E1002)
                // or a keeping builtin's, which copies it in. Where the call
                // itself escapes, what it answers may hold its arguments:
                // - a declared function's answer never does. A `read`
                //   parameter moved anywhere that outlives the call is E1002
                //   (returned bare, in a struct or as an element, or stored),
                //   and the one other way out, a walk over the parameter,
                //   binds an element this predicate copies before it leaves.
                //   So its plain arguments are read.
                // - a builtin's answer (`get`, `first`) may be an element of
                //   its receiver, a constructor's holds its arguments, and a
                //   method, a fn value or a C call is not known to be held
                //   to E1002 here: their plain arguments take the call's own
                //   position.
                let plain = if (self.declared)(e.span) {
                    Pos::Read
                } else if d
                    .callee()
                    .and_then(MemberExpr::cast)
                    .is_some_and(|m| self.keeps_as_is(m))
                {
                    // A pool's `init` keeps the value it is handed as it is,
                    // on every compiling machine: a binding handed to it
                    // must be the loop's own.
                    Pos::Escape
                } else {
                    pos
                };
                // A builtin mutator's `mut` receiver whose answer goes
                // nowhere, with no lent or taken argument, is written and
                // kept by nothing: the runtime keeps no part of its
                // receiver, the answer is read here, and no other operand is
                // lent to receive one. A declared `mut self` method is not
                // among them: it may move its receiver into module state and
                // store a fresh value back.
                let plain_args = d
                    .args()
                    .into_iter()
                    .flat_map(|l| l.args())
                    .all(|a| a.mode().is_none());
                if let Some(c) = d.callee() {
                    match c.kind {
                        SyntaxKind::MemberExpr => {
                            let m = MemberExpr::cast(c);
                            if let Some(b) = m.and_then(|m| m.base()) {
                                if pos == Pos::Read
                                    && plain_args
                                    && self.lends_derived(b)
                                    && m.is_some_and(|m| self.builtin_mutator(m, b))
                                {
                                    self.wrote = true;
                                    if let Some(inner) = ParenExpr::cast(b).and_then(|x| x.expr()) {
                                        self.expr(inner, Pos::Read);
                                    }
                                } else {
                                    self.expr(b, plain);
                                }
                            }
                        }
                        _ => self.expr(c, Pos::Read),
                    }
                }
                // wolf-lang#1177: a `push` stores a fresh operand as it is
                // ([`fresh_operand`]), so the parts of a literal it is handed
                // go where the list goes: they escape. Any method named
                // `push` counts, a builtin or not; for a declared one the
                // only cost is a copy.
                let pushes = d
                    .callee()
                    .and_then(MemberExpr::cast)
                    .and_then(|m| m.member())
                    .is_some_and(|t| text(self.src, t.span) == "push");
                for a in d.args().into_iter().flat_map(|l| l.args()) {
                    let kept = pushes
                        && a.value()
                            .is_some_and(|v| fresh_operand(v, self.declared, self.expr_ty));
                    self.arg(a, if kept { Pos::Escape } else { plain });
                }
            }
            SyntaxKind::BracketApply => {
                let d = BracketApply::cast(e).expect("kind");
                if let Some(c) = d.callee() {
                    self.expr(c, pos);
                }
                for a in d.args().into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = a.value() {
                        self.expr(v, Pos::Read);
                    }
                }
            }
            SyntaxKind::MemberExpr => {
                if let Some(b) = MemberExpr::cast(e).and_then(|m| m.base()) {
                    self.expr(b, pos);
                }
            }
            // A literal holds its parts, and they go where it goes. Where
            // the literal escapes (assigned, returned, stored by index as a
            // temporary, which an index store keeps as it is, wolf-lang#438)
            // its parts escape with it. Where it is read, they are read: a
            // `let` makes the bound name derived (its own uses answer for
            // it) and a function's argument is a `read` one. A `push` keeps
            // a literal as it is (wolf-lang#1177): the call arm walks one in
            // the escaping position.
            SyntaxKind::StructLit => {
                let d = StructLit::cast(e).expect("kind");
                for f in d.fields() {
                    match f.value() {
                        Some(v) => self.expr(v, pos),
                        None => {
                            // `S { row }`: the shorthand moves the name.
                            if pos == Pos::Escape
                                && f.name()
                                    .is_some_and(|t| self.names.contains(&text(self.src, t.span)))
                            {
                                self.escaped = true;
                            }
                        }
                    }
                }
            }
            SyntaxKind::TupleExpr | SyntaxKind::ListLit => {
                for c in e.nodes() {
                    self.expr(c, pos);
                }
            }
            SyntaxKind::Block => self.block(e, pos),
            // A region, `in` or `scope` block is a block. Its region
            // changes where the block's own allocations go, never the
            // walked element, which lives outside it; a value it hands out
            // leaves by the block's own position, and what its body does
            // with the binding is what it would do outside. A closure or
            // `spawn` inside still copies, as anywhere. `unsafe` is not
            // among them: a raw write through a pointer to the binding has
            // no place this walk can root, so it stays a construct the walk
            // does not know.
            SyntaxKind::RegionBlock => {
                let d = wolf_ast::RegionBlock::cast(e).expect("kind");
                if let Some(st) = d.strategy() {
                    self.expr(st, Pos::Read);
                }
                if let Some(v) = d.cap().and_then(|c| c.value()) {
                    self.expr(v, Pos::Read);
                }
                if let Some(b) = d.body() {
                    self.block(b.syntax(), pos);
                }
            }
            SyntaxKind::InBlock => {
                let d = wolf_ast::InBlock::cast(e).expect("kind");
                if let Some(r) = d.region() {
                    self.expr(r, Pos::Read);
                }
                if let Some(b) = d.body() {
                    self.block(b.syntax(), pos);
                }
            }
            SyntaxKind::ScopeExpr => {
                if let Some(b) = wolf_ast::ScopeExpr::cast(e).and_then(|d| d.body()) {
                    self.block(b.syntax(), pos);
                }
            }
            SyntaxKind::IfExpr | SyntaxKind::WhileExpr | SyntaxKind::LoopExpr => {
                let cond = match e.kind {
                    SyntaxKind::IfExpr => wolf_ast::IfExpr::cast(e).and_then(|d| d.condition()),
                    SyntaxKind::WhileExpr => {
                        wolf_ast::WhileExpr::cast(e).and_then(|d| d.condition())
                    }
                    _ => None,
                }
                .map(|c| c.span);
                let p = if e.kind == SyntaxKind::IfExpr {
                    pos
                } else {
                    Pos::Read
                };
                for c in e.nodes() {
                    if Some(c.span) == cond {
                        self.expr(c, Pos::Read);
                    } else {
                        self.expr(c, p);
                    }
                }
            }
            SyntaxKind::MatchExpr => {
                let d = MatchExpr::cast(e).expect("kind");
                if let Some(s) = d.scrutinee() {
                    self.expr(s, Pos::Read);
                }
                for arm in d.arms() {
                    if let Some(g) = arm.guard() {
                        self.expr(g, Pos::Read);
                    }
                    if let Some(b) = arm.body() {
                        self.expr(b, pos);
                    }
                }
            }
            SyntaxKind::MatchGuard => {
                for c in e.nodes() {
                    self.expr(c, Pos::Read);
                }
            }
            SyntaxKind::ForExpr => {
                let d = ForExpr::cast(e).expect("kind");
                if let Some(i) = d.iterable() {
                    self.expr(i, Pos::Read);
                }
                if let Some(b) = d.body() {
                    self.block(b.syntax(), Pos::Read);
                }
            }
            SyntaxKind::ReturnExpr => {
                if let Some(v) = ReturnExpr::cast(e).and_then(|d| d.value()) {
                    self.expr(v, Pos::Escape);
                }
            }
            SyntaxKind::BreakExpr => {
                if let Some(v) = BreakExpr::cast(e).and_then(|d| d.value()) {
                    self.expr(v, Pos::Escape);
                }
            }
            SyntaxKind::ElseExpr => {
                let d = ElseExpr::cast(e).expect("kind");
                if let Some(s) = d.scrutinized() {
                    self.expr(s, pos);
                }
                if let Some(f) = d.fallback() {
                    self.expr(f, pos);
                }
            }
            // Closures, `spawn`, regions, scopes, `when`, `select`, raw
            // borrows, inline C and assembly, a nested item, an error node:
            // anything else that names a derived value copies.
            _ => {
                if self.spells(e) {
                    self.escaped = true;
                }
            }
        }
    }

    /// Is `m` (on the lent receiver `b`) a builtin mutator: `push`, `pop`,
    /// `clear`, `reserve` or `reset` on a `List`, `clear` or `remove` on a
    /// `Map`? The checker resolves these names on such a receiver to the
    /// builtin before any home-module or declared method
    /// (`[type.method.resolve]`), so the receiver's recorded type decides it;
    /// an unrecorded type answers no.
    fn builtin_mutator(&self, m: MemberExpr<'_>, b: &GreenNode) -> bool {
        let Some(name) = m.member().map(|t| text(self.src, t.span)) else {
            return false;
        };
        let Some(inner) = ParenExpr::cast(b).and_then(|x| x.expr()) else {
            return false;
        };
        match (self.expr_ty)(unparen(inner).span) {
            Some(TyKind::List(_)) => {
                matches!(
                    name.as_str(),
                    "push" | "pop" | "clear" | "reserve" | "reset"
                )
            }
            Some(TyKind::Map(..)) => matches!(name.as_str(), "clear" | "remove"),
            _ => false,
        }
    }

    /// Is `m` a builtin that keeps a plain argument without copying it in:
    /// `init` on a `Pool` receiver? A `push` copies its element in, and a
    /// channel's `send` takes only payloads that cannot share mutable data
    /// (E1102). An unrecorded receiver type answers no; the call then keeps
    /// the rule of every other builtin.
    fn keeps_as_is(&self, m: MemberExpr<'_>) -> bool {
        let Some(name) = m.member().map(|t| text(self.src, t.span)) else {
            return false;
        };
        let Some(b) = m.base() else {
            return false;
        };
        let recv = match ParenExpr::cast(b).and_then(|x| x.expr()) {
            Some(inner) => unparen(inner),
            None => b,
        };
        name == "init" && matches!((self.expr_ty)(recv.span), Some(TyKind::Pool(_)))
    }

    /// `(mut p)` where `p` is a place rooted at a derived name.
    fn lends_derived(&self, b: &GreenNode) -> bool {
        b.kind == SyntaxKind::ParenExpr
            && b.tokens().any(|t| t.kind == SyntaxKind::MutKw)
            && !b.tokens().any(|t| t.kind == SyntaxKind::TakeKw)
            && ParenExpr::cast(b)
                .and_then(|x| x.expr())
                .is_some_and(|inner| {
                    is_place(unparen(inner))
                        && root(inner).is_some_and(|r| self.names.contains(&text(self.src, r.span)))
                })
    }

    fn arg(&mut self, a: Arg<'_>, plain: Pos) {
        let p = if a.mode().is_some() {
            Pos::Escape
        } else {
            plain
        };
        if let Some(v) = a.value() {
            self.expr(v, p);
        }
    }
}

/// Grow `names` by every name the body binds from an expression that spells
/// one of them: a `let`/`var` initializer, a `match` scrutinee (each arm's
/// pattern), a nested `for`'s iterable (its pattern), an `else` scrutinee
/// (its handler's pattern).
fn derive(n: &GreenNode, src: &[u8], names: &mut HashSet<String>) {
    match n.kind {
        SyntaxKind::LetDecl | SyntaxKind::VarDecl => {
            for b in wolf_ast::binding_binders(n) {
                if let (Some(p), Some(init)) = (b.pattern, b.init)
                    && spells(init, src, names)
                {
                    idents(p, src, names);
                }
            }
        }
        SyntaxKind::MatchExpr => {
            let d = MatchExpr::cast(n).expect("kind");
            if d.scrutinee().is_some_and(|s| spells(s, src, names)) {
                for arm in d.arms() {
                    if let Some(p) = arm.pattern() {
                        idents(p, src, names);
                    }
                }
            }
        }
        SyntaxKind::ForExpr => {
            let d = ForExpr::cast(n).expect("kind");
            if d.iterable().is_some_and(|i| spells(i, src, names))
                && let Some(p) = d.pattern()
            {
                idents(p, src, names);
            }
        }
        SyntaxKind::ElseExpr => {
            let d = ElseExpr::cast(n).expect("kind");
            if d.scrutinized().is_some_and(|s| spells(s, src, names))
                && let Some(p) = d.handler_pattern()
            {
                idents(p, src, names);
            }
        }
        _ => {}
    }
    for c in n.nodes() {
        derive(c, src, names);
    }
}

/// Every identifier a pattern spells: the names it binds, and harmlessly
/// the variant and field names beside them (more names only copy more).
fn idents(p: &GreenNode, src: &[u8], out: &mut HashSet<String>) {
    for t in p.tokens() {
        if t.kind == SyntaxKind::Ident {
            out.insert(text(src, t.span));
        }
    }
    for c in p.nodes() {
        idents(c, src, out);
    }
}

/// Does any identifier token in this subtree spell one of `names`?
fn spells(n: &GreenNode, src: &[u8], names: &HashSet<String>) -> bool {
    n.tokens()
        .any(|t| t.kind == SyntaxKind::Ident && names.contains(&text(src, t.span)))
        || n.nodes().any(|c| spells(c, src, names))
}

/// The name a place is rooted at: `a` of `a.f[i].g`.
fn root(place: &GreenNode) -> Option<&GreenNode> {
    let place = unparen(place);
    match place.kind {
        SyntaxKind::PathExpr => Some(place),
        SyntaxKind::BracketApply => BracketApply::cast(place)?.callee().and_then(root),
        SyntaxKind::MemberExpr => MemberExpr::cast(place)?.base().and_then(root),
        _ => None,
    }
}

/// A name, a field of a place, or an element of one.
fn is_place(e: &GreenNode) -> bool {
    match e.kind {
        SyntaxKind::PathExpr => true,
        SyntaxKind::BracketApply => BracketApply::cast(e)
            .and_then(|b| b.callee())
            .is_some_and(|c| is_place(unparen(c))),
        SyntaxKind::MemberExpr => MemberExpr::cast(e)
            .and_then(|m| m.base())
            .is_some_and(|b| is_place(unparen(b))),
        _ => false,
    }
}

fn text(src: &[u8], span: Span) -> String {
    String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize])
        .trim()
        .to_string()
}

fn unparen(e: &GreenNode) -> &GreenNode {
    if e.kind == SyntaxKind::ParenExpr
        && !e
            .tokens()
            .any(|t| matches!(t.kind, SyntaxKind::MutKw | SyntaxKind::TakeKw))
    {
        ParenExpr::cast(e).and_then(|p| p.expr()).unwrap_or(e)
    } else {
        e
    }
}
