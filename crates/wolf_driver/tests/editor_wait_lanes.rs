//! s248 (wolf-lang#795; ruling #93 R2 A; yew `docs/MAP.md` §5 G2) — one
//! wait over several descriptors and a signal set with a deadline,
//! `[os.signal.wait.fds]`, driven through a pseudo-terminal on every
//! machine: the terminal, a pipe, `RESIZE` and a 25 ms deadline, each
//! winner seen; the race its registration closes; and its shape rows.
//!
//! The harness is `term_editor_lanes.rs`'s (s234): each fixture under
//! `fixtures/editor_wait/` runs as the session leader of a fresh
//! pseudo-terminal that is its controlling terminal, 24x80, every signal
//! it could meet at its default and an empty mask; the harness waits for a
//! marker and then types or resizes the window through the master (the
//! kernel sends SIGWINCH to the terminal's foreground group: the program,
//! the session's only process). Nothing here signals a process the test
//! did not start. The program's SigBlk and SigIgn at `ready` are printed
//! for every row (linux `/proc`; macOS `ps`).
//!
//! Red at s248's prediction commit: every fixture is E0301 (the name does
//! not exist), a compile error the gate fails on.
//!
//! Machines: the checked machine under `conform-run --checked`, the
//! native and release binaries — plain, and on linux again under
//! `taskset` on four CPUs of the allocation (wolf-lang#570) — and lupin under `conform-run`. The
//! programs write to `/dev/tty`, never to standard output (the checked
//! machine and lupin buffer standard output into their record), but for
//! the race and the rows, which run without a terminal.

#![cfg(unix)]

mod lane_exit;
mod test_cpus;

use std::io::{Read as _, Write as _};
use std::os::fd::{AsRawFd as _, FromRawFd as _};
use std::os::unix::fs::OpenOptionsExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/editor_wait")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("editor_wait_lanes")
        .join(test);
    std::fs::create_dir_all(d.join("target")).expect("scratch dir");
    d
}

/// The sibling lupin, found exactly as `pairing.rs` finds it.
fn sibling_lupin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LUPIN") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while dir.pop() {
        let candidate = dir.join("../wolf-interp/target/release/lupin");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// The lupin builds that predate the mirror, by version and the commit
/// their records report: 0.1.54, the 0.2.33 pairing (`0e536e8`), and
/// 0.1.55, the 0.2.34 pairing (`e642f26`, r39), do not resolve
/// `os_signal_wait_fds` (measured, s248's §2 and r39; wolf-interp#268).
const PRE_MIRROR_LUPIN: &[(&str, &str)] = &[("0.1.54", "0e536e8"), ("0.1.55", "e642f26")];

/// Of those, the versions that resolve `os_signal_listen` and refuse it by
/// name (is77, wolf-interp#259): on a fixture that listens before its first
/// s248 name they stop at the listen, refused by name, not "does not
/// resolve" (0.1.55 on `wait_fds_race.lu`, measured, r39).
const LISTEN_REFUSED_BY_NAME: &[&str] = &["0.1.55"];

/// r40/r41: published .56 and .57 resolve the wait; both wait fixtures
/// reach the listen, refused by name. No unlisted build inherits this pin.
fn listen_before_wait(version: &str, commit: &str) -> bool {
    [("0.1.56", "28547b4"), ("0.1.57", "fb054c8"), ("0.1.58", "6240efe")]
        .iter()
        .any(|(v, c)| *v == version && commit.starts_with(c))
}

#[test]
fn only_measured_listen_refusals_are_pinned() {
    assert!(listen_before_wait("0.1.56", "28547b4"));
    assert!(listen_before_wait("0.1.57", "fb054c8"));
    assert!(!listen_before_wait("0.1.57", "28547b4"));
    assert!(!listen_before_wait("0.1.57", "unknown"));
    assert!(!listen_before_wait("0.1.58", "fb054c8"));
    assert!(listen_before_wait("0.1.58", "6240efe"));
    assert!(!listen_before_wait("0.1.58", "unknown"));
    assert!(!listen_before_wait("0.1.59", "6240efe"));
}

