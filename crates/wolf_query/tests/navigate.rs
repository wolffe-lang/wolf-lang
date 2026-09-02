//! The navigation queries at the contract level (s133): definition,
//! references and rename over a real two-file package, answered from
//! the binding table — locals, parameters, items across files through
//! `use`, fields through `.`, enum variants in value and pattern
//! position, methods — plus rename's refusal set, by name.

use std::path::PathBuf;

use wolf_query::{QueryHost, RenameOutcome, RenamePrep, Snapshot};

struct Pkg {
    dir: PathBuf,
}

impl Pkg {
    fn new(name: &str, files: &[(&str, &str)]) -> Pkg {
        // Tests run in parallel and each owns its directory: a shared
        // name would let one test's `Drop` delete another's package
        // mid-query.
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("wolf_nav_{name}_{}_{seq}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        for (rel, text) in files {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, text).unwrap();
        }
        Pkg { dir }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.join(rel)
    }
}

impl Drop for Pkg {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

const MAIN: &str = "\
use shapes

struct Point {
    x: int,
    y: int,
}

impl Point {
    fn sum(self) -> int {
        self.x + self.y
    }
}

enum Color {
    Red,
    Rgb(int, int, int),
}

fn brightness(c: Color) -> int {
    match c {
        Red => 1,
        Rgb(r, g, b) => (r + g + b) / 3,
    }
}

fn main() -> !int {
    let p = Point { x: 1, y: 2 }
    let total = p.sum() + p.x + shapes.area(3) + brightness(Color.Rgb(1, 2, 3))
    print(\"{total}\")
    0
}
";

const GEO: &str = "\
//! member: true
/// The square's area.
pub fn area(side: int) -> int {
    side * side
}

fn double(side: int) -> int {
    area(side) + side
}
";

fn pkg() -> Pkg {
    Pkg::new("two", &[("main.lu", MAIN), ("shapes/geo.lu", GEO)])
}

/// Byte offset of the `nth` (0-based) occurrence of `needle` in `src`.
fn at(src: &str, needle: &str, nth: usize) -> u32 {
    src.match_indices(needle)
        .nth(nth)
        .expect("needle present")
        .0 as u32
}

fn snapshot() -> (Pkg, Snapshot) {
    let pkg = pkg();
    let host = QueryHost::new();
    let snapshot = host.snapshot();
    (pkg, snapshot)
}

/// Definition on a method call, a field through `.`, a variant in
/// value and in pattern position, a parameter, and a local — each to
/// its declaration's name token, with the asking token as `origin`.
#[test]
fn definition_reaches_every_declaration_kind_in_one_file() {
    let (pkg, snap) = snapshot();
    let main = pkg.path("main.lu");
    let def = |off: u32| snap.definition(&main, off).unwrap();

    // `p.sum()` → `fn sum`.
    let use_sum = at(MAIN, "p.sum()", 0) + 2;
    let d = def(use_sum).expect("method resolves");
    assert_eq!(d.path, main);
    assert_eq!(d.span.lo, at(MAIN, "fn sum", 0) + 3);
    assert_eq!((d.origin.lo, d.origin.hi), (use_sum, use_sum + 3));

    // `p.x` → the field `x` in `struct Point`.
    let use_x = at(MAIN, "p.x", 0) + 2;
    let d = def(use_x).expect("field resolves");
    assert_eq!(d.span.lo, at(MAIN, "    x: int", 0) + 4);

    // `self.y` → the field `y`.
    let d = def(at(MAIN, "self.y", 0) + 5).expect("field through self");
    assert_eq!(d.span.lo, at(MAIN, "    y: int", 0) + 4);

    // `Color.Rgb(1, 2, 3)` → the variant; `Rgb(r, g, b)` (pattern) too.
    let variant = at(MAIN, "    Rgb(int", 0) + 4;
    let d = def(at(MAIN, "Color.Rgb(1", 0) + 6).expect("variant value");
    assert_eq!(d.span.lo, variant);
    let d = def(at(MAIN, "Rgb(r, g, b)", 0)).expect("variant pattern");
    assert_eq!(d.span.lo, variant);

    // A parameter use → its binder; a pattern binder use → its binder.
    let d = def(at(MAIN, "match c", 0) + 6).expect("param");
    assert_eq!(d.span.lo, at(MAIN, "(c: Color)", 0) + 1);
    let d = def(at(MAIN, "(r + g + b)", 0) + 1).expect("pattern binder");
    assert_eq!(d.span.lo, at(MAIN, "Rgb(r, g, b)", 0) + 4);

    // A local: `{total}` inside the f-string → `let total`.
    let d = def(at(MAIN, "{total}", 0) + 1).expect("local through interpolation");
    assert_eq!(d.span.lo, at(MAIN, "let total", 0) + 4);

    // A type name in a signature → the struct.
    let d = def(at(MAIN, "Point {", 0)).expect("struct literal head");
    assert_eq!(d.span.lo, at(MAIN, "struct Point", 0) + 7);
}

/// Cross-file: `shapes.area` reaches the sibling's declaration; the
/// module name reaches the module's first file; the import line is a
/// reference too.
#[test]
fn definition_crosses_files_through_the_module_graph() {
    let (pkg, snap) = snapshot();
    let main = pkg.path("main.lu");
    let geo = pkg.path("shapes/geo.lu");

    let d = snap
        .definition(&main, at(MAIN, "shapes.area(3)", 0) + 7)
        .unwrap()
        .expect("cross-file item");
    assert_eq!(d.path, geo);
    assert_eq!(d.span.lo, at(GEO, "pub fn area", 0) + 7);

    let d = snap
        .definition(&main, at(MAIN, "shapes.area(3)", 0))
        .unwrap()
        .expect("module namespace");
    assert_eq!(d.path, geo);
    assert_eq!((d.span.lo, d.span.hi), (0, 0));

    // From inside the sibling, `area(side)` reaches the same token.
    let d = snap
        .definition(&geo, at(GEO, "area(side)", 0))
        .unwrap()
        .expect("same-module item");
    assert_eq!(d.path, geo);
    assert_eq!(d.span.lo, at(GEO, "pub fn area", 0) + 7);
}

/// Builtins, prelude names and keywords answer `None` — never an
/// error, never a guess.
#[test]
fn definition_answers_none_for_the_unnavigable() {
    let (pkg, snap) = snapshot();
    let main = pkg.path("main.lu");
    assert!(
        snap.definition(&main, at(MAIN, "print(", 0))
            .unwrap()
            .is_none()
    );
    assert!(
        snap.definition(&main, at(MAIN, "x: int", 0) + 3)
            .unwrap()
            .is_none()
    );
    assert!(
        snap.definition(&main, at(MAIN, "fn main", 0))
            .unwrap()
            .is_none()
    );
    // Whitespace: nothing there.
    assert!(
        snap.definition(&main, at(MAIN, "\n\n", 0))
            .unwrap()
            .is_none()
    );
}

/// References: every use across the package in (file, offset) order,
/// the declaration only when asked, from either end.
#[test]
fn references_are_package_wide_and_ordered() {
    let (pkg, snap) = snapshot();
    let main = pkg.path("main.lu");
    let geo = pkg.path("shapes/geo.lu");

    // From the use in main.lu, declaration excluded.
    let refs = snap
        .references(&main, at(MAIN, "shapes.area(3)", 0) + 7, false)
        .unwrap()
        .expect("resolves");
    let shape: Vec<(PathBuf, u32)> = refs.iter().map(|r| (r.path.clone(), r.span.lo)).collect();
    assert_eq!(
        shape,
        vec![
            (main.clone(), at(MAIN, "shapes.area(3)", 0) + 7),
            (geo.clone(), at(GEO, "area(side)", 0)),
        ]
    );

    // From the declaration in geo.lu, declaration included. The
    // package is the one around the ENTRY (the v0 single-entry model,
    // D32): asked from the sibling, the reachable set is the `shapes`
    // module alone — main.lu's use is not in it. Honest, and named as
    // the workspace-root residue in COMPAT.
    let refs = snap
        .references(&geo, at(GEO, "pub fn area", 0) + 7, true)
        .unwrap()
        .expect("resolves");
    let shape: Vec<(PathBuf, u32)> = refs.iter().map(|r| (r.path.clone(), r.span.lo)).collect();
    assert_eq!(
        shape,
        vec![
            (geo.clone(), at(GEO, "pub fn area", 0) + 7),
            (geo.clone(), at(GEO, "area(side)", 0)),
        ]
    );

    // A parameter: its binder and its two uses, body-scoped (the
    // other `side` in `double` is a different binding).
    let refs = snap
        .references(&geo, at(GEO, "(side: int)", 0) + 1, true)
        .unwrap()
        .expect("resolves");
    let offsets: Vec<u32> = refs.iter().map(|r| r.span.lo).collect();
    assert_eq!(
        offsets,
        vec![
            at(GEO, "(side: int)", 0) + 1,
            at(GEO, "side * side", 0),
            at(GEO, "side * side", 0) + 7,
        ]
    );

    // A field: declaration, literal init, `p.x`, `self.x`.
    let refs = snap
        .references(&main, at(MAIN, "    x: int", 0) + 4, true)
        .unwrap()
        .expect("resolves");
    let offsets: Vec<u32> = refs.iter().map(|r| r.span.lo).collect();
    assert_eq!(
        offsets,
        vec![
            at(MAIN, "    x: int", 0) + 4,
            at(MAIN, "self.x", 0) + 5,
            at(MAIN, "{ x: 1", 0) + 2,
            at(MAIN, "p.x", 0) + 2,
        ]
    );

    // Prelude names have uses but no declaration.
    let refs = snap
        .references(&main, at(MAIN, "print(", 0), true)
        .unwrap()
        .expect("prelude uses");
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].span.lo, at(MAIN, "print(", 0));
}

