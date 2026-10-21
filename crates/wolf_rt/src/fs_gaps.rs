//! s254 (wolf-lang#820, #818; yew yw02 G30, G32): what an editor's save
//! path, its journal and its clipboard met as absences —
//! `[os.fs.access]`, `[os.fs.lock]`, `[os.fs.fstat.record]`,
//! `[os.fs.mkfifo]`, `[os.fs.nonblock]`. (The open that does not follow a
//! final link is a flag of the open itself, [`crate::fs::fs_mode::NOFOLLOW`];
//! the wait for write readiness is `os_signal_wait_fds`'s, in
//! [`crate::signal`].)
//!
//! A module of its own, as `fs_save.rs` is: it shares [`crate::fs`]'s
//! handle table, error codes and host-error word and nothing else. No call
//! here waits for the host: the lock is tried once, the write is one
//! `write(2)`, and a call on a handle holds the table lock only for the
//! host's own call, as `fs_write_chunk` does.

use crate::fs::{code_of, fs_code, os_error_clear, os_error_record, os_error_set, with_handle};
use crate::str::write_word;

/// `access(2)`'s mode bits as the language spells them — POSIX's values,
/// the same on every unix host (`R_OK` 4, `W_OK` 2, `X_OK` 1, `F_OK` 0).
pub mod access_mode {
    pub const EXISTS: i64 = 0;
    pub const EXEC: i64 = 1;
    pub const WRITE: i64 = 2;
    pub const READ: i64 = 4;
}

/// The path as a C string, or `None` for one holding a NUL — no host call
/// can name it (the family answers `io`, as std's own path calls do).
#[cfg(unix)]
fn c_path(path: &str) -> Option<std::ffi::CString> {
    std::ffi::CString::new(path).ok()
}

/// `fs_access(path, mode) -> () ! {denied, invalid, io, not_found,
/// unsupported}` (`[os.fs.access]`) — `access(2)`: may this process, by its
/// REAL user and group ids (POSIX's rule; they are the effective ones
/// unless the program is set-id), do everything `mode` asks of what `path`
/// names, a final link followed? `mode` is 0 (the name exists) or any sum
/// of 4 read, 2 write, 1 execute; outside `0..=7` it is `invalid`, before
/// the host. `denied` is the host's refusal (`EACCES`, `EPERM`, `EROFS` — a
/// write on a read-only mount — and `ETXTBSY`), `not_found` a missing name
/// (a dangling link too), everything else `io` with the host's number in
/// `os_error()`. windows answers `unsupported` by name: its permission is
/// an access-control list `access(2)`'s bits do not describe.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_access(pp: i64, pl: i64, mode: i64) -> i64 {
    os_error_clear();
    if !(0..=7).contains(&mode) {
        return fs_code::INVALID;
    }
    #[cfg(unix)]
    {
        let path = unsafe { crate::str::view(pp, pl) };
        let Some(c) = c_path(path) else {
            return fs_code::IO;
        };
        let mut want = 0;
        if mode & access_mode::READ != 0 {
            want |= libc::R_OK;
        }
        if mode & access_mode::WRITE != 0 {
            want |= libc::W_OK;
        }
        if mode & access_mode::EXEC != 0 {
            want |= libc::X_OK;
        }
        if mode == access_mode::EXISTS {
            want = libc::F_OK;
        }
        // SAFETY: a NUL-terminated path this call owns; a plain query.
        // s254 PLANT: every ask granted, the host never asked.
        let _ = (c, want);
        if true {
            return fs_code::OK;
        }
        access_code(&std::io::Error::last_os_error())
    }
    #[cfg(not(unix))]
    {
        let _ = (pp, pl);
        fs_code::UNSUPPORTED
    }
}

/// `access(2)`'s refusals as the call's rows ([`__wolf_rt_fs_access`]).
#[cfg(unix)]
pub(crate) fn access_code(e: &std::io::Error) -> i64 {
    os_error_record(e);
    match e.raw_os_error() {
        Some(libc::EACCES | libc::EPERM | libc::EROFS | libc::ETXTBSY) => fs_code::DENIED,
        Some(libc::ENOENT) => fs_code::NOT_FOUND,
        _ => fs_code::IO,
    }
}

