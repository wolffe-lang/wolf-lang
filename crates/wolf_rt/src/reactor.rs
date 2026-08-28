//! The io reactor (s35; kqueue port s59) — one lazy readiness set
//! (epoll on linux, kqueue on macOS) plus a timer wheel,
//! composing the task layer (s32 park/blocking-compensation, s34 kill
//! teardown) with the net syscall floor (net.rs, the s39 v0 tier).
//!
//! # The model (D16/X6, at this campaign's depth)
//!
//! A task that must wait for io SUBMITS a wait — an fd interest, a
//! deadline, or both — and parks in a runtime-owned blocking point
//! until the reactor delivers the completion. The reactor is one
//! thread, lazily spawned on the FIRST submission (D15: a program
//! that performs no io carries no reactor thread, no epoll fd, no
//! timer state — the no_io test family holds it), multiplexing every
//! pending wait through one `epoll_wait`. v0's posture — each net
//! call kernel-blocked inside its own syscall, invisible to
//! cancellation and deadlines — becomes: the SYSCALL never blocks
//! (readiness is awaited first), the wait is a futex park the task
//! layer understands (compensation applies, kill teardown reaches it,
//! deadlines compose), and the kernel-side wait is centralized in the
//! reactor regardless of how many tasks are parked.
//!
//! Tasks still hold their threads while parked (D13 — no green
//! threads; blocking compensation keeps the pool's parallelism
//! whole). What the reactor changes is WHERE the wait happens and
//! what can end it: delivery, deadline, or teardown.
//!
//! # Timers (the `timer.fire` wheel)
//!
//! Deadlines ride a binary heap keyed by monotonic `Instant`; the
//! earliest pending deadline bounds `epoll_wait`'s sleep. A fired
//! deadline resolves its wait as [`IoWait::TimedOut`] and emits the
//! `timer.fire` seam kind — activated by s33's timeout arms, and per
//! `[sched.stable]` this wheel inherits the name (same kind, second
//! producer). This is what makes the net tier's `timeout` row
//! REACHABLE: net.rs arms per-socket deadlines through [`wait_fd`]'s
//! net flavor.
//!
//! # Schedule points (spec/07, X12)
//!
//! Completion delivery is a decision: WHICH pending completion is
//! delivered next, and when. It routes through the one seam as the
//! `io.arrive` kind, appended to `[sched.point.set]` by this sprint
//! per `[sched.stable]`'s append rule (the net module's reserved
//! completion-arrival note, activated). s36's `--chaos`
//! delay/reorder injection and the simulated reactor plug in at this
//! seam; nothing in the runtime's io path resolves a wait without
//! passing through it.
//!
//! # The io effect-token story (runtime depth: zero)
//!
//! WIR threads io operations on an effect token; like region tokens
//! it is ERASED at codegen — ordering is carried by the calls the
//! code already issued. The reactor adds no runtime token state and
//! no reordering: a completion only resolves a wait the program
//! submitted, and delivery happens-before the waiter's return (the
//! completion edge, the io analogue of `[conc.mm.hb.chan]`). The
//! token story stays purely static.
//!
//! # Lanes, precisely
//!
//! The NATIVE runtime (this crate, linked into compiled programs)
//! routes net waits through this module — linux and macOS, the task
//! layer's gate (s28/s59). The CHECKED lane
//! (`wolf_mem::ubcheck`'s net tier) keeps its v0 blocking-syscall
//! path: the checked machine is single-stepping a program under a
//! deterministic budget and owes no scheduling; its io story joins
//! the simulated reactor in s36, against this module's seam. Other
//! hosts keep v0 blocking net (the IOCP port sprint widens against
//! this interface, readiness adapted underneath — never the other
//! way around; kqueue crossed exactly that way at s59).
//!
//! # Cancellation
//!
//! [`wait_fd`] and [`sleep_until`] are cancellation points
//! (`[conc.cancel.points]`): a cancelled scope surfaces
//! [`IoWait::Cancelled`] as a value, and a KILLED proc's task
//! terminates at the wait by kill teardown (`[conc.proc.kill]`, no
//! further user code). The net flavor is kill-only: the net row
//! vocabulary `{refused, timeout, closed, io}` has no cancellation
//! row — surfacing one is the native lowering's row-ABI work, still
//! an honest refusal there — so a merely-cancelled net wait keeps
//! waiting (v0's kernel-blocked posture, inherited), while kill
//! teardown now reaches it (strictly more responsive than v0).

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Condvar, Mutex, Once, OnceLock};
use std::time::{Duration, Instant};

