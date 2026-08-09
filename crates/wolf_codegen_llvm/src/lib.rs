//! Release backend: WIR -> LLVM (c09, s41-s45).
//!
//! Contract: receives small, deduplicated, pre-optimized, fact-annotated
//! WIR and lowers it with the full D3 fact encoding (noalias/alias.scope
//! per region domain, invariant.load for frozen regions, ranges). Name
//! reserved now; implementation begins at s41.
