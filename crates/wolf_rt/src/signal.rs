//! Signal RECEPTION (s114, wolf-lang#126) — the program hears the
//! signal, as an EVENT in normal task control flow, never as a callback
//! running in async-signal-handler context.
//!
//! # The safe design (not negotiable)
//!
//! Async-signal-safety makes real handlers a minefield: a handler may
//! call almost nothing (no allocation, no mutex, no wolf code). Every
//! serious runtime converts a signal into a pollable event and handles
//! it in ordinary code — the self-pipe / signalfd pattern. Wolf does
//! the same: [`trampoline`] is the minimal async-signal-safe C-shaped
//! handler (it maps the signal number to a MEANING byte and `write`s
//! ONE byte to a self-pipe — the only steps a handler is allowed), and
//! a dedicated drain thread ([`reader_main`]) reads those bytes in
//! normal thread context and delivers meanings to parked waiters. No
//! wolf code EVER runs in the handler.
//!
//! # The abstraction is by MEANING
//!
//! `os_kill` (the SEND side, `os.rs`) and this RECEIVE side agree on
//! meanings, not raw signal numbers: RELOAD / TERMINATE / QUIT /
//! UPGRADE (the four wws needs), mapped to platform signals at the
//! boundary ([`to_signal`] / [`from_signal`]) — never raw ints in
//! portable code. The set is a bitmask so `os_signal_wait(set)` can
//! await any of several meanings and report which arrived.
//!
//! # Platform matrix (spec `[os.signal.platform]`)
//!
//! - **Linux / macOS (here):** `sigaction` trampoline → self-pipe →
//!   drain thread. Full set. One platform seam: linux creates the pipe
//!   with `pipe2(O_CLOEXEC)`, macOS with `pipe` + two
//!   `fcntl(FD_CLOEXEC)` (no pipe2 there — the s59 widening
//!   `[os.signal.platform]` pre-authorized). The module rides the task
//!   layer's platform gate, which covers both since s59; FreeBSD joins
//!   with s61 (same POSIX shape, or `kqueue` `EVFILT_SIGNAL`).
//! - **Windows (s60b):** POSIX signals do not exist; the meaning
//!   abstraction maps to `SetConsoleCtrlHandler` — CTRL_C / CTRL_BREAK
//!   / CTRL_CLOSE → TERMINATE / QUIT / TERMINATE, exactly the clause's
//!   table. The console handler is the trampoline's twin with one
//!   luxury: the system runs it on a thread of its own, in normal
//!   context, so it locks the hub and enqueues directly — no self-pipe,
//!   no drain thread. A meaning the program listens for is consumed
//!   (the handler returns TRUE); one it does not is left to the
//!   console's default disposition (FALSE: the process ends, as an
//!   unhandled SIGTERM would). RELOAD (SIGHUP) and UPGRADE (SIGUSR2)
//!   have NO Windows analog for EXTERNAL delivery: a NAMED platform gap
//!   — wws reload/upgrade on Windows uses a control channel (ws04's
//!   ungated half), not a signal. Self-delivery (`raise`) is a plain
//!   in-process enqueue on windows for every meaning — `kill(getpid())`
//!   has no console twin that would not also hit every process on the
//!   console (`GenerateConsoleCtrlEvent` is console-wide) — so the
//!   loopback rows and a program's own reload path work; an unlistened
//!   self-raise of TERMINATE/QUIT ends the process as Ctrl+C would
//!   (`STATUS_CONTROL_C_EXIT`), and of RELOAD/UPGRADE is dropped (there
//!   is no disposition to deliver to — the checked machine's posture).
//!
//! # Determinism (target 4 — X12; spec `[os.signal.det]`)
//!
//! A signal is external non-determinism. Under the deterministic test
//! scheduler (`--schedules`/`--replay`, `task::det`) delivery is
//! EXCLUDED from the replay stream (ruling (b)): signal arrival emits
//! NO `sched-ev` record — it is not a `pick`, not a schedule point.
//! The deterministic scheduler owns task/channel interleavings, not OS
//! signal arrival; a server's real SIGHUP is not a thing you replay.
//! Exclusion is implemented BY CONSTRUCTION here: nothing in this
//! module calls `sched_point`, so `task::det`'s recorder never sees a
//! signal. A signal WAIT still parks through `task::blocking` (its
//! block-enter/block-exit are ordinary schedule points); only the
//! external ARRIVAL is excluded. The loopback witness stays
//! deterministic because the program waits for exactly the signal it
//! raised — the OUTPUT is causally pinned even though arrival TIMING is
//! not.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

