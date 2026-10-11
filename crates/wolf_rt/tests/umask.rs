//! s230 (wolf-lang#704, #346): `os_umask`, and the two creations that
//! take a mode — `[os.fs.umask]`, `[os.fs.create.mode]`.
//!
//! **Why a test binary of its own.** The file mode creation mask is the
//! PROCESS's: a test that sets it changes the mode of every file any
//! other thread of the same process creates meanwhile. Cargo runs each
//! file under `tests/` as its own process, so nothing outside this file
//! can see a mask set here; inside it every test takes [`MASK`], so no
//! two run side by side; and each puts back the mask it found before it
//! lets go. No test asks who it runs as, and each works inside a
//! directory it made.

#![cfg(unix)]

use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use wolf_rt::fs::{__wolf_rt_fs_close, __wolf_rt_os_error, fs_code};
use wolf_rt::fs_change::{
    __wolf_rt_fs_create_dir_mode, __wolf_rt_fs_open_perm, __wolf_rt_fs_set_mode, __wolf_rt_os_umask,
};

/// Held by every test in this binary: the mask is one word per process.
static MASK: Mutex<()> = Mutex::new(());

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("wolf-rt-s230-umask-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn pair_of(s: &str) -> (i64, i64) {
    (s.as_ptr() as i64, s.len() as i64)
}

fn mode_of(p: &Path) -> i64 {
    i64::from(std::fs::metadata(p).unwrap().mode() & 0o7777)
}

fn open_perm(p: &Path, open: i64, mode: i64) -> i64 {
    let s = p.display().to_string();
    let (pp, pl) = pair_of(&s);
    unsafe { __wolf_rt_fs_open_perm(pp, pl, open, mode) }
}

fn mkdir_mode(p: &Path, mode: i64) -> i64 {
    let s = p.display().to_string();
    let (pp, pl) = pair_of(&s);
    unsafe { __wolf_rt_fs_create_dir_mode(pp, pl, mode) }
}

/// The call answers the mask it replaced, and a read is two calls.
#[test]
fn umask_answers_the_mask_it_replaced() {
    let _g = MASK.lock().unwrap_or_else(|p| p.into_inner());
    let first = __wolf_rt_os_umask(0o027);
    assert!((0..=0o777).contains(&first), "a mask: {first:o}");
    assert_eq!(__wolf_rt_os_umask(0o077), 0o027);
    assert_eq!(__wolf_rt_os_umask(first), 0o077);
    // A mask outside the nine bits changes nothing.
    assert_eq!(__wolf_rt_os_umask(0o1000), -fs_code::INVALID);
    assert_eq!(__wolf_rt_os_umask(first), first);
}

/// A creation gets its mode less the mask, from the first instant; an
/// existing file keeps its own; the plain spellings ask for 0o666 and
/// 0o777.
#[test]
fn a_creation_takes_its_mode_less_the_mask() {
    let _g = MASK.lock().unwrap_or_else(|p| p.into_inner());
    let d = scratch("create");
    let old = __wolf_rt_os_umask(0o027);

    let private = d.join("private");
    let fd = open_perm(&private, 4, 0o600);
    assert!(fd >= 3, "a handle: {fd}");
    assert_eq!(mode_of(&private), 0o600);
    assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);

    // 0o777 less 0o027.
    let exe = d.join("exe");
    let fd = open_perm(&exe, 1, 0o777);
    assert_eq!(mode_of(&exe), 0o750);
    assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);

    // An existing file keeps its mode whatever the call asks.
    let fd = open_perm(&private, 1, 0o777);
    assert_eq!(mode_of(&private), 0o600);
    assert_eq!(__wolf_rt_fs_close(fd), fs_code::OK);

    // Create-new on a path that is there is `exists`, with the host's
    // number beside it.
    assert_eq!(open_perm(&private, 4, 0o600), -fs_code::EXISTS);
    assert_eq!(__wolf_rt_os_error(), i64::from(libc::EEXIST));
    // A read open never creates, whatever the mode.
    assert_eq!(open_perm(&d.join("absent"), 0, 0o600), -fs_code::NOT_FOUND);

    let sub = d.join("sub");
    assert_eq!(mkdir_mode(&sub, 0o700), fs_code::OK);
    assert_eq!(mode_of(&sub), 0o700);
    let wide = d.join("wide");
    assert_eq!(mkdir_mode(&wide, 0o777), fs_code::OK);
    assert_eq!(mode_of(&wide), 0o750);
    assert_eq!(mkdir_mode(&sub, 0o700), fs_code::EXISTS);
    assert_eq!(mkdir_mode(&d.join("no/such"), 0o700), fs_code::NOT_FOUND);

    assert_eq!(__wolf_rt_os_umask(old), 0o027);
    let _ = std::fs::remove_dir_all(&d);
}

/// `mkdir -p` under a mask that takes the owner's write bit: the parent
/// is created 0o500 and nothing can be made inside it — until the mask
/// is lifted for the parent, or its mode is set after (wolf-lang#704's
/// witness, as a runtime test).
#[test]
fn a_mask_that_takes_owner_write_is_survivable() {
    let _g = MASK.lock().unwrap_or_else(|p| p.into_inner());
    let d = scratch("mkdirp");
    let old = __wolf_rt_os_umask(0o277);

    let a = d.join("a");
    assert_eq!(mkdir_mode(&a, 0o777), fs_code::OK);
    assert_eq!(mode_of(&a), 0o500);
    // The repair GNU's `mkdir -p` makes: u+wx on the parent.
    let s = a.display().to_string();
    let (pp, pl) = pair_of(&s);
    assert_eq!(unsafe { __wolf_rt_fs_set_mode(pp, pl, 0o700) }, fs_code::OK);
    assert_eq!(mkdir_mode(&a.join("b"), 0o777), fs_code::OK);
    assert_eq!(mode_of(&a.join("b")), 0o500);

    assert_eq!(__wolf_rt_os_umask(old), 0o277);
    let _ = std::fs::remove_dir_all(&d);
}
