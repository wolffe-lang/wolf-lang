//! Name resolution, pass B (s12): signatures and bodies.
//!
//! Pass A ([`crate::graph`]) built the module graph, the per-module item
//! tables, and every file's import bindings. This pass walks each file's
//! declarations and resolves every name in signatures and bodies against
//! file-scoped imports + module items + the prelude — never against
//! types (D27): member access on values, error-row tags (a capitalized
//! name that fails lookup is a *candidate tag*, D30), and pattern paths
//! all defer to the type checker (s13).
//!
//! Modules resolve independently once the tables exist, so this pass
//! runs per-module in parallel (rayon) over the topological order;
//! `WOLF_SEMA_SINGLE_THREAD=1` forces the sequential fallback for
//! debugging. Each module's diagnostic sink is seeded with its files'
//! parse error regions — the s10 cascade-suppression contract — and the
//! merged output is deterministically ordered regardless of thread
//! interleaving.

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;
use wolf_ast::{
    Arg, ArgList, AsmExpr, Block, CallExpr, ClosureExpr, ConstDecl, ElseExpr, EnumDecl,
    EnumVariant, ErrorRow, FnDecl, ForExpr, GenericParamList, GreenNode, ImplDecl, LetDecl,
    MatchArm, MatchExpr, MemberExpr, Param, ParamList, PathExpr, PathType, RegionBlock, RetType,
    ScopeExpr, SelectArm, SpawnExpr, StringExpr, StructDecl, StructField, SyntaxKind, TraitDecl,
    TypeDecl, VarDecl, is_expr_kind, is_type_kind,
};
use wolf_diag::{Applicability, Diagnostic, Diagnostics, Suggestion, codes};
use wolf_span::Span;

use crate::graph::{
    AliasTable, BindTarget, Binding, ModuleLoader, Package, Vis, load_package, pattern_names,
};
use crate::prelude;

/// The environment variable that forces single-threaded resolution.
pub const SINGLE_THREAD_ENV: &str = "WOLF_SEMA_SINGLE_THREAD";

/// Every tag name spelled in an `ErrorRow` anywhere under `node` — the
/// lexical pre-scan behind the lowercase-tag deferral (#4): a lowercase
/// miss that matches a row written in the enclosing fn (return row,
/// parameter row, `let` annotation row) may be a tag, and only typing
/// can decide. Plain misses keep their E0301.
pub(crate) fn lexical_row_tags(node: &GreenNode, src: &[u8]) -> BTreeSet<String> {
    fn walk(node: &GreenNode, src: &[u8], out: &mut BTreeSet<String>) {
        if node.kind == SyntaxKind::ErrorRow
            && let Some(row) = wolf_ast::ErrorRow::cast(node)
        {
            for entry in row.entries() {
                if let Some(path) = entry.path() {
                    let name = path
                        .segments()
                        .map(|t| {
                            String::from_utf8_lossy(&src[t.span.lo as usize..t.span.hi as usize])
                                .into_owned()
                        })
                        .collect::<Vec<_>>()
                        .join(".");
                    out.insert(name);
                }
            }
        }
        for child in node.nodes() {
            walk(child, src, out);
        }
    }
    let mut out = BTreeSet::new();
    walk(node, src, &mut out);
    out
}

/// Every SINGLE-SEGMENT tag name spelled in an `ErrorRow` under `node`
/// that is not one of `generics` (a payload-less generic entry is the
/// row's polymorphic tail, not a tag — deferring it would trade E0301
/// for silence). The per-parameter scan behind D52's argument-position
/// deferral ([`Resolver::callee_param_rows`]); dotted tags need no
/// deferral (a bare identifier cannot spell one).
fn collect_bare_row_tags(
    node: &GreenNode,
    src: &[u8],
    generics: &BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    if node.kind == SyntaxKind::ErrorRow
        && let Some(row) = ErrorRow::cast(node)
    {
        for entry in row.entries() {
            let Some(path) = entry.path() else { continue };
            let segs: Vec<_> = path.segments().collect();
            if let [one] = segs.as_slice() {
                let name =
                    String::from_utf8_lossy(&src[one.span.lo as usize..one.span.hi as usize])
                        .into_owned();
                if !generics.contains(&name) {
                    out.insert(name);
                }
            }
        }
    }
    for child in node.nodes() {
        collect_bare_row_tags(child, src, generics, out);
    }
}

/// What one resolved name occurrence bound to (s133 — the binding
/// table). The resolver decides every name in signatures and bodies;
/// this is that decision, kept. `wolf lsp` navigates it (definition,
/// references, rename) and never runs a textual search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefTarget {
    /// A lexical binding in the same file — a parameter, a pattern
    /// binder, a generic parameter, a block-level item, a scope or
    /// region name, an import alias: the binder token's span. A
    /// binder records itself here too, so declaration and use share
    /// one identity.
    Local(Span),
    /// A module item (fn / struct / enum / trait / type / const /
    /// global), by module index and name — the package's own item
    /// table is the cross-file source of truth (D32 modules).
    Item { module: usize, name: String },
    /// A package module used as a namespace (`use geometry` →
    /// `geometry.area`).
    Module(usize),
    /// A std stub module or item (no source at v0).
    Std,
    /// The `c` FFI namespace (`import c`; header contents are c10's).
    CNamespace,
    /// An ambient prelude name (D31): no declaration to navigate to.
    Prelude(String),
    /// A builtin type name (`int`, `str`, `List`, …).
    BuiltinType(String),
    /// `Self` in a trait or impl (keyword-bound, no binder token).
    SelfKw,
}

/// One resolved name occurrence: the token's span and what it bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameRef {
    pub span: Span,
    pub target: RefTarget,
}

/// A fully resolved package: the loaded graph plus every diagnostic the
/// front half of sema can produce (parse, graph, resolution), in the
/// engine's deterministic order.
#[derive(Debug)]
pub struct Resolution {
    pub package: Package,
    pub diagnostics: Vec<Diagnostic>,
    /// The binding table (s133), one list per `package.files` entry:
    /// every name the resolver bound in that file, in visit order —
    /// uses AND binders (a binder is a `Local` ref to itself; an item
    /// declaration is an `Item` ref to itself; an import's bound name
    /// is a ref to what it imports). Names the resolver defers to
    /// typing (member access, error-row tags, pattern paths) are not
    /// here — the checker's `TypedBody::member_refs` carries those.
    pub refs: Vec<Vec<NameRef>>,
}

/// Load and resolve a package. Reads [`SINGLE_THREAD_ENV`] to pick the
/// sequential fallback; see [`resolve_package_with`] for explicit
/// control.
pub fn resolve_package(
    loader: &mut dyn ModuleLoader,
    aliases: &AliasTable,
) -> Result<Resolution, String> {
    let single = std::env::var(SINGLE_THREAD_ENV).is_ok_and(|v| v == "1");
    resolve_package_with(loader, aliases, single)
}

