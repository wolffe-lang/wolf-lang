//! `os_fork` (s247, ruling #90, `[os.proc.fork]`) — a child that keeps
//! running wolf code, sound beside the runtime's own threads.
//!
//! # What the child is
//!
//! fork(2): one thread, the caller's, at the point of return; the memory
//! of the parent at that instant; every descriptor shared, offsets
//! included; the dispositions and the caller's mask kept, nothing pending.
//! A threaded parent's other threads do not exist in the child, so a lock
//! one of them held at the instant is held for ever there, and the
//! child's first use of it waits for ever: the runtime's own (the pool's
//! queues, the hub's state, the reactor's table, the child table), Rust's
//! standard library's (the environment, the standard streams) and the C
//! library's (its allocator) alike. pthread_atfork(3) is the C answer for
//! one library's locks; the runtime's answer is that no other thread of
//! the process holds any lock at the instant.
//!
//! # The protocol (the `busy` rule plus a hold of the idle threads)
//!
//! 1. **The rule.** The call answers `busy`, before the host is asked,
//!    when the caller is a pool worker (a task or a proc handler: the
//!    child would be a worker with no `main` to return to), when any task
//!    is live anywhere in the process (queued, running or parked: a task
//!    blocked in a system call holds its thread and may hold a lock — the
//!    standard output's, while a write to a full pipe blocks — and a task
//!    computing reaches no point where it could stop, since the runtime
//!    has no preemption of its own, `task/mod.rs`), or under the
//!    deterministic scheduler. So only `main`'s thread forks, and only
//!    from a pool with no work.
//! 2. **The hold.** Then every thread the runtime made is idle, and each
//!    is brought to rest where it holds no lock: the pool's workers parked
//!    at the top of their loop (`task::fork_hold`, a handshake, which also
//!    joins every worker that was retiring, so none is in its exit path);
//!    the hub's drain thread and the reactor's thread claimed in their
//!    kernel wait (`poll`; `epoll_wait` or `kevent`) by a compare-and-swap
//!    on their state word, without waking them — one that comes back from
//!    the wait while claimed parks there, holding nothing, until the fork
//!    is done. Each comes to rest in bounded time because nothing is queued
//!    for it. The calling thread's catchable signals are blocked across the
//!    call, so no handler runs on it while the child is being made.
//! 3. **The fork**, then the parent releases the three in the reverse
//!    order (the workers' release waits until each has left its hold
//!    point, so the next fork counts afresh).
//! 4. **The child re-makes what it must**, before the call returns there,
//!    in the reverse of the holds' order, descriptors first: the readiness
//!    set's inherited descriptors closed (linux shares an epoll set across
//!    a fork; macOS hands a child no kqueue); the hub's pipes closed and
//!    made again (the inherited ends are the parent's: a trampoline's byte
//!    there would be read by the parent's drain thread), so a signal that
//!    comes before the child asks for anything waits in its own pipe; the
//!    pool emptied (made again at the child's first spawn); the child
//!    table, every slot a tombstone (the parent's children are not the
//!    child's). Then the caller's mask comes back. The threads are the
//!    child's first use's to make — its first spawn the workers, its first
//!    wait or listen the drain thread (which then unblocks the meanings the
//!    program armed), its first park the readiness set and its thread —
//!    and a host refusal there is that call's own `io`, as for a runtime
//!    no start made (s239, s243). A refusal of the hub's pipes ends the
//!    child — one stderr line and exit 71, ruling #80's start rule at the
//!    child's start — since there is no caller to answer.
//!
//! Threads the runtime did not make (a C library a program imports may
//! start some) are outside the rule, as raw FFI that blocks is outside
//! blocking compensation (`task/mod.rs`).

use crate::os::proc_code;

