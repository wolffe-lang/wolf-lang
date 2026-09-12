//! Rendered snapshots for the E06xx error-row family (s15, D30/D22) —
//! every code ships with at least one reviewed fixture (`cargo xtask
//! diag-catalog` enforces the pairing). The catalog case this family
//! exists to beat: Koka-style row mismatches that dump both full rows —
//! wolf renders tags-missing/tags-extra only, however large the rows.

use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

fn render_rows(src: &str) -> String {
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
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
    insta::assert_snapshot!(name, render_rows(src));
}

// ---------------------------------------------------------- E0601 -----

#[test]
fn e0601_duplicate_tag_in_row() {
    snap_one(
        "e0601_duplicate_tag",
        "fn go() -> int ! {Io(str), Io(str)} {\n    7\n}\n\nfn main() -> !int {\n    go() else 0\n}\n",
    );
}

// ---------------------------------------------------------- E0602 -----

/// The headline case: `?` propagates a tag the caller's row lacks —
/// named exactly, with the signature-extending fix-it.
#[test]
fn e0602_missing_tag_with_fixit() {
    snap_one(
        "e0602_missing_tag",
        "fn read(path: str) -> int ! {Io(str), NotFound(str)} {\n    NotFound(path)\n}\n\nfn render(path: str) -> int ! {empty} {\n    read(path)?\n}\n\nfn main() -> !int {\n    render(\"cfg\") else 0\n}\n",
    );
}

/// The catalog degradation case (D22): both rows exceed five tags —
/// the report names the missing tags only, never dumps either row.
#[test]
fn e0602_large_rows_render_tags_only() {
    snap_one(
        "e0602_large_rows",
        "fn deep() -> int ! {A, B, C, D, Q, F, G} {\n    7\n}\n\nfn shallow() -> int ! {H, I, J, K, L, M} {\n    deep()?\n}\n\nfn main() -> !int {\n    shallow() else 0\n}\n",
    );
}

// ---------------------------------------------------------- E0603 -----

#[test]
fn e0603_try_on_infallible() {
    snap_one(
        "e0603_try_infallible",
        "fn seven() -> int {\n    7\n}\n\nfn main() -> !int {\n    seven()?\n}\n",
    );
}

// ---------------------------------------------------------- E0604 -----

#[test]
fn e0604_try_in_infallible_fn() {
    snap_one(
        "e0604_nonfallible_caller",
        "fn may() -> int ! {bad} {\n    bad\n}\n\nfn calm() -> int {\n    may()?\n}\n\nfn main() -> !int {\n    calm()\n}\n",
    );
}

// ---------------------------------------------------------- E0605 -----

#[test]
fn e0605_pub_inferred_row() {
    snap_one(
        "e0605_pub_inferred",
        "/// Booms.\npub fn boom(n: int) -> !int {\n    if n == 0 { return Empty }\n    n\n}\n\nfn main() -> !int {\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0606 -----

#[test]
fn e0606_payload_mismatch_on_shared_tag() {
    snap_one(
        "e0606_payload_mismatch",
        "fn read() -> int ! {NotFound(int)} {\n    NotFound(4)\n}\n\nfn show() -> int ! {NotFound(str)} {\n    read()?\n}\n\nfn main() -> !int {\n    show() else 0\n}\n",
    );
}

// ---------------------------------------------------------- E0609 -----

/// D51: a nested row flattens, so one tag in two layers with two
/// payload shapes cannot collapse — the report names both shapes and
/// the fix-it names the wrapper-type answer.
#[test]
fn e0609_nested_layer_payload_conflict() {
    snap_one(
        "e0609_layer_conflict",
        "fn poke(n: int) -> int ! {Bad(int)} ! {Bad(str)} {\n    n\n}\n\nfn main() -> !int {\n    poke(3) else 0\n}\n",
    );
}

// ---------------------------------------------------------- E0607 -----

#[test]
fn e0607_errdefer_in_infallible_fn() {
    snap_one(
        "e0607_errdefer_infallible",
        "fn tidy() -> int {\n    errdefer print(\"undo\")\n    1\n}\n\nfn main() -> !int {\n    tidy()\n}\n",
    );
}

// ---------------------------------------------------------- E0608 -----

#[test]
fn e0608_else_on_infallible() {
    snap_one(
        "e0608_else_infallible",
        "fn seven() -> int {\n    7\n}\n\nfn main() -> !int {\n    seven() else 0\n}\n",
    );
}

// ---------------------------------------------------------- E0610 -----

/// s158 (`[type.err.alias.cycle]`, wolf-lang#36): a cycle among
/// error-set aliases is E0610, reported ONCE, at the alias that closes
/// it, with the loop named in full — the sibling of E0503 for trait
/// aliases and E0513 for associated-type bindings.
#[test]
fn e0610_error_set_alias_cycle() {
    insta::assert_snapshot!(
        "e0610_error_alias_cycle",
        render_rows(
            "error A = {B, io}\nerror B = {A, parse}\n\nfn f() -> int ! A {\n    0\n}\n\nfn main() -> !int {\n    f() else 0\n}\n"
        )
    );
}

/// An alias entry names tags; it does not declare one, so it carries no
/// payload of its own (E0601, `[type.err.alias.union]`).
#[test]
fn e0601_error_set_alias_entry_takes_no_payload() {
    insta::assert_snapshot!(
        "e0601_error_alias_payload",
        render_rows(
            "error IoErrors = {none, parse}\n\nfn f() -> int ! {IoErrors(int)} {\n    0\n}\n\nfn main() -> !int {\n    f() else 0\n}\n"
        )
    );
}

/// Transparency, both directions and to any terminating depth
/// ([type.err.alias.transparent]): the aliased signature and the
/// spelled-out row are the same type, `?` crosses between them with no
/// conversion, and a chain of aliases flattens to its tags.
#[test]
fn error_set_aliases_are_transparent_both_ways() {
    assert_eq!(
        render_rows(
            "error Io = {none, parse}\nerror Config = {Io, closed}\n\nfn spelled(ok: bool) -> int ! {none, parse, closed} {\n    if ok { 1 } else { return closed }\n}\n\nfn aliased(ok: bool) -> int ! Config {\n    spelled(ok)?\n}\n\nfn back(ok: bool) -> int ! {none, parse, closed} {\n    aliased(ok)?\n}\n\nfn main() -> !int {\n    back(true) else 0\n}\n"
        ),
        ""
    );
}
