//! One test per parser diagnostic code, each pinning the *structured*
//! diagnostic (code, severity, spans, message, notes) in an insta
//! snapshot — the s10 catalog's seed.

mod util;

use wolf_parse::codes;

/// Parse, pick the first diagnostic with `code`, snapshot its Debug
/// form under `name`.
fn snap(name: &str, src: &str, code: wolf_diag::Code) {
    let parse = util::parse(src);
    let d = parse
        .diagnostics
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| {
            panic!(
                "no {code} diagnostic for {src:?}; got {:?}",
                parse.diagnostics
            )
        });
    insta::assert_snapshot!(name, format!("{d:#?}"));
}

#[test]
fn e0008_keyword_used_as_identifier() {
    // corpus/grammar/when_reserved.lu is the conformance fixture for
    // this: `fn when(` must produce E0008.
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/grammar/when_reserved.lu");
    let src = std::fs::read_to_string(fixture).expect("read when_reserved.lu");
    let parse = util::parse(&src);
    assert_eq!(
        parse
            .diagnostics
            .iter()
            .filter(|d| d.code == codes::KEYWORD_AS_IDENT)
            .count(),
        1,
        "when_reserved.lu must produce exactly one E0008"
    );
    snap("e0008_when_reserved", &src, codes::KEYWORD_AS_IDENT);
    // And the minimal spelling, message pinned too.
    snap(
        "e0008_minimal",
        "fn when(a: int) { }\n",
        codes::KEYWORD_AS_IDENT,
    );
}

/// #243 (s138): a keyword where a parameter name goes is ONE report.
/// E0008 names the keyword and consumes it as the name; the type is
/// demanded only when a `:` says the writer meant a parameter here.
/// Without one, "expected `:` after the parameter name" one line after
/// "this is not a name" was the same confusion twice — and in a call's
/// argument list re-keyed as a header by a stray `fn`, the second
/// report was the one that overran the blast-radius bound.
#[test]
fn e0008_keyword_parameter_reports_once() {
    let count = |src: &str, code: wolf_diag::Code| {
        util::parse(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == code)
            .count()
    };
    for src in ["fn f(true, x: int) { }\n", "fn f(true) { }\n"] {
        assert_eq!(count(src, codes::KEYWORD_AS_IDENT), 1, "{src:?}: the name");
        assert_eq!(
            count(src, codes::EXPECTED_TOKEN),
            0,
            "{src:?}: no second report on the same token"
        );
    }
    // A `:` says the writer meant a parameter: the name is the report,
    // the type parses.
    assert_eq!(count("fn f(true: int) { }\n", codes::KEYWORD_AS_IDENT), 1);
    assert_eq!(count("fn f(true: int) { }\n", codes::EXPECTED_TOKEN), 0);
    // A REAL name with no type keeps its report — this is not a licence
    // to stop asking for types.
    assert_eq!(count("fn f(x) { }\n", codes::EXPECTED_TOKEN), 1);
}

#[test]
fn e0201_pattern_separator_fix() {
    // D67 (#190): the pattern family's required separator, with the
    // machine-applicable "add the comma" fix — one zero-width
    // insertion flush against the previous member (`x, ..`, never
    // `x ,..`).
    let src = "fn f() { let Point { x .. } = p\n}\n";
    let parse = util::parse(src);
    let d = parse
        .diagnostics
        .iter()
        .find(|d| d.code == codes::EXPECTED_TOKEN)
        .expect("E0201 for the comma-less `..`");
    let sugg = &d.suggestions[0];
    assert_eq!(
        sugg.applicability,
        wolf_diag::Applicability::MachineApplicable
    );
    assert_eq!(sugg.edits.len(), 1);
    assert_eq!(sugg.edits[0].1, ",");
    assert_eq!(
        sugg.edits[0].0.lo, sugg.edits[0].0.hi,
        "the fix is an insertion"
    );
    snap("e0201_rest_needs_comma", src, codes::EXPECTED_TOKEN);
    snap(
        "e0201_fields_need_comma",
        "fn f() { let Point { x y } = p\n}\n",
        codes::EXPECTED_TOKEN,
    );
}

