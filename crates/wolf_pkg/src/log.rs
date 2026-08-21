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
//! c15 registry later — only the fetch URL scheme changes.
//!
//! The Merkle half (s51 Target 4, the Go sumdb design adopted
//! wholesale): the log is an append-only sequence of record lines,
//! leaf-hashed RFC-6962-style (`H(0x00‖line)`, `H(0x01‖l‖r)`) over
//! blake3. A **tree head** (`log.head` beside the log file) pins
//! `{size, root}` under a signature so even the author cannot swap
//! bits under a tag; clients verify **inclusion** (this record is
//! under the signed root) and **consistency** (the new head extends
//! the old one append-only). The head's signature field carries an
//! algorithm tag: v1 ships `b3k:` (keyed blake3 — the static log
//! maintainer's key, sufficient to catch mirror tampering for
//! keyholders and to pin the FORMAT); an asymmetric scheme is a c15
//! decision and slots into the same field without a format change.
//!
//! OPERATOR NOTE (D48): `b3k:` is a MAC, not a public signature —
//! anyone holding the verify key can also MINT heads. Hand the key
//! only to parties you would trust to sign, and do not present a v1
//! head to third parties as a trust root; public verifiability
//! arrives with c15's asymmetric tag.

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

// ------------------------------------------------------------ merkle --

/// RFC-6962-shaped leaf hash over one record line (no trailing
/// newline): `blake3(0x00 ‖ line)`.
fn leaf_hash(line: &str) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(&[0x00]);
    h.update(line.as_bytes());
    *h.finalize().as_bytes()
}

/// Interior node: `blake3(0x01 ‖ left ‖ right)`.
fn node_hash(l: &[u8; 32], r: &[u8; 32]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(&[0x01]);
    h.update(l);
    h.update(r);
    *h.finalize().as_bytes()
}

fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 {
        return Err(format!("bad hash length {}", s.len()));
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16)
            .map_err(|_| format!("bad hex in `{s}`"))?;
    }
    Ok(out)
}

/// Largest power of two STRICTLY less than `n` (n ≥ 2).
fn split_point(n: usize) -> usize {
    let mut k = 1usize;
    while k * 2 < n {
        k *= 2;
    }
    k
}

/// The record lines of a log file, trimmed, comments/blanks skipped —
/// the leaf sequence every Merkle computation runs over.
pub fn read_records(log_file: &Path) -> Result<Vec<String>, String> {
    let text = match std::fs::read_to_string(log_file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("read {}: {e}", log_file.display())),
    };
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect())
}

/// MTH over `lines[lo..hi]` (RFC 6962 §2.1). Empty range is the hash
/// of the empty string by convention; no head is ever issued over it.
fn mth(lines: &[String], lo: usize, hi: usize) -> [u8; 32] {
    match hi - lo {
        0 => *blake3::hash(b"").as_bytes(),
        1 => leaf_hash(&lines[lo]),
        n => {
            let k = split_point(n);
            let l = mth(lines, lo, lo + k);
            let r = mth(lines, lo + k, hi);
            node_hash(&l, &r)
        }
    }
}

/// The Merkle root over a full record sequence.
pub fn tree_root(lines: &[String]) -> String {
    hex(&mth(lines, 0, lines.len()))
}

/// Inclusion proof for `lines[index]` (RFC 6962 PATH, sibling hashes
/// leaf-to-root order).
pub fn inclusion_proof(lines: &[String], index: usize) -> Result<Vec<String>, String> {
    if index >= lines.len() {
        return Err(format!(
            "index {index} out of range for a log of {}",
            lines.len()
        ));
    }
    fn path(lines: &[String], m: usize, lo: usize, hi: usize, out: &mut Vec<String>) {
        let n = hi - lo;
        if n == 1 {
            return;
        }
        let k = split_point(n);
        if m < k {
            path(lines, m, lo, lo + k, out);
            out.push(hex(&mth(lines, lo + k, hi)));
        } else {
            path(lines, m - k, lo + k, hi, out);
            out.push(hex(&mth(lines, lo, lo + k)));
        }
    }
    let mut out = Vec::new();
    path(lines, index, 0, lines.len(), &mut out);
    Ok(out)
}

