//! The runtime's own descriptors stay off 0, 1 and 2 (s245,
//! wolf-lang#778, `[os.fs.std]`).
//!
//! A host hands out the LOWEST free descriptor (pipe(2), epoll_create(2),
//! eventfd(2), kqueue(2)), so a program started without a standard
//! descriptor (`prog >&-`) would find the runtime's signal hub or
//! readiness set sitting on it before `main`: its `fs_fstat(1)` answers
//! ok and its writes to standard output go into the runtime's pipe. 0.2.31
//! did exactly that on the dev tier (the hub made at start, s243). Every
//! descriptor the runtime makes for itself goes through [`off_std`], which
//! moves one the host placed on 0, 1 or 2 to the lowest free number at 3
//! or above (`fcntl(F_DUPFD_CLOEXEC, 3)`) and closes the low one, so the
//! standard descriptor is closed again as the program sees it.
//!
//! The move is two calls, so another thread opening a descriptor between
//! them could take the low number back; the hub and the readiness set are
//! made before `main`, when the program has no other thread. A wake pipe
//! made by `os_signal_wait_fd` under a running program is the one maker
//! called with threads about; the window there is those two calls.

use std::os::fd::RawFd;

/// `fd` itself when it is 3 or above; else a close-on-exec duplicate at
/// 3 or above, with `fd` closed. On the host's refusal `fd` is closed
/// too and the error is answered, so nothing the caller made is left
/// open on 0–2.
pub(crate) fn off_std(fd: RawFd) -> std::io::Result<RawFd> {
    if fd > 2 {
        return Ok(fd);
    }
    // SAFETY: `fd` is a descriptor the caller just made and owns; the
    // duplicate is checked below.
    let up = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    let err = (up < 0).then(std::io::Error::last_os_error);
    // SAFETY: the caller's own low descriptor, closed once.
    unsafe { libc::close(fd) };
    match err {
        Some(e) => Err(e),
        None => Ok(up),
    }
}

/// Both ends of a fresh pipe off 0–2 ([`off_std`]); on a refusal both
/// are closed.
pub(crate) fn pipe_off_std(fds: [RawFd; 2]) -> std::io::Result<[RawFd; 2]> {
    let r = match off_std(fds[0]) {
        Ok(r) => r,
        Err(e) => {
            // SAFETY: the other end is ours and published nowhere.
            unsafe { libc::close(fds[1]) };
            return Err(e);
        }
    };
    match off_std(fds[1]) {
        Ok(w) => Ok([r, w]),
        Err(e) => {
            // SAFETY: the moved read end is ours and published nowhere.
            unsafe { libc::close(r) };
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipe() -> [RawFd; 2] {
        let mut fds = [0; 2];
        // SAFETY: plain pipe creation; checked.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        fds
    }

    /// A descriptor already above the standard three is left as it is.
    /// (One placed on 0–2 is driven by the driver's `stdfd_lanes` gate, in
    /// a child started with that descriptor closed: moving this test
    /// process's own 0 would race the other tests' opens.)
    #[test]
    fn a_descriptor_above_two_stays_where_it_is() {
        let p = pipe();
        assert!(p[0] > 2 && p[1] > 2, "the test runner holds 0-2: {p:?}");
        assert_eq!(pipe_off_std(p).expect("no move"), p);
        // SAFETY: our own pipe.
        unsafe {
            libc::close(p[0]);
            libc::close(p[1]);
        }
    }
}
