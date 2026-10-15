//! s241 (wolf-lang#739, #740, #741, #762; ruling #79) — a signal to a
//! process group and to every process, this process into a group, the
//! host's own signal numbers and a signal's words, on every machine.
//! `[os.signal.send.group]`, `[os.signal.send.all]`, `[os.term.group]`,
//! `[os.signal.host]`, `[os.signal.text]`.
//!
//! Before this lane no machine resolved a name for any of it (E0301 on
//! the checked machine, native and release; "does not resolve" on
//! lupin). Each witness runs on the checked machine and lupin under
//! `conform-run` and as native and release binaries, and every machine's
//! standard output and standard error are printed, so a `--nocapture`
//! log is the four-machine table (and each program's SigBlk and SigIgn,
//! read at its start on linux, is in it).
//!
//! **Nothing here signals a process on a host it did not start, and
//! nothing signals or moves anything on a linux host at all.** The
//! witnesses are three kinds:
//!
//! - EVERY HOST (windows too): the shape rows (`corpus/os/group_rows.lu`)
//!   — nothing is sent or moved.
//! - ON A UNIX HOST: the host's signal table read (`table.lu`), held to
//!   s240's measured tables — nothing is sent.
//! - LIVE: the group send (`job.lu`) and the move into a group with
//!   `kill 0` (`pgid.lu`) — on linux each machine in a throwaway
//!   `--network none --init` container of its own (podman on kasumi,
//!   docker on CI), with a pid namespace of its own, every mount
//!   read-only; on macOS on CI's throwaway runner, where each program
//!   signals only the groups it started and its own group once it leads
//!   it. `send_all` (`all.lu`) runs ONLY in a container, as the
//!   super-user and as user 1000 beside a super-user's process, and the
//!   program itself refuses to start where its process id is not a fresh
//!   namespace's. The process-spawning rows also run under
//!   `taskset -c 0-3`, asked from INSIDE the container (wolf-lang#570).
//!
//! **The engine.** `WOLF_GROUP_ENGINE` names it; otherwise the first of
//! `docker` and `podman` that answers. `WOLF_GROUP_IMAGE` names the image
//! (default `docker.io/library/ubuntu:24.04`): it supplies a C library
//! the host's binaries run on, `sh`, `sleep`, `setsid`, `setpriv` and
//! `taskset`, and nothing of wolf's. Where no engine runs the image the
//! container witnesses SKIP, loudly and by name — and
//! **`WOLF_GROUP_REQUIRE_CONTAINER=1` (CI's linux job) makes that a
//! failure**.
//!
//! lupin answers the shape rows and refuses each well-formed call by name
//! (ruling #79 R8: std has no `kill(-pgid)`, no `setpgid` on itself and
//! no `strsignal`, and lupin admits no `unsafe`). lupin 0.1.53 (the
//! 0.2.30 pairing) predates the mirror — it resolves none of the six
//! names — and is pinned here by version as pre-mirror.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The lupin releases that predate the mirror, with the commit each
/// archive reports: 0.1.53 (`7896edd`, wolf 0.2.30's pairing) resolves
/// none of the six names ("`os_signal_send_group` does not resolve").
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
        .join("group_lanes")
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
             (LUPIN={}) — the oracle leg of s241's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    found
}

#[cfg(unix)]
fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/group")
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
        // stops at the first s241 name ("does not resolve"), or earlier
        // at a call it already refused by name (`os_pgid`, as the mirror
        // does).
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
            "lupin {} (pre-mirror) refuses {name} at an unknown name or the one it refused \
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

/// The shape rows, decided before any host is asked — one truth on every
/// host, windows included; nothing is sent or moved. Red at 0.2.30:
/// E0301 (no name resolves).
#[test]
fn the_shape_rows_on_every_host() {
    let dir = scratch("rows");
    let entry = corpus("os/group_rows.lu");
    let want = declared_stdout(&entry);
    every_lane_runs_on_the_host(&entry, &dir, &[], Lupin::Mirrors, &|got, what| {
        assert_eq!(got, want, "{what} (s241)");
    });
}

