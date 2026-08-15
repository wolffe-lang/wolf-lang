//! The **reference worker** — a small, honest implementation of the
//! importer protocol.
//!
//! # What this is for
//!
//! The interface is the deliverable; an interface with one
//! implementation is a guess. The reference worker exists so the
//! conformance suite runs in CI on a machine with no C toolchain at
//! all, so the protocol has a second implementation keeping it honest,
//! and so `wolf c-import` does something a reader can step through.
//!
//! # What this is NOT
//!
//! **Not a C frontend.** That is campaign c15, and it is a non-target
//! of this sprint. This scanner reads a deliberately narrow subset of C
//! declaration syntax and **refuses everything else by name** — which
//! is the behaviour under test, not a limitation being apologised for.
//! It does not evaluate `#if`, it does not implement C's full
//! declarator grammar, and it does not know your platform's headers.
//!
//! A production import of real system headers wants the libclang-backed
//! worker (or, later, c15's frontend), which speaks the same protocol
//! and drops in without a compiler change. That is the entire point of
//! [`crate::protocol`].
//!
//! # The subset it does read
//!
//! - `#define NAME …` and `#define NAME(a, b) …`
//! - function declarations, including `static inline`
//! - `extern` object declarations
//! - `typedef` of a scalar or pointer type
//! - `struct` definitions, including bitfields
//! - `union` definitions — always demoted (a union's live member is not
//!   in the header)
//! - `enum` definitions
//! - `#include "x.h"` / `#include <x.h>`, resolved against the include
//!   paths
//!
//! Anything else at file scope is refused by name at the declaration,
//! and its siblings are unaffected.

use std::collections::BTreeMap;

use crate::artifact::{
    Artifact, ConstValue, Decl, DeclKind, Linkage, MacroDef, MacroKind, SourceLoc,
};
use crate::cache::ImportRequest;
use crate::ctype::{
    CType, CTypeId, EnumDef, EnumId, Field, IntSpelling, Qual, Record, RecordId, RecordKind,
    TargetInfo,
};
use crate::protocol::{Request, Response};
use crate::refuse::{Demotion, Refusal, Status};

/// This worker's identity, as reported by `--version` and folded into
/// the cache key.
pub const REFERENCE_WORKER_ID: &str = "wolf-cimport-reference 1";

/// Where header text comes from. Abstracted so the conformance suite
/// can run entirely in memory — a test that writes headers to a
/// temporary directory to read them back is testing the filesystem.
pub trait HeaderSource {
    /// Resolve `name` against `include_paths`, returning
    /// `(display_path, text)`.
    fn read(&self, name: &str, include_paths: &[String]) -> Option<(String, String)>;
}

/// Headers held in memory, keyed by name.
#[derive(Default, Debug)]
pub struct MemHeaders(pub BTreeMap<String, String>);

impl MemHeaders {
    pub fn new<const N: usize>(items: [(&str, &str); N]) -> MemHeaders {
        MemHeaders(
            items
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }
}

impl HeaderSource for MemHeaders {
    fn read(&self, name: &str, _include_paths: &[String]) -> Option<(String, String)> {
        self.0.get(name).map(|t| (name.to_string(), t.clone()))
    }
}

/// Headers on disk, resolved against the include paths in order.
#[derive(Default, Debug)]
pub struct DiskHeaders;

impl HeaderSource for DiskHeaders {
    fn read(&self, name: &str, include_paths: &[String]) -> Option<(String, String)> {
        for dir in include_paths {
            let p = std::path::Path::new(dir).join(name);
            if let Ok(text) = std::fs::read_to_string(&p) {
                return Some((p.display().to_string(), text));
            }
        }
        None
    }
}

/// Answer one request.
pub fn serve(req: &Request, headers: &dyn HeaderSource) -> Response {
    match req {
        Request::Import(r) => match import(r, headers) {
            Ok(a) => Response::Artifact(crate::encode::encode(&a)),
            Err(e) => Response::Err(e),
        },
        // Live macro expansion needs the original TU's preprocessor
        // state. The reference worker keeps tokens, not a preprocessor,
        // so it says so instead of expanding something plausible.
        Request::ExpandMacro { name, .. } => Response::Err(format!(
            "the reference worker records macro `{name}` but does not expand it: \
             re-expansion needs a preprocessor in the original translation unit, \
             which is the libclang worker's job (or c15's)"
        )),
    }
}

/// Import a header set into an artifact.
pub fn import(req: &ImportRequest, headers: &dyn HeaderSource) -> Result<Artifact, String> {
    let target = TargetInfo::for_triple(&req.target).ok_or_else(|| {
        format!(
            "unknown target `{}` — the importer parameterizes types per target and \
             will not guess widths for one it does not know",
            req.target
        )
    })?;

    let mut a = Artifact::new(REFERENCE_WORKER_ID, target.clone());
    a.headers = req.headers.clone();

    let mut cx = Cx {
        a: &mut a,
        target,
        defines: req.defines.iter().cloned().collect(),
        tags: BTreeMap::new(),
        ordinary: BTreeMap::new(),
        typedefs: BTreeMap::new(),
        seen_files: Vec::new(),
    };

    for h in &req.headers {
        let (path, text) = headers.read(h, &req.include_paths).ok_or_else(|| {
            format!(
                "could not find `{h}` in any include path ({})",
                if req.include_paths.is_empty() {
                    "none were given".to_string()
                } else {
                    req.include_paths.join(", ")
                }
            )
        })?;
        cx.file(&path, &text, headers, &req.include_paths, 0)?;
    }

    resolve_tag_collisions(&mut a);
    a.canonicalize();
    Ok(a)
}

// ------------------------------------------------------------ context --

struct Cx<'a> {
    a: &'a mut Artifact,
    target: TargetInfo,
    defines: BTreeMap<String, String>,
    /// tag name -> record/enum id
    tags: BTreeMap<String, CTypeId>,
    /// ordinary identifiers already declared (functions, objects)
    ordinary: BTreeMap<String, ()>,
    /// typedef name -> (type, size in bytes, alignment)
    typedefs: BTreeMap<String, (CTypeId, u64, u32)>,
    seen_files: Vec<String>,
}

