//! The s40 native os/env runtime — argv, environment, cwd, exit.
//!
//! Semantics are `ubcheck.rs`'s `os_builtin`, entry for entry, with
//! the one DOCUMENTED lane asymmetry: the checked machine's `env_set`
//! writes a machine-local overlay (it runs inside a threaded test
//! host where `setenv` is unsound), while this runtime writes the
//! compiled program's own real environment — the program owns its
//! process. Everything else mirrors: `env_vars` is sorted `K=V` with
//! non-UTF-8 entries skipped, `env_get` rows are `missing`/`utf8`,
//! `env_set` rejects `=`/NUL/empty names as `invalid`, argv drops the
//! program name.
//!
//! The process trio (`os_spawn`/`os_wait`/`os_kill`) crossed at s107
//! (c26's last crossing, wolf-lang#118): a [`ChildTable`] over
//! `std::process` — dense i64 handles into an id-keyed store (the
//! NetTable shape), never raw pids, so a forged handle can never
//! alias a foreign OS process. Argv is array-only, straight from the
//! `List[str]` header (no shell-string spawn exists anywhere, by
//! construction). Child stdio (s111, wolf-lang#129/F-0065): stdout
//! and stderr INHERIT the parent's — the child writes through, so a
//! parent whose stdout is captured (conform-run's pipe, a test rig)
//! sees the child's output in its own stream; stdin stays null-wired
//! (a child never consumes the parent's input; it reads immediate
//! EOF). Capture-to-string handles remain the named upstream ask on
//! #129 — no capture surface is declared (s40/s107), and none is
//! invented here. The checked lane's `os_builtin` mirrors this
//! wiring entry for entry.
//!
//! # Zombie discipline (the reviewer flag)
//!
//! `os_wait` REAPS: `Child::wait` collects the OS exit status, and a
//! successful wait tombstones the slot (double wait is `io`).
//! `os_kill` does NOT tombstone — kill-then-wait is the documented
//! reap path (the checked lane's own posture), so a killed child is
//! never stranded unreapable behind a dead handle. A program that
//! kills and never waits leaves the reap to process exit, exactly as
//! the checked machine does; the TESTS wait — never sample — per the
//! #50 lesson, and the kill test below proves the kill-then-wait
//! sequence leaves no zombie behind.
//!
//! Error codes per entry (lowering maps them to row tags):
//! `env_get`: 0 ok, 1 missing, 2 utf8. `env_set`: 0 ok, 1 invalid.
//! `os_cwd`, `os_exe` (s90/#69): 0 ok, 1 io. The process trio:
//! [`proc_code`].

use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use crate::list::push_str;
use crate::str::{ambient_copy, view, write_pair, write_word};

/// `env_args() -> List[str]` — the program's arguments, program name
/// dropped (argv[0] is the binary's path, not the program's input).
/// Non-UTF-8 arguments are skipped (unreachable through the str
/// tier).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_env_args() -> i64 {
    let hdr = crate::list::new_list(16);
    for a in std::env::args().skip(1) {
        push_str(hdr, &a);
    }
    hdr as i64
}

/// `env_get(name) -> str ! {missing, utf8}`.
///
/// # Safety
///
/// A valid str pair; `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_env_get(np: i64, nl: i64, out: i64) -> i64 {
    let name = unsafe { view(np, nl) };
    match std::env::var(name) {
        Ok(v) => {
            let p = ambient_copy(v.as_bytes());
            unsafe { write_pair(out, p as i64, v.len() as i64) };
            0
        }
        Err(std::env::VarError::NotPresent) => 1,
        Err(std::env::VarError::NotUnicode(_)) => 2,
    }
}

/// `env_set(name, value) -> () ! {invalid}` — writes the process's
/// real environment (see the module doc's lane-asymmetry note).
///
/// # Safety
///
/// Both pairs must be valid str pairs. The write itself follows the
/// platform `setenv` contract: sound while no other thread reads the
/// environment concurrently — tasks that race `env_get` against
/// `env_set` are a program-owned data race, and the checked lane's
/// overlay is the racefree reference the std facade may later adopt
/// runtime-wide.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_env_set(np: i64, nl: i64, vp: i64, vl: i64) -> i64 {
    let (name, value) = unsafe { (view(np, nl), view(vp, vl)) };
    if name.is_empty() || name.contains('=') || name.contains('\0') || value.contains('\0') {
        return 1;
    }
    // SAFETY: name/value validated above; concurrency posture is the
    // caller's per the fn-level contract.
    unsafe { std::env::set_var(name, value) };
    0
}

