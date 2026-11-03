//! s277 (wolf-lang#707, #1029, #1030, #1031, #1033): the descriptor walk —
//! calls relative to a directory the program HOLDS, and the mode set on a
//! handle. `[os.fs.at]`, `[os.fs.at.open]`, `[os.fs.at.stat]`,
//! `[os.fs.at.lstat]`, `[os.fs.at.remove]`, `[os.fs.at.remove.dir]`,
//! `[os.fs.at.mkdir]`, `[os.fs.at.symlink]`, `[os.fs.at.link]`,
//! `[os.fs.at.readdir]`, `[os.fs.mode.fd]`. (The open that refuses a
//! non-directory is a flag of the open itself, [`crate::fs::fs_mode::DIRECTORY`].)
//!
//! A module of its own, as `fs_gaps.rs` is: it shares [`crate::fs`]'s handle
//! table, error codes and host-error word and nothing else. A directory
//! handle is a handle of the same table as a file's: what `fs_open_mode`
//! (or [`__wolf_rt_fs_open_at`]) answered for a directory, best opened with
//! the directory and no-follow bits.
//!
//! # One order for every call (`[os.fs.change.note]`'s)
//!
//! 1. **Shape** (`invalid`): an empty name or an absolute one (an absolute
//!    name would ignore the handle — the escape the family exists to close),
//!    an open word, a mode or a permission outside its set, an empty link
//!    target. The same answer on every host.
//! 2. **Host** (`unsupported`): windows, by name — no handle of the table
//!    is a directory there, and Win32's create has no directory-relative
//!    form.
//! 3. **The host's one call**, relative to the handle, its error through the
//!    family's mapping — except `ENOTDIR` (the handle is no directory, a
//!    middle component is a file, the directory bit refused a file) and
//!    `ELOOP` (the no-follow bit refused a final link, or a loop on the
//!    way), which are `invalid` on every call here, the number in
//!    `os_error()`. A closed or forged handle is `io` with no number.
//!
//! The calls that cannot park (the stats, the removals, the creations, the
//! links, `fchmod`) run on the table's descriptor under the table's lock, as
//! `fs_set_owner_fd` does; the open, which can (a fifo opened without the
//! non-blocking mode waits for a writer), and the listing, which reads a
//! whole directory, run on a duplicate of the handle with the lock released.

use crate::fs::{fs_code, os_error_clear};
#[cfg(unix)]
use crate::fs::{os_error_record, stat_kind, stat_word, with_handle};
use crate::str::view;
#[cfg(unix)]
use crate::str::write_word;

/// The shape of a name relative to a handle (step 1): `Err(INVALID)` for an
/// empty name or an absolute one (a leading `/`), on every host.
fn name_shape(name: &str) -> Result<(), i64> {
    if name.is_empty() || name.starts_with('/') {
        Err(fs_code::INVALID)
    } else {
        Ok(())
    }
}

/// The name as a C string, or the code it answers: the shape's `invalid`,
/// then a NUL inside it `io` (the path family's rule).
#[cfg(unix)]
fn c_name(name: &str) -> Result<std::ffi::CString, i64> {
    name_shape(name)?;
    std::ffi::CString::new(name).map_err(|_| fs_code::IO)
}

/// One open word of `[os.fs.open]` with its two flags, decoded: the mode,
/// no-follow, directory. `None` is `invalid`: a mode outside the six, a bit
/// above 32, the directory bit beside a mode that is not a read.
fn open_word(open: i64) -> Option<(i64, bool, bool)> {
    use crate::fs::fs_mode as m;
    let nofollow = open & m::NOFOLLOW != 0;
    let directory = open & m::DIRECTORY != 0;
    let mode = open & !(m::NOFOLLOW | m::DIRECTORY);
    if !(m::READ..=m::READ_NONBLOCK).contains(&mode) {
        return None;
    }
    if directory && mode != m::READ && mode != m::READ_NONBLOCK {
        return None;
    }
    Some((mode, nofollow, directory))
}

/// The `open(2)` flags of a decoded open word — `[os.fs.open]`'s six modes as
/// `OpenOptions` makes them, close-on-exec as every runtime descriptor is.
#[cfg(unix)]
fn open_flags(mode: i64, nofollow: bool, directory: bool) -> libc::c_int {
    use crate::fs::fs_mode as m;
    let mut f = match mode {
        m::READ => libc::O_RDONLY,
        m::WRITE => libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
        m::APPEND => libc::O_WRONLY | libc::O_APPEND | libc::O_CREAT,
        m::READ_WRITE => libc::O_RDWR | libc::O_CREAT,
        m::CREATE_NEW => libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
        _ => libc::O_RDONLY | libc::O_NONBLOCK,
    };
    f |= libc::O_CLOEXEC;
    if nofollow {
        f |= libc::O_NOFOLLOW;
    }
    if directory {
        f |= libc::O_DIRECTORY;
    }
    f
}

