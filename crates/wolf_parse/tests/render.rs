//! Human-renderer snapshots over *real* parser diagnostics — the s10
//! contract's remaining layout cases: a span inside an f-string
//! interpolation (byte-exact via the s07 fragment spans), the D25
//! negative-index hint with its edit preview, and the unclosed-
//! delimiter two-locus shape.

mod util;

use std::path::Path;
use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_span::SourceMap;

fn render_all(src: &str) -> String {
    let mut sm = SourceMap::new();
    let file = sm.intern(Path::new("demo.lu"));
    let parse = wolf_parse::parse_file(file, src.as_bytes());
    assert!(!parse.diagnostics.is_empty(), "expected diagnostics");
    let mut sources = Sources::new();
    sources.add(file, "demo.lu", src.as_bytes());
    let opts = RenderOptions::default();
    let mut out = String::new();
    for d in &parse.diagnostics {
        out.push_str(&render_human(d, &sources, &opts));
        out.push('\n');
    }
    out
}

/// A diagnostic whose span sits *inside* an f-string interpolation —
/// the missing-operand hole in `"{1 + }"` — exact to the byte.
#[test]
fn span_inside_interpolation() {
    insta::assert_snapshot!(
        "render_interp_span",
        render_all("fn f() { let s = \"count: {1 + }\"\n}\n")
    );
}

/// The D25 hint end to end: `s[-1]` renders prose, note, and the
/// `^`-edit preview.
#[test]
fn negative_index_hint() {
    insta::assert_snapshot!("render_e0209", render_all("fn f() { let last = s[-1]\n}\n"));
}

/// The unclosed-delimiter two-locus shape: primary at the opener,
/// secondary where the parser gave up.
#[test]
fn unclosed_paren_two_locus() {
    insta::assert_snapshot!("render_e0202", render_all("fn f(a: int\nfn g() { }\n"));
}
