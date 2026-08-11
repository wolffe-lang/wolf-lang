//! The query surface at the contract level (clauses 1, 4, 5): a real
//! multi-file package in a temp dir, read through overlays.

use std::path::PathBuf;

use wolf_query::{Change, QueryHost, SymbolKind};

struct Pkg {
    dir: PathBuf,
}

impl Pkg {
    fn new(name: &str, files: &[(&str, &str)]) -> Pkg {
        let dir = std::env::temp_dir().join(format!("wolf_query_{name}_{}", std::process::id()));
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

/// Diagnostics mirror the CLI ladder: the unused-import package
/// reports exactly E0305, with its machine-applicable fix-it intact.
#[test]
fn diagnostics_carry_structured_suggestions() {
    let pkg = Pkg::new(
        "diag",
        &[
            ("main.lu", "use util\n\nfn main() -> int {\n    0\n}\n"),
            (
                "util/u.lu",
                "//! member: true\n/// One.\npub fn helper() -> int {\n    1\n}\n\
                 /// Two — a second item keeps the module-shape lints quiet.\n\
                 pub fn helper2() -> int {\n    2\n}\n",
            ),
        ],
    );
    let host = QueryHost::new();
    let snapshot = host.snapshot();
    let batch = snapshot
        .diagnostics(&pkg.path("main.lu"))
        .unwrap()
        .expect("package loads");
    assert_eq!(batch.phase, "resolve");
    assert_eq!(batch.diagnostics.len(), 1);
    let d = &batch.diagnostics[0];
    assert_eq!(d.code.as_str(), "E0305");
    assert_eq!((d.primary.span.lo, d.primary.span.hi), (4, 8));
    let sugg = &d.suggestions[0];
    assert_eq!(
        sugg.applicability,
        wolf_diag::Applicability::MachineApplicable
    );
    assert_eq!(sugg.edits.len(), 1);
    assert_eq!((sugg.edits[0].0.lo, sugg.edits[0].0.hi), (0, 9));
}

/// Overlays layer over disk; closing restores disk. (Snapshots are
/// dropped before each write — holding one across `apply_change` on
/// the same thread is the documented deadlock, by design: writers
/// block on readers.)
#[test]
fn overlay_lifecycle() {
    let pkg = Pkg::new("overlay", &[("main.lu", "fn main() -> int {\n    0\n}\n")]);
    let path = pkg.path("main.lu");
    let host = QueryHost::new();
    let count = |host: &QueryHost| {
        host.snapshot()
            .diagnostics(&path)
            .unwrap()
            .unwrap()
            .diagnostics
            .len()
    };

    assert_eq!(count(&host), 0, "disk text is clean");
    host.apply_change(Change::Open {
        path: path.clone(),
        text: b"fn main() -> int {\n    lett x = 1\n    0\n}\n".to_vec(),
    });
    assert!(count(&host) > 0, "overlay text is broken");
    host.apply_change(Change::Close { path: path.clone() });
    assert_eq!(count(&host), 0, "close restores disk");
}

/// Type-at-position answers on the checked subset and refuses
/// honestly outside it.
#[test]
fn type_at_position_honest_subset() {
    let pkg = Pkg::new(
        "hover",
        &[(
            "main.lu",
            "fn add(a: int, b: int) -> int {\n    let s = a + b\n    s\n}\n",
        )],
    );
    let path = pkg.path("main.lu");
    let host = QueryHost::new();
    let snapshot = host.snapshot();

    // `s` in `let s` (line 1 col 8 → byte 40).
    let src = std::fs::read(&path).unwrap();
    let off = |needle: &str, occurrence: usize| -> u32 {
        let text = String::from_utf8(src.clone()).unwrap();
        text.match_indices(needle).nth(occurrence).unwrap().0 as u32
    };
    let h = snapshot
        .type_at(&path, off("s = a", 0))
        .unwrap()
        .expect("local answers");
    assert_eq!(h.text, "s: int");

    // The tail expression `s`.
    let h = snapshot
        .type_at(&path, off("    s\n", 0) + 4)
        .unwrap()
        .expect("expr answers");
    assert_eq!(h.text, "int");

    // Whitespace between items: nothing, honestly.
    assert!(
        snapshot
            .type_at(&path, src.len() as u32 - 1)
            .unwrap()
            .is_none()
    );
}

/// Def-of-symbol from s12 resolution: locals, module items, and
/// import bindings.
#[test]
fn def_of_symbol_resolution() {
    let pkg = Pkg::new(
        "defs",
        &[(
            "main.lu",
            "fn two() -> int {\n    2\n}\n\nfn main() -> int {\n    let x = two()\n    x\n}\n",
        )],
    );
    let path = pkg.path("main.lu");
    let src = std::fs::read_to_string(&path).unwrap();
    let host = QueryHost::new();
    let snapshot = host.snapshot();

    // `two()` call → the `fn two` name token.
    let call = src.match_indices("two()").next().unwrap().0 as u32;
    let def = snapshot.def_of(&path, call).unwrap().expect("resolves");
    assert_eq!(def.path, path);
    assert_eq!((def.span.lo, def.span.hi), (3, 6));

    // The trailing `x` → its `let x` binding.
    let use_x = src.rfind("    x\n").unwrap() as u32 + 4;
    let def = snapshot
        .def_of(&path, use_x)
        .unwrap()
        .expect("local resolves");
    let let_x = src.find("let x").unwrap() as u32 + 4;
    assert_eq!(def.span.lo, let_x);
}

/// Document symbols come from the parse tree — they answer on broken
/// files too (D22 resilience).
#[test]
fn document_symbols_resilient() {
    let pkg = Pkg::new(
        "syms",
        &[(
            "main.lu",
            "struct P {\n    x: int\n}\n\nfn ok() -> int {\n    1\n}\n\nfn broken( {\n",
        )],
    );
    let host = QueryHost::new();
    let snapshot = host.snapshot();
    let syms = snapshot
        .document_symbols(&pkg.path("main.lu"))
        .unwrap()
        .expect("parses");
    let names: Vec<(&str, SymbolKind)> = syms.iter().map(|s| (s.name.as_str(), s.kind)).collect();
    assert!(names.contains(&("P", SymbolKind::Struct)), "{names:?}");
    assert!(names.contains(&("ok", SymbolKind::Fn)), "{names:?}");
}

/// Formatting goes through the one formatter and is idempotent.
#[test]
fn format_is_idempotent() {
    let pkg = Pkg::new(
        "fmt",
        &[(
            "main.lu",
            "fn main() -> int {\n    let  x   =  1\n    x\n}\n",
        )],
    );
    let path = pkg.path("main.lu");
    let host = QueryHost::new();
    let snapshot = host.snapshot();
    let once = snapshot.format(&path).unwrap().expect("formats");
    assert!(!once.partial);
    assert_ne!(once.text, std::fs::read(&path).unwrap());

    // Writers block on readers: drop the snapshot before writing.
    drop(snapshot);
    host.apply_change(Change::Open {
        path: path.clone(),
        text: once.text.clone(),
    });
    let snapshot = host.snapshot();
    let twice = snapshot.format(&path).unwrap().expect("formats");
    assert_eq!(twice.text, once.text, "canonical text is a fixed point");
}

/// The contract version is the s57 handshake surface.
#[test]
fn contract_version_is_one() {
    assert_eq!(wolf_query::CONTRACT_VERSION, 1);
}
