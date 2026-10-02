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
//!   `read_chunk`/`write_chunk` (handle) carry `List[byte]` (s136,
//!   wolf-lang#231 — `List[int]`, the carrier s77/s81 established,
//!   before that), minted at exact capacity so a read charges its
//!   region one header plus the payload. No `utf8` row: a lone
//!   `0x80` is data, not a decode failure.
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
/// runtime decides itself (a mode outside [`fs_mode`], a byte list of
/// the wrong element width — an FFI caller's, since s136 typed the
/// argument `List[byte]`), while `EXISTS` and `CROSS_DEVICE`
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
    /// s199 (wolf-lang#426, `[os.fs.seek]`): the handle has no offset
    /// to move or read at — a pipe, a fifo, a socket, a terminal
    /// (`ESPIPE`). A tag of its own because the response differs from
    /// `io`: a program falls back to reading forward.
    pub const UNSEEKABLE: i64 = 9;
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
    /// Read-only, and the OPEN ITSELF may not park (s149,
    /// wolf-lang#289, `[os.fs.open]`): `O_NONBLOCK` rides the unix
    /// open, so a name that turns out to be a fifo with no writer
    /// answers a HANDLE at once instead of parking the calling hand
    /// until some writer appears. A regular file — every request that
    /// matters to a file server — is bit for bit mode [`READ`]: the
    /// flag has no effect on a regular file's open, read or write.
    /// What it buys is that a server stops paying a PATH stat to learn
    /// whether the name is safe to open and classifies from
    /// `fs_fstat` on the handle it already has (nginx's
    /// `ngx_open_file_wrapper` shape). windows has no such flag on
    /// this path and serves the mode as [`READ`], by name in the
    /// clause.
    pub const READ_NONBLOCK: i64 = 5;
}

/// The open files, by handle. Slots 0, 1 and 2 are never filled: those
/// three numbers are the process's standard streams (`[os.fs.std]`,
/// s199 — wolf-lang#424), so the first open answers 3 and
/// [`std_stream`] serves 0..2 for the calls that take them.
static FILES: Mutex<Vec<Option<File>>> = Mutex::new(Vec::new());

/// The first number an open answers (`[os.fs.std]`).
pub const FIRST_HANDLE: usize = 3;

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

/// Read a `List[byte]` header as bytes, or `None` when the header's
/// element width is not one — the caller reports [`fs_code::INVALID`].
/// Since s136 (wolf-lang#231) the consumers are typed `List[byte]`, so
/// an out-of-range element is unconstructible and the only `invalid` a
/// direct FFI caller can earn is a list of the wrong width (the s81
/// byte source's refusal, with a write's spelling of the answer:
/// writing is not decoding, so `utf8` would be a lie).
///
/// # Safety
///
/// `hdr` must be a live list header.
pub(crate) unsafe fn byte_elems(hdr: i64) -> Option<Vec<u8>> {
    unsafe { crate::list::u8_elems(hdr) }.map(<[u8]>::to_vec)
}

/// Materialize `bytes` as a `List[byte]` — one buffer at exact
/// capacity ([`crate::list::from_bytes`]) — and write its header
/// through `out`.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
pub(crate) unsafe fn write_bytes_list(out: i64, bytes: &[u8]) {
    let hdr = crate::list::from_bytes(bytes);
    unsafe { write_word(out, hdr as i64) };
}

