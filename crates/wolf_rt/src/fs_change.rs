//! s230 (wolf-lang#346, #698, #699, #700, #702, #704): the calls that
//! CHANGE a file without writing its bytes — a mode, a link, a time, a
//! length — and the process's file mode creation mask. `[os.fs.mode]`,
//! `[os.fs.create.mode]`, `[os.fs.symlink]`, `[os.fs.link]`,
//! `[os.fs.times]`, `[os.fs.truncate]`, `[os.fs.umask]`.
//!
//! s218 gave programs the read side (`fs_stat`'s mode, link count and
//! four times); this is the write side `chmod`, `touch`, `ln`,
//! `mkdir -m` and `cp -p` need. A module of its own, as `exec.rs` is:
//! it shares [`crate::fs`]'s handle table, error codes and host-error
//! word and nothing else.
//!
//! # One order for every call
//!
//! 1. **Shape** (`invalid`): an argument no host could be asked about —
//!    a mode outside its bits, a nanosecond count that is neither a
//!    count nor one of the two special values, a negative length, an
//!    empty link target. Decided before the host is touched, so it is
//!    the same answer on every host.
//! 2. **Host** (`unsupported`): the hosts that have no such operation
//!    say so BY NAME — windows has no permission bits, no umask and no
//!    unprivileged symbolic link.
//! 3. **The host's own call**, once, and its error through the family's
//!    mapping ([`crate::fs`]'s `code_of`), which also records the
//!    host's number for `os_error()`.
//!
//! No call here retries, stats first, or makes a second system call to
//! tidy up: each is the one host call its clause names.

#[cfg(unix)]
use crate::fs::mint_file;
use crate::fs::{code_of, fs_code, open_options, os_error_clear, with_handle};
use crate::str::view;

/// The widest mode [`__wolf_rt_fs_set_mode`] takes: the nine permission
/// bits with set-user-id, set-group-id and sticky — `fs_stat`'s word 4.
pub const MODE_MAX: i64 = 0o7777;

/// The widest mode a CREATION takes: the nine permission bits. What a
/// host does with the three above them at `open` or `mkdir` differs
/// (linux keeps them, macOS drops them — measured), so they are not
/// part of the surface; a program sets them with `fs_set_mode`.
pub const PERM_MAX: i64 = 0o777;

/// `fs_set_times`'s nanosecond word for "the host's clock, now"
/// (`UTIME_NOW`): the seconds word beside it is not read.
pub const TIME_NOW: i64 = -1;

/// `fs_set_times`'s nanosecond word for "leave this time as it is"
/// (`UTIME_OMIT`): the seconds word beside it is not read.
pub const TIME_LEAVE: i64 = -2;

/// One of `fs_set_times`'s two times, decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeArg {
    /// Seconds from the Unix epoch (negative before it) and
    /// nanoseconds `0..=999_999_999`.
    At(i64, i64),
    /// The host's clock at the call.
    Now,
    /// Unchanged.
    Leave,
}

/// Decode a `(seconds, nanoseconds)` pair of `fs_set_times`; `None` is
/// the `invalid` row.
pub fn time_arg(s: i64, ns: i64) -> Option<TimeArg> {
    match ns {
        TIME_NOW => Some(TimeArg::Now),
        TIME_LEAVE => Some(TimeArg::Leave),
        0..=999_999_999 => Some(TimeArg::At(s, ns)),
        _ => None,
    }
}

/// `fs_set_mode(path, mode) -> () ! {denied, invalid, io, not_found,
/// unsupported}` — `chmod(2)`: the file's permission, set-id and sticky
/// bits become exactly `mode`. A final symbolic link is FOLLOWED (no
/// tier-1 host changes a link's own mode through a portable call, and
/// none reads one for access); a dangling link is `not_found`. `invalid`
/// is a mode outside `0..=0o7777`; `denied` is a file this process does
/// not own.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_set_mode(pp: i64, pl: i64, mode: i64) -> i64 {
    os_error_clear();
    if !(0..=MODE_MAX).contains(&mode) {
        return fs_code::INVALID;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let path = unsafe { view(pp, pl) };
        match std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode as u32)) {
            Err(e) => code_of(&e),
            Ok(()) => fs_code::OK,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (pp, pl);
        fs_code::UNSUPPORTED
    }
}

