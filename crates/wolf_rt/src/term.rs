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
//! [`mode::STRIP8`] (`ISTRIP` set). s272 (wolf-lang#976) adds bit 24
//! [`mode::RAW_LF`] (`ONLCR` cleared). Any other bit set is `invalid`.
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
//! **s292** adds a pseudo-terminal and the settings a program on it is
//! given: [`open_pty`] (the master and a slave, `[os.term.pty]`),
//! [`pty_name`], [`set_size`] (`[os.term.size.set]`), and [`attr`] and
//! [`set_attr`], one setting at a time by RFC 4254 section 8's opcode
//! (`[os.term.attr]`, the vocabulary in [`attr`](mod@attr)).
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
    /// s272 (wolf-lang#976, R-s272-7): a written LF reaches the terminal as
    /// LF even with output processing on — the newline is not mapped to CR
    /// LF (`ONLCR` CLEARED). s248's family rule: the bit names the setting
    /// a terminal does not usually have, so 0 is what a terminal is given
    /// and a mode read and set back keeps `ONLCR` exactly as found. Above
    /// `VTIME`'s byte, so every mode built before it reads and sets the
    /// same int on a terminal that maps newlines (yew's REPL needs output
    /// processing WITH the mapping: `RAW_OUTPUT` and `RAW_LF` clear).
    pub const RAW_LF: i64 = 1 << 24;
    /// Every bit a mode may carry: the nine flags, `VMIN`, `VTIME`.
    pub const VALID: i64 = CANONICAL
        | ECHO
        | SIGNALS
        | FLOW
        | EXTENDED
        | RAW_CR
        | RAW_OUTPUT
        | STRIP8
        | 0xFF00
        | 0xFF_0000
        | RAW_LF;
}

/// s234 (`[os.term.chars]`): how many special characters
/// `os_term_chars` answers, in this order: erase, kill, interrupt, quit,
/// suspend, end of file, word erase, literal next, start, stop,
/// reprint, discard.
pub const CHARS_COUNT: usize = 12;

/// s292 (`[os.term.attr]`): the vocabulary of `os_term_attr` and
/// `os_term_set_attr` — the opcodes RFC 4254 section 8 gives a terminal's
/// modes, with RFC 8160's 42 (IUTF8) — and the runtime's own table of
/// speeds. Where each op lives in a host's `termios`, and which ops a host
/// lacks, is `sys::field`'s.
pub mod attr {
    /// What an op names, and so the values it reads and takes.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Kind {
        /// A special character (`c_cc`): its byte 0..255, or -1 disabled.
        Char,
        /// A flag: 0 clear, 1 set.
        Flag,
        /// The character size, 90 CS7 and 91 CS8: 1 when it is that size.
        /// A set of 1 selects it; a set of 0 leaves the size as found.
        Size,
        /// A speed in bits per second, through [`SPEEDS`].
        Speed,
    }

    /// Every op of the vocabulary, in order: 56.
    pub const OPS: [i64; 56] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 30, 31, 32, 33, 34, 35, 36,
        37, 38, 39, 40, 41, 42, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 70, 71, 72, 73,
        74, 75, 90, 91, 92, 93, 128, 129,
    ];

    /// The rates, in bits per second, that every host names. A speed
    /// crosses the C library only through this table, never as a `B`
    /// constant's value (those differ between C libraries: glibc 2.42's
    /// are the rates, older ones codes).
    pub const SPEEDS: [i64; 19] = [
        0, 50, 75, 110, 134, 150, 200, 300, 600, 1200, 1800, 2400, 4800, 9600, 19200, 38400, 57600,
        115200, 230400,
    ];

    /// The kind of `op`; `None` for an op outside the vocabulary
    /// (`invalid`).
    pub fn kind(op: i64) -> Option<Kind> {
        match op {
            1..=18 => Some(Kind::Char),
            30..=42 | 50..=62 | 70..=75 | 92 | 93 => Some(Kind::Flag),
            90 | 91 => Some(Kind::Size),
            128 | 129 => Some(Kind::Speed),
            _ => None,
        }
    }

    /// Whether `v` is a value of `kind`'s range (`invalid` otherwise): a
    /// character -1..255, a flag or a size 0..1, a speed 0..4294967295.
    pub fn fits(kind: Kind, v: i64) -> bool {
        match kind {
            Kind::Char => (-1..=255).contains(&v),
            Kind::Flag | Kind::Size => (0..=1).contains(&v),
            Kind::Speed => (0..=4_294_967_295).contains(&v),
        }
    }

    /// `os_term_set_size`'s shape: each of the four 0..65535.
    pub fn size_fits(fields: [i64; 4]) -> bool {
        fields.iter().all(|v| (0..=65535).contains(v))
    }
}

