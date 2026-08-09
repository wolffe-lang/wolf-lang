//! The declaration grammar (spec/01 §2, §4, §5) — resilient recursive
//! descent over the event-driven skeleton. Bodies (`{…}` after headers)
//! and initializer expressions (`= …` up to TERM) are fenced into
//! `BlockPending` / `ExprPending` raw-token groups for s09.
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
    loop {
        p.eat_terms();
        if p.at_eof() {
            break;
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
pub(crate) fn item(p: &mut Parser<'_>, in_body: bool) {
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
        TokenKind::Kw(Keyword::Fn | Keyword::Comptime | Keyword::Extern | Keyword::Export) => {
            fn_item(p, m)
        }
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
            let msg = if prefixed {
                "expected a declaration after attributes/visibility"
            } else {
                "expected a declaration"
            };
            p.error(codes::UNEXPECTED_TOPLEVEL, p.current_span(), msg);
            p.skip_until(in_body, |k| k == TokenKind::Term);
            m.complete(p, SyntaxKind::ErrorNode);
        }
    }
}

// ------------------------------------------------------------ functions --

fn fn_item(p: &mut Parser<'_>, m: Marker) {
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
                        "expected an ABI string after `extern`",
                    );
                    p.missing();
                }
            }
            _ => break,
        }
    }
    p.expect_kw(Keyword::Fn, "`fn`");
    name_token(p, "function");
    if p.at_punct(Punct::LBracket) {
        generic_param_list(p);
    }
    let params_ok = if p.at_punct(Punct::LParen) {
        param_list(p)
    } else {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `(` to start the parameter list",
        );
        p.missing();
        false
    };
    if p.at_punct(Punct::Arrow) {
        ret_type(p);
    }
    // Body: a fenced block, or TERM for the bodyless form.
    if p.at_punct(Punct::LBrace) {
        block_pending(p);
    } else if p.at(TokenKind::Term) {
        p.bump();
    } else if !p.at_eof() && !p.at_decl_start() && !p.at_punct(Punct::RBrace) {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `{` or line end after the function header",
        );
        p.recover_until(true, |k| {
            matches!(k, TokenKind::Punct(Punct::LBrace) | TokenKind::Term)
        });
        if p.at_punct(Punct::LBrace) {
            block_pending(p);
        } else if p.at(TokenKind::Term) {
            p.bump();
        }
    } else if params_ok && !p.at_eof() {
        // Header was fine but the body is missing before the next
        // declaration (same line, no TERM).
        p.error(codes::EXPECTED_TOKEN, p.here(), "expected a function body");
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
        param(p);
        if p.at_punct(Punct::Comma) {
            p.bump();
            continue;
        }
        if p.at_punct(Punct::RParen) || paren_list_escape(p) {
            continue;
        }
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `,` or `)` in the parameter list",
        );
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
                "expected a parameter name; `_` is not a parameter name",
            );
            p.bump();
            param_type(p);
        }
        TokenKind::Kw(k) if !is_decl_keyword(k) => {
            keyword_as_ident(p, "parameter");
            param_type(p);
        }
        _ => {
            p.error(codes::EXPECTED_TOKEN, p.here(), "expected a parameter");
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
    p.expect_punct(Punct::Colon, "`:` after the parameter name");
    type_required(p);
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
                    "expected a generic parameter",
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
                "expected a bound or `type` after `:`",
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
    type_required(p);
    if p.at_punct(Punct::Not) {
        p.bump();
        if p.at_punct(Punct::LBrace) {
            error_row(p);
        } else {
            p.error(
                codes::EXPECTED_TOKEN,
                p.here(),
                "expected `{` to open the error row after `!`",
            );
            p.missing();
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
        if p.at_eof() || p.at_decl_start() {
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
                if p.at_punct(Punct::LParen) {
                    paren_type_list(p);
                }
                e.complete(p, SyntaxKind::RowEntry);
            }
            _ => {
                p.error(
                    codes::EXPECTED_TOKEN,
                    p.current_span(),
                    "expected an error-row entry",
                );
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

fn binding_item(p: &mut Parser<'_>, m: Marker, kw: Keyword, kind: SyntaxKind) {
    p.bump(); // let/var/const
    if kw == Keyword::Const {
        name_token(p, "constant");
    } else if !pattern(p) {
        p.error(codes::EXPECTED_PATTERN, p.here(), "expected a pattern");
        p.missing();
    }
    if p.at_punct(Punct::Colon) {
        p.bump();
        type_required(p);
    }
    if p.at_punct(Punct::Eq) {
        p.bump();
        expr_pending(p);
    } else {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `=` and an initializer",
        );
        p.missing();
        if !p.at(TokenKind::Term) && !p.at_eof() && !p.at_decl_start() && !p.at_punct(Punct::RBrace)
        {
            p.recover_until(true, |k| {
                k == TokenKind::Term || k == TokenKind::Punct(Punct::Eq)
            });
            if p.at_punct(Punct::Eq) {
                p.bump();
                expr_pending(p);
            }
        }
    }
    if p.at(TokenKind::Term) {
        p.bump();
    } else if !p.at_eof() && !p.at_punct(Punct::RBrace) {
        // ExprPending stops only at TERM / `}` / EOF / a declaration
        // start — the latter means two declarations on one line.
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected line end after the initializer",
        );
        p.missing();
    }
    m.complete(p, kind);
}

// ----------------------------------------------------------- type items --

fn type_item(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // `type`
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

fn struct_item(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // `struct`
    name_token(p, "struct");
    if p.at_punct(Punct::LBracket) {
        generic_param_list(p);
    }
    struct_body(p);
    m.complete(p, SyntaxKind::StructDecl);
}

fn enum_item(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // `enum`
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
        match p.current() {
            TokenKind::Ident | TokenKind::PoundBracket | TokenKind::Kw(_) => struct_field(p),
            _ => {
                p.error(codes::EXPECTED_TOKEN, p.current_span(), "expected a field");
                p.recover_until(true, |k| {
                    matches!(k, TokenKind::Punct(Punct::Comma) | TokenKind::Term)
                });
                if p.at_punct(Punct::Comma) {
                    p.bump();
                }
            }
        }
        if p.pos() == before {
            unclosed(p, opener, "{");
            break;
        }
    }
}

/// `attribute* visibility? IDENT ':' type ','?`.
fn struct_field(p: &mut Parser<'_>) {
    let m = p.start();
    while p.at(TokenKind::PoundBracket) {
        attribute(p);
    }
    if p.at_kw(Keyword::Pub) {
        visibility(p);
    }
    name_token(p, "field");
    p.expect_punct(Punct::Colon, "`:` after the field name");
    type_required(p);
    if p.at_punct(Punct::Comma) {
        p.bump();
    }
    m.complete(p, SyntaxKind::StructField);
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
        match p.current() {
            TokenKind::Ident => enum_variant(p),
            TokenKind::Kw(k) if !is_decl_keyword(k) => enum_variant(p),
            _ => {
                p.error(
                    codes::EXPECTED_TOKEN,
                    p.current_span(),
                    "expected an enum variant",
                );
                p.recover_until(true, |k| {
                    matches!(k, TokenKind::Punct(Punct::Comma) | TokenKind::Term)
                });
            }
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

fn trait_item(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // `trait`
    name_token(p, "trait");
    if p.at_punct(Punct::LBracket) {
        generic_param_list(p);
    }
    member_body(p);
    m.complete(p, SyntaxKind::TraitDecl);
}

fn impl_item(p: &mut Parser<'_>, m: Marker) {
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
            "expected the implemented trait or type name",
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
            p.error(
                codes::UNEXPECTED_TOPLEVEL,
                p.current_span(),
                "expected a declaration",
            );
            p.recover_until(true, |k| k == TokenKind::Term);
        }
        if p.pos() == before {
            unclosed(p, opener, "{");
            break;
        }
    }
}

// -------------------------------------------------------- use / import --

fn use_item(p: &mut Parser<'_>, m: Marker) {
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

fn import_c_item(p: &mut Parser<'_>, m: Marker) {
    p.bump(); // `import`
    if p.at(TokenKind::Ident) {
        if p.current_text() != b"c" {
            p.error(
                codes::EXPECTED_TOKEN,
                p.current_span(),
                "expected `c` after `import` (only C headers can be imported)",
            );
        }
        p.bump();
    } else {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected `c` after `import`",
        );
        p.missing();
    }
    if p.at_str_begin() {
        string_lit(p);
    } else {
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            "expected a header name string",
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
        p.error(
            codes::EXPECTED_TOKEN,
            p.here(),
            format!("expected line end after the {what}"),
        );
        p.recover_until(true, |k| k == TokenKind::Term);
        if p.at(TokenKind::Term) {
            p.bump();
        }
    }
}

// ------------------------------------------- attributes & visibility --

/// `#[' attr (',' attr)* ']` `[gram.item.attr]`.
fn attribute(p: &mut Parser<'_>) {
    let m = p.start();
    let opener = p.current_span();
    p.bump(); // `#[`
    loop {
        if p.at_punct(Punct::RBracket) {
            p.bump();
            break;
        }
        if p.at_eof() || p.at_decl_start() || p.at_punct(Punct::LBrace) {
            unclosed(p, opener, "#[");
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
                    "malformed attribute: expected an attribute name",
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
            unclosed(p, opener, "#[");
            break;
        }
    }
    m.complete(p, SyntaxKind::Attribute);
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
                    "malformed attribute: expected a nested attribute or literal",
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
                "malformed attribute: expected a literal after `=`",
            );
            p.missing();
        }
    }
    m.complete(p, SyntaxKind::AttrInput);
}

