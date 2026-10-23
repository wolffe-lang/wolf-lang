//! A killed task leaves at its park (s260, wolf-lang#683).
//!
//! `[conc.proc.kill]` step 1: the proc's task tree is cancelled *without
//! running any further user code*. A Rust task body meets that by
//! unwinding ([`super::pool::KilledToken`]); a COMPILED body cannot be
//! unwound — wolf code has no unwind tables (`[abi.native.nounwind]`) —
//! and until s260 the park answered the program instead: the reactor
//! completed the claimed wait `Cancelled`, `net_read` answered `io`, and
//! the killed proc ran its `else` arm and the statements after it before
//! it was gone (t06 replayed it at 0.2.34 and at trunk `06cff722`; the
//! channel and `when` waits answered status 2 and the lowered teardown
//! branch returned one frame, so a caller one frame up ran on).
//!
//! # The leave
//!
//! Every compiled task body is entered through [`run_compiled`], which
//! records the machine state a return from the entry would restore — the
//! callee-saved registers, the stack pointer and the return address
//! ([`Ctx`], saved by `enter`, a few instructions per target) — and
//! keeps a [`Frame`] for the task in a thread-local. A runtime call that
//! parks ends with [`leave_if_killed`]: when the calling task's scope is
//! killed, `leave` restores that state and "returns" from the entry with
//! [`super::conc_abi::CANCEL_TAG`], discarding every frame between: the
//! compiled frames of the task and the runtime call's own (a plain old
//! frame by then — its locals are dropped before the call). No compiled
//! instruction after the park runs: no `else`, no `defer`, no caller.
//! The worker thread is not lost: it returns into the pool's task loop,
//! where the task is retired as any other (the contained-trap path, by
//! contrast, parks its thread for good, which a server that kills a proc
//! per timed-out request could not afford).
//!
//! # What the discarded frames held, and who answers for it
//!
//! - **The proc's regions** are its ledger's: the reaper bulk-frees them
//!   (step 3), after the tree has quiesced. A frame's ambient region is
//!   the thread's, and `with_root_ambient` above the entry puts it back.
//! - **Scopes the task opened and had not joined** (a `scope { }` block
//!   the park sat in) are joined here, innermost first, before the task
//!   is retired: their children are in the killed tree and leave at
//!   their own parks, and a task never outlives its scope
//!   (`[conc.task.join]`) — so the reaper's join still sees the whole
//!   tree come to rest before the regions go.
//! - **A `when` set held** stays held: shared state across procs belongs
//!   to channels and supervisors (step 1's stated consequence), and the
//!   release is the body's own exit. The thread's holder token is renewed
//!   and its `when` depth put back, so the next task on this worker is
//!   neither mistaken for the holder nor flagged as nested.
//! - **Channel references, open files and sockets** are not released:
//!   their release is code the program would have run. A channel the
//!   frames referenced lives on (a leaked reference, never a dangling
//!   one).
//!
//! # Where a task leaves
//!
//! At the exit of every runtime call that parks a compiled task — the
//! reactor's waits (`net_read`, `net_read_bytes`, `net_write`,
//! `net_write_bytes`, `net_writev`, `net_writev_head`, `net_accept`,
//! `net_wait`), the channel calls (`send`, `recv`, `select`), the `when`
//! acquisition, the signal hub's waits, `time_sleep_ms`, a scope's join,
//! a proc's join and kill, the child waits — and at the poll compiled
//! code makes after a status-2 wait (`__wolf_rt_task_killed`). A park the
//! kill cannot wake (the host's `waitpid`, the dial, a pipe read in the
//! kernel) leaves when the host returns. A task that is computing when
//! the kill lands runs on until its next such call: the clause's
//! compute-bound half is the checkpoint story, recorded in `conc_abi.rs`.
//!
//! # Hosts
//!
//! x86-64 (System V: linux, macOS, freebsd; and windows' x64 convention)
//! and aarch64 (AAPCS64: linux, macOS). On x86-64 a CET shadow stack, if
//! the host enables one for the process, is unwound by the same count as
//! the frames (`rdsspq` reads zero, and nothing is done, where none is
//! enabled). Any other architecture keeps the pre-s260 answer: the park
//! returns to the program (no target the CI builds is one).

