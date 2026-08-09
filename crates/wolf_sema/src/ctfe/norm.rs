//! Const-generic normalization (s16, Target 4): kills `N+1 ≠ N+1`.
//!
//! Equality of const expressions in generic position is decided in
//! three steps, in order, and the line between them is fixed:
//!
//! 1. **Closed expressions** (no generic parameters) fully
//!    CTFE-evaluate and compare by value — `Buf[4]` equals `Buf[2 + 2]`.
//! 2. **Linear arithmetic over generic parameters** — `+`/`-` chains
//!    of parameters and integer literals — normalizes to the canonical
//!    ring form `Σ cᵢ·xᵢ + k`, so `N + 1` ≡ `1 + N` and Rust's
//!    RFC-2000 "same AST node or nothing" wart is dead at a defined
//!    line (report 02 Needs-work #8).
//! 3. **Beyond linear** — any `*`, `/`, `%`, or bit operator touching
//!    a parameter — is opaque: two spellings that are not literally
//!    identical require an explicit witness (E0707 documents the
//!    line). `N * 2` vs `N + N` sits here *deliberately*: coefficients
//!    arise only from repeated addition, never from multiplication,
//!    so the boundary is syntactic and predictable.
//!
//! The comparison runs over the *rendered opaque forms* the s13
//! elaborator interns ([`crate::types::TyKind::Unsupported`], e.g.
//! `"Buf[N + 1]"`): [`opaque_eq`] splits the head and bracket
//! arguments textually, then compares argument-wise.

use std::collections::BTreeMap;

/// The three-valued answer of an opaque-form comparison.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpaqueEq {
    /// Provably equal (identical, or normal forms coincide).
    Equal,
    /// Provably or presumptively distinct (normal forms differ, or
    /// the forms are not const expressions at all and differ).
    Distinct,
    /// Both sides are const expressions, at least one is beyond the
    /// linear line, and the spellings differ: equality needs an
    /// explicit witness (E0707).
    NeedsWitness,
}

/// A canonical linear form: `Σ cᵢ·xᵢ + k` with the terms keyed (and
/// therefore ordered) by parameter name.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct LinExpr {
    pub terms: BTreeMap<String, i128>,
    pub k: i128,
}

/// What a const-expression argument normalizes to.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Norm {
    /// Steps 1–2: a closed value or a linear combination.
    Linear(LinExpr),
    /// Step 3: a const expression beyond the linear line.
    Beyond,
    /// Not a const expression (a type argument, or unparseable).
    NotConst,
}

// --------------------------------------------------------- tokenizer ---

#[derive(Clone, PartialEq, Eq, Debug)]
enum Tok {
    Int(i128),
    Ident(String),
    Plus,
    Minus,
    /// An operator beyond the linear line — `*` `/` `%` `&` `|` `^`
    /// and `l`/`r` for the shifts — kept concrete so *closed*
    /// beyond-expressions can still evaluate (step 1).
    Beyond(char),
    LParen,
    RParen,
    /// Anything else (brackets, dots, …): not a const expression.
    Other,
}

fn tokenize(s: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        match c {
            ' ' | '\t' => i += 1,
            '0'..='9' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
                    i += 1;
                }
                let text: String = s[start..i].chars().filter(|&c| c != '_').collect();
                match text.parse::<i128>() {
                    Ok(v) => out.push(Tok::Int(v)),
                    Err(_) => out.push(Tok::Other),
                }
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                out.push(Tok::Ident(s[start..i].to_string()));
            }
            '+' => {
                out.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                out.push(Tok::Minus);
                i += 1;
            }
            '*' | '/' | '%' | '&' | '|' | '^' => {
                out.push(Tok::Beyond(c));
                i += 1;
            }
            '<' | '>' => {
                // `<<` / `>>` are beyond; a lone `<`/`>` is not const.
                if i + 1 < b.len() && b[i + 1] == b[i] {
                    out.push(Tok::Beyond(if c == '<' { 'l' } else { 'r' }));
                    i += 2;
                } else {
                    out.push(Tok::Other);
                    i += 1;
                }
            }
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            _ => {
                out.push(Tok::Other);
                i += 1;
            }
        }
    }
    out
}

