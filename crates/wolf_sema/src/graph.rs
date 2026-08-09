//! The module graph (s12, D32): directory = module.
//!
//! Every `.lu` file in a directory belongs to one module; file
//! boundaries are invisible to importers, and the module's path is its
//! directory path from the package root. Loading is *lazy and
//! import-driven*: the root module loads first, and further modules load
//! only when a `use` names them — so unrelated sibling directories cost
//! nothing and cannot poison a compilation.
//!
//! This pass (pass A of s12's two-pass design) parses each loaded file,
//! collects the per-module item table (duplicate definitions report
//! [`wolf_diag::codes::E0302`] here, with both spans), resolves every
//! file's import prefix to concrete bindings, records the dependency
//! edges, and rejects cycles ([`wolf_diag::codes::E0303`]) with the full
//! cycle path. Bodies are pass B's job ([`crate::resolve`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use wolf_ast::{GreenNode, SyntaxKind};
use wolf_ast::{ImportCDecl, UseDecl, is_pattern_kind};
use wolf_diag::{Diagnostic, codes};
use wolf_span::{FileId, SourceMap, Span};

use crate::prelude;

// ---------------------------------------------------------------- input --

/// One source file as handed to sema by a [`ModuleLoader`].
#[derive(Debug)]
pub struct RawFile {
    pub file: FileId,
    /// Display name for diagnostics — relative, never absolute (the
    /// interface format forbids absolute paths, D7 determinism).
    pub display: String,
    pub src: Vec<u8>,
}

/// Where modules come from. The compiler's disk layout ([`DiskLoader`])
/// and the in-memory test/LSP form ([`MemoryLoader`]) both implement
/// this, so resolution and interface emission never touch IO themselves.
pub trait ModuleLoader {
    /// Package identity. A stub until the package manager (s51) gives
    /// packages real names.
    fn package_name(&self) -> String {
        "pkg".to_string()
    }

    /// The files of the module at directory path `path` (`[]` is the
    /// package root), sorted by file name, or `None` if no such module
    /// exists. Called at most once per path.
    fn load_module(&mut self, path: &[String]) -> Option<Vec<RawFile>>;

    /// Candidate first-segment module names under the root — suggestion
    /// fodder for unresolved imports, nothing more.
    fn root_module_names(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// A file-participation predicate for [`DiskLoader`]: given a file's
/// bytes, does it join the compilation? The conform harness admits only
/// files whose header carries `member: true`.
pub type FileFilter<'a> = Box<dyn Fn(&[u8]) -> bool + 'a>;

/// Disk-backed loader. `from_entry` implements conform-run-style
/// single-entry compilation: the package root is the entry file's
/// directory, the entry's own directory is the root module, and sibling
/// subdirectories are child modules. The `filter` decides which
/// non-entry files participate (the conform harness admits only files
/// marked `member: true`); `from_dir` takes every `.lu` file.
pub struct DiskLoader<'a> {
    root: PathBuf,
    entry: Option<PathBuf>,
    sm: &'a mut SourceMap,
    filter: FileFilter<'a>,
}

impl<'a> DiskLoader<'a> {
    /// Entry-file mode. Returns `None` when `entry` has no parent
    /// directory or is not a file.
    pub fn from_entry(entry: &Path, sm: &'a mut SourceMap, filter: FileFilter<'a>) -> Option<Self> {
        if !entry.is_file() {
            return None;
        }
        let root = entry.parent()?.to_path_buf();
        Some(DiskLoader {
            root,
            entry: Some(entry.to_path_buf()),
            sm,
            filter,
        })
    }

    /// Whole-directory mode (`wolf interface <dir>`): every `.lu` file
    /// participates.
    pub fn from_dir(dir: &Path, sm: &'a mut SourceMap) -> Self {
        DiskLoader {
            root: dir.to_path_buf(),
            entry: None,
            sm,
            filter: Box::new(|_| true),
        }
    }
}

impl ModuleLoader for DiskLoader<'_> {
    fn load_module(&mut self, path: &[String]) -> Option<Vec<RawFile>> {
        let mut dir = self.root.clone();
        for seg in path {
            dir.push(seg);
        }
        let entries = std::fs::read_dir(&dir).ok()?;
        let mut names: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "lu"))
            .collect();
        names.sort();
        let mut out = Vec::new();
        for p in names {
            let src = std::fs::read(&p).ok()?;
            let is_entry = self.entry.as_deref() == Some(p.as_path());
            if !is_entry && !(self.filter)(&src) {
                continue;
            }
            let file = self.sm.intern(&p);
            out.push(RawFile {
                file,
                display: p.display().to_string().replace('\\', "/"),
                src,
            });
        }
        if out.is_empty() { None } else { Some(out) }
    }

    fn root_module_names(&mut self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort();
        names
    }
}

/// In-memory loader for tests (and, later, the resident compiler): a
/// map from module path to named sources. Owns its [`SourceMap`].
#[derive(Default)]
pub struct MemoryLoader {
    package: String,
    modules: BTreeMap<Vec<String>, Vec<(String, Vec<u8>)>>,
    sm: SourceMap,
}

