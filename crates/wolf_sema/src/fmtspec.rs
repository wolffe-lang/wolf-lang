//! The format-spec mini-language (s38, D26/X9 — the §7.4 amendment
//! candidate filed as wolf-lang#28, made executable).
//!
//! Grammar, exactly the #28 candidate text:
//!
//! ```text
//! FORMAT_SPEC ::= [[fill] align] [sign] ['0'] [width] ['.' precision] [type]
//! fill        ::= <any single byte at v1>
//! align       ::= '<' | '^' | '>'
//! sign        ::= '+'
//! width       ::= DIGIT+
//! precision   ::= DIGIT+
//! type        ::= 'b' | 'o' | 'x' | 'X' | 'e' | 'E' | 'f'
//! ```
//!
//! Every spec in an f-string is comptime-known (the grammar admits no
//! computed spec), so this module runs at COMPILE TIME only: sema
//! parses and validates every spec (malformed → E0412, mismatched
//! against the hole's type → E0413), the checked executor renders
//! through [`apply`], and the native lane passes the [`pack`]ed spec to
//! the `wolf_rt` shims as an immediate. **No runtime spec parsing
//! exists anywhere.**
//!
//! Semantics (member-by-member the wolf-std sc05 reference functions —
//! `std.fmt.pad_left`/`center`/`with_sign`/`to_hex`/…,
//! `std.fmt.decimal.to_str`/`to_str_fixed`/`to_str_exp`):
//! - `width` is a minimum in BYTES (D25's currency); a value at least
//!   that wide is never truncated. Padding `fill` defaults to a space.
//! - Default alignment: LEFT for `str`/`bool`, RIGHT for numbers.
//! - `^` centering puts the odd fill byte on the RIGHT
//!   (`center("a", 4) == " a  "` — the wolf-std pin).
//! - `'0'` zero-pads AFTER the sign (`{-42:06}` is `-00042`), and
//!   combining it with an explicit fill/align is an error (E0412),
//!   never a silent pick. `{n:08}` therefore zero-pads — lupin's
//!   absorb-into-width reading was a bug under any §7.4 (#28 item 3).
//! - `'+'` prints `+` on non-negative numbers; zero takes `+`; for
//!   `f64` it applies to `+0.0` and never hides `-0.0`.
//! - `precision` on `f64` is the digit count after the point for
//!   `f`/`e`/`E`; on `str` it is a maximum length in BYTES that never
//!   splits a code point (truncate to the largest boundary at or below
//!   it). Precision on an integer or bool is a type mismatch.
//! - `type`: `b`/`o`/`x`/`X` are sign-magnitude integer renderings
//!   without a prefix (`-255` in `x` is `-ff`); `e`/`E` are `%e`-style
//!   with a signed, at-least-two-digit exponent (`1.50e+00`); `f` is
//!   fixed-point. Missing precision under `e`/`E`/`f` defaults to 6
//!   (the C default — recorded in the sprint notes).
//! - With no `type`, an `f64` renders as the SHORTEST decimal that
//!   reads back as the same bits, positional while the decimal
//!   exponent of the leading digit is in `-4..=16`, scientific outside
//!   it — `std.fmt.decimal.to_str`'s layout rule, line for line.
//! - Non-finite floats render `nan`/`inf`/`-inf`; the sign flag marks
//!   `+inf` but never `+nan`; zero-padding falls back to spaces for
//!   them (there is no digit run to extend).
//!
//! v1 bounds, diagnosed rather than guessed: `fill` is a single BYTE
//! (a multi-byte fill cannot hit an exact byte width — the same
//! contract wolf-std's `pad_left_with` enforces with an `assert`
//! trap), and `width`/`precision` cap at 65535.

/// Alignment inside a padded field.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// The `type` selector: base or float notation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Bin,
    Oct,
    Hex,
    HexUpper,
    Exp,
    ExpUpper,
    Fixed,
}

impl Kind {
    /// The spec character that selects this kind.
    pub fn ch(self) -> char {
        match self {
            Kind::Bin => 'b',
            Kind::Oct => 'o',
            Kind::Hex => 'x',
            Kind::HexUpper => 'X',
            Kind::Exp => 'e',
            Kind::ExpUpper => 'E',
            Kind::Fixed => 'f',
        }
    }
}

