//! wolf-std#156: the bounded inventory calls — `[os.fs.cursor]` and
//! `[os.fs.component]`. A backup inventory must hold a whole tree in a
//! fixed memory ceiling, refuse what it cannot answer, and never follow a
//! source link, so it needs three things the descriptor walk (`fs_at`)
//! does not give it:
//!
//! - **A cursor** over a held directory that answers ONE entry a call and
//!   admits each against the caller's limits (entries, total name bytes,
//!   longest name, records read) BEFORE it copies the name: a directory
//!   past a limit is `budget`, never a list the caller had no say in.
//!   `fs_read_dir_entries_at` reads a whole directory into one list, so a
//!   caller can only check its size after paying for it.
//! - **One component** at a held directory: a name given as raw bytes
//!   (no UTF-8 needed, none repaired), refused `invalid` unless it is
//!   exactly one component — not empty, not `.` or `..`, no `/`, no NUL, at
//!   most [`limits::COMPONENT_MAX`] bytes — before any host call. Its record
//!   never follows a final link; its open never follows one, opens only a
//!   file or a directory (a link, a fifo, a socket or a device is never
//!   opened, so a swapped-in fifo cannot park the caller), and holds the
//!   opened file to the identity (device and inode) the caller expects.
//! - **A link read with a maximum**: the target is read into a buffer of
//!   `max + 1` bytes, so a target of exactly `max` bytes is answered whole
//!   and one byte longer is `budget`, never a shorter target that looks
//!   complete; the link's identity is held before and after the read.
//!
//! # One order for every call
//!
//! 1. **Shape** (`invalid`): a limit below zero, a name that is not one
//!    component, an open word outside the read modes, a maximum outside
//!    `0..=LINK_MAX`. The same answer on every host.
//! 2. **Host** (`unsupported`): windows, by name.
//! 3. **Capacity** (`budget`): the live-cursor ceiling, before the host.
//! 4. **The host's calls**, relative to the held handle: `ENOTDIR` and
//!    `ELOOP` are `invalid` (the descriptor walk's rule), the rest the
//!    family's mapping; a closed or forged handle or cursor is `io` with no
//!    number.
//! 5. **Identity** (`changed`): the device and inode the caller expects,
//!    held before the open or read and again after it.
//!
//! # The cursor's stream
//!
//! A cursor opens its OWN description of the held directory (`openat` of
//! `.` relative to the handle), so two cursors on one handle never share an
//! offset and the handle's own offset is never moved. The stream is the C
//! library's `readdir(3)` over that descriptor: a fixed buffer the library
//! allocates once at `fdopendir(3)` (glibc's is at most a block of the
//! directory, musl's 2 KiB), refilled by `getdents64` on linux and PAX — no
//! allocation that grows with the directory. On macOS the library reads a
//! whole directory only on a union mount; that case is not claimed bounded.
//!
//! An entry's kind is the held directory's no-follow answer for the name
//! (`fstatat` with `AT_SYMLINK_NOFOLLOW`), never the entry's type hint;
//! when that lookup fails the name is still answered, with kind
//! [`limits::UNOBSERVED`], and the caller asks `fs_lstat_component` why.

#[cfg(unix)]
use crate::fs::os_error_record;
use crate::fs::{fs_code, os_error_clear};
#[cfg(unix)]
use crate::str::write_word;

/// The fixed bounds of the family.
pub mod limits {
    /// Cursors open at once in one process; one more is `budget`.
    pub const LIVE_CURSORS: usize = 64;
    /// The longest component, in bytes: `NAME_MAX` on linux, macOS and PAX.
    pub const COMPONENT_MAX: usize = 255;
    /// The largest maximum a link read accepts (its buffer is one more).
    pub const LINK_MAX: i64 = 65536;
    /// The kind byte of an entry whose no-follow lookup failed.
    pub const UNOBSERVED: u8 = 255;
}

/// `budget` (12) and `changed` (13): the family's two codes beside
/// [`fs_code`]'s. Neither coarsens: only the calls that declare them
/// produce them.
pub mod code {
    pub const BUDGET: i64 = 12;
    pub const CHANGED: i64 = 13;
}

