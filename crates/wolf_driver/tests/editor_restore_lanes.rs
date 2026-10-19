//! s248 (wolf-lang#794; ruling #93 R1 C; yew `docs/MAP.md` §5 G1) — the
//! runtime restores the terminal on every exit it owns,
//! `[os.term.restore]`. yw00's witness (`notes/yw00-evidence/witness/
//! trap-leaves-raw.lu`: raw mode, then a bounds trap, and the shell's
//! `stty` after reads `-icanon -echo`) re-run here with
//! `os_term_restore_at_exit`, and the same under every other way out:
//! a return from `main`, `os_exit`, an error from `main`, the stack
//! guard's report, `os_exit_signal`; the two limits the clause states (no
//! registration, and a death by a signal nobody listened for); the last
//! registration winning; and a registration of the mode the terminal
//! already has.
//!
//! **Where.** Every row runs on a pseudo-terminal a test started, with a
//! scratch `HOME`: on linux `script(1)` inside a throwaway `--network none`
//! container (no init; the test's directory at `/w`, the fixtures and the
//! machines read-only), because rows fault and end the program by a
//! signal, which happen only to processes the test started and never on a
//! host's process table; on macOS on CI's throwaway runner only
//! (`GITHUB_ACTIONS`), on a pseudo-terminal this test opens. The engine is
//! s242's (`WOLF_JOBWAIT_ENGINE`, else docker then podman;
//! `WOLF_JOBWAIT_IMAGE`, default ubuntu:24.04), and
//! `WOLF_JOBWAIT_REQUIRE_CONTAINER=1` (CI's linux job) makes a missing
//! engine a failure, never a skip.
//!
//! **What is read.** The terminal's settings before the program and after
//! it (`stty -g`, the shell that leads the terminal's session reading them
//! around its child) — equal
//! is "restored" — and the transcript: the bytes registered (`LEFT`)
//! follow the program's own last line. Machines: the checked machine
//! under `conform-run --checked` (its mode from `s248.mode`), the native
//! and release binaries — plain, and on linux again under
//! `taskset -c 0-3` (wolf-lang#570).
//!
//! Red at s248's prediction commit: E0301 (the name does not exist); at
//! 0.2.33, yw00's witness and every row without the call leave the
//! terminal `-icanon -echo` (s248's §2, kasumi `~/lanes/s248/ev/w0/`).

#![cfg(unix)]

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/editor_restore")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("editor_restore_lanes")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(d.join("home")).expect("scratch dir");
    d
}

/// The native or release binary of `entry`. A compile error FAILS the
/// gate (it is the defect at a pre-fix head, never an environment).
fn build(entry: &Path, dir: &Path, release: bool) -> Option<PathBuf> {
    let exe = dir.join(if release { "r-release" } else { "r-native" });
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(entry)
        .arg("-o")
        .arg(&exe)
        .arg("--no-cache")
        .current_dir(dir);
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
            "SKIP: environment cannot build the {} tier: {}",
            if release { "release" } else { "native" },
            stderr.trim()
        );
        return None;
    }
    assert!(out.status.success(), "wolf build failed: {stderr}");
    Some(exe)
}

/// One row's expectation: the mode, whether the terminal is restored,
/// the bytes the registration writes (`None` for none), the compiled
/// tiers' exit status in the shell's terms, and the checked machine's
/// verdict (`None`: it is ended by the host, no record).
struct Row {
    mode: &'static str,
    restored: bool,
    left: Option<&'static str>,
    status: i32,
    checked: Option<&'static str>,
}

