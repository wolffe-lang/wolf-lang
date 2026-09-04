//! The human CLI renderer — RFC-1644 layout, Elm voice (`VOICE.md`).
//!
//! Layout, per diagnostic:
//!
//! ```text
//! error[E0104]: this line starts left of the string's margin
//!   --> src/main.lu:3:1
//!    |
//!  3 |   bad
//!    |  ^^ the line starts here
//!  5 |     """
//!    |  ---- the closing `"""` sets the margin here
//!    |
//!    = note: indent every content line at least as far as the closing `"""`.
//! help: indent this line to the margin
//!    |
//!  3 |     bad
//!    |
//! ```
//!
//! Handled here: multi-line spans (start-line underline runs to the end
//! of the line, end-line underline carries the label; long spans elide
//! their middle with a `...` gutter row), secondary spans in *other
//! files* (their own `:::` locus block — the sema shape), spans inside
//! f-string interpolations (byte-exact via s07 fragment spans — nothing
//! special, which is the point), tab expansion (tabs render as 4-column
//! stops; underlines stay aligned), and width-aware truncation (long
//! lines are windowed around the annotation with `…` at the cut edges;
//! a second annotation on the same line that falls past a cut marks the
//! cut and keeps its label — see `render_line`).
//!
//! ANSI color is behind [`RenderOptions::color`], default off; the
//! snapshot renderer is exactly this renderer with color off, so
//! snapshots are always plain.

use std::collections::HashMap;

use crate::{Diagnostic, Severity, Suggestion};
use wolf_span::{FileId, LineIndex, Span};

/// Rendering knobs. `width` is the maximum source-line width in display
/// columns before the renderer windows the line around the annotation.
#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub color: bool,
    pub width: usize,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            color: false,
            width: 100,
        }
    }
}

struct Entry {
    name: String,
    src: Vec<u8>,
    index: LineIndex,
}

/// The renderer's view of the sources: register each file once, render
/// any number of diagnostics against it.
#[derive(Default)]
pub struct Sources {
    files: HashMap<FileId, Entry>,
}

impl Sources {
    pub fn new() -> Sources {
        Sources::default()
    }

    pub fn add(&mut self, file: FileId, name: impl Into<String>, src: &[u8]) {
        self.files.insert(
            file,
            Entry {
                name: name.into(),
                src: src.to_vec(),
                index: LineIndex::new(src),
            },
        );
    }

    fn get(&self, file: FileId) -> Option<&Entry> {
        self.files.get(&file)
    }
}

// ------------------------------------------------------------- palette --

struct Palette {
    accent: &'static str, // severity color (red / yellow)
    gutter: &'static str, // line numbers, pipes, locus arrow
    bold: &'static str,
    reset: &'static str,
}

impl Palette {
    fn new(color: bool, severity: Severity) -> Palette {
        if !color {
            return Palette {
                accent: "",
                gutter: "",
                bold: "",
                reset: "",
            };
        }
        Palette {
            accent: match severity {
                Severity::Error => "\x1b[31m",
                Severity::Warning => "\x1b[33m",
            },
            gutter: "\x1b[34m",
            bold: "\x1b[1m",
            reset: "\x1b[0m",
        }
    }
}

// ---------------------------------------------------------- annotations --

#[derive(Clone)]
struct Ann {
    span: Span,
    label: String,
    primary: bool,
}