#[test]
fn e0201_expr_list_separator_fix() {
    // D69 (s132): the expression-list separators — struct-literal
    // fields, closure params, capture names — carry the same
    // machine-applicable "add the comma" insertion as D67's pattern
    // family: one zero-width edit flush against the previous member.
    let src = "fn f() { let p = Point { x: 1 y: 2 }\n}\n";
    let parse = util::parse(src);
    let d = parse
        .diagnostics
        .iter()
        .find(|d| d.code == codes::EXPECTED_TOKEN)
        .expect("E0201 for the comma-less literal fields");
    let sugg = &d.suggestions[0];
    assert_eq!(
        sugg.applicability,
        wolf_diag::Applicability::MachineApplicable
    );
    assert_eq!(sugg.edits.len(), 1);
    assert_eq!(sugg.edits[0].1, ",");
    assert_eq!(
        sugg.edits[0].0.lo, sugg.edits[0].0.hi,
        "the fix is an insertion"
    );
    snap(
        "e0201_literal_fields_need_comma",
        src,
        codes::EXPECTED_TOKEN,
    );
    snap(
        "e0201_closure_params_need_comma",
        "fn f() { let g = fn(a b) a\n}\n",
        codes::EXPECTED_TOKEN,
    );
}

#[test]
fn e0201_expected_token() {
    // (`fn (` reads as a stray closure line — E0203 — so the missing
    // name is pinned on a generic header instead.)
    snap(
        "e0201_missing_name",
        "fn [T](x: int) { }\n",
        codes::EXPECTED_TOKEN,
    );
    snap("e0201_missing_init", "let x\n", codes::EXPECTED_TOKEN);
    // A bare `..` is outside `[gram.expr.primary]`: both alternatives
    // require an endpoint, and the parser used to admit it anyway, so
    // `s[..]` ran and `s[..=]` trapped (wolf-lang#88).
    snap(
        "e0201_bare_range",
        "fn f(s: str) -> str { s[..] }\n",
        codes::EXPECTED_TOKEN,
    );
    snap(
        "e0201_bare_range_inclusive",
        "fn f(s: str) -> str { s[..=] }\n",
        codes::EXPECTED_TOKEN,
    );
}

/// `[gram.pat.range]` (s147, #287): the open ranges are refused BY
/// NAME in pattern position — `lo..` at its missing high end, `..hi`
/// at the operator — each with the note that spells the closed form.
/// (The book's ch03 papercut was a diagnostic that never said "range".)
#[test]
fn e0201_open_range_patterns() {
    snap(
        "e0201_range_pat_open_high",
        "fn f(n: int) -> int {\n    match n {\n        10.. => 1,\n        _ => 0,\n    }\n}\n",
        codes::EXPECTED_TOKEN,
    );
    snap(
        "e0201_range_pat_open_low",
        "fn f(n: int) -> int {\n    match n {\n        ..10 => 1,\n        _ => 0,\n    }\n}\n",
        codes::EXPECTED_TOKEN,
    );
}

/// D63's two refusal teach-notes: one initializer for several names
/// offers both spellings; the Python bare tuple is refused by name.
#[test]
fn e0201_binding_group_teach_notes() {
    snap(
        "e0201_group_one_init_many_names",
        "var i, c = 0\n",
        codes::EXPECTED_TOKEN,
    );
    snap(
        "e0201_group_bare_tuple",
        "let a, b = 1, 2\n",
        codes::EXPECTED_TOKEN,
    );
    // A group with no initializer anywhere keeps the production's
    // plain letter — no teach-note.
    snap(
        "e0201_group_uninitialized",
        "var i, c\n",
        codes::EXPECTED_TOKEN,
    );
}

/// The endpoint rule is one-sided: only the TRAILING endpoint is
/// optional, so `a..`, `..b` and `a..b` all parse clean. Pinned beside
/// the refusal so a future tightening cannot quietly take them too.
#[test]
fn range_forms_with_an_endpoint_parse_clean() {
    for src in [
        "fn f(s: str) -> str { s[1..] }\n",
        "fn f(s: str) -> str { s[..1] }\n",
        "fn f(s: str) -> str { s[0..1] }\n",
        "fn f(s: str) -> str { s[..=1] }\n",
        "fn f(s: str) -> str { s[^2..] }\n",
        "fn f() -> int { var n = 0\n for i in 0..3 { n += i }\n n }\n",
    ] {
        let parse = util::parse(src);
        assert!(
            parse
                .diagnostics
                .iter()
                .all(|d| d.severity != wolf_diag::Severity::Error),
            "{src:?} must parse clean, got {:?}",
            parse.diagnostics
        );
    }
}

#[test]
fn e0202_unclosed_delimiter() {
    snap(
        "e0202_unclosed_brace",
        "fn f() {\n",
        codes::UNCLOSED_DELIMITER,
    );
    snap(
        "e0202_unclosed_paren",
        "fn f(a: int\nfn g() { }\n",
        codes::UNCLOSED_DELIMITER,
    );
}

