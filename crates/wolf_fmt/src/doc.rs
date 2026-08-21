//! The document IR: a deliberately tiny Wadler/Prettier doc algebra.
//!
//! Seven constructors carry the whole formatter — `text`, `concat`,
//! `line`/`softline`/`hardline`, `group`, `indent` — plus three
//! pragmatics the trivia machinery needs: `raw` (verbatim multi-line
//! bytes for strings, inline C, and error-region pass-through),
//! `line_suffix` (trailing comments ride to the end of the current
//! line), and `if_break` (trailing commas / inline-`;` separators).
//!
//! Rendering is the classic two-mode algorithm: a [`Group`] renders
//! flat when nothing inside forces a break and the flat form fits the
//! width; otherwise every `line`/`softline` inside it becomes a
//! newline at the current indent. `hardline`, `blankline`, `raw`, and
//! `break_parent` force every enclosing group broken (Prettier's
//! propagation rule).

/// Fixed line width — locked by s11, changes ride editions (D36).
pub const WIDTH: usize = 100;
/// Fixed indent — 4 spaces, no tabs (`[gram.fmt.indent]`).
pub const INDENT: usize = 4;

/// One node of the document IR.
#[derive(Clone, Debug)]
pub enum Doc {
    /// Literal bytes, no newlines.
    Text(Vec<u8>),
    /// Verbatim bytes, may contain newlines; emitted without
    /// re-indentation and forces enclosing groups broken.
    Raw(Vec<u8>),
    /// Space when flat, newline when broken.
    Line,
    /// Nothing when flat, newline when broken.
    Softline,
    /// Always a newline.
    Hardline,
    /// A newline only when the cursor is mid-line: a comment that owned
    /// its own source line must start a fresh output line, but callers
    /// that already broke (statement layout) must not gain a blank.
    /// Forces enclosing groups broken like Hardline does.
    FreshLine,
    /// Always a blank line (two newlines).
    Blankline,
    /// Sequence.
    Concat(Vec<Doc>),
    /// Flat-if-it-fits region.
    Group(Vec<Doc>),
    /// Indent one level (applies to line breaks inside).
    Indent(Vec<Doc>),
    /// `broken` bytes when the enclosing group is broken, `flat` when flat.
    IfBreak { broken: Vec<u8>, flat: Vec<u8> },
    /// Bytes stashed until just before the next emitted newline
    /// (trailing comments). Does not count toward width.
    LineSuffix(Vec<u8>),
    /// Zero-width; forces every enclosing group broken.
    BreakParent,
    /// Renders like `Concat`, but hard breaks inside do **not** force
    /// enclosing groups broken (the hug shield: a trailing block
    /// argument may break internally while the call and any member
    /// chain around it stay flat).
    Shield(Vec<Doc>),
}

impl Doc {
    pub fn text(s: impl Into<Vec<u8>>) -> Doc {
        Doc::Text(s.into())
    }

    /// Does this subtree force enclosing groups to break?
    pub fn forced(&self) -> bool {
        match self {
            Doc::Hardline | Doc::FreshLine | Doc::Blankline | Doc::BreakParent => true,
            Doc::Raw(bytes) => bytes.contains(&b'\n'),
            Doc::Concat(ds) | Doc::Group(ds) | Doc::Indent(ds) => ds.iter().any(Doc::forced),
            Doc::Shield(_) => false,
            _ => false,
        }
    }
}

/// Will this doc certainly render with a newline, wherever it is
/// placed? Unlike [`Doc::forced`] this PIERCES shields — a hug shield
/// keeps enclosing groups flat, but the newlines inside it are still
/// real ink on the page — and it also counts a flat run that cannot
/// fit even at column zero, which the renderer will break by width.
/// `block()`'s inline choice asks this question; `forced()` answers a
/// different one (group breaking), and conflating them let a block
/// whose shielded closure was certain to break render "inline" on
/// pass one and multiline in reality (idem_member_chain_width's
/// second layer).
pub(crate) fn wont_render_inline(d: &Doc) -> bool {
    fn pierced(d: &Doc) -> bool {
        match d {
            Doc::Hardline | Doc::FreshLine | Doc::Blankline | Doc::BreakParent => true,
            Doc::Raw(bytes) => bytes.contains(&b'\n'),
            Doc::Concat(ds) | Doc::Group(ds) | Doc::Indent(ds) | Doc::Shield(ds) => {
                ds.iter().any(pierced)
            }
            _ => false,
        }
    }
    // The width half must PIERCE shields too — `fits` deliberately
    // stops at one (the hug contract), which would blind this check to
    // exactly the content whose width decides the question.
    fn overflows(d: &Doc, budget: &mut isize) -> bool {
        match d {
            Doc::Text(t) | Doc::Raw(t) => {
                match t.iter().position(|&b| b == b'\n') {
                    // A break inside: the flat prefix ends here.
                    Some(_) => false,
                    None => {
                        *budget -= t.iter().filter(|&&b| (b & 0xC0) != 0x80).count() as isize;
                        *budget < 0
                    }
                }
            }
            Doc::Line => {
                *budget -= 1;
                *budget < 0
            }
            Doc::IfBreak { flat, .. } => {
                *budget -= flat.iter().filter(|&&b| (b & 0xC0) != 0x80).count() as isize;
                *budget < 0
            }
            Doc::Concat(ds) | Doc::Group(ds) | Doc::Indent(ds) | Doc::Shield(ds) => {
                ds.iter().any(|c| overflows(c, budget))
            }
            _ => false,
        }
    }
    pierced(d) || overflows(d, &mut (WIDTH as isize))
}

