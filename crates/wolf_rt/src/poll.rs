//! s137 (#127, `[os.net.wait]`): the LEVEL-TRIGGERED readiness call,
//! the reactor's sibling and deliberately not part of it.
//!
//! `crate::reactor` owns the oneshot, edge-consuming readiness set the
//! TASK layer parks on (epoll, kqueue, `WSAPoll`), and it exists only
//! on the hosts that have a poller written for them. This module owns
//! the other question a program can ask about io — "which of these
//! sockets can I read right now?" — and it answers it with the call
//! every unix has had since before any of those existed, `poll(2)`,
//! plus `WSAPoll` on windows, which is the same call under another
//! name.
//!
//! Two reasons it is separate rather than a function in the reactor.
//! First, a spawn-free serving loop must not disturb the task layer's
//! registrations, and it has no task to park; asking `poll(2)` about a
//! borrowed set of descriptors touches no kernel state at all. Second,
//! this call works on every host with a libc — freebsd included, where
//! the reactor's module is not compiled — so a tier-2 target that
//! `cargo check`s the workspace gets `net_wait` for free rather than a
//! missing module.

/// The raw io handle a readiness question is asked about: a file
/// descriptor on unix, a `SOCKET` on windows (the reactor's own alias,
/// repeated here because this module outlives its cfg gate).
#[cfg(unix)]
pub(crate) type RawIo = std::os::fd::RawFd;
#[cfg(windows)]
pub(crate) type RawIo = std::os::windows::io::RawSocket;

/// s137 (#127, `[os.net.wait]`): readiness over a SET of handles, in
/// one blocking call, for a program that runs no tasks.
///
/// This is deliberately NOT the reactor's poller. That set is
/// the task layer's: its interests are ONESHOT and keyed per (fd,
/// filter), armed by a parked task and consumed by the wake. A
/// spawn-free serving loop asking "which of these twelve sockets can
/// I read?" must neither disturb those registrations nor be limited
/// to one descriptor at a time — and it has no task to park. So the
/// readiness question is asked directly, with a level-triggered call
/// that carries the whole set: `poll(2)` on unix, `WSAPoll` on
/// windows (the same shape, which is why the reactor's own windows
/// rung is built on it). See this module's own doc for why it lives
/// here rather than inside the reactor.
///
/// Answers one flag per input fd, in the input's order: true when the
/// next read will not block — data, a pending connection, an ended
/// peer (`POLLHUP`), or an error (`POLLERR`/`POLLNVAL`), each of
/// which the following syscall reports for itself. All false is the
/// timeout, which is an answer and not a failure. `timeout_ms < 0`
/// blocks until something is ready. `EINTR` retries — a signal is not
/// an answer.
#[cfg(unix)]
pub(crate) fn readable(fds: &[RawIo], timeout_ms: i32) -> std::io::Result<Vec<bool>> {
    let mut pfds: Vec<libc::pollfd> = fds
        .iter()
        .map(|&fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    loop {
        // SAFETY: a live, correctly-counted pollfd array.
        let rc = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, timeout_ms) };
        if rc < 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::EINTR) {
                for p in &mut pfds {
                    p.revents = 0;
                }
                continue;
            }
            return Err(e);
        }
        let ready = libc::POLLIN | libc::POLLHUP | libc::POLLERR | libc::POLLNVAL;
        return Ok(pfds.iter().map(|p| p.revents & ready != 0).collect());
    }
}

/// The windows twin of [`poll_readable`] — `WSAPoll` over the caller's
/// set. `POLLRDNORM` is the readable bit there; `POLLHUP`/`POLLERR`
/// come back in `revents` unasked, exactly as on unix.
#[cfg(windows)]
pub(crate) fn readable(fds: &[RawIo], timeout_ms: i32) -> std::io::Result<Vec<bool>> {
    #[repr(C)]
    struct PollFd {
        fd: usize,
        events: i16,
        revents: i16,
    }
    const POLLRDNORM: i16 = 0x0100;
    const POLLERR: i16 = 0x0001;
    const POLLHUP: i16 = 0x0002;
    const POLLNVAL: i16 = 0x0004;
    // Declared directly (D15: no `windows-sys` in the runtime).
    #[link(name = "ws2_32")]
    unsafe extern "system" {
        fn WSAPoll(fds: *mut PollFd, count: u32, timeout_ms: i32) -> i32;
        fn WSAGetLastError() -> i32;
    }
    let mut pfds: Vec<PollFd> = fds
        .iter()
        .map(|&fd| PollFd {
            fd: fd as usize,
            events: POLLRDNORM,
            revents: 0,
        })
        .collect();
    // SAFETY: a live, correctly-counted WSAPOLLFD array.
    let rc = unsafe { WSAPoll(pfds.as_mut_ptr(), pfds.len() as u32, timeout_ms) };
    if rc < 0 {
        // SAFETY: a plain error read.
        return Err(std::io::Error::from_raw_os_error(unsafe {
            WSAGetLastError()
        }));
    }
    let ready = POLLRDNORM | POLLHUP | POLLERR | POLLNVAL;
    Ok(pfds.iter().map(|p| p.revents & ready != 0).collect())
}
