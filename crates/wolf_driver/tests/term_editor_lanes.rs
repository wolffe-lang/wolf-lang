//! s234 (wolf-lang#719, W05; pelt's sh11 line editor) — what a line
//! editor asks of the terminal, driven through a pseudo-terminal on every
//! machine. `[os.term.size]`, `[os.term.chars]`, `[os.term.mode]`'s
//! `FLOW` and `EXTENDED`, `[os.signal.wait.fd]`, and s232's `RESIZE`.
//!
//! Each fixture under `fixtures/term_editor/` runs as the session leader
//! of a fresh pseudo-terminal that is its controlling terminal, sized
//! 24x80 as an emulator would size it, every signal it could meet at its
//! default and an empty mask (the harness's own state decides nothing).
//! The harness waits for a marker on the terminal and then acts as the
//! terminal's other side: it resizes the window (the kernel sends
//! SIGWINCH to the foreground group), types, moves a special character
//! or turns IXON off through the master (one settings object for the
//! pair), or sends a signal with `kill` — never by typing, so a wait that
//! hears it heard a signal nothing typed. The program's SigBlk and SigIgn
//! at `ready` are printed for every row (linux `/proc`; macOS `ps`).
//!
//! Red at trunk `549dea59`: every fixture is E0301 (none of the three
//! names exists), a compile error the gate fails on. The pelt witnesses
//! these come from (`notes/sh06-evidence/witness/`,
//! `notes/sh11-evidence/witness/`) were run at trunk and at the head for
//! the PR's evidence.
//!
//! Machines: the checked machine under `conform-run --checked`, the
//! native and release binaries — plain, and on linux again under
//! `taskset -c 0-3` (wolf-lang#570) — and lupin under `conform-run`. The
//! programs write to `/dev/tty`, never to standard output (the checked
//! machine and lupin buffer standard output into their record). lupin
//! forbids `unsafe` and links no libc: it refuses each s234 call by name
//! (`[os.term.editor.note]` R6), and each fixture makes an s234 call
//! before it opens the terminal, so the refusal names it. lupin 0.1.51
//! and 0.1.52 (the 0.2.28 and 0.2.29 pairings) predated the mirror and
//! answered "does not resolve" for the same names, pinned by version as
//! s180 designed; 0.1.53 (the 0.2.30 pairing, r35) carries it, measured
//! on the published archive, so the pin is dropped.

#![cfg(unix)]

mod lane_exit;

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
        .join("tests/fixtures/term_editor")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("term_editor_lanes")
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
/// their records report. Empty since the 0.2.30 pairing: 0.1.53 (r35)
/// carries wolf-interp's s234, measured; the 0.1.51 release (`f72e78d`),
/// wolf-interp `71b8d97` and the 0.1.52 release (`025428b`) did not
/// ("`os_signal_wait_fd` does not resolve").
const PRE_MIRROR_LUPIN: &[(&str, &str)] = &[];

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

