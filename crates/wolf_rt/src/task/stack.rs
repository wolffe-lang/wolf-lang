//! Pooled, lazily-committed native stacks (s32 Target 3) — unix.
//!
//! **Reserve large, commit on fault**: each worker stack reserves a
//! large virtual span (`WOLF_TASK_STACK` bytes, default 8 MiB) with
//! `mmap(PROT_READ|PROT_WRITE, MAP_NORESERVE|MAP_ANON)`; the kernel's
//! demand paging commits page-by-page on first touch, so a task using
//! 12 KiB of stack costs ~12 KiB of RSS. The low end of the span is an
//! `mprotect(PROT_NONE)` **guard region**; hitting it is a
//! deterministic fault reported with the task's name (the SIGSEGV /
//! SIGBUS handler below). Portability note (contract): under strict
//! overcommit (`vm.overcommit_memory=2`) `MAP_NORESERVE` semantics
//! differ — wolf binaries on such hosts account full reservations; we
//! do not silently shrink the reserve.
//!
//! **Layout of one span** (low → high):
//!
//! ```text
//! [ guard 64K | thread stack (reserve, grows down) | altstack 32K ]
//! ```
//!
//! The signal altstack rides in the same mapping — separate from the
//! stack proper, so it is intact when the guard is hit — and recycles
//! with it.
//!
//! **Recycling**: stacks never unmap; a retiring worker's stack
//! returns to the pool (after `pthread_join` proves the thread is
//! gone — pool.rs owns that dance) and committed pages above a small
//! floor are released with `madvise(MADV_FREE)` (fallback
//! `MADV_DONTNEED`), so pooled stacks do not pin peak RSS. Stacks
//! never move, ever — FFI pointers into stack memory are legal exactly
//! as in C (the whole reason D13 rejected green threads; contrast the
//! Go register-calling/cgo-pointer machinery cited in mod.rs).
//!
//! Windows is stack_win.rs (s60b): the same surface over the kernel's
//! own `VirtualAlloc(MEM_RESERVE)` + `PAGE_GUARD` thread stacks and a
//! vectored exception handler for the report — its module doc says
//! why the span is the kernel's there and what (idle trim, the s36
//! commit-failure injection) is the s60c delta.

#![cfg(unix)]

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering::SeqCst};

/// Guard region at the low end. Larger than one page so big frames
/// cannot leap it undetected in the debug tier.
const GUARD: usize = 64 * 1024;
/// Signal altstack at the high end.
const ALT: usize = 32 * 1024;
/// Committed floor kept on trim (hot path stays warm).
const FLOOR: usize = 64 * 1024;
/// Default reserve for the stack proper.
const DEFAULT_RESERVE: usize = 8 << 20;

/// Reserve size for the stack proper (`WOLF_TASK_STACK`, bytes).
pub fn reserve_size() -> usize {
    use std::sync::OnceLock;
    static SIZE: OnceLock<usize> = OnceLock::new();
    *SIZE.get_or_init(|| {
        std::env::var("WOLF_TASK_STACK")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .map(|n| n.max(FLOOR + GUARD).next_multiple_of(page()))
            .unwrap_or(DEFAULT_RESERVE)
    })
}

fn page() -> usize {
    // SAFETY: sysconf is always callable.
    (unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).max(4096) as usize
}

/// One mapped span. Owned by the pool; never unmapped, never moved.
pub struct TaskStack {
    base: *mut u8,
    total: usize,
}

// SAFETY: the span is plain memory; ownership is handed between the
// pool and exactly one worker thread at a time.
unsafe impl Send for TaskStack {}

