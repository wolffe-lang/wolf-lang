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
//!
//! # s90 (wolf-lang#51, #52): bytes, directories, modes, atomicity
//!
//! The s38 nine covered TEXT files and nothing else. The additions
//! here are one theme in four parts, and every one of them is
//! `std::fs` only — no `#[cfg]`, no unixism, nothing that behaves
//! differently on the other side of a platform boundary. Where a
//! platform cannot do the thing, the answer is an ERROR ROW.
//!
//! - **Modes.** [`__wolf_rt_fs_open`]'s third parameter widened from a
//!   create flag to a MODE (see [`fs_mode`]). Mode 2 is a real
//!   `O_APPEND`/`FILE_APPEND_DATA` handle, so `std.fs.append_text`
//!   stops reading the file it is appending to. An unknown mode is
//!   [`fs_code::INVALID`], decided before the filesystem is touched.
//! - **Bytes.** `read_bytes`/`write_bytes` (whole file) and
//!   `read_chunk`/`write_chunk` (handle) carry `List[int]`, the byte
//!   carrier s77/s81 established. No `utf8` row: a lone `0x80` is
//!   data, not a decode failure.
//! - **Directories.** `read_dir` lists ENTRY NAMES, **sorted** — see
//!   its doc for why the alternative is untestable. `create_dir` /
//!   `remove_dir` take a recursive flag.
//! - **Atomicity.** `rename` promises the EFFECT, never atomicity;
//!   [`__wolf_rt_fs_open`]'s mode 4 (create-new) is the one
//!   atomically-promisable primitive on all three tier-1 targets.
//!   Read [`__wolf_rt_fs_rename`] before building a durable-save
//!   idiom on top of it.

use std::fs::File;
use std::io::{BufRead as _, Read as _, Write as _};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::list::{new_list, push_int, push_str};
use crate::str::{ambient_copy, view, write_pair, write_word};

/// Error codes of the fs family (lowering maps them to row tags).
///
/// 0–5 are s38's. 6–8 are s90's: `INVALID` is a caller mistake the
/// runtime decides itself (a mode outside [`fs_mode`], a `List[int]`
/// element that is not a byte), while `EXISTS` and `CROSS_DEVICE`
/// come out of `io::ErrorKind` like `NOT_FOUND`/`DENIED` and take the
/// same checked-parity coarsening: a builtin whose row does not
/// declare the tag reports `io`.
pub mod fs_code {
    pub const OK: i64 = 0;
    pub const NOT_FOUND: i64 = 1;
    pub const DENIED: i64 = 2;
    pub const IO: i64 = 3;
    pub const UTF8: i64 = 4;
    pub const EOF: i64 = 5;
    pub const INVALID: i64 = 6;
    pub const EXISTS: i64 = 7;
    pub const CROSS_DEVICE: i64 = 8;
}

/// `fs_open_mode`'s mode argument. The set is deliberately small and
/// PORTABLE: every one of these five is an `OpenOptions` combination
/// with the same meaning on linux, macOS and windows.
pub mod fs_mode {
    /// Read-only; a missing file is `not_found` (s38's `fs_open`).
    pub const READ: i64 = 0;
    /// Write-only, created if absent, TRUNCATED if present (s38's
    /// `fs_create`).
    pub const WRITE: i64 = 1;
    /// Append-only, created if absent. Every write goes to the end of
    /// the file as it is at the moment of the write — the whole point
    /// of wolf-lang#52.
    pub const APPEND: i64 = 2;
    /// Read + write, created if absent, NOT truncated. The handle
    /// starts at offset 0.
    pub const READ_WRITE: i64 = 3;
    /// Read + write, and the create must WIN: an existing path is the
    /// `exists` row. This is the one primitive whose atomicity is
    /// promisable on every tier-1 target (`O_CREAT|O_EXCL`,
    /// `CREATE_NEW`) — lock files and unique temp names build on it.
    pub const CREATE_NEW: i64 = 4;
}

static FILES: Mutex<Vec<Option<File>>> = Mutex::new(Vec::new());

fn code_of(e: &std::io::Error) -> i64 {
    match e.kind() {
        std::io::ErrorKind::NotFound => fs_code::NOT_FOUND,
        std::io::ErrorKind::PermissionDenied => fs_code::DENIED,
        std::io::ErrorKind::AlreadyExists => fs_code::EXISTS,
        std::io::ErrorKind::CrossesDevices => fs_code::CROSS_DEVICE,
        _ => fs_code::IO,
    }
}

