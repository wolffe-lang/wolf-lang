//! Green-tree assembly: fold the flat event stream over the lexer's
//! token stream into a `wolf_ast` green tree. This crate owns the
//! `TokenKind` → `SyntaxKind` mapping — `wolf_ast` only names the kinds.

use crate::parser::Event;
use wolf_ast::{Child, GreenNode, GreenToken, SyntaxKind};
use wolf_lex::{Keyword, Punct, Token, TokenKind};
use wolf_span::{FileId, Span};

/// The lexer-token → syntax-kind mapping (string payloads collapse: the
/// payload is derivable from the source text).
pub(crate) fn syntax_kind(kind: TokenKind) -> SyntaxKind {
    match kind {
        TokenKind::Ident => SyntaxKind::Ident,
        TokenKind::Underscore => SyntaxKind::Underscore,
        TokenKind::Int => SyntaxKind::Int,
        TokenKind::Float => SyntaxKind::Float,
        TokenKind::Char => SyntaxKind::Char,
        TokenKind::Kw(k) => keyword_kind(k),
        TokenKind::Punct(p) => punct_kind(p),
        TokenKind::PoundBracket => SyntaxKind::PoundBracket,
        TokenKind::PoundBangBracket => SyntaxKind::PoundBangBracket,
        TokenKind::Term => SyntaxKind::Term,
        TokenKind::StrBegin(_) => SyntaxKind::StrBegin,
        TokenKind::StrFragment => SyntaxKind::StrFragment,
        TokenKind::InterpOpen => SyntaxKind::InterpOpen,
        TokenKind::InterpClose => SyntaxKind::InterpClose,
        TokenKind::FormatSpecBegin => SyntaxKind::FormatSpecBegin,
        TokenKind::StrEnd { .. } => SyntaxKind::StrEnd,
        TokenKind::Error => SyntaxKind::ErrorToken,
        TokenKind::Eof => SyntaxKind::Eof,
    }
}

fn keyword_kind(k: Keyword) -> SyntaxKind {
    match k {
        Keyword::As => SyntaxKind::AsKw,
        Keyword::Asm => SyntaxKind::AsmKw,
        Keyword::Assume => SyntaxKind::AssumeKw,
        Keyword::Borrow => SyntaxKind::BorrowKw,
        Keyword::Break => SyntaxKind::BreakKw,
        Keyword::Comptime => SyntaxKind::ComptimeKw,
        Keyword::Const => SyntaxKind::ConstKw,
        Keyword::Continue => SyntaxKind::ContinueKw,
        Keyword::Copy => SyntaxKind::CopyKw,
        Keyword::Defer => SyntaxKind::DeferKw,
        Keyword::Distinct => SyntaxKind::DistinctKw,
        Keyword::Dyn => SyntaxKind::DynKw,
        Keyword::Else => SyntaxKind::ElseKw,
        Keyword::Enum => SyntaxKind::EnumKw,
        Keyword::Errdefer => SyntaxKind::ErrdeferKw,
        Keyword::Export => SyntaxKind::ExportKw,
        Keyword::Extern => SyntaxKind::ExternKw,
        Keyword::False => SyntaxKind::FalseKw,
        Keyword::Fn => SyntaxKind::FnKw,
        Keyword::For => SyntaxKind::ForKw,
        Keyword::Freeze => SyntaxKind::FreezeKw,
        Keyword::Handle => SyntaxKind::HandleKw,
        Keyword::If => SyntaxKind::IfKw,
        Keyword::Impl => SyntaxKind::ImplKw,
        Keyword::Import => SyntaxKind::ImportKw,
        Keyword::In => SyntaxKind::InKw,
        Keyword::Let => SyntaxKind::LetKw,
        Keyword::Loop => SyntaxKind::LoopKw,
        Keyword::Match => SyntaxKind::MatchKw,
        Keyword::Move => SyntaxKind::MoveKw,
        Keyword::Mut => SyntaxKind::MutKw,
        Keyword::Proc => SyntaxKind::ProcKw,
        Keyword::Pub => SyntaxKind::PubKw,
        Keyword::Region => SyntaxKind::RegionKw,
        Keyword::Return => SyntaxKind::ReturnKw,
        Keyword::Scope => SyntaxKind::ScopeKw,
        Keyword::Select => SyntaxKind::SelectKw,
        Keyword::Shared => SyntaxKind::SharedKw,
        Keyword::Spawn => SyntaxKind::SpawnKw,
        Keyword::Struct => SyntaxKind::StructKw,
        Keyword::Take => SyntaxKind::TakeKw,
        Keyword::Trait => SyntaxKind::TraitKw,
        Keyword::True => SyntaxKind::TrueKw,
        Keyword::Type => SyntaxKind::TypeKw,
        Keyword::Unsafe => SyntaxKind::UnsafeKw,
        Keyword::Use => SyntaxKind::UseKw,
        Keyword::Var => SyntaxKind::VarKw,
        Keyword::Weak => SyntaxKind::WeakKw,
        Keyword::When => SyntaxKind::WhenKw,
        Keyword::While => SyntaxKind::WhileKw,
    }
}