use core::ffi::c_void;
use std::cell::Cell;
use std::sync::Arc;

use super::scope::ScopeInner;

/// A compiled task entry (`[abi.native.taskenv]`'s entry function).
pub(crate) type Entry = unsafe extern "C" fn(*mut c_void) -> i64;

/// The machine state `enter` saves and `leave` restores. Layout per
/// target, in bytes: x86-64 System V — rbx, rbp, r12..r15 (0..48), the
/// stack pointer at the entry (48, pointing at the return address),
/// mxcsr (56), the x87 control word (60), the shadow-stack pointer (64);
/// windows x64 — rbx, rbp, rdi, rsi, r12..r15 (0..64), the stack pointer
/// (64), mxcsr (72), the control word (76), the shadow-stack pointer
/// (80), xmm6..xmm15 (96..256); aarch64 — x19..x28 (0..80), x29 and x30
/// (80, 88), sp (96), d8..d15 (104..168).
#[repr(C, align(16))]
pub(crate) struct Ctx([u64; 32]);

impl Ctx {
    const ZERO: Ctx = Ctx([0; 32]);
}

/// The leaving machinery for one compiled task on this thread.
struct Frame {
    ctx: Ctx,
    /// The task's spawn scope: killed, directly or through its proc.
    scope: Option<Arc<ScopeInner>>,
    /// Set by [`leave_if_killed`] just before the frames are discarded.
    left: bool,
    /// Scopes the task's frames opened and had not joined (raw
    /// `Arc<ScopeInner>` handles, `__wolf_rt_scope_new`'s answer).
    open: Vec<*const ScopeInner>,
    /// The `when` depth at the entry.
    when_depth: u32,
    prev: *mut Frame,
}

thread_local! {
    static CURRENT: Cell<*mut Frame> = const { Cell::new(std::ptr::null_mut()) };
}

/// Run the compiled `entry(env)` so that a park can leave it: its answer,
/// or [`super::conc_abi::CANCEL_TAG`] when the task left at a park.
///
/// # Safety
///
/// `entry` must be callable with `env` (codegen's contract for a task or
/// proc entry), and the call must be made from a task's own runtime frame
/// with nothing below it but the entry.
pub(crate) unsafe fn run_compiled(entry: Entry, env: *mut c_void) -> i64 {
    let mut f = Frame {
        ctx: Ctx::ZERO,
        scope: super::pool::current_scope(),
        left: false,
        open: Vec::new(),
        when_depth: super::when::depth(),
        prev: CURRENT.with(Cell::get),
    };
    let fp: *mut Frame = &mut f;
    CURRENT.with(|c| c.set(fp));
    // SAFETY: the caller's contract for `entry`; `ctx` lives in this frame,
    // which outlives the call (a leave lands back here, in this frame).
    let tag = unsafe { arch::enter(entry, env, &raw mut (*fp).ctx) };
    // SAFETY: `fp` is `f`, read through the pointer the thread-local holds:
    // a leave wrote `left` and `open` through it.
    let (prev, left) = unsafe { ((*fp).prev, (*fp).left) };
    CURRENT.with(|c| c.set(prev));
    if left {
        // SAFETY: as above; the frames that used `f` are gone.
        unsafe { tidy(&mut *fp) };
    }
    tag
}

/// After a leave: the runtime's own clearing-up of what the discarded
/// frames held (the module doc's list), in the task's runtime frame.
unsafe fn tidy(f: &mut Frame) {
    super::when::after_leave(f.when_depth);
    while let Some(p) = f.open.pop() {
        // SAFETY: an unjoined handle from `__wolf_rt_scope_new`, whose
        // owner (a discarded frame) can no longer join or free it.
        let inner = unsafe { Arc::from_raw(p) };
        let _ = inner.join();
    }
}

