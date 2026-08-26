//! The s68 first wave: one reviewed snapshot per lint (the s10
//! catalog law, extended to warnings by s67). Every fixture here is
//! the smallest program its lint's scar describes; the corpus twins
//! live under `corpus/lints/`.

use wolf_diag::{RenderOptions, Severity, Sources, render_human};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

fn render_warnings(src: &str, typed: bool) -> String {
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        !res.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error),
        "fixtures resolve without errors: {:?}",
        res.diagnostics
    );
    let mut diags = res.diagnostics.clone();
    if typed {
        let tc = typecheck_package_with(&res.package, true);
        assert!(
            tc.not_yet.is_empty(),
            "fixtures typecheck fully: {:?}",
            tc.not_yet
        );
        assert!(
            !tc.has_errors(),
            "fixtures typecheck clean: {:?}",
            tc.diagnostics
        );
        diags.extend(tc.diagnostics.iter().cloned());
    }
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    let mut out = String::new();
    for d in diags.iter().filter(|d| d.severity == Severity::Warning) {
        out.push_str(&render_human(d, &sources, &RenderOptions::default()));
        out.push('\n');
    }
    assert!(!out.is_empty(), "every wave fixture warns");
    out
}

fn snap(name: &str, src: &str) {
    insta::assert_snapshot!(name, render_warnings(src, false));
}

fn snap_typed(name: &str, src: &str) {
    insta::assert_snapshot!(name, render_warnings(src, true));
}

// ------------------------------------------------- resolve rung ----

#[test]
fn w0304_shadowed_prelude_fn() {
    snap(
        "w0304_shadow_prelude",
        "fn min(a: int, b: int) -> int {\n    if a < b { a } else { b }\n}\n\
         fn main() -> !int {\n    min(0, 1)\n}\n",
    );
}

#[test]
fn w0304_shadowed_intrinsic_binding() {
    // The sharper case: a *binding* named `assert` wins over the
    // intrinsic inside its scope.
    snap(
        "w0304_shadow_binding",
        "fn main() -> !int {\n    let assert = 1\n    assert - 1\n}\n",
    );
}

#[test]
fn w0305_tag_collides_with_item() {
    snap(
        "w0305_tag_collision",
        "fn stuck() -> int {\n    9\n}\n\
         fn probe(n: int) -> int ! {stuck} {\n    \
             if n == 0 { return stuck }\n    n\n}\n\
         fn main() -> !int {\n    probe(1) else 0\n}\n",
    );
}

/// D52 ([gram.expr.tagident]): at a checked position whose expected
/// row declares the tag, a local named like it WINS (locals shadow),
/// and the collision warns at the use — the typing-rung emitter,
/// beside the wave's declaration-site one above.
#[test]
fn w0305_local_shadows_tag_at_argument() {
    snap_typed(
        "w0305_tag_shadowed_at_use",
        "fn or(v: int ! {none}, d: int) -> int {\n    v else d\n}\n\
         fn main() -> !int {\n    let none = 3\n    \
             if or(none, 9) == 3 { 0 } else { 1 }\n}\n",
    );
}

#[test]
fn w0306_prefix_operator_statement() {
    snap(
        "w0306_prefix_statement",
        "fn main() -> !int {\n    let a = 2\n    let b = 2\n    a\n    - b\n    a - b\n}\n",
    );
}

#[test]
fn w0307_comparison_after_else() {
    snap(
        "w0307_else_comparison",
        "fn maybe(n: int) -> bool ! {empty} {\n    \
             if n == 0 { return empty }\n    n > 1\n}\n\
         fn main() -> !int {\n    \
             let a = 1\n    let b = 1\n    \
             let ok = maybe(0) else a == b\n    \
             if ok { 0 } else { 1 }\n}\n",
    );
}

#[test]
fn w0308_mut_argument_in_interpolation() {
    snap(
        "w0308_mut_in_interp",
        "fn bump(mut n: int) -> int {\n    n += 1\n    n\n}\n\
         fn main() -> !int {\n    var a = 10\n    print(\"{bump(mut a)}\")\n    a - 11\n}\n",
    );
}

#[test]
fn w0309_raw_literal_interp_braces() {
    snap(
        "w0309_raw_braces",
        "fn main() -> !int {\n    let who = \"reader\"\n    let s = r\"{who}\"\n    \
         print(s)\n    if who == \"reader\" { 0 } else { 1 }\n}\n",
    );
}

#[test]
fn w0602_pub_anonymous_row() {
    snap(
        "w0602_pub_row",
        "/// Reads a thing; documented so only the row shape warns.\n\
         pub fn wide(path: str) -> int ! {stale, lost} {\n    \
             if path == \"\" { return stale }\n    1\n}\n\
         fn main() -> !int {\n    wide(\"p\") else 0\n}\n",
    );
}

