//! The doc-comment model — ONE model, two consumers (s53).
//!
//! Hover (s52) and `wolf doc` (s53) read doc comments through this
//! module and nothing else, so a doc comment can never mean one thing
//! in an editor and another on a documentation page. The contract:
//!
//! - `///` documents the item that follows; `//!` documents the
//!   enclosing module (and, for a script, carries its frontmatter —
//!   `wolf_pkg::script` owns that half and reads the same leading
//!   block).
//! - The body is markdown. The **first sentence** is the summary line
//!   used in listings and in hover's first paragraph.
//! - Fenced code blocks are **doctests** by default. The directive
//!   language is three words wide, deliberately: `no_run` (compile,
//!   don't execute), `should_fail(E0402, …)` (must be refused, with the
//!   codes named), `ignore` (not a program — prose in a fence).
//! - `[List.push]`-shaped bracket spans are intra-doc links. They are
//!   resolved against name resolution by [`resolve_links`], so a
//!   renamed item breaks the link loudly instead of rotting silently.
//!
//! `////` is NOT a doc comment (the lexer's rule, `[gram.lex.comment]`
//! — a ruler line is a ruler line); this module keeps that rule rather
//! than re-deriving a looser one from bytes.

use wolf_ast::{Child, GreenNode, GreenToken};
use wolf_diag::{Diagnostic, codes};
use wolf_sema::{ItemKind, Vis};
use wolf_span::{FileId, Span};

// ------------------------------------------------------------- fences ----

/// A fence directive — the whole language (three words, s01-style).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Directive {
    /// Compile it, never run it.
    NoRun,
    /// The build must be refused, with these diagnostic codes.
    ShouldFail(Vec<String>),
    /// Not a program: rendered, never compiled.
    Ignore,
}

/// One fenced code block of a doc comment.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DocFence {
    /// The info string's first word (`wolf` when absent — a bare fence
    /// in a wolf doc comment is wolf code).
    pub lang: String,
    pub directives: Vec<Directive>,
    /// The fence body, `///`-stripped, newline-terminated per line.
    pub code: String,
    /// The line index (0-based, within the doc comment) the fence opened
    /// at — enough to point a reader at it without pretending to a
    /// byte span the doc text no longer has.
    pub line: usize,
}

/// Fence languages that name a wolf **program** — the doctest runner's
/// set. `lu` is the file extension, accepted because a reader who typed
/// the extension meant wolf.
const RUNNABLE_LANGS: &[&str] = &["wolf", "lu"];

/// Fence languages that name a wolf **example** but not necessarily a
/// program the doctest runner can execute verbatim.
///
/// `wolf-doc-example` is wolf-std's own dialect, and it predates this
/// generator: each line is either an ASSERTION (a relational expression,
/// which that repository's rig turns into a checked branch) or a
/// statement. Those 331 blocks are real, reviewed, executed examples, so
/// documentation counts them and the JSON index carries them verbatim —
/// but `wolf test` does not run them, because a bare `a == b` is not a
/// wolf statement and pretending otherwise would report hundreds of
/// false failures. Closing that gap is either a migration in wolf-std or
/// an assertion-line dialect in the runner; the directive language here
/// stays three words wide (`no_run`, `should_fail`, `ignore`).
const EXAMPLE_LANGS: &[&str] = &["wolf", "lu", "wolf-doc-example"];

impl DocFence {
    /// Is this fence a doctest — a wolf program the test runner owns and
    /// executes? `ignore`d fences and foreign-language fences are prose.
    pub fn is_doctest(&self) -> bool {
        RUNNABLE_LANGS.contains(&self.lang.as_str())
            && !self.directives.contains(&Directive::Ignore)
    }

    /// Is this fence a wolf example at all? Coverage counts these: an
    /// item with a reviewed, executed example carries an example, whether
    /// or not this toolchain is the thing that executes it.
    pub fn is_example(&self) -> bool {
        EXAMPLE_LANGS.contains(&self.lang.as_str()) && !self.directives.contains(&Directive::Ignore)
    }

