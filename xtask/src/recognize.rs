//! THROWAWAY (s03 Target 5): delete when s08–s09 parser lands.
//!
//! Earley recognizer over a hand-encoded token-level grammar transcribed
//! from spec/01-grammar.md §2–§5 (§3.2's precedence table is the
//! authority for the expression tiers, which the EBNF prose-elides).
//! It accepts/rejects token streams, counts distinct parses (ambiguity
//! detection), and emits a production-trace summary for the unique parse
//! (the wordcount.lu snapshot). No error recovery — this is scaffolding,
//! not the s08–s09 parser.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::speclex::Tok;

/// Terminal matchers over lexer tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tm {
    Id,
    IdIs(&'static str),
    Kw(&'static str),
    /// Any reserved keyword (member position is keyword-transparent).
    AnyKw,
    P(&'static str),
    Int,
    Float,
    Str,
    RawStr,
    GenStr,
    SS,
    SM,
    SE,
    Under,
    Term,
}

impl Tm {
    fn matches(&self, t: &Tok) -> bool {
        match (self, t) {
            (Tm::Id, Tok::Ident(_)) => true,
            (Tm::IdIs(s), Tok::Ident(x)) => x == s,
            (Tm::Kw(s), Tok::Kw(x)) => x == s,
            (Tm::AnyKw, Tok::Kw(_)) => true,
            (Tm::P(s), Tok::Punct(x)) => s == x,
            (Tm::Int, Tok::Int) => true,
            (Tm::Float, Tok::Float) => true,
            (Tm::Str, Tok::Str) => true,
            (Tm::RawStr, Tok::RawStr) => true,
            (Tm::GenStr, Tok::GenStr) => true,
            (Tm::SS, Tok::StrStart) => true,
            (Tm::SM, Tok::StrMid) => true,
            (Tm::SE, Tok::StrEnd) => true,
            (Tm::Under, Tok::Underscore) => true,
            (Tm::Term, Tok::Term) => true,
            _ => false,
        }
    }

    fn render(&self) -> String {
        match self {
            Tm::Id => "IDENT".into(),
            Tm::AnyKw => "reserved_kw".into(),
            Tm::IdIs(s) | Tm::Kw(s) => format!("'{s}'"),
            Tm::P(s) => format!("'{s}'"),
            Tm::Int => "INT".into(),
            Tm::Float => "FLOAT".into(),
            Tm::Str => "STRING".into(),
            Tm::RawStr => "RAW_STRING".into(),
            Tm::GenStr => "GENERALIZED_STRING".into(),
            Tm::SS => "STR_START".into(),
            Tm::SM => "STR_MID".into(),
            Tm::SE => "STR_END".into(),
            Tm::Under => "'_'".into(),
            Tm::Term => "TERM".into(),
        }
    }
}

/// Grammar-definition symbol (borrowed names, interned at build time).
#[derive(Debug, Clone, Copy)]
enum S {
    N(&'static str),
    T(Tm),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Sym {
    N(u32),
    T(Tm),
}

struct Prod {
    lhs: u32,
    rhs: Vec<Sym>,
}

pub struct Grammar {
    nt_names: Vec<&'static str>,
    prods: Vec<Prod>,
    by_lhs: Vec<Vec<u32>>,
    start: u32,
}

pub struct Analysis {
    /// Number of distinct parses, saturated at [`CAP`].
    pub parses: u64,
    /// Production-trace summary of the unique parse (parses == 1).
    pub trace: Option<String>,
    /// Smallest ambiguous span, when parses > 1.
    pub ambiguity: Option<String>,
}

const CAP: u64 = 1 << 40;

// ------------------------------------------------------------- building --

struct Builder {
    names: Vec<&'static str>,
    ids: HashMap<&'static str, u32>,
    prods: Vec<Prod>,
}

impl Builder {
    fn nt(&mut self, name: &'static str) -> u32 {
        *self.ids.entry(name).or_insert_with(|| {
            self.names.push(name);
            (self.names.len() - 1) as u32
        })
    }
}

impl Grammar {
    pub fn analyze(&self, toks: &[Tok]) -> Result<Analysis, String> {
        let (spans, furthest) = self.chart(toks);
        let n = toks.len() as u32;
        if !spans
            .get(&(self.start, 0))
            .is_some_and(|ends| ends.contains(&n))
        {
            return Err(reject_message(toks, furthest));
        }
        let mut counter = Counter {
            g: self,
            toks,
            spans: &spans,
            memo_nt: HashMap::new(),
            memo_seq: HashMap::new(),
        };
        let parses = counter.count_nt(self.start, 0, n);
        let trace = (parses == 1).then(|| {
            let mut out = String::new();
            counter.trace_nt(self.start, 0, n, 0, &mut out);
            out
        });
        let ambiguity = (parses > 1).then(|| counter.ambiguity_report());
        Ok(Analysis {
            parses,
            trace,
            ambiguity,
        })
    }

    /// Earley chart; returns completed spans (nt, start) -> ends, plus
    /// the furthest token position any item reached (for diagnostics).
    #[allow(clippy::type_complexity)]
    fn chart(&self, toks: &[Tok]) -> (HashMap<(u32, u32), BTreeSet<u32>>, usize) {
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        struct Item {
            prod: u32,
            dot: u32,
            origin: u32,
        }
        let n = toks.len();
        let mut sets: Vec<Vec<Item>> = vec![Vec::new(); n + 1];
        let mut seen: Vec<HashSet<Item>> = vec![HashSet::new(); n + 1];
        let mut spans: HashMap<(u32, u32), BTreeSet<u32>> = HashMap::new();
        let add =
            |sets: &mut Vec<Vec<Item>>, seen: &mut Vec<HashSet<Item>>, at: usize, it: Item| {
                if seen[at].insert(it) {
                    sets[at].push(it);
                }
            };
        for p in &self.by_lhs[self.start as usize] {
            add(
                &mut sets,
                &mut seen,
                0,
                Item {
                    prod: *p,
                    dot: 0,
                    origin: 0,
                },
            );
        }
        for i in 0..=n {
            let mut idx = 0;
            while idx < sets[i].len() {
                let it = sets[i][idx];
                idx += 1;
                let rhs = &self.prods[it.prod as usize].rhs;
                if (it.dot as usize) == rhs.len() {
                    // complete
                    let lhs = self.prods[it.prod as usize].lhs;
                    spans.entry((lhs, it.origin)).or_default().insert(i as u32);
                    let parents: Vec<Item> = sets[it.origin as usize]
                        .iter()
                        .filter(|par| {
                            let prhs = &self.prods[par.prod as usize].rhs;
                            prhs.get(par.dot as usize) == Some(&Sym::N(lhs))
                        })
                        .copied()
                        .collect();
                    for par in parents {
                        add(
                            &mut sets,
                            &mut seen,
                            i,
                            Item {
                                prod: par.prod,
                                dot: par.dot + 1,
                                origin: par.origin,
                            },
                        );
                    }
                } else {
                    match rhs[it.dot as usize] {
                        Sym::N(nt) => {
                            for p in &self.by_lhs[nt as usize] {
                                add(
                                    &mut sets,
                                    &mut seen,
                                    i,
                                    Item {
                                        prod: *p,
                                        dot: 0,
                                        origin: i as u32,
                                    },
                                );
                            }
                        }
                        Sym::T(tm) => {
                            if i < n && tm.matches(&toks[i]) {
                                add(
                                    &mut sets,
                                    &mut seen,
                                    i + 1,
                                    Item {
                                        prod: it.prod,
                                        dot: it.dot + 1,
                                        origin: it.origin,
                                    },
                                );
                            }
                        }
                    }
                }
            }
            if sets[i].is_empty() && i > 0 {
                // dead set: nothing can scan further
                break;
            }
        }
        let furthest = (0..=n).rev().find(|&i| !sets[i].is_empty()).unwrap_or(0);
        (spans, furthest)
    }
}

fn reject_message(toks: &[Tok], far: usize) -> String {
    let ctx: Vec<String> = toks
        .iter()
        .skip(far.saturating_sub(4))
        .take(8)
        .map(render_tok)
        .collect();
    format!(
        "no parse (stuck near token {far} of {}: {})",
        toks.len(),
        ctx.join(" ")
    )
}

fn render_tok(t: &Tok) -> String {
    match t {
        Tok::Ident(s) => s.clone(),
        Tok::Kw(s) => s.clone(),
        Tok::Punct(p) => (*p).to_string(),
        Tok::Int => "INT".into(),
        Tok::Float => "FLOAT".into(),
        Tok::Str => "STR".into(),
        Tok::RawStr => "RAWSTR".into(),
        Tok::GenStr => "GENSTR".into(),
        Tok::StrStart => "STR_START".into(),
        Tok::StrMid => "STR_MID".into(),
        Tok::StrEnd => "STR_END".into(),
        Tok::Underscore => "_".into(),
        Tok::Term => "TERM".into(),
    }
}

// ------------------------------------------------------------- counting --

struct Counter<'a> {
    g: &'a Grammar,
    toks: &'a [Tok],
    spans: &'a HashMap<(u32, u32), BTreeSet<u32>>,
    memo_nt: HashMap<(u32, u32, u32), u64>,
    memo_seq: HashMap<(u32, u32, u32, u32), u64>,
}

impl Counter<'_> {
    /// Distinct derivations of nonterminal `nt` over tokens [i, j).
    /// Terminates because the grammar has no ε productions and no unit
    /// cycles: every recursion strictly shrinks the span or descends the
    /// tier chain.
    fn count_nt(&mut self, nt: u32, i: u32, j: u32) -> u64 {
        if let Some(&c) = self.memo_nt.get(&(nt, i, j)) {
            return c;
        }
        if !self.spans.get(&(nt, i)).is_some_and(|e| e.contains(&j)) {
            self.memo_nt.insert((nt, i, j), 0);
            return 0;
        }
        let mut total: u64 = 0;
        for p in self.g.by_lhs[nt as usize].clone() {
            total = total.saturating_add(self.count_seq(p, 0, i, j)).min(CAP);
        }
        self.memo_nt.insert((nt, i, j), total);
        total
    }

    /// Ways to derive rhs[k..] of production `p` over [i, j).
    fn count_seq(&mut self, p: u32, k: u32, i: u32, j: u32) -> u64 {
        let rhs = &self.g.prods[p as usize].rhs;
        if (k as usize) == rhs.len() {
            return u64::from(i == j);
        }
        if let Some(&c) = self.memo_seq.get(&(p, k, i, j)) {
            return c;
        }
        let remaining_min = (rhs.len() - k as usize - 1) as u32; // ≥1 token per symbol
        let total = match rhs[k as usize] {
            Sym::T(tm) => {
                if i < j && tm.matches(&self.toks[i as usize]) {
                    self.count_seq(p, k + 1, i + 1, j)
                } else {
                    0
                }
            }
            Sym::N(nt) => {
                let ends: Vec<u32> = self
                    .spans
                    .get(&(nt, i))
                    .map(|e| {
                        e.iter()
                            .copied()
                            .filter(|&m| m > i && m + remaining_min <= j)
                            .collect()
                    })
                    .unwrap_or_default();
                let mut acc: u64 = 0;
                for m in ends {
                    let a = self.count_nt(nt, i, m);
                    if a == 0 {
                        continue;
                    }
                    let b = self.count_seq(p, k + 1, m, j);
                    acc = acc.saturating_add(a.saturating_mul(b)).min(CAP);
                }
                acc
            }
        };
        self.memo_seq.insert((p, k, i, j), total);
        total
    }

    // -------------------------------------------------------- tracing --

    /// Emit the unique parse as a leftmost-derivation summary: one
    /// production per line, pre-order, unit-chain steps (rhs = single
    /// nonterminal) compressed away.
    fn trace_nt(&mut self, nt: u32, i: u32, j: u32, depth: usize, out: &mut String) {
        let prod = self.g.by_lhs[nt as usize]
            .clone()
            .into_iter()
            .find(|&p| self.count_seq(p, 0, i, j) > 0)
            .expect("unique parse has a production");
        let rhs = self.g.prods[prod as usize].rhs.clone();
        if let [Sym::N(child)] = rhs[..] {
            // unit step: compress
            self.trace_nt(child, i, j, depth, out);
            return;
        }
        let rendered: Vec<String> = rhs
            .iter()
            .map(|s| match s {
                Sym::N(x) => self.g.nt_names[*x as usize].to_string(),
                Sym::T(tm) => tm.render(),
            })
            .collect();
        out.push_str(&"  ".repeat(depth));
        out.push_str(self.g.nt_names[nt as usize]);
        out.push_str(" ::= ");
        out.push_str(&rendered.join(" "));
        out.push('\n');
        // walk children left to right along the unique split
        let mut at = i;
        for (k, sym) in rhs.iter().enumerate() {
            match sym {
                Sym::T(_) => at += 1,
                Sym::N(child) => {
                    let m = self
                        .spans
                        .get(&(*child, at))
                        .into_iter()
                        .flat_map(|e| e.iter().copied())
                        .find(|&m| {
                            m > at
                                && self.count_nt(*child, at, m) > 0
                                && self.count_seq(prod, (k + 1) as u32, m, j) > 0
                        })
                        .expect("unique split");
                    self.trace_nt(*child, at, m, depth + 1, out);
                    at = m;
                }
            }
        }
    }

    /// For an ambiguous input: report the smallest ambiguous span.
    fn ambiguity_report(&mut self) -> String {
        let all: Vec<(u32, u32, u32)> = self
            .spans
            .iter()
            .flat_map(|(&(nt, i), ends)| ends.iter().map(move |&j| (nt, i, j)))
            .collect();
        let mut best: Option<(u32, u32, u32, u64)> = None;
        for (nt, i, j) in all {
            let c = self.count_nt(nt, i, j);
            if c > 1 && best.is_none_or(|(_, bi, bj, _)| j - i < bj - bi) {
                best = Some((nt, i, j, c));
            }
        }
        match best {
            Some((nt, i, j, c)) => {
                let excerpt: Vec<String> = self.toks[i as usize..j as usize]
                    .iter()
                    .take(16)
                    .map(render_tok)
                    .collect();
                format!(
                    "smallest ambiguous span: {} over tokens {i}..{j} ({c} parses): {}",
                    self.g.nt_names[nt as usize],
                    excerpt.join(" ")
                )
            }
            None => "ambiguous at whole-input level only".into(),
        }
    }
}

// -------------------------------------------------------- wolf grammar --

/// The wolf surface grammar, transcribed from spec/01-grammar.md. Tier
/// names follow §3.2's climb; `ns_*` is the no-struct-literal expression
/// mode of `[gram.amb.structlit]` (condition/scrutinee position).
pub fn wolf_grammar() -> Grammar {
    use S::*;
    use Tm::{Float, GenStr, Id, Int, RawStr, SE, SM, SS, Str, Term, Under};
    let id = T(Id);
    let int = T(Int);
    let term = T(Term);
    fn k(s: &'static str) -> S {
        S::T(Tm::Kw(s))
    }
    fn p(s: &'static str) -> S {
        S::T(Tm::P(s))
    }
    fn ii(s: &'static str) -> S {
        S::T(Tm::IdIs(s))
    }

    #[allow(clippy::too_many_lines)]
    let rules: Vec<(&'static str, Vec<Vec<S>>)> = vec![
        // ---- unit & items [gram.item] ----
        (
            "unit",
            vec![vec![N("u_elem")], vec![N("u_elem"), N("unit")]],
        ),
        ("u_elem", vec![vec![N("item")], vec![term]]),
        (
            "item",
            vec![
                vec![N("bare_item")],
                vec![k("pub"), N("bare_item")],
                vec![k("pub"), p("("), ii("pkg"), p(")"), N("bare_item")],
                vec![N("attribute"), N("item")],
            ],
        ),
        (
            "bare_item",
            vec![
                vec![N("fn_item")],
                vec![N("let_t")],
                vec![N("var_t")],
                vec![N("const_t")],
                vec![N("type_item")],
                vec![N("struct_item")],
                vec![N("enum_item")],
                vec![N("trait_item")],
                vec![N("impl_item")],
                vec![N("use_item")],
                vec![N("import_c_item")],
            ],
        ),
        (
            "let_core",
            vec![
                vec![k("let"), N("pattern"), p("="), N("expr")],
                vec![k("let"), N("pattern"), p(":"), N("type"), p("="), N("expr")],
            ],
        ),
        ("let_t", vec![vec![N("let_core"), term]]),
        (
            "var_core",
            vec![
                vec![k("var"), N("pattern"), p("="), N("expr")],
                vec![k("var"), N("pattern"), p(":"), N("type"), p("="), N("expr")],
            ],
        ),
        ("var_t", vec![vec![N("var_core"), term]]),
        (
            "const_core",
            vec![
                vec![k("const"), id, p("="), N("expr")],
                vec![k("const"), id, p(":"), N("type"), p("="), N("expr")],
            ],
        ),
        ("const_t", vec![vec![N("const_core"), term]]),
        // ---- functions [gram.item.fn] ----
        (
            "fn_item",
            vec![vec![N("fn_head"), N("block")], vec![N("fn_head"), term]],
        ),
        (
            "fn_head",
            vec![
                vec![k("fn"), id, N("fn_sig")],
                vec![N("fn_quals"), k("fn"), id, N("fn_sig")],
            ],
        ),
        (
            "fn_quals",
            vec![vec![N("fn_qual")], vec![N("fn_qual"), N("fn_quals")]],
        ),
        (
            "fn_qual",
            vec![
                vec![k("comptime")],
                vec![k("extern"), T(Str)],
                vec![k("export")],
            ],
        ),
        (
            "fn_sig",
            vec![
                vec![p("("), p(")")],
                vec![p("("), N("params"), p(")")],
                vec![N("generics"), p("("), p(")")],
                vec![N("generics"), p("("), N("params"), p(")")],
                vec![p("("), p(")"), N("fn_ret")],
                vec![p("("), N("params"), p(")"), N("fn_ret")],
                vec![N("generics"), p("("), p(")"), N("fn_ret")],
                vec![N("generics"), p("("), N("params"), p(")"), N("fn_ret")],
            ],
        ),
        ("generics", vec![vec![p("["), N("gparams"), p("]")]]),
        (
            "gparams",
            vec![
                vec![N("gparam")],
                vec![N("gparam"), p(",")],
                vec![N("gparam"), p(","), N("gparams")],
            ],
        ),
        (
            "gparam",
            vec![
                vec![id],
                vec![id, p(":"), N("bound")],
                vec![id, p(":"), k("type")],
            ],
        ),
        (
            "bound",
            vec![vec![N("path")], vec![N("path"), p("+"), N("bound")]],
        ),
        (
            "params",
            vec![
                vec![N("param")],
                vec![N("param"), p(",")],
                vec![N("param"), p(","), N("params")],
            ],
        ),
        (
            "param",
            vec![
                vec![id, p(":"), N("type")],
                vec![k("mut"), id, p(":"), N("type")],
                vec![k("take"), id, p(":"), N("type")],
                vec![ii("self")],
                vec![k("mut"), ii("self")],
                vec![k("take"), ii("self")],
                vec![ii("self"), N("view_set")],
                vec![k("mut"), ii("self"), N("view_set")],
                vec![k("take"), ii("self"), N("view_set")],
            ],
        ),
        ("view_set", vec![vec![p("."), p("{"), N("vs_ids"), p("}")]]),
        ("vs_ids", vec![vec![id], vec![id, p(","), N("vs_ids")]]),
        ("fn_ret", vec![vec![p("->"), N("ret_type")]]),
        (
            "ret_type",
            vec![vec![N("type")], vec![N("type"), p("!"), N("error_row")]],
        ),
        ("error_row", vec![vec![p("{"), N("rows"), p("}")]]),
        (
            "rows",
            vec![
                vec![N("row_e")],
                vec![N("row_e"), p(",")],
                vec![N("row_e"), p(","), N("rows")],
                vec![N("row_e"), p(","), p("..")],
                vec![N("row_e"), p(","), p(".."), p(",")],
            ],
        ),
        (
            "row_e",
            vec![
                vec![N("path")],
                vec![N("path"), p("("), N("type_list"), p(")")],
            ],
        ),
        (
            "type_list",
            vec![vec![N("type")], vec![N("type"), p(","), N("type_list")]],
        ),
        // ---- type declarations [gram.item.type] ----
        (
            "type_item",
            vec![
                vec![k("type"), id, p("="), N("type_def")],
                vec![k("type"), id, N("generics"), p("="), N("type_def")],
            ],
        ),
        (
            "type_def",
            vec![vec![N("struct_def")], vec![N("enum_def")], vec![N("type")]],
        ),
        (
            "struct_def",
            vec![
                vec![k("struct"), p("{"), p("}")],
                vec![k("struct"), p("{"), N("fields"), p("}")],
            ],
        ),
        (
            "fields",
            vec![vec![N("field")], vec![N("field"), N("fields")]],
        ),
        (
            "field",
            vec![
                vec![N("field_base")],
                vec![N("attribute"), N("field")],
                vec![k("pub"), N("field_base")],
                vec![k("pub"), p("("), ii("pkg"), p(")"), N("field_base")],
            ],
        ),
        (
            "field_base",
            vec![
                vec![id, p(":"), N("type")],
                vec![id, p(":"), N("type"), p(",")],
            ],
        ),
        (
            "enum_def",
            vec![vec![k("enum"), p("{"), N("variants"), p("}")]],
        ),
        (
            "variants",
            vec![
                vec![N("variant")],
                vec![N("variant"), p(",")],
                vec![N("variant"), p(","), N("variants")],
            ],
        ),
        (
            "variant",
            vec![vec![id], vec![id, p("("), N("type_list"), p(")")]],
        ),
        (
            "struct_item",
            vec![
                vec![k("struct"), id, p("{"), p("}")],
                vec![k("struct"), id, p("{"), N("fields"), p("}")],
                vec![k("struct"), id, N("generics"), p("{"), p("}")],
                vec![k("struct"), id, N("generics"), p("{"), N("fields"), p("}")],
            ],
        ),
        (
            "enum_item",
            vec![
                vec![k("enum"), id, p("{"), N("variants"), p("}")],
                vec![k("enum"), id, N("generics"), p("{"), N("variants"), p("}")],
            ],
        ),
        // ---- traits & impls [gram.item.trait] ----
        (
            "trait_item",
            vec![
                vec![k("trait"), id, p("{"), p("}")],
                vec![k("trait"), id, p("{"), N("mseq"), p("}")],
                vec![k("trait"), id, N("generics"), p("{"), p("}")],
                vec![k("trait"), id, N("generics"), p("{"), N("mseq"), p("}")],
            ],
        ),
        ("mseq", vec![vec![N("melem")], vec![N("melem"), N("mseq")]]),
        ("melem", vec![vec![N("member_item")], vec![term]]),
        (
            "member_item",
            vec![vec![N("fn_item")], vec![N("type_item")], vec![N("const_t")]],
        ),
        (
            "impl_item",
            vec![
                vec![k("impl"), N("path"), p("{"), p("}")],
                vec![k("impl"), N("path"), p("{"), N("mseq"), p("}")],
                vec![k("impl"), N("generics"), N("path"), p("{"), p("}")],
                vec![
                    k("impl"),
                    N("generics"),
                    N("path"),
                    p("{"),
                    N("mseq"),
                    p("}"),
                ],
                vec![k("impl"), N("path"), k("for"), N("type"), p("{"), p("}")],
                vec![
                    k("impl"),
                    N("path"),
                    k("for"),
                    N("type"),
                    p("{"),
                    N("mseq"),
                    p("}"),
                ],
                vec![
                    k("impl"),
                    N("generics"),
                    N("path"),
                    k("for"),
                    N("type"),
                    p("{"),
                    p("}"),
                ],
                vec![
                    k("impl"),
                    N("generics"),
                    N("path"),
                    k("for"),
                    N("type"),
                    p("{"),
                    N("mseq"),
                    p("}"),
                ],
            ],
        ),
        // ---- imports [gram.item.use] ----
        (
            "use_item",
            vec![
                vec![k("use"), N("path"), term],
                vec![k("use"), N("path"), k("as"), id, term],
                vec![
                    k("use"),
                    N("path"),
                    p("."),
                    p("{"),
                    N("use_ids"),
                    p("}"),
                    term,
                ],
                vec![
                    k("use"),
                    N("path"),
                    p("."),
                    p("{"),
                    N("use_ids"),
                    p("}"),
                    k("as"),
                    id,
                    term,
                ],
            ],
        ),
        (
            "use_ids",
            vec![vec![id], vec![id, p(",")], vec![id, p(","), N("use_ids")]],
        ),
        (
            "import_c_item",
            vec![vec![k("import"), ii("c"), T(Str), term]],
        ),
        // ---- attributes [gram.item.attr] ----
        ("attribute", vec![vec![p("#["), N("attrs"), p("]")]]),
        (
            "attrs",
            vec![vec![N("attr1")], vec![N("attr1"), p(","), N("attrs")]],
        ),
        (
            "attr1",
            vec![vec![N("path")], vec![N("path"), N("attr_input")]],
        ),
        (
            "attr_input",
            vec![vec![p("("), N("aargs"), p(")")], vec![p("="), N("literal")]],
        ),
        (
            "aargs",
            vec![vec![N("aarg")], vec![N("aarg"), p(","), N("aargs")]],
        ),
        ("aarg", vec![vec![N("attr1")], vec![N("literal")]]),
        ("path", vec![vec![id], vec![id, p("."), N("path")]]),
        // ---- blocks & statements [gram.expr.block] ----
        (
            "block",
            vec![vec![p("{"), p("}")], vec![p("{"), N("stmt_seq"), p("}")]],
        ),
        (
            "stmt_seq",
            vec![
                vec![N("stmt_t")],
                vec![N("stmt_t"), N("stmt_seq")],
                vec![N("fin_stmt")],
            ],
        ),
        (
            "stmt_t",
            vec![
                vec![N("let_t")],
                vec![N("var_t")],
                vec![N("const_t")],
                vec![N("assign_t")],
                vec![N("defer_t")],
                vec![N("expr_t")],
                vec![N("assume_t")],
                vec![N("item_stmt")],
                vec![N("attr_stmt")],
            ],
        ),
        ("item_stmt", vec![vec![N("item"), term]]),
        (
            "attr_stmt",
            vec![
                vec![N("attribute"), N("nonitem_t")],
                vec![N("attribute"), N("attr_stmt")],
            ],
        ),
        (
            "nonitem_t",
            vec![
                vec![N("let_t")],
                vec![N("var_t")],
                vec![N("const_t")],
                vec![N("assign_t")],
                vec![N("defer_t")],
                vec![N("expr_t")],
                vec![N("assume_t")],
            ],
        ),
        ("assign_core", vec![vec![N("expr"), N("asnop"), N("expr")]]),
        ("assign_t", vec![vec![N("assign_core"), term]]),
        (
            "asnop",
            vec![
                vec![p("=")],
                vec![p("+=")],
                vec![p("-=")],
                vec![p("*=")],
                vec![p("/=")],
                vec![p("%=")],
                vec![p("&=")],
                vec![p("|=")],
                vec![p("^=")],
                vec![p("<<=")],
                vec![p(">>=")],
            ],
        ),
        (
            "defer_core",
            vec![vec![k("defer"), N("expr")], vec![k("errdefer"), N("expr")]],
        ),
        ("defer_t", vec![vec![N("defer_core"), term]]),
        (
            "assume_core",
            vec![vec![
                k("assume"),
                ii("noalias"),
                N("expr"),
                N("assume_tail"),
            ]],
        ),
        (
            "assume_tail",
            vec![
                vec![p(","), N("expr")],
                vec![p(","), N("expr"), N("assume_tail")],
            ],
        ),
        ("assume_t", vec![vec![N("assume_core"), term]]),
        ("expr_t", vec![vec![N("expr"), term]]),
        (
            "fin_stmt",
            vec![
                vec![N("expr")],
                vec![N("let_core")],
                vec![N("var_core")],
                vec![N("const_core")],
                vec![N("assign_core")],
                vec![N("defer_core")],
                vec![N("assume_core")],
            ],
        ),
        // ---- expressions: top split ----
        // Block-ending forms are whole-expression alternatives, not
        // operands: this encodes [gram.amb.else] (an `if`'s `}` takes the
        // `else`) and [gram.amb.closure] (closure bodies extend maximally)
        // structurally, so parse counting sees exactly one derivation.
        ("expr", vec![vec![N("expr_nb")], vec![N("block")]]),
        (
            "expr_nb",
            vec![
                vec![N("else_expr")],
                vec![N("closure")],
                vec![N("jump_expr")],
                vec![N("spawn_expr")],
                vec![N("freeze_expr")],
                vec![N("borrow_expr")],
                vec![N("blocky_nb")],
            ],
        ),
        (
            "blocky_nb",
            vec![
                vec![N("if_expr")],
                vec![N("match_expr")],
                vec![N("for_expr")],
                vec![N("while_expr")],
                vec![N("loop_expr")],
                vec![N("region_sugar")],
                vec![N("in_expr")],
                vec![N("scope_expr")],
                vec![N("select_expr")],
                vec![N("when_expr")],
                vec![N("unsafe_expr")],
                vec![N("asm_expr")],
            ],
        ),
        (
            "jump_expr",
            vec![
                vec![k("return")],
                vec![k("return"), N("expr")],
                vec![k("break")],
                vec![k("break"), N("expr")],
                vec![k("continue")],
            ],
        ),
        ("freeze_expr", vec![vec![k("freeze"), N("expr")]]),
        (
            "spawn_expr",
            vec![vec![k("spawn"), k("proc"), N("path"), N("call_args")]],
        ),
        (
            "borrow_expr",
            vec![vec![k("borrow"), N("expr_nb"), ii("from"), N("expr_nb")]],
        ),
        // ---- closures [gram.expr.closure] ----
        (
            "closure",
            vec![
                vec![k("fn"), p("("), p(")"), N("cbody")],
                vec![k("fn"), p("("), N("cparams"), p(")"), N("cbody")],
            ],
        ),
        ("cbody", vec![vec![N("block")], vec![N("else_expr")]]),
        (
            "cparams",
            vec![
                vec![N("cp")],
                vec![N("cp"), p(",")],
                vec![N("cp"), p(","), N("cparams")],
            ],
        ),
        (
            "cp",
            vec![
                vec![id],
                vec![k("mut"), id],
                vec![k("take"), id],
                vec![id, p(":"), N("type")],
                vec![k("mut"), id, p(":"), N("type")],
                vec![k("take"), id, p(":"), N("type")],
            ],
        ),
        // ---- the climb, §3.2, tiers 15..2 ----
        (
            "else_expr",
            vec![
                vec![N("range_expr")],
                vec![N("range_expr"), k("else"), N("else_cont")],
            ],
        ),
        (
            "else_cont",
            vec![
                vec![N("else_expr")],
                vec![N("block")],
                vec![p("|"), N("pattern"), p("|"), N("else_body")],
            ],
        ),
        ("else_body", vec![vec![N("else_expr")], vec![N("block")]]),
        (
            "range_expr",
            vec![
                vec![N("r_end")],
                vec![N("r_end"), p("..")],
                vec![N("r_end"), p("..=")],
                vec![N("r_end"), p(".."), N("r_end")],
                vec![N("r_end"), p("..="), N("r_end")],
                vec![p(".."), N("r_end")],
                vec![p("..="), N("r_end")],
            ],
        ),
        (
            "r_end",
            vec![vec![N("or_expr")], vec![p("^"), N("or_expr")]],
        ),
        (
            "or_expr",
            vec![
                vec![N("and_expr")],
                vec![N("or_expr"), p("||"), N("and_expr")],
            ],
        ),
        (
            "and_expr",
            vec![
                vec![N("cmp_expr")],
                vec![N("and_expr"), p("&&"), N("cmp_expr")],
            ],
        ),
        (
            "cmp_expr",
            vec![
                vec![N("bor_expr")],
                vec![N("bor_expr"), N("cmpop"), N("bor_expr")],
            ],
        ),
        (
            "cmpop",
            vec![
                vec![p("==")],
                vec![p("!=")],
                vec![p("<")],
                vec![p(">")],
                vec![p("<=")],
                vec![p(">=")],
                vec![p("<=>")],
            ],
        ),
        (
            "bor_expr",
            vec![
                vec![N("xor_expr")],
                vec![N("bor_expr"), p("|"), N("xor_expr")],
            ],
        ),
        (
            "xor_expr",
            vec![
                vec![N("band_expr")],
                vec![N("xor_expr"), p("^"), N("band_expr")],
            ],
        ),
        (
            "band_expr",
            vec![
                vec![N("shift_expr")],
                vec![N("band_expr"), p("&"), N("shift_expr")],
            ],
        ),
        (
            "shift_expr",
            vec![
                vec![N("add_expr")],
                vec![N("shift_expr"), p("<<"), N("add_expr")],
                vec![N("shift_expr"), p(">>"), N("add_expr")],
            ],
        ),
        (
            "add_expr",
            vec![
                vec![N("mul_expr")],
                vec![N("add_expr"), p("+"), N("mul_expr")],
                vec![N("add_expr"), p("-"), N("mul_expr")],
            ],
        ),
        (
            "mul_expr",
            vec![
                vec![N("cast_expr")],
                vec![N("mul_expr"), p("*"), N("cast_expr")],
                vec![N("mul_expr"), p("/"), N("cast_expr")],
                vec![N("mul_expr"), p("%"), N("cast_expr")],
            ],
        ),
        (
            "cast_expr",
            vec![
                vec![N("prefix_expr")],
                vec![N("cast_expr"), k("as"), N("type")],
            ],
        ),
        (
            "prefix_expr",
            vec![
                vec![N("postfix_expr")],
                vec![p("!"), N("prefix_expr")],
                vec![p("-"), N("prefix_expr")],
                vec![p("&"), k("mut"), N("prefix_expr")],
                vec![p("&"), N("prefix_expr")],
                vec![p("*"), N("prefix_expr")],
                vec![k("move"), N("prefix_expr")],
                vec![k("copy"), N("prefix_expr")],
                vec![k("shared"), N("prefix_expr")],
            ],
        ),
        (
            "postfix_expr",
            vec![
                vec![N("primary")],
                vec![N("postfix_expr"), N("call_args")],
                vec![N("postfix_expr"), N("index_args")],
                vec![N("postfix_expr"), p("."), N("member")],
                vec![N("postfix_expr"), p("?")],
            ],
        ),
        // member position is keyword-transparent: `.take(n)`, `s.spawn(…)`
        ("member", vec![vec![id], vec![int], vec![T(Tm::AnyKw)]]),
        (
            "primary",
            vec![
                vec![N("literal")],
                vec![id],
                vec![N("struct_lit")],
                vec![N("paren_expr")],
                vec![N("region_value")],
            ],
        ),
        (
            "literal",
            vec![
                vec![int],
                vec![T(Float)],
                vec![k("true")],
                vec![k("false")],
                vec![N("string_lit")],
            ],
        ),
        (
            "string_lit",
            vec![
                vec![T(Str)],
                vec![T(RawStr)],
                vec![T(GenStr)],
                vec![N("interp_str")],
            ],
        ),
        ("interp_str", vec![vec![T(SS), N("expr"), N("istail")]]),
        (
            "istail",
            vec![vec![T(SE)], vec![T(SM), N("expr"), N("istail")]],
        ),
        ("paren_expr", vec![vec![p("("), N("elist"), p(")")]]),
        (
            "elist",
            vec![
                vec![N("expr")],
                vec![N("expr"), p(",")],
                vec![N("expr"), p(","), N("elist")],
            ],
        ),
        (
            "struct_lit",
            vec![
                vec![N("path"), p("{"), p("}")],
                vec![N("path"), p("{"), N("fi_list"), p("}")],
            ],
        ),
        (
            "fi_list",
            vec![
                vec![N("fi")],
                vec![N("fi"), p(",")],
                vec![N("fi"), p(","), N("fi_list")],
            ],
        ),
        ("fi", vec![vec![id, p(":"), N("expr")], vec![id]]),
        (
            "region_value",
            vec![
                vec![k("region"), p("("), p(")")],
                vec![k("region"), p("("), N("rstrat"), p(")")],
            ],
        ),
        (
            "rstrat",
            vec![vec![ii("rc")], vec![ii("pool"), p("("), N("type"), p(")")]],
        ),
        (
            "call_args",
            vec![vec![p("("), p(")")], vec![p("("), N("ca_list"), p(")")]],
        ),
        (
            "ca_list",
            vec![
                vec![N("ca")],
                vec![N("ca"), p(",")],
                vec![N("ca"), p(","), N("ca_list")],
            ],
        ),
        (
            "ca",
            vec![
                vec![N("expr")],
                vec![k("mut"), N("expr")],
                vec![k("take"), N("expr")],
            ],
        ),
        (
            "index_args",
            vec![vec![p("["), p("]")], vec![p("["), N("ia_list"), p("]")]],
        ),
        (
            "ia_list",
            vec![
                vec![N("ia")],
                vec![N("ia"), p(",")],
                vec![N("ia"), p(","), N("ia_list")],
            ],
        ),
        // index_arg: call_arg | type forms no expression can spell
        (
            "ia",
            vec![
                vec![N("ca")],
                vec![k("shared"), N("type")],
                vec![k("handle"), N("type")],
                vec![k("weak"), N("type")],
                vec![k("distinct"), N("type")],
                vec![k("region")],
                vec![k("dyn"), N("path")],
            ],
        ),
        // ---- control flow [gram.expr.flow] ----
        (
            "if_expr",
            vec![
                vec![k("if"), N("ns_range"), N("block")],
                vec![k("if"), N("ns_range"), N("block"), k("else"), N("block")],
                vec![k("if"), N("ns_range"), N("block"), k("else"), N("if_expr")],
            ],
        ),
        (
            "match_expr",
            vec![
                vec![k("match"), N("ns_range"), p("{"), p("}")],
                vec![k("match"), N("ns_range"), p("{"), N("arms"), p("}")],
            ],
        ),
        (
            "arms",
            vec![
                vec![N("arm_last")],
                vec![N("arm_e"), p(","), N("arms")],
                vec![N("arm_b"), p(","), N("arms")],
                vec![N("arm_b"), term, N("arms")],
                vec![N("arm_b"), N("arms")],
            ],
        ),
        (
            "arm_last",
            vec![
                vec![N("arm_e")],
                vec![N("arm_e"), p(",")],
                vec![N("arm_e"), term],
                vec![N("arm_b")],
                vec![N("arm_b"), p(",")],
                vec![N("arm_b"), term],
            ],
        ),
        (
            "arm_e",
            vec![
                vec![N("pattern"), p("=>"), N("expr_nb")],
                vec![N("pattern"), k("if"), N("expr_nb"), p("=>"), N("expr_nb")],
            ],
        ),
        (
            "arm_b",
            vec![
                vec![N("pattern"), p("=>"), N("block")],
                vec![N("pattern"), k("if"), N("expr_nb"), p("=>"), N("block")],
            ],
        ),
        (
            "for_expr",
            vec![vec![
                k("for"),
                N("pattern"),
                k("in"),
                N("ns_range"),
                N("block"),
            ]],
        ),
        (
            "while_expr",
            vec![vec![k("while"), N("ns_range"), N("block")]],
        ),
        ("loop_expr", vec![vec![k("loop"), N("block")]]),
        // ---- regions [gram.expr.region] ----
        (
            "region_sugar",
            vec![
                vec![k("region"), id, N("block")],
                vec![k("region"), id, p(":"), N("rstrat"), N("block")],
                vec![k("region"), N("block")],
                vec![k("region"), p(":"), N("rstrat"), N("block")],
            ],
        ),
        ("in_expr", vec![vec![k("in"), N("ns_range"), N("block")]]),
        // ---- concurrency [gram.expr.conc] ----
        (
            "scope_expr",
            vec![
                vec![k("scope"), N("block")],
                vec![k("scope"), id, N("block")],
            ],
        ),
        (
            "select_expr",
            vec![
                vec![k("select"), p("{"), p("}")],
                vec![k("select"), p("{"), N("sarms"), p("}")],
            ],
        ),
        (
            "sarms",
            vec![
                vec![N("sarm_last")],
                vec![N("sarm_e"), p(","), N("sarms")],
                vec![N("sarm_b"), p(","), N("sarms")],
                vec![N("sarm_b"), term, N("sarms")],
                vec![N("sarm_b"), N("sarms")],
            ],
        ),
        (
            "sarm_last",
            vec![
                vec![N("sarm_e")],
                vec![N("sarm_e"), p(",")],
                vec![N("sarm_e"), term],
                vec![N("sarm_b")],
                vec![N("sarm_b"), p(",")],
                vec![N("sarm_b"), term],
            ],
        ),
        (
            "sarm_e",
            vec![
                vec![
                    N("pattern"),
                    ii("from"),
                    N("expr_nb"),
                    p("=>"),
                    N("expr_nb"),
                ],
                vec![
                    ii("timeout"),
                    p("("),
                    N("expr"),
                    p(")"),
                    p("=>"),
                    N("expr_nb"),
                ],
            ],
        ),
        (
            "sarm_b",
            vec![
                vec![N("pattern"), ii("from"), N("expr_nb"), p("=>"), N("block")],
                vec![
                    ii("timeout"),
                    p("("),
                    N("expr"),
                    p(")"),
                    p("=>"),
                    N("block"),
                ],
            ],
        ),
        (
            "when_expr",
            vec![vec![
                k("when"),
                p("("),
                N("expr"),
                p(","),
                N("wexprs"),
                p(")"),
                N("block"),
            ]],
        ),
        (
            "wexprs",
            vec![
                vec![N("expr")],
                vec![N("expr"), p(",")],
                vec![N("expr"), p(","), N("wexprs")],
            ],
        ),
        // ---- unsafe tier [gram.expr.unsafe] ----
        (
            "unsafe_expr",
            vec![
                vec![k("unsafe"), N("block")],
                vec![k("unsafe"), ii("c"), N("block")],
                vec![k("unsafe"), ii("c"), N("capture_list"), N("block")],
            ],
        ),
        ("capture_list", vec![vec![p("["), N("cl_ids"), p("]")]]),
        (
            "cl_ids",
            vec![vec![id], vec![id, p(",")], vec![id, p(","), N("cl_ids")]],
        ),
        (
            "asm_expr",
            vec![
                vec![k("asm"), p("{"), N("string_lit"), p("}")],
                vec![k("asm"), p("{"), N("string_lit"), p(","), p("}")],
                vec![k("asm"), p("{"), N("string_lit"), p(","), N("aops"), p("}")],
            ],
        ),
        (
            "aops",
            vec![
                vec![N("aop")],
                vec![N("aop"), p(",")],
                vec![N("aop"), p(","), N("aops")],
            ],
        ),
        (
            "aop",
            vec![
                vec![id, p("="), N("adir"), p("("), id, p(")"), N("expr_nb")],
                vec![N("adir"), p("("), id, p(")"), N("expr_nb")],
            ],
        ),
        (
            "adir",
            vec![
                vec![k("in")],
                vec![ii("out")],
                vec![ii("inout")],
                vec![ii("lateout")],
            ],
        ),
        // ---- no-struct-literal expression mode [gram.amb.structlit] ----
        (
            "ns_range",
            vec![
                vec![N("ns_rend")],
                vec![N("ns_rend"), p("..")],
                vec![N("ns_rend"), p("..=")],
                vec![N("ns_rend"), p(".."), N("ns_rend")],
                vec![N("ns_rend"), p("..="), N("ns_rend")],
                vec![p(".."), N("ns_rend")],
                vec![p("..="), N("ns_rend")],
            ],
        ),
        ("ns_rend", vec![vec![N("ns_or")], vec![p("^"), N("ns_or")]]),
        (
            "ns_or",
            vec![vec![N("ns_and")], vec![N("ns_or"), p("||"), N("ns_and")]],
        ),
        (
            "ns_and",
            vec![vec![N("ns_cmp")], vec![N("ns_and"), p("&&"), N("ns_cmp")]],
        ),
        (
            "ns_cmp",
            vec![
                vec![N("ns_bor")],
                vec![N("ns_bor"), N("cmpop"), N("ns_bor")],
            ],
        ),
        (
            "ns_bor",
            vec![vec![N("ns_xor")], vec![N("ns_bor"), p("|"), N("ns_xor")]],
        ),
        (
            "ns_xor",
            vec![vec![N("ns_band")], vec![N("ns_xor"), p("^"), N("ns_band")]],
        ),
        (
            "ns_band",
            vec![
                vec![N("ns_shift")],
                vec![N("ns_band"), p("&"), N("ns_shift")],
            ],
        ),
        (
            "ns_shift",
            vec![
                vec![N("ns_add")],
                vec![N("ns_shift"), p("<<"), N("ns_add")],
                vec![N("ns_shift"), p(">>"), N("ns_add")],
            ],
        ),
        (
            "ns_add",
            vec![
                vec![N("ns_mul")],
                vec![N("ns_add"), p("+"), N("ns_mul")],
                vec![N("ns_add"), p("-"), N("ns_mul")],
            ],
        ),
        (
            "ns_mul",
            vec![
                vec![N("ns_cast")],
                vec![N("ns_mul"), p("*"), N("ns_cast")],
                vec![N("ns_mul"), p("/"), N("ns_cast")],
                vec![N("ns_mul"), p("%"), N("ns_cast")],
            ],
        ),
        (
            "ns_cast",
            vec![vec![N("ns_prefix")], vec![N("ns_cast"), k("as"), N("type")]],
        ),
        (
            "ns_prefix",
            vec![
                vec![N("ns_postfix")],
                vec![p("!"), N("ns_prefix")],
                vec![p("-"), N("ns_prefix")],
                vec![p("&"), k("mut"), N("ns_prefix")],
                vec![p("&"), N("ns_prefix")],
                vec![p("*"), N("ns_prefix")],
                vec![k("move"), N("ns_prefix")],
                vec![k("copy"), N("ns_prefix")],
            ],
        ),
        (
            "ns_postfix",
            vec![
                vec![N("ns_primary")],
                vec![N("ns_postfix"), N("call_args")],
                vec![N("ns_postfix"), N("index_args")],
                vec![N("ns_postfix"), p("."), N("member")],
                vec![N("ns_postfix"), p("?")],
            ],
        ),
        (
            "ns_primary",
            vec![vec![N("literal")], vec![id], vec![N("paren_expr")]],
        ),
        // ---- patterns [gram.pat] ----
        (
            "pattern",
            vec![
                vec![N("p_simple")],
                vec![N("p_simple"), p("|"), N("pattern")],
            ],
        ),
        (
            "p_simple",
            vec![
                vec![T(Under)],
                vec![N("literal")],
                vec![id],
                vec![id, p("@"), N("p_simple")],
                vec![N("path"), p("("), N("p_list"), p(")")],
                vec![p("("), N("p_list"), p(")")],
            ],
        ),
        (
            "p_list",
            vec![
                vec![N("pattern")],
                vec![N("pattern"), p(",")],
                vec![N("pattern"), p(","), N("p_list")],
            ],
        ),
        // ---- types [gram.type] ----
        (
            "type",
            vec![
                vec![N("path")],
                vec![N("path"), N("type_args")],
                vec![p("!"), N("type")],
                vec![k("shared"), N("type")],
                vec![k("handle"), N("type")],
                vec![k("weak"), N("type")],
                vec![k("distinct"), N("type")],
                vec![p("*"), N("type")],
                vec![k("dyn"), N("path")],
                vec![p("("), N("tt_list"), p(")")],
                vec![N("fn_type")],
                vec![k("type")],
                vec![k("region")],
            ],
        ),
        ("type_args", vec![vec![p("["), N("ta_list"), p("]")]]),
        (
            "ta_list",
            vec![
                vec![N("type")],
                vec![N("type"), p(",")],
                vec![N("type"), p(","), N("ta_list")],
            ],
        ),
        (
            "tt_list",
            vec![
                vec![N("type")],
                vec![N("type"), p(",")],
                vec![N("type"), p(","), N("tt_list")],
            ],
        ),
        (
            "fn_type",
            vec![
                vec![k("fn"), p("("), p(")")],
                vec![k("fn"), p("("), N("ft_list"), p(")")],
                vec![k("fn"), p("("), p(")"), p("->"), N("ret_type")],
                vec![
                    k("fn"),
                    p("("),
                    N("ft_list"),
                    p(")"),
                    p("->"),
                    N("ret_type"),
                ],
            ],
        ),
        (
            "ft_list",
            vec![vec![N("type")], vec![N("type"), p(","), N("ft_list")]],
        ),
    ];

    let mut b = Builder {
        names: Vec::new(),
        ids: HashMap::new(),
        prods: Vec::new(),
    };
    let start = b.nt("unit");
    for (lhs, alts) in rules {
        let lhs_id = b.nt(lhs);
        for alt in alts {
            assert!(!alt.is_empty(), "epsilon production for {lhs}");
            let rhs: Vec<Sym> = alt
                .into_iter()
                .map(|s| match s {
                    N(name) => Sym::N(b.nt(name)),
                    T(tm) => Sym::T(tm),
                })
                .collect();
            b.prods.push(Prod { lhs: lhs_id, rhs });
        }
    }
    let mut by_lhs: Vec<Vec<u32>> = vec![Vec::new(); b.names.len()];
    for (i, prd) in b.prods.iter().enumerate() {
        by_lhs[prd.lhs as usize].push(i as u32);
    }
    for (nt, prods) in by_lhs.iter().enumerate() {
        assert!(
            !prods.is_empty(),
            "nonterminal `{}` referenced but never defined",
            b.names[nt]
        );
    }
    Grammar {
        nt_names: b.names,
        prods: b.prods,
        by_lhs,
        start,
    }
}