const ROWS: &[Row] = &[
    Row {
        mode: "none",
        restored: false,
        left: None,
        status: 0,
        checked: Some("exit(0)"),
    },
    Row {
        mode: "return",
        restored: true,
        left: Some("LEFT"),
        status: 0,
        checked: Some("exit(0)"),
    },
    Row {
        mode: "exit",
        restored: true,
        left: Some("LEFT"),
        status: 5,
        checked: Some("exit(5)"),
    },
    Row {
        mode: "main-err",
        restored: true,
        left: Some("LEFT"),
        status: 1,
        checked: Some("exit(1)"),
    },
    Row {
        mode: "trap",
        restored: true,
        left: Some("LEFT"),
        status: 134,
        checked: Some("trap(bounds)"),
    },
    // The checked machine has no stack guard: the recursion reaches its
    // call-depth budget, an `unsupported` end, which it restores on too.
    Row {
        mode: "guard",
        restored: true,
        left: Some("LEFT"),
        status: 134,
        checked: Some("unsupported"),
    },
    // The checked machine refuses `os_exit_signal` by name (it does not
    // end the `wolf` process by a signal): an `unsupported` end, restored.
    Row {
        mode: "exit-signal",
        restored: true,
        left: Some("LEFT"),
        status: 143,
        checked: Some("unsupported"),
    },
    // The clause's limit: a death by a signal nobody listened for runs no
    // runtime code. (The checked machine is the `wolf` process: it dies.)
    Row {
        mode: "killed",
        restored: false,
        left: None,
        status: 143,
        checked: None,
    },
    Row {
        mode: "last-wins",
        restored: true,
        left: Some("LEFT-B"),
        status: 0,
        checked: Some("exit(0)"),
    },
    // The registration of the mode the terminal already has: allowed;
    // the terminal ends raw, as registered, and the bytes are written.
    Row {
        mode: "same-mode",
        restored: false,
        left: Some("LEFT"),
        status: 0,
        checked: Some("exit(0)"),
    },
];

/// Where the rows run: a throwaway container (linux) or CI's throwaway
/// runner (macOS).
struct Live {
    engine: Option<String>,
    dir: PathBuf,
}

fn image() -> String {
    std::env::var("WOLF_JOBWAIT_IMAGE").unwrap_or_else(|_| "docker.io/library/ubuntu:24.04".into())
}

