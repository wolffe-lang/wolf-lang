//! The D22 bet as a machine-checked property: for single-token
//! mutations of corpus files, (a) the parser emits at most 3
//! diagnostics — 5 for *structural* mutations (delimiters unbalanced,
//! or a declaration keyword inserted/removed), since those shift every
//! delimiter after them or re-key the statement structure, and each
//! enclosing tier legitimately reports once — and (b) every
//! declaration whose token range is untouched by the mutation still
//! parses without error nodes or missing markers (possibly re-parented
//! — deleting a `}` may nest a following declaration, but it must nest
//! *cleanly*).
//!
//! Mutations are token-level: delete / duplicate / swap-adjacent /
//! replace-from-pool, applied to the token core spans of the original
//! source. String-episode tokens are not mutation targets here: their
//! balance is the *lexer's* recovery domain (fuzzed by the s07 `lex`
//! target and the `parse_mutated` target, which mutate freely); this
//! property pins the parser tier's containment.
//!
//! Budget: `MUTATE_BUDGET` mutations per corpus file (default 3 for PR
//! speed; nightly runs crank it up). Deterministic per (file, index):
//! failures reproduce.

use std::path::{Path, PathBuf};
use wolf_ast::{Child, GreenNode};
use wolf_lex::TokenKind;

// ------------------------------------------------------- tiny PRNG ------

/// xorshift64* — deterministic, seedable, no deps.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

// ------------------------------------------------------- the harness ----

const REPLACEMENT_POOL: &[&str] = &[
    "}", ")", "(", "{", "[", "]", ",", ";", "fn", "let", "else", "=>", "=", "+", ".", "1", "x",
    "mut", "match",
];

/// Is this token a string-episode piece (excluded as a mutation target
/// — see the module docs)?
fn is_string_piece(k: TokenKind) -> bool {
    matches!(
        k,
        TokenKind::StrBegin(_)
            | TokenKind::StrFragment
            | TokenKind::InterpOpen
            | TokenKind::InterpClose
            | TokenKind::FormatSpecBegin
            | TokenKind::StrEnd { .. }
    )
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read corpus dir") {
        let p = entry.expect("dir entry").path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|e| e == "lu") {
            out.push(p);
        }
    }
}

fn has_damage(node: &GreenNode) -> bool {
    if node.kind == wolf_ast::SyntaxKind::ErrorNode {
        return true;
    }
    for c in &node.children {
        match c {
            Child::Node(n) => {
                if has_damage(n) {
                    return true;
                }
            }
            Child::Token(t) => {
                if t.kind == wolf_ast::SyntaxKind::Missing {
                    return true;
                }
            }
        }
    }
    false
}

/// Find a node of `kind` starting at `lo` and ending at `hi` (or one
/// byte short — a mutation elsewhere can suppress the declaration's
/// trailing inserted terminator without touching its tokens), anywhere
/// in the tree (an untouched declaration may have been re-parented).
fn find_span(node: &GreenNode, kind: wolf_ast::SyntaxKind, lo: u32, hi: u32) -> Option<&GreenNode> {
    if node.kind == kind && node.span.lo == lo && (node.span.hi == hi || node.span.hi + 1 == hi) {
        return Some(node);
    }
    for n in node.nodes() {
        if let Some(found) = find_span(n, kind, lo, hi) {
            return Some(found);
        }
    }
    None
}

/// One mutation: replace byte range `lo..hi` with `text`.
struct Mutation {
    lo: u32,
    hi: u32,
    text: Vec<u8>,
    describe: String,
}

fn pick_mutation(rng: &mut Rng, src: &[u8], tokens: &[wolf_lex::Token]) -> Option<Mutation> {
    // Candidate targets: real tokens — no Eof, no string pieces, and no
    // newline-spanning terminators (splicing a line break out lets a
    // preceding `//` comment swallow the next line: a lexical effect,
    // not a parser-recovery scenario; explicit `;` terminators remain
    // fair game).
    let targets: Vec<usize> = (0..tokens.len().saturating_sub(1))
        .filter(|&i| {
            let t = &tokens[i];
            !is_string_piece(t.kind)
                && !t.span.is_empty()
                && !(t.kind == TokenKind::Term
                    && src[t.span.lo as usize..t.span.hi as usize].contains(&b'\n'))
        })
        .collect();
    if targets.is_empty() {
        return None;
    }
    let i = targets[rng.below(targets.len())];
    let span = tokens[i].span;
    let text = |s: wolf_span::Span| src[s.lo as usize..s.hi as usize].to_vec();
    // All splices are space-padded so a mutation never *glues* two
    // neighboring tokens into a new one — that would mutate tokens
    // outside the chosen range and void the untouched-declaration
    // bookkeeping.
    Some(match rng.below(4) {
        0 => Mutation {
            lo: span.lo,
            hi: span.hi,
            text: b" ".to_vec(),
            describe: format!("delete token {i} at {}..{}", span.lo, span.hi),
        },
        1 => {
            let mut t = b" ".to_vec();
            t.extend_from_slice(&text(span));
            t.push(b' ');
            t.extend_from_slice(&text(span));
            t.push(b' ');
            Mutation {
                lo: span.lo,
                hi: span.hi,
                text: t,
                describe: format!("duplicate token {i} at {}..{}", span.lo, span.hi),
            }
        }
        2 => {
            // Swap strictly adjacent tokens (same line — crossing a
            // terminator would be two wreck sites, not one mutation).
            let j = i + 1;
            if j >= tokens.len() - 1
                || is_string_piece(tokens[j].kind)
                || tokens[j].span.is_empty()
                || tokens[j].kind == TokenKind::Term
                || tokens[i].kind == TokenKind::Term
            {
                return None;
            }
            let (a, b) = (tokens[i].span, tokens[j].span);
            let mut t = b" ".to_vec();
            t.extend_from_slice(&text(b));
            t.extend_from_slice(&src[a.hi as usize..b.lo as usize]);
            t.extend_from_slice(&text(a));
            t.push(b' ');
            Mutation {
                lo: a.lo,
                hi: b.hi,
                text: t,
                describe: format!("swap tokens {i}/{j} at {}..{}", a.lo, b.hi),
            }
        }
        _ => {
            let repl = REPLACEMENT_POOL[rng.below(REPLACEMENT_POOL.len())];
            let mut t = b" ".to_vec();
            t.extend_from_slice(repl.as_bytes());
            t.push(b' ');
            Mutation {
                lo: span.lo,
                hi: span.hi,
                text: t,
                describe: format!(
                    "replace token {i} at {}..{} with `{repl}`",
                    span.lo, span.hi
                ),
            }
        }
    })
}

