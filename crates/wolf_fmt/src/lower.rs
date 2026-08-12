//! Tree → doc lowering: the canonical style, one function per shape.
//!
//! Layout summary (spec/01 §7 is the authority; decisions the spec
//! leaves open are listed in the crate docs and locked by tests):
//!
//! - Paren/bracket lists (args, params, generics, tuples) are
//!   fit-based: one line when they fit, one element per line with a
//!   trailing comma when they do not (`[gram.fmt.commas]`).
//! - Brace constructs (blocks, struct defs/literals, `asm`) respect
//!   the source's inline-vs-multiline choice: the formatter breaks
//!   them when they must break (width, comments, statement count) but
//!   never joins a multiline block onto one line. Inline blocks are
//!   the guard-clause form (`[gram.fmt.inline]`): ≤2 statements, `;`
//!   separators, within width.
//! - `match`/`select` bodies and `trait`/`impl` bodies are always
//!   multiline.
//! - Multiline expressions break *after* binary operators and after
//!   `.` (`[gram.fmt.continuation]`), continuations indented once.
//! - String episodes, inline-C bodies, and error regions are emitted
//!   byte-verbatim from the source.

use std::collections::{HashMap, HashSet};

use wolf_ast::{Child, GreenNode, GreenToken, SyntaxKind as K};
use wolf_span::Span;

use crate::doc::Doc;

// ------------------------------------------------------------ context ---

/// X4 sugar-preference rewrite: the `in r { … }` at the key span is
/// emitted as `region r(…) { … }` block sugar, and the `let` that bound
/// `r` is dropped.
pub(crate) struct Sugar {
    pub name: Vec<u8>,
    /// The `RegionStrategy` node from the value form, if any.
    pub strategy: Option<GreenNode>,
}

pub(crate) struct Fmt<'a> {
    pub src: &'a [u8],
    /// InBlock span → sugar rewrite.
    pub sugar: HashMap<(u32, u32), Sugar>,
    /// LetDecl spans elided by the sugar rewrite.
    pub drop_lets: HashSet<(u32, u32)>,
    /// Trivia spans already emitted elsewhere (the file header).
    pub consumed: std::cell::RefCell<HashSet<(u32, u32)>>,
}

/// Expression position, for redundant-paren dropping. Only `ParenExpr`
/// consults it; everything else just forwards a sensible child context.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Ctx {
    /// Positions delimited on both sides (args, rhs of `=`, tuple and
    /// list elements, arm bodies, block tails …).
    Free,
    /// Condition/scrutinee position: no-struct-literal mode.
    Cond,
    /// Operand of a binary operator of tier `t`; `left` is the
    /// left-hand side (left-associative tiers absorb equals there).
    BinOperand {
        tier: u8,
        left: bool,
        assoc_left: bool,
    },
    /// Operand of a prefix operator (tier 3).
    Prefix,
    /// Left of `as` (tier 4).
    Cast,
    /// Base of a postfix form (tier 2).
    Postfix,
    /// Range endpoint (operand must be tier ≤ 13).
    RangeEnd,
    /// `[…]` argument (also `Free`, minus the `[-1]` E0209 trap).
    IndexArg,
}

// ---------------------------------------------------------- utilities ---

pub(crate) fn first_token(n: &GreenNode) -> Option<&GreenToken> {
    for c in &n.children {
        match c {
            Child::Token(t) => return Some(t),
            Child::Node(m) => {
                if let Some(t) = first_token(m) {
                    return Some(t);
                }
            }
        }
    }
    None
}

pub(crate) fn last_token(n: &GreenNode) -> Option<&GreenToken> {
    for c in n.children.iter().rev() {
        match c {
            Child::Token(t) => return Some(t),
            Child::Node(m) => {
                if let Some(t) = last_token(m) {
                    return Some(t);
                }
            }
        }
    }
    None
}

/// Start of the node's first token's leading trivia (comments travel
/// with their construct).
fn lead_start(n: &GreenNode) -> u32 {
    match first_token(n) {
        Some(t) => t.leading.first().map_or(t.span.lo, |s| s.lo),
        None => n.span.lo,
    }
}

/// Start of the node's first *comment* (or the token itself) — the
/// position blank-line gaps are measured to.
fn comment_or_token_start(src: &[u8], n: &GreenNode) -> u32 {
    // Zero-width tokens (`Missing`, the recovery placeholder) are
    // transparent: they own no trivia and sit *after* the comment, so
    // stopping at one measured the gap across the comment's own line
    // and read it as a blank. A damaged match arm then gained a blank
    // line per pass — an idempotence class the well-formed arm hid.
    let mut toks: Vec<&GreenToken> = Vec::new();
    collect_tokens(n, &mut toks);
    for t in toks {
        if let Some(s) = t
            .leading
            .iter()
            .find(|s| src[s.lo as usize..].starts_with(b"//"))
        {
            return s.lo;
        }
        if t.span.lo < t.span.hi {
            return t.span.lo;
        }
    }
    n.span.lo
}

/// Did this trivia span start its own source line (only whitespace
/// between the previous newline, or file start, and it)?
fn own_line(src: &[u8], lo: u32) -> bool {
    let lo = lo as usize;
    src[..lo]
        .iter()
        .rev()
        .take_while(|&&b| b != b'\n')
        .all(|b| b.is_ascii_whitespace())
}

/// Source column (bytes since line start) of a position.
fn src_col(src: &[u8], pos: u32) -> usize {
    let mut i = pos as usize;
    let mut col = 0usize;
    while i > 0 && src[i - 1] != b'\n' {
        i -= 1;
        col += 1;
    }
    col
}

/// End of the node's last token's trailing trivia.
fn trail_end(n: &GreenNode) -> u32 {
    match last_token(n) {
        Some(t) => t.trailing.last().map_or(t.span.hi, |s| s.hi),
        None => n.span.hi,
    }
}

/// End of the last *non-terminator* token (with trailing trivia) — the
/// position statement-gap measurement starts from, so the terminating
/// newline (the `Term` token) counts as part of the gap.
fn code_end(n: &GreenNode) -> u32 {
    fn walk(n: &GreenNode) -> Option<u32> {
        for c in n.children.iter().rev() {
            match c {
                Child::Token(t) if t.kind == K::Term || t.kind == K::Missing => {
                    // Keep any comment attached to the `;` itself.
                    if let Some(s) = t.trailing.last() {
                        return Some(s.hi);
                    }
                }
                Child::Token(t) => {
                    return Some(t.trailing.last().map_or(t.span.hi, |s| s.hi));
                }
                Child::Node(m) => {
                    if let Some(e) = walk(m) {
                        return Some(e);
                    }
                }
            }
        }
        None
    }
    walk(n).unwrap_or(n.span.hi)
}

/// Does this subtree contain parse damage, not counting the interiors
/// of nested blocks (those repair themselves at their own level)?
fn shallow_error(n: &GreenNode) -> bool {
    if matches!(n.kind, K::ErrorNode) {
        return true;
    }
    for c in &n.children {
        match c {
            Child::Token(t) => {
                if matches!(t.kind, K::ErrorToken | K::Missing) {
                    return true;
                }
            }
            Child::Node(m) => {
                if m.kind == K::Block {
                    // A block handles its own damage — but its
                    // delimiters must be present for that to work.
                    if block_frame_damaged(m) {
                        return true;
                    }
                } else if shallow_error(m) {
                    return true;
                }
            }
        }
    }
    false
}

/// A block whose `{`/`}` frame itself is damaged cannot self-repair.
fn block_frame_damaged(b: &GreenNode) -> bool {
    let has_open = b.tokens().any(|t| t.kind == K::LBrace);
    let has_close = b.tokens().any(|t| t.kind == K::RBrace);
    !has_open || !has_close
}

fn is_comment(kind_bytes: &[u8]) -> bool {
    kind_bytes.starts_with(b"//")
}

fn trim_end(bytes: &[u8]) -> &[u8] {
    let mut hi = bytes.len();
    while hi > 0 && matches!(bytes[hi - 1], b' ' | b'\t' | b'\r') {
        hi -= 1;
    }
    &bytes[..hi]
}

/// Statement-shaped child? (Anything between `{`/`}` that is not a
/// delimiter or terminator.)
fn is_stmt_child(k: K) -> bool {
    k.is_node()
}

/// Kinds the paren-dropper never unwraps: their extent or block
/// structure interacts with the surrounding grammar in ways plain
/// precedence does not describe.
fn paren_blacklisted(k: K) -> bool {
    matches!(
        k,
        K::ClosureExpr
            | K::IfExpr
            | K::MatchExpr
            | K::ForExpr
            | K::WhileExpr
            | K::LoopExpr
            | K::Block
            | K::UnsafeBlock
            | K::InlineC
            | K::AsmExpr
            | K::ScopeExpr
            | K::SelectExpr
            | K::WhenExpr
            | K::SpawnExpr
            | K::RegionBlock
            | K::RegionValue
            | K::InBlock
            | K::FreezeExpr
            | K::BorrowExpr
            | K::StructLit
            | K::ReturnExpr
            | K::BreakExpr
            | K::ContinueExpr
            | K::FromEndExpr
            | K::ErrorNode
    )
}

/// Precedence tier of an expression node (spec/01 §3.2; lower binds
/// tighter). Unknown shapes report a tier no context accepts.
fn tier(n: &GreenNode) -> u8 {
    match n.kind {
        K::PathExpr | K::LiteralExpr | K::StringExpr | K::TupleExpr | K::ParenExpr => 1,
        K::CallExpr | K::BracketApply | K::MemberExpr | K::TryExpr => 2,
        K::PrefixExpr => 3,
        K::CastExpr => 4,
        K::BinExpr => bin_op_token(n).map_or(99, bin_tier),
        K::RangeExpr => 14,
        K::ElseExpr => 15,
        _ => 99,
    }
}

fn bin_op_token(n: &GreenNode) -> Option<K> {
    n.tokens().map(|t| t.kind).find(|k| bin_tier(*k) != 99)
}

fn bin_tier(k: K) -> u8 {
    match k {
        K::Star | K::Slash | K::Percent => 5,
        K::Plus | K::Minus => 6,
        K::Shl | K::Shr => 7,
        K::Amp => 8,
        K::Caret => 9,
        K::Pipe => 10,
        K::EqEq | K::NotEq | K::Lt | K::Gt | K::LtEq | K::GtEq | K::Spaceship => 11,
        K::AmpAmp => 12,
        K::PipePipe => 13,
        _ => 99,
    }
}

fn tier_associates_left(t: u8) -> bool {
    // 11 (comparisons) and 14 (ranges) are non-associative; `else`
    // (15) is right-associative.
    !matches!(t, 11 | 14 | 15)
}

