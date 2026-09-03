//! The lexer (s07).
//!
//! Contract: mode-stack lexing for f-strings (nested braces, format specs),
//! `"""` dedent by closing-delimiter column, `re"…"` generalized literals,
//! byte-exact spans, lossless trivia (the formatter's substrate). Fuzzed
//! from day one (s01 scaffold).
//!
//! # Shape
//!
//! [`lex`] is **total**: arbitrary bytes in, token stream + diagnostics
//! out — malformed input produces [`TokenKind::Error`] tokens and
//! [`wolf_diag::Diagnostic`]s, never a failure and never a panic (the one
//! documented precondition: sources larger than `u32::MAX` bytes are not
//! lexable, because spans are `u32` byte offsets per D25).
//!
//! Tokens are `(kind, span)` — no owned text; text is always derived by
//! slicing the source with the token's [`Span`]. Trivia (whitespace and
//! `//` / `///` / `//!` comments) is attached to tokens rust-analyzer
//! style: a token owns its *leading* trivia (everything since the previous
//! token's line ended) and its *trailing* trivia (same-line pieces up to,
//! not including, the first newline).
//!
//! **Lossless invariant**: concatenating, in stream order, every token's
//! leading trivia + token span + trailing trivia reproduces the input
//! byte-for-byte. The final [`TokenKind::Eof`] token (zero-width, always
//! present, the completion marker) carries any dangling end-of-file
//! trivia as its leading trivia.
//!
//! # String token protocol (what s08 consumes)
//!
//! Every string literal — plain `"…"`, multiline `"""`, raw `r#"…"#`,
//! generalized `re"…"` — is one balanced episode:
//!
//! ```text
//! StrBegin(kind)  ( StrFragment | Error
//!                 | InterpOpen …tokens… [FormatSpecBegin …] InterpClose )*
//! StrEnd { dedent }
//! ```
//!
//! - [`TokenKind::StrBegin`] spans the opening delimiter: `"` or `"""` or
//!   `r##"` (fences included) or `re"` (prefix included — the generalized
//!   prefix's span is `lo..hi-1` of the `StrBegin` span; it is not stored
//!   separately because it is derivable).
//! - [`TokenKind::StrFragment`] is a maximal run of literal text with its
//!   *original* span — escapes (`\n`, `\u{…}`) and the `{{` / `}}` brace
//!   escapes stay inside fragments *uncooked*; value construction is
//!   comptime/sema work (D26), the lexer stays span-honest.
//! - [`TokenKind::InterpOpen`] (`{`) re-enters normal tokenization; the
//!   lexer tracks brace/paren/bracket nesting so a depth-0 `}` emits
//!   [`TokenKind::InterpClose`] and resumes the string. A depth-0 `:`
//!   inside the interpolation emits [`TokenKind::FormatSpecBegin`]; spec
//!   text is emitted as `StrFragment`s and nested `{width}` interpolations
//!   re-push (PEP 498 lineage). Nested string literals inside
//!   interpolations nest through the same stack.
//! - [`TokenKind::StrEnd`] spans the closing delimiter and carries the
//!   literal's dedent margin width in bytes (SE-0168 closing-column rule;
//!   `0` for everything but multiline literals). Fragments keep original
//!   spans — the consumer strips `dedent` bytes after each newline when
//!   cooking the value.
//! - Raw and generalized bodies never contain escapes or interpolation:
//!   they are a single `StrFragment` (omitted when empty).
//!
//! **Balance guarantee**: every `StrBegin` is matched by exactly one
//! `StrEnd` and every `InterpOpen` by exactly one `InterpClose`, even on
//! malformed input — recovery emits zero-width closers (plain strings and
//! their format specs recover at end of line, multiline/raw at end of
//! file, generalized at end of line) plus a diagnostic. The mode stack is
//! empty when `Eof` is emitted, always.
//!
//! # Statement termination `[gram.lex.newline]`
//!
//! [`TokenKind::Term`] is emitted at a newline iff the last token on the
//! line is an identifier / `_`, a literal (`Int`, `Float`, `Char`,
//! `StrEnd`, `true`, `false`), `return` / `break` / `continue`, a closing `)` `]`
//! `}`, or postfix `?` — unless the innermost enclosing delimiter is `(`,
//! `[`, or an interpolation (a `{…}` block re-enables insertion whatever
//! it is nested in), or the previous token is the `]` closing an
//! attribute `#[…]`. An inserted `Term` spans the newline byte; an
//! explicit `;` is the same `Term` kind spanning the `;` (empty-statement
//! detection, E0002, is the parser's job). A zero-width `Term` is emitted
//! at end of file when the last line qualifies and lacks a newline.
//! Newlines that don't terminate are whitespace trivia.
//!
//! `#[` lexes as one dedicated [`TokenKind::PoundBracket`] token (not
//! `#` + `[`): the attribute opener is what suppresses `Term` after its
//! closing `]`, so it is a first-class kind.
//!
//! # Diagnostics (E01xx — the lexer's family)
//!
//! | code  | meaning                                                    |
//! |-------|------------------------------------------------------------|
//! | E0101 | invalid escape sequence                                    |
//! | E0102 | unterminated string literal / interpolation (a bare `{`)   |
//! | E0103 | a `"""` delimiter shares its line with text (open or close)|
//! | E0104 | a multiline content line sits left of the closing margin   |
//! | E0105 | margin tab/space mismatch vs. the closing margin           |
//! | E0106 | invalid UTF-8 (reported once per run of bad bytes)         |
//! | E0107 | stray byte / character (a mid-file BOM, lone `}` in a str) |
//! | E0108 | string/interpolation nesting deeper than [`MAX_NEST`]      |
//! | E0109 | unterminated raw or generalized literal                    |
//! | E0110 | malformed `char` literal (s121, `[gram.lex.char]`)         |
//!
//! One rule per code (D74, s136 — wolf-lang#230): E0103 is the
//! delimiter rule (both the opening `"""` with text after it and the
//! closing `"""` with text before it — delimiters stand alone), E0104
//! the margin rule, E0105 the tab/space rule, and E0102 owns the bare
//! `{` in a plain string (the interpolation it opens never closes
//! before the line ends). A byte order mark at byte 0 is stripped as
//! [`TriviaKind::Bom`], never a diagnostic; anywhere else it is E0107.

