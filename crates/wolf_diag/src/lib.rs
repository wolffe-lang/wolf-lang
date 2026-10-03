//! The diagnostics engine (s10, D22).
//!
//! Structured diagnostic values behind a reporter interface: every phase
//! pushes [`Diagnostic`]s into a per-run [`Diagnostics`] sink; nothing in
//! the compiler ever formats its own error text inline (`cargo xtask
//! print-gate` enforces this). Rendering is pluggable behind [`Reporter`]:
//! the CLI human format ([`render_human`], RFC-1644 layout, Elm voice —
//! see `VOICE.md`), the JSON line format ([`render_json_line`], schema in
//! `diag-schema.md`), and the snapshot format (the human renderer with
//! color off — snapshots are always plain).
//!
//! # Codes
//!
//! Every diagnostic carries a [`Code`] from the registry in
//! [`registry`] — one rigid `code!` entry per code, each with a one-line
//! summary and a mandatory extended explanation (`wolf --explain E0101`).
//! [`Code`] is constructible *only* by the registry (private field), so a
//! diagnostic without a registered code is a compile error, and every
//! code has ≥1 reviewed snapshot fixture (the s01 hook, gated in CI).
//! Retired codes retire their numbers; numbers are never reused.
//!
//! # Cascade suppression
//!
//! One root cause ≈ one diagnostic. The sink carries a suppression-span
//! set fed by the parser: whenever recovery skips tokens into an error
//! node, that region is suppressed, and any *later* diagnostic whose
//! primary span falls inside a suppressed region is dropped on push
//! (unless force-emitted via [`Diagnostics::push_forced`]). The report
//! that *names* the wreck is always emitted before the region is
//! suppressed, so the root cause survives and the echoes drop.
//!
//! **The `<error>` convention (for s13 type checking):** an error node in
//! the tree types as `<error>`, and `<error>` unifies with every type
//! *silently* — no "expected int, found `<error>`" cascades. Sema seeds
//! its sink with the parser's suppression regions
//! ([`Diagnostics::suppress`]) so even diagnostics computed *about* a
//! wrecked region stay quiet; only force-emitted diagnostics pierce it.
//!
//! # Ordering
//!
//! Reported output is deterministic: [`sort_diagnostics`] orders by
//! (file, span, code), so runs are diffable and snapshots stable
//! regardless of phase interleaving.

use wolf_span::Span;

pub mod lint;
pub mod registry;
mod render;
mod render_json;
pub mod suggest;

pub use registry::{Code, CodeInfo, codes, explain};
pub use render::{RenderOptions, Sources, render_human};
pub use render_json::{DIAG_SCHEMA_VERSION, render_json_line, render_json_line_with_files};

/// How bad it is. An error rejects meaning and cannot be configured
/// away; a warning flags a legal-but-inadvisable program and is
/// leveled — allow/warn/deny — by the [`lint`] machinery (s67,
/// spec/01 §9).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    /// The lowercase keyword used in headers and machine formats.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// A source span with the sentence fragment the renderer prints next to
/// its underline. The primary label says *what* went wrong there;
/// secondary labels say *why* ("the margin is set here").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LabeledSpan {
    pub span: Span,
    /// May be empty — the underline still points, it just says nothing.
    pub label: String,
}

impl LabeledSpan {
    pub fn new(span: Span, label: impl Into<String>) -> LabeledSpan {
        LabeledSpan {
            span,
            label: label.into(),
        }
    }
}

/// Can a tool apply this suggestion without asking?
///
/// The currency `wolf fix` (D34) and the LSP quick-fix path (s52) spend:
/// only `MachineApplicable` edits are applied unattended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Applicability {
    /// The edit is certainly what the user meant; applying it cannot
    /// change a correct program's meaning.
    MachineApplicable,
    /// A plausible fix that needs a human eye.
    Maybe,
    /// The replacement contains placeholder text the user must fill in.
    HasPlaceholders,
}

