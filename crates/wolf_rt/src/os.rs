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

/// `os_cpus() -> int ! {io}` (s137, wolf-lang#233, `[os.cpus]`) — the
/// count of SCHEDULABLE cores, `>= 1`, or the `io` code.
/// `available_parallelism` is the one call that answers it on every
/// tier-1 host and answers the right thing: on linux it honours a
/// cgroup cpu quota and a cpu affinity mask, which `/proc/cpuinfo`
/// (what a program had to read before this call existed) does not —
/// a container with two cpus of quota on a 64-core host reads 64
/// there and 2 here. `sysctl hw.ncpu` on macOS, `GetSystemInfo` on
/// windows, both behind the same call.
///
/// Never 0, and never a silent 1: a host that cannot answer is the
/// `io` row, so `worker_processes auto` can say it did not learn the
/// number instead of quietly running one worker.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_cpus(out: i64) -> i64 {
    match std::thread::available_parallelism() {
        Err(_) => 1,
        Ok(n) => {
            unsafe { crate::str::write_word(out, n.get() as i64) };
            0
        }
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
        self.spawn_with_env(exe, args, inherit, &[])
    }

    /// [`ChildTable::spawn_with`] with environment additions — the
    /// SAME call, with one seam the shipping surface does not expose:
    /// `os_spawn_with` hands a child descriptors, not variables, and
    /// `env_set` is the program's own way to arrange its environment.
    /// The seam exists so the inherit witness can re-execute THIS
    /// binary as its own child and tell it what to check, without a
    /// second copy of the staging hook that could drift from this one.
    #[cfg_attr(not(unix), allow(unused_variables))]
    pub(crate) fn spawn_with_env(
        &mut self,
        exe: &str,
        args: &[&str],
        inherit: &[InheritFd],
        env: &[(&str, &str)],
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
        for (k, v) in env {
            cmd.env(k, v);
        }
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

    /// `os_spawn_fds(exe, args, map)` (s215, `[os.proc.fds]`): spawn
    /// `exe` with `args` and the child's descriptors set up as `map`
    /// says. Each entry is `(target, source)`: `source` an OS
    /// descriptor of this process (already duplicated by the shim, so
    /// it cannot be closed under the spawn) that the child receives AS
    /// `target`, or `None` for "closed in the child". A target the map
    /// does not name keeps the spawn posture of `[os.proc.spawn]`: 0
    /// the null device, 1 and 2 inherited, nothing else open (every
    /// runtime descriptor is close-on-exec). The map arrives validated
    /// ([`map_of`]); this only places it.
    ///
    /// Mechanics (unix): stdio is INHERITED at the `Command` level so
    /// that, inside the forked child, 0..2 are still this process's
    /// own when the `pre_exec` hook reads them as sources. The hook
    /// stages every source above the highest target (`F_DUPFD`, so a
    /// source already sitting at some target number is read before
    /// anything is written there), then `dup2`s each stage onto its
    /// target — which clears close-on-exec on the target and only
    /// there — closes each stage, closes the `None` targets, and last
    /// opens the null device onto 0 when the map does not name 0. The
    /// hook runs after `fork` in a multithreaded process, so it calls
    /// only async-signal-safe functions on fixed arrays: the map is
    /// bounded at [`MAX_FD_PAIRS`]. windows: a non-empty map is
    /// `unsupported`, by name, with no child (a HANDLE is not a small
    /// number a child can be handed at a position).
    #[cfg_attr(not(unix), allow(unused_variables))]
    pub fn spawn_fds(
        &mut self,
        exe: &str,
        args: &[&str],
        map: &[(InheritFd, Option<InheritFd>)],
    ) -> Result<i64, ProcErr> {
        if exe.is_empty() {
            return Err("not_found");
        }
        if map.is_empty() {
            return self.spawn_with(exe, args, &[]);
        }
        self.spawn_job(exe, args, map, &JobOpts::PLAIN)
    }

    /// `os_spawn_job(exe, args, map, group, tty, defaults)` (s219,
    /// `[os.proc.job]`): [`ChildTable::spawn_fds`] with the three things
    /// a job-control shell does between `fork` and `exec`, in this order
    /// inside the child's hook, before the map is placed (so `tty` is
    /// still this process's descriptor): join or lead a process group
    /// (`setpgid(0, group)`; 0 leads a new one), make that group the
    /// terminal's foreground (`tcsetpgrp` with `SIGTTOU` blocked for the
    /// call, then the mask restored), and set every signal in `defaults`
    /// back to the host default. The parent repeats `setpgid` and
    /// `tcsetpgrp` after the spawn — the classic double call, so neither
    /// side races the other; a failure there is ignored (the child may
    /// already have exec'd or ended). A failure in the child's hook is
    /// the spawn's error: `EPERM` from a group in another session is
    /// `denied`, the rest `io`. windows: `unsupported` unless every
    /// option is off.
    #[cfg_attr(not(unix), allow(unused_variables))]
    pub fn spawn_job(
        &mut self,
        exe: &str,
        args: &[&str],
        map: &[(InheritFd, Option<InheritFd>)],
        opts: &JobOpts,
    ) -> Result<i64, ProcErr> {
        if exe.is_empty() {
            return Err("not_found");
        }
        if map.len() > MAX_FD_PAIRS {
            return Err("invalid");
        }
        #[cfg(not(unix))]
        {
            Err("unsupported")
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            let group = opts.group as libc::pid_t;
            let tty = opts.tty.unwrap_or(-1);
            let defaults = opts.defaults;
            let mut cmd = Command::new(exe);
            cmd.args(args)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
            let n = map.len();
            let mut targets = [-1 as InheritFd; MAX_FD_PAIRS];
            let mut sources = [-1 as InheritFd; MAX_FD_PAIRS];
            let mut floor: InheritFd = 3;
            let mut names_zero = false;
            for (i, &(t, s)) in map.iter().enumerate() {
                targets[i] = t;
                sources[i] = s.unwrap_or(-1);
                floor = floor.max(t + 1);
                names_zero |= t == 0;
            }
            // SAFETY: the hook calls only async-signal-safe functions
            // (`fcntl`, `dup2`, `close`, `open` on a static path) over
            // fixed-size arrays — no allocation and no locks, which is
            // the whole `pre_exec` contract in a process that holds
            // other threads.
            unsafe {
                cmd.pre_exec(move || {
                    if group >= 0 && libc::setpgid(0, group) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if tty >= 0 {
                        let r = with_ttou_blocked(|| libc::tcsetpgrp(tty, libc::getpgrp()));
                        if r < 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                    }
                    for &sig in &defaults {
                        if sig > 0 {
                            let mut sa: libc::sigaction = std::mem::zeroed();
                            sa.sa_sigaction = libc::SIG_DFL;
                            libc::sigemptyset(&mut sa.sa_mask);
                            libc::sigaction(sig, &sa, std::ptr::null_mut());
                        }
                    }
                    let mut staged = [-1 as InheritFd; MAX_FD_PAIRS];
                    for i in 0..n {
                        if sources[i] >= 0 {
                            let s = libc::fcntl(sources[i], libc::F_DUPFD_CLOEXEC, floor);
                            if s < 0 {
                                return Err(std::io::Error::last_os_error());
                            }
                            staged[i] = s;
                        }
                    }
                    for i in 0..n {
                        if staged[i] >= 0 {
                            if libc::dup2(staged[i], targets[i]) < 0 {
                                return Err(std::io::Error::last_os_error());
                            }
                            libc::close(staged[i]);
                        } else {
                            libc::close(targets[i]);
                        }
                    }
                    if !names_zero {
                        let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR);
                        if null < 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                        if null != 0 {
                            if libc::dup2(null, 0) < 0 {
                                return Err(std::io::Error::last_os_error());
                            }
                            libc::close(null);
                        }
                    }
                    Ok(())
                });
            }
            match cmd.spawn() {
                Err(e) => Err(match e.kind() {
                    std::io::ErrorKind::NotFound => "not_found",
                    std::io::ErrorKind::PermissionDenied => "denied",
                    _ => "io",
                }),
                Ok(child) => {
                    // The parent's half of the double call.
                    let pid = child.id() as libc::pid_t;
                    if group >= 0 {
                        let pg = if group == 0 { pid } else { group };
                        // SAFETY: plain syscalls on a child we own.
                        unsafe {
                            libc::setpgid(pid, pg);
                            if tty >= 0 {
                                with_ttou_blocked(|| libc::tcsetpgrp(tty, pg));
                            }
                        }
                    }
                    let h = self.children.len() as i64;
                    self.children.push(Some(child));
                    Ok(h)
                }
            }
        }
    }

    /// `os_proc_pid(h)` (s219): the child's OS process id — the group
    /// id a job leader gives its group. `io` for a forged or reaped
    /// handle.
    pub fn pid(&self, h: i64) -> Result<i64, ProcErr> {
        let Some(Some(child)) = usize::try_from(h).ok().and_then(|i| self.children.get(i)) else {
            return Err("io");
        };
        Ok(i64::from(child.id()))
    }

    /// `os_wait_status(h)` (s219, `[os.proc.status]`): [`Self::wait`],
    /// but a death by signal N answers `-N` instead of the `signal` row,
    /// so a shell can say `128 + N`; an exit code is itself (0..255).
    /// The two never collide.
    pub fn wait_status(&mut self, h: i64) -> Result<i64, ProcErr> {
        let Some(slot) = usize::try_from(h)
            .ok()
            .and_then(|i| self.children.get_mut(i))
        else {
            return Err("io");
        };
        let Some(child) = slot.as_mut() else {
            return Err("io");
        };
        match child.wait() {
            Err(_) => Err("io"),
            Ok(status) => {
                *slot = None;
                if let Some(c) = status.code() {
                    return Ok(i64::from(c));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt as _;
                    if let Some(sig) = status.signal() {
                        return Ok(-i64::from(sig));
                    }
                }
                Err("io")
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

/// What `os_spawn_job` asks of the child beyond its map (s219):
/// `group` -1 stay in this process's group, 0 lead a new one, > 0 join
/// that one; `tty` a descriptor whose terminal the child's group takes
/// as foreground; `defaults` the signal numbers set back to default
/// (0 slots unused).
#[derive(Clone, Copy, Debug)]
pub struct JobOpts {
    pub group: i64,
    pub tty: Option<InheritFd>,
    pub defaults: [i32; 9],
}

impl JobOpts {
    /// `os_spawn_fds`'s posture: no group, no terminal, no resets.
    pub const PLAIN: JobOpts = JobOpts {
        group: -1,
        tty: None,
        defaults: [0; 9],
    };
}

/// Run `f` with `SIGTTOU` blocked on the calling thread, then restore
/// the thread's mask (s219): a process outside the terminal's
/// foreground group may change the foreground or the mode only while
/// it blocks or ignores `SIGTTOU` (XBD 11.1.4), and blocking for the
/// call, on this thread only, changes nothing anyone else can see.
/// Async-signal-safe (`pthread_sigmask` and `f`'s own calls), so the
/// spawn hook uses it too.
#[cfg(unix)]
pub(crate) fn with_ttou_blocked(f: impl FnOnce() -> libc::c_int) -> libc::c_int {
    // SAFETY: zeroed sigsets filled by sigemptyset/sigaddset.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        let mut old: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTTOU);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut old);
        let r = f();
        let e = *errno_loc();
        libc::pthread_sigmask(libc::SIG_SETMASK, &old, std::ptr::null_mut());
        *errno_loc() = e;
        r
    }
}

#[cfg(all(unix, target_os = "linux"))]
unsafe fn errno_loc() -> *mut libc::c_int {
    unsafe { libc::__errno_location() }
}
#[cfg(all(unix, not(target_os = "linux")))]
unsafe fn errno_loc() -> *mut libc::c_int {
    unsafe { libc::__error() }
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
    /// s215 (`[os.proc.fds]`): a descriptor map that is not one — an
    /// odd length, a target outside `0..=MAX_FD_TARGET`, a target named
    /// twice, a source below -1, more than [`super::MAX_FD_PAIRS`]
    /// pairs — decided before any handle is read or child made.
    /// Declared only by `os_spawn_fds`.
    pub const INVALID: i64 = 6;
}

/// The most `(target, source)` pairs one `os_spawn_fds` places (s215):
/// the `pre_exec` hook keeps the map on the stack.
pub const MAX_FD_PAIRS: usize = 64;

/// The highest descriptor number a map may name in the child (s215).
/// POSIX guarantees a process at least 20 descriptors and every tier-1
/// unix allows far more; 255 is room for any shell's `N>&M` and keeps
/// the staging floor small.
pub const MAX_FD_TARGET: i64 = 255;

/// `os_spawn_fds`'s map, read and checked before anything else happens
/// (s215, `[os.proc.fds]`): the flat `[target, source, …]` list as
/// `(target, source)` pairs, `source` `None` for `-1` (closed in the
/// child). `Err("invalid")` for a list that is not a map: an odd
/// length, a target outside `0..=MAX_FD_TARGET`, a repeated target, a
/// source below `-1`, more than [`MAX_FD_PAIRS`] pairs. The checked
/// machine's `os_spawn_fds` applies the same rules in the same order.
pub fn map_of(flat: &[i64]) -> Result<Vec<(i64, Option<i64>)>, ProcErr> {
    if !flat.len().is_multiple_of(2) || flat.len() / 2 > MAX_FD_PAIRS {
        return Err("invalid");
    }
    let mut out: Vec<(i64, Option<i64>)> = Vec::with_capacity(flat.len() / 2);
    for pair in flat.chunks_exact(2) {
        let (t, s) = (pair[0], pair[1]);
        if !(0..=MAX_FD_TARGET).contains(&t) || s < -1 || out.iter().any(|&(u, _)| u == t) {
            return Err("invalid");
        }
        out.push((t, (s >= 0).then_some(s)));
    }
    Ok(out)
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
        "invalid" => proc_code::INVALID,
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

/// `os_spawn_fds(exe: str, args: List[str], map: List[int]) -> int !
/// {denied, invalid, io, not_found, unsupported}` (s215,
/// `[os.proc.fds]`) — the child's handle (>= 0), or `-code`. The map is
/// read in a fixed order, so a caller can tell from the code how far
/// the call got: its SHAPE first (`invalid`, [`map_of`]), then the HOST
/// (windows: a non-empty map is `unsupported`), then every SOURCE handle
/// (`io` for a closed or forged one — each is duplicated here and held
/// until the spawn returns), and only then the program (`not_found`,
/// `denied`, `io`). Nothing before the last step makes a child.
///
/// # Safety
///
/// `ep`/`el` a valid str pair; `args` a live `List[str]` header; `map`
/// a live `List[int]` header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_spawn_fds(ep: i64, el: i64, args: i64, map: i64) -> i64 {
    let exe = unsafe { view(ep, el) };
    let Some(pairs) = (unsafe { crate::list::str_pair_elems(args) }) else {
        return -proc_code::IO;
    };
    let argv: Vec<&str> = pairs.iter().map(|&[p, l]| unsafe { view(p, l) }).collect();
    let Some(flat) = (unsafe { crate::list::i64_elems(map) }) else {
        return -proc_code::IO;
    };
    let entries = match map_of(flat) {
        Ok(m) => m,
        Err(t) => return -proc_code_of_tag(t),
    };
    if cfg!(not(unix)) && !entries.is_empty() {
        return -proc_code::UNSUPPORTED;
    }
    // Each source duplicated and HELD: the files live until the spawn
    // returns, so the numbers the child's hook reads cannot be closed
    // and reused by another task in between.
    let mut held: Vec<std::fs::File> = Vec::with_capacity(entries.len());
    let mut placed: Vec<(InheritFd, Option<InheritFd>)> = Vec::with_capacity(entries.len());
    for &(t, s) in &entries {
        let src = match s {
            None => None,
            Some(h) => match crate::fs::dup_of(h) {
                None => return -proc_code::IO,
                Some(f) => {
                    let fd = raw_of(&f);
                    held.push(f);
                    Some(fd)
                }
            },
        };
        placed.push((t as InheritFd, src));
    }
    let r = children().spawn_fds(exe, &argv, &placed);
    drop(held);
    match r {
        Ok(h) => h,
        Err(t) => -proc_code_of_tag(t),
    }
}

/// `os_spawn_job(exe: str, args: List[str], map: List[int], group: int,
/// tty: int, defaults: int) -> int ! {denied, invalid, io, not_found,
/// unsupported}` (s219, `[os.proc.job]`) — the child's handle (>= 0),
/// or `-code`. Read in a fixed order, as `os_spawn_fds` reads its map:
/// the SHAPE (`invalid`: a bad map, `group` below -1, `tty` below -1,
/// a `defaults` bit outside the meanings), then the HOST (windows: any
/// option on, or a non-empty map, is `unsupported`), then the `tty`
/// handle (`io` unless it is an open terminal) and every map source
/// (`io`), and only then the program. With every option off this is
/// `os_spawn_fds`.
///
/// # Safety
///
/// `ep`/`el` a valid str pair; `args` a live `List[str]` header; `map`
/// a live `List[int]` header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_spawn_job(
    ep: i64,
    el: i64,
    args: i64,
    map: i64,
    group: i64,
    tty: i64,
    defaults: i64,
) -> i64 {
    let exe = unsafe { view(ep, el) };
    let Some(pairs) = (unsafe { crate::list::str_pair_elems(args) }) else {
        return -proc_code::IO;
    };
    let argv: Vec<&str> = pairs.iter().map(|&[p, l]| unsafe { view(p, l) }).collect();
    let Some(flat) = (unsafe { crate::list::i64_elems(map) }) else {
        return -proc_code::IO;
    };
    let entries = match map_of(flat) {
        Ok(m) => m,
        Err(t) => return -proc_code_of_tag(t),
    };
    if group < -1 || tty < -1 || defaults < 0 || defaults & !SIGNAL_MEANINGS != 0 {
        return -proc_code::INVALID;
    }
    let plain = group == -1 && tty == -1 && defaults == 0;
    if cfg!(not(unix)) && !(plain && entries.is_empty()) {
        return -proc_code::UNSUPPORTED;
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let sigs = crate::signal::signals_of(defaults);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let sigs = {
        if defaults != 0 {
            return -proc_code::UNSUPPORTED;
        }
        [0i32; 9]
    };
    let mut held: Vec<std::fs::File> = Vec::with_capacity(entries.len() + 1);
    let mut tty_fd: Option<InheritFd> = None;
    if tty >= 0 {
        if crate::fs::is_terminal(tty) != Some(true) {
            return -proc_code::IO;
        }
        match crate::fs::dup_of(tty) {
            None => return -proc_code::IO,
            Some(f) => {
                tty_fd = Some(raw_of(&f));
                held.push(f);
            }
        }
    }
    let mut placed: Vec<(InheritFd, Option<InheritFd>)> = Vec::with_capacity(entries.len());
    for &(t, s) in &entries {
        let src = match s {
            None => None,
            Some(h) => match crate::fs::dup_of(h) {
                None => return -proc_code::IO,
                Some(f) => {
                    let fd = raw_of(&f);
                    held.push(f);
                    Some(fd)
                }
            },
        };
        placed.push((t as InheritFd, src));
    }
    let opts = JobOpts {
        group,
        tty: tty_fd,
        defaults: sigs,
    };
    let r = if plain {
        children().spawn_fds(exe, &argv, &placed)
    } else {
        children().spawn_job(exe, &argv, &placed, &opts)
    };
    drop(held);
    match r {
        Ok(h) => h,
        Err(t) => -proc_code_of_tag(t),
    }
}

/// The meaning bits a spawn's `defaults` may name (s219) — the signal
/// module's `ALL`, restated here because that module is platform-gated.
const SIGNAL_MEANINGS: i64 = 511;

/// `os_proc_pid(h) -> int ! {io}` (s219) — the child's process id (> 0)
/// or `-IO`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_proc_pid(h: i64) -> i64 {
    match children().pid(h) {
        Ok(p) => p,
        Err(t) => -proc_code_of_tag(t),
    }
}

/// `os_wait_status(h) -> int ! {io}` (s219, `[os.proc.status]`) — waits
/// and REAPS as `os_wait` does; the status (an exit code, or `-N` for a
/// death by signal N) through `out` on code 0.
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_wait_status(h: i64, out: i64) -> i64 {
    match children().wait_status(h) {
        Ok(st) => {
            unsafe { write_word(out, st) };
            proc_code::OK
        }
        Err(t) => proc_code_of_tag(t),
    }
}

#[cfg(unix)]
fn raw_of(f: &std::fs::File) -> InheritFd {
    use std::os::fd::AsRawFd as _;
    f.as_raw_fd()
}

#[cfg(not(unix))]
fn raw_of(_f: &std::fs::File) -> InheritFd {
    -1
}

/// `os_pipe() -> (int, int) ! {io}` (s215, `[os.proc.pipe]`) — a pipe's
/// read end and write end, as two fs handles (3 and up), written
/// through `out` (16 bytes) on code 0. Both ends are close-on-exec, as
/// every runtime descriptor is: a child receives one only through an
/// `os_spawn_fds` map. `io` (1) is a host that cannot make one (the
/// descriptor limit).
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_pipe(out: i64) -> i64 {
    let Ok((r, w)) = std::io::pipe() else {
        return 1;
    };
    #[cfg(unix)]
    let (rf, wf) = (
        std::fs::File::from(std::os::fd::OwnedFd::from(r)),
        std::fs::File::from(std::os::fd::OwnedFd::from(w)),
    );
    #[cfg(windows)]
    let (rf, wf) = (
        std::fs::File::from(std::os::windows::io::OwnedHandle::from(r)),
        std::fs::File::from(std::os::windows::io::OwnedHandle::from(w)),
    );
    let rh = crate::fs::mint_file(rf);
    let wh = crate::fs::mint_file(wf);
    unsafe { write_pair(out, rh, wh) };
    0
}

/// `os_chdir(path) -> () ! {denied, io, not_found}` (s215,
/// `[os.fs.chdir]`) — the process's working directory becomes `path`,
/// resolved against the current one when relative. Codes: 0 ok, 1
/// `not_found`, 2 `denied`, 3 `io` (a path that names a file, and every
/// other host failure).
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_chdir(pp: i64, pl: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    match std::env::set_current_dir(path) {
        Ok(()) => 0,
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => 1,
            std::io::ErrorKind::PermissionDenied => 2,
            _ => 3,
        },
    }
}

