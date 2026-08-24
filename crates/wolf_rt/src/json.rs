//! The s107 native json runtime — the `json_*` builtin tier's
//! runtime half (RFC 8259 query kernels; c26's last crossing,
//! wolf-lang#118).
//!
//! # Where the ONE parser lives (the drift question, answered)
//!
//! The checked lane's reference implementation is `wolf_mem::json`
//! (its module doc pins parse, rendering, and error semantics). This
//! crate may not reach it: the crate graph is locked and wolf_rt
//! depends on nothing above `wolf_span` (D15) — so this module is the
//! HAND MIRROR, branch for branch, and the driver's `json_parity`
//! test pins the two against a shared vector battery, exactly the
//! `fmtspec` precedent (`wolf_rt::io` mirrors `wolf_sema::fmtspec`;
//! `net::err_tag` mirrors `ubcheck::net_err_tag`). Change semantics
//! in either copy and json_parity is what tells you the other copy
//! did not move.
//!
//! Scope is the query-kernel tier std.x.json wraps (D31): full RFC
//! 8259 parse (every escape, surrogate pairs, explicit [`MAX_DEPTH`]);
//! dotted-path queries (digit segments index arrays, others key
//! objects, `""` is the root); rendering (strings decode, numbers
//! keep their SOURCE spelling, containers render as their raw slice
//! at the root and re-render nested). No DOM handle table exists at
//! this tier: the declared surface is four stateless entries over the
//! source text (`ubcheck`'s json arm is the authority), so a parse
//! cache would be surface the checked lane does not declare.
//!
//! PURE — the one builtin family with no capability tag, and the one
//! shim family with no process-global state.
//!
//! Errors are three honest kinds mapped to codes ([`json_code`]);
//! lowering turns codes into the declared D30 rows (`parse`,
//! `missing`, `kind`) — the runtime never sees a tag name.

use crate::str::{ambient_copy, view, write_pair};

/// Maximum container nesting the parser will descend (RFC 8259 §9;
/// mirror of `wolf_mem::json::MAX_DEPTH`).
pub const MAX_DEPTH: usize = 128;

/// Why a json operation failed (mirror of `wolf_mem::json::JsonErr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonErr {
    /// The text violates RFC 8259 (or exceeds [`MAX_DEPTH`]).
    Parse,
    /// The path addresses no node.
    Missing,
    /// The addressed node has the wrong kind for the operation.
    Kind,
}

/// A parsed value: kind plus the byte span of its source text.
/// Strings carry their decoded form; containers carry children.
#[derive(Debug, Clone)]
enum Val {
    Null,
    Bool(bool),
    /// Numbers keep their source text (exactness is the source's).
    Num {
        lo: usize,
        hi: usize,
    },
    Str(String),
    Arr(Vec<Val>),
    /// Declaration order preserved (query semantics + determinism).
    Obj(Vec<(String, Val)>),
}

