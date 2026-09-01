//! Positive coverage of the full declaration grammar (spec §2, §4, §5)
//! through the typed accessor layer — every construct the sprint claims
//! is exercised against clean input here.

mod util;

use wolf_parse::codes;

use wolf_ast::{
    ConstDecl, EnumDecl, FnDecl, GreenNode, ImplDecl, ImportCDecl, LetDecl, ParamMode, StructDecl,
    SyntaxKind, TraitDecl, TypeDecl, UseDecl, VarDecl,
};

/// Parse expecting zero diagnostics.
fn clean(src: &str) -> GreenNode {
    let p = util::parse(src);
    assert!(
        p.diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {:?}",
        p.diagnostics
    );
    p.root
}

fn text(src: &str, span: wolf_span::Span) -> &str {
    &src[span.lo as usize..span.hi as usize]
}

// ------------------------------------------------------------ use trees --

#[test]
fn use_plain_path() {
    let src = "use std.fs\n";
    let root = clean(src);
    let u = root.nodes().find_map(UseDecl::cast).expect("use item");
    let segs: Vec<&str> = u
        .path()
        .expect("path")
        .segments()
        .map(|t| text(src, t.span))
        .collect();
    assert_eq!(segs, ["std", "fs"]);
    assert!(u.group().is_none());
    assert!(u.alias().is_none());
}

#[test]
fn use_group_and_alias() {
    let src = "use std.{fs, net}\nuse verylongname as vln\n";
    let root = clean(src);
    let uses: Vec<_> = root.nodes().filter_map(UseDecl::cast).collect();
    assert_eq!(uses.len(), 2);
    let names: Vec<&str> = uses[0]
        .group()
        .expect("group")
        .names()
        .map(|t| text(src, t.span))
        .collect();
    assert_eq!(names, ["fs", "net"]);
    assert_eq!(text(src, uses[1].alias().expect("alias").span), "vln");
}

#[test]
fn import_c() {
    let src = "import c \"stdlib.h\"\n";
    let root = clean(src);
    let i = root.nodes().find_map(ImportCDecl::cast).expect("import");
    assert_eq!(
        text(src, i.header().expect("header").syntax().span),
        "\"stdlib.h\""
    );
}

// ----------------------------------------------------------- visibility --

#[test]
fn visibility_pub_and_pkg() {
    let src = "pub fn f() { }\npub(pkg) let x = 1\n";
    let root = clean(src);
    let f = root.nodes().find_map(FnDecl::cast).expect("fn");
    assert!(!f.visibility().expect("vis").is_pkg());
    let l = root.nodes().find_map(LetDecl::cast).expect("let");
    assert!(l.visibility().expect("vis").is_pkg());
}

// ----------------------------------------------------------- attributes --

#[test]
fn attributes_generic_syntax() {
    let src = "#[trusted, noalloc]\nfn f() { }\n#[cfg(target = \"x86_64\"), repr(c)]\nstruct S { x: int }\n";
    let root = clean(src);
    let f = root.nodes().find_map(FnDecl::cast).expect("fn");
    let attr = f.attributes().next().expect("attr");
    let names: Vec<&str> = attr
        .items()
        .map(|a| text(src, a.path().expect("path").syntax().span))
        .collect();
    assert_eq!(names, ["trusted", "noalloc"]);
    let s = root.nodes().find_map(StructDecl::cast).expect("struct");
    let attr = s.attributes().next().expect("attr");
    let items: Vec<_> = attr.items().collect();
    assert_eq!(items.len(), 2);
    assert!(items[0].input().is_some(), "cfg(...) has input");
}

#[test]
fn inner_attribute_heads_the_file() {
    // `#![index(1)]` — the file-wide form, first non-trivia construct
    // ([gram.attr.index]); the annotated items follow untouched.
    let src = "#![index(1)]\nfn f() { }\n";
    let root = clean(src);
    let inner = root
        .nodes()
        .find_map(wolf_ast::InnerAttribute::cast)
        .expect("inner attribute");
    let item = inner.items().next().expect("attr item");
    assert_eq!(text(src, item.path().expect("path").syntax().span), "index");
    assert!(item.input().is_some(), "index(1) has input");
    assert!(
        root.nodes().any(|n| FnDecl::cast(n).is_some()),
        "fn follows"
    );
    // Below a shebang is still "first" — the interpreter line is trivia.
    let src2 = "#!/usr/bin/env -S wolf run\n#![index(1)]\nfn f() { }\n";
    let root2 = clean(src2);
    assert!(
        root2
            .nodes()
            .any(|n| wolf_ast::InnerAttribute::cast(n).is_some())
    );
}

