//! s266 (wolf-lang#936, `[os.fs.error]`) — every spawn call and a failed
//! `os_exec` carry the host's number in `os_error()` beside the row, on
//! native, release and the checked machine, for each reason a program
//! cannot run: a missing name (`ENOENT`), arguments past the host's
//! ceiling (`E2BIG`), a file with no `#!` line (`ENOEXEC`), a file without
//! an execute bit and a directory (`EACCES`), and a file another
//! descriptor holds open for writing (`ETXTBSY`, linux only); and a bare
//! name's search, missing (`ENOENT`) or finding only a file without an
//! execute bit (`EACCES`), on the spawns and on `os_exec` alike.
//!
//! pelt's sh26 asked for it (pelt#68, witness
//! `notes/sh26-evidence/probe/wolf-spawn-errno/`): at wolf 0.2.35 every
//! one of these answered its row with `os_error()` 0, so a shell could
//! not say `Text file busy` the way dash does. s260 (#682) made the
//! native runtime and the spawn family on the checked machine keep the
//! number; the checked machine's `os_exec` still decided a name holding
//! `/` by its own look before the exec (so its print buffers stay in the
//! record) and left 0 for a missing file, a directory and a file without
//! an execute bit, where native said 2 and 13; and `os_exec`'s own `PATH`
//! search left 0 on every machine where the spawns' `execvp` said 2 and
//! 13 (R-s266-3: the search answers what `execvp(3)` reports). Measured
//! by s266 on kasumi (linux 7.2.8, in the lane's container) and nomad
//! (macOS 26.4, APFS): the numbers are the same on both hosts, and macOS
//! runs a file held open for writing, so its `busy` row spawns and its
//! `os_exec` replaces the program (`notes/s266-evidence/spawn-errno/`).
//!
//! lupin is not run here: its spawns leave `os_error()` 0 at 0.1.56
//! (`spawn_noexec_lanes.rs` pins that by version); the mirror is
//! wolf-interp's.

#![cfg(unix)]

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const CALLS: [&str; 5] = [
    "os_spawn",
    "os_spawn_with",
    "os_spawn_fds",
    "os_spawn_job",
    "os_exec",
];

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

