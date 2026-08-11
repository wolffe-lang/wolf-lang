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
        "pub fn wide(path: str) -> int ! {stale, lost} {\n    \
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
