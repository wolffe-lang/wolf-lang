//! The completion query (s122): keywords, in-scope names, and member
//! completion after `.` — computed compiler-side so the LSP shim stays
//! a pure serialization boundary (clause 4: byte offsets in, compiler
//! values out; no LSP types below this line).
//!
//! # The incomplete buffer is the normal case
//!
//! Completion fires mid-edit, when the document usually does **not**
//! parse (a trailing `recv.` is a syntax error, and the phase ladder
//! stops at its first erroring rung). The query therefore never
//! demands a clean ladder:
//!
//! - **Name context** answers from the resilient parse of the entry
//!   file (D22: a tree exists for broken code too), plus the
//!   revision's memoized package analysis when the ladder got far
//!   enough — imports and sibling-file module items from resolve,
//!   inferred local types from typecheck.
//! - **Member context** (`recv.` / `recv.par`) re-runs the ladder on
//!   *repaired* text — the `.partialword` at the cursor deleted —
//!   through a patched clone of the snapshot's overlays, recovering
//!   the receiver's type exactly where the unrepaired buffer would
//!   refuse. When the repair still does not type the receiver, a
//!   syntactic fallback reads the receiver's declared annotation
//!   (`fn f(s: str)` / `let p: Point`); when that too fails, the
//!   answer is the **empty list** — conservative, never wrong, never
//!   an error (the track's rule: garbage member completion is worse
//!   than honest absence).
//! - **When parse recovery fails entirely** (the ladder dies at lex),
//!   keywords still answer, plus whatever items the best-effort tree
//!   recovered. Pinned by protocol tests in `wolf_lsp`.
//!
//! # Member sources and their residue
//!
//! Served: the builtin `str` method surface (s37/s120, mirrored in
//! [`STR_MEMBERS`] — a reviewed snapshot in `wolf_lsp` pins the list
//! against drift from `wolf_sema::check`'s table); struct fields and
//! impl methods of nominal types (from the elaborated signature
//! tables); enum variants and associated functions after an enum
//! *type name*; a module's exported items after an import binding.
//! Residue, refused with an empty list rather than guessed: members
//! of `List`/`Pool`/`Map`/`Chan`/`Mutex`/ptr and the other builtin
//! surfaces (their method tables live only in `wolf_sema::check`'s
//! call-typing today — lifting them into a queryable table is the
//! named follow-up), members through `shared`/`handle`/`weak`
//! wrappers, and the `std` stub namespace.
//!
//! No prefix filtering happens here: the client filters on the word
//! (the LSP model); the query returns everything valid at the cursor.

use std::collections::HashSet;
use std::path::Path;

use wolf_ast::{GreenNode, SyntaxKind};
use wolf_sema::graph::{BindTarget, Item, ItemKind, Package};
use wolf_sema::sig::ItemSig;
use wolf_sema::types::{self, Prim, TyId, TyKind, TypeTable};
use wolf_span::SourceMap;

use crate::host::{Cancelled, Snapshot};
use crate::queries::{PackageAnalysis, decl_at, file_index, pattern_idents};

// ------------------------------------------------------------- results --

/// Completion item kinds (the shim maps these onto LSP's enum).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompletionKind {
    Keyword,
    Function,
    Method,
    Variable,
    Field,
    Struct,
    Enum,
    Variant,
    Trait,
    TypeAlias,
    Const,
    Module,
}

/// One completion candidate: label + kind always, `detail` carrying
/// the signature or type wherever the compiler has it, doc-comment
/// text where it is trivially there.
#[derive(Clone, Debug)]
pub struct Completion {
    pub label: String,
    pub kind: CompletionKind,
    /// Rendered wolf source: a signature (`fn add(a: int) -> int`),
    /// a typing (`s: str`), or an item header.
    pub detail: Option<String>,
    /// Doc-comment text (`///` lines, stripped), when present.
    pub doc: Option<String>,
}

impl Completion {
    fn new(label: impl Into<String>, kind: CompletionKind) -> Completion {
        Completion {
            label: label.into(),
            kind,
            detail: None,
            doc: None,
        }
    }

    fn detail(mut self, d: impl Into<String>) -> Completion {
        self.detail = Some(d.into());
        self
    }
}

/// Collects completions with first-insert-wins deduplication by label
/// (locals shadow items shadow keywords — callers insert in scope
/// order).
#[derive(Default)]
struct Bag {
    out: Vec<Completion>,
    seen: HashSet<String>,
}

impl Bag {
    fn push(&mut self, c: Completion) {
        if self.seen.insert(c.label.clone()) {
            self.out.push(c);
        }
    }
}

// ---------------------------------------------------------- the query --

