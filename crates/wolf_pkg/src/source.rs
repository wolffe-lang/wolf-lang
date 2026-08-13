//! Dependency sources (s51): the content-addressed store and the v1
//! transports — `path` and `git` (X7: the registry stays a stub whose
//! entry *form* is stable; fetching it is c15).

use std::path::{Path, PathBuf};

use crate::lock;

/// The global content-addressed store: fetched packages land here,
/// immutable, keyed by their tree hash — per-project mutable
/// environments are the disease (reports/07 §Needs work #9).
///
/// `WOLF_STORE` overrides (tests, hermetic CI); otherwise the user
/// cache directory: `$XDG_CACHE_HOME/wolf/store`, `~/.cache/wolf/store`
/// (unix), `%LOCALAPPDATA%\wolf\store` (windows).
pub fn store_dir() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("WOLF_STORE").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    Ok(cache_root()?.join("store"))
}

/// The one cache root everything lives under (s53's spelled-out
/// layout). `WOLF_CACHE` overrides; otherwise the XDG-respecting user
/// cache directory:
///
/// ```text
/// ~/.cache/wolf/
/// ├── store/<multihash>/   package sources — immutable (s51)
/// ├── log/                 cached transparency-log heads + proofs
/// ├── builds/<key>/        artifacts, shared with project builds
/// └── scripts/<id>/        script-mode pin: wolf.sum + resolution
/// ```
///
/// Nothing here is ever mutated in place; `wolf cache gc` is the only
/// deletion.
pub fn cache_root() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("WOLF_CACHE").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    if let Some(p) = std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(p).join("wolf"));
    }
    if let Some(p) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(p).join(".cache/wolf"));
    }
    if let Some(p) = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(p).join("wolf"));
    }
    Err("no cache directory (set WOLF_CACHE, XDG_CACHE_HOME, HOME, or LOCALAPPDATA)".to_string())
}

/// `<cache>/scripts/<script-id>/` — a script's pinned resolution.
pub fn script_dir(id: &str) -> Result<PathBuf, String> {
    Ok(cache_root()?.join("scripts").join(id))
}

/// `<cache>/scripts/by-path/<hash(path)>` — the pointer from a script's
/// PATH to the script-id its last resolution used.
///
/// Two clauses of s53 §4 need both keys, and they are not the same key.
/// "Editing frontmatter changes script-id ⇒ clean re-resolve" means the
/// resolution directory is keyed on the frontmatter, so a pin is never
/// edited in place. "`--locked` refuses if the frontmatter drifted from
/// the pinned resolution" means something must remember what the pin
/// WAS for this file — which is path-keyed by construction, since the
/// frontmatter is exactly what changed.
pub fn script_pointer(abs_path: &Path) -> Result<PathBuf, String> {
    let key = blake3::hash(abs_path.to_string_lossy().as_bytes()).to_hex()[..32].to_string();
    Ok(cache_root()?.join("scripts").join("by-path").join(key))
}

/// `<cache>/builds/` — compiled artifacts, shared with project builds
/// so a dependency cached by any project makes every script using it
/// build without fetching.
pub fn builds_dir() -> Result<PathBuf, String> {
    Ok(cache_root()?.join("builds"))
}

/// The store path of a content address (`b3:<hex>` → `<store>/<hex>`).
pub fn store_path(store: &Path, hash: &str) -> PathBuf {
    store.join(hash.trim_start_matches("b3:"))
}

/// Ensure a git dependency's tree is in the store; return its root and
/// content address.
///
/// - With a ledger pin whose store entry exists (and no `refresh`):
///   offline, no VCS involved — the store is the truth.
/// - Otherwise: `git clone --depth 1 --branch <tag>` into a temp dir,
///   strip `.git`, hash the tree, move it under its address. If a pin
///   was expected and the fetched bits hash differently, that is the
///   E1506 alarm — the caller refuses the build.
pub fn ensure_git(
    store: &Path,
    url: &str,
    tag: &str,
    pinned: Option<&str>,
    refresh: bool,
) -> Result<(PathBuf, String), String> {
    if let Some(pin) = pinned
        && !refresh
    {
        let at = store_path(store, pin);
        if at.is_dir() {
            // Every build re-derives the address: the store is
            // immutable by contract, and a mutated tree is the E1506
            // alarm, not a cache hit.
            let actual = lock::hash_tree(&at)?;
            if actual != pin {
                return Err(format!(
                    "{url} (tag {tag}): the store tree hashes {actual}, wolf.sum pins {pin} — \
                     the bits are not the bits the ledger witnessed"
                ));
            }
            return Ok((at, pin.to_string()));
        }
    }
    // Fetch. Deterministic temp name per (pid, url-hash) so parallel
    // fetches of different deps never collide.
    let tmp = store.join(format!(
        "fetch-{}-{}",
        std::process::id(),
        blake3::hash(format!("{url}#{tag}").as_bytes()).to_hex()
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(store).map_err(|e| format!("create {}: {e}", store.display()))?;
    let out = std::process::Command::new("git")
        .args(["clone", "--quiet", "--depth", "1", "--branch", tag, url])
        .arg(&tmp)
        .output()
        .map_err(|e| format!("cannot run `git`: {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!(
            "git clone of {url} (tag {tag}) failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let _ = std::fs::remove_dir_all(tmp.join(".git"));
    let hash = lock::hash_tree(&tmp)?;
    if let Some(pin) = pinned
        && pin != hash
        && !refresh
    {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!(
            "{url} (tag {tag}): fetched tree hashes {hash}, wolf.sum pins {pin} — \
             the bits are not the bits the ledger witnessed"
        ));
    }
    let dest = store_path(store, &hash);
    if dest.is_dir() {
        let _ = std::fs::remove_dir_all(&tmp);
    } else {
        std::fs::rename(&tmp, &dest).map_err(|e| format!("move into store: {e}"))?;
    }
    Ok((dest, hash))
}
