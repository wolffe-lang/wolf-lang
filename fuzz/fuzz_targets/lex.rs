//! s07 lexer fuzz target. Lexing is TOTAL: arbitrary bytes in, token
//! stream + diagnostics out. Asserted invariants (beyond "no panic"):
//! lossless round-trip, string/interp protocol balance, and the `Eof`
//! completion marker (mode stack empty at end of input). Seeds live in
//! fuzz/corpus/lex/.

#![no_main]

use libfuzzer_sys::fuzz_target;
use wolf_lex::TokenKind;

fuzz_target!(|data: &[u8]| {
    // Spans are u32 byte offsets; the documented precondition.
    if data.len() > u32::MAX as usize {
        return;
    }
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(std::path::Path::new("fuzz.lu"));
    let lexed = wolf_lex::lex(file, data);

    // Lossless: leading trivia + token + trailing trivia, concatenated in
    // stream order, reproduce the input byte-for-byte.
    assert_eq!(lexed.reassemble(data), data, "lossless round-trip violated");

    // Completion marker: the mode stack fully drained.
    assert_eq!(
        lexed.tokens.last().map(|t| t.kind),
        Some(TokenKind::Eof),
        "missing Eof completion marker"
    );

    // Protocol balance even on malformed input: every StrBegin has its
    // StrEnd, every InterpOpen its InterpClose.
    let count =
        |pred: fn(&TokenKind) -> bool| lexed.tokens.iter().filter(|t| pred(&t.kind)).count();
    assert_eq!(
        count(|k| matches!(k, TokenKind::StrBegin(_))),
        count(|k| matches!(k, TokenKind::StrEnd { .. })),
        "unbalanced string episode"
    );
    assert_eq!(
        count(|k| matches!(k, TokenKind::InterpOpen)),
        count(|k| matches!(k, TokenKind::InterpClose)),
        "unbalanced interpolation"
    );
});
