//! The parser (s08–s09).
//!
//! Contract: resilient recursive descent implementing spec/01-grammar.md —
//! error nodes, panic-mode sync on declaration keywords at brace depth 0
//! (the D22 architecture bet), expression orientation, `[]` generics via a
//! single deferred `BracketApply` node. Recovery is fuzz-tested: a
//! single-token mutation may not blast more than its enclosing declaration.
//!
//! # s08 scope
//!
//! The full *declaration* grammar (spec §2, types per §4, patterns per
//! §5) over an event-driven skeleton (start/token/finish events, rollback
//! checkpoints; the builder assembles the `wolf_ast` green tree). Bodies
//! are fenced: `{…}` after a header becomes a `BlockPending` node of raw
//! tokens, `= …` initializers become `ExprPending` — complete and
//! lossless, opened by s09. Expression-shaped `[…]` type arguments
//! (const generics) park as `TypeArgPending` for sema (D29).
//!
//! The output tree is **complete and lossless for any byte sequence**:
//! every lexer token lands in the tree exactly once, skipped tokens live
//! in ordinary `ErrorNode`s, and required-but-absent tokens are marked
//! with zero-width `Missing` tokens. `wolf_ast::verify` checks all of it
//! (debug builds re-check on every parse).
//!
//! # Diagnostics (E02xx — the parser's family, plus E0008)
//!
//! | code  | meaning                                                     |
//! |-------|-------------------------------------------------------------|
//! | E0008 | reserved keyword used as an identifier (spec §9)            |
//! | E0201 | expected token/construct (the generic `expect` miss)        |
//! | E0202 | unclosed delimiter (reported at the opener)                 |
//! | E0203 | unexpected tokens where a declaration was expected          |
//! | E0204 | malformed attribute                                         |
//! | E0205 | malformed generic parameter list                            |
//! | E0206 | expected a type                                             |
//! | E0207 | expected a pattern                                          |

use wolf_ast::GreenNode;
use wolf_diag::Diagnostic;
use wolf_lex::Lexed;
use wolf_span::FileId;

mod builder;
mod grammar;
mod parser;

/// Diagnostic codes the parser can emit. Stable: they participate in
/// the differential protocol (spec/06) and the s10 catalog. `E0008` is
/// fixed by spec/01 §9; `E02xx` is the parser's family.
pub mod codes {
    pub const KEYWORD_AS_IDENT: &str = "E0008";
    pub const EXPECTED_TOKEN: &str = "E0201";
    pub const UNCLOSED_DELIMITER: &str = "E0202";
    pub const UNEXPECTED_TOPLEVEL: &str = "E0203";
    pub const MALFORMED_ATTRIBUTE: &str = "E0204";
    pub const MALFORMED_GENERICS: &str = "E0205";
    pub const EXPECTED_TYPE: &str = "E0206";
    pub const EXPECTED_PATTERN: &str = "E0207";
}

/// The result of parsing: a complete lossless tree and the parse-tier
/// diagnostics (lexer diagnostics are separate — [`parse_file`] merges
/// both).
#[derive(Debug)]
pub struct Parse {
    pub root: GreenNode,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parse an already-lexed token stream. `src` must be the bytes `lexed`
/// came from (token text and contextual keywords are read through it).
/// Diagnostics are the parser's own only.
pub fn parse_tokens(lexed: &Lexed, src: &[u8]) -> Parse {
    let mut p = parser::Parser::new(&lexed.tokens, src);
    grammar::source_file(&mut p);
    let file = lexed.tokens[0].span.file;
    let parser::Parser { events, diags, .. } = p;
    let root = builder::build(events, &lexed.tokens, file);
    #[cfg(debug_assertions)]
    if let Err(e) = wolf_ast::verify(&root, src) {
        panic!("tree verifier failed: {e}");
    }
    Parse {
        root,
        diagnostics: diags,
    }
}

/// Lex + parse in one step; diagnostics from both tiers, merged in
/// source order (stable for equal positions: lexer first).
pub fn parse_file(file: FileId, src: &[u8]) -> Parse {
    let lexed = wolf_lex::lex(file, src);
    let mut parse = parse_tokens(&lexed, src);
    let mut all = lexed.diagnostics;
    all.append(&mut parse.diagnostics);
    all.sort_by_key(|d| (d.span.lo, d.span.hi));
    parse.diagnostics = all;
    parse
}
