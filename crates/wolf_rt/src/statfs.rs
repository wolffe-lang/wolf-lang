//! s272 (wolf-lang#686, `[os.fs.statfs]`; ruled yes by the maintainer
//! 2026-11-01; R-s272-1 to R-s272-4, the recommendations implemented): the
//! record of the FILE SYSTEM that holds a path — what `df` and `stat -f`
//! print.
//!
//! `fs_statfs(path) -> List[int] ! {denied, io, not_found}` answers twelve
//! numbered words in `[os.fs.stat]`'s shape, a `have` word first:
//!
//! | word | name | linux (`statfs`) | macOS (`statfs`) |
//! |---|---|---|---|
//! | 0 | have | bit i set when word i was answered | |
//! | 1 | block size | `f_bsize` (GNU's "Block size") | `f_iosize` |
//! | 2 | fragment size | `f_frsize` (GNU's "Fundamental block size") | `f_bsize` |
//! | 3, 4, 5 | blocks, free, available | in fragment-size units | |
//! | 6, 7 | files, free files | | |
//! | 8 | name max | `f_namelen` | `pathconf(_PC_NAME_MAX)` |
//! | 9 | flags | bit 0 read-only, bit 1 no set-id | |
//! | 10 | type | `f_type` | `f_type` |
//! | 11 | id | `f_fsid`, its first half high (GNU's `%i`) | |
//!
//! A word the host does not answer is 0 with its `have` bit clear. Other
//! unixes (freebsd: tier-2, a cross-check) answer POSIX's `statvfs`, which
//! has no type; windows answers the space words from ONE
//! `GetDiskFreeSpaceExW`, in bytes (fragment size 1), and nothing else.

use crate::fs::{code_of, fs_code, os_error_clear};

/// The record's words (`[os.fs.statfs]`).
pub mod word {
    pub const HAVE: usize = 0;
    pub const BLOCK_SIZE: usize = 1;
    pub const FRAGMENT_SIZE: usize = 2;
    pub const BLOCKS: usize = 3;
    pub const FREE: usize = 4;
    pub const AVAILABLE: usize = 5;
    pub const FILES: usize = 6;
    pub const FREE_FILES: usize = 7;
    pub const NAME_MAX: usize = 8;
    pub const FLAGS: usize = 9;
    pub const TYPE: usize = 10;
    pub const ID: usize = 11;
    /// The record's length.
    pub const WORDS: usize = 12;
}

/// The flags word's bits (R-s272-2): POSIX's two `statvfs` flags, the same
/// on every host.
pub mod flag {
    /// The file system is mounted read-only (`ST_RDONLY`).
    pub const READ_ONLY: i64 = 1;
    /// Set-user-id and set-group-id bits are ignored (`ST_NOSUID`).
    pub const NO_SET_ID: i64 = 2;
}

/// The record being built: the words and their `have` bits.
struct Rec {
    w: [i64; word::WORDS],
}

impl Rec {
    fn new() -> Rec {
        let mut w = [0i64; word::WORDS];
        w[word::HAVE] = 1 << word::HAVE;
        Rec { w }
    }

    fn set(&mut self, i: usize, v: i64) {
        self.w[i] = v;
        self.w[word::HAVE] |= 1 << i;
    }
}

/// An unsigned host count carried in an `int`'s 64 bits.
#[allow(clippy::cast_possible_wrap)]
fn u(v: u64) -> i64 {
    v as i64
}

/// The two 32-bit halves of a host's file-system id as one word, the
/// first high — the order GNU `stat -f` prints it in (`%i`, measured on
/// hasu: halves `318c2f13` and `8f9ecd45` print `318c2f138f9ecd45`).
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
fn id_of(halves: [i32; 2]) -> i64 {
    u((u64::from(halves[0] as u32) << 32) | u64::from(halves[1] as u32))
}

