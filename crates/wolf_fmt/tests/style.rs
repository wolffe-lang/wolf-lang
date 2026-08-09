//! The canonical style, locked as tests (s11 Target 1).
//!
//! Spec/01 §7 `[gram.fmt.*]` is the authority; every decision the spec
//! leaves open is *encoded here* — this file is the style decision
//! record. Changing any expectation below is a style change and rides
//! the D36/RFC-2437 stability policy (edition-gated once settled).

fn fmt(src: &str) -> String {
    let out = wolf_fmt::format_text(src.as_bytes());
    assert!(!out.fell_back, "self-check fell back for {src:?}");
    assert!(!out.partial, "unexpected syntax errors in {src:?}");
    String::from_utf8(out.text).expect("formatter output is UTF-8")
}

#[track_caller]
fn check(src: &str, want: &str) {
    let got = fmt(src);
    assert_eq!(
        got, want,
        "\n== input ==\n{src}\n== got ==\n{got}\n== want ==\n{want}"
    );
    // Everything the suite locks is idempotent by construction.
    assert_eq!(fmt(&got), got, "not idempotent");
}

// ------------------------------------------------- [gram.fmt.indent] ----

#[test]
fn four_space_indent_no_tabs() {
    check(
        "fn main() {\n\tlet x = 1\n  let y = 2\n}\n",
        "fn main() {\n    let x = 1\n    let y = 2\n}\n",
    );
}

#[test]
fn width_100_breaks_arg_lists_with_trailing_comma() {
    // 6 × 18-char args + callee > 100 → one per line, trailing comma.
    let src = "fn main() { f(aaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbbbbb, cccccccccccccccccc, dddddddddddddddddd, eeeeeeeeeeeeeeeeee) }\n";
    check(
        src,
        "fn main() {\n    f(\n        aaaaaaaaaaaaaaaaaa,\n        bbbbbbbbbbbbbbbbbb,\n        cccccccccccccccccc,\n        dddddddddddddddddd,\n        eeeeeeeeeeeeeeeeee,\n    )\n}\n",
    );
}

#[test]
fn fitting_lists_collapse_to_one_line_without_trailing_comma() {
    // Decision: paren/bracket lists are fit-based — a list written
    // multiline that fits in 100 columns collapses ([gram.fmt.commas]:
    // trailing comma multiline, none inline).
    check(
        "fn main() {\n    let d = dot(a,\n                b)\n}\n",
        "fn main() {\n    let d = dot(a, b)\n}\n",
    );
}

// -------------------------------------------------- [gram.fmt.brace] ----

#[test]
fn open_brace_on_construct_line_and_else_on_close_line() {
    // `[gram.fmt.brace]` is mostly grammar-enforced (a newline before
    // `{` or `else` cannot parse); the formatter's half is keeping the
    // canonical multiline shape stable.
    check(
        "fn main() {\n    if c {\n        a()\n    } else {\n        b()\n    }\n}\n",
        "fn main() {\n    if c {\n        a()\n    } else {\n        b()\n    }\n}\n",
    );
}

#[test]
fn exactly_one_blank_line_between_items() {
    check(
        "fn a() { 1 }\nfn b() { 2 }\n\n\n\nfn c() { 3 }\n",
        "fn a() { 1 }\n\nfn b() { 2 }\n\nfn c() { 3 }\n",
    );
}

#[test]
fn blank_lines_inside_items_cap_at_one() {
    check(
        "fn main() {\n    let a = 1\n\n\n\n    let b = 2\n}\n",
        "fn main() {\n    let a = 1\n\n    let b = 2\n}\n",
    );
}

// -------------------------------------------- [gram.fmt.continuation] ----

#[test]
fn short_continuations_collapse() {
    check(
        "fn main() {\n    let a = 1 +\n        2\n}\n",
        "fn main() {\n    let a = 1 + 2\n}\n",
    );
}

#[test]
fn long_binary_chains_break_after_the_operator() {
    let src = "fn main() { let result = aaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbb + cccccccccccccccccccccccccccc + dddddddddddddddddddd }\n";
    check(
        src,
        "fn main() {\n    let result = aaaaaaaaaaaaaaaaaaaaaaaaaaaa +\n        bbbbbbbbbbbbbbbbbbbbbbbbbbbb +\n        cccccccccccccccccccccccccccc +\n        dddddddddddddddddddd\n}\n",
    );
}

#[test]
fn long_member_chains_break_after_the_dot() {
    let src = "fn main() { let v = collection.aaaaaaaaaaaaaaaaaaaa().bbbbbbbbbbbbbbbbbbbb().cccccccccccccccccccc().dddddddddddddddddddd() }\n";
    check(
        src,
        "fn main() {\n    let v = collection.\n        aaaaaaaaaaaaaaaaaaaa().\n        bbbbbbbbbbbbbbbbbbbb().\n        cccccccccccccccccccc().\n        dddddddddddddddddddd()\n}\n",
    );
}