/// Verify an inclusion proof: does `line` at `index` in a log of
/// `size` roll up to `root`?
pub fn verify_inclusion(
    line: &str,
    index: usize,
    size: usize,
    proof: &[String],
    root: &str,
) -> Result<(), String> {
    if index >= size {
        return Err(format!("index {index} out of range for size {size}"));
    }
    fn roll(m: usize, n: usize, leaf: [u8; 32], proof: &[String]) -> Result<[u8; 32], String> {
        if n == 1 {
            return if proof.is_empty() {
                Ok(leaf)
            } else {
                Err("proof longer than the tree is deep".to_string())
            };
        }
        let Some((last, rest)) = proof.split_last() else {
            return Err("proof shorter than the tree is deep".to_string());
        };
        let sib = unhex(last)?;
        let k = split_point(n);
        if m < k {
            Ok(node_hash(&roll(m, k, leaf, rest)?, &sib))
        } else {
            Ok(node_hash(&sib, &roll(m - k, n - k, leaf, rest)?))
        }
    }
    let got = hex(&roll(index, size, leaf_hash(line), proof)?);
    if got == root {
        Ok(())
    } else {
        Err(format!(
            "inclusion proof does not reach the signed root (got {got}, head says {root})"
        ))
    }
}

/// Consistency proof that the log of size `n` extends the log that had
/// size `m` (RFC 6962 PROOF/SUBPROOF).
pub fn consistency_proof(lines: &[String], m: usize) -> Result<Vec<String>, String> {
    let n = lines.len();
    if m == 0 || m > n {
        return Err(format!("no consistency proof from size {m} to {n}"));
    }
    fn sub(lines: &[String], m: usize, lo: usize, hi: usize, b: bool, out: &mut Vec<String>) {
        let n = hi - lo;
        if m == n {
            if !b {
                out.push(hex(&mth(lines, lo, hi)));
            }
            return;
        }
        let k = split_point(n);
        if m <= k {
            sub(lines, m, lo, lo + k, b, out);
            out.push(hex(&mth(lines, lo + k, hi)));
        } else {
            sub(lines, m - k, lo + k, hi, false, out);
            out.push(hex(&mth(lines, lo, lo + k)));
        }
    }
    let mut out = Vec::new();
    sub(lines, m, 0, n, true, &mut out);
    Ok(out)
}

/// Verify a consistency proof between two heads. The old root appears
/// in the reconstruction wherever the old tree survives as a whole
/// subtree; both reconstructed roots must match their heads.
pub fn verify_consistency(
    m: usize,
    n: usize,
    old_root: &str,
    new_root: &str,
    proof: &[String],
) -> Result<(), String> {
    if m == 0 || m > n {
        return Err(format!("no consistency between sizes {m} and {n}"));
    }
    if m == n {
        return if proof.is_empty() && old_root == new_root {
            Ok(())
        } else {
            Err("same-size heads must be identical with an empty proof".to_string())
        };
    }
    /// Reconstruct (old subtree root, new subtree root); `b` marks the
    /// SUBPROOF branch where the old tree is still a whole subtree and
    /// its root is the verifier's own `old` head, not proof material.
    fn walk(
        m: usize,
        n: usize,
        proof: &[String],
        b: bool,
        old: &[u8; 32],
    ) -> Result<([u8; 32], [u8; 32]), String> {
        if m == n {
            return if b {
                if proof.is_empty() {
                    Ok((*old, *old))
                } else {
                    Err("consistency proof longer than needed".to_string())
                }
            } else {
                let [one] = proof else {
                    return Err("aligned subtree wants exactly one hash".to_string());
                };
                let h = unhex(one)?;
                Ok((h, h))
            };
        }
        let Some((last, rest)) = proof.split_last() else {
            return Err("consistency proof shorter than needed".to_string());
        };
        let sib = unhex(last)?;
        let k = split_point(n);
        if m <= k {
            // The old tree lives wholly in the left subtree; `sib` is
            // the NEW right subtree, absent from the old root.
            let (o, nw) = walk(m, k, rest, b, old)?;
            Ok((o, node_hash(&nw, &sib)))
        } else {
            // The left subtree (size k) is shared verbatim: `sib` is
            // its root in BOTH trees.
            let (o, nw) = walk(m - k, n - k, rest, false, old)?;
            Ok((node_hash(&sib, &o), node_hash(&sib, &nw)))
        }
    }
    let old_h = unhex(old_root)?;
    let (got_old, got_new) = walk(m, n, proof, true, &old_h)?;
    if hex(&got_old) != old_root {
        return Err(format!(
            "consistency proof does not reproduce the OLD head (got {})",
            hex(&got_old)
        ));
    }
    if hex(&got_new) != new_root {
        return Err(format!(
            "consistency proof does not reproduce the NEW head (got {})",
            hex(&got_new)
        ));
    }
    Ok(())
}

// --------------------------------------------------------- tree head --

/// The signed head: `head size=N root=<hex> sig=<alg>:<hex>` — one
/// line, stored beside the log (`log.head`). The signature covers
/// `"wolf-log-head\n" ‖ size ‖ "\n" ‖ root`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TreeHead {
    pub size: usize,
    pub root: String,
    pub sig: String,
}