/// A parsed, validated-shape format spec (all fields comptime-known).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FormatSpec {
    /// Fill byte; `None` means space. Present only with an alignment.
    pub fill: Option<u8>,
    /// Explicit alignment; `None` = the type's default.
    pub align: Option<Align>,
    /// `'+'` — print the sign on non-negative numbers.
    pub sign: bool,
    /// `'0'` — zero-pad after the sign.
    pub zero: bool,
    pub width: Option<u16>,
    pub precision: Option<u16>,
    pub kind: Option<Kind>,
}

impl FormatSpec {
    /// Is this the empty spec (`{x:}` — everything default)?
    pub fn is_default(&self) -> bool {
        *self == FormatSpec::default()
    }
}

/// A malformed spec: what is wrong, precisely enough for E0412's
/// message to teach the grammar.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SpecError {
    /// A byte the grammar has no place for, at `at` (byte offset into
    /// the spec text, after the `:`).
    Unexpected { at: usize, ch: char },
    /// `'0'` combined with an explicit fill/align — the spec must pick
    /// one spelling, never the compiler.
    ZeroWithAlign,
    /// `'.'` with no digits after it.
    MissingPrecision,
    /// A multi-byte fill cannot hit an exact byte width (D25).
    MultiByteFill(char),
    /// An alignment character used as the fill (`{x:>>8}`) — #28
    /// names this diagnosable: it reads as a typo, never as intent.
    AlignAsFill(char),
    /// width/precision beyond the v1 cap of 65535.
    TooWide { what: &'static str },
}

impl SpecError {
    pub fn message(&self) -> String {
        match self {
            SpecError::Unexpected { ch, .. } => format!(
                "`{ch}` has no place in a format spec — the grammar is \
                 `[[fill]align][+][0][width][.precision][type]` with type one of \
                 `b o x X e E f`"
            ),
            SpecError::ZeroWithAlign => "`0` zero-pads after the sign and cannot combine with an \
                                         explicit fill or alignment — spell one or the other"
                .to_string(),
            SpecError::MissingPrecision => {
                "`.` must be followed by the precision digits (like `.2`)".to_string()
            }
            SpecError::MultiByteFill(c) => format!(
                "`{c}` is a multi-byte fill — width counts bytes (D25), so the fill \
                 must be a single byte"
            ),
            SpecError::AlignAsFill(c) => format!(
                "`{c}{c}` reads as a doubled alignment, not as \"fill with `{c}`\" — \
                 alignment characters cannot be the fill"
            ),
            SpecError::TooWide { what } => {
                format!("this {what} is larger than the 65535 cap")
            }
        }
    }
}

/// What a spec can apply to — the hole's type, classified.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HoleClass {
    Str,
    Bool,
    Int,
    Float,
}

impl HoleClass {
    pub fn describe(self) -> &'static str {
        match self {
            HoleClass::Str => "`str`",
            HoleClass::Bool => "`bool`",
            HoleClass::Int => "an integer",
            HoleClass::Float => "a float",
        }
    }
}

/// A well-formed spec that does not fit the hole's type (E0413).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mismatch {
    /// `+` on a non-number.
    SignOn(HoleClass),
    /// `0` on a non-number.
    ZeroOn(HoleClass),
    /// `.N` on an integer or bool.
    PrecisionOn(HoleClass),
    /// A base kind (`b o x X`) off integers, or a float kind
    /// (`e E f`) off floats.
    KindOn { kind: Kind, class: HoleClass },
}