/// A `SystemTime` as milliseconds from the Unix epoch, negative before
/// it. `None` when the value does not fit an `i64` (a clock far enough
/// off that a timestamp is meaningless) — the caller reports `io`.
fn unix_ms(t: SystemTime) -> Option<i64> {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).ok(),
        Err(before) => i64::try_from(before.duration().as_millis())
            .ok()
            .map(|ms| -ms),
    }
}

/// Read a `List[int]` header as bytes, or `None` when an element is
/// not one (outside `0..=255`, or the header's element width is wrong)
/// — the caller reports [`fs_code::INVALID`]. Same refusal the s81
/// byte source makes, with a write's spelling of the answer: writing
/// is not decoding, so `utf8` would be a lie.
///
/// # Safety
///
/// `hdr` must be a live `List[int]` header.
unsafe fn byte_elems(hdr: i64) -> Option<Vec<u8>> {
    let elems = unsafe { crate::list::i64_elems(hdr) }?;
    let mut bytes = Vec::with_capacity(elems.len());
    for &v in elems {
        bytes.push(u8::try_from(v).ok()?);
    }
    Some(bytes)
}

/// Materialize `bytes` as a `List[int]` and write its header through
/// `out`.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
unsafe fn write_bytes_list(out: i64, bytes: &[u8]) {
    let hdr = new_list(8);
    for &b in bytes {
        push_int(hdr, i64::from(b));
    }
    unsafe { write_word(out, hdr as i64) };
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

/// The open family: the fd (>= 0), or `-code` on failure.
///
/// `fs_open(path)` is mode [`fs_mode::READ`], `fs_create(path)` is
/// mode [`fs_mode::WRITE`], and `fs_open_mode(path, mode)` passes the
/// caller's own — s38's two entries are the two modes it happened to
/// have, so widening the flag kept every existing call site exact.
/// A mode outside [`fs_mode`] is `-`[`fs_code::INVALID`], decided
/// before the filesystem is touched.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_open(pp: i64, pl: i64, mode: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    let mut o = std::fs::OpenOptions::new();
    let opts = match mode {
        fs_mode::READ => o.read(true),
        fs_mode::WRITE => o.write(true).create(true).truncate(true),
        fs_mode::APPEND => o.append(true).create(true),
        fs_mode::READ_WRITE => o.read(true).write(true).create(true),
        fs_mode::CREATE_NEW => o.read(true).write(true).create_new(true),
        _ => return -fs_code::INVALID,
    };
    match opts.open(path) {
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
    // s90: the HANDLE is checked before the size. It used to be the
    // other way round here and the other way round again in the
    // checked lane, so `fs_read(closed_fd, 0)` was `ok("")` natively
    // and `io` under the executor — a cross-lane divergence #40 left
    // behind. A forged handle is `io` whatever `max` says.
    let mut files = FILES.lock().unwrap_or_else(|p| p.into_inner());
    let Some(Some(f)) = usize::try_from(fd).ok().and_then(|i| files.get_mut(i)) else {
        return fs_code::IO;
    };
    if max <= 0 {
        let p = ambient_copy(b"");
        unsafe { write_pair(out, p as i64, 0) };
        return fs_code::OK;
    }
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

// ------------------------------------- s90: bytes (wolf-lang#51) --

/// `fs_read_bytes(path) -> List[int] ! {not_found, denied, io}` — the
/// whole file as bytes. No `utf8` row: bytes are bytes.
///
/// # Safety
///
/// A valid str pair; `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_read_bytes(pp: i64, pl: i64, out: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    match std::fs::read(path) {
        Err(e) => code_of(&e),
        Ok(bytes) => {
            unsafe { write_bytes_list(out, &bytes) };
            fs_code::OK
        }
    }
}

/// `fs_write_bytes(path, bytes) -> () ! {not_found, denied, invalid,
/// io}` — the whole file from bytes; a list element outside `0..=255`
/// is `invalid` and nothing is written.
///
/// # Safety
///
/// A valid str pair; `hdr` a live `List[int]` header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_write_bytes(pp: i64, pl: i64, hdr: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    let Some(bytes) = (unsafe { byte_elems(hdr) }) else {
        return fs_code::INVALID;
    };
    match std::fs::write(path, &bytes) {
        Err(e) => code_of(&e),
        Ok(()) => fs_code::OK,
    }
}

