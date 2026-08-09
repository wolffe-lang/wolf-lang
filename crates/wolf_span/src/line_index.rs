//! Byte offset → line/column lookup (s10).
//!
//! The diagnostics renderer needs `file:line:col` loci and per-line
//! slices of the source. [`LineIndex`] is built once per file — a
//! sorted table of line-start offsets — and answers both queries by
//! binary search. It lives here because `wolf_span` owns source
//! positions and depends on nothing; every reporter shares one
//! definition of "line 3, column 7".
//!
//! Lines are split on `\n` only (`\r` stays part of the line — spans
//! are byte-exact and the renderer trims it). `line` is 0-based and
//! `col` is a 0-based *byte* offset within the line; presentation
//! (1-based, character- or width-aware columns) is the renderer's job.

/// A 0-based line / byte-column position (see module docs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineCol {
    pub line: u32,
    /// Byte offset from the start of the line.
    pub col: u32,
}

/// Sorted line-start table for one source buffer.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of the first byte of each line. Always non-empty:
    /// line 0 starts at offset 0, even in an empty buffer.
    starts: Vec<u32>,
    /// Total buffer length in bytes.
    len: u32,
}

impl LineIndex {
    /// Index `src`. Panics on buffers longer than `u32::MAX` (spans are
    /// `u32` byte offsets, D25 — the lexer enforces the same bound).
    pub fn new(src: &[u8]) -> LineIndex {
        let len = u32::try_from(src.len()).expect("source exceeds u32::MAX bytes");
        let mut starts = vec![0u32];
        for (i, &b) in src.iter().enumerate() {
            if b == b'\n' {
                starts.push(i as u32 + 1);
            }
        }
        LineIndex { starts, len }
    }

    /// Number of lines (a trailing newline opens a final empty line).
    pub fn line_count(&self) -> u32 {
        self.starts.len() as u32
    }

    /// The line containing `offset` (0-based). Offsets past the end of
    /// the buffer clamp to the last line.
    pub fn line_of(&self, offset: u32) -> u32 {
        match self.starts.binary_search(&offset.min(self.len)) {
            Ok(line) => line as u32,
            Err(insert) => (insert - 1) as u32,
        }
    }

    /// Line + byte column of `offset` (both 0-based; see module docs).
    pub fn line_col(&self, offset: u32) -> LineCol {
        let offset = offset.min(self.len);
        let line = self.line_of(offset);
        LineCol {
            line,
            col: offset - self.starts[line as usize],
        }
    }

    /// Byte range `[start, end)` of `line`, excluding the terminating
    /// `\n`. Panics if `line >= line_count()`.
    pub fn line_range(&self, line: u32) -> (u32, u32) {
        let start = self.starts[line as usize];
        let end = self
            .starts
            .get(line as usize + 1)
            .map_or(self.len, |&next| next - 1);
        (start, end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_buffer_has_one_line() {
        let idx = LineIndex::new(b"");
        assert_eq!(idx.line_count(), 1);
        assert_eq!(idx.line_col(0), LineCol { line: 0, col: 0 });
        assert_eq!(idx.line_range(0), (0, 0));
    }

    #[test]
    fn lines_and_columns() {
        //            0123 4567 89
        let idx = LineIndex::new(b"ab\ncde\n\nf");
        assert_eq!(idx.line_count(), 4);
        assert_eq!(idx.line_col(0), LineCol { line: 0, col: 0 });
        assert_eq!(idx.line_col(2), LineCol { line: 0, col: 2 }); // the \n itself
        assert_eq!(idx.line_col(3), LineCol { line: 1, col: 0 });
        assert_eq!(idx.line_col(5), LineCol { line: 1, col: 2 });
        assert_eq!(idx.line_col(7), LineCol { line: 2, col: 0 });
        assert_eq!(idx.line_col(8), LineCol { line: 3, col: 0 });
        assert_eq!(idx.line_range(0), (0, 2));
        assert_eq!(idx.line_range(1), (3, 6));
        assert_eq!(idx.line_range(2), (7, 7));
        assert_eq!(idx.line_range(3), (8, 9));
    }

    #[test]
    fn trailing_newline_opens_an_empty_final_line() {
        let idx = LineIndex::new(b"x\n");
        assert_eq!(idx.line_count(), 2);
        assert_eq!(idx.line_range(1), (2, 2));
        // Past-the-end offsets clamp instead of panicking (zero-width
        // spans at EOF are routine in recovery).
        assert_eq!(idx.line_col(99), LineCol { line: 1, col: 0 });
    }

    #[test]
    fn crlf_keeps_the_cr_in_the_line() {
        let idx = LineIndex::new(b"ab\r\ncd");
        assert_eq!(idx.line_range(0), (0, 3)); // "ab\r"
        assert_eq!(idx.line_col(4), LineCol { line: 1, col: 0 });
    }
}