/// Leave the calling compiled task if its scope is killed (never returns
/// then); otherwise nothing. Cheap when there is no compiled task on this
/// thread (one thread-local read) and when there is one that is alive (a
/// flag read on its scope chain).
///
/// # Safety
///
/// Called only from a runtime call compiled code made directly (an
/// `extern "C"` shim), as its last step: nothing in the calling frame
/// may still need dropping, because the frame is discarded.
pub(crate) unsafe fn leave_if_killed() {
    let fp = CURRENT.with(Cell::get);
    if fp.is_null() {
        return;
    }
    // SAFETY: a live frame of this thread's running compiled task.
    let killed = unsafe { (*fp).scope.as_ref().is_some_and(|s| s.is_killed()) };
    if !killed || !arch::LEAVES {
        return;
    }
    // SAFETY: as above.
    unsafe {
        (*fp).left = true;
        arch::leave(&raw const (*fp).ctx, super::conc_abi::CANCEL_TAG)
    }
}

/// Whether the calling thread runs a compiled task whose scope is killed
/// (the lowered poll's question, `__wolf_rt_task_killed`).
pub(crate) fn in_killed_compiled_task() -> bool {
    let fp = CURRENT.with(Cell::get);
    // SAFETY: a live frame of this thread's running compiled task.
    !fp.is_null() && unsafe { (*fp).scope.as_ref().is_some_and(|s| s.is_killed()) }
}

/// `__wolf_rt_scope_new` opened `scope` on this thread: if a compiled
/// task runs here, it is its frames' to join, or a leave's.
pub(crate) fn scope_opened(scope: *const ScopeInner) {
    let fp = CURRENT.with(Cell::get);
    if !fp.is_null() {
        // SAFETY: a live frame of this thread's running compiled task.
        unsafe { (*fp).open.push(scope) };
    }
}

/// `__wolf_rt_scope_join_free` joined `scope`: no longer a leave's.
pub(crate) fn scope_closed(scope: *const ScopeInner) {
    let fp = CURRENT.with(Cell::get);
    if fp.is_null() {
        return;
    }
    // SAFETY: a live frame of this thread's running compiled task.
    let open = unsafe { &mut (*fp).open };
    if let Some(i) = open.iter().rposition(|&p| p == scope) {
        open.remove(i);
    }
}

/// A task's `time_sleep_ms` (s260, #683): a sleep the kill ends. Until
/// s260 a sleeping task slept in the host (`std::thread::sleep`) for the
/// whole duration — a proc killed while asleep ran on afterwards, and its
/// kill waited the sleep out. Now the sleep waits on a condvar the parked
/// registry nudges at every cancellation (`parked::wake_all`), so the kill
/// of the task's proc ends it at once and the shim leaves. A polite
/// cancellation does not end it: a sleep is not a cancellation point
/// (`[conc.cancel.points]`), and that is unchanged. Off a task (`main`'s
/// own sleep) and in the det domain, the host's sleep, as before.
pub(crate) fn sleep_unless_killed(d: std::time::Duration) {
    use std::sync::{Condvar, Mutex, PoisonError};
    let Some(scope) = super::pool::current_scope() else {
        std::thread::sleep(d);
        return;
    };
    if super::hooks::det_in_domain() {
        std::thread::sleep(d);
        return;
    }
    struct Sleeper {
        m: Mutex<()>,
        cv: Condvar,
    }
    impl super::parked::Wake for Sleeper {
        fn wake(&self) {
            let _g = self.m.lock().unwrap_or_else(PoisonError::into_inner);
            self.cv.notify_all();
        }
    }
    let sl = Sleeper {
        m: Mutex::new(()),
        cv: Condvar::new(),
    };
    let end = std::time::Instant::now() + d;
    // The registry's protocol: register first, then take the own lock and
    // look; the guard drops before the registration does.
    let _parked = super::parked::park_on(&sl);
    let mut g = sl.m.lock().unwrap_or_else(PoisonError::into_inner);
    loop {
        if scope.is_killed() {
            return;
        }
        let now = std::time::Instant::now();
        if now >= end {
            return;
        }
        g = sl
            .cv
            .wait_timeout(g, end - now)
            .unwrap_or_else(PoisonError::into_inner)
            .0;
    }
}

