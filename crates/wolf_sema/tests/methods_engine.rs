//! s17 engine tests: method resolution order, receiver modes, enum
//! variants, trait default bodies, and the closed cast set — the
//! positive halves whose negatives live in `method_diagnostics.rs`.

use wolf_sema::check::{BodyResult, Dispatch, TypedBody};
use wolf_sema::{
    AliasTable, MemoryLoader, Typecheck, resolve_package_with, typecheck_package_with,
};

fn check_files(files: &[(&[&str], &str, &str)]) -> Typecheck {
    let mut ml = MemoryLoader::new("s17");
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
    check_files(&[(&[], "main.lu", src)])
}

fn assert_clean(tc: &Typecheck) {
    assert!(tc.not_yet.is_empty(), "not-yet: {:?}", tc.not_yet);
    assert!(tc.diagnostics.is_empty(), "diags: {:?}", tc.diagnostics);
}

fn body<'a>(tc: &'a Typecheck, name: &str) -> &'a TypedBody {
    tc.bodies
        .iter()
        .find_map(|b| match (b.body.name == name, &b.result) {
            (true, BodyResult::Checked(tb)) => Some(tb),
            _ => None,
        })
        .unwrap_or_else(|| panic!("body `{name}` checked"))
}

// ------------------------------------------------ method resolution ----

#[test]
fn inherent_method_call_types_and_dispatches() {
    let tc = check_one(
        "struct P {\n    x: int,\n}\n\n\
         impl P {\n    fn dist(self) -> int {\n        self.x\n    }\n}\n\n\
         fn main() -> !int {\n    let p = P { x: 3 }\n    let d = p.dist()\n    d\n}\n",
    );
    assert_clean(&tc);
    let tb = body(&tc, "main");
    assert_eq!(tb.local_type("d").as_deref(), Some("int"));
    assert!(
        tb.dispatch
            .iter()
            .any(|(_, d)| matches!(d, Dispatch::Inherent { method, .. } if method == "dist")),
        "{:?}",
        tb.dispatch
    );
}

#[test]
fn trait_method_in_scope_dispatches() {
    let tc = check_one(
        "trait Show {\n    fn show(self) -> str\n}\n\n\
         struct P {\n    x: int,\n}\n\n\
         impl Show for P {\n    fn show(self) -> str {\n        \"p {self.x}\"\n    }\n}\n\n\
         fn main() -> !int {\n    let p = P { x: 1 }\n    print(p.show())\n    0\n}\n",
    );
    assert_clean(&tc);
    let tb = body(&tc, "main");
    assert!(
        tb.dispatch.iter().any(|(_, d)| matches!(
            d,
            Dispatch::Trait { name, method, dyn_call: false, .. }
                if name == "Show" && method == "show"
        )),
        "{:?}",
        tb.dispatch
    );
}

#[test]
fn inherent_shadows_trait_method() {
    // Resolution order is fixed: the type's own impl wins; no
    // ambiguity report.
    let tc = check_one(
        "trait Speak {\n    fn speak(self) -> str\n}\n\n\
         struct Dog {\n    id: int,\n}\n\n\
         impl Dog {\n    fn speak(self) -> str {\n        \"woof\"\n    }\n}\n\n\
         impl Speak for Dog {\n    fn speak(self) -> str {\n        \"trait woof\"\n    }\n}\n\n\
         fn main() -> !int {\n    let d = Dog { id: 1 }\n    print(d.speak())\n    0\n}\n",
    );
    assert_clean(&tc);
    let tb = body(&tc, "main");
    assert!(
        tb.dispatch
            .iter()
            .any(|(_, d)| matches!(d, Dispatch::Inherent { method, .. } if method == "speak")),
        "inherent wins: {:?}",
        tb.dispatch
    );
}

#[test]
fn moded_receivers_follow_the_declaration() {
    let tc = check_one(
        "struct P {\n    x: int,\n}\n\n\
         impl P {\n    fn bump(mut self) -> int {\n        self.x += 1\n        self.x\n    }\n\n    \
         fn done(take self) -> int {\n        self.x\n    }\n}\n\n\
         fn main() -> !int {\n    var p = P { x: 1 }\n    let a = (mut p).bump()\n    \
         let b = (take p).done()\n    a + b\n}\n",
    );
    assert_clean(&tc);
}

#[test]
fn archetype_receiver_resolves_through_bounds() {
    let tc = check_one(
        "trait Show {\n    fn show(self) -> str\n}\n\n\
         fn describe[T: Show](v: T) -> str {\n    v.show()\n}\n\n\
         struct P {\n    x: int,\n}\n\n\
         impl Show for P {\n    fn show(self) -> str {\n        \"p\"\n    }\n}\n\n\
         fn main() -> !int {\n    print(describe(P { x: 1 }))\n    0\n}\n",
    );
    assert_clean(&tc);
}

