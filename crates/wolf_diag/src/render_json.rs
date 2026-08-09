//! The machine format: one JSON object per diagnostic, one per line.
//!
//! This is the LSP/editor contract before the LSP exists (s52 consumes
//! it), and the corpus runner's structured `fail(E…)` matching feeds on
//! it. The schema is versioned (`"diag_schema": 1`) and documented in
//! `crates/wolf_diag/diag-schema.md`; `tests/json_schema.rs` holds the
//! compatibility test. This format is a *superset* of the differential
//! protocol's diagnostic record (spec/06 `[proto.record.diag]`:
//! `{code, span, severity}`) — conform-run's stdout record stays
//! minimal, this rides on stderr.
//!
//! Serialization is hand-rolled (wolf_diag depends only on wolf_span by
//! law); field order is fixed and part of the format's determinism.

use crate::{Applicability, Diagnostic, LabeledSpan, Suggestion};

/// Current schema version, emitted as `diag_schema` on every line.
pub const DIAG_SCHEMA_VERSION: u32 = 1;

/// Render `d` as its one-line JSON object (no trailing newline).
pub fn render_json_line(d: &Diagnostic) -> String {
    let mut s = String::with_capacity(256);
    s.push_str("{\"diag_schema\":");
    s.push_str(&DIAG_SCHEMA_VERSION.to_string());
    s.push_str(",\"code\":");
    string(&mut s, d.code.as_str());
    s.push_str(",\"severity\":");
    string(&mut s, d.severity.as_str());
    s.push_str(",\"message\":");
    string(&mut s, &d.message);
    s.push_str(",\"primary\":");
    labeled_span(&mut s, &d.primary);
    s.push_str(",\"secondary\":[");
    for (i, sec) in d.secondary.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        labeled_span(&mut s, sec);
    }
    s.push_str("],\"notes\":[");
    for (i, note) in d.notes.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        string(&mut s, note);
    }
    s.push_str("],\"suggestions\":[");
    for (i, sugg) in d.suggestions.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        suggestion(&mut s, sugg);
    }
    s.push(']');
    // Optional within schema v1 (consumers ignore unknown keys): the
    // structural row diff of E06xx diagnostics — tags only, never
    // whole rows (s15/D30).
    if let Some(rd) = &d.row_diff {
        s.push_str(",\"row_diff\":{\"missing\":[");
        for (i, t) in rd.missing.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            string(&mut s, t);
        }
        s.push_str("],\"extra\":[");
        for (i, t) in rd.extra.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            string(&mut s, t);
        }
        s.push_str("]}");
    }
    s.push('}');
    s
}

fn labeled_span(s: &mut String, ls: &LabeledSpan) {
    s.push_str("{\"file\":");
    s.push_str(&ls.span.file.index().to_string());
    s.push_str(",\"span\":[");
    s.push_str(&ls.span.lo.to_string());
    s.push(',');
    s.push_str(&ls.span.hi.to_string());
    s.push_str("],\"label\":");
    string(s, &ls.label);
    s.push('}');
}

fn suggestion(s: &mut String, sugg: &Suggestion) {
    s.push_str("{\"message\":");
    string(s, &sugg.message);
    s.push_str(",\"applicability\":");
    string(
        s,
        match sugg.applicability {
            Applicability::MachineApplicable => "machine-applicable",
            Applicability::Maybe => "maybe",
            Applicability::HasPlaceholders => "has-placeholders",
        },
    );
    s.push_str(",\"edits\":[");
    for (i, (span, replacement)) in sugg.edits.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str("{\"file\":");
        s.push_str(&span.file.index().to_string());
        s.push_str(",\"span\":[");
        s.push_str(&span.lo.to_string());
        s.push(',');
        s.push_str(&span.hi.to_string());
        s.push_str("],\"replacement\":");
        string(s, replacement);
        s.push('}');
    }
    s.push_str("]}");
}

/// JSON string literal with full escaping.
fn string(s: &mut String, text: &str) {
    s.push('"');
    for c in text.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                s.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => s.push(c),
        }
    }
    s.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Diagnostic, Suggestion, codes};
    use std::path::Path;
    use wolf_span::{SourceMap, Span};

    #[test]
    fn escaping_is_sound() {
        let mut s = String::new();
        string(&mut s, "a\"b\\c\nd\te\u{1}");
        assert_eq!(s, r#""a\"b\\c\nd\te\u0001""#);
    }

    #[test]
    fn line_shape() {
        let mut sm = SourceMap::new();
        let f = sm.intern(Path::new("t.lu"));
        let d = Diagnostic::error(codes::E0209, Span::new(f, 2, 4), "no negative indexing")
            .with_label("here")
            .with_note("use `^`.")
            .with_suggestion(Suggestion::new(
                "replace `-` with `^`",
                vec![(Span::new(f, 2, 3), "^".to_string())],
                crate::Applicability::MachineApplicable,
            ));
        let line = render_json_line(&d);
        assert!(line.starts_with("{\"diag_schema\":1,\"code\":\"E0209\""));
        assert!(!line.contains('\n'));
        assert!(line.contains("\"applicability\":\"machine-applicable\""));
        assert!(line.ends_with('}'));
    }
}