/// A call's host refusal (step 3): `ENOTDIR` and `ELOOP` are `invalid`, the
/// number recorded; anything else the family's mapping.
#[cfg(unix)]
pub(crate) fn at_code(e: &std::io::Error) -> i64 {
    if matches!(e.raw_os_error(), Some(libc::ENOTDIR | libc::ELOOP)) {
        os_error_record(e);
        return fs_code::INVALID;
    }
    crate::fs::code_of(e)
}

/// `Ok(())` for a zero return, else the host's error, read at once.
#[cfg(unix)]
fn rc_of(rc: libc::c_int) -> std::io::Result<()> {
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// `fs_open_at(dir, name, open, perm) -> int ! {denied, exists, invalid, io,
/// not_found, unsupported}` (`[os.fs.at.open]`) — `openat(2)`: `name`
/// opened relative to the directory the handle holds, in one of
/// `[os.fs.open]`'s six modes with its two flags (16 no final link
/// followed, 32 a directory only), a creation given `perm` less the umask
/// (`fs_open_perm`'s rule). The handle (>= 3), or `-code`. Shape: the open
/// word, then `perm` outside `0..=0o777`, then the name; under the
/// no-follow bit freebsd's `EMLINK` is `invalid` too (`fs_open_mode`'s
/// rule).
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_open_at(
    dir: i64,
    np: i64,
    nl: i64,
    open: i64,
    perm: i64,
) -> i64 {
    os_error_clear();
    let Some((mode, nofollow, directory)) = open_word(open) else {
        return -fs_code::INVALID;
    };
    if !(0..=crate::fs_change::PERM_MAX).contains(&perm) {
        return -fs_code::INVALID;
    }
    let name = unsafe { view(np, nl) };
    #[cfg(unix)]
    {
        use std::os::fd::{AsRawFd as _, FromRawFd as _};
        let c = match c_name(name) {
            Ok(c) => c,
            Err(code) => return -code,
        };
        // The open may park (a fifo, no writer): a duplicate, the lock released.
        let Some(d) = crate::fs::dup_of(dir) else {
            return -fs_code::IO;
        };
        let flags = open_flags(mode, nofollow, directory);
        // SAFETY: a live descriptor (the duplicate) and a NUL-terminated name.
        let raw = unsafe { libc::openat(d.as_raw_fd(), c.as_ptr(), flags, perm as libc::c_uint) };
        let e = std::io::Error::last_os_error();
        drop(d);
        if raw < 0 {
            if matches!(e.raw_os_error(), Some(libc::ENOTDIR | libc::ELOOP)) {
                return -at_code(&e);
            }
            return -crate::fs::open_code(&e, open);
        }
        // SAFETY: the descriptor the host just answered, owned from here.
        crate::fs::mint_file(unsafe { std::fs::File::from_raw_fd(raw) })
    }
    #[cfg(not(unix))]
    {
        let _ = (dir, mode, nofollow, directory);
        if let Err(code) = name_shape(name) {
            return -code;
        }
        -fs_code::UNSUPPORTED
    }
}

/// Milliseconds from the Unix epoch for a `timespec`, as `fs_modified_ms`
/// answers a `SystemTime` (`[os.fs.stat]`'s word 2): a whole count, toward
/// zero on either side of the epoch. `None` outside `i64`.
#[cfg(unix)]
fn ms_of(sec: i64, nsec: i64) -> Option<i64> {
    let total = i128::from(sec) * 1_000_000_000 + i128::from(nsec);
    let ms = if total >= 0 {
        total / 1_000_000
    } else {
        -((-total) / 1_000_000)
    };
    i64::try_from(ms).ok()
}

/// The record's kind for a `st_mode`'s file type bits.
#[cfg(unix)]
fn kind_of_mode(mode: u32) -> i64 {
    match mode & (libc::S_IFMT as u32) {
        m if m == libc::S_IFREG as u32 => stat_kind::FILE,
        m if m == libc::S_IFDIR as u32 => stat_kind::DIR,
        m if m == libc::S_IFLNK as u32 => stat_kind::LINK,
        m if m == libc::S_IFIFO as u32 => stat_kind::FIFO,
        m if m == libc::S_IFSOCK as u32 => stat_kind::SOCKET,
        m if m == libc::S_IFCHR as u32 => stat_kind::CHAR_DEVICE,
        m if m == libc::S_IFBLK as u32 => stat_kind::BLOCK_DEVICE,
        _ => stat_kind::OTHER,
    }
}

/// The host's words of one stat, before they are a record.
#[cfg(unix)]
struct Words {
    mode: u32,
    size: i64,
    nlink: i64,
    uid: i64,
    gid: i64,
    blocks: i64,
    dev: i64,
    ino: i64,
    rdev: i64,
    atime: (i64, i64),
    mtime: (i64, i64),
    ctime: (i64, i64),
    btime: Option<(i64, i64)>,
}

/// `[os.fs.stat]`'s twenty words from a stat's words — [`crate::fs::stat_record`]'s
/// words, kinds, units and `have` bits, built from the host's integers where
/// that one is built from a `Metadata`. `None` is the family's `io`: a
/// modification time outside `i64` milliseconds.
#[cfg(unix)]
fn record_of(w: &Words) -> Option<[i64; stat_word::WORDS]> {
    use stat_word as s;
    let mut r = [0i64; s::WORDS];
    r[s::KIND] = kind_of_mode(w.mode);
    r[s::SIZE] = w.size;
    r[s::MODIFIED_MS] = ms_of(w.mtime.0, w.mtime.1)?;
    r[s::MODE] = i64::from(w.mode & 0o7777);
    r[s::NLINK] = w.nlink;
    r[s::UID] = w.uid;
    r[s::GID] = w.gid;
    r[s::BLOCKS] = w.blocks;
    r[s::DEV] = w.dev;
    r[s::INO] = w.ino;
    r[s::RDEV] = w.rdev;
    (r[s::ATIME_S], r[s::ATIME_NS]) = w.atime;
    (r[s::MTIME_S], r[s::MTIME_NS]) = w.mtime;
    (r[s::CTIME_S], r[s::CTIME_NS]) = w.ctime;
    // Every word through ctime's nanoseconds is the host's on unix.
    let mut have: i64 = (1 << (s::CTIME_NS + 1)) - 1;
    if let Some((bs, bn)) = w.btime {
        r[s::BTIME_S] = bs;
        r[s::BTIME_NS] = bn;
        have |= (1 << s::BTIME_S) | (1 << s::BTIME_NS);
    }
    r[s::HAVE] = have;
    Some(r)
}

/// One `fstatat(2)`'s words (`AT_SYMLINK_NOFOLLOW` unless `follow`); the
/// birth time where the host's `stat` carries one (macOS, freebsd).
#[cfg(unix)]
fn fstatat_words(
    dirfd: libc::c_int,
    name: &std::ffi::CStr,
    follow: bool,
) -> std::io::Result<Words> {
    // SAFETY: `stat` is plain integers; all-zero is a valid value.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    let flags = if follow { 0 } else { libc::AT_SYMLINK_NOFOLLOW };
    // SAFETY: a live descriptor, a NUL-terminated name, a writable stat.
    rc_of(unsafe { libc::fstatat(dirfd, name.as_ptr(), &mut st, flags) })?;
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
    let btime = Some((st.st_birthtime as i64, st.st_birthtime_nsec as i64));
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "freebsd")))]
    let btime = None;
    Ok(Words {
        mode: st.st_mode as u32,
        size: st.st_size as i64,
        nlink: st.st_nlink as i64,
        uid: i64::from(st.st_uid),
        gid: i64::from(st.st_gid),
        blocks: st.st_blocks as i64,
        dev: st.st_dev as i64,
        ino: st.st_ino as i64,
        rdev: st.st_rdev as i64,
        atime: (st.st_atime as i64, st.st_atime_nsec as i64),
        mtime: (st.st_mtime as i64, st.st_mtime_nsec as i64),
        ctime: (st.st_ctime as i64, st.st_ctime_nsec as i64),
        btime,
    })
}