/// Would this expression, spelled bare in no-struct-literal position,
/// expose a `{` on its spine and steal the construct's block?
fn exposed_brace(n: &GreenNode) -> bool {
    match n.kind {
        K::StructLit
        | K::Block
        | K::IfExpr
        | K::MatchExpr
        | K::ForExpr
        | K::WhileExpr
        | K::LoopExpr
        | K::UnsafeBlock
        | K::InlineC
        | K::AsmExpr
        | K::ScopeExpr
        | K::SelectExpr
        | K::WhenExpr
        | K::RegionBlock
        | K::InBlock
        | K::ClosureExpr
        | K::ErrorNode => true,
        K::BinExpr | K::RangeExpr | K::ElseExpr => n.nodes().any(exposed_brace),
        K::PrefixExpr | K::CastExpr | K::TryExpr | K::FreezeExpr | K::FromEndExpr => {
            n.nodes().next().is_some_and(exposed_brace)
        }
        K::CallExpr | K::BracketApply | K::MemberExpr => {
            // Only the base/callee side is exposed; arguments are
            // bracketed.
            n.nodes()
                .find(|c| c.kind != K::ArgList)
                .is_some_and(exposed_brace)
        }
        _ => false,
    }
}

// -------------------------------------------------------- pair spacing ---

/// Default single-space rule between two adjacent tokens, used by the
/// generic child walk. Structural lowerings override where needed.
fn pair_space(prev: K, next: K) -> bool {
    // Openers and member dots are tight on the right.
    if matches!(
        prev,
        K::LParen | K::LBracket | K::Dot | K::PoundBracket | K::DotDot | K::DotDotEq | K::Star
    ) {
        return false;
    }
    // `!` is tight on the right except before an error row's `{`.
    if prev == K::Not {
        return next == K::LBrace;
    }
    // Closers/separators are tight on the left.
    if matches!(
        next,
        K::RParen
            | K::RBracket
            | K::Comma
            | K::Question
            | K::Dot
            | K::DotDot
            | K::DotDotEq
            | K::Colon
            | K::Term
    ) {
        return prev == K::Comma && matches!(next, K::DotDot | K::DotDotEq);
    }
    // Call-shaped `(`/`[`: tight after names, literals, and closers;
    // spaced after separators, operators, and most keywords.
    if matches!(next, K::LParen | K::LBracket) {
        return !matches!(
            prev,
            K::Ident
                | K::SelfKw
                | K::Int
                | K::Float
                | K::RParen
                | K::RBracket
                | K::RBrace
                | K::Question
                | K::StrEnd
                | K::PubKw
                | K::RegionKw
                | K::FnKw
        ) && prev != K::Not;
    }
    true
}

// ------------------------------------------------------------ builder ---

