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

#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::cell::RefCell;
use std::cell::{Cell, UnsafeCell};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

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
        register_guard(self.base as usize, self.base as usize + GUARD);
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
// Guard bounds belong to the current thread, not a process-wide registry:
// a wild pointer into another worker's guard is not this thread overflowing.
thread_local! {
    static CURRENT_GUARD: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    static OWN_ALT: RefCell<Option<AltStack>> = const { RefCell::new(None) };
}

fn register_guard(lo: usize, hi: usize) {
    CURRENT_GUARD.with(|g| g.set((lo, hi)));
}

fn guard_hit(addr: usize) -> bool {
    CURRENT_GUARD.with(|g| {
        let (lo, hi) = g.get();
        lo <= addr && addr < hi
    })
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct AltStack(*mut libc::c_void);
#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Drop for AltStack {
    fn drop(&mut self) {
        // SAFETY: our thread is leaving, outside any handler. Disable
        // before unmapping; no other thread can use this signal stack.
        unsafe {
            let disabled = libc::stack_t {
                ss_sp: std::ptr::null_mut(),
                ss_flags: libc::SS_DISABLE,
                ss_size: 0,
            };
            libc::sigaltstack(&disabled, std::ptr::null_mut());
            libc::munmap(self.0, ALT);
        }
    }
}

/// Register a system-provided stack (fallback, signal or reactor thread;
/// the main thread is [`enter_main_thread`]). The pooled pthread path
/// registers its own explicit guard. Reuse an existing altstack, notably
/// a Rust thread's, without owning it.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn enter_thread() {
    if CURRENT_GUARD.with(|g| g.get().1 != 0) {
        return;
    }
    let Some(bounds) = pthread_bounds() else {
        return;
    };
    // A thread that ends unmaps the altstack it was given (`AltStack`).
    let Some(ptr) = ensure_altstack() else {
        return;
    };
    if !ptr.is_null() {
        OWN_ALT.with(|a| *a.borrow_mut() = Some(AltStack(ptr)));
    }
    register_guard(bounds.0, bounds.1);
    install_overflow_handler();
}

/// s265 (wolf-lang#918): the main thread's registration, from the entry
/// shim (`__wolf_rt_stack_start`) before user `main`. The same guard,
/// altstack and handler as [`enter_thread`], paid for more cheaply:
/// - linux: the bounds come from [`main_stack_bounds`] (one read of
///   `/proc/self/maps` with no stdio, no scanf, no CPU-set query), not
///   `pthread_getattr_np(3)`;
/// - the altstack lives as long as the process: the main thread's end
///   is the process's, so no thread-exit destructor is registered and
///   nothing is unmapped on the way out.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn enter_main_thread() {
    if CURRENT_GUARD.with(|g| g.get().1 != 0) {
        return;
    }
    #[cfg(target_os = "linux")]
    let bounds = main_stack_bounds().or_else(pthread_bounds);
    #[cfg(not(target_os = "linux"))]
    let bounds = pthread_bounds();
    let Some(bounds) = bounds else {
        return;
    };
    if ensure_altstack().is_none() {
        return;
    }
    register_guard(bounds.0, bounds.1);
    install_overflow_handler();
}

/// The calling thread's guard, from the host's pthread queries.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn pthread_bounds() -> Option<(usize, usize)> {
    // SAFETY: pthread queries describe only this live thread. The Linux
    // main-thread attributes include its RLIMIT_STACK range; macOS gives
    // the high address, rather than the low one. Neither query faults.
    unsafe {
        #[cfg(target_os = "linux")]
        {
            let mut attr: libc::pthread_attr_t = std::mem::zeroed();
            let mut low = std::ptr::null_mut();
            let mut len = 0;
            let mut guard = 0;
            if libc::pthread_getattr_np(libc::pthread_self(), &mut attr) != 0 {
                return None;
            }
            let rc = libc::pthread_attr_getstack(&attr, &mut low, &mut len);
            let grc = libc::pthread_attr_getguardsize(&attr, &mut guard);
            libc::pthread_attr_destroy(&mut attr);
            if rc != 0 || grc != 0 || len == 0 {
                return None;
            }
            let hi = low as usize;
            Some((hi.saturating_sub(guard.max(page())), hi))
        }
        #[cfg(target_os = "macos")]
        {
            let thread = libc::pthread_self();
            let top = libc::pthread_get_stackaddr_np(thread) as usize;
            let len = libc::pthread_get_stacksize_np(thread);
            let hi = top.saturating_sub(len);
            Some((hi.saturating_sub(page()), hi))
        }
    }
}

