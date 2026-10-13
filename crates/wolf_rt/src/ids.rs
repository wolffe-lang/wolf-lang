//! s236 (wolf-lang#633, #701, #742): who a process runs as, a file's
//! owner, and the drop. `[os.proc.ids]`, `[os.proc.ids.set]`,
//! `[os.fs.owner]`.
//!
//! Three small things a program could not do: read its user and group
//! ids (a server told to drop privilege has to know whether it is
//! root), change a file's owner (`chown`, `chgrp`, `cp -p`, `install
//! -o`), and become another user for good (`su`, `login`, a server
//! after it binds its low port).
//!
//! # One order for every call
//!
//! As [`crate::fs_change`]: the SHAPE of the arguments first
//! (`invalid`, the same on every host), then the HOST (`unsupported`,
//! by name), then the host's own calls.
//!
//! # The drop is one call, and it is whole or it is fatal
//!
//! [`__wolf_rt_os_set_ids`] makes the three changes in the one order
//! that works — the supplementary groups, then the group ids, then the
//! user ids: once the user ids are another user's the process may no
//! longer change a group (measured, s236: `setresuid` first leaves
//! `setresgid` and `setgroups` both `EPERM`, and the process keeps the
//! group id and the groups it started with). The call then READS the
//! ids and the groups back and tries to take the old ids back, which
//! must fail.
//!
//! A program is never handed a process that is partly one user and
//! partly another:
//!
//! - a refusal BEFORE anything changed returns its row;
//! - a refusal AFTER a change is put back (the user ids are the last
//!   step, so the process still holds what it needs to), the result is
//!   read back, and the row is returned only if the process is exactly
//!   as it was;
//! - anything else — the put-back refused, a read-back that disagrees,
//!   an old id that can still be taken — ends the process:
//!   [`half_dropped`] writes one line on standard error and aborts. Not
//!   a trap: a proc boundary contains a trap (D68), and the ids are the
//!   whole process's.

#[cfg(unix)]
use crate::fs::{code_of, with_handle};
use crate::fs::{fs_code, os_error_clear};
use crate::os::proc_code;
#[cfg(unix)]
use crate::str::view;

/// The largest user or group id a call takes. One below the 32-bit
/// all-ones value, which every host call here reads as "leave this id
/// alone" and which is therefore never an id (`chown(2)`,
/// `setresuid(2)`).
pub const ID_MAX: i64 = 0xFFFF_FFFE;

/// `fs_set_owner`'s and `fs_set_owner_fd`'s word for "leave this id as
/// it is".
pub const ID_LEAVE: i64 = -1;

/// The most supplementary groups one `os_set_ids` takes — linux's
/// `NGROUPS_MAX` (`getgroups(2)`: 65536 since 2.6.4).
pub const GROUPS_MAX: usize = 65536;

/// An owner argument decoded: `Some(None)` leaves the id alone,
/// `Some(Some(id))` sets it, `None` is the `invalid` row.
pub fn owner_arg(v: i64) -> Option<Option<u32>> {
    match v {
        ID_LEAVE => Some(None),
        0..=ID_MAX => Some(Some(v as u32)),
        _ => None,
    }
}

/// `os_set_ids`'s arguments decoded: the user id, the group id and the
/// supplementary groups SORTED with repeats dropped — the set the host
/// will hold and the call reads back (linux keeps the list sorted and
/// keeps repeats, measured; a set has none). `None` is the `invalid`
/// row: an id outside `0..=ID_MAX` (there is no "leave" here) or more
/// than [`GROUPS_MAX`] groups.
pub fn ids_shape(uid: i64, gid: i64, groups: &[i64]) -> Option<(u32, u32, Vec<u32>)> {
    let id = |v: i64| (0..=ID_MAX).contains(&v).then_some(v as u32);
    let (u, g) = (id(uid)?, id(gid)?);
    if groups.len() > GROUPS_MAX {
        return None;
    }
    let mut set = Vec::with_capacity(groups.len());
    for &x in groups {
        set.push(id(x)?);
    }
    set.sort_unstable();
    set.dedup();
    Some((u, g, set))
}

