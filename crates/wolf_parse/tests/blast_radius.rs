//! The D22 bet as a machine-checked property: for single-token
//! mutations of corpus files, (a) the parser emits at most 3
//! diagnostics — 5 for *structural* mutations (delimiters unbalanced,
//! or a declaration keyword inserted/removed), since those shift every
//! delimiter after them or re-key the statement structure, and each
//! enclosing tier legitimately reports once — and (b) every
//! declaration whose token range is untouched by the mutation still
//! parses without error nodes or missing markers (possibly re-parented
//! — deleting a `}` may nest a following declaration, but it must nest
//! *cleanly*), with one designed exception (wolf-lang#283): a mutation
//! that starts a line with `else` withdraws the terminator above it
//! (`[gram.lex.newline]`, #276), and the one statement that terminator
//! closed may take the `else` in — it stays a node of its own kind at
//! its own start, and nothing else moves.
//!
//! Mutations are token-level: delete / duplicate / swap-adjacent /
//! replace-from-pool, applied to the token core spans of the original
//! source. String-episode tokens are not mutation targets here: their
//! balance is the *lexer's* recovery domain (fuzzed by the s07 `lex`
//! target and the `parse_mutated` target, which mutate freely); this
//! property pins the parser tier's containment.
//!
//! The bounds count WRECK SITES, not diagnostics (ruling #27,
//! wolf-lang#367, 2026-10-02): one mutation that breaks two separate
//! places is two problems, not a cascade. The rule, exactly
//! ([`wreck_sites`]): order the added cascade diagnostics by offset;
//! two consecutive ones belong to different sites when the MUTATED tree
//! holds an intact construct — a declaration, a statement or a match /
//! select arm, with no error node and no missing token anywhere inside
//! it — lying wholly between them. An intact construct is the parser
//! proving it had the thread back: it read real source correctly after
//! the first break and before the second. Each site is held to the
//! bound (3, or 5 when structural); the bounds do not move. A cascade —
//! the parser reporting one confusion again and again, or reading the
//! wreckage as new broken constructs — has no intact construct between
//! its reports, so it stays one site and still fails.
//!
//! Budget: `MUTATE_BUDGET` mutations per corpus file (default 3 for PR
//! speed; nightly runs crank it up). Deterministic per (file, index):
//! failures reproduce. Every violation in the run is collected and
//! reported together: asserting on the first one hid #367 behind #360
//! for a release.

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

/// Find a node of `kind` that STARTS at `lo`, whatever its extent.
fn find_start(node: &GreenNode, kind: wolf_ast::SyntaxKind, lo: u32) -> Option<&GreenNode> {
    if node.kind == kind && node.span.lo == lo {
        return Some(node);
    }
    node.nodes().find_map(|n| find_start(n, kind, lo))
}

/// The one designed reach-back (wolf-lang#283): `[gram.lex.newline]`
/// inserts no terminator at a newline whose next token is `else`, so
/// a mutation that puts `else` at the start of a line WITHDRAWS the
/// terminator that closed the statement above it, and that statement
/// — only that one; the lexer withholds exactly one terminator — may
/// re-parse with the `else` inside it. Returns the withheld
/// terminator's original span: the last original token before the
/// mutation site was a newline-spanning `Term`, and the mutated
/// stream has no `Term` there any more. Nothing else in the lexer
/// withholds a terminator on what FOLLOWS (the delimiter rule reads
/// the enclosing opener, the attribute rule the previous token), so a
/// withheld terminator is always the `else` lookahead — asserted at
/// the use site.
fn withheld_terminator(
    orig: &[wolf_lex::Token],
    mutated: &[wolf_lex::Token],
    src: &[u8],
    lo: u32,
) -> Option<wolf_span::Span> {
    let before = orig
        .iter()
        .rev()
        .find(|t| t.span.hi <= lo && !t.span.is_empty())?;
    if before.kind != TokenKind::Term
        || !src[before.span.lo as usize..before.span.hi as usize].contains(&b'\n')
    {
        return None;
    }
    // It precedes the splice, so its position is unshifted.
    let still = mutated
        .iter()
        .any(|t| t.kind == TokenKind::Term && t.span.lo == before.span.lo);
    (!still).then_some(before.span)
}

// ------------------------------------------------------- wreck sites ----

