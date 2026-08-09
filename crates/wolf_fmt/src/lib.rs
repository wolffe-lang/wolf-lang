//! `wolf fmt` — the zero-option formatter over the lossless tree
//! (s11; decisions D34/D36, X4, D22).
//!
//! One canonical style, fixed forever at this layer: width 100,
//! 4-space indent, trailing commas in multiline lists, brace and
//! blank-line policy per spec/01 §7 `[gram.fmt.*]`. There is no
//! configuration — no flags, no attributes, no config file. The escape
//! hatch is raw strings, and nothing else (D34: style wars are a tax
//! wolf never pays).
//!
//! # Pipeline
//!
//! parse (lossless green tree, s08/s09) → doc IR ([`doc`], a tiny
//! Wadler/Prettier algebra) → render at fixed width. Comments ride the
//! tree's trivia: leading comments stay with their construct, trailing
//! comments stay on their line (hand column-alignment before a
//! trailing comment is preserved verbatim), doc comments are never
//! re-flowed, blank lines survive up to the cap (one inside items,
//! exactly one between top-level items).
//!
//! # Broken code
//!
//! Statements containing error nodes — plus a one-statement margin on
//! each side — pass through byte-identical; clean siblings still
//! format ([`FormatOutcome::partial`] reports it, the driver exits
//! nonzero with [`codes::PARTIAL_FORMAT`]). As a belt-and-braces
//! invariant the formatter re-parses its own output and compares
//! normalized trees, comment multisets, and diagnostic codes against
//! the input; any mismatch abandons the run and returns the input
//! unchanged ([`FormatOutcome::fell_back`]) — the formatter can be
//! wrong, but it cannot corrupt.
//!
//! # Stability policy (D36, rustfmt RFC 2437 lineage)
//!
//! The canonical style is versioned: [`STYLE_VERSION`] names the
//! current style and is stamped into bench/report metadata so style
//! churn can never silently invalidate snapshots. Settled formatting
//! (everything exercised by the corpus and this crate's tests) changes
//! only at an edition boundary. Formatting for syntax added after this
//! sprint enters as *unstable* — best effort, may change without an
//! edition — until a sprint explicitly settles it, at which point its
//! shape joins the test suite and freezes.

use std::path::Path;

use wolf_diag::Diagnostic;
use wolf_span::FileId;

pub mod canon;
pub mod doc;
mod lower;

pub use canon::{NTree, comment_multiset, normalize};

/// The canonical-style version stamped into tooling metadata (D36).
/// Bumps only with an edition-gated style change.
pub const STYLE_VERSION: &str = "1";

/// Diagnostic codes the formatter can emit — semantic aliases for the
/// s10 registry entries (`wolf_diag::registry`, the single place a
/// code can be born).
pub mod codes {
    use wolf_diag::{Code, codes as c};

    /// The file had syntax errors: error regions passed through
    /// byte-identical, clean siblings formatted, exit is nonzero.
    pub const PARTIAL_FORMAT: Code = c::W0301;
}

/// The result of formatting one source text.
#[derive(Debug)]
pub struct FormatOutcome {
    /// The formatted text (or the input, byte-identical, when
    /// `fell_back`).
    pub text: Vec<u8>,
    /// Syntax errors were present: error regions passed through
    /// verbatim. The CLI reports [`codes::PARTIAL_FORMAT`] and exits
    /// nonzero.
    pub partial: bool,
    /// The self-check failed and the input was returned unchanged.
    /// Always a formatter bug; property tests assert it never fires on
    /// the corpus.
    pub fell_back: bool,
    /// Parse-tier diagnostics for the *input* (what made `partial`
    /// true).
    pub diagnostics: Vec<Diagnostic>,
}

/// Format `src`. Infallible by construction: any internal invariant
/// failure returns the input unchanged rather than corrupting it.
pub fn format_source(file: FileId, src: &[u8]) -> FormatOutcome {
    let parse = wolf_parse::parse_file(file, src);
    let partial = parse
        .diagnostics
        .iter()
        .any(|d| d.severity == wolf_diag::Severity::Error);

    let (sugar, drop_lets) = canon::region_sugar_rewrites(&parse.root, src);
    let fmt = lower::Fmt {
        src,
        sugar,
        drop_lets,
        consumed: std::cell::RefCell::new(std::collections::HashSet::new()),
    };
    let doc = fmt.source_file(&parse.root);
    let text = doc::render(&doc);

    // The self-check: the output must parse to the same normalized
    // tree, with the same comments and the same diagnostic codes.
    let ok = verify_output(file, src, &parse, &text);
    if !ok {
        return FormatOutcome {
            text: src.to_vec(),
            partial,
            fell_back: true,
            diagnostics: parse.diagnostics,
        };
    }
    FormatOutcome {
        text,
        partial,
        fell_back: false,
        diagnostics: parse.diagnostics,
    }
}

fn verify_output(file: FileId, src: &[u8], parse: &wolf_parse::Parse, out: &[u8]) -> bool {
    let reparse = wolf_parse::parse_file(file, out);
    if normalize(&parse.root, src) != normalize(&reparse.root, out) {
        return false;
    }
    if comment_multiset(file, src) != comment_multiset(file, out) {
        return false;
    }
    let codes = |ds: &[Diagnostic]| {
        let mut v: Vec<&'static str> = ds.iter().map(|d| d.code.as_str()).collect();
        v.sort_unstable();
        v
    };
    codes(&parse.diagnostics) == codes(&reparse.diagnostics)
}

/// The raw render, self-check skipped — for the formatter's own test
/// suite to inspect what the check would reject. Not part of the tool
/// surface.
#[doc(hidden)]
pub fn render_unchecked(src: &[u8]) -> Vec<u8> {
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(Path::new("fmt.lu"));
    let parse = wolf_parse::parse_file(file, src);
    let (sugar, drop_lets) = canon::region_sugar_rewrites(&parse.root, src);
    let fmt = lower::Fmt {
        src,
        sugar,
        drop_lets,
        consumed: std::cell::RefCell::new(std::collections::HashSet::new()),
    };
    doc::render(&fmt.source_file(&parse.root))
}

/// Convenience for tools and tests: format a buffer with a throwaway
/// file id.
pub fn format_text(src: &[u8]) -> FormatOutcome {
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(Path::new("fmt.lu"));
    format_source(file, src)
}
