//! wolf-lang#1153 (`[os.proc.subreaper]`, `[os.proc.poll.adopted]`): a
//! program can mark itself a child subreaper and reap the orphans it adopts
//! without touching the children its process table holds.
//!
//! `os_subreaper_set(on: bool) -> bool ! {unsupported, denied, io}` answers
//! the previous value, `os_subreaper() -> bool ! {unsupported, io}` reads
//! it, and `os_wait_adopted_poll() -> (int, int) ! {unsupported, io}` never
//! waits: `(pid, status)` once for each ended child the table does not hold,
//! `(0, 0)` when none has ended. The rows run on the checked machine, native
//! and release, with one stdout each; lupin has no such builtins.
//!
//! - Off linux (macOS, windows), every call answers `unsupported`
//!   (`fixtures/subreaper/unsupported.lu`), on the host itself: nothing is
//!   started.
//! - On linux the live rows start a shell that leaves a grandchild behind,
//!   so they run only inside a throwaway `--network none` container, each
//!   machine in one of its own, under `sh -c` as the container's process 1
//!   so that the program is not process 1 (process 1 adopts every orphan
//!   whether or not it asked). The job-wait witnesses' switches choose the
//!   place: `WOLF_JOBWAIT_ENGINE` (else the first of `docker` and `podman`
//!   that answers), `WOLF_JOBWAIT_IMAGE` (default
//!   `docker.io/library/ubuntu:24.04`), and `WOLF_JOBWAIT_REQUIRE_CONTAINER`
//!   makes a missing engine a failure. The native and release binaries are
//!   built on the host; the checked machine is this `wolf`, mounted.
//!   `WOLF_REAPER_IN_CONTAINER=1` says the test itself already runs inside
//!   a throwaway container where it is not process 1 (a local record): the
//!   rows then run where the test does.
//!
//! Every skip prints a `SKIP:` line past the test harness's capture, so a
//! passing run still shows it; a row no machine ran fails.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A line a passing test still shows: written to stderr's handle directly,
/// past the harness's capture.
fn loud(line: &str) {
    use std::io::Write as _;
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/subreaper")
}

fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("subreaper_lanes")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

/// The native or release binary of `entry`. A compile error FAILS the
/// gate (it is the defect at a pre-fix head, never an environment).
fn build(entry: &Path, dir: &Path, release: bool) -> Option<PathBuf> {
    let tier = if release { "release" } else { "native" };
    let exe = dir.join(format!("a-{tier}{}", std::env::consts::EXE_SUFFIX));
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
        assert!(
            std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
            "required {tier} tier unavailable: {stderr}"
        );
        loud(&format!(
            "SKIP: {} --{tier}: this host cannot build the tier: {}",
            entry.display(),
            stderr.trim()
        ));
        return None;
    }
    assert!(out.status.success(), "wolf build failed: {stderr}");
    Some(exe)
}

/// Where a row's machines run.
enum Place {
    /// Where the test runs.
    Here,
    /// A throwaway container of this engine, one per machine.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Container(String),
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn image() -> String {
    std::env::var("WOLF_JOBWAIT_IMAGE").unwrap_or_else(|_| "docker.io/library/ubuntu:24.04".into())
}

impl Place {
    /// `machine`'s run of `entry`, whose binaries are in `dir`: its argv as
    /// this place sees the files, or `None` where the tier cannot build.
    fn argv(&self, machine: &str, entry: &Path, dir: &Path) -> Option<Vec<String>> {
        let inside = matches!(self, Place::Container(_));
        let name = entry.file_name().unwrap().to_string_lossy().to_string();
        if machine == "--checked" {
            let (w, e) = if inside {
                ("/machines/wolf".to_string(), format!("/fx/{name}"))
            } else {
                (wolf().to_string(), entry.display().to_string())
            };
            return Some(vec![
                w,
                "conform-run".into(),
                "--json".into(),
                "--checked".into(),
                e,
            ]);
        }
        let exe = build(entry, dir, machine == "--release")?;
        Some(vec![if inside {
            format!("/w/{}", exe.file_name().unwrap().to_string_lossy())
        } else {
            exe.display().to_string()
        }])
    }