/// Is `name` exactly one component: not empty, not `.` or `..`, no `/`,
/// no NUL, at most [`limits::COMPONENT_MAX`] bytes?
pub(crate) fn component_ok(name: &[u8]) -> bool {
    !name.is_empty()
        && name.len() <= limits::COMPONENT_MAX
        && name != b"."
        && name != b".."
        && !name.contains(&b'/')
        && !name.contains(&0)
}

/// The open word `fs_open_component` accepts: a read (0) or a
/// non-blocking read (5), with the no-follow bit (16, implied anyway) and
/// the directory bit (32). `Some((nonblocking, directory))`, or `None`
/// for `invalid`.
pub(crate) fn component_open_word(open: i64) -> Option<(bool, bool)> {
    use crate::fs::fs_mode as m;
    let directory = open & m::DIRECTORY != 0;
    match open & !(m::NOFOLLOW | m::DIRECTORY) {
        m::READ => Some((false, directory)),
        m::READ_NONBLOCK => Some((true, directory)),
        _ => None,
    }
}

/// The component of a `List[byte]` header as a C string, or the code it
/// answers (`invalid`: the wrong element width, or not one component).
#[cfg(unix)]
unsafe fn component_of(hdr: i64) -> Result<std::ffi::CString, i64> {
    let Some(bytes) = (unsafe { crate::list::u8_elems(hdr) }) else {
        return Err(fs_code::INVALID);
    };
    if !component_ok(bytes) {
        return Err(fs_code::INVALID);
    }
    std::ffi::CString::new(bytes).map_err(|_| fs_code::INVALID)
}

/// The shape check alone, for the hosts that answer `unsupported`.
#[cfg(not(unix))]
unsafe fn component_shape(hdr: i64) -> Result<(), i64> {
    match unsafe { crate::list::u8_elems(hdr) } {
        Some(b) if component_ok(b) => Ok(()),
        _ => Err(fs_code::INVALID),
    }
}

// ---------------------------------------------------------- the cursor --

/// Where a cursor's stream stands.
#[cfg(unix)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stream {
    Open,
    /// The end was read: `eof` from here on.
    Ended,
    /// One record past the scan limit was read: `budget` from here on.
    Spent,
    /// The host refused a read: `io` from here on.
    Failed,
}

/// One cursor: the stream, the caller's limits and what it has admitted.
#[cfg(unix)]
struct Cursor {
    dir: *mut libc::DIR,
    max_entries: i64,
    max_bytes: i64,
    max_name: i64,
    max_scan: i64,
    entries: i64,
    bytes: i64,
    scanned: i64,
    stream: Stream,
}

// SAFETY: the stream is only touched under the cursor's own mutex, and a
// `DIR` is not tied to the thread that opened it.
#[cfg(unix)]
unsafe impl Send for Cursor {}

#[cfg(unix)]
impl Drop for Cursor {
    fn drop(&mut self) {
        // SAFETY: the stream this cursor owns, closed once (its descriptor
        // with it).
        unsafe { libc::closedir(self.dir) };
    }
}

/// The live cursors by number. A number is never reused (the counter only
/// goes up), so a closed number misses and answers `io`; at most
/// [`limits::LIVE_CURSORS`] entries, so a lookup is a short scan.
#[cfg(unix)]
struct Cursors {
    next: i64,
    live: Vec<(i64, std::sync::Arc<std::sync::Mutex<Cursor>>)>,
}

#[cfg(unix)]
static CURSORS: std::sync::Mutex<Cursors> = std::sync::Mutex::new(Cursors {
    next: 1,
    live: Vec::new(),
});

#[cfg(unix)]
fn cursors() -> std::sync::MutexGuard<'static, Cursors> {
    CURSORS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(unix)]
fn cursor_of(id: i64) -> Option<std::sync::Arc<std::sync::Mutex<Cursor>>> {
    cursors()
        .live
        .iter()
        .find(|(n, _)| *n == id)
        .map(|(_, c)| std::sync::Arc::clone(c))
}

