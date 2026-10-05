//! `cargo xtask prelude-diff` (s212; ruling #39 = B, wolf-lang#586,
//! wolf-lsp#32).
//!
//! The two historical diffs the release lane missed are tested on the
//! tags' own `prelude.rs` bytes. CI checks out shallow, with no tags, so
//! the bytes are committed under `fixtures/prelude/` and each file's git
//! blob id is pinned here to `git rev-parse <tag>:crates/wolf_sema/src/
//! prelude.rs` — a fixture that is not the tag's file reds before any
//! diff is read. The git plumbing (`at_rev`, both sources, the binary
//! itself) is tested on a throwaway repository built here.

use std::path::{Path, PathBuf};
use std::process::Command;

use xtask::prelude::{self, Source};

fn fixture(tag: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/prelude")
        .join(format!("{tag}.rs.txt"))
}

/// `git rev-parse <tag>:crates/wolf_sema/src/prelude.rs`, read off the
/// tags 2026-10-05.
const BLOBS: &[(&str, &str)] = &[
    ("v0.2.15", "ed217ab3f59e3864d904f613c80e9be6453fe788"),
    ("v0.2.16", "ae8e0be9a94a1e78ec91a2a9f4f9f6bd7a245b6d"),
    ("v0.2.22", "62b0ffb7812cb3fed8fa0e3dc9cc2927e309cc4c"),
    ("v0.2.23", "970db00a418e2a12afb5239f5a41fbf138a36d56"),
];

fn tables_at(tag: &str) -> prelude::List {
    let path = fixture(tag);
    let out = Command::new("git")
        .args(["hash-object", "--"])
        .arg(&path)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git hash-object {}", path.display());
    let blob = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let want = BLOBS
        .iter()
        .find(|(t, _)| *t == tag)
        .expect("a pinned tag")
        .1;
    assert_eq!(blob, want, "{} is not {tag}'s prelude.rs", path.display());
    let src = std::fs::read_to_string(&path).expect("fixture reads");
    let list = prelude::from_tables(&src).expect("the tag's tables parse");
    assert_eq!(list.source, Source::Tables);
    list
}

fn names(es: &[prelude::Entry]) -> Vec<&str> {
    es.iter().map(|e| e.name.as_str()).collect()
}

fn has(l: &prelude::List, n: &str) -> bool {
    l.names.iter().any(|e| e.name == n)
}

/// 0.2.23 (kw08) added `align_of` and `offset_of`; boreutils'
/// `bore.offset_of` drew W0304 on it (bu16). `size_of` is NOT in the
/// diff: it has been ambient since s16, and the 0.2.22 binary already
/// warns on a module's `fn size_of` (s212 §2,
/// `~/lanes/s212/evidence/s2-w0304-probe.log` on kasumi, d3de86c1…).
#[test]
fn v0_2_22_to_v0_2_23_adds_align_of_and_offset_of() {
    let a = tables_at("v0.2.22");
    let b = tables_at("v0.2.23");
    let d = prelude::diff(&a, &b);
    assert_eq!(names(&d.added), ["align_of", "offset_of"]);
    assert!(d.removed.is_empty(), "{:?}", names(&d.removed));
    assert!(has(&a, "size_of") && has(&b, "size_of"), "size_of at both");
    assert_eq!((a.names.len(), b.names.len()), (121, 123));
    let md = prelude::render_markdown("v0.2.22", "v0.2.23", &d);
    assert!(md.contains("`align_of` and `offset_of`"), "{md}");
    assert!(md.contains("W0304"), "{md}");
    assert!(!md.contains("size_of"), "{md}");
}

/// 0.2.16 (s170) added the concurrency handle types `Scope` and `Proc`,
/// which wolf-lsp's type-name gate could not see (wolf-lsp#32).
#[test]
fn v0_2_15_to_v0_2_16_adds_scope_and_proc() {
    let a = tables_at("v0.2.15");
    let b = tables_at("v0.2.16");
    let d = prelude::diff(&a, &b);
    assert_eq!(names(&d.added), ["Scope", "Proc"]);
    assert!(d.removed.is_empty(), "{:?}", names(&d.removed));
    assert_eq!((a.names.len(), b.names.len()), (116, 118));
    let text = prelude::render_text("v0.2.15", &a, "v0.2.16", &b, &d);
    assert!(text.contains("2 added, 0 removed"), "{text}");
    assert!(
        text.contains("+ Scope") && text.contains("+ Proc"),
        "{text}"
    );
}

/// The tables reader takes the second table as `builtin_type` and
/// claims nothing else; a file whose tables carry kinds is refused (it
/// carries `spec/prelude.json`, which is what is read there).
#[test]
fn the_tables_reader_claims_only_what_the_tables_say() {
    let l = tables_at("v0.2.23");
    let int = l.names.iter().find(|e| e.name == "int").expect("int");
    assert_eq!(int.kind.as_deref(), Some("builtin_type"));
    let print = l.names.iter().find(|e| e.name == "print").expect("print");
    assert_eq!(
        (print.kind.as_ref(), print.anchor.as_ref(), print.w0304),
        (None, None, None)
    );
    let head = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/wolf_sema/src/prelude.rs"),
    )
    .expect("head's prelude.rs");
    assert!(
        prelude::from_tables(&head).is_err(),
        "head's rows carry kinds"
    );
}

