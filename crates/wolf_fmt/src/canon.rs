//! Canonicalization support: the X4 sugar-preference detector and the
//! normalized-tree comparison the safety net and the round-trip
//! property share.
//!
//! **The bar** (X4, D34): a rewrite is admissible only when it is
//! provable *from syntax alone* and meaning-preserving under any future
//! grammar. The formatter may never change program meaning; rewrites
//! that need name resolution or types belong to `wolf fix`, not
//! `wolf fmt`. The rewrites that clear the bar today:
//!
//! - `let r = region(…)` immediately followed by exactly one
//!   `in r { … }`, with no other mention of `r` anywhere else in the
//!   enclosing block, becomes `region r(: strategy)? { … }`. A region
//!   that is sent, frozen, returned, stored, re-entered, or merely
//!   mentioned again is untouched.
//! - Redundant parentheses drop per the precedence table (spec §3.2),
//!   with a blacklist for extent-sensitive forms (closures, blocks,
//!   struct literals, jumps) and a no-struct-literal spine check in
//!   condition position.
//! - `else { if … }` collapses to `else if …` when the block holds
//!   exactly that one comment-free `if`.

use std::collections::{HashMap, HashSet};

use wolf_ast::{Child, GreenNode, SyntaxKind as K};

use crate::lower::Sugar;

/// Scan every block for the X4 sugar-preference pattern. Returns
/// (InBlock span → sugar, LetDecl spans to drop).
pub(crate) type SugarRewrites = (HashMap<(u32, u32), Sugar>, HashSet<(u32, u32)>);

pub(crate) fn region_sugar_rewrites(root: &GreenNode, src: &[u8]) -> SugarRewrites {
    let mut sugar = HashMap::new();
    let mut drops = HashSet::new();
    walk_blocks(root, src, &mut sugar, &mut drops);
    (sugar, drops)
}

fn walk_blocks(
    n: &GreenNode,
    src: &[u8],
    sugar: &mut HashMap<(u32, u32), Sugar>,
    drops: &mut HashSet<(u32, u32)>,
) {
    if n.kind == K::Block {
        scan_block(n, src, sugar, drops);
    }
    for c in &n.children {
        if let Child::Node(m) = c {
            walk_blocks(m, src, sugar, drops);
        }
    }
}

fn scan_block(
    b: &GreenNode,
    src: &[u8],
    sugar: &mut HashMap<(u32, u32), Sugar>,
    drops: &mut HashSet<(u32, u32)>,
) {
    let stmts: Vec<&GreenNode> = b
        .children
        .iter()
        .filter_map(|c| match c {
            Child::Node(n) => Some(n),
            Child::Token(_) => None,
        })
        .collect();
    for i in 0..stmts.len() {
        let Some((name, strategy)) = eligible_let(stmts[i], src) else {
            continue;
        };
        let Some(next) = stmts.get(i + 1) else {
            continue;
        };
        // Every mention of `name` in the block outside the binding
        // statement itself.
        let mut mentions = 0usize;
        for (j, s) in stmts.iter().enumerate() {
            if j != i {
                mentions += count_ident(s, src, &name);
            }
        }
        if mentions != 1 {
            continue;
        }
        // The single mention must be the operand of an `in` block in
        // the *immediately following* statement, reachable without
        // crossing a closure, nested function, or nested block (the
        // rewrite moves creation to that point; anything that could
        // defer or repeat evaluation disqualifies).
        let Some(inblock) = find_direct_inblock(next, src, &name) else {
            continue;
        };
        sugar.insert(
            (inblock.span.lo, inblock.span.hi),
            Sugar {
                name: name.clone(),
                strategy: strategy.cloned(),
            },
        );
        drops.insert((stmts[i].span.lo, stmts[i].span.hi));
    }
}