/// One value with the span of its raw text (containers render as the
/// source slice between these bounds).
#[derive(Debug, Clone)]
struct Node {
    val: Val,
    lo: usize,
    hi: usize,
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        while let Some(&c) = self.b.get(self.i) {
            // Exactly RFC 8259's insignificant whitespace set.
            if matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn lit(&mut self, s: &str) -> Result<(), JsonErr> {
        if self.b[self.i..].starts_with(s.as_bytes()) {
            self.i += s.len();
            Ok(())
        } else {
            Err(JsonErr::Parse)
        }
    }

    fn value(&mut self, depth: usize) -> Result<Node, JsonErr> {
        if depth > MAX_DEPTH {
            return Err(JsonErr::Parse);
        }
        self.ws();
        let lo = self.i;
        let val = match self.b.get(self.i).copied().ok_or(JsonErr::Parse)? {
            b'n' => {
                self.lit("null")?;
                Val::Null
            }
            b't' => {
                self.lit("true")?;
                Val::Bool(true)
            }
            b'f' => {
                self.lit("false")?;
                Val::Bool(false)
            }
            b'"' => Val::Str(self.string()?),
            b'{' => {
                self.i += 1;
                let mut members = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b'}') {
                    self.i += 1;
                } else {
                    loop {
                        self.ws();
                        if self.b.get(self.i) != Some(&b'"') {
                            return Err(JsonErr::Parse);
                        }
                        let key = self.string()?;
                        self.ws();
                        if self.b.get(self.i) != Some(&b':') {
                            return Err(JsonErr::Parse);
                        }
                        self.i += 1;
                        let v = self.value(depth + 1)?;
                        members.push((key, v.val));
                        self.ws();
                        match self.b.get(self.i) {
                            Some(&b',') => self.i += 1,
                            Some(&b'}') => {
                                self.i += 1;
                                break;
                            }
                            _ => return Err(JsonErr::Parse),
                        }
                    }
                }
                Val::Obj(members)
            }
            b'[' => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b']') {
                    self.i += 1;
                } else {
                    loop {
                        let v = self.value(depth + 1)?;
                        items.push(v.val);
                        self.ws();
                        match self.b.get(self.i) {
                            Some(&b',') => self.i += 1,
                            Some(&b']') => {
                                self.i += 1;
                                break;
                            }
                            _ => return Err(JsonErr::Parse),
                        }
                    }
                }
                Val::Arr(items)
            }
            b'-' | b'0'..=b'9' => {
                self.number()?;
                Val::Num { lo, hi: self.i }
            }
            _ => return Err(JsonErr::Parse),
        };
        Ok(Node {
            val,
            lo,
            hi: self.i,
        })
    }

    /// RFC 8259 §6: `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`.
    fn number(&mut self) -> Result<(), JsonErr> {
        if self.b.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        match self.b.get(self.i).copied() {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.b.get(self.i), Some(b'0'..=b'9')) {
                    self.i += 1;
                }
            }
            _ => return Err(JsonErr::Parse),
        }
        if self.b.get(self.i) == Some(&b'.') {
            self.i += 1;
            if !matches!(self.b.get(self.i), Some(b'0'..=b'9')) {
                return Err(JsonErr::Parse);
            }
            while matches!(self.b.get(self.i), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        if matches!(self.b.get(self.i), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.b.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if !matches!(self.b.get(self.i), Some(b'0'..=b'9')) {
                return Err(JsonErr::Parse);
            }
            while matches!(self.b.get(self.i), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        Ok(())
    }

    /// RFC 8259 §7: a quoted string, escapes decoded, `\uXXXX`
    /// including surrogate pairs; unpaired surrogates and raw control
    /// characters are `Parse` errors.
    fn string(&mut self) -> Result<String, JsonErr> {
        debug_assert_eq!(self.b.get(self.i), Some(&b'"'));
        self.i += 1;
        let mut out = String::new();
        loop {
            let c = self.b.get(self.i).copied().ok_or(JsonErr::Parse)?;
            match c {
                b'"' => {
                    self.i += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.i += 1;
                    let e = self.b.get(self.i).copied().ok_or(JsonErr::Parse)?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let ch = if (0xD800..0xDC00).contains(&hi) {
                                // High surrogate: require `\uXXXX` low.
                                if self.b.get(self.i) != Some(&b'\\')
                                    || self.b.get(self.i + 1) != Some(&b'u')
                                {
                                    return Err(JsonErr::Parse);
                                }
                                self.i += 2;
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(JsonErr::Parse);
                                }
                                let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                                char::from_u32(cp).ok_or(JsonErr::Parse)?
                            } else if (0xDC00..0xE000).contains(&hi) {
                                return Err(JsonErr::Parse); // lone low
                            } else {
                                char::from_u32(hi).ok_or(JsonErr::Parse)?
                            };
                            out.push(ch);
                        }
                        _ => return Err(JsonErr::Parse),
                    }
                }
                0x00..=0x1F => return Err(JsonErr::Parse),
                _ => {
                    // One UTF-8 scalar (the input is a Rust str, so
                    // continuation structure is already valid).
                    let start = self.i;
                    self.i += 1;
                    while self.b.get(self.i).is_some_and(|&b| (b & 0xC0) == 0x80) {
                        self.i += 1;
                    }
                    out.push_str(
                        std::str::from_utf8(&self.b[start..self.i]).map_err(|_| JsonErr::Parse)?,
                    );
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, JsonErr> {
        let mut v = 0u32;
        for _ in 0..4 {
            let c = self.b.get(self.i).copied().ok_or(JsonErr::Parse)?;
            let d = (c as char).to_digit(16).ok_or(JsonErr::Parse)?;
            v = v * 16 + d;
            self.i += 1;
        }
        Ok(v)
    }
}

/// Parse a complete text: one value, insignificant whitespace around
/// it, nothing else.
fn parse(s: &str) -> Result<Node, JsonErr> {
    let mut p = Parser {
        b: s.as_bytes(),
        i: 0,
    };
    let n = p.value(0)?;
    p.ws();
    if p.i == s.len() {
        Ok(n)
    } else {
        Err(JsonErr::Parse)
    }
}

/// Walk `path` from the root: dotted segments, digits index arrays,
/// anything else keys objects. Returns the node's `Val` and its span.
fn walk(root: Node, path: &str) -> Result<(Val, usize, usize), JsonErr> {
    let (mut v, mut lo, mut hi) = (root.val, root.lo, root.hi);
    if path.is_empty() {
        return Ok((v, lo, hi));
    }
    for seg in path.split('.') {
        if seg.is_empty() {
            return Err(JsonErr::Missing);
        }
        match v {
            Val::Arr(items) => {
                let ix: usize = seg.parse().map_err(|_| JsonErr::Missing)?;
                let n = items.into_iter().nth(ix).ok_or(JsonErr::Missing)?;
                v = n;
                // Children of containers lose their own spans in the
                // DOM; recompute below only for the root render path.
                (lo, hi) = (0, 0);
            }
            Val::Obj(members) => {
                let hit = members
                    .into_iter()
                    .find(|(k, _)| k == seg)
                    .ok_or(JsonErr::Missing)?;
                v = hit.1;
                (lo, hi) = (0, 0);
            }
            _ => return Err(JsonErr::Missing),
        }
    }
    Ok((v, lo, hi))
}

/// Is `s` one valid RFC 8259 text?
pub fn valid(s: &str) -> bool {
    parse(s).is_ok()
}

/// `json_type`: the addressed node's kind name — one of `object`,
/// `array`, `str`, `num`, `bool`, `null`.
pub fn type_of(s: &str, path: &str) -> Result<&'static str, JsonErr> {
    let (v, _, _) = walk(parse(s)?, path)?;
    Ok(match v {
        Val::Null => "null",
        Val::Bool(_) => "bool",
        Val::Num { .. } => "num",
        Val::Str(_) => "str",
        Val::Arr(_) => "array",
        Val::Obj(_) => "object",
    })
}

/// `json_len`: element count of an array, member count of an object;
/// scalars are the `Kind` error.
pub fn len_of(s: &str, path: &str) -> Result<i64, JsonErr> {
    let (v, _, _) = walk(parse(s)?, path)?;
    match v {
        Val::Arr(items) => Ok(items.len() as i64),
        Val::Obj(members) => Ok(members.len() as i64),
        _ => Err(JsonErr::Kind),
    }
}

/// `json_get`: the addressed node rendered as a str — strings decode,
/// numbers keep their source spelling, literals spell themselves,
/// containers render as canonical-ish source (their raw slice for the
/// root; re-rendered for nested containers, since the DOM drops child
/// spans — both are valid RFC 8259 texts).
pub fn get(s: &str, path: &str) -> Result<String, JsonErr> {
    let (v, lo, hi) = walk(parse(s)?, path)?;
    Ok(render(&v, s, lo, hi))
}

fn render(v: &Val, src: &str, lo: usize, hi: usize) -> String {
    match v {
        Val::Null => "null".to_string(),
        Val::Bool(true) => "true".to_string(),
        Val::Bool(false) => "false".to_string(),
        Val::Num { lo, hi } => src[*lo..*hi].to_string(),
        Val::Str(t) => t.clone(),
        Val::Arr(_) | Val::Obj(_) if hi > lo => src[lo..hi].trim().to_string(),
        Val::Arr(items) => {
            let inner: Vec<String> = items.iter().map(|i| render_json(i, src)).collect();
            format!("[{}]", inner.join(","))
        }
        Val::Obj(members) => {
            let inner: Vec<String> = members
                .iter()
                .map(|(k, m)| format!("{}:{}", quote(k), render_json(m, src)))
                .collect();
            format!("{{{}}}", inner.join(","))
        }
    }
}

/// Render a nested value as JSON text (strings re-quoted).
fn render_json(v: &Val, src: &str) -> String {
    match v {
        Val::Str(t) => quote(t),
        other => render(other, src, 0, 0),
    }
}

/// Quote + escape a string per RFC 8259 (the minimal escape set).
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ------------------------------------------- the s107 shim tier --

/// Error codes of the json family (lowering maps them to the declared
/// D30 rows; the runtime never sees a tag name — the fs/net
/// discipline).
pub mod json_code {
    pub const OK: i64 = 0;
    pub const PARSE: i64 = 1;
    pub const MISSING: i64 = 2;
    pub const KIND: i64 = 3;
}

/// A [`JsonErr`] as its wire code ([`json_code`]).
fn code_of(e: JsonErr) -> i64 {
    match e {
        JsonErr::Parse => json_code::PARSE,
        JsonErr::Missing => json_code::MISSING,
        JsonErr::Kind => json_code::KIND,
    }
}

/// `json_valid(s) -> bool` — 1 for one valid RFC 8259 text, else 0.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_json_valid(sp: i64, sl: i64) -> i64 {
    i64::from(valid(unsafe { view(sp, sl) }))
}

/// `json_get(s, path) -> str ! {parse, missing}` — the rendered node
/// through the out slot on code 0.
///
/// # Safety
///
/// Two valid str pairs; `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_json_get(sp: i64, sl: i64, pp: i64, pl: i64, out: i64) -> i64 {
    let (s, path) = unsafe { (view(sp, sl), view(pp, pl)) };
    match get(s, path) {
        Err(e) => code_of(e),
        Ok(v) => {
            let p = ambient_copy(v.as_bytes());
            unsafe { write_pair(out, p as i64, v.len() as i64) };
            json_code::OK
        }
    }
}

