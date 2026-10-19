//! s248 (wolf-lang#796, #797; ruling #93; yew `docs/MAP.md` §5 G3, G4):
//! what an editor's save and its workspace ask of the file system — the
//! two syncs of the atomic save and the canonical path. `[os.fs.sync]`,
//! `[os.fs.realpath]`.
//!
//! **The atomic save** (yew's invariant 1) is five calls: create a
//! temporary file in the target's directory, exclusively (open mode 4,
//! or `fs_open_perm(tmp, 4, perm)` for the target's permission bits —
//! `[os.fs.open]`, `[os.fs.create.mode]`; `exists` when the name is
//! taken), write it, [`__wolf_rt_fs_sync`] it, `fs_rename` it over the
//! target, and [`__wolf_rt_fs_sync_dir`] the directory so the rename is
//! on the disk too. A crash at any point leaves the old file or the new
//! one, never a mix and never neither — the sync before the rename makes
//! the new bytes durable before any name points at them, and the sync of
//! the directory makes the name's move durable.
//!
//! A module of its own, as `fs_change.rs` is: it shares [`crate::fs`]'s
//! handle table, error codes and host-error word and nothing else. Each
//! sync is a wait the program asked for (a disk flush takes
//! milliseconds), so it runs as one ([`crate::host_wait`]: the pool
//! compensates a worker it holds) and on a duplicate of the handle, never
//! under the handle table's lock.

use crate::fs::{code_of, dup_of, fs_code, os_error_clear};
use crate::str::{ambient_copy, view, write_pair};

/// `fs_sync(fd) -> () ! {io}` (`[os.fs.sync]`) — the file's bytes and
/// metadata on the disk: `fsync(2)` on linux, `fcntl(F_FULLFSYNC)` on
/// macOS (its `fsync(2)` leaves them in the drive's cache), and
/// `FlushFileBuffers` on windows — `File::sync_all`. 0..2 are the
/// standard streams (`[os.fs.std]`). A closed or forged handle is `io`
/// with no host number; every host refusal (a pipe, a terminal, a
/// windows handle open only for reading) is `io` with its number in
/// `os_error()`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_fs_sync(fd: i64) -> i64 {
    os_error_clear();
    let Some(f) = dup_of(fd) else {
        return fs_code::IO;
    };
    match crate::host_wait(|| f.sync_all()) {
        Ok(()) => fs_code::OK,
        Err(e) => {
            let _ = code_of(&e);
            fs_code::IO
        }
    }
}

/// `fs_sync_dir(path) -> () ! {denied, invalid, io, not_found,
/// unsupported}` (`[os.fs.sync]`) — a directory's entries on the disk:
/// the directory opened for reading (`O_DIRECTORY`, close-on-exec) and
/// synced as [`__wolf_rt_fs_sync`] syncs a file, so a rename or a
/// creation in it survives a crash. A missing name is `not_found`, a
/// name that is not a directory `invalid` (`ENOTDIR`), a refused search
/// `denied`, anything else `io`. windows has no directory to sync (its
/// directories are not opened as files) and answers `unsupported` by
/// name before the path is read.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_sync_dir(pp: i64, pl: i64) -> i64 {
    os_error_clear();
    let path = unsafe { view(pp, pl) };
    sync_dir(path)
}

#[cfg(unix)]
fn sync_dir(path: &str) -> i64 {
    use std::os::unix::fs::OpenOptionsExt as _;
    let opened = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(path);
    let d = match opened {
        Ok(d) => d,
        Err(e) if e.raw_os_error() == Some(libc::ENOTDIR) => {
            crate::fs::os_error_record(&e);
            return fs_code::INVALID;
        }
        Err(e) => return code_of(&e),
    };
    match crate::host_wait(|| d.sync_all()) {
        Ok(()) => fs_code::OK,
        Err(e) => {
            let _ = code_of(&e);
            fs_code::IO
        }
    }
}

#[cfg(not(unix))]
fn sync_dir(_path: &str) -> i64 {
    fs_code::UNSUPPORTED
}

