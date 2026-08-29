//! s107 — the json and process builtin tiers, cross-lane (c26's last
//! crossing, wolf-lang#118: the checked-native builtin split closes).
//!
//! The net_native discipline over the two remaining families: every
//! witness runs `conform-run` on the checked lane (the reference
//! executor) AND both native tiers (`--native` = wir → Cranelift →
//! cc, `--release` = wir → LLVM -O2 → clang) and must produce the
//! IDENTICAL verdict and stdout — byte-equal, per file. The
//! acceptance's ten-run clause is asserted where process scheduling
//! could flap: the live spawn/wait/kill fixture (a real child's exit
//! and signal timing), per the #50 wait-don't-sample lesson — the
//! corpus files themselves are deterministic by construction (no
//! child is ever spawned; json is pure).
//!
//! lupin (wolf-interp) is the differential's fourth lane through
//! `cargo xtask differ`; at 0.1.13 it RESOLVES both families but
//! declines them at eval by design ("rather than risk a second,
//! guessed RFC 8259 reading"; "this machine runs no child processes
//! by design") — an `unsupported` skip under [proto.cmp], never a
//! divergence, so the byte-equality asserted here is the three
//! wolfgang lanes'. The contract's expectation that lupin executes
//! the json witnesses did not survive contact with lupin's own
//! source; the delta is recorded in the sprint closeout.
//!
//! Hosts the native tier refuses skip loudly at runtime (the s59
//! pattern: these tests start passing the moment a gate lifts).

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn corpus(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(file)
}

struct Obs {
    verdict: String,
    stdout: String,
}

/// One conform-run lane over a file. `None` (with a loud SKIP) only
/// for environment failures (exit 2: no cc/clang, no rt staticlib);
/// refusals stay visible as `unsupported` verdicts.
fn lane(path: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(path)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) && flag != "--checked" {
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {}: {}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    // The release tier refuses this HOST by name (linux/x86-64 +
    // macOS/aarch64 since s127): a loud skip, not a verdict (s59).
    if flag == "--release"
        && rec["verdict"] == "unsupported"
        && String::from_utf8_lossy(&out.stderr).contains("release tier targets linux/x86-64")
    {
        eprintln!("SKIP: the release tier refuses this host");
        return None;
    }
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    })
}

/// Run all three lanes; assert the expected verdict/stdout on checked
/// and byte-identical observations on both native tiers.
fn parity(path: &Path, want_verdict: &str, want_stdout: &str) {
    let checked = lane(path, "--checked").expect("checked lane always runs");
    assert_eq!(
        checked.verdict,
        want_verdict,
        "{}: checked verdict (stdout {:?})",
        path.display(),
        checked.stdout
    );
    assert_eq!(
        checked.stdout,
        want_stdout,
        "{}: checked stdout",
        path.display()
    );
    for flag in ["--native", "--release"] {
        let Some(native) = lane(path, flag) else {
            return;
        };
        assert_eq!(
            native.verdict,
            checked.verdict,
            "{}: {flag} verdict diverges from checked",
            path.display()
        );
        assert_eq!(
            native.stdout,
            checked.stdout,
            "{}: {flag} stdout diverges from checked",
            path.display()
        );
    }
}

/// The s40 query litmus crosses: valid/get/type/len agree on all
/// three lanes, byte for byte — the one parser, witnessed.
#[test]
fn json_query_agrees_on_every_lane() {
    parity(
        &corpus("json/query.lu"),
        "exit(0)",
        "true\nlupin 42\narray null 3 2\n",
    );
}

/// The fail-pin: each of the three json rows must be ITS tag on every
/// lane — `parse` for the RFC violation, `missing` for the absent
/// path, `kind` for the scalar length — never a coarser one.
#[test]
fn json_rows_agree_on_every_lane() {
    parity(&corpus("json/rows.lu"), "exit(0)", "parse\nmissing\nkind\n");
}

/// The process fail-pin: empty argv is handled `not_found`, forged
/// handles are `io` on wait and kill — rows on every lane, no child
/// ever spawned.
#[test]
fn spawn_rows_agree_on_every_lane() {
    parity(&corpus("os/spawn_rows.lu"), "exit(0)", "empty\nio\nio2\n");
}