#[test]
fn dyn_receiver_dispatches_by_name() {
    // Static and dyn calls resolve identically; the body checks even
    // though nothing constructs the object here.
    let tc = check_one(
        "trait Show {\n    fn show(self) -> str\n}\n\n\
         fn render(s: dyn Show) -> str {\n    s.show()\n}\n\n\
         fn main() -> !int {\n    0\n}\n",
    );
    assert_clean(&tc);
    let tb = body(&tc, "render");
    assert!(
        tb.dispatch
            .iter()
            .any(|(_, d)| matches!(d, Dispatch::Trait { dyn_call: true, .. })),
        "{:?}",
        tb.dispatch
    );
}

#[test]
fn associated_function_calls_on_the_type() {
    let tc = check_one(
        "struct P {\n    x: int,\n}\n\n\
         impl P {\n    fn origin() -> P {\n        P { x: 0 }\n    }\n}\n\n\
         fn main() -> !int {\n    let p = P.origin()\n    p.x\n}\n",
    );
    assert_clean(&tc);
    let tb = body(&tc, "main");
    assert_eq!(tb.local_type("p").as_deref(), Some("P"));
}

#[test]
fn trait_default_body_uses_self_methods() {
    let tc = check_one(
        "trait Greeter {\n    fn name(self) -> str\n\n    \
         fn greet(self) -> str {\n        \"hi {self.name()}\"\n    }\n}\n\n\
         struct P {\n    x: int,\n}\n\n\
         impl Greeter for P {\n    fn name(self) -> str {\n        \"p\"\n    }\n}\n\n\
         fn main() -> !int {\n    let p = P { x: 1 }\n    print(p.greet())\n    0\n}\n",
    );
    assert_clean(&tc);
}

// ------------------------------------------------------ enum variants --

#[test]
fn enum_construction_and_match() {
    let tc = check_one(
        "enum Color {\n    Red,\n    Green,\n    Rgb(int, int, int),\n}\n\n\
         fn main() -> !int {\n    let c = Color.Rgb(1, 2, 3)\n    \
         let v = match c {\n        Red => 0,\n        Green => 1,\n        \
         Rgb(r, _, _) => r,\n    }\n    v\n}\n",
    );
    assert_clean(&tc);
    let tb = body(&tc, "main");
    assert_eq!(tb.local_type("c").as_deref(), Some("Color"));
    assert!(
        tb.matches.iter().any(|&(_, exhaustive)| exhaustive),
        "{:?}",
        tb.matches
    );
}

// --------------------------------------------------------- open rows ---

#[test]
fn open_row_admits_new_tags_and_rest_arm_consumes() {
    // The Roc growing-tag-union bar: the producer raises a tag its
    // open row never lists, and the rest-arm consumer compiles
    // untouched.
    let tc = check_one(
        "fn probe(n: int) -> int ! {Io(int), ..} {\n    \
         if n < 0 {\n        return Weird\n    }\n    \
         if n == 0 {\n        return Io(4)\n    }\n    n\n}\n\n\
         fn main() -> !int {\n    let v = probe(3) else |err| {\n        \
         match err {\n            Io(_) => 1,\n            _ => 0,\n        }\n    }\n    v\n}\n",
    );
    assert_clean(&tc);
}

// ----------------------------------------------------------- casts -----

#[test]
fn cast_set_recorded() {
    use wolf_sema::check::CastKind;
    let tc = check_one(
        "type Meters = distinct int\n\n\
         fn main() -> !int {\n    let a = 3 as i64\n    \
         let m = 7 as Meters\n    let back = m as int\n    let _ = a\n    back\n}\n",
    );
    assert_clean(&tc);
    let tb = body(&tc, "main");
    let kinds: Vec<CastKind> = tb.casts.iter().map(|&(_, _, _, k)| k).collect();
    assert!(kinds.contains(&CastKind::Numeric), "{kinds:?}");
    assert!(kinds.contains(&CastKind::Adapter), "{kinds:?}");
}

// ------------------------------------------------ warnings-only bodies --

#[test]
fn unreachable_arm_warns_but_body_checks() {
    let tc = check_one(
        "fn main() -> !int {\n    let b = true\n    \
         let v = match b {\n        true => 1,\n        false => 2,\n        _ => 3,\n    }\n    v\n}\n",
    );
    assert!(tc.not_yet.is_empty(), "{:?}", tc.not_yet);
    assert!(!tc.has_errors(), "{:?}", tc.diagnostics);
    let warn: Vec<_> = tc
        .diagnostics
        .iter()
        .filter(|d| d.severity == wolf_diag::Severity::Warning)
        .collect();
    assert_eq!(warn.len(), 1, "{:?}", tc.diagnostics);
    let tb = body(&tc, "main");
    assert_eq!(tb.warnings.len(), 1);
}
