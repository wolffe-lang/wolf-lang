//! Behavioral tests for the s16 comptime engine: the `evaluate`
//! boundary (values in, values out — no checker types), memoization,
//! same-host determinism of content hashes, float canonicalization,
//! reflection round trips, and the derive-class exemplar whose
//! generated impl passes ordinary s14 checking.

use wolf_sema::ctfe::derive::{Provenance, check_generated, derive_eq};
use wolf_sema::ctfe::{Budget, CtType, Engine, ValId, ValueKind};
use wolf_sema::{
    AliasTable, MemoryLoader, Prim, Resolution, TraitRef, Typecheck, resolve_package_with,
    typecheck_package_with,
};

fn resolve_one(src: &str) -> Resolution {
    let mut ml = MemoryLoader::new("t");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics.is_empty(),
        "test inputs resolve clean: {:?}",
        res.diagnostics
    );
    res
}

fn check(res: &Resolution) -> Typecheck {
    typecheck_package_with(&res.package, true)
}

/// The module a named body lives in (single-file packages).
fn module_of(tc: &Typecheck, name: &str) -> usize {
    tc.bodies
        .iter()
        .find(|b| b.body.name == name)
        .unwrap_or_else(|| panic!("no body named {name}"))
        .body
        .module
}

fn int_of(e: &Engine<'_>, v: ValId) -> i128 {
    match e.arena.kind(v) {
        ValueKind::Int { v, .. } => *v,
        other => panic!("expected an integer value, got {other:?}"),
    }
}

// --------------------------------------------------- the boundary ------

const FIB: &str = "comptime fn fib(n: int) -> int {\n    \
     if n < 2 { n } else { fib(n - 1) + fib(n - 2) }\n}\n\n\
     fn main() -> !int { 0 }\n";

#[test]
fn evaluate_boundary_fib_memoizes() {
    let res = resolve_one(FIB);
    let tc = check(&res);
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    let m = module_of(&tc, "fib");
    let mut e = Engine::new(&res.package, &tc.sigs, &tc.bodies);
    let arg = e.arena.int(25, Prim::Int);
    let v = e
        .evaluate(m, "fib", &[arg], Budget::default())
        .expect("fib(25) evaluates");
    assert_eq!(int_of(&e, v), 75025);
    // Memoization linearizes the naive recursion: without it fib(25)
    // is ~250k calls; with it, one body per distinct argument. The
    // fuel spent must sit far under the default budget.
    assert!(
        e.stats.fuel_used < 50_000,
        "memoization linearizes fib: {} steps",
        e.stats.fuel_used
    );
    // Re-evaluations are pure memo hits — the hit rate crosses 1/2.
    for n in 0..=25 {
        let a = e.arena.int(n, Prim::Int);
        let r = e.evaluate(m, "fib", &[a], Budget::default()).expect("hit");
        assert!(matches!(e.arena.kind(r), ValueKind::Int { .. }));
    }
    assert!(
        e.stats.hit_rate() > 0.5,
        "memo hit rate {} with {} evals",
        e.stats.hit_rate(),
        e.stats.evals
    );
    assert_eq!(e.stats.evals, 27, "one root eval plus 26 re-evaluations");
}

// ----------------------------------------------- same-host determinism --

const DETERMINISM: &str = "\
comptime fn fib(n: int) -> int {\n    \
if n < 2 { n } else { fib(n - 1) + fib(n - 2) }\n}\n\n\
comptime fn mknan() -> f64 {\n    0.0 / 0.0\n}\n\n\
comptime fn greet(who: str) -> str {\n    who\n}\n\n\
struct Pt {\n    x: int,\n    y: int,\n}\n\n\
comptime fn mk(a: int, b: int) -> Pt {\n    Pt { x: a, y: b }\n}\n\n\
comptime fn info_name(T: type) -> str {\n    typeinfo(T).name\n}\n\n\
fn main() -> !int { 0 }\n";

/// Every call in the determinism suite, against one fresh engine.
/// Returns the result content hashes keyed by call index.
fn run_suite(res: &Resolution, tc: &Typecheck, order: &[usize]) -> Vec<[u8; 32]> {
    let m = module_of(tc, "fib");
    let mut e = Engine::new(&res.package, &tc.sigs, &tc.bodies);
    let mut out = vec![[0u8; 32]; 5];
    for &i in order {
        let v = match i {
            0 => {
                let a = e.arena.int(20, Prim::Int);
                e.evaluate(m, "fib", &[a], Budget::default())
            }
            1 => e.evaluate(m, "mknan", &[], Budget::default()),
            2 => {
                let a = e.arena.str_("wolf");
                e.evaluate(m, "greet", &[a], Budget::default())
            }
            3 => {
                let a = e.arena.int(3, Prim::Int);
                let b = e.arena.int(4, Prim::Int);
                e.evaluate(m, "mk", &[a, b], Budget::default())
            }
            4 => {
                let t = e.arena.ty(CtType::Nominal {
                    module: m as u32,
                    name: "Pt".to_string(),
                });
                e.evaluate(m, "info_name", &[t], Budget::default())
            }
            _ => unreachable!(),
        }
        .unwrap_or_else(|f| panic!("call {i} evaluates: {f:?}"));
        out[i] = e.arena.content_hash(v);
    }
    out
}

