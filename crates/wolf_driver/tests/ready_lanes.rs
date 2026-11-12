//! s290, the one wait (`[os.ready.wait]`, `[os.ready.set]`,
//! `[os.ready.facts]`, `[os.net.write.some]`, `[os.net.peer.cred]`) — on
//! the checked machine (`conform-run --json --checked`) and as native and
//! release binaries, linux also under `taskset` on four CPUs. Each fixture
//! makes its own pty masters, pipes and sockets in its scratch directory,
//! so the test needs no terminal around it.
//!
//! Each row runs on one side, the checked machine or native and release,
//! so a break on one side reds its rows by name and leaves the other's
//! green.
//!
//! - `a_mixed_set_*` (`fixtures/ready/mixed.lu`): two pty masters, a pipe,
//!   a Unix listener with a connection pending, two connected clients, a
//!   regular file and a flooded pipe, all ready at once and every one
//!   answered by both forms; fifty waits that read only the flood answer
//!   every entry every time; a change, a removal, a write interest and two
//!   closes in the set.
//! - `write_interest_and_busy_*` (`fixtures/ready/drain.lu`): a Unix stream
//!   and a pty master written to `busy`, write waits that answer their
//!   deadline, the drain, write answered at once; a peer gone is `closed`.
//! - `the_hangups_*` (`fixtures/ready/hangup.lu`): a pty master whose
//!   child exited, a pipe whose writer left, a pipe whose reader left, a
//!   Unix and a TCP stream whose peer closed — each host's facts.
//! - `a_meaning_with_entries_*` (`fixtures/ready/meaning.lu`): a meaning
//!   and a ready entry in one answer; `set` 0 takes none; a sibling's raise
//!   ends a park; 500 raises raced against the kernel-held set's park.
//! - `the_peer_credentials_*` (`fixtures/ready/cred.lu`), and
//!   `a_fork_child_and_its_parents_set_native_and_release`
//!   (`fixtures/ready/fork.lu`; the checked machine has no fork).
//! - `the_corpus_rows_*` (`corpus/os/ready_rows.lu`): the rows every host
//!   answers.
//! - `lupin_refuses_the_one_wait_by_name`: the corpus rows on a sibling
//!   lupin, pinned by version.
//!
//! At the base every fixture is E0301 at `wolf build` (exit 2) and at the
//! checked machine's resolve: a red, never a skip.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

mod test_cpus;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ready")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace")
        .to_path_buf()
}

fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("ready_lanes")
        .join(test);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

/// `wolf build` of a fixture; a refusal (E0301 at the base: exit 2) is a
/// failure, never a skip.
fn build(entry: &Path, dir: &Path, release: bool) -> PathBuf {
    let stem = entry.file_stem().unwrap().to_string_lossy().into_owned();
    let bin = dir.join(format!("{stem}{}", if release { "-release" } else { "" }));
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(entry)
        .arg("--no-cache")
        .arg("-o")
        .arg(&bin);
    if release {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf build runs");
    assert!(
        out.status.success(),
        "wolf build {} (release={release}) exit {:?}: {}",
        entry.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    bin
}

/// The side a row runs on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    /// `conform-run --json --checked`.
    Checked,
    /// The native binary (also pinned to four CPUs where the host has
    /// them) and the release binary.
    Native,
}

/// Each machine's name and what the fixture printed there; a machine whose
/// program did not exit 0 is a failure with its output.
fn run_lanes(entry: &Path, dir: &Path, side: Side) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if side == Side::Checked {
        let o = Command::new(wolf())
            .args(["conform-run", "--json", "--checked"])
            .arg(entry)
            .current_dir(dir)
            .output()
            .expect("conform-run runs");
        let text = String::from_utf8_lossy(&o.stdout).into_owned();
        let start = text
            .find("{\"")
            .unwrap_or_else(|| panic!("checked: no record:\n{text}"));
        let record = serde_json::Deserializer::from_str(&text[start..])
            .into_iter::<serde_json::Value>()
            .next()
            .and_then(Result::ok)
            .unwrap_or_else(|| panic!("checked: an unreadable record:\n{text}"));
        assert_eq!(
            record["verdict"].as_str(),
            Some("exit(0)"),
            "checked: {text}\n{}",
            String::from_utf8_lossy(&o.stderr)
        );
        out.push((
            "checked".to_string(),
            record["stdout_inline"].as_str().unwrap_or("").to_string(),
        ));
        return out;
    }
    for (tier, release) in [("native", false), ("release", true)] {
        let bin = build(entry, dir, release);
        let mut argvs = vec![(tier.to_string(), vec![bin.to_string_lossy().into_owned()])];
        if !release && let Some(pin) = test_cpus::pin(4, module_path!()) {
            let mut argv = pin.argv();
            argv.push(bin.to_string_lossy().into_owned());
            argvs.push((format!("{tier} {}", pin.label()), argv));
        }
        for (name, argv) in argvs {
            let o = Command::new(&argv[0])
                .args(&argv[1..])
                .current_dir(dir)
                .output()
                .expect("the binary runs");
            assert!(
                o.status.success(),
                "{name}: exit {:?}\n{}{}",
                o.status.code(),
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            out.push((name, String::from_utf8_lossy(&o.stdout).into_owned()));
        }
    }
    out
}

fn expect_lines(lane: &str, got: &str, want: &[String]) {
    for w in want {
        assert!(
            got.lines().any(|l| l == w),
            "{lane}: missing line {w:?}\n--- printed\n{got}"
        );
    }
}

fn lines(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| (*s).to_string()).collect()
}

