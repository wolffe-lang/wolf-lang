//! Interim debug backend: WIR -> Cranelift (s28-s31).
//!
//! Contract: sits behind the `wolf_backend` trait (defined in s28) so the
//! owned Tier-F backend (c12) replaces it without the driver noticing.
//! Scaffolding by decision D1: this crate is deleted at c12 closeout.
