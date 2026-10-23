//! s260 (wolf-lang#682, `[os.proc.exec]`) — a file without the `#!`
//! line the kernel needs is `io` with `ENOEXEC` in `os_error()`, on
//! every spawn and on `os_exec`, on every machine; nothing re-runs it
//! under `/bin/sh`.
//!
//! Measured at the control, wolf 0.2.34 (`9da7bdc`, kasumi, in an
//! archlinux container — `docs/s260-the-three-wrong-answers.md`): every
//! spawn that installs a `pre_exec` hook RAN the `#!`-less file
//! (`os_spawn_fds` with a map, `os_spawn_job`, and `os_spawn` once the
//! program ignores `BROKEN_PIPE`, which hooks it too), and `junk` got
//! bash's own `cannot execute binary file`; the spawns with no hook and
//! `os_exec` answered `io`; `os_error()` was 0 after all of them. The
//! checked machine did the same. The fallback was the C library's:
//! the standard library execs a hooked spawn with `execvp`, which runs
//! an `ENOEXEC` file under `/bin/sh` (`execvp(3)`).
//!
//! lupin 0.1.55 (record commit `e642f26`, build pin `294d626`) answers
//! `io` for every spawn but leaves `os_error()` 0, and its `os_exec` RUNS the file (it
//! replaces itself through `CommandExt::exec`, the same `execvp`): both
//! pinned by version and commit, wolf-interp's to mirror. It declines
//! `os_signal_ignore` by name (no `unsafe`), a posture.
//!
//! The fixtures spawn host programs (`sh` through the control), so this
//! runs on unix only: native and release (and native again under
//! `taskset -c 0-3` on linux), the checked machine and lupin under
//! `conform-run`, each in a fresh directory holding the three files.

#![cfg(unix)]

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const ISSUE: &str = "wolf-lang#682";

fn ensure_rt_staticlib() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .expect("cargo builds wolf_rt");
        assert!(status.success(), "wolf_rt staticlib build failed");
    });
}

/// A fresh directory with `main.lu` (the fixture) and the issue's three
/// files: `noshebang` (writes `ran-marker` if anything runs it), `junk`
/// (four NUL bytes) and `control` (`#!/bin/sh`, `exit 7`).
fn stage(fixture: &str, label: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("spawn_noexec_lanes")
        .join(format!("{fixture}-{label}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/spawn_noexec")
            .join(format!("{fixture}.lu")),
        d.join("main.lu"),
    )
    .unwrap();
    std::fs::write(d.join("noshebang"), "echo ran > ran-marker\n").unwrap();
    std::fs::write(d.join("junk"), b"\0\0\0\0").unwrap();
    std::fs::write(d.join("control"), "#!/bin/sh\nexit 7\n").unwrap();
    for f in ["noshebang", "junk", "control"] {
        std::fs::set_permissions(d.join(f), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    d
}

fn run_in(mut cmd: Command, dir: &Path) -> Output {
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.output().expect("spawn")
}

/// The environment skip, strictly (#550, #571): an exit 2 is a skip
/// only when the host cannot run the tier; a compile error never is.
fn unavailable(out: &Output, tier: &str) -> bool {
    if !lane_exit::environment_refusal(out, tier) {
        return false;
    }
    let msg = String::from_utf8_lossy(&out.stderr);
    assert!(
        msg.contains("release tier targets") || msg.contains("cannot run the release tier"),
        "{tier}: a compile error is not a skip: {msg}"
    );
    true
}

/// What the program printed on each wolfgang lane: (lane, stdout).
fn wolfgang_runs(fixture: &str) -> Vec<(String, String)> {
    ensure_rt_staticlib();
    let mut seen = Vec::new();
    for tier in ["native", "release"] {
        let dir = stage(fixture, tier);
        let mut b = Command::new(wolf());
        b.args(["build", "main.lu", "-o", tier]);
        if tier == "release" {
            b.arg("--release");
        }
        let out = run_in(b, &dir);
        if unavailable(&out, tier) {
            eprintln!("SKIP: {fixture} {tier}: the host cannot run the tier");
            continue;
        }
        assert!(out.status.success(), "{fixture} {tier} build: {out:?}");
        let run = run_in(Command::new(dir.join(tier)), &dir);
        assert!(run.status.success(), "{fixture} {tier}: {run:?}");
        seen.push((
            tier.to_string(),
            String::from_utf8_lossy(&run.stdout).into_owned(),
        ));
        if tier == "native" && cfg!(target_os = "linux") && Path::new("/usr/bin/taskset").exists() {
            let again = stage(fixture, "taskset");
            std::fs::copy(dir.join(tier), again.join(tier)).unwrap();
            let mut t = Command::new("taskset");
            t.args(["-c", "0-3"]).arg(again.join(tier));
            let run = run_in(t, &again);
            assert!(
                run.status.success(),
                "{fixture} native under taskset: {run:?}"
            );
            seen.push((
                "native, taskset -c 0-3".into(),
                String::from_utf8_lossy(&run.stdout).into_owned(),
            ));
        }
    }
    let dir = stage(fixture, "checked");
    let mut c = Command::new(wolf());
    c.args(["conform-run", "--json", "--checked", "main.lu"]);
    let out = run_in(c, &dir);
    assert!(out.status.success(), "{fixture} checked: {out:?}");
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{fixture} checked record: {e}: {out:?}"));
    assert_eq!(rec["verdict"], "exit(0)", "{fixture} checked: {rec}");
    seen.push((
        "checked".into(),
        rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    ));
    seen
}

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

/// lupin's version line and `conform-run` output in a staged directory,
/// or `None` with no sibling (`WOLF_PAIRING_REQUIRE_SIBLING` makes that
/// a failure, r10/#253).
fn lupin_run(fixture: &str) -> Option<(String, Output, PathBuf)> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found — \
             the oracle leg of {ISSUE}'s gate did not run"
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        return None;
    };
    let version = String::from_utf8_lossy(
        &Command::new(&lupin)
            .arg("--version")
            .output()
            .expect("lupin runs")
            .stdout,
    )
    .into_owned();
    let dir = stage(fixture, "lupin");
    let mut c = Command::new(&lupin);
    c.args(["conform-run", "main.lu", "--json"]);
    let out = run_in(c, &dir);
    Some((version, out, dir))
}

