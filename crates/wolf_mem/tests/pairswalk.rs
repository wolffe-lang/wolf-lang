//! s273 (wolf-lang#972) — the `pairs()` walk predicate, asserted
//! directly rather than through its two consumers (the checked machine's
//! `eval_for`, `wolf_wir`'s `lower_for`).
//!
//! Soundness rests on one answer: `Some` is what lets a machine walk the
//! map's own entries instead of a fresh list of them, so the shapes that
//! must keep materializing are pinned beside the ones that may not. Every
//! input type-checks clean and the receiver's type is the checker's own.

use wolf_ast::{ForExpr, GreenNode, SyntaxKind};
use wolf_mem::pairswalk::walk_recv;
use wolf_sema::check::BodyResult;
use wolf_sema::types::TyKind;
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};
use wolf_span::Span;

/// The predicate's answer for the first `for` in `src`'s function `f`.
fn walk(src: &str) -> Option<String> {
    let mut ml = MemoryLoader::new("walk");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        !res.diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error),
        "input resolves clean: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    assert!(!tc.has_errors(), "input types clean: {:?}", tc.diagnostics);
    let file = &res.package.files[0];
    let src = &file.raw.src;
    fn find(n: &GreenNode) -> Option<&GreenNode> {
        if n.kind == SyntaxKind::ForExpr {
            return Some(n);
        }
        n.nodes().find_map(find)
    }
    let f = find(&file.parse.root).expect("a for loop");
    let d = ForExpr::cast(f).expect("kind");
    let ty = |sp: Span| -> Option<TyKind> {
        tc.bodies.iter().find_map(|b| match &b.result {
            BodyResult::Checked(tb) => tb
                .exprs
                .iter()
                .find(|(s, _)| *s == sp)
                .map(|(_, id)| tb.table.kind(*id).clone()),
            _ => None,
        })
    };
    walk_recv(d, src, &ty).map(|(_, name)| name)
}

#[test]
fn a_body_that_never_names_the_map_walks_its_entries() {
    let sum = "fn f(m: Map[str, int]) -> int {\n    var t = 0\n    for p in m.pairs() {\n        t += p.1\n    }\n    t\n}\n\nfn main() {}\n";
    assert_eq!(walk(sum).as_deref(), Some("m"));
    // std.map.get's body, generic, over a local Eq.
    let get = "trait Eq {\n    fn eq(self, other: Self) -> bool\n}\n\nfn f[K: Eq, V](m: Map[K, V], k: K) -> V ! {none} {\n    for pair in m.pairs() {\n        if Eq.eq(pair.0, k) {\n            return pair.1\n        }\n    }\n    none\n}\n\nfn main() {}\n";
    assert_eq!(walk(get).as_deref(), Some("m"));
    // A `mut` parameter, parenthesized.
    let paren = "fn f(mut m: Map[str, int]) -> int {\n    var t = 0\n    for p in (m).pairs() {\n        t += p.1\n    }\n    t\n}\n\nfn main() {}\n";
    assert_eq!(walk(paren).as_deref(), Some("m"));
}

#[test]
fn a_body_that_names_the_map_materializes() {
    for stmt in [
        "m[\"x\"] = p.1",
        "t += m.len",
        "print(\"{m.len}\")",
        "let m = 3\n        t += m",
        "g(mut m)",
    ] {
        let src = format!(
            "fn g(mut m: Map[str, int]) {{\n    m[\"y\"] = 0\n}}\n\nfn f(mut m: Map[str, int]) -> int {{\n    var t = 0\n    for p in m.pairs() {{\n        t += p.1\n        {stmt}\n    }}\n    t\n}}\n\nfn main() {{}}\n"
        );
        assert_eq!(walk(&src), None, "a body spelling `m`: {stmt}");
    }
}

#[test]
fn every_other_iterable_materializes() {
    // A field path: another name could reach the struct.
    let field = "struct T {\n    m: Map[str, int],\n}\n\nfn f(t: T) -> int {\n    var n = 0\n    for p in t.m.pairs() {\n        n += p.1\n    }\n    n\n}\n\nfn main() {}\n";
    assert_eq!(walk(field), None);
    // A call's map: no name at all.
    let call = "fn mk() -> Map[str, int] {\n    Map[str, int]()\n}\n\nfn f() -> int {\n    var n = 0\n    for p in mk().pairs() {\n        n += p.1\n    }\n    n\n}\n\nfn main() {}\n";
    assert_eq!(walk(call), None);
    // A list bound first is a value, not a walk.
    let bound = "fn f(m: Map[str, int]) -> int {\n    var n = 0\n    let ps = m.pairs()\n    for p in ps {\n        n += p.1\n    }\n    n\n}\n\nfn main() {}\n";
    assert_eq!(walk(bound), None);
    // A List is not a Map.
    let list = "fn f(xs: List[int]) -> int {\n    var n = 0\n    for x in xs {\n        n += x\n    }\n    n\n}\n\nfn main() {}\n";
    assert_eq!(walk(list), None);
}
