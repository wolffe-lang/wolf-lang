//! A hole-free string literal's VALUE, cooked from its source text
//! (s210, wolf-lang#585): the `"""` dedent by the closing delimiter's
//! column (D26, `[gram.lex.str.multi]`), the raw fence
//! (`[gram.lex.str.raw]`: no escapes), the shared escape set with
//! `\x…`/`\u{…}` (`[gram.lex.str.escape]`), and `{{`/`}}` as one brace
//! (`[gram.lex.str]`).
//!
//! The comptime engine's evaluator reads literals through here. Before
//! s210 it carried its own decoder (one quote stripped each end, five
//! escapes), so every value it produced from a literal — a module
//! initializer (kw09, `[mem.static.3]`) and s71's fold table alike —
//! held the raw source text of a `"""`, raw, code-point or brace
//! literal. The two executing lanes keep their own byte-identical
//! copies (`wolf_wir::lower`, `wolf_mem::ubcheck`); `wolf_wir`'s tests
//! pin this one against the lowering's over a literal table, so the
//! three cannot part silently again.

/// The value of a hole-free string literal, from its full source text
/// (delimiters included). Interpolated literals are not this function's:
/// their holes need the evaluator.
pub fn cook(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let cooked = if bytes.starts_with(b"\"\"\"") {
        let inner = &bytes[3..bytes.len().saturating_sub(3).max(3)];
        decode_escapes(&dedent_multiline(inner))
    } else if let Some(inner) = raw_inner(bytes) {
        inner.to_vec()
    } else {
        let inner = if bytes.len() >= 2 {
            &bytes[1..bytes.len() - 1]
        } else {
            bytes
        };
        decode_escapes(inner)
    };
    // Source text is UTF-8 and every escape decodes to a whole code
    // point, so this never replaces anything.
    String::from_utf8_lossy(&cooked).into_owned()
}

/// The inner bytes of a raw literal (`r"…"`, `r#"…"#`, …), or `None`
/// when the text is not raw-delimited.
fn raw_inner(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.first() != Some(&b'r') {
        return None;
    }
    let hashes = bytes[1..].iter().take_while(|&&b| b == b'#').count();
    let open = 1 + hashes;
    if bytes.get(open) != Some(&b'"') {
        return None;
    }
    let start = open + 1;
    let end = bytes.len().saturating_sub(1 + hashes).max(start);
    Some(&bytes[start..end])
}

/// Dedent a `"""` literal's inner bytes by the closing delimiter's
/// column: the opening newline drops, every line loses the closing
/// line's indentation where it starts with it.
fn dedent_multiline(inner: &[u8]) -> Vec<u8> {
    let mut inner = inner;
    if inner.starts_with(b"\r\n") {
        inner = &inner[2..];
    } else if inner.first() == Some(&b'\n') {
        inner = &inner[1..];
    }
    let last_nl = inner.iter().rposition(|&b| b == b'\n');
    let (body, indent) = match last_nl {
        Some(i) => inner.split_at(i + 1),
        None => return inner.to_vec(),
    };
    if !indent.iter().all(|&b| b == b' ' || b == b'\t') {
        return inner.to_vec();
    }
    let mut out = Vec::with_capacity(body.len());
    let mut start = 0;
    while start < body.len() {
        let end = body[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| start + p + 1)
            .unwrap_or(body.len());
        let line = &body[start..end];
        out.extend_from_slice(line.strip_prefix(indent).unwrap_or(line));
        start = end;
    }
    out
}

/// The escape set over a hole-free byte run.
fn decode_escapes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\\' && i + 1 < bytes.len() {
            if let Some((ch, consumed)) = decode_codepoint_escape(&bytes[i..]) {
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                i += consumed;
                continue;
            }
            out.push(match bytes[i + 1] {
                b'n' => b'\n',
                b't' => b'\t',
                b'r' => b'\r',
                b'0' => 0,
                other => other,
            });
            i += 2;
            continue;
        }
        if (c == b'{' || c == b'}') && bytes.get(i + 1) == Some(&c) {
            out.push(c);
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// A `\xNN` or `\u{…}` escape at the start of `bytes` (the backslash):
/// the code point and the bytes consumed, or `None` for any other shape.
fn decode_codepoint_escape(bytes: &[u8]) -> Option<(char, usize)> {
    match bytes.get(1)? {
        b'x' => {
            let hex = bytes.get(2..4)?;
            let n = u32::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
            Some((char::from_u32(n)?, 4))
        }
        b'u' => {
            if bytes.get(2) != Some(&b'{') {
                return None;
            }
            let close = bytes[3..].iter().position(|&b| b == b'}')?;
            let s = std::str::from_utf8(&bytes[3..3 + close]).ok()?;
            if s.is_empty() || s.len() > 6 {
                return None;
            }
            let n = u32::from_str_radix(s, 16).ok()?;
            Some((char::from_u32(n)?, 3 + close + 1))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::cook;

    /// Each shape the engine's old decoder got wrong, by the value the
    /// spec gives it (the bytes lupin and both lowerings produce).
    #[test]
    fn every_literal_shape_cooks_to_its_value() {
        let rows: &[(&str, &str)] = &[
            ("\"plain\"", "plain"),
            ("\"tab\\there\\n\"", "tab\there\n"),
            ("\"q\\\"q\\\\\"", "q\"q\\"),
            (
                "\"\"\"\n    two\n      lines\n    \"\"\"",
                "two\n  lines\n",
            ),
            ("\"\"\"\r\n  a\r\n  \"\"\"", "a\r\n"),
            ("\"\"\"one line\"\"\"", "one line"),
            ("\"\"\"\n  \\tx\n  \"\"\"", "\tx\n"),
            ("r\"a\\nb\"", "a\\nb"),
            ("r#\"say \"hi\"\"#", "say \"hi\""),
            ("\"\\u{41}\\x42\\u{1F43A}\"", "AB\u{1F43A}"),
            ("\"{{x}}\"", "{x}"),
            ("\"\"", ""),
        ];
        for (raw, want) in rows {
            assert_eq!(cook(raw), *want, "the value of {raw}");
        }
    }
}