/// Materialize `bytes` as a `List[int]` — one `int` per octet — and
/// write its header through `out`. The pre-s136 byte carrier, kept for
/// the one producer whose clause still says `List[int]`: `os_random`
/// (`[os.random]`). Not a byte surface, so not one of #231's eight.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
pub(crate) unsafe fn write_int_list(out: i64, bytes: &[u8]) {
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
        // s149 (#289): the same read open, carrying `O_NONBLOCK` on
        // the hosts that have it — one flag on the open the program
        // was going to make anyway, never a second call. windows has
        // no equivalent here and serves the mode as READ.
        fs_mode::READ_NONBLOCK => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                o.read(true).custom_flags(libc::O_NONBLOCK)
            }
            #[cfg(not(unix))]
            {
                o.read(true)
            }
        }
        _ => return -fs_code::INVALID,
    };
    match opts.open(path) {
        Err(e) => -code_of(&e),
        Ok(f) => {
            let mut files = FILES.lock().unwrap_or_else(|p| p.into_inner());
            if files.len() < FIRST_HANDLE {
                files.resize_with(FIRST_HANDLE, || None);
            }
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

/// `fs_read_bytes(path) -> List[byte] ! {not_found, denied, io}` — the
/// whole file as bytes (`List[byte]` since s136, wolf-lang#231: one
/// ledger byte per payload byte). No `utf8` row: bytes are bytes.
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
/// io}` — the whole file from a `List[byte]`; a list of the wrong
/// element width (an FFI caller's, never typed code's) is `invalid`
/// and nothing is written.
///
/// # Safety
///
/// A valid str pair; `hdr` a live `List[byte]` header.
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

/// `fs_read_chunk(fd, max) -> List[byte] ! {eof, io}` — the byte twin
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

/// `fs_write_chunk(fd, bytes) -> () ! {invalid, io}` — `bytes` a
/// `List[byte]` (s136).
///
/// # Safety
///
/// `hdr` must be a live `List[byte]` header.
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

// ---------------------- s142: the stat on a handle (wolf-lang#261) --

/// `fs_fstat(fd) -> List[int] ! {not_found, denied, io}` — `[kind,
/// size, modified_ms]` from ONE `metadata()` on the open handle:
/// nginx's `open` + `fstat`, where a wolf file server was paying path
/// stats for each answer (#261). `kind` is 0 for a regular file, 1 for
/// a directory, 2 for anything else (a fifo, a socket, a device);
/// `size` and `modified_ms` are [`__wolf_rt_fs_stat`]'s words, in its
/// units and with its `io` for a value outside `i64`. A closed or
/// forged handle is `io`, the family's rule.
///
/// The row set is the path stat's — `not_found` and `denied` are
/// declared so a caller can write one handler for both spellings —
/// though on an open handle the hosts answer `io` for nearly
/// everything: the entry is already resolved.
///
/// Host posture: linux, macOS and freebsd open a directory read-only
/// (`fs_open` on a directory succeeds), so `kind` 1 is reachable there.
/// Windows refuses to open a directory as a file (`denied` from
/// `fs_open` — `CreateFileW` without `FILE_FLAG_BACKUP_SEMANTICS`), so
/// on windows every handle this call sees is a file or a device, and
/// a server classifies directories by path there, as it did before.
/// That difference is `fs_open`'s and is stated, not papered.
///
/// s199 (wolf-lang#424, `[os.fs.std]`): `fd` 0, 1 and 2 are the
/// process's standard streams, so a program can ask what its stdin or
/// stdout IS — a regular file (`kind` 0, its size), a pipe or a
/// terminal (`kind` 2). Before s199 those three numbers were indices
/// into this table and answered `io` whatever the descriptors were.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_fstat(fd: i64, out: i64) -> i64 {
    // s199 (#424): 0, 1 and 2 are the standard streams (`[os.fs.std]`).
    // The fd table is released before the list is minted: allocation
    // is the ambient region's business (`fs_read_chunk`'s order).
    let Some((md, disk)) = with_handle(fd, |f| (f.metadata(), is_disk(f))) else {
        return fs_code::IO;
    };
    let md = match md {
        Err(e) => return code_of(&e),
        Ok(m) => m,
    };
    // windows: a handle that is not a disk file is `kind` 2 whatever
    // the metadata calls it ([`is_disk`]).
    let kind = if md.is_file() && disk {
        0
    } else if md.is_dir() {
        1
    } else {
        2
    };
    let Ok(size) = i64::try_from(md.len()) else {
        return fs_code::IO;
    };
    let Some(ms) = md.modified().ok().and_then(unix_ms) else {
        return fs_code::IO;
    };
    let hdr = new_list(8);
    push_int(hdr, kind);
    push_int(hdr, size);
    push_int(hdr, ms);
    unsafe { write_word(out, hdr as i64) };
    fs_code::OK
}

// ---------- s199: offsets and the standard streams (#426, #424) --

/// Descriptors 0, 1 and 2 — the standard streams the process was
/// started with — borrowed as a `File` for one call, never closed.
/// `None` is a host that hands the process no such stream (windows'
/// null std handle); a descriptor that is merely closed reaches the
/// host call and answers its error there (`EBADF`, the `io` row).
fn std_stream(fd: i64) -> Option<std::mem::ManuallyDrop<File>> {
    #[cfg(unix)]
    {
        use std::os::unix::io::FromRawFd as _;
        let n = i32::try_from(fd).ok().filter(|n| (0..3).contains(n))?;
        // SAFETY: 0..2 are integers the host defines, and the
        // `ManuallyDrop` means this borrow never closes the descriptor
        // whatever it names.
        Some(std::mem::ManuallyDrop::new(unsafe { File::from_raw_fd(n) }))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _};
        let h = match fd {
            0 => std::io::stdin().as_raw_handle(),
            1 => std::io::stdout().as_raw_handle(),
            2 => std::io::stderr().as_raw_handle(),
            _ => return None,
        };
        if h.is_null() {
            return None;
        }
        // SAFETY: the process's own std handle, borrowed for one call;
        // the `ManuallyDrop` never closes it.
        Some(std::mem::ManuallyDrop::new(unsafe {
            File::from_raw_handle(h)
        }))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = fd;
        None
    }
}

