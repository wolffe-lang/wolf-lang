//! s253 (wolf-lang#811, #812, #815, #819) — the process calls boreutils
//! and yew met: no call made a process's end core-free or set its core
//! limit; a wolf program held a handler on 33 it never installed; an
//! inherited descriptor >= 3 could not be used as a file handle; and a
//! program could not ask which OS and architecture it runs on.
//! `[os.proc.core]`, `[os.signal.host.reserved]`, `[os.proc.adopt]`,
//! `[os.uname]`.
//!
//! Each witness runs on the checked machine and lupin under `conform-run`
//! and as native and release binaries, and every machine's standard
//! output and standard error are printed, so a `--nocapture` log is the
//! table.
//!
//! **Nothing here signals a process the test did not start, and nothing
//! is signalled, faulted or dumped on a linux host at all.** The
//! witnesses are three kinds:
//!
//! - EVERY HOST (windows too): the OS and architecture words
//!   (`host.lu`), the shape rows of the core limit
//!   (`corpus/os/core_limit_rows.lu`) and of the C library's numbers
//!   (`corpus/os/signal_reserved_rows.lu`) — nothing is sent, and the one
//!   change a row makes (a limit set to itself, a disposition set back) is
//!   undone.
//! - ON A UNIX HOST: a program started with descriptor 5 on a file adopts
//!   it (`adopt.lu`) — nothing is sent; windows answers its shape rows.
//! - LIVE: a child that ends by SEGV after lowering its core limit or
//!   after `os_core_off` (`core_parent.lu` starting `core_child.lu`), and
//!   a 33 sent to a child with no thread and to one that ran a task
//!   (`reserved_parent.lu` starting `reserved_child.lu`) — on linux each
//!   machine in a throwaway `--network none` container of its own, NO init
//!   (the shell is process 1, so the programs are not), the test's own
//!   directory its working directory, `ulimit -c 0` unless the row is the
//!   cores-on one; on macOS on CI's throwaway runner only
//!   (`GITHUB_ACTIONS`), `ulimit -c 0`, and never the reserved rows (macOS
//!   has no real-time signals). The process rows also run under
//!   `taskset` on four CPUs of the allocation where there are four cpus (wolf-lang#570).
//!
//! **Cores.** A core is written only where the container's working
//! directory is, the test's own scratch directory, and removed there. A
//! host whose `kernel.core_pattern` pipes cores to a program
//! (`systemd-coredump` on kasumi) pipes a core even under `ulimit -c 0`
//! (core(5): "The RLIMIT_CORE limit is not enforced for core dumps that
//! are piped to a program"), so the cores-off rows expect the core bit
//! there for every end but the one after `os_core_off`, and the cores-on
//! row (which would write outside the test's directory) SKIPs there, by
//! name. CI's linux job sets a file pattern and `WOLF_CORE_REQUIRE=1`
//! makes that skip a failure.
//!
//! **The engine.** `WOLF_JOBWAIT_ENGINE` names it (s242's), otherwise the
//! first of `docker` and `podman` that answers; `WOLF_JOBWAIT_IMAGE` the
//! image (default `docker.io/library/ubuntu:24.04`). Where no engine runs
//! it the container witnesses SKIP, loudly and by name — and
//! `WOLF_JOBWAIT_REQUIRE_CONTAINER=1` (CI's linux job) makes that a
//! failure.
//!
//! **lupin 0.1.55**, the pairing, predates every new answer: none of the
//! six names resolves. Each is pinned here by version, exactly as
//! measured; a newer lupin is held to the mirror (wolf-interp issues
//! listed in the PR).

mod lane_exit;
mod test_cpus;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A `REPORT` line for the CI log even when the test passes: the
/// process's own standard error, past libtest's capture.
fn report(line: &str) {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

/// The pairing at this lane, which predates the mirror of every answer
/// s253 adds (each pinned below by what it was measured to say).
// r40: the published 0.1.56 archive still predates these six names.
// r41: the published .57 retains these measured pre-mirror answers.
// r41 resume: published .58 re-measured the same named gaps.
const PRE_MIRROR_LUPIN: &[&str] = &["0.1.55", "0.1.56", "0.1.57", "0.1.58"];

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    reason: String,
    version: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let text = String::from_utf8_lossy(bytes).replace('\r', "");
    let rec: serde_json::Value = text
        .lines()
        .rev()
        .find_map(|l| {
            l.find('{')
                .and_then(|i| serde_json::from_str(l[i..].trim()).ok())
        })
        .unwrap_or_else(|| panic!("{what} record parses: {text}"));
    let field = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: field("verdict"),
        stdout: field("stdout_inline"),
        reason: {
            let r = field("x-unsupported");
            if r.is_empty() {
                field("x-unsupported-construct")
            } else {
                r
            }
        },
        version: field("impl_version"),
    }
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/process_calls")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn corpus(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(name);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// The stdout a corpus row's own header declares.
fn declared_stdout(entry: &Path) -> String {
    let text = std::fs::read_to_string(entry).expect("corpus row reads");
    let line = text
        .lines()
        .find(|l| l.starts_with("//! check: run("))
        .unwrap_or_else(|| panic!("{} declares a run", entry.display()));
    let start = line.find("stdout=\"").expect("a declared stdout") + "stdout=\"".len();
    let end = line.rfind("\")").expect("the directive closes");
    line[start..end].replace("\\n", "\n") + "\n"
}