impl<'a> Fmt<'a> {
    fn slice(&self, span: Span) -> &'a [u8] {
        &self.src[span.lo as usize..span.hi as usize]
    }

    fn newlines_between(&self, lo: u32, hi: u32) -> usize {
        if hi <= lo {
            return 0;
        }
        self.src[lo as usize..hi as usize]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
    }

    /// ≥1 blank line between two source positions?
    fn blank_between(&self, lo: u32, hi: u32) -> bool {
        self.newlines_between(lo, hi) >= 2
    }

    // ---------------------------------------------------------- trivia --

    /// Leading comments of `t`, each on its own line, blank gaps
    /// preserved up to one. The break *before* the first comment is the
    /// caller's separator.
    fn lead(&self, t: &GreenToken, out: &mut Vec<Doc>) {
        let mut prev_end: Option<u32> = None;
        for s in &t.leading {
            let bytes = self.slice(*s);
            if !is_comment(bytes) {
                continue;
            }
            if self.consumed.borrow().contains(&(s.lo, s.hi)) {
                continue;
            }
            if let Some(pe) = prev_end {
                if self.blank_between(pe, s.lo) {
                    out.push(Doc::Blankline);
                } else {
                    out.push(Doc::Hardline);
                }
            } else if own_line(self.src, s.lo) {
                // First comment of the run, own-line in source: it must
                // start a fresh output line even when the emitter sits
                // mid-line (expression positions push no break of their
                // own — the fmt fuzz found `!` swallowing the comment).
                out.push(Doc::FreshLine);
            }
            // Column-preserving offset for trailing-comment
            // continuations, anchored on the statement head; comments
            // dangling before a closer (or EOF) sit at the plain
            // indent.
            // The offset is a render fixed point PROVIDED the comment and
            // its anchor render at the same base indent — the layout code
            // owes that invariant (the bin-continuation crash broke it by
            // indenting the comment's line but not the operand's).
            let extra = if matches!(t.kind, K::RBrace | K::RParen | K::RBracket | K::Eof) {
                0
            } else {
                src_col(self.src, s.lo).saturating_sub(src_col(self.src, t.span.lo))
            };
            let mut text = vec![b' '; extra];
            text.extend_from_slice(trim_end(bytes));
            out.push(Doc::Text(text));
            prev_end = Some(s.hi);
        }
        if let Some(pe) = prev_end {
            if self.blank_between(pe, t.span.lo) {
                out.push(Doc::Blankline);
            } else {
                out.push(Doc::Hardline);
            }
        }
    }

    /// Trailing comment of `t` (at most one — comments run to end of
    /// line). The whitespace run before it is preserved verbatim so
    /// hand alignment survives; minimum one space.
    fn trail(&self, t: &GreenToken, out: &mut Vec<Doc>) {
        let mut ws: &[u8] = b"";
        for s in &t.trailing {
            let bytes = self.slice(*s);
            if is_comment(bytes) {
                let mut suffix = if ws.is_empty() {
                    b" ".to_vec()
                } else {
                    ws.to_vec()
                };
                suffix.extend_from_slice(trim_end(bytes));
                out.push(Doc::LineSuffix(suffix));
                ws = b"";
            } else {
                ws = bytes;
            }
        }
    }

    /// Emit a token at a bespoke site with canonical `text`: leading
    /// comments first, then the text, then the trailing comment.
    fn kw(&self, t: Option<&GreenToken>, text: &str, out: &mut Vec<Doc>) {
        if let Some(t) = t {
            self.lead(t, out);
        }
        out.push(Doc::text(text));
        if let Some(t) = t {
            self.trail(t, out);
        }
    }

    /// Emit one token: leading comments, text (overridable), trailing
    /// comment.
    fn tok(&self, t: &GreenToken, out: &mut Vec<Doc>) {
        self.lead(t, out);
        if !matches!(t.kind, K::Term | K::Missing | K::Eof) {
            out.push(Doc::Text(self.slice(t.span).to_vec()));
        }
        self.trail(t, out);
    }

    /// Trivia of a token that is itself dropped (`Term`, stripped
    /// commas): comments survive, text does not.
    fn tok_trivia_only(&self, t: &GreenToken, out: &mut Vec<Doc>) {
        self.lead(t, out);
        self.trail(t, out);
    }

    /// Would a trailing comment land mid-construct? (Those force
    /// multiline layout so no comment can slide across code.)
    ///
    /// The unit is the *direct child* — an operand of a chain, a
    /// statement of a block — not the lexical token. A trailing comment
    /// renders as a `Doc::LineSuffix`, which floats to the end of the
    /// line it lands on, so inside one child it can only ever reach
    /// that child's end: exactly where breaking would put it. Only a
    /// comment in a non-final child has code left to slide across.
    ///
    /// Attributing the comment to the token it lexically trails was an
    /// idempotence class in every shape the emitter re-homes — `x +
    /// f.//\ny` forced a break on pass one and joined on pass two, once
    /// the comment had floated onto the chain's last token. The old
    /// `PrefixExpr`-only deferral was this same rule discovered one
    /// node kind at a time; per-child subsumes it.
    fn has_inner_trailing_comment(&self, n: &GreenNode) -> bool {
        let mut flags: Vec<bool> = Vec::new();
        for c in &n.children {
            let mut toks: Vec<&GreenToken> = Vec::new();
            match c {
                Child::Token(t) => toks.push(t),
                Child::Node(m) => collect_tokens(m, &mut toks),
            }
            if toks.is_empty() {
                continue;
            }
            flags.push(toks.iter().any(|t| self.has_trail_comment(t)));
        }
        let cut = flags.len().saturating_sub(1);
        flags[..cut].iter().any(|f| *f)
    }

    fn has_lead_comment(&self, t: &GreenToken) -> bool {
        t.leading.iter().any(|s| is_comment(self.slice(*s)))
    }

    fn has_trail_comment(&self, t: &GreenToken) -> bool {
        t.trailing.iter().any(|s| is_comment(self.slice(*s)))
    }

    fn subtree_has_comment(&self, n: &GreenNode) -> bool {
        let mut toks: Vec<&GreenToken> = Vec::new();
        collect_tokens(n, &mut toks);
        toks.iter().any(|t| {
            t.leading.iter().any(|s| is_comment(self.slice(*s)))
                || t.trailing.iter().any(|s| is_comment(self.slice(*s)))
        })
    }

    // --------------------------------------------------------- verbatim --

    /// Byte-identical pass-through of a statement/item, leading
    /// comments included (Target 3).
    /// Where a verbatim run over `n` may begin: [`lead_start`], but
    /// never inside trivia another emitter already consumed (the `//!`
    /// module-header block is the only such emitter today).
    fn unconsumed_start(&self, n: &GreenNode) -> u32 {
        let start = lead_start(n);
        let Some(t) = first_token(n) else {
            return start;
        };
        let consumed = self.consumed.borrow();
        t.leading
            .iter()
            .find(|s| !consumed.contains(&(s.lo, s.hi)))
            .map_or_else(|| t.span.lo, |s| s.lo)
            .max(start)
    }

    fn verbatim(&self, from: u32, to: u32) -> Doc {
        let mut lo = from as usize;
        let hi = to as usize;
        // Skip the indentation of the first line; the renderer supplies
        // the current indent.
        while lo < hi && matches!(self.src[lo], b' ' | b'\t' | b'\r' | b'\n') {
            lo += 1;
        }
        let mut bytes = self.src[lo..hi].to_vec();
        while matches!(bytes.last(), Some(b'\n' | b' ' | b'\t' | b'\r')) {
            bytes.pop();
        }
        Doc::Raw(bytes)
    }

    // ------------------------------------------------------ entry point --

    pub(crate) fn source_file(&self, root: &GreenNode) -> Doc {
        let mut out = Vec::new();

        // The `//!` header block stays at the very top, untouched.
        let mut header_end: Option<u32> = None;
        if let Some(first) = first_token_of_children(root) {
            let mut consumed = self.consumed.borrow_mut();
            for s in &first.leading {
                let bytes = self.slice(*s);
                if bytes.starts_with(b"//!") {
                    consumed.insert((s.lo, s.hi));
                    header_end = Some(s.hi);
                } else if is_comment(bytes) {
                    break;
                } else if let Some(he) = header_end {
                    // Whitespace after the last header line: stop if a
                    // blank separates header from what follows.
                    if self.blank_between(he, s.hi) {
                        break;
                    }
                }
            }
            drop(consumed);
            let mut first_line = true;
            for s in &first.leading {
                if !self.consumed.borrow().contains(&(s.lo, s.hi)) {
                    continue;
                }
                if !first_line {
                    out.push(Doc::Hardline);
                }
                out.push(Doc::Text(trim_end(self.slice(*s)).to_vec()));
                first_line = false;
            }
        }

        // Partition items; imports sort to the front
        // (`[gram.fmt.imports]`).
        let items: Vec<&GreenNode> = root.children.iter().filter_map(as_node).collect();
        let any_shallow = items.iter().any(|n| shallow_error(n));
        let mut uses: Vec<&GreenNode> = Vec::new();
        let mut imports: Vec<&GreenNode> = Vec::new();
        let mut rest: Vec<&GreenNode> = Vec::new();
        for it in &items {
            match it.kind {
                K::UseDecl if !any_shallow && !shallow_error(it) => uses.push(it),
                K::ImportCDecl if !any_shallow && !shallow_error(it) => imports.push(it),
                _ => rest.push(it),
            }
        }
        uses.sort_by_key(|a| self.use_sort_key(a));
        imports.sort_by_key(|a| self.import_sort_key(a));

        let ordered: Vec<(&GreenNode, bool)> = uses
            .iter()
            .chain(imports.iter())
            .map(|n| (*n, true))
            .chain(rest.iter().map(|n| (*n, false)))
            .collect();

        // One-node verbatim margin around damaged items.
        let broken = margin_set(&ordered.iter().map(|(n, _)| *n).collect::<Vec<_>>());

        let mut first = true;
        let mut i = 0usize;
        while i < ordered.len() {
            let (item, is_import) = ordered[i];
            if !first {
                let prev_import = i > 0 && ordered[i - 1].1;
                if is_import && prev_import {
                    out.push(Doc::Hardline);
                } else {
                    out.push(Doc::Blankline);
                }
            } else if header_end.is_some() {
                out.push(Doc::Blankline);
            }
            first = false;
            if broken.contains(&i) {
                // Coalesce an adjacent verbatim run into one raw block
                // so inter-statement bytes stay identical.
                let start = i;
                let mut end = i;
                while end + 1 < ordered.len() && broken.contains(&(end + 1)) {
                    end += 1;
                }
                // `unconsumed_start`, not `lead_start`: the `//!`
                // header block already emitted the module doc comments,
                // and they live in the FIRST item's leading trivia. A
                // verbatim run from `lead_start` re-emitted them, so
                // the comment showed up twice — pass one got away with
                // it because a *different* comment went missing in the
                // same output and the multiset balanced, and pass two
                // then failed the formatter's own self-check.
                out.push(self.verbatim(
                    self.unconsumed_start(ordered[start].0),
                    trail_end(ordered[end].0),
                ));
                i = end + 1;
                continue;
            }
            self.item(item, &mut out);
            i += 1;
        }

        // Trailing comments at end of file.
        for c in &root.children {
            if let Child::Token(t) = c
                && t.kind == K::Eof
            {
                let mut tail = Vec::new();
                self.lead(t, &mut tail);
                if !tail.is_empty() {
                    // Two separate idempotence classes met here.
                    //
                    // The anchor: the last item, or — in a file that is
                    // nothing but comments — the end of the `//!`
                    // header. Without the header arm the blank after
                    // the header was dropped, and since the header
                    // block absorbs one more `//!` per pass (it stops
                    // at the first blank) the loss took two passes to
                    // converge.
                    //
                    // And `code_end`, not `trail_end`: the gap must be
                    // measured from the last *code* byte so the item's
                    // own terminating newline counts as part of it.
                    // From `trail_end` it ate one newline per pass —
                    // two blank lines became one, then none.
                    let prev = ordered.last().map(|(n, _)| code_end(n)).or(header_end);
                    if let Some(prev) = prev {
                        // Preserve blank (capped) before the tail
                        // comments, measured from the first comment the
                        // header did not consume.
                        let lo = t
                            .leading
                            .iter()
                            .find(|s| {
                                is_comment(self.slice(**s))
                                    && !self.consumed.borrow().contains(&(s.lo, s.hi))
                            })
                            .map(|s| s.lo)
                            .unwrap_or(t.span.lo);
                        if self.blank_between(prev, lo) {
                            out.push(Doc::Blankline);
                        } else {
                            out.push(Doc::Hardline);
                        }
                    }
                    out.append(&mut tail);
                }
            }
        }
        Doc::Concat(out)
    }

    /// The token text of a subtree with trivia erased — the *formatted*
    /// spelling, not the source bytes.
    ///
    /// Sort keys must be invariant under formatting or the sort is not
    /// a fixpoint: keying on the raw source slice let `use//\ne` and
    /// its own formatted form `use e` compare differently, so pass one
    /// and pass two ordered the imports differently. Whitespace and
    /// comments cannot decide an import's place.
    fn code_key(&self, n: &GreenNode) -> Vec<u8> {
        let mut toks: Vec<&GreenToken> = Vec::new();
        collect_tokens(n, &mut toks);
        let mut out = Vec::new();
        for t in toks {
            if matches!(t.kind, K::Term | K::Missing | K::Eof) {
                continue;
            }
            out.extend_from_slice(self.slice(t.span));
        }
        out
    }

    fn use_sort_key(&self, n: &GreenNode) -> (u8, Vec<u8>) {
        let path = n
            .child_node(K::Path)
            .map(|p| self.code_key(p))
            .unwrap_or_default();
        let std = path == b"std" || path.starts_with(b"std.");
        (u8::from(!std), self.code_key(n))
    }

    fn import_sort_key(&self, n: &GreenNode) -> Vec<u8> {
        n.child_node(K::StringLit)
            .map(|s| self.code_key(s))
            .unwrap_or_default()
    }

    // ------------------------------------------------------- statements --

    fn item(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        self.stmt(n, out);
    }

    fn stmt(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        match n.kind {
            K::ExprStmt => self.walk_children(n, out, Ctx::Free),
            K::AssignStmt => {
                let mut it = n.nodes();
                let place = it.next();
                let rhs = it.next();
                if let Some(p) = place {
                    self.expr(p, out, Ctx::Postfix);
                }
                if let Some(op) = n.tokens().find(|t| !matches!(t.kind, K::Term | K::Missing)) {
                    out.push(Doc::text(" "));
                    self.tok(op, out);
                    out.push(Doc::text(" "));
                }
                if let Some(r) = rhs {
                    self.expr(r, out, Ctx::Free);
                }
                self.leftover_term_trivia(n, out);
            }
            K::DeferStmt | K::AssumeStmt => self.walk_children(n, out, Ctx::Free),
            _ => self.node(n, out, Ctx::Free),
        }
    }

    /// Emit trivia of trailing `Term`s that the structural lowerings
    /// skip.
    fn leftover_term_trivia(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        for c in &n.children {
            if let Child::Token(t) = c
                && matches!(t.kind, K::Term | K::Missing)
            {
                self.tok_trivia_only(t, out);
            }
        }
    }

    // ------------------------------------------------------------ block --

    fn block(&self, b: &GreenNode, out: &mut Vec<Doc>) {
        let Some(lbrace) = b.child_token(K::LBrace) else {
            // Damaged frame: pass through.
            out.push(self.verbatim(lead_start(b), trail_end(b)));
            return;
        };
        let rbrace = b.tokens().filter(|t| t.kind == K::RBrace).last();
        let stmts: Vec<&GreenNode> = b
            .children
            .iter()
            .filter_map(as_node)
            .filter(|n| is_stmt_child(n.kind))
            .collect();

        // Empty block.
        if stmts.is_empty() {
            let dangling = rbrace.is_some_and(|r| self.has_lead_comment(r));
            if !dangling {
                self.tok(lbrace, out);
                out.push(Doc::text("}"));
                if let Some(r) = rbrace {
                    self.trail(r, out);
                }
                return;
            }
        }

        let src_inline =
            rbrace.is_some_and(|r| self.newlines_between(lbrace.span.hi, r.span.lo) == 0);

        // Build statement docs (with one-node verbatim margins around
        // damage).
        let broken = margin_set(&stmts);
        let mut bodies: Vec<Doc> = Vec::new();
        let mut seps: Vec<Doc> = Vec::new(); // separator BEFORE stmt i
        {
            let mut i = 0usize;
            let mut prev_end = lbrace
                .trailing
                .last()
                .map(|s| s.hi)
                .unwrap_or(lbrace.span.hi);
            while i < stmts.len() {
                let lo = comment_or_token_start(self.src, stmts[i]);
                seps.push(if self.blank_between(prev_end, lo) {
                    Doc::Blankline
                } else {
                    Doc::Hardline
                });
                if broken.contains(&i) {
                    let start = i;
                    let mut end = i;
                    while end + 1 < stmts.len() && broken.contains(&(end + 1)) {
                        end += 1;
                    }
                    bodies.push(self.verbatim(lead_start(stmts[start]), trail_end(stmts[end])));
                    for j in start + 1..=end {
                        seps.push(Doc::Hardline); // placeholder, unused
                        bodies.push(Doc::Concat(Vec::new()));
                        let _ = j;
                    }
                    prev_end = code_end(stmts[end]);
                    i = end + 1;
                    continue;
                }
                let mut d = Vec::new();
                if self
                    .drop_lets
                    .contains(&(stmts[i].span.lo, stmts[i].span.hi))
                {
                    bodies.push(Doc::Concat(Vec::new()));
                    prev_end = code_end(stmts[i]);
                    i += 1;
                    // The separator before a dropped stmt is dropped
                    // too.
                    seps.pop();
                    seps.push(Doc::Concat(Vec::new()));
                    continue;
                }
                self.stmt(stmts[i], &mut d);
                bodies.push(Doc::Concat(d));
                prev_end = code_end(stmts[i]);
                i += 1;
            }
        }

        let live: Vec<(&Doc, &Doc)> = seps
            .iter()
            .zip(bodies.iter())
            .filter(|(_, b)| !matches!(b, Doc::Concat(v) if v.is_empty()))
            .collect();

        let inline_ok = src_inline
            && live.len() <= 2
            && !live.iter().any(|(_, b)| b.forced())
            && !self.has_inner_trailing_comment(b)
            && rbrace.is_some_and(|r| !self.has_lead_comment(r));

        if inline_ok && !live.is_empty() {
            let mut g = Vec::new();
            self.lead(lbrace, &mut g);
            g.push(Doc::text("{"));
            self.trail(lbrace, &mut g);
            let mut inner = Vec::new();
            for (i, (_, body)) in live.iter().enumerate() {
                if i > 0 {
                    inner.push(Doc::IfBreak {
                        broken: Vec::new(),
                        flat: b";".to_vec(),
                    });
                }
                inner.push(Doc::Line);
                inner.push((*body).clone());
            }
            g.push(Doc::Indent(inner));
            g.push(Doc::Line);
            g.push(Doc::text("}"));
            if let Some(r) = rbrace {
                self.trail(r, out);
            }
            out.push(Doc::Group(g));
            return;
        }

        // Multiline form.
        self.lead(lbrace, out);
        out.push(Doc::text("{"));
        self.trail(lbrace, out);
        let mut inner = Vec::new();
        for (sep, body) in live {
            inner.push(sep.clone());
            inner.push(body.clone());
        }
        // Dangling comments before `}`.
        if let Some(r) = rbrace {
            let mut dangle = Vec::new();
            self.lead(r, &mut dangle);
            if !dangle.is_empty() {
                // `lead` ends with a break; we supply the one before.
                let first_lo = r
                    .leading
                    .iter()
                    .find(|s| is_comment(self.slice(**s)))
                    .map(|s| s.lo);
                let prev = stmts.last().map(|s| code_end(s)).unwrap_or(lbrace.span.hi);
                inner.push(if first_lo.is_some_and(|lo| self.blank_between(prev, lo)) {
                    Doc::Blankline
                } else {
                    Doc::Hardline
                });
                // Drop lead()'s trailing break; the block supplies it.
                if matches!(dangle.last(), Some(Doc::Hardline | Doc::Blankline)) {
                    dangle.pop();
                }
                inner.append(&mut dangle);
            }
        }
        out.push(Doc::Indent(inner));
        out.push(Doc::Hardline);
        out.push(Doc::text("}"));
        if let Some(r) = rbrace {
            self.trail(r, out);
        }
        out.push(Doc::BreakParent);
    }

    // ------------------------------------------------------------ lists --

    /// Delimited, comma-separated list. `pad`: spaces inside braces
    /// when flat. `force_break`: always multiline. Trailing comma when
    /// broken, none when flat (`[gram.fmt.commas]`).
    #[allow(clippy::too_many_arguments)]
    fn list(
        &self,
        open: &GreenToken,
        close: Option<&GreenToken>,
        elems: &[&GreenNode],
        commas: &[&GreenToken],
        pad: bool,
        force_break: bool,
        one_tuple: bool,
        out: &mut Vec<Doc>,
        elem_ctx: Ctx,
    ) {
        let mut g = Vec::new();
        self.lead(open, &mut g);
        g.push(Doc::Text(self.slice(open.span).to_vec()));
        self.trail(open, &mut g);
        if elems.is_empty() {
            if let Some(c) = close {
                self.tok(c, &mut g);
            }
            out.push(Doc::Group(g));
            return;
        }
        let mut inner = Vec::new();
        inner.push(if pad { Doc::Line } else { Doc::Softline });
        for (i, e) in elems.iter().enumerate() {
            if i > 0 {
                inner.push(Doc::text(","));
                if let Some(c) = commas.get(i - 1) {
                    self.tok_trivia_only(c, &mut inner);
                }
                inner.push(Doc::Line);
            }
            self.expr(e, &mut inner, elem_ctx);
        }
        if one_tuple && elems.len() == 1 {
            inner.push(Doc::text(","));
        } else {
            inner.push(Doc::IfBreak {
                broken: b",".to_vec(),
                flat: Vec::new(),
            });
        }
        // A comma trailing the last element in source may carry trivia.
        if commas.len() >= elems.len()
            && let Some(c) = commas.last()
        {
            self.tok_trivia_only(c, &mut inner);
        }
        g.push(Doc::Indent(inner));
        g.push(if pad { Doc::Line } else { Doc::Softline });
        if let Some(c) = close {
            self.tok(c, &mut g);
        }
        if force_break {
            g.push(Doc::BreakParent);
        }
        // Comments anywhere inside (except trailing the closer) force
        // the broken layout.
        let inner_comment = elems.iter().any(|e| self.subtree_has_comment(e))
            || commas.iter().any(|c| self.subtree_token_has_comment(c))
            || self.has_lead_comment_opt(close);
        if inner_comment {
            g.push(Doc::BreakParent);
        }
        out.push(Doc::Group(g));
    }

    fn subtree_token_has_comment(&self, t: &GreenToken) -> bool {
        t.leading.iter().any(|s| is_comment(self.slice(*s)))
            || t.trailing.iter().any(|s| is_comment(self.slice(*s)))
    }

    fn has_lead_comment_opt(&self, t: Option<&GreenToken>) -> bool {
        t.is_some_and(|t| self.has_lead_comment(t))
    }

    /// Split a delimited node's children into (open, elems, commas,
    /// close).
    #[allow(clippy::type_complexity)]
    fn split_list<'n>(
        &self,
        n: &'n GreenNode,
        open_kind: K,
        close_kind: K,
    ) -> (
        Option<&'n GreenToken>,
        Vec<&'n GreenNode>,
        Vec<&'n GreenToken>,
        Option<&'n GreenToken>,
    ) {
        let mut open = None;
        let mut close = None;
        let mut elems = Vec::new();
        let mut commas = Vec::new();
        for c in &n.children {
            match c {
                Child::Token(t) if t.kind == open_kind && open.is_none() => open = Some(t),
                Child::Token(t) if t.kind == close_kind => close = Some(t),
                Child::Token(t) if t.kind == K::Comma => commas.push(t),
                Child::Token(t) if matches!(t.kind, K::Term | K::Missing) => {}
                Child::Token(_) => {}
                Child::Node(m) => elems.push(m),
            }
        }
        (open, elems, commas, close)
    }

    // -------------------------------------------------------- expr walk --

    /// Generic child walk with pair spacing — the fallback for simple
    /// sequences (paths, patterns, types, small declarations).
    fn walk_children(&self, n: &GreenNode, out: &mut Vec<Doc>, ctx: Ctx) {
        let mut prev: Option<K> = None;
        for c in &n.children {
            match c {
                Child::Token(t) => {
                    if matches!(t.kind, K::Term | K::Missing | K::Eof) {
                        self.tok_trivia_only(t, out);
                        continue;
                    }
                    if let Some(p) = prev
                        && pair_space(p, t.kind)
                    {
                        out.push(Doc::text(" "));
                    }
                    self.tok(t, out);
                    prev = Some(t.kind);
                }
                Child::Node(m) => {
                    if m.kind == K::Attribute {
                        self.node(m, out, Ctx::Free);
                        out.push(Doc::Hardline);
                        prev = None;
                        continue;
                    }
                    let first = first_token(m).map(|t| t.kind);
                    if let (Some(p), Some(f)) = (prev, first)
                        && self.child_space(p, f, m.kind)
                    {
                        out.push(Doc::text(" "));
                    }
                    self.node(m, out, child_ctx(n.kind, m, ctx));
                    prev = last_token(m).map(|t| t.kind);
                }
            }
        }
    }

    fn child_space(&self, prev: K, first: K, node_kind: K) -> bool {
        match node_kind {
            // Attached lists never take a leading space.
            K::ParamList
            | K::GenericParamList
            | K::TypeArgList
            | K::ArgList
            | K::ViewSet
            | K::UseGroup
            | K::AttrInput
            | K::TypeArgPending => {
                first == K::Eq && pair_space(prev, first) // `#[k = v]`
            }
            K::CaptureList => true,
            _ => pair_space(prev, first),
        }
    }

    fn expr(&self, n: &GreenNode, out: &mut Vec<Doc>, ctx: Ctx) {
        self.node(n, out, ctx);
    }

    // -------------------------------------------------------- dispatch --

    fn node(&self, n: &GreenNode, out: &mut Vec<Doc>, ctx: Ctx) {
        match n.kind {
            K::SourceFile => out.push(self.source_file(n)),
            K::ErrorNode => out.push(self.verbatim(lead_start(n), trail_end(n))),
            K::Block => self.block(n, out),

            // ------------------------------------------------- items --
            K::FnDecl
            | K::TypeDecl
            | K::StructDecl
            | K::EnumDecl
            | K::TraitDecl
            | K::ImplDecl
            | K::LetDecl
            | K::VarDecl
            | K::ConstDecl
            | K::UseDecl
            | K::ImportCDecl => self.decl(n, out),

            K::Attribute => {
                // `#[a, b]` — tight to the brackets, spaced list.
                let mut prev: Option<K> = None;
                for c in &n.children {
                    match c {
                        Child::Token(t) => {
                            if let Some(p) = prev {
                                if t.kind == K::Comma || p == K::PoundBracket {
                                    // tight
                                } else if pair_space(p, t.kind) {
                                    out.push(Doc::text(" "));
                                }
                            }
                            self.tok(t, out);
                            prev = Some(t.kind);
                        }
                        Child::Node(m) => {
                            if prev == Some(K::Comma) {
                                out.push(Doc::text(" "));
                            }
                            self.node(m, out, Ctx::Free);
                            prev = last_token(m).map(|t| t.kind);
                        }
                    }
                }
            }

            // ------------------------------------------------- lists --
            K::ArgList => {
                let bracket = n.child_token(K::LBracket).is_some();
                let (open, elems, commas, close) = if bracket {
                    self.split_list(n, K::LBracket, K::RBracket)
                } else {
                    self.split_list(n, K::LParen, K::RParen)
                };
                let Some(open) = open else {
                    self.walk_children(n, out, ctx);
                    return;
                };
                let ectx = if bracket { Ctx::IndexArg } else { Ctx::Free };
                // Hug a trailing braced argument: keep the argument
                // list flat and let the block break inside.
                let hug = !elems.is_empty()
                    && elems.last().is_some_and(|e| huggable(e))
                    && elems[..elems.len() - 1]
                        .iter()
                        .all(|e| !self.doc_of(e, ectx).forced() && !self.subtree_has_comment(e))
                    && !self.subtree_token_has_comment(open)
                    && commas
                        .iter()
                        .take(elems.len().saturating_sub(1))
                        .all(|c| !self.subtree_token_has_comment(c));
                if hug {
                    let mut d = Vec::new();
                    self.lead(open, &mut d);
                    d.push(Doc::Text(self.slice(open.span).to_vec()));
                    self.trail(open, &mut d);
                    for (i, e) in elems.iter().enumerate() {
                        if i > 0 {
                            d.push(Doc::text(", "));
                        }
                        self.expr(e, &mut d, ectx);
                        if i + 1 < elems.len()
                            && let Some(c) = commas.get(i)
                        {
                            self.tok_trivia_only(c, &mut d);
                        }
                    }
                    if let Some(c) = close {
                        self.tok(c, &mut d);
                    }
                    out.push(Doc::Shield(d));
                    return;
                }
                self.list(open, close, &elems, &commas, false, false, false, out, ectx);
            }
            K::ParamList | K::GenericParamList | K::TypeArgList | K::CaptureList => {
                let (open, elems, commas, close) = if n.child_token(K::LParen).is_some() {
                    self.split_list(n, K::LParen, K::RParen)
                } else {
                    self.split_list(n, K::LBracket, K::RBracket)
                };
                if let Some(open) = open {
                    // CaptureList has bare Ident children (tokens);
                    // handle via generic walk when no element nodes.
                    if elems.is_empty() && n.kind == K::CaptureList {
                        self.capture_list(n, out);
                        return;
                    }
                    self.list(
                        open,
                        close,
                        &elems,
                        &commas,
                        false,
                        false,
                        false,
                        out,
                        Ctx::Free,
                    );
                } else {
                    self.walk_children(n, out, ctx);
                }
            }
            K::TupleExpr | K::TuplePat | K::TupleType => {
                let (open, elems, commas, close) = self.split_list(n, K::LParen, K::RParen);
                if let Some(open) = open {
                    let one = elems.len() == 1;
                    self.list(
                        open,
                        close,
                        &elems,
                        &commas,
                        false,
                        false,
                        one,
                        out,
                        Ctx::Free,
                    );
                } else {
                    self.walk_children(n, out, ctx);
                }
            }
            K::UseGroup | K::ViewSet => {
                // `.{fs, net}` — tight, single line.
                self.tight_brace_idents(n, out);
            }
            K::ErrorRow => {
                let (open, elems, commas, close) = self.split_list(n, K::LBrace, K::RBrace);
                if let Some(open) = open {
                    // `..` open-row marker is a token; re-emit by hand.
                    if n.tokens().any(|t| matches!(t.kind, K::DotDot)) {
                        self.walk_children(n, out, ctx);
                    } else {
                        self.list(
                            open,
                            close,
                            &elems,
                            &commas,
                            false,
                            false,
                            false,
                            out,
                            Ctx::Free,
                        );
                    }
                } else {
                    self.walk_children(n, out, ctx);
                }
            }
            K::StructLit => {
                let mut it = n.nodes();
                let path = it.next();
                if let Some(p) = path {
                    self.node(p, out, Ctx::Postfix);
                }
                out.push(Doc::text(" "));
                let (open, elems, commas, close) = self.split_list(n, K::LBrace, K::RBrace);
                if let Some(open) = open {
                    let fields: Vec<&GreenNode> = elems
                        .into_iter()
                        .filter(|e| e.kind == K::FieldInit)
                        .collect();
                    let force = self.brace_multiline(open, close);
                    self.list(
                        open,
                        close,
                        &fields,
                        &commas,
                        true,
                        force,
                        false,
                        out,
                        Ctx::Free,
                    );
                } else {
                    self.walk_children(n, out, ctx);
                }
            }

            // ------------------------------------------------- exprs --
            K::ParenExpr => self.paren(n, out, ctx),
            K::BinExpr => self.bin_chain(n, out),
            K::CallExpr | K::BracketApply | K::MemberExpr | K::TryExpr => {
                self.postfix_chain(n, out)
            }
            K::PrefixExpr => {
                let mut prev: Option<&GreenToken> = None;
                // A trailing comment on a prefix OPERATOR reattaches after
                // the operand: emitting it at the operator leaves a comment
                // between `!` and its operand that the next format pass
                // joins differently (the fmt fuzz idempotence neighbor).
                let mut deferred: Vec<&GreenToken> = Vec::new();
                for c in &n.children {
                    match c {
                        Child::Token(t) => {
                            if let Some(p) = prev {
                                if matches!(p.kind, K::Amp) && t.kind == K::MutKw {
                                    // `&mut` tight
                                } else if is_word_op(p.kind) {
                                    out.push(Doc::text(" "));
                                }
                            }
                            self.lead(t, out);
                            if !matches!(t.kind, K::Term | K::Missing | K::Eof) {
                                out.push(Doc::Text(self.slice(t.span).to_vec()));
                            }
                            deferred.push(t);
                            prev = Some(t);
                        }
                        Child::Node(m) => {
                            if let Some(p) = prev
                                && is_word_op(p.kind)
                            {
                                out.push(Doc::text(" "));
                            }
                            self.expr(m, out, Ctx::Prefix);
                            for t in deferred.drain(..) {
                                self.trail(t, out);
                            }
                        }
                    }
                }
                // Recovery shape with no operand node: the trails must
                // still land somewhere.
                for t in deferred.drain(..) {
                    self.trail(t, out);
                }
            }
            K::CastExpr => {
                let mut it = n.nodes();
                if let Some(lhs) = it.next() {
                    self.expr(lhs, out, Ctx::Cast);
                }
                out.push(Doc::text(" "));
                self.kw(n.child_token(K::AsKw), "as", out);
                out.push(Doc::text(" "));
                if let Some(ty) = it.next() {
                    self.node(ty, out, Ctx::Free);
                }
            }
            K::RangeExpr => {
                // Tight: `a..b`, `..=n`, `a..`.
                let mut prev: Option<K> = None;
                for c in &n.children {
                    match c {
                        Child::Token(t) => {
                            self.tok(t, out);
                            prev = Some(t.kind);
                        }
                        Child::Node(m) => {
                            self.expr(m, out, Ctx::RangeEnd);
                            prev = last_token(m).map(|t| t.kind);
                        }
                    }
                }
                let _ = prev;
            }
            K::FromEndExpr => {
                for c in &n.children {
                    match c {
                        Child::Token(t) => self.tok(t, out),
                        Child::Node(m) => self.expr(m, out, Ctx::Prefix),
                    }
                }
            }
            K::ElseExpr => {
                // `expr else expr` / `expr else |p| body` (tier 15).
                let mut nodes = n.nodes();
                let lhs = nodes.next();
                if let Some(l) = lhs {
                    self.expr(
                        l,
                        out,
                        Ctx::BinOperand {
                            tier: 15,
                            left: true,
                            assoc_left: false,
                        },
                    );
                }
                out.push(Doc::text(" "));
                self.kw(n.child_token(K::ElseKw), "else", out);
                out.push(Doc::text(" "));
                let pipes: Vec<&GreenToken> = n.tokens().filter(|t| t.kind == K::Pipe).collect();
                let mut rest: Vec<&GreenNode> = nodes.collect();
                if pipes.len() == 2 && !rest.is_empty() {
                    out.push(Doc::text("|"));
                    let pat = rest.remove(0);
                    self.node(pat, out, Ctx::Free);
                    out.push(Doc::text("| "));
                }
                for r in rest {
                    self.expr(
                        r,
                        out,
                        Ctx::BinOperand {
                            tier: 15,
                            left: false,
                            assoc_left: false,
                        },
                    );
                }
            }
            K::IfExpr => self.if_expr(n, out),
            K::MatchExpr => self.match_like(n, out, K::MatchKw),
            K::SelectExpr => self.match_like(n, out, K::SelectKw),
            K::MatchArm | K::SelectArm => self.arm(n, out),
            K::WhileExpr => {
                self.kw(n.child_token(K::WhileKw), "while", out);
                out.push(Doc::text(" "));
                let mut it = n.nodes();
                if let Some(cond) = it.next() {
                    self.expr(cond, out, Ctx::Cond);
                }
                out.push(Doc::text(" "));
                if let Some(b) = it.next() {
                    self.node(b, out, Ctx::Free);
                }
            }
            K::ForExpr => {
                self.kw(n.child_token(K::ForKw), "for", out);
                out.push(Doc::text(" "));
                let mut it = n.nodes();
                if let Some(pat) = it.next() {
                    self.node(pat, out, Ctx::Free);
                }
                out.push(Doc::text(" "));
                self.kw(n.child_token(K::InKw), "in", out);
                out.push(Doc::text(" "));
                if let Some(iter) = it.next() {
                    self.expr(iter, out, Ctx::Cond);
                }
                out.push(Doc::text(" "));
                if let Some(b) = it.next() {
                    self.node(b, out, Ctx::Free);
                }
            }
            K::LoopExpr | K::UnsafeBlock | K::ScopeExpr => self.kw_block(n, out),
            K::WhenExpr => {
                self.kw(n.child_token(K::WhenKw), "when", out);
                out.push(Doc::text(" "));
                let (open, elems, commas, close) = self.split_list(n, K::LParen, K::RParen);
                let elems: Vec<&GreenNode> =
                    elems.into_iter().filter(|e| e.kind != K::Block).collect();
                if let Some(open) = open {
                    self.list(
                        open,
                        close,
                        &elems,
                        &commas,
                        false,
                        false,
                        false,
                        out,
                        Ctx::Free,
                    );
                }
                out.push(Doc::text(" "));
                if let Some(b) = n.child_node(K::Block) {
                    self.block(b, out);
                }
            }
            K::RegionBlock => {
                // `region name? (: strategy)? { … }`
                let mut d = Vec::new();
                self.kw(n.child_token(K::RegionKw), "region", &mut d);
                if let Some(name) = n.child_token(K::Ident) {
                    d.push(Doc::text(" "));
                    self.tok(name, &mut d);
                }
                if let Some(strategy) = n.child_node(K::RegionStrategy) {
                    self.kw(n.child_token(K::Colon), ":", &mut d);
                    d.push(Doc::text(" "));
                    self.node(strategy, &mut d, Ctx::Free);
                }
                d.push(Doc::text(" "));
                if let Some(b) = n.child_node(K::Block) {
                    self.block(b, &mut d);
                }
                out.push(Doc::Concat(d));
            }
            K::InBlock => {
                if let Some(sugar) = self.sugar.get(&(n.span.lo, n.span.hi)) {
                    // X4 sugar preference: `in r { … }` (with its `let`
                    // dropped) becomes `region r(…) { … }`.
                    let mut d = Vec::new();
                    d.push(Doc::text("region "));
                    d.push(Doc::Text(sugar.name.clone()));
                    if let Some(s) = &sugar.strategy {
                        d.push(Doc::text(": "));
                        self.node(s, &mut d, Ctx::Free);
                    }
                    d.push(Doc::text(" "));
                    if let Some(b) = n.child_node(K::Block) {
                        self.block(b, &mut d);
                    }
                    out.push(Doc::Concat(d));
                    return;
                }
                self.kw(n.child_token(K::InKw), "in", out);
                out.push(Doc::text(" "));
                let mut it = n.nodes();
                if let Some(r) = it.next() {
                    self.expr(r, out, Ctx::Cond);
                }
                out.push(Doc::text(" "));
                if let Some(b) = it.next() {
                    self.node(b, out, Ctx::Free);
                }
            }
            K::AsmExpr => self.asm(n, out),
            K::InlineC => {
                // `unsafe c [caps] { … }` — the body is opaque bytes.
                let mut prev: Option<K> = None;
                for c in &n.children {
                    match c {
                        Child::Token(t) => {
                            if prev.is_some() {
                                out.push(Doc::text(" "));
                            }
                            self.tok(t, out);
                            prev = Some(t.kind);
                        }
                        Child::Node(m) if m.kind == K::InlineCBody => {
                            out.push(Doc::text(" "));
                            out.push(self.verbatim(m.span.lo, trail_end(m)));
                            prev = Some(K::RBrace);
                        }
                        Child::Node(m) => {
                            out.push(Doc::text(" "));
                            self.node(m, out, Ctx::Free);
                            prev = last_token(m).map(|t| t.kind);
                        }
                    }
                }
            }
            K::InlineCBody => out.push(self.verbatim(n.span.lo, trail_end(n))),

            // Strings: whole episodes are verbatim (never re-flowed,
            // never broken mid-token).
            K::StringExpr | K::StringLit => {
                if let (Some(first), Some(last)) = (first_token(n), last_token(n)) {
                    self.lead(first, out);
                    out.push(Doc::Raw(
                        self.src[first.span.lo as usize..last.span.hi as usize].to_vec(),
                    ));
                    self.trail(last, out);
                }
            }

            // --------------------------------------------- catch-all --
            _ => self.walk_children(n, out, ctx),
        }
    }

    fn doc_of(&self, n: &GreenNode, ctx: Ctx) -> Doc {
        let mut d = Vec::new();
        self.expr(n, &mut d, ctx);
        Doc::Concat(d)
    }

    fn capture_list(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        let mut prev: Option<K> = None;
        for c in &n.children {
            if let Child::Token(t) = c {
                if prev == Some(K::Comma) && t.kind != K::RBracket {
                    out.push(Doc::text(" "));
                }
                self.tok(t, out);
                prev = Some(t.kind);
            }
        }
    }

    fn tight_brace_idents(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        let mut prev: Option<K> = None;
        for c in &n.children {
            match c {
                Child::Token(t) => {
                    if prev == Some(K::Comma) {
                        out.push(Doc::text(" "));
                    }
                    self.tok(t, out);
                    prev = Some(t.kind);
                }
                Child::Node(m) => {
                    if prev == Some(K::Comma) {
                        out.push(Doc::text(" "));
                    }
                    self.node(m, out, Ctx::Free);
                    prev = last_token(m).map(|t| t.kind);
                }
            }
        }
    }

    /// Was this brace pair written multiline in source?
    fn brace_multiline(&self, open: &GreenToken, close: Option<&GreenToken>) -> bool {
        close.is_some_and(|c| self.newlines_between(open.span.hi, c.span.lo) > 0)
    }

    // ---------------------------------------------------------- parens --

    fn paren(&self, n: &GreenNode, out: &mut Vec<Doc>, ctx: Ctx) {
        let inner = n.nodes().next();
        let lp = n.child_token(K::LParen);
        let rp = n.child_token(K::RParen);
        // A moded receiver `(mut p)` / `(take p)` (X1): the parens are
        // load-bearing syntax, never droppable; the receiver stays flat.
        let mode = n.tokens().find(|t| matches!(t.kind, K::MutKw | K::TakeKw));
        if let Some(mode) = mode {
            let mut g = Vec::new();
            if let Some(lp) = lp {
                self.lead(lp, &mut g);
                g.push(Doc::text("("));
                self.trail(lp, &mut g);
            }
            self.tok(mode, &mut g);
            g.push(Doc::text(" "));
            if let Some(e) = inner {
                self.expr(e, &mut g, Ctx::Free);
            }
            if let Some(rp) = rp {
                self.tok(rp, &mut g);
            }
            out.push(Doc::Concat(g));
            return;
        }
        // Droppability consults LEADING comments only, and the split is
        // load-bearing in both directions.
        //
        // A *trailing* comment renders as a line suffix and floats to
        // the end of its output line, so which token owns it changes
        // from pass to pass: `(//\n0)` hangs the comment off `(` and
        // prints `(0) //`, where it now trails `)`. Any guard reading a
        // trailing run therefore flips between passes.
        //
        // A *leading* comment does not float — but dropping the parens
        // re-homes it from the delimiter to the expression's own first
        // token, and an own-line comment forces its group broken, so
        // `//\n(r.0)` printed flat on pass one and broken on pass two.
        // Keeping the parens keeps the ownership, and that is stable.
        let droppable = match inner {
            Some(e) => {
                !paren_blacklisted(e.kind)
                    && lp.is_none_or(|t| !self.has_lead_comment(t))
                    && rp.is_none_or(|t| !self.has_lead_comment(t))
                    && ctx_allows_bare(ctx, e)
            }
            None => false,
        };
        if droppable {
            if let Some(lp) = lp {
                self.tok_trivia_only(lp, out);
            }
            self.expr(inner.unwrap(), out, ctx);
            if let Some(rp) = rp {
                self.tok_trivia_only(rp, out);
            }
            return;
        }
        let mut g = Vec::new();
        if let Some(lp) = lp {
            self.lead(lp, &mut g);
            g.push(Doc::text("("));
            self.trail(lp, &mut g);
        }
        if let Some(e) = inner {
            let mut d = Vec::new();
            self.expr(e, &mut d, Ctx::Free);
            g.push(Doc::Indent(vec![Doc::Softline, Doc::Concat(d)]));
        }
        g.push(Doc::Softline);
        if let Some(rp) = rp {
            self.tok(rp, &mut g);
        }
        out.push(Doc::Group(g));
    }

    // --------------------------------------------------------- binexpr --

    /// Flatten a same-tier chain: break after operators, continuations
    /// indented once (`[gram.fmt.continuation]`).
    fn bin_chain(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        let t = tier(n);
        if t == 99 {
            // Recovered operator the precedence table doesn't know
            // (e.g. `=` parsed as an expression during recovery):
            // generic spacing, no chain layout.
            self.walk_children(n, out, Ctx::Free);
            return;
        }
        let mut parts: Vec<Doc> = Vec::new();
        self.flatten_bin(n, t, &mut parts, true);
        let mut g = parts;
        let force = self.has_inner_trailing_comment(n);
        if force {
            g.push(Doc::BreakParent);
        }
        out.push(Doc::Group(g));
    }

    fn flatten_bin(&self, n: &GreenNode, t: u8, parts: &mut Vec<Doc>, leftmost: bool) {
        if n.kind == K::BinExpr && tier(n) == t && tier_associates_left(t) {
            let mut it = n.nodes();
            let lhs = it.next();
            let rhs = it.next();
            if let (Some(l), Some(r), Some(op)) =
                (lhs, rhs, n.tokens().find(|tk| bin_tier(tk.kind) != 99))
            {
                self.flatten_bin(l, t, parts, leftmost);
                let mut opd = Vec::new();
                opd.push(Doc::text(" "));
                self.tok(op, &mut opd);
                opd.push(Doc::Line);
                let mut rd = Vec::new();
                self.expr(
                    r,
                    &mut rd,
                    Ctx::BinOperand {
                        tier: t,
                        left: false,
                        assoc_left: tier_associates_left(t),
                    },
                );
                // The operand rides INSIDE the continuation indent: its
                // leading-comment hardlines must land at the same base as
                // the break after the operator, or comment columns re-derive
                // against a moved anchor and compound each pass.
                opd.append(&mut rd);
                parts.push(Doc::Indent(opd));
                return;
            }
        }
        // Leaf operand (or a non-chainable tier).
        let mut d = Vec::new();
        if n.kind == K::BinExpr && tier(n) == t && !tier_associates_left(t) {
            // Non-associative tier: still one operator application.
            let mut it = n.nodes();
            let lhs = it.next();
            let rhs = it.next();
            if let (Some(l), Some(r), Some(op)) =
                (lhs, rhs, n.tokens().find(|tk| bin_tier(tk.kind) != 99))
            {
                self.expr(
                    l,
                    &mut d,
                    Ctx::BinOperand {
                        tier: t,
                        left: true,
                        assoc_left: false,
                    },
                );
                let mut opd = Vec::new();
                opd.push(Doc::text(" "));
                self.tok(op, &mut opd);
                opd.push(Doc::Line);
                let mut rd = Vec::new();
                self.expr(
                    r,
                    &mut rd,
                    Ctx::BinOperand {
                        tier: t,
                        left: false,
                        assoc_left: false,
                    },
                );
                // Same base-indent invariant as the associative chain.
                opd.append(&mut rd);
                d.push(Doc::Indent(opd));
                parts.push(Doc::Concat(d));
                return;
            }
        }
        self.expr(
            n,
            &mut d,
            Ctx::BinOperand {
                tier: t,
                left: leftmost,
                assoc_left: tier_associates_left(t),
            },
        );
        parts.push(Doc::Concat(d));
    }

    // --------------------------------------------------------- postfix --

    /// One postfix chain: base, then `.member`/calls/indexes/`?`,
    /// breaking *after* dots (trailing style).
    fn postfix_chain(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        let mut base = Vec::new();
        let mut ops = Vec::new();
        self.flatten_postfix(n, &mut base, &mut ops);
        let mut g = Vec::new();
        g.push(Doc::Concat(base));
        g.extend(ops);
        out.push(Doc::Group(g));
    }

    fn flatten_postfix(&self, n: &GreenNode, base: &mut Vec<Doc>, ops: &mut Vec<Doc>) {
        match n.kind {
            K::CallExpr | K::BracketApply => {
                let mut it = n.nodes();
                if let Some(callee) = it.next() {
                    self.flatten_postfix(callee, base, ops);
                }
                if let Some(args) = n.child_node(K::ArgList) {
                    let mut d = Vec::new();
                    self.node(args, &mut d, Ctx::Free);
                    push_op(base, ops, d);
                }
            }
            K::TryExpr => {
                if let Some(inner) = n.nodes().next() {
                    self.flatten_postfix(inner, base, ops);
                }
                if let Some(q) = n.child_token(K::Question) {
                    let mut d = Vec::new();
                    self.tok(q, &mut d);
                    push_op(base, ops, d);
                }
            }
            K::MemberExpr => {
                let mut it = n.nodes();
                if let Some(b) = it.next() {
                    self.flatten_postfix(b, base, ops);
                }
                let mut d = Vec::new();
                if let Some(dot) = n.child_token(K::Dot) {
                    self.lead(dot, &mut d);
                    d.push(Doc::text("."));
                    self.trail(dot, &mut d);
                }
                d.push(Doc::Softline);
                // Member: Ident, Int, or a keyword-transparent name.
                let mut seen_dot = false;
                for c in &n.children {
                    match c {
                        Child::Token(t) if t.kind == K::Dot => seen_dot = true,
                        Child::Token(t) if seen_dot => self.tok(t, &mut d),
                        _ => {}
                    }
                }
                ops.push(Doc::Indent(d));
            }
            _ => self.expr(n, base, Ctx::Postfix),
        }
    }

    // ------------------------------------------------------------- ifs --

    fn if_expr(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        self.kw(n.child_token(K::IfKw), "if", out);
        out.push(Doc::text(" "));
        let mut nodes: Vec<&GreenNode> = n.nodes().collect();
        if nodes.is_empty() {
            return;
        }
        let cond = nodes.remove(0);
        self.expr(cond, out, Ctx::Cond);
        out.push(Doc::text(" "));
        if !nodes.is_empty() {
            let then = nodes.remove(0);
            self.node(then, out, Ctx::Free);
        }
        if let Some(else_kw) = n.child_token(K::ElseKw) {
            out.push(Doc::text(" "));
            self.kw(Some(else_kw), "else", out);
            out.push(Doc::text(" "));
            if let Some(branch) = nodes.first() {
                // `else { if … }` → `else if …` when the block is
                // exactly one comment-free `if`.
                if let Some(inner_if) = sole_if_of_block(branch)
                    && !self.block_frame_has_comment(branch)
                {
                    self.if_expr(inner_if, out);
                    return;
                }
                self.node(branch, out, Ctx::Free);
            }
        }
    }

    fn block_frame_has_comment(&self, b: &GreenNode) -> bool {
        b.tokens().any(|t| self.subtree_token_has_comment(t))
    }

    // ----------------------------------------------------- match/select --

    fn match_like(&self, n: &GreenNode, out: &mut Vec<Doc>, kw: K) {
        if let Some(t) = n.child_token(kw) {
            self.lead(t, out);
            out.push(Doc::Text(self.slice(t.span).to_vec()));
            self.trail(t, out);
        }
        // Scrutinee (match only).
        let arms: Vec<&GreenNode> = n
            .nodes()
            .filter(|c| matches!(c.kind, K::MatchArm | K::SelectArm))
            .collect();
        if kw == K::MatchKw
            && let Some(scrutinee) = n
                .nodes()
                .find(|c| !matches!(c.kind, K::MatchArm | K::SelectArm))
        {
            out.push(Doc::text(" "));
            self.expr(scrutinee, out, Ctx::Cond);
        }
        out.push(Doc::text(" "));
        let Some(lb) = n.child_token(K::LBrace) else {
            return;
        };
        self.lead(lb, out);
        out.push(Doc::text("{"));
        self.trail(lb, out);
        let rb = n.tokens().filter(|t| t.kind == K::RBrace).last();
        if arms.is_empty() {
            out.push(Doc::text("}"));
            if let Some(r) = rb {
                self.trail(r, out);
            }
            return;
        }
        let mut inner = Vec::new();
        let mut prev_end = lb.trailing.last().map(|s| s.hi).unwrap_or(lb.span.hi);
        for arm in &arms {
            let lo = comment_or_token_start(self.src, arm);
            inner.push(if self.blank_between(prev_end, lo) {
                Doc::Blankline
            } else {
                Doc::Hardline
            });
            if shallow_error(arm) {
                inner.push(self.verbatim(lead_start(arm), trail_end(arm)));
            } else {
                self.arm(arm, &mut inner);
            }
            prev_end = code_end(arm);
        }
        out.push(Doc::Indent(inner));
        out.push(Doc::Hardline);
        out.push(Doc::text("}"));
        if let Some(r) = rb {
            self.trail(r, out);
        }
        out.push(Doc::BreakParent);
    }

    fn arm(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        // pattern (guard)? ('from' expr)? => body — then a canonical
        // trailing comma (`[gram.fmt.commas]`).
        let mut g = Vec::new();
        let mut past_arrow = false;
        let mut prev: Option<K> = None;
        let mut body: Vec<&GreenNode> = Vec::new();
        for c in &n.children {
            match c {
                Child::Token(t) => match t.kind {
                    K::FatArrow => {
                        g.push(Doc::text(" "));
                        self.kw(Some(t), "=>", &mut g);
                        past_arrow = true;
                    }
                    K::Comma | K::Term => self.tok_trivia_only(t, &mut g),
                    _ => {
                        if prev.is_some() && pair_space(prev.unwrap(), t.kind) {
                            g.push(Doc::text(" "));
                        }
                        self.tok(t, &mut g);
                        prev = Some(t.kind);
                    }
                },
                Child::Node(m) => {
                    if past_arrow {
                        body.push(m);
                    } else {
                        if let (Some(p), Some(f)) = (prev, first_token(m).map(|t| t.kind))
                            && pair_space(p, f)
                        {
                            g.push(Doc::text(" "));
                        }
                        self.node(m, &mut g, Ctx::Cond);
                        prev = last_token(m).map(|t| t.kind);
                    }
                }
            }
        }
        for b in body {
            if blocky(b.kind) {
                g.push(Doc::text(" "));
                let mut d = Vec::new();
                self.node(b, &mut d, Ctx::Free);
                g.push(Doc::Concat(d));
            } else {
                let mut d = Vec::new();
                self.expr(b, &mut d, Ctx::Free);
                g.push(Doc::Group(vec![Doc::Indent(vec![
                    Doc::Line,
                    Doc::Concat(d),
                ])]));
            }
        }
        g.push(Doc::text(","));
        out.push(Doc::Concat(g));
    }

    // ----------------------------------------------------------- decls --

    fn decl(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        match n.kind {
            K::StructDecl | K::EnumDecl => {
                // `struct Name { fields }` — header, then a brace body.
                let mut prev: Option<K> = None;
                let mut body_started = false;
                let (open, _, commas, close) = self.split_list(n, K::LBrace, K::RBrace);
                for c in &n.children {
                    match c {
                        Child::Token(t)
                            if open.is_some_and(|o| std::ptr::eq(o, t)) && !body_started =>
                        {
                            body_started = true;
                            out.push(Doc::text(" "));
                            let fields: Vec<&GreenNode> = n
                                .nodes()
                                .filter(|m| matches!(m.kind, K::StructField | K::EnumVariant))
                                .collect();
                            let force = self.brace_multiline(open.unwrap(), close);
                            self.field_list(open.unwrap(), close, &fields, &commas, force, out);
                        }
                        Child::Token(t) => {
                            if body_started {
                                continue;
                            }
                            if let Some(p) = prev
                                && pair_space(p, t.kind)
                            {
                                out.push(Doc::text(" "));
                            }
                            self.tok(t, out);
                            prev = Some(t.kind);
                        }
                        Child::Node(m) => {
                            if body_started {
                                continue;
                            }
                            match m.kind {
                                K::Attribute => {
                                    self.node(m, out, Ctx::Free);
                                    out.push(Doc::Hardline);
                                    prev = None;
                                }
                                K::GenericParamList => {
                                    self.node(m, out, Ctx::Free);
                                    prev = last_token(m).map(|t| t.kind);
                                }
                                K::StructField | K::EnumVariant => {}
                                _ => {
                                    if let (Some(p), Some(f)) =
                                        (prev, first_token(m).map(|t| t.kind))
                                        && pair_space(p, f)
                                    {
                                        out.push(Doc::text(" "));
                                    }
                                    self.node(m, out, Ctx::Free);
                                    prev = last_token(m).map(|t| t.kind);
                                }
                            }
                        }
                    }
                }
            }
            K::TraitDecl | K::ImplDecl => self.container_decl(n, out),
            _ => {
                // Generic declaration walk: attributes on their own
                // lines, everything else by pair spacing; `TypeDecl`
                // bodies (StructDef/EnumDef) handled where met.
                let mut prev: Option<K> = None;
                for c in &n.children {
                    match c {
                        Child::Token(t) => {
                            if matches!(t.kind, K::Term | K::Missing) {
                                self.tok_trivia_only(t, out);
                                continue;
                            }
                            if let Some(p) = prev
                                && pair_space(p, t.kind)
                            {
                                out.push(Doc::text(" "));
                            }
                            self.tok(t, out);
                            prev = Some(t.kind);
                        }
                        Child::Node(m) => match m.kind {
                            K::Attribute => {
                                self.node(m, out, Ctx::Free);
                                out.push(Doc::Hardline);
                                prev = None;
                            }
                            K::StructDef | K::EnumDef => {
                                if prev.is_some() {
                                    out.push(Doc::text(" "));
                                }
                                self.struct_def(m, out);
                                prev = Some(K::RBrace);
                            }
                            _ => {
                                if let (Some(p), Some(f)) = (prev, first_token(m).map(|t| t.kind))
                                    && self.child_space(p, f, m.kind)
                                {
                                    out.push(Doc::text(" "));
                                }
                                self.node(m, out, Ctx::Free);
                                prev = last_token(m).map(|t| t.kind);
                            }
                        },
                    }
                }
            }
        }
    }

    fn struct_def(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        // `struct { fields }` / `enum { variants }` in type position.
        let kw = n
            .tokens()
            .find(|t| matches!(t.kind, K::StructKw | K::EnumKw));
        if let Some(kw) = kw {
            self.tok(kw, out);
        }
        out.push(Doc::text(" "));
        let (open, _, commas, close) = self.split_list(n, K::LBrace, K::RBrace);
        let fields: Vec<&GreenNode> = n
            .nodes()
            .filter(|m| matches!(m.kind, K::StructField | K::EnumVariant))
            .collect();
        if let Some(open) = open {
            let force = self.brace_multiline(open, close);
            self.field_list(open, close, &fields, &commas, force, out);
        }
    }

    /// Struct fields / enum variants: padded braces, comma-separated,
    /// trailing commas when broken.
    fn field_list(
        &self,
        open: &GreenToken,
        close: Option<&GreenToken>,
        fields: &[&GreenNode],
        commas: &[&GreenToken],
        force: bool,
        out: &mut Vec<Doc>,
    ) {
        let mut g = Vec::new();
        self.lead(open, &mut g);
        g.push(Doc::text("{"));
        self.trail(open, &mut g);
        if fields.is_empty() {
            g.push(Doc::text("}"));
            if let Some(c) = close {
                self.trail(c, &mut g);
            }
            out.push(Doc::Group(g));
            return;
        }
        let mut inner = Vec::new();
        inner.push(Doc::Line);
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                inner.push(Doc::text(","));
                if let Some(c) = commas.get(i - 1) {
                    self.tok_trivia_only(c, &mut inner);
                }
                inner.push(Doc::Line);
            }
            // A struct field's own trailing comma is stripped and
            // re-added canonically.
            self.field(f, &mut inner);
        }
        inner.push(Doc::IfBreak {
            broken: b",".to_vec(),
            flat: Vec::new(),
        });
        if commas.len() >= fields.len()
            && let Some(c) = commas.last()
        {
            self.tok_trivia_only(c, &mut inner);
        }
        g.push(Doc::Indent(inner));
        g.push(Doc::Line);
        if let Some(c) = close {
            self.lead(c, &mut g);
        }
        g.push(Doc::text("}"));
        if let Some(c) = close {
            self.trail(c, &mut g);
        }
        if force
            || fields.iter().any(|f| self.subtree_has_comment(f))
            || commas.iter().any(|c| self.subtree_token_has_comment(c))
        {
            g.push(Doc::BreakParent);
        }
        out.push(Doc::Group(g));
    }

    /// One struct field / enum variant, with any trailing `,` removed
    /// (the list re-adds separators).
    fn field(&self, f: &GreenNode, out: &mut Vec<Doc>) {
        let mut prev: Option<K> = None;
        let mut depth = 0usize;
        for c in &f.children {
            match c {
                Child::Token(t) => {
                    match t.kind {
                        K::LParen | K::LBracket => depth += 1,
                        K::RParen | K::RBracket => depth = depth.saturating_sub(1),
                        _ => {}
                    }
                    // only the list separator is stripped; a payload's
                    // interior commas (`Rgb(int, int, int)`) are content
                    if t.kind == K::Comma && depth == 0 {
                        self.tok_trivia_only(t, out);
                        continue;
                    }
                    if let Some(p) = prev
                        && pair_space(p, t.kind)
                    {
                        out.push(Doc::text(" "));
                    }
                    self.tok(t, out);
                    prev = Some(t.kind);
                }
                Child::Node(m) => {
                    if m.kind == K::Attribute {
                        self.node(m, out, Ctx::Free);
                        out.push(Doc::Hardline);
                        prev = None;
                        continue;
                    }
                    if let (Some(p), Some(fk)) = (prev, first_token(m).map(|t| t.kind))
                        && self.child_space(p, fk, m.kind)
                    {
                        out.push(Doc::text(" "));
                    }
                    self.node(m, out, Ctx::Free);
                    prev = last_token(m).map(|t| t.kind);
                }
            }
        }
    }

    /// `trait`/`impl` bodies: header + always-multiline member list,
    /// blank gaps preserved (capped at one).
    fn container_decl(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        let (open, _, _, close) = self.split_list(n, K::LBrace, K::RBrace);
        let mut prev: Option<K> = None;
        let mut members: Vec<&GreenNode> = Vec::new();
        let mut in_body = false;
        for c in &n.children {
            match c {
                Child::Token(t) if Some(t.span) == open.map(|o| o.span) => {
                    in_body = true;
                }
                Child::Token(_) => {}
                Child::Node(m) => {
                    if in_body {
                        members.push(m);
                    }
                }
            }
        }
        // Header: everything before `{`.
        for c in &n.children {
            match c {
                Child::Token(t) => {
                    if Some(t.span) == open.map(|o| o.span) {
                        break;
                    }
                    if let Some(p) = prev
                        && pair_space(p, t.kind)
                    {
                        out.push(Doc::text(" "));
                    }
                    self.tok(t, out);
                    prev = Some(t.kind);
                }
                Child::Node(m) => {
                    if in_body && members.iter().any(|x| std::ptr::eq(*x, m)) {
                        break;
                    }
                    if m.kind == K::Attribute {
                        self.node(m, out, Ctx::Free);
                        out.push(Doc::Hardline);
                        prev = None;
                        continue;
                    }
                    if let (Some(p), Some(f)) = (prev, first_token(m).map(|t| t.kind))
                        && self.child_space(p, f, m.kind)
                    {
                        out.push(Doc::text(" "));
                    }
                    self.node(m, out, Ctx::Free);
                    prev = last_token(m).map(|t| t.kind);
                }
            }
        }
        out.push(Doc::text(" "));
        let Some(open) = open else {
            return;
        };
        self.lead(open, out);
        out.push(Doc::text("{"));
        self.trail(open, out);
        if members.is_empty() && !self.has_lead_comment_opt(close) {
            out.push(Doc::text("}"));
            if let Some(c) = close {
                self.trail(c, out);
            }
            return;
        }
        let broken = margin_set(&members);
        let mut inner = Vec::new();
        let mut prev_end = open.trailing.last().map(|s| s.hi).unwrap_or(open.span.hi);
        let mut i = 0usize;
        while i < members.len() {
            let lo = comment_or_token_start(self.src, members[i]);
            inner.push(if self.blank_between(prev_end, lo) {
                Doc::Blankline
            } else {
                Doc::Hardline
            });
            if broken.contains(&i) {
                let start = i;
                let mut end = i;
                while end + 1 < members.len() && broken.contains(&(end + 1)) {
                    end += 1;
                }
                inner.push(self.verbatim(lead_start(members[start]), trail_end(members[end])));
                prev_end = code_end(members[end]);
                i = end + 1;
                continue;
            }
            let mut d = Vec::new();
            self.stmt(members[i], &mut d);
            inner.push(Doc::Concat(d));
            prev_end = code_end(members[i]);
            i += 1;
        }
        if let Some(c) = close {
            let mut dangle = Vec::new();
            self.lead(c, &mut dangle);
            if !dangle.is_empty() {
                inner.push(Doc::Hardline);
                if matches!(dangle.last(), Some(Doc::Hardline | Doc::Blankline)) {
                    dangle.pop();
                }
                inner.append(&mut dangle);
            }
        }
        out.push(Doc::Indent(inner));
        out.push(Doc::Hardline);
        out.push(Doc::text("}"));
        if let Some(c) = close {
            self.trail(c, out);
        }
        out.push(Doc::BreakParent);
    }

    // ------------------------------------------------------------- asm --

    fn asm(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        self.kw(n.child_token(K::AsmKw), "asm", out);
        out.push(Doc::text(" "));
        let (open, elems, commas, close) = self.split_list(n, K::LBrace, K::RBrace);
        if let Some(open) = open {
            let force = self.brace_multiline(open, close);
            self.list(
                open,
                close,
                &elems,
                &commas,
                true,
                force,
                false,
                out,
                Ctx::Free,
            );
        }
    }

    /// `loop { }`, `unsafe { }`, `scope name? { }`.
    fn kw_block(&self, n: &GreenNode, out: &mut Vec<Doc>) {
        let mut prev: Option<K> = None;
        for c in &n.children {
            match c {
                Child::Token(t) => {
                    if prev.is_some() {
                        out.push(Doc::text(" "));
                    }
                    self.tok(t, out);
                    prev = Some(t.kind);
                }
                Child::Node(m) => {
                    if prev.is_some() {
                        out.push(Doc::text(" "));
                    }
                    self.node(m, out, Ctx::Free);
                    prev = last_token(m).map(|t| t.kind);
                }
            }
        }
    }
}