/// `let r = region(…)` with a plain name, no type annotation, no
/// attributes, and no comments anywhere on the statement.
fn eligible_let<'a>(stmt: &'a GreenNode, src: &[u8]) -> Option<(Vec<u8>, Option<&'a GreenNode>)> {
    if stmt.kind != K::LetDecl {
        return None;
    }
    if stmt.child_token(K::Colon).is_some() {
        return None;
    }
    if stmt.nodes().any(|c| c.kind == K::Attribute) {
        return None;
    }
    let pat = stmt.child_node(K::IdentPat)?;
    let name = pat.child_token(K::Ident)?.text(src).to_vec();
    let value = stmt.child_node(K::RegionValue)?;
    // Only [LetKw, IdentPat, Eq, RegionValue, Term?] qualifies.
    for c in stmt.nodes() {
        if !matches!(c.kind, K::IdentPat | K::RegionValue) {
            return None;
        }
    }
    if has_comment(stmt, src) {
        return None;
    }
    let strategy = value.child_node(K::RegionStrategy);
    Some((name, strategy))
}

fn has_comment(n: &GreenNode, src: &[u8]) -> bool {
    let mut found = false;
    visit_tokens(n, &mut |t| {
        for s in t.leading.iter().chain(t.trailing.iter()) {
            if src[s.lo as usize..].starts_with(b"//") {
                found = true;
            }
        }
    });
    found
}

fn visit_tokens<'n>(n: &'n GreenNode, f: &mut impl FnMut(&'n wolf_ast::GreenToken)) {
    for c in &n.children {
        match c {
            Child::Token(t) => f(t),
            Child::Node(m) => visit_tokens(m, f),
        }
    }
}

fn count_ident(n: &GreenNode, src: &[u8], name: &[u8]) -> usize {
    let mut count = 0usize;
    visit_tokens(n, &mut |t| {
        if t.kind == K::Ident && t.text(src) == name {
            count += 1;
        }
    });
    count
}

/// The `in name { … }` node, reachable from `stmt` without crossing a
/// closure, function, or block boundary.
fn find_direct_inblock<'a>(stmt: &'a GreenNode, src: &[u8], name: &[u8]) -> Option<&'a GreenNode> {
    if stmt.kind == K::InBlock {
        let operand = stmt.nodes().next()?;
        if operand.kind == K::PathExpr
            && operand.children.len() == 1
            && operand
                .child_token(K::Ident)
                .is_some_and(|t| t.text(src) == name)
        {
            return Some(stmt);
        }
        return None;
    }
    if matches!(stmt.kind, K::ClosureExpr | K::FnDecl | K::Block) {
        return None;
    }
    for c in stmt.nodes() {
        if let Some(found) = find_direct_inblock(c, src, name) {
            return Some(found);
        }
    }
    None
}

// ---------------------------------------------------------- normalize ---

/// A comparable tree shape: kinds and token texts, trivia and spans
/// erased, with the formatter's syntax-directed canonicalizations
/// applied to *both* sides so `parse(fmt(s))` and `parse(s)` meet in
/// the middle (the round-trip modulus).
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum NTree {
    Node(K, Vec<NTree>),
    Tok(K, Vec<u8>),
}

/// Normalize a parse tree for round-trip comparison.
pub fn normalize(root: &GreenNode, src: &[u8]) -> NTree {
    let (sugar, drops) = region_sugar_rewrites(root, src);
    norm_node(root, src, &sugar, &drops)
}