// ------------------------------------------------------------ parser ---

/// Normalize one bracket-argument spelling. The grammar of the linear
/// tier is exactly `('+'|'-') (INT | IDENT | '(' expr ')')` chains;
/// the appearance of any beyond-the-line operator with the rest still
/// const-shaped yields [`Norm::Beyond`]; anything else is
/// [`Norm::NotConst`].
pub fn normalize(s: &str) -> Norm {
    let toks = tokenize(s);
    if toks.iter().any(|t| matches!(t, Tok::Other)) {
        return Norm::NotConst;
    }
    if toks.iter().any(|t| matches!(t, Tok::Beyond(_))) {
        // Const-shaped (only idents, ints, arithmetic) but beyond the
        // linear line. A *closed* beyond-expression (no parameters)
        // still evaluates — step 1 of the ladder.
        if toks.iter().any(|t| matches!(t, Tok::Ident(_))) {
            return Norm::Beyond;
        }
        let mut p = Closed { toks: &toks, at: 0 };
        return match p.expr(0) {
            Some(v) if p.at == toks.len() => Norm::Linear(LinExpr {
                terms: BTreeMap::new(),
                k: v,
            }),
            _ => Norm::Beyond,
        };
    }
    let mut p = Parser { toks: &toks, at: 0 };
    match p.sum(1) {
        Some(lin) if p.at == toks.len() => Norm::Linear(lin),
        _ => Norm::NotConst,
    }
}