impl Snapshot {
    /// Completion candidates at `offset` (a byte offset into `entry`'s
    /// current text). `Ok(None)` when the entry is unreadable;
    /// otherwise always a list — possibly empty in member position
    /// when the receiver cannot be typed (conservative, never a
    /// guess). See the module docs for the incomplete-buffer
    /// contract.
    pub fn completions(
        &self,
        entry: &Path,
        offset: u32,
    ) -> Result<Option<Vec<Completion>>, Cancelled> {
        self.guard(|| self.completions_impl(entry, offset))
    }

    fn completions_impl(&self, entry: &Path, offset: u32) -> Option<Vec<Completion>> {
        self.begin();
        let entry = crate::overlay::normalize(entry);
        let text = self.file_text(&entry)?;
        let cursor = (offset as usize).min(text.len());
        let ws = word_start(&text, cursor);
        // `recv.par|` — member position: the byte before the partial
        // word is a `.` that is not part of a `..` range operator.
        if ws > 0 && text[ws - 1] == b'.' && !(ws > 1 && text[ws - 2] == b'.') {
            return Some(self.member_completions(&entry, &text, ws - 1, cursor));
        }
        Some(self.scope_completions(&entry, &text, cursor))
    }

    // ------------------------------------------------- name position --

    /// Keywords + in-scope names: locals and parameters of the
    /// enclosing body, the module's items, file import bindings.
    fn scope_completions(&self, entry: &Path, text: &[u8], cursor: usize) -> Vec<Completion> {
        let mut bag = Bag::default();
        let analysis = self.analysis(entry);

        // Locals first (they shadow): inferred types when the current
        // text's enclosing body checked, syntactic declarations (with
        // annotation details) always.
        if let Some(a) = &analysis {
            typed_locals(a, entry, cursor, &mut bag);
        }
        self.checkpoint();
        // Resilient parse of the current text — this answers on broken
        // buffers where the ladder stopped before resolve.
        let mut sm = SourceMap::new();
        let id = sm.intern(entry);
        let parse = wolf_parse::parse_file(id, text);
        syntactic_locals(&parse.root, text, cursor, &mut bag);

        // Module items + imports from the resolved package when the
        // ladder got there (mid-edit it usually still does: a partial
        // *name* is valid syntax, unlike a partial member).
        if let Some(a) = &analysis
            && let Some(res) = &a.resolution
            && let Some(file_idx) = file_index(res, entry)
        {
            resolution_scope(&res.package, file_idx, &mut bag);
        }
        self.checkpoint();

        // Entry-file items from the resilient tree — redundant when
        // resolve answered (dedup eats them), load-bearing when it
        // did not.
        syntactic_items(&parse.root, text, &mut bag);

        // Keywords last: reserved words can never collide with idents.
        for (kw, _) in wolf_lex::KEYWORDS {
            bag.push(Completion::new(*kw, CompletionKind::Keyword));
        }
        bag.out
    }

    // ----------------------------------------------- member position --

    /// Completion after `.`: repair the text (delete `.partial`),
    /// re-run the ladder on the patched overlay, and type the
    /// receiver. Routes, in order: typed receiver expression → named
    /// receiver (import binding / enum or struct type name) →
    /// declared-annotation fallback → empty.
    fn member_completions(
        &self,
        entry: &Path,
        text: &[u8],
        dot: usize,
        cursor: usize,
    ) -> Vec<Completion> {
        let we = word_end(text, cursor);
        let rs = word_start(text, dot);
        let recv_word = std::str::from_utf8(&text[rs..dot]).unwrap_or("");

        let mut patched = Vec::with_capacity(text.len());
        patched.extend_from_slice(&text[..dot]);
        patched.extend_from_slice(&text[we..]);
        let mut overlays = self.overlays.clone();
        overlays.set(entry.to_path_buf(), patched);
        let Some(a) = self.compute_analysis_from(entry, &overlays) else {
            return Vec::new();
        };
        self.checkpoint();

        if let Some(list) = typed_receiver_route(&a, entry, dot) {
            return list;
        }
        if !recv_word.is_empty() {
            if let Some(list) = named_receiver_route(&a, entry, recv_word) {
                return list;
            }
            if let Some(list) = annotation_route(self, &a, entry, text, dot, recv_word) {
                return list;
            }
        }
        Vec::new()
    }
}

// -------------------------------------------------- scope: the pieces --