/// Would `docs`, rendered flat starting at `col`, stay within the width
/// through its next hard stop? (Classic first-fit measure.)
fn fits(docs: &[Doc], col: usize) -> bool {
    let mut budget = WIDTH as isize - col as isize;
    let mut stack: Vec<&Doc> = docs.iter().rev().collect();
    while let Some(d) = stack.pop() {
        if budget < 0 {
            return false;
        }
        match d {
            Doc::Text(t) => budget -= chars(t) as isize,
            Doc::Raw(r) => {
                // A raw block forces a break anyway; measure to its
                // first newline.
                match r.iter().position(|&b| b == b'\n') {
                    Some(i) => return budget - chars(&r[..i]) as isize >= 0,
                    None => budget -= chars(r) as isize,
                }
            }
            Doc::Line => budget -= 1,
            Doc::Softline => {}
            // A forced break ends the line: everything up to here fit.
            Doc::Hardline | Doc::FreshLine | Doc::Blankline => return budget >= 0,
            Doc::Concat(ds) | Doc::Group(ds) | Doc::Indent(ds) => {
                for c in ds.iter().rev() {
                    stack.push(c);
                }
            }
            // The hug shield's contract is that the construct inside
            // manages its own breaking while the call and chain around
            // it stay FLAT — so measurement ends here, successfully.
            // Descending instead made the chain's fit depend on the
            // shielded block's internal geometry: an inline-in-source
            // block measured flat (huge) and broke the chain, while
            // the same block written multiline measured to its first
            // hardline and the chain joined — one flip per pass
            // (idem_member_chain_width).
            Doc::Shield(_) => return budget >= 0,
            Doc::IfBreak { flat, .. } => budget -= chars(flat) as isize,
            Doc::LineSuffix(_) | Doc::BreakParent => {}
        }
    }
    budget >= 0
}

/// Character count for width purposes (UTF-8 scalar values; invalid
/// bytes count one column each).
fn chars(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| (b & 0xC0) != 0x80).count()
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Flat,
    Broken,
}

