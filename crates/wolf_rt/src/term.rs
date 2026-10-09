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
//! (`ECHO`), bit 2 [`mode::SIGNALS`] (`ISIG`), `VMIN` in bits 8..15 and
//! `VTIME` (tenths of a second) in bits 16..23. Any other bit set is
//! `invalid`.
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
    /// Every bit a mode may carry: the three flags, `VMIN`, `VTIME`.
    pub const VALID: i64 = CANONICAL | ECHO | SIGNALS | 0xFF00 | 0xFF_0000;
}

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
        ] {
            if m & bit != 0 {
                t.c_lflag |= flag;
            } else {
                t.c_lflag &= !flag;
            }
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
    pub fn set_mode(_h: i64, _m: i64) -> Result<(), i64> {
        Err(proc_code::UNSUPPORTED)
    }
    pub fn pgid() -> Result<i64, i64> {
        Err(proc_code::UNSUPPORTED)
    }
}

pub use sys::{foreground, mode_of, pgid, set_foreground, set_mode};

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
        assert_eq!(set_mode(1_000_000, 8), Err(proc_code::INVALID));
        assert_eq!(set_mode(1_000_000, 1 << 24), Err(proc_code::INVALID));
        assert_eq!(set_mode(1_000_000, -1), Err(proc_code::INVALID));
    }

    /// The group is this process's own (a test runner is in some group).
    #[test]
    fn pgid_is_the_process_group() {
        // SAFETY: getpgrp cannot fail.
        assert_eq!(pgid(), Ok(i64::from(unsafe { libc::getpgrp() })));
    }
}