impl Mismatch {
    pub fn message(&self) -> String {
        match self {
            Mismatch::SignOn(c) => format!(
                "`+` prints a sign on numbers, and this value is {} — remove it \
                 or format a number",
                c.describe()
            ),
            Mismatch::ZeroOn(c) => format!(
                "`0` zero-pads a number's digits, and this value is {}",
                c.describe()
            ),
            Mismatch::PrecisionOn(c) => format!(
                "`.precision` means digits-after-the-point on a float and a byte \
                 cap on a `str`; this value is {}",
                c.describe()
            ),
            Mismatch::KindOn { kind, class } => match kind {
                Kind::Bin | Kind::Oct | Kind::Hex | Kind::HexUpper => format!(
                    "`{}` renders an integer in another base, and this value is {}",
                    kind.ch(),
                    class.describe()
                ),
                Kind::Exp | Kind::ExpUpper | Kind::Fixed => format!(
                    "`{}` is a float notation, and this value is {}",
                    kind.ch(),
                    class.describe()
                ),
            },
        }
    }
}

/// Parse a spec's text (WITHOUT the leading `:`). The empty string is
/// the default spec.
pub fn parse(s: &str) -> Result<FormatSpec, SpecError> {
    let chars: Vec<char> = s.chars().collect();
    let mut spec = FormatSpec::default();
    let mut i = 0usize;
    let is_align = |c: char| matches!(c, '<' | '^' | '>');
    let to_align = |c: char| match c {
        '<' => Align::Left,
        '^' => Align::Center,
        _ => Align::Right,
    };
    // [[fill] align]
    if chars.len() >= 2 && is_align(chars[1]) {
        let fill = chars[0];
        if fill.len_utf8() != 1 {
            return Err(SpecError::MultiByteFill(fill));
        }
        if is_align(fill) {
            return Err(SpecError::AlignAsFill(fill));
        }
        spec.fill = Some(fill as u8);
        spec.align = Some(to_align(chars[1]));
        i = 2;
    } else if !chars.is_empty() && is_align(chars[0]) {
        spec.align = Some(to_align(chars[0]));
        i = 1;
    }
    // [sign]
    if i < chars.len() && chars[i] == '+' {
        spec.sign = true;
        i += 1;
    }
    // ['0'] — only when digits or end-of-field follow it as a FLAG;
    // a lone `0` is the flag with no width (harmless), `08` is
    // flag + width 8.
    if i < chars.len() && chars[i] == '0' {
        if spec.align.is_some() {
            return Err(SpecError::ZeroWithAlign);
        }
        spec.zero = true;
        i += 1;
    }
    // [width]
    let width_start = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i > width_start {
        let w: u32 = chars[width_start..i]
            .iter()
            .collect::<String>()
            .parse()
            .map_err(|_| SpecError::TooWide { what: "width" })?;
        spec.width = Some(u16::try_from(w).map_err(|_| SpecError::TooWide { what: "width" })?);
    }
    // ['.' precision]
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        let p_start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        if i == p_start {
            return Err(SpecError::MissingPrecision);
        }
        let p: u32 = chars[p_start..i]
            .iter()
            .collect::<String>()
            .parse()
            .map_err(|_| SpecError::TooWide { what: "precision" })?;
        spec.precision =
            Some(u16::try_from(p).map_err(|_| SpecError::TooWide { what: "precision" })?);
    }
    // [type]
    if i < chars.len() {
        spec.kind = Some(match chars[i] {
            'b' => Kind::Bin,
            'o' => Kind::Oct,
            'x' => Kind::Hex,
            'X' => Kind::HexUpper,
            'e' => Kind::Exp,
            'E' => Kind::ExpUpper,
            'f' => Kind::Fixed,
            ch => {
                return Err(SpecError::Unexpected {
                    at: chars[..i].iter().map(|c| c.len_utf8()).sum(),
                    ch,
                });
            }
        });
        i += 1;
    }
    if i < chars.len() {
        return Err(SpecError::Unexpected {
            at: chars[..i].iter().map(|c| c.len_utf8()).sum(),
            ch: chars[i],
        });
    }
    Ok(spec)
}