impl MemoryLoader {
    pub fn new(package: &str) -> Self {
        MemoryLoader {
            package: package.to_string(),
            ..Default::default()
        }
    }

    /// Add one file to the module at `module` (`&[]` is the root).
    /// Files resolve in name order regardless of insertion order.
    pub fn add_file(&mut self, module: &[&str], name: &str, src: &str) {
        self.modules
            .entry(module.iter().map(|s| s.to_string()).collect())
            .or_default()
            .push((name.to_string(), src.as_bytes().to_vec()));
    }
}

impl ModuleLoader for MemoryLoader {
    fn package_name(&self) -> String {
        self.package.clone()
    }

    fn load_module(&mut self, path: &[String]) -> Option<Vec<RawFile>> {
        let mut files = self.modules.get(path)?.clone();
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let mut out = Vec::new();
        for (name, src) in files {
            let virt = format!("mem://{}/{name}", path.join("/"));
            let file = self.sm.intern(Path::new(&virt));
            out.push(RawFile {
                file,
                display: virt,
                src,
            });
        }
        if out.is_empty() { None } else { Some(out) }
    }

    fn root_module_names(&mut self) -> Vec<String> {
        self.modules
            .keys()
            .filter_map(|k| k.first().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

/// Package-level aliasing (s51's slot): first import segment → module
/// path it stands for. Empty until the package manager exists; it is an
/// input parameter so s51 slots in without touching resolution.
#[derive(Default, Debug, Clone)]
pub struct AliasTable {
    map: BTreeMap<String, Vec<String>>,
}

impl AliasTable {
    pub fn insert(&mut self, alias: &str, path: &[&str]) {
        self.map.insert(
            alias.to_string(),
            path.iter().map(|s| s.to_string()).collect(),
        );
    }

    pub fn lookup(&self, first: &str) -> Option<&[String]> {
        self.map.get(first).map(|v| v.as_slice())
    }

    fn names(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(|s| s.as_str())
    }
}

// ------------------------------------------------------------- items ----

/// Item kinds carried by the module table and the interface format.
/// The declaration order of the enum is the canonical sort order
/// (`wolfi` items sort by `(kind, name)`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum ItemKind {
    Fn,
    Struct,
    Enum,
    Type,
    Trait,
    Const,
    Let,
    Var,
}

impl ItemKind {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(v: u8) -> Option<ItemKind> {
        use ItemKind::*;
        [Fn, Struct, Enum, Type, Trait, Const, Let, Var]
            .into_iter()
            .find(|k| k.as_u8() == v)
    }

    pub fn keyword(self) -> &'static str {
        match self {
            ItemKind::Fn => "fn",
            ItemKind::Struct => "struct",
            ItemKind::Enum => "enum",
            ItemKind::Type => "type",
            ItemKind::Trait => "trait",
            ItemKind::Const => "const",
            ItemKind::Let => "let",
            ItemKind::Var => "var",
        }
    }
}

/// Item visibility (D32): private is the default, `pub` exports from
/// the module, `pub(pkg)` shares within the package.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Vis {
    Private,
    Pkg,
    Pub,
}

impl Vis {
    pub fn as_u8(self) -> u8 {
        match self {
            Vis::Private => 0,
            Vis::Pkg => 1,
            Vis::Pub => 2,
        }
    }

    pub fn from_u8(v: u8) -> Option<Vis> {
        [Vis::Private, Vis::Pkg, Vis::Pub]
            .into_iter()
            .find(|x| x.as_u8() == v)
    }

    pub fn render(self) -> &'static str {
        match self {
            Vis::Private => "",
            Vis::Pkg => "pub(pkg) ",
            Vis::Pub => "pub ",
        }
    }
}

/// One top-level item in a module's table.
#[derive(Debug, Clone)]
pub struct Item {
    pub name: String,
    pub kind: ItemKind,
    pub vis: Vis,
    /// The defining name token's span (diagnostics only — never
    /// serialized into interfaces).
    pub name_span: Span,
    /// Index into [`Package::files`].
    pub file: usize,
    /// Index of the declaration among the file root's item nodes.
    pub decl: usize,
}

/// A module's merged item table (all files, one namespace).
#[derive(Debug, Default)]
pub struct ItemTable {
    pub items: Vec<Item>,
    index: BTreeMap<String, usize>,
}

impl ItemTable {
    pub fn get(&self, name: &str) -> Option<&Item> {
        self.index.get(name).map(|&i| &self.items[i])
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.index.keys().map(|s| s.as_str())
    }
}

// ---------------------------------------------------------- bindings ----

/// What a file-scoped import binding points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindTarget {
    /// A package module (index into [`Package::modules`]).
    PkgModule(usize),
    /// A stub std module (index into [`prelude::STD_MODULES`]).
    StdModule(usize),
    /// An item imported from a package module.
    Item { module: usize, name: String },
    /// An item imported from the std stub.
    StdItem,
    /// The opaque `c` namespace of `import c "hdr"` (contents deferred
    /// to the real importer, c10).
    CNamespace,
    /// An import that failed to resolve (already reported) — uses of
    /// the name stay silent instead of cascading.
    Poisoned,
}

