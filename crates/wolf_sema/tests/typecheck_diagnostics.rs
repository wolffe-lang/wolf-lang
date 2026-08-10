//! Rendered snapshots for the E04xx type-checker family — every code
//! ships with at least one reviewed fixture (`cargo xtask diag-catalog`
//! enforces the pairing), and the error-message catalog accepts
//! confusing-error regression cases from here on: these are mined from
//! the classics (Elm's if-branch mismatch and argument-vs-return
//! confusion, Rust's wrong-arg-count and field typos), reviewed as
//! artifacts (D22).

use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

fn render_types(files: &[(&[&str], &str, &str)]) -> String {
    let mut ml = MemoryLoader::new("snap");
    for (m, n, s) in files {
        ml.add_file(m, n, s);
    }
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics.is_empty(),
        "snapshot inputs resolve clean: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    let mut out = String::new();
    for d in &tc.diagnostics {
        out.push_str(&render_human(d, &sources, &RenderOptions::default()));
        out.push('\n');
    }
    out
}

fn snap_one(name: &str, src: &str) {
    insta::assert_snapshot!(name, render_types(&[(&[], "main.lu", src)]));
}

// ---------------------------------------------------------- E0401 -----

/// Catalog case 1 (the Elm classic): if branches disagree — neither
/// side is "expected".
#[test]
fn e0401_if_branch_mismatch() {
    snap_one(
        "e0401_if_branches",
        "fn main() -> !int {\n    let label = if true { 1 } else { \"one\" }\n    0\n}\n",
    );
}

/// Catalog case 2: return-type mismatch with the because chain.
#[test]
fn e0401_return_mismatch_provenance() {
    snap_one(
        "e0401_return_provenance",
        "fn answer() -> int {\n    \"forty-two\"\n}\n\nfn main() -> !int {\n    answer()\n}\n",
    );
}

/// Catalog case 3 (argument-vs-return confusion): passing a function
/// where its *result* was meant — the fn-type-vs-value rendering must
/// make the missing `()` obvious.
#[test]
fn e0401_argument_vs_return_confusion() {
    snap_one(
        "e0401_arg_vs_return",
        "fn get_num() -> int {\n    41\n}\n\nfn add(a: int, b: int) -> int {\n    a + b\n}\n\nfn main() -> !int {\n    add(get_num, 1)\n}\n",
    );
}

/// Catalog case 4 (deep diff): large function types differing in one
/// parameter — the diff names the part, not the wall of text.
#[test]
fn e0401_deep_fn_type_diff() {
    snap_one(
        "e0401_deep_diff",
        "fn fold(step: fn(int, str, bool) -> int, seed: int) -> int {\n    seed\n}\n\nfn step3(a: int, b: str, c: int) -> int {\n    a\n}\n\nfn main() -> !int {\n    fold(step3, 0)\n}\n",
    );
}

/// Catalog case 5: int vs float never converts silently; the hint
/// names the explicit `as`.
#[test]
fn e0401_int_float_no_implicit_conversion() {
    snap_one(
        "e0401_int_vs_float",
        "fn half(x: f64) -> f64 {\n    x / 2.0\n}\n\nfn main() -> !int {\n    let n = 7\n    let h = half(n as f64)\n    let bad = half(n)\n    0\n}\n",
    );
}

/// Catalog case 6: the truthiness classic — a number where an `if`
/// condition needs `bool`.
#[test]
fn e0401_condition_needs_bool() {
    snap_one(
        "e0401_truthiness",
        "fn main() -> !int {\n    let count = 3\n    if count { 0 } else { 1 }\n}\n",
    );
}

/// Catalog case 7: `let` annotation as the provenance locus.
#[test]
fn e0401_let_annotation_provenance() {
    snap_one(
        "e0401_let_annotation",
        "fn main() -> !int {\n    let limit: int = \"ten\"\n    0\n}\n",
    );
}

