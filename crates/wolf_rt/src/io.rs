//! The s38 io surface: stream-parameterized write shims with
//! comptime-compiled format specs.
//!
//! `wolf_codegen_clif` emits calls to these names for every print
//! segment that carries a format spec, every stderr (`eprint`)
//! segment, and every float segment; the plain stdout no-spec
//! segments keep the frozen s31 `__wolf_rt_print_*` contract in
//! [`crate::native`]. Like those, every call flushes — the process
//! exits through the C entry shim, so no Rust exit hook drains a
//! buffer.
//!
//! # The packed-spec contract
//!
//! A format spec is comptime-known (D26): the compiler parses and
//! validates it (`wolf_sema::fmtspec`), then passes it here PACKED in
//! one `i64` immediate — no runtime spec parsing exists anywhere. The
//! bit layout is mirrored by hand from `wolf_sema::fmtspec` (this
//! crate may depend on nothing in the compiler, D15); the driver's
//! `fmt_parity` test pins the two against each other, and the
//! rendering below must stay byte-identical with the checked
//! executor's (`wolf_sema::fmtspec::apply`) — semantics
//! member-by-member the wolf-std sc05 reference functions:
//!
//! ```text
//! bits  0..8   fill byte (0 = no spec at all; ' ' = default)
//! bits  8..10  align: 0 default, 1 left, 2 center, 3 right
//! bit   10     sign ('+')
//! bit   11     zero ('0')
//! bit   12     has_width
//! bit   13     has_precision
//! bit   14     value is unsigned (int holes only)
//! bits 16..32  width
//! bits 32..48  precision
//! bits 48..52  kind: 0 none, 1 b, 2 o, 3 x, 4 X, 5 e, 6 E, 7 f
//! ```
//!
//! Width is a minimum in BYTES (D25); default alignment left for
//! `str`/`bool`, right for numbers; `^` puts the odd fill byte RIGHT;
//! `0` pads after the sign; `.N` on a `str` truncates to the largest
//! code-point boundary at or below `N`; with no `type` an `f64`
//! renders as the shortest round-trip decimal, positional while the
//! leading digit's decimal exponent is in `-4..=16`, scientific
//! (`1e+23`, signed two-digit exponent) outside it.

use std::io::Write as _;

/// Stream selector for the write shims: 1 = stdout, 2 = stderr
/// (POSIX fd spelling; a symbolic constant, not an actual fd).
pub const STREAM_STDOUT: i64 = 1;
pub const STREAM_STDERR: i64 = 2;

// ------------------------------------------------- the packed spec ----

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Bin,
    Oct,
    Hex,
    HexUpper,
    Exp,
    ExpUpper,
    Fixed,
}

/// The unpacked spec (mirror of `wolf_sema::fmtspec::FormatSpec`).
#[derive(Clone, Copy, Debug, Default)]
struct Spec {
    fill: Option<u8>,
    align: Option<Align>,
    sign: bool,
    zero: bool,
    width: Option<u16>,
    precision: Option<u16>,
    kind: Option<Kind>,
    unsigned: bool,
}

fn unpack(packed: i64) -> Spec {
    let fill = (packed & 0xff) as u8;
    Spec {
        fill: if fill == b' ' || fill == 0 {
            None
        } else {
            Some(fill)
        },
        align: match (packed >> 8) & 0b11 {
            1 => Some(Align::Left),
            2 => Some(Align::Center),
            3 => Some(Align::Right),
            _ => None,
        },
        sign: (packed >> 10) & 1 == 1,
        zero: (packed >> 11) & 1 == 1,
        width: ((packed >> 12) & 1 == 1).then_some(((packed >> 16) & 0xffff) as u16),
        precision: ((packed >> 13) & 1 == 1).then_some(((packed >> 32) & 0xffff) as u16),
        kind: match (packed >> 48) & 0xf {
            1 => Some(Kind::Bin),
            2 => Some(Kind::Oct),
            3 => Some(Kind::Hex),
            4 => Some(Kind::HexUpper),
            5 => Some(Kind::Exp),
            6 => Some(Kind::ExpUpper),
            7 => Some(Kind::Fixed),
            _ => None,
        },
        unsigned: (packed >> 14) & 1 == 1,
    }
}

// ---------------------------------------------------- the rendering ---
//
// Byte-identical with `wolf_sema::fmtspec` — every branch here has a
// twin there, and the parity test walks both over the same vectors.

enum Val<'a> {
    Str(&'a str),
    Bool(bool),
    Int(i64),
    F64(f64),
}

