//! Semantic analysis (s12–s17).
//!
//! Contract: name resolution and module interfaces (`wolfi` files, the D7
//! incremental spine), the bidirectional type checker (D27: signatures on
//! items, inference inside bodies), nominal traits with checked generics
//! (D28), `!T` error rows (D30), and the sandboxed CTFE engine (D29 — no
//! ambient IO; the D33 supply-chain guarantee starts here). Produces the
//! typed HIR that both `wolf_mem` and `wolf_wir` consume.
