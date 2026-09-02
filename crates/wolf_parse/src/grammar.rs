//! The declaration grammar (spec/01 §2, §4, §5) — resilient recursive
//! descent over the event-driven skeleton. Bodies and initializers are
//! parsed by the expression/statement grammar in [`crate::exprs`] (s09);
//! expression-shaped `[…]` type arguments in *type* position still park
//! as `TypeArgPending` for sema (D29).
//!
//! One-token lookahead is the norm; every bounded-lookahead use lives
//! behind a named helper with a comment saying why.

use crate::codes;
use crate::parser::{Marker, Parser, is_decl_keyword};
use wolf_ast::SyntaxKind;
use wolf_lex::{Keyword, Punct, TokenKind};
use wolf_span::Span;

// ------------------------------------------------------------ the unit --

pub(crate) fn source_file(p: &mut Parser<'_>) {
    let m = p.start();
    // The file header: `#![…]` file-wide attributes are legal only
    // here, before the first declaration (`[gram.attr.index]`).
    loop {
        crate::exprs::eat_terms_checked(p);
        if p.at(TokenKind::PoundBangBracket) {
            inner_attribute(p, false);
        } else {
            break;
        }
    }
    loop {
        crate::exprs::eat_terms_checked(p);
        if p.at_eof() {
            break;
        }
        if p.at(TokenKind::PoundBangBracket) {
            // Past the header: parsed for recovery, refused by position.
            inner_attribute(p, true);
            continue;
        }
        let before = p.pos();
        item(p, false);
        // The item parser always makes progress on non-Eof input; this
        // guard turns a violation into a visible stall-breaker instead
        // of an infinite loop.
        if p.pos() == before && !p.at_eof() {
            let e = p.start();
            p.bump();
            e.complete(p, SyntaxKind::ErrorNode);
        }
    }
    p.bump_eof();
    m.complete(p, SyntaxKind::SourceFile);
}

// ---------------------------------------------------------------- items --

/// One item: `attribute* visibility? bare_item`. `in_body` is true when
/// parsing members of a `trait`/`impl` body, where a `}` at depth zero
/// closes the enclosing item and must stop recovery.
/// Parse one declaration, with every recovery inside it floored at the
/// column the declaration starts in.
///
/// A `{` shields recovery because nested items are legal in braces; a
/// `{` that is never closed shields it all the way to `Eof`, and the
/// skip then walks over the NEXT declaration and swallows it whole —
/// no re-parenting, no nodes, the file silently loses its shape. The
/// floor says what indentation already says: a declaration starting at
/// or left of this column is a sibling, and recovery stops before it.
pub(crate) fn item(p: &mut Parser<'_>, in_body: bool) {
    let saved = p.item_floor;
    p.item_floor = Some(p.line_indent(p.current_span().lo));
    item_inner(p, in_body);
    p.item_floor = saved;
}

fn item_inner(p: &mut Parser<'_>, in_body: bool) {
    // An unambiguous *item* keyword ends any stray-line E0203 fold.
    // `let`/`var`/`const` deliberately do not: a function body spilled
    // to the top level (its `{` lost) interleaves bindings with
    // expression statements — that is one wreck, not many.
    if matches!(
        p.current(),
        TokenKind::PoundBracket
            | TokenKind::Kw(
                Keyword::Type
                    | Keyword::Struct
                    | Keyword::Enum
                    | Keyword::Trait
                    | Keyword::Impl
                    | Keyword::Use
                    | Keyword::Import
                    | Keyword::Extern
                    | Keyword::Export
                    | Keyword::Comptime
                    | Keyword::Pub
            )
    ) || (p.at_kw(Keyword::Fn) && p.nth(1) != TokenKind::Punct(Punct::LParen))
    {
        p.toplevel_error_reported = false;
    }
    let m = p.start();
    let mut prefixed = false;
    while p.at(TokenKind::PoundBracket) {
        attribute(p);
        prefixed = true;
    }
    if p.at_kw(Keyword::Pub) {
        visibility(p);
        prefixed = true;
    }
    match p.current() {
        // Bounded lookahead: `fn (` is a closure — a stray expression
        // line up here, not a nameless function item.
        TokenKind::Kw(Keyword::Fn) if p.nth(1) != TokenKind::Punct(Punct::LParen) => fn_item(p, m),
        TokenKind::Kw(Keyword::Comptime | Keyword::Extern | Keyword::Export) => fn_item(p, m),
        TokenKind::Kw(Keyword::Let) => binding_item(p, m, Keyword::Let, SyntaxKind::LetDecl),
        TokenKind::Kw(Keyword::Var) => binding_item(p, m, Keyword::Var, SyntaxKind::VarDecl),
        TokenKind::Kw(Keyword::Const) => binding_item(p, m, Keyword::Const, SyntaxKind::ConstDecl),
        TokenKind::Kw(Keyword::Type) => type_item(p, m),
        TokenKind::Kw(Keyword::Struct) => struct_item(p, m),
        TokenKind::Kw(Keyword::Enum) => enum_item(p, m),
        TokenKind::Kw(Keyword::Trait) => trait_item(p, m),
        TokenKind::Kw(Keyword::Impl) => impl_item(p, m),
        TokenKind::Kw(Keyword::Use) => use_item(p, m),
        TokenKind::Kw(Keyword::Import) => import_c_item(p, m),
        _ => {
            // The typo machinery (s10): an identifier in declaration
            // position within edit distance of a declaration keyword
            // gets "did you mean" plus a machine-applicable edit.
            if p.at(TokenKind::Ident)
                && let Some(d) = decl_keyword_typo(p)
            {
                p.toplevel_diag(d);
                p.skip_until(in_body, |k| k == TokenKind::Term);
                m.complete(p, SyntaxKind::ErrorNode);
                return;
            }
            let msg = if prefixed {
                "expected a declaration after the attributes"
            } else {
                "expected a declaration here"
            };
            p.toplevel_diag(
                wolf_diag::Diagnostic::error(codes::UNEXPECTED_TOPLEVEL, p.current_span(), msg)
                    .with_note(
                        "the top level of a file holds declarations only, each starting \
                         with its keyword: `fn`, `let`, `var`, `type`, `struct`, `enum`, \
                         `trait`, `impl`, `use`, `import`. If this line belongs inside a \
                         function, a `{` is probably missing above it.",
                    ),
            );
            if p.at_punct(Punct::LBrace) {
                // A stray block: parse it — declarations nested inside
                // survive intact instead of being skipped raw (D22).
                crate::exprs::block(p);
            } else {
                p.skip_until(in_body, |k| k == TokenKind::Term);
            }
            m.complete(p, SyntaxKind::ErrorNode);
        }
    }
}

/// The declaration-leading keywords, as candidate strings for the typo
/// suggester (same set as [`is_decl_keyword`]).
const DECL_KEYWORD_TEXTS: &[&str] = &[
    "fn", "let", "var", "type", "struct", "enum", "trait", "impl", "use", "import", "const",
    "extern", "export", "comptime", "pub",
];

/// `fnn` → "did you mean `fn`?" with the machine-applicable edit
/// (VOICE.md rule 4). `None` when nothing is close enough.
fn decl_keyword_typo(p: &Parser<'_>) -> Option<wolf_diag::Diagnostic> {
    let text = std::str::from_utf8(p.current_text()).ok()?;
    let kw = wolf_diag::suggest::best_match(text, DECL_KEYWORD_TEXTS)?;
    let span = p.current_span();
    Some(
        wolf_diag::Diagnostic::error(
            codes::UNEXPECTED_TOPLEVEL,
            span,
            format!("`{text}` is not how a declaration starts — did you mean `{kw}`?"),
        )
        .with_suggestion(wolf_diag::Suggestion::new(
            format!("replace `{text}` with `{kw}`"),
            vec![(span, kw.to_string())],
            wolf_diag::Applicability::MachineApplicable,
        )),
    )
}

// ------------------------------------------------------------ functions --

pub(crate) fn fn_item(p: &mut Parser<'_>, m: Marker) {
    // fn_qual* : 'comptime' | 'extern' STRING | 'export'
    loop {
        match p.current() {
            TokenKind::Kw(Keyword::Comptime | Keyword::Export) => p.bump(),
            TokenKind::Kw(Keyword::Extern) => {
                p.bump();
                if p.at_str_begin() {
                    string_lit(p);
                } else {
                    p.error(
                        codes::EXPECTED_TOKEN,
                        p.here(),
                        "expected a quoted ABI string after `extern`, like `extern \"c\"`",
                    );
                    p.missing();
                }
            }
            _ => break,
        }
    }
    let fn_ok = p.at_kw(Keyword::Fn);
    p.expect_kw(Keyword::Fn, "`fn`");
    let name_ok = matches!(p.current(), TokenKind::Ident)
        || matches!(p.current(), TokenKind::Kw(k) if !is_decl_keyword(k));
    if !fn_ok && !name_ok {
        // Hopeless header (no `fn`, no name): one report — sync to the
        // body block (parsed, keeping its contents) or end of line.
        p.missing();
        p.recover_until(true, |k| {
            matches!(k, TokenKind::Punct(Punct::LBrace)) || k == TokenKind::Term
        });
        if p.at_punct(Punct::LBrace) {
            crate::exprs::block(p);
        } else if p.at(TokenKind::Term) {
            p.bump();
        }
        m.complete(p, SyntaxKind::FnDecl);
        return;
    }
    let named = name_token(p, "function");
    if p.at_punct(Punct::LBracket) {
        generic_param_list(p);
    }
    let params_ok = if p.at_punct(Punct::LParen) {
        param_list(p)
    } else {
        // One report per broken header: a missing name was already
        // diagnosed above.
        if named {
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                "expected `(` to start the parameter list",
            );
        }
        p.missing();
        false
    };
    if p.at_punct(Punct::Arrow) {
        ret_type(p);
    }
    // Body: a block, or TERM for the bodyless form.
    if p.at_punct(Punct::LBrace) {
        crate::exprs::block(p);
    } else if p.at(TokenKind::Term) {
        p.bump();
    } else if !p.at_eof() && !p.at_decl_start() && !p.at_punct(Punct::RBrace) {
        // One report per broken header: a missing parameter list or
        // return type was already diagnosed above.
        if params_ok {
            p.error_unless_folded(
                codes::EXPECTED_TOKEN,
                p.here(),
                "expected `{` or line end after the function header",
            );
        }
        p.recover_until(true, |k| {
            matches!(k, TokenKind::Punct(Punct::LBrace) | TokenKind::Term)
        });
        if p.at_punct(Punct::LBrace) {
            crate::exprs::block(p);
        } else if p.at(TokenKind::Term) {
            p.bump();
        }
    } else if params_ok && !p.at_eof() {
        // Header was fine but the body is missing before the next
        // declaration (same line, no TERM). Folded when an unclosed
        // delimiter was just reported here — same wreck.
        p.error_unless_folded(codes::EXPECTED_TOKEN, p.here(), "expected a function body");
        p.missing();
    }
    m.complete(p, SyntaxKind::FnDecl);
}

