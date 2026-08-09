//! Shared helpers for wolf_lex's integration tests.

// Each test binary compiles this module and uses a subset of it.
#![allow(dead_code)]

use std::path::Path;
use wolf_lex::{Lexed, TokenKind};
use wolf_span::SourceMap;

pub fn lex_bytes(src: &[u8]) -> Lexed {
    let mut sm = SourceMap::new();
    let file = sm.intern(Path::new("test.lu"));
    let lexed = wolf_lex::lex(file, src);
    // Every test doubles as a lossless + protocol-balance check.
    assert_eq!(lexed.reassemble(src), src, "lossless invariant violated");
    assert_balanced(&lexed);
    lexed
}

pub fn lex(src: &str) -> Lexed {
    lex_bytes(src.as_bytes())
}

/// StrBegin/StrEnd and InterpOpen/InterpClose always pair up, and the
/// stream always ends with the `Eof` completion marker.
pub fn assert_balanced(lexed: &Lexed) {
    let count =
        |pred: fn(&TokenKind) -> bool| lexed.tokens.iter().filter(|t| pred(&t.kind)).count();
    assert_eq!(
        count(|k| matches!(k, TokenKind::StrBegin(_))),
        count(|k| matches!(k, TokenKind::StrEnd { .. })),
        "unbalanced StrBegin/StrEnd"
    );
    assert_eq!(
        count(|k| matches!(k, TokenKind::InterpOpen)),
        count(|k| matches!(k, TokenKind::InterpClose)),
        "unbalanced InterpOpen/InterpClose"
    );
    assert_eq!(
        lexed.tokens.last().map(|t| t.kind),
        Some(TokenKind::Eof),
        "missing Eof completion marker"
    );
}

/// Token kinds without trivia, the trailing Eof, or the zero-width
/// end-of-file terminator (tests interested in the EOF Term inspect
/// `tokens` directly).
pub fn kinds(src: &str) -> Vec<TokenKind> {
    let lexed = lex(src);
    lexed
        .tokens
        .iter()
        .filter(|t| t.kind != TokenKind::Eof && !(t.kind == TokenKind::Term && t.span.is_empty()))
        .map(|t| t.kind)
        .collect()
}

pub fn codes(src: &str) -> Vec<&'static str> {
    lex(src)
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect()
}

pub fn term_count(src: &str) -> usize {
    kinds(src).iter().filter(|k| **k == TokenKind::Term).count()
}
