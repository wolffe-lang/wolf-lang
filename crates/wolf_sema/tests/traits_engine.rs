//! Behavioral tests for the s14 trait engine: the golden rule
//! (definition-site checking, call-site-only instantiation errors),
//! qualified calls, associated types/consts through the rewrite
//! engine, coherence, adapters, dyn-safety, and the input/output
//! discipline.

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

const SHOW_WORLD: &str = "\
trait Show {
    fn show(x: Self) -> str
}

struct Point {
    x: int,
}

impl Show for Point {
    fn show(x: Self) -> str {
        \"point {x.x}\"
    }
}

fn describe[T: Show](v: T) -> str {
    Show.show(v)
}
";

// ------------------------------------------------------ the happy set --

#[test]
fn trait_impl_and_bounded_generic_fully_check() {
    let src = format!(
        "{SHOW_WORLD}
fn main() -> !int {{
    let p = Point {{ x: 1 }}
    let s = describe(p)
    print(s)
    0
}}
"
    );
    let tc = check_one(&src);
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn qualified_call_on_concrete_type_selects_the_impl() {
    let src = format!(
        "{SHOW_WORLD}
fn main() -> !int {{
    let p = Point {{ x: 2 }}
    print(Show.show(p))
    0
}}
"
    );
    let tc = check_one(&src);
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn isolated_namespaces_allow_method_name_collisions() {
    // Two traits, same method name — qualified calls disambiguate;
    // no global method soup (Carbon-style).
    let tc = check_one(
        "trait Reader {\n    fn id(x: Self) -> int\n}\n\
         trait Writer {\n    fn id(x: Self) -> str\n}\n\
         struct Both {\n    n: int,\n}\n\
         impl Reader for Both {\n    fn id(x: Self) -> int {\n        x.n\n    }\n}\n\
         impl Writer for Both {\n    fn id(x: Self) -> str {\n        \"w\"\n    }\n}\n\
         fn main() -> !int {\n    let b = Both { n: 3 }\n    let n = Reader.id(b)\n    let s = Writer.id(b)\n    n\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn associated_types_normalize_through_the_rewrite_engine() {
    let tc = check_one(
        "trait Seq {\n    type Item = type\n    fn head(s: Self) -> Self.Item\n}\n\
         struct Counter {\n    value: int,\n}\n\
         impl Seq for Counter {\n    type Item = int\n    fn head(s: Self) -> Self.Item {\n        s.value\n    }\n}\n\
         fn front[T: Seq](s: T) -> T.Item {\n    Seq.head(s)\n}\n\
         fn main() -> !int {\n    let c = Counter { value: 7 }\n    let n: int = front(c)\n    n\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn associated_consts_reach_generic_bodies_through_bounds() {
    let tc = check_one(
        "trait Sized2 {\n    const WIDTH: int = 1\n}\n\
         struct Cell {\n    v: int,\n}\n\
         impl Sized2 for Cell {\n    const WIDTH: int = 4\n}\n\
         fn width_of[T: Sized2](x: T) -> int {\n    T.WIDTH\n}\n\
         fn main() -> !int {\n    let c = Cell { v: 0 }\n    width_of(c)\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn adapter_types_are_their_own_nominals_with_free_casts() {
    // The sanctioned orphan escape (D28): distinct adapter, own impl
    // set, free bidirectional casts to/from the base.
    let tc = check(&[
        (
            &[],
            "main.lu",
            "use media\nuse media.Show\n\n\
             type Cover = distinct media.Song\n\n\
             impl Show for Cover {\n    fn show(x: Self) -> str {\n        \"cover\"\n    }\n}\n\n\
             fn main() -> !int {\n    let s = media.make()\n    let c = s as Cover\n    print(Show.show(c))\n    let back = c as media.Song\n    0\n}\n",
        ),
        (
            &["media"],
            "m.lu",
            "/// Rendering.\npub trait Show {\n    fn show(x: Self) -> str\n}\n\n\
             /// A song.\npub struct Song {\n    pub title: str,\n}\n\n\
             /// One song.\npub fn make() -> Song {\n    Song { title: \"t\" }\n}\n",
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
fn adapter_does_not_inherit_base_impls() {
    // `Song` implements Show; the adapter must NOT (its impl set
    // starts empty) — the qualified call on the adapter is E0502.
    let tc = check_one(
        "trait Show {\n    fn show(x: Self) -> str\n}\n\
         struct Song {\n    title: str,\n}\n\
         impl Show for Song {\n    fn show(x: Self) -> str {\n        x.title\n    }\n}\n\
         type Cover = distinct Song\n\
         fn main() -> !int {\n    let s = Song { title: \"t\" }\n    let c = s as Cover\n    print(Show.show(c))\n    0\n}\n",
    );
    assert_eq!(codes(&tc), ["E0502"], "{:?}", tc.diagnostics);
}

#[test]
fn dyn_objects_satisfy_their_own_trait() {
    // Dyn-safe shape: the receiver is `self` (bare `x: Self` params
    // would be an E0510 self-position escape).
    let src = "trait Draw {\n    fn draw(self) -> str\n}\n\
         struct Dot {\n    x: int,\n}\n\
         impl Draw for Dot {\n    fn draw(self) -> str {\n        \"dot\"\n    }\n}\n\
         fn render(d: dyn Draw) -> str {\n    Draw.draw(d)\n}\n\
         fn main() -> !int {\n    0\n}\n";
    let tc = check_one(src);
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

// --------------------------------------------------- the golden rule ---

#[test]
fn unstated_capability_errors_at_the_definition() {
    let tc = check_one(
        "trait Show {\n    fn show(x: Self) -> str\n}\n\
         fn describe[T](v: T) -> str {\n    Show.show(v)\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
    assert_eq!(codes(&tc), ["E0501"], "{:?}", tc.diagnostics);
    let d = &tc.diagnostics[0];
    assert!(d.message.contains("bounds on `T`"), "{}", d.message);
    assert!(
        d.suggestions
            .iter()
            .any(|s| s.edits.iter().any(|(_, t)| t == ": Show")),
        "add-this-bound edit present: {d:?}"
    );
}

#[test]
fn arithmetic_on_archetypes_is_definition_site_e0501() {
    let tc =
        check_one("fn sum[T](a: T, b: T) -> T {\n    a + b\n}\nfn main() -> !int {\n    0\n}\n");
    assert_eq!(codes(&tc), ["E0501"], "{:?}", tc.diagnostics);
}

#[test]
fn equality_on_archetypes_is_definition_site_e0501() {
    let tc = check_one(
        "fn same[T](a: T, b: T) -> bool {\n    a == b\n}\nfn main() -> !int {\n    0\n}\n",
    );
    assert_eq!(codes(&tc), ["E0501"], "{:?}", tc.diagnostics);
}

#[test]
fn instantiation_errors_land_at_the_call_site_only() {
    // The ACCEPTANCE gate: a well-bounded generic + a bad argument ⇒
    // exactly one error, E0502, at the call site, naming the bound.
    let src = format!(
        "{SHOW_WORLD}
struct Silent {{
    n: int,
}}

fn main() -> !int {{
    let s = Silent {{ n: 0 }}
    let t = describe(s)
    0
}}
"
    );
    let tc = check_one(&src);
    assert_eq!(codes(&tc), ["E0502"], "{:?}", tc.diagnostics);
    let d = &tc.diagnostics[0];
    assert!(
        d.message.contains("`Silent` does not implement `Show`"),
        "{}",
        d.message
    );
    // The error is in main (the call), never inside `describe`.
    let main_line = src.lines().position(|l| l.starts_with("fn main")).unwrap();
    let offset: usize = src.lines().take(main_line).map(|l| l.len() + 1).sum();
    assert!(
        (d.span().lo as usize) > offset,
        "error points into main, not the generic body"
    );
}

#[test]
fn satisfaction_is_cached_and_blanket_impls_match() {
    // A blanket impl through a bound: T: Quiet gets Loud for free.
    let tc = check_one(
        "trait Quiet {\n    fn q(x: Self) -> int\n}\n\
         trait Loud {\n    fn l(x: Self) -> int\n}\n\
         impl[T: Quiet] Loud for T {\n    fn l(x: Self) -> int {\n        Quiet.q(x)\n    }\n}\n\
         struct Mouse {\n    n: int,\n}\n\
         impl Quiet for Mouse {\n    fn q(x: Self) -> int {\n        x.n\n    }\n}\n\
         fn noisy[T: Loud](x: T) -> int {\n    Loud.l(x)\n}\n\
         fn main() -> !int {\n    let m = Mouse { n: 1 }\n    noisy(m)\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn outputs_never_drive_input_inference() {
    // The impl determines Item = int, but writing `front(c)` where a
    // `str` is wanted must NOT search impls backwards from the output:
    // the error is an ordinary mismatch, not a different impl choice.
    let tc = check_one(
        "trait Seq {\n    type Item = type\n    fn head(s: Self) -> Self.Item\n}\n\
         struct Counter {\n    value: int,\n}\n\
         impl Seq for Counter {\n    type Item = int\n    fn head(s: Self) -> Self.Item {\n        s.value\n    }\n}\n\
         fn front[T: Seq](s: T) -> T.Item {\n    Seq.head(s)\n}\n\
         fn main() -> !int {\n    let c = Counter { value: 7 }\n    let s: str = front(c)\n    0\n}\n",
    );
    assert_eq!(codes(&tc), ["E0401"], "{:?}", tc.diagnostics);
}

// ---------------------------------------------------------- coherence --

#[test]
fn orphan_impls_are_e0504() {
    let tc = check(&[
        (
            &[],
            "main.lu",
            "use data\nuse fmt.Show\n\n\
             impl Show for data.Thing {\n    fn show(x: Self) -> str {\n        \"thing\"\n    }\n}\n\n\
             fn main() -> !int {\n    let t = data.make()\n    0\n}\n",
        ),
        (
            &["fmt"],
            "f.lu",
            "/// Rendering.\npub trait Show {\n    fn show(x: Self) -> str\n}\n\
             /// A width hint.\npub fn width() -> int {\n    4\n}\n",
        ),
        (
            &["data"],
            "d.lu",
            "/// A thing.\npub struct Thing {\n    pub n: int,\n}\n/// One thing.\npub fn make() -> Thing {\n    Thing { n: 0 }\n}\n",
        ),
    ]);
    assert!(codes(&tc).contains(&"E0504"), "{:?}", tc.diagnostics);
}

#[test]
fn uncovered_impl_params_are_e0505() {
    let tc = check_one(
        "trait Show {\n    fn show(x: Self) -> str\n}\n\
         struct Point {\n    x: int,\n}\n\
         impl[T] Show for Point {\n    fn show(x: Self) -> str {\n        \"p\"\n    }\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
    assert!(codes(&tc).contains(&"E0505"), "{:?}", tc.diagnostics);
}

#[test]
fn overlapping_impls_are_e0506_including_blankets() {
    let tc = check_one(
        "trait Show {\n    fn show(x: Self) -> str\n}\n\
         struct Point {\n    x: int,\n}\n\
         impl Show for Point {\n    fn show(x: Self) -> str {\n        \"a\"\n    }\n}\n\
         impl[T] Show for T {\n    fn show(x: Self) -> str {\n        \"b\"\n    }\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
    assert!(codes(&tc).contains(&"E0506"), "{:?}", tc.diagnostics);
}

#[test]
fn impl_conformance_mismatches_are_e0507() {
    let tc = check_one(
        "trait Show {\n    fn show(x: Self) -> str\n}\n\
         struct Point {\n    x: int,\n}\n\
         impl Show for Point {\n    fn show(x: Self) -> int {\n        1\n    }\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
    assert!(codes(&tc).contains(&"E0507"), "{:?}", tc.diagnostics);
}

#[test]
fn rewrite_cycles_in_impls_are_e0513() {
    let tc = check_one(
        "trait Pair {\n    type A = type\n    type B = type\n}\n\
         struct P {\n    n: int,\n}\n\
         impl Pair for P {\n    type A = Self.B\n    type B = Self.A\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
    assert!(codes(&tc).contains(&"E0513"), "{:?}", tc.diagnostics);
}

// -------------------------------------------------------- dyn safety ---

#[test]
fn dyn_safety_violations_have_their_own_codes() {
    // Generic method → E0508.
    let tc = check_one(
        "trait Mapper {\n    fn map[U](x: Self, u: U) -> int\n}\n\
         fn go(d: dyn Mapper) -> int {\n    0\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
    assert!(codes(&tc).contains(&"E0508"), "{:?}", tc.diagnostics);
    // Associated-type escape → E0509.
    let tc = check_one(
        "trait Iterish {\n    type Item = type\n    fn head(x: Self) -> Self.Item\n}\n\
         fn go(d: dyn Iterish) -> int {\n    0\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
    assert!(codes(&tc).contains(&"E0509"), "{:?}", tc.diagnostics);
    // Self outside receiver position → E0510.
    let tc = check_one(
        "trait Merge {\n    fn join(a: Self, b: Self) -> Self\n}\n\
         fn go(d: dyn Merge) -> int {\n    0\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
    assert!(codes(&tc).contains(&"E0510"), "{:?}", tc.diagnostics);
}

// ---------------------------------------------------------- ceilings ---

#[test]
fn hkp_and_gats_are_catalog_rejections() {
    let tc =
        check_one("fn apply[F, T](f: F[T]) -> int {\n    0\n}\nfn main() -> !int {\n    0\n}\n");
    assert!(codes(&tc).contains(&"E0511"), "{:?}", tc.diagnostics);
    let tc =
        check_one("trait Family {\n    type Member[X] = type\n}\nfn main() -> !int {\n    0\n}\n");
    assert!(codes(&tc).contains(&"E0512"), "{:?}", tc.diagnostics);
}

#[test]
fn non_trait_bounds_are_e0503() {
    let tc = check_one(
        "struct Post {\n    n: int,\n}\n\
         fn go[T: Post](x: T) -> int {\n    0\n}\nfn main() -> !int {\n    0\n}\n",
    );
    assert!(codes(&tc).contains(&"E0503"), "{:?}", tc.diagnostics);
}

// -------------------------------------------- impl bodies check (s14) --

#[test]
fn impl_member_bodies_check_with_self_bound() {
    // A wrong impl body is a plain E0401 inside the impl.
    let tc = check_one(
        "trait Show {\n    fn show(x: Self) -> str\n}\n\
         struct Point {\n    x: int,\n}\n\
         impl Show for Point {\n    fn show(x: Self) -> str {\n        x.x\n    }\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
    assert_eq!(codes(&tc), ["E0401"], "{:?}", tc.diagnostics);
}

#[test]
fn inherent_impl_bodies_and_self_receiver_check() {
    let tc = check_one(
        "struct Point {\n    x: int,\n}\n\
         impl Point {\n    fn double(self) -> int {\n        self.x * 2\n    }\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
    assert!(
        tc.fully_checked(),
        "{:?} / {:?}",
        tc.diagnostics,
        tc.not_yet
    );
}

#[test]
fn trait_default_bodies_check_against_archetype() {
    // s17: a default body checks once against the trait's own
    // archetype Self — this one is fine…
    let tc = check_one(
        "trait Show {\n    fn show(x: Self) -> str {\n        \"default\"\n    }\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
    assert!(tc.not_yet.is_empty(), "{:?}", tc.not_yet);
    assert!(tc.diagnostics.is_empty(), "{:?}", tc.diagnostics);
    // …and one that uses a capability the trait does not grant is the
    // golden rule firing at the definition (E0501).
    let bad = check_one(
        "trait Show {\n    fn show(x: Self) -> str {\n        \"{x + 1}\"\n    }\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
    assert!(
        bad.diagnostics
            .iter()
            .any(|d| format!("{:?}", d.code).contains("E0501")),
        "{:?}",
        bad.diagnostics
    );
}

// ------------------------------------------------- hash properties -----

#[test]
fn impl_addition_moves_hashes_generic_body_edits_do_not() {
    use wolf_sema::{build_interfaces, load_package};
    let build = |src: &str| {
        let mut ml = MemoryLoader::new("prop");
        ml.add_file(&[], "main.lu", src);
        let pkg = load_package(&mut ml, &AliasTable::default()).expect("loads");
        assert!(pkg.diagnostics.is_empty(), "{:?}", pkg.diagnostics);
        build_interfaces(&pkg)
            .pop()
            .expect("root interface present")
    };
    let base = "pub trait Show {\n    fn show(self) -> str\n}\n\
                pub struct Point {\n    pub x: int,\n}\n\
                impl Show for Point {\n    fn show(self) -> str {\n        \"a\"\n    }\n}\n\
                pub fn describe[T: Show](v: T) -> str {\n    Show.show(v)\n}\n";
    let a = build(base);
    // Generic BODY edit: hashes fixed (bodies are not interface).
    let body_edit = base.replace("    Show.show(v)\n", "    let s = Show.show(v)\n    s\n");
    let b = build(&body_edit);
    assert_eq!(
        a.export_hash, b.export_hash,
        "generic bodies are not interface"
    );
    assert_eq!(a.pkg_hash, b.pkg_hash);
    // Impl BODY edit: hashes fixed.
    let impl_body_edit = base.replace("        \"a\"\n", "        \"b\"\n");
    let c = build(&impl_body_edit);
    assert_eq!(
        a.export_hash, c.export_hash,
        "impl bodies are not interface"
    );
    assert_eq!(a.pkg_hash, c.pkg_hash);
    // Impl REMOVAL: both hashes move.
    let removed = base.replace(
        "impl Show for Point {\n    fn show(self) -> str {\n        \"a\"\n    }\n}\n",
        "",
    );
    let d = build(&removed);
    assert_ne!(
        a.export_hash, d.export_hash,
        "impl removal is an interface change"
    );
    assert_ne!(a.pkg_hash, d.pkg_hash);
    // Dyn-safety record rides in the interface.
    assert_eq!(a.dyns.len(), 1);
    assert!(a.dyns[0].dyn_safe);
    assert_eq!(a.dyns[0].methods, ["show"]);
    assert_eq!(a.impls.len(), 1);
}