/// `env_vars() -> List[str]` — `K=V` lines, SORTED (determinism over
/// environ order), non-UTF-8 entries skipped.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_env_vars() -> i64 {
    let mut vars: Vec<String> = std::env::vars_os()
        .filter_map(|(k, v)| {
            Some(format!(
                "{}={}",
                k.into_string().ok()?,
                v.into_string().ok()?
            ))
        })
        .collect();
    vars.sort();
    let hdr = crate::list::new_list(16);
    for kv in &vars {
        push_str(hdr, kv);
    }
    hdr as i64
}

/// `os_cwd() -> str ! {io}` — a non-UTF-8 cwd is `io` (unreachable
/// through the str tier, the fs coarsening rule).
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_cwd(out: i64) -> i64 {
    match std::env::current_dir() {
        Err(_) => 1,
        Ok(p) => match p.to_str() {
            None => 1,
            Some(s) => {
                let cp = ambient_copy(s.as_bytes());
                unsafe { write_pair(out, cp as i64, s.len() as i64) };
                0
            }
        },
    }
}

/// `os_exe() -> str ! {io}` (s90, wolf-lang#69) — the RUNNING
/// executable's path. Portable on every tier-1 target through
/// `std::env::current_exe` (procfs on linux, `_NSGetExecutablePath` on
/// macOS, `GetModuleFileNameW` on windows); a non-UTF-8 or
/// unrepresentable answer is `io`, the same coarsening `os_cwd` uses.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_exe(out: i64) -> i64 {
    match std::env::current_exe() {
        Err(_) => 1,
        Ok(p) => match p.to_str() {
            None => 1,
            Some(s) => {
                let cp = ambient_copy(s.as_bytes());
                unsafe { write_pair(out, cp as i64, s.len() as i64) };
                0
            }
        },
    }
}

/// `os_exit(code)` — immediate termination, code masked to the
/// process range exactly as the checked lane masks it
/// (`rem_euclid(256)`); defers do NOT run (the documented contract).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_exit(code: i64) -> ! {
    // `main` never returns through here, so the compiler's dump before
    // `ret` cannot fire; this is the s45 counterpart. No-op in a
    // normal build.
    crate::prof::dump_on_exit();
    std::process::exit(code.rem_euclid(256) as i32)
}

// ------------------------- s107: the process trio (wolf-lang#118) --

/// A process operation's failure: the row tag it raises. The
/// vocabulary is `{not_found, denied, signal, io}` — `not_found` and
/// `denied` at spawn (rule 3: anything else coarsens to `io`),
/// `signal` a child that died without an exit code, `io` every
/// operation on a forged or reaped handle (a checkable condition,
/// never a trap).
pub type ProcErr = &'static str;

/// The child table: index = the `int` handle wolf code holds; `None`
/// after a successful wait (the reap tombstones; double wait is
/// `io`). `kill` does NOT tombstone: kill-then-wait is the reap path
/// (see the module doc's zombie discipline). Deliberately NOT the OS
/// pid — dense small ints into this table, the NetTable/fs shape.
#[derive(Debug, Default)]
pub struct ChildTable {
    children: Vec<Option<Child>>,
}

impl ChildTable {
    /// `const` so the shim tier's process table ([`CHILDREN`]) can
    /// live in a `static Mutex` without lazy-init machinery (the
    /// fs/net precedent).
    pub const fn new() -> ChildTable {
        ChildTable {
            children: Vec::new(),
        }
    }