#[test]
fn same_host_determinism_shuffled_call_orders() {
    // Two fresh packages, two fresh engines, different call orders:
    // the content hashes — the determinism surface — must agree
    // exactly (arena ids are creation-order accidents; hashes are
    // portable).
    let res1 = resolve_one(DETERMINISM);
    let tc1 = check(&res1);
    assert!(
        tc1.fully_checked(),
        "{:?} / {:?}",
        tc1.diagnostics,
        tc1.not_yet
    );
    let res2 = resolve_one(DETERMINISM);
    let tc2 = check(&res2);
    let h1 = run_suite(&res1, &tc1, &[0, 1, 2, 3, 4]);
    let h2 = run_suite(&res2, &tc2, &[3, 1, 4, 0, 2]);
    let h3 = run_suite(&res1, &tc1, &[4, 3, 2, 1, 0]);
    assert_eq!(h1, h2, "hashes are call-order and engine independent");
    assert_eq!(h1, h3, "hashes are call-order independent within a package");
}

#[test]
fn single_thread_and_parallel_checking_agree() {
    // The package-level comptime pass runs after bodies check; its
    // diagnostics must not depend on the checking schedule.
    let src = "\
comptime fn brim() -> i32 {\n    let big: i32 = 2147483647\n    big + 1\n}\n\n\
comptime fn shout() -> bool {\n    assert(1 == 2)\n    true\n}\n\n\
fn main() -> !int {\n    const B = brim()\n    const S = shout()\n    0\n}\n";
    let res = resolve_one(src);
    let seq = typecheck_package_with(&res.package, true);
    let par = typecheck_package_with(&res.package, false);
    let render = |tc: &Typecheck| -> Vec<String> {
        tc.diagnostics.iter().map(|d| format!("{d:?}")).collect()
    };
    assert_eq!(render(&seq), render(&par));
    assert!(!seq.diagnostics.is_empty(), "the suite has real faults");
}

// ------------------------------------------------- float determinism ---

#[test]
fn nan_results_canonicalize() {
    let res = resolve_one(DETERMINISM);
    let tc = check(&res);
    let m = module_of(&tc, "mknan");
    let expect_bits = 0x7ff8_0000_0000_0000u64;
    for _ in 0..2 {
        let mut e = Engine::new(&res.package, &tc.sigs, &tc.bodies);
        let v = e
            .evaluate(m, "mknan", &[], Budget::default())
            .expect("0.0 / 0.0 evaluates to NaN, not a fault");
        match e.arena.kind(v) {
            ValueKind::Float { bits, w } => {
                assert_eq!(*w, Prim::F64);
                assert_eq!(*bits, expect_bits, "NaN payloads never leak host defaults");
            }
            other => panic!("expected a float, got {other:?}"),
        }
    }
}

// ------------------------------------------- reflection round trips ----

const REFLECT: &str = "\
trait Show {\n    fn show(v: Self) -> str\n}\n\n\
struct Pt {\n    x: int,\n    y: int,\n}\n\n\
impl Show for Pt {\n    fn show(v: Pt) -> str {\n        \"pt\"\n    }\n}\n\n\
comptime fn built(desc: int) -> type {\n    typebuild(desc)\n}\n\n\
comptime fn built_fields(desc: int) -> int {\n    typeinfo(typebuild(desc)).fields.len\n}\n\n\
comptime fn showable(T: type) -> bool {\n    implements(T, Show)\n}\n\n\
fn main() -> !int { 0 }\n";

/// The `(name, type)` pair descriptor `typebuild` consumes, as a
/// comptime value: `[("x", int), ("y", f64)]`.
fn descriptor(e: &mut Engine<'_>) -> ValId {
    let sx = e.arena.str_("x");
    let tint = e.arena.ty(CtType::Prim(Prim::Int));
    let sy = e.arena.str_("y");
    let tf64 = e.arena.ty(CtType::Prim(Prim::F64));
    let p1 = e.arena.intern(ValueKind::Tuple(vec![sx, tint]));
    let p2 = e.arena.intern(ValueKind::Tuple(vec![sy, tf64]));
    e.arena.intern(ValueKind::Slice(vec![p1, p2]))
}

