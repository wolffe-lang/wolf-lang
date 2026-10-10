//! s225 (wolf-lang#534) — replacing the running program, and removing a
//! variable, on every machine. `[os.proc.exec]`, `[os.env.unset]`.
//!
//! Before this lane a wolf program could not become another program
//! (boreutils dropped `env`; pelt's `exec` ran its command as a child and
//! exited), and could not remove a variable (pelt's `unset HOME` still
//! reached `/usr/bin/env`). Two kinds of witness:
//!
//! - the corpus rows `os/exec_rows.lu` and `os/env_unset.lu`, which
//!   replace nothing and spawn nothing, on every host, under
//!   `conform-run` on the checked machine, the native and release tiers
//!   and lupin;
//! - the fixtures under `fixtures/exec/`, which exec `sh` or spawn it,
//!   so they run on unix only: the checked machine and lupin under
//!   `conform-run`, the native and release binaries `wolf build` made —
//!   plain, and on linux again under `taskset -c 0-3` (wolf-lang#570).
//!
//! An exec that succeeds ends the observation: under `conform-run` no
//! record follows, and the stream is the program's own bytes then the new
//! image's — on the checked machine and lupin exactly as on a native
//! binary. The image witness reads its pid from the new image and holds
//! it against the pid this harness started, so "the process was
//! replaced, not spawned" is a number, not a stdout.
//!
//! lupin 0.1.49 (the pairing, release archive commit `f516a5f`) and its
//! trunk at the rebase (`51cb491`) predate the mirror: they resolve
//! neither name and answer `unsupported`. They are pinned by version and
//! commit as pre-mirror (s180's design), so the mirror's own release
//! drops the pins rather than widening them.

mod lane_exit;

#[cfg(unix)]
use std::io::{BufRead as _, BufReader, Read as _};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The lupin builds that predate the mirror, by version and the commit
/// their records report.
const PRE_MIRROR_LUPIN: &[(&str, Option<&str>)] =
    &[("0.1.49", Some("f516a5f")), ("0.1.49", Some("51cb491"))];

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
        .join("exec_lanes")
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
        .join("tests/fixtures/exec")
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
         (LUPIN={}) — the oracle leg of s225's gate did not run",
        std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
    );
    eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
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

#[cfg(unix)]
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