impl Cx<'_> {
    fn file_index(&mut self, path: &str) -> u32 {
        if let Some(i) = self.a.files.iter().position(|f| f == path) {
            return i as u32;
        }
        self.a.files.push(path.to_string());
        (self.a.files.len() - 1) as u32
    }

    fn file(
        &mut self,
        path: &str,
        text: &str,
        headers: &dyn HeaderSource,
        includes: &[String],
        depth: u32,
    ) -> Result<(), String> {
        if depth > 32 {
            return Err(format!("`{path}`: include nesting is too deep (a cycle?)"));
        }
        if self.seen_files.iter().any(|f| f == path) {
            return Ok(()); // include guards, effectively
        }
        self.seen_files.push(path.to_string());
        let fid = self.file_index(path);

        let stripped = strip_comments(text);
        let mut it = Lines::new(&stripped);
        while let Some((line_no, line)) = it.next_logical() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            if let Some(rest) = t.strip_prefix('#') {
                let rest = rest.trim_start();
                if let Some(d) = rest.strip_prefix("define") {
                    self.define(d.trim_start(), fid, line_no);
                } else if let Some(inc) = rest.strip_prefix("include")
                    && let Some(name) = include_name(inc.trim())
                    && let Some((p, txt)) = headers.read(&name, includes)
                {
                    // A header we cannot find is not fatal: the
                    // declarations we did read stay usable, which is the
                    // demotion principle applied to files.
                    self.file(&p, &txt, headers, includes, depth + 1)?;
                }
                // Every other directive (`#if`, `#pragma`, …) is
                // skipped. The reference worker does not evaluate
                // conditionals; see the module docs.
                continue;
            }
            // Gather a full declaration: up to the `;` at brace depth 0,
            // or a `{ … }` body followed by `;`.
            let Some(decl_text) = it.declaration(t) else {
                continue;
            };
            self.declaration(&decl_text, fid, line_no);
        }
        Ok(())
    }

    // -------------------------------------------------------- macros --

    fn define(&mut self, rest: &str, file: u32, line: u32) {
        let loc = SourceLoc { file, line, col: 9 };
        let (name, params, body) = split_define(rest);
        if name.is_empty() {
            return;
        }
        // A macro the request already fixed with `-D` is the request's,
        // not the header's.
        if self.defines.contains_key(&name) {
            return;
        }
        if self.a.macros.iter().any(|m| m.name == name) {
            // Redefinition: the headers do not say which body reaches a
            // wolf call site, so neither do we.
            if let Some(m) = self.a.macros.iter_mut().find(|m| m.name == name) {
                m.status = Status::refuse(Refusal::MacroRedefined);
            }
            return;
        }
        let tokens = tokenize(&body);

        if let Some(params) = params {
            let status = classify_function_macro(&tokens);
            self.a.macros.push(MacroDef {
                name,
                kind: MacroKind::Function {
                    variadic: params.iter().any(|p| p == "..."),
                    params: params.into_iter().filter(|p| p != "...").collect(),
                    tokens,
                },
                loc,
                status,
            });
            return;
        }

        let (value, status) = self.object_macro_value(&tokens);
        self.a.macros.push(MacroDef {
            name,
            kind: MacroKind::Object { value, tokens },
            loc,
            status,
        });
    }

    /// Object-like macros become typed constants when their expansion
    /// parses as a C constant expression; everything else demotes with
    /// a reason.
    fn object_macro_value(&mut self, tokens: &[String]) -> (Option<ConstValue>, Status) {
        if tokens.is_empty() {
            // `#define GUARD` — a flag, not a value. Not an error, and
            // not a constant either.
            return (None, Status::refuse(Refusal::MacroNotConstant));
        }
        if !balanced(tokens) {
            return (None, Status::refuse(Refusal::MacroUnbalanced));
        }
        if tokens.iter().any(|t| t == ";" || t == "{") {
            return (None, Status::refuse(Refusal::MacroExpandsToStatement));
        }
        if tokens.iter().any(|t| t == "##" || t == "#") {
            return (None, Status::refuse(Refusal::MacroTokenPasting));
        }
        if tokens.iter().any(|t| t == "_Generic") {
            return (None, Status::refuse(Refusal::MacroGenericDispatch));
        }

        // A string literal.
        let inner: Vec<&String> = tokens.iter().filter(|t| *t != "(" && *t != ")").collect();
        if inner.len() == 1 && inner[0].starts_with('"') && inner[0].ends_with('"') {
            let s = inner[0].trim_matches('"').to_string();
            return (Some(ConstValue::Str(s.into_bytes())), Status::Ok);
        }
        // An integer constant expression: `-1`, `(-1)`, `0x7fff`, or
        // simple arithmetic over integer literals.
        match eval_int(&inner) {
            Some(v) => {
                let ty = self.a.types.int_(&self.target);
                (Some(ConstValue::Int { value: v, ty }), Status::Ok)
            }
            None => {
                // Names something we may or may not have imported.
                let unknown = inner
                    .iter()
                    .find(|t| is_ident(t) && !self.ordinary.contains_key(**t));
                match unknown {
                    Some(u) => (
                        None,
                        Status::refuse(Refusal::MacroReferencesUnknown((*u).clone())),
                    ),
                    None => (None, Status::refuse(Refusal::MacroNotConstant)),
                }
            }
        }
    }

    // -------------------------------------------------- declarations --

    fn declaration(&mut self, text: &str, file: u32, line: u32) {
        let loc = SourceLoc { file, line, col: 1 };
        let toks = tokenize(text);
        if toks.is_empty() {
            return;
        }
        match toks[0].as_str() {
            "struct" | "union" if toks.iter().any(|t| t == "{") => {
                self.record(&toks, loc);
                return;
            }
            "enum" if toks.iter().any(|t| t == "{") => {
                self.enumeration(&toks, loc);
                return;
            }
            "typedef" => {
                self.typedef(&toks, loc);
                return;
            }
            // `extern "C" {`, `__extension__`, attributes at file scope:
            // not declarations we can name, and not ours to guess at.
            _ => {}
        }
        self.function_or_object(&toks, loc);
    }

    fn record(&mut self, toks: &[String], loc: SourceLoc) {
        let kind = if toks[0] == "union" {
            RecordKind::Union
        } else {
            RecordKind::Struct
        };
        let name = if toks.len() > 1 && is_ident(&toks[1]) {
            toks[1].clone()
        } else {
            String::new()
        };
        let open = toks.iter().position(|t| t == "{").expect("checked");
        let close = matching(toks, open).unwrap_or(toks.len());

        let id = RecordId(self.a.records.len() as u32);
        let ty = self.a.types.intern(CType::Record(id));

        // A union is opaque, always, and says why. Which member is live
        // is a rule the C programmer knows and the header never states.
        let mut opaque = kind == RecordKind::Union;
        let mut fields = Vec::new();
        let mut offset_bits: u64 = 0;
        let mut max_align: u32 = 1;
        let mut max_size: u64 = 0;
        let mut refusal: Option<Refusal> =
            (kind == RecordKind::Union).then_some(Refusal::UnionActiveMember);

        for member in split_members(&toks[open + 1..close]) {
            match self.field(&member) {
                Ok((fname, fty, bits, size, align)) => {
                    max_align = max_align.max(align);
                    max_size = max_size.max(size);
                    let at = match kind {
                        RecordKind::Union => 0,
                        RecordKind::Struct => match bits {
                            // A bitfield packs against the previous one.
                            Some(_) => offset_bits,
                            None => align_to(offset_bits, u64::from(align) * 8),
                        },
                    };
                    match bits {
                        Some(w) => offset_bits = at + u64::from(w),
                        None => offset_bits = at + size * 8,
                    }
                    fields.push(Field {
                        name: fname,
                        ty: fty,
                        offset_bits: at,
                        bit_width: bits,
                    });
                }
                Err(r) => {
                    // A member we could not size makes every *later*
                    // offset a guess, so the record goes opaque rather
                    // than reporting a layout that is wrong from here
                    // down. This is the difference between "we imported
                    // less" and "we imported a lie".
                    opaque = true;
                    refusal.get_or_insert(r);
                }
            }
        }

        let size_bytes = if opaque && kind == RecordKind::Struct {
            // Refused a member: we do not know the size, and saying
            // zero would be worse than saying nothing.
            None
        } else if kind == RecordKind::Union {
            Some(align_to(max_size, u64::from(max_align)))
        } else {
            Some(align_to(offset_bits.div_ceil(8), u64::from(max_align)))
        };

        self.a.records.push(Record {
            name: name.clone(),
            kind,
            size_bytes,
            align_bytes: Some(max_align),
            fields,
            opaque,
        });
        if !name.is_empty() {
            self.tags.insert(format!("{} {name}", kind.tag()), ty);
            self.a.decls.push(Decl {
                name: name.clone(),
                wolf_name: name,
                kind: DeclKind::Tag { ty },
                linkage: Linkage::None,
                loc,
                status: match refusal {
                    Some(r) => Status::refuse_at(r, Demotion::Opaque),
                    None => Status::Ok,
                },
            });
        }
    }

    fn enumeration(&mut self, toks: &[String], loc: SourceLoc) {
        let name = if toks.len() > 1 && is_ident(&toks[1]) {
            toks[1].clone()
        } else {
            String::new()
        };
        let open = toks.iter().position(|t| t == "{").expect("checked");
        let close = matching(toks, open).unwrap_or(toks.len());

        // C picks the underlying type; every target we support picks
        // `int` for an enum that fits in one.
        let underlying = self.a.types.int_(&self.target);
        let id = EnumId(self.a.enums.len() as u32);
        let ty = self.a.types.intern(CType::Enum(id));

        let mut constants = Vec::new();
        let mut next: i128 = 0;
        for item in toks[open + 1..close].split(|t| t == ",") {
            let item: Vec<&String> = item.iter().filter(|t| !t.is_empty()).collect();
            if item.is_empty() {
                continue;
            }
            let cname = item[0].clone();
            if !is_ident(&cname) {
                continue;
            }
            let value = if item.len() > 2 && item[1] == "=" {
                eval_int(&item[2..]).unwrap_or(next)
            } else {
                next
            };
            next = value + 1;
            constants.push((cname.clone(), value));
            self.a.decls.push(Decl {
                name: cname.clone(),
                wolf_name: cname,
                kind: DeclKind::EnumConst { ty, value },
                linkage: Linkage::None,
                loc,
                status: Status::Ok,
            });
        }
        self.a.enums.push(EnumDef {
            name: name.clone(),
            underlying,
            constants,
        });
        if !name.is_empty() {
            self.tags.insert(format!("enum {name}"), ty);
            self.a.decls.push(Decl {
                name: name.clone(),
                wolf_name: name,
                kind: DeclKind::Tag { ty },
                linkage: Linkage::None,
                loc,
                status: Status::Ok,
            });
        }
    }

    fn typedef(&mut self, toks: &[String], loc: SourceLoc) {
        // `typedef <specs> <ptrs> NAME ;`
        let body: Vec<String> = toks[1..].iter().filter(|t| *t != ";").cloned().collect();
        let Some(name) = body.last().cloned() else {
            return;
        };
        if !is_ident(&name) {
            self.refuse_named(
                "(unnamed typedef)",
                loc,
                Refusal::Unmodelled("a typedef whose declarator this worker does not parse".into()),
            );
            return;
        }
        match self.base_type(&body[..body.len() - 1]) {
            Ok((ty, size, align)) => {
                self.typedefs.insert(name.clone(), (ty, size, align));
                self.a.decls.push(Decl {
                    name: name.clone(),
                    wolf_name: name,
                    kind: DeclKind::Typedef { ty },
                    linkage: Linkage::None,
                    loc,
                    status: Status::Ok,
                });
            }
            Err(r) => self.refuse_named(&name, loc, r),
        }
    }

    fn function_or_object(&mut self, toks: &[String], loc: SourceLoc) {
        let toks: Vec<String> = toks.iter().filter(|t| *t != ";").cloned().collect();
        if toks.is_empty() {
            return;
        }
        let is_static = toks.iter().any(|t| t == "static");
        let is_inline = toks.iter().any(|t| t == "inline");
        let specs: Vec<String> = toks
            .into_iter()
            .filter(|t| !matches!(t.as_str(), "extern" | "static" | "inline" | "__inline"))
            .collect();

        let Some(open) = specs.iter().position(|t| t == "(") else {
            // An object declaration: `<specs> <ptrs> NAME`.
            let Some(name) = specs.last().cloned() else {
                return;
            };
            if !is_ident(&name) {
                return;
            }
            match self.base_type(&specs[..specs.len() - 1]) {
                Ok((ty, _, _)) => {
                    self.ordinary.insert(name.clone(), ());
                    self.a.decls.push(Decl {
                        name: name.clone(),
                        wolf_name: name,
                        kind: DeclKind::Object { ty },
                        linkage: if is_static {
                            Linkage::Internal
                        } else {
                            Linkage::External
                        },
                        loc,
                        status: Status::Ok,
                    });
                }
                Err(r) => self.refuse_named(&name, loc, r),
            }
            return;
        };

        if open == 0 {
            return; // not a declaration we can name
        }
        let name = specs[open - 1].clone();
        if !is_ident(&name) {
            // A function-pointer declarator, an attribute, something
            // else: named honestly rather than half-read.
            self.refuse_named(
                "(unnamed declaration)",
                loc,
                Refusal::Unmodelled("a declarator this worker does not parse".into()),
            );
            return;
        }
        let close = matching(&specs, open).unwrap_or(specs.len());

        let ret = match self.base_type(&specs[..open - 1]) {
            Ok((ty, _, _)) => ty,
            Err(r) => {
                self.refuse_fn(&name, loc, r);
                return;
            }
        };

        // Parameters.
        let inner = &specs[open + 1..close.min(specs.len())];
        let mut params = Vec::new();
        let mut variadic = false;
        // `f()` and `f(void)` are different declarations: the first is
        // the old form and promises nothing about its parameters.
        let unprototyped = inner.is_empty();
        let mut refusal = None;
        if unprototyped || (inner.len() == 1 && inner[0] == "void") {
            // `f(void)`: a prototype with no parameters.
        } else {
            for p in inner.split(|t| t == ",") {
                let p: Vec<String> = p.to_vec();
                if p.is_empty() {
                    continue;
                }
                if p.len() == 1 && p[0] == "..." {
                    variadic = true;
                    continue;
                }
                // Drop a parameter name if present.
                let body: Vec<String> = if p.len() > 1 && is_ident(p.last().expect("non-empty")) {
                    p[..p.len() - 1].to_vec()
                } else {
                    p.clone()
                };
                let body = if body.is_empty() { p } else { body };
                match self.base_type(&body) {
                    Ok((ty, _, _)) => params.push(ty),
                    Err(r) => {
                        refusal.get_or_insert(r);
                        break;
                    }
                }
            }
        }

        if unprototyped {
            self.refuse_fn(&name, loc, Refusal::UnprototypedFunction);
            return;
        }
        if let Some(r) = refusal {
            self.refuse_fn(&name, loc, r);
            return;
        }

        let ty = self.a.types.intern(CType::Func {
            ret,
            params,
            variadic,
        });
        self.ordinary.insert(name.clone(), ());
        // A `static inline` in a header has no link-time symbol. The
        // reference worker builds no companion object (that needs a C
        // compiler, which is the libclang worker's business), so it
        // says so rather than emitting a declaration that would fail at
        // link time after a long build — bindgen's exact failure mode.
        let status = if is_inline && is_static {
            Status::refuse(Refusal::InlineWithoutShim)
        } else {
            Status::Ok
        };
        self.a.decls.push(Decl {
            name: name.clone(),
            wolf_name: name,
            kind: DeclKind::Func {
                ty,
                inline_only: is_inline,
            },
            linkage: if is_static {
                Linkage::Internal
            } else {
                Linkage::External
            },
            loc,
            status,
        });
    }

    fn field(
        &mut self,
        toks: &[String],
    ) -> Result<(String, CTypeId, Option<u32>, u64, u32), Refusal> {
        // `<specs> <ptrs> NAME [: bits]`
        let (head, bits) = match toks.iter().position(|t| t == ":") {
            Some(i) => {
                let w = eval_int(&toks[i + 1..].iter().collect::<Vec<_>>())
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or(Refusal::BitfieldLayout)?;
                (&toks[..i], Some(w))
            }
            None => (toks, None),
        };
        if head.is_empty() {
            return Err(Refusal::Unmodelled("an empty struct member".into()));
        }
        // A flexible array member: the length lives in a sibling field.
        if head.len() >= 2 && head[head.len() - 1] == "]" && head[head.len() - 2] == "[" {
            return Err(Refusal::FlexibleArrayMember);
        }
        let name = head.last().cloned().unwrap_or_default();
        if !is_ident(&name) {
            return Err(Refusal::Unmodelled(
                "a struct member declarator this worker does not parse".into(),
            ));
        }
        let (ty, size, align) = self.base_type(&head[..head.len() - 1])?;
        Ok((name, ty, bits, size, align))
    }

    /// Parse a declaration-specifier sequence plus pointer suffixes
    /// into a type, its size in bytes, and its alignment.
    fn base_type(&mut self, toks: &[String]) -> Result<(CTypeId, u64, u32), Refusal> {
        let mut quals = Qual::default();
        let mut words: Vec<&str> = Vec::new();
        let mut ptrs = 0usize;
        for t in toks {
            match t.as_str() {
                "const" => quals.is_const = true,
                "volatile" => quals.is_volatile = true,
                "restrict" | "__restrict" | "__restrict__" => quals.is_restrict = true,
                "*" => ptrs += 1,
                "" => {}
                w => words.push(w),
            }
        }

        let t = &self.target;
        let ptr_bytes = u64::from(t.pointer_bits / 8);
        let ptr_align = u32::from(t.pointer_align_bits / 8);

        // A tag reference (`struct foo *`).
        if words.len() == 2 && matches!(words[0], "struct" | "union" | "enum") {
            let key = format!("{} {}", words[0], words[1]);
            let base = match self.tags.get(&key) {
                Some(&id) => id,
                None => {
                    // Declared but never defined here.
                    if ptrs == 0 {
                        return Err(Refusal::IncompleteType);
                    }
                    let id = RecordId(self.a.records.len() as u32);
                    self.a.records.push(Record {
                        name: words[1].to_string(),
                        kind: if words[0] == "union" {
                            RecordKind::Union
                        } else {
                            RecordKind::Struct
                        },
                        size_bytes: None,
                        align_bytes: None,
                        fields: Vec::new(),
                        opaque: true,
                    });
                    let ty = self.a.types.intern(CType::Record(id));
                    self.tags.insert(key, ty);
                    ty
                }
            };
            // By value, the tag's own size and alignment are what the
            // enclosing record needs to place the next field. Reporting
            // zero here put every following field at the wrong offset
            // while the artifact still said `ok` — the exact shape of
            // failure this importer exists to refuse.
            let (size, align) = match self.a.types.get(base) {
                CType::Record(r) => {
                    let rec = &self.a.records[r.0 as usize];
                    // An opaque record BY VALUE is not usable: its
                    // members are exactly what we said we do not know,
                    // and a by-value parameter needs all of them. A
                    // *pointer* to it stays fine, which is the whole
                    // point of opaque.
                    if ptrs == 0 && rec.opaque {
                        return Err(Refusal::DependsOnRefused(format!(
                            "{} {}",
                            rec.kind.tag(),
                            if rec.name.is_empty() {
                                "(anonymous)"
                            } else {
                                &rec.name
                            }
                        )));
                    }
                    match (rec.size_bytes, rec.align_bytes) {
                        (Some(s), Some(a)) => (s, a),
                        _ if ptrs > 0 => (0, 1),
                        // Incomplete, by value: no size to place it by.
                        _ => return Err(Refusal::IncompleteType),
                    }
                }
                // An enum is its underlying integer.
                CType::Enum(_) => {
                    let bytes = u64::from(self.target.int_bits / 8);
                    (bytes, bytes as u32)
                }
                _ => (0, 1),
            };
            return Ok(self.apply_ptrs_sized(base, ptrs, quals, size, align, ptr_bytes, ptr_align));
        }

        // A typedef name, with the size and alignment recorded when the
        // typedef was read (same reason as above).
        if words.len() == 1
            && let Some(&(id, size, align)) = self.typedefs.get(words[0])
        {
            return Ok(self.apply_ptrs_sized(id, ptrs, quals, size, align, ptr_bytes, ptr_align));
        }

        let joined = words.join(" ");
        let (base, size, align) = match joined.as_str() {
            "void" => {
                if ptrs == 0 {
                    (self.a.types.void(), 0, 1)
                } else {
                    (self.a.types.void(), 1, 1)
                }
            }
            "_Bool" | "bool" => (self.a.types.intern(CType::Bool), 1, 1),
            "char" => (self.a.types.char_(t), 1, 1),
            "signed char" => (
                self.a.types.intern(CType::Int {
                    bits: 8,
                    signed: true,
                    spelling: IntSpelling::Char,
                }),
                1,
                1,
            ),
            "unsigned char" => (
                self.a.types.intern(CType::Int {
                    bits: 8,
                    signed: false,
                    spelling: IntSpelling::Char,
                }),
                1,
                1,
            ),
            "short" | "short int" | "signed short" => (
                int_ty(self.a, t.short_bits, true, IntSpelling::Short),
                u64::from(t.short_bits / 8),
                (t.short_bits / 8) as u32,
            ),
            "unsigned short" | "unsigned short int" => (
                int_ty(self.a, t.short_bits, false, IntSpelling::Short),
                u64::from(t.short_bits / 8),
                (t.short_bits / 8) as u32,
            ),
            "int" | "signed" | "signed int" => (
                int_ty(self.a, t.int_bits, true, IntSpelling::Int),
                u64::from(t.int_bits / 8),
                (t.int_bits / 8) as u32,
            ),
            "unsigned" | "unsigned int" => (
                int_ty(self.a, t.int_bits, false, IntSpelling::Int),
                u64::from(t.int_bits / 8),
                (t.int_bits / 8) as u32,
            ),
            "long" | "long int" | "signed long" => (
                int_ty(self.a, t.long_bits, true, IntSpelling::Long),
                u64::from(t.long_bits / 8),
                (t.long_bits / 8) as u32,
            ),
            "unsigned long" | "unsigned long int" => (
                int_ty(self.a, t.long_bits, false, IntSpelling::Long),
                u64::from(t.long_bits / 8),
                (t.long_bits / 8) as u32,
            ),
            "long long" | "long long int" | "signed long long" => (
                int_ty(self.a, t.long_long_bits, true, IntSpelling::LongLong),
                u64::from(t.long_long_bits / 8),
                (t.long_long_bits / 8) as u32,
            ),
            "unsigned long long" | "unsigned long long int" => (
                int_ty(self.a, t.long_long_bits, false, IntSpelling::LongLong),
                u64::from(t.long_long_bits / 8),
                (t.long_long_bits / 8) as u32,
            ),
            "size_t" => {
                let ty = self.a.types.size_t(t);
                (ty, u64::from(t.size_t_bits / 8), (t.size_t_bits / 8) as u32)
            }
            "ssize_t" | "ptrdiff_t" => (
                int_ty(self.a, t.size_t_bits, true, IntSpelling::SizeT),
                u64::from(t.size_t_bits / 8),
                (t.size_t_bits / 8) as u32,
            ),
            "wchar_t" => (
                int_ty(self.a, t.wchar_bits, t.wchar_signed, IntSpelling::WcharT),
                u64::from(t.wchar_bits / 8),
                (t.wchar_bits / 8) as u32,
            ),
            "float" => (self.a.types.intern(CType::Float { bits: 32 }), 4, 4),
            "double" => (self.a.types.intern(CType::Float { bits: 64 }), 8, 8),
            // Refused by name rather than approximated — every one of
            // these has a wrong-but-plausible mapping that would
            // compile and misbehave.
            "long double" => return Err(Refusal::LongDouble),
            _ if joined.starts_with("_Complex") || joined.contains("_Complex") => {
                return Err(Refusal::ComplexType);
            }
            _ if joined.starts_with("_Atomic") => return Err(Refusal::AtomicType),
            _ if joined.starts_with("_BitInt") || joined.contains("__int128") => {
                return Err(Refusal::BitInt);
            }
            _ if joined.contains("__vector") || joined.contains("vector_size") => {
                return Err(Refusal::VectorType);
            }
            other => {
                return Err(Refusal::Unmodelled(format!(
                    "the type `{other}` is outside the reference worker's subset"
                )));
            }
        };
        Ok(self.apply_ptrs_sized(base, ptrs, quals, size, align, ptr_bytes, ptr_align))
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_ptrs_sized(
        &mut self,
        base: CTypeId,
        ptrs: usize,
        quals: Qual,
        size: u64,
        align: u32,
        ptr_bytes: u64,
        ptr_align: u32,
    ) -> (CTypeId, u64, u32) {
        if ptrs == 0 {
            return (base, size, align);
        }
        let mut ty = base;
        for i in 0..ptrs {
            // The qualifiers belong to the innermost pointee.
            let q = if i == 0 { quals } else { Qual::default() };
            ty = self.a.types.ptr_to(ty, q);
        }
        (ty, ptr_bytes, ptr_align)
    }

    fn refuse_named(&mut self, name: &str, loc: SourceLoc, r: Refusal) {
        self.a.decls.push(Decl {
            name: name.to_string(),
            wolf_name: name.to_string(),
            kind: DeclKind::Typedef {
                ty: self.a.types.intern(CType::Refused(r.tag().to_string())),
            },
            linkage: Linkage::None,
            loc,
            status: Status::refuse(r),
        });
    }

    fn refuse_fn(&mut self, name: &str, loc: SourceLoc, r: Refusal) {
        let ty = self.a.types.intern(CType::Refused(r.tag().to_string()));
        self.ordinary.insert(name.to_string(), ());
        self.a.decls.push(Decl {
            name: name.to_string(),
            wolf_name: name.to_string(),
            kind: DeclKind::Func {
                ty,
                inline_only: false,
            },
            linkage: Linkage::External,
            loc,
            status: Status::refuse_at(r, Demotion::ExternOnly),
        });
    }
}

