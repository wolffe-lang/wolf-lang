//! The controlling terminal (s219, wolf-lang#622; spec `[os.term]`):
//! the process group whose turn it is at the terminal, and the small
//! part of the terminal's mode an interactive program needs — whether
//! input comes a line at a time (canonical), whether typed characters
//! are echoed, whether the interrupt, quit and suspend characters
//! become signals, and how a non-canonical read waits (`VMIN`,
//! `VTIME`). POSIX.1-2024 XBD 11 is the reference; every other field of
//! the terminal's settings is left as it is found.
//!
//! **A mode is one int**, so a program saves it and restores it as a
//! value: bit 0 [`mode::CANONICAL`] (`ICANON`), bit 1 [`mode::ECHO`]
//! (`ECHO`), bit 2 [`mode::SIGNALS`] (`ISIG`), bit 3 [`mode::FLOW`]
//! (`IXON`) and bit 4 [`mode::EXTENDED`] (`IEXTEN`) — s234, #719 —
//! `VMIN` in bits 8..15 and `VTIME` (tenths of a second) in bits
//! 16..23. s248 (wolf-lang#802) adds bit 5 [`mode::RAW_CR`] (`ICRNL`
//! cleared), bit 6 [`mode::RAW_OUTPUT`] (`OPOST` cleared) and bit 7
//! [`mode::STRIP8`] (`ISTRIP` set). Any other bit set is `invalid`.
//!
//! **s234 (wolf-lang#719, W05)** adds what a line editor asks of the
//! terminal beside its mode: the window's size ([`size`], `TIOCGWINSZ`)
//! and the special characters the user chose ([`chars`]: erase, kill,
//! interrupt, … as `stty` sets them). The wait on a descriptor and a
//! signal set together is the signal hub's ([`crate::signal`]'s
//! `wait_fd`), because only the hub knows when a meaning is queued.
//!
//! **`SIGTTOU`.** A process outside the foreground group that changes
//! the foreground or the mode is sent `SIGTTOU` (default: stop) unless
//! it blocks or ignores it. A shell taking the terminal back from a
//! finished job is exactly such a process, so both setters block
//! `SIGTTOU` on the calling thread for the call ([`crate::os`]'s
//! `with_ttou_blocked`) — nothing else's mask moves.
//!
//! Handles are the fs family's: 0..2 the standard streams, 3 and up the
//! table (a `/dev/tty` opened with `fs_open_mode`, say). The call acts
//! on a duplicate held for its duration. windows: every call is
//! `unsupported`, by name — the console has no process groups.

use crate::os::proc_code;

/// The mode bits (`[os.term.mode]`).
pub mod mode {
    /// Input a line at a time, with the terminal's own editing (`ICANON`).
    pub const CANONICAL: i64 = 1;
    /// Typed characters are echoed (`ECHO`).
    pub const ECHO: i64 = 2;
    /// The interrupt, quit and suspend characters raise signals (`ISIG`).
    pub const SIGNALS: i64 = 4;
    /// s234: the start and stop characters (Ctrl-Q, Ctrl-S) resume and
    /// pause output, and never reach the program (`IXON`).
    pub const FLOW: i64 = 8;
    /// s234: the host's extended input processing — literal-next,
    /// word-erase, reprint, discard — is on (`IEXTEN`); with it on, macOS
    /// keeps Ctrl-V and Ctrl-O from the program even when canonical
    /// input is off.
    pub const EXTENDED: i64 = 16;
    /// s248 (wolf-lang#802): a typed carriage return reaches the program as
    /// CR (`ICRNL` CLEARED) — Enter is a key of its own, not Ctrl-J. The
    /// three s248 bits name the setting a terminal does NOT usually have,
    /// so 0 in each is what a terminal is given (`ICRNL` and `OPOST` on,
    /// `ISTRIP` off) and every mode a program built before them still sets
    /// what it set (ruling asked R7).
    pub const RAW_CR: i64 = 32;
    /// s248 (#802): bytes written reach the terminal as written — no
    /// output processing, so a LF is a LF and not CR LF (`OPOST` CLEARED).
    pub const RAW_OUTPUT: i64 = 64;
    /// s248 (#802): input bytes are stripped to seven bits (`ISTRIP` set).
    pub const STRIP8: i64 = 128;
    /// Every bit a mode may carry: the eight flags, `VMIN`, `VTIME`.
    pub const VALID: i64 = CANONICAL
        | ECHO
        | SIGNALS
        | FLOW
        | EXTENDED
        | RAW_CR
        | RAW_OUTPUT
        | STRIP8
        | 0xFF00
        | 0xFF_0000;
}

