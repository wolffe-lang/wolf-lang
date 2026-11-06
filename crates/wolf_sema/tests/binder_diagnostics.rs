//! Variant names in binder positions (`[gram.pat.binder]`): W0801's binder
//! half and the `select` arm's E0806, both resolve-rung. One rendered
//! snapshot per position (the rendering is the reviewed artifact, D22), and
//! structured checks of every fix's edits and applicability, because a
//! machine-applicable edit is a promise `wolf fix` and the editor keep.

use wolf_diag::{Applicability, Diagnostic, RenderOptions, Sources, render_human};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with};

type Files<'a> = &'a [(&'a [&'a str], &'a str, &'a str)];

fn resolve(files: Files<'_>) -> (Vec<Diagnostic>, Sources) {
    let mut ml = MemoryLoader::new("snap");
    for (m, n, s) in files {
        ml.add_file(m, n, s);
    }
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    let diags = res
        .diagnostics
        .iter()
        .filter(|d| d.code.as_str() == "W0801" || d.code.as_str() == "E0806")
        .cloned()
        .collect();
    (diags, sources)
}

fn render(files: Files<'_>) -> String {
    let (diags, sources) = resolve(files);
    assert!(!diags.is_empty(), "every fixture here reports");
    let mut out = String::new();
    for d in &diags {
        out.push_str(&render_human(d, &sources, &RenderOptions::default()));
        out.push('\n');
    }
    out
}

fn snap(name: &str, src: &str) {
    insta::assert_snapshot!(name, render(&[(&[], "main.lu", src)]));
}

/// The one diagnostic a single-file fixture reports, with its source.
fn only(src: &str) -> (Diagnostic, String) {
    let (diags, _) = resolve(&[(&[], "main.lu", src)]);
    assert_eq!(diags.len(), 1, "one report: {diags:?}");
    (diags.into_iter().next().unwrap(), src.to_string())
}

/// The text each edit of suggestion `i` replaces, and its replacement.
fn edits(d: &Diagnostic, src: &str, i: usize) -> Vec<(String, String)> {
    d.suggestions[i]
        .edits
        .iter()
        .map(|(sp, new)| (src[sp.lo as usize..sp.hi as usize].to_string(), new.clone()))
        .collect()
}

const KIND: &str = "enum Kind { Drink, Food, Gratuity }\n\n";

// ------------------------------------------------- the positions ----

#[test]
fn w0801_let_tuple() {
    snap(
        "w0801_binder_let_tuple",
        &format!(
            "{KIND}fn main() -> !int {{\n    let (Drink, n) = (Kind.Food, 3)\n    \
             print(\"{{n}}\")\n    match Drink {{\n        Kind.Food => 0,\n        _ => 1,\n    }}\n}}\n"
        ),
    );
}

#[test]
fn w0801_let_struct_field() {
    snap(
        "w0801_binder_struct_field",
        &format!(
            "{KIND}struct P {{ k: Kind, n: int }}\n\n\
             fn main() -> !int {{\n    let P {{ k: Drink, n }} = P {{ k: Kind.Food, n: 3 }}\n    \
             print(\"{{n}}\")\n    match Drink {{\n        Kind.Food => 0,\n        _ => 1,\n    }}\n}}\n"
        ),
    );
}

#[test]
fn w0801_for_header() {
    snap(
        "w0801_binder_for",
        &format!(
            "{KIND}fn main() -> !int {{\n    var t = 0\n    \
             for (Food, n) in [(Kind.Drink, 1)] {{\n        t = t + n\n    }}\n    t - 1\n}}\n"
        ),
    );
}

#[test]
fn w0801_else_handler() {
    snap(
        "w0801_binder_else",
        &format!(
            "{KIND}fn pick(n: int) -> int ! {{gone}} {{\n    if n > 0 {{ n }} else {{ gone }}\n}}\n\n\
             fn main() -> !int {{\n    let a = pick(0) else |Gratuity| 7\n    a - 7\n}}\n"
        ),
    );
}

#[test]
fn w0801_at_name() {
    snap(
        "w0801_binder_at_name",
        &format!(
            "{KIND}fn main() -> !int {{\n    let Drink @ (k, n) = (Kind.Food, 3)\n    n - 3\n}}\n"
        ),
    );
}

#[test]
fn w0801_two_homes() {
    snap(
        "w0801_binder_two_homes",
        "enum Kind { Drink, Food }\nenum Menu { Drink, Dessert }\n\n\
         fn main() -> !int {\n    let (Drink, n) = (3, 4)\n    Drink + n - 7\n}\n",
    );
}

