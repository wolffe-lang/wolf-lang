//! The expression & statement grammar (spec/01 §3) — s09 opens the s08
//! fences. One Pratt climb over §3.2's fifteen-tier table (tightest
//! first; comparisons and ranges do not chain), blocks as expressions,
//! `else` as the defaulting operator (D30), `BracketApply` as the single
//! postfix `[…]` form (D29 — nothing to disambiguate at parse time),
//! region sugar vs value forms as distinct node kinds (X4), and the
//! concurrency/unsafe surface (§3.6–§3.8).
//!
//! Recovery ladder inside blocks (D22): an expression error syncs to
//! end-of-line first, a statement keyword second, `}` third. Expression
//! holes are `Missing` markers + error nodes; the tree stays complete
//! and lossless for any input.

use crate::codes;
use crate::grammar::{self, is_str_begin};
use crate::parser::{CompletedMarker, Marker, Parser, is_decl_keyword};
use wolf_ast::SyntaxKind;
use wolf_lex::{Keyword, Punct, TokenKind};

// ------------------------------------------------------ binding powers --

/// Tier 15: `else` defaulting (right-assoc — `r == l`).
const ELSE_BP: (u8, u8) = (2, 2);
/// Tier 14: ranges (non-assoc; chain guarded).
const RANGE_BP: (u8, u8) = (4, 5);
/// Tier 4: `as` cast (type operand — no right bp).
const AS_BP: u8 = 24;
/// Tier 3: prefix operand strength (postfix binds tighter, `as` looser).
const PREFIX_BP: u8 = 25;
/// `^n` end-relative operand: one `or_expr` (`r_end ::= '^' or_expr`).
const FROM_END_BP: u8 = 6;

/// Expression nesting limit: recursion is bounded so hostile input
/// (fuzzers) cannot overflow the stack; remaining tokens land in error
/// nodes via the statement recovery ladder.
const MAX_EXPR_DEPTH: u32 = 256;

/// Binary operators, tiers 5–13: `(left_bp, right_bp, is_comparison)`.
fn bin_op(k: TokenKind) -> Option<(u8, u8, bool)> {
    let TokenKind::Punct(p) = k else { return None };
    Some(match p {
        Punct::PipePipe => (6, 7, false),
        Punct::AmpAmp => (8, 9, false),
        Punct::EqEq
        | Punct::NotEq
        | Punct::Lt
        | Punct::Gt
        | Punct::LtEq
        | Punct::GtEq
        | Punct::Spaceship => (10, 11, true),
        Punct::Pipe => (12, 13, false),
        Punct::Caret => (14, 15, false),
        Punct::Amp => (16, 17, false),
        Punct::Shl | Punct::Shr => (18, 19, false),
        Punct::Plus | Punct::Minus => (20, 21, false),
        Punct::Star | Punct::Slash | Punct::Percent => (22, 23, false),
        _ => return None,
    })
}

fn is_assign_op(k: TokenKind) -> bool {
    matches!(
        k,
        TokenKind::Punct(
            Punct::Eq
                | Punct::PlusEq
                | Punct::MinusEq
                | Punct::StarEq
                | Punct::SlashEq
                | Punct::PercentEq
                | Punct::AmpEq
                | Punct::PipeEq
                | Punct::CaretEq
                | Punct::ShlEq
                | Punct::ShrEq
        )
    )
}

/// Can an expression start at the current token? (FIRST(expr) — used
/// for optional operands and salvage decisions.) A *named* `fn` is a
/// declaration, not a closure, so it does not count.
fn at_expr_start(p: &Parser<'_>) -> bool {
    if p.at_kw(Keyword::Fn) {
        return p.nth(1) == TokenKind::Punct(Punct::LParen);
    }
    can_start_expr(p.current())
}

/// Can `k` begin an expression? (FIRST(expr) — used for optional
/// operands: range ends, `return`/`break` values.)
fn can_start_expr(k: TokenKind) -> bool {
    match k {
        TokenKind::Ident
        | TokenKind::Int
        | TokenKind::Float
        | TokenKind::Char
        | TokenKind::StrBegin(_) => true,
        TokenKind::Punct(
            Punct::LParen
            | Punct::LBrace
            | Punct::Not
            | Punct::Minus
            | Punct::Amp
            | Punct::Star
            | Punct::Caret
            | Punct::DotDot
            | Punct::DotDotEq,
        ) => true,
        TokenKind::Kw(k) => matches!(
            k,
            Keyword::True
                | Keyword::False
                | Keyword::If
                | Keyword::Match
                | Keyword::For
                | Keyword::While
                | Keyword::Loop
                | Keyword::Fn
                | Keyword::Region
                | Keyword::In
                | Keyword::Scope
                | Keyword::Select
                | Keyword::When
                | Keyword::Spawn
                | Keyword::Unsafe
                | Keyword::Asm
                | Keyword::Borrow
                | Keyword::Move
                | Keyword::Copy
                | Keyword::Shared
                | Keyword::Freeze
                | Keyword::Return
                | Keyword::Break
                | Keyword::Continue
        ),
        _ => false,
    }
}

/// Statement keywords — the block-level sync set (recovery ladder
/// step 2; step 1 is end-of-line, step 3 is `}`).
fn is_stmt_keyword(k: Keyword) -> bool {
    matches!(
        k,
        Keyword::For
            | Keyword::While
            | Keyword::Loop
            | Keyword::Match
            | Keyword::Defer
            | Keyword::Errdefer
            | Keyword::When
            | Keyword::Return
            | Keyword::Region
            | Keyword::If
            | Keyword::Scope
            | Keyword::Select
            | Keyword::Unsafe
            | Keyword::Break
            | Keyword::Continue
    )
}

/// A token that cannot begin a statement but continues an expression —
/// the leading-operator continuation error (E0001, `[gram.amb.newline]`).
fn is_leading_operator(k: TokenKind) -> bool {
    if is_assign_op(k) {
        return true;
    }
    matches!(
        k,
        TokenKind::Punct(
            Punct::Plus
                | Punct::Slash
                | Punct::Percent
                | Punct::Caret
                | Punct::AmpAmp
                | Punct::PipePipe
                | Punct::Pipe
                | Punct::EqEq
                | Punct::NotEq
                | Punct::Lt
                | Punct::Gt
                | Punct::LtEq
                | Punct::GtEq
                | Punct::Spaceship
                | Punct::Shl
                | Punct::Shr
                | Punct::Dot
                | Punct::Question,
        ) | TokenKind::Kw(Keyword::As)
    )
}

// -------------------------------------------------------------- context --

/// Expression-context restrictions, threaded through the climb.
#[derive(Clone, Copy, Default)]
struct Ctx {
    /// Condition/scrutinee position (`[gram.amb.structlit]`): a `{`
    /// after a path begins the block, never a struct literal.
    no_struct_lit: bool,
    /// Parsing the head expression of a statement: stop before an
    /// assignment operator so the statement wraps `AssignStmt`.
    stmt_head: bool,
    /// How many string literals enclose this position (E0007 fires at 9;
    /// the lexer's hard rail at 32 is E0108).
    str_depth: u8,
}

impl Ctx {
    /// A fresh nested context (inside delimiters restrictions reset;
    /// the string depth persists).
    fn inner(self) -> Ctx {
        Ctx {
            no_struct_lit: false,
            stmt_head: false,
            str_depth: self.str_depth,
        }
    }

    /// Condition/scrutinee position.
    fn condition(self) -> Ctx {
        Ctx {
            no_struct_lit: true,
            stmt_head: false,
            str_depth: self.str_depth,
        }
    }
}

// ------------------------------------------------------- entry points --

/// A required expression in a neutral context (declaration
/// initializers). Diagnoses + `Missing` when none can start here.
pub(crate) fn expr_required(p: &mut Parser<'_>) {
    expr_required_ctx(p, Ctx::default());
}

fn expr_required_ctx(p: &mut Parser<'_>, ctx: Ctx) {
    if expr_bp(p, 0, ctx).is_none() {
        p.error(codes::EXPECTED_TOKEN, p.here(), "expected an expression");
        p.missing();
    }
}

/// Consume statement terminators at a statement boundary. An explicit
/// `;` with no statement before it on the same line — right after a
/// block opener or another terminator — terminates nothing: the empty
/// statement, E0002 (`[gram.lex.newline]`). A `;` directly after a
/// construct (`fn a() { };`) is the interchangeable terminator.
pub(crate) fn eat_terms_checked(p: &mut Parser<'_>) {
    while p.at(TokenKind::Term) {
        let after_nothing = matches!(
            p.prev_kind(),
            None | Some(TokenKind::Term | TokenKind::Punct(Punct::LBrace))
        );
        if p.current_text() == b";" && after_nothing {
            let span = p.current_span();
            p.push_diag(
                wolf_diag::Diagnostic::error(
                    codes::EMPTY_STATEMENT,
                    span,
                    "this `;` terminates nothing",
                )
                .with_label("no statement comes before it")
                .with_note(
                    "in wolf, `;` and the line end are the same terminator, and a \
                     terminator must end a statement. Remove the `;` — it is only \
                     needed between statements sharing one line.",
                )
                .with_suggestion(wolf_diag::Suggestion::new(
                    "remove the `;`",
                    vec![(span, String::new())],
                    wolf_diag::Applicability::MachineApplicable,
                )),
            );
            let e = p.start();
            p.bump();
            e.complete(p, SyntaxKind::ErrorNode);
        } else {
            p.bump();
        }
    }
}

// --------------------------------------------------------------- blocks --

