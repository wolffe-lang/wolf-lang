//! The navigation queries (s133): definition, references, rename.
//!
//! Every answer comes from the binding table — the lexical half the
//! resolver kept ([`wolf_sema::Resolution::refs`]: every name it
//! bound, uses and binders alike) and the type-dependent half the
//! checker kept ([`wolf_sema::TypedBody::member_refs`]: fields,
//! variants, methods). Nothing here searches text. A name the
//! compiler never bound (a deferred error-row tag, a member on an
//! untyped receiver, anything inside a body that did not reach
//! typecheck) answers `None`, honestly.
//!
//! # Identity
//!
//! A [`Symbol`] is what a token at the cursor denotes. Declared
//! things — locals, items, fields, variants, methods — reduce to
//! their declaration's name token ([`Symbol::Decl`]), which is the key
//! both halves of the table share: a use in one file and a
//! declaration in another meet on that span (`FileId` included), so
//! cross-file navigation is a lookup, not a search. The rest have no
//! declaration to reach: a package module is a directory (D32), a
//! prelude name is ambient (D31), a builtin type is the compiler's,
//! std stubs and `import c` symbols live outside the package.
//!
//! # Rename's refusal set (the contract's letter)
//!
//! `rename` never produces a partial edit. It refuses BY NAME — a
//! reason naming the token — when the cursor is on a keyword
//! (`self` and `Self` included), a builtin type, a prelude name, a
//! std stub or `import c` symbol (cross-package), or a module (a
//! directory rename is not a text edit); and when the new name is not
//! an identifier or is a keyword. The D59 `//!` header carries no
//! identifier (`member: true` / `member: false` is a boolean), so no
//! rename ever touches a `//!` line — stated here so the question the
//! contract asks has its answer in one place.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wolf_ast::{Child, GreenNode, GreenToken, SyntaxKind};
use wolf_sema::sig::ItemSig;
use wolf_sema::{BodyResult, RefTarget, Resolution};
use wolf_span::Span;

use crate::host::{Cancelled, Snapshot};
use crate::queries::{DefResult, PackageAnalysis, file_index};

// ------------------------------------------------------------- results --

/// What the token under the cursor denotes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Symbol {
    /// Declared in this package: the declaration's name token.
    Decl(Span),
    /// A package module — a directory (D32), not a token.
    Module(usize),
    /// An ambient prelude name (D31).
    Prelude(String),
    /// A builtin type name.
    BuiltinType(String),
    /// A std stub module or item.
    Std,
    /// A symbol of the `c` namespace (`import c`).
    CNamespace,
    /// `Self`.
    SelfKw,
}

/// What `prepareRename` answers for a position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenamePrep {
    /// Renameable: the token's span and its current text (the
    /// placeholder).
    Ok { span: Span, name: String },
    /// Refused by name — the reason names the token and why.
    Refused(String),
}

/// One file's share of a rename.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEdits {
    pub path: PathBuf,
    /// (span, replacement), ascending by offset.
    pub edits: Vec<(Span, String)>,
}

/// What `rename` answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameOutcome {
    /// The whole edit — every reference and the declaration, per file.
    Edits(Vec<FileEdits>),
    /// Refused by name; no edit at all.
    Refused(String),
}

// --------------------------------------------------------- the queries --

impl Snapshot {
    /// Go to definition (s133): the declaration of the name at
    /// `offset`, with the token that asked (`origin`). `Ok(None)` when
    /// nothing navigable is there — a builtin, a prelude name, a
    /// keyword, a name the compiler never bound.
    pub fn definition(&self, entry: &Path, offset: u32) -> Result<Option<DefResult>, Cancelled> {
        self.guard(|| self.definition_impl(entry, offset))
    }

    /// Find references (s133): every use of the name at `offset`
    /// across the package, in (file, offset) order; the declaration
    /// included when `include_declaration`. `Ok(None)` when nothing
    /// navigable is there.
    pub fn references(
        &self,
        entry: &Path,
        offset: u32,
        include_declaration: bool,
    ) -> Result<Option<Vec<DefResult>>, Cancelled> {
        self.guard(|| self.references_impl(entry, offset, include_declaration))
    }