/// Does a well-formed spec fit a hole of this class?
pub fn validate(spec: &FormatSpec, class: HoleClass) -> Result<(), Mismatch> {
    let numeric = matches!(class, HoleClass::Int | HoleClass::Float);
    if spec.sign && !numeric {
        return Err(Mismatch::SignOn(class));
    }
    if spec.zero && !numeric {
        return Err(Mismatch::ZeroOn(class));
    }
    if spec.precision.is_some() && !matches!(class, HoleClass::Str | HoleClass::Float) {
        return Err(Mismatch::PrecisionOn(class));
    }
    if let Some(kind) = spec.kind {
        let ok = match kind {
            Kind::Bin | Kind::Oct | Kind::Hex | Kind::HexUpper => class == HoleClass::Int,
            Kind::Exp | Kind::ExpUpper | Kind::Fixed => class == HoleClass::Float,
        };
        if !ok {
            return Err(Mismatch::KindOn { kind, class });
        }
    }
    Ok(())
}

// ------------------------------------------------------------ packing --
//
// The native lane carries a spec to the `wolf_rt` write shims as ONE
// i64 immediate. Layout (LSB up) — `wolf_rt::io` mirrors these
// constants by hand (it may depend on nothing in the compiler, D15);
// the driver's `fmt_parity` test pins the two against each other:
//
//   bits  0..8   fill byte (0 = spec absent entirely; ' ' = default)
//   bits  8..10  align: 0 default, 1 left, 2 center, 3 right
//   bit   10     sign ('+')
//   bit   11     zero ('0')
//   bit   12     has_width
//   bit   13     has_precision
//   bit   14     value is unsigned (int holes only)
//   bits 16..32  width
//   bits 32..48  precision
//   bits 48..52  kind: 0 none, 1 b, 2 o, 3 x, 4 X, 5 e, 6 E, 7 f

/// Bit 14 of the packed form: the hole's integer value is unsigned.
pub const PACK_UNSIGNED: i64 = 1 << 14;

/// Pack a spec for the native shims. Never 0 (fill defaults to `' '`),
/// so `spec == 0` means "no spec" at the shim boundary.
pub fn pack(spec: &FormatSpec) -> i64 {
    let mut p: i64 = i64::from(spec.fill.unwrap_or(b' '));
    p |= (match spec.align {
        None => 0i64,
        Some(Align::Left) => 1,
        Some(Align::Center) => 2,
        Some(Align::Right) => 3,
    }) << 8;
    if spec.sign {
        p |= 1 << 10;
    }
    if spec.zero {
        p |= 1 << 11;
    }
    if let Some(w) = spec.width {
        p |= 1 << 12;
        p |= i64::from(w) << 16;
    }
    if let Some(pr) = spec.precision {
        p |= 1 << 13;
        p |= i64::from(pr) << 32;
    }
    p |= (match spec.kind {
        None => 0i64,
        Some(Kind::Bin) => 1,
        Some(Kind::Oct) => 2,
        Some(Kind::Hex) => 3,
        Some(Kind::HexUpper) => 4,
        Some(Kind::Exp) => 5,
        Some(Kind::ExpUpper) => 6,
        Some(Kind::Fixed) => 7,
    }) << 48;
    p
}

// ---------------------------------------------------------- rendering --

/// A hole value at rendering time.
#[derive(Clone, Copy, Debug)]
pub enum FmtValue<'a> {
    Str(&'a str),
    Bool(bool),
    Int { v: i64, unsigned: bool },
    F64(f64),
}

impl FmtValue<'_> {
    pub fn class(&self) -> HoleClass {
        match self {
            FmtValue::Str(_) => HoleClass::Str,
            FmtValue::Bool(_) => HoleClass::Bool,
            FmtValue::Int { .. } => HoleClass::Int,
            FmtValue::F64(_) => HoleClass::Float,
        }
    }
}

/// Render a value under a spec. `Err` is a validation mismatch — the
/// checker rules those out at compile time; callers that can still see
/// one (an unresolved hole type the checker skipped) refuse honestly.
pub fn apply(spec: &FormatSpec, v: FmtValue<'_>) -> Result<String, Mismatch> {
    validate(spec, v.class())?;
    let core = match v {
        FmtValue::Str(s) => match spec.precision {
            Some(p) => {
                let mut end = (p as usize).min(s.len());
                while !s.is_char_boundary(end) {
                    end -= 1;
                }
                s[..end].to_string()
            }
            None => s.to_string(),
        },
        FmtValue::Bool(b) => b.to_string(),
        FmtValue::Int { v: n, unsigned } => render_int(n, unsigned, spec),
        FmtValue::F64(x) => render_f64(x, spec),
    };
    Ok(pad(core, v, spec))
}