use crate::task::{blocking, current_scope, kill_teardown_check};

/// The meaning vocabulary — a bitmask (spec `[os.signal.set]`). The
/// four wws needs; `os_kill`'s send side and this receive side agree on
/// these, never on raw signal numbers.
pub mod meaning {
    /// Re-read configuration / drain old workers (nginx `SIGHUP`).
    pub const RELOAD: i64 = 1;
    /// Fast shutdown (`SIGTERM`).
    pub const TERMINATE: i64 = 2;
    /// Graceful shutdown after drain (`SIGQUIT`).
    pub const QUIT: i64 = 4;
    /// Binary-swap / zero-downtime upgrade (`SIGUSR2`).
    pub const UPGRADE: i64 = 8;
    /// Every meaning this sprint delivers.
    pub const ALL: i64 = RELOAD | TERMINATE | QUIT | UPGRADE;
}

/// Error codes the WIR lowering keys on (the `os.rs` `proc_code`
/// precedent). `listen`/`raise` return `OK`/`IO`; `wait` returns the
/// delivered meaning (> 0) or `-IO`.
pub mod sig_code {
    /// Success.
    pub const OK: i64 = 0;
    /// The one checkable failure row (a bad set, an install failure).
    pub const IO: i64 = 1;
}

/// The four meanings in canonical bit order (iteration helper).
const MEANINGS: [i64; 4] = [
    meaning::RELOAD,
    meaning::TERMINATE,
    meaning::QUIT,
    meaning::UPGRADE,
];

/// The signal hub: the installed-meaning set and the FIFO of delivered
/// meanings awaiting waiters (plus, on unix, the self-pipe's read end).
struct Hub {
    #[cfg(unix)]
    read_fd: std::os::fd::RawFd,
    state: Mutex<HubState>,
    cv: Condvar,
}

struct HubState {
    /// Meanings whose delivery (a `sigaction` handler; the console
    /// handler) is installed.
    installed: i64,
    /// Delivered meanings, oldest first (a coalescing edge queue).
    queue: VecDeque<i64>,
}

static HUB: OnceLock<Hub> = OnceLock::new();

/// Lazy init (D15 pay-for-what-you-use — a program that never listens
/// carries no signal pipe and no drain thread): first `listen` builds
/// the hub and the platform's delivery path ([`sys::init`]).
fn hub() -> &'static Hub {
    let h = HUB.get_or_init(|| Hub {
        #[cfg(unix)]
        read_fd: sys::make_pipe(),
        state: Mutex::new(HubState {
            installed: 0,
            queue: VecDeque::new(),
        }),
        cv: Condvar::new(),
    });
    sys::init(h);
    h
}

/// Enqueue a delivered meaning and wake the waiters — the drain
/// thread's step on unix, the console handler's own on windows.
fn deliver(h: &Hub, m: i64) {
    let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
    st.queue.push_back(m);
    h.cv.notify_all();
}

/// The per-platform delivery path behind the hub — the WHOLE per-OS
/// surface of this module: `init` (unix: the self-pipe's drain
/// thread; windows: nothing to start), `listen_one` (unix: the
/// `sigaction` trampoline for the meaning's signal; windows: the one
/// console handler), `raise` (unix: `kill(getpid())`; windows: the
/// in-process enqueue the module doc explains).
#[cfg(unix)]
mod sys {
    use super::{Hub, deliver, meaning, sig_code};
    use std::os::fd::RawFd;
    use std::sync::Once;
    use std::sync::atomic::{AtomicI32, Ordering};

    /// The self-pipe write end, reachable from the async-signal
    /// handler WITHOUT a lock (an atomic load is async-signal-safe; a
    /// mutex is not). `-1` until the hub is initialized.
    static SELF_PIPE_W: AtomicI32 = AtomicI32::new(-1);
    static READER: Once = Once::new();