    /// `prepareRename` (s133): can the name at `offset` be renamed,
    /// and what is it? `Ok(None)` when there is no name there at all.
    pub fn prepare_rename(
        &self,
        entry: &Path,
        offset: u32,
    ) -> Result<Option<RenamePrep>, Cancelled> {
        self.guard(|| self.prepare_rename_impl(entry, offset))
    }

    /// Rename (s133): references + declaration rewritten to
    /// `new_name`, per file — or the refusal, never a partial edit.
    /// `Ok(None)` when there is no name at `offset`.
    pub fn rename(
        &self,
        entry: &Path,
        offset: u32,
        new_name: &str,
    ) -> Result<Option<RenameOutcome>, Cancelled> {
        self.guard(|| self.rename_impl(entry, offset, new_name))
    }

    // ---------------------------------------------------------- innards --

    fn definition_impl(&self, entry: &Path, offset: u32) -> Option<DefResult> {
        let a = self.analysis(entry)?;
        let res = a.resolution.as_ref()?;
        let file_idx = file_index(res, entry)?;
        let (origin, symbol) = symbol_at(&a, file_idx, offset)?;
        let origin = origin.span;
        match symbol {
            Symbol::Decl(span) => Some(DefResult {
                path: path_of(res, span)?,
                span,
                origin,
            }),
            // A module is a directory: its first file, at the top.
            Symbol::Module(m) => {
                let first = *res.package.modules[m].files.first()?;
                let target = &res.package.files[first];
                Some(DefResult {
                    path: target.raw.display.clone().into(),
                    span: Span::new(target.raw.file, 0, 0),
                    origin,
                })
            }
            _ => None,
        }
    }

    fn references_impl(
        &self,
        entry: &Path,
        offset: u32,
        include_declaration: bool,
    ) -> Option<Vec<DefResult>> {
        let a = self.analysis(entry)?;
        let res = a.resolution.as_ref()?;
        let file_idx = file_index(res, entry)?;
        let (origin, symbol) = symbol_at(&a, file_idx, offset)?;
        let spans = references_of(&a, &symbol, include_declaration);
        let mut out: Vec<DefResult> = spans
            .into_iter()
            .filter_map(|span| {
                Some(DefResult {
                    path: path_of(res, span)?,
                    span,
                    origin: origin.span,
                })
            })
            .collect();
        // Deterministic: file path, then offset (the contract's order).
        out.sort_by(|x, y| x.path.cmp(&y.path).then(x.span.lo.cmp(&y.span.lo)));
        out.dedup_by(|x, y| x.path == y.path && x.span == y.span);
        Some(out)
    }

    fn prepare_rename_impl(&self, entry: &Path, offset: u32) -> Option<RenamePrep> {
        let a = self.analysis(entry)?;
        let res = a.resolution.as_ref()?;
        let file_idx = file_index(res, entry)?;
        let unit = &res.package.files[file_idx];
        let token = token_at(&unit.parse.root, offset)?;
        let text = String::from_utf8_lossy(token.text(&unit.raw.src)).into_owned();
        // Keywords first (`self` in expression position lexes as an
        // identifier, so the check is by text as well as by kind);
        // a non-name token (a literal, punctuation) is nothing to
        // rename at all.
        if let Some(reason) = refuse_token(token.kind, &text) {
            return Some(RenamePrep::Refused(reason));
        }
        if token.kind != SyntaxKind::Ident {
            return None;
        }
        let (_, symbol) = symbol_at(&a, file_idx, offset)?;
        Some(match refuse_symbol(res, &symbol, &text) {
            Some(reason) => RenamePrep::Refused(reason),
            None => RenamePrep::Ok {
                span: token.span,
                name: text,
            },
        })
    }