impl TaskStack {
    /// Map a fresh span (guard + stack + altstack) and register its
    /// guard range for the fault handler. `None` on mmap failure.
    pub fn map() -> Option<TaskStack> {
        let total = GUARD + reserve_size() + ALT;
        // SAFETY: fresh anonymous private mapping.
        let base = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                total,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE,
                -1,
                0,
            )
        };
        if base == libc::MAP_FAILED {
            return None;
        }
        let base = base.cast::<u8>();
        // No transparent huge pages on a task stack (s231): on a host
        // whose THP mode is `always`, the first touch of a stack could
        // fault in a whole 2 MiB page, so a parked task's stack cost
        // 2 MiB or 8 KiB depending on what the kernel had free (kasumi:
        // 64 of 2,031 stacks were huge, 45 % of the process's RSS). A
        // stack is touched a page at a time; it is paged that way.
        #[cfg(target_os = "linux")]
        // SAFETY: advice on our own fresh mapping; failure is harmless.
        unsafe {
            libc::madvise(base.cast(), total, libc::MADV_NOHUGEPAGE);
        }
        // SAFETY: protecting the low GUARD bytes of our own mapping.
        if unsafe { libc::mprotect(base.cast(), GUARD, libc::PROT_NONE) } != 0 {
            // SAFETY: unmapping the mapping we just made.
            unsafe { libc::munmap(base.cast(), total) };
            return None;
        }
        register_guard(base as usize, base as usize + GUARD);
        Some(TaskStack { base, total })
    }

    /// Low address of the thread stack proper (just above the guard).
    pub fn stack_lo(&self) -> *mut u8 {
        // SAFETY: in-bounds offset of our mapping.
        unsafe { self.base.add(GUARD) }
    }

    /// Byte length of the thread stack proper.
    pub fn stack_len(&self) -> usize {
        self.total - GUARD - ALT
    }

    /// Low address / length of the altstack region (top of the span).
    pub fn altstack(&self) -> (*mut u8, usize) {
        // SAFETY: in-bounds offset of our mapping.
        (unsafe { self.base.add(self.total - ALT) }, ALT)
    }

    /// Install this span's altstack on the calling thread (worker
    /// entry) so the overflow report has room to run.
    pub fn enter(&self) {
        let (sp, len) = self.altstack();
        let ss = libc::stack_t {
            ss_sp: sp.cast(),
            ss_flags: 0,
            ss_size: len,
        };
        // SAFETY: valid stack_t over memory that outlives the thread's
        // tenure on this span.
        unsafe { libc::sigaltstack(&ss, std::ptr::null_mut()) };
    }

    /// The stack-proper bounds, for the worker's own TLS trim record
    /// (idle workers trim themselves through [`trim_range`]).
    pub fn span(&self) -> (usize, usize) {
        (self.stack_lo() as usize, self.stack_len())
    }
}

/// Free-function trim over `(lo, len)` stack-proper bounds: release
/// committed pages from `lo` up to [`FLOOR`] bytes below `sp`.
pub fn trim_range(lo: usize, len: usize, sp: usize) {
    let hi = lo + len;
    if sp < lo || sp > hi {
        return;
    }
    let keep_to = sp.saturating_sub(FLOOR) & !(page() - 1);
    if keep_to <= lo {
        return;
    }
    let n = keep_to - lo;
    // SAFETY: [lo, keep_to) is inside the mapping, at least FLOOR
    // bytes below the caller's live stack pointer; MADV_FREE only
    // marks pages reclaimable (the region is dead stack space).
    unsafe {
        #[cfg(target_os = "linux")]
        if libc::madvise(lo as *mut _, n, libc::MADV_FREE) != 0 {
            libc::madvise(lo as *mut _, n, libc::MADV_DONTNEED);
        }
        #[cfg(not(target_os = "linux"))]
        libc::madvise(lo as *mut _, n, libc::MADV_FREE);
    }
}

// ---- guard registry + the overflow report --------------------------------
//
// Append-only lock-free list of guard ranges. Stacks are pooled and
// never unmapped, so entries are never removed — the handler can walk
// the list with plain atomic loads (async-signal-safe).

struct GuardNode {
    lo: usize,
    hi: usize,
    next: *const GuardNode,
}

static GUARDS: AtomicPtr<GuardNode> = AtomicPtr::new(std::ptr::null_mut());

fn register_guard(lo: usize, hi: usize) {
    let node = Box::into_raw(Box::new(GuardNode {
        lo,
        hi,
        next: std::ptr::null(),
    }));
    loop {
        let head = GUARDS.load(SeqCst);
        // SAFETY: node is ours until the CAS publishes it.
        unsafe { (*node).next = head };
        if GUARDS.compare_exchange(head, node, SeqCst, SeqCst).is_ok() {
            return;
        }
    }
}

fn guard_hit(addr: usize) -> bool {
    let mut p = GUARDS.load(SeqCst) as *const GuardNode;
    while !p.is_null() {
        // SAFETY: nodes are never freed.
        let n = unsafe { &*p };
        if addr >= n.lo && addr < n.hi {
            return true;
        }
        p = n.next;
    }
    false
}

// The faulting task's label, pre-rendered per worker thread so the
// handler only copies bytes (async-signal-safe).
thread_local! {
    static FAULT_LABEL: UnsafeCell<[u8; 96]> = const { UnsafeCell::new([0; 96]) };
}

