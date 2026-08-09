//! The s09 headline: every corpus file with a pass-tier expectation
//! parses with **zero diagnostics and zero error nodes**, and every
//! syntax-tier counter-example (`check: fail(E0xxx)`) fails with
//! exactly its expected code — nothing more, nothing less.
//!
//! Expectations come from the corpus directive headers (`//! check:`).
//! Codes E1000+ are post-parse tiers (sema/memory/runtime): those files
//! must *parse* cleanly here; their failure belongs to later phases.

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
/// is syntax-tier (E0001–E0999).
fn expected_parse_failure(src: &str) -> Option<String> {
    let line = src.lines().find(|l| l.starts_with("//! check:"))?;
    let rest = line.split("fail(").nth(1)?;
    let code = rest.split(')').next()?.trim();
    let num: u32 = code.strip_prefix('E')?.parse().ok()?;
    (num < 1000).then(|| code.to_string())
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
    for f in &files {
        let bytes = std::fs::read(f).expect("read corpus file");
        let src = String::from_utf8_lossy(&bytes).into_owned();
        let file = sm.intern(f);
        let parse = wolf_parse::parse_file(file, &bytes);
        wolf_ast::verify(&parse.root, &bytes)
            .unwrap_or_else(|e| panic!("verifier failed for {}: {e}", f.display()));
        match expected_parse_failure(&src) {
            Some(code) => {
                fail += 1;
                assert!(
                    !parse.diagnostics.is_empty(),
                    "{} must fail with {code} but parsed clean",
                    f.display()
                );
                for d in &parse.diagnostics {
                    assert_eq!(
                        d.code,
                        code,
                        "{} must fail with exactly {code}; got {:?}",
                        f.display(),
                        parse.diagnostics
                    );
                }
            }
            None => {
                pass += 1;
                assert!(
                    parse.diagnostics.is_empty(),
                    "{} must parse clean; got {:?}",
                    f.display(),
                    parse.diagnostics
                );
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
    // The ledger: 4 syntax-tier counter-examples exist today
    // (E0001, E0002, E0006, E0008); everything else must pass.
    assert_eq!(fail, 4, "syntax-tier fail-file count drifted");
    assert!(pass > 40, "expected the full corpus, saw {pass} pass files");
}
