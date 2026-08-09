//! Behavioral tests for the s13 checker: the checkable set checks, the
//! not-yet set refuses honestly, inference is stable, and the
//! independence/parallelism contract holds.

use wolf_sema::check::BodyResult;
use wolf_sema::{
    AliasTable, MemoryLoader, Resolution, Typecheck, resolve_package_with, typecheck_package_with,
};

fn resolve(files: &[(&[&str], &str, &str)]) -> Resolution {
    let mut ml = MemoryLoader::new("t");
    for (m, n, s) in files {
        ml.add_file(m, n, s);
    }
    resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads")
}

fn check(files: &[(&[&str], &str, &str)]) -> Typecheck {
    let res = resolve(files);
    assert!(
        res.diagnostics.is_empty(),
        "test inputs must resolve clean: {:?}",
        res.diagnostics
    );
    typecheck_package_with(&res.package, true)
}

fn check_one(src: &str) -> Typecheck {
    check(&[(&[], "main.lu", src)])
}

fn codes(tc: &Typecheck) -> Vec<&str> {
    tc.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

fn body<'a>(tc: &'a Typecheck, name: &str) -> &'a BodyResult {
    &tc.bodies
        .iter()
        .find(|b| b.body.name == name)
        .unwrap_or_else(|| panic!("no body named {name}"))
        .result
}

// ------------------------------------------------------ the happy set --

#[test]
fn hello_world_fully_checks() {
    let tc = check_one(
        "fn main() -> !int {\n    let who = \"wolf\"\n    print(\"hello, {who}\")\n    0\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    let BodyResult::Checked(t) = body(&tc, "main") else {
        panic!("main checks")
    };
    assert_eq!(t.local_type("who").as_deref(), Some("str"));
}

#[test]
fn ok_injection_accepts_plain_value_at_err_union() {
    // `main() -> !int { 0 }` must check (implicit T ⇒ !T in check mode).
    let tc = check_one("fn main() -> !int { 0 }\n");
    assert!(tc.fully_checked(), "{:?}", tc.diagnostics);
}