    /// Spawn `argv[0]` with `argv[1..]` — stdout/stderr inherited
    /// (write-through, #129), stdin null-wired. An empty
    /// argv names no program: `not_found`. Spawn failures map
    /// `NotFound`/`PermissionDenied` and coarsen the rest to `io` —
    /// the checked lane's exact table.
    pub fn spawn(&mut self, argv: &[&str]) -> Result<i64, ProcErr> {
        let Some((prog, rest)) = argv.split_first() else {
            return Err("not_found");
        };
        let spawned = Command::new(prog)
            .args(rest)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn();
        match spawned {
            Err(e) => Err(match e.kind() {
                std::io::ErrorKind::NotFound => "not_found",
                std::io::ErrorKind::PermissionDenied => "denied",
                _ => "io",
            }),
            Ok(child) => {
                let h = self.children.len() as i64;
                self.children.push(Some(child));
                Ok(h)
            }
        }
    }

    /// `os_spawn_with(exe, args, inherit)` (s137, #235,
    /// `[os.proc.inherit]`): [`ChildTable::spawn`] with an inherit set —
    /// OS descriptors (already mapped from this process's net handles
    /// by the shim) the child receives as **3, 4, …** in the order
    /// given. The numbering is the contract: a parent tells its child
    /// "the listener is 3" without learning any descriptor number
    /// itself, and the child's `net_adopt_listener(3)` is a real
    /// listener on the same port. Mechanics (unix): a `pre_exec` hook
    /// in the forked child STAGES every source above the target range
    /// (`F_DUPFD` from `3 + n`, so a source that already sits at some
    /// target number is never clobbered by an earlier `dup2`), then
    /// `dup2`s each stage onto its target (which clears close-on-exec
    /// on the target and only there) and closes the stage; the
    /// parent's own descriptors stay close-on-exec and vanish at exec.
    /// The hook runs after `fork` in a multithreaded process, so it
    /// allocates nothing: the set is bounded at [`MAX_INHERIT`], and a
    /// larger one is `io` before any child exists. Stdio is
    /// [`ChildTable::spawn`]'s. Windows: a non-empty set is
    /// `unsupported`, by name (handle inheritance exists there and
    /// the runtime's test suite measures it on the runner, but a
    /// `SOCKET` is not a small stable number the parent can name in
    /// argv — the serving rung is docs/platforms.md's).
    #[cfg_attr(not(unix), allow(unused_variables))]
    pub fn spawn_with(
        &mut self,
        exe: &str,
        args: &[&str],
        inherit: &[InheritFd],
    ) -> Result<i64, ProcErr> {
        if exe.is_empty() {
            return Err("not_found");
        }
        if inherit.len() > MAX_INHERIT {
            return Err("io");
        }
        let mut cmd = Command::new(exe);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        #[cfg(unix)]
        if !inherit.is_empty() {
            use std::os::unix::process::CommandExt as _;
            let n = inherit.len();
            let mut set = [-1 as InheritFd; MAX_INHERIT];
            set[..n].copy_from_slice(inherit);
            // SAFETY: the hook calls only async-signal-safe functions
            // (`fcntl`, `dup2`, `close`) on a fixed-size array — no
            // allocation, no locks — which is the whole `pre_exec`
            // contract in a process that holds other threads.
            unsafe {
                cmd.pre_exec(move || {
                    let floor = 3 + n as libc::c_int;
                    let mut staged = [-1 as InheritFd; MAX_INHERIT];
                    for i in 0..n {
                        let s = libc::fcntl(set[i], libc::F_DUPFD, floor);
                        if s < 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                        staged[i] = s;
                    }
                    for (i, &s) in staged[..n].iter().enumerate() {
                        if libc::dup2(s, 3 + i as libc::c_int) < 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                        libc::close(s);
                    }
                    Ok(())
                });
            }
        }
        #[cfg(not(unix))]
        if !inherit.is_empty() {
            return Err("unsupported");
        }
        match cmd.spawn() {
            Err(e) => Err(match e.kind() {
                std::io::ErrorKind::NotFound => "not_found",
                std::io::ErrorKind::PermissionDenied => "denied",
                _ => "io",
            }),
            Ok(child) => {
                let h = self.children.len() as i64;
                self.children.push(Some(child));
                Ok(h)
            }
        }
    }

