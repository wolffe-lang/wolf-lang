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
    /// A group whose OWN breaks are a last resort: it stays flat
    /// whenever the groups nested inside it can bring the line inside
    /// the width by themselves (s156, wolf-lang#339). A member chain is
    /// the one construct with two competing break points on one line —
    /// the receiver dots and the calls' argument lists — and a plain
    /// `Group` takes the outer one first, which is why
    /// `(mut kw).push(word32_le(…) as int)` came back as `(mut kw).` /
    /// `push(` when breaking the argument list alone gives a 97-column
    /// line with every token where a reader expects it. Measured with
    /// its nested groups treated as BROKEN — i.e. asking "is there a
    /// break below me that fixes this line?" rather than "does all of
    /// this fit flat?" — so a chain whose calls have nothing to break
    /// (`a.bbbb().cccc().dddd()`) still breaks at its dots, and as one.
    LastResort(Vec<Doc>),
    /// Indent one level (applies to line breaks inside).
    Indent(Vec<Doc>),
    /// Indent one level only when the ENCLOSING group renders broken.
    /// The member-link rider: a call's argument list is laid against
    /// the line its callee name landed on, and that line is one level
    /// in exactly when the receiver-dot break put it there
    /// (`[gram.fmt.break]`, wolf-lang#339 — the closing `)` used to
    /// dedent past both the argument and the method name).
    IndentIfBroken(Vec<Doc>),
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
            Doc::Concat(ds)
            | Doc::Group(ds)
            | Doc::LastResort(ds)
            | Doc::Indent(ds)
            | Doc::IndentIfBroken(ds) => ds.iter().any(Doc::forced),
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
            Doc::Concat(ds)
            | Doc::Group(ds)
            | Doc::LastResort(ds)
            | Doc::Indent(ds)
            | Doc::IndentIfBroken(ds)
            | Doc::Shield(ds) => ds.iter().any(pierced),
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
            Doc::Concat(ds)
            | Doc::Group(ds)
            | Doc::LastResort(ds)
            | Doc::Indent(ds)
            | Doc::IndentIfBroken(ds)
            | Doc::Shield(ds) => ds.iter().any(|c| overflows(c, budget)),
            _ => false,
        }
    }
    pierced(d) || overflows(d, &mut (WIDTH as isize))
}

