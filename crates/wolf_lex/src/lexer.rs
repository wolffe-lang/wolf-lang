//! The engine: one forward pass over bytes, a frame stack for delimiter
//! tracking and string modes, byte-exact spans, total on arbitrary input.

use wolf_diag::{Applicability, Diagnostic, Diagnostics, Suggestion};
use wolf_span::{FileId, Span};

/// The escapes wolf understands — the shared E0101 next-step note.
const ESCAPE_NOTE: &str = "the escapes wolf understands are `\\n`, `\\t`, `\\r`, `\\0`, `\\\\`, \
     `\\\"`, `\\xNN`, and `\\u{…}`. For a literal backslash, write `\\\\`; for text with no \
     escapes at all, use a raw string `r\"…\"`.";

use crate::{
    Lexed, MAX_NEST, Punct, StrKind, Token, TokenKind, Trivia, TriviaKind, codes, keyword,
};

/// Lex `src` (arbitrary bytes) for `file`. Total: never fails, never
/// panics — except on sources longer than `u32::MAX` bytes (spans are
/// `u32` byte offsets, D25).
pub fn lex(file: FileId, src: &[u8]) -> Lexed {
    assert!(
        src.len() <= u32::MAX as usize,
        "source exceeds u32::MAX bytes; spans are u32 byte offsets (D25)"
    );
    let lexer = Lexer {
        src,
        file,
        pos: 0,
        tokens: Vec::new(),
        diags: Diagnostics::new(),
        frames: Vec::new(),
        pending: Vec::new(),
        line_broken: true,
        attr_close: false,
    };
    lexer.run()
}

/// One entry on the context stack. `Paren`/`Bracket`/`Brace` exist for
/// newline suppression and attribute tracking only — delimiter *matching*
/// is the parser's job, so mismatched closers never pop the wrong frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Frame {
    Paren,
    Bracket {
        attr: bool,
    },
    Brace,
    /// Inside `{…}` of a string; `in_spec` after the depth-0 `:`.
    Interp {
        in_spec: bool,
    },
    Str(StrFrame),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct StrFrame {
    kind: StrKind,
    /// Start of the `StrBegin` token (for unterminated-literal spans).
    open_lo: usize,
    /// First byte after the opening delimiter.
    content_lo: usize,
}

enum Decoded {
    Char(char, usize),
    Invalid(usize),
    Eof,
}

/// Longest-match punctuation table (multi-byte entries first).
#[rustfmt::skip]
const PUNCTS: &[(&[u8], Punct)] = &[
    (b"<<=", Punct::ShlEq), (b">>=", Punct::ShrEq), (b"<=>", Punct::Spaceship),
    (b"..=", Punct::DotDotEq),
    (b"->", Punct::Arrow), (b"=>", Punct::FatArrow), (b"==", Punct::EqEq),
    (b"!=", Punct::NotEq), (b"<=", Punct::LtEq), (b">=", Punct::GtEq),
    (b"&&", Punct::AmpAmp), (b"||", Punct::PipePipe), (b"<<", Punct::Shl),
    (b">>", Punct::Shr), (b"+=", Punct::PlusEq), (b"-=", Punct::MinusEq),
    (b"*=", Punct::StarEq), (b"/=", Punct::SlashEq), (b"%=", Punct::PercentEq),
    (b"&=", Punct::AmpEq), (b"|=", Punct::PipeEq), (b"^=", Punct::CaretEq),
    (b"..", Punct::DotDot),
    (b"(", Punct::LParen), (b")", Punct::RParen), (b"[", Punct::LBracket),
    (b"]", Punct::RBracket), (b"{", Punct::LBrace), (b"}", Punct::RBrace),
    (b",", Punct::Comma), (b".", Punct::Dot), (b":", Punct::Colon),
    (b"=", Punct::Eq), (b"+", Punct::Plus), (b"-", Punct::Minus),
    (b"*", Punct::Star), (b"/", Punct::Slash), (b"%", Punct::Percent),
    (b"&", Punct::Amp), (b"|", Punct::Pipe), (b"^", Punct::Caret),
    (b"<", Punct::Lt), (b">", Punct::Gt), (b"!", Punct::Not),
    (b"?", Punct::Question), (b"@", Punct::At),
];

fn is_inline_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | 0x0b | 0x0c)
}

struct Lexer<'a> {
    src: &'a [u8],
    file: FileId,
    pos: usize,
    tokens: Vec<Token>,
    diags: Diagnostics,
    frames: Vec<Frame>,
    /// Leading trivia collected for the next token.
    pending: Vec<Trivia>,
    /// A newline has passed since the last token (trivia now attaches as
    /// leading, and the per-line terminator decision is spent).
    line_broken: bool,
    /// The last token was the `]` closing a `#[…]` attribute.
    attr_close: bool,
}

