//! The hand-written `.wir` fixture corpus (s24 acceptance: ≥10
//! functions incl. loops, calls, tokens): every fixture parses,
//! verifies, round-trips to a byte-identical fixpoint, and its
//! canonical dump is a reviewed snapshot (the s01 "IR dumps" family).

use std::path::PathBuf;

fn fixtures() -> Vec<(String, String)> {
    let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("fixtures dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|e| e == "wir") {
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            let src = std::fs::read_to_string(&path).expect("fixture readable");
            out.push((stem, src));
        }
    }
    out.sort();
    out
}

#[test]
fn corpus_is_big_enough() {
    let fx = fixtures();
    assert!(
        fx.len() >= 10,
        "s24 acceptance wants ≥10 fixtures, found {}",
        fx.len()
    );
    // Loops, calls, and token threading must all be represented.
    for required in ["loop_sum", "call", "tokens"] {
        assert!(fx.iter().any(|(s, _)| s == required), "missing {required}");
    }
}

#[test]
fn fixtures_parse_verify_and_reach_fixpoint() {
    for (name, src) in fixtures() {
        let m = wolf_wir::parse_module(&src)
            .unwrap_or_else(|e| panic!("{name}.wir does not parse: {e}"));
        wolf_wir::verify_module(&m).unwrap_or_else(|e| panic!("{name}.wir does not verify:\n{e}"));
        let p1 = wolf_wir::print_module(&m);
        let m2 = wolf_wir::parse_module(&p1)
            .unwrap_or_else(|e| panic!("{name}.wir canonical form does not re-parse: {e}\n{p1}"));
        let p2 = wolf_wir::print_module(&m2);
        assert_eq!(p1, p2, "{name}.wir: print→parse→print is not a fixpoint");
        wolf_wir::verify_module(&m2)
            .unwrap_or_else(|e| panic!("{name}.wir re-parsed form does not verify:\n{e}"));
    }
}

#[test]
fn fixture_dumps_are_reviewed_snapshots() {
    for (name, src) in fixtures() {
        let m = wolf_wir::parse_module(&src).expect("parses (see fixpoint test)");
        insta::assert_snapshot!(format!("dump__{name}"), wolf_wir::print_module(&m));
    }
}