// -------------------------------------------------- [gram.fmt.inline] ----

#[test]
fn guard_clause_blocks_stay_inline_with_semicolons() {
    check(
        "fn main() {\n    if xs.is_empty() { print(usage); return 2 }\n}\n",
        "fn main() {\n    if xs.is_empty() { print(usage); return 2 }\n}\n",
    );
}

#[test]
fn three_statement_inline_blocks_break_and_lose_semicolons() {
    check(
        "fn main() {\n    if c { a(); b(); d() }\n}\n",
        "fn main() {\n    if c {\n        a()\n        b()\n        d()\n    }\n}\n",
    );
}

#[test]
fn multiline_blocks_are_never_joined() {
    // Decision: the formatter breaks blocks but never joins them — a
    // block the author wrote multiline stays multiline even when it
    // would fit (source-respecting, gofmt lineage).
    check(
        "fn main() {\n    if c {\n        a()\n    }\n}\n",
        "fn main() {\n    if c {\n        a()\n    }\n}\n",
    );
}

#[test]
fn stray_semicolons_between_statements_are_stripped() {
    check(
        "fn main() {\n    let x = 1;\n    x.go();\n    0\n}\n",
        "fn main() {\n    let x = 1\n    x.go()\n    0\n}\n",
    );
}

#[test]
fn statements_never_share_a_line_outside_inline_blocks() {
    check(
        "fn main() {\n    a(); b()\n    0\n}\n",
        "fn main() {\n    a()\n    b()\n    0\n}\n",
    );
}

// ------------------------------------------------- [gram.fmt.imports] ----

#[test]
fn imports_sort_std_first_then_packages_then_import_c() {
    check(
        "import c \"stdlib.h\"\nuse zzz.last\nuse std.net\nuse std.fs\n\nfn main() { 0 }\n",
        "use std.fs\nuse std.net\nuse zzz.last\nimport c \"stdlib.h\"\n\nfn main() { 0 }\n",
    );
}

#[test]
fn import_block_gets_one_blank_after_and_none_within() {
    check(
        "use std.fs\n\n\nuse std.net\nfn main() { 0 }\n",
        "use std.fs\nuse std.net\n\nfn main() { 0 }\n",
    );
}

// -------------------------------------------------- else-if collapse ----

#[test]
fn else_block_holding_only_an_if_collapses_to_else_if() {
    check(
        "fn main() {\n    if a { 1 } else { if b { 2 } else { 3 } }\n}\n",
        "fn main() {\n    if a { 1 } else if b { 2 } else { 3 }\n}\n",
    );
}

// ---------------------------------------------------- paren dropping ----

#[test]
fn redundant_parens_drop_per_the_precedence_table() {
    check(
        "fn main() {\n    let a = (x) + ((y))\n    let b = (x * y) + z\n    let c = f((x))\n}\n",
        "fn main() {\n    let a = x + y\n    let b = x * y + z\n    let c = f(x)\n}\n",
    );
}

#[test]
fn needed_parens_are_kept() {
    let keep = [
        // precedence: child looser than parent
        "fn main() { let a = (x + y) * z }\n",
        // non-associative comparison: dropping would be E0003
        "fn main() { let a = (x < y) == z }\n",
        // struct literal in condition position: dropping would be E0006
        "fn main() { if p == (Point { x: 0 }) { 0 } else { 1 } }\n",
        // closure extent: dropping would swallow the call
        "fn main() { let a = (fn(v) v + 1)(3) }\n",
        // range endpoints only take tier-13 operands
        "fn main() { let r = (a..b).contains(x) }\n",
        // spec/01 §3.2 spells prefix `shared` over a struct literal
        // with parens; the formatter honors that spelling
        "fn main() { let a = shared (Cfg { limit: 7 }) }\n",
    ];
    for src in keep {
        check(src, src);
    }
}

#[test]
fn parens_around_negative_literal_index_are_kept() {
    // Dropping would turn the index into the E0209 counter-example.
    check(
        "fn main() { let a = xs[(-1)] }\n",
        "fn main() { let a = xs[(-1)] }\n",
    );
}

// ------------------------------------------------- match and select ----

#[test]
fn match_bodies_are_multiline_with_trailing_commas_either_way() {
    check(
        "fn main() {\n    match e { A => 1, B(x) => { go(x) }\n        C => 3 }\n}\n",
        "fn main() {\n    match e {\n        A => 1,\n        B(x) => { go(x) },\n        C => 3,\n    }\n}\n",
    );
}

// ------------------------------------------------------ punctuation ----