/// `(param, …)`. Returns whether the closing `)` was found.
fn param_list(p: &mut Parser<'_>) -> bool {
    let m = p.start();
    let opener = p.current_span();
    p.bump(); // `(`
    let mut closed = true;
    loop {
        p.eat_terms();
        if p.at_punct(Punct::RParen) {
            p.bump();
            break;
        }
        if paren_list_escape(p) {
            unclosed(p, opener, "(");
            closed = false;
            break;
        }
        let before = p.pos();
        let diags_before = p.diags.len();
        param(p);
        if p.diags.len() > diags_before {
            // The broken parameter was reported; the separator miss
            // that follows is the same wreck.
            p.arg_error_reported = true;
        }
        if p.at_punct(Punct::Comma) {
            p.bump();
            continue;
        }
        if p.at_punct(Punct::RParen) || paren_list_escape(p) {
            continue;
        }
        p.arg_list_error(p.here(), "expected `,` or `)` in the parameter list");
        p.recover_until(true, |k| {
            matches!(k, TokenKind::Punct(Punct::Comma | Punct::RParen))
        });
        if p.at_punct(Punct::Comma) {
            p.bump();
        }
        if p.pos() == before {
            // No progress: bail out rather than loop.
            unclosed(p, opener, "(");
            closed = false;
            break;
        }
    }
    m.complete(p, SyntaxKind::ParamList);
    closed
}

/// Would continuing this paren-delimited list eat the next declaration
/// or the function body? (The unclosed-`(` escape hatch.)
fn paren_list_escape(p: &Parser<'_>) -> bool {
    p.at_eof()
        || p.at_decl_start()
        || p.at_punct(Punct::LBrace)
        || p.at_punct(Punct::RBrace)
        || p.at_punct(Punct::Arrow)
}

fn param(p: &mut Parser<'_>) {
    let m = p.start();
    if matches!(p.current(), TokenKind::Kw(Keyword::Mut | Keyword::Take)) {
        p.bump();
    }
    match p.current() {
        TokenKind::Ident if p.current_text() == b"self" => {
            // Contextual receiver: reclassified, text unchanged.
            p.bump_as(SyntaxKind::SelfKw);
            if at_view_set(p) {
                view_set(p);
            }
        }
        TokenKind::Ident => {
            p.bump();
            param_type(p);
        }
        TokenKind::Underscore => {
            p.error(
                codes::EXPECTED_TOKEN,
                p.current_span(),
                "`_` is not a parameter name — give this parameter a real name",
            );
            p.bump();
            param_type(p);
        }
        TokenKind::Kw(k) if !is_decl_keyword(k) => {
            keyword_as_ident(p, "parameter");
            param_type(p);
        }
        _ => {
            p.arg_list_error(p.here(), "expected a parameter");
            p.recover_until(true, |k| {
                matches!(
                    k,
                    TokenKind::Punct(Punct::Comma | Punct::RParen | Punct::Arrow)
                )
            });
        }
    }
    m.complete(p, SyntaxKind::Param);
}

fn param_type(p: &mut Parser<'_>) {
    if p.at_punct(Punct::Colon) {
        p.bump();
        type_required(p);
    } else {
        // One report for the missing ascription; still salvage a type
        // that starts here anyway (`fn f(x int)`).
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `:` and a type after the parameter name",
        );
        p.missing();
        type_(p);
    }
}

/// Bounded lookahead: a view set is `self` `.` `{` — the `.` alone is
/// not enough to commit (a stray `.` must stay an error, and one-token
/// lookahead sees only the `.`), so we peek one further for the `{`.
fn at_view_set(p: &Parser<'_>) -> bool {
    p.at_punct(Punct::Dot) && p.nth(1) == TokenKind::Punct(Punct::LBrace)
}

/// `.{a, b}` — field-granular exclusivity on a receiver.
fn view_set(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(); // `.`
    let opener = p.current_span();
    p.bump(); // `{`
    loop {
        p.eat_terms();
        if p.at_punct(Punct::RBrace) {
            p.bump();
            break;
        }
        if p.at_eof() || p.at_decl_start() || p.at_punct(Punct::RParen) {
            unclosed(p, opener, "{");
            break;
        }
        if p.at(TokenKind::Ident) {
            p.bump();
        } else if p.at_punct(Punct::Comma) {
            // handled below
        } else {
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                "expected a field name in the view set",
            );
            if !p.recover_until(true, |k| {
                matches!(k, TokenKind::Punct(Punct::Comma | Punct::RParen))
            }) {
                break;
            }
        }
        if p.at_punct(Punct::Comma) {
            p.bump();
        }
    }
    m.complete(p, SyntaxKind::ViewSet);
}

/// `[T, U: Bound + Bound, N: type]`.
fn generic_param_list(p: &mut Parser<'_>) {
    let m = p.start();
    let opener = p.current_span();
    p.bump(); // `[`
    loop {
        if p.at_punct(Punct::RBracket) {
            p.bump();
            break;
        }
        if p.at_eof()
            || p.at_decl_start()
            || matches!(
                p.current(),
                TokenKind::Punct(Punct::LParen | Punct::LBrace | Punct::RBrace | Punct::Arrow)
            )
        {
            unclosed(p, opener, "[");
            break;
        }
        let before = p.pos();
        match p.current() {
            TokenKind::Ident => generic_param(p),
            TokenKind::Kw(k) if !is_decl_keyword(k) => generic_param(p),
            _ => {
                p.error(
                    codes::MALFORMED_GENERICS,
                    p.current_span(),
                    "expected a generic parameter name here, like `T` or `N: type`",
                );
                p.recover_until(true, |k| {
                    matches!(k, TokenKind::Punct(Punct::Comma | Punct::RBracket))
                });
            }
        }
        if p.at_punct(Punct::Comma) {
            p.bump();
        }
        if p.pos() == before {
            unclosed(p, opener, "[");
            break;
        }
    }
    m.complete(p, SyntaxKind::GenericParamList);
}

fn generic_param(p: &mut Parser<'_>) {
    let m = p.start();
    name_token(p, "generic parameter");
    if p.at_punct(Punct::Colon) {
        p.bump();
        if p.at_kw(Keyword::Type) {
            // `N: type` — comptime type parameter.
            p.bump();
        } else if p.at(TokenKind::Ident) {
            let b = p.start();
            loop {
                path(p, "bound");
                if p.at_punct(Punct::Plus) {
                    p.bump();
                } else {
                    break;
                }
            }
            b.complete(p, SyntaxKind::TypeBound);
        } else {
            p.error(
                codes::MALFORMED_GENERICS,
                p.here(),
                "expected a trait bound or `type` after the `:`",
            );
            p.missing();
        }
    }
    m.complete(p, SyntaxKind::GenericParam);
}

/// `-> type ('!' error_row)?`.
fn ret_type(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(); // `->`
    let diags_before = p.diags.len();
    type_required_no_row(p);
    if p.diags.len() > diags_before {
        // The broken return type is this header's one report.
        p.fold_line_end();
    }
    while p.at_punct(Punct::Not) {
        p.bump();
        if p.at_punct(Punct::LBrace) {
            // Right-recursive (#34): every further `! {row}` is
            // another row child of this RetType — the header's flat
            // tree shape holds, and sema refuses the nested meaning
            // by name until the spec rules it.
            error_row(p);
        } else {
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                "expected `{` to open the error row after `!`",
            );
            p.missing();
            // One report per broken header tail.
            p.fold_line_end();
            break;
        }
    }
    m.complete(p, SyntaxKind::RetType);
}

/// `{ path(payload)?, …, ..? }` — the explicit error row (D30).
fn error_row(p: &mut Parser<'_>) {
    let m = p.start();
    let opener = p.current_span();
    p.bump(); // `{`
    loop {
        p.eat_terms();
        if p.at_punct(Punct::RBrace) {
            p.bump();
            break;
        }
        // Row entries are `path (types)` lists — a keyword or `{` here
        // means the row was never closed and this token belongs to the
        // function body. Escape rather than cascade (D22).
        if p.at_eof()
            || p.at_decl_start()
            || p.at_punct(Punct::LBrace)
            || matches!(p.current(), TokenKind::Kw(_))
        {
            unclosed(p, opener, "{");
            break;
        }
        if p.at_punct(Punct::DotDot) {
            // `..` — the open-row marker.
            p.bump();
            if p.at_punct(Punct::Comma) {
                p.bump();
            }
            continue;
        }
        let before = p.pos();
        match p.current() {
            TokenKind::Ident => {
                let e = p.start();
                path(p, "error tag");
                let before_payload = p.diag_count();
                if p.at_punct(Punct::LParen) {
                    paren_type_list(p);
                }
                e.complete(p, SyntaxKind::RowEntry);
                // `tag(types)` is also the shape of a call statement, so
                // the escape above cannot see that this `{` opened a
                // function body: `print("before")` is a perfectly good
                // row entry until its payload turns out to be a string
                // rather than a type. That failure is the signal. One
                // report, then escape (D22) — parsing on read a whole
                // body as a row and charged a diagnostic per statement.
                if p.diag_count() > before_payload {
                    unclosed(p, opener, "{");
                    break;
                }
            }
            _ => {
                p.arg_list_error(p.current_span(), "expected an error-row entry");
                p.recover_until(true, |k| {
                    matches!(k, TokenKind::Punct(Punct::Comma) | TokenKind::Term)
                });
            }
        }
        if p.at_punct(Punct::Comma) {
            p.bump();
        } else if !p.at_punct(Punct::RBrace) && p.at(TokenKind::Ident) {
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                "expected `,` between error-row entries",
            );
        }
        if p.pos() == before {
            unclosed(p, opener, "{");
            break;
        }
    }
    m.complete(p, SyntaxKind::ErrorRow);
}

