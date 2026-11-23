//! wolf-lang#1179: the memory tier's list of host builtins whose `str`
//! answer is an allocation in the current region
//! (`wolf_mem::region_sites::HOST_STR_SITES`) is held against the runtime's
//! source, both ways, failing by name.
//!
//! The runtime copies a `str` into the current region through one
//! primitive, `wolf_rt::str::region_copy`. This test reads every `.rs` file
//! under `crates/wolf_rt/src` as text, finds each function whose body
//! mentions `region_copy` or a function that does (followed to a fixed
//! point, by name), and resolves the runtime's exported entry points
//! (`extern "C" fn __wolf_rt_*`) among them:
//!
//! - an entry point `__wolf_rt_<name>` whose `<name>` the prelude declares
//!   is a builtin that answers bytes in the current region, and must be in
//!   `HOST_STR_SITES`;
//! - any other entry point must be in [`COVERED_ELSEWHERE`] below, which
//!   says what makes its result a site (a method of `STR_METHOD_SITES`, or
//!   a language form the memory tier counts itself);
//! - every name in `HOST_STR_SITES` must be such an entry point, or one of
//!   [`EXCEPTIONS`], with its reason.
//!
//! A second rule pins the copying primitive itself: the functions whose
//! body copies bytes into an allocation made in the current region (they
//! mention `ambient_region`, `alloc_in` and `copy_nonoverlapping`) are
//! exactly `region_copy`, so a second primitive beside it fails here by
//! name instead of escaping the first rule.
//!
//! **What would make it miss a site.** It reads source text, so it sees
//! only what the text says: comments, string and character literals and
//! `#[cfg(test)]` items are blanked first; functions and `macro_rules!`
//! bodies are followed by name, so a helper, a method, a function pointer
//! or a macro that reaches `region_copy` is found (the rows at the end plant
//! each one). It misses a copy into the current region that never names
//! `region_copy` or a function that does and does not have the primitive's
//! shape (a cursor bumped by hand into a region chunk, say), code outside
//! `crates/wolf_rt/src` (a generated or `include!`d file), and an entry
//! point not declared `extern "C"` with the `__wolf_rt_` prefix. Matching by
//! name over-approximates (two functions of one name are one here), which
//! can only add entry points, never lose one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use wolf_mem::region_sites::{HOST_STR_SITES, STR_METHOD_SITES};

/// The one primitive that copies a `str` into the current region.
const PRIMITIVE: &str = "region_copy";

/// Runtime entry points that reach the primitive but are no host builtin,
/// and what makes their result a site in the memory tier: the methods of
/// `STR_METHOD_SITES` they serve, or (with no method) the language form
/// the tier counts on its own.
const COVERED_ELSEWHERE: &[(&str, &[&str], &str)] = &[
    (
        "__wolf_rt_str_case",
        &["upper", "lower"],
        "the case methods",
    ),
    (
        "__wolf_rt_str_repeat",
        &["repeat"],
        "`repeat`, and `copy region`'s copy-out, which aims at the enclosing region on purpose",
    ),
    ("__wolf_rt_str_replace", &["replace"], "`replace`"),
    (
        "__wolf_rt_strbuf_finish",
        &[],
        "interpolation and `+`, sites by their own rules in the memory tier",
    ),
];

/// Names in `HOST_STR_SITES` whose runtime does not copy into the current
/// region, and why each stays listed.
const EXCEPTIONS: &[(&str, &str)] = &[(
    "os_error_text",
    "its runtime copies into the process root, so the site only refuses a program that \
     would be safe; moving its bytes to the region is a behaviour change of its own",
)];

/// A function or macro found in the runtime's source.
#[derive(Debug)]
struct Item {
    name: String,
    file: String,
    exported: bool,
    mentions: BTreeSet<String>,
}

