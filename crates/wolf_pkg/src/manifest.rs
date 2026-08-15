//! `wolf.pkg` — the declarative package manifest (s51, D33).
//!
//! One `pkg { }` block of `key: value` data in wolf's literal syntax,
//! lexed by the compiler's own lexer (the manifest never grows a
//! second grammar). Reading a manifest executes nothing, by
//! construction: values are strings, integers, bools, bare capability
//! words, lists, and maps — there is no expression form, no
//! interpolation, and any key that spells a build-time script hook is
//! refused at parse time with E1503 (D33: **no build scripts, ever**).
//!
//! The line-based s22 stub (`trusted = a, b` / `lints.deny = …`) is a
//! different, older surface; [`is_manifest`] tells the two apart so
//! stub files keep working where they already do.

use std::collections::BTreeSet;

use wolf_cimport::Recipe;
use wolf_cimport::recipe::Sysroot;
use wolf_diag::{Diagnostic, codes};
use wolf_lex::{Keyword, Punct, StrKind, Token, TokenKind};
use wolf_span::{FileId, Span};

use crate::version::Version;

/// The I13 capability vocabulary — closed set, locked by Round 4.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Cap {
    Net,
    Fs,
    Exec,
    Env,
    Ffi,
    Unsafe,
    Comptime,
}

impl Cap {
    pub fn as_str(self) -> &'static str {
        match self {
            Cap::Net => "net",
            Cap::Fs => "fs",
            Cap::Exec => "exec",
            Cap::Env => "env",
            Cap::Ffi => "ffi",
            Cap::Unsafe => "unsafe",
            Cap::Comptime => "comptime",
        }
    }

    pub fn parse(s: &str) -> Option<Cap> {
        Some(match s {
            "net" => Cap::Net,
            "fs" => Cap::Fs,
            "exec" => Cap::Exec,
            "env" => Cap::Env,
            "ffi" => Cap::Ffi,
            "unsafe" => Cap::Unsafe,
            "comptime" => Cap::Comptime,
            _ => return None,
        })
    }
}

/// Where a dependency's bits come from. Exactly one source form per
/// entry; the registry form is schema-stable now, service-backed at
/// c15 (X7: registry-shaped protocol now, hosted service later).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DepSource {
    /// `{ path: "../local-util" }` — a local tree, never hashed into
    /// the ledger (it is yours to edit).
    Path { path: String },
    /// `{ git: "https://…", tag: "v1.4.0" }` — pinned VCS fetch into
    /// the content-addressed store.
    Git { url: String, tag: String },
    /// `{ pkg: "owner/name", major: 1, min: "1.2.0" }` — a registry
    /// dependency (X7 stub at v0: the form parses and resolves in MVS
    /// unit universes; fetching it is c15).
    Registry {
        pkg: String,
        major: u32,
        min: Version,
    },
}

/// One dependency entry: `alias: { …source… }`.
#[derive(Clone, Debug)]
pub struct Dep {
    pub alias: String,
    /// Span of the whole entry (key through value) — `wolf rm` splices
    /// exactly this (plus a trailing comma) out of the text.
    pub span: Span,
    pub source: DepSource,
}

/// A parsed `wolf.pkg`. Spans point into the manifest file so
/// diagnostics land on the manifest itself.
#[derive(Clone, Debug)]
pub struct Manifest {
    pub name: String,
    pub name_span: Span,
    pub version: Version,
    pub edition: String,
    /// `wolf: "0.9"` — minimum toolchain (advisory at v0).
    pub wolf_min: Option<String>,
    /// Immutable identity minted at init (advisory at v0).
    pub fingerprint: Option<u64>,
    pub deps: Vec<Dep>,
    /// Target-scoped dep sections (parsed and carried; test-only deps
    /// never leak to consumers — resolution of these is `wolf test`'s
    /// call, not the consumer build's).
    pub test_deps: Vec<Dep>,
    pub bench_deps: Vec<Dep>,
    /// `capabilities: [net, fs]` (I13).
    pub caps: Vec<Cap>,
    pub caps_span: Option<Span>,
    /// `c: { name: { headers: [...] } }` — the declarative C recipes
    /// (s46, c10). D33's answer to header location: the manifest says
    /// where, and nothing is executed to find out.
    pub c: Vec<Recipe>,
    pub c_span: Option<Span>,
    /// The whole `pkg { … }` block.
    pub span: Span,
    /// The `deps: { … }` map's span, when present (`wolf add` inserts
    /// just before its closing brace).
    pub deps_span: Option<Span>,
}