/// The committed `spec/prelude.json` reads, every row fully recorded,
/// and it diffs empty against itself.
#[test]
fn the_head_json_reads() {
    let text =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../spec/prelude.json"))
            .expect("spec/prelude.json");
    let l = prelude::from_json(&text).expect("parses");
    assert_eq!(l.source, Source::Json);
    assert!(!l.names.is_empty());
    for e in &l.names {
        assert!(e.kind.is_some() && e.w0304.is_some(), "{}", e.name);
    }
    assert!(l.marks.as_ref().is_some_and(|m| !m.is_empty()));
    let d = prelude::diff(&l, &l);
    assert!(d.added.is_empty() && d.removed.is_empty());
    assert_eq!(d.marks, Some((vec![], vec![])));
    assert!(prelude::from_json(&text.replace("wolf-prelude/0", "wolf-prelude/9")).is_err());
}

fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["-c", "user.name=s212", "-c", "user.email=s212@invalid"])
        .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn commit_tag(repo: &Path, path: &str, text: &str, tag: &str) {
    let p = repo.join(path);
    std::fs::create_dir_all(p.parent().expect("a parent")).expect("mkdir");
    std::fs::write(&p, text).expect("write");
    git(repo, &["add", path]);
    git(repo, &["commit", "-q", "-m", tag]);
    git(repo, &["tag", tag]);
}

/// A json revision written by hand: the 0.2.23 names plus one.
fn synthetic_json(extra: &str) -> String {
    let l = tables_at("v0.2.23");
    let mut rows: Vec<String> = l
        .names
        .iter()
        .map(|e| {
            format!(
                "    {{\"name\": \"{}\", \"kind\": \"{}\", \"anchor\": null, \"w0304\": true}}",
                e.name,
                e.kind.as_deref().unwrap_or("function")
            )
        })
        .collect();
    rows.push(format!(
        "    {{\"name\": \"{extra}\", \"kind\": \"function\", \"anchor\": \"os.host.sigs\", \"w0304\": true}}"
    ));
    format!(
        "{{\n  \"schema\": \"wolf-prelude/0\",\n  \"names\": [\n{}\n  ],\n  \"marks\": [\n    {{\"name\": \"io\", \"anchor\": \"os.host.sigs\", \"by\": [\"{extra}\"]}}\n  ]\n}}\n",
        rows.join(",\n")
    )
}

/// `at_rev` through real git: tables at two tags, then a tag carrying
/// `spec/prelude.json` (read in preference to the tables beside it),
/// and the xtask binary's own output on the same repository.
#[test]
fn at_rev_reads_tags_both_ways() {
    let repo = std::env::temp_dir().join(format!("s212-prelude-diff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).expect("tmp repo");
    git(&repo, &["init", "-q"]);
    let src = |t: &str| std::fs::read_to_string(fixture(t)).expect("fixture");
    commit_tag(&repo, prelude::TABLES_PATH, &src("v0.2.22"), "t22");
    commit_tag(&repo, prelude::TABLES_PATH, &src("v0.2.23"), "t23");
    commit_tag(&repo, prelude::JSON_PATH, &synthetic_json("fs_frob"), "tj");

    let a = prelude::at_rev(&repo, "t22").expect("t22");
    let b = prelude::at_rev(&repo, "t23").expect("t23");
    let j = prelude::at_rev(&repo, "tj").expect("tj");
    assert_eq!(
        (a.source, b.source, j.source),
        (Source::Tables, Source::Tables, Source::Json)
    );
    assert_eq!(
        names(&prelude::diff(&a, &b).added),
        ["align_of", "offset_of"]
    );
    let dj = prelude::diff(&b, &j);
    assert_eq!(names(&dj.added), ["fs_frob"]);
    assert_eq!(dj.added[0].anchor.as_deref(), Some("os.host.sigs"));
    assert!(dj.marks.is_none(), "t23 published no marks");
    assert!(prelude::at_rev(&repo, "no-such-tag").is_err());

    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .current_dir(&repo)
            .args(args)
            .output()
            .expect("xtask runs")
    };
    let out = run(&["prelude-diff", "t22", "t23"]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.starts_with("prelude-diff t22..t23 (prelude.rs tables at t22, prelude.rs tables at t23): 2 added, 0 removed"), "{text}");
    let out = run(&["prelude-diff", "t23", "tj", "--markdown"]);
    assert_eq!(out.status.code(), Some(0));
    let md = String::from_utf8_lossy(&out.stdout);
    assert!(
        md.starts_with(
            "**New prelude names.** tj adds `fs_frob` (`fs_frob`: function, `[os.host.sigs]`)."
        ),
        "{md}"
    );
    let out = run(&["prelude-diff", "t22"]);
    assert_eq!(out.status.code(), Some(2), "one revision is a usage error");
    let out = run(&["prelude-diff", "t22", "nope"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "an unknown revision is an error"
    );
    let _ = std::fs::remove_dir_all(&repo);
}