fn int_ty(a: &mut Artifact, bits: u16, signed: bool, spelling: IntSpelling) -> CTypeId {
    a.types.intern(CType::Int {
        bits,
        signed,
        spelling,
    })
}

fn align_to(v: u64, a: u64) -> u64 {
    if a == 0 { v } else { v.div_ceil(a) * a }
}

/// C keeps tags and ordinary identifiers in separate name spaces
/// (c23-n3220 §6.2.3); wolf's `c` namespace is one. When a tag collides
/// with an ordinary identifier, the *tag* is renamed — visibly, and
/// with its C name kept in the artifact.
fn resolve_tag_collisions(a: &mut Artifact) {
    let ordinary: std::collections::BTreeSet<String> = a
        .decls
        .iter()
        .filter(|d| !matches!(d.kind, DeclKind::Tag { .. }))
        .map(|d| d.name.clone())
        .collect();
    for d in &mut a.decls {
        let DeclKind::Tag { ty } = &d.kind else {
            continue;
        };
        if !ordinary.contains(&d.name) {
            continue;
        }
        let prefix = match a.types.get(*ty) {
            CType::Enum(_) => "enum",
            CType::Record(r) => a
                .records
                .get(r.0 as usize)
                .map(|rec| rec.kind.tag())
                .unwrap_or("struct"),
            _ => "struct",
        };
        d.wolf_name = format!("{prefix}_{}", d.name);
    }
}

