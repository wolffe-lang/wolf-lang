//! s219 (wolf-lang#622; pelt's H3) — Ctrl-C, process groups and the
//! terminal, driven through a pseudo-terminal on every machine.
//! `[os.signal.disp]`, `[os.signal.poll]`, `[os.proc.job]`,
//! `[os.proc.status]`, `[os.term]`, `[os.term.mode]`.
//!
//! Each fixture under `fixtures/ctrl_c/` runs as the session leader of a
//! fresh pseudo-terminal that is its controlling terminal — what a
//! terminal emulator gives a shell — started with every signal this lane
//! tests at its default disposition and an empty mask (a test harness
//! may itself ignore SIGINT: a `bash &` job does, and the ignore would be
//! inherited across exec and decide every row). The harness waits for
//! the program's `ready` line on the terminal, types what the row says
//! (Ctrl-C is byte 3, the terminal's interrupt character, so the KERNEL
//! sends SIGINT to the foreground group — nothing here calls `kill`),
//! and judges the transcript and how the process ended. The program's
//! SigBlk and SigIgn at `ready` are printed for every row (linux reads
//! `/proc`; macOS `ps` has the mask and no ignored set), so no answer
//! can have been decided by an inherited mask or ignore.
//!
//! Machines: the checked machine under `conform-run --checked` (the
//! conform-run process IS the machine, so it is what the terminal
//! signals), the native and release binaries `wolf build` made — plain,
//! and on linux again under `taskset -c 0-3` (wolf-lang#570) — and
//! lupin under `conform-run`. The programs write to `/dev/tty`, never
//! to standard output, because the checked machine and lupin buffer
//! standard output into their record.
//!
//! `control.lu` uses only s38/s90 calls and dies of Ctrl-C on every
//! machine: the harness delivers. lupin (0.1.48, the pairing, and the
//! mirror alike) refuses the other four by name — it forbids `unsafe`
//! and links no libc, so it has no `sigaction`, `tcsetpgrp` or
//! `tcsetattr` (`[os.term.note]` R8).

#![cfg(unix)]

mod lane_exit;

use std::io::{Read as _, Write as _};
use std::os::fd::FromRawFd as _;
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
        .join("tests/fixtures/ctrl_c")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("ctrl_c_lanes")
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
const RESET: [libc::c_int; 9] = [
    libc::SIGINT,
    libc::SIGQUIT,
    libc::SIGTSTP,
    libc::SIGTTIN,
    libc::SIGTTOU,
    libc::SIGCHLD,
    libc::SIGHUP,
    libc::SIGTERM,
    libc::SIGPIPE,
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
        if cfg!(target_os = "linux") && Path::new("/usr/bin/taskset").exists() {
            out.push(Lane {
                name: format!("{tier} taskset -c 0-3"),
                argv: vec!["/usr/bin/taskset".into(), "-c".into(), "0-3".into(), exe],
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
                 the oracle leg of s219's gate did not run"
            );
            eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        }
    }
    (dir, out)
}

/// lupin's verdict and its by-name reason, from the record it printed on
/// the terminal.
fn lupin_verdict(text: &str) -> (String, String) {
    text.lines()
        .rev()
        .find_map(|l| {
            l.find('{').and_then(|i| {
                serde_json::from_str::<serde_json::Value>(l[i..].trim())
                    .ok()
                    .map(|v| {
                        (
                            v["verdict"].as_str().unwrap_or("").to_string(),
                            v["x-unsupported"].as_str().unwrap_or("").to_string(),
                        )
                    })
            })
        })
        .unwrap_or_default()
}

