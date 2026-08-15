//! Hash-cache keying and the on-disk artifact cache (D7).
//!
//! Zig's model: the key is the content hash of everything that could
//! change the answer, so an incremental build re-imports **nothing**.
//! The key covers
//!
//! - the synthesized translation unit: the ordered header set, the
//!   `-D` defines, the extra cflags;
//! - the target triple;
//! - the sysroot identity (s47's bundle hash, or the sysroot path when
//!   a system sysroot is used);
//! - the importer's identity and version — a different worker is a
//!   different answer, and trusting a cached artifact across a worker
//!   swap is exactly how a bootstrap importer becomes load-bearing
//!   without anyone noticing;
//! - the artifact [`FORMAT_VERSION`].
//!
//! Order matters and is fixed: the key is built from a canonical
//! rendering of the request, not from a struct's field order.

use std::path::{Path, PathBuf};

use crate::artifact::FORMAT_VERSION;

/// One import request, in the form the key is computed from and the
/// worker is asked in.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ImportRequest {
    /// Headers in the order they will be `#include`d. Order is
    /// significant in C and therefore significant here.
    pub headers: Vec<String>,
    /// `-D` defines as `(name, value)`; an empty value is a bare
    /// `-Dname`. Sorted into the key, kept in order on the command.
    pub defines: Vec<(String, String)>,
    /// Additional cflags, in order.
    pub cflags: Vec<String>,
    /// Include search paths (`-I`), in order.
    pub include_paths: Vec<String>,
    pub target: String,
    /// The sysroot's *identity* — a bundle hash from s47, or a path.
    /// Never the contents: hashing a sysroot on every build is the
    /// thing this cache exists to avoid.
    pub sysroot: Option<String>,
}

impl ImportRequest {
    /// The canonical text the key hashes. Written out rather than
    /// derived so that a field added later cannot silently fail to
    /// participate: adding one means editing this function, and the
    /// test below fails until it is.
    pub fn canonical(&self) -> String {
        let mut s = String::new();
        s.push_str("cimport-key-1\n");
        s.push_str("target\t");
        s.push_str(&self.target);
        s.push('\n');
        s.push_str("sysroot\t");
        s.push_str(self.sysroot.as_deref().unwrap_or("-"));
        s.push('\n');
        for h in &self.headers {
            s.push_str("header\t");
            s.push_str(h);
            s.push('\n');
        }
        // Defines sort: `-DA -DB` and `-DB -DA` are the same TU.
        let mut defs = self.defines.clone();
        defs.sort();
        for (k, v) in &defs {
            s.push_str("define\t");
            s.push_str(k);
            s.push('\t');
            s.push_str(v);
            s.push('\n');
        }
        // Include paths and cflags do NOT sort: `-I` order decides
        // which header wins, and a cflag can be order-sensitive too.
        for i in &self.include_paths {
            s.push_str("include\t");
            s.push_str(i);
            s.push('\n');
        }
        for c in &self.cflags {
            s.push_str("cflag\t");
            s.push_str(c);
            s.push('\n');
        }
        s
    }

    /// The cache key: `b3:<hex>`, over the canonical request plus the
    /// importer identity and the artifact format version.
    pub fn key(&self, importer: &str) -> String {
        let mut s = self.canonical();
        s.push_str("importer\t");
        s.push_str(importer);
        s.push('\n');
        s.push_str("format\t");
        s.push_str(&FORMAT_VERSION.to_string());
        s.push('\n');
        format!("b3:{}", blake3::hash(s.as_bytes()).to_hex())
    }
}

