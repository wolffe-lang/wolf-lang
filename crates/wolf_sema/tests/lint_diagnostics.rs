//! The s67 allow-scan: reviewed snapshots for W0302/W0303 (the
//! diag-catalog fixture rule) and the region semantics of
//! `#[allow(…)]` — item-granular, body included.

use wolf_diag::lint::Selector;
use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_sema::{AliasTable, MemoryLoader, Resolution, resolve_package_with, scan_allows};

fn resolve(src: &str) -> Resolution {
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads")
}

fn render_scan(src: &str) -> String {
    let res = resolve(src);
    assert!(
        !res.diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error),
        "fixture must resolve clean"
    );
    let scan = scan_allows(&res.package);
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    let mut out = String::new();
    for d in &scan.diagnostics {
        out.push_str(&render_human(d, &sources, &RenderOptions::default()));
        out.push('\n');
    }
    out
}

#[test]
fn w0302_unknown_code_in_allow() {
    insta::assert_snapshot!(
        "w0302_unknown_code",
        render_scan("#[allow(w9999)]\nfn f() -> int {\n    1\n}\n")
    );
}

#[test]
fn w0303_allow_of_nothing() {
    insta::assert_snapshot!(
        "w0303_allow_nothing",
        render_scan("#[allow]\nfn f() -> int {\n    1\n}\n")
    );
}

#[test]
fn allow_regions_cover_the_attributed_item() {
    let src = "#[allow(w1301, w13xx)]\nfn f() -> int {\n    1\n}\n\nfn g() -> int {\n    2\n}\n";
    let res = resolve(src);
    let scan = scan_allows(&res.package);
    assert!(scan.diagnostics.is_empty(), "both selectors are valid");
    assert_eq!(scan.allows.len(), 2);
    assert_eq!(scan.allows[0].selector, Selector::Code("W1301".into()));
    assert_eq!(scan.allows[1].selector, Selector::Family("W13".into()));
    // The region covers f's whole declaration (attribute through body)
    // and stops before g.
    let body_pos = src.find("    1").expect("body") as u32;
    let g_pos = src.find("fn g").expect("g") as u32;
    let r = scan.allows[0].span;
    assert!(r.lo <= body_pos && body_pos < r.hi, "body inside region");
    assert!(r.hi <= g_pos, "next item outside region");
}

#[test]
fn clean_files_scan_empty() {
    let res = resolve("fn f() -> int {\n    1\n}\n");
    let scan = scan_allows(&res.package);
    assert!(scan.allows.is_empty());
    assert!(scan.diagnostics.is_empty());
}