/// s234 (`[os.term.chars]`): how many special characters
/// `os_term_chars` answers, in this order: erase, kill, interrupt, quit,
/// suspend, end of file, word erase, literal next, start, stop,
/// reprint, discard.
pub const CHARS_COUNT: usize = 12;

#[cfg(unix)]
mod sys {
    use super::{mode, proc_code};
    use std::os::fd::AsRawFd as _;

    fn fd_of(h: i64) -> Result<std::fs::File, i64> {
        crate::fs::dup_of(h).ok_or(proc_code::IO)
    }

    pub fn foreground(h: i64) -> Result<i64, i64> {
        let f = fd_of(h)?;
        // SAFETY: a descriptor we hold.
        let pg = unsafe { libc::tcgetpgrp(f.as_raw_fd()) };
        if pg < 0 {
            Err(proc_code::IO)
        } else {
            Ok(i64::from(pg))
        }
    }

    pub fn set_foreground(h: i64, pgid: i64) -> Result<(), i64> {
        let Ok(pg) = libc::pid_t::try_from(pgid) else {
            return Err(proc_code::IO);
        };
        if pg <= 0 {
            return Err(proc_code::IO);
        }
        let f = fd_of(h)?;
        let fd = f.as_raw_fd();
        // SAFETY: a descriptor we hold and a plain group id.
        let r = crate::os::with_ttou_blocked(|| unsafe { libc::tcsetpgrp(fd, pg) });
        if r < 0 { Err(proc_code::IO) } else { Ok(()) }
    }

