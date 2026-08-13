//! `wolf doc` — the documentation generator (s53, D31/D34/D36).
//!
//! The stdlib's docs are **generated, not hand-maintained**, and they
//! cannot lie about types: every rendered signature comes from the
//! compiler's own pretty-printer (`wolf_sema::item_signature`, the same
//! renderer the `wolfi` interface and its hashes are built from), and
//! every doc comment comes from the one doc-comment model
//! ([`wolf_query::docs`]) that also feeds editor hover.
//!
//! # Two outputs, one site
//!
//! - **Static HTML** that needs no JavaScript to read and works from
//!   `file://` — an index page plus one page per module.
//! - **A stable JSON index** (`index.json`), the hook for search,
//!   cross-package linking and the future registry's docs hosting
//!   (X7-shaped: the format is fixed now, the service is later).
//!
//! # `--check`, and why it looks like `diag-catalog`
//!
//! Generated documentation that is not verified byte-for-byte drifts
//! from its source the first time someone edits the output. So the
//! generator is a *pure function* from a resolved package to a set of
//! (relative path, bytes) pairs ([`Site`]), and [`Site::check`] compares
//! that set against a directory — the same shape `cargo xtask
//! diag-catalog --check` gives `docs/diagnostics.md`. Nothing about the
//! rendering depends on the clock, the host, or map iteration order.
//!
//! # Coverage
//!
//! [`Coverage`] is the c08 burn-down list: every `pub` item that carries
//! no doc comment, and every one that carries no doctest. `wolf doc`
//! reports it always and gates on it when asked, which is how "the docs
//! cannot rot" stops being a slogan.

mod html;
mod json;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wolf_query::{DocFence, DocItem, DocPackage};

pub use html::escape;

/// The generated site: relative path → bytes. A `BTreeMap`, so the
/// order two runs write files in is the same order.
#[derive(Debug, Default, Clone)]
pub struct Site {
    pub files: BTreeMap<String, Vec<u8>>,
}

/// What a `--check` run found. Empty in every field = in sync.
#[derive(Debug, Default)]
pub struct Drift {
    /// Generated, but absent on disk.
    pub missing: Vec<String>,
    /// On disk and generated, with different bytes.
    pub changed: Vec<String>,
    /// On disk under the output root, but not generated — a stale page
    /// is as wrong as a changed one (it is still served).
    pub stale: Vec<String>,
}

impl Drift {
    pub fn in_sync(&self) -> bool {
        self.missing.is_empty() && self.changed.is_empty() && self.stale.is_empty()
    }
}

/// Generator options.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Document private items too (`--private`).
    pub private: bool,
    /// The resolved dependency surface, as `(alias, name, version)` —
    /// a package's documentation is generated against the dependency
    /// set that resolved, and the index records it so a reader can tell
    /// which world the page describes.
    pub deps: Vec<(String, String, String)>,
    /// A short label for the site's title (the package name by
    /// default; `std` for a `--std-root` run).
    pub title: String,
}

/// The doc-coverage burn-down (c08's gate, report-only until the
/// stdlib's gaps are closed).
#[derive(Debug, Default)]
pub struct Coverage {
    /// `pub`/`pub(pkg)` items considered.
    pub total: usize,
    /// Items carrying a doc comment.
    pub documented: usize,
    /// Items carrying at least one wolf EXAMPLE fence. Coverage asks
    /// "is there a reviewed example?", not "does this toolchain run
    /// it?" — see `wolf_query::docs`'s language tables.
    pub with_doctest: usize,
    /// `module::name` of every item with no doc comment, sorted.
    pub undocumented: Vec<String>,
    /// `module::name` of every documented item with no example, sorted.
    pub no_doctest: Vec<String>,
}

impl Coverage {
    /// The one-line summary `wolf doc` prints.
    pub fn summary(&self) -> String {
        format!(
            "doc coverage: {}/{} items documented, {}/{} carry an example",
            self.documented, self.total, self.with_doctest, self.total
        )
    }

    /// Does every considered item carry both a doc comment and a
    /// doctest? (The c08 facade gate, once the burn-down is done.)
    pub fn complete(&self) -> bool {
        self.undocumented.is_empty() && self.no_doctest.is_empty()
    }
}

/// One extracted doctest, ready for the test runner.
#[derive(Debug, Clone)]
pub struct Doctest {
    /// Dotted module path (`""` = the package root).
    pub module: String,
    /// The documented item, or `""` for a module-level fence.
    pub item: String,
    /// `module::item#N` — stable, and the name `wolf test` reports.
    pub name: String,
    pub fence: DocFence,
}