/// The host's number for "would have to wait": `EWOULDBLOCK` on unix
/// (`EAGAIN` on linux and macOS), `ERROR_LOCK_VIOLATION` (33) on windows.
fn would_block_number() -> i64 {
    #[cfg(unix)]
    {
        i64::from(libc::EWOULDBLOCK)
    }
    #[cfg(not(unix))]
    {
        33
    }
}

/// A lock call's host refusal: `ENOSYS` (a host with no such call — PAX,
/// whose kernel has no `flock` yet) and `EOPNOTSUPP` (a filesystem that
/// keeps no locks) are `unsupported`, by name; anything else `io`.
fn lock_code(e: &std::io::Error) -> i64 {
    os_error_record(e);
    #[cfg(unix)]
    if matches!(e.raw_os_error(), Some(libc::ENOSYS | libc::EOPNOTSUPP)) {
        return fs_code::UNSUPPORTED;
    }
    fs_code::IO
}

/// `fs_lock(fd, kind) -> () ! {busy, invalid, io, unsupported}`
/// (`[os.fs.lock]`) — an advisory lock on the file behind the handle: kind
/// 1 shared, 2 exclusive; anything else `invalid`, before the handle is
/// looked at. Tried ONCE, never waited for: a conflicting lock held through
/// another open of the file — by another process, or by this one through
/// another handle — is `busy`, the host's `EWOULDBLOCK` in `os_error()`.
/// The lock belongs to the OPEN FILE (`flock(2)` on unix, `LockFileEx` over
/// the whole file on windows: Rust's `File::try_lock`/`try_lock_shared`),
/// so the handle's duplicates — a child's copy through `os_spawn_fds`, a
/// forked child's — share it, and it lasts until [`__wolf_rt_fs_unlock`]
/// or until the last of them is closed. A closed or forged handle is `io`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_fs_lock(fd: i64, kind: i64) -> i64 {
    os_error_clear();
    if kind != 1 && kind != 2 {
        return fs_code::INVALID;
    }
    // s254 PLANT: no lock is taken.
    let Some(r) = with_handle(fd, |_f| Ok::<(), std::fs::TryLockError>(())) else {
        return fs_code::IO;
    };
    match r {
        Ok(()) => fs_code::OK,
        Err(std::fs::TryLockError::WouldBlock) => {
            os_error_set(would_block_number());
            fs_code::BUSY
        }
        Err(std::fs::TryLockError::Error(e)) => lock_code(&e),
    }
}

/// `fs_unlock(fd) -> () ! {io, unsupported}` (`[os.fs.lock]`) — the
/// handle's lock released; a handle that holds none answers ok on every
/// host (windows' "not locked", `ERROR_NOT_LOCKED`, is no refusal here).
/// A closed or forged handle is `io`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_fs_unlock(fd: i64) -> i64 {
    os_error_clear();
    let Some(r) = with_handle(fd, std::fs::File::unlock) else {
        return fs_code::IO;
    };
    match r {
        Ok(()) => fs_code::OK,
        Err(e) if cfg!(windows) && e.raw_os_error() == Some(158) => fs_code::OK,
        Err(e) => lock_code(&e),
    }
}

/// `fs_fstat_record(fd) -> List[int] ! {io}` (`[os.fs.fstat.record]`) —
/// `[os.fs.stat]`'s twenty-word record ([`crate::fs::stat_record`]) from
/// ONE metadata read on the handle (`fstat`; `statx` on linux): the
/// file's identity (`dev`, `ino`) without a path walked between the open
/// and the question. 0..2 are the standard streams (`[os.fs.std]`). A
/// handle never names a link (an open follows it, or refuses it under
/// `NOFOLLOW`), so `kind` is never 3. windows: the record's windows column,
/// and a handle that is no disk file is kind 2 (`fs_fstat`'s rule). Every
/// refusal — a closed or forged handle, a word outside `int` — is `io`.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_fstat_record(fd: i64, out: i64) -> i64 {
    use crate::fs::{stat_kind, stat_word};
    os_error_clear();
    // The table is released before the list is minted (`fs_fstat`'s order).
    let Some((md, disk)) = with_handle(fd, |f| (f.metadata(), crate::fs::is_disk(f))) else {
        return fs_code::IO;
    };
    let md = match md {
        Err(e) => {
            let _ = code_of(&e);
            return fs_code::IO;
        }
        Ok(m) => m,
    };
    let Some(mut rec) = crate::fs::stat_record(&md) else {
        return fs_code::IO;
    };
    if !disk && rec[stat_word::KIND] == stat_kind::FILE {
        rec[stat_word::KIND] = stat_kind::OTHER;
    }
    // s254 PLANT: the identity lost.
    rec[stat_word::DEV] = 0;
    rec[stat_word::INO] = 0;
    let hdr = crate::list::new_list(8);
    for w in rec {
        crate::list::push_int(hdr, w);
    }
    unsafe { write_word(out, hdr as i64) };
    fs_code::OK
}

