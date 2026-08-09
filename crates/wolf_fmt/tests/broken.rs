//! Broken code passes through (s11 Target 3): error regions — plus a
//! one-statement margin — are emitted byte-identical, clean siblings
//! still format, the outcome is flagged partial, and the W0301
//! diagnostic renders (its reviewed snapshot lives here).

fn fmt(src: &str) -> wolf_fmt::FormatOutcome {
    wolf_fmt::format_text(src.as_bytes())
}

#[test]
fn clean_siblings_format_around_a_broken_item() {
    let src = "fn far() {   1 }\n\nfn near() {   3 }\n\nfn broken( {\n\nfn also_ok() {   2 }\n";
    let out = fmt(src);
    assert!(out.partial, "syntax errors must flag partial");
    let text = String::from_utf8_lossy(&out.text).into_owned();
    if out.fell_back {
        // The safety net may veto sibling formatting when recovery
        // shifts; byte-identical input is the contract then.
        assert_eq!(out.text.as_slice(), src.as_bytes());
        return;
    }
    // The broken region (plus its one-item margin: `near`, `also_ok`)
    // survives byte-identical; the sibling outside the margin
    // reformats.
    assert!(
        text.contains("fn far() { 1 }"),
        "clean sibling did not format: {text}"
    );
    assert!(
        text.contains("fn near() {   3 }"),
        "margin sibling must stay verbatim: {text}"
    );
    assert!(
        text.contains("fn broken( {"),
        "error region not byte-identical: {text}"
    );
}

#[test]
fn broken_statement_passes_through_with_margin() {
    let src = "fn main() {\n    let ok1  =  1\n    let a = 1\n    let broken = (1 +\n    let b = 2\n    let ok2  =  2\n    0\n}\n";
    let out = fmt(src);
    assert!(out.partial);
    if out.fell_back {
        assert_eq!(out.text.as_slice(), src.as_bytes());
        return;
    }
    let text = String::from_utf8_lossy(&out.text).into_owned();
    // The wreck and its one-statement margins are untouched.
    assert!(text.contains("let broken = (1 +"), "{text}");
    // The statement outside the margin still canonicalizes.
    assert!(text.contains("let ok1 = 1"), "{text}");
}

#[test]
fn error_region_bytes_are_identical() {
    let src = "fn main() {\n    good()\n    ????\n    also_good()\n}\n";
    let out = fmt(src);
    assert!(out.partial);
    if !out.fell_back {
        let text = String::from_utf8_lossy(&out.text).into_owned();
        assert!(text.contains("????"), "{text}");
    }
    // Never silently drop the file's content: reformat or return as-is.
    assert!(!out.text.is_empty());
}

#[test]
fn clean_files_are_not_partial() {
    let out = fmt("fn main() { 0 }\n");
    assert!(!out.partial);
    assert!(!out.fell_back);
}

#[test]
fn wolf_parse_broken_suite_never_panics_and_stays_partial() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../wolf_parse/tests/broken");
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).expect("broken suite dir") {
        let p = entry.expect("entry").path();
        if p.extension().is_none_or(|e| e != "lu") {
            continue;
        }
        let src = std::fs::read(&p).expect("read");
        let out = wolf_fmt::format_text(&src);
        assert!(out.partial, "{} must flag partial", p.display());
        if out.fell_back {
            assert_eq!(
                out.text,
                src,
                "{}: fallback must be byte-identical",
                p.display()
            );
        } else {
            // Formatting must be stable even on wrecks.
            let again = wolf_fmt::format_text(&out.text);
            assert_eq!(again.text, out.text, "{} not idempotent", p.display());
        }
        checked += 1;
    }
    assert!(checked >= 10, "broken suite went missing?");
}

/// The reviewed snapshot for W0301 (diag-catalog fixture rule: every
/// code ships with at least one committed snapshot).
#[test]
fn w0301_partial_format_diagnostic_snapshot() {
    let src = b"fn broken( {\n";
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(std::path::Path::new("broken.lu"));
    let out = wolf_fmt::format_source(file, src);
    assert!(out.partial);
    let d = wolf_diag::Diagnostic::warning(
        wolf_fmt::codes::PARTIAL_FORMAT,
        wolf_span::Span::new(file, 0, 1),
        "this file has syntax errors, so it was only partially formatted",
    )
    .with_note(
        "regions with syntax errors (and one statement around them) were left \
         byte-for-byte untouched; fix the parse errors and run `wolf fmt` again",
    );
    let mut sources = wolf_diag::Sources::new();
    sources.add(file, "broken.lu".to_string(), src);
    let rendered = wolf_diag::render_human(&d, &sources, &wolf_diag::RenderOptions::default());
    insta::assert_snapshot!("w0301_partial_format", rendered);
}