#[cfg(all(target_arch = "x86_64", not(windows)))]
mod arch {
    use super::{Ctx, Entry};
    use core::ffi::c_void;

    pub(super) const LEAVES: bool = true;

    /// Save the state a return to the caller restores, then call
    /// `entry(env)` and return its answer.
    #[unsafe(naked)]
    pub(super) unsafe extern "C" fn enter(entry: Entry, env: *mut c_void, ctx: *mut Ctx) -> i64 {
        // rdi = entry, rsi = env, rdx = ctx; on entry rsp points at the
        // return address.
        core::arch::naked_asm!(
            "mov [rdx], rbx",
            "mov [rdx + 8], rbp",
            "mov [rdx + 16], r12",
            "mov [rdx + 24], r13",
            "mov [rdx + 32], r14",
            "mov [rdx + 40], r15",
            "mov [rdx + 48], rsp",
            "stmxcsr dword ptr [rdx + 56]",
            "fnstcw word ptr [rdx + 60]",
            "xor eax, eax",
            "rdsspq rax",
            "mov [rdx + 64], rax",
            // Re-align for the call (rsp is 8 mod 16 here).
            "push rbp",
            "mov rax, rdi",
            "mov rdi, rsi",
            "call rax",
            "pop rbp",
            "ret",
        )
    }

    /// Restore `ctx` and return `code` from the `enter` that saved it.
    #[unsafe(naked)]
    pub(super) unsafe extern "C" fn leave(ctx: *const Ctx, code: i64) -> ! {
        // rdi = ctx, rsi = code.
        core::arch::naked_asm!(
            // A shadow stack, if the host enabled one: unwind it to the
            // entry's depth, 255 entries at most per `incsspq`.
            "xor eax, eax",
            "rdsspq rax",
            "test rax, rax",
            "jz 3f",
            "mov rcx, [rdi + 64]",
            "sub rcx, rax",
            "shr rcx, 3",
            "2:",
            "test rcx, rcx",
            "jz 3f",
            "mov rdx, 255",
            "cmp rcx, rdx",
            "cmovb rdx, rcx",
            "incsspq rdx",
            "sub rcx, rdx",
            "jmp 2b",
            "3:",
            "mov rbx, [rdi]",
            "mov rbp, [rdi + 8]",
            "mov r12, [rdi + 16]",
            "mov r13, [rdi + 24]",
            "mov r14, [rdi + 32]",
            "mov r15, [rdi + 40]",
            "ldmxcsr dword ptr [rdi + 56]",
            "fldcw word ptr [rdi + 60]",
            "mov rsp, [rdi + 48]",
            "mov rax, rsi",
            "ret",
        )
    }
}

#[cfg(all(target_arch = "x86_64", windows))]
mod arch {
    use super::{Ctx, Entry};
    use core::ffi::c_void;

    pub(super) const LEAVES: bool = true;

