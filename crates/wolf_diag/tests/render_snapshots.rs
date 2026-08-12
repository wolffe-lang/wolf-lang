//! Renderer snapshots over hand-built diagnostics — the layout cases
//! the s10 contract names: multi-line primary spans, multiple secondary
//! spans, suggestion previews, tab expansion, width truncation, and the
//! mock sema shape (secondary-heavy, across two files) that proves the
//! API for s13/s18 before those clients exist. Real lexer/parser
//! diagnostics get their renderer snapshots in wolf_lex/wolf_parse
//! (the crate graph points that way).
//!
//! Snapshots are always plain: the snapshot renderer IS the human
//! renderer with `color: false` (the default).

use std::path::Path;
use wolf_diag::{
    Applicability, Diagnostic, RenderOptions, Sources, Suggestion, codes, render_human,
};
use wolf_span::{FileId, SourceMap, Span};

fn one_file(src: &str) -> (Sources, FileId) {
    let mut sm = SourceMap::new();
    let f = sm.intern(Path::new("demo.lu"));
    let mut sources = Sources::new();
    sources.add(f, "demo.lu", src.as_bytes());
    (sources, f)
}

#[test]
fn multi_line_primary_span() {
    let src = "fn f() -> int {\n    let x = (1 +\n        2 +\n        3\n    x\n}\n";
    let (sources, f) = one_file(src);
    // Primary spans the whole parenthesized region, lines 2..=4.
    let d = Diagnostic::error(
        codes::E0202,
        Span::new(f, 28, 55),
        "this `(` is never closed",
    )
    .with_label("the expression it opens ends nowhere")
    .with_note("add the closing `)`.");
    insta::assert_snapshot!(
        "multiline_primary",
        render_human(&d, &sources, &RenderOptions::default())
    );
}

#[test]
fn elided_middle_of_a_long_span() {
    let mut src = String::from("let a = (1 +\n");
    for i in 0..8 {
        src.push_str(&format!("    {i} +\n"));
    }
    src.push_str("    9\n");
    let (sources, f) = one_file(&src);
    let d = Diagnostic::error(
        codes::E0202,
        Span::new(f, 8, src.len() as u32 - 1),
        "this `(` is never closed",
    )
    .with_label("still open at the end of the file");
    insta::assert_snapshot!(
        "multiline_elided",
        render_human(&d, &sources, &RenderOptions::default())
    );
}

#[test]
fn two_secondary_spans() {
    let src = "region r {\n    let h = alloc(r)\n    freeze r\n    h.write(1)\n}\n";
    let (sources, f) = one_file(src);
    let d = Diagnostic::error(
        codes::E0201,
        Span::new(f, 49, 59),
        "this handle cannot write into a frozen region",
    )
    .with_label("write attempted here")
    .with_secondary(Span::new(f, 0, 8), "the region is created here")
    .with_secondary(Span::new(f, 36, 44), "and frozen here")
    .with_note("a frozen region is immutable for the rest of its life.");
    insta::assert_snapshot!(
        "two_secondaries",
        render_human(&d, &sources, &RenderOptions::default())
    );
}

#[test]
fn suggestion_with_edit_preview() {
    let src = "fn f() {\n    let last = s[-1]\n}\n";
    let (sources, f) = one_file(src);
    let d = Diagnostic::error(
        codes::E0209,
        Span::new(f, 26, 28),
        "wolf has no negative indexing",
    )
    .with_label("did you mean `^1`?")
    .with_suggestion(Suggestion::new(
        "index from the end: `^1`",
        vec![(Span::new(f, 26, 27), "^".to_string())],
        Applicability::MachineApplicable,
    ));
    insta::assert_snapshot!(
        "suggestion_edit_preview",
        render_human(&d, &sources, &RenderOptions::default())
    );
}

#[test]
fn tab_expansion_keeps_underlines_aligned() {
    let src = "fn f() {\n\tlet x\t= $ + 1\n}\n";
    let (sources, f) = one_file(src);
    // `$` sits after two tabs; the underline must land on it exactly.
    let d = Diagnostic::error(
        codes::E0107,
        Span::new(f, 18, 19),
        "the character `$` is not part of wolf syntax",
    )
    .with_label("no wolf token can start with this");
    let out = render_human(&d, &sources, &RenderOptions::default());
    // Alignment invariant: the caret sits in the same column as the `$`.
    let lines: Vec<&str> = out.lines().collect();
    let src_row = lines
        .iter()
        .find(|l| l.contains("$ + 1"))
        .expect("source row");
    let caret_row = lines.iter().find(|l| l.contains('^')).expect("caret row");
    assert_eq!(
        src_row.find('$').expect("dollar"),
        caret_row.find('^').expect("caret"),
        "tab expansion must keep the underline aligned:\n{out}"
    );
    insta::assert_snapshot!("tab_expansion", out);
}