/// `pub` / `pub(pkg)`.
fn visibility(p: &mut Parser<'_>) {
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
                "expected `pkg` in the visibility qualifier",
            );
            p.recover_until(true, |k| k == TokenKind::Punct(Punct::RParen));
        }
        p.expect_punct(Punct::RParen, "`)` to close `pub(`");
    }
    m.complete(p, SyntaxKind::Visibility);
}

// ---------------------------------------------------------------- paths --

/// `IDENT ('.' IDENT)*`. Reserved keywords in segment position get the
/// E0008 treatment.
fn path(p: &mut Parser<'_>, what: &str) {
    let m = p.start();
    if !name_token(p, what) {
        m.complete(p, SyntaxKind::Path);
        return;
    }
    while p.at_punct(Punct::Dot) {
        p.bump();
        if !name_token(p, "path segment") {
            break;
        }
    }
    m.complete(p, SyntaxKind::Path);
}

// ---------------------------------------------------------------- types --

/// Parse a type per `[gram.type]`. Returns false (emitting nothing)
/// when the current token cannot begin a type.
fn type_(p: &mut Parser<'_>) -> bool {
    match p.current() {
        TokenKind::Punct(Punct::Not) => {
            let m = p.start();
            p.bump();
            type_required(p);
            m.complete(p, SyntaxKind::ErrorUnionType);
        }
        TokenKind::Punct(Punct::Star) => {
            let m = p.start();
            p.bump();
            type_required(p);
            m.complete(p, SyntaxKind::PtrType);
        }
        TokenKind::Kw(Keyword::Shared | Keyword::Handle | Keyword::Weak | Keyword::Distinct) => {
            let m = p.start();
            p.bump();
            type_required(p);
            m.complete(p, SyntaxKind::PrefixType);
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
                    "expected a trait path after `dyn`",
                );
                p.missing();
            }
            m.complete(p, SyntaxKind::DynType);
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
                    "expected `(` in the function type",
                );
                p.missing();
            }
            if p.at_punct(Punct::Arrow) {
                ret_type(p);
            }
            m.complete(p, SyntaxKind::FnType);
        }
        TokenKind::Kw(Keyword::Type) => {
            let m = p.start();
            p.bump();
            m.complete(p, SyntaxKind::TypeType);
        }
        TokenKind::Kw(Keyword::Region) => {
            let m = p.start();
            p.bump();
            m.complete(p, SyntaxKind::RegionType);
        }
        TokenKind::Punct(Punct::LParen) => {
            let m = p.start();
            paren_type_list(p);
            m.complete(p, SyntaxKind::TupleType);
        }
        TokenKind::Ident => {
            let m = p.start();
            path(p, "type");
            if p.at_punct(Punct::LBracket) {
                type_args(p);
            }
            m.complete(p, SyntaxKind::PathType);
        }
        _ => return false,
    }
    true
}

