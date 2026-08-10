//! The textual-format parser. Together with `print.rs` it closes the
//! round-trip loop: `print → parse → print` reaches a byte-identical
//! fixpoint. The parser accepts hand-written WIR (symbolic value and
//! block names, decimal float constants); the printer normalizes
//! everything to canonical form on the way back out.
//!
//! Structure per function is three passes over its lines:
//! A. block headers — every block and every (typed) block parameter
//!    exists before any instruction parses;
//! B. instructions in order — instruction operands must be defined
//!    textually earlier (block params count as defined), which is
//!    exactly the shape canonical RPO output has;
//! C. fact lines — facts may cite any value in the function, so they
//!    resolve after the whole body is known.
//!
//! Grammar: `wolf_wir/text.md`.

use crate::entity::EntityRef;
use crate::facts::{DerefSize, FactData, FactKind, Just, Theorem};
use crate::ir::{Aux, Block, Function, Mode, Module, Param, SigId, Value};
use crate::ops::{FloatCc, IntCc, Opcode};
use crate::types::{self, RegionId, TypeData, TypeId};
use std::collections::HashMap;

/// A deterministic parse error with a position in the input text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line: u32,
    pub col: u32,
    pub msg: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "wir parse error: line {}, col {}: {}",
            self.line, self.col, self.msg
        )
    }
}

impl std::error::Error for ParseError {}

type PResult<T> = Result<T, ParseError>;

fn err<T>(line: u32, col: u32, msg: impl Into<String>) -> PResult<T> {
    Err(ParseError {
        line,
        col,
        msg: msg.into(),
    })
}

// ------------------------------------------------------------ tokens ----

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    /// Identifier; may contain interior dots (`iadd.chk`, `mem.r0`).
    Ident(String),
    /// `%name`.
    Val(String),
    /// `@name`.
    Global(String),
    /// Number lexeme, verbatim (`42`, `-7`, `0x3ff0000000000000`, `1.5`).
    Num(String),
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Eq,
    Arrow,
    DotDotEq,
    Newline,
}

impl Tok {
    fn describe(&self) -> String {
        match self {
            Tok::Ident(s) => format!("`{s}`"),
            Tok::Val(s) => format!("`%{s}`"),
            Tok::Global(s) => format!("`@{s}`"),
            Tok::Num(s) => format!("`{s}`"),
            Tok::LParen => "`(`".into(),
            Tok::RParen => "`)`".into(),
            Tok::LBrace => "`{`".into(),
            Tok::RBrace => "`}`".into(),
            Tok::Comma => "`,`".into(),
            Tok::Colon => "`:`".into(),
            Tok::Eq => "`=`".into(),
            Tok::Arrow => "`->`".into(),
            Tok::DotDotEq => "`..=`".into(),
            Tok::Newline => "end of line".into(),
        }
    }
}

type Spanned = (Tok, u32, u32);

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_cont(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn tokenize(src: &str) -> PResult<Vec<Spanned>> {
    let mut toks = Vec::new();
    let bytes: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut line = 1u32;
    let mut col = 1u32;
    macro_rules! push {
        ($t:expr, $c:expr) => {
            toks.push(($t, line, $c))
        };
    }
    while i < bytes.len() {
        let c = bytes[i];
        let start_col = col;
        match c {
            ' ' | '\t' | '\r' => {
                i += 1;
                col += 1;
            }
            '\n' => {
                push!(Tok::Newline, start_col);
                i += 1;
                line += 1;
                col = 1;
            }
            ';' => {
                while i < bytes.len() && bytes[i] != '\n' {
                    i += 1;
                    col += 1;
                }
            }
            '(' => {
                push!(Tok::LParen, start_col);
                i += 1;
                col += 1;
            }
            ')' => {
                push!(Tok::RParen, start_col);
                i += 1;
                col += 1;
            }
            '{' => {
                push!(Tok::LBrace, start_col);
                i += 1;
                col += 1;
            }
            '}' => {
                push!(Tok::RBrace, start_col);
                i += 1;
                col += 1;
            }
            ',' => {
                push!(Tok::Comma, start_col);
                i += 1;
                col += 1;
            }
            ':' => {
                push!(Tok::Colon, start_col);
                i += 1;
                col += 1;
            }
            '=' => {
                push!(Tok::Eq, start_col);
                i += 1;
                col += 1;
            }
            '.' => {
                if bytes.get(i + 1) == Some(&'.') && bytes.get(i + 2) == Some(&'=') {
                    push!(Tok::DotDotEq, start_col);
                    i += 3;
                    col += 3;
                } else {
                    return err(line, start_col, "unexpected `.`");
                }
            }
            '-' => {
                if bytes.get(i + 1) == Some(&'>') {
                    push!(Tok::Arrow, start_col);
                    i += 2;
                    col += 2;
                } else if bytes.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
                    let (lex, n) = lex_number(&bytes[i..]);
                    i += n;
                    col += n as u32;
                    push!(Tok::Num(lex), start_col);
                } else {
                    return err(line, start_col, "unexpected `-`");
                }
            }
            '%' | '@' => {
                let mut j = i + 1;
                while j < bytes.len() && is_ident_cont(bytes[j]) {
                    j += 1;
                }
                if j == i + 1 {
                    return err(line, start_col, format!("`{c}` needs a name after it"));
                }
                let name: String = bytes[i + 1..j].iter().collect();
                let n = (j - i) as u32;
                if c == '%' {
                    push!(Tok::Val(name), start_col);
                } else {
                    push!(Tok::Global(name), start_col);
                }
                i = j;
                col += n;
            }
            c if c.is_ascii_digit() => {
                let (lex, n) = lex_number(&bytes[i..]);
                i += n;
                col += n as u32;
                push!(Tok::Num(lex), start_col);
            }
            c if is_ident_start(c) => {
                let mut j = i;
                loop {
                    j += 1;
                    while j < bytes.len() && is_ident_cont(bytes[j]) {
                        j += 1;
                    }
                    // Dotted continuation: `.` followed by an ident start.
                    if j + 1 < bytes.len()
                        && bytes[j] == '.'
                        && (is_ident_start(bytes[j + 1]) || bytes[j + 1].is_ascii_digit())
                    {
                        j += 1;
                        continue;
                    }
                    break;
                }
                let name: String = bytes[i..j].iter().collect();
                let n = (j - i) as u32;
                push!(Tok::Ident(name), start_col);
                i = j;
                col += n;
            }
            other => {
                return err(line, start_col, format!("unexpected character `{other}`"));
            }
        }
    }
    toks.push((Tok::Newline, line, col));
    Ok(toks)
}

