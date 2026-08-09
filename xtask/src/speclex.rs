//! THROWAWAY (s03 Target 5): delete when s08–s09 parser lands.
//!
//! Spec-faithful lexer for the grammar recognizer. Implements
//! `[gram.lex]`: keywords (loaded at runtime from spec/grammar.ebnf's
//! `reserved_kw` production so this file can never drift from the spec),
//! `[gram.lex.number]` (`1.` = int-then-member; `1..2` never a float),
//! the string mode stack `[gram.lex.str]` (interpolation re-entry with
//! STR_START/STR_MID/STR_END, `{{`/`}}`, format spec after the first
//! top-level `:`, `"""` closing-column dedent — lexical-only, raw
//! `r"…"`/`r#"…"#`, generalized `IDENT"…"`), and newline termination
//! exactly per `[gram.lex.newline]` (last-token classes; innermost
//! delimiter decides suppression; attribute-`]` exception; explicit `;`
//! with the empty-statement error E0002).

use std::collections::BTreeSet;

/// Token kinds for the recognizer. Text is kept only where the grammar
/// needs it (identifiers match contextual-keyword terminals by text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Ident(String),
    Kw(String),
    Int,
    Float,
    /// Complete string with no interpolation (plain or multiline).
    Str,
    RawStr,
    GenStr,
    StrStart,
    StrMid,
    StrEnd,
    Punct(&'static str),
    Underscore,
    Term,
}