/// s292 (`[os.term.pty]`): the runtime makes its pty opens one at a time.
/// POSIX does not make `ptsname` thread-safe, and macOS's `posix_openpt`
/// failed when eight threads opened at once (s272's tests); the unit
/// tests' own opens take this lock too.
pub(crate) static PTY_OPEN: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
        // s272 (#976): the output newline mapping, read whatever OPOST is.
        if t.c_oflag & libc::ONLCR == 0 {
            m |= mode::RAW_LF;
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
        if m & mode::RAW_CR != 0 {
            t.c_iflag &= !libc::ICRNL;
        } else {
            t.c_iflag |= libc::ICRNL;
        }
        if m & mode::RAW_OUTPUT != 0 {
            t.c_oflag &= !libc::OPOST;
        } else {
            t.c_oflag |= libc::OPOST;
        }
        if m & mode::STRIP8 != 0 {
            t.c_iflag |= libc::ISTRIP;
        } else {
            t.c_iflag &= !libc::ISTRIP;
        }
        // s272 (#976): ONLCR is an output flag; RAW_LF CLEARS it. It acts
        // only while OPOST is on, and is kept as the mode says either way,
        // so a restore puts back the caller's mapping exactly.
        if m & mode::RAW_LF != 0 {
            t.c_oflag &= !libc::ONLCR;
        } else {
            t.c_oflag |= libc::ONLCR;
        }
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

    /// s290 (`[os.term.session]`): this process's session id,
    /// `getsid(0)`. It cannot fail for the caller itself (its two errors
    /// name another process), as `getpgrp` cannot.
    pub fn sid() -> Result<i64, i64> {
        // SAFETY: getsid of this process (pid 0).
        Ok(i64::from(unsafe { libc::getsid(0) }))
    }

    /// s290 (`[os.term.session]`): `setsid()` — the CALLING process leads
    /// a new session and a new group, both ids its pid, with no
    /// controlling terminal. One system call on the process's own state,
    /// so any thread may make it. `EPERM` (the caller leads a group, a
    /// session leader included) is `DENIED`, anything else `IO`; the
    /// host's number goes to `os_error()`.
    pub fn setsid() -> Result<(), i64> {
        crate::fs::os_error_clear();
        // SAFETY: setsid on this process; it takes no argument.
        if unsafe { libc::setsid() } >= 0 {
            return Ok(());
        }
        let e = std::io::Error::last_os_error();
        crate::fs::os_error_record(&e);
        Err(match e.raw_os_error() {
            Some(libc::EPERM) => proc_code::DENIED,
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

    // ---- s292: the pseudo-terminal and its settings --------------------

    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        /// macOS has `ptsname_r` since 10.13.4; the `libc` crate does not
        /// declare it there.
        fn ptsname_r(fd: libc::c_int, buf: *mut libc::c_char, len: libc::size_t) -> libc::c_int;
    }
    #[cfg(not(target_os = "macos"))]
    use libc::ptsname_r;

    /// The slave's name for the master `fd` (`ptsname_r`); `None` when
    /// `fd` is no master.
    fn name_of(fd: libc::c_int) -> Option<std::ffi::CString> {
        let mut buf = [0 as libc::c_char; 128];
        // SAFETY: a buffer of its stated length; the name is written
        // NUL-terminated within it on success.
        let r = unsafe { ptsname_r(fd, buf.as_mut_ptr(), buf.len()) };
        if r != 0 {
            return None;
        }
        // SAFETY: ptsname_r succeeded, so `buf` holds a NUL-terminated name.
        Some(unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_owned())
    }

    /// s292 (`[os.term.pty]`): a new pseudo-terminal's master and a slave,
    /// both close-on-exec and opened with `O_NOCTTY`, as table handles. A
    /// refusal is `IO` with the host's number in `os_error()`, and closes
    /// whatever was opened.
    pub fn open_pty() -> Result<(i64, i64), i64> {
        use std::os::fd::FromRawFd as _;
        crate::fs::os_error_clear();
        let fail = || {
            crate::fs::os_error_record(&std::io::Error::last_os_error());
            proc_code::IO
        };
        let _one = super::PTY_OPEN.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: posix_openpt answers a descriptor we own or -1.
        let m = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC) };
        if m < 0 {
            return Err(fail());
        }
        // SAFETY: `m` is ours; the File closes it on every early return.
        let master = unsafe { std::fs::File::from_raw_fd(m) };
        // SAFETY: the master we just opened.
        if unsafe { libc::grantpt(m) } != 0 || unsafe { libc::unlockpt(m) } != 0 {
            return Err(fail());
        }
        let Some(name) = name_of(m) else {
            return Err(fail());
        };
        // SAFETY: a NUL-terminated path; the answer is a descriptor we own
        // or -1.
        let s = unsafe {
            libc::open(
                name.as_ptr(),
                libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
            )
        };
        if s < 0 {
            return Err(fail());
        }
        // SAFETY: `s` is ours.
        let slave = unsafe { std::fs::File::from_raw_fd(s) };
        Ok((crate::fs::mint_file(master), crate::fs::mint_file(slave)))
    }

    /// s292 (`[os.term.pty]`): the slave's name for the master `h` names;
    /// `IO` for a handle that is closed, forged or no master.
    pub fn pty_name(h: i64) -> Result<String, i64> {
        let f = fd_of(h)?;
        let name = name_of(f.as_raw_fd()).ok_or(proc_code::IO)?;
        name.into_string().map_err(|_| proc_code::IO)
    }

    /// s292 (`[os.term.size.set]`): the window's size, all four fields of
    /// the host's `winsize` (`TIOCSWINSZ`). The shape first: a field
    /// outside 0..65535 is `INVALID` before the handle is read.
    pub fn set_size(h: i64, rows: i64, cols: i64, xpix: i64, ypix: i64) -> Result<(), i64> {
        if !super::attr::size_fits([rows, cols, xpix, ypix]) {
            return Err(proc_code::INVALID);
        }
        let f = fd_of(h)?;
        let w = libc::winsize {
            ws_row: rows as u16,
            ws_col: cols as u16,
            ws_xpixel: xpix as u16,
            ws_ypixel: ypix as u16,
        };
        // SAFETY: a winsize for the ioctl on a descriptor we hold.
        if unsafe { libc::ioctl(f.as_raw_fd(), libc::TIOCSWINSZ, &w) } < 0 {
            Err(proc_code::IO)
        } else {
            Ok(())
        }
    }

    /// s292 (`[os.term.attr]`): where an op lives in this host's `termios`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Field {
        /// A special character, `c_cc[i]`.
        Cc(usize),
        Iflag(libc::tcflag_t),
        Oflag(libc::tcflag_t),
        Lflag(libc::tcflag_t),
        Cflag(libc::tcflag_t),
        /// The character size: this `CSIZE` value (`CS7`, `CS8`).
        Csize(libc::tcflag_t),
        Ispeed,
        Ospeed,
    }

    /// The place of `op` in this host's `termios`; `None` for an op the
    /// host lacks (linux and PAX: 11, 15, 17; macOS: 15, 16, 37, 52, 71)
    /// or one outside the vocabulary.
    pub fn field(op: i64) -> Option<Field> {
        use Field::{Cc, Cflag, Csize, Iflag, Ispeed, Lflag, Oflag, Ospeed};
        Some(match op {
            1 => Cc(libc::VINTR),
            2 => Cc(libc::VQUIT),
            3 => Cc(libc::VERASE),
            4 => Cc(libc::VKILL),
            5 => Cc(libc::VEOF),
            6 => Cc(libc::VEOL),
            7 => Cc(libc::VEOL2),
            8 => Cc(libc::VSTART),
            9 => Cc(libc::VSTOP),
            10 => Cc(libc::VSUSP),
            #[cfg(target_os = "macos")]
            11 => Cc(libc::VDSUSP),
            12 => Cc(libc::VREPRINT),
            13 => Cc(libc::VWERASE),
            14 => Cc(libc::VLNEXT),
            #[cfg(any(target_os = "linux", target_os = "android"))]
            16 => Cc(libc::VSWTC),
            #[cfg(target_os = "macos")]
            17 => Cc(libc::VSTATUS),
            18 => Cc(libc::VDISCARD),
            30 => Iflag(libc::IGNPAR),
            31 => Iflag(libc::PARMRK),
            32 => Iflag(libc::INPCK),
            33 => Iflag(libc::ISTRIP),
            34 => Iflag(libc::INLCR),
            35 => Iflag(libc::IGNCR),
            36 => Iflag(libc::ICRNL),
            #[cfg(any(target_os = "linux", target_os = "android"))]
            37 => Iflag(libc::IUCLC),
            38 => Iflag(libc::IXON),
            39 => Iflag(libc::IXANY),
            40 => Iflag(libc::IXOFF),
            41 => Iflag(libc::IMAXBEL),
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
            42 => Iflag(libc::IUTF8),
            50 => Lflag(libc::ISIG),
            51 => Lflag(libc::ICANON),
            #[cfg(any(target_os = "linux", target_os = "android"))]
            52 => Lflag(libc::XCASE),
            53 => Lflag(libc::ECHO),
            54 => Lflag(libc::ECHOE),
            55 => Lflag(libc::ECHOK),
            56 => Lflag(libc::ECHONL),
            57 => Lflag(libc::NOFLSH),
            58 => Lflag(libc::TOSTOP),
            59 => Lflag(libc::IEXTEN),
            60 => Lflag(libc::ECHOCTL),
            61 => Lflag(libc::ECHOKE),
            62 => Lflag(libc::PENDIN),
            70 => Oflag(libc::OPOST),
            #[cfg(any(target_os = "linux", target_os = "android"))]
            71 => Oflag(libc::OLCUC),
            72 => Oflag(libc::ONLCR),
            73 => Oflag(libc::OCRNL),
            74 => Oflag(libc::ONOCR),
            75 => Oflag(libc::ONLRET),
            90 => Csize(libc::CS7),
            91 => Csize(libc::CS8),
            92 => Cflag(libc::PARENB),
            93 => Cflag(libc::PARODD),
            128 => Ispeed,
            129 => Ospeed,
            _ => return None,
        })
    }

    /// The host's speed for each of [`super::attr::SPEEDS`], index for
    /// index: the `libc` crate's constants, the kernel's codes on linux
    /// (what `c_cflag` holds) and the rates on macOS.
    pub const SPEED_CODES: [libc::speed_t; 19] = [
        libc::B0,
        libc::B50,
        libc::B75,
        libc::B110,
        libc::B134,
        libc::B150,
        libc::B200,
        libc::B300,
        libc::B600,
        libc::B1200,
        libc::B1800,
        libc::B2400,
        libc::B4800,
        libc::B9600,
        libc::B19200,
        libc::B38400,
        libc::B57600,
        libc::B115200,
        libc::B230400,
    ];

    /// A speed's code in `t`. On linux the kernel keeps it in `c_cflag`
    /// (`CBAUD` with `CBAUDEX`), one speed for input and output, as musl
    /// and glibc before 2.42 keep it; the runtime reads and writes those bits
    /// itself, so it names no C library's speed calls there (glibc 2.42
    /// versions them, and a static link against `libc.a` resolves no
    /// versioned name).
    #[cfg(target_os = "linux")]
    fn speed_of(t: &libc::termios, _input: bool) -> libc::speed_t {
        t.c_cflag & (libc::CBAUD | libc::CBAUDEX)
    }
    /// macOS: `speed_t` is the rate itself, and its calls are unversioned.
    #[cfg(not(target_os = "linux"))]
    fn speed_of(t: &libc::termios, input: bool) -> libc::speed_t {
        // SAFETY: a termios tcgetattr filled.
        unsafe {
            if input {
                libc::cfgetispeed(t)
            } else {
                libc::cfgetospeed(t)
            }
        }
    }

    /// `code` written as the input or output speed, every other field as
    /// found. On linux an input speed of 0 means "the output's" and sets
    /// nothing (as musl's `cfsetispeed`); its read-back then names the
    /// output's speed.
    #[cfg(target_os = "linux")]
    fn put_speed(t: &mut libc::termios, input: bool, code: libc::speed_t) {
        if input && code == libc::B0 {
            return;
        }
        t.c_cflag = (t.c_cflag & !(libc::CBAUD | libc::CBAUDEX)) | code;
    }
    #[cfg(not(target_os = "linux"))]
    fn put_speed(t: &mut libc::termios, input: bool, code: libc::speed_t) {
        // SAFETY: a termios tcgetattr filled and a code from the table.
        unsafe {
            if input {
                libc::cfsetispeed(t, code);
            } else {
                libc::cfsetospeed(t, code);
            }
        }
    }

    fn speed_code(rate: i64) -> Option<libc::speed_t> {
        let i = super::attr::SPEEDS.iter().position(|&r| r == rate)?;
        Some(SPEED_CODES[i])
    }

    fn speed_rate(code: libc::speed_t) -> Option<i64> {
        let i = SPEED_CODES.iter().position(|&c| c == code)?;
        Some(super::attr::SPEEDS[i])
    }

    /// The op's value in `t`; a speed the table does not name is
    /// `UNSUPPORTED`.
    fn read_field(t: &libc::termios, f: Field) -> Result<i64, i64> {
        let on = |flags: libc::tcflag_t, m: libc::tcflag_t| i64::from(flags & m != 0);
        Ok(match f {
            Field::Cc(i) => {
                let c = t.c_cc[i];
                if c == DISABLED { -1 } else { i64::from(c) }
            }
            Field::Iflag(m) => on(t.c_iflag, m),
            Field::Oflag(m) => on(t.c_oflag, m),
            Field::Lflag(m) => on(t.c_lflag, m),
            Field::Cflag(m) => on(t.c_cflag, m),
            Field::Csize(cs) => i64::from(t.c_cflag & libc::CSIZE == cs),
            Field::Ispeed => speed_rate(speed_of(t, true)).ok_or(proc_code::UNSUPPORTED)?,
            Field::Ospeed => speed_rate(speed_of(t, false)).ok_or(proc_code::UNSUPPORTED)?,
        })
    }

    /// `v` written into the op's place in `t`, every other field as found.
    /// A speed's `v` is in the table (the caller checked).
    fn write_field(t: &mut libc::termios, f: Field, v: i64) {
        let put = |flags: &mut libc::tcflag_t, m: libc::tcflag_t| {
            if v != 0 {
                *flags |= m;
            } else {
                *flags &= !m;
            }
        };
        match f {
            Field::Cc(i) => t.c_cc[i] = if v < 0 { DISABLED } else { v as libc::cc_t },
            Field::Iflag(m) => put(&mut t.c_iflag, m),
            Field::Oflag(m) => put(&mut t.c_oflag, m),
            Field::Lflag(m) => put(&mut t.c_lflag, m),
            Field::Cflag(m) => put(&mut t.c_cflag, m),
            Field::Csize(cs) => {
                if v == 1 {
                    t.c_cflag = (t.c_cflag & !libc::CSIZE) | cs;
                }
            }
            Field::Ispeed | Field::Ospeed => {
                if let Some(code) = speed_code(v) {
                    put_speed(t, f == Field::Ispeed, code);
                }
            }
        }
    }

    /// What a set of `v` reads back as when the host keeps it.
    fn kept_as(f: Field, v: i64) -> i64 {
        match f {
            Field::Cc(_) if v < 0 || v as libc::cc_t == DISABLED => -1,
            Field::Csize(_) => 1,
            _ => v,
        }
    }

    /// The order of refusal both calls share before the handle: the shape
    /// (`INVALID`), then the host (`UNSUPPORTED`: an op it lacks).
    fn op_field(op: i64) -> Result<Field, i64> {
        super::attr::kind(op).ok_or(proc_code::INVALID)?;
        field(op).ok_or(proc_code::UNSUPPORTED)
    }

    /// s292 (`[os.term.attr]`): one setting of the terminal `h` names.
    pub fn attr(h: i64, op: i64) -> Result<i64, i64> {
        let place = op_field(op)?;
        let f = fd_of(h)?;
        let t = get(f.as_raw_fd())?;
        read_field(&t, place)
    }

    /// s292 (`[os.term.attr]`): set one setting, every other field as
    /// found (`TCSADRAIN`, `SIGTTOU` blocked), then read it back: a set
    /// the host did not keep is `UNSUPPORTED`, with what was found put
    /// back.
    pub fn set_attr(h: i64, op: i64, v: i64) -> Result<(), i64> {
        let Some(kind) = super::attr::kind(op) else {
            return Err(proc_code::INVALID);
        };
        if !super::attr::fits(kind, v) {
            return Err(proc_code::INVALID);
        }
        let place = field(op).ok_or(proc_code::UNSUPPORTED)?;
        if kind == super::attr::Kind::Speed && speed_code(v).is_none() {
            return Err(proc_code::UNSUPPORTED);
        }
        let f = fd_of(h)?;
        let fd = f.as_raw_fd();
        let found = get(fd)?;
        if kind == super::attr::Kind::Size && v == 0 {
            // A size not selected leaves the size as found.
            return Ok(());
        }
        let mut t = found;
        write_field(&mut t, place, v);
        let set = |t: &libc::termios| {
            // SAFETY: a descriptor we hold and a termios tcgetattr filled.
            crate::os::with_ttou_blocked(|| unsafe { libc::tcsetattr(fd, libc::TCSADRAIN, t) })
        };
        // The setting is read back whatever `tcsetattr` answered: C
        // libraries answer the same kernel differently (glibc 2.42 answers
        // 0 for CS7 on a pty, which keeps CS8; glibc 2.39 `EINVAL`).
        let rc = set(&t);
        let back = get(fd)?;
        if read_field(&back, place) != Ok(kept_as(place, v)) {
            let _ = set(&found);
            return Err(proc_code::UNSUPPORTED);
        }
        if rc < 0 {
            return Err(proc_code::IO);
        }
        Ok(())
    }

    /// s292: the table, one line per op and per speed, for the row that
    /// holds the checked machine's transcription equal to it.
    pub fn attr_table() -> Vec<String> {
        let mut out = Vec::new();
        for op in super::attr::OPS {
            match field(op) {
                Some(f) => out.push(format!("{op} {f:?}")),
                None => out.push(format!("{op} lacks")),
            }
        }
        for (rate, code) in super::attr::SPEEDS.iter().zip(SPEED_CODES.iter()) {
            out.push(format!("speed {rate} {code}"));
        }
        out.push(format!("disabled {DISABLED}"));
        out
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
    /// s290: windows has no sessions of this kind.
    pub fn sid() -> Result<i64, i64> {
        Err(proc_code::UNSUPPORTED)
    }
    pub fn setsid() -> Result<(), i64> {
        crate::fs::os_error_clear();
        Err(proc_code::UNSUPPORTED)
    }
    pub fn size(_h: i64) -> Result<(i64, i64), i64> {
        Err(proc_code::UNSUPPORTED)
    }
    pub fn chars(_h: i64) -> Result<[i64; super::CHARS_COUNT], i64> {
        Err(proc_code::UNSUPPORTED)
    }
    /// s292: windows has no pseudo-terminal of this kind; every call is
    /// `unsupported`, after its shape.
    pub fn open_pty() -> Result<(i64, i64), i64> {
        crate::fs::os_error_clear();
        Err(proc_code::UNSUPPORTED)
    }
    pub fn pty_name(_h: i64) -> Result<String, i64> {
        Err(proc_code::UNSUPPORTED)
    }
    pub fn set_size(_h: i64, rows: i64, cols: i64, xpix: i64, ypix: i64) -> Result<(), i64> {
        if !super::attr::size_fits([rows, cols, xpix, ypix]) {
            return Err(proc_code::INVALID);
        }
        Err(proc_code::UNSUPPORTED)
    }
    pub fn attr(_h: i64, op: i64) -> Result<i64, i64> {
        super::attr::kind(op).ok_or(proc_code::INVALID)?;
        Err(proc_code::UNSUPPORTED)
    }
    pub fn set_attr(_h: i64, op: i64, v: i64) -> Result<(), i64> {
        let kind = super::attr::kind(op).ok_or(proc_code::INVALID)?;
        if !super::attr::fits(kind, v) {
            return Err(proc_code::INVALID);
        }
        Err(proc_code::UNSUPPORTED)
    }
}