/// The constructs whose intact presence between two diagnostics proves
/// the parser resynchronized: declarations, statements, and arms — the
/// units recovery resumes at. Smaller nodes (a name, a type, a pattern)
/// survive inside a cascade and prove nothing.
fn is_resync_construct(kind: wolf_ast::SyntaxKind) -> bool {
    use wolf_ast::SyntaxKind as K;
    kind.is_item()
        || matches!(
            kind,
            K::ExprStmt | K::AssignStmt | K::DeferStmt | K::AssumeStmt | K::MatchArm | K::SelectArm
        )
}

/// The first intact construct lying wholly in `lo..hi`, if any.
fn intact_between(node: &GreenNode, lo: u32, hi: u32) -> Option<&GreenNode> {
    if node.span.hi <= lo || node.span.lo >= hi {
        return None;
    }
    if is_resync_construct(node.kind)
        && !node.span.is_empty()
        && node.span.lo >= lo
        && node.span.hi <= hi
        && !has_damage(node)
    {
        return Some(node);
    }
    node.nodes().find_map(|n| intact_between(n, lo, hi))
}

/// Ruling #27's partition (module docs): the added cascade diagnostics,
/// ordered by offset, cut wherever an intact construct lies wholly
/// between the end of the site so far and the next diagnostic.
fn wreck_sites<'d>(
    root: &GreenNode,
    added: &[&'d wolf_diag::Diagnostic],
) -> Vec<Vec<&'d wolf_diag::Diagnostic>> {
    let mut sorted = added.to_vec();
    sorted.sort_by_key(|d| (d.primary.span.lo, d.primary.span.hi));
    let mut sites: Vec<Vec<&wolf_diag::Diagnostic>> = Vec::new();
    let mut reach = 0u32;
    for d in sorted {
        let span = d.primary.span;
        match sites.last_mut() {
            Some(site) if intact_between(root, reach, span.lo).is_none() => site.push(d),
            _ => sites.push(vec![d]),
        }
        reach = reach.max(span.hi);
    }
    sites
}

/// The mutated parse's diagnostics that the baseline does not account
/// for. A baseline diagnostic (the corpus counter-example files) is
/// matched by code at its span moved by the splice; one the wreck moved
/// or recoded pays for the nearest remaining diagnostic of its kind
/// (boundary or cascade) — so the count is exactly the count-based
/// subtraction this replaced, and only WHICH ones are added is new.
fn added_diagnostics<'d>(
    original: &[wolf_diag::Diagnostic],
    mutated: &'d [wolf_diag::Diagnostic],
    m: &Mutation,
) -> Vec<&'d wolf_diag::Diagnostic> {
    let delta = m.text.len() as i64 - (m.hi - m.lo) as i64;
    let boundary = |c: wolf_diag::Code| c == wolf_parse::codes::UNCLOSED_DELIMITER;
    let mut left: Vec<&wolf_diag::Diagnostic> = mutated.iter().collect();
    let mut unmatched: Vec<(wolf_diag::Code, u32)> = Vec::new();
    for b in original {
        let s = b.primary.span;
        let moved = if s.hi <= m.lo {
            Some((s.lo, s.hi))
        } else if s.lo >= m.hi {
            Some(((s.lo as i64 + delta) as u32, (s.hi as i64 + delta) as u32))
        } else {
            None
        };
        let at = moved.and_then(|(lo, hi)| {
            left.iter().position(|d| {
                d.code == b.code && d.primary.span.lo == lo && d.primary.span.hi == hi
            })
        });
        match at {
            Some(i) => {
                left.remove(i);
            }
            None => unmatched.push((b.code, moved.map_or(m.lo, |(lo, _)| lo))),
        }
    }
    for (code, near) in unmatched {
        let pick = left
            .iter()
            .enumerate()
            .filter(|(_, d)| boundary(d.code) == boundary(code))
            .min_by_key(|(_, d)| (d.code != code, d.primary.span.lo.abs_diff(near)))
            .map(|(i, _)| i);
        if let Some(i) = pick {
            left.remove(i);
        }
    }
    left
}

fn render_sites(sites: &[Vec<&wolf_diag::Diagnostic>]) -> String {
    sites
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let ds: Vec<String> = s
                .iter()
                .map(|d| {
                    format!(
                        "{} {}..{} {}",
                        d.code, d.primary.span.lo, d.primary.span.hi, d.message
                    )
                })
                .collect();
            format!("site {} ({}): [{}]", i + 1, s.len(), ds.join("; "))
        })
        .collect::<Vec<_>>()
        .join("\n    ")
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