impl Tok {
    /// `[gram.lex.newline]`: token classes a terminator is inserted after.
    fn ends_statement(&self) -> bool {
        match self {
            Tok::Ident(_) | Tok::Underscore => true,
            Tok::Int | Tok::Float => true,
            Tok::Str | Tok::RawStr | Tok::GenStr | Tok::StrEnd => true,
            Tok::Kw(k) => matches!(
                k.as_str(),
                "true" | "false" | "return" | "break" | "continue"
            ),
            Tok::Punct(p) => matches!(*p, ")" | "]" | "}" | "?"),
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct LexError {
    pub line: usize,
    pub msg: String,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

/// Extract the reserved-keyword set from the `reserved_kw ::=` production
/// of the extracted grammar (spec/grammar.ebnf) at runtime.
pub fn reserved_keywords(ebnf: &str) -> Result<BTreeSet<String>, String> {
    let mut kws = BTreeSet::new();
    let mut in_prod = false;
    for line in ebnf.lines() {
        if line.starts_with("reserved_kw") && line.contains("::=") {
            in_prod = true;
        } else if in_prod && (line.trim().is_empty() || line.contains("::=")) {
            break;
        }
        if in_prod {
            let mut rest = line;
            while let Some(start) = rest.find('\'') {
                let Some(len) = rest[start + 1..].find('\'') else {
                    break;
                };
                kws.insert(rest[start + 1..start + 1 + len].to_string());
                rest = &rest[start + 1 + len + 1..];
            }
        }
    }
    if kws.is_empty() {
        return Err("no reserved_kw production found in grammar.ebnf".into());
    }
    Ok(kws)
}

/// Delimiter kinds on the lexer's stack. The innermost decides newline
/// suppression (`[gram.lex.newline]` exceptions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Delim {
    Paren,
    Bracket { attr: bool },
    Brace,
    Interp,
}

pub fn lex(src: &str, kws: &BTreeSet<String>) -> Result<Vec<Tok>, LexError> {
    let mut lx = Lexer {
        chars: src.chars().collect(),
        pos: 0,
        line: 1,
        kws,
        toks: Vec::new(),
        delims: Vec::new(),
        last_attr_close: false,
        str_depth: 0,
    };
    lx.run()?;
    Ok(lx.toks)
}

struct Lexer<'a> {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    kws: &'a BTreeSet<String>,
    toks: Vec<Tok>,
    delims: Vec<Delim>,
    /// Set when the last emitted token is the `]` of an attribute.
    last_attr_close: bool,
    str_depth: usize,
}

const PUNCTS: &[&str] = &[
    "<<=", ">>=", "<=>", "..=", "->", "=>", "==", "!=", "<=", ">=", "&&", "||", "<<", ">>", "+=",
    "-=", "*=", "/=", "%=", "&=", "|=", "^=", "..", "(", ")", "[", "]", "{", "}", ",", ".", ":",
    "=", "+", "-", "*", "/", "%", "&", "|", "^", "<", ">", "!", "?", "@",
];

impl Lexer<'_> {
    fn err(&self, msg: impl Into<String>) -> LexError {
        LexError {
            line: self.line,
            msg: msg.into(),
        }
    }

    fn peek(&self, off: usize) -> Option<char> {
        self.chars.get(self.pos + off).copied()
    }

    fn starts_with(&self, s: &str) -> bool {
        s.chars().enumerate().all(|(i, c)| self.peek(i) == Some(c))
    }

    fn emit(&mut self, t: Tok) {
        self.last_attr_close = false;
        self.toks.push(t);
    }

    fn run(&mut self) -> Result<(), LexError> {
        while self.pos < self.chars.len() {
            self.scan_one()?;
        }
        if !self.delims.is_empty() {
            return Err(self.err("unclosed delimiter at end of file"));
        }
        Ok(())
    }

    /// Insert a TERM at a newline iff the innermost delimiter allows it
    /// and the last token is in the terminating class.
    fn maybe_term(&mut self) {
        let suppressed = matches!(
            self.delims.last(),
            Some(Delim::Paren) | Some(Delim::Bracket { .. }) | Some(Delim::Interp)
        );
        if suppressed || self.last_attr_close {
            return;
        }
        if self.toks.last().is_some_and(Tok::ends_statement) {
            self.toks.push(Tok::Term);
        }
    }

    /// Scan one lexeme (or trivia). In interpolation mode the caller loop
    /// is the string scanner; the shared delimiter stack routes `}` / `:`.
    fn scan_one(&mut self) -> Result<(), LexError> {
        let c = self.chars[self.pos];
        match c {
            '\n' => {
                self.maybe_term();
                self.line += 1;
                self.pos += 1;
            }
            c if c.is_whitespace() => self.pos += 1,
            '/' if self.peek(1) == Some('/') => {
                while self.pos < self.chars.len() && self.chars[self.pos] != '\n' {
                    self.pos += 1;
                }
            }
            ';' => {
                let empty = matches!(
                    self.toks.last(),
                    None | Some(Tok::Term) | Some(Tok::Punct("{"))
                );
                if empty {
                    return Err(self.err("empty statement (E0002)"));
                }
                self.pos += 1;
                self.emit(Tok::Term);
            }
            '"' => self.scan_string()?,
            '#' if self.peek(1) == Some('[') => {
                self.pos += 2;
                self.delims.push(Delim::Bracket { attr: true });
                self.emit(Tok::Punct("#["));
            }
            '#' => return Err(self.err("stray `#`")),
            c if c.is_ascii_digit() => self.scan_number(),
            c if c == '_' || unicode_ident::is_xid_start(c) => self.scan_word()?,
            _ => self.scan_punct()?,
        }
        Ok(())
    }

    fn scan_punct(&mut self) -> Result<(), LexError> {
        let op = PUNCTS
            .iter()
            .find(|p| self.starts_with(p))
            .copied()
            .ok_or_else(|| self.err(format!("unexpected character `{}`", self.chars[self.pos])))?;
        self.pos += op.len();
        match op {
            "(" => self.delims.push(Delim::Paren),
            "[" => self.delims.push(Delim::Bracket { attr: false }),
            "{" => self.delims.push(Delim::Brace),
            ")" => {
                if self.delims.pop() != Some(Delim::Paren) {
                    return Err(self.err("unmatched `)`"));
                }
            }
            "]" => match self.delims.pop() {
                Some(Delim::Bracket { attr }) => {
                    self.emit(Tok::Punct("]"));
                    self.last_attr_close = attr;
                    return Ok(());
                }
                _ => return Err(self.err("unmatched `]`")),
            },
            "}" => match self.delims.pop() {
                Some(Delim::Brace) => {}
                // Closes the enclosing interpolation: no token; the
                // string scanner resumes.
                Some(Delim::Interp) => return Ok(()),
                _ => return Err(self.err("unmatched `}`")),
            },
            ":" if self.delims.last() == Some(&Delim::Interp) => {
                // First top-level `:` in an interpolation: format spec,
                // consumed lexically (nested `{…}` groups allowed).
                self.scan_format_spec()?;
                self.delims.pop();
                return Ok(());
            }
            _ => {}
        }
        self.emit(Tok::Punct(op));
        Ok(())
    }

    /// Consume a format spec up to (and including) the `}` that closes
    /// the interpolation. `[gram.amb.fmtcolon]`
    fn scan_format_spec(&mut self) -> Result<(), LexError> {
        let mut depth = 0usize;
        while let Some(c) = self.peek(0) {
            self.pos += 1;
            match c {
                '{' => depth += 1,
                '}' if depth == 0 => return Ok(()),
                '}' => depth -= 1,
                '\n' => return Err(self.err("newline in format spec")),
                _ => {}
            }
        }
        Err(self.err("unterminated format spec"))
    }

    fn scan_number(&mut self) {
        let digits = |lx: &mut Self, radix: u32| {
            while lx.peek(0).is_some_and(|c| c == '_' || c.is_digit(radix)) {
                lx.pos += 1;
            }
        };
        if self.starts_with("0x") || self.starts_with("0o") || self.starts_with("0b") {
            let radix = match self.peek(1) {
                Some('x') => 16,
                Some('o') => 8,
                _ => 2,
            };
            self.pos += 2;
            digits(self, radix);
            self.emit(Tok::Int);
            return;
        }
        digits(self, 10);
        let mut float = false;
        // A float needs digits on BOTH sides of the dot: `1.` is an int
        // followed by member `.`; `1..2` is a range. [gram.lex.number]
        if self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
            digits(self, 10);
            float = true;
        }
        if matches!(self.peek(0), Some('e') | Some('E')) {
            let sign = matches!(self.peek(1), Some('+') | Some('-'));
            let exp_digit = self.peek(if sign { 2 } else { 1 });
            if exp_digit.is_some_and(|c| c.is_ascii_digit()) {
                self.pos += if sign { 2 } else { 1 };
                digits(self, 10);
                float = true;
            }
        }
        self.emit(if float { Tok::Float } else { Tok::Int });
    }

