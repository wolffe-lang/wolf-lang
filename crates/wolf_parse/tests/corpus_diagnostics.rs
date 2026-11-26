//! Complete structured diagnostics for comparing an unchanged corpus
//! across parser changes. Set `PARSE_DIAGNOSTICS_OUT` to keep the export.

use std::fmt::Write;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read corpus directory") {
        let path = entry.expect("corpus entry").path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "lu") {
            files.push(path);
        }
    }
}

#[test]
fn unmutated_corpus_structured_diagnostics() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "the complete corpus must be present");
    let mut output = String::new();
    for path in files {
        let src = std::fs::read(&path).expect("read corpus subject");
        // Each subject has source id zero in either checkout. Only its
        // path identifies the subject; every diagnostic field is kept.
        let mut sources = wolf_span::SourceMap::new();
        let parse = wolf_parse::parse_file(sources.intern(&path), &src);
        wolf_ast::verify(&parse.root, &src).expect("complete lossless tree");
        let fields: Vec<_> = parse
            .diagnostics
            .iter()
            .map(|d| {
                // Diagnostic's manual Debug omits `subject`. Destructure
                // exhaustively so a future field cannot silently disappear.
                let wolf_diag::Diagnostic {
                    code,
                    severity,
                    message,
                    primary,
                    secondary,
                    notes,
                    suggestions,
                    row_diff,
                    subject,
                } = d;
                format!(
                    "{:?}",
                    (
                        code,
                        severity,
                        message,
                        primary,
                        secondary,
                        notes,
                        suggestions,
                        row_diff,
                        subject
                    )
                )
            })
            .collect();
        let rel = path
            .strip_prefix(&root)
            .expect("corpus-relative subject")
            .to_string_lossy()
            .replace('\\', "/");
        assert!(!rel.contains(['\t', '\n']), "one subject per export line");
        writeln!(output, "{rel}\t{fields:?}").expect("write diagnostic record");
    }
    if let Some(path) = std::env::var_os("PARSE_DIAGNOSTICS_OUT") {
        std::fs::write(path, output).expect("write complete diagnostic export");
    }
}
