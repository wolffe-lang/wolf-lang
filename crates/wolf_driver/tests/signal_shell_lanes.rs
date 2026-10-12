//! s232 (wolf-lang#679, #681, #691, #141, #423) — the signal and process
//! surface a POSIX shell needs, on every machine. `[os.signal.set]`,
//! `[os.signal.poll]`, `[os.signal.raise]`, `[os.signal.ignored]`,
//! `[os.signal.send]`, `[os.signal.number]`, `[os.proc.self]`.
//!
//! Before this lane a shell written in wolf could not trap `USR1`,
//! `ALRM`, `PIPE` or most other signals (a listen of their bits answered
//! ok and armed nothing), could not ask which signals it was started
//! ignoring, could poll 0 for a signal already delivered, and could not
//! name its own id or signal a process it had not spawned. Two kinds of
//! witness:
//!
//! - the corpus row `os/signal_shell_rows.lu`, which signals nothing, on
//!   every host, under `conform-run` on the checked machine, the native
//!   and release tiers and lupin;
//! - the fixtures under `fixtures/signal_shell/`, which signal this
//!   process or a child, so they run on unix only: the checked machine
//!   and lupin under `conform-run`, the native and release binaries
//!   `wolf build` made — plain, and on linux again under
//!   `taskset -c 0-3` (wolf-lang#570, and the cell #691 was found in).
//!
//! Every lane is started with the listenable signals at their default
//! and an empty signal mask, set in the child before it runs (so a
//! harness started under `nohup`, or from a shell that ignores `INT`,
//! does not decide a row); a test that needs a signal ignored at start
//! says which. The harness's own `SigBlk`/`SigIgn` are printed first.
//!
//! lupin 0.1.51 (the 0.2.28 pairing) predates the mirror: it resolves
//! none of the five new names and answers `unsupported`, pinned by
//! version as pre-mirror (s180's design). lupin never listens
//! (`os_signal_listen` is declined, ruling #57 R8), and refuses the send
//! and the disposition read by name (R9 here): those fixtures expect
//! its refusal.

mod lane_exit;

#[cfg(unix)]
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The lupin builds that predate the mirror, by version and the commit
/// their records report: the 0.1.51 release (`f72e78d`, the 0.2.28
/// pairing, which is also wolf-interp trunk at this rebase). 0.1.50
/// (`32b970d`, and trunk `c00b88c`) were listed until 0.2.28 moved the
/// pairing; they are dropped, not widened.
const PRE_MIRROR_LUPIN: &[(&str, Option<&str>)] = &[("0.1.51", Some("f72e78d"))];

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
    commit: String,
}

fn pre_mirror(lupin: &Obs) -> bool {
    PRE_MIRROR_LUPIN
        .iter()
        .any(|(v, c)| *v == lupin.version && c.is_none_or(|c| lupin.commit.starts_with(c)))
}

/// The record in `text`, if a line of it parses as one.
fn record_in(text: &str) -> Option<Obs> {
    let rec: serde_json::Value = text.lines().rev().find_map(|l| {
        l.find('{')
            .and_then(|i| serde_json::from_str(l[i..].trim()).ok())
    })?;
    rec.get("verdict")?;
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        commit: rec["commit"].as_str().unwrap_or("").to_string(),
    })
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let text = String::from_utf8_lossy(bytes);
    record_in(&text).unwrap_or_else(|| panic!("{what} record parses: {text}"))
}

/// A working directory of the test's own (`[os.fs.path]`).
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("signal_shell_lanes")
        .join(test);
    std::fs::create_dir_all(d.join("target")).expect("scratch dir");
    d
}

fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

#[cfg(unix)]
fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/signal_shell")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
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

fn no_sibling() {
    assert!(
        std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
        "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
         (LUPIN={}) — the oracle leg of s232's gate did not run",
        std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
    );
    eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
}

