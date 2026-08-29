//! The s126 origin-marker scan (D61 `[gram.attr.index]`): reviewed
//! snapshots for E0813 and W0317 (the diag-catalog fixture rule), and
//! the region semantics of `#[index(…)]` — innermost lexical scope
//! wins, absence means origin 0.

use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_sema::{
    AliasTable, MemoryLoader, Resolution, resolve_package_with, scan_origins,
    typecheck_package_with,
};

fn resolve(src: &str) -> Resolution {
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads")
}

fn render(res: &Resolution, diags: &[wolf_diag::Diagnostic]) -> String {
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    let mut out = String::new();
    for d in diags {
        out.push_str(&render_human(d, &sources, &RenderOptions::default()));
        out.push('\n');
    }
    out
}

fn render_scan(src: &str) -> String {
    let res = resolve(src);
    let scan = scan_origins(&res.package);
    render(&res, &scan.diagnostics)
}

#[test]
fn e0813_origin_out_of_range() {
    insta::assert_snapshot!(
        "e0813_origin_seven",
        render_scan("#![index(7)]\nfn f() -> int {\n    1\n}\n")
    );
}

#[test]
fn e0813_unknown_inner_attribute() {
    insta::assert_snapshot!(
        "e0813_unknown_inner",
        render_scan("#![consttime]\nfn f() -> int {\n    1\n}\n")
    );
}

#[test]
fn e0813_marker_without_an_origin() {
    insta::assert_snapshot!(
        "e0813_no_origin",
        render_scan("fn f() -> int {\n    #[index]\n    let x = 1\n    x\n}\n")
    );
}

#[test]
fn e0813_duplicate_marker() {
    insta::assert_snapshot!(
        "e0813_duplicate",
        render_scan("fn f() -> int {\n    #[index(1), index(0)]\n    let x = 1\n    x\n}\n")
    );
}

#[test]
fn w0317_get_literal_in_one_origin_scope() {
    let res = resolve(
        "#![index(1)]\nfn f() -> int {\n    var xs = List[int]()\n    (mut xs).push(5)\n    \
         xs.get(1) else { 0 - 1 }\n}\n",
    );
    let tc = typecheck_package_with(&res.package, true);
    let warns: Vec<_> = tc
        .diagnostics
        .iter()
        .filter(|d| d.code.as_str() == "W0317")
        .cloned()
        .collect();
    assert_eq!(warns.len(), 1, "exactly one W0317: {:?}", tc.diagnostics);
    insta::assert_snapshot!("w0317_get_literal", render(&res, &warns));
}

// ------------------------------------------------------- the region law --

#[test]
fn origin_regions_innermost_wins_and_absence_is_zero() {
    let src = "fn f() -> int {\n    #[index(1)]\n    {\n        #[index(0)]\n        {\n            \
               let a = 1\n        }\n        let b = 2\n    }\n    3\n}\n";
    let res = resolve(src);
    let scan = scan_origins(&res.package);
    assert!(scan.diagnostics.is_empty(), "{:?}", scan.diagnostics);
    let file = res.package.files[0].raw.file;
    let at = |pat: &str| {
        let lo = src.find(pat).expect("pattern") as u32;
        wolf_span::Span::new(file, lo, lo + pat.len() as u32)
    };
    // `let a` sits in the inner 0-region, `let b` in the outer
    // 1-region, `3` outside both.
    assert_eq!(scan.map.origin_at(at("let a")), 0);
    assert_eq!(scan.map.origin_at(at("let b")), 1);
    assert_eq!(scan.map.origin_at(at("3\n")), 0);
}

#[test]
fn origin_file_default_reaches_everything_and_marks_nothing_extra() {
    let src = "#![index(1)]\nfn f() -> int {\n    #[index(0)]\n    let a = 1\n    a\n}\n";
    let res = resolve(src);
    let scan = scan_origins(&res.package);
    assert!(scan.diagnostics.is_empty(), "{:?}", scan.diagnostics);
    let file = res.package.files[0].raw.file;
    let at = |pat: &str| {
        let lo = src.find(pat).expect("pattern") as u32;
        wolf_span::Span::new(file, lo, lo + pat.len() as u32)
    };
    assert_eq!(scan.map.origin_at(at("let a")), 0, "statement marker wins");
    assert_eq!(scan.map.origin_at(at("a\n}")), 1, "file default elsewhere");
    // An unmarked package scans to an empty map — the zero-cost path.
    let clean = resolve("fn f() -> int {\n    1\n}\n");
    assert!(scan_origins(&clean.package).map.is_empty());
}
