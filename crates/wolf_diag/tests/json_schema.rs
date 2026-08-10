//! Compatibility test for diag-schema version 1 (`diag-schema.md`).
//!
//! Two directions: (a) the committed fixture lines still parse and
//! carry every schema field with the right type — a schema change that
//! breaks old consumers breaks this test; (b) what `render_json_line`
//! emits today round-trips through a JSON parser into the same shape.

use std::path::Path;
use wolf_diag::{
    Applicability, DIAG_SCHEMA_VERSION, Diagnostic, Suggestion, codes, render_json_line,
};
use wolf_span::{SourceMap, Span};

/// Field-check one schema-v1 object.
fn check_line(v: &serde_json::Value) {
    assert_eq!(v["diag_schema"], DIAG_SCHEMA_VERSION);
    let code = v["code"].as_str().expect("code is a string");
    assert!(code.len() == 5 && (code.starts_with('E') || code.starts_with('W')));
    assert!(matches!(
        v["severity"].as_str().expect("severity"),
        "error" | "warning"
    ));
    v["message"].as_str().expect("message is a string");
    let span_obj = |s: &serde_json::Value| {
        s["file"].as_u64().expect("file index");
        let sp = s["span"].as_array().expect("span array");
        assert_eq!(sp.len(), 2);
        let (lo, hi) = (sp[0].as_u64().expect("lo"), sp[1].as_u64().expect("hi"));
        assert!(lo <= hi, "span not half-open ordered");
        s["label"].as_str().expect("label is a string");
    };
    span_obj(&v["primary"]);
    for s in v["secondary"].as_array().expect("secondary array") {
        span_obj(s);
    }
    for n in v["notes"].as_array().expect("notes array") {
        n.as_str().expect("note is a string");
    }
    for s in v["suggestions"].as_array().expect("suggestions array") {
        s["message"].as_str().expect("suggestion message");
        assert!(matches!(
            s["applicability"].as_str().expect("applicability"),
            "machine-applicable" | "maybe" | "has-placeholders"
        ));
        for e in s["edits"].as_array().expect("edits array") {
            e["file"].as_u64().expect("edit file");
            assert_eq!(e["span"].as_array().expect("edit span").len(), 2);
            e["replacement"].as_str().expect("replacement");
        }
    }
    // `row_diff` is optional within v1 (added by s15; consumers ignore
    // unknown keys). When present, it is the structural shape.
    if let Some(rd) = v.get("row_diff")
        && !rd.is_null()
    {
        for t in rd["missing"].as_array().expect("missing array") {
            t.as_str().expect("missing tag is a string");
        }
        for t in rd["extra"].as_array().expect("extra array") {
            t.as_str().expect("extra tag is a string");
        }
    }
    // `files` is optional within v1 (added for the wolf-lsp harness's
    // secondary-file-table request): the index→path table, in
    // source-map intern order. When present, every span's file index
    // must resolve through it.
    if let Some(files) = v.get("files")
        && !files.is_null()
    {
        let files = files.as_array().expect("files array");
        for f in files {
            f.as_str().expect("file path is a string");
        }
        let in_table = |s: &serde_json::Value| {
            let idx = s["file"].as_u64().expect("file index") as usize;
            assert!(idx < files.len(), "span file {idx} resolves in `files`");
        };
        in_table(&v["primary"]);
        for s in v["secondary"].as_array().expect("secondary array") {
            in_table(s);
        }
        for s in v["suggestions"].as_array().expect("suggestions array") {
            for e in s["edits"].as_array().expect("edits array") {
                in_table(e);
            }
        }
    }
}

#[test]
fn committed_fixture_still_parses() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/diag-schema-v1.jsonl");
    let text = std::fs::read_to_string(fixture).expect("read fixture");
    let mut n = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: serde_json::Value = serde_json::from_str(line).expect("fixture line parses");
        check_line(&v);
        n += 1;
    }
    assert!(n >= 3, "fixture covers several shapes");
}

#[test]
fn emitted_lines_conform() {
    let mut sm = SourceMap::new();
    let f = sm.intern(Path::new("t.lu"));
    let d = Diagnostic::error(codes::E0102, Span::new(f, 3, 9), "this string never closes")
        .with_label("it opens here and runs to the end of the line")
        .with_secondary(Span::new(f, 0, 3), "in this binding")
        .with_note("a `\"…\"` string must close before the line ends.")
        .with_suggestion(Suggestion::new(
            "close the string before the line ends",
            vec![(Span::new(f, 9, 9), "\"".to_string())],
            Applicability::Maybe,
        ));
    let line = render_json_line(&d);
    let v: serde_json::Value = serde_json::from_str(&line).expect("emitted line parses");
    check_line(&v);
    assert_eq!(v["code"], "E0102");
    assert_eq!(v["primary"]["span"][0], 3);
    assert_eq!(v["suggestions"][0]["applicability"], "maybe");
    assert_eq!(v["suggestions"][0]["edits"][0]["replacement"], "\"");
}

#[test]
fn emitted_lines_with_a_file_table_conform_and_resolve_every_span() {
    let mut sm = SourceMap::new();
    let entry = sm.intern(Path::new("resolve/dupdef/main.lu"));
    let sibling = sm.intern(Path::new("resolve/dupdef/twice.lu"));
    // The cross-file shape the table exists for: primary in the
    // sibling, secondary in the entry.
    let d = Diagnostic::error(
        codes::E0302,
        Span::new(sibling, 15, 21),
        "`helper` is defined twice in this module",
    )
    .with_label("defined again here")
    .with_secondary(Span::new(entry, 3, 9), "first defined here");
    let files: Vec<String> = sm
        .paths()
        .map(|p| p.display().to_string().replace('\\', "/"))
        .collect();
    let line = wolf_diag::render_json_line_with_files(&d, &files);
    let v: serde_json::Value = serde_json::from_str(&line).expect("emitted line parses");
    check_line(&v);
    // Same object, same version — `files` is additive within v1.
    assert_eq!(v["diag_schema"], DIAG_SCHEMA_VERSION);
    assert_eq!(
        v["files"],
        serde_json::json!(["resolve/dupdef/main.lu", "resolve/dupdef/twice.lu"])
    );
    // Intern order is the index: primary's file 1 is the sibling.
    assert_eq!(v["primary"]["file"], 1);
    assert_eq!(v["secondary"][0]["file"], 0);
    // Without the table the line is byte-identical minus the member.
    assert!(line.starts_with(&render_json_line(&d)[..render_json_line(&d).len() - 1]));
}
