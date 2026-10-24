//! C variadic declarations retain the fixed parameter list and the ellipsis.
mod util;

use wolf_ast::FnDecl;

#[test]
fn c_variadic_import_and_export_parse() {
    for src in [
        "extern \"c\" fn printf(fmt: *u8, ...) -> i32\n",
        "export fn sum(n: i32, ...) -> i64 { 0 }\n",
        "extern \"c\" fn sum(n: i32, ...) -> i64 { 0 }\n",
        "export fn sum(n: i32, scale: f64, ...) -> i64 { 0 }\n",
    ] {
        let p = util::parse(src);
        assert!(p.diagnostics.is_empty(), "{src}: {:?}", p.diagnostics);
        let f = p.root.nodes().find_map(FnDecl::cast).unwrap();
        assert!(f.is_variadic());
        let count = f.params().unwrap().params().count();
        assert_eq!(count, if src.contains("scale") { 2 } else { 1 });
    }
}

#[test]
fn ellipsis_is_final_and_membrane_only() {
    for src in [
        "fn f(n: i32, ...) {}\n",
        "extern \"c\" fn f(...)\n",
        "export fn f(n: i32, ..., x: i32) {}\n",
        "export fn f(n: i32, ...,) {}\n",
        "export fn f(n: i32, .. .) {}\n",
        "export fn f(n: i32, ...: i32) {}\n",
    ] {
        let p = util::parse(src);
        assert!(
            p.diagnostics.iter().any(|d| d.code == "E0822"),
            "{src}: {:?}",
            p.diagnostics
        );
    }
    let p = util::parse("fn f(n: i32) {}\n");
    assert!(!p.root.nodes().find_map(FnDecl::cast).unwrap().is_variadic());
}

#[test]
fn e0822_ellipsis_diagnostic() {
    let p = util::parse("export fn f(n: i32, ..., x: i32) {}\n");
    let d = p.diagnostics.iter().find(|d| d.code == "E0822").unwrap();
    insta::assert_snapshot!("e0822_variadic_position", format!("{d:#?}"));
}