/// The record of the file system that holds `path` (a final link
/// followed), or the host's refusal.
pub fn record(path: &str) -> Result<[i64; word::WORDS], std::io::Error> {
    sys::record(path)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod sys {
    use super::{Rec, id_of, u, word};

    pub fn record(path: &str) -> Result<[i64; word::WORDS], std::io::Error> {
        let c = std::ffi::CString::new(path)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        // SAFETY: a zeroed statfs the call fills; a NUL-terminated path.
        let s = unsafe {
            let mut s: libc::statfs = std::mem::zeroed();
            if libc::statfs(c.as_ptr(), &mut s) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            s
        };
        let mut r = Rec::new();
        r.set(word::BLOCK_SIZE, s.f_bsize as i64);
        // statfs(2): "f_frsize: fragment size (since Linux 2.6)"; a kernel
        // that leaves it 0 counts in `f_bsize` units (statvfs's rule).
        let frag = if s.f_frsize == 0 {
            s.f_bsize
        } else {
            s.f_frsize
        };
        r.set(word::FRAGMENT_SIZE, frag as i64);
        r.set(word::BLOCKS, u(s.f_blocks as u64));
        r.set(word::FREE, u(s.f_bfree as u64));
        r.set(word::AVAILABLE, u(s.f_bavail as u64));
        r.set(word::FILES, u(s.f_files as u64));
        r.set(word::FREE_FILES, u(s.f_ffree as u64));
        r.set(word::NAME_MAX, s.f_namelen as i64);
        // statfs(2): `f_flags` holds the mount's flags only where the kernel
        // sets ST_VALID (0x20, since 2.6.36); otherwise the bit stays clear.
        const ST_VALID: u64 = 0x20;
        let flags = s.f_flags as u64;
        if flags & ST_VALID != 0 {
            let mut f = 0;
            if flags & libc::ST_RDONLY as u64 != 0 {
                f |= super::flag::READ_ONLY;
            }
            if flags & libc::ST_NOSUID as u64 != 0 {
                f |= super::flag::NO_SET_ID;
            }
            r.set(word::FLAGS, f);
        }
        r.set(word::TYPE, u(s.f_type as u64));
        // SAFETY: fsid_t is two C ints (statfs(2)); its fields are private
        // in the binding, so the eight bytes are read as such.
        let halves: [i32; 2] = unsafe { std::ptr::read((&raw const s.f_fsid).cast()) };
        r.set(word::ID, id_of(halves));
        Ok(r.w)
    }
}

#[cfg(target_vendor = "apple")]
mod sys {
    use super::{Rec, id_of, u, word};

    pub fn record(path: &str) -> Result<[i64; word::WORDS], std::io::Error> {
        let c = std::ffi::CString::new(path)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        // SAFETY: a zeroed statfs the call fills; a NUL-terminated path.
        let s = unsafe {
            let mut s: libc::statfs = std::mem::zeroed();
            if libc::statfs(c.as_ptr(), &mut s) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            s
        };
        let mut r = Rec::new();
        // statfs(2) on macOS: `f_bsize` is the fundamental block size (the
        // unit of the counts), `f_iosize` the optimal transfer size.
        r.set(word::BLOCK_SIZE, i64::from(s.f_iosize));
        r.set(word::FRAGMENT_SIZE, i64::from(s.f_bsize));
        r.set(word::BLOCKS, u(s.f_blocks));
        r.set(word::FREE, u(s.f_bfree));
        r.set(word::AVAILABLE, u(s.f_bavail));
        r.set(word::FILES, u(s.f_files));
        r.set(word::FREE_FILES, u(s.f_ffree));
        // R-s272-3: macOS's statfs carries no name length; POSIX's
        // pathconf is the second call that does.
        // SAFETY: a NUL-terminated path; a plain query.
        let n = unsafe { libc::pathconf(c.as_ptr(), libc::_PC_NAME_MAX) };
        if n > 0 {
            r.set(word::NAME_MAX, n as i64);
        }
        let mut f = 0;
        if u64::from(s.f_flags) & libc::MNT_RDONLY as u64 != 0 {
            f |= super::flag::READ_ONLY;
        }
        if u64::from(s.f_flags) & libc::MNT_NOSUID as u64 != 0 {
            f |= super::flag::NO_SET_ID;
        }
        r.set(word::FLAGS, f);
        r.set(word::TYPE, i64::from(s.f_type));
        // SAFETY: fsid_t is two 32-bit ints (statfs(2)), read as such.
        let halves: [i32; 2] = unsafe { std::ptr::read((&raw const s.f_fsid).cast()) };
        r.set(word::ID, id_of(halves));
        Ok(r.w)
    }
}

#[cfg(all(
    unix,
    not(any(target_os = "linux", target_os = "android", target_vendor = "apple"))
))]
mod sys {
    use super::{Rec, u, word};

    /// POSIX's `statvfs` (freebsd and the other unixes): every word but
    /// the type, which POSIX does not carry.
    pub fn record(path: &str) -> Result<[i64; word::WORDS], std::io::Error> {
        let c = std::ffi::CString::new(path)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        // SAFETY: a zeroed statvfs the call fills; a NUL-terminated path.
        let s = unsafe {
            let mut s: libc::statvfs = std::mem::zeroed();
            if libc::statvfs(c.as_ptr(), &mut s) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            s
        };
        let mut r = Rec::new();
        r.set(word::BLOCK_SIZE, s.f_bsize as i64);
        r.set(word::FRAGMENT_SIZE, s.f_frsize as i64);
        r.set(word::BLOCKS, u(s.f_blocks as u64));
        r.set(word::FREE, u(s.f_bfree as u64));
        r.set(word::AVAILABLE, u(s.f_bavail as u64));
        r.set(word::FILES, u(s.f_files as u64));
        r.set(word::FREE_FILES, u(s.f_ffree as u64));
        r.set(word::NAME_MAX, s.f_namemax as i64);
        let mut f = 0;
        if s.f_flag as u64 & libc::ST_RDONLY as u64 != 0 {
            f |= super::flag::READ_ONLY;
        }
        if s.f_flag as u64 & libc::ST_NOSUID as u64 != 0 {
            f |= super::flag::NO_SET_ID;
        }
        r.set(word::FLAGS, f);
        r.set(word::ID, u(s.f_fsid as u64));
        Ok(r.w)
    }
}