/// A concrete fix: prose plus the byte-exact edits that implement it.
/// Each edit replaces `span` with the replacement string (a zero-width
/// span is an insertion).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Suggestion {
    pub message: String,
    pub edits: Vec<(Span, String)>,
    pub applicability: Applicability,
}

impl Suggestion {
    pub fn new(
        message: impl Into<String>,
        edits: Vec<(Span, String)>,
        applicability: Applicability,
    ) -> Suggestion {
        Suggestion {
            message: message.into(),
            edits,
            applicability,
        }
    }
}

/// A structural error-row diff (s15, D30): the tags one row lacks and
/// the tags it has over — never whole rows. Rides the JSON format
/// structurally (`row_diff`, optional) so tools need not parse prose.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct RowDiff {
    /// Tags required but absent from the target row, source-rendered
    /// (`Io(IoError)`).
    pub missing: Vec<String>,
    /// Tags present but not required, source-rendered.
    pub extra: Vec<String>,
}

/// One structured diagnostic. `code` participates in the differential
/// protocol (spec/06 `[proto.record.diag]`); messages never do — wording
/// is a per-implementation quality concern governed by `VOICE.md`.
#[derive(Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    /// The headline: one full sentence, per the voice guide.
    pub message: String,
    /// Where it went wrong (underlined `^^^` with its label).
    pub primary: LabeledSpan,
    /// Why it went wrong (underlined `---`, possibly in other files).
    pub secondary: Vec<LabeledSpan>,
    /// Free-standing prose after the code frame (`= note: …`).
    pub notes: Vec<String>,
    /// Concrete fixes, rendered as prose + an edited-line preview.
    pub suggestions: Vec<Suggestion>,
    /// The structural row diff, when this diagnostic is about error
    /// rows (E06xx).
    pub row_diff: Option<RowDiff>,
    /// The binding this diagnostic is *about*, and the region over
    /// which that spelling is the same binding (s154, wolf-lang#325).
    /// A warning carries the scope it reasons over (a function body);
    /// an error carries the use site it fired on. It exists so one
    /// pass can tell that two diagnostics from different phases are
    /// arguing about the same name — see [`suppress_mode_shadowed`].
    /// Never rendered.
    pub subject: Option<(String, Span)>,
}

// Manual Debug: `row_diff` appears only when present, so the many
// committed Debug-format snapshots of row-less diagnostics stay
// byte-stable (snapshots are reviewed artifacts).
impl std::fmt::Debug for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Diagnostic");
        s.field("code", &self.code)
            .field("severity", &self.severity)
            .field("message", &self.message)
            .field("primary", &self.primary)
            .field("secondary", &self.secondary)
            .field("notes", &self.notes)
            .field("suggestions", &self.suggestions);
        if let Some(rd) = &self.row_diff {
            s.field("row_diff", rd);
        }
        s.finish()
    }
}

impl Diagnostic {
    pub fn new(
        code: Code,
        severity: Severity,
        span: Span,
        message: impl Into<String>,
    ) -> Diagnostic {
        Diagnostic {
            code,
            severity,
            message: message.into(),
            primary: LabeledSpan::new(span, ""),
            secondary: Vec::new(),
            notes: Vec::new(),
            suggestions: Vec::new(),
            row_diff: None,
            subject: None,
        }
    }

    pub fn error(code: Code, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(code, Severity::Error, span, message)
    }

    pub fn warning(code: Code, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(code, Severity::Warning, span, message)
    }

    /// The primary span (convenience — the protocol's `span` field).
    pub fn span(&self) -> Span {
        self.primary.span
    }

    /// Label the primary span (builder-style).
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Diagnostic {
        self.primary.label = label.into();
        self
    }

    /// Attach a secondary labeled span (the *why* locus).
    #[must_use]
    pub fn with_secondary(mut self, span: Span, label: impl Into<String>) -> Diagnostic {
        self.secondary.push(LabeledSpan::new(span, label));
        self
    }