/// The exact #243 counter-example, pinned deterministically (no
/// seed): replacing the `=` of `let a = net_listen_with("127.0.0.1:0",
/// true, 16) else |e| match e {` in `corpus/net/reuse_port.lu` with
/// `fn` — the nightly's `[replace token 35 at 1999..2000 with \`fn\`]`
/// — drew SIX cascade diagnostics against the structural bound of
/// five. Reduced to the smallest program that produces six:
///
/// ```text
/// fn main() -> !int {
///     let a fn f("s", true, 16) else |e| match e {
///         x => {
///             0
///         },
///     }
///     0
/// }
/// ```
///
/// and read one by one: (1) the binding has no value — the mutation
/// site, honest; (2) `"s"` is not a parameter — the argument list read
/// as a header, one report for the run of non-parameters; (3) `true`
/// is a reserved keyword and cannot name a parameter; (4) expected `:`
/// after the parameter name — ON THE SAME TOKEN as (3), one line after
/// saying it is not a name; (5) expected `{` after the header, at
/// `else`; (6) expected the line to end, at the arm's `=>` inside the
/// `{` the phantom function took as its body. Five and six are
/// independent confusions (the header's end, then the body's
/// contents); three and four are one confusion reported twice. The
/// recovery is fixed there — a keyword-named parameter with no `:`
/// following is one report — and the count is five: one per enclosing
/// tier (binding, parameter run, parameter, header, body), which is
/// exactly the module doc's rationale for the structural bound. The
/// bound itself does not move.
#[test]
fn keyword_named_parameter_keeps_the_structural_bound() {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/net/reuse_port.lu");
    let src = std::fs::read(&f).expect("read reuse_port.lu");
    let probe = b"let a = net_listen_with(";
    let start = src
        .windows(probe.len())
        .position(|w| w == probe)
        .expect("reuse_port.lu still spells `let a = net_listen_with(`")
        + b"let a ".len();
    assert_eq!(src[start], b'=');
    let mut mutated = src.clone();
    mutated.splice(start..start + 1, b" fn ".iter().copied());

    let mut sm = wolf_span::SourceMap::new();
    let baseline = wolf_parse::parse_tokens(&wolf_lex::lex(sm.intern(&f), &src), &src);
    let mfile = sm.intern(&f.with_extension("mut_fn"));
    let parse = wolf_parse::parse_tokens(&wolf_lex::lex(mfile, &mutated), &mutated);
    wolf_ast::verify(&parse.root, &mutated).expect("verifier clean");
    let cascade = |ds: &[wolf_diag::Diagnostic]| {
        ds.iter()
            .filter(|d| d.code != wolf_parse::codes::UNCLOSED_DELIMITER)
            .count()
    };
    let added = cascade(&parse.diagnostics).saturating_sub(cascade(&baseline.diagnostics));
    assert!(
        added <= 5,
        "#243 regression: {added} added cascade diagnostics (max 5): {:?}",
        parse.diagnostics
    );
    // And the reading above stays true: `true` is reported exactly
    // once, as the name.
    assert_eq!(
        parse
            .diagnostics
            .iter()
            .filter(|d| d.code == wolf_parse::codes::KEYWORD_AS_IDENT)
            .count(),
        1,
        "the keyword-named parameter is one report"
    );
}