fn norm_node(
    n: &GreenNode,
    src: &[u8],
    sugar: &HashMap<(u32, u32), Sugar>,
    drops: &HashSet<(u32, u32)>,
) -> NTree {
    // Redundant-paren erasure, recursively: `((y))` lifts to `y`.
    if n.kind == K::ParenExpr
        && let Some(inner) = n.nodes().next()
    {
        return norm_node(inner, src, sugar, drops);
    }
    let mut kids: Vec<NTree> = Vec::new();
    for c in &n.children {
        match c {
            Child::Token(t) => match t.kind {
                // Terminators and separators are layout, not shape.
                K::Term | K::Comma | K::Eof => {}
                _ => kids.push(NTree::Tok(t.kind, t.text(src).to_vec())),
            },
            Child::Node(m) => {
                // Dropped `let r = region(…)` (X4).
                if drops.contains(&(m.span.lo, m.span.hi)) {
                    continue;
                }
                // X4: value-form pair normalizes to the sugar shape.
                if m.kind == K::InBlock
                    && let Some(s) = sugar.get(&(m.span.lo, m.span.hi))
                {
                    let mut rb: Vec<NTree> = Vec::new();
                    rb.push(NTree::Tok(K::RegionKw, b"region".to_vec()));
                    rb.push(NTree::Tok(K::Ident, s.name.clone()));
                    if let Some(strat) = &s.strategy {
                        rb.push(norm_node(strat, src, sugar, drops));
                    }
                    if let Some(b) = m.child_node(K::Block) {
                        rb.push(norm_node(b, src, sugar, drops));
                    }
                    kids.push(NTree::Node(K::RegionBlock, rb));
                    continue;
                }
                // The formatter's emitted sugar normalizes identically.
                if m.kind == K::RegionBlock
                    && let Some(name) = m.child_token(K::Ident)
                {
                    let mut rb: Vec<NTree> = Vec::new();
                    rb.push(NTree::Tok(K::RegionKw, b"region".to_vec()));
                    rb.push(NTree::Tok(K::Ident, name.text(src).to_vec()));
                    if let Some(strat) = m.child_node(K::RegionStrategy) {
                        rb.push(norm_node(strat, src, sugar, drops));
                    }
                    if let Some(b) = m.child_node(K::Block) {
                        rb.push(norm_node(b, src, sugar, drops));
                    }
                    kids.push(NTree::Node(K::RegionBlock, rb));
                    continue;
                }
                kids.push(norm_node(m, src, sugar, drops));
            }
        }
    }

    // `else if` ↔ `else { if }`: normalize the collapsed spelling to
    // the block spelling.
    if n.kind == K::IfExpr
        && let Some(last) = kids.last()
        && matches!(last, NTree::Node(K::IfExpr, _))
    {
        let inner = kids.pop().unwrap();
        kids.push(NTree::Node(
            K::Block,
            vec![
                NTree::Tok(K::LBrace, b"{".to_vec()),
                NTree::Node(K::ExprStmt, vec![inner]),
                NTree::Tok(K::RBrace, b"}".to_vec()),
            ],
        ));
    }

    // Top level: imports sort to the front (`[gram.fmt.imports]`).
    if n.kind == K::SourceFile {
        let mut uses: Vec<NTree> = Vec::new();
        let mut imports: Vec<NTree> = Vec::new();
        let mut rest: Vec<NTree> = Vec::new();
        for k in kids {
            match &k {
                NTree::Node(K::UseDecl, _) => uses.push(k),
                NTree::Node(K::ImportCDecl, _) => imports.push(k),
                _ => rest.push(k),
            }
        }
        let key = |t: &NTree| -> (u8, String) {
            let text = flat_text(t);
            let std = text.starts_with("use std.") || text == "use std";
            (u8::from(!std), text)
        };
        uses.sort_by_key(|a| key(a));
        imports.sort_by_key(flat_text);
        let mut all = uses;
        all.append(&mut imports);
        all.append(&mut rest);
        kids = all;
    }

    NTree::Node(n.kind, kids)
}

fn flat_text(t: &NTree) -> String {
    fn walk(t: &NTree, out: &mut Vec<u8>) {
        match t {
            NTree::Tok(_, text) => {
                if !out.is_empty() {
                    out.push(b' ');
                }
                out.extend_from_slice(text);
            }
            NTree::Node(_, kids) => {
                for k in kids {
                    walk(k, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(t, &mut out);
    String::from_utf8_lossy(&out).into_owned()
}

/// The comment multiset of a source text (trailing whitespace
/// trimmed) — round-trip asserts no comment is lost or duplicated.
pub fn comment_multiset(file: wolf_span::FileId, src: &[u8]) -> Vec<Vec<u8>> {
    let lexed = wolf_lex::lex(file, src);
    let mut out: Vec<Vec<u8>> = Vec::new();
    for t in &lexed.tokens {
        for tr in t.leading.iter().chain(t.trailing.iter()) {
            if matches!(
                tr.kind,
                wolf_lex::TriviaKind::LineComment
                    | wolf_lex::TriviaKind::DocComment
                    | wolf_lex::TriviaKind::InnerDocComment
            ) {
                let bytes = &src[tr.span.lo as usize..tr.span.hi as usize];
                let mut b = bytes.to_vec();
                while matches!(b.last(), Some(b' ' | b'\t' | b'\r')) {
                    b.pop();
                }
                out.push(b);
            }
        }
    }
    out.sort();
    out
}