    fn scan_word(&mut self) -> Result<(), LexError> {
        let start = self.pos;
        self.pos += 1;
        while self
            .peek(0)
            .is_some_and(|c| c == '_' || unicode_ident::is_xid_continue(c))
        {
            self.pos += 1;
        }
        let word: String = self.chars[start..self.pos].iter().collect();
        // Raw string r"…" / r#"…"#.
        if word == "r" && matches!(self.peek(0), Some('"') | Some('#')) {
            return self.scan_raw_string();
        }
        // Generalized literal IDENT"…" (no whitespace, raw-mode body).
        if self.peek(0) == Some('"') && !self.kws.contains(&word) && word != "_" {
            self.pos += 1;
            while let Some(c) = self.peek(0) {
                self.pos += 1;
                if c == '"' {
                    self.emit(Tok::GenStr);
                    return Ok(());
                }
                if c == '\n' {
                    break;
                }
            }
            return Err(self.err("unterminated generalized literal"));
        }
        if word == "_" {
            self.emit(Tok::Underscore);
        } else if self.kws.contains(&word) {
            self.emit(Tok::Kw(word));
        } else {
            self.emit(Tok::Ident(word));
        }
        Ok(())
    }

    fn scan_raw_string(&mut self) -> Result<(), LexError> {
        let mut fence = 0usize;
        while self.peek(0) == Some('#') {
            fence += 1;
            self.pos += 1;
        }
        if self.peek(0) != Some('"') {
            return Err(self.err("malformed raw string"));
        }
        self.pos += 1;
        while self.pos < self.chars.len() {
            if self.chars[self.pos] == '\n' {
                self.line += 1;
            }
            if self.chars[self.pos] == '"' {
                let closed = (1..=fence).all(|i| self.peek(i) == Some('#'));
                if closed {
                    self.pos += 1 + fence;
                    self.emit(Tok::RawStr);
                    return Ok(());
                }
            }
            self.pos += 1;
        }
        Err(self.err("unterminated raw string"))
    }

