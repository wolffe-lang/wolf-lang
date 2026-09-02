//! Task stacks and the guard-page reporter — windows (s60b; the
//! stack.rs twin, same surface).
//!
//! **The reservation is the kernel's.** A Win32 thread stack IS
//! `VirtualAlloc(MEM_RESERVE)` plus a `PAGE_GUARD` page that ntdll
//! walks down on first touch: `CreateThread` with
//! `STACK_SIZE_PARAM_IS_A_RESERVATION` reserves the span
//! (`WOLF_TASK_STACK` bytes, the unix default of 8 MiB) and commits
//! page-by-page on fault — exactly the reserve-large/commit-on-fault
//! posture stack.rs builds by hand with `mmap` + `mprotect`. What
//! windows does NOT offer is a thread on memory WE mapped: there is no
//! `pthread_attr_setstack`; the TEB's stack bounds are the kernel's,
//! and moving them is fiber territory (undocumented for threads). So
//! the spans are not pooled across threads here — a worker keeps its
//! stack for its whole life (the pool keeps workers alive, so the
//! recycle the unix store exists for — a thread-exit artifact — does
//! not arise), and a retiring extra worker's stack goes back with its
//! thread. Idle trim (`MEM_DECOMMIT` beneath the parked frame and a
//! re-armed `PAGE_GUARD`, the `_resetstkoflw` shape) is the s60c
//! delta; the s36 chaos hooks' commit-failure injection lands with it.
//!
//! **The reporter is a vectored exception handler** — the one place
//! s60a's finding (sited traps are calls, never SEH) does not reach.
//! A stack overflow is not a call: it is the kernel raising
//! `STATUS_STACK_OVERFLOW` (`0xC00000FD`) on the guard page, and
//! without a handler the process dies with that status and no words
//! (the s60a ledger's named gap, measured). The VEH is the
//! `sigaction(SIGSEGV)` twin: installed at pool init (D15 — a program
//! that never spawns installs nothing), it matches ONLY that status,
//! writes the report stack.rs writes — `wolf-rt: stack overflow in
//! task '<name>'` — and terminates the process with the trap-
//! discipline exit (134, D70's one number) through `TerminateProcess`,
//! the `_exit` twin: no DLL detach, no unwinding through wolf frames
//! (`[abi.native.nounwind]`), no containment. A stack overflow is
//! process death on every host — `stack-overflow` is not in the closed
//! trap vocabulary (the c07 note stack.rs carries), so it is never a
//! proc's `fault(kind)`. Every other exception continues the search
//! untouched: Rust's own `catch_unwind` machinery, the CRT, and
//! debuggers see exactly what they saw before.
//!
//! **Room to run.** The handler executes on the overflowing thread,
//! beneath the guard page that just fired, in the space
//! `SetThreadStackGuarantee` holds back for exactly this — the
//! altstack twin, set at worker entry ([`enter`]) and for the
//! installing thread. It allocates nothing, locks nothing, and calls
//! two kernel32 entry points.
//!
//! One report the unix handler cannot give: a thread that is not a
//! worker (the main thread, once the pool is up) reports too —
//! `wolf-rt: stack overflow` without a task name — because the VEH is
//! process-wide where the guard registry is per-span. Same words, same
//! number; strictly more voice, never less.

#![cfg(windows)]

use std::cell::UnsafeCell;

/// Committed floor the reserve may never drop under (the unix
/// figure, kept for `WOLF_TASK_STACK` parity).
const FLOOR: usize = 64 * 1024;
/// The unix guard's width — part of the minimum so a tiny
/// `WOLF_TASK_STACK` still leaves a usable span.
const GUARD: usize = 64 * 1024;
/// Default reserve for the stack proper.
const DEFAULT_RESERVE: usize = 8 << 20;
/// x86-64 page size (the kernel's; `GetSystemInfo` would say the
/// same and is not worth the call).
const PAGE: usize = 4096;
/// The kernel's stack-overflow status.
const STATUS_STACK_OVERFLOW: u32 = 0xC000_00FD;
/// "Not ours — keep dispatching."
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
/// `GetStdHandle(STD_ERROR_HANDLE)`.
const STD_ERROR_HANDLE: u32 = -12i32 as u32;
/// Stack guarantee beneath the guard page for the report to run in
/// (std's own figure for its overflow handler).
const GUARANTEE: u32 = 0x5000;

/// Reserve size for the stack proper (`WOLF_TASK_STACK`, bytes) —
/// the same law as the unix module: an explicit size is clamped to
/// a usable minimum and rounded to pages; the default is 8 MiB.
pub fn reserve_size() -> usize {
    use std::sync::OnceLock;
    static SIZE: OnceLock<usize> = OnceLock::new();
    *SIZE.get_or_init(|| {
        std::env::var("WOLF_TASK_STACK")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .map(|n| n.max(FLOOR + GUARD).next_multiple_of(PAGE))
            .unwrap_or(DEFAULT_RESERVE)
    })
}

#[repr(C)]
struct ExceptionRecord {
    code: u32,
    flags: u32,
    record: *mut ExceptionRecord,
    address: *mut core::ffi::c_void,
    number_parameters: u32,
    information: [usize; 15],
}

