//! Resolution + capability diagnostics, snapshot-reviewed (s51):
//! E1504 (undeclared capability), E1505 (resolution failure), E1506
//! (ledger mismatch). File snapshots — the diag-catalog fixture rule
//! counts these as each code's reviewed artifact.
//!
//! Deterministic by construction: manifest displays are `wolf.pkg` /
//! `pkg://alias/wolf.pkg` (never on-disk paths, D7), and the fixture
//! messages carry fixed urls and fixed-content hashes only.

use std::path::{Path, PathBuf};

use wolf_diag::{HumanReporter, RenderOptions, Reporter, Sources};
use wolf_pkg::{Lock, ResolveOpts, resolve_project};

fn tmpdir(case: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn render(project: &wolf_pkg::Project) -> String {
    let mut sources = Sources::new();
    for m in &project.manifests {
        sources.add(m.file, m.display.clone(), m.text.as_bytes());
    }
    let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
    for d in &project.diagnostics {
        reporter.report(d);
    }
    reporter.take_output()
}

fn opts(lock: Option<Lock>) -> ResolveOpts {
    ResolveOpts {
        lock,
        fetch_unpinned: false,
        refresh: false,
        store: None,
        offline: false,
    }
}

#[test]
fn registry_source_is_the_x7_stub_e1505() {
    let dir = tmpdir("e1505_registry");
    std::fs::write(
        dir.join("wolf.pkg"),
        "pkg {\n    name:    \"demo/app\",\n    version: \"0.1.0\",\n\n    deps: {\n        json: { pkg: \"wolf-std/json\", major: 1, min: \"1.2.0\" },\n    },\n}\n",
    )
    .unwrap();
    let mut sm = wolf_span::SourceMap::new();
    let project = resolve_project(&dir, &mut sm, &opts(None));
    assert!(project.has_errors());
    insta::assert_snapshot!("e1505_registry_stub", render(&project));
}

#[test]
fn undeclared_capability_e1504() {
    let dir = tmpdir("e1504_caps");
    std::fs::write(
        dir.join("wolf.pkg"),
        "pkg {\n    name:         \"demo/app\",\n    version:      \"0.1.0\",\n\n    capabilities: [fs],\n}\n",
    )
    .unwrap();
    let mut sm = wolf_span::SourceMap::new();
    let project = resolve_project(&dir, &mut sm, &opts(None));
    assert!(!project.has_errors(), "{:?}", project.diagnostics);
    // The build's module graph says the root imports std.net — the
    // manifest declares only fs.
    let module_imports = vec![(String::new(), vec!["std.net".to_string()])];
    let diags = wolf_pkg::audit::capability_check(&project, &module_imports);
    let mut sources = Sources::new();
    for m in &project.manifests {
        sources.add(m.file, m.display.clone(), m.text.as_bytes());
    }
    let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
    for d in &diags {
        reporter.report(d);
    }
    insta::assert_snapshot!("e1504_undeclared_capability", reporter.take_output());
}

#[test]
fn tampered_store_tree_e1506() {
    let dir = tmpdir("e1506_tamper");
    // A pinned git dep whose store entry exists but holds tampered
    // bits: the re-derived address disagrees with the pin. No network,
    // no git — the store is the fixture.
    let store = dir.join("store");
    let pin_hex = "1111111111111111111111111111111111111111111111111111111111111111";
    std::fs::create_dir_all(store.join(pin_hex)).unwrap();
    std::fs::write(
        store.join(pin_hex).join("lib.lu"),
        "pub fn tampered() -> int {\n    666\n}\n",
    )
    .unwrap();
    let app = dir.join("app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(
        app.join("wolf.pkg"),
        "pkg {\n    name:    \"demo/app\",\n    version: \"0.1.0\",\n\n    deps: {\n        g: { git: \"file:///pinned.example/g.git\", tag: \"v1.0.0\" },\n    },\n}\n",
    )
    .unwrap();
    let lock = Lock::parse(&format!("g 1.0.0 b3:{pin_hex} caps=-\n")).unwrap();
    let mut o = opts(Some(lock));
    o.store = Some(store);
    let mut sm = wolf_span::SourceMap::new();
    let project = resolve_project(&app, &mut sm, &o);
    assert!(project.has_errors());
    insta::assert_snapshot!("e1506_tampered_store", render(&project));
}

// ----------------------------------------------------------- s53 codes ----

#[test]
fn offline_missing_dependency_is_e1509() {
    // s53 §5: one actionable error naming the package, the version, and
    // the command that would fetch it. Fails CLOSED — a package that was
    // never verified cannot be used unverified.
    let dir = tmpdir("e1509_offline");
    std::fs::write(
        dir.join("wolf.pkg"),
        "pkg {\n    name:    \"demo/app\",\n    version: \"0.1.0\",\n\n    deps: {\n        g: { git: \"https://example.org/g.git\", tag: \"v1.0.0\" },\n    },\n}\n",
    )
    .unwrap();
    let mut o = opts(None);
    o.offline = true;
    let mut sm = wolf_span::SourceMap::new();
    let project = resolve_project(&dir, &mut sm, &o);
    assert!(project.has_errors());
    insta::assert_snapshot!("e1509_offline_missing_dep", render(&project));
}

#[test]
fn frontmatter_drift_under_locked_is_e1508() {
    // s53 §4: `--locked` asserts the pin still answers the frontmatter.
    // The span is the frontmatter block — the thing that moved.
    let text = "#!/usr/bin/env -S wolf run\n//! pkg {\n//!     edition: \"1\",\n//! }\n\nfn main() -> !int { 0 }\n";
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(Path::new("drift.lu"));
    let mut sources = Sources::new();
    sources.add(file, "drift.lu".to_string(), text.as_bytes());
    let fm = wolf_pkg::script::find(text).expect("frontmatter");
    let d = wolf_pkg::script::drift_diagnostic(wolf_span::Span::new(file, fm.lo, fm.hi));
    let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
    reporter.report(&d);
    insta::assert_snapshot!("e1508_frontmatter_drift", reporter.take_output());
}
