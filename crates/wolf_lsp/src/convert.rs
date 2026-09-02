//! Compiler values → LSP wire shapes. Pure functions; every position
//! translation routes through [`crate::positions`] (the one-module
//! rule). Nothing here reaches back into `wolf_query`.

use std::collections::HashMap;
use std::path::Path;

use lsp_types::{
    CodeAction, CodeActionKind, CompletionItem, CompletionItemKind, DiagnosticRelatedInformation,
    DiagnosticSeverity, DocumentSymbol, Location, NumberOrString, Range,
    SymbolKind as LspSymbolKind, TextEdit, Url, WorkspaceEdit,
};
use wolf_query::{Completion, CompletionKind, DiagnosticsBatch, DocSymbol, SymbolKind};
use wolf_span::{LineIndex, Span};

use crate::positions::{Encoding, offset_to_position};

/// Span → range within one known file's text.
pub fn range_of(src: &[u8], index: &LineIndex, span: Span, enc: Encoding) -> Range {
    Range {
        start: offset_to_position(src, index, span.lo, enc),
        end: offset_to_position(src, index, span.hi, enc),
    }
}

/// A file's `Url`, echoing the client's own string when the document is
/// open (clients compare URIs byte-for-byte — report 09 hygiene list).
pub fn url_for(path: &Path, open_urls: &HashMap<std::path::PathBuf, Url>) -> Option<Url> {
    if let Some(u) = open_urls.get(path) {
        return Some(u.clone());
    }
    Url::from_file_path(path).ok()
}

/// The diagnostics of `batch` whose primary span lies in `path`,
/// converted. Secondary spans become related information (possibly in
/// other files); notes ride as appended message lines so clients that
/// render only `message` still show them.
pub fn diagnostics_for_file(
    batch: &DiagnosticsBatch,
    path: &Path,
    enc: Encoding,
    open_urls: &HashMap<std::path::PathBuf, Url>,
) -> Vec<lsp_types::Diagnostic> {
    let Some(target) = batch.files.iter().find(|f| f.path == path) else {
        return Vec::new();
    };
    let index = LineIndex::new(&target.text);
    batch
        .diagnostics
        .iter()
        .filter(|d| d.primary.span.file == target.file)
        .map(|d| to_lsp_diagnostic(batch, &target.text, &index, d, enc, open_urls))
        .collect()
}

fn to_lsp_diagnostic(
    batch: &DiagnosticsBatch,
    src: &[u8],
    index: &LineIndex,
    d: &wolf_diag::Diagnostic,
    enc: Encoding,
    open_urls: &HashMap<std::path::PathBuf, Url>,
) -> lsp_types::Diagnostic {
    let mut message = d.message.clone();
    if !d.primary.label.is_empty() {
        message.push_str(&format!("\n{}", d.primary.label));
    }
    for note in &d.notes {
        message.push_str(&format!("\nnote: {note}"));
    }
    let related: Vec<DiagnosticRelatedInformation> = d
        .secondary
        .iter()
        .filter_map(|sec| {
            let sf = batch.source(sec.span.file)?;
            let uri = url_for(&sf.path, open_urls)?;
            let idx = LineIndex::new(&sf.text);
            Some(DiagnosticRelatedInformation {
                location: Location {
                    uri,
                    range: range_of(&sf.text, &idx, sec.span, enc),
                },
                message: sec.label.clone(),
            })
        })
        .collect();
    lsp_types::Diagnostic {
        range: range_of(src, index, d.primary.span, enc),
        severity: Some(match d.severity {
            wolf_diag::Severity::Error => DiagnosticSeverity::ERROR,
            wolf_diag::Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(NumberOrString::String(d.code.as_str().to_string())),
        code_description: None,
        source: Some("wolf".to_string()),
        message,
        related_information: if related.is_empty() {
            None
        } else {
            Some(related)
        },
        tags: None,
        data: None,
    }
}

/// Machine-applicable fix-its overlapping `range_span` → quickfix code
/// actions, with fully-resolved inline edits (facsimile cannot resolve
/// lazily — report 09 hygiene list).
pub fn code_actions_for(
    batch: &DiagnosticsBatch,
    path: &Path,
    range_span: Span,
    enc: Encoding,
    open_urls: &HashMap<std::path::PathBuf, Url>,
) -> Vec<CodeAction> {
    let Some(target) = batch.files.iter().find(|f| f.path == path) else {
        return Vec::new();
    };
    let index = LineIndex::new(&target.text);
    let mut out = Vec::new();
    for d in &batch.diagnostics {
        if d.primary.span.file != target.file {
            continue;
        }
        // Overlap (inclusive at edges — a cursor *on* the squiggle
        // counts as touching it).
        let s = d.primary.span;
        if s.lo > range_span.hi || s.hi < range_span.lo {
            continue;
        }
        let lsp_diag = to_lsp_diagnostic(batch, &target.text, &index, d, enc, open_urls);
        for sugg in &d.suggestions {
            if sugg.applicability != wolf_diag::Applicability::MachineApplicable {
                continue;
            }
            let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
            let mut ok = true;
            for (span, replacement) in &sugg.edits {
                let Some(sf) = batch.source(span.file) else {
                    ok = false;
                    break;
                };
                let Some(uri) = url_for(&sf.path, open_urls) else {
                    ok = false;
                    break;
                };
                let idx = LineIndex::new(&sf.text);
                changes.entry(uri).or_default().push(TextEdit {
                    range: range_of(&sf.text, &idx, *span, enc),
                    new_text: replacement.clone(),
                });
            }
            if !ok {
                continue;
            }
            out.push(CodeAction {
                title: sugg.message.clone(),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![lsp_diag.clone()]),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    document_changes: None,
                    change_annotations: None,
                }),
                command: None,
                is_preferred: Some(true),
                disabled: None,
                data: None,
            });
        }
    }
    out
}

