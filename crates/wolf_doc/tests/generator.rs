//! The generator as a pure function (s53): a resolved package in, a set
//! of (path, bytes) pairs out. These tests pin the parts a `--check`
//! gate depends on — determinism, the JSON schema, the markdown subset,
//! and the fence-directive language — without a filesystem or a driver.

use wolf_query::{Directive, DocFence};

fn resolve(files: &[(&[&str], &str, &str)]) -> wolf_sema::Resolution {
    let mut loader = wolf_sema::MemoryLoader::new("demo");
    for (module, name, src) in files {
        loader.add_file(module, name, src);
    }
    wolf_sema::resolve_package(&mut loader, &wolf_sema::AliasTable::default())
        .expect("the fixture resolves")
}

const SRC: &str = r#"//! The module's own prose. This is its summary sentence.
//!
//! ```wolf
//! demo.one() == 1
//! ```

/// One. The rest of the body says more, and links to [two].
///
/// - a bullet
///   with a continuation
/// - another
///
/// ```wolf,no_run
/// let x = one()
/// ```
pub fn one() -> int {
    1
}

/// Two, whose fence must be refused.
///
/// ```wolf,should_fail(E0401, E0402)
/// two("nope")
/// ```
pub fn two(n: int) -> int {
    n
}

/// Undocumentable prose in a fence.
///
/// ```text
/// <n>: <label>
/// ```
pub(pkg) fn three() -> int {
    3
}

fn hidden() -> int {
    4
}
"#;

#[test]
fn the_doc_model_reads_one_comment_model() {
    let res = resolve(&[(&[], "demo.lu", SRC)]);
    let docs = wolf_query::doc_package(&res, false);
    assert_eq!(docs.modules.len(), 1);
    let m = &docs.modules[0];
    let doc = m.doc.as_ref().expect("the module has prose");
    assert_eq!(doc.summary, "The module's own prose.");
    assert_eq!(doc.fences.len(), 1);
    assert!(doc.fences[0].is_doctest());
    // Private items are not published surface.
    let names: Vec<&str> = m.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["one", "three", "two"],
        "canonical (kind, name) order"
    );
    // Summaries stop at the first sentence; bodies keep everything.
    let one = &m.items[0];
    assert_eq!(one.doc.as_ref().expect("doc").summary, "One.");
    assert!(one.doc.as_ref().expect("doc").text.contains("a bullet"));
    assert_eq!(one.doc.as_ref().expect("doc").links, vec!["two"]);
    // Signatures come from the compiler's own pretty-printer.
    assert_eq!(one.sig, "fn one() -> int");
    // The directive language, all of it.
    let fence = |i: usize| -> &DocFence { &m.items[i].doc.as_ref().expect("doc").fences[0] };
    assert_eq!(fence(0).directives, vec![Directive::NoRun]);
    assert_eq!(
        fence(2).directives,
        vec![Directive::ShouldFail(vec![
            "E0401".to_string(),
            "E0402".to_string()
        ])],
        "a should_fail list survives the comma inside its parentheses"
    );
    // A `text` fence is prose, not a program.
    assert!(!fence(1).is_doctest(), "a text fence became a doctest");
    assert_eq!(fence(1).lang, "text");
}

#[test]
fn private_is_opt_in_and_never_moves_coverage() {
    let res = resolve(&[(&[], "demo.lu", SRC)]);
    let public = wolf_query::doc_package(&res, false);
    let private = wolf_query::doc_package(&res, true);
    assert_eq!(public.modules[0].items.len(), 3);
    assert_eq!(private.modules[0].items.len(), 4);
    assert!(private.modules[0].items.iter().any(|i| i.name == "hidden"));
    let a = wolf_doc::coverage(&public);
    let b = wolf_doc::coverage(&private);
    assert_eq!((a.total, a.documented, a.with_doctest), (3, 3, 2));
    assert_eq!(
        (a.total, a.documented, a.with_doctest),
        (b.total, b.documented, b.with_doctest),
        "--private must not change what a gate reads"
    );
    assert_eq!(a.no_doctest, vec!["three".to_string()]);
    assert!(a.undocumented.is_empty());
}

#[test]
fn broken_links_are_reported_and_good_ones_are_not() {
    let res = resolve(&[(
        &[],
        "demo.lu",
        "/// Links to [one] and to [nosuchthing].\npub fn one() -> int { 1 }\n",
    )]);
    let docs = wolf_query::doc_package(&res, false);
    let diags = wolf_query::resolve_links(&docs, &res);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code.as_str(), "W1501");
    assert!(
        diags[0].message.contains("nosuchthing"),
        "{}",
        diags[0].message
    );
    assert_eq!(diags[0].severity, wolf_diag::Severity::Warning);
}

