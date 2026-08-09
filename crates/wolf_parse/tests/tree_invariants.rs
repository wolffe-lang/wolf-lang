//! Direct checks of the tree invariants against the lexer's own stream:
//! every lexed token appears in the tree exactly once, in order, with
//! its span — on clean and broken input alike.

mod util;

use std::path::Path;
use wolf_ast::{Child, GreenNode, GreenToken, SyntaxKind};
use wolf_span::SourceMap;

fn tree_tokens<'a>(node: &'a GreenNode, out: &mut Vec<&'a GreenToken>) {
    for child in &node.children {
        match child {
            Child::Node(n) => tree_tokens(n, out),
            Child::Token(t) => out.push(t),
        }
    }
}

fn assert_token_stream_preserved(src: &str) {
    let mut sm = SourceMap::new();
    let file = sm.intern(Path::new("t.lu"));
    let lexed = wolf_lex::lex(file, src.as_bytes());
    let parse = wolf_parse::parse_tokens(&lexed, src.as_bytes());
    let mut toks = Vec::new();
    tree_tokens(&parse.root, &mut toks);
    let real: Vec<_> = toks
        .iter()
        .filter(|t| t.kind != SyntaxKind::Missing)
        .collect();
    assert_eq!(
        real.len(),
        lexed.tokens.len(),
        "token count mismatch for {src:?}"
    );
    for (tree_tok, lex_tok) in real.iter().zip(&lexed.tokens) {
        assert_eq!(tree_tok.span, lex_tok.span, "span mismatch for {src:?}");
        assert_eq!(
            tree_tok.leading.len(),
            lex_tok.leading.len(),
            "leading trivia lost for {src:?}"
        );
        assert_eq!(
            tree_tok.trailing.len(),
            lex_tok.trailing.len(),
            "trailing trivia lost for {src:?}"
        );
    }
}

#[test]
fn every_lexed_token_exactly_once_in_order() {
    for src in [
        "fn main() -> !int {\n    print(\"hi {x}\")\n    0\n}\n",
        "use std.{fs, net}\nlet x: Map[str, int] = y\n",
        "fn broken(x: int\nfn ok() { }\n",
        "1 + 2\n#[)]\nfnn zap\n}}}\n",
        "trait T { fn f(self) -> int\nconst C = 1 }\n",
        "",
    ] {
        assert_token_stream_preserved(src);
    }
}

#[test]
fn corpus_token_streams_preserved() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read dir") {
            let p = entry.expect("entry").path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "lu") {
                let src = std::fs::read_to_string(&p).expect("read");
                assert_token_stream_preserved(&src);
                count += 1;
            }
        }
    }
    assert!(count > 50, "expected the full corpus, saw {count}");
}