/// The native or release binary of `entry`. A compile error FAILS the
/// gate (it is the defect at a pre-fix head, never an environment).
fn build(entry: &Path, dir: &Path, release: bool) -> Option<PathBuf> {
    let exe = dir.join(if release { "a-release" } else { "a-native" });
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

/// How a run under the terminal ended.
#[derive(Debug, PartialEq, Eq)]
enum End {
    Exit(i32),
    Signal(i32),
    Timeout,
}

/// One program on a fresh pseudo-terminal: its transcript, how it
/// ended, and its signal state at `ready`.
struct Tty {
    child: std::process::Child,
    master: std::fs::File,
    buf: Arc<Mutex<Vec<u8>>>,
    sig: String,
}

/// The signals a terminal leaves at their default when it starts a
/// program, and resets here before exec.
const RESET: [libc::c_int; 10] = [
    libc::SIGINT,
    libc::SIGQUIT,
    libc::SIGTSTP,
    libc::SIGTTIN,
    libc::SIGTTOU,
    libc::SIGCHLD,
    libc::SIGHUP,
    libc::SIGTERM,
    libc::SIGPIPE,
    libc::SIGWINCH,
];

impl Tty {
    fn start(argv: &[String], dir: &Path) -> Tty {
        // SAFETY: the pty calls on descriptors this test owns; the
        // child's hook calls only async-signal-safe functions.
        unsafe {
            let m = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
            assert!(m >= 0, "posix_openpt");
            assert_eq!(libc::grantpt(m), 0, "grantpt");
            assert_eq!(libc::unlockpt(m), 0, "unlockpt");
            let name = std::ffi::CStr::from_ptr(libc::ptsname(m))
                .to_string_lossy()
                .into_owned();
            let master = std::fs::File::from_raw_fd(m);
            let slave = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NOCTTY)
                .open(&name)
                .expect("open the pty's slave");
            // The size a terminal emulator would have told it — after the
            // slave is open: macOS refuses TIOCSWINSZ on a master whose
            // slave nobody has opened (ENOTTY, measured black-box).
            let w = libc::winsize {
                ws_row: 24,
                ws_col: 80,
                ws_xpixel: 0,
                ws_ypixel: 0,
            };
            assert_eq!(libc::ioctl(m, libc::TIOCSWINSZ, &w), 0, "TIOCSWINSZ");
            let mut cmd = Command::new(&argv[0]);
            cmd.args(&argv[1..])
                .current_dir(dir)
                .stdin(Stdio::from(slave.try_clone().expect("dup")))
                .stdout(Stdio::from(slave.try_clone().expect("dup")))
                .stderr(Stdio::from(slave));
            cmd.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                for s in RESET {
                    let mut sa: libc::sigaction = std::mem::zeroed();
                    sa.sa_sigaction = libc::SIG_DFL;
                    libc::sigemptyset(&mut sa.sa_mask);
                    libc::sigaction(s, &sa, std::ptr::null_mut());
                }
                let mut none: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut none);
                libc::pthread_sigmask(libc::SIG_SETMASK, &none, std::ptr::null_mut());
                Ok(())
            });
            let child = cmd.spawn().expect("spawn under the pty");
            let buf = Arc::new(Mutex::new(Vec::new()));
            let sink = buf.clone();
            let mut rd = master.try_clone().expect("dup master");
            std::thread::spawn(move || {
                let mut b = [0u8; 4096];
                loop {
                    match rd.read(&mut b) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => sink.lock().unwrap().extend_from_slice(&b[..n]),
                    }
                }
            });
            Tty {
                child,
                master,
                buf,
                sig: String::new(),
            }
        }
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.buf.lock().unwrap()).replace('\r', "")
    }

    /// s248 (#802): the master's bytes as the wire carried them, CRs and
    /// all.
    fn wire(&self) -> Vec<u8> {
        self.buf.lock().unwrap().clone()
    }

    /// Wait until the transcript holds `want`; false at the deadline.
    fn wait_for(&self, want: &str, secs: u64) -> bool {
        let end = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < end {
            if self.text().contains(want) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// The process's signal mask and ignored set now (linux `/proc`;
    /// macOS `ps` has only the mask).
    fn record_sig(&mut self) {
        let pid = self.child.id();
        self.sig = if cfg!(target_os = "linux") {
            std::fs::read_to_string(format!("/proc/{pid}/status"))
                .unwrap_or_default()
                .lines()
                .filter(|l| l.starts_with("SigBlk") || l.starts_with("SigIgn"))
                .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            let o = Command::new("ps")
                .args(["-o", "sigmask=", "-p", &pid.to_string()])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            format!("SigBlk {o} SigIgn n/a(macOS ps has no ignored column)")
        };
    }

    fn send(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).expect("type at the terminal");
    }

    fn finish(mut self, secs: u64) -> (End, String, String) {
        use std::os::unix::process::ExitStatusExt as _;
        let end = Instant::now() + Duration::from_secs(secs);
        let how = loop {
            match self.child.try_wait().expect("try_wait") {
                Some(st) => {
                    break match (st.code(), st.signal()) {
                        (Some(c), _) => End::Exit(c),
                        (None, Some(s)) => End::Signal(s),
                        _ => End::Timeout,
                    };
                }
                None if Instant::now() > end => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break End::Timeout;
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        };
        std::thread::sleep(Duration::from_millis(100));
        (how, self.text(), self.sig.clone())
    }
}

/// What the harness does once a marker is on the terminal.
enum Act {
    /// Type these bytes.
    Type(&'static [u8]),
    /// Resize the window (rows, columns): the kernel signals SIGWINCH.
    Resize(u16, u16),
}

impl Tty {
    fn act(&mut self, a: &Act) {
        match a {
            Act::Type(b) => self.send(b),
            Act::Resize(rows, cols) => {
                let w = libc::winsize {
                    ws_row: *rows,
                    ws_col: *cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                // SAFETY: a winsize for the master's ioctl.
                let r = unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &w) };
                assert_eq!(r, 0, "TIOCSWINSZ");
            }
        }
    }
}

/// One machine's way of running a fixture.
struct Lane {
    name: String,
    argv: Vec<String>,
    lupin: bool,
}

/// Every lane for `name`: checked, native and release (plain, and under
/// taskset on linux), lupin when a sibling exists.
fn lanes(name: &str) -> (PathBuf, Vec<Lane>) {
    let entry = fixture(name);
    let dir = scratch(&name.replace('.', "_"));
    let mut out = vec![Lane {
        name: "checked".into(),
        argv: vec![
            wolf().into(),
            "conform-run".into(),
            entry.to_string_lossy().into_owned(),
            "--checked".into(),
            "--json".into(),
        ],
        lupin: false,
    }];
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, &dir, release) else {
            continue;
        };
        let exe = exe.to_string_lossy().into_owned();
        out.push(Lane {
            name: tier.into(),
            argv: vec![exe.clone()],
            lupin: false,
        });
        if let Some(pin) = test_cpus::pin(4, module_path!()) {
            let mut argv = pin.argv();
            argv.push(exe);
            out.push(Lane {
                name: format!("{tier} {}", pin.label()),
                argv,
                lupin: false,
            });
        }
    }
    match sibling_lupin() {
        Some(l) => out.push(Lane {
            name: "lupin".into(),
            argv: vec![
                l.to_string_lossy().into_owned(),
                "conform-run".into(),
                entry.to_string_lossy().into_owned(),
                "--json".into(),
            ],
            lupin: true,
        }),
        None => {
            assert!(
                std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
                "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found — \
                 the oracle leg of s248's gate did not run"
            );
            eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        }
    }
    (dir, out)
}