    /// Build the self-pipe; returns the read end (the write end
    /// publishes through [`SELF_PIPE_W`]).
    pub fn make_pipe() -> RawFd {
        let mut fds = [0i32; 2];
        // SAFETY: plain pipe creation; result checked. Linux gets the
        // atomic-CLOEXEC pipe2; macOS has no pipe2, so it is `pipe` +
        // two `fcntl(F_SETFD, FD_CLOEXEC)` — the exact widening
        // `[os.signal.platform]` pre-authorized (s59). The set-race
        // window pipe2 closes (a concurrent fork+exec between pipe and
        // fcntl) does not arise here: the hub initializes once, under
        // OnceLock, before any wolf code could spawn a process off it.
        #[cfg(target_os = "linux")]
        let rc = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
        #[cfg(not(target_os = "linux"))]
        let rc = unsafe {
            let rc = libc::pipe(fds.as_mut_ptr());
            if rc == 0 {
                libc::fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC);
                libc::fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC);
            }
            rc
        };
        assert!(rc == 0, "signal self-pipe failed");
        // Write end nonblocking: the handler must never block.
        // SAFETY: our own fd; standard nonblocking flip.
        unsafe {
            let fl = libc::fcntl(fds[1], libc::F_GETFL);
            libc::fcntl(fds[1], libc::F_SETFL, fl | libc::O_NONBLOCK);
        }
        SELF_PIPE_W.store(fds[1], Ordering::Relaxed);
        fds[0]
    }

    /// Spawn the one drain thread (once).
    pub fn init(h: &'static Hub) {
        READER.call_once(|| {
            let read_fd = h.read_fd;
            std::thread::Builder::new()
                .name("wolf-signal".into())
                .spawn(move || reader_main(h, read_fd))
                .expect("spawn signal drain thread");
        });
    }

    /// Map a single meaning bit to its platform signal number.
    fn to_signal(m: i64) -> Option<i32> {
        match m {
            meaning::RELOAD => Some(libc::SIGHUP),
            meaning::TERMINATE => Some(libc::SIGTERM),
            meaning::QUIT => Some(libc::SIGQUIT),
            meaning::UPGRADE => Some(libc::SIGUSR2),
            _ => None,
        }
    }

    /// Map a delivered signal number back to its meaning bit (0 = a
    /// signal we never installed, ignored). Async-signal-safe: integer
    /// compares only, called from the trampoline.
    fn from_signal(sig: i32) -> i64 {
        if sig == libc::SIGHUP {
            meaning::RELOAD
        } else if sig == libc::SIGTERM {
            meaning::TERMINATE
        } else if sig == libc::SIGQUIT {
            meaning::QUIT
        } else if sig == libc::SIGUSR2 {
            meaning::UPGRADE
        } else {
            0
        }
    }

    /// The async-signal-safe trampoline: map the signal to a meaning
    /// byte and `write` it to the self-pipe. This is the ENTIRE handler
    /// — no allocation, no lock, no wolf code. A full (nonblocking)
    /// pipe drops the byte, which coalesces rapid repeats: standard
    /// signal semantics (real-time signals / queued siginfo are
    /// explicitly out of scope).
    extern "C" fn trampoline(sig: libc::c_int) {
        let m = from_signal(sig);
        if m == 0 {
            return;
        }
        let byte = m as u8; // meanings 1/2/4/8 fit one byte
        let fd = SELF_PIPE_W.load(Ordering::Relaxed);
        if fd >= 0 {
            // SAFETY: `write` is async-signal-safe; a single-byte write
            // to the runtime's own nonblocking pipe. The return is
            // ignored on purpose — a full pipe (EAGAIN) drops,
            // coalescing.
            unsafe {
                libc::write(fd, (&raw const byte).cast(), 1);
            }
        }
    }

    /// The drain thread: read meaning bytes off the self-pipe in
    /// NORMAL thread context (locks, allocates — everything the
    /// handler cannot), enqueue them, and wake waiters. One blocking
    /// `read`, not an event loop — the sanctioned "dedicated thread"
    /// shape (the reactor thread's twin), not a second epoll.
    fn reader_main(h: &'static Hub, read_fd: RawFd) {
        let mut buf = [0u8; 64];
        loop {
            // SAFETY: valid read end; buffer is live and correctly
            // sized.
            let n = unsafe { libc::read(read_fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 {
                // EINTR: retry. Any other error on our own pipe is
                // fatal to delivery — end the thread (only reachable on
                // teardown).
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                return;
            }
            if n == 0 {
                return; // write end closed (process teardown)
            }
            for &b in &buf[..n as usize] {
                deliver(h, i64::from(b));
            }
        }
    }

    /// Install the trampoline for the meaning's signal (`SA_RESTART`:
    /// pooled syscalls are not torn by a delivered signal; the
    /// reactor's `epoll_wait` handles EINTR regardless).
    pub fn listen_one(_h: &Hub, bit: i64) -> Result<(), ()> {
        let Some(sig) = to_signal(bit) else {
            return Ok(());
        };
        // SAFETY: installing a static extern handler; the sigaction
        // struct is zeroed then filled, the mask emptied — the stack.rs
        // idiom.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = trampoline as extern "C" fn(libc::c_int) as usize;
            libc::sigemptyset(&mut sa.sa_mask);
            sa.sa_flags = libc::SA_RESTART;
            if libc::sigaction(sig, &sa, std::ptr::null_mut()) == 0 {
                Ok(())
            } else {
                Err(())
            }
        }
    }

    /// Send one signal to THIS process: `kill(getpid(), sig)` targets
    /// the process so any thread without the signal blocked runs the
    /// trampoline. `IO` for an unmapped meaning or a failed send.
    pub fn raise(m: i64) -> i64 {
        let Some(sig) = to_signal(m) else {
            return sig_code::IO;
        };
        // SAFETY: getpid + kill with a validated signal number.
        let rc = unsafe { libc::kill(libc::getpid(), sig) };
        if rc == 0 { sig_code::OK } else { sig_code::IO }
    }
}

