//! The s40 native fs/io runtime — the s38 builtin family, real
//! filesystem, D30 rows only.
//!
//! Every entry returns a small ERROR CODE; lowering maps codes to the
//! module's interned row tags (coarsening undeclared tags to `io`,
//! exactly the checked executor's `errtag`) and builds the `!T` value —
//! the runtime never traps and never sees a tag name. Text results
//! materialize in the ambient region ([`crate::str`]'s design note)
//! and return as `{ptr, len}` pairs through caller out slots.
//!
//! The fd table mirrors the checked lane's: sequential indices, never
//! reused, `close` tombstones the slot (double close = `io`). A forged
//! or foreign fd is `io`, never a trap — same as checked.
//!
//! Semantics are `ubcheck.rs`'s `io_fs_builtin`, call for call:
//! `fs_read`'s 1 MiB clamp, `read_line`'s `\r` strip, `fs_read_text`'s
//! UTF-8 gate — parity is by construction.

use std::fs::File;
use std::io::{BufRead as _, Read as _, Write as _};
use std::sync::Mutex;

use crate::str::{ambient_copy, view, write_pair};

/// Error codes of the fs family (lowering maps them to row tags).
pub mod fs_code {
    pub const OK: i64 = 0;
    pub const NOT_FOUND: i64 = 1;
    pub const DENIED: i64 = 2;
    pub const IO: i64 = 3;
    pub const UTF8: i64 = 4;
    pub const EOF: i64 = 5;
}

static FILES: Mutex<Vec<Option<File>>> = Mutex::new(Vec::new());

fn code_of(e: &std::io::Error) -> i64 {
    match e.kind() {
        std::io::ErrorKind::NotFound => fs_code::NOT_FOUND,
        std::io::ErrorKind::PermissionDenied => fs_code::DENIED,
        _ => fs_code::IO,
    }
}

unsafe fn write_text(out: i64, bytes: Vec<u8>) -> i64 {
    match String::from_utf8(bytes) {
        Ok(s) => {
            let p = ambient_copy(s.as_bytes());
            unsafe { write_pair(out, p as i64, s.len() as i64) };
            fs_code::OK
        }
        Err(_) => fs_code::UTF8,
    }
}

/// `fs_read_text(path) -> str ! {not_found, denied, io, utf8}`.
///
/// # Safety
///
/// A valid str pair; `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_read_text(pp: i64, pl: i64, out: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    match std::fs::read(path) {
        Err(e) => code_of(&e),
        Ok(bytes) => unsafe { write_text(out, bytes) },
    }
}

/// `fs_write_text(path, contents) -> () ! {not_found, denied, io}`.
///
/// # Safety
///
/// Both pairs must be valid str pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_write_text(pp: i64, pl: i64, cp: i64, cl: i64) -> i64 {
    let (path, contents) = unsafe { (view(pp, pl), view(cp, cl)) };
    match std::fs::write(path, contents.as_bytes()) {
        Err(e) => code_of(&e),
        Ok(()) => fs_code::OK,
    }
}

/// `fs_open(path)` (create = 0) / `fs_create(path)` (create = 1):
/// the fd (>= 0), or `-code` on failure.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_open(pp: i64, pl: i64, create: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    let opened = if create == 0 {
        std::fs::OpenOptions::new().read(true).open(path)
    } else {
        File::create(path)
    };
    match opened {
        Err(e) => -code_of(&e),
        Ok(f) => {
            let mut files = FILES.lock().unwrap_or_else(|p| p.into_inner());
            files.push(Some(f));
            (files.len() - 1) as i64
        }
    }
}

/// `fs_read(fd, max) -> str ! {eof, io, utf8}` — one read of at most
/// `max` bytes (clamped to 1 MiB, the checked lane's clamp); 0 bytes
/// at a positive `max` is `eof`.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_read(fd: i64, max: i64, out: i64) -> i64 {
    if max <= 0 {
        let p = ambient_copy(b"");
        unsafe { write_pair(out, p as i64, 0) };
        return fs_code::OK;
    }
    let mut files = FILES.lock().unwrap_or_else(|p| p.into_inner());
    let Some(Some(f)) = usize::try_from(fd).ok().and_then(|i| files.get_mut(i)) else {
        return fs_code::IO;
    };
    let mut buf = vec![0u8; (max as u64).min(1 << 20) as usize];
    match f.read(&mut buf) {
        Err(e) => code_of(&e),
        Ok(0) => fs_code::EOF,
        Ok(n) => {
            buf.truncate(n);
            unsafe { write_text(out, buf) }
        }
    }
}