    /// Wait for the child and REAP it (the slot tombstones on any
    /// completed wait). The exit code, or `signal` for a child that
    /// died without one (unix); a forged or already-reaped handle is
    /// `io`, and so is a failed wait syscall.
    pub fn wait(&mut self, h: i64) -> Result<i64, ProcErr> {
        let Some(slot) = usize::try_from(h)
            .ok()
            .and_then(|i| self.children.get_mut(i))
        else {
            return Err("io");
        };
        let Some(child) = slot.as_mut() else {
            return Err("io"); // double wait
        };
        match child.wait() {
            Err(_) => Err("io"),
            Ok(status) => {
                *slot = None; // reaped
                match status.code() {
                    Some(c) => Ok(i64::from(c)),
                    // Died without a code (a signal, unix): its own
                    // outcome, never a fake code.
                    None => Err("signal"),
                }
            }
        }
    }

    /// Kill the child. Already-exited is `io` (the child is not yours
    /// to kill anymore); the handle stays live for the wait that
    /// reaps it — kill never tombstones.
    pub fn kill(&mut self, h: i64) -> Result<(), ProcErr> {
        let Some(Some(child)) = usize::try_from(h)
            .ok()
            .and_then(|i| self.children.get_mut(i))
        else {
            return Err("io");
        };
        child.kill().map_err(|_| "io")
    }
}

/// The OS descriptor type an inherit set carries (s137): a unix fd; on
/// windows the set is refused before it is read, so the type only has
/// to exist.
#[cfg(unix)]
pub type InheritFd = std::os::fd::RawFd;
#[cfg(not(unix))]
pub type InheritFd = i32;

/// The most descriptors one `os_spawn_with` hands over (s137): the
/// `pre_exec` hook stages the set on the stack, and no prefork server
/// hands a child more listeners than this.
pub const MAX_INHERIT: usize = 64;

/// Error codes of the process family (lowering maps them to row tags,
/// coarsening any the call's row does not declare to `io`).
pub mod proc_code {
    pub const OK: i64 = 0;
    pub const NOT_FOUND: i64 = 1;
    pub const DENIED: i64 = 2;
    pub const SIGNAL: i64 = 3;
    pub const IO: i64 = 4;
    /// s137 (#235): an inherit set on a host whose runtime does not
    /// hand descriptors across a spawn (windows at this pin) — refused
    /// BY NAME, declared only by `os_spawn_with`.
    pub const UNSUPPORTED: i64 = 5;
}

/// The process-wide child table behind the shim trio — the fs
/// `FILES`/net `NET` precedent.
static CHILDREN: Mutex<ChildTable> = Mutex::new(ChildTable::new());

fn children() -> std::sync::MutexGuard<'static, ChildTable> {
    CHILDREN.lock().unwrap_or_else(|p| p.into_inner())
}

/// A row tag as its wire code ([`proc_code`]).
fn proc_code_of_tag(tag: ProcErr) -> i64 {
    match tag {
        "not_found" => proc_code::NOT_FOUND,
        "denied" => proc_code::DENIED,
        "signal" => proc_code::SIGNAL,
        "unsupported" => proc_code::UNSUPPORTED,
        _ => proc_code::IO,
    }
}

/// `os_spawn_with(exe: str, args: List[str], inherit: List[int]) -> int
/// ! {unsupported, not_found, denied, io}` (s137, #235,
/// `[os.proc.inherit]`) — the child's handle (>= 0), or `-code`.
/// `inherit` holds this process's NET handles; the shim maps each to
/// its OS descriptor first ([`crate::net::raw_fds_of`]) and a forged
/// or closed one is the `io` code with no child spawned. A header of
/// the wrong element width (FFI-only) is `io` too.
///
/// # Safety
///
/// `ep`/`el` a valid str pair; `args` a live `List[str]` header;
/// `inherit` a live `List[int]` header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_spawn_with(ep: i64, el: i64, args: i64, inherit: i64) -> i64 {
    let exe = unsafe { view(ep, el) };
    let Some(pairs) = (unsafe { crate::list::str_pair_elems(args) }) else {
        return -proc_code::IO;
    };
    let argv: Vec<&str> = pairs.iter().map(|&[p, l]| unsafe { view(p, l) }).collect();
    let Some(handles) = (unsafe { crate::list::i64_elems(inherit) }) else {
        return -proc_code::IO;
    };
    #[cfg(unix)]
    let fds: Vec<InheritFd> = match crate::net::raw_fds_of(handles) {
        Some(f) => f,
        None => return -proc_code::IO,
    };
    #[cfg(not(unix))]
    let fds: Vec<InheritFd> = handles.iter().map(|_| -1).collect();
    match children().spawn_with(exe, &argv, &fds) {
        Ok(h) => h,
        Err(t) => -proc_code_of_tag(t),
    }
}