/// A fresh working directory of the test's own.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("process_calls_lanes")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

fn run_in(mut cmd: Command, dir: &Path) -> Output {
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.output().expect("spawn")
}

/// The native or release binary of `entry`, as `dir/<stem>-<tier>`. A
/// compile error FAILS the gate (it is the defect at a pre-fix head,
/// never an environment); an environment that cannot build the tier
/// skips loudly.
fn build(entry: &Path, dir: &Path, stem: &str, release: bool) -> Option<PathBuf> {
    let tier = if release { "release" } else { "native" };
    let exe = dir.join(format!("{stem}-{tier}{}", std::env::consts::EXE_SUFFIX));
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(entry)
        .arg("-o")
        .arg(&exe)
        .arg("--no-cache");
    if release {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf build runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("error["),
        "wolf build {} refused the program (a compile error is the defect, never a skip):\n{stderr}",
        entry.display()
    );
    if lane_exit::environment_refusal(&out, "wolf build") {
        eprintln!(
            "SKIP: environment cannot build the {tier} tier: {}",
            stderr.trim()
        );
        return None;
    }
    assert!(out.status.success(), "wolf build failed: {stderr}");
    Some(exe)
}

/// The sibling lupin, found exactly as `pairing.rs` finds it, or a loud
/// skip — a failure under `WOLF_PAIRING_REQUIRE_SIBLING`.
fn lupin_or_skip() -> Option<PathBuf> {
    let found = if let Some(p) = std::env::var_os("LUPIN") {
        let p = PathBuf::from(p);
        p.is_file().then_some(p)
    } else {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut hit = None;
        while dir.pop() {
            let candidate = dir.join("../wolf-interp/target/release/lupin");
            if candidate.is_file() {
                hit = Some(candidate);
                break;
            }
        }
        hit
    };
    if found.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of s253's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    found
}

/// What lupin is held to for one witness: 0.1.55's measured answer, and
/// the mirror's for anything newer.
enum Lupin<'a> {
    /// 0.1.55 stops at a name it does not resolve (one of these); a newer
    /// lupin mirrors the compiler.
    Unresolved(&'a [&'a str]),
    /// Every lupin refuses by name, at one of these calls: the program
    /// meets a call lupin refuses by design (no `sigaction`, no
    /// `SIGRTMIN` in std: ruling #79 R8) before any new name.
    Refuses(&'a [&'a str]),
}

fn judge_lupin(obs: &Obs, name: &str, want: &str, lupin: &Lupin<'_>) {
    eprintln!(
        "== lupin {} on {name}: {} {}\n{}",
        obs.version, obs.verdict, obs.reason, obs.stdout
    );
    let pre = PRE_MIRROR_LUPIN.contains(&obs.version.as_str());
    let refuses = |calls: &[&str], unresolved: bool| {
        assert_eq!(
            obs.verdict, "unsupported",
            "lupin {} refuses {name} by name: {obs:?}",
            obs.version
        );
        assert!(
            calls.iter().any(|c| obs.reason.contains(&format!("`{c}`"))),
            "lupin {}'s refusal of {name} names one of {calls:?}: {}",
            obs.version,
            obs.reason
        );
        if unresolved {
            assert!(
                obs.reason.contains("does not resolve"),
                "lupin {} (pre-mirror) does not resolve {calls:?}: {}",
                obs.version,
                obs.reason
            );
        }
    };
    match *lupin {
        Lupin::Unresolved(calls) => {
            if pre {
                refuses(calls, true);
            } else {
                assert_eq!(
                    (obs.verdict.as_str(), obs.stdout.as_str()),
                    ("exit(0)", want),
                    "lupin {} on {name}: {obs:?}",
                    obs.version
                );
            }
        }
        Lupin::Refuses(calls) => refuses(calls, false),
    }
}

/// `entry` on every machine, ON THE HOST, in `dir`: the checked machine
/// and lupin under `conform-run`, the native and release binaries run
/// directly. Each stdout is held to `want`.
fn every_lane_answers_on_the_host(entry: &Path, dir: &Path, want: &str, lupin: Lupin<'_>) {
    let name = entry.file_name().unwrap().to_string_lossy().to_string();
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--checked")
        .arg("--json");
    let out = run_in(cmd, dir);
    let checked = parse_obs(&out.stdout, "the observation");
    eprintln!(
        "== checked on {name}: {} {}\n{}{}",
        checked.verdict,
        checked.reason,
        checked.stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        (checked.verdict.as_str(), checked.stdout.as_str()),
        ("exit(0)", want),
        "the CHECKED machine on {name}: {checked:?}"
    );
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(entry, dir, "a", release) else {
            continue;
        };
        let out = run_in(Command::new(&exe), dir);
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        eprintln!(
            "== {tier} on {name}: exit {:?}\n{stdout}{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(0), "the {tier} binary on {name}");
        assert_eq!(stdout, want, "the {tier} binary on {name}");
    }
    if let Some(l) = lupin_or_skip() {
        let mut cmd = Command::new(&l);
        cmd.arg("conform-run").arg(entry).arg("--json");
        let out = run_in(cmd, dir);
        judge_lupin(
            &parse_obs(&out.stdout, "lupin's observation"),
            &name,
            want,
            &lupin,
        );
    }
}

