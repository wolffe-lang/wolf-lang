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
            "/// Area.\npub fn area(side: int) -> int {\n    side * side\n}\n\
             /// Twice.\npub fn twice(side: int) -> int {\n    side + side\n}\n",
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
fn str_methods_type_since_s37() {
    // s13 refused builtin-receiver methods honestly; s37's builtin
    // `str` surface types them (D24/D25).
    let tc = check_one("fn main() -> !int {\n    let s = \"abc\".upper()\n    s.len\n}\n");
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    // Outside the s37 set stays an honest refusal.
    let tc = check_one("fn main() -> !int {\n    let s = \"abc\".frobnicate()\n    0\n}\n");
    assert!(is_nyc(&tc, "main"));
}

#[test]
fn generic_instantiation_checks_since_s14() {
    // s13 refused this honestly; s14's instantiation solves `T` from
    // the argument and the call checks end to end.
    let tc = check_one("fn first[T](x: T) -> T { x }\nfn main() -> !int { first(1) }\n");
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn else_defaulting_and_try_check_since_s15() {
    // s13 refused these honestly; s15's row engine types them.
    let tc = check_one(
        "fn may() -> !int { 1 }\n\
         fn a() -> !int { may() else 0 }\n\
         fn b() -> !int { may()? }\n\
         fn main() -> !int { 0 }\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn regions_type_since_s19_freeze_since_s20() {
    // s13 refused every region form; s19 types the creation/ambient
    // surface (X4); s20 types `freeze`: a frozen region block yields
    // its body's value, `freeze r` on a region value yields `region`,
    // and a non-region operand is a type error.
    let tc = check_one(
        "fn r() -> int { region tmp { 1 } }\n\
         fn v() -> int { let a = region(rc)\n    in a { 2 } }\n\
         fn main() -> !int { 0 }\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    let tc = check_one(
        "fn f() -> int { freeze region { 1 } }\n\
         fn g() -> int { let a = region()\n    let b = freeze a\n    in b { 2 } }\n\
         fn main() -> !int { 0 }\n",
    );
    assert!(
        tc.fully_checked() && !tc.has_errors(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    let tc = check_one("fn main() -> !int { let x = 1\n    freeze x\n    0 }\n");
    assert!(tc.has_errors(), "{:?}", tc.diagnostics);
}

#[test]
fn in_target_must_be_a_region() {
    let tc = check_one("fn main() -> !int {\n    let x = 1\n    in x { 2 }\n    0\n}\n");
    assert!(tc.has_errors(), "{:?}", tc.diagnostics);
}

#[test]
fn unsafe_blocks_typecheck_since_s22_but_asm_still_refuses() {
    // s22: `unsafe { }` types as its body's value inside a fully safe
    // signature ([mem.unsafe.scope]); the ring *rules* are wolf_mem's.
    let tc = check_one("fn main() -> !int { unsafe { 0 } }\n");
    assert!(
        tc.not_yet.is_empty(),
        "unsafe blocks type now: {:?}",
        tc.not_yet
    );
    assert!(!tc.has_errors(), "{:?}", tc.diagnostics);
    // Inline asm has no pinned semantics until c10 — still honest.
    let tc = check_one(
        "fn main() -> !int {\n    var t = 0\n    unsafe {\n        asm {\n            \"nop\",\n        }\n    }\n    t\n}\n",
    );
    assert!(is_nyc(&tc, "main"));
}

#[test]
fn str_slicing_types_since_s37() {
    // `s[a..b]` is the D25 checked byte slice — typed as `str`,
    // including `^n` end-relative endpoints.
    let tc = check_one(
        "fn main() -> !int {\n    let s = \"abcdef\"\n    let h = s[0..3]\n    let t = s[..^1]\n    h.len + t.len\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    // Bracket application outside the container/str set stays an
    // honest refusal.
    let tc = check_one("fn main() -> !int {\n    let n = 7\n    let h = n[0..2]\n    0\n}\n");
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

// ----------------------------------------------- str ordering (#7) ----

#[test]
fn str_relational_operators_type() {
    // The #7 ruling: `str` orders byte-lexicographically — `<` family
    // yields `bool`, `<=>` yields `int`.
    let tc = check_one(
        "fn main() -> !int {\n    let a = \"apple\" < \"banana\"\n    let o = \"a\" <=> \"b\"\n    if a { o } else { 1 }\n}\n",
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
    assert_eq!(t.local_type("a").as_deref(), Some("bool"));
    assert_eq!(t.local_type("o").as_deref(), Some("int"));
}

#[test]
fn str_compared_to_number_still_reports() {
    let tc = check_one("fn main() -> !int {\n    let b = \"a\" < 3\n    0\n}\n");
    assert!(!tc.diagnostics.is_empty(), "mixed operands still report");
}

// ------------------------------------------------ assert arity (#9) ----

#[test]
fn assert_takes_an_optional_message() {
    let tc = check_one(
        "fn main() -> !int {\n    let x = 3\n    assert(x > 0, \"positive\")\n    assert(x > 0)\n    0\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn assert_wrong_arity_is_e0402() {
    let tc = check_one("fn main() -> !int {\n    assert(true, \"m\", 3)\n    0\n}\n");
    assert_eq!(codes(&tc), ["E0402"]);
    assert!(
        tc.diagnostics[0].message.contains("1 or 2"),
        "{}",
        tc.diagnostics[0].message
    );
}

// -------------------------------------- closure capture records (s105) ----

/// Every closure records its capture set, keyed by the closure
/// expression's span — the s73 spawn machinery generalized. The WIR
/// and mem tiers read this record; nothing re-derives captures.
#[test]
fn closures_record_their_capture_sets() {
    let tc = check_one(
        "fn main() -> !int {\n    let base = 30\n    let f = fn(x: int) { x + base }\n    let g = fn() 2\n    if f(4) + g() == 36 { 0 } else { 1 }\n}\n",
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
    // Two closures, two records: `f` captures `base`, `g` nothing.
    let mut sets: Vec<Vec<&str>> = t
        .task_captures
        .iter()
        .map(|(_, caps)| caps.iter().map(|c| c.name.as_str()).collect())
        .collect();
    sets.sort();
    assert_eq!(sets, [Vec::<&str>::new(), vec!["base"]]);
}

/// A closure parameter is not a capture, and a nested closure's
/// captures propagate into the enclosing closure's record.
#[test]
fn nested_closure_captures_propagate() {
    let tc = check_one(
        "fn main() -> !int {\n    let k = 7\n    let outer = fn(x: int) {\n        let inner = fn() k\n        x + inner()\n    }\n    if outer(1) == 8 { 0 } else { 1 }\n}\n",
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
    let mut sets: Vec<Vec<&str>> = t
        .task_captures
        .iter()
        .map(|(_, caps)| caps.iter().map(|c| c.name.as_str()).collect())
        .collect();
    sets.sort();
    // inner records `k`; outer records `k` too (its body resolves it
    // below the outer limit). `x` is a parameter, never a capture.
    assert_eq!(sets, [vec!["k"], vec!["k"]]);
}

// -------------------------------------------------- #34: nested rows --

/// A nested row parses (s108: the grammar's `type '!' error_row`
/// admits its own result) and FLATTENS (D51): `T ! {a} ! {b}` is
/// `T ! {a ∪ b}` — one union, tags merged, in return and parameter
/// position alike. The bodies check clean; nothing downstream knows
/// the layers were ever spelled apart.
#[test]
fn nested_row_flattens() {
    for src in [
        // Return position (#34's original reproducer).
        "fn nest(x: int) -> int ! {none} ! {none} {\n    if x < 1 { return none }\n    x\n}\n\
         fn main() -> !int {\n    let v = nest(16) else { return 1 }\n    if v == 16 { 0 } else { 1 }\n}\n",
        // Parameter position.
        "fn take_nested(v: int ! {none} ! {none}) -> int {\n    7\n}\n\
         fn main() -> !int {\n    if take_nested(3) == 7 { 0 } else { 1 }\n}\n",
        // A payload-carrying tag in both layers with the SAME payload
        // type merges silently (union semantics).
        "fn poke(n: int) -> int ! {Bad(int), none} ! {Bad(int)} {\n    if n == 0 { return Bad(7) }\n    if n == 1 { return none }\n    n\n}\n\
         fn main() -> !int {\n    let a = poke(0) else 3\n    if a == 3 { 0 } else { 1 }\n}\n",
    ] {
        let tc = check_one(src);
        assert!(
            tc.fully_checked() && tc.diagnostics.is_empty(),
            "expected the flattened union to check clean for {src:?}: {:?} / {:?}",
            tc.diagnostics,
            tc.not_yet
        );
    }
}

// --------------------------------------- #38 / D52: declared-row-first --

/// D52 ([gram.expr.tagident]): a bare lowercase identifier in a
/// checked position whose expected type is an error union declaring
/// that tag resolves as the tag — argument position and annotated
/// `let` position join the raise-site rule.
#[test]
fn declared_tag_resolves_at_argument_and_let() {
    for src in [
        // Argument position (`or(none, 9)` — std.option's shape).
        "fn or(v: int ! {none}, d: int) -> int {\n    v else d\n}\n\
         fn main() -> !int {\n    if or(none, 9) == 9 { 0 } else { 1 }\n}\n",
        // Annotated-let position.
        "fn main() -> !int {\n    let v: int ! {none} = none\n    let w = v else 5\n    if w == 5 { 0 } else { 1 }\n}\n",
    ] {
        let tc = check_one(src);
        assert!(
            tc.fully_checked() && !tc.has_errors(),
            "expected the declared tag to resolve for {src:?}: {:?} / {:?}",
            tc.diagnostics,
            tc.not_yet
        );
    }
}

/// D52's priced hazard: a local named like a declared tag SHADOWS it
/// (locals win, resolution's rule everywhere) — and W0305 warns at
/// the use, so the collision is never silent.
#[test]
fn local_shadows_declared_tag_and_w0305_warns() {
    let tc = check_one(
        "fn or(v: int ! {none}, d: int) -> int {\n    v else d\n}\n\
         fn main() -> !int {\n    let none = 3\n    if or(none, 9) == 3 { 0 } else { 1 }\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    assert_eq!(codes(&tc), ["W0305"], "{:?}", tc.diagnostics);
}

/// D51's recorded cost: the same tag in both layers with CONFLICTING
/// payload types cannot flatten — E0609, once, at the outer entry.
#[test]
fn nested_row_payload_conflict_is_e0609() {
    let tc = check_one(
        "fn poke(n: int) -> int ! {Bad(int)} ! {Bad(str)} {\n    n\n}\n\
         fn main() -> !int {\n    poke(3) else 0\n}\n",
    );
    assert_eq!(codes(&tc), ["E0609"], "{:?}", tc.diagnostics);
}

// ------------------------------------------------ #116b: nested fns --

/// A nested named fn checks as a capture-free fn value and binds like
/// a `let`: direct call, HOF pass, and call through a binding all
/// type.
#[test]
fn nested_fn_checks_and_binds() {
    let tc = check_one(
        "fn apply(f: fn(int) -> bool, v: int) -> bool { f(v) }\n\
         fn main() -> !int {\n    fn odd(v: int) -> bool { v % 2 == 1 }\n    if odd(3) {} else { return 1 }\n    if apply(odd, 5) {} else { return 2 }\n    let g = odd\n    if g(7) {} else { return 3 }\n    0\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

/// The scoped-out shapes refuse by name: a capture of an enclosing
/// local, and a generic nested fn.
#[test]
fn nested_fn_refusals_are_named() {
    let cap = check_one(
        "fn main() -> !int {\n    let base = 4\n    fn plus(v: int) -> int { v + base }\n    if plus(1) == 5 { 0 } else { 1 }\n}\n",
    );
    assert!(
        cap.not_yet
            .iter()
            .any(|n| n.construct.contains("capturing enclosing locals")),
        "{:?}",
        cap.not_yet
    );
    let generic = check_one(
        "fn main() -> !int {\n    fn id[T](v: T) -> T { v }\n    if id(1) == 1 { 0 } else { 1 }\n}\n",
    );
    assert!(
        generic
            .not_yet
            .iter()
            .any(|n| n.construct.contains("a generic nested fn")),
        "{:?}",
        generic.not_yet
    );
}
