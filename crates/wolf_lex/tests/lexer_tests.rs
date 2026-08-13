//! Targeted unit tests for the s07 contract: f-string modes, dedent,
//! termination, literal-boundary ambiguities, trivia conventions.

mod util;

use util::{codes, kinds, lex, term_count};
use wolf_lex::{Keyword, Punct, StrKind, TokenKind};

use TokenKind::*;

fn kw(k: Keyword) -> TokenKind {
    TokenKind::Kw(k)
}
fn p(pn: Punct) -> TokenKind {
    TokenKind::Punct(pn)
}

// ------------------------------------------------------------- f-strings

#[test]
fn interp_nested_braces_depth_three() {
    // Braces inside the interpolation are tracked, not parsed; a `{…}`
    // block nests to depth 3 without terminating the interpolation.
    let ks = kinds(r#""{ if x { { { 1 } } } else { 2 } }""#);
    assert_eq!(
        codes(r#""{ if x { { { 1 } } } else { 2 } }""#),
        Vec::<&str>::new()
    );
    assert_eq!(ks[0], StrBegin(StrKind::Plain));
    assert_eq!(ks[1], InterpOpen);
    assert_eq!(ks[ks.len() - 2], InterpClose);
    assert_eq!(ks[ks.len() - 1], StrEnd { dedent: 0 });
    assert_eq!(ks.iter().filter(|k| **k == p(Punct::LBrace)).count(), 4);
    assert_eq!(ks.iter().filter(|k| **k == p(Punct::RBrace)).count(), 4);
}

#[test]
fn interp_nested_string() {
    // `"{m.get("k")}"` — a string inside the interpolation nests via the
    // mode stack, no escapes needed.
    let ks = kinds(r#""{m.get("k")}""#);
    assert_eq!(
        ks,
        vec![
            StrBegin(StrKind::Plain),
            InterpOpen,
            Ident,
            p(Punct::Dot),
            Ident,
            p(Punct::LParen),
            StrBegin(StrKind::Plain),
            StrFragment,
            StrEnd { dedent: 0 },
            p(Punct::RParen),
            InterpClose,
            StrEnd { dedent: 0 },
        ]
    );
}

#[test]
fn format_spec_with_nested_interpolation() {
    // PEP 498 nested specs: `"{x:>{w}.2}"`.
    let ks = kinds(r#""{x:>{w}.2}""#);
    assert_eq!(
        ks,
        vec![
            StrBegin(StrKind::Plain),
            InterpOpen,
            Ident,
            FormatSpecBegin,
            StrFragment, // ">"
            InterpOpen,
            Ident, // w
            InterpClose,
            StrFragment, // ".2"
            InterpClose,
            StrEnd { dedent: 0 },
        ]
    );
}

#[test]
fn format_spec_colon_only_at_depth_zero() {
    // `:` inside nested `[…]` is not top-level ([gram.amb.fmtcolon] uses
    // nested delimiters; the depth-0 one after `]` starts the spec).
    let ks = kinds(r#""{m["k"]:>8}""#);
    assert!(ks.contains(&FormatSpecBegin));
    let colon_pos = ks.iter().position(|k| *k == FormatSpecBegin).unwrap();
    assert_eq!(ks[colon_pos - 1], p(Punct::RBracket));
}

#[test]
fn brace_escapes_are_literal() {
    let ks = kinds(r#""{{not interpolated}}""#);
    assert_eq!(
        ks,
        vec![StrBegin(StrKind::Plain), StrFragment, StrEnd { dedent: 0 }]
    );
}

#[test]
fn valid_escapes_stay_in_fragments() {
    let src = r#""a\n\t\r\\\"\0\x7f\u{1F43A}b""#;
    assert_eq!(codes(src), Vec::<&str>::new());
    assert_eq!(
        kinds(src),
        vec![StrBegin(StrKind::Plain), StrFragment, StrEnd { dedent: 0 }]
    );
}

#[test]
fn invalid_escape_is_error_token_and_lexing_continues() {
    let lexed = lex(r#""a\qb""#);
    let ks: Vec<_> = lexed.tokens.iter().map(|t| t.kind).collect();
    assert_eq!(
        ks,
        vec![
            StrBegin(StrKind::Plain),
            StrFragment, // "a"
            Error,       // \q
            StrFragment, // "b"
            StrEnd { dedent: 0 },
            Term, // EOF terminator after a literal
            Eof,
        ]
    );
    assert_eq!(lexed.diagnostics.len(), 1);
    assert_eq!(lexed.diagnostics[0].code, "E0101");
    // exact span: the two bytes `\q`
    assert_eq!(
        (
            lexed.diagnostics[0].span().lo,
            lexed.diagnostics[0].span().hi
        ),
        (2, 4)
    );
}

#[test]
fn unterminated_plain_string_recovers_at_eol() {
    let src = "let s = \"abc\nlet t = 1\n";
    let lexed = lex(src);
    assert_eq!(codes(src), vec!["E0102"]);
    let ks: Vec<_> = lexed.tokens.iter().map(|t| t.kind).collect();
    // zero-width StrEnd closes the episode; the newline then inserts a
    // Term (StrEnd is a literal end); line two lexes normally.
    let close = ks.iter().position(|k| matches!(k, StrEnd { .. })).unwrap();
    assert!(lexed.tokens[close].span.is_empty());
    assert_eq!(ks[close + 1], Term);
    assert!(ks[close + 2..].starts_with(&[kw(Keyword::Let), Ident, p(Punct::Eq), Int]));
}

#[test]
fn interp_reaching_eof_is_closed_and_balanced() {
    let lexed = lex("\"{x");
    assert!(lexed.diagnostics.iter().any(|d| d.code == "E0102"));
    let ks: Vec<_> = lexed.tokens.iter().map(|t| t.kind).collect();
    assert_eq!(
        ks,
        vec![
            StrBegin(StrKind::Plain),
            InterpOpen,
            Ident,
            InterpClose, // zero-width recovery
            StrEnd { dedent: 0 },
            Term, // EOF terminator: StrEnd is a literal end
            Eof
        ]
    );
}

#[test]
fn nesting_deeper_than_max_is_an_error_token() {
    let src = "\"{".repeat(wolf_lex::MAX_NEST);
    let lexed = lex(&src);
    assert!(lexed.diagnostics.iter().any(|d| d.code == "E0108"));
}

// ------------------------------------------------------------- multiline

#[test]
fn multiline_dedent_by_closing_column() {
    let src = "let p = \"\"\"\n    a\n    b\n    \"\"\"\n";
    let lexed = lex(src);
    assert_eq!(lexed.diagnostics, vec![]);
    assert!(lexed.tokens.iter().any(|t| t.kind == StrEnd { dedent: 4 }));
}

#[test]
fn multiline_blank_lines_are_exempt() {
    let src = "let p = \"\"\"\n    a\n\n   \n    b\n    \"\"\"\n";
    assert_eq!(codes(src), Vec::<&str>::new());
}

#[test]
fn multiline_under_indent_diagnostic() {
    let src = "let p = \"\"\"\n  bad\n    \"\"\"\n";
    let lexed = lex(src);
    assert_eq!(lexed.diagnostics.len(), 1);
    let d = &lexed.diagnostics[0];
    assert_eq!(d.code, "E0104");
    // points at the offending line AND notes the margin
    assert_eq!(d.span().lo, 12);
    assert_eq!(d.notes.len(), 1);
}

#[test]
fn multiline_tab_space_margin_mismatch() {
    let src = "let p = \"\"\"\n\t\t\t\tbad\n    \"\"\"\n";
    let lexed = lex(src);
    assert_eq!(lexed.diagnostics.len(), 1);
    assert_eq!(lexed.diagnostics[0].code, "E0105");
    assert_eq!(lexed.diagnostics[0].secondary.len(), 1);
}

#[test]
fn multiline_tokens_after_opening_quotes() {
    let src = "let p = \"\"\"oops\n    a\n    \"\"\"\n";
    assert_eq!(codes(src), vec!["E0103"]);
}

#[test]
fn multiline_with_interpolation() {
    let src = "let p = \"\"\"\n    a {x:>8} b\n    \"\"\"\n";
    let lexed = lex(src);
    assert_eq!(lexed.diagnostics, vec![]);
    let ks: Vec<_> = lexed.tokens.iter().map(|t| t.kind).collect();
    assert!(ks.contains(&InterpOpen));
    assert!(ks.contains(&FormatSpecBegin));
    assert!(ks.contains(&StrEnd { dedent: 4 }));
}

#[test]
fn multiline_unterminated_recovers_at_eof() {
    let src = "let p = \"\"\"\n    a\n";
    let lexed = lex(src);
    assert!(lexed.diagnostics.iter().any(|d| d.code == "E0102"));
}

// ------------------------------------------------------- raw/generalized

#[test]
fn raw_string_with_fences() {
    let ks = kinds(r###"let x = r#"say "hi""#"###);
    assert!(ks.contains(&StrBegin(StrKind::Raw { hashes: 1 })));
    // no escapes, no interpolation: one fragment
    assert_eq!(ks.iter().filter(|k| **k == StrFragment).count(), 1);
    assert!(!ks.contains(&InterpOpen));
}

#[test]
fn raw_string_backslashes_and_braces_are_content() {
    let ks = kinds(r#"let x = r"a\d{2}+""#);
    assert_eq!(codes(r#"let x = r"a\d{2}+""#), Vec::<&str>::new());
    assert!(!ks.contains(&InterpOpen));
    assert!(!ks.contains(&Error));
}

#[test]
fn generalized_vs_two_tokens() {
    // `re"x"` — ONE literal episode with a Generalized begin whose span
    // covers the prefix + quote.
    let one = lex(r#"re"[a-z]+""#);
    assert_eq!(one.tokens[0].kind, StrBegin(StrKind::Generalized));
    assert_eq!((one.tokens[0].span.lo, one.tokens[0].span.hi), (0, 3));
    // `re "x"` — ident, then a plain string.
    let two = kinds(r#"re "[a-z]+""#);
    assert_eq!(two[0], Ident);
    assert_eq!(two[1], StrBegin(StrKind::Plain));
}

#[test]
fn keyword_prefix_is_never_generalized() {
    // `if"x"` = keyword + plain string (the prefix must be a non-keyword
    // identifier, [gram.lex.str.gen]).
    let ks = kinds(r#"if"x""#);
    assert_eq!(ks[0], kw(Keyword::If));
    assert_eq!(ks[1], StrBegin(StrKind::Plain));
}

#[test]
fn generalized_unterminated_recovers_at_eol() {
    let src = "let x = re\"abc\nlet y = 1\n";
    assert_eq!(codes(src), vec!["E0109"]);
}

// --------------------------------------------------------------- numbers

#[test]
fn number_boundaries() {
    // 1.s member · 1.0 float · 1..2 range · 1.0e5 float · 1.e5 member
    assert_eq!(kinds("1.s"), vec![Int, p(Punct::Dot), Ident]);
    assert_eq!(kinds("1.0"), vec![Float]);
    assert_eq!(kinds("1..2"), vec![Int, p(Punct::DotDot), Int]);
    assert_eq!(kinds("1.0e5"), vec![Float]);
    assert_eq!(kinds("1.e5"), vec![Int, p(Punct::Dot), Ident]);
    assert_eq!(kinds("1e5"), vec![Float]);
    assert_eq!(kinds("1.0E-5"), vec![Float]);
    assert_eq!(kinds("2_147_483_647"), vec![Int]);
    assert_eq!(kinds("0x9e37_79b9"), vec![Int]);
    assert_eq!(kinds("0o777"), vec![Int]);
    assert_eq!(kinds("0b10_10"), vec![Int]);
    // radix prefix without a digit is `0` + word, not an error
    assert_eq!(kinds("0x"), vec![Int, Ident]);
    assert_eq!(kinds("0b2"), vec![Int, Ident]);
}

// ----------------------------------------------------------- termination

#[test]
fn term_inserted_after_each_terminator_class() {
    // [gram.lex.newline]: ident, `_`, literals, return/break/continue,
    // `)`, `]`, `}`, postfix `?`.
    for src in [
        "x\n",
        "_\n",
        "1\n",
        "1.0\n",
        "\"s\"\n",
        "r\"s\"\n",
        "re\"s\"\n",
        "true\n",
        "false\n",
        "return\n",
        "break\n",
        "continue\n",
        "f()\n",
        "a[1]\n",
        "{ x }\n",
        "x?\n",
    ] {
        assert_eq!(term_count(src), 1, "expected one Term in {src:?}");
    }
}

#[test]
fn term_not_inserted_after_non_terminators() {
    for src in [
        "x +\n", "x,\n", "let\n", "x.\n", "x =\n", "(\n", "[\n", "{\n", "x &&\n", "if\n", "x <\n",
    ] {
        assert_eq!(term_count(src), 0, "expected no Term in {src:?}");
    }
}

#[test]
fn term_suppressed_inside_paren_and_bracket_and_interp() {
    assert_eq!(term_count("f(a\n)\n"), 1); // only after `)`
    assert_eq!(term_count("a[1\n]\n"), 1); // only after `]`
    assert_eq!(term_count("\"{a\n}\"\n"), 1); // only after StrEnd
}

#[test]
fn term_reenabled_inside_braces_within_parens() {
    // A `{…}` block re-enables insertion whatever it is nested in.
    assert_eq!(term_count("f(fn() { a\n })\n"), 2); // after `a` and after `)`
}

#[test]
fn no_term_after_attribute_close() {
    let src = "#[cfg(x)]\nfn f()\n";
    let ks = kinds(src);
    let close = ks.iter().position(|k| *k == p(Punct::RBracket)).unwrap();
    assert_eq!(
        ks[close + 1],
        kw(Keyword::Fn),
        "no Term after attribute `]`"
    );
    // ...but a non-attribute `]` still terminates (asserted above).
}

#[test]
fn explicit_semicolon_is_term_with_its_span() {
    let lexed = lex("f(); g()\n");
    let semi = lexed
        .tokens
        .iter()
        .find(|t| t.kind == Term && !t.span.is_empty() && t.span.lo == 3)
        .expect("explicit `;` Term");
    assert_eq!((semi.span.lo, semi.span.hi), (3, 4));
    // ...and no second Term for the `;` line beyond the newline one.
    assert_eq!(term_count("f(); g()\n"), 2);
}

#[test]
fn eof_without_newline_still_terminates() {
    let lexed = lex("let x = 1");
    let ks: Vec<_> = lexed.tokens.iter().map(|t| t.kind).collect();
    assert_eq!(ks[ks.len() - 2], Term);
    assert!(lexed.tokens[ks.len() - 2].span.is_empty());
}

#[test]
fn eof_term_suppressed_inside_parens() {
    let ks = kinds("f(x");
    assert!(!ks.contains(&Term));
}

// ---------------------------------------------------------------- trivia

#[test]
fn trailing_trivia_ends_at_first_newline() {
    let lexed = lex("x  // c\n  y\n");
    let x = &lexed.tokens[0];
    assert_eq!(x.kind, Ident);
    // `  ` and `// c` trail x; the newline becomes the Term token; `  `
    // on line two leads y.
    assert_eq!(x.trailing.len(), 2);
    assert_eq!(x.trailing[1].kind, wolf_lex::TriviaKind::LineComment);
    assert_eq!(lexed.tokens[1].kind, Term);
    let y = &lexed.tokens[2];
    assert_eq!(y.leading.len(), 1);
    assert_eq!(y.leading[0].kind, wolf_lex::TriviaKind::Whitespace);
}

#[test]
fn doc_comment_kinds() {
    let lexed = lex("//! inner\n/// outer\n// plain\n//// plain too\nx\n");
    let lead = &lexed.tokens[0].leading;
    let kindset: Vec<_> = lead.iter().map(|t| t.kind).collect();
    use wolf_lex::TriviaKind::*;
    assert_eq!(
        kindset,
        vec![
            InnerDocComment,
            Whitespace,
            DocComment,
            Whitespace,
            LineComment,
            Whitespace,
            LineComment,
            Whitespace
        ]
    );
}

#[test]
fn shebang_is_trivia_at_byte_zero_only() {
    // s53: `wolf run script.lu` lexes an executable file unchanged.
    let src = "#!/usr/bin/env -S wolf run\n//! doc\nx\n";
    let lexed = lex(src);
    assert!(!lexed.has_errors(), "{:?}", lexed.diagnostics);
    let lead = &lexed.tokens[0].leading;
    use wolf_lex::TriviaKind::*;
    assert_eq!(lead[0].kind, Shebang);
    assert_eq!(
        &src[lead[0].span.lo as usize..lead[0].span.hi as usize],
        "#!/usr/bin/env -S wolf run"
    );
    assert!(
        lead.iter().any(|t| t.kind == InnerDocComment),
        "the module header still lexes after the shebang"
    );
    // The lossless invariant holds with the new trivia in it.
    assert_eq!(lexed.reassemble(src.as_bytes()), src.as_bytes());
    // One offset only: a `#!` on line 2 is still the stray-byte error.
    let later = lex("x\n#!/bin/sh\n");
    assert!(later.has_errors(), "a mid-file `#!` must not be trivia");
    assert!(
        later.diagnostics.iter().any(|d| d.code.as_str() == "E0107"),
        "{:?}",
        later.diagnostics
    );
}

#[test]
fn eof_token_owns_dangling_trivia() {
    let lexed = lex("x\n// tail comment\n");
    let eof = lexed.tokens.last().unwrap();
    assert_eq!(eof.kind, Eof);
    assert!(!eof.leading.is_empty());
}

// ------------------------------------------------------------- total-ness

#[test]
fn stray_and_invalid_utf8_produce_error_tokens() {
    assert_eq!(codes("let $ = 1\n"), vec!["E0107"]);
    let lexed = util::lex_bytes(b"let \xff\xfe x = 1\n");
    assert_eq!(
        lexed.diagnostics.len(),
        1,
        "one report per run of bad bytes"
    );
    assert_eq!(lexed.diagnostics[0].code, "E0106");
}

#[test]
fn bom_is_rejected() {
    let lexed = util::lex_bytes("\u{feff}let x = 1\n".as_bytes());
    assert_eq!(lexed.diagnostics[0].code, "E0107");
    assert!(lexed.diagnostics[0].message.contains("byte order mark"));
}

#[test]
fn mismatched_closers_do_not_confuse_the_stack() {
    // Delimiter matching is the parser's job; the lexer must not let a
    // stray `)` break interpolation tracking.
    let src = "\"{ ) }\"\n";
    let ks = kinds(src);
    assert!(ks.contains(&InterpClose));
    assert!(ks.contains(&StrEnd { dedent: 0 }));
}