    /// The codes a `should_fail` fence must produce (empty = any error).
    pub fn should_fail(&self) -> Option<&[String]> {
        self.directives.iter().find_map(|d| match d {
            Directive::ShouldFail(codes) => Some(codes.as_slice()),
            _ => None,
        })
    }

    pub fn no_run(&self) -> bool {
        self.directives.contains(&Directive::NoRun)
    }
}

// --------------------------------------------------------- doc comment ---

/// A parsed doc comment: the markdown body, its summary sentence, its
/// fences, and its intra-doc links.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct DocComment {
    /// The full markdown body, comment markers stripped.
    pub text: String,
    /// The first sentence of the first paragraph (listings, hover).
    pub summary: String,
    pub fences: Vec<DocFence>,
    /// `[target]` spans, in order, deduplicated: the link text as
    /// written.
    pub links: Vec<String>,
}

impl DocComment {
    /// Does this comment carry at least one runnable doctest?
    pub fn has_doctest(&self) -> bool {
        self.fences.iter().any(DocFence::is_doctest)
    }

    /// Does it carry at least one wolf example (the coverage question)?
    pub fn has_example(&self) -> bool {
        self.fences.iter().any(DocFence::is_example)
    }
}

/// The `//!` directive keys the toolchain reads out of a module header:
/// corpus check expectations and the module-participation flag. They are
/// machinery, not prose, so documentation never prints them — a reader
/// of a generated page has no use for `phase: run`.
const DIRECTIVE_KEYS: &[&str] = &["check:", "phase:", "conforms:", "warns:", "member:"];

fn is_directive(payload: &str) -> bool {
    let t = payload.trim();
    DIRECTIVE_KEYS.iter().any(|k| t.starts_with(k))
}

/// Strip one comment marker from a line, returning its payload.
/// `None` when the line is not a doc line of `marker` shape.
fn strip_marker(line: &str, marker: &str) -> Option<String> {
    let t = line.trim_start();
    let rest = t.strip_prefix(marker)?;
    // `////` is a ruler, not a doc comment (the lexer's rule).
    if marker == "///" && rest.starts_with('/') {
        return None;
    }
    Some(rest.strip_prefix(' ').unwrap_or(rest).to_string())
}

/// The `///` doc comment attached to a token's leading trivia. A
/// non-doc comment breaks the run: the doc comment of an item is the
/// block *immediately* above it, so the LAST run before the token wins.
pub fn outer_doc(token: &GreenToken, src: &[u8]) -> Option<DocComment> {
    collect(token.leading.iter().copied(), src, "///", false)
}

/// The `//!` module doc comment of a file: the FIRST `//!` block, which
/// is a header — it ends where it ends, and the `///` comment of the
/// file's first item does not erase it. Prose only: directive lines are
/// dropped, and a `pkg { … }` frontmatter literal stays in the text,
/// because it IS documentation of the script (the manifest reader is
/// `wolf_pkg::script`).
pub fn module_doc(root: &GreenNode, src: &[u8]) -> Option<DocComment> {
    let first = first_token(root)?;
    collect(first.leading.iter().copied(), src, "//!", true)
}

/// `header`: take the first run and stop (a module header); otherwise
/// take the last run before the token (an item's doc comment).
fn collect(
    spans: impl Iterator<Item = Span>,
    src: &[u8],
    marker: &str,
    header: bool,
) -> Option<DocComment> {
    let mut lines: Vec<String> = Vec::new();
    let mut started = false;
    'spans: for span in spans {
        let text = String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]);
        for line in text.lines() {
            match strip_marker(line, marker) {
                Some(payload) => {
                    started = true;
                    if header && is_directive(&payload) {
                        continue;
                    }
                    lines.push(payload);
                }
                None if line.trim().is_empty() => {}
                None if header && started => break 'spans,
                None if header => {}
                // A plain comment (or a ruler) breaks the run: what
                // documents the item is the block touching it.
                None => lines.clear(),
            }
        }
    }
    // Directive-only headers leave leading blanks behind.
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return None;
    }
    Some(parse_body(&lines))
}