/// A new description of the directory `dirfd` holds, as a stream: `openat`
/// of `.` (a description of its own, never a duplicate's shared offset),
/// then `fdopendir`.
#[cfg(unix)]
fn stream_of(dirfd: libc::c_int) -> std::io::Result<*mut libc::DIR> {
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    // SAFETY: a live descriptor and a NUL-terminated name.
    let raw = unsafe { libc::openat(dirfd, c".".as_ptr(), flags) };
    if raw < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: a descriptor this call owns; on success the stream owns it.
    let dp = unsafe { libc::fdopendir(raw) };
    if dp.is_null() {
        let e = std::io::Error::last_os_error();
        // SAFETY: still ours: fdopendir failed.
        unsafe { libc::close(raw) };
        return Err(e);
    }
    Ok(dp)
}

/// `fs_dir_cursor_open(dir, max_entries, max_bytes, max_name, max_scan)
/// -> int ! {budget, denied, invalid, io, not_found, unsupported}`
/// (`[os.fs.cursor.open]`) — a cursor over the held directory: at most
/// `max_entries` entries, `max_bytes` name bytes in all, no name longer
/// than `max_name`, at most `max_scan` records read (`.` and `..`
/// included). The cursor's number (>= 1), or `-code`. A limit below zero
/// is `invalid`; the sixty-fifth live cursor `budget`; a handle that is
/// no directory `invalid` (`ENOTDIR`).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_fs_dir_cursor_open(
    dir: i64,
    max_entries: i64,
    max_bytes: i64,
    max_name: i64,
    max_scan: i64,
) -> i64 {
    os_error_clear();
    if [max_entries, max_bytes, max_name, max_scan]
        .iter()
        .any(|&v| v < 0)
    {
        return -fs_code::INVALID;
    }
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd as _;
        // The table held across the open: the ceiling is checked and the
        // cursor inserted under one lock (opening a directory never parks).
        let mut t = cursors();
        if t.live.len() >= limits::LIVE_CURSORS {
            return -code::BUDGET;
        }
        let Some(r) = crate::fs::with_handle(dir, |f| stream_of(f.as_raw_fd())) else {
            return -fs_code::IO;
        };
        let dp = match r {
            Err(e) => return -crate::fs_at::at_code(&e),
            Ok(dp) => dp,
        };
        let id = t.next;
        t.next += 1;
        t.live.push((
            id,
            std::sync::Arc::new(std::sync::Mutex::new(Cursor {
                dir: dp,
                max_entries,
                max_bytes,
                max_name,
                max_scan,
                entries: 0,
                bytes: 0,
                scanned: 0,
                stream: Stream::Open,
            })),
        ));
        id
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        -fs_code::UNSUPPORTED
    }
}

/// One step of a cursor: `Ok(Some((kind, name)))` for an admitted entry,
/// `Ok(None)` at the end, `Err(code)` for `budget` or `io`. The name is
/// checked against every limit while it is still the stream's own bytes;
/// only an admitted one is copied.
#[cfg(unix)]
fn step(c: &mut Cursor) -> Result<Option<(u8, Vec<u8>)>, i64> {
    loop {
        match c.stream {
            Stream::Open => {}
            Stream::Ended => return Ok(None),
            Stream::Spent => return Err(code::BUDGET),
            Stream::Failed => return Err(fs_code::IO),
        }
        crate::fs_at::errno_clear();
        // SAFETY: a live stream, read under the cursor's lock.
        let ent = unsafe { libc::readdir(c.dir) };
        if ent.is_null() {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error().unwrap_or(0) != 0 {
                os_error_record(&e);
                c.stream = Stream::Failed;
                return Err(fs_code::IO);
            }
            c.stream = Stream::Ended;
            return Ok(None);
        }
        c.scanned += 1;
        if c.scanned > c.max_scan {
            c.stream = Stream::Spent;
            return Err(code::BUDGET);
        }
        // SAFETY: the entry the stream answered, valid until the next read;
        // its name is NUL-terminated inside the record.
        let name = unsafe { std::ffi::CStr::from_ptr((*ent).d_name.as_ptr()) };
        let raw = name.to_bytes();
        if raw == b"." || raw == b".." {
            continue;
        }
        let len = raw.len() as i64;
        if len > c.max_name || c.entries >= c.max_entries || len > c.max_bytes - c.bytes {
            return Err(code::BUDGET);
        }
        c.entries += 1;
        c.bytes += len;
        // SAFETY: the stream's own descriptor, live; the name as answered.
        let fd = unsafe { libc::dirfd(c.dir) };
        let kind = match crate::fs_at::stat_words(fd, name, false) {
            Ok(w) => w.identity().0 as u8,
            Err(_) => limits::UNOBSERVED,
        };
        return Ok(Some((kind, raw.to_vec())));
    }
}