/// linux's `statx(2)` record, laid out from the man page's field list (its
/// padding omitted there: natural alignment, 256 bytes); measured against
/// `fstatat(2)` word for word on hasu (`notes/s277-evidence/probe/`). The C
/// library's own declaration is absent from the musl binding, so the call
/// goes through `syscall(2)`.
#[cfg(any(target_os = "linux", target_os = "android"))]
#[repr(C)]
#[derive(Default)]
struct Statx {
    mask: u32,
    blksize: u32,
    attributes: u64,
    nlink: u32,
    uid: u32,
    gid: u32,
    mode: u16,
    pad0: u16,
    ino: u64,
    size: u64,
    blocks: u64,
    attributes_mask: u64,
    atime: StatxTime,
    btime: StatxTime,
    ctime: StatxTime,
    mtime: StatxTime,
    rdev_major: u32,
    rdev_minor: u32,
    dev_major: u32,
    dev_minor: u32,
    rest: [u64; 14],
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct StatxTime {
    sec: i64,
    nsec: u32,
    pad: i32,
}

/// `statx(2)`'s mask words (`STATX_BASIC_STATS`, `STATX_BTIME`) and the
/// flag that makes it act as `stat(2)` does at an automount point.
#[cfg(any(target_os = "linux", target_os = "android"))]
const STATX_BASIC_STATS: u32 = 0x7ff;
#[cfg(any(target_os = "linux", target_os = "android"))]
const STATX_BTIME: u32 = 0x800;

/// One `statx(2)` relative to `dirfd`, the birth time where the file system
/// keeps one; `fstatat(2)` where the kernel refuses the call (`ENOSYS`, or
/// `EPERM` from a filter that predates it).
#[cfg(any(target_os = "linux", target_os = "android"))]
fn stat_words(dirfd: libc::c_int, name: &std::ffi::CStr, follow: bool) -> std::io::Result<Words> {
    let mut x = Statx::default();
    let mut flags = libc::AT_NO_AUTOMOUNT;
    if !follow {
        flags |= libc::AT_SYMLINK_NOFOLLOW;
    }
    // SAFETY: a live descriptor, a NUL-terminated name and a 256-byte
    // writable record the kernel fills.
    let rc = unsafe {
        libc::syscall(
            libc::SYS_statx,
            dirfd,
            name.as_ptr(),
            flags,
            STATX_BASIC_STATS | STATX_BTIME,
            &raw mut x,
        )
    };
    if rc != 0 {
        let e = std::io::Error::last_os_error();
        if matches!(e.raw_os_error(), Some(libc::ENOSYS | libc::EPERM)) {
            return fstatat_words(dirfd, name, follow);
        }
        return Err(e);
    }
    let t = |s: StatxTime| (s.sec, i64::from(s.nsec));
    Ok(Words {
        mode: u32::from(x.mode),
        size: x.size as i64,
        nlink: i64::from(x.nlink),
        uid: i64::from(x.uid),
        gid: i64::from(x.gid),
        blocks: x.blocks as i64,
        dev: libc::makedev(x.dev_major, x.dev_minor) as i64,
        ino: x.ino as i64,
        rdev: libc::makedev(x.rdev_major, x.rdev_minor) as i64,
        atime: t(x.atime),
        mtime: t(x.mtime),
        ctime: t(x.ctime),
        btime: (x.mask & STATX_BTIME != 0).then(|| t(x.btime)),
    })
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
fn stat_words(dirfd: libc::c_int, name: &std::ffi::CStr, follow: bool) -> std::io::Result<Words> {
    fstatat_words(dirfd, name, follow)
}

/// `fs_stat_at(dir, name)` (follow = 1) / `fs_lstat_at(dir, name)` (follow
/// = 0) `-> List[int] ! {denied, invalid, io, not_found, unsupported}`
/// (`[os.fs.at.stat]`, `[os.fs.at.lstat]`) — `[os.fs.stat]`'s twenty-word
/// record of `name` relative to the held directory, from ONE host call
/// (`statx` on linux, `fstatat` elsewhere): `fs_stat_at` follows a final
/// link, `fs_lstat_at` answers the link itself.
///
/// # Safety
///
/// A valid str pair; `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_stat_at(
    dir: i64,
    np: i64,
    nl: i64,
    follow: i64,
    out: i64,
) -> i64 {
    os_error_clear();
    let name = unsafe { view(np, nl) };
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd as _;
        let c = match c_name(name) {
            Ok(c) => c,
            Err(code) => return code,
        };
        let Some(r) = with_handle(dir, |f| stat_words(f.as_raw_fd(), &c, follow != 0)) else {
            return fs_code::IO;
        };
        let w = match r {
            Err(e) => return at_code(&e),
            Ok(w) => w,
        };
        let Some(rec) = record_of(&w) else {
            return fs_code::IO;
        };
        // The table released, the list is minted (`fs_fstat`'s order).
        let hdr = crate::list::new_list(8);
        for v in rec {
            crate::list::push_int(hdr, v);
        }
        unsafe { write_word(out, hdr as i64) };
        fs_code::OK
    }
    #[cfg(not(unix))]
    {
        let _ = (dir, follow, out);
        if let Err(code) = name_shape(name) {
            return code;
        }
        fs_code::UNSUPPORTED
    }
}