/// `fs_mkfifo(path, perm) -> () ! {denied, exists, invalid, io, not_found,
/// unsupported}` (`[os.fs.mkfifo]`) — `mkfifo(3)`: a fifo named `path`,
/// its permission bits `perm` less the process's umask (`fs_open_perm`'s
/// rule); `perm` outside `0..=0o777` is `invalid`, before the host. A taken
/// name (a dangling link included) is `exists`, a missing parent
/// `not_found`, a refused write of the directory `denied`. windows has no
/// fifo in its file system and answers `unsupported` by name; so does a
/// file system or kernel that makes none ([`mkfifo_code`]).
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_mkfifo(pp: i64, pl: i64, perm: i64) -> i64 {
    os_error_clear();
    if !(0..=0o777).contains(&perm) {
        return fs_code::INVALID;
    }
    #[cfg(unix)]
    {
        let path = unsafe { crate::str::view(pp, pl) };
        let Some(c) = c_path(path) else {
            return fs_code::IO;
        };
        // s254 PLANT: a regular file, not a fifo.
        let _ = c;
        if std::fs::OpenOptions::new().write(true).create_new(true).open(path).is_ok() {
            return fs_code::OK;
        }
        mkfifo_code(&std::io::Error::last_os_error())
    }
    #[cfg(not(unix))]
    {
        let _ = (pp, pl);
        fs_code::UNSUPPORTED
    }
}

/// `mkfifo(3)`'s refusals as the call's rows: `EPERM` is "the file system
/// does not support the type of node requested" (`mknod(2)`: for a fifo it
/// asks no privilege, so nothing else can mean it), `ENOSYS` a kernel with
/// no such call and `EOPNOTSUPP` a file system that
/// makes none — each `unsupported`, by name (PAX refuses a fifo `EPERM`,
/// pax `kernel/user/user.lu`); the rest the family's mapping.
#[cfg(unix)]
pub(crate) fn mkfifo_code(e: &std::io::Error) -> i64 {
    if matches!(
        e.raw_os_error(),
        Some(libc::EPERM | libc::ENOSYS | libc::EOPNOTSUPP)
    ) {
        os_error_record(e);
        return fs_code::UNSUPPORTED;
    }
    code_of(e)
}

/// `fs_set_nonblock(fd, on: bool) -> () ! {invalid, io, unsupported}`
/// (`[os.fs.nonblock]`) — `O_NONBLOCK` set (`on`) or cleared on the open
/// file behind the handle (`fcntl(F_SETFL)`): afterwards a write the host
/// cannot take now answers at once instead of parking ([`__wolf_rt_fs_write_some`]
/// says `busy`; `fs_write_chunk` and the reads answer `io` with `EAGAIN` in
/// `os_error()`, their rows unchanged). The flag is the open file's, so the
/// handle's duplicates see it too; a pipe's two ends are two open files. 0,
/// 1 and 2 are `invalid`, by name: their open file is shared with whoever
/// started the process, and a flag set on it would outlive the program.
/// windows has no such flag on a handle and answers `unsupported`; a
/// closed or forged handle is `io`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_fs_set_nonblock(fd: i64, on: i64) -> i64 {
    os_error_clear();
    if crate::fs::is_std(fd) {
        return fs_code::INVALID;
    }
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd as _;
        let Some(r) = with_handle(fd, |f| {
            let raw = f.as_raw_fd();
            // SAFETY: the table's own live descriptor, under its lock.
            let fl = unsafe { libc::fcntl(raw, libc::F_GETFL) };
            if fl < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let want = if on != 0 {
                fl | libc::O_NONBLOCK
            } else {
                fl & !libc::O_NONBLOCK
            };
            // s254 PLANT: the flag never set.
            let _ = want;
            if false {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        }) else {
            return fs_code::IO;
        };
        match r {
            Ok(()) => fs_code::OK,
            Err(e) => {
                os_error_record(&e);
                fs_code::IO
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = on;
        fs_code::UNSUPPORTED
    }
}

/// One `write` of `bytes` to `f`, an interrupted one asked again.
fn write_once(mut f: &std::fs::File, bytes: &[u8]) -> std::io::Result<usize> {
    use std::io::Write as _;
    loop {
        match f.write(bytes) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            r => return r,
        }
    }
}