impl Live {
    fn new(test: &str) -> Option<Live> {
        let dir = scratch(test);
        if cfg!(target_os = "macos") {
            if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
                eprintln!(
                    "SKIP: {test}: not CI's throwaway runner — the rows fault and signal their \
                     own processes and run on macOS only there"
                );
                return None;
            }
            return Some(Live { engine: None, dir });
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
                .args(["sh", "-c", "command -v script stty taskset"])
                .stdin(Stdio::null())
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    return Some(Live {
                        engine: Some(e.clone()),
                        dir,
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
            "SKIP: {test}: no container engine ran {} ({why:?}) — the rows did not run",
            image()
        );
        None
    }

    /// The machine's command line as the row's shell sees it.
    fn at(&self, p: &Path) -> String {
        match self.engine {
            Some(_) => format!("/w/{}", p.file_name().unwrap().to_string_lossy()),
            None => p.display().to_string(),
        }
    }

    fn wolf_at(&self) -> String {
        match self.engine {
            Some(_) => "/machines/wolf".into(),
            None => wolf().into(),
        }
    }

    fn fixture_at(&self, name: &str) -> String {
        match self.engine {
            Some(_) => format!("/fx/{name}"),
            None => fixture(name).display().to_string(),
        }
    }

    /// Every row of `ROWS` for one machine, `cmd` the command with the
    /// mode appended (or written to `s248.mode` first for the checked
    /// machine): each row's settings before and after and its transcript.
    /// A shell is the session leader of the row's terminal and reads the
    /// settings with `stty -g` before the program and after it (the
    /// program is the shell's child: a terminal whose session leader has
    /// ended is revoked on macOS, so its settings cannot be read then).
    fn rows(&self, lane: &str, cmd: &str, by_file: bool) -> Vec<(String, String, String, String)> {
        let tag = lane.replace(' ', "_");
        let mut script = String::from("set -u\n");
        let mut inner = Vec::new();
        for r in ROWS {
            let n = format!("{tag}.{}", r.mode);
            let run = if by_file {
                format!("printf %s {} > s248.mode; {cmd}", r.mode)
            } else {
                format!("{cmd} {}", r.mode)
            };
            let row = format!("stty -g > {n}.before; {run}; echo rc=$?; stty -g > {n}.after");
            if self.engine.is_some() {
                script.push_str(&format!(
                    "script -qec '{row}' /dev/null > {n}.log 2>&1 < /dev/null\n"
                ));
            }
            inner.push(row);
        }
        match &self.engine {
            Some(e) => {
                let mut c = Command::new(e);
                c.args(["run", "--rm", "--network", "none"])
                    .arg("-e")
                    .arg("HOME=/w/home")
                    .arg("-v")
                    .arg(format!("{}:/w", self.dir.display()))
                    .arg("-v")
                    .arg(format!(
                        "{}:/fx:ro",
                        fixture("restore.lu").parent().unwrap().display()
                    ))
                    .arg("-v")
                    .arg(format!("{}:/machines/wolf:ro", wolf()))
                    .args(["-w", "/w"])
                    .arg(image())
                    .args(["sh", "-c", &script]);
                let out = bounded(c, &self.dir, 300).expect("the box finishes its rows");
                assert!(
                    out.status.success(),
                    "the box ran {lane}'s rows: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                ROWS.iter()
                    .map(|r| {
                        let n = format!("{tag}.{}", r.mode);
                        let read = |ext: &str| {
                            std::fs::read_to_string(self.dir.join(format!("{n}.{ext}")))
                                .unwrap_or_default()
                        };
                        (
                            r.mode.to_string(),
                            read("before"),
                            read("after"),
                            read("log").replace('\r', ""),
                        )
                    })
                    .collect()
            }
            None => ROWS
                .iter()
                .zip(inner)
                .map(|(r, row)| {
                    let n = format!("{tag}.{}", r.mode);
                    let text = pty_shell(&row, &self.dir);
                    let read = |ext: &str| {
                        std::fs::read_to_string(self.dir.join(format!("{n}.{ext}")))
                            .unwrap_or_default()
                    };
                    (r.mode.to_string(), read("before"), read("after"), text)
                })
                .collect(),
        }
    }
}

/// A command's output, or `None` past `secs` (killed: the child the test
/// started).
fn bounded(mut cmd: Command, dir: &Path, secs: u64) -> Option<Output> {
    let mut child = cmd
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    let end = Instant::now() + Duration::from_secs(secs);
    loop {
        if child.try_wait().expect("try_wait").is_some() {
            return Some(child.wait_with_output().expect("output"));
        }
        if Instant::now() > end {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// macOS (CI's runner): one row's shell line on a fresh pseudo-terminal
/// this test opens — `/bin/sh` the session leader, the terminal its
/// controlling one, a scratch `HOME` — and its transcript, CRs dropped.
fn pty_shell(line: &str, dir: &Path) -> String {
    use std::io::Read as _;
    use std::os::fd::FromRawFd as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    use std::os::unix::process::CommandExt as _;
    // SAFETY: the pty calls on descriptors this test owns; the child's
    // hook calls only async-signal-safe functions.
    unsafe {
        let m = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
        assert!(m >= 0, "posix_openpt");
        assert_eq!(libc::grantpt(m), 0);
        assert_eq!(libc::unlockpt(m), 0);
        let name = std::ffi::CStr::from_ptr(libc::ptsname(m))
            .to_string_lossy()
            .into_owned();
        let master = std::fs::File::from_raw_fd(m);
        let slave = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(&name)
            .expect("the slave");
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", line])
            .current_dir(dir)
            .env("HOME", dir.join("home"))
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave.try_clone().unwrap()));
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            for s in [libc::SIGTERM, libc::SIGSEGV, libc::SIGBUS, libc::SIGTTOU] {
                let mut sa: libc::sigaction = std::mem::zeroed();
                sa.sa_sigaction = libc::SIG_DFL;
                libc::sigaction(s, &sa, std::ptr::null_mut());
            }
            let mut none: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut none);
            libc::pthread_sigmask(libc::SIG_SETMASK, &none, std::ptr::null_mut());
            Ok(())
        });
        let mut child = cmd.spawn().expect("spawn under the pty");
        drop(slave);
        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = buf.clone();
        let mut rd = master.try_clone().expect("dup master");
        std::thread::spawn(move || {
            let mut b = [0u8; 4096];
            while let Ok(n) = rd.read(&mut b) {
                if n == 0 {
                    break;
                }
                sink.lock().unwrap().extend_from_slice(&b[..n]);
            }
        });
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            if child.try_wait().expect("try_wait").is_some() {
                break;
            }
            if Instant::now() > end {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(200));
        let text = String::from_utf8_lossy(&buf.lock().unwrap()).replace('\r', "");
        drop(master);
        text
    }
}