// ------------------------------------------------------ every host --

/// The words `[os.uname]` fixes for a kernel's `sysname` and `machine`,
/// written here apart from the runtime's (the test is the oracle).
#[cfg(unix)]
fn host_words() -> (String, String) {
    // SAFETY: `uname` fills a zeroed struct; each field is NUL-terminated.
    let u = unsafe {
        let mut u: libc::utsname = std::mem::zeroed();
        assert_eq!(libc::uname(&mut u), 0, "uname answers");
        u
    };
    let field = |f: &[libc::c_char]| -> String {
        let b: Vec<u8> = f
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        String::from_utf8_lossy(&b).to_ascii_lowercase()
    };
    let os = match field(&u.sysname).as_str() {
        "darwin" => "macos".to_string(),
        other => other.to_string(),
    };
    let arch = match field(&u.machine).as_str() {
        "arm64" => "aarch64".to_string(),
        "amd64" => "x86_64".to_string(),
        other => other.to_string(),
    };
    (os, arch)
}

#[cfg(not(unix))]
fn host_words() -> (String, String) {
    ("windows".to_string(), std::env::consts::ARCH.to_string())
}

/// #819: `os_name()` and `os_arch()` answer the kernel's words on every
/// machine — `linux`/`x86_64` on kasumi and CI's linux runner, `macos`/
/// `aarch64` on CI's macOS runner, `windows` there — the same words the
/// test reads from its own `uname`. Red at 0.2.34: E0301 (yew's
/// `witness/host-os`).
#[test]
fn the_host_names_its_kernel() {
    let dir = scratch("host");
    let (os, arch) = host_words();
    report(&format!(
        "REPORT the host's words here: name={os} arch={arch}"
    ));
    #[cfg(target_os = "linux")]
    assert_eq!(os, "linux", "a linux test binary on a linux kernel");
    #[cfg(target_os = "macos")]
    assert_eq!(os, "macos", "a macOS test binary on Darwin");
    let want = format!("name={os}\narch={arch}\n");
    every_lane_answers_on_the_host(
        &fixture("host.lu"),
        &dir,
        &want,
        Lupin::Unresolved(&["os_name"]),
    );
}

/// #811: the shape rows of `os_set_core_limit`, decided before any host is
/// asked, and the reader's shape — one truth on every host, windows
/// included. Red at 0.2.34: E0301 (no name resolves).
#[test]
fn the_core_limit_shape_rows_on_every_host() {
    let dir = scratch("core_rows");
    let entry = corpus("os/core_limit_rows.lu");
    let want = declared_stdout(&entry);
    every_lane_answers_on_the_host(
        &entry,
        &dir,
        &want,
        Lupin::Unresolved(&["os_set_core_limit", "os_core_limit"]),
    );
}

/// #812: the C library's numbers are refused by name in the calls that set
/// a disposition or end the process by a signal, their words answer, the
/// first number a program may use is its own, and of those numbers the
/// program catches none it did not listen for — on every host. Red at
/// 0.2.34: `ignore_one(-32)` was `io`, and `os_exit_signal(-32)` ended the
/// program (exit 160).
#[test]
fn the_c_librarys_numbers_on_every_host() {
    let dir = scratch("reserved_rows");
    let entry = corpus("os/signal_reserved_rows.lu");
    let want = declared_stdout(&entry);
    every_lane_answers_on_the_host(&entry, &dir, &want, Lupin::Refuses(&["os_signal_realtime"]));
}

// ------------------------------------------------------------ unix --