/// Set the label the overflow report names (worker sets this around
/// each task run). Truncates to the buffer.
pub fn set_fault_label(label: &str) {
    FAULT_LABEL.with(|b| {
        // SAFETY: thread-local, no aliasing.
        let buf = unsafe { &mut *b.get() };
        let n = label.len().min(buf.len() - 1);
        buf[..n].copy_from_slice(&label.as_bytes()[..n]);
        buf[n] = 0;
    });
}

// s246 (wolf-lang#783, #784, `[os.signal.disp.one]`, `[os.signal.exit]`):
// what the PROGRAM chose for a SEGV or a BUS another process (or this
// one) SENT — a `kill`, `tgkill` or `sigqueue`, never a fault. The
// handler below owns both signals for the stack guard, so a sent one used
// to land here, find no guard, set the default and return: the process
// lived through the first one it was sent (measured at 0.2.32: a program
// that had run a task survived `os_signal_send(os_pid(), SEGMENT_FAULT)`
// and exited 0; one that never spawned, so had no handler, died of it).
// Now a sent one does what the program chose: the default (the host's
// action, death by the signal) or nothing (`os_signal_ignore_one`). A
// real fault is still the runtime's.
static SENT_SEGV_IGNORED: AtomicBool = AtomicBool::new(false);
static SENT_BUS_IGNORED: AtomicBool = AtomicBool::new(false);

fn sent_flag(sig: libc::c_int) -> Option<&'static AtomicBool> {
    match sig {
        libc::SIGSEGV => Some(&SENT_SEGV_IGNORED),
        libc::SIGBUS => Some(&SENT_BUS_IGNORED),
        _ => None,
    }
}

/// Whether `code` (a `siginfo_t`'s `si_code`) says a process SENT the
/// signal. linux: every sender's code is at or below 0 (`SI_USER` 0,
/// `SI_QUEUE`, `SI_TKILL` and the rest negative); a fault's is above 0
/// (`SEGV_MAPERR`, `SEGV_ACCERR`, `BUS_ADRERR`, … and the kernel's own
/// `SI_KERNEL`). macOS (and the BSDs): a sender's codes are `SI_USER`
/// (0x10001, `kill` and `raise`) and its neighbours (`SI_QUEUE`, …), far
/// above a fault's kind (1, 2, 3); the `libc` crate names none of them
/// for these targets, so the range is written here, and what holds it is
/// the macOS runner's witness (`signal_gaps_lanes.rs`: a sent SEGV ends
/// the program, an ignored one does not, a real overflow is reported).
fn sent(code: libc::c_int) -> bool {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        code <= 0
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        code <= 0 || code >= 0x10000
    }
}

/// s246: the program's choice for a SENT `SIGSEGV` or `SIGBUS` —
/// `ignore` true for nothing at all, false for the host's default. The
/// handler is installed first (it is the one place a sent fault is
/// told from a real one), so a later pool start cannot undo the choice.
pub fn set_sent_fault(sig: libc::c_int, ignore: bool) -> bool {
    let Some(flag) = sent_flag(sig) else {
        return false;
    };
    install_overflow_handler();
    flag.store(ignore, SeqCst);
    true
}

/// s246: whether the program ignores a sent `SIGSEGV` or `SIGBUS` now.
pub fn sent_fault_ignored(sig: libc::c_int) -> bool {
    sent_flag(sig).is_some_and(|f| f.load(SeqCst))
}

/// s246 (`[os.signal.disp]`'s rule across `exec`): the signals of the
/// fault meanings this program ignores, which a child or an exec'd image
/// must start with ignored — the handler that tells a sent one from a
/// fault does not survive `exec`, so `SIG_IGN` stands in for it there.
pub fn ignored_sent_faults() -> [libc::c_int; 2] {
    [
        if SENT_SEGV_IGNORED.load(SeqCst) {
            libc::SIGSEGV
        } else {
            0
        },
        if SENT_BUS_IGNORED.load(SeqCst) {
            libc::SIGBUS
        } else {
            0
        },
    ]
}

/// s246: after a FAILED exec put the handler back on the fault signals
/// [`ignored_sent_faults`] set to `SIG_IGN` for it.
pub fn reinstall_after_exec(sigs: [libc::c_int; 2]) {
    for sig in sigs {
        if sig != 0 {
            install_on(sig);
        }
    }
}