// ------------------------------------------------------------ functions --

#[test]
fn fn_qualifiers() {
    let src = "comptime fn fib(n: int) -> int { 1 }\nextern \"c\" fn wolf() -> i32 { 42 }\nexport fn e() { }\nextern \"c\" fn puts(s: str) -> int\n";
    let root = clean(src);
    let fns: Vec<_> = root.nodes().filter_map(FnDecl::cast).collect();
    assert_eq!(fns.len(), 4);
    assert!(fns[0].is_comptime());
    assert_eq!(
        text(src, fns[1].extern_abi().expect("abi").syntax().span),
        "\"c\""
    );
    assert!(fns[2].is_export());
    assert!(fns[3].body().is_none(), "bodyless extern form");
    assert!(fns[1].body().is_some());
}

#[test]
fn fn_generics_bounds_and_type_params() {
    let src = "fn f[T, U: Show + Eq, N: type](x: T) -> U { }\n";
    let root = clean(src);
    let f = root.nodes().find_map(FnDecl::cast).expect("fn");
    let params: Vec<_> = f.generics().expect("generics").params().collect();
    assert_eq!(params.len(), 3);
    assert_eq!(text(src, params[0].name().expect("name").span), "T");
    assert!(params[0].bound().is_none());
    let bounds: Vec<&str> = params[1]
        .bound()
        .expect("bound")
        .paths()
        .map(|p| text(src, p.syntax().span))
        .collect();
    assert_eq!(bounds, ["Show", "Eq"]);
    assert!(params[2].is_type_param());
}

#[test]
fn fn_params_modes_self_view_set() {
    let src = "fn norm(mut self.{x, y}, take other: Vec3, n: int) { }\n";
    let root = clean(src);
    let f = root.nodes().find_map(FnDecl::cast).expect("fn");
    let params: Vec<_> = f.params().expect("params").params().collect();
    assert_eq!(params.len(), 3);
    assert!(params[0].is_self());
    assert_eq!(params[0].mode(), Some(ParamMode::Mut));
    let fields: Vec<&str> = params[0]
        .view_set()
        .expect("view set")
        .fields()
        .map(|t| text(src, t.span))
        .collect();
    assert_eq!(fields, ["x", "y"]);
    assert_eq!(params[1].mode(), Some(ParamMode::Take));
    assert_eq!(text(src, params[1].name().expect("name").span), "other");
    assert_eq!(params[2].mode(), None, "default read mode has no keyword");
    assert!(params[2].ty().is_some());
}

#[test]
fn fn_return_types() {
    let src = "fn a() -> !int { }\nfn b() -> int ! {BadDigit(ParseError), io.Error, ..} { }\nfn c() -> (int, int) ! {TooShort} { }\n";
    let root = clean(src);
    let fns: Vec<_> = root.nodes().filter_map(FnDecl::cast).collect();
    assert_eq!(fns.len(), 3);
    let a_ret = fns[0].ret_ty().expect("ret");
    assert_eq!(a_ret.ty().expect("ty").kind, SyntaxKind::ErrorUnionType);
    assert!(a_ret.error_row().is_none());
    let b_ret = fns[1].ret_ty().expect("ret");
    assert_eq!(b_ret.ty().expect("ty").kind, SyntaxKind::PathType);
    let row = b_ret.error_row().expect("row");
    let entries: Vec<_> = row.entries().collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(
        text(src, entries[1].path().expect("path").syntax().span),
        "io.Error"
    );
    assert_eq!(entries[0].payload().count(), 1);
    assert!(row.is_open(), "`..` marks the row open");
    let c_ret = fns[2].ret_ty().expect("ret");
    assert_eq!(c_ret.ty().expect("ty").kind, SyntaxKind::TupleType);
    assert!(c_ret.error_row().is_some());
}