/// `prog` (with `args`) started in `dir` with nothing open above 2 but
/// descriptor 5, which is `out` opened for writing (in the child, between
/// fork and exec: every other descriptor the test process holds — a
/// jobserver's, libtest's — is closed first).
#[cfg(unix)]
fn run_with_five(prog: &Path, args: &[&str], dir: &Path, out: &Path) -> Output {
    use std::os::fd::AsRawFd as _;
    use std::os::unix::process::CommandExt as _;
    let file = std::fs::File::create(out).expect("the out file");
    let raw = file.as_raw_fd();
    eprintln!("REPORT run_with_five: source fd={raw}, target fd=5");
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: between fork and exec the closure makes only system calls.
    unsafe {
        cmd.pre_exec(move || {
            if libc::dup2(raw, 5) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // dup2(5, 5) is a no-op: File::create's close-on-exec bit
            // survives unless we clear it explicitly on our target.
            let flags = libc::fcntl(5, libc::F_GETFD);
            if flags < 0 || libc::fcntl(5, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            for fd in 3..1024 {
                if fd != 5 {
                    libc::close(fd);
                }
            }
            Ok(())
        });
    }
    let o = cmd.output().expect("the program runs");
    drop(file);
    o
}

/// What `adopt.lu` prints on this host, its file named `out`.
#[cfg(unix)]
fn adopt_want(out: &str) -> String {
    let link = |l: &str| {
        if cfg!(target_os = "linux") {
            l.to_string()
        } else {
            "none".to_string()
        }
    };
    format!(
        "zero=invalid one=invalid two=invalid negative=invalid past=invalid\n\
         link before: {}\n\
         a child before the adopt holds 5: yes\n\
         adopted as handle 3\n\
         link after the adopt: {}\n\
         5 again while it is a handle: io\n\
         every other number in 3..63 not io: none\n\
         a child after the adopt holds 5: no\n\
         5 after the close: io\n\
         link after the close: none\n",
        link(out),
        link(out)
    )
}

/// #903: a release-only seam for the repeated fd-5 witness. The
/// subprocess runner reserves 3/4 so File::create must return 5;
/// ordinary workspace tests also exercise this helper without reserves.
#[cfg(unix)]
#[test]
fn inherited_descriptor_adoption_release_only() {
    let dir = scratch("adopt_release_fd5");
    let exe = build(&fixture("adopt.lu"), &dir, "adopt", true)
        .expect("the release-only descriptor witness must compile");
    let file = dir.join("out-release");
    let out = run_with_five(&exe, &[], &dir, &file);
    let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
    eprintln!(
        "REPORT release-only adopt: status={:?}, stdout={stdout:?}, stderr={:?}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stdout, adopt_want("out-release"));
    assert_eq!(std::fs::read_to_string(file).unwrap(), "X\nR");
}

/// #815: a program started with descriptor 5 open on a file adopts it as
/// an fs handle (3, its first), writes through it and closes it; the
/// standard three, a negative number and one past `i32` are `invalid`;
/// 5 again while it is a handle (now close-on-exec), every other number in
/// 3..63 (the runtime's own descriptors among them, each close-on-exec)
/// and 5 after its close are `io`; a child spawned before the adoption
/// held 5 and wrote `X`, one after does not. On every machine, the
/// checked one adopting the `wolf` process's own 5; linux reads
/// `/proc/self/fd/5` around it. Red at 0.2.34: E0301 (yew's
/// `witness/inherited-fd` wrote nothing: `io`).
#[cfg(unix)]
#[test]
fn an_inherited_descriptor_is_adopted() {
    let dir = scratch("adopt");
    let entry = fixture("adopt.lu");
    let check = |machine: &str, out: &Output, file: &Path| {
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read_to_string(file).unwrap_or_default();
        report(&format!(
            "REPORT adopt.lu {machine}: exit {:?}: {} || {} || {name}: {bytes:?}",
            out.status.code(),
            stdout.replace('\n', " | "),
            String::from_utf8_lossy(&out.stderr).replace('\n', " | ")
        ));
        (stdout, bytes, name)
    };
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, &dir, "adopt", release) else {
            continue;
        };
        let file = dir.join(format!("out-{tier}"));
        let out = run_with_five(&exe, &[], &dir, &file);
        let (stdout, bytes, name) = check(tier, &out, &file);
        assert_eq!(out.status.code(), Some(0), "the {tier} binary");
        assert_eq!(stdout, adopt_want(&name), "#815: the {tier} binary");
        assert_eq!(
            bytes, "X\nR",
            "#815: the {tier} binary's child and its own write"
        );
    }
    let file = dir.join("out-checked");
    let entry_s = entry.display().to_string();
    let out = run_with_five(
        Path::new(wolf()),
        &["conform-run", &entry_s, "--checked", "--json"],
        &dir,
        &file,
    );
    let checked = parse_obs(&out.stdout, "the checked observation");
    let (_, bytes, name) = check("checked", &out, &file);
    eprintln!(
        "== checked: {} {}\n{}",
        checked.verdict, checked.reason, checked.stdout
    );
    assert_eq!(
        (checked.verdict.as_str(), checked.stdout.as_str()),
        ("exit(0)", adopt_want(&name).as_str()),
        "#815: the CHECKED machine adopts the wolf process's 5: {checked:?}"
    );
    assert_eq!(
        bytes, "X\nR",
        "#815: the checked machine's child and its own write"
    );
    if let Some(l) = lupin_or_skip() {
        let file = dir.join("out-lupin");
        let out = run_with_five(&l, &["conform-run", &entry_s, "--json"], &dir, &file);
        let obs = parse_obs(&out.stdout, "lupin's observation");
        // lupin has no signal family: the program's listen, before any
        // new name, is refused by name on every lupin.
        judge_lupin(
            &obs,
            "adopt.lu",
            &adopt_want("out-lupin"),
            &Lupin::Refuses(&["os_signal_listen"]),
        );
    }
}

/// #815 on windows: the shape rows, then `unsupported` for 5, by name.
#[cfg(not(unix))]
#[test]
fn an_inherited_descriptor_is_adopted() {
    let dir = scratch("adopt");
    every_lane_answers_on_the_host(
        &fixture("adopt.lu"),
        &dir,
        "zero=invalid one=invalid two=invalid negative=invalid past=invalid\nadopt 5: unsupported\n",
        Lupin::Refuses(&["os_signal_listen"]),
    );
}

// ------------------------------------------------------------ live --

/// Where the live witnesses run: a throwaway container (linux) or CI's
/// throwaway runner (macOS).
#[cfg(unix)]
struct Live {
    /// `Some(engine)` in a container; `None` on the runner itself.
    engine: Option<String>,
    /// The test's own directory: `/w` in the container, the cwd on the
    /// runner. A core is written here and nowhere else.
    dir: PathBuf,
    lupin: Option<PathBuf>,
}

#[cfg(unix)]
fn image() -> String {
    std::env::var("WOLF_JOBWAIT_IMAGE").unwrap_or_else(|_| "docker.io/library/ubuntu:24.04".into())
}