/// An altstack for the calling thread: `Some(null)` when it already has
/// one (a Rust thread's, kept and not owned), `Some(ptr)` for a fresh
/// [`ALT`]-byte mapping now installed, `None` when none could be made.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn ensure_altstack() -> Option<*mut libc::c_void> {
    // SAFETY: queried signal-stack descriptor is local; a fresh mapping
    // outlives all handler invocations (its owner decides when it ends).
    unsafe {
        let mut old: libc::stack_t = std::mem::zeroed();
        if libc::sigaltstack(std::ptr::null(), &mut old) != 0 {
            return None;
        }
        if old.ss_flags & libc::SS_DISABLE == 0 {
            return Some(std::ptr::null_mut());
        }
        let ptr = libc::mmap(
            std::ptr::null_mut(),
            ALT,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        if ptr == libc::MAP_FAILED {
            return None;
        }
        let ss = libc::stack_t {
            ss_sp: ptr,
            ss_flags: 0,
            ss_size: ALT,
        };
        if libc::sigaltstack(&ss, std::ptr::null_mut()) != 0 {
            libc::munmap(ptr, ALT);
            return None;
        }
        Some(ptr)
    }
}

/// s265 (wolf-lang#918): the main thread's guard on linux without
/// `pthread_getattr_np(3)`. proc(5) documents `/proc/self/maps` as one
/// line per mapping in address order, each starting `start-end ` in hex;
/// getrlimit(2) documents `RLIMIT_STACK` as the most the main stack may
/// grow. The stack is the mapping that holds this frame; it may grow down
/// to `RLIMIT_STACK` below its top, never into the mapping beneath it;
/// the guard is the page under that floor. These are the bounds
/// glibc's `pthread_getattr_np` reports for the main thread, measured
/// black-box on kasumi (s265 note, `stackprobe`) and held by the unit
/// test `main_stack_bounds_match_pthread`; the cost it removes is that
/// call's stdio and scanf over the same file, its `fstat` and its
/// CPU-set query (`sched_getaffinity`). `None` when the file cannot be
/// read or no mapping holds the frame: the caller falls back.
#[cfg(target_os = "linux")]
fn main_stack_bounds() -> Option<(usize, usize)> {
    let marker = 0u8;
    let here = std::hint::black_box(&raw const marker) as usize;
    let (lo, hi, below) = mapping_holding(here)?;
    // SAFETY: a plain query of this process's own limit.
    let rlim = unsafe {
        let mut r: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_STACK, &mut r) != 0 {
            return None;
        }
        r.rlim_cur
    };
    let pg = page();
    let floor = if rlim == libc::RLIM_INFINITY {
        below
    } else {
        // Page-aligned up: the stack never grows past the limit.
        hi.saturating_sub(rlim as usize).next_multiple_of(pg).max(below)
    };
    let _ = lo;
    Some((floor.saturating_sub(pg), floor))
}

/// s265: the main thread's guard both ways, `(cheap, pthread)`, for the
/// equivalence test (`tests/main_stack_bounds.rs`, run on the main thread).
#[cfg(target_os = "linux")]
pub fn main_guard_both() -> (Option<(usize, usize)>, Option<(usize, usize)>) {
    (main_stack_bounds(), pthread_bounds())
}

