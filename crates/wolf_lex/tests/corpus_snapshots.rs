//! Full token+trivia stream snapshots for EVERY corpus file — reviewed,
//! committed, and the first thing that moves when the lexer changes.

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
fn corpus_token_streams() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "corpus not found at {}", root.display());
    let mut sm = wolf_span::SourceMap::new();
    for f in &files {
        let bytes = std::fs::read(f).expect("read corpus file");
        let file = sm.intern(f);
        let lexed = wolf_lex::lex(file, &bytes);
        assert_eq!(
            lexed.reassemble(&bytes),
            bytes,
            "lossless invariant violated for {}",
            f.display()
        );
        let rel = f.strip_prefix(&root).expect("under corpus root");
        let name = rel
            .with_extension("")
            .to_string_lossy()
            .replace(['/', '\\'], "__");
        insta::assert_snapshot!(name, lexed.dump(&bytes));
    }
}
