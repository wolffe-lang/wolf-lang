//! The queries (contract clauses 4–5): diagnostics-for-package,
//! type-at-position, def-of-symbol, document symbols, formatting.
//!
//! All of them run on a [`Snapshot`], return compiler values (byte
//! spans, [`wolf_diag::Diagnostic`]), and pass through the one catch
//! layer ([`Snapshot::guard`]) so cancellation surfaces as
//! `Err(Cancelled)` and never as a dropped request.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use wolf_ast::{Child, GreenNode, GreenToken, SyntaxKind};
use wolf_diag::Diagnostic;
use wolf_span::{FileId, SourceMap, Span};

use crate::host::{Cancelled, Snapshot};
use crate::overlay::OverlayLoader;

// ------------------------------------------------------------- results --

/// One source file of an analyzed package: identity, path, and the
/// exact text the analysis read (overlay or disk).
#[derive(Clone)]
pub struct SourceFile {
    pub file: FileId,
    pub path: PathBuf,
    pub text: Arc<Vec<u8>>,
}

/// The diagnostics of the package around one entry file — the same
/// codes at the same spans the CLI reports for the same bytes
/// (contract clause 5).
pub struct DiagnosticsBatch {
    /// Every file the analysis loaded (the entry alone when the ladder
    /// stopped at lex/parse). Span `FileId`s resolve through this list.
    pub files: Vec<SourceFile>,
    /// The full ladder output, deterministically sorted.
    pub diagnostics: Vec<Diagnostic>,
    /// The deepest phase the ladder completed for this package:
    /// `"lex" | "parse" | "resolve" | "typecheck"`.
    pub phase: &'static str,
}

impl DiagnosticsBatch {
    /// Resolve a span's file to its source entry.
    pub fn source(&self, file: FileId) -> Option<&SourceFile> {
        self.files.iter().find(|s| s.file == file)
    }
}

/// A type-at-position answer (the s13–s17 typed subset).
pub struct HoverResult {
    /// Rendered wolf source: `name: ty`, a bare type, or an item
    /// header (`fn count`).
    pub text: String,
    /// Doc-comment text (`///` lines, stripped), when trivially there.
    pub doc: Option<String>,
    /// The span the answer is about (highlight range).
    pub span: Span,
}

/// A def-of-symbol answer: the defining name token's location.
pub struct DefResult {
    pub path: PathBuf,
    pub span: Span,
}

/// Document-symbol kinds (the shim maps these onto LSP's enum).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SymbolKind {
    Fn,
    Struct,
    Enum,
    Variant,
    Field,
    Trait,
    Impl,
    TypeAlias,
    Const,
    Let,
    Var,
}

/// One entry of the document outline (from the parse tree — resilient,
/// answers on broken code too).
pub struct DocSymbol {
    pub name: String,
    pub kind: SymbolKind,
    /// The whole construct (outline range).
    pub full_span: Span,
    /// The defining name token (selection range).
    pub name_span: Span,
    pub children: Vec<DocSymbol>,
}

/// A whole-document format answer.
pub struct FormatResult {
    /// The canonical text. Byte-identical input formats to itself —
    /// the shim turns that into zero edits.
    pub text: Vec<u8>,
    /// Syntax errors were present: error regions passed through
    /// verbatim (`wolf_fmt` partial contract).
    pub partial: bool,
}

// ------------------------------------------------------- the analysis --

/// The memoized per-entry analysis (v0 execution model: computed at
/// most once per entry per revision; see the crate docs).
pub(crate) struct PackageAnalysis {
    pub(crate) files: Vec<SourceFile>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) phase: &'static str,
    /// Present from the resolve rung down (even when errors stop the
    /// ladder there — hover and def-of answer on what resolved).
    pub(crate) resolution: Option<wolf_sema::Resolution>,
    /// Present when the resolve rung passed clean; its diagnostics are
    /// withheld from `diagnostics` while any body is `NotYetCheckable`
    /// (the CLI's conservatism ledger, mirrored).
    pub(crate) typecheck: Option<wolf_sema::Typecheck>,
}

fn first_error(ds: &[Diagnostic]) -> bool {
    ds.iter().any(|d| d.severity == wolf_diag::Severity::Error)
}

