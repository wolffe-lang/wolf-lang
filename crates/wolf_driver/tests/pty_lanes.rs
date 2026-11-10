//! s292 (`[os.term.pty]`, `[os.term.size.set]`, `[os.term.attr]`) — a
//! pseudo-terminal opened through the new call, on the checked machine
//! (`conform-run --json --checked`) and as native and release binaries,
//! linux also under `taskset` on four CPUs. Each fixture opens its own
//! pty, so the test needs no terminal around it.
//!
//! Each row runs on one side, the checked machine or native and release,
//! so a break on one side reds its rows by name and leaves the other's
//! green.
//!
//! - `the_pair_on_the_checked_machine`, `the_pair_native_and_release`
//!   (`fixtures/pty/pair.lu`): the pair are terminals; the name has the
//!   host's form and opens a terminal; a size set on each end reads on the
//!   other; every RFC 4254 opcode on the slave round-trips, or is lacking,
//!   or is not kept, exactly as `[os.term.attr]`'s table says for this
//!   host; the name of a slave is `io`; the name is gone after every
//!   descriptor closes.
//! - `the_documented_order_with_a_fork_native_and_release`
//!   (`fixtures/pty/winch.lu`; the checked machine has no fork): a child
//!   that leads a session on the slave reads the size set before it, hears
//!   a new size and a pixel-only change but not the same size again, and
//!   is interrupted by the Ctrl-G its VINTR was set to before the fork.
//! - `the_documented_order_through_a_spawn_on_the_checked_machine`,
//!   `the_documented_order_through_a_spawn_native_and_release`
//!   (`fixtures/pty/spawned.lu`): the same order with `os_spawn_session` in
//!   place of the fork, the child the compiled `fixtures/pty/helper.lu`.
//! - `the_checked_table_is_the_runtimes`: the checked machine's
//!   transcription of the op table equals the runtime's on this host.
//! - `lupin_does_not_resolve_the_pty_calls_before_its_mirror`: the corpus
//!   rows on a sibling lupin, pinned by version.
//!
//! At the base every fixture is E0301 at `wolf build` (exit 2): a red,
//! never a skip.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

mod test_cpus;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pty")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("pty_lanes")
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

/// Each machine's name and what the fixture printed there.
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

/// `[os.term.attr]`'s table for this host: what each op answers on a
/// pty's slave.
fn op_row(op: i64) -> &'static str {
    let (lacks, not_kept): (&[i64], &[i64]) = if cfg!(target_os = "macos") {
        (&[15, 16, 37, 52, 71], &[62])
    } else {
        (&[11, 15, 17], &[90, 92])
    };
    if lacks.contains(&op) {
        "lacks"
    } else if not_kept.contains(&op) {
        "not kept"
    } else {
        "kept"
    }
}

const OPS: [i64; 56] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 30, 31, 32, 33, 34, 35, 36, 37,
    38, 39, 40, 41, 42, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 70, 71, 72, 73, 74, 75,
    90, 91, 92, 93, 128, 129,
];

fn the_pair(side: Side, dir: &str) {
    let dir = scratch(dir);
    let mut want: Vec<String> = vec![
        "pair: handles true, master a terminal true, slave a terminal true".into(),
        "by name: a terminal true".into(),
        "size: master to slave 40 132, slave to master 50 100".into(),
        "name of a slave: io".into(),
        "gone: true".into(),
    ];
    want.extend(OPS.iter().map(|op| format!("op {op}: {}", op_row(*op))));
    let prefix = if cfg!(target_os = "macos") {
        "name: /dev/ttys"
    } else {
        "name: /dev/pts/"
    };
    for (lane, got) in run_lanes(&fixture("pair.lu"), &dir, side) {
        expect_lines(&lane, &got, &want);
        assert!(
            got.lines().any(|l| l.starts_with(prefix)),
            "{lane}: no line starting {prefix:?}\n{got}"
        );
    }
}

#[test]
fn the_pair_on_the_checked_machine() {
    the_pair(Side::Checked, "pair-checked");
}

#[test]
fn the_pair_native_and_release() {
    the_pair(Side::Native, "pair-native");
}

#[test]
fn the_documented_order_with_a_fork_native_and_release() {
    let dir = scratch("winch");
    let want: Vec<String> = [
        "winch: the child ready at the size set before it true",
        "winch: a new size heard 1, the same size again heard 0",
        "winch: a pixel-only change heard true",
        "winch: Ctrl-G interrupts true, status 7",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    for (lane, got) in run_lanes(&fixture("winch.lu"), &dir, Side::Native) {
        expect_lines(&lane, &got, &want);
    }
}

/// The documented order on every machine, the checked one included: a
/// spawn in a new session in place of the fork, the child a compiled
/// helper on the slave.
fn through_a_spawn(side: Side, dir: &str) {
    let dir = scratch(dir);
    let helper = build(&fixture("helper.lu"), &dir, false);
    std::fs::write(dir.join("s292.helper"), helper.to_string_lossy().as_bytes())
        .expect("the helper's name");
    let want: Vec<String> = [
        "spawned: the child ready at the size set before it true",
        "spawned: a new size heard 1, the same size again heard 0",
        "spawned: a pixel-only change heard true",
        "spawned: Ctrl-G interrupts true, status 7",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    for (lane, got) in run_lanes(&fixture("spawned.lu"), &dir, side) {
        expect_lines(&lane, &got, &want);
    }
}

#[test]
fn the_documented_order_through_a_spawn_on_the_checked_machine() {
    through_a_spawn(Side::Checked, "spawned-checked");
}

#[test]
fn the_documented_order_through_a_spawn_native_and_release() {
    through_a_spawn(Side::Native, "spawned-native");
}

/// `[os.term.attr]`: the checked machine performs the calls in the `wolf`
/// process with the runtime's op table transcribed (the runtime is not a
/// dependency of the compiler's crates); on each host the two tables are
/// one, op for op, speed for speed, and the disabled byte.
#[test]
fn the_checked_table_is_the_runtimes() {
    let rt = wolf_rt::term::attr_table();
    assert_eq!(rt.len(), 56 + 19 + 1, "{rt:#?}");
    assert_eq!(wolf_mem::ubcheck::pty_attr_table(), rt);
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace")
        .to_path_buf()
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

/// The lupin releases that predate wolf-interp's mirror of s292: each
/// answers a program naming the five calls "does not resolve".
const PRE_MIRROR_LUPIN: &[&str] = &["0.1.60"];

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

/// lupin, the reference interpreter: a release before its mirror refuses
/// the corpus rows because the five names do not resolve; the mirror
/// answers every one of them (each is decided before a terminal is
/// reached) with the lines the corpus row states.
#[test]
fn lupin_does_not_resolve_the_pty_calls_before_its_mirror() {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found"
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        return;
    };
    let entry = workspace().join("corpus/os/pty_rows.lu");
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(&entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (verdict, reason, version, stdout) = record_fields(&text);
    eprintln!("s292 lupin {version}: {verdict} ({reason})");
    if PRE_MIRROR_LUPIN.contains(&version.as_str()) {
        assert_eq!(verdict, "unsupported", "lupin {version}: {text}");
        assert!(
            reason.contains("does not resolve"),
            "lupin {version} predates the mirror: {reason}"
        );
    } else {
        assert_eq!(verdict, "exit(0)", "lupin {version}: {text}");
        assert!(
            stdout.contains("lacking_ops: true") && stdout.contains("forged_speed: true"),
            "lupin {version} answers the rows: {stdout}"
        );
    }
}