fn punct_kind(p: Punct) -> SyntaxKind {
    match p {
        Punct::LParen => SyntaxKind::LParen,
        Punct::RParen => SyntaxKind::RParen,
        Punct::LBracket => SyntaxKind::LBracket,
        Punct::RBracket => SyntaxKind::RBracket,
        Punct::LBrace => SyntaxKind::LBrace,
        Punct::RBrace => SyntaxKind::RBrace,
        Punct::Comma => SyntaxKind::Comma,
        Punct::Dot => SyntaxKind::Dot,
        Punct::DotDot => SyntaxKind::DotDot,
        Punct::DotDotEq => SyntaxKind::DotDotEq,
        Punct::Colon => SyntaxKind::Colon,
        Punct::At => SyntaxKind::At,
        Punct::Arrow => SyntaxKind::Arrow,
        Punct::FatArrow => SyntaxKind::FatArrow,
        Punct::Eq => SyntaxKind::Eq,
        Punct::EqEq => SyntaxKind::EqEq,
        Punct::NotEq => SyntaxKind::NotEq,
        Punct::Lt => SyntaxKind::Lt,
        Punct::Gt => SyntaxKind::Gt,
        Punct::LtEq => SyntaxKind::LtEq,
        Punct::GtEq => SyntaxKind::GtEq,
        Punct::Spaceship => SyntaxKind::Spaceship,
        Punct::Plus => SyntaxKind::Plus,
        Punct::Minus => SyntaxKind::Minus,
        Punct::Star => SyntaxKind::Star,
        Punct::Slash => SyntaxKind::Slash,
        Punct::Percent => SyntaxKind::Percent,
        Punct::Amp => SyntaxKind::Amp,
        Punct::AmpAmp => SyntaxKind::AmpAmp,
        Punct::Pipe => SyntaxKind::Pipe,
        Punct::PipePipe => SyntaxKind::PipePipe,
        Punct::Caret => SyntaxKind::Caret,
        Punct::Not => SyntaxKind::Not,
        Punct::Question => SyntaxKind::Question,
        Punct::Shl => SyntaxKind::Shl,
        Punct::Shr => SyntaxKind::Shr,
        Punct::PlusEq => SyntaxKind::PlusEq,
        Punct::MinusEq => SyntaxKind::MinusEq,
        Punct::StarEq => SyntaxKind::StarEq,
        Punct::SlashEq => SyntaxKind::SlashEq,
        Punct::PercentEq => SyntaxKind::PercentEq,
        Punct::AmpEq => SyntaxKind::AmpEq,
        Punct::PipeEq => SyntaxKind::PipeEq,
        Punct::CaretEq => SyntaxKind::CaretEq,
        Punct::ShlEq => SyntaxKind::ShlEq,
        Punct::ShrEq => SyntaxKind::ShrEq,
    }
}

/// Fold events + tokens into the green tree. Consumes exactly the events
/// the parser produced; the parser guarantees every lexer token was
/// bumped exactly once (the verifier re-checks from spans).
pub(crate) fn build(mut events: Vec<Event>, tokens: &[Token], file: FileId) -> GreenNode {
    // (kind, children) for every open node; the root stays at index 0.
    let mut stack: Vec<(SyntaxKind, Vec<Child>)> = Vec::new();
    let mut token_pos = 0usize;
    // End of the last emitted token's core span — where zero-width
    // Missing markers (and empty nodes) sit.
    let mut cursor: u32 = 0;
    let mut finished: Option<GreenNode> = None;

    for i in 0..events.len() {
        match events[i] {
            Event::Start {
                kind: SyntaxKind::Tombstone,
                ..
            } => {}
            Event::Start {
                kind,
                forward_parent,
            } => {
                // Resolve the forward-parent chain: parents opened later
                // in the event stream enclose this node (precede()).
                let mut kinds = vec![kind];
                let mut fp = forward_parent;
                while let Some(j) = fp {
                    let j = j as usize;
                    if let Event::Start {
                        kind: k,
                        forward_parent: next,
                    } = &mut events[j]
                    {
                        kinds.push(*k);
                        fp = *next;
                        *k = SyntaxKind::Tombstone; // consumed here
                    } else {
                        unreachable!("forward parent is not a Start");
                    }
                }
                for k in kinds.into_iter().rev() {
                    stack.push((k, Vec::new()));
                }
            }
            Event::Finish => {
                let (kind, children) = stack.pop().expect("Finish without Start");
                let span = node_span(&children, file, cursor);
                let node = GreenNode {
                    kind,
                    span,
                    children,
                };
                match stack.last_mut() {
                    Some((_, parent)) => parent.push(Child::Node(node)),
                    None => finished = Some(node),
                }
            }
            Event::Token { kind_override } => {
                let tok = &tokens[token_pos];
                token_pos += 1;
                cursor = tok.span.hi;
                let green = GreenToken {
                    kind: kind_override.unwrap_or_else(|| syntax_kind(tok.kind)),
                    span: tok.span,
                    leading: tok.leading.iter().map(|t| t.span).collect(),
                    trailing: tok.trailing.iter().map(|t| t.span).collect(),
                };
                stack
                    .last_mut()
                    .expect("token outside any node")
                    .1
                    .push(Child::Token(green));
            }
            Event::Missing => {
                let green = GreenToken {
                    kind: SyntaxKind::Missing,
                    span: Span::new(file, cursor, cursor),
                    leading: Vec::new(),
                    trailing: Vec::new(),
                };
                stack
                    .last_mut()
                    .expect("missing marker outside any node")
                    .1
                    .push(Child::Token(green));
            }
        }
    }
    assert!(stack.is_empty(), "unbalanced Start/Finish events");
    assert_eq!(token_pos, tokens.len(), "not every token was consumed");
    finished.expect("no root node")
}

fn node_span(children: &[Child], file: FileId, cursor: u32) -> Span {
    let mut it = children.iter().map(Child::span);
    match it.next() {
        None => Span::new(file, cursor, cursor),
        Some(first) => it.fold(first, Span::join),
    }
}