fn a_mixed_set(side: Side, dir: &str) {
    let dir = scratch(dir);
    let want = lines(&[
        "one_shot: [0, 1, 1, 1, 2, 1, 3, 1, 4, 1, 5, 1, 6, 1, 7, 1] meaning 0",
        "flooded: true",
        "set_all: true meaning 0",
        "fair: 50 of 50",
        "changed: key 12 -1, key 99 1, key 11 -1, entries 7",
        "both_interests: key 14 3",
        "closed: key 10 -1, key 15 -1, entries 5",
        "closed_handle_add: io",
    ]);
    for (lane, got) in run_lanes(&fixture("mixed.lu"), &dir, side) {
        expect_lines(&lane, &got, &want);
    }
}

#[test]
fn a_mixed_set_on_the_checked_machine() {
    a_mixed_set(Side::Checked, "mixed-checked");
}

#[test]
fn a_mixed_set_native_and_release() {
    a_mixed_set(Side::Native, "mixed-native");
}

fn write_interest_and_busy(side: Side, dir: &str) {
    let dir = scratch(dir);
    let want = lines(&[
        "socket_full_at: busy taken true",
        "socket_busy_errno: true",
        "socket_wait_full: [] [] waited true",
        "socket_room: [0, 2] [5, 2]",
        "socket_again: true",
        "socket_empty: 0",
        "socket_peer_gone: closed",
        "listener_write_some: io",
        "pty_full_at: busy taken true settled true",
        "pty_wait_full: [] [] waited true",
        "pty_room: [0, 2] [6, 2]",
    ]);
    for (lane, got) in run_lanes(&fixture("drain.lu"), &dir, side) {
        expect_lines(&lane, &got, &want);
    }
}

#[test]
fn write_interest_and_busy_on_the_checked_machine() {
    write_interest_and_busy(Side::Checked, "drain-checked");
}

#[test]
fn write_interest_and_busy_native_and_release() {
    write_interest_and_busy(Side::Native, "drain-native");
}

fn the_hangups(side: Side, dir: &str) {
    let dir = scratch(dir);
    // `[os.ready.facts]`'s table: the read of a hung-up pty master, and a
    // pipe's write end whose reader left, are the host's.
    let (pty_read, reader_gone) = if cfg!(target_os = "macos") {
        ("eof", "[0, 6] [1, 6]")
    } else {
        ("io", "[0, 10] [1, 10]")
    };
    let want = vec![
        "pty_idle: []".to_string(),
        "child_status: 0".to_string(),
        "pty_child_exited: [0, 5] [1, 5]".to_string(),
        format!("pty_read_after: {pty_read}"),
        "pipe_writer_gone: [0, 5] [1, 5]".to_string(),
        format!("pipe_reader_gone: {reader_gone}"),
        "unix_peer_closed: [0, 5] [1, 5] [1, 7]".to_string(),
        "tcp_peer_closed: [0, 5] [1, 5]".to_string(),
    ];
    for (lane, got) in run_lanes(&fixture("hangup.lu"), &dir, side) {
        expect_lines(&lane, &got, &want);
    }
}

#[test]
fn the_hangups_on_the_checked_machine() {
    the_hangups(Side::Checked, "hangup-checked");
}

#[test]
fn the_hangups_native_and_release() {
    the_hangups(Side::Native, "hangup-native");
}