/// Inferred `name: ty` for locals of the checked body containing
/// `cursor` — only when the *current* text typechecked (a clean
/// buffer; mid-edit the syntactic walk below carries the labels).
fn typed_locals(a: &PackageAnalysis, entry: &Path, cursor: usize, bag: &mut Bag) {
    let Some(res) = &a.resolution else { return };
    let Some(tc) = &a.typecheck else { return };
    let Some(file_idx) = file_index(res, entry) else {
        return;
    };
    let unit = &res.package.files[file_idx];
    let Some((decl, member)) = decl_at(&unit.parse.root, cursor as u32) else {
        return;
    };
    let Some(outcome) = tc
        .bodies
        .iter()
        .find(|o| o.body.file == file_idx && o.body.decl == decl && o.body.member == member)
    else {
        return;
    };
    let wolf_sema::BodyResult::Checked(tb) = &outcome.result else {
        return;
    };
    for (name, span, ty) in &tb.locals {
        if span.lo as usize <= cursor {
            bag.push(
                Completion::new(name.clone(), CompletionKind::Variable)
                    .detail(format!("{name}: {}", render(&tb.table, *ty))),
            );
        }
    }
}

/// Parameters and `let`/`var` bindings lexically in scope at `cursor`,
/// from the resilient tree: at each level, siblings that *end* before
/// the cursor count, then descend into the node containing it. Match
/// and `for` bindings ride the typed path only (named residue).
fn syntactic_locals(root: &GreenNode, src: &[u8], cursor: usize, bag: &mut Bag) {
    let Some(mut item) = root
        .nodes()
        .filter(|n| n.kind.is_item())
        .find(|n| n.span.lo as usize <= cursor && cursor <= n.span.hi as usize)
    else {
        return;
    };
    if matches!(item.kind, SyntaxKind::ImplDecl | SyntaxKind::TraitDecl)
        && let Some(m) = item
            .nodes()
            .filter(|n| n.kind.is_item())
            .find(|n| n.span.lo as usize <= cursor && cursor <= n.span.hi as usize)
    {
        item = m;
    }
    if item.kind == SyntaxKind::FnDecl
        && let Some(d) = wolf_ast::FnDecl::cast(item)
        && let Some(params) = d.params()
    {
        for p in params.params() {
            let Some(name) = p.name() else { continue };
            let label = String::from_utf8_lossy(name.text(src)).into_owned();
            let mut c = Completion::new(label.clone(), CompletionKind::Variable);
            if let Some(ty) = p.ty() {
                c = c.detail(format!("{label}: {}", node_text(ty, src)));
            }
            bag.push(c);
        }
    }
    bindings_in_scope(item, src, cursor, bag);
}

fn bindings_in_scope(node: &GreenNode, src: &[u8], cursor: usize, bag: &mut Bag) {
    for n in node.nodes() {
        let is_binding = matches!(n.kind, SyntaxKind::LetDecl | SyntaxKind::VarDecl);
        if is_binding && n.span.hi as usize <= cursor {
            push_binding(n, src, bag);
        } else if n.span.lo as usize <= cursor && cursor <= n.span.hi as usize {
            // The cursor sits inside: a binding's own initializer sees
            // outer bindings, not itself; descend either way.
            bindings_in_scope(n, src, cursor, bag);
        }
    }
}

fn push_binding(n: &GreenNode, src: &[u8], bag: &mut Bag) {
    let ty = n
        .nodes()
        .find(|c| wolf_ast::is_type_kind(c.kind))
        .map(|t| node_text(t, src));
    let Some(pat) = n.nodes().find(|c| wolf_ast::is_pattern_kind(c.kind)) else {
        return;
    };
    for tok in pattern_idents(pat) {
        let label = String::from_utf8_lossy(tok.text(src)).into_owned();
        let mut c = Completion::new(label.clone(), CompletionKind::Variable);
        if let Some(ty) = &ty {
            c = c.detail(format!("{label}: {ty}"));
        }
        bag.push(c);
    }
}

/// Import bindings + the module's merged item table (all files, one
/// namespace — D32), with elaborated signature details and doc text.
fn resolution_scope(pkg: &Package, file_idx: usize, bag: &mut Bag) {
    let Some(module_idx) = pkg.modules.iter().position(|m| m.files.contains(&file_idx)) else {
        return;
    };
    let md = &pkg.modules[module_idx];
    let Some(file_pos) = md.files.iter().position(|&f| f == file_idx) else {
        return;
    };
    for b in &md.bindings[file_pos] {
        match &b.target {
            BindTarget::PkgModule(m) => bag.push(
                Completion::new(b.name.clone(), CompletionKind::Module)
                    .detail(format!("module {}", pkg.modules[*m].dotted())),
            ),
            BindTarget::StdModule(_) => bag
                .push(Completion::new(b.name.clone(), CompletionKind::Module).detail("std module")),
            BindTarget::CNamespace => {
                bag.push(Completion::new(b.name.clone(), CompletionKind::Module).detail("import c"))
            }
            BindTarget::Item { module, name } => {
                if let Some(item) = pkg.tables[*module].get(name) {
                    bag.push(item_completion(pkg, *module, item));
                } else {
                    bag.push(Completion::new(b.name.clone(), CompletionKind::Variable));
                }
            }
            BindTarget::StdItem => {
                bag.push(Completion::new(b.name.clone(), CompletionKind::Variable).detail("std"));
            }
            BindTarget::Poisoned => {}
        }
    }
    for item in &pkg.tables[module_idx].items {
        bag.push(item_completion(pkg, module_idx, item));
    }
}