/// Lex a number starting at `chars[0]` (an ASCII digit or `-`).
/// Returns (lexeme, chars consumed). `0..` stops before the dots;
/// `8x%n` stops before the `x`.
fn lex_number(chars: &[char]) -> (String, usize) {
    let mut j = 0usize;
    if chars[0] == '-' {
        j = 1;
    }
    if chars.get(j) == Some(&'0') && chars.get(j + 1) == Some(&'x') {
        j += 2;
        while j < chars.len() && chars[j].is_ascii_hexdigit() {
            j += 1;
        }
    } else {
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
        // Fractional part, but not the `..=` of a range.
        if chars.get(j) == Some(&'.') && chars.get(j + 1).is_some_and(|c| c.is_ascii_digit()) {
            j += 1;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
        }
    }
    (chars[..j].iter().collect(), j)
}

// ------------------------------------------------------- line cursor ----

/// A cursor over one line's tokens (the trailing Newline excluded).
struct Line<'a> {
    toks: &'a [Spanned],
    i: usize,
}

impl<'a> Line<'a> {
    fn new(toks: &'a [Spanned]) -> Line<'a> {
        Line { toks, i: 0 }
    }

    fn pos(&self) -> (u32, u32) {
        match self.toks.get(self.i).or_else(|| self.toks.last()) {
            Some(&(_, l, c)) => (l, c),
            None => (0, 0),
        }
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|(t, _, _)| t)
    }

    fn next(&mut self) -> Option<&Tok> {
        let t = self.toks.get(self.i).map(|(t, _, _)| t);
        if t.is_some() {
            self.i += 1;
        }
        t
    }

    fn at_end(&self) -> bool {
        self.i >= self.toks.len()
    }

    fn fail<T>(&self, msg: impl Into<String>) -> PResult<T> {
        let (l, c) = self.pos();
        err(l, c, msg)
    }

    fn expect(&mut self, want: Tok) -> PResult<()> {
        match self.peek() {
            Some(t) if *t == want => {
                self.i += 1;
                Ok(())
            }
            Some(t) => {
                let d = t.describe();
                self.fail(format!("expected {}, found {d}", want.describe()))
            }
            None => self.fail(format!("expected {} at end of line", want.describe())),
        }
    }

    fn expect_end(&self) -> PResult<()> {
        if self.at_end() {
            Ok(())
        } else {
            self.fail("unexpected trailing tokens")
        }
    }

    fn ident(&mut self) -> PResult<String> {
        match self.peek() {
            Some(Tok::Ident(s)) => {
                let s = s.clone();
                self.i += 1;
                Ok(s)
            }
            Some(t) => {
                let d = t.describe();
                self.fail(format!("expected an identifier, found {d}"))
            }
            None => self.fail("expected an identifier at end of line"),
        }
    }

    fn val_name(&mut self) -> PResult<String> {
        match self.peek() {
            Some(Tok::Val(s)) => {
                let s = s.clone();
                self.i += 1;
                Ok(s)
            }
            Some(t) => {
                let d = t.describe();
                self.fail(format!("expected a value (`%name`), found {d}"))
            }
            None => self.fail("expected a value at end of line"),
        }
    }

    fn num(&mut self) -> PResult<String> {
        match self.peek() {
            Some(Tok::Num(s)) => {
                let s = s.clone();
                self.i += 1;
                Ok(s)
            }
            Some(t) => {
                let d = t.describe();
                self.fail(format!("expected a number, found {d}"))
            }
            None => self.fail("expected a number at end of line"),
        }
    }
}