fn head_payload(size: usize, root: &str) -> Vec<u8> {
    format!("wolf-log-head\n{size}\n{root}").into_bytes()
}

/// v1 signature: keyed blake3 under the static log maintainer's key,
/// tagged `b3k:`. The TAG is the frozen surface; c15's asymmetric
/// scheme changes the tag, not the head format.
pub fn sign_head(size: usize, root: &str, key: &[u8; 32]) -> String {
    let mut h = blake3::Hasher::new_keyed(key);
    h.update(&head_payload(size, root));
    format!("b3k:{}", hex(h.finalize().as_bytes()))
}

impl TreeHead {
    pub fn over(lines: &[String], key: &[u8; 32]) -> TreeHead {
        let root = tree_root(lines);
        let sig = sign_head(lines.len(), &root, key);
        TreeHead {
            size: lines.len(),
            root,
            sig,
        }
    }

    pub fn render(&self) -> String {
        format!(
            "head size={} root={} sig={}\n",
            self.size, self.root, self.sig
        )
    }

    pub fn parse(text: &str) -> Result<TreeHead, String> {
        let line = text.trim();
        let mut parts = line.split_whitespace();
        if parts.next() != Some("head") {
            return Err("head file must start with `head`".to_string());
        }
        let (mut size, mut root, mut sig) = (None, None, None);
        for p in parts {
            if let Some(v) = p.strip_prefix("size=") {
                size = Some(v.parse::<usize>().map_err(|_| format!("bad size `{v}`"))?);
            } else if let Some(v) = p.strip_prefix("root=") {
                root = Some(v.to_string());
            } else if let Some(v) = p.strip_prefix("sig=") {
                sig = Some(v.to_string());
            } else {
                return Err(format!("unknown head field `{p}`"));
            }
        }
        Ok(TreeHead {
            size: size.ok_or("head missing size=")?,
            root: root.ok_or("head missing root=")?,
            sig: sig.ok_or("head missing sig=")?,
        })
    }

    /// Verify the signature. Only `b3k:` verifies at v1; an unknown
    /// algorithm tag is an ERROR, never a silent pass.
    pub fn verify_sig(&self, key: &[u8; 32]) -> Result<(), String> {
        let Some(_hex) = self.sig.strip_prefix("b3k:") else {
            let alg = self.sig.split(':').next().unwrap_or("?");
            return Err(format!(
                "head signed with `{alg}`, which this toolchain cannot verify"
            ));
        };
        let want = sign_head(self.size, &self.root, key);
        if want == self.sig {
            Ok(())
        } else {
            Err("head signature does not verify under the log key".to_string())
        }
    }
}

/// The head file beside a log file.
pub fn head_path(log_file: &Path) -> std::path::PathBuf {
    let mut name = log_file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "log".to_string());
    name.push_str(".head");
    log_file.with_file_name(name)
}

// ------------------------------------------------------ client verify --