/// One module item as a completion: elaborated one-line signature +
/// doc comment (the same text `wolf doc` and hover read).
fn item_completion(pkg: &Package, module: usize, item: &Item) -> Completion {
    let kind = match item.kind {
        ItemKind::Fn => CompletionKind::Function,
        ItemKind::Struct => CompletionKind::Struct,
        ItemKind::Enum => CompletionKind::Enum,
        ItemKind::Type => CompletionKind::TypeAlias,
        ItemKind::Trait => CompletionKind::Trait,
        ItemKind::Const => CompletionKind::Const,
        ItemKind::Let | ItemKind::Var => CompletionKind::Variable,
    };
    let sig = wolf_sema::item_signature(pkg, module, item);
    let mut c = Completion::new(item.name.clone(), kind);
    if let Some(first) = sig.lines().next()
        && !first.trim().is_empty()
    {
        c = c.detail(first.trim().to_string());
    }
    c.doc = item_doc(pkg, item);
    c
}

fn item_doc(pkg: &Package, item: &Item) -> Option<String> {
    let unit = pkg.files.get(item.file)?;
    let node = unit
        .parse
        .root
        .nodes()
        .filter(|n| n.kind.is_item())
        .nth(item.decl)?;
    let tok = crate::docs::first_token(node)?;
    crate::docs::outer_doc(tok, &unit.raw.src).map(|d| d.text)
}

/// Top-level items of the entry file from the resilient tree — the
/// broken-buffer fallback when resolve never ran. Details are source
/// header slices, docs the item's own `///` block.
fn syntactic_items(root: &GreenNode, src: &[u8], bag: &mut Bag) {
    for n in root.nodes().filter(|n| n.kind.is_item()) {
        let kind = match n.kind {
            SyntaxKind::FnDecl => CompletionKind::Function,
            SyntaxKind::StructDecl => CompletionKind::Struct,
            SyntaxKind::EnumDecl => CompletionKind::Enum,
            SyntaxKind::TraitDecl => CompletionKind::Trait,
            SyntaxKind::TypeDecl => CompletionKind::TypeAlias,
            SyntaxKind::ConstDecl => CompletionKind::Const,
            SyntaxKind::LetDecl | SyntaxKind::VarDecl => CompletionKind::Variable,
            _ => continue, // impl blocks and imports name nothing here
        };
        let doc = crate::docs::first_token(n)
            .and_then(|t| crate::docs::outer_doc(t, src))
            .map(|d| d.text);
        if matches!(kind, CompletionKind::Variable) {
            push_binding(n, src, bag);
            continue;
        }
        let Some(name) = item_name(n, src) else {
            continue;
        };
        let mut c = Completion::new(name, kind);
        if let Some(h) = header_line(n, src) {
            c = c.detail(h);
        }
        c.doc = doc;
        bag.push(c);
    }
}

fn item_name(n: &GreenNode, src: &[u8]) -> Option<String> {
    let tok = match n.kind {
        SyntaxKind::FnDecl => wolf_ast::FnDecl::cast(n)?.name()?,
        SyntaxKind::StructDecl => wolf_ast::StructDecl::cast(n)?.name()?,
        SyntaxKind::EnumDecl => wolf_ast::EnumDecl::cast(n)?.name()?,
        SyntaxKind::TraitDecl => wolf_ast::TraitDecl::cast(n)?.name()?,
        SyntaxKind::TypeDecl => wolf_ast::TypeDecl::cast(n)?.name()?,
        SyntaxKind::ConstDecl => wolf_ast::ConstDecl::cast(n)?.name()?,
        _ => return None,
    };
    Some(String::from_utf8_lossy(tok.text(src)).into_owned())
}

/// The item's header as written: its source up to the body `{`, the
/// initializer `=`, or the first newline — whichever comes first.
fn header_line(n: &GreenNode, src: &[u8]) -> Option<String> {
    let slice = &src[n.span.lo as usize..n.span.hi as usize];
    let cut = slice
        .iter()
        .position(|&b| b == b'{' || b == b'=' || b == b'\n')
        .unwrap_or(slice.len());
    let head = String::from_utf8_lossy(&slice[..cut]).trim().to_string();
    if head.is_empty() { None } else { Some(head) }
}

// ------------------------------------------------- member: the routes --

