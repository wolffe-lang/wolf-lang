//! s246 (wolf-lang#781, #783, #784, #785, #787) — the signal gaps 0.2.32
//! left: a descriptor wait that made a descriptor while the program ran,
//! no way to ignore a send-only meaning or a host number, no way to end
//! by a fault signal, no core in a child's status, and no words for
//! signal 0. `[os.signal.wait.fd]`, `[os.signal.disp.one]`,
//! `[os.signal.exit]`, `[os.proc.status]`, `[os.proc.poll]`,
//! `[os.signal.text]`.
//!
//! Each witness runs on the checked machine and lupin under `conform-run`
//! and as native and release binaries where the call is theirs, and
//! every machine's standard output and standard error are printed, so a
//! `--nocapture` log is the table (with each live program's SigBlk and
//! SigIgn, read on linux before it sends anything).
//!
//! **Nothing here signals a process the test did not start, and nothing
//! is signalled, faulted or dumped on a linux host at all.** The
//! witnesses are three kinds:
//!
//! - EVERY HOST (windows too): the shape rows of the three new calls
//!   (`corpus/os/signal_one_rows.lu`) and signal 0's words
//!   (`text_zero.lu`) — nothing is sent or changed.
//! - ON A UNIX HOST: the descriptor wait beside `fs_fstat(1)` with 1
//!   closed (`race781.lu`) — nothing is sent.
//! - LIVE: a group send of a fault the sender ignores (`group_fault.lu`),
//!   a program that ends itself by a signal or is sent one
//!   (`parent.lu` starting `exit_by.lu`), and a child that dumps core
//!   (`core.lu`) — on linux each machine in a throwaway `--network none`
//!   container of its own, NO init (the shell is process 1, so the
//!   programs are not: a namespace's process 1 is not ended by a signal
//!   it has no handler for), the test's own directory its working
//!   directory, `ulimit -c 0` unless the row is the core's; on macOS on
//!   CI's throwaway runner only (`GITHUB_ACTIONS`), `ulimit -c 0`, never
//!   the cores-on row. The process rows also run under `taskset -c 0-3`,
//!   asked from inside (wolf-lang#570).
//!
//! **Cores.** A core is written only where the container's working
//! directory is, the test's own scratch directory, and removed there. A
//! host whose `kernel.core_pattern` pipes cores to a program (systemd's
//! `systemd-coredump` on kasumi and hasu) would write the cores-on row's
//! core OUTSIDE it, so that row SKIPs there, by name; such a host pipes a
//! core even under `ulimit -c 0` (core(5): RLIMIT_CORE is not read for a
//! pipe; measured on kasumi, s246, `(core dumped)` with `ulimit -c 0`,
//! and the handler stored nothing), so the cores-off row expects one
//! there. CI's linux job sets a file pattern, `core.%p`, on its throwaway
//! runner and **`WOLF_CORE_REQUIRE=1` makes the cores-on row's skip a
//! failure**.
//!
//! **The engine.** `WOLF_JOBWAIT_ENGINE` names it (s242's), otherwise the
//! first of `docker` and `podman` that answers; `WOLF_JOBWAIT_IMAGE` the
//! image (default `docker.io/library/ubuntu:24.04`). Where no engine runs
//! it the container witnesses SKIP, loudly and by name — and
//! **`WOLF_JOBWAIT_REQUIRE_CONTAINER=1` (CI's linux job) makes that a
//! failure**.
//!
//! **lupin 0.1.54**, the pairing, predates every new answer: the three
//! new names do not resolve, `os_signal_text(0)` is `invalid` (its old
//! shape row), and `os_wait_status` answers `-N` for a child that dumped
//! core. Each is pinned here by version, exactly as measured; a newer
//! lupin is held to the mirror (wolf-interp issues listed in the PR).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A `REPORT` line for the CI log even when the test passes: the
/// process's own standard error, past libtest's capture (which takes
/// only the print macros).
fn report(line: &str) {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

/// The pairing at this lane, which predates the mirror of every answer
/// s246 adds (each pinned below by what it was measured to say).
const PRE_MIRROR_LUPIN: &str = "0.1.54";

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
        .join("tests/fixtures/signal_gaps")
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
        .join("signal_gaps_lanes")
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
             (LUPIN={}) — the oracle leg of s246's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    found
}

