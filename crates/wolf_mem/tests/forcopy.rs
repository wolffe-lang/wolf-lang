//! wolf-lang#1169 — the `for` binding's copy predicate
//! ([`wolf_mem::forcopy::needs_own_copy`]), asserted directly rather than
//! through its two consumers (the checked machine's `eval_for`, `wolf_wir`'s
//! `lower_for_list`).
//!
//! Soundness rests on one answer: `false` is what lets a machine bind the
//! walked element itself, so every shape that must copy is pinned by name
//! beside the read-only shapes that must not pay for one. Each input
//! resolves clean, and the types the predicate reads are the checker's own.

use wolf_ast::{ForExpr, GreenNode, SyntaxKind};
use wolf_mem::forcopy::{declared_call, needs_own_copy};
use wolf_sema::check::BodyResult;
use wolf_sema::types::TyKind;
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};
use wolf_span::Span;

/// The predicate's answer for the first `for` in `src`. `typed` asks that
/// the input type-check clean too; a shape the checker refuses (a value
/// carried by `break` out of a `for`) is still asked, over the types it did
/// record. `known` false answers no type at any span.
fn ask(src: &str, typed: bool, known: bool) -> bool {
    let mut ml = MemoryLoader::new("forcopy");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        !res.diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error),
        "input resolves clean: {:?}\n{src}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    if typed {
        assert!(
            !tc.has_errors(),
            "input types clean: {:?}\n{src}",
            tc.diagnostics
        );
    }
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
        if !known {
            return None;
        }
        tc.bodies.iter().find_map(|b| match &b.result {
            BodyResult::Checked(tb) => tb
                .exprs
                .iter()
                .find(|(s, _)| *s == sp)
                .map(|(_, id)| tb.table.kind(*id).clone()),
            _ => None,
        })
    };
    let declared = |sp: Span| -> bool {
        tc.bodies.iter().any(|b| match &b.result {
            BodyResult::Checked(tb) => tb.calls.iter().any(|(s, c)| *s == sp && declared_call(c)),
            _ => false,
        })
    };
    needs_own_copy(d, src, &ty, &declared)
}

/// A loop over `rows: List[List[int]]` in `f`, with `body` inside it.
fn rows(body: &str) -> String {
    format!(
        "fn f() -> int {{\n    let rows = [[1, 2], [3, 4]]\n    var out = [[0]]\n    var last = [0]\n    var total = 0\n    var seen = \"\"\n    for row in rows {{\n        {body}\n    }}\n    print(\"{{out.len}} {{last.len}} {{seen}}\")\n    total\n}}\n\nstruct W {{ items: List[int] }}\n\nfn zap(take xs: List[int]) -> int {{\n    xs[0] = 0\n    0\n}}\n\nfn sum(xs: List[int]) -> int {{\n    xs.len\n}}\n\nfn fresh(xs: List[int]) -> List[int] {{\n    copy xs\n}}\n\nenum Bag {{ Full(List[int]), Empty }}\n\nfn main() {{}}\n"
    )
}

/// A loop over `cube: List[List[List[int]]]`.
fn cube(body: &str) -> String {
    format!(
        "fn f() -> int {{\n    let cube = [[[1, 2], [3, 4]], [[5, 6], [7, 8]]]\n    for plane in cube {{\n        {body}\n    }}\n    cube.len\n}}\n\nfn main() {{}}\n"
    )
}

/// A loop over `boxes: List[Box]`.
fn boxes(body: &str) -> String {
    format!(
        "enum Box {{ Full(List[int]), Empty }}\n\nfn f() -> int {{\n    let boxes = [Box.Full([1, 2]), Box.Empty]\n    for b in boxes {{\n        {body}\n    }}\n    boxes.len\n}}\n\nfn main() {{}}\n"
    )
}