#[test]
fn width_truncation_windows_long_lines() {
    let pad = "x + ".repeat(40);
    let src = format!("let v = {pad}$ + {pad}1\n");
    let (sources, f) = one_file(&src);
    let bad = 8 + pad.len() as u32;
    let d = Diagnostic::error(
        codes::E0107,
        Span::new(f, bad, bad + 1),
        "the character `$` is not part of wolf syntax",
    )
    .with_label("here, deep inside a very long line");
    insta::assert_snapshot!(
        "width_truncation",
        render_human(
            &d,
            &sources,
            &RenderOptions {
                width: 60,
                ..RenderOptions::default()
            }
        )
    );
}

/// The mock sema shape (s10 contract): secondary-heavy, three
/// secondaries across two files — proves the API and layout s13/s18
/// will lean on, before those clients exist.
#[test]
fn mock_sema_case_secondaries_across_two_files() {
    let main_src = "use geo\n\nfn area(s: geo.Shape) -> f64 {\n    s.scale(2)\n}\n";
    let geo_src = "pub struct Shape {\n    dims: f64,\n}\n\nfn scale(self, k: f64) { }\n";
    let mut sm = SourceMap::new();
    let main = sm.intern(Path::new("main.lu"));
    let geo = sm.intern(Path::new("geo.lu"));
    let mut sources = Sources::new();
    sources.add(main, "main.lu", main_src.as_bytes());
    sources.add(geo, "geo.lu", geo_src.as_bytes());
    let d = Diagnostic::error(
        codes::E0201,
        Span::new(main, 44, 54),
        "`scale` is private to module `geo`",
    )
    .with_label("called here")
    .with_secondary(
        Span::new(geo, 40, 45),
        "`scale` is declared here, without `pub`",
    )
    .with_secondary(Span::new(geo, 11, 16), "its type is public")
    .with_secondary(Span::new(main, 4, 7), "the module is imported here")
    .with_note("mark `scale` as `pub fn scale(…)` in geo.lu, or call it from inside `geo`.");
    insta::assert_snapshot!(
        "mock_sema_two_files",
        render_human(&d, &sources, &RenderOptions::default())
    );
}

#[test]
fn color_is_behind_the_flag() {
    let src = "let $ = 1\n";
    let (sources, f) = one_file(src);
    let d = Diagnostic::error(
        codes::E0107,
        Span::new(f, 4, 5),
        "the character `$` is not part of wolf syntax",
    );
    let plain = render_human(&d, &sources, &RenderOptions::default());
    assert!(!plain.contains('\x1b'), "default rendering is plain");
    let colored = render_human(
        &d,
        &sources,
        &RenderOptions {
            color: true,
            ..RenderOptions::default()
        },
    );
    assert!(colored.contains("\x1b[31m"), "errors color red");
    // Same text once the ANSI codes are stripped.
    let stripped: String = {
        let mut out = String::new();
        let mut chars = colored.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c2 in chars.by_ref() {
                    if c2 == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    };
    assert_eq!(stripped, plain);
}

/// The teach note renders once per (code, note) group, not once per
/// occurrence — a program with five offending spawns should not print
/// the channels lesson five times. Note-grouping is the reporter's job
/// (every driver path funnels through it); the reviewed shape is a
/// *run* of diagnostics, so this snapshot is the whole report.
#[test]
fn a_repeated_teach_note_renders_once_per_group() {
    use wolf_diag::{HumanReporter, Reporter};

    let src = "fn main() -> !int {\n    var n = 0\n    n = 1\n    n = 2\n    0\n}\n";
    let (sources, f) = one_file(src);
    let lesson = "task captures are copies, `imm` shares, or region moves (D14) — never mutable \
                  windows onto the parent's locals.";
    let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
    for (lo, hi, name) in [(38u32, 39u32, "first"), (48, 49, "second")] {
        reporter.report(
            &Diagnostic::error(
                codes::E1101,
                Span::new(f, lo, hi),
                format!("this task writes to `n` ({name} spawn)"),
            )
            .with_label("tasks cannot mutate captured state")
            .with_note(lesson),
        );
    }
    // A different code with the same words keeps its own copy: groups
    // are per code, never global.
    reporter.report(
        &Diagnostic::warning(
            codes::W1101,
            Span::new(f, 48, 49),
            "this write stays inside the task",
        )
        .with_note(lesson),
    );
    insta::assert_snapshot!("teach_note_grouped", reporter.take_output());
}
