//! s216 (wolf-lang#618): which declared fns hand back only STATIC `str`
//! bytes — the precision half of the call-result rule.
//!
//! `[mem.region.escape]` makes a call's `str` result an allocation in
//! the caller's ambient region (the callee builds where its caller
//! decides, D12), so a call inside a `region` block mints a site there
//! and the result cannot leave the block. That is the sound default,
//! and it refuses a helper that only ever answers a literal —
//! boreutils' `bore.io_error() -> str { "Input/output error" }`, held
//! past its `region pass` — whose bytes are static and outlive every
//! region. This pass names those helpers so the call mints nothing.
//!
//! A fn is static when every expression that can become its result —
//! the body's tail and every `return`'s operand — is static: a string
//! literal with no interpolation hole; a parenthesized, block-tail,
//! `if`/`else` or `match` form whose every branch is static; a call to
//! a fn already proven static; or a diverging `return` (its operand is
//! judged on its own). Anything else — an interpolation, `+`, a
//! parameter, a field, an unknown call — may name region bytes, and the
//! fn is not static. The set is a least fixpoint over the package's
//! checked bodies, so mutual recursion among literal-only helpers
//! proves nothing (sound: a fn left out only stays conservative).

use std::collections::{HashMap, HashSet};

use wolf_ast::{Block, FnDecl, GreenNode, IfExpr, MatchExpr, ReturnExpr, StringExpr, SyntaxKind};
use wolf_sema::check::CallSig;
use wolf_sema::{BodyResult, Package, Typecheck};
use wolf_span::Span;

/// One candidate: a checked fn body with a name.
struct Cand<'a> {
    name: Span,
    body: Block<'a>,
    calls: HashMap<Span, &'a CallSig>,
}

/// The NAME spans (a `CallSig::decl_span`) of every declared fn whose
/// result is only ever static `str` bytes.
pub(crate) fn static_str_fns(pkg: &Package, tc: &Typecheck) -> HashSet<Span> {
    let mut cands: Vec<Cand<'_>> = Vec::new();
    for outcome in &tc.bodies {
        let BodyResult::Checked(tb) = &outcome.result else {
            continue;
        };
        let b = &outcome.body;
        let root = &pkg.files[b.file].parse.root;
        let Some(node) = root.nodes().filter(|n| n.kind.is_item()).nth(b.decl) else {
            continue;
        };
        let node = match b.member {
            None => node,
            Some(mi) => match node.nodes().filter(|n| n.kind.is_item()).nth(mi) {
                Some(inner) => inner,
                None => continue,
            },
        };
        if node.kind != SyntaxKind::FnDecl {
            continue;
        }
        let Some(d) = FnDecl::cast(node) else {
            continue;
        };
        let (Some(name), Some(body)) = (d.name(), d.body()) else {
            continue;
        };
        cands.push(Cand {
            name: name.span,
            body,
            calls: tb.calls.iter().map(|(s, c)| (*s, c)).collect(),
        });
    }
    let mut set: HashSet<Span> = HashSet::new();
    loop {
        let mut grew = false;
        for c in &cands {
            if set.contains(&c.name) {
                continue;
            }
            if fn_is_static(c, &set) {
                set.insert(c.name);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    set
}

fn fn_is_static(c: &Cand<'_>, set: &HashSet<Span>) -> bool {
    let mut results: Vec<&GreenNode> = Vec::new();
    // No tail: the result comes from `return`s alone (or the fn is unit,
    // and no `str` call reaches it).
    if let Some(t) = c.body.trailing_expr() {
        results.push(t);
    }
    collect_returns(c.body.syntax(), &mut results);
    if results.is_empty() {
        return false;
    }
    results.into_iter().all(|e| is_static(e, c, set, 0))
}

/// Every `return` operand under `n`, not entering a closure or a
/// nested fn (their returns are their own).
fn collect_returns<'a>(n: &'a GreenNode, out: &mut Vec<&'a GreenNode>) {
    for k in n.nodes() {
        match k.kind {
            SyntaxKind::ClosureExpr | SyntaxKind::FnDecl => continue,
            SyntaxKind::ReturnExpr => {
                if let Some(v) = ReturnExpr::cast(k).and_then(|r| r.value()) {
                    out.push(v);
                    collect_returns(v, out);
                }
            }
            _ => collect_returns(k, out),
        }
    }
}

fn block_tail_static(b: Block<'_>, c: &Cand<'_>, set: &HashSet<Span>, depth: u32) -> bool {
    b.trailing_expr()
        .is_some_and(|t| is_static(t, c, set, depth + 1))
}

fn is_static(e: &GreenNode, c: &Cand<'_>, set: &HashSet<Span>, depth: u32) -> bool {
    if depth > 64 {
        return false;
    }
    match e.kind {
        SyntaxKind::StringExpr => {
            StringExpr::cast(e).is_some_and(|s| s.interps().all(|i| i.expr().is_none()))
        }
        SyntaxKind::ParenExpr => e
            .nodes()
            .next()
            .is_some_and(|inner| is_static(inner, c, set, depth + 1)),
        SyntaxKind::Block => Block::cast(e).is_some_and(|b| block_tail_static(b, c, set, depth)),
        SyntaxKind::IfExpr => {
            let Some(d) = IfExpr::cast(e) else {
                return false;
            };
            let then_ok = d
                .then_block()
                .is_some_and(|b| block_tail_static(b, c, set, depth));
            let else_ok = d
                .else_branch()
                .is_some_and(|el| is_static(el, c, set, depth + 1));
            then_ok && else_ok
        }
        SyntaxKind::MatchExpr => {
            let Some(d) = MatchExpr::cast(e) else {
                return false;
            };
            let mut any = false;
            for arm in d.arms() {
                any = true;
                if !arm.body().is_some_and(|b| is_static(b, c, set, depth + 1)) {
                    return false;
                }
            }
            any
        }
        SyntaxKind::CallExpr => c
            .calls
            .get(&e.span)
            .and_then(|cs| cs.decl_span)
            .is_some_and(|ds| set.contains(&ds)),
        SyntaxKind::ReturnExpr => true,
        _ => false,
    }
}
