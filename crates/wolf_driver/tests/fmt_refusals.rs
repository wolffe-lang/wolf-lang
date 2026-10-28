//! #506: invalid source is not "formatted", in either CLI mode.
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_parser_counterexample_refuses_with_its_parser_codes() {
    fn walk(path: &std::path::Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().is_some_and(|e| e == "lu") {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&root().join("corpus"), &mut files);
    let mut refusals = 0;
    for file in &files {
        let src = std::fs::read(file).unwrap();
        let mut sm = wolf_span::SourceMap::new();
        let parsed = wolf_parse::parse_file(sm.intern(file), &src);
        let errors: Vec<_> = parsed
            .diagnostics
            .iter()
            .filter(|d| d.severity == wolf_diag::Severity::Error)
            .collect();
        if errors.is_empty() {
            continue;
        }
        refusals += 1;
        let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
            .args(["fmt", "--check"])
            .arg(file)
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{}: {err}", file.display());
        for d in errors {
            assert!(
                err.contains(d.code.as_str()),
                "{}: missing {}: {err}",
                file.display(),
                d.code
            );
        }
    }
    assert!(refusals >= 39, "the counterexamples must not disappear");
    println!(
        "#506: {} corpus files, {refusals} parser counterexamples, every code retained",
        files.len()
    );
}

#[test]
fn parser_refusals_keep_their_codes_in_check_and_write_modes() {
    for (file, code) in [
        ("newline_leading.lu", "E0001"),
        ("if_then_missing.lu", "E0201"),
        ("when_reserved.lu", "E0008"),
    ] {
        let src = std::fs::read(root().join("corpus/grammar").join(file)).unwrap();
        let dir =
            std::env::temp_dir().join(format!("wolf-fmt-refusal-{}-{file}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(file);
        for check in [true, false] {
            std::fs::write(&path, &src).unwrap();
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_wolf"));
            cmd.arg("fmt");
            if check {
                cmd.arg("--check");
            }
            let out = cmd.arg(&path).output().unwrap();
            let err = String::from_utf8_lossy(&out.stderr);
            assert_eq!(out.status.code(), Some(1), "{file}, check={check}: {err}");
            assert!(err.contains(code), "{file}, check={check}: {err}");
            assert!(
                err.contains(file),
                "the parser's site must be present: {err}"
            );
            if check {
                assert_eq!(std::fs::read(&path).unwrap(), src);
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
