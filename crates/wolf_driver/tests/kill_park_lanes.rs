//! s260 (wolf-lang#683, `[conc.proc.kill]` step 1) — a killed proc
//! parked at any of the runtime's parking sites runs no further user
//! code: no `else` arm, no statement after the park, no caller one frame
//! up; the join answers `killed`.
//!
//! Measured at the control, wolf 0.2.34 (`9da7bdc`, kasumi, in an
//! archlinux container, `docs/s260-the-three-wrong-answers.md`), native
//! and release alike: the reactor's waits (`net_read`, `net_accept`,
//! `net_write`) and the signal hub's (`os_signal_wait`,
//! `os_signal_wait_fds`) came back on their failure arm and ran on; a
//! proc's `join` answered `cancelled` to its `else`; a channel wait and a
//! `select` one frame down returned that frame (the lowered teardown
//! branch) and the caller ran on; `time_sleep_ms` slept its whole
//! duration and the proc ran on after it, the kill waiting it out; a
//! `scope { }` block's join returned into the code after the block. The
//! waits already right were a receive and a `when` in the entry frame.
//! The checked machine and lupin 0.1.55 ran none of it.
//!
//! One fixture per site (`fixtures/kill_park/`), each printing what the
//! clause forbids should the proc run on, and `join: …; kill prompt|slow`
//! from `main` (the kill blocks until the proc is gone). `os_wait` is the
//! site no kill can end — the host's `waitpid` holds the child table —
//! so its proc leaves when the child does and the kill is `slow`
//! (R-s260-2); the checked machine and lupin serialize a host wait with
//! the program, so their proc ends before the kill (`normal`), which is
//! their recorded posture, not a defect. Native runs again under
//! `taskset -c 0-3` on linux. Unix only: the `os_wait` row starts the
//! host's `sleep`, and windows' parks are held to the leave by
//! `wolf_rt::task::leave`'s own tests, which run there.

#![cfg(unix)]

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const ISSUE: &str = "wolf-lang#683";

const PROMPT: &str = "join: killed; kill prompt\n";

/// (fixture, the compiled tiers' answer, the checked machine's and
/// lupin's answer).
const ROWS: &[(&str, &str, &str)] = &[
    (
        "net_read",
        "join: killed; the pipe: (nothing on the pipe)\n",
        "join: killed; the pipe: (nothing on the pipe)\n",
    ),
    ("net_read_helper", PROMPT, PROMPT),
    ("net_accept", PROMPT, PROMPT),
    ("net_write", PROMPT, PROMPT),
    ("chan_recv_entry", PROMPT, PROMPT),
    ("chan_recv_helper", PROMPT, PROMPT),
    ("select_helper", PROMPT, PROMPT),
    ("signal_wait", PROMPT, PROMPT),
    ("signal_wait_fds", PROMPT, PROMPT),
    ("sleep", PROMPT, PROMPT),
    ("proc_join", PROMPT, PROMPT),
    ("scope_join", PROMPT, PROMPT),
    ("when_acquire", PROMPT, PROMPT),
    (
        "os_wait",
        "join: killed; kill slow\n",
        "after-park-ran 0\njoin: normal; kill prompt\n",
    ),
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

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/kill_park")
        .join(format!("{name}.lu"))
}

fn scratch(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("kill_park_lanes")
        .join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::copy(fixture(name), d.join("main.lu")).unwrap();
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

fn record_stdout(out: &Output, what: &str) -> String {
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{what} record: {e}: {out:?}"));
    assert_eq!(rec["verdict"], "exit(0)", "{what}: {rec}");
    rec["stdout_inline"].as_str().unwrap_or("").to_string()
}

/// Every lane's answer for one site.
fn every_lane(name: &str, compiled: &str, machine: &str) {
    ensure_rt_staticlib();
    let dir = scratch(name);
    for tier in ["native", "release"] {
        let mut b = Command::new(wolf());
        b.args(["build", "main.lu", "-o", tier]);
        if tier == "release" {
            b.arg("--release");
        }
        let out = run_in(b, &dir);
        if unavailable(&out, tier) {
            eprintln!("SKIP: {name} {tier}: the host cannot run the tier");
            continue;
        }
        assert!(out.status.success(), "{name} {tier} build: {out:?}");
        let run = run_in(Command::new(dir.join(tier)), &dir);
        assert!(run.status.success(), "{name} {tier}: {run:?}");
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            compiled,
            "{name} on {tier} ({ISSUE})"
        );
        if tier == "native" && cfg!(target_os = "linux") && Path::new("/usr/bin/taskset").exists() {
            let mut t = Command::new("taskset");
            t.args(["-c", "0-3"]).arg(dir.join(tier));
            let run = run_in(t, &dir);
            assert!(run.status.success(), "{name} under taskset: {run:?}");
            assert_eq!(
                String::from_utf8_lossy(&run.stdout),
                compiled,
                "{name} on native under taskset -c 0-3 ({ISSUE})"
            );
        }
    }
    let mut c = Command::new(wolf());
    c.args(["conform-run", "--json", "--checked", "main.lu"]);
    let out = run_in(c, &dir);
    assert!(out.status.success(), "{name} checked: {out:?}");
    assert_eq!(
        record_stdout(&out, "the checked machine"),
        machine,
        "{name} on the checked machine"
    );
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found — \
             the reference leg of {ISSUE}'s gate did not run"
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the reference leg is absent");
        return;
    };
    let mut l = Command::new(lupin);
    l.args(["conform-run", "main.lu", "--json"]);
    let out = run_in(l, &dir);
    assert_eq!(
        record_stdout(&out, "lupin"),
        machine,
        "{name} on lupin, the reference"
    );
}

macro_rules! site {
    ($test:ident, $name:literal) => {
        #[test]
        fn $test() {
            let (_, compiled, machine) = ROWS
                .iter()
                .find(|(n, _, _)| *n == $name)
                .expect("a row per site");
            every_lane($name, compiled, machine);
        }
    };
}

site!(the_issues_witness_net_read, "net_read");
site!(net_read_one_frame_down, "net_read_helper");
site!(net_accept, "net_accept");
site!(net_write, "net_write");
site!(a_receive_in_the_entry_frame, "chan_recv_entry");
site!(a_receive_one_frame_down, "chan_recv_helper");
site!(a_select_one_frame_down, "select_helper");
site!(os_signal_wait, "signal_wait");
site!(os_signal_wait_fds, "signal_wait_fds");
site!(a_sleep, "sleep");
site!(a_procs_join, "proc_join");
site!(a_scope_join, "scope_join");
site!(a_when_acquisition, "when_acquire");
site!(os_wait_leaves_when_the_child_ends, "os_wait");

/// Every fixture has a row and every row a fixture.
#[test]
fn every_site_has_a_row() {
    let mut have: Vec<String> = std::fs::read_dir(fixture("net_read").parent().unwrap())
        .unwrap()
        .map(|e| {
            e.unwrap()
                .path()
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    have.sort();
    let mut rows: Vec<String> = ROWS.iter().map(|(n, _, _)| n.to_string()).collect();
    rows.sort();
    assert_eq!(have, rows);
}
