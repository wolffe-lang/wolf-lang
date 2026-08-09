//! The memory checker (s18–s23) — the language's soul.
//!
//! Contract: Tier-0 MVS exclusivity (loan sets over a checker-internal,
//! WIR-shaped check CFG — never exported; WIR builds from typed HIR),
//! region inference and the region checker (D10), Perceus-style `shared`
//! insertion and generational `handle` pools (X5), the unsafe tier with
//! Tree-Borrows-shaped provenance (D11), and the miri-lite UB checker
//! (s23). Checker facts flow to WIR as verified fact annotations.
