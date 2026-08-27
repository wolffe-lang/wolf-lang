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
        "fn answer() -> int {\n    41\n}\n\nfn add(a: int, b: int) -> int {\n    a + b\n}\n\nfn main() -> !int {\n    add(answer, 1)\n}\n",
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

/// `assert` takes 1 or 2 arguments (#9) — the arity report says so.
#[test]
fn e0402_assert_arity() {
    snap_one(
        "e0402_assert_arity",
        "fn main() -> !int {\n    assert(true, \"msg\", 3)\n    0\n}\n",
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

// E0410 moved to the RESOLVE rung at s29 (DIV-2026-010): its snapshots
// live in `tests/diagnostics.rs` now — binding immutability is lexical
// knowledge, and the type checker no longer reports it.

// ---------------------------------------------------------- E0411 -----

/// Catalog case (s37): a single `s[^1]` is still character indexing —
/// the end-relative *slices* are the honest spelling. (The Python
/// reflex `s[-1]` is caught one rung earlier, as E0209 with its own
/// `^n` fix-it.)
#[test]
fn e0411_from_end_single_position() {
    snap_one(
        "e0411_from_end_single",
        "fn main() -> !int {\n    let s = \"wolf\"\n    let last = s[^1]\n    0\n}\n",
    );
}

/// Catalog case (s37): `s[i]` — no character indexing on `str`, by
/// decision (D25); the note names the honest alternatives.
#[test]
fn e0411_char_index_hint() {
    snap_one(
        "e0411_char_index",
        "fn main() -> !int {\n    let s = \"wolf\"\n    let i = 2\n    let c = s[i]\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0412 -----

/// Catalog case (s38): a spec character the grammar has no place for —
/// specs are comptime-known, so the error lands at the literal, never
/// at run time (#28).
#[test]
fn e0412_malformed_spec() {
    snap_one(
        "e0412_malformed",
        "fn main() -> !int {\n    let n = 42\n    print(\"[{n:>8q}]\")\n    0\n}\n",
    );
}

/// Catalog case (s38): the zero FLAG after an explicit alignment —
/// the spec must pick zero-padding OR alignment; the compiler never
/// picks silently. (`{n:0>8}` is different and legal: fill `0`,
/// align `>` — the two-character form reads as fill+align.)
#[test]
fn e0412_zero_with_align() {
    snap_one(
        "e0412_zero_with_align",
        "fn main() -> !int {\n    let n = 42\n    print(\"[{n:>08}]\")\n    0\n}\n",
    );
}

/// Catalog case (s38): the #28-named typo `{x:>>8}` — a doubled
/// alignment, not "fill with `>`".
#[test]
fn e0412_align_as_fill() {
    snap_one(
        "e0412_align_as_fill",
        "fn main() -> !int {\n    let n = 42\n    print(\"[{n:>>8}]\")\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0413 -----

/// Catalog case (s38): precision on an integer — `.2` means
/// digits-after-the-point on a float and a byte cap on a `str`,
/// nothing on an `int`.
#[test]
fn e0413_precision_on_int() {
    snap_one(
        "e0413_precision_on_int",
        "fn main() -> !int {\n    let n = 42\n    print(\"[{n:.2}]\")\n    0\n}\n",
    );
}

/// Catalog case (s38): a base kind off integers — `x` renders an
/// integer in hex; a `str` has no base.
#[test]
fn e0413_hex_on_str() {
    snap_one(
        "e0413_hex_on_str",
        "fn main() -> !int {\n    let s = \"wolf\"\n    print(\"[{s:x}]\")\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0414 -----

/// Catalog case (#106): `fn main() -> str` used to run on the checked
/// rung with the value silently dropped and refuse at the native
/// rung's C-entry shim — a lane divergence over a declaration fact.
/// The shape is ruled here, at typecheck, with the legal spellings.
#[test]
fn e0414_str_main() {
    snap_one(
        "e0414_str_main",
        "fn main() -> str {\n    print(\"hi\")\n    \"nope\"\n}\n",
    );
}

/// The process hands `main` no arguments.
#[test]
fn e0414_main_with_params() {
    snap_one("e0414_main_params", "fn main(x: int) -> int {\n    x\n}\n");
}

/// Nothing exists to choose the entry's type arguments.
#[test]
fn e0414_generic_main() {
    snap_one(
        "e0414_generic_main",
        "fn main[T]() {\n    print(\"t\")\n}\n",
    );
}

/// The boundary from the other side: every legal shape stays silent —
/// `()`, `int`, `!int`, `!()`, and an explicit row over `int`.
#[test]
fn e0414_legal_shapes_are_silent() {
    for src in [
        "fn main() {\n    print(\"u\")\n}\n",
        "fn main() -> int {\n    0\n}\n",
        "fn main() -> !int {\n    0\n}\n",
        "fn main() -> !() {\n    print(\"u\")\n}\n",
        "fn main() -> int ! {none} {\n    0\n}\n",
    ] {
        let out = render_types(&[(&[], "main.lu", src)]);
        assert!(out.is_empty(), "expected silence for {src:?}, got:\n{out}");
    }
}

/// A non-root `main` is just a function — the entry rule reads the
/// root module only.
#[test]
fn e0414_non_root_main_is_ordinary() {
    let out = render_types(&[
        (
            &[],
            "main.lu",
            "use util\nfn main() -> !int {\n    print(util.main(3))\n    print(\"{util.twice(3)}\")\n    0\n}\n",
        ),
        (
            &["util"],
            "u.lu",
            "/// The nested namesake: takes a parameter, returns `str`.\npub fn main(v: int) -> str {\n    \"{v}\"\n}\n/// A second item, so the module is not one of ceremony.\npub fn twice(v: int) -> int {\n    v * 2\n}\n",
        ),
    ]);
    assert!(out.is_empty(), "expected silence, got:\n{out}");
}

// ------------------------------------------- #35 (narrowed): bottom --

/// #35, narrowed (s108): `assert(false)` — the spelled-out literal —
/// types as `!` and inhabits the `T` a generic handler's fallback
/// owes, exactly as return-divergence already did.
#[test]
fn assert_false_diverges_in_fallback() {
    let out = render_types(&[(
        &[],
        "main.lu",
        "fn expect[T](v: T ! {none}, msg: str) -> T {\n    let hit = v else |_| {\n        print(\"FAILED: {msg}\")\n        assert(false)\n    }\n    hit\n}\n\nfn main() -> !int {\n    let a = expect(7, \"seven\")\n    if a == 7 { 0 } else { 1 }\n}\n",
    )]);
    assert!(out.is_empty(), "expected silence, got:\n{out}");
}

/// The scope boundary, pinned: a COMPUTED condition is not the
/// literal — the checker cannot know it diverges, so the fallback
/// still owes a `T` (E0401). Widening beyond the literal is the
/// surface question that stays open on #35.
#[test]
fn e0401_computed_assert_still_owes_t() {
    snap_one(
        "e0401_computed_assert_fallback",
        "fn expect[T](v: T ! {none}, cond: bool) -> T {\n    let hit = v else |_| {\n        assert(cond)\n    }\n    hit\n}\n\nfn main() -> !int {\n    let a = expect(7, false)\n    if a == 7 { 0 } else { 1 }\n}\n",
    );
}

// ---------------------------------------------------------- E1607 -----

/// c28 [ct.attr.public]: a `public(…)` entry naming no parameter is
/// refused AT THE ATTRIBUTE — the alternative is a secret-by-default
/// parameter the author believes is public, refused later with a
/// message about a branch they think is licensed.
#[test]
fn e1607_public_names_no_parameter() {
    snap_one(
        "e1607_public_names_no_parameter",
        "#[consttime(public(nope))]\nfn f(k: int) -> int {\n    0\n}\n\nfn main() -> !int {\n    f(1)\n}\n",
    );
}

/// The other malformed shapes: a non-`public` argument, and `public`
/// without a list.
#[test]
fn e1607_malformed_argument_shapes() {
    snap_one(
        "e1607_malformed_shapes",
        "#[consttime(frob(k))]\nfn f(k: int) -> int {\n    0\n}\n\n#[consttime(public)]\nfn g(k: int) -> int {\n    0\n}\n\nfn main() -> !int {\n    f(1) + g(1)\n}\n",
    );
}

/// The well-formed spellings stay silent: bare, and a real parameter.
#[test]
fn e1607_well_formed_stays_clean() {
    assert_eq!(
        render_types(&[(
            &[],
            "main.lu",
            "#[consttime]\nfn f(k: wrapping[u64]) -> wrapping[u64] {\n    k\n}\n\n#[consttime(public(n))]\nfn g(k: wrapping[u64], n: int) -> wrapping[u64] {\n    k\n}\n\nfn main() -> !int {\n    let r = f(1) | g(2, 3)\n    if r == 3 { 0 } else { 1 }\n}\n",
        )]),
        ""
    );
}