// ------------------------------------------------------------- bindings --

/// The shape one binder of a `let`/`var` group parsed with — the
/// group-end report (D63's two teach-notes) is decided over the whole
/// sequence, so each binder records what it saw.
struct BinderShape {
    /// A pattern was parsed (a missing one already got its report).
    ok: bool,
    has_init: bool,
    /// The "pattern" was a bare literal (`2` in `let a, b = 1, 2`) —
    /// the bare-tuple tell.
    lit_pat: bool,
    /// The truncation was already reported inline (the single-binder
    /// recovery path ran).
    reported: bool,
    /// Where the `=` should have been (zero-width).
    eq_site: Span,
}

/// One `pattern (':' type)? ('=' expr)?` of a `let`/`var`. A missing
/// `=` before a `,` — and, inside a group, before the statement's end
/// — is *deferred* to the caller: the right report depends on the
/// whole group (D63). The zero-width `Missing` placeholder is still
/// inserted here so the tree keeps its shape. Every other truncation
/// keeps the single-binder recovery unchanged.
fn binder(p: &mut Parser<'_>, in_group: bool) -> BinderShape {
    let mut shape = BinderShape {
        ok: true,
        has_init: false,
        lit_pat: false,
        reported: false,
        eq_site: p.here(),
    };
    match pattern(p) {
        Some(cm) => shape.lit_pat = cm.kind() == SyntaxKind::LiteralPat,
        None => {
            p.error(codes::EXPECTED_PATTERN, p.here(), "expected a pattern");
            p.missing();
            shape.ok = false;
            shape.reported = true;
        }
    }
    if p.at_punct(Punct::Colon) {
        p.bump();
        type_required(p);
    }
    if p.at_punct(Punct::Eq) {
        p.bump();
        crate::exprs::expr_required(p);
        shape.has_init = true;
        return shape;
    }
    shape.eq_site = p.here();
    let at_stmt_end =
        p.at(TokenKind::Term) || p.at_eof() || p.at_decl_start() || p.at_punct(Punct::RBrace);
    if p.at_punct(Punct::Comma) {
        // A group separator: this binder ends valueless; the caller
        // owns the report.
        p.missing();
    } else if in_group && at_stmt_end {
        // The group's last binder is valueless (`var i, c`): defer,
        // and fold the follow-up missing-terminator diagnostic.
        p.missing();
        p.fold_line_end();
    } else {
        // One report per truncated binding: a missing binder was
        // already diagnosed above.
        if shape.ok {
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                "this binding has no value — expected `=` and an initializer",
            );
        }
        shape.reported = true;
        p.missing();
        // ...and fold the follow-up missing-terminator diagnostic.
        p.fold_line_end();
        if !at_stmt_end {
            p.recover_until(true, |k| {
                k == TokenKind::Term || k == TokenKind::Punct(Punct::Eq)
            });
            if p.at_punct(Punct::Eq) {
                p.bump();
                crate::exprs::expr_required(p);
                shape.has_init = true;
            }
        }
    }
    shape
}

/// The group-end report for deferred valueless binders (D63's refusal
/// ladder): the bare tuple by name, one-initializer-for-several-names
/// with both spellings, or the production's plain letter.
fn group_report(p: &mut Parser<'_>, kw: Keyword, shapes: &[BinderShape]) {
    let kw = if kw == Keyword::Var { "var" } else { "let" };
    let deferred: Vec<usize> = shapes
        .iter()
        .enumerate()
        .filter(|(_, s)| s.ok && !s.has_init && !s.reported)
        .map(|(i, _)| i)
        .collect();
    if deferred.is_empty() {
        return;
    }
    let Some(first_init) = shapes.iter().position(|s| s.has_init) else {
        // `var i, c` — no initializer anywhere: the production's
        // letter, unchanged. One report per valueless binder.
        for &i in &deferred {
            p.error(
                codes::EXPECTED_TOKEN,
                shapes[i].eq_site,
                "this binding has no value — expected `=` and an initializer",
            );
        }
        return;
    };
    if let Some(&after) = deferred.iter().find(|&&i| i > first_init) {
        // `let a, b = 1, 2` — the bare tuple, refused by name. One
        // report covers the whole shape; its note carries both fixes,
        // so the leading valueless binders stay unreported.
        p.push_diag(
            wolf_diag::Diagnostic::error(
                codes::EXPECTED_TOKEN,
                shapes[after].eq_site,
                format!("this value has no name — `{kw} a, b = 1, 2` (the bare tuple) is not wolf"),
            )
            .with_note(format!(
                "a comma groups complete bindings, each with its own `=`: \
                 `{kw} a = 1, b = 2`. To unpack one value into several names, \
                 destructure with parens on both sides: `{kw} (a, b) = (1, 2)`.",
            )),
        );
        return;
    }
    // `var i, c = 0` — one initializer cannot serve several names.
    for &i in &deferred {
        p.push_diag(
            wolf_diag::Diagnostic::error(
                codes::EXPECTED_TOKEN,
                shapes[i].eq_site,
                "this binding has no value — expected `=` and an initializer",
            )
            .with_note(format!(
                "one initializer cannot serve several names. Destructure — \
                 `{kw} (i, c) = (0, 0)` — or give each name its own value: \
                 `{kw} i = 0, c = 0`.",
            )),
        );
    }
}

pub(crate) fn binding_item(p: &mut Parser<'_>, m: Marker, kw: Keyword, kind: SyntaxKind) {
    p.bump(); // let/var/const
    if kw == Keyword::Const {
        // `const` binds one name — no comma group (D63 is let/var).
        let binder_ok = name_token(p, "constant");
        if p.at_punct(Punct::Colon) {
            p.bump();
            type_required(p);
        }
        if p.at_punct(Punct::Eq) {
            p.bump();
            crate::exprs::expr_required(p);
        } else {
            // One report per truncated binding: a missing binder was
            // already diagnosed above.
            if binder_ok {
                p.error(
                    codes::EXPECTED_TOKEN,
                    p.here(),
                    "this binding has no value — expected `=` and an initializer",
                );
            }
            p.missing();
            // ...and fold the follow-up missing-terminator diagnostic.
            p.fold_line_end();
            if !p.at(TokenKind::Term)
                && !p.at_eof()
                && !p.at_decl_start()
                && !p.at_punct(Punct::RBrace)
            {
                p.recover_until(true, |k| {
                    k == TokenKind::Term || k == TokenKind::Punct(Punct::Eq)
                });
                if p.at_punct(Punct::Eq) {
                    p.bump();
                    crate::exprs::expr_required(p);
                }
            }
        }
    } else {
        // `let`/`var`: binder (',' binder)* — D63. A single binder
        // keeps the flat shape; a comma wraps every binder in its own
        // `Binder` node.
        let first_m = p.start();
        let first = binder(p, false);
        if p.at_punct(Punct::Comma) {
            first_m.complete(p, SyntaxKind::Binder);
            let mut shapes = vec![first];
            while p.at_punct(Punct::Comma) {
                p.bump();
                let bm = p.start();
                shapes.push(binder(p, true));
                bm.complete(p, SyntaxKind::Binder);
            }
            group_report(p, kw, &shapes);
        } else {
            first_m.abandon(p);
        }
    }
    if p.at(TokenKind::Term) {
        p.bump();
    } else if !p.at_eof() && !p.at_punct(Punct::RBrace) {
        // The initializer expression stops only at TERM / `}` / EOF or
        // an unconsumable token — the latter means junk on this line.
        p.expected_line_end("initializer");
        p.recover_until(true, |k| k == TokenKind::Term);
        if p.at(TokenKind::Term) {
            p.bump();
        }
    }
    m.complete(p, kind);
}

// ----------------------------------------------------------- type items --

/// After an item keyword, is the header hopeless — no name, no
/// structural anchor (`[`, `{`, `=`)? Then the keyword itself most
/// likely strayed here (a mutation, a half-deleted line): report once
/// through the stray-line fold and move on, instead of a cascade of
/// name/`=`/body diagnostics.
fn hopeless_item_header(p: &Parser<'_>) -> bool {
    let name_ok = match p.current() {
        TokenKind::Ident => true,
        TokenKind::Kw(k) => !is_decl_keyword(k),
        _ => false,
    };
    !name_ok
        && !matches!(
            p.current(),
            TokenKind::Punct(Punct::LBracket | Punct::LBrace | Punct::Eq)
        )
}

pub(crate) fn type_item(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // `type`
    // A `{` is recoverable for `struct`/`enum`/`trait` — a brace block
    // IS their body, so only the name is missing. A type alias has no
    // brace form (`type N = T`), so `type {` is hopeless, and parsing on
    // reported it three times at the same `{`: the missing name, then
    // the missing type after the absent `=`. Report once instead.
    if hopeless_item_header(p) || p.at_punct(Punct::LBrace) {
        p.toplevel_error(p.here(), "expected a type name");
        p.missing();
        m.complete(p, SyntaxKind::ErrorNode);
        return;
    }
    name_token(p, "type");
    if p.at_punct(Punct::LBracket) {
        generic_param_list(p);
    }
    p.expect_punct(Punct::Eq, "`=` in the type declaration");
    match p.current() {
        TokenKind::Kw(Keyword::Struct) => {
            let d = p.start();
            p.bump();
            struct_body(p);
            d.complete(p, SyntaxKind::StructDef);
        }
        TokenKind::Kw(Keyword::Enum) => {
            let d = p.start();
            p.bump();
            enum_body(p);
            d.complete(p, SyntaxKind::EnumDef);
        }
        _ => type_required(p),
    }
    if p.at(TokenKind::Term) {
        p.bump(); // TERM? — optional per [gram.item.type]
    }
    m.complete(p, SyntaxKind::TypeDecl);
}

