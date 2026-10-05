//! The packed format-spec renderer, without an allocator (kw12).
//!
//! The twin of the hosted `wolf_rt::io` renderer (itself the twin of
//! `wolf_sema::fmtspec`), branch for branch, for the hole types the
//! freestanding target admits — str, bool, char (as str) and the
//! integers; floats are refused on the target before a strbuf is
//! reached (K10(b)). The hosted renderer builds `String`s; this one
//! streams the same bytes into a sink. Parity is gated end to end: the
//! freestanding witness renders a spec sweep and its bytes must equal
//! the hosted build's (`crates/wolf_driver/tests/freestanding_alloc.rs`).

#[derive(Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bin,
    Oct,
    Hex,
    HexUpper,
    Other,
}

#[derive(Clone, Copy)]
struct Spec {
    fill: u8,
    align: Option<Align>,
    sign: bool,
    zero: bool,
    width: usize,
    precision: Option<usize>,
    kind: Option<Kind>,
    unsigned: bool,
}

/// The packed layout (`wolf_rt::io::unpack`): fill byte 0..8, align
/// 8..10, sign 10, zero 11, width-present 12, precision-present 13,
/// unsigned 14, width 16..32, precision 32..48, kind 48..52.
fn unpack(packed: i64) -> Spec {
    let fill = (packed & 0xff) as u8;
    Spec {
        fill: if fill == 0 { b' ' } else { fill },
        align: match (packed >> 8) & 0b11 {
            1 => Some(Align::Left),
            2 => Some(Align::Center),
            3 => Some(Align::Right),
            _ => None,
        },
        sign: (packed >> 10) & 1 == 1,
        zero: (packed >> 11) & 1 == 1,
        width: if (packed >> 12) & 1 == 1 {
            ((packed >> 16) & 0xffff) as usize
        } else {
            0
        },
        precision: ((packed >> 13) & 1 == 1).then_some(((packed >> 32) & 0xffff) as usize),
        kind: match (packed >> 48) & 0xf {
            0 => None,
            1 => Some(Kind::Bin),
            2 => Some(Kind::Oct),
            3 => Some(Kind::Hex),
            4 => Some(Kind::HexUpper),
            // Exp, ExpUpper, Fixed: an integer renders in decimal.
            5..=7 => Some(Kind::Other),
            _ => None,
        },
        unsigned: (packed >> 14) & 1 == 1,
    }
}

/// A hole's value.
pub(crate) enum Val<'a> {
    Str(&'a [u8]),
    Bool(bool),
    Int(i64),
}

/// The digits of `mag` in `base` (2, 8, 10 or 16), most significant
/// first, written at the END of `buf`; returns the start index.
fn digits(mut mag: u64, base: u64, upper: bool, buf: &mut [u8; 72]) -> usize {
    let table: &[u8; 16] = if upper {
        b"0123456789ABCDEF"
    } else {
        b"0123456789ABCDEF"
    };
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = table[(mag % base) as usize];
        mag /= base;
        if mag == 0 {
            break;
        }
    }
    i
}

/// Render `v` under the packed `spec` into `sink`, byte-identical with
/// the hosted `render_*_packed`.
pub(crate) fn render_packed(v: Val<'_>, packed: i64, sink: &mut dyn FnMut(&[u8])) {
    let spec = unpack(packed);
    let mut buf = [0u8; 72];
    // The unpadded rendering (`core` in the hosted renderer).
    let core: &[u8] = match v {
        Val::Str(s) => match spec.precision {
            Some(p) => {
                let mut end = p.min(s.len());
                // Back off to a char boundary (never inside a UTF-8
                // continuation byte), as `is_char_boundary` does.
                while end < s.len() && end > 0 && (s[end] & 0xC0) == 0x80 {
                    end -= 1;
                }
                &s[..end]
            }
            None => s,
        },
        Val::Bool(b) => {
            if b {
                b"true"
            } else {
                b"false"
            }
        }
        Val::Int(n) => {
            let (neg, mag) = if spec.unsigned {
                (false, n as u64)
            } else {
                (n < 0, n.unsigned_abs())
            };
            let start = match spec.kind {
                Some(Kind::Bin) => digits(mag, 2, false, &mut buf),
                Some(Kind::Oct) => digits(mag, 8, false, &mut buf),
                Some(Kind::Hex) => digits(mag, 16, false, &mut buf),
                Some(Kind::HexUpper) => digits(mag, 16, true, &mut buf),
                Some(Kind::Other) | None => digits(mag, 10, false, &mut buf),
            };
            let start = if neg {
                buf[start - 1] = b'-';
                start - 1
            } else if spec.sign {
                buf[start - 1] = b'+';
                start - 1
            } else {
                start
            };
            &buf[start..]
        }
    };
    if spec.width <= core.len() {
        sink(core);
        return;
    }
    let missing = spec.width - core.len();
    if spec.zero {
        // Every admitted value is finite: the sign (if the rendering
        // starts with one) stays in front of the zeros.
        let (sign, rest) = match core.first() {
            Some(b'-' | b'+') => core.split_at(1),
            _ => (&core[..0], core),
        };
        sink(sign);
        repeat(sink, b"0", missing);
        sink(rest);
        return;
    }
    // The fill byte is a `char` (U+0000..U+00FF): its UTF-8 encoding.
    let mut fill_buf = [0u8; 4];
    let fill = char::from(spec.fill).encode_utf8(&mut fill_buf).as_bytes();
    let align = spec.align.unwrap_or(match v {
        Val::Int(_) => Align::Right,
        Val::Str(_) | Val::Bool(_) => Align::Left,
    });
    match align {
        Align::Left => {
            sink(core);
            repeat(sink, fill, missing);
        }
        Align::Right => {
            repeat(sink, fill, missing);
            sink(core);
        }
        Align::Center => {
            let l = missing / 2;
            repeat(sink, fill, l);
            sink(core);
            repeat(sink, fill, missing - l);
        }
    }
}

fn repeat(sink: &mut dyn FnMut(&[u8]), unit: &[u8], n: usize) {
    for _ in 0..n {
        sink(unit);
    }
}