impl Snapshot {
    /// Compute (or reuse) the package analysis around `entry`,
    /// mirroring the CLI's phase ladder exactly. `None` when the entry
    /// is unreadable.
    pub(crate) fn analysis(&self, entry: &Path) -> Option<Arc<PackageAnalysis>> {
        let key = crate::overlay::normalize(entry);
        let entry = key.as_path();
        if let Some(hit) = self
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
        {
            return Some(Arc::clone(hit));
        }
        let computed = Arc::new(self.compute_analysis(entry)?);
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, Arc::clone(&computed));
        Some(computed)
    }

    fn compute_analysis(&self, entry: &Path) -> Option<PackageAnalysis> {
        self.compute_analysis_from(entry, &self.overlays)
    }

    /// The same phase ladder, read through an explicit overlay set.
    /// The completion query analyzes *repaired* text (the `.partial`
    /// at the cursor deleted) through a patched clone of the
    /// snapshot's overlays; those results are never cached — the
    /// repair is per-request. Everything else is byte-identical to
    /// the memoized path.
    pub(crate) fn compute_analysis_from(
        &self,
        entry: &Path,
        overlays: &crate::overlay::OverlayStore,
    ) -> Option<PackageAnalysis> {
        self.begin();
        let text = overlays.read(entry)?;
        let mut sm = SourceMap::new();
        let id = sm.intern(entry);
        let entry_file = SourceFile {
            file: id,
            path: entry.to_path_buf(),
            text: Arc::clone(&text),
        };

        // Rung 1: lex. An error stops the ladder with the lex set only.
        let lexed = wolf_lex::lex(id, text.as_slice());
        if first_error(&lexed.diagnostics) {
            return Some(PackageAnalysis {
                files: vec![entry_file],
                diagnostics: lexed.diagnostics,
                phase: "lex",
                resolution: None,
                typecheck: None,
            });
        }
        self.checkpoint();

        // Rung 2: parse (entry only — package loading is rung 3's).
        let parsed = wolf_parse::parse_tokens(&lexed, text.as_slice());
        let mut all = lexed.diagnostics;
        all.extend(parsed.diagnostics);
        wolf_diag::sort_diagnostics(&mut all);
        if first_error(&all) {
            return Some(PackageAnalysis {
                files: vec![entry_file],
                diagnostics: all,
                phase: "parse",
                resolution: None,
                typecheck: None,
            });
        }
        self.checkpoint();

        // Rung 3: resolve the single-entry package through the overlay.
        let mut loader = OverlayLoader::new(entry, overlays, &mut sm)?;
        let res =
            wolf_sema::resolve_package(&mut loader, &wolf_sema::AliasTable::default()).ok()?;
        let files: Vec<SourceFile> = res
            .package
            .files
            .iter()
            .map(|u| SourceFile {
                file: u.raw.file,
                path: PathBuf::from(&u.raw.display),
                text: Arc::new(u.raw.src.clone()),
            })
            .collect();
        if first_error(&res.diagnostics) {
            let diagnostics = res.diagnostics.clone();
            return Some(PackageAnalysis {
                files,
                diagnostics,
                phase: "resolve",
                resolution: Some(res),
                typecheck: None,
            });
        }
        self.checkpoint();

        // Rung 4: typecheck. Partial type errors are withheld while any
        // body is NotYetCheckable (the CLI's rule, kept byte-for-byte);
        // the typed bodies still serve hover on what *was* checked.
        let tc = wolf_sema::typecheck_package(&res);
        let (diagnostics, phase) = if tc.not_yet.is_empty() {
            let mut all = res.diagnostics.clone();
            all.extend(tc.diagnostics.iter().cloned());
            wolf_diag::sort_diagnostics(&mut all);
            (all, "typecheck")
        } else {
            (res.diagnostics.clone(), "resolve")
        };
        Some(PackageAnalysis {
            files,
            diagnostics,
            phase,
            resolution: Some(res),
            typecheck: Some(tc),
        })
    }

    // -------------------------------------------------- public queries --

    /// Diagnostics for the package around `entry` (contract clause 5).
    /// `Ok(None)` when the entry is unreadable.
    pub fn diagnostics(&self, entry: &Path) -> Result<Option<DiagnosticsBatch>, Cancelled> {
        self.guard(|| {
            self.analysis(entry).map(|a| DiagnosticsBatch {
                files: a.files.clone(),
                diagnostics: a.diagnostics.clone(),
                phase: a.phase,
            })
        })
    }

    /// Type-at-position on the typed subset. `NotYetCheckable` regions
    /// (and files that never reached typecheck) answer `None` honestly
    /// — never a guess.
    pub fn type_at(&self, entry: &Path, offset: u32) -> Result<Option<HoverResult>, Cancelled> {
        self.guard(|| self.type_at_impl(entry, offset))
    }

    /// Def-of-symbol from s12 resolution: file-scoped import bindings,
    /// then module items; inside checked bodies, local bindings first.
    pub fn def_of(&self, entry: &Path, offset: u32) -> Result<Option<DefResult>, Cancelled> {
        self.guard(|| self.def_of_impl(entry, offset))
    }

    /// The document outline, from the parse tree alone (answers on
    /// broken and unresolved code — D22's resilience paying out).
    pub fn document_symbols(&self, path: &Path) -> Result<Option<Vec<DocSymbol>>, Cancelled> {
        self.guard(|| {
            self.begin();
            let text = self.file_text(path)?;
            let mut sm = SourceMap::new();
            let id = sm.intern(path);
            let parse = wolf_parse::parse_file(id, &text);
            Some(outline(&parse.root, &text))
        })
    }

    /// The documentation surface of the package around `entry` — the
    /// same model `wolf doc` renders and hover reads (s53). `Ok(None)`
    /// when the entry never reached resolution.
    pub fn docs(
        &self,
        entry: &Path,
        private: bool,
    ) -> Result<Option<crate::docs::DocPackage>, Cancelled> {
        self.guard(|| {
            let a = self.analysis(entry)?;
            let res = a.resolution.as_ref()?;
            Some(crate::docs::doc_package(res, private))
        })
    }

    /// Whole-document formatting via the one formatter (`wolf_fmt`).
    /// `Ok(None)` when the file is unreadable or the formatter fell
    /// back (self-check failure — never corrupt, so never edit).
    pub fn format(&self, path: &Path) -> Result<Option<FormatResult>, Cancelled> {
        self.guard(|| {
            self.begin();
            let text = self.file_text(path)?;
            let mut sm = SourceMap::new();
            let id = sm.intern(path);
            let out = wolf_fmt::format_source(id, &text);
            if out.fell_back {
                return None;
            }
            Some(FormatResult {
                text: out.text,
                partial: out.partial,
            })
        })
    }

    // ---------------------------------------------------------- innards --

    fn type_at_impl(&self, entry: &Path, offset: u32) -> Option<HoverResult> {
        let a = self.analysis(entry)?;
        let res = a.resolution.as_ref()?;
        let file_idx = file_index(res, entry)?;
        let unit = &res.package.files[file_idx];

        // Item-name hover answers from the parse tree (doc comments are
        // trivially available there), independent of the typed subset.
        if let Some(hit) = item_header_at(&unit.parse.root, &unit.raw.src, offset) {
            return Some(hit);
        }

        let tc = a.typecheck.as_ref()?;
        let (decl, member) = decl_at(&unit.parse.root, offset)?;
        let outcome = tc
            .bodies
            .iter()
            .find(|o| o.body.file == file_idx && o.body.decl == decl && o.body.member == member)?;
        let wolf_sema::BodyResult::Checked(tb) = &outcome.result else {
            return None; // NotYetCheckable / errored: nothing, honestly.
        };
        let render = |t| wolf_sema::types::render(&tb.table, t, &|_| Err("_"));
        // A local's name token answers `name: ty`; otherwise the
        // innermost recorded expression answers its type.
        if let Some((name, span, ty)) = tb
            .locals
            .iter()
            .find(|(_, s, _)| s.lo <= offset && offset < s.hi)
        {
            return Some(HoverResult {
                text: format!("{name}: {}", render(*ty)),
                doc: None,
                span: *span,
            });
        }
        let (span, ty) = tb
            .exprs
            .iter()
            .filter(|(s, _)| s.lo <= offset && offset < s.hi)
            .min_by_key(|(s, _)| s.hi - s.lo)?;
        // A subscript inside a 1-origin scope states the mode (D61,
        // `[gram.attr.index]`): the shift is invisible in the source,
        // so the hover is where it becomes visible.
        let doc = if tc.sigs.origins.origin_at(*span) == 1
            && bracket_apply_with_span(&unit.parse.root, *span)
        {
            Some(
                "Subscripts here count from 1 — an `index(1)` origin marker \
                 is in force (`[gram.attr.index]`)."
                    .to_string(),
            )
        } else {
            None
        };
        Some(HoverResult {
            text: render(*ty),
            doc,
            span: *span,
        })
    }

    fn def_of_impl(&self, entry: &Path, offset: u32) -> Option<DefResult> {
        let a = self.analysis(entry)?;
        let res = a.resolution.as_ref()?;
        let pkg = &res.package;
        let file_idx = file_index(res, entry)?;
        let unit = &pkg.files[file_idx];
        let token = ident_at(&unit.parse.root, offset)?;
        let text = String::from_utf8_lossy(token.text(&unit.raw.src)).into_owned();
        let module_idx = pkg
            .modules
            .iter()
            .position(|m| m.files.contains(&file_idx))?;
        let md = &pkg.modules[module_idx];
        let file_pos = md.files.iter().position(|&f| f == file_idx)?;

        // Locals shadow everything: nearest preceding same-named local
        // binding in the checked body containing the offset.
        if let Some(tc) = a.typecheck.as_ref()
            && let Some((decl, member)) = decl_at(&unit.parse.root, offset)
            && let Some(outcome) = tc
                .bodies
                .iter()
                .find(|o| o.body.file == file_idx && o.body.decl == decl && o.body.member == member)
            && let wolf_sema::BodyResult::Checked(tb) = &outcome.result
            && let Some((_, span, _)) = tb
                .locals
                .iter()
                .filter(|(n, s, _)| n == &text && s.lo <= offset)
                .max_by_key(|(_, s, _)| s.lo)
        {
            return Some(DefResult {
                path: pkg.files[file_idx].raw.display.clone().into(),
                span: *span,
            });
        }

        // File-scoped import bindings (s12).
        if let Some(binding) = md.bindings[file_pos].iter().find(|b| b.name == text) {
            match &binding.target {
                wolf_sema::BindTarget::Item { module, name } => {
                    let item = pkg.tables[*module].get(name)?;
                    let target = &pkg.files[item.file];
                    return Some(DefResult {
                        path: target.raw.display.clone().into(),
                        span: item.name_span,
                    });
                }
                wolf_sema::BindTarget::PkgModule(m) => {
                    let first = *pkg.modules[*m].files.first()?;
                    let target = &pkg.files[first];
                    return Some(DefResult {
                        path: target.raw.display.clone().into(),
                        span: Span::new(target.raw.file, 0, 0),
                    });
                }
                _ => {}
            }
        }

        // The module's own item table.
        let item = pkg.tables[module_idx].get(&text)?;
        let target = &pkg.files[item.file];
        Some(DefResult {
            path: target.raw.display.clone().into(),
            span: item.name_span,
        })
    }
}

