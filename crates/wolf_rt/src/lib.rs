//! The wolf runtime (c07) — links into USER programs, not the compiler.
//!
//! Contract, by law (D15): pay-for-what-you-use; no global GC, no mandatory
//! background threads; a wolf binary that never spawns is a C binary. May
//! depend on `wolf_span` at most, nothing else in the workspace —
//! `cargo xtask deps-check` enforces it. Deterministic-scheduler hooks
//! (s36) are part of this crate's v1 spec surface.

// s200 (#411): the bulk byte scan over a `List[byte]`, `[mem.list.bytes]`.
pub mod bytes;
// s253 (wolf-lang#811): the core limit, and an end that dumps no core —
// `[os.proc.core]`.
pub mod coredump;
// s225 (wolf-lang#534): replacing the running program and removing a
// variable — `[os.proc.exec]`, `[os.env.unset]`.
pub mod exec;
// s247 (ruling #90): a child that keeps running wolf code — `[os.proc.fork]`.
pub mod fork;
pub mod fs;
// s230 (wolf-lang#346, #698, #699, #700, #702, #704): the calls that
// change a file without writing its bytes, and the umask — `[os.fs.mode]`,
// `[os.fs.create.mode]`, `[os.fs.symlink]`, `[os.fs.link]`, `[os.fs.times]`,
// `[os.fs.truncate]`, `[os.fs.umask]`.
pub mod fs_change;
// s248 (wolf-lang#796, #797; ruling #93): the atomic save's two syncs and
// the canonical path — `[os.fs.sync]`, `[os.fs.realpath]`.
pub mod fs_save;
// s253 (wolf-lang#819): which OS and architecture the program runs on, as
// the kernel answers — `[os.uname]`.
pub mod host;
// s228 (wolf-lang#661): the table behind `fs_open`'s and the net
// calls' handles — numbers never reused, storage always.
pub(crate) mod handles;
// s236 (wolf-lang#633, #701, #742): who the process runs as, a file's
// owner, and the drop — `[os.proc.ids]`, `[os.proc.ids.set]`,
// `[os.fs.owner]`.
pub mod ids;
pub mod io;
// The s107 json query kernels (c26's last crossing): the HAND MIRROR
// of `wolf_mem::json` — the locked graph keeps the reference out of
// reach (D15), the driver's `json_parity` test keeps the copies from
// drifting (the fmtspec precedent).
pub mod json;
pub mod list;
// The s152 keyed store (`[type.map]`): the list buffer discipline
// under a linear-scan key lookup, one entry layout on both tiers.
pub mod map;
pub mod native;
pub mod net;
pub mod os;
// Profile counters (s45). Present in the archive always, referenced
// only by `--profile-gen` builds — `--gc-sections` drops them from
// every other binary, so the never-required posture costs nothing.
pub mod prof;
// The native `Pool[T]` (s173): a generational slot arena and the
// `handle T` word that names a slot without addressing it.
pub mod pool;
pub mod quarantine;
// The OS random source (s118, #143): OS-provided entropy or a TRAP —
// no userspace generator, no seeding, no fallback. NOT linux-gated:
// the call has no concurrency-layer dependency, so the macOS/Windows
// arms compile and unit-test on the host matrix today; compiled-
// program delivery follows the backend port (`[os.random.platform]`).
pub mod random;
// The io reactor (s35) shares the task layer's platform posture:
// epoll on linux (the campaign floor), kqueue on macOS since s59, and
// `WSAPoll` on windows since s60b — each port promised by the
// reactor's interface, readiness adapted underneath (`Interest`/
// `Ready` unchanged). IOCP (the completion-port rung: async fs, the
// scale story) is s60c's, behind the same seam.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub mod reactor;
// s137 (#127): the reactor's LEVEL-TRIGGERED sibling — `poll(2)` /
// `WSAPoll` over a borrowed set, for `net_wait`. Separate from the
// reactor on purpose: it disturbs no task registration, and it
// compiles on every host with a libc (freebsd included, where the
// reactor's poller is not written).
#[cfg(any(unix, windows))]
pub(crate) mod poll;
pub(crate) mod scratch;
// s245 (wolf-lang#778): the runtime's own descriptors (the signal hub's
// pipes, the readiness set's) never take a closed 0, 1 or 2.
#[cfg(unix)]
pub(crate) mod stdfd;
// Signal RECEPTION (s114, #126): the meaning-based receive surface
// over the self-pipe/sigaction trampoline (linux at s114, macOS since
// s59 — `pipe` + FD_CLOEXEC where pipe2 does not exist, spec
// `[os.signal.platform]`'s pre-authorized widening) and over
// `SetConsoleCtrlHandler` on windows (s60b — the console-control
// mapping the same clause spec'd). Rides the task layer's platform
// gate; BSD delivery widens with s61 (a NAMED stop).
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub mod signal;
pub mod str;
// s219 (wolf-lang#622): the controlling terminal — its foreground
// process group and its mode (canonical, echo, signals, vmin/vtime).
pub mod term;
pub mod time;
// The task layer opened on linux (s28's platform posture), crossed
// to macOS at s59 (the stack plumbing — mmap/madvise/guard-fault
// reporting — already carried macOS arms, and the pool is POSIX
// pthreads), and to windows at s60b (Win32 threads on the kernel's
// own reserve-and-guard stacks, the guard reporter a vectored
// exception handler — task/stack_win.rs). Other hosts compile
// wolf_rt without it; freebsd (s61) widens this gate next.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub mod task;

/// A wait the program asked for by name — `time_sleep_ms`, `net_wait`,
/// the dial — on a thread that may be a pool worker: where the task
/// layer exists the pool compensates for it as for any runtime wait
/// (s231, wolf-lang#713); elsewhere there is no pool to tell.
pub(crate) fn host_wait<R>(f: impl FnOnce() -> R) -> R {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        task::compensated(f)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        f()
    }
}