/// lupin's record on the terminal: verdict, by-name reason, version,
/// commit.
fn lupin_record(text: &str) -> (String, String, String, String) {
    text.lines()
        .rev()
        .find_map(|l| {
            l.find('{').and_then(|i| {
                serde_json::from_str::<serde_json::Value>(l[i..].trim())
                    .ok()
                    .map(|v| {
                        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
                        (
                            s("verdict"),
                            s("x-unsupported"),
                            s("impl_version"),
                            s("commit"),
                        )
                    })
            })
        })
        .unwrap_or_default()
}

/// Run one fixture on every lane: wait for `ready`, record the signal
/// state, do each act once its marker is on the terminal, then judge with
/// `check(lane, end, transcript)`. lupin must refuse, naming `first` —
/// by name at the mirror, "does not resolve" when it predates it (pinned
/// by version).
fn every_lane(name: &str, steps: &[(&str, Act)], first: &str, check: &dyn Fn(&str, &End, &str)) {
    let (dir, ls) = lanes(name);
    for lane in ls {
        let mut t = Tty::start(&lane.argv, &dir);
        if lane.lupin {
            let (end, text, _) = t.finish(60);
            let (v, why, version, commit) = lupin_record(&text);
            eprintln!("s248 {name} [lupin {version} {commit}] end={end:?} verdict={v} why={why:?}");
            assert_eq!(v, "unsupported", "lupin on {name}: {text}");
            let first = if first == "os_signal_wait_fds" && listen_before_wait(&version, &commit) {
                "os_signal_listen"
            } else {
                first
            };
            assert!(
                why.contains(&format!("`{first}`")),
                "lupin on {name} refused for another reason than `{first}`: {why}"
            );
            if why.contains("does not resolve") {
                assert!(
                    PRE_MIRROR_LUPIN
                        .iter()
                        .any(|(v, c)| *v == version && commit.starts_with(c)),
                    "lupin {version} at {commit} does not resolve `{first}`, and it is not a \
                     pinned pre-mirror build: {why}"
                );
                eprintln!("PINNED pre-mirror lupin {version} {commit}: {why}");
            } else {
                eprintln!("REFUSED BY NAME lupin {version} {commit}: {why}");
            }
            continue;
        }
        let ready = t.wait_for("ready", 60);
        t.record_sig();
        eprintln!("s248 {name} [{}] ready={ready} {}", lane.name, t.sig);
        assert!(ready, "{name} on {}: never ready: {}", lane.name, t.text());
        for (marker, act) in steps {
            if !marker.is_empty() {
                assert!(
                    t.wait_for(marker, 30),
                    "{name} on {}: no {marker:?}: {}",
                    lane.name,
                    t.text()
                );
            }
            std::thread::sleep(Duration::from_millis(300));
            t.act(act);
        }
        let (end, text, sig) = t.finish(60);
        eprintln!(
            "s248 {name} [{}] end={end:?} {sig} transcript={text:?}",
            lane.name
        );
        check(&lane.name, &end, &text);
        eprintln!("PASS s248 {name} [{}]", lane.name);
    }
}

