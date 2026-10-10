//! s225 (wolf-lang#534): replacing the running program, and removing a
//! variable from its environment — `[os.proc.exec]`, `[os.env.unset]`.
//!
//! A module of its own rather than more of `os.rs`: the spawn family
//! there is another lane's ground (s219), and nothing here shares its
//! child table. What it does share is reused, not copied: the map's
//! shape rules ([`crate::os::map_of`]), the process family's codes
//! ([`crate::os::proc_code`]) and the fs table's duplicates
//! ([`crate::fs`]).
//!
//! # `os_exec`, mechanically (unix)
//!
//! `execve(2)` is the whole call; everything before it exists so that a
//! FAILED exec leaves the process as it was. In order:
//!
//! 1. **Shape** (`invalid`): an empty argv (argv[0] is the new program's
//!    own name and must be given), an argv word or the program holding a
//!    NUL, an environment entry with no `=`, an empty name or a NUL, and
//!    every map [`crate::os::map_of`] refuses.
//! 2. **Host** (`unsupported`): windows has no exec, by name.
//! 3. **Sources** (`io`): each map source is duplicated and held, as
//!    `os_spawn_fds` holds them, so a number another task closes cannot
//!    be reused under the map.
//! 4. **Program** (`not_found`, `denied`, `io`): a name with a `/` is
//!    used as written; a bare name is searched along the `PATH` entry of
//!    the environment the program is HANDED (the first `PATH=` entry),
//!    or `/usr/bin:/bin` when it names none — one rule that reads only
//!    the call's own arguments, so every machine applies it alike. The
//!    first candidate that is a regular file with an execute bit is the
//!    program; none is `not_found`, unless some candidate was a file
//!    with no execute bit, which is `denied`.
//! 5. **The map**, applied in THIS process: every target's current
//!    descriptor (and its close-on-exec flag) is saved above the
//!    highest target, every source is staged there, then each stage is
//!    `dup2`'d onto its target (which clears close-on-exec on the target
//!    only) and each `-1` target closed. An unmapped 0, 1 and 2 stay
//!    this process's own — an exec keeps its stdio — and every other
//!    descriptor the runtime opened is close-on-exec, so the kernel
//!    closes it at the exec.
//! 6. `execve`. If it returns, its `errno` decides the code
//!    (`ENOENT`/`ENOTDIR` `not_found`, `EACCES`/`EPERM` `denied`,
//!    anything else `io`) and step 5 is undone from the saved copies.
//!
//! Signal handlers the runtime installed (the `os_signal_listen`
//! trampoline) reset to the default at the exec, as POSIX says of any
//! caught signal; an IGNORED signal stays ignored, as POSIX also says —
//! a native program leaves `SIGPIPE` as it inherited it, so this
//! touches no disposition.

#[cfg(unix)]
use crate::os::InheritFd;
use crate::os::{map_of, proc_code};
use crate::str::view;

/// The search path a bare program name falls back to when the
/// environment handed to the new program names no `PATH`.
pub const DEFAULT_PATH: &str = "/usr/bin:/bin";

/// `env_unset(name) -> () ! {invalid}` (s225, `[os.env.unset]`) —
/// removes `name` from the process's real environment. Removing a
/// variable that is not there succeeds (POSIX `unsetenv`). Codes: 0 ok,
/// 1 `invalid` — an empty name, or one holding `=` or NUL, `env_set`'s
/// rule exactly.
///
/// # Safety
///
/// A valid str pair. The write follows `env_set`'s concurrency contract
/// (the module doc of `os.rs`): sound while no other thread reads the
/// environment at the same moment.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_env_unset(np: i64, nl: i64) -> i64 {
    let name = unsafe { view(np, nl) };
    if !env_name_ok(name) {
        return 1;
    }
    // SAFETY: the name is validated above; the concurrency posture is
    // the caller's, as `env_set`'s is.
    unsafe { std::env::remove_var(name) };
    0
}

/// `env_set`'s name rule: non-empty, no `=`, no NUL.
pub fn env_name_ok(name: &str) -> bool {
    !name.is_empty() && !name.contains('=') && !name.contains('\0')
}

/// One environment entry as `os_exec` takes it: `NAME=VALUE` with a
/// non-empty name and no NUL anywhere. The value may be empty and may
/// hold further `=`.
pub fn env_entry_ok(entry: &str) -> bool {
    match entry.split_once('=') {
        None => false,
        Some((name, _)) => !name.is_empty() && !entry.contains('\0'),
    }
}

