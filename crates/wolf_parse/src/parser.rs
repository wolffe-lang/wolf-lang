//! The event-driven parser skeleton: the parser emits a flat
//! start/token/finish event stream over the lexed tokens; the builder
//! assembles the green tree afterward. Speculative parsing is cheap —
//! a [`Checkpoint`] truncates events, position, and diagnostics.

use wolf_ast::SyntaxKind;
use wolf_diag::Diagnostic;
use wolf_lex::{Keyword, Punct, Token, TokenKind};
use wolf_span::Span;

/// One parser event; the builder folds these into the green tree.
#[derive(Debug)]
pub(crate) enum Event {
    /// Open a node. `kind` may still be `Tombstone` (abandoned marker —
    /// skipped by the builder). `forward_parent` supports late wrapping
    /// (`CompletedMarker::precede`).
    Start {
        kind: SyntaxKind,
        forward_parent: Option<u32>,
    },
    /// Close the most recently opened node.
    Finish,
    /// Consume the next lexer token into the current node.
    /// `kind_override` reclassifies contextual keywords (e.g. `self`).
    Token { kind_override: Option<SyntaxKind> },
    /// Insert a zero-width [`SyntaxKind::Missing`] marker.
    Missing,
}

/// An open node; must be completed or abandoned.
pub(crate) struct Marker {
    pos: u32,
}

/// A closed node; can be wrapped by a later parent (`precede`).
#[derive(Clone, Copy)]
pub(crate) struct CompletedMarker {
    pos: u32,
}

/// A rollback point for speculative parsing (event stream, token
/// position, and diagnostics all rewind together).
pub(crate) struct Checkpoint {
    events: usize,
    pos: usize,
    diags: usize,
}

pub(crate) struct Parser<'a> {
    tokens: &'a [Token],
    src: &'a [u8],
    pos: usize,
    pub(crate) events: Vec<Event>,
    pub(crate) diags: Vec<Diagnostic>,
}

/// The panic-mode sync set: declaration-leading keywords (spec §2) —
/// recovery stops at these at brace depth zero.
pub(crate) fn is_decl_keyword(k: Keyword) -> bool {
    matches!(
        k,
        Keyword::Fn
            | Keyword::Let
            | Keyword::Var
            | Keyword::Type
            | Keyword::Struct
            | Keyword::Enum
            | Keyword::Trait
            | Keyword::Impl
            | Keyword::Use
            | Keyword::Import
            | Keyword::Const
            | Keyword::Extern
            | Keyword::Export
            | Keyword::Comptime
            | Keyword::Pub
    )
}

impl<'a> Parser<'a> {
    pub(crate) fn new(tokens: &'a [Token], src: &'a [u8]) -> Self {
        Parser {
            tokens,
            src,
            pos: 0,
            events: Vec::new(),
            diags: Vec::new(),
        }
    }

    // ------------------------------------------------------ inspection --

    pub(crate) fn current(&self) -> TokenKind {
        self.tokens[self.pos.min(self.tokens.len() - 1)].kind
    }

    /// Lookahead. Bounded uses only, each behind a named helper in the
    /// grammar with a comment saying why.
    pub(crate) fn nth(&self, n: usize) -> TokenKind {
        self.tokens[(self.pos + n).min(self.tokens.len() - 1)].kind
    }

    pub(crate) fn current_span(&self) -> Span {
        self.tokens[self.pos.min(self.tokens.len() - 1)].span
    }

    /// Zero-width span at the start of the current token — where a
    /// missing token would have been.
    pub(crate) fn here(&self) -> Span {
        let s = self.current_span();
        Span::new(s.file, s.lo, s.lo)
    }

