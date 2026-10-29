//! #947: ledger diagnostics have the ordinary file/line/column renderer.
use std::path::Path;
use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_span::{SourceMap, Span};

#[test]
fn lent_value_refusal_has_a_reviewed_source_site_and_fix_pattern() {
    let src = "fn consume(view: List[byte]) {\n    let owned = view\n}\n";
    let mut sm = SourceMap::new();
    let file = sm.intern(Path::new("lend.lu"));
    let mut sources = Sources::new();
    sources.add(file, "lend.lu", src.as_bytes());
    let lo = src.rfind("view").unwrap() as u32;
    let construct = "a lent `bytes()` view in a value position (bind it with `let` to materialize)";
    let diag = wolf_sema::ledger_diagnostic(construct, Span::new(file, lo, lo + 4)).unwrap();
    assert_eq!(diag.code.as_str(), "E1016");
    insta::assert_snapshot!(
        "lent_value_refusal",
        render_human(&diag, &sources, &RenderOptions::default())
    );
    for construct in [
        "assignment to a lent `bytes()` view (a byte view is read-only)",
        "a lent `bytes()` view captured by a task (bind it with `let` to materialize)",
    ] {
        assert!(wolf_sema::ledger_diagnostic(construct, Span::new(file, lo, lo + 4)).is_some());
    }
    assert!(
        wolf_sema::ledger_diagnostic(
            "methods on builtin types (the std surface)",
            Span::new(file, lo, lo + 4)
        )
        .is_none()
    );
}
