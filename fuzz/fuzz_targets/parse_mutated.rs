//! s09 recovery-fuzzing target (the acceptance teeth): corpus-seeded
//! token-level mutations. The fuzzer's bytes choose a corpus file and a
//! sequence of mutations (delete / duplicate / swap / replace N random
//! tokens); the mutated source is parsed. Invariants: no panic, the
//! tree verifier passes (token tiling, span nesting), and tree text
//! reproduces the mutated input byte-for-byte (lossless).
//!
//! Unlike the deterministic blast-radius property test in
//! `crates/wolf_parse/tests/blast_radius.rs` (which also bounds the
//! diagnostic count), this target mutates *everything* — string-mode
//! tokens and terminators included — hunting for panics and tree-shape
//! violations rather than containment regressions.

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The corpus, loaded once (falls back to a tiny embedded seed when the
/// fuzzer runs outside the repo checkout).
fn corpus() -> &'static Vec<Vec<u8>> {
    static CORPUS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    CORPUS.get_or_init(|| {
        let mut files: Vec<PathBuf> = Vec::new();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus");
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|e| e == "lu") {
                    files.push(p);
                }
            }
        }
        files.sort();
        let mut out: Vec<Vec<u8>> = files
            .iter()
            .filter_map(|f| std::fs::read(f).ok())
            .collect();
        if out.is_empty() {
            out.push(b"fn main() -> !int {\n    let x = f(1, 2)?\n    x\n}\n".to_vec());
        }
        out
    })
}

const REPLACEMENTS: &[&str] = &[
    "}", ")", "(", "{", "[", "]", ",", ";", "fn", "let", "else", "=>", "=", "+", ".", "1", "x",
    "mut", "match", "\"", "..", "?", "|",
];

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let files = corpus();
    let mut src = files[data[0] as usize % files.len()].clone();
    let mut bytes = data[1..].iter().copied();

    // Apply up to 8 token-level mutations, re-lexing between rounds so
    // later mutations see the mutated token stream.
    let mut sm = wolf_span::SourceMap::new();
    for round in 0..8usize {
        let (Some(a), Some(b), Some(c)) = (bytes.next(), bytes.next(), bytes.next()) else {
            break;
        };
        let file = sm.intern(Path::new(&format!("m{round}.lu")));
        let lexed = wolf_lex::lex(file, &src);
        // Real tokens only (Eof is zero-width).
        let n = lexed.tokens.len().saturating_sub(1);
        if n == 0 {
            break;
        }
        let i = (a as usize * 256 + b as usize) % n;
        let span = lexed.tokens[i].span;
        let (lo, hi) = (span.lo as usize, span.hi as usize);
        let tok = src[lo..hi].to_vec();
        match c % 4 {
            0 => {
                // delete
                src.splice(lo..hi, std::iter::empty());
            }
            1 => {
                // duplicate (space-separated)
                let mut t = tok.clone();
                t.push(b' ');
                t.extend_from_slice(&tok);
                src.splice(lo..hi, t);
            }
            2 => {
                // swap with the next token
                if i + 1 < n {
                    let next = lexed.tokens[i + 1].span;
                    let (nlo, nhi) = (next.lo as usize, next.hi as usize);
                    let mut t = src[nlo..nhi].to_vec();
                    t.extend_from_slice(&src[hi..nlo]);
                    t.extend_from_slice(&tok);
                    src.splice(lo..nhi, t);
                }
            }
            _ => {
                // replace from the pool
                let repl = REPLACEMENTS[c as usize % REPLACEMENTS.len()];
                src.splice(lo..hi, repl.bytes());
            }
        }
        if src.len() > 1 << 20 {
            break; // keep runaway growth bounded
        }
    }

    let file = sm.intern(Path::new("mutated.lu"));
    let parse = wolf_parse::parse_file(file, &src);

    // The invariants: verifier passes, tree is lossless.
    if let Err(e) = wolf_ast::verify(&parse.root, &src) {
        panic!("tree verifier failed on mutated corpus input: {e}");
    }
    assert_eq!(parse.root.text(&src), src, "lossless round-trip violated");
});
