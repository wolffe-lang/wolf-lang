//! `cargo xtask prelude-diff <tagA> <tagB> [--markdown]` (s212; ruling
//! #39 = B, wolf-lang#586): the prelude names a release adds and
//! removes, for the CHANGELOG's "Read this before you bump the pin".
//!
//! A name the prelude gains is a name a downstream module may already
//! declare, and that declaration now draws W0304, which
//! `--deny-warnings` makes fatal (bu16: boreutils' `bore.offset_of` at
//! 0.2.23, 0 of 27 utilities built). The release lane needs that list
//! from the tags, not from memory.
//!
//! The list at a revision is read from `spec/prelude.json` (schema
//! `wolf-prelude/0`, what that revision's `wolf prelude --json`
//! printed). A tag older than that file has no list, so the names are
//! read from the two ambient tables in that tag's
//! `crates/wolf_sema/src/prelude.rs` (`PRELUDE` and `BUILTIN_TYPES`, then
//! plain string arrays): names only, with `builtin_type` known for the
//! second table and nothing else claimed.

use std::path::Path;
use std::process::Command;

/// Where a revision's list came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// `spec/prelude.json` — every field known.
    Json,
    /// The tag's `prelude.rs` tables — names only.
    Tables,
}

impl Source {
    fn describe(self) -> &'static str {
        match self {
            Source::Json => "spec/prelude.json",
            Source::Tables => "prelude.rs tables",
        }
    }
}

/// One ambient name at a revision. `None` is "not recorded there".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: Option<String>,
    pub anchor: Option<String>,
    pub w0304: Option<bool>,
}

/// The ambient names at one revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List {
    pub source: Source,
    pub names: Vec<Entry>,
    /// The builtin marks, when the revision published them.
    pub marks: Option<Vec<String>>,
}

pub const JSON_PATH: &str = "spec/prelude.json";
pub const TABLES_PATH: &str = "crates/wolf_sema/src/prelude.rs";
pub const SCHEMA: &str = "wolf-prelude/0";

/// Parse `wolf prelude --json` / `spec/prelude.json`.
pub fn from_json(text: &str) -> Result<List, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("{JSON_PATH} does not parse: {e}"))?;
    if v["schema"] != SCHEMA {
        return Err(format!(
            "{JSON_PATH}: schema {} (this xtask reads {SCHEMA})",
            v["schema"]
        ));
    }
    let arr = |key: &str| {
        v[key]
            .as_array()
            .ok_or_else(|| format!("{JSON_PATH}: `{key}` is not an array"))
    };
    let mut names = Vec::new();
    for e in arr("names")? {
        let name = e["name"]
            .as_str()
            .ok_or_else(|| format!("{JSON_PATH}: a names entry has no name: {e}"))?;
        names.push(Entry {
            name: name.to_string(),
            kind: Some(
                e["kind"]
                    .as_str()
                    .ok_or_else(|| format!("{JSON_PATH}: `{name}` has no kind"))?
                    .to_string(),
            ),
            anchor: e["anchor"].as_str().map(str::to_string),
            w0304: Some(
                e["w0304"]
                    .as_bool()
                    .ok_or_else(|| format!("{JSON_PATH}: `{name}` has no w0304"))?,
            ),
        });
    }
    let mut marks = Vec::new();
    for m in arr("marks")? {
        let name = m["name"]
            .as_str()
            .ok_or_else(|| format!("{JSON_PATH}: a marks entry has no name: {m}"))?;
        marks.push(name.to_string());
    }
    Ok(List {
        source: Source::Json,
        names,
        marks: Some(marks),
    })
}

/// The string literals of `pub const <name>: &[&str] = &[ … ];`, comments
/// stripped. `Ok(None)` when the file has no such constant.
fn str_array(src: &str, name: &str) -> Result<Option<Vec<String>>, String> {
    let head = format!("pub const {name}: &[&str] = &[");
    let Some(start) = src.find(&head) else {
        return Ok(None);
    };
    let body = &src[start + head.len()..];
    let end = body
        .find("];")
        .ok_or_else(|| format!("{TABLES_PATH}: `{name}` never closes"))?;
    let mut out = Vec::new();
    for line in body[..end].lines() {
        let code = line.split("//").next().unwrap_or("");
        let mut rest = code;
        while let Some(open) = rest.find('"') {
            let after = &rest[open + 1..];
            let close = after
                .find('"')
                .ok_or_else(|| format!("{TABLES_PATH}: unterminated string in `{name}`"))?;
            out.push(after[..close].to_string());
            rest = &after[close + 1..];
        }
    }
    Ok(Some(out))
}