#[cfg(unix)]
pub use sys::{Field, attr_table, field};
pub use sys::{
    attr, chars, foreground, mode_of, open_pty, pgid, pty_name, set_attr, set_foreground, set_mode,
    set_pgid, set_size, setsid, sid, size,
};

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

/// `os_sid() -> int ! {unsupported}` (s290, `[os.term.session]`) — this
/// process's session id (> 0) or `-code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_sid() -> i64 {
    sid().unwrap_or_else(|c| -c)
}

/// `os_setsid() -> () ! {denied, io, unsupported}` (s290,
/// `[os.term.session]`) — 0 ok, else the code.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_setsid() -> i64 {
    setsid().map_or_else(|c| c, |()| proc_code::OK)
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

/// `os_term_open_pty() -> (int, int) ! {io, unsupported}` (s292,
/// `[os.term.pty]`) — 0 and the (master, slave) handles through `out`, or
/// the code.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_term_open_pty(out: i64) -> i64 {
    match open_pty() {
        Ok((m, s)) => {
            unsafe { crate::str::write_pair(out, m, s) };
            proc_code::OK
        }
        Err(c) => c,
    }
}

/// `os_term_pty_name(fd) -> str ! {io, unsupported}` (s292) — 0 and the
/// name's (pointer, length) through `out`, the text in the current region,
/// or the code.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_term_pty_name(h: i64, out: i64) -> i64 {
    match pty_name(h) {
        Ok(name) => {
            let p = crate::str::region_copy(name.as_bytes());
            unsafe { crate::str::write_pair(out, p as i64, name.len() as i64) };
            proc_code::OK
        }
        Err(c) => c,
    }
}