/// `fs_write(fd, s) -> () ! {io}`.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_write(fd: i64, sp: i64, sl: i64) -> i64 {
    let s = unsafe { view(sp, sl) };
    let mut files = FILES.lock().unwrap_or_else(|p| p.into_inner());
    let Some(Some(f)) = usize::try_from(fd).ok().and_then(|i| files.get_mut(i)) else {
        return fs_code::IO;
    };
    match f.write_all(s.as_bytes()) {
        Err(e) => code_of(&e),
        Ok(()) => fs_code::OK,
    }
}

/// `fs_close(fd) -> () ! {io}` — tombstones the slot; double close is
/// `io`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_fs_close(fd: i64) -> i64 {
    let mut files = FILES.lock().unwrap_or_else(|p| p.into_inner());
    match usize::try_from(fd).ok().and_then(|i| files.get_mut(i)) {
        Some(slot @ Some(_)) => {
            *slot = None;
            fs_code::OK
        }
        _ => fs_code::IO,
    }
}

/// `fs_remove(path) -> () ! {not_found, denied, io}`.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_remove(pp: i64, pl: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    match std::fs::remove_file(path) {
        Err(e) => code_of(&e),
        Ok(()) => fs_code::OK,
    }
}

/// `fs_exists(path) -> bool` — 1/0, never a row.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_exists(pp: i64, pl: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    i64::from(std::path::Path::new(path).exists())
}

/// `read_line() -> str ! {eof}` — one line from real stdin, `\n`
/// consumed, one trailing `\r` stripped (the checked lane's CRLF
/// handling); end of input is the `eof` tag.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_read_line(out: i64) -> i64 {
    let mut line = Vec::new();
    let stdin = std::io::stdin();
    match stdin.lock().read_until(b'\n', &mut line) {
        Err(_) => fs_code::IO,
        Ok(0) => fs_code::EOF,
        Ok(_) => {
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            unsafe { write_text(out, line) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair_of(s: &str) -> (i64, i64) {
        (s.as_ptr() as i64, s.len() as i64)
    }

    #[test]
    fn roundtrip_and_rows() {
        let dir = std::env::temp_dir().join(format!("wolf-rt-fs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.tmp");
        let path_s = path.display().to_string();
        let (pp, pl) = pair_of(&path_s);
        let (cp, cl) = pair_of("three wolves\n");
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        unsafe {
            assert_eq!(__wolf_rt_fs_write_text(pp, pl, cp, cl), fs_code::OK);
            assert_eq!(__wolf_rt_fs_exists(pp, pl), 1);
            assert_eq!(__wolf_rt_fs_read_text(pp, pl, o), fs_code::OK);
            assert_eq!(view(out[0], out[1]), "three wolves\n");
            assert_eq!(__wolf_rt_fs_remove(pp, pl), fs_code::OK);
            assert_eq!(__wolf_rt_fs_exists(pp, pl), 0);
            // A missing file is the not_found code, never a trap.
            assert_eq!(__wolf_rt_fs_read_text(pp, pl, o), fs_code::NOT_FOUND);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fd_table_matches_the_checked_shape() {
        let dir = std::env::temp_dir().join(format!("wolf-rt-fd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fd.tmp");
        let path_s = path.display().to_string();
        let (pp, pl) = pair_of(&path_s);
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        unsafe {
            let fd = __wolf_rt_fs_open(pp, pl, 1);
            assert!(fd >= 0);
            let (sp, sl) = pair_of("pack");
            assert_eq!(__wolf_rt_fs_write(fd, sp, sl), fs_code::OK);
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::IO); // double close
            let fd2 = __wolf_rt_fs_open(pp, pl, 0);
            assert!(fd2 > fd); // never reused
            assert_eq!(__wolf_rt_fs_read(fd2, 1024, o), fs_code::OK);
            assert_eq!(view(out[0], out[1]), "pack");
            assert_eq!(__wolf_rt_fs_read(fd2, 1024, o), fs_code::EOF);
            assert_eq!(__wolf_rt_fs_close(fd2), fs_code::OK);
            // A forged fd is io, never a trap.
            assert_eq!(__wolf_rt_fs_write(9999, sp, sl), fs_code::IO);
            assert_eq!(__wolf_rt_fs_remove(pp, pl), fs_code::OK);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
