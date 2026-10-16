//! s242 (wolf-lang#680; ruling #79 R9's third lane) — the job wait: the
//! poll of the children (`os_wait_poll`, `[os.proc.poll]`), which never
//! blocks and reports a stop as `256 + N`, and the wait a signal cuts
//! short — the poll with `os_signal_wait` on `CHILD` and the trapped set,
//! which has no timer (`[os.signal.wait]`, `[os.signal.checked]`).
//!
//! Before this lane no machine resolved `os_wait_poll` (E0301 on the
//! checked machine, native and release; "does not resolve" on lupin),
//! the native `os_signal_wait` woke every 5 ms, and the checked machine
//! refused a blocking signal wait with no other task by name. Each
//! witness runs on the checked machine and lupin under `conform-run` and
//! as native and release binaries, and every machine's standard output
//! and standard error are printed, so a `--nocapture` log is the
//! four-machine table (with each program's and each job's SigBlk and
//! SigIgn, read on linux).
//!
//! **Nothing here signals a process the test did not start, and nothing
//! is stopped or signalled on a linux host at all.** The witnesses:
//!
//! - EVERY HOST (windows too): the poll with no child
//!   (`corpus/os/job_poll_rows.lu`) — nothing is started.
//! - LIVE: a job stopped by each of the four stop signals, continued and
//!   ended, and a group of two stopped by one send (`stops.lu`); a
//!   `wait` cut short by a trapped `TERMINATE` that a child the program
//!   started sends it (`interrupt.lu`, #680's case 1) — on linux each
//!   machine in a throwaway `--network none` container of its own, on
//!   macOS on CI's throwaway runner, where each program signals only the
//!   jobs it started and itself. The linux container runs **without
//!   `--init`**: podman's init (catatonit) starts its child with
//!   `SIGTSTP`, `SIGTTIN` and `SIGTTOU` blocked (SigBlk `0x380000`, s241),
//!   a spawn keeps the mask, and a job that blocks them is not stopped by
//!   them (measured, s242: a `SUSPEND` stayed pending). Without the init
//!   the program is the namespace's process 1 and every mask is empty,
//!   which each witness prints. The process rows also run under
//!   `taskset -c 0-3`, asked from inside (wolf-lang#570).
//! - LINUX, A CONTAINER: an idle program's context switches, every
//!   thread's, counted from outside for three seconds while it is parked
//!   in `os_signal_wait(CHILD | INTERRUPT)` (`idle.lu`) — none is
//!   expected; 0.2.30 made about 585.
//!
//! **The engine.** `WOLF_JOBWAIT_ENGINE` names it; otherwise the first of
//! `docker` and `podman` that answers. `WOLF_JOBWAIT_IMAGE` names the
//! image (default `docker.io/library/ubuntu:24.04`). Where no engine runs
//! the image the container witnesses SKIP, loudly and by name — and
//! **`WOLF_JOBWAIT_REQUIRE_CONTAINER=1` (CI's linux job) makes that a
//! failure**.
//!
//! lupin answers the poll with no child, `(-1, 0)`, and refuses it by
//! name once a child is live (`live.lu`, on the host: nothing is
//! signalled); it resolves no `os_signal_listen` at all, so the live
//! witnesses that listen for `CHILD` stop there on lupin (ruling owed R9: the standard library's
//! `try_wait` cannot see a stop, and lupin admits no `unsafe`). lupin
//! 0.1.53 (the 0.2.30 pairing) predates the mirror and is pinned here by
//! version as pre-mirror.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The lupin releases that predate the mirror, with the commit each
/// archive reports: 0.1.53 (`7896edd`, wolf 0.2.30's pairing) does not
/// resolve `os_wait_poll`.
const PRE_MIRROR_LUPIN: &[(&str, Option<&str>)] = &[("0.1.53", Some("7896edd"))];

fn pre_mirror(lupin: &Obs) -> bool {
    PRE_MIRROR_LUPIN
        .iter()
        .any(|(v, c)| *v == lupin.version && c.is_none_or(|c| lupin.commit.starts_with(c)))
}