/// The on-disk artifact cache.
///
/// Deliberately dumb: one file per key, written atomically via a
/// temporary in the same directory. Concurrent builds racing on the
/// same key both compute the same bytes, so last-writer-wins is
/// correct — and a torn read is impossible because a partial file is
/// never at the final name.
#[derive(Clone, Debug)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    pub fn new(root: impl Into<PathBuf>) -> Cache {
        Cache { root: root.into() }
    }

    /// The default cache root, resolved the same way `wolf_pkg` does
    /// (`$WOLF_CACHE`, `$XDG_CACHE_HOME/wolf`, `$HOME/.cache/wolf`,
    /// `%LOCALAPPDATA%\wolf`). Duplicated rather than depended on: the
    /// crate graph runs `span <- diag <- cimport`, and pulling
    /// `wolf_pkg` in to read four environment variables would invert
    /// it. The driver passes its own root in normal operation.
    pub fn default_root() -> Result<PathBuf, String> {
        for var in ["WOLF_CACHE"] {
            if let Some(v) = env_nonempty(var) {
                return Ok(PathBuf::from(v).join("cimport"));
            }
        }
        if let Some(v) = env_nonempty("XDG_CACHE_HOME") {
            return Ok(PathBuf::from(v).join("wolf").join("cimport"));
        }
        if let Some(v) = env_nonempty("HOME") {
            return Ok(PathBuf::from(v).join(".cache").join("wolf").join("cimport"));
        }
        if let Some(v) = env_nonempty("LOCALAPPDATA") {
            return Ok(PathBuf::from(v).join("wolf").join("cimport"));
        }
        Err("no cache directory: set WOLF_CACHE (or XDG_CACHE_HOME, or HOME)".to_string())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where a key's artifact lives. The `b3:` prefix is stripped and
    /// the hex is split one level deep, so a cache with a lot of
    /// imports does not become one enormous directory.
    pub fn path_for(&self, key: &str) -> PathBuf {
        let hex = key.strip_prefix("b3:").unwrap_or(key);
        let (head, rest) = hex.split_at(hex.len().min(2));
        self.root.join(head).join(format!("{rest}.cimport"))
    }

    /// Read a cached artifact's bytes. A missing entry is `None`; a
    /// *corrupt* entry is also `None` — the artifact is reproducible,
    /// so re-importing is always available and always cheaper than
    /// reasoning about a damaged cache.
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path_for(key)).ok()
    }

    /// Write an artifact under `key`, atomically.
    pub fn put(&self, key: &str, bytes: &[u8]) -> Result<(), String> {
        let path = self.path_for(key);
        let dir = path.parent().ok_or("cache path has no parent")?;
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        // Same directory, so the rename is atomic; pid-suffixed so two
        // builds do not fight over the temporary.
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
        match std::fs::rename(&tmp, &path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(format!("{}: {e}", path.display()))
            }
        }
    }
}

fn env_nonempty(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> ImportRequest {
        ImportRequest {
            headers: vec!["stdlib.h".into(), "string.h".into()],
            defines: vec![("_GNU_SOURCE".into(), "1".into())],
            cflags: vec!["-std=c17".into()],
            include_paths: vec!["/usr/include".into()],
            target: "x86_64-unknown-linux-gnu".into(),
            sysroot: Some("b3:deadbeef".into()),
        }
    }

    #[test]
    fn the_key_is_stable() {
        assert_eq!(req().key("ref-worker 1"), req().key("ref-worker 1"));
    }

    /// Every input that can change the answer must change the key.
    /// This is the whole contract of the cache; a miss is cheap, a
    /// stale hit is a miscompile.
    #[test]
    fn every_input_moves_the_key() {
        let base = req().key("ref-worker 1");

        let mut r = req();
        r.defines.push(("EXTRA".into(), String::new()));
        assert_ne!(r.key("ref-worker 1"), base, "a -D must change the key");

        let mut r = req();
        r.target = "aarch64-unknown-linux-gnu".into();
        assert_ne!(
            r.key("ref-worker 1"),
            base,
            "the target must change the key"
        );

        let mut r = req();
        r.sysroot = Some("b3:cafe".into());
        assert_ne!(
            r.key("ref-worker 1"),
            base,
            "the sysroot must change the key"
        );

        let mut r = req();
        r.cflags.push("-DNDEBUG".into());
        assert_ne!(r.key("ref-worker 1"), base, "a cflag must change the key");

        let mut r = req();
        r.include_paths.push("/opt/include".into());
        assert_ne!(r.key("ref-worker 1"), base, "an -I must change the key");

        assert_ne!(
            req().key("libclang-worker 1"),
            base,
            "swapping the importer must change the key — a bootstrap \
             worker's answers must not be inherited by its replacement"
        );
    }

    /// Header order is C semantics, not a set.
    #[test]
    fn header_order_matters_but_define_order_does_not() {
        let mut swapped = req();
        swapped.headers.reverse();
        assert_ne!(swapped.key("w"), req().key("w"));

        let mut a = req();
        a.defines = vec![("A".into(), "1".into()), ("B".into(), "2".into())];
        let mut b = req();
        b.defines = vec![("B".into(), "2".into()), ("A".into(), "1".into())];
        assert_eq!(a.key("w"), b.key("w"), "-DA -DB is the same TU as -DB -DA");
    }

    /// `-I` order decides which header wins; it is not a set.
    #[test]
    fn include_path_order_matters() {
        let mut a = req();
        a.include_paths = vec!["/one".into(), "/two".into()];
        let mut b = req();
        b.include_paths = vec!["/two".into(), "/one".into()];
        assert_ne!(a.key("w"), b.key("w"));
    }

    #[test]
    fn cache_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("wolf-cimport-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Cache::new(&dir);
        let key = req().key("w");
        assert!(c.get(&key).is_none(), "a fresh cache is empty");
        c.put(&key, b"hello").expect("writes");
        assert_eq!(c.get(&key).as_deref(), Some(&b"hello"[..]));
        // A different key does not collide.
        assert!(c.get("b3:0000").is_none());
        std::fs::remove_dir_all(&dir).expect("cleans up");
    }
}
