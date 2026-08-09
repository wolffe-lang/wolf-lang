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
/// notes indented beneath.
pub fn render_diags(parse: &Parse) -> String {
    let mut out = String::new();
    for d in &parse.diagnostics {
        let _ = writeln!(
            out,
            "{} [{:?}] {}..{} {}",
            d.code, d.severity, d.span.lo, d.span.hi, d.message
        );
        for (span, label) in &d.notes {
            let _ = writeln!(out, "    note {}..{} {}", span.lo, span.hi, label);
        }
    }
    out
}

/// The parser diagnostics' codes, in order.
pub fn codes(src: &str) -> Vec<&'static str> {
    parse(src).diagnostics.iter().map(|d| d.code).collect()
}