#[test]
fn e0203_unexpected_top_level_tokens() {
    snap("e0203_stray_expr", "1 + 2\n", codes::UNEXPECTED_TOPLEVEL);
}

#[test]
fn e0204_malformed_attribute() {
    snap(
        "e0204_attr_garbage",
        "#[)]\nfn f() { }\n",
        codes::MALFORMED_ATTRIBUTE,
    );
}

#[test]
fn e0211_inner_attribute_after_first_item() {
    snap(
        "e0211_inner_attr_misplaced",
        "fn f() { }\n#![index(1)]\nfn g() { }\n",
        codes::MISPLACED_INNER_ATTRIBUTE,
    );
}

#[test]
fn e0211_inner_attribute_in_statement_position() {
    snap(
        "e0211_inner_attr_in_block",
        "fn f() {\n    #![index(1)]\n    let x = 1\n}\n",
        codes::MISPLACED_INNER_ATTRIBUTE,
    );
}

#[test]
fn e0205_malformed_generics() {
    snap(
        "e0205_bad_generics",
        "fn f[[T]](x: int) { }\n",
        codes::MALFORMED_GENERICS,
    );
}

#[test]
fn e0206_expected_type() {
    snap("e0206_missing_type", "let x: = 1\n", codes::EXPECTED_TYPE);
}

#[test]
fn e0207_expected_pattern() {
    snap(
        "e0207_missing_pattern",
        "let = 1\n",
        codes::EXPECTED_PATTERN,
    );
}

// ---------------------------------------------- spec §9 codes (s09) ------

#[test]
fn e0001_leading_operator_continuation() {
    // corpus/grammar/newline_leading.lu is the conformance fixture.
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/grammar/newline_leading.lu");
    let src = std::fs::read_to_string(fixture).expect("read newline_leading.lu");
    let parse = util::parse(&src);
    assert_eq!(
        parse.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        [codes::LEADING_OPERATOR],
        "newline_leading.lu must fail with exactly E0001"
    );
    snap("e0001_leading_operator", &src, codes::LEADING_OPERATOR);
}

#[test]
fn e0002_empty_statement() {
    // corpus/grammar/semicolon.lu is the conformance fixture.
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/grammar/semicolon.lu");
    let src = std::fs::read_to_string(fixture).expect("read semicolon.lu");
    let parse = util::parse(&src);
    assert_eq!(
        parse.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        [codes::EMPTY_STATEMENT],
        "semicolon.lu must fail with exactly E0002"
    );
    snap("e0002_empty_statement", &src, codes::EMPTY_STATEMENT);
}

#[test]
fn e0003_comparison_chain() {
    snap(
        "e0003_comparison_chain",
        "fn f() { let x = a < b < c\n}\n",
        codes::COMPARISON_CHAIN,
    );
}

#[test]
fn e0006_struct_literal_in_condition() {
    // corpus/grammar/structlit_cond.lu is the conformance fixture.
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/grammar/structlit_cond.lu");
    let src = std::fs::read_to_string(fixture).expect("read structlit_cond.lu");
    let parse = util::parse(&src);
    assert_eq!(
        parse.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        [codes::STRUCT_LIT_IN_COND],
        "structlit_cond.lu must fail with exactly E0006"
    );
    snap("e0006_structlit_cond", &src, codes::STRUCT_LIT_IN_COND);
}

#[test]
fn e0007_interp_nesting_too_deep() {
    let mut lit = String::from("\"x\"");
    for _ in 0..8 {
        lit = format!("\"{{{lit}}}\"");
    }
    snap(
        "e0007_interp_depth",
        &format!("fn f() {{ let s = {lit}\n}}\n"),
        codes::INTERP_TOO_DEEP,
    );
}

#[test]
fn e0208_assignment_in_expression() {
    snap(
        "e0208_assign_in_expr",
        "fn f() { let x = (y = 2)\n}\n",
        codes::ASSIGN_IN_EXPR,
    );
}

// ------------------------------------------------- s10 additions ---------