/// What lupin (with the mirror) is expected to say of a witness.
#[derive(Clone, Copy)]
enum Lupin {
    /// Every row, as the compiler's machines.
    Mirrors,
    /// Refused by name: the verdict is `unsupported` and the reason
    /// names this call.
    Refuses(&'static str),
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    reason: String,
    version: String,
    commit: String,
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
        reason: field("x-unsupported"),
        version: field("impl_version"),
        commit: field("commit"),
    }
}

/// A fresh working directory of the test's own, holding `target/`.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("job_wait_lanes")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(d.join("target")).expect("scratch dir");
    d
}

fn run_in(mut cmd: Command, dir: &Path) -> Output {
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.output().expect("spawn")
}

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

/// The sibling lupin, or a loud skip — a failure under
/// `WOLF_PAIRING_REQUIRE_SIBLING`.
fn lupin_or_skip() -> Option<PathBuf> {
    let found = sibling_lupin();
    if found.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of s242's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    found
}

#[cfg(unix)]
fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/jobwait")
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

/// What lupin said of `name`, judged: pinned pre-mirror, mirroring, or
/// refusing by name.
fn judge_lupin(obs: &Obs, name: &str, lupin: Lupin, check: &dyn Fn(&str, &str)) {
    eprintln!(
        "== lupin {} ({}) on {name}: {} {}\n{}",
        obs.version, obs.commit, obs.verdict, obs.reason, obs.stdout
    );
    if pre_mirror(obs) {
        // 0.1.53 resolves a name when the call is reached: a witness
        // stops at `os_wait_poll` ("does not resolve"), or earlier at a
        // call it already refused by name.
        assert_eq!(
            obs.verdict, "unsupported",
            "lupin {} at {} (pre-mirror, pinned by version) on {name}: {obs:?}",
            obs.version, obs.commit
        );
        let named = match lupin {
            Lupin::Refuses(call) => obs.reason.contains(&format!("`{call}`")),
            Lupin::Mirrors => false,
        };
        assert!(
            obs.reason.contains("does not resolve") || named,
            "lupin {} (pre-mirror) refuses {name} at an unknown name or one it refused \
             before: {}",
            obs.version,
            obs.reason
        );
        return;
    }
    match lupin {
        Lupin::Mirrors => {
            assert_eq!(
                obs.verdict, "exit(0)",
                "lupin {} on {name}: {obs:?}",
                obs.version
            );
            check(&obs.stdout, &format!("lupin {} on {name}", obs.version));
        }
        Lupin::Refuses(call) => {
            assert_eq!(
                obs.verdict, "unsupported",
                "lupin {} refuses {name} by name: {obs:?}",
                obs.version
            );
            assert!(
                obs.reason.contains(call),
                "lupin {}'s refusal of {name} names `{call}`: {}",
                obs.version,
                obs.reason
            );
        }
    }
}