/// Run `f` on the file `fd` names — a standard stream for 0..2
/// ([`std_stream`]), the table's slot otherwise, held under the table
/// lock for the call. `None` is a closed or forged handle (`io`).
fn with_handle<R>(fd: i64, f: impl FnOnce(&File) -> R) -> Option<R> {
    if (0..FIRST_HANDLE as i64).contains(&fd) {
        let file = std_stream(fd)?;
        return Some(f(&file));
    }
    let files = FILES.lock().unwrap_or_else(|p| p.into_inner());
    let Some(Some(file)) = usize::try_from(fd).ok().and_then(|i| files.get(i)) else {
        return None;
    };
    Some(f(file))
}

/// The code of a failed offset call: `unseekable` for `ESPIPE`
/// (`ErrorKind::NotSeekable`), `invalid` for an offset the host refuses
/// as out of range (`EINVAL`, a seek below zero), the family's mapping
/// otherwise.
fn seek_code(e: &std::io::Error) -> i64 {
    match e.kind() {
        // PLANT (s199): the runtime forgets ESPIPE — the gate's native
        // and release legs must go red while the checked leg stays green.
        std::io::ErrorKind::NotSeekable => fs_code::IO,
        std::io::ErrorKind::InvalidInput => fs_code::INVALID,
        _ => code_of(e),
    }
}

/// Is `f` a disk file, by the host's own classification? unix: always
/// `true` here — `fstat`'s mode bits already tell a fifo, a socket or a
/// device from a regular file, and an offset call there answers the
/// host's `ESPIPE`. windows: `GetFileType` is `FILE_TYPE_DISK`. The
/// metadata std reads on windows does NOT tell: it calls an anonymous
/// pipe a regular file whose size is the bytes waiting in it, and a
/// seek on a pipe "succeeds" while a positional read consumes the pipe
/// (measured on the windows runner, wolf-lang CI run 37062798818, job
/// 111023227162: `fstat0 kind=0 size=19` with standard input a pipe).
#[cfg(windows)]
fn is_disk(f: &File) -> bool {
    use std::os::windows::io::AsRawHandle as _;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileType(h: *mut core::ffi::c_void) -> u32;
    }
    const FILE_TYPE_DISK: u32 = 1;
    // SAFETY: the handle is `f`'s, live for the duration of the call.
    unsafe { GetFileType(f.as_raw_handle()) == FILE_TYPE_DISK }
}

#[cfg(not(windows))]
fn is_disk(_f: &File) -> bool {
    true
}

/// windows has no `ESPIPE` on this path (a pipe or console handle's
/// file pointer is undefined, not refused), so an offset call there
/// asks first: anything that is not a disk file ([`is_disk`]) is
/// `unseekable`, by name. unix asks the host and maps its `ESPIPE`.
#[cfg(windows)]
fn windows_unseekable(f: &File) -> bool {
    !is_disk(f)
}