/// The harness's own signal masks (linux), for the log.
#[cfg(unix)]
fn harness_masks() -> String {
    std::fs::read_to_string("/proc/self/status")
        .map(|t| {
            t.lines()
                .filter(|l| l.starts_with("SigBlk") || l.starts_with("SigIgn"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_else(|_| "no /proc".into())
}

/// The signals every lane starts at their default (the twenty a program
/// may catch), with `ignore` set to ignored instead, and an empty mask.
#[cfg(unix)]
fn start_clean(cmd: &mut Command, ignore: &'static [libc::c_int]) {
    const CATCHABLE: [libc::c_int; 20] = [
        libc::SIGHUP,
        libc::SIGTERM,
        libc::SIGQUIT,
        libc::SIGUSR2,
        libc::SIGINT,
        libc::SIGTSTP,
        libc::SIGTTIN,
        libc::SIGTTOU,
        libc::SIGCHLD,
        libc::SIGUSR1,
        libc::SIGALRM,
        libc::SIGPIPE,
        libc::SIGABRT,
        libc::SIGCONT,
        libc::SIGWINCH,
        libc::SIGURG,
        libc::SIGVTALRM,
        libc::SIGPROF,
        libc::SIGXCPU,
        libc::SIGXFSZ,
    ];
    // SAFETY: only async-signal-safe calls (sigaction, sigprocmask) in
    // the child between fork and exec, on constant data.
    unsafe {
        cmd.pre_exec(move || {
            for &sig in &CATCHABLE {
                let mut sa: libc::sigaction = std::mem::zeroed();
                sa.sa_sigaction = if ignore.contains(&sig) {
                    libc::SIG_IGN
                } else {
                    libc::SIG_DFL
                };
                libc::sigemptyset(&mut sa.sa_mask);
                libc::sigaction(sig, &sa, std::ptr::null_mut());
            }
            let mut none: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut none);
            libc::sigprocmask(libc::SIG_SETMASK, &none, std::ptr::null_mut());
            Ok(())
        });
    }
}

#[cfg(unix)]
/// The native or release binary of `entry`. A compile error FAILS the
/// gate (it is the defect at a pre-fix head, never an environment).
fn build(entry: &Path, dir: &Path, release: bool) -> Option<PathBuf> {
    let exe = dir.join(format!(
        "a-{}{}",
        if release { "release" } else { "native" },
        std::env::consts::EXE_SUFFIX
    ));
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
            "SKIP: environment cannot build the {} tier: {}",
            if release { "release" } else { "native" },
            stderr.trim()
        );
        return None;
    }
    assert!(out.status.success(), "wolf build failed: {stderr}");
    Some(exe)
}

#[cfg(unix)]
/// One way to run a program: its name in messages and the command.
struct Lane {
    name: String,
    cmd: Command,
    /// The lane answers through a record when the program returns
    /// (checked, lupin); a binary prints straight through.
    recorded: bool,
    lupin: bool,
}

/// `taskset` on linux, from PATH (NixOS keeps it outside `/usr/bin`).
#[cfg(unix)]
fn taskset() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("taskset"))
        .find(|p| p.is_file())
}

#[cfg(unix)]
/// Every machine a unix fixture runs on: checked and lupin under
/// `conform-run`, the native and release binaries — plain, and on linux
/// again under `taskset -c 0-3` (taskset execs its command, so the pid
/// is the program's).
fn lanes(entry: &Path, dir: &Path) -> Vec<Lane> {
    let mut out = Vec::new();
    let mut c = Command::new(wolf());
    c.arg("conform-run")
        .arg(entry)
        .arg("--checked")
        .arg("--json");
    out.push(Lane {
        name: "checked".into(),
        cmd: c,
        recorded: true,
        lupin: false,
    });
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(entry, dir, release) else {
            continue;
        };
        out.push(Lane {
            name: tier.into(),
            cmd: Command::new(&exe),
            recorded: false,
            lupin: false,
        });
        if let Some(taskset) = taskset() {
            let mut t = Command::new(taskset);
            t.args(["-c", "0-3"]).arg(&exe);
            out.push(Lane {
                name: format!("{tier} (taskset -c 0-3)"),
                cmd: t,
                recorded: false,
                lupin: false,
            });
        }
    }
    match sibling_lupin() {
        Some(l) => {
            let mut c = Command::new(l);
            c.arg("conform-run").arg(entry).arg("--json");
            out.push(Lane {
                name: "lupin".into(),
                cmd: c,
                recorded: true,
                lupin: true,
            });
        }
        None => no_sibling(),
    }
    out
}

/// Run `cmd` in `dir`, 0 the null device, 1 and 2 piped.
fn run(mut cmd: Command, dir: &Path) -> Output {
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.output().expect("run")
}

