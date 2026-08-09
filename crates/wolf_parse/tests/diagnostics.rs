//! One test per parser diagnostic code, each pinning the *structured*
//! diagnostic (code, severity, spans, message, notes) in an insta
//! snapshot — the s10 catalog's seed.

mod util;

use wolf_parse::codes;

/// Parse, pick the first diagnostic with `code`, snapshot its Debug
/// form under `name`.
fn snap(name: &str, src: &str, code: &'static str) {
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
    snap(
        "e0201_missing_name",
        "fn (x: int) { }\n",
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