/// Tab-expanded display text of a source line plus the byte→display
/// column mapping (index `i` is the display column of byte `i`; the
/// final entry is the line's display width). Tabs advance to the next
/// multiple of 4; multi-byte UTF-8 sequences count one column at their
/// first byte.
fn expand(line: &[u8]) -> (String, Vec<usize>) {
    let mut text = String::new();
    let mut map = Vec::with_capacity(line.len() + 1);
    let mut col = 0usize;
    let mut i = 0usize;
    while i < line.len() {
        let b = line[i];
        map.push(col);
        if b == b'\t' {
            let stop = 4 - col % 4;
            for _ in 0..stop {
                text.push(' ');
            }
            col += stop;
            i += 1;
        } else if b < 0x80 {
            text.push(if b == b'\r' { ' ' } else { b as char });
            col += 1;
            i += 1;
        } else {
            // One display column per decoded char; invalid bytes render
            // as one replacement char each.
            let chunk_end = line.len().min(i + 4);
            match std::str::from_utf8(&line[i..chunk_end]) {
                Ok(s) => {
                    let c = s.chars().next().unwrap_or('\u{fffd}');
                    for _ in 1..c.len_utf8() {
                        map.push(col);
                    }
                    text.push(c);
                    col += 1;
                    i += c.len_utf8();
                }
                Err(e) if e.valid_up_to() > 0 => {
                    let s = std::str::from_utf8(&line[i..i + e.valid_up_to()]).expect("valid");
                    let c = s.chars().next().expect("nonempty");
                    for _ in 1..c.len_utf8() {
                        map.push(col);
                    }
                    text.push(c);
                    col += 1;
                    i += c.len_utf8();
                }
                Err(_) => {
                    text.push('\u{fffd}');
                    col += 1;
                    i += 1;
                }
            }
        }
    }
    map.push(col);
    (text, map)
}

/// Window `text` (display chars) to `width` around `focus`, returning
/// the windowed text and the column shift applied. Cut edges render as
/// `…`.
fn window(text: &str, width: usize, focus: usize) -> (String, usize) {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return (text.to_string(), 0);
    }
    let start = focus
        .saturating_sub(width / 2)
        .min(chars.len().saturating_sub(width));
    let mut out: Vec<char> = chars[start..start + width].to_vec();
    if start > 0 {
        out[0] = '…';
    }
    if start + width < chars.len() {
        let last = out.len() - 1;
        out[last] = '…';
    }
    (out.into_iter().collect(), start)
}

// ------------------------------------------------------------ rendering --

/// Render one diagnostic in the human CLI format (see module docs).
/// This is also the snapshot format: pass `color: false` (the default)
/// and the output is plain text.
pub fn render_human(d: &Diagnostic, sources: &Sources, opts: &RenderOptions) -> String {
    let p = Palette::new(opts.color, d.severity);
    let mut out = String::new();

    // Header: severity[code]: message.
    out.push_str(&format!(
        "{}{}{}[{}]{}{}: {}{}\n",
        p.bold,
        p.accent,
        d.severity.as_str(),
        d.code,
        p.reset,
        p.bold,
        d.message,
        p.reset,
    ));

    // Group annotations by file: the primary span's file first, then
    // each other file in order of first appearance.
    let mut order: Vec<FileId> = vec![d.primary.span.file];
    let mut groups: HashMap<FileId, Vec<Ann>> = HashMap::new();
    groups.entry(d.primary.span.file).or_default().push(Ann {
        span: d.primary.span,
        label: d.primary.label.clone(),
        primary: true,
    });
    for s in &d.secondary {
        if !order.contains(&s.span.file) {
            order.push(s.span.file);
        }
        groups.entry(s.span.file).or_default().push(Ann {
            span: s.span,
            label: s.label.clone(),
            primary: false,
        });
    }

    // Gutter width: max displayed 1-based line number, all groups.
    let mut max_line = 1u32;
    for file in &order {
        if let Some(entry) = sources.get(*file) {
            for ann in &groups[file] {
                max_line = max_line.max(entry.index.line_of(end_offset(ann.span)) + 1);
            }
        }
    }
    let gw = max_line.to_string().len();

    for (gi, file) in order.iter().enumerate() {
        let anns = &groups[file];
        let Some(entry) = sources.get(*file) else {
            // Unregistered file: numeric locus, no code frame.
            out.push_str(&format!(
                "{:gw$}{}-->{} file#{} @ bytes {}..{}\n",
                "",
                p.gutter,
                p.reset,
                file.index(),
                anns[0].span.lo,
                anns[0].span.hi,
            ));
            continue;
        };
        render_group(&mut out, entry, anns, gi == 0, gw, opts, &p);
    }

    for note in &d.notes {
        let head = format!(
            "{:gw$} {}={} {}note{}: ",
            "", p.gutter, p.reset, p.bold, p.reset
        );
        let cont = format!("{:gw$}   ", "");
        out.push_str(&wrap_prose(note, &head, &cont, opts.width));
    }

    for sugg in &d.suggestions {
        render_suggestion(&mut out, d, sugg, sources, gw, opts, &p);
    }

    out
}

