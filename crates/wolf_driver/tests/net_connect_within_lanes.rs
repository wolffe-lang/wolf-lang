//! wolf-lang#1146 (`[os.net.connect.within]`): a dial with a budget.
//!
//! `net_connect_within(addr, within_ms) -> int ! {io, refused, timeout,
//! invalid}`. A budget of zero or less is `invalid`, answered before any
//! dial. One budget covers every address the name resolves to, tried in
//! turn; name resolution is not inside it. Until it, `net_connect` waited
//! for the host's own connect timeout (75 s or more for an address that
//! drops SYNs), and its `timeout` row could not be reached.
//!
//! The rows run on the checked machine, native and release with one
//! verdict and stdout: the three corpus rows, a held dial answering
//! `timeout` inside its budget (unix: windows refuses a dial past a full
//! backlog), and a name whose first address refuses connecting to its
//! second inside one budget. lupin has no such builtin and is not a machine
//! here. A run past its deadline is killed by its own child handle and
//! FAILS; a test no machine ran FAILS too.
//!
//! The two-address row dials `WOLF_TWO_ADDRESS_HOST` when it is set (a name
//! the operator gave two addresses of one family, a hosts entry, so the
//! witness never waits on IPv6), else `localhost`. Where that name has
//! fewer than two distinct addresses the row prints a `SKIP:` line a record
//! counts, and `WOLF_TWO_ADDRESS_REQUIRE=1` makes that skip a failure.
mod lane_exit;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const MACHINES: [&str; 3] = ["--checked", "--native", "--release"];

/// Far past any honest run (each answers in about a second, the held dial
/// in under 3 s), far short of a dial that waits for the host's timeout.
const LIMIT: Duration = Duration::from_secs(60);

fn corpus(row: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/net")
        .join(row)
}

/// A fixture of this file's own as `main.lu` in a scratch directory, with
/// `edit.0` replaced by `edit.1` when given.
fn stage(name: &str, edit: Option<(&str, &str)>) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/net_connect_within")
        .join(format!("{name}.lu"));
    let mut body = std::fs::read_to_string(&src).expect("read the fixture");
    if let Some((from, to)) = edit {
        assert!(body.contains(from), "fixture anchor missing: {from}");
        body = body.replacen(from, to, 1);
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("net_connect_within")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, body).expect("write the fixture");
    entry
}

/// Wait for `child` until `LIMIT`; past it the child is killed by its own
/// handle. `None` is that stall.
fn finish(mut child: Child) -> (Option<Output>, Duration) {
    let t0 = Instant::now();
    loop {
        if child.try_wait().expect("try_wait").is_some() {
            break;
        }
        if t0.elapsed() > LIMIT {
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

/// One machine's record: `Ok(None)` when the host cannot run that tier (a
/// loud SKIP, refused under `WOLF_TEST_NATIVE_REQUIRE`), `Err` for a stall
/// or a run with no record.
fn conform(entry: &Path, machine: &str) -> Result<Option<serde_json::Value>, String> {
    let child = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .args(["conform-run", "--json", machine])
        .arg(entry)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run wolf");
    let what = format!("{machine} {}", entry.display());
    let (out, elapsed) = finish(child);
    let Some(out) = out else {
        return Err(format!("{what}: no answer in {elapsed:?} (a stall)"));
    };
    if machine != "--checked" && lane_exit::environment_refusal(&out, machine) {
        assert!(
            std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
            "required {machine} unavailable: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        eprintln!("SKIP: {what}: {}", String::from_utf8_lossy(&out.stderr));
        return Ok(None);
    }
    if !out.status.success() {
        return Err(format!(
            "{what}: {} {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    serde_json::from_slice(&out.stdout)
        .map(Some)
        .map_err(|e| format!("{what}: {e}: {}", String::from_utf8_lossy(&out.stdout)))
}

/// `verdict` and `stdout` from every machine; each runs before the verdict,
/// so a red run reports each one's answer (`REPORT` lines).
fn answers(entry: &Path, verdict: &str, stdout: &str) {
    let mut wrong = Vec::new();
    let mut ran = 0;
    for machine in MACHINES {
        match conform(entry, machine) {
            Err(why) => {
                eprintln!("REPORT {why}");
                wrong.push(why);
            }
            Ok(None) => {}
            Ok(Some(rec)) => {
                ran += 1;
                eprintln!(
                    "REPORT {machine} {}: {} {}",
                    entry.display(),
                    rec["verdict"],
                    rec["stdout_inline"]
                );
                if rec["verdict"] != verdict || rec["stdout_inline"] != stdout {
                    wrong.push(format!("{machine}: {rec}"));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert!(ran > 0, "no machine ran {}", entry.display());
}

#[test]
fn a_listener_answers_inside_the_budget() {
    answers(&corpus("connect_within_ok.lu"), "exit(0)", "echo: howl\n");
}

#[test]
fn a_budget_of_zero_or_less_is_invalid() {
    answers(
        &corpus("connect_within_invalid.lu"),
        "exit(0)",
        "invalid invalid\n",
    );
}

#[test]
fn a_closed_port_is_refused() {
    answers(
        &corpus("connect_within_refused.lu"),
        "exit(1)",
        "handled: 0\nerror: refused\n",
    );
}

/// A dial the kernel holds answers `timeout` inside its 300 ms budget.
#[cfg(unix)]
#[test]
fn a_held_dial_times_out_inside_its_budget() {
    answers(
        &stage("timeout", None),
        "exit(0)",
        "timed true bounded true\n",
    );
}

/// The listener sits on the second address the name resolves to here; the
/// first refuses, and the one budget covers both.
#[test]
fn every_resolved_address_is_tried_in_turn() {
    use std::net::ToSocketAddrs as _;
    let name = std::env::var("WOLF_TWO_ADDRESS_HOST").unwrap_or_else(|_| "localhost".to_string());
    let addrs: Vec<std::net::SocketAddr> = match (name.as_str(), 0).to_socket_addrs() {
        Ok(a) => a.collect(),
        Err(e) => panic!("{name} does not resolve: {e}"),
    };
    let mut distinct: Vec<std::net::IpAddr> = Vec::new();
    for a in &addrs {
        if !distinct.contains(&a.ip()) {
            distinct.push(a.ip());
        }
    }
    eprintln!("REPORT {name} resolves to {addrs:?}");
    if distinct.len() < 2 {
        let line = format!(
            "SKIP: every_resolved_address_is_tried_in_turn: {name} resolves to {distinct:?}; \
             one address cannot witness the order"
        );
        assert!(
            std::env::var_os("WOLF_TWO_ADDRESS_REQUIRE").is_none(),
            "{line} (WOLF_TWO_ADDRESS_REQUIRE is set)"
        );
        eprintln!("{line}");
        return;
    }
    let listen = match distinct[1] {
        std::net::IpAddr::V6(ip) => format!("[{ip}]:0"),
        std::net::IpAddr::V4(ip) => format!("{ip}:0"),
    };
    eprintln!("REPORT dialling {name}, listening on {listen} (its second address)");
    let staged = stage("two_addresses", Some(("@LISTEN@", &listen)));
    let body = std::fs::read_to_string(&staged).expect("read the staged row");
    std::fs::write(&staged, body.replacen("@NAME@", &name, 1)).expect("name the row");
    answers(&staged, "exit(0)", "connected\n");
}
