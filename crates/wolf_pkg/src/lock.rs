//! `wolf.sum` — the integrity ledger (s51 Target 3), plus the tree
//! hashing that content-addresses a package.
//!
//! The ledger is **not a resolution input**: MVS output is a pure
//! function of manifests; `wolf.sum` only witnesses that the bits a
//! build sees are the bits a human once fetched. One line per resolved
//! package, sorted, byte-stable:
//!
//! ```text
//! line := name " " version " " ("b3:" hex64 | "-") " caps=" (caplist | "-")
//! ```
//!
//! `path` dependencies carry `-` for the hash (a local tree is yours
//! to edit; pinning it would make every edit a "tamper") but still
//! record their capability set — `wolf audit` diffs capability
//! acquisition across upgrades from these lines (I13). Hashes are
//! blake3 over the source-filtered tree (`.lu`, `.wolfi`, `wolf.pkg`),
//! Zig-style: VCS noise never perturbs identity.

use std::collections::BTreeMap;
use std::path::Path;

use crate::manifest::Cap;

/// One `wolf.sum` line.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LockEntry {
    pub name: String,
    pub version: String,
    /// `b3:<hex>` content address, or `None` for path deps.
    pub hash: Option<String>,
    pub caps: Vec<Cap>,
}

/// The parsed ledger, keyed by package name.
#[derive(Clone, Default, Debug)]
pub struct Lock {
    pub entries: BTreeMap<String, LockEntry>,
}

impl Lock {
    pub fn parse(text: &str) -> Result<Lock, String> {
        let mut entries = BTreeMap::new();
        for (i, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (Some(name), Some(version), Some(hash), Some(caps)) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return Err(format!(
                    "wolf.sum line {}: expected `name version b3:hash caps=…`",
                    i + 1
                ));
            };
            let hash = match hash {
                "-" => None,
                h if h.starts_with("b3:") => Some(h.to_string()),
                other => {
                    return Err(format!(
                        "wolf.sum line {}: bad hash field `{other}` (b3:<hex> or -)",
                        i + 1
                    ));
                }
            };
            let Some(caplist) = caps.strip_prefix("caps=") else {
                return Err(format!("wolf.sum line {}: missing `caps=` field", i + 1));
            };
            let mut parsed_caps = Vec::new();
            if caplist != "-" {
                for c in caplist.split(',') {
                    parsed_caps.push(Cap::parse(c).ok_or_else(|| {
                        format!("wolf.sum line {}: unknown capability `{c}`", i + 1)
                    })?);
                }
            }
            entries.insert(
                name.to_string(),
                LockEntry {
                    name: name.to_string(),
                    version: version.to_string(),
                    hash,
                    caps: parsed_caps,
                },
            );
        }
        Ok(Lock { entries })
    }

    /// Byte-stable rendering: sorted by name, one line each.
    pub fn render(&self) -> String {
        let mut out = String::from(
            "# wolf.sum — integrity ledger. Do not edit; `wolf add/rm/update` rewrite it.\n",
        );
        for e in self.entries.values() {
            let hash = e.hash.as_deref().unwrap_or("-");
            let caps = if e.caps.is_empty() {
                "-".to_string()
            } else {
                e.caps
                    .iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            };
            out.push_str(&format!(
                "{} {} {} caps={}\n",
                e.name, e.version, hash, caps
            ));
        }
        out
    }
}

/// Does this file participate in a package's content identity?
/// Source-filtered on purpose: editor droppings, VCS metadata, and
/// build caches never perturb the hash.
fn is_identity_file(path: &Path) -> bool {
    if path.file_name().is_some_and(|n| n == "wolf.pkg") {
        return true;
    }
    path.extension().is_some_and(|e| e == "lu" || e == "wolfi")
}

/// The blake3 content address of a package tree: every identity file,
/// sorted by relative path (`/`-normalized), hashed as
/// `path \0 len \0 bytes`. Returns `b3:<hex>`.
pub fn hash_tree(root: &Path) -> Result<String, String> {
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    collect(root, root, &mut files)?;
    files.sort();
    let mut hasher = blake3::Hasher::new();
    for (rel, path) in files {
        let bytes = std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        hasher.update(rel.as_bytes());
        hasher.update(&[0]);
        hasher.update(bytes.len().to_le_bytes().as_slice());
        hasher.update(&[0]);
        hasher.update(&bytes);
    }
    Ok(format!("b3:{}", hasher.finalize().to_hex()))
}

fn collect(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, std::path::PathBuf)>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read dir {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        // VCS + cache directories are never identity.
        if path.is_dir() {
            if name == ".git" || name == ".lu-cache" || name == ".hg" || name == ".svn" {
                continue;
            }
            collect(root, &path, out)?;
        } else if is_identity_file(&path) {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(case: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wolf-pkg-lock-{}-{case}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn lock_round_trips_byte_stable() {
        let text = "\
# wolf.sum — integrity ledger. Do not edit; `wolf add/rm/update` rewrite it.
acme/redis 1.4.0 b3:aa11 caps=net
local/util 0.1.0 - caps=-
";
        let lock = Lock::parse(text).unwrap();
        assert_eq!(lock.render(), text);
        let again = Lock::parse(&lock.render()).unwrap();
        assert_eq!(again.render(), text);
    }

    #[test]
    fn lock_rejects_malformed_lines() {
        assert!(Lock::parse("just-a-name\n").is_err());
        assert!(Lock::parse("a 1.0.0 sha256:xx caps=-\n").is_err());
        assert!(Lock::parse("a 1.0.0 b3:xx caps=teleport\n").is_err());
    }

    #[test]
    fn tree_hash_ignores_vcs_noise_and_non_source() {
        let d = tmpdir("noise");
        std::fs::write(d.join("main.lu"), "fn main() -> !int { 0 }\n").unwrap();
        std::fs::write(d.join("wolf.pkg"), "pkg { name: \"a/b\" }\n").unwrap();
        let h1 = hash_tree(&d).unwrap();
        // Droppings that must not perturb identity.
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::write(d.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(d.join("README.md"), "prose\n").unwrap();
        std::fs::create_dir_all(d.join(".lu-cache")).unwrap();
        std::fs::write(d.join(".lu-cache/x.o"), "obj").unwrap();
        let h2 = hash_tree(&d).unwrap();
        assert_eq!(h1, h2);
        // A source edit is identity.
        std::fs::write(d.join("main.lu"), "fn main() -> !int { 1 }\n").unwrap();
        let h3 = hash_tree(&d).unwrap();
        assert_ne!(h1, h3);
        assert!(h1.starts_with("b3:"));
    }
}