/// Rename: the whole edit set per file — declaration, the `use`-site
/// path segment, every use in every file — and never a partial one.
#[test]
fn rename_edits_every_file_that_names_the_symbol() {
    let (pkg, snap) = snapshot();
    let main = pkg.path("main.lu");
    let geo = pkg.path("shapes/geo.lu");

    let prep = snap
        .prepare_rename(&geo, at(GEO, "pub fn area", 0) + 7)
        .unwrap()
        .expect("a name");
    assert_eq!(
        prep,
        RenamePrep::Ok {
            span: wolf_span::Span::new(
                match &prep {
                    RenamePrep::Ok { span, .. } => span.file,
                    RenamePrep::Refused(_) => unreachable!(),
                },
                at(GEO, "pub fn area", 0) + 7,
                at(GEO, "pub fn area", 0) + 11
            ),
            name: "area".to_string(),
        }
    );

    let out = snap
        .rename(&main, at(MAIN, "shapes.area(3)", 0) + 7, "square")
        .unwrap()
        .expect("a name");
    let RenameOutcome::Edits(files) = out else {
        panic!("refused: {out:?}");
    };
    type Shape = Vec<(PathBuf, Vec<(u32, u32, String)>)>;
    let shape: Shape = files
        .iter()
        .map(|f| {
            (
                f.path.clone(),
                f.edits
                    .iter()
                    .map(|(s, t)| (s.lo, s.hi, t.clone()))
                    .collect(),
            )
        })
        .collect();
    let a = at(MAIN, "shapes.area(3)", 0) + 7;
    let d = at(GEO, "pub fn area", 0) + 7;
    let u = at(GEO, "area(side)", 0);
    assert_eq!(
        shape,
        vec![
            (main, vec![(a, a + 4, "square".to_string())]),
            (
                geo,
                vec![
                    (d, d + 4, "square".to_string()),
                    (u, u + 4, "square".to_string())
                ]
            ),
        ]
    );
}

