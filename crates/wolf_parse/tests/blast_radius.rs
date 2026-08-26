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

/// The exact #20 counter-example, pinned deterministically (no seed):
/// replacing the `:` of `fn sneak[N: type]` in
/// `corpus/comptime/norm_witness.lu` with `1` draws four parser
/// diagnostics — one per enclosing recovery tier. A `:` mutation
/// re-keys binding structure, so it is STRUCTURAL (max 5); before #20
/// it was misclassified into the tight bound and only checkout-path-
/// dependent seeding kept CI from seeing it.
#[test]
fn colon_mutation_in_generics_is_structural() {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/comptime/norm_witness.lu");
    let src = std::fs::read(&f).expect("read norm_witness.lu");
    let colon = b"fn sneak[N: type]";
    let start = src
        .windows(colon.len())
        .position(|w| w == colon)
        .expect("norm_witness.lu still declares `fn sneak[N: type]`")
        + b"fn sneak[N".len();
    let mut mutated = src.clone();
    mutated.splice(start..start + 1, b" 1 ".iter().copied());

    let mut sm = wolf_span::SourceMap::new();
    let baseline = wolf_parse::parse_tokens(&wolf_lex::lex(sm.intern(&f), &src), &src);
    let mfile = sm.intern(&f.with_extension("mut_colon"));
    let parse = wolf_parse::parse_tokens(&wolf_lex::lex(mfile, &mutated), &mutated);
    wolf_ast::verify(&parse.root, &mutated).expect("verifier clean");
    let added = parse
        .diagnostics
        .len()
        .saturating_sub(baseline.diagnostics.len());
    assert!(
        added <= 5,
        "#20 regression: {added} added parser diagnostics (max 5): {:?}",
        parse.diagnostics
    );
}

/// The exact #109 counter-example, pinned deterministically (no
/// seed): swapping `match op` to `op match` in
/// `corpus/strings/match_str_dispatch.lu` used to draw FOUR cascade
/// diagnostics — the statement boundary, then the arm list swallowed
/// whole as a block-expression *scrutinee* (three more inside and
/// after it). [gram.amb.structlit] says a `{` in scrutinee position
/// begins the construct's block, never an expression; with the parser
/// honoring that, the wreck is two reports: the statement boundary
/// and the missing scrutinee, and the arms parse clean.
#[test]
fn swapped_match_keyword_keeps_the_tight_bound() {
    let f =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/strings/match_str_dispatch.lu");
    let src = std::fs::read(&f).expect("read match_str_dispatch.lu");
    let probe = b"match op {";
    let start = src
        .windows(probe.len())
        .position(|w| w == probe)
        .expect("match_str_dispatch.lu still spells `match op {`");
    let mut mutated = src.clone();
    mutated.splice(
        start..start + b"match op".len(),
        b" op match ".iter().copied(),
    );

    let mut sm = wolf_span::SourceMap::new();
    let baseline = wolf_parse::parse_tokens(&wolf_lex::lex(sm.intern(&f), &src), &src);
    let mfile = sm.intern(&f.with_extension("mut_swap"));
    let parse = wolf_parse::parse_tokens(&wolf_lex::lex(mfile, &mutated), &mutated);
    wolf_ast::verify(&parse.root, &mutated).expect("verifier clean");
    let added = parse
        .diagnostics
        .len()
        .saturating_sub(baseline.diagnostics.len());
    assert!(
        added <= 3,
        "#109 regression: {added} added parser diagnostics (max 3): {:?}",
        parse.diagnostics
    );
}