/// Route 1: the receiver is a checked expression ending at the dot —
/// its recorded type names the member surface.
fn typed_receiver_route(a: &PackageAnalysis, entry: &Path, dot: usize) -> Option<Vec<Completion>> {
    let res = a.resolution.as_ref()?;
    let tc = a.typecheck.as_ref()?;
    let file_idx = file_index(res, entry)?;
    let unit = &res.package.files[file_idx];
    let (decl, member) = decl_at(&unit.parse.root, (dot as u32).saturating_sub(1))?;
    let outcome = tc
        .bodies
        .iter()
        .find(|o| o.body.file == file_idx && o.body.decl == decl && o.body.member == member)?;
    let wolf_sema::BodyResult::Checked(tb) = &outcome.result else {
        return None;
    };
    // The longest checked expression ending exactly at the dot — the
    // whole postfix chain, not its last segment.
    let (_, ty) = tb
        .exprs
        .iter()
        .filter(|(s, _)| s.hi as usize == dot)
        .max_by_key(|(s, _)| s.hi - s.lo)?;
    Some(members_of_type(&tb.table, *ty, tc))
}

/// Route 2: the receiver word names an import binding (module members)
/// or a type in scope (enum variants / associated functions).
fn named_receiver_route(
    a: &PackageAnalysis,
    entry: &Path,
    recv_word: &str,
) -> Option<Vec<Completion>> {
    let res = a.resolution.as_ref()?;
    let pkg = &res.package;
    let file_idx = file_index(res, entry)?;
    let module_idx = pkg
        .modules
        .iter()
        .position(|m| m.files.contains(&file_idx))?;
    let md = &pkg.modules[module_idx];
    let file_pos = md.files.iter().position(|&f| f == file_idx)?;

    if let Some(b) = md.bindings[file_pos].iter().find(|b| b.name == recv_word) {
        match &b.target {
            BindTarget::PkgModule(m) => {
                let mut bag = Bag::default();
                for item in &pkg.tables[*m].items {
                    if item.vis != wolf_sema::graph::Vis::Private {
                        bag.push(item_completion(pkg, *m, item));
                    }
                }
                return Some(bag.out);
            }
            // The std stub's contents are not modeled at v0: empty,
            // honestly, rather than invented.
            BindTarget::StdModule(_) | BindTarget::StdItem | BindTarget::CNamespace => {
                return Some(Vec::new());
            }
            BindTarget::Item { module, name } => {
                if let Some(item) = pkg.tables[*module].get(name)
                    && matches!(item.kind, ItemKind::Enum | ItemKind::Struct)
                {
                    return Some(type_name_members(a, pkg, *module, item));
                }
            }
            BindTarget::Poisoned => return Some(Vec::new()),
        }
    }
    let item = pkg.tables[module_idx].get(recv_word)?;
    if matches!(item.kind, ItemKind::Enum | ItemKind::Struct) {
        return Some(type_name_members(a, pkg, module_idx, item));
    }
    None
}

/// Route 3: no typed expression, but the receiver is a simple name
/// with a declared annotation in the enclosing function — read the
/// annotation text and complete from the type it names. This is what
/// keeps member completion alive when the repaired body still has a
/// type error elsewhere.
fn annotation_route(
    snap: &Snapshot,
    a: &PackageAnalysis,
    entry: &Path,
    text: &[u8],
    dot: usize,
    recv_word: &str,
) -> Option<Vec<Completion>> {
    // Prefer the resolved tree for the entry; fall back to a fresh
    // resilient parse of the *current* (unrepaired) text.
    let parsed;
    let (root, src): (&GreenNode, &[u8]) = match a.resolution.as_ref().and_then(|res| {
        let fi = file_index(res, entry)?;
        Some(&res.package.files[fi])
    }) {
        Some(unit) => (&unit.parse.root, &unit.raw.src),
        None => {
            let mut sm = SourceMap::new();
            let id = sm.intern(entry);
            parsed = wolf_parse::parse_file(id, text);
            (&parsed.root, text)
        }
    };
    snap.checkpoint();
    let ty_text = declared_annotation(root, src, dot, recv_word)?;

    if ty_text == "str" {
        return Some(str_members());
    }
    // A named type: find it in the resolved package (own module
    // first, then import bindings), then complete its value members.
    if let Some(res) = a.resolution.as_ref()
        && let Some(file_idx) = file_index(res, entry)
    {
        let pkg = &res.package;
        let module_idx = pkg
            .modules
            .iter()
            .position(|m| m.files.contains(&file_idx))?;
        let (m, item) = find_type_item(pkg, module_idx, file_idx, &ty_text)?;
        return Some(value_members_of_item(a, pkg, m, item));
    }
    // No resolution at all (ladder died before resolve): a syntactic
    // struct/enum in the entry file is still an honest answer.
    let decl = root.nodes().filter(|n| n.kind.is_item()).find(|n| {
        matches!(n.kind, SyntaxKind::StructDecl | SyntaxKind::EnumDecl)
            && item_name(n, src).as_deref() == Some(ty_text.as_str())
    })?;
    Some(syntactic_fields(decl, src))
}

