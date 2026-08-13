//! Script mode's manifest: the frontmatter that rides in a `//!` block
//! (s53). "A script is a first-class package whose manifest rides in
//! the module doc comment."
//!
//! One schema, one parser, one set of diagnostics — the cargo-script
//! RFC 3502 lesson, taken seriously. This module does not parse
//! anything: it finds the `pkg { … }` literal inside the leading `//!`
//! block and hands [`crate::manifest::parse`] a **shadow text** — a
//! byte-for-byte-length copy of the script in which every byte outside
//! the manifest literal is a space (newlines kept, so lines and columns
//! survive). The manifest parser therefore reports spans that point at
//! the REAL FILE, at the real line and column, with the real source
//! line rendered under the caret. No second grammar, no re-based
//! offsets, no "error in <frontmatter>" fiction.
//!
//! The subset is enforced here rather than in the schema, because
//! `wolf.pkg` legitimately carries keys a single file cannot mean:
//! a script's identity is its path, so `name`/`version`/`fingerprint`
//! have nothing to identify; target-scoped dep sections and C recipes
//! belong to a package that has outgrown one file (E1507's fix-it says
//! so, and names the verb that performs the promotion).

use std::path::Path;

use wolf_diag::{Diagnostic, codes};
use wolf_span::{FileId, Span};

use crate::manifest::{self, Manifest};

/// The frontmatter keys a script may carry (s53 §3).
pub const SCRIPT_KEYS: &[&str] = &["edition", "wolf", "deps", "features", "capabilities"];

/// A script's frontmatter, located in its own text.
#[derive(Clone, Debug)]
pub struct Frontmatter {
    /// Byte range of the `pkg { … }` literal within the script text
    /// (the payload bytes, comment markers already excluded per line).
    pub lo: u32,
    pub hi: u32,
    /// The manifest-literal source with `//!` markers stripped — the
    /// bytes the script identity is keyed on.
    pub literal: String,
}

/// The leading `//!` block of a script, as (payload, byte-range) pairs
/// per line. Stops at the first line that is not `//!`-shaped and not
/// blank-before-the-block; a `#!` first line is skipped (trivia).
fn inner_doc_lines(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    let mut started = false;
    for line in text.split_inclusive('\n') {
        let len = line.len();
        let trimmed = line.trim_end_matches(['\n', '\r']);
        let t = trimmed.trim_start();
        let indent = trimmed.len() - t.len();
        if !started && (t.is_empty() || t.starts_with("#!")) {
            at += len;
            continue;
        }
        if let Some(rest) = t.strip_prefix("//!") {
            started = true;
            // Payload begins after the marker, and after ONE space.
            let mut payload_lo = at + indent + 3;
            if rest.starts_with(' ') {
                payload_lo += 1;
            }
            out.push((payload_lo, at + trimmed.len()));
            at += len;
            continue;
        }
        break;
    }
    out
}