/// `! {row}` is first-class on every type position (#3), not only
/// returns: a parameter type and a `let` annotation carry the row as
/// an `ErrorUnionType` node wrapping the inner type + an `ErrorRow`.
#[test]
fn postfix_row_in_param_and_let_positions() {
    fn find_kind(node: &GreenNode, kind: SyntaxKind) -> Option<&GreenNode> {
        if node.kind == kind {
            return Some(node);
        }
        node.nodes().find_map(|n| find_kind(n, kind))
    }
    let src = "fn or(v: int ! {None}, d: int) -> int { d }\n\
               fn main() -> !int {\n    let v: int ! {None, Gone(int)} = None\n    0\n}\n";
    let root = clean(src);
    let fns: Vec<_> = root.nodes().filter_map(FnDecl::cast).collect();
    let params: Vec<_> = fns[0].params().expect("params").params().collect();
    let pty = params[0].ty().expect("ty");
    assert_eq!(pty.kind, SyntaxKind::ErrorUnionType);
    assert!(pty.nodes().any(|n| n.kind == SyntaxKind::PathType));
    let prow = pty
        .nodes()
        .find_map(wolf_ast::ErrorRow::cast)
        .expect("param row");
    assert_eq!(prow.entries().count(), 1);
    assert_eq!(
        params[1].ty().expect("ty").kind,
        SyntaxKind::PathType,
        "the row binds to one parameter type only"
    );
    // Return position stays `ret_type`'s: no ErrorUnionType wrapper.
    assert_eq!(
        fns[0].ret_ty().expect("ret").ty().expect("ty").kind,
        SyntaxKind::PathType
    );
    let let_decl = find_kind(fns[1].syntax(), SyntaxKind::LetDecl).expect("let");
    let ann = LetDecl::cast(let_decl)
        .expect("cast")
        .ty()
        .expect("annotation");
    assert_eq!(ann.kind, SyntaxKind::ErrorUnionType);
    let lrow = ann
        .nodes()
        .find_map(wolf_ast::ErrorRow::cast)
        .expect("let row");
    let entries: Vec<_> = lrow.entries().collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].payload().count(), 1);
}

// ----------------------------------------------------------- type items --

#[test]
fn type_items_all_defs() {
    let src = "type Meters = distinct f64\ntype Pair = (int, str)\ntype Cb = fn(int, str) -> !int\ntype S = struct { x: int }\ntype E = enum { A, B(int) }\n";
    let root = clean(src);
    let types: Vec<_> = root.nodes().filter_map(TypeDecl::cast).collect();
    assert_eq!(types.len(), 5);
    let kinds: Vec<SyntaxKind> = types.iter().map(|t| t.def().expect("def").kind).collect();
    assert_eq!(
        kinds,
        [
            SyntaxKind::PrefixType, // distinct
            SyntaxKind::TupleType,
            SyntaxKind::FnType,
            SyntaxKind::StructDef,
            SyntaxKind::EnumDef,
        ]
    );
    assert_eq!(text(src, types[0].name().expect("name").span), "Meters");
}

#[test]
fn struct_item_fields() {
    let src = "struct P { #[old] pub x: int, y: str }\n";
    let root = clean(src);
    let s = root.nodes().find_map(StructDecl::cast).expect("struct");
    let fields: Vec<_> = s.fields().collect();
    assert_eq!(fields.len(), 2);
    assert!(fields[0].visibility().is_some());
    assert_eq!(fields[0].attributes().count(), 1);
    assert_eq!(text(src, fields[0].name().expect("name").span), "x");
    assert_eq!(fields[1].ty().expect("ty").kind, SyntaxKind::PathType);
}

#[test]
fn enum_item_variants() {
    let src = "enum Shape { Circle(f64), Rect(f64, f64), Empty }\n";
    let root = clean(src);
    let e = root.nodes().find_map(EnumDecl::cast).expect("enum");
    let variants: Vec<_> = e.variants().collect();
    assert_eq!(variants.len(), 3);
    let payloads: Vec<usize> = variants.iter().map(|v| v.payload().count()).collect();
    assert_eq!(payloads, [1, 2, 0]);
}