/// The declared type annotation governing `recv_word` at `dot`: the
/// nearest preceding `let`/`var` with that name and an ascription,
/// else the enclosing function's parameter of that name.
fn declared_annotation(
    root: &GreenNode,
    src: &[u8],
    dot: usize,
    recv_word: &str,
) -> Option<String> {
    let probe = dot.saturating_sub(1);
    let mut item = root
        .nodes()
        .filter(|n| n.kind.is_item())
        .find(|n| n.span.lo as usize <= probe && probe <= n.span.hi as usize)?;
    if matches!(item.kind, SyntaxKind::ImplDecl | SyntaxKind::TraitDecl)
        && let Some(m) = item
            .nodes()
            .filter(|n| n.kind.is_item())
            .find(|n| n.span.lo as usize <= probe && probe <= n.span.hi as usize)
    {
        item = m;
    }
    // Nearest preceding annotated binding of that name (in-order walk;
    // the last hit is the innermost/shadowing one).
    fn annotated(n: &GreenNode, src: &[u8], dot: usize, name: &str, hit: &mut Option<String>) {
        for c in n.nodes() {
            let is_binding = matches!(c.kind, SyntaxKind::LetDecl | SyntaxKind::VarDecl);
            if is_binding && c.span.hi as usize <= dot {
                let names_match = c
                    .nodes()
                    .find(|p| wolf_ast::is_pattern_kind(p.kind))
                    .is_some_and(|p| pattern_idents(p).any(|t| t.text(src) == name.as_bytes()));
                if names_match && let Some(ty) = c.nodes().find(|t| wolf_ast::is_type_kind(t.kind))
                {
                    *hit = Some(node_text(ty, src));
                }
            } else if c.span.lo as usize <= dot && dot <= c.span.hi as usize {
                annotated(c, src, dot, name, hit);
            }
        }
    }
    let mut hit = None;
    annotated(item, src, dot, recv_word, &mut hit);
    if hit.is_some() {
        return hit;
    }
    if item.kind == SyntaxKind::FnDecl
        && let Some(d) = wolf_ast::FnDecl::cast(item)
        && let Some(params) = d.params()
    {
        for p in params.params() {
            if let Some(name) = p.name()
                && name.text(src) == recv_word.as_bytes()
                && let Some(ty) = p.ty()
            {
                return Some(node_text(ty, src));
            }
        }
    }
    None
}

// -------------------------------------------------- member: the types --

