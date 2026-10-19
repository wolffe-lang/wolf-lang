//! s247 (ruling #90, `[os.proc.fork]`) — the fork: a child that keeps
//! running wolf code, sound beside the runtime's own threads.
//!
//! Each witness is a fixture under `fixtures/fork/`, built on the native
//! and release tiers and run where a fork may run:
//!
//! - **linux**: inside a throwaway `--network none` container of its own,
//!   NO init (the shell is process 1, so the programs are not), the test's
//!   own directory at `/w` its working directory; each row again under
//!   `taskset -c 0-3`, asked from inside (wolf-lang#570), where the place
//!   has four cpus. `WOLF_FORK_ENGINE` names the engine (otherwise the
//!   first of `docker` and `podman` that answers), `WOLF_FORK_IMAGE` the
//!   image (default `docker.io/library/ubuntu:24.04`). Where no engine
//!   runs it the witnesses SKIP, loudly and by name, and
//!   **`WOLF_FORK_REQUIRE_CONTAINER=1` (CI's linux job) makes that a
//!   failure**.
//! - **macOS**: on CI's throwaway runner only (`GITHUB_ACTIONS`).
//!
//! **No process is forked on a host otherwise; every fork loop is capped
//! (the storm at 64 children, killed by recorded pid and reaped; the
//! timing loop at the count it is given); nothing is signalled that the
//! program did not start or is not.** Every row's stdout and stderr is
//! printed as a `REPORT` line, so a green log is the table.
//!
//! **Red at trunk** (`85dab5d7`, 0.2.33): every fixture is E0301,
//! `os_fork` not in scope — a compile error the build step fails on
//! (never a skip). **Red with the child's re-making planted out**: the hub
//! row (the child's trampoline writes the parent's self-pipe: the parent's
//! poll takes the child's TERMINATE, the child is killed at the cap) and
//! the readiness-set row (the child's waits ride the parent's epoll set or
//! a kqueue it was never handed: killed at the cap).
//!
//! The checked machine refuses `os_fork` by name (it is the `wolf` process
//! interpreting the program); lupin 0.1.54, the pairing, does not resolve
//! the name, and a newer lupin is held to a refusal by name (it admits no
//! `unsafe`, and Rust's standard library has no fork; a wolf-interp
//! issue). Both run on the host: neither forks.

mod lane_exit;

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

/// The pairing at this lane, which predates `os_fork`.
const PRE_MIRROR_LUPIN: &str = "0.1.54";

#[derive(Debug)]
struct Obs {
    verdict: String,
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
        .join("tests/fixtures/fork")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// A fresh working directory of the test's own.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("fork_lanes")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
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
             (LUPIN={}) — the oracle leg of s247's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    found
}

// ------------------------------------------------- the checked machine --

/// The checked machine and lupin answer `os_fork` with a refusal by name,
/// on every host: neither forks the process it runs in. On windows the
/// checked machine answers `unsupported` as the runtime does there, so
/// `main.lu` ends 1 after printing the row. lupin 0.1.54 does not resolve
/// the name; a newer lupin refuses it by name.
#[test]
fn the_checked_machine_and_lupin_refuse_by_name() {
    let dir = scratch("machines");
    let entry = fixture("main.lu");
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(&entry)
        .arg("--checked")
        .arg("--json")
        .current_dir(&dir)
        .stdin(Stdio::null())
        .output()
        .expect("conform-run runs");
    let checked = parse_obs(&out.stdout, "the checked machine's record");
    report(&format!(
        "REPORT checked on main.lu: {} {}",
        checked.verdict, checked.reason
    ));
    if cfg!(unix) {
        assert_eq!(
            checked.verdict, "unsupported",
            "the checked machine refuses os_fork by name: {checked:?}"
        );
        assert!(
            checked.reason.contains("`os_fork`"),
            "the checked machine names the call: {}",
            checked.reason
        );
    } else {
        assert_eq!(
            checked.verdict, "exit(1)",
            "the checked machine answers unsupported on windows: {checked:?}"
        );
    }
    let Some(l) = lupin_or_skip() else {
        return;
    };
    let out = Command::new(&l)
        .arg("conform-run")
        .arg(&entry)
        .arg("--json")
        .current_dir(&dir)
        .stdin(Stdio::null())
        .output()
        .expect("lupin runs");
    let obs = parse_obs(&out.stdout, "lupin's record");
    report(&format!(
        "REPORT lupin {} on main.lu: {} {}",
        obs.version, obs.verdict, obs.reason
    ));
    assert_eq!(obs.verdict, "unsupported", "lupin refuses os_fork: {obs:?}");
    assert!(
        obs.reason.contains("`os_fork`"),
        "lupin's refusal names the call: {}",
        obs.reason
    );
    if obs.version == PRE_MIRROR_LUPIN {
        assert!(
            obs.reason.contains("does not resolve"),
            "lupin {} predates os_fork: {}",
            obs.version,
            obs.reason
        );
    }
}