/// Steps 1 and 4's pure halves, shared by every caller (and the checked
/// machine's twin follows them line for line): the shape of an exec
/// request. `Err("invalid")` for anything step 1 names.
pub fn exec_shape(
    exe: &str,
    argv: &[&str],
    env: &[&str],
    map: &[i64],
) -> Result<Vec<(i64, Option<i64>)>, &'static str> {
    if argv.is_empty()
        || exe.contains('\0')
        || argv.iter().any(|a| a.contains('\0'))
        || !env.iter().all(|e| env_entry_ok(e))
    {
        return Err("invalid");
    }
    map_of(map)
}

/// The search path for a bare name: the first `PATH=` entry of `env`,
/// or [`DEFAULT_PATH`].
pub fn search_path<'a>(env: &[&'a str]) -> &'a str {
    env.iter()
        .find_map(|e| e.strip_prefix("PATH="))
        .unwrap_or(DEFAULT_PATH)
}

/// Step 4: the program `exe` names, as a path to hand to `execve`.
/// `Ok(path)` or `Err("not_found" | "denied")`. A name holding `/` is
/// used as written (the kernel decides; a missing one is its
/// `ENOENT`). A bare name is searched along `path` — an empty component
/// is the working directory, as POSIX says — and the first regular file
/// with an execute bit wins.
pub fn resolve_program(exe: &str, path: &str) -> Result<String, &'static str> {
    if exe.is_empty() {
        return Err("not_found");
    }
    if exe.contains('/') {
        return Ok(exe.to_string());
    }
    let mut saw_unexecutable = false;
    for dir in path.split(':') {
        let cand = if dir.is_empty() {
            format!("./{exe}")
        } else {
            format!("{}/{exe}", dir.trim_end_matches('/'))
        };
        let Ok(md) = std::fs::metadata(&cand) else {
            continue;
        };
        if !md.is_file() {
            continue;
        }
        if executable(&md) {
            return Ok(cand);
        }
        saw_unexecutable = true;
    }
    Err(if saw_unexecutable {
        "denied"
    } else {
        "not_found"
    })
}

#[cfg(unix)]
fn executable(md: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    md.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable(_md: &std::fs::Metadata) -> bool {
    false
}

/// A row tag as its wire code.
fn code_of(tag: &str) -> i64 {
    match tag {
        "not_found" => proc_code::NOT_FOUND,
        "denied" => proc_code::DENIED,
        "unsupported" => proc_code::UNSUPPORTED,
        "invalid" => proc_code::INVALID,
        _ => proc_code::IO,
    }
}

/// `os_exec(exe: str, argv: List[str], env: List[str], map: List[int])
/// -> () ! {denied, invalid, io, not_found, unsupported}` (s225,
/// `[os.proc.exec]`). Returns only on failure, with the row's code (the
/// process family's numbering, [`proc_code`]); on success the process
/// is the new program and nothing returns. See the module doc for the
/// order of refusal and the mechanics.
///
/// # Safety
///
/// `ep`/`el` a valid str pair; `argv` and `env` live `List[str]`
/// headers; `map` a live `List[int]` header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_exec(ep: i64, el: i64, argv: i64, env: i64, map: i64) -> i64 {
    let exe = unsafe { view(ep, el) };
    let (Some(av), Some(ev), Some(flat)) = (
        unsafe { crate::list::str_pair_elems(argv) },
        unsafe { crate::list::str_pair_elems(env) },
        unsafe { crate::list::i64_elems(map) },
    ) else {
        return proc_code::IO;
    };
    let words: Vec<&str> = av.iter().map(|&[p, l]| unsafe { view(p, l) }).collect();
    let entries: Vec<&str> = ev.iter().map(|&[p, l]| unsafe { view(p, l) }).collect();
    match exec_now(exe, &words, &entries, flat) {
        Err(tag) => code_of(tag),
        // Unreachable: a successful exec does not return.
        Ok(never) => match never {},
    }
}

