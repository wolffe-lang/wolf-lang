//! The overlay store (contract clause 1) and the overlay-aware module
//! loader: the CLI's single-entry package semantics (conform-run rules,
//! `wolf_sema::DiskLoader`) read through open editor buffers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use wolf_sema::{DiskLoader, ModuleLoader, RawFile};
use wolf_span::SourceMap;

/// The trees a package resolves against beyond its own directory
/// (s235, wolf-lang#724): the std root that backs `use std.…` and the
/// home modules of std's data-type methods, and the dependency roots a
/// `wolf.pkg` names. The same two inputs `wolf build` hands
/// `wolf_sema::DiskLoader` (`with_std_root`, `with_dep_roots`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageRoots {
    /// `use std.X` reads `<std_root>/X/`; `None` keeps the prelude stub.
    pub std_root: Option<PathBuf>,
    /// Import alias → dependency package root (s51).
    pub dep_roots: BTreeMap<String, PathBuf>,
}

/// Package root directory (the entry file's directory, the CLI's
/// single-entry rule) → the roots it resolves against. The host's
/// owner supplies it: the query layer reads no environment, flag or
/// manifest of its own, so the order lives in exactly one place (the
/// driver's, shared with `wolf build`).
pub type RootsResolver = Arc<dyn Fn(&Path) -> PackageRoots + Send + Sync>;

/// Open-buffer contents layered over disk. Cloning is cheap (paths +
/// `Arc`'d texts); a [`crate::Snapshot`] owns a frozen clone.
#[derive(Debug, Default, Clone)]
pub struct OverlayStore {
    files: BTreeMap<PathBuf, Arc<Vec<u8>>>,
}

impl OverlayStore {
    pub fn set(&mut self, path: PathBuf, text: Vec<u8>) {
        self.files.insert(normalize(&path), Arc::new(text));
    }

    pub fn remove(&mut self, path: &Path) {
        self.files.remove(&normalize(path));
    }

    /// Overlay first, disk second. Disk misses are `None` — a missing
    /// file is an answer, not an error, at this layer.
    pub fn read(&self, path: &Path) -> Option<Arc<Vec<u8>>> {
        let path = normalize(path);
        if let Some(text) = self.files.get(&path) {
            return Some(Arc::clone(text));
        }
        std::fs::read(&path).ok().map(Arc::new)
    }

    /// Overlay paths directly inside `dir` with extension `lu` — the
    /// unsaved-and-never-saved files a directory listing would miss.
    fn overlaid_in_dir(&self, dir: &Path) -> Vec<PathBuf> {
        self.files
            .keys()
            .filter(|p| p.parent() == Some(dir) && p.extension().is_some_and(|e| e == "lu"))
            .cloned()
            .collect()
    }
}

/// Lexical path normalization (no filesystem access): strips `.` and
/// resolves `..` against named components so the same file edited and
/// listed arrives at one key. Symlink identity is out of scope for v0.
pub(crate) fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// `wolf_sema::DiskLoader`'s single-entry semantics, reading through an
/// [`OverlayStore`]: root = the entry file's directory, subdirectories
/// are child modules loaded on import, and membership follows
/// `wolf_sema::standalone_mark` (D59) — plain files join, standalone
/// entries stay out — mirroring the driver's participation rule
/// byte-for-byte (the one-truth invariant, contract clause 5).
/// Overlay-only files participate too — an unsaved new file is real to
/// the editor and therefore to us.
///
/// `use std.…` and dependency-alias paths read through
/// `wolf_sema::DiskLoader` itself, configured with [`PackageRoots`] —
/// the very loader `wolf build` uses, so the std and dependency trees
/// load with its whole-package membership and its `std://`/`pkg://`
/// displays (D7). Those trees are read from disk, not through the
/// overlays: an open, unsaved std buffer does not change what the
/// user's package resolves against, as it does not for `wolf build`.
pub(crate) struct OverlayLoader<'a> {
    root: PathBuf,
    entry: PathBuf,
    overlays: &'a OverlayStore,
    sm: &'a mut SourceMap,
    roots: PackageRoots,
}