// ------------------------------------------------------- text helpers --

/// Replace comments with spaces, preserving offsets so line numbers
/// stay right. String and char literals are respected.
fn strip_comments(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                let mut j = i + 2;
                while j + 1 < b.len() && !(b[j] == b'*' && b[j + 1] == b'/') {
                    out.push(if b[j] == b'\n' { '\n' } else { ' ' });
                    j += 1;
                }
                out.push_str("  ");
                i = (j + 2).min(b.len());
                if i > b.len() {
                    break;
                }
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                while i < b.len() && b[i] != b'\n' {
                    out.push(' ');
                    i += 1;
                }
            }
            q @ (b'"' | b'\'') => {
                out.push(q as char);
                i += 1;
                while i < b.len() && b[i] != q {
                    if b[i] == b'\\' && i + 1 < b.len() {
                        out.push(b[i] as char);
                        i += 1;
                    }
                    out.push(b[i] as char);
                    i += 1;
                }
                if i < b.len() {
                    out.push(q as char);
                    i += 1;
                }
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}

/// A line reader that joins backslash continuations and can gather a
/// whole declaration.
struct Lines<'a> {
    lines: Vec<&'a str>,
    at: usize,
}

impl<'a> Lines<'a> {
    fn new(s: &'a str) -> Lines<'a> {
        Lines {
            lines: s.lines().collect(),
            at: 0,
        }
    }

    /// The next logical line (continuations joined), with its 1-based
    /// starting line number.
    fn next_logical(&mut self) -> Option<(u32, String)> {
        let start = self.at;
        if start >= self.lines.len() {
            return None;
        }
        let mut buf = String::new();
        while let Some(l) = self.lines.get(self.at) {
            self.at += 1;
            // A trailing backslash joins the next line (phase 2).
            match l.strip_suffix('\\') {
                Some(head) => {
                    buf.push_str(head);
                    buf.push(' ');
                }
                None => {
                    buf.push_str(l);
                    break;
                }
            }
        }
        Some(((start + 1) as u32, buf))
    }

    /// Gather from `first` to the end of a declaration: the `;` at
    /// brace depth zero, or a balanced `{ … }` followed by `;`.
    fn declaration(&mut self, first: &str) -> Option<String> {
        let mut buf = first.to_string();
        let mut depth = brace_delta(first);
        let mut paren = paren_delta(first);
        while (depth > 0 || paren > 0) || !buf.contains(';') {
            let (_, next) = self.next_logical()?;
            if next.trim_start().starts_with('#') {
                // A directive inside a declaration means we lost the
                // thread; give up on this one rather than mis-read it.
                return Some(buf);
            }
            depth += brace_delta(&next);
            paren += paren_delta(&next);
            buf.push(' ');
            buf.push_str(&next);
            if buf.len() > 64 * 1024 {
                return Some(buf);
            }
        }
        Some(buf)
    }
}

fn brace_delta(s: &str) -> i32 {
    s.chars()
        .map(|c| match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        })
        .sum()
}

