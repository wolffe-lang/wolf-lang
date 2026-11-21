//! wolf-lang#728: a `select`'s timeout beside a proc parked in a host
//! read, on the checked machine.
//!
//! `fixtures/checked_kill_parked_read` is lobo's `body_join_kill.lu`
//! shape: a proc parks in a read nobody answers (its own deadline 60 s),
//! the root waits in `select { exit(…) from m, timeout(300.ms) }`, kills
//! the proc, joins it and asks that nothing more arrives from it; then a
//! proc that ends on its own is joined. native and release print the
//! three lines below in about a second.
//!
//! The checked machine's `timeout` arms run on a virtual clock
//! (`[exec.checked.sched]`). At 0.2.40 that clock moved only when no task
//! was ready, and a task waiting in a host read is ready again every
//! millisecond (`[exec.checked.task]`), so the 300 ms arm never fired:
//! the read ran out its 60 s deadline, the proc wrote and ended, and the
//! select took the exit arm (`parked: joined true`, `killed false`). On
//! linux under a 45 or 60 s ceiling that read as a stall. While a task
//! waits on host I/O the clock now follows real elapsed time.
//!
//! The gate runs the program through `wolf conform-run --checked --json`
//! under a 20 s deadline (again on four CPUs of the allocation on linux)
//! and through the native tier (60 s, its build included). A run past its
//! deadline is killed by its own child handle and FAILS. lupin is not a
//! machine here: its loopback read does not park as the host's does, so
//! its proc ends on its own before the patience runs out.

mod lane_exit;
mod test_cpus;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const WANT: &str = "parked: joined false\n\
                    ended: killed true gone true quiet-after-join true\n\
                    after: ok true\n";

/// The checked machine answers in about a second once its clock follows
/// the read; at 0.2.40 it answered only after the read's own 60 s.
const CHECKED_LIMIT: Duration = Duration::from_secs(20);

/// The native tier compiles and links the program first.
const NATIVE_LIMIT: Duration = Duration::from_secs(60);

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

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

fn entry() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/checked_kill_parked_read/main.lu")
}

/// Wait for `child` until `limit`; past it the child is killed by its
/// own handle. `None` is that stall.
fn finish(mut child: Child, limit: Duration) -> (Option<Output>, Duration) {
    let t0 = Instant::now();
    loop {
        if child.try_wait().expect("try_wait").is_some() {
            break;
        }
        if t0.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return (None, t0.elapsed());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let status = child.wait().expect("wait");
    let elapsed = t0.elapsed();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_end(&mut stdout);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_end(&mut stderr);
    }
    (
        Some(Output {
            status,
            stdout,
            stderr,
        }),
        elapsed,
    )
}

/// One machine's answer: `Ok(None)` when the host cannot run that tier
/// (a loud SKIP, refused under `WOLF_TEST_NATIVE_REQUIRE`), `Err` for a
/// wrong answer, a run past `limit` or one with no record.
fn answer(mut cmd: Command, what: &str, limit: Duration) -> Result<Option<()>, String> {
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run wolf");
    let (out, elapsed) = finish(child, limit);
    let Some(out) = out else {
        let why = format!("{what}: no answer in {elapsed:?}; killed at its deadline (a stall)");
        eprintln!("REPORT {why}");
        return Err(why);
    };
    if what.starts_with("--native") && lane_exit::environment_refusal(&out, what) {
        assert!(
            std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
            "required {what} unavailable: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        eprintln!("SKIP: {what}: {}", String::from_utf8_lossy(&out.stderr));
        return Ok(None);
    }
    if !out.status.success() {
        return Err(format!(
            "{what}: {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("{what}: {e}: {}", String::from_utf8_lossy(&out.stdout)))?;
    eprintln!(
        "REPORT {what}: {} {:?} in {elapsed:?}",
        rec["verdict"], rec["stdout_inline"]
    );
    if rec["verdict"] != "exit(0)" || rec["stdout_inline"] != WANT {
        return Err(format!("{what}: {rec}"));
    }
    Ok(Some(()))
}

fn conform(machine: &str) -> Command {
    let mut cmd = Command::new(wolf());
    cmd.args(["conform-run", "--json", machine]).arg(entry());
    cmd
}

/// #728: the timeout fires beside the parked read; the kill and the join
/// follow; every machine prints native's three lines.
#[test]
fn a_timeout_fires_beside_a_parked_read() {
    let mut wrong = Vec::new();
    let mut runs: Vec<(String, Command, Duration)> =
        vec![("--checked".into(), conform("--checked"), CHECKED_LIMIT)];
    if cfg!(target_os = "linux")
        && let Some(pin) = test_cpus::pin(4, "a_timeout_fires_beside_a_parked_read")
    {
        let mut cmd = pin.command(wolf());
        cmd.args(["conform-run", "--json", "--checked"])
            .arg(entry());
        runs.push((
            format!("--checked under {}", pin.label()),
            cmd,
            CHECKED_LIMIT,
        ));
    }
    ensure_rt_staticlib();
    runs.push(("--native".into(), conform("--native"), NATIVE_LIMIT));
    for (what, cmd, limit) in runs {
        if let Err(why) = answer(cmd, &what, limit) {
            wrong.push(why);
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
