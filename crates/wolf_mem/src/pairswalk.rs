//! The `pairs()` walk as a view (s273, wolf-lang#972) — when a `for`
//! over `m.pairs()` may read the map's own entries instead of a fresh
//! list of them.
//!
//! # Why
//!
//! `m.pairs()` answers a fresh `List[(K, V)]` of every entry
//! (`[type.map]`), and a fresh list is an allocation in the ambient
//! region (`[mem.region.create.3]`) that nothing frees before the region
//! ends. wolf-std's `map.get` is a walk over it —
//! `for pair in m.pairs() { if Eq.eq(pair.0, k) { return pair.1 } }` —
//! so every lookup copied the whole map into its caller's region: N gets
//! on an N-entry map kept N² × 24 bytes natively, 2.4 GB at 10,000 keys,
//! where the same program in C peaks at 2.2 MB (#972, measured in
//! `notes/s273-evidence/`). The list is never named: it exists for the
//! loop header and dies with the loop.
//!
//! # The rule, and why it is sound
//!
//! The walk is the map's own entries, in order, exactly when nothing the
//! loop does can change them — then the snapshot `pairs()` promises and
//! the live entries are the same bytes, and the copy is unobservable but
//! for the ledger (`[mem.region.promote.1]`; `[mem.region.account.1]`:
//! "an allocation the implementation promotes or elides may charge
//! nothing"). This module proves the narrow case, syntactically, once for
//! both consumers (`ubcheck`'s `eval_for` and `wolf_wir::lower`'s
//! `lower_for`):
//!
//! - the iterable is `recv.pairs()` with no arguments, its receiver a
//!   bare name (no path, no call) whose type is `Map[K, V]`;
//! - the loop body never spells that name — no read, no write, no lend,
//!   no move, no shadowing `let`.
//!
//! Each consumer then asks two things only it can answer: that the name
//! is a LOCAL binding or parameter of the running function (a
//! module-level `var` could be written by any callee without the body
//! naming it), and that the call is the builtin `pairs` (not a home
//! module's function). With all four, no write can reach the map during
//! the loop: tier-0 exclusivity gives a local `Map` one name, a closure
//! cannot write its captures, and a callee reaches the map only through
//! an argument the body would have to spell. Anything else — a `let`
//! binding the list, an argument, a body that names the map, a field
//! path — materializes exactly as before, which is always correct.

use wolf_ast::{CallExpr, ForExpr, GreenNode, MemberExpr, ParenExpr, SyntaxKind};
use wolf_sema::types::TyKind;
use wolf_span::Span;

/// The receiver of a `for pat in recv.pairs() { body }` that may walk
/// the map's own entries, and the name it is spelled by; `None` for
/// every other loop (the caller materializes).
pub fn walk_recv<'t>(
    d: ForExpr<'t>,
    src: &[u8],
    expr_ty: &dyn Fn(Span) -> Option<TyKind>,
) -> Option<(&'t GreenNode, String)> {
    let iter = d.iterable()?;
    if iter.kind != SyntaxKind::CallExpr {
        return None;
    }
    let call = CallExpr::cast(iter)?;
    if call
        .args()
        .into_iter()
        .flat_map(|l| l.args())
        .next()
        .is_some()
    {
        return None;
    }
    let m = MemberExpr::cast(call.callee()?)?;
    if m.member().map(|t| text(src, t.span)).as_deref() != Some("pairs") {
        return None;
    }
    let recv = unparen(m.base()?);
    if recv.kind != SyntaxKind::PathExpr {
        return None;
    }
    let name = text(src, recv.span);
    if !is_ident(&name) || name == "self" {
        return None;
    }
    if !matches!(expr_ty(recv.span), Some(TyKind::Map(..))) {
        return None;
    }
    let body = d.body()?;
    if spells(body.syntax(), src, &name) {
        return None;
    }
    Some((recv, name))
}

/// Does any identifier token in this subtree spell `name`? A field or
/// method of that spelling counts too: the answer only ever makes the
/// caller materialize, which is never wrong.
fn spells(n: &GreenNode, src: &[u8], name: &str) -> bool {
    n.tokens()
        .any(|t| t.kind == SyntaxKind::Ident && text(src, t.span) == name)
        || n.nodes().any(|c| spells(c, src, name))
}

fn is_ident(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next().is_some_and(|c| c == '_' || c.is_alphabetic())
        && cs.all(|c| c == '_' || c.is_alphanumeric())
}

fn text(src: &[u8], span: Span) -> String {
    String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize])
        .trim()
        .to_string()
}

fn unparen(e: &GreenNode) -> &GreenNode {
    if e.kind == SyntaxKind::ParenExpr {
        ParenExpr::cast(e).and_then(|p| p.expr()).unwrap_or(e)
    } else {
        e
    }
}