fn a_meaning_with_entries(side: Side, dir: &str) {
    let dir = scratch(dir);
    let want = lines(&[
        "one_shot_both: [0, 1] 512",
        "held_both: [3, 1] 512",
        "set_zero: [0, 1] 0 [3, 1] 0 queued 512",
        "sibling_raise: [0, 512, 0, 512] within true",
        "race rounds=500 meanings=500",
    ]);
    for (lane, got) in run_lanes(&fixture("meaning.lu"), &dir, side) {
        expect_lines(&lane, &got, &want);
    }
}

#[test]
fn a_meaning_with_entries_on_the_checked_machine() {
    a_meaning_with_entries(Side::Checked, "meaning-checked");
}

#[test]
fn a_meaning_with_entries_native_and_release() {
    a_meaning_with_entries(Side::Native, "meaning-native");
}

fn the_peer_credentials(side: Side, dir: &str) {
    let dir = scratch(dir);
    let want = lines(&[
        "accepted: pid true uid true gid true",
        "dialed: pid true uid true gid true",
        "listener: io",
        "tcp: io io",
        "closed: io",
    ]);
    for (lane, got) in run_lanes(&fixture("cred.lu"), &dir, side) {
        expect_lines(&lane, &got, &want);
    }
}

#[test]
fn the_peer_credentials_on_the_checked_machine() {
    the_peer_credentials(Side::Checked, "cred-checked");
}

#[test]
fn the_peer_credentials_native_and_release() {
    the_peer_credentials(Side::Native, "cred-native");
}

#[test]
fn a_fork_child_and_its_parents_set_native_and_release() {
    let dir = scratch("fork");
    // `[os.net.peer.cred]`: macOS's LOCAL_PEERPID answers only while the
    // peer is connected.
    let after = if cfg!(target_os = "macos") {
        "after_close: pid -1, uid true"
    } else {
        "after_close: pid the child's, uid true"
    };
    let want = lines(&[
        "child: set io wait io close ok",
        "parent: child exit 0, its pid true, read k",
        after,
        "parent_set: [1, 1] 0",
    ]);
    for (lane, got) in run_lanes(&fixture("fork.lu"), &dir, Side::Native) {
        expect_lines(&lane, &got, &want);
    }
}

/// The corpus rows' own directive, on each side: every line `true`.
fn the_corpus_rows(side: Side, dir: &str) {
    let dir = scratch(dir);
    let entry = workspace().join("corpus/os/ready_rows.lu");
    for (lane, got) in run_lanes(&entry, &dir, side) {
        let bad: Vec<&str> = got.lines().filter(|l| !l.ends_with(": true")).collect();
        assert!(bad.is_empty(), "{lane}: rows not true: {bad:#?}\n{got}");
        assert_eq!(got.lines().count(), 31, "{lane}: {got}");
    }
}

#[test]
fn the_corpus_rows_on_the_checked_machine() {
    the_corpus_rows(Side::Checked, "rows-checked");
}

#[test]
fn the_corpus_rows_native_and_release() {
    the_corpus_rows(Side::Native, "rows-native");
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

/// lupin's record: verdict, by-name reason, version, stdout.
fn record_fields(text: &str) -> (String, String, String, String) {
    text.lines()
        .rev()
        .find_map(|l| {
            l.find('{').and_then(|i| {
                serde_json::from_str::<serde_json::Value>(l[i..].trim())
                    .ok()
                    .map(|v| {
                        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
                        (
                            s("verdict"),
                            s("x-unsupported"),
                            s("impl_version"),
                            s("stdout_inline"),
                        )
                    })
            })
        })
        .unwrap_or_default()
}

/// lupin, the reference interpreter, has no `poll` and no peer-credential
/// call (`[os.ready.wait]`, `[os.net.peer.cred]`): every release refuses
/// the corpus rows by name — before its mirror because the names do not
/// resolve, after it because the waits and the credentials are refused by
/// name. Never a wrong answer.
#[test]
fn lupin_refuses_the_one_wait_by_name() {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found"
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        return;
    };
    let entry = workspace().join("corpus/os/ready_rows.lu");
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(&entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (verdict, reason, version, _stdout) = record_fields(&text);
    eprintln!("s290 lupin {version}: {verdict} ({reason})");
    assert_eq!(verdict, "unsupported", "lupin {version}: {text}");
    assert!(
        reason.contains("does not resolve") || reason.contains("os_wait_ready"),
        "lupin {version} refuses by name: {reason}"
    );
}