#[test]
fn w1101_task_writes_captured_copy() {
    snap(
        "w1101_task_capture_write",
        "fn main() -> !int {\n    var sum = 0\n    scope s {\n        \
             s.spawn(fn() {\n            sum = sum + 1\n        })\n    }\n    \
             sum\n}\n",
    );
}

#[test]
fn w1102_capture_then_reassign() {
    snap(
        "w1102_stale_capture",
        "fn main() -> !int {\n    var total = 0\n    \
             let add = fn(c: int) total + c\n    \
             total = 5\n    \
             add(1) - 1\n}\n",
    );
}

#[test]
fn w1302_assume_operand_reassigned() {
    snap(
        "w1302_assume_reassigned",
        "import c \"stdlib.h\"\n\
         fn main() -> !int {\n    var out = 0\n    \
             // # Safety: p/q distinct live allocations; q rebinds to p\n    \
             // below, after which only p's cell is touched.\n    \
             unsafe {\n        \
                 let p = c.malloc(4) as *u8\n        \
                 var q = c.malloc(4) as *u8\n        \
                 assume noalias p, q\n        \
                 let spare = q\n        \
                 q = p\n        \
                 p[0] = 7\n        \
                 out = q[0] as int\n        \
                 c.free(spare)\n        \
                 c.free(p)\n    }\n    \
             out - 7\n}\n",
    );
}

// ----------------------------------------------- typecheck rung ----

#[test]
fn w0601_discarded_fallible_result() {
    snap_typed(
        "w0601_discarded_result",
        "fn fetch(n: int) -> int ! {empty} {\n    \
             if n == 0 { return empty }\n    n\n}\n\
         fn main() -> !int {\n    fetch(3)\n    \
             let kept = fetch(2) else 0\n    \
             if kept == 2 { 0 } else { 1 }\n}\n",
    );
}

#[test]
fn w0401_literal_does_not_fit_cast() {
    snap_typed(
        "w0401_narrowing_literal",
        "fn main() -> !int {\n    let flag = 1\n    \
             if flag == 0 {\n        let tiny = 300 as u8\n        \
             return tiny as int\n    }\n    0\n}\n",
    );
}

#[test]
fn w0402_zero_minus_negation() {
    snap_typed(
        "w0402_float_zero_minus",
        "fn main() -> !int {\n    let x = 1.5\n    let flipped = 0.0 - x\n    \
             if flipped < 0.0 { 0 } else { 1 }\n}\n",
    );
}

#[test]
fn w0801_capitalized_binder() {
    snap_typed(
        "w0801_binder_binds",
        "fn main() -> !int {\n    let n = 3\n    \
             let v = match n {\n        0 => 5,\n        Zed => Zed + 1,\n    }\n    \
             if v == 4 { 0 } else { 1 }\n}\n",
    );
}

// -------------------------------------------------- boundaries ----

#[test]
fn writes_through_projections_stay_silent() {
    // `p[0] = …` writes *through* a binding: none of W1101/W1102/
    // W1302's business (the mem tier owns those facts).
    let src = "import c \"stdlib.h\"\n\
         fn main() -> !int {\n    var out = 0\n    \
             // # Safety: p and q never alias; both freed once.\n    \
             unsafe {\n        \
                 let p = c.malloc(4) as *u8\n        \
                 let q = c.malloc(4) as *u8\n        \
                 assume noalias p, q\n        \
                 p[0] = 1\n        \
                 q[0] = 2\n        \
                 out = (p[0] + q[0]) as int\n        \
                 c.free(p)\n        \
                 c.free(q)\n    }\n    \
             out - 3\n}\n";
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics.is_empty(),
        "no warnings on projection writes: {:?}",
        res.diagnostics
    );
}

#[test]
fn when_bodies_are_exempt_from_w1101() {
    // [conc.when.body]: the acquired set is synchronized state — the
    // one shape where cross-task writes are the design.
    let src = "fn main() -> !int {\n    let a = Mutex(1)\n    let b = Mutex(2)\n    scope s {\n        \
         s.spawn(fn() {\n            when (a, b) { a += 1; b += 1 }\n        })\n    }\n    0\n}\n";
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics.is_empty(),
        "when bodies stay silent: {:?}",
        res.diagnostics
    );
}

#[test]
fn provisional_standins_are_exempt_from_w0304() {
    // `worker` and friends exist so corpus programs can define them.
    let src = "fn worker() -> int {\n    1\n}\nfn main() -> !int {\n    worker() - 1\n}\n";
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics.is_empty(),
        "stand-ins stay silent: {:?}",
        res.diagnostics
    );
}