#[test]
fn the_broken_link_warning_is_a_reviewed_artifact() {
    // W1501's committed fixture: rendered, so the reader-facing prose is
    // reviewed like every other diagnostic (the diag-catalog rule).
    let src = "/// Links to [one] and to [nosuchthing].\npub fn one() -> int { 1 }\n";
    let res = resolve(&[(&[], "demo.lu", src)]);
    let docs = wolf_query::doc_package(&res, false);
    let diags = wolf_query::resolve_links(&docs, &res);
    let mut sources = wolf_diag::Sources::new();
    for unit in &res.package.files {
        sources.add(unit.raw.file, "demo.lu".to_string(), &unit.raw.src);
    }
    use wolf_diag::Reporter;
    let mut reporter = wolf_diag::HumanReporter::new(&sources, wolf_diag::RenderOptions::default());
    for d in &diags {
        reporter.report(d);
    }
    insta::assert_snapshot!("w1501_broken_intra_doc_link", reporter.take_output());
}

#[test]
fn markdown_links_and_checkboxes_are_not_intra_doc_links() {
    let res = resolve(&[(
        &[],
        "demo.lu",
        "/// See [the site](https://example.org), and `[not.a.link]`, and [ ].\n\
         /// A fenced [alsonot] does not count:\n\
         /// ```text\n\
         /// [neither]\n\
         /// ```\n\
         pub fn one() -> int { 1 }\n",
    )]);
    let docs = wolf_query::doc_package(&res, false);
    let links = &docs.modules[0].items[0].doc.as_ref().expect("doc").links;
    assert_eq!(links, &vec!["alsonot".to_string()], "{links:?}");
}

#[test]
fn the_site_is_deterministic_and_the_index_is_the_schema() {
    let res = resolve(&[(&[], "demo.lu", SRC)]);
    let docs = wolf_query::doc_package(&res, false);
    let opts = wolf_doc::Options {
        private: false,
        deps: vec![(
            "util".to_string(),
            "demo/util".to_string(),
            "1.2.0".to_string(),
        )],
        title: "demo".to_string(),
    };
    let a = wolf_doc::render(&docs, &opts);
    let b = wolf_doc::render(&docs, &opts);
    assert_eq!(
        a.files.keys().collect::<Vec<_>>(),
        b.files.keys().collect::<Vec<_>>()
    );
    for (k, v) in &a.files {
        assert_eq!(v, &b.files[k], "{k} is not byte-stable");
    }
    let json = String::from_utf8(a.files["index.json"].clone()).expect("utf-8");
    insta::assert_snapshot!("index_json_schema", json);
}

#[test]
fn a_module_page_needs_no_javascript() {
    let res = resolve(&[(&[], "demo.lu", SRC)]);
    let docs = wolf_query::doc_package(&res, false);
    let site = wolf_doc::render(&docs, &wolf_doc::Options::default());
    let page = String::from_utf8(site.files["module.html"].clone()).expect("utf-8");
    assert!(!page.contains("<script"), "{page}");
    assert!(
        !page.contains("http://") && !page.contains("https://"),
        "no network:\n{page}"
    );
    insta::assert_snapshot!("module_page", page);
}

#[test]
fn doctests_are_named_stably() {
    let res = resolve(&[
        (&[], "demo.lu", SRC),
        (
            &["inner"],
            "inner.lu",
            "//! member: true\n\n/// Documented.\n///\n/// ```wolf\n/// inner.four() == 4\n/// ```\npub fn four() -> int { 4 }\n",
        ),
    ]);
    let docs = wolf_query::doc_package(&res, false);
    let names: Vec<String> = wolf_doc::doctests(&docs)
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert_eq!(
        names,
        vec![
            "(root)#0".to_string(),
            "one#0".to_string(),
            "two#0".to_string(),
        ],
        "an unimported module is not in the package; {names:?}"
    );
}

#[test]
fn directive_lines_are_machinery_not_prose() {
    // A module header's `check:`/`phase:`/`member:` lines are read by the
    // toolchain and never printed as documentation.
    let res = resolve(&[(
        &[],
        "demo.lu",
        "//! check: pass\n//! phase: run\n//! member: true\n//!\n//! Real prose.\n\npub fn one() -> int { 1 }\n",
    )]);
    let docs = wolf_query::doc_package(&res, false);
    let doc = docs.modules[0].doc.as_ref().expect("prose survives");
    assert_eq!(doc.text, "Real prose.");
    // A header of nothing but directives is no documentation at all.
    let res = resolve(&[(
        &[],
        "demo.lu",
        "//! phase: run\n\npub fn one() -> int { 1 }\n",
    )]);
    let docs = wolf_query::doc_package(&res, false);
    assert!(docs.modules[0].doc.is_none());
}