/// `taskset` on linux, from PATH (NixOS keeps it outside `/usr/bin`).
fn taskset() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("taskset"))
        .find(|p| p.is_file())
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
    /// Send this signal to the program with `kill`.
    Kill(libc::c_int),
    /// Through the master: the interrupt character becomes Ctrl-X,
    /// literal-next is disabled, IXON goes off.
    Restyle,
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
            Act::Kill(sig) => {
                let pid = libc::pid_t::try_from(self.child.id()).expect("pid");
                // SAFETY: a signal to the child this test started.
                assert_eq!(unsafe { libc::kill(pid, *sig) }, 0, "kill");
            }
            Act::Restyle => {
                #[cfg(any(target_os = "linux", target_os = "android"))]
                const DISABLED: libc::cc_t = 0;
                #[cfg(not(any(target_os = "linux", target_os = "android")))]
                const DISABLED: libc::cc_t = 0xFF;
                // SAFETY: tcgetattr/tcsetattr on the master, which sets
                // the pair's settings.
                unsafe {
                    let fd = self.master.as_raw_fd();
                    let mut t: libc::termios = std::mem::zeroed();
                    assert_eq!(libc::tcgetattr(fd, &mut t), 0, "tcgetattr");
                    t.c_cc[libc::VINTR] = 0x18;
                    t.c_cc[libc::VLNEXT] = DISABLED;
                    t.c_iflag &= !libc::IXON;
                    assert_eq!(libc::tcsetattr(fd, libc::TCSANOW, &t), 0, "tcsetattr");
                }
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
        if let Some(t) = taskset() {
            out.push(Lane {
                name: format!("{tier} taskset -c 0-3"),
                argv: vec![
                    t.to_string_lossy().into_owned(),
                    "-c".into(),
                    "0-3".into(),
                    exe,
                ],
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
                 the oracle leg of s234's gate did not run"
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
            eprintln!("s234 {name} [lupin {version} {commit}] end={end:?} verdict={v} why={why:?}");
            assert_eq!(v, "unsupported", "lupin on {name}: {text}");
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
        eprintln!("s234 {name} [{}] ready={ready} {}", lane.name, t.sig);
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
            "s234 {name} [{}] end={end:?} {sig} transcript={text:?}",
            lane.name
        );
        check(&lane.name, &end, &text);
        eprintln!("PASS s234 {name} [{}]", lane.name);
    }
}

/// Items 1 and 2: the size the terminal was given, and after a resize
/// the wait answers `RESIZE` (16384) and the size is the new one.
#[test]
fn the_size_and_a_resize_while_waiting() {
    every_lane(
        "size.lu",
        &[("ready size", Act::Resize(30, 100))],
        "os_term_size",
        &|lane, end, text| {
            assert!(text.contains("ready size 24 80"), "size on {lane}: {text}");
            assert!(
                text.contains("resized 16384 30 100"),
                "size on {lane}: {text}"
            );
            assert_eq!(*end, End::Exit(0), "size on {lane}: {text}");
        },
    );
}

/// Item 3: one wait ends for a key (answer 0, the key still there to
/// read) and for a SIGINT sent with `kill` (answer 16), nothing typed.
#[test]
fn the_wait_ends_for_a_key_and_for_a_signal() {
    every_lane(
        "wait_key.lu",
        &[
            ("ready", Act::Type(b"k")),
            ("key 0 107", Act::Kill(libc::SIGINT)),
        ],
        "os_signal_wait_fd",
        &|lane, end, text| {
            assert!(text.contains("ready early=-1"), "wait on {lane}: {text}");
            assert!(text.contains("key 0 107"), "wait on {lane}: {text}");
            assert!(text.contains("signal 16"), "wait on {lane}: {text}");
            assert_eq!(*end, End::Exit(0), "wait on {lane}: {text}");
        },
    );
}

/// Item 4: the special characters the other side set, FLOW following
/// IXON, and with every flag off Ctrl-S, Ctrl-V, Ctrl-O and Ctrl-Q all
/// arrive as bytes.
#[test]
fn the_characters_and_flow_and_extended() {
    every_lane(
        "chars.lu",
        &[
            ("ready", Act::Restyle),
            ("ready", Act::Type(b"g")),
            ("raw", Act::Type(b"\x13\x16\x0f\x11.")),
        ],
        "os_term_chars",
        &|lane, end, text| {
            assert!(
                text.contains("ready intr=3 flow=8 ext=16"),
                "chars on {lane}: {text}"
            );
            assert!(
                text.contains("after intr=24 lnext=-1 flow=0"),
                "chars on {lane}: {text}"
            );
            assert!(
                text.contains("bytes 19 22 15 17"),
                "chars on {lane}: {text}"
            );
            assert_eq!(*end, End::Exit(0), "chars on {lane}: {text}");
        },
    );
}

/// The corpus row that needs no terminal: every wolfgang lane under
/// `conform-run` prints it; lupin prints it too at the mirror (every row
/// is a shape row it reads) or, pre-mirror, answers `unsupported`.
#[test]
fn the_terminal_rows_on_every_machine() {
    let rel = "os/term_editor_rows.lu";
    let want = "wait_empty=io\nwait_send_only=io\nwait_past_table=io\nmode_bit5=invalid\n\
mode_negative=invalid\nmode_past_vtime=invalid\n";
    let entry = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(entry.is_file(), "corpus row missing: {}", entry.display());
    let dir = scratch("rows");
    let record = |cmd: &mut Command| -> (String, String, String, String) {
        let out = cmd
            .current_dir(&dir)
            .stdin(Stdio::null())
            .output()
            .expect("conform-run");
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let v: serde_json::Value = text
            .lines()
            .rev()
            .find_map(|l| {
                l.find('{')
                    .and_then(|i| serde_json::from_str(l[i..].trim()).ok())
            })
            .unwrap_or_else(|| panic!("a record on {rel}: {text}"));
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        (
            s("verdict"),
            s("stdout_inline"),
            s("impl_version"),
            s("commit"),
        )
    };
    for flag in ["--checked", "--native", "--release"] {
        let (verdict, stdout, ..) = record(
            Command::new(wolf())
                .arg("conform-run")
                .arg(&entry)
                .arg(flag)
                .arg("--json"),
        );
        assert_eq!(
            (verdict.as_str(), stdout.as_str()),
            ("exit(0)", want),
            "the {flag} lane on {rel} (s234)"
        );
        eprintln!("PASS the {flag} lane on {rel}");
    }
    match sibling_lupin() {
        Some(l) => {
            let (verdict, stdout, version, commit) =
                record(Command::new(l).arg("conform-run").arg(&entry).arg("--json"));
            if PRE_MIRROR_LUPIN
                .iter()
                .any(|(v, c)| *v == version && commit.starts_with(c))
            {
                assert_eq!(
                    verdict, "unsupported",
                    "pre-mirror lupin {version} on {rel}"
                );
                eprintln!("PINNED pre-mirror lupin {version} {commit} on {rel}: {verdict}");
            } else {
                assert_eq!(
                    (verdict.as_str(), stdout.as_str()),
                    ("exit(0)", want),
                    "lupin {version} at {commit} on {rel} (s234)"
                );
                eprintln!("PASS lupin {version} {commit} on {rel}");
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