/// Render a package's documentation site.
pub fn render(docs: &DocPackage, opts: &Options) -> Site {
    let title = if opts.title.is_empty() {
        docs.package.clone()
    } else {
        opts.title.clone()
    };
    let mut site = Site::default();
    site.files
        .insert("index.html".to_string(), html::index(docs, opts, &title));
    for module in &docs.modules {
        let path = html::module_page_name(&module.path);
        site.files
            .insert(path, html::module_page(docs, module, opts, &title));
    }
    site.files.insert(
        "index.json".to_string(),
        json::index(docs, opts, &title).into_bytes(),
    );
    site.files
        .insert("style.css".to_string(), html::STYLE.as_bytes().to_vec());
    site
}

impl Site {
    /// Write the site under `root`, creating directories as needed.
    /// Every generated file is written; nothing else is touched (a
    /// `--check` run is what reports strays).
    pub fn write(&self, root: &Path) -> Result<(), String> {
        for (rel, bytes) in &self.files {
            let at = root.join(rel);
            if let Some(parent) = at.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("create {}: {e}", parent.display()))?;
            }
            std::fs::write(&at, bytes).map_err(|e| format!("write {}: {e}", at.display()))?;
        }
        Ok(())
    }

    /// Compare the site against `root` byte-for-byte — the `--check`
    /// mode, and the sibling of `cargo xtask diag-catalog --check`.
    pub fn check(&self, root: &Path) -> Drift {
        let mut drift = Drift::default();
        for (rel, bytes) in &self.files {
            let at = root.join(rel);
            match std::fs::read(&at) {
                Ok(on_disk) if &on_disk == bytes => {}
                Ok(_) => drift.changed.push(rel.clone()),
                Err(_) => drift.missing.push(rel.clone()),
            }
        }
        let mut on_disk = Vec::new();
        collect_files(root, root, &mut on_disk);
        on_disk.sort();
        for rel in on_disk {
            if !self.files.contains_key(&rel) {
                drift.stale.push(rel);
            }
        }
        drift
    }
}

fn collect_files(root: &Path, at: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(at) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            collect_files(root, &p, out);
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// The doc-coverage report over the documented surface.
pub fn coverage(docs: &DocPackage) -> Coverage {
    let mut c = Coverage::default();
    for (module, item) in docs.items() {
        // Private items are not published surface; the gate is about
        // what consumers can see.
        if item.vis == wolf_sema::Vis::Private {
            continue;
        }
        c.total += 1;
        let qualified = qualify(&module.path, &item.name);
        match &item.doc {
            Some(d) => {
                c.documented += 1;
                if d.has_example() {
                    c.with_doctest += 1;
                } else {
                    c.no_doctest.push(qualified);
                }
            }
            None => c.undocumented.push(qualified),
        }
    }
    c.undocumented.sort();
    c.no_doctest.sort();
    c
}

/// Every doctest in the package, in a deterministic order.
pub fn doctests(docs: &DocPackage) -> Vec<Doctest> {
    let mut out = Vec::new();
    for module in &docs.modules {
        if let Some(d) = &module.doc {
            for (n, fence) in d.fences.iter().filter(|f| f.is_doctest()).enumerate() {
                out.push(Doctest {
                    module: module.path.clone(),
                    item: String::new(),
                    name: format!("{}#{n}", module_label(&module.path)),
                    fence: fence.clone(),
                });
            }
        }
        for item in &module.items {
            let Some(d) = &item.doc else { continue };
            for (n, fence) in d.fences.iter().filter(|f| f.is_doctest()).enumerate() {
                out.push(Doctest {
                    module: module.path.clone(),
                    item: item.name.clone(),
                    name: format!("{}#{n}", qualify(&module.path, &item.name)),
                    fence: fence.clone(),
                });
            }
        }
    }
    out
}

fn module_label(path: &str) -> String {
    if path.is_empty() {
        "(root)".to_string()
    } else {
        path.to_string()
    }
}

fn qualify(module: &str, name: &str) -> String {
    if module.is_empty() {
        name.to_string()
    } else {
        format!("{module}::{name}")
    }
}

/// The visibility word a page prints.
pub(crate) fn vis_word(item: &DocItem) -> &'static str {
    match item.vis {
        wolf_sema::Vis::Pub => "pub",
        wolf_sema::Vis::Pkg => "pub(pkg)",
        wolf_sema::Vis::Private => "private",
    }
}