/// A fresh directory: `main.lu` (the fixture) beside `w/`, which holds
/// the reasons — `nox` (no execute bit), `dd` (a directory), `bin` (two
/// NUL bytes and no `#!` line, executable) and `busy` (a `#!/bin/sh`
/// script, executable, that the caller holds open for writing).
fn stage(label: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("spawn_errno_lanes")
        .join(label);
    let _ = std::fs::remove_dir_all(&d);
    let w = d.join("w");
    std::fs::create_dir_all(w.join("dd")).unwrap();
    std::fs::create_dir_all(w.join("p")).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/spawn_errno/main.lu"),
        d.join("main.lu"),
    )
    .unwrap();
    std::fs::write(w.join("nox"), "echo hi\n").unwrap();
    std::fs::write(w.join("bin"), b"\0\0binary\n").unwrap();
    std::fs::write(w.join("busy"), "#!/bin/sh\necho ran >&2\n").unwrap();
    for f in ["bin", "busy"] {
        std::fs::set_permissions(w.join(f), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(w.join("p/noxcmd"), "echo hi\n").unwrap();
    for f in ["nox", "p/noxcmd"] {
        std::fs::set_permissions(w.join(f), std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    d
}

/// The writer `busy`'s `ETXTBSY` needs: open for appending while the
/// probe runs (close-on-exec, so no child inherits it; the open file is
/// what the kernel counts).
fn hold_open(dir: &Path) -> std::fs::File {
    std::fs::OpenOptions::new()
        .append(true)
        .open(dir.join("w/busy"))
        .expect("open busy for writing")
}

/// Every call's line for one reason: the row and the number with its
/// text, or a spawn that ran.
fn rows(out: &mut String, label: &str, calls: &[&str], answer: &str) {
    for call in calls {
        out.push_str(&format!("{call} {label}: {answer}\n"));
    }
}

/// What this host answers, line for line ([`stage`]'s reasons in the
/// fixture's order, `busy` last). On macOS `busy` runs: its four spawns
/// exit 0, and its `os_exec` replaces the program, so nothing follows.
fn expected() -> String {
    let mut s = String::new();
    rows(&mut s, "ok", &CALLS[..4], "spawned os_error 0 status 0");
    rows(
        &mut s,
        "missing",
        &CALLS,
        "not_found os_error 2 'No such file or directory'",
    );
    rows(
        &mut s,
        "noexec",
        &CALLS,
        "io os_error 8 'Exec format error'",
    );
    rows(
        &mut s,
        "nox",
        &CALLS,
        "denied os_error 13 'Permission denied'",
    );
    rows(
        &mut s,
        "dir",
        &CALLS,
        "denied os_error 13 'Permission denied'",
    );
    rows(
        &mut s,
        "big",
        &CALLS,
        "io os_error 7 'Argument list too long'",
    );
    rows(
        &mut s,
        "bare-missing",
        &CALLS,
        "not_found os_error 2 'No such file or directory'",
    );
    rows(
        &mut s,
        "bare-nox",
        &CALLS,
        "denied os_error 13 'Permission denied'",
    );
    if cfg!(target_os = "macos") {
        rows(&mut s, "busy", &CALLS[..4], "spawned os_error 0 status 0");
    } else {
        rows(&mut s, "busy", &CALLS, "io os_error 26 'Text file busy'");
    }
    s
}

/// The probe's `PATH`: the bare-name rows' candidates, then the host's
/// `sh` and `true` (`os_exec` hands the new program the same list).
const PROBE_PATH: &str = "./p:/usr/bin:/bin";

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

/// A diff-friendly comparison: the first line that differs, then all.
fn same(lane: &str, got: &str, want: &str) {
    if got == want {
        return;
    }
    let first = got
        .lines()
        .zip(want.lines())
        .find(|(g, w)| g != w)
        .map(|(g, w)| format!("got  {g:?}\nwant {w:?}"))
        .unwrap_or_else(|| "one is a prefix of the other".to_string());
    panic!("{lane}: os_error() rows differ\n{first}\n--- got\n{got}--- want\n{want}");
}

#[test]
fn every_spawn_and_a_failed_exec_carry_the_hosts_number() {
    ensure_rt_staticlib();
    let want = expected();
    let mut ran = 0;
    for tier in ["native", "release"] {
        let dir = stage(tier);
        let mut b = Command::new(wolf());
        b.args(["build", "main.lu", "-o", tier]);
        if tier == "release" {
            b.arg("--release");
        }
        let out = run_in(b, &dir);
        if unavailable(&out, tier) {
            eprintln!("SKIP: {tier}: the host cannot run the tier");
            continue;
        }
        assert!(out.status.success(), "{tier} build: {out:?}");
        let writer = hold_open(&dir);
        let mut probe = Command::new(dir.join(tier));
        probe.env("PATH", PROBE_PATH);
        let run = run_in(probe, &dir.join("w"));
        drop(writer);
        assert!(run.status.success(), "{tier}: {run:?}");
        same(tier, &String::from_utf8_lossy(&run.stdout), &want);
        ran += 1;
        eprintln!("s266 {tier}: every row's number, PASS");
    }
    // The checked machine answers through `conform-run --checked` (never
    // `wolf run`). A failed `os_exec` writes its print buffers to
    // descriptor 1 first (`[os.proc.exec]`), so the program's text is
    // every line before the record plus the record's `stdout_inline`;
    // an exec that succeeds (macOS's `busy`) leaves no record at all.
    let dir = stage("checked");
    let writer = hold_open(&dir);
    let mut c = Command::new(wolf());
    c.args(["conform-run", "--checked", "--json", "../main.lu"])
        .env("PATH", PROBE_PATH);
    let run = run_in(c, &dir.join("w"));
    drop(writer);
    assert!(run.status.success(), "checked: {run:?}");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    let mut lines: Vec<&str> = stdout.lines().collect();
    let record: Option<serde_json::Value> = lines
        .last()
        .and_then(|l| serde_json::from_str(l).ok())
        .filter(|r: &serde_json::Value| r.get("verdict").is_some());
    let mut got = String::new();
    if record.is_some() {
        lines.pop();
    }
    for l in &lines {
        got.push_str(l);
        got.push('\n');
    }
    match &record {
        Some(rec) => {
            assert!(
                cfg!(not(target_os = "macos")),
                "checked: macOS's busy exec should have replaced the machine: {rec}"
            );
            assert_eq!(rec["verdict"], "exit(0)", "checked: {rec}");
            got.push_str(rec["stdout_inline"].as_str().unwrap_or(""));
        }
        None => assert!(
            cfg!(target_os = "macos"),
            "checked: no record on linux: {stdout}"
        ),
    }
    same("checked", &got, &want);
    eprintln!("s266 checked: every row's number, PASS");
    eprintln!("s266: {ran} of the two native tiers ran");
}
