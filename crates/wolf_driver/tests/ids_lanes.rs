//! s236 (wolf-lang#633, #701, #742) — who a process runs as, a file's
//! owner, and the drop, on every machine. `[os.proc.ids]`,
//! `[os.proc.ids.set]`, `[os.fs.owner]`.
//!
//! Before this lane no machine resolved a name for any of it (E0301 on
//! the checked machine, native and release; "does not resolve" on
//! lupin). Each witness runs on the checked machine and lupin under
//! `conform-run` and as native and release binaries, and every machine's
//! standard output is printed, so a `--nocapture` log is the
//! four-machine table.
//!
//! **Nothing here changes an id or an owner on a host.** The witnesses
//! are two kinds:
//!
//! - ON THE HOST (every unix; the two corpus rows on windows too): the
//!   ids read and held to `id`, the owner calls that change nothing
//!   (`-1 -1`, the owner a file already has), and the refusals a user
//!   gets. The programs ask who they are and do not ask the
//!   super-user's questions.
//! - IN A CONTAINER (linux): the drop, a drop refused at its last step,
//!   and real owner changes run as the super-user inside a throwaway
//!   `--network none` container, each machine in a container of its
//!   own, removed when it exits. Every bind mount is READ-ONLY and the
//!   programs write only to the container's own `/tmp`, so no file on
//!   the host changes owner — asserted after each run. The native and
//!   release binaries are built on the host and run inside; the checked
//!   machine and lupin are the host's `wolf` and `lupin` binaries,
//!   mounted. The process-spawning rows also run under
//!   `taskset -c 0-3`, asked from INSIDE the container (wolf-lang#570).
//!
//! **The engine.** `WOLF_IDS_ENGINE` names it; otherwise the first of
//! `docker` and `podman` that answers. `WOLF_IDS_IMAGE` names the image
//! (default `docker.io/library/ubuntu:24.04`): it supplies a C library
//! the host's binaries can run on, `sh`, `id` and `taskset`, and nothing
//! of wolf's. Where no engine can run the image the container witnesses
//! SKIP, loudly and by name — and **`WOLF_IDS_REQUIRE_CONTAINER=1`
//! (CI's linux job) makes that a failure**, so the drop cannot go
//! unwitnessed behind a green. macOS and windows have no linux
//! container: there the three container tests are not compiled, and
//! `os_set_ids` itself answers `unsupported` (macOS, `host.lu`) or the
//! corpus rows' `unsupported` side (windows).
//!
//! lupin 0.1.52 (`025428b`, the 0.2.29 pairing) predates the mirror — it
//! resolves none of the eight names — and is pinned by version and
//! release commit as pre-mirror (s180's design). A lupin with the mirror
//! serves the two owner calls and refuses the five reads and
//! `os_set_ids` BY NAME (it has no host call for an id without
//! `unsafe`).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The lupin releases that predate the mirror, with the commit each
/// archive reports: 0.1.52 (r34's release, `025428b`), measured —
/// "`os_uid` does not resolve".
const PRE_MIRROR_LUPIN: &[(&str, Option<&str>)] = &[("0.1.52", Some("025428b"))];

fn pre_mirror(lupin: &Obs) -> bool {
    PRE_MIRROR_LUPIN
        .iter()
        .any(|(v, c)| *v == lupin.version && c.is_none_or(|c| lupin.commit.starts_with(c)))
}

/// The verdict a pre-mirror lupin gives a program calling a name it
/// does not resolve.
const PRE: (&str, &str) = ("unsupported", "");

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
        .join("ids_lanes")
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
             (LUPIN={}) — the oracle leg of s236's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    found
}

#[cfg(unix)]
fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ids")
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

/// The stdout a corpus row's own header declares — the one truth the
/// file states for every host (`check: run(exit=0, stdout="…")`).
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
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            PRE,
            "lupin {} at {} (pre-mirror, pinned by version) on {name}",
            obs.version,
            obs.commit
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