/// `fs_realpath(path) -> str ! {io, not_found}` (`[os.fs.realpath]`) —
/// the canonical absolute name of what `path` names, as `realpath(3)`
/// resolves it (`std::fs::canonicalize`): every link followed, every `.`
/// and `..` taken after the link before it (so `l/..` is the link
/// target's parent), no trailing slash. A relative path is resolved
/// against the working directory. A missing name — a dangling link
/// included — is `not_found`; a loop of links, a component that is not
/// a directory, a refused search and an answer that is not UTF-8 are
/// `io`, with the host's number in `os_error()` where it gave one.
/// windows answers the host's final name (`GetFinalPathNameByHandleW`,
/// the `\\?\` form).
///
/// # Safety
///
/// A valid str pair; `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_realpath(pp: i64, pl: i64, out: i64) -> i64 {
    os_error_clear();
    let path = unsafe { view(pp, pl) };
    match std::fs::canonicalize(path) {
        Err(e) => match code_of(&e) {
            fs_code::NOT_FOUND => fs_code::NOT_FOUND,
            _ => fs_code::IO,
        },
        Ok(p) => match p.to_str() {
            None => fs_code::IO,
            Some(s) => {
                let cp = ambient_copy(s.as_bytes());
                unsafe { write_pair(out, cp as i64, s.len() as i64) };
                fs_code::OK
            }
        },
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn pair(s: &str) -> (i64, i64) {
        (s.as_ptr() as i64, s.len() as i64)
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wolf-rt-save-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::canonicalize(&d).unwrap()
    }

    /// A file open for writing syncs; a forged handle and a pipe are
    /// `io`, the pipe with the host's number.
    #[test]
    fn sync_a_file_and_refuse_the_rest() {
        let d = scratch("sync");
        let f = std::fs::File::create(d.join("f")).unwrap();
        let h = crate::fs::mint_file(f);
        assert_eq!(__wolf_rt_fs_sync(h), fs_code::OK);
        assert_eq!(__wolf_rt_fs_sync(1_000_000), fs_code::IO);
        assert_eq!(crate::fs::__wolf_rt_os_error(), 0, "a forged handle has no host number");
        let mut p = [0i32; 2];
        // SAFETY: a plain pipe this test owns.
        assert_eq!(unsafe { libc::pipe(p.as_mut_ptr()) }, 0);
        // SAFETY: the write end, owned from here.
        let w = crate::fs::mint_file(unsafe {
            <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(p[1])
        });
        assert_eq!(__wolf_rt_fs_sync(w), fs_code::IO);
        assert_ne!(crate::fs::__wolf_rt_os_error(), 0, "the host refused the pipe");
        // SAFETY: the read end, closed.
        unsafe { libc::close(p[0]) };
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The directory syncs; a missing name is `not_found`, a file
    /// `invalid`.
    #[test]
    fn sync_a_directory_and_refuse_the_rest() {
        let d = scratch("dir");
        std::fs::write(d.join("f"), b"x").unwrap();
        let s = d.to_str().unwrap().to_string();
        let (p, l) = pair(&s);
        // SAFETY: a str pair this test owns.
        assert_eq!(unsafe { __wolf_rt_fs_sync_dir(p, l) }, fs_code::OK);
        let missing = format!("{s}/none");
        let (p, l) = pair(&missing);
        // SAFETY: as above.
        assert_eq!(unsafe { __wolf_rt_fs_sync_dir(p, l) }, fs_code::NOT_FOUND);
        let file = format!("{s}/f");
        let (p, l) = pair(&file);
        // SAFETY: as above.
        assert_eq!(unsafe { __wolf_rt_fs_sync_dir(p, l) }, fs_code::INVALID);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A link's `..` is taken after the link; a dangling link is
    /// `not_found`, a loop `io`.
    #[test]
    fn realpath_follows_then_climbs() {
        let d = scratch("real");
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        std::os::unix::fs::symlink("a/b", d.join("l")).unwrap();
        std::os::unix::fs::symlink("nope", d.join("dangle")).unwrap();
        std::os::unix::fs::symlink("y", d.join("x")).unwrap();
        std::os::unix::fs::symlink("x", d.join("y")).unwrap();
        let ask = |rel: &str| -> Result<String, i64> {
            let path = format!("{}/{rel}", d.display());
            let (p, l) = pair(&path);
            let mut out = [0i64; 2];
            // SAFETY: a str pair and a 16-byte slot this test owns.
            let rc = unsafe { __wolf_rt_fs_realpath(p, l, out.as_mut_ptr() as i64) };
            if rc != fs_code::OK {
                return Err(rc);
            }
            // SAFETY: the shim wrote a (pointer, length) pair of UTF-8.
            Ok(unsafe { view(out[0], out[1]) }.to_string())
        };
        let base = d.display().to_string();
        assert_eq!(ask("l/.."), Ok(format!("{base}/a")));
        assert_eq!(ask("l"), Ok(format!("{base}/a/b")));
        assert_eq!(ask("dangle"), Err(fs_code::NOT_FOUND));
        assert_eq!(ask("x"), Err(fs_code::IO));
        let _ = std::fs::remove_dir_all(&d);
    }
}