#[test]
fn every_store_through_the_binding_copies() {
    let maps = "fn f() -> int {\n    var a = Map[str, int]()\n    a[\"k\"] = 1\n    let maps = [a]\n    for m in maps {\n        m[\"k\"] = 9\n    }\n    maps.len\n}\n\nfn main() {}\n";
    let field = "struct Row { items: List[int] }\n\nfn f() -> int {\n    let rows = [Row { items: [1, 2] }]\n    for r in rows {\n        r.items[0] = 0\n    }\n    rows.len\n}\n\nfn main() {}\n";
    let tuple = "fn f() -> int {\n    let pairs = [([1, 2], 7)]\n    for (xs, n) in pairs {\n        xs[0] = n\n    }\n    pairs.len\n}\n\nfn main() {}\n";
    let pairs = "fn f(m: Map[str, List[int]]) -> int {\n    for p in m.pairs() {\n        p.1[0] = 0\n    }\n    m.len\n}\n\nfn main() {}\n";
    for (name, src) in [
        ("store", rows("row[0] = 0")),
        ("compound store", rows("row[0] += 1")),
        (
            "read after a store",
            rows("row[0] = row[0] + 10\n        seen = \"{seen} {row[0]}\""),
        ),
        ("store two deep", cube("plane[0][0] = 0")),
        ("map key store", maps.to_string()),
        ("element store under a field", field.to_string()),
        ("store through a destructured name", tuple.to_string()),
        (
            "store through a match arm",
            boxes(
                "match b {\n            Full(xs) => { xs[0] = 0 },\n            Empty => {},\n        }",
            ),
        ),
        ("store through a pairs() element", pairs.to_string()),
    ] {
        assert!(ask(&src, true, true), "must copy: {name}\n{src}");
    }
}

#[test]
fn every_escape_of_the_binding_copies() {
    let ret = "struct Grid { rows: List[List[int]] }\n\nfn first(mut g: Grid) -> List[int] {\n    for r in g.rows {\n        return r\n    }\n    [0]\n}\n\nfn main() {}\n";
    for (name, src, typed) in [
        ("let move", rows("let r = row\n        r[0] = 0"), true),
        (
            "element moved out",
            cube("let line = plane[0]\n        line[0] = 0"),
            true,
        ),
        (
            "struct literal",
            rows("let w = W { items: row }\n        w.items[0] = 0"),
            true,
        ),
        ("mut lend of the binding", rows("(mut row).push(9)"), true),
        (
            "mut lend of a var moved from it",
            rows("var r = row\n        (mut r).push(9)"),
            true,
        ),
        ("take argument", rows("let _ = zap(take row)"), true),
        ("assigned out of the loop", rows("last = row"), true),
        (
            "an enum constructor holding it, assigned out",
            rows(
                "let b = Bag.Full(row)\n        match b {\n            Full(xs) => { last = xs },\n            Empty => {},\n        }",
            ),
            true,
        ),
        ("returned", ret.to_string(), true),
        (
            "tuple literal",
            rows("let t = (row, 1)\n        t.0[0] = 0"),
            true,
        ),
        (
            "an index store of a temporary built from it",
            rows("out[0] = W { items: row }.items"),
            true,
        ),
        (
            "closure naming it",
            rows("let g = fn(i: int) row[i]\n        total += g(0)"),
            false,
        ),
        (
            "spawn naming it",
            rows("scope s {\n            s.spawn(fn() { print(\"{row[0]}\") })\n        }"),
            false,
        ),
        (
            "break carrying it",
            rows("let x = loop { break row }"),
            false,
        ),
        (
            "a derived name through a match arm",
            boxes(
                "match b {\n            Full(xs) => { (mut xs).push(1) },\n            Empty => {},\n        }",
            ),
            true,
        ),
        (
            "a derived name through a nested for",
            cube("for line in plane {\n            (mut line).push(1)\n        }"),
            true,
        ),
        (
            "a shape the walk does not know (a frozen region naming it)",
            rows("let fr = freeze region { W { items: row } }"),
            false,
        ),
        (
            "a store through it inside a region block",
            rows("region r {\n            row[0] = 0\n        }"),
            true,
        ),
        // `unsafe` stays a construct the walk does not know: a raw write
        // through a pointer to the binding has no place the walk can root.
        (
            "an unsafe block naming it",
            rows("unsafe {\n            total += row[0]\n        }"),
            true,
        ),
    ] {
        assert!(ask(&src, typed, true), "must copy: {name}\n{src}");
    }
}