#[cfg(unix)]
impl Live {
    /// The place to run, or a loud skip (a failure where CI requires it).
    fn new(test: &str) -> Option<Live> {
        let dir = scratch(test);
        if cfg!(target_os = "macos") {
            if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
                eprintln!(
                    "SKIP: {test}: not CI's throwaway runner — the live witnesses signal their \
                     own processes and run on macOS only there"
                );
                return None;
            }
            return Some(Live {
                engine: None,
                dir,
                lupin: lupin_or_skip(),
            });
        }
        let required = std::env::var_os("WOLF_JOBWAIT_REQUIRE_CONTAINER").is_some();
        let candidates: Vec<String> = match std::env::var("WOLF_JOBWAIT_ENGINE") {
            Ok(e) => vec![e],
            Err(_) => vec!["docker".into(), "podman".into()],
        };
        let mut why = Vec::new();
        for e in &candidates {
            let out = Command::new(e)
                .args(["run", "--rm", "--network", "none"])
                .arg(image())
                .args(["sh", "-c", "echo $$"])
                .stdin(Stdio::null())
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    let pid = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    assert_eq!(
                        pid, "1",
                        "{e}: the container's shell is not its pid namespace's process 1"
                    );
                    return Some(Live {
                        engine: Some(e.clone()),
                        dir,
                        lupin: lupin_or_skip(),
                    });
                }
                Ok(o) => why.push(format!(
                    "{e}: {}",
                    String::from_utf8_lossy(&o.stderr)
                        .lines()
                        .next()
                        .unwrap_or("")
                )),
                Err(err) => why.push(format!("{e}: {err}")),
            }
        }
        assert!(
            !required,
            "WOLF_JOBWAIT_REQUIRE_CONTAINER is set and no engine ran {} — {test} did not run: \
             {why:?}",
            image()
        );
        eprintln!(
            "SKIP: {test}: no container engine ran {} ({why:?}) — the live witnesses did not run",
            image()
        );
        None
    }

    /// A path in the test's directory as the program sees it.
    fn at(&self, name: &str) -> String {
        match self.engine {
            Some(_) => format!("/w/{name}"),
            None => self.dir.join(name).display().to_string(),
        }
    }

    /// The `wolf` binary, as the program sees it.
    fn wolf_at(&self) -> String {
        match self.engine {
            Some(_) => "/machines/wolf".into(),
            None => wolf().into(),
        }
    }

    fn lupin_at(&self) -> Option<String> {
        self.lupin.as_ref().map(|l| match self.engine {
            Some(_) => "/machines/lupin".into(),
            None => l.display().to_string(),
        })
    }

    fn fixture_at(&self, name: &str) -> String {
        match self.engine {
            Some(_) => format!("/fx/{name}"),
            None => fixture(name).display().to_string(),
        }
    }

    /// Write each `(name, value)` as `s253.<name>` in `sub` of the test's
    /// directory ("" for itself), where the fixtures read their settings.
    fn conf(&self, sub: &str, conf: &[(&str, &str)]) {
        let d = self.dir.join(sub);
        std::fs::create_dir_all(&d).expect("the settings' directory");
        for (k, v) in conf {
            std::fs::write(d.join(format!("s253.{k}")), v).expect("a setting is written");
        }
    }

    /// `script` under `sh -c` in the test's directory: in a throwaway
    /// container (no network, NO init, the test's directory at `/w`
    /// read-write and its working directory, the fixtures and machines
    /// read-only), or on the runner.
    fn sh(&self, script: &str) -> Output {
        let mut cmd = match &self.engine {
            Some(e) => {
                let mut c = Command::new(e);
                c.args(["run", "--rm", "--network", "none"]);
                c.arg("-v").arg(format!("{}:/w", self.dir.display()));
                c.arg("-v").arg(format!(
                    "{}:/fx:ro",
                    fixture("host.lu").parent().unwrap().display()
                ));
                c.arg("-v").arg(format!("{}:/machines/wolf:ro", wolf()));
                if let Some(l) = &self.lupin {
                    c.arg("-v")
                        .arg(format!("{}:/machines/lupin:ro", l.display()));
                }
                c.args(["-w", "/w"]).arg(image()).args(["sh", "-c", script]);
                c
            }
            None => {
                let mut c = Command::new("sh");
                c.args(["-c", script]).current_dir(&self.dir);
                c
            }
        };
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.output().expect("the engine (or sh) runs")
    }

    /// The cpus the place has (four or more lights the taskset rows).
    fn four_cpus(&self) -> bool {
        if cfg!(target_os = "macos") {
            return false;
        }
        let out = self.sh("nproc");
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse::<usize>()
            .unwrap_or(1)
            >= 4
    }

    /// The host's `kernel.core_pattern` pipes cores to a program.
    fn cores_piped(&self) -> bool {
        cfg!(target_os = "linux")
            && std::fs::read_to_string("/proc/sys/kernel/core_pattern")
                .is_ok_and(|p| p.trim_start().starts_with('|'))
    }
}