/// `fs_remove_at(dir, name)` (rmdir = 0) / `fs_remove_dir_at(dir, name)`
/// (rmdir = 1) `-> () ! {denied, invalid, io, not_found, unsupported}`
/// (`[os.fs.at.remove]`, `[os.fs.at.remove.dir]`) — `unlinkat(2)`, plain or
/// with `AT_REMOVEDIR`. A directory given to the plain form is the host's
/// row, `fs_remove`'s split (linux `io`, macOS `denied`); a file or a link
/// given to the directory form is `ENOTDIR`, `invalid`; a directory that is
/// not empty `io`.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_remove_at(dir: i64, np: i64, nl: i64, rmdir: i64) -> i64 {
    os_error_clear();
    let name = unsafe { view(np, nl) };
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd as _;
        let c = match c_name(name) {
            Ok(c) => c,
            Err(code) => return code,
        };
        let flags = if rmdir != 0 { libc::AT_REMOVEDIR } else { 0 };
        let Some(r) = with_handle(dir, |f| {
            // SAFETY: a live descriptor (under the table's lock) and a name.
            rc_of(unsafe { libc::unlinkat(f.as_raw_fd(), c.as_ptr(), flags) })
        }) else {
            return fs_code::IO;
        };
        match r {
            Err(e) => at_code(&e),
            Ok(()) => fs_code::OK,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (dir, rmdir);
        if let Err(code) = name_shape(name) {
            return code;
        }
        fs_code::UNSUPPORTED
    }
}