/// Windows has no fork: the call answers `unsupported`, by name, on both
/// compiled tiers.
#[cfg(windows)]
#[test]
fn a_fork_on_windows_is_unsupported() {
    let dir = scratch("windows");
    for release in [false, true] {
        let Some(exe) = build(&fixture("main.lu"), &dir, "main", release) else {
            continue;
        };
        let out = Command::new(&exe)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .output()
            .expect("the program runs");
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        report(&format!("REPORT main.lu on windows: {stdout:?}"));
        assert_eq!(stdout, "fork: unsupported\n");
        assert_eq!(out.status.code(), Some(1));
    }
}

// ------------------------------------------------------------ the place --

/// Where the forks run: a throwaway container (linux) or CI's runner
/// (macOS).
#[cfg(unix)]
struct Live {
    /// `Some(engine)` in a container; `None` on the runner itself.
    engine: Option<String>,
    /// The test's own directory: `/w` in the container, the cwd on the
    /// runner. The binaries are built into it.
    dir: PathBuf,
}

#[cfg(unix)]
fn image() -> String {
    std::env::var("WOLF_FORK_IMAGE").unwrap_or_else(|_| "docker.io/library/ubuntu:24.04".into())
}

#[cfg(unix)]
impl Live {
    /// The place to run, or a loud skip (a failure where CI requires it).
    fn new(test: &str) -> Option<Live> {
        let dir = scratch(test);
        if cfg!(target_os = "macos") {
            if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
                eprintln!(
                    "SKIP: {test}: not CI's throwaway runner — the fork witnesses run on macOS \
                     only there"
                );
                return None;
            }
            return Some(Live { engine: None, dir });
        }
        let required = std::env::var_os("WOLF_FORK_REQUIRE_CONTAINER").is_some();
        let candidates: Vec<String> = match std::env::var("WOLF_FORK_ENGINE") {
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
            "WOLF_FORK_REQUIRE_CONTAINER is set and no engine ran {} — {test} did not run: \
             {why:?}",
            image()
        );
        eprintln!(
            "SKIP: {test}: no container engine ran {} ({why:?}) — the fork witnesses did not run",
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

    /// `script` under `sh -c` in the test's directory: in a throwaway
    /// container (no network, NO init, the directory at `/w` read-write
    /// and its working directory), or on the runner.
    fn sh(&self, script: &str) -> Output {
        let mut cmd = match &self.engine {
            Some(e) => {
                let mut c = Command::new(e);
                c.args(["run", "--rm", "--network", "none"]);
                c.arg("-v").arg(format!("{}:/w", self.dir.display()));
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
}

/// Each compiled machine's row for `entry`: its label and the command the
/// place runs (the binary, or `taskset -c 0-3` and the binary).
#[cfg(unix)]
fn rows(live: &Live, entry: &Path, stem: &str, test: &str) -> Vec<(String, String)> {
    let four = live.four_cpus();
    if !four && cfg!(target_os = "linux") {
        eprintln!("SKIP: {test}: fewer than four cpus — the taskset -c 0-3 rows did not run");
    }
    let mut out = Vec::new();
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(entry, &live.dir, stem, release) else {
            continue;
        };
        let name = exe.file_name().unwrap().to_string_lossy().to_string();
        out.push((tier.to_string(), live.at(&name)));
        if four {
            out.push((
                format!("{tier} taskset -c 0-3"),
                format!("taskset -c 0-3 {}", live.at(&name)),
            ));
        }
    }
    out
}

/// Run every row of `name` through `wrap` (the command line around the
/// row's program) and hold its stdout, `rc=` line included, to `want`.
#[cfg(unix)]
fn every_row(test: &str, name: &str, wrap: &dyn Fn(&Live, &str) -> String, want: &str) {
    let Some(live) = Live::new(test) else {
        return;
    };
    let entry = fixture(name);
    let stem = name.trim_end_matches(".lu");
    for (row, prog) in rows(&live, &entry, stem, test) {
        let out = live.sh(&wrap(&live, &prog));
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        report(&format!(
            "REPORT {name} {row}: {} || {}",
            stdout.replace('\n', " | "),
            String::from_utf8_lossy(&out.stderr).replace('\n', " | ")
        ));
        assert_eq!(stdout, want, "{name}, the {row} binary");
    }
}

#[cfg(unix)]
fn plain(_: &Live, prog: &str) -> String {
    format!("{prog}; echo rc=$?")
}

// -------------------------------------------------------- the witnesses --

/// A fork from `main` with nothing started: the child's own pid, the
/// parent as its parent, `(0, -1)`; the parent's `(pid, h)`, with
/// `os_proc_pid(h)` the child and `os_wait_status(h)` its exit code; a
/// second wait `io`.
#[cfg(unix)]
#[test]
fn a_fork_from_main() {
    every_row(
        "main",
        "main.lu",
        &plain,
        "child: pid is new true, ppid is the parent true, handle -1\n\
         parent: pid > 0 true, handle 0, os_proc_pid names it true, status 7\n\
         parent: a second wait: io\n\
         rc=0\n",
    );
}

/// Fork then `os_exec`, the common shape: the new program keeps the
/// child's pid.
#[cfg(unix)]
#[test]
fn a_fork_then_exec_keeps_the_pid() {
    every_row(
        "exec",
        "exec.lu",
        &plain,
        "exec: the new program's pid is the child's true, status 0\nrc=0\n",
    );
}

/// A child's child, and the wait tables: the parent's children are not
/// the child's (`io`, as `ECHILD`), the child's poll sees none live, and
/// each waits for its own.
#[cfg(unix)]
#[test]
fn a_fork_chain_and_the_wait_tables() {
    every_row(
        "chain",
        "chain.lu",
        &plain,
        "child: wait on the parent's spawn: io\n\
         child: wait on a handle it never had: io\n\
         child: poll (-1, 0)\n\
         grandchild: its parent is the child true\n\
         child: grandchild handle 1, status 5, a -1 b -1\n\
         parent: child handle 1 status 9, spawn handle 0 status 3\n\
         rc=0\n",
    );
}

/// The hub across a fork: a TERMINATE to the child is the child's and
/// never the parent's, and the reverse. The place's own SigBlk and SigIgn
/// are printed first (linux), so a red row can be read against them. Then
/// again with TERMINATE BLOCKED from the start (`blockexec.c`, the mask a
/// job inherits from a shell that blocks it): the parent's listen has its
/// drain thread unblock it there, and the child's first wait makes a drain
/// thread of the child's that must unblock it again — without that, the
/// child's TERMINATE pends for ever and the parent's cap kills it.
///
/// Which row tells a child that kept the parent's pipes (the hub plant):
/// on linux the plain row — the child's trampoline writes the parent's
/// pipe, the parent's drain thread takes the byte, and the child, parked in
/// a futex wait the handler does not end, is killed at the cap (CI
/// 39314575470); on macOS the BLOCKED row — there a handler run on the
/// child's only thread ends its condvar wait, and the child's own look
/// drains the inherited pipe before the parent's drain thread does, so the
/// plain row is a race the child wins, while with TERMINATE blocked nothing
/// in the child can take it (the same run: killed at the cap).
#[cfg(unix)]
#[test]
fn the_hub_is_the_childs_own() {
    let want = "hub: the child took its TERMINATE true\n\
                hub: child status 0, the parent heard the child's TERMINATE false\n\
                hub: the second child heard the parent's TERMINATE false\n\
                hub: the parent took its TERMINATE true, second child status 0\n\
                rc=0\n";
    let mask = |_: &Live, prog: &str| {
        let status = if cfg!(target_os = "linux") {
            "grep -E 'SigBlk|SigIgn' /proc/self/status >&2; "
        } else {
            ""
        };
        format!("{status}{prog}; echo rc=$?")
    };
    every_row("hub", "hub.lu", &mask, want);
    let Some(live) = Live::new("hub_blocked") else {
        return;
    };
    let exe = live.dir.join("blockexec");
    let mut cc = Command::new("cc");
    cc.arg("-O2")
        .arg("-o")
        .arg(&exe)
        .arg(fixture("blockexec.c"));
    if cfg!(target_os = "linux") {
        cc.arg("-static");
    }
    let built = cc.output().map(|o| o.status.success()).unwrap_or(false);
    if !built {
        assert!(
            std::env::var_os("WOLF_FORK_REQUIRE_CONTAINER").is_none(),
            "no cc built blockexec.c where the fork witnesses are required"
        );
        eprintln!("SKIP: the_hub_is_the_childs_own's blocked row: no cc built blockexec.c");
        return;
    }
    // SIGTERM is 15 on linux and macOS.
    let term = libc::SIGTERM;
    for (row, prog) in rows(&live, &fixture("hub.lu"), "hub", "hub_blocked") {
        // `taskset -c 0-3 /w/hub-…` → `taskset -c 0-3 /w/blockexec 15 /w/hub-…`
        let line = match prog.rsplit_once(' ') {
            Some((pre, bin)) => format!("{pre} {} {term} {bin}", live.at("blockexec")),
            None => format!("{} {term} {prog}", live.at("blockexec")),
        };
        let out = live.sh(&format!("{line}; echo rc=$?"));
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        report(&format!(
            "REPORT hub.lu with TERMINATE blocked from the start, {row}: {} || {}",
            stdout.replace('\n', " | "),
            String::from_utf8_lossy(&out.stderr).replace('\n', " | ")
        ));
        assert_eq!(
            stdout, want,
            "hub.lu with TERMINATE blocked from the start, {row}"
        );
    }
}

/// The readiness set across a fork: parent and child each parked in their
/// own reactor at once, a net round trip between them.
#[cfg(unix)]
#[test]
fn the_readiness_set_is_the_childs_own() {
    every_row(
        "reactor",
        "reactor.lu",
        &plain,
        "reactor: the child took pong\n\
         reactor: the parent took ping, child status 0\n\
         rc=0\n",
    );
}

/// The `busy` rule: a fork with a task parked, and from inside a task,
/// answers `busy`; after the scope, a fork works and the child's own pool
/// runs its tasks.
#[cfg(unix)]
#[test]
fn the_busy_rule_and_the_pool() {
    every_row(
        "tasks",
        "tasks.lu",
        &plain,
        "tasks: a fork with a task parked: busy\n\
         tasks: a fork from inside a task: busy\n\
         tasks: the child's own tasks ran, sum 6\n\
         tasks: after the scope a fork works, child status 0\n\
         tasks: a fork right after each of 300 one-task scopes: busy 0 times\n\
         rc=0\n",
    );
}

/// `[os.proc.fds]`'s rows across a fork: one write offset, one read
/// offset, a pipe end inherited.
#[cfg(unix)]
#[test]
fn descriptors_and_offsets_are_shared() {
    let dir = |live: &Live, prog: &str| format!("{prog} {}; echo rc=$?", live.at(""));
    every_row(
        "fds",
        "fds.lu",
        &dir,
        "fds: one write offset: abc\n\
         fds: the child read 2\n\
         fds: one read offset: 1 then the child, then 3\n\
         fds: a pipe end inherited: via the pipe\n\
         rc=0\n",
    );
}

/// A forked child in the foreground group writes to the terminal: the
/// program runs as the leader of a fresh pseudo-terminal's session
/// (`script`: util-linux's `-qec` on linux, the BSD form on macOS), and
/// the transcript is held.
#[cfg(unix)]
#[test]
fn a_child_in_the_foreground_writes_to_the_terminal() {
    let Some(live) = Live::new("tty") else {
        return;
    };
    let entry = fixture("tty.lu");
    for (row, prog) in rows(&live, &entry, "tty", "tty") {
        let script = if cfg!(target_os = "linux") {
            format!("script -qec '{prog}' /dev/null; echo rc=$?")
        } else {
            format!("script -q /dev/null {prog}; echo rc=$?")
        };
        let out = live.sh(&script);
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        report(&format!(
            "REPORT tty.lu {row}: {} || {}",
            stdout.replace('\n', " | "),
            String::from_utf8_lossy(&out.stderr).replace('\n', " | ")
        ));
        assert!(
            stdout.contains(
                "inherited the parent's group true, in the foreground true, writes to the terminal\n"
            ),
            "tty.lu, the {row} binary: the child's line: {stdout:?}"
        );
        assert!(
            stdout.contains("tty: child status 0, the parent took the terminal back true\n"),
            "tty.lu, the {row} binary: the parent's line: {stdout:?}"
        );
        assert!(
            stdout.ends_with("rc=0\n"),
            "tty.lu, the {row} binary: {stdout:?}"
        );
    }
}

/// A bounded storm: 64 children, each killed by its recorded pid and
/// reaped. Then a host refusal before the cap — a user's low
/// `RLIMIT_NPROC` (linux: user 1000 in the container; macOS: the runner's
/// user, whose processes already pass it) — answers `io` with the host's
/// number, and the children made until then are killed and reaped.
#[cfg(unix)]
#[test]
fn a_bounded_storm_and_the_hosts_refusal() {
    every_row(
        "storm",
        "storm.lu",
        &plain,
        "storm: 64 forked of a cap of 64, 64 killed by recorded pid, 64 reaped, 64 ended by KILL\n\
         rc=0\n",
    );
    let Some(live) = Live::new("storm_limit") else {
        return;
    };
    for (row, prog) in rows(&live, &fixture("storm.lu"), "storm", "storm_limit") {
        // `exec`, so the shell that lowers the limit forks nothing under
        // it (macOS: the runner's user already has more processes than 12,
        // and a `bash` fork there is refused before the program runs).
        let script = if live.engine.is_some() {
            format!(
                "setpriv --reuid=1000 --regid=1000 --clear-groups bash -c 'ulimit -u 12; exec \
                 {prog}'; echo rc=$?"
            )
        } else {
            format!("bash -c 'ulimit -u 12; exec {prog}'; echo rc=$?")
        };
        let out = live.sh(&script);
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        report(&format!(
            "REPORT storm.lu under a low RLIMIT_NPROC, {row}: {} || {}",
            stdout.replace('\n', " | "),
            String::from_utf8_lossy(&out.stderr).replace('\n', " | ")
        ));
        assert!(
            stdout.starts_with("storm: refused before the cap true: io (host code "),
            "storm.lu, the {row} binary under a low RLIMIT_NPROC: {stdout:?}"
        );
        assert!(
            stdout
                .ends_with("every child made killed true, reaped true, ended by KILL true\nrc=0\n"),
            "storm.lu, the {row} binary under a low RLIMIT_NPROC: {stdout:?}"
        );
    }
}

/// The pelt shape, fork, run, wait, 200 times a shape: every status 0. The
/// milliseconds are REPORTed, never held (a timing is the PR's evidence,
/// measured on kasumi; a runner's is noise); dash's command substitution
/// loops beside them where the place has dash.
#[cfg(unix)]
#[test]
fn the_pelt_shape_loop() {
    let Some(live) = Live::new("loop") else {
        return;
    };
    let entry = fixture("loop.lu");
    for (row, prog) in rows(&live, &entry, "loop", "loop") {
        for shape in ["exit", "exec", "spawn"] {
            let out = live.sh(&format!("{prog} {shape} 200; echo rc=$?"));
            let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
            report(&format!(
                "REPORT loop.lu {row} {shape}: {} || {}",
                stdout.replace('\n', " | "),
                String::from_utf8_lossy(&out.stderr).replace('\n', " | ")
            ));
            assert_eq!(
                stdout,
                format!("loop: {shape} 200, every status 0 true\nrc=0\n"),
                "loop.lu, the {row} binary, {shape}"
            );
        }
    }
    let dash = live.sh(
        "command -v dash >/dev/null || exit 0; for b in 'x=$(true)' 'x=$(/bin/true)'; do \
         s=$(date +%s%N); dash -c \"i=0; while [ \\$i -lt 200 ]; do $b; i=\\$((i+1)); done\"; \
         e=$(date +%s%N); echo \"dash $b x 200: $(( (e - s) / 1000000 )) ms\"; done",
    );
    report(&format!(
        "REPORT dash beside it: {}",
        String::from_utf8_lossy(&dash.stdout).replace('\n', " | ")
    ));
}

/// A toy subshell, the shape pelt's sh20 ports: traps back to default in
/// the child, a program run there, the status as a shell reports it.
#[cfg(unix)]
#[test]
fn a_toy_subshell() {
    every_row(
        "subshell",
        "subshell.lu",
        &plain,
        "subshell: the child's own TERMINATE ended it: 143; the parent's trap held: true\n\
         subshell: a program run in the child: 3\n\
         subshell: QUIT stays ignored in the child: 0\n\
         rc=0\n",
    );
}

/// The inventory's allocator row, a control and black box: the HOST C
/// library's allocator across a fork made while three other threads are
/// inside it (`fixtures/fork/mallocfork.c`, 300 forks). The runtime does not
/// lean on the answer — it forks only with every thread of its own at rest
/// in the kernel, none in the allocator — so the line is REPORTed, never
/// held; it says what a protocol without the hold would have stood on
/// (kasumi, glibc 2.39 in ubuntu:24.04: 500 of 500 children ran their own
/// malloc). Built by the runner's `cc` — static on linux, so the binary
/// carries the runner's C library into the container — and no `cc` skips
/// loudly.
#[cfg(unix)]
#[test]
fn the_host_allocator_across_a_fork_a_control() {
    let Some(live) = Live::new("mallocfork") else {
        return;
    };
    let src = fixture("mallocfork.c");
    let exe = live.dir.join("mallocfork");
    let mut cc = Command::new("cc");
    cc.arg("-O2").arg("-o").arg(&exe).arg(&src).arg("-lpthread");
    if cfg!(target_os = "linux") {
        cc.arg("-static");
    }
    match cc.output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            eprintln!(
                "SKIP: the allocator control did not build: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            );
            return;
        }
        Err(e) => {
            eprintln!("SKIP: no cc for the allocator control: {e}");
            return;
        }
    }
    let out = live.sh(&format!("{} 300", live.at("mallocfork")));
    let stdout = String::from_utf8_lossy(&out.stdout);
    report(&format!(
        "REPORT the host allocator across a fork ({}): {} || {}",
        std::env::consts::OS,
        stdout.trim(),
        String::from_utf8_lossy(&out.stderr).trim()
    ));
    assert!(
        stdout.starts_with("mallocfork: 300 forks while 3 threads malloc: "),
        "the control ran: {stdout:?}"
    );
}

/// The inventory's thread census (`[os.proc.fork.note]`) and the child's
/// one thread: a program that names the hub and a parking net call counts
/// its threads by the host at each state — printed for the record, since a
/// pool's size is the host's cpus — and the forked child is held to ONE
/// thread at the return, its drain thread made by its first listen and its
/// reactor thread by its first park (one more each).
#[cfg(unix)]
#[test]
fn the_child_is_one_thread() {
    let Some(live) = Live::new("census") else {
        return;
    };
    let entry = fixture("census.lu");
    for (row, prog) in rows(&live, &entry, "census", "census") {
        let out = live.sh(&format!("{prog}; echo rc=$?"));
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        report(&format!(
            "REPORT census.lu {row} ({}): {} || {}",
            std::env::consts::OS,
            stdout.replace('\n', " | "),
            String::from_utf8_lossy(&out.stderr).replace('\n', " | ")
        ));
        let count = |what: &str| -> i64 {
            stdout
                .lines()
                .find_map(|l| l.strip_prefix(what))
                .and_then(|n| n.trim().parse().ok())
                .unwrap_or_else(|| panic!("census.lu {row}: no `{what}` line: {stdout:?}"))
        };
        assert_eq!(
            count("census: child threads "),
            1,
            "census.lu {row}: {stdout:?}"
        );
        assert_eq!(
            count("census: the child after its first listen: "),
            2,
            "census.lu {row}: {stdout:?}"
        );
        assert_eq!(
            count("census: the child after its first park: "),
            3,
            "census.lu {row}: {stdout:?}"
        );
        assert_eq!(
            count("census: main, the hub and the set made before main, no task: "),
            3,
            "census.lu {row}: {stdout:?}"
        );
        assert!(stdout.ends_with("rc=0\n"), "census.lu {row}: {stdout:?}");
    }
}