    fn get(fd: i32) -> Result<libc::termios, i64> {
        // SAFETY: a zeroed termios filled by tcgetattr.
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(fd, &mut t) < 0 {
                return Err(proc_code::IO);
            }
            Ok(t)
        }
    }

    pub fn mode_of(h: i64) -> Result<i64, i64> {
        let f = fd_of(h)?;
        let t = get(f.as_raw_fd())?;
        let mut m = 0i64;
        if t.c_lflag & libc::ICANON != 0 {
            m |= mode::CANONICAL;
        }
        if t.c_lflag & libc::ECHO != 0 {
            m |= mode::ECHO;
        }
        if t.c_lflag & libc::ISIG != 0 {
            m |= mode::SIGNALS;
        }
        if t.c_iflag & libc::IXON != 0 {
            m |= mode::FLOW;
        }
        if t.c_lflag & libc::IEXTEN != 0 {
            m |= mode::EXTENDED;
        }
        // s248 (#802): the three bits that name what a terminal does not
        // usually have.
        if t.c_iflag & libc::ICRNL == 0 {
            m |= mode::RAW_CR;
        }
        if t.c_oflag & libc::OPOST == 0 {
            m |= mode::RAW_OUTPUT;
        }
        if t.c_iflag & libc::ISTRIP != 0 {
            m |= mode::STRIP8;
        }
        m |= i64::from(t.c_cc[libc::VMIN]) << 8;
        m |= i64::from(t.c_cc[libc::VTIME]) << 16;
        Ok(m)
    }

    pub fn set_mode(h: i64, m: i64) -> Result<(), i64> {
        if m < 0 || m & !mode::VALID != 0 {
            return Err(proc_code::INVALID);
        }
        let f = fd_of(h)?;
        let fd = f.as_raw_fd();
        let mut t = get(fd)?;
        apply_bits(&mut t, m);
        // TCSADRAIN: output already written lands under the old mode;
        // input already typed is kept.
        // SAFETY: a descriptor we hold and a termios tcgetattr filled.
        let r =
            crate::os::with_ttou_blocked(|| unsafe { libc::tcsetattr(fd, libc::TCSADRAIN, &t) });
        if r < 0 { Err(proc_code::IO) } else { Ok(()) }
    }

    /// The mode's fields written into `t`, every other field as found —
    /// `set_mode`'s and the exit restore's one encoding (s248: the
    /// restore runs it in a signal handler too, so it only assigns).
    pub(super) fn apply_bits(t: &mut libc::termios, m: i64) {
        for (bit, flag) in [
            (mode::CANONICAL, libc::ICANON),
            (mode::ECHO, libc::ECHO),
            (mode::SIGNALS, libc::ISIG),
            (mode::EXTENDED, libc::IEXTEN),
        ] {
            if m & bit != 0 {
                t.c_lflag |= flag;
            } else {
                t.c_lflag &= !flag;
            }
        }
        // s234: IXON is an input flag, the rest local flags.
        if m & mode::FLOW != 0 {
            t.c_iflag |= libc::IXON;
        } else {
            t.c_iflag &= !libc::IXON;
        }
        // s248 (#802): ICRNL and ISTRIP are input flags, OPOST the output
        // one; RAW_CR and RAW_OUTPUT CLEAR theirs.
        // s248 PLANT (never merge): bits 5-7 change nothing.
        t.c_cc[libc::VMIN] = ((m >> 8) & 0xFF) as libc::cc_t;
        t.c_cc[libc::VTIME] = ((m >> 16) & 0xFF) as libc::cc_t;
    }

    /// s248 (`[os.term.restore]`): a duplicate of the descriptor `h`
    /// names, at 3 or above and close-on-exec (never one of the standard
    /// three, s245's rule), checked to be a terminal; `IO` for a closed,
    /// forged or non-terminal handle, or a duplicate the host refuses.
    pub fn hold(h: i64) -> Result<libc::c_int, i64> {
        let f = fd_of(h)?;
        // SAFETY: a duplicate of a descriptor we hold, owned from here.
        let d = unsafe { libc::fcntl(f.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
        if d < 0 {
            return Err(proc_code::IO);
        }
        if get(d).is_err() {
            // SAFETY: the duplicate is ours.
            unsafe { libc::close(d) };
            return Err(proc_code::IO);
        }
        Ok(d)
    }

    /// s248 (`[os.term.restore]`): the registered mode set on `fd` and the
    /// bytes written after it. Only async-signal-safe calls (`tcgetattr`,
    /// `pthread_sigmask`, `tcsetattr`, `write`) and no allocation: the
    /// stack guard runs this in its handler. A descriptor that is no
    /// longer a terminal is left alone, bytes and all.
    pub fn restore_on(fd: libc::c_int, m: i64, bytes: &[u8]) {
        // SAFETY: a zeroed termios filled by tcgetattr on our duplicate.
        let mut t: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut t) } < 0 {
            return;
        }
        apply_bits(&mut t, m);
        // SAFETY: our duplicate and a termios tcgetattr filled.
        let _ =
            crate::os::with_ttou_blocked(|| unsafe { libc::tcsetattr(fd, libc::TCSADRAIN, &t) });
        let mut at = 0;
        while at < bytes.len() {
            // SAFETY: the registration's own bytes, on our duplicate.
            let n = unsafe { libc::write(fd, bytes[at..].as_ptr().cast(), bytes.len() - at) };
            if n > 0 {
                at += n as usize;
                continue;
            }
            // SAFETY: errno read after the failed write.
            if n < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break;
        }
    }

    pub fn pgid() -> Result<i64, i64> {
        // SAFETY: getpgrp cannot fail.
        Ok(i64::from(unsafe { libc::getpgrp() }))
    }

    /// s241 (#762, `[os.term.group]`): `setpgid(0, pgid)` — the CALLING
    /// process into group `pgid`, or leading a new group of its own for
    /// 0. The shape (`INVALID`: below 0 or past a pid's width) is
    /// decided before the host; `EPERM` (a session leader, a group of
    /// another session or none at all) is `DENIED`, `EINVAL` `INVALID`,
    /// anything else `IO`.
    pub fn set_pgid(pgid: i64) -> Result<(), i64> {
        let Ok(g) = libc::pid_t::try_from(pgid) else {
            return Err(proc_code::INVALID);
        };
        if g < 0 {
            return Err(proc_code::INVALID);
        }
        // SAFETY: setpgid on this process (pid 0) with a checked group id.
        if unsafe { libc::setpgid(0, g) } == 0 {
            return Ok(());
        }
        Err(match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EPERM) => proc_code::DENIED,
            Some(libc::EINVAL) => proc_code::INVALID,
            _ => proc_code::IO,
        })
    }

    /// s234 (`[os.term.size]`): the window's rows and columns as the
    /// terminal reports them (`TIOCGWINSZ`); 0 for a size it was never
    /// told (a fresh pseudo-terminal).
    pub fn size(h: i64) -> Result<(i64, i64), i64> {
        let f = fd_of(h)?;
        // SAFETY: a zeroed winsize filled by the ioctl on a descriptor
        // we hold.
        unsafe {
            let mut w: libc::winsize = std::mem::zeroed();
            if libc::ioctl(f.as_raw_fd(), libc::TIOCGWINSZ, &mut w) < 0 {
                return Err(proc_code::IO);
            }
            Ok((i64::from(w.ws_row), i64::from(w.ws_col)))
        }
    }

    /// The value a disabled special character reads as (measured
    /// black-box with `stty intr undef`: 0 on linux, 255 on macOS and the
    /// BSDs).
    #[cfg(any(target_os = "linux", target_os = "android"))]
    const DISABLED: libc::cc_t = 0;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    const DISABLED: libc::cc_t = 0xFF;

    /// s234 (`[os.term.chars]`): the twelve special characters, in
    /// [`super::CHARS_COUNT`]'s order, each its byte or -1 when disabled.
    pub fn chars(h: i64) -> Result<[i64; super::CHARS_COUNT], i64> {
        let f = fd_of(h)?;
        let t = get(f.as_raw_fd())?;
        let idx = [
            libc::VERASE,
            libc::VKILL,
            libc::VINTR,
            libc::VQUIT,
            libc::VSUSP,
            libc::VEOF,
            libc::VWERASE,
            libc::VLNEXT,
            libc::VSTART,
            libc::VSTOP,
            libc::VREPRINT,
            libc::VDISCARD,
        ];
        let mut out = [0i64; super::CHARS_COUNT];
        for (slot, &i) in out.iter_mut().zip(idx.iter()) {
            let c = t.c_cc[i];
            *slot = if c == DISABLED { -1 } else { i64::from(c) };
        }
        Ok(out)
    }
}

