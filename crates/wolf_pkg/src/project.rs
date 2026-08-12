//! Project resolution over the disk (s51): walk `wolf.pkg` manifests
//! from a project root, resolve every dependency's source (`path`
//! trees in place, `git` trees through the content-addressed store),
//! and hand the driver the loader roots + the capability graph.
//!
//! Reading the full dependency graph never executes anything (D33):
//! this module reads manifests and files, runs `git clone` for pinned
//! fetches, and nothing else — no hooks, no scripts, no fallbacks.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wolf_diag::{Diagnostic, codes};
use wolf_span::{FileId, SourceMap, Span};

use crate::lock::{Lock, LockEntry};
use crate::manifest::{self, Cap, DepSource, Manifest};
use crate::source;

/// One resolved package (node in the dependency graph).
#[derive(Debug)]
pub struct ResolvedPkg {
    /// Import alias (first path segment consumers write); `""` for the
    /// root package.
    pub alias: String,
    /// Manifest `name` (falls back to the alias for manifest-less
    /// trees — wolf-std's layout is the blessed example).
    pub name: String,
    pub version: String,
    pub root: PathBuf,
    pub caps: Vec<Cap>,
    /// Where an E1504 lands: the `capabilities` list (or the name) in
    /// this package's own manifest, else the consumer's dep entry.
    pub blame_span: Option<Span>,
    /// Edges into `Project::pkgs`.
    pub deps: Vec<usize>,
    /// Content address for store-backed (git) packages.
    pub hash: Option<String>,
    /// The std facade rides its own loader slot, not `dep_roots`.
    pub is_std: bool,
}

/// A manifest file the caller should register with its `Sources` so
/// manifest-spanned diagnostics render.
#[derive(Debug)]
pub struct ManifestSource {
    pub file: FileId,
    pub display: String,
    pub text: String,
}

/// The resolved project.
#[derive(Debug, Default)]
pub struct Project {
    /// `pkgs[0]` is the root package.
    pub pkgs: Vec<ResolvedPkg>,
    /// alias → package root directory, for the module loader. One flat
    /// alias namespace at v0 (conflicting aliases are an error).
    pub dep_roots: BTreeMap<String, PathBuf>,
    /// A dependency named `std` with a `path` source: the std facade,
    /// served through the loader's std slot (`use std.…`).
    pub std_root: Option<PathBuf>,
    pub manifests: Vec<ManifestSource>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Project {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error)
    }

    /// The ledger this resolution witnesses: every non-root,
    /// non-facade package, keyed by alias — hash for store-backed
    /// trees, `-` for local ones; capability sets always (the
    /// `wolf audit` diff base).
    pub fn to_lock(&self) -> Lock {
        let mut lock = Lock::default();
        for p in &self.pkgs[1..] {
            if p.is_std {
                continue;
            }
            lock.entries.insert(
                p.alias.clone(),
                LockEntry {
                    name: p.alias.clone(),
                    version: p.version.clone(),
                    hash: p.hash.clone(),
                    caps: p.caps.clone(),
                },
            );
        }
        lock
    }
}

/// Resolution options.
pub struct ResolveOpts {
    /// The parsed `wolf.sum`, when one exists.
    pub lock: Option<Lock>,
    /// May an *unpinned* git dependency hit the network? Verbs
    /// (`wolf add/update`) say yes; `wolf build` says no — a build
    /// only fetches what the ledger already pins (offline-repeatable
    /// once the store is warm).
    pub fetch_unpinned: bool,
    /// Re-fetch pinned git deps (`wolf update`).
    pub refresh: bool,
    /// Content-store override (tests, hermetic tooling); `None` uses
    /// [`source::store_dir`] (WOLF_STORE / the user cache).
    pub store: Option<PathBuf>,
}

fn e1505(span: Span, msg: impl Into<String>) -> Diagnostic {
    Diagnostic::error(codes::E1505, span, msg)
}