/// #795 (`[os.signal.wait.fds]`): one wait over `[tty, pipe]`, `RESIZE`
/// and a deadline — the deadline (-1, 0) after 25 ms, the poll (-1, 0),
/// the pipe (1, 0), the meaning before the readable pipe and then the
/// pipe, a key typed at the terminal (0, 0) and a resize (-1, 16384).
#[test]
fn one_wait_hears_the_tty_a_pipe_a_signal_and_a_deadline() {
    every_lane(
        "wait_fds.lu",
        &[
            ("ready key", Act::Type(b"k")),
            ("ready resize", Act::Resize(30, 100)),
        ],
        "os_signal_wait_fds",
        &|lane, end, text| {
            for want in [
                "early -9 -9\n",
                "deadline -1 0 waited=true\n",
                "poll -1 0\n",
                "pipe 1 0\n",
                "first -1 16384 then 1 0\n",
                "key 0 0 107\n",
                "resized -1 16384\n",
                "done 1\n",
            ] {
                assert!(
                    text.contains(want),
                    "wait_fds on {lane}: no {want:?}: {text}"
                );
            }
            assert_eq!(*end, End::Exit(0), "wait_fds on {lane}: {text}");
        },
    );
}

/// What a program printed and how it ended, or `None` when it ran past
/// `secs` (killed — the child the test started — and that is the red).
fn bounded(mut cmd: Command, dir: &Path, secs: u64) -> Option<(Option<i32>, String, String)> {
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
            let out = child.wait_with_output().expect("output");
            return Some((
                out.status.code(),
                String::from_utf8_lossy(&out.stdout).replace('\r', ""),
                String::from_utf8_lossy(&out.stderr).into_owned(),
            ));
        }
        if Instant::now() > end {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A `conform-run` record's verdict, stdout, by-name reason, version and
/// commit.
fn record_of(stdout: &str) -> (String, String, String, String, String) {
    let v: serde_json::Value = stdout
        .lines()
        .rev()
        .find_map(|l| {
            l.find('{')
                .and_then(|i| serde_json::from_str(l[i..].trim()).ok())
        })
        .unwrap_or_else(|| panic!("a record: {stdout}"));
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    (
        s("verdict"),
        s("stdout_inline"),
        s("x-unsupported"),
        s("impl_version"),
        s("commit"),
    )
}

/// A fixture with no terminal on every machine, each bounded by `secs`:
/// the checked machine under `conform-run`, the binaries directly (and
/// under taskset on linux), lupin under `conform-run` — 0.1.54 and
/// 0.1.55 refuse, naming one of `first` (the first name each does not
/// resolve), pinned by version.
fn every_lane_plain(name: &str, secs: u64, want: &str, first: &[&str]) {
    let entry = fixture(name);
    let dir = scratch(&name.replace('.', "_"));
    let checked = bounded(
        {
            let mut c = Command::new(wolf());
            c.arg("conform-run")
                .arg(&entry)
                .args(["--checked", "--json"]);
            c
        },
        &dir,
        secs,
    )
    .unwrap_or_else(|| panic!("the checked machine on {name} ran past {secs} s (a lost wake?)"));
    let (verdict, stdout, ..) = record_of(&checked.1);
    eprintln!("s248 {name} [checked] {verdict} {stdout:?}");
    assert_eq!(
        (verdict.as_str(), stdout.as_str()),
        ("exit(0)", want),
        "checked on {name}"
    );
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, &dir, release) else {
            continue;
        };
        let mut runs = vec![(tier.to_string(), Command::new(&exe))];
        if let Some(pin) = test_cpus::pin(4, module_path!()) {
            runs.push((format!("{tier} {}", pin.label()), pin.command(&exe)));
        }
        for (lane, cmd) in runs {
            let (code, stdout, stderr) = bounded(cmd, &dir, secs).unwrap_or_else(|| {
                panic!("the {lane} binary on {name} ran past {secs} s (a lost wake?)")
            });
            eprintln!("s248 {name} [{lane}] exit {code:?} {stdout:?} {stderr}");
            assert_eq!((code, stdout.as_str()), (Some(0), want), "{lane} on {name}");
            eprintln!("PASS s248 {name} [{lane}]");
        }
    }
    match sibling_lupin() {
        Some(l) => {
            let mut c = Command::new(l);
            c.arg("conform-run").arg(&entry).arg("--json");
            let out = bounded(c, &dir, secs).expect("lupin answers");
            let (verdict, stdout, why, version, commit) = record_of(&out.1);
            eprintln!("s248 {name} [lupin {version} {commit}] {verdict} {why:?}");
            if PRE_MIRROR_LUPIN
                .iter()
                .any(|(v, c)| *v == version && commit.starts_with(c))
                || listen_before_wait(&version, &commit)
            {
                assert_eq!(verdict, "unsupported", "pre-mirror lupin on {name}");
                let measured = if LISTEN_REFUSED_BY_NAME.contains(&version.as_str())
                    || listen_before_wait(&version, &commit)
                {
                    first.contains(&"os_signal_listen")
                        && why.contains("`os_signal_listen`")
                        && !why.contains("does not resolve")
                } else {
                    first.iter().any(|f| why.contains(&format!("`{f}`")))
                        && why.contains("does not resolve")
                };
                assert!(measured, "pre-mirror lupin {version} on {name}: {why}");
                eprintln!("PINNED pre-mirror lupin {version} {commit} on {name}: {why}");
            } else {
                assert_eq!(
                    (verdict.as_str(), stdout.as_str()),
                    ("exit(0)", want),
                    "lupin {version} at {commit} on {name}"
                );
            }
        }
        None => {
            assert!(
                std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
                "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found"
            );
            eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        }
    }
}