/// `fs_read_chunk(fd, max) -> List[int] ! {eof, io}` — the byte twin
/// of `fs_read`, with the identical 1 MiB clamp and the identical
/// "0 bytes at a positive `max` is `eof`" rule. Unlike `fs_read` it
/// cannot land inside a code point, so a chunked reader over binary
/// input is finally expressible.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_read_chunk(fd: i64, max: i64, out: i64) -> i64 {
    // Handle first, size second — `fs_read`'s order, on both lanes.
    let mut files = FILES.lock().unwrap_or_else(|p| p.into_inner());
    let Some(Some(f)) = usize::try_from(fd).ok().and_then(|i| files.get_mut(i)) else {
        return fs_code::IO;
    };
    if max <= 0 {
        unsafe { write_bytes_list(out, b"") };
        return fs_code::OK;
    }
    let mut buf = vec![0u8; (max as u64).min(1 << 20) as usize];
    let r = f.read(&mut buf);
    // The fd table is released before the list is minted: allocation
    // is the ambient region's business and has no reason to sit behind
    // the fs lock.
    drop(files);
    match r {
        Err(e) => code_of(&e),
        Ok(0) => fs_code::EOF,
        Ok(n) => {
            buf.truncate(n);
            unsafe { write_bytes_list(out, &buf) };
            fs_code::OK
        }
    }
}

/// `fs_write_chunk(fd, bytes) -> () ! {invalid, io}`.
///
/// # Safety
///
/// `hdr` must be a live `List[int]` header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_write_chunk(fd: i64, hdr: i64) -> i64 {
    let Some(bytes) = (unsafe { byte_elems(hdr) }) else {
        return fs_code::INVALID;
    };
    let mut files = FILES.lock().unwrap_or_else(|p| p.into_inner());
    let Some(Some(f)) = usize::try_from(fd).ok().and_then(|i| files.get_mut(i)) else {
        return fs_code::IO;
    };
    match f.write_all(&bytes) {
        Err(e) => code_of(&e),
        Ok(()) => fs_code::OK,
    }
}

// ------------------------------- s90: directories (wolf-lang#51) --

/// `fs_read_dir(path) -> List[str] ! {not_found, denied, utf8, io}` —
/// the directory's ENTRY NAMES (not paths: joining is the caller's,
/// and a name is the one part of a directory record every tier-1
/// platform agrees on). `.` and `..` never appear.
///
/// **SORTED**, byte-wise, and that is a promise, not an accident.
/// Directory iteration order is a filesystem's private business —
/// ext4's htree hashes, APFS and NTFS index differently, and the same
/// directory reorders itself after inserts — so an unsorted listing
/// makes every test written against it pass on its author's machine
/// and fail in CI. The cost is one sort of an already-materialized
/// list. A caller that genuinely wants raw order needs a builtin whose
/// NAME says so; there is deliberately no flag.
///
/// A name the host holds in bytes this `str` tier cannot represent
/// (non-UTF-8 on linux, an unpaired surrogate on windows) fails the
/// whole listing with `utf8` rather than being silently dropped:
/// silently dropping it would make `read_dir` misreport the directory
/// with no way for the program to notice. The row is recoverable, and
/// it narrows without breaking anyone the day wolf grows an OS-string
/// type.
///
/// # Safety
///
/// A valid str pair; `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_read_dir(pp: i64, pl: i64, out: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    let entries = match std::fs::read_dir(path) {
        Err(e) => return code_of(&e),
        Ok(rd) => rd,
    };
    let mut names: Vec<String> = Vec::new();
    for entry in entries {
        match entry {
            Err(e) => return code_of(&e),
            Ok(e) => match e.file_name().into_string() {
                Ok(n) => names.push(n),
                Err(_) => return fs_code::UTF8,
            },
        }
    }
    names.sort();
    let hdr = new_list(16);
    for n in &names {
        push_str(hdr, n);
    }
    unsafe { write_word(out, hdr as i64) };
    fs_code::OK
}