#[test]
fn generic_struct_and_enum() {
    let src = "struct Wrap[T] { value: T }\nenum Opt[T] { Some(T), None }\n";
    let root = clean(src);
    let s = root.nodes().find_map(StructDecl::cast).expect("struct");
    assert_eq!(s.generics().expect("generics").params().count(), 1);
    let e = root.nodes().find_map(EnumDecl::cast).expect("enum");
    assert_eq!(e.generics().expect("generics").params().count(), 1);
}

// ----------------------------------------------------------- trait/impl --

#[test]
fn trait_and_impl_members_reentrant() {
    let src = "trait Show[T] {\n    fn show(self) -> str\n    type Out = int\n    const N = 3\n}\nimpl Show for Point {\n    fn show(self) -> str { \"p\" }\n}\n";
    let root = clean(src);
    let t = root.nodes().find_map(TraitDecl::cast).expect("trait");
    let member_kinds: Vec<SyntaxKind> = t.members().map(|m| m.kind).collect();
    assert_eq!(
        member_kinds,
        [
            SyntaxKind::FnDecl,
            SyntaxKind::TypeDecl,
            SyntaxKind::ConstDecl
        ]
    );
    let i = root.nodes().find_map(ImplDecl::cast).expect("impl");
    assert_eq!(
        text(src, i.trait_path().expect("path").syntax().span),
        "Show"
    );
    assert_eq!(text(src, i.self_ty().expect("for type").span), "Point");
    let members: Vec<_> = i.members().collect();
    assert_eq!(members.len(), 1);
    let m = FnDecl::cast(members[0]).expect("member fn");
    assert!(m.body().is_some(), "member fn body fences as BlockPending");
    assert!(
        m.params()
            .expect("params")
            .params()
            .next()
            .expect("self")
            .is_self()
    );
}

#[test]
fn inherent_impl_on_generic_subject() {
    // [gram.item.trait] writes the impl subject as a bare `path`;
    // bracket application on it is accepted leniently (spec issue
    // recorded in the sprint report).
    let src = "impl[T] List[T] {\n    fn len(self) -> int { 0 }\n}\n";
    let root = clean(src);
    let i = root.nodes().find_map(ImplDecl::cast).expect("impl");
    assert_eq!(i.generics().expect("impl generics").params().count(), 1);
    assert_eq!(
        text(src, i.trait_path().expect("path").syntax().span),
        "List"
    );
    assert!(i.self_ty().is_none(), "no `for` clause");
    assert_eq!(i.members().count(), 1);
}

// -------------------------------------------------------------- bindings --

#[test]
fn bindings_let_var_const() {
    let src = "let (a, b): (int, int) = pair\nvar v = 3\nconst C: int = 4\n";
    let root = clean(src);
    let l = root.nodes().find_map(LetDecl::cast).expect("let");
    assert_eq!(l.pattern().expect("pattern").kind, SyntaxKind::TuplePat);
    assert_eq!(l.ty().expect("ty").kind, SyntaxKind::TupleType);
    assert!(l.init().is_some());
    let v = root.nodes().find_map(VarDecl::cast).expect("var");
    assert_eq!(v.pattern().expect("pattern").kind, SyntaxKind::IdentPat);
    let c = root.nodes().find_map(ConstDecl::cast).expect("const");
    assert_eq!(text(src, c.name().expect("name").span), "C");
    assert!(c.init().is_some());
}