/// Render at [`WIDTH`]. The result always ends with exactly one
/// newline (when nonempty) and contains no trailing whitespace on any
/// line.
pub fn render(doc: &Doc) -> Vec<u8> {
    let mut out = Vec::new();
    let mut col = 0usize;
    // Indent is written lazily so blank lines carry no spaces.
    let mut pending_indent: Option<usize> = None;
    let mut suffixes: Vec<u8> = Vec::new();
    let mut stack: Vec<(usize, Mode, &Doc)> = vec![(0, Mode::Broken, doc)];

    fn newline(out: &mut Vec<u8>, suffixes: &mut Vec<u8>, col: &mut usize) {
        if !suffixes.is_empty() {
            // Overlap-merge: the line's own trailing spaces and the
            // suffix's preserved leading run count ONCE (total = max,
            // not sum) — summing added one space per format pass and
            // broke idempotence (the fmt fuzz's fourth find).
            let line_ws = out.iter().rev().take_while(|&&b| b == b' ').count();
            let suffix_ws = suffixes.iter().take_while(|&&b| b == b' ').count();
            let drop = line_ws.min(suffix_ws);
            out.extend_from_slice(&suffixes[drop..]);
            suffixes.clear();
        }
        while out.last() == Some(&b' ') {
            out.pop();
        }
        out.push(b'\n');
        *col = 0;
    }

    while let Some((ind, mode, d)) = stack.pop() {
        match d {
            Doc::Text(t) => {
                if let Some(n) = pending_indent.take() {
                    out.extend(std::iter::repeat_n(b' ', n));
                    col = n;
                }
                out.extend_from_slice(t);
                col += chars(t);
            }
            Doc::Raw(r) => {
                if let Some(n) = pending_indent.take() {
                    out.extend(std::iter::repeat_n(b' ', n));
                    col = n;
                }
                out.extend_from_slice(r);
                match r.iter().rposition(|&b| b == b'\n') {
                    Some(i) => col = chars(&r[i + 1..]),
                    None => col += chars(r),
                }
            }
            Doc::Line => match mode {
                Mode::Flat => {
                    if pending_indent.is_none() {
                        out.push(b' ');
                        col += 1;
                    }
                }
                Mode::Broken => {
                    newline(&mut out, &mut suffixes, &mut col);
                    pending_indent = Some(ind);
                }
            },
            Doc::Softline => {
                if mode == Mode::Broken {
                    newline(&mut out, &mut suffixes, &mut col);
                    pending_indent = Some(ind);
                }
            }
            Doc::Hardline => {
                newline(&mut out, &mut suffixes, &mut col);
                pending_indent = Some(ind);
            }
            Doc::FreshLine => {
                if pending_indent.is_some() {
                    // Already at a line start awaiting indent: adopt this
                    // context's indent, emit nothing.
                    pending_indent = Some(ind);
                } else if col > 0 {
                    let start = out.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
                    if out[start..].iter().all(|&b| b == b' ') {
                        // Nothing but indentation has been written to
                        // this line, so it has not really started:
                        // rewind it instead of ending it. Ending it
                        // left an empty line that the *next* pass read
                        // as a source blank and preserved — a blank
                        // line per pass, forever (an emitter that
                        // pushes a separator before an empty operand,
                        // such as a damaged match arm's missing
                        // pattern, is enough to trigger it).
                        out.truncate(start);
                        col = 0;
                    } else {
                        newline(&mut out, &mut suffixes, &mut col);
                    }
                    pending_indent = Some(ind);
                }
            }
            Doc::Blankline => {
                newline(&mut out, &mut suffixes, &mut col);
                out.push(b'\n');
                pending_indent = Some(ind);
            }
            Doc::Concat(ds) | Doc::Shield(ds) => {
                for c in ds.iter().rev() {
                    stack.push((ind, mode, c));
                }
            }
            Doc::Indent(ds) => {
                for c in ds.iter().rev() {
                    stack.push((ind + INDENT, mode, c));
                }
            }
            Doc::Group(ds) => {
                let flat =
                    !ds.iter().any(Doc::forced) && fits(ds, pending_indent.unwrap_or(col).max(col));
                let m = if flat { Mode::Flat } else { Mode::Broken };
                for c in ds.iter().rev() {
                    stack.push((ind, m, c));
                }
            }
            Doc::IfBreak { broken, flat } => {
                let t = if mode == Mode::Broken { broken } else { flat };
                if !t.is_empty() {
                    if let Some(n) = pending_indent.take() {
                        out.extend(std::iter::repeat_n(b' ', n));
                        col = n;
                    }
                    out.extend_from_slice(t);
                    col += chars(t);
                }
            }
            Doc::LineSuffix(s) => suffixes.extend_from_slice(s),
            Doc::BreakParent => {}
        }
    }
    if !suffixes.is_empty() {
        out.append(&mut suffixes);
    }
    // Exactly one trailing newline.
    while matches!(out.last(), Some(b'\n' | b' ' | b'\t')) {
        out.pop();
    }
    if !out.is_empty() {
        out.push(b'\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Doc {
        Doc::text(s.as_bytes().to_vec())
    }

    #[test]
    fn flat_group_stays_flat() {
        let d = Doc::Group(vec![t("f("), Doc::Softline, t("x"), Doc::Softline, t(")")]);
        assert_eq!(render(&d), b"f(x)\n");
    }

    #[test]
    fn wide_group_breaks_with_indent() {
        let long = "x".repeat(120);
        let d = Doc::Group(vec![
            t("f("),
            Doc::Indent(vec![Doc::Softline, t(&long)]),
            Doc::Softline,
            t(")"),
        ]);
        let out = render(&d);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, format!("f(\n    {long}\n)\n"));
    }

    #[test]
    fn hardline_forces_all_enclosing_groups() {
        let d = Doc::Group(vec![
            t("a"),
            Doc::Line,
            Doc::Group(vec![t("b"), Doc::Hardline, t("c")]),
        ]);
        assert_eq!(render(&d), b"a\nb\nc\n");
    }

    #[test]
    fn if_break_selects_by_mode() {
        let flat = Doc::Group(vec![
            t("["),
            t("1"),
            Doc::IfBreak {
                broken: b",".to_vec(),
                flat: b"".to_vec(),
            },
            Doc::Softline,
            t("]"),
        ]);
        assert_eq!(render(&flat), b"[1]\n");
        let broken = Doc::Group(vec![
            t("["),
            Doc::Indent(vec![
                Doc::Softline,
                t("1"),
                Doc::IfBreak {
                    broken: b",".to_vec(),
                    flat: b"".to_vec(),
                },
            ]),
            Doc::Softline,
            t("]"),
            Doc::BreakParent,
        ]);
        assert_eq!(render(&broken), b"[\n    1,\n]\n");
    }

    #[test]
    fn line_suffix_rides_to_line_end() {
        let d = Doc::Concat(vec![
            t("code"),
            Doc::LineSuffix(b" // hey".to_vec()),
            Doc::Hardline,
            t("next"),
        ]);
        assert_eq!(render(&d), b"code // hey\nnext\n");
    }

    #[test]
    fn blank_lines_carry_no_indent() {
        // The empty line itself has no trailing spaces; content after
        // it picks the indent back up.
        let d = Doc::Indent(vec![t("a"), Doc::Blankline, t("b")]);
        assert_eq!(render(&d), b"a\n\n    b\n");
    }

    #[test]
    fn width_counts_chars_not_bytes() {
        assert_eq!(chars("géométrie".as_bytes()), 9);
        assert_eq!(chars(b"ascii"), 5);
    }
}