/// One file-scoped import binding.
#[derive(Debug, Clone)]
pub struct Binding {
    pub name: String,
    pub target: BindTarget,
    /// The whole `use` declaration (the E0305 delete-the-line span).
    pub decl_span: Span,
    /// The bound name's own token span.
    pub name_span: Span,
    /// The byte edit that removes just this binding (for group imports
    /// this deletes the name from the group, not the line).
    pub removal: Vec<(Span, String)>,
    /// Exempt from unused-import enforcement (`import c`, poisoned).
    pub unused_exempt: bool,
}

// ------------------------------------------------------------ package ----

/// One parsed source file of the package.
#[derive(Debug)]
pub struct SourceUnit {
    pub raw: RawFile,
    pub parse: wolf_parse::Parse,
}

/// One loaded module.
#[derive(Debug)]
pub struct ModuleData {
    /// Directory path from the package root (`[]` = root module).
    pub path: Vec<String>,
    /// Indices into [`Package::files`].
    pub files: Vec<usize>,
    /// Direct dependency edges (cycle-closing edges excluded).
    pub deps: BTreeSet<usize>,
    /// Per file (parallel to `files`): the import bindings, in
    /// declaration order.
    pub bindings: Vec<Vec<Binding>>,
}

impl ModuleData {
    /// Human name: dotted path, or "the package root".
    pub fn display_name(&self) -> String {
        if self.path.is_empty() {
            "the package root".to_string()
        } else {
            format!("`{}`", self.path.join("."))
        }
    }

    /// Dotted path for machine surfaces (interfaces); root is "".
    pub fn dotted(&self) -> String {
        self.path.join(".")
    }
}

/// The loaded package: files, modules, item tables, load-time
/// diagnostics, and a deterministic dependency-first order.
#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub files: Vec<SourceUnit>,
    pub modules: Vec<ModuleData>,
    pub tables: Vec<ItemTable>,
    /// Module indices, dependencies before dependents; the root module
    /// (index 0) comes last.
    pub topo: Vec<usize>,
    /// Pass-A diagnostics: duplicate definitions, import-path errors,
    /// cycles, import collisions.
    pub diagnostics: Vec<Diagnostic>,
}

/// Load a package from its root module outward (pass A). Errors with a
/// human-readable string only when the root itself cannot be loaded.
pub fn load_package(
    loader: &mut dyn ModuleLoader,
    aliases: &AliasTable,
) -> Result<Package, String> {
    let mut st = LoadState {
        loader,
        aliases,
        files: Vec::new(),
        modules: Vec::new(),
        tables: Vec::new(),
        index: BTreeMap::new(),
        loading: Vec::new(),
        chain: Vec::new(),
        diags: Vec::new(),
    };
    if st.load(&[]).is_none() {
        return Err("the package root has no wolf source files".to_string());
    }
    // Dependency-first deterministic order (deps ascend by index).
    let mut topo = Vec::new();
    let mut seen = vec![false; st.modules.len()];
    fn post(i: usize, modules: &[ModuleData], seen: &mut [bool], out: &mut Vec<usize>) {
        if seen[i] {
            return;
        }
        seen[i] = true;
        for &d in &modules[i].deps {
            post(d, modules, seen, out);
        }
        out.push(i);
    }
    post(0, &st.modules, &mut seen, &mut topo);
    for i in 0..st.modules.len() {
        post(i, &st.modules, &mut seen, &mut topo);
    }
    Ok(Package {
        name: st.loader.package_name(),
        files: st.files,
        modules: st.modules,
        tables: st.tables,
        topo,
        diagnostics: st.diags,
    })
}

// ----------------------------------------------------------- internals --

/// Owned shape of one `use`/`import c` declaration (extracted from the
/// tree so loading can recurse without holding borrows).
struct UseData {
    segments: Vec<(String, Span)>,
    group: Option<Vec<(String, Span)>>,
    alias: Option<(String, Span)>,
    decl_span: Span,
    import_c: bool,
}

struct ChainEdge {
    module: usize,
    span: Span,
}

struct LoadState<'a> {
    loader: &'a mut dyn ModuleLoader,
    aliases: &'a AliasTable,
    files: Vec<SourceUnit>,
    modules: Vec<ModuleData>,
    tables: Vec<ItemTable>,
    index: BTreeMap<Vec<String>, usize>,
    loading: Vec<bool>,
    chain: Vec<ChainEdge>,
    diags: Vec<Diagnostic>,
}