#[cfg(windows)]
mod sys {
    use super::{HUB, Hub, deliver, meaning, sig_code};
    use std::sync::Once;

    /// `SetConsoleCtrlHandler` event codes.
    const CTRL_C_EVENT: u32 = 0;
    const CTRL_BREAK_EVENT: u32 = 1;
    const CTRL_CLOSE_EVENT: u32 = 2;
    /// What the console's default handler exits an unhandled Ctrl+C
    /// with — the disposition an unlistened TERMINATE/QUIT self-raise
    /// mirrors.
    const STATUS_CONTROL_C_EXIT: u32 = 0xC000_013A;

    // Declared directly (D15: no `windows-sys` in the runtime); both
    // are kernel32, on every wolf link line.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(handler: unsafe extern "system" fn(u32) -> i32, add: i32) -> i32;
        fn ExitProcess(code: u32) -> !;
    }

    static HANDLER: Once = Once::new();

    /// Nothing to start: the console handler is installed by the
    /// first `listen`, and the system runs it on a thread of its own.
    pub fn init(_h: &'static Hub) {}

    /// The spec'd table (`[os.signal.platform]`): CTRL_C → TERMINATE,
    /// CTRL_BREAK → QUIT, CTRL_CLOSE → TERMINATE. Everything else
    /// (logoff, shutdown) is not ours: 0.
    fn from_event(ev: u32) -> i64 {
        match ev {
            CTRL_C_EVENT | CTRL_CLOSE_EVENT => meaning::TERMINATE,
            CTRL_BREAK_EVENT => meaning::QUIT,
            _ => 0,
        }
    }

    /// The console handler — the trampoline's twin, run by the system
    /// on its own thread in normal context: lock the hub, enqueue,
    /// wake. TRUE (handled) when the program listens for the meaning;
    /// FALSE otherwise, so the console's default disposition (process
    /// exit) applies exactly as an unhandled SIGTERM's would.
    unsafe extern "system" fn ctrl_handler(ev: u32) -> i32 {
        let m = from_event(ev);
        if m == 0 {
            return 0;
        }
        let Some(h) = HUB.get() else {
            return 0;
        };
        let listened = h.state.lock().unwrap_or_else(|p| p.into_inner()).installed & m != 0;
        if !listened {
            return 0;
        }
        deliver(h, m);
        1
    }

    /// Install the one console handler (once, on the first listened
    /// meaning of any kind — RELOAD/UPGRADE included, which only
    /// self-delivery can ever reach here). A failed install is `IO`.
    pub fn listen_one(_h: &Hub, _bit: i64) -> Result<(), ()> {
        let mut ok = true;
        HANDLER.call_once(|| {
            // SAFETY: registering a static extern fn with the console.
            ok = unsafe { SetConsoleCtrlHandler(ctrl_handler, 1) } != 0;
        });
        if ok { Ok(()) } else { Err(()) }
    }

    /// Self-delivery, in process (the module doc says why there is no
    /// console twin of `kill(getpid())`): a listened meaning is
    /// enqueued as the console handler would enqueue it; an unlistened
    /// TERMINATE/QUIT ends the process as an unhandled Ctrl+C does; an
    /// unlistened RELOAD/UPGRADE has no disposition to reach and is
    /// dropped. An unmapped meaning is `IO`.
    pub fn raise(m: i64) -> i64 {
        if !matches!(
            m,
            meaning::RELOAD | meaning::TERMINATE | meaning::QUIT | meaning::UPGRADE
        ) {
            return sig_code::IO;
        }
        let h = super::hub();
        let listened = h.state.lock().unwrap_or_else(|p| p.into_inner()).installed & m != 0;
        if listened {
            deliver(h, m);
        } else if m == meaning::TERMINATE || m == meaning::QUIT {
            // SAFETY: process exit with the console's own status.
            unsafe { ExitProcess(STATUS_CONTROL_C_EXIT) };
        }
        sig_code::OK
    }
}