#[test]
fn single_token_mutations_have_bounded_blast_radius() {
    let budget: usize = std::env::var("MUTATE_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "corpus not found at {}", root.display());

    let mut sm = wolf_span::SourceMap::new();
    for f in &files {
        let src = std::fs::read(f).expect("read corpus file");
        let file = sm.intern(f);
        let lexed = wolf_lex::lex(file, &src);
        let original = wolf_parse::parse_tokens(&lexed, &src);
        // The untouched-declaration ledger: top-level items and their
        // spans in the original parse. Items that are damaged in the
        // *baseline* (the corpus counter-example files) are exempt —
        // the property tracks clean declarations staying clean.
        let items: Vec<(wolf_ast::SyntaxKind, u32, u32)> = original
            .root
            .nodes()
            .filter(|n| n.kind.is_item() && !has_damage(n))
            .map(|n| (n.kind, n.span.lo, n.span.hi))
            .collect();

        for iteration in 0..budget {
            let seed =
                fnv(f.to_string_lossy().as_bytes()) ^ (iteration as u64).wrapping_mul(0x9e37);
            let mut rng = Rng::new(seed);
            let Some(m) = pick_mutation(&mut rng, &src, &lexed.tokens) else {
                continue;
            };
            let mut mutated = Vec::with_capacity(src.len() + 8);
            mutated.extend_from_slice(&src[..m.lo as usize]);
            mutated.extend_from_slice(&m.text);
            mutated.extend_from_slice(&src[m.hi as usize..]);
            let delta = m.text.len() as i64 - (m.hi - m.lo) as i64;

            let mfile = sm.intern(&f.with_extension(format!("mut{iteration}")));
            let mlexed = wolf_lex::lex(mfile, &mutated);
            let parse = wolf_parse::parse_tokens(&mlexed, &mutated);
            let ctx = || format!("{} [{}]", f.display(), m.describe);

            // Invariant 0: complete lossless tree, verifier clean.
            wolf_ast::verify(&parse.root, &mutated)
                .unwrap_or_else(|e| panic!("verifier failed for {}: {e}", ctx()));

            // Invariant 1: the parser emits at most 3 diagnostics (5
            // when the mutation unbalances delimiters — see module
            // docs).
            let delims = |bytes: &[u8]| -> Vec<u8> {
                bytes
                    .iter()
                    .copied()
                    .filter(|b| matches!(b, b'(' | b')' | b'[' | b']' | b'{' | b'}'))
                    .collect()
            };
            let decl_kw = |bytes: &[u8]| -> Vec<String> {
                let s = String::from_utf8_lossy(bytes).into_owned();
                s.split_whitespace()
                    .filter(|w| {
                        [
                            "fn", "let", "var", "const", "type", "struct", "enum", "trait", "impl",
                            "use", "import", "pub", "extern", "export", "comptime",
                        ]
                        .contains(w)
                    })
                    .map(str::to_owned)
                    .collect()
            };
            let removed = &src[m.lo as usize..m.hi as usize];
            // Structural: the mutation touches the delimiter skeleton,
            // a declaration keyword, a `;`, or an `=`/`=>` — the
            // constructs that key nesting, statement, and binding
            // structure (moving or changing any of them shifts
            // everything downstream). Everything else must stay within
            // the tight bound.
            let keyed = |bytes: &[u8]| {
                !delims(bytes).is_empty()
                    || !decl_kw(bytes).is_empty()
                    || bytes.contains(&b';')
                    || bytes.contains(&b'=')
            };
            let structural = keyed(removed) || keyed(&m.text);
            let max = if structural { 5 } else { 3 };
            // Baseline diagnostics (the corpus counter-example files)
            // are pre-existing; the property bounds the *added* ones.
            let added = parse
                .diagnostics
                .len()
                .saturating_sub(original.diagnostics.len());
            assert!(
                added <= max,
                "{}: {added} added parser diagnostics (max {max}): {:?}",
                ctx(),
                parse.diagnostics
            );

            // Invariant 2: untouched declarations parse without error
            // nodes or missing markers (wherever they re-parented).
            for &(kind, lo, hi) in &items {
                let (mlo, mhi) = if hi <= m.lo {
                    (lo, hi)
                } else if lo >= m.hi {
                    ((lo as i64 + delta) as u32, (hi as i64 + delta) as u32)
                } else {
                    continue; // touched by the mutation
                };
                let node = find_span(&parse.root, kind, mlo, mhi).unwrap_or_else(|| {
                    panic!(
                        "{}: untouched {kind:?} {lo}..{hi} not found at {mlo}..{mhi}",
                        ctx()
                    )
                });
                assert!(
                    !has_damage(node),
                    "{}: untouched {kind:?} at {mlo}..{mhi} contains error nodes",
                    ctx()
                );
            }
        }
    }
}