#[cfg(not(unix))]
mod sys {
    use super::proc_code;
    pub fn foreground(_h: i64) -> Result<i64, i64> {
        Err(proc_code::UNSUPPORTED)
    }
    pub fn set_foreground(_h: i64, _pgid: i64) -> Result<(), i64> {
        Err(proc_code::UNSUPPORTED)
    }
    pub fn mode_of(_h: i64) -> Result<i64, i64> {
        Err(proc_code::UNSUPPORTED)
    }
    pub fn set_mode(_h: i64, m: i64) -> Result<(), i64> {
        // The shape first, as on unix: `invalid` before the host.
        if m < 0 || m & !super::mode::VALID != 0 {
            return Err(proc_code::INVALID);
        }
        Err(proc_code::UNSUPPORTED)
    }
    pub fn pgid() -> Result<i64, i64> {
        Err(proc_code::UNSUPPORTED)
    }
    pub fn set_pgid(pgid: i64) -> Result<(), i64> {
        if pgid < 0 || pgid > i64::from(i32::MAX) {
            return Err(proc_code::INVALID);
        }
        Err(proc_code::UNSUPPORTED)
    }
    pub fn size(_h: i64) -> Result<(i64, i64), i64> {
        Err(proc_code::UNSUPPORTED)
    }
    pub fn chars(_h: i64) -> Result<[i64; super::CHARS_COUNT], i64> {
        Err(proc_code::UNSUPPORTED)
    }
}

pub use sys::{chars, foreground, mode_of, pgid, set_foreground, set_mode, set_pgid, size};