/// Every machine a unix fixture runs on: checked and lupin under
/// `conform-run`, the native and release binaries — plain, and on linux
/// again under `taskset -c 0-3` (taskset execs its command, so the pid
/// is the program's).
fn lanes(entry: &Path, dir: &Path) -> Vec<Lane> {
    let mut out = Vec::new();
    let mut c = Command::new(wolf());
    c.arg("conform-run").arg(entry).arg("--checked").arg("--json");
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

/// Run `cmd` in `dir` with `stdin` written to descriptor 0, 1 and 2
/// piped, and `WOLF_S225_HOST=1` in its environment.
fn run_fed(mut cmd: Command, dir: &Path, stdin: &str) -> Output {
    cmd.current_dir(dir)
        .env("WOLF_S225_HOST", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn");
    let mut w = child.stdin.take().expect("stdin");
    let _ = w.write_all(stdin.as_bytes());
    drop(w);
    child.wait_with_output().expect("wait")
}

#[cfg(unix)]
/// A fixture that RETURNS on every machine (no exec succeeds in it): a
/// record's stdout on checked and lupin, the binary's own elsewhere.
fn every_lane_prints(name: &str, stdin: &str, want: &str, lupin_refuses: bool) {
    let entry = fixture(name);
    let dir = scratch(&name.replace('.', "_"));
    for lane in lanes(&entry, &dir) {
        let out = run_fed(lane.cmd, &dir, stdin);
        let what = format!("the {} lane on {name}", lane.name);
        let got = if lane.recorded {
            let obs = parse_obs(&out.stdout, &what);
            if lane.lupin && (pre_mirror(&obs) || lupin_refuses) {
                assert_eq!(
                    obs.verdict, "unsupported",
                    "{what}: lupin {} at {} refuses (pre-mirror, or by name)",
                    obs.version, obs.commit
                );
                eprintln!("PINNED {what}: lupin {} at {} unsupported", obs.version, obs.commit);
                continue;
            }
            assert_eq!(obs.verdict, "exit(0)", "{what}: {obs:?}");
            obs.stdout
        } else {
            assert_eq!(
                out.status.code(),
                Some(0),
                "{what}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).into_owned()
        };
        assert_eq!(got, want, "{what} (s225)");
        eprintln!("PASS {what}");
    }
}

#[cfg(unix)]
/// A fixture whose exec SUCCEEDS: the stream is the program's bytes and
/// then the new image's, with no record on any machine; `want` is it.
/// A pre-mirror lupin answers a record (`unsupported`) instead, and a
/// refusing one too.
fn every_lane_execs(name: &str, want: &str, lupin_refuses: bool) {
    let entry = fixture(name);
    let dir = scratch(&name.replace('.', "_"));
    for lane in lanes(&entry, &dir) {
        let out = run_fed(lane.cmd, &dir, "");
        let what = format!("the {} lane on {name}", lane.name);
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        if lane.lupin
            && let Some(obs) = record_in(&text)
        {
            assert!(
                pre_mirror(&obs) || lupin_refuses,
                "{what}: a record where the image should have been replaced: {obs:?}"
            );
            assert_eq!(obs.verdict, "unsupported", "{what}: {obs:?}");
            eprintln!("PINNED {what}: lupin {} at {} unsupported", obs.version, obs.commit);
            continue;
        }
        assert!(
            record_in(&text).is_none(),
            "{what}: a record follows a successful exec: {text}"
        );
        assert_eq!(
            out.status.code(),
            Some(0),
            "{what}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(text, want, "{what} (s225)");
        eprintln!("PASS {what}");
    }
}

/// The open descriptors 3..19 of a control `sh` this harness starts
/// with the stdio a lane gets — what any process it starts inherits.
#[cfg(unix)]
fn control_open_set(dir: &Path) -> String {
    let out = run_fed(
        {
            let mut c = Command::new("/bin/sh");
            c.args([
                "-c",
                "n=; for f in 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19; do [ -e /dev/fd/$f ] && n=$n$f,; done; echo open=$n",
            ]);
            c
        },
        dir,
        "",
    );
    assert!(out.status.success(), "the control sh runs");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// argv as the kernel holds it for `pid`: `/proc/<pid>/cmdline` on
/// linux, `ps -o args=` elsewhere (macOS, freebsd).
#[cfg(unix)]
fn command_line(pid: u32) -> String {
    if cfg!(target_os = "linux") {
        let raw = std::fs::read(format!("/proc/{pid}/cmdline")).expect("cmdline");
        return raw
            .split(|&b| b == 0)
            .map(|w| String::from_utf8_lossy(w).into_owned())
            .collect::<Vec<_>>()
            .join(" ");
    }
    let out = Command::new("ps")
        .args(["-o", "args=", "-p", &pid.to_string()])
        .output()
        .expect("ps runs");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ------------------------------------------------ the corpus rows --

/// A corpus row: every wolfgang lane under `conform-run` prints `want`,
/// and lupin agrees or answers its pinned pre-mirror verdict.
fn every_lane_says(rel: &str, want: &str) {
    let entry = corpus(rel);
    let dir = scratch(&rel.replace(['/', '.'], "_"));
    for flag in ["--checked", "--native", "--release"] {
        let mut cmd = Command::new(wolf());
        cmd.arg("conform-run").arg(&entry).arg(flag).arg("--json");
        let out = run_fed(cmd, &dir, "");
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
            "the {flag} lane on {rel} (s225)"
        );
        eprintln!("PASS the {flag} lane on {rel}");
    }
    match sibling_lupin() {
        Some(l) => {
            let mut cmd = Command::new(l);
            cmd.arg("conform-run").arg(&entry).arg("--json");
            let obs = parse_obs(&run_fed(cmd, &dir, "").stdout, "lupin");
            if pre_mirror(&obs) {
                assert_eq!(
                    (obs.verdict.as_str(), obs.stdout.as_str()),
                    ("unsupported", ""),
                    "lupin {} at {} (pre-mirror, pinned) on {rel}",
                    obs.version,
                    obs.commit
                );
                eprintln!("PINNED lupin {} at {} on {rel} (pre-mirror)", obs.version, obs.commit);
            } else {
                assert_eq!(
                    (obs.verdict.as_str(), obs.stdout.as_str()),
                    ("exit(0)", want),
                    "lupin {} at {} on {rel} (s225 mirrored)",
                    obs.version,
                    obs.commit
                );
                eprintln!("PASS lupin {} at {} on {rel}", obs.version, obs.commit);
            }
        }
        None => no_sibling(),
    }
}

/// The exec rows that replace nothing: the shape is `invalid` before
/// anything else is read, and the program is still there. Red at trunk
/// a0169704: E0301 (`os_exec` unknown) on every wolfgang lane; lupin
/// 0.1.49 `unsupported`.
#[test]
fn an_exec_of_a_bad_shape_is_invalid_and_returns() {
    every_lane_says(
        "os/exec_rows.lu",
        "no_argv=invalid\nno_equals=invalid\nempty_name=invalid\nodd_map=invalid\n\
out_of_range=invalid\nrepeated=invalid\nbad_source=invalid\nstill_here\n",
    );
}

/// `env_unset` removes a variable: `missing` after it, absent
/// is fine, bad names `invalid`, a later set brings it back. Red at
/// trunk: E0301 (`env_unset` unknown); lupin 0.1.49 `unsupported`.
#[test]
fn env_unset_round_trips() {
    every_lane_says(
        "os/env_unset.lu",
        "got: tarn\nafter: <missing>\nabsent: ok\nempty: invalid\nequals: invalid\n\
again: fell\n",
    );
}

// ------------------------------------------- the unix fixtures --

/// The image is replaced and the process is not: the new `sh` prints the
/// pid this harness started, carries argv[0] exactly as given (read from
/// the kernel while the image waits on its input), receives two
/// arguments with a space intact, the one variable it was handed and not
/// the host's, a line of this harness's input (0 survives an exec), and
/// exactly the descriptors a control `sh` started here has — the file the
/// program opened and did not map is gone. `sh` is found by its bare name
/// along the PATH the program handed over.
#[cfg(unix)]
#[test]
fn exec_replaces_the_image_and_keeps_the_pid() {
    let entry = fixture("image.lu");
    let dir = scratch("image");
    let control = control_open_set(&dir);
    eprintln!("control descriptor set: {control}");
    for mut lane in lanes(&entry, &dir) {
        let what = format!("the {} lane on image.lu", lane.name);
        lane.cmd
            .current_dir(&dir)
            .env("WOLF_S225_HOST", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = lane.cmd.spawn().expect("spawn");
        let pid = child.id();
        let mut stdin = child.stdin.take().expect("stdin");
        let mut reader = BufReader::new(child.stdout.take().expect("stdout"));
        let mut seen = String::new();
        let mut image_pid = None;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).expect("read") == 0 {
                break;
            }
            seen.push_str(&line);
            if let Some(p) = line.trim().strip_prefix("pid=") {
                image_pid = Some(p.parse::<u32>().expect("a pid"));
                break;
            }
        }
        let Some(image_pid) = image_pid else {
            // No image: only a lupin that predates the mirror may say so,
            // and it says it with a record.
            drop(stdin);
            let _ = child.wait();
            let obs = record_in(&seen).unwrap_or_else(|| panic!("{what}: no pid, no record: {seen}"));
            assert!(
                lane.lupin && pre_mirror(&obs),
                "{what}: the image never ran: {obs:?}"
            );
            assert_eq!(obs.verdict, "unsupported", "{what}");
            eprintln!("PINNED {what}: lupin {} at {} (pre-mirror)", obs.version, obs.commit);
            continue;
        };
        assert_eq!(
            image_pid, pid,
            "{what}: the new image's pid is the process this harness started"
        );
        let argv = command_line(pid);
        assert!(
            argv.starts_with("wolf-s225-argv0 -c "),
            "{what}: argv[0] as given, read from the kernel: {argv:?}"
        );
        stdin.write_all(b"from-harness\n").expect("feed");
        drop(stdin);
        let mut rest = String::new();
        reader.read_to_string(&mut rest).expect("rest");
        seen.push_str(&rest);
        let status = child.wait().expect("wait");
        assert_eq!(status.code(), Some(0), "{what}");
        let want = format!(
            "before exec handle_above_2=true\npid={pid}\nstdin=from-harness\n\
zero=zero one=one two=two words\nenv=fell\nhost=[]\n{control}\n"
        );
        assert_eq!(seen, want, "{what} (s225)");
        eprintln!("PASS {what} (pid {pid})");
    }
}

/// The map at an exec: a file placed AS descriptor 5 is read there, and
/// a `-1` entry leaves 2 closed in the new image. lupin refuses it by
/// name.
#[cfg(unix)]
#[test]
fn exec_places_the_map() {
    every_lane_execs("fd_map.lu", "fd5=five-line\ntwo=closed\n", true);
}

/// The exec rows that return — a missing path, a bare name the handed
/// PATH cannot find, a file without an execute bit — and a failed exec
/// that had mapped a file onto 0 leaves 0 as it was: the line read from
/// it is this harness's.
#[cfg(unix)]
#[test]
fn a_failed_exec_answers_its_row_and_puts_the_process_back() {
    every_lane_prints(
        "fail.lu",
        "from-harness\n",
        "abs_missing=not_found\nbare_missing=not_found\ndenied=denied\n\
mapped_then_failed=not_found\nstdin=from-harness\n",
        false,
    );
}

/// pelt's `pending/unset_env`: a child no longer sees a variable the
/// program removed — one it set itself, and one it inherited.
#[cfg(unix)]
#[test]
fn a_child_no_longer_sees_a_removed_variable() {
    every_lane_prints(
        "unset_child.lu",
        "",
        "set=1\nunset=0\nhost=1\nhost_unset=0\nset_again=1\n",
        false,
    );
}
