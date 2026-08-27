//! Byte spans, source maps, and interned file identities.
//!
//! Contract: the bottom of the crate graph — depends on nothing in the
//! workspace. Every diagnostic, token, tree node, and IR entity references
//! source positions through this crate's types. Spans are byte-exact
//! (see s07: lossless trivia and byte-offset string semantics per D25).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod line_index;

pub use line_index::{LineCol, LineIndex};

/// Interned identity of a source file. Cheap to copy, stable for the life
/// of the [`SourceMap`] that produced it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct FileId(u32);

impl FileId {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// Rebuild a [`FileId`] from [`FileId::index`]'s value. The one
    /// legitimate consumer is the lossless span chain THROUGH an IR
    /// that stores the index as a plain integer (WIR's per-function
    /// `src_file` debug aux) and needs the id back to mint diagnostic
    /// spans against the same [`SourceMap`]. Never invent indices.
    pub fn from_index(index: usize) -> FileId {
        FileId(u32::try_from(index).expect("file index fits u32"))
    }
}

/// A byte-exact half-open range `[lo, hi)` in one source file.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Span {
    pub file: FileId,
    pub lo: u32,
    pub hi: u32,
}

impl Span {
    /// Panics if `lo > hi` — spans are never inverted.
    pub fn new(file: FileId, lo: u32, hi: u32) -> Self {
        assert!(lo <= hi, "inverted span: {lo}..{hi}");
        Span { file, lo, hi }
    }

    pub fn len(self) -> u32 {
        self.hi - self.lo
    }

    pub fn is_empty(self) -> bool {
        self.lo == self.hi
    }

    /// Smallest span covering both. Panics across files — a joined span is
    /// only meaningful within one file.
    pub fn join(self, other: Span) -> Span {
        assert_eq!(self.file, other.file, "span join across files");
        Span {
            file: self.file,
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }
}

/// Path → [`FileId`] interner. Identity is the path as given (no
/// canonicalization here — the driver decides what a path means).
#[derive(Default, Debug)]
pub struct SourceMap {
    files: Vec<PathBuf>,
    index: HashMap<PathBuf, FileId>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern `path`, returning the existing id if already present.
    pub fn intern(&mut self, path: &Path) -> FileId {
        if let Some(&id) = self.index.get(path) {
            return id;
        }
        let id = FileId(u32::try_from(self.files.len()).expect("more than u32::MAX files"));
        self.files.push(path.to_path_buf());
        self.index.insert(path.to_path_buf(), id);
        id
    }

    /// The path interned for `id`. Panics on an id from another map.
    pub fn path(&self, id: FileId) -> &Path {
        &self.files[id.index()]
    }

    /// Every interned path, in intern order — position `i` is the path
    /// of `FileId` `i`. This is the index→path table machine outputs
    /// embed (diag-schema `files`), so span file indices resolve
    /// outside the process that interned them.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files.iter().map(PathBuf::as_path)
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}
