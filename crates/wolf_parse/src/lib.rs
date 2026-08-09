//! The parser (s08–s09).
//!
//! Contract: resilient recursive descent implementing spec/01-grammar.md —
//! error nodes, panic-mode sync on declaration keywords at brace depth 0
//! (the D22 architecture bet), expression orientation, `[]` generics via a
//! single deferred `BracketApply` node. Recovery is fuzz-tested: a
//! single-token mutation may not blast more than its enclosing declaration.
//!
//! # Scope (s08 + s09)
//!
//! The full surface grammar: declarations (spec §2, types per §4,
//! patterns per §5) and the complete expression/statement grammar
//! (spec §3 — one Pratt climb over §3.2's table, blocks as expressions,
//! `else` defaulting, `BracketApply`, regions, the concurrency and
//! unsafe surface) over an event-driven skeleton (start/token/finish
//! events, rollback checkpoints; the builder assembles the `wolf_ast`
//! green tree). Expression-shaped `[…]` type arguments in *type*
//! position (const generics) park as `TypeArgPending` for sema (D29).
//!
//! The output tree is **complete and lossless for any byte sequence**:
//! every lexer token lands in the tree exactly once, skipped tokens live
//! in ordinary `ErrorNode`s, and required-but-absent tokens are marked
//! with zero-width `Missing` tokens. `wolf_ast::verify` checks all of it
//! (debug builds re-check on every parse).
//!
//! # Diagnostics (E02xx — the parser's family, plus spec §9's E000x)
//!
//! | code  | meaning                                                     |
//! |-------|-------------------------------------------------------------|
//! | E0001 | leading-operator continuation (`[gram.amb.newline]`)        |
//! | E0002 | empty statement — a `;` that terminates nothing             |
//! | E0003 | comparison chaining (`a < b < c`)                           |
//! | E0005 | `else` on a new line (`[gram.amb.else]`)                    |
//! | E0006 | struct literal in condition position (`[gram.amb.structlit]`)|
//! | E0007 | string interpolations nested deeper than 8                  |
//! | E0008 | reserved keyword used as an identifier (spec §9)            |
//! | E0201 | expected token/construct (the generic `expect` miss)        |
//! | E0202 | unclosed delimiter (reported at the opener)                 |
//! | E0203 | unexpected tokens where a declaration was expected          |
//! | E0204 | malformed attribute                                         |
//! | E0205 | malformed generic parameter list                            |
//! | E0206 | expected a type                                             |
//! | E0207 | expected a pattern                                          |
//! | E0208 | assignment used as an expression                            |
//! | E0209 | negative integer literal used as an index (D25 `^` hint)    |

use wolf_ast::GreenNode;
use wolf_diag::Diagnostic;
use wolf_lex::Lexed;
use wolf_span::{FileId, Span};

mod builder;
mod exprs;
mod grammar;
mod parser;

/// Diagnostic codes the parser can emit — semantic aliases for the s10
/// registry entries (`wolf_diag::registry`, the single place a code can
/// be born). Stable: they participate in the differential protocol
/// (spec/06) and the diagnostic catalog. `E000x` codes are fixed by
/// spec/01 §9; `E02xx` is the parser's family.
pub mod codes {
    use wolf_diag::{Code, codes as c};

    pub const LEADING_OPERATOR: Code = c::E0001;
    pub const EMPTY_STATEMENT: Code = c::E0002;
    pub const COMPARISON_CHAIN: Code = c::E0003;
    pub const ELSE_ON_NEW_LINE: Code = c::E0005;
    pub const STRUCT_LIT_IN_COND: Code = c::E0006;
    pub const INTERP_TOO_DEEP: Code = c::E0007;
    pub const KEYWORD_AS_IDENT: Code = c::E0008;
    pub const EXPECTED_TOKEN: Code = c::E0201;
    pub const UNCLOSED_DELIMITER: Code = c::E0202;
    pub const UNEXPECTED_TOPLEVEL: Code = c::E0203;
    pub const MALFORMED_ATTRIBUTE: Code = c::E0204;
    pub const MALFORMED_GENERICS: Code = c::E0205;
    pub const EXPECTED_TYPE: Code = c::E0206;
    pub const EXPECTED_PATTERN: Code = c::E0207;
    pub const ASSIGN_IN_EXPR: Code = c::E0208;
    pub const NEGATIVE_INDEX: Code = c::E0209;
    pub const RECEIVER_MODE: Code = c::E0210;
}

/// The result of parsing: a complete lossless tree and the parse-tier
/// diagnostics (lexer diagnostics are separate — [`parse_file`] merges
/// both).
#[derive(Debug)]
pub struct Parse {
    pub root: GreenNode,
    pub diagnostics: Vec<Diagnostic>,
    /// The regions recovery skipped into error nodes — the cascade-
    /// suppression set (s10). Later phases seed their sink with these
    /// (`wolf_diag::Diagnostics::suppress`) so diagnostics computed
    /// *about* a wrecked region stay quiet; see the `<error>`-unifies-
    /// silently convention in `wolf_diag`'s crate docs.
    pub error_regions: Vec<Span>,
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
    let (diagnostics, error_regions) = diags.into_parts();
    Parse {
        root,
        diagnostics,
        error_regions,
    }
}

/// Lex + parse in one step; diagnostics from both tiers, merged in the
/// engine's deterministic order (file, span, code — stable for full
/// ties: lexer first).
pub fn parse_file(file: FileId, src: &[u8]) -> Parse {
    let lexed = wolf_lex::lex(file, src);
    let mut parse = parse_tokens(&lexed, src);
    let mut all = lexed.diagnostics;
    all.append(&mut parse.diagnostics);
    wolf_diag::sort_diagnostics(&mut all);
    parse.diagnostics = all;
    parse
}
