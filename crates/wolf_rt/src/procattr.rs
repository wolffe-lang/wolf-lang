//! s272 (wolf-lang#921, #922; R-s272-5 and R-s272-6, the recommendations
//! implemented): two attributes of the calling process a program could not
//! reach — its ROOT directory (`chroot(2)`, `[os.proc.chroot]`) and its
//! scheduling PRIORITY, the nice value (`getpriority(2)` and
//! `setpriority(2)` in `nice(2)`'s shape, `[os.proc.priority]`).
//!
//! - `os_chroot(path) -> () ! {denied, io, not_found, unsupported}` —
//!   `chroot(2)` and nothing more: the working directory is left where it
//!   is (coreutils' `chroot` calls `chdir("/")` next; a program calls
//!   `os_chdir("/")`). `denied` is `EPERM` (not privileged) and `EACCES` (a
//!   component not searchable), `not_found` a missing path, anything else
//!   `io` (a file is `ENOTDIR`), each with the host's number beside the row.
//!   linux looks the path up before it asks for the privilege (a missing
//!   path is `not_found` to anyone); macOS asks first (`denied`), measured.
//! - `os_getpriority() -> int ! {unsupported}` — the nice value
//!   (`getpriority(PRIO_PROCESS, 0)`; -1 is a value, so errno is read).
//! - `os_setpriority(delta) -> () ! {denied, io, unsupported}` — the value
//!   read, `delta` added (saturating), clamped to the host's
//!   `[PRIO_MIN, PRIO_MAX]` and set. A lowering without the privilege is
//!   `denied` (`EACCES`, 13, on linux and macOS; `EPERM` where a host says
//!   so), told apart by `os_error()` and its text — what GNU `nice` prints
//!   ("cannot set niceness: Permission denied", measured in the same
//!   container). On linux the nice value is the calling THREAD's (NPTL;
//!   setpriority(2) BUGS), and a child or an exec takes the caller's.
//!
//! windows has neither: both answer `unsupported`, by name.

use crate::fs::{os_error_clear, os_error_record};
use crate::os::proc_code;

/// The host's refusal as `proc_code`'s, the number left beside it.
fn code_of(e: &std::io::Error) -> i64 {
    os_error_record(e);
    match e.kind() {
        std::io::ErrorKind::NotFound => proc_code::NOT_FOUND,
        std::io::ErrorKind::PermissionDenied => proc_code::DENIED,
        _ => proc_code::IO,
    }
}

/// `chroot(2)` on `path`; the code (`proc_code`'s OK, DENIED, NOT_FOUND,
/// IO, UNSUPPORTED).
pub fn chroot(path: &str) -> i64 {
    os_error_clear();
    #[cfg(unix)]
    {
        let Ok(c) = std::ffi::CString::new(path) else {
            return proc_code::IO;
        };
        // SAFETY: a NUL-terminated path this call owns.
        if unsafe { libc::chroot(c.as_ptr()) } == 0 {
            return proc_code::OK;
        }
        code_of(&std::io::Error::last_os_error())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        proc_code::UNSUPPORTED
    }
}

/// The calling process's nice value, or `None` on a host with none
/// (windows).
pub fn priority() -> Option<i64> {
    #[cfg(unix)]
    {
        // getpriority(2): -1 is a legal answer, so errno is cleared before
        // the call and read after it; for the calling process the call
        // cannot fail (`which` and `who` are constants).
        // SAFETY: errno is this thread's; the call takes constants.
        unsafe {
            *errno_slot() = 0;
            let p = libc::getpriority(libc::PRIO_PROCESS, 0);
            if p == -1 && *errno_slot() != 0 {
                return None;
            }
            Some(i64::from(p))
        }
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// The value `delta` asks for from `now`: saturating, inside the host's
/// range (the host clamps too; linux's top is 19, below `PRIO_MAX`).
pub fn target(now: i64, delta: i64) -> i64 {
    #[cfg(unix)]
    let (lo, hi) = (i64::from(libc::PRIO_MIN), i64::from(libc::PRIO_MAX));
    #[cfg(not(unix))]
    let (lo, hi) = (-20, 20);
    now.saturating_add(delta).clamp(lo, hi)
}

/// `os_setpriority`'s code (`proc_code`'s OK, DENIED, IO, UNSUPPORTED).
pub fn set_priority(delta: i64) -> i64 {
    os_error_clear();
    #[cfg(unix)]
    {
        let Some(now) = priority() else {
            return proc_code::IO;
        };
        let want = target(now, delta) as libc::c_int;
        // SAFETY: setpriority with constants and an int in the host's range.
        if unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, want) } == 0 {
            return proc_code::OK;
        }
        code_of(&std::io::Error::last_os_error())
    }
    #[cfg(not(unix))]
    {
        let _ = delta;
        proc_code::UNSUPPORTED
    }
}

/// This thread's `errno` (`os.rs`'s pair, by copy).
#[cfg(all(unix, target_os = "linux"))]
unsafe fn errno_slot() -> *mut libc::c_int {
    unsafe { libc::__errno_location() }
}
#[cfg(all(unix, not(target_os = "linux")))]
unsafe fn errno_slot() -> *mut libc::c_int {
    unsafe { libc::__error() }
}

/// `os_chroot(path) -> () ! {denied, io, not_found, unsupported}` — the
/// code.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_chroot(pp: i64, pl: i64) -> i64 {
    let path = unsafe { crate::str::view(pp, pl) };
    chroot(path)
}

/// `os_getpriority() -> int ! {unsupported}` — 0 and the value through
/// `out` (it may be negative), or the code.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_getpriority(out: i64) -> i64 {
    match priority() {
        Some(p) => {
            unsafe { crate::str::write_word(out, p) };
            proc_code::OK
        }
        None => proc_code::UNSUPPORTED,
    }
}

/// `os_setpriority(delta) -> () ! {denied, io, unsupported}` — the code.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_setpriority(delta: i64) -> i64 {
    set_priority(delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The target is saturating and inside the range, whatever the delta.
    #[test]
    fn the_target_saturates_and_clamps() {
        assert_eq!(target(0, 0), 0);
        assert_eq!(target(0, 5), 5);
        assert_eq!(target(10, i64::MAX), 20);
        assert_eq!(target(-10, i64::MIN), -20);
        assert_eq!(target(19, 1), 20);
    }

    /// A missing path is `not_found` on linux (looked up first) and
    /// `denied` on macOS (the privilege asked first), for a test that is not
    /// root; windows has no root.
    #[test]
    fn chroot_of_a_missing_path() {
        let c = chroot("wolf-s272-no-such-dir/inner");
        #[cfg(any(target_os = "linux", target_os = "android"))]
        assert_eq!(c, proc_code::NOT_FOUND);
        #[cfg(target_vendor = "apple")]
        assert_eq!(c, proc_code::DENIED);
        #[cfg(not(unix))]
        assert_eq!(c, proc_code::UNSUPPORTED);
        let _ = c;
    }

    /// The value reads, and a set of a delta of 0 moves nothing.
    #[cfg(unix)]
    #[test]
    fn the_priority_reads_and_a_zero_delta_keeps_it() {
        let p = priority().expect("a unix host has a nice value");
        assert!((-20..=20).contains(&p), "{p}");
        assert_eq!(set_priority(0), proc_code::OK);
        assert_eq!(priority(), Some(p));
    }

    #[cfg(not(unix))]
    #[test]
    fn windows_has_no_nice_value() {
        assert_eq!(priority(), None);
        assert_eq!(set_priority(1), proc_code::UNSUPPORTED);
    }
}