/// Keys that ask for code to run on the host at build/fetch time.
/// Refused unconditionally, at any nesting depth (E1503, D33).
const FORBIDDEN_KEYS: &[&str] = &[
    "build",
    "build_rs",
    "script",
    "scripts",
    "hook",
    "hooks",
    "install",
    "preinstall",
    "pre_install",
    "postinstall",
    "post_install",
    "prebuild",
    "pre_build",
    "postbuild",
    "post_build",
];

/// Is this text an s51 `pkg { }` manifest (as opposed to the s22
/// line-based stub, or prose)? Cheap and syntactic: the first
/// non-comment, non-blank line starts with `pkg` followed by `{`.
pub fn is_manifest(text: &str) -> bool {
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with("//") {
            continue;
        }
        let Some(rest) = t.strip_prefix("pkg") else {
            return false;
        };
        return rest.trim_start().starts_with('{');
    }
    false
}

// ------------------------------------------------------------- values ----

/// A manifest value — data, nothing else.
#[derive(Clone, Debug)]
pub enum Value {
    Str(String, Span),
    Int(u64, Span),
    Bool(bool, Span),
    /// A bare word (capability names: `net`, `unsafe`, …).
    Word(String, Span),
    List(Vec<Value>, Span),
    Map(Vec<Entry>, Span),
}

impl Value {
    pub fn span(&self) -> Span {
        match self {
            Value::Str(_, s)
            | Value::Int(_, s)
            | Value::Bool(_, s)
            | Value::Word(_, s)
            | Value::List(_, s)
            | Value::Map(_, s) => *s,
        }
    }

    fn kind_name(&self) -> &'static str {
        match self {
            Value::Str(..) => "a string",
            Value::Int(..) => "an integer",
            Value::Bool(..) => "a bool",
            Value::Word(..) => "a bare word",
            Value::List(..) => "a list",
            Value::Map(..) => "a map",
        }
    }
}

/// One `key: value` entry with the spans `wolf add`/`wolf rm` edit by.
#[derive(Clone, Debug)]
pub struct Entry {
    pub key: String,
    pub key_span: Span,
    pub value: Value,
}

impl Entry {
    /// Key through value end — the splice range for `wolf rm`.
    fn span(&self) -> Span {
        Span::new(self.key_span.file, self.key_span.lo, self.value.span().hi)
    }
}

// ------------------------------------------------------------- parser ----