/// `block ::= '{' stmt* expr? '}'` — the caller must be at `{`.
/// Statement vs trailing expression is decided by the terminator: the
/// final `ExprStmt` of a block with no `Term` child is the block's
/// value (`[gram.lex.newline]`).
pub(crate) fn block(p: &mut Parser<'_>) -> CompletedMarker {
    debug_assert!(p.at_punct(Punct::LBrace), "block off `{{`");
    let m = p.start();
    let opener = p.current_span();
    p.bump(); // `{`
    loop {
        eat_terms_checked(p);
        if p.at_punct(Punct::RBrace) {
            p.bump();
            break;
        }
        if p.at_eof() {
            grammar::unclosed(p, opener, "{");
            break;
        }
        let before = p.pos();
        stmt(p);
        if p.pos() == before {
            // The statement parser always progresses on non-Eof input;
            // this guard turns a violation into a visible stall-breaker.
            let e = p.start();
            p.bump();
            e.complete(p, SyntaxKind::ErrorNode);
        }
    }
    m.complete(p, SyntaxKind::Block)
}

fn block_required(p: &mut Parser<'_>) {
    if p.at_punct(Punct::LBrace) {
        block(p);
    } else {
        // Folded when the construct's condition already reported.
        p.error_unless_folded(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `{` to open the block",
        );
        p.missing();
        // One report for the truncated construct; fold the follow-up
        // missing-terminator diagnostic.
        p.fold_line_end();
    }
}

// ---------------------------------------------------------- statements --

/// One statement: `attribute* stmt_base` (`[gram.expr.block]`). Nested
/// item declarations re-enter the s08 item grammar.
fn stmt(p: &mut Parser<'_>) {
    let m = p.start();
    let mut prefixed = false;
    while p.at(TokenKind::PoundBracket) {
        grammar::attribute(p);
        prefixed = true;
    }
    if p.at_kw(Keyword::Pub) {
        grammar::visibility(p);
        prefixed = true;
    }
    match p.current() {
        TokenKind::Kw(Keyword::Let) => {
            grammar::binding_item(p, m, Keyword::Let, SyntaxKind::LetDecl)
        }
        TokenKind::Kw(Keyword::Var) => {
            grammar::binding_item(p, m, Keyword::Var, SyntaxKind::VarDecl)
        }
        TokenKind::Kw(Keyword::Const) => {
            grammar::binding_item(p, m, Keyword::Const, SyntaxKind::ConstDecl)
        }
        // Bounded lookahead: `fn (` is a closure expression; `fn name`
        // is a nested function item. One token decides.
        TokenKind::Kw(Keyword::Fn) if p.nth(1) != TokenKind::Punct(Punct::LParen) => {
            grammar::fn_item(p, m)
        }
        TokenKind::Kw(Keyword::Comptime | Keyword::Extern | Keyword::Export) => {
            grammar::fn_item(p, m)
        }
        TokenKind::Kw(Keyword::Type) => grammar::type_item(p, m),
        TokenKind::Kw(Keyword::Struct) => grammar::struct_item(p, m),
        TokenKind::Kw(Keyword::Enum) => grammar::enum_item(p, m),
        TokenKind::Kw(Keyword::Trait) => grammar::trait_item(p, m),
        TokenKind::Kw(Keyword::Impl) => grammar::impl_item(p, m),
        TokenKind::Kw(Keyword::Use) => grammar::use_item(p, m),
        TokenKind::Kw(Keyword::Import) => grammar::import_c_item(p, m),
        TokenKind::Kw(Keyword::Defer | Keyword::Errdefer) => defer_stmt(p, m),
        TokenKind::Kw(Keyword::Assume) => assume_stmt(p, m),
        // A `#![…]` in statement position: the file-wide form belongs at
        // the top of the file; the scoped spelling here is `#[…]`.
        // Parsed for recovery, refused by position (E0211).
        TokenKind::PoundBangBracket => {
            grammar::inner_attribute(p, true);
            m.complete(p, SyntaxKind::ErrorNode);
        }
        TokenKind::Kw(Keyword::Else) => {
            // E0005: the inserted terminator after `}` orphaned this
            // `else` ([gram.amb.else] — `} else` must share the line).
            p.error(
                codes::ELSE_ON_NEW_LINE,
                p.current_span(),
                "`else` must be on the same line as the `}` that closes the \
                 preceding block — move it up to `} else`",
            );
            p.skip_until(true, |k| k == TokenKind::Term);
            m.complete(p, SyntaxKind::ErrorNode);
        }
        k if is_leading_operator(k) => {
            let op = String::from_utf8_lossy(p.current_text()).into_owned();
            p.error(
                codes::LEADING_OPERATOR,
                p.current_span(),
                format!(
                    "this line starts with `{op}`, but a statement cannot begin \
                     with an operator; wolf continuations are trailing — move \
                     `{op}` to the end of the previous line"
                ),
            );
            p.skip_until(true, |k| k == TokenKind::Term);
            m.complete(p, SyntaxKind::ErrorNode);
        }
        _ => expr_stmt(p, m, prefixed),
    }
}

/// An expression statement, or an assignment statement when an
/// `assign_op` follows the head expression (`[gram.expr.assign]` —
/// assignment is a statement, never an expression).
fn expr_stmt(p: &mut Parser<'_>, m: Marker, prefixed: bool) {
    let ctx = Ctx {
        stmt_head: true,
        ..Ctx::default()
    };
    if expr_bp(p, 0, ctx).is_none() {
        let msg = if prefixed {
            "expected a statement after the attributes"
        } else {
            "expected a statement"
        };
        // Folded when a truncated construct was just reported here.
        p.error_unless_folded(codes::EXPECTED_TOKEN, p.current_span(), msg);
        // Recovery ladder: end of line, then a statement keyword, `}`.
        p.skip_until(true, |k| {
            k == TokenKind::Term || matches!(k, TokenKind::Kw(kw) if is_stmt_keyword(kw))
        });
        if p.at(TokenKind::Term) {
            p.bump();
        }
        m.complete(p, SyntaxKind::ErrorNode);
        return;
    }
    if is_assign_op(p.current()) {
        p.bump(); // the assignment operator
        expr_required_ctx(p, Ctx::default());
        stmt_terminator(p);
        m.complete(p, SyntaxKind::AssignStmt);
        return;
    }
    stmt_terminator(p);
    m.complete(p, SyntaxKind::ExprStmt);
}

/// The `Term` that ends a statement. Absent before `}` / EOF — that is
/// the trailing-expression position (or Go's omitted terminator).
fn stmt_terminator(p: &mut Parser<'_>) {
    if p.at(TokenKind::Term) {
        p.bump();
    } else if !p.at_punct(Punct::RBrace) && !p.at_eof() {
        p.expected_line_end("expression");
        p.recover_until(true, |k| {
            k == TokenKind::Term || matches!(k, TokenKind::Kw(kw) if is_stmt_keyword(kw))
        });
        if p.at(TokenKind::Term) {
            p.bump();
        }
    }
}

/// `('defer' | 'errdefer') expr TERM`.
fn defer_stmt(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // defer / errdefer
    expr_required(p);
    stmt_terminator(p);
    m.complete(p, SyntaxKind::DeferStmt);
}

/// `'assume' 'noalias' expr (',' expr)+ TERM` (`[gram.expr.unsafe]`).
fn assume_stmt(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // assume
    if p.at(TokenKind::Ident) && p.current_text() == b"noalias" {
        p.bump();
    } else {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `noalias` after `assume`",
        );
        p.missing();
    }
    expr_required(p);
    let mut count = 1usize;
    while p.at_punct(Punct::Comma) {
        p.bump();
        expr_required(p);
        count += 1;
    }
    if count < 2 {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "`assume noalias` takes at least two places (one place aliases \
             nothing)",
        );
    }
    stmt_terminator(p);
    m.complete(p, SyntaxKind::AssumeStmt);
}

// -------------------------------------------------------- the climb ----

/// The precedence climb over §3.2's table. Returns `None` (emitting
/// nothing) when no expression can start here.
fn expr_bp(p: &mut Parser<'_>, min_bp: u8, ctx: Ctx) -> Option<CompletedMarker> {
    if p.depth >= MAX_EXPR_DEPTH {
        // Bail before the stack does; the caller's recovery consumes
        // the tokens losslessly.
        return None;
    }
    p.depth += 1;
    let result = expr_bp_inner(p, min_bp, ctx);
    p.depth -= 1;
    result
}