/// Catalog case 8: match arms disagree (the honest both-arms framing).
#[test]
fn e0401_match_arms_disagree() {
    snap_one(
        "e0401_match_arms",
        "fn main() -> !int {\n    let v = match 2 {\n        0 => 10,\n        _ => \"lots\",\n    }\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0402 -----

/// Catalog case 9 (the Rust classic): wrong argument count, definition
/// site shown.
#[test]
fn e0402_wrong_arg_count() {
    snap_one(
        "e0402_arg_count",
        "fn area(width: int, height: int) -> int {\n    width * height\n}\n\nfn main() -> !int {\n    area(3)\n}\n",
    );
}

// ---------------------------------------------------------- E0403 -----

/// Catalog case 10: field typo with suggestion and definition site.
#[test]
fn e0403_unknown_field_typo() {
    snap_one(
        "e0403_field_typo",
        "struct Circle {\n    radius: f64,\n}\n\nfn main() -> !int {\n    let c = Circle { radius: 1.0 }\n    let r = c.radbus\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0404 -----

#[test]
fn e0404_infinite_type() {
    snap_one(
        "e0404_infinite",
        "fn main() -> !int {\n    let f = fn(g) g(g)\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0405 -----

#[test]
fn e0405_uninferable_closure_param() {
    snap_one(
        "e0405_cannot_infer",
        "fn main() -> !int {\n    let f = fn(x) x\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0406 -----

#[test]
fn e0406_struct_called_like_fn() {
    snap_one(
        "e0406_not_callable",
        "struct Point {\n    x: int,\n    y: int,\n}\n\nfn main() -> !int {\n    let p = Point(1, 2)\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0407 -----

#[test]
fn e0407_unannotated_global() {
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(
        &[],
        "main.lu",
        "let banner = \"hello\"\n\nfn main() -> !int {\n    0\n}\n",
    );
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let tc = typecheck_package_with(&res.package, true);
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    let mut out = String::new();
    for d in &tc.diagnostics {
        out.push_str(&render_human(d, &sources, &RenderOptions::default()));
        out.push('\n');
    }
    insta::assert_snapshot!("e0407_missing_annotation", out);
}

// ---------------------------------------------------------- E0408 -----

#[test]
fn e0408_missing_struct_field() {
    snap_one(
        "e0408_missing_field",
        "struct Config {\n    host: str,\n    port: int,\n}\n\nfn main() -> !int {\n    let c = Config { host: \"wolf\" }\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0409 -----

/// Catalog case: `+` does not join strings — interpolation does.
#[test]
fn e0409_string_plus_string() {
    snap_one(
        "e0409_string_plus",
        "fn main() -> !int {\n    let first = \"wo\"\n    let both = first + \"lf\"\n    0\n}\n",
    );
}

#[test]
fn e0409_logic_on_numbers() {
    snap_one(
        "e0409_logic_on_int",
        "fn main() -> !int {\n    let ok = 1 && 0\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0410 -----

/// Issue #2: `let` reassignment slipped past both implementations.
/// The fix-it rewrites `let` to `var`; the secondary span pins the
/// immutable binding site.
#[test]
fn e0410_let_reassign() {
    snap_one(
        "e0410_let_reassign",
        "fn main() -> !int {\n    let x = 1\n    x = 2\n    0\n}\n",
    );
}

/// Compound assignment is assignment: `+=` on a `let` binding fails
/// the same way.
#[test]
fn e0410_let_compound_assign() {
    snap_one(
        "e0410_compound",
        "fn main() -> !int {\n    let total = 10\n    total += 5\n    0\n}\n",
    );
}

/// An item-level `let` global is immutable too ([gram.item.let]:
/// item-level and statement-level share the grammar).
#[test]
fn e0410_global_let_assign() {
    snap_one(
        "e0410_global",
        "let limit: int = 8\n\nfn main() -> !int {\n    limit = 9\n    0\n}\n",
    );
}

/// The non-cases: `var` reassigns fine; a `var` shadowing a `let`
/// assigns fine; a parameter is not a `let` binding; a fresh `let`
/// *shadowing* is not an assignment.
#[test]
fn e0410_non_cases_stay_clean() {
    let src = "fn bump(n: int) -> int {\n    var m = n\n    m = m + 1\n    let k = m\n    let k = k + 1\n    var k = k\n    k += 2\n    k\n}\n\nfn main() -> !int {\n    bump(1)\n    0\n}\n";
    assert_eq!(render_types(&[(&[], "main.lu", src)]), "");
}