/// #795: a meaning raised in the window between the wait's last look at
/// the queue and its park ends the park — 2000 rounds, each a sibling
/// task's raise as the waiter goes to wait, every one answered. A lost
/// wake parks the waiter for ever and the 120 s bound is the red.
#[test]
fn no_meaning_raised_beside_the_wait_is_lost() {
    every_lane_plain(
        "wait_fds_race.lu",
        120,
        "race rounds=2000 meanings=2000\n",
        // lupin 0.1.54 does not resolve the listen (s163's measurement);
        // 0.1.55 resolves it and refuses it by name (is77).
        &["os_signal_listen", "os_signal_wait_fds"],
    );
}

/// #802 (`[os.term.mode]`; yw01's W1 Enter witness): with `RAW_CR` a typed
/// CR reads CR (13), without it LF (10); with `STRIP8` a typed 0xC3 reads
/// 0x43 (67); the two bits read back set (32 + 128). Red at s248's
/// prediction commit and at 0.2.33: bit 5 and bit 7 are `invalid` there
/// (`[os.term.mode]` had no field for them), so the first set fails.
#[test]
fn raw_mode_reads_enter_as_cr_and_strips_on_request() {
    every_lane(
        "mode_enter.lu",
        &[
            ("ready enter", Act::Type(b"\r")),
            ("ready again", Act::Type(b"\r")),
            ("ready strip", Act::Type(b"\xc3")),
        ],
        "os_term_mode",
        &|lane, end, text| {
            for want in ["bits 160\n", "cr 13 then 10\n", "strip 67 early=-1\n"] {
                assert!(
                    text.contains(want),
                    "mode_enter on {lane}: no {want:?}: {text}"
                );
            }
            assert_eq!(*end, End::Exit(0), "mode_enter on {lane}: {text}");
        },
    );
}