/// `fs_seek(fd, off, whence) -> int ! {invalid, io, unseekable}` —
/// move the handle's offset and answer the new one, from the start
/// (`[os.fs.seek]`). `whence` 0 is from the start, 1 from the current
/// offset, 2 from the end — POSIX's `SEEK_SET`/`SEEK_CUR`/`SEEK_END`,
/// one `lseek`. A `whence` outside the set is `invalid`, decided before
/// the host is touched; an offset that lands below zero is `invalid`
/// and the cursor does not move; past the end is legal and a read there
/// is `eof`. A pipe, fifo, socket or terminal is `unseekable`; a closed
/// or forged handle is `io`.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_seek(fd: i64, off: i64, whence: i64, out: i64) -> i64 {
    use std::io::{Seek as _, SeekFrom};
    let r = with_handle(fd, |f| {
        let to = match whence {
            0 => match u64::try_from(off) {
                Ok(o) => SeekFrom::Start(o),
                Err(_) => return Err(fs_code::INVALID),
            },
            1 => SeekFrom::Current(off),
            2 => SeekFrom::End(off),
            _ => return Err(fs_code::INVALID),
        };
        #[cfg(windows)]
        {
            if windows_unseekable(f) {
                return Err(fs_code::UNSEEKABLE);
            }
            // ERROR_NEGATIVE_SEEK's mapping is not pinned by std, so
            // the one refusal the clause names is decided here.
            let base = match to {
                SeekFrom::Start(_) => 0,
                SeekFrom::Current(_) => {
                    let mut g = f;
                    g.stream_position().map_err(|e| seek_code(&e))? as i128
                }
                SeekFrom::End(_) => f.metadata().map_err(|e| code_of(&e))?.len() as i128,
            };
            if !matches!(to, SeekFrom::Start(_)) && base + i128::from(off) < 0 {
                return Err(fs_code::INVALID);
            }
        }
        let mut g = f;
        g.seek(to).map_err(|e| seek_code(&e))
    });
    match r {
        None => fs_code::IO,
        Some(Err(code)) => code,
        Some(Ok(at)) => match i64::try_from(at) {
            Ok(at) => {
                unsafe { write_word(out, at) };
                fs_code::OK
            }
            Err(_) => fs_code::IO,
        },
    }
}

/// `fs_tell(fd) -> int ! {io, unseekable}` — the handle's offset from
/// the start, the cursor not moved (`[os.fs.tell]`; `lseek(fd, 0,
/// SEEK_CUR)`). Rows as [`__wolf_rt_fs_seek`]'s.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_tell(fd: i64, out: i64) -> i64 {
    use std::io::Seek as _;
    let r = with_handle(fd, |f| {
        #[cfg(windows)]
        {
            if windows_unseekable(f) {
                return Err(fs_code::UNSEEKABLE);
            }
        }
        // No `invalid` in this row: only `ESPIPE` is told apart.
        let mut g = f;
        g.stream_position().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotSeekable => fs_code::UNSEEKABLE,
            _ => code_of(&e),
        })
    });
    match r {
        None => fs_code::IO,
        Some(Err(code)) => code,
        Some(Ok(at)) => match i64::try_from(at) {
            Ok(at) => {
                unsafe { write_word(out, at) };
                fs_code::OK
            }
            Err(_) => fs_code::IO,
        },
    }
}

/// One positional read of at most `buf.len()` bytes at `off`, the
/// handle's cursor untouched: `pread` on unix; on windows `seek_read`
/// moves the file pointer, so it is put back — under the table lock,
/// so no other hand sees the moved cursor. An interrupted read is
/// retried, never an `io`.
fn read_at(f: &File, buf: &mut [u8], off: u64) -> std::io::Result<usize> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt as _;
        loop {
            match f.read_at(buf, off) {
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                r => return r,
            }
        }
    }
    #[cfg(windows)]
    {
        use std::io::{Seek as _, SeekFrom};
        use std::os::windows::fs::FileExt as _;
        let mut g = f;
        let at = g.stream_position()?;
        let r = f.seek_read(buf, off);
        g.seek(SeekFrom::Start(at))?;
        r
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (f, buf, off);
        Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
    }
}

