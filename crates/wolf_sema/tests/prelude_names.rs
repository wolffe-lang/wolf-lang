//! s212 (ruling #39 = B, wolf-lang#586, wolf-lsp#32): the published
//! prelude list against the language it describes.
//!
//! `prelude::names()` is generated from the rows resolution reads, so
//! the list cannot name what the checker does not resolve. What this
//! file adds is the other two directions: every anchor the list cites
//! is a clause the spec has (`spec/anchors.json`), and every row's
//! `w0304` is what the resolver actually says when a module declares
//! that name — measured here per name, not restated.

use std::path::Path;
use wolf_diag::Severity;
use wolf_sema::prelude::{self, Kind};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with};

fn anchors_json() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/anchors.json");
    std::fs::read_to_string(&path).expect("spec/anchors.json reads")
}

#[test]
fn every_cited_anchor_is_a_spec_clause() {
    let index = anchors_json();
    let mut cited: Vec<&str> = prelude::names().iter().filter_map(|n| n.anchor).collect();
    cited.push(prelude::HOST_SIGS_ANCHOR);
    for a in cited {
        assert!(
            index.contains(&format!("\"{a}\":")),
            "the prelude list cites `[{a}]`, which spec/anchors.json does not carry"
        );
    }
}

/// The list's shape, pinned. A name added to (or removed from) the
/// prelude moves one of these: say so in the CHANGELOG's "Read this
/// before you bump the pin" (`cargo xtask prelude-diff`), regenerate
/// `spec/prelude.json`, and move the count here in the same commit.
#[test]
fn the_list_by_kind() {
    let all = prelude::names();
    let count = |k: Kind| all.iter().filter(|n| n.kind == k).count();
    let got = [
        ("builtin_type", count(Kind::BuiltinType)),
        ("type", count(Kind::Type)),
        ("function", count(Kind::Function)),
        ("intrinsic", count(Kind::Intrinsic)),
        ("provisional", count(Kind::Provisional)),
        ("mark", prelude::marks().len()),
        (
            "unanchored",
            all.iter().filter(|n| n.anchor.is_none()).count(),
        ),
        ("w0304", all.iter().filter(|n| n.w0304).count()),
    ];
    assert_eq!(
        got,
        [
            ("builtin_type", 18),
            ("type", 8),
            ("function", 107),
            ("intrinsic", 8),
            ("provisional", 6),
            ("mark", 18),
            ("unanchored", 30),
            ("w0304", 139),
        ],
        "the prelude moved"
    );
}

/// The smallest module that declares `name` at the top level.
fn declaring(name: &str) -> String {
    if name.starts_with(|c: char| c.is_ascii_uppercase()) {
        format!("struct {name} {{\n    a: int,\n}}\n\nfn main() {{\n}}\n")
    } else {
        format!("fn {name}() -> int {{\n    return 1\n}}\n\nfn main() {{\n}}\n")
    }
}

/// Does resolving a module that declares `name` draw W0304?
fn draws_w0304(name: &str) -> bool {
    let mut ml = MemoryLoader::new("probe");
    ml.add_file(&[], "main.lu", &declaring(name));
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let errors: Vec<_> = res
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "`{name}`: the probe resolves: {errors:?}"
    );
    res.diagnostics.iter().any(|d| d.code.as_str() == "W0304")
}

#[test]
fn w0304_is_what_the_resolver_says() {
    let mut wrong = Vec::new();
    for n in prelude::names() {
        if draws_w0304(n.name) != n.w0304 {
            wrong.push((n.name, n.w0304));
        }
    }
    assert!(
        wrong.is_empty(),
        "published w0304 disagrees with the resolver: {wrong:?}"
    );
    assert!(!draws_w0304("frobnicate"), "the control draws nothing");
}