// ---- s248: the restore at exit (`[os.term.restore]`, wolf-lang#794) ----
//
// Ruling #93 R1 C: the runtime restores the terminal on every exit it
// owns, in the runtime, never in a handler of the program's. ONE
// registration a process: a duplicate of the terminal's descriptor (>= 3,
// close-on-exec), the mode, the bytes to write after it, and the pid that
// registered (a forked child is another process and restores nothing).
// The last registration wins. It is run — and taken, so once — from each
// exit the runtime owns, as prof's dump is (explicit sites, no `atexit`):
// the trap reports (`native.rs`), a main's error, `os_exit`, a return from
// `main` (the entry shim calls [`__wolf_rt_term_restore_exit`] in a
// program that names the registration), the proc root's death,
// `os_exit_signal` before its raise, and the stack guard after its report
// — the one site in a signal handler, so the registration is guarded by
// an atomic claim the handler tries once, and the restore allocates and
// frees nothing.

#[cfg(unix)]
mod at_exit {
    use std::cell::UnsafeCell;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub(super) struct Restore {
        pub pid: libc::pid_t,
        pub fd: libc::c_int,
        pub mode: i64,
        pub bytes: Box<[u8]>,
    }

    struct Slot(UnsafeCell<Option<Restore>>);
    // SAFETY: every access to the cell holds `CLAIM`.
    unsafe impl Sync for Slot {}

    static CLAIM: AtomicBool = AtomicBool::new(false);
    static SLOT: Slot = Slot(UnsafeCell::new(None));

    /// Claim the slot; `wait` false tries once (a handler may not spin on
    /// a claim its own thread may hold).
    fn claim(wait: bool) -> bool {
        loop {
            if CLAIM
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return true;
            }
            if !wait {
                return false;
            }
            std::thread::yield_now();
        }
    }

    fn release() {
        CLAIM.store(false, Ordering::Release);
    }

    /// The registration, replacing any before it (whose duplicate is
    /// closed: no exit is using it — an exit takes the slot first).
    pub(super) fn register(r: Restore) {
        claim(true);
        // SAFETY: `CLAIM` is held.
        let old = unsafe { (*SLOT.0.get()).replace(r) };
        release();
        if let Some(o) = old {
            // SAFETY: the old registration's own duplicate.
            unsafe { libc::close(o.fd) };
        }
    }

    /// Take the registration and restore, in this process only. The
    /// process is ending, so nothing is freed (the handler may not).
    pub(super) fn run(in_handler: bool) {
        // s248 PLANT (never merge): the restore runs nothing.
        if in_handler || !in_handler {
            return;
        }
        if !claim(!in_handler) {
            return;
        }
        // SAFETY: `CLAIM` is held.
        let r = unsafe { (*SLOT.0.get()).take() };
        release();
        let Some(r) = r else {
            return;
        };
        // SAFETY: getpid cannot fail.
        if unsafe { libc::getpid() } == r.pid {
            super::sys::restore_on(r.fd, r.mode, &r.bytes);
        }
        std::mem::forget(r);
    }
}

/// `os_term_restore_at_exit(fd, mode, bytes)` (s248, `[os.term.restore]`):
/// register the restore. The order of refusal: a mode outside the bits is
/// `INVALID` before the handle is read; then windows is `UNSUPPORTED`;
/// then a closed, forged or non-terminal handle (or a duplicate the host
/// will not make) is `IO`. A mode the terminal already has is allowed.
pub fn restore_at_exit(h: i64, m: i64, bytes: &[u8]) -> Result<(), i64> {
    if m < 0 || m & !mode::VALID != 0 {
        return Err(proc_code::INVALID);
    }
    #[cfg(unix)]
    {
        let fd = sys::hold(h)?;
        at_exit::register(at_exit::Restore {
            // SAFETY: getpid cannot fail.
            pid: unsafe { libc::getpid() },
            fd,
            mode: m,
            bytes: bytes.into(),
        });
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (h, bytes);
        Err(proc_code::UNSUPPORTED)
    }
}

/// s248: run the registered restore, once, from an exit the runtime owns
/// (not a signal handler). Nothing registered, nothing done.
pub fn restore_now() {
    #[cfg(unix)]
    at_exit::run(false);
}

/// s248: [`restore_now`] from the stack guard's handler: one try at the
/// claim, and only async-signal-safe calls.
pub fn restore_in_handler() {
    #[cfg(unix)]
    at_exit::run(true);
}

// ---- the C entry surface (codes are `os::proc_code`'s) -----------------