// --------------------------------------------------------- free helpers ---

fn as_node(c: &Child) -> Option<&GreenNode> {
    match c {
        Child::Node(n) => Some(n),
        Child::Token(_) => None,
    }
}

fn first_token_of_children(n: &GreenNode) -> Option<&GreenToken> {
    first_token(n).or_else(|| {
        n.children.iter().find_map(|c| match c {
            Child::Token(t) => Some(t),
            Child::Node(_) => None,
        })
    })
}

fn collect_tokens<'n>(n: &'n GreenNode, out: &mut Vec<&'n GreenToken>) {
    for c in &n.children {
        match c {
            Child::Token(t) => out.push(t),
            Child::Node(m) => collect_tokens(m, out),
        }
    }
}

fn is_word_op(k: K) -> bool {
    matches!(k, K::MoveKw | K::CopyKw | K::SharedKw | K::MutKw)
}

fn blocky(k: K) -> bool {
    matches!(
        k,
        K::Block
            | K::IfExpr
            | K::MatchExpr
            | K::ForExpr
            | K::WhileExpr
            | K::LoopExpr
            | K::UnsafeBlock
            | K::ScopeExpr
            | K::SelectExpr
            | K::WhenExpr
            | K::RegionBlock
            | K::InBlock
            | K::InlineC
            | K::AsmExpr
    )
}