/// `entry` on every machine, ON THE HOST, in `dir`: the checked machine
/// and lupin under `conform-run`, the native and release binaries run
/// directly. `check` judges each stdout.
fn every_lane_runs_on_the_host(entry: &Path, dir: &Path, lupin: Lupin, check: &dyn Fn(&str, &str)) {
    let name = entry.file_name().unwrap().to_string_lossy().to_string();
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--checked")
        .arg("--json");
    let out = run_in(cmd, dir);
    assert!(
        out.status.success(),
        "conform-run --checked failed on {name}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let checked = parse_obs(&out.stdout, "the observation");
    eprintln!(
        "== checked on {name}: {}\n{}",
        checked.verdict, checked.stdout
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
        let out = run_in(Command::new(&exe), dir);
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        eprintln!(
            "== {tier} on {name}: exit {:?}\n{stdout}",
            out.status.code()
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
        cmd.arg("conform-run").arg(entry).arg("--json");
        let out = run_in(cmd, dir);
        let obs = parse_obs(&out.stdout, "lupin's observation");
        judge_lupin(&obs, &name, lupin, check);
    }
}

/// A corpus row on every machine: each stdout is the one the row's own
/// header declares.
fn the_corpus_row_runs(row: &str, test: &str, lupin: Lupin) {
    let dir = scratch(test);
    let entry = corpus(row);
    let want = declared_stdout(&entry);
    every_lane_runs_on_the_host(&entry, &dir, lupin, &|got, what| {
        assert_eq!(got, want, "{what} (s236)");
    });
}

// ------------------------------------------------- every host --

/// The shape rows (`invalid` before the host, on every host), and the
/// owner calls that change nothing: `ok` where the host has owners,
/// `unsupported` where it has none. Red at trunk: E0301 (no name
/// resolves).
#[test]
fn the_rows_that_change_nothing_on_every_host() {
    the_corpus_row_runs("os/ids_rows.lu", "rows", Lupin::Mirrors);
}

/// The four ids and the groups are served together or refused together,
/// and a new file is owned by the effective user. lupin has no host
/// call for an id and refuses by name on a host that has ids; on
/// windows every machine, lupin included, answers the `unsupported`
/// row, so there lupin runs the row as written.
#[test]
fn who_it_runs_as_on_every_host() {
    let lupin = if cfg!(windows) {
        Lupin::Mirrors
    } else {
        Lupin::Refuses("os_uid")
    };
    the_corpus_row_runs("os/ids_read.lu", "read", lupin);
}

// ------------------------------------------------------- unix --

/// One line of `id`'s output, for this test process (which every
/// machine under test is a child of, so the ids are its own).
#[cfg(unix)]
fn id(flag: &str) -> String {
    let out = Command::new("id").arg(flag).output().expect("id runs");
    assert!(out.status.success(), "id {flag}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(unix)]
fn numbers(s: &str) -> std::collections::BTreeSet<u64> {
    s.split_whitespace()
        .map(|w| {
            w.parse()
                .unwrap_or_else(|_| panic!("a group id, not {w:?}"))
        })
        .collect()
}

/// The ids against `id`, a reader that is no machine under test: the
/// real and effective user id, the real and effective group id. The
/// supplementary groups against the host's own word for this process:
/// on linux the `Groups:` line of `/proc/self/status` (exactly
/// `getgroups`), the same set; elsewhere `id -G`, which may list more
/// than `getgroups` holds (macOS asks its directory service), so every
/// group the program lists must be in it.
#[cfg(unix)]
#[test]
fn the_ids_are_the_hosts() {
    let dir = scratch("who");
    let want_ids = format!(
        "uid={} euid={} gid={} egid={}",
        id("-ru"),
        id("-u"),
        id("-rg"),
        id("-g")
    );
    let all = numbers(&id("-G"));
    eprintln!("== the host: {want_ids}; id -G: {all:?}");
    every_lane_runs_on_the_host(
        &fixture("who.lu"),
        &dir,
        Lupin::Refuses("os_uid"),
        &|got, what| {
            let mut lines = got.lines();
            assert_eq!(lines.next(), Some(want_ids.as_str()), "{what} (s236)");
            let groups = lines
                .next()
                .and_then(|l| l.strip_prefix("groups:"))
                .unwrap_or_else(|| panic!("{what}: a groups line in {got:?}"));
            let listed = numbers(groups);
            #[cfg(target_os = "linux")]
            {
                let status = std::fs::read_to_string("/proc/self/status").expect("status");
                let host = status
                    .lines()
                    .find_map(|l| l.strip_prefix("Groups:"))
                    .expect("a Groups line");
                assert_eq!(listed, numbers(host), "{what}: the host's own Groups line");
            }
            assert!(
                listed.is_subset(&all),
                "{what}: every listed group is one `id -G` names: {listed:?} in {all:?}"
            );
        },
    );
}

/// The owner calls that change nothing and the refusal a user gets, on
/// the host: `-1 -1`, the owner a file already has (by path, on a link
/// itself, on a handle), a missing path, a dangling link, a closed
/// handle, and a file that may not be given to the super-user. The
/// program reads no id, so lupin serves every row.
#[cfg(unix)]
#[test]
fn owner_calls_change_nothing_on_a_host() {
    let dir = scratch("owner_host");
    let to_root = if id("-u") == "0" {
        "to root: not asked (this is the super-user)"
    } else {
        "to root: denied errno=1 by handle=denied"
    };
    let want = format!(
        "leave: ok same: ok a link itself: ok\n\
missing: not_found errno=2 dangling: followed=not_found itself=ok\n\
by handle: leave=ok same=ok closed=io errno=0\n\
{to_root}\n\
owner unchanged: true\n"
    );
    every_lane_runs_on_the_host(
        &fixture("owner_host.lu"),
        &dir,
        Lupin::Mirrors,
        &|got, what| {
            assert_eq!(got, want, "{what} (s236)");
            assert!(
                !dir.join("h").exists(),
                "{what}: the program removed its tree"
            );
        },
    );
}

/// The drop asked by a user, on the host: `denied` where it is served
/// (linux: the first step is refused and nothing has changed) and
/// `unsupported`, by name, where it is not; asked on the super-user's
/// host it is not asked at all. The ids and groups read the same after.
/// lupin refuses `os_euid` by name.
#[cfg(unix)]
#[test]
fn a_user_may_not_set_ids_on_a_host() {
    let dir = scratch("set_host");
    let refused = if cfg!(target_os = "linux") {
        "denied"
    } else {
        "unsupported"
    };
    let set_ids = if id("-u") == "0" {
        "set ids: not asked (this is the super-user)".to_string()
    } else {
        format!("set ids: own={refused} root={refused}")
    };
    let want = format!("{set_ids}\nunchanged: true\n");
    every_lane_runs_on_the_host(
        &fixture("set_host.lu"),
        &dir,
        Lupin::Refuses("os_euid"),
        &|got, what| assert_eq!(got, want, "{what} (s236)"),
    );
}

// -------------------------------------------- linux, a container --

#[cfg(target_os = "linux")]
mod container {
    use super::*;
    use std::os::unix::fs::MetadataExt as _;

    fn image() -> String {
        std::env::var("WOLF_IDS_IMAGE").unwrap_or_else(|_| "docker.io/library/ubuntu:24.04".into())
    }

    fn required() -> bool {
        std::env::var_os("WOLF_IDS_REQUIRE_CONTAINER").is_some()
    }

    /// The engine that can run the image with no network, or a loud
    /// skip — a failure under `WOLF_IDS_REQUIRE_CONTAINER`.
    fn engine(test: &str) -> Option<String> {
        let candidates: Vec<String> = match std::env::var("WOLF_IDS_ENGINE") {
            Ok(e) => vec![e],
            Err(_) => vec!["docker".into(), "podman".into()],
        };
        let mut why = Vec::new();
        for e in &candidates {
            let out = Command::new(e)
                .args(["run", "--rm", "--network", "none"])
                .arg(image())
                .args(["id", "-u"])
                .stdin(Stdio::null())
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    let uid = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    assert_eq!(
                        uid, "0",
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
            "WOLF_IDS_REQUIRE_CONTAINER is set and no engine ran {} — {test} did not run: {why:?}",
            image()
        );
        eprintln!(
            "SKIP: {test}: no container engine ran {} ({why:?}) — the witness that sets ids and \
             owners did not run on this box",
            image()
        );
        None
    }

    /// One throwaway container: no network, removed on exit, every
    /// mount read-only, its working directory the image's own `/tmp`.
    struct Throwaway<'a> {
        engine: &'a str,
        extra: &'a [&'a str],
        mounts: Vec<(PathBuf, &'static str)>,
    }

    impl Throwaway<'_> {
        fn run(&self, argv: &[&str]) -> Output {
            let mut cmd = Command::new(self.engine);
            cmd.args(["run", "--rm", "--network", "none"])
                .args(self.extra);
            for (host, at) in &self.mounts {
                cmd.arg("-v").arg(format!("{}:{at}:ro", host.display()));
            }
            cmd.args(["-w", "/tmp"]).arg(image()).args(argv);
            cmd.stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            cmd.output().expect("the engine runs")
        }

        /// How many cpus a process inside may run on.
        fn cpus(&self) -> usize {
            let out = self.run(&["nproc"]);
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse()
                .unwrap_or(1)
        }
    }

    /// Every entry under `dir` is still this user's: nothing a
    /// container did reached a file on the host.
    fn nothing_on_the_host_changed_owner(dir: &Path, what: &str) {
        let me = std::fs::metadata(env!("CARGO_MANIFEST_DIR"))
            .expect("the crate directory")
            .uid();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(p) = stack.pop() {
            let md = std::fs::symlink_metadata(&p).expect("a scratch entry");
            assert_eq!(
                md.uid(),
                me,
                "{what}: {} changed owner on the host",
                p.display()
            );
            if md.is_dir() {
                for e in std::fs::read_dir(&p)
                    .expect("a scratch directory")
                    .flatten()
                {
                    stack.push(e.path());
                }
            }
        }
    }

    /// `name` (a fixture) on every machine, each in a container of its
    /// own started with `extra`: the checked machine and lupin under
    /// `conform-run`, the native and release binaries built on the host
    /// and run inside — plainly, and under `taskset -c 0-3` asked from
    /// inside when the container has four cpus. `check` judges each
    /// stdout; `threads` says whether the binaries must report more
    /// than one thread on standard error (`threads: N`).
    fn every_lane_runs_in_a_container(
        name: &str,
        test: &str,
        extra: &[&str],
        threads: bool,
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
            extra,
            mounts,
        };
        let program = format!("/fx/{name}");

        let out = b.run(&[
            "/machines/wolf",
            "conform-run",
            &program,
            "--checked",
            "--json",
        ]);
        assert!(
            out.status.success(),
            "conform-run --checked in the container failed on {name}: exit {:?}\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        let checked = parse_obs(&out.stdout, "the observation");
        eprintln!(
            "== checked on {name} (in {engine}): {}\n{}",
            checked.verdict, checked.stdout
        );
        assert_eq!(
            checked.verdict, "exit(0)",
            "the CHECKED lane on {name}: {checked:?}"
        );
        check(&checked.stdout, &format!("checked on {name}"));

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
            let mut rows: Vec<(String, Vec<&str>)> =
                vec![(tier.to_string(), vec![inside.as_str()])];
            if four {
                rows.push((
                    format!("{tier} taskset -c 0-3"),
                    vec!["taskset", "-c", "0-3", inside.as_str()],
                ));
            }
            for (row, argv) in rows {
                let out = b.run(&argv);
                let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
                let stderr = String::from_utf8_lossy(&out.stderr);
                eprintln!(
                    "== {row} on {name} (in {engine}): exit {:?}\n{stdout}{stderr}",
                    out.status.code()
                );
                assert_eq!(
                    out.status.code(),
                    Some(0),
                    "the {row} binary on {name}, in the container: {stderr}"
                );
                check(&stdout, &format!("{row} on {name}"));
                if threads {
                    let n: usize = stderr
                        .lines()
                        .find_map(|l| l.strip_prefix("threads: "))
                        .and_then(|n| n.trim().parse().ok())
                        .unwrap_or_else(|| panic!("{row} on {name}: a thread count in {stderr:?}"));
                    assert!(
                        n >= 2,
                        "{row} on {name}: the runtime has worker threads, so the drop was read \
                         in more than one ({n})"
                    );
                }
            }
        }
        if lupin_bin.is_some() {
            let out = b.run(&["/machines/lupin", "conform-run", &program, "--json"]);
            let obs = parse_obs(&out.stdout, "lupin's observation");
            // Every container program asks who it is first.
            judge_lupin(&obs, name, Lupin::Refuses("os_euid"), check);
        }
        nothing_on_the_host_changed_owner(&dir, test);
    }

    /// The drop, as the container's super-user: three groups planted,
    /// then one call to user 1000, group 1000, groups {1000, 2000}.
    /// Read back by the program, by the host for EVERY thread, and by a
    /// child `id`; the way back is `denied`; a new file is the user's.
    /// With the user ids set first (the planted break) this is where the
    /// group id 0 and the three groups are left behind, and the process
    /// aborts naming them.
    #[test]
    fn the_drop_in_a_container() {
        let want = "the super-user, with groups: uid=0 euid=0 gid=0 egid=0 groups=[0, 4, 27]\n\
dropped: uid=1000 euid=1000 gid=1000 egid=1000 groups=[1000, 2000]\n\
every thread agrees: true: Uid: 1000 1000 1000 1000; Gid: 1000 1000 1000 1000; Groups: 1000 2000; \n\
a child is: exit=0 1000 1000 1000 2000\n\
the way back: denied\n\
still: uid=1000 euid=1000 gid=1000 egid=1000 groups=[1000, 2000]\n\
a file it makes is owned: 1000:1000\n";
        every_lane_runs_in_a_container("drop.lu", "drop", &[], true, &|got, what| {
            assert_eq!(got, want, "{what} (s236)");
        });
    }

    /// A drop refused at its LAST step — a container without the power
    /// to change user ids, so the groups and the group ids change and
    /// the user ids are then refused — is `denied` and leaves the
    /// process exactly as it was, in every thread.
    #[test]
    fn a_drop_refused_at_the_last_step_is_put_back_whole() {
        let want = "before: uid=0 euid=0 gid=0 egid=0 groups=[0, 4, 27]\n\
the drop: denied\n\
after: uid=0 euid=0 gid=0 egid=0 groups=[0, 4, 27]\n\
every thread agrees: true: Uid: 0 0 0 0; Gid: 0 0 0 0; Groups: 0 4 27; \n\
as it was: true\n";
        every_lane_runs_in_a_container(
            "refused.lu",
            "refused",
            &["--cap-drop", "SETUID"],
            false,
            &|got, what| assert_eq!(got, want, "{what} (s236)"),
        );
    }

    /// Owners, as the container's super-user and then as user 1000: set
    /// and read back through `fs_stat`, one id left alone, the set-id
    /// bits the host clears, a link itself against its target, a
    /// dangling link, the handle form; then what a user may and may not
    /// do with a file it owns and one it does not.
    #[test]
    fn an_owner_changed_in_a_container() {
        let want = "made: 0:0 mode=3565\n\
both: ok 1000:2000 mode=493\n\
the group alone: ok 1000:3000; the user alone: ok 1001:3000\n\
a link itself: ok link=1234:1234 target=1001:3000\n\
through the link: ok link=1234:1234 target=5:6\n\
dangling: followed=not_found errno=2 itself=ok 7:8\n\
by handle: ok 1000:1000; the group alone: ok 1000:2000; closed=io errno=0\n\
a directory: ok 1000:2000\n\
as user 1000, its own file: to root=denied errno=1 to another user=denied\n\
its group: to its own=ok 1000:1000 to its second=ok 1000:2000 to one it is not in=denied 1000:2000\n\
by handle: its own group=ok to root=denied 1000:1000\n\
a file it does not own: the user=denied the group=denied 0:0\n";
        every_lane_runs_in_a_container("owner.lu", "owner", &[], false, &|got, what| {
            assert_eq!(got, want, "{what} (s236)");
        });
    }
}