/// `os_isatty(fd) -> bool ! {io}` (s215, `[os.fs.isatty]`) — whether
/// the handle names a terminal: 0..2 the standard streams, 3 and up
/// the table. The answer (0 or 1) goes through `out` on code 0; a
/// closed or forged handle is code 1 (`io`).
///
/// # Safety
///
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_isatty(fd: i64, out: i64) -> i64 {
    match crate::fs::is_terminal(fd) {
        None => 1,
        Some(t) => {
            unsafe { write_word(out, i64::from(t)) };
            0
        }
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

    /// s137 (#233, `[os.cpus]`): the count is at least one and the
    /// call answers it — every tier-1 host does. The exact number is
    /// the machine's, so the relation is what is pinned, never a
    /// value; it is also compared against the thread pool's own view,
    /// because both read the same source and a disagreement would
    /// mean the runtime is telling a program one thing and sizing
    /// itself by another.
    #[test]
    fn cpus_is_at_least_one_and_agrees_with_the_pool() {
        let mut out = [0i64; 1];
        let o = out.as_mut_ptr() as i64;
        assert_eq!(
            unsafe { __wolf_rt_os_cpus(o) },
            0,
            "every tier-1 host answers"
        );
        assert!(out[0] >= 1, "schedulable cores: {}", out[0]);
        let want = std::thread::available_parallelism().map_or(1, |n| n.get() as i64);
        assert_eq!(out[0], want, "the same source the pool sizes itself from");
    }

    // ---------------- s137: the inherit set (#235, [os.proc.inherit]) --

    /// s137: a listener handed through `spawn_with` is the child's 3.
    /// Two children answer it. The SHELL'S child settles the coarse
    /// question with `test -S /dev/fd/3` — a socket at that number, or
    /// not — and its negative control is a plain spawn, where 3 is not
    /// a socket at all (the CLOEXEC posture #235 measured with `lsof`).
    /// THIS BINARY'S child settles the fine one: re-executed with two
    /// listeners on DIFFERENT ports, it adopts 3 and 4 and reports
    /// whether each is the listener the parent put at that position —
    /// which is the whole contract, since a caller names a descriptor
    /// only by where it sits. It also asks whether anything ABOVE the
    /// set is one of the parent's listeners, because "the runtime
    /// hands exactly this many" is the other half of the promise.
    ///
    /// What it deliberately does NOT assert is that descriptor
    /// `3 + n` is closed outright. A process may hold inheritable
    /// descriptors that are none of the runtime's business — a test
    /// harness's pipe, a library's — and `[os.proc.inherit]` promises
    /// only that nothing above the set is there because the runtime
    /// put it there. Measured: a `! test -e /dev/fd/5` assertion here
    /// failed on the GitHub macOS runners against a descriptor this
    /// crate never opened.
    #[cfg(unix)]
    #[test]
    fn spawn_with_hands_descriptors_to_the_child_from_3() {
        use std::os::fd::AsRawFd as _;
        // The re-executed child's half.
        if let Ok(v) = std::env::var("WOLF_S137_INHERIT_CHILD") {
            let want: Vec<i64> = v.split(',').filter_map(|w| w.parse().ok()).collect();
            let mut t = crate::net::NetTable::new();
            let mut code = 0i32;
            for (i, &port) in want.iter().enumerate() {
                let at = 3 + i as i64;
                let got = t.adopt_listener(at).ok().and_then(|h| t.port(h).ok());
                if got != Some(port) {
                    code = 10 + i as i32;
                }
            }
            // Nothing above the set is one of the parent's listeners.
            let above = 3 + want.len() as i64;
            if let Ok(h) = t.adopt_listener(above)
                && t.port(h).is_ok_and(|p| want.contains(&p))
            {
                code = 20;
            }
            std::process::exit(code);
        }
        let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let l2 = std::net::TcpListener::bind("127.0.0.1:0").expect("bind 2");
        let p1 = i64::from(l.local_addr().expect("addr").port());
        let p2 = i64::from(l2.local_addr().expect("addr 2").port());
        assert_ne!(p1, p2, "two distinct ports, so ORDER is observable");
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
        // The fine question, answered by this binary re-executed.
        let exe = std::env::current_exe().expect("exe");
        let mut kids = ChildTable::new();
        let h = kids
            .spawn_with_env(
                exe.to_str().expect("utf8 exe"),
                &[
                    "--exact",
                    "os::tests::spawn_with_hands_descriptors_to_the_child_from_3",
                    "--nocapture",
                ],
                &[l.as_raw_fd(), l2.as_raw_fd()],
                &[("WOLF_S137_INHERIT_CHILD", &format!("{p1},{p2}"))],
            )
            .expect("re-exec");
        assert_eq!(
            kids.wait(h),
            Ok(0),
            "3 and 4 are the parent's listeners IN ORDER (10/11 = wrong \
             listener at that position, 20 = a third one above the set)"
        );
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

    // ---------------- s215: the child's descriptors ([os.proc.fds]) --

    /// The map's shape is decided before anything is read: odd, out of
    /// range, repeated, a source below -1 and too many pairs are each
    /// `invalid`; -1 is "closed in the child".
    #[test]
    fn map_of_rows() {
        assert_eq!(map_of(&[]), Ok(vec![]));
        assert_eq!(map_of(&[1, 4, 2, -1]), Ok(vec![(1, Some(4)), (2, None)]));
        assert_eq!(map_of(&[1]), Err("invalid"));
        assert_eq!(map_of(&[256, 3]), Err("invalid"));
        assert_eq!(map_of(&[-1, 3]), Err("invalid"));
        assert_eq!(map_of(&[1, 3, 1, 4]), Err("invalid"));
        assert_eq!(map_of(&[1, -2]), Err("invalid"));
        let big: Vec<i64> = (0..(MAX_FD_PAIRS as i64 + 1))
            .flat_map(|t| [t, 1])
            .collect();
        assert_eq!(map_of(&big), Err("invalid"));
    }

    fn pipe_pair() -> (i64, i64) {
        let mut out = [0i64; 2];
        assert_eq!(unsafe { __wolf_rt_os_pipe(out.as_mut_ptr() as i64) }, 0);
        assert!(out[0] >= 3 && out[1] >= 3 && out[0] != out[1], "{out:?}");
        (out[0], out[1])
    }

    fn read_all(h: i64) -> Vec<u8> {
        let mut got = Vec::new();
        loop {
            let mut pair = [0i64; 2];
            let rc = unsafe { crate::fs::__wolf_rt_fs_read(h, 4096, pair.as_mut_ptr() as i64) };
            if rc != crate::fs::fs_code::OK {
                assert_eq!(rc, crate::fs::fs_code::EOF, "read the pipe");
                return got;
            }
            got.extend_from_slice(unsafe { view(pair[0], pair[1]) }.as_bytes());
        }
    }

    fn spawn_fds_shim(exe: &str, args: &[&str], map: &[i64]) -> i64 {
        let a = crate::list::new_list(16);
        for x in args {
            crate::list::push_str(a, x);
        }
        let m = crate::list::new_list(8);
        for &x in map {
            crate::list::push_int(m, x);
        }
        unsafe { __wolf_rt_os_spawn_fds(exe.as_ptr() as i64, exe.len() as i64, a as i64, m as i64) }
    }

    fn wait_code(h: i64) -> i64 {
        let mut out = [0i64; 1];
        assert_eq!(
            unsafe { __wolf_rt_os_wait(h, out.as_mut_ptr() as i64) },
            proc_code::OK
        );
        out[0]
    }

    /// A pipe's ends are fs handles: write one, close it, read the
    /// other to its end; neither is a terminal; a forged handle is
    /// `io` to `isatty`.
    #[test]
    fn pipe_round_trip_through_the_fs_table() {
        let (r, w) = pipe_pair();
        let msg = "howl\n";
        assert_eq!(
            unsafe { crate::fs::__wolf_rt_fs_write(w, msg.as_ptr() as i64, msg.len() as i64) },
            crate::fs::fs_code::OK
        );
        let mut t = [7i64; 1];
        assert_eq!(unsafe { __wolf_rt_os_isatty(w, t.as_mut_ptr() as i64) }, 0);
        assert_eq!(t[0], 0, "a pipe is not a terminal");
        assert_eq!(crate::fs::__wolf_rt_fs_close(w), crate::fs::fs_code::OK);
        assert_eq!(read_all(r), msg.as_bytes());
        assert_eq!(crate::fs::__wolf_rt_fs_close(r), crate::fs::fs_code::OK);
        assert_eq!(
            unsafe { __wolf_rt_os_isatty(r, t.as_mut_ptr() as i64) },
            1,
            "closed"
        );
        assert_eq!(
            unsafe { __wolf_rt_os_isatty(999_999, t.as_mut_ptr() as i64) },
            1
        );
    }

    /// `os_chdir`'s rows that leave the directory alone (a test host is
    /// threaded, so no test here moves it): a missing path is
    /// `not_found`, a file is `io`.
    #[test]
    fn chdir_rows_that_do_not_move() {
        let missing = "wolf-s215-no-such-directory/inner";
        assert_eq!(
            unsafe { __wolf_rt_os_chdir(missing.as_ptr() as i64, missing.len() as i64) },
            1
        );
        let file = std::env::current_exe().expect("exe");
        let f = file.to_str().expect("utf-8 exe path");
        assert_eq!(
            unsafe { __wolf_rt_os_chdir(f.as_ptr() as i64, f.len() as i64) },
            3
        );
    }

    /// The map's order of refusal, with no child made: shape, then
    /// sources, then the program.
    #[test]
    fn spawn_fds_refuses_in_order() {
        let ghost = "wolf-s215-no-such-program";
        assert_eq!(spawn_fds_shim(ghost, &[], &[1]), -proc_code::INVALID);
        assert_eq!(
            spawn_fds_shim(ghost, &[], &[1, 999_999]),
            if cfg!(unix) {
                -proc_code::IO
            } else {
                -proc_code::UNSUPPORTED
            }
        );
        assert_eq!(spawn_fds_shim(ghost, &[], &[]), -proc_code::NOT_FOUND);
        assert_eq!(spawn_fds_shim("", &[], &[]), -proc_code::NOT_FOUND);
    }

    /// A child's stdout is a pipe this process reads.
    #[cfg(unix)]
    #[test]
    fn spawn_fds_hands_the_child_a_pipe_as_stdout() {
        let (r, w) = pipe_pair();
        let h = spawn_fds_shim("/bin/sh", &["-c", "echo hi; echo err >&2"], &[1, w, 2, w]);
        assert!(h >= 0, "spawn: {h}");
        assert_eq!(crate::fs::__wolf_rt_fs_close(w), crate::fs::fs_code::OK);
        assert_eq!(read_all(r), b"hi\nerr\n");
        assert_eq!(wait_code(h), 0);
        crate::fs::__wolf_rt_fs_close(r);
    }

    /// A descriptor above 2: the child reads 5, which this process
    /// filled; and a closed entry is closed in the child.
    #[cfg(unix)]
    #[test]
    fn spawn_fds_places_five_and_closes_two() {
        let (r5, w5) = pipe_pair();
        let msg = "five\n";
        unsafe { crate::fs::__wolf_rt_fs_write(w5, msg.as_ptr() as i64, msg.len() as i64) };
        crate::fs::__wolf_rt_fs_close(w5);
        let (r, w) = pipe_pair();
        let h = spawn_fds_shim("cat", &["/dev/fd/5"], &[5, r5, 1, w]);
        assert!(h >= 0, "spawn: {h}");
        crate::fs::__wolf_rt_fs_close(w);
        crate::fs::__wolf_rt_fs_close(r5);
        assert_eq!(read_all(r), b"five\n");
        assert_eq!(wait_code(h), 0);
        crate::fs::__wolf_rt_fs_close(r);
        let (r, w) = pipe_pair();
        let h = spawn_fds_shim(
            "/bin/sh",
            &["-c", "test -e /dev/fd/2 && echo open || echo closed"],
            &[1, w, 2, -1],
        );
        assert!(h >= 0);
        crate::fs::__wolf_rt_fs_close(w);
        assert_eq!(read_all(r), b"closed\n");
        assert_eq!(wait_code(h), 0);
        crate::fs::__wolf_rt_fs_close(r);
    }

    /// No descriptor of this process reaches a child it did not map:
    /// the child's `/dev/fd` lists exactly what a child of the test
    /// host itself lists (the control: whatever the harness hands
    /// down), with a file and a pipe open here.
    #[cfg(unix)]
    #[test]
    fn spawn_fds_leaks_nothing() {
        let control = std::process::Command::new("ls")
            .arg("/dev/fd/")
            .output()
            .expect("ls runs");
        let path = std::env::temp_dir().join(format!("wolf-s215-leak-{}", std::process::id()));
        std::fs::write(&path, "x").expect("write");
        let p = path.to_str().expect("utf-8");
        let f = unsafe { crate::fs::__wolf_rt_fs_open(p.as_ptr() as i64, p.len() as i64, 0) };
        assert!(f >= 3);
        let (r, w) = pipe_pair();
        let h = spawn_fds_shim("ls", &["/dev/fd/"], &[1, w]);
        assert!(h >= 0);
        crate::fs::__wolf_rt_fs_close(w);
        let got = read_all(r);
        assert_eq!(wait_code(h), 0);
        assert_eq!(
            String::from_utf8_lossy(&got),
            String::from_utf8_lossy(&control.stdout)
        );
        crate::fs::__wolf_rt_fs_close(r);
        crate::fs::__wolf_rt_fs_close(f);
        let _ = std::fs::remove_file(&path);
    }

    /// windows refuses a non-empty map by name, with no child; an empty
    /// map is the plain spawn.
    #[cfg(windows)]
    #[test]
    fn spawn_fds_refuses_a_map_by_name_on_windows() {
        let (r, w) = pipe_pair();
        assert_eq!(
            spawn_fds_shim("cmd", &["/c", "exit 0"], &[1, w]),
            -proc_code::UNSUPPORTED
        );
        let h = spawn_fds_shim("cmd", &["/c", "exit 3"], &[]);
        assert!(h >= 0);
        assert_eq!(wait_code(h), 3);
        crate::fs::__wolf_rt_fs_close(r);
        crate::fs::__wolf_rt_fs_close(w);
    }

    fn spawn_job_shim(
        exe: &str,
        args: &[&str],
        map: &[i64],
        group: i64,
        tty: i64,
        defaults: i64,
    ) -> i64 {
        let a = crate::list::new_list(16);
        for x in args {
            crate::list::push_str(a, x);
        }
        let m = crate::list::new_list(8);
        for &x in map {
            crate::list::push_int(m, x);
        }
        unsafe {
            __wolf_rt_os_spawn_job(
                exe.as_ptr() as i64,
                exe.len() as i64,
                a as i64,
                m as i64,
                group,
                tty,
                defaults,
            )
        }
    }

    fn wait_status_of(h: i64) -> i64 {
        let mut out = [0i64; 1];
        assert_eq!(
            unsafe { __wolf_rt_os_wait_status(h, out.as_mut_ptr() as i64) },
            proc_code::OK
        );
        out[0]
    }

    /// s219 (`[os.proc.job]`): the shape is read first — a group below
    /// -1, a tty below -1, a defaults bit outside the nine meanings are
    /// `invalid` with no child made.
    #[test]
    fn spawn_job_shape_is_invalid_first() {
        assert_eq!(
            spawn_job_shim("true", &[], &[], -2, -1, 0),
            -proc_code::INVALID
        );
        assert_eq!(
            spawn_job_shim("true", &[], &[], -1, -2, 0),
            -proc_code::INVALID
        );
        assert_eq!(
            spawn_job_shim("true", &[], &[], -1, -1, 512),
            -proc_code::INVALID
        );
        assert_eq!(
            spawn_job_shim("true", &[], &[1], -1, -1, 0),
            -proc_code::INVALID
        );
    }

    /// s219 (`[os.proc.status]`): an exit code is itself, a death by
    /// signal N is -N, and the handle is reaped (a second wait is `io`).
    #[cfg(unix)]
    #[test]
    fn wait_status_carries_the_signal_number() {
        let h = spawn_job_shim("false", &[], &[], -1, -1, 0);
        assert!(h >= 0);
        assert_eq!(wait_status_of(h), 1);
        let h = spawn_job_shim("sleep", &["30"], &[], -1, -1, 0);
        assert!(h >= 0);
        assert_eq!(__wolf_rt_os_kill(h), proc_code::OK);
        assert_eq!(wait_status_of(h), -i64::from(libc::SIGKILL));
        let mut out = [0i64; 1];
        assert_eq!(
            unsafe { __wolf_rt_os_wait_status(h, out.as_mut_ptr() as i64) },
            proc_code::IO
        );
    }

    /// s219: group 0 makes the child the leader of a new group (its
    /// group id is its pid, not ours); a later child joins it by id;
    /// os_proc_pid answers the pid and is `io` for a forged handle.
    #[cfg(unix)]
    #[test]
    fn spawn_job_leads_and_joins_a_group() {
        let a = spawn_job_shim("sleep", &["30"], &[], 0, -1, 0);
        assert!(a >= 0);
        let pa = __wolf_rt_os_proc_pid(a);
        assert!(pa > 0);
        let b = spawn_job_shim("sleep", &["30"], &[], pa, -1, 0);
        assert!(b >= 0);
        let pb = __wolf_rt_os_proc_pid(b);
        // SAFETY: plain getpgid on our own children.
        unsafe {
            assert_eq!(i64::from(libc::getpgid(pa as libc::pid_t)), pa);
            assert_eq!(i64::from(libc::getpgid(pb as libc::pid_t)), pa);
            assert_ne!(i64::from(libc::getpgrp()), pa);
        }
        for h in [a, b] {
            assert_eq!(__wolf_rt_os_kill(h), proc_code::OK);
            assert_eq!(wait_status_of(h), -i64::from(libc::SIGKILL));
        }
        assert_eq!(__wolf_rt_os_proc_pid(1_000_000), -proc_code::IO);
    }

    /// s219: joining a group that does not exist is the hook's EPERM,
    /// `denied`, with no child left behind.
    #[cfg(unix)]
    #[test]
    fn spawn_job_into_a_missing_group_is_denied() {
        // pid 1's group belongs to another session.
        assert_eq!(
            spawn_job_shim("true", &[], &[], 1, -1, 0),
            -proc_code::DENIED
        );
    }

    /// s219: a tty that is not a terminal is `io` before any child.
    #[cfg(unix)]
    #[test]
    fn spawn_job_tty_must_be_a_terminal() {
        let p = std::env::temp_dir().join(format!("wolf-rt-job-{}", std::process::id()));
        std::fs::write(&p, b"x").unwrap();
        let h = crate::fs::mint_file(std::fs::File::open(&p).unwrap());
        assert_eq!(spawn_job_shim("true", &[], &[], 0, h, 0), -proc_code::IO);
        let _ = std::fs::remove_file(&p);
    }
}
