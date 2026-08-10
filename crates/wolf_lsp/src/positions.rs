//! Position-encoding negotiation and byte-offset ⇄ LSP-position
//! translation. **The protocol's UTF-16 wart lives here and only here**
//! (report 09 L2): `wolf_query` speaks byte offsets, every conversion
//! in the shim routes through this module, and no other file mentions
//! code units.
//!
//! All three 3.17 encodings are supported. Negotiation prefers `utf-8`
//! when the client offers it (positions become byte columns — zero
//! conversion), then the mandatory `utf-16` default, then `utf-32`
//! (code points — what rope-backed editors natively count). A client
//! that advertises nothing gets `utf-16`, per spec.
//!
//! Spec sharp edges honored: `character` greater than the line length
//! clamps to the line end; a line past EOF clamps to the last line;
//! positions never split a code point (a mid-surrogate `character`
//! resolves to the code point's start).

use lsp_types::{InitializeParams, Position, PositionEncodingKind};
use wolf_span::LineIndex;

/// The negotiated encoding, fixed for the session at `initialize`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoding {
    Utf8,
    Utf16,
    Utf32,
}

impl Encoding {
    pub fn kind(self) -> PositionEncodingKind {
        match self {
            Encoding::Utf8 => PositionEncodingKind::UTF8,
            Encoding::Utf16 => PositionEncodingKind::UTF16,
            Encoding::Utf32 => PositionEncodingKind::UTF32,
        }
    }
}

/// Pick the session encoding from the client's declared preferences.
pub fn negotiate(init: &InitializeParams) -> Encoding {
    let offered = init
        .capabilities
        .general
        .as_ref()
        .and_then(|g| g.position_encodings.as_deref())
        .unwrap_or(&[]);
    if offered.contains(&PositionEncodingKind::UTF8) {
        Encoding::Utf8
    } else if offered.contains(&PositionEncodingKind::UTF16) || offered.is_empty() {
        Encoding::Utf16
    } else if offered.contains(&PositionEncodingKind::UTF32) {
        Encoding::Utf32
    } else {
        // The client offered only encodings we don't know; utf-16 is
        // the only spec-legal fallback.
        Encoding::Utf16
    }
}

/// Length of the UTF-8 sequence starting at `b` (invalid lead bytes
/// count as one-byte units so translation is total on any input).
fn seq_len(b: u8) -> usize {
    match b {
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

/// Units this sequence occupies in `enc` (utf-16: astral = 2).
fn units(enc: Encoding, len: usize) -> u32 {
    match enc {
        Encoding::Utf8 => len as u32,
        Encoding::Utf32 => 1,
        Encoding::Utf16 => {
            if len == 4 {
                2
            } else {
                1
            }
        }
    }
}

/// Byte offset → LSP position. Offsets past EOF clamp (zero-width
/// spans at EOF are routine in recovery).
pub fn offset_to_position(src: &[u8], index: &LineIndex, offset: u32, enc: Encoding) -> Position {
    let offset = offset.min(src.len() as u32);
    let lc = index.line_col(offset);
    let (start, _) = index.line_range(lc.line);
    let line_bytes = &src[start as usize..(start + lc.col) as usize];
    let mut character = 0u32;
    if enc == Encoding::Utf8 {
        character = lc.col;
    } else {
        let mut i = 0usize;
        while i < line_bytes.len() {
            let len = seq_len(line_bytes[i]).min(line_bytes.len() - i);
            character += units(enc, len);
            i += len;
        }
    }
    Position {
        line: lc.line,
        character,
    }
}

/// LSP position → byte offset, with the spec's clamping rules.
pub fn position_to_offset(src: &[u8], index: &LineIndex, pos: Position, enc: Encoding) -> u32 {
    let line = pos.line.min(index.line_count() - 1);
    let (start, end) = index.line_range(line);
    let line_bytes = &src[start as usize..end as usize];
    if enc == Encoding::Utf8 {
        return start + pos.character.min(line_bytes.len() as u32);
    }
    let mut i = 0usize;
    let mut character = 0u32;
    while i < line_bytes.len() && character < pos.character {
        let len = seq_len(line_bytes[i]).min(line_bytes.len() - i);
        let step = units(enc, len);
        if character + step > pos.character {
            break; // a mid-code-point position resolves to its start
        }
        character += step;
        i += len;
    }
    start + i as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    // The spec's own example: in `a𐐀b`, `b` is at utf-16 offset 3,
    // utf-8 offset 5, utf-32 offset 2.
    #[test]
    fn spec_surrogate_example() {
        let src = "a\u{10400}b".as_bytes();
        let idx = LineIndex::new(src);
        let b_off = 5u32;
        assert_eq!(
            offset_to_position(src, &idx, b_off, Encoding::Utf16).character,
            3
        );
        assert_eq!(
            offset_to_position(src, &idx, b_off, Encoding::Utf8).character,
            5
        );
        assert_eq!(
            offset_to_position(src, &idx, b_off, Encoding::Utf32).character,
            2
        );
        for (enc, ch) in [
            (Encoding::Utf16, 3),
            (Encoding::Utf8, 5),
            (Encoding::Utf32, 2),
        ] {
            let pos = Position {
                line: 0,
                character: ch,
            };
            assert_eq!(position_to_offset(src, &idx, pos, enc), b_off);
        }
    }

    #[test]
    fn clamping_rules() {
        let src = b"ab\ncd";
        let idx = LineIndex::new(src);
        // Character past line end clamps to line end.
        let pos = Position {
            line: 0,
            character: 99,
        };
        assert_eq!(position_to_offset(src, &idx, pos, Encoding::Utf16), 2);
        // Line past EOF clamps to the last line.
        let pos = Position {
            line: 99,
            character: 0,
        };
        assert_eq!(position_to_offset(src, &idx, pos, Encoding::Utf16), 3);
    }

    #[test]
    fn mid_surrogate_resolves_to_code_point_start() {
        let src = "\u{10400}x".as_bytes();
        let idx = LineIndex::new(src);
        let pos = Position {
            line: 0,
            character: 1, // inside the surrogate pair
        };
        assert_eq!(position_to_offset(src, &idx, pos, Encoding::Utf16), 0);
    }

    #[test]
    fn round_trips_on_multiline_unicode() {
        let src = "let s = \"héllo\"\nlet 𝕩 = 1\n".as_bytes();
        let idx = LineIndex::new(src);
        for enc in [Encoding::Utf8, Encoding::Utf16, Encoding::Utf32] {
            // Walking by sequence length keeps every probe on a
            // code-point boundary.
            let mut off = 0u32;
            while (off as usize) < src.len() {
                let pos = offset_to_position(src, &idx, off, enc);
                assert_eq!(position_to_offset(src, &idx, pos, enc), off, "enc {enc:?}");
                off += seq_len(src[off as usize]) as u32;
            }
        }
    }
}