/// `fs_create_dir(path)` (all = 0) / `fs_create_dir_all(path)`
/// (all = 1) `-> () ! {exists, not_found, denied, io}`.
///
/// The single-level form is strict: an existing path is `exists`, a
/// missing parent is `not_found`. The recursive form creates the
/// parents and is idempotent — an already-present directory is OK,
/// which is what makes it the one to reach for before a write.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_create_dir(pp: i64, pl: i64, all: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    let r = if all == 0 {
        std::fs::create_dir(path)
    } else {
        std::fs::create_dir_all(path)
    };
    match r {
        Err(e) => code_of(&e),
        Ok(()) => fs_code::OK,
    }
}

/// `fs_remove_dir(path)` (all = 0) / `fs_remove_dir_all(path)`
/// (all = 1) `-> () ! {not_found, denied, io}`.
///
/// The single-level form removes an EMPTY directory only; a non-empty
/// one is `io` (the platforms disagree on the errno's identity, and
/// rule 3 of the taxonomy is one tag per actionable response — the
/// response to both is the same). The recursive form is the inverse of
/// `create_dir_all`, so a program that made a tree can unmake it.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_remove_dir(pp: i64, pl: i64, all: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    let r = if all == 0 {
        std::fs::remove_dir(path)
    } else {
        std::fs::remove_dir_all(path)
    };
    match r {
        Err(e) => code_of(&e),
        Ok(()) => fs_code::OK,
    }
}

// ---------------------------------- s90: metadata (wolf-lang#51) --

/// `fs_is_file(path)` (want = 0) / `fs_is_dir(path)` (want = 1) —
/// 1/0, never a row, following symlinks. TOTAL like `fs_exists`: an
/// unreadable or missing path is simply not a file and not a
/// directory, so `exists` can finally say WHAT exists.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_is(pp: i64, pl: i64, want: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    let Ok(md) = std::fs::metadata(path) else {
        return 0;
    };
    i64::from(if want == 0 { md.is_file() } else { md.is_dir() })
}

/// `fs_size(path)` (which = 0) / `fs_modified_ms(path)` (which = 1)
/// `-> int ! {not_found, denied, io}`.
///
/// `size` is bytes. `modified_ms` is milliseconds from the Unix epoch,
/// negative before it — the `time_unix_ms` unit, so the two are
/// comparable without a conversion nobody would get right. A host that
/// cannot report a modification time, or reports one outside `i64`
/// milliseconds, is `io`: there is no `unsupported` tag because there
/// is no different response to it.
///
/// # Safety
///
/// A valid str pair; `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_stat(pp: i64, pl: i64, which: i64, out: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    let md = match std::fs::metadata(path) {
        Err(e) => return code_of(&e),
        Ok(m) => m,
    };
    let v = match which {
        0 => match i64::try_from(md.len()) {
            Ok(n) => n,
            Err(_) => return fs_code::IO,
        },
        1 => match md.modified().ok().and_then(unix_ms) {
            Some(ms) => ms,
            None => return fs_code::IO,
        },
        _ => return fs_code::INVALID,
    };
    unsafe { write_word(out, v) };
    fs_code::OK
}

// ------------------------------------ s90: rename (wolf-lang#51) --