    /// Attach a free-standing note.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Diagnostic {
        self.notes.push(note.into());
        self
    }

    /// Attach a suggestion.
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: Suggestion) -> Diagnostic {
        self.suggestions.push(suggestion);
        self
    }

    /// Attach the structural row diff (E06xx).
    #[must_use]
    pub fn with_row_diff(mut self, diff: RowDiff) -> Diagnostic {
        self.row_diff = Some(diff);
        self
    }

    /// Record the binding this diagnostic is about (s154, #325): the
    /// name, and the region over which that spelling is one binding.
    #[must_use]
    pub fn about(mut self, name: impl Into<String>, scope: Span) -> Diagnostic {
        self.subject = Some((name.into(), scope));
        self
    }

    /// [`Diagnostic::about`] for a name the caller may not have (a
    /// place expression that is not a bare binding): `None` records
    /// nothing.
    #[must_use]
    pub fn about_opt(self, name: Option<String>, scope: Span) -> Diagnostic {
        match name {
            Some(n) => self.about(n, scope),
            None => self,
        }
    }
}

/// W1002 stands down where a mode ERROR already spoke about the same
/// parameter (s154, wolf-lang#325).
///
/// W1002 ("this `mut` parameter is never written") is a resolve-rung
/// lint with a flat, syntactic write scan; E0804 (the receiver mode is
/// not spelled at the call site) and E1014 (a write reaches a `read`
/// parameter) are later, typed refusals. When both fire on one
/// parameter they disagree about whether a line writes it — and only
/// the error is right, because the write the lint could not see is
/// exactly the one the error is naming. Following W1002's fix-it there
/// (drop the `mut`) walks the reader AWAY from the fix E0804 asks for.
/// Two diagnostics that disagree about whether a line writes are one
/// too many: the warning is dropped for that parameter.
///
/// s184 (wolf-lang#464): the same holds for E1001 at a `mut`
/// parameter's move out of it — the one E1001 that names a subject. A
/// move out of the parameter IS a write to the caller's place, and the
/// refusal asks for a store back; "never written" beside it is false.
pub fn suppress_mode_shadowed(diags: &mut Vec<Diagnostic>) {
    let mode_errors: Vec<(String, Span)> = diags
        .iter()
        .filter(|d| matches!(d.code.as_str(), "E0804" | "E1014" | "E1001"))
        .filter_map(|d| d.subject.as_ref().map(|(n, s)| (n.clone(), *s)))
        .collect();
    if mode_errors.is_empty() {
        return;
    }
    let shadowed = |d: &Diagnostic| {
        let Some((name, scope)) = &d.subject else {
            return false;
        };
        mode_errors.iter().any(|(n, use_site)| {
            n == name
                && use_site.file == scope.file
                && use_site.lo >= scope.lo
                && use_site.hi <= scope.hi
        })
    };
    diags.retain(|d| d.code.as_str() != "W1002" || !shadowed(d));
}

/// A deny-promoted W1002 waits for the mem rung (s185, wolf-lang#469).
///
/// W1002's scan runs at the resolve rung; the refusals that retire it
/// ([`suppress_mode_shadowed`]) run later — E0804 and E1014 at
/// typecheck, #464's at-return E1001 at mem. Under `--deny-warnings` a
/// front end that stopped on the promoted W1002 at resolve never
/// produced the refusal that retires it, and reported the retired
/// wrong diagnosis with its drop-the-`mut` help instead. So a promoted
/// W1002 does not stop a front-end rung by itself: the rungs run on,
/// the retirement is applied, and the last front-end rung stops on any
/// W1002 still standing — the same program is refused either way.
pub fn waits_for_mem(d: &Diagnostic) -> bool {
    d.severity == Severity::Error && d.code.as_str() == "W1002"
}

/// Does an error stop a front-end rung BEFORE the last one? Every
/// error does except a promoted W1002 ([`waits_for_mem`]).
pub fn stops_early(diags: &[Diagnostic]) -> bool {
    diags
        .iter()
        .any(|d| d.severity == Severity::Error && !waits_for_mem(d))
}

