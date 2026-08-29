//! The event-driven parser skeleton: the parser emits a flat
//! start/token/finish event stream over the lexed tokens; the builder
//! assembles the green tree afterward. Speculative parsing is cheap —
//! a [`Checkpoint`] truncates events, position, and diagnostics.

use wolf_ast::SyntaxKind;
use wolf_diag::{Code, DiagMark, Diagnostic, Diagnostics};
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
    kind: SyntaxKind,
}

impl CompletedMarker {
    /// The kind this node was completed with (the expression climb
    /// keys chain rules — ranges, comparisons — off it).
    pub(crate) fn kind(self) -> SyntaxKind {
        self.kind
    }
}

/// A rollback point for speculative parsing (event stream, token
/// position, and diagnostics all rewind together).
pub(crate) struct Checkpoint {
    events: usize,
    pos: usize,
    diags: DiagMark,
}

pub(crate) struct Parser<'a> {
    tokens: &'a [Token],
    src: &'a [u8],
    pos: usize,
    pub(crate) events: Vec<Event>,
    /// The s10 sink: cascade suppression lives here — every skipped
    /// region recovery wraps in an error node is fed to
    /// [`Diagnostics::suppress`], so later diagnostics inside it drop.
    pub(crate) diags: Diagnostics,
    /// Expression-recursion depth (bounded — hostile nesting degrades
    /// into error recovery instead of overflowing the stack).
    pub(crate) depth: u32,
    /// The lexer's delimiter-frame automaton, replayed over consumed
    /// tokens (same top-match pop rule): whenever the innermost frame
    /// is `(`/`[`, the lexer suppressed terminator insertion there
    /// (`[gram.lex.newline]`).
    frames: Vec<Punct>,
    /// Missing-terminator fold: one unclosed `(`/`[` suppresses
    /// terminators for every following line — every such miss is one
    /// wreck, reported once per file (s10 owns real cascade
    /// suppression). Genuine same-line misses (no suppression active)
    /// always report. Also set by unclosed-delimiter and
    /// truncated-construct reports — the escape *is* the line's error.
    suppressed_miss_reported: bool,
    /// Indent of the declaration currently being parsed. Recovery may
    /// skip anything nested deeper, but never a declaration starting at
    /// or left of this column — see [`skip_until_from`].
    pub(crate) item_floor: Option<u32>,
    /// One broken argument list reports once per line region — nested
    /// lists dragged into the same wreck stay silent (D22 containment).
    /// Cleared whenever a real terminator is consumed.
    pub(crate) arg_error_reported: bool,
    /// A run of not-a-declaration lines reports once — cleared when a
    /// real declaration keyword is reached (D22 containment).
    pub(crate) toplevel_error_reported: bool,
    /// Assignment-in-expression (E0208) reports once per line region —
    /// a chain (`a = b = c`) is one mistake. Cleared by any consumed
    /// terminator.
    pub(crate) assign_error_reported: bool,
    /// One-shot: a truncated-construct report was just emitted; the
    /// immediately following missing-terminator miss is the same wreck.
    /// Cleared by any consumed terminator.
    line_end_fold_once: bool,
    /// Unclosed-delimiter reports discovered at end of file fold into
    /// one: the file ended mid-wreck, and every still-open delimiter is
    /// the same story.
    pub(crate) eof_unclosed_reported: bool,
    /// Byte position of the last unclosed-delimiter report's stall
    /// point: several openers stalling at the same token are one wreck.
    pub(crate) last_unclosed_at: Option<u32>,
    /// A run of structurally broken match/select arms reports once —
    /// cleared when an arm parses without diagnostics (D22
    /// containment; one misaligned arm otherwise poisons the list).
    pub(crate) arm_error_reported: bool,
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
            diags: Diagnostics::new(),
            depth: 0,
            frames: Vec::new(),
            suppressed_miss_reported: false,
            item_floor: None,
            arg_error_reported: false,
            toplevel_error_reported: false,
            arm_error_reported: false,
            line_end_fold_once: false,
            assign_error_reported: false,
            eof_unclosed_reported: false,
            last_unclosed_at: None,
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

    /// The span of the `n`-th lookahead token (clamped to Eof).
    pub(crate) fn nth_span(&self, n: usize) -> Span {
        self.tokens[(self.pos + n).min(self.tokens.len() - 1)].span
    }

    /// The kind of the token *before* the current one (`None` at the
    /// start of the stream). Used by the empty-statement check: a `;`
    /// terminates nothing iff nothing sits between it and the previous
    /// terminator / block opener.
    pub(crate) fn prev_kind(&self) -> Option<TokenKind> {
        self.pos.checked_sub(1).map(|i| self.tokens[i].kind)
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

    /// The `n`-th lookahead token's text (the D25 negative-index hint
    /// quotes the literal).
    pub(crate) fn nth_text(&self, n: usize) -> &'a [u8] {
        let s = self.nth_span(n);
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
    /// How many diagnostics have been reported so far — recovery uses
    /// it to notice that a sub-parse just failed.
    pub(crate) fn diag_count(&self) -> usize {
        self.diags.len()
    }

    /// Leading-whitespace width of the line containing `off`.
    pub(crate) fn line_indent(&self, off: u32) -> u32 {
        let off = off as usize;
        let start = self.src[..off.min(self.src.len())]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        let mut n = start;
        while n < self.src.len() && matches!(self.src[n], b' ' | b'\t') {
            n += 1;
        }
        (n - start) as u32
    }

    /// Does a line break sit between these two offsets?
    pub(crate) fn crosses_line(&self, from: u32, to: u32) -> bool {
        let (from, to) = (from as usize, (to as usize).min(self.src.len()));
        from < to && self.src[from..to].contains(&b'\n')
    }

    /// A keyword that only ever opens a TOP-LEVEL item. `let`/`var`/
    /// `const` are excluded: they are ordinary statements inside a
    /// block, so they say nothing about where the block ends.
    pub(crate) fn at_toplevel_decl_start(&self) -> bool {
        matches!(
            self.current(),
            TokenKind::Kw(
                Keyword::Fn
                    | Keyword::Struct
                    | Keyword::Enum
                    | Keyword::Trait
                    | Keyword::Impl
                    | Keyword::Use
                    | Keyword::Import
                    | Keyword::Extern
                    // `pub` is the visibility prefix of a declaration and
                    // nothing else — a cross-line `.` reaching a `pub`
                    // swallowed the NEXT item's visibility and re-keyed
                    // its span (#109's second latent case).
                    | Keyword::Pub
            )
        )
    }

    pub(crate) fn at_decl_start(&self) -> bool {
        match self.current() {
            TokenKind::PoundBracket | TokenKind::PoundBangBracket => true,
            TokenKind::Kw(k) => is_decl_keyword(k),
            _ => false,
        }
    }

    // ------------------------------------------------------ consumption --

    pub(crate) fn bump(&mut self) {
        assert!(!self.at_eof(), "bump past Eof");
        match self.current() {
            TokenKind::Term => {
                // A real terminator ends the per-line folds — the
                // argument fold only once every `(`/`[` list is closed
                // (terminators inside a block nested in a broken list
                // do not end the list's wreck).
                if !self
                    .frames
                    .iter()
                    .any(|f| matches!(f, Punct::LParen | Punct::LBracket))
                {
                    self.arg_error_reported = false;
                }
                self.line_end_fold_once = false;
                self.assign_error_reported = false;
            }
            TokenKind::Punct(p @ (Punct::LParen | Punct::LBracket | Punct::LBrace)) => {
                self.frames.push(p);
            }
            // `#[` / `#![` open a bracket the lexer suppresses
            // terminators inside; the frame mirror must agree, or an
            // unclosed attribute leaves every later line's missing
            // terminator unfolded (the s126 blast-radius wreck).
            TokenKind::PoundBracket | TokenKind::PoundBangBracket => {
                self.frames.push(Punct::LBracket);
            }
            TokenKind::Punct(Punct::RParen) => {
                if self.frames.last() == Some(&Punct::LParen) {
                    self.frames.pop();
                }
            }
            TokenKind::Punct(Punct::RBracket) => {
                if self.frames.last() == Some(&Punct::LBracket) {
                    self.frames.pop();
                }
            }
            TokenKind::Punct(Punct::RBrace) if self.frames.last() == Some(&Punct::LBrace) => {
                self.frames.pop();
            }
            _ => {}
        }
        self.events.push(Event::Token {
            kind_override: None,
        });
        self.pos += 1;
    }

    /// Is terminator insertion suppressed at the current position (the
    /// innermost consumed delimiter is `(` or `[`)?
    fn term_suppressed(&self) -> bool {
        matches!(self.frames.last(), Some(Punct::LParen | Punct::LBracket))
    }

    /// Enter the missing-terminator fold without reporting: the caller
    /// already emitted the line's error (a truncated construct).
    pub(crate) fn fold_line_end(&mut self) {
        self.line_end_fold_once = true;
    }

    /// An unclosed `(`/`[`/`{` was just reported: every downstream
    /// suppressed-terminator miss is that same wreck — pre-fold them
    /// all (plus the immediate one-shot).
    pub(crate) fn fold_unclosed(&mut self) {
        self.line_end_fold_once = true;
        self.suppressed_miss_reported = true;
        // A list stalling on the same wreck needs no second report.
        self.arg_error_reported = true;
    }

    /// A missing-terminator diagnostic, folded: in a region where an
    /// unclosed `(`/`[` suppressed insertion, only the first miss is
    /// reported (s10 owns full cascade suppression; this keeps the
    /// blast radius of one lost delimiter bounded, D22).
    pub(crate) fn expected_line_end(&mut self, what: &str) {
        if self.line_end_fold_once {
            self.line_end_fold_once = false;
            return;
        }
        if self.term_suppressed() {
            // A cascade of an earlier delimiter wreck: fold.
            if self.suppressed_miss_reported {
                return;
            }
            self.suppressed_miss_reported = true;
        }
        self.error(
            crate::codes::EXPECTED_TOKEN,
            self.here(),
            format!("expected the line to end after the {what}"),
        );
    }

    /// An unexpected-at-declaration-position diagnostic (E0203), folded
    /// over a run of stray lines (see `toplevel_error_reported`).
    pub(crate) fn toplevel_error(&mut self, span: Span, message: impl Into<String>) {
        self.toplevel_diag(Diagnostic::error(
            crate::codes::UNEXPECTED_TOPLEVEL,
            span,
            message,
        ));
    }

    /// [`Parser::toplevel_error`] for pre-built diagnostics (the typo
    /// path attaches a machine-applicable suggestion).
    pub(crate) fn toplevel_diag(&mut self, d: Diagnostic) {
        if self.toplevel_error_reported {
            return;
        }
        self.toplevel_error_reported = true;
        self.push_diag(d);
    }

    /// An error suppressed when a truncated-construct report was just
    /// emitted at the same spot (the one-shot fold).
    pub(crate) fn error_unless_folded(
        &mut self,
        code: Code,
        span: Span,
        message: impl Into<String>,
    ) {
        if self.line_end_fold_once {
            self.line_end_fold_once = false;
            return;
        }
        self.error(code, span, message);
    }

    /// An arm-structure diagnostic (pattern / `from` / `=>` /
    /// separator), folded over a run of broken arms (see
    /// `arm_error_reported`).
    pub(crate) fn arm_error(&mut self, span: Span, message: impl Into<String>) {
        if self.arm_error_reported {
            return;
        }
        self.arm_error_reported = true;
        self.error(crate::codes::EXPECTED_TOKEN, span, message);
    }

    /// An argument-list diagnostic, folded per line region (see
    /// `arg_error_reported`).
    pub(crate) fn arg_list_error(&mut self, span: Span, message: impl Into<String>) {
        if self.arg_error_reported {
            return;
        }
        self.arg_error_reported = true;
        self.error(crate::codes::EXPECTED_TOKEN, span, message);
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

    pub(crate) fn error(&mut self, code: Code, span: Span, message: impl Into<String>) {
        self.diags.push(Diagnostic::error(code, span, message));
    }

    /// Push a fully built diagnostic (label / secondary / suggestion
    /// call sites build it themselves).
    pub(crate) fn push_diag(&mut self, d: Diagnostic) {
        self.diags.push(d);
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
            diags: self.diags.mark(),
        }
    }

    /// Rewind to `cp` — events, token position, and diagnostics
    /// (including suppression regions) together.
    pub(crate) fn rollback(&mut self, cp: Checkpoint) {
        self.events.truncate(cp.events);
        self.pos = cp.pos;
        self.diags.rollback(cp.diags);
    }

    // --------------------------------------------------------- recovery --

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    /// Panic-mode recovery: skip tokens until a sync point, attaching
    /// them to whatever node is currently open. Sync points: any token
    /// `stop` accepts at local delimiter depth zero, `Eof`, and — when
    /// not *shielded* — the declaration-leading keywords / `#[` and a
    /// `}` closing the current item (when `stop_at_rbrace`). Shielding
    /// counts only `{…}` blocks and string episodes: nested items are
    /// legal in braces and keyword tokens occur in interpolations, but
    /// an open `(`/`[` never shields — a declaration keyword inside
    /// parens means an unclosed delimiter that must not swallow the
    /// next declaration (D22). Returns how many tokens were skipped.
    pub(crate) fn skip_until(
        &mut self,
        stop_at_rbrace: bool,
        stop: impl Fn(TokenKind) -> bool,
    ) -> usize {
        self.skip_until_from(stop_at_rbrace, None, stop)
    }

    /// [`skip_until`] with a *sibling floor*: an indent at or below
    /// which a declaration keyword ends recovery **whatever the depth**.
    ///
    /// A `{` shields, because nested items are legal in braces — but a
    /// `{` that is never closed shields all the way to `Eof`, and the
    /// D22 break that exists to protect the next declaration is exactly
    /// what gets disabled. One mutated brace in `traits/show_bound.lu`
    /// swallowed two whole functions this way: they produced no nodes
    /// at all, and the parser had already said `this `{` is never
    /// closed` while recovery skipped straight past it.
    ///
    /// The floor is the indent of the construct being recovered. An
    /// item keyword no deeper than that opened a sibling, not a nested
    /// item, so a brace nest claiming to contain it is not plausible
    /// and the shield does not apply. Passing `None` keeps the old
    /// behaviour for sites where indentation says nothing.
    pub(crate) fn skip_until_from(
        &mut self,
        stop_at_rbrace: bool,
        sibling_floor: Option<u32>,
        stop: impl Fn(TokenKind) -> bool,
    ) -> usize {
        let sibling_floor = sibling_floor.or(self.item_floor);
        let from = self.current_span();
        let mut skipped = 0usize;
        let mut depth = 0usize;
        let mut shield = 0usize;
        loop {
            let k = self.current();
            if k == TokenKind::Eof {
                break;
            }
            if depth == 0 && stop(k) {
                break;
            }
            // Universal depth-zero stops: an `InterpClose` (or format
            // spec) at depth zero belongs to the enclosing
            // interpolation — recovery must never eat its way out of a
            // string (the interpolation's own recovery owns it).
            if depth == 0 && matches!(k, TokenKind::InterpClose | TokenKind::FormatSpecBegin) {
                break;
            }
            // Depth-independent: a sibling-level item keyword ends
            // recovery even inside a brace that claims to contain it.
            if let (Some(floor), TokenKind::Kw(kw)) = (sibling_floor, k)
                && is_decl_keyword(kw)
                && self.line_indent(self.current_span().lo) <= floor
            {
                break;
            }
            if shield == 0 {
                match k {
                    TokenKind::PoundBracket | TokenKind::PoundBangBracket => break,
                    TokenKind::Kw(kw) if is_decl_keyword(kw) => break,
                    TokenKind::Punct(Punct::RBrace) if stop_at_rbrace => break,
                    _ => {}
                }
            }
            match k {
                TokenKind::Punct(Punct::LParen | Punct::LBracket) => depth += 1,
                TokenKind::Punct(Punct::LBrace)
                | TokenKind::StrBegin(_)
                | TokenKind::InterpOpen => {
                    depth += 1;
                    shield += 1;
                }
                TokenKind::Punct(Punct::RParen | Punct::RBracket) => {
                    depth = depth.saturating_sub(1)
                }
                TokenKind::Punct(Punct::RBrace)
                | TokenKind::StrEnd { .. }
                | TokenKind::InterpClose => {
                    depth = depth.saturating_sub(1);
                    shield = shield.saturating_sub(1);
                }
                _ => {}
            }
            self.bump();
            skipped += 1;
        }
        // Cascade suppression (s10): the skipped junk is one already-
        // reported wreck — feed its extent to the sink so any later
        // diagnostic whose primary span lands inside it is dropped.
        if skipped > 0 {
            let to = self.tokens[self.pos - 1].span;
            self.diags.suppress(Span::new(from.file, from.lo, to.hi));
        }
        skipped
    }

    /// Interpolation recovery: skip to the `InterpClose` that ends the
    /// current interpolation, wrapping the junk in an `ErrorNode`. The
    /// lexer *guarantees* the closer exists (episode balance), so —
    /// unlike [`Parser::skip_until`] — declaration keywords are not
    /// sync points here: they are string content gone wrong, and
    /// stopping on them would detonate the enclosing statement.
    pub(crate) fn recover_to_interp_close(&mut self) {
        let m = self.start();
        let from = self.current_span();
        let mut depth = 0usize;
        let mut skipped = 0usize;
        loop {
            match self.current() {
                TokenKind::Eof => break,
                TokenKind::InterpClose if depth == 0 => break,
                // An interpolation that reaches a sibling declaration
                // was never closed, and scanning on ate the rest of the
                // file: in `typecheck/trait_default.lu` a `)` mutated
                // to `match` sent this loop over a struct and an impl,
                // which then existed as no nodes at all. Same floor the
                // rest of recovery uses.
                TokenKind::Kw(kw)
                    if is_decl_keyword(kw)
                        && self
                            .item_floor
                            .is_some_and(|f| self.line_indent(self.current_span().lo) <= f) =>
                {
                    break;
                }
                // Nested strings *and* nested interpolations (format
                // specs re-nest: `{v:>{w}}`) shield their closers.
                TokenKind::StrBegin(_) | TokenKind::InterpOpen => depth += 1,
                TokenKind::StrEnd { .. } | TokenKind::InterpClose => {
                    depth = depth.saturating_sub(1)
                }
                _ => {}
            }
            self.bump();
            skipped += 1;
        }
        if skipped > 0 {
            let to = self.tokens[self.pos - 1].span;
            self.diags.suppress(Span::new(from.file, from.lo, to.hi));
            m.complete(self, SyntaxKind::ErrorNode);
        } else {
            m.abandon(self);
        }
    }

    /// [`Parser::skip_until`], wrapping whatever was skipped in an
    /// `ErrorNode` (created only if at least one token was skipped).
    pub(crate) fn recover_until(
        &mut self,
        stop_at_rbrace: bool,
        stop: impl Fn(TokenKind) -> bool,
    ) -> bool {
        self.recover_until_from(stop_at_rbrace, None, stop)
    }

    /// [`recover_until`] with a sibling floor — see [`skip_until_from`].
    pub(crate) fn recover_until_from(
        &mut self,
        stop_at_rbrace: bool,
        sibling_floor: Option<u32>,
        stop: impl Fn(TokenKind) -> bool,
    ) -> bool {
        let m = self.start();
        let skipped = self.skip_until_from(stop_at_rbrace, sibling_floor, stop);
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
        CompletedMarker {
            pos: self.pos,
            kind,
        }
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