pub(crate) fn struct_item(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // `struct`
    if hopeless_item_header(p) {
        p.toplevel_error(p.here(), "expected a struct name");
        p.missing();
        m.complete(p, SyntaxKind::ErrorNode);
        return;
    }
    name_token(p, "struct");
    if p.at_punct(Punct::LBracket) {
        generic_param_list(p);
    }
    struct_body(p);
    m.complete(p, SyntaxKind::StructDecl);
}

pub(crate) fn enum_item(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // `enum`
    if hopeless_item_header(p) {
        p.toplevel_error(p.here(), "expected an enum name");
        p.missing();
        m.complete(p, SyntaxKind::ErrorNode);
        return;
    }
    name_token(p, "enum");
    if p.at_punct(Punct::LBracket) {
        generic_param_list(p);
    }
    enum_body(p);
    m.complete(p, SyntaxKind::EnumDecl);
}

/// Would continuing this brace-delimited body eat the next declaration?
/// Attributes and `pub` are excluded — they can begin a field.
fn body_escape(p: &Parser<'_>) -> bool {
    match p.current() {
        TokenKind::Eof => true,
        TokenKind::Kw(k) => is_decl_keyword(k) && k != Keyword::Pub,
        _ => false,
    }
}

fn struct_body(p: &mut Parser<'_>) {
    if !p.at_punct(Punct::LBrace) {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `{` to open the struct body",
        );
        p.missing();
        return;
    }
    let opener = p.current_span();
    p.bump();
    loop {
        p.eat_terms();
        if p.at_punct(Punct::RBrace) {
            p.bump();
            break;
        }
        if body_escape(p) {
            unclosed(p, opener, "{");
            break;
        }
        let before = p.pos();
        let diags_before = p.diags.len();
        match p.current() {
            TokenKind::Ident | TokenKind::PoundBracket | TokenKind::Kw(_) => {
                // Speculative: if the "field" turns out to be the next
                // declaration (its attributes/`pub` prefix followed by
                // a declaration keyword), rewind and escape — the
                // prefix belongs to that declaration, not to a field.
                let cp = p.checkpoint();
                if !struct_field(p) {
                    p.rollback(cp);
                    unclosed(p, opener, "{");
                    break;
                }
            }
            _ => {
                p.arm_error(p.current_span(), "expected a field");
                p.recover_until(true, |k| {
                    matches!(k, TokenKind::Punct(Punct::Comma) | TokenKind::Term)
                });
                if p.at_punct(Punct::Comma) {
                    p.bump();
                }
            }
        }
        if p.diags.len() == diags_before {
            p.arm_error_reported = false;
        }
        if p.pos() == before {
            unclosed(p, opener, "{");
            break;
        }
    }
}

/// `attribute* visibility? IDENT ':' type ','?`. Returns false —
/// emitting nothing the caller keeps — when the name position holds a
/// declaration keyword: the consumed prefix belongs to the *next*
/// declaration and the caller rewinds.
fn struct_field(p: &mut Parser<'_>) -> bool {
    let m = p.start();
    while p.at(TokenKind::PoundBracket) {
        attribute(p);
    }
    if p.at_kw(Keyword::Pub) {
        visibility(p);
    }
    let name_ok = match p.current() {
        TokenKind::Ident => true,
        TokenKind::Kw(k) => !is_decl_keyword(k),
        _ => false,
    };
    if !name_ok && p.at_decl_start() {
        m.abandon(p);
        return false;
    }
    if name_ok {
        name_token(p, "field");
    } else {
        p.arm_error(p.here(), "expected a field name");
        p.missing();
    }
    if p.at_punct(Punct::Colon) {
        p.bump();
        type_required(p);
    } else {
        // One report per broken field; salvage a plain path type that
        // starts here anyway (`x f64`), and sync to the next field
        // otherwise. Keyword-led types are not salvaged — a stray `fn`
        // here is the next declaration, not a field type.
        if name_ok {
            p.arm_error(p.here(), "expected `:` and a type after the field name");
        }
        p.missing();
        if !(name_ok && p.at(TokenKind::Ident) && type_(p)) {
            p.recover_until(true, |k| {
                matches!(k, TokenKind::Punct(Punct::Comma) | TokenKind::Term)
            });
        }
    }
    if p.at_punct(Punct::Comma) {
        p.bump();
    }
    m.complete(p, SyntaxKind::StructField);
    true
}

fn enum_body(p: &mut Parser<'_>) {
    if !p.at_punct(Punct::LBrace) {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `{` to open the enum body",
        );
        p.missing();
        return;
    }
    let opener = p.current_span();
    p.bump();
    loop {
        p.eat_terms();
        if p.at_punct(Punct::RBrace) {
            p.bump();
            break;
        }
        if body_escape(p) {
            unclosed(p, opener, "{");
            break;
        }
        let before = p.pos();
        let diags_before = p.diags.len();
        match p.current() {
            TokenKind::Ident => enum_variant(p),
            TokenKind::Kw(k) if !is_decl_keyword(k) => enum_variant(p),
            _ => {
                p.arm_error(p.current_span(), "expected an enum variant");
                p.recover_until(true, |k| {
                    matches!(k, TokenKind::Punct(Punct::Comma) | TokenKind::Term)
                });
            }
        }
        if p.diags.len() == diags_before {
            p.arm_error_reported = false;
        }
        if p.at_punct(Punct::Comma) {
            p.bump();
        } else if !p.at_punct(Punct::RBrace) && p.at(TokenKind::Ident) {
            // Commas between variants are required by [gram.item.type]
            // (newlines do not substitute); keep parsing after saying so.
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                "expected `,` between enum variants",
            );
        }
        if p.pos() == before {
            unclosed(p, opener, "{");
            break;
        }
    }
}

/// `IDENT ('(' type (',' type)* ')')?`.
fn enum_variant(p: &mut Parser<'_>) {
    let m = p.start();
    name_token(p, "variant");
    if p.at_punct(Punct::LParen) {
        paren_type_list(p);
    }
    m.complete(p, SyntaxKind::EnumVariant);
}

// ---------------------------------------------------------- trait/impl --

pub(crate) fn trait_item(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // `trait`
    if hopeless_item_header(p) {
        p.toplevel_error(p.here(), "expected a trait name");
        p.missing();
        m.complete(p, SyntaxKind::ErrorNode);
        return;
    }
    name_token(p, "trait");
    if p.at_punct(Punct::LBracket) {
        generic_param_list(p);
    }
    member_body(p);
    m.complete(p, SyntaxKind::TraitDecl);
}

pub(crate) fn impl_item(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // `impl`
    if p.at_punct(Punct::LBracket) {
        generic_param_list(p);
    }
    if p.at(TokenKind::Ident) {
        path(p, "implemented name");
        // [gram.item.trait] gives the impl subject as a bare `path`;
        // generic subjects (`impl[T] List[T]`) need the bracket
        // application, accepted here leniently (spec issue noted in the
        // sprint report).
        if p.at_punct(Punct::LBracket) {
            type_args(p);
        }
    } else {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected the name of the trait or type this `impl` is for",
        );
        p.missing();
    }
    if p.at_kw(Keyword::For) {
        p.bump();
        type_required(p);
    }
    member_body(p);
    m.complete(p, SyntaxKind::ImplDecl);
}

/// `{ item* }` — members parsed re-entrantly by the item parser.
fn member_body(p: &mut Parser<'_>) {
    if !p.at_punct(Punct::LBrace) {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `{` to open the body",
        );
        p.missing();
        return;
    }
    let opener = p.current_span();
    p.bump();
    loop {
        p.eat_terms();
        if p.at_punct(Punct::RBrace) {
            p.bump();
            break;
        }
        if p.at_eof() {
            unclosed(p, opener, "{");
            break;
        }
        let before = p.pos();
        if p.at_decl_start() {
            item(p, true);
        } else {
            p.toplevel_error(p.current_span(), "expected a declaration");
            p.recover_until(true, |k| k == TokenKind::Term);
        }
        if p.pos() == before {
            unclosed(p, opener, "{");
            break;
        }
    }
}

// -------------------------------------------------------- use / import --

pub(crate) fn use_item(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // `use`
    use_path(p);
    if at_use_group(p) {
        p.bump(); // `.`
        use_group(p);
    }
    if p.at_kw(Keyword::As) {
        p.bump();
        name_token(p, "import alias");
    }
    item_term(p, "`use` item");
    m.complete(p, SyntaxKind::UseDecl);
}

/// The `use` path: like [`path`], but stops before a `.` that opens a
/// `.{…}` group.
fn use_path(p: &mut Parser<'_>) {
    let m = p.start();
    if !name_token(p, "import path") {
        m.complete(p, SyntaxKind::Path);
        return;
    }
    while p.at_punct(Punct::Dot) {
        if at_use_group(p) {
            break;
        }
        p.bump();
        if !name_token(p, "path segment") {
            break;
        }
    }
    m.complete(p, SyntaxKind::Path);
}

/// Bounded lookahead: `.` followed by `{` opens a use group; a `.`
/// followed by an identifier continues the path. One token is not
/// enough to tell them apart.
fn at_use_group(p: &Parser<'_>) -> bool {
    p.at_punct(Punct::Dot) && p.nth(1) == TokenKind::Punct(Punct::LBrace)
}