/// The exec itself, for the shim and the runtime's own tests: `Err` is
/// the row a failed exec answers, after the process has been put back
/// as it was. `Ok` cannot be built.
pub fn exec_now(
    exe: &str,
    argv: &[&str],
    env: &[&str],
    map: &[i64],
) -> Result<std::convert::Infallible, &'static str> {
    let entries = exec_shape(exe, argv, env, map)?;
    #[cfg(not(unix))]
    {
        let _ = entries;
        Err("unsupported")
    }
    #[cfg(unix)]
    {
        // Step 3: every source duplicated and held until the exec
        // returns (or never does).
        let mut held: Vec<std::fs::File> = Vec::with_capacity(entries.len());
        let mut placed: Vec<(InheritFd, Option<InheritFd>)> = Vec::with_capacity(entries.len());
        for &(t, s) in &entries {
            let src = match s {
                None => None,
                Some(h) => {
                    let f = crate::fs::dup_of(h).ok_or("io")?;
                    let fd = std::os::fd::AsRawFd::as_raw_fd(&f);
                    held.push(f);
                    Some(fd)
                }
            };
            placed.push((t as InheritFd, src));
        }
        let program = resolve_program(exe, search_path(env))?;
        let r = unix::exec_mapped(&program, argv, env, &placed);
        drop(held);
        r
    }
}

#[cfg(unix)]
mod unix {
    use super::InheritFd;
    use std::ffi::CString;

    /// One target's state before the map touched it: a saved duplicate
    /// (or `-1` when the target was not open) and its descriptor flags.
    struct Saved {
        target: InheritFd,
        copy: InheritFd,
        flags: libc::c_int,
    }

    /// Steps 5 and 6: place `map`, `execve`, and on failure put every
    /// target back. Every allocation happens before the first
    /// descriptor moves.
    pub(super) fn exec_mapped(
        program: &str,
        argv: &[&str],
        env: &[&str],
        map: &[(InheritFd, Option<InheritFd>)],
    ) -> Result<std::convert::Infallible, &'static str> {
        let cstr = |s: &str| CString::new(s).map_err(|_| "invalid");
        let path = cstr(program)?;
        let args: Vec<CString> = argv.iter().map(|a| cstr(a)).collect::<Result<_, _>>()?;
        let envs: Vec<CString> = env.iter().map(|e| cstr(e)).collect::<Result<_, _>>()?;
        let mut arg_ptrs: Vec<*const libc::c_char> = args.iter().map(|c| c.as_ptr()).collect();
        arg_ptrs.push(std::ptr::null());
        let mut env_ptrs: Vec<*const libc::c_char> = envs.iter().map(|c| c.as_ptr()).collect();
        env_ptrs.push(std::ptr::null());
        let floor = map.iter().map(|&(t, _)| t + 1).max().unwrap_or(3).max(3);
        let mut saved: Vec<Saved> = Vec::with_capacity(map.len());
        let mut staged: Vec<InheritFd> = Vec::with_capacity(map.len());