use wolf_span::Span;

mod lexer;

pub use lexer::lex;

/// Diagnostic codes the lexer can emit. Stable: they participate in the
/// differential protocol (spec/06) and the s10 catalog.
pub mod codes {
    // Semantic aliases for the s10 registry entries (`wolf_diag::registry`,
    // the single place a code can be born).
    use wolf_diag::{Code, codes as c};

    pub const INVALID_ESCAPE: Code = c::E0101;
    pub const UNTERMINATED_STRING: Code = c::E0102;
    /// D74: a `"""` delimiter that shares its line with text — the
    /// opening one (text after it) or the closing one (text before it).
    pub const DELIMITER_SHARES_LINE: Code = c::E0103;
    pub const UNDER_INDENTED: Code = c::E0104;
    pub const MARGIN_MISMATCH: Code = c::E0105;
    pub const INVALID_UTF8: Code = c::E0106;
    pub const STRAY_BYTE: Code = c::E0107;
    pub const NESTING_TOO_DEEP: Code = c::E0108;
    pub const UNTERMINATED_RAW: Code = c::E0109;
    pub const MALFORMED_CHAR: Code = c::E0110;
}

/// Maximum string/interpolation mode-stack depth. Deeper input gets an
/// E0108 error token instead of unbounded stack growth. (Note: spec/01
/// §1.5 sets the *semantic* nesting limit at 8 with its own diagnostic —
/// that friendlier ceiling is enforced by the parser; this is the
/// lexer's hard safety rail per the s07 contract.)
pub const MAX_NEST: usize = 32;

/// The 50 reserved keywords `[gram.inv.kw]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[rustfmt::skip]
pub enum Keyword {
    As, Asm, Assume, Borrow, Break, Comptime, Const, Continue, Copy, Defer,
    Distinct, Dyn, Else, Enum, Errdefer, Export, Extern, False, Fn, For,
    Freeze, Handle, If, Impl, Import, In, Let, Loop, Match, Move,
    Mut, Proc, Pub, Region, Return, Scope, Select, Shared, Spawn, Struct,
    Take, Trait, True, Type, Unsafe, Use, Var, Weak, When, While,
}