/// `{a, b}` of `use path.{a, b}`.
fn use_group(p: &mut Parser<'_>) {
    let m = p.start();
    let opener = p.current_span();
    p.bump(); // `{`
    loop {
        p.eat_terms();
        if p.at_punct(Punct::RBrace) {
            p.bump();
            break;
        }
        if p.at_eof() || p.at_decl_start() {
            unclosed(p, opener, "{");
            break;
        }
        let before = p.pos();
        match p.current() {
            TokenKind::Ident => p.bump(),
            TokenKind::Kw(k) if !is_decl_keyword(k) => {
                keyword_as_ident(p, "imported");
            }
            _ => {
                p.error(
                    codes::EXPECTED_TOKEN,
                    p.current_span(),
                    "expected an imported name",
                );
                p.recover_until(true, |k| {
                    matches!(k, TokenKind::Punct(Punct::Comma) | TokenKind::Term)
                });
            }
        }
        if p.at_punct(Punct::Comma) {
            p.bump();
        }
        if p.pos() == before {
            unclosed(p, opener, "{");
            break;
        }
    }
    m.complete(p, SyntaxKind::UseGroup);
}

pub(crate) fn import_c_item(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // `import`
    if p.at(TokenKind::Ident) {
        if p.current_text() != b"c" {
            p.error(
                codes::EXPECTED_TOKEN,
                p.current_span(),
                "only C headers can be imported — write `import c \"header.h\"`",
            );
        }
        p.bump();
    } else {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `c` after `import` — the form is `import c \"header.h\"`",
        );
        p.missing();
    }
    if p.at_str_begin() {
        string_lit(p);
    } else {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected the header name as a string, like `import c \"stdio.h\"`",
        );
        p.missing();
    }
    item_term(p, "`import c` item");
    m.complete(p, SyntaxKind::ImportCDecl);
}

/// The TERM that ends a TERM-terminated item, with recovery.
fn item_term(p: &mut Parser<'_>, what: &str) {
    if p.at(TokenKind::Term) {
        p.bump();
    } else if !p.at_eof() && !p.at_decl_start() && !p.at_punct(Punct::RBrace) {
        p.expected_line_end(what);
        if p.at_punct(Punct::LBrace) {
            // A stray block: parse it rather than skipping raw — a
            // never-closed `{` would otherwise swallow the file, and
            // declarations inside a closed one survive intact.
            let e = p.start();
            crate::exprs::block(p);
            e.complete(p, SyntaxKind::ErrorNode);
        } else {
            p.recover_until(true, |k| k == TokenKind::Term);
        }
        if p.at(TokenKind::Term) {
            p.bump();
        }
    }
}

// ------------------------------------------- attributes & visibility --

/// `#[' attr (',' attr)* ']` `[gram.item.attr]`.
pub(crate) fn attribute(p: &mut Parser<'_>) {
    let m = p.start();
    let opener = p.current_span();
    p.bump(); // `#[`
    attr_list_tail(p, opener, "#[");
    m.complete(p, SyntaxKind::Attribute);
}

/// `#![' attr (',' attr)* ']` — the file-wide attribute
/// (`[gram.attr.index]`). Legal only as the very first construct of a
/// file (after the shebang); `misplaced` marks every other position and
/// reports E0211 while still parsing the node, so one mistake is one
/// diagnostic.
pub(crate) fn inner_attribute(p: &mut Parser<'_>, misplaced: bool) {
    let m = p.start();
    let opener = p.current_span();
    if misplaced {
        p.error(
            codes::MISPLACED_INNER_ATTRIBUTE,
            opener,
            "a file-wide `#![…]` attribute must be the first thing in its \
             file — move it above the first declaration",
        );
    }
    p.bump(); // `#![`
    attr_list_tail(p, opener, "#![");
    m.complete(p, SyntaxKind::InnerAttribute);
}

/// The shared `attr (',' attr)* ']'` tail of both attribute forms;
/// `opener_text` names the opener in the unclosed-delimiter report.
fn attr_list_tail(p: &mut Parser<'_>, opener: Span, opener_text: &'static str) {
    loop {
        if p.at_punct(Punct::RBracket) {
            p.bump();
            break;
        }
        if p.at_eof() || p.at_decl_start() || p.at_punct(Punct::LBrace) {
            unclosed(p, opener, opener_text);
            break;
        }
        let before = p.pos();
        match p.current() {
            TokenKind::Ident => attr_item(p),
            TokenKind::Kw(k) if !is_decl_keyword(k) => attr_item(p),
            _ => {
                p.error(
                    codes::MALFORMED_ATTRIBUTE,
                    p.current_span(),
                    "this attribute needs a name: `#[name]` or `#[name(args)]`",
                );
                p.recover_until(true, |k| {
                    matches!(k, TokenKind::Punct(Punct::Comma | Punct::RBracket))
                });
            }
        }
        if p.at_punct(Punct::Comma) {
            p.bump();
        }
        if p.pos() == before {
            unclosed(p, opener, opener_text);
            break;
        }
    }
}

/// `path attr_input?`.
fn attr_item(p: &mut Parser<'_>) {
    let m = p.start();
    path(p, "attribute");
    if p.at_punct(Punct::LParen) {
        attr_input_args(p);
    } else if p.at_punct(Punct::Eq) {
        attr_input_eq(p);
    }
    m.complete(p, SyntaxKind::AttrItem);
}

/// `'(' attr_arg (',' attr_arg)* ')'` where `attr_arg ::= attr | literal`.
fn attr_input_args(p: &mut Parser<'_>) {
    let m = p.start();
    let opener = p.current_span();
    p.bump(); // `(`
    loop {
        if p.at_punct(Punct::RParen) {
            p.bump();
            break;
        }
        if p.at_eof() || p.at_decl_start() || p.at_punct(Punct::RBracket) {
            unclosed(p, opener, "(");
            break;
        }
        let before = p.pos();
        match p.current() {
            TokenKind::Ident => attr_item(p),
            TokenKind::Int | TokenKind::Float | TokenKind::Kw(Keyword::True | Keyword::False) => {
                p.bump()
            }
            k if is_str_begin(k) => string_lit(p),
            _ => {
                p.error(
                    codes::MALFORMED_ATTRIBUTE,
                    p.current_span(),
                    "attribute arguments are literals or nested attributes, like `#[repr(c)]`",
                );
                p.recover_until(true, |k| {
                    matches!(
                        k,
                        TokenKind::Punct(Punct::Comma | Punct::RParen | Punct::RBracket)
                    )
                });
            }
        }
        if p.at_punct(Punct::Comma) {
            p.bump();
        }
        if p.pos() == before {
            unclosed(p, opener, "(");
            break;
        }
    }
    m.complete(p, SyntaxKind::AttrInput);
}

/// `'=' literal`.
fn attr_input_eq(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(); // `=`
    match p.current() {
        TokenKind::Int | TokenKind::Float | TokenKind::Kw(Keyword::True | Keyword::False) => {
            p.bump()
        }
        k if is_str_begin(k) => string_lit(p),
        _ => {
            p.error(
                codes::MALFORMED_ATTRIBUTE,
                p.here(),
                "expected a literal after the `=`, like `#[deprecated = \"note\"]`",
            );
            p.missing();
        }
    }
    m.complete(p, SyntaxKind::AttrInput);
}

/// `pub` / `pub(pkg)`.
pub(crate) fn visibility(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(); // `pub`
    if p.at_punct(Punct::LParen) {
        p.bump();
        if p.at(TokenKind::Ident) && p.current_text() == b"pkg" {
            p.bump();
        } else {
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                "the only visibility qualifier is `pub(pkg)` — expected `pkg` here",
            );
            p.recover_until(true, |k| k == TokenKind::Punct(Punct::RParen));
        }
        p.expect_punct(Punct::RParen, "`)` to close `pub(`");
    }
    m.complete(p, SyntaxKind::Visibility);
}

// ---------------------------------------------------------------- paths --

/// `IDENT ('.' IDENT)*`. Reserved keywords in segment position get the
/// E0008 treatment. Bounded lookahead: a `.` not followed by a name
/// stays unconsumed — it belongs to whatever broke around the path,
/// and the caller's recovery handles it with one report, not two.
pub(crate) fn path(p: &mut Parser<'_>, what: &str) {
    let m = p.start();
    if !name_token(p, what) {
        m.complete(p, SyntaxKind::Path);
        return;
    }
    while p.at_punct(Punct::Dot) {
        let nameable = match p.nth(1) {
            TokenKind::Ident => true,
            TokenKind::Kw(k) => !is_decl_keyword(k),
            _ => false,
        };
        if !nameable {
            break;
        }
        p.bump();
        name_token(p, "path segment");
    }
    m.complete(p, SyntaxKind::Path);
}

// ---------------------------------------------------------------- types --

/// Parse a type per `[gram.type]`, postfix `! {row}` tail included.
/// Returns false (emitting nothing) when the current token cannot
/// begin a type.
pub(crate) fn type_(p: &mut Parser<'_>) -> bool {
    type_general(p, true)
}