/// `os_spawn(argv: List[str]) -> int ! {not_found, denied, io}` — the
/// child's handle (>= 0), or `-code` (the `fs_open` convention). The
/// argv arrives as one list header whose 16-byte elements are str
/// pairs; a header of the wrong element width (unreachable from
/// compiled code — sema types the argument) is the `io` code, the
/// only answer that is not undefined behaviour.
///
/// # Safety
///
/// `hdr` must be a live `List[str]` header from
/// [`crate::list::__wolf_rt_list_new`] whose elements are valid str
/// pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_spawn(hdr: i64) -> i64 {
    let Some(pairs) = (unsafe { crate::list::str_pair_elems(hdr) }) else {
        return -proc_code::IO;
    };
    let argv: Vec<&str> = pairs.iter().map(|&[p, l]| unsafe { view(p, l) }).collect();
    match children().spawn(&argv) {
        Ok(h) => h,
        Err(t) => -proc_code_of_tag(t),
    }
}

/// `os_wait(h) -> int ! {signal, io}` — parks until the child exits,
/// REAPS it (see the module doc's zombie discipline), and writes the
/// exit code through `out` on code 0.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_wait(h: i64, out: i64) -> i64 {
    // The wait blocks with the table UNLOCKED past a short snapshot?
    // No: unlike the net shims, `Child::wait` needs `&mut Child`, so
    // the wait holds the lock — one child, one waiter is the corpus
    // discipline (a second handle to the same child does not exist;
    // handles are affine ints wolf code cannot forge into aliases
    // that both wait). The trade is recorded here rather than hidden.
    match children().wait(h) {
        Ok(code) => {
            unsafe { write_word(out, code) };
            proc_code::OK
        }
        Err(t) => proc_code_of_tag(t),
    }
}