#[test]
fn w0801_imported_enum() {
    insta::assert_snapshot!(
        "w0801_binder_imported",
        render(&[
            (
                &[],
                "main.lu",
                "use kinds.Kind\n\nfn main() -> !int {\n    let (Food, n) = (Kind.Drink, 3)\n    \
                 match Food {\n        Kind.Drink => n - 3,\n        _ => 1,\n    }\n}\n",
            ),
            (
                &["kinds"],
                "kinds.lu",
                "/// The kinds.\npub enum Kind { Drink, Food }\n\n/// One more export.\n\
                 pub fn none() -> int {\n    0\n}\n",
            ),
        ])
    );
}

#[test]
fn e0806_select_arm() {
    snap(
        "e0806_select_arm_variant",
        &format!(
            "{KIND}fn main() -> !int {{\n    let ch = channel[Kind](1)\n    ch.send(Kind.Food)?\n    \
             var got = 0\n    select {{\n        Drink from ch => {{ got = 1 }},\n        \
             timeout(1.s) => {{ got = 2 }},\n    }}\n    got - 1\n}}\n"
        ),
    );
}

// ------------------------------------------------------ the fixes ----

#[test]
fn the_rename_reaches_every_use_and_comes_first() {
    let src = format!(
        "{KIND}fn main() -> !int {{\n    let (Drink, n) = (3, 4)\n    let f = fn() Drink + n\n    \
         print(\"{{Drink}}\")\n    f() - 7\n}}\n"
    );
    let (d, src) = only(&src);
    assert_eq!(d.code.as_str(), "W0801");
    assert_eq!(d.suggestions.len(), 2);
    assert_eq!(
        d.suggestions[0].applicability,
        Applicability::MachineApplicable
    );
    assert_eq!(
        edits(&d, &src, 0),
        vec![
            ("Drink".to_string(), "drink".to_string()),
            ("Drink".to_string(), "drink".to_string()),
            ("Drink".to_string(), "drink".to_string()),
        ],
        "the binder, the closure's capture and the interpolation"
    );
    assert_eq!(
        d.suggestions[1].applicability,
        Applicability::MachineApplicable
    );
    assert_eq!(
        edits(&d, &src, 1),
        vec![("Drink".to_string(), "Kind.Drink".to_string())]
    );
}

#[test]
fn a_taken_lowercase_name_gets_an_underscore() {
    let src = format!(
        "{KIND}fn main() -> !int {{\n    let drink = 1\n    let (Drink, n) = (2, 3)\n    \
         Drink + n + drink - 6\n}}\n"
    );
    let (d, src) = only(&src);
    assert_eq!(
        d.suggestions[0].message,
        "bind it under a lowercase name: `drink_`"
    );
    assert!(edits(&d, &src, 0).iter().all(|(_, new)| new == "drink_"));
}

#[test]
fn a_keyword_spelling_gets_an_underscore() {
    let src = "enum Shape { Type, Box }\n\nfn main() -> !int {\n    let (Type, n) = (1, 2)\n    \
               Type + n - 3\n}\n";
    let (d, _) = only(src);
    assert_eq!(
        d.suggestions[0].message,
        "bind it under a lowercase name: `type_`"
    );
}

#[test]
fn a_struct_shorthand_spells_the_field_out() {
    let src = "enum Kind { Drink, Food }\nstruct Q { Drink: int, n: int }\n\n\
               fn main() -> !int {\n    let Q { Drink, n } = Q { Drink: 5, n: 3 }\n    \
               let r = Q { Drink, n }\n    r.n + Drink - 8\n}\n";
    let (d, src) = only(src);
    assert_eq!(
        edits(&d, &src, 0),
        vec![
            ("Drink".to_string(), "Drink: drink".to_string()),
            ("Drink".to_string(), "Drink: drink".to_string()),
            ("Drink".to_string(), "drink".to_string()),
        ],
        "the pattern's shorthand, the literal's shorthand, and the plain use"
    );
    assert_eq!(
        edits(&d, &src, 1),
        vec![("Drink".to_string(), "Drink: Kind.Drink".to_string())]
    );
}

