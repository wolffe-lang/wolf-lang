//! Rendered snapshots for the E05xx trait/generics family (s14) —
//! every code ships with at least one reviewed fixture (`cargo xtask
//! diag-catalog` enforces the pairing). The set doubles as the
//! golden-rule catalog: definition-site errors with add-this-bound
//! hints, call-site instantiation errors naming the unmet bound,
//! coherence rejections, ceiling rejections, and the dyn-safety
//! classes (VOICE.md reviewed).

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

/// E0501 — the golden rule at the definition, with the
/// add-this-bound machine edit.
#[test]
fn e0501_add_this_bound() {
    snap_one(
        "e0501_add_bound",
        "trait Show {\n    fn show(x: Self) -> str\n}\n\
         fn describe[T](v: T) -> str {\n    Show.show(v)\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
}

/// E0501 — an operator capability no bound can grant yet.
#[test]
fn e0501_operator_capability() {
    snap_one(
        "e0501_operator",
        "fn sum[T](a: T, b: T) -> T {\n    a + b\n}\nfn main() -> !int {\n    0\n}\n",
    );
}

/// E0502 — instantiation fails at the call site only, naming the
/// unmet bound (never a backtrace into the callee).
#[test]
fn e0502_unmet_bound_at_call_site() {
    snap_one(
        "e0502_unmet_bound",
        "trait Show {\n    fn show(x: Self) -> str\n}\n\
         struct Point {\n    x: int,\n}\n\
         impl Show for Point {\n    fn show(x: Self) -> str {\n        \"p\"\n    }\n}\n\
         struct Silent {\n    n: int,\n}\n\
         fn describe[T: Show](v: T) -> str {\n    Show.show(v)\n}\n\
         fn main() -> !int {\n    let s = Silent { n: 0 }\n    let t = describe(s)\n    0\n}\n",
    );
}

/// E0503 — a bound that is not a trait.
#[test]
fn e0503_bound_not_a_trait() {
    snap_one(
        "e0503_not_a_trait",
        "struct Post {\n    n: int,\n}\n\
         fn go[T: Post](x: T) -> int {\n    0\n}\nfn main() -> !int {\n    0\n}\n",
    );
}

/// E0504 — the simple orphan rule, with the adapter escape in the
/// note.
#[test]
fn e0504_orphan_impl() {
    insta::assert_snapshot!(
        "e0504_orphan",
        render_types(&[
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
                "pub trait Show {\n    fn show(x: Self) -> str\n}\n",
            ),
            (
                &["data"],
                "d.lu",
                "pub struct Thing {\n    pub n: int,\n}\npub fn make() -> Thing {\n    Thing { n: 0 }\n}\n",
            ),
        ])
    );
}

/// E0505 — an uncovered impl-header parameter, rejected outright.
#[test]
fn e0505_uncovered_param() {
    snap_one(
        "e0505_uncovered",
        "trait Show {\n    fn show(x: Self) -> str\n}\n\
         struct Point {\n    x: int,\n}\n\
         impl[T] Show for Point {\n    fn show(x: Self) -> str {\n        \"p\"\n    }\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
}

/// E0506 — overlap by trial unification; a blanket impl overlaps a
/// specific one exactly like a duplicate would.
#[test]
fn e0506_blanket_overlap() {
    snap_one(
        "e0506_overlap",
        "trait Show {\n    fn show(x: Self) -> str\n}\n\
         struct Point {\n    x: int,\n}\n\
         impl Show for Point {\n    fn show(x: Self) -> str {\n        \"a\"\n    }\n}\n\
         impl[T] Show for T {\n    fn show(x: Self) -> str {\n        \"b\"\n    }\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
}

/// E0507 — impl/trait conformance: a signature mismatch.
#[test]
fn e0507_conformance_mismatch() {
    snap_one(
        "e0507_mismatch",
        "trait Show {\n    fn show(x: Self) -> str\n}\n\
         struct Point {\n    x: int,\n}\n\
         impl Show for Point {\n    fn show(x: Self) -> int {\n        1\n    }\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
}

/// E0508 — dyn-unsafe: a generic method.
#[test]
fn e0508_dyn_generic_method() {
    snap_one(
        "e0508_dyn_generic",
        "trait Mapper {\n    fn map[U](self, u: U) -> int\n}\n\
         fn go(d: dyn Mapper) -> int {\n    0\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
}

/// E0509 — dyn-unsafe: an associated type escapes.
#[test]
fn e0509_dyn_assoc_escape() {
    snap_one(
        "e0509_dyn_escape",
        "trait Iterish {\n    type Item = type\n    fn head(self) -> Self.Item\n}\n\
         fn go(d: dyn Iterish) -> int {\n    0\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
}

/// E0510 — dyn-unsafe: `Self` outside receiver position.
#[test]
fn e0510_dyn_self_position() {
    snap_one(
        "e0510_dyn_self",
        "trait Merge {\n    fn join(self, other: Self) -> Self\n}\n\
         fn go(d: dyn Merge) -> int {\n    0\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
}

/// E0511 — the no-HKP ceiling.
#[test]
fn e0511_no_hkp() {
    snap_one(
        "e0511_hkp",
        "fn apply[F, T](f: F[T]) -> int {\n    0\n}\nfn main() -> !int {\n    0\n}\n",
    );
}

/// E0512 — the no-GATs ceiling.
#[test]
fn e0512_no_gats() {
    snap_one(
        "e0512_gat",
        "trait Family {\n    type Member[X] = type\n}\nfn main() -> !int {\n    0\n}\n",
    );
}

/// E0513 — cyclic associated-type bindings, rejected up front.
#[test]
fn e0513_rewrite_cycle() {
    snap_one(
        "e0513_cycle",
        "trait Pair {\n    type A = type\n    type B = type\n}\n\
         struct P {\n    n: int,\n}\n\
         impl Pair for P {\n    type A = Self.B\n    type B = Self.A\n}\n\
         fn main() -> !int {\n    0\n}\n",
    );
}