fn parse_i64(line: &Line, lex: &str) -> PResult<i64> {
    let (neg, body) = match lex.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, lex),
    };
    let mag: i128 = if let Some(hex) = body.strip_prefix("0x") {
        match u64::from_str_radix(hex, 16) {
            Ok(u) => {
                if neg {
                    return line.fail("negative hex integers are not supported");
                }
                // Hex is a bit pattern: 0xffffffffffffffff is -1.
                return Ok(u as i64);
            }
            Err(_) => return line.fail(format!("bad hex integer `{lex}`")),
        }
    } else {
        match body.parse::<i128>() {
            Ok(v) => v,
            Err(_) => return line.fail(format!("bad integer `{lex}`")),
        }
    };
    let v = if neg { -mag } else { mag };
    i64::try_from(v).map_err(|_| ParseError {
        line: line.pos().0,
        col: line.pos().1,
        msg: format!("integer `{lex}` does not fit in 64 bits"),
    })
}

fn parse_i128(line: &Line, lex: &str) -> PResult<i128> {
    lex.parse::<i128>().map_err(|_| {
        let (l, c) = line.pos();
        ParseError {
            line: l,
            col: c,
            msg: format!("bad integer `{lex}`"),
        }
    })
}

fn parse_u64(line: &Line, lex: &str) -> PResult<u64> {
    let r = if let Some(hex) = lex.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)
    } else {
        lex.parse::<u64>()
    };
    r.map_err(|_| {
        let (l, c) = line.pos();
        ParseError {
            line: l,
            col: c,
            msg: format!("bad unsigned integer `{lex}`"),
        }
    })
}

// ------------------------------------------------------------ parser ----

struct Parser {
    module: Module,
    /// Signatures visible to `call` sites, by name (decls + parsed fns).
    known_sigs: HashMap<String, SigId>,
    fn_names: HashMap<String, ()>,
}

/// Parse a whole module. Deterministic errors with line/col positions.
pub fn parse_module(src: &str) -> Result<Module, ParseError> {
    let toks = tokenize(src)?;
    let mut p = Parser {
        module: Module::new(),
        known_sigs: HashMap::new(),
        fn_names: HashMap::new(),
    };
    let mut i = 0usize;
    while i < toks.len() {
        match &toks[i].0 {
            Tok::Newline => {
                i += 1;
            }
            Tok::Ident(kw) if kw == "decl" => {
                let end = line_end(&toks, i);
                let mut line = Line::new(&toks[i..end]);
                line.next(); // `decl`
                let name = match line.peek() {
                    Some(Tok::Global(n)) => {
                        let n = n.clone();
                        line.next();
                        n
                    }
                    _ => return line.fail("expected `@name` after `decl`"),
                };
                let sig = parse_sig(&mut line, &mut p.module)?;
                line.expect_end()?;
                if p.known_sigs.contains_key(&name) {
                    return line.fail(format!("`@{name}` is declared twice"));
                }
                p.known_sigs.insert(name.clone(), sig);
                p.module.add_decl(name, sig);
                i = end;
            }
            Tok::Ident(kw) if kw == "fn" => {
                i = parse_function(&toks, i, &mut p)?;
            }
            t => {
                let (l, c) = (toks[i].1, toks[i].2);
                return err(
                    l,
                    c,
                    format!("expected `decl` or `fn`, found {}", t.describe()),
                );
            }
        }
    }
    Ok(p.module)
}

/// Index just past this line's tokens (the Newline is excluded from the
/// line and skipped by the caller via the returned index).
fn line_end(toks: &[Spanned], start: usize) -> usize {
    let mut i = start;
    while i < toks.len() && toks[i].0 != Tok::Newline {
        i += 1;
    }
    i
}

/// `(mut ptr, i64) -> f64, i64` — params (with modes) and results.
fn parse_sig(line: &mut Line, module: &mut Module) -> PResult<SigId> {
    line.expect(Tok::LParen)?;
    let mut params = Vec::new();
    if line.peek() != Some(&Tok::RParen) {
        loop {
            let mode = match line.peek() {
                Some(Tok::Ident(kw)) if kw == "mut" => {
                    line.next();
                    Mode::Mut
                }
                Some(Tok::Ident(kw)) if kw == "read" => {
                    line.next();
                    Mode::Read
                }
                Some(Tok::Ident(kw)) if kw == "take" => {
                    line.next();
                    Mode::Take
                }
                _ => Mode::Val,
            };
            let ty = parse_type(line, module)?;
            params.push(Param { ty, mode });
            if line.peek() == Some(&Tok::Comma) {
                line.next();
            } else {
                break;
            }
        }
    }
    line.expect(Tok::RParen)?;
    let mut results = Vec::new();
    if line.peek() == Some(&Tok::Arrow) {
        line.next();
        loop {
            results.push(parse_type(line, module)?);
            if line.peek() == Some(&Tok::Comma) {
                line.next();
            } else {
                break;
            }
        }
    }
    Ok(module.make_sig(params, results))
}

