//! Syntax tree with lossless trivia (s08–s09).
//!
//! Contract: the lossless concrete tree (error nodes included) plus the
//! typed AST view over it. Round-trips source byte-for-byte — `wolf fmt`
//! (s11) and resilient diagnostics (D22) both depend on that property.