/// `fs_dir_cursor_next(cursor) -> List[byte] ! {budget, eof, io}`
/// (`[os.fs.cursor.next]`) — the next admitted entry, `[kind] ++ name
/// bytes`, in the host's order, `.` and `..` left out. An entry past a
/// limit is `budget` and is consumed (the next call reads on); one record
/// past the scan limit is `budget` for good; `eof` is the end and only the
/// end; a read the host refuses is `io` for good. A closed or forged
/// cursor is `io`.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_dir_cursor_next(cursor: i64, out: i64) -> i64 {
    os_error_clear();
    #[cfg(unix)]
    {
        let Some(c) = cursor_of(cursor) else {
            return fs_code::IO;
        };
        let r = step(&mut c.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        match r {
            Err(code) => code,
            Ok(None) => fs_code::EOF,
            Ok(Some((kind, name))) => {
                let (hdr, data) = crate::list::new_list_len(1, name.len() + 1);
                // SAFETY: `data` addresses exactly `name.len() + 1` bytes.
                unsafe {
                    *data = kind;
                    core::ptr::copy_nonoverlapping(name.as_ptr(), data.add(1), name.len());
                    write_word(out, hdr as i64);
                }
                fs_code::OK
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (cursor, out);
        fs_code::IO
    }
}

/// `fs_dir_cursor_close(cursor) -> () ! {io}` (`[os.fs.cursor.close]`) —
/// the cursor's stream closed and its number retired; a closed or forged
/// cursor is `io`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_fs_dir_cursor_close(cursor: i64) -> i64 {
    os_error_clear();
    #[cfg(unix)]
    {
        let mut t = cursors();
        let Some(i) = t.live.iter().position(|(n, _)| *n == cursor) else {
            return fs_code::IO;
        };
        // The stream closes when the last holder lets go (a step running on
        // another task finishes first).
        let _closed = t.live.swap_remove(i);
        fs_code::OK
    }
    #[cfg(not(unix))]
    {
        let _ = cursor;
        fs_code::IO
    }
}

// ------------------------------------------------------- one component --

/// `fs_lstat_component(dir, name) -> List[int] ! {denied, invalid, io,
/// not_found, unsupported}` (`[os.fs.component.lstat]`) — `[os.fs.stat]`'s
/// twenty-word record of ONE component at the held directory, never
/// following a final link (`fs_lstat_at`'s call, the name as bytes).
///
/// # Safety
///
/// `name` must be a live `List[byte]` header; `out` must address 8
/// writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_lstat_component(dir: i64, name: i64, out: i64) -> i64 {
    os_error_clear();
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd as _;
        let c = match unsafe { component_of(name) } {
            Ok(c) => c,
            Err(code) => return code,
        };
        let Some(r) =
            crate::fs::with_handle(dir, |f| crate::fs_at::stat_words(f.as_raw_fd(), &c, false))
        else {
            return fs_code::IO;
        };
        let w = match r {
            Err(e) => return crate::fs_at::at_code(&e),
            Ok(w) => w,
        };
        let Some(rec) = crate::fs_at::record_of(&w) else {
            return fs_code::IO;
        };
        let hdr = crate::list::new_list(8);
        for v in rec {
            crate::list::push_int(hdr, v);
        }
        unsafe { write_word(out, hdr as i64) };
        fs_code::OK
    }
    #[cfg(not(unix))]
    {
        let _ = (dir, out);
        if let Err(code) = unsafe { component_shape(name) } {
            return code;
        }
        fs_code::UNSUPPORTED
    }
}