fn parse_type(line: &mut Line, module: &mut Module) -> PResult<TypeId> {
    match line.peek() {
        Some(Tok::LBrace) => {
            line.next();
            let mut fields = Vec::new();
            if line.peek() != Some(&Tok::RBrace) {
                loop {
                    fields.push(parse_type(line, module)?);
                    if line.peek() == Some(&Tok::Comma) {
                        line.next();
                    } else {
                        break;
                    }
                }
            }
            line.expect(Tok::RBrace)?;
            Ok(module.types.intern(TypeData::Agg(fields)))
        }
        Some(Tok::Ident(_)) => {
            let name = line.ident()?;
            scalar_type(&name)
                .map(Ok)
                .unwrap_or_else(|| match mem_type(&name) {
                    Some(r) => Ok(module.types.mem(r)),
                    None => line.fail(format!("unknown type `{name}`")),
                })
        }
        _ => line.fail("expected a type"),
    }
}

fn scalar_type(name: &str) -> Option<TypeId> {
    match name {
        "i8" => Some(types::I8),
        "i16" => Some(types::I16),
        "i32" => Some(types::I32),
        "i64" => Some(types::I64),
        "f32" => Some(types::F32),
        "f64" => Some(types::F64),
        "bool" => Some(types::BOOL),
        "ptr" => Some(types::PTR),
        "io" => Some(types::IO),
        _ => None,
    }
}

fn mem_type(name: &str) -> Option<RegionId> {
    let idx = name.strip_prefix("mem.r")?;
    idx.parse::<u32>().ok().map(RegionId::new)
}

// -------------------------------------------------- function parsing ----

enum LineKind {
    Header(Block),
    Fact,
    Inst,
}