// ------------------------------------------- the idiom arbiter ----

#[test]
fn w0310_get_prefix() {
    snap(
        "w0310_get_prefix",
        "fn get_total(a: int, b: int) -> int {\n    a + b\n}\n\
         fn main() -> !int {\n    get_total(1, 2) - 3\n}\n",
    );
}

#[test]
fn w0311_predicate_not_bool() {
    snap(
        "w0311_predicate_int",
        "fn is_positive(n: int) -> int {\n    if n > 0 { 1 } else { 0 }\n}\n\
         fn main() -> !int {\n    is_positive(3) - 1\n}\n",
    );
}

#[test]
fn w0312_as_view_consumes() {
    snap(
        "w0312_as_view_take",
        "fn as_length(take s: str) -> int {\n    s.len\n}\n\
         fn main() -> !int {\n    let t = \"four\"\n    as_length(take t) - 4\n}\n",
    );
}

#[test]
fn w0313_pub_without_doc() {
    snap(
        "w0313_pub_undocumented",
        "pub fn exported(n: int) -> int {\n    n + 1\n}\n\
         fn main() -> !int {\n    exported(1) - 2\n}\n",
    );
}

#[test]
fn w0603_tag_case_contradicts_payload() {
    snap(
        "w0603_tag_case_payload",
        "fn fetch(n: int) -> int ! {Stale, flat(int)} {\n    \
             if n < 0 { return Stale }\n    n\n}\n\
         fn main() -> !int {\n    let v = fetch(3) else 0\n    v - 3\n}\n",
    );
}

#[test]
fn w0603_none_with_payload() {
    snap(
        "w0603_none_payload",
        "fn lookup(n: int) -> int ! {none(int)} {\n    \
             if n < 0 { return none(0) }\n    n\n}\n\
         fn main() -> !int {\n    let v = lookup(2) else 0\n    v - 2\n}\n",
    );
}

#[test]
fn w0604_get_without_row() {
    snap(
        "w0604_get_total_fn",
        "fn get(i: int) -> int {\n    i * 2\n}\n\
         fn main() -> !int {\n    get(2) - 4\n}\n",
    );
}

#[test]
fn w1002_mut_param_never_written() {
    snap(
        "w1002_mut_unwritten",
        "fn offset(mut base: int, delta: int) -> int {\n    base + delta\n}\n\
         fn main() -> !int {\n    var b = 1\n    offset(mut b, 2) - 3\n}\n",
    );
}

#[test]
fn w1003_take_returned_unchanged() {
    snap(
        "w1003_take_returned",
        "fn keep(take v: int) -> int {\n    v\n}\n\
         fn main() -> !int {\n    let n = 5\n    keep(take n) - 5\n}\n",
    );
}

fn render_package_warnings(files: &[(&[&str], &str, &str)]) -> String {
    let mut ml = MemoryLoader::new("snap");
    for (path, name, src) in files {
        ml.add_file(path, name, src);
    }
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        !res.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error),
        "fixtures resolve without errors: {:?}",
        res.diagnostics
    );
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    let mut out = String::new();
    for d in res
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
    {
        out.push_str(&render_human(d, &sources, &RenderOptions::default()));
        out.push('\n');
    }
    assert!(!out.is_empty(), "every package fixture warns");
    out
}

#[test]
fn w0314_one_item_module() {
    let out = render_package_warnings(&[
        (
            &[],
            "main.lu",
            "use solo\nfn main() -> !int {\n    solo.only() - 1\n}\n",
        ),
        (
            &["solo"],
            "only.lu",
            "/// The module's only item.\npub fn only() -> int {\n    1\n}\n",
        ),
    ]);
    insta::assert_snapshot!("w0314_one_item_module", out);
}

#[test]
fn w0315_pkg_item_unused() {
    let out = render_package_warnings(&[
        (
            &[],
            "main.lu",
            "use store\nfn main() -> !int {\n    store.used() - 2\n}\n",
        ),
        (
            &["store"],
            "data.lu",
            "/// The taken-up export.\npub fn used() -> int {\n    2\n}\n\
             pub(pkg) fn spare() -> int {\n    9\n}\n",
        ),
    ]);
    insta::assert_snapshot!("w0315_pkg_unused", out);
}

#[test]
fn w0316_ancestor_import() {
    let out = render_package_warnings(&[
        (
            &[],
            "main.lu",
            "use outer\nuse outer.inner\n\
             fn main() -> !int {\n    outer.base() + inner.leaf() - 5\n}\n",
        ),
        (
            &["outer"],
            "base.lu",
            "/// The base value.\npub fn base() -> int {\n    2\n}\n\
             /// A second item.\npub fn twice() -> int {\n    4\n}\n",
        ),
        (
            &["outer", "inner"],
            "leaf.lu",
            "use outer\n\
             /// One more than base.\npub fn leaf() -> int {\n    outer.base() + 1\n}\n\
             /// A second item.\npub fn leaf2() -> int {\n    5\n}\n",
        ),
    ]);
    insta::assert_snapshot!("w0316_ancestor_import", out);
}