/// [`type_`] with the postfix row switchable: rows are first-class on
/// every type position (#3) — `let v: int ! {None}`, parameter and
/// payload types — except an item return, where [`ret_type`] owns the
/// `! {row}` tail (keeping the header's tree shape).
fn type_general(p: &mut Parser<'_>, postfix_row: bool) -> bool {
    let cm = match p.current() {
        TokenKind::Punct(Punct::Not) => {
            let m = p.start();
            p.bump();
            type_required(p);
            m.complete(p, SyntaxKind::ErrorUnionType)
        }
        TokenKind::Punct(Punct::Star) => {
            let m = p.start();
            p.bump();
            type_required(p);
            m.complete(p, SyntaxKind::PtrType)
        }
        TokenKind::Kw(Keyword::Shared | Keyword::Handle | Keyword::Weak | Keyword::Distinct) => {
            let m = p.start();
            p.bump();
            type_required(p);
            m.complete(p, SyntaxKind::PrefixType)
        }
        TokenKind::Kw(Keyword::Dyn) => {
            let m = p.start();
            p.bump();
            if p.at(TokenKind::Ident) {
                path(p, "trait");
            } else {
                p.error(
                    codes::EXPECTED_TYPE,
                    p.here(),
                    "expected a trait name after `dyn`, like `dyn Writer`",
                );
                p.missing();
            }
            m.complete(p, SyntaxKind::DynType)
        }
        TokenKind::Kw(Keyword::Fn) => {
            let m = p.start();
            p.bump();
            if p.at_punct(Punct::LParen) {
                paren_type_list(p);
            } else {
                p.error(
                    codes::EXPECTED_TOKEN,
                    p.here(),
                    "a function type spells its parameters: `fn(int) -> int`",
                );
                p.missing();
            }
            if p.at_punct(Punct::Arrow) {
                ret_type(p);
            }
            m.complete(p, SyntaxKind::FnType)
        }
        TokenKind::Kw(Keyword::Type) => {
            let m = p.start();
            p.bump();
            m.complete(p, SyntaxKind::TypeType)
        }
        TokenKind::Kw(Keyword::Region) => {
            let m = p.start();
            p.bump();
            m.complete(p, SyntaxKind::RegionType)
        }
        TokenKind::Punct(Punct::LParen) => {
            let m = p.start();
            paren_type_list(p);
            m.complete(p, SyntaxKind::TupleType)
        }
        TokenKind::Ident => {
            let m = p.start();
            path(p, "type");
            if p.at_punct(Punct::LBracket) {
                type_args(p);
            }
            m.complete(p, SyntaxKind::PathType)
        }
        _ => return false,
    };
    // Only `!` immediately opening a row is a tail; a lone `!` belongs
    // to whatever follows (a prefix `!T`, an expression). The tail is
    // right-recursive (#34): `T ! {a} ! {b}` parses as
    // `(T ! {a}) ! {b}` — the grammar's `type '!' error_row`
    // production admits its own result — each further row wrapping
    // the node before it. What a nested union MEANS is sema's
    // question (it refuses by name until the spec rules); the
    // parser's job is to stop answering a grammar question with
    // E0201.
    let mut cm = cm;
    while postfix_row && p.at_punct(Punct::Not) && p.nth(1) == TokenKind::Punct(Punct::LBrace) {
        let m = cm.precede(p);
        p.bump(); // `!`
        error_row(p);
        cm = m.complete(p, SyntaxKind::ErrorUnionType);
    }
    true
}

/// [`type_`], diagnosing E0206 + Missing when no type can start here.
pub(crate) fn type_required(p: &mut Parser<'_>) {
    if !type_(p) {
        p.error(codes::EXPECTED_TYPE, p.here(), "expected a type");
        p.missing();
    }
}

/// [`type_required`] minus the postfix row — return position, where
/// [`ret_type`] parses the `! {row}` tail itself.
fn type_required_no_row(p: &mut Parser<'_>) {
    if !type_general(p, false) {
        p.error(codes::EXPECTED_TYPE, p.here(), "expected a type");
        p.missing();
    }
}

/// `'(' (type (',' type)* ','?)? ')'` — tuple types, fn-type parameter
/// lists, variant payloads, row payloads.
fn paren_type_list(p: &mut Parser<'_>) {
    let opener = p.current_span();
    p.bump(); // `(`
    loop {
        if p.at_punct(Punct::RParen) {
            p.bump();
            break;
        }
        if p.at_eof()
            || p.at_decl_start()
            || matches!(
                p.current(),
                TokenKind::Punct(Punct::LBrace | Punct::RBrace | Punct::Arrow)
            )
        {
            unclosed(p, opener, "(");
            break;
        }
        let before = p.pos();
        if !type_(p) {
            p.error(codes::EXPECTED_TYPE, p.current_span(), "expected a type");
            p.recover_until(true, |k| {
                matches!(k, TokenKind::Punct(Punct::Comma | Punct::RParen))
            });
        }
        if p.at_punct(Punct::Comma) {
            p.bump();
        } else if !p.at_punct(Punct::RParen) && p.pos() != before {
            // the grammar requires the separator: `(int, int, int)`, never
            // `(int int int)` — leniency here was DIV-001
            p.error(
                codes::EXPECTED_TOKEN,
                p.current_span(),
                "expected `,` or `)` after this type",
            );
        }
        if p.pos() == before {
            unclosed(p, opener, "(");
            break;
        }
    }
}

/// `'[' type_arg (',' type_arg)* ','? ']'` — type application.
fn type_args(p: &mut Parser<'_>) {
    let m = p.start();
    let opener = p.current_span();
    p.bump(); // `[`
    loop {
        if p.at_punct(Punct::RBracket) {
            p.bump();
            break;
        }
        if p.at_eof() || type_arg_escape(p.current()) {
            unclosed(p, opener, "[");
            break;
        }
        let before = p.pos();
        type_arg(p);
        if p.at_punct(Punct::Comma) {
            p.bump();
            continue;
        }
        if p.at_punct(Punct::RBracket) {
            continue;
        }
        if p.pos() == before {
            unclosed(p, opener, "[");
            break;
        }
    }
    m.complete(p, SyntaxKind::TypeArgList);
}

/// Tokens that can never continue a type argument at depth zero —
/// reaching one means the `[` was never closed. `->` and `{` are
/// included: a fn-type argument parses through the *type* grammar
/// (its arrow is behind `fn(…)`), so a bare depth-zero arrow or brace
/// here belongs to the enclosing header, and stopping keeps an
/// unclosed `[` from swallowing the function body (D22).
fn type_arg_escape(k: TokenKind) -> bool {
    matches!(
        k,
        TokenKind::Punct(Punct::Arrow | Punct::LBrace)
            | TokenKind::Kw(
                Keyword::Let
                    | Keyword::Var
                    | Keyword::Const
                    | Keyword::Use
                    | Keyword::Import
                    | Keyword::Trait
                    | Keyword::Impl
                    | Keyword::Pub
                    | Keyword::Struct
                    | Keyword::Enum
            )
    )
}

/// One `[…]` argument: a type, or an expression-shaped token group
/// (const generics), parked as `TypeArgPending` for sema (D29).
fn type_arg(p: &mut Parser<'_>) {
    let starts_type = matches!(
        p.current(),
        TokenKind::Ident
            | TokenKind::Punct(Punct::Not | Punct::Star | Punct::LParen)
            | TokenKind::Kw(
                Keyword::Fn
                    | Keyword::Dyn
                    | Keyword::Type
                    | Keyword::Region
                    | Keyword::Shared
                    | Keyword::Handle
                    | Keyword::Weak
                    | Keyword::Distinct
            )
    );
    if starts_type {
        // Speculative type parse behind the event-rollback checkpoint:
        // `Map[str, int]` and `Buf[N * 2]` both start with an identifier
        // — only what follows the parsed type tells them apart, so try
        // the type grammar and roll back if it did not consume the
        // whole argument.
        let cp = p.checkpoint();
        type_(p);
        if p.at_punct(Punct::Comma) || p.at_punct(Punct::RBracket) {
            return;
        }
        p.rollback(cp);
    }
    type_arg_pending(p);
}

/// Park a raw expression-shaped token group up to a depth-zero `,` `]`.
fn type_arg_pending(p: &mut Parser<'_>) {
    let m = p.start();
    let consumed = raw_scan(p, |k| {
        matches!(
            k,
            TokenKind::Punct(Punct::Comma | Punct::RBracket | Punct::RBrace)
        ) || k == TokenKind::Term
            || type_arg_escape(k)
    });
    if consumed == 0 {
        m.abandon(p);
        p.error(codes::EXPECTED_TYPE, p.here(), "expected a type argument");
        p.missing();
    } else {
        m.complete(p, SyntaxKind::TypeArgPending);
    }
}

// ------------------------------------------------------------- patterns --

/// `[gram.pat]`, or-patterns wrapped via `precede`. Returns `None` if
/// no pattern can start here (nothing emitted); otherwise the
/// completed pattern node (callers key shape decisions — the D63
/// bare-tuple teach-note — off its kind).
pub(crate) fn pattern(p: &mut Parser<'_>) -> Option<crate::parser::CompletedMarker> {
    let first = pattern_atom(p)?;
    if p.at_punct(Punct::Pipe) {
        let m = first.precede(p);
        while p.at_punct(Punct::Pipe) {
            p.bump();
            if pattern_atom(p).is_none() {
                p.error(
                    codes::EXPECTED_PATTERN,
                    p.here(),
                    "expected a pattern after `|`",
                );
                p.missing();
                break;
            }
        }
        return Some(m.complete(p, SyntaxKind::OrPat));
    }
    Some(first)
}

pub(crate) fn pattern_atom(p: &mut Parser<'_>) -> Option<crate::parser::CompletedMarker> {
    match p.current() {
        TokenKind::Underscore => {
            let m = p.start();
            p.bump();
            Some(m.complete(p, SyntaxKind::WildcardPat))
        }
        TokenKind::Int
        | TokenKind::Float
        | TokenKind::Char
        | TokenKind::Kw(Keyword::True | Keyword::False) => {
            let m = p.start();
            p.bump();
            Some(m.complete(p, SyntaxKind::LiteralPat))
        }
        k if is_str_begin(k) => {
            let m = p.start();
            string_lit(p);
            Some(m.complete(p, SyntaxKind::LiteralPat))
        }
        TokenKind::Punct(Punct::LParen) => {
            let m = p.start();
            let opener = p.current_span();
            p.bump();
            let mut list_diags = p.diag_count();
            loop {
                if p.at_punct(Punct::RParen) {
                    p.bump();
                    break;
                }
                if p.at_eof() || p.at_decl_start() || p.at_punct(Punct::Eq) {
                    unclosed(p, opener, "(");
                    break;
                }
                let before = p.pos();
                if pattern(p).is_none() {
                    if !p.arm_error_reported {
                        p.arm_error_reported = true;
                        p.error(
                            codes::EXPECTED_PATTERN,
                            p.current_span(),
                            "expected a pattern",
                        );
                    }
                    p.recover_until(true, |k| {
                        matches!(
                            k,
                            TokenKind::Punct(Punct::Comma | Punct::RParen | Punct::Eq)
                        )
                    });
                }
                pattern_separator(
                    p,
                    Punct::RParen,
                    "the tuple pattern's elements",
                    &mut list_diags,
                );
                if p.pos() == before {
                    unclosed(p, opener, "(");
                    break;
                }
            }
            Some(m.complete(p, SyntaxKind::TuplePat))
        }
        TokenKind::Ident if at_binding_pattern(p) => {
            let m = p.start();
            p.bump(); // name
            p.bump(); // `@`
            if pattern_atom(p).is_none() {
                p.error(
                    codes::EXPECTED_PATTERN,
                    p.here(),
                    "expected a pattern after `@`",
                );
                p.missing();
            }
            Some(m.complete(p, SyntaxKind::BindingPat))
        }
        TokenKind::Ident if at_path_pattern(p) => {
            let m = p.start();
            path(p, "pattern");
            if p.at_punct(Punct::LParen) {
                pattern_payload(p);
            } else if p.at_punct(Punct::LBrace) {
                // `[gram.pat.struct]` (s129, #179): `Point { x, y: p, .. }`.
                struct_pat_fields(p);
                return Some(m.complete(p, SyntaxKind::StructPat));
            } else {
                p.error(
                    codes::EXPECTED_TOKEN,
                    p.here(),
                    "a dotted path in a pattern must carry a payload, like `io.Error(e)`",
                );
                p.missing();
            }
            Some(m.complete(p, SyntaxKind::PathPat))
        }
        TokenKind::Ident => {
            let m = p.start();
            p.bump();
            Some(m.complete(p, SyntaxKind::IdentPat))
        }
        _ => None,
    }
}