/// [`resolve_package`] with the threading choice made by the caller.
pub fn resolve_package_with(
    loader: &mut dyn ModuleLoader,
    aliases: &AliasTable,
    single_thread: bool,
) -> Result<Resolution, String> {
    let package = load_package(loader, aliases)?;
    let order: Vec<usize> = package.topo.clone();
    let run = |&m: &usize| resolve_module(&package, m);
    let per_module: Vec<ModuleResolved> = if single_thread {
        order.iter().map(run).collect()
    } else {
        order.par_iter().map(run).collect()
    };
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    for unit in &package.files {
        diagnostics.extend(unit.parse.diagnostics.iter().cloned());
    }
    diagnostics.extend(package.diagnostics.iter().cloned());
    let mut refs: Vec<Vec<NameRef>> = package.files.iter().map(|_| Vec::new()).collect();
    for m in per_module {
        diagnostics.extend(m.diagnostics);
        for (file, file_refs) in m.refs {
            refs[file] = file_refs;
        }
    }
    // The package-shape pass (the idiom arbiter): module-graph lints
    // that no per-module walk can see. Cascade suppression applies as
    // everywhere on the resolve rung.
    let mut pkg_sink = Diagnostics::new();
    for unit in &package.files {
        for &region in &unit.parse.error_regions {
            pkg_sink.suppress(region);
        }
    }
    crate::wave::check_package(&package, &mut pkg_sink);
    diagnostics.extend(pkg_sink.into_vec());
    wolf_diag::sort_diagnostics(&mut diagnostics);
    Ok(Resolution {
        package,
        diagnostics,
        refs,
    })
}

/// One module's resolve output: its diagnostics and, per member file,
/// the binding table.
struct ModuleResolved {
    diagnostics: Vec<Diagnostic>,
    refs: Vec<(usize, Vec<NameRef>)>,
}

/// Resolve one module's files (independent of every other module once
/// pass A has run — the parallel unit).
fn resolve_module(pkg: &Package, module: usize) -> ModuleResolved {
    let mut sink = Diagnostics::new();
    let mut refs = Vec::new();
    for &fi in &pkg.modules[module].files {
        for &region in &pkg.files[fi].parse.error_regions {
            sink.suppress(region);
        }
    }
    for (slot, &fi) in pkg.modules[module].files.iter().enumerate() {
        let bindings = &pkg.modules[module].bindings[slot];
        let mut r = Resolver {
            pkg,
            module,
            file: fi,
            bindings,
            used: vec![false; bindings.len()],
            scopes: Vec::new(),
            row_tags: Vec::new(),
            sink: &mut sink,
            refs: Vec::new(),
            aliased: vec![false; bindings.len()],
        };
        r.run();
        refs.push((fi, r.refs));
    }
    // E0410 — `let` immutability — is a resolve-rung law (binding
    // structure is lexical knowledge; DIV-2026-010): checked here, per
    // file, against the module's item-level globals.
    let globals = crate::letcheck::module_globals(pkg, module);
    for &fi in &pkg.modules[module].files {
        crate::letcheck::check_file(pkg, fi, &globals, &mut sink);
    }
    // The s68 lint wave's resolve-rung pass (syntax + lexical binding
    // structure only — see `wave`): warnings, never errors.
    for &fi in &pkg.modules[module].files {
        crate::wave::check_file(pkg, module, fi, &mut sink);
    }
    ModuleResolved {
        diagnostics: sink.into_vec(),
        refs,
    }
}

// ------------------------------------------------------------ resolver --

struct Resolver<'a> {
    pkg: &'a Package,
    module: usize,
    file: usize,
    bindings: &'a [Binding],
    used: Vec<bool>,
    /// Lexical scopes, innermost last: name → the binder token's span
    /// (`None` for the keyword-bound `Self`).
    scopes: Vec<BTreeMap<String, Option<Span>>>,
    /// Per enclosing fn (a stack, for nested items): every tag name
    /// spelled in one of its error rows — the lowercase-deferral set
    /// ([`lexical_row_tags`], #4).
    row_tags: Vec<BTreeSet<String>>,
    sink: &'a mut Diagnostics,
    /// The binding table under construction (s133).
    refs: Vec<NameRef>,
    /// Per import binding: bound through an alias (`use m.x as y`)?
    aliased: Vec<bool>,
}