/// `os_term_foreground(fd) -> int ! {io, unsupported}` — the foreground
/// group (>= 0) or `-code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_term_foreground(h: i64) -> i64 {
    foreground(h).unwrap_or_else(|c| -c)
}

/// `os_term_set_foreground(fd, pgid) -> () ! {io, unsupported}` — 0 or
/// the code.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_term_set_foreground(h: i64, pgid: i64) -> i64 {
    set_foreground(h, pgid).err().unwrap_or(proc_code::OK)
}

/// `os_term_mode(fd) -> int ! {io, unsupported}` — the packed mode
/// (>= 0) or `-code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_term_mode(h: i64) -> i64 {
    mode_of(h).unwrap_or_else(|c| -c)
}

/// `os_term_set_mode(fd, mode) -> () ! {invalid, io, unsupported}` — 0
/// or the code.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_term_set_mode(h: i64, m: i64) -> i64 {
    set_mode(h, m).err().unwrap_or(proc_code::OK)
}

/// `os_term_restore_at_exit(fd, mode, bytes: List[byte]) -> () ! {invalid,
/// io, unsupported}` (s248, wolf-lang#794) — 0 or the code. The bytes are
/// copied at the call.
///
/// # Safety
///
/// `bytes` must be a live `List[byte]` header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_term_restore_at_exit(h: i64, m: i64, bytes: i64) -> i64 {
    let Some(b) = (unsafe { crate::list::u8_elems(bytes) }) else {
        return proc_code::INVALID;
    };
    restore_at_exit(h, m, b).err().unwrap_or(proc_code::OK)
}

/// s248 (`[os.term.restore]`): the restore on a return from `main` — the
/// entry shim calls this before it returns, in a program that names
/// `os_term_restore_at_exit`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_term_restore_exit() {
    restore_now();
}

/// `os_pgid() -> int ! {unsupported}` — this process's group (>= 0) or
/// `-code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_pgid() -> i64 {
    pgid().unwrap_or_else(|c| -c)
}

/// `os_set_pgid(pgid) -> () ! {denied, invalid, io, unsupported}` (s241)
/// — 0 ok, else the code.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_set_pgid(pgid: i64) -> i64 {
    set_pgid(pgid).map_or_else(|c| c, |()| proc_code::OK)
}

/// `os_term_size(fd) -> (int, int) ! {io, unsupported}` (s234) — 0 and
/// the pair (rows, columns) through `out`, or the code.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_term_size(h: i64, out: i64) -> i64 {
    match size(h) {
        Ok((rows, cols)) => {
            unsafe { crate::str::write_pair(out, rows, cols) };
            proc_code::OK
        }
        Err(c) => c,
    }
}

