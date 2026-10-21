//! s253 (wolf-lang#815, `[os.proc.adopt]`; ruling asked,
//! `[os.process.calls.note]` R4, the recommendation implemented): a
//! descriptor this process INHERITED, at 3 or above, made an fs handle —
//! `fs_adopt(fd) -> int ! {invalid, io, unsupported}`.
//!
//! The fs table's handles are its own numbers, never descriptors
//! (`[os.fs.std]`), so a program started with `5>out` could not write to
//! 5 (yew's witness: `fs_write_chunk(5, …)` is `io`). The adoption is
//! `net_adopt_listener`'s (`[os.proc.inherit]`) for any kind of
//! descriptor: the kernel is asked, the handle owns it from then on and
//! `fs_close` closes it, and it is marked close-on-exec, so this process's
//! children get it only through a descriptor map.
//!
//! **What may be adopted** is told by close-on-exec. A descriptor that
//! came across `exec` cannot carry the flag (the exec would have closed
//! it), and every descriptor the runtime makes carries it from birth — the
//! fs and net tables', the signal hub's and the readiness set's, an
//! adopted one's. So the rule is one question to the kernel: open
//! (`F_GETFD` answers) and not close-on-exec. Anything else is `io` —
//! not open, or a descriptor this process already holds (the runtime's
//! own, or one adopted before) — never a trap and never a second owner
//! waiting to close a number twice. 0, 1 and 2 are the standard streams,
//! already handles 0..2, and a number outside 3..=i32::MAX is not a
//! descriptor: `invalid`, before the host is asked. Windows answers
//! `unsupported` after that shape (a handle there is not a small stable
//! number).

use crate::fs::fs_code;

/// `fs_adopt`'s answer: the new handle (3 and up), or the negated code.
pub fn adopt(fd: i64) -> i64 {
    crate::fs::os_error_clear();
    if fd < 3 || fd > i64::from(i32::MAX) {
        return -fs_code::INVALID;
    }
    #[cfg(unix)]
    {
        use std::os::fd::FromRawFd as _;
        use std::sync::Mutex;
        // Two adoptions of one number ask and mark under one lock, so
        // only one of them can find the flag clear.
        static ADOPT: Mutex<()> = Mutex::new(());
        let _held = ADOPT.lock().unwrap_or_else(|p| p.into_inner());
        let raw = fd as std::os::fd::RawFd;
        // SAFETY: F_GETFD reads the descriptor's flags and nothing else.
        let flags = unsafe { libc::fcntl(raw, libc::F_GETFD) };
        if flags < 0 {
            crate::fs::os_error_record(&std::io::Error::last_os_error());
            return -fs_code::IO;
        }
        if flags & libc::FD_CLOEXEC != 0 {
            return -fs_code::IO;
        }
        // SAFETY: F_SETFD on a descriptor the kernel just said is open.
        if unsafe { libc::fcntl(raw, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
            crate::fs::os_error_record(&std::io::Error::last_os_error());
            return -fs_code::IO;
        }
        // SAFETY: an open descriptor no handle and no runtime part holds
        // (it was not close-on-exec until the line above, under the lock);
        // the table becomes its one owner.
        let f = unsafe { std::fs::File::from_raw_fd(raw) };
        crate::fs::mint_file(f)
    }
    #[cfg(not(unix))]
    {
        -fs_code::UNSUPPORTED
    }
}

/// `fs_adopt(fd) -> int ! {invalid, io, unsupported}` — the handle, or
/// the negated code (`fs_open`'s shape).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_fs_adopt(fd: i64) -> i64 {
    adopt(fd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shape_is_refused_first() {
        for fd in [i64::MIN, -1, 0, 1, 2, i64::from(i32::MAX) + 1] {
            assert_eq!(adopt(fd), -fs_code::INVALID, "{fd}");
        }
    }

    /// A descriptor made without close-on-exec (as one inherited across
    /// `exec` is) adopts once; the handle writes to it; a second adopt is
    /// `io`; a close-on-exec descriptor (one of this process's own) and a
    /// closed number are `io`.
    #[cfg(unix)]
    #[test]
    fn an_inherited_descriptor_adopts_once() {
        use std::io::{Read as _, Seek as _};
        use std::os::fd::AsRawFd as _;
        let dir = std::env::temp_dir().join(format!("wolf_rt_adopt_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out");
        let own = std::fs::File::create(&path).unwrap();
        // The runtime's own kind of descriptor: close-on-exec from birth.
        assert_eq!(adopt(i64::from(own.as_raw_fd())), -fs_code::IO);
        // An inherited kind: a duplicate without the flag.
        // SAFETY: dup of a live descriptor.
        let raw = unsafe { libc::dup(own.as_raw_fd()) };
        assert!(raw >= 3);
        let h = adopt(i64::from(raw));
        assert!(h >= 3, "adopted as a handle: {h}");
        // SAFETY: F_GETFD reads flags.
        let flags = unsafe { libc::fcntl(raw, libc::F_GETFD) };
        assert_ne!(flags & libc::FD_CLOEXEC, 0, "close-on-exec once adopted");
        assert_eq!(adopt(i64::from(raw)), -fs_code::IO, "a second adopt");
        crate::fs::with_handle(h, |f| {
            use std::io::Write as _;
            let mut f = f;
            f.write_all(b"R").unwrap();
        })
        .expect("the handle is live");
        let mut back = String::new();
        let mut r = std::fs::File::open(&path).unwrap();
        r.rewind().unwrap();
        r.read_to_string(&mut back).unwrap();
        assert_eq!(back, "R");
        drop(own);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
