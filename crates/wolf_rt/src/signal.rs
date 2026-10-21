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
    /// s247 (`[os.proc.fork]`): atomics, because a forked child closes the
    /// ends it inherited (they are the parent's pipes) and puts its own in
    /// their place before `os_fork` returns there.
    #[cfg(unix)]
    read_fd: std::sync::atomic::AtomicI32,
    #[cfg(unix)]
    ctl_fd: std::sync::atomic::AtomicI32,
    state: Mutex<HubState>,
    cv: Condvar,
}

struct HubState {
    /// Meanings whose delivery (a `sigaction` handler; the console
    /// handler) is installed.
    installed: i64,
    /// Delivered meanings, oldest first (a coalescing edge queue).
    queue: VecDeque<i64>,
    /// s234 (#719, `[os.signal.wait.fd]`), s246 (#781): the descriptor
    /// waits parked now share ONE wake pipe, made before `main` with the
    /// hub (`__wolf_rt_signal_wait_fd_start`), so no call makes a
    /// descriptor while the program runs. Whoever moves a meaning onto the
    /// queue ([`sys::drain`], under this lock) starts a new generation and,
    /// unless the pipe holds its byte already, writes one; the byte stays
    /// until every wait registered at that generation has looked at the
    /// queue (`wake_unacked` reaches 0), and the last one to look empties
    /// the pipe. A wait blocked in `poll(2)` on its descriptor and the
    /// pipe therefore learns a meaning came, without a timer, and none
    /// takes another's wake. Until s246 each call made a pipe of its own,
    /// and its end could sit on a closed 0, 1 or 2 for the moment before
    /// the runtime moved it (measured at 0.2.32: `fs_fstat(1)` on another
    /// task answered ok 760,739 times in 2 s, kasumi).
    #[cfg(unix)]
    wake: WakeState,
}

/// s246 (#781): the shared wake pipe's bookkeeping, under the hub lock.
#[cfg(unix)]
#[derive(Default)]
struct WakeState {
    /// Descriptor waits registered now.
    waiters: usize,
    /// Bumped by every move of a meaning onto the queue while a wait is
    /// registered.
    generation: u64,
    /// Waits registered at `generation` that have not looked at the queue since.
    unacked: usize,
    /// The pipe holds its byte.
    full: bool,
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
                read_fd: std::sync::atomic::AtomicI32::new(read_fd),
                #[cfg(unix)]
                ctl_fd: std::sync::atomic::AtomicI32::new(ctl_fd),
                state: Mutex::new(HubState {
                    installed: 0,
                    queue: VecDeque::new(),
                    #[cfg(unix)]
                    wake: WakeState::default(),
                }),
                cv: Condvar::new(),
            })
        }
    };
    sys::init(h)?;
    // s247 (`[os.proc.fork]`): a forked child's first drain thread
    // unblocks what the program armed.
    #[cfg(unix)]
    sys::unblock_after_fork(h);
    Ok(h)
}

/// The hub made before `main` (s243, wolf-lang#763, `[os.signal.hub]`):
/// the entry shim calls this (through `__wolf_rt_signal_start`) in a
/// program that names a call which makes the hub — `os_signal_listen`,
/// `os_signal_wait`, `os_signal_wait_fd` — so none of them asks the host
/// for a pipe or a thread once the program runs. `Err` is the host's
/// refusal; nothing is left half-made that a later call cannot finish.
/// Until s243 the hub was made by the first `listen` or `wait` and its
/// refusal panicked (an abort: the entry points cannot unwind), so a
/// process that had used its descriptors up before it first listened
/// ended there. A caller that did not start it (no compiled program is
/// one) still meets [`try_hub`] at those calls, and they answer `io`.
pub fn start() -> std::io::Result<()> {
    try_hub().map(|_| ())
}