/// `os_term_chars(fd) -> List[int] ! {io, unsupported}` (s234) — 0 and
/// the minted twelve-word list through `out`, or the code. The list is
/// minted in the ambient region after the terminal is read.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_term_chars(h: i64, out: i64) -> i64 {
    match chars(h) {
        Ok(cs) => {
            let hdr = crate::list::new_list(8);
            for c in cs {
                crate::list::push_int(hdr, c);
            }
            unsafe { crate::str::write_word(out, hdr as i64) };
            proc_code::OK
        }
        Err(c) => c,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A file is not a terminal: every terminal call on it is `io`, and
    /// a forged handle is `io` before any syscall.
    #[test]
    fn not_a_terminal_is_io() {
        let p = std::env::temp_dir().join(format!("wolf-rt-term-{}", std::process::id()));
        std::fs::write(&p, b"x").unwrap();
        let h = crate::fs::mint_file(std::fs::File::open(&p).unwrap());
        assert_eq!(mode_of(h), Err(proc_code::IO));
        assert_eq!(set_mode(h, mode::CANONICAL), Err(proc_code::IO));
        assert_eq!(foreground(h), Err(proc_code::IO));
        assert_eq!(set_foreground(h, 1), Err(proc_code::IO));
        assert_eq!(mode_of(1_000_000), Err(proc_code::IO));
        let _ = std::fs::remove_file(&p);
    }

    /// A mode with a bit outside the eight flags, VMIN and VTIME is
    /// `invalid` before the handle is read. s248 (#802): bits 5..7 are
    /// fields now, so a forged handle is `io` for them; bit 24 is the first
    /// bit that is none.
    #[test]
    fn a_mode_outside_the_bits_is_invalid() {
        assert_eq!(set_mode(1_000_000, 32), Err(proc_code::IO));
        assert_eq!(set_mode(1_000_000, 64 | 128), Err(proc_code::IO));
        assert_eq!(set_mode(1_000_000, 1 << 24), Err(proc_code::INVALID));
        assert_eq!(set_mode(1_000_000, -1), Err(proc_code::INVALID));
    }

    /// The group is this process's own (a test runner is in some group).
    #[test]
    fn pgid_is_the_process_group() {
        // SAFETY: getpgrp cannot fail.
        assert_eq!(pgid(), Ok(i64::from(unsafe { libc::getpgrp() })));
    }

    /// A fresh pseudo-terminal: the master's descriptor and the slave
    /// as a table handle (POSIX `posix_openpt`, `grantpt`, `unlockpt`,
    /// `ptsname`).
    fn pty() -> (std::fs::File, i64) {
        use std::os::fd::FromRawFd as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        // SAFETY: the POSIX pseudo-terminal calls on a descriptor we own.
        unsafe {
            let m = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
            assert!(m >= 0, "posix_openpt");
            assert_eq!(libc::grantpt(m), 0);
            assert_eq!(libc::unlockpt(m), 0);
            let name = std::ffi::CStr::from_ptr(libc::ptsname(m))
                .to_string_lossy()
                .into_owned();
            let slave = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NOCTTY)
                .open(name)
                .expect("open the slave");
            (std::fs::File::from_raw_fd(m), crate::fs::mint_file(slave))
        }
    }

    /// s234 (`[os.term.size]`): the size the other side set, and a new
    /// one after a resize; a file is `io`.
    #[test]
    fn size_reads_what_the_terminal_was_told() {
        use std::os::fd::AsRawFd as _;
        let (m, s) = pty();
        for (rows, cols) in [(24u16, 80u16), (30, 100)] {
            // SAFETY: a winsize for the master's ioctl.
            unsafe {
                let w = libc::winsize {
                    ws_row: rows,
                    ws_col: cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                assert_eq!(libc::ioctl(m.as_raw_fd(), libc::TIOCSWINSZ, &w), 0);
            }
            assert_eq!(size(s), Ok((i64::from(rows), i64::from(cols))));
        }
        assert_eq!(size(1_000_000), Err(proc_code::IO));
    }

    /// s234 (`[os.term.chars]`, `[os.term.mode]`): the interrupt
    /// character the other side moved, a disabled one as -1; FLOW and
    /// EXTENDED read back what is set, and a mode read and set again
    /// changes nothing.
    #[test]
    fn chars_and_the_two_new_bits() {
        use std::os::fd::AsRawFd as _;
        let (m, s) = pty();
        let base = chars(s).expect("chars");
        assert_eq!(base.len(), CHARS_COUNT);
        // SAFETY: tcgetattr / tcsetattr on the master, which sets the
        // pair's settings.
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            assert_eq!(libc::tcgetattr(m.as_raw_fd(), &mut t), 0);
            t.c_cc[libc::VINTR] = 0x18;
            t.c_cc[libc::VLNEXT] = sys_disabled();
            assert_eq!(libc::tcsetattr(m.as_raw_fd(), libc::TCSANOW, &t), 0);
        }
        let now = chars(s).expect("chars");
        assert_eq!(now[2], 0x18, "interrupt is Ctrl-X");
        assert_eq!(now[7], -1, "literal next disabled");
        assert_eq!(now[0], base[0], "erase unchanged");
        let old = mode_of(s).expect("mode");
        let raw =
            old & !(mode::CANONICAL | mode::ECHO | mode::SIGNALS | mode::FLOW | mode::EXTENDED);
        assert_eq!(set_mode(s, raw), Ok(()));
        assert_eq!(mode_of(s), Ok(raw));
        // SAFETY: a read of the pair's settings.
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            assert_eq!(libc::tcgetattr(m.as_raw_fd(), &mut t), 0);
            assert_eq!(t.c_iflag & libc::IXON, 0, "IXON off");
            assert_eq!(t.c_lflag & libc::IEXTEN, 0, "IEXTEN off");
        }
        assert_eq!(set_mode(s, old), Ok(()));
        assert_eq!(mode_of(s), Ok(old));
        assert_eq!(
            set_mode(s, 1 << 24),
            Err(proc_code::INVALID),
            "bit 24 is not a field"
        );
        assert_eq!(chars(1_000_000), Err(proc_code::IO));
    }

    /// s248 (#802, `[os.term.mode]`): RAW_CR clears ICRNL, RAW_OUTPUT
    /// clears OPOST, STRIP8 sets ISTRIP, each read back; a fresh terminal
    /// reads 0 in all three (the usual settings); and a mode without them
    /// gives the usual settings back.
    #[test]
    fn the_three_raw_bits() {
        use std::os::fd::AsRawFd as _;
        let (m, s) = pty();
        let old = mode_of(s).expect("mode");
        assert_eq!(
            old & (mode::RAW_CR | mode::RAW_OUTPUT | mode::STRIP8),
            0,
            "usual: {old}"
        );
        let all = old | mode::RAW_CR | mode::RAW_OUTPUT | mode::STRIP8;
        assert_eq!(set_mode(s, all), Ok(()));
        assert_eq!(mode_of(s), Ok(all));
        // SAFETY: a read of the pair's settings on the master.
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            assert_eq!(libc::tcgetattr(m.as_raw_fd(), &mut t), 0);
            assert_eq!(t.c_iflag & libc::ICRNL, 0, "ICRNL off");
            assert_eq!(t.c_oflag & libc::OPOST, 0, "OPOST off");
            assert_ne!(t.c_iflag & libc::ISTRIP, 0, "ISTRIP on");
        }
        assert_eq!(set_mode(s, old), Ok(()));
        assert_eq!(mode_of(s), Ok(old), "the usual settings back");
    }

    /// s248 (`[os.term.restore]`): the registered mode comes back and the
    /// bytes follow it; the last registration wins; it runs once; a mode
    /// outside the bits is `invalid` and a file `io`.
    #[test]
    fn the_restore_sets_the_mode_and_writes_the_bytes_once() {
        use std::io::Read as _;
        use std::os::fd::AsRawFd as _;
        let (mut m, s) = pty();
        // SAFETY: the master non-blocking, so an empty read answers at once.
        unsafe {
            let fl = libc::fcntl(m.as_raw_fd(), libc::F_GETFL);
            libc::fcntl(m.as_raw_fd(), libc::F_SETFL, fl | libc::O_NONBLOCK);
        }
        let saved = mode_of(s).expect("mode");
        let raw = saved & !(mode::CANONICAL | mode::ECHO) | 256;
        assert_eq!(restore_at_exit(s, 1 << 24, b"x"), Err(proc_code::INVALID));
        let p = std::env::temp_dir().join(format!("wolf-rt-restore-{}", std::process::id()));
        std::fs::write(&p, b"x").unwrap();
        let file = crate::fs::mint_file(std::fs::File::open(&p).unwrap());
        assert_eq!(restore_at_exit(file, saved, b"x"), Err(proc_code::IO));
        assert_eq!(restore_at_exit(1_000_000, saved, b"x"), Err(proc_code::IO));
        let _ = std::fs::remove_file(&p);
        assert_eq!(restore_at_exit(s, saved, b"first"), Ok(()));
        assert_eq!(restore_at_exit(s, saved, b"LEFT"), Ok(()));
        assert_eq!(set_mode(s, raw), Ok(()));
        assert_eq!(mode_of(s), Ok(raw));
        restore_now();
        assert_eq!(mode_of(s), Ok(saved), "the registered mode is back");
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut got = Vec::new();
        let mut b = [0u8; 64];
        while let Ok(n) = m.read(&mut b) {
            if n == 0 {
                break;
            }
            got.extend_from_slice(&b[..n]);
        }
        assert_eq!(got, b"LEFT", "the last registration's bytes, once");
        assert_eq!(set_mode(s, raw), Ok(()));
        restore_now();
        assert_eq!(mode_of(s), Ok(raw), "taken: a second exit restores nothing");
        assert_eq!(set_mode(s, saved), Ok(()));
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn sys_disabled() -> libc::cc_t {
        0
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    fn sys_disabled() -> libc::cc_t {
        0xFF
    }
}