/// `fs_open_perm(path, open, mode) -> int ! {denied, exists, invalid,
/// io, not_found, unsupported}` — `fs_open_mode(path, open)` with the
/// permission bits a CREATION uses: the file, if this call creates it,
/// gets `mode` less the process's umask, from its first instant. An
/// existing file keeps its mode, and an `open` that never creates (0,
/// 5) ignores `mode` after checking it. The handle (>= 0), or `-code`.
/// `invalid` is an `open` outside `[os.fs.open]`'s set or a `mode`
/// outside `0..=0o777`, decided in that order before the host.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_open_perm(pp: i64, pl: i64, open: i64, mode: i64) -> i64 {
    os_error_clear();
    let Some(mut opts) = open_options(open) else {
        return -fs_code::INVALID;
    };
    if !(0..=PERM_MAX).contains(&mode) {
        return -fs_code::INVALID;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        let path = unsafe { view(pp, pl) };
        match opts.mode(mode as u32).open(path) {
            Err(e) => -code_of(&e),
            Ok(f) => mint_file(f),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (pp, pl, &mut opts);
        -fs_code::UNSUPPORTED
    }
}

/// `fs_create_dir_mode(path, mode) -> () ! {denied, exists, invalid,
/// io, not_found, unsupported}` — `mkdir(2)`: one directory, created
/// with `mode` less the process's umask (`fs_create_dir` asks for
/// `0o777`). `invalid` is a mode outside `0..=0o777`.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_create_dir_mode(pp: i64, pl: i64, mode: i64) -> i64 {
    os_error_clear();
    if !(0..=PERM_MAX).contains(&mode) {
        return fs_code::INVALID;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        let path = unsafe { view(pp, pl) };
        match std::fs::DirBuilder::new().mode(mode as u32).create(path) {
            Err(e) => code_of(&e),
            Ok(()) => fs_code::OK,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (pp, pl);
        fs_code::UNSUPPORTED
    }
}

/// `fs_symlink(target, path) -> () ! {denied, exists, invalid, io,
/// not_found, unsupported}` — `symlink(2)`: `path` becomes a symbolic
/// link whose stored target is `target`, VERBATIM — not resolved, not
/// checked, not made absolute, so a link to nothing is made like any
/// other and `fs_read_link` reads the same bytes back. `exists` is any
/// entry already at `path` (a dangling link included); `not_found` a
/// missing parent. `invalid` is an empty target: linux refuses one and
/// macOS makes it (measured), so the language decides.
///
/// # Safety
///
/// Two valid str pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_symlink(tp: i64, tl: i64, pp: i64, pl: i64) -> i64 {
    os_error_clear();
    let target = unsafe { view(tp, tl) };
    if target.is_empty() {
        return fs_code::INVALID;
    }
    #[cfg(unix)]
    {
        let path = unsafe { view(pp, pl) };
        match std::os::unix::fs::symlink(target, path) {
            Err(e) => code_of(&e),
            Ok(()) => fs_code::OK,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (pp, pl);
        fs_code::UNSUPPORTED
    }
}

/// `fs_link(existing, path) -> () ! {cross_device, denied, exists, io,
/// not_found}` — `linkat(2)` with no flag (`CreateHardLinkW`): `path`
/// becomes a second name of the file `existing` names. A final
/// symbolic link in `existing` is NOT followed: the new name is a
/// second name of the link itself. `exists` is any entry already at
/// `path` — the call never replaces, which is what makes it the atomic
/// publish `fs_rename` is not; `cross_device` two file systems;
/// `denied` a directory, or a file system with no hard links.
///
/// # Safety
///
/// Two valid str pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_link(ep: i64, el: i64, pp: i64, pl: i64) -> i64 {
    os_error_clear();
    let (existing, path) = unsafe { (view(ep, el), view(pp, pl)) };
    match std::fs::hard_link(existing, path) {
        Err(e) => code_of(&e),
        Ok(()) => fs_code::OK,
    }
}

