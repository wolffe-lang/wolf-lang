//! Shared helpers for wolf_parse's integration tests.

#![allow(dead_code)]

use std::fmt::Write;
use std::path::Path;
use wolf_parse::Parse;
use wolf_span::SourceMap;

/// Lex + parse `src`; every call re-checks the tree invariants
/// (lossless text, token tiling, span nesting).
pub fn parse(src: &str) -> Parse {
    parse_bytes(src.as_bytes())
}

pub fn parse_bytes(src: &[u8]) -> Parse {
    let mut sm = SourceMap::new();
    let file = sm.intern(Path::new("test.lu"));
    let parse = wolf_parse::parse_file(file, src);
    wolf_ast::verify(&parse.root, src).expect("tree verifier");
    assert_eq!(parse.root.text(src), src, "lossless invariant violated");
    parse
}

/// Diagnostics, one per line: `code [severity] lo..hi message` with
/// labels, secondary spans, notes, and suggestions indented beneath —
/// the full structured value, so snapshot review sees everything.
pub fn render_diags(parse: &Parse) -> String {
    let mut out = String::new();
    for d in &parse.diagnostics {
        let _ = writeln!(
            out,
            "{} [{:?}] {}..{} {}",
            d.code,
            d.severity,
            d.span().lo,
            d.span().hi,
            d.message
        );
        if !d.primary.label.is_empty() {
            let _ = writeln!(out, "    label {}", d.primary.label);
        }
        for s in &d.secondary {
            let _ = writeln!(
                out,
                "    secondary {}..{} {}",
                s.span.lo, s.span.hi, s.label
            );
        }
        for n in &d.notes {
            let _ = writeln!(out, "    note {n}");
        }
        for s in &d.suggestions {
            let _ = writeln!(out, "    help [{:?}] {}", s.applicability, s.message);
            for (span, replacement) in &s.edits {
                let _ = writeln!(
                    out,
                    "        edit {}..{} -> {:?}",
                    span.lo, span.hi, replacement
                );
            }
        }
    }
    out
}

/// The parser diagnostics' codes, in order.
pub fn codes(src: &str) -> Vec<&'static str> {
    parse(src)
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect()
}
