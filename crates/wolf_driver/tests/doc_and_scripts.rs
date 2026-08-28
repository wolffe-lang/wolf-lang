//! s53 acceptance — `wolf doc`, doctests, and script mode.
//!
//! The claims under test, in the contract's order:
//!
//! - **Doctests**: a fixture package with passing, `no_run`,
//!   `should_fail` and `ignore` fences produces the expected
//!   `wolf test --json` records, and a broken fence fails the run.
//! - **`wolf doc`**: HTML + a JSON index, signatures from the compiler's
//!   own pretty-printer, `pub`/`pub(pkg)` respected, `--private`
//!   widening the surface, `--check` catching drift byte-for-byte.
//! - **Script end-to-end**: the frontmatter example runs cold with
//!   resolve+build, then reruns WARM with no resolver and no compiler,
//!   well under the 50ms overhead target; `--locked` catches frontmatter
//!   drift; a std-only script writes nothing outside `builds/`.
//! - **The prompt**: script mode asks, `--yes` answers, a non-TTY errors,
//!   and project mode never asks.
//!
//! Native codegen is linux/x86-64 in c06, so the executing halves gate
//! there; the refusal and generation halves would run anywhere, and one
//! honest gate beats two (the s51 suite's rule, kept).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn repo_root() -> PathBuf {
    // crates/wolf_driver -> crates -> <root>
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/script")
}

fn doc_fixture() -> PathBuf {
    repo_root().join("crates/wolf_doc/fixtures/pkg")
}

fn case_dir(name: &str) -> PathBuf {
    let dest = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest).expect("mkdir case");
    dest
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("read fixture dir").flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_tree(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).expect("copy fixture file");
        }
    }
}

