//! The homogeneous green tree: nodes are `kind + children`, children are
//! nodes or tokens, tokens carry their span plus leading/trailing trivia
//! spans. Deliberately lean — no interning, no reference counting, no
//! incremental sharing yet (the resident-compiler campaign may add them);
//! direct child iteration serves as the cursor layer.

use crate::kind::SyntaxKind;
use wolf_span::Span;

/// A leaf: one source token (or a zero-width `Missing`/`Eof` marker) with
/// its attached trivia spans. Text is always derived by slicing the
/// source — the tree owns no strings.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GreenToken {
    pub kind: SyntaxKind,
    pub span: Span,
    /// Leading trivia spans (whitespace/comments), in source order.
    pub leading: Vec<Span>,
    /// Trailing trivia spans (same-line, up to the first newline).
    pub trailing: Vec<Span>,
}

impl GreenToken {
    /// The token's text (core span only, no trivia).
    pub fn text<'a>(&self, src: &'a [u8]) -> &'a [u8] {
        &src[self.span.lo as usize..self.span.hi as usize]
    }
}

/// A child is a node or a token — the homogeneous shape.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Child {
    Node(GreenNode),
    Token(GreenToken),
}

impl Child {
    /// The child's core span (token span, or node span).
    pub fn span(&self) -> Span {
        match self {
            Child::Node(n) => n.span,
            Child::Token(t) => t.span,
        }
    }
}

/// An interior node: kind + span + children. The span is the union of the
/// children's core spans (zero-width at the node's position when all
/// children are zero-width or absent) — verifier-checked.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GreenNode {
    pub kind: SyntaxKind,
    pub span: Span,
    pub children: Vec<Child>,
}

impl GreenNode {
    /// Child nodes, in order.
    pub fn nodes(&self) -> impl Iterator<Item = &GreenNode> {
        self.children.iter().filter_map(|c| match c {
            Child::Node(n) => Some(n),
            Child::Token(_) => None,
        })
    }

    /// Child tokens, in order.
    pub fn tokens(&self) -> impl Iterator<Item = &GreenToken> {
        self.children.iter().filter_map(|c| match c {
            Child::Token(t) => Some(t),
            Child::Node(_) => None,
        })
    }

    /// First child node of `kind`.
    pub fn child_node(&self, kind: SyntaxKind) -> Option<&GreenNode> {
        self.nodes().find(|n| n.kind == kind)
    }

    /// First child token of `kind`.
    pub fn child_token(&self, kind: SyntaxKind) -> Option<&GreenToken> {
        self.tokens().find(|t| t.kind == kind)
    }

    /// Reconstruct this subtree's text: leading trivia + token + trailing
    /// trivia for every token, in order. On the root this is the lossless
    /// invariant made callable.
    pub fn text(&self, src: &[u8]) -> Vec<u8> {
        fn walk(node: &GreenNode, src: &[u8], out: &mut Vec<u8>) {
            for child in &node.children {
                match child {
                    Child::Node(n) => walk(n, src, out),
                    Child::Token(t) => {
                        let slice = |s: Span| &src[s.lo as usize..s.hi as usize];
                        for tr in &t.leading {
                            out.extend_from_slice(slice(*tr));
                        }
                        out.extend_from_slice(t.text(src));
                        for tr in &t.trailing {
                            out.extend_from_slice(slice(*tr));
                        }
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(self, src, &mut out);
        out
    }

    /// Indented full-tree dump (nodes with spans, tokens with previews) —
    /// the broken-input snapshot format.
    pub fn dump(&self, src: &[u8]) -> String {
        use std::fmt::Write;
        fn preview(src: &[u8], span: Span) -> String {
            let bytes = &src[span.lo as usize..span.hi as usize];
            let text: String = String::from_utf8_lossy(bytes).chars().take(28).collect();
            let esc: String = text.escape_debug().collect();
            let cut = bytes.len() > 28;
            format!("\"{esc}{}\"", if cut { "…" } else { "" })
        }
        fn walk(node: &GreenNode, src: &[u8], depth: usize, out: &mut String) {
            let pad = "  ".repeat(depth);
            let _ = writeln!(
                out,
                "{pad}{:?} {}..{}",
                node.kind, node.span.lo, node.span.hi
            );
            for child in &node.children {
                match child {
                    Child::Node(n) => walk(n, src, depth + 1, out),
                    Child::Token(t) => {
                        let _ = writeln!(
                            out,
                            "{}  {:?} {}..{} {}",
                            pad,
                            t.kind,
                            t.span.lo,
                            t.span.hi,
                            preview(src, t.span)
                        );
                    }
                }
            }
        }
        let mut out = String::new();
        walk(self, src, 0, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::SyntaxKind;
    use std::path::Path;
    use wolf_span::SourceMap;

    #[test]
    fn text_reassembles_trivia_and_tokens_in_order() {
        // "  fn  x " — leading/trailing trivia interleaved with tokens.
        let src = b"  fn  x ";
        let mut sm = SourceMap::new();
        let f = sm.intern(Path::new("t.lu"));
        let root = GreenNode {
            kind: SyntaxKind::SourceFile,
            span: Span::new(f, 2, 7),
            children: vec![
                Child::Token(GreenToken {
                    kind: SyntaxKind::FnKw,
                    span: Span::new(f, 2, 4),
                    leading: vec![Span::new(f, 0, 2)],
                    trailing: vec![Span::new(f, 4, 6)],
                }),
                Child::Node(GreenNode {
                    kind: SyntaxKind::ErrorNode,
                    span: Span::new(f, 6, 7),
                    children: vec![Child::Token(GreenToken {
                        kind: SyntaxKind::Ident,
                        span: Span::new(f, 6, 7),
                        leading: Vec::new(),
                        trailing: vec![Span::new(f, 7, 8)],
                    })],
                }),
            ],
        };
        assert_eq!(root.text(src), src);
        assert_eq!(root.child_token(SyntaxKind::FnKw).unwrap().text(src), b"fn");
        assert_eq!(root.nodes().count(), 1);
        assert!(root.child_node(SyntaxKind::ErrorNode).is_some());
        let dump = root.dump(src);
        assert!(dump.contains("SourceFile 2..7"));
        assert!(dump.contains("FnKw 2..4 \"fn\""));
    }
}