/// `(start, end, previous end)` of the `/proc/self/maps` line whose
/// range holds `addr`, read with plain `open`/`read` into a stack buffer
/// and parsed as the lines arrive (a line may span two reads).
#[cfg(target_os = "linux")]
fn mapping_holding(addr: usize) -> Option<(usize, usize, usize)> {
    // SAFETY: a fixed path, our own buffer, the descriptor closed below.
    let fd = unsafe { libc::open(c"/proc/self/maps".as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
    if fd < 0 {
        return None;
    }
    let mut buf = [0u8; 4096];
    // The line parser: field 0 the start, 1 the end, 2 the rest.
    let (mut field, mut start, mut end, mut prev_end) = (0u8, 0usize, 0usize, 0usize);
    let mut found = None;
    'read: loop {
        // SAFETY: `buf` is ours and `fd` is open.
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            break;
        }
        for &b in &buf[..n as usize] {
            match (field, b) {
                (_, b'\n') => {
                    if field == 2 {
                        if start <= addr && addr < end {
                            found = Some((start, end, prev_end));
                            break 'read;
                        }
                        prev_end = end;
                    }
                    (field, start, end) = (0, 0, 0);
                }
                (0, b'-') => field = 1,
                (1, b' ') => field = 2,
                (0 | 1, _) => {
                    let Some(d) = (b as char).to_digit(16) else {
                        field = 3; // not a mapping line: skip it
                        continue;
                    };
                    let v = if field == 0 { &mut start } else { &mut end };
                    *v = v.wrapping_mul(16).wrapping_add(d as usize);
                }
                _ => {}
            }
        }
    }
    // SAFETY: closing the descriptor opened above.
    unsafe { libc::close(fd) };
    found
}

// Read the instruction address from the kernel's saved context. No
// unwinder, loader lock, allocation, symbol lookup or file I/O in a handler.
unsafe fn fault_pc(ctx: *mut libc::c_void) -> usize {
    // SAFETY: SA_SIGINFO supplied a ucontext for this delivery.
    unsafe {
        let uc = &*ctx.cast::<libc::ucontext_t>();
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            uc.uc_mcontext.gregs[libc::REG_RIP as usize] as usize
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        {
            uc.uc_mcontext.pc as usize
        }
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            (*uc.uc_mcontext).__ss.__pc as usize
        }
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        {
            (*uc.uc_mcontext).__ss.__rip as usize
        }
        #[cfg(not(any(
            all(
                target_os = "linux",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ),
            all(
                target_os = "macos",
                any(target_arch = "x86_64", target_arch = "aarch64")
            )
        )))]
        {
            let _ = uc;
            0
        }
    }
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

/// Whether a delivery's `siginfo_t` says a process SENT the signal.
/// linux: by `si_code` alone — every sender's is at or below 0 (`SI_USER`
/// 0, `SI_QUEUE`, `SI_TKILL` and the rest negative), a fault's above 0
/// (`SEGV_MAPERR`, `SEGV_ACCERR`, `BUS_ADRERR`, … and the kernel's own
/// `SI_KERNEL`). **macOS cannot say** (measured on CI's runner, s246 run
/// 39218842446: a SEGV sent by `kill` and by `raise` arrives with
/// `si_code` 2, `si_pid` 0 and `si_addr` 0 — the code a write to address
/// 16 gets, with `si_addr` 0x10), so there a delivery at address 0 is
/// taken as sent only when this process announced a send of that signal
/// to itself ([`note_self_send`]: `os_signal_send` to its own id, a group
/// send to its own group) and it has not been taken yet.
fn sent(sig: libc::c_int, code: libc::c_int, addr: usize) -> bool {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let _ = (sig, addr);
        code <= 0
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        let _ = code;
        addr == 0 && take_self_send(sig)
    }
}

/// s246 (macOS): sends of `SIGSEGV`/`SIGBUS` this process made to itself
/// and the handler has not met yet ([`sent`]).
#[cfg(not(any(target_os = "linux", target_os = "android")))]
static SELF_SENDS: [std::sync::atomic::AtomicUsize; 2] = [
    std::sync::atomic::AtomicUsize::new(0),
    std::sync::atomic::AtomicUsize::new(0),
];

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn self_slot(sig: libc::c_int) -> Option<&'static std::sync::atomic::AtomicUsize> {
    match sig {
        libc::SIGSEGV => Some(&SELF_SENDS[0]),
        libc::SIGBUS => Some(&SELF_SENDS[1]),
        _ => None,
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn take_self_send(sig: libc::c_int) -> bool {
    self_slot(sig).is_some_and(|n| n.fetch_update(SeqCst, SeqCst, |k| k.checked_sub(1)).is_ok())
}

/// s246: a send of `sig` that reaches this process is about to be made
/// (`ok` false: it was refused, take the note back). Only macOS reads it
/// ([`sent`]); linux's siginfo says who sent a signal by itself.
pub fn note_self_send(sig: libc::c_int, ok: bool) {
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    if let Some(n) = self_slot(sig) {
        if ok {
            n.fetch_add(1, SeqCst);
        } else {
            let _ = n.fetch_update(SeqCst, SeqCst, |k| k.checked_sub(1));
        }
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let _ = (sig, ok);
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
            let _ = install_on(sig);
        }
    }
}