#[test]
fn typebuild_typeinfo_round_trip() {
    let res = resolve_one(REFLECT);
    let tc = check(&res);
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    let m = module_of(&tc, "built");
    // The synthesized type comes back as a self-contained type value.
    let mut e = Engine::new(&res.package, &tc.sigs, &tc.bodies);
    let desc = descriptor(&mut e);
    let t = e
        .evaluate(m, "built", &[desc], Budget::default())
        .expect("typebuild evaluates");
    match e.arena.kind(t) {
        ValueKind::Type(ct) => {
            assert_eq!(ct.render(), "struct { x: int, y: f64 }");
        }
        other => panic!("expected a type value, got {other:?}"),
    }
    // And reflecting the synthesized type sees the same fields back.
    let desc2 = descriptor(&mut e);
    let n = e
        .evaluate(m, "built_fields", &[desc2], Budget::default())
        .expect("typeinfo(typebuild(desc)) evaluates");
    assert_eq!(int_of(&e, n), 2);
}

#[test]
fn implements_answers_true_and_false() {
    let res = resolve_one(REFLECT);
    let tc = check(&res);
    let m = module_of(&tc, "showable");
    let mut e = Engine::new(&res.package, &tc.sigs, &tc.bodies);
    let pt = e.arena.ty(CtType::Nominal {
        module: m as u32,
        name: "Pt".to_string(),
    });
    let yes = e
        .evaluate(m, "showable", &[pt], Budget::default())
        .expect("implements(Pt, Show) evaluates");
    assert_eq!(*e.arena.kind(yes), ValueKind::Bool(true));
    let int_ty = e.arena.ty(CtType::Prim(Prim::Int));
    let no = e
        .evaluate(m, "showable", &[int_ty], Budget::default())
        .expect("implements(int, Show) evaluates");
    assert_eq!(*e.arena.kind(no), ValueKind::Bool(false));
}

// ------------------------------------------------ the derive exemplar --

#[test]
fn derive_eq_generated_impl_passes_s14_checking() {
    // The generator consumes semantic values (reflection over `Pt`'s
    // fields) and produces semantic values (an impl definition) — and
    // the output goes through ordinary s14 conformance checking, clean.
    let src = "\
trait Eq {\n    fn eq(a: Self, b: Self) -> bool\n}\n\n\
struct Pt {\n    x: int,\n    y: int,\n}\n\n\
fn main() -> !int { 0 }\n";
    let res = resolve_one(src);
    let tc = check(&res);
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
    let m = module_of(&tc, "main");
    let site = wolf_span::Span::new(res.package.files[0].raw.file, 0, 0);
    let prov = Provenance {
        generator: "derive_eq".to_string(),
        call_site: site,
    };
    let tr = TraitRef {
        module: m,
        name: "Eq".to_string(),
    };
    let generated = derive_eq(&tc.sigs, m, "Pt", &tr, &prov).expect("Pt is derivable");
    let diags = check_generated(&res.package, &tc.sigs, generated, &prov);
    assert!(
        diags.is_empty(),
        "the generated impl conforms to `Eq`: {diags:?}"
    );
}

#[test]
fn derive_eq_refuses_non_primitive_fields_with_provenance() {
    let src = "\
trait Eq {\n    fn eq(a: Self, b: Self) -> bool\n}\n\n\
struct Inner {\n    v: int,\n}\n\n\
struct Outer {\n    inner: Inner,\n}\n\n\
fn main() -> !int { 0 }\n";
    let res = resolve_one(src);
    let tc = check(&res);
    let m = module_of(&tc, "main");
    let site = wolf_span::Span::new(res.package.files[0].raw.file, 0, 0);
    let prov = Provenance {
        generator: "derive_eq".to_string(),
        call_site: site,
    };
    let tr = TraitRef {
        module: m,
        name: "Eq".to_string(),
    };
    let err = derive_eq(&tc.sigs, m, "Outer", &tr, &prov).expect_err("Outer refuses");
    assert!(err.iter().any(|d| d.code.as_str() == "E0705"));
    assert!(
        err.iter()
            .all(|d| d.secondary.iter().any(|s| s.label.contains("derive_eq"))),
        "every refusal names its generator: {err:?}"
    );
}

#[test]
fn two_arg_assert_passes_at_comptime() {
    // #9: `assert(cond, msg)` — the message rides along; the passing
    // path is unaffected.
    let src = "comptime fn ok() -> bool {\n    assert(1 + 1 == 2, \"math holds\")\n    true\n}\n\n\
               fn main() -> !int {\n    const K = ok()\n    if K { 0 } else { 1 }\n}\n";
    let res = resolve_one(src);
    let tc = typecheck_package_with(&res.package, true);
    assert!(tc.diagnostics.is_empty(), "{:?}", tc.diagnostics);
}