/// `fs_create_dir_at(dir, name, mode) -> () ! {denied, exists, invalid, io,
/// not_found, unsupported}` (`[os.fs.at.mkdir]`) — `mkdirat(2)`: one
/// directory under the held one, `mode` less the umask from its first
/// instant (`fs_create_dir_mode`'s rule); `mode` outside `0..=0o777` is
/// `invalid`, then the name's shape.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_create_dir_at(dir: i64, np: i64, nl: i64, mode: i64) -> i64 {
    os_error_clear();
    if !(0..=crate::fs_change::PERM_MAX).contains(&mode) {
        return fs_code::INVALID;
    }
    let name = unsafe { view(np, nl) };
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd as _;
        let c = match c_name(name) {
            Ok(c) => c,
            Err(code) => return code,
        };
        let Some(r) = with_handle(dir, |f| {
            // SAFETY: a live descriptor (under the table's lock) and a name.
            rc_of(unsafe { libc::mkdirat(f.as_raw_fd(), c.as_ptr(), mode as libc::mode_t) })
        }) else {
            return fs_code::IO;
        };
        match r {
            Err(e) => at_code(&e),
            Ok(()) => fs_code::OK,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        if let Err(code) = name_shape(name) {
            return code;
        }
        fs_code::UNSUPPORTED
    }
}

/// `fs_symlink_at(target, dir, name) -> () ! {denied, exists, invalid, io,
/// not_found, unsupported}` (`[os.fs.at.symlink]`) — `symlinkat(2)`: `name`
/// under the held directory becomes a symbolic link whose stored target is
/// `target`, VERBATIM (`fs_symlink`'s rule: never resolved, an empty target
/// `invalid` before the host — linux refuses one and macOS makes it,
/// measured), then the name's shape. A NUL in the target is `io`.
///
/// # Safety
///
/// Two valid str pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_symlink_at(
    tp: i64,
    tl: i64,
    dir: i64,
    np: i64,
    nl: i64,
) -> i64 {
    os_error_clear();
    let target = unsafe { view(tp, tl) };
    if target.is_empty() {
        return fs_code::INVALID;
    }
    let name = unsafe { view(np, nl) };
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd as _;
        let c = match c_name(name) {
            Ok(c) => c,
            Err(code) => return code,
        };
        let Ok(t) = std::ffi::CString::new(target) else {
            return fs_code::IO;
        };
        let Some(r) = with_handle(dir, |f| {
            // SAFETY: a live descriptor (under the table's lock), two names.
            rc_of(unsafe { libc::symlinkat(t.as_ptr(), f.as_raw_fd(), c.as_ptr()) })
        }) else {
            return fs_code::IO;
        };
        match r {
            Err(e) => at_code(&e),
            Ok(()) => fs_code::OK,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        if let Err(code) = name_shape(name) {
            return code;
        }
        fs_code::UNSUPPORTED
    }
}

/// `fs_link_at(src_dir, src, dst_dir, dst, follow) -> () ! {cross_device,
/// denied, exists, invalid, io, not_found, unsupported}`
/// (`[os.fs.at.link]`) — `linkat(2)` between two held directories: `dst`
/// under `dst_dir` becomes a second name of the file `src` names under
/// `src_dir`. `follow` false links a final symbolic link itself (no flag,
/// `fs_link`'s rule), true what it names (`AT_SYMLINK_FOLLOW`, wolf-lang#910).
/// It never replaces (`exists`); two file systems are `cross_device`; a
/// directory `denied`. The two names' shapes, source first.
///
/// # Safety
///
/// Two valid str pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_link_at(
    sd: i64,
    sp: i64,
    sl: i64,
    dd: i64,
    dp: i64,
    dl: i64,
    follow: i64,
) -> i64 {
    os_error_clear();
    let (src, dst) = unsafe { (view(sp, sl), view(dp, dl)) };
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd as _;
        let s = match c_name(src) {
            Ok(c) => c,
            Err(code) => return code,
        };
        let d = match c_name(dst) {
            Ok(c) => c,
            Err(code) => return code,
        };
        // Two handles, one table lock: the source is duplicated first.
        let Some(sdup) = crate::fs::dup_of(sd) else {
            return fs_code::IO;
        };
        let flags = if follow != 0 {
            libc::AT_SYMLINK_FOLLOW
        } else {
            0
        };
        let Some(r) = with_handle(dd, |f| {
            // SAFETY: two live descriptors and two names.
            rc_of(unsafe {
                libc::linkat(
                    sdup.as_raw_fd(),
                    s.as_ptr(),
                    f.as_raw_fd(),
                    d.as_ptr(),
                    flags,
                )
            })
        }) else {
            return fs_code::IO;
        };
        match r {
            Err(e) => at_code(&e),
            Ok(()) => fs_code::OK,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (sd, dd, follow);
        if let Err(code) = name_shape(src).and_then(|()| name_shape(dst)) {
            return code;
        }
        fs_code::UNSUPPORTED
    }
}

/// Clear the C library's `errno`, so `readdir(3)`'s end and its failure can
/// be told apart (both answer a null entry).
#[cfg(unix)]
fn errno_clear() {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    // SAFETY: the calling thread's own errno word.
    unsafe {
        *libc::__errno_location() = 0;
    }
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
    // SAFETY: as above.
    unsafe {
        *libc::__error() = 0;
    }
}

