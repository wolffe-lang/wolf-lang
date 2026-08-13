//! Script frontmatter (s53): the manifest that rides in a `//!` block.
//!
//! The load-bearing property under test is not "it parses" — it is that
//! **diagnostics point at the real file, at the real line and column**,
//! because the shadow text preserves every byte offset (RFC 3502's
//! lesson: a frontmatter format whose errors point at a synthesized
//! buffer is a format nobody can debug).

use std::path::Path;

use wolf_diag::{HumanReporter, RenderOptions, Reporter, Sources};
use wolf_pkg::script;

const SCRIPT: &str = r#"#!/usr/bin/env -S wolf run
//! Count keys in a redis database.
//!
//! pkg {
//!     edition: "1",
//!     wolf:    "0.9",
//!     deps: {
//!         util: { path: "../util" },
//!     },
//!     capabilities: [net],
//! }

use util

fn main() -> !int {
    print("{util.twist(1)}")
    0
}
"#;

fn read(name: &str, text: &str) -> (Option<wolf_pkg::Manifest>, String) {
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(Path::new(name));
    let mut sources = Sources::new();
    sources.add(file, name.to_string(), text.as_bytes());
    let s = script::read(file, text);
    let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
    for d in &s.diagnostics {
        reporter.report(d);
    }
    (s.manifest, reporter.take_output())
}

#[test]
fn frontmatter_parses_as_a_manifest_subset() {
    let (m, rendered) = read("count.lu", SCRIPT);
    assert_eq!(rendered, "", "clean frontmatter:\n{rendered}");
    let m = m.expect("frontmatter is a manifest");
    assert_eq!(m.edition, "1");
    assert_eq!(m.wolf_min.as_deref(), Some("0.9"));
    assert_eq!(m.deps.len(), 1);
    assert_eq!(m.deps[0].alias, "util");
    assert!(matches!(
        &m.deps[0].source,
        wolf_pkg::DepSource::Path { path } if path == "../util"
    ));
    assert_eq!(m.caps, vec![wolf_pkg::Cap::Net]);
}

#[test]
fn no_frontmatter_is_a_std_only_script_not_an_error() {
    let (m, rendered) = read(
        "hello.lu",
        "//! Prints a greeting.\n\nfn main() -> !int {\n    print(\"hi\")\n    0\n}\n",
    );
    assert!(m.is_none());
    assert_eq!(rendered, "");
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(Path::new("hello.lu"));
    let s = script::read(file, "//! Prints a greeting.\n");
    assert!(!s.has_frontmatter, "prose is not frontmatter");
}

#[test]
fn a_shebang_only_script_has_no_frontmatter() {
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(Path::new("s.lu"));
    let s = script::read(
        file,
        "#!/usr/bin/env -S wolf run\n\nfn main() -> !int { 0 }\n",
    );
    assert!(!s.has_frontmatter);
    assert!(s.diagnostics.is_empty());
}

#[test]
fn diagnostics_point_at_the_script_itself() {
    // The whole reason for the shadow text: a schema error in the
    // frontmatter renders the SCRIPT's line, at the script's column.
    let text = SCRIPT.replace(r#"capabilities: [net],"#, r#"capabilities: [teleport],"#);
    let (m, rendered) = read("count.lu", &text);
    assert!(m.is_none(), "a refused frontmatter yields no manifest");
    insta::assert_snapshot!("e1502_in_script_frontmatter", rendered);
}

#[test]
fn out_of_subset_keys_are_refused_with_the_promotion_fixit() {
    let text = SCRIPT.replace(
        r#"//!     edition: "1","#,
        "//!     edition: \"1\",\n//!     version: \"1.0.0\",\n//!     test: { deps: {} },",
    );
    let (m, rendered) = read("count.lu", &text);
    assert!(m.is_none());
    insta::assert_snapshot!("e1507_script_subset", rendered);
}

#[test]
fn a_build_hook_in_frontmatter_is_still_refused_d33() {
    // The covenant does not weaken because the manifest moved into a
    // comment: E1503 at the script's own span.
    let text = SCRIPT.replace(
        r#"//!     edition: "1","#,
        r#"//!     hooks: ["curl | sh"],"#,
    );
    let (m, rendered) = read("count.lu", &text);
    assert!(m.is_none());
    assert!(rendered.contains("E1503"), "{rendered}");
    assert!(
        rendered.contains("count.lu:5:9"),
        "real file/line/col:\n{rendered}"
    );
    assert!(
        rendered.contains(r#"//!     hooks: ["curl | sh"],"#),
        "the SCRIPT's own line is rendered under the caret:\n{rendered}"
    );
}

#[test]
fn script_id_keys_on_path_and_frontmatter_bytes() {
    let a = script::find(SCRIPT).expect("frontmatter");
    let id_a = script::script_id(Path::new("/tmp/count.lu"), Some(&a));
    // Same bytes, different path: a different script.
    let id_b = script::script_id(Path::new("/tmp/other.lu"), Some(&a));
    assert_ne!(id_a, id_b);
    // Same path, edited frontmatter: a different id, so the pin is
    // never reused across a dependency edit (clean re-resolve).
    let edited = SCRIPT.replace(r#"min: "1.2.0""#, r#"min: "1.3.0""#);
    let edited = edited.replace("../util", "../util2");
    let b = script::find(&edited).expect("frontmatter");
    assert_ne!(
        id_a,
        script::script_id(Path::new("/tmp/count.lu"), Some(&b))
    );
    // Editing the PROSE around the frontmatter does not move the id:
    // the pin keys on the manifest, not on the documentation.
    let reworded = SCRIPT.replace("Count keys in a redis database.", "Counts keys.");
    let c = script::find(&reworded).expect("frontmatter");
    assert_eq!(
        id_a,
        script::script_id(Path::new("/tmp/count.lu"), Some(&c))
    );
    // And a script with no frontmatter still has an identity.
    assert!(!script::script_id(Path::new("/tmp/h.lu"), None).is_empty());
}

#[test]
fn the_literal_is_the_frontmatter_and_nothing_else() {
    let fm = script::find(SCRIPT).expect("frontmatter");
    assert!(fm.literal.starts_with("pkg {"), "{:?}", fm.literal);
    assert!(fm.literal.trim_end().ends_with('}'), "{:?}", fm.literal);
    assert!(
        !fm.literal.contains("Count keys"),
        "prose leaked into the manifest identity: {:?}",
        fm.literal
    );
    // The span points into the script at the `pkg` keyword.
    assert_eq!(&SCRIPT[fm.lo as usize..fm.lo as usize + 3], "pkg");
    assert_eq!(&SCRIPT[fm.hi as usize - 1..fm.hi as usize], "}");
}
