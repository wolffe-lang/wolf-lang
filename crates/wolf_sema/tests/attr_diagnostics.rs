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

/// kw01 wrote this with `packed`, which kw08 implemented; `transparent`
/// is the known representation still refused.
#[test]
fn e0817_repr_arguments() {
    insta::assert_snapshot!(
        "e0817_repr_args",
        render_attrs(
            "#[repr(c, transparent), repr(bogus)]\nstruct P {\n    a: u8,\n    b: u32,\n}\n"
        )
    );
}

/// kw08: `packed` and `align(N)` are implemented; nothing to refuse.
#[test]
fn packed_and_aligned_reprs_are_accepted() {
    let out = render_attrs(
        "#[repr(c, packed)]\nstruct P {\n    a: u8,\n    b: u32,\n}\n\n\
         #[repr(c, align(4096))]\nstruct Page {\n    e: u64,\n}\n\n\
         #[repr(c)]\n#[repr(align(0x10))]\nstruct A {\n    x: u32,\n}\n",
    );
    assert_eq!(out, "", "kw08's shapes are representations: {out}");
}

/// kw08 (E0820): an alignment that is not a power of two, zero, or past
/// gcc's 2^28 ceiling; a non-integer alignment.
#[test]
fn e0820_alignment_values() {
    insta::assert_snapshot!(
        "e0820_align_values",
        render_attrs(
            "#[repr(c, align(3))]\nstruct A {\n    x: u8,\n}\n\n\
             #[repr(c, align(0))]\nstruct B {\n    x: u8,\n}\n\n\
             #[repr(c, align(536870912))]\nstruct C {\n    x: u8,\n}\n\n\
             #[repr(c, align(\"8\"))]\nstruct D {\n    x: u8,\n}\n"
        )
    );
}

/// kw08 (E0820): how representations combine — `packed`/`align`
/// without `c`, both at once, on a generic struct, named twice.
#[test]
fn e0820_combinations() {
    insta::assert_snapshot!(
        "e0820_combinations",
        render_attrs(
            "#[repr(packed)]\nstruct A {\n    x: u8,\n}\n\n\
             #[repr(c, packed, align(8))]\nstruct B {\n    x: u8,\n}\n\n\
             #[repr(c, packed)]\nstruct C[T] {\n    x: T,\n}\n\n\
             #[repr(c, c)]\nstruct D {\n    x: u8,\n}\n\n\
             #[repr(align(8))]\nstruct E {\n    x: u8,\n}\n"
        )
    );
}

#[test]
fn e0817_misplaced() {
    insta::assert_snapshot!(
        "e0817_misplaced",
        render_attrs(
            "#[repr(c)]\nfn f() -> int {\n    1\n}\n\n#[consttime]\nstruct S {\n    x: int,\n}\n"
        )
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

/// kw09 (`[abi.link.section]`): what `#[section]` still refuses — a
/// `const` (no storage), a name that is not one, and Rust's spelling.
#[test]
fn e0817_section_shapes() {
    insta::assert_snapshot!(
        "e0817_section_shapes",
        render_attrs(
            "#[section(\".rodata.kw\")]\nconst LIMIT: int = 5\n\n\
             #[section(\"a b\")]\nvar v: int = 0\n\n\
             #[link_section(\".text.boot\")]\nfn boot() -> int {\n    1\n}\n"
        )
    );
}

/// kw09 (`[abi.link.extern]`): E0821's syntactic shapes (attrs) and its
/// type (signatures) — an initializer, a `var`, a body position, a
/// non-pointer type.
#[test]
fn e0821_extern_let_shapes() {
    let src = "extern \"c\" let A: *u8 = 0 as *u8\n\n\
               extern \"c\" var B: *u8\n\n\
               extern \"c\" let C: int\n\n\
               fn f() -> int {\n    extern \"c\" let D: *u8\n    1\n}\n";
    let res = resolve(src);
    // The signature pass carries the attribute check's diagnostics too.
    let sigs = wolf_sema::build_sigs(&res.package);
    let all: Vec<&wolf_diag::Diagnostic> = sigs.diagnostics.iter().collect();
    insta::assert_snapshot!("e0821_extern_let_shapes", render(&res, &all));
}

#[test]
fn the_implemented_set_is_silent() {
    let src = "#[repr(c)]\nstruct P {\n    a: u8,\n}\n\n#[consttime]\nfn f(k: int) -> int {\n    k\n}\n\n\
               #[allow(w1301)]\nfn g() -> int {\n    #[index(1)]\n    let x = 1\n    x\n}\n\n\
               extern \"c\" fn h() -> i32 {\n    1\n}\n\n\
               #[section(\".text.boot\")]\nexport fn kmain() -> i64 {\n    1\n}\n\n\
               #[section(\".data.kw\")]\nvar placed: int = 0\n\n\
               extern \"c\" let __kernel_end: *u8\n\n\
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