use crate::task::{SchedEvent, blocking, current_scope, kill_teardown_check, sched_point};

/// The platform poller behind the reactor: one kernel readiness set,
/// one wake channel, one blocking wait. Everything above this seam —
/// the waiter table, the timer wheel, cancellation, the `io.arrive`
/// schedule point — is shared verbatim across platforms; this is the
/// WHOLE per-OS surface (the s35 module doc's promised kqueue seam,
/// cashed at s59).
///
/// - **linux**: epoll (oneshot interests) + an eventfd wake token.
/// - **macOS**: kqueue (`EV_ONESHOT` interests keyed on (fd, filter))
///   + an `EVFILT_USER` wake event (`EV_CLEAR` self-resets, so there
///   is nothing to drain).
#[cfg(target_os = "linux")]
mod sys {
    use super::{Interest, RawFd};

    /// Reserved epoll user-data for the wake eventfd.
    const WAKE_TOKEN: u64 = u64::MAX;

    pub struct Poller {
        epfd: RawFd,
        wakefd: RawFd,
    }

    impl Poller {
        pub fn new() -> Poller {
            // SAFETY: plain fd creation; results checked below.
            let epfd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
            assert!(epfd >= 0, "epoll_create1 failed");
            // SAFETY: plain fd creation; result checked below.
            let wakefd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
            assert!(wakefd >= 0, "eventfd failed");
            let mut ev = libc::epoll_event {
                events: libc::EPOLLIN as u32,
                u64: WAKE_TOKEN,
            };
            // SAFETY: valid epfd/wakefd and a live event struct.
            let rc = unsafe { libc::epoll_ctl(epfd, libc::EPOLL_CTL_ADD, wakefd, &mut ev) };
            assert_eq!(rc, 0, "epoll_ctl(wakefd) failed");
            Poller { epfd, wakefd }
        }

        /// Arm an fd interest (oneshot) under `token`. `false` when
        /// the fd cannot be registered (forged/foreign).
        pub fn arm(&self, fd: RawFd, interest: Interest, token: u64) -> bool {
            let want = match interest {
                Interest::Read => libc::EPOLLIN,
                Interest::Write => libc::EPOLLOUT,
            };
            let mut ev = libc::epoll_event {
                events: (want | libc::EPOLLRDHUP | libc::EPOLLONESHOT) as u32,
                u64: token,
            };
            // SAFETY: valid epfd and a live event struct; fd is the
            // caller's open descriptor.
            let rc = unsafe { libc::epoll_ctl(self.epfd, libc::EPOLL_CTL_ADD, fd, &mut ev) };
            if rc == 0 {
                return true;
            }
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EEXIST) {
                // A previous oneshot on this fd is still in the set
                // (resolution raced the DEL): re-arm in place. One
                // pending wait per fd at a time is the v1 contract.
                // SAFETY: as above.
                let rc = unsafe { libc::epoll_ctl(self.epfd, libc::EPOLL_CTL_MOD, fd, &mut ev) };
                return rc == 0;
            }
            false
        }

        /// Remove `fd` from the set (resolution cleanup; a raced or
        /// already-gone fd is fine — the set self-heals under oneshot).
        pub fn del(&self, fd: RawFd) {
            // SAFETY: valid epfd; DEL takes no event struct.
            unsafe { libc::epoll_ctl(self.epfd, libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut()) };
        }

