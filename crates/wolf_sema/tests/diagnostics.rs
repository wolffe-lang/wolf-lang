//! Rendered snapshots for the E03xx resolution family — every code
//! ships with at least one reviewed fixture (s10 catalog discipline;
//! `cargo xtask diag-catalog` enforces the pairing).

use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_sema::{AliasTable, MemoryLoader, Resolution, resolve_package_with};

fn resolve(files: &[(&[&str], &str, &str)]) -> Resolution {
    let mut ml = MemoryLoader::new("snap");
    for (m, n, s) in files {
        ml.add_file(m, n, s);
    }
    resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads")
}

fn render(res: &Resolution) -> String {
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    let mut out = String::new();
    for d in &res.diagnostics {
        out.push_str(&render_human(d, &sources, &RenderOptions::default()));
        out.push('\n');
    }
    out
}

#[test]
fn e0301_unresolved_name_with_typo_suggestion() {
    let res = resolve(&[(
        &[],
        "main.lu",
        "fn main() -> !int {\n    pirnt(\"hello\")\n    0\n}\n",
    )]);
    insta::assert_snapshot!("e0301_typo", render(&res));
}

#[test]
fn e0301_missing_module_item() {
    let res = resolve(&[
        (
            &[],
            "main.lu",
            "use geometry\n\nfn main() -> !int {\n    geometry.aera(3)\n}\n",
        ),
        (
            &["geometry"],
            "shapes.lu",
            "pub fn area(side: int) -> int {\n    side * side\n}\n",
        ),
    ]);
    insta::assert_snapshot!("e0301_member", render(&res));
}

#[test]
fn e0302_duplicate_definition_across_files() {
    let res = resolve(&[
        (
            &[],
            "main.lu",
            "fn helper() -> int {\n    1\n}\n\nfn main() -> !int {\n    helper()\n}\n",
        ),
        (&[], "twice.lu", "fn helper() -> int {\n    2\n}\n"),
    ]);
    insta::assert_snapshot!("e0302_duplicate", render(&res));
}

#[test]
fn e0303_import_cycle_full_path() {
    let res = resolve(&[
        (
            &[],
            "main.lu",
            "use alpha\n\nfn main() -> !int {\n    alpha.ping(3)\n}\n",
        ),
        (
            &["alpha"],
            "a.lu",
            "use beta\n\npub fn ping(n: int) -> int {\n    beta.pong(n)\n}\n",
        ),
        (
            &["beta"],
            "b.lu",
            "use alpha\n\npub fn pong(n: int) -> int {\n    alpha.ping(n)\n}\n",
        ),
    ]);
    insta::assert_snapshot!("e0303_cycle", render(&res));
}

#[test]
fn e0304_private_item_names_required_visibility() {
    let res = resolve(&[
        (
            &[],
            "main.lu",
            "use vault\n\nfn main() -> !int {\n    vault.secret()\n}\n",
        ),
        (
            &["vault"],
            "keys.lu",
            "fn secret() -> int {\n    41\n}\n\npub fn open() -> int {\n    secret()\n}\n",
        ),
    ]);
    insta::assert_snapshot!("e0304_private", render(&res));
}

#[test]
fn e0305_unused_import_machine_applicable_fix() {
    let res = resolve(&[
        (
            &[],
            "main.lu",
            "use tools\n\nfn main() -> !int {\n    0\n}\n",
        ),
        (
            &["tools"],
            "kit.lu",
            "pub fn sharpen() -> int {\n    1\n}\n",
        ),
    ]);
    insta::assert_snapshot!("e0305_unused", render(&res));
}

#[test]
fn e0306_duplicate_import() {
    let res = resolve(&[
        (
            &[],
            "main.lu",
            "use tools\nuse gear as tools\n\nfn main() -> !int {\n    tools.sharpen()\n}\n",
        ),
        (
            &["tools"],
            "kit.lu",
            "pub fn sharpen() -> int {\n    1\n}\n",
        ),
        (&["gear"], "g.lu", "pub fn sharpen() -> int {\n    2\n}\n"),
    ]);
    insta::assert_snapshot!("e0306_duplicate_import", render(&res));
}

// ------------------------------------------------------------ E0410 ---
// Moved here from the typecheck suite at s29 (DIV-2026-010): `let`
// immutability is binding structure — lexical knowledge — so E0410 is
// the resolve rung's law, matching the interpreter's placement.

/// Issue #2: `let` reassignment slipped past both implementations.
/// The fix-it rewrites `let` to `var`; the secondary span pins the
/// immutable binding site.
#[test]
fn e0410_let_reassign() {
    let res = resolve(&[(
        &[],
        "main.lu",
        "fn main() -> !int {\n    let x = 1\n    x = 2\n    0\n}\n",
    )]);
    insta::assert_snapshot!("e0410_let_reassign", render(&res));
}

/// Compound assignment is assignment: `+=` on a `let` binding fails
/// the same way.
#[test]
fn e0410_let_compound_assign() {
    let res = resolve(&[(
        &[],
        "main.lu",
        "fn main() -> !int {\n    let total = 10\n    total += 5\n    0\n}\n",
    )]);
    insta::assert_snapshot!("e0410_compound", render(&res));
}

/// An item-level `let` global is immutable too ([gram.item.let]:
/// item-level and statement-level share the grammar).
#[test]
fn e0410_global_let_assign() {
    let res = resolve(&[(
        &[],
        "main.lu",
        "let limit: int = 8\n\nfn main() -> !int {\n    limit = 9\n    0\n}\n",
    )]);
    insta::assert_snapshot!("e0410_global", render(&res));
}

/// The non-cases: `var` reassigns fine; a `var` shadowing a `let`
/// assigns fine; a parameter is not a `let` binding; a fresh `let`
/// *shadowing* is not an assignment.
#[test]
fn e0410_non_cases_stay_clean() {
    let src = "fn bump(n: int) -> int {\n    var m = n\n    m = m + 1\n    let k = m\n    let k = k + 1\n    var k = k\n    k += 2\n    k\n}\n\nfn main() -> !int {\n    bump(1)\n    0\n}\n";
    let res = resolve(&[(&[], "main.lu", src)]);
    assert_eq!(render(&res), "");
}

/// The E0410 walk sees through control-flow scopes: a `let` captured
/// by shadowing inside a `for` body still rejects, the loop pattern
/// itself is assignable, and an `else |e|` handler binding is not a
/// `let`.
#[test]
fn e0410_scope_shapes_stay_clean() {
    let src = "fn main() -> !int {\n    var acc = 0\n    for i in 0..3 {\n        i = i + 1\n        acc += i\n    }\n    let cap = 9\n    if acc > cap {\n        var cap = acc\n        cap = 0\n        acc = cap\n    }\n    0\n}\n";
    let res = resolve(&[(&[], "main.lu", src)]);
    assert_eq!(render(&res), "");
}