    /// Plain and multiline strings — the mode stack. `[gram.lex.str]`
    fn scan_string(&mut self) -> Result<(), LexError> {
        self.str_depth += 1;
        if self.str_depth > 8 {
            return Err(self.err("string interpolation nesting deeper than 8 (E0007)"));
        }
        let multi = self.starts_with("\"\"\"");
        self.pos += if multi { 3 } else { 1 };
        if multi && self.peek(0) == Some('\n') {
            // First newline after the opening `"""` is dropped.
            self.pos += 1;
            self.line += 1;
        }
        let mut line_starts: Vec<usize> = vec![self.pos];
        let mut any_interp = false;
        loop {
            let Some(c) = self.peek(0) else {
                return Err(self.err("unterminated string literal"));
            };
            match c {
                '\\' => self.scan_escape()?,
                '\n' if !multi => return Err(self.err("newline in single-line string")),
                '\n' => {
                    self.pos += 1;
                    self.line += 1;
                    line_starts.push(self.pos);
                }
                '"' if !multi => {
                    self.pos += 1;
                    self.emit(if any_interp { Tok::StrEnd } else { Tok::Str });
                    self.str_depth -= 1;
                    return Ok(());
                }
                '"' if self.starts_with("\"\"\"") => {
                    let close_at = self.pos;
                    self.pos += 3;
                    self.check_dedent(&line_starts, close_at)?;
                    self.emit(if any_interp { Tok::StrEnd } else { Tok::Str });
                    self.str_depth -= 1;
                    return Ok(());
                }
                '"' => self.pos += 1, // lone `"` inside `"""` body
                '{' if self.peek(1) == Some('{') => self.pos += 2,
                '}' if self.peek(1) == Some('}') => self.pos += 2,
                '}' => return Err(self.err("lone `}` in string (write `}}`)")),
                '{' => {
                    self.pos += 1;
                    self.emit(if any_interp {
                        Tok::StrMid
                    } else {
                        Tok::StrStart
                    });
                    any_interp = true;
                    // Re-enter normal token mode until this interpolation
                    // closes (its frame is popped by `}` or a format spec).
                    let depth = self.delims.len();
                    self.delims.push(Delim::Interp);
                    while self.delims.len() > depth {
                        if self.pos >= self.chars.len() {
                            return Err(self.err("unterminated interpolation"));
                        }
                        self.scan_one()?;
                    }
                }
                _ => self.pos += 1,
            }
        }
    }

    fn scan_escape(&mut self) -> Result<(), LexError> {
        match self.peek(1) {
            Some('n' | 't' | 'r' | '\\' | '"' | '0') => {
                self.pos += 2;
                Ok(())
            }
            Some('x') => {
                self.pos += 4;
                Ok(())
            }
            Some('u') if self.peek(2) == Some('{') => {
                self.pos += 3;
                while self.peek(0).is_some_and(|c| c != '}') {
                    self.pos += 1;
                }
                self.pos += 1;
                Ok(())
            }
            other => Err(self.err(format!("unknown escape `\\{}`", other.unwrap_or(' ')))),
        }
    }

    /// `[gram.lex.str.multi]`: the closing delimiter's column sets the
    /// dedent; every content line must start with at least that much
    /// whitespace. Lexical-only validation.
    fn check_dedent(&self, line_starts: &[usize], close_at: usize) -> Result<(), LexError> {
        let close_line_start = line_starts
            .iter()
            .rev()
            .find(|&&s| s <= close_at)
            .copied()
            .unwrap_or(0);
        let col = close_at - close_line_start;
        if self.chars[close_line_start..close_at]
            .iter()
            .any(|c| !c.is_whitespace())
        {
            return Err(self.err("closing `\"\"\"` must be alone on its line"));
        }
        for &s in line_starts {
            if s == close_line_start {
                continue;
            }
            let line: Vec<char> = self.chars[s..]
                .iter()
                .take_while(|&&c| c != '\n')
                .copied()
                .collect();
            if line.iter().all(|c| c.is_whitespace()) {
                continue; // blank lines are exempt
            }
            if line.len() < col || line[..col].iter().any(|c| !c.is_whitespace()) {
                return Err(self.err("multiline string under-indented for closing `\"\"\"` dedent"));
            }
        }
        Ok(())
    }
}

// Minimal XID checks without a dependency: ASCII fast path, and accept
// any non-ASCII alphabetic (the corpus is ASCII; this is throwaway).
mod unicode_ident {
    pub fn is_xid_start(c: char) -> bool {
        c.is_ascii_alphabetic() || (!c.is_ascii() && c.is_alphabetic())
    }
    pub fn is_xid_continue(c: char) -> bool {
        c.is_ascii_alphanumeric() || (!c.is_ascii() && c.is_alphanumeric())
    }
}