#[test]
fn a_body_that_only_reads_binds_the_element() {
    for (name, src) in [
        (
            "element and length reads",
            rows("total += row[0] + row.len"),
        ),
        ("interpolation", rows("seen = \"{seen} {row[0]}\"")),
        ("condition", rows("if row[0] > 1 { total += 1 }")),
        (
            "a scalar moved out and assigned",
            rows("let first = row[0]\n        total = first"),
        ),
        ("plain call argument", rows("total += sum(row)")),
        ("plain receiver", rows("if row.is_empty() { total += 1 }")),
        // `push` copies any plain element in (wolf-lang#385,
        // `[mem.region.edge.elem]`): `out` holds a copy, never `row`.
        ("push of the binding", rows("(mut out).push(row)")),
        // An index store copies a place in (wolf-lang#438).
        ("index store of the binding", rows("out[0] = row")),
        // A declared function's answer never holds a `read` argument: a
        // move out of one that outlives the call is E1002, and a walk
        // over one copies its binding before it leaves.
        (
            "a function's answer assigned out",
            rows("last = fresh(row)"),
        ),
        (
            "a function's answer stored by index",
            rows("out[0] = fresh(row)"),
        ),
        // A region, `in` or `scope` block is a block: what it does with the
        // binding is read where it stands, as outside it.
        (
            "a region block that reads it",
            rows("region r {\n            total += row[0]\n        }"),
        ),
        (
            "a scope block that reads it",
            rows("scope s {\n            total += row[0]\n        }"),
        ),
        ("copy of the binding", rows("last = copy row")),
        (
            "nested for that reads",
            cube("for line in plane {\n            let n = line[0]\n        }"),
        ),
    ] {
        assert!(!ask(&src, true, true), "must not copy: {name}\n{src}");
    }
    let inb = "fn f() -> int {\n    let rows = [[1, 2]]\n    let r = region()\n    var total = 0\n    for row in rows {\n        in r {\n            total += row[0]\n        }\n    }\n    total\n}\n\nfn main() {}\n";
    assert!(
        !ask(inb, true, true),
        "must not copy: an in block that reads it"
    );
    let wild = "fn f() -> int {\n    let rows = [[1, 2]]\n    var n = 0\n    for _ in rows {\n        n += 1\n    }\n    n\n}\n\nfn main() {}\n";
    assert!(
        !ask(wild, true, true),
        "must not copy: a wildcard binds nothing"
    );
}

/// The primitive exemption reads the checker's type: an `int` element
/// assigned out is its own copy; a generic one, or one whose type is not
/// known at the span, asks for the copy.
#[test]
fn only_a_known_primitive_escapes_free() {
    let int = "fn f(xs: List[int], d: int) -> int {\n    var last = d\n    for x in xs {\n        last = x\n    }\n    last\n}\n\nfn main() {}\n";
    assert!(!ask(int, true, true), "an int assigned out: no copy");
    let generic = "fn f[T](xs: List[T], d: T) -> T {\n    var last = copy d\n    for x in xs {\n        last = x\n    }\n    last\n}\n\nfn main() {}\n";
    assert!(
        ask(generic, true, true),
        "a generic element assigned out: copy"
    );
    let scalar = rows("let first = row[0]\n        total = first");
    assert!(ask(&scalar, true, false), "no type known at the span: copy");
}