struct Parser<'a> {
    toks: &'a [Token],
    src: &'a [u8],
    pos: usize,
    diags: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Token {
        &self.toks[self.pos]
    }

    fn bump(&mut self) -> &Token {
        let t = &self.toks[self.pos];
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }

    /// Statement terminators are the lexer's newline bookkeeping; the
    /// manifest grammar separates entries by comma or newline, so
    /// `Term` is skippable everywhere.
    fn skip_terms(&mut self) {
        while matches!(self.peek().kind, TokenKind::Term) {
            self.bump();
        }
    }

    fn text(&self, sp: Span) -> String {
        String::from_utf8_lossy(&self.src[sp.lo as usize..sp.hi as usize]).into_owned()
    }

    fn err(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.push(
            Diagnostic::error(codes::E1501, span, msg).with_note(
                "wolf.pkg is declarative data (D33): one `pkg { }` block of \
                 `key: value` entries — strings, integers, bare capability \
                 words, `[ ]` lists, `{ }` maps."
                    .to_string(),
            ),
        );
    }

    /// The D33 gate: a key spelling a build-time hook fails the parse
    /// with its own code and rationale — refused, not ignored.
    fn check_forbidden_key(&mut self, key: &str, span: Span) -> bool {
        if !FORBIDDEN_KEYS.contains(&key) {
            return false;
        }
        self.diags.push(
            Diagnostic::error(
                codes::E1503,
                span,
                format!(
                    "`{key}` asks for code to run at build time — wolf has no build scripts, ever"
                ),
            )
            .with_label("refused unconditionally (D33)")
            .with_note(
                "adding a wolf dependency never means arbitrary code runs on your \
                 machine: no build.rs, no post-install hooks, no Turing-complete \
                 manifest (D33). Declare C-library needs in the declarative `c: { }` \
                 recipe, compute in sandboxed `comptime`, or wrap a \
                 system/prebuilt library."
                    .to_string(),
            ),
        );
        true
    }

    fn parse_manifest(&mut self) -> Option<Value> {
        self.skip_terms();
        let t = self.bump().clone();
        let is_pkg = matches!(t.kind, TokenKind::Ident) && self.text(t.span) == "pkg";
        if !is_pkg {
            self.err(t.span, "a manifest starts with `pkg {`");
            return None;
        }
        self.skip_terms();
        let open = self.peek().clone();
        if !matches!(open.kind, TokenKind::Punct(Punct::LBrace)) {
            self.err(open.span, "expected `{` after `pkg`");
            return None;
        }
        let map = self.parse_map()?;
        self.skip_terms();
        let after = self.peek().clone();
        if !matches!(after.kind, TokenKind::Eof) {
            self.err(after.span, "nothing may follow the `pkg { }` block");
        }
        Some(map)
    }

    /// Parse `{ entries }`; the opening brace is the current token.
    fn parse_map(&mut self) -> Option<Value> {
        let open = self.bump().span;
        let mut entries: Vec<Entry> = Vec::new();
        loop {
            self.skip_terms();
            // Separators between entries: commas are optional where a
            // newline already separated.
            while matches!(self.peek().kind, TokenKind::Punct(Punct::Comma)) {
                self.bump();
                self.skip_terms();
            }
            let t = self.peek().clone();
            match t.kind {
                TokenKind::Punct(Punct::RBrace) => {
                    let close = self.bump().span;
                    let span = Span::new(open.file, open.lo, close.hi);
                    return Some(Value::Map(entries, span));
                }
                TokenKind::Eof => {
                    self.err(t.span, "unclosed `{` — expected `}`");
                    return None;
                }
                TokenKind::Ident | TokenKind::Kw(_) => {
                    let key_tok = self.bump().clone();
                    let key = self.text(key_tok.span);
                    let forbidden = self.check_forbidden_key(&key, key_tok.span);
                    self.skip_terms();
                    let colon = self.peek().clone();
                    if !matches!(colon.kind, TokenKind::Punct(Punct::Colon)) {
                        self.err(colon.span, format!("expected `:` after key `{key}`"));
                        return None;
                    }
                    self.bump();
                    self.skip_terms();
                    let value = self.parse_value()?;
                    if forbidden {
                        // The value parsed (so the error is singular
                        // and precise), but the entry is refused.
                        continue;
                    }
                    entries.push(Entry {
                        key,
                        key_span: key_tok.span,
                        value,
                    });
                }
                _ => {
                    let txt = self.text(t.span);
                    self.err(t.span, format!("expected a key or `}}`, found `{txt}`"));
                    return None;
                }
            }
        }
    }

    fn parse_value(&mut self) -> Option<Value> {
        let t = self.peek().clone();
        match t.kind {
            TokenKind::StrBegin(kind) => self.parse_string(kind),
            TokenKind::Int => {
                let t = self.bump().clone();
                let raw = self.text(t.span).replace('_', "");
                let v = if let Some(hex) = raw.strip_prefix("0x").or(raw.strip_prefix("0X")) {
                    u64::from_str_radix(hex, 16)
                } else {
                    raw.parse()
                };
                match v {
                    Ok(v) => Some(Value::Int(v, t.span)),
                    Err(_) => {
                        self.err(t.span, "integer does not fit 64 bits");
                        None
                    }
                }
            }
            TokenKind::Kw(Keyword::True) => {
                let t = self.bump().clone();
                Some(Value::Bool(true, t.span))
            }
            TokenKind::Kw(Keyword::False) => {
                let t = self.bump().clone();
                Some(Value::Bool(false, t.span))
            }
            // Bare words: capability names. `unsafe` and `comptime`
            // are keywords to the lexer but words to the manifest.
            TokenKind::Ident | TokenKind::Kw(_) => {
                let t = self.bump().clone();
                let w = self.text(t.span);
                Some(Value::Word(w, t.span))
            }
            TokenKind::Punct(Punct::LBrace) => self.parse_map(),
            TokenKind::Punct(Punct::LBracket) => {
                let open = self.bump().span;
                let mut items = Vec::new();
                loop {
                    self.skip_terms();
                    while matches!(self.peek().kind, TokenKind::Punct(Punct::Comma)) {
                        self.bump();
                        self.skip_terms();
                    }
                    let t = self.peek().clone();
                    match t.kind {
                        TokenKind::Punct(Punct::RBracket) => {
                            let close = self.bump().span;
                            return Some(Value::List(
                                items,
                                Span::new(open.file, open.lo, close.hi),
                            ));
                        }
                        TokenKind::Eof => {
                            self.err(t.span, "unclosed `[` — expected `]`");
                            return None;
                        }
                        _ => items.push(self.parse_value()?),
                    }
                }
            }
            _ => {
                let txt = self.text(t.span);
                self.err(t.span, format!("expected a value, found `{txt}`"));
                None
            }
        }
    }

    /// A string episode. Interpolation is refused: a manifest is data,
    /// and data does not compute.
    fn parse_string(&mut self, _kind: StrKind) -> Option<Value> {
        let begin = self.bump().span;
        let mut out = String::new();
        loop {
            let t = self.peek().clone();
            match t.kind {
                TokenKind::StrFragment => {
                    let raw = self.text(t.span);
                    out.push_str(&unescape(&raw));
                    self.bump();
                }
                TokenKind::InterpOpen => {
                    self.err(
                        t.span,
                        "manifest strings never interpolate — a manifest is data (D33)",
                    );
                    return None;
                }
                TokenKind::StrEnd { .. } => {
                    let end = self.bump().span;
                    return Some(Value::Str(out, Span::new(begin.file, begin.lo, end.hi)));
                }
                _ => {
                    self.err(t.span, "unterminated string in manifest");
                    return None;
                }
            }
        }
    }
}

