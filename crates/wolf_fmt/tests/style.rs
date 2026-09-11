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
    // `[gram.fmt.brace]`'s `{` half is grammar-enforced (a newline
    // before `{` cannot parse); the `} else` half is the formatter's
    // since wolf-lang#276 admitted a leading `else` (the test below),
    // and this one keeps the canonical multiline shape stable.
    check(
        "fn main() {\n    if c {\n        a()\n    } else {\n        b()\n    }\n}\n",
        "fn main() {\n    if c {\n        a()\n    } else {\n        b()\n    }\n}\n",
    );
}

#[test]
fn a_leading_else_is_relaid_onto_the_closing_brace_line() {
    // wolf-lang#276: the parser admits an `else` that starts a line
    // (`[gram.lex.newline]`'s lookahead); `[gram.fmt.brace]` does not
    // move, so the maintainer's aligned layout formats to the canonical
    // `} else` form — and `check` proves the result is a fixed point.
    check(
        "fn main() {\n    if (s[i..i+1] == \"\\t\")      { t += \"<tab>\" }\n    else if (s[i..i+1] == \"\\n\") { t += \"<nl>\" }\n    else                        { t += s[i..i+1] }\n}\n",
        "fn main() {\n    if s[i..i + 1] == \"\\t\" { t += \"<tab>\" } else if s[i..i + 1] == \"\\n\" { t += \"<nl>\" } else {\n        t += s[i..i + 1]\n    }\n}\n",
    );
    // The multiline shape: `}` newline `else {` becomes `} else {`.
    check(
        "fn main() {\n    if c {\n        a()\n    }\n    else {\n        b()\n    }\n}\n",
        "fn main() {\n    if c {\n        a()\n    } else {\n        b()\n    }\n}\n",
    );
    // And the defaulting operator across lines is re-laid trailing.
    check(
        "fn main() {\n    let v = f()\n        else 0\n}\n",
        "fn main() {\n    let v = f() else 0\n}\n",
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

// ------------------------------------------- [gram.item.let] groups ----

#[test]
fn binding_group_fits_on_one_line() {
    // D63/D34: a comma group is ONE statement; spacing is `a = 1, b = 2`.
    check(
        "fn main() {\n    var i=0,c=1\n    let a = 2  ,  b = a + 3\n}\n",
        "fn main() {\n    var i = 0, c = 1\n    let a = 2, b = a + 3\n}\n",
    );
}

#[test]
fn binding_group_breaks_one_binder_per_line() {
    // Over 100 columns the group breaks one-binder-per-line with a
    // continuation indent — never into separate statements.
    let src = "fn main() {\n    let quite_a_long_name = compute_something(1, 2, 3), another_long_name = compute_something(4, 5, 6), third_one = 9\n}\n";
    check(
        src,
        "fn main() {\n    let quite_a_long_name = compute_something(1, 2, 3),\n        another_long_name = compute_something(4, 5, 6),\n        third_one = 9\n}\n",
    );
}

#[test]
fn binding_group_never_splits_into_statements() {
    // A fitting multi-line spelling collapses back to the group.
    check(
        "fn main() {\n    let a = 1,\n        b = 2\n}\n",
        "fn main() {\n    let a = 1, b = 2\n}\n",
    );
}

// ---------------------------------------------------- [gram.fmt.if] ----
//
// s151 (wolf-lang#307): the formatter never converts between the braced
// and the bare `if` — the author's choice stands. Inside a form it
// normalizes; the one conversion is the width fallback, bare → braced.

#[test]
fn bare_if_stays_bare_and_braced_stays_braced() {
    let bare = "fn f(leap: bool) -> int {\n    if leap then 29 else 28\n}\n";
    check(bare, bare);
    let inline = "fn f(leap: bool) -> int {\n    if leap { 29 } else { 28 }\n}\n";
    check(inline, inline);
    let braced = "fn f(leap: bool) -> int {\n    if leap {\n        29\n    } else {\n        28\n    }\n}\n";
    check(braced, braced);
}

#[test]
fn bare_if_normalizes_spacing_inside_the_form() {
    check(
        "fn f(leap: bool) -> int {\n    if   leap   then   29   else   28\n}\n",
        "fn f(leap: bool) -> int {\n    if leap then 29 else 28\n}\n",
    );
    // One-armed, in statement position.
    let stmt = "fn f(c: bool) {\n    if c then print(\"a\")\n}\n";
    check(stmt, stmt);
}

#[test]
fn then_before_a_block_is_dropped() {
    // The braced form has one spelling.
    check(
        "fn f(leap: bool) -> int {\n    if leap then { 29 } else { 28 }\n}\n",
        "fn f(leap: bool) -> int {\n    if leap { 29 } else { 28 }\n}\n",
    );
}

#[test]
fn bare_if_keeps_the_parens_around_a_defaulting_else() {
    // `[gram.amb.else]`: the parens are what keep `else 0` from the
    // `if`'s own `else` — in either branch.
    let src = "fn f(c: bool) -> int {\n    let v = if c then (parse(s) else 0) else (parse(t) else 1)\n    v\n}\n";
    check(src, src);
    // Redundant parens elsewhere in a bare branch still go.
    check(
        "fn f(c: bool) -> int {\n    if c then (29) else (28)\n}\n",
        "fn f(c: bool) -> int {\n    if c then 29 else 28\n}\n",
    );
}

#[test]
fn if_chain_links_keep_their_own_form() {
    let bare = "fn f(n: int) -> str {\n    if n < 0 then \"neg\" else if n == 0 then \"zero\" else \"pos\"\n}\n";
    check(bare, bare);
    // A braced link inside a bare chain keeps its braces and does not
    // make the bare head break.
    let mixed = "fn f(n: int) -> str {\n    if n < 0 then \"neg\" else if n == 0 {\n        \"zero\"\n    } else {\n        \"pos\"\n    }\n}\n";
    check(mixed, mixed);
    let tail = "fn f(n: int) -> str {\n    if n < 0 { \"neg\" } else if n == 0 then \"zero\" else \"pos\"\n}\n";
    check(tail, tail);
}

#[test]
fn bare_if_past_the_width_breaks_to_the_braced_form_once() {
    // The only conversion, one direction: the broken rendering IS the
    // braced form, and `check`'s idempotence assertion is the fixed
    // point (a braced `if` never comes back).
    let long = "if some_rather_long_condition(alpha, beta) then compute_the_first_branch(gamma) else compute_the_other_branch(delta)";
    assert!(4 + long.len() > 100);
    check(
        &format!("fn f() -> int {{\n    {long}\n}}\n"),
        "fn f() -> int {\n    if some_rather_long_condition(alpha, beta) {\n        compute_the_first_branch(gamma)\n    } else {\n        compute_the_other_branch(delta)\n    }\n}\n",
    );
    // A bare chain breaks as one.
    let chain = "if aaaaaaaaaaaaaaaaaaaaaaaaaaaa then 1 else if bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb then 2 else 33333333333333333333";
    assert!(4 + chain.len() > 100);
    check(
        &format!("fn f() -> int {{\n    {chain}\n}}\n"),
        "fn f() -> int {\n    if aaaaaaaaaaaaaaaaaaaaaaaaaaaa {\n        1\n    } else if bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb {\n        2\n    } else {\n        33333333333333333333\n    }\n}\n",
    );
    // One-armed.
    let one = "if some_rather_long_condition(alpha, beta, gamma, delta) then do_the_one_thing(epsilon, zeta, eta)";
    assert!(4 + one.len() > 100);
    check(
        &format!("fn f() {{\n    {one}\n}}\n"),
        "fn f() {\n    if some_rather_long_condition(alpha, beta, gamma, delta) {\n        do_the_one_thing(epsilon, zeta, eta)\n    }\n}\n",
    );
}

#[test]
fn bare_if_with_a_leading_else_is_relaid_trailing() {
    // `[gram.lex.newline]` admits the leading `else` in the bare form
    // too; the formatter re-lays it on one line (`[gram.fmt.brace]`).
    check(
        "fn f(c: bool) -> int {\n    let v = if c then f()\n        else 0\n    v\n}\n",
        "fn f(c: bool) -> int {\n    let v = if c then f() else 0\n    v\n}\n",
    );
}

#[test]
fn bare_if_in_a_match_arm_and_a_call() {
    let arm = "fn days(m: int, leap: bool) -> int {\n    match m {\n        2 => if leap then 29 else 28,\n        4 | 6 | 9 | 11 => 30,\n        _ => 31,\n    }\n}\n";
    check(arm, arm);
    let call = "fn f(c: bool) -> int {\n    g(if c then 1 else 2, 3)\n}\n";
    check(call, call);
}

#[test]
fn then_is_an_identifier_everywhere_else() {
    let src = "fn f(then: bool, a: Ordering, b: Ordering) -> int {\n    if then { 1 } else if a.then(b) == a then 2 else 3\n}\n";
    check(src, src);
}
