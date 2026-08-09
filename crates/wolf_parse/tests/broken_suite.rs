//! The broken-input suite: deliberately mangled declarations, one
//! snapshot each showing BOTH the full tree (error nodes + spans) and
//! the diagnostics — the D22 bet made falsifiable.
//!
//! Review criterion, pinned in every snapshot: each fixture's damage is
//! contained (one error, one region — diagnostics count and error-node
//! extent are in the snapshot) and every later declaration in the file
//! still parses clean.

mod util;

use std::path::{Path, PathBuf};

#[test]
fn broken_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/broken");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("read tests/broken")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "lu"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no fixtures in {}", root.display());
    for f in &files {
        let src = std::fs::read_to_string(f).expect("read fixture");
        let parse = util::parse(&src);
        let mut out = String::new();
        out.push_str("== source ==\n");
        out.push_str(&src);
        out.push_str("== tree ==\n");
        out.push_str(&parse.root.dump(src.as_bytes()));
        out.push_str("== diagnostics ==\n");
        out.push_str(&util::render_diags(&parse));
        let name = f.file_stem().expect("stem").to_string_lossy().into_owned();
        insta::assert_snapshot!(name, out);
    }
}