#[cfg(unix)]
/// What a lane printed, or `None` for a lupin that refuses (pre-mirror,
/// or by name where `lupin_refuses`) — said in the log either way. A
/// pre-mirror lupin answers `unsupported`, or the verdict `pinned` names
/// for a fixture where its own pre-s232 rows decide first.
fn lane_stdout(
    lane: &Lane,
    out: &Output,
    what: &str,
    lupin_refuses: bool,
    pinned: &str,
) -> Option<String> {
    if lane.recorded {
        let obs = parse_obs(&out.stdout, what);
        if lane.lupin && (pre_mirror(&obs) || lupin_refuses) {
            let want = if pre_mirror(&obs) {
                pinned
            } else {
                "unsupported"
            };
            assert_eq!(
                obs.verdict, want,
                "{what}: lupin {} at {} refuses (pre-mirror, or by name)",
                obs.version, obs.commit
            );
            eprintln!(
                "{} {what}: lupin {} at {} {}",
                if pre_mirror(&obs) {
                    "PINNED"
                } else {
                    "REFUSED BY NAME"
                },
                obs.version,
                obs.commit,
                obs.verdict
            );
            return None;
        }
        assert_eq!(obs.verdict, "exit(0)", "{what}: {obs:?}");
        return Some(obs.stdout);
    }
    assert_eq!(
        out.status.code(),
        Some(0),
        "{what}: ended {:?}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(unix)]
/// A fixture that prints `want` on every machine, started with `ignore`
/// ignored and every other listenable signal at its default.
fn every_lane_prints(name: &str, ignore: &'static [libc::c_int], want: &str, lupin_refuses: bool) {
    every_lane_prints_pinned(name, ignore, want, lupin_refuses, "unsupported");
}

#[cfg(unix)]
/// [`every_lane_prints`], with the verdict a pre-mirror lupin answers.
fn every_lane_prints_pinned(
    name: &str,
    ignore: &'static [libc::c_int],
    want: &str,
    lupin_refuses: bool,
    pinned: &str,
) {
    eprintln!("harness: {}", harness_masks());
    let entry = fixture(name);
    // One directory per (fixture, start): two tests run one fixture with
    // different ignores, side by side, and each builds its own binaries.
    let dir = scratch(&format!("{}_{}", name.replace('.', "_"), ignore.len()));
    for mut lane in lanes(&entry, &dir) {
        start_clean(&mut lane.cmd, ignore);
        let what = format!("the {} lane on {name}", lane.name);
        let cmd = std::mem::replace(&mut lane.cmd, Command::new("true"));
        let out = run(cmd, &dir);
        let Some(got) = lane_stdout(&lane, &out, &what, lupin_refuses, pinned) else {
            continue;
        };
        assert_eq!(got, want, "{what} (s232)");
        eprintln!("PASS {what}");
    }
}

// ------------------------------------------------ the corpus rows --

/// The rows that signal nothing: every wolfgang lane under `conform-run`
/// prints `want`, and lupin agrees or answers its pinned pre-mirror
/// verdict. Red at trunk 6143943d: E0301 (`os_signal_send` unknown) on
/// every wolfgang lane; lupin 0.1.51 `unsupported`.
#[test]
fn the_shell_rows_on_every_machine() {
    let rel = "os/signal_shell_rows.lu";
    let want = "poll_outside=io\npoll_past_table=io\npoll_quiet_user=0\nignore_kill=io\n\
default_past_table=io\nsend_pid_zero=invalid\nsend_pid_negative=invalid\nsend_two_bits=invalid\n\
send_past_table=invalid\nsend_wide_pid=invalid\nnumber_two_bits=invalid\nnumber_zero=invalid\n\
number_interrupt: true\nnumber_kill: true\npid: true\nppid: true\n";
    let entry = corpus(rel);
    let dir = scratch("rows");
    for flag in ["--checked", "--native", "--release"] {
        let mut cmd = Command::new(wolf());
        cmd.arg("conform-run").arg(&entry).arg(flag).arg("--json");
        let out = run(cmd, &dir);
        if flag != "--checked" && lane_exit::environment_refusal(&out, &format!("wolf {flag}")) {
            eprintln!(
                "SKIP: environment cannot run the {flag} lane: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            continue;
        }
        let obs = parse_obs(&out.stdout, &format!("{flag} on {rel}"));
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want),
            "the {flag} lane on {rel} (s232)"
        );
        eprintln!("PASS the {flag} lane on {rel}");
    }
    match sibling_lupin() {
        Some(l) => {
            let mut cmd = Command::new(l);
            cmd.arg("conform-run").arg(&entry).arg("--json");
            let obs = parse_obs(&run(cmd, &dir).stdout, "lupin");
            if pre_mirror(&obs) {
                assert_eq!(
                    obs.verdict, "unsupported",
                    "lupin {} at {} (pre-mirror, pinned) on {rel}",
                    obs.version, obs.commit
                );
                eprintln!(
                    "PINNED lupin {} at {} on {rel} (pre-mirror)",
                    obs.version, obs.commit
                );
            } else {
                assert_eq!(
                    (obs.verdict.as_str(), obs.stdout.as_str()),
                    ("exit(0)", want),
                    "lupin {} at {} on {rel} (s232 mirrored)",
                    obs.version,
                    obs.commit
                );
                eprintln!("PASS lupin {} at {} on {rel}", obs.version, obs.commit);
            }
        }
        None => no_sibling(),
    }
}

// ------------------------------------------- the unix fixtures --

/// #679's smaller finding: a listen of a bit outside the listenable
/// meanings is `io`. Red at trunk 6143943d: `kill=ok`, `past_table=ok`,
/// `every_bit=ok` on checked, native and release. lupin never listens.
#[cfg(unix)]
#[test]
fn a_listen_outside_the_table_is_io() {
    every_lane_prints(
        "listen_shape.lu",
        &[],
        "kill=io\npast_table=io\nevery_bit=io\nempty=io\nuser=ok\n",
        true,
    );
}

/// #679 and #423: each of the eleven new meanings, sent by a child to
/// this process, is caught and polled, and the program lives. Red at
/// trunk: the program died by SIGUSR1 on the first round.
#[cfg(unix)]
#[test]
fn each_new_meaning_is_caught() {
    every_lane_prints(
        "trap_each.lu",
        &[],
        "USR1=caught\nALRM=caught\nPIPE=caught\nABRT=caught\nCONT=caught\nWINCH=caught\n\
URG=caught\nVTALRM=caught\nPROF=caught\nXCPU=caught\nXFSZ=caught\nalive\n",
        true,
    );
}

/// #679 (pelt's `pending/trap_ignore_usr1`): an ignored `USER` and
/// `BROKEN_PIPE` are survived and handed to a child. Red at trunk: the
/// ignore of 512 was `io` and SIGUSR1 ended the program. lupin 0.1.51's
/// own shape row answers first there (`io`, so `exit(1)`): pinned.
#[cfg(unix)]
#[test]
fn an_ignored_user_signal_is_survived_and_inherited() {
    every_lane_prints_pinned(
        "ignore_user.lu",
        &[],
        "alive\nchild_USR1_ignored=true\nchild_PIPE_ignored=true\n",
        true,
        "exit(1)",
    );
}

/// #423: a write to a pipe nobody reads answers `io` when
/// `BROKEN_PIPE` is ignored or listened, and the program lives; the
/// listened one is polled. Red at trunk: the ignore of 2048 was `io`
/// and the write ended the program by SIGPIPE. lupin 0.1.51 answers its
/// own shape row first (`exit(1)`): pinned.
#[cfg(unix)]
#[test]
fn a_broken_pipe_is_a_row_when_ignored_or_listened() {
    every_lane_prints_pinned(
        "broken_pipe.lu",
        &[],
        "ignored_write=io\nlistened_write=io\nlistened_poll=2048\nalive\n",
        true,
        "exit(1)",
    );
}

/// #681: the dispositions the program was started with — `TERMINATE`
/// and `USER` ignored by the harness; a listen ends one ignore. Red at
/// trunk: E0301 (`os_signal_ignored` unknown).
#[cfg(unix)]
#[test]
fn what_was_ignored_at_start_is_read() {
    every_lane_prints(
        "ignored_at_start.lu",
        &[libc::SIGTERM, libc::SIGUSR1],
        "terminate=true\nuser=true\nbroken_pipe=false\ninterrupt=false\n\
terminate_after_listen=false\nuser_still=true\n",
        true,
    );
}

/// #681 on the one signal the `wolf` binary's own runtime changes:
/// started with `BROKEN_PIPE` ignored the program reads it ignored, and
/// started with it at the default (the test above) not — on the checked
/// machine too, where `wolf` records the inherited disposition before
/// Rust's runtime ignores it.
#[cfg(unix)]
#[test]
fn an_inherited_broken_pipe_ignore_is_read_on_every_machine() {
    every_lane_prints(
        "ignored_at_start.lu",
        &[libc::SIGTERM, libc::SIGUSR1, libc::SIGPIPE],
        "terminate=true\nuser=true\nbroken_pipe=true\ninterrupt=false\n\
terminate_after_listen=false\nuser_still=true\n",
        true,
    );
}

/// #141: a signal to a process by id. Red at trunk: E0301
/// (`os_signal_send` unknown).
#[cfg(unix)]
#[test]
fn a_signal_reaches_any_pid() {
    // SAFETY: geteuid takes nothing and cannot fail.
    let root = unsafe { libc::geteuid() } == 0;
    let want = format!(
        "term=ok\nterm_status=-{}\nkill=ok\nkill_status=-9\nself_zero=ok\nnobody=not_found\npid1={}\n",
        libc::SIGTERM,
        if root { "ok" } else { "denied" }
    );
    let want: &'static str = Box::leak(want.into_boxed_str());
    every_lane_prints("send.lu", &[], want, true);
}

/// #141: this process's id is the process the harness started, its
/// parent's is the harness, and a child's `$PPID` is it. Red at trunk:
/// E0301 (`os_pid` unknown).
#[cfg(unix)]
#[test]
fn this_process_ids_are_the_hosts() {
    eprintln!("harness: {}", harness_masks());
    let entry = fixture("ids.lu");
    let dir = scratch("ids");
    let me = std::process::id();
    for mut lane in lanes(&entry, &dir) {
        start_clean(&mut lane.cmd, &[]);
        let what = format!("the {} lane on ids.lu", lane.name);
        let mut cmd = std::mem::replace(&mut lane.cmd, Command::new("true"));
        cmd.current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = cmd.spawn().expect("spawn");
        let pid = child.id();
        let out = child.wait_with_output().expect("wait");
        let Some(got) = lane_stdout(&lane, &out, &what, false, "unsupported") else {
            continue;
        };
        assert_eq!(
            got,
            format!("pid={pid}\nppid={me}\nchild_parent_is_me=true\n"),
            "{what} (s232)"
        );
        eprintln!("PASS {what} (pid {pid}, parent {me})");
    }
}

/// #691: a poll never trails a delivery — 2,000 raises and 2,000 reaps
/// of a child that signalled this process, every one polled at once.
/// Red at trunk 6143943d: `raise_then_poll_missed=2000` on native and
/// release, and the reap shape missed 2 or 3 of 2,000 under
/// `taskset -c 0-3` (kasumi). lupin never listens.
#[cfg(unix)]
#[test]
fn a_poll_never_trails_a_delivery() {
    every_lane_prints(
        "poll_exact.lu",
        &[],
        "raise_then_poll_missed=0\nreap_then_poll_missed=0\n",
        true,
    );
}

/// `[os.signal.number]`: every meaning's number is the host's, on every
/// machine lupin included (its table against the host's).
#[cfg(unix)]
#[test]
fn every_number_is_the_hosts() {
    let host = [
        libc::SIGHUP,
        libc::SIGTERM,
        libc::SIGQUIT,
        libc::SIGUSR2,
        libc::SIGINT,
        libc::SIGTSTP,
        libc::SIGTTIN,
        libc::SIGTTOU,
        libc::SIGCHLD,
        libc::SIGUSR1,
        libc::SIGALRM,
        libc::SIGPIPE,
        libc::SIGABRT,
        libc::SIGCONT,
        libc::SIGWINCH,
        libc::SIGURG,
        libc::SIGVTALRM,
        libc::SIGPROF,
        libc::SIGXCPU,
        libc::SIGXFSZ,
        libc::SIGKILL,
        libc::SIGSTOP,
        libc::SIGSEGV,
        libc::SIGBUS,
        libc::SIGFPE,
        libc::SIGILL,
        libc::SIGTRAP,
        libc::SIGSYS,
    ];
    let line = host
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    let want: &'static str = Box::leak(format!("{line}\n").into_boxed_str());
    every_lane_prints("number.lu", &[], want, false);
}