/// `src` with comments, string literals and character literals replaced by
/// spaces (newlines kept), so braces and names inside them are not read.
fn blank(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    let wipe = |out: &mut Vec<u8>, from: usize, to: usize| {
        for c in out.iter_mut().take(to).skip(from) {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    };
    while i < b.len() {
        if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            let end = b[i..]
                .iter()
                .position(|&c| c == b'\n')
                .map_or(b.len(), |n| i + n);
            wipe(&mut out, i, end);
            i = end;
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            let mut depth = 0usize;
            let start = i;
            while i < b.len() {
                if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            wipe(&mut out, start, i);
        } else if b[i] == b'r'
            && (b.get(i + 1) == Some(&b'"') || b.get(i + 1) == Some(&b'#'))
            && (i == 0 || !is_ident(b[i - 1]))
        {
            // A raw string: r"…" or r#"…"#, any number of hashes.
            let mut j = i + 1;
            let mut hashes = 0;
            while b.get(j) == Some(&b'#') {
                hashes += 1;
                j += 1;
            }
            if b.get(j) != Some(&b'"') {
                i += 1;
                continue;
            }
            let start = i;
            j += 1;
            loop {
                if j >= b.len() {
                    break;
                }
                if b[j] == b'"' && (0..hashes).all(|k| b.get(j + 1 + k) == Some(&b'#')) {
                    j += 1 + hashes;
                    break;
                }
                j += 1;
            }
            wipe(&mut out, start, j);
            i = j;
        } else if b[i] == b'"' {
            let start = i;
            i += 1;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            wipe(&mut out, start, i.min(b.len()));
        } else if b[i] == b'\'' {
            // A character literal ('x', '\n', '\u{..}', '{'), never a
            // lifetime ('a, 'static), which has no closing quote soon after.
            let close = if b.get(i + 1) == Some(&b'\\') {
                // The escaped character itself may be a quote ('\'').
                b.get(i + 3..)
                    .and_then(|rest| rest.iter().take(12).position(|&c| c == b'\''))
                    .map(|n| i + 3 + n)
            } else {
                let ch = src[i + 1..].chars().next().map_or(1, char::len_utf8);
                (b.get(i + 1 + ch) == Some(&b'\'')).then_some(i + 1 + ch)
            };
            match close {
                Some(end) => {
                    wipe(&mut out, i, end + 1);
                    i = end + 1;
                }
                None => i += 1,
            }
        } else {
            i += 1;
        }
    }
    String::from_utf8(out).expect("blanking keeps UTF-8: only ASCII bytes are replaced")
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// The index just past the brace that closes the one at `open`.
fn close_of(b: &[u8], open: usize) -> usize {
    let mut depth = 0usize;
    for (k, &c) in b.iter().enumerate().skip(open) {
        if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 {
                return k + 1;
            }
        }
    }
    b.len()
}

/// The identifier that starts at `at`, if one does.
fn ident_at(b: &[u8], at: usize) -> Option<(usize, usize)> {
    if at >= b.len() || !(b[at].is_ascii_alphabetic() || b[at] == b'_') {
        return None;
    }
    if at > 0 && is_ident(b[at - 1]) {
        return None;
    }
    let end = b[at..]
        .iter()
        .position(|&c| !is_ident(c))
        .map_or(b.len(), |n| at + n);
    Some((at, end))
}

/// `src` with every `#[cfg(test)]` item blanked (a test module or a test
/// function: the attribute through the brace that closes the item).
fn without_tests(src: &str) -> String {
    let mut b = src.as_bytes().to_vec();
    let marker = b"#[cfg(test)]";
    let mut from = 0;
    while let Some(n) = b[from..].windows(marker.len()).position(|w| w == marker) {
        let at = from + n;
        let Some(open) = b[at..].iter().position(|&c| c == b'{' || c == b';') else {
            break;
        };
        let open = at + open;
        let end = if b[open] == b'{' {
            close_of(&b, open)
        } else {
            open + 1
        };
        for c in b.iter_mut().take(end).skip(at) {
            if *c != b'\n' {
                *c = b' ';
            }
        }
        from = end;
    }
    String::from_utf8(b).expect("ASCII blanking keeps UTF-8")
}

/// Every function and `macro_rules!` macro in one blanked file.
fn items_of(file: &str, text: &str) -> Vec<Item> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let Some((s, e)) = ident_at(b, i) else {
            i += 1;
            continue;
        };
        let word = &text[s..e];
        let decl = match word {
            "fn" => {
                let mut j = e;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                ident_at(b, j)
            }
            "macro_rules" if b.get(e) == Some(&b'!') => {
                let mut j = e + 1;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                ident_at(b, j)
            }
            _ => None,
        };
        let Some((ns, ne)) = decl else {
            i = e;
            continue;
        };
        let Some(open) = b[ne..].iter().position(|&c| c == b'{' || c == b';') else {
            break;
        };
        let open = ne + open;
        if b[open] == b';' {
            // A declaration with no body (a trait method, an extern block).
            i = open + 1;
            continue;
        }
        let end = close_of(b, open);
        let line_start = text[..s].rfind('\n').map_or(0, |n| n + 1);
        let signature = &text[line_start..open];
        let name = text[ns..ne].to_string();
        let exported = signature.contains("extern") && name.starts_with("__wolf_rt_");
        let mut mentions = BTreeSet::new();
        let mut k = open;
        while k < end {
            if let Some((ms, me)) = ident_at(b, k) {
                mentions.insert(text[ms..me].to_string());
                k = me;
            } else {
                k += 1;
            }
        }
        out.push(Item {
            name,
            file: file.to_string(),
            exported,
            mentions,
        });
        // Nested items are read again on their own; continue inside.
        i = open + 1;
    }
    out
}

