//! The lexer (s07).
//!
//! Contract: mode-stack lexing for f-strings (nested braces, format specs),
//! `"""` dedent by closing-delimiter column, `re"…"` generalized literals,
//! byte-exact spans, lossless trivia (the formatter's substrate). Fuzzed
//! from day one (s01 scaffold).
