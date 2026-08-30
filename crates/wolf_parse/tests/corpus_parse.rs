//! The s09 headline: every corpus file with a pass-tier expectation
//! parses with **zero diagnostics and zero error nodes**, and every
//! syntax-tier counter-example (`check: fail(E0xxx)`) fails with
//! exactly its expected code — nothing more, nothing less.
//!
//! Expectations come from the corpus directive headers (`//! check:`).
//! Codes E0300+ are post-parse tiers (resolution, sema, memory,
//! runtime): those files must *parse* cleanly here; their failure
//! belongs to later phases.

use std::path::{Path, PathBuf};
use wolf_ast::{Child, GreenNode, SyntaxKind};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read corpus dir") {
        let p = entry.expect("dir entry").path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|e| e == "lu") {
            out.push(p);
        }
    }
}

/// The `fail(EXXXX)` code from the `//! check:` header, if the failure
/// is syntax-tier (E0001–E0299 — E03xx is resolution's family, s12).
/// One numbering quirk: E0004 (`1.e5` float-exponent member access)
/// carries a spec/01 §9 number but fires at *typecheck* — the parse
/// tree is a legal member access (c02 closeout; s17 implements it).
fn expected_parse_failure(src: &str) -> Option<String> {
    let line = src.lines().find(|l| l.starts_with("//! check:"))?;
    let rest = line.split("fail(").nth(1)?;
    let code = rest.split(')').next()?.trim();
    if code == "E0004" {
        return None;
    }
    let num: u32 = code.strip_prefix('E')?.parse().ok()?;
    (num < 300).then(|| code.to_string())
}

/// The `.lu` files directly in `dir` (no recursion — a module is one
/// directory, D32).
fn collect_flat(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read module dir") {
        let p = entry.expect("dir entry").path();
        if p.is_file() && p.extension().is_some_and(|e| e == "lu") {
            out.push(p);
        }
    }
}

fn count_error_nodes(node: &GreenNode) -> usize {
    let mut n = usize::from(node.kind == SyntaxKind::ErrorNode);
    for c in &node.children {
        if let Child::Node(child) = c {
            n += count_error_nodes(child);
        }
    }
    n
}

fn count_missing(node: &GreenNode) -> usize {
    let mut n = 0;
    for c in &node.children {
        match c {
            Child::Node(child) => n += count_missing(child),
            Child::Token(t) => n += usize::from(t.kind == SyntaxKind::Missing),
        }
    }
    n
}

#[test]
fn corpus_parse_expectations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "corpus not found at {}", root.display());
    let mut sm = wolf_span::SourceMap::new();
    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut member_fail = 0usize;
    for f in &files {
        let bytes = std::fs::read(f).expect("read corpus file");
        let src = String::from_utf8_lossy(&bytes).into_owned();
        let file = sm.intern(f);
        let parse = wolf_parse::parse_file(file, &bytes);
        wolf_ast::verify(&parse.root, &bytes)
            .unwrap_or_else(|e| panic!("verifier failed for {}: {e}", f.display()));
        match expected_parse_failure(&src) {
            Some(code) if parse.diagnostics.is_empty() => {
                // D59 (directory = module): an entry may pin a
                // parse-family code that fires in a MEMBER SIBLING
                // rather than in its own bytes — the corpus witness
                // for the once-silent unparseable sibling
                // (corpus/resolve/broken_sibling). The pin is honest
                // only if some sibling really fails with that code.
                member_fail += 1;
                let dir = f.parent().expect("corpus file has a directory");
                let mut sibling_files = Vec::new();
                collect_flat(dir, &mut sibling_files);
                let sibling_fails = sibling_files.iter().any(|sib| {
                    if sib == f {
                        return false;
                    }
                    let bytes = std::fs::read(sib).expect("read sibling");
                    let sib_file = sm.intern(sib);
                    let p = wolf_parse::parse_file(sib_file, &bytes);
                    p.diagnostics.iter().any(|d| d.code.as_str() == code)
                });
                assert!(
                    sibling_fails,
                    "{} pins {code} but neither it nor a sibling fails with it",
                    f.display()
                );
            }
            Some(code) => {
                fail += 1;
                for d in &parse.diagnostics {
                    assert_eq!(
                        d.code.as_str(),
                        code,
                        "{} must fail with exactly {code}; got {:?}",
                        f.display(),
                        parse.diagnostics
                    );
                }
            }
            None if !parse.diagnostics.is_empty() => {
                // The mirror of the member-sibling case: a bare MEMBER
                // file (no directives, D59) may be deliberately
                // unparseable when a sibling entry pins its failure —
                // the broken_sibling witness itself.
                member_fail += 1;
                let dir = f.parent().expect("corpus file has a directory");
                let mut sibling_files = Vec::new();
                collect_flat(dir, &mut sibling_files);
                let pinned = sibling_files.iter().any(|sib| {
                    if sib == f {
                        return false;
                    }
                    let src = std::fs::read_to_string(sib).unwrap_or_default();
                    expected_parse_failure(&src).is_some_and(|code| {
                        parse.diagnostics.iter().any(|d| d.code.as_str() == code)
                    })
                });
                assert!(
                    pinned,
                    "{} fails to parse and no sibling entry pins it: {:?}",
                    f.display(),
                    parse.diagnostics
                );
            }
            None => {
                pass += 1;
                assert_eq!(
                    count_error_nodes(&parse.root),
                    0,
                    "{} must parse with zero error nodes",
                    f.display()
                );
                assert_eq!(
                    count_missing(&parse.root),
                    0,
                    "{} must parse with zero missing markers",
                    f.display()
                );
            }
        }
    }
    // The ledger: 9 own-file syntax-tier counter-examples exist today
    // (E0001, E0002, E0006, E0008, E0210, s88's E0201 bare `..` —
    // wolf-lang#88 — s126's E0211 misplaced `#![…]`, and s128's two
    // D63 refusal teach-notes: E0201 one-initializer-many-names and
    // E0201 bare-tuple, both `[gram.item.let]`), plus 1
    // member-sibling case (s124's broken_sibling, D59); everything
    // else must pass.
    assert_eq!(fail, 9, "syntax-tier fail-file count drifted");
    assert_eq!(
        member_fail, 2,
        "member-sibling fail-file count drifted (the broken_sibling \
         entry + its deliberately unparseable member)"
    );
    assert!(pass > 40, "expected the full corpus, saw {pass} pass files");
}