/// Bounded lookahead: `x @ pat` and bare `x` both start with an
/// identifier — only the `@` after it decides.
fn at_binding_pattern(p: &Parser<'_>) -> bool {
    p.nth(1) == TokenKind::Punct(Punct::At)
}

/// Bounded lookahead: `Tag(pat)` / `io.Error(pat)` / `Point { … }`
/// vs. bare `x` — the `.`, `(`, or `{` after the identifier decides
/// (an identifier pattern is never followed by `{` in any position a
/// pattern is parsed, so the brace is free — `[gram.pat.struct]`).
fn at_path_pattern(p: &Parser<'_>) -> bool {
    matches!(
        p.nth(1),
        TokenKind::Punct(Punct::Dot | Punct::LParen | Punct::LBrace)
    )
}

/// The separating comma is REQUIRED between pattern members (D67:
/// the production is the law — `(',' pattern)*` never licensed the
/// comma-less run wolfgang's recovery loop accepted, and lupin
/// refuses it). Called after each member: a `,` is consumed; the
/// closer (or a truncation the loop's own guards will report) is
/// end-of-list; anything else — `..` included, which follows a
/// separator like one more member — is E0201 with a
/// machine-applicable "add the comma" insertion, and parsing
/// continues as if the comma were present, so the tree keeps every
/// member and one deleted comma costs exactly one report. The report
/// latches per LIST (`reported`): a damaged list whose recovery
/// re-enters the loop must not pay one separator report per member it
/// chews — the D22 cascade budget measured exactly that wreck
/// (MUTATE_BUDGET=300, a duplicated `(` in a payload) — while the
/// well-formed miss keeps its full teach-note.
fn pattern_separator(p: &mut Parser<'_>, closer: Punct, what: &str, list_diags: &mut usize) {
    if p.at_punct(Punct::Comma) {
        p.bump();
        return;
    }
    if p.at_punct(closer) || p.at_eof() || p.at_decl_start() || p.at_punct(Punct::Eq) {
        return;
    }
    if p.diag_count() != *list_diags {
        // The list is already a reported wreck (a damaged member, an
        // earlier separator miss, recovery in flight): the comma is
        // not the lesson, and the D22 budget owns the count.
        return;
    }
    let span = p.here();
    let (msg, note) = if p.at_punct(Punct::DotDot) {
        (
            "expected `,` before `..` — it ignores the remaining fields like one more member"
                .to_string(),
            "the separating comma is required, and `..` follows one: `Point { x, .. }`, \
             never `Point { x .. }` ([gram.pat.struct])",
        )
    } else {
        (
            format!("expected `,` between {what}"),
            "the separating comma is required between pattern members ([gram.pat])",
        )
    };
    let anchor = p.prev_end().unwrap_or_else(|| p.at_here());
    p.push_diag(
        wolf_diag::Diagnostic::error(codes::EXPECTED_TOKEN, span, msg)
            .with_note(note)
            .with_suggestion(wolf_diag::Suggestion::new(
                "add the comma",
                vec![(anchor, ",".to_string())],
                wolf_diag::Applicability::MachineApplicable,
            )),
    );
    // Once per list, and never into a wreck: the sentinel makes every
    // later miss in this list read as "already reported", and the arm
    // latch folds the member wreckage that often follows a real wreck's
    // first separator miss — the D22 posture damaged members already
    // hold (one report per damaged arm region, resynchronize, move on).
    *list_diags = usize::MAX;
    p.arm_error_reported = true;
}

/// D69's expression-list twin of [`pattern_separator`] (s132): the
/// separating comma is REQUIRED between struct-literal fields,
/// closure parameters, and captured names — those productions never
/// licensed the comma-less run their recovery loops accepted, and
/// lupin refuses every lax spelling (E0201, the newline-separated
/// struct literal included). Called after each member: a `,` is
/// consumed; the closer (or a truncation the loop's own guards will
/// report) is end-of-list; anything else is E0201 with the
/// machine-applicable "add the comma" insertion at the previous
/// member's end, and parsing continues as if the comma were present.
/// `terms_close` is the struct literal's layout carve-out: a
/// terminator run whose next significant token is the CLOSER is the
/// production's own trailing layout (`Point {\n  x: 7,\n}`), while
/// one followed by another member is the newline-separated lax form,
/// refused like the same-line one. Latches once per list and never
/// into a reported wreck — [`pattern_separator`]'s D22 discipline,
/// verbatim.
pub(crate) fn list_separator(
    p: &mut Parser<'_>,
    closer: Punct,
    what: &str,
    note: &'static str,
    terms_close: bool,
    list_diags: &mut usize,
) {
    if p.at_punct(Punct::Comma) {
        p.bump();
        return;
    }
    if p.at_punct(closer) || p.at_eof() || p.at_decl_start() || p.at_punct(Punct::Eq) {
        return;
    }
    let mut span = p.here();
    if p.at(TokenKind::Term) {
        if terms_close {
            // Bounded lookahead past the terminator run: closer (or
            // the file's end) = layout; a member = the lax form,
            // reported AT the member the missing comma should precede
            // (where lupin points).
            let mut k = 0;
            while matches!(p.nth(k), TokenKind::Term) {
                k += 1;
            }
            if matches!(p.nth(k), TokenKind::Punct(pk) if pk == closer)
                || p.nth(k) == TokenKind::Eof
            {
                return;
            }
            span = p.nth_span(k);
        } else {
            // Bracketed lists treat a terminator as truncation — the
            // loop's own unclosed guard owns that report.
            return;
        }
    }
    if p.diag_count() != *list_diags {
        // The list is already a reported wreck (a damaged member, an
        // earlier separator miss, recovery in flight): the comma is
        // not the lesson, and the D22 budget owns the count.
        return;
    }
    let anchor = p.prev_end().unwrap_or_else(|| p.at_here());
    p.push_diag(
        wolf_diag::Diagnostic::error(
            codes::EXPECTED_TOKEN,
            span,
            format!("expected `,` between {what}"),
        )
        .with_note(note)
        .with_suggestion(wolf_diag::Suggestion::new(
            "add the comma",
            vec![(anchor, ",".to_string())],
            wolf_diag::Applicability::MachineApplicable,
        )),
    );
    *list_diags = usize::MAX;
    p.arm_error_reported = true;
}

/// `'(' pattern (',' pattern)* ','? ')'` after a path pattern.
fn pattern_payload(p: &mut Parser<'_>) {
    let opener = p.current_span();
    p.bump(); // `(`
    let mut list_diags = p.diag_count();
    loop {
        if p.at_punct(Punct::RParen) {
            p.bump();
            break;
        }
        if p.at_eof() || p.at_decl_start() || p.at_punct(Punct::Eq) {
            unclosed(p, opener, "(");
            break;
        }
        let before = p.pos();
        if pattern(p).is_none() {
            if !p.arm_error_reported {
                p.arm_error_reported = true;
                p.error(
                    codes::EXPECTED_PATTERN,
                    p.current_span(),
                    "expected a pattern",
                );
            }
            p.recover_until(true, |k| {
                matches!(
                    k,
                    TokenKind::Punct(Punct::Comma | Punct::RParen | Punct::Eq)
                )
            });
        }
        pattern_separator(p, Punct::RParen, "the payload's patterns", &mut list_diags);
        if p.pos() == before {
            unclosed(p, opener, "(");
            break;
        }
    }
}