fn huggable(n: &GreenNode) -> bool {
    let e = match n.kind {
        K::Arg => match n.nodes().next() {
            Some(inner) => inner,
            None => return false,
        },
        _ => n,
    };
    match e.kind {
        K::StructLit | K::Block | K::RegionBlock | K::InBlock | K::MatchExpr | K::ScopeExpr => true,
        K::ClosureExpr => e.child_node(K::Block).is_some(),
        _ => false,
    }
}

fn push_op(base: &mut Vec<Doc>, ops: &mut Vec<Doc>, d: Vec<Doc>) {
    if ops.is_empty() {
        base.extend(d);
    } else {
        ops.push(Doc::Concat(d));
    }
}

/// `else { if … }` → the inner `if`, when the block is exactly that.
fn sole_if_of_block(b: &GreenNode) -> Option<&GreenNode> {
    if b.kind != K::Block {
        return None;
    }
    let stmts: Vec<&GreenNode> = b
        .children
        .iter()
        .filter_map(as_node)
        .filter(|n| is_stmt_child(n.kind))
        .collect();
    if stmts.len() != 1 || stmts[0].kind != K::ExprStmt {
        return None;
    }
    let inner: Vec<&GreenNode> = stmts[0].nodes().collect();
    if inner.len() == 1 && inner[0].kind == K::IfExpr {
        Some(inner[0])
    } else {
        None
    }
}

