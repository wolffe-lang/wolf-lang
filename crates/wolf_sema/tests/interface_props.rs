//! The `wolfi` stability properties (s12 acceptance): byte determinism
//! across independent builds, and the two-hash invariance matrix —
//! body edits move neither hash, `pub` signature edits move both,
//! `pub(pkg)`-only edits move only `pkg_hash`, and whitespace / file
//! order / item order move nothing at all.

use wolf_sema::{
    AliasTable, Interface, MemoryLoader, build_interfaces, decode, encode, load_package,
};

fn interfaces(files: &[(&[&str], &str, &str)]) -> Vec<Interface> {
    let mut ml = MemoryLoader::new("prop");
    for (m, n, s) in files {
        ml.add_file(m, n, s);
    }
    let pkg = load_package(&mut ml, &AliasTable::default()).expect("root loads");
    assert!(
        pkg.diagnostics.is_empty(),
        "property packages resolve clean: {:?}",
        pkg.diagnostics
    );
    build_interfaces(&pkg)
}

fn geometry(ifaces: &[Interface]) -> &Interface {
    ifaces
        .iter()
        .find(|i| i.module_path == ["geometry"])
        .expect("geometry module present")
}

const BASE_MAIN: (&[&str], &str, &str) = (
    &[],
    "main.lu",
    "use geometry\n\nfn main() -> !int {\n    if geometry.area(3) == 9 { 0 } else { 1 }\n}\n",
);

const BASE_GEO: (&[&str], &str, &str) = (
    &["geometry"],
    "shapes.lu",
    "pub fn area(side: int) -> int {\n    side * side\n}\n\
     pub(pkg) fn perimeter(side: int) -> int {\n    4 * side\n}\n\
     fn hidden(side: int) -> int {\n    side\n}\n",
);

#[test]
fn independent_rebuilds_are_byte_identical() {
    let a = interfaces(&[BASE_MAIN, BASE_GEO]);
    let b = interfaces(&[BASE_MAIN, BASE_GEO]);
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(encode(x), encode(y), "wolfi emission must be deterministic");
    }
}

#[test]
fn loader_round_trips_every_module() {
    for iface in interfaces(&[BASE_MAIN, BASE_GEO]) {
        let bytes = encode(&iface);
        let back = decode(&bytes).expect("loader reads what emission wrote");
        assert_eq!(back, iface);
        assert_eq!(encode(&back), bytes);
    }
}

#[test]
fn body_edit_changes_neither_hash() {
    let base = interfaces(&[BASE_MAIN, BASE_GEO]);
    let edited = interfaces(&[
        BASE_MAIN,
        (
            &["geometry"],
            "shapes.lu",
            "pub fn area(side: int) -> int {\n    let s = side\n    s * s\n}\n\
             pub(pkg) fn perimeter(side: int) -> int {\n    side * 4\n}\n\
             fn hidden(side: int) -> int {\n    side + 1\n}\n",
        ),
    ]);
    let (a, b) = (geometry(&base), geometry(&edited));
    assert_eq!(a.export_hash, b.export_hash, "body edits are invisible");
    assert_eq!(a.pkg_hash, b.pkg_hash, "body edits are invisible");
    assert_eq!(encode(a), encode(b), "the whole artifact is body-blind");
}

#[test]
fn pub_signature_edit_changes_both_hashes() {
    let base = interfaces(&[BASE_MAIN, BASE_GEO]);
    let edited = interfaces(&[
        BASE_MAIN,
        (
            &["geometry"],
            "shapes.lu",
            "pub fn area(side: i64) -> i64 {\n    side * side\n}\n\
             pub(pkg) fn perimeter(side: int) -> int {\n    4 * side\n}\n\
             fn hidden(side: int) -> int {\n    side\n}\n",
        ),
    ]);
    let (a, b) = (geometry(&base), geometry(&edited));
    assert_ne!(a.export_hash, b.export_hash, "exported signature changed");
    assert_ne!(a.pkg_hash, b.pkg_hash, "the package partition sees it too");
}

