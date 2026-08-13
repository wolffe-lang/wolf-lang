//! The stable JSON index — `wolf-doc/0`.
//!
//! The hook for search, cross-package linking, and the registry's docs
//! hosting (X7: the format is pinned now, the service later). Hand-
//! written rather than serialized, for the same reason the interface
//! encoder is: the byte order of a published format is a decision, not
//! a library's default. Keys are emitted in a fixed order, numbers are
//! integers, and nothing in the output depends on the host, the clock,
//! or map iteration order.
//!
//! The schema, one line:
//!
//! ```text
//! {schema, package, title, private, deps:[{alias,name,version}],
//!  coverage:{total,documented,with_doctest},
//!  modules:[{path,page,doc,summary,items:[{name,kind,vis,sig,page,anchor,
//!    doc,summary,links:[],doctests:[{index,lang,directives:[],code}]}]}]}
//! ```

use wolf_query::{Directive, DocComment, DocPackage};

use crate::html::module_page_name;
use crate::{Options, coverage, vis_word};

/// JSON string escaping (RFC 8259): quotes, backslash, the C0 controls.
fn q(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn arr(items: Vec<String>) -> String {
    format!("[{}]", items.join(","))
}

fn links(doc: Option<&DocComment>) -> String {
    arr(doc
        .map(|d| d.links.iter().map(|l| q(l)).collect())
        .unwrap_or_default())
}

fn doctests(doc: Option<&DocComment>) -> String {
    let Some(d) = doc else {
        return "[]".to_string();
    };
    let mut out = Vec::new();
    for (i, f) in d.fences.iter().filter(|f| f.is_example()).enumerate() {
        let directives: Vec<String> = f
            .directives
            .iter()
            .map(|dir| match dir {
                Directive::NoRun => q("no_run"),
                Directive::Ignore => q("ignore"),
                Directive::ShouldFail(codes) if codes.is_empty() => q("should_fail"),
                Directive::ShouldFail(codes) => q(&format!("should_fail({})", codes.join(", "))),
            })
            .collect();
        out.push(format!(
            "{{\"index\":{i},\"lang\":{},\"directives\":{},\"code\":{}}}",
            q(&f.lang),
            arr(directives),
            q(&f.code)
        ));
    }
    arr(out)
}

fn anchor(name: &str) -> String {
    let mut out = String::from("item.");
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    out
}

/// Render the index. Newline-terminated, two-space indentation at the
/// module level only — small enough to read, stable enough to diff.
pub fn index(docs: &DocPackage, opts: &Options, title: &str) -> String {
    let cov = coverage(docs);
    let deps: Vec<String> = opts
        .deps
        .iter()
        .map(|(alias, name, version)| {
            format!(
                "{{\"alias\":{},\"name\":{},\"version\":{}}}",
                q(alias),
                q(name),
                q(version)
            )
        })
        .collect();
    let mut modules = Vec::new();
    for m in &docs.modules {
        let mut items = Vec::new();
        for it in &m.items {
            items.push(format!(
                "\n      {{\"name\":{},\"kind\":{},\"vis\":{},\"sig\":{},\"page\":{},\
                 \"anchor\":{},\"doc\":{},\"summary\":{},\"links\":{},\"doctests\":{}}}",
                q(&it.name),
                q(it.kind.keyword()),
                q(vis_word(it)),
                q(&it.sig),
                q(&module_page_name(&m.path)),
                q(&anchor(&it.name)),
                q(it.doc.as_ref().map(|d| d.text.as_str()).unwrap_or("")),
                q(it.doc.as_ref().map(|d| d.summary.as_str()).unwrap_or("")),
                links(it.doc.as_ref()),
                doctests(it.doc.as_ref())
            ));
        }
        modules.push(format!(
            "\n    {{\"path\":{},\"page\":{},\"doc\":{},\"summary\":{},\"links\":{},\
             \"doctests\":{},\"items\":{}}}",
            q(&m.path),
            q(&module_page_name(&m.path)),
            q(m.doc.as_ref().map(|d| d.text.as_str()).unwrap_or("")),
            q(m.doc.as_ref().map(|d| d.summary.as_str()).unwrap_or("")),
            links(m.doc.as_ref()),
            doctests(m.doc.as_ref()),
            arr(items)
        ));
    }
    format!(
        "{{\"schema\":\"wolf-doc/0\",\"package\":{},\"title\":{},\"private\":{},\
         \"deps\":{},\"coverage\":{{\"total\":{},\"documented\":{},\"with_doctest\":{}}},\
         \"modules\":{}}}\n",
        q(&docs.package),
        q(title),
        docs.private,
        arr(deps),
        cov.total,
        cov.documented,
        cov.with_doctest,
        arr(modules)
    )
}