    fn rename_impl(&self, entry: &Path, offset: u32, new_name: &str) -> Option<RenameOutcome> {
        let prep = self.prepare_rename_impl(entry, offset)?;
        let RenamePrep::Ok { .. } = prep else {
            let RenamePrep::Refused(reason) = prep else {
                unreachable!()
            };
            return Some(RenameOutcome::Refused(reason));
        };
        if let Some(reason) = refuse_new_name(new_name) {
            return Some(RenameOutcome::Refused(reason));
        }
        let refs = self.references_impl(entry, offset, true)?;
        let mut per_file: BTreeMap<PathBuf, Vec<(Span, String)>> = BTreeMap::new();
        for r in refs {
            per_file
                .entry(r.path)
                .or_default()
                .push((r.span, new_name.to_string()));
        }
        Some(RenameOutcome::Edits(
            per_file
                .into_iter()
                .map(|(path, edits)| FileEdits { path, edits })
                .collect(),
        ))
    }
}

// ----------------------------------------------------------- identity --

/// The token at `offset` and what it denotes, from the binding table.
pub(crate) fn symbol_at(
    a: &PackageAnalysis,
    file_idx: usize,
    offset: u32,
) -> Option<(&GreenToken, Symbol)> {
    let res = a.resolution.as_ref()?;
    let unit = &res.package.files[file_idx];
    let token = token_at(&unit.parse.root, offset)?;
    if token.kind != SyntaxKind::Ident {
        return None;
    }
    // The lexical half: uses and binders the resolver recorded.
    if let Some(r) = res.refs[file_idx].iter().find(|r| r.span == token.span) {
        return Some((token, target_symbol(res, &r.target)?));
    }
    // The type-dependent half: member uses the checker recorded, and
    // member declarations (their name tokens are the keys).
    let tc = a.typecheck.as_ref()?;
    for o in &tc.bodies {
        if o.body.file != file_idx {
            continue;
        }
        if let BodyResult::Checked(tb) = &o.result
            && let Some((_, decl)) = tb.member_refs.iter().find(|(u, _)| *u == token.span)
        {
            return Some((token, Symbol::Decl(*decl)));
        }
    }
    if is_member_decl(&tc.sigs, token.span) {
        return Some((token, Symbol::Decl(token.span)));
    }
    None
}

fn target_symbol(res: &Resolution, target: &RefTarget) -> Option<Symbol> {
    Some(match target {
        RefTarget::Local(s) => Symbol::Decl(*s),
        RefTarget::Item { module, name } => {
            Symbol::Decl(res.package.tables[*module].get(name)?.name_span)
        }
        RefTarget::Module(m) => Symbol::Module(*m),
        RefTarget::Std => Symbol::Std,
        RefTarget::CNamespace => Symbol::CNamespace,
        RefTarget::Prelude(n) => Symbol::Prelude(n.clone()),
        RefTarget::BuiltinType(n) => Symbol::BuiltinType(n.clone()),
        RefTarget::SelfKw => Symbol::SelfKw,
    })
}

/// Is `span` the name token of a field, variant, method, or trait
/// method declaration (the checker's decl keys)?
fn is_member_decl(sigs: &wolf_sema::SigTables, span: Span) -> bool {
    let in_items = sigs
        .modules
        .iter()
        .flat_map(|m| m.values())
        .any(|sig| match sig {
            ItemSig::Struct(s) => s.fields.iter().any(|f| f.span == span),
            ItemSig::Enum { variants, .. } => variants.iter().any(|v| v.span == span),
            _ => false,
        });
    in_items
        || sigs
            .impls
            .iter()
            .any(|i| i.methods.iter().any(|m| m.name_span == span))
        || sigs
            .traits
            .values()
            .any(|t| t.methods.iter().any(|m| m.name_span == span))
}