/// Run one row on every lane: wait for `ready`, record the signal state,
/// type each input (waiting for its marker first, when one is given),
/// then judge with `check(lane, end, transcript)`. lupin is judged by
/// `lupin_wants`: the verdict it must give and a word its by-name reason
/// must contain (lupin refuses every row here — see each test).
fn every_lane(
    name: &str,
    steps: &[(&str, &[u8])],
    lupin_wants: (&str, &str),
    check: &dyn Fn(&str, &End, &str),
) {
    let (dir, ls) = lanes(name);
    for lane in ls {
        let mut t = Tty::start(&lane.argv, &dir);
        if lane.lupin {
            let (end, text, _) = t.finish(60);
            let (v, why) = lupin_verdict(&text);
            eprintln!("s219 {name} [lupin] end={end:?} verdict={v} why={why:?}");
            assert_eq!(v, lupin_wants.0, "lupin on {name}: {text}");
            assert!(
                why.contains(lupin_wants.1),
                "lupin on {name} refused for another reason than `{}`: {why}",
                lupin_wants.1
            );
            continue;
        }
        let ready = t.wait_for("ready", 60);
        t.record_sig();
        eprintln!("s219 {name} [{}] ready={ready} {}", lane.name, t.sig);
        assert!(ready, "{name} on {}: never ready: {}", lane.name, t.text());
        for (marker, bytes) in steps {
            if !marker.is_empty() {
                assert!(
                    t.wait_for(marker, 30),
                    "{name} on {}: no {marker:?}: {}",
                    lane.name,
                    t.text()
                );
            }
            std::thread::sleep(Duration::from_millis(300));
            t.send(bytes);
        }
        let (end, text, sig) = t.finish(60);
        eprintln!(
            "s219 {name} [{}] end={end:?} {sig} transcript={text:?}",
            lane.name
        );
        check(&lane.name, &end, &text);
    }
}

/// The control: Ctrl-C kills a program that does nothing about SIGINT,
/// on every wolfgang machine — the harness delivers the signal. lupin
/// serves the fs tier only inside its working directory and declines
/// `/dev/tty` by name (`[os.fs.path.domain]`), so it never reaches the
/// read; its refusal is recorded, and every lupin row below names the
/// s219 call each program makes before it opens the terminal.
#[test]
fn control_dies_of_ctrl_c() {
    every_lane(
        "control.lu",
        &[("", b"\x03")],
        ("unsupported", "/dev/tty"),
        &|lane, end, text| {
            assert_eq!(*end, End::Signal(2), "control on {lane}: {text}");
            assert!(!text.contains("never"), "control on {lane}: {text}");
        },
    );
}

/// Witness 1: an ignored INTERRUPT — Ctrl-C, then a line; the program
/// survives and answers.
#[test]
fn an_ignored_interrupt_survives_ctrl_c() {
    every_lane(
        "ignore.lu",
        &[("", b"\x03"), ("", b"go\n")],
        ("unsupported", "os_signal_ignore"),
        &|lane, end, text| {
            assert_eq!(*end, End::Exit(0), "ignore on {lane}: {text}");
            assert!(text.contains("survived 3"), "ignore on {lane}: {text}");
        },
    );
}

/// Witness 2: a listened INTERRUPT is an event the program polls.
#[test]
fn a_listened_interrupt_is_an_event() {
    every_lane(
        "listen.lu",
        &[("", b"\x03")],
        ("unsupported", "os_signal_listen"),
        &|lane, end, text| {
            assert_eq!(*end, End::Exit(0), "listen on {lane}: {text}");
            assert!(text.contains("got 16"), "listen on {lane}: {text}");
        },
    );
}

/// Witnesses 3 and 4: a child in its own group, handed the terminal,
/// dies of Ctrl-C (-2) while the parent, which never touches SIGINT,
/// lives, and the terminal is the child's during and the parent's after.
#[test]
fn a_child_group_takes_ctrl_c_and_the_terminal_comes_back() {
    every_lane(
        "group.lu",
        &[("ready", b"\x03")],
        ("unsupported", "os_pgid"),
        &|lane, end, text| {
            assert!(
                text.contains("ready child-fg=true mine=true"),
                "group on {lane}: {text}"
            );
            assert!(
                text.contains("child=-2 back=true"),
                "group on {lane}: {text}"
            );
            assert_eq!(*end, End::Exit(0), "group on {lane}: {text}");
        },
    );
}

/// Witness 5: raw mode reads one key without Enter, and the mode reads
/// back as it was saved.
#[test]
fn raw_mode_reads_one_key_and_restores() {
    every_lane(
        "raw.lu",
        &[("ready raw=260", b"x")],
        ("unsupported", "os_term_mode"),
        &|lane, end, text| {
            assert!(text.contains("ready raw=260"), "raw on {lane}: {text}");
            assert!(
                text.contains("key=120 restored=true"),
                "raw on {lane}: {text}"
            );
            assert_eq!(*end, End::Exit(0), "raw on {lane}: {text}");
        },
    );
}