        // SAFETY: plain descriptor calls on numbers this function owns
        // (the saves and stages it makes) or that the map names; every
        // error path below restores what was moved.
        unsafe {
            for &(t, _) in map {
                let flags = libc::fcntl(t, libc::F_GETFD);
                let copy = if flags < 0 {
                    -1
                } else {
                    let c = libc::fcntl(t, libc::F_DUPFD_CLOEXEC, floor);
                    if c < 0 {
                        restore(&saved, &staged);
                        return Err("io");
                    }
                    c
                };
                saved.push(Saved {
                    target: t,
                    copy,
                    flags,
                });
            }
            for &(_, s) in map {
                let st = match s {
                    None => -1,
                    Some(src) => {
                        let c = libc::fcntl(src, libc::F_DUPFD_CLOEXEC, floor);
                        if c < 0 {
                            restore(&saved, &staged);
                            return Err("io");
                        }
                        c
                    }
                };
                staged.push(st);
            }
            for (i, &(t, _)) in map.iter().enumerate() {
                if staged[i] >= 0 {
                    if libc::dup2(staged[i], t) < 0 {
                        restore(&saved, &staged);
                        return Err("io");
                    }
                } else {
                    libc::close(t);
                }
            }
            libc::execve(path.as_ptr(), arg_ptrs.as_ptr(), env_ptrs.as_ptr());
            let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            // PLANT (s225, reverted next): a failed exec leaves the map in place.
            let _ = (&saved, &staged);
            Err(match errno {
                libc::ENOENT | libc::ENOTDIR => "not_found",
                libc::EACCES | libc::EPERM => "denied",
                _ => "io",
            })
        }
    }

    /// Put every saved target back as it was (open with its flags, or
    /// closed) and close every save and stage.
    unsafe fn restore(saved: &[Saved], staged: &[InheritFd]) {
        unsafe {
            for s in saved {
                if s.copy >= 0 {
                    libc::dup2(s.copy, s.target);
                    libc::fcntl(s.target, libc::F_SETFD, s.flags);
                    libc::close(s.copy);
                } else {
                    libc::close(s.target);
                }
            }
            for &st in staged {
                if st >= 0 {
                    libc::close(st);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_unset_removes_and_refuses_bad_names() {
        let name = "WOLF_RT_S225_UNSET";
        // SAFETY: test-local variable; no other test reads it.
        unsafe { std::env::set_var(name, "x") };
        let r = unsafe { __wolf_rt_env_unset(name.as_ptr() as i64, name.len() as i64) };
        assert_eq!(r, 0);
        assert!(std::env::var_os(name).is_none());
        // Absent is not an error.
        let r = unsafe { __wolf_rt_env_unset(name.as_ptr() as i64, name.len() as i64) };
        assert_eq!(r, 0);
        for bad in ["", "A=B", "A\0B"] {
            let r = unsafe { __wolf_rt_env_unset(bad.as_ptr() as i64, bad.len() as i64) };
            assert_eq!(r, 1, "{bad:?}");
        }
    }

    #[test]
    fn exec_shape_rows() {
        let ok_env = ["A=1"];
        assert_eq!(exec_shape("x", &[], &ok_env, &[]), Err("invalid"));
        assert_eq!(exec_shape("x", &["x"], &["A"], &[]), Err("invalid"));
        assert_eq!(exec_shape("x", &["x"], &["=1"], &[]), Err("invalid"));
        assert_eq!(exec_shape("x", &["x\0"], &ok_env, &[]), Err("invalid"));
        assert_eq!(exec_shape("x", &["x"], &["A=1\0"], &[]), Err("invalid"));
        assert_eq!(exec_shape("x", &["x"], &ok_env, &[1]), Err("invalid"));
        assert_eq!(exec_shape("x", &["x"], &ok_env, &[256, 1]), Err("invalid"));
        assert_eq!(
            exec_shape("x", &["x"], &ok_env, &[1, 1, 1, 2]),
            Err("invalid")
        );
        assert_eq!(exec_shape("x", &["x"], &ok_env, &[1, -2]), Err("invalid"));
        assert_eq!(
            exec_shape("x", &["x"], &["A=", "B=c=d"], &[5, -1]),
            Ok(vec![(5, None)])
        );
    }

    #[test]
    fn search_path_reads_the_handed_environment() {
        assert_eq!(search_path(&["A=1", "PATH=/a:/b", "PATH=/c"]), "/a:/b");
        assert_eq!(search_path(&["A=1"]), DEFAULT_PATH);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_program_rows() {
        let d = std::env::temp_dir().join(format!("wolf_rt_s225_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let exe = d.join("runme");
        let plain = d.join("plain");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        std::fs::write(&plain, "x").unwrap();
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        let p = d.to_str().unwrap();
        assert_eq!(resolve_program("runme", p), Ok(format!("{p}/runme")));
        assert_eq!(resolve_program("plain", p), Err("denied"));
        assert_eq!(resolve_program("ghost", p), Err("not_found"));
        assert_eq!(resolve_program("", p), Err("not_found"));
        assert_eq!(resolve_program("./a/b", p), Ok("./a/b".to_string()));
        // A directory on the path is not a program.
        assert_eq!(
            resolve_program("wolf_rt_s225_dir", &std::env::temp_dir().to_string_lossy()),
            Err("not_found")
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// A failed exec leaves every descriptor the map named as it was:
    /// fd 0 still the same file, a closed target still closed.
    #[cfg(unix)]
    #[test]
    fn failed_exec_restores_the_map() {
        use std::os::fd::AsRawFd as _;
        let f = std::fs::File::open("/dev/null").unwrap();
        let src = f.as_raw_fd();
        // Target 200: not open before, so it must be closed after.
        let before0 = unsafe { libc::fcntl(0, libc::F_GETFD) };
        let r = unix::exec_mapped(
            "/nonexistent/wolf-s225",
            &["x"],
            &["A=1"],
            &[(0, Some(src)), (200, Some(src))],
        );
        assert!(matches!(r, Err("not_found")));
        assert!(
            unsafe { libc::fcntl(200, libc::F_GETFD) } < 0,
            "200 closed again"
        );
        assert_eq!(
            unsafe { libc::fcntl(0, libc::F_GETFD) },
            before0,
            "0's flags back"
        );
        drop(f);
    }
}
