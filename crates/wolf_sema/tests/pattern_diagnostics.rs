//! Rendered snapshots for the s17 pattern family: exhaustiveness with
//! witnesses (E0801), unreachable arms (E0802, warning), refutable
//! bindings (E0806), pattern shape mismatches (E0808), and row-tag
//! misses in match position (E0602). Every code ships with a reviewed
//! fixture (`cargo xtask diag-catalog` enforces the pairing) — the
//! rendering IS the artifact (D22).

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
    assert!(
        tc.not_yet.is_empty(),
        "fixtures check fully: {:?}",
        tc.not_yet
    );
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

// ---------------------------------------------------------- E0801 -----

/// Missing enum variants, witnesses named — including the payload
/// shape of `Rgb(_, _, _)`.
#[test]
fn e0801_enum_witnesses() {
    snap_one(
        "e0801_enum_witnesses",
        "enum Color {\n    Red,\n    Green,\n    Rgb(int, int, int),\n}\n\n\
         fn main() -> !int {\n    let c = Color.Red\n    \
         let v = match c {\n        Red => 0,\n    }\n    v\n}\n",
    );
}

/// Integer literals never cover `int`: the witness is the concrete
/// smallest uncovered value ("not covered: `2`").
#[test]
fn e0801_int_witness() {
    snap_one(
        "e0801_int_witness",
        "fn main() -> !int {\n    let n = 5\n    \
         let v = match n {\n        0 => 1,\n        1 => 2,\n    }\n    v\n}\n",
    );
}

/// A sealed row's tags are a closed set; the missing tag is named.
#[test]
fn e0801_row_missing_tag() {
    snap_one(
        "e0801_row_missing_tag",
        "fn f(n: int) -> int ! {Io(int), timeout} {\n    \
         if n == 0 {\n        return timeout\n    }\n    \
         if n == 1 {\n        return Io(3)\n    }\n    n\n}\n\n\
         fn main() -> !int {\n    let v = f(2) else |err| {\n        \
         match err {\n            Io(_) => 1,\n        }\n    }\n    v\n}\n",
    );
}

/// Guards do not count toward coverage.
#[test]
fn e0801_guard_non_contribution() {
    snap_one(
        "e0801_guard_non_contribution",
        "fn main() -> !int {\n    let b = true\n    \
         let v = match b {\n        true => 1,\n        false if b => 2,\n    }\n    v\n}\n",
    );
}

// ---------------------------------------------------------- E0802 -----

/// The wildcard after full case analysis is dead — a warning citing
/// the covering arms.
#[test]
fn e0802_unreachable_after_full_split() {
    snap_one(
        "e0802_unreachable_arm",
        "fn main() -> !int {\n    let b = false\n    \
         let v = match b {\n        true => 1,\n        false => 2,\n        _ => 3,\n    }\n    v\n}\n",
    );
}

/// A duplicate literal arm is subsumed by its first appearance.
#[test]
fn e0802_duplicate_literal() {
    snap_one(
        "e0802_duplicate_literal",
        "fn main() -> !int {\n    let n = 4\n    \
         let v = match n {\n        1 => 1,\n        1 => 2,\n        _ => 0,\n    }\n    v\n}\n",
    );
}

// ---------------------------------------------------------- E0806 -----