/// Wrap prose at `width` display columns onto `head` + `cont` prefixes
/// (word boundaries; a single overlong word stays whole).
fn wrap_prose(text: &str, head: &str, cont: &str, width: usize) -> String {
    let budget = width.saturating_sub(12).max(40);
    let mut out = String::new();
    let mut line = String::new();
    let mut first = true;
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > budget {
            out.push_str(if first { head } else { cont });
            out.push_str(&line);
            out.push('\n');
            line.clear();
            first = false;
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || first {
        out.push_str(if first { head } else { cont });
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// The last byte actually covered by `span` (zero-width spans point at
/// their own position).
fn end_offset(span: Span) -> u32 {
    if span.hi > span.lo {
        span.hi - 1
    } else {
        span.lo
    }
}

fn gutter_row(out: &mut String, gw: usize, p: &Palette) {
    out.push_str(&format!("{:gw$} {}|{}\n", "", p.gutter, p.reset));
}

#[allow(clippy::too_many_arguments)]
fn render_group(
    out: &mut String,
    entry: &Entry,
    anns: &[Ann],
    is_primary_group: bool,
    gw: usize,
    opts: &RenderOptions,
    p: &Palette,
) {
    let idx = &entry.index;
    // Locus: line:col of the group's leading annotation (the primary
    // span, or the file's first secondary), 1-based; col in characters.
    let head = anns.iter().find(|a| a.primary).unwrap_or(&anns[0]);
    let lc = idx.line_col(head.span.lo);
    let (ls, _le) = idx.line_range(lc.line);
    let col_chars = String::from_utf8_lossy(&entry.src[ls as usize..(ls + lc.col) as usize])
        .chars()
        .count();
    let arrow = if is_primary_group { "-->" } else { ":::" };
    out.push_str(&format!(
        "{:gw$}{}{}{} {}:{}:{}\n",
        "",
        p.gutter,
        arrow,
        p.reset,
        entry.name,
        lc.line + 1,
        col_chars + 1,
    ));
    gutter_row(out, gw, p);

    // Which lines to display: for each annotation its start and end
    // lines, plus the middle when the span is short; long spans elide.
    let mut lines: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut anns_sorted: Vec<&Ann> = anns.iter().collect();
    anns_sorted.sort_by_key(|a| (a.span.lo, a.span.hi));
    for ann in &anns_sorted {
        let start = idx.line_of(ann.span.lo);
        let end = idx.line_of(end_offset(ann.span));
        if end - start <= 4 {
            for l in start..=end {
                lines.insert(l);
            }
        } else {
            lines.insert(start);
            lines.insert(start + 1);
            lines.insert(end);
        }
    }

    let mut prev: Option<u32> = None;
    for &line in &lines {
        if let Some(pl) = prev {
            if line == pl + 2 {
                // A gap of exactly one line: just show it.
                render_line(out, entry, anns_sorted.as_slice(), pl + 1, gw, opts, p);
            } else if line > pl + 1 {
                out.push_str(&format!("{}...{}\n", p.gutter, p.reset));
            }
        }
        render_line(out, entry, anns_sorted.as_slice(), line, gw, opts, p);
        prev = Some(line);
    }
    gutter_row(out, gw, p);
}

fn render_line(
    out: &mut String,
    entry: &Entry,
    anns: &[&Ann],
    line: u32,
    gw: usize,
    opts: &RenderOptions,
    p: &Palette,
) {
    let idx = &entry.index;
    let (ls, le) = idx.line_range(line);
    let bytes = &entry.src[ls as usize..le as usize];
    let (text, map) = expand(bytes);

    // Underline rows this line owes: (disp_lo, disp_hi, label, primary).
    let mut rows: Vec<(usize, usize, String, bool)> = Vec::new();
    for ann in anns {
        let start_line = idx.line_of(ann.span.lo);
        let end_line = idx.line_of(end_offset(ann.span));
        if line < start_line || line > end_line {
            continue;
        }
        let width = |b: u32| -> usize {
            let rel = (b.max(ls).min(le) - ls) as usize;
            map[rel]
        };
        if start_line == end_line {
            let lo = width(ann.span.lo);
            let hi = width(ann.span.hi).max(lo + 1);
            rows.push((lo, hi, ann.label.clone(), ann.primary));
        } else if line == start_line {
            let lo = width(ann.span.lo);
            let hi = (*map.last().expect("map nonempty")).max(lo + 1);
            rows.push((lo, hi, String::new(), ann.primary));
        } else if line == end_line {
            let hi = width(ann.span.hi).max(1);
            rows.push((0, hi, ann.label.clone(), ann.primary));
        }
        // Middle lines of a multi-line span: shown, not underlined.
    }

    // Window the line around the first underline (or render whole).
    let focus = rows.first().map_or(0, |r| r.0);
    let (shown, shift) = window(&text, opts.width, focus);
    out.push_str(&format!(
        "{}{:>gw$} |{} {}\n",
        p.gutter,
        line + 1,
        p.reset,
        shown.trim_end(),
    ));
    for (lo, hi, label, primary) in rows {
        // BOTH ends clamp into the window (#238). `window` centres on
        // the FIRST row's start, and one group may point twice at one
        // long line — the annotation three hundred columns to the right
        // of the focus lands past the cut. `lo` used to keep its true
        // column while only `hi` was clamped to the width, so the caret
        // run was sized `hi - lo` with `hi` LEFT of `lo`: a `usize`
        // underflow, and `str::repeat` then asked for near-`usize::MAX`
        // bytes and aborted the process. A clipped row now marks the
        // cut edge — the `…` the window put there — and keeps its
        // label, which is the half a reader actually needs.
        let lo = lo.saturating_sub(shift).min(opts.width.saturating_sub(1));
        let hi = hi.saturating_sub(shift).min(opts.width).max(lo + 1);
        let mark = if primary { "^" } else { "-" };
        let color = if primary { p.accent } else { p.gutter };
        let mut row = format!(
            "{:gw$} {}|{} {:pad$}{}{}{}",
            "",
            p.gutter,
            p.reset,
            "",
            color,
            mark.repeat(hi - lo),
            p.reset,
            pad = lo,
        );
        if !label.is_empty() {
            row.push(' ');
            row.push_str(color);
            row.push_str(&label);
            row.push_str(p.reset);
        }
        out.push_str(&row);
        out.push('\n');
    }
}

/// `help:` prose plus a preview of the affected line(s) with the edits
/// applied.
fn render_suggestion(
    out: &mut String,
    d: &Diagnostic,
    sugg: &Suggestion,
    sources: &Sources,
    gw: usize,
    opts: &RenderOptions,
    p: &Palette,
) {
    out.push_str(&format!("{}help{}: {}\n", p.bold, p.reset, sugg.message));
    // Preview per file (nearly always exactly one).
    let mut files: Vec<FileId> = Vec::new();
    for (span, _) in &sugg.edits {
        if !files.contains(&span.file) {
            files.push(span.file);
        }
    }
    let _ = d;
    for file in files {
        let Some(entry) = sources.get(file) else {
            continue;
        };
        // Apply this file's edits back-to-front.
        let mut edits: Vec<&(Span, String)> =
            sugg.edits.iter().filter(|(s, _)| s.file == file).collect();
        edits.sort_by_key(|(s, _)| s.lo);
        let mut edited = entry.src.clone();
        for (span, replacement) in edits.iter().rev() {
            edited.splice(span.lo as usize..span.hi as usize, replacement.bytes());
        }
        let new_index = LineIndex::new(&edited);
        // Affected lines, at their post-edit positions.
        let mut delta = 0i64;
        let mut lines: Vec<u32> = Vec::new();
        for (span, replacement) in &edits {
            let new_lo = (span.lo as i64 + delta) as u32;
            let line = new_index.line_of(new_lo);
            if !lines.contains(&line) {
                lines.push(line);
            }
            delta += replacement.len() as i64 - (span.hi - span.lo) as i64;
        }
        gutter_row(out, gw, p);
        for line in lines {
            let (ls, le) = new_index.line_range(line);
            let (text, _) = expand(&edited[ls as usize..le as usize]);
            let (shown, _) = window(&text, opts.width, 0);
            out.push_str(&format!(
                "{}{:>gw$} |{} {}\n",
                p.gutter,
                line + 1,
                p.reset,
                shown.trim_end(),
            ));
        }
        gutter_row(out, gw, p);
    }
}
