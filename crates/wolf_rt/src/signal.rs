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
//!   **An inherited mask (s188, wolf-lang#483).** A process inherits
//!   its signal mask across `fork` and `exec`; a job started from a
//!   shell that blocks SIGQUIT has it blocked on every thread, and a
//!   handler for it never runs (`kill -QUIT` pends forever). Masks are
//!   per thread, and a process-directed signal goes to any thread that
//!   does not block it, so `listen` has the drain thread — the one
//!   thread the runtime keeps for the program's whole life (pool
//!   workers retire) — unblock exactly the signals the program arms,
//!   and waits until it has. Every other bit of every thread's mask
//!   stays as inherited, and the runtime never touches a signal the
//!   program did not arm. The request rides the self-pipe as a control
//!   byte (`CTL_UNBLOCK`), because only a thread can change its
//!   own mask.
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
    /// s219 (wolf-lang#622): the terminal's interrupt character, Ctrl-C
    /// (`SIGINT`).
    pub const INTERRUPT: i64 = 16;
    /// s219: the terminal's suspend character, Ctrl-Z (`SIGTSTP`).
    pub const SUSPEND: i64 = 32;
    /// s219: a background process group read the terminal (`SIGTTIN`).
    pub const BG_READ: i64 = 64;
    /// s219: a background process group wrote to, or changed, the
    /// terminal (`SIGTTOU`).
    pub const BG_WRITE: i64 = 128;
    /// s219: a child stopped or ended (`SIGCHLD`).
    pub const CHILD: i64 = 256;
    /// s232 (wolf-lang#679): the user-defined signal wws does not take
    /// (`SIGUSR1`; `SIGUSR2` is `UPGRADE`).
    pub const USER: i64 = 512;
    /// s232: a timer ran out (`SIGALRM`).
    pub const ALARM: i64 = 1 << 10;
    /// s232 (wolf-lang#423): a write to a pipe or socket nobody reads
    /// (`SIGPIPE`).
    pub const BROKEN_PIPE: i64 = 1 << 11;
    /// s232: the process was asked to abort (`SIGABRT`).
    pub const ABORT: i64 = 1 << 12;
    /// s232: the process was continued after a stop (`SIGCONT`).
    pub const CONTINUE: i64 = 1 << 13;
    /// s232: the terminal's window changed size (`SIGWINCH`).
    pub const RESIZE: i64 = 1 << 14;
    /// s232: out-of-band data arrived on a socket (`SIGURG`).
    pub const URGENT: i64 = 1 << 15;
    /// s232: a virtual (user-time) timer ran out (`SIGVTALRM`).
    pub const VIRTUAL_ALARM: i64 = 1 << 16;
    /// s232: a profiling timer ran out (`SIGPROF`).
    pub const PROFILE_ALARM: i64 = 1 << 17;
    /// s232: the CPU-time limit was passed (`SIGXCPU`).
    pub const CPU_LIMIT: i64 = 1 << 18;
    /// s232: the file-size limit was passed (`SIGXFSZ`).
    pub const FILE_LIMIT: i64 = 1 << 19;
    /// s232 (wolf-lang#141), SEND only: the end no process can catch
    /// (`SIGKILL`).
    pub const KILL: i64 = 1 << 20;
    /// s232, SEND only: the stop no process can catch (`SIGSTOP`).
    pub const STOP: i64 = 1 << 21;
    /// s232, SEND only — a fault signal: a handler that returns from a
    /// real fault runs the faulting instruction again, and the runtime's
    /// stack guard owns `SIGSEGV` and `SIGBUS` (`task::stack`).
    pub const SEGMENT_FAULT: i64 = 1 << 22;
    /// s232, SEND only (fault): `SIGBUS`.
    pub const BUS_ERROR: i64 = 1 << 23;
    /// s232, SEND only (fault): `SIGFPE`.
    pub const ARITHMETIC: i64 = 1 << 24;
    /// s232, SEND only (fault): `SIGILL`.
    pub const ILLEGAL: i64 = 1 << 25;
    /// s232, SEND only (fault): `SIGTRAP`.
    pub const BREAKPOINT: i64 = 1 << 26;
    /// s232, SEND only (fault): `SIGSYS`.
    pub const BAD_SYSCALL: i64 = 1 << 27;
    /// The s114 four, the meanings windows maps a console event to.
    pub const V1: i64 = RELOAD | TERMINATE | QUIT | UPGRADE;
    /// Every meaning a program may listen for, ignore, put back to the
    /// default, wait for, poll and raise on itself: the twenty catchable
    /// signals (s232: bits 0..19).
    pub const LISTENABLE: i64 = (1 << 20) - 1;
    /// Every meaning a program may SEND (`os_signal_send`) and ask the
    /// number of (`os_signal_number`): the twenty, `KILL`, `STOP` and the
    /// six fault signals (s232: bits 0..27).
    pub const SENDABLE: i64 = (1 << 28) - 1;
}

/// Error codes the WIR lowering keys on (the `os.rs` `proc_code`
/// precedent). `listen`/`raise` return `OK`/`IO`; `wait` returns the
/// delivered meaning (> 0) or `-IO`.
pub mod sig_code {
    /// Success.
    pub const OK: i64 = 0;
    /// The one checkable failure row (a bad set, an install failure).
    pub const IO: i64 = 1;
    /// s219: a disposition this host has no analog for (windows), by
    /// name — declared by `os_signal_ignore` and `os_signal_default`.
    pub const UNSUPPORTED: i64 = 2;
}

/// How many meanings can be listened for — the self-pipe carries a
/// delivered meaning's index, `0..LISTENED_COUNT`.
pub const LISTENED_COUNT: usize = 20;

/// How many meanings there are (`SENDABLE`'s bits).
pub const MEANING_COUNT: usize = 28;

/// Every listenable meaning in canonical bit order (iteration helper);
/// a meaning is `1 << i` for its index `i` here, and the self-pipe
/// carries the index.
const MEANINGS: [i64; LISTENED_COUNT] = {
    let mut a = [0i64; LISTENED_COUNT];
    let mut i = 0;
    while i < LISTENED_COUNT {
        a[i] = 1 << i;
        i += 1;
    }
    a
};

/// A set every set-taking call accepts (s232, `[os.signal.set]`'s one
/// shape rule): non-empty, and every bit a listenable meaning.
#[must_use]
pub const fn listenable_set(set: i64) -> bool {
    set != 0 && set & !meaning::LISTENABLE == 0
}

/// One listenable meaning (`os_signal_raise`'s operand).
#[must_use]
pub const fn one_listenable(m: i64) -> bool {
    m > 0 && m & (m - 1) == 0 && m & !meaning::LISTENABLE == 0
}

/// One meaning of the whole table (`os_signal_send`'s and
/// `os_signal_number`'s operand).
#[must_use]
pub const fn one_sendable(m: i64) -> bool {
    m > 0 && m & (m - 1) == 0 && m & !meaning::SENDABLE == 0
}

/// What a meaning's signal does to this process (s219,
/// `[os.signal.disp]`): the host's default action, nothing, or the
/// trampoline (listen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disp {
    Default,
    Ignore,
}

/// The signal hub: the installed-meaning set and the FIFO of delivered
/// meanings awaiting waiters (plus, on unix, the self-pipe's read end
/// and the control pipe's).
struct Hub {
    #[cfg(unix)]
    read_fd: std::os::fd::RawFd,
    #[cfg(unix)]
    ctl_fd: std::os::fd::RawFd,
    state: Mutex<HubState>,
    cv: Condvar,
}