/// The checked machine's record in a transcript: its verdict and the
/// standard output it buffered (`None` for a null one).
fn record_in(text: &str) -> Option<(String, Option<String>)> {
    text.lines().rev().find_map(|l| {
        let i = l.find('{')?;
        let v: serde_json::Value = serde_json::from_str(l[i..].trim()).ok()?;
        Some((
            v["verdict"].as_str()?.to_string(),
            v["stdout_inline"].as_str().map(str::to_string),
        ))
    })
}

/// #794: every exit the runtime owns restores the registered mode and
/// writes the registered bytes after the program's last line; the
/// clause's two limits restore nothing; the last registration wins; the
/// mode the terminal already has may be registered.
#[test]
fn the_terminal_is_restored_on_every_exit_the_runtime_owns() {
    let Some(live) = Live::new("restore") else {
        return;
    };
    let entry = fixture("restore.lu");
    let mut lanes: Vec<(String, String, bool)> = vec![(
        "checked".into(),
        format!(
            "{} conform-run {} --checked --json",
            live.wolf_at(),
            live.fixture_at("restore.lu")
        ),
        true,
    )];
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, &live.dir, release) else {
            continue;
        };
        lanes.push((tier.into(), live.at(&exe), false));
        if live.engine.is_some() {
            lanes.push((
                format!("{tier} taskset -c 0-3"),
                format!("taskset -c 0-3 {}", live.at(&exe)),
                false,
            ));
        }
    }
    let mut misses = Vec::new();
    for (lane, cmd, by_file) in &lanes {
        let got = live.rows(lane, cmd, *by_file);
        for (r, (mode, before, after, text)) in ROWS.iter().zip(got) {
            let restored = !before.is_empty() && before == after;
            let raw_line = format!("raw {mode}\n");
            // The checked machine buffers the program's standard output into
            // its record, which it prints after its end (and its restore):
            // there the program's line is in the record, and the bytes come
            // before it. A row it ends `unsupported` (or the host ends)
            // carries no output to show.
            let record = record_in(&text);
            let shown = if lane == "checked" {
                match (r.checked, record.as_ref()) {
                    (Some("unsupported") | None, _) => Some(0),
                    (_, Some((_, Some(out)))) if out.contains(&raw_line) => Some(0),
                    _ => None,
                }
            } else {
                text.find(&raw_line)
            };
            let left_at = r.left.and_then(|l| {
                text.find(&format!("{l}\n"))
                    .or_else(|| text.find(&format!("{l}\x1b")))
            });
            let rc = text
                .lines()
                .rev()
                .find_map(|l| l.strip_prefix("rc="))
                .and_then(|v| v.trim().parse::<i32>().ok());
            eprintln!(
                "s248 restore [{lane}] {mode}: restored={restored} rc={rc:?} left_at={left_at:?} \
                 before={before:?} after={after:?}\n{text}"
            );
            let mut miss = |what: String| misses.push(format!("[{lane}] {mode}: {what}"));
            if shown.is_none() {
                miss(format!("the program never reached raw mode: {text:?}"));
                continue;
            }
            if restored != r.restored {
                miss(format!(
                    "restored={restored}, wanted {} (before {before:?}, after {after:?})",
                    r.restored
                ));
            }
            match (r.left, left_at, shown) {
                (Some(_), Some(at), Some(raw)) if at > raw || lane == "checked" => {}
                (Some(l), _, _) => miss(format!("no {l:?} after `raw {mode}`: {text:?}")),
                (None, _, _) => {
                    if text.contains("LEFT") {
                        miss(format!("bytes written though nothing restores: {text:?}"));
                    }
                }
            }
            if mode == "last-wins" && text.contains("LEFT-A") {
                miss("the first registration's bytes were written".into());
            }
            if lane == "checked" {
                let v = record.map(|(v, _)| v);
                if v.as_deref() != r.checked {
                    miss(format!("verdict {v:?}, wanted {:?}", r.checked));
                }
            } else if rc != Some(r.status) {
                miss(format!("status {rc:?}, wanted {}", r.status));
            }
        }
        eprintln!("DONE s248 restore [{lane}]");
    }
    assert!(
        misses.is_empty(),
        "restore rows missed:\n{}",
        misses.join("\n")
    );
}