/// `os_signal_listen(set)` — register interest in a set of meanings,
/// installing the platform's delivery for each ([`sys::listen_one`]).
/// Idempotent per meaning. `IO` if any install fails.
pub fn listen(mask: i64) -> i64 {
    let h = hub();
    let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
    for &bit in &MEANINGS {
        if mask & bit != 0 && st.installed & bit == 0 {
            if sys::listen_one(h, bit).is_err() {
                return sig_code::IO;
            }
            st.installed |= bit;
        }
    }
    sig_code::OK
}

/// `os_signal_wait(set)` — park the calling task until a meaning in
/// `set` is delivered, returning that meaning. Parks through
/// `task::blocking` (pool compensation, c19). A KILL teardown point
/// (`[conc.proc.kill]`): a killed supervisor terminates at the wait
/// instead of hanging on a signal that never comes. Plain cancellation
/// keeps waiting (the net tier's kill-only posture — the `{io}` row
/// vocabulary has no cancellation row). An empty or all-invalid set is
/// `-IO` (nothing could ever arrive — never a hang).
pub fn wait(mask: i64) -> i64 {
    let want = mask & meaning::ALL;
    if want == 0 {
        return -sig_code::IO;
    }
    let h = hub();
    blocking(|| {
        let scope = current_scope();
        let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if let Some(pos) = st.queue.iter().position(|&m| m & want != 0) {
                let m = st.queue.remove(pos).expect("position just found it");
                return m;
            }
            if let Some(s) = scope.as_ref()
                && s.is_killed()
            {
                drop(st);
                // Kill teardown terminates the task here; on a killed
                // proc's unwindable frame it never returns, so the
                // `-IO` below is only the C/main-frame fallback.
                kill_teardown_check(s);
                return -sig_code::IO;
            }
            let (g, _) =
                h.cv.wait_timeout(st, Duration::from_millis(5))
                    .unwrap_or_else(|p| p.into_inner());
            st = g;
        }
    })
}

/// `os_signal_raise(meaning)` — send one signal to THIS process (the
/// self-send companion to `os_kill`'s send-to-child; the deterministic
/// loopback the witness rides, and a real capability: a program
/// triggering its own reload path). `kill(getpid(), sig)` on unix; the
/// in-process enqueue on windows ([`sys::raise`]). `IO` for an unmapped
/// meaning or a failed send.
pub fn raise(m: i64) -> i64 {
    sys::raise(m)
}

// ---- the C entry surface -------------------------------------------------
//
// The compiled program calls these; codes mirror `sig_code`.

/// `os_signal_listen(set: int) -> () ! {io}` — 0 ok, else io.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_listen(mask: i64) -> i64 {
    listen(mask)
}

/// `os_signal_wait(set: int) -> int ! {io}` — the delivered meaning
/// (>= 0), or a negated code (`-IO`) on failure.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_wait(mask: i64) -> i64 {
    wait(mask)
}

