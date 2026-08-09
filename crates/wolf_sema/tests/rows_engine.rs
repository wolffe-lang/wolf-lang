//! The s15 error-row engine: width-only subtyping at boundaries, `?`
//! propagation and re-tagging, `else` defaulting/handling, inferred-row
//! sealing (the Zig-trap fix, demonstrably), HOF row-tail polymorphism,
//! and the `errdefer`/trace-point records the typed HIR carries for
//! s27/s32.

use wolf_sema::{
    AliasTable, BodyResult, MemoryLoader, Typecheck, resolve_package_with, typecheck_package_with,
};

fn check(files: &[(&[&str], &str, &str)]) -> Typecheck {
    let mut ml = MemoryLoader::new("rows");
    for (m, n, s) in files {
        ml.add_file(m, n, s);
    }
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics.is_empty(),
        "inputs resolve clean: {:?}",
        res.diagnostics
    );
    typecheck_package_with(&res.package, true)
}

fn check_one(src: &str) -> Typecheck {
    check(&[(&[], "main.lu", src)])
}

fn codes(tc: &Typecheck) -> Vec<&'static str> {
    tc.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

// ------------------------------------------------------- propagation --

#[test]
fn try_widens_into_a_larger_row() {
    let tc = check_one(
        "fn read() -> int ! {NotFound(str)} { NotFound(\"boot\") }\n\
         fn parse() -> int ! {BadDigit, NotFound(str)} { read()? }\n\
         fn main() -> !int { parse() else 0 }\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn try_missing_tag_is_e0602_with_row_extending_fixit() {
    let tc = check_one(
        "fn read() -> int ! {NotFound(str)} { NotFound(\"boot\") }\n\
         fn render() -> int ! {Empty} { read()? }\n\
         fn main() -> !int { render() else 0 }\n",
    );
    assert_eq!(codes(&tc), ["E0602"]);
    let d = &tc.diagnostics[0];
    assert!(
        d.message.contains("`NotFound(str)`") && d.message.contains("`render`"),
        "names exactly the missing tag: {}",
        d.message
    );
    let sugg = &d.suggestions[0];
    assert_eq!(sugg.edits[0].1, ", NotFound(str)");
    let rd = d.row_diff.as_ref().expect("structural row diff rides");
    assert_eq!(rd.missing, ["NotFound(str)"]);
    assert!(rd.extra.is_empty());
    // The JSON line carries the diff structurally.
    let line = wolf_diag::render_json_line(d);
    assert!(
        line.contains("\"row_diff\":{\"missing\":[\"NotFound(str)\"],\"extra\":[]}"),
        "{line}"
    );
}

#[test]
fn return_boundary_also_widens() {
    // A narrower fallible value flows out as a wider one (checking
    // direction only — never subsumption inside the unifier).
    let tc = check_one(
        "fn a() -> int ! {X} { X }\n\
         fn b() -> int ! {X, Y} { a() }\n\
         fn main() -> !int { b() else 0 }\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn shared_tag_payload_conflict_is_e0606() {
    let tc = check_one(
        "fn read() -> int ! {NotFound(int)} { NotFound(4) }\n\
         fn show() -> int ! {NotFound(str)} { read()? }\n\
         fn main() -> !int { show() else 0 }\n",
    );
    assert_eq!(codes(&tc), ["E0606"]);
    assert!(tc.diagnostics[0].message.contains("`NotFound`"));
}

#[test]
fn raising_an_undeclared_tag_is_e0602() {
    let tc = check_one(
        "fn go() -> int ! {Io} { Broken }\n\
         fn main() -> !int { go() else 0 }\n",
    );
    assert_eq!(codes(&tc), ["E0602"]);
    assert!(tc.diagnostics[0].message.contains("`Broken`"));
}

#[test]
fn tag_payload_arity_mismatch_is_e0606() {
    let tc = check_one(
        "fn go() -> int ! {Bad(int)} { Bad }\n\
         fn main() -> !int { go() else 0 }\n",
    );
    assert_eq!(codes(&tc), ["E0606"]);
}

#[test]
fn tag_payloads_check_pointwise() {
    let tc = check_one(
        "fn go() -> int ! {Bad(int, str)} { Bad(1, 2) }\n\
         fn main() -> !int { go() else 0 }\n",
    );
    assert_eq!(codes(&tc), ["E0401"]);
    assert!(
        tc.diagnostics[0].message.contains("`Bad`"),
        "payload provenance names the tag: {}",
        tc.diagnostics[0].message
    );
}

// ------------------------------------------------- `?`/`else` misuse --

#[test]
fn try_on_infallible_value_is_e0603() {
    let tc = check_one("fn main() -> !int {\n    let x = 1\n    x?\n}\n");
    assert_eq!(codes(&tc), ["E0603"]);
}

#[test]
fn try_in_infallible_function_is_e0604_with_fixit() {
    let tc = check_one(
        "fn may() -> int ! {Bad} { Bad }\n\
         fn f() -> int { may()? }\n\
         fn main() -> !int { f() }\n",
    );
    assert_eq!(codes(&tc), ["E0604"]);
    let sugg = &tc.diagnostics[0].suggestions[0];
    assert_eq!(sugg.edits[0].1, " ! {Bad}");
}

#[test]
fn else_on_infallible_value_is_e0608() {
    let tc = check_one("fn main() -> !int {\n    let v = 1 else 0\n    v\n}\n");
    assert_eq!(codes(&tc), ["E0608"]);
}

#[test]
fn else_handler_binds_the_row_value() {
    let tc = check_one(
        "fn may() -> int ! {Oops} { Oops }\n\
         fn main() -> !int {\n    let v = may() else |err| 0\n    v\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    let body = tc
        .bodies
        .iter()
        .find(|b| b.body.name == "main")
        .expect("main");
    let BodyResult::Checked(tb) = &body.result else {
        panic!("checked")
    };
    assert_eq!(
        tb.local_type("err").as_deref(),
        Some("{Oops}"),
        "the handler binds the caught error at the row type"
    );
}

// ------------------------------------------------------- errdefer -----

#[test]
fn errdefer_needs_a_fallible_function() {
    let tc = check_one(
        "fn f() -> int {\n    errdefer print(\"cleanup\")\n    1\n}\n\
         fn main() -> !int { f() }\n",
    );
    assert_eq!(codes(&tc), ["E0607"]);
    let sugg = &tc.diagnostics[0].suggestions[0];
    assert_eq!(sugg.edits[0].1, "defer");
}

#[test]
fn cleanups_and_trace_points_ride_the_typed_hir() {
    let tc = check_one(
        "fn may() -> int ! {Bad} { Bad }\n\
         fn work() -> !int {\n    errdefer print(\"undo\")\n    defer print(\"always\")\n    let v = may()?\n    v\n}\n\
         fn main() -> !int { work() else 0 }\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    let body = tc
        .bodies
        .iter()
        .find(|b| b.body.name == "work")
        .expect("work");
    let BodyResult::Checked(tb) = &body.result else {
        panic!("checked")
    };
    // Declaration order, errdefer flagged — s27 lowers the strict
    // LIFO interleave from exactly this.
    assert_eq!(tb.cleanups.len(), 2);
    assert!(tb.cleanups[0].1, "first cleanup is the errdefer");
    assert!(!tb.cleanups[1].1, "second is the plain defer");
    assert!(
        !tb.trace_points.is_empty(),
        "each `?` site is an error-trace hook point"
    );
}

// ------------------------------------------------------- sealing ------

#[test]
fn inferred_rows_seal_through_recursion_and_pointers() {
    // The Zig trap, demonstrably fixed: recurse through and take a
    // pointer to an inferred-row private function — both compile,
    // because the sealed row makes it an ordinary first-class value.
    let tc = check_one(
        "fn helper(n: int) -> !int {\n    if n == 0 { return Empty }\n    helper(n - 1)?\n}\n\
         fn main() -> !int {\n    let f = helper\n    let v = f(3)?\n    v\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    let body = tc
        .bodies
        .iter()
        .find(|b| b.body.name == "main")
        .expect("main");
    let BodyResult::Checked(tb) = &body.result else {
        panic!("checked")
    };
    assert_eq!(
        tb.local_type("f").as_deref(),
        Some("fn(int) -> int ! {Empty}"),
        "the pointer's type carries the sealed concrete row"
    );
    // The sealed facts surface for `wolf interface`.
    let sealed: Vec<&str> = tc.sigs.sealed.iter().map(|(_, _, r)| r.as_str()).collect();
    assert!(sealed.contains(&"fn helper -> int ! {Empty}"), "{sealed:?}");
}

#[test]
fn mutual_recursion_reaches_the_fixpoint() {
    let tc = check_one(
        "fn even(n: int) -> !bool {\n    if n == 0 { return Zero }\n    odd(n - 1)?\n}\n\
         fn odd(n: int) -> !bool {\n    if n == 0 { return Stuck }\n    even(n - 1)?\n}\n\
         fn main() -> !int {\n    let e = even(4) else |_| false\n    if e { 0 } else { 1 }\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    let sealed: Vec<&str> = tc.sigs.sealed.iter().map(|(_, _, r)| r.as_str()).collect();
    assert!(
        sealed.contains(&"fn even -> bool ! {Stuck, Zero}"),
        "both cycle members absorb both tags: {sealed:?}"
    );
    assert!(
        sealed.contains(&"fn odd -> bool ! {Stuck, Zero}"),
        "{sealed:?}"
    );
}

#[test]
fn pub_inferred_row_is_e0605_with_state_the_row_fixit() {
    let tc = check_one(
        "pub fn boom() -> !int { Bad }\n\
         fn main() -> !int { 0 }\n",
    );
    assert_eq!(codes(&tc), ["E0605"]);
    let sugg = &tc.diagnostics[0].suggestions[0];
    assert_eq!(sugg.edits[0].1, "-> int ! {Bad}");
}

#[test]
fn pub_inferred_row_that_cannot_fail_suggests_dropping_the_bang() {
    let tc = check_one(
        "pub(pkg) fn calm() -> !int { 7 }\n\
         fn main() -> !int { 0 }\n",
    );
    assert_eq!(codes(&tc), ["E0605"]);
    let sugg = &tc.diagnostics[0].suggestions[0];
    assert_eq!(sugg.edits[0].1, "-> int");
}

// ------------------------------------------------- HOF row tails ------

#[test]
fn hof_row_tail_polymorphism_via_row_variables() {
    // `rethrows` solved by a type parameter (SE-0413's conclusion):
    // the row variable `{E}` unifies with each call site's row.
    let tc = check_one(
        "fn apply[T, E](x: T, f: fn(T) -> T ! {E}) -> T ! {E} { f(x)? }\n\
         fn half(n: int) -> int ! {Neg} {\n    if n < 0 { return Neg }\n    n / 2\n}\n\
         fn main() -> !int {\n    let v = apply(4, half) else 0\n    v\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn abstract_row_rejects_concrete_injection() {
    // Inside the generic body, `E` is the caller's row: a concrete
    // tag cannot be assumed into it.
    let tc = check_one(
        "fn apply[T, E](x: T, f: fn(T) -> T ! {E}) -> T ! {E} {\n    if false { return Hijack }\n    f(x)?\n}\n\
         fn main() -> !int { 0 }\n",
    );
    assert_eq!(codes(&tc), ["E0602"]);
    assert!(tc.diagnostics[0].message.contains("abstract row"));
}

// ------------------------------------------- three-module widening ----

#[test]
fn propagation_widens_across_module_hops() {
    let tc = check(&[
        (
            &[],
            "main.lu",
            "use parse\n\nfn main() -> !int {\n    let v = parse.pair(\"42\") else |_| 0\n    v\n}\n",
        ),
        (
            &["parse"],
            "parse.lu",
            "use io\n\npub fn pair(s: str) -> int ! {BadDigit, NotFound(str), Locked} { io.read(s)? }\n",
        ),
        (
            &["io"],
            "io.lu",
            "pub fn read(path: str) -> int ! {Locked, NotFound(str)} {\n    if path == \"\" { return NotFound(path) }\n    Locked\n}\n",
        ),
    ]);
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}