/// The boundary of the call rule: only a declared function's answer is
/// known not to hold its plain argument. A builtin's answer may be an element
/// of its receiver (`get`, `first`), and a method is a different record:
/// each copies when its answer is assigned out.
#[test]
fn a_call_answer_that_may_hold_the_binding_copies() {
    let cube = |call: &str| {
        format!(
            "fn f() -> int {{\n    let cube = [[[1, 2], [3, 4]]]\n    var last = [0]\n    for plane in cube {{\n        last = {call}\n    }}\n    last.len\n}}\n\nfn main() {{}}\n"
        )
    };
    for (name, src) in [
        (
            "a builtin get's answer, assigned out",
            cube("plane.get(0) else [9]"),
        ),
        (
            "a builtin first's answer, assigned out",
            cube("plane.first() else [9]"),
        ),
        (
            "a builtin last's answer, assigned out",
            cube("plane.last() else [9]"),
        ),
    ] {
        assert!(ask(&src, true, true), "must copy: {name}\n{src}");
    }
    let method = "struct W { items: List[int] }\n\nimpl W {\n    fn pick(self) -> List[int] {\n        copy self.items\n    }\n}\n\nfn f() -> int {\n    let ws = [W { items: [1, 2] }]\n    var last = [0]\n    for w in ws {\n        last = w.pick()\n    }\n    last.len\n}\n\nfn main() {}\n";
    assert!(
        ask(method, true, true),
        "must copy: a method's answer, assigned out"
    );
}

/// The law the call rule leans on, held here rather than only cited: a
/// declared function that keeps a plain parameter past the call is refused
/// (E1002, `[mem.tier0.mode.read]`), whether it returns the parameter,
/// carries it out in a struct, or stores it into a `mut` parameter, and
/// whether it is declared in the root module or in another one, as std's
/// functions are. `wolf_mem::check_package` judges every typed body of the
/// package, std's among them. The same function with `copy` at the move is
/// not refused.
#[test]
fn a_declared_function_cannot_keep_its_plain_parameter() {
    fn codes(files: &[(&[&str], &str, &str)]) -> Vec<String> {
        let mut ml = MemoryLoader::new("law");
        for (m, n, s) in files {
            ml.add_file(m, n, s);
        }
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
        let mem = wolf_mem::check_package(&res.package, &tc);
        mem.diagnostics
            .iter()
            .map(|d| d.code.as_str().to_string())
            .collect()
    }
    for (name, src) in [
        (
            "returned",
            "fn keep(b: List[int]) -> List[int] { b }\n\nfn main() -> !int {\n    let xs = [1]\n    let ys = keep(xs)\n    ys.len + xs.len\n}\n",
        ),
        (
            "carried out in a struct",
            "struct Box { items: List[int] }\n\nfn wrap(b: List[int]) -> Box { Box { items: b } }\n\nfn main() -> !int {\n    let xs = [1]\n    let w = wrap(xs)\n    w.items.len + xs.len\n}\n",
        ),
        (
            "stored into a mut parameter",
            "fn replace(mut out: List[int], b: List[int]) { out = b }\n\nfn main() -> !int {\n    var o = [0]\n    let xs = [1]\n    replace(mut o, xs)\n    o.len + xs.len\n}\n",
        ),
    ] {
        let got = codes(&[(&[], "main.lu", src)]);
        assert!(got.iter().any(|c| c == "E1002"), "{name}: {got:?}\n{src}");
    }
    let other = codes(&[
        (
            &[],
            "main.lu",
            "use keeper.keep\n\nfn main() -> !int {\n    let xs = [1]\n    let ys = keep(xs)\n    ys.len + xs.len\n}\n",
        ),
        (
            &["keeper"],
            "keeper.lu",
            "pub fn keep(b: List[int]) -> List[int] {\n    b\n}\n",
        ),
    ]);
    assert!(
        other.iter().any(|c| c == "E1002"),
        "a function of another module: {other:?}"
    );
    let copied = codes(&[(
        &[],
        "main.lu",
        "fn keep(b: List[int]) -> List[int] { copy b }\n\nfn main() -> !int {\n    let xs = [1]\n    let ys = keep(xs)\n    ys.len + xs.len\n}\n",
    )]);
    assert!(
        !copied.iter().any(|c| c == "E1002"),
        "copy at the move is not refused: {copied:?}"
    );
}