/// s246 (wolf-lang#781, `[os.signal.wait.fd]`): the shared wake pipe of
/// the descriptor waits, made before `main` (through
/// `__wolf_rt_signal_wait_fd_start`, after [`start`]) in a program that
/// names `os_signal_wait_fd`, so no wait makes a descriptor while other
/// threads run — the window between `pipe` and the move off 0–2 that
/// s245 left (wolf-lang#781) has no time to happen in. Windows has no
/// descriptor wait. `Err` is the host's refusal.
pub fn start_wait_fd() -> std::io::Result<()> {
    try_hub()?;
    #[cfg(unix)]
    sys::make_wake()?;
    Ok(())
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
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicUsize, Ordering};
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

    /// s247 (ruling #90, `[os.proc.fork]`): the drain thread's life, as
    /// the fork reads it — not made; made and not yet at its loop (its
    /// thread's start is running); at its loop, working; in its kernel
    /// wait (`poll`), holding nothing; claimed there by a fork (it does
    /// not go on past its wait until the fork releases it); ended
    /// (teardown).
    static READER_STATE: AtomicU8 = AtomicU8::new(READER_NONE);
    const READER_NONE: u8 = 0;
    const READER_STARTING: u8 = 1;
    const READER_LOOPING: u8 = 2;
    const READER_POLLING: u8 = 3;
    const READER_HELD: u8 = 4;
    const READER_GONE: u8 = 5;
    /// The drain thread itself, for the fork's unpark.
    static READER: Mutex<Option<std::thread::Thread>> = Mutex::new(None);
    /// s247: a forked child's drain thread, made at its first wait or
    /// listen, unblocks the meanings the program armed before it starts
    /// work (the parent's unblocks were on the parent's thread).
    static UNBLOCK_AFTER_FORK: AtomicBool = AtomicBool::new(false);

    /// s247: the drain thread back from its kernel wait — on, unless a fork
    /// claimed it there; then parked, holding nothing, until the fork
    /// releases it.
    fn back_from_poll() {
        while READER_STATE
            .compare_exchange(
                READER_POLLING,
                READER_LOOPING,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_err()
        {
            if READER_STATE.load(Ordering::SeqCst) != READER_HELD {
                return;
            }
            std::thread::park();
        }
    }

    /// s247: claim the drain thread for a fork where it holds nothing — in
    /// its kernel wait — waiting out a start or a moment of work (each
    /// bounded) first; nothing where there is no drain thread. The thread is
    /// not woken: if nothing comes it never knows.
    pub fn fork_hold() {
        let mut round = 0;
        loop {
            match READER_STATE.load(Ordering::SeqCst) {
                READER_STARTING | READER_LOOPING => crate::task::backoff(&mut round),
                READER_POLLING => {
                    if READER_STATE
                        .compare_exchange(
                            READER_POLLING,
                            READER_HELD,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        )
                        .is_ok()
                    {
                        return;
                    }
                }
                _ => return,
            }
        }
    }

    /// s247: the parent's side — the claim ends, and a drain thread that
    /// came back from its wait meanwhile is unparked.
    pub fn fork_release() {
        if READER_STATE
            .compare_exchange(
                READER_HELD,
                READER_POLLING,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
            && let Some(t) = READER.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
        {
            t.unpark();
        }
    }

    /// s247: after a forked child's [`init`] made its drain thread, the
    /// armed meanings unblocked on it (once).
    pub fn unblock_after_fork(h: &Hub) {
        if UNBLOCK_AFTER_FORK.swap(false, Ordering::SeqCst) {
            unblock(super::installed(h));
        }
    }

    /// s247: the child's descriptors — the hub's pipes it inherited are the
    /// parent's (a byte its trampoline wrote there would be read by the
    /// parent's drain thread), so each inherited end is closed and new
    /// pipes take their place (off 0–2, as at start), the wake pipe too if
    /// the parent had made it; the queue starts empty (a meaning delivered
    /// before the fork was the parent's; the host clears the child's
    /// pending signals) and the installed set stays (the dispositions are
    /// inherited). Runs with signals blocked on the only thread there is.
    /// The child's drain thread is made at its first wait or listen (a
    /// byte its trampoline writes before then waits in its own pipe, and a
    /// poll reads the pipe itself). `Err` is the host's refusal of a pipe.
    pub fn fork_child_pipes(h: &'static Hub) -> std::io::Result<()> {
        // A trampoline in flight on a parent's thread at the fork is not
        // this process's.
        IN_FLIGHT.store(0, Ordering::SeqCst);
        let had_wake = WAKE_R.load(Ordering::SeqCst) >= 0;
        for fd in [
            h.read_fd.swap(-1, Ordering::SeqCst),
            h.ctl_fd.swap(-1, Ordering::SeqCst),
            SELF_PIPE_W.swap(-1, Ordering::SeqCst),
            CTL_PIPE_W.swap(-1, Ordering::SeqCst),
            WAKE_R.swap(-1, Ordering::SeqCst),
            WAKE_W.swap(-1, Ordering::SeqCst),
        ] {
            if fd >= 0 {
                // SAFETY: this process's copy of an end the parent shares;
                // the parent's own stays open.
                unsafe { libc::close(fd) };
            }
        }
        READER_UP.store(false, Ordering::SeqCst);
        READER_STATE.store(READER_NONE, Ordering::SeqCst);
        *READER.lock().unwrap_or_else(|p| p.into_inner()) = None;
        *UNBLOCKED.lock().unwrap_or_else(|p| p.into_inner()) = 0;
        UNBLOCK_AFTER_FORK.store(true, Ordering::SeqCst);
        {
            let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
            st.queue.clear();
            st.wake = super::WakeState::default();
        }
        let (r, c) = make_pipes()?;
        h.read_fd.store(r, Ordering::SeqCst);
        h.ctl_fd.store(c, Ordering::SeqCst);
        if had_wake {
            make_wake()?;
        }
        Ok(())
    }

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
        // s245 (wolf-lang#778): never on a standard descriptor the
        // program was started without — the hub is made before `main`,
        // and a pipe on a closed 1 is a standard output that is open.
        crate::stdfd::pipe_off_std(fds)
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
            READER_STATE.store(READER_STARTING, Ordering::SeqCst);
            if let Err(e) = std::thread::Builder::new()
                .name("wolf-signal".into())
                .spawn(move || reader_main(h))
            {
                READER_STATE.store(READER_NONE, Ordering::SeqCst);
                return Err(e);
            }
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
            let n = unsafe {
                libc::read(
                    h.read_fd.load(Ordering::Relaxed),
                    buf.as_mut_ptr().cast(),
                    buf.len(),
                )
            };
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
        // s242: and every `os_signal_wait`, which parks on the condvar
        // with no timer — whoever drains (the drain thread, a poll, a
        // descriptor wait) wakes it, so a meaning another thread's look
        // moved onto the queue is never left unseen.
        if moved {
            h.cv.notify_all();
            // s246 (#781): one shared pipe, one byte a generation.
            if st.wake.waiters > 0 {
                st.wake.generation += 1;
                st.wake.unacked = st.wake.waiters;
                let w = WAKE_W.load(Ordering::Relaxed);
                if !st.wake.full && w >= 0 {
                    let one = 1u8;
                    // SAFETY: a one-byte write to the runtime's own
                    // nonblocking pipe, under the lock every reader of
                    // its state holds.
                    unsafe { libc::write(w, (&raw const one).cast(), 1) };
                    st.wake.full = true;
                }
            }
        }
        moved
    }

    /// s246 (#781): the shared wake pipe's ends (read, write), `-1` until
    /// made: before `main` in a program that names `os_signal_wait_fd`
    /// ([`make_wake`], through `__wolf_rt_signal_wait_fd_start`), else by
    /// the first descriptor wait of a runtime nothing started.
    static WAKE_R: AtomicI32 = AtomicI32::new(-1);
    static WAKE_W: AtomicI32 = AtomicI32::new(-1);
    static WAKE_MAKING: Mutex<()> = Mutex::new(());

    /// s246 (#781): make the shared wake pipe unless it is made — both
    /// ends nonblocking, close-on-exec and off 0–2 ([`cloexec_pipe`]).
    /// Made before `main`, nothing else runs while its end is on a closed
    /// 0, 1 or 2; `Err` is the host's refusal.
    pub fn make_wake() -> std::io::Result<RawFd> {
        let _one = WAKE_MAKING.lock().unwrap_or_else(|p| p.into_inner());
        let r = WAKE_R.load(Ordering::Acquire);
        if r >= 0 {
            return Ok(r);
        }
        let p = cloexec_pipe()?;
        set_nonblocking(p[0]);
        set_nonblocking(p[1]);
        WAKE_W.store(p[1], Ordering::Release);
        WAKE_R.store(p[0], Ordering::Release);
        Ok(p[0])
    }

    /// s246: a registered wait has looked at the queue at the current
    /// generation; the last of a generation's waits empties the pipe and
    /// tells the others, parked in [`wait_fd`]'s condvar wait, to poll.
    fn ack(h: &Hub, st: &mut HubState, seen: &mut u64) {
        if *seen == st.wake.generation {
            return;
        }
        *seen = st.wake.generation;
        st.wake.unacked = st.wake.unacked.saturating_sub(1);
        if st.wake.unacked == 0 && st.wake.full {
            let r = WAKE_R.load(Ordering::Relaxed);
            let mut buf = [0u8; 16];
            // SAFETY: our own nonblocking pipe; emptied, it answers
            // EAGAIN.
            while unsafe { libc::read(r, buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
            st.wake.full = false;
            h.cv.notify_all();
        }
    }

    /// s246: a wait leaves the shared pipe's bookkeeping.
    fn deregister(h: &Hub, st: &mut HubState, seen: &mut u64) {
        ack(h, st, seen);
        st.wake.waiters = st.wake.waiters.saturating_sub(1);
    }

    /// s234 (#719, `[os.signal.wait.fd]`): park until a meaning in
    /// `want` is queued (answered, taken off the queue) or the file `h`
    /// names has something to read (0; nothing is read). s248 (#795):
    /// the one-descriptor case of [`wait_fds`] with no deadline, its
    /// answers and rows unchanged. Codes are `sig_code`'s, negated.
    pub fn wait_fd(h_fd: i64, want: i64) -> i64 {
        match wait_fds(&[h_fd], want, -1) {
            Ok((-1, m)) => m,
            Ok(_) => 0,
            Err(c) => -c,
        }
    }

    /// s248 (#795, `[os.signal.wait.fds]`): park until a meaning in
    /// `want` is queued — `(-1, m)`, taken off the queue — or one of the
    /// files `handles` names has something to read (a byte, the end of
    /// input, a hang-up) — `(i, 0)`, the lowest such index, nothing read
    /// — or `deadline_ms` passes — `(-1, 0)`. A meaning wins when both
    /// are ready. `deadline_ms` 0 looks once and never parks; -1 never
    /// passes. The wait is `poll(2)` on the descriptors (duplicates held
    /// for the wait) and the shared wake pipe (s246, #781: made before
    /// `main`, [`make_wake`]), registered in the hub under its lock
    /// BEFORE the queue is looked at for the last time, so a meaning
    /// queued after that look writes the pipe and none is missed. With
    /// no deadline and no scope that can be killed the poll has no
    /// timeout: an idle wait wakes for nothing. Inside such a scope it
    /// looks for a kill every 100 ms (the teardown point `os_signal_wait`
    /// is). `Err` is a `sig_code`: a closed or forged handle, a hub or
    /// pipe the host will not give, a poll the host refuses — `IO`.
    pub fn wait_fds(handles: &[i64], want: i64, deadline_ms: i64) -> Result<(i64, i64), i64> {
        use std::os::fd::AsRawFd as _;
        use std::time::{Duration, Instant};
        let mut files = Vec::with_capacity(handles.len());
        for &h in handles {
            let Some(f) = crate::fs::dup_of(h) else {
                return Err(sig_code::IO);
            };
            files.push(f);
        }
        let fds: Vec<RawFd> = files.iter().map(|f| f.as_raw_fd()).collect();
        // s239 (wolf-lang#750): a park asks the host for nothing it can
        // refuse with an abort — a hub or a wake pipe the host will not
        // give (a process at its descriptor limit) is `io`. s246: in a
        // compiled program both are made before `main`, so this asks for
        // nothing; only a runtime nothing started makes the pipe here.
        let Ok(h) = super::try_hub() else {
            return Err(sig_code::IO);
        };
        let Ok(wake) = make_wake() else {
            return Err(sig_code::IO);
        };
        let end = u64::try_from(deadline_ms)
            .ok()
            .map(|ms| Instant::now() + Duration::from_millis(ms));
        // The time left before the deadline: `None` for none, a zero
        // duration once it has passed.
        let left = || end.map(|e| e.saturating_duration_since(Instant::now()));
        let out = crate::task::blocking(|| {
            let scope = crate::task::current_scope();
            let mut registered = false;
            let mut seen = 0u64;
            loop {
                settle(super::installed(h));
                let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
                drain(h, &mut st);
                if registered {
                    ack(h, &mut st, &mut seen);
                }
                if let Some(m) = super::take(&mut st, want) {
                    if registered {
                        deregister(h, &mut st, &mut seen);
                    }
                    return Ok((-1, m));
                }
                // s248: the deadline 0 — one look at the descriptors and
                // no park, so no registration (nothing waits for a wake).
                if left() == Some(Duration::ZERO) && !registered {
                    drop(st);
                    return match wait_many(&fds, wake, 0) {
                        Ok((Some(i), _)) => {
                            settle(super::installed(h));
                            let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
                            drain(h, &mut st);
                            Ok(super::take(&mut st, want).map_or((i as i64, 0), |m| (-1, m)))
                        }
                        Ok((None, _)) => Ok((-1, 0)),
                        Err(e) if e == libc::EINTR || e == libc::EAGAIN => Ok((-1, 0)),
                        Err(_) => Err(sig_code::IO),
                    };
                }
                if !registered {
                    st.wake.waiters += 1;
                    seen = st.wake.generation;
                    registered = true;
                }
                // The pipe still holds the byte of a generation this wait
                // has looked at: the waits that have not will empty it.
                // Until then a poll would answer at once, so wait for the
                // last of them (or a new generation, or the deadline) on
                // the hub's condvar.
                let mut timed_out = false;
                while st.wake.full && st.wake.generation == seen {
                    let cap = match (left(), scope.as_ref()) {
                        (Some(d), Some(_)) => Some(d.min(Duration::from_millis(100))),
                        (Some(d), None) => Some(d),
                        (None, Some(_)) => Some(Duration::from_millis(100)),
                        (None, None) => None,
                    };
                    if scope.as_ref().is_some_and(|s| s.is_killed()) {
                        break;
                    }
                    if left() == Some(Duration::ZERO) {
                        timed_out = true;
                        break;
                    }
                    st = match cap {
                        Some(d) => {
                            h.cv.wait_timeout(st, d)
                                .unwrap_or_else(|p| p.into_inner())
                                .0
                        }
                        None => h.cv.wait(st).unwrap_or_else(|p| p.into_inner()),
                    };
                }
                if st.wake.generation != seen {
                    continue;
                }
                if let Some(s) = scope.as_ref()
                    && s.is_killed()
                {
                    deregister(h, &mut st, &mut seen);
                    drop(st);
                    crate::task::kill_teardown_check(s);
                    return Err(sig_code::IO);
                }
                if timed_out {
                    deregister(h, &mut st, &mut seen);
                    return Ok((-1, 0));
                }
                drop(st);
                // The poll's timeout: the time left, capped at the kill
                // look's 100 ms inside a scope that can be killed; -1
                // (none) with neither.
                let timeout = match (left(), scope.is_some()) {
                    (Some(d), true) => d.min(Duration::from_millis(100)),
                    (Some(d), false) => d,
                    (None, true) => Duration::from_millis(100),
                    (None, false) => Duration::MAX,
                };
                let timeout = if timeout == Duration::MAX {
                    -1
                } else {
                    // Rounded UP to whole ms, so a wake never comes early.
                    i32::try_from(timeout.as_micros().div_ceil(1000)).unwrap_or(i32::MAX)
                };
                let ready = match wait_many(&fds, wake, timeout) {
                    Ok((ready, _woke)) => ready,
                    Err(e) if e == libc::EINTR || e == libc::EAGAIN => continue,
                    Err(_) => {
                        let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
                        deregister(h, &mut st, &mut seen);
                        return Err(sig_code::IO);
                    }
                };
                if let Some(i) = ready {
                    // One more look: a meaning that came with the input
                    // wins (the clause's order).
                    settle(super::installed(h));
                    let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
                    drain(h, &mut st);
                    deregister(h, &mut st, &mut seen);
                    return Ok(super::take(&mut st, want).map_or((i as i64, 0), |m| (-1, m)));
                }
                if left() == Some(Duration::ZERO) {
                    // The deadline: one last look at the queue.
                    settle(super::installed(h));
                    let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
                    drain(h, &mut st);
                    deregister(h, &mut st, &mut seen);
                    return Ok(super::take(&mut st, want).map_or((-1, 0), |m| (-1, m)));
                }
                // A wake (or the kill look's timeout): the next pass acks
                // the generation and looks at the queue.
            }
        });
        drop(files);
        out
    }

    /// s234: wait until one of `fds` has something to read or `wake` has
    /// a byte (or `timeout_ms` passes; -1 never), answering (the lowest
    /// ready index, wake). `poll(2)` first; where the host's poll refuses
    /// a descriptor — macOS answers `POLLNVAL` for a terminal opened by
    /// path (`/dev/tty`), measured black-box, where `select(2)` on the
    /// same descriptor works — `select(2)`, for descriptors under
    /// `FD_SETSIZE`. `Err` is the `errno`. s248 (#795): over several
    /// descriptors (it took one).
    pub fn wait_many(
        fds: &[RawFd],
        wake: RawFd,
        timeout_ms: i32,
    ) -> Result<(Option<usize>, bool), i32> {
        let errno = || {
            std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO)
        };
        let mut pfds: Vec<libc::pollfd> = fds
            .iter()
            .chain(std::iter::once(&wake))
            .map(|&fd| libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();
        let n = pfds.len();
        // SAFETY: `n` live pollfds.
        if unsafe { libc::poll(pfds.as_mut_ptr(), n as libc::nfds_t, timeout_ms) } < 0 {
            return Err(errno());
        }
        if pfds[..n - 1]
            .iter()
            .all(|p| p.revents & libc::POLLNVAL == 0)
        {
            let ready = pfds[..n - 1].iter().position(|p| p.revents != 0);
            return Ok((ready, pfds[n - 1].revents != 0));
        }
        let limit = libc::FD_SETSIZE as i64;
        if fds
            .iter()
            .chain(std::iter::once(&wake))
            .any(|&fd| i64::from(fd) >= limit)
        {
            return Err(libc::EBADF);
        }
        // SAFETY: a zeroed fd_set filled by FD_ZERO/FD_SET with
        // descriptors under FD_SETSIZE; a timeval or none.
        unsafe {
            let mut set: libc::fd_set = std::mem::zeroed();
            libc::FD_ZERO(&mut set);
            let mut top = wake;
            for &fd in fds.iter().chain(std::iter::once(&wake)) {
                libc::FD_SET(fd, &mut set);
                top = top.max(fd);
            }
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
                top + 1,
                &mut set,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                tvp,
            );
            if n < 0 {
                return Err(errno());
            }
            let ready = fds.iter().position(|&fd| libc::FD_ISSET(fd, &set));
            Ok((ready, libc::FD_ISSET(wake, &set)))
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
        *READER.lock().unwrap_or_else(|p| p.into_inner()) = Some(std::thread::current());
        READER_STATE.store(READER_LOOPING, Ordering::SeqCst);
        reader_loop(h);
        READER_STATE.store(READER_GONE, Ordering::SeqCst);
    }

    fn reader_loop(h: &'static Hub) {
        let read_fd = h.read_fd.load(Ordering::Relaxed);
        let ctl_fd = h.ctl_fd.load(Ordering::Relaxed);
        let mut fds = [
            libc::pollfd {
                fd: read_fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: ctl_fd,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        loop {
            fds[0].revents = 0;
            fds[1].revents = 0;
            // s247 (`[os.proc.fork]`): in the kernel's wait the thread holds
            // nothing, so a fork may claim it there without waking it.
            READER_STATE.store(READER_POLLING, Ordering::SeqCst);
            // SAFETY: two live pollfds; no timeout.
            let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
            back_from_poll();
            if n < 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                return; // fatal to delivery — only reachable on teardown
            }
            if fds[1].revents != 0 {
                let mut buf = [0u8; 64];
                // SAFETY: the control pipe's read end is readable.
                let k = unsafe { libc::read(ctl_fd, buf.as_mut_ptr().cast(), buf.len()) };
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

    /// s246 (wolf-lang#783, `[os.signal.disp.one]`): one meaning in the
    /// SENDING shape ([`host_signal`]: one bit of the table or `-N`, never
    /// 0) set ignored or back to the default. `KILL` and `STOP`, which no
    /// process can catch or ignore, are `INVALID` with the shape. A
    /// listenable meaning's signal — named by its bit or by its number —
    /// is the set call's ([`super::set_disposition`]: it ends a listen).
    /// `SEGMENT_FAULT` and `BUS_ERROR` are the stack guard's: the choice
    /// is recorded for a SENT one, and a real fault stays the runtime's
    /// (`task::stack`). Any other signal is `sigaction` with `SIG_IGN` or
    /// `SIG_DFL`, so a child inherits an ignore as POSIX says; the host's
    /// refusal is `IO`. s253 (wolf-lang#812, `[os.signal.host.reserved]`):
    /// the C library's own numbers ([`reserved`]: glibc's 32 and 33, whose
    /// `sigaction` refused them as `IO` until s253) are `INVALID`, by name,
    /// before the host is asked — the same on every C library.
    /// Codes are `os::proc_code`'s.
    pub fn set_disp_one(m: i64, d: super::Disp) -> i64 {
        use crate::os::proc_code;
        let sig = match host_signal(m, false) {
            Ok(sig) => sig,
            Err(c) => return c,
        };
        if sig == libc::SIGKILL || sig == libc::SIGSTOP || reserved(sig) {
            return proc_code::INVALID;
        }
        if let Some(&bit) = super::MEANINGS.iter().find(|&&b| to_signal(b) == Some(sig)) {
            return match super::set_disposition(bit, d) {
                sig_code::OK => proc_code::OK,
                _ => proc_code::IO,
            };
        }
        if crate::task::set_sent_fault(sig, matches!(d, super::Disp::Ignore)) {
            return proc_code::OK;
        }
        // SAFETY: a zeroed sigaction with a constant disposition.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = match d {
                super::Disp::Default => libc::SIG_DFL,
                super::Disp::Ignore => libc::SIG_IGN,
            };
            libc::sigemptyset(&mut sa.sa_mask);
            if libc::sigaction(sig, &sa, std::ptr::null_mut()) == 0 {
                proc_code::OK
            } else {
                proc_code::IO
            }
        }
    }

    /// s246 (wolf-lang#784, `[os.signal.exit]`): end THIS process by one
    /// meaning's signal (the sending shape), as a child's end by it reads
    /// to its parent: the signal set back to the default (the runtime's
    /// stack guard and any listen's trampoline let go of it), unblocked on
    /// the calling thread, and raised there — `raise(3)`, which does not
    /// return while the default ends the process. A process that lives on
    /// (the default of `CHILD`, `CONTINUE`, `URGENT` or `RESIZE` is to do
    /// nothing; a stop that is continued; a pid namespace's process 1,
    /// which the host does not end by a signal it has no handler for)
    /// exits with 128 + N. Only a shape refusal returns (`INVALID`).
    /// s253 (wolf-lang#812, `[os.signal.host.reserved]`): the C library's
    /// own numbers ([`reserved`]) are refused so, by name — until s253
    /// glibc's 32 and 33 never ended the process (its `sigaction` refuses
    /// the default and its `raise` does not deliver them: exit 160 and
    /// 161, measured on both tiers with and without a thread).
    pub fn exit_signal(m: i64) -> i64 {
        let sig = match host_signal(m, false) {
            Ok(sig) => sig,
            Err(c) => return c,
        };
        if reserved(sig) {
            return crate::os::proc_code::INVALID;
        }
        crate::prof::dump_on_exit();
        // s248 (`[os.term.restore]`): the restore before the raise — the
        // default ends the process with no runtime code after it.
        crate::term::restore_now();
        // SAFETY: a default disposition, an unblock and a raise of a
        // validated signal number on this thread.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = libc::SIG_DFL;
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(sig, &sa, std::ptr::null_mut());
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, sig);
            libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
            libc::raise(sig);
        }
        std::process::exit(128 + sig)
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

    /// s241 (#740, `[os.signal.host]`): the host's last signal number.
    /// linux: the C library's `SIGRTMAX` (64 under glibc and musl,
    /// measured by s240); macOS: 31 (`NSIG` 32, measured by s240); any
    /// other host is not measured, so it answers `unsupported` rather
    /// than guess.
    pub fn last() -> Option<libc::c_int> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            Some(libc::SIGRTMAX())
        }
        #[cfg(target_os = "macos")]
        {
            Some(31)
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
        {
            None
        }
    }

    /// s241 (`[os.signal.host]`): the C library's real-time range, or
    /// (0, 0) on a host with none (macOS). `None` where [`last`] is.
    pub fn realtime() -> Option<(libc::c_int, libc::c_int)> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            Some((libc::SIGRTMIN(), libc::SIGRTMAX()))
        }
        #[cfg(target_os = "macos")]
        {
            Some((0, 0))
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
        {
            None
        }
    }

    /// s241 (#740, `[os.signal.send]`, `[os.signal.host]`): the host
    /// signal a SENDING call's (or `os_signal_text`'s) meaning names —
    /// 0 for the meaning 0 when `zero` allows it, one bit of the table
    /// for its signal, or `-N` for the host's signal `N`, `1 <= N <=`
    /// [`last`]. Anything else is `INVALID`.
    /// s253 (wolf-lang#812, `[os.signal.host.reserved]`): whether host
    /// signal `sig` is the C library's own — on linux the kernel's
    /// real-time signals from its first, 32, up to the C library's first
    /// (`SIGRTMIN()`, which signal(7) says the library moves past the ones
    /// it keeps): glibc's 32 and 33 (nptl(7): its threads' cancellation
    /// and set-id signals; glibc installs its own handler on 33 when a
    /// process starts its second thread, measured), musl's 32 to 34. A
    /// program may send them and read their words; it may not set them or
    /// end itself by one. macOS has no real-time signals: none.
    pub fn reserved(sig: libc::c_int) -> bool {
        // s253 PLANT (never merge): no number is the C library's.
        let _ = sig;
        false
    }

    pub fn host_signal(m: i64, zero: bool) -> Result<libc::c_int, i64> {
        use crate::os::proc_code;
        if m == 0 {
            return if zero { Ok(0) } else { Err(proc_code::INVALID) };
        }
        if m < 0 {
            let Some(last) = last() else {
                return Err(proc_code::INVALID);
            };
            return match m.checked_neg().map(libc::c_int::try_from) {
                Some(Ok(n)) if (1..=last).contains(&n) => Ok(n),
                _ => Err(proc_code::INVALID),
            };
        }
        if !super::one_sendable(m) {
            return Err(proc_code::INVALID);
        }
        to_signal(m).ok_or(proc_code::INVALID)
    }

    /// `kill(target, sig)` and the host's answer as an `os::proc_code`
    /// (s232's map: `ESRCH` not_found, `EPERM` denied, `EINVAL` invalid,
    /// anything else io). The caller has checked the shape.
    fn kill_code(target: libc::pid_t, sig: libc::c_int) -> i64 {
        use crate::os::proc_code;
        // SAFETY: kill with a target and a signal the caller validated.
        if unsafe { libc::kill(target, sig) } == 0 {
            return proc_code::OK;
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ESRCH) => proc_code::NOT_FOUND,
            Some(libc::EPERM) => proc_code::DENIED,
            Some(libc::EINVAL) => proc_code::INVALID,
            _ => proc_code::IO,
        }
    }

    /// s232 (#141, `[os.signal.send]`): `kill(pid, sig)` for a pid > 0
    /// and one meaning (s241: or `-N`, a host number), or signal 0 for
    /// `m == 0` (the existence and permission check). Codes are
    /// `os::proc_code`'s.
    pub fn send(pid: i64, m: i64) -> i64 {
        use crate::os::proc_code;
        let Ok(p) = libc::pid_t::try_from(pid) else {
            return proc_code::INVALID;
        };
        if p <= 0 {
            return proc_code::INVALID;
        }
        match host_signal(m, true) {
            // s246 (macOS): a fault signal to this process is announced
            // first, so the stack guard can tell it from a fault.
            // SAFETY: getpid has no preconditions.
            Ok(sig) if p == unsafe { libc::getpid() } => self_kill(p, sig),
            Ok(sig) => kill_code(p, sig),
            Err(c) => c,
        }
    }

    /// s246: `kill_code` of a target that includes this process, the
    /// fault signals announced to the stack guard first
    /// (`task::note_self_send`) and the note taken back on a refusal.
    fn self_kill(target: libc::pid_t, sig: libc::c_int) -> i64 {
        let fault = sig == libc::SIGSEGV || sig == libc::SIGBUS;
        if fault {
            crate::task::note_self_send(sig, true);
        }
        let rc = kill_code(target, sig);
        if fault && rc != crate::os::proc_code::OK {
            crate::task::note_self_send(sig, false);
        }
        rc
    }

    /// s241 (#739, `[os.signal.send.group]`): `kill(-pgid, sig)` — every
    /// member of group `pgid` this process may signal. A `pgid` of 1 or
    /// less is `INVALID` before the host: `kill`'s `-1` is every process
    /// (`[os.signal.send.all]`), not group 1, and 0 is the caller's own
    /// group, which a program names by `os_pgid()`.
    pub fn send_group(pgid: i64, m: i64) -> i64 {
        use crate::os::proc_code;
        let Ok(g) = libc::pid_t::try_from(pgid) else {
            return proc_code::INVALID;
        };
        if g <= 1 {
            return proc_code::INVALID;
        }
        match host_signal(m, true) {
            // s246 (macOS): a send to this process's own group reaches it.
            // SAFETY: getpgrp has no preconditions.
            Ok(sig) if g == unsafe { libc::getpgrp() } => self_kill(-g, sig),
            Ok(sig) => kill_code(-g, sig),
            Err(c) => c,
        }
    }

    /// s241 (#739, `[os.signal.send.all]`): `kill(-1, sig)` — every
    /// process this one may signal, itself excepted (the host decides
    /// which; linux also leaves out its namespace's pid 1).
    pub fn send_all(m: i64) -> i64 {
        match host_signal(m, true) {
            Ok(sig) => kill_code(-1, sig),
            Err(c) => c,
        }
    }

    /// s241 (#741, `[os.signal.text]`): the C library's description of
    /// one signal, read in the C locale. `strsignal(3)` translates by the
    /// calling thread's locale (wolf never calls `setlocale`, so the
    /// global one is "C" already); the read sets the C locale on this
    /// thread around the call anyway (`uselocale(3)`), so the words do
    /// not depend on what an embedding process chose. The C library may
    /// answer from a buffer it reuses, so the read is serialized and
    /// copied out under one lock. s246 (wolf-lang#787): the meaning 0 is
    /// signal 0, and its words are `strsignal(0)`'s — glibc's `Unknown
    /// signal 0`, the row GNU `kill -t` and `kill -L` print for `EXIT`.
    pub fn text(m: i64) -> Result<String, i64> {
        static READ: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let sig = host_signal(m, true)?;
        let _one = READ
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: a locale made from the name "C" (or none), installed
        // on this thread only for the read and put back; the text is
        // copied out before the lock is let go.
        unsafe {
            let c_locale = libc::newlocale(libc::LC_ALL_MASK, c"C".as_ptr(), std::ptr::null_mut());
            let before = if c_locale.is_null() {
                std::ptr::null_mut()
            } else {
                libc::uselocale(c_locale)
            };
            let p = libc::strsignal(sig);
            let out = if p.is_null() {
                None
            } else {
                Some(std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned())
            };
            if !c_locale.is_null() {
                libc::uselocale(before);
                libc::freelocale(c_locale);
            }
            // POSIX says `strsignal` always answers a string; a C library
            // that answered none reads as the empty text, never a row the
            // call does not declare.
            Ok(out.unwrap_or_default())
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
        let Ok(h) = super::try_hub() else {
            return sig_code::IO;
        };
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

    /// s248 (`[os.signal.wait.fds]`): no descriptor wait beside the
    /// console — by name, after the shape.
    pub fn wait_fds(_handles: &[i64], _want: i64, _deadline_ms: i64) -> Result<(i64, i64), i64> {
        Err(sig_code::UNSUPPORTED)
    }

    /// s232 (`[os.signal.ignored]`): no dispositions to read — by name.
    pub fn ignored() -> i64 {
        -sig_code::UNSUPPORTED
    }

    /// s232 (`[os.signal.number]`): no signal numbers on windows.
    pub fn number(m: i64) -> i64 {
        use crate::os::proc_code;
        if !super::one_sendable(m) {
            return -proc_code::INVALID;
        }
        -proc_code::UNSUPPORTED
    }

    /// s241 (`[os.signal.host]`): windows has no signal numbers.
    pub fn last() -> Option<i32> {
        None
    }

    /// s241 (`[os.signal.host]`): no real-time range either.
    pub fn realtime() -> Option<(i32, i32)> {
        None
    }

    /// s241 (`[os.signal.send]`, `[os.signal.host]`): the SHAPE of a
    /// sending call's meaning on a host with no signal numbers — 0 (when
    /// `zero` allows it), one bit of the table, or any `-N` (windows has
    /// no last number to hold `N` to: a host number is past the shape
    /// and the call is `unsupported` with the rest). `Ok` carries no
    /// signal: nothing is sent here.
    pub fn host_signal(m: i64, zero: bool) -> Result<i32, i64> {
        use crate::os::proc_code;
        if (m == 0 && zero) || m < 0 || super::one_sendable(m) {
            Ok(0)
        } else {
            Err(proc_code::INVALID)
        }
    }

    /// s232 (`[os.signal.send]`): the console has no signal to send to
    /// another process — `unsupported` by name, after the shape.
    pub fn send(pid: i64, m: i64) -> i64 {
        use crate::os::proc_code;
        if pid <= 0 || pid > i64::from(i32::MAX) {
            return proc_code::INVALID;
        }
        match host_signal(m, true) {
            Ok(_) => proc_code::UNSUPPORTED,
            Err(c) => c,
        }
    }

    /// s241 (`[os.signal.send.group]`): no process groups for signals
    /// on windows — `unsupported` by name, after the shape.
    pub fn send_group(pgid: i64, m: i64) -> i64 {
        use crate::os::proc_code;
        if pgid <= 1 || pgid > i64::from(i32::MAX) {
            return proc_code::INVALID;
        }
        match host_signal(m, true) {
            Ok(_) => proc_code::UNSUPPORTED,
            Err(c) => c,
        }
    }

    /// s241 (`[os.signal.send.all]`): `unsupported` after the shape.
    pub fn send_all(m: i64) -> i64 {
        use crate::os::proc_code;
        match host_signal(m, true) {
            Ok(_) => proc_code::UNSUPPORTED,
            Err(c) => c,
        }
    }

    /// s241 (`[os.signal.text]`): no signal descriptions on windows —
    /// `unsupported` after the shape (s246: 0 is in the shape).
    pub fn text(m: i64) -> Result<String, i64> {
        host_signal(m, true)?;
        Err(crate::os::proc_code::UNSUPPORTED)
    }

    /// s246 (`[os.signal.disp.one]`): no dispositions on windows —
    /// `unsupported` after the shape (0, `KILL` and `STOP` are `invalid`).
    pub fn set_disp_one(m: i64, _d: super::Disp) -> i64 {
        use crate::os::proc_code;
        if m == 0 || m == meaning::KILL || m == meaning::STOP {
            return proc_code::INVALID;
        }
        match host_signal(m, false) {
            Ok(_) => proc_code::UNSUPPORTED,
            Err(c) => c,
        }
    }

    /// s246 (`[os.signal.exit]`): no signal to end by on windows —
    /// `unsupported` after the shape.
    pub fn exit_signal(m: i64) -> i64 {
        match host_signal(m, false) {
            Ok(_) => crate::os::proc_code::UNSUPPORTED,
            Err(c) => c,
        }
    }
}

/// `os_signal_listen(set)` — register interest in a set of meanings,
/// installing the platform's delivery for each ([`sys::listen_one`]).
/// Idempotent per meaning. `IO` for a set that is empty or holds a bit
/// that is not a listenable meaning (s232, #679: before, such a bit was
/// dropped and the call answered ok), before anything changes; `IO` if
/// any install fails, or if the host will not give the hub (s243,
/// #763: a program the entry shim started has it already).
pub fn listen(mask: i64) -> i64 {
    if !listenable_set(mask) {
        return sig_code::IO;
    }
    // s243 (#763): a hub the host will not give is `io`, never an abort.
    let Ok(h) = try_hub() else {
        return sig_code::IO;
    };
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

// ---- the fork (s247, ruling #90, `[os.proc.fork]`) ------------------------

/// s247: hold the hub's drain thread for a fork (nothing where the hub was
/// never made).
#[cfg(unix)]
pub(crate) fn fork_hold() {
    sys::fork_hold();
}

/// s247: the parent's side after the fork.
#[cfg(unix)]
pub(crate) fn fork_release() {
    sys::fork_release();
}

/// s247: the child's hub — its own pipes in place of the inherited ones,
/// with signals still blocked; its drain thread comes at its first wait
/// or listen. `Ok(false)`: no hub was made, nothing inherited.
#[cfg(unix)]
pub(crate) fn fork_child_pipes() -> std::io::Result<bool> {
    match HUB.get() {
        Some(h) => sys::fork_child_pipes(h).map(|()| true),
        None => Ok(false),
    }
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
/// is `-IO` (nothing could ever arrive — never a hang), and so is a hub
/// the host will not give (s243, #763). Its first look
/// is a poll's (s232, #691): a signal delivered or pending before the
/// call is taken without parking.
pub fn wait(mask: i64) -> i64 {
    if !listenable_set(mask) {
        return -sig_code::IO;
    }
    let want = mask;
    // s243 (#763): a hub the host will not give is `io`, never an abort.
    let Ok(h) = try_hub() else {
        return -sig_code::IO;
    };
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
            // s242 (wolf-lang#680, `[os.signal.wait]`): with no scope
            // that can be killed the wait has no timer — an idle wait
            // wakes for nothing (it was a 5 ms tick: about 195 context
            // switches a second, measured at 0.2.30). Whoever moves a
            // meaning onto the queue wakes it ([`sys::drain`] on unix,
            // [`deliver`] on windows). Inside a scope that can be killed
            // it still looks for a kill every 5 ms.
            st = match scope {
                Some(_) => {
                    h.cv.wait_timeout(st, Duration::from_millis(5))
                        .unwrap_or_else(|p| p.into_inner())
                        .0
                }
                None => h.cv.wait(st).unwrap_or_else(|p| p.into_inner()),
            };
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

/// `os_signal_wait_fds(fds, set, deadline_ms)` (s248, wolf-lang#795,
/// `[os.signal.wait.fds]`; ruling #93 R2 A): park until a meaning in `set`
/// is delivered — `(-1, m)`, taken off the queue as [`wait`] takes it —
/// or one of the files `fds` names has something to read — `(i, 0)`, the
/// lowest such index, nothing read — or `deadline_ms` passes — `(-1, 0)`.
/// The meaning wins when both are ready. 0 looks once, -1 waits for ever.
/// The order of refusal: a set [`listen`] would refuse is `IO` (shape
/// first, as [`wait_fd`]); a deadline below -1 `INVALID`; then windows
/// `UNSUPPORTED`, by name; then a closed or forged handle, or a hub or
/// pipe the host will not give, `IO`. Codes are `os::proc_code`'s.
/// [`wait_fd`] is its one-descriptor case with no deadline.
pub fn wait_fds(fds: &[i64], mask: i64, deadline_ms: i64) -> Result<(i64, i64), i64> {
    use crate::os::proc_code;
    if !listenable_set(mask) {
        return Err(proc_code::IO);
    }
    if deadline_ms < -1 {
        return Err(proc_code::INVALID);
    }
    sys::wait_fds(fds, mask, deadline_ms).map_err(|c| {
        if c == sig_code::UNSUPPORTED {
            proc_code::UNSUPPORTED
        } else {
            proc_code::IO
        }
    })
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

/// s241 (#739, `[os.signal.send.group]`): one meaning (or `-N`) to every
/// member of group `pgid` (> 1) this process may signal; `os::proc_code`'s
/// codes, as [`send`].
pub fn send_group(pgid: i64, m: i64) -> i64 {
    sys::send_group(pgid, m)
}

/// s241 (#739, `[os.signal.send.all]`): one meaning (or `-N`) to every
/// process this one may signal, itself excepted.
pub fn send_all(m: i64) -> i64 {
    sys::send_all(m)
}

/// s241 (#740, `[os.signal.host]`): the host's last signal number, or
/// `-UNSUPPORTED`.
pub fn last() -> i64 {
    sys::last().map_or(-crate::os::proc_code::UNSUPPORTED, i64::from)
}

/// s241 (`[os.signal.host]`): the C library's real-time range, (0, 0)
/// where there is none, or `Err(UNSUPPORTED)`.
pub fn realtime() -> Result<(i64, i64), i64> {
    sys::realtime()
        .map(|(lo, hi)| (i64::from(lo), i64::from(hi)))
        .ok_or(crate::os::proc_code::UNSUPPORTED)
}

/// s241 (#741, `[os.signal.text]`): the C library's words for one
/// meaning's signal (or `-N`'s), in the C locale; `Err(INVALID)` or
/// `Err(UNSUPPORTED)`.
pub fn text(m: i64) -> Result<String, i64> {
    sys::text(m)
}

/// s246 (#783, `[os.signal.disp.one]`): one meaning in the sending shape
/// — one bit of the table or `-N` — ignored or back to its default;
/// `os::proc_code`'s codes (0 ok, 6 invalid, 5 unsupported, 4 io).
pub fn set_disposition_one(m: i64, d: Disp) -> i64 {
    sys::set_disp_one(m, d)
}

/// s246 (#784, `[os.signal.exit]`): end this process by one meaning's
/// signal; returns only the shape's `INVALID` (or windows' `UNSUPPORTED`).
pub fn exit_signal(m: i64) -> i64 {
    sys::exit_signal(m)
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

/// `os_signal_ignore_one(meaning: int) -> () ! {invalid, io,
/// unsupported}` (s246) — 0 ok, else an `os::proc_code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_ignore_one(m: i64) -> i64 {
    set_disposition_one(m, Disp::Ignore)
}

/// `os_signal_default_one(meaning: int) -> () ! {invalid, io,
/// unsupported}` (s246) — 0 ok, else an `os::proc_code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_default_one(m: i64) -> i64 {
    set_disposition_one(m, Disp::Default)
}

/// `os_exit_signal(meaning: int) -> () ! {invalid, unsupported}` (s246)
/// — never returns for a meaning in the shape on unix; else an
/// `os::proc_code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_exit_signal(m: i64) -> i64 {
    exit_signal(m)
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

/// `os_signal_wait_fds(fds: List[int], set: int, deadline_ms: int) ->
/// (int, int) ! {invalid, io, unsupported}` (s248, wolf-lang#795) — 0 and
/// the pair (index or -1, meaning or 0) through `out`, or an
/// `os::proc_code`. The handles are the `List[int]`'s words, read before
/// the wait.
///
/// # Safety
///
/// `fds` must be a live `List[int]` header; `out` must address 16
/// writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_signal_wait_fds(
    fds: i64,
    mask: i64,
    deadline_ms: i64,
    out: i64,
) -> i64 {
    // Copied out before the wait: the list is the program's, and another
    // task may change it while this one is parked.
    let Some(handles) = (unsafe { crate::list::i64_elems(fds) }).map(<[i64]>::to_vec) else {
        return crate::os::proc_code::IO;
    };
    match wait_fds(&handles, mask, deadline_ms) {
        Ok((i, m)) => {
            unsafe { crate::str::write_pair(out, i, m) };
            crate::os::proc_code::OK
        }
        Err(c) => c,
    }
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

/// `os_signal_send_group(pgid: int, meaning: int) -> () ! {denied,
/// invalid, io, not_found, unsupported}` (s241) — 0 ok, else an
/// `os::proc_code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_send_group(pgid: i64, m: i64) -> i64 {
    send_group(pgid, m)
}

/// `os_signal_send_all(meaning: int) -> () ! {denied, invalid, io,
/// not_found, unsupported}` (s241) — 0 ok, else an `os::proc_code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_send_all(m: i64) -> i64 {
    send_all(m)
}

/// `os_signal_last() -> int ! {unsupported}` (s241) — the number (> 0),
/// or `-UNSUPPORTED` (an `os::proc_code`).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_signal_last() -> i64 {
    last()
}

/// `os_signal_realtime() -> (int, int) ! {unsupported}` (s241) — 0 and
/// the pair (first, last) through `out`, or the code.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_signal_realtime(out: i64) -> i64 {
    match realtime() {
        Ok((lo, hi)) => {
            unsafe { crate::str::write_pair(out, lo, hi) };
            crate::os::proc_code::OK
        }
        Err(c) => c,
    }
}

/// `os_signal_text(meaning: int) -> str ! {invalid, unsupported}`
/// (s241) — 0 and the text's (pointer, length) through `out`, the bytes
/// in the ambient region, or the code.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_signal_text(m: i64, out: i64) -> i64 {
    match text(m) {
        Ok(t) => {
            let p = crate::str::ambient_copy(t.as_bytes());
            unsafe { crate::str::write_pair(out, p as i64, t.len() as i64) };
            crate::os::proc_code::OK
        }
        Err(c) => c,
    }
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
        // s241: a negative meaning is a host number now, so the shape
        // rows hold `-N` past the host's last (never `-2`, which is
        // SIGINT to this very process).
        let past = -(last() + 1);
        for (pid, m) in [
            (0, 0),
            (-1, 0),
            (me, 3),
            (me, 1 << 28),
            (i64::MAX, 0),
            (me, past),
            (me, i64::MIN),
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

    /// s241 (#739, `[os.signal.send.group]`, `[os.signal.send.all]`):
    /// the shape rows, decided before the host — nothing is sent. Group
    /// 1 and below are `invalid` (`kill`'s -1 is every process, not
    /// group 1). Live sends are witnessed only inside a container
    /// (wolf_driver's `group_lanes`), never from a test on a host.
    #[cfg(unix)]
    #[test]
    fn group_and_all_shape_rows() {
        use crate::os::proc_code;
        let past = -(last() + 1);
        for (g, m) in [
            (1, 0),
            (0, 0),
            (-5, 0),
            (i64::from(i32::MAX) + 1, 0),
            (2, 3),
            (2, 1 << 28),
            (2, past),
            (2, i64::MIN),
        ] {
            assert_eq!(send_group(g, m), proc_code::INVALID, "send_group({g}, {m})");
        }
        for m in [3, 1 << 28, past, i64::MIN, -(1 << 40)] {
            assert_eq!(send_all(m), proc_code::INVALID, "send_all({m})");
        }
        // A group no process leads: the largest a pid_t holds.
        assert_eq!(
            send_group(i64::from(i32::MAX), 0),
            proc_code::NOT_FOUND,
            "signal 0 to a group nobody leads"
        );
    }

    /// s241 (#762, `[os.term.group]`): the shape rows only — the test
    /// process is never moved.
    #[cfg(unix)]
    #[test]
    fn set_pgid_shape_rows() {
        use crate::os::proc_code;
        for g in [-1, i64::MIN, i64::from(i32::MAX) + 1] {
            assert_eq!(crate::term::set_pgid(g), Err(proc_code::INVALID), "{g}");
        }
    }

    /// s241 (#740, `[os.signal.host]`): the last number, the real-time
    /// range and how a meaning or `-N` resolves, as s240 measured them.
    #[cfg(unix)]
    #[test]
    fn host_numbers() {
        use crate::os::proc_code;
        let last = last();
        let (lo, hi) = realtime().expect("a measured host");
        #[cfg(target_os = "linux")]
        {
            assert_eq!(last, 64);
            assert_eq!(hi, 64);
            assert!(lo == 34 || lo == 35, "glibc 34, musl 35: {lo}");
        }
        #[cfg(target_os = "macos")]
        {
            assert_eq!(last, 31);
            assert_eq!((lo, hi), (0, 0));
        }
        assert!(last > 0 && (lo, hi) <= (last, last));
        assert_eq!(sys::host_signal(-1, true), Ok(1));
        assert_eq!(
            sys::host_signal(-last, true),
            Ok(libc::c_int::try_from(last).unwrap())
        );
        assert_eq!(sys::host_signal(-(last + 1), true), Err(proc_code::INVALID));
        assert_eq!(sys::host_signal(0, true), Ok(0));
        assert_eq!(sys::host_signal(0, false), Err(proc_code::INVALID));
        assert_eq!(sys::host_signal(meaning::KILL, true), Ok(libc::SIGKILL));
    }

    /// s241 (#741, `[os.signal.text]`): the C library's words, as s240's
    /// tables recorded them (docs/s240/ev/*/sigtab-*-C.txt); the shape
    /// rows `invalid`.
    #[cfg(unix)]
    #[test]
    fn text_rows() {
        use crate::os::proc_code;
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        {
            assert_eq!(text(meaning::RELOAD).unwrap(), "Hangup");
            assert_eq!(text(meaning::TERMINATE).unwrap(), "Terminated");
            assert_eq!(text(meaning::KILL).unwrap(), "Killed");
            assert_eq!(text(-34).unwrap(), "Real-time signal 0");
        }
        #[cfg(target_os = "macos")]
        {
            assert_eq!(text(meaning::RELOAD).unwrap(), "Hangup: 1");
            assert_eq!(text(meaning::KILL).unwrap(), "Killed: 9");
        }
        for i in 0..MEANING_COUNT {
            assert!(!text(1 << i).unwrap().is_empty(), "bit {i}");
        }
        // s246 (#787): signal 0 has words, the C library's.
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        assert_eq!(text(0).unwrap(), "Unknown signal 0");
        let zero = text(0).expect("signal 0's words");
        assert!(!zero.is_empty());
        eprintln!("REPORT os_signal_text(0) = {zero:?}");
        for bad in [3, 1 << 28, -(last() + 1), i64::MIN] {
            assert_eq!(text(bad), Err(proc_code::INVALID), "text {bad}");
        }
    }

    /// s246 (#783, `[os.signal.disp.one]`): the sending shape's rows —
    /// 0, two bits, past the table, a host number past the last, and
    /// `KILL` and `STOP` by bit or by number are `INVALID` and change
    /// nothing; a host number with no meaning is set and read back with
    /// `sigaction`; glibc's own 32 is the host's refusal, `IO`. (The fault
    /// meanings are the stack guard's, and `exit_signal` ends the process:
    /// both are the driver's container witnesses, `signal_gaps_lanes.rs`.)
    #[cfg(unix)]
    #[test]
    fn disposition_one_rows() {
        use crate::os::proc_code;
        let _g = serial();
        let disp = |sig: libc::c_int| -> libc::sighandler_t {
            // SAFETY: a read-only sigaction query into a zeroed struct.
            unsafe {
                let mut old: libc::sigaction = std::mem::zeroed();
                assert_eq!(libc::sigaction(sig, std::ptr::null(), &mut old), 0);
                old.sa_sigaction
            }
        };
        for bad in [
            0,
            3,
            1 << 28,
            meaning::KILL,
            meaning::STOP,
            -9,
            -i64::from(libc::SIGSTOP),
            -(last() + 1),
            i64::MIN,
        ] {
            for d in [Disp::Ignore, Disp::Default] {
                assert_eq!(
                    set_disposition_one(bad, d),
                    proc_code::INVALID,
                    "one {bad} {d:?}"
                );
            }
        }
        // (`KILL` and `STOP` are in `os_exit_signal`'s shape: they would
        // end or stop this test, so only the shape's refusals run here.)
        for bad in [0, 3, 1 << 28, -(last() + 1), i64::MIN] {
            assert_eq!(exit_signal(bad), proc_code::INVALID, "exit {bad}");
        }
        #[cfg(target_os = "linux")]
        {
            let rt = libc::SIGRTMIN() + 6;
            assert_eq!(
                set_disposition_one(-i64::from(rt), Disp::Ignore),
                proc_code::OK
            );
            assert_eq!(disp(rt), libc::SIG_IGN, "a host number ignored");
            assert_eq!(
                set_disposition_one(-i64::from(rt), Disp::Default),
                proc_code::OK
            );
            assert_eq!(disp(rt), libc::SIG_DFL, "and back to the default");
        }
        // s253 (#812, `[os.signal.host.reserved]`): the C library's numbers
        // are refused by name, `invalid` (until s253 glibc's `sigaction`
        // refused 32 and 33 as `io`), and so is an end by one.
        #[cfg(target_os = "linux")]
        for n in 32..libc::SIGRTMIN() {
            let m = -i64::from(n);
            assert_eq!(
                set_disposition_one(m, Disp::Ignore),
                proc_code::INVALID,
                "the C library keeps {n} for itself"
            );
            assert_eq!(
                set_disposition_one(m, Disp::Default),
                proc_code::INVALID,
                "the C library keeps {n} for itself"
            );
            assert_eq!(exit_signal(m), proc_code::INVALID, "no end by {n}");
        }
        // A listenable meaning by its number is the set call's.
        let usr1 = -i64::from(libc::SIGUSR1);
        assert_eq!(set_disposition_one(usr1, Disp::Ignore), proc_code::OK);
        assert_eq!(disp(libc::SIGUSR1), libc::SIG_IGN);
        assert_eq!(
            set_disposition_one(meaning::USER, Disp::Default),
            proc_code::OK
        );
        assert_eq!(disp(libc::SIGUSR1), libc::SIG_DFL);
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

    /// s248 (#795, `[os.signal.wait.fds]`): over two pipes — the
    /// deadline (-1, 0) after it passed, the poll (deadline 0) at once,
    /// the lowest ready index, the meaning before the input; a deadline
    /// below -1 `invalid`, a bad set and a forged handle `io`.
    #[cfg(unix)]
    #[test]
    fn wait_fds_answers_the_deadline_an_index_or_the_meaning() {
        use crate::os::proc_code;
        use std::io::Write as _;
        let _g = serial();
        assert_eq!(listen(meaning::RESIZE), sig_code::OK);
        let (r0, w0) = table_pipe();
        let (r1, w1) = table_pipe();
        let t0 = std::time::Instant::now();
        assert_eq!(wait_fds(&[r0, r1], meaning::RESIZE, 25), Ok((-1, 0)));
        let took = t0.elapsed();
        assert!(
            took >= Duration::from_millis(25),
            "the deadline passed: {took:?}"
        );
        let t0 = std::time::Instant::now();
        assert_eq!(wait_fds(&[r0, r1], meaning::RESIZE, 0), Ok((-1, 0)));
        assert!(
            t0.elapsed() < Duration::from_millis(20),
            "a poll never parks"
        );
        assert_eq!(
            wait_fds(&[], meaning::RESIZE, 0),
            Ok((-1, 0)),
            "no descriptors"
        );
        crate::fs::with_handle(w1, |f| (&*f).write_all(b"b").unwrap()).unwrap();
        assert_eq!(wait_fds(&[r0, r1], meaning::RESIZE, -1), Ok((1, 0)));
        crate::fs::with_handle(w0, |f| (&*f).write_all(b"a").unwrap()).unwrap();
        assert_eq!(
            wait_fds(&[r0, r1], meaning::RESIZE, 0),
            Ok((0, 0)),
            "the lowest"
        );
        assert_eq!(raise(meaning::RESIZE), sig_code::OK);
        assert_eq!(
            wait_fds(&[r0, r1], meaning::RESIZE, -1),
            Ok((-1, meaning::RESIZE)),
            "the meaning wins"
        );
        assert_eq!(
            wait_fds(&[r0], meaning::RESIZE, -2),
            Err(proc_code::INVALID)
        );
        assert_eq!(wait_fds(&[r0], 0, 5), Err(proc_code::IO), "empty set");
        assert_eq!(
            wait_fds(&[r0], meaning::RESIZE, -2),
            Err(proc_code::INVALID)
        );
        assert_eq!(
            wait_fds(&[r0, 1 << 40], meaning::RESIZE, 5),
            Err(proc_code::IO)
        );
        for h in [w0, r0, w1, r1] {
            assert_eq!(crate::fs::__wolf_rt_fs_close(h), 0);
        }
        assert_eq!(
            set_disposition(meaning::RESIZE, Disp::Default),
            sig_code::OK
        );
    }
}