fn paren_delta(s: &str) -> i32 {
    s.chars()
        .map(|c| match c {
            '(' => 1,
            ')' => -1,
            _ => 0,
        })
        .sum()
}

fn include_name(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('<') {
        return rest.split('>').next().map(str::to_string);
    }
    if let Some(rest) = s.strip_prefix('"') {
        return rest.split('"').next().map(str::to_string);
    }
    None
}

/// `NAME(a, b) body` / `NAME body` → (name, params, body).
fn split_define(s: &str) -> (String, Option<Vec<String>>, String) {
    let s = s.trim();
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    let name = s[..end].to_string();
    let rest = &s[end..];
    // Function-like only when `(` touches the name — `#define A (1)` is
    // an object-like macro whose body is parenthesised.
    if let Some(inner) = rest.strip_prefix('(')
        && let Some(close) = inner.find(')')
    {
        let params: Vec<String> = inner[..close]
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        return (name, Some(params), inner[close + 1..].trim().to_string());
    }
    (name, None, rest.trim().to_string())
}

/// A C token scanner: identifiers, numbers, string/char literals, and
/// the punctuation the subset needs.
pub fn tokenize(s: &str) -> Vec<String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let st = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(s[st..i].to_string());
            continue;
        }
        if c.is_ascii_digit() {
            let st = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                i += 1;
            }
            out.push(s[st..i].to_string());
            continue;
        }
        if c == b'"' || c == b'\'' {
            let st = i;
            i += 1;
            while i < b.len() && b[i] != c {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(b.len());
            out.push(s[st..i].to_string());
            continue;
        }
        // Multi-character punctuators, longest first. Exactly one match
        // per position, and the loop always advances — the alternative
        // spins forever on `...` in a variadic parameter list, which is
        // how this was found.
        let multi = [
            "...", "<<=", ">>=", "<<", ">>", "->", "##", "|=", "&=", "^=", "==", "!=", "<=", ">=",
            "&&", "||", "+=", "-=", "*=", "/=",
        ];
        if let Some(p) = multi.iter().find(|p| s[i..].starts_with(**p)) {
            out.push((*p).to_string());
            i += p.len();
            continue;
        }
        // Any other single byte is its own token. `\r` is line noise.
        if c != b'\r' {
            out.push(s[i..i + 1].to_string());
        }
        i += 1;
    }
    out
}

