//! Human-renderer snapshots over *real* lexer diagnostics — the s07
//! shapes the s10 contract names: the dedent-margin two-locus case
//! (primary on the offending line, secondary on the margin) and a
//! machine-applicable suggestion born in the lexer.

mod util;

use std::path::Path;
use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_span::SourceMap;

fn render_all(src: &str) -> String {
    let mut sm = SourceMap::new();
    let file = sm.intern(Path::new("demo.lu"));
    let lexed = wolf_lex::lex(file, src.as_bytes());
    assert!(!lexed.diagnostics.is_empty(), "expected diagnostics");
    let mut sources = Sources::new();
    sources.add(file, "demo.lu", src.as_bytes());
    let opts = RenderOptions::default();
    let mut out = String::new();
    for d in &lexed.diagnostics {
        out.push_str(&render_human(d, &sources, &opts));
        out.push('\n');
    }
    out
}

/// The s07 dedent diagnostic: two loci — the under-indented line and
/// the closing `"""` whose whitespace sets the margin.
#[test]
fn e0104_dedent_two_locus() {
    insta::assert_snapshot!(
        "render_e0104_two_locus",
        render_all("let p = \"\"\"\n    good\n  bad\n    \"\"\"\n")
    );
}

/// A lexer-born machine-applicable suggestion: lone `}` in a string.
#[test]
fn e0107_lone_brace_suggestion() {
    insta::assert_snapshot!("render_e0107_lone_brace", render_all("let s = \"a}b\"\n"));
}

/// The unterminated string: label on the opening quote, insert-`"` fix.
#[test]
fn e0102_unterminated() {
    insta::assert_snapshot!(
        "render_e0102_unterminated",
        render_all("let s = \"abc\nlet t = 1\n")
    );
}
