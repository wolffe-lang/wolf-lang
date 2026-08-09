//! Byte spans, source maps, and interned file identities.
//!
//! Contract: the bottom of the crate graph — depends on nothing in the
//! workspace. Every diagnostic, token, tree node, and IR entity references
//! source positions through this crate's types. Spans are byte-exact
//! (see s07: lossless trivia and byte-offset string semantics per D25).