/// `entry` on every machine, ON THE HOST, in `dir`, with `env` added:
/// the checked machine and lupin under `conform-run`, the native and
/// release binaries run directly. `check` judges each stdout.
fn every_lane_runs_on_the_host(
    entry: &Path,
    dir: &Path,
    env: &[(&str, &str)],
    lupin: Lupin,
    check: &dyn Fn(&str, &str),
) {
    let name = entry.file_name().unwrap().to_string_lossy().to_string();
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--checked")
        .arg("--json")
        .envs(env.iter().copied());
    let out = run_in(cmd, dir);
    assert!(
        out.status.success(),
        "conform-run --checked failed on {name}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let checked = parse_obs(&out.stdout, "the observation");
    eprintln!(
        "== checked on {name} {env:?}: {}\n{}{}",
        checked.verdict,
        checked.stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        checked.verdict, "exit(0)",
        "the CHECKED lane on {name}: {checked:?}"
    );
    check(&checked.stdout, &format!("checked on {name}"));
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(entry, dir, release) else {
            continue;
        };
        let mut cmd = Command::new(&exe);
        cmd.envs(env.iter().copied());
        let out = run_in(cmd, dir);
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        eprintln!(
            "== {tier} on {name} {env:?}: exit {:?}\n{stdout}{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            out.status.code(),
            Some(0),
            "the {tier} binary on {name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        check(&stdout, &format!("{tier} on {name}"));
    }
    if let Some(lupin_bin) = lupin_or_skip() {
        let mut cmd = Command::new(&lupin_bin);
        cmd.arg("conform-run")
            .arg(entry)
            .arg("--json")
            .envs(env.iter().copied());
        let out = run_in(cmd, dir);
        let obs = parse_obs(&out.stdout, "lupin's observation");
        judge_lupin(&obs, &name, lupin, check);
    }
}

// ------------------------------------------------- every host --

/// The poll with no child, `(-1, 0)`, before and after a spawn the host
/// refused — one truth on every host, windows included. Red at 0.2.30:
/// E0301.
#[test]
fn the_poll_with_no_child_on_every_host() {
    let dir = scratch("rows");
    let entry = corpus("os/job_poll_rows.lu");
    let want = declared_stdout(&entry);
    every_lane_runs_on_the_host(&entry, &dir, &[], Lupin::Mirrors, &|got, what| {
        assert_eq!(got, want, "{what} (s242)");
    });
}

// ------------------------------------------------------- unix --

/// The poll with a live child, on the host, nothing signalled: an exit
/// of 7 is reported once and reaped. lupin answers the first poll (no
/// child) and refuses the poll by name once a child is live.
#[cfg(unix)]
#[test]
fn the_poll_with_a_live_child_on_the_host() {
    let dir = scratch("live");
    let want = "before any child: (-1, 0)\n\
a child that exits 7: reported: true, its code: 7\n\
then: (-1, 0)\n";
    every_lane_runs_on_the_host(
        &fixture("live.lu"),
        &dir,
        &[],
        Lupin::Refuses("os_wait_poll"),
        &|got, what| assert_eq!(got, want, "{what} (s242)"),
    );
}

/// `stops.lu`'s output. One row per stop signal is the host's: whether
/// a continue makes the host say `CHILD` (linux: yes; macOS: as the
/// runner answers, `CONTINUE_SAYS_CHILD`).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn stops_want() -> String {
    let said = CONTINUE_SAYS_CHILD;
    let mut out = String::from(
        "no children: (-1, 0)\n\
a job running, nothing to report: (-1, 1)\n",
    );
    for name in ["STOP", "SUSPEND", "BG_READ", "BG_WRITE"] {
        out.push_str(&format!(
            "{name} sent: ok\n\
{name}: reported for the job: true, a stop (above 255): true, by its number: true\n\
{name}: the stop said CHILD: true\n\
{name}: reported once: true\n\
{name}: CONTINUE sent: ok\n\
{name}: the continue said CHILD: {said}\n\
{name}: the continue reported: false, still live: 1\n"
        ));
    }
    out.push_str(
        "TERMINATE sent: ok\n\
ended: reported for the job: true, by -TERMINATE's number: true\n\
the end said CHILD: true\n\
then nothing, none live: (-1, 0)\n\
a wait after the poll reaped it: io\n\
a group of two stopped, both reported: true, both by STOP: true\n\
and ended, both reported: true, both by TERMINATE: true\n\
an exit: true, its code: 3\n\
nothing left: (-1, 0)\n",
    );
    out
}

/// Whether the host says `CHILD` when a stopped child is continued:
/// linux does (measured in the container, s242: `CLD_CONTINUED`); macOS
/// does not (CI's runner, run 38955877584: four continues, no `CHILD`,
/// where each stop and the end said it). A continue is never reported by
/// the poll on either host.
#[cfg(target_os = "linux")]
const CONTINUE_SAYS_CHILD: bool = true;
#[cfg(target_os = "macos")]
const CONTINUE_SAYS_CHILD: bool = false;

#[cfg(any(target_os = "linux", target_os = "macos"))]
const INTERRUPT_WANT: &str = "the wait was cut short by TERMINATE: true\n\
before the job's two seconds: true\n\
the status a shell gives: 143\n\
the job and the sender reaped: (-1, 0)\n";

