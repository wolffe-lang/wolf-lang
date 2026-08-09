//! Rendered snapshots for the s17 method-resolution and cast family:
//! ambiguity (E0803), the X1 receiver-mode law (E0804), the closed
//! cast set (E0805), out-of-scope traits (E0807), unknown methods
//! (E0403), and the `1.e5` float-exponent classic (E0004). Reviewed
//! artifacts, one per shape (D22).

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
    assert!(
        tc.not_yet.is_empty(),
        "fixtures check fully: {:?}",
        tc.not_yet
    );
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

// ---------------------------------------------------------- E0803 -----

/// Two in-scope traits provide the method: qualify.
#[test]
fn e0803_two_trait_ambiguity() {
    snap_one(
        "e0803_two_traits",
        "trait Loud {\n    fn speak(self) -> str\n}\n\n\
         trait Quiet {\n    fn speak(self) -> str\n}\n\n\
         struct Dog {\n    id: int,\n}\n\n\
         impl Loud for Dog {\n    fn speak(self) -> str {\n        \"WOOF\"\n    }\n}\n\n\
         impl Quiet for Dog {\n    fn speak(self) -> str {\n        \"woof\"\n    }\n}\n\n\
         fn main() -> !int {\n    let d = Dog { id: 1 }\n    print(d.speak())\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0804 -----

/// A `mut self` method on a bare receiver — the machine-applicable
/// insert-`(mut …)` fix-it.
#[test]
fn e0804_bare_mut_receiver() {
    snap_one(
        "e0804_bare_mut_receiver",
        "struct P {\n    x: int,\n}\n\n\
         impl P {\n    fn bump(mut self) -> int {\n        self.x\n    }\n}\n\n\
         fn main() -> !int {\n    var p = P { x: 1 }\n    p.bump()\n}\n",
    );
}

/// A mode on a `read self` method — drop it.
#[test]
fn e0804_superfluous_mode() {
    snap_one(
        "e0804_superfluous_mode",
        "struct P {\n    x: int,\n}\n\n\
         impl P {\n    fn dist(self) -> int {\n        self.x\n    }\n}\n\n\
         fn main() -> !int {\n    var p = P { x: 1 }\n    (mut p).dist()\n}\n",
    );
}

/// The wrong keyword: declared `take`, called with `mut`.
#[test]
fn e0804_wrong_mode() {
    snap_one(
        "e0804_wrong_mode",
        "struct P {\n    x: int,\n}\n\n\
         impl P {\n    fn done(take self) -> int {\n        self.x\n    }\n}\n\n\
         fn main() -> !int {\n    var p = P { x: 1 }\n    (mut p).done()\n}\n",
    );
}

// ---------------------------------------------------------- E0805 -----

/// `bool as int` — no truthiness bridge.
#[test]
fn e0805_bool_to_int() {
    snap_one(
        "e0805_bool_to_int",
        "fn main() -> !int {\n    let b = true\n    let n = b as int\n    n\n}\n",
    );
}

/// `str as int` — parsing is a function, not a cast.
#[test]
fn e0805_str_to_int() {
    snap_one(
        "e0805_str_to_int",
        "fn main() -> !int {\n    let s = \"5\"\n    let n = s as int\n    n\n}\n",
    );
}

// ---------------------------------------------------------- E0807 -----

/// The method exists, but its trait was never imported in this file.
#[test]
fn e0807_out_of_scope_trait() {
    insta::assert_snapshot!(
        "e0807_out_of_scope",
        render_types(&[
            (
                &["fmt"],
                "f.lu",
                "pub trait Show {\n    fn show(self) -> str\n}\n",
            ),
            (
                &[],
                "p.lu",
                "use fmt.Show\n\npub struct P {\n    pub x: int,\n}\n\n\
                 impl Show for P {\n    fn show(self) -> str {\n        \"p\"\n    }\n}\n",
            ),
            (
                &[],
                "main.lu",
                "fn main() -> !int {\n    let p = P { x: 1 }\n    print(p.show())\n    0\n}\n",
            ),
        ])
    );
}

// ---------------------------------------------------------- E0403 -----

/// No method anywhere — with the receiver's own offerings suggested.
#[test]
fn e0403_unknown_method_typo() {
    snap_one(
        "e0403_unknown_method",
        "struct P {\n    x: int,\n}\n\n\
         impl P {\n    fn dist(self) -> int {\n        self.x\n    }\n}\n\n\
         fn main() -> !int {\n    let p = P { x: 1 }\n    p.dost()\n}\n",
    );
}

// ---------------------------------------------------------- E0004 -----

/// `1.e5` parses as member access; the fix writes the fraction out.
#[test]
fn e0004_float_exponent_member() {
    snap_one(
        "e0004_float_exponent",
        "fn main() -> !int {\n    let x = 1.e5\n    let _ = x\n    0\n}\n",
    );
}