extern "C" fn overflow_handler(
    sig: libc::c_int,
    info: *mut libc::siginfo_t,
    ctx: *mut libc::c_void,
) {
    // SAFETY: si_addr is valid for SIGSEGV/SIGBUS with SA_SIGINFO.
    #[cfg(target_os = "linux")]
    let addr = unsafe { (*info).si_addr() } as usize;
    #[cfg(not(target_os = "linux"))]
    let addr = unsafe { (*info).si_addr } as usize;
    // s246: a SENT signal is the program's, by the choice it made: none
    // of the guard's business. (`si_code` is valid in every siginfo.)
    // SAFETY: `info` is the kernel's siginfo for this delivery.
    if sent(sig, unsafe { (*info).si_code }, addr) {
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
    if guard_hit(addr) {
        // "wolf-rt: stack overflow in task '<label>'\n" — byte-built,
        // write(2) direct, then a deterministic trap-discipline exit.
        let mut msg = [0u8; 256];
        let mut n = 0;
        let mut put = |s: &[u8]| {
            let k = s.len().min(msg.len() - n);
            msg[n..n + k].copy_from_slice(&s[..k]);
            n += k;
        };
        put(b"wolf-trap: stack-overflow\n  at pc=0x");
        // SAFETY: ctx is the kernel context passed to this SA_SIGINFO handler.
        let pc = unsafe { fault_pc(ctx) };
        let mut hex = [b'0'; 2 * std::mem::size_of::<usize>()];
        for (i, byte) in hex.iter_mut().rev().enumerate() {
            *byte = b"0123456789abcdef"[(pc >> (i * 4)) & 15];
        }
        put(&hex);
        put(b"\n");
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
        // s248 (`[os.term.restore]`): the registered terminal restore,
        // after the report — async-signal-safe, one try at its claim.
        crate::term::restore_in_handler();
        // [conf.trap.stack]: process death, never proc containment or unwind.
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
    // s246 (macOS, where [`sent`] cannot tell): at address 0 the delivery
    // is a fault there or a signal another process sent, and under the
    // default both end the process by it — the signal again makes the
    // second do so too, as the re-fault does the first. Under an ignore
    // a sent one from another process lives through this once (the
    // handler is gone), and a fault re-faults and ends it.
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    if addr == 0 && !sent_fault_ignored(sig) {
        // SAFETY: raise is async-signal-safe; the default is in place.
        unsafe { libc::raise(sig) };
    }
}

/// The guard-fault reporter on one signal; the disposition it replaced
/// (`None` when the host refused the change).
fn install_on(sig: libc::c_int) -> Option<libc::sighandler_t> {
    // SAFETY: installing a handler with a static extern fn; `old` is ours.
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = overflow_handler
            as extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void)
            as usize;
        sa.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        libc::sigemptyset(&mut sa.sa_mask);
        let mut old: libc::sigaction = std::mem::zeroed();
        (libc::sigaction(sig, &sa, &mut old) == 0).then_some(old.sa_sigaction)
    }
}

/// Install the guard-fault reporter (idempotent). System-thread entry,
/// pool init and the first explicit sent-fault disposition all use this
/// same process-wide installation; guard ranges remain thread-local.
/// s246: on `SIGBUS` on every unix host (macOS only before): the sent
/// rule must hold for both fault meanings the runtime owns, and a real
/// bus error outside a guard still dies the ordinary way (the handler
/// sets the default and the fault comes again).
pub fn install_overflow_handler() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // Preserve an ignored SENT signal across exec/startup. A real
        // guard fault is still ours, even when the sent meaning is ignored.
        // s265 (wolf-lang#918): the disposition replaced is read by the
        // installing call itself, one `sigaction` a signal, not two.
        for sig in [libc::SIGSEGV, libc::SIGBUS] {
            if install_on(sig) == Some(libc::SIG_IGN) {
                sent_flag(sig).unwrap().store(true, SeqCst);
            }
        }
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
        register_guard(lo - GUARD, lo);
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