// ----------------------------------- macOS, CI's throwaway runner --

/// A job stopped, continued and ended on CI's throwaway macOS runner:
/// the program signals only the jobs it started. Not compiled on linux,
/// where the same program runs in a container (below).
#[cfg(target_os = "macos")]
#[test]
fn a_job_stopped_continued_and_ended_on_the_runner() {
    let dir = scratch("stops_host");
    let want = stops_want();
    every_lane_runs_on_the_host(
        &fixture("stops.lu"),
        &dir,
        &[],
        Lupin::Refuses("os_signal_listen"),
        &|got, what| assert_eq!(got, want, "{what} (s242)"),
    );
}

/// A `wait` cut short by a trapped `TERMINATE` on CI's throwaway macOS
/// runner: the sender is a child of the program's, and the signal goes
/// to the program itself (`$PPID`).
#[cfg(target_os = "macos")]
#[test]
fn a_wait_cut_short_on_the_runner() {
    let dir = scratch("interrupt_host");
    every_lane_runs_on_the_host(
        &fixture("interrupt.lu"),
        &dir,
        &[],
        Lupin::Refuses("os_signal_listen"),
        &|got, what| assert_eq!(got, INTERRUPT_WANT, "{what} (s242)"),
    );
}

// -------------------------------------------- linux, a container --

#[cfg(target_os = "linux")]
mod container {
    use super::*;

    fn image() -> String {
        std::env::var("WOLF_JOBWAIT_IMAGE")
            .unwrap_or_else(|_| "docker.io/library/ubuntu:24.04".into())
    }

    fn required() -> bool {
        std::env::var_os("WOLF_JOBWAIT_REQUIRE_CONTAINER").is_some()
    }

    /// The engine that can run the image with no network and no init,
    /// or a loud skip — a failure under `WOLF_JOBWAIT_REQUIRE_CONTAINER`.
    /// The probe asserts the container has a pid namespace of its own:
    /// with no init its first process is process 1.
    fn engine(test: &str) -> Option<String> {
        let candidates: Vec<String> = match std::env::var("WOLF_JOBWAIT_ENGINE") {
            Ok(e) => vec![e],
            Err(_) => vec!["docker".into(), "podman".into()],
        };
        let mut why = Vec::new();
        for e in &candidates {
            let out = Command::new(e)
                .args(["run", "--rm", "--network", "none"])
                .arg(image())
                .args(["sh", "-c", "echo $$; id -u"])
                .stdin(Stdio::null())
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    let text = String::from_utf8_lossy(&o.stdout).to_string();
                    let mut lines = text.lines();
                    let pid: u32 = lines
                        .next()
                        .and_then(|l| l.trim().parse().ok())
                        .unwrap_or(u32::MAX);
                    assert_eq!(
                        pid, 1,
                        "{e}: the container's first process is {pid}, not a fresh pid \
                         namespace's process 1"
                    );
                    assert_eq!(
                        lines.next().map(str::trim),
                        Some("0"),
                        "{e}: the container's first process is its super-user"
                    );
                    return Some(e.clone());
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
            !required(),
            "WOLF_JOBWAIT_REQUIRE_CONTAINER is set and no engine ran {} — {test} did not \
             run: {why:?}",
            image()
        );
        eprintln!(
            "SKIP: {test}: no container engine ran {} ({why:?}) — the live job-wait witnesses \
             did not run on this box",
            image()
        );
        None
    }

    /// One throwaway container: no network, NO init (an init's blocked
    /// stop signals would reach the jobs, see the module doc), removed on
    /// exit, every mount read-only, its working directory the image's own
    /// `/tmp`. Never `--pid host`.
    struct Throwaway<'a> {
        engine: &'a str,
        env: &'a [(&'a str, &'a str)],
        mounts: Vec<(PathBuf, &'static str)>,
    }

