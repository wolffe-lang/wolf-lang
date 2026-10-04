//! kw08 (K4, STATUS #31): the layout queries and the packed-field lend
//! rule. Reviewed snapshots for E0819 (a packed field lent), E0708 on
//! `align_of`/`offset_of` of a native-layout struct, and E0403 for an
//! `offset_of` field the struct does not have; and the queries' values,
//! held by comptime `assert`s (a wrong number is E0710).

use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

fn render_tc(src: &str) -> String {
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

const GDTR: &str = "#[repr(c, packed)]\nstruct Gdtr {\n    limit: u16,\n    base: u64,\n}\n\n";

#[test]
fn e0819_a_mut_claim_of_a_packed_field() {
    insta::assert_snapshot!(
        "e0819_mut_arg",
        render_tc(&format!(
            "{GDTR}fn bump(mut x: u64) {{\n    x += 1\n}}\n\n\
             fn main() -> int {{\n    var d = Gdtr {{ limit: 7, base: 41 }}\n    \
             bump(mut d.base)\n    d.base as int - 42\n}}\n"
        ))
    );
}

/// A field INSIDE a packed field is as misplaced as the field itself,
/// and an aggregate passed `read` is a lend of its address.
#[test]
fn e0819_through_a_nested_field_and_a_read_aggregate() {
    insta::assert_snapshot!(
        "e0819_nested",
        render_tc(
            "#[repr(c)]\nstruct In {\n    x: u32,\n    y: u32,\n}\n\n\
             #[repr(c, packed)]\nstruct Out {\n    tag: u8,\n    inner: In,\n}\n\n\
             fn bump(mut x: u32) {\n    x += 1\n}\n\n\
             fn sum(v: In) -> int {\n    v.x as int + v.y as int\n}\n\n\
             fn main() -> int {\n    var o = Out { tag: 1, inner: In { x: 2, y: 3 } }\n    \
             bump(mut o.inner.x)\n    sum(o.inner) - 6\n}\n"
        )
    );
}

/// The fix the note names compiles: copy out, lend the copy, write
/// back. A scalar packed field passed `read` is a copy, not a lend.
#[test]
fn a_copied_packed_field_may_be_lent_and_a_scalar_read() {
    let out = render_tc(&format!(
        "{GDTR}fn bump(mut x: u64) {{\n    x += 1\n}}\n\n\
         fn twice(x: u64) -> u64 {{\n    x * 2\n}}\n\n\
         fn main() -> int {{\n    var d = Gdtr {{ limit: 7, base: 41 }}\n    \
         var b = d.base\n    bump(mut b)\n    d.base = b\n    \
         twice(d.base) as int - 84\n}}\n"
    ));
    assert_eq!(out, "", "the copy-out shape and a scalar read are not lends: {out}");
}

#[test]
fn e0708_align_and_offset_of_a_native_layout_struct() {
    insta::assert_snapshot!(
        "e0708_align_offset_native",
        render_tc(
            "struct Vec2 {\n    x: f64,\n    y: f64,\n}\n\n\
             fn main() -> !int {\n    const A = align_of(Vec2)\n    \
             const O = offset_of(Vec2, y)\n    if A + O == 16 { 0 } else { 1 }\n}\n"
        )
    );
}

/// A repr(c) struct whose field is native-layout has no clause layout
/// either; the refusal names the field.
#[test]
fn e0708_names_the_native_field() {
    insta::assert_snapshot!(
        "e0708_native_field",
        render_tc(
            "struct Vec2 {\n    x: f64,\n    y: f64,\n}\n\n\
             #[repr(c)]\nstruct Wrap {\n    tag: u8,\n    v: Vec2,\n}\n\n\
             fn main() -> !int {\n    const S = size_of(Wrap)\n    if S == 24 { 0 } else { 1 }\n}\n"
        )
    );
}

#[test]
fn e0403_offset_of_a_field_the_struct_lacks() {
    insta::assert_snapshot!(
        "e0403_offset_of_unknown_field",
        render_tc(&format!(
            "{GDTR}fn main() -> !int {{\n    const O = offset_of(Gdtr, bse)\n    \
             if O == 2 {{ 0 }} else {{ 1 }}\n}}\n"
        ))
    );
}

/// Every number here is what gcc 16.2.1 and clang 23.1.1 print for the
/// same declarations (`repr_c_raw_layout.rs` asks them on every host);
/// a wrong one fails its comptime `assert` (E0710).
#[test]
fn the_queries_answer_the_c_layout() {
    let src = "#[repr(c)]\nstruct C3 {\n    a: u8,\n    b: u32,\n    c: u8,\n}\n\n\
               #[repr(c, packed)]\nstruct P3 {\n    a: u8,\n    b: u32,\n    c: u8,\n}\n\n\
               #[repr(c, align(16))]\nstruct A16 {\n    x: u32,\n}\n\n\
               #[repr(c)]\nstruct Outer {\n    tag: u8,\n    inner: A16,\n    z: u16,\n}\n\n\
               #[repr(c)]\nstruct PNest {\n    a: u8,\n    p: P3,\n    z: u16,\n}\n\n\
               #[repr(c)]\nstruct Ptrs {\n    a: u8,\n    p: *u8,\n    d: f32,\n}\n\n\
               #[repr(c, align(2))]\nstruct Low {\n    x: u64,\n}\n\n\
               comptime fn check() -> bool {\n    \
               assert(size_of(C3) == 12)\n    assert(align_of(C3) == 4)\n    \
               assert(offset_of(C3, c) == 8)\n    \
               assert(size_of(P3) == 6)\n    assert(align_of(P3) == 1)\n    \
               assert(offset_of(P3, b) == 1)\n    assert(offset_of(P3, c) == 5)\n    \
               assert(size_of(A16) == 16)\n    assert(align_of(A16) == 16)\n    \
               assert(size_of(Outer) == 48)\n    assert(offset_of(Outer, inner) == 16)\n    \
               assert(offset_of(Outer, z) == 32)\n    \
               assert(size_of(PNest) == 10)\n    assert(align_of(PNest) == 2)\n    \
               assert(offset_of(PNest, z) == 8)\n    \
               assert(size_of(Ptrs) == 24)\n    assert(offset_of(Ptrs, p) == 8)\n    \
               assert(offset_of(Ptrs, d) == 16)\n    \
               assert(size_of(Low) == 8)\n    assert(align_of(Low) == 8)\n    \
               assert(size_of(u16) == 2)\n    assert(align_of(u16) == 2)\n    \
               assert(align_of(bool) == 1)\n    \
               true\n}\n\n\
               fn main() -> !int {\n    const OK = check()\n    if OK { 0 } else { 1 }\n}\n";
    let out = render_tc(src);
    assert_eq!(out, "", "every query answers C's number: {out}");
    // The control: one wrong number is E0710, so the asserts above are
    // read, not skipped.
    let wrong = src.replace("assert(size_of(P3) == 6)", "assert(size_of(P3) == 12)");
    let out = render_tc(&wrong);
    assert!(out.contains("E0710"), "a wrong layout number must fail: {out}");
}