impl LoadState<'_> {
    /// Load (or find) the module at `path`. `None` = no such module.
    fn load(&mut self, path: &[String]) -> Option<usize> {
        if let Some(&i) = self.index.get(path) {
            return Some(i);
        }
        let raw = self.loader.load_module(path)?;
        let idx = self.modules.len();
        self.index.insert(path.to_vec(), idx);
        self.modules.push(ModuleData {
            path: path.to_vec(),
            files: Vec::new(),
            deps: BTreeSet::new(),
            bindings: Vec::new(),
        });
        self.tables.push(ItemTable::default());
        self.loading.push(true);
        // Parse every file of the module.
        let mut file_indices = Vec::new();
        for r in raw {
            let parse = wolf_parse::parse_file(r.file, &r.src);
            file_indices.push(self.files.len());
            self.files.push(SourceUnit { raw: r, parse });
        }
        self.modules[idx].files = file_indices.clone();
        // Merged item table, duplicate definitions reported here.
        let mut table = ItemTable::default();
        for &fi in &file_indices {
            let unit = &self.files[fi];
            for (name, kind, vis, span, decl) in collect_items(&unit.parse.root, &unit.raw.src) {
                match table.index.get(&name) {
                    Some(&first) => {
                        let prev = &table.items[first];
                        self.diags.push(
                            Diagnostic::error(
                                codes::E0302,
                                span,
                                format!("the name `{name}` is defined twice in this module"),
                            )
                            .with_label("defined again here")
                            .with_secondary(prev.name_span, "first defined here")
                            .with_note(
                                "every `.lu` file in a directory is one module (D32), so a \
                                 top-level name may be defined only once across all of them; \
                                 rename or remove one of the definitions.",
                            ),
                        );
                    }
                    None => {
                        table.index.insert(name.clone(), table.items.len());
                        table.items.push(Item {
                            name,
                            kind,
                            vis,
                            name_span: span,
                            file: fi,
                            decl,
                        });
                    }
                }
            }
        }
        self.tables[idx] = table;
        // Resolve the import prefix of each file (may recurse).
        for (slot, &fi) in file_indices.iter().enumerate() {
            let uses = extract_uses(&self.files[fi].parse.root, &self.files[fi].raw.src);
            let mut file_bindings: Vec<Binding> = Vec::new();
            for u in uses {
                for b in self.resolve_use(idx, &u) {
                    self.merge_binding(idx, &mut file_bindings, b);
                }
            }
            debug_assert!(slot == self.modules[idx].bindings.len());
            self.modules[idx].bindings.push(file_bindings);
        }
        self.loading[idx] = false;
        Some(idx)
    }

    /// Add `b` to a file's binding list, reporting E0306 collisions
    /// (first binding wins; `import c` merges with itself silently).
    fn merge_binding(&mut self, module: usize, list: &mut Vec<Binding>, b: Binding) {
        if let Some(prev) = list.iter().find(|p| p.name == b.name) {
            if prev.target == BindTarget::CNamespace && b.target == BindTarget::CNamespace {
                return; // many `import c` headers, one namespace — normal
            }
            self.diags.push(
                Diagnostic::error(
                    codes::E0306,
                    b.name_span,
                    format!("`{}` is already bound by an earlier import", b.name),
                )
                .with_label("imported again here")
                .with_secondary(prev.name_span, "first bound here")
                .with_note(
                    "drop the repeated import, or give this one its own name with \
                     `use path as other_name`.",
                ),
            );
            return;
        }
        if let Some(item) = self.tables[module].get(&b.name) {
            self.diags.push(
                Diagnostic::error(
                    codes::E0306,
                    b.name_span,
                    format!(
                        "the imported name `{}` collides with a definition in this module",
                        b.name
                    ),
                )
                .with_label("imported here")
                .with_secondary(item.name_span, "already defined here")
                .with_note(
                    "rename the import with `use path as other_name`, or rename the \
                     module-level definition.",
                ),
            );
            return;
        }
        list.push(b);
    }

    /// Resolve one `use`/`import c` declaration into bindings.
    fn resolve_use(&mut self, importer: usize, u: &UseData) -> Vec<Binding> {
        if u.import_c {
            // `import c "hdr"` binds the contextual namespace `c`
            // opaquely; header contents are c10's business.
            return vec![Binding {
                name: "c".to_string(),
                target: BindTarget::CNamespace,
                decl_span: u.decl_span,
                name_span: u.decl_span,
                removal: vec![(u.decl_span, String::new())],
                unused_exempt: true,
            }];
        }
        if u.segments.is_empty() {
            return Vec::new(); // parse recovery already reported it
        }
        // Alias table first (s51's slot): remap the first segment.
        let mut segments = u.segments.clone();
        if let Some(mapped) = self.aliases.lookup(&segments[0].0) {
            let span = segments[0].1;
            let mut remapped: Vec<(String, Span)> =
                mapped.iter().map(|s| (s.clone(), span)).collect();
            remapped.extend(segments.into_iter().skip(1));
            segments = remapped;
        }
        if segments[0].0 == "std" {
            return self.resolve_std_use(u, &segments);
        }
        self.resolve_pkg_use(importer, u, &segments)
    }

    fn resolve_std_use(&mut self, u: &UseData, segments: &[(String, Span)]) -> Vec<Binding> {
        let texts: Vec<&str> = segments.iter().map(|(s, _)| s.as_str()).collect();
        let path_span = segments
            .first()
            .map(|(_, s)| segments.iter().fold(*s, |a, (_, b)| a.join(*b)))
            .expect("nonempty");
        // Longest std-module prefix.
        let (mi, k) = match (1..=texts.len())
            .rev()
            .find_map(|k| prelude::std_module(&texts[..k]).map(|m| (m, k)))
        {
            Some(hit) => hit,
            None => {
                self.diags.push(unresolved_import(
                    path_span,
                    &texts.join("."),
                    &std_suggestions(),
                ));
                return vec![poisoned(u, segments)];
            }
        };
        let rest = &segments[k..];
        if let Some(group) = &u.group {
            if !rest.is_empty() {
                self.diags.push(unresolved_import(
                    path_span,
                    &texts.join("."),
                    &std_suggestions(),
                ));
                return vec![poisoned(u, segments)];
            }
            return group
                .iter()
                .map(|(gname, gspan)| {
                    let mut full: Vec<&str> = texts[..k].to_vec();
                    full.push(gname);
                    let target = if let Some(sub) = prelude::std_module(&full) {
                        BindTarget::StdModule(sub)
                    } else if prelude::STD_MODULES[mi].items.contains(&gname.as_str()) {
                        BindTarget::StdItem
                    } else {
                        let items: Vec<String> = prelude::STD_MODULES[mi]
                            .items
                            .iter()
                            .map(|s| s.to_string())
                            .collect();
                        self.diags
                            .push(no_such_item(*gspan, gname, &texts[..k].join("."), &items));
                        BindTarget::Poisoned
                    };
                    group_binding(u, gname, *gspan, target)
                })
                .collect();
        }
        match rest {
            [] => vec![whole_binding(u, segments, BindTarget::StdModule(mi))],
            [(iname, ispan)] => {
                if prelude::STD_MODULES[mi].items.contains(&iname.as_str()) {
                    vec![whole_binding(u, segments, BindTarget::StdItem)]
                } else {
                    let items: Vec<String> = prelude::STD_MODULES[mi]
                        .items
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    self.diags
                        .push(no_such_item(*ispan, iname, &texts[..k].join("."), &items));
                    vec![poisoned(u, segments)]
                }
            }
            _ => {
                self.diags.push(unresolved_import(
                    path_span,
                    &texts.join("."),
                    &std_suggestions(),
                ));
                vec![poisoned(u, segments)]
            }
        }
    }

    fn resolve_pkg_use(
        &mut self,
        importer: usize,
        u: &UseData,
        segments: &[(String, Span)],
    ) -> Vec<Binding> {
        let texts: Vec<String> = segments.iter().map(|(s, _)| s.clone()).collect();
        // Longest module-path prefix, probing the loader lazily. The
        // chain edge rides along so a cycle can be drawn in full.
        self.chain.push(ChainEdge {
            module: importer,
            span: u.decl_span,
        });
        let mut hit = None;
        for k in (1..=texts.len()).rev() {
            if let Some(m) = self.load(&texts[..k]) {
                hit = Some((m, k));
                break;
            }
        }
        self.chain.pop();
        let Some((target, k)) = hit else {
            let first_span = segments[0].1;
            let mut cands: Vec<String> = self.loader.root_module_names();
            cands.push("std".to_string());
            cands.extend(self.aliases.names().map(str::to_string));
            cands.sort();
            cands.dedup();
            self.diags
                .push(unresolved_import(first_span, &segments[0].0, &cands));
            return vec![poisoned(u, segments)];
        };
        if target == importer {
            self.diags.push(
                Diagnostic::error(
                    codes::E0306,
                    u.decl_span,
                    "a module cannot import itself".to_string(),
                )
                .with_label("this file is already part of that module")
                .with_note(
                    "every `.lu` file in the directory shares one namespace — sibling \
                     files' names are visible here without any import.",
                ),
            );
            return vec![poisoned(u, segments)];
        }
        if self.loading[target] {
            self.report_cycle(importer, target, u.decl_span);
            // Bind anyway (the table exists) so uses do not cascade.
        } else {
            self.modules[importer].deps.insert(target);
        }
        let rest = &segments[k..];
        if let Some(group) = &u.group {
            if !rest.is_empty() {
                let span = segments[0].1;
                self.diags
                    .push(unresolved_import(span, &texts.join("."), &[]));
                return vec![poisoned(u, segments)];
            }
            let mut out = Vec::new();
            for (gname, gspan) in group {
                let mut sub = texts[..k].to_vec();
                sub.push(gname.clone());
                // A group name may be a child module or an item.
                let target_kind = if let Some(m) = self.load(&sub) {
                    if self.loading[m] {
                        self.report_cycle(importer, m, *gspan);
                    } else if m != importer {
                        self.modules[importer].deps.insert(m);
                    }
                    BindTarget::PkgModule(m)
                } else {
                    self.item_import(importer, target, gname, *gspan)
                };
                out.push(group_binding(u, gname, *gspan, target_kind));
            }
            return out;
        }
        match rest {
            [] => vec![whole_binding(u, segments, BindTarget::PkgModule(target))],
            [(iname, ispan)] => {
                let t = self.item_import(importer, target, iname, *ispan);
                vec![whole_binding(u, segments, t)]
            }
            _ => {
                let span = segments[0].1;
                self.diags
                    .push(unresolved_import(span, &texts.join("."), &[]));
                vec![poisoned(u, segments)]
            }
        }
    }

    /// Resolve an item import (`use module.item`): existence plus
    /// visibility, reporting E0301/E0304 as needed.
    fn item_import(
        &mut self,
        _importer: usize,
        module: usize,
        name: &str,
        span: Span,
    ) -> BindTarget {
        let mod_name = self.modules[module].display_name();
        match self.tables[module].get(name) {
            None => {
                let cands: Vec<String> = self.tables[module].names().map(str::to_string).collect();
                self.diags
                    .push(no_such_item(span, name, mod_name.trim_matches('`'), &cands));
                BindTarget::Poisoned
            }
            Some(item) if item.vis == Vis::Private => {
                self.diags.push(private_item(span, name, &mod_name, item));
                BindTarget::Item {
                    module,
                    name: name.to_string(),
                }
            }
            Some(_) => BindTarget::Item {
                module,
                name: name.to_string(),
            },
        }
    }

    /// Report the full cycle: `target → … → importer → target`.
    fn report_cycle(&mut self, importer: usize, target: usize, closing: Span) {
        let pos = self
            .chain
            .iter()
            .position(|e| e.module == target)
            .unwrap_or(0);
        let mut names: Vec<String> = self.chain[pos..]
            .iter()
            .map(|e| self.modules[e.module].display_name())
            .collect();
        names.push(self.modules[importer].display_name());
        names.push(self.modules[target].display_name());
        names.dedup();
        let mut d = Diagnostic::error(
            codes::E0303,
            closing,
            format!(
                "this import completes a cycle: {}",
                names.join(" \u{2192} ")
            ),
        )
        .with_label("the cycle closes here");
        for e in &self.chain[pos..] {
            d = d.with_secondary(
                e.span,
                format!(
                    "{} enters the cycle here",
                    self.modules[e.module].display_name()
                ),
            );
        }
        d = d.with_note(
            "imports between modules must form a DAG (D32); extract what both sides \
             need into a third module they can each import.",
        );
        self.diags.push(d);
    }
}