/// A compiled machine's row: the binary's name in the test's directory,
/// and again under `taskset` on four of the allocation (`test_cpus`).
#[cfg(unix)]
fn compiled_rows(live: &Live, entry: &Path, stem: &str, test: &str) -> Vec<(String, String)> {
    // The pinned rows: four of the allocation (`test_cpus`, wolf-lang#925),
    // asked of the place's own `taskset`.
    let four = if live.four_cpus() {
        test_cpus::subset(4, test)
    } else {
        if cfg!(target_os = "linux") {
            eprintln!("SKIP: {test}: fewer than four cpus — the pinned rows did not run");
        }
        None
    };
    let mut rows = Vec::new();
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(entry, &live.dir, stem, release) else {
            continue;
        };
        let name = exe.file_name().unwrap().to_string_lossy().to_string();
        rows.push((tier.to_string(), live.at(&name)));
        if let Some(list) = &four {
            rows.push((
                format!("{tier} taskset -c {list}"),
                format!("taskset -c {list} {}", live.at(&name)),
            ));
        }
    }
    rows
}

/// The checked machine's (or lupin's) record of `fixture` in the place,
/// in `sub` of the test's directory, with `conf` written there first and
/// `limit` (a `ulimit` line) before it.
#[cfg(unix)]
fn record_in(
    live: &Live,
    machine: &str,
    flags: &str,
    name: &str,
    sub: &str,
    limit: &str,
    conf: &[(&str, &str)],
) -> (Obs, String) {
    live.conf(sub, conf);
    let out = live.sh(&format!(
        "{limit}; cd {} && {machine} conform-run {} {flags} --json",
        live.at(sub),
        live.fixture_at(name)
    ));
    let obs = parse_obs(&out.stdout, &format!("{machine} on {name}"));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    eprintln!(
        "== {machine} on {name} {conf:?}: {} {}\n{}{stderr}",
        obs.verdict, obs.reason, obs.stdout,
    );
    (obs, stderr)
}

/// The bits `mode`'s child reported for `field` (`SigIgn`, `SigCgt`) in a
/// run's standard error (`reserved_child.lu` writes them before it says it
/// is ready), or `None` when it wrote none.
#[cfg(target_os = "linux")]
fn child_mask(stderr: &str, mode: &str, field: &str) -> Option<u64> {
    let key = format!("{mode}: {field}: ");
    let line = stderr.lines().find(|l| l.starts_with(&key))?;
    u64::from_str_radix(line[key.len()..].trim(), 16).ok()
}

/// How a `wait` child that was sent 33 ends, by the masks it had at the
/// send: with 33 caught (glibc's handler, a thread started) or ignored (as
/// glibc's `posix_spawn` starts a child where the kernel refuses `clone3`:
/// CI's docker runner, measured) it lives and exits 0; at the default it
/// dies of 33.
#[cfg(target_os = "linux")]
fn end_by_masks(stderr: &str, mode: &str) -> &'static str {
    let bit = 1u64 << 32;
    let ign = child_mask(stderr, mode, "SigIgn").expect("the child's SigIgn");
    let cgt = child_mask(stderr, mode, "SigCgt").expect("the child's SigCgt");
    if ign & bit != 0 || cgt & bit != 0 {
        "exit 0"
    } else {
        "signal 33"
    }
}

/// The limit a `read` reported against the shell's own, read by the
/// shell from `/proc/self/limits` after its `ulimit` (linux; bytes,
/// `unlimited` as -1).
#[cfg(unix)]
fn limits_line(text: &str) -> Option<(i64, i64)> {
    let line = text.lines().find(|l| l.starts_with("Max core file size"))?;
    let words: Vec<&str> = line.split_whitespace().collect();
    let num = |w: &str| -> Option<i64> {
        if w == "unlimited" {
            Some(-1)
        } else {
            w.parse().ok()
        }
    };
    Some((num(words.get(4)?)?, num(words.get(5)?)?))
}