/// A literal pattern in `let` position: matching cannot fail there.
#[test]
fn e0806_refutable_let() {
    snap_one(
        "e0806_refutable_let",
        "fn main() -> !int {\n    let n = 3\n    let 1 = n\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0809 -----

/// `else |Tag(p)|` over a wider row: the handler pattern must cover
/// the row entire (s71, #43) — the diagnostic names the missing case
/// and points at the match-over-row shape.
#[test]
fn e0809_handler_uncovered() {
    snap_one(
        "e0809_handler_uncovered",
        "fn poke(n: int) -> int ! {Io(int), timeout} {\n    \
         if n == 0 {\n        return Io(9)\n    }\n    \
         if n == 1 {\n        return timeout\n    }\n    n\n}\n\n\
         fn main() -> !int {\n    let v = poke(2) else |Io(e)| { e }\n    v\n}\n",
    );
}

/// The covering form is clean: a single-tag row destructured at the
/// handler binds the payload, not the tag (sc08's convention).
#[test]
fn e0809_single_tag_covers() {
    snap_one(
        "e0809_single_tag_covers",
        "fn f(n: int) -> int ! {Io(int)} {\n    \
         if n == 0 {\n        return Io(1)\n    }\n    n\n}\n\n\
         fn main() -> !int {\n    let v = f(0) else |Io(e)| { e }\n    v\n}\n",
    );
}

// ---------------------------------------------------------- E0808 -----

/// A variant pattern over a plain integer.
#[test]
fn e0808_variant_over_int() {
    snap_one(
        "e0808_variant_over_int",
        "fn main() -> !int {\n    let n = 3\n    \
         let v = match n {\n        Io(x) => x,\n        _ => 0,\n    }\n    v\n}\n",
    );
}

/// Payload arity: the pattern binds fewer pieces than the variant
/// carries.
#[test]
fn e0808_payload_arity() {
    snap_one(
        "e0808_payload_arity",
        "enum Color {\n    Red,\n    Rgb(int, int, int),\n}\n\n\
         fn main() -> !int {\n    let c = Color.Red\n    \
         let v = match c {\n        Red => 0,\n        Rgb(r) => r,\n    }\n    v\n}\n",
    );
}

// ------------------------------------------------- E0602 in patterns ---

/// An arm matching a tag the sealed row does not include.
#[test]
fn e0602_pattern_unknown_tag() {
    snap_one(
        "e0602_pattern_unknown_tag",
        "fn f(n: int) -> int ! {Io(int)} {\n    \
         if n == 0 {\n        return Io(1)\n    }\n    n\n}\n\n\
         fn main() -> !int {\n    let v = f(2) else |err| {\n        \
         match err {\n            Io(_) => 1,\n            Timeout => 2,\n        }\n    }\n    v\n}\n",
    );
}

// ------------------------------------------- range patterns (s147) ---

/// `[gram.pat.range]` (#287): `5..5` stops before its own low end —
/// E0815 with the one-value spelling offered.
#[test]
fn e0815_empty_range_same_ends() {
    snap_one(
        "e0815_empty_range_same_ends",
        "fn main() -> !int {\n    let n = 5\n    \
         let v = match n {\n        5..5 => 1,\n        _ => 0,\n    }\n    v\n}\n",
    );
}

/// `9..=3` runs backwards — E0815 with the swap offered.
#[test]
fn e0815_empty_range_backwards() {
    snap_one(
        "e0815_empty_range_backwards",
        "fn main() -> !int {\n    let n = 5\n    \
         let v = match n {\n        9..=3 => 1,\n        _ => 0,\n    }\n    v\n}\n",
    );
}

/// A literal inside an earlier range is dead: E0802 cites the range
/// arm (subsumption is exact against one covering arm).
#[test]
fn e0802_literal_inside_earlier_range() {
    snap_one(
        "e0802_literal_inside_earlier_range",
        "fn main() -> !int {\n    let n = 5\n    \
         let v = match n {\n        0..10 => 1,\n        5 => 2,\n        _ => 0,\n    }\n    v\n}\n",
    );
}

/// Ranges keep the column infinite: the witness is the first value
/// past every covering range and literal (`0..10` witnesses `10`).
#[test]
fn e0801_witness_past_range() {
    snap_one(
        "e0801_witness_past_range",
        "fn main() -> !int {\n    let n = 5\n    \
         let v = match n {\n        0..10 => 1,\n    }\n    v\n}\n",
    );
}

/// A `str` endpoint: E0808 — `str` has no order clause.
#[test]
fn e0808_range_str_endpoints() {
    snap_one(
        "e0808_range_str_endpoints",
        "fn main() -> !int {\n    let s = \"m\"\n    \
         let v = match s {\n        \"a\"..\"n\" => 1,\n        _ => 0,\n    }\n    v\n}\n",
    );
}

/// Mixed endpoint types: E0401 at the second endpoint.
#[test]
fn e0401_range_mixed_endpoints() {
    snap_one(
        "e0401_range_mixed_endpoints",
        "fn main() -> !int {\n    let n = 5\n    \
         let v = match n {\n        0..'z' => 1,\n        _ => 0,\n    }\n    v\n}\n",
    );
}

/// A range is refutable: a binder position refuses it (E0806).
#[test]
fn e0806_range_in_let() {
    snap_one(
        "e0806_range_in_let",
        "fn main() -> !int {\n    let 0..10 = 5\n    0\n}\n",
    );
}