struct HubState {
    /// Meanings whose delivery (a `sigaction` handler; the console
    /// handler) is installed.
    installed: i64,
    /// Delivered meanings, oldest first (a coalescing edge queue).
    queue: VecDeque<i64>,
    /// s234 (#719, `[os.signal.wait.fd]`): the write ends of the wake
    /// pipes of the `os_signal_wait_fd` calls parked now. Whoever moves
    /// a meaning onto the queue ([`sys::drain`], under this lock) writes
    /// one byte to each, so a wait blocked in `poll(2)` on its
    /// descriptor and its pipe learns a meaning came without a timer.
    #[cfg(unix)]
    waiters: Vec<std::os::fd::RawFd>,
}

static HUB: OnceLock<Hub> = OnceLock::new();
/// Serializes the hub's making (s239): two first calls make one hub.
static HUB_MAKING: Mutex<()> = Mutex::new(());

/// Lazy init (D15 pay-for-what-you-use — a program that never listens
/// carries no signal pipe and no drain thread): the first call that
/// needs it builds the hub and the platform's delivery path
/// ([`sys::init`]) — or answers the host's refusal (s239, wolf-lang#750:
/// a pipe or the drain thread the host would not give). Nothing is left
/// half-made that a later call cannot finish.
fn try_hub() -> std::io::Result<&'static Hub> {
    if sys::running()
        && let Some(h) = HUB.get()
    {
        return Ok(h);
    }
    let _making = HUB_MAKING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let h = match HUB.get() {
        Some(h) => h,
        None => {
            #[cfg(unix)]
            let (read_fd, ctl_fd) = sys::make_pipes()?;
            HUB.get_or_init(move || Hub {
                #[cfg(unix)]
                read_fd,
                #[cfg(unix)]
                ctl_fd,
                state: Mutex::new(HubState {
                    installed: 0,
                    queue: VecDeque::new(),
                    #[cfg(unix)]
                    waiters: Vec::new(),
                }),
                cv: Condvar::new(),
            })
        }
    };
    sys::init(h)?;
    Ok(h)
}

/// The hub for the calls that do not park — `listen`, `ignore`,
/// `default`, `poll`, `raise`: a refusal to make it still ends the
/// process there (s239's census: not a park, so not that lane's rule;
/// named in `[os.net.reactor]`'s note). The one parking call,
/// `os_signal_wait_fd`, asks [`try_hub`] and answers `io`.
fn hub() -> &'static Hub {
    match try_hub() {
        Ok(h) => h,
        Err(e) => panic!("the signal hub could not be made: {e}"),
    }
}

/// Enqueue a delivered meaning and wake the waiters — the console
/// handler's own step on windows (unix enqueues in [`sys::drain`]).
#[cfg(windows)]
fn deliver(h: &Hub, m: i64) {
    let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
    st.queue.push_back(m);
    h.cv.notify_all();
}