/// Resolve the project rooted at `root_dir` (which must hold an s51
/// `pkg { }` manifest). Never panics on user input: every failure is a
/// diagnostic in `Project::diagnostics`.
pub fn resolve_project(root_dir: &Path, sm: &mut SourceMap, opts: &ResolveOpts) -> Project {
    let mut project = Project::default();
    let Some((root_manifest, root_file)) =
        load_manifest(root_dir, "wolf.pkg".to_string(), sm, &mut project)
    else {
        return project;
    };
    let root_span = root_manifest.span;
    project.pkgs.push(ResolvedPkg {
        alias: String::new(),
        name: root_manifest.name.clone(),
        version: root_manifest.version.to_string(),
        root: root_dir.to_path_buf(),
        caps: root_manifest.caps.clone(),
        blame_span: Some(root_manifest.caps_span.unwrap_or(root_manifest.name_span)),
        deps: Vec::new(),
        hash: None,
        is_std: false,
    });
    let _ = root_file;

    // BFS over dependency declarations. Each work item: (consumer
    // package index, its manifest) — path deps resolve relative to the
    // consumer's root.
    let mut work: Vec<(usize, Manifest)> = vec![(0, root_manifest)];
    // Canonical root path → node index (dedup: a diamond is one node).
    let mut by_root: BTreeMap<PathBuf, usize> = BTreeMap::new();
    by_root.insert(canon(root_dir), 0);

    while let Some((consumer, m)) = work.pop() {
        for dep in &m.deps {
            let node = resolve_dep(
                consumer,
                dep,
                &mut project,
                sm,
                opts,
                &mut by_root,
                &mut work,
            );
            if let Some(node) = node
                && !project.pkgs[consumer].deps.contains(&node)
            {
                project.pkgs[consumer].deps.push(node);
            }
        }
    }
    for p in &mut project.pkgs {
        p.deps.sort_unstable();
    }
    // The ledger is a witness: verify recorded hashes for everything
    // resolved (store-backed nodes hashed on the way in).
    if let Some(lock) = &opts.lock {
        for p in &project.pkgs[1..] {
            if let (Some(entry), Some(hash)) = (lock.entries.get(&p.alias), p.hash.as_ref())
                && let Some(pinned) = &entry.hash
                && pinned != hash
                && !opts.refresh
            {
                project.diagnostics.push(
                    Diagnostic::error(
                        codes::E1506,
                        root_span,
                        format!(
                            "dependency `{}` hashes {hash}, but wolf.sum pins {pinned}",
                            p.alias
                        ),
                    )
                    .with_note(
                        "the bits on disk are not the bits the ledger witnessed. If this \
                         change is intentional, `wolf update` refreshes the ledger; \
                         otherwise treat it as the supply-chain alarm it is."
                            .to_string(),
                    ),
                );
            }
        }
    }
    project
}

