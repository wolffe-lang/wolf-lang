//! WIR — the mid-level SSA IR that is the product (s24–s27).
//!
//! Contract: flat index arenas, block parameters (no phis), effect tokens,
//! textual round-trip, and a verifier for which facts (region provenance,
//! noalias edges, ranges) are SEMANTICS: passes may rely on them and may
//! not drop them — there is no droppable-metadata channel (D2). Built from
//! sema's typed HIR via Braun on-the-fly SSA with peephole-at-construction.