#[test]
fn e0209_negative_index() {
    // The D25 hint: `s[-1]` → "use `s[^1]`", machine-applicable edit.
    let src = "fn f() { let x = s[-1]\n}\n";
    let parse = util::parse(src);
    let d = parse
        .diagnostics
        .iter()
        .find(|d| d.code == codes::NEGATIVE_INDEX)
        .expect("E0209 for s[-1]");
    let sugg = &d.suggestions[0];
    assert_eq!(
        sugg.applicability,
        wolf_diag::Applicability::MachineApplicable
    );
    assert_eq!(sugg.edits.len(), 1);
    assert_eq!(sugg.edits[0].1, "^");
    snap("e0209_negative_index", src, codes::NEGATIVE_INDEX);
    // Only the whole-argument `-INT` shape fires: arithmetic does not.
    assert!(
        !util::codes("fn f() { let x = s[i - 1]\n}\n").contains(&"E0209"),
        "`s[i - 1]` is arithmetic, not negative indexing"
    );
}

// ------------------------------------------------- s17 additions ---------

#[test]
fn e0210_moded_receiver_outside_receiver_position() {
    // `(mut x)` is a receiver spelling (X1): legal only immediately
    // before `.`; detached it marks nothing.
    let src = "fn f() { let x = (mut y)\n}\n";
    snap("e0210_moded_receiver", src, codes::RECEIVER_MODE);
    // The receiver position itself parses clean…
    assert!(
        !util::codes("fn f() { let x = (mut p).norm()\n}\n").contains(&"E0210"),
        "`(mut p).norm()` is the legal receiver form"
    );
    // …and so does `take`.
    assert!(
        !util::codes("fn f() { let x = (take p).close()\n}\n").contains(&"E0210"),
        "`(take p).close()` is the legal receiver form"
    );
    // An argument-position moded paren is not receiver position.
    assert!(
        util::codes("fn f() { g((mut y))\n}\n").contains(&"E0210"),
        "a moded paren inside an argument list is not a receiver"
    );
}

#[test]
fn e0203_keyword_typo_suggests_fn() {
    // The typo machinery: `fnn` at declaration position gets "did you
    // mean `fn`?" with a machine-applicable edit.
    let src = "fnn broken() { 1 }\n";
    let parse = util::parse(src);
    let d = parse
        .diagnostics
        .iter()
        .find(|d| d.code == codes::UNEXPECTED_TOPLEVEL)
        .expect("E0203 for fnn");
    assert!(d.message.contains("did you mean `fn`?"), "{}", d.message);
    assert_eq!(
        d.suggestions[0].applicability,
        wolf_diag::Applicability::MachineApplicable
    );
    assert_eq!(d.suggestions[0].edits[0].1, "fn");
    snap("e0203_keyword_typo", src, codes::UNEXPECTED_TOPLEVEL);
}

/// `[gram.expr.if]` (s151, wolf-lang#307): a condition followed by
/// neither `{` nor `then` is one E0201 whose note names both spellings;
/// mixed forms in one `if` and a `let` in a bare branch are refused by
/// name, with the same note.
#[test]
fn e0201_if_forms() {
    snap(
        "e0201_if_missing_then",
        "fn f(c: bool) -> int { if c 29 else 28 }\n",
        codes::EXPECTED_TOKEN,
    );
    snap(
        "e0201_if_mixed_forms",
        "fn f(c: bool) -> int { if c then 29 else { 28 } }\n",
        codes::EXPECTED_TOKEN,
    );
    snap(
        "e0201_if_let_in_bare_branch",
        "fn f(c: bool) -> int {\n    if c then let x = 1 else 0\n    0\n}\n",
        codes::EXPECTED_TOKEN,
    );
}

/// wolf-lang#360 — a type declaration with no `=` is ONE wreck, and
/// the parser says one thing about it.
///
/// Pressing on past the missing `=` reported the SAME token three
/// times: `expected `=` in the type declaration` here, `expected a
/// type` from the alias body below it, and `expected a declaration
/// here` when the top level met the token still sitting there. The
/// blast-radius property caught it at `MUTATE_BUDGET=1000` as 4
/// cascade against the tight bound of 3 on
/// `comptime/assert_static.lu [swap tokens 6/7 at 316..321]`, where
/// swapping a `,` with the `type` keyword strands a bare `type`
/// inside a parameter list.
///
/// The whole code list is pinned, not a count of one code: three
/// reports on one token is precisely what a per-code count hides.
#[test]
fn a_type_declaration_without_eq_reports_once() {
    let all = |src: &str| {
        util::parse(src)
            .diagnostics
            .iter()
            .map(|d| format!("{}", d.code))
            .collect::<Vec<_>>()
    };
    // The reduced shape of the mutation: a named alias, no `=`.
    assert_eq!(all("type n: int\n"), [format!("{}", codes::EXPECTED_TOKEN)]);
    // And with nothing after the name at all.
    assert_eq!(all("type n\n"), [format!("{}", codes::EXPECTED_TOKEN)]);
    // A well-formed alias is still a well-formed alias.
    assert!(
        all("type N = int\n").is_empty(),
        "{:?}",
        all("type N = int\n")
    );
    // The declaration AFTER the wreck still parses: recovery consumes
    // the line it could not read, not the file.
    assert_eq!(
        all("type n: int\nfn g() -> int { 1 }\n"),
        [format!("{}", codes::EXPECTED_TOKEN)]
    );
}