/// `fs_write_some(fd, bytes: List[byte]) -> int ! {busy, invalid, io}`
/// (`[os.fs.nonblock]`) — ONE write of `bytes` (all of them or a first
/// part), answering how many the host took: at least 1 for a non-empty
/// list, 0 for an empty one (the handle still checked). On a non-blocking
/// handle that can take nothing now it is `busy`, the host's `EAGAIN` in
/// `os_error()`; on a blocking one it waits as `fs_write_chunk` does, for
/// the first byte only. A write to a pipe whose reader has gone meets the
/// host's `SIGPIPE` first (`[os.fs.std]`); a program that ignores
/// BROKEN_PIPE sees `io` (`EPIPE`). 0..2 are the standard streams, 1 and 2
/// after std's own buffer is flushed (`fs_write_chunk`'s order). `invalid`
/// is a list of the wrong element width (an FFI caller's); a closed or
/// forged handle is `io`.
///
/// # Safety
///
/// `hdr` must be a live `List[byte]` header; `out` must address 8 writable
/// bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_write_some(fd: i64, hdr: i64, out: i64) -> i64 {
    os_error_clear();
    let Some(bytes) = (unsafe { crate::list::u8_elems(hdr) }) else {
        return fs_code::INVALID;
    };
    let r = if crate::fs::is_std(fd) {
        let Some(f) = crate::fs::std_stream(fd) else {
            return fs_code::IO;
        };
        if bytes.is_empty() {
            Ok(0)
        } else {
            match fd {
                1 => {
                    use std::io::Write as _;
                    let mut lock = std::io::stdout().lock();
                    let _ = lock.flush();
                    write_once(&f, bytes)
                }
                2 => {
                    let _lock = std::io::stderr().lock();
                    write_once(&f, bytes)
                }
                _ => write_once(&f, bytes),
            }
        }
    } else {
        let Some(r) = with_handle(fd, |f| {
            if bytes.is_empty() {
                Ok(0)
            } else {
                write_once(f, bytes)
            }
        }) else {
            return fs_code::IO;
        };
        r
    };
    match r {
        Ok(n) => {
            unsafe { write_word(out, n as i64) };
            fs_code::OK
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
            os_error_record(&e);
            fs_code::BUSY
        }
        Err(e) => {
            os_error_record(&e);
            fs_code::IO
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn pair(s: &str) -> (i64, i64) {
        (s.as_ptr() as i64, s.len() as i64)
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wolf-rt-gaps-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn list_of(bytes: &[u8]) -> i64 {
        crate::list::from_bytes(bytes) as i64
    }

    /// A file exists, reads and writes; a missing name is `not_found`; a
    /// mode past 7 is `invalid` before the host.
    #[test]
    fn access_answers_the_hosts_rows() {
        let d = scratch("access");
        std::fs::write(d.join("f"), b"x").unwrap();
        let f = d.join("f").display().to_string();
        let (p, l) = pair(&f);
        // SAFETY: str pairs this test owns.
        unsafe {
            assert_eq!(__wolf_rt_fs_access(p, l, 0), fs_code::OK);
            assert_eq!(__wolf_rt_fs_access(p, l, 6), fs_code::OK);
            assert_eq!(__wolf_rt_fs_access(p, l, 8), fs_code::INVALID);
            assert_eq!(__wolf_rt_fs_access(p, l, -1), fs_code::INVALID);
        }
        let missing = d.join("none").display().to_string();
        let (p, l) = pair(&missing);
        // SAFETY: as above.
        assert_eq!(unsafe { __wolf_rt_fs_access(p, l, 0) }, fs_code::NOT_FOUND);
        let under = format!("{f}/x");
        let (p, l) = pair(&under);
        // SAFETY: as above.
        assert_eq!(unsafe { __wolf_rt_fs_access(p, l, 0) }, fs_code::IO);
        assert_eq!(
            access_code(&std::io::Error::from_raw_os_error(libc::EROFS)),
            fs_code::DENIED,
            "a write on a read-only mount is a refusal"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Two handles of one file conflict; a shared lock admits another; a
    /// kind outside 1..2 is `invalid`; `ENOSYS` (PAX) is `unsupported`.
    #[test]
    fn the_lock_is_the_open_files() {
        let d = scratch("lock");
        let path = d.join("lk");
        std::fs::write(&path, b"j").unwrap();
        let a = crate::fs::mint_file(std::fs::File::open(&path).unwrap());
        let b = crate::fs::mint_file(std::fs::File::open(&path).unwrap());
        assert_eq!(__wolf_rt_fs_lock(a, 2), fs_code::OK);
        assert_eq!(__wolf_rt_fs_lock(b, 2), fs_code::BUSY);
        assert_eq!(
            crate::fs::__wolf_rt_os_error(),
            i64::from(libc::EWOULDBLOCK)
        );
        assert_eq!(__wolf_rt_fs_lock(b, 1), fs_code::BUSY);
        assert_eq!(__wolf_rt_fs_unlock(a), fs_code::OK);
        assert_eq!(__wolf_rt_fs_lock(b, 1), fs_code::OK);
        assert_eq!(__wolf_rt_fs_lock(a, 1), fs_code::OK);
        assert_eq!(__wolf_rt_fs_lock(a, 3), fs_code::INVALID);
        assert_eq!(__wolf_rt_fs_lock(1_000_000, 2), fs_code::IO);
        assert_eq!(__wolf_rt_fs_unlock(1_000_000), fs_code::IO);
        assert_eq!(
            lock_code(&std::io::Error::from_raw_os_error(libc::ENOSYS)),
            fs_code::UNSUPPORTED
        );
        let _ = crate::fs::__wolf_rt_fs_close(a);
        let _ = crate::fs::__wolf_rt_fs_close(b);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The handle's record is the path's: same identity, same length.
    #[test]
    fn the_handle_record_is_the_paths() {
        let d = scratch("record");
        let path = d.join("r");
        std::fs::write(&path, b"twelve bytes").unwrap();
        let h = crate::fs::mint_file(std::fs::File::open(&path).unwrap());
        let mut out = [0i64; 1];
        // SAFETY: an 8-byte slot this test owns.
        assert_eq!(
            unsafe { __wolf_rt_fs_fstat_record(h, out.as_mut_ptr() as i64) },
            fs_code::OK
        );
        // SAFETY: the shim minted a List[int] header.
        let words = unsafe { crate::list::i64_elems(out[0]) }.unwrap().to_vec();
        use std::os::unix::fs::MetadataExt as _;
        let md = std::fs::metadata(&path).unwrap();
        assert_eq!(words.len(), 20);
        assert_eq!(words[1], 12);
        assert_eq!(words[9], md.dev() as i64);
        assert_eq!(words[10], md.ino() as i64);
        let _ = crate::fs::__wolf_rt_fs_close(h);
        // SAFETY: as above.
        assert_eq!(
            unsafe { __wolf_rt_fs_fstat_record(h, out.as_mut_ptr() as i64) },
            fs_code::IO
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// `[os.fs.open.nofollow]`: bit 16 opens a file, refuses a final link
    /// `invalid` with `ELOOP` in `os_error()`, and leaves a link in a middle
    /// component followed; any other bit above the mode is `invalid`.
    #[test]
    fn the_nofollow_bit_refuses_a_final_link() {
        use crate::fs::{__wolf_rt_fs_close, __wolf_rt_fs_open, fs_mode};
        let d = scratch("nofollow");
        std::fs::write(d.join("t"), b"t").unwrap();
        std::os::unix::fs::symlink("t", d.join("l")).unwrap();
        std::os::unix::fs::symlink(".", d.join("dl")).unwrap();
        let open = |rel: &str, mode: i64| {
            let p = d.join(rel).display().to_string();
            let (pp, pl) = pair(&p);
            // SAFETY: a str pair this test owns.
            unsafe { __wolf_rt_fs_open(pp, pl, mode) }
        };
        let fd = open("t", fs_mode::NOFOLLOW);
        assert!(fd >= 3, "a file opens under the bit: {fd}");
        assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
        assert_eq!(open("l", fs_mode::NOFOLLOW), -fs_code::INVALID);
        assert_eq!(crate::fs::__wolf_rt_os_error(), i64::from(libc::ELOOP));
        assert_eq!(
            open("l", fs_mode::READ_WRITE | fs_mode::NOFOLLOW),
            -fs_code::INVALID
        );
        let fd = open("dl/t", fs_mode::READ_NONBLOCK | fs_mode::NOFOLLOW);
        assert!(fd >= 3, "a middle link is followed: {fd}");
        assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
        assert_eq!(
            open("l", fs_mode::CREATE_NEW | fs_mode::NOFOLLOW),
            -fs_code::EXISTS
        );
        assert_eq!(open("t", 32), -fs_code::INVALID);
        assert_eq!(open("t", -16), -fs_code::INVALID);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A fifo is made with its bits; made again `exists`; bits past 0777
    /// `invalid`.
    #[test]
    fn mkfifo_makes_a_fifo() {
        let d = scratch("fifo");
        let p = d.join("ff").display().to_string();
        let (pp, pl) = pair(&p);
        // SAFETY: a str pair this test owns.
        unsafe {
            assert_eq!(__wolf_rt_fs_mkfifo(pp, pl, 0o600), fs_code::OK);
            assert_eq!(__wolf_rt_fs_mkfifo(pp, pl, 0o600), fs_code::EXISTS);
            assert_eq!(__wolf_rt_fs_mkfifo(pp, pl, 0o1000), fs_code::INVALID);
        }
        use std::os::unix::fs::FileTypeExt as _;
        assert!(std::fs::metadata(&p).unwrap().file_type().is_fifo());
        for n in [libc::EPERM, libc::ENOSYS, libc::EOPNOTSUPP] {
            assert_eq!(
                mkfifo_code(&std::io::Error::from_raw_os_error(n)),
                fs_code::UNSUPPORTED,
                "a file system or kernel that makes no fifo ({n})"
            );
        }
        assert_eq!(
            mkfifo_code(&std::io::Error::from_raw_os_error(libc::EACCES)),
            fs_code::DENIED
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A non-blocking write end takes the pipe's capacity, then answers
    /// `busy`; 0..2 are refused the flag; an empty list writes 0.
    #[test]
    fn a_non_blocking_pipe_answers_busy_when_full() {
        let mut p = [0i32; 2];
        // SAFETY: a plain pipe this test owns.
        assert_eq!(unsafe { libc::pipe(p.as_mut_ptr()) }, 0);
        // SAFETY: both ends owned from here.
        let (r, w) = unsafe {
            use std::os::fd::FromRawFd as _;
            (
                crate::fs::mint_file(std::fs::File::from_raw_fd(p[0])),
                crate::fs::mint_file(std::fs::File::from_raw_fd(p[1])),
            )
        };
        assert_eq!(__wolf_rt_fs_set_nonblock(w, 1), fs_code::OK);
        assert_eq!(__wolf_rt_fs_set_nonblock(1, 1), fs_code::INVALID);
        assert_eq!(__wolf_rt_fs_set_nonblock(1_000_000, 1), fs_code::IO);
        let chunk = list_of(&[b'x'; 4096]);
        let mut out = [0i64; 1];
        let mut taken = 0;
        let last = loop {
            // SAFETY: a live List[byte] and an 8-byte slot.
            let rc = unsafe { __wolf_rt_fs_write_some(w, chunk, out.as_mut_ptr() as i64) };
            if rc != fs_code::OK {
                break rc;
            }
            assert!(out[0] > 0, "a non-empty write takes at least one byte");
            taken += out[0];
            assert!(taken <= 1 << 24, "a pipe that never fills");
        };
        assert_eq!(last, fs_code::BUSY);
        assert!(taken > 0);
        let empty = list_of(&[]);
        // SAFETY: as above.
        assert_eq!(
            unsafe { __wolf_rt_fs_write_some(w, empty, out.as_mut_ptr() as i64) },
            fs_code::OK
        );
        assert_eq!(out[0], 0);
        let _ = crate::fs::__wolf_rt_fs_close(r);
        let _ = crate::fs::__wolf_rt_fs_close(w);
    }
}