/// Text → keyword table. Normative source: `reserved_kw` in
/// spec/grammar.ebnf (a unit test asserts this table matches it exactly).
#[rustfmt::skip]
pub const KEYWORDS: &[(&str, Keyword)] = &[
    ("as", Keyword::As), ("asm", Keyword::Asm), ("assume", Keyword::Assume),
    ("borrow", Keyword::Borrow), ("break", Keyword::Break),
    ("comptime", Keyword::Comptime), ("const", Keyword::Const),
    ("continue", Keyword::Continue), ("copy", Keyword::Copy),
    ("defer", Keyword::Defer), ("distinct", Keyword::Distinct),
    ("dyn", Keyword::Dyn), ("else", Keyword::Else), ("enum", Keyword::Enum),
    ("errdefer", Keyword::Errdefer), ("export", Keyword::Export),
    ("extern", Keyword::Extern), ("false", Keyword::False),
    ("fn", Keyword::Fn), ("for", Keyword::For), ("freeze", Keyword::Freeze),
    ("handle", Keyword::Handle), ("if", Keyword::If), ("impl", Keyword::Impl),
    ("import", Keyword::Import), ("in", Keyword::In), ("let", Keyword::Let),
    ("loop", Keyword::Loop), ("match", Keyword::Match), ("move", Keyword::Move),
    ("mut", Keyword::Mut), ("proc", Keyword::Proc), ("pub", Keyword::Pub),
    ("region", Keyword::Region), ("return", Keyword::Return),
    ("scope", Keyword::Scope), ("select", Keyword::Select),
    ("shared", Keyword::Shared), ("spawn", Keyword::Spawn),
    ("struct", Keyword::Struct), ("take", Keyword::Take),
    ("trait", Keyword::Trait), ("true", Keyword::True), ("type", Keyword::Type),
    ("unsafe", Keyword::Unsafe), ("use", Keyword::Use), ("var", Keyword::Var),
    ("weak", Keyword::Weak), ("when", Keyword::When), ("while", Keyword::While),
];

pub fn keyword(text: &str) -> Option<Keyword> {
    KEYWORDS
        .binary_search_by_key(&text, |(s, _)| s)
        .ok()
        .map(|i| KEYWORDS[i].1)
}

/// Punctuation and operators — every operator in spec §3.2's precedence
/// table, the delimiters, `@` (pattern bindings), and `.`/`,`/`:` /`=`.
/// `;` is not here (it lexes as [`TokenKind::Term`]); `#[` is
/// [`TokenKind::PoundBracket`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[rustfmt::skip]
pub enum Punct {
    LParen, RParen, LBracket, RBracket, LBrace, RBrace,
    Comma, Dot, DotDot, DotDotEq, Colon, At, Arrow, FatArrow,
    Eq, EqEq, NotEq, Lt, Gt, LtEq, GtEq, Spaceship,
    Plus, Minus, Star, Slash, Percent,
    Amp, AmpAmp, Pipe, PipePipe, Caret, Not, Question, Shl, Shr,
    PlusEq, MinusEq, StarEq, SlashEq, PercentEq,
    AmpEq, PipeEq, CaretEq, ShlEq, ShrEq,
}

/// Which delimiter opened a string episode (payload of
/// [`TokenKind::StrBegin`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StrKind {
    /// `"…"` — escapes + interpolation.
    Plain,
    /// `"""` — dedent by closing column, escapes + interpolation.
    Multiline,
    /// `r"…"` / `r#"…"#` — no escapes, no interpolation.
    Raw { hashes: u32 },
    /// `ident"…"` (no whitespace) — raw-mode body; desugars in sema (s16).
    Generalized,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokenKind {
    Ident,
    /// `_`, the wildcard — its own kind because it is never a binding.
    Underscore,
    Int,
    Float,
    /// `'a'` — a `char` literal (s121, `[gram.lex.char]`): one Unicode
    /// scalar value between single quotes; escapes stay uncooked in the
    /// span (value construction is sema work, the lexer stays
    /// span-honest, exactly as string fragments do).
    Char,
    Kw(Keyword),
    Punct(Punct),
    /// `#[` — the attribute opener (see crate docs).
    PoundBracket,
    /// `#![` — the file-wide (inner) attribute opener (`[gram.attr.index]`).
    /// One dedicated token for the same reason as `#[`; a `#!` at byte 0
    /// NOT followed by `[` is still the shebang line.
    PoundBangBracket,
    /// Statement terminator: inserted (spans the newline, or zero-width
    /// at EOF) or explicit (spans the `;`).
    Term,
    StrBegin(StrKind),
    StrFragment,
    /// `{` switching from string text to expression tokens.
    InterpOpen,
    /// The interpolation-closing `}`.
    InterpClose,
    /// The depth-0 `:` that starts a format spec inside an interpolation.
    FormatSpecBegin,
    /// Closing delimiter; `dedent` is the multiline margin width in bytes
    /// (0 for plain/raw/generalized).
    StrEnd {
        dedent: u32,
    },
    /// Bytes that form no token: stray characters, invalid UTF-8, invalid
    /// escapes, unterminated-literal recovery points (zero-width). Always
    /// paired with a diagnostic.
    Error,
    /// Completion marker: always the last token, zero-width, owns
    /// end-of-file trivia.
    Eof,
}