fn parse_function(toks: &[Spanned], start: usize, p: &mut Parser) -> PResult<usize> {
    // Header line: `fn @name(SIG) [-> ...] {`.
    let head_end = line_end(toks, start);
    let mut head = Line::new(&toks[start..head_end]);
    head.next(); // `fn`
    let name = match head.peek() {
        Some(Tok::Global(n)) => {
            let n = n.clone();
            head.next();
            n
        }
        _ => return head.fail("expected `@name` after `fn`"),
    };
    if p.fn_names.contains_key(&name) {
        return head.fail(format!("`@{name}` is defined twice"));
    }
    // The signature ends at the trailing `{`.
    if head.toks.last().map(|(t, _, _)| t) != Some(&Tok::LBrace) {
        return head.fail("function header must end with `{`");
    }
    let sig_toks = &head.toks[head.i..head.toks.len() - 1];
    let mut sig_line = Line::new(sig_toks);
    let sig = parse_sig(&mut sig_line, &mut p.module)?;
    sig_line.expect_end()?;
    if let Some(&prev) = p.known_sigs.get(&name)
        && p.module.sigs[prev] != p.module.sigs[sig]
    {
        return head.fail(format!("conflicting signature for `@{name}`"));
    }

    // Collect body lines until the closing `}` at aggregate-brace depth 0.
    let mut lines: Vec<&[Spanned]> = Vec::new();
    let mut i = head_end + 1; // past the Newline after `{`
    let mut close = None;
    let mut cur_start = i;
    let mut depth = 0i32;
    while i < toks.len() {
        match &toks[i].0 {
            Tok::Newline => {
                if cur_start < i {
                    lines.push(&toks[cur_start..i]);
                }
                cur_start = i + 1;
                i += 1;
            }
            Tok::LBrace => {
                depth += 1;
                i += 1;
            }
            Tok::RBrace => {
                if depth == 0 {
                    if cur_start < i {
                        let (l, c) = (toks[i].1, toks[i].2);
                        return err(l, c, "`}` must be on its own line");
                    }
                    close = Some(i);
                    break;
                }
                depth -= 1;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    let Some(close) = close else {
        let (l, c) = toks.last().map(|&(_, l, c)| (l, c)).unwrap_or((0, 0));
        return err(l, c, format!("`@{name}`: missing closing `}}`"));
    };

    let mut func = Function::new(name.clone(), sig);
    let mut blocks: HashMap<String, Block> = HashMap::new();
    let mut values: HashMap<String, Value> = HashMap::new();

    // Pass A: block headers (a line whose last token is `:` at depth 0).
    let mut kinds: Vec<LineKind> = Vec::new();
    for lt in &lines {
        let is_header = matches!(lt.last(), Some((Tok::Colon, _, _)))
            && matches!(lt.first(), Some((Tok::Ident(_), _, _)));
        if !is_header {
            let is_fact = matches!(lt.first(), Some((Tok::Ident(k), _, _)) if k == "fact");
            kinds.push(if is_fact {
                LineKind::Fact
            } else {
                LineKind::Inst
            });
            continue;
        }
        let mut line = Line::new(lt);
        let label = line.ident()?;
        if blocks.contains_key(&label) {
            return line.fail(format!("block `{label}` is defined twice"));
        }
        let mut param_tys: Vec<TypeId> = Vec::new();
        let mut param_names: Vec<String> = Vec::new();
        if line.peek() == Some(&Tok::LParen) {
            line.next();
            if line.peek() != Some(&Tok::RParen) {
                loop {
                    let vname = line.val_name()?;
                    line.expect(Tok::Colon)?;
                    let ty = parse_type(&mut line, &mut p.module)?;
                    if values.contains_key(&vname) {
                        return line.fail(format!("value `%{vname}` is defined twice"));
                    }
                    values.insert(vname.clone(), Value::new(0)); // placeholder
                    param_names.push(vname);
                    param_tys.push(ty);
                    if line.peek() == Some(&Tok::Comma) {
                        line.next();
                    } else {
                        break;
                    }
                }
            }
            line.expect(Tok::RParen)?;
        }
        line.expect(Tok::Colon)?;
        line.expect_end()?;
        let block = func.make_block(&param_tys);
        for (pname, v) in param_names.iter().zip(func.block_params(block)) {
            values.insert(pname.clone(), v);
        }
        blocks.insert(label, block);
        kinds.push(LineKind::Header(block));
    }

    // Pass B: instructions, in order.
    let mut current: Option<Block> = None;
    let mut fact_lines: Vec<&[Spanned]> = Vec::new();
    for (lt, kind) in lines.iter().zip(&kinds) {
        match kind {
            LineKind::Header(b) => current = Some(*b),
            LineKind::Fact => fact_lines.push(lt),
            LineKind::Inst => {
                let mut line = Line::new(lt);
                let Some(block) = current else {
                    return line.fail("instruction before the first block header");
                };
                parse_inst(&mut line, p, &mut func, block, &blocks, &mut values)?;
            }
        }
    }

    // Pass C: facts.
    for lt in fact_lines {
        let mut line = Line::new(lt);
        parse_fact(&mut line, &mut func, &values)?;
    }

    p.known_sigs.entry(name.clone()).or_insert(sig);
    p.fn_names.insert(name, ());
    p.module.add_func(func);
    Ok(close + 1)
}

// ---------------------------------------------------- instructions ------

struct Mnemonic {
    op: Opcode,
    /// Type suffix (`iconst.i64`, `load.f64`, `sext.i64`, `store.i32`).
    ty: Option<TypeId>,
    icc: Option<IntCc>,
    fcc: Option<FloatCc>,
}

fn parse_mnemonic(line: &Line, name: &str) -> PResult<Mnemonic> {
    // Fixed mnemonics first (some contain dots themselves).
    let fixed: &[(&str, Opcode)] = &[
        ("bconst", Opcode::Bconst),
        ("iadd.chk", Opcode::IaddChk),
        ("isub.chk", Opcode::IsubChk),
        ("imul.chk", Opcode::ImulChk),
        ("idiv.chk", Opcode::IdivChk),
        ("irem.chk", Opcode::IremChk),
        ("iadd.wrap", Opcode::IaddWrap),
        ("isub.wrap", Opcode::IsubWrap),
        ("imul.wrap", Opcode::ImulWrap),
        ("iadd.sat", Opcode::IaddSat),
        ("isub.sat", Opcode::IsubSat),
        ("imul.sat", Opcode::ImulSat),
        ("band", Opcode::Band),
        ("bor", Opcode::Bor),
        ("bxor", Opcode::Bxor),
        ("shl", Opcode::Shl),
        ("lshr", Opcode::Lshr),
        ("ashr", Opcode::Ashr),
        ("fadd", Opcode::Fadd),
        ("fsub", Opcode::Fsub),
        ("fmul", Opcode::Fmul),
        ("fdiv", Opcode::Fdiv),
        ("fneg", Opcode::Fneg),
        ("fma", Opcode::Fma),
        ("ptr.off", Opcode::PtrOff),
        ("agg.make", Opcode::AggMake),
        ("agg.get", Opcode::AggGet),
        ("call", Opcode::Call),
        ("jmp", Opcode::Jmp),
        ("br", Opcode::Br),
        ("ret", Opcode::Ret),
        ("trap", Opcode::Trap),
        ("region.new", Opcode::RegionNew),
        ("region.alloc", Opcode::RegionAlloc),
        ("region.free", Opcode::RegionFree),
        ("rc.dup", Opcode::RcDup),
        ("rc.drop", Opcode::RcDrop),
        ("sync.freeze", Opcode::SyncFreeze),
        ("sync.transfer", Opcode::SyncTransfer),
        ("eu.make.ok", Opcode::EuMakeOk),
        ("eu.make.err", Opcode::EuMakeErr),
        ("eu.is_err", Opcode::EuIsErr),
        ("eu.ok", Opcode::EuOk),
        ("eu.err", Opcode::EuErr),
    ];
    if let Some(&(_, op)) = fixed.iter().find(|&&(m, _)| m == name) {
        return Ok(Mnemonic {
            op,
            ty: None,
            icc: None,
            fcc: None,
        });
    }
    if let Some((prefix, suffix)) = name.rsplit_once('.') {
        let typed: &[(&str, Opcode)] = &[
            ("iconst", Opcode::Iconst),
            ("fconst", Opcode::Fconst),
            ("sext", Opcode::Sext),
            ("zext", Opcode::Zext),
            ("itrunc", Opcode::Itrunc),
            ("load", Opcode::Load),
            ("store", Opcode::Store),
        ];
        if let Some(&(_, op)) = typed.iter().find(|&&(m, _)| m == prefix) {
            let Some(ty) = scalar_type(suffix) else {
                return line.fail(format!(
                    "`{prefix}` needs a scalar type suffix, got `{suffix}`"
                ));
            };
            return Ok(Mnemonic {
                op,
                ty: Some(ty),
                icc: None,
                fcc: None,
            });
        }
        if prefix == "icmp" {
            if let Some(cc) = IntCc::ALL.iter().find(|cc| cc.mnemonic() == suffix) {
                return Ok(Mnemonic {
                    op: Opcode::Icmp,
                    ty: None,
                    icc: Some(*cc),
                    fcc: None,
                });
            }
            return line.fail(format!("unknown icmp condition `{suffix}`"));
        }
        if prefix == "fcmp" {
            if let Some(cc) = FloatCc::ALL.iter().find(|cc| cc.mnemonic() == suffix) {
                return Ok(Mnemonic {
                    op: Opcode::Fcmp,
                    ty: None,
                    icc: None,
                    fcc: Some(*cc),
                });
            }
            return line.fail(format!("unknown fcmp condition `{suffix}`"));
        }
    }
    line.fail(format!("unknown instruction `{name}`"))
}

fn resolve(
    line: &Line,
    values: &HashMap<String, Value>,
    func: &Function,
    name: &str,
) -> PResult<Value> {
    match values.get(name) {
        Some(&v) if func.values.contains(v) => Ok(v),
        _ => line.fail(format!(
            "`%{name}` is not defined yet (WIR text lists defs before uses; block parameters are pre-declared)"
        )),
    }
}

/// Parse `%a, %b, %c` (each resolved). Empty allowed.
fn parse_val_list(
    line: &mut Line,
    values: &HashMap<String, Value>,
    func: &Function,
) -> PResult<Vec<Value>> {
    let mut out = Vec::new();
    while let Some(Tok::Val(_)) = line.peek() {
        let n = line.val_name()?;
        out.push(resolve(line, values, func, &n)?);
        if line.peek() == Some(&Tok::Comma) {
            line.next();
        } else {
            break;
        }
    }
    Ok(out)
}

/// Parse a branch edge: `label` or `label(%a, %b)`.
fn parse_edge(
    line: &mut Line,
    values: &HashMap<String, Value>,
    blocks: &HashMap<String, Block>,
    func: &mut Function,
) -> PResult<crate::ir::BlockCall> {
    let label = line.ident()?;
    let Some(&block) = blocks.get(&label) else {
        return line.fail(format!("branch to undefined block `{label}`"));
    };
    let mut args = Vec::new();
    if line.peek() == Some(&Tok::LParen) {
        line.next();
        args = parse_val_list(line, values, func)?;
        line.expect(Tok::RParen)?;
    }
    Ok(func.block_call(block, &args))
}

#[allow(clippy::too_many_arguments)]
fn parse_inst(
    line: &mut Line,
    p: &mut Parser,
    func: &mut Function,
    block: Block,
    blocks: &HashMap<String, Block>,
    values: &mut HashMap<String, Value>,
) -> PResult<()> {
    // Results: `%r[: ty] {, %r[: ty]} =` — present iff the line starts
    // with a value.
    let mut result_names: Vec<String> = Vec::new();
    let mut result_annots: Vec<Option<TypeId>> = Vec::new();
    if matches!(line.peek(), Some(Tok::Val(_))) {
        loop {
            let name = line.val_name()?;
            if values.contains_key(&name) {
                return line.fail(format!("value `%{name}` is defined twice"));
            }
            let annot = if line.peek() == Some(&Tok::Colon) {
                line.next();
                Some(parse_type(line, &mut p.module)?)
            } else {
                None
            };
            result_names.push(name);
            result_annots.push(annot);
            if line.peek() == Some(&Tok::Comma) {
                line.next();
            } else {
                break;
            }
        }
        line.expect(Tok::Eq)?;
    }
    let mname = line.ident()?;
    let mn = parse_mnemonic(line, &mname)?;
    let op = mn.op;

    // Parse operands and compute result types.
    let mut args: Vec<Value> = Vec::new();
    let mut aux = Aux::None;
    let result_tys: Vec<TypeId> = match op {
        Opcode::Iconst => {
            let lex = line.num()?;
            aux = Aux::Int(parse_i64(line, &lex)?);
            vec![mn.ty.expect("typed mnemonic")]
        }
        Opcode::Fconst => {
            let ty = mn.ty.expect("typed mnemonic");
            let lex = line.num()?;
            let bits = if let Some(hex) = lex.strip_prefix("0x") {
                u64::from_str_radix(hex, 16).map_err(|_| {
                    let (l, c) = line.pos();
                    ParseError {
                        line: l,
                        col: c,
                        msg: format!("bad float bits `{lex}`"),
                    }
                })?
            } else {
                let v: f64 = lex.parse().map_err(|_| {
                    let (l, c) = line.pos();
                    ParseError {
                        line: l,
                        col: c,
                        msg: format!("bad float literal `{lex}`"),
                    }
                })?;
                if ty == types::F32 {
                    (v as f32).to_bits() as u64
                } else {
                    v.to_bits()
                }
            };
            aux = Aux::FloatBits(bits);
            vec![ty]
        }
        Opcode::Bconst => {
            let kw = line.ident()?;
            match kw.as_str() {
                "true" => aux = Aux::Bool(true),
                "false" => aux = Aux::Bool(false),
                other => return line.fail(format!("expected `true` or `false`, found `{other}`")),
            }
            vec![types::BOOL]
        }
        Opcode::IaddChk
        | Opcode::IsubChk
        | Opcode::ImulChk
        | Opcode::IdivChk
        | Opcode::IremChk
        | Opcode::IaddWrap
        | Opcode::IsubWrap
        | Opcode::ImulWrap
        | Opcode::IaddSat
        | Opcode::IsubSat
        | Opcode::ImulSat
        | Opcode::Band
        | Opcode::Bor
        | Opcode::Bxor
        | Opcode::Shl
        | Opcode::Lshr
        | Opcode::Ashr
        | Opcode::Fadd
        | Opcode::Fsub
        | Opcode::Fmul
        | Opcode::Fdiv => {
            args = parse_val_list(line, values, func)?;
            if args.len() != 2 {
                return line.fail(format!("`{mname}` takes exactly 2 operands"));
            }
            vec![func.value_ty(args[0])]
        }
        Opcode::Fneg => {
            args = parse_val_list(line, values, func)?;
            if args.len() != 1 {
                return line.fail("`fneg` takes exactly 1 operand");
            }
            vec![func.value_ty(args[0])]
        }
        Opcode::Fma => {
            args = parse_val_list(line, values, func)?;
            if args.len() != 3 {
                return line.fail("`fma` takes exactly 3 operands");
            }
            vec![func.value_ty(args[0])]
        }
        Opcode::Icmp => {
            aux = Aux::IntCc(mn.icc.expect("icmp cc"));
            args = parse_val_list(line, values, func)?;
            if args.len() != 2 {
                return line.fail("`icmp` takes exactly 2 operands");
            }
            vec![types::BOOL]
        }
        Opcode::Fcmp => {
            aux = Aux::FloatCc(mn.fcc.expect("fcmp cc"));
            args = parse_val_list(line, values, func)?;
            if args.len() != 2 {
                return line.fail("`fcmp` takes exactly 2 operands");
            }
            vec![types::BOOL]
        }
        Opcode::Sext | Opcode::Zext | Opcode::Itrunc => {
            args = parse_val_list(line, values, func)?;
            if args.len() != 1 {
                return line.fail(format!("`{mname}` takes exactly 1 operand"));
            }
            vec![mn.ty.expect("typed mnemonic")]
        }
        Opcode::PtrOff => {
            // ptr.off %p, %i, SCALE
            let n = line.val_name()?;
            args.push(resolve(line, values, func, &n)?);
            line.expect(Tok::Comma)?;
            let n = line.val_name()?;
            args.push(resolve(line, values, func, &n)?);
            line.expect(Tok::Comma)?;
            let lex = line.num()?;
            aux = Aux::Scale(parse_u64(line, &lex)?);
            vec![types::PTR]
        }
        Opcode::Load => {
            args = parse_val_list(line, values, func)?;
            if args.len() != 2 {
                return line.fail("`load` takes a pointer and a mem token");
            }
            vec![mn.ty.expect("typed mnemonic")]
        }
        Opcode::Store => {
            args = parse_val_list(line, values, func)?;
            if args.len() != 3 {
                return line.fail("`store` takes a value, a pointer, and a mem token");
            }
            let sty = mn.ty.expect("typed mnemonic");
            if func.value_ty(args[0]) != sty {
                return line.fail(format!(
                    "store type suffix `{}` does not match the stored value's type `{}`",
                    p.module.types.display(sty),
                    p.module.types.display(func.value_ty(args[0]))
                ));
            }
            // The result is the successor token: same type as the input.
            vec![func.value_ty(args[2])]
        }
        Opcode::AggMake => {
            args = parse_val_list(line, values, func)?;
            if args.is_empty() {
                return line.fail("`agg.make` needs at least one field");
            }
            let field_tys: Vec<TypeId> = args.iter().map(|&v| func.value_ty(v)).collect();
            vec![p.module.types.intern(TypeData::Agg(field_tys))]
        }
        Opcode::AggGet => {
            let n = line.val_name()?;
            let agg = resolve(line, values, func, &n)?;
            args.push(agg);
            line.expect(Tok::Comma)?;
            let lex = line.num()?;
            let k = parse_u64(line, &lex)?;
            aux = Aux::Int(k as i64);
            let TypeData::Agg(fields) = p.module.types.get(func.value_ty(agg)) else {
                return line.fail("`agg.get` needs an aggregate operand");
            };
            let Some(&fty) = fields.get(k as usize) else {
                return line.fail(format!("aggregate has no field {k}"));
            };
            vec![fty]
        }
        Opcode::Call => {
            let callee = match line.peek() {
                Some(Tok::Global(n)) => {
                    let n = n.clone();
                    line.next();
                    n
                }
                _ => return line.fail("expected `@callee` after `call`"),
            };
            let Some(&sig) = p.known_sigs.get(&callee) else {
                return line.fail(format!(
                    "call to `@{callee}` before its `decl` or definition"
                ));
            };
            line.expect(Tok::LParen)?;
            args = parse_val_list(line, values, func)?;
            line.expect(Tok::RParen)?;
            // Import (dedup by name within the function).
            let existing = func
                .ext_funcs
                .iter()
                .find(|(_, d)| d.name == callee)
                .map(|(k, _)| k);
            let ef = match existing {
                Some(k) => k,
                None => func.import_func(callee.clone(), sig),
            };
            aux = Aux::Callee(ef);
            let sig_data = &p.module.sigs[sig];
            let mut tys = sig_data.results.clone();
            for prm in &sig_data.params {
                if p.module.types.is_token(prm.ty) {
                    tys.push(prm.ty);
                }
            }
            tys
        }
        Opcode::Jmp => {
            let edge = parse_edge(line, values, blocks, func)?;
            aux = Aux::Jump(edge);
            vec![]
        }
        Opcode::Br => {
            let n = line.val_name()?;
            args.push(resolve(line, values, func, &n)?);
            line.expect(Tok::Comma)?;
            let t = parse_edge(line, values, blocks, func)?;
            line.expect(Tok::Comma)?;
            let e = parse_edge(line, values, blocks, func)?;
            aux = Aux::Br(t, e);
            vec![]
        }
        Opcode::Ret => {
            args = parse_val_list(line, values, func)?;
            vec![]
        }
        Opcode::Trap => vec![],
        // Reserved ops: generic form, result types must be explicit.
        _ => {
            debug_assert!(op.is_reserved());
            args = parse_val_list(line, values, func)?;
            let mut tys = Vec::with_capacity(result_annots.len());
            for annot in &result_annots {
                match annot {
                    Some(t) => tys.push(*t),
                    None => {
                        return line.fail(format!(
                            "`{mname}` is reserved (s26/s27): its results need explicit types, e.g. `%r: ptr = {mname} …`"
                        ));
                    }
                }
            }
            tys
        }
    };
    line.expect_end()?;

    if result_names.len() != result_tys.len() {
        return line.fail(format!(
            "`{mname}` produces {} result(s), but {} are named",
            result_tys.len(),
            result_names.len()
        ));
    }
    // Non-reserved annotations must agree with the computed type.
    if !op.is_reserved() {
        for (annot, ty) in result_annots.iter().zip(&result_tys) {
            if let Some(a) = annot
                && a != ty
            {
                return line.fail(format!(
                    "result annotation `{}` does not match the computed type `{}`",
                    p.module.types.display(*a),
                    p.module.types.display(*ty)
                ));
            }
        }
    }
    let (_, result_vals) = func.append_inst(block, op, &args, &result_tys, aux);
    for (name, v) in result_names.into_iter().zip(result_vals) {
        values.insert(name, v);
    }
    Ok(())
}

// ------------------------------------------------------------- facts ----

fn parse_fact(
    line: &mut Line,
    func: &mut Function,
    values: &HashMap<String, Value>,
) -> PResult<()> {
    let fact_val = |line: &Line, name: &str| -> PResult<Value> {
        match values.get(name) {
            Some(&v) => Ok(v),
            None => line.fail(format!("fact references undefined value `%{name}`")),
        }
    };
    line.next(); // `fact`
    let kind_kw = line.ident()?;
    let kind = match kind_kw.as_str() {
        "noalias" => {
            let a = line.val_name()?;
            let a = fact_val(line, &a)?;
            let b = line.val_name()?;
            let b = fact_val(line, &b)?;
            FactKind::Noalias(a, b)
        }
        "deref" => {
            let v = line.val_name()?;
            let v = fact_val(line, &v)?;
            let lex = line.num()?;
            let elem = parse_u64(line, &lex)?;
            let size = if matches!(line.peek(), Some(Tok::Ident(x)) if x == "x") {
                line.next();
                let c = line.val_name()?;
                let count = fact_val(line, &c)?;
                DerefSize::Scaled { elem, count }
            } else {
                DerefSize::Const(elem)
            };
            FactKind::Deref(v, size)
        }
        "range" => {
            let v = line.val_name()?;
            let v = fact_val(line, &v)?;
            let lo_lex = line.num()?;
            let lo = parse_i128(line, &lo_lex)?;
            line.expect(Tok::DotDotEq)?;
            let hi_lex = line.num()?;
            let hi = parse_i128(line, &hi_lex)?;
            FactKind::Range(v, lo, hi)
        }
        "region" => {
            let v = line.val_name()?;
            let v = fact_val(line, &v)?;
            let r = line.ident()?;
            let Some(idx) = r.strip_prefix('r').and_then(|s| s.parse::<u32>().ok()) else {
                return line.fail(format!("expected a region (`r0`), found `{r}`"));
            };
            FactKind::Region(v, RegionId::new(idx))
        }
        "frozen" => {
            let v = line.val_name()?;
            let v = fact_val(line, &v)?;
            FactKind::Frozen(v)
        }
        other => return line.fail(format!("unknown fact kind `{other}`")),
    };
    line.expect(Tok::Colon)?;
    let jkw = line.ident()?;
    let just = if jkw == "op" {
        if let Some(Tok::Val(_)) = line.peek() {
            let n = line.val_name()?;
            Just::Op(fact_val(line, &n)?)
        } else {
            Just::DefOp
        }
    } else if let Some(t) = Theorem::ALL.iter().find(|t| t.tag() == jkw) {
        Just::Theorem(*t)
    } else {
        return line.fail(format!("unknown justification `{jkw}`"));
    };
    line.expect_end()?;
    func.add_fact(FactData::new(kind, just));
    Ok(())
}