fn expr_bp_inner(p: &mut Parser<'_>, min_bp: u8, ctx: Ctx) -> Option<CompletedMarker> {
    let (mut lhs, mut path_shaped) = primary(p, ctx)?;
    // Comparisons are non-associative and do not chain (E0003): track
    // whether the current lhs is itself a comparison application.
    let mut lhs_is_cmp = false;
    let mut cmp_reported = false;
    loop {
        match p.current() {
            // ---------------------------------- postfix tier (2): always
            TokenKind::Punct(Punct::LParen) => {
                let m = lhs.precede(p);
                arg_list(p, ctx);
                lhs = m.complete(p, SyntaxKind::CallExpr);
                path_shaped = false;
            }
            TokenKind::Punct(Punct::LBracket) => {
                // BracketApply: indexing AND generic application — one
                // node kind, resolved in sema ([gram.amb.brackets]).
                // A line-initial `[` never reaches here: the inserted
                // terminator broke the climb ([gram.lex.newline]).
                let m = lhs.precede(p);
                arg_list(p, ctx);
                lhs = m.complete(p, SyntaxKind::BracketApply);
                path_shaped = false;
            }
            TokenKind::Punct(Punct::Dot) => {
                let m = lhs.precede(p);
                let dot_hi = p.current_span().hi;
                p.bump(); // `.`
                // member ::= IDENT | INT | reserved_kw — member position
                // is keyword-transparent (`.take(n)`, `s.spawn(…)`).
                let is_ident = p.at(TokenKind::Ident);
                // ...but not across a line break, and not for a keyword
                // that only ever opens a top-level item. A `.` at end of
                // line suppresses terminator insertion so `x\n.f()`
                // chains, and a damaged `}` therefore let the `.` reach
                // over a blank line and swallow the `fn` beginning the
                // NEXT declaration — the rest of the file became one
                // member access. Same-line keyword members still parse.
                let steals_item =
                    p.at_toplevel_decl_start() && p.crosses_line(dot_hi, p.current_span().lo);
                match p.current() {
                    _ if steals_item => {
                        p.error(
                            codes::EXPECTED_TOKEN,
                            p.here(),
                            "expected a member name after `.`",
                        );
                        p.missing();
                    }
                    TokenKind::Ident | TokenKind::Int | TokenKind::Kw(_) => p.bump(),
                    _ => {
                        p.error(
                            codes::EXPECTED_TOKEN,
                            p.here(),
                            "expected a member name after `.`",
                        );
                        p.missing();
                    }
                }
                lhs = m.complete(p, SyntaxKind::MemberExpr);
                path_shaped = path_shaped && is_ident;
            }
            TokenKind::Punct(Punct::Question) => {
                let m = lhs.precede(p);
                p.bump();
                lhs = m.complete(p, SyntaxKind::TryExpr);
                path_shaped = false;
            }
            TokenKind::Punct(Punct::LBrace) if path_shaped => {
                // struct_lit ::= path '{' … — legal only outside
                // condition/scrutinee position ([gram.amb.structlit]).
                if ctx.no_struct_lit {
                    if !at_struct_lit_fields(p) {
                        break; // the `{` opens the block — not ours
                    }
                    p.error(
                        codes::STRUCT_LIT_IN_COND,
                        p.current_span(),
                        "struct literals are not allowed in condition or \
                         scrutinee position — wrap the literal in \
                         parentheses: `(Path { … })`",
                    );
                }
                lhs = struct_lit(p, lhs, ctx);
                path_shaped = false;
            }
            // ------------------------------------------- `as` (tier 4)
            TokenKind::Kw(Keyword::As) if AS_BP >= min_bp => {
                let m = lhs.precede(p);
                p.bump();
                grammar::type_required(p);
                lhs = m.complete(p, SyntaxKind::CastExpr);
                path_shaped = false;
            }
            // ---------------------------------------- ranges (tier 14)
            TokenKind::Punct(Punct::DotDot | Punct::DotDotEq) if RANGE_BP.0 >= min_bp => {
                if lhs.kind() == SyntaxKind::RangeExpr {
                    // Ranges are non-associative; leave the second `..`
                    // for the container to diagnose.
                    break;
                }
                let m = lhs.precede(p);
                p.bump();
                maybe_range_end(p, ctx);
                lhs = m.complete(p, SyntaxKind::RangeExpr);
                path_shaped = false;
            }
            // ------------------------------- `else` defaulting (tier 15)
            TokenKind::Kw(Keyword::Else) if ELSE_BP.0 >= min_bp => {
                let m = lhs.precede(p);
                p.bump(); // else
                else_body(p, ctx);
                lhs = m.complete(p, SyntaxKind::ElseExpr);
                path_shaped = false;
            }
            // --------------------- assignment is a statement, not this
            k if is_assign_op(k) => {
                if ctx.stmt_head || min_bp > 0 {
                    break;
                }
                if !p.assign_error_reported {
                    p.assign_error_reported = true;
                    p.push_diag(
                        wolf_diag::Diagnostic::error(
                            codes::ASSIGN_IN_EXPR,
                            p.current_span(),
                            "assignment is a statement, so `=` produces no value here",
                        )
                        .with_label("this `=` cannot be part of an expression")
                        .with_note(
                            "if you meant to compare, write `==`. If you meant to \
                             assign, do it on its own line first ([gram.expr.assign]).",
                        ),
                    );
                }
                let m = lhs.precede(p);
                p.bump();
                if expr_bp(p, 0, ctx).is_none() {
                    p.error(codes::EXPECTED_TOKEN, p.here(), "expected an expression");
                    p.missing();
                }
                lhs = m.complete(p, SyntaxKind::BinExpr);
                path_shaped = false;
            }
            // ------------------------------------ binary tiers 5–13
            k => {
                let Some((l_bp, r_bp, is_cmp)) = bin_op(k) else {
                    break;
                };
                if l_bp < min_bp {
                    break;
                }
                if is_cmp && lhs_is_cmp && !cmp_reported {
                    p.error(
                        codes::COMPARISON_CHAIN,
                        p.current_span(),
                        "comparison operators do not chain — did you mean \
                         `a < b && b < c`?",
                    );
                    cmp_reported = true;
                }
                let m = lhs.precede(p);
                p.bump(); // the operator
                if expr_bp(p, r_bp, ctx).is_none() {
                    p.error(codes::EXPECTED_TOKEN, p.here(), "expected an expression");
                    p.missing();
                }
                lhs = m.complete(p, SyntaxKind::BinExpr);
                lhs_is_cmp = is_cmp;
                path_shaped = false;
                continue;
            }
        }
        lhs_is_cmp = false;
    }
    Some(lhs)
}

/// A `{` in condition position after a path: peek whether it opens a
/// struct-literal field list (`IDENT ':'`) — the E0006 shape — or a
/// block. Bounded lookahead (2), per `[gram.amb.structlit]`: the block
/// interpretation wins everywhere the peek is ambiguous.
fn at_struct_lit_fields(p: &Parser<'_>) -> bool {
    p.nth(1) == TokenKind::Ident && p.nth(2) == TokenKind::Punct(Punct::Colon)
}

/// The optional end of a range: `..`, `..b`, `..^b`.
fn maybe_range_end(p: &mut Parser<'_>, ctx: Ctx) {
    if p.at_punct(Punct::Caret) {
        from_end(p, ctx);
    } else if at_expr_start(p) && expr_bp(p, RANGE_BP.1, ctx).is_none() {
        p.error(codes::EXPECTED_TOKEN, p.here(), "expected an expression");
        p.missing();
    }
}

/// The LEADING-dots alternative of `range_expr` — `('..' | '..=')
/// r_end` — whose endpoint is NOT optional ([gram.expr.primary]).
///
/// Only the *trailing* endpoint may be omitted (`a..`), so `a..`,
/// `..b` and `a..b` are expressions and a bare `..` is not. That is
/// deliberate rather than an oversight: `..` already means "and the
/// rest" one production away (`error_row`'s open-row marker), which is
/// exactly the ambiguity an unrestricted bare `..` in expression
/// position would create. wolf-lang#88 — the parser used to admit it,
/// so `s[..]` ran and `s[..=]` trapped, both outside the grammar.
fn range_end_required(p: &mut Parser<'_>, ctx: Ctx) {
    if p.at_punct(Punct::Caret) || at_expr_start(p) {
        maybe_range_end(p, ctx);
        return;
    }
    p.push_diag(
        wolf_diag::Diagnostic::error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected an expression after `..`",
        )
        .with_label("this range has no endpoint")
        .with_note(
            "a range needs at least one endpoint: `a..b`, `a..` or `..b`. \
             A bare `..` is not an expression — to name a whole string or \
             list, use the value itself rather than slicing it \
             ([gram.expr.primary]).",
        ),
    );
    p.missing();
}

/// `'^' or_expr` — an end-relative endpoint (D25, `r_end`).
fn from_end(p: &mut Parser<'_>, ctx: Ctx) -> CompletedMarker {
    let m = p.start();
    p.bump(); // `^`
    if expr_bp(p, FROM_END_BP, ctx).is_none() {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected an expression after `^`",
        );
        p.missing();
    }
    m.complete(p, SyntaxKind::FromEndExpr)
}

/// The body of a defaulting `else` (D30): `else expr`, `else block`,
/// or `else |pat| expr-or-block` — expression bodies include jump
/// expressions (`else |_| return 1`).
fn else_body(p: &mut Parser<'_>, ctx: Ctx) {
    let mut handler_ok = true;
    if p.at_punct(Punct::Pipe) {
        p.bump(); // `|`
        // A single pattern: or-patterns would consume the closing `|`.
        if grammar::pattern_atom(p).is_none() {
            p.error(
                codes::EXPECTED_PATTERN,
                p.here(),
                "expected a pattern for the `else` handler",
            );
            p.missing();
            handler_ok = false;
        }
        if p.at_punct(Punct::Pipe) {
            p.bump();
        } else {
            // One report per broken handler; skip the junk to the
            // closing `|` (or the body) instead of cascading.
            if handler_ok {
                p.error(
                    codes::EXPECTED_TOKEN,
                    p.here(),
                    "expected `|` to close the handler pattern",
                );
                handler_ok = false;
            }
            p.missing();
            p.recover_until(true, |k| {
                matches!(k, TokenKind::Punct(Punct::Pipe | Punct::LBrace)) || k == TokenKind::Term
            });
            if p.at_punct(Punct::Pipe) {
                p.bump();
            }
        }
    }
    if p.at_punct(Punct::LBrace) {
        block(p);
    } else if expr_bp(p, ELSE_BP.1, ctx).is_none() {
        if handler_ok {
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                "expected an expression after `else`",
            );
        }
        p.missing();
        p.fold_line_end();
    }
}

/// A required condition/scrutinee/iterable expression. On failure: one
/// report, then sync to the construct's block (or end of line) so a
/// wrecked condition cannot cascade into body diagnostics.
fn condition_required(p: &mut Parser<'_>, ctx: Ctx, what: &str) {
    if expr_bp(p, 0, ctx.condition()).is_some() {
        return;
    }
    p.error(
        codes::EXPECTED_TOKEN,
        p.current_span(),
        format!("expected {what}"),
    );
    p.missing();
    p.recover_until(true, |k| {
        k == TokenKind::Punct(Punct::LBrace) || k == TokenKind::Term
    });
    p.fold_line_end();
}