#[repr(C)]
struct ExceptionPointers {
    record: *mut ExceptionRecord,
    context: *mut core::ffi::c_void,
}

// The five kernel32 entry points this module needs, declared here
// directly (D15: no `windows-sys` in the runtime — the libc posture,
// raw bindings for exactly the calls we make). kernel32 is on every
// wolf link line (`rustc --print native-static-libs` names it).
#[link(name = "kernel32")]
unsafe extern "system" {
    fn AddVectoredExceptionHandler(
        first: u32,
        handler: unsafe extern "system" fn(*mut ExceptionPointers) -> i32,
    ) -> *mut core::ffi::c_void;
    fn SetThreadStackGuarantee(size: *mut u32) -> i32;
    fn GetStdHandle(which: u32) -> *mut core::ffi::c_void;
    fn WriteFile(
        handle: *mut core::ffi::c_void,
        buf: *const u8,
        len: u32,
        written: *mut u32,
        overlapped: *mut core::ffi::c_void,
    ) -> i32;
    fn GetCurrentProcess() -> *mut core::ffi::c_void;
    fn TerminateProcess(process: *mut core::ffi::c_void, code: u32) -> i32;
}

// The faulting task's label, pre-rendered per worker thread so the
// handler only copies bytes (the unix module's discipline: nothing
// in the handler allocates or locks).
thread_local! {
    static FAULT_LABEL: UnsafeCell<[u8; 96]> = const { UnsafeCell::new([0; 96]) };
}

/// Set the label the overflow report names (the worker sets this
/// around each task run). Truncates to the buffer.
pub fn set_fault_label(label: &str) {
    FAULT_LABEL.with(|b| {
        // SAFETY: thread-local, no aliasing.
        let buf = unsafe { &mut *b.get() };
        let n = label.len().min(buf.len() - 1);
        buf[..n].copy_from_slice(&label.as_bytes()[..n]);
        buf[n] = 0;
    });
}

/// Worker entry: hold back the stack guarantee so the overflow report
/// has room to run beneath the guard page (the altstack twin).
pub fn enter() {
    let mut size = GUARANTEE;
    // SAFETY: a plain in-out u32 on the calling thread.
    unsafe { SetThreadStackGuarantee(&mut size) };
}

unsafe extern "system" fn overflow_handler(info: *mut ExceptionPointers) -> i32 {
    // SAFETY: the dispatcher hands a valid pointer pair for the
    // duration of the call.
    let code = unsafe { (*(*info).record).code };
    if code != STATUS_STACK_OVERFLOW {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    // "wolf-rt: stack overflow in task '<label>'\n" — byte-built,
    // WriteFile direct, then the deterministic trap-discipline exit.
    let mut msg = [0u8; 160];
    let mut n = 0;
    let mut put = |s: &[u8]| {
        let k = s.len().min(msg.len() - n);
        msg[n..n + k].copy_from_slice(&s[..k]);
        n += k;
    };
    put(b"wolf-rt: stack overflow");
    FAULT_LABEL.with(|b| {
        // SAFETY: thread-local, the handler runs on the faulting thread.
        let buf = unsafe { &*b.get() };
        let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
        if len > 0 {
            put(b" in task '");
            put(&buf[..len]);
            put(b"'");
        }
    });
    put(b"\n");
    // SAFETY: a valid stderr handle and a live buffer; TerminateProcess
    // on our own process never returns.
    unsafe {
        let mut written = 0u32;
        WriteFile(
            GetStdHandle(STD_ERROR_HANDLE),
            msg.as_ptr(),
            n as u32,
            &mut written,
            std::ptr::null_mut(),
        );
        TerminateProcess(GetCurrentProcess(), crate::native::TRAP_EXIT_CODE as u32);
    }
    EXCEPTION_CONTINUE_SEARCH
}

/// Install the guard-fault reporter (idempotent; called at pool init —
/// a binary that never spawns never installs a handler). First in the
/// vectored chain, so nothing else (std's own handler in a `cargo
/// test` host, a debugger's) speaks before wolf does. The installing
/// thread gets the stack guarantee too: the main thread reports.
pub fn install_overflow_handler() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: registering a static extern fn; the handle is
        // process-lifetime (never removed) and deliberately dropped.
        unsafe {
            AddVectoredExceptionHandler(1, overflow_handler);
        }
        enter();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reserve law holds on this host: the default is 8 MiB and
    /// the label round-trips through the per-thread buffer.
    #[test]
    fn reserve_and_label() {
        assert!(reserve_size() >= FLOOR + GUARD);
        assert_eq!(reserve_size() % PAGE, 0);
        set_fault_label("deep-recursor");
        FAULT_LABEL.with(|b| {
            // SAFETY: thread-local, no aliasing.
            let buf = unsafe { &*b.get() };
            assert_eq!(&buf[..13], b"deep-recursor");
            assert_eq!(buf[13], 0);
        });
        set_fault_label("");
        FAULT_LABEL.with(|b| {
            // SAFETY: as above.
            let buf = unsafe { &*b.get() };
            assert_eq!(buf[0], 0);
        });
    }
}