impl<'a> OverlayLoader<'a> {
    /// `None` when `entry` has no parent directory or no readable text.
    pub(crate) fn new(
        entry: &Path,
        overlays: &'a OverlayStore,
        sm: &'a mut SourceMap,
    ) -> Option<Self> {
        let entry = normalize(entry);
        overlays.read(&entry)?;
        let root = entry.parent()?.to_path_buf();
        Some(OverlayLoader {
            root,
            entry,
            overlays,
            sm,
            roots: PackageRoots::default(),
        })
    }

    /// Resolve `std` and dependency paths against `roots`.
    pub(crate) fn with_roots(mut self, roots: PackageRoots) -> Self {
        self.roots = roots;
        self
    }

    /// Is `path` served by a std or dependency tree rather than the
    /// package directory? (`DiskLoader::load_module`'s own test.)
    fn is_external(&self, path: &[String]) -> bool {
        path.first().is_some_and(|s| {
            (s == "std" && self.roots.std_root.is_some()) || self.roots.dep_roots.contains_key(s)
        })
    }

    /// The build's loader over the same roots, borrowing this loader's
    /// source map so every file keeps one `FileId`.
    fn disk(&mut self) -> DiskLoader<'_> {
        DiskLoader::from_dir(&self.root, self.sm)
            .with_std_root(self.roots.std_root.clone())
            .with_dep_roots(self.roots.dep_roots.clone())
    }
}

impl ModuleLoader for OverlayLoader<'_> {
    fn load_module(&mut self, path: &[String]) -> Option<wolf_sema::LoadedModule> {
        if self.is_external(path) {
            return self.disk().load_module(path);
        }
        let mut dir = self.root.clone();
        for seg in path {
            dir.push(seg);
        }
        // Disk listing ∪ overlay listing, deduplicated, name-sorted —
        // the same deterministic order DiskLoader produces.
        let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "lu"))
            .map(|p| normalize(&p))
            .collect();
        for p in self.overlays.overlaid_in_dir(&dir) {
            if !names.contains(&p) {
                names.push(p);
            }
        }
        names.sort();
        let mut out = wolf_sema::LoadedModule::default();
        for p in names {
            let Some(src) = self.overlays.read(&p) else {
                continue;
            };
            let is_entry = p == self.entry;
            let base = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let excluded = !is_entry && wolf_sema::is_standalone_entry(&base, &src);
            let file = self.sm.intern(&p);
            let raw = RawFile {
                file,
                display: p.display().to_string().replace('\\', "/"),
                src: src.to_vec(),
            };
            if excluded {
                out.excluded.push(raw);
            } else {
                out.files.push(raw);
            }
        }
        // No directory and no overlays either: the module path names
        // nothing at all (DiskLoader's `None` case).
        if out.files.is_empty() && out.excluded.is_empty() && !dir.is_dir() {
            return None;
        }
        Some(out)
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

    fn has_std_root(&self) -> bool {
        self.roots.std_root.is_some()
    }

    fn std_module_names(&mut self) -> Vec<String> {
        self.disk().std_module_names()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_wins_over_disk_and_close_restores() {
        let dir = std::env::temp_dir().join(format!("wolf_query_ov_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.lu");
        std::fs::write(&p, b"disk").unwrap();
        let mut ov = OverlayStore::default();
        assert_eq!(&*ov.read(&p).unwrap(), b"disk");
        ov.set(p.clone(), b"overlay".to_vec());
        assert_eq!(&*ov.read(&p).unwrap(), b"overlay");
        ov.remove(&p);
        assert_eq!(&*ov.read(&p).unwrap(), b"disk");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn normalize_strips_dots() {
        assert_eq!(
            normalize(Path::new("/a/b/../c/./d.lu")),
            PathBuf::from("/a/c/d.lu")
        );
    }
}