// ------------------------------------------------------- tree helpers --

/// Position of `entry` within the loaded package's file list.
pub(crate) fn file_index(res: &wolf_sema::Resolution, entry: &Path) -> Option<usize> {
    let entry_display = crate::overlay::normalize(entry)
        .display()
        .to_string()
        .replace('\\', "/");
    res.package
        .files
        .iter()
        .position(|u| u.raw.display == entry_display)
}

/// The (decl ordinal, member ordinal) coordinates `BodyRef` uses, for
/// the item containing `offset`.
/// Is there a `BracketApply` node with exactly this span? The hover's
/// answering expression is a subscript precisely when there is.
fn bracket_apply_with_span(node: &GreenNode, span: Span) -> bool {
    if node.kind == SyntaxKind::BracketApply && node.span == span {
        return true;
    }
    node.nodes()
        .filter(|c| c.span.lo <= span.lo && span.hi <= c.span.hi)
        .any(|c| bracket_apply_with_span(c, span))
}

pub(crate) fn decl_at(root: &GreenNode, offset: u32) -> Option<(usize, Option<usize>)> {
    let (decl, node) = root
        .nodes()
        .filter(|n| n.kind.is_item())
        .enumerate()
        .find(|(_, n)| n.span.lo <= offset && offset < n.span.hi)?;
    if matches!(node.kind, SyntaxKind::ImplDecl | SyntaxKind::TraitDecl) {
        let member = node
            .nodes()
            .filter(|n| n.kind.is_item())
            .enumerate()
            .find(|(_, m)| m.span.lo <= offset && offset < m.span.hi)
            .map(|(i, _)| i);
        if member.is_some() {
            return Some((decl, member));
        }
    }
    Some((decl, None))
}