/// `fs_set_times(path, atime_s, atime_ns, mtime_s, mtime_ns, follow) ->
/// () ! {denied, invalid, io, not_found}` — `utimensat(2)`: the access
/// and modification times, each one of [`TimeArg`]'s three. `follow`
/// 0 sets a final symbolic link's OWN times (`AT_SYMLINK_NOFOLLOW`).
/// `invalid` is a nanosecond word that is none of the three, decided
/// before the host.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_set_times(
    pp: i64,
    pl: i64,
    atime_s: i64,
    atime_ns: i64,
    mtime_s: i64,
    mtime_ns: i64,
    follow: i64,
) -> i64 {
    os_error_clear();
    let (Some(a), Some(m)) = (time_arg(atime_s, atime_ns), time_arg(mtime_s, mtime_ns)) else {
        return fs_code::INVALID;
    };
    let path = unsafe { view(pp, pl) };
    match set_times(path, a, m, follow != 0) {
        Err(e) => code_of(&e),
        Ok(()) => fs_code::OK,
    }
}

/// One `utimensat`. A path holding a NUL never reaches the host (an
/// error with no host number, the family's `io`).
#[cfg(unix)]
pub fn set_times(path: &str, a: TimeArg, m: TimeArg, follow: bool) -> std::io::Result<()> {
    fn spec(t: TimeArg) -> libc::timespec {
        // SAFETY: `timespec` is plain integers (and padding on some
        // targets); all-zero is a valid value.
        let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
        match t {
            TimeArg::At(s, ns) => {
                ts.tv_sec = s as libc::time_t;
                ts.tv_nsec = ns as _;
            }
            TimeArg::Now => ts.tv_nsec = libc::UTIME_NOW as _,
            TimeArg::Leave => ts.tv_nsec = libc::UTIME_OMIT as _,
        }
        ts
    }
    let c = std::ffi::CString::new(path)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let times = [spec(a), spec(m)];
    let flags = if follow { 0 } else { libc::AT_SYMLINK_NOFOLLOW };
    // SAFETY: a NUL-terminated path and two initialized timespecs.
    if unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), flags) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// windows: the file opened for `FILE_WRITE_ATTRIBUTES` alone — a
/// directory too (`FILE_FLAG_BACKUP_SEMANTICS`), the link itself when
/// `follow` is off (`FILE_FLAG_OPEN_REPARSE_POINT`) — and its times set
/// on the handle. "Now" is this process's clock read once for the
/// call: windows has no `UTIME_NOW`, and no ownership rule it would
/// differ by.
#[cfg(windows)]
pub fn set_times(path: &str, a: TimeArg, m: TimeArg, follow: bool) -> std::io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt as _;
    const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    fn at(s: i64, ns: i64) -> std::io::Result<std::time::SystemTime> {
        let epoch = std::time::UNIX_EPOCH;
        let t = if s >= 0 {
            epoch.checked_add(std::time::Duration::new(s as u64, ns as u32))
        } else {
            epoch
                .checked_sub(std::time::Duration::from_secs(s.unsigned_abs()))
                .and_then(|t| t.checked_add(std::time::Duration::new(0, ns as u32)))
        };
        t.ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))
    }
    let now = std::time::SystemTime::now();
    let mut ft = std::fs::FileTimes::new();
    match a {
        TimeArg::At(s, ns) => ft = ft.set_accessed(at(s, ns)?),
        TimeArg::Now => ft = ft.set_accessed(now),
        TimeArg::Leave => {}
    }
    match m {
        TimeArg::At(s, ns) => ft = ft.set_modified(at(s, ns)?),
        TimeArg::Now => ft = ft.set_modified(now),
        TimeArg::Leave => {}
    }
    let mut flags = FILE_FLAG_BACKUP_SEMANTICS;
    if !follow {
        flags |= FILE_FLAG_OPEN_REPARSE_POINT;
    }
    let f = std::fs::OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .custom_flags(flags)
        .open(path)?;
    f.set_times(ft)
}

