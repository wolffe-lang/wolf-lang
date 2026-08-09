//! The extracted grammar (EBNF + keyword inventory, which lives in an
//! ```ebnf block) is snapshot-tested: every grammar change is a reviewed
//! diff (s03 acceptance).

#[test]
fn extracted_grammar_snapshot() {
    let md = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../spec/01-grammar.md"
    ))
    .expect("read grammar spec");
    insta::assert_snapshot!(xtask::spec::extract_ebnf(&md));
}