// ------------------------------------------------------------- primary --

/// One primary expression (`[gram.expr.primary]`). The `bool` reports
/// whether the result is path-shaped (a candidate struct-literal head).
fn primary(p: &mut Parser<'_>, ctx: Ctx) -> Option<(CompletedMarker, bool)> {
    let cm = match p.current() {
        TokenKind::Int
        | TokenKind::Float
        | TokenKind::Char
        | TokenKind::Kw(Keyword::True | Keyword::False) => {
            let m = p.start();
            p.bump();
            m.complete(p, SyntaxKind::LiteralExpr)
        }
        k if is_str_begin(k) => string_expr(p, ctx),
        TokenKind::Ident => {
            let m = p.start();
            p.bump();
            let cm = m.complete(p, SyntaxKind::PathExpr);
            return Some((cm, true));
        }
        TokenKind::Punct(Punct::LParen) => paren_or_tuple(p, ctx),
        // A leading `{` in condition/scrutinee position begins the
        // construct's block — never a block expression
        // ([gram.amb.structlit]). Letting it parse as one made
        // `match {` swallow the whole arm list as a scrutinee and
        // over-cascade recovery past the measured max (#109).
        TokenKind::Punct(Punct::LBrace) if ctx.no_struct_lit => return None,
        TokenKind::Punct(Punct::LBrace) => block(p),
        TokenKind::Punct(Punct::Not | Punct::Minus | Punct::Star) => prefix_expr(p, ctx),
        TokenKind::Punct(Punct::Amp) => {
            let m = p.start();
            p.bump(); // `&`
            if p.at_kw(Keyword::Mut) {
                p.bump(); // `&mut`
            }
            prefix_operand(p, ctx);
            m.complete(p, SyntaxKind::PrefixExpr)
        }
        TokenKind::Kw(Keyword::Move | Keyword::Copy | Keyword::Shared) => prefix_expr(p, ctx),
        TokenKind::Kw(Keyword::Freeze) => {
            let m = p.start();
            p.bump();
            prefix_operand(p, ctx);
            m.complete(p, SyntaxKind::FreezeExpr)
        }
        TokenKind::Punct(Punct::Caret) => from_end(p, ctx),
        TokenKind::Punct(Punct::DotDot | Punct::DotDotEq) => {
            let m = p.start();
            p.bump();
            range_end_required(p, ctx);
            m.complete(p, SyntaxKind::RangeExpr)
        }
        TokenKind::Kw(Keyword::If) => if_expr(p, ctx),
        TokenKind::Kw(Keyword::Match) => match_expr(p, ctx),
        TokenKind::Kw(Keyword::For) => for_expr(p, ctx),
        TokenKind::Kw(Keyword::While) => {
            let m = p.start();
            p.bump();
            condition_required(p, ctx, "the `while` condition");
            block_required(p);
            m.complete(p, SyntaxKind::WhileExpr)
        }
        TokenKind::Kw(Keyword::Loop) => {
            let m = p.start();
            p.bump();
            block_required(p);
            m.complete(p, SyntaxKind::LoopExpr)
        }
        // Bounded lookahead: closures are anonymous (`fn(`); a *named*
        // `fn` in expression position is the next declaration leaking
        // in — not an expression (the caller's recovery syncs on it).
        TokenKind::Kw(Keyword::Fn) if p.nth(1) == TokenKind::Punct(Punct::LParen) => {
            closure(p, ctx)
        }
        TokenKind::Kw(Keyword::Region) => region_expr(p),
        TokenKind::Kw(Keyword::In) => {
            let m = p.start();
            p.bump();
            condition_required(p, ctx, "the target region of `in`");
            block_required(p);
            m.complete(p, SyntaxKind::InBlock)
        }
        TokenKind::Kw(Keyword::Scope) => {
            let m = p.start();
            p.bump();
            if p.at(TokenKind::Ident) {
                p.bump(); // the scope-handle binder (D16)
            }
            block_required(p);
            m.complete(p, SyntaxKind::ScopeExpr)
        }
        TokenKind::Kw(Keyword::Select) => select_expr(p, ctx),
        TokenKind::Kw(Keyword::When) => when_expr(p, ctx),
        TokenKind::Kw(Keyword::Spawn) => spawn_expr(p, ctx),
        TokenKind::Kw(Keyword::Unsafe) => unsafe_expr(p),
        TokenKind::Kw(Keyword::Asm) => asm_expr(p, ctx),
        TokenKind::Kw(Keyword::Borrow) => borrow_expr(p, ctx),
        TokenKind::Kw(Keyword::Return) => jump_expr(p, ctx, SyntaxKind::ReturnExpr),
        TokenKind::Kw(Keyword::Break) => jump_expr(p, ctx, SyntaxKind::BreakExpr),
        TokenKind::Kw(Keyword::Continue) => {
            let m = p.start();
            p.bump();
            m.complete(p, SyntaxKind::ContinueExpr)
        }
        _ => return None,
    };
    Some((cm, false))
}

/// `! - * & move copy shared` — tier-3 prefix application.
fn prefix_expr(p: &mut Parser<'_>, ctx: Ctx) -> CompletedMarker {
    let m = p.start();
    p.bump(); // the operator
    prefix_operand(p, ctx);
    m.complete(p, SyntaxKind::PrefixExpr)
}

fn prefix_operand(p: &mut Parser<'_>, ctx: Ctx) {
    if expr_bp(p, PREFIX_BP, ctx).is_none() {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected an operand expression",
        );
        p.missing();
    }
}

/// `return expr?` / `break expr?` — never-type expressions (§3.4).
fn jump_expr(p: &mut Parser<'_>, ctx: Ctx, kind: SyntaxKind) -> CompletedMarker {
    let m = p.start();
    p.bump(); // return / break
    if at_expr_start(p) {
        expr_required_ctx(p, ctx);
    }
    m.complete(p, kind)
}

// ------------------------------------------------------ compound forms --

/// `'(' expr (',' expr)* ','? ')'` — grouping or tuple; comma decides.
fn paren_or_tuple(p: &mut Parser<'_>, ctx: Ctx) -> CompletedMarker {
    let m = p.start();
    let opener = p.current_span();
    p.bump(); // `(`
    let inner = ctx.inner();
    let mut tuple = false;
    // `(mut expr)` / `(take expr)` — the X1 moded receiver
    // ([gram.expr.primary]): legal only immediately before a `.`
    // member; anywhere else the mode marks nothing (E0210).
    if matches!(p.current(), TokenKind::Kw(Keyword::Mut | Keyword::Take)) {
        let mode_span = p.current_span();
        let mode_text = if p.at_kw(Keyword::Mut) { "mut" } else { "take" };
        p.bump(); // the mode marker
        if expr_bp(p, 0, inner).is_none() {
            p.arg_list_error(
                p.current_span(),
                "expected an expression after the receiver mode",
            );
            p.recover_until(true, |k| {
                matches!(k, TokenKind::Punct(Punct::RParen)) || k == TokenKind::Term
            });
        }
        let mut close_hi = mode_span.hi;
        if p.at_punct(Punct::RParen) {
            close_hi = p.current_span().hi;
            p.bump();
        } else {
            grammar::unclosed(p, opener, "(");
        }
        let cm = m.complete(p, SyntaxKind::ParenExpr);
        if !p.at_punct(Punct::Dot) {
            // spec pins the primary span to the whole parenthesized
            // receiver, not the keyword (DIV-2026-007)
            let whole = wolf_span::Span::new(mode_span.file, opener.lo, close_hi);
            p.error(
                codes::RECEIVER_MODE,
                whole,
                format!(
                    "`{mode_text}` marks a method receiver, but no method call \
                     follows the `)`"
                ),
            );
        }
        return cm;
    }
    if p.at_punct(Punct::RParen) {
        // `()` is not in the grammar (no unit literal) — diagnose and
        // keep the shape.
        p.arg_list_error(p.here(), "expected an expression inside the parentheses");
        p.missing();
        p.bump(); // `)`
        return m.complete(p, SyntaxKind::ParenExpr);
    }
    loop {
        if p.at_punct(Punct::RParen) {
            p.bump();
            break;
        }
        if paren_escape(p) {
            grammar::unclosed(p, opener, "(");
            break;
        }
        let before = p.pos();
        if expr_bp(p, 0, inner).is_none() {
            p.arg_list_error(p.current_span(), "expected an expression");
            p.recover_until(true, |k| {
                matches!(k, TokenKind::Punct(Punct::Comma | Punct::RParen)) || k == TokenKind::Term
            });
        }
        if p.at_punct(Punct::Comma) {
            tuple = true;
            p.bump();
            continue;
        }
        if p.at_punct(Punct::RParen) {
            continue; // closed at the top of the loop
        }
        if p.pos() == before {
            grammar::unclosed(p, opener, "(");
            break;
        }
        if !paren_escape(p) {
            p.arg_list_error(
                p.here(),
                "expected `,` or `)` in the parenthesized expression",
            );
            p.recover_until(true, |k| {
                matches!(k, TokenKind::Punct(Punct::Comma | Punct::RParen)) || k == TokenKind::Term
            });
            if p.at_punct(Punct::Comma) {
                tuple = true;
                p.bump();
            }
        }
    }
    m.complete(
        p,
        if tuple {
            SyntaxKind::TupleExpr
        } else {
            SyntaxKind::ParenExpr
        },
    )
}

