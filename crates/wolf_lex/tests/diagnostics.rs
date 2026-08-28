//! One reviewed insta snapshot per lexer diagnostic code (E0101–E0109) —
//! the s01 habit, enforcement tightening in s10. Snapshots capture the
//! full structured value: code, severity, span, message, notes.

mod util;

fn diag_snapshot(name: &str, src: &str) {
    let lexed = util::lex(src);
    assert!(
        !lexed.diagnostics.is_empty(),
        "expected a diagnostic for {src:?}"
    );
    insta::assert_snapshot!(name, format!("{:#?}", lexed.diagnostics));
}

#[test]
fn e0101_invalid_escape() {
    diag_snapshot("e0101_unknown", r#"let s = "a\qb""#);
}

#[test]
fn e0101_invalid_hex_escape() {
    diag_snapshot("e0101_hex", r#"let s = "a\x2gb""#);
}

#[test]
fn e0101_invalid_unicode_escape() {
    diag_snapshot("e0101_unicode", r#"let s = "a\u{}b""#);
}

#[test]
fn e0102_unterminated_string() {
    diag_snapshot("e0102_eol", "let s = \"abc\nlet t = 1\n");
}

#[test]
fn e0102_unterminated_multiline_at_eof() {
    diag_snapshot("e0102_multiline_eof", "let p = \"\"\"\n    a\n");
}

#[test]
fn e0103_tokens_after_opening_multiline() {
    diag_snapshot("e0103", "let p = \"\"\"oops\n    a\n    \"\"\"\n");
}

#[test]
fn e0104_under_indented_line() {
    diag_snapshot("e0104", "let p = \"\"\"\n  bad\n    \"\"\"\n");
}

#[test]
fn e0105_margin_tab_space_mismatch() {
    diag_snapshot("e0105", "let p = \"\"\"\n\t\t\t\tbad\n    \"\"\"\n");
}

#[test]
fn e0106_invalid_utf8() {
    let lexed = util::lex_bytes(b"let \xff\xfe x = 1\n");
    insta::assert_snapshot!("e0106", format!("{:#?}", lexed.diagnostics));
}

#[test]
fn e0107_stray_byte() {
    diag_snapshot("e0107", "let $ = 1\n");
}

#[test]
fn e0107_lone_brace_in_string() {
    diag_snapshot("e0107_lone_brace", r#"let s = "a}b""#);
}

#[test]
fn e0108_nesting_too_deep() {
    let src = "\"{".repeat(wolf_lex::MAX_NEST);
    let lexed = util::lex(&src);
    let deep: Vec<_> = lexed
        .diagnostics
        .iter()
        .filter(|d| d.code == "E0108")
        .collect();
    insta::assert_snapshot!("e0108", format!("{deep:#?}"));
}

#[test]
fn e0109_unterminated_generalized() {
    diag_snapshot("e0109_generalized", "let x = re\"abc\n");
}

#[test]
fn e0109_unterminated_raw() {
    diag_snapshot("e0109_raw", "let x = r#\"abc\n");
}

#[test]
fn e0110_empty_char() {
    diag_snapshot("e0110_empty", "let c = ''\n");
}

#[test]
fn e0110_two_scalars() {
    diag_snapshot("e0110_two_scalars", "let c = 'ab'\n");
}

#[test]
fn e0110_combining_grapheme() {
    // One glyph, two scalars — the grapheme note.
    diag_snapshot("e0110_grapheme", "let c = 'e\u{301}'\n");
}

#[test]
fn e0110_unterminated() {
    diag_snapshot("e0110_unterminated", "let c = 'a\nlet d = 1\n");
}

#[test]
fn e0110_surrogate_escape() {
    // The surrogate gap refused at the literal — the same domain the
    // trapping `int as char` cast enforces at run time (D57).
    diag_snapshot("e0110_surrogate", "let c = '\\u{D800}'\n");
}

#[test]
fn e0110_beyond_last_scalar() {
    diag_snapshot("e0110_beyond", "let c = '\\u{110000}'\n");
}
