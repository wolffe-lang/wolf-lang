//! The teeth (s11 Target 5): idempotence, round-trip, and stability,
//! over the full corpus and proptest-generated inputs. Case count
//! follows proptest's `PROPTEST_CASES` env var (nightly cranks it, per
//! the s01 convention).

use std::path::{Path, PathBuf};

use proptest::prelude::*;

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let p = entry.expect("entry").path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|e| e == "lu") {
            out.push(p);
        }
    }
}

fn corpus_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();
    assert!(files.len() >= 50, "corpus went missing?");
    files
}

fn norm(src: &[u8]) -> wolf_fmt::NTree {
    let mut sm = wolf_span::SourceMap::new();
    let f = sm.intern(Path::new("prop.lu"));
    let parse = wolf_parse::parse_file(f, src);
    wolf_fmt::normalize(&parse.root, src)
}

fn comments(src: &[u8]) -> Vec<Vec<u8>> {
    let mut sm = wolf_span::SourceMap::new();
    let f = sm.intern(Path::new("prop.lu"));
    wolf_fmt::comment_multiset(f, src)
}

/// The three properties for one input. `require_full` additionally
/// rejects fallback (corpus files must always fully format).
fn holds_for(src: &[u8], require_full: bool, name: &str) {
    let out = wolf_fmt::format_text(src);
    if require_full {
        assert!(!out.fell_back, "{name}: self-check fell back");
    }
    // Idempotence: fmt ∘ fmt == fmt, byte-equal.
    let again = wolf_fmt::format_text(&out.text);
    assert_eq!(again.text, out.text, "{name}: not idempotent");
    // Round-trip: equal trees after trivia erasure (modulo the
    // documented canonicalizations) and equal comment multisets.
    assert_eq!(norm(src), norm(&out.text), "{name}: round-trip tree drift");
    assert_eq!(
        comments(src),
        comments(&out.text),
        "{name}: comment multiset drift"
    );
}

#[test]
fn corpus_idempotence_and_round_trip() {
    for f in corpus_files() {
        let src = std::fs::read(&f).expect("read corpus file");
        holds_for(&src, true, &f.display().to_string());
    }
}

/// Stability: the committed corpus is formatter-canonical — formatting
/// it is the identity, byte-for-byte (`[gram.fmt.canon]`). This is the
/// gate every later sprint feels.
#[test]
fn corpus_stability_formatting_is_identity() {
    for f in corpus_files() {
        let src = std::fs::read(&f).expect("read corpus file");
        let out = wolf_fmt::format_text(&src);
        assert_eq!(
            out.text,
            src,
            "{} is not formatter-canonical — run `wolf fmt corpus`",
            f.display()
        );
    }
}

// ------------------------------------------------------ proptest side ---

/// Wolfish token soup: keywords, idents, punctuation, comments, blank
/// lines — biased toward things that lex, including broken shapes.
fn wolfish() -> impl Strategy<Value = String> {
    let atom = prop_oneof![
        Just("fn ".to_string()),
        Just("let ".to_string()),
        Just("var ".to_string()),
        Just("if ".to_string()),
        Just("else ".to_string()),
        Just("match ".to_string()),
        Just("region ".to_string()),
        Just("in ".to_string()),
        Just("freeze ".to_string()),
        Just("move ".to_string()),
        Just("return ".to_string()),
        Just("x".to_string()),
        Just("ys".to_string()),
        Just("f".to_string()),
        Just("1".to_string()),
        Just("2.5".to_string()),
        Just("\"s\"".to_string()),
        Just("\"a {x} b\"".to_string()),
        Just("(".to_string()),
        Just(")".to_string()),
        Just("[".to_string()),
        Just("]".to_string()),
        Just("{ ".to_string()),
        Just("} ".to_string()),
        Just("+ ".to_string()),
        Just("== ".to_string()),
        Just(", ".to_string()),
        Just(": ".to_string()),
        Just("; ".to_string()),
        Just("? ".to_string()),
        Just(".".to_string()),
        Just("..".to_string()),
        Just("=> ".to_string()),
        Just("= ".to_string()),
        Just("\n".to_string()),
        Just("\n\n".to_string()),
        Just("// note\n".to_string()),
        Just("/// doc\n".to_string()),
    ];
    proptest::collection::vec(atom, 0..64).prop_map(|v| v.concat())
}

/// Small well-formed programs, assembled from statement templates.
fn wolfish_program() -> impl Strategy<Value = String> {
    let stmt = prop_oneof![
        Just("    let a = 1 + 2 * 3\n".to_string()),
        Just("    var b = f(x, y)\n".to_string()),
        Just("    if c { one() } else { two() }\n".to_string()),
        Just("    for i in 0..n { sum += i }\n".to_string()),
        Just("    let r = region()\n".to_string()),
        Just("    let v = in r { build() }\n".to_string()),
        Just("    let s = \"got {x:>8}\"\n".to_string()),
        Just("    xs.push((1, 2))\n".to_string()),
        Just("    // a comment\n    let c2 = 3   // trailing\n".to_string()),
        Just("    match e {\n        A => 1,\n        B => { go() },\n    }\n".to_string()),
        Just("    defer close(mut res)\n".to_string()),
        Just("    let q = xs[^1..]\n".to_string()),
    ];
    proptest::collection::vec(stmt, 0..8)
        .prop_map(|stmts| format!("fn main() -> !int {{\n{}    0\n}}\n", stmts.concat()))
}

proptest! {
    /// Any byte soup: never panic; when the input has errors the
    /// formatter still upholds every invariant (possibly by falling
    /// back to the input, which satisfies them trivially).
    #[test]
    fn soup_upholds_invariants(src in wolfish()) {
        holds_for(src.as_bytes(), false, "soup");
    }

    /// Generated well-formed programs must fully format (no fallback)
    /// and uphold every invariant.
    #[test]
    fn programs_fully_format(src in wolfish_program()) {
        let out = wolf_fmt::format_text(src.as_bytes());
        prop_assert!(!out.partial, "template program failed to parse");
        holds_for(src.as_bytes(), true, "program");
    }
}

/// Fuzz-found idempotence regressions (2026-08-12, the nightly's fmt
/// crash + its neighbor from the same sweep): a leading comment inside
/// a binary continuation compounded one indent level per pass; a
/// trailing comment on a prefix operator made pass one and pass two
/// disagree about joining the operand. Broken inputs, so fallback is
/// allowed — the properties must hold either way.
#[test]
fn fuzz_regressions() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/regressions");
    let mut n = 0;
    for e in std::fs::read_dir(dir).expect("regressions dir") {
        let p = e.expect("entry").path();
        let src = std::fs::read(&p).expect("read regression");
        holds_for(&src, false, &p.display().to_string());
        n += 1;
    }
    assert!(n >= 2, "the regression corpus never shrinks");
}