/// Would continuing this parenthesized expression eat the enclosing
/// construct? `fn (` stays legal — closures are expressions — but a
/// *named* `fn` is the next declaration (bounded lookahead).
fn paren_escape(p: &Parser<'_>) -> bool {
    match p.current() {
        TokenKind::Eof | TokenKind::Term => true,
        TokenKind::Punct(Punct::RBrace) => true,
        TokenKind::Kw(Keyword::Fn) => p.nth(1) != TokenKind::Punct(Punct::LParen),
        TokenKind::Kw(k) => is_decl_keyword(k),
        _ => false,
    }
}

/// The delimited argument list of a call (`(`) or bracket apply (`[`).
/// `call_arg ::= ('mut' | 'take')? expr`; bracket arguments also admit
/// the type-only forms (`prefix_type_kw type`, bare `region`) — one
/// grammar, one node kind, sema sees a single argument list (D29).
fn arg_list(p: &mut Parser<'_>, ctx: Ctx) {
    let m = p.start();
    let opener = p.current_span();
    let bracket = p.at_punct(Punct::LBracket);
    let (open_text, close) = if bracket {
        ("[", Punct::RBracket)
    } else {
        ("(", Punct::RParen)
    };
    p.bump(); // the opener
    let inner = ctx.inner();
    loop {
        if p.at_punct(close) {
            p.bump();
            break;
        }
        if arg_escape(p, close) {
            grammar::unclosed(p, opener, open_text);
            break;
        }
        let before = p.pos();
        arg(p, inner, close);
        if p.at_punct(Punct::Comma) {
            p.bump();
            continue;
        }
        if p.at_punct(close) {
            continue;
        }
        if p.pos() == before {
            grammar::unclosed(p, opener, open_text);
            break;
        }
        if !arg_escape(p, close) {
            p.arg_list_error(
                p.here(),
                format!(
                    "expected `,` or `{}` in the argument list",
                    if bracket { "]" } else { ")" }
                ),
            );
            p.recover_until(true, |k| {
                matches!(k, TokenKind::Punct(pc) if pc == close || pc == Punct::Comma)
                    || k == TokenKind::Term
            });
            if p.at_punct(Punct::Comma) {
                p.bump();
            }
        }
    }
    m.complete(p, SyntaxKind::ArgList);
}

/// Would continuing this argument list eat the enclosing construct?
/// Same named-`fn` lookahead as [`paren_escape`].
fn arg_escape(p: &Parser<'_>, close: Punct) -> bool {
    match p.current() {
        TokenKind::Eof | TokenKind::Term => true,
        TokenKind::Punct(Punct::RBrace) => close != Punct::RBrace,
        TokenKind::Kw(Keyword::Fn) => p.nth(1) != TokenKind::Punct(Punct::LParen),
        TokenKind::Kw(k) => is_decl_keyword(k),
        _ => false,
    }
}

/// One argument.
fn arg(p: &mut Parser<'_>, ctx: Ctx, close: Punct) {
    let a = p.start();
    match p.current() {
        TokenKind::Kw(Keyword::Mut | Keyword::Take) => {
            p.bump(); // the X1 call-site mode marker
            expr_required_ctx(p, ctx);
        }
        // The forms only types can spell (`List[handle Node]`) — the
        // rest of type syntax IS expression syntax in wolf (D29).
        TokenKind::Kw(Keyword::Shared | Keyword::Handle | Keyword::Weak | Keyword::Distinct) => {
            grammar::type_required(p);
        }
        // Bounded lookahead: bare `region` is the region *type*
        // (`channel[region]`); `region(…)`/`region {…}` are expressions.
        TokenKind::Kw(Keyword::Region)
            if p.nth(1) == TokenKind::Punct(Punct::Comma)
                || p.nth(1) == TokenKind::Punct(close) =>
        {
            grammar::type_required(p);
        }
        _ => {
            // The D25 hint: a unary-minus integer literal alone in a
            // bracket argument is Python-style negative indexing, which
            // wolf spells `^n` (from the end). One report, with the
            // machine-applicable `-` → `^` edit; the expression still
            // parses so the tree stays complete.
            if close == Punct::RBracket
                && p.at_punct(Punct::Minus)
                && p.nth(1) == TokenKind::Int
                && matches!(p.nth(2), TokenKind::Punct(Punct::RBracket | Punct::Comma))
            {
                let minus = p.current_span();
                let index = String::from_utf8_lossy(p.nth_text(1)).into_owned();
                p.push_diag(
                    wolf_diag::Diagnostic::error(
                        codes::NEGATIVE_INDEX,
                        minus.join(p.nth_span(1)),
                        "wolf has no negative indexing",
                    )
                    .with_label(format!("did you mean `^{index}`?"))
                    .with_note(
                        "wolf counts from the end with `^`: `s[^1]` is the last \
                         element, `s[1..^1]` trims one from each side (D25).",
                    )
                    .with_suggestion(wolf_diag::Suggestion::new(
                        format!("index from the end: `^{index}`"),
                        vec![(minus, "^".to_string())],
                        wolf_diag::Applicability::MachineApplicable,
                    )),
                );
            }
            if expr_bp(p, 0, ctx).is_none() {
                p.arg_list_error(p.current_span(), "expected an argument");
                p.recover_until(true, |k| {
                    matches!(k, TokenKind::Punct(pc) if pc == close || pc == Punct::Comma)
                        || k == TokenKind::Term
                });
            }
        }
    }
    a.complete(p, SyntaxKind::Arg);
}

/// `path '{' (field_init (',' field_init)* ','?)? '}'` — wraps the
/// already-parsed path chain.
fn struct_lit(p: &mut Parser<'_>, lhs: CompletedMarker, ctx: Ctx) -> CompletedMarker {
    let m = lhs.precede(p);
    let opener = p.current_span();
    p.bump(); // `{`
    let inner = ctx.inner();
    loop {
        p.eat_terms();
        if p.at_punct(Punct::RBrace) {
            p.bump();
            break;
        }
        if p.at_eof() || matches!(p.current(), TokenKind::Kw(k) if is_decl_keyword(k)) {
            grammar::unclosed(p, opener, "{");
            break;
        }
        let before = p.pos();
        field_init(p, inner);
        // Note: no per-field latch reset here — a `{` mistaken for a
        // struct literal chews line-shaped "fields" that alternate
        // between clean and broken; one report covers the wreck.
        if p.at_punct(Punct::Comma) {
            p.bump();
        }
        if p.pos() == before {
            grammar::unclosed(p, opener, "{");
            break;
        }
    }
    m.complete(p, SyntaxKind::StructLit)
}

/// `IDENT (':' expr)?` — `Point { x }` shorthand binds from the name.
fn field_init(p: &mut Parser<'_>, ctx: Ctx) {
    let f = p.start();
    if p.at(TokenKind::Ident) {
        p.bump();
        if p.at_punct(Punct::Colon) {
            p.bump();
            expr_required_ctx(p, ctx);
        }
    } else {
        p.arm_error(p.current_span(), "expected a field initializer");
        p.recover_until(true, |k| {
            matches!(k, TokenKind::Punct(Punct::Comma | Punct::RBrace)) || k == TokenKind::Term
        });
    }
    f.complete(p, SyntaxKind::FieldInit);
}

// --------------------------------------------------------- string mode --

/// One whole string episode as an expression, interpolations parsed as
/// real expressions (`[gram.lex.str]`). `ctx.str_depth` counts the
/// enclosing strings: nesting past 8 is E0007 ("you do not want this");
/// the lexer's hard rail at 32 is E0108.
fn string_expr(p: &mut Parser<'_>, ctx: Ctx) -> CompletedMarker {
    let m = p.start();
    let depth = ctx.str_depth.saturating_add(1);
    if depth == 9 {
        p.push_diag(
            wolf_diag::Diagnostic::error(
                codes::INTERP_TOO_DEEP,
                p.current_span(),
                "string interpolations nest 9 levels deep here — the limit is 8",
            )
            .with_label("the ninth level starts here")
            .with_note(
                "hoist the innermost string into a `let` binding and interpolate \
                 the binding; each hoist removes a level.",
            ),
        );
    }
    p.bump(); // StrBegin
    loop {
        match p.current() {
            TokenKind::StrFragment | TokenKind::Error => p.bump(),
            TokenKind::InterpOpen => {
                let inner = Ctx {
                    no_struct_lit: false,
                    stmt_head: false,
                    str_depth: depth,
                };
                interp(p, inner);
            }
            TokenKind::StrEnd { .. } => {
                p.bump();
                break;
            }
            // Defensive: the lexer guarantees episode balance.
            _ => break,
        }
    }
    m.complete(p, SyntaxKind::StringExpr)
}

/// `'{' expr format_spec? '}'` inside a string.
fn interp(p: &mut Parser<'_>, ctx: Ctx) {
    let m = p.start();
    p.bump(); // InterpOpen
    let expr_ok = expr_bp(p, 0, ctx).is_some();
    if !expr_ok {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected an expression in the interpolation",
        );
        p.missing();
    }
    if p.at(TokenKind::FormatSpecBegin) {
        let f = p.start();
        p.bump(); // `:`
        loop {
            match p.current() {
                TokenKind::StrFragment | TokenKind::Error => p.bump(),
                TokenKind::InterpOpen => interp(p, ctx), // `{width}` re-nests
                _ => break,
            }
        }
        f.complete(p, SyntaxKind::FormatSpec);
    }
    if p.at(TokenKind::InterpClose) {
        p.bump();
    } else {
        // One report per broken interpolation: the failed-expression
        // diagnostic already covers the junk case, and the per-line
        // fold covers a wrecked interpolation payload.
        if expr_ok {
            p.arg_list_error(p.here(), "expected `}` to close the interpolation");
        }
        p.recover_to_interp_close();
        if p.at(TokenKind::InterpClose) {
            p.bump();
        } else {
            p.missing();
        }
    }
    m.complete(p, SyntaxKind::Interp);
}