/// The phase a code is born in, read off its family (the registry's
/// numbering, `registry.rs` docs): `E01xx` the lexer (0); `E000x` and
/// `E02xx` the parser (1); `E03xx` name resolution (2); every later
/// family (3). Warnings read the same way by number. This is the
/// phase half of ruling #28's order ([`sort_diagnostics`]).
pub fn phase_rank(code: Code) -> u8 {
    let n: u32 = code.as_str()[1..].parse().unwrap_or(u32::MAX);
    match n {
        100..=199 => 0,
        0..=99 | 200..=299 => 1,
        300..=399 => 2,
        _ => 3,
    }
}

/// Deterministic report order, and ruling #28 (wolf-lang#377,
/// `[proto.record.first]`): file, then the earliest byte offset, then
/// the earlier phase at the same offset (lex, parse, resolve, later —
/// [`phase_rank`]); past the ruling, the shorter span, then the code.
/// The first ERROR in this order is the code a refusal carries on every
/// lane ([`first_error`]). Stable for full ties.
pub fn sort_diagnostics(diags: &mut [Diagnostic]) {
    diags.sort_by_key(report_order);
}

fn report_order(d: &Diagnostic) -> (wolf_span::FileId, u32, u8, u32, &'static str) {
    let span = d.primary.span;
    (span.file, span.lo, phase_rank(d.code), span.hi, d.code.as_str())
}

/// The diagnostic a refusal carries: the first ERROR in
/// [`sort_diagnostics`]'s order, whatever order `diags` is in.
pub fn first_error(diags: &[Diagnostic]) -> Option<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .min_by_key(|d| report_order(d))
}

