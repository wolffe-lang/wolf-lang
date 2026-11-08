//! glibc's malloc arena limit, settled before any thread runs the program
//! (s286, wolf-lang#1051).
//!
//! glibc gives each new thread a malloc arena. The first time a thread
//! needs one past glibc's test bound (`M_ARENA_TEST`: 8 where `long` is 8
//! bytes, 2 where it is 4), glibc reads the CPU count to set its limit,
//! and that read, `get_nprocs()`, opens `/sys/devices/system/cpu/online`
//! on the lowest free descriptor (get_nprocs(3)). In a program started
//! with descriptor 1 closed, that open was descriptor 1 for a few
//! microseconds, on a pool worker, while main ran, and `fs_fstat(1)` saw
//! it (#1051). [`crate::stdfd`] moves the runtime's own descriptors off
//! 0–2; this one is the C library's, so nothing moved it.
//!
//! The limit is settled in two steps, so glibc never runs its test:
//!
//! 1. [`settle_at_entry`], from the entry shim's first call before `main`,
//!    with no other thread in the process: `M_ARENA_MAX` is set to the test
//!    bound. glibc does not use its test bound once the limit is nonzero
//!    (mallopt(3)), so it never reads the CPU count, for any thread; and a
//!    program with no more arenas than the bound allocates as it did. This
//!    step makes no system call.
//! 2. [`settle_at_pool_start`], from the pool's one-time initialization,
//!    before its first worker exists: `M_ARENA_MAX` is set to the limit
//!    glibc documents for itself, 8 per CPU online (`2 *` on 32-bit), from
//!    the same `get_nprocs()`. Its file lands on the thread that starts the
//!    pool, the only thread running the program at that point; the runtime's
//!    own threads (the signal drain, the reactor) never look at a
//!    program's descriptors.
//!
//! One program allocates differently: one whose C library starts more
//! threads than the bound before any pool exists. Those threads share the
//! bound's arenas until the pool starts (for good if it never does), where
//! glibc would have allowed 8 per CPU.
//!
//! The user's own limit is left alone. The environment the program started
//! with, which is the one glibc read, is read once: `MALLOC_ARENA_MAX` with
//! any value, or a `GLIBC_TUNABLES` item named `glibc.malloc.arena_max`
//! ([`names_a_limit`]), skips both steps. A test bound alone
//! (`MALLOC_ARENA_TEST`, `glibc.malloc.arena_test`) is not a limit: it only
//! moves the moment glibc reads the CPU count, which is the read this
//! settle removes. In a set-user-ID or set-group-ID program
//! (`AT_SECURE`), glibc ignores both (mallopt(3) for the variable;
//! measured for the tunable, glibc 2.39: a set-user-ID probe held 160
//! arenas on 20 CPUs with either set to 2), so they are not the user's
//! setting there and the limit is settled.
//!
//! Off linux-gnu both calls do nothing: glibc's arenas are glibc's.

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod imp {
    use std::os::unix::ffi::OsStrExt as _;
    use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

    /// glibc's documented test bound, and its limit's multiple of the CPUs
    /// online: 8 on 64-bit, 2 on 32-bit (mallopt(3); the manual's
    /// `glibc.malloc.arena_test` and `glibc.malloc.arena_max`).
    const PER_CPU: libc::c_int = if cfg!(target_pointer_width = "64") {
        8
    } else {
        2
    };

    const UNDECIDED: u8 = 0;
    const SETTLE: u8 = 1;
    const LEAVE: u8 = 2;

    /// Whether the limit is the runtime's to settle, decided once, at the
    /// first step that runs (the entry's, in a compiled program).
    static DECISION: AtomicU8 = AtomicU8::new(UNDECIDED);

    unsafe extern "C" {
        /// get_nprocs(3): the processors online, the count glibc's limit uses.
        fn get_nprocs() -> libc::c_int;
    }

    fn settles() -> bool {
        match DECISION.load(Relaxed) {
            SETTLE => true,
            LEAVE => false,
            _ => {
                // SAFETY: reads the auxiliary vector the kernel gave the process.
                let secure = unsafe { libc::getauxval(libc::AT_SECURE) } != 0;
                let tunables = std::env::var_os("GLIBC_TUNABLES");
                let user = !secure
                    && super::names_a_limit(
                        std::env::var_os("MALLOC_ARENA_MAX").is_some(),
                        tunables.as_deref().map(|t| t.as_bytes()),
                    );
                DECISION.store(if user { LEAVE } else { SETTLE }, Relaxed);
                !user
            }
        }
    }

    pub(crate) fn settle_at_entry() {
        if settles() {
            // SAFETY: a malloc parameter, set before any other thread exists.
            unsafe { libc::mallopt(libc::M_ARENA_MAX, PER_CPU) };
        }
    }

    pub(crate) fn settle_at_pool_start() {
        if settles() {
            // SAFETY: a plain query; its descriptor is closed before it returns.
            let online = unsafe { get_nprocs() }.max(1);
            let limit = online.saturating_mul(PER_CPU).max(PER_CPU);
            // SAFETY: a malloc parameter, set before the pool's first worker.
            unsafe { libc::mallopt(libc::M_ARENA_MAX, limit) };
        }
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
mod imp {
    pub(crate) fn settle_at_entry() {}
    // Unused where the task layer is absent (`lib.rs`'s gate on `task`).
    #[allow(dead_code)]
    pub(crate) fn settle_at_pool_start() {}
}

pub(crate) use imp::settle_at_entry;
// The pool, and so its step, exists where the task layer does.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) use imp::settle_at_pool_start;

/// Whether the start environment names an arena limit of the user's:
/// `MALLOC_ARENA_MAX` is set (`max_var`), with any value, or `tunables`
/// (`GLIBC_TUNABLES`, colon-separated `name=value` items) holds an item
/// whose name is exactly `glibc.malloc.arena_max`. How glibc reads the
/// value is glibc's business; the user asked for their own policy.
#[cfg_attr(not(all(target_os = "linux", target_env = "gnu")), allow(dead_code))]
pub(crate) fn names_a_limit(max_var: bool, tunables: Option<&[u8]>) -> bool {
    max_var
        || tunables.is_some_and(|t| {
            t.split(|&b| b == b':').any(|item| {
                item.split(|&b| b == b'=').next() == Some(&b"glibc.malloc.arena_max"[..])
            })
        })
}

#[cfg(test)]
mod tests {
    use super::names_a_limit;

    #[test]
    fn the_users_limit_is_named_by_either_spelling() {
        assert!(!names_a_limit(false, None), "no setting");
        assert!(names_a_limit(true, None), "MALLOC_ARENA_MAX, any value");
        for t in [
            "glibc.malloc.arena_max=2",
            "glibc.malloc.arena_max=0",
            "glibc.malloc.trim_threshold=128:glibc.malloc.arena_max=4",
            "glibc.malloc.arena_max=4:glibc.malloc.check=3",
            "glibc.malloc.arena_max",
        ] {
            assert!(names_a_limit(false, Some(t.as_bytes())), "{t}");
        }
    }

    #[test]
    fn a_test_bound_or_a_lookalike_is_not_a_limit() {
        for t in [
            "",
            ":",
            "glibc.malloc.arena_test=1",
            "glibc.malloc.arena_maxx=2",
            "xglibc.malloc.arena_max=2",
            "glibc.malloc.check=3:",
            "glibc.malloc.trim_threshold=glibc.malloc.arena_max",
        ] {
            assert!(!names_a_limit(false, Some(t.as_bytes())), "{t}");
        }
    }
}
