//! `wolf prelude [--json]` (s212; ruling #39 = B, wolf-lang#586,
//! wolf-lsp#32): every name a program can use without an import, with
//! its kind and the spec anchor that defines it, plus the builtin marks
//! (the row tags the host builtin table declares).
//!
//! The rows are `wolf_sema::prelude::names()` and `marks()`, which read
//! the tables resolution reads, so this list cannot drift from what the
//! checker resolves. `--json` prints schema `wolf-prelude/0`
//! (docs/prelude-json.md), one entry per line so a diff of two pins is
//! a diff of lines; `spec/prelude.json` is that output, committed, and
//! `tests/prelude_list.rs` reds when the two part.

use wolf_sema::prelude::{self, Kind};

/// The schema name `--json` prints. A change to a field's meaning, or a
/// removed field, bumps the number; a new field does not.
pub const SCHEMA: &str = "wolf-prelude/0";

fn q(s: &str) -> String {
    serde_json::to_string(s).expect("a str serializes")
}

fn anchor(a: Option<&str>) -> String {
    a.map_or_else(|| "null".to_string(), q)
}

/// The `--json` text, newline-terminated.
pub fn json() -> String {
    let names = prelude::names();
    let marks = prelude::marks();
    let mut out = format!("{{\n  \"schema\": {},\n  \"names\": [\n", q(SCHEMA));
    for (i, n) in names.iter().enumerate() {
        let comma = if i + 1 < names.len() { "," } else { "" };
        out.push_str(&format!(
            "    {{\"name\": {}, \"kind\": {}, \"anchor\": {}, \"w0304\": {}}}{comma}\n",
            q(n.name),
            q(n.kind.as_str()),
            anchor(n.anchor),
            n.w0304
        ));
    }
    out.push_str("  ],\n  \"marks\": [\n");
    for (i, m) in marks.iter().enumerate() {
        let comma = if i + 1 < marks.len() { "," } else { "" };
        let by: Vec<String> = m.by.iter().map(|b| q(b)).collect();
        out.push_str(&format!(
            "    {{\"name\": {}, \"anchor\": {}, \"by\": [{}]}}{comma}\n",
            q(&m.name),
            q(prelude::HOST_SIGS_ANCHOR),
            by.join(", ")
        ));
    }
    out.push_str("  ]\n}\n");
    out
}

/// The human table.
pub fn human() -> String {
    let names = prelude::names();
    let marks = prelude::marks();
    let count = |k: Kind| names.iter().filter(|n| n.kind == k).count();
    let mut out = format!(
        "The prelude: {} names a program uses without an import ({} builtin_type, {} type, \
         {} function, {} intrinsic, {} provisional) and {} builtin marks.\n\
         W0304 = a module that declares the name draws W0304 (fatal under --deny-warnings).\n\
         `--json` prints the same rows as {SCHEMA}.\n\n",
        names.len(),
        count(Kind::BuiltinType),
        count(Kind::Type),
        count(Kind::Function),
        count(Kind::Intrinsic),
        count(Kind::Provisional),
        marks.len(),
    );
    out.push_str(&format!(
        "{:<20} {:<13} {:<20} W0304\n",
        "NAME", "KIND", "ANCHOR"
    ));
    for n in &names {
        let a = n
            .anchor
            .map_or_else(|| "-".to_string(), |a| format!("[{a}]"));
        let w = if n.w0304 { "yes" } else { "no" };
        out.push_str(&format!(
            "{:<20} {:<13} {:<20} {w}\n",
            n.name,
            n.kind.as_str(),
            a
        ));
    }
    out.push_str(&format!(
        "\n{:<20} {:<13} {:<20} CARRIED BY\n",
        "MARK", "", "ANCHOR"
    ));
    for m in &marks {
        out.push_str(&format!(
            "{:<20} {:<13} {:<20} {}\n",
            m.name,
            "",
            format!("[{}]", prelude::HOST_SIGS_ANCHOR),
            m.by.join(" ")
        ));
    }
    out
}

/// `wolf prelude [--json]`: prints and returns; a stray argument is a
/// usage error.
pub fn prelude(args: &[String]) -> String {
    match args {
        [] => human(),
        [a] if a == "--json" => json(),
        _ => crate::help::usage_exit("prelude"),
    }
}