/// Collapse repeated teach notes *destructively*: within one sorted
/// batch, a note that already appeared verbatim on an earlier
/// diagnostic of the same code is dropped from the later ones.
///
/// Prefer [`HumanReporter`], which does the same grouping at render
/// time and therefore leaves the diagnostic values whole — a JSON or
/// LSP consumer wants every note on every item, and a phase that
/// deletes notes from the data cannot tell the two audiences apart.
/// This remains for a caller that owns its own text rendering.
pub fn dedup_notes(diags: &mut [Diagnostic]) {
    let mut seen: std::collections::HashSet<(&'static str, String)> =
        std::collections::HashSet::new();
    for d in diags {
        let code = d.code.as_str();
        d.notes.retain(|n| seen.insert((code, n.clone())));
    }
}

/// A rollback point for [`Diagnostics`] (speculative parsing rewinds
/// diagnostics and suppressions together).
#[derive(Clone, Copy, Debug)]
pub struct DiagMark {
    diags: usize,
    suppressed: usize,
}

/// The per-run sink phases push diagnostics into, with the cascade
/// suppression described in the crate docs.
#[derive(Default, Debug, Clone)]
pub struct Diagnostics {
    diags: Vec<Diagnostic>,
    /// Error-node regions: later diagnostics inside them are dropped.
    suppressed: Vec<Span>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Is `span` inside a suppressed region? Containment is strict at
    /// the region's end: a zero-width span *at* the boundary (the sync
    /// point recovery stopped at) is outside — follow-up diagnostics at
    /// the resume position must survive.
    fn is_suppressed(&self, span: Span) -> bool {
        self.suppressed
            .iter()
            .any(|r| r.file == span.file && span.lo >= r.lo && span.hi <= r.hi && (span.lo < r.hi))
    }

    /// Push, unless the primary span falls inside a suppressed region
    /// (cascade suppression — see the crate docs).
    pub fn push(&mut self, d: Diagnostic) {
        if self.is_suppressed(d.primary.span) {
            return;
        }
        // One position, one expectation miss. E0201 is the parser's
        // workhorse ("expected X here"), and recovery routinely asks
        // twice about the same character: `spawn {` used to answer with
        // both "expected `proc` after `spawn`" and "expected the line
        // to end after the expression" under one caret — two sentences,
        // one mistake, and the reader has to guess which to fix. Only
        // E0201 is folded, and only against its own span: codes that
        // report a *set* of facts at one span (an impl missing two
        // associated types, say) each still speak, and `push_forced`
        // pierces this as it pierces suppression.
        if d.code == codes::E0201
            && self
                .diags
                .iter()
                .any(|p| p.code == d.code && p.primary.span == d.primary.span)
        {
            return;
        }
        self.diags.push(d);
    }

    /// Push unconditionally, piercing suppression (for diagnostics that
    /// are *about* a wrecked region and still worth reading).
    pub fn push_forced(&mut self, d: Diagnostic) {
        self.diags.push(d);
    }

    /// Suppress future diagnostics whose primary span falls inside
    /// `region` (the parser feeds error-node extents here).
    pub fn suppress(&mut self, region: Span) {
        if !region.is_empty() {
            self.suppressed.push(region);
        }
    }

    /// The suppression set accumulated so far (s13 seeds sema's sink
    /// with the parser's regions).
    pub fn suppressed_regions(&self) -> &[Span] {
        &self.suppressed
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

    /// A rollback point (see [`DiagMark`]).
    pub fn mark(&self) -> DiagMark {
        DiagMark {
            diags: self.diags.len(),
            suppressed: self.suppressed.len(),
        }
    }

    /// Rewind to `mark`, dropping diagnostics and suppressions pushed
    /// since.
    pub fn rollback(&mut self, mark: DiagMark) {
        self.diags.truncate(mark.diags);
        self.suppressed.truncate(mark.suppressed);
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.diags
    }

    /// Split into (diagnostics, suppressed regions).
    pub fn into_parts(self) -> (Vec<Diagnostic>, Vec<Span>) {
        (self.diags, self.suppressed)
    }
}

impl<'a> IntoIterator for &'a Diagnostics {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// The generation/reporting split: phases fill a [`Diagnostics`] sink;
/// the driver drains it through one of these. Implementations render to
/// an internal buffer (`take_output`) so compiler phases stay print-free.
pub trait Reporter {
    fn report(&mut self, d: &Diagnostic);
    /// Drain whatever the reporter accumulated.
    fn take_output(&mut self) -> String;
}

/// [`Reporter`] for the human CLI format (RFC-1644 layout).
pub struct HumanReporter<'s> {
    sources: &'s Sources,
    opts: RenderOptions,
    out: String,
    /// Teach notes already rendered in this run, keyed by (code, text).
    /// Reading the same paragraph five times teaches nothing the first
    /// reading did not — the group keeps it once and the rest stay
    /// lean. Machine consumers (`JsonReporter`) keep every note.
    taught: std::collections::HashSet<(&'static str, String)>,
}

impl<'s> HumanReporter<'s> {
    pub fn new(sources: &'s Sources, opts: RenderOptions) -> Self {
        HumanReporter {
            sources,
            opts,
            out: String::new(),
            taught: std::collections::HashSet::new(),
        }
    }
}

impl Reporter for HumanReporter<'_> {
    fn report(&mut self, d: &Diagnostic) {
        // Note-grouping is a *rendering* decision, so it lives here
        // rather than at each of the driver's many assembly points —
        // one of which is always the one that gets forgotten (the
        // typecheck batch was deduped; every warning the later phases
        // raised still repeated its paragraph per occurrence).
        let code = d.code.as_str();
        let fresh: Vec<String> = d
            .notes
            .iter()
            .filter(|n| self.taught.insert((code, (*n).clone())))
            .cloned()
            .collect();
        let rendered = if fresh.len() == d.notes.len() {
            render_human(d, self.sources, &self.opts)
        } else {
            let mut lean = d.clone();
            lean.notes = fresh;
            render_human(&lean, self.sources, &self.opts)
        };
        self.out.push_str(&rendered);
        self.out.push('\n');
    }

    fn take_output(&mut self) -> String {
        std::mem::take(&mut self.out)
    }
}

/// [`Reporter`] for the machine format: one JSON object per line
/// (`diag-schema.md`).
#[derive(Default)]
pub struct JsonReporter {
    out: String,
    files: Option<Vec<String>>,
}

impl JsonReporter {
    pub fn new() -> Self {
        Self::default()
    }

    /// A reporter that stamps every line with the `files` member — the
    /// index→path table in SourceMap intern order, so consumers can
    /// resolve spans whose `file` is not the entry (additive within
    /// schema v1).
    pub fn with_files(files: Vec<String>) -> Self {
        JsonReporter {
            out: String::new(),
            files: Some(files),
        }
    }
}

impl Reporter for JsonReporter {
    fn report(&mut self, d: &Diagnostic) {
        let line = match &self.files {
            Some(files) => render_json_line_with_files(d, files),
            None => render_json_line(d),
        };
        self.out.push_str(&line);
        self.out.push('\n');
    }

    fn take_output(&mut self) -> String {
        std::mem::take(&mut self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use wolf_span::SourceMap;

    fn file() -> wolf_span::FileId {
        let mut sm = SourceMap::new();
        sm.intern(Path::new("t.lu"))
    }

    #[test]
    fn collector_orders_and_classifies() {
        let f = file();
        let mut sink = Diagnostics::new();
        assert!(sink.is_empty());
        sink.push(Diagnostic::warning(
            codes::E0201,
            Span::new(f, 0, 1),
            "first",
        ));
        assert!(!sink.has_errors());
        sink.push(
            Diagnostic::error(codes::E0102, Span::new(f, 2, 5), "second")
                .with_secondary(Span::new(f, 0, 1), "opened here")
                .with_note("a free-standing note"),
        );
        assert!(sink.has_errors());
        assert_eq!(sink.len(), 2);
        let v = sink.into_vec();
        assert_eq!(v[0].code, "E0201");
        assert_eq!(v[1].code, codes::E0102);
        assert_eq!(v[1].secondary.len(), 1);
        assert_eq!(v[1].notes.len(), 1);
    }

    /// Ruling #28 (wolf-lang#377): the earliest byte offset wins; at the
    /// same offset the earlier phase. The four shapes the order decides:
    /// a parse error BEFORE a lex error (the old ladder answered the lex
    /// code), a lex and a parse error at one offset whose parse span is
    /// shorter (the old order answered the parse code), a resolve error
    /// against a parse error at one offset, and a later code at an
    /// earlier offset.
    #[test]
    fn first_error_is_earliest_offset_then_earlier_phase() {
        let f = file();
        let e = |c, lo, hi| Diagnostic::error(c, Span::new(f, lo, hi), "x");
        assert_eq!(phase_rank(codes::E0102), 0);
        assert_eq!(phase_rank(codes::E0008), 1);
        assert_eq!(phase_rank(codes::E0203), 1);
        assert_eq!(phase_rank(codes::E0301), 2);
        assert_eq!(phase_rank(codes::E0401), 3);
        let parse_first = [e(codes::E0102, 40, 45), e(codes::E0207, 10, 13)];
        assert_eq!(first_error(&parse_first).unwrap().code, codes::E0207);
        let same_offset = [e(codes::E0203, 0, 1), e(codes::E0102, 0, 2)];
        assert_eq!(first_error(&same_offset).unwrap().code, codes::E0102);
        let resolve_tie = [e(codes::E0301, 5, 6), e(codes::E0201, 5, 9)];
        assert_eq!(first_error(&resolve_tie).unwrap().code, codes::E0201);
        let later_code_earlier = [e(codes::E0201, 9, 10), e(codes::E0401, 3, 4)];
        assert_eq!(first_error(&later_code_earlier).unwrap().code, codes::E0401);
        // Warnings never carry the refusal.
        let warned = [
            Diagnostic::warning(codes::E0201, Span::new(f, 0, 1), "w"),
            e(codes::E0203, 4, 5),
        ];
        assert_eq!(first_error(&warned).unwrap().code, codes::E0203);
        // The sort agrees with first_error.
        let mut v = same_offset.to_vec();
        sort_diagnostics(&mut v);
        assert_eq!(v[0].code, codes::E0102);
    }

    #[test]
    fn suppression_drops_cascades_but_not_boundaries() {
        let f = file();
        let mut sink = Diagnostics::new();
        // The root-cause report lands before the region is suppressed.
        sink.push(Diagnostic::error(
            codes::E0202,
            Span::new(f, 10, 11),
            "root cause",
        ));
        sink.suppress(Span::new(f, 10, 50));
        // An echo inside the wreck: dropped.
        sink.push(Diagnostic::error(
            codes::E0201,
            Span::new(f, 20, 25),
            "echo",
        ));
        // Zero-width at the region's end boundary (the sync point): kept.
        sink.push(Diagnostic::error(
            codes::E0201,
            Span::new(f, 50, 50),
            "resume point",
        ));
        // Outside entirely: kept.
        sink.push(Diagnostic::error(
            codes::E0203,
            Span::new(f, 60, 61),
            "later",
        ));
        // Force-emitted inside: kept.
        sink.push_forced(Diagnostic::error(
            codes::E0206,
            Span::new(f, 30, 31),
            "pierces",
        ));
        let v: Vec<&'static str> = sink.iter().map(|d| d.code.as_str()).collect();
        assert_eq!(v, ["E0202", "E0201", "E0203", "E0206"]);
    }

    #[test]
    fn mark_rolls_back_diags_and_suppressions_together() {
        let f = file();
        let mut sink = Diagnostics::new();
        sink.push(Diagnostic::error(codes::E0201, Span::new(f, 0, 1), "keep"));
        let mark = sink.mark();
        sink.push(Diagnostic::error(codes::E0202, Span::new(f, 2, 3), "drop"));
        sink.suppress(Span::new(f, 2, 9));
        sink.rollback(mark);
        assert_eq!(sink.len(), 1);
        assert!(sink.suppressed_regions().is_empty());
        // The rolled-back suppression no longer applies.
        sink.push(Diagnostic::error(codes::E0203, Span::new(f, 3, 4), "kept"));
        assert_eq!(sink.len(), 2);
    }

    #[test]
    fn deterministic_ordering() {
        let mut sm = SourceMap::new();
        let a = sm.intern(Path::new("a.lu"));
        let b = sm.intern(Path::new("b.lu"));
        let mut v = vec![
            Diagnostic::error(codes::E0203, Span::new(b, 0, 1), "b file"),
            Diagnostic::error(codes::E0202, Span::new(a, 5, 9), "a late"),
            Diagnostic::error(codes::E0201, Span::new(a, 5, 9), "a late, lower code"),
            Diagnostic::error(codes::E0208, Span::new(a, 0, 2), "a early"),
        ];
        sort_diagnostics(&mut v);
        let order: Vec<&'static str> = v.iter().map(|d| d.code.as_str()).collect();
        assert_eq!(order, ["E0208", "E0201", "E0202", "E0203"]);
    }

    /// s185 (wolf-lang#469): only a PROMOTED W1002 waits for the mem
    /// rung. Its warning form stops nothing, and every other error
    /// (promoted or not) still stops the rung that produced it.
    #[test]
    fn a_promoted_w1002_alone_waits_for_the_mem_rung() {
        let f = file();
        let lint = Diagnostic::warning(codes::W1002, Span::new(f, 0, 1), "never written");
        let mut promoted = lint.clone();
        promoted.severity = Severity::Error;
        let mut other = Diagnostic::warning(codes::E0802, Span::new(f, 2, 3), "dead arm");
        other.severity = Severity::Error;
        let hard = Diagnostic::error(codes::E1001, Span::new(f, 4, 5), "moved");

        assert!(!waits_for_mem(&lint), "a warning is not an error at all");
        assert!(waits_for_mem(&promoted));
        assert!(!waits_for_mem(&other), "only W1002 has a later retirement");
        assert!(!waits_for_mem(&hard));

        assert!(!stops_early(&[lint.clone(), promoted.clone()]));
        assert!(stops_early(&[promoted.clone(), other]));
        assert!(stops_early(&[promoted, hard]));
        assert!(!stops_early(&[lint]));
    }
}