/// `fs_read_at(fd, off, max) -> List[byte] ! {eof, invalid, io,
/// unseekable}` — `fs_read_chunk` at an offset, the handle's own
/// cursor left exactly where it was (`[os.fs.read_at]`). The order is
/// the family's: the handle first (closed or forged is `io`), then the
/// offset (below zero is `invalid`), then `max` (at or below zero is
/// the empty list), the 1 MiB clamp, and 0 bytes at a positive `max`
/// is `eof` — at or past the end. A pipe, fifo, socket or terminal is
/// `unseekable`.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_read_at(fd: i64, off: i64, max: i64, out: i64) -> i64 {
    let r = with_handle(fd, |f| {
        let Ok(off) = u64::try_from(off) else {
            return Err(fs_code::INVALID);
        };
        if max <= 0 {
            return Ok(Vec::new());
        }
        #[cfg(windows)]
        {
            if windows_unseekable(f) {
                return Err(fs_code::UNSEEKABLE);
            }
        }
        let mut buf = vec![0u8; (max as u64).min(1 << 20) as usize];
        match read_at(f, &mut buf, off) {
            Err(e) => Err(seek_code(&e)),
            Ok(0) => Err(fs_code::EOF),
            Ok(n) => {
                buf.truncate(n);
                Ok(buf)
            }
        }
    });
    // The fd table is released before the list is minted
    // (`fs_read_chunk`'s order).
    match r {
        None => fs_code::IO,
        Some(Err(code)) => code,
        Some(Ok(bytes)) => {
            unsafe { write_bytes_list(out, &bytes) };
            fs_code::OK
        }
    }
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

    /// `[os.fs.remove]` (s163, wolf-lang#365): `fs_remove` on a
    /// DIRECTORY is refused with a row and removes nothing, and WHICH
    /// row is the host's error kind through `code_of` — pinned here per
    /// host, because the corpus witness (`fs/remove_dir_refused.lu`)
    /// can only pin the refusal. A host that stops delegating to its
    /// `unlink`/`DeleteFileW` answer reds here by name.
    #[test]
    fn remove_on_a_directory_is_the_hosts_row() {
        let dir = std::env::temp_dir().join(format!("wolf-rt-fs-rmdir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir_s = dir.display().to_string();
        let (pp, pl) = pair_of(&dir_s);
        let row = unsafe { __wolf_rt_fs_remove(pp, pl) };
        assert!(dir.is_dir(), "fs_remove must not remove a directory");
        #[cfg(target_os = "linux")]
        assert_eq!(
            row,
            fs_code::IO,
            "linux: unlink(2) on a directory is EISDIR -> io"
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            row,
            fs_code::DENIED,
            "macOS: unlink(2) on a directory is EPERM -> denied"
        );
        #[cfg(windows)]
        assert_eq!(
            row,
            fs_code::DENIED,
            "windows: DeleteFileW on a directory is access-denied -> denied"
        );
        assert_ne!(row, fs_code::OK);
        std::fs::remove_dir_all(&dir).unwrap();
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

    /// The octets of a `List[byte]` read back one at a time through the
    /// caller-slot entry — the compiled lanes' `b[i]` shape, so the
    /// exact-capacity mint is checked against the ordinary read path.
    fn list_u8(hdr: i64) -> Vec<u8> {
        let n = unsafe { crate::list::__wolf_rt_list_len(hdr) };
        (0..n)
            .map(|i| {
                let mut cell = [0u8; 1];
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

    /// A list of bytes, the shape a compiled `List[byte]` argument has.
    /// A `List[byte]` of the given octets — pushed one at a time, the
    /// shape compiled code builds (`from_bytes` is the producers'
    /// shape; a consumer must take both).
    fn bytes_list(bs: &[u8]) -> i64 {
        let hdr = crate::list::new_list(1);
        for b in bs {
            crate::list::push_raw(hdr, core::ptr::from_ref(b));
        }
        hdr as i64
    }

    /// A `List[int]` — the WRONG width for a byte consumer (s136): the
    /// one `invalid` a direct FFI caller can still earn.
    fn int_list(bs: &[i64]) -> i64 {
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
            assert_eq!(list_u8(out[0]), vec![0x80, 0, 0xff, 0x41]);
            // Chunked, over a handle, at a boundary that would have
            // split a code point for `fs_read`.
            let fd = __wolf_rt_fs_open(pp, pl, fs_mode::READ);
            assert_eq!(__wolf_rt_fs_read_chunk(fd, 1, o), fs_code::OK);
            assert_eq!(list_u8(out[0]), vec![0x80]);
            assert_eq!(__wolf_rt_fs_read_chunk(fd, 8, o), fs_code::OK);
            assert_eq!(list_u8(out[0]), vec![0, 0xff, 0x41]);
            assert_eq!(__wolf_rt_fs_read_chunk(fd, 8, o), fs_code::EOF);
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            // Not-a-byte-list is `invalid`, on both entry points: since
            // s136 the consumers take `List[byte]`, so the one shape an
            // FFI caller can still get wrong is the element width.
            assert_eq!(
                __wolf_rt_fs_write_bytes(pp, pl, int_list(&[256])),
                fs_code::INVALID
            );
            let fd = __wolf_rt_fs_open(pp, pl, fs_mode::APPEND);
            assert_eq!(
                __wolf_rt_fs_write_chunk(fd, int_list(&[0x2e])),
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
            assert_eq!(list_u8(out[0]), vec![0x80, 0, 0xff, 0x41, 0x2e]);
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

    /// s142 (#261): the stat on an open handle — three words off one
    /// `metadata()`, agreeing with the path stat; a closed and a
    /// forged handle are `io`; a directory handle is kind 1 where the
    /// host opens directories (unix).
    #[test]
    fn fstat_reads_the_open_handle() {
        let dir = scratch("fstat");
        let f = dir.join("f.bin");
        std::fs::write(&f, b"12345").unwrap();
        let fs_ = f.display().to_string();
        let (fp, fl) = pair_of(&fs_);
        let mut out = [0i64; 1];
        let o = out.as_mut_ptr() as i64;
        unsafe {
            let fd = __wolf_rt_fs_open(fp, fl, fs_mode::READ);
            assert!(fd >= 0);
            assert_eq!(__wolf_rt_fs_fstat(fd, o), fs_code::OK);
            let st = crate::list::i64_elems(out[0])
                .expect("a List[int]")
                .to_vec();
            assert_eq!(st.len(), 3);
            assert_eq!(st[0], 0, "kind: a regular file");
            assert_eq!(st[1], 5, "size");
            assert_eq!(__wolf_rt_fs_stat(fp, fl, 1, o), fs_code::OK);
            assert_eq!(st[2], out[0], "mtime agrees with the path stat");
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            assert_eq!(__wolf_rt_fs_fstat(fd, o), fs_code::IO, "closed");
            assert_eq!(__wolf_rt_fs_fstat(1 << 40, o), fs_code::IO, "forged");
            assert_eq!(__wolf_rt_fs_fstat(-1, o), fs_code::IO, "negative");
            if cfg!(unix) {
                let d = dir.display().to_string();
                let (dp, dl) = pair_of(&d);
                let dfd = __wolf_rt_fs_open(dp, dl, fs_mode::READ);
                assert!(dfd >= 0, "unix opens a directory read-only");
                assert_eq!(__wolf_rt_fs_fstat(dfd, o), fs_code::OK);
                let st = crate::list::i64_elems(out[0])
                    .expect("a List[int]")
                    .to_vec();
                assert_eq!(st[0], 1, "kind: a directory");
                assert_eq!(__wolf_rt_fs_close(dfd), fs_code::OK);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// s149 (#289, `[os.fs.open]`): mode 5 is mode 0 that cannot
    /// park. On a REGULAR file the two are indistinguishable — same
    /// handle, same bytes, same `fs_fstat` — and on a FIFO nobody
    /// writes, mode 5 answers a handle at once where mode 0 would
    /// park this test until the harness timed out. The fifo arm is
    /// the whole reason the mode exists (a file server handed a name
    /// under an operator-controlled root), and it is a crate test
    /// rather than a corpus witness because the language has no
    /// `mkfifo`: the witness that DOES run on every tier is
    /// `corpus/fs/open_nonblock.lu`.
    ///
    /// There is no assertion that mode 5 "did not block" — the
    /// assertion is the test finishing. That is the honest shape: a
    /// regression here hangs, which the gauntlet reports.
    #[test]
    fn read_nonblock_matches_read_on_a_file_and_never_parks_on_a_fifo() {
        let dir = scratch("nonblock");
        let f = dir.join("f.txt");
        std::fs::write(&f, b"12345").unwrap();
        let fs_ = f.display().to_string();
        let (fp, fl) = pair_of(&fs_);
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        unsafe {
            let fd = __wolf_rt_fs_open(fp, fl, fs_mode::READ_NONBLOCK);
            assert!(fd >= 0, "a regular file opens as it always did");
            assert_eq!(__wolf_rt_fs_read(fd, 16, o), fs_code::OK);
            assert_eq!(view(out[0], out[1]), "12345", "and reads the same bytes");
            assert_eq!(__wolf_rt_fs_fstat(fd, o), fs_code::OK);
            let st = crate::list::i64_elems(out[0])
                .expect("a List[int]")
                .to_vec();
            assert_eq!(st[0], 0, "kind: a regular file, classified off the handle");
            assert_eq!(st[1], 5, "size");
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            // A missing path is `not_found`, mode 0's row.
            let miss = dir.join("nope").display().to_string();
            let (mp, ml) = pair_of(&miss);
            assert_eq!(
                __wolf_rt_fs_open(mp, ml, fs_mode::READ_NONBLOCK),
                -fs_code::NOT_FOUND
            );
        }
        #[cfg(unix)]
        {
            let fifo = dir.join("pipe");
            let c = std::ffi::CString::new(fifo.display().to_string()).unwrap();
            // SAFETY: a NUL-terminated path in a scratch directory.
            let rc = unsafe { libc::mkfifo(c.as_ptr(), 0o600) };
            assert_eq!(rc, 0, "mkfifo");
            let fs2 = fifo.display().to_string();
            let (pp, pl) = pair_of(&fs2);
            unsafe {
                let fd = __wolf_rt_fs_open(pp, pl, fs_mode::READ_NONBLOCK);
                assert!(fd >= 0, "a writerless fifo answers a handle, at once");
                assert_eq!(__wolf_rt_fs_fstat(fd, o), fs_code::OK);
                let st = crate::list::i64_elems(out[0])
                    .expect("a List[int]")
                    .to_vec();
                assert_eq!(
                    st[0], 2,
                    "kind 2: NOT a regular file — what a server refuses on"
                );
                // The read a server would not make: with NO writer
                // POSIX answers end-of-file (`eof`, measured on both
                // unix hosts), and with a writer that has said
                // nothing the non-blocking read is `EAGAIN` (`io`).
                // Either way the hand comes back — which is the whole
                // claim.
                assert_eq!(__wolf_rt_fs_read(fd, 16, o), fs_code::EOF);
                assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ----------------------------------------------------- s199 --

    /// s199 (#426, `[os.fs.seek]`, `[os.fs.tell]`, `[os.fs.read_at]`):
    /// the offset moved from each end, read back, and left alone by a
    /// positional read; `invalid`, `eof` and `io` where the clauses
    /// put them; the first handle above 2 (`[os.fs.std]`).
    #[test]
    fn seek_tell_and_read_at_on_a_file() {
        let dir = scratch("seek");
        let f = dir.join("f.txt");
        std::fs::write(&f, b"0123456789").unwrap();
        let fs_ = f.display().to_string();
        let (fp, fl) = pair_of(&fs_);
        let mut w = [0i64; 1];
        let o = w.as_mut_ptr() as i64;
        unsafe {
            let fd = __wolf_rt_fs_open(fp, fl, fs_mode::READ);
            assert!(fd >= FIRST_HANDLE as i64, "0..2 are the standard streams");
            assert_eq!(__wolf_rt_fs_read_chunk(fd, 3, o), fs_code::OK);
            assert_eq!(__wolf_rt_fs_tell(fd, o), fs_code::OK);
            assert_eq!(w[0], 3, "tell after a 3-byte read");
            assert_eq!(__wolf_rt_fs_seek(fd, 0, 2, o), fs_code::OK);
            assert_eq!(w[0], 10, "seek to the end answers the size");
            assert_eq!(__wolf_rt_fs_read_chunk(fd, 4, o), fs_code::EOF);
            assert_eq!(__wolf_rt_fs_seek(fd, -3, 2, o), fs_code::OK);
            assert_eq!(w[0], 7);
            assert_eq!(__wolf_rt_fs_read_chunk(fd, 16, o), fs_code::OK);
            assert_eq!(list_u8(w[0]), b"789");
            assert_eq!(__wolf_rt_fs_seek(fd, 2, 0, o), fs_code::OK);
            assert_eq!(__wolf_rt_fs_seek(fd, 3, 1, o), fs_code::OK);
            assert_eq!(w[0], 5, "whence 1 counts from the cursor");
            // The positional read: bytes at 1, the cursor still at 5.
            assert_eq!(__wolf_rt_fs_read_at(fd, 1, 2, o), fs_code::OK);
            assert_eq!(list_u8(w[0]), b"12");
            assert_eq!(__wolf_rt_fs_tell(fd, o), fs_code::OK);
            assert_eq!(w[0], 5, "read_at leaves the cursor alone");
            assert_eq!(__wolf_rt_fs_read_at(fd, 10, 4, o), fs_code::EOF);
            assert_eq!(__wolf_rt_fs_read_at(fd, 3, 0, o), fs_code::OK);
            assert_eq!(list_u8(w[0]), b"", "max 0 is the empty list");
            assert_eq!(__wolf_rt_fs_read_at(fd, -1, 4, o), fs_code::INVALID);
            // Past the end is legal; the read there is eof.
            assert_eq!(__wolf_rt_fs_seek(fd, 4, 2, o), fs_code::OK);
            assert_eq!(w[0], 14);
            assert_eq!(__wolf_rt_fs_read_chunk(fd, 4, o), fs_code::EOF);
            // `invalid`: a whence outside the set, a start below zero,
            // a result below zero — and the cursor does not move.
            assert_eq!(__wolf_rt_fs_seek(fd, 0, 3, o), fs_code::INVALID);
            assert_eq!(__wolf_rt_fs_seek(fd, -1, 0, o), fs_code::INVALID);
            assert_eq!(__wolf_rt_fs_seek(fd, -100, 1, o), fs_code::INVALID);
            assert_eq!(__wolf_rt_fs_tell(fd, o), fs_code::OK);
            assert_eq!(w[0], 14, "a refused seek moves nothing");
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            // Closed, forged and negative handles are `io`, handle first.
            for bad in [fd, 1 << 40, -1] {
                assert_eq!(__wolf_rt_fs_seek(bad, 0, 9, o), fs_code::IO);
                assert_eq!(__wolf_rt_fs_tell(bad, o), fs_code::IO);
                assert_eq!(__wolf_rt_fs_read_at(bad, -1, 4, o), fs_code::IO);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// s199 (#426): a handle with no offset is `unseekable` — a fifo
    /// here (`ESPIPE`), opened with mode 5 so the open does not park.
    #[cfg(unix)]
    #[test]
    fn a_fifo_is_unseekable() {
        let dir = scratch("unseekable");
        let fifo = dir.join("pipe");
        let c = std::ffi::CString::new(fifo.display().to_string()).unwrap();
        // SAFETY: a NUL-terminated path in a scratch directory.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0, "mkfifo");
        let fs_ = fifo.display().to_string();
        let (pp, pl) = pair_of(&fs_);
        let mut w = [0i64; 1];
        let o = w.as_mut_ptr() as i64;
        unsafe {
            let fd = __wolf_rt_fs_open(pp, pl, fs_mode::READ_NONBLOCK);
            assert!(fd >= FIRST_HANDLE as i64);
            assert_eq!(__wolf_rt_fs_seek(fd, 0, 2, o), fs_code::UNSEEKABLE);
            assert_eq!(__wolf_rt_fs_tell(fd, o), fs_code::UNSEEKABLE);
            assert_eq!(__wolf_rt_fs_read_at(fd, 0, 4, o), fs_code::UNSEEKABLE);
            // The order holds on a pipe too: `invalid` before the host.
            assert_eq!(__wolf_rt_fs_seek(fd, 0, 7, o), fs_code::INVALID);
            assert_eq!(__wolf_rt_fs_read_at(fd, -1, 4, o), fs_code::INVALID);
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// s199 (#424, `[os.fs.std]`): 0, 1 and 2 are never a file's
    /// handle — `fs_read`, `fs_write` and `fs_close` answer `io` there
    /// (reading the streams is #405's), and an fs_fstat of each answers
    /// SOMETHING other than the old table miss whenever the test runner
    /// gave the process that descriptor. What each one IS depends on
    /// the runner, so the per-kind answers are the driver gate's
    /// (`fs_std_lanes.rs`, stdin a file and a pipe).
    #[test]
    fn the_standard_streams_are_not_table_slots() {
        let mut w = [0i64; 2];
        let o = w.as_mut_ptr() as i64;
        let (sp, sl) = pair_of("x");
        unsafe {
            for fd in 0..3 {
                assert_eq!(__wolf_rt_fs_read(fd, 1, o), fs_code::IO, "read {fd}");
                assert_eq!(__wolf_rt_fs_write(fd, sp, sl), fs_code::IO, "write {fd}");
                assert_eq!(__wolf_rt_fs_close(fd), fs_code::IO, "close {fd}");
            }
            #[cfg(unix)]
            {
                // stderr is open under every runner this suite meets.
                assert_eq!(__wolf_rt_fs_fstat(2, o), fs_code::OK, "fstat(2)");
                let st = crate::list::i64_elems(w[0]).expect("a List[int]");
                assert!((0..=2).contains(&st[0]), "a kind: {st:?}");
            }
        }
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