/// Value members of a type: struct fields + impl methods for
/// nominals, the builtin surface for `str`. Everything else answers
/// empty (the residue list in the module docs) — typed-but-unmodeled
/// must not guess.
fn members_of_type(table: &TypeTable, ty: TyId, tc: &wolf_sema::Typecheck) -> Vec<Completion> {
    match table.kind(ty) {
        TyKind::Prim(Prim::Str) => str_members(),
        TyKind::Nominal { module, name, .. } => {
            nominal_members(tc, *module as usize, name, Receiver::Value)
        }
        _ => Vec::new(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Receiver {
    /// A value of the type: fields + `self` methods.
    Value,
    /// The type name itself: enum variants + associated functions.
    TypeName,
}

fn nominal_members(
    tc: &wolf_sema::Typecheck,
    module: usize,
    name: &str,
    recv: Receiver,
) -> Vec<Completion> {
    let mut bag = Bag::default();
    let table = &tc.sigs.table;
    match tc.sigs.get(module, name) {
        Some(ItemSig::Struct(s)) if recv == Receiver::Value => {
            for f in &s.fields {
                bag.push(
                    Completion::new(f.name.clone(), CompletionKind::Field).detail(format!(
                        "{}: {}",
                        f.name,
                        render(table, f.ty)
                    )),
                );
            }
        }
        Some(ItemSig::Enum { variants, .. }) if recv == Receiver::TypeName => {
            for v in variants {
                let detail = if v.payload.is_empty() {
                    v.name.clone()
                } else {
                    let parts: Vec<String> = v.payload.iter().map(|t| render(table, *t)).collect();
                    format!("{}({})", v.name, parts.join(", "))
                };
                bag.push(Completion::new(v.name.clone(), CompletionKind::Variant).detail(detail));
            }
        }
        _ => {}
    }
    for im in &tc.sigs.impls {
        let TyKind::Nominal {
            module: m2,
            name: n2,
            ..
        } = table.kind(im.self_ty)
        else {
            continue;
        };
        if *m2 as usize != module || n2 != name {
            continue;
        }
        for meth in &im.methods {
            let wanted = match recv {
                Receiver::Value => meth.has_self,
                Receiver::TypeName => !meth.has_self,
            };
            if !wanted {
                continue;
            }
            let kind = if meth.has_self {
                CompletionKind::Method
            } else {
                CompletionKind::Function
            };
            bag.push(
                Completion::new(meth.name.clone(), kind)
                    .detail(render_fn_sig(table, &meth.name, &meth.sig)),
            );
        }
    }
    bag.out
}

/// Members after a type *name* (`Color.` / `Point.`): variants and
/// associated functions from the signature tables when typecheck ran,
/// else the syntactic declaration.
fn type_name_members(
    a: &PackageAnalysis,
    pkg: &Package,
    module: usize,
    item: &Item,
) -> Vec<Completion> {
    if let Some(tc) = &a.typecheck {
        return nominal_members(tc, module, &item.name, Receiver::TypeName);
    }
    item_decl_node(pkg, item)
        .map(|(n, src)| syntactic_variants(n, src))
        .unwrap_or_default()
}

/// Value members via an annotation's type name.
fn value_members_of_item(
    a: &PackageAnalysis,
    pkg: &Package,
    module: usize,
    item: &Item,
) -> Vec<Completion> {
    if let Some(tc) = &a.typecheck {
        return nominal_members(tc, module, &item.name, Receiver::Value);
    }
    if item.kind == ItemKind::Struct
        && let Some((n, src)) = item_decl_node(pkg, item)
    {
        return syntactic_fields(n, src);
    }
    Vec::new()
}

/// Resolve `ty_text` to a struct/enum item: own module's table first,
/// then this file's import bindings.
fn find_type_item<'p>(
    pkg: &'p Package,
    module_idx: usize,
    file_idx: usize,
    ty_text: &str,
) -> Option<(usize, &'p Item)> {
    if let Some(item) = pkg.tables[module_idx].get(ty_text)
        && matches!(item.kind, ItemKind::Struct | ItemKind::Enum)
    {
        return Some((module_idx, item));
    }
    let md = &pkg.modules[module_idx];
    let file_pos = md.files.iter().position(|&f| f == file_idx)?;
    let b = md.bindings[file_pos].iter().find(|b| b.name == ty_text)?;
    if let BindTarget::Item { module, name } = &b.target
        && let Some(item) = pkg.tables[*module].get(name)
        && matches!(item.kind, ItemKind::Struct | ItemKind::Enum)
    {
        return Some((*module, item));
    }
    None
}

fn item_decl_node<'p>(pkg: &'p Package, item: &Item) -> Option<(&'p GreenNode, &'p [u8])> {
    let unit = pkg.files.get(item.file)?;
    let n = unit
        .parse
        .root
        .nodes()
        .filter(|n| n.kind.is_item())
        .nth(item.decl)?;
    Some((n, &unit.raw.src))
}

fn syntactic_fields(decl: &GreenNode, src: &[u8]) -> Vec<Completion> {
    let mut bag = Bag::default();
    if decl.kind == SyntaxKind::StructDecl
        && let Some(d) = wolf_ast::StructDecl::cast(decl)
    {
        for f in d.fields() {
            let Some(name) = f.name() else { continue };
            let label = String::from_utf8_lossy(name.text(src)).into_owned();
            let mut c = Completion::new(label.clone(), CompletionKind::Field);
            if let Some(ty) = f.ty() {
                c = c.detail(format!("{label}: {}", node_text(ty, src)));
            }
            bag.push(c);
        }
    }
    bag.out
}

fn syntactic_variants(decl: &GreenNode, src: &[u8]) -> Vec<Completion> {
    let mut bag = Bag::default();
    if decl.kind == SyntaxKind::EnumDecl
        && let Some(d) = wolf_ast::EnumDecl::cast(decl)
    {
        for v in d.variants() {
            let Some(name) = v.name() else { continue };
            let label = String::from_utf8_lossy(name.text(src)).into_owned();
            bag.push(Completion::new(label, CompletionKind::Variant));
        }
    }
    bag.out
}

// ------------------------------------------------------ str builtins --

/// The builtin `str` method surface (s37 + s120's `chars`), mirrored
/// from `wolf_sema::check`'s `str_method_call` table. A reviewed
/// snapshot in `wolf_lsp` pins this list; growing the surface there
/// means growing it here in the same change.
const STR_MEMBERS: &[(&str, &str, &str)] = &[
    (
        "is_empty",
        "fn is_empty() -> bool",
        "`true` when the string has no bytes.",
    ),
    (
        "get",
        "fn get(range: Range[int]) -> str ! {none}",
        "Checked byte-range slice: out-of-range or split-code-point \
         offsets answer `{none}` instead of trapping.",
    ),
    (
        "bytes",
        "fn bytes() -> List[int]",
        "The byte view, materialized at v0.",
    ),
    (
        "chars",
        "fn chars() -> List[char]",
        "The code points as `char` values (s120; typed by D58).",
    ),
    (
        "starts_with",
        "fn starts_with(needle: str) -> bool",
        "Does the string begin with `needle`?",
    ),
    (
        "ends_with",
        "fn ends_with(needle: str) -> bool",
        "Does the string end with `needle`?",
    ),
    (
        "contains",
        "fn contains(needle: str) -> bool",
        "Does `needle` occur anywhere in the string?",
    ),
    (
        "find",
        "fn find(needle: str) -> int ! {none}",
        "Byte offset of the first occurrence; absence is `{none}`.",
    ),
    (
        "rfind",
        "fn rfind(needle: str) -> int ! {none}",
        "Byte offset of the last occurrence; absence is `{none}`.",
    ),
    (
        "count",
        "fn count(needle: str) -> int",
        "Non-overlapping occurrences of `needle`.",
    ),
    (
        "split",
        "fn split(sep: str) -> List[str]",
        "Substrings between occurrences of `sep`.",
    ),
    (
        "words",
        "fn words() -> List[str]",
        "Whitespace-separated words.",
    ),
    (
        "lines",
        "fn lines() -> List[str]",
        "The lines, without terminators.",
    ),
    (
        "trim",
        "fn trim() -> str",
        "Leading and trailing whitespace removed.",
    ),
    (
        "trim_start",
        "fn trim_start() -> str",
        "Leading whitespace removed.",
    ),
    (
        "trim_end",
        "fn trim_end() -> str",
        "Trailing whitespace removed.",
    ),
    ("lower", "fn lower() -> str", "Lowercased."),
    ("upper", "fn upper() -> str", "Uppercased."),
    (
        "strip_prefix",
        "fn strip_prefix(needle: str) -> str ! {none}",
        "The string after `needle`, or `{none}` when it is not a prefix.",
    ),
    (
        "strip_suffix",
        "fn strip_suffix(needle: str) -> str ! {none}",
        "The string before `needle`, or `{none}` when it is not a suffix.",
    ),
    (
        "repeat",
        "fn repeat(count: int) -> str",
        "The string repeated `count` times.",
    ),
    (
        "replace",
        "fn replace(from: str, to: str) -> str",
        "Every occurrence of `from` replaced with `to`.",
    ),
];

fn str_members() -> Vec<Completion> {
    STR_MEMBERS
        .iter()
        .map(|(name, sig, doc)| {
            let mut c = Completion::new(*name, CompletionKind::Method).detail(*sig);
            c.doc = Some((*doc).to_string());
            c
        })
        .collect()
}

// ------------------------------------------------------------ helpers --

fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

fn word_start(text: &[u8], cursor: usize) -> usize {
    let mut i = cursor;
    while i > 0 && is_ident_byte(text[i - 1]) {
        i -= 1;
    }
    i
}

fn word_end(text: &[u8], cursor: usize) -> usize {
    let mut i = cursor;
    while i < text.len() && is_ident_byte(text[i]) {
        i += 1;
    }
    i
}

fn node_text(n: &GreenNode, src: &[u8]) -> String {
    String::from_utf8_lossy(&src[n.span.lo as usize..n.span.hi as usize])
        .trim()
        .to_string()
}

fn render(table: &TypeTable, ty: TyId) -> String {
    types::render(table, ty, &|_| Err("_"))
}

/// `fn name(a: int, b: str) -> ret`, the receiver elided; a unit
/// return elides the arrow.
fn render_fn_sig(table: &TypeTable, name: &str, sig: &wolf_sema::sig::FnSig) -> String {
    let params: Vec<String> = sig
        .params
        .iter()
        .filter(|p| p.name != "self")
        .map(|p| format!("{}: {}", p.name, render(table, p.ty)))
        .collect();
    let ret = render(table, sig.ret);
    if ret == "()" {
        format!("fn {name}({})", params.join(", "))
    } else {
        format!("fn {name}({}) -> {ret}", params.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_boundaries() {
        let t = b"let s = ab.cd";
        assert_eq!(word_start(t, 13), 11); // |cd
        assert_eq!(word_start(t, 11), 11); // at the dot boundary
        assert_eq!(word_end(t, 11), 13);
        assert_eq!(word_start(t, 5), 4); // s|
    }

    #[test]
    fn str_member_table_is_sorted_unique() {
        // Determinism guard: labels unique (dedup would silently drop).
        let mut names: Vec<&str> = STR_MEMBERS.iter().map(|(n, _, _)| *n).collect();
        let len = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), len, "duplicate str member label");
    }
}