/// `fs_truncate(fd, len) -> () ! {invalid, io}` — `ftruncate(2)`
/// (`SetEndOfFile`): the file behind the handle becomes exactly `len`
/// bytes — shorter, or longer with zeros no write stored (a hole where
/// the file system keeps them). The handle's offset does NOT move.
/// `invalid` is a negative `len`, decided before the host; every host
/// refusal is `io` — a handle not open for writing, a directory, a
/// pipe, a length past the file system's limit — with the host's
/// number in `os_error()`; a closed or forged handle is `io` with no
/// number. 0, 1 and 2 are the standard streams (`[os.fs.std]`).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_fs_truncate(fd: i64, len: i64) -> i64 {
    os_error_clear();
    let Ok(len) = u64::try_from(len) else {
        return fs_code::INVALID;
    };
    match with_handle(fd, |f| f.set_len(len)) {
        None => fs_code::IO,
        Some(Err(e)) => {
            // The host's number is recorded; the row is `io` whatever
            // kind std gave the error (`EINVAL` on unix, access denied
            // on windows, for the same read-only handle).
            let _ = code_of(&e);
            fs_code::IO
        }
        Some(Ok(())) => fs_code::OK,
    }
}

/// `os_umask(mask) -> int ! {invalid, unsupported}` — `umask(2)`: the
/// process's file mode creation mask becomes `mask`, and the mask it
/// replaced is the answer (>= 0), or `-code`. The only form POSIX has:
/// a program READS the mask by setting one and setting it back.
/// `invalid` is a mask outside `0..=0o777`.
///
/// The mask is the PROCESS's — every task's, and every child's at its
/// spawn. A program sets it where a C program does: before it starts
/// tasks, or with every creation under its own control.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_umask(mask: i64) -> i64 {
    if !(0..=PERM_MAX).contains(&mask) {
        return -fs_code::INVALID;
    }
    #[cfg(unix)]
    {
        // SAFETY: `umask` cannot fail and takes no pointer.
        i64::from(unsafe { libc::umask(mask as libc::mode_t) })
    }
    #[cfg(not(unix))]
    {
        -fs_code::UNSUPPORTED
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{__wolf_rt_fs_close, __wolf_rt_os_error};

    fn pair_of(s: &str) -> (i64, i64) {
        (s.as_ptr() as i64, s.len() as i64)
    }

    /// A directory of this test's own, under the host's temporary
    /// directory, removed and remade.
    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wolf-rt-s230-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn s(p: &std::path::Path) -> String {
        p.display().to_string()
    }

    #[test]
    fn the_time_words_decode() {
        assert_eq!(time_arg(5, 0), Some(TimeArg::At(5, 0)));
        assert_eq!(
            time_arg(-5, 999_999_999),
            Some(TimeArg::At(-5, 999_999_999))
        );
        assert_eq!(time_arg(77, TIME_NOW), Some(TimeArg::Now));
        assert_eq!(time_arg(77, TIME_LEAVE), Some(TimeArg::Leave));
        assert_eq!(time_arg(0, 1_000_000_000), None);
        assert_eq!(time_arg(0, -3), None);
    }

    /// Shape before host, on every host: none of these touches the
    /// file system, and none leaves a host number behind.
    #[test]
    fn a_bad_shape_is_invalid_before_the_host() {
        let (pp, pl) = pair_of("wolf-s230-no-such-path");
        unsafe {
            assert_eq!(__wolf_rt_fs_set_mode(pp, pl, 0o10000), fs_code::INVALID);
            assert_eq!(__wolf_rt_fs_set_mode(pp, pl, -1), fs_code::INVALID);
            assert_eq!(
                __wolf_rt_fs_create_dir_mode(pp, pl, 0o1000),
                fs_code::INVALID
            );
            assert_eq!(__wolf_rt_fs_open_perm(pp, pl, 9, 0o600), -fs_code::INVALID);
            assert_eq!(__wolf_rt_fs_open_perm(pp, pl, 1, 0o1000), -fs_code::INVALID);
            assert_eq!(__wolf_rt_fs_symlink(0, 0, pp, pl), fs_code::INVALID);
            assert_eq!(
                __wolf_rt_fs_set_times(pp, pl, 0, 1_000_000_000, 0, 0, 1),
                fs_code::INVALID
            );
            assert_eq!(
                __wolf_rt_fs_set_times(pp, pl, 0, 0, 0, -3, 1),
                fs_code::INVALID
            );
        }
        assert_eq!(__wolf_rt_fs_truncate(1 << 40, -1), fs_code::INVALID);
        assert_eq!(__wolf_rt_os_umask(0o1000), -fs_code::INVALID);
        assert_eq!(__wolf_rt_os_umask(-1), -fs_code::INVALID);
        assert_eq!(__wolf_rt_os_error(), 0);
        assert!(!std::path::Path::new("wolf-s230-no-such-path").exists());
    }

    /// A second name is the same file; the call never replaces; a
    /// missing source is `not_found`. Every tier-1 host.
    #[test]
    fn a_hard_link_is_a_second_name() {
        let d = scratch("link");
        let (a, b) = (s(&d.join("a")), s(&d.join("b")));
        std::fs::write(&a, b"wolves").unwrap();
        let ((ap, al), (bp, bl)) = (pair_of(&a), pair_of(&b));
        unsafe {
            assert_eq!(__wolf_rt_fs_link(ap, al, bp, bl), fs_code::OK);
            assert_eq!(__wolf_rt_fs_link(ap, al, bp, bl), fs_code::EXISTS);
            assert_ne!(__wolf_rt_os_error(), 0, "the host's number for exists");
            let gone = s(&d.join("gone"));
            let c = s(&d.join("c"));
            let ((gp, gl), (cp, cl)) = (pair_of(&gone), pair_of(&c));
            assert_eq!(__wolf_rt_fs_link(gp, gl, cp, cl), fs_code::NOT_FOUND);
        }
        std::fs::write(&a, b"more wolves").unwrap();
        assert_eq!(std::fs::read(&b).unwrap(), b"more wolves");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Shorter, longer with zeros, the offset unmoved; a read-only
    /// handle and a closed one are `io`. Every tier-1 host.
    #[test]
    fn truncate_sets_the_length_and_keeps_the_offset() {
        use crate::fs::{__wolf_rt_fs_open, __wolf_rt_fs_tell, __wolf_rt_fs_write};
        let d = scratch("truncate");
        let p = s(&d.join("f"));
        let (pp, pl) = pair_of(&p);
        let (tp, tl) = pair_of("0123456789");
        unsafe {
            let fd = __wolf_rt_fs_open(pp, pl, 3);
            assert!(fd >= 3);
            assert_eq!(__wolf_rt_fs_write(fd, tp, tl), fs_code::OK);
            assert_eq!(__wolf_rt_fs_truncate(fd, 4), fs_code::OK);
            assert_eq!(std::fs::read(&p).unwrap(), b"0123");
            let mut at = 0i64;
            assert_eq!(__wolf_rt_fs_tell(fd, (&raw mut at) as i64), fs_code::OK);
            assert_eq!(at, 10, "the offset does not move");
            assert_eq!(__wolf_rt_fs_truncate(fd, 8), fs_code::OK);
            assert_eq!(std::fs::read(&p).unwrap(), b"0123\0\0\0\0");
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            assert_eq!(__wolf_rt_fs_truncate(fd, 0), fs_code::IO);
            assert_eq!(
                __wolf_rt_os_error(),
                0,
                "a closed handle has no host number"
            );
            let ro = __wolf_rt_fs_open(pp, pl, 0);
            assert_eq!(__wolf_rt_fs_truncate(ro, 0), fs_code::IO);
            assert_ne!(
                __wolf_rt_os_error(),
                0,
                "the host refused a read-only handle"
            );
            assert_eq!(std::fs::read(&p).unwrap().len(), 8);
            assert_eq!(__wolf_rt_fs_close(ro), fs_code::OK);
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Both times to the nanosecond, one left alone, "now" from the
    /// host, a directory, a missing path. Every tier-1 host (windows
    /// keeps 100 ns, so the nanoseconds here are multiples of 100).
    #[test]
    fn set_times_sets_each_time_or_leaves_it() {
        let d = scratch("times");
        let p = s(&d.join("f"));
        std::fs::write(&p, b"x").unwrap();
        let (pp, pl) = pair_of(&p);
        let secs = |t: std::time::SystemTime| {
            let d = t.duration_since(std::time::UNIX_EPOCH).unwrap();
            (d.as_secs() as i64, i64::from(d.subsec_nanos()))
        };
        unsafe {
            assert_eq!(
                __wolf_rt_fs_set_times(
                    pp,
                    pl,
                    1_000_000_000,
                    123_456_700,
                    1_500_000_000,
                    7_654_300,
                    1
                ),
                fs_code::OK
            );
            let md = std::fs::metadata(&p).unwrap();
            assert_eq!(secs(md.accessed().unwrap()), (1_000_000_000, 123_456_700));
            assert_eq!(secs(md.modified().unwrap()), (1_500_000_000, 7_654_300));
            // Leave the access time, move the modification time.
            assert_eq!(
                __wolf_rt_fs_set_times(pp, pl, 0, TIME_LEAVE, 1_600_000_000, 0, 1),
                fs_code::OK
            );
            let md = std::fs::metadata(&p).unwrap();
            assert_eq!(secs(md.accessed().unwrap()), (1_000_000_000, 123_456_700));
            assert_eq!(secs(md.modified().unwrap()), (1_600_000_000, 0));
            // "Now" is the host's clock: later than the year 2020.
            assert_eq!(
                __wolf_rt_fs_set_times(pp, pl, 0, TIME_LEAVE, 0, TIME_NOW, 0),
                fs_code::OK
            );
            let md = std::fs::metadata(&p).unwrap();
            assert_eq!(secs(md.accessed().unwrap()), (1_000_000_000, 123_456_700));
            assert!(secs(md.modified().unwrap()).0 > 1_577_836_800);
            // A directory's times are set like a file's.
            let dir = s(&d);
            let (dp, dl) = pair_of(&dir);
            assert_eq!(
                __wolf_rt_fs_set_times(dp, dl, 0, TIME_LEAVE, 1_400_000_000, 0, 1),
                fs_code::OK
            );
            assert_eq!(
                secs(std::fs::metadata(&d).unwrap().modified().unwrap()),
                (1_400_000_000, 0)
            );
            let gone = s(&d.join("gone"));
            let (gp, gl) = pair_of(&gone);
            assert_eq!(
                __wolf_rt_fs_set_times(gp, gl, 0, 0, 0, 0, 1),
                fs_code::NOT_FOUND
            );
            assert_ne!(__wolf_rt_os_error(), 0);
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    fn mode_of(p: &std::path::Path) -> u32 {
        use std::os::unix::fs::MetadataExt as _;
        std::fs::symlink_metadata(p).unwrap().mode() & 0o7777
    }

    /// The bits become exactly the mode, a link is followed, a dangling
    /// one and a missing path are `not_found`.
    #[cfg(unix)]
    #[test]
    fn set_mode_sets_exactly_the_bits_and_follows_a_link() {
        let d = scratch("mode");
        let f = d.join("f");
        std::fs::write(&f, b"x").unwrap();
        std::os::unix::fs::symlink("f", d.join("ln")).unwrap();
        std::os::unix::fs::symlink("nope", d.join("dangling")).unwrap();
        let (fs_, ln, dang) = (s(&f), s(&d.join("ln")), s(&d.join("dangling")));
        let ((fp, fl), (lp, ll), (dp, dl)) = (pair_of(&fs_), pair_of(&ln), pair_of(&dang));
        unsafe {
            for mode in [0o600, 0o755, 0o4711, 0o000, 0o644] {
                assert_eq!(__wolf_rt_fs_set_mode(fp, fl, mode), fs_code::OK);
                assert_eq!(i64::from(mode_of(&f)), mode);
            }
            let link_before = mode_of(&d.join("ln"));
            assert_eq!(__wolf_rt_fs_set_mode(lp, ll, 0o640), fs_code::OK);
            assert_eq!(mode_of(&f), 0o640, "the link's target changed");
            assert_eq!(mode_of(&d.join("ln")), link_before, "the link did not");
            assert_eq!(__wolf_rt_fs_set_mode(dp, dl, 0o600), fs_code::NOT_FOUND);
            assert_eq!(__wolf_rt_os_error(), i64::from(libc::ENOENT));
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The target is stored verbatim, a link to nothing is made, an
    /// entry in the way — a dangling link too — is `exists`, a missing
    /// parent `not_found`; a hard link to a link is the link.
    #[cfg(unix)]
    #[test]
    fn a_symlink_stores_its_target_verbatim() {
        let d = scratch("symlink");
        std::fs::write(d.join("a"), b"12345").unwrap();
        let mk = |target: &str, name: &str| -> i64 {
            let p = s(&d.join(name));
            let ((tp, tl), (pp, pl)) = (pair_of(target), pair_of(&p));
            unsafe { __wolf_rt_fs_symlink(tp, tl, pp, pl) }
        };
        assert_eq!(mk("a", "ln"), fs_code::OK);
        assert_eq!(mk("../no/such/place", "dangling"), fs_code::OK);
        assert_eq!(
            std::fs::read_link(d.join("dangling")).unwrap(),
            std::path::Path::new("../no/such/place")
        );
        assert_eq!(std::fs::read(d.join("ln")).unwrap(), b"12345");
        assert_eq!(mk("b", "ln"), fs_code::EXISTS);
        assert_eq!(__wolf_rt_os_error(), i64::from(libc::EEXIST));
        assert_eq!(mk("b", "dangling"), fs_code::EXISTS);
        assert_eq!(mk("a", "no-dir/ln"), fs_code::NOT_FOUND);
        // A hard link to a link is a second name of the LINK.
        let (ln, hl) = (s(&d.join("ln")), s(&d.join("hl")));
        let ((lp, ll), (hp, hl_)) = (pair_of(&ln), pair_of(&hl));
        unsafe {
            assert_eq!(__wolf_rt_fs_link(lp, ll, hp, hl_), fs_code::OK);
        }
        assert!(
            std::fs::symlink_metadata(d.join("hl"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        // And a directory has no second name.
        let (dir, dl2) = (s(&d), s(&d.join("dirlink")));
        let ((dp, dl), (xp, xl)) = (pair_of(&dir), pair_of(&dl2));
        unsafe {
            assert_eq!(__wolf_rt_fs_link(dp, dl, xp, xl), fs_code::DENIED);
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A link's own times, with the target's left alone.
    #[cfg(unix)]
    #[test]
    fn set_times_without_follow_sets_the_link() {
        use std::os::unix::fs::MetadataExt as _;
        let d = scratch("ltimes");
        std::fs::write(d.join("a"), b"x").unwrap();
        std::os::unix::fs::symlink("a", d.join("ln")).unwrap();
        let before = std::fs::metadata(d.join("a")).unwrap().mtime();
        let ln = s(&d.join("ln"));
        let (lp, ll) = pair_of(&ln);
        unsafe {
            assert_eq!(
                __wolf_rt_fs_set_times(lp, ll, 1_000_000_000, 1, 1_000_000_000, 2, 0),
                fs_code::OK
            );
        }
        let l = std::fs::symlink_metadata(d.join("ln")).unwrap();
        assert_eq!((l.atime(), l.atime_nsec()), (1_000_000_000, 1));
        assert_eq!((l.mtime(), l.mtime_nsec()), (1_000_000_000, 2));
        assert_eq!(std::fs::metadata(d.join("a")).unwrap().mtime(), before);
        // Followed, the same call moves the target.
        unsafe {
            assert_eq!(
                __wolf_rt_fs_set_times(lp, ll, 0, TIME_LEAVE, 900_000_000, 3, 1),
                fs_code::OK
            );
        }
        let a = std::fs::metadata(d.join("a")).unwrap();
        assert_eq!((a.mtime(), a.mtime_nsec()), (900_000_000, 3));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The hosts with no such operation say so by name, after the shape.
    #[cfg(not(unix))]
    #[test]
    fn windows_refuses_modes_symlinks_and_the_umask_by_name() {
        let d = scratch("unsupported");
        let p = s(&d.join("f"));
        std::fs::write(&p, b"x").unwrap();
        let (pp, pl) = pair_of(&p);
        let (tp, tl) = pair_of("f");
        let n = s(&d.join("n"));
        let (np, nl) = pair_of(&n);
        unsafe {
            assert_eq!(__wolf_rt_fs_set_mode(pp, pl, 0o600), fs_code::UNSUPPORTED);
            assert_eq!(
                __wolf_rt_fs_open_perm(np, nl, 4, 0o600),
                -fs_code::UNSUPPORTED
            );
            assert_eq!(
                __wolf_rt_fs_create_dir_mode(np, nl, 0o700),
                fs_code::UNSUPPORTED
            );
            assert_eq!(__wolf_rt_fs_symlink(tp, tl, np, nl), fs_code::UNSUPPORTED);
        }
        assert_eq!(__wolf_rt_os_umask(0o22), -fs_code::UNSUPPORTED);
        assert!(!std::path::Path::new(&n).exists(), "nothing was made");
        let _ = std::fs::remove_dir_all(&d);
    }
}