/// Minimal escape processing for manifest strings — the plain-string
/// escapes a path or URL could carry; everything else stays literal.
fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('{') => out.push('{'),
            Some('}') => out.push('}'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

// ------------------------------------------------------------- schema ----

fn schema_err(span: Span, msg: impl Into<String>) -> Diagnostic {
    Diagnostic::error(codes::E1502, span, msg).with_note(
        "the wolf.pkg schema: `name`, `version`, `edition`, `wolf`, \
         `fingerprint`, `deps`, `test`, `bench`, `features`, `capabilities`, \
         `paths`, `min_age`, `c`, `lints`, `trusted`. Dependency sources: \
         `{ path: \"…\" }`, `{ git: \"…\", tag: \"…\" }`, or \
         `{ pkg: \"owner/name\", major: N, min: \"X.Y.Z\" }`."
            .to_string(),
    )
}

fn expect_str(key: &str, v: &Value, diags: &mut Vec<Diagnostic>) -> Option<(String, Span)> {
    match v {
        Value::Str(s, sp) => Some((s.clone(), *sp)),
        other => {
            diags.push(schema_err(
                other.span(),
                format!("`{key}` takes a string, not {}", other.kind_name()),
            ));
            None
        }
    }
}

/// Parse the `c: { }` block — the declarative C recipes (s46, c10).
///
/// This is D33's answer to "where is `stdlib.h`?". Every other
/// language's answer is *run something and ask*: a configure script,
/// `pkg-config`, a `build.rs` that shells out. Wolf declares it here
/// and the compiler resolves it, so adding a C dependency still never
/// means arbitrary code runs on the machine doing the build.
///
/// Recipes are validated by [`wolf_cimport::Recipe::check`] — absolute
/// include paths, `..` escapes, response files, plugin flags and
/// anything else whose effect is "and then run this" are refused here,
/// at the manifest, rather than on a consumer's machine.
fn parse_c_recipes(v: &Value, diags: &mut Vec<Diagnostic>) -> Vec<Recipe> {
    let Value::Map(entries, _) = v else {
        diags.push(schema_err(
            v.span(),
            format!(
                "`c` takes a map of `name: {{ headers: [\"…\"] }}` recipes, not {}",
                v.kind_name()
            ),
        ));
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries {
        let Value::Map(fields, _) = &e.value else {
            diags.push(schema_err(
                e.value.span(),
                format!("the C recipe `{}` takes a map of fields", e.key),
            ));
            continue;
        };
        let mut r = Recipe {
            name: e.key.clone(),
            ..Recipe::default()
        };
        let mut span = e.key_span;
        for f in fields {
            span = f.key_span;
            // The recipe's own D33 gate. The manifest-wide check
            // already refuses `build`/`script`/`hook`; a recipe adds
            // the C-flavoured ways to ask for the same thing.
            if wolf_cimport::recipe::FORBIDDEN_RECIPE_KEYS.contains(&f.key.as_str()) {
                diags.push(
                    Diagnostic::error(
                        codes::E1503,
                        f.key_span,
                        format!(
                            "`{}` asks the build to run a program to find a C library \
                             — wolf has no build scripts, ever",
                            f.key
                        ),
                    )
                    .with_label("refused unconditionally (D33)")
                    .with_note(
                        "a C recipe is data: say where the headers are with \
                         `headers`, `include` and `define`, and what to link with \
                         `link`. If a library's location genuinely varies, that is a \
                         `sysroot: system` decision said out loud, not a script."
                            .to_string(),
                    ),
                );
                continue;
            }
            match f.key.as_str() {
                "headers" => r.headers = str_list("headers", &f.value, diags),
                "include" => r.include = str_list("include", &f.value, diags),
                "cflags" => r.cflags = str_list("cflags", &f.value, diags),
                "link" => r.link = str_list("link", &f.value, diags),
                "define" => {
                    let Value::Map(defs, _) = &f.value else {
                        diags.push(schema_err(
                            f.value.span(),
                            "`define` takes a map of `NAME: \"value\"` entries".to_string(),
                        ));
                        continue;
                    };
                    for d in defs {
                        match &d.value {
                            Value::Str(s, _) => {
                                r.define.insert(d.key.clone(), s.clone());
                            }
                            Value::Int(n, _) => {
                                r.define.insert(d.key.clone(), n.to_string());
                            }
                            other => diags.push(schema_err(
                                other.span(),
                                format!(
                                    "a `define` value is a string or an integer, not {}",
                                    other.kind_name()
                                ),
                            )),
                        }
                    }
                }
                "sysroot" => {
                    // A named choice, never a path: `bundled` is the
                    // per-target header bundle (which is also what makes
                    // cross-compilation work), `system` is the host's
                    // own headers, admitted out loud.
                    let word = match &f.value {
                        Value::Word(w, _) => Some(w.clone()),
                        Value::Str(s, _) => Some(s.clone()),
                        other => {
                            diags.push(schema_err(
                                other.span(),
                                "`sysroot` is `bundled` or `system`".to_string(),
                            ));
                            None
                        }
                    };
                    if let Some(w) = word {
                        match Sysroot::parse(&w) {
                            Some(s) => r.sysroot = s,
                            None => diags.push(schema_err(
                                f.value.span(),
                                format!(
                                    "`sysroot` is `bundled` or `system`, not `{w}` — it \
                                     names a header source, it is not a path to search"
                                ),
                            )),
                        }
                    }
                }
                other => diags.push(schema_err(
                    f.key_span,
                    format!(
                        "unknown C recipe field `{other}` (headers, include, define, \
                         cflags, link, sysroot)"
                    ),
                )),
            }
        }
        // The importer owns what a well-formed recipe is; the manifest
        // just reports what it says.
        for err in r.check() {
            diags.push(
                schema_err(
                    span,
                    format!(
                        "the C recipe `{}` has a bad `{}` entry `{}`: {}",
                        err.recipe, err.field, err.value, err.why
                    ),
                )
                .with_note(err.note),
            );
        }
        out.push(r);
    }
    out
}

fn str_list(key: &str, v: &Value, diags: &mut Vec<Diagnostic>) -> Vec<String> {
    let Value::List(items, _) = v else {
        diags.push(schema_err(
            v.span(),
            format!("`{key}` takes a list of strings, not {}", v.kind_name()),
        ));
        return Vec::new();
    };
    let mut out = Vec::new();
    for it in items {
        match it {
            Value::Str(s, _) => out.push(s.clone()),
            other => diags.push(schema_err(
                other.span(),
                format!("`{key}` holds strings, not {}", other.kind_name()),
            )),
        }
    }
    out
}

fn parse_deps(key: &str, v: &Value, diags: &mut Vec<Diagnostic>) -> Vec<Dep> {
    let Value::Map(entries, _) = v else {
        diags.push(schema_err(
            v.span(),
            format!("`{key}` takes a map of `alias: {{ source }}` entries"),
        ));
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries {
        let Value::Map(fields, _) = &e.value else {
            diags.push(schema_err(
                e.value.span(),
                format!(
                    "dependency `{}` takes a source map, not {}",
                    e.key,
                    e.value.kind_name()
                ),
            ));
            continue;
        };
        let get = |name: &str| fields.iter().find(|f| f.key == name).map(|f| &f.value);
        let known: BTreeSet<&str> = ["path", "git", "tag", "pkg", "major", "min", "lazy"].into();
        for f in fields {
            if !known.contains(f.key.as_str()) {
                diags.push(schema_err(
                    f.key_span,
                    format!("unknown dependency field `{}`", f.key),
                ));
            }
        }
        let source = match (get("path"), get("git"), get("pkg")) {
            (Some(p), None, None) => {
                expect_str("path", p, diags).map(|(path, _)| DepSource::Path { path })
            }
            (None, Some(g), None) => {
                let url = expect_str("git", g, diags);
                let tag = match get("tag") {
                    Some(t) => expect_str("tag", t, diags),
                    None => {
                        diags.push(schema_err(
                            e.value.span(),
                            format!("git dependency `{}` needs a `tag:` pin", e.key),
                        ));
                        None
                    }
                };
                match (url, tag) {
                    (Some((url, _)), Some((tag, _))) => Some(DepSource::Git { url, tag }),
                    _ => None,
                }
            }
            (None, None, Some(p)) => {
                let pkg = expect_str("pkg", p, diags);
                let major = match get("major") {
                    Some(Value::Int(n, _)) => Some(*n as u32),
                    Some(other) => {
                        diags.push(schema_err(
                            other.span(),
                            "`major` takes an integer".to_string(),
                        ));
                        None
                    }
                    None => Some(1),
                };
                let min = match get("min") {
                    Some(Value::Str(s, sp)) => match Version::parse(s) {
                        Some(v) => Some(v),
                        None => {
                            diags.push(schema_err(
                                *sp,
                                format!("`min` is not a version: `{s}` (dotted numerics, e.g. \"1.2.0\")"),
                            ));
                            None
                        }
                    },
                    Some(other) => {
                        diags.push(schema_err(
                            other.span(),
                            "`min` takes a version string".to_string(),
                        ));
                        None
                    }
                    None => Some(Version::default()),
                };
                match (pkg, major, min) {
                    (Some((pkg, _)), Some(major), Some(min)) => {
                        Some(DepSource::Registry { pkg, major, min })
                    }
                    _ => None,
                }
            }
            _ => {
                diags.push(schema_err(
                    e.value.span(),
                    format!(
                        "dependency `{}` needs exactly one source: `path:`, `git:` + `tag:`, or `pkg:`",
                        e.key
                    ),
                ));
                None
            }
        };
        if let Some(source) = source {
            out.push(Dep {
                alias: e.key.clone(),
                span: e.span(),
                source,
            });
        }
    }
    out
}

/// Schema knobs. There is exactly one, and it exists because a script's
/// manifest rides in the script (s53): a script's identity is its path,
/// so it has no `name:` to state, while a package on disk must have one.
#[derive(Clone, Debug)]
pub struct ParseOpts {
    /// Is a `name:` entry required? (`false` for script frontmatter,
    /// which supplies `default_name` instead.)
    pub require_name: bool,
    /// The name to record when `require_name` is false and none was
    /// written.
    pub default_name: String,
}

impl Default for ParseOpts {
    fn default() -> Self {
        ParseOpts {
            require_name: true,
            default_name: String::new(),
        }
    }
}

/// Parse and schema-check a manifest. `file` is the manifest's own
/// FileId (register the text under it so spans render). On any error
/// the manifest is refused — `None` plus the diagnostics.
pub fn parse(file: FileId, text: &str) -> (Option<Manifest>, Vec<Diagnostic>) {
    parse_opts(file, text, &ParseOpts::default())
}

/// [`parse`] with the schema knobs spelled out.
pub fn parse_opts(
    file: FileId,
    text: &str,
    opts: &ParseOpts,
) -> (Option<Manifest>, Vec<Diagnostic>) {
    let lexed = wolf_lex::lex(file, text.as_bytes());
    let mut diags: Vec<Diagnostic> = lexed.diagnostics;
    let had_lex_errors = diags
        .iter()
        .any(|d| d.severity == wolf_diag::Severity::Error);
    let mut p = Parser {
        toks: &lexed.tokens,
        src: text.as_bytes(),
        pos: 0,
        diags: Vec::new(),
    };
    let root = if had_lex_errors {
        None
    } else {
        p.parse_manifest()
    };
    diags.extend(p.diags);
    let Some(Value::Map(entries, pkg_span)) = root else {
        return (None, diags);
    };
    if diags
        .iter()
        .any(|d| d.severity == wolf_diag::Severity::Error)
    {
        // E1503 (or a lex error) already refused the manifest.
        return (None, diags);
    }

    let mut m = Manifest {
        name: String::new(),
        name_span: pkg_span,
        version: Version::default(),
        edition: "1".to_string(),
        wolf_min: None,
        fingerprint: None,
        deps: Vec::new(),
        test_deps: Vec::new(),
        bench_deps: Vec::new(),
        caps: Vec::new(),
        caps_span: None,
        c: Vec::new(),
        c_span: None,
        span: pkg_span,
        deps_span: None,
    };
    for e in &entries {
        match e.key.as_str() {
            "name" => {
                if let Some((s, sp)) = expect_str("name", &e.value, &mut diags) {
                    m.name = s;
                    m.name_span = sp;
                }
            }
            "version" => {
                if let Some((s, sp)) = expect_str("version", &e.value, &mut diags) {
                    match Version::parse(&s) {
                        Some(v) => m.version = v,
                        None => diags.push(schema_err(
                            sp,
                            format!("`version` is not a version: `{s}` (dotted numerics)"),
                        )),
                    }
                }
            }
            "edition" => {
                if let Some((s, _)) = expect_str("edition", &e.value, &mut diags) {
                    m.edition = s;
                }
            }
            "wolf" => {
                if let Some((s, _)) = expect_str("wolf", &e.value, &mut diags) {
                    m.wolf_min = Some(s);
                }
            }
            "fingerprint" => match &e.value {
                Value::Int(v, _) => m.fingerprint = Some(*v),
                other => diags.push(schema_err(
                    other.span(),
                    "`fingerprint` takes an integer".to_string(),
                )),
            },
            "deps" => {
                m.deps = parse_deps("deps", &e.value, &mut diags);
                m.deps_span = Some(e.value.span());
            }
            "test" | "bench" => {
                let Value::Map(fields, _) = &e.value else {
                    diags.push(schema_err(
                        e.value.span(),
                        format!("`{}` takes a map (e.g. `{{ deps: {{ … }} }}`)", e.key),
                    ));
                    continue;
                };
                for f in fields {
                    if f.key == "deps" {
                        let parsed = parse_deps(&e.key, &f.value, &mut diags);
                        if e.key == "test" {
                            m.test_deps = parsed;
                        } else {
                            m.bench_deps = parsed;
                        }
                    } else {
                        diags.push(schema_err(
                            f.key_span,
                            format!("unknown `{}` field `{}` (only `deps`)", e.key, f.key),
                        ));
                    }
                }
            }
            "capabilities" => {
                let Value::List(items, sp) = &e.value else {
                    diags.push(schema_err(
                        e.value.span(),
                        "`capabilities` takes a list of capability words".to_string(),
                    ));
                    continue;
                };
                m.caps_span = Some(*sp);
                for it in items {
                    match it {
                        Value::Word(w, sp) => match Cap::parse(w) {
                            Some(c) => m.caps.push(c),
                            None => diags.push(schema_err(
                                *sp,
                                format!(
                                    "`{w}` is not a capability (net, fs, exec, env, ffi, unsafe, comptime)"
                                ),
                            )),
                        },
                        other => diags.push(schema_err(
                            other.span(),
                            format!(
                                "capabilities are bare words, not {}",
                                other.kind_name()
                            ),
                        )),
                    }
                }
                m.caps.sort();
                m.caps.dedup();
            }
            // Parsed-and-carried schema surface: stable shape now,
            // semantics with their own campaigns (features/c: c10,
            // paths: the ledger filter, min_age: RFC 3923 cooldown,
            // lints/trusted: bridge to the s22/s67 stubs).
            "c" => {
                m.c = parse_c_recipes(&e.value, &mut diags);
                m.c_span = Some(e.value.span());
            }
            "features" | "paths" | "min_age" | "lints" | "trusted" => {}
            other => {
                diags.push(schema_err(
                    e.key_span,
                    format!("unknown manifest key `{other}`"),
                ));
            }
        }
    }
    if m.name.is_empty() {
        if opts.require_name {
            diags.push(schema_err(
                pkg_span,
                "the manifest needs a `name: \"owner/pkg\"` entry".to_string(),
            ));
        } else {
            m.name = opts.default_name.clone();
        }
    }
    if diags
        .iter()
        .any(|d| d.severity == wolf_diag::Severity::Error)
    {
        return (None, diags);
    }
    (Some(m), diags)
}

// -------------------------------------------------------------- edits ----

/// Render a dependency entry line for `wolf add`.
pub fn render_dep(alias: &str, source: &DepSource) -> String {
    match source {
        DepSource::Path { path } => format!("{alias}: {{ path: \"{path}\" }}"),
        DepSource::Git { url, tag } => {
            format!("{alias}: {{ git: \"{url}\", tag: \"{tag}\" }}")
        }
        DepSource::Registry { pkg, major, min } => {
            format!("{alias}: {{ pkg: \"{pkg}\", major: {major}, min: \"{min}\" }}")
        }
    }
}

/// Insert a dependency entry into manifest text (`wolf add`): inside
/// the existing `deps: { … }` map, or as a fresh `deps:` section
/// before the `pkg` block's closing brace. Pure text-in/text-out — the
/// manifest stays the user's file, comments and all.
pub fn insert_dep(text: &str, m: &Manifest, alias: &str, source: &DepSource) -> String {
    let entry = render_dep(alias, source);
    match m.deps_span {
        Some(sp) => {
            // Before the deps map's closing `}`.
            let at = sp.hi as usize - 1;
            let (head, tail) = text.split_at(at);
            let head_trimmed = head.trim_end_matches([' ', '\t']);
            let needs_comma = !head_trimmed.trim_end().ends_with(['{', ',']);
            let comma = if needs_comma { "," } else { "" };
            format!("{head_trimmed}{comma}\n        {entry},\n    {tail}")
        }
        None => {
            let at = m.span.hi as usize - 1;
            let (head, tail) = text.split_at(at);
            let head_trimmed = head.trim_end_matches([' ', '\t']);
            format!("{head_trimmed}    deps: {{\n        {entry},\n    }},\n{tail}")
        }
    }
}

/// Remove dependency `alias` from manifest text (`wolf rm`): splice
/// out its entry span plus a trailing comma. `None` when no such dep.
pub fn remove_dep(text: &str, m: &Manifest, alias: &str) -> Option<String> {
    let dep = m.deps.iter().find(|d| d.alias == alias)?;
    let mut lo = dep.span.lo as usize;
    let mut hi = dep.span.hi as usize;
    let bytes = text.as_bytes();
    // Swallow a trailing comma and the rest of the line.
    while hi < bytes.len() && (bytes[hi] == b' ' || bytes[hi] == b'\t') {
        hi += 1;
    }
    if hi < bytes.len() && bytes[hi] == b',' {
        hi += 1;
    }
    while hi < bytes.len() && (bytes[hi] == b' ' || bytes[hi] == b'\t') {
        hi += 1;
    }
    if hi < bytes.len() && bytes[hi] == b'\n' {
        hi += 1;
    }
    // Swallow the line's leading indentation.
    while lo > 0 && (bytes[lo - 1] == b' ' || bytes[lo - 1] == b'\t') {
        lo -= 1;
    }
    Some(format!("{}{}", &text[..lo], &text[hi..]))
}

/// A minimal manifest for `wolf add` in a project that has none yet.
pub fn minimal_manifest(name: &str) -> String {
    format!("pkg {{\n    name:    \"{name}\",\n    version: \"0.1.0\",\n    edition: \"1\",\n}}\n")
}