/// Build a [`DocComment`] from already-stripped payload lines. Public
/// so `wolf_pkg`'s script reader and the test runner can parse a doc
/// body they extracted themselves without duplicating fence rules.
pub fn parse_body(lines: &[String]) -> DocComment {
    let text = lines.join("\n");
    DocComment {
        summary: summary_of(lines),
        fences: fences_of(lines),
        links: links_of(&text),
        text,
    }
}

/// The first sentence of the first paragraph: everything up to the
/// first `. ` / `.\n` / final `.`, with markdown emphasis left alone
/// (the renderer handles it) and the period kept.
fn summary_of(lines: &[String]) -> String {
    let mut para = String::new();
    for l in lines {
        let t = l.trim();
        if t.is_empty() {
            if para.is_empty() {
                continue;
            }
            break;
        }
        // A fence or a heading is not a summary sentence.
        if t.starts_with("```") || t.starts_with('#') {
            if para.is_empty() {
                continue;
            }
            break;
        }
        if !para.is_empty() {
            para.push(' ');
        }
        para.push_str(t);
    }
    // Sentence end: a period followed by whitespace or end of text, not
    // inside `code` and not part of an ellipsis.
    let bytes = para.as_bytes();
    let mut in_code = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'`' => in_code = !in_code,
            b'.' if !in_code => {
                let next = bytes.get(i + 1).copied();
                let prev = if i == 0 {
                    None
                } else {
                    bytes.get(i - 1).copied()
                };
                if next != Some(b'.') && prev != Some(b'.') {
                    match next {
                        None | Some(b' ') => return para[..=i].trim_end().to_string(),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    para
}

/// Parse ``` fences and their directives.
fn fences_of(lines: &[String]) -> Vec<DocFence> {
    let mut out = Vec::new();
    let mut open: Option<(usize, String, Vec<Directive>, Vec<String>)> = None;
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim_start();
        let Some(info) = t.strip_prefix("```") else {
            if let Some((_, _, _, body)) = open.as_mut() {
                body.push(l.clone());
            }
            continue;
        };
        match open.take() {
            Some((line, lang, directives, body)) => out.push(DocFence {
                lang,
                directives,
                code: if body.is_empty() {
                    String::new()
                } else {
                    format!("{}\n", body.join("\n"))
                },
                line,
            }),
            None => {
                let (lang, directives) = parse_info(info.trim());
                open = Some((i, lang, directives, Vec::new()));
            }
        }
    }
    // An unclosed fence still yields its body: the reader sees the code,
    // and the doctest runner gets a program to reject if it is broken.
    if let Some((line, lang, directives, body)) = open {
        out.push(DocFence {
            lang,
            directives,
            code: if body.is_empty() {
                String::new()
            } else {
                format!("{}\n", body.join("\n"))
            },
            line,
        });
    }
    out
}

/// `wolf,no_run` / `should_fail(E0402)` / `ignore` / `` / `text`.
fn parse_info(info: &str) -> (String, Vec<Directive>) {
    let mut lang: Option<String> = None;
    let mut directives = Vec::new();
    for word in split_info(info) {
        let w = word.trim();
        if w.is_empty() {
            continue;
        }
        if w == "no_run" {
            directives.push(Directive::NoRun);
        } else if w == "ignore" {
            directives.push(Directive::Ignore);
        } else if let Some(rest) = w.strip_prefix("should_fail") {
            let inner = rest
                .trim()
                .strip_prefix('(')
                .and_then(|s| s.strip_suffix(')'))
                .unwrap_or("");
            let codes: Vec<String> = inner
                .split(',')
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .collect();
            directives.push(Directive::ShouldFail(codes));
        } else if lang.is_none() {
            lang = Some(w.to_string());
        }
    }
    (lang.unwrap_or_else(|| "wolf".to_string()), directives)
}

/// Split an info string on commas and whitespace, but never inside the
/// parentheses of `should_fail(E0402, E0403)`.
fn split_info(info: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0u32;
    for c in info.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                cur.push(c);
            }
            ',' | ' ' | '\t' if depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// `[target]` spans that are intra-doc links: not `[text](url)`
/// markdown links, not `[ ]` checkboxes, and never inside a fence or
/// inline code.
fn links_of(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let bytes = line.as_bytes();
        let mut in_code = false;
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'`' => in_code = !in_code,
                b'[' if !in_code => {
                    let Some(close) = line[i + 1..].find(']') else {
                        break;
                    };
                    let target = &line[i + 1..i + 1 + close];
                    let after = line[i + 1 + close + 1..].chars().next();
                    let is_md_link = after == Some('(') || after == Some('[');
                    if !is_md_link && is_link_target(target) && !out.iter().any(|t| t == target) {
                        out.push(target.to_string());
                    }
                    i += close + 1;
                }
                _ => {}
            }
            i += 1;
        }
    }
    out
}