/// #811: what a process can know and set of its own core limit, and an
/// end by SEGV that dumps no core. `core_child.lu`'s binary (native,
/// release) reads the limit it was born with — the shell's, as
/// `/proc/self/limits` says on linux — lowers it to 0 and 0, and is
/// `denied` a hard limit above that; the checked machine does the same to
/// the `wolf` process it runs in. Then `core_parent.lu`, on every machine,
/// reports through `os_wait_status_core` how the child ended by SEGV three
/// ways: as it was born, after `os_set_core_limit(0, 0)`, after
/// `os_core_off()`. Cores off (`ulimit -c 0`): a host that pipes cores
/// dumps the first two anyway (core(5)), a file pattern neither; the third
/// never. Cores on (`ulimit -c unlimited`, a file pattern only, CI's
/// linux job): the first dumps a core file in the test's directory, the
/// second and third none. Red at 0.2.34: E0301 (no name resolves).
#[cfg(unix)]
#[test]
fn a_core_limit_and_an_end_with_no_core() {
    let test = "core";
    let Some(live) = Live::new(test) else {
        return;
    };
    let child = fixture("core_child.lu");
    let parent = fixture("core_parent.lu");
    // What a process can know and set of its limit.
    let mut kids = Vec::new();
    for release in [false, true] {
        let Some(c) = build(&child, &live.dir, "child", release) else {
            continue;
        };
        kids.push(live.at(&c.file_name().unwrap().to_string_lossy()));
    }
    let lower_raise = "lower to 0 0: ok\nnow: 0 0\nraise the hard to 4096: denied\nnow: 0 0\n";
    for kid in &kids {
        let out = live.sh(&format!(
            "ulimit -S -c 0; {kid} read; cat /proc/self/limits 2>/dev/null; {kid} lower-raise"
        ));
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        report(&format!(
            "REPORT core_child.lu {kid} read, lower-raise: {} || {}",
            stdout.replace('\n', " | "),
            String::from_utf8_lossy(&out.stderr).replace('\n', " | ")
        ));
        let first = stdout.lines().next().unwrap_or("");
        let born: Vec<i64> = first.split(' ').filter_map(|w| w.parse().ok()).collect();
        assert_eq!(
            born.len(),
            2,
            "#811: {kid} read prints two numbers: {first:?}"
        );
        assert_eq!(
            born[0], 0,
            "#811: {kid} was born with the shell's soft limit 0"
        );
        if cfg!(target_os = "linux") {
            let shell = limits_line(&stdout).expect("the shell's /proc/self/limits");
            assert_eq!(
                (born[0], born[1]),
                shell,
                "#811: {kid} reads the limit it was born with, the shell's"
            );
        } else {
            assert!(
                born[1] == -1 || born[1] >= 0,
                "#811: a hard limit: {first:?}"
            );
        }
        assert!(
            stdout.ends_with(lower_raise),
            "#811: {kid} lowers its limit and is denied a raise: {stdout:?}"
        );
    }
    let (checked, _) = record_in(
        &live,
        &live.wolf_at(),
        "--checked",
        "core_child.lu",
        "checked-lower-raise",
        "ulimit -S -c 0",
        &[("mode", "lower-raise")],
    );
    assert_eq!(
        (checked.verdict.as_str(), checked.stdout.as_str()),
        ("exit(0)", lower_raise),
        "#811: the CHECKED machine lowers the wolf process's limit and is denied a raise"
    );
    // The three ends, through the `_core` wait.
    let piped = live.cores_piped();
    let mut legs = vec![("off", "ulimit -c 0", piped)];
    if cfg!(target_os = "macos") {
        eprintln!("SKIP: core (on): macOS writes cores to /cores, outside the test's directory");
    } else if piped {
        assert!(
            std::env::var_os("WOLF_CORE_REQUIRE").is_none(),
            "WOLF_CORE_REQUIRE is set and this host pipes cores to a program — the cores-on row \
             did not run"
        );
        eprintln!(
            "SKIP: core (on): this host's core_pattern pipes cores to a program, which would \
             write the core outside the test's directory"
        );
    } else {
        legs.push(("on", "ulimit -c unlimited", true));
    }
    let rows = compiled_rows(&live, &parent, "parent", test);
    for kid in &kids {
        for (leg, limit, dumps) in &legs {
            let first = if *dumps {
                "signal 11 core"
            } else {
                "signal 11"
            };
            let second = if *leg == "off" && piped {
                "signal 11 core"
            } else {
                "signal 11"
            };
            let want = format!("segv: {first}\nsegv-limit0: {second}\nsegv-off: signal 11\n");
            let cores_here = usize::from(*dumps && !piped);
            for (row, prog) in &rows {
                let sub = format!(
                    "{leg}-{}-{}",
                    kid.rsplit('/').next().unwrap(),
                    row.replace(' ', "_")
                );
                live.conf(&sub, &[("child", kid.as_str())]);
                let out = live.sh(&format!(
                    "{limit}; cd {} && {prog}; echo rc=$?; ls | grep -c '^core' ; rm -f core*",
                    live.at(&sub)
                ));
                let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
                report(&format!(
                    "REPORT core_parent.lu {row} of {kid} (cores {leg}): {} || {}",
                    stdout.replace('\n', " | "),
                    String::from_utf8_lossy(&out.stderr).replace('\n', " | ")
                ));
                assert_eq!(
                    stdout,
                    format!("{want}rc=0\n{cores_here}\n"),
                    "#811: the {row} parent of {kid}, cores {leg}"
                );
            }
            if *leg == "off" {
                let sub = format!("{leg}-{}-checked", kid.rsplit('/').next().unwrap());
                let (checked, _) = record_in(
                    &live,
                    &live.wolf_at(),
                    "--checked",
                    "core_parent.lu",
                    &sub,
                    limit,
                    &[("child", kid.as_str())],
                );
                assert_eq!(
                    (checked.verdict.as_str(), checked.stdout.as_str()),
                    ("exit(0)", want.as_str()),
                    "#811: the CHECKED machine as the parent of {kid}, cores {leg}"
                );
                if let Some(l) = live.lupin_at() {
                    let sub = format!("{leg}-{}-lupin", kid.rsplit('/').next().unwrap());
                    let (obs, _) = record_in(
                        &live,
                        &l,
                        "",
                        "core_parent.lu",
                        &sub,
                        limit,
                        &[("child", kid.as_str())],
                    );
                    eprintln!(
                        "== lupin {} as the parent: {} {}",
                        obs.version, obs.verdict, obs.reason
                    );
                    assert_eq!(
                        (obs.verdict.as_str(), obs.stdout.as_str()),
                        ("exit(0)", want.as_str()),
                        "lupin {} as the parent of {kid} (it serves the _core wait since \
                         0.1.55): {obs:?}",
                        obs.version
                    );
                }
            }
        }
    }
}