/// The no-follow kind and identity of `name` at `dirfd`.
#[cfg(unix)]
fn identity_at(dirfd: libc::c_int, name: &std::ffi::CStr) -> std::io::Result<(i64, i64, i64)> {
    crate::fs_at::stat_words(dirfd, name, false).map(|w| w.identity())
}

/// The kind and identity of an open descriptor (`fstat`).
#[cfg(unix)]
fn identity_fd(fd: libc::c_int) -> std::io::Result<(i64, i64, i64)> {
    // SAFETY: `stat` is plain integers; all-zero is a valid value.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: a live descriptor and a writable stat.
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((
        crate::fs_at::kind_of_mode(st.st_mode),
        st.st_dev as i64,
        st.st_ino as i64,
    ))
}

/// `fs_open_component(dir, name, open, dev, ino) -> int ! {changed,
/// denied, invalid, io, not_found, unsupported}`
/// (`[os.fs.component.open]`) — ONE component at the held directory,
/// opened for reading only if it is a file or a directory whose device
/// and inode are `dev` and `ino`: the handle (>= 3), or `-code`. The open
/// word is a read (0) or a non-blocking read (5) with the no-follow bit
/// (16, implied) and the directory bit (32); anything else is `invalid`.
/// A link, a fifo, a socket or a device is `invalid` and never opened; a
/// different identity before the open is `changed`, and so is one after
/// it (the descriptor is closed, nothing read). The open itself is
/// non-blocking; a read word's handle is made blocking once it is held.
///
/// # Safety
///
/// `name` must be a live `List[byte]` header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_open_component(
    dir: i64,
    name: i64,
    open: i64,
    dev: i64,
    ino: i64,
) -> i64 {
    os_error_clear();
    let Some((nonblocking, directory)) = component_open_word(open) else {
        return -fs_code::INVALID;
    };
    #[cfg(unix)]
    {
        use crate::fs::stat_kind;
        use std::os::fd::{AsRawFd as _, FromRawFd as _};
        let c = match unsafe { component_of(name) } {
            Ok(c) => c,
            Err(code) => return -code,
        };
        // The lookup and the open under the table's lock: neither parks (the
        // open is non-blocking, and only a file or a directory is opened).
        let Some(r) = crate::fs::with_handle(dir, |f| -> Result<libc::c_int, i64> {
            let d = f.as_raw_fd();
            let (kind, d0, i0) = identity_at(d, &c).map_err(|e| crate::fs_at::at_code(&e))?;
            if kind != stat_kind::FILE && kind != stat_kind::DIR {
                return Err(fs_code::INVALID);
            }
            if directory && kind != stat_kind::DIR {
                return Err(fs_code::INVALID);
            }
            if (d0, i0) != (dev, ino) {
                return Err(code::CHANGED);
            }
            let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
            if directory {
                flags |= libc::O_DIRECTORY;
            }
            // SAFETY: a live descriptor and a NUL-terminated name.
            let raw = unsafe { libc::openat(d, c.as_ptr(), flags) };
            if raw < 0 {
                let e = std::io::Error::last_os_error();
                // The name was a file or a directory a moment ago: a link or
                // a non-directory found now is a change.
                if matches!(e.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) {
                    os_error_record(&e);
                    return Err(code::CHANGED);
                }
                return Err(crate::fs::code_of(&e));
            }
            Ok(raw)
        }) else {
            return -fs_code::IO;
        };
        let raw = match r {
            Err(code) => return -code,
            Ok(raw) => raw,
        };
        // SAFETY: the descriptor the host just answered, owned from here
        // (closed by the drop on every refusal below).
        let file = unsafe { std::fs::File::from_raw_fd(raw) };
        match identity_fd(raw) {
            Err(e) => return -crate::fs::code_of(&e),
            Ok((kind, d1, i1)) => {
                if (d1, i1) != (dev, ino) || (kind != stat_kind::FILE && kind != stat_kind::DIR) {
                    return -code::CHANGED;
                }
            }
        }
        if !nonblocking {
            // SAFETY: a live descriptor; F_GETFL/F_SETFL take and return ints.
            let fl = unsafe { libc::fcntl(raw, libc::F_GETFL) };
            if fl < 0 || unsafe { libc::fcntl(raw, libc::F_SETFL, fl & !libc::O_NONBLOCK) } < 0 {
                return -crate::fs::code_of(&std::io::Error::last_os_error());
            }
        }
        crate::fs::mint_file(file)
    }
    #[cfg(not(unix))]
    {
        let _ = (dir, dev, ino, nonblocking, directory);
        if let Err(code) = unsafe { component_shape(name) } {
            return -code;
        }
        -fs_code::UNSUPPORTED
    }
}

