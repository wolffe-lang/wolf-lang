//! Diagnostic types and rendering.
//!
//! Contract: structured diagnostics behind a reporter interface (s10):
//! error codes, labeled spans, Elm-voice rendering (RFC-1644 layout),
//! cascade suppression, and a JSON reporter for tooling. Every diagnostic
//! the compiler can emit gets a reviewed snapshot (s01 harness hook).
//! Depends only on `wolf_span`.
//!
//! s07 scope: the *structured values* only. [`Diagnostic`] carries a
//! stable code, a severity, a primary span, a message, and secondary
//! labeled spans (notes). [`Diagnostics`] is the minimal sink phases
//! push into. Rendering, cascade suppression, and the JSON reporter
//! arrive in s10 — nothing here should grow ahead of that.

use wolf_span::Span;

/// How bad it is. s10 may add lints/levels; error vs. warning is enough
/// for the lexer (which currently only errors).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Error,
    Warning,
}

/// One structured diagnostic. `code` is a stable identifier from the
/// diagnostic catalog (e.g. `"E0102"`); codes participate in the
/// differential protocol (spec/06 [proto.record.diag]), messages never do.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub span: Span,
    pub message: String,
    /// Secondary labeled spans ("the margin is set here").
    pub notes: Vec<(Span, String)>,
}

impl Diagnostic {
    pub fn error(code: &'static str, span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: Severity::Error,
            span,
            message: message.into(),
            notes: Vec::new(),
        }
    }

    pub fn warning(code: &'static str, span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: Severity::Warning,
            span,
            message: message.into(),
            notes: Vec::new(),
        }
    }

    /// Attach a secondary labeled span (builder-style).
    #[must_use]
    pub fn with_note(mut self, span: Span, label: impl Into<String>) -> Self {
        self.notes.push((span, label.into()));
        self
    }
}

/// The sink phases push diagnostics into. Deliberately just an ordered
/// collector today; s10 puts the reporter interface behind it.
#[derive(Default, Debug, Clone)]
pub struct Diagnostics {
    diags: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, d: Diagnostic) {
        self.diags.push(d);
    }

    pub fn has_errors(&self) -> bool {
        self.diags.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn len(&self) -> usize {
        self.diags.len()
    }

    pub fn is_empty(&self) -> bool {
        self.diags.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
        self.diags.iter()
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.diags
    }
}

impl<'a> IntoIterator for &'a Diagnostics {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use wolf_span::SourceMap;

    #[test]
    fn collector_orders_and_classifies() {
        let mut sm = SourceMap::new();
        let f = sm.intern(Path::new("t.lu"));
        let mut sink = Diagnostics::new();
        assert!(sink.is_empty());
        sink.push(Diagnostic::warning("W0001", Span::new(f, 0, 1), "first"));
        assert!(!sink.has_errors());
        sink.push(
            Diagnostic::error("E0102", Span::new(f, 2, 5), "second")
                .with_note(Span::new(f, 0, 1), "opened here"),
        );
        assert!(sink.has_errors());
        assert_eq!(sink.len(), 2);
        let v = sink.into_vec();
        assert_eq!(v[0].code, "W0001");
        assert_eq!(v[1].code, "E0102");
        assert_eq!(v[1].notes.len(), 1);
    }
}