/// Locate the `pkg { … }` literal inside a script's leading `//!`
/// block. `None` when there is none — a script with no frontmatter is
/// a std-only script, which is not an error but the common case.
pub fn find(text: &str) -> Option<Frontmatter> {
    let lines = inner_doc_lines(text);
    if lines.is_empty() {
        return None;
    }
    // Find the line whose payload opens the literal: `pkg` then `{`.
    let mut start: Option<usize> = None;
    let mut depth = 0i32;
    let mut end: Option<usize> = None;
    for &(lo, hi) in &lines {
        let payload = &text[lo..hi];
        if start.is_none() {
            let t = payload.trim_start();
            let Some(rest) = t.strip_prefix("pkg") else {
                continue;
            };
            if !rest.trim_start().starts_with('{') {
                continue;
            }
            start = Some(lo + (payload.len() - t.len()));
        }
        // Brace depth over the payload; strings in a manifest never
        // contain braces that are not escaped, and interpolation is
        // refused outright, so counting bytes is honest here.
        let mut in_str = false;
        let mut escaped = false;
        for (i, b) in payload.bytes().enumerate() {
            if escaped {
                escaped = false;
                continue;
            }
            match b {
                b'\\' if in_str => escaped = true,
                b'"' => in_str = !in_str,
                b'{' if !in_str => depth += 1,
                b'}' if !in_str => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(lo + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        if end.is_some() {
            break;
        }
    }
    let (lo, hi) = (start?, end?);
    // The literal, markers stripped, for the identity hash.
    let mut literal = String::new();
    for &(plo, phi) in &lines {
        let a = plo.max(lo);
        let b = phi.min(hi);
        if a < b {
            literal.push_str(&text[a..b]);
            literal.push('\n');
        }
    }
    Some(Frontmatter {
        lo: lo as u32,
        hi: hi as u32,
        literal,
    })
}

/// Build the offset-preserving shadow text: the manifest literal in
/// place, everything else whitespace. Line breaks are preserved so a
/// diagnostic's line/column and rendered source line are the script's
/// own.
fn shadow(text: &str, fm: &Frontmatter) -> String {
    let lines = inner_doc_lines(text);
    let mut keep = vec![false; text.len()];
    for &(plo, phi) in &lines {
        let a = plo.max(fm.lo as usize);
        let b = phi.min(fm.hi as usize);
        if a < b {
            keep[a..b].fill(true);
        }
    }
    let mut out = String::with_capacity(text.len());
    for (i, ch) in text.char_indices() {
        if keep[i] {
            out.push(ch);
        } else if ch == '\n' {
            out.push('\n');
        } else {
            // One space per BYTE, so every later offset is unmoved even
            // when the replaced character was multi-byte.
            for _ in 0..ch.len_utf8() {
                out.push(' ');
            }
        }
    }
    out
}

/// The result of reading a script's frontmatter.
pub struct Script {
    /// `None` when the script carries no frontmatter (std-only) or the
    /// frontmatter was refused.
    pub manifest: Option<Manifest>,
    /// True when a `pkg { … }` literal was present at all — the
    /// difference between "std-only script" and "broken frontmatter".
    pub has_frontmatter: bool,
    pub frontmatter: Option<Frontmatter>,
    pub diagnostics: Vec<Diagnostic>,
}

fn e1507(span: Span, msg: impl Into<String>) -> Diagnostic {
    Diagnostic::error(codes::E1507, span, msg).with_note(
        "a script's frontmatter carries `edition`, `wolf`, `deps`, `features` and \
         `capabilities` — nothing else. `wolf init --from-script` promotes the script \
         to a package with a real manifest, dependency entries kept verbatim."
            .to_string(),
    )
}

/// Read the frontmatter of `text` (the whole script), reporting every
/// diagnostic against `file` at the script's own spans.
pub fn read(file: FileId, text: &str) -> Script {
    let Some(fm) = find(text) else {
        return Script {
            manifest: None,
            has_frontmatter: false,
            frontmatter: None,
            diagnostics: Vec::new(),
        };
    };
    let shadow_text = shadow(text, &fm);
    debug_assert_eq!(shadow_text.len(), text.len(), "shadow text moved offsets");
    // A script has no `name:` to state — its identity is its path — so
    // the schema's one knob is turned off and the name is synthetic.
    // Writing `name:` anyway is out of subset (E1507, below).
    let (mut manifest, mut diags) = manifest::parse_opts(
        file,
        &shadow_text,
        &manifest::ParseOpts {
            require_name: false,
            default_name: "script/local".to_string(),
        },
    );
    // The subset gate, on the accepted shape.
    if let Some(m) = &manifest {
        for (key, span) in out_of_subset(&shadow_text, m) {
            diags.push(e1507(
                span,
                format!("`{key}` is not a key a script's frontmatter may carry"),
            ));
        }
    }
    if diags
        .iter()
        .any(|d| d.severity == wolf_diag::Severity::Error)
    {
        manifest = None;
    }
    Script {
        manifest,
        has_frontmatter: true,
        frontmatter: Some(fm),
        diagnostics: diags,
    }
}

/// Top-level keys of the parsed frontmatter that a script may not
/// carry, with their spans. Re-reads the shadow text's key tokens,
/// because [`Manifest`] normalizes keys away.
fn out_of_subset(shadow_text: &str, m: &Manifest) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    // Only the keys the manifest actually recorded can be checked by
    // value; for the rest, scan the literal's own top level.
    let block = &shadow_text[m.span.lo as usize..m.span.hi as usize];
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    let mut word_start: Option<usize> = None;
    let bytes = block.as_bytes();
    for i in 0..bytes.len() {
        let b = bytes[i];
        if escaped {
            escaped = false;
            continue;
        }
        match b {
            b'\\' if in_str => escaped = true,
            b'"' => in_str = !in_str,
            _ if in_str => {}
            b'{' => {
                depth += 1;
                word_start = None;
            }
            b'}' => {
                depth -= 1;
                word_start = None;
            }
            b'_' | b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' if depth == 1 => {
                if word_start.is_none() {
                    word_start = Some(i);
                }
            }
            b':' if depth == 1 => {
                if let Some(ws) = word_start.take() {
                    let key = &block[ws..i];
                    if !SCRIPT_KEYS.contains(&key) {
                        out.push((
                            key.to_string(),
                            Span::new(m.span.file, m.span.lo + ws as u32, m.span.lo + i as u32),
                        ));
                    }
                }
            }
            _ => word_start = None,
        }
    }
    out
}

/// The `--locked` refusal (s53 §4): the frontmatter changed since this
/// script's resolution was pinned. `span` is the frontmatter block —
/// the thing that moved.
pub fn drift_diagnostic(span: Span) -> Diagnostic {
    Diagnostic::error(
        codes::E1508,
        span,
        "this script's frontmatter changed since its resolution was pinned",
    )
    .with_label("the pinned resolution answers a different manifest")
    .with_note(
        "`--locked` asserts the pin still answers the frontmatter, which is the \
         posture CI wants. Run without `--locked`, or with `--update`, to \
         re-resolve and re-pin. Nothing in the cache was modified."
            .to_string(),
    )
}

/// A script's cache identity: `hash(absolute path, frontmatter)` —
/// s53 §4. Editing the frontmatter changes the id, so a clean
/// re-resolve happens by construction and no pin is ever mutated in
/// place. The path is included because two scripts may carry identical
/// frontmatter and different `path:` deps resolve relative to them.
pub fn script_id(abs_path: &Path, frontmatter: Option<&Frontmatter>) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"wolf-script/0\n");
    h.update(abs_path.to_string_lossy().as_bytes());
    h.update(b"\n");
    h.update(
        frontmatter
            .map(|f| f.literal.as_str())
            .unwrap_or("")
            .as_bytes(),
    );
    h.finalize().to_hex()[..32].to_string()
}