#[test]
fn struct_patterns() {
    // `[gram.pat.struct]` (s129, #179): shorthand, explicit + nested,
    // `..` rest, dotted path, `@`-binding over a struct pattern.
    let src = "let Point { x, y } = p\nlet Point { x: a, .. } = p\nlet geo.Point { x } = q\nlet Seg { a: Point { x, .. }, b: _ } = s\nlet whole @ Point { x, .. } = p\n";
    let root = clean(src);
    let pats: Vec<SyntaxKind> = root
        .nodes()
        .filter_map(LetDecl::cast)
        .map(|l| l.pattern().expect("pattern").kind)
        .collect();
    assert_eq!(
        pats,
        [
            SyntaxKind::StructPat,
            SyntaxKind::StructPat,
            SyntaxKind::StructPat,
            SyntaxKind::StructPat,
            SyntaxKind::BindingPat,
        ]
    );
    let first = root
        .nodes()
        .find_map(LetDecl::cast)
        .and_then(|l| l.pattern())
        .and_then(wolf_ast::StructPat::cast)
        .expect("struct pattern");
    assert!(!first.has_rest());
    let names: Vec<&str> = first
        .fields()
        .map(|f| text(src, f.name_span().expect("name")))
        .collect();
    assert_eq!(names, ["x", "y"]);
    // Shorthand members carry their binding as an IdentPat.
    assert!(
        first
            .fields()
            .all(|f| f.pattern().expect("sub").kind == SyntaxKind::IdentPat)
    );
    let second = root
        .nodes()
        .filter_map(LetDecl::cast)
        .nth(1)
        .and_then(|l| l.pattern())
        .and_then(wolf_ast::StructPat::cast)
        .expect("struct pattern");
    assert!(second.has_rest());
    let f = second.fields().next().expect("field");
    assert_eq!(text(src, f.name().expect("name").span), "x");
    assert_eq!(f.pattern().expect("sub").kind, SyntaxKind::IdentPat);
    let nested = root
        .nodes()
        .filter_map(LetDecl::cast)
        .nth(3)
        .and_then(|l| l.pattern())
        .and_then(wolf_ast::StructPat::cast)
        .expect("struct pattern");
    let kinds: Vec<SyntaxKind> = nested
        .fields()
        .map(|f| f.pattern().expect("sub").kind)
        .collect();
    assert_eq!(kinds, [SyntaxKind::StructPat, SyntaxKind::WildcardPat]);
}

#[test]
fn struct_pattern_rest_must_be_last() {
    let src = "let Point { .., x } = p\n";
    let p = util::parse(src);
    assert!(
        !p.diagnostics.is_empty(),
        "`..` before a field must be rejected"
    );
}

#[test]
fn pattern_separators_are_required() {
    // D67 (#190): the production is the law — the comma separates
    // members throughout the family, `..` included. Each lax spelling
    // is E0201; the recovery keeps every member in the tree (as-if-
    // comma), so one deleted comma costs exactly one report.
    for (src, what) in [
        ("let Point { x .. } = p\n", "comma-less `..`"),
        ("let Point { x y } = p\n", "comma-less struct fields"),
        ("let Point { x y z } = p\n", "a comma-less field run"),
        ("let (a b) = pair\n", "comma-less tuple elements"),
        ("let Some(a b) = opt\n", "comma-less payload patterns"),
    ] {
        let p = util::parse(src);
        assert!(
            p.diagnostics
                .iter()
                .any(|d| d.code == codes::EXPECTED_TOKEN),
            "{what} must refuse at E0201: {src:?}"
        );
    }
    // The comma-full spellings stay clean — the tightening narrows
    // the accept set to the production, nothing else.
    for src in [
        "let Point { x, .. } = p\n",
        "let Point { x, y } = p\n",
        "let Point { x, y, } = p\n",
        "let (a, b) = pair\n",
        "let Some(a, b) = opt\n",
    ] {
        let p = util::parse(src);
        assert!(
            p.diagnostics.is_empty(),
            "comma-full pattern must stay clean: {src:?} -> {:?}",
            p.diagnostics.first().map(|d| d.code)
        );
    }
    // One deleted comma, one report (the D22 budget's shape).
    let p = util::parse("let Point { x y, z } = p\n");
    assert_eq!(
        p.diagnostics.len(),
        1,
        "one missing separator is one diagnostic"
    );
}

