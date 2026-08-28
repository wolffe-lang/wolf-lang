//! s124 acceptance — the module explains itself (D59, #145/#149).
//!
//! `wolf build <file>` forms the file's directory module: plain
//! siblings are in scope, duplicates across siblings are E0302, an
//! unparseable sibling fails the build instead of vanishing (exit 0
//! while ignoring a file was the bug), and standalone entries
//! (`member: false`, the corpus pair, scripts, `_test.lu`) stay out —
//! with an E0301 that says which situation the user is in.
//!
//! Everything uses `--emit=wir`, so no cc/linker is needed and the
//! tests run on every host; the run-for-real halves live in
//! `corpus/resolve/` (bare_sibling, dup_bare, broken_sibling,
//! plain_subdir, standalone_pair).

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixture(case: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("modform-{case}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (name, src) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, src).expect("write fixture");
    }
    dir
}

/// `wolf build <entry> --emit=wir`: (exit code, stderr).
fn build(dir: &Path, entry: &str) -> (i32, String) {
    let out = Command::new(wolf())
        .arg("build")
        .arg(dir.join(entry))
        .arg("--emit=wir")
        .arg("-o")
        .arg(dir.join("out.wir"))
        .output()
        .expect("run wolf build");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// #149 probe 1: a directiveless sibling is a member — its function
/// is in scope with no `use`.
#[test]
fn bare_sibling_is_in_scope() {
    let dir = fixture(
        "bare",
        &[
            ("a.lu", "fn main() -> !int {\n    helper()\n}\n"),
            ("b.lu", "fn helper() -> int {\n    42\n}\n"),
        ],
    );
    let (code, err) = build(&dir, "a.lu");
    assert_eq!(code, 0, "sibling joins the module:\n{err}");
}

/// #149 probe 2: a duplicate across bare siblings is E0302, with both
/// definition sites named — the answer lupin always gave.
#[test]
fn duplicate_across_siblings_is_e0302() {
    let dir = fixture(
        "dup",
        &[
            (
                "a.lu",
                "fn helper() -> int {\n    1\n}\n\nfn main() -> !int {\n    helper()\n}\n",
            ),
            ("b.lu", "fn helper() -> int {\n    2\n}\n"),
        ],
    );
    let (code, err) = build(&dir, "a.lu");
    assert_eq!(code, 1, "duplicates fail:\n{err}");
    assert!(err.contains("E0302"), "{err}");
    assert!(
        err.contains("a.lu") && err.contains("b.lu"),
        "both sites named:\n{err}"
    );
}

/// #149 probe 3 — the silence gets a voice: an unparseable sibling
/// fails the build; exit 0 while ignoring a file was the bug.
#[test]
fn unparseable_sibling_fails_loudly() {
    let dir = fixture(
        "broken",
        &[
            ("a.lu", "fn main() -> !int {\n    0\n}\n"),
            ("b.lu", "fn broken( {{{ not wolf\n"),
        ],
    );
    let (code, err) = build(&dir, "a.lu");
    assert_eq!(code, 1, "a broken member is a build error:\n{err}");
    assert!(err.contains("b.lu"), "the sibling is named:\n{err}");
}

/// The teachable E0301: the name lives in a standalone sibling, and
/// the note names the file, the marker, and the fix.
#[test]
fn standalone_sibling_note_names_the_marker() {
    let dir = fixture(
        "teach",
        &[
            ("a.lu", "fn main() -> !int {\n    helper()\n}\n"),
            (
                "b.lu",
                "//! member: false\nfn helper() -> int {\n    42\n}\n",
            ),
        ],
    );
    let (code, err) = build(&dir, "a.lu");
    assert_eq!(code, 1);
    assert!(err.contains("E0301"), "{err}");
    // The note wraps at a width the scratch path's length decides, so
    // compare with whitespace normalized (the wrap once split
    // "defines `helper`" and the assert went red on path length alone).
    let flat = err.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        err.contains("b.lu`") && flat.contains("defines `helper`"),
        "the defining file is named:\n{err}"
    );
    assert!(err.contains("member: false"), "the marker is named:\n{err}");
    assert!(
        err.contains("remove the standalone marker"),
        "the fix is stated:\n{err}"
    );
}

/// #145's other face: importing a directory whose files are all
/// standalone entries explains why no module forms there.
#[test]
fn all_standalone_directory_note() {
    let dir = fixture(
        "allstandalone",
        &[
            ("main.lu", "use tools\n\nfn main() -> !int {\n    0\n}\n"),
            (
                "tools/one.lu",
                "//! check: run(exit=0)\n//! phase: run\nfn main() -> !int {\n    0\n}\n",
            ),
        ],
    );
    let (code, err) = build(&dir, "main.lu");
    assert_eq!(code, 1);
    assert!(err.contains("E0301"), "{err}");
    // The note wraps at a width the scratch path's length decides, so
    // compare with whitespace normalized (the wrap once split
    // "standalone entry" and the assert went red on path length alone).
    let flat = err.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("declares itself a standalone entry"),
        "the situation is explained:\n{err}"
    );
    assert!(
        err.contains("tools/one.lu"),
        "the opted-out file is named:\n{err}"
    );
}

/// Target 4 — several standalone programs share one directory, in
/// both spellings (`member: false`, a `_test.lu` name), each building
/// alone; a plain sibling is a *shared member* visible to every one
/// of them. A standalone mark opts the file itself out — it never
/// shrinks the module the build forms around it (D59).
#[test]
fn standalone_programs_coexist() {
    let dir = fixture(
        "coexist",
        &[
            (
                "scratch.lu",
                "//! member: false\nfn main() -> !int {\n    base_value()\n}\n",
            ),
            (
                "probe.lu",
                "//! member: false\nfn main() -> !int {\n    base_value() + 1\n}\n",
            ),
            (
                "app_test.lu",
                "fn main() -> !int {\n    base_value() + 2\n}\n",
            ),
            ("util.lu", "fn base_value() -> int {\n    0\n}\n"),
        ],
    );
    for entry in ["scratch.lu", "probe.lu", "app_test.lu"] {
        let (code, err) = build(&dir, entry);
        assert_eq!(
            code, 0,
            "{entry} builds alone, with the plain helper in scope:\n{err}"
        );
    }
}

/// The flip side, pinned on purpose: a *plain* sibling with `main` is
/// a member of every build's module — a standalone entry does not
/// exclude it, so the collision reports E0302 (whose note names the
/// `member: false` escape).
#[test]
fn plain_main_beside_a_standalone_one_is_e0302() {
    let dir = fixture(
        "plainmain",
        &[
            ("app.lu", "fn main() -> !int {\n    0\n}\n"),
            (
                "scratch.lu",
                "//! member: false\nfn main() -> !int {\n    1\n}\n",
            ),
        ],
    );
    let (code, err) = build(&dir, "scratch.lu");
    assert_eq!(code, 1, "the plain main joins and collides:\n{err}");
    assert!(err.contains("E0302"), "{err}");
}