/// The exact #285 counter-example, pinned deterministically: replacing
/// the `{` of the spawned closure's `let v = ch.recv() else |_| {
/// return }` in `corpus/typecheck/closure_return.lu` with `fn` — the
/// nightly's `[replace token 168 at 1183..1184 with \`fn\`]` — drew
/// SIX cascade diagnostics against the structural bound of five.
///
/// The wreck is one: `main`'s body spills to the top level. Five of
/// the six read as the #243 tiers do (the `else` operand, the phantom
/// `fn return`, its parameter list, the `s.spawn(` argument list it
/// closed, and the stray-line fold). The sixth was the SAME fold
/// reported twice, because the spilled body contains a nested
/// `fn clamp(…)` between the two stray runs, and an unambiguous item
/// keyword ended the fold on token identity alone.
///
/// Indentation already says which it is: `fn clamp` sits four columns
/// past the column real declarations sit in, so it is nested inside
/// the wreck, not a sibling of it. The fold looks through it now, and
/// the two runs are one report — the #243 reading (one wreck, one
/// report), applied to the fold instead of to a token. The bound does
/// not move.
#[test]
fn nested_item_inside_a_spilled_body_is_one_stray_run() {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/typecheck/closure_return.lu");
    let src = std::fs::read(&f).expect("read closure_return.lu");
    let probe = b"let v = ch.recv() else |_| {";
    let at = src
        .windows(probe.len())
        .rposition(|w| w == probe)
        .expect("closure_return.lu still spells the spawned closure's `else |_| {`")
        + probe.len()
        - 1;
    assert_eq!(src[at], b'{');
    let mut mutated = src.clone();
    mutated.splice(at..at + 1, b"fn".iter().copied());

    let mut sm = wolf_span::SourceMap::new();
    let baseline = wolf_parse::parse_tokens(&wolf_lex::lex(sm.intern(&f), &src), &src);
    let mfile = sm.intern(&f.with_extension("mut_fn"));
    let parse = wolf_parse::parse_tokens(&wolf_lex::lex(mfile, &mutated), &mutated);
    wolf_ast::verify(&parse.root, &mutated).expect("verifier clean");
    let cascade = |ds: &[wolf_diag::Diagnostic]| {
        ds.iter()
            .filter(|d| d.code != wolf_parse::codes::UNCLOSED_DELIMITER)
            .count()
    };
    let added = cascade(&parse.diagnostics).saturating_sub(cascade(&baseline.diagnostics));
    assert!(
        added <= 5,
        "#285 regression: {added} added cascade diagnostics (max 5): {:?}",
        parse.diagnostics
    );
    // And the reading above stays true: the spilled body is ONE
    // stray-line run, however many nested items sit inside it.
    assert_eq!(
        parse
            .diagnostics
            .iter()
            .filter(|d| d.code == wolf_parse::codes::UNEXPECTED_TOPLEVEL)
            .count(),
        1,
        "a spilled body with a nested `fn` inside it is one report"
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

/// One corpus file, read and parsed once for every mutation of it.
struct Subject {
    path: PathBuf,
    /// Corpus-relative, `/`-separated: the seed's input.
    rel: String,
    src: Vec<u8>,
    lexed: wolf_lex::Lexed,
    original: wolf_parse::Parse,
    /// The untouched-declaration ledger: top-level items and their
    /// spans in the original parse. Items that are damaged in the
    /// *baseline* (the corpus counter-example files) are exempt — the
    /// property tracks clean declarations staying clean.
    items: Vec<(wolf_ast::SyntaxKind, u32, u32)>,
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

fn subject(f: &Path, sm: &mut wolf_span::SourceMap) -> Subject {
    let root = corpus_root();
    let src = std::fs::read(f).unwrap_or_else(|e| panic!("read {}: {e}", f.display()));
    let lexed = wolf_lex::lex(sm.intern(f), &src);
    let original = wolf_parse::parse_tokens(&lexed, &src);
    let items = original
        .root
        .nodes()
        .filter(|n| n.kind.is_item() && !has_damage(n))
        .map(|n| (n.kind, n.span.lo, n.span.hi))
        .collect();
    let rel = f.strip_prefix(&root).unwrap_or(f);
    Subject {
        path: f.to_path_buf(),
        rel: rel.to_string_lossy().replace('\\', "/"),
        src,
        lexed,
        original,
        items,
    }
}

/// The `iteration`-th mutation of a file. Seed from the CORPUS-RELATIVE
/// path (separators normalized): the explored mutation set must be
/// identical on every platform AND in every checkout location. Seeding
/// from the absolute path made CI and local runs explore different
/// mutations — a red `cargo test --workspace` at a green-CI sha (#20).
fn mutation_of(s: &Subject, iteration: usize) -> Option<Mutation> {
    let seed = fnv(s.rel.as_bytes()) ^ (iteration as u64).wrapping_mul(0x9e37);
    let mut rng = Rng::new(seed);
    pick_mutation(&mut rng, &s.src, &s.lexed.tokens)
}

/// Every invariant one mutation breaks, as messages (empty when it
/// holds), and the mutated source.
fn violations(
    s: &Subject,
    m: &Mutation,
    iteration: usize,
    sm: &mut wolf_span::SourceMap,
) -> (Vec<String>, Vec<u8>) {
    let mut mutated = Vec::with_capacity(s.src.len() + 8);
    mutated.extend_from_slice(&s.src[..m.lo as usize]);
    mutated.extend_from_slice(&m.text);
    mutated.extend_from_slice(&s.src[m.hi as usize..]);
    let delta = m.text.len() as i64 - (m.hi - m.lo) as i64;

    let mfile = sm.intern(&s.path.with_extension(format!("mut{iteration}")));
    let mlexed = wolf_lex::lex(mfile, &mutated);
    let parse = wolf_parse::parse_tokens(&mlexed, &mutated);
    let ctx = || format!("{} [{}]", s.path.display(), m.describe);
    let mut failures: Vec<String> = Vec::new();

    // Invariant 0: complete lossless tree, verifier clean.
    if let Err(e) = wolf_ast::verify(&parse.root, &mutated) {
        failures.push(format!("verifier failed for {}: {e}", ctx()));
        return (failures, mutated);
    }

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
    let removed = &s.src[m.lo as usize..m.hi as usize];
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
    let structural = keyed(removed) || keyed(&m.text) || in_generics(&s.original.root);
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
    //
    // Cascade is counted per WRECK SITE (ruling #27, module docs
    // and [`wreck_sites`]); boundaries stay per mutation — they
    // are per unclosed opener already.
    let added = added_diagnostics(&s.original.diagnostics, &parse.diagnostics, &m);
    let (bounds, cascade): (Vec<_>, Vec<_>) = added
        .into_iter()
        .partition(|d| d.code == wolf_parse::codes::UNCLOSED_DELIMITER);
    let sites = wreck_sites(&parse.root, &cascade);
    if let Some(worst) = sites.iter().map(Vec::len).max().filter(|&n| n > max) {
        failures.push(format!(
            "{}: {worst} added cascade diagnostics at one wreck site (max {max}; {} \
             site(s), {} in all):\n    {}",
            ctx(),
            sites.len(),
            cascade.len(),
            render_sites(&sites)
        ));
    }
    if bounds.len() > MAX_BOUNDARY {
        failures.push(format!(
            "{}: {} added recovery-boundary diagnostics (max {MAX_BOUNDARY}): {:?}",
            ctx(),
            bounds.len(),
            parse.diagnostics
        ));
    }

    // Invariant 2: untouched declarations parse without error
    // nodes or missing markers (wherever they re-parented) —
    // with the one designed exception, wolf-lang#283: a
    // mutation that puts `else` at the start of a line withdraws
    // the terminator above it (`[gram.lex.newline]`'s lookahead,
    // #276), and the statement that terminator closed may take
    // the `else` in. The reach is one statement by construction
    // (one terminator withheld), and the property asserts
    // exactly that: the token after the withheld terminator IS
    // `else`; at most the ONE item ending at that terminator is
    // exempt; and even that item is still a node of its own
    // kind starting where it started — it grew, it did not
    // vanish. Every other untouched declaration holds as before.
    let withheld = withheld_terminator(&s.lexed.tokens, &mlexed.tokens, &s.src, m.lo);
    if let Some(term) = withheld {
        let next = mlexed
            .tokens
            .iter()
            .find(|t| t.span.lo >= term.hi && !t.span.is_empty())
            .map(|t| t.kind);
        if next != Some(TokenKind::Kw(wolf_lex::Keyword::Else)) {
            failures.push(format!(
                "{}: a terminator was withheld at {}..{} and the next token is not \
                 `else` ({next:?})",
                ctx(),
                term.lo,
                term.hi
            ));
        }
    }
    let exempt = withheld.and_then(|t| {
        s.items
            .iter()
            .position(|&(_, _, hi)| hi == t.hi || hi == t.lo)
    });
    for (idx, &(kind, lo, hi)) in s.items.iter().enumerate() {
        let (mlo, mhi) = if hi <= m.lo {
            (lo, hi)
        } else if lo >= m.hi {
            ((lo as i64 + delta) as u32, (hi as i64 + delta) as u32)
        } else {
            continue; // touched by the mutation
        };
        match find_span(&parse.root, kind, mlo, mhi) {
            Some(node) if !has_damage(node) => {}
            _ if Some(idx) == exempt => {
                if find_start(&parse.root, kind, mlo).is_none() {
                    failures.push(format!(
                        "{}: the statement before a line-leading `else` is no longer a \
                         {kind:?} starting at {mlo}",
                        ctx()
                    ));
                }
            }
            Some(_) => failures.push(format!(
                "{}: untouched {kind:?} at {mlo}..{mhi} contains error nodes",
                ctx()
            )),
            None => failures.push(format!(
                "{}: untouched {kind:?} {lo}..{hi} not found at {mlo}..{mhi}",
                ctx()
            )),
        }
    }
    (failures, mutated)
}

#[test]
fn single_token_mutations_have_bounded_blast_radius() {
    let budget: usize = std::env::var("MUTATE_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let root = corpus_root();
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "corpus not found at {}", root.display());

    let mut sm = wolf_span::SourceMap::new();
    let mut failures: Vec<String> = Vec::new();
    let mut mutations = 0usize;
    for f in &files {
        let s = subject(f, &mut sm);
        for iteration in 0..budget {
            let Some(m) = mutation_of(&s, iteration) else {
                continue;
            };
            mutations += 1;
            let (found, mutated) = violations(&s, &m, iteration, &mut sm);
            // `BLAST_DUMP=<dir>` writes each violating mutation's source,
            // named for its file and iteration, so a nightly red is one
            // `tree_dump` away from its tree.
            if !found.is_empty()
                && let Some(dir) = std::env::var_os("BLAST_DUMP")
            {
                let name = s.rel.replace('/', "__");
                let out = Path::new(&dir).join(format!("{name}.mut{iteration}.lu"));
                std::fs::write(&out, &mutated).expect("write BLAST_DUMP file");
            }
            failures.extend(found);
        }
    }
    eprintln!(
        "blast radius: {} corpus files, {mutations} mutations at budget {budget}, {} violation(s)",
        files.len(),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} violation(s) of the blast-radius property:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The budget-1000 sweep's violations, pinned by (file, iteration) so
/// the PR gauntlet holds them, not only a 1000-budget run nobody
/// schedules. Before s203 the sweep asserted on its FIRST violation, so
/// at 1000 it stopped at #367 and never reported these; collecting
/// every violation found seventeen more (s203's `BLAST_DUMP` run at
/// `d3648d6a`). Each was a recovery path, fixed in the parser; none
/// moved a bound. A corpus edit to one of these files re-rolls its
/// mutations, so a row that stops reproducing its shape is re-derived
/// from a sweep, not deleted.
const SWEEP_1000: &[(&str, usize)] = &[
    ("conc/proc_cap_fault_join.lu", 451),
    ("conc/proc_cap_fault_join.lu", 897),
    ("ct/membrane.lu", 909),
    ("grammar/index_origin_bad.lu", 642),
    ("grammar/index_origin_closure.lu", 367),
    ("grammar/match_arm_at_binding.lu", 442),
    ("grammar/struct_pattern_match_arm.lu", 804),
    ("grammar/struct_pattern_unknown_field.lu", 607),
    ("kernels/ct_tag_compare.lu", 789),
    ("memory/recv_claim_arg_closure.lu", 305),
    ("memory/recv_claim_arg_closure.lu", 808),
    ("net/inherit_listener.lu", 361),
    ("rows/negative/error_alias_cycle.lu", 976),
    ("typecheck/cast_set.lu", 344),
    ("typecheck/cast_set.lu", 543),
    ("typecheck/cast_set.lu", 876),
    ("typecheck/method_scope/p.lu", 954),
];

#[test]
fn the_budget_1000_sweep_cases_hold() {
    let mut sm = wolf_span::SourceMap::new();
    let mut failures = Vec::new();
    for &(rel, iteration) in SWEEP_1000 {
        let s = subject(&corpus_root().join(rel), &mut sm);
        let m = mutation_of(&s, iteration)
            .unwrap_or_else(|| panic!("{rel} mutation {iteration} no longer picks a target"));
        failures.extend(violations(&s, &m, iteration, &mut sm).0);
    }
    assert!(
        failures.is_empty(),
        "{} of the pinned budget-1000 cases fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// A pinned nightly case: splice `text` over the `len` bytes that start
/// `skip` bytes into the first `probe` in corpus file `rel`, and return
/// the mutated parse with its wreck sites (cascade only; boundaries
/// counted apart, as in the property).
fn pinned_case(
    rel: &str,
    probe: &[u8],
    skip: usize,
    len: usize,
    text: &[u8],
) -> (wolf_parse::Parse, Vec<Vec<wolf_diag::Diagnostic>>) {
    let f = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    let src = std::fs::read(&f).unwrap_or_else(|e| panic!("read {rel}: {e}"));
    let lo = src
        .windows(probe.len())
        .position(|w| w == probe)
        .unwrap_or_else(|| panic!("{rel} still spells {:?}", String::from_utf8_lossy(probe)))
        + skip;
    let m = Mutation {
        lo: lo as u32,
        hi: (lo + len) as u32,
        text: text.to_vec(),
        describe: String::new(),
    };
    let mut mutated = src.clone();
    mutated.splice(lo..lo + len, text.iter().copied());
    let mut sm = wolf_span::SourceMap::new();
    let baseline = wolf_parse::parse_tokens(&wolf_lex::lex(sm.intern(&f), &src), &src);
    let mfile = sm.intern(&f.with_extension("pinned"));
    let parse = wolf_parse::parse_tokens(&wolf_lex::lex(mfile, &mutated), &mutated);
    wolf_ast::verify(&parse.root, &mutated).expect("verifier clean");
    let cascade: Vec<&wolf_diag::Diagnostic> =
        added_diagnostics(&baseline.diagnostics, &parse.diagnostics, &m)
            .into_iter()
            .filter(|d| d.code != wolf_parse::codes::UNCLOSED_DELIMITER)
            .collect();
    let sites = wreck_sites(&parse.root, &cascade)
        .into_iter()
        .map(|s| s.into_iter().cloned().collect())
        .collect();
    (parse, sites)
}

/// wolf-lang#367, the nightly's budget-1000 `conc/capture_mut_arg.lu
/// [replace token 30 at 520..525 with \`match\`]`, pinned: `scope s {`
/// becomes `match s {`, and each of the two `s.spawn(fn() { … })` lines
/// is read as a match arm. Each breaks the same way — `spawn` as a path
/// segment (E0008), the `=>` missing at the `fn` (E0201) — four cascade
/// diagnostics against the tight bound of three, two at each line.
///
/// Ruling #27: two separate places, two problems. The tree says they
/// are separate, which is the rule's whole test: the first arm's body is
/// the closure `fn() { bump(mut n) }`, read cleanly, and its statement
/// `bump(mut n)` is an intact construct lying wholly between the two
/// reports. (The issue read the arm BETWEEN them as intact; it is not —
/// it is the stray `)` as an arm, an error node whose own reports the
/// arm fold swallowed, so it emitted nothing and reset the fold.)
#[test]
fn scope_read_as_match_is_two_wreck_sites() {
    let (parse, sites) = pinned_case(
        "conc/capture_mut_arg.lu",
        b"scope s {",
        0,
        b"scope".len(),
        b" match ",
    );
    let total: usize = sites.iter().map(Vec::len).sum();
    assert_eq!(
        total, 4,
        "four cascade diagnostics: {:?}",
        parse.diagnostics
    );
    assert_eq!(sites.len(), 2, "two wreck sites: {sites:?}");
    for site in &sites {
        assert_eq!(
            site.len(),
            2,
            "each site is one spawn line's two reports: {site:?}"
        );
        assert!(site.len() <= 3, "each site within the tight bound");
    }
    // The construct that separates them is the first arm's own statement.
    let (a, b) = (
        sites[0].iter().map(|d| d.primary.span.hi).max().unwrap(),
        sites[1][0].primary.span.lo,
    );
    let between = intact_between(&parse.root, a, b).expect("an intact construct between the sites");
    assert_eq!(
        between.kind,
        wolf_ast::SyntaxKind::ExprStmt,
        "it is `bump(mut n)`"
    );
}

/// wolf-lang#544, the nightly red since `abf4e5cf`: `memory/
/// recv_view_arg_outside_write.lu [replace token 2 at 644..645 with
/// \`let\`]`, pinned. The struct's `{` becomes `let`: `struct V` has no
/// body (E0201), and the body is read as ONE `let` with three valueless
/// binders, `let x: int, y: int, z: int,` — one report per binder (D63)
/// and the closing `}` after the trailing comma.
///
/// Under ruling #27 this is ONE site: nothing between any two of its
/// reports is an intact construct (the binders are not statements, and
/// every one carries a missing initializer). So the site rule does not
/// excuse it, and that is the rule having teeth: a cascade stays one
/// site. What brought it under the structural bound is the parser: the
/// `}` was reported twice on one token — E0207 for the binder the
/// trailing comma promised, then E0203 for the same `}` as a stray
/// top-level line — and a stray run whose first token already carries a
/// report is that report's wreck (the #243 reading: one token, one
/// report). Five, one site.
#[test]
fn struct_body_read_as_let_is_one_wreck_site_within_the_bound() {
    let (parse, sites) = pinned_case(
        "memory/recv_view_arg_outside_write.lu",
        b"struct V {",
        b"struct V ".len(),
        1,
        b" let ",
    );
    assert_eq!(sites.len(), 1, "one wreck site, not several: {sites:?}");
    assert!(
        sites[0].len() <= 5,
        "#544 regression: {} cascade diagnostics at one site (max 5, structural): {:?}",
        sites[0].len(),
        parse.diagnostics
    );
    // And the reading above stays true: the closing `}` is one report.
    let brace = sites[0].iter().map(|d| d.primary.span.lo).max().unwrap();
    assert_eq!(
        parse
            .diagnostics
            .iter()
            .filter(|d| d.primary.span.lo == brace)
            .count(),
        1,
        "the stray `}}` is reported once: {:?}",
        parse.diagnostics
    );
}

/// The exact #283 counter-example, pinned deterministically (no
/// seed) so it runs in the ordinary gauntlet, not only the nightly:
/// replacing the `fn` of `fn main` in
/// `corpus/lints/ancestor_import/main.lu` with `else` — the nightly's
/// `[replace token 8 at 311..313 with \`else\`]`. Before #276 the
/// `else` was an orphan reported where it stood; with
/// `[gram.lex.newline]`'s lookahead, the newline after `use
/// outer.inner` inserts no terminator, so that declaration continues
/// into the `else` and takes the wreck in as its own error node.
/// Disposition (a) of #283: the property admits exactly this — the
/// terminator withheld is the one before the `else`, the ONE
/// declaration it closed is the only one that moves, and it is still
/// a `UseDecl` beginning at the same byte. The declaration above it
/// (`use outer`) parses clean at its original span, and the wreck is
/// one diagnostic. (b) — narrowing the lookahead to `else` followed by
/// `{` or `if` — was declined: it would buy the property nothing a
/// reader wants (a bare `else` on its own line is an error either way,
/// reported once either way) at the price of one more layout rule in
/// the clause and a divergence from wolf-interp's mirror (#75), which
/// copied the wide form.
#[test]
fn line_leading_else_reaches_exactly_one_statement_back() {
    let f =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/lints/ancestor_import/main.lu");
    let src = std::fs::read(&f).expect("read ancestor_import/main.lu");
    let probe = b"\n\nfn main()";
    let start = src
        .windows(probe.len())
        .position(|w| w == probe)
        .expect("ancestor_import/main.lu still spells `fn main()` after a blank line")
        + 2;
    assert_eq!(&src[start..start + 2], b"fn");
    let mut mutated = src.clone();
    mutated.splice(start..start + 2, b" else ".iter().copied());

    let mut sm = wolf_span::SourceMap::new();
    let lexed = wolf_lex::lex(sm.intern(&f), &src);
    let baseline = wolf_parse::parse_tokens(&lexed, &src);
    let items: Vec<(wolf_ast::SyntaxKind, u32, u32)> = baseline
        .root
        .nodes()
        .filter(|n| n.kind.is_item() && !has_damage(n))
        .map(|n| (n.kind, n.span.lo, n.span.hi))
        .collect();
    let uses: Vec<_> = items
        .iter()
        .filter(|(k, _, _)| *k == wolf_ast::SyntaxKind::UseDecl)
        .collect();
    assert_eq!(uses.len(), 2, "the file declares its two `use` items");

    let mfile = sm.intern(&f.with_extension("mut_else"));
    let mlexed = wolf_lex::lex(mfile, &mutated);
    let parse = wolf_parse::parse_tokens(&mlexed, &mutated);
    wolf_ast::verify(&parse.root, &mutated).expect("verifier clean");
    let added = parse
        .diagnostics
        .len()
        .saturating_sub(baseline.diagnostics.len());
    assert!(
        added <= 5,
        "#283 regression: {added} added parser diagnostics (max 5, structural): {:?}",
        parse.diagnostics
    );

    // The terminator withheld is the one right before the `else`, and
    // it closed the second `use`.
    let term = withheld_terminator(&lexed.tokens, &mlexed.tokens, &src, start as u32)
        .expect("the newline before the mutated `else` no longer inserts a terminator");
    let &(kind2, lo2, hi2) = uses[1];
    assert!(
        term.hi == hi2 || term.lo == hi2,
        "the withheld terminator ends `use outer.inner`"
    );
    // The first `use` is untouched and clean at its own span.
    let &(kind1, lo1, hi1) = uses[0];
    let first = find_span(&parse.root, kind1, lo1, hi1).expect("`use outer` still at its span");
    assert!(!has_damage(first), "`use outer` parses clean");
    // The second took the `else` in: no longer clean at its old
    // extent, still a `UseDecl` starting at the same byte.
    let old = find_span(&parse.root, kind2, lo2, hi2);
    assert!(
        old.is_none_or(has_damage),
        "the reach-back is real: `use outer.inner` is not clean at its old extent"
    );
    let grown = find_start(&parse.root, kind2, lo2)
        .expect("`use outer.inner` is still a UseDecl starting where it started");
    assert!(
        has_damage(grown),
        "the wreck is inside the declaration that took the `else`"
    );
    assert!(
        grown.span.hi > start as u32,
        "the declaration grew to absorb the `else`, not the other way round"
    );
}
