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
//! 16..23. Any other bit set is `invalid`.
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
    /// Every bit a mode may carry: the five flags, `VMIN`, `VTIME`.
    pub const VALID: i64 = CANONICAL | ECHO | SIGNALS | FLOW | EXTENDED | 0xFF00 | 0xFF_0000;
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
            m |= 0;
        }
        if t.c_lflag & libc::IEXTEN != 0 {
            m |= mode::EXTENDED;
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
        t.c_cc[libc::VMIN] = ((m >> 8) & 0xFF) as libc::cc_t;
        t.c_cc[libc::VTIME] = ((m >> 16) & 0xFF) as libc::cc_t;
        // TCSADRAIN: output already written lands under the old mode;
        // input already typed is kept.
        // SAFETY: a descriptor we hold and a termios tcgetattr filled.
        let r =
            crate::os::with_ttou_blocked(|| unsafe { libc::tcsetattr(fd, libc::TCSADRAIN, &t) });
        if r < 0 { Err(proc_code::IO) } else { Ok(()) }
    }

    pub fn pgid() -> Result<i64, i64> {
        // SAFETY: getpgrp cannot fail.
        Ok(i64::from(unsafe { libc::getpgrp() }))
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
            Ok((i64::from(w.ws_col), i64::from(w.ws_row)))
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
    pub fn size(_h: i64) -> Result<(i64, i64), i64> {
        Err(proc_code::UNSUPPORTED)
    }
    pub fn chars(_h: i64) -> Result<[i64; super::CHARS_COUNT], i64> {
        Err(proc_code::UNSUPPORTED)
    }
}

pub use sys::{chars, foreground, mode_of, pgid, set_foreground, set_mode, size};

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

/// `os_pgid() -> int ! {unsupported}` — this process's group (>= 0) or
/// `-code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_pgid() -> i64 {
    pgid().unwrap_or_else(|c| -c)
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

    /// A mode with a bit outside the three flags, VMIN and VTIME is
    /// `invalid` before the handle is read.
    #[test]
    fn a_mode_outside_the_bits_is_invalid() {
        assert_eq!(set_mode(1_000_000, 32), Err(proc_code::INVALID));
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
            set_mode(s, 32),
            Err(proc_code::INVALID),
            "bit 5 is not a field"
        );
        assert_eq!(chars(1_000_000), Err(proc_code::IO));
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