/// `fs_read_link_component(dir, name, max, dev, ino) -> List[byte] !
/// {budget, changed, denied, invalid, io, not_found, unsupported}`
/// (`[os.fs.component.readlink]`) — the target of the link ONE component
/// names at the held directory, its bytes as stored, read into `max + 1`
/// bytes: a target of `max` bytes or fewer is answered whole, a longer
/// one is `budget`. `max` outside `0..=LINK_MAX` is `invalid`; a name that
/// is no link `invalid`; a link whose device and inode are not `dev` and
/// `ino`, before or after the read, `changed`.
///
/// # Safety
///
/// `name` must be a live `List[byte]` header; `out` must address 8
/// writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_read_link_component(
    dir: i64,
    name: i64,
    max: i64,
    dev: i64,
    ino: i64,
    out: i64,
) -> i64 {
    os_error_clear();
    if !(0..=limits::LINK_MAX).contains(&max) {
        return fs_code::INVALID;
    }
    #[cfg(unix)]
    {
        use crate::fs::stat_kind;
        use std::os::fd::AsRawFd as _;
        let c = match unsafe { component_of(name) } {
            Ok(c) => c,
            Err(code) => return code,
        };
        let Some(r) = crate::fs::with_handle(dir, |f| -> Result<Vec<u8>, i64> {
            let d = f.as_raw_fd();
            let (kind, d0, i0) = identity_at(d, &c).map_err(|e| crate::fs_at::at_code(&e))?;
            if kind != stat_kind::LINK {
                return Err(fs_code::INVALID);
            }
            if (d0, i0) != (dev, ino) {
                return Err(code::CHANGED);
            }
            let cap = max as usize + 1;
            let mut buf = vec![0u8; cap];
            // SAFETY: a live descriptor, a NUL-terminated name and `cap`
            // writable bytes.
            let n = unsafe { libc::readlinkat(d, c.as_ptr(), buf.as_mut_ptr().cast(), cap) };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                // A link a moment ago: no link now is a change.
                if e.raw_os_error() == Some(libc::EINVAL) {
                    os_error_record(&e);
                    return Err(code::CHANGED);
                }
                return Err(crate::fs::code_of(&e));
            }
            match identity_at(d, &c) {
                Ok((k1, d1, i1)) if k1 == stat_kind::LINK && (d1, i1) == (dev, ino) => {}
                Ok(_) => return Err(code::CHANGED),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    os_error_record(&e);
                    return Err(code::CHANGED);
                }
                Err(e) => return Err(crate::fs_at::at_code(&e)),
            }
            let n = n as usize;
            if n > max as usize {
                return Err(code::BUDGET);
            }
            buf.truncate(n);
            Ok(buf)
        }) else {
            return fs_code::IO;
        };
        match r {
            Err(code) => code,
            Ok(target) => {
                unsafe { crate::fs::write_bytes_list(out, &target) };
                fs_code::OK
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (dir, dev, ino, out);
        if let Err(code) = unsafe { component_shape(name) } {
            return code;
        }
        fs_code::UNSUPPORTED
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::fs::{__wolf_rt_fs_close, fs_mode};

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wolf-rt-cursor-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn hold(p: &std::path::Path) -> i64 {
        let s = p.display().to_string();
        // SAFETY: a str pair this test owns.
        let fd = unsafe {
            crate::fs::__wolf_rt_fs_open(
                s.as_ptr() as i64,
                s.len() as i64,
                fs_mode::DIRECTORY | fs_mode::NOFOLLOW,
            )
        };
        assert!(fd >= 3, "a directory opens under both bits: {fd}");
        fd
    }

    fn next(c: i64) -> Result<Vec<u8>, i64> {
        let mut out = [0i64; 1];
        // SAFETY: an 8-byte slot this test owns.
        let rc = unsafe { __wolf_rt_fs_dir_cursor_next(c, out.as_mut_ptr() as i64) };
        if rc != fs_code::OK {
            return Err(rc);
        }
        // SAFETY: the shim minted a List[byte] header.
        Ok(unsafe { crate::list::u8_elems(out[0]) }.unwrap().to_vec())
    }

    fn bytes(b: &[u8]) -> i64 {
        crate::list::from_bytes(b) as i64
    }

    /// The component's shape, on every host: one name, no more.
    #[test]
    fn a_component_is_one_name() {
        let ok: [&[u8]; 5] = [b"a", b"...", b"a b", &[255], &[b'x'; 255]];
        for name in ok {
            assert!(component_ok(name), "{name:?}");
        }
        let bad: [&[u8]; 7] = [b"", b".", b"..", b"a/b", b"/", b"a\0", &[b'x'; 256]];
        for name in bad {
            assert!(!component_ok(name), "{name:?}");
        }
        assert_eq!(component_open_word(0), Some((false, false)));
        assert_eq!(component_open_word(48), Some((false, true)));
        assert_eq!(component_open_word(21), Some((true, false)));
        for bad in [1, 2, 3, 4, 6, 64, -1] {
            assert_eq!(component_open_word(bad), None, "{bad}");
        }
    }

    /// Each limit admits exactly its value and refuses one past it before
    /// the name is copied; the scan limit is sticky; `eof` only at the end.
    #[test]
    fn the_limits_admit_at_and_refuse_one_past() {
        let d = scratch("limits");
        std::fs::write(d.join("a"), b"x").unwrap();
        std::fs::write(d.join("bb"), b"y").unwrap();
        let h = hold(&d);
        let c = __wolf_rt_fs_dir_cursor_open(h, 2, 3, 2, 8);
        assert!(c >= 1, "{c}");
        let mut got = vec![next(c).unwrap(), next(c).unwrap()];
        got.sort();
        assert_eq!(got, vec![b"\0a".to_vec(), b"\0bb".to_vec()]);
        assert_eq!(next(c), Err(fs_code::EOF));
        assert_eq!(next(c), Err(fs_code::EOF));
        assert_eq!(__wolf_rt_fs_dir_cursor_close(c), fs_code::OK);
        // Two bytes in all: "a" fits, "bb" would make three.
        let c = __wolf_rt_fs_dir_cursor_open(h, 2, 2, 2, 8);
        let mut seen = Vec::new();
        for _ in 0..3 {
            seen.push(next(c));
        }
        assert!(seen.contains(&Ok(b"\0a".to_vec())) || seen.contains(&Ok(b"\0bb".to_vec())));
        assert!(seen.contains(&Err(code::BUDGET)), "{seen:?}");
        assert_eq!(next(c), Err(fs_code::EOF));
        assert_eq!(__wolf_rt_fs_dir_cursor_close(c), fs_code::OK);
        let c = __wolf_rt_fs_dir_cursor_open(h, 2, 3, 2, 0);
        assert_eq!(next(c), Err(code::BUDGET));
        assert_eq!(next(c), Err(code::BUDGET));
        assert_eq!(__wolf_rt_fs_dir_cursor_close(c), fs_code::OK);
        assert_eq!(__wolf_rt_fs_dir_cursor_close(c), fs_code::IO);
        assert_eq!(next(c), Err(fs_code::IO));
        assert_eq!(
            __wolf_rt_fs_dir_cursor_open(h, -1, 0, 0, 0),
            -fs_code::INVALID
        );
        assert_eq!(
            __wolf_rt_fs_dir_cursor_open(1_000_000, 1, 1, 1, 1),
            -fs_code::IO
        );
        let _ = __wolf_rt_fs_close(h);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The record, the open and the link read of one component, through a
    /// handle whose path was moved; identity held; shapes refused first.
    #[test]
    fn one_component_by_identity() {
        use std::os::unix::fs::MetadataExt as _;
        let d = scratch("component");
        std::fs::create_dir(d.join("in")).unwrap();
        std::fs::write(d.join("in/f"), b"wolves").unwrap();
        std::os::unix::fs::symlink("abcd", d.join("in/l")).unwrap();
        let h = hold(&d.join("in"));
        std::fs::rename(d.join("in"), d.join("moved")).unwrap();
        let fm = std::fs::symlink_metadata(d.join("moved/f")).unwrap();
        let lm = std::fs::symlink_metadata(d.join("moved/l")).unwrap();
        let (fdev, fino) = (fm.dev() as i64, fm.ino() as i64);
        let (ldev, lino) = (lm.dev() as i64, lm.ino() as i64);
        let mut out = [0i64; 1];
        let slot = out.as_mut_ptr() as i64;
        unsafe {
            assert_eq!(
                __wolf_rt_fs_lstat_component(h, bytes(b"f"), slot),
                fs_code::OK
            );
            let rec = crate::list::i64_elems(out[0]).unwrap().to_vec();
            assert_eq!(
                rec,
                crate::fs::stat_record(&fm).unwrap().to_vec(),
                "the path's record"
            );
            assert_eq!(
                __wolf_rt_fs_lstat_component(h, bytes(b".."), slot),
                fs_code::INVALID
            );
            assert_eq!(
                __wolf_rt_fs_lstat_component(h, bytes(b"x/f"), slot),
                fs_code::INVALID
            );
            assert_eq!(
                __wolf_rt_fs_lstat_component(h, bytes(b"zz"), slot),
                fs_code::NOT_FOUND
            );
            let fd = __wolf_rt_fs_open_component(h, bytes(b"f"), 0, fdev, fino);
            assert!(fd >= 3, "{fd}");
            let _ = __wolf_rt_fs_close(fd);
            assert_eq!(
                __wolf_rt_fs_open_component(h, bytes(b"f"), 0, fdev, fino + 1),
                -code::CHANGED
            );
            assert_eq!(
                __wolf_rt_fs_open_component(h, bytes(b"l"), 0, ldev, lino),
                -fs_code::INVALID
            );
            assert_eq!(
                __wolf_rt_fs_open_component(h, bytes(b"f"), 1, fdev, fino),
                -fs_code::INVALID
            );
            assert_eq!(
                __wolf_rt_fs_read_link_component(h, bytes(b"l"), 4, ldev, lino, slot),
                fs_code::OK
            );
            assert_eq!(crate::list::u8_elems(out[0]).unwrap(), b"abcd");
            assert_eq!(
                __wolf_rt_fs_read_link_component(h, bytes(b"l"), 3, ldev, lino, slot),
                code::BUDGET
            );
            assert_eq!(
                __wolf_rt_fs_read_link_component(h, bytes(b"l"), 4, ldev, lino + 1, slot),
                code::CHANGED
            );
            assert_eq!(
                __wolf_rt_fs_read_link_component(h, bytes(b"f"), 4, fdev, fino, slot),
                fs_code::INVALID
            );
            assert_eq!(
                __wolf_rt_fs_read_link_component(
                    h,
                    bytes(b"l"),
                    limits::LINK_MAX + 1,
                    ldev,
                    lino,
                    slot
                ),
                fs_code::INVALID
            );
        }
        let _ = __wolf_rt_fs_close(h);
        let _ = std::fs::remove_dir_all(&d);
    }
}