/// The per-platform delivery path behind the hub — the WHOLE per-OS
/// surface of this module: `init` (unix: the drain thread; windows:
/// nothing to start), `listen_one` (unix: the `sigaction` trampoline
/// for the meaning's signal; windows: the one console handler), `raise`
/// (unix: `kill(getpid())`; windows: the in-process enqueue the module
/// doc explains), `settle`/`drain` (unix: s232's exact poll; windows:
/// nothing, its handler enqueues under the lock).
#[cfg(unix)]
mod sys {
    use super::{Hub, HubState, meaning, sig_code};
    use std::os::fd::RawFd;
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};
    use std::sync::{Condvar, Mutex};

    /// The self-pipe write end, reachable from the async-signal
    /// handler WITHOUT a lock (an atomic load is async-signal-safe; a
    /// mutex is not). `-1` until the hub is initialized. Its bytes are
    /// delivered meanings' INDEXES (`0..LISTENED_COUNT`) and nothing
    /// else (s232: the unblock requests have a pipe of their own,
    /// [`CTL_PIPE_W`], because a poll now reads this one too).
    static SELF_PIPE_W: AtomicI32 = AtomicI32::new(-1);
    /// The control pipe's write end (s232): "drain thread, unblock the
    /// signal of the meaning whose index this byte is" (wolf-lang#483).
    /// Only the drain thread reads it — only a thread can change its
    /// own mask.
    static CTL_PIPE_W: AtomicI32 = AtomicI32::new(-1);
    /// The drain thread runs (s239: set after a spawn the host
    /// accepted; until then the next call to [`init`] asks again).
    static READER_UP: AtomicBool = AtomicBool::new(false);

    /// s232 (#691): trampolines running now, on any thread. A poll
    /// waits for this to be 0 before it reads the pipe, so a handler
    /// mid-write is not missed.
    static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
    /// s232 (#691): trampolines completed since the start. A poll lets
    /// pending signals be delivered on its own thread until a round
    /// delivers none ([`settle`]).
    static DELIVERED: AtomicUsize = AtomicUsize::new(0);

    /// The meanings whose signal the drain thread has unblocked on
    /// itself, and the condvar `unblock` waits on until it has.
    static UNBLOCKED: Mutex<i64> = Mutex::new(0);
    static UNBLOCKED_CV: Condvar = Condvar::new();

    /// A close-on-exec pipe: linux `pipe2(O_CLOEXEC)`; macOS has no
    /// pipe2, so `pipe` + two `fcntl(F_SETFD, FD_CLOEXEC)` — the exact
    /// widening `[os.signal.platform]` pre-authorized (s59). The
    /// set-race window pipe2 closes (a concurrent fork+exec between
    /// pipe and fcntl) does not arise here: the hub initializes once,
    /// under OnceLock, before any wolf code could spawn a process off
    /// it.
    fn cloexec_pipe() -> std::io::Result<[RawFd; 2]> {
        let mut fds = [0i32; 2];
        // SAFETY: plain pipe creation; result checked.
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
        if rc != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(fds)
    }

    fn set_nonblocking(fd: RawFd) {
        // SAFETY: our own fd; standard nonblocking flip.
        unsafe {
            let fl = libc::fcntl(fd, libc::F_GETFL);
            libc::fcntl(fd, libc::F_SETFL, fl | libc::O_NONBLOCK);
        }
    }

    /// Build the self-pipe and the control pipe; returns their read
    /// ends (the write ends publish through [`SELF_PIPE_W`] and
    /// [`CTL_PIPE_W`]). The self-pipe is nonblocking at BOTH ends: the
    /// handler must never block, and since s232 a poll reads it too and
    /// must never wait.
    pub fn make_pipes() -> std::io::Result<(RawFd, RawFd)> {
        let sig = cloexec_pipe()?;
        let ctl = match cloexec_pipe() {
            Ok(ctl) => ctl,
            Err(e) => {
                // SAFETY: the first pair is ours and published nowhere.
                unsafe {
                    libc::close(sig[0]);
                    libc::close(sig[1]);
                }
                return Err(e);
            }
        };
        set_nonblocking(sig[0]);
        set_nonblocking(sig[1]);
        set_nonblocking(ctl[1]);
        SELF_PIPE_W.store(sig[1], Ordering::Relaxed);
        CTL_PIPE_W.store(ctl[1], Ordering::Relaxed);
        Ok((sig[0], ctl[0]))
    }

    /// Whether the drain thread runs.
    pub fn running() -> bool {
        READER_UP.load(Ordering::SeqCst)
    }

    /// Spawn the one drain thread, unless it runs — or answer the
    /// host's refusal (s239); the hub's making serializes the callers.
    pub fn init(h: &'static Hub) -> std::io::Result<()> {
        if !READER_UP.load(Ordering::SeqCst) {
            std::thread::Builder::new()
                .name("wolf-signal".into())
                .spawn(move || reader_main(h))?;
            READER_UP.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Map a single meaning bit to its platform signal number — the
    /// whole table, the eight send-only meanings included (s232).
    pub fn to_signal(m: i64) -> Option<i32> {
        match m {
            meaning::RELOAD => Some(libc::SIGHUP),
            meaning::TERMINATE => Some(libc::SIGTERM),
            meaning::QUIT => Some(libc::SIGQUIT),
            meaning::UPGRADE => Some(libc::SIGUSR2),
            meaning::INTERRUPT => Some(libc::SIGINT),
            meaning::SUSPEND => Some(libc::SIGTSTP),
            meaning::BG_READ => Some(libc::SIGTTIN),
            meaning::BG_WRITE => Some(libc::SIGTTOU),
            meaning::CHILD => Some(libc::SIGCHLD),
            meaning::USER => Some(libc::SIGUSR1),
            meaning::ALARM => Some(libc::SIGALRM),
            meaning::BROKEN_PIPE => Some(libc::SIGPIPE),
            meaning::ABORT => Some(libc::SIGABRT),
            meaning::CONTINUE => Some(libc::SIGCONT),
            meaning::RESIZE => Some(libc::SIGWINCH),
            meaning::URGENT => Some(libc::SIGURG),
            meaning::VIRTUAL_ALARM => Some(libc::SIGVTALRM),
            meaning::PROFILE_ALARM => Some(libc::SIGPROF),
            meaning::CPU_LIMIT => Some(libc::SIGXCPU),
            meaning::FILE_LIMIT => Some(libc::SIGXFSZ),
            meaning::KILL => Some(libc::SIGKILL),
            meaning::STOP => Some(libc::SIGSTOP),
            meaning::SEGMENT_FAULT => Some(libc::SIGSEGV),
            meaning::BUS_ERROR => Some(libc::SIGBUS),
            meaning::ARITHMETIC => Some(libc::SIGFPE),
            meaning::ILLEGAL => Some(libc::SIGILL),
            meaning::BREAKPOINT => Some(libc::SIGTRAP),
            meaning::BAD_SYSCALL => Some(libc::SIGSYS),
            _ => None,
        }
    }

    /// The signal of every meaning in `set`, for a spawn's `defaults`
    /// (s219): a fixed array the child's hook can walk without
    /// allocating; unused slots are 0.
    pub fn signals_of(set: i64) -> [i32; super::LISTENED_COUNT] {
        let mut out = [0i32; super::LISTENED_COUNT];
        for (i, &m) in super::MEANINGS.iter().enumerate() {
            if set & m != 0 {
                out[i] = to_signal(m).unwrap_or(0);
            }
        }
        out
    }

    /// Map a delivered signal number back to its meaning's INDEX, or
    /// `u8::MAX` for a signal no meaning is listened by. Async-signal-
    /// safe: integer compares only, called from the trampoline.
    fn index_of(sig: i32) -> u8 {
        let mut i = 0u8;
        while (i as usize) < super::LISTENED_COUNT {
            if to_signal(1i64 << i) == Some(sig) {
                return i;
            }
            i += 1;
        }
        u8::MAX
    }

    /// This thread's `errno`, which a handler must leave as it found it
    /// (signal-safety(7)): its `write` may set it.
    unsafe fn errno_slot() -> *mut libc::c_int {
        // SAFETY: the C library's per-thread errno location.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        unsafe {
            libc::__errno_location()
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        unsafe {
            libc::__error()
        }
    }

    /// The async-signal-safe trampoline: map the signal to its meaning's
    /// index and `write` that byte to the self-pipe. This is the ENTIRE
    /// handler — no allocation, no lock, no wolf code; two atomic
    /// counters around the write (s232, #691: a poll can tell a handler
    /// is mid-write and wait for it), and `errno` kept. A full
    /// (nonblocking) pipe drops the byte, which coalesces rapid
    /// repeats: standard signal semantics (real-time signals / queued
    /// siginfo are explicitly out of scope).
    extern "C" fn trampoline(sig: libc::c_int) {
        IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
        let byte = index_of(sig);
        let fd = SELF_PIPE_W.load(Ordering::Relaxed);
        if byte != u8::MAX && fd >= 0 {
            // SAFETY: `write` is async-signal-safe; a single-byte write
            // to the runtime's own nonblocking pipe. The return is
            // ignored on purpose — a full pipe (EAGAIN) drops,
            // coalescing. errno is saved and put back around it.
            unsafe {
                let slot = errno_slot();
                let saved = *slot;
                libc::write(fd, (&raw const byte).cast(), 1);
                *slot = saved;
            }
        }
        DELIVERED.fetch_add(1, Ordering::SeqCst);
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }

    /// Move every byte the self-pipe holds onto the queue, under the
    /// hub lock the caller holds; true when one moved. The pipe is
    /// nonblocking, so this never waits. Every reader of the pipe — the
    /// drain thread and each poll — reads it only here, under the lock,
    /// so the queue keeps the pipe's order and no byte is between the
    /// pipe and the queue while another reader looks (s232, #691).
    pub fn drain(h: &Hub, st: &mut HubState) -> bool {
        let mut buf = [0u8; 64];
        let mut moved = false;
        loop {
            // SAFETY: valid nonblocking read end; buffer live and sized.
            let n = unsafe { libc::read(h.read_fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                if n < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                break; // EAGAIN: empty (or the write end gone)
            }
            for &b in &buf[..n as usize] {
                if (b as usize) < super::LISTENED_COUNT {
                    st.queue.push_back(1i64 << b);
                    moved = true;
                }
            }
        }
        // s234 (`[os.signal.wait.fd]`): wake every descriptor wait. A
        // full wake pipe already holds a byte, which is all a wake is.
        if moved {
            for &w in &st.waiters {
                let one = 1u8;
                // SAFETY: a one-byte write to a nonblocking pipe the
                // registered wait owns until it deregisters, under the
                // lock it deregisters under.
                unsafe { libc::write(w, (&raw const one).cast(), 1) };
            }
        }
        moved
    }

    /// s234 (#719, `[os.signal.wait.fd]`): park until a meaning in
    /// `want` is queued (answered, taken off the queue) or the file `h`
    /// names has something to read — a byte, end of input, a hang-up —
    /// (0; nothing is read). A meaning wins when both are ready. The
    /// wait is `poll(2)` on the descriptor and a wake pipe of the call's
    /// own, registered in the hub under its lock BEFORE the queue is
    /// looked at for the last time, so a meaning queued after that look
    /// writes the pipe and none is missed. With no scope that can be
    /// killed the poll has no timeout: an idle wait wakes for nothing.
    /// Inside one it looks for a kill every 100 ms (the teardown point
    /// `os_signal_wait` is). Codes are `sig_code`'s, negated.
    pub fn wait_fd(h_fd: i64, want: i64) -> i64 {
        use std::os::fd::AsRawFd as _;
        let Some(file) = crate::fs::dup_of(h_fd) else {
            return -sig_code::IO;
        };
        let fd = file.as_raw_fd();
        // s239 (wolf-lang#750): a park asks the host for nothing it can
        // refuse with an abort — a hub or a wake pipe the host will not
        // give (a process at its descriptor limit) is `io`.
        let Ok(h) = super::try_hub() else {
            return -sig_code::IO;
        };
        let Ok(wake) = cloexec_pipe() else {
            return -sig_code::IO;
        };
        set_nonblocking(wake[0]);
        set_nonblocking(wake[1]);
        let out = crate::task::blocking(|| {
            let scope = crate::task::current_scope();
            let mut registered = false;
            loop {
                settle(super::installed(h));
                {
                    let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
                    drain(h, &mut st);
                    if let Some(m) = super::take(&mut st, want) {
                        st.waiters.retain(|&w| w != wake[1]);
                        return m;
                    }
                    if !registered {
                        st.waiters.push(wake[1]);
                        registered = true;
                    }
                }
                if let Some(s) = scope.as_ref()
                    && s.is_killed()
                {
                    let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
                    st.waiters.retain(|&w| w != wake[1]);
                    drop(st);
                    crate::task::kill_teardown_check(s);
                    return -sig_code::IO;
                }
                let timeout = if scope.is_some() { 100 } else { -1 };
                let (input, woke) = match wait_two(fd, wake[0], timeout) {
                    Ok(r) => r,
                    Err(e) if e == libc::EINTR || e == libc::EAGAIN => continue,
                    Err(_) => {
                        let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
                        st.waiters.retain(|&w| w != wake[1]);
                        return -sig_code::IO;
                    }
                };
                if woke {
                    let mut buf = [0u8; 16];
                    // SAFETY: our own nonblocking pipe; emptied, it
                    // answers EAGAIN.
                    while unsafe { libc::read(wake[0], buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
                }
                if input {
                    // One more look: a meaning that came with the input
                    // wins (the clause's order).
                    settle(super::installed(h));
                    let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
                    drain(h, &mut st);
                    st.waiters.retain(|&w| w != wake[1]);
                    return super::take(&mut st, want).unwrap_or(0);
                }
            }
        });
        // SAFETY: the wake pipe is ours and deregistered (every return
        // above removes it under the hub lock).
        unsafe {
            libc::close(wake[0]);
            libc::close(wake[1]);
        }
        drop(file);
        out
    }

    /// s234: wait until `fd` has something to read or `wake` has a
    /// byte (or `timeout_ms` passes; -1 never), answering (input, wake).
    /// `poll(2)` first; where the host's poll refuses the descriptor —
    /// macOS answers `POLLNVAL` for a terminal opened by path
    /// (`/dev/tty`), measured black-box, where `select(2)` on the same
    /// descriptor works — `select(2)`, for descriptors under
    /// `FD_SETSIZE`. `Err` is the `errno`.
    pub fn wait_two(fd: RawFd, wake: RawFd, timeout_ms: i32) -> Result<(bool, bool), i32> {
        let errno = || {
            std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO)
        };
        let mut fds = [
            libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: wake,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: two live pollfds.
        if unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout_ms) } < 0 {
            return Err(errno());
        }
        if fds[0].revents & libc::POLLNVAL == 0 {
            return Ok((fds[0].revents != 0, fds[1].revents != 0));
        }
        let limit = libc::FD_SETSIZE as i64;
        if i64::from(fd) >= limit || i64::from(wake) >= limit {
            return Err(libc::EBADF);
        }
        // SAFETY: a zeroed fd_set filled by FD_ZERO/FD_SET with two
        // descriptors under FD_SETSIZE; a timeval or none.
        unsafe {
            let mut set: libc::fd_set = std::mem::zeroed();
            libc::FD_ZERO(&mut set);
            libc::FD_SET(fd, &mut set);
            libc::FD_SET(wake, &mut set);
            // `suseconds_t` is i64 on linux and i32 on macOS: the conversion
            // is a no-op on one host and a widening on the other.
            #[allow(clippy::useless_conversion)]
            let mut tv = libc::timeval {
                tv_sec: (timeout_ms.max(0) / 1000).into(),
                tv_usec: ((timeout_ms.max(0) % 1000) * 1000).into(),
            };
            let tvp = if timeout_ms < 0 {
                std::ptr::null_mut()
            } else {
                &raw mut tv
            };
            let n = libc::select(
                fd.max(wake) + 1,
                &mut set,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                tvp,
            );
            if n < 0 {
                return Err(errno());
            }
            Ok((libc::FD_ISSET(fd, &set), libc::FD_ISSET(wake, &set)))
        }
    }

    /// s232 (#691, `[os.signal.poll]`): before a poll or a wait reads
    /// the pipe, let every signal of a listened meaning that is still
    /// PENDING for this process be delivered — on this thread, now —
    /// and wait out a trampoline running on another. POSIX
    /// `pthread_sigmask`: "if there are any pending unblocked signals
    /// after the call, at least one of those signals shall be delivered
    /// before the call returns". Measured on linux, a call that leaves
    /// the mask as it was delivers nothing, so the listened signals are
    /// first BLOCKED on this thread and then UNBLOCKED — the second call
    /// changes the mask, and a pending one is delivered before it
    /// returns — and the mask is put back as found; repeated until a
    /// round delivers nothing (bounded: a flood of signals must not
    /// hold a poll forever).
    pub fn settle(installed: i64) {
        let mut set_has = false;
        // SAFETY: a zeroed sigset filled by sigemptyset/sigaddset; the
        // mask is this thread's own, put back exactly as found.
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            for &m in &super::MEANINGS {
                if installed & m != 0
                    && let Some(sig) = to_signal(m)
                {
                    libc::sigaddset(&mut set, sig);
                    set_has = true;
                }
            }
            if set_has {
                for _ in 0..64 {
                    let before = DELIVERED.load(Ordering::SeqCst);
                    let mut old: libc::sigset_t = std::mem::zeroed();
                    libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut old);
                    libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
                    libc::pthread_sigmask(libc::SIG_SETMASK, &old, std::ptr::null_mut());
                    if DELIVERED.load(Ordering::SeqCst) == before {
                        break;
                    }
                }
            }
        }
        let mut spins = 0u32;
        while IN_FLIGHT.load(Ordering::SeqCst) != 0 {
            spins += 1;
            if spins < 128 {
                std::hint::spin_loop();
            } else {
                std::thread::yield_now();
            }
        }
    }

    /// Whether `sig` is blocked on the calling thread.
    fn blocked_here(sig: i32) -> bool {
        // SAFETY: a read of this thread's mask (SIG_BLOCK with an empty
        // set changes nothing).
        unsafe {
            let mut none: libc::sigset_t = std::mem::zeroed();
            let mut cur: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut none);
            libc::pthread_sigmask(libc::SIG_BLOCK, &none, &mut cur);
            libc::sigismember(&cur, sig) == 1
        }
    }

    /// The drain thread: wait (`poll(2)`, no lock held) until the
    /// self-pipe or the control pipe has a byte; move the self-pipe's
    /// onto the queue under the hub lock ([`drain`]) and wake waiters;
    /// carry out the control pipe's unblock requests. NORMAL thread
    /// context — locks, allocates, everything the handler cannot. The
    /// sanctioned "dedicated thread" shape (the reactor thread's twin).
    fn reader_main(h: &'static Hub) {
        let mut fds = [
            libc::pollfd {
                fd: h.read_fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: h.ctl_fd,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        loop {
            fds[0].revents = 0;
            fds[1].revents = 0;
            // SAFETY: two live pollfds; no timeout.
            let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
            if n < 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                return; // fatal to delivery — only reachable on teardown
            }
            if fds[1].revents != 0 {
                let mut buf = [0u8; 64];
                // SAFETY: the control pipe's read end is readable.
                let k = unsafe { libc::read(h.ctl_fd, buf.as_mut_ptr().cast(), buf.len()) };
                if k == 0 {
                    return; // write end closed (process teardown)
                }
                if k > 0 {
                    for &b in &buf[..k as usize] {
                        unblock_here(1i64 << b);
                    }
                }
            }
            if fds[0].revents != 0 {
                let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
                if drain(h, &mut st) {
                    h.cv.notify_all();
                }
                if fds[0].revents & libc::POLLIN == 0 {
                    return; // hang-up or error on our own pipe: teardown
                }
            }
        }
    }

    /// On the drain thread: unblock the meaning's signal on THIS thread
    /// (the only mask a thread can change is its own), then publish it.
    /// A signal already pending process-wide is delivered to the
    /// trampoline as soon as it is unblocked, so nothing sent while it
    /// was blocked is lost.
    fn unblock_here(m: i64) {
        if let Some(sig) = to_signal(m) {
            // SAFETY: a zeroed sigset filled by sigemptyset/sigaddset; the
            // old-mask pointer is null.
            unsafe {
                let mut set: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut set);
                libc::sigaddset(&mut set, sig);
                libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
            }
        }
        let mut done = UNBLOCKED.lock().unwrap_or_else(|p| p.into_inner());
        *done |= m;
        UNBLOCKED_CV.notify_all();
    }

    /// Have the drain thread unblock the signals of `armed` (only
    /// those), and return once it has: after `listen` returns, a signal
    /// the program armed reaches it whatever mask the process
    /// inherited. Each meaning is asked for once; the ask is one byte on
    /// the control pipe.
    pub fn unblock(armed: i64) {
        let mut done = UNBLOCKED.lock().unwrap_or_else(|p| p.into_inner());
        let want = armed & meaning::LISTENABLE;
        let missing = want & !*done;
        if missing == 0 {
            return;
        }
        let fd = CTL_PIPE_W.load(Ordering::Relaxed);
        if fd < 0 {
            return;
        }
        for bit in super::MEANINGS {
            if missing & bit != 0 {
                let byte = bit.trailing_zeros() as u8;
                // The write end is nonblocking; a full pipe (the drain
                // thread behind) is retried.
                loop {
                    // SAFETY: a one-byte write to the runtime's own pipe.
                    let n = unsafe { libc::write(fd, (&raw const byte).cast(), 1) };
                    if n == 1 {
                        break;
                    }
                    let e = std::io::Error::last_os_error().raw_os_error();
                    if e != Some(libc::EAGAIN) && e != Some(libc::EINTR) {
                        return;
                    }
                    drop(done);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    done = UNBLOCKED.lock().unwrap_or_else(|p| p.into_inner());
                }
            }
        }
        // Bounded: a drain thread gone at teardown must not hang
        // `listen`; the unblock is then simply not in force.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while *done & want != want {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return;
            }
            done = UNBLOCKED_CV
                .wait_timeout(done, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }

    /// Install the trampoline for the meaning's signal (`SA_RESTART`:
    /// pooled syscalls are not torn by a delivered signal; the
    /// reactor's `epoll_wait` handles EINTR regardless).
    pub fn listen_one(_h: &Hub, bit: i64) -> Result<(), ()> {
        let Some(sig) = to_signal(bit) else {
            return Err(());
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

    /// Set the meaning's signal to the host default or to ignored (s219,
    /// `[os.signal.disp]`) — `sigaction` with `SIG_DFL`/`SIG_IGN`, so a
    /// child inherits an ignore across `exec` exactly as POSIX says.
    pub fn set_disp(bit: i64, d: super::Disp) -> i64 {
        let Some(sig) = to_signal(bit) else {
            return sig_code::IO;
        };
        // SAFETY: a zeroed sigaction with a constant disposition.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = match d {
                super::Disp::Default => libc::SIG_DFL,
                super::Disp::Ignore => libc::SIG_IGN,
            };
            libc::sigemptyset(&mut sa.sa_mask);
            if libc::sigaction(sig, &sa, std::ptr::null_mut()) == 0 {
                sig_code::OK
            } else {
                sig_code::IO
            }
        }
    }

    /// s232 (#681, `[os.signal.ignored]`): the listenable meanings whose
    /// signal is ignored now — `sigaction` with a null new action reads
    /// the disposition and changes nothing.
    pub fn ignored() -> i64 {
        let mut out = 0;
        for &m in &super::MEANINGS {
            let Some(sig) = to_signal(m) else { continue };
            // SAFETY: a read-only sigaction query into a zeroed struct.
            unsafe {
                let mut old: libc::sigaction = std::mem::zeroed();
                if libc::sigaction(sig, std::ptr::null(), &mut old) == 0
                    && old.sa_sigaction == libc::SIG_IGN
                {
                    out |= m;
                }
            }
        }
        out
    }

    /// Send one signal to THIS process. s232 (#691): to the calling
    /// THREAD (`raise(3)`), which POSIX says does not return until the
    /// handler has — so the trampoline has written its byte before
    /// `os_signal_raise` returns, and a poll after it sees the meaning.
    /// (`kill(getpid())`, used before, is process-directed: the host may
    /// hand it to another thread, which runs the trampoline after the
    /// call returns — the measured 2,000 of 2,000.) When this thread
    /// blocks the signal (an inherited mask, s188) a thread-directed
    /// signal would pend here forever, so it is sent to the process
    /// instead and reaches the drain thread, which unblocked it. `IO`
    /// for anything but one listenable meaning, or a failed send.
    pub fn raise(m: i64) -> i64 {
        if !super::one_listenable(m) {
            return sig_code::IO;
        }
        let Some(sig) = to_signal(m) else {
            return sig_code::IO;
        };
        // SAFETY: raise / getpid + kill with a validated signal number.
        let rc = unsafe {
            if blocked_here(sig) {
                libc::kill(libc::getpid(), sig)
            } else {
                libc::raise(sig)
            }
        };
        if rc == 0 { sig_code::OK } else { sig_code::IO }
    }

    /// s232 (#141, `[os.signal.send]`): `kill(pid, sig)` for a pid > 0
    /// and one meaning, or signal 0 for `m == 0` (the existence and
    /// permission check). Codes are `os::proc_code`'s.
    pub fn send(pid: i64, m: i64) -> i64 {
        use crate::os::proc_code;
        let Ok(p) = libc::pid_t::try_from(pid) else {
            return proc_code::INVALID;
        };
        if p <= 0 || (m != 0 && !super::one_sendable(m)) {
            return proc_code::INVALID;
        }
        let sig = if m == 0 {
            0
        } else {
            match to_signal(m) {
                Some(s) => s,
                None => return proc_code::INVALID,
            }
        };
        // SAFETY: kill with a positive pid and a validated signal.
        if unsafe { libc::kill(p, sig) } == 0 {
            return proc_code::OK;
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ESRCH) => proc_code::NOT_FOUND,
            Some(libc::EPERM) => proc_code::DENIED,
            Some(libc::EINVAL) => proc_code::INVALID,
            _ => proc_code::IO,
        }
    }

    /// s232 (`[os.signal.number]`): the host's number for one meaning,
    /// or `-INVALID`.
    pub fn number(m: i64) -> i64 {
        if !super::one_sendable(m) {
            return -crate::os::proc_code::INVALID;
        }
        to_signal(m).map_or(-crate::os::proc_code::INVALID, i64::from)
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
    pub fn init(_h: &'static Hub) -> std::io::Result<()> {
        Ok(())
    }

    /// Nothing runs that could be missing.
    pub fn running() -> bool {
        true
    }

    /// No signal masks on windows: console control events are not
    /// blocked by one (wolf-lang#483 is a unix fact).
    pub fn unblock(_armed: i64) {}

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
    pub fn listen_one(_h: &Hub, bit: i64) -> Result<(), ()> {
        // s219: the terminal and child meanings have no console event
        // behind them — a listen for one is `io`, by name in the clause
        // (`[os.signal.platform]`).
        if bit & !meaning::V1 != 0 {
            return Err(());
        }
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

    /// s219: no `SIG_IGN`/`SIG_DFL` on windows — every disposition call
    /// is `unsupported`, by name (`[os.signal.disp]`).
    pub fn set_disp(_bit: i64, _d: super::Disp) -> i64 {
        sig_code::UNSUPPORTED
    }

    /// s232: nothing to settle — the console handler enqueues under the
    /// hub lock itself, so a delivery is on the queue when it returns.
    pub fn settle(_installed: i64) {}

    /// s232: no self-pipe on windows.
    pub fn drain(_h: &Hub, _st: &mut super::HubState) -> bool {
        false
    }

    /// s234 (`[os.signal.wait.fd]`): no descriptor wait beside the
    /// console — by name, after the shape.
    pub fn wait_fd(_h_fd: i64, _want: i64) -> i64 {
        -sig_code::UNSUPPORTED
    }

    /// s232 (`[os.signal.ignored]`): no dispositions to read — by name.
    pub fn ignored() -> i64 {
        -sig_code::UNSUPPORTED
    }

    /// s232 (`[os.signal.send]`): the console has no signal to send to
    /// another process — `unsupported` by name, after the shape.
    pub fn send(pid: i64, m: i64) -> i64 {
        use crate::os::proc_code;
        if pid <= 0 || pid > i64::from(i32::MAX) || (m != 0 && !super::one_sendable(m)) {
            return proc_code::INVALID;
        }
        proc_code::UNSUPPORTED
    }

    /// s232 (`[os.signal.number]`): no signal numbers on windows.
    pub fn number(m: i64) -> i64 {
        use crate::os::proc_code;
        if !super::one_sendable(m) {
            return -proc_code::INVALID;
        }
        -proc_code::UNSUPPORTED
    }
}

/// `os_signal_listen(set)` — register interest in a set of meanings,
/// installing the platform's delivery for each ([`sys::listen_one`]).
/// Idempotent per meaning. `IO` for a set that is empty or holds a bit
/// that is not a listenable meaning (s232, #679: before, such a bit was
/// dropped and the call answered ok), before anything changes; `IO` if
/// any install fails.
pub fn listen(mask: i64) -> i64 {
    if !listenable_set(mask) {
        return sig_code::IO;
    }
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
    let armed = st.installed & mask;
    // The unblock waits on the drain thread, which takes the hub lock to
    // deliver: release it first.
    drop(st);
    sys::unblock(armed);
    sig_code::OK
}

/// The oldest queued meaning in `want`, taken off the queue.
fn take(st: &mut HubState, want: i64) -> Option<i64> {
    let pos = st.queue.iter().position(|&m| m & want != 0)?;
    st.queue.remove(pos)
}

/// s232 (#691): the meanings installed now, read under the lock.
fn installed(h: &Hub) -> i64 {
    h.state.lock().unwrap_or_else(|p| p.into_inner()).installed
}

/// `os_signal_wait(set)` — park the calling task until a meaning in
/// `set` is delivered, returning that meaning. Parks through
/// `task::blocking` (pool compensation, c19). A KILL teardown point
/// (`[conc.proc.kill]`): a killed supervisor terminates at the wait
/// instead of hanging on a signal that never comes. Plain cancellation
/// keeps waiting (the net tier's kill-only posture — the `{io}` row
/// vocabulary has no cancellation row). A set [`listen`] would refuse
/// is `-IO` (nothing could ever arrive — never a hang). Its first look
/// is a poll's (s232, #691): a signal delivered or pending before the
/// call is taken without parking.
pub fn wait(mask: i64) -> i64 {
    if !listenable_set(mask) {
        return -sig_code::IO;
    }
    let want = mask;
    let h = hub();
    sys::settle(installed(h));
    blocking(|| {
        let scope = current_scope();
        let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            sys::drain(h, &mut st);
            if let Some(m) = take(&mut st, want) {
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

/// `os_signal_ignore(set)` / `os_signal_default(set)` (s219,
/// `[os.signal.disp]`): every meaning in `set` stops being listened and
/// its signal is ignored, or does what the host does by default. A set
/// [`listen`] would refuse is `IO` before anything changes; windows is
/// `UNSUPPORTED`. Meanings already queued stay queued for a `wait` or
/// `poll` — they arrived while listened.
pub fn set_disposition(mask: i64, d: Disp) -> i64 {
    if !listenable_set(mask) {
        return sig_code::IO;
    }
    for &bit in &MEANINGS {
        if mask & bit == 0 {
            continue;
        }
        let rc = sys::set_disp(bit, d);
        if rc != sig_code::OK {
            return rc;
        }
        if let Some(h) = HUB.get() {
            h.state.lock().unwrap_or_else(|p| p.into_inner()).installed &= !bit;
        }
    }
    sig_code::OK
}

/// `os_signal_poll(set)` (s219, `[os.signal.poll]`): the oldest queued
/// meaning in `set`, taken off the queue, or 0 when none has arrived —
/// never a wait. A program that never listened has nothing queued (and
/// no hub is built for the question). A set [`listen`] would refuse is
/// `-IO`. **Exact since s232 (#691):** it first lets every pending
/// signal of a listened meaning be delivered on this thread and waits
/// out a trampoline mid-write ([`sys::settle`]), then reads the
/// self-pipe itself under the hub lock ([`sys::drain`]) — so a signal
/// whose trampoline has run, or that is pending for this process, is on
/// the queue before the poll looks.
pub fn poll(mask: i64) -> i64 {
    if !listenable_set(mask) {
        return -sig_code::IO;
    }
    let Some(h) = HUB.get() else {
        return 0;
    };
    sys::settle(installed(h));
    let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
    sys::drain(h, &mut st);
    take(&mut st, mask).unwrap_or(0)
}

/// `os_signal_wait_fd(fd, set)` (s234, wolf-lang#719,
/// `[os.signal.wait.fd]`): park until a meaning in `set` is delivered —
/// answered and taken off the queue, as [`wait`] does — or the file `fd`
/// names has something to read (0; nothing is read). A set [`listen`]
/// would refuse is `-IO` (shape first), then windows `-UNSUPPORTED`,
/// then a closed or forged handle `-IO`. The exactness of [`poll`] holds
/// for its looks.
pub fn wait_fd(fd: i64, mask: i64) -> i64 {
    if !listenable_set(mask) {
        return -sig_code::IO;
    }
    sys::wait_fd(fd, mask)
}

/// s232 (#681, `[os.signal.ignored]`): the listenable meanings whose
/// signal is ignored now, or `-UNSUPPORTED` (windows). Read before the
/// program's first `ignore`, `default` or `listen` of a meaning, it is
/// the disposition the process started with.
pub fn ignored() -> i64 {
    sys::ignored()
}

/// s232 (#141, `[os.signal.send]`): one meaning to process `pid`, or
/// with `m == 0` the check that one could be sent. `os::proc_code`'s
/// codes (0 ok, 1 not_found, 2 denied, 5 unsupported, 6 invalid, 4 io).
pub fn send(pid: i64, m: i64) -> i64 {
    sys::send(pid, m)
}

/// s232 (`[os.signal.number]`): the host's number for one meaning, or
/// a negated `os::proc_code` (`-INVALID`, `-UNSUPPORTED`).
pub fn number(m: i64) -> i64 {
    sys::number(m)
}

/// The signal numbers of the meanings in `set`, for a child's hook
/// (s219's spawn `defaults`); 0 slots are unused. Empty on windows.
#[cfg(unix)]
pub fn signals_of(set: i64) -> [i32; LISTENED_COUNT] {
    sys::signals_of(set)
}

/// `os_signal_raise(meaning)` — send one signal to THIS process (the
/// self-send companion to `os_kill`'s send-to-child; the deterministic
/// loopback the witness rides, and a real capability: a program
/// triggering its own reload path). `kill(getpid(), sig)` on unix; the
/// in-process enqueue on windows ([`sys::raise`]). `IO` for anything
/// but one listenable meaning, or a failed send.
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

/// `os_signal_ignore(set: int) -> () ! {io, unsupported}` (s219) — 0 ok,
/// 1 io, 2 unsupported.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_ignore(mask: i64) -> i64 {
    set_disposition(mask, Disp::Ignore)
}

/// `os_signal_default(set: int) -> () ! {io, unsupported}` (s219) — 0
/// ok, 1 io, 2 unsupported.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_default(mask: i64) -> i64 {
    set_disposition(mask, Disp::Default)
}

/// `os_signal_poll(set: int) -> int ! {io}` (s219) — the meaning taken
/// (> 0), 0 for none yet, or `-IO`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_poll(mask: i64) -> i64 {
    poll(mask)
}

/// `os_signal_wait_fd(fd: int, set: int) -> int ! {io, unsupported}`
/// (s234) — the meaning taken (> 0), 0 when `fd` has something to read,
/// or a negated `sig_code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_wait_fd(fd: i64, mask: i64) -> i64 {
    wait_fd(fd, mask)
}

/// `os_signal_ignored() -> int ! {unsupported}` (s232) — the ignored
/// set (>= 0), or `-UNSUPPORTED` (`sig_code`, 2).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_ignored() -> i64 {
    ignored()
}