fn render(v: Val<'_>, spec: &Spec) -> String {
    let core = match v {
        Val::Str(s) => match spec.precision {
            Some(p) => {
                let mut end = (p as usize).min(s.len());
                while !s.is_char_boundary(end) {
                    end -= 1;
                }
                s[..end].to_string()
            }
            None => s.to_string(),
        },
        Val::Bool(b) => b.to_string(),
        Val::Int(n) => render_int(n, spec),
        Val::F64(x) => render_f64(x, spec),
    };
    pad(core, &v, spec)
}

fn render_int(n: i64, spec: &Spec) -> String {
    let (neg, mag) = if spec.unsigned {
        (false, n as u64)
    } else {
        (n < 0, n.unsigned_abs())
    };
    let digits = match spec.kind {
        None => mag.to_string(),
        Some(Kind::Bin) => format!("{mag:b}"),
        Some(Kind::Oct) => format!("{mag:o}"),
        Some(Kind::Hex) => format!("{mag:x}"),
        Some(Kind::HexUpper) => format!("{mag:X}"),
        Some(Kind::Exp | Kind::ExpUpper | Kind::Fixed) => mag.to_string(),
    };
    let sign = if neg {
        "-"
    } else if spec.sign {
        "+"
    } else {
        ""
    };
    format!("{sign}{digits}")
}

fn render_f64(x: f64, spec: &Spec) -> String {
    if x.is_nan() {
        return "nan".to_string();
    }
    if x.is_infinite() {
        return match (x < 0.0, spec.sign) {
            (true, _) => "-inf".to_string(),
            (false, true) => "+inf".to_string(),
            (false, false) => "inf".to_string(),
        };
    }
    let plus = spec.sign && !x.is_sign_negative();
    let body = match spec.kind {
        None => f64_shortest(x),
        Some(Kind::Fixed) => {
            let prec = spec.precision.unwrap_or(6) as usize;
            format!("{x:.prec$}")
        }
        Some(Kind::Exp) | Some(Kind::ExpUpper) => {
            let prec = spec.precision.unwrap_or(6) as usize;
            let upper = matches!(spec.kind, Some(Kind::ExpUpper));
            let s = format!("{x:.prec$e}");
            let (m, e) = s.split_once('e').expect("exp form");
            let exp: i32 = e.parse().expect("exp digits");
            let (es, mag) = if exp < 0 {
                ('-', -(exp as i64))
            } else {
                ('+', exp as i64)
            };
            let letter = if upper { 'E' } else { 'e' };
            format!("{m}{letter}{es}{mag:02}")
        }
        Some(_) => f64_shortest(x),
    };
    // Bare precision means fixed (`{x:>8.2}`).
    let body = if spec.kind.is_none() && spec.precision.is_some() {
        let prec = spec.precision.unwrap_or(6) as usize;
        format!("{x:.prec$}")
    } else {
        body
    };
    if plus { format!("+{body}") } else { body }
}

/// The SHORTEST round-trip decimal in the wolf-std
/// `std.fmt.decimal.to_str` layout (positional for leading-digit
/// exponent in `-4..=16`, else `d[.ddd]e±EE`).
pub fn f64_shortest(x: f64) -> String {
    if x.is_nan() {
        return "nan".to_string();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-inf" } else { "inf" }.to_string();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0" } else { "0" }.to_string();
    }
    let s = format!("{x:e}");
    let (m, e) = s.split_once('e').expect("exp form");
    let exponent: i64 = e.parse().expect("exp digits");
    let (sign, m) = match m.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", m),
    };
    let digits: String = m.chars().filter(|&c| c != '.').collect();
    render_shortest(sign, &digits, exponent)
}

fn render_shortest(sign: &str, digits: &str, exponent: i64) -> String {
    if !(-4..=16).contains(&exponent) {
        return render_exp(sign, digits, exponent);
    }
    let count = digits.len() as i64;
    if exponent < 0 {
        let pad = "0".repeat((-exponent - 1) as usize);
        return format!("{sign}0.{pad}{digits}");
    }
    if exponent >= count - 1 {
        let pad = "0".repeat((exponent - count + 1) as usize);
        return format!("{sign}{digits}{pad}");
    }
    let (head, tail) = digits.split_at((exponent + 1) as usize);
    format!("{sign}{head}.{tail}")
}

fn render_exp(sign: &str, digits: &str, exponent: i64) -> String {
    let mantissa = if digits.len() > 1 {
        format!("{}.{}", &digits[..1], &digits[1..])
    } else {
        digits.to_string()
    };
    let (es, mag) = if exponent < 0 {
        ('-', -exponent)
    } else {
        ('+', exponent)
    };
    format!("{sign}{mantissa}e{es}{mag:02}")
}