// ------------------------------------------------- diagnostic builders --

fn std_suggestions() -> Vec<String> {
    prelude::STD_MODULES
        .iter()
        .map(|m| m.path.join("."))
        .collect()
}

fn suggestion_for(input: &str, cands: &[String]) -> Option<String> {
    let refs: Vec<&str> = cands.iter().map(|s| s.as_str()).collect();
    wolf_diag::suggest::best_match(input, &refs).map(str::to_string)
}

fn unresolved_import(span: Span, path: &str, cands: &[String]) -> Diagnostic {
    let mut d = Diagnostic::error(
        codes::E0301,
        span,
        format!("there is no module named `{path}` to import"),
    )
    .with_label("cannot be resolved");
    if let Some(hit) = suggestion_for(path, cands) {
        d = d.with_note(format!("did you mean `{hit}`?"));
    } else {
        d = d.with_note(
            "a module is a sibling directory of the package root, `std.…`, or a \
             package alias.",
        );
    }
    d
}

fn no_such_item(span: Span, name: &str, module: &str, cands: &[String]) -> Diagnostic {
    let mut d = Diagnostic::error(
        codes::E0301,
        span,
        format!("module `{module}` has no item named `{name}`"),
    )
    .with_label("not found in the module");
    if let Some(hit) = suggestion_for(name, cands) {
        d = d.with_suggestion(wolf_diag::Suggestion::new(
            format!("did you mean `{hit}`?"),
            vec![(span, hit)],
            wolf_diag::Applicability::Maybe,
        ));
    }
    d
}