        /// Kick the poller out of its wait.
        pub fn wake(&self) {
            let one: u64 = 1;
            // SAFETY: valid eventfd; 8-byte write is the contract.
            unsafe { libc::write(self.wakefd, (&raw const one).cast(), 8) };
        }

        /// Block up to `timeout_ms` (-1 = until an event); hand every
        /// delivered completion token to `deliver`.
        pub fn wait(&self, timeout_ms: i32, mut deliver: impl FnMut(u64)) {
            // SAFETY: zeroed epoll_event array is a valid buffer.
            let mut events: [libc::epoll_event; 64] = unsafe { std::mem::zeroed() };
            // SAFETY: valid epfd and a live, correctly-sized buffer.
            let n = unsafe { libc::epoll_wait(self.epfd, events.as_mut_ptr(), 64, timeout_ms) };
            if n < 0 {
                return; // EINTR: the caller loops.
            }
            for ev in &events[..n as usize] {
                if ev.u64 == WAKE_TOKEN {
                    // Drain the level-triggered eventfd.
                    let mut buf: u64 = 0;
                    // SAFETY: valid nonblocking eventfd; 8-byte read
                    // is the contract, EAGAIN ends the drain.
                    unsafe { libc::read(self.wakefd, (&raw mut buf).cast(), 8) };
                    continue;
                }
                deliver(ev.u64);
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod sys {
    use super::{Interest, RawFd};

    /// The `EVFILT_USER` wake event's ident (arbitrary; the user
    /// filter namespaces idents separately from fds).
    const WAKE_IDENT: usize = 0;

    pub struct Poller {
        kq: RawFd,
    }

    impl Poller {
        pub fn new() -> Poller {
            // SAFETY: plain kqueue creation; result checked below.
            let kq = unsafe { libc::kqueue() };
            assert!(kq >= 0, "kqueue failed");
            // The wake channel: an EV_CLEAR user event — triggering it
            // wakes the wait, retrieval auto-resets it (no drain).
            let ev = libc::kevent {
                ident: WAKE_IDENT,
                filter: libc::EVFILT_USER,
                flags: libc::EV_ADD | libc::EV_CLEAR,
                fflags: 0,
                data: 0,
                udata: std::ptr::null_mut(),
            };
            // SAFETY: valid kq and a live changelist of one.
            let rc = unsafe { libc::kevent(kq, &ev, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
            assert_eq!(rc, 0, "kevent(EVFILT_USER add) failed");
            Poller { kq }
        }

        /// Arm an fd interest (oneshot) under `token`. kqueue keys
        /// events on (ident, filter), and `EV_ADD` on an existing pair
        /// updates it in place — the epoll EEXIST re-arm dance does
        /// not arise. `EV_RECEIPT` returns the change's own result so
        /// a forged/foreign fd answers `false` here instead of
        /// surfacing later.
        pub fn arm(&self, fd: RawFd, interest: Interest, token: u64) -> bool {
            let filter = match interest {
                Interest::Read => libc::EVFILT_READ,
                Interest::Write => libc::EVFILT_WRITE,
            };
            let ev = libc::kevent {
                ident: fd as usize,
                filter,
                flags: libc::EV_ADD | libc::EV_ONESHOT | libc::EV_RECEIPT,
                fflags: 0,
                data: 0,
                udata: token as usize as *mut libc::c_void,
            };
            let mut out: libc::kevent = unsafe { std::mem::zeroed() };
            // SAFETY: valid kq, one-entry changelist, one-entry
            // receipt buffer.
            let rc = unsafe { libc::kevent(self.kq, &ev, 1, &mut out, 1, std::ptr::null()) };
            rc == 1 && (out.flags & libc::EV_ERROR) != 0 && out.data == 0
        }

        /// Remove `fd`'s interests (both filters; ENOENT for the one
        /// not armed — or already delivered under EV_ONESHOT — is the
        /// self-healing no-op, exactly epoll's raced-DEL posture).
        pub fn del(&self, fd: RawFd) {
            for filter in [libc::EVFILT_READ, libc::EVFILT_WRITE] {
                let ev = libc::kevent {
                    ident: fd as usize,
                    filter,
                    flags: libc::EV_DELETE,
                    fflags: 0,
                    data: 0,
                    udata: std::ptr::null_mut(),
                };
                // SAFETY: valid kq and a live changelist of one.
                unsafe { libc::kevent(self.kq, &ev, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
            }
        }

        /// Kick the poller out of its wait (NOTE_TRIGGER on the user
        /// event).
        pub fn wake(&self) {
            let ev = libc::kevent {
                ident: WAKE_IDENT,
                filter: libc::EVFILT_USER,
                flags: 0,
                fflags: libc::NOTE_TRIGGER,
                data: 0,
                udata: std::ptr::null_mut(),
            };
            // SAFETY: valid kq and a live changelist of one.
            unsafe { libc::kevent(self.kq, &ev, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
        }

        /// Block up to `timeout_ms` (-1 = until an event); hand every
        /// delivered completion token to `deliver`.
        pub fn wait(&self, timeout_ms: i32, mut deliver: impl FnMut(u64)) {
            // SAFETY: zeroed kevent array is a valid receive buffer.
            let mut events: [libc::kevent; 64] = unsafe { std::mem::zeroed() };
            let ts;
            let tsp = if timeout_ms < 0 {
                std::ptr::null()
            } else {
                ts = libc::timespec {
                    tv_sec: (timeout_ms / 1000) as libc::time_t,
                    tv_nsec: i64::from(timeout_ms % 1000) * 1_000_000,
                };
                &ts as *const libc::timespec
            };
            // SAFETY: valid kq and a live, correctly-sized buffer.
            let n =
                unsafe { libc::kevent(self.kq, std::ptr::null(), 0, events.as_mut_ptr(), 64, tsp) };
            if n < 0 {
                return; // EINTR: the caller loops.
            }
            for ev in &events[..n as usize] {
                if ev.filter == libc::EVFILT_USER {
                    continue; // the wake event; EV_CLEAR already reset it
                }
                deliver(ev.udata as usize as u64);
            }
        }
    }
}

/// What a submitted wait is interested in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interest {
    /// Readable (data, a pending connection, EOF, or an error).
    Read,
    /// Writable (send space, or a connect's resolution).
    Write,
}

/// How a submitted wait resolved. Exactly once, always one of these
/// — no completion is ever dropped on the floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoWait {
    /// The fd interest is ready; the following syscall will not
    /// block (it surfaces data, EOF, or the real error itself).
    Ready,
    /// The deadline fired first (`timer.fire`).
    TimedOut,
    /// Cancellation surfaced at the blocking point
    /// (`[conc.cancel.points]`; on a killed proc's unwindable frame
    /// the task terminates instead — callers never see this then).
    Cancelled,
}

/// One parked waiter's completion cell.
struct WaitCell {
    st: Mutex<Option<IoWait>>,
    cv: Condvar,
}

impl WaitCell {
    fn new() -> WaitCell {
        WaitCell {
            st: Mutex::new(None),
            cv: Condvar::new(),
        }
    }

    /// Resolve the wait (first writer wins; exactly-once).
    fn complete(&self, out: IoWait) {
        let mut g = self.st.lock().unwrap();
        if g.is_none() {
            *g = Some(out);
            self.cv.notify_all();
        }
    }
}

/// One pending wait in the reactor's table.
struct Waiter {
    cell: Arc<WaitCell>,
    /// The registered fd, if this wait has an fd interest — removed
    /// from the epoll set by whoever resolves the wait.
    fd: Option<RawFd>,
}

struct State {
    /// Pending waits by submission token. Removing a token from this
    /// map is CLAIMING the wait: exactly one resolver wins.
    waiters: HashMap<u64, Waiter>,
    /// Deadlines (earliest first). Entries whose token is no longer
    /// pending are stale and skipped lazily.
    timers: BinaryHeap<Reverse<(Instant, u64)>>,
}

struct Reactor {
    sys: sys::Poller,
    next_token: AtomicU64,
    state: Mutex<State>,
}

static REACTOR: OnceLock<Reactor> = OnceLock::new();
static THREAD: Once = Once::new();

/// True once the reactor has ever been initialized — the no_io test
/// family's probe (D15: stays false for a program that performs no
/// io).
pub fn initialized() -> bool {
    REACTOR.get().is_some()
}

/// Lazy init: first submission creates the platform poller (epoll or
/// kqueue) with its wake channel, and the one reactor thread.
fn reactor() -> &'static Reactor {
    let r = REACTOR.get_or_init(|| Reactor {
        sys: sys::Poller::new(),
        next_token: AtomicU64::new(0),
        state: Mutex::new(State {
            waiters: HashMap::new(),
            timers: BinaryHeap::new(),
        }),
    });
    THREAD.call_once(|| {
        std::thread::Builder::new()
            .name("wolf-reactor".into())
            .spawn(move || reactor_main(r))
            .expect("spawn reactor thread");
    });
    r
}

impl Reactor {
    /// Arm an fd interest (oneshot) under `token`. `false` when the
    /// fd cannot be registered (forged/foreign) — the caller resolves
    /// the wait as Ready so the following syscall surfaces the real
    /// error row itself. The registration mechanics are the platform
    /// poller's ([`sys::Poller::arm`]).
    fn arm(&self, fd: RawFd, interest: Interest, token: u64) -> bool {
        self.sys.arm(fd, interest, token)
    }

    /// Remove `fd` from the readiness set (resolution cleanup; a
    /// raced or already-gone fd is fine — the set is self-healing
    /// under oneshot).
    fn del(&self, fd: RawFd) {
        self.sys.del(fd);
    }

    /// Kick the reactor thread out of its kernel wait (a new earliest
    /// deadline needs a shorter sleep).
    fn wake(&self) {
        self.sys.wake();
    }

    /// Milliseconds until the earliest LIVE deadline (stale timer
    /// entries pruned), or -1 for "sleep until an event".
    fn next_timeout_ms(&self) -> i32 {
        let mut st = self.state.lock().unwrap();
        while let Some(&Reverse((d, t))) = st.timers.peek() {
            if st.waiters.contains_key(&t) {
                let ms = d.saturating_duration_since(Instant::now()).as_millis();
                // Round up: never spin hot beneath a deadline.
                return i32::try_from(ms.saturating_add(1)).unwrap_or(i32::MAX);
            }
            st.timers.pop();
        }
        -1
    }

    /// Fire every due timer: claim the wait, emit `timer.fire`,
    /// resolve as TimedOut.
    fn fire_due_timers(&self) {
        loop {
            let mut st = self.state.lock().unwrap();
            let due = matches!(st.timers.peek(), Some(&Reverse((d, _))) if d <= Instant::now());
            if !due {
                return;
            }
            let Reverse((_, token)) = st.timers.pop().expect("peeked above");
            let w = st.waiters.remove(&token);
            drop(st);
            if let Some(w) = w {
                if let Some(fd) = w.fd {
                    self.del(fd);
                }
                sched_point(SchedEvent::TimerFire);
                w.cell.complete(IoWait::TimedOut);
            }
        }
    }
}

/// The reactor thread: one kernel-wait loop (`epoll_wait` /
/// `kevent`, behind [`sys::Poller::wait`]) delivering completions and
/// firing timers. Lives for the process once io has happened (idle
/// cost: one thread asleep in the kernel, zero when no io ever
/// happens — D15's lazy contract).
fn reactor_main(r: &'static Reactor) {
    loop {
        let timeout = r.next_timeout_ms();
        r.sys.wait(timeout, |token| {
            // Claim the wait; a missing token is a stale oneshot
            // whose waiter already resolved (cancel/timeout race) —
            // dropped, correctly.
            let w = r.state.lock().unwrap().waiters.remove(&token);
            if let Some(w) = w {
                if let Some(fd) = w.fd {
                    r.del(fd);
                }
                // The completion-arrival decision (spec/07
                // `io.arrive`): delivery order and timing route
                // through the one seam.
                sched_point(SchedEvent::IoArrive { token });
                w.cell.complete(IoWait::Ready);
            }
        });
        r.fire_due_timers();
    }
}

/// Park until the cell resolves; observe cancellation/kill while
/// parked (5ms poll backstop, the task layer's standard at
/// runtime-owned blocking points; real wakeups arrive via the
/// completion's notify).
fn wait_on(cell: &Arc<WaitCell>, token: u64, r: &'static Reactor, cancellable: bool) -> IoWait {
    let scope = current_scope();
    loop {
        {
            let mut g = cell.st.lock().unwrap();
            loop {
                if let Some(out) = *g {
                    return out;
                }
                let stop = scope
                    .as_ref()
                    .is_some_and(|s| (cancellable && s.is_cancelled()) || s.is_killed());
                if stop {
                    break;
                }
                let (g2, _) = cell.cv.wait_timeout(g, Duration::from_millis(5)).unwrap();
                g = g2;
            }
        }
        // Cancellation (or kill) requested: try to CLAIM the wait.
        // Losing the claim means delivery is in flight — loop; the
        // cell resolves shortly (exactly-once either way).
        let claimed = r.state.lock().unwrap().waiters.remove(&token);
        if let Some(w) = claimed {
            if let Some(fd) = w.fd {
                r.del(fd);
            }
            sched_point(SchedEvent::CancelCheck {
                scope: scope.as_ref().map_or(0, |s| s.id()),
                cancelled: true,
            });
            cell.complete(IoWait::Cancelled);
        }
    }
}

/// Submit one wait: an fd interest, a deadline, or both.
fn submit(fd: Option<(RawFd, Interest)>, deadline: Option<Instant>, cancellable: bool) -> IoWait {
    debug_assert!(
        fd.is_some() || deadline.is_some(),
        "a wait needs an fd interest or a deadline"
    );
    let r = reactor();
    let token = r.next_token.fetch_add(1, SeqCst);
    let cell = Arc::new(WaitCell::new());
    {
        // Waiter first, THEN arm: a completion arriving before the
        // waiter is visible would be dropped (lost wakeup).
        let mut st = r.state.lock().unwrap();
        st.waiters.insert(
            token,
            Waiter {
                cell: cell.clone(),
                fd: fd.map(|(f, _)| f),
            },
        );
        if let Some(d) = deadline {
            st.timers.push(Reverse((d, token)));
        }
    }
    if let Some((raw, interest)) = fd
        && !r.arm(raw, interest, token)
    {
        // Unregistrable fd: resolve as Ready so the caller's syscall
        // surfaces the real error itself (a bad handle is a row,
        // never a hang).
        if r.state.lock().unwrap().waiters.remove(&token).is_some() {
            cell.complete(IoWait::Ready);
        }
    }
    if deadline.is_some() {
        // A new deadline may be earlier than the reactor's current
        // sleep bound.
        r.wake();
    }
    let out = blocking(|| wait_on(&cell, token, r, cancellable));
    if out == IoWait::Cancelled
        && let Some(s) = current_scope()
    {
        // A killed proc's task terminates here ([conc.proc.kill]);
        // merely-cancelled (cancellable) waits return the value.
        kill_teardown_check(&s);
    }
    out
}

/// Wait until `fd` is ready for `interest`, the deadline fires, or
/// cancellation surfaces. A cancellation point and a schedule point.
pub fn wait_fd(fd: RawFd, interest: Interest, deadline: Option<Instant>) -> IoWait {
    submit(Some((fd, interest)), deadline, true)
}

/// The net tier's flavor (kill-only; see the module doc's
/// cancellation section): plain cancellation keeps waiting — the row
/// vocabulary has no cancellation row yet — while kill teardown
/// terminates the task at the wait. [`IoWait::Cancelled`] escapes
/// only on a non-unwindable (C/main) frame of a killed proc.
pub(crate) fn wait_fd_net(fd: RawFd, interest: Interest, deadline: Option<Instant>) -> IoWait {
    submit(Some((fd, interest)), deadline, false)
}

/// Park until `deadline` (the timer wheel alone): [`IoWait::TimedOut`]
/// when it fires — `timer.fire` — or [`IoWait::Cancelled`]. A
/// cancellation point.
pub fn sleep_until(deadline: Instant) -> IoWait {
    submit(None, Some(deadline), true)
}

// ---- reactor litmuses ----------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{ExitReason, scope, test_hook};
    use std::sync::atomic::AtomicUsize;

    /// Serialize hook-observing tests and collect every seam event;
    /// the hook clears when the guard drops.
    struct HookGuard(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

    impl Drop for HookGuard {
        fn drop(&mut self) {
            test_hook::set_test_hook(None);
        }
    }

    fn hook_serial() -> (HookGuard, Arc<Mutex<Vec<SchedEvent>>>) {
        let serial = test_hook::SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let events: Arc<Mutex<Vec<SchedEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        test_hook::set_test_hook(Some(Box::new(move |ev| {
            sink.lock().unwrap().push(*ev);
        })));
        (HookGuard(serial), events)
    }

    /// A pipe pair for readiness tests (raw, closed on drop).
    struct Pipe {
        r: RawFd,
        w: RawFd,
    }

    impl Pipe {
        fn new() -> Pipe {
            let mut fds = [0i32; 2];
            // SAFETY: plain pipe creation; checked below (pipe2 on
            // linux for atomic CLOEXEC; macOS has no pipe2 and a
            // test pipe outlives no exec anyway).
            #[cfg(target_os = "linux")]
            let rc = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
            #[cfg(not(target_os = "linux"))]
            let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
            assert_eq!(rc, 0, "pipe failed");
            Pipe {
                r: fds[0],
                w: fds[1],
            }
        }

        fn feed(&self, byte: u8) {
            // SAFETY: valid write end; 1-byte write.
            let n = unsafe { libc::write(self.w, (&raw const byte).cast(), 1) };
            assert_eq!(n, 1);
        }
    }

    impl Drop for Pipe {
        fn drop(&mut self) {
            // SAFETY: our own fds, closed once.
            unsafe {
                libc::close(self.r);
                libc::close(self.w);
            }
        }
    }

    /// Readiness completes a parked wait, and the completion arrival
    /// is the `io.arrive` seam kind (spec/07's appended point).
    #[test]
    fn readiness_completes_wait_and_emits_io_arrive() {
        let (_serial, events) = hook_serial();
        let p = Pipe::new();
        let done = Arc::new(AtomicUsize::new(0));
        let d2 = done.clone();
        let rfd = p.r;
        let r = scope("io-ready", |s| {
            s.spawn("waiter", move |_| {
                assert_eq!(wait_fd(rfd, Interest::Read, None), IoWait::Ready);
                d2.fetch_add(1, SeqCst);
                ExitReason::Normal
            });
            std::thread::sleep(Duration::from_millis(30));
            p.feed(7);
        });
        assert!(r.is_ok());
        assert_eq!(done.load(SeqCst), 1);
        let seen = events.lock().unwrap();
        assert!(
            seen.iter()
                .any(|e| matches!(e, SchedEvent::IoArrive { .. })),
            "no io.arrive event: {seen:?}"
        );
    }

    /// A deadline on a never-ready fd fires `timer.fire` and resolves
    /// TimedOut; the fd stays usable for a fresh wait afterwards
    /// (oneshot cleanup is real).
    #[test]
    fn deadline_fires_on_never_ready_fd() {
        let (_serial, events) = hook_serial();
        let p = Pipe::new();
        let t0 = Instant::now();
        assert_eq!(
            wait_fd(p.r, Interest::Read, Some(t0 + Duration::from_millis(40))),
            IoWait::TimedOut
        );
        assert!(t0.elapsed() >= Duration::from_millis(40));
        assert!(
            events
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, SchedEvent::TimerFire)),
            "no timer.fire event"
        );
        // Re-wait after data: Ready (the epoll entry was cleaned up).
        p.feed(1);
        assert_eq!(
            wait_fd(
                p.r,
                Interest::Read,
                Some(Instant::now() + Duration::from_secs(5))
            ),
            IoWait::Ready
        );
    }

    /// Timer ordering: completions deliver in deadline order (the
    /// wheel is a real ordered wheel, not a poll race) — stable under
    /// any schedule seed, since timer order is clock-driven, never
    /// PRNG-driven (the spec/07 stability posture where sched-ev/1
    /// reaches today).
    #[test]
    fn timers_fire_in_deadline_order() {
        let woke: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        let (w1, w2) = (woke.clone(), woke.clone());
        let r = scope("timer-order", |s| {
            // Spawned longest-first so schedule order and deadline
            // order disagree.
            s.spawn("long", move |_| {
                assert_eq!(
                    sleep_until(Instant::now() + Duration::from_millis(120)),
                    IoWait::TimedOut
                );
                w1.lock().unwrap().push("long");
                ExitReason::Normal
            });
            s.spawn("short", move |_| {
                assert_eq!(
                    sleep_until(Instant::now() + Duration::from_millis(30)),
                    IoWait::TimedOut
                );
                w2.lock().unwrap().push("short");
                ExitReason::Normal
            });
        });
        assert!(r.is_ok());
        assert_eq!(*woke.lock().unwrap(), vec!["short", "long"]);
    }

    /// One reactor thread serves many parked waiters (completion
    /// ROUTING, the X6 shape: pending completions are cheap; tasks
    /// exist because we spawned them, not because the reactor needs
    /// them).
    #[test]
    fn many_waiters_one_reactor() {
        const N: usize = 8;
        let pipes: Vec<Pipe> = (0..N).map(|_| Pipe::new()).collect();
        let done = Arc::new(AtomicUsize::new(0));
        let r = scope("fan-in", |s| {
            for (i, p) in pipes.iter().enumerate() {
                let rfd = p.r;
                let done = done.clone();
                s.spawn(&format!("w{i}"), move |_| {
                    assert_eq!(wait_fd(rfd, Interest::Read, None), IoWait::Ready);
                    done.fetch_add(1, SeqCst);
                    ExitReason::Normal
                });
            }
            std::thread::sleep(Duration::from_millis(30));
            for p in &pipes {
                p.feed(1);
            }
        });
        assert!(r.is_ok());
        assert_eq!(done.load(SeqCst), N);
    }

    /// An io wait is a cancellation point (`[conc.cancel.points]`):
    /// a failing sibling cancels the scope and the parked sleep
    /// surfaces Cancelled as a value, long before its deadline.
    #[test]
    fn cancellation_surfaces_at_parked_io_wait() {
        let t0 = Instant::now();
        let r = scope("io-cancel", |s| {
            s.spawn("sleeper", |_| {
                match sleep_until(Instant::now() + Duration::from_secs(30)) {
                    IoWait::Cancelled => ExitReason::Cancelled,
                    other => panic!("expected cancellation, got {other:?}"),
                }
            });
            std::thread::sleep(Duration::from_millis(20));
            s.spawn("failer", |_| ExitReason::Error { tag: 7 });
        });
        assert_eq!(r, Err(ExitReason::Error { tag: 7 }));
        assert!(
            t0.elapsed() < Duration::from_secs(10),
            "sleep ran to deadline"
        );
    }
}