    /// Save the state a return to the caller restores (windows x64:
    /// rdi, rsi and xmm6..xmm15 are callee-saved too), then call
    /// `entry(env)` with its shadow space and return its answer.
    #[unsafe(naked)]
    pub(super) unsafe extern "C" fn enter(entry: Entry, env: *mut c_void, ctx: *mut Ctx) -> i64 {
        // rcx = entry, rdx = env, r8 = ctx.
        core::arch::naked_asm!(
            "mov [r8], rbx",
            "mov [r8 + 8], rbp",
            "mov [r8 + 16], rdi",
            "mov [r8 + 24], rsi",
            "mov [r8 + 32], r12",
            "mov [r8 + 40], r13",
            "mov [r8 + 48], r14",
            "mov [r8 + 56], r15",
            "mov [r8 + 64], rsp",
            "stmxcsr dword ptr [r8 + 72]",
            "fnstcw word ptr [r8 + 76]",
            "xor eax, eax",
            "rdsspq rax",
            "mov [r8 + 80], rax",
            "movdqu [r8 + 96], xmm6",
            "movdqu [r8 + 112], xmm7",
            "movdqu [r8 + 128], xmm8",
            "movdqu [r8 + 144], xmm9",
            "movdqu [r8 + 160], xmm10",
            "movdqu [r8 + 176], xmm11",
            "movdqu [r8 + 192], xmm12",
            "movdqu [r8 + 208], xmm13",
            "movdqu [r8 + 224], xmm14",
            "movdqu [r8 + 240], xmm15",
            // 32 bytes of shadow space, and the alignment (rsp is 8 mod
            // 16 here).
            "sub rsp, 40",
            "mov rax, rcx",
            "mov rcx, rdx",
            "call rax",
            "add rsp, 40",
            "ret",
        )
    }

    /// Restore `ctx` and return `code` from the `enter` that saved it.
    #[unsafe(naked)]
    pub(super) unsafe extern "C" fn leave(ctx: *const Ctx, code: i64) -> ! {
        // rcx = ctx, rdx = code.
        core::arch::naked_asm!(
            "mov r9, rdx",
            "xor eax, eax",
            "rdsspq rax",
            "test rax, rax",
            "jz 3f",
            "mov r10, [rcx + 80]",
            "sub r10, rax",
            "shr r10, 3",
            "2:",
            "test r10, r10",
            "jz 3f",
            "mov r11, 255",
            "cmp r10, r11",
            "cmovb r11, r10",
            "incsspq r11",
            "sub r10, r11",
            "jmp 2b",
            "3:",
            "mov rbx, [rcx]",
            "mov rbp, [rcx + 8]",
            "mov rdi, [rcx + 16]",
            "mov rsi, [rcx + 24]",
            "mov r12, [rcx + 32]",
            "mov r13, [rcx + 40]",
            "mov r14, [rcx + 48]",
            "mov r15, [rcx + 56]",
            "ldmxcsr dword ptr [rcx + 72]",
            "fldcw word ptr [rcx + 76]",
            "movdqu xmm6, [rcx + 96]",
            "movdqu xmm7, [rcx + 112]",
            "movdqu xmm8, [rcx + 128]",
            "movdqu xmm9, [rcx + 144]",
            "movdqu xmm10, [rcx + 160]",
            "movdqu xmm11, [rcx + 176]",
            "movdqu xmm12, [rcx + 192]",
            "movdqu xmm13, [rcx + 208]",
            "movdqu xmm14, [rcx + 224]",
            "movdqu xmm15, [rcx + 240]",
            "mov rsp, [rcx + 64]",
            "mov rax, r9",
            "ret",
        )
    }
}

#[cfg(target_arch = "aarch64")]
mod arch {
    use super::{Ctx, Entry};
    use core::ffi::c_void;

    pub(super) const LEAVES: bool = true;

    /// Save the state a return to the caller restores, then call
    /// `entry(env)` and return its answer.
    #[unsafe(naked)]
    pub(super) unsafe extern "C" fn enter(entry: Entry, env: *mut c_void, ctx: *mut Ctx) -> i64 {
        // x0 = entry, x1 = env, x2 = ctx; x30 holds the return address.
        core::arch::naked_asm!(
            "stp x19, x20, [x2, #0]",
            "stp x21, x22, [x2, #16]",
            "stp x23, x24, [x2, #32]",
            "stp x25, x26, [x2, #48]",
            "stp x27, x28, [x2, #64]",
            "stp x29, x30, [x2, #80]",
            "mov x9, sp",
            "str x9, [x2, #96]",
            "stp d8, d9, [x2, #104]",
            "stp d10, d11, [x2, #120]",
            "stp d12, d13, [x2, #136]",
            "stp d14, d15, [x2, #152]",
            "stp x29, x30, [sp, #-16]!",
            "mov x29, sp",
            "mov x9, x0",
            "mov x0, x1",
            "blr x9",
            "ldp x29, x30, [sp], #16",
            "ret",
        )
    }

