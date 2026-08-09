//! Expression-level tree snapshots for the s03 ambiguity corpus
//! (`corpus/grammar/*.lu`, the `[gram.amb.*]` annex): the parser's
//! answer to every pinned ambiguity, as a full tree shape — else
//! binding, bracket postfix, closure extent, intdot, struct-literal
//! restriction, terminator edge cases. Snapshots must match the spec's
//! stated answers exactly.

mod util;

use std::path::{Path, PathBuf};

#[test]
fn ambiguity_corpus_tree_shapes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/grammar");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("read corpus/grammar")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "lu"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no grammar corpus at {}", root.display());
    for f in &files {
        let src = std::fs::read_to_string(f).expect("read corpus file");
        let parse = util::parse(&src);
        let mut out = String::new();
        out.push_str("== tree ==\n");
        out.push_str(&parse.root.dump(src.as_bytes()));
        out.push_str("== diagnostics ==\n");
        out.push_str(&util::render_diags(&parse));
        let name = format!(
            "expr_tree__{}",
            f.file_stem().expect("stem").to_string_lossy()
        );
        insta::assert_snapshot!(name, out);
    }
}