fn private_item(span: Span, name: &str, module: &str, item: &Item) -> Diagnostic {
    Diagnostic::error(
        codes::E0304,
        span,
        format!("`{name}` exists in {module}, but it is private"),
    )
    .with_label("not visible from here")
    .with_secondary(item.name_span, "defined here, without `pub`")
    .with_note(format!(
        "mark the {} `pub` to export it from the module, or `pub(pkg)` to share \
         it within this package — `pub(pkg)` is enough for this use.",
        item.kind.keyword()
    ))
}

// -------------------------------------------------- binding builders ----

fn poisoned(u: &UseData, segments: &[(String, Span)]) -> Binding {
    let (name, name_span) = bound_name(u, segments);
    Binding {
        name,
        target: BindTarget::Poisoned,
        decl_span: u.decl_span,
        name_span,
        removal: vec![(u.decl_span, String::new())],
        unused_exempt: true,
    }
}

fn whole_binding(u: &UseData, segments: &[(String, Span)], target: BindTarget) -> Binding {
    let (name, name_span) = bound_name(u, segments);
    Binding {
        name,
        target,
        decl_span: u.decl_span,
        name_span,
        removal: vec![(u.decl_span, String::new())],
        unused_exempt: false,
    }
}

fn group_binding(u: &UseData, name: &str, name_span: Span, target: BindTarget) -> Binding {
    let unused_exempt = target == BindTarget::Poisoned;
    Binding {
        name: name.to_string(),
        target,
        decl_span: u.decl_span,
        name_span,
        // Group-member removal edits are computed at report time (the
        // whole line goes when every member is unused).
        removal: vec![(name_span, String::new())],
        unused_exempt,
    }
}

