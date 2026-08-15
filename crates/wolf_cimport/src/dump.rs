//! The textual dump — `wolf c-import --dump`.
//!
//! Lossless: every field of the artifact appears, so a dump is a
//! reviewable diff of what the importer believes about a header set.
//! This is the form the **conformance suite** snapshots, and the form
//! c15's frontend will be held to: a new importer is correct when its
//! dump matches, not when its internals look right.
//!
//! Deterministic by construction — sorted tables, fixed field order, no
//! host paths beyond the ones the artifact itself records.

use std::fmt::Write as _;

use crate::artifact::{Artifact, ConstValue, DeclKind, MacroKind};
use crate::ctype::CType;
use crate::refuse::Status;

/// Render the artifact.
pub fn dump(a: &Artifact) -> String {
    let mut o = String::new();
    let t = &a.target;

    let _ = writeln!(o, "c-import artifact format {}", a.format_version);
    let _ = writeln!(o, "importer: {}", a.importer);
    let _ = writeln!(o, "target: {}", t.triple);
    let _ = writeln!(
        o,
        "  ptr={} align={} short={} int={} long={} longlong={} size_t={} wchar={}{}",
        t.pointer_bits,
        t.pointer_align_bits,
        t.short_bits,
        t.int_bits,
        t.long_bits,
        t.long_long_bits,
        t.size_t_bits,
        t.wchar_bits,
        if t.wchar_signed { "" } else { "u" },
    );
    let _ = writeln!(
        o,
        "  char={} endian={}",
        if t.char_signed { "signed" } else { "unsigned" },
        t.endian.tag()
    );
    let _ = writeln!(o, "headers: {}", join(&a.headers));
    for (i, f) in a.files.iter().enumerate() {
        let _ = writeln!(o, "file {i}: {f}");
    }

    // ------------------------------------------------------- types --
    let _ = writeln!(o, "\ntypes ({}):", a.types.len());
    for (id, ty) in a.types.iter() {
        let _ = writeln!(o, "  #{} {}", id.0, describe_type(a, ty));
    }

    // ----------------------------------------------------- records --
    if !a.records.is_empty() {
        let _ = writeln!(o, "\nrecords ({}):", a.records.len());
        for (i, r) in a.records.iter().enumerate() {
            let name = if r.name.is_empty() {
                "(anonymous)"
            } else {
                &r.name
            };
            let size = r
                .size_bytes
                .map(|s| s.to_string())
                .unwrap_or_else(|| "incomplete".into());
            let align = r
                .align_bytes
                .map(|s| s.to_string())
                .unwrap_or_else(|| "-".into());
            let _ = writeln!(
                o,
                "  record#{i} {} {name} size={size} align={align}{}",
                r.kind.tag(),
                if r.opaque { " opaque" } else { "" }
            );
            for f in &r.fields {
                match f.bit_width {
                    Some(w) => {
                        let _ = writeln!(
                            o,
                            "    {} : #{} @bit {} width {w}",
                            f.name, f.ty.0, f.offset_bits
                        );
                    }
                    None => {
                        let _ = writeln!(
                            o,
                            "    {} : #{} @byte {}",
                            f.name,
                            f.ty.0,
                            f.offset_bits / 8
                        );
                    }
                }
            }
        }
    }

    // ------------------------------------------------------- enums --
    if !a.enums.is_empty() {
        let _ = writeln!(o, "\nenums ({}):", a.enums.len());
        for (i, e) in a.enums.iter().enumerate() {
            let name = if e.name.is_empty() {
                "(anonymous)"
            } else {
                &e.name
            };
            let _ = writeln!(o, "  enum#{i} {name} : #{}", e.underlying.0);
            for (n, v) in &e.constants {
                let _ = writeln!(o, "    {n} = {v}");
            }
        }
    }

    // ------------------------------------------------------- decls --
    let _ = writeln!(o, "\ndecls ({}):", a.decls.len());
    for d in &a.decls {
        let spelling = a.types.spell(d.kind.ty());
        // A renamed declaration always shows its C name: the rename is
        // the importer's doing (tag/ordinary name-space collisions,
        // c23-n3220 §6.2.3) and hiding it would make the wolf spelling
        // look like something the header chose.
        let renamed = if d.wolf_name == d.name {
            String::new()
        } else {
            format!(" (c name `{}`)", d.name)
        };
        let extra = match &d.kind {
            DeclKind::Func {
                inline_only: true, ..
            } => " inline-only".to_string(),
            DeclKind::EnumConst { value, .. } => format!(" = {value}"),
            _ => String::new(),
        };
        let _ = writeln!(
            o,
            "  {} {}{renamed} : {spelling} [{}]{extra} {} @{}",
            d.kind.tag(),
            d.wolf_name,
            d.linkage.tag(),
            status_str(&d.status),
            loc(a, d.loc),
        );
    }

    // ------------------------------------------------------ macros --
    let _ = writeln!(o, "\nmacros ({}):", a.macros.len());
    for m in &a.macros {
        match &m.kind {
            MacroKind::Object { value, tokens } => {
                let v = match value {
                    Some(ConstValue::Int { value, ty }) => {
                        format!(" = {value} : {}", a.types.spell(*ty))
                    }
                    Some(ConstValue::Float { value, ty }) => {
                        format!(" = {value:?} : {}", a.types.spell(*ty))
                    }
                    Some(ConstValue::Str(b)) => {
                        format!(" = {:?} : char[]", String::from_utf8_lossy(b))
                    }
                    Some(ConstValue::Char { value, ty }) => {
                        format!(" = '{value}' : {}", a.types.spell(*ty))
                    }
                    None => String::new(),
                };
                let _ = writeln!(
                    o,
                    "  object {}{v} tokens[{}] {} @{}",
                    m.name,
                    tokens.join(" "),
                    status_str(&m.status),
                    loc(a, m.loc)
                );
            }
            MacroKind::Function {
                params,
                variadic,
                tokens,
            } => {
                let mut ps = params.clone();
                if *variadic {
                    ps.push("...".to_string());
                }
                let _ = writeln!(
                    o,
                    "  function {}({}) tokens[{}] {} @{}",
                    m.name,
                    ps.join(", "),
                    tokens.join(" "),
                    status_str(&m.status),
                    loc(a, m.loc)
                );
            }
        }
    }

    // ------------------------------------------------------- shims --
    if !a.shims.is_empty() {
        let _ = writeln!(o, "\nshims ({}):", a.shims.len());
        for s in &a.shims {
            let _ = writeln!(o, "  {} [{} bytes of C]", s.function, s.source.len());
        }
    }

    // --------------------------------------------------- refusals --
    // Repeated as a block on purpose: this is the part a human reads,
    // and it must not be something you assemble by grepping.
    let refusals = a.refusals();
    let (ok, refused) = a.tally();
    let _ = writeln!(o, "\nsummary: {ok} imported, {refused} refused");
    if !refusals.is_empty() {
        let _ = writeln!(o, "refused:");
        for (what, demotion, r) in refusals {
            let p = r.payload();
            let payload = if p.is_empty() {
                String::new()
            } else {
                format!("({p})")
            };
            let _ = writeln!(o, "  {what}: {}{payload} -> {demotion}", r.tag());
        }
    }
    o
}