#[test]
fn expr_list_separators_are_required() {
    // D69 (s132, D67's precedent made family-wide): the struct
    // LITERAL, closure-parameter, and capture-list loops tighten to
    // their productions — the comma separates members; a newline is
    // layout, never a separator. Each lax spelling is E0201; the
    // recovery keeps every member in the tree (as-if-comma), so one
    // deleted comma costs exactly one report.
    for (src, what) in [
        (
            "let p = Point { x: 1 y: 2 }
",
            "comma-less literal fields",
        ),
        (
            "let p = Point { x y }
",
            "comma-less shorthand fields",
        ),
        (
            "let p = Point {
    x: 1
    y: 2
}
",
            "newline-separated literal fields",
        ),
        (
            "let f = fn(a b) a
",
            "comma-less closure params",
        ),
        (
            "let f = fn(mut a b: int) a
",
            "comma-less moded params",
        ),
        (
            "fn g() { unsafe c [a b] { } }
",
            "comma-less captures",
        ),
    ] {
        let p = util::parse(src);
        assert!(
            p.diagnostics
                .iter()
                .any(|d| d.code == codes::EXPECTED_TOKEN),
            "{what} must refuse at E0201: {src:?}"
        );
    }
    // The comma-full spellings stay clean — the tightening narrows
    // the accept set to the production, nothing else. The multi-line
    // literal's trailing layout (a terminator run before `}`) stays
    // the production's own.
    for src in [
        "let p = Point { x: 1, y: 2 }
",
        "let p = Point { x, y }
",
        "let p = Point {
    x: 1,
    y: 2,
}
",
        "let p = Point {
    x: 1,
    y: 2
}
",
        "let f = fn(a, b) a
",
        "fn g() { unsafe c [a, b] { } }
",
    ] {
        let p = util::parse(src);
        assert!(
            p.diagnostics.is_empty(),
            "comma-full spelling must stay clean: {src:?} -> {:?}",
            p.diagnostics.first().map(|d| d.code)
        );
    }
    // One deleted comma, one report (the D22 budget's shape) — and
    // the latch holds per list.
    let p = util::parse(
        "let p = Point { x: 1 y: 2, z: 3 }
",
    );
    assert_eq!(
        p.diagnostics.len(),
        1,
        "one missing separator is one diagnostic"
    );
    let p = util::parse(
        "let p = Point { x: 1 y: 2 z: 3 }
",
    );
    assert_eq!(
        p.diagnostics.len(),
        1,
        "two misses in one list still latch to one report"
    );
}

#[test]
fn binding_patterns() {
    let src = "let A | B = x\nlet all @ (a, b) = pair\nlet Some(v) = opt\nlet io.Error(e) = err\nlet _ = ignore\nlet 42 = answer\n";
    let root = clean(src);
    let pats: Vec<SyntaxKind> = root
        .nodes()
        .filter_map(LetDecl::cast)
        .map(|l| l.pattern().expect("pattern").kind)
        .collect();
    assert_eq!(
        pats,
        [
            SyntaxKind::OrPat,
            SyntaxKind::BindingPat,
            SyntaxKind::PathPat,
            SyntaxKind::PathPat,
            SyntaxKind::WildcardPat,
            SyntaxKind::LiteralPat,
        ]
    );
}

#[test]
fn binding_comma_group() {
    // D63: several complete binders under one keyword, each with its
    // own pattern, optional type, and initializer.
    let src = "var i = 0, c = 1\nlet a: int = 1, (x, y) = pair, b = a\n";
    let root = clean(src);
    let v = root.nodes().find_map(VarDecl::cast).expect("var");
    let bs = v.binders();
    assert_eq!(bs.len(), 2);
    assert_eq!(bs[0].pattern.expect("pattern").kind, SyntaxKind::IdentPat);
    assert!(bs[0].init.is_some());
    assert!(bs[1].init.is_some());
    // The flat accessors read the first binder.
    assert_eq!(v.pattern().expect("pattern").kind, SyntaxKind::IdentPat);
    let l = root.nodes().find_map(LetDecl::cast).expect("let");
    let lb = l.binders();
    assert_eq!(lb.len(), 3);
    assert_eq!(lb[0].ty.expect("ascription").kind, SyntaxKind::PathType);
    assert!(lb[1].ty.is_none());
    assert_eq!(lb[1].pattern.expect("pattern").kind, SyntaxKind::TuplePat);
    assert!(lb[2].init.is_some());
}

#[test]
fn binding_single_stays_flat() {
    // A single binder keeps the pre-D63 flat shape — no `Binder` node.
    let src = "let a = 1\nvar v: int = 2\n";
    let root = clean(src);
    for decl in root.nodes() {
        assert!(
            decl.nodes().all(|n| n.kind != SyntaxKind::Binder),
            "flat binding grew a Binder node"
        );
    }
    let l = root.nodes().find_map(LetDecl::cast).expect("let");
    assert_eq!(l.binders().len(), 1);
    assert!(l.binders()[0].init.is_some());
}