// ------------------------------------------------------- unix --

/// s240's measured table for this host's C library: the words for
/// 1..=last, one `N words` line each. linux under glibc
/// (`docs/s240/ev/linux/sigtab-glibc-C.txt`, sha256 3f9883723ac7…, on
/// Ubuntu 24.04's glibc 2.39) and macOS (`sigtab-macos-C.txt`, sha256
/// 217e600c7db4…, CI's runner, macOS 26.6.2); `None` for a C library
/// s240 did not measure.
#[cfg(unix)]
fn measured_table() -> Option<String> {
    let file = if cfg!(all(target_os = "linux", target_env = "gnu")) {
        "strsignal-glibc.txt"
    } else if cfg!(target_os = "macos") {
        "strsignal-macos.txt"
    } else {
        return None;
    };
    Some(std::fs::read_to_string(fixture(file)).expect("the measured table reads"))
}

/// The host's table read on every machine: its last number and
/// real-time range as s240 measured them, the words of every host number
/// 1..=last line for line as s240's table, each of the 28 meanings'
/// words those of its own number — and the same bytes with `LANG` and
/// `LC_ALL` naming a German locale (the C locale either way; glibc
/// translates only after `setlocale`, which wolf never calls, and the
/// read sets the C locale on its own thread besides). lupin refuses
/// `os_signal_last` by name. Red at 0.2.30: E0301.
#[cfg(unix)]
#[test]
fn the_hosts_table_and_its_words() {
    let dir = scratch("table");
    let head = if cfg!(target_os = "linux") {
        "last=64 realtime=34..64\n"
    } else {
        "last=31 realtime=0..0\n"
    };
    let table = measured_table();
    if table.is_none() {
        eprintln!("SKIP: no measured table for this C library — the numbered lines are not held");
    }
    let check = |got: &str, what: &str| {
        let mut lines = got.lines();
        let first = lines.next().unwrap_or("");
        assert_eq!(format!("{first}\n"), head, "{what} (s241)");
        let numbered: Vec<&str> = got
            .lines()
            .skip(1)
            .filter(|l| !l.starts_with("each "))
            .collect();
        if let Some(t) = &table {
            let want: Vec<&str> = t.lines().collect();
            assert_eq!(
                numbered, want,
                "{what}: the words, against s240's table (s241)"
            );
        }
        assert!(
            got.ends_with("each meaning's words are its number's: true\n"),
            "{what} (s241): {got}"
        );
    };
    every_lane_runs_on_the_host(
        &fixture("table.lu"),
        &dir,
        &[],
        Lupin::Refuses("os_signal_last"),
        &check,
    );
    every_lane_runs_on_the_host(
        &fixture("table.lu"),
        &dir,
        &[("LANG", "de_DE.UTF-8"), ("LC_ALL", "de_DE.UTF-8")],
        Lupin::Refuses("os_signal_last"),
        &check,
    );
}

/// `job.lu`'s output, the one host-dependent row named: a group whose
/// one member has ended unreaped is `ok` on linux and `denied` on macOS
/// (`EPERM`; s240 §A; ruling #79 R3), asked by the member's parent and by
/// another process alike (macOS measured in run 38801738189).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn job_want() -> String {
    let (parent, other) = if cfg!(target_os = "macos") {
        ("denied", "refused")
    } else {
        ("ok", "ok")
    };
    format!(
        "a job of three, started\n\
signal 0 to the group: ok\n\
TERMINATE to the group: ok\n\
the leader ended by: -15\n\
the job's own sleeps, which this program never signalled: not_found not_found\n\
the group, once its members are gone: not_found\n\
TERMINATE again: not_found\n\
a group whose one member ended, not reaped, asked by its parent: {parent}\n\
the same group asked by another process: {other}\n\
reaped (0), then: not_found\n\
a host number to a group: ok\n\
it ended by that number: true\n\
a host number to one process: ok\n\
it ended by that number: true\n\
one past the host's last, to a group: invalid\n\
one past the host's last, to a process: invalid\n"
    )
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
const PGID_WANT: &str = "a negative group: invalid\n\
lead a group of its own: ok\n\
it leads: true\n\
join a job's group: ok\n\
it is in the job's group: true\n\
lead its own again: ok\n\
it leads again: true\n\
a group nobody leads: denied\n\
the job ended by: -15\n\
kill 0, as a send to os_pgid(): ok\n\
the caller heard it: true\n\
and its child ended by: -15\n\
back to the group it started in: ok\n\
it is there: true\n";