#[test]
fn pkg_only_edit_changes_only_pkg_hash() {
    let base = interfaces(&[BASE_MAIN, BASE_GEO]);
    let edited = interfaces(&[
        BASE_MAIN,
        (
            &["geometry"],
            "shapes.lu",
            "pub fn area(side: int) -> int {\n    side * side\n}\n\
             pub(pkg) fn perimeter(side: i64) -> i64 {\n    4 * side\n}\n\
             fn hidden(side: int) -> int {\n    side\n}\n",
        ),
    ]);
    let (a, b) = (geometry(&base), geometry(&edited));
    assert_eq!(
        a.export_hash, b.export_hash,
        "external dependents stay valid"
    );
    assert_ne!(a.pkg_hash, b.pkg_hash, "same-package dependents rebuild");
}

#[test]
fn adding_a_private_item_changes_nothing() {
    let base = interfaces(&[BASE_MAIN, BASE_GEO]);
    let edited = interfaces(&[
        BASE_MAIN,
        (
            &["geometry"],
            "shapes.lu",
            "pub fn area(side: int) -> int {\n    side * side\n}\n\
             pub(pkg) fn perimeter(side: int) -> int {\n    4 * side\n}\n\
             fn hidden(side: int) -> int {\n    side\n}\n\
             fn extra() -> int {\n    9\n}\n",
        ),
    ]);
    assert_eq!(
        encode(geometry(&base)),
        encode(geometry(&edited)),
        "private items are not interface surface"
    );
}

#[test]
fn whitespace_edit_changes_nothing() {
    let base = interfaces(&[BASE_MAIN, BASE_GEO]);
    let edited = interfaces(&[
        BASE_MAIN,
        (
            &["geometry"],
            "shapes.lu",
            "pub fn area(side: int)   ->   int {\n        side * side\n}\n\n\n\
             pub(pkg) fn perimeter(side: int) -> int {\n    4 * side\n}\n\
             fn hidden(side: int) -> int {\n    side\n}\n",
        ),
    ]);
    assert_eq!(encode(geometry(&base)), encode(geometry(&edited)));
}

#[test]
fn item_reorder_within_a_file_changes_nothing() {
    let base = interfaces(&[BASE_MAIN, BASE_GEO]);
    let edited = interfaces(&[
        BASE_MAIN,
        (
            &["geometry"],
            "shapes.lu",
            "fn hidden(side: int) -> int {\n    side\n}\n\
             pub(pkg) fn perimeter(side: int) -> int {\n    4 * side\n}\n\
             pub fn area(side: int) -> int {\n    side * side\n}\n",
        ),
    ]);
    assert_eq!(
        encode(geometry(&base)),
        encode(geometry(&edited)),
        "items sort canonically by (kind, name)"
    );
}

#[test]
fn file_order_within_the_directory_changes_nothing() {
    // The same two items, first split across `a.lu`/`b.lu`, then with
    // the file names swapped so the load order flips.
    let split_one = interfaces(&[
        BASE_MAIN,
        (
            &["geometry"],
            "a.lu",
            "pub fn area(side: int) -> int {\n    side * side\n}\n",
        ),
        (
            &["geometry"],
            "b.lu",
            "pub(pkg) fn perimeter(side: int) -> int {\n    4 * side\n}\n",
        ),
    ]);
    let split_two = interfaces(&[
        BASE_MAIN,
        (
            &["geometry"],
            "b.lu",
            "pub fn area(side: int) -> int {\n    side * side\n}\n",
        ),
        (
            &["geometry"],
            "a.lu",
            "pub(pkg) fn perimeter(side: int) -> int {\n    4 * side\n}\n",
        ),
    ]);
    assert_eq!(encode(geometry(&split_one)), encode(geometry(&split_two)));
}

#[test]
fn dep_signature_change_propagates_through_dep_hashes() {
    let base = interfaces(&[BASE_MAIN, BASE_GEO]);
    let edited = interfaces(&[
        BASE_MAIN,
        (
            &["geometry"],
            "shapes.lu",
            "pub fn area(side: i64) -> i64 {\n    side * side\n}\n\
             pub(pkg) fn perimeter(side: int) -> int {\n    4 * side\n}\n\
             fn hidden(side: int) -> int {\n    side\n}\n",
        ),
    ]);
    let root_a = base
        .iter()
        .find(|i| i.module_path.is_empty())
        .expect("root");
    let root_b = edited
        .iter()
        .find(|i| i.module_path.is_empty())
        .expect("root");
    assert_ne!(
        root_a.export_hash, root_b.export_hash,
        "the dep's export hash rides in the dependent's header"
    );
}