/// A link target is a dotted path of wolf identifiers, optionally with
/// a trailing `()`: `push`, `List.push`, `std.str.trim()`. Anything
/// else in brackets is ordinary markdown.
fn is_link_target(t: &str) -> bool {
    let t = t.strip_suffix("()").unwrap_or(t);
    if t.is_empty() {
        return false;
    }
    t.split('.').all(|seg| {
        !seg.is_empty()
            && seg
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// Depth-first first token of a node (where its doc trivia lives).
pub(crate) fn first_token(node: &GreenNode) -> Option<&GreenToken> {
    for child in &node.children {
        match child {
            Child::Token(t) => return Some(t),
            Child::Node(n) => {
                if let Some(t) = first_token(n) {
                    return Some(t);
                }
            }
        }
    }
    None
}

// ------------------------------------------------------- the doc model ---

/// One documented item.
#[derive(Clone, Debug)]
pub struct DocItem {
    pub name: String,
    pub kind: ItemKind,
    pub vis: Vis,
    /// The elaborated signature, from the compiler's own pretty-printer
    /// (`wolf_sema::item_signature`) — never re-parsed from source.
    pub sig: String,
    pub doc: Option<DocComment>,
    /// Display path of the file the item is declared in (never an
    /// on-disk absolute path — D7).
    pub file: String,
    pub name_span: Span,
    /// The item's `FileId`, for link diagnostics.
    pub file_id: FileId,
}

/// One documented module.
#[derive(Clone, Debug)]
pub struct DocModule {
    /// Dotted path from the package root (`""` = the root module).
    pub path: String,
    pub doc: Option<DocComment>,
    /// Display path of the file the `//!` block was read from.
    pub file: String,
    /// Where a module-level diagnostic lands: the head of that file.
    pub doc_span: Option<Span>,
    /// Canonical order: `(kind, name)` — the interface's order, so a
    /// page and an interface list the same surface in the same order.
    pub items: Vec<DocItem>,
}

/// A package's documentation surface.
#[derive(Clone, Debug)]
pub struct DocPackage {
    pub package: String,
    pub modules: Vec<DocModule>,
    /// Whether private items were included (`wolf doc --private`).
    pub private: bool,
}

impl DocPackage {
    /// Every item, module-major then canonical — the JSON index's order.
    pub fn items(&self) -> impl Iterator<Item = (&DocModule, &DocItem)> {
        self.modules
            .iter()
            .flat_map(|m| m.items.iter().map(move |i| (m, i)))
    }
}

/// Build the documentation surface of a resolved package.
///
/// `private` includes items the interface deliberately never carries;
/// without it the surface is exactly `pub` + `pub(pkg)`, the boundary
/// s12 already draws.
pub fn doc_package(res: &wolf_sema::Resolution, private: bool) -> DocPackage {
    let pkg = &res.package;
    let mut modules = Vec::new();
    for &m in &pkg.topo {
        let md = &pkg.modules[m];
        // Module doc: the `//!` block of the module's first file, in
        // load order (a directory module's files are already sorted).
        let doc = md
            .files
            .first()
            .and_then(|&f| module_doc(&pkg.files[f].parse.root, &pkg.files[f].raw.src));
        let head = md.files.first().map(|&f| &pkg.files[f].raw);
        let (mod_file, doc_span) = match head {
            Some(raw) => (raw.display.clone(), Some(Span::new(raw.file, 0, 0))),
            None => (String::new(), None),
        };
        let mut items: Vec<DocItem> = Vec::new();
        for item in &pkg.tables[m].items {
            if !private && item.vis == Vis::Private {
                continue;
            }
            let unit = &pkg.files[item.file];
            let node = unit
                .parse
                .root
                .nodes()
                .filter(|n| n.kind.is_item())
                .nth(item.decl);
            let doc = node
                .and_then(first_token)
                .and_then(|t| outer_doc(t, &unit.raw.src));
            items.push(DocItem {
                name: item.name.clone(),
                kind: item.kind,
                vis: item.vis,
                sig: wolf_sema::item_signature(pkg, m, item),
                doc,
                file: unit.raw.display.clone(),
                name_span: item.name_span,
                file_id: unit.raw.file,
            });
        }
        items.sort_by(|a, b| (a.kind.as_u8(), &a.name).cmp(&(b.kind.as_u8(), &b.name)));
        modules.push(DocModule {
            path: md.dotted(),
            doc,
            file: mod_file,
            doc_span,
            items,
        });
    }
    modules.sort_by(|a, b| a.path.cmp(&b.path));
    DocPackage {
        package: pkg.name.clone(),
        modules,
        private,
    }
}

/// Resolve every intra-doc link against the package's own name
/// resolution. An unresolved link is W1501 at the documented item's
/// name span — a build warning, an error under `--deny-warnings` (the
/// CI posture): `[List.push]` links or it breaks, so docs cannot rot
/// into stringly references.
pub fn resolve_links(docs: &DocPackage, res: &wolf_sema::Resolution) -> Vec<Diagnostic> {
    let pkg = &res.package;
    // Every name the package can name: `module.item`, `item` within its
    // own module, and every module path.
    let mut known: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (m, table) in pkg.tables.iter().enumerate() {
        let dotted = pkg.modules[m].dotted();
        if !dotted.is_empty() {
            known.insert(dotted.clone());
            if let Some(last) = dotted.rsplit('.').next() {
                known.insert(last.to_string());
            }
        }
        for item in &table.items {
            known.insert(item.name.clone());
            if dotted.is_empty() {
                continue;
            }
            known.insert(format!("{dotted}.{}", item.name));
            if let Some(last) = dotted.rsplit('.').next() {
                known.insert(format!("{last}.{}", item.name));
            }
        }
    }
    // Type members: `Point.x` resolves through the type's own fields
    // and the impls that carry its methods. v0 accepts `Type.member`
    // when `Type` is a known item — the member half is checked by the
    // typechecker on the doctest that exercises it, not by string.
    let mut out = Vec::new();
    let mut check = |target: &str, item: Option<&DocItem>, module: &DocModule| {
        let t = target.strip_suffix("()").unwrap_or(target);
        if known.contains(t) {
            return;
        }
        // `A.b`: accept when `A` is known (a member reference).
        if let Some((head, _)) = t.split_once('.')
            && known.contains(head)
        {
            return;
        }
        // Std paths are documented elsewhere; a `std.` link is a
        // cross-package reference the JSON index carries unresolved
        // rather than a rot report.
        if t.starts_with("std.") {
            return;
        }
        let (span, what) = match item {
            Some(i) => (Some(i.name_span), format!("`{}`", i.name)),
            None => (
                module.doc_span,
                if module.path.is_empty() {
                    "the package root".to_string()
                } else {
                    format!("module `{}`", module.path)
                },
            ),
        };
        // No span, no diagnostic: a warning without a location is noise.
        let Some(span) = span else { return };
        out.push(
            Diagnostic::warning(
                codes::W1501,
                span,
                format!("the doc comment on {what} links to `{target}`, which does not resolve"),
            )
            .with_note(
                "an intra-doc link resolves through name resolution, so a rename \
                 breaks it here instead of rotting into a dead reference. Write the \
                 path the code writes, or drop the brackets to make it prose."
                    .to_string(),
            ),
        );
    };
    for module in &docs.modules {
        if let Some(d) = &module.doc {
            for link in &d.links {
                check(link, None, module);
            }
        }
        for item in &module.items {
            if let Some(d) = &item.doc {
                for link in &d.links {
                    check(link, Some(item), module);
                }
            }
        }
    }
    out
}