/// A damaged construct may announce its own extent — one E0202 per
/// opener left unclosed, so the ceiling is how deep the delimiters nest
/// at the damage, not how much damage there is.
///
/// Three is measured, not chosen: over 85 198 mutations the corpus tops
/// out at a `[` inserted inside a call inside a block, which leaves
/// exactly `[`, `(` and `{` open and reports each once. 82% of
/// mutations add no boundary at all, and only 6 reach three. A corpus
/// file that nests deeper is a deliberate ratchet of this number, the
/// way the lane floors work — not a licence to raise it when a wreck
/// gets noisier.
const MAX_BOUNDARY: usize = 3;

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
            // Seed from the CORPUS-RELATIVE path (separators
            // normalized): the explored mutation set must be identical
            // on every platform AND in every checkout location.
            // Seeding from the absolute path made CI and local runs
            // explore different mutations — a red `cargo test
            // --workspace` at a green-CI sha (#20).
            let rel = f.strip_prefix(&root).unwrap_or(f);
            let seed = fnv(rel.to_string_lossy().replace('\\', "/").as_bytes())
                ^ (iteration as u64).wrapping_mul(0x9e37);
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
            // a declaration keyword, a `;`, an `=`/`=>`, or a `:` —
            // the constructs that key nesting, statement, and binding
            // structure (moving or changing any of them shifts
            // everything downstream; `:` keys `name: type` in params,
            // generic params, fields and lets — losing it inside
            // `fn f[N: type]` legitimately draws one report per
            // enclosing tier, the #20 finding). Everything else must
            // stay within the tight bound.
            let keyed = |bytes: &[u8]| {
                !delims(bytes).is_empty()
                    || !decl_kw(bytes).is_empty()
                    || bytes.contains(&b';')
                    || bytes.contains(&b'=')
                    || bytes.contains(&b':')
            };
            // Damage INSIDE a generic parameter list re-keys the whole
            // declaration header (`fn f[N: type](…) -> …`): parameter
            // name, bracket balance, parameter list and return type
            // each report once — one per enclosing tier, the module-doc
            // allowance. Classified structurally by the ORIGINAL tree,
            // not by mutation bytes (#20: deleting the bare `N` is as
            // structural as replacing the `:`).
            let in_generics = |node: &GreenNode| {
                fn hit(n: &GreenNode, lo: u32, hi: u32) -> bool {
                    if n.kind == wolf_ast::SyntaxKind::GenericParamList
                        && lo < n.span.hi
                        && hi > n.span.lo
                    {
                        return true;
                    }
                    n.nodes().any(|c| hit(c, lo, hi))
                }
                hit(node, m.lo, m.hi)
            };
            let structural = keyed(removed) || keyed(&m.text) || in_generics(&original.root);
            let max = if structural { 5 } else { 3 };
            // Baseline diagnostics (the corpus counter-example files)
            // are pre-existing; the property bounds the *added* ones.
            //
            // Two kinds of added diagnostic, counted apart, because they
            // answer different questions and one pays for the other.
            //
            // CASCADE is what this property exists to bound: the parser
            // losing the thread and reporting the same wreck again and
            // again, or misreading the wreckage as new constructs.
            //
            // A BOUNDARY diagnostic (E0202, `this `{` is never closed`)
            // is the opposite. It is the parser saying exactly where the
            // damage ends, once per unclosed opener, and it is the thing
            // that STOPS the wreck from swallowing what follows. Charged
            // to the mutation it made recovery self-defeating: closing a
            // damaged block costs a diagnostic, so a fix for the
            // untouched-declarations invariant below (a block that ends
            // at a sibling-level item keyword) paid for itself by
            // breaking this one — measured, one case fixed for one case
            // broken, a net of zero. The bound was a cap on how well the
            // parser was allowed to recover.
            //
            // Their own budget keeps the teeth: boundaries are bounded
            // per unclosed opener, so a storm of them still fails, and
            // nesting depth in the corpus is what sets the number.
            let boundaries = |ds: &[wolf_diag::Diagnostic], want: bool| {
                ds.iter()
                    .filter(|d| (d.code == wolf_parse::codes::UNCLOSED_DELIMITER) == want)
                    .count()
            };
            let added_of = |want: bool| {
                boundaries(&parse.diagnostics, want)
                    .saturating_sub(boundaries(&original.diagnostics, want))
            };
            let cascade = added_of(false);
            let boundary = added_of(true);
            assert!(
                cascade <= max,
                "{}: {cascade} added cascade diagnostics (max {max}): {:?}",
                ctx(),
                parse.diagnostics
            );
            assert!(
                boundary <= MAX_BOUNDARY,
                "{}: {boundary} added recovery-boundary diagnostics (max {MAX_BOUNDARY}): {:?}",
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