impl TokenKind {
    /// Is a statement terminator inserted after this token at a newline?
    /// The closed set from `[gram.lex.newline]`.
    pub fn ends_statement(self) -> bool {
        match self {
            TokenKind::Ident
            | TokenKind::Underscore
            | TokenKind::Int
            | TokenKind::Float
            | TokenKind::Char
            | TokenKind::StrEnd { .. } => true,
            TokenKind::Kw(k) => matches!(
                k,
                Keyword::True
                    | Keyword::False
                    | Keyword::Return
                    | Keyword::Break
                    | Keyword::Continue
            ),
            TokenKind::Punct(p) => matches!(
                p,
                Punct::RParen | Punct::RBracket | Punct::RBrace | Punct::Question
            ),
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TriviaKind {
    /// Spaces, tabs, `\r`, and non-terminating newlines.
    Whitespace,
    /// `// …` (to end of line, newline excluded).
    LineComment,
    /// `/// …` outer doc comment.
    DocComment,
    /// `//! …` inner doc comment.
    InnerDocComment,
    /// A UTF-8 byte order mark (`EF BB BF`) at byte 0 only — stripped,
    /// never a diagnostic (D74, s136): an editor that writes one has
    /// not made the file wrong, and the formatter keeps it in place.
    /// Anywhere else the same three bytes are a stray character
    /// (E0107).
    Bom,
    /// `#!…` on the FIRST line of a file only — the interpreter
    /// directive of an executable script (s53). Trivia, not a comment:
    /// it is not `//`-shaped, it carries no prose, and it is legal at
    /// exactly one byte offset in a translation unit (`#!` anywhere
    /// else is still the stray-byte error it always was).
    Shebang,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub span: Span,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub leading: Vec<Trivia>,
    pub trailing: Vec<Trivia>,
}

/// The result of [`lex`]: the token stream (ending in `Eof`) and every
/// diagnostic, in source order.
#[derive(Debug)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<wolf_diag::Diagnostic>,
}

impl Lexed {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error)
    }

    /// Reconstruct the input from spans — the lossless invariant made
    /// callable (unit tests and the fuzz target assert equality).
    pub fn reassemble(&self, src: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(src.len());
        let slice = |s: Span| &src[s.lo as usize..s.hi as usize];
        for t in &self.tokens {
            for tr in &t.leading {
                out.extend_from_slice(slice(tr.span));
            }
            out.extend_from_slice(slice(t.span));
            for tr in &t.trailing {
                out.extend_from_slice(slice(tr.span));
            }
        }
        out
    }

    /// Compact one-piece-per-line textual dump (kind + span + slice
    /// preview) — the snapshot-test format.
    pub fn dump(&self, src: &[u8]) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let preview = |s: Span| -> String {
            let bytes = &src[s.lo as usize..s.hi as usize];
            let text: String = String::from_utf8_lossy(bytes).chars().take(36).collect();
            let esc: String = text.escape_debug().collect();
            let cut = bytes.len() > 36;
            format!("\"{esc}{}\"", if cut { "…" } else { "" })
        };
        for t in &self.tokens {
            for tr in &t.leading {
                let _ = writeln!(
                    out,
                    "    lead  {:?} {}..{} {}",
                    tr.kind,
                    tr.span.lo,
                    tr.span.hi,
                    preview(tr.span)
                );
            }
            let _ = writeln!(
                out,
                "{:?} {}..{} {}",
                t.kind,
                t.span.lo,
                t.span.hi,
                preview(t.span)
            );
            for tr in &t.trailing {
                let _ = writeln!(
                    out,
                    "    trail {:?} {}..{} {}",
                    tr.kind,
                    tr.span.lo,
                    tr.span.hi,
                    preview(tr.span)
                );
            }
        }
        if !self.diagnostics.is_empty() {
            let _ = writeln!(out, "-- diagnostics --");
            for d in &self.diagnostics {
                let _ = writeln!(
                    out,
                    "{} [{:?}] {}..{} {}",
                    d.code,
                    d.severity,
                    d.span().lo,
                    d.span().hi,
                    d.message
                );
            }
        }
        out
    }
}

#[cfg(test)]
mod table_tests {
    use super::*;

    #[test]
    fn keyword_table_is_sorted_and_complete() {
        // binary_search in `keyword()` requires sort order.
        assert!(KEYWORDS.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(KEYWORDS.len(), 50, "[gram.inv.kw]: the count is a checksum");
        assert_eq!(keyword("when"), Some(Keyword::When));
        assert_eq!(keyword("timeout"), None, "contextual keywords stay idents");
    }
}