struct Parser<'a> {
    toks: &'a [Tok],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn sum(&mut self, sign: i128) -> Option<LinExpr> {
        let mut acc = self.atom(sign)?;
        loop {
            match self.peek() {
                Some(Tok::Plus) => {
                    self.at += 1;
                    let rhs = self.atom(sign)?;
                    acc = add(acc, rhs)?;
                }
                Some(Tok::Minus) => {
                    self.at += 1;
                    let rhs = self.atom(-sign)?;
                    acc = add(acc, rhs)?;
                }
                _ => break,
            }
        }
        Some(acc)
    }

    fn atom(&mut self, sign: i128) -> Option<LinExpr> {
        match self.peek().cloned() {
            Some(Tok::Int(v)) => {
                self.at += 1;
                Some(LinExpr {
                    terms: BTreeMap::new(),
                    k: v.checked_mul(sign)?,
                })
            }
            Some(Tok::Ident(name)) => {
                self.at += 1;
                let mut terms = BTreeMap::new();
                terms.insert(name, sign);
                Some(LinExpr { terms, k: 0 })
            }
            Some(Tok::Minus) => {
                self.at += 1;
                self.atom(-sign)
            }
            Some(Tok::LParen) => {
                self.at += 1;
                let inner = self.sum(sign)?;
                match self.peek() {
                    Some(Tok::RParen) => {
                        self.at += 1;
                        Some(inner)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

fn add(mut a: LinExpr, b: LinExpr) -> Option<LinExpr> {
    for (name, c) in b.terms {
        let slot = a.terms.entry(name).or_insert(0);
        *slot = slot.checked_add(c)?;
    }
    a.k = a.k.checked_add(b.k)?;
    a.terms.retain(|_, &mut c| c != 0);
    Some(a)
}

/// Precedence-climbing evaluator for *closed* const expressions —
/// step 1 of the ladder, with checked arithmetic throughout (X3 holds
/// even here: an overflowing closed form stays Beyond rather than
/// wrapping into a wrong equality).
struct Closed<'a> {
    toks: &'a [Tok],
    at: usize,
}

impl Closed<'_> {
    fn prec(op: char) -> u8 {
        match op {
            '*' | '/' | '%' => 6,
            'l' | 'r' => 4,
            '&' => 3,
            '^' => 2,
            '|' => 1,
            _ => 0,
        }
    }

    fn expr(&mut self, min: u8) -> Option<i128> {
        let mut lhs = self.leaf()?;
        loop {
            let (op, prec) = match self.toks.get(self.at) {
                Some(Tok::Plus) => ('+', 5u8),
                Some(Tok::Minus) => ('-', 5u8),
                Some(&Tok::Beyond(op)) => (op, Self::prec(op)),
                _ => break,
            };
            if prec < min {
                break;
            }
            self.at += 1;
            let rhs = self.expr(prec + 1)?;
            lhs = match op {
                '+' => lhs.checked_add(rhs)?,
                '-' => lhs.checked_sub(rhs)?,
                '*' => lhs.checked_mul(rhs)?,
                '/' => lhs.checked_div(rhs)?,
                '%' => lhs.checked_rem(rhs)?,
                '&' => lhs & rhs,
                '|' => lhs | rhs,
                '^' => lhs ^ rhs,
                'l' => lhs.checked_shl(u32::try_from(rhs).ok()?)?,
                'r' => lhs.checked_shr(u32::try_from(rhs).ok()?)?,
                _ => return None,
            };
        }
        Some(lhs)
    }

    fn leaf(&mut self) -> Option<i128> {
        match self.toks.get(self.at).cloned() {
            Some(Tok::Int(v)) => {
                self.at += 1;
                Some(v)
            }
            Some(Tok::Minus) => {
                self.at += 1;
                self.leaf()?.checked_neg()
            }
            Some(Tok::LParen) => {
                self.at += 1;
                let v = self.expr(0)?;
                match self.toks.get(self.at) {
                    Some(Tok::RParen) => {
                        self.at += 1;
                        Some(v)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

// ----------------------------------------------------- opaque compare ---

/// Split an opaque rendering `Head[a1, a2, …]` into the head and its
/// top-level bracket arguments. `None` when the form has no brackets.
fn split_opaque(s: &str) -> Option<(&str, Vec<&str>)> {
    let open = s.find('[')?;
    if !s.ends_with(']') {
        return None;
    }
    let head = &s[..open];
    let inner = &s[open + 1..s.len() - 1];
    let mut args = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (i, c) in inner.char_indices() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                args.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        args.push(last);
    }
    Some((head, args))
}

/// Compare two opaque type renderings under the three-step ladder.
/// Identical strings are equal by interning before this is ever
/// called; this decides the *different-spelling* cases.
pub fn opaque_eq(a: &str, b: &str) -> OpaqueEq {
    if a == b {
        return OpaqueEq::Equal;
    }
    let (Some((ha, aa)), Some((hb, ab))) = (split_opaque(a), split_opaque(b)) else {
        return OpaqueEq::Distinct;
    };
    if ha != hb || aa.len() != ab.len() {
        return OpaqueEq::Distinct;
    }
    let mut verdict = OpaqueEq::Equal;
    for (x, y) in aa.iter().zip(ab.iter()) {
        if x == y {
            continue;
        }
        match (normalize(x), normalize(y)) {
            (Norm::Linear(lx), Norm::Linear(ly)) => {
                if lx != ly {
                    return OpaqueEq::Distinct;
                }
            }
            // One or both sides beyond the line, spellings differ:
            // the witness tier — unless the other side is not a const
            // expression at all, which is a plain mismatch.
            (Norm::Beyond, Norm::Beyond)
            | (Norm::Beyond, Norm::Linear(_))
            | (Norm::Linear(_), Norm::Beyond) => verdict = OpaqueEq::NeedsWitness,
            _ => return OpaqueEq::Distinct,
        }
    }
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lin(s: &str) -> LinExpr {
        match normalize(s) {
            Norm::Linear(l) => l,
            other => panic!("`{s}` should be linear, got {other:?}"),
        }
    }

    #[test]
    fn ring_normalization_makes_commutation_invisible() {
        assert_eq!(lin("N + 1"), lin("1 + N"));
        assert_eq!(lin("N + M + 1"), lin("1 + M + N"));
        assert_eq!(lin("N - M"), lin("0 - M + N"));
        assert_eq!(lin("N + N"), lin("N + N"));
        assert_ne!(lin("N + 1"), lin("N + 2"));
        assert_ne!(lin("N"), lin("M"));
    }

    #[test]
    fn closed_expressions_compare_by_value() {
        assert_eq!(opaque_eq("Buf[4]", "Buf[2 + 2]"), OpaqueEq::Equal);
        assert_eq!(opaque_eq("Buf[4]", "Buf[2 * 2]"), OpaqueEq::Equal);
        assert_eq!(opaque_eq("Buf[8]", "Buf[1 << 3]"), OpaqueEq::Equal);
        assert_eq!(opaque_eq("Buf[7]", "Buf[1 + 2 * 3]"), OpaqueEq::Equal);
        assert_eq!(opaque_eq("Buf[4]", "Buf[5]"), OpaqueEq::Distinct);
        assert_eq!(opaque_eq("Buf[N + 1]", "Buf[1 + N]"), OpaqueEq::Equal);
        assert_eq!(opaque_eq("Buf[N + 1]", "Buf[N + 2]"), OpaqueEq::Distinct);
    }

    #[test]
    fn the_witness_line_holds_exactly_where_documented() {
        // `N*2` vs `N+N`: both const-shaped, one beyond linear —
        // witness required, never silently equal, never silently
        // distinct.
        assert_eq!(
            opaque_eq("Buf[N * 2]", "Buf[N + N]"),
            OpaqueEq::NeedsWitness
        );
        assert_eq!(
            opaque_eq("Buf[N * M]", "Buf[M * N]"),
            OpaqueEq::NeedsWitness
        );
        assert_eq!(
            opaque_eq("Buf[N << 1]", "Buf[N + N]"),
            OpaqueEq::NeedsWitness
        );
        // Different heads or arity: plain mismatch, no witness talk.
        assert_eq!(opaque_eq("Buf[N * 2]", "Arr[N + N]"), OpaqueEq::Distinct);
        assert_eq!(opaque_eq("Buf[N]", "Buf[N, M]"), OpaqueEq::Distinct);
        // Type-shaped arguments (not const expressions) compare
        // textually only.
        assert_eq!(opaque_eq("List[int]", "List[str]"), OpaqueEq::Distinct);
        assert_eq!(opaque_eq("List[int]", "List[int]"), OpaqueEq::Equal);
    }

    #[test]
    fn property_linear_equivalence_class_closed_under_shuffles() {
        // A deterministic LCG drives the property run: random linear
        // expressions, rendered in shuffled orders with random
        // parenthesization, must all normalize identically.
        let mut seed: u64 = 0x5eed_cafe_f00d_0001;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        for _ in 0..500 {
            let nterms = 1 + (next() % 4) as usize;
            let mut terms: Vec<String> = Vec::new();
            for i in 0..nterms {
                let name = ["N", "M", "K", "P"][i % 4];
                terms.push(name.to_string());
            }
            terms.push(format!("{}", next() % 100));
            let render = |order: &[usize]| -> String {
                let parts: Vec<&str> = order.iter().map(|&i| terms[i].as_str()).collect();
                parts.join(" + ")
            };
            let base: Vec<usize> = (0..terms.len()).collect();
            let mut shuf = base.clone();
            // Fisher–Yates with the LCG.
            for i in (1..shuf.len()).rev() {
                let j = (next() as usize) % (i + 1);
                shuf.swap(i, j);
            }
            let a = render(&base);
            let b = render(&shuf);
            assert_eq!(
                lin(&a),
                lin(&b),
                "shuffle changed the normal form: `{a}` vs `{b}`"
            );
            assert_eq!(
                opaque_eq(&format!("Buf[{a}]"), &format!("Buf[{b}]")),
                OpaqueEq::Equal
            );
        }
    }

    #[test]
    fn property_beyond_forms_never_silently_unify() {
        let mut seed: u64 = 0xbead_5eed_0000_0002;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        for _ in 0..200 {
            let c = 2 + next() % 7;
            let a = format!("Buf[N * {c}]");
            // The additive spelling of the same value.
            let b = format!("Buf[{}]", vec!["N"; c as usize].join(" + "));
            assert_eq!(
                opaque_eq(&a, &b),
                OpaqueEq::NeedsWitness,
                "`{a}` vs `{b}` must sit on the witness side of the line"
            );
        }
    }
}