impl Lexer<'_> {
    fn run(mut self) -> Lexed {
        while self.pos < self.src.len() {
            match self.frames.last() {
                Some(Frame::Str(f)) => {
                    let f = *f;
                    self.body_step(f);
                }
                Some(Frame::Interp { in_spec: true }) => self.spec_step(),
                _ => self.normal_step(),
            }
        }
        self.finish();
        Lexed {
            tokens: self.tokens,
            diagnostics: self.diags.into_vec(),
        }
    }

    // ------------------------------------------------------------ plumbing

    fn span(&self, lo: usize, hi: usize) -> Span {
        Span::new(self.file, lo as u32, hi as u32)
    }

    fn peek(&self, off: usize) -> Option<u8> {
        self.src.get(self.pos + off).copied()
    }

    fn starts_with(&self, at: usize, needle: &[u8]) -> bool {
        self.src[at..].starts_with(needle)
    }

    fn emit(&mut self, kind: TokenKind, lo: usize, hi: usize) {
        let leading = std::mem::take(&mut self.pending);
        self.tokens.push(Token {
            kind,
            span: self.span(lo, hi),
            leading,
            trailing: Vec::new(),
        });
        self.line_broken = false;
        self.attr_close = false;
    }

    /// The E0108 hard-rail diagnostic (three emit sites, one wording).
    fn nest_diag(&mut self, lo: usize, hi: usize) {
        let span = self.span(lo, hi);
        self.diags.push(
            Diagnostic::error(
                codes::NESTING_TOO_DEEP,
                span,
                format!("strings and interpolations nest more than {MAX_NEST} levels deep here"),
            )
            .with_label(format!("level {} would start here", MAX_NEST + 1))
            .with_note(
                "this is the lexer's hard safety rail; the language's own limit is 8 \
                 levels (E0007). Hoist the innermost string into a `let` binding — \
                 each hoist removes a level.",
            ),
        );
    }

    fn push_trivia(&mut self, kind: TriviaKind, lo: usize, hi: usize) {
        if lo == hi {
            return;
        }
        let piece = Trivia {
            kind,
            span: self.span(lo, hi),
        };
        match self.tokens.last_mut() {
            Some(last) if !self.line_broken => last.trailing.push(piece),
            _ => self.pending.push(piece),
        }
    }

    /// Decode the UTF-8 character at `at`, or the length of the invalid
    /// byte run in front of it.
    fn decode(&self, at: usize) -> Decoded {
        let Some(&b) = self.src.get(at) else {
            return Decoded::Eof;
        };
        if b < 0x80 {
            return Decoded::Char(b as char, 1);
        }
        let chunk = &self.src[at..self.src.len().min(at + 4)];
        match std::str::from_utf8(chunk) {
            Ok(s) => match s.chars().next() {
                Some(c) => Decoded::Char(c, c.len_utf8()),
                None => Decoded::Invalid(1),
            },
            Err(e) if e.valid_up_to() > 0 => {
                match std::str::from_utf8(&chunk[..e.valid_up_to()])
                    .ok()
                    .and_then(|s| s.chars().next())
                {
                    Some(c) => Decoded::Char(c, c.len_utf8()),
                    None => Decoded::Invalid(1),
                }
            }
            Err(e) => Decoded::Invalid(e.error_len().unwrap_or(chunk.len()).max(1)),
        }
    }

    fn nest_depth(&self) -> usize {
        self.frames
            .iter()
            .filter(|f| matches!(f, Frame::Str(_) | Frame::Interp { .. }))
            .count()
    }

    /// Would a terminator be inserted at a newline right now?
    /// `[gram.lex.newline]`: last-token class, innermost-delimiter
    /// suppression, and the attribute-`]` exception.
    fn term_due(&self) -> bool {
        if self.attr_close {
            return false;
        }
        if matches!(
            self.frames.last(),
            Some(Frame::Paren | Frame::Bracket { .. } | Frame::Interp { .. })
        ) {
            return false;
        }
        self.tokens.last().is_some_and(|t| t.kind.ends_statement())
    }

    // ------------------------------------------------------------- normal

    fn normal_step(&mut self) {
        self.skip_trivia();
        if self.pos < self.src.len() {
            self.lex_token();
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.peek(0) {
                Some(b'\n') => {
                    if !self.line_broken && self.term_due() {
                        let lo = self.pos;
                        self.pos += 1;
                        self.emit(TokenKind::Term, lo, self.pos);
                        // The inserted terminator IS the line break.
                        self.line_broken = true;
                    } else {
                        let start = self.pos;
                        self.line_broken = true;
                        self.pos += 1;
                        while self.peek(0).is_some_and(|b| is_inline_ws(b) || b == b'\n') {
                            self.pos += 1;
                        }
                        self.push_trivia(TriviaKind::Whitespace, start, self.pos);
                    }
                }
                Some(b) if is_inline_ws(b) => {
                    let start = self.pos;
                    while self.peek(0).is_some_and(is_inline_ws) {
                        self.pos += 1;
                    }
                    // Stop before `\n`: trailing trivia ends at the first
                    // newline, and the newline may become a Term token.
                    self.push_trivia(TriviaKind::Whitespace, start, self.pos);
                }
                // `#!` at byte 0: the script interpreter line (s53).
                // Trivia, so `wolf run script.lu` lexes an executable
                // file unchanged and the formatter round-trips it. One
                // offset only — a `#!` on line 2 stays E0107.
                Some(b'#') if self.pos == 0 && self.peek(1) == Some(b'!') => {
                    let start = self.pos;
                    while self.peek(0).is_some_and(|b| b != b'\n') {
                        self.pos += 1;
                    }
                    self.push_trivia(TriviaKind::Shebang, start, self.pos);
                }
                Some(b'/') if self.peek(1) == Some(b'/') => {
                    let start = self.pos;
                    while self.peek(0).is_some_and(|b| b != b'\n') {
                        self.pos += 1;
                    }
                    let rest = &self.src[start..self.pos];
                    let kind = if rest.starts_with(b"//!") {
                        TriviaKind::InnerDocComment
                    } else if rest.starts_with(b"///") && !rest.starts_with(b"////") {
                        TriviaKind::DocComment
                    } else {
                        TriviaKind::LineComment
                    };
                    self.push_trivia(kind, start, self.pos);
                }
                _ => return,
            }
        }
    }

    fn lex_token(&mut self) {
        let b = self.src[self.pos];
        match b {
            b'"' => self.open_quote_string(),
            b'0'..=b'9' => self.number(),
            b'#' => {
                if self.peek(1) == Some(b'[') {
                    let lo = self.pos;
                    self.pos += 2;
                    self.emit(TokenKind::PoundBracket, lo, self.pos);
                    self.frames.push(Frame::Bracket { attr: true });
                } else {
                    self.stray();
                }
            }
            b';' => {
                let lo = self.pos;
                self.pos += 1;
                self.emit(TokenKind::Term, lo, self.pos);
            }
            b'_' | b'a'..=b'z' | b'A'..=b'Z' => self.word(),
            _ if b < 0x80 => match PUNCTS
                .iter()
                .find(|(text, _)| self.starts_with(self.pos, text))
            {
                Some(&(text, p)) => self.punct(text.len(), p),
                None => self.stray(),
            },
            _ => match self.decode(self.pos) {
                Decoded::Char(c, _) if unicode_ident::is_xid_start(c) => self.word(),
                _ => self.stray(),
            },
        }
    }

    fn punct(&mut self, len: usize, p: Punct) {
        let lo = self.pos;
        self.pos += len;
        // `}` and `:` are string-protocol tokens when the innermost frame
        // is an interpolation.
        match p {
            Punct::RBrace if matches!(self.frames.last(), Some(Frame::Interp { .. })) => {
                self.frames.pop();
                self.emit(TokenKind::InterpClose, lo, self.pos);
                return;
            }
            Punct::Colon
                if matches!(self.frames.last(), Some(Frame::Interp { in_spec: false })) =>
            {
                if let Some(Frame::Interp { in_spec }) = self.frames.last_mut() {
                    *in_spec = true;
                }
                self.emit(TokenKind::FormatSpecBegin, lo, self.pos);
                return;
            }
            _ => {}
        }
        self.emit(TokenKind::Punct(p), lo, self.pos);
        match p {
            Punct::LParen => self.frames.push(Frame::Paren),
            Punct::LBracket => self.frames.push(Frame::Bracket { attr: false }),
            Punct::LBrace => self.frames.push(Frame::Brace),
            Punct::RParen => {
                if matches!(self.frames.last(), Some(Frame::Paren)) {
                    self.frames.pop();
                }
            }
            Punct::RBracket => {
                if let Some(&Frame::Bracket { attr }) = self.frames.last() {
                    self.frames.pop();
                    if attr {
                        // No terminator after an attribute's closing `]`.
                        self.attr_close = true;
                    }
                }
            }
            Punct::RBrace => {
                if matches!(self.frames.last(), Some(Frame::Brace)) {
                    self.frames.pop();
                }
            }
            _ => {}
        }
    }

    fn number(&mut self) {
        let lo = self.pos;
        // Radix prefixes require a first digit ([gram.lex.number]); `0x`
        // without one is the integer `0` followed by a word.
        if self.src[self.pos] == b'0' {
            let radix = match self.peek(1) {
                Some(b'x') => 16,
                Some(b'o') => 8,
                Some(b'b') => 2,
                _ => 0,
            };
            if radix != 0 && self.peek(2).is_some_and(|b| (b as char).is_digit(radix)) {
                self.pos += 2;
                while self
                    .peek(0)
                    .is_some_and(|b| b == b'_' || (b as char).is_digit(radix))
                {
                    self.pos += 1;
                }
                self.emit(TokenKind::Int, lo, self.pos);
                return;
            }
        }
        let dec = |lx: &mut Self| {
            while lx.peek(0).is_some_and(|b| b == b'_' || b.is_ascii_digit()) {
                lx.pos += 1;
            }
        };
        dec(self);
        let mut float = false;
        // A float needs digits on BOTH sides of the dot: `1.` is int then
        // member `.`, `1..2` a range, `1.e5` int-member. [gram.lex.number]
        if self.peek(0) == Some(b'.') && self.peek(1).is_some_and(|b| b.is_ascii_digit()) {
            self.pos += 1;
            dec(self);
            float = true;
        }
        if matches!(self.peek(0), Some(b'e' | b'E')) {
            let signed = matches!(self.peek(1), Some(b'+' | b'-'));
            let first = self.peek(if signed { 2 } else { 1 });
            if first.is_some_and(|b| b.is_ascii_digit()) {
                self.pos += if signed { 2 } else { 1 };
                dec(self);
                float = true;
            }
        }
        self.emit(
            if float {
                TokenKind::Float
            } else {
                TokenKind::Int
            },
            lo,
            self.pos,
        );
    }

    fn word(&mut self) {
        let lo = self.pos;
        if let Decoded::Char(_, n) = self.decode(self.pos) {
            self.pos += n;
        }
        while let Decoded::Char(c, n) = self.decode(self.pos) {
            if c == '_' || unicode_ident::is_xid_continue(c) {
                self.pos += n;
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.src[lo..self.pos]).unwrap_or("");
        // Raw string `r"…"` / `r#"…"#` — variable `#` fences.
        if text == "r" {
            let mut q = self.pos;
            let mut hashes = 0u32;
            while self.src.get(q) == Some(&b'#') {
                hashes += 1;
                q += 1;
            }
            if self.src.get(q) == Some(&b'"') {
                self.open_string(StrKind::Raw { hashes }, lo, q + 1);
                return;
            }
        }
        // Generalized literal: IDENT `"` with no whitespace between
        // (`re"x"` is one episode; `re "x"` is two tokens). The prefix is
        // any identifier that is not a reserved keyword. [gram.lex.str.gen]
        if self.src.get(self.pos) == Some(&b'"') && keyword(text).is_none() && text != "_" {
            self.open_string(StrKind::Generalized, lo, self.pos + 1);
            return;
        }
        let kind = if text == "_" {
            TokenKind::Underscore
        } else if let Some(k) = keyword(text) {
            TokenKind::Kw(k)
        } else {
            TokenKind::Ident
        };
        self.emit(kind, lo, self.pos);
    }

    /// Merge and report a run of junk: invalid UTF-8 (E0106, once per
    /// run) or stray characters (E0107).
    fn stray(&mut self) {
        let lo = self.pos;
        match self.decode(self.pos) {
            Decoded::Invalid(n) => {
                self.pos += n;
                while let Decoded::Invalid(n) = self.decode(self.pos) {
                    self.pos += n;
                }
                let span = self.span(lo, self.pos);
                self.diags.push(
                    Diagnostic::error(
                        codes::INVALID_UTF8,
                        span,
                        "this part of the file is not valid UTF-8",
                    )
                    .with_label("these bytes decode as no character")
                    .with_note(
                        "wolf source files are UTF-8 ([gram.lex.source]). Re-save the file \
                         as UTF-8; the bytes were skipped so the rest still lexes.",
                    ),
                );
                self.emit(TokenKind::Error, lo, self.pos);
            }
            Decoded::Char(first, n) => {
                self.pos += n;
                while let Decoded::Char(c, n) = self.decode(self.pos) {
                    if Self::is_stray(c) {
                        self.pos += n;
                    } else {
                        break;
                    }
                }
                let span = self.span(lo, self.pos);
                let d = if first == '\u{feff}' {
                    Diagnostic::error(
                        codes::STRAY_BYTE,
                        span,
                        "a byte order mark (BOM) does not belong in a wolf source file",
                    )
                    .with_label("the invisible BOM character sits here")
                    .with_note(
                        "wolf reads sources as plain UTF-8 and never uses a BOM \
                         ([gram.lex.source]). Save the file without one.",
                    )
                    .with_suggestion(Suggestion::new(
                        "delete the byte order mark",
                        vec![(span, String::new())],
                        Applicability::MachineApplicable,
                    ))
                } else {
                    Diagnostic::error(
                        codes::STRAY_BYTE,
                        span,
                        format!(
                            "the character `{}` is not part of wolf syntax",
                            first.escape_debug()
                        ),
                    )
                    .with_label("no wolf token can start with this")
                    .with_note(
                        "delete it — or if you cannot see anything here, an invisible \
                         character was pasted in; retype the line.",
                    )
                };
                self.diags.push(d);
                self.emit(TokenKind::Error, lo, self.pos);
            }
            Decoded::Eof => {}
        }
    }

    fn is_stray(c: char) -> bool {
        if c == '\u{feff}' {
            return true;
        }
        if c.is_ascii() {
            let b = c as u8;
            !(is_inline_ws(b)
                || b == b'\n'
                || b.is_ascii_alphanumeric()
                || matches!(b, b'_' | b'"' | b'#' | b';')
                || PUNCTS.iter().any(|(t, _)| t[0] == b))
        } else {
            !(unicode_ident::is_xid_start(c) || c.is_whitespace())
        }
    }

    // ------------------------------------------------------------ strings

    /// Open a string episode: emit `StrBegin` spanning `open_lo..content_lo`
    /// and push the frame (or an E0108 error token past [`MAX_NEST`]).
    fn open_string(&mut self, kind: StrKind, open_lo: usize, content_lo: usize) {
        if self.nest_depth() >= MAX_NEST {
            self.nest_diag(open_lo, content_lo);
            self.pos = content_lo;
            self.emit(TokenKind::Error, open_lo, content_lo);
            return;
        }
        self.pos = content_lo;
        self.emit(TokenKind::StrBegin(kind), open_lo, content_lo);
        self.frames.push(Frame::Str(StrFrame {
            kind,
            open_lo,
            content_lo,
        }));
    }

    fn open_quote_string(&mut self) {
        let lo = self.pos;
        if self.starts_with(self.pos, b"\"\"\"") {
            self.open_string(StrKind::Multiline, lo, lo + 3);
            if !matches!(self.frames.last(), Some(Frame::Str(_))) {
                return; // depth-limited; no frame pushed
            }
            // Content starts on the next line; anything else on the
            // opening line is E0103 (lexing continues, the bytes stay
            // ordinary content).
            let mut i = self.pos;
            while self.src.get(i).is_some_and(|&b| is_inline_ws(b)) {
                i += 1;
            }
            if self.src.get(i).is_some_and(|&b| b != b'\n') {
                let mut eol = i;
                while self.src.get(eol).is_some_and(|&b| b != b'\n') {
                    eol += 1;
                }
                let span = self.span(i, eol);
                let open_span = self.span(lo, lo + 3);
                self.diags.push(
                    Diagnostic::error(
                        codes::AFTER_OPENING_MULTILINE,
                        span,
                        "a multiline string's content starts on the line after the \
                         opening `\"\"\"`",
                    )
                    .with_label("move this text down to the next line")
                    .with_secondary(open_span, "the multiline string opens here")
                    .with_note(
                        "the opener must be the last thing on its line: the whitespace \
                         before the closing `\"\"\"` sets the margin stripped from every \
                         content line.",
                    ),
                );
            }
        } else {
            self.open_string(StrKind::Plain, lo, lo + 1);
        }
    }

    /// Fragment scanning for plain/multiline bodies (escapes + interp).
    fn body_step(&mut self, f: StrFrame) {
        match f.kind {
            StrKind::Plain | StrKind::Multiline => self.cooked_body(f),
            StrKind::Raw { hashes } => self.raw_body(hashes),
            StrKind::Generalized => self.gen_body(f),
        }
    }

    fn flush_frag(&mut self, start: usize) {
        if self.pos > start {
            let (lo, hi) = (start, self.pos);
            self.emit(TokenKind::StrFragment, lo, hi);
        }
    }

    fn cooked_body(&mut self, f: StrFrame) {
        let multi = f.kind == StrKind::Multiline;
        let start = self.pos;
        loop {
            let Some(&b) = self.src.get(self.pos) else {
                self.flush_frag(start);
                return; // EOF: finish() closes with E0102
            };
            match b {
                b'\\' => match self.escape_len() {
                    Ok(n) => self.pos += n,
                    Err((n, msg)) => {
                        self.flush_frag(start);
                        let lo = self.pos;
                        self.pos += n;
                        self.escape_diag(lo, self.pos, msg);
                        self.emit(TokenKind::Error, lo, self.pos);
                        return;
                    }
                },
                b'{' if self.peek(1) == Some(b'{') => self.pos += 2,
                b'}' if self.peek(1) == Some(b'}') => self.pos += 2,
                b'{' => {
                    self.flush_frag(start);
                    let lo = self.pos;
                    self.pos += 1;
                    if self.nest_depth() >= MAX_NEST {
                        self.nest_diag(lo, self.pos);
                        self.emit(TokenKind::Error, lo, self.pos);
                        return;
                    }
                    self.emit(TokenKind::InterpOpen, lo, self.pos);
                    self.frames.push(Frame::Interp { in_spec: false });
                    return;
                }
                b'}' => {
                    self.flush_frag(start);
                    let lo = self.pos;
                    self.pos += 1;
                    self.lone_brace_diag(lo);
                    self.emit(TokenKind::Error, lo, self.pos);
                    return;
                }
                b'"' if !multi => {
                    self.flush_frag(start);
                    let lo = self.pos;
                    self.pos += 1;
                    self.emit(TokenKind::StrEnd { dedent: 0 }, lo, self.pos);
                    self.frames.pop();
                    return;
                }
                b'"' if multi && self.starts_with(self.pos, b"\"\"\"") => {
                    self.flush_frag(start);
                    let lo = self.pos;
                    let dedent = self.close_multiline(&f, lo);
                    self.pos += 3;
                    self.emit(TokenKind::StrEnd { dedent }, lo, self.pos);
                    self.frames.pop();
                    return;
                }
                b'\n' if !multi => {
                    // Unterminated plain string: recover at end of line.
                    self.flush_frag(start);
                    self.unterminated_here(&f);
                    return;
                }
                _ => self.pos += 1,
            }
        }
    }

    /// The E0101 diagnostic: the site-specific message plus the shared
    /// list-of-escapes note.
    fn escape_diag(&mut self, lo: usize, hi: usize, msg: String) {
        let span = self.span(lo, hi);
        self.diags
            .push(Diagnostic::error(codes::INVALID_ESCAPE, span, msg).with_note(ESCAPE_NOTE));
    }

    /// The lone-`}` diagnostic (E0107), with its machine-applicable fix.
    fn lone_brace_diag(&mut self, lo: usize) {
        let span = self.span(lo, lo + 1);
        self.diags.push(
            Diagnostic::error(
                codes::STRAY_BYTE,
                span,
                "this `}` has no matching `{` in the string",
            )
            .with_label("a lone `}` would end an interpolation")
            .with_suggestion(Suggestion::new(
                "for a literal brace, write `}}`",
                vec![(span, "}}".to_string())],
                Applicability::MachineApplicable,
            )),
        );
    }

    /// Diagnose an unterminated plain string at the current position and
    /// close its episode with a zero-width `StrEnd` (protocol balance).
    fn unterminated_here(&mut self, f: &StrFrame) {
        let span = self.span(f.open_lo, self.pos);
        let at = self.pos;
        let insert_at = self.span(at, at);
        self.diags.push(
            Diagnostic::error(codes::UNTERMINATED_STRING, span, "this string never closes")
                .with_label("it opens here and runs to the end of the line")
                .with_note(
                    "a `\"…\"` string must close before the line ends. For text that \
                     spans lines, use a multiline `\"\"\"` string.",
                )
                .with_suggestion(Suggestion::new(
                    "close the string before the line ends",
                    vec![(insert_at, "\"".to_string())],
                    Applicability::Maybe,
                )),
        );
        self.emit(TokenKind::StrEnd { dedent: 0 }, at, at);
        self.frames.pop();
    }

    /// Escape length at `self.pos` (which holds `\`), or `(consumed, msg)`
    /// for an invalid escape. `[gram.lex.str.escape]`:
    /// `\n \t \r \\ \" \0 \xNN \u{1-6 hex}`.
    fn escape_len(&self) -> Result<usize, (usize, String)> {
        match self.peek(1) {
            Some(b'n' | b't' | b'r' | b'\\' | b'"' | b'0') => Ok(2),
            Some(b'x') => {
                let hex = |o: usize| self.peek(o).is_some_and(|b| b.is_ascii_hexdigit());
                if hex(2) && hex(3) {
                    Ok(4)
                } else {
                    let got = 2 + usize::from(hex(2));
                    Err((
                        got,
                        "a `\\x` escape needs exactly two hex digits, like `\\x7f`".into(),
                    ))
                }
            }
            Some(b'u') => {
                if self.peek(2) != Some(b'{') {
                    return Err((
                        2,
                        "a `\\u` escape is written `\\u{…}`, like `\\u{1f60a}`".into(),
                    ));
                }
                let mut i = 3;
                while self.peek(i).is_some_and(|b| b.is_ascii_hexdigit()) {
                    i += 1;
                }
                let digits = i - 3;
                if self.peek(i) == Some(b'}') && (1..=6).contains(&digits) {
                    Ok(i + 1)
                } else {
                    let consumed = i + usize::from(self.peek(i) == Some(b'}'));
                    Err((
                        consumed,
                        "a `\\u{…}` escape needs one to six hex digits inside the braces".into(),
                    ))
                }
            }
            Some(_) => match self.decode(self.pos + 1) {
                Decoded::Char(c, n) => Err((
                    1 + n,
                    format!("there is no `\\{}` escape in wolf", c.escape_debug()),
                )),
                _ => Err((1, "this `\\` escapes nothing wolf recognizes".into())),
            },
            None => Err((
                1,
                "this `\\` sits at the very end of the file with nothing to escape".into(),
            )),
        }
    }

    /// SE-0168 dedent: the exact whitespace before the closing `"""` is
    /// the margin every content line must start with. Returns the margin
    /// width in bytes (the `StrEnd` payload). Emits E0104/E0105.
    fn close_multiline(&mut self, f: &StrFrame, close_at: usize) -> u32 {
        let close_line_start = self.src[..close_at]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        if close_line_start <= f.content_lo {
            // Closing delimiter on the opening line — already E0103.
            return 0;
        }
        let margin: &[u8] = &self.src[close_line_start..close_at];
        if margin.iter().any(|&b| !is_inline_ws(b)) {
            let span = self.span(close_at, close_at + 3);
            self.diags.push(
                Diagnostic::error(
                    codes::UNDER_INDENTED,
                    span,
                    "the closing `\"\"\"` must stand alone on its line",
                )
                .with_label("only whitespace may sit before it")
                .with_note(
                    "the closing delimiter's column sets the margin stripped from every \
                     content line, so nothing else can share its line. Move it to its \
                     own line.",
                ),
            );
            return 0;
        }
        let margin = margin.to_vec();
        let margin_span_lo = close_line_start;
        // Content lines: each line start strictly between the opening
        // line and the closing line.
        let mut line_start = None::<usize>;
        for i in f.content_lo..close_line_start {
            if self.src[i] == b'\n' {
                line_start = Some(i + 1);
                continue;
            }
            let Some(ls) = line_start else { continue };
            // First byte of a content line reached: validate, then skip
            // the rest of the line.
            let mut eol = i;
            while eol < close_line_start && self.src[eol] != b'\n' {
                eol += 1;
            }
            let line = &self.src[ls..eol];
            let blank = line.iter().all(|&b| is_inline_ws(b));
            if !blank && !line.starts_with(&margin) {
                let lead = line.iter().take_while(|&&b| is_inline_ws(b)).count();
                let (code, msg, label, note) = if lead >= margin.len() {
                    (
                        codes::MARGIN_MISMATCH,
                        "this line's margin mixes tabs and spaces differently from the \
                         closing `\"\"\"`",
                        "the mismatch starts here",
                        "wolf compares the margin byte-for-byte, never by visual width. \
                         Re-indent this line with the same tab/space mix as the closing \
                         `\"\"\"`'s line.",
                    )
                } else {
                    (
                        codes::UNDER_INDENTED,
                        "this line starts left of the string's margin",
                        "the line starts here",
                        "the whitespace before the closing `\"\"\"` is the margin stripped \
                         from every content line. Indent this line at least that far, or \
                         move the closing `\"\"\"` left.",
                    )
                };
                let hi = ls + lead.max(1).min(line.len());
                let span = self.span(ls, hi);
                let note_span = if margin.is_empty() {
                    self.span(close_at, close_at + 3)
                } else {
                    self.span(margin_span_lo, close_at)
                };
                self.diags.push(
                    Diagnostic::error(code, span, msg)
                        .with_label(label)
                        .with_secondary(note_span, "the margin is set here")
                        .with_note(note),
                );
            }
            line_start = None; // handled this line
        }
        margin.len() as u32
    }

    fn raw_body(&mut self, hashes: u32) {
        let start = self.pos;
        loop {
            let Some(&b) = self.src.get(self.pos) else {
                self.flush_frag(start);
                return; // EOF: finish() closes with E0109
            };
            if b == b'"' {
                let fence_end = self.pos + 1 + hashes as usize;
                let closed = self
                    .src
                    .get(self.pos + 1..fence_end)
                    .is_some_and(|s| s.iter().all(|&c| c == b'#'));
                if closed {
                    self.flush_frag(start);
                    let lo = self.pos;
                    self.pos = fence_end;
                    self.emit(TokenKind::StrEnd { dedent: 0 }, lo, self.pos);
                    self.frames.pop();
                    return;
                }
            }
            self.pos += 1;
        }
    }

    fn gen_body(&mut self, f: StrFrame) {
        let start = self.pos;
        loop {
            match self.src.get(self.pos) {
                None | Some(b'\n') => {
                    self.flush_frag(start);
                    let at = self.pos;
                    let span = self.span(f.open_lo, at);
                    self.diags.push(
                        Diagnostic::error(
                            codes::UNTERMINATED_RAW,
                            span,
                            "this generalized literal never closes",
                        )
                        .with_label("it opens here and must close with `\"` before the line ends")
                        .with_suggestion(Suggestion::new(
                            "close the literal before the line ends",
                            vec![(self.span(at, at), "\"".to_string())],
                            Applicability::Maybe,
                        )),
                    );
                    self.emit(TokenKind::StrEnd { dedent: 0 }, at, at);
                    self.frames.pop();
                    return;
                }
                Some(b'"') => {
                    self.flush_frag(start);
                    let lo = self.pos;
                    self.pos += 1;
                    self.emit(TokenKind::StrEnd { dedent: 0 }, lo, self.pos);
                    self.frames.pop();
                    return;
                }
                Some(_) => self.pos += 1,
            }
        }
    }

    /// Format-spec text mode: fragments, `{{`/`}}` escapes, nested
    /// `{width}` interpolations, depth-0 `}` closes the interpolation.
    fn spec_step(&mut self) {
        let enclosing = self.frames.iter().rev().find_map(|fr| match fr {
            Frame::Str(f) => Some(*f),
            _ => None,
        });
        let multi = enclosing.is_some_and(|f| f.kind == StrKind::Multiline);
        let start = self.pos;
        loop {
            let Some(&b) = self.src.get(self.pos) else {
                self.flush_frag(start);
                return; // EOF: finish() drains
            };
            match b {
                b'{' if self.peek(1) == Some(b'{') => self.pos += 2,
                b'}' if self.peek(1) == Some(b'}') => self.pos += 2,
                b'{' => {
                    self.flush_frag(start);
                    let lo = self.pos;
                    self.pos += 1;
                    if self.nest_depth() >= MAX_NEST {
                        self.nest_diag(lo, self.pos);
                        self.emit(TokenKind::Error, lo, self.pos);
                        return;
                    }
                    self.emit(TokenKind::InterpOpen, lo, self.pos);
                    self.frames.push(Frame::Interp { in_spec: false });
                    return;
                }
                b'}' => {
                    self.flush_frag(start);
                    let lo = self.pos;
                    self.pos += 1;
                    self.frames.pop(); // the Interp { in_spec: true }
                    self.emit(TokenKind::InterpClose, lo, self.pos);
                    return;
                }
                b'\\' => match self.escape_len() {
                    Ok(n) => self.pos += n,
                    Err((n, msg)) => {
                        self.flush_frag(start);
                        let lo = self.pos;
                        self.pos += n;
                        self.escape_diag(lo, self.pos, msg);
                        self.emit(TokenKind::Error, lo, self.pos);
                        return;
                    }
                },
                b'\n' if !multi => {
                    // Unterminated plain-string format spec: recover at
                    // end of line, closing interp + string (balance).
                    self.flush_frag(start);
                    let at = self.pos;
                    let open_lo = enclosing.map_or(at, |f| f.open_lo);
                    let span = self.span(open_lo, at);
                    self.diags.push(
                        Diagnostic::error(
                            codes::UNTERMINATED_STRING,
                            span,
                            "this string never closes",
                        )
                        .with_label(
                            "it opens here, and its format spec is still open when the \
                             line ends",
                        )
                        .with_note(
                            "a `\"…\"` string — and any `{value:…}` format spec inside it — \
                             must close before the line ends.",
                        ),
                    );
                    self.frames.pop(); // Interp
                    self.emit(TokenKind::InterpClose, at, at);
                    if matches!(self.frames.last(), Some(Frame::Str(_))) {
                        self.frames.pop();
                        self.emit(TokenKind::StrEnd { dedent: 0 }, at, at);
                    }
                    return;
                }
                _ => self.pos += 1,
            }
        }
    }

    // -------------------------------------------------------------- close

    /// Drain the stack at EOF (zero-width closers + diagnostics), insert
    /// the end-of-file terminator if the last line qualifies, and emit
    /// the `Eof` completion marker.
    fn finish(&mut self) {
        let end = self.src.len();
        let mut rest: Vec<Frame> = Vec::new();
        while let Some(frame) = self.frames.pop() {
            match frame {
                Frame::Interp { .. } => self.emit(TokenKind::InterpClose, end, end),
                Frame::Str(f) => {
                    let span = self.span(f.open_lo, end);
                    let d = match f.kind {
                        StrKind::Raw { .. } => Diagnostic::error(
                            codes::UNTERMINATED_RAW,
                            span,
                            "this raw string never closes before the end of the file",
                        )
                        .with_label("it opens here")
                        .with_note(
                            "a raw string closes at `\"` followed by the same number of \
                             `#` as its opener — check that the closing fence matches.",
                        ),
                        StrKind::Generalized => Diagnostic::error(
                            codes::UNTERMINATED_RAW,
                            span,
                            "this generalized literal never closes",
                        )
                        .with_label("it opens here and must close with `\"`"),
                        _ => Diagnostic::error(
                            codes::UNTERMINATED_STRING,
                            span,
                            "this string is still open when the file ends",
                        )
                        .with_label("it opens here")
                        .with_note("add the closing delimiter."),
                    };
                    self.diags.push(d);
                    self.emit(TokenKind::StrEnd { dedent: 0 }, end, end);
                }
                other => rest.push(other),
            }
        }
        // EOF terminator: only for a final line that never saw a newline.
        let suppressed = matches!(rest.first(), Some(Frame::Paren | Frame::Bracket { .. }));
        if !self.line_broken
            && !self.attr_close
            && !suppressed
            && self.tokens.last().is_some_and(|t| t.kind.ends_statement())
        {
            self.emit(TokenKind::Term, end, end);
        }
        self.emit(TokenKind::Eof, end, end);
    }
}
