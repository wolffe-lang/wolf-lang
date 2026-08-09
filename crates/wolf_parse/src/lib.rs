//! The parser (s08–s09).
//!
//! Contract: resilient recursive descent implementing spec/01-grammar.md —
//! error nodes, panic-mode sync on declaration keywords at brace depth 0
//! (the D22 architecture bet), expression orientation, `[]` generics via a
//! single deferred `BracketApply` node. Recovery is fuzz-tested: a
//! single-token mutation may not blast more than its enclosing declaration.