/// `'{' field_pat (',' field_pat)* (',' '..'?)? '}'` after a struct
/// pattern's path (`[gram.pat.struct]`, s129 #179; the separator
/// requirement — `..` included — is D67's, measured at #190). Each
/// member is a `FieldPat`: the shorthand wraps a single `IdentPat`
/// (the binding IS the field name); `IDENT ':' pattern` matches the
/// field against any pattern. A `..` before the brace closes the list
/// — nothing may follow it, and a separator precedes it like any
/// member's. Recovery mirrors [`pattern_payload`]'s discipline (the
/// D22 budget): one report per damaged member, resynchronize at
/// `,`/`}`/`=`, and a truncation at statement scale reports the
/// unclosed opener once.
fn struct_pat_fields(p: &mut Parser<'_>) {
    let opener = p.current_span();
    p.bump(); // `{`
    let mut list_diags = p.diag_count();
    loop {
        if p.at_punct(Punct::RBrace) {
            p.bump();
            break;
        }
        if p.at_eof() || p.at_decl_start() || p.at_punct(Punct::Eq) {
            unclosed(p, opener, "{");
            break;
        }
        if p.at_punct(Punct::DotDot) {
            let rm = p.start();
            p.bump();
            rm.complete(p, SyntaxKind::RestPat);
            if p.at_punct(Punct::RBrace) {
                p.bump();
            } else {
                p.error(
                    codes::EXPECTED_TOKEN,
                    p.here(),
                    "`..` ignores the remaining fields and must be last — expected `}`",
                );
                p.recover_until(true, |k| {
                    k == TokenKind::Punct(Punct::RBrace) || k == TokenKind::Punct(Punct::Eq)
                });
                if p.at_punct(Punct::RBrace) {
                    p.bump();
                }
            }
            break;
        }
        let before = p.pos();
        if p.at(TokenKind::Ident) {
            let fm = p.start();
            if p.nth(1) == TokenKind::Punct(Punct::Colon) {
                p.bump(); // field name
                p.bump(); // `:`
                if pattern(p).is_none() {
                    p.error(
                        codes::EXPECTED_PATTERN,
                        p.here(),
                        "expected a pattern after `:`",
                    );
                    p.missing();
                }
            } else {
                // Shorthand: the name binds itself.
                let im = p.start();
                p.bump();
                im.complete(p, SyntaxKind::IdentPat);
            }
            fm.complete(p, SyntaxKind::FieldPat);
        } else {
            if !p.arm_error_reported {
                p.arm_error_reported = true;
                p.error(
                    codes::EXPECTED_PATTERN,
                    p.current_span(),
                    "expected a field name",
                );
            }
            p.recover_until(true, |k| {
                matches!(
                    k,
                    TokenKind::Punct(Punct::Comma | Punct::RBrace | Punct::Eq | Punct::DotDot)
                )
            });
        }
        pattern_separator(
            p,
            Punct::RBrace,
            "the struct pattern's fields",
            &mut list_diags,
        );
        if p.pos() == before {
            unclosed(p, opener, "{");
            break;
        }
    }
}

// -------------------------------------------------- opaque bodies --------

/// An opaque `{…}` consumed by delimiter matching into an
/// `InlineCBody` node holding the raw tokens (the body of an
/// `unsafe c` block is C text, not wolf syntax — c10 owns its meaning).
/// Complete and lossless; unbalanced delimiters degrade with a
/// closest-match heuristic and an E0202 at the unclosed opener.
pub(crate) fn opaque_brace_block(p: &mut Parser<'_>) {
    let m = p.start();
    let opener = p.current_span();
    p.bump(); // `{`
    let mut stack: Vec<(Punct, Span)> = vec![(Punct::LBrace, opener)];
    loop {
        match p.current() {
            TokenKind::Eof => {
                flush_unclosed(p, &mut stack);
                p.missing();
                break;
            }
            k if is_str_begin(k) => raw_string_episode(p),
            TokenKind::Punct(pc @ (Punct::LParen | Punct::LBracket | Punct::LBrace)) => {
                stack.push((pc, p.current_span()));
                p.bump();
            }
            TokenKind::Punct(pc @ (Punct::RParen | Punct::RBracket)) => {
                close_delim(p, &mut stack, pc, 1);
                p.bump();
            }
            TokenKind::Punct(Punct::RBrace) => {
                // Pop unclosed non-brace openers above the nearest `{`
                // (closest-match), then close it.
                while let Some(&(k, s)) = stack.last() {
                    if k == Punct::LBrace {
                        break;
                    }
                    unclosed_diag(p, s, open_text(k));
                    stack.pop();
                }
                stack.pop();
                p.bump();
                if stack.is_empty() {
                    break;
                }
            }
            _ => p.bump(),
        }
    }
    m.complete(p, SyntaxKind::InlineCBody);
}

/// Raw scan with delimiter matching: consume tokens into the current
/// node until `stop` matches at delimiter depth zero (or Eof). String
/// episodes are consumed wholesale; unclosed openers get E0202 at Eof;
/// mismatched closers use the closest-match heuristic. Declaration
/// keywords are a hard stop at *any* depth: the scan's only client is
/// `TypeArgPending` (expression-shaped const-generic arguments), where
/// a declaration keyword can only mean an unclosed delimiter about to
/// swallow the rest of the file (D22 containment).
fn raw_scan(p: &mut Parser<'_>, stop: impl Fn(TokenKind) -> bool) -> usize {
    let mut stack: Vec<(Punct, Span)> = Vec::new();
    let mut consumed = 0usize;
    loop {
        let k = p.current();
        if k == TokenKind::Eof {
            flush_unclosed(p, &mut stack);
            break;
        }
        if let TokenKind::Kw(kw) = k
            && is_decl_keyword(kw)
        {
            flush_unclosed(p, &mut stack);
            break;
        }
        if stack.is_empty() && stop(k) {
            break;
        }
        match k {
            k if is_str_begin(k) => raw_string_episode(p),
            TokenKind::Punct(pc @ (Punct::LParen | Punct::LBracket | Punct::LBrace)) => {
                stack.push((pc, p.current_span()));
                p.bump();
            }
            TokenKind::Punct(pc @ (Punct::RParen | Punct::RBracket | Punct::RBrace)) => {
                close_delim(p, &mut stack, pc, 0);
                p.bump();
            }
            _ => p.bump(),
        }
        consumed += 1;
    }
    consumed
}

/// Closest-match close: pop unclosed openers (diagnosing each) down to
/// the one `closer` matches; a closer with no match in the stack (above
/// `floor`) is consumed as stray content.
fn close_delim(p: &mut Parser<'_>, stack: &mut Vec<(Punct, Span)>, closer: Punct, floor: usize) {
    let want = match closer {
        Punct::RParen => Punct::LParen,
        Punct::RBracket => Punct::LBracket,
        _ => Punct::LBrace,
    };
    let Some(idx) = stack[floor..]
        .iter()
        .rposition(|&(k, _)| k == want)
        .map(|i| i + floor)
    else {
        return; // stray closer: content
    };
    while stack.len() > idx + 1 {
        let (k, s) = stack.pop().expect("nonempty above idx");
        unclosed_diag(p, s, open_text(k));
    }
    stack.pop();
}

fn flush_unclosed(p: &mut Parser<'_>, stack: &mut Vec<(Punct, Span)>) {
    while let Some((k, s)) = stack.pop() {
        unclosed_diag(p, s, open_text(k));
    }
}

fn open_text(k: Punct) -> &'static str {
    match k {
        Punct::LParen => "(",
        Punct::LBracket => "[",
        _ => "{",
    }
}

/// One whole string episode, consumed raw (the lexer guarantees
/// `StrBegin`/`StrEnd` balance even on malformed input).
fn raw_string_episode(p: &mut Parser<'_>) {
    debug_assert!(p.at_str_begin());
    let mut depth = 0usize;
    loop {
        match p.current() {
            TokenKind::StrBegin(_) => depth += 1,
            TokenKind::StrEnd { .. } => depth -= 1,
            TokenKind::Eof => break, // defensive; the lexer forbids this
            _ => {}
        }
        p.bump();
        if depth == 0 {
            break;
        }
    }
}

/// A string episode wrapped as a `StringLit` node (header positions).
pub(crate) fn string_lit(p: &mut Parser<'_>) {
    let m = p.start();
    raw_string_episode(p);
    m.complete(p, SyntaxKind::StringLit);
}

// -------------------------------------------------------------- helpers --

pub(crate) fn is_str_begin(k: TokenKind) -> bool {
    matches!(k, TokenKind::StrBegin(_))
}

/// A name position: bump an `Ident`; a reserved keyword gets E0008
/// (spec §9) and is consumed as the name; anything else gets E0201 and
/// a Missing marker. Declaration-leading keywords are *not* consumed —
/// they are sync points and almost certainly start the next item.
pub(crate) fn name_token(p: &mut Parser<'_>, what: &str) -> bool {
    match p.current() {
        TokenKind::Ident => {
            p.bump();
            true
        }
        TokenKind::Kw(k) if !is_decl_keyword(k) => {
            keyword_as_ident(p, what);
            true
        }
        _ => {
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                format!("expected a {what} name"),
            );
            p.missing();
            false
        }
    }
}

/// E0008: keyword used as an identifier — names the keyword and
/// suggests renaming (wolf has no raw identifiers).
fn keyword_as_ident(p: &mut Parser<'_>, what: &str) {
    let text = String::from_utf8_lossy(p.current_text()).into_owned();
    let span = p.current_span();
    p.push_diag(
        wolf_diag::Diagnostic::error(
            codes::KEYWORD_AS_IDENT,
            span,
            format!("`{text}` is a reserved keyword, so it cannot name a {what}"),
        )
        .with_label("pick another name")
        .with_note(format!(
            "all 50 keywords are reserved everywhere, and wolf has no raw \
             identifiers. `{text}_` is the usual dodge, or a more specific word.",
        )),
    );
    p.bump();
}

/// E0202 at the unclosed opener, noting where scanning stopped, plus a
/// zero-width Missing marker for the absent closer.
pub(crate) fn unclosed(p: &mut Parser<'_>, opener: Span, what: &str) {
    unclosed_diag(p, opener, what);
    p.missing();
}

/// The E0202 diagnostic alone (closest-match pops mid-scan: the fence
/// content is raw tokens, a Missing marker there would be noise).
fn unclosed_diag(p: &mut Parser<'_>, opener: Span, what: &str) {
    // The escape is this line's error, and every downstream
    // suppressed-terminator miss is the same wreck (D22 containment).
    p.fold_unclosed();
    // Likewise, delimiters still open when the file ends are one wreck.
    if p.at_eof() {
        if p.eof_unclosed_reported {
            return;
        }
        p.eof_unclosed_reported = true;
    }
    let here = p.here();
    // Several openers stalling at the same token are one wreck too.
    if p.last_unclosed_at == Some(here.lo) {
        return;
    }
    p.last_unclosed_at = Some(here.lo);
    let closer = match what {
        "(" => ")",
        "[" | "#[" => "]",
        _ => "}",
    };
    p.push_diag(
        wolf_diag::Diagnostic::error(
            codes::UNCLOSED_DELIMITER,
            opener,
            format!("this `{what}` is never closed"),
        )
        .with_label("opened here")
        .with_secondary(
            here,
            format!("the parser expected the closing `{closer}` by here"),
        ),
    );
}