/// `os_fork() -> (int, int) ! {busy, io, unsupported}` (s247, ruling #90,
/// `[os.proc.fork]`) — code 0 and the pair through `out`: in the parent
/// the child's process id and its handle in the child table, in the
/// child `(0, -1)`; else the code: `BUSY` (the rule), `IO` (the host's
/// refusal, its number in `os_error()`), `UNSUPPORTED` (a host with no
/// fork the runtime serves: windows; any host without the task layer).
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_fork(out: i64) -> i64 {
    match fork() {
        Ok((a, b)) => {
            // SAFETY: the caller's 16-byte slot.
            unsafe { crate::str::write_pair(out, a, b) };
            proc_code::OK
        }
        Err(code) => code,
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn fork() -> Result<(i64, i64), i64> {
    Err(proc_code::UNSUPPORTED)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn fork() -> Result<(i64, i64), i64> {
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
    /// One fork at a time (a second caller could only be a thread the
    /// runtime did not make).
    static FORKING: AtomicBool = AtomicBool::new(false);
    crate::fs::os_error_clear();
    if crate::task::on_worker() || crate::task::det_in_domain() || crate::task::live_tasks() > 0 {
        return Err(proc_code::BUSY);
    }
    if FORKING.swap(true, SeqCst) {
        return Err(proc_code::BUSY);
    }
    crate::task::fork_hold();
    crate::signal::fork_hold();
    crate::reactor::fork_hold();
    let old = block_signals();
    // SAFETY: fork(2); every other thread of the runtime's is parked at a
    // point where it holds no lock (above), so the child's memory is
    // consistent; the child re-makes what is per process below.
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        child(&old);
        FORKING.store(false, SeqCst);
        return Ok((0, -1));
    }
    let err = (pid < 0).then(std::io::Error::last_os_error);
    restore_signals(&old);
    crate::reactor::fork_release();
    crate::signal::fork_release();
    crate::task::fork_release();
    FORKING.store(false, SeqCst);
    if let Some(e) = err {
        crate::fs::os_error_record(&e);
        return Err(proc_code::IO);
    }
    Ok((i64::from(pid), crate::os::adopt_forked(pid)))
}

/// The child's side, with signals blocked: the descriptors (the readiness
/// set's closed first, then the hub's pipes made), the pool and the table;
/// then the caller's mask back. The threads come at the child's first use.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn child(old: &libc::sigset_t) {
    crate::reactor::fork_child();
    // PLANT (s247, never to merge): the child's hub re-making planted out —
    // the child keeps the parent's pipes and the parent's drain-thread state.
    let _plant = crate::signal::fork_child_pipes as fn() -> std::io::Result<bool>;
    let _keep = refuse as fn(&str, &std::io::Error) -> !;
    crate::task::fork_child();
    crate::os::fork_child_clear();
    restore_signals(old);
}

/// A host refusal in the child: one stderr line and
/// [`crate::native::START_FAILED_EXIT`], with no exit handler of the
/// parent's run (they are the parent's).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn refuse(what: &str, e: &std::io::Error) -> ! {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(
        err,
        "wolf: a forked child could not make {what} ({e}); the child did not start"
    );
    let _ = err.flush();
    // SAFETY: _exit(2) ends this process at once.
    unsafe { libc::_exit(crate::native::START_FAILED_EXIT) }
}

/// Block every signal a process may block on the calling thread, but the
/// ones a fault raises (blocking those is undefined when the fault
/// comes); the old mask is answered.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn block_signals() -> libc::sigset_t {
    // SAFETY: zeroed sigsets filled by sigfillset/sigdelset; the mask is
    // this thread's own.
    unsafe {
        let mut all: libc::sigset_t = std::mem::zeroed();
        let mut old: libc::sigset_t = std::mem::zeroed();
        libc::sigfillset(&mut all);
        for s in [
            libc::SIGSEGV,
            libc::SIGBUS,
            libc::SIGILL,
            libc::SIGFPE,
            libc::SIGTRAP,
        ] {
            libc::sigdelset(&mut all, s);
        }
        libc::pthread_sigmask(libc::SIG_BLOCK, &all, &mut old);
        old
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn restore_signals(old: &libc::sigset_t) {
    // SAFETY: the mask [`block_signals`] answered, put back.
    unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, old, std::ptr::null_mut()) };
}
