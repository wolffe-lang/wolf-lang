//! The wolf runtime (c07) — links into USER programs, not the compiler.
//!
//! Contract, by law (D15): pay-for-what-you-use; no global GC, no mandatory
//! background threads; a wolf binary that never spawns is a C binary. May
//! depend on `wolf_span` at most, nothing else in the workspace —
//! `cargo xtask deps-check` enforces it. Deterministic-scheduler hooks
//! (s36) are part of this crate's v1 spec surface.

pub mod io;
pub mod native;
pub mod quarantine;
// The task layer is linux-only at this campaign stage — the same
// platform posture as native codegen (s28: M1 targets linux/x86-64;
// mmap/pthread stack plumbing uses linux-specific surface). Other
// hosts compile wolf_rt without it; the fan-out's port sprints widen.
#[cfg(target_os = "linux")]
pub mod task;