    /// Restore `ctx` and return `code` from the `enter` that saved it.
    #[unsafe(naked)]
    pub(super) unsafe extern "C" fn leave(ctx: *const Ctx, code: i64) -> ! {
        // x0 = ctx, x1 = code.
        core::arch::naked_asm!(
            "ldp x19, x20, [x0, #0]",
            "ldp x21, x22, [x0, #16]",
            "ldp x23, x24, [x0, #32]",
            "ldp x25, x26, [x0, #48]",
            "ldp x27, x28, [x0, #64]",
            "ldp x29, x30, [x0, #80]",
            "ldr x9, [x0, #96]",
            "mov sp, x9",
            "ldp d8, d9, [x0, #104]",
            "ldp d10, d11, [x0, #120]",
            "ldp d12, d13, [x0, #136]",
            "ldp d14, d15, [x0, #152]",
            "mov x0, x1",
            "ret",
        )
    }
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
mod arch {
    use super::{Ctx, Entry};
    use core::ffi::c_void;

    /// No leave on this architecture: the park returns to the program,
    /// as before s260 (no CI target is one).
    pub(super) const LEAVES: bool = false;

    pub(super) unsafe extern "C" fn enter(entry: Entry, env: *mut c_void, _ctx: *mut Ctx) -> i64 {
        // SAFETY: the caller's contract.
        unsafe { entry(env) }
    }

    pub(super) unsafe extern "C" fn leave(_ctx: *const Ctx, _code: i64) -> ! {
        unreachable!("no leave on this architecture")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn returns(env: *mut c_void) -> i64 {
        env as i64 + 1
    }

    /// The entry's own answer comes back through `enter`, and the
    /// callee-saved state the caller holds survives the call (the
    /// optimizer keeps `keep` in such a register across it).
    #[test]
    fn enter_answers_the_entrys_value() {
        let keep: Vec<u64> = (0..64).collect();
        let r = unsafe { run_compiled(returns, std::ptr::without_provenance_mut(41)) };
        assert_eq!(r, 42);
        assert_eq!(keep.iter().sum::<u64>(), 2016);
        assert!(CURRENT.with(Cell::get).is_null(), "the frame is gone");
    }

    unsafe extern "C" fn deep(env: *mut c_void) -> i64 {
        // A few frames of "compiled" code below the entry, then a leave
        // forced by the test's flag (a killed scope stands in for it).
        fn down(n: u32, out: *mut u64) -> i64 {
            let mut pad = [n as u64; 32];
            std::hint::black_box(&mut pad);
            if n == 0 {
                let fp = CURRENT.with(Cell::get);
                unsafe {
                    (*fp).left = true;
                    arch::leave(&raw const (*fp).ctx, super::super::conc_abi::CANCEL_TAG)
                }
            }
            let r = down(n - 1, out);
            // Never reached on the leave: no frame below resumes.
            unsafe { *out += 1 };
            r + pad[0] as i64
        }
        down(16, env.cast())
    }

    /// A leave from frames below the entry lands back in `run_compiled`
    /// with the cancel tag, and none of those frames resumes.
    #[test]
    fn a_leave_discards_the_frames_below_the_entry() {
        if !arch::LEAVES {
            return;
        }
        let mut resumed = 0u64;
        let before: Vec<u64> = (0..64).collect();
        let r = unsafe { run_compiled(deep, (&raw mut resumed).cast()) };
        assert_eq!(r, super::super::conc_abi::CANCEL_TAG);
        assert_eq!(resumed, 0, "no discarded frame ran again");
        assert_eq!(
            before.iter().sum::<u64>(),
            2016,
            "the caller's state survived"
        );
        assert!(CURRENT.with(Cell::get).is_null(), "the frame is gone");
        // And the thread goes on entering tasks.
        assert_eq!(
            unsafe { run_compiled(returns, std::ptr::without_provenance_mut(1)) },
            2
        );
    }
}
