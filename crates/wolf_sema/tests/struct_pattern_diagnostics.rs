//! Rendered snapshots for `[gram.pat.struct]`'s field-list rules
//! (s129, #179): E0814 in each of its three directions — a missing
//! field without `..`, a duplicated field, an empty field list — plus
//! the E0403 unknown-field render in pattern position (the code
//! member access and struct literals already own) and the clean twin.
//! Reviewed artifacts, one per shape (D22; the diag-catalog fixture
//! rule).

use wolf_diag::{RenderOptions, Sources};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

fn render_types(src: &str) -> String {
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "snapshot inputs resolve without errors: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    let mut all: Vec<wolf_diag::Diagnostic> = res
        .diagnostics
        .iter()
        .chain(tc.diagnostics.iter())
        .cloned()
        .collect();
    wolf_diag::sort_diagnostics(&mut all);
    let mut reporter = wolf_diag::HumanReporter::new(&sources, RenderOptions::default());
    for d in &all {
        wolf_diag::Reporter::report(&mut reporter, d);
    }
    let mut out = wolf_diag::Reporter::take_output(&mut reporter);
    if out.is_empty() {
        out.push_str("(clean)");
    }
    out
}

const POINT: &str = "struct Point {\n    x: int,\n    y: int,\n}\n\n";

#[test]
fn e0814_missing_field_without_rest() {
    insta::assert_snapshot!(
        "e0814_missing_field",
        render_types(&format!(
            "{POINT}fn main() -> !int {{\n    let p = Point {{ x: 1, y: 2 }}\n    \
             let Point {{ x }} = p\n    x\n}}\n"
        ))
    );
}

#[test]
fn e0814_duplicate_field() {
    insta::assert_snapshot!(
        "e0814_duplicate_field",
        render_types(&format!(
            "{POINT}fn main() -> !int {{\n    let p = Point {{ x: 1, y: 2 }}\n    \
             let Point {{ x, x, .. }} = p\n    x\n}}\n"
        ))
    );
}

#[test]
fn e0814_empty_field_list() {
    insta::assert_snapshot!(
        "e0814_empty",
        render_types(&format!(
            "{POINT}fn main() -> !int {{\n    let p = Point {{ x: 1, y: 2 }}\n    \
             let Point {{ .. }} = p\n    0\n}}\n"
        ))
    );
}

#[test]
fn e0403_unknown_field_in_pattern() {
    insta::assert_snapshot!(
        "e0403_pattern_unknown_field",
        render_types(&format!(
            "{POINT}fn main() -> !int {{\n    let p = Point {{ x: 1, y: 2 }}\n    \
             let Point {{ x, z, .. }} = p\n    x\n}}\n"
        ))
    );
}

#[test]
fn clean_struct_patterns_bind() {
    insta::assert_snapshot!(
        "struct_pattern_clean",
        render_types(&format!(
            "{POINT}fn main() -> !int {{\n    let p = Point {{ x: 1, y: 2 }}\n    \
             let Point {{ x, y: b }} = p\n    let Point {{ x: a, .. }} = p\n    \
             a + x + b\n}}\n"
        ))
    );
}