fn pad(core: String, v: &Val<'_>, spec: &Spec) -> String {
    let width = spec.width.unwrap_or(0) as usize;
    if width <= core.len() {
        return core;
    }
    let missing = width - core.len();
    if spec.zero {
        let finite = match v {
            Val::F64(x) => x.is_finite(),
            _ => true,
        };
        if finite {
            let (sign, rest) = match core.strip_prefix(['-', '+']) {
                Some(rest) => (&core[..1], rest),
                None => ("", core.as_str()),
            };
            return format!("{sign}{}{rest}", "0".repeat(missing));
        }
        return format!("{}{core}", " ".repeat(missing));
    }
    let fill = spec.fill.unwrap_or(b' ') as char;
    let align = spec.align.unwrap_or(match v {
        Val::Int(_) | Val::F64(_) => Align::Right,
        Val::Str(_) | Val::Bool(_) => Align::Left,
    });
    let fills = |n: usize| fill.to_string().repeat(n);
    match align {
        Align::Left => format!("{core}{}", fills(missing)),
        Align::Right => format!("{}{core}", fills(missing)),
        Align::Center => {
            let l = missing / 2;
            format!("{}{core}{}", fills(l), fills(missing - l))
        }
    }
}

// ------------------------------------------------- parity test API ----
//
// The driver's `fmt_parity` test renders the same vectors through
// `wolf_sema::fmtspec::apply` and through these, pinning the packed
// layout and every rendering branch against the compiler's reference.
// Not part of the symbol contract.

#[doc(hidden)]
pub fn render_str_packed(s: &str, packed: i64) -> String {
    render(Val::Str(s), &unpack(packed))
}

#[doc(hidden)]
pub fn render_bool_packed(b: bool, packed: i64) -> String {
    render(Val::Bool(b), &unpack(packed))
}

#[doc(hidden)]
pub fn render_i64_packed(v: i64, packed: i64) -> String {
    render(Val::Int(v), &unpack(packed))
}

#[doc(hidden)]
pub fn render_f64_packed(v: f64, packed: i64) -> String {
    render(Val::F64(v), &unpack(packed))
}

// ----------------------------------------------------- the shims ------

fn write_stream(stream: i64, bytes: &[u8]) {
    if stream == STREAM_STDERR {
        let mut err = std::io::stderr().lock();
        let _ = err.write_all(bytes);
        let _ = err.flush();
    } else {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(bytes);
        let _ = out.flush();
    }
}

/// Write `len` bytes at `ptr` to `stream`, formatted under the packed
/// `spec` (0 = raw), flushed.
///
/// # Safety
///
/// `ptr` must point at `len` readable bytes of valid UTF-8 (compiled
/// wolf code passes rodata or checked str values only).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_write_str(stream: i64, ptr: *const u8, len: i64, spec: i64) {
    if ptr.is_null() || len < 0 {
        return;
    }
    let bytes = if len == 0 {
        &[][..]
    } else {
        unsafe { core::slice::from_raw_parts(ptr, len as usize) }
    };
    if spec == 0 {
        write_stream(stream, bytes);
        return;
    }
    let s = String::from_utf8_lossy(bytes);
    let out = render(Val::Str(&s), &unpack(spec));
    write_stream(stream, out.as_bytes());
}

/// Write a 64-bit integer to `stream` under the packed `spec`
/// (0 = plain decimal), flushed. Bit 14 of the spec marks the value
/// unsigned.
///
/// # Safety
///
/// Callable from any thread at any time; takes no pointers.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_write_i64(stream: i64, v: i64, spec: i64) {
    let out = if spec == 0 {
        v.to_string()
    } else {
        render(Val::Int(v), &unpack(spec))
    };
    write_stream(stream, out.as_bytes());
}

/// Write `true`/`false` to `stream` under the packed `spec`
/// (0 = raw), flushed. `i8` — WIR bools cross the boundary as one
/// byte (only the low byte is defined under SysV).
///
/// # Safety
///
/// Callable from any thread at any time; takes no pointers.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_write_bool(stream: i64, v: i8, spec: i64) {
    let out = if spec == 0 {
        (v != 0).to_string()
    } else {
        render(Val::Bool(v != 0), &unpack(spec))
    };
    write_stream(stream, out.as_bytes());
}