/// `os_term_set_size(fd, rows, columns, xpixels, ypixels) -> () !
/// {invalid, io, unsupported}` (s292, `[os.term.size.set]`) — 0 or the
/// code.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_term_set_size(
    h: i64,
    rows: i64,
    cols: i64,
    xp: i64,
    yp: i64,
) -> i64 {
    set_size(h, rows, cols, xp, yp)
        .err()
        .unwrap_or(proc_code::OK)
}

/// `os_term_attr(fd, op) -> int ! {invalid, io, unsupported}` (s292,
/// `[os.term.attr]`) — 0 and the value through `out`, or the code: a
/// value may be -1 (a character disabled), so it never shares the
/// answer word with a code.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_term_attr(h: i64, op: i64, out: i64) -> i64 {
    match attr(h, op) {
        Ok(v) => {
            unsafe { crate::str::write_word(out, v) };
            proc_code::OK
        }
        Err(c) => c,
    }
}

/// `os_term_set_attr(fd, op, value) -> () ! {invalid, io, unsupported}`
/// (s292, `[os.term.attr]`) — 0 or the code.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_term_set_attr(h: i64, op: i64, v: i64) -> i64 {
    set_attr(h, op, v).err().unwrap_or(proc_code::OK)
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

    /// A mode with a bit outside the nine flags, VMIN and VTIME is
    /// `invalid` before the handle is read. s248 (#802): bits 5..7 are
    /// fields now, so a forged handle is `io` for them; s272 (#976): so is
    /// bit 24 (`RAW_LF`), and bit 25 is the first bit that is none.
    #[test]
    fn a_mode_outside_the_bits_is_invalid() {
        assert_eq!(set_mode(1_000_000, 32), Err(proc_code::IO));
        assert_eq!(set_mode(1_000_000, 64 | 128), Err(proc_code::IO));
        assert_eq!(set_mode(1_000_000, 1 << 24), Err(proc_code::IO));
        assert_eq!(restore_at_exit(1_000_000, 1 << 24, &[]), Err(proc_code::IO));
        assert_eq!(set_mode(1_000_000, 1 << 25), Err(proc_code::INVALID));
        assert_eq!(
            restore_at_exit(1_000_000, 1 << 25, &[]),
            Err(proc_code::INVALID)
        );
        assert_eq!(set_mode(1_000_000, -1), Err(proc_code::INVALID));
    }

    /// s272 (#976): `RAW_LF` reads the slave's ONLCR as found (on, then
    /// off), and a set with it clear turns the mapping on with OPOST; a set
    /// of the mode read back puts it back exactly.
    #[test]
    fn raw_lf_reads_and_sets_onlcr() {
        let (_master, slave) = pty();
        let fd = crate::fs::dup_of(slave).expect("the slave");
        let onlcr = |on: bool| {
            use std::os::fd::AsRawFd as _;
            // SAFETY: tcgetattr/tcsetattr on the slave this test opened.
            unsafe {
                let mut t: libc::termios = std::mem::zeroed();
                assert_eq!(libc::tcgetattr(fd.as_raw_fd(), &mut t), 0);
                if on {
                    t.c_oflag |= libc::ONLCR;
                } else {
                    t.c_oflag &= !libc::ONLCR;
                }
                assert_eq!(libc::tcsetattr(fd.as_raw_fd(), libc::TCSANOW, &t), 0);
            }
        };
        let has = || {
            use std::os::fd::AsRawFd as _;
            // SAFETY: tcgetattr on the slave this test opened.
            unsafe {
                let mut t: libc::termios = std::mem::zeroed();
                assert_eq!(libc::tcgetattr(fd.as_raw_fd(), &mut t), 0);
                t.c_oflag & libc::ONLCR != 0
            }
        };
        onlcr(true);
        let on = mode_of(slave).expect("mode");
        assert_eq!(on & mode::RAW_LF, 0, "ONLCR on reads RAW_LF clear: {on:#x}");
        onlcr(false);
        let off = mode_of(slave).expect("mode");
        assert_eq!(
            off & mode::RAW_LF,
            mode::RAW_LF,
            "ONLCR off reads RAW_LF: {off:#x}"
        );
        assert_eq!(off & !mode::RAW_LF, on, "nothing else moved");
        set_mode(slave, off & !mode::RAW_LF & !mode::RAW_OUTPUT).expect("set");
        assert!(has(), "the mapping is on after a set with RAW_LF clear");
        set_mode(slave, off).expect("set back");
        assert!(!has(), "the mode read is put back exactly");
        assert_eq!(mode_of(slave), Ok(off));
    }

    /// The group is this process's own (a test runner is in some group).
    #[test]
    fn pgid_is_the_process_group() {
        // SAFETY: getpgrp cannot fail.
        assert_eq!(pgid(), Ok(i64::from(unsafe { libc::getpgrp() })));
    }

    /// The term tests open their pairs at once, and a pair is opened
    /// whole under this lock. POSIX does not require `ptsname` to be
    /// thread-safe (its answer may live in storage the next call
    /// overwrites: on linux one test opened another's slave), and
    /// macOS's `posix_openpt` answered -1 with errno -6 when eight
    /// threads opened at once, never under one lock (the probe and its
    /// runs: `notes/s272-evidence/nomad/ptyprobe/`).
    ///
    /// s292: the lock is the runtime's own ([`super::PTY_OPEN`]), which
    /// [`open_pty`] takes too.
    fn pairs() -> &'static std::sync::Mutex<()> {
        &super::PTY_OPEN
    }

    /// A fresh pseudo-terminal: the master's descriptor and the slave
    /// as a table handle (POSIX `posix_openpt`, `grantpt`, `unlockpt`,
    /// `ptsname`).
    fn pty() -> (std::fs::File, i64) {
        use std::os::fd::FromRawFd as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        let _held = pairs().lock().unwrap_or_else(|p| p.into_inner());
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

    /// s272: each pair the helper opens is its own: a size set on the
    /// master reads back on the slave while eight threads open pairs at
    /// once, as the term tests do. hasu's head4 run at `7ca4f949` failed
    /// four of these tests at once (a size of 0x0, a mode word with
    /// another test's bits) because one test's slave was another's; on
    /// macOS this row failed at `posix_openpt` itself (see `PAIRS`).
    #[test]
    fn the_pty_helper_answers_its_own_pair() {
        use std::os::fd::AsRawFd as _;
        let workers: Vec<_> = (0..8u16)
            .map(|t| {
                std::thread::spawn(move || {
                    let mut crossed = 0u32;
                    for i in 0..100u16 {
                        let (m, s) = pty();
                        let rows = t * 100 + i + 1;
                        // SAFETY: a winsize for this pair's master.
                        unsafe {
                            let w = libc::winsize {
                                ws_row: rows,
                                ws_col: 7,
                                ws_xpixel: 0,
                                ws_ypixel: 0,
                            };
                            assert_eq!(libc::ioctl(m.as_raw_fd(), libc::TIOCSWINSZ, &w), 0);
                        }
                        if size(s) != Ok((i64::from(rows), 7)) {
                            crossed += 1;
                        }
                        drop(m);
                        assert_eq!(crate::fs::__wolf_rt_fs_close(s), crate::fs::fs_code::OK);
                    }
                    crossed
                })
            })
            .collect();
        let crossed: u32 = workers.into_iter().map(|w| w.join().unwrap_or(1)).sum();
        assert_eq!(crossed, 0, "pairs whose slave was another pair's");
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
            set_mode(s, 1 << 25),
            Err(proc_code::INVALID),
            "bit 25 is not a field (s272: bit 24 is RAW_LF)"
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
        assert_eq!(restore_at_exit(s, 1 << 25, b"x"), Err(proc_code::INVALID));
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

    /// s292 (`[os.term.pty]`): the pair are two table handles, both
    /// terminals and close-on-exec; the master's name has the host's form
    /// and opens a terminal; the slave and a file have no name (`io`).
    #[test]
    fn open_pty_answers_a_named_pair() {
        use std::os::fd::AsRawFd as _;
        let (m, s) = open_pty().expect("a pty");
        assert!(m >= 3 && s >= 3 && m != s, "{m} {s}");
        for h in [m, s] {
            assert_eq!(crate::fs::is_terminal(h), Some(true), "{h} a terminal");
            let f = crate::fs::dup_of(h).expect("held");
            // The duplicate is close-on-exec by construction; the table's
            // own descriptor is checked through F_GETFD on it.
            let raw = crate::fs::with_handle(h, |f| f.as_raw_fd()).expect("live");
            // SAFETY: F_GETFD on a descriptor the table holds.
            let fl = unsafe { libc::fcntl(raw, libc::F_GETFD) };
            assert_ne!(fl & libc::FD_CLOEXEC, 0, "{h} close-on-exec");
            drop(f);
        }
        let name = pty_name(m).expect("the name");
        let prefix = if cfg!(target_os = "macos") {
            "/dev/ttys"
        } else {
            "/dev/pts/"
        };
        assert!(name.starts_with(prefix), "{name}");
        assert!(std::fs::metadata(&name).is_ok(), "{name} exists");
        assert_eq!(pty_name(s), Err(proc_code::IO), "a slave has no name");
        assert_eq!(pty_name(1_000_000), Err(proc_code::IO));
        assert_eq!(crate::fs::__wolf_rt_fs_close(s), crate::fs::fs_code::OK);
        assert_eq!(crate::fs::__wolf_rt_fs_close(m), crate::fs::fs_code::OK);
    }

    /// s292 (`[os.term.size.set]`): a size set on either end reads on the
    /// other; a field outside 0..65535 is `invalid` before the handle; a
    /// forged handle `io`.
    #[test]
    fn set_size_on_either_end() {
        let (m, s) = open_pty().expect("a pty");
        assert_eq!(set_size(m, 40, 132, 0, 0), Ok(()));
        assert_eq!(size(s), Ok((40, 132)));
        assert_eq!(set_size(s, 50, 100, 640, 480), Ok(()));
        assert_eq!(size(m), Ok((50, 100)));
        assert_eq!(set_size(1_000_000, -1, 80, 0, 0), Err(proc_code::INVALID));
        assert_eq!(
            set_size(1_000_000, 24, 65536, 0, 0),
            Err(proc_code::INVALID)
        );
        assert_eq!(
            set_size(1_000_000, 24, 80, 0, 70_000),
            Err(proc_code::INVALID)
        );
        assert_eq!(set_size(1_000_000, 24, 80, 0, 0), Err(proc_code::IO));
        let _ = crate::fs::__wolf_rt_fs_close(s);
        let _ = crate::fs::__wolf_rt_fs_close(m);
    }

    /// `[os.term.attr]`'s table for this host: each op on a pty's slave
    /// round-trips, is lacking, or is not kept.
    fn op_row(op: i64) -> &'static str {
        let (lacks, not_kept): (&[i64], &[i64]) = if cfg!(target_os = "macos") {
            (&[15, 16, 37, 52, 71], &[62])
        } else {
            (&[11, 15, 17], &[90, 92])
        };
        if lacks.contains(&op) {
            "lacks"
        } else if not_kept.contains(&op) {
            "not kept"
        } else {
            "kept"
        }
    }

    /// s292 (`[os.term.attr]`): every op of the vocabulary, set on the
    /// slave to a value it does not have and read back on the master, as
    /// the host's table says; a set the host does not keep is
    /// `unsupported` with every field as it was.
    #[test]
    fn every_op_round_trips_or_is_refused_by_name() {
        use std::os::fd::AsRawFd as _;
        let (m, s) = open_pty().expect("a pty");
        let fm = crate::fs::dup_of(m).expect("the master");
        let image = || {
            // SAFETY: tcgetattr on the master this test opened.
            unsafe {
                let mut t: libc::termios = std::mem::zeroed();
                assert_eq!(libc::tcgetattr(fm.as_raw_fd(), &mut t), 0);
                (t.c_iflag, t.c_oflag, t.c_cflag, t.c_lflag, t.c_cc)
            }
        };
        for op in attr::OPS {
            let row = op_row(op);
            let v = match attr(s, op) {
                Err(c) => {
                    assert_eq!((c, row), (proc_code::UNSUPPORTED, "lacks"), "op {op}");
                    assert_eq!(set_attr(s, op, 1), Err(proc_code::UNSUPPORTED), "op {op}");
                    continue;
                }
                Ok(v) => v,
            };
            assert_ne!(row, "lacks", "op {op} read {v}");
            let want = match attr::kind(op).expect("in the vocabulary") {
                attr::Kind::Char => {
                    if v == 7 {
                        8
                    } else {
                        7
                    }
                }
                attr::Kind::Size => 1,
                attr::Kind::Speed => {
                    if v == 9600 {
                        19200
                    } else {
                        9600
                    }
                }
                attr::Kind::Flag => 1 - v,
            };
            let before = image();
            let r = set_attr(s, op, want);
            if row == "not kept" {
                assert_eq!(r, Err(proc_code::UNSUPPORTED), "op {op}");
                assert_eq!(attr(m, op), Ok(v), "op {op}: as it was");
                assert_eq!(image(), before, "op {op}: every field as it was");
            } else {
                assert_eq!(r, Ok(()), "op {op} {v} -> {want}");
                assert_eq!(attr(m, op), Ok(want), "op {op}: read on the other end");
                assert_eq!(set_attr(s, op, v), Ok(()), "op {op} set back");
            }
        }
        let _ = crate::fs::__wolf_rt_fs_close(s);
        let _ = crate::fs::__wolf_rt_fs_close(m);
    }

    /// s292 (`[os.term.attr]`): the order of refusal — the shape, then the
    /// host (an op it lacks, a speed outside the table), then the handle.
    #[test]
    fn the_order_of_refusal() {
        let forged = 1_000_000;
        for op in [0, 19, 29, 43, 49, 63, 69, 76, 89, 94, 127, 130, 160, -1] {
            assert_eq!(attr(forged, op), Err(proc_code::INVALID), "op {op}");
            assert_eq!(set_attr(forged, op, 1), Err(proc_code::INVALID), "op {op}");
        }
        for (op, v) in [
            (1, 256),
            (1, -2),
            (53, 2),
            (53, -1),
            (90, 2),
            (129, -1),
            (129, 1 << 32),
        ] {
            assert_eq!(
                set_attr(forged, op, v),
                Err(proc_code::INVALID),
                "op {op} = {v}"
            );
        }
        let lacking: &[i64] = if cfg!(target_os = "macos") {
            &[15, 16, 37, 52, 71]
        } else {
            &[11, 15, 17]
        };
        for &op in lacking {
            assert_eq!(attr(forged, op), Err(proc_code::UNSUPPORTED), "op {op}");
            assert_eq!(
                set_attr(forged, op, 1),
                Err(proc_code::UNSUPPORTED),
                "op {op}"
            );
        }
        assert_eq!(set_attr(forged, 129, 12345), Err(proc_code::UNSUPPORTED));
        assert_eq!(set_attr(forged, 129, 9600), Err(proc_code::IO));
        assert_eq!(attr(forged, 1), Err(proc_code::IO));
        assert_eq!(set_attr(forged, 53, 1), Err(proc_code::IO));
        // A file is no terminal: `io` after the shape and the host.
        let p = std::env::temp_dir().join(format!("wolf-rt-attr-{}", std::process::id()));
        std::fs::write(&p, b"x").unwrap();
        let file = crate::fs::mint_file(std::fs::File::open(&p).unwrap());
        assert_eq!(attr(file, 1), Err(proc_code::IO));
        assert_eq!(set_attr(file, 90, 0), Err(proc_code::IO));
        assert_eq!(set_attr(file, 1, 300), Err(proc_code::INVALID));
        let _ = crate::fs::__wolf_rt_fs_close(file);
        let _ = std::fs::remove_file(&p);
    }

    /// s292: a disabled character reads -1; a set of -1, or of the host's
    /// disabled byte, disables it.
    #[test]
    fn a_disabled_character_is_minus_one() {
        let (m, s) = open_pty().expect("a pty");
        assert_eq!(set_attr(s, 14, -1), Ok(()));
        assert_eq!(attr(s, 14), Ok(-1));
        assert_eq!(chars(s).expect("chars")[7], -1, "literal next disabled");
        assert_eq!(set_attr(s, 14, 22), Ok(()));
        assert_eq!(attr(s, 14), Ok(22));
        assert_eq!(set_attr(s, 14, i64::from(sys_disabled())), Ok(()));
        assert_eq!(attr(s, 14), Ok(-1));
        let _ = crate::fs::__wolf_rt_fs_close(s);
        let _ = crate::fs::__wolf_rt_fs_close(m);
    }

    /// s292: the table names every op once and every speed once.
    #[test]
    fn the_table_has_every_op_and_speed() {
        let t = attr_table();
        assert_eq!(t.len(), attr::OPS.len() + attr::SPEEDS.len() + 1);
        for op in attr::OPS {
            assert!(attr::kind(op).is_some(), "{op}");
        }
        let mut codes: Vec<_> = sys::SPEED_CODES.to_vec();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), attr::SPEEDS.len(), "one code per rate");
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