#[test]
fn deferred_row_tag_returns_through_err_union() {
    let tc = check_one(
        "fn may(flag: bool) -> !int {\n    if flag { return Failed }\n    7\n}\n\
         fn main() -> !int { may(false) }\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn arithmetic_and_locals_infer_through_context() {
    let tc = check_one("fn main() -> !int {\n    var d = 0\n    let x = 10 / d\n    x\n}\n");
    assert!(tc.fully_checked(), "{:?}", tc.diagnostics);
    let BodyResult::Checked(t) = body(&tc, "main") else {
        panic!("checked")
    };
    // `x` flows into the `!int` return: context, not defaulting.
    assert_eq!(t.local_type("x").as_deref(), Some("int"));
    assert_eq!(t.local_type("d").as_deref(), Some("int"));
}

#[test]
fn literal_defaulting_is_a_rule() {
    // `sum` never touches an annotated type: {integer} defaults to i32.
    let tc = check_one(
        "fn main() -> !int {\n    var sum = 0\n    for i in 1..10 { sum += i }\n    if sum == 45 { 0 } else { 1 }\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    let BodyResult::Checked(t) = body(&tc, "main") else {
        panic!("checked")
    };
    assert_eq!(t.local_type("sum").as_deref(), Some("i32"));
    assert_eq!(t.local_type("i").as_deref(), Some("i32"));
}

#[test]
fn float_literals_default_f64() {
    let tc = check_one("fn main() -> !int {\n    let x = 1.5\n    0\n}\n");
    assert!(tc.fully_checked());
    let BodyResult::Checked(t) = body(&tc, "main") else {
        panic!("checked")
    };
    assert_eq!(t.local_type("x").as_deref(), Some("f64"));
}

#[test]
fn struct_fields_on_nominal_values_check() {
    let tc = check_one(
        "struct P { x: int, y: int }\n\
         fn bump(mut a: int, mut b: int) { a += 1; b += 1 }\n\
         fn main() -> !int {\n    var p = P { x: 1, y: 2 }\n    bump(mut p.x, mut p.y)\n    if p.x == 2 && p.y == 3 { 0 } else { 1 }\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn closures_take_parameter_types_from_context() {
    let tc = check_one(
        "fn apply(f: fn(int) -> int, v: int) -> int { f(v) }\n\
         fn main() -> !int {\n    let r = apply(fn(a) a + 1, 41)\n    r\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn wrapping_types_are_integer_family() {
    let tc = check_one(
        "fn main() -> !int {\n    var h: wrapping[u32] = 0x9e3779b9\n    for _ in 0..8 { h = h * 1664525 + 1013904223 }\n    0\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn cross_module_calls_check_against_dependency_signatures() {
    let tc = check(&[
        (
            &[],
            "main.lu",
            "use geometry\n\nfn main() -> !int {\n    if geometry.area(3) == 9 { 0 } else { 1 }\n}\n",
        ),
        (
            &["geometry"],
            "shapes.lu",
            "pub fn area(side: int) -> int {\n    side * side\n}\n",
        ),
    ]);
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn match_with_literal_patterns_checks() {
    let tc = check_one(
        "fn classify(n: int) -> int {\n    match n {\n        0 => 10,\n        1 => 20,\n        other => other,\n    }\n}\n\
         fn main() -> !int { classify(1) }\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn tuple_destructuring_is_irrefutable_and_checked() {
    let tc = check_one("fn main() -> !int {\n    let (a, b) = (1, 2)\n    a + b\n}\n");
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn generic_identity_body_checks_with_rigid_vars() {
    // Unbounded generic bodies check against their own rigids…
    let tc = check_one("fn id[T](x: T) -> T { x }\nfn main() -> !int { 0 }\n");
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn unannotated_global_walks_but_reports_e0407() {
    let res = resolve(&[(
        &[],
        "main.lu",
        "let banner = \"hello\"\nfn main() -> !int { 0 }\n",
    )]);
    let tc = typecheck_package_with(&res.package, true);
    assert!(codes(&tc).contains(&"E0407"), "{:?}", tc.diagnostics);
}

// -------------------------------------------------- the not-yet set ----

fn is_nyc(tc: &Typecheck, name: &str) -> bool {
    matches!(body(tc, name), BodyResult::NotYetCheckable(_))
}

#[test]
fn method_calls_are_not_yet_checkable() {
    let tc = check_one("fn main() -> !int {\n    let s = \"abc\".upper()\n    0\n}\n");
    assert!(is_nyc(&tc, "main"));
    assert!(!tc.fully_checked());
}

#[test]
fn generic_instantiation_is_not_yet_checkable() {
    let tc = check_one("fn first[T](x: T) -> T { x }\nfn main() -> !int { first(1) }\n");
    assert!(is_nyc(&tc, "main"));
}

#[test]
fn else_defaulting_and_try_are_not_yet_checkable() {
    let tc = check_one(
        "fn may() -> !int { 1 }\n\
         fn a() -> !int { may() else 0 }\n\
         fn main() -> !int { 0 }\n",
    );
    assert!(is_nyc(&tc, "a"));
}

#[test]
fn concurrency_and_regions_are_not_yet_checkable() {
    let tc = check_one(
        "fn r() -> int { region tmp { 1 } }\n\
         fn main() -> !int { 0 }\n",
    );
    assert!(is_nyc(&tc, "r"));
}

#[test]
fn unsafe_tier_is_not_yet_checkable() {
    let tc = check_one("fn main() -> !int { unsafe { 0 } }\n");
    assert!(is_nyc(&tc, "main"));
}

#[test]
fn indexing_is_not_yet_checkable() {
    let tc =
        check_one("fn main() -> !int {\n    let s = \"abcdef\"\n    let h = s[0..3]\n    0\n}\n");
    assert!(is_nyc(&tc, "main"));
}

#[test]
fn equality_on_structs_waits_for_traits() {
    let tc = check_one(
        "struct Point { x: int }\n\
         fn main() -> !int {\n    let p = Point { x: 0 }\n    if p == (Point { x: 0 }) { 0 } else { 1 }\n}\n",
    );
    assert!(is_nyc(&tc, "main"));
}

// ----------------------------------------------------------- errors ----

#[test]
fn if_branch_mismatch_is_honest_both_ways() {
    let tc =
        check_one("fn main() -> !int {\n    let x = if true { 1 } else { \"one\" }\n    0\n}\n");
    assert_eq!(codes(&tc), ["E0401"]);
    let d = &tc.diagnostics[0];
    assert!(d.message.contains("disagree"), "{}", d.message);
    assert_eq!(d.secondary.len(), 1, "both branches shown");
}

#[test]
fn wrong_arg_count_is_e0402_with_definition_site() {
    let tc = check_one(
        "fn area(w: int, h: int) -> int { w * h }\n\
         fn main() -> !int { area(3) }\n",
    );
    assert_eq!(codes(&tc), ["E0402"]);
    assert_eq!(tc.diagnostics[0].secondary.len(), 1);
}

#[test]
fn unknown_field_is_e0403_with_typo_suggestion() {
    let tc = check_one(
        "struct Circle { radius: f64 }\n\
         fn main() -> !int {\n    let c = Circle { radius: 1.0 }\n    let r = c.radius\n    let bad = c.radbus\n    0\n}\n",
    );
    assert_eq!(codes(&tc), ["E0403"]);
    assert!(
        tc.diagnostics[0].suggestions[0].message.contains("radius"),
        "{:?}",
        tc.diagnostics[0]
    );
}

#[test]
fn infinite_type_is_e0404() {
    let tc = check_one("fn main() -> !int {\n    let f = fn(g) g(g)\n    0\n}\n");
    assert!(codes(&tc).contains(&"E0404"), "{:?}", codes(&tc));
}

#[test]
fn uninferable_closure_param_is_e0405() {
    let tc = check_one("fn main() -> !int {\n    let f = fn(x) x\n    0\n}\n");
    assert!(codes(&tc).contains(&"E0405"), "{:?}", codes(&tc));
}

#[test]
fn calling_a_struct_is_e0406() {
    let tc = check_one(
        "struct Point { x: int }\n\
         fn main() -> !int {\n    let p = Point(1)\n    0\n}\n",
    );
    assert!(codes(&tc).contains(&"E0406"), "{:?}", codes(&tc));
}

#[test]
fn missing_struct_fields_are_e0408() {
    let tc = check_one(
        "struct P { x: int, y: int }\n\
         fn main() -> !int {\n    let p = P { x: 1 }\n    0\n}\n",
    );
    assert_eq!(codes(&tc), ["E0408"]);
}

#[test]
fn logic_on_numbers_is_e0409() {
    let tc = check_one("fn main() -> !int {\n    if 1 && 2 { 0 } else { 1 }\n}\n");
    assert!(codes(&tc).contains(&"E0409"), "{:?}", codes(&tc));
}

#[test]
fn return_type_mismatch_has_provenance_chain() {
    let tc = check_one("fn answer() -> int { \"forty-two\" }\nfn main() -> !int { answer() }\n");
    assert_eq!(codes(&tc), ["E0401"]);
    let d = &tc.diagnostics[0];
    assert!(d.message.contains("`answer` must return"), "{}", d.message);
    assert_eq!(d.secondary.len(), 1, "the return type is the because span");
}

// ------------------------------------------- independence & stability --

#[test]
fn parallel_and_sequential_agree() {
    let files: &[(&[&str], &str, &str)] = &[(
        &[],
        "main.lu",
        "fn f(a: int) -> int { a + 1 }\n\
         fn g(b: int) -> str { \"x\" }\n\
         fn broken() -> int { \"no\" }\n\
         fn main() -> !int { f(1) }\n",
    )];
    let res_a = resolve(files);
    let a = typecheck_package_with(&res_a.package, true);
    let res_b = resolve(files);
    let b = typecheck_package_with(&res_b.package, false);
    assert_eq!(
        a.diagnostics.len(),
        b.diagnostics.len(),
        "thread interleaving never changes output"
    );
    for (x, y) in a.diagnostics.iter().zip(b.diagnostics.iter()) {
        assert_eq!(x.code, y.code);
        assert_eq!(x.message, y.message);
    }
}

/// Property: inferred types are stable under body-statement permutation
/// where dataflow permits.
#[test]
fn inference_is_stable_under_statement_permutation() {
    // Three independent bindings + a use that fixes none of them.
    let stmts = ["let a = 1", "let b = 2.5", "let c = true"];
    let perms: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let mut expected: Option<(String, String, String)> = None;
    for p in perms {
        let body: String = p.iter().map(|&i| format!("    {}\n", stmts[i])).collect();
        let src = format!("fn main() -> !int {{\n{body}    0\n}}\n");
        let tc = check_one(&src);
        assert!(tc.fully_checked(), "{:?}", tc.diagnostics);
        let BodyResult::Checked(t) = body_result(&tc) else {
            panic!("checked")
        };
        let got = (
            t.local_type("a").unwrap(),
            t.local_type("b").unwrap(),
            t.local_type("c").unwrap(),
        );
        match &expected {
            None => expected = Some(got),
            Some(e) => assert_eq!(*e, got, "permutation changed inference"),
        }
    }

    fn body_result(tc: &Typecheck) -> &BodyResult {
        &tc.bodies
            .iter()
            .find(|b| b.body.name == "main")
            .expect("main")
            .result
    }
}

#[test]
fn suppressed_regions_stay_quiet_in_typecheck() {
    // A parse wreck inside main: the resolver stays quiet, and so must
    // the type checker (the `<error>` convention).
    let src = "fn main() -> !int {\n    let == zzz\n    0\n}\n";
    let res = {
        let mut ml = MemoryLoader::new("t");
        ml.add_file(&[], "main.lu", src);
        resolve_package_with(&mut ml, &AliasTable::default(), true).expect("loads")
    };
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        tc.diagnostics.is_empty(),
        "no E04xx echoes off a parse wreck: {:?}",
        tc.diagnostics
    );
}