/// The contextual `error` item starts a declaration, so recovery must
/// stop at one — wolf-lang#360's neighbours.
///
/// `error` (s158, `[gram.item.error]`) lexes as an `Ident` and is
/// reclassified only in item position. Every `TokenKind::Kw`-only
/// test of "does a declaration start here?" was blind to it, so the
/// D22 escapes built on those tests walked straight through the next
/// declaration and ate it. That is the same shape as wolf-lang#356 in
/// the semantic-token walk: a contextual keyword added later, and an
/// older "is this a keyword?" test nobody taught about it.
///
/// Both witnesses are the blast-radius property's
/// untouched-declarations invariant, which was red at
/// `MUTATE_BUDGET=300` on trunk v0.2.13.
#[test]
fn recovery_stops_at_a_contextual_error_declaration() {
    let decls = |src: &str| {
        fn walk(n: &wolf_ast::GreenNode, out: &mut Vec<String>) {
            if n.kind == wolf_ast::SyntaxKind::ErrorDecl {
                out.push(format!("ErrorDecl {}..{}", n.span.lo, n.span.hi));
            }
            if n.kind == wolf_ast::SyntaxKind::FnDecl {
                out.push(format!("FnDecl {}..{}", n.span.lo, n.span.hi));
            }
            for c in n.nodes() {
                walk(c, out);
            }
        }
        let parse = util::parse(src);
        let mut out = Vec::new();
        walk(&parse.root, &mut out);
        out.len()
    };
    // Healthy: two aliases and a function, three declarations.
    let good = "error A = {B, io}\n\nerror B = {A, parse}\n\nfn f() -> int { 1 }\n";
    assert_eq!(decls(good), 3, "the healthy file");
    // The `}` of the first alias deleted: the row is unclosed, but the
    // SECOND alias and the function are untouched and still parse.
    // Recovery used to swallow them as more row entries.
    let deleted = "error A = {B, io\n\nerror B = {A, parse}\n\nfn f() -> int { 1 }\n";
    assert_eq!(decls(deleted), 3, "a deleted `}} ` eats no neighbour");
    // The `}` replaced by `=>`, which suppresses the inserted
    // terminator — so the skip found no stop and ran to end of file.
    let arrowed = "error A = {B, io =>\n\nerror B = {A, parse}\n\nfn f() -> int { 1 }\n";
    assert_eq!(decls(arrowed), 3, "a `=>` eats no neighbour either");
}

/// wolf-lang#157 (ch04) — a function VALUE with a return-type
/// annotation is told the rule, not where the parse stopped.
///
/// `fn(c: int) -> int { … }` used to report `expected the closure
/// body` with the caret on the `->`: true of the parse, useless to
/// the reader, and it named neither the rule nor a spelling that
/// works. One report now, spanning the whole stray annotation.
#[test]
fn a_function_value_takes_no_return_type() {
    let src = "fn main() -> !int {\n    let f = fn(c: int) -> int { c + 1 }\n    0\n}\n";
    let parse = util::parse(src);
    let ds = &parse.diagnostics;
    assert_eq!(ds.len(), 1, "exactly one report: {ds:?}");
    assert_eq!(ds[0].code, codes::EXPECTED_TOKEN);
    assert_eq!(ds[0].message, "a function value takes no return type");
    // The caret covers `-> int`, not just the arrow.
    let span = ds[0].primary.span;
    assert_eq!(&src[span.lo as usize..span.hi as usize], "-> int");
    // The note names spellings that actually work.
    let notes = ds[0].notes.join(" ");
    assert!(notes.contains("fn(c: int) { c + 1 }"), "{notes}");
    assert!(notes.contains("fn(c) c + 1"), "{notes}");
    // And all three of those spellings parse clean.
    for body in ["fn(c: int) { c + 1 }", "fn(c) c + 1", "fn(c: int) c + 1"] {
        let ok = format!("fn main() -> !int {{\n    let f = {body}\n    0\n}}\n");
        assert!(
            util::parse(&ok).diagnostics.is_empty(),
            "{body}: {:?}",
            util::parse(&ok).diagnostics
        );
    }
}