    impl Throwaway<'_> {
        fn run(&self, argv: &[String]) -> Output {
            let mut cmd = Command::new(self.engine);
            cmd.args(["run", "--rm", "--network", "none"]);
            for (k, v) in self.env {
                cmd.arg("-e").arg(format!("{k}={v}"));
            }
            for (host, at) in &self.mounts {
                cmd.arg("-v").arg(format!("{}:{at}:ro", host.display()));
            }
            cmd.args(["-w", "/tmp"]).arg(image()).args(argv);
            cmd.stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            cmd.output().expect("the engine runs")
        }

        fn cpus(&self) -> usize {
            let out = self.run(&["nproc".to_string()]);
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse()
                .unwrap_or(1)
        }
    }

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    /// The machines a fixture runs on in a container: the checked one
    /// under `conform-run` (process 1 is `wolf`), and the native and
    /// release binaries built on the host (process 1 is the program),
    /// each also under `taskset -c 0-3` when the container has four cpus.
    fn machines(
        b: &Throwaway<'_>,
        entry: &Path,
        dir: &Path,
        name: &str,
        test: &str,
    ) -> Vec<(String, Vec<String>)> {
        let program = format!("/fx/{name}");
        let mut rows: Vec<(String, Vec<String>)> = vec![(
            "checked".to_string(),
            strs(&[
                "/machines/wolf",
                "conform-run",
                &program,
                "--checked",
                "--json",
            ]),
        )];
        let four = b.cpus() >= 4;
        if !four {
            eprintln!(
                "SKIP: {test}: the container has fewer than four cpus — the taskset -c 0-3 rows \
                 did not run"
            );
        }
        for release in [false, true] {
            let tier = if release { "release" } else { "native" };
            let Some(exe) = build(entry, dir, release) else {
                continue;
            };
            let inside = format!("/w/{}", exe.file_name().unwrap().to_string_lossy());
            rows.push((tier.to_string(), strs(&[&inside])));
            if four {
                rows.push((
                    format!("{tier} taskset -c 0-3"),
                    strs(&["taskset", "-c", "0-3", &inside]),
                ));
            }
        }
        rows
    }

    fn throwaway<'a>(
        engine: &'a str,
        env: &'a [(&'a str, &'a str)],
        dir: &Path,
        entry: &Path,
        lupin: Option<&PathBuf>,
    ) -> Throwaway<'a> {
        let fixtures = entry.parent().unwrap().to_path_buf();
        let mut mounts = vec![
            (dir.to_path_buf(), "/w"),
            (fixtures, "/fx"),
            (PathBuf::from(wolf()), "/machines/wolf"),
        ];
        if let Some(l) = lupin {
            mounts.push((l.clone(), "/machines/lupin"));
        }
        Throwaway {
            engine,
            env,
            mounts,
        }
    }

    /// `name` (a fixture) on every machine, each in a container of its
    /// own: `check` judges each stdout; each program's standard error
    /// (its SigBlk and SigIgn, and its jobs') is printed beside it.
    fn every_lane_runs_in_a_container(
        name: &str,
        test: &str,
        lupin: Lupin,
        check: &dyn Fn(&str, &str),
    ) {
        let Some(engine) = engine(test) else {
            return;
        };
        let entry = fixture(name);
        let dir = scratch(test);
        let lupin_bin = lupin_or_skip();
        let b = throwaway(&engine, &[], &dir, &entry, lupin_bin.as_ref());
        for (row, argv) in machines(&b, &entry, &dir, name, test) {
            let out = b.run(&argv);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if row == "checked" {
                assert!(
                    out.status.success(),
                    "conform-run --checked in the container failed on {name}: exit {:?}\n{stderr}",
                    out.status.code()
                );
                let checked = parse_obs(&out.stdout, "the observation");
                eprintln!(
                    "== checked on {name} (in {engine}, no init): {}\n{}{stderr}",
                    checked.verdict, checked.stdout
                );
                assert_eq!(
                    checked.verdict, "exit(0)",
                    "the CHECKED lane on {name}: {checked:?}"
                );
                check(&checked.stdout, &format!("checked on {name}"));
                continue;
            }
            let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
            eprintln!(
                "== {row} on {name} (in {engine}, no init): exit {:?}\n{stdout}{stderr}",
                out.status.code()
            );
            assert_eq!(
                out.status.code(),
                Some(0),
                "the {row} binary on {name}, in the container: {stderr}"
            );
            check(&stdout, &format!("{row} on {name}"));
        }
        if lupin_bin.is_some() {
            let program = format!("/fx/{name}");
            let out = b.run(&strs(&[
                "/machines/lupin",
                "conform-run",
                &program,
                "--json",
            ]));
            let obs = parse_obs(&out.stdout, "lupin's observation");
            judge_lupin(&obs, name, lupin, check);
        }
    }

    /// A job stopped by `STOP`, `SUSPEND`, `BG_READ` and `BG_WRITE`, each
    /// reported once as `256 + N` and continued (not reported), then ended
    /// and reaped; a group of two stopped by one send; an exit's code.
    #[test]
    fn a_job_stopped_continued_and_ended_in_a_container() {
        let want = stops_want();
        every_lane_runs_in_a_container(
            "stops.lu",
            "stops",
            Lupin::Refuses("os_signal_listen"),
            &|got, what| assert_eq!(got, want, "{what} (s242)"),
        );
    }

    /// #680's case 1: a `wait` cut short by a trapped `TERMINATE`.
    #[test]
    fn a_wait_cut_short_in_a_container() {
        every_lane_runs_in_a_container(
            "interrupt.lu",
            "interrupt",
            Lupin::Refuses("os_signal_listen"),
            &|got, what| assert_eq!(got, INTERRUPT_WANT, "{what} (s242)"),
        );
    }

    /// The most context switches an idle program may make, every thread
    /// summed, in the three seconds counted: none is expected (the
    /// measurement on kasumi was 0 on every machine); 0.2.30's native
    /// wait made about 585.
    const IDLE_MOST: u64 = 5;

    /// An idle program parked in `os_signal_wait(CHILD | INTERRUPT)`:
    /// its context switches, every thread's, counted from outside for
    /// three seconds; then the job ends, `CHILD` wakes it and the poll
    /// answers. The checked machine at 0.2.30 refused this wait by name.
    #[test]
    fn an_idle_wait_makes_no_context_switch_in_a_container() {
        let test = "idle";
        let Some(engine) = engine(test) else {
            return;
        };
        let entry = fixture("idle.lu");
        let dir = scratch(test);
        let env = [("S242_IDLE_SECS", "6")];
        let b = throwaway(&engine, &env, &dir, &entry, None);
        // The program in the background of a `sh` that counts its
        // threads' switches from /proc for three seconds once it has
        // parked, then waits for it.
        let counter = "\"$@\" > /tmp/out 2> /tmp/err & p=$!; sleep 1.5; \
             sum() { cat /proc/$p/task/*/status 2>/dev/null | \
             awk '/ctxt_switches/ {s+=$2} END {print s+0}'; }; \
             grep -E 'SigBlk|SigIgn' /proc/$p/status; \
             echo threads=$(ls /proc/$p/task | wc -l); \
             a=$(sum); sleep 3; b=$(sum); echo switches=$((b-a)); \
             wait $p; echo rc=$?; cat /tmp/out /tmp/err";
        for (row, argv) in machines(&b, &entry, &dir, "idle.lu", test) {
            let mut full = strs(&["sh", "-c", counter, "sh"]);
            full.extend(argv);
            let out = b.run(&full);
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            eprintln!(
                "== {row} on idle.lu (in {engine}, no init):\n{text}{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let switches: u64 = text
                .lines()
                .find_map(|l| l.strip_prefix("switches="))
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or_else(|| panic!("{row}: the count was read: {text}"));
            assert!(
                switches <= IDLE_MOST,
                "{row}: an idle wait made {switches} context switches in three seconds \
                 (at most {IDLE_MOST}; s242)"
            );
            assert!(
                text.contains("rc=0"),
                "{row}: the program ended well: {text}"
            );
            assert!(
                text.contains("woke by CHILD: true; the job: true, its status: 0"),
                "{row}: CHILD woke it and the poll answered: {text}"
            );
        }
    }
}