/// Read the two ambient tables of a `prelude.rs` that predates
/// `spec/prelude.json`.
pub fn from_tables(src: &str) -> Result<List, String> {
    let (Some(prelude), Some(types)) =
        (str_array(src, "PRELUDE")?, str_array(src, "BUILTIN_TYPES")?)
    else {
        return Err(format!(
            "{TABLES_PATH}: no `PRELUDE`/`BUILTIN_TYPES` string tables — a revision whose \
             tables carry kinds carries {JSON_PATH} too"
        ));
    };
    let entry = |name: String, kind: Option<&str>| Entry {
        name,
        kind: kind.map(str::to_string),
        anchor: None,
        w0304: None,
    };
    let mut names: Vec<Entry> = prelude.into_iter().map(|n| entry(n, None)).collect();
    names.extend(types.into_iter().map(|n| entry(n, Some("builtin_type"))));
    Ok(List {
        source: Source::Tables,
        names,
        marks: None,
    })
}

/// `git show <rev>:<path>` in `repo`; `Ok(None)` when the path does not
/// exist at that revision, `Err` when the revision itself does not.
fn git_show(repo: &Path, rev: &str, path: &str) -> Result<Option<String>, String> {
    let ok = Command::new("git")
        .current_dir(repo)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ])
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !ok.status.success() {
        return Err(format!(
            "no revision `{rev}` here (a shallow clone has no tags: `git fetch --tags`)"
        ));
    }
    let out = Command::new("git")
        .current_dir(repo)
        .args(["show", &format!("{rev}:{path}")])
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        String::from_utf8(out.stdout)
            .map(Some)
            .map_err(|e| format!("{rev}:{path} is not UTF-8: {e}"))
    } else {
        Ok(None)
    }
}

/// The list at `rev`: its `spec/prelude.json`, else its tables.
pub fn at_rev(repo: &Path, rev: &str) -> Result<List, String> {
    if let Some(text) = git_show(repo, rev, JSON_PATH)? {
        return from_json(&text).map_err(|e| format!("{rev}: {e}"));
    }
    match git_show(repo, rev, TABLES_PATH)? {
        Some(src) => from_tables(&src).map_err(|e| format!("{rev}: {e}")),
        None => Err(format!("{rev}: neither {JSON_PATH} nor {TABLES_PATH}")),
    }
}

/// What moved between two lists. Entries are `b`'s for an addition and
/// `a`'s for a removal, in their list's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diff {
    pub added: Vec<Entry>,
    pub removed: Vec<Entry>,
    /// Marks added and removed, when both revisions published marks.
    pub marks: Option<(Vec<String>, Vec<String>)>,
}

pub fn diff(a: &List, b: &List) -> Diff {
    let has = |l: &List, n: &str| l.names.iter().any(|e| e.name == n);
    let added = b
        .names
        .iter()
        .filter(|e| !has(a, &e.name))
        .cloned()
        .collect();
    let removed = a
        .names
        .iter()
        .filter(|e| !has(b, &e.name))
        .cloned()
        .collect();
    let marks = match (&a.marks, &b.marks) {
        (Some(ma), Some(mb)) => Some((
            mb.iter().filter(|m| !ma.contains(m)).cloned().collect(),
            ma.iter().filter(|m| !mb.contains(m)).cloned().collect(),
        )),
        _ => None,
    };
    Diff {
        added,
        removed,
        marks,
    }
}

fn field<T: std::fmt::Display>(v: &Option<T>) -> String {
    v.as_ref()
        .map_or_else(|| "?".to_string(), |x| x.to_string())
}