/// `os_signal_raise(sig: int) -> () ! {io}` — 0 ok, else io.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_raise(m: i64) -> i64 {
    raise(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{ExitReason, scope};
    use std::sync::atomic::Ordering;

    /// The hub is process-global (one self-pipe, one queue): serialize
    /// the signal tests so one test's delivered meaning can never be
    /// consumed by another's wait.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The deterministic loopback (the witness's runtime twin): listen,
    /// self-raise, wait — the meaning comes back. Ten iterations pin
    /// stability without sampling (the #50 lesson: wait, never poll).
    #[test]
    fn self_raise_delivers_the_meaning() {
        let _g = serial();
        assert_eq!(listen(meaning::RELOAD), sig_code::OK);
        for _ in 0..10 {
            assert_eq!(raise(meaning::RELOAD), sig_code::OK);
            assert_eq!(wait(meaning::RELOAD), meaning::RELOAD);
        }
    }

    /// A wait selects only its set: an unrelated delivered meaning stays
    /// queued while a wait for a different meaning blocks past it, then
    /// its own arrival wakes it — and the earlier one is still there.
    #[test]
    fn wait_filters_by_set() {
        let _g = serial();
        assert_eq!(listen(meaning::TERMINATE | meaning::QUIT), sig_code::OK);
        assert_eq!(raise(meaning::TERMINATE), sig_code::OK);
        // Deliver TERMINATE first; a QUIT waiter must not take it.
        // Give the drain thread the QUIT too, then wait for QUIT.
        assert_eq!(raise(meaning::QUIT), sig_code::OK);
        assert_eq!(wait(meaning::QUIT), meaning::QUIT);
        // TERMINATE is still pending for its own waiter.
        assert_eq!(wait(meaning::TERMINATE), meaning::TERMINATE);
    }

    /// The wws shape: a supervisor task parks on the set while a sibling
    /// raises the signal — delivery reaches the parked waiter through
    /// the pool's blocking compensation, no wolf code in a handler.
    #[test]
    fn parked_supervisor_wakes_on_sibling_raise() {
        let _g = serial();
        assert_eq!(listen(meaning::UPGRADE), sig_code::OK);
        let got = std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0));
        let sink = got.clone();
        let r = scope("supervisor", |s| {
            s.spawn("waiter", move |_| {
                let m = wait(meaning::UPGRADE);
                sink.store(m, Ordering::SeqCst);
                ExitReason::Normal
            });
            std::thread::sleep(Duration::from_millis(30)); // let it park
            s.spawn("raiser", |_| {
                assert_eq!(raise(meaning::UPGRADE), sig_code::OK);
                ExitReason::Normal
            });
        });
        assert!(r.is_ok());
        assert_eq!(got.load(Ordering::SeqCst), meaning::UPGRADE);
    }

    /// An empty / all-invalid set never hangs: it is `-IO` at once.
    #[test]
    fn empty_set_is_io_not_a_hang() {
        let _g = serial();
        assert_eq!(wait(0), -sig_code::IO);
    }

    /// Raising an unmapped meaning is `io`, never a send of a wild
    /// signal number.
    #[test]
    fn raise_unmapped_is_io() {
        let _g = serial();
        assert_eq!(raise(999), sig_code::IO);
    }

    /// Determinism (target 4): a self-raised signal under the det
    /// scheduler is EXCLUDED from the recording — a `listen`/`raise`/
    /// `wait` sequence contributes NO signal `sched-ev` (there is no
    /// signal kind), so the stream is byte-identical across runs of the
    /// same seed even though arrival timing is external.
    #[test]
    fn det_excludes_signal_delivery_from_replay() {
        let _g = serial();
        use crate::task::det::{Source, run};
        assert_eq!(listen(meaning::RELOAD), sig_code::OK);
        let stream = |seed: u64| -> String {
            run(Source::Seed(seed), || {
                assert_eq!(raise(meaning::RELOAD), sig_code::OK);
                assert_eq!(wait(meaning::RELOAD), meaning::RELOAD);
            })
            .serialize()
        };
        // Same seed → byte-identical stream (arrival excluded, so it
        // cannot perturb the record).
        assert_eq!(stream(7), stream(7));
    }
}