// --------------------------------------------------------- control flow --

/// `if expr block ('else' (if_expr | block))?` (`[gram.expr.flow]`).
/// An `else` here belongs to the `if` only when `if` or `{` follows —
/// otherwise it is the defaulting operator on the completed if
/// expression (`[gram.amb.else]`, report 05's corner case).
fn if_expr(p: &mut Parser<'_>, ctx: Ctx) -> CompletedMarker {
    let m = p.start();
    p.bump(); // if
    condition_required(p, ctx, "the `if` condition");
    block_required(p);
    // E0005: `else` on the next line — the inserted terminator after
    // `}` would orphan it. Diagnose, absorb the terminator, continue.
    if p.at(TokenKind::Term)
        && p.nth(1) == TokenKind::Kw(Keyword::Else)
        && matches!(
            p.nth(2),
            TokenKind::Kw(Keyword::If) | TokenKind::Punct(Punct::LBrace)
        )
        && p.current_text() != b";"
    {
        p.error(
            codes::ELSE_ON_NEW_LINE,
            p.nth_span(1),
            "`else` must be on the same line as the `}` that closes the \
             then-block — write `} else`",
        );
        p.bump(); // the terminator joins the if
    }
    if p.at_kw(Keyword::Else)
        && matches!(
            p.nth(1),
            TokenKind::Kw(Keyword::If) | TokenKind::Punct(Punct::LBrace)
        )
    {
        p.bump(); // else
        if p.at_kw(Keyword::If) {
            if_expr(p, ctx);
        } else {
            block(p);
        }
    }
    m.complete(p, SyntaxKind::IfExpr)
}

/// `for pattern in expr block`.
fn for_expr(p: &mut Parser<'_>, ctx: Ctx) -> CompletedMarker {
    let m = p.start();
    p.bump(); // for
    if grammar::pattern(p).is_none() {
        p.error(
            codes::EXPECTED_PATTERN,
            p.here(),
            "expected the loop pattern after `for`",
        );
        p.missing();
        if !p.at_kw(Keyword::In) {
            // Hopeless header — one report, sync to the loop body.
            p.recover_until(true, |k| {
                k == TokenKind::Punct(Punct::LBrace) || k == TokenKind::Term
            });
            p.fold_line_end();
            if p.at_punct(Punct::LBrace) {
                block(p);
            }
            return m.complete(p, SyntaxKind::ForExpr);
        }
    }
    p.expect_kw(Keyword::In, "`in` in the for loop");
    condition_required(p, ctx, "the iterable expression");
    block_required(p);
    m.complete(p, SyntaxKind::ForExpr)
}

/// Do the braces at the current position (`p` at `{`) enclose
/// arm-shaped content — a `=>` at nesting depth 1 before the matching
/// `}`? Lookahead, bounded by the brace's own extent, used only on the
/// wreck path (a `match` whose scrutinee is missing): committing such
/// a brace to the arm grammar when its content is statement-shaped
/// misreads every statement as a pattern and over-cascades recovery
/// (#109's neighbor case), while refusing a genuine arm list would
/// wreck the honest `match { A => … }` typo. The content decides.
fn braces_enclose_arms(p: &Parser<'_>) -> bool {
    debug_assert!(p.at_punct(Punct::LBrace), "lookahead off `{{`");
    let mut depth = 1usize;
    for i in 1.. {
        match p.nth(i) {
            TokenKind::Punct(Punct::LBrace | Punct::LParen | Punct::LBracket) => depth += 1,
            TokenKind::Punct(Punct::RBrace | Punct::RParen | Punct::RBracket) => {
                depth -= 1;
                if depth == 0 {
                    return false; // closed without a top-level `=>`
                }
            }
            TokenKind::Punct(Punct::FatArrow) if depth == 1 => return true,
            TokenKind::Eof => return false,
            _ => {}
        }
    }
    false
}

/// `match expr '{' arm* '}'`.
fn match_expr(p: &mut Parser<'_>, ctx: Ctx) -> CompletedMarker {
    let m = p.start();
    p.bump(); // match
    let scrutinee_ok = expr_bp(p, 0, ctx.condition()).is_some();
    if !scrutinee_ok {
        // condition_required's shape, inlined: the arms decision below
        // needs to know the scrutinee was missing.
        p.error(
            codes::EXPECTED_TOKEN,
            p.current_span(),
            "expected the match scrutinee",
        );
        p.missing();
        p.recover_until(true, |k| {
            k == TokenKind::Punct(Punct::LBrace) || k == TokenKind::Term
        });
        p.fold_line_end();
    }
    if !p.at_punct(Punct::LBrace) || (!scrutinee_ok && !braces_enclose_arms(p)) {
        p.error_unless_folded(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `{` to open the match arms",
        );
        p.missing();
        p.fold_line_end();
        return m.complete(p, SyntaxKind::MatchExpr);
    }
    let opener = p.current_span();
    p.bump(); // `{`
    loop {
        p.eat_terms();
        if p.at_punct(Punct::RBrace) {
            p.bump();
            break;
        }
        if p.at_eof() || matches!(p.current(), TokenKind::Kw(k) if is_decl_keyword(k)) {
            grammar::unclosed(p, opener, "{");
            break;
        }
        let before = p.pos();
        let diags_before = p.diags.len();
        match_arm(p, ctx);
        if p.diags.len() == diags_before {
            p.arm_error_reported = false;
        }
        if p.pos() == before {
            grammar::unclosed(p, opener, "{");
            break;
        }
    }
    m.complete(p, SyntaxKind::MatchExpr)
}

/// `pattern ('if' expr)? '=>' (expr | block) arm_sep?`.
fn match_arm(p: &mut Parser<'_>, ctx: Ctx) {
    let a = p.start();
    if grammar::pattern(p).is_none() {
        if !p.arm_error_reported {
            p.arm_error_reported = true;
            p.error(
                codes::EXPECTED_PATTERN,
                p.current_span(),
                "expected a pattern to start the match arm",
            );
        }
        p.recover_until(true, |k| {
            matches!(k, TokenKind::Punct(Punct::FatArrow | Punct::Comma)) || k == TokenKind::Term
        });
    }
    if p.at_kw(Keyword::If) {
        let g = p.start();
        p.bump();
        expr_required_ctx(p, ctx.inner());
        g.complete(p, SyntaxKind::MatchGuard);
    }
    if !arm_arrow(p, "match") {
        a.complete(p, SyntaxKind::MatchArm);
        return;
    }
    let body_is_block = arm_body(p, ctx);
    arm_separator(p, body_is_block);
    a.complete(p, SyntaxKind::MatchArm);
}

/// The `=>` of a match/select arm, with bail-out recovery: a miss is
/// diagnosed once; when a body can still start here the arm continues
/// (`2 20,` — just the arrow missing), otherwise the junk is skipped
/// to `=>` / `,` / end-of-line and the arm is abandoned (returns
/// false) so one broken arm cannot poison its neighbors.
fn arm_arrow(p: &mut Parser<'_>, what: &str) -> bool {
    if p.at_punct(Punct::FatArrow) {
        p.bump();
        return true;
    }
    p.arm_error(p.here(), format!("expected `=>` in the {what} arm"));
    if at_expr_start(p) {
        p.missing();
        return true;
    }
    p.recover_until(true, |k| {
        matches!(k, TokenKind::Punct(Punct::FatArrow | Punct::Comma)) || k == TokenKind::Term
    });
    if p.at_punct(Punct::FatArrow) {
        p.bump();
        true
    } else {
        p.missing();
        if p.at_punct(Punct::Comma) || p.at(TokenKind::Term) {
            p.bump();
        }
        false
    }
}

/// The arm body: a block, or one maximal expression. Returns whether it
/// was block-bodied (the separator rule differs, §3.4).
fn arm_body(p: &mut Parser<'_>, ctx: Ctx) -> bool {
    if p.at_punct(Punct::LBrace) {
        block(p);
        true
    } else {
        if expr_bp(p, 0, ctx.inner()).is_none() {
            p.error(codes::EXPECTED_TOKEN, p.here(), "expected the arm body");
            p.missing();
        }
        false
    }
}

/// Arm separators (§3.4, shared with `select`): the comma is required
/// after an expression-bodied arm that is followed by another arm,
/// optional after a block-bodied arm — the inserted terminator after
/// its `}` is the separator.
fn arm_separator(p: &mut Parser<'_>, body_is_block: bool) {
    if p.at_punct(Punct::Comma) {
        p.bump();
    } else if p.at(TokenKind::Term) {
        p.bump();
        if !body_is_block && !p.at_punct(Punct::RBrace) && !p.at_eof() {
            p.arm_error(p.here(), "expected `,` after the expression-bodied arm");
        }
    } else if !p.at_punct(Punct::RBrace) && !p.at_eof() {
        p.arm_error(p.here(), "expected `,` or a line break between arms");
    }
}

// ------------------------------------------------------------ closures --

/// `'fn' '(' params_untyped? ')' (block | expr)` — an expression body
/// extends maximally rightward (`[gram.amb.closure]`); only a token the
/// expression grammar cannot consume terminates it.
fn closure(p: &mut Parser<'_>, ctx: Ctx) -> CompletedMarker {
    let m = p.start();
    p.bump(); // fn
    let diags_before = p.diags.len();
    let params_ok = if p.at_punct(Punct::LParen) {
        closure_params(p);
        p.diags.len() == diags_before
    } else {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `(` to start the closure parameters",
        );
        p.missing();
        false
    };
    if p.at_punct(Punct::LBrace) {
        block(p);
    } else if expr_bp(p, 0, ctx).is_none() {
        // One report per truncated closure: a missing parameter list
        // was already diagnosed above, and a body starting with an
        // assignment operator is better described by E0208's site.
        if params_ok && !is_assign_op(p.current()) {
            p.error(codes::EXPECTED_TOKEN, p.here(), "expected the closure body");
        }
        p.missing();
    }
    m.complete(p, SyntaxKind::ClosureExpr)
}