/// Write an `f64` to `stream` under the packed `spec` (0 = the
/// shortest-round-trip rendering), flushed.
///
/// # Safety
///
/// Callable from any thread at any time; takes no pointers.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_write_f64(stream: i64, v: f64, spec: i64) {
    let out = if spec == 0 {
        f64_shortest(v)
    } else {
        render(Val::F64(v), &unpack(spec))
    };
    write_stream(stream, out.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    // A hand-rolled packer for the tests only (the compiler side owns
    // the real one; the driver's fmt_parity test pins both).
    #[allow(clippy::too_many_arguments)]
    fn pack(
        fill: Option<u8>,
        align: Option<Align>,
        sign: bool,
        zero: bool,
        width: Option<u16>,
        precision: Option<u16>,
        kind: Option<Kind>,
        unsigned: bool,
    ) -> i64 {
        let mut p: i64 = i64::from(fill.unwrap_or(b' '));
        p |= (match align {
            None => 0i64,
            Some(Align::Left) => 1,
            Some(Align::Center) => 2,
            Some(Align::Right) => 3,
        }) << 8;
        if sign {
            p |= 1 << 10;
        }
        if zero {
            p |= 1 << 11;
        }
        if let Some(w) = width {
            p |= 1 << 12;
            p |= i64::from(w) << 16;
        }
        if let Some(pr) = precision {
            p |= 1 << 13;
            p |= i64::from(pr) << 32;
        }
        p |= (match kind {
            None => 0i64,
            Some(Kind::Bin) => 1,
            Some(Kind::Oct) => 2,
            Some(Kind::Hex) => 3,
            Some(Kind::HexUpper) => 4,
            Some(Kind::Exp) => 5,
            Some(Kind::ExpUpper) => 6,
            Some(Kind::Fixed) => 7,
        }) << 48;
        if unsigned {
            p |= 1 << 14;
        }
        p
    }

    fn spec(packed: i64) -> Spec {
        unpack(packed)
    }

    #[test]
    fn padding_matches_the_reference() {
        let right8 = spec(pack(
            None,
            Some(Align::Right),
            false,
            false,
            Some(8),
            None,
            None,
            false,
        ));
        assert_eq!(render(Val::Str("hi"), &right8), "      hi");
        let star = spec(pack(
            Some(b'*'),
            Some(Align::Right),
            false,
            false,
            Some(8),
            None,
            None,
            false,
        ));
        assert_eq!(render(Val::Int(42), &star), "******42");
        let center4 = spec(pack(
            None,
            Some(Align::Center),
            false,
            false,
            Some(4),
            None,
            None,
            false,
        ));
        assert_eq!(render(Val::Str("a"), &center4), " a  "); // odd RIGHT
        // Defaults: numbers right, str left; width in bytes.
        let w6 = spec(pack(None, None, false, false, Some(6), None, None, false));
        assert_eq!(render(Val::Int(42), &w6), "    42");
        assert_eq!(render(Val::Str("é"), &w6), "é    ");
    }

    #[test]
    #[allow(clippy::approx_constant)] // 3.14159 is the #10 repro value, not π
    fn sign_zero_bases_and_precision_match_the_reference() {
        let plus = spec(pack(None, None, true, false, None, None, None, false));
        assert_eq!(render(Val::Int(0), &plus), "+0");
        let z6 = spec(pack(None, None, false, true, Some(6), None, None, false));
        assert_eq!(render(Val::Int(-42), &z6), "-00042");
        let hex = spec(pack(
            None,
            None,
            false,
            false,
            None,
            None,
            Some(Kind::Hex),
            false,
        ));
        assert_eq!(render(Val::Int(-255), &hex), "-ff");
        let p2 = spec(pack(None, None, false, false, None, Some(2), None, false));
        assert_eq!(render(Val::Str("aé"), &p2), "a"); // boundary honest
        assert_eq!(render(Val::F64(3.14159), &p2), "3.14"); // bare .N = fixed
        let e2 = spec(pack(
            None,
            None,
            false,
            false,
            None,
            Some(2),
            Some(Kind::Exp),
            false,
        ));
        assert_eq!(render(Val::F64(1.5), &e2), "1.50e+00");
    }

    #[test]
    #[allow(clippy::excessive_precision)] // the 17-digit pins ARE the point
    fn shortest_matches_decimal_to_str() {
        assert_eq!(f64_shortest(3.0), "3");
        assert_eq!(f64_shortest(0.1), "0.1");
        assert_eq!(f64_shortest(-0.0), "-0");
        assert_eq!(f64_shortest(9.999999999999999e22), "1e+23");
        assert_eq!(f64_shortest(1e16), "10000000000000000");
        assert_eq!(f64_shortest(1e17), "1e+17");
        assert_eq!(f64_shortest(1e-5), "1e-05");
    }
}
