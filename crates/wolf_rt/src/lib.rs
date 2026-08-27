//! The wolf runtime (c07) — links into USER programs, not the compiler.
//!
//! Contract, by law (D15): pay-for-what-you-use; no global GC, no mandatory
//! background threads; a wolf binary that never spawns is a C binary. May
//! depend on `wolf_span` at most, nothing else in the workspace —
//! `cargo xtask deps-check` enforces it. Deterministic-scheduler hooks
//! (s36) are part of this crate's v1 spec surface.

pub mod fs;
pub mod io;
// The s107 json query kernels (c26's last crossing): the HAND MIRROR
// of `wolf_mem::json` — the locked graph keeps the reference out of
// reach (D15), the driver's `json_parity` test keeps the copies from
// drifting (the fmtspec precedent).
pub mod json;
pub mod list;
pub mod native;
pub mod net;
pub mod os;
// Profile counters (s45). Present in the archive always, referenced
// only by `--profile-gen` builds — `--gc-sections` drops them from
// every other binary, so the never-required posture costs nothing.
pub mod prof;
pub mod quarantine;
// The OS random source (s118, #143): OS-provided entropy or a TRAP —
// no userspace generator, no seeding, no fallback. NOT linux-gated:
// the call has no concurrency-layer dependency, so the macOS/Windows
// arms compile and unit-test on the host matrix today; compiled-
// program delivery follows the backend port (`[os.random.platform]`).
pub mod random;
// The io reactor (s35) shares the task layer's platform posture:
// epoll first (this campaign's floor); the kqueue/IOCP port sprints
// widen against its interface, readiness adapted underneath.
#[cfg(target_os = "linux")]
pub mod reactor;
// Signal RECEPTION (s114, #126): the self-pipe/sigaction trampoline
// and the meaning-based receive surface. Rides the task layer's
// platform gate — the whole native concurrency floor is linux at this
// campaign stage (reactor/net/task); macOS/BSD/Windows delivery widens
// with the port sprints (a NAMED stop, spec `[os.signal.platform]`).
#[cfg(target_os = "linux")]
pub mod signal;
pub mod str;
pub mod time;
// The task layer is linux-only at this campaign stage — the same
// platform posture as native codegen (s28: M1 targets linux/x86-64;
// mmap/pthread stack plumbing uses linux-specific surface). Other
// hosts compile wolf_rt without it; the fan-out's port sprints widen.
#[cfg(target_os = "linux")]
pub mod task;