/// Depth-first identifier token containing `offset`.
fn ident_at(node: &GreenNode, offset: u32) -> Option<&GreenToken> {
    for child in &node.children {
        match child {
            Child::Token(t) => {
                if t.kind == SyntaxKind::Ident && t.span.lo <= offset && offset < t.span.hi {
                    return Some(t);
                }
            }
            Child::Node(n) => {
                if n.span.lo <= offset
                    && offset < n.span.hi
                    && let Some(t) = ident_at(n, offset)
                {
                    return Some(t);
                }
            }
        }
    }
    None
}

/// Hover on a top-level item's *name token*: header text + doc comment.
fn item_header_at(root: &GreenNode, src: &[u8], offset: u32) -> Option<HoverResult> {
    let item = root
        .nodes()
        .filter(|n| n.kind.is_item())
        .find(|n| n.span.lo <= offset && offset < n.span.hi)?;
    let (keyword, name_tok) = item_keyword_and_name(item)?;
    if !(name_tok.span.lo <= offset && offset < name_tok.span.hi) {
        return None;
    }
    // ONE doc-comment model (s53): hover reads what `wolf doc` reads.
    let doc = crate::docs::first_token(item)
        .and_then(|t| crate::docs::outer_doc(t, src))
        .map(|d| d.text);
    let name_text = String::from_utf8_lossy(name_tok.text(src));
    Some(HoverResult {
        text: format!("{keyword} {name_text}"),
        doc,
        span: name_tok.span,
    })
}