/// `json_type(s, path) -> str ! {parse, missing}` — the kind name
/// through the out slot on code 0.
///
/// # Safety
///
/// Two valid str pairs; `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_json_type(sp: i64, sl: i64, pp: i64, pl: i64, out: i64) -> i64 {
    let (s, path) = unsafe { (view(sp, sl), view(pp, pl)) };
    match type_of(s, path) {
        Err(e) => code_of(e),
        Ok(k) => {
            let p = ambient_copy(k.as_bytes());
            unsafe { write_pair(out, p as i64, k.len() as i64) };
            json_code::OK
        }
    }
}

/// `json_len(s, path) -> int ! {parse, missing, kind}` — the count
/// (>= 0), or `-code` (the `fs_open` convention: one i64 return
/// carries both; a json length is never negative).
///
/// # Safety
///
/// Two valid str pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_json_len(sp: i64, sl: i64, pp: i64, pl: i64) -> i64 {
    let (s, path) = unsafe { (view(sp, sl), view(pp, pl)) };
    match len_of(s, path) {
        Err(e) => -code_of(e),
        Ok(n) => n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The reference battery (mirrored from `wolf_mem::json`'s own
    // tests so a semantic edit fails HERE too, not only in the
    // driver's json_parity pin).

    #[test]
    fn rfc_smoke() {
        assert!(valid("null"));
        assert!(valid(" { \"a\" : [1, 2.5, -3e+2] , \"b\" : \"x\" } "));
        assert!(valid("[]"));
        assert!(valid("\"\""));
        assert!(!valid(""));
        assert!(!valid("{"));
        assert!(!valid("[1,]"));
        assert!(!valid("01"));
        assert!(!valid("1."));
        assert!(!valid(".5"));
        assert!(!valid("+1"));
        assert!(!valid("nul"));
        assert!(!valid("\"\u{0009}\"")); // raw control char
        assert!(!valid("[1] [2]")); // trailing content
        assert!(!valid("'a'"));
    }

    #[test]
    fn escapes_and_surrogates() {
        assert_eq!(get(r#""a\nb\t\"\\\/""#, "").unwrap(), "a\nb\t\"\\/");
        assert_eq!(get(r#""Aé""#, "").unwrap(), "Aé");
        // Surrogate pair: U+1F43A (wolf face).
        assert_eq!(get(r#""🐺""#, "").unwrap(), "\u{1F43A}");
        assert!(!valid(r#""\ud83d""#)); // lone high surrogate
        assert!(!valid(r#""\udc3a""#)); // lone low surrogate
        assert!(!valid(r#""\x41""#));
    }

    #[test]
    fn paths_and_kinds() {
        let s = r#"{"users":[{"name":"lupin","tags":[1,2,3]},{"name":"ainu"}],"n":42}"#;
        assert_eq!(get(s, "users.0.name").unwrap(), "lupin");
        assert_eq!(get(s, "users.1.name").unwrap(), "ainu");
        assert_eq!(get(s, "n").unwrap(), "42");
        assert_eq!(type_of(s, "users").unwrap(), "array");
        assert_eq!(type_of(s, "users.0").unwrap(), "object");
        assert_eq!(type_of(s, "n").unwrap(), "num");
        assert_eq!(len_of(s, "users").unwrap(), 2);
        assert_eq!(len_of(s, "users.0.tags").unwrap(), 3);
        assert_eq!(len_of(s, "").unwrap(), 2);
        assert_eq!(get(s, "users.9.name").unwrap_err(), JsonErr::Missing);
        assert_eq!(get(s, "absent").unwrap_err(), JsonErr::Missing);
        assert_eq!(get(s, "n.x").unwrap_err(), JsonErr::Missing);
        assert_eq!(len_of(s, "n").unwrap_err(), JsonErr::Kind);
        assert_eq!(get("{", "x").unwrap_err(), JsonErr::Parse);
    }

    #[test]
    fn numbers_render_as_source() {
        assert_eq!(get("1e3", "").unwrap(), "1e3");
        assert_eq!(get("-0.50", "").unwrap(), "-0.50");
        assert_eq!(
            get("9223372036854775808", "").unwrap(),
            "9223372036854775808" // beyond i64: exactness preserved
        );
    }

    #[test]
    fn depth_limit_is_a_parse_error() {
        let deep = "[".repeat(MAX_DEPTH + 2) + &"]".repeat(MAX_DEPTH + 2);
        assert!(!valid(&deep));
        let ok = "[".repeat(64) + &"]".repeat(64);
        assert!(valid(&ok));
    }

    #[test]
    fn container_get_renders_json() {
        let s = r#" {"a": [1, {"b": "c d"}]} "#;
        assert_eq!(get(s, "").unwrap(), r#"{"a": [1, {"b": "c d"}]}"#);
        // Nested containers re-render (child spans are the DOM's loss).
        assert_eq!(get(s, "a").unwrap(), r#"[1,{"b":"c d"}]"#);
    }

    // --------------------------- the extern surface, code for code --

    fn pair_of(s: &str) -> (i64, i64) {
        (s.as_ptr() as i64, s.len() as i64)
    }

    /// `corpus/json/query.lu`'s spine through the shims: codes out,
    /// str pairs through the out slot — the fs/net shape.
    #[test]
    fn shim_query_roundtrip() {
        let doc = r#"{"pack": [{"name": "lupin"}, {"name": "ainu"}], "n": 42, "a": [1, 2, 3], "b": null}"#;
        let (sp, sl) = pair_of(doc);
        let (vp, vl) = pair_of("[1, 2, 3]");
        assert_eq!(unsafe { __wolf_rt_json_valid(vp, vl) }, 1);
        let (bp, bl) = pair_of("{");
        assert_eq!(unsafe { __wolf_rt_json_valid(bp, bl) }, 0);
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        let (qp, ql) = pair_of("pack.0.name");
        assert_eq!(
            unsafe { __wolf_rt_json_get(sp, sl, qp, ql, o) },
            json_code::OK
        );
        assert_eq!(unsafe { view(out[0], out[1]) }, "lupin");
        let (tp, tl) = pair_of("a");
        assert_eq!(
            unsafe { __wolf_rt_json_type(sp, sl, tp, tl, o) },
            json_code::OK
        );
        assert_eq!(unsafe { view(out[0], out[1]) }, "array");
        assert_eq!(unsafe { __wolf_rt_json_len(sp, sl, tp, tl) }, 3);
        let (rp, rl) = pair_of("");
        assert_eq!(unsafe { __wolf_rt_json_len(sp, sl, rp, rl) }, 4);
    }

    /// `corpus/json/rows.lu`'s spine: each failure kind is its code,
    /// never a trap.
    #[test]
    fn shim_error_codes() {
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        let (bp, bl) = pair_of("{");
        let (xp, xl) = pair_of("x");
        assert_eq!(
            unsafe { __wolf_rt_json_get(bp, bl, xp, xl, o) },
            json_code::PARSE
        );
        let (ap, al) = pair_of("[1]");
        let (np, nl) = pair_of("9");
        assert_eq!(
            unsafe { __wolf_rt_json_get(ap, al, np, nl, o) },
            json_code::MISSING
        );
        assert_eq!(
            unsafe { __wolf_rt_json_type(ap, al, np, nl, o) },
            json_code::MISSING
        );
        let (zp, zl) = pair_of("0");
        assert_eq!(
            unsafe { __wolf_rt_json_len(ap, al, zp, zl) },
            -json_code::KIND
        );
        assert_eq!(
            unsafe { __wolf_rt_json_len(bp, bl, xp, xl) },
            -json_code::PARSE
        );
    }
}
