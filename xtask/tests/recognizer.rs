//! THROWAWAY (s03 Target 5): delete when s08–s09 parser lands.
//!
//! Grammar snapshot: the production trace of `corpus/wordcount.lu` —
//! grammar changes show up as reviewable diffs (s03 acceptance). Plus
//! unit tests for the trickiest lexer rules: interpolation nesting,
//! multiline dedent, newline-termination token classes, and the
//! int-dot family (`1.s` vs `1.0` vs `1..2`).

use std::collections::BTreeSet;

use xtask::recognize::wolf_grammar;
use xtask::speclex::{Tok, lex, reserved_keywords};

fn keywords() -> BTreeSet<String> {
    let ebnf =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../spec/grammar.ebnf"))
            .expect("read spec/grammar.ebnf");
    reserved_keywords(&ebnf).expect("reserved_kw production")
}

fn toks(src: &str) -> Vec<Tok> {
    lex(src, &keywords()).expect("lexes")
}

/// Render a token stream compactly for assertions.
fn spell(toks: &[Tok]) -> String {
    toks.iter()
        .map(|t| match t {
            Tok::Ident(s) => s.clone(),
            Tok::Kw(s) => format!("kw:{s}"),
            Tok::Punct(p) => (*p).to_string(),
            Tok::Int => "INT".into(),
            Tok::Float => "FLOAT".into(),
            Tok::Str => "STR".into(),
            Tok::RawStr => "RAW".into(),
            Tok::GenStr => "GEN".into(),
            Tok::StrStart => "S<".into(),
            Tok::StrMid => "S|".into(),
            Tok::StrEnd => "S>".into(),
            Tok::Underscore => "_".into(),
            Tok::Term => ";".into(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn keyword_inventory_is_the_closed_set_of_50() {
    assert_eq!(keywords().len(), 50, "[gram.inv.kw] checksum");
}

#[test]
fn wordcount_production_trace_snapshot() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../corpus/wordcount.lu"
    ))
    .expect("read corpus/wordcount.lu");
    let analysis = wolf_grammar()
        .analyze(&toks(&src))
        .expect("wordcount.lu is accepted");
    assert_eq!(analysis.parses, 1, "wordcount.lu must parse uniquely");
    insta::assert_snapshot!(analysis.trace.expect("unique parse has a trace"));
}

// ------------------------------------------------------ lexer: strings --

#[test]
fn interpolation_nesting_reenters_normal_mode() {
    // one nesting level of f-string inside an interpolation, no escapes
    let t = toks("let x = \"{ \"wrap-{inner}-wrap\" }\"\n");
    assert_eq!(spell(&t), "kw:let x = S< S< inner S> S> ;");
    // nested plain string + call inside interpolation
    let t = toks("print(\"{\"#\".repeat(n)}\")\n");
    assert_eq!(spell(&t), "print ( S< STR . repeat ( n ) S> ) ;");
    // literal braces never open interpolation
    let t = toks("print(\"{{not interpolated}}\")\n");
    assert_eq!(spell(&t), "print ( STR ) ;");
}

#[test]
fn format_spec_starts_at_first_top_level_colon() {
    // `:` after `]` is top-level -> format spec is consumed lexically
    let t = toks("print(\"{m[\"k\"]:>8}\")\n");
    assert_eq!(spell(&t), "print ( S< m [ STR ] S> ) ;");
    // interpolated width inside the spec stays lexical
    let t = toks("print(\"{m[\"k\"]:>{w}}\")\n");
    assert_eq!(spell(&t), "print ( S< m [ STR ] S> ) ;");
    // `:` inside nested `{…}` is NOT top-level [gram.amb.fmtcolon]
    let t = toks("print(\"{ Point { x: 0 }.x }\")\n");
    assert_eq!(spell(&t), "print ( S< Point { x : INT } . x S> ) ;");
}

#[test]
fn interpolation_nesting_depth_limit_is_eight() {
    // depth 8 (E0007): strings nested in interpolations 9 levels deep
    let mut src = String::from("let x = ");
    src.push_str(&"\"{".repeat(9));
    src.push('1');
    src.push_str(&"}\"".repeat(9));
    src.push('\n');
    let err = lex(&src, &keywords()).unwrap_err();
    assert!(err.msg.contains("E0007"), "{}", err.msg);
}

#[test]
fn multiline_dedent_follows_closing_column() {
    let ok = "let p = \"\"\"\n    line one\n    line two\n    \"\"\"\n";
    assert_eq!(spell(&toks(ok)), "kw:let p = STR ;");
    // content line under-indented relative to the closing delimiter
    let bad = "let p = \"\"\"\n  short\n    \"\"\"\n";
    let err = lex(bad, &keywords()).unwrap_err();
    assert!(err.msg.contains("under-indented"), "{}", err.msg);
}

// ------------------------------------------- lexer: newline termination --

#[test]
fn term_inserted_after_each_terminating_class() {
    // identifier, literal, string end, `?`, closing delimiters, jump kws
    assert_eq!(spell(&toks("x\n")), "x ;");
    assert_eq!(spell(&toks("1\n")), "INT ;");
    assert_eq!(spell(&toks("1.5\n")), "FLOAT ;");
    assert_eq!(spell(&toks("\"s\"\n")), "STR ;");
    assert_eq!(spell(&toks("f()\n")), "f ( ) ;");
    assert_eq!(spell(&toks("a[0]\n")), "a [ INT ] ;");
    assert_eq!(spell(&toks("f()?\n")), "f ( ) ? ;");
    assert_eq!(spell(&toks("true\n")), "kw:true ;");
    assert_eq!(spell(&toks("return\n")), "kw:return ;");
    assert_eq!(spell(&toks("_\n")), "_ ;");
}

#[test]
fn no_term_after_operators_dots_commas_or_open_delims() {
    assert_eq!(spell(&toks("a +\nb\n")), "a + b ;");
    assert_eq!(spell(&toks("a.\nb\n")), "a . b ;");
    assert_eq!(spell(&toks("f(a,\nb)\n")), "f ( a , b ) ;");
    assert_eq!(spell(&toks("let x =\n1\n")), "kw:let x = INT ;");
    assert_eq!(spell(&toks("if c {\n}\n")), "kw:if c { } ;");
}

#[test]
fn innermost_delimiter_decides_suppression() {
    // parens suppress...
    assert_eq!(spell(&toks("f(a\n)\n")), "f ( a ) ;");
    // ...but a block inside parens re-enables insertion
    assert_eq!(
        spell(&toks("f(fn() {\ng()\nh()\n})\n")),
        "f ( kw:fn ( ) { g ( ) ; h ( ) ; } ) ;"
    );
}

#[test]
fn attribute_close_bracket_inserts_no_term() {
    assert_eq!(
        spell(&toks("#[noalloc]\nfn f() { }\n")),
        "#[ noalloc ] kw:fn f ( ) { } ;"
    );
    // a plain index `]` still terminates
    assert_eq!(spell(&toks("a[0]\nb\n")), "a [ INT ] ; b ;");
}

#[test]
fn explicit_semicolon_and_empty_statement() {
    assert_eq!(spell(&toks("a(); b()\n")), "a ( ) ; b ( ) ;");
    // stray `;` = empty statement, E0002
    let err = lex("fn f() {\n;\n}\n", &keywords()).unwrap_err();
    assert!(err.msg.contains("E0002"), "{}", err.msg);
    // blank lines never double-terminate
    assert_eq!(spell(&toks("a\n\n\nb\n")), "a ; b ;");
}

// ------------------------------------------------- lexer: int-dot rules --

#[test]
fn int_dot_member_float_and_range() {
    assert_eq!(spell(&toks("1.s\n")), "INT . s ;"); // member on int
    assert_eq!(spell(&toks("1.0\n")), "FLOAT ;");
    assert_eq!(spell(&toks("1..2\n")), "INT .. INT ;"); // never a float
    assert_eq!(spell(&toks("1..=2\n")), "INT ..= INT ;");
    assert_eq!(spell(&toks("1.0e5\n")), "FLOAT ;");
    assert_eq!(spell(&toks("1e5\n")), "FLOAT ;");
    // `1.e5` is member access on `1` (counter-example, E0004 diagnostic)
    assert_eq!(spell(&toks("1.e5\n")), "INT . e5 ;");
    assert_eq!(spell(&toks("4096.kb\n")), "INT . kb ;");
    assert_eq!(spell(&toks("0x9e3779b9\n")), "INT ;");
    assert_eq!(spell(&toks("2_147_483_647\n")), "INT ;");
}

// ------------------------------------------------- recognizer smoke --

#[test]
fn recognizer_accepts_and_rejects() {
    let g = wolf_grammar();
    let ok = g.analyze(&toks("fn main() -> !int {\n    0\n}\n")).unwrap();
    assert_eq!(ok.parses, 1);
    // leading-operator continuation is a broken statement (E0001 shape)
    let t = toks("fn main() -> !int {\n    let a = 1\n        + 2\n    0\n}\n");
    assert!(g.analyze(&t).is_err());
    // comparison chaining does not parse (E0003)
    let t = toks("fn f() { let x = a < b < c\n}\n");
    assert!(g.analyze(&t).is_err());
}