/// The record's kind for a directory entry's `d_type`; `None` where the
/// entry carries none (`DT_UNKNOWN`).
#[cfg(unix)]
fn kind_of_dtype(t: u8) -> Option<i64> {
    match t {
        libc::DT_REG => Some(stat_kind::FILE),
        libc::DT_DIR => Some(stat_kind::DIR),
        libc::DT_LNK => Some(stat_kind::LINK),
        libc::DT_FIFO => Some(stat_kind::FIFO),
        libc::DT_SOCK => Some(stat_kind::SOCKET),
        libc::DT_CHR => Some(stat_kind::CHAR_DEVICE),
        libc::DT_BLK => Some(stat_kind::BLOCK_DEVICE),
        libc::DT_UNKNOWN => None,
        _ => Some(stat_kind::OTHER),
    }
}

/// The entries of the directory `d` (a duplicate of the held handle, owned
/// and closed here): rewound, read to the end with `readdir(3)`, `.` and
/// `..` left out, each `[kind] ++ name bytes`.
#[cfg(unix)]
fn list_held(d: std::fs::File) -> std::io::Result<Vec<Vec<u8>>> {
    use std::os::fd::IntoRawFd as _;
    let raw = d.into_raw_fd();
    // SAFETY: a descriptor this call owns; on success the stream owns it.
    let dp = unsafe { libc::fdopendir(raw) };
    if dp.is_null() {
        let e = std::io::Error::last_os_error();
        // SAFETY: still ours: fdopendir failed.
        unsafe { libc::close(raw) };
        return Err(e);
    }
    // SAFETY: a live stream. The duplicate shares the held handle's offset:
    // the listing starts at the first entry whatever an earlier one left.
    unsafe { libc::rewinddir(dp) };
    let mut rows = Vec::new();
    let result = loop {
        errno_clear();
        // SAFETY: a live stream.
        let ent = unsafe { libc::readdir(dp) };
        if ent.is_null() {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error().unwrap_or(0) != 0 {
                break Err(e);
            }
            break Ok(());
        }
        // SAFETY: the entry the stream answered, valid until the next read.
        let (name, dtype) = unsafe {
            (
                std::ffi::CStr::from_ptr((*ent).d_name.as_ptr())
                    .to_bytes()
                    .to_vec(),
                (*ent).d_type,
            )
        };
        if name == b"." || name == b".." {
            continue;
        }
        let kind = kind_of_dtype(dtype).unwrap_or_else(|| {
            let Ok(c) = std::ffi::CString::new(name.clone()) else {
                return stat_kind::OTHER;
            };
            // SAFETY: the stream's own descriptor, live.
            let fd = unsafe { libc::dirfd(dp) };
            fstatat_words(fd, &c, false).map_or(stat_kind::OTHER, |w| kind_of_mode(w.mode))
        });
        let mut row = Vec::with_capacity(name.len() + 1);
        row.push(kind as u8);
        row.extend_from_slice(&name);
        rows.push(row);
    };
    // SAFETY: the stream, closed once (its descriptor with it).
    unsafe { libc::closedir(dp) };
    result.map(|()| rows)
}

/// `fs_read_dir_entries_at(dir) -> List[List[byte]] ! {denied, invalid, io,
/// not_found, unsupported}` (`[os.fs.at.readdir]`) — the held directory's
/// own entries, `[os.fs.readdir]`'s shape (the host's order, `.` and `..`
/// left out, `[kind] ++ name bytes`), read through a duplicate of the
/// handle (`fdopendir(3)`), rewound first. A handle that is no directory is
/// `invalid` (`ENOTDIR`).
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_read_dir_entries_at(dir: i64, out: i64) -> i64 {
    os_error_clear();
    #[cfg(unix)]
    {
        let Some(d) = crate::fs::dup_of(dir) else {
            return fs_code::IO;
        };
        let rows = match list_held(d) {
            Err(e) => return at_code(&e),
            Ok(r) => r,
        };
        let hdr = crate::list::new_list(8);
        for row in &rows {
            let inner = crate::list::from_bytes(row) as i64;
            crate::list::push_raw(hdr, (&raw const inner).cast());
        }
        unsafe { write_word(out, hdr as i64) };
        fs_code::OK
    }
    #[cfg(not(unix))]
    {
        let _ = (dir, out);
        fs_code::UNSUPPORTED
    }
}