/// One file of a rename: its URI, the text the analysis read, and
/// the (span, replacement) edits ascending.
pub type FileEdit = (Url, std::sync::Arc<Vec<u8>>, Vec<(Span, String)>);

/// A rename's edit set → `WorkspaceEdit` (s133), in the shape the
/// client declared: `documentChanges` (a `TextDocumentEdit` per file,
/// `version: null` — the server tracks no versions) when
/// `document_changes`, the `changes` map otherwise. Files arrive in
/// the query's order (path); edits within a file ascending.
pub fn workspace_edit(files: &[FileEdit], enc: Encoding, document_changes: bool) -> WorkspaceEdit {
    let edits_of = |text: &[u8], edits: &[(Span, String)]| -> Vec<TextEdit> {
        let index = LineIndex::new(text);
        edits
            .iter()
            .map(|(span, new_text)| TextEdit {
                range: range_of(text, &index, *span, enc),
                new_text: new_text.clone(),
            })
            .collect()
    };
    if document_changes {
        let docs = files
            .iter()
            .map(|(uri, text, edits)| lsp_types::TextDocumentEdit {
                text_document: lsp_types::OptionalVersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: None,
                },
                edits: edits_of(text, edits)
                    .into_iter()
                    .map(lsp_types::OneOf::Left)
                    .collect(),
            })
            .collect();
        return WorkspaceEdit {
            changes: None,
            document_changes: Some(lsp_types::DocumentChanges::Edits(docs)),
            change_annotations: None,
        };
    }
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for (uri, text, edits) in files {
        changes.insert(uri.clone(), edits_of(text, edits));
    }
    WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}

fn lsp_completion_kind(kind: CompletionKind) -> CompletionItemKind {
    match kind {
        CompletionKind::Keyword => CompletionItemKind::KEYWORD,
        CompletionKind::Function => CompletionItemKind::FUNCTION,
        CompletionKind::Method => CompletionItemKind::METHOD,
        CompletionKind::Variable => CompletionItemKind::VARIABLE,
        CompletionKind::Field => CompletionItemKind::FIELD,
        CompletionKind::Struct => CompletionItemKind::STRUCT,
        CompletionKind::Enum => CompletionItemKind::ENUM,
        CompletionKind::Variant => CompletionItemKind::ENUM_MEMBER,
        CompletionKind::Trait => CompletionItemKind::INTERFACE,
        CompletionKind::TypeAlias => CompletionItemKind::CLASS,
        CompletionKind::Const => CompletionItemKind::CONSTANT,
        CompletionKind::Module => CompletionItemKind::MODULE,
    }
}

/// Completion candidates, converted: `kind` always (editors sort and
/// icon on it), `detail` carrying the signature or typing, doc text
/// as markdown (the same prose hover renders).
pub fn completion_items(items: &[Completion]) -> Vec<CompletionItem> {
    items
        .iter()
        .map(|c| CompletionItem {
            label: c.label.clone(),
            kind: Some(lsp_completion_kind(c.kind)),
            detail: c.detail.clone(),
            documentation: c.doc.as_ref().map(|d| {
                lsp_types::Documentation::MarkupContent(lsp_types::MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: d.clone(),
                })
            }),
            ..CompletionItem::default()
        })
        .collect()
}

fn lsp_symbol_kind(kind: SymbolKind) -> LspSymbolKind {
    match kind {
        SymbolKind::Fn => LspSymbolKind::FUNCTION,
        SymbolKind::Struct => LspSymbolKind::STRUCT,
        SymbolKind::Enum => LspSymbolKind::ENUM,
        SymbolKind::Variant => LspSymbolKind::ENUM_MEMBER,
        SymbolKind::Field => LspSymbolKind::FIELD,
        SymbolKind::Trait => LspSymbolKind::INTERFACE,
        SymbolKind::Impl => LspSymbolKind::OBJECT,
        SymbolKind::TypeAlias => LspSymbolKind::CLASS,
        SymbolKind::Const => LspSymbolKind::CONSTANT,
        SymbolKind::Let | SymbolKind::Var => LspSymbolKind::VARIABLE,
    }
}

/// The outline tree, converted. (`DocumentSymbol` carries a deprecated
/// mandatory field; the allow is scoped to its construction.)
pub fn document_symbols(
    src: &[u8],
    index: &LineIndex,
    syms: &[DocSymbol],
    enc: Encoding,
) -> Vec<DocumentSymbol> {
    syms.iter()
        .map(|s| {
            let children = document_symbols(src, index, &s.children, enc);
            #[allow(deprecated)]
            DocumentSymbol {
                name: s.name.clone(),
                detail: None,
                kind: lsp_symbol_kind(s.kind),
                tags: None,
                deprecated: None,
                range: range_of(src, index, s.full_span, enc),
                selection_range: range_of(src, index, s.name_span, enc),
                children: if children.is_empty() {
                    None
                } else {
                    Some(children)
                },
            }
        })
        .collect()
}