fn bound_name(u: &UseData, segments: &[(String, Span)]) -> (String, Span) {
    if let Some((a, s)) = &u.alias {
        return (a.clone(), *s);
    }
    segments
        .last()
        .map(|(n, s)| (n.clone(), *s))
        .unwrap_or_else(|| ("_".to_string(), u.decl_span))
}

// -------------------------------------------------------- tree readers --

fn text(src: &[u8], span: Span) -> String {
    String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned()
}

/// Extract every `use`/`import c` declaration of a file, in order.
fn extract_uses(root: &GreenNode, src: &[u8]) -> Vec<UseData> {
    let mut out = Vec::new();
    for node in root.nodes() {
        match node.kind {
            SyntaxKind::UseDecl => {
                let u = UseDecl::cast(node).expect("kind checked");
                let segments = u
                    .path()
                    .map(|p| {
                        p.segments()
                            .map(|t| (text(src, t.span), t.span))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let group = u.group().map(|g| {
                    g.names()
                        .map(|t| (text(src, t.span), t.span))
                        .collect::<Vec<_>>()
                });
                let alias = u.alias().map(|t| (text(src, t.span), t.span));
                out.push(UseData {
                    segments,
                    group,
                    alias,
                    decl_span: node.span,
                    import_c: false,
                });
            }
            SyntaxKind::ImportCDecl => {
                let _ = ImportCDecl::cast(node);
                out.push(UseData {
                    segments: Vec::new(),
                    group: None,
                    alias: None,
                    decl_span: node.span,
                    import_c: true,
                });
            }
            _ => {}
        }
    }
    out
}

/// Names a pattern binds, in source order.
pub(crate) fn pattern_names(node: &GreenNode, src: &[u8]) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    fn walk(node: &GreenNode, src: &[u8], out: &mut Vec<(String, Span)>) {
        match node.kind {
            SyntaxKind::IdentPat => {
                if let Some(t) = node.child_token(SyntaxKind::Ident) {
                    out.push((text(src, t.span), t.span));
                }
            }
            SyntaxKind::BindingPat => {
                if let Some(t) = node.child_token(SyntaxKind::Ident) {
                    out.push((text(src, t.span), t.span));
                }
                for c in node.nodes() {
                    walk(c, src, out);
                }
            }
            SyntaxKind::TuplePat | SyntaxKind::OrPat | SyntaxKind::PathPat => {
                // A PathPat's path is a tag/variant (type-dependent,
                // s13); only its payload patterns bind.
                for c in node.nodes().filter(|n| is_pattern_kind(n.kind)) {
                    walk(c, src, out);
                }
            }
            _ => {}
        }
    }
    walk(node, src, &mut out);
    out
}

/// Collect the top-level items of one file: (name, kind, vis, name
/// span, decl index among item nodes).
fn collect_items(root: &GreenNode, src: &[u8]) -> Vec<(String, ItemKind, Vis, Span, usize)> {
    use wolf_ast::{
        ConstDecl, EnumDecl, FnDecl, LetDecl, StructDecl, TraitDecl, TypeDecl, VarDecl,
    };
    let mut out = Vec::new();
    for (decl, node) in root.nodes().filter(|n| n.kind.is_item()).enumerate() {
        let vis = item_vis(node);
        let mut single = |name: Option<&wolf_ast::GreenToken>, kind: ItemKind| {
            if let Some(t) = name {
                out.push((text(src, t.span), kind, vis, t.span, decl));
            }
        };
        match node.kind {
            SyntaxKind::FnDecl => single(FnDecl::cast(node).and_then(|d| d.name()), ItemKind::Fn),
            SyntaxKind::TypeDecl => {
                single(TypeDecl::cast(node).and_then(|d| d.name()), ItemKind::Type)
            }
            SyntaxKind::StructDecl => single(
                StructDecl::cast(node).and_then(|d| d.name()),
                ItemKind::Struct,
            ),
            SyntaxKind::EnumDecl => {
                single(EnumDecl::cast(node).and_then(|d| d.name()), ItemKind::Enum)
            }
            SyntaxKind::TraitDecl => single(
                TraitDecl::cast(node).and_then(|d| d.name()),
                ItemKind::Trait,
            ),
            SyntaxKind::ConstDecl => single(
                ConstDecl::cast(node).and_then(|d| d.name()),
                ItemKind::Const,
            ),
            SyntaxKind::LetDecl => {
                if let Some(pat) = LetDecl::cast(node).and_then(|d| d.pattern()) {
                    for (name, span) in pattern_names(pat, src) {
                        out.push((name, ItemKind::Let, vis, span, decl));
                    }
                }
            }
            SyntaxKind::VarDecl => {
                if let Some(pat) = VarDecl::cast(node).and_then(|d| d.pattern()) {
                    for (name, span) in pattern_names(pat, src) {
                        out.push((name, ItemKind::Var, vis, span, decl));
                    }
                }
            }
            _ => {} // impl blocks, imports: no module-level name
        }
    }
    out
}

/// An item node's declared visibility.
pub(crate) fn item_vis(node: &GreenNode) -> Vis {
    match node.child_node(SyntaxKind::Visibility) {
        None => Vis::Private,
        Some(v) => {
            if v.child_token(SyntaxKind::LParen).is_some() {
                Vis::Pkg
            } else {
                Vis::Pub
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(files: &[(&[&str], &str, &str)]) -> Package {
        let mut ml = MemoryLoader::new("t");
        for (m, n, s) in files {
            ml.add_file(m, n, s);
        }
        load_package(&mut ml, &AliasTable::default()).expect("root loads")
    }

    #[test]
    fn lazy_loading_skips_unimported_siblings() {
        let pkg = load(&[
            (&[], "main.lu", "fn main() -> !int { 0 }\n"),
            (&["noise"], "n.lu", "fn main() -> !int { 0 }\n"),
        ]);
        assert_eq!(pkg.modules.len(), 1, "unimported `noise` must not load");
        assert!(pkg.diagnostics.is_empty());
    }

    #[test]
    fn import_loads_and_orders_deps_first() {
        let pkg = load(&[
            (
                &[],
                "main.lu",
                "use util\nfn main() -> !int { util.helper() }\n",
            ),
            (&["util"], "u.lu", "pub fn helper() -> int { 1 }\n"),
        ]);
        assert_eq!(pkg.modules.len(), 2);
        assert_eq!(pkg.topo, vec![1, 0], "deps come before dependents");
        assert!(pkg.modules[0].deps.contains(&1));
    }

    #[test]
    fn duplicate_definitions_across_files_report_e0302() {
        let pkg = load(&[
            (
                &[],
                "a.lu",
                "fn twice() -> int { 1 }\nfn main() -> !int { twice() }\n",
            ),
            (&[], "b.lu", "//! member: true\nfn twice() -> int { 2 }\n"),
        ]);
        let codes: Vec<&str> = pkg.diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert_eq!(codes, ["E0302"]);
        assert_eq!(pkg.diagnostics[0].secondary.len(), 1, "both spans shown");
    }

    #[test]
    fn cycle_reports_the_full_path() {
        let pkg = load(&[
            (
                &[],
                "main.lu",
                "use alpha\nfn main() -> !int { alpha.a() }\n",
            ),
            (
                &["alpha"],
                "a.lu",
                "use beta\npub fn a() -> int { beta.b() }\n",
            ),
            (
                &["beta"],
                "b.lu",
                "use alpha\npub fn b() -> int { alpha.a() }\n",
            ),
        ]);
        let cycles: Vec<&Diagnostic> = pkg
            .diagnostics
            .iter()
            .filter(|d| d.code == codes::E0303)
            .collect();
        assert_eq!(cycles.len(), 1);
        assert!(cycles[0].message.contains('\u{2192}'), "renders the path");
        assert!(!cycles[0].secondary.is_empty(), "every edge has a span");
    }

    #[test]
    fn self_import_is_e0306() {
        let pkg = load(&[
            (&[], "main.lu", "use me\nfn main() -> !int { me.x() }\n"),
            (&["me"], "m.lu", "use me\npub fn x() -> int { 1 }\n"),
        ]);
        assert!(
            pkg.diagnostics.iter().any(|d| d.code == codes::E0306),
            "{:?}",
            pkg.diagnostics
        );
    }

    #[test]
    fn item_and_group_imports_bind() {
        let pkg = load(&[
            (
                &[],
                "main.lu",
                "use util.helper\nuse util.{other}\nfn main() -> !int { helper() + other() }\n",
            ),
            (
                &["util"],
                "u.lu",
                "pub fn helper() -> int { 1 }\npub fn other() -> int { 2 }\n",
            ),
        ]);
        assert!(pkg.diagnostics.is_empty(), "{:?}", pkg.diagnostics);
        let names: Vec<&str> = pkg.modules[0].bindings[0]
            .iter()
            .map(|b| b.name.as_str())
            .collect();
        assert_eq!(names, ["helper", "other"]);
    }

    #[test]
    fn private_item_import_is_e0304() {
        let pkg = load(&[
            (
                &[],
                "main.lu",
                "use util.hidden\nfn main() -> !int { hidden() }\n",
            ),
            (&["util"], "u.lu", "fn hidden() -> int { 1 }\n"),
        ]);
        assert!(pkg.diagnostics.iter().any(|d| d.code == codes::E0304));
    }

    #[test]
    fn std_use_binds_fs() {
        let pkg = load(&[(
            &[],
            "main.lu",
            "use std.fs\nfn main() -> !int { fs.read_text(\"x\") else 0\n0 }\n",
        )]);
        assert!(
            pkg.diagnostics.is_empty(),
            "std.fs must resolve: {:?}",
            pkg.diagnostics
        );
        assert!(matches!(
            pkg.modules[0].bindings[0][0].target,
            BindTarget::StdModule(_)
        ));
    }
}