macro_rules! id_read {
    ($(#[$doc:meta])* $sym:ident, $call:ident) => {
        $(#[$doc])*
        #[unsafe(no_mangle)]
        pub extern "C" fn $sym() -> i64 {
            #[cfg(unix)]
            {
                // SAFETY: takes nothing and cannot fail.
                i64::from(unsafe { libc::$call() })
            }
            #[cfg(not(unix))]
            {
                -proc_code::UNSUPPORTED
            }
        }
    };
}

id_read!(
    /// `os_uid() -> int ! {unsupported}` (`[os.proc.ids]`) — the
    /// process's REAL user id (`getuid`): who started it. The id (>= 0),
    /// or `-UNSUPPORTED` on a host with no user ids (windows).
    __wolf_rt_os_uid,
    getuid
);
id_read!(
    /// `os_euid() -> int ! {unsupported}` — the EFFECTIVE user id
    /// (`geteuid`): the one the host checks. 0 is the super-user.
    __wolf_rt_os_euid,
    geteuid
);
id_read!(
    /// `os_gid() -> int ! {unsupported}` — the real group id (`getgid`).
    __wolf_rt_os_gid,
    getgid
);
id_read!(
    /// `os_egid() -> int ! {unsupported}` — the effective group id
    /// (`getegid`).
    __wolf_rt_os_egid,
    getegid
);

/// The process's supplementary groups as the host answers them
/// (`getgroups`), in the host's order. The list can change between the
/// call that asks its length and the call that reads it; a list that
/// grew is asked again.
#[cfg(unix)]
pub fn groups() -> Result<Vec<u32>, i64> {
    for _ in 0..8 {
        // SAFETY: a zero size reads the count and writes nothing.
        let n = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
        if n < 0 {
            return Err(proc_code::IO);
        }
        let mut buf: Vec<libc::gid_t> = vec![0; n as usize + 8];
        // SAFETY: `buf` holds `buf.len()` writable ids.
        let m = unsafe { libc::getgroups(buf.len() as libc::c_int, buf.as_mut_ptr()) };
        if m >= 0 {
            buf.truncate(m as usize);
            return Ok(buf);
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINVAL) {
            return Err(proc_code::IO);
        }
    }
    Err(proc_code::IO)
}

/// `os_groups() -> List[int] ! {io, unsupported}` (`[os.proc.ids]`) —
/// 0 and the minted list through `out`, or the code.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_groups(out: i64) -> i64 {
    #[cfg(unix)]
    {
        match groups() {
            Err(c) => c,
            Ok(gs) => {
                let hdr = crate::list::new_list(8);
                for g in gs {
                    crate::list::push_int(hdr, i64::from(g));
                }
                unsafe { crate::str::write_word(out, hdr as i64) };
                proc_code::OK
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = out;
        proc_code::UNSUPPORTED
    }
}

/// The fs family's code for a refused owner change: the family's
/// mapping (which records the host's number for `os_error()`), except
/// that the host's `EINVAL` — an id with no mapping in this user
/// namespace (measured: 70000 in a container that maps 0..65535), or a
/// handle that has no owner to change — is `invalid`, not `io`.
#[cfg(unix)]
fn owner_code(e: &std::io::Error) -> i64 {
    let c = code_of(e);
    if e.raw_os_error() == Some(libc::EINVAL) {
        fs_code::INVALID
    } else {
        c
    }
}

/// `-1`, the host's "leave this id alone", for an absent id.
#[cfg(unix)]
fn host_id(v: Option<u32>) -> u32 {
    v.unwrap_or(u32::MAX)
}

/// `fs_set_owner(path, uid, gid, follow) -> () ! {denied, invalid, io,
/// not_found, unsupported}` (`[os.fs.owner]`) — `fchownat(2)`: the
/// file's owner and group become `uid` and `gid`, `-1` leaving one as
/// it is. `follow` 0 changes a final symbolic link ITSELF
/// (`AT_SYMLINK_NOFOLLOW`, `chown -h`). `invalid` is an id that is
/// neither `-1` nor in `0..=ID_MAX`, decided before the host, or one
/// the host says it has no mapping for; `denied` is a change this
/// process may not make.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_set_owner(
    pp: i64,
    pl: i64,
    uid: i64,
    gid: i64,
    follow: i64,
) -> i64 {
    os_error_clear();
    let (Some(u), Some(g)) = (owner_arg(uid), owner_arg(gid)) else {
        return fs_code::INVALID;
    };
    #[cfg(unix)]
    {
        let path = unsafe { view(pp, pl) };
        match set_owner(path, u, g, follow != 0) {
            Err(e) => owner_code(&e),
            Ok(()) => fs_code::OK,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (pp, pl, u, g, follow);
        fs_code::UNSUPPORTED
    }
}

/// One `fchownat`. A path holding a NUL never reaches the host (an
/// error with no host number, the family's `io`).
#[cfg(unix)]
pub fn set_owner(
    path: &str,
    uid: Option<u32>,
    gid: Option<u32>,
    follow: bool,
) -> std::io::Result<()> {
    let c = std::ffi::CString::new(path)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let flags = if follow { 0 } else { libc::AT_SYMLINK_NOFOLLOW };
    // SAFETY: a NUL-terminated path; the ids are plain words.
    let rc = unsafe {
        libc::fchownat(
            libc::AT_FDCWD,
            c.as_ptr(),
            host_id(uid) as libc::uid_t,
            host_id(gid) as libc::gid_t,
            flags,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// `fs_set_owner_fd(fd, uid, gid) -> () ! {denied, invalid, io,
/// unsupported}` (`[os.fs.owner]`) — `fchown(2)` on a handle of the fs
/// table, 0, 1 and 2 the standard streams (`[os.fs.std]`): the file the
/// handle holds, whatever its name is now. A closed or forged handle is
/// `io` with no host number, the family's rule.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_fs_set_owner_fd(fd: i64, uid: i64, gid: i64) -> i64 {
    os_error_clear();
    let (Some(u), Some(g)) = (owner_arg(uid), owner_arg(gid)) else {
        return fs_code::INVALID;
    };
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd as _;
        let r = with_handle(fd, |f| {
            // SAFETY: the descriptor is open for the length of the call
            // (the table's lock, or a standard stream).
            let rc = unsafe {
                libc::fchown(
                    f.as_raw_fd(),
                    host_id(u) as libc::uid_t,
                    host_id(g) as libc::gid_t,
                )
            };
            if rc == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
        match r {
            None => fs_code::IO,
            Some(Err(e)) => owner_code(&e),
            Some(Ok(())) => fs_code::OK,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (fd, u, g);
        fs_code::UNSUPPORTED
    }
}

/// `os_set_ids(uid, gid, groups) -> () ! {denied, invalid, io,
/// unsupported}` (`[os.proc.ids.set]`) — this process becomes user
/// `uid`, group `gid`, with exactly the supplementary groups `groups`,
/// for good: real, effective and saved ids. 0, or the code — and the
/// code only ever with the process exactly as it was (the module
/// documentation has the rule; [`half_dropped`] the other ending).
///
/// `invalid` is an id outside `0..=ID_MAX` or more than [`GROUPS_MAX`]
/// groups, decided before the host, and an id the host has no mapping
/// for; `unsupported` every host but linux (macOS has no way to read a
/// drop back: `getgroups(2)` there); `denied` a process that may not
/// change its ids.
///
/// # Safety
///
/// `groups` a live `List[int]` header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_set_ids(uid: i64, gid: i64, groups: i64) -> i64 {
    let Some(flat) = (unsafe { crate::list::i64_elems(groups) }) else {
        return proc_code::IO;
    };
    let Some((u, g, set)) = ids_shape(uid, gid, flat) else {
        return proc_code::INVALID;
    };
    #[cfg(target_os = "linux")]
    {
        match drop_to(u, g, &set) {
            Ok(()) => proc_code::OK,
            Err(c) => c,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (u, g, set);
        proc_code::UNSUPPORTED
    }
}

/// Everything `os_set_ids` changes, as the host reads it back.
#[cfg(target_os = "linux")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ids {
    /// Real, effective, saved user id.
    pub uid: [u32; 3],
    /// Real, effective, saved group id.
    pub gid: [u32; 3],
    /// The supplementary groups, sorted, repeats dropped.
    pub groups: Vec<u32>,
}

#[cfg(target_os = "linux")]
impl Ids {
    /// The process's ids now; `None` when the host will not say.
    pub fn read() -> Option<Ids> {
        let (mut r, mut e, mut s) = (0, 0, 0);
        // SAFETY: three writable ids.
        if unsafe { libc::getresuid(&mut r, &mut e, &mut s) } != 0 {
            return None;
        }
        let uid = [r, e, s];
        // SAFETY: three writable ids.
        if unsafe { libc::getresgid(&mut r, &mut e, &mut s) } != 0 {
            return None;
        }
        let mut groups = groups().ok()?;
        groups.sort_unstable();
        groups.dedup();
        Some(Ids {
            uid,
            gid: [r, e, s],
            groups,
        })
    }

    fn show(&self) -> String {
        format!(
            "uid {}/{}/{} gid {}/{}/{} groups {:?}",
            self.uid[0],
            self.uid[1],
            self.uid[2],
            self.gid[0],
            self.gid[1],
            self.gid[2],
            self.groups
        )
    }
}

/// The process family's code for a refused id change.
#[cfg(target_os = "linux")]
fn ids_code(errno: Option<i32>) -> i64 {
    match errno {
        Some(libc::EPERM) => proc_code::DENIED,
        Some(libc::EINVAL) => proc_code::INVALID,
        _ => proc_code::IO,
    }
}

#[cfg(target_os = "linux")]
fn errno() -> Option<i32> {
    std::io::Error::last_os_error().raw_os_error()
}

/// The three host calls. Each is the C library's, which makes the
/// change in EVERY thread of the process (`nptl(7)`: the kernel keeps
/// the ids per thread) — the runtime's workers included.
#[cfg(target_os = "linux")]
mod host {
    pub fn setgroups(gs: &[u32]) -> bool {
        // SAFETY: `gs.len()` readable ids (a dangling pointer is never
        // read for a length of 0).
        unsafe { libc::setgroups(gs.len(), gs.as_ptr()) == 0 }
    }
    pub fn setresgid(g: [u32; 3]) -> bool {
        // SAFETY: plain words.
        unsafe { libc::setresgid(g[0], g[1], g[2]) == 0 }
    }
    pub fn setresuid(u: [u32; 3]) -> bool {
        // SAFETY: plain words.
        unsafe { libc::setresuid(u[0], u[1], u[2]) == 0 }
    }
}

/// One drop at a time: two tasks asking at once would each read the
/// other's half-made change as its own.
#[cfg(target_os = "linux")]
static DROP: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The drop. `Err(code)` only with the process exactly as it was.
#[cfg(target_os = "linux")]
pub fn drop_to(uid: u32, gid: u32, groups: &[u32]) -> Result<(), i64> {
    let _one = DROP.lock().unwrap_or_else(|p| p.into_inner());
    let Some(before) = Ids::read() else {
        return Err(proc_code::IO);
    };
    let want = Ids {
        uid: [uid; 3],
        gid: [gid; 3],
        groups: groups.to_vec(),
    };
    // 1. The supplementary groups. Refused, nothing has changed.
    if !host::setgroups(groups) {
        return Err(ids_code(errno()));
    }
    // 2. The group ids. Refused, the groups go back.
    if !host::setresgid(want.gid) {
        let code = ids_code(errno());
        return put_back(&before, &want, "the group ids were refused", code);
    }
    // 3. The user ids, last: after them the process may change no group.
    if !host::setresuid(want.uid) {
        let code = ids_code(errno());
        return put_back(&before, &want, "the user ids were refused", code);
    }
    // 4. It took: what the host reads back is what was asked.
    match Ids::read() {
        Some(after) if after == want => {}
        after => half_dropped(
            "the ids read back are not the ids asked for",
            &before,
            after.as_ref(),
            &want,
        ),
    }
    // 5. And it holds: no id the process had can be taken back. (A
    //    process that asked to be the super-user may take anything, so
    //    there is nothing to try.)
    if uid != 0 {
        for (i, &old) in before.uid.iter().enumerate() {
            // SAFETY: plain words.
            if old != uid && !before.uid[..i].contains(&old) && unsafe { libc::seteuid(old) } == 0 {
                half_dropped(
                    "the user id it had can still be taken back",
                    &before,
                    Ids::read().as_ref(),
                    &want,
                );
            }
        }
        for (i, &old) in before.gid.iter().enumerate() {
            // SAFETY: plain words.
            if old != gid && !before.gid[..i].contains(&old) && unsafe { libc::setegid(old) } == 0 {
                half_dropped(
                    "the group id it had can still be taken back",
                    &before,
                    Ids::read().as_ref(),
                    &want,
                );
            }
        }
    }
    Ok(())
}

/// A step after the first was refused: the user ids have not moved (they
/// are the last step, and a refused `setresuid` changes nothing), so
/// the process may still set its groups. Put the group ids and the
/// groups back, read them, and answer `code` only if the process is
/// exactly `before`.
#[cfg(target_os = "linux")]
fn put_back(before: &Ids, want: &Ids, why: &'static str, code: i64) -> Result<(), i64> {
    // What the two calls answer is not asked: the read-back decides. A
    // call refused over a value the process already holds has changed
    // nothing, and a call that answered 0 is believed only once read.
    let _ = host::setresgid(before.gid);
    let _ = host::setgroups(&before.groups);
    let now = Ids::read();
    if now.as_ref() == Some(before) {
        return Err(code);
    }
    half_dropped(why, before, now.as_ref(), want)
}

/// The process is neither who it was nor who it asked to be. One line
/// on standard error — what was asked, what it was, what it is — and
/// the process ends by `abort` (SIGABRT). Never returns; never a trap,
/// which a proc boundary would contain.
#[cfg(target_os = "linux")]
fn half_dropped(why: &str, before: &Ids, now: Option<&Ids>, want: &Ids) -> ! {
    use std::io::Write as _;
    let now = now.map_or_else(|| "unreadable".to_string(), Ids::show);
    let line = format!(
        "wolf: os_set_ids: {why}: asked for {}; was {}; is {now} — the process stops rather than run half-dropped\n",
        want.show(),
        before.show(),
    );
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(line.as_bytes());
    let _ = err.flush();
    std::process::abort()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::__wolf_rt_os_error;

    fn pair_of(s: &str) -> (i64, i64) {
        (s.as_ptr() as i64, s.len() as i64)
    }

    #[test]
    fn an_owner_word_is_an_id_or_leave() {
        assert_eq!(owner_arg(-1), Some(None));
        assert_eq!(owner_arg(0), Some(Some(0)));
        assert_eq!(owner_arg(ID_MAX), Some(Some(0xFFFF_FFFE)));
        assert_eq!(owner_arg(ID_MAX + 1), None, "all-ones is the host's -1");
        assert_eq!(owner_arg(-2), None);
        assert_eq!(owner_arg(i64::MAX), None);
    }

    #[test]
    fn the_ids_shape_sorts_the_groups_and_has_no_leave() {
        assert_eq!(
            ids_shape(1000, 1000, &[2000, 1000, 2000]),
            Some((1000, 1000, vec![1000, 2000]))
        );
        assert_eq!(ids_shape(0, 0, &[]), Some((0, 0, vec![])));
        assert_eq!(ids_shape(-1, 0, &[]), None, "no leave for the user id");
        assert_eq!(ids_shape(0, -1, &[]), None, "nor the group id");
        assert_eq!(ids_shape(0, 0, &[-1]), None);
        assert_eq!(ids_shape(ID_MAX + 1, 0, &[]), None);
        assert_eq!(ids_shape(0, 0, &[ID_MAX + 1]), None);
        assert_eq!(ids_shape(0, 0, &vec![7; GROUPS_MAX]), Some((0, 0, vec![7])));
        assert_eq!(ids_shape(0, 0, &vec![7; GROUPS_MAX + 1]), None);
    }

    /// Shape before host, on every host: nothing is asked of the file
    /// system and no host number is left behind.
    #[test]
    fn a_bad_owner_is_invalid_before_the_host() {
        let (pp, pl) = pair_of("wolf-s236-no-such-path");
        unsafe {
            assert_eq!(__wolf_rt_fs_set_owner(pp, pl, -2, -1, 1), fs_code::INVALID);
            assert_eq!(
                __wolf_rt_fs_set_owner(pp, pl, -1, ID_MAX + 1, 0),
                fs_code::INVALID
            );
        }
        assert_eq!(__wolf_rt_fs_set_owner_fd(1 << 40, -2, 0), fs_code::INVALID);
        assert_eq!(__wolf_rt_fs_set_owner_fd(1 << 40, 0, -7), fs_code::INVALID);
        assert_eq!(__wolf_rt_os_error(), 0);
    }

    /// The four ids are the host's own, and the groups are `getgroups`.
    #[cfg(unix)]
    #[test]
    fn the_ids_are_the_hosts() {
        use std::os::unix::fs::MetadataExt as _;
        // SAFETY: the four take nothing and cannot fail.
        unsafe {
            assert_eq!(__wolf_rt_os_uid(), i64::from(libc::getuid()));
            assert_eq!(__wolf_rt_os_euid(), i64::from(libc::geteuid()));
            assert_eq!(__wolf_rt_os_gid(), i64::from(libc::getgid()));
            assert_eq!(__wolf_rt_os_egid(), i64::from(libc::getegid()));
        }
        // A file this process creates is owned by its effective user.
        let p = std::env::temp_dir().join(format!("wolf-rt-s236-ids-{}", std::process::id()));
        std::fs::write(&p, b"x").unwrap();
        assert_eq!(
            i64::from(std::fs::metadata(&p).unwrap().uid()),
            __wolf_rt_os_euid()
        );
        let _ = std::fs::remove_file(&p);
        let gs = groups().expect("getgroups answers");
        assert!(gs.len() <= GROUPS_MAX);
    }

    /// The owner calls that change nothing: `-1 -1`, and the owner a
    /// file already has. A missing path is `not_found`, a dangling link
    /// is `not_found` followed and itself otherwise, and a closed handle
    /// is `io` with no host number. No owner on this host changes.
    #[cfg(unix)]
    #[test]
    fn an_owner_call_that_changes_nothing() {
        use crate::fs::{__wolf_rt_fs_close, __wolf_rt_fs_open};
        use std::os::unix::fs::MetadataExt as _;
        let d = std::env::temp_dir().join(format!("wolf-rt-s236-owner-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("f");
        std::fs::write(&f, b"x").unwrap();
        std::os::unix::fs::symlink("nope", d.join("dangling")).unwrap();
        let md = std::fs::metadata(&f).unwrap();
        let (uid, gid) = (i64::from(md.uid()), i64::from(md.gid()));
        let fs_ = f.display().to_string();
        let (fp, fl) = pair_of(&fs_);
        let dang = d.join("dangling").display().to_string();
        let (dp, dl) = pair_of(&dang);
        let gone = d.join("gone").display().to_string();
        let (gp, gl) = pair_of(&gone);
        unsafe {
            assert_eq!(__wolf_rt_fs_set_owner(fp, fl, -1, -1, 1), fs_code::OK);
            assert_eq!(__wolf_rt_fs_set_owner(fp, fl, uid, gid, 1), fs_code::OK);
            assert_eq!(__wolf_rt_fs_set_owner(fp, fl, uid, gid, 0), fs_code::OK);
            assert_eq!(
                __wolf_rt_fs_set_owner(gp, gl, -1, -1, 1),
                fs_code::NOT_FOUND
            );
            assert_eq!(__wolf_rt_os_error(), i64::from(libc::ENOENT));
            assert_eq!(
                __wolf_rt_fs_set_owner(dp, dl, -1, -1, 1),
                fs_code::NOT_FOUND
            );
            assert_eq!(__wolf_rt_fs_set_owner(dp, dl, -1, -1, 0), fs_code::OK);
            let fd = __wolf_rt_fs_open(fp, fl, 0);
            assert!(fd >= 3);
            assert_eq!(__wolf_rt_fs_set_owner_fd(fd, -1, -1), fs_code::OK);
            assert_eq!(__wolf_rt_fs_set_owner_fd(fd, uid, gid), fs_code::OK);
            assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            assert_eq!(__wolf_rt_fs_set_owner_fd(fd, -1, -1), fs_code::IO);
            assert_eq!(
                __wolf_rt_os_error(),
                0,
                "a closed handle has no host number"
            );
            // Root's files are not a user's to take, and a user's are
            // not his to give away.
            if libc::geteuid() != 0 {
                assert_eq!(__wolf_rt_fs_set_owner(fp, fl, 0, -1, 1), fs_code::DENIED);
                assert_eq!(__wolf_rt_os_error(), i64::from(libc::EPERM));
                let fd = __wolf_rt_fs_open(fp, fl, 0);
                assert_eq!(__wolf_rt_fs_set_owner_fd(fd, 0, -1), fs_code::DENIED);
                assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);
            }
        }
        let after = std::fs::metadata(&f).unwrap();
        assert_eq!((i64::from(after.uid()), i64::from(after.gid())), (uid, gid));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A user may not set ids, and is exactly who it was afterwards:
    /// `setgroups` is the first step and the first refusal.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_user_is_denied_and_unchanged() {
        // SAFETY: takes nothing.
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("SKIP: a_user_is_denied_and_unchanged runs as a user, and this is root");
            return;
        }
        let before = Ids::read().unwrap();
        assert_eq!(
            drop_to(before.uid[0], before.gid[0], &before.groups),
            Err(proc_code::DENIED)
        );
        assert_eq!(drop_to(0, 0, &[]), Err(proc_code::DENIED));
        assert_eq!(Ids::read().unwrap(), before);
    }

    /// The hosts with no such thing say so by name, after the shape.
    #[cfg(not(unix))]
    #[test]
    fn windows_refuses_ids_and_owners_by_name() {
        let (pp, pl) = pair_of("wolf-s236-no-such-path");
        assert_eq!(__wolf_rt_os_uid(), -proc_code::UNSUPPORTED);
        assert_eq!(__wolf_rt_os_euid(), -proc_code::UNSUPPORTED);
        assert_eq!(__wolf_rt_os_gid(), -proc_code::UNSUPPORTED);
        assert_eq!(__wolf_rt_os_egid(), -proc_code::UNSUPPORTED);
        let mut out = 0i64;
        unsafe {
            assert_eq!(
                __wolf_rt_os_groups((&raw mut out) as i64),
                proc_code::UNSUPPORTED
            );
            assert_eq!(
                __wolf_rt_fs_set_owner(pp, pl, -1, -1, 1),
                fs_code::UNSUPPORTED
            );
        }
        assert_eq!(__wolf_rt_fs_set_owner_fd(1, -1, -1), fs_code::UNSUPPORTED);
    }
}