#[test]
fn operator_spacing_is_canonical() {
    check(
        "fn main() {\n    let a = i+1\n    let r = 0 ..  n\n    let s = xs[^1 ..]\n    let p = q as *u8\n    let m = &mut x\n    let t = (1,)\n    let n2 = -x.abs()\n}\n",
        "fn main() {\n    let a = i + 1\n    let r = 0..n\n    let s = xs[^1..]\n    let p = q as *u8\n    let m = &mut x\n    let t = (1,)\n    let n2 = -x.abs()\n}\n",
    );
}

#[test]
fn struct_defs_respect_source_multiline_and_gain_trailing_commas() {
    check(
        "struct Vec3 { x: f64, y: f64, z: f64 }\n\nstruct Node {\n    value: int,\n    next: handle Node\n}\n",
        "struct Vec3 { x: f64, y: f64, z: f64 }\n\nstruct Node {\n    value: int,\n    next: handle Node,\n}\n",
    );
}

#[test]
fn fn_headers_and_error_rows_are_tight() {
    check(
        "fn digit(s: str, i: int) -> int ! {BadDigit(ParseError), TooShort} { 0 }\n",
        "fn digit(s: str, i: int) -> int ! {BadDigit(ParseError), TooShort} { 0 }\n",
    );
}

#[test]
fn hugged_trailing_block_arguments_keep_the_call_flat() {
    check(
        "fn main() {\n    let counts = args.par(fn(path) {\n        count(path)\n    })?\n}\n",
        "fn main() {\n    let counts = args.par(fn(path) {\n        count(path)\n    })?\n}\n",
    );
}

// ---------------------------------------------------------- strings ----

#[test]
fn string_episodes_are_verbatim() {
    // Decision: strings (plain, multiline, raw, generalized) are never
    // rewritten or re-flowed at v1, interpolations included —
    // [gram.fmt.strings]'s preferences apply to new code, not to
    // rewriting existing literals (a stability-tier decision deferred,
    // like doc-comment reflow).
    check(
        "fn main() {\n    let s = \"a {x:>8} b\"\n    let r = r\"raw \\ text\"\n    let m = \"\"\"\n        line\n        \"\"\"\n    0\n}\n",
        "fn main() {\n    let s = \"a {x:>8} b\"\n    let r = r\"raw \\ text\"\n    let m = \"\"\"\n        line\n        \"\"\"\n    0\n}\n",
    );
}

// --------------------------------------------------------- comments ----

#[test]
fn comment_fidelity() {
    // Leading comments stay with their statement; trailing comments
    // keep their hand alignment; doc comments are never re-flowed;
    // dangling comments in empty blocks survive.
    check(
        "/// Documented — never re-flowed even though this line is quite short.\nfn a() { 1 }\n\nfn main() {\n    // leading comment\n    let x = 1      // aligned trailing comment\n    let long = 2\n}\n\nfn empty() {\n    // dangling\n}\n",
        "/// Documented — never re-flowed even though this line is quite short.\nfn a() { 1 }\n\nfn main() {\n    // leading comment\n    let x = 1      // aligned trailing comment\n    let long = 2\n}\n\nfn empty() {\n    // dangling\n}\n",
    );
}

#[test]
fn inner_doc_header_is_preserved_untouched_with_one_blank_after() {
    check(
        "//! check: run(exit=0)\n//! phase: parse\n//!\n//! Prose.\nfn main() { 0 }\n",
        "//! check: run(exit=0)\n//! phase: parse\n//!\n//! Prose.\n\nfn main() { 0 }\n",
    );
}

#[test]
fn trailing_comment_continuation_keeps_its_column() {
    check(
        "fn main() {\n    xs.push(xs.len)                // read xs while its tag\n                                   // stays Reserved\n    0\n}\n",
        "fn main() {\n    xs.push(xs.len)                // read xs while its tag\n                                   // stays Reserved\n    0\n}\n",
    );
}

// ------------------------------------------------------ empty blocks ----

#[test]
fn empty_blocks_close_up() {
    check("fn main() {\n}\n", "fn main() {}\n");
}

// ------------------------------------------------- container bodies ----

#[test]
fn trait_and_impl_bodies_are_always_multiline() {
    check(
        "trait Show {\n    fn show(self) -> str\n}\n\nimpl Show for P {\n    fn show(self) -> str { \"p\" }\n}\n",
        "trait Show {\n    fn show(self) -> str\n}\n\nimpl Show for P {\n    fn show(self) -> str { \"p\" }\n}\n",
    );
}

// ------------------------------------------------------ file shape ----

#[test]
fn file_ends_with_exactly_one_newline() {
    check("fn main() { 0 }", "fn main() { 0 }\n");
    check("fn main() { 0 }\n\n\n", "fn main() { 0 }\n");
}