fn item_keyword_and_name(item: &GreenNode) -> Option<(&'static str, &GreenToken)> {
    let name = match item.kind {
        SyntaxKind::FnDecl => wolf_ast::FnDecl::cast(item)?.name()?,
        SyntaxKind::StructDecl => wolf_ast::StructDecl::cast(item)?.name()?,
        SyntaxKind::EnumDecl => wolf_ast::EnumDecl::cast(item)?.name()?,
        SyntaxKind::TraitDecl => wolf_ast::TraitDecl::cast(item)?.name()?,
        SyntaxKind::TypeDecl => wolf_ast::TypeDecl::cast(item)?.name()?,
        SyntaxKind::ConstDecl => wolf_ast::ConstDecl::cast(item)?.name()?,
        _ => return None,
    };
    let keyword = match item.kind {
        SyntaxKind::FnDecl => "fn",
        SyntaxKind::StructDecl => "struct",
        SyntaxKind::EnumDecl => "enum",
        SyntaxKind::TraitDecl => "trait",
        SyntaxKind::TypeDecl => "type",
        SyntaxKind::ConstDecl => "const",
        _ => unreachable!(),
    };
    Some((keyword, name))
}

// ------------------------------------------------------------ outline --

fn outline(root: &GreenNode, src: &[u8]) -> Vec<DocSymbol> {
    root.nodes()
        .filter(|n| n.kind.is_item())
        .filter_map(|n| symbol_of(n, src, false))
        .collect()
}