/// `os_signal_send(pid: int, meaning: int) -> () ! {denied, invalid, io,
/// not_found, unsupported}` (s232) — 0 ok, else an `os::proc_code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_send(pid: i64, m: i64) -> i64 {
    send(pid, m)
}

/// `os_signal_number(meaning: int) -> int ! {invalid, unsupported}`
/// (s232) — the number (> 0), or a negated `os::proc_code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_number(m: i64) -> i64 {
    number(m)
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

    /// s219: the five new meanings ride the self-pipe by INDEX — BG_WRITE
    /// (128, the old control bit) and CHILD (256, past a byte) come back
    /// as themselves, and the trampoline sees SIGINT as INTERRUPT.
    #[cfg(unix)]
    #[test]
    fn new_meanings_round_trip_by_index() {
        let _g = serial();
        let set = meaning::INTERRUPT | meaning::BG_WRITE | meaning::CHILD;
        assert_eq!(listen(set), sig_code::OK);
        for m in [meaning::INTERRUPT, meaning::BG_WRITE, meaning::CHILD] {
            assert_eq!(raise(m), sig_code::OK);
            assert_eq!(wait(m), m, "meaning {m}");
        }
        // Leave SIGINT and SIGTTOU as the harness found them.
        assert_eq!(
            set_disposition(meaning::INTERRUPT | meaning::BG_WRITE, Disp::Default),
            sig_code::OK
        );
    }

    /// s219 (`[os.signal.poll]`): poll never waits — 0 with nothing
    /// queued, the meaning once one is, then 0 again; the set filters.
    #[cfg(unix)]
    #[test]
    fn poll_takes_what_arrived_and_never_waits() {
        let _g = serial();
        assert_eq!(listen(meaning::SUSPEND | meaning::BG_READ), sig_code::OK);
        assert_eq!(poll(meaning::SUSPEND), 0);
        assert_eq!(raise(meaning::BG_READ), sig_code::OK);
        // s232 (#691): no sleep — the poll reads the pipe itself.
        assert_eq!(poll(meaning::BG_READ | meaning::SUSPEND), meaning::BG_READ);
        assert_eq!(poll(meaning::BG_READ), 0);
        assert_eq!(poll(0), -sig_code::IO);
        assert_eq!(
            set_disposition(meaning::SUSPEND | meaning::BG_READ, Disp::Default),
            sig_code::OK
        );
    }

    /// s219 (`[os.signal.disp]`): an ignored INTERRUPT is not delivered
    /// (the process lives through its own SIGINT and nothing queues), and
    /// a child started now inherits the ignore across exec — SigIgn bit
    /// 1 in its /proc row — while a LISTENED one would come back default.
    #[cfg(target_os = "linux")]
    #[test]
    fn ignore_survives_and_is_inherited_listen_is_not() {
        let _g = serial();
        let sigint_bit = 1u64 << (libc::SIGINT - 1);
        let child_ign = || -> u64 {
            let out = std::process::Command::new("grep")
                .args(["SigIgn", "/proc/self/status"])
                .output()
                .expect("grep runs");
            let t = String::from_utf8_lossy(&out.stdout);
            u64::from_str_radix(t.split_whitespace().nth(1).expect("a SigIgn row"), 16)
                .expect("hex")
        };
        assert_eq!(
            set_disposition(meaning::INTERRUPT, Disp::Ignore),
            sig_code::OK
        );
        assert_eq!(raise(meaning::INTERRUPT), sig_code::OK);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(
            poll(meaning::INTERRUPT),
            0,
            "an ignored meaning queues nothing"
        );
        assert_ne!(
            child_ign() & sigint_bit,
            0,
            "ignored stays ignored across exec"
        );
        assert_eq!(listen(meaning::INTERRUPT), sig_code::OK);
        assert_eq!(
            child_ign() & sigint_bit,
            0,
            "listened becomes default across exec"
        );
        assert_eq!(
            set_disposition(meaning::INTERRUPT, Disp::Default),
            sig_code::OK
        );
    }

    /// s219: a disposition call with an empty set, or a bit outside the
    /// meanings, changes nothing and is `IO` — s232: `USER` (512) is a
    /// meaning now; `KILL` (send-only) and a bit past the table are not.
    #[test]
    fn disposition_of_a_bad_set_is_io() {
        let _g = serial();
        assert_eq!(set_disposition(0, Disp::Ignore), sig_code::IO);
        assert_eq!(set_disposition(meaning::KILL, Disp::Default), sig_code::IO);
        assert_eq!(set_disposition(1 << 40, Disp::Default), sig_code::IO);
        assert_eq!(
            set_disposition(meaning::RELOAD | (1 << 28), Disp::Ignore),
            sig_code::IO
        );
        assert_eq!(set_disposition(-1, Disp::Ignore), sig_code::IO);
    }

    /// s232 (#679, `[os.signal.set]`): one shape rule for every set —
    /// a listen of a set holding a bit that is not a listenable meaning
    /// is `IO` (it answered ok before, the bit dropped), and so are
    /// wait and poll of one; raise takes one listenable meaning.
    #[test]
    fn a_bit_outside_the_listenable_meanings_is_io_everywhere() {
        let _g = serial();
        for bad in [
            0,
            1 << 20,
            1 << 27,
            1 << 28,
            1 << 40,
            -1,
            meaning::RELOAD | (1 << 30),
        ] {
            assert_eq!(listen(bad), sig_code::IO, "listen {bad}");
            assert_eq!(poll(bad), -sig_code::IO, "poll {bad}");
            assert_eq!(wait(bad), -sig_code::IO, "wait {bad}");
        }
        for bad in [0, 3, meaning::KILL, meaning::STOP, 1 << 40, -16] {
            assert_eq!(raise(bad), sig_code::IO, "raise {bad}");
        }
        assert!(listenable_set(meaning::LISTENABLE));
        assert!(!listenable_set(meaning::SENDABLE));
    }

    /// s232 (#679): the eleven new catchable meanings ride the self-pipe
    /// by index, and each one's real signal reaches the trampoline.
    #[cfg(unix)]
    #[test]
    fn the_eleven_new_meanings_round_trip() {
        let _g = serial();
        let set = meaning::USER
            | meaning::ALARM
            | meaning::BROKEN_PIPE
            | meaning::ABORT
            | meaning::CONTINUE
            | meaning::RESIZE
            | meaning::URGENT
            | meaning::VIRTUAL_ALARM
            | meaning::PROFILE_ALARM
            | meaning::CPU_LIMIT
            | meaning::FILE_LIMIT;
        assert_eq!(listen(set), sig_code::OK);
        let mut m = meaning::USER;
        while m <= meaning::FILE_LIMIT {
            assert_eq!(raise(m), sig_code::OK);
            assert_eq!(poll(m), m, "meaning {m}");
            m <<= 1;
        }
        assert_eq!(set_disposition(set, Disp::Default), sig_code::OK);
    }

    /// s232 (#691, `[os.signal.poll]`): a poll right after a raise sees
    /// it — 2,000 rounds of 2,000, where trunk answered 0 in all of them.
    #[cfg(unix)]
    #[test]
    fn poll_right_after_raise_never_misses() {
        let _g = serial();
        assert_eq!(listen(meaning::INTERRUPT), sig_code::OK);
        let mut missed = 0;
        for _ in 0..2000 {
            assert_eq!(raise(meaning::INTERRUPT), sig_code::OK);
            if poll(meaning::INTERRUPT) != meaning::INTERRUPT {
                missed += 1;
                let _late = wait(meaning::INTERRUPT);
            }
        }
        assert_eq!(missed, 0, "poll after raise missed {missed} of 2000");
        assert_eq!(
            set_disposition(meaning::INTERRUPT, Disp::Default),
            sig_code::OK
        );
    }

    /// s232 (#691): a raise from a thread that blocks the signal still
    /// arrives (it goes to the process, and the drain thread takes it) —
    /// the thread-directed raise is used only where it can be delivered.
    #[cfg(unix)]
    #[test]
    fn raise_from_a_thread_that_blocks_it_still_arrives() {
        let _g = serial();
        assert_eq!(listen(meaning::ALARM), sig_code::OK);
        // SAFETY: this thread's own mask, put back below.
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            let mut old: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, libc::SIGALRM);
            libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut old);
            for _ in 0..50 {
                assert_eq!(raise(meaning::ALARM), sig_code::OK);
                assert_eq!(wait(meaning::ALARM), meaning::ALARM);
            }
            libc::pthread_sigmask(libc::SIG_SETMASK, &old, std::ptr::null_mut());
        }
        assert_eq!(set_disposition(meaning::ALARM, Disp::Default), sig_code::OK);
    }

    /// s232 (#681, `[os.signal.ignored]`): the read follows the
    /// disposition and changes nothing.
    #[cfg(unix)]
    #[test]
    fn ignored_reads_the_disposition() {
        let _g = serial();
        let before = ignored();
        assert!(before >= 0 && before & !meaning::LISTENABLE == 0);
        assert_eq!(set_disposition(meaning::USER, Disp::Ignore), sig_code::OK);
        assert_ne!(ignored() & meaning::USER, 0);
        assert_eq!(
            ignored() & meaning::USER,
            meaning::USER,
            "a read changes nothing"
        );
        assert_eq!(listen(meaning::USER), sig_code::OK);
        assert_eq!(ignored() & meaning::USER, 0, "a listen ends the ignore");
        assert_eq!(set_disposition(meaning::USER, Disp::Default), sig_code::OK);
        assert_eq!(ignored() & meaning::USER, 0);
    }

    /// s232 (#141, `[os.signal.send]`): the shape, then the host.
    #[cfg(unix)]
    #[test]
    fn send_rows() {
        use crate::os::proc_code;
        let _g = serial();
        let me = i64::from(std::process::id());
        assert_eq!(send(me, 0), proc_code::OK, "kill -0 of this process");
        for (pid, m) in [
            (0, 0),
            (-1, 0),
            (me, 3),
            (me, 1 << 28),
            (i64::MAX, 0),
            (me, -2),
        ] {
            assert_eq!(send(pid, m), proc_code::INVALID, "send({pid}, {m})");
        }
        // A pid no process has: the largest a pid_t holds.
        assert_eq!(send(i64::from(i32::MAX), 0), proc_code::NOT_FOUND);
        // A child ended by a sent TERMINATE.
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = i64::from(child.id());
        assert_eq!(send(pid, meaning::TERMINATE), proc_code::OK);
        use std::os::unix::process::ExitStatusExt as _;
        assert_eq!(child.wait().unwrap().signal(), Some(libc::SIGTERM));
        // And one ended by KILL, a send-only meaning.
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        assert_eq!(send(i64::from(child.id()), meaning::KILL), proc_code::OK);
        assert_eq!(child.wait().unwrap().signal(), Some(libc::SIGKILL));
    }

    /// s232 (`[os.signal.number]`): one meaning's number, the table's
    /// last bit included; anything else `-INVALID`.
    #[cfg(unix)]
    #[test]
    fn number_rows() {
        use crate::os::proc_code;
        assert_eq!(number(meaning::INTERRUPT), i64::from(libc::SIGINT));
        assert_eq!(number(meaning::KILL), 9);
        assert_eq!(number(meaning::BAD_SYSCALL), i64::from(libc::SIGSYS));
        assert_eq!(number(meaning::USER), i64::from(libc::SIGUSR1));
        for bad in [0, 3, 1 << 28, -1] {
            assert_eq!(number(bad), -proc_code::INVALID, "number {bad}");
        }
        let mut seen = std::collections::HashSet::new();
        for i in 0..MEANING_COUNT {
            let n = number(1 << i);
            assert!(n > 0 && seen.insert(n), "bit {i}: {n}");
        }
    }

    /// A pipe whose two ends are in the fs table: (read handle, write
    /// handle) — the descriptor `wait_fd` waits on.
    #[cfg(unix)]
    fn table_pipe() -> (i64, i64) {
        let (r, w) = std::io::pipe().expect("pipe");
        let rf = std::fs::File::from(std::os::fd::OwnedFd::from(r));
        let wf = std::fs::File::from(std::os::fd::OwnedFd::from(w));
        (crate::fs::mint_file(rf), crate::fs::mint_file(wf))
    }

    /// s234 (#719, `[os.signal.wait.fd]`): input waiting answers 0 and
    /// reads nothing; a meaning already delivered answers the meaning
    /// first, even with input waiting; the shape rule and a forged
    /// handle are `-IO`.
    #[cfg(unix)]
    #[test]
    fn wait_fd_answers_input_or_the_meaning() {
        use std::io::{Read as _, Write as _};
        let _g = serial();
        assert_eq!(listen(meaning::RESIZE), sig_code::OK);
        let (r, w) = table_pipe();
        assert_eq!(wait_fd(r, 0), -sig_code::IO, "empty set");
        assert_eq!(wait_fd(r, 1 << 20), -sig_code::IO, "KILL is not listenable");
        assert_eq!(wait_fd(1 << 40, meaning::RESIZE), -sig_code::IO, "forged");
        crate::fs::with_handle(w, |f| (&*f).write_all(b"k").unwrap()).unwrap();
        assert_eq!(wait_fd(r, meaning::RESIZE), 0, "input waiting");
        assert_eq!(
            wait_fd(r, meaning::RESIZE),
            0,
            "still waiting: nothing was read"
        );
        assert_eq!(raise(meaning::RESIZE), sig_code::OK);
        assert_eq!(
            wait_fd(r, meaning::RESIZE),
            meaning::RESIZE,
            "the meaning wins"
        );
        let mut b = [0u8; 1];
        crate::fs::with_handle(r, |f| (&*f).read_exact(&mut b).unwrap()).unwrap();
        assert_eq!(&b, b"k");
        assert_eq!(crate::fs::__wolf_rt_fs_close(w), 0);
        assert_eq!(crate::fs::__wolf_rt_fs_close(r), 0);
        assert_eq!(
            set_disposition(meaning::RESIZE, Disp::Default),
            sig_code::OK
        );
    }

    /// s234: a signal sent from another thread while the wait is parked
    /// on an empty descriptor wakes it promptly (no timer: the delivery
    /// path writes the wait's pipe), 200 rounds; and end of input (the
    /// write end closed) answers 0.
    #[cfg(unix)]
    #[test]
    fn wait_fd_wakes_for_a_signal_while_parked() {
        let _g = serial();
        assert_eq!(listen(meaning::USER), sig_code::OK);
        let (r, w) = table_pipe();
        let mut slow = 0;
        for _ in 0..200 {
            let t = std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(2));
                // SAFETY: a process-directed signal to ourselves.
                unsafe { libc::kill(libc::getpid(), libc::SIGUSR1) };
            });
            let t0 = std::time::Instant::now();
            assert_eq!(wait_fd(r, meaning::USER), meaning::USER);
            if t0.elapsed() > Duration::from_millis(50) {
                slow += 1;
            }
            t.join().unwrap();
        }
        assert_eq!(slow, 0, "{slow} of 200 waits took over 50 ms");
        assert_eq!(crate::fs::__wolf_rt_fs_close(w), 0);
        assert_eq!(wait_fd(r, meaning::USER), 0, "end of input is ready");
        assert_eq!(set_disposition(meaning::USER, Disp::Default), sig_code::OK);
    }
}
