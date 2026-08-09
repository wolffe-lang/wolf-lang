//! The tree verifier: the invariants that make the resilient-parsing bet
//! (D22) falsifiable. Runs in every test and in the fuzz target; the
//! parser calls it behind `debug_assertions`.

use crate::green::{Child, GreenNode, GreenToken};
use crate::kind::SyntaxKind;
use wolf_span::Span;

/// Check the three tree invariants against the source:
///
/// 1. **Coverage / order**: the concatenation of every token's leading
///    trivia + core span + trailing trivia tiles the file exactly —
///    every source byte appears exactly once, in order (which implies
///    every source token appears exactly once, in order).
/// 2. **Lossless**: tree text equals the source byte-for-byte.
/// 3. **Span nesting**: every node's span is the union of its children's
///    spans (zero-width for childless nodes), and kinds are used on the
///    right side of the token/node partition.
pub fn verify(root: &GreenNode, src: &[u8]) -> Result<(), String> {
    let mut cursor: u32 = 0;
    verify_node(root, &mut cursor)?;
    if cursor as usize != src.len() {
        return Err(format!(
            "tree covers only {cursor} of {} source bytes",
            src.len()
        ));
    }
    let text = root.text(src);
    if text != src {
        return Err("tree text differs from source".to_string());
    }
    Ok(())
}

fn expect_piece(what: &str, span: Span, cursor: &mut u32) -> Result<(), String> {
    // Zero-width pieces (Missing markers, EOF-inserted terminators, the
    // Eof token, lexer recovery points) contribute no bytes; they sit at
    // token-core positions while the cursor includes trivia, so they are
    // exempt from the tiling check.
    if span.is_empty() {
        return Ok(());
    }
    if span.lo != *cursor {
        return Err(format!(
            "{what} at {}..{} does not start at cursor {}",
            span.lo, span.hi, cursor
        ));
    }
    if span.hi < span.lo {
        return Err(format!("{what} span inverted: {}..{}", span.lo, span.hi));
    }
    *cursor = span.hi;
    Ok(())
}

fn verify_token(tok: &GreenToken, cursor: &mut u32) -> Result<(), String> {
    if !tok.kind.is_token() {
        return Err(format!("node kind {:?} used as a token", tok.kind));
    }
    for tr in &tok.leading {
        expect_piece("leading trivia", *tr, cursor)?;
    }
    expect_piece("token", tok.span, cursor)?;
    for tr in &tok.trailing {
        expect_piece("trailing trivia", *tr, cursor)?;
    }
    Ok(())
}

fn verify_node(node: &GreenNode, cursor: &mut u32) -> Result<(), String> {
    if !node.kind.is_node() {
        return Err(format!("token kind {:?} used as a node", node.kind));
    }
    if node.kind == SyntaxKind::Tombstone {
        return Err("Tombstone survived into the tree".to_string());
    }
    let mut union: Option<Span> = None;
    for child in &node.children {
        match child {
            Child::Node(n) => verify_node(n, cursor)?,
            Child::Token(t) => verify_token(t, cursor)?,
        }
        let s = child.span();
        union = Some(match union {
            None => s,
            Some(u) => u.join(s),
        });
    }
    match union {
        Some(u) => {
            if node.span != u {
                return Err(format!(
                    "{:?} span {}..{} != children union {}..{}",
                    node.kind, node.span.lo, node.span.hi, u.lo, u.hi
                ));
            }
        }
        None => {
            if !node.span.is_empty() {
                return Err(format!(
                    "childless {:?} has non-empty span {}..{}",
                    node.kind, node.span.lo, node.span.hi
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use wolf_span::{FileId, SourceMap};

    fn file() -> FileId {
        let mut sm = SourceMap::new();
        sm.intern(Path::new("t.lu"))
    }

    fn tok(f: FileId, kind: SyntaxKind, lo: u32, hi: u32) -> GreenToken {
        GreenToken {
            kind,
            span: Span::new(f, lo, hi),
            leading: Vec::new(),
            trailing: Vec::new(),
        }
    }

    #[test]
    fn accepts_a_tiling_tree() {
        let f = file();
        let src = b"ab";
        let root = GreenNode {
            kind: SyntaxKind::SourceFile,
            span: Span::new(f, 0, 2),
            children: vec![
                Child::Token(tok(f, SyntaxKind::Ident, 0, 2)),
                Child::Token(tok(f, SyntaxKind::Eof, 2, 2)),
            ],
        };
        verify(&root, src).unwrap();
    }

    #[test]
    fn rejects_gaps_and_bad_spans() {
        let f = file();
        let src = b"ab";
        // Gap: token starts at 1, not 0.
        let root = GreenNode {
            kind: SyntaxKind::SourceFile,
            span: Span::new(f, 1, 2),
            children: vec![Child::Token(tok(f, SyntaxKind::Ident, 1, 2))],
        };
        assert!(verify(&root, src).is_err());
        // Node span not the union of children.
        let root = GreenNode {
            kind: SyntaxKind::SourceFile,
            span: Span::new(f, 0, 1),
            children: vec![Child::Token(tok(f, SyntaxKind::Ident, 0, 2))],
        };
        assert!(verify(&root, src).is_err());
        // Incomplete coverage.
        let root = GreenNode {
            kind: SyntaxKind::SourceFile,
            span: Span::new(f, 0, 1),
            children: vec![Child::Token(tok(f, SyntaxKind::Ident, 0, 1))],
        };
        assert!(verify(&root, src).is_err());
    }

    #[test]
    fn rejects_kind_partition_violations() {
        let f = file();
        let src = b"a";
        let root = GreenNode {
            kind: SyntaxKind::SourceFile,
            span: Span::new(f, 0, 1),
            children: vec![Child::Token(GreenToken {
                kind: SyntaxKind::FnDecl, // node kind as token
                span: Span::new(f, 0, 1),
                leading: Vec::new(),
                trailing: Vec::new(),
            })],
        };
        assert!(verify(&root, src).is_err());
    }
}