const ROWS: &str = "\
./noshebang spawn_fds[1,1]: io os_error=8
./noshebang spawn_fds[]: io os_error=8
./noshebang spawn: io os_error=8
./noshebang spawn_job[1,1]: io os_error=8
./junk spawn_fds[1,1]: io os_error=8
./junk spawn_fds[]: io os_error=8
./junk spawn: io os_error=8
./junk spawn_job[1,1]: io os_error=8
./control spawn_fds[1,1]: RAN status 7
./control spawn_fds[]: RAN status 7
./control spawn: RAN status 7
./control spawn_job[1,1]: RAN status 7
marker=false
";

/// lupin 0.1.55: every spawn `io`, but `os_error()` left 0 (kasumi, s260).
const ROWS_LUPIN_0_1_55: &str = "\
./noshebang spawn_fds[1,1]: io os_error=0
./noshebang spawn_fds[]: io os_error=0
./noshebang spawn: io os_error=0
./noshebang spawn_job[1,1]: io os_error=0
./junk spawn_fds[1,1]: io os_error=0
./junk spawn_fds[]: io os_error=0
./junk spawn: io os_error=0
./junk spawn_job[1,1]: io os_error=0
./control spawn_fds[1,1]: RAN status 7
./control spawn_fds[]: RAN status 7
./control spawn: RAN status 7
./control spawn_job[1,1]: RAN status 7
marker=false
";

/// The issue's call and its family, on the issue's files.
#[test]
fn a_file_with_no_shebang_is_io_on_every_spawn_and_every_lane() {
    for (lane, stdout) in wolfgang_runs("spawn_rows") {
        assert_eq!(stdout, ROWS, "{lane} ({ISSUE})");
    }
    let Some((version, out, _)) = lupin_run("spawn_rows") else {
        return;
    };
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("lupin's record: {e}: {out:?}"));
    let got = rec["stdout_inline"].as_str().unwrap_or("");
    assert_eq!(rec["verdict"], "exit(0)", "lupin: {rec}");
    if got == ROWS {
        return;
    }
    assert!(
        rec["impl_version"] == "0.1.55" && rec["commit"] == "e642f26" && got == ROWS_LUPIN_0_1_55,
        "lupin {version}'s rows: {got:?} (want the clause's, or 0.1.55's pinned rows)"
    );
}

/// A plain `os_spawn` that a hook reaches (the program ignores
/// `BROKEN_PIPE`): `io`, never `/bin/sh`.
#[test]
fn a_hooked_plain_spawn_is_io_too() {
    const WANT: &str = "./noshebang spawn, BROKEN_PIPE ignored: io os_error=8\nmarker=false\n";
    for (lane, stdout) in wolfgang_runs("pipe_ignored") {
        assert_eq!(stdout, WANT, "{lane} ({ISSUE})");
    }
    let Some((version, out, _)) = lupin_run("pipe_ignored") else {
        return;
    };
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("lupin's record: {e}: {out:?}"));
    if rec["verdict"] == "exit(0)" && rec["stdout_inline"] == WANT {
        return;
    }
    assert_eq!(rec["verdict"], "unsupported", "lupin {version}: {rec}");
    assert!(
        rec["x-unsupported"]
            .as_str()
            .is_some_and(|m| m.contains("os_signal_ignore")),
        "lupin {version} declines the disposition by name: {rec}"
    );
}

/// `os_exec` on the `#!`-less file: `io`, `ENOEXEC`, the process still
/// this one.
#[test]
fn os_exec_on_a_file_with_no_shebang_is_io_with_enoexec() {
    const WANT: &str = "exec: io os_error=8 marker=false\n";
    for (lane, stdout) in wolfgang_runs("exec_row") {
        assert_eq!(stdout, WANT, "{lane} ({ISSUE})");
    }
    let Some((version, out, dir)) = lupin_run("exec_row") else {
        return;
    };
    if let Ok(rec) = serde_json::from_slice::<serde_json::Value>(&out.stdout)
        && rec["verdict"] == "exit(0)"
        && rec["stdout_inline"] == WANT
    {
        return;
    }
    // lupin 0.1.55 (its `--version` names build pin `294d626`; the
    // records say `e642f26`) replaces itself with `/bin/sh noshebang`: no
    // record, the marker written.
    assert!(
        version.contains("lupin 0.1.55 ")
            && version.contains("294d626")
            && out.stdout.is_empty()
            && dir.join("ran-marker").exists(),
        "lupin {version}'s exec: {out:?} (want the clause's answer, or 0.1.55's pinned run)"
    );
}