/// Every item of every `.rs` file under `dir`, test items left out.
fn runtime_items(dir: &Path) -> Vec<Item> {
    fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .map(|e| e.expect("dir entry").path())
            .collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(&p, files);
            } else if p.extension().is_some_and(|x| x == "rs") {
                files.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, &mut files);
    assert!(
        !files.is_empty(),
        "no runtime source under {}",
        dir.display()
    );
    let mut items = Vec::new();
    for f in files {
        let src = std::fs::read_to_string(&f).expect("runtime source reads");
        let rel = f.strip_prefix(dir).unwrap_or(&f).display().to_string();
        items.extend(items_of(&rel, &without_tests(&blank(&src))));
    }
    items
}

/// The names of the items that reach `primitive`: those that mention it or
/// an item that does, to a fixed point. The primitive itself is left out.
fn reaching(items: &[Item], primitive: &str) -> BTreeSet<String> {
    let mut reach: BTreeSet<String> = BTreeSet::new();
    loop {
        let before = reach.len();
        for it in items {
            if it.name != primitive
                && (it.mentions.contains(primitive)
                    || it
                        .mentions
                        .iter()
                        .any(|m| m != &it.name && reach.contains(m)))
            {
                reach.insert(it.name.clone());
            }
        }
        if reach.len() == before {
            return reach;
        }
    }
}

/// The exported entry points among `reach`, with the file each is in.
fn entry_points(items: &[Item], reach: &BTreeSet<String>) -> BTreeMap<String, String> {
    items
        .iter()
        .filter(|it| it.exported && reach.contains(&it.name))
        .map(|it| (it.name.clone(), it.file.clone()))
        .collect()
}

fn runtime_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../wolf_rt/src")
}

fn prelude_functions() -> BTreeSet<&'static str> {
    wolf_sema::prelude::PRELUDE.iter().map(|a| a.name).collect()
}

/// Every entry point that answers a `str` in the current region is a site,
/// by name, and every listed name is such an entry point or an exception.
#[test]
fn every_region_str_entry_point_is_a_listed_site() {
    let items = runtime_items(&runtime_dir());
    let reach = reaching(&items, PRIMITIVE);
    let entries = entry_points(&items, &reach);
    let prelude = prelude_functions();
    let mut wrong = Vec::new();

    let mut served = BTreeSet::new();
    for (sym, file) in &entries {
        let name = sym.trim_start_matches("__wolf_rt_");
        if prelude.contains(name) {
            served.insert(name.to_string());
            if !HOST_STR_SITES.contains(&name) {
                wrong.push(format!(
                    "`{sym}` ({file}) places its str in the current region, and `{name}` is not in \
                     wolf_mem::region_sites::HOST_STR_SITES: a call returned out of its region \
                     would read freed bytes with no diagnostic"
                ));
            }
        } else if let Some((_, methods, _)) = COVERED_ELSEWHERE
            .iter()
            .find(|(s, _, _)| *s == sym.as_str())
        {
            for m in *methods {
                if !STR_METHOD_SITES.contains(m) {
                    wrong.push(format!(
                        "`{sym}` ({file}) serves `{m}`, which is not in \
                         wolf_mem::region_sites::STR_METHOD_SITES"
                    ));
                }
            }
        } else {
            wrong.push(format!(
                "`{sym}` ({file}) reaches `{PRIMITIVE}`, and it is neither a prelude builtin nor in \
                 this test's COVERED_ELSEWHERE: say which site makes its result one"
            ));
        }
    }
    for name in HOST_STR_SITES {
        let excepted = EXCEPTIONS.iter().any(|(n, _)| n == name);
        if served.contains(*name) && excepted {
            wrong.push(format!(
                "`{name}` is an exception here, but its runtime now copies into the current region: \
                 drop the exception"
            ));
        }
        if !served.contains(*name) && !excepted {
            wrong.push(format!(
                "`{name}` is in HOST_STR_SITES, but no `__wolf_rt_{name}` reaches `{PRIMITIVE}`: \
                 remove it, or name it an exception with its reason"
            ));
        }
    }
    for (sym, _, _) in COVERED_ELSEWHERE {
        if !entries.contains_key(*sym) {
            wrong.push(format!(
                "`{sym}` is in COVERED_ELSEWHERE, but it no longer reaches `{PRIMITIVE}`"
            ));
        }
    }
    for m in STR_METHOD_SITES {
        if !COVERED_ELSEWHERE.iter().any(|(_, ms, _)| ms.contains(m)) {
            wrong.push(format!(
                "the method `{m}` is in STR_METHOD_SITES, but no entry point here serves it"
            ));
        }
    }
    eprintln!(
        "REPORT {} entry points reach `{PRIMITIVE}`: {}",
        entries.len(),
        entries.keys().cloned().collect::<Vec<_>>().join(", ")
    );
    assert!(
        wrong.is_empty(),
        "the region-site list and the runtime disagree (wolf-lang#1179):\n- {}",
        wrong.join("\n- ")
    );
}