    /// The current token's text (for contextual keywords: `self`, `pkg`,
    /// `c`).
    pub(crate) fn current_text(&self) -> &'a [u8] {
        let s = self.current_span();
        &self.src[s.lo as usize..s.hi as usize]
    }

    pub(crate) fn at(&self, kind: TokenKind) -> bool {
        self.current() == kind
    }

    pub(crate) fn at_punct(&self, p: Punct) -> bool {
        self.current() == TokenKind::Punct(p)
    }

    pub(crate) fn at_kw(&self, k: Keyword) -> bool {
        self.current() == TokenKind::Kw(k)
    }

    pub(crate) fn at_eof(&self) -> bool {
        self.current() == TokenKind::Eof
    }

    pub(crate) fn at_str_begin(&self) -> bool {
        matches!(self.current(), TokenKind::StrBegin(_))
    }

    /// Is the current token a declaration start (sync point)?
    pub(crate) fn at_decl_start(&self) -> bool {
        match self.current() {
            TokenKind::PoundBracket => true,
            TokenKind::Kw(k) => is_decl_keyword(k),
            _ => false,
        }
    }

    // ------------------------------------------------------ consumption --

    pub(crate) fn bump(&mut self) {
        assert!(!self.at_eof(), "bump past Eof");
        self.events.push(Event::Token {
            kind_override: None,
        });
        self.pos += 1;
    }

    /// Bump, reclassifying the token's kind (contextual keywords).
    pub(crate) fn bump_as(&mut self, kind: SyntaxKind) {
        assert!(!self.at_eof(), "bump past Eof");
        self.events.push(Event::Token {
            kind_override: Some(kind),
        });
        self.pos += 1;
    }

    /// Consume the final `Eof` token (it carries end-of-file trivia).
    pub(crate) fn bump_eof(&mut self) {
        assert!(self.at_eof(), "bump_eof off Eof");
        self.events.push(Event::Token {
            kind_override: None,
        });
        self.pos += 1;
    }

    /// Insert a zero-width missing-token marker (after diagnosing).
    pub(crate) fn missing(&mut self) {
        self.events.push(Event::Missing);
    }

    pub(crate) fn eat_terms(&mut self) {
        while self.at(TokenKind::Term) {
            self.bump();
        }
    }

    /// `expect`: bump `kind` or diagnose (E0201) + insert a zero-width
    /// missing marker and continue.
    pub(crate) fn expect_punct(&mut self, punct: Punct, what: &str) -> bool {
        if self.at_punct(punct) {
            self.bump();
            true
        } else {
            self.error(
                crate::codes::EXPECTED_TOKEN,
                self.here(),
                format!("expected {what}"),
            );
            self.missing();
            false
        }
    }

    pub(crate) fn expect_kw(&mut self, kw: Keyword, what: &str) -> bool {
        if self.at_kw(kw) {
            self.bump();
            true
        } else {
            self.error(
                crate::codes::EXPECTED_TOKEN,
                self.here(),
                format!("expected {what}"),
            );
            self.missing();
            false
        }
    }

    // ------------------------------------------------------ diagnostics --

    pub(crate) fn error(&mut self, code: &'static str, span: Span, message: impl Into<String>) {
        self.diags.push(Diagnostic::error(code, span, message));
    }

    pub(crate) fn error_with_note(
        &mut self,
        code: &'static str,
        span: Span,
        message: impl Into<String>,
        note_span: Span,
        note: impl Into<String>,
    ) {
        self.diags
            .push(Diagnostic::error(code, span, message).with_note(note_span, note));
    }

    // ----------------------------------------------------------- events --

    pub(crate) fn start(&mut self) -> Marker {
        let pos = self.events.len() as u32;
        self.events.push(Event::Start {
            kind: SyntaxKind::Tombstone,
            forward_parent: None,
        });
        Marker { pos }
    }

    pub(crate) fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            events: self.events.len(),
            pos: self.pos,
            diags: self.diags.len(),
        }
    }

    /// Rewind to `cp` — events, token position, and diagnostics together.
    pub(crate) fn rollback(&mut self, cp: Checkpoint) {
        self.events.truncate(cp.events);
        self.pos = cp.pos;
        self.diags.truncate(cp.diags);
    }

    // --------------------------------------------------------- recovery --

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    /// Panic-mode recovery: skip tokens until a sync point, attaching
    /// them to whatever node is currently open. Sync points, all at
    /// local delimiter depth zero: the declaration-leading keywords /
    /// `#[`, `Eof`, a `}` closing the current item (when
    /// `stop_at_rbrace`), and any token `stop` accepts. Skipped
    /// delimiters are depth-tracked so a keyword inside a skipped `{…}`
    /// never syncs; string episodes are skipped wholesale (the lexer
    /// guarantees their balance). Returns how many tokens were skipped.
    pub(crate) fn skip_until(
        &mut self,
        stop_at_rbrace: bool,
        stop: impl Fn(TokenKind) -> bool,
    ) -> usize {
        let mut skipped = 0usize;
        let mut depth = 0usize;
        loop {
            let k = self.current();
            if k == TokenKind::Eof {
                break;
            }
            if depth == 0 {
                if stop(k) {
                    break;
                }
                match k {
                    TokenKind::PoundBracket => break,
                    TokenKind::Kw(kw) if is_decl_keyword(kw) => break,
                    TokenKind::Punct(Punct::RBrace) if stop_at_rbrace => break,
                    _ => {}
                }
            }
            match k {
                TokenKind::Punct(Punct::LParen | Punct::LBracket | Punct::LBrace)
                | TokenKind::StrBegin(_)
                | TokenKind::InterpOpen => depth += 1,
                TokenKind::Punct(Punct::RParen | Punct::RBracket | Punct::RBrace)
                | TokenKind::StrEnd { .. }
                | TokenKind::InterpClose => depth = depth.saturating_sub(1),
                _ => {}
            }
            self.bump();
            skipped += 1;
        }
        skipped
    }

    /// [`Parser::skip_until`], wrapping whatever was skipped in an
    /// `ErrorNode` (created only if at least one token was skipped).
    pub(crate) fn recover_until(
        &mut self,
        stop_at_rbrace: bool,
        stop: impl Fn(TokenKind) -> bool,
    ) -> bool {
        let m = self.start();
        let skipped = self.skip_until(stop_at_rbrace, stop);
        if skipped > 0 {
            m.complete(self, SyntaxKind::ErrorNode);
            true
        } else {
            m.abandon(self);
            false
        }
    }
}

impl Marker {
    pub(crate) fn complete(self, p: &mut Parser<'_>, kind: SyntaxKind) -> CompletedMarker {
        match &mut p.events[self.pos as usize] {
            Event::Start { kind: slot, .. } => *slot = kind,
            _ => unreachable!("marker does not point at a Start event"),
        }
        p.events.push(Event::Finish);
        CompletedMarker { pos: self.pos }
    }

    /// Drop an unused marker (its Start stays a Tombstone; the builder
    /// skips it).
    pub(crate) fn abandon(self, p: &mut Parser<'_>) {
        if self.pos as usize == p.events.len() - 1 {
            p.events.pop();
        }
        // A non-trailing tombstone is simply skipped by the builder.
    }
}

impl CompletedMarker {
    /// Open a new node that will *contain* this completed one — the
    /// event-stream form of "wrap what I already parsed" (used for
    /// or-patterns).
    pub(crate) fn precede(self, p: &mut Parser<'_>) -> Marker {
        let new = p.start();
        match &mut p.events[self.pos as usize] {
            Event::Start { forward_parent, .. } => *forward_parent = Some(new.pos),
            _ => unreachable!("completed marker does not point at a Start event"),
        }
        new
    }
}
