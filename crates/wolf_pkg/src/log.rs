//! The transparency-log record format (s51 Target 4, X7): designed
//! registry-shaped NOW, transported dumbly at v1.
//!
//! One append-only line per published `owner/pkg@version`:
//!
//! ```text
//! record := key " tree=" addr " manifest=" addr " interface=" addr
//! key    := owner "/" pkg "@" version
//! addr   := "b3:" hex64
//! ```
//!
//! Nothing here assumes VCS transport (the X7 contract): the same
//! lookup/verify logic serves a statically hosted file today and the
//! c15 registry later — only the fetch URL scheme changes. Merkle
//! tree heads, signatures, and inclusion proofs are the service-side
//! half and arrive with it; the *record* format is what must not
//! change, so it is pinned (and tested) now.

use std::path::Path;

/// One log record: the three content addresses that make a published
/// version immutable — tree (the paths-filtered source), manifest,
/// and interface (I11: definition-hash identity).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LogRecord {
    /// `owner/pkg@version`.
    pub key: String,
    /// `b3:` address of the package tree ([`crate::lock::hash_tree`]).
    pub tree: String,
    /// `b3:` address of the manifest bytes.
    pub manifest: String,
    /// `b3:` address of the package's `.wolfi` interface surface.
    pub interface: String,
}

impl LogRecord {
    pub fn render(&self) -> String {
        format!(
            "{} tree={} manifest={} interface={}\n",
            self.key, self.tree, self.manifest, self.interface
        )
    }

    pub fn parse(line: &str) -> Result<LogRecord, String> {
        let mut parts = line.split_whitespace();
        let key = parts.next().ok_or("empty log line")?.to_string();
        let mut tree = None;
        let mut manifest = None;
        let mut interface = None;
        for p in parts {
            if let Some(v) = p.strip_prefix("tree=") {
                tree = Some(v.to_string());
            } else if let Some(v) = p.strip_prefix("manifest=") {
                manifest = Some(v.to_string());
            } else if let Some(v) = p.strip_prefix("interface=") {
                interface = Some(v.to_string());
            } else {
                return Err(format!("unknown log field `{p}`"));
            }
        }
        Ok(LogRecord {
            key,
            tree: tree.ok_or("log record missing tree=")?,
            manifest: manifest.ok_or("log record missing manifest=")?,
            interface: interface.ok_or("log record missing interface=")?,
        })
    }
}

/// The b3 content address of a byte string (manifest/interface hashes
/// for log records).
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("b3:{}", blake3::hash(bytes).to_hex())
}

/// Look `key` up in a static log file (dumb-file v1 transport:
/// one record per line). `Ok(None)` = not published.
pub fn lookup(log_file: &Path, key: &str) -> Result<Option<LogRecord>, String> {
    let text = match std::fs::read_to_string(log_file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("read {}: {e}", log_file.display())),
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let rec = LogRecord::parse(line)?;
        if rec.key == key {
            return Ok(Some(rec));
        }
    }
    Ok(None)
}

/// Append a record (the maintainer flow behind `wolf publish` v1).
/// Append-only by contract: re-publishing an existing key is refused —
/// even the author cannot swap bits under a tag.
pub fn append(log_file: &Path, rec: &LogRecord) -> Result<(), String> {
    if lookup(log_file, &rec.key)?.is_some() {
        return Err(format!(
            "`{}` is already in the log — a published version is immutable (append-only, X7)",
            rec.key
        ));
    }
    let mut text = std::fs::read_to_string(log_file).unwrap_or_default();
    text.push_str(&rec.render());
    std::fs::write(log_file, text).map_err(|e| format!("write {}: {e}", log_file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_round_trips() {
        let rec = LogRecord {
            key: "acme/redis@1.4.0".to_string(),
            tree: hash_bytes(b"tree"),
            manifest: hash_bytes(b"manifest"),
            interface: hash_bytes(b"iface"),
        };
        let parsed = LogRecord::parse(rec.render().trim()).unwrap();
        assert_eq!(parsed, rec);
    }

    #[test]
    fn append_is_append_only() {
        let f = std::env::temp_dir().join(format!("wolf-pkg-log-{}", std::process::id()));
        let _ = std::fs::remove_file(&f);
        let rec = LogRecord {
            key: "a/b@1.0.0".to_string(),
            tree: hash_bytes(b"t"),
            manifest: hash_bytes(b"m"),
            interface: hash_bytes(b"i"),
        };
        append(&f, &rec).unwrap();
        assert_eq!(lookup(&f, "a/b@1.0.0").unwrap(), Some(rec.clone()));
        assert_eq!(lookup(&f, "a/b@2.0.0").unwrap(), None);
        // Same key, different bits: refused loudly.
        let tampered = LogRecord {
            tree: hash_bytes(b"other"),
            ..rec
        };
        let err = append(&f, &tampered).unwrap_err();
        assert!(err.contains("immutable"), "{err}");
        let _ = std::fs::remove_file(&f);
    }
}