/// The primitive's shape has one owner: a second function that copies
/// bytes into an allocation in the current region fails here by name.
#[test]
fn region_copy_is_the_only_copy_into_the_current_region() {
    let items = runtime_items(&runtime_dir());
    let copiers: Vec<String> = items
        .iter()
        .filter(|it| {
            ["ambient_region", "alloc_in", "copy_nonoverlapping"]
                .iter()
                .all(|m| it.mentions.contains(*m))
        })
        .map(|it| format!("`{}` ({})", it.name, it.file))
        .collect();
    assert_eq!(
        copiers,
        vec![format!("`{PRIMITIVE}` (str.rs)")],
        "a function other than `{PRIMITIVE}` copies bytes into the current region: route it \
         through `{PRIMITIVE}`, or teach this test its name (wolf-lang#1179)"
    );
}

/// The scanner, on a planted source: a two-level helper, a method, a
/// function pointer and a macro, each between an entry point and the
/// primitive, are all found; the decoys (a comment, a string, a test
/// module) are not.
#[test]
fn the_scanner_follows_planted_helpers() {
    let planted = r##"
pub(crate) fn region_copy(bytes: &[u8]) -> *const u8 { core::ptr::null() }

/// `__wolf_rt_by_helper` calls `stash`, which calls `deeper`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_by_helper(out: i64) -> i64 { stash(out, "a") }
fn stash(out: i64, s: &str) -> i64 { deeper(out, s) }
fn deeper(out: i64, s: &str) -> i64 { let p = crate::str::region_copy(s.as_bytes()); 0 }

struct Namer;
impl Namer {
    fn place(&self, s: &str) -> *const u8 { region_copy(s.as_bytes()) }
}
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_by_method(out: i64) -> i64 { Namer.place("b"); 0 }

#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_by_pointer(out: i64) -> i64 { let f: fn(&[u8]) -> *const u8 = region_copy; 0 }

macro_rules! copy_it { ($s:expr) => { region_copy($s) } }
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_by_macro(out: i64) -> i64 { copy_it!(b"c"); 0 }

#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_decoy(out: i64) -> i64 {
    // region_copy is named in this comment only
    let s = "region_copy in a string { not a brace";
    let c = '{';
    0
}

#[cfg(test)]
mod tests {
    #[unsafe(no_mangle)]
    pub extern "C" fn __wolf_rt_in_tests(out: i64) -> i64 { super::region_copy(b""); 0 }
}
"##;
    let items = items_of("planted.rs", &without_tests(&blank(planted)));
    let reach = reaching(&items, PRIMITIVE);
    let found: Vec<String> = entry_points(&items, &reach).into_keys().collect();
    assert_eq!(
        found,
        vec![
            "__wolf_rt_by_helper".to_string(),
            "__wolf_rt_by_macro".to_string(),
            "__wolf_rt_by_method".to_string(),
            "__wolf_rt_by_pointer".to_string(),
        ],
        "the scanner on the planted source"
    );
}