impl Resolver<'_> {
    fn src(&self) -> &[u8] {
        &self.pkg.files[self.file].raw.src
    }

    fn text(&self, span: Span) -> String {
        let src = self.src();
        String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned()
    }

    fn run(&mut self) {
        let root = &self.pkg.files[self.file].parse.root;
        self.record_declarations(root);
        for node in root.nodes().filter(|n| n.kind.is_item()) {
            self.resolve_item(node);
        }
        self.report_unused_imports(root);
    }

    /// Record one resolved occurrence in the binding table.
    fn record(&mut self, span: Span, target: RefTarget) {
        self.refs.push(NameRef { span, target });
    }

    /// The declaration-side entries of the binding table (s133): every
    /// module item's name token as an `Item` ref to itself, and every
    /// import's bound name as a ref to what it imports — so a
    /// declaration and its uses share one identity, and a `use` line
    /// is a reference the rename edit reaches. An aliased import
    /// (`use m.item as alias`) binds the alias as a `Local` (uses of
    /// the alias must not follow an item rename) and records the
    /// path's item segment as the `Item` ref.
    fn record_declarations(&mut self, root: &GreenNode) {
        for item in self.pkg.tables[self.module].items.iter() {
            if item.file == self.file {
                self.record(
                    item.name_span,
                    RefTarget::Item {
                        module: self.module,
                        name: item.name.clone(),
                    },
                );
            }
        }
        let aliased: Vec<(Span, Span)> = root
            .nodes()
            .filter(|n| n.kind == SyntaxKind::UseDecl)
            .filter_map(wolf_ast::UseDecl::cast)
            .filter_map(|u| {
                let alias = u.alias()?;
                let last = u.path()?.segments().last()?;
                Some((alias.span, last.span))
            })
            .collect();
        for (i, b) in self.bindings.iter().enumerate() {
            let target = match &b.target {
                BindTarget::Item { module, name } => RefTarget::Item {
                    module: *module,
                    name: name.clone(),
                },
                BindTarget::PkgModule(m) => RefTarget::Module(*m),
                BindTarget::StdModule(_) | BindTarget::StdItem => RefTarget::Std,
                BindTarget::CNamespace => RefTarget::CNamespace,
                BindTarget::Poisoned => continue,
            };
            match aliased.iter().find(|(a, _)| *a == b.name_span) {
                Some((alias, item_seg)) => {
                    self.aliased[i] = true;
                    self.refs.push(NameRef {
                        span: *alias,
                        target: RefTarget::Local(*alias),
                    });
                    self.refs.push(NameRef {
                        span: *item_seg,
                        target,
                    });
                }
                None => self.refs.push(NameRef {
                    span: b.name_span,
                    target,
                }),
            }
        }
    }

    /// The target a use of import binding `i` records: the item it
    /// names, or the alias binder when the import is aliased.
    fn binding_target(&self, i: usize) -> RefTarget {
        let b = &self.bindings[i];
        if self.aliased[i] {
            return RefTarget::Local(b.name_span);
        }
        match &b.target {
            BindTarget::Item { module, name } => RefTarget::Item {
                module: *module,
                name: name.clone(),
            },
            BindTarget::PkgModule(m) => RefTarget::Module(*m),
            BindTarget::StdModule(_) | BindTarget::StdItem => RefTarget::Std,
            BindTarget::CNamespace => RefTarget::CNamespace,
            BindTarget::Poisoned => RefTarget::Local(b.name_span),
        }
    }

    // ------------------------------------------------------ items ------

    fn resolve_item(&mut self, node: &GreenNode) {
        match node.kind {
            SyntaxKind::FnDecl => self.resolve_fn(node),
            SyntaxKind::StructDecl => {
                if let Some(d) = StructDecl::cast(node) {
                    self.push_scope();
                    self.bind_generics(d.generics());
                    for f in d.fields() {
                        self.resolve_field(f);
                    }
                    self.pop_scope();
                }
            }
            SyntaxKind::EnumDecl => {
                if let Some(d) = EnumDecl::cast(node) {
                    self.push_scope();
                    self.bind_generics(d.generics());
                    for v in d.variants() {
                        self.resolve_variant(v);
                    }
                    self.pop_scope();
                }
            }
            SyntaxKind::TypeDecl => {
                if let Some(d) = TypeDecl::cast(node) {
                    self.push_scope();
                    self.bind_generics(d.generics());
                    if let Some(def) = d.def() {
                        self.resolve_type_def(def);
                    }
                    self.pop_scope();
                }
            }
            SyntaxKind::TraitDecl => {
                if let Some(d) = TraitDecl::cast(node) {
                    self.push_scope();
                    // `Self` names the implementing type inside trait
                    // and impl member signatures (s14).
                    self.bind("Self".to_string(), None);
                    self.bind_generics(d.generics());
                    for m in d.members() {
                        self.resolve_item(m);
                    }
                    self.pop_scope();
                }
            }
            SyntaxKind::ImplDecl => {
                if let Some(d) = ImplDecl::cast(node) {
                    self.push_scope();
                    self.bind("Self".to_string(), None);
                    self.bind_generics(d.generics());
                    if let Some(p) = d.trait_path() {
                        self.resolve_path_first(p.syntax(), false);
                    }
                    if let Some(t) = d.self_ty() {
                        self.resolve_type(t);
                    }
                    for m in d.members() {
                        self.resolve_item(m);
                    }
                    self.pop_scope();
                }
            }
            SyntaxKind::ConstDecl => {
                if let Some(d) = ConstDecl::cast(node) {
                    if let Some(t) = d.ty() {
                        self.resolve_type(t);
                    }
                    if let Some(e) = d.init() {
                        self.resolve_expr(e);
                    }
                }
            }
            SyntaxKind::LetDecl => {
                if let Some(d) = LetDecl::cast(node) {
                    for b in d.binders() {
                        if let Some(t) = b.ty {
                            self.resolve_type(t);
                        }
                        if let Some(e) = b.init {
                            self.resolve_expr(e);
                        }
                    }
                }
            }
            SyntaxKind::VarDecl => {
                if let Some(d) = VarDecl::cast(node) {
                    for b in d.binders() {
                        if let Some(t) = b.ty {
                            self.resolve_type(t);
                        }
                        if let Some(e) = b.init {
                            self.resolve_expr(e);
                        }
                    }
                }
            }
            // Imports were fully handled by pass A.
            SyntaxKind::UseDecl | SyntaxKind::ImportCDecl => {}
            _ => {}
        }
    }

    fn resolve_fn(&mut self, node: &GreenNode) {
        let Some(d) = FnDecl::cast(node) else { return };
        self.row_tags.push(lexical_row_tags(node, self.src()));
        self.push_scope();
        self.bind_generics(d.generics());
        if let Some(params) = d.params() {
            for p in params.params() {
                if let Some(t) = p.ty() {
                    self.resolve_type(t);
                }
            }
        }
        if let Some(ret) = d.ret_ty() {
            self.resolve_ret(ret);
        }
        self.push_scope();
        if let Some(params) = d.params() {
            self.bind_params(params);
        }
        if let Some(body) = d.body() {
            self.resolve_block(body);
        }
        self.pop_scope();
        self.pop_scope();
        self.row_tags.pop();
    }

    fn resolve_field(&mut self, f: StructField<'_>) {
        if let Some(t) = f.ty() {
            self.resolve_type(t);
        }
    }

    fn resolve_variant(&mut self, v: EnumVariant<'_>) {
        for t in v.payload() {
            self.resolve_type(t);
        }
    }

    fn resolve_type_def(&mut self, def: &GreenNode) {
        match def.kind {
            SyntaxKind::StructDef => {
                for f in def.nodes().filter_map(StructField::cast) {
                    self.resolve_field(f);
                }
            }
            SyntaxKind::EnumDef => {
                for v in def.nodes().filter_map(EnumVariant::cast) {
                    self.resolve_variant(v);
                }
            }
            _ => self.resolve_type(def),
        }
    }

    // ------------------------------------------------------ scopes -----

    fn push_scope(&mut self) {
        self.scopes.push(BTreeMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Bind `name` in the innermost scope. A binder with a token
    /// records itself in the binding table (s133: declaration and use
    /// share one identity); the keyword-bound `Self` has none.
    fn bind(&mut self, name: String, span: Option<Span>) {
        if let Some(s) = span {
            self.record(s, RefTarget::Local(s));
        }
        if let Some(top) = self.scopes.last_mut() {
            top.insert(name, span);
        }
    }

    fn in_scope(&self, name: &str) -> bool {
        self.lookup(name).is_some()
    }

    /// The innermost lexical binding of `name`: `Some(binder span)`,
    /// or `Some(None)` for `Self`.
    fn lookup(&self, name: &str) -> Option<Option<Span>> {
        self.scopes.iter().rev().find_map(|s| s.get(name).copied())
    }

    fn bind_generics(&mut self, list: Option<GenericParamList<'_>>) {
        let Some(list) = list else { return };
        // Bind every parameter first (bounds may mention them), then
        // resolve the bounds.
        let names: Vec<(String, Span)> = list
            .params()
            .filter_map(|p| p.name().map(|t| (self.text(t.span), t.span)))
            .collect();
        for (n, s) in names {
            self.bind(n, Some(s));
        }
        for p in list.params() {
            if let Some(bound) = p.bound() {
                for path in bound.paths() {
                    self.resolve_path_first(path.syntax(), false);
                }
            }
        }
    }

    fn bind_params(&mut self, params: ParamList<'_>) {
        for p in params.params() {
            if p.is_self() {
                // The receiver binds `self` to its own keyword token,
                // so uses of `self` navigate to the parameter and
                // references stay body-scoped.
                let span = p.syntax().child_token(SyntaxKind::SelfKw).map(|t| t.span);
                self.bind("self".to_string(), span);
            } else if let Some(t) = p.name() {
                let name = self.text(t.span);
                self.bind(name, Some(t.span));
            }
        }
    }

    fn bind_pattern(&mut self, pat: &GreenNode) {
        let names = pattern_names(pat, self.src());
        for (n, s) in names {
            self.bind(n, Some(s));
        }
    }

    // ------------------------------------------------------ lookup -----

    fn binding_index(&self, name: &str) -> Option<usize> {
        self.bindings.iter().position(|b| b.name == name)
    }

    /// Every name visible here, canonically ordered (suggestion fodder).
    fn candidates(&self) -> Vec<String> {
        let mut set: BTreeSet<String> = BTreeSet::new();
        for s in &self.scopes {
            set.extend(s.keys().cloned());
        }
        set.extend(self.bindings.iter().map(|b| b.name.clone()));
        set.extend(self.pkg.tables[self.module].names().map(str::to_string));
        set.extend(prelude::PRELUDE.iter().map(|s| s.to_string()));
        set.extend(prelude::BUILTIN_TYPES.iter().map(|s| s.to_string()));
        set.into_iter().collect()
    }

    /// Resolve a single name occurrence. `defer_tags` applies D30's
    /// rule: in expression position a capitalized miss may be an
    /// error-row tag, whose existence only typing can decide.
    fn resolve_name(&mut self, name: &str, span: Span, defer_tags: bool) {
        if let Some(binder) = self.lookup(name) {
            let target = binder.map_or(RefTarget::SelfKw, RefTarget::Local);
            self.record(span, target);
            return;
        }
        if name == "self" {
            return; // a bare `self` outside any receiver: s17's
        }
        if let Some(i) = self.binding_index(name) {
            self.used[i] = true;
            let target = self.binding_target(i);
            self.record(span, target);
            return;
        }
        if self.pkg.tables[self.module].get(name).is_some() {
            self.record(
                span,
                RefTarget::Item {
                    module: self.module,
                    name: name.to_string(),
                },
            );
            return;
        }
        if prelude::in_prelude(name) {
            self.record(span, RefTarget::Prelude(name.to_string()));
            return;
        }
        if prelude::is_builtin_type(name) {
            self.record(span, RefTarget::BuiltinType(name.to_string()));
            return;
        }
        if defer_tags && name.chars().next().is_some_and(char::is_uppercase) {
            return; // candidate error-row tag (D30) — s13's call
        }
        // A lowercase miss can still be a tag when the enclosing fn
        // spells it in some error row (#4): `return none` against
        // `-> int ! {none}`. Purely lexical, so plain typos keep
        // their report below.
        if defer_tags && self.row_tags.iter().any(|s| s.contains(name)) {
            return;
        }
        let cands = self.candidates();
        let mut d = Diagnostic::error(
            codes::E0301,
            span,
            format!("nothing named `{name}` is in scope"),
        )
        .with_label("not found");
        let refs: Vec<&str> = cands.iter().map(|s| s.as_str()).collect();
        if let Some(hit) = wolf_diag::suggest::best_match(name, &refs) {
            d = d.with_suggestion(Suggestion::new(
                format!("did you mean `{hit}`?"),
                vec![(span, hit.to_string())],
                Applicability::Maybe,
            ));
        }
        // The teachable case (D59): a sibling file defines the name
        // but opted out of the module — say so instead of leaving the
        // reader hunting for a typo.
        let d = crate::graph::excluded_definer_notes(d, &self.pkg.modules[self.module], name);
        self.sink.push(d);
    }

    /// Resolve the first segment of a `Path` node; segments after a
    /// namespace binding are checked against that namespace.
    fn resolve_path_first(&mut self, path: &GreenNode, defer_tags: bool) {
        let mut segs = path.tokens().filter(|t| t.kind == SyntaxKind::Ident);
        let Some(first) = segs.next() else { return };
        let name = self.text(first.span);
        if let Some(binder) = self.lookup(&name) {
            let target = binder.map_or(RefTarget::SelfKw, RefTarget::Local);
            self.record(first.span, target);
            return;
        }
        if name == "self" {
            return;
        }
        if let Some(i) = self.binding_index(&name) {
            self.used[i] = true;
            let target = self.bindings[i].target.clone();
            let recorded = self.binding_target(i);
            self.record(first.span, recorded);
            if let Some(second) = segs.next() {
                let member = self.text(second.span);
                self.check_namespace_member(&target, &name, &member, second.span);
            }
            return;
        }
        self.resolve_name(&name, first.span, defer_tags);
    }

    /// Check `ns.member` against a namespace binding's item table.
    fn check_namespace_member(
        &mut self,
        target: &BindTarget,
        ns_name: &str,
        member: &str,
        span: Span,
    ) {
        match target {
            BindTarget::CNamespace | BindTarget::Poisoned => {}
            BindTarget::StdModule(mi) => {
                let m = &prelude::STD_MODULES[*mi];
                if m.items.contains(&member) {
                    self.record(span, RefTarget::Std);
                } else {
                    let cands: Vec<String> = m.items.iter().map(|s| s.to_string()).collect();
                    self.sink
                        .push(graph_no_such_item(span, member, ns_name, &cands));
                }
            }
            BindTarget::PkgModule(midx) => match self.pkg.tables[*midx].get(member) {
                None => {
                    let cands: Vec<String> =
                        self.pkg.tables[*midx].names().map(str::to_string).collect();
                    let d = graph_no_such_item(span, member, ns_name, &cands);
                    let d =
                        crate::graph::excluded_definer_notes(d, &self.pkg.modules[*midx], member);
                    self.sink.push(d);
                }
                Some(item) if item.vis == Vis::Private => {
                    // Navigable even though refused: the definition
                    // is exactly what the reader wants to see.
                    self.record(
                        span,
                        RefTarget::Item {
                            module: *midx,
                            name: member.to_string(),
                        },
                    );
                    let mod_name = self.pkg.modules[*midx].display_name();
                    self.sink.push(
                        Diagnostic::error(
                            codes::E0304,
                            span,
                            format!("`{member}` exists in {mod_name}, but it is private"),
                        )
                        .with_label("not visible from here")
                        .with_secondary(item.name_span, "defined here, without `pub`")
                        .with_note(format!(
                            "mark the {} `pub` to export it from the module, or \
                             `pub(pkg)` to share it within this package — `pub(pkg)` \
                             is enough for this use.",
                            item.kind.keyword()
                        )),
                    );
                }
                Some(_) => self.record(
                    span,
                    RefTarget::Item {
                        module: *midx,
                        name: member.to_string(),
                    },
                ),
            },
            // A value import: deeper members are type-dependent (s13).
            BindTarget::Item { .. } | BindTarget::StdItem => {}
        }
    }

    // ---------------------------------------------------- statements ---

    fn resolve_block(&mut self, block: Block<'_>) {
        self.push_scope();
        for stmt in block.statements() {
            self.resolve_stmt(stmt);
        }
        self.pop_scope();
    }

    fn resolve_stmt(&mut self, stmt: &GreenNode) {
        match stmt.kind {
            SyntaxKind::ExprStmt
            | SyntaxKind::AssignStmt
            | SyntaxKind::DeferStmt
            | SyntaxKind::AssumeStmt => self.resolve_child_exprs(stmt),
            SyntaxKind::LetDecl => {
                if let Some(d) = LetDecl::cast(stmt) {
                    // Binders in order (D63): a later initializer may
                    // read an earlier binder's names.
                    for b in d.binders() {
                        if let Some(t) = b.ty {
                            self.resolve_type(t);
                        }
                        if let Some(e) = b.init {
                            self.resolve_expr(e);
                        }
                        if let Some(p) = b.pattern {
                            self.bind_pattern(p);
                        }
                    }
                }
            }
            SyntaxKind::VarDecl => {
                if let Some(d) = VarDecl::cast(stmt) {
                    for b in d.binders() {
                        if let Some(t) = b.ty {
                            self.resolve_type(t);
                        }
                        if let Some(e) = b.init {
                            self.resolve_expr(e);
                        }
                        if let Some(p) = b.pattern {
                            self.bind_pattern(p);
                        }
                    }
                }
            }
            SyntaxKind::ConstDecl => {
                if let Some(d) = ConstDecl::cast(stmt) {
                    if let Some(t) = d.ty() {
                        self.resolve_type(t);
                    }
                    if let Some(e) = d.init() {
                        self.resolve_expr(e);
                    }
                    if let Some(n) = d.name() {
                        let name = self.text(n.span);
                        self.bind(name, Some(n.span));
                    }
                }
            }
            SyntaxKind::FnDecl => {
                if let Some(n) = FnDecl::cast(stmt).and_then(|d| d.name()) {
                    let name = self.text(n.span);
                    self.bind(name, Some(n.span));
                }
                self.resolve_fn(stmt);
            }
            SyntaxKind::StructDecl
            | SyntaxKind::EnumDecl
            | SyntaxKind::TypeDecl
            | SyntaxKind::TraitDecl => {
                if let Some(n) = stmt.child_token(SyntaxKind::Ident) {
                    let name = self.text(n.span);
                    self.bind(name, Some(n.span));
                }
                self.resolve_item(stmt);
            }
            SyntaxKind::ImplDecl => self.resolve_item(stmt),
            _ => {}
        }
    }

    fn resolve_child_exprs(&mut self, node: &GreenNode) {
        for n in node.nodes() {
            if is_expr_kind(n.kind) {
                self.resolve_expr(n);
            }
        }
    }

    // -------------------------------------------------- expressions ----

    fn resolve_expr(&mut self, node: &GreenNode) {
        match node.kind {
            SyntaxKind::PathExpr => {
                if let Some(t) = PathExpr::cast(node).and_then(|p| p.ident()) {
                    let name = self.text(t.span);
                    self.resolve_name(&name, t.span, true);
                }
            }
            SyntaxKind::MemberExpr => self.resolve_member(node),
            SyntaxKind::StructLit => {
                for n in node.nodes() {
                    if n.kind == SyntaxKind::FieldInit {
                        self.resolve_child_exprs(n);
                    } else if is_expr_kind(n.kind) {
                        self.resolve_expr(n);
                    }
                }
            }
            SyntaxKind::CallExpr | SyntaxKind::BracketApply => {
                // D52 ([gram.expr.tagident]): arguments join the
                // raise-position deferral — a bare lowercase miss in
                // argument position whose callee's declared parameter
                // row spells that tag is typing's to resolve.
                let rows = (node.kind == SyntaxKind::CallExpr)
                    .then(|| self.callee_param_rows(node))
                    .flatten();
                for n in node.nodes() {
                    if is_expr_kind(n.kind) {
                        self.resolve_expr(n);
                    } else if let Some(args) = ArgList::cast(n) {
                        self.resolve_args(args, rows.as_deref());
                    }
                }
            }
            SyntaxKind::CastExpr => {
                self.resolve_child_exprs(node);
                if let Some(t) = node.nodes().find(|n| is_type_kind(n.kind)) {
                    self.resolve_type(t);
                }
            }
            SyntaxKind::Block => {
                if let Some(b) = Block::cast(node) {
                    self.resolve_block(b);
                }
            }
            SyntaxKind::MatchExpr => {
                if let Some(m) = MatchExpr::cast(node) {
                    if let Some(s) = m.scrutinee() {
                        self.resolve_expr(s);
                    }
                    for arm in m.arms() {
                        self.resolve_match_arm(arm);
                    }
                }
            }
            SyntaxKind::ForExpr => {
                if let Some(f) = ForExpr::cast(node) {
                    if let Some(it) = f.iterable() {
                        self.resolve_expr(it);
                    }
                    self.push_scope();
                    if let Some(p) = f.pattern() {
                        self.bind_pattern(p);
                    }
                    if let Some(b) = f.body() {
                        self.resolve_block(b);
                    }
                    self.pop_scope();
                }
            }
            SyntaxKind::ElseExpr => {
                if let Some(e) = ElseExpr::cast(node) {
                    if let Some(s) = e.scrutinized() {
                        self.resolve_expr(s);
                    }
                    self.push_scope();
                    if let Some(p) = e.handler_pattern() {
                        self.bind_pattern(p);
                    }
                    if let Some(f) = e.fallback() {
                        self.resolve_expr(f);
                    }
                    self.pop_scope();
                }
            }
            SyntaxKind::ClosureExpr => {
                if let Some(c) = ClosureExpr::cast(node) {
                    if let Some(params) = c.params() {
                        for p in params.params() {
                            if let Some(t) = Param::ty(p) {
                                self.resolve_type(t);
                            }
                        }
                    }
                    self.push_scope();
                    if let Some(params) = c.params() {
                        self.bind_params(params);
                    }
                    if let Some(b) = c.body() {
                        self.resolve_expr(b);
                    }
                    self.pop_scope();
                }
            }
            SyntaxKind::ScopeExpr => {
                if let Some(s) = ScopeExpr::cast(node) {
                    self.push_scope();
                    if let Some(n) = s.name() {
                        let name = self.text(n.span);
                        self.bind(name, Some(n.span));
                    }
                    if let Some(b) = s.body() {
                        self.resolve_block(b);
                    }
                    self.pop_scope();
                }
            }
            SyntaxKind::RegionBlock => {
                if let Some(r) = RegionBlock::cast(node) {
                    self.push_scope();
                    if let Some(n) = r.name() {
                        let name = self.text(n.span);
                        self.bind(name, Some(n.span));
                    }
                    // The strategy (`rc` / `pool(T)`) is a closed
                    // contextual form — nothing to resolve at v0.
                    if let Some(b) = r.body() {
                        self.resolve_block(b);
                    }
                    self.pop_scope();
                }
            }
            SyntaxKind::RegionValue => {}
            SyntaxKind::SelectExpr => {
                for arm in node.nodes().filter_map(SelectArm::cast) {
                    self.push_scope();
                    if let Some(p) = arm.pattern() {
                        self.bind_pattern(p);
                    }
                    self.resolve_child_exprs(arm.syntax());
                    self.pop_scope();
                }
            }
            SyntaxKind::SpawnExpr => {
                if let Some(s) = SpawnExpr::cast(node) {
                    if let Some(p) = s.proc_path() {
                        self.resolve_path_first(p.syntax(), true);
                    }
                    if let Some(args) = s.args() {
                        self.resolve_args(args, None);
                    }
                }
            }
            SyntaxKind::StringExpr => {
                if let Some(s) = StringExpr::cast(node) {
                    for i in s.interps() {
                        // The format spec is skipped: its own nested
                        // interpolations resolve through their exprs,
                        // but spec text is not name-shaped.
                        if let Some(e) = i.expr() {
                            self.resolve_expr(e);
                        }
                    }
                }
            }
            SyntaxKind::AsmExpr => {
                // The template string's `{t}` holes name asm *operands*,
                // not bindings — never resolved. Operand exprs are real.
                if let Some(a) = AsmExpr::cast(node) {
                    for op in a.operands() {
                        if let Some(e) = op.expr() {
                            self.resolve_expr(e);
                        }
                    }
                }
            }
            SyntaxKind::InlineC => {} // opaque C body (c10)
            SyntaxKind::LiteralExpr => {}
            SyntaxKind::MatchGuard => self.resolve_child_exprs(node),
            // Everything else recurses over child expressions (and
            // blocks, which are expressions).
            _ => self.resolve_child_exprs(node),
        }
    }

    fn resolve_args(&mut self, args: ArgList<'_>, rows: Option<&[BTreeSet<String>]>) {
        for (i, a) in args.args().enumerate() {
            if let Some(v) = Arg::value(a) {
                if is_type_kind(v.kind) {
                    self.resolve_type(v);
                    continue;
                }
                // D52 ([gram.expr.tagident]) — declared-row-first, one
                // position wider: a bare lowercase identifier that
                // resolves NOWHERE, sitting in an argument position
                // whose declared parameter row spells that tag, defers
                // to typing (which injects the tag against the
                // expected row exactly as at a raise site). Anything
                // that resolves — a local, an import, a module item —
                // resolves as itself (locals shadow; W0305 prices the
                // collision), and a name no row declares keeps its
                // E0301 below.
                if let Some(rows) = rows
                    && v.kind == SyntaxKind::PathExpr
                    && let Some(t) = PathExpr::cast(v).and_then(|p| p.ident())
                {
                    let name = self.text(t.span);
                    if !name.chars().next().is_some_and(char::is_uppercase)
                        && rows.get(i).is_some_and(|s| s.contains(&name))
                        && !self.in_scope(&name)
                        && name != "self"
                        && self.binding_index(&name).is_none()
                        && self.pkg.tables[self.module].get(&name).is_none()
                        && !prelude::in_prelude(&name)
                        && !prelude::is_builtin_type(&name)
                    {
                        continue; // the tag — s13's call
                    }
                }
                self.resolve_expr(v);
            }
        }
    }

    /// D52's lexical half: for a call through a bare fn name, the tag
    /// names each declared parameter row spells — minus the callee's
    /// generic parameters (a payload-less generic entry is the row's
    /// tail, not a tag) — indexed by parameter position. `None` when
    /// the callee is not a path to a fn declaration resolution can see
    /// (a local fn value, a member call, a builtin): a lowercase miss
    /// there keeps its E0301, and the deferral stays exactly as wide
    /// as the clause — the EXPECTED type has to declare the tag.
    fn callee_param_rows(&self, call: &GreenNode) -> Option<Vec<BTreeSet<String>>> {
        let callee = CallExpr::cast(call)?.callee()?;
        let t = PathExpr::cast(callee)?.ident()?;
        let name = self.text(t.span);
        if self.in_scope(&name) || name == "self" {
            return None; // a local shadows the item — no decl to read
        }
        let (module, item_name) = match self.binding_index(&name) {
            Some(i) => match &self.bindings[i].target {
                BindTarget::Item { module, name } => (*module, name.clone()),
                _ => return None,
            },
            None => (self.module, name),
        };
        let item = self.pkg.tables[module].get(&item_name)?;
        let node = crate::sig::item_node(self.pkg, item);
        let d = FnDecl::cast(node)?;
        let src = &self.pkg.files[item.file].raw.src;
        let text_of = |span: Span| -> String {
            String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned()
        };
        let generics: BTreeSet<String> = d
            .generics()
            .map(|g| {
                g.params()
                    .filter_map(|p| p.name())
                    .map(|t| text_of(t.span))
                    .collect()
            })
            .unwrap_or_default();
        let params = d.params()?;
        Some(
            params
                .params()
                .filter(|p| !p.is_self())
                .map(|p| {
                    let mut tags = BTreeSet::new();
                    if let Some(ty) = Param::ty(p) {
                        collect_bare_row_tags(ty, src, &generics, &mut tags);
                    }
                    tags
                })
                .collect(),
        )
    }

    fn resolve_match_arm(&mut self, arm: MatchArm<'_>) {
        self.push_scope();
        if let Some(p) = arm.pattern() {
            self.bind_pattern(p);
        }
        if let Some(g) = arm.guard() {
            self.resolve_child_exprs(g);
        }
        if let Some(b) = arm.body() {
            self.resolve_expr(b);
        }
        self.pop_scope();
    }

    fn resolve_member(&mut self, node: &GreenNode) {
        let Some(m) = MemberExpr::cast(node) else {
            return;
        };
        let Some(base) = m.base() else { return };
        if base.kind == SyntaxKind::PathExpr
            && let Some(t) = PathExpr::cast(base).and_then(|p| p.ident())
        {
            let name = self.text(t.span);
            if let Some(binder) = self.lookup(&name) {
                let target = binder.map_or(RefTarget::SelfKw, RefTarget::Local);
                self.record(t.span, target);
                return;
            }
            if name != "self" {
                if let Some(i) = self.binding_index(&name) {
                    self.used[i] = true;
                    let target = self.bindings[i].target.clone();
                    let recorded = self.binding_target(i);
                    self.record(t.span, recorded);
                    if let Some(mem) = m.member().filter(|t| t.kind == SyntaxKind::Ident) {
                        let member = self.text(mem.span);
                        self.check_namespace_member(&target, &name, &member, mem.span);
                    }
                    return;
                }
                self.resolve_name(&name, t.span, true);
            }
            return;
        }
        self.resolve_expr(base);
        // The member itself is type-dependent — s13's job.
    }

    // ------------------------------------------------------- types -----

    fn resolve_type(&mut self, node: &GreenNode) {
        match node.kind {
            SyntaxKind::PathType => {
                if let Some(t) = PathType::cast(node) {
                    if let Some(p) = t.path() {
                        self.resolve_path_first(p.syntax(), false);
                    }
                    if let Some(args) = t.args() {
                        for a in args.args() {
                            if is_type_kind(a.kind) {
                                self.resolve_type(a);
                            }
                            // TypeArgPending (expression-shaped const
                            // generics) opens in s13 (D29).
                        }
                    }
                }
            }
            SyntaxKind::DynType => {
                if let Some(p) = node.child_node(SyntaxKind::Path) {
                    self.resolve_path_first(p, false);
                }
            }
            SyntaxKind::ErrorUnionType
            | SyntaxKind::PrefixType
            | SyntaxKind::PtrType
            | SyntaxKind::TupleType => {
                for n in node.nodes().filter(|n| is_type_kind(n.kind)) {
                    self.resolve_type(n);
                }
            }
            SyntaxKind::FnType => {
                for n in node.nodes() {
                    if is_type_kind(n.kind) {
                        self.resolve_type(n);
                    } else if let Some(r) = RetType::cast(n) {
                        self.resolve_ret(r);
                    }
                }
            }
            // `type`, `region`: keywords, nothing to resolve.
            SyntaxKind::TypeType | SyntaxKind::RegionType => {}
            _ => {}
        }
    }

    fn resolve_ret(&mut self, ret: RetType<'_>) {
        if let Some(t) = ret.ty() {
            self.resolve_type(t);
        }
        // The explicit error row is structural (D30): its tags are
        // declared by use, so the row resolves nothing here.
    }

    // ---------------------------------------------- unused imports -----

    fn report_unused_imports(&mut self, root: &GreenNode) {
        // Group members share a decl; when the whole declaration is
        // dead the fix removes the line, otherwise just the name.
        for (i, b) in self.bindings.iter().enumerate() {
            if self.used[i] || b.unused_exempt {
                continue;
            }
            let decl_mates: Vec<usize> = self
                .bindings
                .iter()
                .enumerate()
                .filter(|(_, o)| o.decl_span == b.decl_span)
                .map(|(j, _)| j)
                .collect();
            let whole_decl_dead = decl_mates
                .iter()
                .all(|&j| !self.used[j] && !self.bindings[j].unused_exempt);
            let solo = decl_mates.len() == 1;
            let edits = if solo || whole_decl_dead {
                if decl_mates.first() != Some(&i) {
                    continue; // one report per dead declaration
                }
                vec![(b.decl_span, String::new())]
            } else {
                group_removal_edits(root, b.name_span)
            };
            let d = Diagnostic::error(
                codes::E0305,
                b.name_span,
                format!("the import `{}` is never used", b.name),
            )
            .with_label("imported here, used nowhere in this file")
            .with_note(
                "imports are file-scoped (D32): a name used only in a sibling file \
                 must be imported there instead.",
            )
            .with_suggestion(Suggestion::new(
                "delete the unused import",
                edits,
                Applicability::MachineApplicable,
            ));
            self.sink.push(d);
        }
    }
}

/// The byte edit that removes one name (and its separator comma) from a
/// `use path.{…}` group.
fn group_removal_edits(root: &GreenNode, name_span: Span) -> Vec<(Span, String)> {
    fn find_group(node: &GreenNode, span: Span) -> Option<&GreenNode> {
        for n in node.nodes() {
            if n.kind == SyntaxKind::UseGroup && n.span.lo <= span.lo && span.hi <= n.span.hi {
                return Some(n);
            }
            if let Some(hit) = find_group(n, span) {
                return Some(hit);
            }
        }
        None
    }
    let Some(group) = find_group(root, name_span) else {
        return vec![(name_span, String::new())];
    };
    let tokens: Vec<_> = group.tokens().collect();
    let pos = tokens.iter().position(|t| t.span == name_span);
    let Some(pos) = pos else {
        return vec![(name_span, String::new())];
    };
    // Prefer swallowing the comma after the name; fall back to the one
    // before (last member); else just the name.
    if let Some(next) = tokens.get(pos + 1).filter(|t| t.kind == SyntaxKind::Comma) {
        return vec![(name_span.join(next.span), String::new())];
    }
    if pos > 0 && tokens[pos - 1].kind == SyntaxKind::Comma {
        return vec![(tokens[pos - 1].span.join(name_span), String::new())];
    }
    vec![(name_span, String::new())]
}

/// Shared with pass A's wording (one voice for "no such item").
fn graph_no_such_item(span: Span, name: &str, module: &str, cands: &[String]) -> Diagnostic {
    let refs: Vec<&str> = cands.iter().map(|s| s.as_str()).collect();
    let mut d = Diagnostic::error(
        codes::E0301,
        span,
        format!("module `{module}` has no item named `{name}`"),
    )
    .with_label("not found in the module");
    if let Some(hit) = wolf_diag::suggest::best_match(name, &refs) {
        d = d.with_suggestion(Suggestion::new(
            format!("did you mean `{hit}`?"),
            vec![(span, hit.to_string())],
            Applicability::Maybe,
        ));
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::MemoryLoader;

    fn resolve(files: &[(&[&str], &str, &str)]) -> Resolution {
        let mut ml = MemoryLoader::new("t");
        for (m, n, s) in files {
            ml.add_file(m, n, s);
        }
        resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads")
    }

    fn codes_of(r: &Resolution) -> Vec<&str> {
        r.diagnostics.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn std_root_keeps_prelude_ambient_and_checks_real_members() {
        // With a real std tree configured the prelude stays ambient
        // (D31) and `std` member checks answer from the real item
        // table: `fs.read_text` resolves, `fs.gone` is E0301.
        let mut ml = MemoryLoader::new("t");
        ml.add_file(
            &[],
            "main.lu",
            "use std.fs\nfn main() -> !int {\n    print(\"hi\")\n    \
             fs.read_text(\"x\")\n    fs.gone()\n    0\n}\n",
        );
        ml.add_std_file(&["fs"], "fs.lu", "pub fn read_text(p: str) -> str { p }\n");
        let r = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
        assert_eq!(codes_of(&r), ["E0301"], "{:?}", r.diagnostics);
        assert!(
            r.diagnostics[0].message.contains("gone"),
            "only the missing member errors: {}",
            r.diagnostics[0].message
        );
    }

    #[test]
    fn clean_single_file_resolves() {
        let r = resolve(&[(
            &[],
            "main.lu",
            "fn main() -> !int {\n    let who = \"wolf\"\n    print(\"hi {who}\")\n    0\n}\n",
        )]);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn typo_gets_a_suggestion() {
        let r = resolve(&[(
            &[],
            "main.lu",
            "fn main() -> !int {\n    pirnt(\"hi\")\n    0\n}\n",
        )]);
        assert_eq!(codes_of(&r), ["E0301"]);
        let d = &r.diagnostics[0];
        assert!(d.suggestions[0].message.contains("print"), "{d:?}");
    }

    #[test]
    fn capitalized_miss_defers_as_row_tag() {
        let r = resolve(&[(
            &[],
            "main.lu",
            "fn may(flag: bool) -> !int {\n    if flag { return Failed }\n    7\n}\n\
             fn main() -> !int { may(false) else 0\n0 }\n",
        )]);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn lowercase_miss_defers_only_when_a_row_spells_it() {
        // A lowercase miss spelled in the enclosing fn's error row is
        // a candidate tag (#4) — deferred to typing.
        let r = resolve(&[(
            &[],
            "main.lu",
            "fn f(flag: bool) -> int ! {none} {\n    if flag { return none }\n    7\n}\n\
             fn main() -> !int { f(true) else 0 }\n",
        )]);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        // A plain lowercase miss keeps its E0301.
        let r = resolve(&[(
            &[],
            "main.lu",
            "fn main() -> !int {\n    let x = nowhere\n    x\n}\n",
        )]);
        assert_eq!(codes_of(&r), ["E0301"]);
    }

    #[test]
    fn arg_position_defers_when_callee_row_spells_the_tag() {
        // D52 ([gram.expr.tagident]): the callee's declared parameter
        // row spells `none`, so the bare argument defers to typing.
        let r = resolve(&[(
            &[],
            "main.lu",
            "fn or(v: int ! {none}, d: int) -> int {\n    v else d\n}\n\
             fn main() -> !int { if or(none, 9) == 9 { 0 } else { 1 } }\n",
        )]);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        // The negative: a name the declared row does NOT spell keeps
        // its E0301 — the deferral is exactly as wide as the row.
        let r = resolve(&[(
            &[],
            "main.lu",
            "fn or(v: int ! {none}, d: int) -> int {\n    v else d\n}\n\
             fn main() -> !int { if or(gone, 9) == 9 { 0 } else { 1 } }\n",
        )]);
        assert_eq!(codes_of(&r), ["E0301"]);
        // Positions do not smear: the tag name in the WRONG argument
        // slot (a plain-int parameter) keeps its E0301 too.
        let r = resolve(&[(
            &[],
            "main.lu",
            "fn or(v: int ! {none}, d: int) -> int {\n    v else d\n}\n\
             fn main() -> !int { if or(4, none) == 4 { 0 } else { 1 } }\n",
        )]);
        assert_eq!(codes_of(&r), ["E0301"]);
    }

    #[test]
    fn arg_position_defers_through_an_import() {
        // The callee's decl lives in another module; the binding walks
        // there and reads the same declared row.
        let r = resolve(&[
            (
                &["util"],
                "util.lu",
                "/// Defaulting over `none`.\npub fn or(v: int ! {none}, d: int) -> int {\n    v else d\n}\n\
                 /// A second export (module-shape lint quiet).\npub fn id(v: int) -> int {\n    v\n}\n",
            ),
            (
                &[],
                "main.lu",
                "use util.{or}\n\nfn main() -> !int { if or(none, 9) == 9 { 0 } else { 1 } }\n",
            ),
        ]);
        let errors: Vec<&str> = r
            .diagnostics
            .iter()
            .filter(|d| d.severity == wolf_diag::Severity::Error)
            .map(|d| d.code.as_str())
            .collect();
        assert!(errors.is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn forward_references_across_files_resolve() {
        let r = resolve(&[
            (&[], "a.lu", "fn main() -> !int { later() }\n"),
            (&[], "b.lu", "fn later() -> !int { 0 }\n"),
        ]);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn unused_import_is_machine_fixable() {
        let r = resolve(&[
            (&[], "main.lu", "use util\nfn main() -> !int { 0 }\n"),
            (
                &["util"],
                "u.lu",
                "/// One.\npub fn helper() -> int { 1 }\n/// Two.\npub fn also() -> int { 2 }\n",
            ),
        ]);
        assert_eq!(codes_of(&r), ["E0305"]);
        let s = &r.diagnostics[0].suggestions[0];
        assert_eq!(s.applicability, Applicability::MachineApplicable);
        assert_eq!(s.edits.len(), 1);
        assert!(s.edits[0].1.is_empty(), "the edit deletes the line");
    }

    #[test]
    fn private_member_access_is_e0304() {
        let r = resolve(&[
            (
                &[],
                "main.lu",
                "use vault\nfn main() -> !int { vault.secret() }\n",
            ),
            (
                &["vault"],
                "v.lu",
                "fn secret() -> int { 41 }\n/// The door.\npub fn open() -> int { secret() }\n",
            ),
        ]);
        assert_eq!(codes_of(&r), ["E0304"]);
        assert!(r.diagnostics[0].message.contains("private"));
    }

    #[test]
    fn pkg_visibility_is_enough_in_package() {
        let r = resolve(&[
            (
                &[],
                "main.lu",
                "use store\nfn main() -> !int { store.limit() }\n",
            ),
            (
                &["store"],
                "s.lu",
                "pub(pkg) fn limit() -> int { 8 }\n/// Two.\npub fn cap() -> int { 9 }\n",
            ),
        ]);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn suppression_keeps_wrecked_regions_quiet() {
        // The statement is wrecked by a parse error and recovery skips
        // `== zzz_undefined` into an error region; resolution walks the
        // rest of the body normally (`0` still resolves) and computes
        // nothing inside the wreck — its sink is seeded with the
        // parser's error regions, so even a forced echo would drop.
        let src = "fn main() -> !int {\n    let == zzz_undefined\n    print(\"ok\")\n    0\n}\n";
        let r = resolve(&[(&[], "main.lu", src)]);
        assert!(
            !r.package.files[0].parse.error_regions.is_empty(),
            "the wreck produced a suppression region"
        );
        assert!(
            r.diagnostics
                .iter()
                .all(|d| d.code.as_str().starts_with("E02")),
            "only the parse report survives: {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn single_thread_env_matches_parallel() {
        let files: &[(&[&str], &str, &str)] = &[
            (
                &[],
                "main.lu",
                "use util\nfn main() -> !int { util.helper() }\n",
            ),
            (
                &["util"],
                "u.lu",
                "/// One.\npub fn helper() -> int { 1 }\n/// Two.\npub fn also() -> int { 2 }\n",
            ),
        ];
        let a = resolve(files);
        let mut ml = MemoryLoader::new("t");
        for (m, n, s) in files {
            ml.add_file(m, n, s);
        }
        let b = resolve_package_with(&mut ml, &AliasTable::default(), false).expect("loads");
        assert_eq!(a.diagnostics, b.diagnostics);
    }
}