/// `closure_param ::= param_mode? IDENT (':' type)?`.
fn closure_params(p: &mut Parser<'_>) {
    let pl = p.start();
    let opener = p.current_span();
    p.bump(); // `(`
    loop {
        if p.at_punct(Punct::RParen) {
            p.bump();
            break;
        }
        if p.at_eof()
            || p.at(TokenKind::Term)
            || p.at_punct(Punct::LBrace)
            || p.at_punct(Punct::RBrace)
        {
            grammar::unclosed(p, opener, "(");
            break;
        }
        let before = p.pos();
        let cp = p.start();
        if matches!(p.current(), TokenKind::Kw(Keyword::Mut | Keyword::Take)) {
            p.bump();
        }
        if p.at(TokenKind::Ident) || matches!(p.current(), TokenKind::Kw(k) if !is_decl_keyword(k))
        {
            grammar::name_token(p, "closure parameter");
        } else {
            p.arg_list_error(p.here(), "expected a closure parameter name");
            p.missing();
        }
        if p.at_punct(Punct::Colon) {
            p.bump();
            grammar::type_required(p);
        }
        cp.complete(p, SyntaxKind::Param);
        if p.at_punct(Punct::Comma) {
            p.bump();
        }
        if p.pos() == before {
            grammar::unclosed(p, opener, "(");
            break;
        }
    }
    pl.complete(p, SyntaxKind::ParamList);
}

// -------------------------------------------------------- regions (X4) --

/// `[gram.expr.region]` — the sugar
/// (`region name? ('(' cap ')')? (: strategy)? {…}`) and value
/// (`region '(' (strategy (',' cap)? | cap)? ')'`) forms are distinct
/// node kinds: the sugar means create + scope + free, the value form
/// is first-class. The cap clause (`cap ':' expr`, s132 —
/// `[mem.region.cap.1]`, D68/#187) is a creation-time byte budget; on
/// the sugar form its parenthesis follows the NAME (an anonymous
/// sugar block takes no cap — `region (cap: N)` is the value form's
/// spelling by the grammar's own disambiguation, so a budgeted sugar
/// block names its region).
fn region_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(); // region
    if p.at_punct(Punct::LParen) {
        p.bump();
        if p.at_punct(Punct::RParen) {
            p.bump();
        } else {
            if at_region_cap(p) {
                region_cap(p);
            } else {
                region_strategy(p);
                if p.at_punct(Punct::Comma) {
                    p.bump();
                    if at_region_cap(p) {
                        region_cap(p);
                    } else {
                        p.error(
                            codes::EXPECTED_TOKEN,
                            p.here(),
                            "expected `cap: <bytes>` — the strategy is first, the cap last",
                        );
                        p.missing();
                    }
                }
            }
            p.expect_punct(Punct::RParen, "`)` to close `region(`");
        }
        return m.complete(p, SyntaxKind::RegionValue);
    }
    if p.at(TokenKind::Ident) {
        p.bump(); // the region name
        if p.at_punct(Punct::LParen) {
            // `region r(cap: N)` — the sugar form's cap parenthesis.
            let opener = p.current_span();
            p.bump();
            if at_region_cap(p) {
                region_cap(p);
            } else {
                p.error(
                    codes::EXPECTED_TOKEN,
                    p.here(),
                    "expected `cap: <bytes>` — the parenthesis after a region's name \
                     holds its byte budget ([mem.region.cap.1])",
                );
                p.missing();
                p.recover_until(true, |k| {
                    matches!(k, TokenKind::Punct(Punct::RParen | Punct::LBrace))
                        || k == TokenKind::Term
                });
            }
            if !p.at_punct(Punct::RParen) {
                grammar::unclosed(p, opener, "(");
            } else {
                p.bump();
            }
        }
    }
    if p.at_punct(Punct::Colon) {
        p.bump();
        region_strategy(p);
    }
    block_required(p);
    m.complete(p, SyntaxKind::RegionBlock)
}

/// At the start of a cap clause: the contextual `cap` followed by `:`
/// (`[gram.inv.ctx]` — `cap` stays an ordinary identifier everywhere
/// else, the strategy names' own discipline).
fn at_region_cap(p: &Parser<'_>) -> bool {
    p.at(TokenKind::Ident)
        && p.current_text() == b"cap"
        && matches!(p.nth(1), TokenKind::Punct(Punct::Colon))
}

/// `region_cap ::= 'cap' ':' expr` — the creation-time byte budget
/// (`[mem.region.cap.1]`): an `int` expression, evaluated at region
/// creation.
fn region_cap(p: &mut Parser<'_>) {
    let c = p.start();
    p.bump(); // cap
    p.bump(); // :
    expr_required(p);
    c.complete(p, SyntaxKind::RegionCap);
}

/// `region_strategy ::= 'rc' | 'pool' '(' type ')'` — `rc` and `pool`
/// are contextual (`[gram.inv.ctx]`).
fn region_strategy(p: &mut Parser<'_>) {
    let s = p.start();
    if p.at(TokenKind::Ident) && p.current_text() == b"rc" {
        p.bump();
    } else if p.at(TokenKind::Ident) && p.current_text() == b"pool" {
        p.bump();
        if p.at_punct(Punct::LParen) {
            p.bump();
            grammar::type_required(p);
            p.expect_punct(Punct::RParen, "`)` to close `pool(`");
        } else {
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                "expected `(` — the pool strategy names its element type",
            );
            p.missing();
        }
    } else {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected a region strategy: `rc` or `pool(T)`",
        );
        p.missing();
        // One report per broken strategy: sync to the closer.
        p.recover_until(true, |k| {
            matches!(k, TokenKind::Punct(Punct::RParen | Punct::LBrace)) || k == TokenKind::Term
        });
        p.fold_line_end();
    }
    s.complete(p, SyntaxKind::RegionStrategy);
}

// -------------------------------------------------- concurrency surface --

/// `select '{' select_arm (arm_sep select_arm)* arm_sep? '}'`.
fn select_expr(p: &mut Parser<'_>, ctx: Ctx) -> CompletedMarker {
    let m = p.start();
    p.bump(); // select
    if !p.at_punct(Punct::LBrace) {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `{` to open the select arms",
        );
        p.missing();
        p.fold_line_end();
        return m.complete(p, SyntaxKind::SelectExpr);
    }
    let opener = p.current_span();
    p.bump(); // `{`
    loop {
        p.eat_terms();
        if p.at_punct(Punct::RBrace) {
            p.bump();
            break;
        }
        if p.at_eof() || matches!(p.current(), TokenKind::Kw(k) if is_decl_keyword(k)) {
            grammar::unclosed(p, opener, "{");
            break;
        }
        let before = p.pos();
        let diags_before = p.diags.len();
        select_arm(p, ctx);
        if p.diags.len() == diags_before {
            p.arm_error_reported = false;
        }
        if p.pos() == before {
            grammar::unclosed(p, opener, "{");
            break;
        }
    }
    m.complete(p, SyntaxKind::SelectExpr)
}

/// `pattern 'from' expr '=>' body` or `'timeout' '(' expr ')' '=>'
/// body` — `from` and `timeout` are contextual (`[gram.inv.ctx]`).
fn select_arm(p: &mut Parser<'_>, ctx: Ctx) {
    let a = p.start();
    // Bounded lookahead: `timeout(` is the timeout arm; a bare
    // `timeout` identifier could only be a pattern, which needs `from`.
    if p.at(TokenKind::Ident)
        && p.current_text() == b"timeout"
        && p.nth(1) == TokenKind::Punct(Punct::LParen)
    {
        p.bump(); // timeout
        p.bump(); // `(`
        let timeout_ok = if expr_bp(p, 0, ctx.inner()).is_some() {
            true
        } else {
            // One report per broken timeout arm.
            p.arm_error(p.here(), "expected the timeout duration expression");
            p.missing();
            false
        };
        if p.at_punct(Punct::RParen) {
            p.bump();
        } else {
            if timeout_ok {
                p.arm_error(p.here(), "expected `)` to close `timeout(`");
            }
            p.missing();
        }
    } else {
        if grammar::pattern(p).is_none() {
            if !p.arm_error_reported {
                p.arm_error_reported = true;
                p.error(
                    codes::EXPECTED_PATTERN,
                    p.current_span(),
                    "expected a pattern to start the select arm",
                );
            }
            p.recover_until(true, |k| {
                matches!(k, TokenKind::Punct(Punct::FatArrow | Punct::Comma))
                    || k == TokenKind::Term
            });
        }
        if p.at(TokenKind::Ident) && p.current_text() == b"from" {
            p.bump();
            expr_required_ctx(p, ctx.inner());
        } else {
            p.arm_error(p.here(), "expected `from` in the select arm");
            p.missing();
            // Salvage the channel expression only when one can start
            // here; otherwise let the arrow recovery take over.
            if at_expr_start(p) {
                expr_required_ctx(p, ctx.inner());
            }
        }
    }
    if !arm_arrow(p, "select") {
        a.complete(p, SyntaxKind::SelectArm);
        return;
    }
    let body_is_block = arm_body(p, ctx);
    arm_separator(p, body_is_block);
    a.complete(p, SyntaxKind::SelectArm);
}

