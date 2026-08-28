//! The compiler-side query API for editor tooling (s52, D34/D9).
//!
//! **THIS CRATE IS THE s57 CONTRACT.** `wolf_lsp` (the protocol shim),
//! `wolf doc` (s53), and every future editor feature are clients of the
//! API below; when s57's resident daemon lands it implements *this*
//! interface and nothing above it changes. Any post-s52 change to the
//! public surface requires a paired PR against s57's plan (sprint
//! acceptance). [`CONTRACT_VERSION`] names the surface; bump it on any
//! breaking change and record the delta in the campaign closeout.
//!
//! # The contract, in five clauses
//!
//! 1. **Inputs are overlays over disk.** The host owns a set of file
//!    overlays (open editor buffers, possibly unsaved); every query
//!    reads through the overlay first and disk second. Opening, editing
//!    and closing overlays are the only mutations ([`Change`]).
//! 2. **Reads are snapshots.** [`QueryHost::snapshot`] freezes the
//!    current state; all queries run against a [`Snapshot`] and are safe
//!    to call from any number of threads concurrently.
//! 3. **Writes cancel readers** (the rust-analyzer architecture, frozen
//!    here for s57 — report 09 §Wolf's answer): [`QueryHost::apply_change`]
//!    marks every outstanding snapshot cancelled and **blocks until all
//!    of them drop**; a query that observes the mark unwinds with a
//!    private sentinel that is caught at exactly one layer — the public
//!    query entry — and surfaces as `Err(`[`Cancelled`]`)`. Clients may
//!    also cancel a single snapshot ([`Snapshot::cancel_token`]) to
//!    honor `$/cancelRequest`. A cancelled query always returns; it is
//!    never dropped on the floor.
//! 4. **Queries speak byte offsets** ([`wolf_span::Span`]) and compiler
//!    values ([`wolf_diag::Diagnostic`]). No LSP types, no UTF-16, no
//!    serialization below this boundary (the `ide`-layer rule).
//! 5. **One truth.** [`Snapshot::diagnostics`] computes exactly the
//!    phase ladder the `wolf` CLI runs for the same bytes (lex → parse →
//!    resolve → typecheck, conform-run rules: single-entry package,
//!    `member: true` participation, typecheck errors withheld while any
//!    body is `NotYetCheckable`) — editor diagnostics and CLI builds
//!    are the same codes at the same spans, by construction.
//!
//! # v0 execution model
//!
//! Non-resident: one analysis pass per request batch, memoized per
//! package entry until the next change (the host clears the memo on
//! every [`QueryHost::apply_change`]; interface-hash-bounded reuse is
//! s57's, D7). Slow is acceptable in v0; wrong process-model
//! assumptions are not — overlays, cancellation and concurrent
//! snapshots above are real and tested, not stubs.
//!
//! Cancellation granularity in v0 is the phase boundary (checkpoints
//! between lex/parse/resolve/typecheck); the resident daemon tightens
//! granularity without changing the contract.

mod completions;
pub mod docs;
mod host;
mod overlay;
mod queries;

pub use completions::{Completion, CompletionKind};
pub use docs::{
    Directive, DocComment, DocFence, DocItem, DocModule, DocPackage, doc_package, resolve_links,
};
pub use host::{CancelToken, Cancelled, Change, QueryHost, Snapshot};
pub use queries::{
    DefResult, DiagnosticsBatch, DocSymbol, FormatResult, HoverResult, SourceFile, SymbolKind,
};

/// The query-contract version (clause list in the crate docs). Bump on
/// any breaking change to the public surface; s57 implements the same
/// numbered contract.
///
/// v2 (s53): the [`docs`] model joins the surface — additive, but the
/// version names a surface rather than a compatibility promise, so it
/// moves. `wolf doc` and hover now read doc comments through one
/// module, which is the clause-5 "one truth" rule applied to prose.
///
/// v3 (s122): [`Snapshot::completions`] joins the surface — additive.
/// Its incomplete-buffer contract (answer from what the frontend
/// recovered; repaired-text re-analysis for member position; the
/// empty list, never an error, when a receiver cannot be typed) is
/// part of the surface the s57 daemon must honor.
pub const CONTRACT_VERSION: u32 = 3;

/// Test-only knob: when set to a number of milliseconds, every query
/// sleeps that long at its first checkpoint (in small cancellable
/// slices). Exists so the cancellation path is testable end-to-end
/// with a genuinely slow query; never set outside tests.
pub const TEST_SLOW_ENV: &str = "WOLF_QUERY_TEST_SLOW_MS";