/// Integer core: optional `+`, sign-magnitude base rendering.
fn render_int(n: i64, unsigned: bool, spec: &FormatSpec) -> String {
    let (neg, mag) = if unsigned {
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
        // validate() rules float kinds off integers.
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

/// Float core: shortest / fixed / exp per the kind, sign flag applied.
fn render_f64(x: f64, spec: &FormatSpec) -> String {
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
            let upper = spec.kind == Some(Kind::ExpUpper);
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
        // validate() rules base kinds off floats.
        Some(_) => f64_shortest(x),
    };
    // Shortest-mode `{f:.N}` — precision with no kind means fixed
    // (the #10 ask `{x:>8.2}` reads as fixed with 2 digits).
    let body = if spec.kind.is_none() && spec.precision.is_some() {
        let prec = spec.precision.unwrap_or(6) as usize;
        format!("{x:.prec$}")
    } else {
        body
    };
    if plus { format!("+{body}") } else { body }
}

/// The SHORTEST decimal that reads back as the same bits, laid out by
/// `std.fmt.decimal.to_str`'s rule: positional while the decimal
/// exponent of the leading digit is in `-4..=16`, scientific outside —
/// `1e+23`-shaped with a signed, two-digit-minimum exponent.
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
    // Rust's `{:e}` is the shortest round-trip mantissa (flt2dec
    // shortest mode) in `d[.ddd]e±E` form.
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

/// `sign` + `digits` laid out around the decimal point for a leading
/// digit at `10^exponent` — the wolf-std `render_shortest`, ported
/// line for line.
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

/// `d[.ddd]e±EE` — the wolf-std `render_exp`, exponent zero-padded to
/// at least two digits, sign always spelled.
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

/// Width/fill/alignment (and the zero-pad path) around a rendered
/// core. Width counts BYTES (D25).
fn pad(core: String, v: FmtValue<'_>, spec: &FormatSpec) -> String {
    let width = spec.width.unwrap_or(0) as usize;
    if width <= core.len() {
        return core;
    }
    let missing = width - core.len();
    // `'0'`: pad with zeros between the sign and the digits. A
    // non-finite float has no digit run — space-pad right-aligned
    // instead (the C posture).
    if spec.zero {
        let finite = match v {
            FmtValue::F64(x) => x.is_finite(),
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
    let align = spec.align.unwrap_or(match v.class() {
        HoleClass::Int | HoleClass::Float => Align::Right,
        HoleClass::Str | HoleClass::Bool => Align::Left,
    });
    let fills = |n: usize| fill.to_string().repeat(n);
    match align {
        Align::Left => format!("{core}{}", fills(missing)),
        Align::Right => format!("{}{core}", fills(missing)),
        Align::Center => {
            // Odd byte goes RIGHT (wolf-std `center`).
            let l = missing / 2;
            format!("{}{core}{}", fills(l), fills(missing - l))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> FormatSpec {
        parse(s).expect("spec parses")
    }

    fn fmt(s: &str, v: FmtValue<'_>) -> String {
        apply(&p(s), v).expect("spec fits")
    }

    fn int(v: i64) -> FmtValue<'static> {
        FmtValue::Int { v, unsigned: false }
    }

    #[test]
    fn grammar_parses_every_field() {
        assert_eq!(p(""), FormatSpec::default());
        assert_eq!(
            p("*>8"),
            FormatSpec {
                fill: Some(b'*'),
                align: Some(Align::Right),
                width: Some(8),
                ..FormatSpec::default()
            }
        );
        assert_eq!(
            p("+08.2f"),
            FormatSpec {
                sign: true,
                zero: true,
                width: Some(8),
                precision: Some(2),
                kind: Some(Kind::Fixed),
                ..FormatSpec::default()
            }
        );
        // A digit fill is legal when an align follows it.
        assert_eq!(p("0>4").fill, Some(b'0'));
        // `{n:08}` is the zero FLAG plus width 8 — never width 08
        // with space fill (#28 item 3, lupin's bug).
        let z = p("08");
        assert!(z.zero);
        assert_eq!(z.width, Some(8));
    }

    #[test]
    fn grammar_refuses_what_it_cannot_mean() {
        assert!(matches!(parse("q"), Err(SpecError::Unexpected { .. })));
        assert!(matches!(parse(">>8"), Err(SpecError::AlignAsFill('>'))));
        assert!(matches!(parse("8."), Err(SpecError::MissingPrecision)));
        assert!(matches!(parse("0>08"), Err(SpecError::ZeroWithAlign)));
        assert!(matches!(parse("é>4"), Err(SpecError::MultiByteFill('é'))));
        assert!(matches!(parse("99999"), Err(SpecError::TooWide { .. })));
        assert!(matches!(parse("4x2"), Err(SpecError::Unexpected { .. })));
    }

    #[test]
    fn mismatches_are_ruled_out() {
        assert!(validate(&p(".2"), HoleClass::Int).is_err());
        assert!(validate(&p("x"), HoleClass::Str).is_err());
        assert!(validate(&p("+"), HoleClass::Bool).is_err());
        assert!(validate(&p("08"), HoleClass::Str).is_err());
        assert!(validate(&p("e"), HoleClass::Int).is_err());
        assert!(validate(&p("x"), HoleClass::Float).is_err());
        assert!(validate(&p(".8"), HoleClass::Str).is_ok());
        assert!(validate(&p("+08.2f"), HoleClass::Float).is_ok());
    }

    #[test]
    fn padding_matches_wolf_std() {
        // pad_left / pad_right / center pins from std/fmt tests.
        assert_eq!(fmt(">8", FmtValue::Str("hi")), "      hi");
        assert_eq!(fmt("<8", FmtValue::Str("hi")), "hi      ");
        assert_eq!(fmt("^4", FmtValue::Str("a")), " a  "); // odd RIGHT
        assert_eq!(fmt("*>8", int(42)), "******42");
        // Never truncates.
        assert_eq!(fmt(">3", FmtValue::Str("wolves")), "wolves");
        // Width is BYTES: "é" is 2, "🐺" is 4.
        assert_eq!(fmt(">3", FmtValue::Str("é")), " é");
        assert_eq!(fmt(">4", FmtValue::Str("🐺")), "🐺");
        // Defaults: str/bool left, numbers right.
        assert_eq!(fmt("8", FmtValue::Str("hi")), "hi      ");
        assert_eq!(fmt("8", FmtValue::Bool(true)), "true    ");
        assert_eq!(fmt("6", int(42)), "    42");
    }

    #[test]
    fn sign_zero_and_bases_match_wolf_std() {
        // with_sign: zero takes `+`.
        assert_eq!(fmt("+", int(42)), "+42");
        assert_eq!(fmt("+", int(0)), "+0");
        assert_eq!(fmt("+", int(-42)), "-42");
        // Zero-pad AFTER the sign.
        assert_eq!(fmt("06", int(-42)), "-00042");
        assert_eq!(fmt("08", int(42)), "00000042");
        assert_eq!(fmt("+06", int(42)), "+00042");
        // Bases: sign-magnitude, no prefix (to_hex/-255 pin).
        assert_eq!(fmt("x", int(-255)), "-ff");
        assert_eq!(fmt("X", int(255)), "FF");
        assert_eq!(fmt("b", int(5)), "101");
        assert_eq!(fmt("o", int(8)), "10");
        // int_min renders (sign-magnitude survives it).
        assert_eq!(fmt("x", int(i64::MIN)), "-8000000000000000");
    }

    #[test]
    fn str_precision_respects_boundaries() {
        assert_eq!(fmt(".2", FmtValue::Str("wolf")), "wo");
        // Truncating inside "é" backs off to the boundary.
        assert_eq!(fmt(".1", FmtValue::Str("é")), "");
        assert_eq!(fmt(".3", FmtValue::Str("aé")), "aé");
        assert_eq!(fmt(".2", FmtValue::Str("aé")), "a");
        assert_eq!(fmt(">4.2", FmtValue::Str("wolf")), "  wo");
    }

    #[test]
    #[allow(clippy::approx_constant)] // 3.14159 is the #10 repro value, not π
    fn float_fixed_and_exp_match_decimal() {
        // to_str_fixed pins: exact value, round-half-even.
        assert_eq!(fmt(".0f", FmtValue::F64(0.5)), "0");
        assert_eq!(fmt(".0f", FmtValue::F64(1.5)), "2");
        assert_eq!(fmt(".0f", FmtValue::F64(2.5)), "2");
        assert_eq!(fmt(".20f", FmtValue::F64(0.1)), "0.10000000000000000555");
        assert_eq!(fmt(".2f", FmtValue::F64(-0.0)), "-0.00");
        // to_str_exp pins.
        assert_eq!(fmt(".2e", FmtValue::F64(1.5)), "1.50e+00");
        assert_eq!(fmt(".0e", FmtValue::F64(1.5)), "2e+00");
        assert_eq!(fmt(".2e", FmtValue::F64(0.0)), "0.00e+00");
        assert_eq!(fmt(".2E", FmtValue::F64(1.5)), "1.50E+00");
        // Bare precision means fixed (the #10 headline `{x:>8.2}`).
        assert_eq!(fmt(">8.2", FmtValue::F64(3.14159)), "    3.14");
        // Sign + zero-pad on floats.
        assert_eq!(fmt("+.2f", FmtValue::F64(0.0)), "+0.00");
        assert_eq!(fmt("08.2f", FmtValue::F64(-1.5)), "-0001.50");
    }

    #[test]
    #[allow(clippy::excessive_precision)] // the 17-digit pins ARE the point
    fn float_shortest_matches_decimal_to_str() {
        // to_str pins from std/fmt/decimal.
        assert_eq!(f64_shortest(3.0), "3");
        assert_eq!(f64_shortest(0.1), "0.1");
        assert_eq!(f64_shortest(-0.0), "-0");
        assert_eq!(f64_shortest(9.999999999999999e22), "1e+23");
        assert_eq!(f64_shortest(1e16), "10000000000000000");
        assert_eq!(f64_shortest(1e17), "1e+17");
        assert_eq!(f64_shortest(1e-4), "0.0001");
        assert_eq!(f64_shortest(1e-5), "1e-05");
        assert_eq!(f64_shortest(1.5), "1.5");
        assert_eq!(f64_shortest(f64::NAN), "nan");
        assert_eq!(f64_shortest(f64::INFINITY), "inf");
        assert_eq!(f64_shortest(f64::NEG_INFINITY), "-inf");
        // Round-trip: shortest reads back to the same bits.
        for x in [0.3, 2.5e-12, 123456.789, 1.7976931348623157e308] {
            assert_eq!(
                f64_shortest(x).parse::<f64>().unwrap().to_bits(),
                x.to_bits()
            );
        }
    }

    #[test]
    fn nonfinite_specs_stay_honest() {
        assert_eq!(fmt("+.2f", FmtValue::F64(f64::INFINITY)), "+inf");
        assert_eq!(fmt(".2f", FmtValue::F64(f64::NAN)), "nan");
        // Zero-padding a non-finite falls back to spaces.
        assert_eq!(fmt("06.2f", FmtValue::F64(f64::NAN)), "   nan");
    }

    #[test]
    fn packing_round_trips_the_fields() {
        let s = p("*>8.2f");
        let packed = pack(&s);
        assert_eq!(packed & 0xff, i64::from(b'*'));
        assert_eq!((packed >> 8) & 0b11, 3); // right
        assert_eq!((packed >> 12) & 1, 1); // has width
        assert_eq!((packed >> 16) & 0xffff, 8);
        assert_eq!((packed >> 13) & 1, 1); // has precision
        assert_eq!((packed >> 32) & 0xffff, 2);
        assert_eq!((packed >> 48) & 0xf, 7); // f
        assert_ne!(pack(&FormatSpec::default()), 0); // 0 ⇔ absent
    }
}