/// The live-child witness, ten-run (the acceptance clause lands where
/// process scheduling could actually flap): a real spawn/wait carries
/// the exit code, the reap tombstones (double wait is `io`), and
/// kill-then-wait is the `signal` row — WAITED for, never sampled
/// (#50). Inline fixture: `/bin/sh` is this test's own platform
/// floor (the file is linux-gated with the native tier), matching
/// the checked twins in `crates/wolf_mem/tests/os_time_json.rs`.
#[test]
fn live_spawn_wait_kill_agrees_ten_times() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("s107_live_child");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("s107_live_child.lu");
    std::fs::write(
        &entry,
        r#"
fn main() -> !int {
    var argv = List[str]()
    (mut argv).push("/bin/sh")
    (mut argv).push("-c")
    (mut argv).push("exit 7")
    let h = os_spawn(argv)?
    let code = os_wait(h)?
    print("code: {code}")
    os_wait(h) else |_| { print("reaped"); -1 }
    var argv2 = List[str]()
    (mut argv2).push("/bin/sh")
    (mut argv2).push("-c")
    (mut argv2).push("sleep 30")
    let h2 = os_spawn(argv2)?
    os_kill(h2)?
    os_wait(h2) else |_| { print("signalled"); -1 }
    0
}
"#,
    )
    .expect("write fixture");
    let want = "code: 7\nreaped\nsignalled\n";
    let checked = lane(&entry, "--checked").expect("checked lane always runs");
    assert_eq!(checked.verdict, "exit(0)", "checked live-child verdict");
    assert_eq!(checked.stdout, want, "checked live-child stdout");
    for flag in ["--native", "--release"] {
        for run in 0..10 {
            let Some(obs) = lane(&entry, flag) else {
                return;
            };
            assert_eq!(obs.verdict, "exit(0)", "{flag} run {run}: verdict");
            assert_eq!(obs.stdout, want, "{flag} run {run}: stdout flapped");
        }
    }
}

/// #129 (s111): the child's stdout/stderr are INHERITED, not
/// null-wired — the child writes through to whatever the parent's
/// stdout is. On the native tiers the compiled program runs under
/// conform-run's pipe, so the child's bytes land in the program's own
/// observed stdout (exactly the wws test-runner use case). On the
/// checked lane the machine's print stream is a BUFFER the child's
/// fd-level writes cannot enter: the child writes to conform-run's
/// real stdout instead, AHEAD of the observation record — asserted
/// here raw, as the documented asymmetry (capture-to-string stays
/// the named upstream ask on #129; stdin stays null-wired, so the
/// `cat` child sees immediate EOF and echoes nothing from it).
#[test]
fn spawned_child_stdout_writes_through() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("s111_child_stdout");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("s111_child_stdout.lu");
    std::fs::write(
        &entry,
        r#"
fn main() -> !int {
    print("before")
    var argv = List[str]()
    (mut argv).push("/bin/sh")
    (mut argv).push("-c")
    (mut argv).push("echo child-through; cat")
    let h = os_spawn(argv)?
    let code = os_wait(h)?
    print("after {code}")
    0
}
"#,
    )
    .expect("write fixture");
    for flag in ["--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            return;
        };
        assert_eq!(obs.verdict, "exit(0)", "{flag} verdict");
        assert_eq!(
            obs.stdout, "before\nchild-through\nafter 0\n",
            "{flag}: the child's line must appear in the program's own stdout"
        );
    }
    // Checked lane, raw: the child's bytes precede the observation
    // record on the host stream (wait orders them), and the record's
    // own stdout_inline carries only the machine's prints.
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(&entry)
        .arg("--checked")
        .arg("--json")
        .output()
        .expect("wolf runs");
    assert!(out.status.success(), "checked lane runs");
    let text = String::from_utf8_lossy(&out.stdout);
    let (child, record) = text
        .split_once('{')
        .expect("child bytes then the JSON record");
    assert_eq!(child, "child-through\n", "checked: write-through bytes");
    let rec: serde_json::Value =
        serde_json::from_str(&format!("{{{record}")).expect("record parses");
    assert_eq!(rec["verdict"].as_str(), Some("exit(0)"));
    assert_eq!(
        rec["stdout_inline"].as_str(),
        Some("before\nafter 0\n"),
        "checked: the machine's buffered prints exclude fd-level child bytes"
    );
}