/// Indices of statements needing verbatim pass-through: damaged ones
/// plus a one-node margin on each side (Target 3).
fn margin_set(stmts: &[&GreenNode]) -> HashSet<usize> {
    let mut broken = HashSet::new();
    for (i, s) in stmts.iter().enumerate() {
        if shallow_error(s) {
            broken.insert(i);
            if i > 0 {
                broken.insert(i - 1);
            }
            if i + 1 < stmts.len() {
                broken.insert(i + 1);
            }
        }
    }
    broken
}

/// The context an expression child of `parent` sits in when reached by
/// the generic walk.
fn child_ctx(parent: K, _child: &GreenNode, outer: Ctx) -> Ctx {
    match parent {
        // `Arg` is transparent: an index argument's guard (the `[-1]`
        // E0209 trap) must reach the expression inside it.
        K::Arg => outer,
        _ => Ctx::Free,
    }
}

/// Can the parens be dropped around `inner` in context `ctx`?
fn ctx_allows_bare(ctx: Ctx, inner: &GreenNode) -> bool {
    let t = tier(inner);
    if t == 99 {
        return false;
    }
    match ctx {
        Ctx::Free => t <= 15,
        Ctx::Cond => t <= 13 && !exposed_brace(inner),
        Ctx::BinOperand {
            tier: pt,
            left,
            assoc_left,
        } => t < pt || (t == pt && assoc_left && left),
        Ctx::Prefix => t <= 3,
        Ctx::Cast => t <= 4,
        Ctx::Postfix => t <= 2,
        Ctx::RangeEnd => t <= 13,
        Ctx::IndexArg => {
            // Everything but the `[-1]` → E0209 trap.
            !(inner.kind == K::PrefixExpr
                && inner.child_token(K::Minus).is_some()
                && inner
                    .nodes()
                    .next()
                    .is_some_and(|e| e.kind == K::LiteralExpr))
        }
    }
}