// -------------------------------------- arbiter boundary facts ----

fn resolve_only(src: &str) -> Vec<wolf_diag::Diagnostic> {
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    resolve_package_with(&mut ml, &AliasTable::default(), true)
        .expect("root loads")
        .diagnostics
}

#[test]
fn arbiter_clean_shapes_stay_silent() {
    // Documented pub, predicate answering bool, borrowed as_ view,
    // get with its absence row, written mut, transformed take, a row
    // variable in a generic signature, and correctly-cased tags: the
    // conventions' own shapes fire nothing.
    let src = "/// Documented.\npub fn fine(n: int) -> int {\n    n\n}\n\
         fn is_even(n: int) -> bool {\n    n % 2 == 0\n}\n\
         fn as_view(s: str) -> str {\n    s\n}\n\
         fn get(i: int) -> int ! {none} {\n    if i < 0 { return none }\n    i\n}\n\
         fn bump(mut n: int) -> int {\n    n += 1\n    n\n}\n\
         fn grow(take v: int) -> int {\n    v + 1\n}\n\
         fn wrap[T, E](x: T, f: fn(T) -> T ! {E}) -> T ! {E} {\n    f(x)?\n}\n\
         fn parse(s: str) -> int ! {Bad(int), eof} {\n    if s == \"\" { return eof }\n    if s == \"x\" { return Bad(0) }\n    1\n}\n\
         fn main() -> !int {\n    var m = 1\n    let g = get(2) else 0\n    let p = parse(\"ok\") else 0\n    \
             fine(1) + is_even(2).to_int() + as_view(\"a\").len + g + bump(mut m) + grow(take p) - 9\n}\n";
    let diags = resolve_only(src);
    assert!(diags.is_empty(), "conforming shapes stay silent: {diags:?}");
}

#[test]
fn w1002_fix_applicability_is_honest() {
    // Private fn, plain calls: the fix rewrites declaration AND call
    // sites, machine-applicable. A `pub` fn keeps a Maybe decl-only
    // fix (callers outside the module are invisible).
    let private = resolve_only(
        "fn offset(mut base: int, d: int) -> int {\n    base + d\n}\n\
         fn main() -> !int {\n    var b = 1\n    offset(mut b, 2) - 3\n}\n",
    );
    let w = private
        .iter()
        .find(|d| d.code.as_str() == "W1002")
        .expect("W1002 fires");
    let sug = w.suggestions.first().expect("carries a fix");
    assert_eq!(
        sug.applicability,
        wolf_diag::Applicability::MachineApplicable
    );
    assert_eq!(sug.edits.len(), 2, "declaration + one call site");

    let public = resolve_only(
        "/// Documented, exported, and warned once for the dead mut.\n\
         pub fn offset(mut base: int, d: int) -> int {\n    base + d\n}\n\
         fn main() -> !int {\n    var b = 1\n    offset(mut b, 2) - 3\n}\n",
    );
    let w = public
        .iter()
        .find(|d| d.code.as_str() == "W1002")
        .expect("W1002 fires on pub too");
    let sug = w.suggestions.first().expect("still carries a fix");
    assert_eq!(sug.applicability, wolf_diag::Applicability::Maybe);
    assert_eq!(sug.edits.len(), 1, "declaration only");
}

#[test]
fn package_shape_counterparts_stay_silent() {
    // Two-item module, used pub(pkg), parent-to-child import: the
    // healthy versions of all three structure lints.
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(
        &[],
        "main.lu",
        "use store\nuse store.depot\n\
         fn main() -> !int {\n    store.used() + store.also() + depot.spare() - 6\n}\n",
    );
    ml.add_file(
        &["store"],
        "data.lu",
        "use store.depot\n\
         /// Used.\npub fn used() -> int {\n    1\n}\n\
         /// Also used — and it takes the pkg item up.\npub fn also() -> int {\n    depot.spare() - 1\n}\n",
    );
    ml.add_file(
        &["store", "depot"],
        "d.lu",
        "/// Package-visible and taken up by the parent.\npub(pkg) fn spare() -> int {\n    2\n}\n\
         /// A second item.\npub fn other() -> int {\n    4\n}\n",
    );
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics.is_empty(),
        "healthy package shapes stay silent: {:?}",
        res.diagnostics
    );
}