/// The plain report: one line per moved name.
pub fn render_text(ra: &str, a: &List, rb: &str, b: &List, d: &Diff) -> String {
    let mut out = format!(
        "prelude-diff {ra}..{rb} ({} at {ra}, {} at {rb}): {} added, {} removed\n",
        a.source.describe(),
        b.source.describe(),
        d.added.len(),
        d.removed.len()
    );
    for (sign, es) in [("+", &d.added), ("-", &d.removed)] {
        for e in es {
            out.push_str(&format!(
                "{sign} {:<20} kind={} anchor={} w0304={}\n",
                e.name,
                field(&e.kind),
                e.anchor.as_deref().map_or_else(
                    || if e.kind.is_some() && e.w0304.is_some() {
                        "none"
                    } else {
                        "?"
                    }
                    .to_string(),
                    |x| format!("[{x}]")
                ),
                field(&e.w0304)
            ));
        }
    }
    match &d.marks {
        Some((add, rem)) => {
            for m in add {
                out.push_str(&format!("+ mark {m}\n"));
            }
            for m in rem {
                out.push_str(&format!("- mark {m}\n"));
            }
        }
        None => out.push_str("marks: not compared (one side predates spec/prelude.json)\n"),
    }
    out
}

fn code_list(es: &[&Entry]) -> String {
    let words: Vec<String> = es.iter().map(|e| format!("`{}`", e.name)).collect();
    match words.len() {
        0 => String::new(),
        1 => words[0].clone(),
        2 => format!("{} and {}", words[0], words[1]),
        n => format!("{}, and {}", words[..n - 1].join(", "), words[n - 1]),
    }
}

/// The CHANGELOG paragraph for "Read this before you bump the pin".
pub fn render_markdown(ra: &str, rb: &str, d: &Diff) -> String {
    let mut out = String::new();
    if d.added.is_empty() {
        out.push_str(&format!(
            "**New prelude names:** none (`cargo xtask prelude-diff {ra} {rb}`).\n"
        ));
    } else {
        let hazard: Vec<&Entry> = d.added.iter().filter(|e| e.w0304 != Some(false)).collect();
        let quiet: Vec<&Entry> = d.added.iter().filter(|e| e.w0304 == Some(false)).collect();
        let mut kinds: Vec<String> = Vec::new();
        for e in &d.added {
            let k = match (&e.kind, &e.anchor) {
                (Some(k), Some(a)) => format!("`{}`: {k}, `[{a}]`", e.name),
                (Some(k), None) => format!("`{}`: {k}", e.name),
                (None, _) => continue,
            };
            kinds.push(k);
        }
        out.push_str(&format!(
            "**New prelude names.** {rb} adds {}",
            code_list(&d.added.iter().collect::<Vec<_>>())
        ));
        if kinds.is_empty() {
            out.push('.');
        } else {
            out.push_str(&format!(" ({}).", kinds.join("; ")));
        }
        if !hazard.is_empty() {
            let unknown = hazard.iter().any(|e| e.w0304.is_none());
            out.push_str(&format!(
                " A module that declares a function, type or binding named {} now draws \
                 W0304 (it shadows the ambient name), which `--deny-warnings` makes fatal: \
                 rename yours.",
                code_list(&hazard)
            ));
            if unknown {
                out.push_str(&format!(
                    " ({rb} predates `spec/prelude.json`, which records W0304 per name, so \
                     this sentence assumes it.)"
                ));
            }
        }
        if !quiet.is_empty() {
            out.push_str(&format!(" {} draws no W0304.", code_list(&quiet)));
        }
        out.push_str(&format!(" (`cargo xtask prelude-diff {ra} {rb}`)\n"));
    }
    if !d.removed.is_empty() {
        out.push_str(&format!(
            "\n**Prelude names removed.** {}: a program that used one without an import \
             no longer resolves it (E0301).\n",
            code_list(&d.removed.iter().collect::<Vec<_>>())
        ));
    }
    if let Some((add, _)) = &d.marks
        && !add.is_empty()
    {
        let ws: Vec<String> = add.iter().map(|m| format!("`{m}`")).collect();
        out.push_str(&format!(
            "\n**New builtin marks:** {} (`[os.host.sigs]`).\n",
            ws.join(", ")
        ));
    }
    out
}