/// The refusal set, by name: keywords (`fn`, `self`), builtin types,
/// prelude names, modules, and a new name that is not an identifier.
#[test]
fn rename_refuses_by_name() {
    let (pkg, snap) = snapshot();
    let main = pkg.path("main.lu");
    let refusal = |off: u32, new: &str| -> String {
        match snap.rename(&main, off, new).unwrap().expect("a token") {
            RenameOutcome::Refused(r) => r,
            RenameOutcome::Edits(e) => panic!("edited instead of refusing: {e:?}"),
        }
    };
    assert!(refusal(at(MAIN, "fn main", 0), "g").contains("`fn` is a keyword"));
    assert!(refusal(at(MAIN, "self.x", 0), "me").contains("`self` is a keyword"));
    assert!(refusal(at(MAIN, "x: int", 0) + 3, "num").contains("`int` is a builtin type"));
    assert!(refusal(at(MAIN, "print(", 0), "say").contains("`print` is a prelude name"));
    assert!(refusal(at(MAIN, "shapes.area", 0), "geo").contains("directory"));
    let field = at(MAIN, "    x: int", 0) + 4;
    assert!(refusal(field, "let").contains("`let` is a keyword"));
    assert!(refusal(field, "9x").contains("not an identifier"));
    assert!(refusal(field, "a b").contains("not an identifier"));
    // Nothing at all (whitespace) is `None`, not a refusal.
    assert!(
        snap.rename(&main, at(MAIN, "\n\n", 0), "z")
            .unwrap()
            .is_none()
    );
    // prepareRename refuses with the same reasons.
    assert!(matches!(
        snap.prepare_rename(&main, at(MAIN, "print(", 0)).unwrap(),
        Some(RenamePrep::Refused(r)) if r.contains("prelude")
    ));
}

/// A body that did not typecheck keeps its lexical half: items and
/// locals still navigate, members honestly do not.
#[test]
fn navigation_degrades_honestly_without_typecheck() {
    let broken = MAIN.replace("let total = p.sum()", "let total = p.sum() + \"s\"");
    let pkg = Pkg::new("broken", &[("main.lu", &broken), ("shapes/geo.lu", GEO)]);
    let host = QueryHost::new();
    let snap = host.snapshot();
    let main = pkg.path("main.lu");
    let batch = snap.diagnostics(&main).unwrap().unwrap();
    assert!(
        batch
            .diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error),
        "the edit breaks typing: {:?}",
        batch.diagnostics
    );
    // Items still resolve (the resolver's half is intact)...
    let d = snap
        .definition(&main, at(&broken, "shapes.area(3)", 0) + 7)
        .unwrap()
        .expect("item still navigates");
    assert_eq!(d.path, pkg.path("shapes/geo.lu"));
    // ...and so do locals.
    assert!(
        snap.definition(&main, at(&broken, "{total}", 0) + 1)
            .unwrap()
            .is_some()
    );
    // The field in the broken body is refused with `None`, never a
    // guess.
    assert!(
        snap.definition(&main, at(&broken, "p.x", 0) + 2)
            .unwrap()
            .is_none()
    );
}