/// [`type_`], diagnosing E0206 + Missing when no type can start here.
fn type_required(p: &mut Parser<'_>) {
    if !type_(p) {
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

/// Keywords that can never continue a type argument — reaching one at
/// depth zero means the `[` was never closed.
fn type_arg_escape(k: TokenKind) -> bool {
    matches!(
        k,
        TokenKind::Kw(
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

/// `[gram.pat]`, or-patterns wrapped via `precede`. Returns false if no
/// pattern can start here (nothing emitted).
fn pattern(p: &mut Parser<'_>) -> bool {
    let Some(first) = pattern_atom(p) else {
        return false;
    };
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
        m.complete(p, SyntaxKind::OrPat);
    }
    true
}

fn pattern_atom(p: &mut Parser<'_>) -> Option<crate::parser::CompletedMarker> {
    match p.current() {
        TokenKind::Underscore => {
            let m = p.start();
            p.bump();
            Some(m.complete(p, SyntaxKind::WildcardPat))
        }
        TokenKind::Int | TokenKind::Float | TokenKind::Kw(Keyword::True | Keyword::False) => {
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
                if !pattern(p) {
                    p.error(
                        codes::EXPECTED_PATTERN,
                        p.current_span(),
                        "expected a pattern",
                    );
                    p.recover_until(true, |k| {
                        matches!(
                            k,
                            TokenKind::Punct(Punct::Comma | Punct::RParen | Punct::Eq)
                        )
                    });
                }
                if p.at_punct(Punct::Comma) {
                    p.bump();
                }
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
            } else {
                p.error(
                    codes::EXPECTED_TOKEN,
                    p.here(),
                    "expected `(` — a dotted path pattern needs a payload",
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

/// Bounded lookahead: `Tag(pat)` / `io.Error(pat)` vs. bare `x` — the
/// `.` or `(` after the identifier decides.
fn at_path_pattern(p: &Parser<'_>) -> bool {
    matches!(p.nth(1), TokenKind::Punct(Punct::Dot | Punct::LParen))
}

/// `'(' pattern (',' pattern)* ','? ')'` after a path pattern.
fn pattern_payload(p: &mut Parser<'_>) {
    let opener = p.current_span();
    p.bump(); // `(`
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
        if !pattern(p) {
            p.error(
                codes::EXPECTED_PATTERN,
                p.current_span(),
                "expected a pattern",
            );
            p.recover_until(true, |k| {
                matches!(
                    k,
                    TokenKind::Punct(Punct::Comma | Punct::RParen | Punct::Eq)
                )
            });
        }
        if p.at_punct(Punct::Comma) {
            p.bump();
        }
        if p.pos() == before {
            unclosed(p, opener, "(");
            break;
        }
    }
}

// ------------------------------------------------------- fences (s09) --

/// `{…}` after a header: consumed by delimiter matching into a
/// `BlockPending` node holding the raw tokens — complete and lossless;
/// s09 opens it. Unbalanced delimiters degrade with a closest-match
/// heuristic and an E0202 at the unclosed opener.
fn block_pending(p: &mut Parser<'_>) {
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
    m.complete(p, SyntaxKind::BlockPending);
}

/// `= …` initializer up to a depth-zero TERM (left for the caller), `}`
/// or a declaration keyword — parked raw for s09.
fn expr_pending(p: &mut Parser<'_>) {
    let m = p.start();
    let consumed = raw_scan(p, |k| {
        matches!(
            k,
            TokenKind::Term
                | TokenKind::Punct(Punct::RBrace)
                | TokenKind::PoundBracket
                | TokenKind::Kw(
                    Keyword::Let
                        | Keyword::Var
                        | Keyword::Const
                        | Keyword::Use
                        | Keyword::Import
                        | Keyword::Type
                        | Keyword::Struct
                        | Keyword::Enum
                        | Keyword::Trait
                        | Keyword::Impl
                        | Keyword::Pub
                        | Keyword::Comptime
                        | Keyword::Extern
                        | Keyword::Export
                )
        )
    });
    if consumed == 0 {
        m.abandon(p);
        p.error(codes::EXPECTED_TOKEN, p.here(), "expected an expression");
        p.missing();
    } else {
        m.complete(p, SyntaxKind::ExprPending);
    }
}

/// Raw scan with delimiter matching: consume tokens into the current
/// node until `stop` matches at delimiter depth zero (or Eof). String
/// episodes are consumed wholesale; unclosed openers get E0202 at Eof;
/// mismatched closers use the closest-match heuristic. Note: `fn` is
/// deliberately *not* a stop in initializer scans — closures
/// (`let f = fn() { … }`) are expressions.
fn raw_scan(p: &mut Parser<'_>, stop: impl Fn(TokenKind) -> bool) -> usize {
    let mut stack: Vec<(Punct, Span)> = Vec::new();
    let mut consumed = 0usize;
    loop {
        let k = p.current();
        if k == TokenKind::Eof {
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
fn string_lit(p: &mut Parser<'_>) {
    let m = p.start();
    raw_string_episode(p);
    m.complete(p, SyntaxKind::StringLit);
}

// -------------------------------------------------------------- helpers --

fn is_str_begin(k: TokenKind) -> bool {
    matches!(k, TokenKind::StrBegin(_))
}

/// A name position: bump an `Ident`; a reserved keyword gets E0008
/// (spec §9) and is consumed as the name; anything else gets E0201 and
/// a Missing marker. Declaration-leading keywords are *not* consumed —
/// they are sync points and almost certainly start the next item.
fn name_token(p: &mut Parser<'_>, what: &str) -> bool {
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
    p.error(
        codes::KEYWORD_AS_IDENT,
        p.current_span(),
        format!(
            "`{text}` is a reserved keyword and cannot be a {what} name; \
             pick another name (wolf has no raw identifiers)"
        ),
    );
    p.bump();
}

/// E0202 at the unclosed opener, noting where scanning stopped, plus a
/// zero-width Missing marker for the absent closer.
fn unclosed(p: &mut Parser<'_>, opener: Span, what: &str) {
    unclosed_diag(p, opener, what);
    p.missing();
}

/// The E0202 diagnostic alone (closest-match pops mid-scan: the fence
/// content is raw tokens, a Missing marker there would be noise).
fn unclosed_diag(p: &mut Parser<'_>, opener: Span, what: &str) {
    let here = p.here();
    p.error_with_note(
        codes::UNCLOSED_DELIMITER,
        opener,
        format!("unclosed `{what}`"),
        here,
        "expected the closing delimiter by here",
    );
}