// ----------------------------------- macOS, CI's throwaway runner --

/// The group send on CI's throwaway macOS runner: the program signals
/// only the groups it started (it refuses a job that does not lead its
/// own). Not compiled on linux, where the same program runs in a
/// container (below).
#[cfg(target_os = "macos")]
#[test]
fn a_group_send_on_the_runner() {
    let dir = scratch("job_host");
    let want = job_want();
    every_lane_runs_on_the_host(
        &fixture("job.lu"),
        &dir,
        &[],
        Lupin::Refuses("os_pgid"),
        &|got, what| assert_eq!(got, want, "{what} (s241)"),
    );
}

/// The move into a group and `kill 0` on CI's throwaway macOS runner:
/// the program sends to its own group only once it leads it, and goes
/// back to the runner's group at the end.
#[cfg(target_os = "macos")]
#[test]
fn the_move_and_kill_zero_on_the_runner() {
    let dir = scratch("pgid_host");
    every_lane_runs_on_the_host(
        &fixture("pgid.lu"),
        &dir,
        &[],
        Lupin::Refuses("os_pgid"),
        &|got, what| assert_eq!(got, PGID_WANT, "{what} (s241)"),
    );
}

// -------------------------------------------- linux, a container --

#[cfg(target_os = "linux")]
mod container {
    use super::*;

    fn image() -> String {
        std::env::var("WOLF_GROUP_IMAGE")
            .unwrap_or_else(|_| "docker.io/library/ubuntu:24.04".into())
    }

    fn required() -> bool {
        std::env::var_os("WOLF_GROUP_REQUIRE_CONTAINER").is_some()
    }

