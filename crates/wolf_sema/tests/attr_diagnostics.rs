//! kw01 (K13, K7 — wolf-lang#519, #524): reviewed snapshots for E0817
//! (an attribute not known, not implemented, or misplaced; a `cfg` the
//! compiler cannot decide) and E0818 (an ABI string other than `"c"`),
//! the diag-catalog fixture rule; and the cfg strip's effect on what
//! resolution sees.

use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_sema::{AliasTable, MemoryLoader, Resolution, resolve_package_with};

fn resolve(src: &str) -> Resolution {
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads")
}

fn render(res: &Resolution, diags: &[&wolf_diag::Diagnostic]) -> String {
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

/// The attribute check's diagnostics.
fn render_attrs(src: &str) -> String {
    let res = resolve(src);
    let check = wolf_sema::attrs::check(&res.package);
    let all: Vec<&wolf_diag::Diagnostic> = check.iter().collect();
    render(&res, &all)
}

#[test]
fn e0817_unknown_attribute() {
    insta::assert_snapshot!(
        "e0817_unknown",
        render_attrs("#[frobnicate(yes)]\nfn main() -> int {\n    0\n}\n")
    );
}

#[test]
fn e0817_contract_not_implemented() {
    insta::assert_snapshot!(
        "e0817_noalloc",
        render_attrs("#[noalloc]\nfn f() -> int {\n    1\n}\n")
    );
}

#[test]
fn e0817_repr_arguments() {
    insta::assert_snapshot!(
        "e0817_repr_args",
        render_attrs("#[repr(c, packed), repr(bogus)]\nstruct P {\n    a: u8,\n    b: u32,\n}\n")
    );
}

#[test]
fn e0817_misplaced() {
    insta::assert_snapshot!(
        "e0817_misplaced",
        render_attrs("#[repr(c)]\nfn f() -> int {\n    1\n}\n\n#[consttime]\nstruct S {\n    x: int,\n}\n")
    );
}

#[test]
fn e0817_cfg_unknown_target() {
    insta::assert_snapshot!(
        "e0817_cfg_unknown_target",
        render_attrs("#[cfg(target = \"x86-64\")]\nfn f() -> int {\n    1\n}\n")
    );
}

#[test]
fn e0817_cfg_unknown_predicate() {
    insta::assert_snapshot!(
        "e0817_cfg_unix",
        render_attrs("#[cfg(unix)]\nfn f() -> int {\n    1\n}\n")
    );
}

#[test]
fn e0818_abi_string() {
    insta::assert_snapshot!(
        "e0818_x86_interrupt",
        render_attrs("extern \"x86-interrupt\" fn isr() {\n    let x = 1\n}\n")
    );
}

#[test]
fn the_implemented_set_is_silent() {
    let src = "#[repr(c)]\nstruct P {\n    a: u8,\n}\n\n#[consttime]\nfn f(k: int) -> int {\n    k\n}\n\n\
               #[allow(w1301)]\nfn g() -> int {\n    #[index(1)]\n    let x = 1\n    x\n}\n\n\
               extern \"c\" fn h() -> i32 {\n    1\n}\n\n\
               #[cfg(target = \"x86_64\")]\nfn a() -> int {\n    1\n}\n\n\
               #[cfg(target = \"aarch64\")]\nfn a() -> int {\n    2\n}\n";
    assert_eq!(render_attrs(src), "");
}

/// A dropped item is never defined: the freestanding target is no
/// hosted build's, so its function is gone before the item table is
/// built, and the two arch-gated `a`s never collide (E0302).
#[test]
fn a_dropped_item_is_never_defined() {
    let src = "#[cfg(target = \"x86_64-unknown-none\")]\nfn kernel_only() -> int {\n    1\n}\n\n\
               #[cfg(target = \"x86_64\")]\nfn a() -> int {\n    1\n}\n\n\
               #[cfg(target = \"aarch64\")]\nfn a() -> int {\n    2\n}\n";
    let res = resolve(src);
    let names: Vec<&str> = res.package.tables[0]
        .items
        .iter()
        .map(|i| i.name.as_str())
        .collect();
    assert_eq!(names, ["a"], "{names:?}");
    assert!(
        res.package.diagnostics.is_empty(),
        "{:?}",
        res.package.diagnostics
    );
}