/// Would `docs`, rendered flat starting at `col`, stay within the
/// width **to the end of the line they land on**?
///
/// `rest` is the renderer's own pending stack (its last element is the
/// next doc to be emitted), each entry carrying the mode it will be
/// rendered in. Measuring `docs` alone — the first-fit form this was
/// until s156 — asks whether a construct fits *in isolation*, and the
/// page is not written in isolation: a parameter list ending at column
/// 93 "fits", and then the ` -> List[byte] {` the renderer has already
/// committed to putting after it runs the line to 103, where the only
/// group still able to break is the return type's (wolf-lang#339's two
/// and three: a two-token type application split across three lines to
/// rescue a signature, and a 105-column line that `--check` accepts
/// because a second pass reproduces it). Measuring the tail makes the
/// outermost group that CAN fix the line the one that breaks, which is
/// also why a receiver-dot break stops being taken ahead of the
/// argument break that would have sufficed (#339's one).
///
/// Measurement ends at the first newline the renderer will really
/// emit: a `Line`/`Softline` reached in `Broken` mode, or any forced
/// break. Groups in `docs` are measured flat (that is the question
/// being asked); groups in `rest` inherit the mode recorded for them
/// and so stop the measurement at their first break opportunity —
/// Prettier's rule, and the reason this is not quadratic in practice.
///
/// `probe_breaks_nested` is [`Doc::LastResort`]'s question: measure
/// `docs` flat, but treat every group nested inside them as BROKEN, so
/// the measurement stops at the first break a group *below* this one
/// could take. "Can something under me fix this line?" rather than
/// "does all of this fit flat?".
fn fits(
    docs: &[&Doc],
    rest: &[(usize, Mode, &Doc)],
    col: usize,
    probe_breaks_nested: bool,
) -> bool {
    let mut budget = WIDTH as isize - col as isize;
    let mut stack: Vec<(Mode, &Doc)> = Vec::with_capacity(docs.len() + rest.len());
    // `rest` is a stack: its last element is emitted first, so it is
    // pushed onto the measuring stack first (deepest).
    stack.extend(rest.iter().map(|(_, m, d)| (*m, *d)));
    stack.extend(docs.iter().rev().map(|d| (Mode::Flat, *d)));
    while let Some((mode, d)) = stack.pop() {
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
            Doc::Line => match mode {
                Mode::Flat | Mode::FlatOpen => budget -= 1,
                Mode::Broken => return budget >= 0,
            },
            Doc::Softline => match mode {
                Mode::Flat | Mode::FlatOpen => {}
                Mode::Broken => return budget >= 0,
            },
            // A forced break ends the line: everything up to here fit.
            Doc::Hardline | Doc::FreshLine | Doc::Blankline => return budget >= 0,
            Doc::Concat(ds) | Doc::Indent(ds) | Doc::IndentIfBroken(ds) => {
                for c in ds.iter().rev() {
                    stack.push((mode, c));
                }
            }
            Doc::Group(ds) | Doc::LastResort(ds) => {
                // A group whose content forces a break will break
                // wherever it lands; otherwise it inherits — flat
                // inside the docs under question, and the renderer's
                // recorded mode out in the tail. Under a last-resort
                // probe the nested groups are read as broken instead:
                // the question being asked is whether they can rescue
                // the line, and a group that can break ends it.
                let m = if ds.iter().any(Doc::forced) || (probe_breaks_nested && mode == Mode::Flat)
                {
                    Mode::Broken
                } else {
                    mode
                };
                for c in ds.iter().rev() {
                    stack.push((m, c));
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
            Doc::IfBreak { broken, flat } => {
                budget -= chars(if mode == Mode::Broken { broken } else { flat }) as isize
            }
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

/// Would taking this doc's own break actually achieve the width?
///
/// A line is sometimes past the width because of a token nothing may
/// split — `[gram.fmt.indent]` licenses exactly that — and orphaning a
/// receiver on a line of its own does not make such a line shorter:
/// `t.err = "<a 140-column message>"` came back as `t.` / `err = …`,
/// one line longer and no narrower. The last break this doc owns is
/// the one that decides the final line, so that line is what is
/// measured, at the indent the break would put it on. Nested groups
/// count as breakable (they are), and a doc with no break of its own
/// has nothing to gain.
fn break_would_help(ds: &[Doc], rest: &[(usize, Mode, &Doc)], ind: usize) -> bool {
    fn own<'a>(ds: &'a [Doc], out: &mut Vec<&'a Doc>) {
        for d in ds {
            match d {
                // The doc's OWN sequence: a group below it makes its
                // own decision and is one item here, not a break.
                Doc::Concat(cs) | Doc::Indent(cs) | Doc::IndentIfBroken(cs) => own(cs, out),
                _ => out.push(d),
            }
        }
    }
    let mut flat: Vec<&Doc> = Vec::new();
    own(ds, &mut flat);
    let Some(i) = flat
        .iter()
        .rposition(|d| matches!(d, Doc::Line | Doc::Softline))
    else {
        return false;
    };
    fits(&flat[i + 1..], rest, ind + INDENT, true)
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Flat,
    /// Flat for this doc's OWN `Line`s and `IfBreak`s, but the groups
    /// nested inside it still measure themselves: the mode a
    /// [`Doc::LastResort`] hands its children when it decided to stay
    /// flat *because* a group below it can break. Plain `Flat` would
    /// propagate down and make that break impossible, which is the
    /// whole point of the construct.
    FlatOpen,
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
                Mode::Flat | Mode::FlatOpen => {
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
            Doc::IndentIfBroken(ds) => {
                let i = if mode == Mode::Broken {
                    ind + INDENT
                } else {
                    ind
                };
                for c in ds.iter().rev() {
                    stack.push((i, mode, c));
                }
            }
            Doc::LastResort(ds) => {
                // Like `Group`, but the measure asks whether a group
                // BELOW this one can bring the line inside the width
                // (see the variant's own note): the chain's dots break
                // only when the argument lists cannot do it alone —
                // and then those lists must actually be free to break,
                // which is what `FlatOpen` preserves. Under a flat
                // parent the whole subtree is already committed flat.
                let m = if mode == Mode::Flat {
                    Mode::Flat
                } else if ds.iter().any(Doc::forced) {
                    Mode::Broken
                } else if fits(
                    &ds.iter().collect::<Vec<_>>(),
                    &stack,
                    pending_indent.unwrap_or(col).max(col),
                    true,
                ) || !break_would_help(ds, &stack, ind)
                {
                    Mode::FlatOpen
                } else {
                    Mode::Broken
                };
                for c in ds.iter().rev() {
                    stack.push((ind, m, c));
                }
            }
            Doc::Group(ds) => {
                // A group under a FLAT parent inherits flat (the
                // classic algorithm's invariant) instead of
                // re-measuring. For measured content the two agree:
                // the parent's `fits` descended into this group, so it
                // fits, and a forced child would have forced the
                // parent broken. The difference is content the parent
                // could not see — a hug shield's interior (and
                // anything after one), where `fits` deliberately
                // stops. There a fresh measure at the true column let
                // this group break by width UNDER a flat parent: the
                // flat chain around it was a lie the next pass
                // (reading the break from the source) refused to
                // repeat, cascading its outer blocks multiline — one
                // layout per pass (idem_member_chain_width).
                // Structural forces still break; width inside a shield
                // defers to the shield's contract that the
                // surroundings stay flat.
                let flat = !ds.iter().any(Doc::forced)
                    && (mode == Mode::Flat
                        || fits(
                            &ds.iter().collect::<Vec<_>>(),
                            &stack,
                            pending_indent.unwrap_or(col).max(col),
                            false,
                        ));
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
