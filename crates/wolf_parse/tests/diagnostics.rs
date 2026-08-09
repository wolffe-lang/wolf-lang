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
fn e0005_else_on_new_line() {
    snap(
        "e0005_else_new_line",
        "fn f() { let x = if c { 1 }\n    else { 2 }\n}\n",
        codes::ELSE_ON_NEW_LINE,
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
