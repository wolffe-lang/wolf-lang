//! Declaration-structure snapshots for EVERY corpus file (item kinds +
//! names + spans, bodies as `BlockPending`) — the s08 counterpart of the
//! s07 token-stream suite. Every file also passes the tree verifier.

use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("read corpus dir");
    for entry in entries {
        let p = entry.expect("dir entry").path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|e| e == "lu") {
            out.push(p);
        }
    }
}

#[test]
fn corpus_declaration_structure() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "corpus not found at {}", root.display());
    let mut sm = wolf_span::SourceMap::new();
    for f in &files {
        let bytes = std::fs::read(f).expect("read corpus file");
        let file = sm.intern(f);
        let parse = wolf_parse::parse_file(file, &bytes);
        wolf_ast::verify(&parse.root, &bytes)
            .unwrap_or_else(|e| panic!("verifier failed for {}: {e}", f.display()));
        assert_eq!(
            parse.root.text(&bytes),
            bytes,
            "lossless invariant violated for {}",
            f.display()
        );
        let mut dump = wolf_ast::dump_decls(&parse.root, &bytes);
        if !parse.diagnostics.is_empty() {
            dump.push_str("-- diagnostics --\n");
            for d in &parse.diagnostics {
                dump.push_str(&format!(
                    "{} [{:?}] {}..{} {}\n",
                    d.code,
                    d.severity,
                    d.span().lo,
                    d.span().hi,
                    d.message
                ));
            }
        }
        let rel = f.strip_prefix(&root).expect("under corpus root");
        let name = rel
            .with_extension("")
            .to_string_lossy()
            .replace(['/', '\\'], "__");
        insta::assert_snapshot!(name, dump);
    }
}