/// #812: whose handler a wolf program holds on 33, and what the C
/// library's numbers do. `reserved_view.lu` (native, release) catches none
/// of 32..34 when it starts and 33 after its first task — glibc's own,
/// installed with the process's second thread (nptl(7); black-box C on
/// glibc 2.39 and 2.44 the same, musl none: s253's M1). Then
/// `reserved_parent.lu`, on every machine, starts `reserved_child.lu`'s
/// binary: `os_exit_signal(-32)` and `(-33)`, with and without a task,
/// are refused by name (`invalid`: the child exits 3); a 33 sent to a
/// `wait` child ends it at the default and not where the child holds 33
/// caught (glibc's handler, after a task) or ignored (glibc's
/// `posix_spawn` starts a child with 32 and 33 ignored where the kernel
/// refuses `clone3`: CI's docker runner; at the default under kasumi's
/// podman — s253's M3), each row held to the masks the child wrote at the
/// send. linux only, in a container (macOS has no real-time signals). Red
/// at 0.2.34: the four exits were 160 and 161.
#[cfg(target_os = "linux")]
#[test]
fn the_c_librarys_handler_and_numbers() {
    let test = "reserved";
    let Some(live) = Live::new(test) else {
        return;
    };
    let glibc = cfg!(target_env = "gnu");
    for (row, prog) in compiled_rows(&live, &fixture("reserved_view.lu"), "view", test) {
        let out = live.sh(&prog);
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        report(&format!(
            "REPORT reserved_view.lu {row}: {} || {}",
            stdout.replace('\n', " | "),
            String::from_utf8_lossy(&out.stderr).replace('\n', " | ")
        ));
        if glibc {
            assert_eq!(
                stdout, "start: none\nafter a task: 33\n",
                "#812: the {row} program holds no handler on 32..34 until its second thread, \
                 then glibc's on 33"
            );
        }
    }
    let exits = "exit32: exit 3\nexit32-task: exit 3\nexit33: exit 3\nexit33-task: exit 3\n";
    // The two `wait` rows by the masks the child had at the send, and
    // glibc's handler on 33 after the task.
    let want_of = |stderr: &str| -> String {
        if glibc {
            let cgt = child_mask(stderr, "wait-task", "SigCgt").expect("wait-task's SigCgt");
            assert_ne!(
                cgt & (1u64 << 32),
                0,
                "#812: after a task the child catches 33 (glibc's handler): {stderr}"
            );
        }
        let (a, b) = (
            end_by_masks(stderr, "wait"),
            end_by_masks(stderr, "wait-task"),
        );
        report(&format!(
            "REPORT the wait children's ends by their masks: wait {a} (SigIgn {:x?}), wait-task {b}",
            child_mask(stderr, "wait", "SigIgn")
        ));
        format!("{exits}wait, sent 33: {a}\nwait-task, sent 33: {b}\n")
    };
    for release in [false, true] {
        let Some(c) = build(&fixture("reserved_child.lu"), &live.dir, "child", release) else {
            continue;
        };
        let kid = live.at(&c.file_name().unwrap().to_string_lossy());
        for (row, prog) in compiled_rows(&live, &fixture("reserved_parent.lu"), "parent", test) {
            let sub = format!(
                "{}-{}",
                kid.rsplit('/').next().unwrap(),
                row.replace(' ', "_")
            );
            live.conf(&sub, &[("child", kid.as_str())]);
            let out = live.sh(&format!(
                "ulimit -c 0; cd {} && {prog}; echo rc=$?",
                live.at(&sub)
            ));
            let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
            let stderr = String::from_utf8_lossy(&out.stderr);
            report(&format!(
                "REPORT reserved_parent.lu {row} of {kid}: {} || {}",
                stdout.replace('\n', " | "),
                stderr.replace('\n', " | ")
            ));
            if glibc {
                let want = want_of(&stderr);
                assert_eq!(
                    stdout,
                    format!("{want}rc=0\n"),
                    "#812: the {row} parent of {kid}"
                );
                assert_eq!(
                    stderr.matches("exit_signal: invalid").count(),
                    4,
                    "#812: each refusal names its row: {stderr}"
                );
            }
        }
        let sub = format!("{}-checked", kid.rsplit('/').next().unwrap());
        let (checked, checked_err) = record_in(
            &live,
            &live.wolf_at(),
            "--checked",
            "reserved_parent.lu",
            &sub,
            "ulimit -c 0",
            &[("child", kid.as_str())],
        );
        if glibc {
            let want = want_of(&checked_err);
            assert_eq!(
                (checked.verdict.as_str(), checked.stdout.as_str()),
                ("exit(0)", want.as_str()),
                "#812: the CHECKED machine as the parent of {kid}"
            );
        }
        if let Some(l) = live.lupin_at() {
            let sub = format!("{}-lupin", kid.rsplit('/').next().unwrap());
            let (obs, _) = record_in(
                &live,
                &l,
                "",
                "reserved_parent.lu",
                &sub,
                "ulimit -c 0",
                &[("child", kid.as_str())],
            );
            eprintln!(
                "== lupin {} as the parent: {} {}\n{}",
                obs.version, obs.verdict, obs.reason, obs.stdout
            );
            // lupin answers the spawns and waits and refuses a well-formed
            // send by name (ruling #79 R8), after the four exits' lines.
            assert_eq!(
                obs.verdict, "unsupported",
                "lupin {} as the parent refuses the send by name: {obs:?}",
                obs.version
            );
            assert!(
                obs.reason.contains("`os_signal_send`"),
                "lupin {}'s refusal names the send: {}",
                obs.version,
                obs.reason
            );
        }
    }
}
