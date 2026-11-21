//! wolf-lang#718: a program that dials more connections than its
//! listener's backlog holds before it accepts them, on the checked
//! machine.
//!
//! The witness is `waitwolf loop 0 N` (parked_tasks.rs's fixture, from
//! wolf-lang#713): one task listens and accepts while another spawns N
//! dialers, each of which connects, says hello and waits for the reply.
//! The checked machine runs one task at a time (`[exec.checked.task]`)
//! from a first-in-first-out ready queue (`[exec.checked.sched]`), so
//! the dialers make their dials before the accepting task runs again.
//! The listener's backlog is 128; past it the kernel holds a dial until
//! an accept makes room. At 0.2.40 `net_connect` was one blocking
//! connect on the thread that holds the machine, so that dial held the
//! whole machine and the accept that would have made room never ran: the
//! issue saw N <= 129 pass and N = 130 stop. native and release pass
//! N = 1000; their dial waits on a worker thread of its own.
//!
//! The gate runs `loop 0 130` through `wolf conform-run --checked --json`
//! under a 20 s deadline and through the native tier (60 s, its build
//! included). A run past its deadline is killed by its own child handle
//! and FAILS the gate. Both must print the witness's line.
//!
//! A dial beside other tasks makes its connect on a host thread of its
//! own. The second gate (unix; the checked machine) bounds that thread:
//! `fixtures/checked_net_dials/dial_killed.lu` kills a proc while its
//! dial waits on a listener nobody accepts from, and returns with the
//! listener still open, so the connect is still waiting when the run
//! ends. The run must end at once with `join: killed; kill prompt`; one
//! that waited for the connect would wait out the kernel's connect
//! timeout (75 s or more) and be killed at the deadline. windows is left
//! out: its checked `net_listen_with` takes std's backlog, so nothing
//! holds the dial there.

mod lane_exit;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Past the 128-connection backlog, by two.
const N: usize = 130;

/// The checked machine answers in seconds once its dial waits beside the
/// other tasks; at 0.2.40 it never answered.
const CHECKED_LIMIT: Duration = Duration::from_secs(20);

/// The native tier compiles and links the witness first.
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

/// Raise this process's open-file limit as far as the host allows; the
/// programs run below inherit it. `loop 0 130` holds 261 sockets, past
/// macOS's default of 256.
// `rlim_t` is u64 on linux and macOS and i64 on the BSDs.
#[allow(clippy::unnecessary_cast)]
fn raise_fd_limit() -> u64 {
    #[cfg(unix)]
    {
        let mut r = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: a valid out-pointer for getrlimit/setrlimit.
        unsafe {
            libc::getrlimit(libc::RLIMIT_NOFILE, &mut r);
            let want = r.rlim_max.min(10_240);
            if want > r.rlim_cur {
                let raised = libc::rlimit {
                    rlim_cur: want,
                    rlim_max: r.rlim_max,
                };
                libc::setrlimit(libc::RLIMIT_NOFILE, &raised);
            }
            libc::getrlimit(libc::RLIMIT_NOFILE, &mut r);
        }
        r.rlim_cur as u64
    }
    #[cfg(not(unix))]
    {
        u64::MAX
    }
}

/// The witness with its arguments fixed to `loop 0 n` (conform-run takes
/// no program arguments), in a scratch directory of its own.
fn stage(n: usize) -> PathBuf {
    let src =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parked_tasks/waitwolf/main.lu");
    let body = std::fs::read_to_string(&src).expect("read the witness");
    let from = "    let args = env_args()";
    assert!(body.contains(from), "the witness's argument line moved");
    let body = body.replacen(
        from,
        &format!("    let args = [\"loop\", \"0\", \"{n}\"]"),
        1,
    );
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("checked_net_dials")
        .join(format!("waitwolf_{n}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, body).expect("write the witness");
    entry
}

/// The witness's last line for `loop 0 n`.
fn want(n: usize) -> String {
    format!(
        "waitwolf loop: {n} connections at once, {n} requests answered, {n} answered right, \
         {n} ended as they should"
    )
}

fn last_line(s: &str) -> &str {
    s.trim_end().lines().last().unwrap_or("")
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

/// One machine's conform-run record for `entry`: `Ok(None)` when the
/// host cannot run that tier (a loud SKIP, refused under
/// `WOLF_TEST_NATIVE_REQUIRE`), `Err` for a run past `limit` or one with
/// no record.
fn conform(
    entry: &Path,
    machine: &str,
    limit: Duration,
) -> Result<Option<serde_json::Value>, String> {
    let child = Command::new(wolf())
        .args(["conform-run", "--json", machine])
        .arg(entry)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run wolf");
    let (out, elapsed) = finish(child, limit);
    let Some(out) = out else {
        let why = format!("{machine}: no answer in {elapsed:?}; killed at its deadline (a stall)");
        eprintln!("REPORT {why}");
        return Err(why);
    };
    if machine != "--checked" && lane_exit::environment_refusal(&out, machine) {
        assert!(
            std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
            "required {machine} unavailable: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        eprintln!("SKIP: {machine}: {}", String::from_utf8_lossy(&out.stderr));
        return Ok(None);
    }
    if !out.status.success() {
        return Err(format!(
            "{machine}: {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("{machine}: {e}: {}", String::from_utf8_lossy(&out.stdout)))?;
    eprintln!(
        "REPORT {machine}: {} {:?} in {elapsed:?}",
        rec["verdict"],
        last_line(rec["stdout_inline"].as_str().unwrap_or(""))
    );
    Ok(Some(rec))
}

/// #718: 130 dials, then 130 accepts. The checked machine answers as the
/// native tier does, inside its deadline. Both machines run before the
/// verdict, so a red run shows each one's answer.
#[test]
fn dials_past_the_backlog_before_accept() {
    let fds = raise_fd_limit();
    assert!(
        fds >= 512,
        "the witness needs 261 sockets; open-file limit {fds}"
    );
    let entry = stage(N);
    let mut wrong = Vec::new();
    for (machine, limit) in [("--checked", CHECKED_LIMIT), ("--native", NATIVE_LIMIT)] {
        if machine == "--native" {
            ensure_rt_staticlib();
        }
        match conform(&entry, machine, limit) {
            Err(why) => wrong.push(why),
            Ok(None) => {}
            Ok(Some(rec)) => {
                let line = last_line(rec["stdout_inline"].as_str().unwrap_or(""));
                if rec["verdict"] != "exit(0)" || line != want(N) {
                    wrong.push(format!("{machine}: {rec}"));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A fixture of this file's own, as `main.lu` in a scratch directory (one
/// module per directory).
#[cfg(unix)]
fn stage_fixture(name: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/checked_net_dials")
        .join(format!("{name}.lu"));
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("checked_net_dials")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::copy(&src, &entry).expect("stage the fixture");
    entry
}

/// #718: a proc killed while its dial waits leaves nothing the run waits
/// for at its end. The dial's host thread is still inside a connect the
/// kernel holds when `main` returns.
#[cfg(unix)]
#[test]
fn a_dial_killed_while_it_waits_leaves_nothing_to_join() {
    let entry = stage_fixture("dial_killed");
    let rec = match conform(&entry, "--checked", CHECKED_LIMIT) {
        Ok(Some(rec)) => rec,
        Ok(None) => unreachable!("the checked machine is never refused by the host"),
        Err(why) => panic!("{why}"),
    };
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(rec["stdout_inline"], "join: killed; kill prompt\n", "{rec}");
}