/// `fs_rename(from, to) -> () ! {not_found, denied, cross_device,
/// exists, io}` — move a file or directory within a filesystem, in
/// one operation, WITHOUT reading its contents. This is what
/// `std.fs.move_file` was emulating with copy-then-remove.
///
/// # It does not promise atomicity, and the missing name is the point
///
/// POSIX `rename(2)` replaces an existing destination atomically.
/// Windows does not offer that guarantee: `MoveFileEx` with
/// `MOVEFILE_REPLACE_EXISTING` is documented to replace, not to
/// replace atomically, and it fails outright against a destination
/// another process holds open without delete sharing. So "rename over
/// a live file and readers see one version or the other" is a promise
/// that cannot be kept on a tier-1 target — and per the platform rule,
/// a promise that cannot be kept portably does not get a `#[cfg]` that
/// keeps it on two targets out of three.
///
/// The consequence is a NAME that is absent: there is no
/// `fs_rename_atomic`, and this one claims only the effect. The
/// atomically-promisable primitive the language does offer is
/// [`fs_mode::CREATE_NEW`] — exclusive creation wins or loses
/// atomically everywhere — which is the right base for lock files and
/// unique temp names.
///
/// `cross_device` is the one universal divergence from "the move
/// works": `EXDEV` on unix, `ERROR_NOT_SAME_DEVICE` on windows. It is
/// a declared row precisely so a caller can fall back to
/// `read_bytes` + `write_bytes` + `remove` — which, as of this sprint,
/// is a real fallback for binary files and not a text operation
/// wearing a disguise.
///
/// The other place the platforms part company is a rename ONTO an
/// existing DIRECTORY: unix replaces an empty one and refuses a
/// non-empty one, windows refuses both. That is why `exists` is in
/// the row — the divergence surfaces as a tag a caller can handle,
/// which is the rule (a platform difference becomes a row, never a
/// silent difference in what the call did).
///
/// # Safety
///
/// Two valid str pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_rename(fp: i64, fl: i64, tp: i64, tl: i64) -> i64 {
    let (from, to) = unsafe { (view(fp, fl), view(tp, tl)) };
    match std::fs::rename(from, to) {
        Err(e) => code_of(&e),
        Ok(()) => fs_code::OK,
    }
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

    // ------------------------------------------------------ s90 --

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wolf-rt-s90-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn list_i64(hdr: i64) -> Vec<i64> {
        let n = unsafe { crate::list::__wolf_rt_list_len(hdr) };
        (0..n)
            .map(|i| {
                let mut cell = [0i64; 1];
                let rc =
                    unsafe { crate::list::__wolf_rt_list_read(hdr, i, cell.as_mut_ptr() as i64) };
                assert_eq!(rc, 1);
                cell[0]
            })
            .collect()
    }

    fn list_str(hdr: i64) -> Vec<String> {
        let n = unsafe { crate::list::__wolf_rt_list_len(hdr) };
        (0..n)
            .map(|i| {
                let mut pair = [0i64; 2];
                let rc =
                    unsafe { crate::list::__wolf_rt_list_read(hdr, i, pair.as_mut_ptr() as i64) };
                assert_eq!(rc, 1);
                unsafe { view(pair[0], pair[1]).to_string() }
            })
            .collect()
    }

    /// A list of bytes, the shape a compiled `List[int]` argument has.
    fn bytes_list(bs: &[i64]) -> i64 {
        let hdr = crate::list::new_list(8);
        for &b in bs {
            crate::list::push_int(hdr, b);
        }
        hdr as i64
    }

    /// #52: append mode adds, it does not rewrite. A lone `0x80` in
    /// the file proves the append never decoded what was already
    /// there — the exact complaint the issue makes.
    #[test]
    fn append_mode_appends_without_reading() {
        let dir = scratch("append");
        let path = dir.join("log.bin");
        std::fs::write(&path, [0x80u8]).unwrap();
        let p = path.display().to_string();
        let (pp, pl) = pair_of(&p);
        unsafe {
            let fd = __wolf_rt_fs_open(pp, pl, fs_mode::APPEND);
            assert!(fd >= 0);
            let (sp, sl) = pair_of("tail");
            assert_eq!(__wolf_rt_fs_write(fd, sp, sl), fs_code::OK);
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
        }
        assert_eq!(std::fs::read(&path).unwrap(), b"\x80tail");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn modes_cover_read_write_append_rw_and_exclusive() {
        let dir = scratch("modes");
        let path = dir.join("m.txt");
        let p = path.display().to_string();
        let (pp, pl) = pair_of(&p);
        unsafe {
            // READ on a missing file is not_found, not a create.
            assert_eq!(
                __wolf_rt_fs_open(pp, pl, fs_mode::READ),
                -fs_code::NOT_FOUND
            );
            // CREATE_NEW wins once, then loses with `exists`.
            let fd = __wolf_rt_fs_open(pp, pl, fs_mode::CREATE_NEW);
            assert!(fd >= 0);
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            assert_eq!(
                __wolf_rt_fs_open(pp, pl, fs_mode::CREATE_NEW),
                -fs_code::EXISTS
            );
            // WRITE truncates; READ_WRITE does not.
            let fd = __wolf_rt_fs_open(pp, pl, fs_mode::WRITE);
            let (sp, sl) = pair_of("abcd");
            assert_eq!(__wolf_rt_fs_write(fd, sp, sl), fs_code::OK);
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            let fd = __wolf_rt_fs_open(pp, pl, fs_mode::READ_WRITE);
            assert!(fd >= 0);
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            assert_eq!(std::fs::read(&path).unwrap(), b"abcd");
            // An unknown mode never touches the filesystem.
            assert_eq!(__wolf_rt_fs_open(pp, pl, 99), -fs_code::INVALID);
            assert_eq!(__wolf_rt_fs_open(pp, pl, -1), -fs_code::INVALID);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #51: byte io is not text io. The witness is a file whose
    /// contents no text reader can hold.
    #[test]
    fn byte_io_carries_a_lone_0x80() {
        let dir = scratch("bytes");
        let path = dir.join("bin.dat");
        let p = path.display().to_string();
        let (pp, pl) = pair_of(&p);
        let mut out = [0i64; 1];
        let o = out.as_mut_ptr() as i64;
        unsafe {
            let src = bytes_list(&[0x80, 0, 0xff, 0x41]);
            assert_eq!(__wolf_rt_fs_write_bytes(pp, pl, src), fs_code::OK);
            // The text reader refuses what the byte reader carries.
            let mut text = [0i64; 2];
            assert_eq!(
                __wolf_rt_fs_read_text(pp, pl, text.as_mut_ptr() as i64),
                fs_code::UTF8
            );
            assert_eq!(__wolf_rt_fs_read_bytes(pp, pl, o), fs_code::OK);
            assert_eq!(list_i64(out[0]), vec![0x80, 0, 0xff, 0x41]);
            // Chunked, over a handle, at a boundary that would have
            // split a code point for `fs_read`.
            let fd = __wolf_rt_fs_open(pp, pl, fs_mode::READ);
            assert_eq!(__wolf_rt_fs_read_chunk(fd, 1, o), fs_code::OK);
            assert_eq!(list_i64(out[0]), vec![0x80]);
            assert_eq!(__wolf_rt_fs_read_chunk(fd, 8, o), fs_code::OK);
            assert_eq!(list_i64(out[0]), vec![0, 0xff, 0x41]);
            assert_eq!(__wolf_rt_fs_read_chunk(fd, 8, o), fs_code::EOF);
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            // Not-a-byte is `invalid`, on both entry points.
            assert_eq!(
                __wolf_rt_fs_write_bytes(pp, pl, bytes_list(&[256])),
                fs_code::INVALID
            );
            let fd = __wolf_rt_fs_open(pp, pl, fs_mode::APPEND);
            assert_eq!(
                __wolf_rt_fs_write_chunk(fd, bytes_list(&[-1])),
                fs_code::INVALID
            );
            assert_eq!(
                __wolf_rt_fs_write_chunk(fd, bytes_list(&[0x2e])),
                fs_code::OK
            );
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            // The refused write left the file alone; the accepted one
            // appended one byte.
            assert_eq!(__wolf_rt_fs_read_bytes(pp, pl, o), fs_code::OK);
            assert_eq!(list_i64(out[0]), vec![0x80, 0, 0xff, 0x41, 0x2e]);
            // A forged fd is io, never a trap — the s38 rule holds.
            assert_eq!(__wolf_rt_fs_read_chunk(9999, 8, o), fs_code::IO);
            assert_eq!(
                __wolf_rt_fs_write_chunk(9999, bytes_list(&[1])),
                fs_code::IO
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The sorting DECISION, asserted: entries come back byte-ordered
    /// whatever order the filesystem hands them over in. Created out
    /// of order on purpose.
    #[test]
    fn read_dir_is_sorted_and_names_only() {
        let dir = scratch("readdir");
        for n in ["zebra.txt", "alpha.txt", "Mid.txt", "beta"] {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        std::fs::create_dir(dir.join("sub")).unwrap();
        let p = dir.display().to_string();
        let (pp, pl) = pair_of(&p);
        let mut out = [0i64; 1];
        unsafe {
            assert_eq!(
                __wolf_rt_fs_read_dir(pp, pl, out.as_mut_ptr() as i64),
                fs_code::OK
            );
        }
        // Byte order: uppercase before lowercase, no `.`/`..`, names
        // rather than paths.
        assert_eq!(
            list_str(out[0]),
            vec!["Mid.txt", "alpha.txt", "beta", "sub", "zebra.txt"]
        );
        let missing = dir.join("nope");
        let m = missing.display().to_string();
        let (mp, ml) = pair_of(&m);
        assert_eq!(
            unsafe { __wolf_rt_fs_read_dir(mp, ml, out.as_mut_ptr() as i64) },
            fs_code::NOT_FOUND
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dir_create_remove_and_metadata() {
        let dir = scratch("dirs");
        let deep = dir.join("a/b/c");
        let d = deep.display().to_string();
        let (dp, dl) = pair_of(&d);
        let one = dir.join("solo");
        let s = one.display().to_string();
        let (sp_, sl_) = pair_of(&s);
        let mut out = [0i64; 1];
        let o = out.as_mut_ptr() as i64;
        unsafe {
            // Single-level: a missing parent is not_found.
            assert_eq!(__wolf_rt_fs_create_dir(dp, dl, 0), fs_code::NOT_FOUND);
            assert_eq!(__wolf_rt_fs_create_dir(dp, dl, 1), fs_code::OK);
            // The recursive form is idempotent; the strict one is not.
            assert_eq!(__wolf_rt_fs_create_dir(dp, dl, 1), fs_code::OK);
            assert_eq!(__wolf_rt_fs_create_dir(dp, dl, 0), fs_code::EXISTS);
            assert_eq!(__wolf_rt_fs_create_dir(sp_, sl_, 0), fs_code::OK);
            // Metadata says WHAT exists.
            assert_eq!(__wolf_rt_fs_is(sp_, sl_, 0), 0); // is_file
            assert_eq!(__wolf_rt_fs_is(sp_, sl_, 1), 1); // is_dir
            let f = dir.join("f.bin");
            std::fs::write(&f, b"12345").unwrap();
            let fs_ = f.display().to_string();
            let (fp, fl) = pair_of(&fs_);
            assert_eq!(__wolf_rt_fs_is(fp, fl, 0), 1);
            assert_eq!(__wolf_rt_fs_is(fp, fl, 1), 0);
            assert_eq!(__wolf_rt_fs_stat(fp, fl, 0, o), fs_code::OK);
            assert_eq!(out[0], 5);
            assert_eq!(__wolf_rt_fs_stat(fp, fl, 1, o), fs_code::OK);
            // A file written just now is stamped within a decade of
            // now on any sane host — the assertion is unit sanity
            // (ms, not s, not ns), not clock precision.
            let now = crate::time::__wolf_rt_time_unix_ms();
            assert!(
                (out[0] - now).abs() < 315_360_000_000,
                "modified_ms {} vs now {now}",
                out[0]
            );
            // A missing path is the row, never a trap or a sentinel.
            let gone = dir.join("gone");
            let g = gone.display().to_string();
            let (gp, gl) = pair_of(&g);
            assert_eq!(__wolf_rt_fs_stat(gp, gl, 0, o), fs_code::NOT_FOUND);
            assert_eq!(__wolf_rt_fs_is(gp, gl, 0), 0);
            assert_eq!(__wolf_rt_fs_is(gp, gl, 1), 0);
            // Non-empty removal is io; recursive removal is the
            // inverse of create_dir_all.
            let ap = dir.join("a");
            let a = ap.display().to_string();
            let (app, apl) = pair_of(&a);
            assert_eq!(__wolf_rt_fs_remove_dir(app, apl, 0), fs_code::IO);
            assert_eq!(__wolf_rt_fs_remove_dir(app, apl, 1), fs_code::OK);
            assert_eq!(__wolf_rt_fs_remove_dir(app, apl, 1), fs_code::NOT_FOUND);
            assert_eq!(__wolf_rt_fs_remove_dir(sp_, sl_, 0), fs_code::OK);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_moves_without_reading() {
        let dir = scratch("rename");
        let from = dir.join("from.bin");
        // Contents no text path could carry: the move is not a copy
        // through a `str`.
        std::fs::write(&from, [0x80u8, 0xff]).unwrap();
        let to = dir.join("to.bin");
        let (f, t) = (from.display().to_string(), to.display().to_string());
        let (fp, fl) = pair_of(&f);
        let (tp, tl) = pair_of(&t);
        unsafe {
            assert_eq!(__wolf_rt_fs_rename(fp, fl, tp, tl), fs_code::OK);
        }
        assert!(!from.exists());
        assert_eq!(std::fs::read(&to).unwrap(), [0x80, 0xff]);
        // A missing source is the row.
        assert_eq!(
            unsafe { __wolf_rt_fs_rename(fp, fl, tp, tl) },
            fs_code::NOT_FOUND
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