#[test]
fn two_enums_make_each_test_a_maybe() {
    let src = "enum Kind { Drink, Food }\nenum Menu { Drink, Dessert }\n\n\
               fn main() -> !int {\n    let (Drink, n) = (3, 4)\n    Drink + n - 7\n}\n";
    let (d, src) = only(src);
    assert_eq!(d.suggestions.len(), 3);
    assert_eq!(
        d.suggestions[0].applicability,
        Applicability::MachineApplicable
    );
    for (i, q) in [(1, "Kind.Drink"), (2, "Menu.Drink")] {
        assert_eq!(d.suggestions[i].applicability, Applicability::Maybe);
        assert_eq!(
            edits(&d, &src, i),
            vec![("Drink".to_string(), q.to_string())]
        );
    }
}

#[test]
fn a_variant_that_carries_values_tests_with_wildcards() {
    let src = "enum Opt { Some(int), Pair(int, int), Empty }\n\n\
               fn main() -> !int {\n    let (Pair, n) = (1, 2)\n    Pair + n - 3\n}\n";
    let (d, src) = only(src);
    assert_eq!(
        edits(&d, &src, 1),
        vec![("Pair".to_string(), "Opt.Pair(_, _)".to_string())]
    );
}

#[test]
fn the_name_before_at_offers_the_rename_alone() {
    let src = format!(
        "{KIND}fn main() -> !int {{\n    let Drink @ (k, n) = (Kind.Food, 3)\n    n - 3\n}}\n"
    );
    let (d, _) = only(&src);
    assert_eq!(d.suggestions.len(), 1);
}

#[test]
fn the_select_refusal_renames_the_receive() {
    let src = format!(
        "{KIND}fn main() -> !int {{\n    let ch = channel[Kind](1)\n    ch.send(Kind.Food)?\n    \
         select {{\n        Drink from ch => {{ print(\"{{Drink}}\") }},\n        \
         timeout(1.s) => {{}},\n    }}\n    0\n}}\n"
    );
    let (d, src) = only(&src);
    assert_eq!(d.code.as_str(), "E0806");
    assert_eq!(d.suggestions.len(), 1);
    assert_eq!(
        d.suggestions[0].applicability,
        Applicability::MachineApplicable
    );
    assert_eq!(
        edits(&d, &src, 0),
        vec![
            ("Drink".to_string(), "drink".to_string()),
            ("Drink".to_string(), "drink".to_string()),
        ]
    );
}

// ---------------------------------------------------- the boundary ----

#[test]
fn names_that_read_as_no_test_stay_silent() {
    for src in [
        // a lowercase binder
        format!("{KIND}fn main() -> !int {{\n    let (drink, n) = (Kind.Food, 3)\n    n - 3\n}}\n"),
        // a capitalized name that spells no variant
        format!("{KIND}fn main() -> !int {{\n    let (Zed, n) = (5, 3)\n    Zed - n - 2\n}}\n"),
        // a parameter and a closure parameter
        format!(
            "{KIND}fn twice(Drink: int) -> int {{\n    Drink * 2\n}}\n\n\
             fn main() -> !int {{\n    let g = fn(Drink: int) Drink + 1\n    twice(g(0)) - 2\n}}\n"
        ),
        // a match arm: a test, not a binder
        format!(
            "{KIND}fn main() -> !int {{\n    match Kind.Food {{\n        Drink => 1,\n        _ => 0,\n    }}\n}}\n"
        ),
        // a lowercase variant
        "enum Mode { fast, slow }\n\nfn main() -> !int {\n    let (fast, n) = (1, 2)\n    fast + n - 3\n}\n"
            .to_string(),
        // a lowercase select receive
        format!(
            "{KIND}fn main() -> !int {{\n    let ch = channel[Kind](1)\n    ch.send(Kind.Food)?\n    \
             select {{\n        drink from ch => {{}},\n        timeout(1.s) => {{}},\n    }}\n    0\n}}\n"
        ),
    ] {
        let (diags, _) = resolve(&[(&[], "main.lu", &src)]);
        assert!(diags.is_empty(), "silent: {diags:?}\n{src}");
    }
}

#[test]
fn an_enum_reached_through_its_module_is_not_in_scope() {
    let (diags, _) = resolve(&[
        (
            &[],
            "main.lu",
            "use kinds\n\nfn main() -> !int {\n    let (Drink, n) = (kinds.Kind.Food, 3)\n    n - 3\n}\n",
        ),
        (
            &["kinds"],
            "kinds.lu",
            "/// The kinds.\npub enum Kind { Drink, Food }\n\n/// One more export.\n\
             pub fn none() -> int {\n    0\n}\n",
        ),
    ]);
    assert!(diags.is_empty(), "{diags:?}");
}