// ---------------------------------------------------------------- types --

fn let_ty(src: &str) -> SyntaxKind {
    let root = clean(src);
    let l = root.nodes().find_map(LetDecl::cast).expect("let");
    l.ty().expect("ascribed type").kind
}

#[test]
fn type_forms() {
    assert_eq!(let_ty("let x: Map[str, int] = y\n"), SyntaxKind::PathType);
    assert_eq!(let_ty("let x: !int = y\n"), SyntaxKind::ErrorUnionType);
    assert_eq!(let_ty("let x: *u8 = y\n"), SyntaxKind::PtrType);
    assert_eq!(let_ty("let x: dyn Show = y\n"), SyntaxKind::DynType);
    assert_eq!(let_ty("let x: (int, str) = y\n"), SyntaxKind::TupleType);
    assert_eq!(let_ty("let x: fn(int) -> int = y\n"), SyntaxKind::FnType);
    assert_eq!(let_ty("let x: type = y\n"), SyntaxKind::TypeType);
    assert_eq!(let_ty("let x: region = y\n"), SyntaxKind::RegionType);
    assert_eq!(let_ty("let x: shared Config = y\n"), SyntaxKind::PrefixType);
    assert_eq!(let_ty("let x: handle Node = y\n"), SyntaxKind::PrefixType);
    assert_eq!(let_ty("let x: weak Parent = y\n"), SyntaxKind::PrefixType);
    assert_eq!(let_ty("let x: distinct f64 = y\n"), SyntaxKind::PrefixType);
}

#[test]
fn type_args_types_and_const_generics() {
    let src = "let x: Map[str, List[(T, int)]] = y\nlet b: Buf[N * 2, int] = z\n";
    let root = clean(src);
    let lets: Vec<_> = root.nodes().filter_map(LetDecl::cast).collect();
    assert_eq!(lets.len(), 2);
    // Nested pure-type application.
    let map = wolf_ast::PathType::cast(lets[0].ty().expect("ty")).expect("path type");
    let args: Vec<SyntaxKind> = map.args().expect("args").args().map(|a| a.kind).collect();
    assert_eq!(args, [SyntaxKind::PathType, SyntaxKind::PathType]);
    // Expression-shaped const-generic argument parks as TypeArgPending.
    let buf = wolf_ast::PathType::cast(lets[1].ty().expect("ty")).expect("path type");
    let args: Vec<SyntaxKind> = buf.args().expect("args").args().map(|a| a.kind).collect();
    assert_eq!(args, [SyntaxKind::TypeArgPending, SyntaxKind::PathType]);
}

// -------------------------------------------------- bodies & resilience --

#[test]
fn bodies_are_parsed_and_lossless() {
    let src =
        "fn main() {\n    let s = \"nested {interp} stuff\"\n    if x { y() } else { z() }\n}\n";
    let root = clean(src);
    let f = root.nodes().find_map(FnDecl::cast).expect("fn");
    let body = f.body().expect("body");
    assert_eq!(body.syntax().kind, SyntaxKind::Block);
    assert_eq!(
        text(src, body.syntax().span),
        "{\n    let s = \"nested {interp} stuff\"\n    if x { y() } else { z() }\n}"
    );
    // s09 opened the fences: the body has real statement structure.
    assert_eq!(body.statements().count(), 2);
}

#[test]
fn semicolon_terms_between_items() {
    let src = "fn a() { };\nfn b() { }\n";
    let root = clean(src);
    assert_eq!(root.nodes().filter_map(FnDecl::cast).count(), 2);
}

#[test]
fn total_on_nasty_inputs() {
    // No panics, complete lossless trees — whatever the bytes.
    for src in [
        "",
        "\n\n\n",
        "fn",
        "#[",
        "\"",
        "{{{{",
        "]]]",
        "fn f( fn g( fn h(",
        "let = = =",
        "🐺🐺 fn ok() { }",
        "use use use",
        "impl impl impl",
        "type type type",
        "trait T { trait U { } }",
        "fn f() -> -> int { }",
    ] {
        let _ = util::parse(src); // util asserts verify + lossless
    }
}