    /// The engine that can run the image with no network and an init of
    /// its own, or a loud skip — a failure under
    /// `WOLF_GROUP_REQUIRE_CONTAINER`. The probe asserts the container
    /// has a pid namespace of its own: its init is process 1 and the
    /// probe a small number.
    fn engine(test: &str) -> Option<String> {
        let candidates: Vec<String> = match std::env::var("WOLF_GROUP_ENGINE") {
            Ok(e) => vec![e],
            Err(_) => vec!["docker".into(), "podman".into()],
        };
        let mut why = Vec::new();
        for e in &candidates {
            let out = Command::new(e)
                .args(["run", "--rm", "--network", "none", "--init"])
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
                    assert!(
                        pid < 64,
                        "{e}: a process in the container is {pid}, not a fresh pid namespace's"
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
            "WOLF_GROUP_REQUIRE_CONTAINER is set and no engine ran {} — {test} did not run: \
             {why:?}",
            image()
        );
        eprintln!(
            "SKIP: {test}: no container engine ran {} ({why:?}) — the live group witnesses did \
             not run on this box",
            image()
        );
        None
    }

    /// How a machine's command is started inside the container.
    #[derive(Clone, Copy, Debug)]
    enum Wrap {
        /// As the container's super-user, a child of its init.
        Plain,
        /// A session leader of its own (`setsid -w`).
        Session,
        /// As user 1000, beside a process of the super-user's whose id
        /// is in `S241_ROOTPID`.
        User,
    }

    impl Wrap {
        fn argv(self, inner: &[String]) -> Vec<String> {
            let mut v: Vec<String> = Vec::new();
            match self {
                Wrap::Plain => {}
                Wrap::Session => v.extend(["setsid".into(), "-w".into()]),
                Wrap::User => v.extend([
                    "sh".into(),
                    "-c".into(),
                    "sleep 300 & export S241_ROOTPID=$!; exec setpriv --reuid=1000 \
                     --regid=1000 --clear-groups \"$@\""
                        .into(),
                    "sh".into(),
                ]),
            }
            v.extend(inner.iter().cloned());
            v
        }
    }

    /// One throwaway container: no network, an init of its own (so an
    /// orphan is reaped and the program is not a session leader), removed
    /// on exit, every mount read-only, its working directory the image's
    /// own `/tmp`. Never `--pid host`.
    struct Throwaway<'a> {
        engine: &'a str,
        env: &'a [(&'a str, &'a str)],
        mounts: Vec<(PathBuf, &'static str)>,
    }

    impl Throwaway<'_> {
        fn run(&self, argv: &[String]) -> Output {
            let mut cmd = Command::new(self.engine);
            cmd.args(["run", "--rm", "--network", "none", "--init"]);
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

    /// `name` (a fixture) on every machine, each in a container of its
    /// own, started as `wrap` says: the checked machine and lupin under
    /// `conform-run`, the native and release binaries built on the host
    /// and run inside — plainly, and under `taskset -c 0-3` asked from
    /// inside when the container has four cpus. `check` judges each
    /// stdout; each program's standard error (its SigBlk and SigIgn) is
    /// printed beside it.
    fn every_lane_runs_in_a_container(
        name: &str,
        test: &str,
        env: &[(&str, &str)],
        wrap: Wrap,
        lupin: Lupin,
        check: &dyn Fn(&str, &str),
    ) {
        let Some(engine) = engine(test) else {
            return;
        };
        let entry = fixture(name);
        let dir = scratch(test);
        let fixtures = entry.parent().unwrap().to_path_buf();
        let mut mounts = vec![
            (dir.clone(), "/w"),
            (fixtures, "/fx"),
            (PathBuf::from(wolf()), "/machines/wolf"),
        ];
        let lupin_bin = lupin_or_skip();
        if let Some(l) = &lupin_bin {
            mounts.push((l.clone(), "/machines/lupin"));
        }
        let b = Throwaway {
            engine: &engine,
            env,
            mounts,
        };
        let program = format!("/fx/{name}");

        let out = b.run(&wrap.argv(&strs(&[
            "/machines/wolf",
            "conform-run",
            &program,
            "--checked",
            "--json",
        ])));
        assert!(
            out.status.success(),
            "conform-run --checked in the container failed on {name} ({wrap:?}): exit {:?}\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        let checked = parse_obs(&out.stdout, "the observation");
        eprintln!(
            "== checked on {name} ({wrap:?}, in {engine}): {}\n{}{}",
            checked.verdict,
            checked.stdout,
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            checked.verdict, "exit(0)",
            "the CHECKED lane on {name} ({wrap:?}): {checked:?}"
        );
        check(&checked.stdout, &format!("checked on {name} ({wrap:?})"));

        let four = b.cpus() >= 4;
        if !four {
            eprintln!(
                "SKIP: {test}: the container has fewer than four cpus — the taskset -c 0-3 rows \
                 did not run"
            );
        }
        for release in [false, true] {
            let tier = if release { "release" } else { "native" };
            let Some(exe) = build(&entry, &dir, release) else {
                continue;
            };
            let inside = format!("/w/{}", exe.file_name().unwrap().to_string_lossy());
            let mut rows: Vec<(String, Vec<String>)> = vec![(tier.to_string(), strs(&[&inside]))];
            if four {
                rows.push((
                    format!("{tier} taskset -c 0-3"),
                    strs(&["taskset", "-c", "0-3", &inside]),
                ));
            }
            for (row, argv) in rows {
                let out = b.run(&wrap.argv(&argv));
                let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
                let stderr = String::from_utf8_lossy(&out.stderr);
                eprintln!(
                    "== {row} on {name} ({wrap:?}, in {engine}): exit {:?}\n{stdout}{stderr}",
                    out.status.code()
                );
                assert_eq!(
                    out.status.code(),
                    Some(0),
                    "the {row} binary on {name} ({wrap:?}), in the container: {stderr}"
                );
                check(&stdout, &format!("{row} on {name} ({wrap:?})"));
            }
        }
        if lupin_bin.is_some() {
            let out = b.run(&wrap.argv(&strs(&[
                "/machines/lupin",
                "conform-run",
                &program,
                "--json",
            ])));
            let obs = parse_obs(&out.stdout, "lupin's observation");
            judge_lupin(&obs, name, lupin, check);
        }
    }

    /// The group send: a job's leader and its own children end by one
    /// send to the group; the group is then gone. A group whose member
    /// ended unreaped is `ok` here (linux). `-N` reaches a group and a
    /// process.
    #[test]
    fn a_group_send_in_a_container() {
        let want = job_want();
        every_lane_runs_in_a_container(
            "job.lu",
            "job",
            &[],
            Wrap::Plain,
            Lupin::Refuses("os_pgid"),
            &|got, what| assert_eq!(got, want, "{what} (s241)"),
        );
    }

    /// The move into a group, joining a job's group, a group nobody leads
    /// `denied`, and `kill 0` reaching the caller and its child.
    #[test]
    fn the_move_and_kill_zero_in_a_container() {
        every_lane_runs_in_a_container(
            "pgid.lu",
            "pgid",
            &[],
            Wrap::Plain,
            Lupin::Refuses("os_pgid"),
            &|got, what| assert_eq!(got, PGID_WANT, "{what} (s241)"),
        );
    }

    /// A session leader may not move (`EPERM`, `denied`).
    #[test]
    fn a_session_leader_may_not_move() {
        let want = "a session leader leads its group: true\n\
and may not move: denied\n\
still: true\n";
        every_lane_runs_in_a_container(
            "pgid.lu",
            "pgid_session",
            &[("S241_SESSION", "1")],
            Wrap::Session,
            Lupin::Refuses("os_pgid"),
            &|got, what| assert_eq!(got, want, "{what} (s241)"),
        );
    }

    /// `send_all` as the container's super-user: every process it
    /// started ends; with nothing left but itself and the namespace's
    /// init, linux answers `not_found` (`ESRCH`: no process was found to
    /// try).
    #[test]
    fn everything_as_the_super_user_in_a_container() {
        let want = "as the super-user: two jobs and a child, started\n\
TERMINATE to everything it may signal: ok\n\
they ended by: -15 -15 -15\n\
signal 0 with nothing of its own left to reach: not_found\n";
        every_lane_runs_in_a_container(
            "all.lu",
            "all_root",
            &[],
            Wrap::Plain,
            Lupin::Refuses("os_signal_send_all"),
            &|got, what| assert_eq!(got, want, "{what} (s241)"),
        );
    }

    /// `send_all` as user 1000 beside a super-user's process: its own
    /// children end, the super-user's process lives (`denied` to a
    /// signal 0), and with only that process left linux answers `ok`
    /// (it found one, and may not signal it: s240's measured row).
    #[test]
    fn everything_as_a_user_in_a_container() {
        let want = "as a user, beside the super-user's process: two jobs and a child, started\n\
TERMINATE to everything it may signal: ok\n\
they ended by: -15 -15 -15\n\
the super-user's process: denied\n\
signal 0 with nothing of its own left to reach: ok\n";
        every_lane_runs_in_a_container(
            "all.lu",
            "all_user",
            &[],
            Wrap::User,
            Lupin::Refuses("os_signal_send_all"),
            &|got, what| assert_eq!(got, want, "{what} (s241)"),
        );
    }
}