/// Verify a project's store-backed packages against a static log
/// (v1 transport: `<dir>/log` + `<dir>/log.head`, dumb files —
/// servable from object storage; the c15 registry changes only how
/// these bytes are fetched, which is the X7 gate).
///
/// Opt-in and strict where it applies: a package ABSENT from the log
/// is not an error (unpublished things exist); a package PRESENT must
/// match its record's tree hash AND carry a valid inclusion proof
/// under the head — and the head must verify under `key` when one is
/// given. A log file without a head is refused: an unheaded log
/// proves nothing.
pub fn verify_project_against_log(
    project: &crate::project::Project,
    log_dir: &Path,
    key: Option<&[u8; 32]>,
) -> Vec<String> {
    let mut errs = Vec::new();
    let log_file = log_dir.join("log");
    let lines = match read_records(&log_file) {
        Ok(l) => l,
        Err(e) => return vec![e],
    };
    if lines.is_empty() {
        return errs;
    }
    let head = match std::fs::read_to_string(head_path(&log_file)) {
        Ok(t) => match TreeHead::parse(&t) {
            Ok(h) => h,
            Err(e) => return vec![format!("{}: {e}", head_path(&log_file).display())],
        },
        Err(e) => {
            return vec![format!(
                "the log has records but no head ({}: {e}) — an unheaded log proves nothing",
                head_path(&log_file).display()
            )];
        }
    };
    if let Some(k) = key
        && let Err(e) = head.verify_sig(k)
    {
        return vec![e];
    }
    if head.size > lines.len() {
        return vec![format!(
            "the head signs {} record(s) but the log holds {} — a truncated mirror",
            head.size,
            lines.len()
        )];
    }
    for p in project.pkgs.iter().skip(1) {
        if p.hash.is_none() || p.is_std {
            continue;
        }
        let key_str = format!("{}@{}", p.name, p.version);
        let Some((idx, line)) = lines
            .iter()
            .enumerate()
            .take(head.size)
            .find(|(_, l)| l.starts_with(&format!("{key_str} ")))
        else {
            continue; // unpublished: the log has nothing to say
        };
        let rec = match LogRecord::parse(line) {
            Ok(r) => r,
            Err(e) => {
                errs.push(format!("log record for `{key_str}`: {e}"));
                continue;
            }
        };
        if Some(&rec.tree) != p.hash.as_ref() {
            errs.push(format!(
                "`{key_str}`: the fetched tree hashes {}, but the transparency log                  records {} — the bits are not the published bits",
                p.hash.as_deref().unwrap_or("?"),
                rec.tree
            ));
            continue;
        }
        match inclusion_proof(&lines[..head.size], idx) {
            Ok(proof) => {
                if let Err(e) = verify_inclusion(line, idx, head.size, &proof, &head.root) {
                    errs.push(format!("`{key_str}`: {e}"));
                }
            }
            Err(e) => errs.push(format!("`{key_str}`: {e}")),
        }
    }
    errs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(n: usize) -> Vec<String> {
        (0..n)
            .map(|i| {
                LogRecord {
                    key: format!("a/p@{i}.0.0"),
                    tree: hash_bytes(format!("t{i}").as_bytes()),
                    manifest: hash_bytes(format!("m{i}").as_bytes()),
                    interface: hash_bytes(format!("i{i}").as_bytes()),
                }
                .render()
                .trim()
                .to_string()
            })
            .collect()
    }

    /// Every (size, index) pair up to 8: the proof verifies, and the
    /// same proof against a tampered line or wrong index fails.
    #[test]
    fn inclusion_holds_and_tampering_fails_exhaustively() {
        for n in 1..=8usize {
            let ls = lines(n);
            let root = tree_root(&ls);
            for i in 0..n {
                let proof = inclusion_proof(&ls, i).unwrap();
                verify_inclusion(&ls[i], i, n, &proof, &root).unwrap();
                let err = verify_inclusion(
                    "a/evil@9.9.9 tree=b3:00 manifest=b3:00 interface=b3:00",
                    i,
                    n,
                    &proof,
                    &root,
                );
                assert!(err.is_err(), "tampered line verified at n={n} i={i}");
                if n > 1 {
                    let wrong = (i + 1) % n;
                    assert!(
                        verify_inclusion(&ls[i], wrong, n, &proof, &root).is_err(),
                        "wrong index verified at n={n} i={i}"
                    );
                }
            }
        }
    }

    /// Every m < n up to 8: consistency verifies; a forked old head
    /// fails; truncated proofs fail.
    #[test]
    fn consistency_holds_and_forks_fail_exhaustively() {
        for n in 1..=8usize {
            let ls = lines(n);
            let new_root = tree_root(&ls);
            for m in 1..=n {
                let old_root = tree_root(&ls[..m]);
                let proof = consistency_proof(&ls, m).unwrap();
                verify_consistency(m, n, &old_root, &new_root, &proof).unwrap();
                // A fork: an old head over DIFFERENT first m records.
                let mut forked = ls[..m].to_vec();
                forked[m - 1] = "z/z@0.0.1 tree=b3:ff manifest=b3:ff interface=b3:ff".to_string();
                let bad = tree_root(&forked);
                if bad != old_root {
                    assert!(
                        verify_consistency(m, n, &bad, &new_root, &proof).is_err(),
                        "forked head verified at m={m} n={n}"
                    );
                }
                if !proof.is_empty() {
                    assert!(
                        verify_consistency(m, n, &old_root, &new_root, &proof[..proof.len() - 1])
                            .is_err(),
                        "truncated proof verified at m={m} n={n}"
                    );
                }
            }
        }
    }

    #[test]
    fn head_signs_parses_and_refuses_unknown_algs() {
        let ls = lines(3);
        let key = [7u8; 32];
        let head = TreeHead::over(&ls, &key);
        let parsed = TreeHead::parse(&head.render()).unwrap();
        assert_eq!(parsed, head);
        parsed.verify_sig(&key).unwrap();
        assert!(parsed.verify_sig(&[8u8; 32]).is_err(), "wrong key verified");
        let alien = TreeHead {
            sig: "ed25519:00".to_string(),
            ..head
        };
        let err = alien.verify_sig(&key).unwrap_err();
        assert!(err.contains("cannot verify"), "{err}");
    }

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