/// Run `wolf` with a private cache root, so nothing touches the
/// developer's real `~/.cache/wolf` and the tests are hermetic.
fn run_wolf(dir: &Path, cache: &Path, args: &[&str]) -> Output {
    Command::new(wolf())
        .args(args)
        .current_dir(dir)
        .env_remove("WOLF_STD")
        .env("WOLF_CACHE", cache)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("wolf runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Exit 2 from a build = environment (no cc/rt lib): skip loudly, as
/// every native suite in this crate does.
fn env_skip(out: &Output, what: &str) -> bool {
    if out.status.code() == Some(2) {
        eprintln!("SKIP {what}: {}", stderr(out).trim());
        return true;
    }
    false
}

// ------------------------------------------------------------ doctests ----

#[test]
fn doctest_records_cover_every_directive() {
    let case = case_dir("s53_doctests");
    let out = run_wolf(
        &repo_root(),
        &case,
        &[
            "test",
            "--doc",
            "--json",
            doc_fixture().to_str().expect("utf-8 path"),
        ],
    );
    let records = stdout(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout:\n{records}\nstderr:\n{}",
        stderr(&out)
    );
    // One suite record, six test records, one summary.
    assert!(
        records.contains(r#""event":"suite""#),
        "a suite record per documented file:\n{records}"
    );
    for (name, detail) in [
        ("shelf::slot_of#0 (doctest)", r#""detail":"exit(0)""#),
        ("shelf::rows#0 (doctest)", r#""detail":"exit(0)""#),
        (
            "shelf::width_of#0 (doctest)",
            r#""detail":"compiles (no_run)""#,
        ),
        ("shelf::is_slot#0 (doctest)", r#""detail":"refused: E0401""#),
        ("total_width#0 (doctest)", r#""detail":"exit(0)""#),
        ("shelf#0 (doctest)", r#""detail":"exit(0)""#),
    ] {
        assert!(
            records.contains(&format!(r#""name":"{name}""#)),
            "missing the record for {name}:\n{records}"
        );
        assert!(
            records.contains(detail),
            "{name}: expected {detail}\n{records}"
        );
    }
    // `ignore` is prose in a fence: it is NOT a test, and no record
    // claims otherwise.
    assert!(
        !records.contains("label#0"),
        "an `ignore` fence became a test:\n{records}"
    );
    assert!(
        records.contains(r#""passed":6"#) && records.contains(r#""failed":0"#),
        "the summary:\n{records}"
    );
}

#[test]
fn a_broken_doctest_fails_the_run() {
    // The covenant, stated as an exit code: a doc example that stops
    // compiling fails `wolf test`, so documentation cannot rot.
    let case = case_dir("s53_doctest_broken");
    let pkg = case.join("pkg");
    copy_tree(&doc_fixture(), &pkg);
    copy_tree(
        &doc_fixture().parent().expect("fixtures").join("shelf"),
        &case.join("shelf"),
    );
    let shelf = case.join("shelf/shelf.lu");
    let text = std::fs::read_to_string(&shelf).expect("shelf source");
    std::fs::write(
        &shelf,
        text.replace(
            "/// shelf.slot_of(0) == 1",
            "/// shelf.slot_of(0, 1, 2) == 1",
        ),
    )
    .expect("break the doctest");
    let out = run_wolf(
        &case,
        &case.join("cache"),
        &["test", "--doc", "--json", "pkg"],
    );
    let records = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "stdout:\n{records}");
    assert!(
        records.contains(
            r#""name":"shelf::slot_of#0 (doctest)","schema":"wolf-test/0","status":"fail""#
        ),
        "the broken fence is the failure:\n{records}"
    );
    assert!(
        stderr(&out).contains("E0402"),
        "the doctest's own diagnostic renders:\n{}",
        stderr(&out)
    );
}

#[test]
fn no_doc_and_doc_are_exclusive() {
    let case = case_dir("s53_doc_flags");
    let out = run_wolf(&case, &case, &["test", "--doc", "--no-doc"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("exclusive"), "{}", stderr(&out));
}

// ------------------------------------------------------------ wolf doc ----

#[test]
fn doc_emits_html_and_a_stable_json_index() {
    let case = case_dir("s53_doc_out");
    let out = run_wolf(
        &repo_root(),
        &case,
        &[
            "doc",
            doc_fixture().to_str().expect("utf-8 path"),
            "--out",
            case.join("doc").to_str().expect("utf-8 path"),
        ],
    );
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    let root = case.join("doc");
    for f in [
        "index.html",
        "index.json",
        "module.html",
        "module.shelf.html",
        "style.css",
    ] {
        assert!(root.join(f).is_file(), "{f} was not written");
    }
    // No JavaScript, ever: the pages are readable from `file://`.
    for f in ["index.html", "module.shelf.html"] {
        let html = std::fs::read_to_string(root.join(f)).expect("page");
        assert!(!html.contains("<script"), "{f} carries a script tag");
        assert!(html.contains("<!DOCTYPE html>"), "{f} is not a document");
    }
    let json = std::fs::read_to_string(root.join("index.json")).expect("index");
    assert!(json.starts_with(r#"{"schema":"wolf-doc/0""#), "{json}");
    // Signatures come from the compiler's pretty-printer, so they carry
    // resolved types rather than source text.
    assert!(
        json.contains(r#""sig":"fn slot_of(w: int) -> int""#),
        "the elaborated signature:\n{json}"
    );
    // `pub(pkg)` is published surface, and the index says which.
    assert!(
        json.contains(r#""name":"rows","kind":"fn","vis":"pub(pkg)""#),
        "{json}"
    );
    // The private item is NOT in a default run.
    assert!(
        !json.contains(r#""name":"hidden""#),
        "a private item leaked:\n{json}"
    );
    // The resolved dependency set is recorded: a page describes one world.
    assert!(
        json.contains(r#""deps":[{"alias":"shelf","name":"demo/shelf","version":"0.3.0"}]"#),
        "the dependency surface:\n{json}"
    );
    // Doctests ride in the index with their directives.
    assert!(json.contains(r#""directives":["no_run"]"#), "{json}");
    assert!(
        json.contains(r#""directives":["should_fail(E0401)"]"#),
        "{json}"
    );
    // Byte-stability: a second run produces identical bytes.
    let again = case.join("doc2");
    let out = run_wolf(
        &repo_root(),
        &case,
        &[
            "doc",
            doc_fixture().to_str().expect("utf-8 path"),
            "--out",
            again.to_str().expect("utf-8 path"),
        ],
    );
    assert_eq!(out.status.code(), Some(0));
    for f in ["index.html", "index.json", "module.shelf.html", "style.css"] {
        assert_eq!(
            std::fs::read(root.join(f)).expect("first"),
            std::fs::read(again.join(f)).expect("second"),
            "{f} is not byte-stable across runs"
        );
    }
}

#[test]
fn doc_check_is_the_ci_posture() {
    let case = case_dir("s53_doc_check");
    let root = case.join("doc");
    let fixture = doc_fixture();
    let regen = |args: &[&str]| {
        let mut all = vec![
            "doc",
            fixture.to_str().expect("utf-8 path"),
            "--out",
            root.to_str().expect("utf-8 path"),
        ];
        all.extend_from_slice(args);
        run_wolf(&repo_root(), &case, &all)
    };
    assert_eq!(regen(&[]).status.code(), Some(0));
    // In sync.
    let out = regen(&["--check"]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert!(stderr(&out).contains("in sync"), "{}", stderr(&out));
    // A hand-edited page is CHANGED.
    let page = root.join("module.shelf.html");
    let mut html = std::fs::read_to_string(&page).expect("page");
    html.push_str("<!-- someone edited the output -->\n");
    std::fs::write(&page, html).expect("tamper");
    let out = regen(&["--check"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("CHANGED module.shelf.html"),
        "{}",
        stderr(&out)
    );
    // A deleted page is MISSING.
    std::fs::remove_file(&page).expect("delete");
    let out = regen(&["--check"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("MISSING module.shelf.html"),
        "{}",
        stderr(&out)
    );
    // A stale page left behind is reported too: it is still served.
    assert_eq!(regen(&[]).status.code(), Some(0));
    std::fs::write(root.join("module.ghost.html"), "stale\n").expect("stray");
    let out = regen(&["--check"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("STALE   module.ghost.html"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn private_widens_the_surface_and_coverage_ignores_it() {
    let case = case_dir("s53_doc_private");
    let fixture = doc_fixture();
    let out = run_wolf(
        &repo_root(),
        &case,
        &[
            "doc",
            fixture.to_str().expect("path"),
            "--private",
            "--json",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    let json = stdout(&out);
    assert!(
        json.contains(r#""name":"hidden","kind":"fn","vis":"private""#),
        "{json}"
    );
    assert!(json.contains(r#""private":true"#), "{json}");
    // Coverage counts CONSUMER-visible items only: the private item does
    // not move the numbers a gate would read.
    let out = run_wolf(
        &repo_root(),
        &case,
        &["doc", fixture.to_str().expect("path"), "--coverage"],
    );
    let plain = stdout(&out);
    let out = run_wolf(
        &repo_root(),
        &case,
        &[
            "doc",
            fixture.to_str().expect("path"),
            "--private",
            "--coverage",
        ],
    );
    let with_private = stdout(&out);
    let line = |s: &str| s.lines().next().unwrap_or("").to_string();
    assert_eq!(
        line(&plain),
        line(&with_private),
        "--private moved coverage"
    );
    assert!(line(&plain).contains("6/7 items documented"), "{plain}");
}

#[test]
fn coverage_gate_names_the_burn_down() {
    let case = case_dir("s53_doc_coverage");
    let out = run_wolf(
        &repo_root(),
        &case,
        &[
            "doc",
            doc_fixture().to_str().expect("path"),
            "--coverage",
            "--require-docs",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "the gate refuses an incomplete set"
    );
    let report = stdout(&out);
    assert!(report.contains("no doc comment: stack_of"), "{report}");
    assert!(report.contains("no doctest:     shelf::label"), "{report}");
}

#[test]
fn a_broken_intra_doc_link_warns_and_denies() {
    let case = case_dir("s53_doc_links");
    let pkg = case.join("pkg");
    copy_tree(&doc_fixture(), &pkg);
    copy_tree(
        &doc_fixture().parent().expect("fixtures").join("shelf"),
        &case.join("shelf"),
    );
    let shelf = case.join("shelf/shelf.lu");
    let text = std::fs::read_to_string(&shelf).expect("source");
    std::fs::write(&shelf, text.replace("[width_of]", "[width_ov]")).expect("break the link");
    let out = run_wolf(
        &case,
        &case.join("cache"),
        &["doc", "pkg", "--out", "out", "--deny-warnings"],
    );
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("W1501"), "{err}");
    assert!(err.contains("`width_ov`"), "{err}");
    // Without --deny-warnings it is a warning, and the docs still build.
    let out = run_wolf(&case, &case.join("cache"), &["doc", "pkg", "--out", "out2"]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert!(stderr(&out).contains("W1501"), "{}", stderr(&out));
    // The page marks the dead link rather than hiding it.
    let html = std::fs::read_to_string(case.join("out2/module.shelf.html")).expect("page");
    assert!(html.contains("class=\"broken\">width_ov"), "{html}");
}

// --------------------------------------------------------- script mode ----

#[test]
fn script_runs_cold_then_warm_with_nothing_beside_it() {
    let case = case_dir("s53_script_cold_warm");
    copy_tree(&fixtures(), &case.join("s"));
    let cache = case.join("cache");
    let dir = case.join("s");
    let out = run_wolf(&dir, &cache, &["run", "counter.lu"]);
    if env_skip(&out, "script cold") {
        return;
    }
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(stdout(&out), "twist(20) = 41\n");
    // The script's own directory is untouched: no `.lu-cache`, no
    // `wolf.sum`, no lockfile. A script stays ONE FILE.
    let beside: Vec<String> = std::fs::read_dir(&dir)
        .expect("read script dir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let mut beside = beside;
    beside.sort();
    assert_eq!(
        beside,
        vec!["counter.lu", "plain.lu", "util"],
        "state leaked beside the script"
    );
    // The pin exists, in the cache, where the layout says.
    let scripts = cache.join("scripts");
    assert!(scripts.is_dir(), "no script state directory");
    let pinned: Vec<PathBuf> = std::fs::read_dir(&scripts)
        .expect("read scripts dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n != "by-path"))
        .collect();
    assert_eq!(pinned.len(), 1, "one script, one pin: {pinned:?}");
    assert!(pinned[0].join("wolf.sum").is_file(), "the hidden ledger");
    assert!(
        pinned[0].join("resolution").is_file(),
        "the pinned resolution"
    );
    let sum = std::fs::read_to_string(pinned[0].join("wolf.sum")).expect("ledger");
    assert!(sum.contains("util 0.2.0"), "{sum}");

    // Warm: the second run must not resolve and must not compile. The
    // observable proof is the build log's absence plus the timing.
    let start = std::time::Instant::now();
    let out = run_wolf(&dir, &cache, &["run", "counter.lu"]);
    let warm = start.elapsed();
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(stdout(&out), "twist(20) = 41\n");
    assert_eq!(
        stderr(&out),
        "",
        "a warm run compiles nothing and says nothing"
    );
    // The target is under 50ms of OVERHEAD on the CI box; this measures
    // the whole wall clock of two process spawns, so a generous ceiling
    // still proves the resolver and the compiler did not run (a cold run
    // of this fixture is two orders of magnitude slower).
    assert!(
        warm < std::time::Duration::from_millis(1500),
        "warm rerun took {warm:?} — the cached artifact was not used"
    );
    // And it is genuinely offline: the same run under --offline works,
    // because a warm run reaches neither the resolver nor the network.
    let out = run_wolf(&dir, &cache, &["run", "--offline", "counter.lu"]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
}

#[test]
fn a_std_only_script_writes_only_builds() {
    let case = case_dir("s53_script_plain");
    copy_tree(&fixtures(), &case.join("s"));
    let cache = case.join("cache");
    let out = run_wolf(&case.join("s"), &cache, &["run", "plain.lu"]);
    if env_skip(&out, "plain script") {
        return;
    }
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(stdout(&out), "plain\n");
    // No frontmatter means no resolution to pin: `builds/` is the only
    // thing that grows, plus the path pointer that makes `--locked`
    // answerable at all.
    assert!(cache.join("builds").is_dir(), "no build cache");
    assert!(
        !cache.join("store").exists(),
        "a std-only script touched the store"
    );
    let scripts = cache.join("scripts");
    if scripts.is_dir() {
        let dirs: Vec<String> = std::fs::read_dir(&scripts)
            .expect("read")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            dirs,
            vec!["by-path"],
            "a std-only script pinned a resolution: {dirs:?}"
        );
    }
}

#[test]
fn locked_catches_frontmatter_drift() {
    let case = case_dir("s53_script_locked");
    copy_tree(&fixtures(), &case.join("s"));
    let cache = case.join("cache");
    let dir = case.join("s");
    let script = dir.join("counter.lu");
    let out = run_wolf(&dir, &cache, &["run", "counter.lu"]);
    if env_skip(&out, "locked") {
        return;
    }
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    // Unchanged: --locked is satisfied.
    let out = run_wolf(&dir, &cache, &["run", "--locked", "counter.lu"]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    // Drifted: --locked refuses, with the frontmatter block underlined
    // and nothing in the cache modified.
    let text = std::fs::read_to_string(&script).expect("script");
    std::fs::write(
        &script,
        text.replace(r#"wolf:    "0.9""#, r#"wolf:    "0.10""#),
    )
    .expect("edit the frontmatter");
    let out = run_wolf(&dir, &cache, &["run", "--locked", "counter.lu"]);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("E1508"), "{err}");
    assert!(err.contains("counter.lu"), "the real file is named:\n{err}");
    // Without --locked the drift re-resolves cleanly (a new script-id, so
    // no pin was edited in place).
    let out = run_wolf(&dir, &cache, &["run", "counter.lu"]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(stdout(&out), "twist(20) = 41\n");
}

#[test]
fn frontmatter_diagnostics_point_at_the_script() {
    let case = case_dir("s53_script_diag");
    copy_tree(&fixtures(), &case.join("s"));
    let dir = case.join("s");
    let script = dir.join("counter.lu");
    let text = std::fs::read_to_string(&script).expect("script");
    std::fs::write(
        &script,
        text.replace(r#"//!     wolf:    "0.9","#, r#"//!     version: "1.0.0","#),
    )
    .expect("out-of-subset key");
    let out = run_wolf(&dir, &case.join("cache"), &["run", "counter.lu"]);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("E1507"), "{err}");
    assert!(
        err.contains("counter.lu:10:9"),
        "real file, line and column:\n{err}"
    );
    assert!(
        err.contains(r#"//!     version: "1.0.0","#),
        "the script's own line renders under the caret:\n{err}"
    );
    assert!(
        err.contains("wolf init --from-script"),
        "the promotion fix-it:\n{err}"
    );
}

#[test]
fn the_prompt_is_script_modes_alone() {
    let case = case_dir("s53_script_prompt");
    copy_tree(&fixtures(), &case.join("s"));
    let dir = case.join("s");
    let cache = case.join("cache");
    let script = dir.join("counter.lu");
    // Remove the dep entry: `use util` now has no dependency.
    let text = std::fs::read_to_string(&script).expect("script");
    let stripped = text.replace(
        "//!     deps: {\n//!         util: { path: \"./util\" },\n//!     },\n",
        "",
    );
    assert_ne!(text, stripped, "the fixture's deps block moved");
    std::fs::write(&script, &stripped).expect("strip deps");
    // Non-TTY: an error, never a hang. (`run_wolf` nulls stdin.)
    let out = run_wolf(&dir, &cache, &["run", "counter.lu"]);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("is imported and not in the frontmatter"),
        "{err}"
    );
    assert!(
        err.contains("--yes"),
        "the automation answer is named:\n{err}"
    );
    // `--yes` accepts, writes real frontmatter, and the script runs.
    let out = run_wolf(&dir, &cache, &["run", "--yes", "counter.lu"]);
    if env_skip(&out, "prompt --yes") {
        return;
    }
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(stdout(&out), "twist(20) = 41\n");
    let edited = std::fs::read_to_string(&script).expect("script");
    assert!(
        edited.contains(r#"//!     deps: { util: { path: "./util" } },"#),
        "the accepted entry is frontmatter a human would write:\n{edited}"
    );
    // Project mode NEVER prompts: the s51 error-with-fix-it stands.
    let proj = case.join("p");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pkg/dogfood"),
        &proj,
    );
    std::fs::remove_file(proj.join("app/wolf.pkg")).expect("drop the manifest");
    let out = run_wolf(&proj, &cache, &["run", "app/main.lu"]);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    let err = stderr(&out);
    assert!(
        !err.contains("frontmatter"),
        "project mode prompted:\n{err}"
    );
    assert!(err.contains("E0301"), "{err}");
}

#[test]
fn init_from_script_promotes_a_script_to_a_package() {
    // The verb E1507's fix-it names, doing what the message promises.
    let case = case_dir("s53_init_from_script");
    copy_tree(&fixtures(), &case.join("s"));
    let dir = case.join("s");
    let out = run_wolf(
        &dir,
        &case.join("cache"),
        &["init", "--from-script", "counter.lu", "--dir", "../p"],
    );
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    let manifest = std::fs::read_to_string(case.join("p/wolf.pkg")).expect("manifest");
    assert!(
        manifest.contains(r#"name:    "local/counter""#),
        "{manifest}"
    );
    assert!(
        manifest.contains(r#"util: { path: "./util" }"#),
        "verbatim deps:\n{manifest}"
    );
    assert!(manifest.contains(r#"wolf:    "0.9""#), "{manifest}");
    let main = std::fs::read_to_string(case.join("p/main.lu")).expect("module");
    assert!(!main.starts_with("#!"), "the shebang travelled:\n{main}");
    // The frontmatter LINES are gone; the prose that mentions them is
    // documentation and stays, which is why this checks for an entry
    // rather than for the word `pkg`.
    assert!(
        !main.contains("//!     edition:") && !main.contains("//!     deps:"),
        "the frontmatter travelled:\n{main}"
    );
    assert!(main.contains("use util"), "{main}");
    // The script is untouched, and a promotion never overwrites.
    assert!(dir.join("counter.lu").is_file());
    let out = run_wolf(
        &dir,
        &case.join("cache"),
        &["init", "--from-script", "counter.lu", "--dir", "../p"],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("refusing to overwrite"),
        "{}",
        stderr(&out)
    );
}

// --------------------------------------------------------- cache verbs ----

#[test]
fn cache_path_and_gc_are_the_only_deletion() {
    let case = case_dir("s53_cache");
    copy_tree(&fixtures(), &case.join("s"));
    let cache = case.join("cache");
    let dir = case.join("s");
    let out = run_wolf(&dir, &cache, &["run", "plain.lu"]);
    if env_skip(&out, "cache gc") {
        return;
    }
    assert_eq!(out.status.code(), Some(0));
    let out = run_wolf(&dir, &cache, &["cache", "path"]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    let listing = stdout(&out);
    for name in ["store", "log", "builds", "scripts"] {
        assert!(
            listing.contains(name),
            "{name} is not in the layout:\n{listing}"
        );
    }
    // A dry run deletes nothing.
    let out = run_wolf(&dir, &cache, &["cache", "gc", "--dry-run"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).contains("would remove"), "{}", stdout(&out));
    assert!(cache.join("builds").is_dir(), "a dry run deleted");
    // The real thing collects derived state and keeps fetched sources.
    let out = run_wolf(&dir, &cache, &["cache", "gc"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).contains("removed"), "{}", stdout(&out));
    let leftovers: Vec<String> = std::fs::read_dir(cache.join("builds"))
        .expect("builds survives as a directory")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(leftovers.is_empty(), "gc left artifacts: {leftovers:?}");
    // And the script still runs afterwards: the cache is derived state.
    let out = run_wolf(&dir, &cache, &["run", "plain.lu"]);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(stdout(&out), "plain\n");
}