extern "C" fn overflow_handler(
    sig: libc::c_int,
    info: *mut libc::siginfo_t,
    _ctx: *mut libc::c_void,
) {
    // s246: a SENT signal is the program's, by the choice it made: none
    // of the guard's business. (`si_code` is valid in every siginfo.)
    // SAFETY: `info` is the kernel's siginfo for this delivery.
    if sent(unsafe { (*info).si_code }) {
        if sent_fault_ignored(sig) {
            return;
        }
        // The default, and the signal again: blocked while this handler
        // runs, it is delivered as the handler returns, and the default
        // ends the process by it (with a core where the host writes one).
        // SAFETY: sigaction and raise are async-signal-safe.
        unsafe {
            let mut dfl: libc::sigaction = std::mem::zeroed();
            dfl.sa_sigaction = libc::SIG_DFL;
            libc::sigaction(sig, &dfl, std::ptr::null_mut());
            libc::raise(sig);
        }
        return;
    }
    // SAFETY: si_addr is valid for SIGSEGV/SIGBUS with SA_SIGINFO.
    #[cfg(target_os = "linux")]
    let addr = unsafe { (*info).si_addr() } as usize;
    #[cfg(not(target_os = "linux"))]
    let addr = unsafe { (*info).si_addr } as usize;
    if guard_hit(addr) {
        // "wolf-rt: stack overflow in task '<label>'\n" — byte-built,
        // write(2) direct, then a deterministic trap-discipline exit.
        let mut msg = [0u8; 160];
        let mut n = 0;
        let mut put = |s: &[u8]| {
            let k = s.len().min(msg.len() - n);
            msg[n..n + k].copy_from_slice(&s[..k]);
            n += k;
        };
        put(b"wolf-rt: stack overflow in task '");
        FAULT_LABEL.with(|b| {
            // SAFETY: thread-local, handler runs on the faulting thread.
            let buf = unsafe { &*b.get() };
            let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
            put(&buf[..len]);
        });
        put(b"'\n");
        // SAFETY: write to stderr with a valid buffer.
        unsafe { libc::write(2, msg.as_ptr().cast(), n) };
        // Same exit discipline as __wolf_rt_trap: reported outcome,
        // never a raw signal death. (The trap-kind vocabulary is
        // closed at s06; adding `stack-overflow` to it is a spec
        // change owed to the c07 closeout.)
        // SAFETY: _exit is async-signal-safe.
        unsafe { libc::_exit(crate::native::TRAP_EXIT_CODE) };
    }
    // Not ours: restore default disposition and return; the re-fault
    // dies the ordinary way.
    // SAFETY: resetting the handler for this signal.
    unsafe {
        let mut dfl: libc::sigaction = std::mem::zeroed();
        dfl.sa_sigaction = libc::SIG_DFL;
        libc::sigaction(sig, &dfl, std::ptr::null_mut());
    }
}

/// The guard-fault reporter on one signal.
fn install_on(sig: libc::c_int) {
    // SAFETY: installing a handler with a static extern fn.
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = overflow_handler
            as extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void)
            as usize;
        sa.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(sig, &sa, std::ptr::null_mut());
    }
}

/// Install the guard-fault reporter (idempotent; called at pool init —
/// a binary that never spawns never installs a handler — and, s246, by
/// a program's first choice for a sent `SEGMENT_FAULT` or `BUS_ERROR`).
/// s246: on `SIGBUS` on every unix host (macOS only before): the sent
/// rule must hold for both fault meanings the runtime owns, and a real
/// bus error outside a guard still dies the ordinary way (the handler
/// sets the default and the fault comes again).
pub fn install_overflow_handler() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        install_on(libc::SIGSEGV);
        install_on(libc::SIGBUS);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_trim_and_registry() {
        let s = TaskStack::map().expect("mmap");
        assert_eq!(s.stack_len(), reserve_size());
        let lo = s.stack_lo() as usize;
        assert!(guard_hit(lo - 1));
        assert!(guard_hit(lo - GUARD));
        assert!(!guard_hit(lo));
        // Touch some pages near the top, then trim from a fake sp.
        let top = lo + s.stack_len();
        // SAFETY: writing inside our own mapping.
        unsafe {
            for off in 1..=8usize {
                *((top - off * 4096) as *mut u8) = 0xAB;
            }
        }
        let (lo_b, len_b) = s.span();
        trim_range(lo_b, len_b, top - 4 * 4096);
        // Altstack region is writable and inside the span.
        let (alt, len) = s.altstack();
        assert_eq!(alt as usize + len, s.base as usize + s.total);
        // SAFETY: writing inside our own mapping.
        unsafe { *alt = 1 };
    }
}