fn is_ident(s: &str) -> bool {
    let mut cs = s.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn balanced(toks: &[String]) -> bool {
    let mut d = 0i32;
    for t in toks {
        match t.as_str() {
            "(" | "[" | "{" => d += 1,
            ")" | "]" | "}" => d -= 1,
            _ => {}
        }
        if d < 0 {
            return false;
        }
    }
    d == 0
}

fn matching(toks: &[String], open: usize) -> Option<usize> {
    let (o, c) = match toks.get(open)?.as_str() {
        "(" => ("(", ")"),
        "{" => ("{", "}"),
        "[" => ("[", "]"),
        _ => return None,
    };
    let mut d = 0i32;
    for (i, t) in toks.iter().enumerate().skip(open) {
        if t == o {
            d += 1;
        } else if t == c {
            d -= 1;
            if d == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Split a struct body's tokens into members at top-level `;`.
fn split_members(toks: &[String]) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    let mut d = 0i32;
    for t in toks {
        match t.as_str() {
            "{" | "(" | "[" => d += 1,
            "}" | ")" | "]" => d -= 1,
            ";" if d == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                continue;
            }
            _ => {}
        }
        cur.push(t.clone());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Evaluate an integer constant expression over literals. Deliberately
/// small: `-1`, `(3)`, `0x1f`, `1 << 4`, `2 + 3`. Anything else is
/// `None`, which becomes a refusal rather than a guess.
fn eval_int<S: AsRef<str>>(toks: &[S]) -> Option<i128> {
    let t: Vec<&str> = toks
        .iter()
        .map(|s| s.as_ref())
        .filter(|s| *s != "(" && *s != ")")
        .collect();
    if t.is_empty() {
        return None;
    }
    if t.len() == 1 {
        return parse_int(t[0]);
    }
    if t.len() == 2 && t[0] == "-" {
        return parse_int(t[1]).map(|v| -v);
    }
    if t.len() == 3 {
        let a = parse_int(t[0])?;
        let b = parse_int(t[2])?;
        return match t[1] {
            "+" => a.checked_add(b),
            "-" => a.checked_sub(b),
            "*" => a.checked_mul(b),
            "/" => (b != 0).then(|| a / b),
            "<<" => (0..127).contains(&b).then(|| a << b),
            ">>" => (0..127).contains(&b).then(|| a >> b),
            "|" => Some(a | b),
            "&" => Some(a & b),
            "^" => Some(a ^ b),
            _ => None,
        };
    }
    None
}

fn parse_int(s: &str) -> Option<i128> {
    let s = s.trim_end_matches(['u', 'U', 'l', 'L']);
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return i128::from_str_radix(h, 16).ok();
    }
    if s.len() > 1 && s.starts_with('0') && s[1..].bytes().all(|c| (b'0'..=b'7').contains(&c)) {
        return i128::from_str_radix(&s[1..], 8).ok();
    }
    s.parse().ok()
}

/// Classify a function-like macro. The v0 contract: expansions that are
/// C *expressions* over the arguments are kept alive; the rest are
/// recorded and refused by class, each with the inline-C escape.
fn classify_function_macro(tokens: &[String]) -> Status {
    if tokens.is_empty() {
        return Status::refuse(Refusal::MacroNotConstant);
    }
    if !balanced(tokens) {
        return Status::refuse(Refusal::MacroUnbalanced);
    }
    if tokens.iter().any(|t| t == "##" || t == "#") {
        return Status::refuse(Refusal::MacroTokenPasting);
    }
    if tokens.iter().any(|t| t == "_Generic") {
        return Status::refuse(Refusal::MacroGenericDispatch);
    }
    if tokens.iter().any(|t| t == ";") || tokens.first().map(String::as_str) == Some("{") {
        return Status::refuse(Refusal::MacroExpandsToStatement);
    }
    // `container_of`-shaped: a cast or `sizeof`/`offsetof` over
    // something that must be a type. The give-away is a type keyword
    // appearing where an argument would.
    if tokens
        .iter()
        .any(|t| matches!(t.as_str(), "typeof" | "__typeof__" | "offsetof"))
    {
        return Status::refuse(Refusal::MacroTypeArgument);
    }
    Status::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import_text(src: &str) -> Artifact {
        let req = ImportRequest {
            headers: vec!["t.h".into()],
            target: "x86_64-unknown-linux-gnu".into(),
            ..Default::default()
        };
        import(&req, &MemHeaders::new([("t.h", src)])).expect("imports")
    }

    #[test]
    fn reads_a_function_prototype() {
        let a = import_text("void *malloc(size_t n);\nvoid free(void *p);\n");
        let m = a.decl("malloc").expect("malloc");
        assert!(m.status.is_ok());
        assert_eq!(a.types.spell(m.kind.ty()), "void *(size_t)");
        let f = a.decl("free").expect("free");
        assert_eq!(a.types.spell(f.kind.ty()), "void(void *)");
    }

    /// One cursed declaration must not cost the header.
    #[test]
    fn a_refused_decl_does_not_kill_its_siblings() {
        let a = import_text(
            "int good_one(int x);\n\
             long double cursed(long double x);\n\
             int good_two(int y);\n",
        );
        assert!(a.decl("good_one").expect("present").status.is_ok());
        assert!(a.decl("good_two").expect("present").status.is_ok());
        let c = a.decl("cursed").expect("present");
        assert_eq!(
            match &c.status {
                Status::Refused { refusal, .. } => refusal.tag(),
                Status::Ok => "ok",
            },
            "long-double"
        );
    }

    /// A union always demotes, and says why.
    #[test]
    fn a_union_is_opaque_by_name() {
        let a = import_text("union value { int i; void *p; };\n");
        let d = a.decl("value").expect("the tag");
        match &d.status {
            Status::Refused { demotion, refusal } => {
                assert_eq!(refusal.tag(), "union-active-member");
                assert_eq!(*demotion, Demotion::Opaque);
            }
            Status::Ok => panic!("a union must not import as transparent"),
        }
        assert!(a.records[0].opaque);
    }

    /// A pointer to an opaque type is usable; the type by value is not.
    /// Importing a by-value union parameter as `ok` would hand a call
    /// site a layout the importer just said it does not know.
    #[test]
    fn an_opaque_type_crosses_by_pointer_only() {
        let a = import_text(
            "union value { int i; void *p; };\n\
             void by_value(union value v);\n\
             void by_pointer(union value *v);\n",
        );
        assert!(
            a.decl("by_pointer").expect("present").status.is_ok(),
            "a pointer to an opaque type is an ordinary address"
        );
        let by_value = a.decl("by_value").expect("present");
        match &by_value.status {
            Status::Refused { refusal, demotion } => {
                assert_eq!(refusal.tag(), "depends-on-refused");
                assert!(refusal.payload().contains("value"), "{refusal:?}");
                // Error-on-use, not extern-only: extern-only means "you
                // may hand-assert a signature", and there is no wolf
                // signature to assert — the parameter type is one wolf
                // cannot spell at all.
                assert_eq!(*demotion, Demotion::ErrorOnUse);
            }
            Status::Ok => panic!("a by-value union needs the layout we refused to know"),
        }
    }

    #[test]
    fn bitfields_get_offsets_and_widths() {
        let a =
            import_text("struct flags { unsigned a : 1; unsigned b : 3; unsigned rest : 28; };\n");
        let r = &a.records[0];
        assert_eq!(r.fields.len(), 3);
        assert_eq!(
            r.fields
                .iter()
                .map(|f| (f.offset_bits, f.bit_width))
                .collect::<Vec<_>>(),
            vec![(0, Some(1)), (1, Some(3)), (4, Some(28))]
        );
    }

    /// Regression: a record used *by value* as a field reported size 0,
    /// so every field after it landed at the wrong offset — while the
    /// artifact still said `ok`. A wrong layout that claims to be right
    /// is the worst thing this importer can produce.
    #[test]
    fn a_nested_record_field_advances_the_offset() {
        let a = import_text(
            "struct point { int x; int y; };\n\
             struct nested { struct point origin; unsigned char tag; size_t count; };\n",
        );
        let point = a.records.iter().find(|r| r.name == "point").expect("point");
        assert_eq!((point.size_bytes, point.align_bytes), (Some(8), Some(4)));

        let nested = a
            .records
            .iter()
            .find(|r| r.name == "nested")
            .expect("nested");
        assert_eq!(
            nested
                .fields
                .iter()
                .map(|f| (f.name.as_str(), f.offset_bits / 8))
                .collect::<Vec<_>>(),
            vec![("origin", 0), ("tag", 8), ("count", 16)],
            "the nested struct occupies its own size, and `count` aligns to 8"
        );
        assert_eq!((nested.size_bytes, nested.align_bytes), (Some(24), Some(8)));
    }

    /// A member the worker could not size makes every later offset a
    /// guess, so the record goes opaque and loses its size rather than
    /// publishing offsets it cannot stand behind.
    #[test]
    fn a_record_with_an_unsizeable_member_reports_no_layout() {
        let a = import_text("struct risky { long double weird; int after; };\n");
        let r = a.records.iter().find(|r| r.name == "risky").expect("risky");
        assert!(
            r.opaque,
            "a record with a refused member must not look whole"
        );
        assert_eq!(
            r.size_bytes, None,
            "reporting a size here would be reporting a guess"
        );
    }

    #[test]
    fn a_typedef_keeps_its_size_for_later_fields() {
        let a = import_text(
            "typedef unsigned long ulong_t;\n\
             struct pair { ulong_t a; unsigned char b; ulong_t c; };\n",
        );
        let r = a.records.iter().find(|r| r.name == "pair").expect("pair");
        assert_eq!(
            r.fields
                .iter()
                .map(|f| f.offset_bits / 8)
                .collect::<Vec<_>>(),
            vec![0, 8, 16]
        );
    }

    #[test]
    fn enums_carry_their_values() {
        let a = import_text("enum color { RED, GREEN, BLUE = 7, NEXT };\n");
        assert_eq!(
            a.enums[0].constants,
            vec![
                ("RED".to_string(), 0),
                ("GREEN".to_string(), 1),
                ("BLUE".to_string(), 7),
                ("NEXT".to_string(), 8)
            ]
        );
        assert!(a.decl("BLUE").expect("constant").status.is_ok());
    }

    /// Object-like macros that are constant expressions become values;
    /// the rest demote with a named reason.
    #[test]
    fn object_macros_become_constants_or_say_why_not() {
        let a = import_text(
            "#define EOF (-1)\n\
             #define SEEK_SET 0\n\
             #define BUFSIZ 0x2000\n\
             #define SHIFTED (1 << 4)\n\
             #define GREETING \"hi\"\n\
             #define STMT do { x(); } while (0)\n\
             #define FRAGMENT ) + 1\n",
        );
        let val = |n: &str| match &a.macro_(n).expect(n).kind {
            MacroKind::Object { value, .. } => value.clone(),
            _ => None,
        };
        assert!(matches!(
            val("EOF"),
            Some(ConstValue::Int { value: -1, .. })
        ));
        assert!(matches!(
            val("SEEK_SET"),
            Some(ConstValue::Int { value: 0, .. })
        ));
        assert!(matches!(
            val("BUFSIZ"),
            Some(ConstValue::Int { value: 8192, .. })
        ));
        assert!(matches!(
            val("SHIFTED"),
            Some(ConstValue::Int { value: 16, .. })
        ));
        assert!(matches!(val("GREETING"), Some(ConstValue::Str(_))));

        let why = |n: &str| match &a.macro_(n).expect(n).status {
            Status::Refused { refusal, .. } => refusal.tag().to_string(),
            Status::Ok => "ok".to_string(),
        };
        assert_eq!(why("STMT"), "macro-expands-to-statement");
        assert_eq!(why("FRAGMENT"), "macro-unbalanced");
    }

    /// Function-like macros stay alive; the deferred classes are named.
    #[test]
    fn function_macros_are_kept_or_refused_by_class() {
        let a = import_text(
            "#define SET(d, s) ((s)->bits |= (1 << (d)))\n\
             #define CONTAINER_OF(p, m) ((typeof(m) *)(p))\n\
             #define GLUE(a, b) a ## b\n\
             #define PICK(x) _Generic((x), int: 1, default: 0)\n",
        );
        assert!(a.macro_("SET").expect("SET").status.is_ok());
        let why = |n: &str| match &a.macro_(n).expect(n).status {
            Status::Refused { refusal, .. } => refusal.tag().to_string(),
            Status::Ok => "ok".to_string(),
        };
        assert_eq!(why("CONTAINER_OF"), "macro-type-argument");
        assert_eq!(why("GLUE"), "macro-token-pasting");
        assert_eq!(why("PICK"), "macro-generic-dispatch");
        // The tokens are kept regardless — a refused macro is still
        // recorded, so the diagnostic can quote it.
        assert!(!a.macro_("GLUE").expect("GLUE").kind.tokens().is_empty());
    }

    /// C's separate name spaces collapse into one wolf namespace, and
    /// the rename is visible rather than a silent winner.
    #[test]
    fn a_tag_colliding_with_a_function_is_renamed_visibly() {
        let a = import_text(
            "struct stat { size_t st_size; };\n\
             int stat(const char *path, struct stat *out);\n",
        );
        let tag = a.decl("struct_stat").expect("renamed tag");
        assert_eq!(tag.name, "stat", "the C name is kept");
        assert!(matches!(tag.kind, DeclKind::Tag { .. }));
        let f = a.decl("stat").expect("the function keeps the plain name");
        assert!(matches!(f.kind, DeclKind::Func { .. }));
    }

    /// A header-only `static inline` has no symbol to link. Saying so
    /// at import time is the whole point — bindgen's failure mode is a
    /// link error after a long build.
    #[test]
    fn static_inline_without_a_shim_is_refused_at_import() {
        let a = import_text("static inline int twice(int x);\n");
        let d = a.decl("twice").expect("present");
        match &d.status {
            Status::Refused { refusal, .. } => assert_eq!(refusal.tag(), "inline-without-shim"),
            Status::Ok => panic!("a header-only inline has no link-time symbol"),
        }
    }

    #[test]
    fn static_functions_have_internal_linkage() {
        let a = import_text("static int helper(int x);\n");
        assert_eq!(
            a.decl("helper").expect("present").linkage,
            Linkage::Internal
        );
    }

    /// An unprototyped declaration promises nothing about its
    /// parameters, and pretending otherwise is how a call goes wrong.
    #[test]
    fn k_and_r_declarations_are_refused() {
        let a = import_text("int ancient();\n");
        let d = a.decl("ancient").expect("present");
        match &d.status {
            Status::Refused { refusal, demotion } => {
                assert_eq!(refusal.tag(), "unprototyped-function");
                assert_eq!(*demotion, Demotion::ExternOnly);
            }
            Status::Ok => panic!("`int f()` promises nothing"),
        }
    }

    #[test]
    fn includes_are_followed_and_guarded_against_cycles() {
        let src = MemHeaders::new([
            ("a.h", "#include \"b.h\"\nint from_a(void);\n"),
            ("b.h", "#include \"a.h\"\nint from_b(void);\n"),
        ]);
        let req = ImportRequest {
            headers: vec!["a.h".into()],
            target: "x86_64-unknown-linux-gnu".into(),
            ..Default::default()
        };
        let a = import(&req, &src).expect("imports");
        assert!(a.decl("from_a").is_some());
        assert!(a.decl("from_b").is_some());
    }

    /// Widths follow the target, which is the reason they are in the
    /// artifact at all.
    #[test]
    fn the_same_header_imports_differently_per_target() {
        let src = "long width(long x);\n";
        let mk = |t: &str| {
            let req = ImportRequest {
                headers: vec!["t.h".into()],
                target: t.into(),
                ..Default::default()
            };
            import(&req, &MemHeaders::new([("t.h", src)])).expect("imports")
        };
        let lin = mk("x86_64-unknown-linux-gnu");
        let win = mk("x86_64-pc-windows-msvc");
        let bits = |a: &Artifact| match a.types.get(
            match a.types.get(a.decl("width").expect("present").kind.ty()) {
                CType::Func { ret, .. } => *ret,
                _ => unreachable!(),
            },
        ) {
            CType::Int { bits, .. } => *bits,
            _ => unreachable!(),
        };
        assert_eq!(bits(&lin), 64);
        assert_eq!(bits(&win), 32, "LLP64: `long` is 32 bits on windows");
    }

    #[test]
    fn an_unknown_target_is_refused_not_guessed() {
        let req = ImportRequest {
            headers: vec!["t.h".into()],
            target: "vax-unknown-vms".into(),
            ..Default::default()
        };
        let e = import(&req, &MemHeaders::new([("t.h", "int f(void);")])).expect_err("refuses");
        assert!(e.contains("will not guess"), "{e}");
    }

    #[test]
    fn a_missing_header_names_the_paths_it_searched() {
        let req = ImportRequest {
            headers: vec!["nope.h".into()],
            include_paths: vec!["/a".into(), "/b".into()],
            target: "x86_64-unknown-linux-gnu".into(),
            ..Default::default()
        };
        let e = import(&req, &MemHeaders::default()).expect_err("refuses");
        assert!(e.contains("/a, /b"), "{e}");
    }

    /// Regression: the tokenizer used to spin forever on the token
    /// *after* a `...`, so any variadic declaration hung the importer.
    /// The scanner must always advance.
    #[test]
    fn the_tokenizer_always_advances() {
        for src in [
            "int printf(const char *fmt, ...);",
            "#define V(...) f(__VA_ARGS__)",
            "a ... b ...., ...",
            "x <<= 1 >>= 2 &&|| ^= !=",
            "...",
            ".",
            "",
        ] {
            // A tokenizer that does not advance never returns, so
            // reaching the assertion at all is most of the test.
            let toks = tokenize(src);
            let rejoined: usize = toks.iter().map(|t| t.len()).sum();
            assert!(
                rejoined <= src.len(),
                "tokenizing `{src}` produced more text than it consumed"
            );
        }
    }

    #[test]
    fn variadic_functions_import_as_variadic() {
        let a = import_text("int printf(const char *fmt, ...);\nint plain(int x);\n");
        let variadic = |n: &str| match a.types.get(a.decl(n).expect(n).kind.ty()) {
            CType::Func { variadic, .. } => *variadic,
            other => panic!("{n} is not a function: {other:?}"),
        };
        assert!(variadic("printf"));
        assert!(!variadic("plain"));
    }

    #[test]
    fn comments_do_not_confuse_the_scanner() {
        let a = import_text(
            "/* a block\n   comment */\n\
             int one(void); // trailing\n\
             /* int hidden(void); */\n\
             int two(void);\n",
        );
        assert!(a.decl("one").is_some());
        assert!(a.decl("two").is_some());
        assert!(
            a.decl("hidden").is_none(),
            "a commented-out decl is not a decl"
        );
    }

    /// The worker answers macro expansion honestly rather than
    /// inventing a plausible expansion.
    #[test]
    fn macro_expansion_is_refused_with_a_reason() {
        let r = serve(
            &Request::ExpandMacro {
                key: "b3:0".into(),
                name: "FD_SET".into(),
                arg_types: vec!["int".into()],
            },
            &MemHeaders::default(),
        );
        match r {
            Response::Err(m) => {
                assert!(m.contains("FD_SET"), "{m}");
                assert!(m.contains("preprocessor"), "{m}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// End to end through the protocol: a request in, an artifact out.
    #[test]
    fn serve_round_trips_through_the_protocol() {
        let req = Request::Import(ImportRequest {
            headers: vec!["t.h".into()],
            target: "x86_64-unknown-linux-gnu".into(),
            ..Default::default()
        });
        let resp = serve(&req, &MemHeaders::new([("t.h", "int f(int x);")]));
        let wire = resp.render();
        let back = Response::parse(&wire).expect("parses");
        let Response::Artifact(bytes) = back else {
            panic!("expected an artifact, got {back:?}");
        };
        let a = crate::encode::decode(&bytes).expect("decodes");
        assert!(a.decl("f").is_some());
        assert_eq!(a.importer, REFERENCE_WORKER_ID);
    }
}