/// `os_kill(h) -> () ! {io}` — terminate the child; the handle stays
/// live for the wait that reaps it.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_kill(h: i64) -> i64 {
    match children().kill(h) {
        Ok(()) => proc_code::OK,
        Err(t) => proc_code_of_tag(t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_roundtrip_and_rows() {
        let name = format!("WOLF_RT_OS_TEST_{}", std::process::id());
        let (np, nl) = (name.as_ptr() as i64, name.len() as i64);
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        unsafe {
            // Absent: the missing code, never a trap.
            assert_eq!(__wolf_rt_env_get(np, nl, o), 1);
            let (vp, vl) = ("den".as_ptr() as i64, 3);
            assert_eq!(__wolf_rt_env_set(np, nl, vp, vl), 0);
            assert_eq!(__wolf_rt_env_get(np, nl, o), 0);
            assert_eq!(view(out[0], out[1]), "den");
            // Invalid names are the invalid code.
            let bad = "A=B";
            assert_eq!(
                __wolf_rt_env_set(bad.as_ptr() as i64, bad.len() as i64, vp, vl),
                1
            );
            assert_eq!(__wolf_rt_env_set(np, 0, vp, vl), 1); // empty
            // SAFETY: test-local name; no concurrent env readers care.
            std::env::remove_var(&name);
        }
    }

    #[test]
    fn vars_are_sorted_kv_lines() {
        let hdr = __wolf_rt_env_vars();
        let n = unsafe { crate::list::__wolf_rt_list_len(hdr) };
        let mut prev = String::new();
        for i in 0..n {
            let mut pair = [0i64; 2];
            let rc = unsafe { crate::list::__wolf_rt_list_read(hdr, i, pair.as_mut_ptr() as i64) };
            assert_eq!(rc, 1, "in-bounds read");
            let kv = unsafe { view(pair[0], pair[1]) };
            assert!(kv.contains('='), "K=V shape: {kv}");
            assert!(prev.as_str() <= kv, "sorted: {prev} <= {kv}");
            prev = kv.to_string();
        }
    }

    #[test]
    fn cwd_is_a_str() {
        let mut out = [0i64; 2];
        assert_eq!(unsafe { __wolf_rt_os_cwd(out.as_mut_ptr() as i64) }, 0);
        assert!(out[1] > 0);
    }

    /// #69: the path must name a file that EXISTS — a rig that spawns
    /// itself as its own child needs a spawnable answer, not a label.
    #[test]
    fn exe_is_an_existing_path() {
        let mut out = [0i64; 2];
        assert_eq!(unsafe { __wolf_rt_os_exe(out.as_mut_ptr() as i64) }, 0);
        let p = unsafe { view(out[0], out[1]) };
        assert!(
            std::path::Path::new(p).is_file(),
            "os_exe names a file: {p}"
        );
    }

    // ------------------- s107: the process trio over a ChildTable --
    //
    // Platform posture: the row tests spawn nothing and run on every
    // tier-1 host; the live-child tests are unix-gated on the checked
    // twins' precedent (`/bin/sh` is the one portable-enough fixture;
    // windows coverage rides the std.process facade sprint with its
    // own fixture story — `crates/wolf_mem/tests/os_time_json.rs`).

    /// `corpus/os/spawn_rows.lu`'s native half: an empty argv names no
    /// program, an unspawnable program is `not_found`, a forged handle
    /// is `io` on wait AND kill — rows, never traps, no child ever
    /// spawned.
    #[test]
    fn process_rows_without_a_child() {
        let mut t = ChildTable::new();
        assert_eq!(t.spawn(&[]), Err("not_found"));
        assert_eq!(t.spawn(&["wolf-s107-no-such-program"]), Err("not_found"));
        assert_eq!(t.wait(99), Err("io"));
        assert_eq!(t.kill(99), Err("io"));
    }

    /// The same rows through the extern surface: the argv arrives as
    /// a real `List[str]` header; codes come back negated on the
    /// spawn handle path (the `fs_open` convention).
    #[test]
    fn shim_process_rows() {
        let empty = crate::list::__wolf_rt_list_new(16);
        assert_eq!(unsafe { __wolf_rt_os_spawn(empty) }, -proc_code::NOT_FOUND);
        let argv = crate::list::new_list(16);
        crate::list::push_str(argv, "wolf-s107-no-such-program");
        assert_eq!(
            unsafe { __wolf_rt_os_spawn(argv as i64) },
            -proc_code::NOT_FOUND
        );
        // A header of the wrong element width is the io code — the
        // only answer that is not undefined behaviour (FFI-only; sema
        // types compiled argv `List[str]`).
        let ints = crate::list::__wolf_rt_list_new(8);
        assert_eq!(unsafe { __wolf_rt_os_spawn(ints) }, -proc_code::IO);
        let mut out = [0i64; 1];
        let o = out.as_mut_ptr() as i64;
        assert_eq!(unsafe { __wolf_rt_os_wait(9_999, o) }, proc_code::IO);
        assert_eq!(__wolf_rt_os_kill(9_999), proc_code::IO);
    }

    /// The reap discipline, witnessed (#50: WAIT for the outcome,
    /// never sample it): the exit code arrives through wait, the
    /// successful wait tombstones, and the double wait is `io`.
    #[cfg(unix)]
    #[test]
    fn wait_reaps_and_double_wait_is_io() {
        let mut t = ChildTable::new();
        let h = t.spawn(&["/bin/sh", "-c", "exit 7"]).expect("spawn");
        assert_eq!(t.wait(h), Ok(7));
        assert_eq!(t.wait(h), Err("io"), "the reap tombstoned the slot");
    }

    /// The zombie-discipline flag, end to end: kill does NOT
    /// tombstone (the handle stays live for the reaping wait), the
    /// wait after the kill is the `signal` row AND the reap — the
    /// child is collected before the test returns, so kill-then-drop
    /// leaks no zombie past it. After the reap the handle is dead on
    /// every entry.
    #[cfg(unix)]
    #[test]
    fn kill_then_wait_is_signal_and_reaps() {
        let mut t = ChildTable::new();
        let h = t.spawn(&["/bin/sh", "-c", "sleep 30"]).expect("spawn");
        assert_eq!(t.kill(h), Ok(()));
        assert_eq!(t.wait(h), Err("signal"), "no exit code: the signal row");
        assert_eq!(t.wait(h), Err("io"), "the signal wait reaped too");
        assert_eq!(t.kill(h), Err("io"), "a reaped handle is not yours");
    }

    /// The live sequence through the extern surface: spawn a real
    /// child by argv pairs, wait for its code through the out word.
    #[cfg(unix)]
    #[test]
    fn shim_spawn_wait_kill_roundtrip() {
        let argv = crate::list::new_list(16);
        crate::list::push_str(argv, "/bin/sh");
        crate::list::push_str(argv, "-c");
        crate::list::push_str(argv, "exit 5");
        let h = unsafe { __wolf_rt_os_spawn(argv as i64) };
        assert!(h >= 0, "spawn hands back a handle: {h}");
        let mut out = [0i64; 1];
        let o = out.as_mut_ptr() as i64;
        assert_eq!(unsafe { __wolf_rt_os_wait(h, o) }, proc_code::OK);
        assert_eq!(out[0], 5);
        assert_eq!(unsafe { __wolf_rt_os_wait(h, o) }, proc_code::IO);
        // Kill-then-wait through the shims: the signal code, then the
        // tombstone.
        let argv2 = crate::list::new_list(16);
        crate::list::push_str(argv2, "/bin/sh");
        crate::list::push_str(argv2, "-c");
        crate::list::push_str(argv2, "sleep 30");
        let h2 = unsafe { __wolf_rt_os_spawn(argv2 as i64) };
        assert!(h2 >= 0);
        assert_eq!(__wolf_rt_os_kill(h2), proc_code::OK);
        assert_eq!(unsafe { __wolf_rt_os_wait(h2, o) }, proc_code::SIGNAL);
        assert_eq!(unsafe { __wolf_rt_os_wait(h2, o) }, proc_code::IO);
    }

    // ---------------- s137: the inherit set (#235, [os.proc.inherit]) --

    /// s137: a listener handed through `spawn_with` is the child's 3 —
    /// measured with the shell's own `test -S /dev/fd/3` (a socket at
    /// that number, or not); a plain spawn hands nothing (3 is not
    /// open: the CLOEXEC posture #235 measured with `lsof`, now the
    /// negative control); two listeners are 3 and 4 IN ORDER and 5 is
    /// closed; an oversized set is `io` before any child exists; the
    /// spawn rows are `spawn`'s.
    #[cfg(unix)]
    #[test]
    fn spawn_with_hands_descriptors_to_the_child_from_3() {
        use std::os::fd::AsRawFd as _;
        let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let l2 = std::net::TcpListener::bind("127.0.0.1:0").expect("bind 2");
        let mut t = ChildTable::new();
        let h = t
            .spawn_with("/bin/sh", &["-c", "test -S /dev/fd/3"], &[l.as_raw_fd()])
            .expect("spawn");
        assert_eq!(t.wait(h), Ok(0), "fd 3 in the child is a socket");
        let h = t
            .spawn_with("/bin/sh", &["-c", "test -S /dev/fd/3"], &[])
            .expect("spawn");
        assert_eq!(
            t.wait(h),
            Ok(1),
            "no inherit set: the child holds no socket"
        );
        let h = t
            .spawn_with(
                "/bin/sh",
                &[
                    "-c",
                    "test -S /dev/fd/3 && test -S /dev/fd/4 && ! test -e /dev/fd/5",
                ],
                &[l.as_raw_fd(), l2.as_raw_fd()],
            )
            .expect("spawn");
        assert_eq!(t.wait(h), Ok(0), "3 and 4 in order, 5 closed");
        // The same descriptor twice is two numbers in the child.
        let h = t
            .spawn_with(
                "/bin/sh",
                &["-c", "test -S /dev/fd/3 && test -S /dev/fd/4"],
                &[l.as_raw_fd(), l.as_raw_fd()],
            )
            .expect("spawn");
        assert_eq!(t.wait(h), Ok(0));
        let big = vec![l.as_raw_fd(); MAX_INHERIT + 1];
        assert_eq!(t.spawn_with("/bin/sh", &["-c", "true"], &big), Err("io"));
        assert_eq!(t.spawn_with("", &[], &[]), Err("not_found"));
        assert_eq!(
            t.spawn_with("wolf-s137-no-such-program", &[], &[l.as_raw_fd()]),
            Err("not_found")
        );
    }

    /// s137: the shim maps NET HANDLES (this process's table indexes)
    /// to descriptors — a real listener handle reaches the child as 3;
    /// a forged handle is the `io` code with nothing spawned; a
    /// header of the wrong element width is `io` too.
    #[cfg(unix)]
    #[test]
    fn shim_spawn_with_maps_net_handles_and_refuses_forged_ones() {
        let addr = "127.0.0.1:0";
        let srv =
            unsafe { crate::net::__wolf_rt_net_listen(addr.as_ptr() as i64, addr.len() as i64) };
        assert!(srv >= 0);
        let exe = "/bin/sh";
        let args = crate::list::new_list(16);
        crate::list::push_str(args, "-c");
        crate::list::push_str(args, "test -S /dev/fd/3");
        let inherit = crate::list::new_list(8);
        crate::list::push_int(inherit, srv);
        let h = unsafe {
            __wolf_rt_os_spawn_with(
                exe.as_ptr() as i64,
                exe.len() as i64,
                args as i64,
                inherit as i64,
            )
        };
        assert!(h >= 0, "spawn: {h}");
        let mut out = [0i64; 1];
        let o = out.as_mut_ptr() as i64;
        assert_eq!(unsafe { __wolf_rt_os_wait(h, o) }, proc_code::OK);
        assert_eq!(out[0], 0, "the child saw the listener at 3");
        let forged = crate::list::new_list(8);
        crate::list::push_int(forged, 99_999);
        assert_eq!(
            unsafe {
                __wolf_rt_os_spawn_with(
                    exe.as_ptr() as i64,
                    exe.len() as i64,
                    args as i64,
                    forged as i64,
                )
            },
            -proc_code::IO
        );
        // A List[str] header where the inherit list should be.
        assert_eq!(
            unsafe {
                __wolf_rt_os_spawn_with(
                    exe.as_ptr() as i64,
                    exe.len() as i64,
                    args as i64,
                    args as i64,
                )
            },
            -proc_code::IO
        );
        assert_eq!(
            crate::net::__wolf_rt_net_close(srv),
            crate::net::net_code::OK
        );
    }

    /// s137 on windows: an inherit set is refused BY NAME (the
    /// `unsupported` code, never a bare `io`) with no child spawned;
    /// an empty set is `spawn` — the child runs and its code comes
    /// back through the reap.
    #[cfg(windows)]
    #[test]
    fn spawn_with_refuses_an_inherit_set_by_name_on_windows() {
        let mut t = ChildTable::new();
        assert_eq!(
            t.spawn_with("cmd", &["/c", "exit 0"], &[7]),
            Err("unsupported")
        );
        let h = t.spawn_with("cmd", &["/c", "exit 3"], &[]).expect("spawn");
        assert_eq!(t.wait(h), Ok(3));
        let exe = "cmd";
        let args = crate::list::new_list(16);
        crate::list::push_str(args, "/c");
        crate::list::push_str(args, "exit 0");
        let inherit = crate::list::new_list(8);
        crate::list::push_int(inherit, 0);
        assert_eq!(
            unsafe {
                __wolf_rt_os_spawn_with(
                    exe.as_ptr() as i64,
                    exe.len() as i64,
                    args as i64,
                    inherit as i64,
                )
            },
            -proc_code::UNSUPPORTED
        );
    }
}