fn status_str(s: &Status) -> String {
    match s {
        Status::Ok => "ok".to_string(),
        Status::Refused { demotion, refusal } => {
            let p = refusal.payload();
            if p.is_empty() {
                format!("refused({}) -> {demotion}", refusal.tag())
            } else {
                format!("refused({}:{p}) -> {demotion}", refusal.tag())
            }
        }
    }
}

fn loc(a: &Artifact, l: crate::artifact::SourceLoc) -> String {
    let file = a
        .files
        .get(l.file as usize)
        .map(String::as_str)
        .unwrap_or("<unknown>");
    if l.line == 0 {
        file.to_string()
    } else {
        format!("{file}:{}:{}", l.line, l.col)
    }
}

fn join(v: &[String]) -> String {
    if v.is_empty() {
        "(none)".to_string()
    } else {
        v.join(", ")
    }
}

fn describe_type(a: &Artifact, t: &CType) -> String {
    match t {
        CType::Void => "void".into(),
        CType::Bool => "_Bool".into(),
        CType::Int {
            bits,
            signed,
            spelling,
        } => format!(
            "int bits={bits} {} spelling={}",
            if *signed { "signed" } else { "unsigned" },
            spelling.tag()
        ),
        CType::Float { bits } => format!("float bits={bits}"),
        CType::Ptr { pointee, qual } => {
            let q = qual.spelling();
            if q.is_empty() {
                format!("ptr -> #{}", pointee.0)
            } else {
                format!("ptr -> #{} ({q})", pointee.0)
            }
        }
        CType::Array { elem, len } => match len {
            Some(n) => format!("array[{n}] of #{}", elem.0),
            None => format!("array[] of #{}", elem.0),
        },
        CType::Func {
            ret,
            params,
            variadic,
        } => {
            let ps: Vec<String> = params.iter().map(|p| format!("#{}", p.0)).collect();
            format!(
                "fn(({}){}) -> #{}",
                ps.join(", "),
                if *variadic { ", ..." } else { "" },
                ret.0
            )
        }
        CType::Record(r) => {
            let name = a
                .records
                .get(r.0 as usize)
                .map(|rec| {
                    let n = if rec.name.is_empty() {
                        "(anonymous)"
                    } else {
                        &rec.name
                    };
                    format!("{} {n}", rec.kind.tag())
                })
                .unwrap_or_else(|| "struct <missing>".into());
            format!("record#{} {name}", r.0)
        }
        CType::Enum(e) => format!("enum#{}", e.0),
        CType::Refused(tag) => format!("refused <{tag}>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{encode, testkit};

    #[test]
    fn dump_is_deterministic() {
        let a = testkit::sample_artifact();
        assert_eq!(dump(&a), dump(&a));
    }

    /// The dump is what the conformance suite compares, so it must
    /// survive the binary format exactly — otherwise a serialization
    /// bug hides behind a matching snapshot.
    #[test]
    fn dump_survives_the_binary_round_trip() {
        let a = testkit::sample_artifact();
        let back = encode::decode(&encode::encode(&a)).expect("decodes");
        assert_eq!(dump(&a), dump(&back));
    }

    /// Every refusal must be visible in the dump, by name. This is the
    /// property the whole sprint turns on.
    #[test]
    fn every_refusal_appears_by_name() {
        let a = testkit::sample_artifact();
        let text = dump(&a);
        assert!(
            !a.refusals().is_empty(),
            "the sample must exercise refusals"
        );
        for (what, _, r) in a.refusals() {
            assert!(text.contains(what), "dump hides the refused name {what}");
            assert!(
                text.contains(r.tag()),
                "dump hides the refusal tag {}",
                r.tag()
            );
        }
        assert!(text.contains("summary:"));
    }

    /// The sample has to keep exercising the shape that always
    /// demotes, or the refusal path stops being tested.
    #[test]
    fn the_sample_exercises_a_union() {
        let a = testkit::sample_artifact();
        assert!(
            a.records
                .iter()
                .any(|r| r.kind == crate::ctype::RecordKind::Union)
        );
    }
}