/// #802 (`[os.term.mode]`; yw01's W1 output witness): with `RAW_OUTPUT` a
/// written LF reaches the wire alone, without it as CR LF; the bit reads
/// back set (64). Red at 0.2.33: bit 6 is `invalid` there.
#[test]
fn raw_output_writes_lf_alone() {
    let (dir, ls) = lanes("mode_opost.lu");
    for lane in ls {
        let t = Tty::start(&lane.argv, &dir);
        if lane.lupin {
            let (_, text, _) = t.finish(60);
            assert!(
                text.contains("`os_term_mode`") && text.contains("unsupported"),
                "lupin refuses mode_opost by name: {text}"
            );
            continue;
        }
        assert!(
            t.wait_for("bits", 60),
            "mode_opost on {}: {}",
            lane.name,
            t.text()
        );
        std::thread::sleep(Duration::from_millis(200));
        let wire = t.wire();
        let (end, text, _) = t.finish(60);
        let has = |b: &[u8]| wire.windows(b.len()).any(|w| w == b);
        eprintln!(
            "s248 mode_opost [{}] end={end:?} wire={:?}",
            lane.name,
            String::from_utf8_lossy(&wire)
        );
        assert!(
            has(b"[raw]\n"),
            "mode_opost on {}: no bare LF: {text}",
            lane.name
        );
        assert!(
            !has(b"[raw]\r\n"),
            "mode_opost on {}: OPOST still on: {text}",
            lane.name
        );
        assert!(
            has(b"[cooked]\r\n"),
            "mode_opost on {}: the mode put back: {text}",
            lane.name
        );
        assert!(
            text.contains("bits 64 early=-1\n"),
            "mode_opost on {}: {text}",
            lane.name
        );
        assert_eq!(end, End::Exit(0), "mode_opost on {}: {text}", lane.name);
        eprintln!("PASS s248 mode_opost [{}]", lane.name);
    }
}
