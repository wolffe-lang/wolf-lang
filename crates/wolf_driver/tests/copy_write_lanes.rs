//! s260 (wolf-lang#642, `[os.fs.copy]`, R-s260-1) — a copy whose write
//! fails keeps what it read, on every machine.
//!
//! Until s260 the read/write rung read up to `max` bytes, the write
//! failed, and the call answered `io` with the source already past the
//! bytes: t06 replayed the issue at 0.2.34 (`9da7bdc`) and at trunk
//! `06cff722`, `copy=-2 os_error=28 (No space left on device)
//! offset_after=17 left_to_read=0 of 17`, native and release alike, and
//! the checked machine lost them the same way through its own buffer.
//! The clause now says the source is put back by the shortfall, so after
//! any `io` it stands exactly past what the sink took.
//!
//! Three witnesses: the two corpus rows (`fs/copy_chunk_failed_write.lu`,
//! a sink opened for reading only, which every host refuses;
//! `fs/copy_chunk_dev_full.lu`, `/dev/full` by path) on the checked
//! machine, native and release, and t06's own program with descriptor 1
//! on `/dev/full` (linux; built native and release — on the checked
//! machine descriptor 1 is the machine's capture, which never fails, so
//! the by-path row is that machine's witness).
//!
//! lupin 0.1.55 (the 0.2.34 pairing, record commit `e642f26`) loses the
//! bytes too, measured on kasumi: its run of the failed-write row is
//! pinned by version AND commit, so a newer lupin that still loses them
//! reds by name (wolf-interp owes the mirror). It declines `/dev/full`
//! by name (`[os.fs.path.domain]`: it serves real files inside its own
//! tree), a posture, not a lag.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const ISSUE: &str = "wolf-lang#642";

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
    commit: String,
    refused: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    let s = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: s("verdict"),
        stdout: s("stdout_inline"),
        version: s("impl_version"),
        commit: s("commit"),
        refused: s("x-unsupported"),
    }
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

/// One wolfgang lane. `None` is the s59 environment skip, named on
/// stderr — never an ICE (lane_exit), never the checked machine.
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    ensure_rt_staticlib();
    let out = Command::new(wolf())
        .current_dir(root())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if flag != "--checked" && lane_exit::environment_refusal(&out, &format!("wolf {flag}")) {
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, "the observation"))
}

/// The sibling lupin, found exactly as `pairing.rs` finds it: `LUPIN`
/// first, then a `wolf-interp` checkout beside an ancestor.
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

/// lupin's observation, or `None` when this box has no sibling
/// (`WOLF_PAIRING_REQUIRE_SIBLING` makes that a failure, r10/#253).
fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of {ISSUE}'s gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        return None;
    };
    let out = Command::new(&lupin)
        .current_dir(root())
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    Some(parse_obs(&out.stdout, "lupin's observation"))
}

/// The environment skip, strictly (#550, #571): an exit 2 is a skip
/// only when the host cannot run the tier at all; a compile error is
/// never one. (Only the linux witness builds a program itself.)
#[cfg(target_os = "linux")]
fn unavailable(out: &std::process::Output, tier: &str) -> bool {
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

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus(rel: &str) -> PathBuf {
    let p = root().join("corpus").join(rel);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// A lupin answer measured on a release that predates the mirror:
/// (version, record commit, verdict, stdout).
type Pin = (&'static str, &'static str, &'static str, &'static str);

/// lupin 0.1.55 loses the 17 bytes (kasumi, s260): the source stands at
/// 17, the retry is `eof` and `main`'s `?` ends the run with it.
const FAILED_WRITE_PRE_MIRROR: &[Pin] = &[(
    "0.1.55",
    "e642f26",
    "exit(1)",
    "copy=io took_nothing=false os_error_set=true offset_after=17\nerror: eof\n",
)];

const FAILED_WRITE: &str = "copy=io took_nothing=true os_error_set=true offset_after=0\nretry=17 same=true\ncleaned=true\n";
const DEV_FULL: &str =
    "copy=io os_error_set=true offset_after=0 left_to_read=17 of 17\ncleaned=true\n";

fn every_wolfgang_lane_says(rel: &str, stdout: &str) {
    let entry = corpus(rel);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", stdout),
            "the {flag} answer on {rel} ({ISSUE}); refusal {:?}",
            obs.refused
        );
    }
}

/// A sink every host refuses: `io`, nothing lost, the retry moves all 17
/// bytes — on all three wolfgang machines; lupin the clause's answer or
/// its measured pre-mirror one.
#[test]
fn a_failed_copy_write_puts_the_source_back_on_every_lane() {
    let rel = "fs/copy_chunk_failed_write.lu";
    every_wolfgang_lane_says(rel, FAILED_WRITE);
    let Some(lupin) = lupin_says(&corpus(rel)) else {
        return;
    };
    let got = (lupin.verdict.as_str(), lupin.stdout.as_str());
    if got == ("exit(0)", FAILED_WRITE) {
        return;
    }
    assert!(
        FAILED_WRITE_PRE_MIRROR
            .iter()
            .any(|(v, c, pv, ps)| *v == lupin.version && *c == lupin.commit && (*pv, *ps) == got),
        "lupin {} ({})'s answer on {rel}: {got:?}, want the clause's or a pre-mirror pin \
         ({FAILED_WRITE_PRE_MIRROR:?})",
        lupin.version,
        lupin.commit,
    );
}

/// `cat f > /dev/full` with the offset read back: the same answer on
/// every wolfgang machine; lupin declines the device by name.
#[test]
fn a_copy_into_dev_full_keeps_its_bytes_on_every_lane() {
    let rel = "fs/copy_chunk_dev_full.lu";
    every_wolfgang_lane_says(rel, DEV_FULL);
    let Some(lupin) = lupin_says(&corpus(rel)) else {
        return;
    };
    if (lupin.verdict.as_str(), lupin.stdout.as_str()) == ("exit(0)", DEV_FULL) {
        return;
    }
    assert!(
        lupin.verdict == "unsupported" && lupin.refused.contains("os.fs.path.domain"),
        "lupin {}'s answer on {rel}: {lupin:?}",
        lupin.version
    );
}

/// t06's program, descriptor 1 on `/dev/full`, native and release.
#[cfg(target_os = "linux")]
#[test]
fn t06_witness_into_dev_full_keeps_its_bytes() {
    ensure_rt_staticlib();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("s260-copy-witness");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        root().join("crates/wolf_driver/tests/fixtures/copy_write/witness.lu"),
        dir.join("main.lu"),
    )
    .unwrap();
    std::fs::write(dir.join("in.txt"), "alpha\nbeta\ngamma\n").unwrap();
    for tier in ["native", "release"] {
        let mut build = Command::new(wolf());
        build
            .current_dir(&dir)
            .args(["build", "main.lu", "-o", tier]);
        if tier == "release" {
            build.arg("--release");
        }
        let out = build.output().unwrap();
        if unavailable(&out, tier) {
            eprintln!("SKIP: t06_witness_into_dev_full_keeps_its_bytes {tier}");
            continue;
        }
        assert!(out.status.success(), "{tier} build: {out:?}");
        let full = std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .expect("/dev/full opens on linux");
        let run = Command::new(dir.join(tier))
            .current_dir(&dir)
            .stdout(full)
            .output()
            .unwrap();
        assert!(run.status.success(), "{tier}: {run:?}");
        assert_eq!(
            String::from_utf8_lossy(&run.stderr).trim_end(),
            "copy=-2 os_error=28 (No space left on device) offset_after=0 left_to_read=17 of 17",
            "{tier} ({ISSUE})"
        );
    }
}