#[cfg(windows)]
mod sys {
    use super::{Rec, u, word};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            dir: *const u16,
            avail_to_caller: *mut u64,
            total: *mut u64,
            total_free: *mut u64,
        ) -> i32;
    }

    /// R-s272-4: the space words from ONE `GetDiskFreeSpaceExW`, in bytes
    /// (fragment size 1); the call takes a directory, so a file's own
    /// directory is asked for it.
    pub fn record(path: &str) -> Result<[i64; word::WORDS], std::io::Error> {
        use std::os::windows::ffi::OsStrExt as _;
        let p = std::path::Path::new(path);
        let md = std::fs::metadata(p)?;
        let dir = if md.is_dir() {
            p.to_path_buf()
        } else {
            match p.parent() {
                Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
                _ => std::path::PathBuf::from("."),
            }
        };
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        // SAFETY: a NUL-terminated wide path and three out words we own.
        if unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut r = Rec::new();
        r.set(word::FRAGMENT_SIZE, 1);
        r.set(word::BLOCKS, u(total));
        r.set(word::FREE, u(free));
        r.set(word::AVAILABLE, u(avail));
        Ok(r.w)
    }
}

/// `fs_statfs(path) -> List[int] ! {denied, io, not_found}`
/// (`[os.fs.statfs]`) — the twelve words ([`record`]) from one host call
/// (macOS: two, the name length's `pathconf`); the row is `fs_stat`'s, and
/// the host's number stays beside a refusal (`[os.fs.error]`).
///
/// # Safety
///
/// A valid str pair; `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_fs_statfs(pp: i64, pl: i64, out: i64) -> i64 {
    os_error_clear();
    let path = unsafe { crate::str::view(pp, pl) };
    let rec = match record(path) {
        Err(e) => return code_of(&e),
        Ok(r) => r,
    };
    let hdr = crate::list::new_list(8);
    for w in rec {
        crate::list::push_int(hdr, w);
    }
    unsafe { crate::str::write_word(out, hdr as i64) };
    fs_code::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The record of the directory the tests run in: the space words
    /// answered and ordered, a positive unit.
    #[test]
    fn the_record_of_the_current_directory() {
        let r = record(".").expect("the current directory has a file system");
        let have = r[word::HAVE];
        for i in [
            word::HAVE,
            word::FRAGMENT_SIZE,
            word::BLOCKS,
            word::FREE,
            word::AVAILABLE,
        ] {
            assert_ne!(have & (1 << i), 0, "word {i} answered: {r:?}");
        }
        assert!(r[word::FRAGMENT_SIZE] > 0, "{r:?}");
        assert!(r[word::FREE] <= r[word::BLOCKS], "{r:?}");
        assert!(r[word::AVAILABLE] <= r[word::FREE], "{r:?}");
        // linux answers every word (the flags where the kernel says they
        // are valid); macOS every word.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        assert_eq!(have | (1 << word::FLAGS), 0xfff, "{r:?}");
        #[cfg(target_vendor = "apple")]
        assert_eq!(have, 0xfff, "{r:?}");
    }

    /// A missing path is the host's NotFound, which the shim answers as
    /// `not_found` with the number beside it.
    #[test]
    fn a_missing_path_is_not_found() {
        let e = record("wolf-s272-no-such-dir/inner").expect_err("missing");
        assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(code_of(&e), fs_code::NOT_FOUND);
    }

    /// A path holding a NUL names nothing a host call can take: `io`.
    #[cfg(unix)]
    #[test]
    fn a_nul_in_the_path_is_io() {
        let e = record("a\0b").expect_err("NUL");
        assert_eq!(code_of(&e), fs_code::IO);
    }

    /// GNU's order for the id: the first half high (measured on hasu).
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    #[test]
    fn the_id_is_gnus_order() {
        assert_eq!(
            id_of([0x318c_2f13, 0x8f9e_cd45_u32 as i32]),
            0x318c_2f13_8f9e_cd45_u64 as i64
        );
        assert_eq!(id_of([0x98, 0]), 0x98_0000_0000);
    }
}
