//! Diagnostic types and rendering.
//!
//! Contract: structured diagnostics behind a reporter interface (s10):
//! error codes, labeled spans, Elm-voice rendering (RFC-1644 layout),
//! cascade suppression, and a JSON reporter for tooling. Every diagnostic
//! the compiler can emit gets a reviewed snapshot (s01 harness hook).
//! Depends only on `wolf_span`.