/// `'when' '(' expr (',' expr)+ ','? ')' block` — whole-set
/// acquisition is the construct's point ([conc.when.order]), so the
/// grammar requires at least two operands. (The old hint here named a
/// sync-type method that does not exist — `Mutex` has no methods;
/// the message now states the whole-set rule instead.)
fn when_expr(p: &mut Parser<'_>, ctx: Ctx) -> CompletedMarker {
    let m = p.start();
    p.bump(); // when
    let mut count = 0usize;
    let mut broken = false;
    if p.at_punct(Punct::LParen) {
        let opener = p.current_span();
        p.bump();
        loop {
            if p.at_punct(Punct::RParen) {
                p.bump();
                break;
            }
            if paren_escape(p) {
                grammar::unclosed(p, opener, "(");
                broken = true;
                break;
            }
            let before = p.pos();
            if expr_bp(p, 0, ctx.inner()).is_none() {
                broken = true;
                p.arg_list_error(p.current_span(), "expected a `when` operand");
                p.recover_until(true, |k| {
                    matches!(k, TokenKind::Punct(Punct::Comma | Punct::RParen))
                        || k == TokenKind::Term
                });
            } else {
                count += 1;
            }
            if p.at_punct(Punct::Comma) {
                p.bump();
                continue;
            }
            if p.at_punct(Punct::RParen) {
                continue;
            }
            if p.pos() == before {
                grammar::unclosed(p, opener, "(");
                broken = true;
                break;
            }
            if !paren_escape(p) {
                broken = true;
                p.arg_list_error(p.here(), "expected `,` or `)` in the `when` operand list");
                p.recover_until(true, |k| {
                    matches!(k, TokenKind::Punct(Punct::Comma | Punct::RParen))
                        || k == TokenKind::Term
                });
                if p.at_punct(Punct::Comma) {
                    p.bump();
                }
            }
        }
        // The arity rule only makes sense over an intact operand list.
        if count < 2 && !broken {
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                "`when` requires at least two operands — it acquires its \
                 whole set at once, so name every sync object the body \
                 touches in one `when` list",
            );
        }
    } else {
        p.error(codes::EXPECTED_TOKEN, p.here(), "expected `(` after `when`");
        p.missing();
        broken = true;
    }
    if broken && !p.at_punct(Punct::LBrace) {
        p.missing(); // the wreck was reported; no block cascade
        p.fold_line_end();
    } else {
        block_required(p);
    }
    m.complete(p, SyntaxKind::WhenExpr)
}

/// `'spawn' 'proc' path call_args` — proc spawning under the current
/// supervisor; linking/monitoring are methods, not keywords.
fn spawn_expr(p: &mut Parser<'_>, ctx: Ctx) -> CompletedMarker {
    let m = p.start();
    p.bump(); // spawn
    if !p.at_kw(Keyword::Proc) && !p.at(TokenKind::Ident) {
        // Hopeless header — `spawn` itself probably strayed here. One
        // report, no proc-name/argument cascade.
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `proc` after `spawn`",
        );
        p.missing();
        return m.complete(p, SyntaxKind::SpawnExpr);
    }
    p.expect_kw(Keyword::Proc, "`proc` after `spawn`");
    let name_ok = p.at(TokenKind::Ident);
    if name_ok {
        grammar::path(p, "proc");
    } else {
        p.error(codes::EXPECTED_TOKEN, p.here(), "expected the proc name");
        p.missing();
    }
    if p.at_punct(Punct::LParen) {
        arg_list(p, ctx);
    } else {
        // One report per truncated spawn: a missing proc name was
        // already diagnosed above.
        if name_ok {
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                "expected `(…)` proc arguments",
            );
        }
        p.missing();
        p.fold_line_end();
    }
    m.complete(p, SyntaxKind::SpawnExpr)
}

// ----------------------------------------------------------- unsafe tier --

/// `'unsafe' block` or `'unsafe' 'c' capture_list? { … }` — the
/// inline-C body is opaque brace-balanced token text (c10 owns it).
fn unsafe_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(); // unsafe
    if p.at(TokenKind::Ident) && p.current_text() == b"c" {
        p.bump(); // contextual `c`
        if p.at_punct(Punct::LBracket) {
            capture_list(p);
        }
        if p.at_punct(Punct::LBrace) {
            grammar::opaque_brace_block(p);
        } else {
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                "expected `{` to open the inline-C body",
            );
            p.missing();
        }
        return m.complete(p, SyntaxKind::InlineC);
    }
    block_required(p);
    m.complete(p, SyntaxKind::UnsafeBlock)
}

/// `'[' IDENT (',' IDENT)* ','? ']'`.
fn capture_list(p: &mut Parser<'_>) {
    let c = p.start();
    let opener = p.current_span();
    p.bump(); // `[`
    loop {
        if p.at_punct(Punct::RBracket) {
            p.bump();
            break;
        }
        if p.at_eof() || p.at(TokenKind::Term) || p.at_punct(Punct::LBrace) {
            grammar::unclosed(p, opener, "[");
            break;
        }
        let before = p.pos();
        if p.at(TokenKind::Ident) {
            p.bump();
        } else {
            p.error(
                codes::EXPECTED_TOKEN,
                p.current_span(),
                "expected a captured name",
            );
            p.recover_until(true, |k| {
                matches!(k, TokenKind::Punct(Punct::Comma | Punct::RBracket))
                    || k == TokenKind::Term
            });
        }
        if p.at_punct(Punct::Comma) {
            p.bump();
        }
        if p.pos() == before {
            grammar::unclosed(p, opener, "[");
            break;
        }
    }
    c.complete(p, SyntaxKind::CaptureList);
}

/// `'asm' '{' STRING (',' asm_operand)* ','? '}'` — typed-operand
/// inline asm (RFC-2873 lineage). The template's `{t}` placeholders
/// lex as interpolations; c10 reinterprets them as operand references.
fn asm_expr(p: &mut Parser<'_>, ctx: Ctx) -> CompletedMarker {
    let m = p.start();
    p.bump(); // asm
    if !p.at_punct(Punct::LBrace) {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `{` to open the asm block",
        );
        p.missing();
        return m.complete(p, SyntaxKind::AsmExpr);
    }
    let opener = p.current_span();
    p.bump(); // `{`
    p.eat_terms();
    if p.at_str_begin() {
        string_expr(p, ctx.inner());
    } else {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected the asm template string",
        );
        p.missing();
    }
    loop {
        p.eat_terms();
        if p.at_punct(Punct::RBrace) {
            p.bump();
            break;
        }
        if p.at_punct(Punct::Comma) {
            p.bump();
            continue;
        }
        if p.at_eof() || matches!(p.current(), TokenKind::Kw(k) if is_decl_keyword(k)) {
            grammar::unclosed(p, opener, "{");
            break;
        }
        let before = p.pos();
        let diags_before = p.diags.len();
        asm_operand(p, ctx);
        if p.diags.len() == diags_before {
            p.arm_error_reported = false;
        }
        if p.pos() == before {
            grammar::unclosed(p, opener, "{");
            break;
        }
    }
    m.complete(p, SyntaxKind::AsmExpr)
}

/// `(IDENT '=')? asm_dir '(' asm_constraint ')' expr` — `in` is the
/// reserved keyword; `out`/`inout`/`lateout` are contextual.
fn asm_operand(p: &mut Parser<'_>, ctx: Ctx) {
    let o = p.start();
    // Bounded lookahead: `t = inout(…)` names the operand; a bare
    // direction starts unnamed. The `=` after an identifier decides.
    if p.at(TokenKind::Ident) && p.nth(1) == TokenKind::Punct(Punct::Eq) {
        p.bump(); // the operand name
        p.bump(); // `=`
    }
    let dir_ok = p.at_kw(Keyword::In) || p.at(TokenKind::Ident);
    if dir_ok {
        p.bump(); // the direction
    } else {
        p.arm_error(
            p.here(),
            "expected an asm direction (`in`, `out`, `inout`, `lateout`)",
        );
        p.missing();
    }
    if p.at_punct(Punct::LParen) {
        p.bump();
        if p.at(TokenKind::Ident) {
            p.bump(); // the constraint / register class
        } else {
            p.arm_error(p.here(), "expected a register-class constraint");
            p.missing();
        }
        p.expect_punct(Punct::RParen, "`)` to close the constraint");
    } else {
        // One report per broken operand; a hopeless one (no direction
        // either) syncs to the next operand.
        if dir_ok {
            p.arm_error(p.here(), "expected `(constraint)` after the asm direction");
        }
        p.missing();
        if !dir_ok {
            p.recover_until(true, |k| {
                k == TokenKind::Punct(Punct::Comma) || k == TokenKind::Term
            });
            o.complete(p, SyntaxKind::AsmOperand);
            return;
        }
    }
    if expr_bp(p, 0, ctx.inner()).is_none() {
        p.arm_error(p.here(), "expected the asm operand expression");
        p.missing();
        p.recover_until(true, |k| {
            k == TokenKind::Punct(Punct::Comma) || k == TokenKind::Term
        });
    }
    o.complete(p, SyntaxKind::AsmOperand);
}

/// `'borrow' expr 'from' expr` — `from` is contextual.
fn borrow_expr(p: &mut Parser<'_>, ctx: Ctx) -> CompletedMarker {
    let m = p.start();
    p.bump(); // borrow
    expr_required_ctx(p, ctx.inner());
    if p.at(TokenKind::Ident) && p.current_text() == b"from" {
        p.bump();
    } else {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `from` in the borrow expression",
        );
        p.missing();
    }
    expr_required_ctx(p, ctx.inner());
    m.complete(p, SyntaxKind::BorrowExpr)
}