fn symbol_of(node: &GreenNode, src: &[u8], nested: bool) -> Option<DocSymbol> {
    let text = |t: &GreenToken| String::from_utf8_lossy(t.text(src)).into_owned();
    let leaf = |name: String, kind, name_span| DocSymbol {
        name,
        kind,
        full_span: node.span,
        name_span,
        children: Vec::new(),
    };
    match node.kind {
        SyntaxKind::FnDecl => {
            let d = wolf_ast::FnDecl::cast(node)?;
            let n = d.name()?;
            let _ = nested; // methods and free fns share the Fn kind in v0
            Some(leaf(text(n), SymbolKind::Fn, n.span))
        }
        SyntaxKind::StructDecl => {
            let d = wolf_ast::StructDecl::cast(node)?;
            let n = d.name()?;
            let children = d
                .fields()
                .filter_map(|f| {
                    let fname = f.name()?;
                    Some(DocSymbol {
                        name: text(fname),
                        kind: SymbolKind::Field,
                        full_span: f.syntax().span,
                        name_span: fname.span,
                        children: Vec::new(),
                    })
                })
                .collect();
            Some(DocSymbol {
                name: text(n),
                kind: SymbolKind::Struct,
                full_span: node.span,
                name_span: n.span,
                children,
            })
        }
        SyntaxKind::EnumDecl => {
            let d = wolf_ast::EnumDecl::cast(node)?;
            let n = d.name()?;
            let children = d
                .variants()
                .filter_map(|v| {
                    let vname = v.name()?;
                    Some(DocSymbol {
                        name: text(vname),
                        kind: SymbolKind::Variant,
                        full_span: v.syntax().span,
                        name_span: vname.span,
                        children: Vec::new(),
                    })
                })
                .collect();
            Some(DocSymbol {
                name: text(n),
                kind: SymbolKind::Enum,
                full_span: node.span,
                name_span: n.span,
                children,
            })
        }
        SyntaxKind::TraitDecl => {
            let d = wolf_ast::TraitDecl::cast(node)?;
            let n = d.name()?;
            let children = d
                .members()
                .filter_map(|m| symbol_of(m, src, true))
                .collect();
            Some(DocSymbol {
                name: text(n),
                kind: SymbolKind::Trait,
                full_span: node.span,
                name_span: n.span,
                children,
            })
        }
        SyntaxKind::ImplDecl => {
            let d = wolf_ast::ImplDecl::cast(node)?;
            // Header rendered from the parts present: `impl Trait for T`
            // or `impl T` — resilient on broken headers.
            let trait_name = d.trait_path().and_then(|p| p.segments().last().map(&text));
            let self_ty = d.self_ty().map(|t| {
                String::from_utf8_lossy(&src[t.span.lo as usize..t.span.hi as usize]).into_owned()
            });
            let name = match (trait_name, self_ty) {
                (Some(tr), Some(ty)) => format!("impl {tr} for {ty}"),
                (Some(tr), None) => format!("impl {tr}"),
                (None, Some(ty)) => format!("impl {ty}"),
                (None, None) => "impl".to_string(),
            };
            let name_span = d.trait_path().map(|p| p.syntax().span).unwrap_or(node.span);
            let children = d
                .members()
                .filter_map(|m| symbol_of(m, src, true))
                .collect();
            Some(DocSymbol {
                name,
                kind: SymbolKind::Impl,
                full_span: node.span,
                name_span,
                children,
            })
        }
        SyntaxKind::TypeDecl => {
            let d = wolf_ast::TypeDecl::cast(node)?;
            let n = d.name()?;
            Some(leaf(text(n), SymbolKind::TypeAlias, n.span))
        }
        SyntaxKind::ConstDecl => {
            let d = wolf_ast::ConstDecl::cast(node)?;
            let n = d.name()?;
            Some(leaf(text(n), SymbolKind::Const, n.span))
        }
        SyntaxKind::LetDecl | SyntaxKind::VarDecl => {
            let kind = if node.kind == SyntaxKind::LetDecl {
                SymbolKind::Let
            } else {
                SymbolKind::Var
            };
            // Binding names live in the pattern; take its identifiers.
            let pat = node.nodes().find(|n| wolf_ast::is_pattern_kind(n.kind))?;
            let first = pattern_idents(pat).next()?;
            Some(leaf(text(first), kind, first.span))
        }
        _ => None,
    }
}

pub(crate) fn pattern_idents(node: &GreenNode) -> impl Iterator<Item = &GreenToken> {
    // Flat walk: identifier tokens anywhere inside the pattern.
    fn collect<'a>(n: &'a GreenNode, out: &mut Vec<&'a GreenToken>) {
        for child in &n.children {
            match child {
                Child::Token(t) if t.kind == SyntaxKind::Ident => out.push(t),
                Child::Node(inner) => collect(inner, out),
                _ => {}
            }
        }
    }
    let mut v = Vec::new();
    collect(node, &mut v);
    v.into_iter()
}