/// What lupin is held to for one witness: 0.1.54's measured answer, and
/// the mirror's for anything newer.
enum Lupin<'a> {
    /// 0.1.54 exits 0 with this stdout; a newer lupin with the compiler's.
    Answers(&'a str),
    /// 0.1.54 stops at a name it does not resolve; a newer lupin mirrors
    /// the compiler (the shape rows).
    Unresolved(&'a str),
    /// Every lupin refuses by name: the reason names this call.
    Refuses(&'a str),
    /// 0.1.54 exits 0 with this stdout; a newer lupin refuses by name.
    AnswersThenRefuses(&'a str, &'a str),
    /// lupin's poll (ruling #84 R9): it refuses by name while a child is
    /// live, and answers once the child it asks of has ended — which a
    /// poll loop meets depends on timing, so either is held: 0.1.54's
    /// answer is this stdout (a newer lupin's the compiler's), its
    /// refusal names this call.
    AnswersOrRefuses(&'a str, &'a str),
    /// Every lupin refuses by name; the reason names one of these calls.
    RefusesOneOf(&'a [&'a str]),
}

fn judge_lupin(obs: &Obs, name: &str, want: &str, lupin: &Lupin<'_>) {
    eprintln!(
        "== lupin {} on {name}: {} {}\n{}",
        obs.version, obs.verdict, obs.reason, obs.stdout
    );
    let pre = obs.version == PRE_MIRROR_LUPIN;
    let exits = |stdout: &str| {
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", stdout),
            "lupin {} on {name}: {obs:?}",
            obs.version
        );
    };
    let refuses = |call: &str| {
        assert_eq!(
            obs.verdict, "unsupported",
            "lupin {} refuses {name} by name: {obs:?}",
            obs.version
        );
        assert!(
            obs.reason.contains(&format!("`{call}`")),
            "lupin {}'s refusal of {name} names `{call}`: {}",
            obs.version,
            obs.reason
        );
    };
    match *lupin {
        Lupin::Answers(old) => exits(if pre { old } else { want }),
        Lupin::Unresolved(call) => {
            if pre {
                refuses(call);
                assert!(
                    obs.reason.contains("does not resolve"),
                    "lupin {} (pre-mirror) does not resolve `{call}`: {}",
                    obs.version,
                    obs.reason
                );
            } else {
                exits(want);
            }
        }
        Lupin::Refuses(call) => refuses(call),
        Lupin::AnswersThenRefuses(old, call) => {
            if pre {
                exits(old);
            } else {
                refuses(call);
            }
        }
        Lupin::RefusesOneOf(calls) => {
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
        }
        Lupin::AnswersOrRefuses(old, call) => {
            if obs.verdict == "unsupported" {
                refuses(call);
            } else {
                exits(if pre { old } else { want });
            }
        }
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
        "== checked on {name}: {}\n{}{}",
        checked.verdict,
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

/// The C library's words for signal 0, read by the test itself (the
/// test binary never calls `setlocale`, so they are the C locale's).
#[cfg(unix)]
fn strsignal_zero() -> String {
    // SAFETY: strsignal answers a string it owns; copied out at once.
    unsafe {
        let p = libc::strsignal(0);
        assert!(!p.is_null(), "strsignal(0) answered no string");
        std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

/// #787: `os_signal_text(0)` is the C library's `strsignal(0)` — glibc's
/// `Unknown signal 0`, what GNU 9.11's `kill -t 0` and `kill -L` print for
/// row 0 (`EXIT`), measured on kasumi (s246) — and `unsupported` on
/// windows. Red at 0.2.32: `text_zero=invalid` on every machine.
#[test]
fn signal_zero_has_words_on_every_host() {
    let dir = scratch("text_zero");
    #[cfg(unix)]
    let want = {
        let words = strsignal_zero();
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        assert_eq!(
            words, "Unknown signal 0",
            "glibc's words for 0 (GNU kill -t 0)"
        );
        report(&format!("REPORT strsignal(0) on this host: {words:?}"));
        format!("text_zero={words}\n")
    };
    #[cfg(not(unix))]
    let want = "text_zero=unsupported\n".to_string();
    every_lane_answers_on_the_host(
        &fixture("text_zero.lu"),
        &dir,
        &want,
        if cfg!(unix) {
            Lupin::AnswersThenRefuses("text_zero=invalid\n", "os_signal_text")
        } else {
            Lupin::Answers("text_zero=invalid\n")
        },
    );
}

/// #783, #784: the shape rows of `os_signal_ignore_one`,
/// `os_signal_default_one` and `os_exit_signal`, decided before any host
/// is asked — one truth on every host, windows included. Red at 0.2.32:
/// E0301 (no name resolves).
#[test]
fn the_one_meaning_shape_rows_on_every_host() {
    let dir = scratch("rows");
    let entry = corpus("os/signal_one_rows.lu");
    let want = declared_stdout(&entry);
    every_lane_answers_on_the_host(
        &entry,
        &dir,
        &want,
        Lupin::Unresolved("os_signal_ignore_one"),
    );
}

// ------------------------------------------------------------ unix --

/// `exe out` started with 0 on `/dev/null` and 1 closed (in the child,
/// between fork and exec), optionally under `taskset -c 0-3`.
#[cfg(unix)]
fn run_with_one_closed(exe: &Path, out: &Path, taskset: bool) -> Output {
    use std::os::unix::process::CommandExt as _;
    let mut cmd = if taskset {
        let mut c = Command::new("taskset");
        c.args(["-c", "0-3"]).arg(exe);
        c
    } else {
        Command::new(exe)
    };
    cmd.arg(out)
        .stdin(std::fs::File::open("/dev/null").expect("/dev/null opens"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: between fork and exec the closure makes one system call.
    unsafe {
        cmd.pre_exec(|| {
            libc::close(1);
            Ok(())
        });
    }
    cmd.output().expect("the program runs")
}

/// Whether `taskset` runs here (linux; wolf-lang#570's four-cpu rows).
#[cfg(unix)]
fn taskset_runs() -> bool {
    cfg!(target_os = "linux")
        && Command::new("taskset")
            .args(["-c", "0", "true"])
            .output()
            .is_ok_and(|o| o.status.success())
}

/// #781: a descriptor wait makes no descriptor while the program runs, so
/// a closed 1 stays closed to another task: two seconds of
/// `os_signal_wait_fd(0, INTERRUPT)` on one task (descriptor 0 at its
/// end, so each wait answers 0 at once) beside `fs_fstat(1)` on main,
/// both tiers, all cpus and under `taskset -c 0-3`. Red at 0.2.32:
/// `fs_fstat(1)` answered ok 760,739 times of 13,607,478 (kasumi), the
/// wake pipe each wait made sitting on the closed 1 until the runtime
/// moved it.
#[cfg(unix)]
#[test]
fn a_descriptor_wait_makes_no_descriptor_while_the_program_runs() {
    let dir = scratch("race781");
    let entry = fixture("race781.lu");
    let four = taskset_runs();
    if !four && cfg!(target_os = "linux") {
        eprintln!("SKIP: no taskset here — the taskset -c 0-3 rows did not run");
    }
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, &dir, "race", release) else {
            continue;
        };
        for taskset in [false, true] {
            if taskset && !four {
                continue;
            }
            let out_file = dir.join(format!("out-{tier}-{taskset}"));
            let run = run_with_one_closed(&exe, &out_file, taskset);
            let text = std::fs::read_to_string(&out_file).unwrap_or_default();
            report(&format!(
                "REPORT race781 {tier}{}: exit {:?}: {}{}",
                if taskset { " (taskset -c 0-3)" } else { "" },
                run.status.code(),
                text.trim(),
                String::from_utf8_lossy(&run.stderr)
            ));
            assert_eq!(run.status.code(), Some(0), "race781 on {tier}: {text}");
            let num = |key: &str| -> u64 {
                text.split(key)
                    .nth(1)
                    .and_then(|r| {
                        r.split(|c: char| !c.is_ascii_digit())
                            .find(|w| !w.is_empty())
                    })
                    .and_then(|w| w.parse().ok())
                    .unwrap_or_else(|| panic!("`{key}` in the report: {text}"))
            };
            let (waits, ready, asks, ok) =
                (num("waits "), num("ready "), num("asked "), num("ok "));
            assert!(
                waits >= 1000 && ready == waits,
                "race781 on {tier}: the waits ran and each answered the input ({text})"
            );
            assert!(
                asks >= 1000,
                "race781 on {tier}: fstat(1) was asked ({text})"
            );
            assert_eq!(
                ok, 0,
                "#781: on {tier} a descriptor wait put a descriptor on the closed 1 while \
                 another task looked ({text})"
            );
        }
    }
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

    /// The `wolf` binary and the fixtures' directory, as the program sees
    /// them.
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

    /// Write each `(name, value)` as `s246.<name>` in `sub` of the test's
    /// directory ("" for itself), where the fixtures read their settings.
    fn conf(&self, sub: &str, conf: &[(&str, &str)]) {
        let d = self.dir.join(sub);
        std::fs::create_dir_all(&d).expect("the settings' directory");
        for (k, v) in conf {
            std::fs::write(d.join(format!("s246.{k}")), v).expect("a setting is written");
        }
    }

    /// `script` under `sh -c`, with `env` set, in the test's directory:
    /// in a throwaway container (no network, NO init, the test's
    /// directory at `/w` read-write and its working directory, the
    /// fixtures and machines read-only), or on the runner.
    fn sh(&self, env: &[(&str, &str)], script: &str) -> Output {
        let mut cmd = match &self.engine {
            Some(e) => {
                let mut c = Command::new(e);
                c.args(["run", "--rm", "--network", "none"]);
                for (k, v) in env {
                    c.arg("-e").arg(format!("{k}={v}"));
                }
                c.arg("-v").arg(format!("{}:/w", self.dir.display()));
                c.arg("-v").arg(format!(
                    "{}:/fx:ro",
                    fixture("core.lu").parent().unwrap().display()
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
                for (k, v) in env {
                    c.env(k, v);
                }
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
        let out = self.sh(&[], "nproc");
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
/// and whether it runs under `taskset -c 0-3`.
#[cfg(unix)]
fn compiled_rows(live: &Live, entry: &Path, stem: &str, test: &str) -> Vec<(String, String)> {
    let four = live.four_cpus();
    if !four && cfg!(target_os = "linux") {
        eprintln!("SKIP: {test}: fewer than four cpus — the taskset -c 0-3 rows did not run");
    }
    let mut rows = Vec::new();
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(entry, &live.dir, stem, release) else {
            continue;
        };
        let name = exe.file_name().unwrap().to_string_lossy().to_string();
        rows.push((tier.to_string(), live.at(&name)));
        if four {
            rows.push((
                format!("{tier} taskset -c 0-3"),
                format!("taskset -c 0-3 {}", live.at(&name)),
            ));
        }
    }
    rows
}

/// The checked machine's (or lupin's) record of `fixture` in the place,
/// with `conf` written beside it first.
#[cfg(unix)]
fn record_in(live: &Live, machine: &str, flags: &str, name: &str, conf: &[(&str, &str)]) -> Obs {
    live.conf("", conf);
    let out = live.sh(
        &[],
        &format!(
            "ulimit -c 0; {machine} conform-run {} {flags} --json",
            live.fixture_at(name)
        ),
    );
    let obs = parse_obs(&out.stdout, &format!("{machine} on {name}"));
    eprintln!(
        "== {machine} on {name} {conf:?}: {} {}\n{}{}",
        obs.verdict,
        obs.reason,
        obs.stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    obs
}

/// #783: a program that leads a group ignores `SEGMENT_FAULT`, then a
/// host number (linux: the C library's first real-time signal), sends
/// each to its whole group — itself and a `sleep` it started — and lives,
/// while the `sleep` ends by it; then `os_signal_default_one` sets the
/// default back. The checked machine refuses the fault meaning by name
/// (the `wolf` process's own stack guard holds it); lupin refuses at
/// `os_set_pgid`, by name, before it. Red at 0.2.32: E0301, no
/// `os_signal_ignore_one`.
#[cfg(unix)]
#[test]
fn a_group_send_of_a_fault_the_sender_ignores() {
    let test = "group_fault";
    let Some(live) = Live::new(test) else {
        return;
    };
    let entry = fixture("group_fault.lu");
    let rt = if cfg!(target_os = "linux") { "34" } else { "0" };
    let mut want = "segment_fault: the job ended by signal 11, this program lived\n".to_string();
    if rt != "0" {
        want += &format!("host {rt}: the job ended by signal {rt}, this program lived\n");
    }
    want += "segment_fault: the default again\n";
    live.conf("", &[("rt", rt)]);
    for (row, prog) in compiled_rows(&live, &entry, "group", test) {
        let out = live.sh(&[], &format!("ulimit -c 0; {prog}; echo rc=$?"));
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        report(&format!(
            "REPORT group_fault.lu {row}: {} || {}",
            stdout.replace('\n', " | "),
            String::from_utf8_lossy(&out.stderr).replace('\n', " | ")
        ));
        assert_eq!(
            stdout,
            format!("{want}rc=0\n"),
            "#783: the {row} binary lives through its own group's fault and host signal"
        );
    }
    let checked = record_in(
        &live,
        &live.wolf_at(),
        "--checked",
        "group_fault.lu",
        &[("rt", rt)],
    );
    assert_eq!(
        checked.verdict, "unsupported",
        "the checked machine refuses a fault meaning's disposition by name: {checked:?}"
    );
    assert!(
        checked.reason.contains("`os_signal_ignore_one`"),
        "the checked machine names the call: {}",
        checked.reason
    );
    if let Some(l) = live.lupin_at() {
        let obs = record_in(&live, &l, "", "group_fault.lu", &[("rt", rt)]);
        judge_lupin(
            &obs,
            "group_fault.lu",
            &want,
            &Lupin::Refuses("os_set_pgid"),
        );
    }
}

/// What `parent.lu` prints for each of `exit_by.lu`'s modes on this host.
/// Its waits are the released `os_wait_status` and `os_wait_poll`, which
/// answer `-N` whether or not the host wrote a core (ruling #88 R3 D), so
/// a SEGV is `signal 11` on every host, a pipe pattern's included.
#[cfg(unix)]
fn exit_by_want() -> (String, String) {
    let segv = "signal 11";
    let child = if cfg!(target_os = "linux") { 17 } else { 20 };
    let mut modes = vec!["exit-segv".to_string()];
    let mut want = format!("exit-segv: {segv}\n");
    if cfg!(target_os = "linux") {
        modes.push("exit-rt:34".into());
        want += "exit-rt: signal 34\n";
    }
    modes.extend(
        [
            "exit-term-listened",
            "exit-child",
            "sent-segv",
            "ignored-sent-segv",
            "overflow-ignored",
        ]
        .map(String::from),
    );
    want += &format!(
        "exit-term-listened: signal 15\nexit-child: exit {}\nsent-segv: {segv}\n\
         ignored-sent-segv: exit 0\noverflow-ignored: exit 134\n",
        128 + child
    );
    (modes.join(" "), want)
}

/// #784: a program ends itself by a signal — `os_exit_signal` of
/// `SEGMENT_FAULT` (the runtime's stack guard lets go of it first), of a
/// host number, of a listened `TERMINATE`; of `CHILD`, whose default does
/// nothing, it exits with 128 + N — and a SEGV it is SENT follows its own
/// disposition: the default ends it, an ignore keeps it alive, and a REAL
/// fault (a task's stack overflow) is still the runtime's report and exit
/// 134 under that ignore. The children are the native and the release
/// build of `exit_by.lu`; the parent, which reports each end through
/// `os_wait_status` and through `os_wait_poll`, runs on every machine.
/// Red at 0.2.32: E0301 (`os_exit_signal`), and `sent-segv` lived.
#[cfg(unix)]
#[test]
fn a_program_ends_itself_by_a_signal() {
    let test = "exit_by";
    let Some(live) = Live::new(test) else {
        return;
    };
    let child = fixture("exit_by.lu");
    let parent = fixture("parent.lu");
    let (modes, want) = exit_by_want();
    for release in [false, true] {
        let Some(c) = build(&child, &live.dir, "child", release) else {
            continue;
        };
        let child_at = live.at(&c.file_name().unwrap().to_string_lossy());
        for how in ["wait", "poll"] {
            let env = [
                ("child", child_at.as_str()),
                ("how", how),
                ("modes", modes.as_str()),
            ];
            live.conf("", &env);
            for (row, prog) in compiled_rows(&live, &parent, "parent", test) {
                let out = live.sh(&[], &format!("ulimit -c 0; {prog}; echo rc=$?"));
                let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
                let stderr = String::from_utf8_lossy(&out.stderr);
                report(&format!(
                    "REPORT parent.lu {row} ({how}) of {child_at}: {} || {}",
                    stdout.replace('\n', " | "),
                    stderr.replace('\n', " | ")
                ));
                assert_eq!(
                    stdout,
                    format!("{want}rc=0\n"),
                    "#784: the {row} parent ({how}) of {child_at}"
                );
                assert!(
                    stderr.contains("wolf-rt: stack overflow in task"),
                    "a real fault under the ignore is still the runtime's report: {stderr}"
                );
                assert_eq!(
                    stderr.matches("lived").count(),
                    1,
                    "only the ignored sent SEGV lives to write `lived`: {stderr}"
                );
            }
            let checked = record_in(&live, &live.wolf_at(), "--checked", "parent.lu", &env);
            assert_eq!(
                (checked.verdict.as_str(), checked.stdout.as_str()),
                ("exit(0)", want.as_str()),
                "the CHECKED machine as the parent ({how}) of {child_at}"
            );
            if let Some(l) = live.lupin_at() {
                let obs = record_in(&live, &l, "", "parent.lu", &env);
                if how == "wait" {
                    judge_lupin(&obs, "parent.lu (wait)", &want, &Lupin::Answers(&want));
                } else {
                    judge_lupin(
                        &obs,
                        "parent.lu (poll)",
                        &want,
                        &Lupin::AnswersOrRefuses(&want, "os_wait_poll"),
                    );
                }
            }
        }
    }
    // The checked machine as the CHILD: it does not end the `wolf`
    // process by a signal, and says so by name (the mode through the
    // environment: `conform-run` passes no argument).
    let obs = record_in(
        &live,
        &live.wolf_at(),
        "--checked",
        "exit_by.lu",
        &[("mode", "exit-segv")],
    );
    assert_eq!(
        obs.verdict, "unsupported",
        "the checked machine refuses os_exit_signal by name: {obs:?}"
    );
    assert!(
        obs.reason.contains("`os_exit_signal`"),
        "the checked machine names the call: {}",
        obs.reason
    );
}

/// #785 (ruling #88 R3 D): a child that dumps core reads `-N` from the
/// released `os_wait_status` and `os_wait_poll`, as it always did, and
/// `-(256 + N)` from their siblings `os_wait_status_core` and
/// `os_wait_poll_core`; one that dumps none (`TERM`) `-N` from all four —
/// on every compiled machine and the checked one. lupin 0.1.54 resolves
/// neither sibling. Cores on (`ulimit -c unlimited`, two core files written
/// in the test's own directory, found and removed there) only where the
/// host's pattern is a file; cores off everywhere (`ulimit -c 0`), which a
/// pipe pattern dumps anyway. Red at 0.2.32: E0301, no sibling; red with
/// the siblings blind to the core: `segv core -11` with the core written.
#[cfg(unix)]
#[test]
fn a_core_rides_the_status() {
    let test = "core";
    let Some(live) = Live::new(test) else {
        return;
    };
    let entry = fixture("core.lu");
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
    let rows = compiled_rows(&live, &entry, "core", test);
    for (leg, limit, core) in legs {
        let segv = if core { -267 } else { -11 };
        let want = format!("segv -11\nterm -15\nsegv core {segv}\nterm core -15\n");
        for how in ["wait", "poll"] {
            let env = [("how", how)];
            for (row, prog) in &rows {
                let sub = format!("{leg}-{how}-{}", row.replace(' ', "_"));
                live.conf(&sub, &env);
                let out = live.sh(
                    &[],
                    &format!(
                        "{limit}; cd {} && {prog}; echo rc=$?; ls | grep -c '^core' ; rm -f core*",
                        live.at(&sub)
                    ),
                );
                let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
                report(&format!(
                    "REPORT core.lu {row} (cores {leg}, {how}): {} || {}",
                    stdout.replace('\n', " | "),
                    String::from_utf8_lossy(&out.stderr).replace('\n', " | ")
                ));
                let cores_here = if core && !piped { 2 } else { 0 };
                assert_eq!(
                    stdout,
                    format!("{want}rc=0\n{cores_here}\n"),
                    "#785: the {row} binary, cores {leg}, through {how}"
                );
            }
            if leg == "off" {
                let checked = record_in(&live, &live.wolf_at(), "--checked", "core.lu", &env);
                assert_eq!(
                    (checked.verdict.as_str(), checked.stdout.as_str()),
                    ("exit(0)", want.as_str()),
                    "the CHECKED machine, cores {leg}, through {how}"
                );
                if let Some(l) = live.lupin_at() {
                    let obs = record_in(&live, &l, "", "core.lu", &env);
                    if how == "wait" {
                        judge_lupin(
                            &obs,
                            "core.lu (wait)",
                            &want,
                            &Lupin::Unresolved("os_wait_status_core"),
                        );
                    } else {
                        // lupin's poll refuses while a child is live (R9 of
                        // ruling #84) and 0.1.54 does not resolve the
                        // sibling: which it meets first is timing.
                        judge_lupin(
                            &obs,
                            "core.lu (poll)",
                            &want,
                            &Lupin::RefusesOneOf(&["os_wait_poll", "os_wait_poll_core"]),
                        );
                    }
                }
            }
        }
    }
}