#[allow(clippy::too_many_arguments)]
fn resolve_dep(
    consumer: usize,
    dep: &crate::manifest::Dep,
    project: &mut Project,
    sm: &mut SourceMap,
    opts: &ResolveOpts,
    by_root: &mut BTreeMap<PathBuf, usize>,
    work: &mut Vec<(usize, Manifest)>,
) -> Option<usize> {
    let consumer_root = project.pkgs[consumer].root.clone();
    let (dir, hash) = match &dep.source {
        DepSource::Path { path } => (consumer_root.join(path), None),
        DepSource::Git { url, tag } => {
            let pinned = opts
                .lock
                .as_ref()
                .and_then(|l| l.entries.get(&dep.alias))
                .and_then(|e| e.hash.clone());
            if pinned.is_none() && !opts.fetch_unpinned {
                project.diagnostics.push(e1505(
                    dep.span,
                    format!(
                        "git dependency `{}` has no wolf.sum pin — run `wolf add`/`wolf update` \
                         to fetch and record it before building",
                        dep.alias
                    ),
                ));
                return None;
            }
            let store = match opts.store.clone().map(Ok).unwrap_or_else(source::store_dir) {
                Ok(s) => s,
                Err(e) => {
                    project.diagnostics.push(e1505(dep.span, e));
                    return None;
                }
            };
            match source::ensure_git(&store, url, tag, pinned.as_deref(), opts.refresh) {
                Ok((dir, hash)) => (dir, Some(hash)),
                Err(e) => {
                    let code = if e.contains("ledger witnessed") || e.contains("wolf.sum pins") {
                        codes::E1506
                    } else {
                        codes::E1505
                    };
                    project
                        .diagnostics
                        .push(Diagnostic::error(code, dep.span, e));
                    return None;
                }
            }
        }
        DepSource::Registry { pkg, .. } => {
            project.diagnostics.push(
                e1505(
                    dep.span,
                    format!(
                        "`{}` is a registry dependency, and the hosted registry arrives at c15",
                        pkg
                    ),
                )
                .with_note(
                    "v0 sources are `path:` and `git:` + `tag:` (X7: the entry form is \
                     stable now, the service later — only the fetch transport changes)."
                        .to_string(),
                ),
            );
            return None;
        }
    };
    if !dir.is_dir() {
        project.diagnostics.push(e1505(
            dep.span,
            format!(
                "dependency `{}` points at `{}`, which is not a directory",
                dep.alias,
                dir.display()
            ),
        ));
        return None;
    }
    let dir_canon = canon(&dir);

    // The std facade: a path dependency named `std` roots `use std.…`.
    if dep.alias == "std" {
        project.std_root = Some(dir.clone());
        if let Some(&i) = by_root.get(&dir_canon) {
            return Some(i);
        }
        let i = project.pkgs.len();
        project.pkgs.push(ResolvedPkg {
            alias: "std".to_string(),
            name: "std".to_string(),
            version: "0.0.0".to_string(),
            root: dir,
            caps: Vec::new(),
            blame_span: Some(dep.span),
            deps: Vec::new(),
            hash,
            is_std: true,
        });
        by_root.insert(dir_canon, i);
        return Some(i);
    }

    // Alias conflicts: one flat alias namespace at v0.
    if let Some(existing) = project.dep_roots.get(&dep.alias)
        && canon(existing) != dir_canon
    {
        project.diagnostics.push(e1505(
            dep.span,
            format!(
                "alias `{}` names two different packages (`{}` and `{}`) — \
                 aliases are one flat namespace at v0",
                dep.alias,
                existing.display(),
                dir.display()
            ),
        ));
        return None;
    }
    if let Some(&i) = by_root.get(&dir_canon) {
        project.dep_roots.insert(dep.alias.clone(), dir);
        return Some(i);
    }

    // A dependency with an s51 manifest resolves recursively; a bare
    // tree is a leaf package (no declared caps, no deps).
    let manifest_path = dir.join("wolf.pkg");
    let manifest_text = std::fs::read_to_string(&manifest_path).ok();
    let i = project.pkgs.len();
    let node = match manifest_text {
        Some(text) if manifest::is_manifest(&text) => {
            match load_manifest(&dir, format!("pkg://{}/wolf.pkg", dep.alias), sm, project) {
                Some((m, _)) => {
                    let node = ResolvedPkg {
                        alias: dep.alias.clone(),
                        name: m.name.clone(),
                        version: m.version.to_string(),
                        root: dir.clone(),
                        caps: m.caps.clone(),
                        blame_span: Some(m.caps_span.unwrap_or(m.name_span)),
                        deps: Vec::new(),
                        hash,
                        is_std: false,
                    };
                    work.push((i, m));
                    node
                }
                None => {
                    // load_manifest recorded why; the dep entry gets
                    // the pointer back to the failing package.
                    project.diagnostics.push(e1505(
                        dep.span,
                        format!(
                            "dependency `{}` has a manifest that does not resolve",
                            dep.alias
                        ),
                    ));
                    return None;
                }
            }
        }
        _ => ResolvedPkg {
            alias: dep.alias.clone(),
            name: dep.alias.clone(),
            version: "0.0.0".to_string(),
            root: dir.clone(),
            caps: Vec::new(),
            blame_span: Some(dep.span),
            deps: Vec::new(),
            hash,
            is_std: false,
        },
    };
    project.pkgs.push(node);
    by_root.insert(dir_canon, i);
    project.dep_roots.insert(dep.alias.clone(), dir);
    Some(i)
}

/// Read + parse `dir/wolf.pkg`; registers the text under a fresh
/// FileId with a deterministic display (`wolf.pkg` for the root,
/// `pkg://alias/wolf.pkg` for deps — an on-disk location never leaks
/// into diagnostics, D7) and accumulates all manifest diagnostics.
fn load_manifest(
    dir: &Path,
    display: String,
    sm: &mut SourceMap,
    project: &mut Project,
) -> Option<(Manifest, FileId)> {
    let path = dir.join("wolf.pkg");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            // No span exists yet for an unreadable root manifest; the
            // driver guards this path, so a plain-string surface is
            // honest enough here.
            let file = sm.intern(&path);
            let span = Span::new(file, 0, 0);
            project.manifests.push(ManifestSource {
                file,
                display,
                text: String::new(),
            });
            project
                .diagnostics
                .push(e1505(span, format!("cannot read {}: {e}", path.display())));
            return None;
        }
    };
    let file = sm.intern(&path);
    project.manifests.push(ManifestSource {
        file,
        display,
        text: text.clone(),
    });
    let (m, diags) = manifest::parse(file, &text);
    project.diagnostics.extend(diags);
    m.map(|m| (m, file))
}

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}
