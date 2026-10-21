//! s253 (wolf-lang#811, `[os.proc.core]`; ruling asked,
//! `[os.process.calls.note]` R1, the recommendation implemented): what a
//! process can know and set of the core its end may leave.
//!
//! - `os_core_limit() -> (int, int) ! {unsupported}` — the core limit,
//!   soft and hard (`getrlimit(RLIMIT_CORE)`), in bytes, -1 for unlimited;
//!   read before the program's own set, the limit it was born with.
//! - `os_set_core_limit(soft, hard) -> () ! {denied, invalid,
//!   unsupported}` — `setrlimit(RLIMIT_CORE)`. The SHAPE first: each limit
//!   is -1 or 0 and up, and the soft one is at most the hard one, so an
//!   unlimited soft under a finite hard is `invalid`; then the HOST
//!   (windows `unsupported`); then the host's answer: `denied` for a raise
//!   of the hard limit it refuses (`EPERM`).
//! - `os_core_off() -> () ! {unsupported}` — this process dumps no core
//!   whatever ends it. On linux (and PAX) that is the "dumpable" attribute
//!   (`prctl(PR_SET_DUMPABLE, 0)`, GNU `timeout`'s own call, measured by
//!   s253): a pattern that pipes cores to a program ignores the limit
//!   (core(5)), never the attribute; `exec` sets the attribute back, so a
//!   child it starts is untouched. Elsewhere a core is a file the soft
//!   limit governs, and the call sets the soft limit to 0 (which a child
//!   inherits).

use crate::os::proc_code;

/// The shape `os_set_core_limit` holds before any host is asked: each
/// limit -1 (unlimited) or 0 and up, the soft at most the hard.
pub fn shape_ok(soft: i64, hard: i64) -> bool {
    if soft < -1 || hard < -1 {
        return false;
    }
    if hard == -1 {
        return true;
    }
    soft != -1 && soft <= hard
}

#[cfg(unix)]
fn word(v: libc::rlim_t) -> i64 {
    if v == libc::RLIM_INFINITY {
        -1
    } else {
        i64::try_from(v).unwrap_or(i64::MAX)
    }
}

#[cfg(unix)]
fn rlim(v: i64) -> libc::rlim_t {
    if v < 0 {
        libc::RLIM_INFINITY
    } else {
        v as libc::rlim_t
    }
}

#[cfg(unix)]
fn get() -> Option<libc::rlimit> {
    let mut r = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: getrlimit fills the struct it is handed.
    (unsafe { libc::getrlimit(libc::RLIMIT_CORE, &mut r) } == 0).then_some(r)
}

/// The core limit, soft and hard, -1 for unlimited; `None` on a host with
/// none (windows).
pub fn limit() -> Option<(i64, i64)> {
    #[cfg(unix)]
    {
        get().map(|r| (word(r.rlim_cur), word(r.rlim_max)))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// `os_set_core_limit`'s code (`proc_code`'s OK, INVALID, UNSUPPORTED,
/// DENIED).
pub fn set(soft: i64, hard: i64) -> i64 {
    if !shape_ok(soft, hard) {
        return proc_code::INVALID;
    }
    #[cfg(unix)]
    {
        let r = libc::rlimit {
            rlim_cur: rlim(soft),
            rlim_max: rlim(hard),
        };
        // SAFETY: setrlimit reads the struct it is handed.
        if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &r) } == 0 {
            return proc_code::OK;
        }
        // After the shape, the host refuses a raise of the hard limit
        // (EPERM) — PAX refuses every set so — and nothing else a call in
        // the shape can meet; a host's EINVAL is its own reading of the
        // shape.
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EINVAL) => proc_code::INVALID,
            _ => proc_code::DENIED,
        }
    }
    #[cfg(not(unix))]
    {
        proc_code::UNSUPPORTED
    }
}

/// `os_core_off`'s code (`proc_code`'s OK or UNSUPPORTED).
pub fn off() -> i64 {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // SAFETY: PR_SET_DUMPABLE with 0 (SUID_DUMP_DISABLE) changes one
        // attribute of the calling process.
        if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } == 0 {
            proc_code::OK
        } else {
            proc_code::UNSUPPORTED
        }
    }
    #[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
    {
        let Some(mut r) = get() else {
            return proc_code::UNSUPPORTED;
        };
        r.rlim_cur = 0;
        // SAFETY: setrlimit reads the struct it is handed; lowering the
        // soft limit needs no privilege.
        if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &r) } == 0 {
            proc_code::OK
        } else {
            proc_code::UNSUPPORTED
        }
    }
    #[cfg(not(unix))]
    {
        proc_code::UNSUPPORTED
    }
}

/// `os_core_limit() -> (int, int) ! {unsupported}` — 0 and the pair (soft,
/// hard) through `out`, or the code.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_core_limit(out: i64) -> i64 {
    match limit() {
        Some((soft, hard)) => {
            unsafe { crate::str::write_pair(out, soft, hard) };
            proc_code::OK
        }
        None => proc_code::UNSUPPORTED,
    }
}

/// `os_set_core_limit(soft, hard) -> () ! {denied, invalid,
/// unsupported}` — the code.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_set_core_limit(soft: i64, hard: i64) -> i64 {
    set(soft, hard)
}

/// `os_core_off() -> () ! {unsupported}` — the code.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_core_off() -> i64 {
    off()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape, decided before any host: one truth everywhere.
    #[test]
    fn the_shape_is_the_clauses() {
        for (s, h, ok) in [
            (0, 0, true),
            (0, -1, true),
            (-1, -1, true),
            (4096, 4096, true),
            (0, 4096, true),
            (5, 4, false),
            (-1, 4096, false),
            (-2, 0, false),
            (0, -2, false),
            (i64::MIN, -1, false),
        ] {
            assert_eq!(shape_ok(s, h), ok, "({s}, {h})");
            if !ok {
                assert_eq!(set(s, h), proc_code::INVALID, "({s}, {h}) is refused first");
            }
        }
    }

    /// The reader answers a limit in the shape, and setting the limit to
    /// itself is allowed (lowering nothing, raising nothing).
    #[cfg(unix)]
    #[test]
    fn the_limit_reads_and_sets_to_itself() {
        let (soft, hard) = limit().expect("a unix host has a core limit");
        assert!(shape_ok(soft, hard), "the read is in the shape: ({soft}, {hard})");
        assert_eq!(set(soft, hard), proc_code::OK, "the limit set to itself");
        assert_eq!(limit(), Some((soft, hard)));
    }

    #[cfg(not(unix))]
    #[test]
    fn windows_has_no_core_limit() {
        assert_eq!(limit(), None);
        assert_eq!(set(0, 0), proc_code::UNSUPPORTED);
        assert_eq!(off(), proc_code::UNSUPPORTED);
    }
}