/// Every span that denotes `symbol` across the package: uses, and the
/// declaration when asked for.
fn references_of(a: &PackageAnalysis, symbol: &Symbol, include_declaration: bool) -> Vec<Span> {
    let Some(res) = a.resolution.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for file_refs in &res.refs {
        for r in file_refs {
            if target_symbol(res, &r.target).as_ref() == Some(symbol) {
                out.push(r.span);
            }
        }
    }
    if let Symbol::Decl(decl) = symbol {
        if let Some(tc) = a.typecheck.as_ref() {
            for o in &tc.bodies {
                if let BodyResult::Checked(tb) = &o.result {
                    out.extend(
                        tb.member_refs
                            .iter()
                            .filter(|(_, d)| d == decl)
                            .map(|(u, _)| *u),
                    );
                }
            }
        }
        if include_declaration {
            out.push(*decl);
        } else {
            out.retain(|s| s != decl);
        }
    }
    out
}

fn path_of(res: &Resolution, span: Span) -> Option<PathBuf> {
    res.package
        .files
        .iter()
        .find(|u| u.raw.file == span.file)
        .map(|u| PathBuf::from(&u.raw.display))
}

// ----------------------------------------------------------- refusals --

fn refuse_token(kind: SyntaxKind, text: &str) -> Option<String> {
    if wolf_lex::KEYWORDS.iter().any(|(kw, _)| *kw == text)
        || matches!(kind, SyntaxKind::SelfKw)
        || text == "self"
        || text == "Self"
    {
        return Some(format!("`{text}` is a keyword and cannot be renamed"));
    }
    None
}

fn refuse_symbol(res: &Resolution, symbol: &Symbol, text: &str) -> Option<String> {
    match symbol {
        Symbol::Decl(_) => None,
        Symbol::Module(m) => Some(format!(
            "`{text}` names the module {} — a directory (D32: directory = module), \
             not a token; rename the directory",
            res.package.modules[*m].display_name()
        )),
        Symbol::Prelude(n) => Some(format!(
            "`{n}` is a prelude name (D31): it has no declaration in this package"
        )),
        Symbol::BuiltinType(n) => Some(format!("`{n}` is a builtin type")),
        Symbol::Std => Some(format!(
            "`{text}` is a std item: it lives outside this package (cross-package \
             rename is refused)"
        )),
        Symbol::CNamespace => Some(format!(
            "`{text}` is a C symbol (`import c`): it lives outside this package"
        )),
        Symbol::SelfKw => Some("`Self` is a keyword and cannot be renamed".to_string()),
    }
}

/// The new name must lex as exactly one identifier that is not a
/// keyword.
fn refuse_new_name(new_name: &str) -> Option<String> {
    if wolf_lex::KEYWORDS.iter().any(|(kw, _)| *kw == new_name)
        || new_name == "self"
        || new_name == "Self"
    {
        return Some(format!("`{new_name}` is a keyword and cannot be a name"));
    }
    let mut sm = wolf_span::SourceMap::new();
    let id = sm.intern(Path::new("<rename>"));
    let lexed = wolf_lex::lex(id, new_name.as_bytes());
    // The lexer closes every input with a terminator and `Eof`;
    // neither is part of the name.
    let idents: Vec<_> = lexed
        .tokens
        .iter()
        .filter(|t| !matches!(t.kind, wolf_lex::TokenKind::Eof | wolf_lex::TokenKind::Term))
        .collect();
    let one_ident = idents.len() == 1
        && idents[0].kind == wolf_lex::TokenKind::Ident
        && (idents[0].span.hi - idents[0].span.lo) as usize == new_name.len();
    if lexed.has_errors() || !one_ident {
        return Some(format!("`{new_name}` is not an identifier"));
    }
    None
}

// ------------------------------------------------------- tree helpers --

/// Depth-first token containing `offset` (tokens carry their trivia,
/// so this is the code token — never whitespace or a comment).
pub(crate) fn token_at(node: &GreenNode, offset: u32) -> Option<&GreenToken> {
    for child in &node.children {
        match child {
            Child::Token(t) => {
                if t.span.lo <= offset && offset < t.span.hi {
                    return Some(t);
                }
            }
            Child::Node(n) => {
                if n.span.lo <= offset
                    && offset < n.span.hi
                    && let Some(t) = token_at(n, offset)
                {
                    return Some(t);
                }
            }
        }
    }
    None
}