/// `fs_set_mode_fd(fd, mode) -> () ! {denied, invalid, io, unsupported}`
/// (`[os.fs.mode.fd]`) — `fchmod(2)` (std's `File::set_permissions`): the
/// permission, set-id and sticky bits of the file the handle holds become
/// exactly `mode`, whatever its name is now. 0..2 are the standard streams
/// (`[os.fs.std]`). `invalid` is a mode outside `0..=0o7777`; windows has
/// no permission bits and answers `unsupported` (`fs_set_mode`'s rule); a
/// closed or forged handle is `io` with no number. `denied` is a file this
/// process does not own (`EPERM`).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_fs_set_mode_fd(fd: i64, mode: i64) -> i64 {
    os_error_clear();
    if !(0..=crate::fs_change::MODE_MAX).contains(&mode) {
        return fs_code::INVALID;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let perm = std::fs::Permissions::from_mode(mode as u32);
        let Some(r) = with_handle(fd, |f| f.set_permissions(perm)) else {
            return fs_code::IO;
        };
        match r {
            Err(e) => crate::fs::code_of(&e),
            Ok(()) => fs_code::OK,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = fd;
        fs_code::UNSUPPORTED
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::fs::{__wolf_rt_fs_close, __wolf_rt_os_error, fs_mode, mint_file};

    fn pair(s: &str) -> (i64, i64) {
        (s.as_ptr() as i64, s.len() as i64)
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wolf-rt-at-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn hold(p: &std::path::Path) -> i64 {
        let s = p.display().to_string();
        let (pp, pl) = pair(&s);
        // SAFETY: a str pair this test owns.
        let fd =
            unsafe { crate::fs::__wolf_rt_fs_open(pp, pl, fs_mode::DIRECTORY | fs_mode::NOFOLLOW) };
        assert!(fd >= 3, "a directory opens under both bits: {fd}");
        fd
    }

    fn record(dir: i64, name: &str, follow: bool) -> Result<Vec<i64>, i64> {
        let (np, nl) = pair(name);
        let mut out = [0i64; 1];
        // SAFETY: a str pair and an 8-byte slot this test owns.
        let rc = unsafe {
            __wolf_rt_fs_stat_at(dir, np, nl, i64::from(follow), out.as_mut_ptr() as i64)
        };
        if rc != fs_code::OK {
            return Err(rc);
        }
        // SAFETY: the shim minted a List[int] header.
        Ok(unsafe { crate::list::i64_elems(out[0]) }.unwrap().to_vec())
    }

    /// The record by handle and name is the path record, word for word, for
    /// a file, a directory, a link (followed and not) and a fifo — the
    /// `statx` layout and the `fstatat` words held to std's metadata.
    #[test]
    fn the_record_by_handle_is_the_paths() {
        let d = scratch("record");
        std::fs::write(d.join("f"), b"twelve bytes").unwrap();
        std::fs::create_dir(d.join("sub")).unwrap();
        std::os::unix::fs::symlink("f", d.join("l")).unwrap();
        let fifo = std::ffi::CString::new(d.join("p").display().to_string()).unwrap();
        // SAFETY: a NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let h = hold(&d);
        for (name, follow) in [
            ("f", true),
            ("sub", true),
            ("l", false),
            ("l", true),
            ("p", false),
        ] {
            let md = if follow {
                std::fs::metadata(d.join(name))
            } else {
                std::fs::symlink_metadata(d.join(name))
            }
            .unwrap();
            let want = crate::fs::stat_record(&md).unwrap().to_vec();
            let got = record(h, name, follow).unwrap();
            assert_eq!(got, want, "{name} follow={follow}");
        }
        assert_eq!(record(h, "missing", false), Err(fs_code::NOT_FOUND));
        assert_eq!(record(h, "f/x", true), Err(fs_code::INVALID));
        assert_eq!(__wolf_rt_os_error(), i64::from(libc::ENOTDIR));
        assert_eq!(record(h, "", false), Err(fs_code::INVALID));
        assert_eq!(record(h, "/", false), Err(fs_code::INVALID));
        assert_eq!(record(1_000_000, "f", false), Err(fs_code::IO));
        let _ = __wolf_rt_fs_close(h);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The millisecond word agrees with `fs_modified_ms`'s on both sides of
    /// the epoch (toward zero, as a `SystemTime`'s duration counts).
    #[test]
    fn the_millisecond_word_is_the_paths() {
        assert_eq!(ms_of(0, 0), Some(0));
        assert_eq!(ms_of(1, 999_999_999), Some(1999));
        assert_eq!(ms_of(-1, 999_999_999), Some(0));
        assert_eq!(ms_of(-1, 0), Some(-1000));
        assert_eq!(ms_of(-2, 500_000_000), Some(-1500));
    }

    /// The open word: the six modes and the two flags; the directory bit on
    /// a read only; a bit above it `invalid`.
    #[test]
    fn the_open_word_is_the_open_familys() {
        assert_eq!(open_word(0), Some((0, false, false)));
        assert_eq!(open_word(48), Some((0, true, true)));
        assert_eq!(open_word(37), Some((5, false, true)));
        assert_eq!(open_word(33), None);
        assert_eq!(open_word(36), None);
        assert_eq!(open_word(6), None);
        assert_eq!(open_word(64), None);
        assert_eq!(open_word(-1), None);
        assert_eq!(open_word(20), Some((4, true, false)));
    }

    /// Through a handle whose path was renamed: an open, a removal, a
    /// directory made and removed, a link made, a second name made, a listing
    /// read twice; a file handle as the directory `invalid` (ENOTDIR).
    #[test]
    fn the_calls_follow_the_handle_not_the_path() {
        let d = scratch("walk");
        std::fs::create_dir(d.join("in")).unwrap();
        std::fs::write(d.join("in/f"), b"wolves").unwrap();
        let h = hold(&d.join("in"));
        std::fs::rename(d.join("in"), d.join("moved")).unwrap();
        let (np, nl) = pair("f");
        // SAFETY: str pairs this test owns, below as here.
        let fd = unsafe { __wolf_rt_fs_open_at(h, np, nl, fs_mode::READ, 0) };
        assert!(fd >= 3, "opened through the handle: {fd}");
        let mut bytes = Vec::new();
        crate::fs::with_handle(fd, |f| {
            use std::io::Read as _;
            let mut r = f;
            r.read_to_end(&mut bytes).unwrap();
        });
        assert_eq!(bytes, b"wolves");
        let (sp, sl) = pair("sub");
        unsafe {
            assert_eq!(__wolf_rt_fs_create_dir_at(h, sp, sl, 0o700), fs_code::OK);
            assert_eq!(
                __wolf_rt_fs_create_dir_at(h, sp, sl, 0o700),
                fs_code::EXISTS
            );
            assert_eq!(
                __wolf_rt_fs_create_dir_at(h, sp, sl, 0o1000),
                fs_code::INVALID
            );
        }
        assert!(d.join("moved/sub").is_dir());
        let (tp, tl) = pair("no/such/place");
        let (lp, ll) = pair("ln");
        unsafe {
            assert_eq!(__wolf_rt_fs_symlink_at(tp, tl, h, lp, ll), fs_code::OK);
            assert_eq!(__wolf_rt_fs_symlink_at(tp, 0, h, lp, ll), fs_code::INVALID);
        }
        assert_eq!(
            std::fs::read_link(d.join("moved/ln")).unwrap().to_str(),
            Some("no/such/place")
        );
        let (gp, gl) = pair("g");
        unsafe {
            assert_eq!(__wolf_rt_fs_link_at(h, np, nl, h, gp, gl, 0), fs_code::OK);
            assert_eq!(
                __wolf_rt_fs_link_at(h, np, nl, h, gp, gl, 0),
                fs_code::EXISTS
            );
        }
        let mut out = [0i64; 1];
        for _ in 0..2 {
            // SAFETY: an 8-byte slot.
            assert_eq!(
                unsafe { __wolf_rt_fs_read_dir_entries_at(h, out.as_mut_ptr() as i64) },
                fs_code::OK
            );
            // SAFETY: the shim minted a list of four entries.
            let n = unsafe { crate::list::__wolf_rt_list_len(out[0]) };
            assert_eq!(n, 4, "f, sub, ln, g: the same on a second read");
        }
        unsafe {
            assert_eq!(__wolf_rt_fs_remove_at(h, gp, gl, 0), fs_code::OK);
            assert_eq!(__wolf_rt_fs_remove_at(h, sp, sl, 1), fs_code::OK);
            assert_eq!(__wolf_rt_fs_remove_at(h, np, nl, 1), fs_code::INVALID);
            assert_eq!(__wolf_rt_os_error(), i64::from(libc::ENOTDIR));
            assert_eq!(__wolf_rt_fs_open_at(fd, np, nl, 0, 0), -fs_code::INVALID);
            assert_eq!(
                __wolf_rt_fs_open_at(h, lp, ll, fs_mode::NOFOLLOW, 0),
                -fs_code::INVALID
            );
        }
        assert!(!d.join("moved/sub").exists());
        // SAFETY: an 8-byte slot.
        assert_eq!(
            unsafe { __wolf_rt_fs_read_dir_entries_at(fd, out.as_mut_ptr() as i64) },
            fs_code::INVALID
        );
        let _ = __wolf_rt_fs_close(fd);
        let _ = __wolf_rt_fs_close(h);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// `fchmod` changes the held file after another is renamed over its
    /// name; a mode past 0o7777 is `invalid`; a forged handle `io`.
    #[test]
    fn the_mode_set_on_a_handle_is_the_held_files() {
        use std::os::unix::fs::PermissionsExt as _;
        let d = scratch("fchmod");
        let a = d.join("a");
        std::fs::write(&a, b"held").unwrap();
        let fd = mint_file(std::fs::File::open(&a).unwrap());
        std::fs::write(d.join("b"), b"other").unwrap();
        std::fs::set_permissions(d.join("b"), std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::rename(d.join("b"), &a).unwrap();
        assert_eq!(__wolf_rt_fs_set_mode_fd(fd, 0o600), fs_code::OK);
        let held =
            crate::fs::with_handle(fd, |f| f.metadata().unwrap().permissions().mode()).unwrap();
        assert_eq!(held & 0o7777, 0o600);
        assert_eq!(
            std::fs::metadata(&a).unwrap().permissions().mode() & 0o7777,
            0o644
        );
        assert_eq!(__wolf_rt_fs_set_mode_fd(fd, 0o10000), fs_code::INVALID);
        assert_eq!(__wolf_rt_fs_set_mode_fd(1_000_000, 0o600), fs_code::IO);
        let _ = __wolf_rt_fs_close(fd);
        let _ = std::fs::remove_dir_all(&d);
    }
}