    fn run(&self, argv: &[String], dir: &Path) -> Output {
        let mut cmd = match self {
            Place::Here => {
                let mut c = Command::new(&argv[0]);
                c.args(&argv[1..]).current_dir(dir);
                c
            }
            Place::Container(engine) => {
                let mut c = Command::new(engine);
                c.args(["run", "--rm", "--network", "none"])
                    .arg("-v")
                    .arg(format!("{}:/w:ro", dir.display()))
                    .arg("-v")
                    .arg(format!("{}:/fx:ro", fixtures().display()))
                    .arg("-v")
                    .arg(format!("{}:/machines/wolf:ro", wolf()))
                    .args(["-w", "/tmp"])
                    .arg(image())
                    // `sh` is process 1, so the program is not: an orphan
                    // reaches it only if it asked to be a subreaper.
                    .args(["sh", "-c", "\"$@\"; exit $?", "sh"])
                    .args(argv);
                c
            }
        };
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("the row runs")
    }
}

/// The linux place for a live row: a throwaway container, or where the test
/// runs when it is one already; `None` (a loud skip, a failure where CI
/// requires the container) when no engine runs the image.
#[cfg(target_os = "linux")]
fn linux_place(test: &str) -> Option<Place> {
    if std::env::var_os("WOLF_REAPER_IN_CONTAINER").is_some_and(|v| !v.is_empty()) {
        loud(&format!(
            "PLACE: {test}: inside the container the test runs in (WOLF_REAPER_IN_CONTAINER)"
        ));
        return Some(Place::Here);
    }
    let candidates: Vec<String> = match std::env::var("WOLF_JOBWAIT_ENGINE") {
        Ok(e) if !e.is_empty() => vec![e],
        _ => vec!["docker".into(), "podman".into()],
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
                loud(&format!(
                    "PLACE: {test}: a throwaway {e} container of {}",
                    image()
                ));
                return Some(Place::Container(e.clone()));
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
        std::env::var_os("WOLF_JOBWAIT_REQUIRE_CONTAINER").is_none(),
        "WOLF_JOBWAIT_REQUIRE_CONTAINER is set and no engine ran {} — {test} did not run: {why:?}",
        image()
    );
    for machine in MACHINES {
        loud(&format!(
            "SKIP: {test} {machine}: no container engine ran {} ({why:?})",
            image()
        ));
    }
    None
}

const MACHINES: [&str; 3] = ["--checked", "--native", "--release"];

/// `name` on every machine at `place`: exit 0 and `want` on each, every
/// answer printed before the verdict.
fn rows(place: &Place, name: &str, test: &str, want: &str) {
    let entry = fixtures().join(name);
    let dir = scratch(test);
    let mut wrong = Vec::new();
    let mut ran = 0;
    for machine in MACHINES {
        let Some(argv) = place.argv(machine, &entry, &dir) else {
            continue;
        };
        let out = place.run(&argv, &dir);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let (verdict, stdout) = if machine == "--checked" {
            let rec: serde_json::Value = match serde_json::from_slice(&out.stdout) {
                Ok(r) => r,
                Err(e) => {
                    wrong.push(format!("{machine}: no record ({e}): {stderr}"));
                    continue;
                }
            };
            (
                rec["verdict"].as_str().unwrap_or("").to_string(),
                rec["stdout_inline"].as_str().unwrap_or("").to_string(),
            )
        } else {
            (
                format!("exit({})", out.status.code().unwrap_or(-1)),
                String::from_utf8_lossy(&out.stdout).replace('\r', ""),
            )
        };
        ran += 1;
        eprintln!(
            "REPORT {machine} {name}: {verdict:?} {stdout:?} | {}",
            stderr.trim()
        );
        if verdict != "exit(0)" || stdout != want {
            wrong.push(format!("{machine}: {verdict} {stdout:?}"));
        }
    }
    loud(&format!(
        "RAN {test} on {ran} of {} machines",
        MACHINES.len()
    ));
    assert!(wrong.is_empty(), "{test}:\n{}", wrong.join("\n"));
    assert!(ran > 0, "no machine ran {test}");
}

/// Off linux nothing is set, started or waited for: three `unsupported`s.
#[cfg(not(target_os = "linux"))]
#[test]
fn the_pair_is_unsupported_off_linux() {
    rows(
        &Place::Here,
        "unsupported.lu",
        "the_pair_is_unsupported_off_linux",
        "set: unsupported\nget: unsupported\npoll: unsupported\n",
    );
}

/// The attribute set, a grandchild adopted and reported once with its own
/// pid, the shell left to the table's poll, nothing left behind, the
/// attribute cleared.
#[cfg(target_os = "linux")]
#[test]
fn an_orphan_is_adopted_and_reaped_once() {
    let test = "an_orphan_is_adopted_and_reaped_once";
    let Some(place) = linux_place(test) else {
        return;
    };
    rows(
        &place,
        "adopted.lu",
        test,
        "set: was false, now true\n\
         adopted: status 0, its own pid true\n\
         adopted again: (0, 0)\n\
         table: the shell true, status 0\n\
         nothing left: (0, 0) (-1, 0), children 0\n\
         clear: was true, now false\n",
    );
}

/// Two adopted children and the table's child end together: the adopted
/// poll reports the two, each once, and leaves the table's to `os_wait_poll`.
#[cfg(target_os = "linux")]
#[test]
fn the_table_child_is_left_to_the_table_poll() {
    let test = "the_table_child_is_left_to_the_table_poll";
    let Some(place) = linux_place(test) else {
        return;
    };
    rows(
        &place,
        "order.lu",
        test,
        "set: was false\n\
         adopted: two true, distinct true, neither the shell true, statuses 0 0\n\
         adopted again: (0, 0)\n\
         table: the shell true, status 3\n",
    );
}

/// The cause the order row once met: a shell that waits for a job of its
/// own reaps its background child that ended meanwhile, so nothing is
/// orphaned and nothing is adopted, mark or no mark.
#[cfg(target_os = "linux")]
#[test]
fn a_shell_that_waits_reaps_its_own_children() {
    let test = "a_shell_that_waits_reaps_its_own_children";
    let Some(place) = linux_place(test) else {
        return;
    };
    rows(
        &place,
        "shell_reaps.lu",
        test,
        "set: was false
table: the shell ended 0
adopted: (0, 0)
",
    );
}

/// Without the attribute the orphan goes to process 1, not to the program.
#[cfg(target_os = "linux")]
#[test]
fn without_the_attribute_nothing_is_adopted() {
    let test = "without_the_attribute_nothing_is_adopted";
    let Some(place) = linux_place(test) else {
        return;
    };
    rows(
        &place,
        "not_adopted.lu",
        test,
        "attribute: false\nadopted: (0, 0)\ntable: the shell true, status 0\n",
    );
}
