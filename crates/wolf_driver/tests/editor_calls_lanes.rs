//! s248 (wolf-lang#796, #797, #798; ruling #93; yew `docs/MAP.md` §5 G3,
//! G4, G8) — what the editor asks of the runtime that needs no terminal:
//! a clock at nanoseconds (`[os.clock.ns]`), the canonical path
//! (`[os.fs.realpath]`), and the atomic save's syncs (`[os.fs.sync]`).
//!
//! Each witness runs on the checked machine and lupin under
//! `conform-run`, and as native and release binaries run directly — on
//! linux the binaries again under `taskset -c 0-3` (wolf-lang#570) — and
//! every machine's standard output is printed, so a `--nocapture` log is
//! the table. Nothing here is signalled, faulted or forked: the fault
//! shim of the save's witness ends the program by `_exit` at a storage
//! boundary, which is a process ending, not a signal.
//!
//! **lupin 0.1.54**, the pairing, predates every s248 name: each is
//! "does not resolve", pinned here by version exactly as measured
//! (s248's §2); a newer lupin is held to the mirror (the wolf-interp
//! debts are listed in the PR).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A `REPORT` line for the CI log even when the test passes: the
/// process's own standard error, past libtest's capture.
fn report(line: &str) {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

/// The pairing at this lane, which predates every s248 name.
const PRE_MIRROR_LUPIN: &str = "0.1.54";

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    reason: String,
    version: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let text = String::from_utf8_lossy(bytes).replace('\r', "");
    let rec: serde_json::Value = text
        .lines()
        .rev()
        .find_map(|l| {
            l.find('{')
                .and_then(|i| serde_json::from_str(l[i..].trim()).ok())
        })
        .unwrap_or_else(|| panic!("{what} record parses: {text}"));
    let field = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: field("verdict"),
        stdout: field("stdout_inline"),
        reason: {
            let r = field("x-unsupported");
            if r.is_empty() {
                field("x-unsupported-construct")
            } else {
                r
            }
        },
        version: field("impl_version"),
    }
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/editor_calls")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// A fresh working directory of the test's own.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("editor_calls_lanes")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

fn run_in(mut cmd: Command, dir: &Path) -> Output {
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.output().expect("spawn")
}

/// The native or release binary of `entry`, as `dir/<stem>-<tier>`. A
/// compile error FAILS the gate (it is the defect at a pre-fix head,
/// never an environment); an environment that cannot build the tier
/// skips loudly.
fn build(entry: &Path, dir: &Path, stem: &str, release: bool) -> Option<PathBuf> {
    let tier = if release { "release" } else { "native" };
    let exe = dir.join(format!("{stem}-{tier}{}", std::env::consts::EXE_SUFFIX));
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(entry)
        .arg("-o")
        .arg(&exe)
        .arg("--no-cache");
    if release {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf build runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("error["),
        "wolf build {} refused the program (a compile error is the defect, never a skip):\n{stderr}",
        entry.display()
    );
    if lane_exit::environment_refusal(&out, "wolf build") {
        eprintln!(
            "SKIP: environment cannot build the {tier} tier: {}",
            stderr.trim()
        );
        return None;
    }
    assert!(out.status.success(), "wolf build failed: {stderr}");
    Some(exe)
}

/// `taskset` on linux, from PATH (NixOS keeps it outside `/usr/bin`).
fn taskset() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("taskset"))
        .find(|p| p.is_file())
}

/// The sibling lupin, found exactly as `pairing.rs` finds it, or a loud
/// skip — a failure under `WOLF_PAIRING_REQUIRE_SIBLING`.
fn lupin_or_skip() -> Option<PathBuf> {
    let found = if let Some(p) = std::env::var_os("LUPIN") {
        let p = PathBuf::from(p);
        p.is_file().then_some(p)
    } else {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut hit = None;
        while dir.pop() {
            let candidate = dir.join("../wolf-interp/target/release/lupin");
            if candidate.is_file() {
                hit = Some(candidate);
                break;
            }
        }
        hit
    };
    if found.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of s248's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    found
}

/// lupin 0.1.54 stops at `call`, which it does not resolve; a newer
/// lupin is held to the compiler's answer.
fn judge_lupin(obs: &Obs, name: &str, want: &str, call: &str) {
    eprintln!(
        "== lupin {} on {name}: {} {}\n{}",
        obs.version, obs.verdict, obs.reason, obs.stdout
    );
    if obs.version == PRE_MIRROR_LUPIN {
        assert_eq!(
            obs.verdict, "unsupported",
            "lupin {} refuses {name}: {obs:?}",
            obs.version
        );
        assert!(
            obs.reason.contains(&format!("`{call}`")) && obs.reason.contains("does not resolve"),
            "lupin {} (pre-mirror) does not resolve `{call}`: {}",
            obs.version,
            obs.reason
        );
        report(&format!(
            "REPORT PINNED pre-mirror lupin {} on {name}: {}",
            obs.version, obs.reason
        ));
    } else {
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want),
            "lupin {} on {name}: {obs:?}",
            obs.version
        );
    }
}

/// `entry` on every machine, in `dir`: the checked machine and lupin
/// under `conform-run`, the native and release binaries run directly
/// (and under `taskset -c 0-3` on linux). Each stdout is held to `want`
/// as `check` reads it; lupin 0.1.54 to its refusal of `call`.
fn every_lane(entry: &Path, dir: &Path, call: &str, check: &dyn Fn(&str, &str)) -> String {
    let name = entry.file_name().unwrap().to_string_lossy().to_string();
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--checked")
        .arg("--json");
    let out = run_in(cmd, dir);
    let checked = parse_obs(&out.stdout, "the observation");
    eprintln!(
        "== checked on {name}: {}\n{}{}",
        checked.verdict,
        checked.stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        checked.verdict, "exit(0)",
        "the CHECKED machine on {name}: {checked:?}"
    );
    check("checked", &checked.stdout);
    report(&format!("REPORT checked {name}: {}", checked.stdout.trim_end()));
    let want = checked.stdout.clone();
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(entry, dir, "a", release) else {
            continue;
        };
        let mut runs = vec![(tier.to_string(), Command::new(&exe))];
        if let Some(t) = taskset() {
            let mut c = Command::new(t);
            c.args(["-c", "0-3"]).arg(&exe);
            runs.push((format!("{tier} taskset -c 0-3"), c));
        }
        for (lane, cmd) in runs {
            let out = run_in(cmd, dir);
            let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
            eprintln!(
                "== {lane} on {name}: exit {:?}\n{stdout}{}",
                out.status.code(),
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(out.status.code(), Some(0), "the {lane} binary on {name}");
            check(&lane, &stdout);
            report(&format!("REPORT {lane} {name}: {}", stdout.trim_end()));
        }
    }
    if let Some(l) = lupin_or_skip() {
        let mut cmd = Command::new(&l);
        cmd.arg("conform-run").arg(entry).arg("--json");
        let out = run_in(cmd, dir);
        judge_lupin(
            &parse_obs(&out.stdout, "lupin's observation"),
            &name,
            &want,
            call,
        );
    }
    want
}

// ------------------------------------------------------------ #798 --

/// #798 (`[os.clock.ns]`): twenty rounds of about 100 us of work, each
/// timed in nanoseconds, none 0 — yw00's witness read 19 of 20 rounds as
/// 0 ms at 0.2.32 (and 19, 20, 20 at 0.2.33, s248's §2). And the clock is
/// the ms clock's. Red at s248's prediction commit: E0301, the name does
/// not exist.
#[test]
fn twenty_rounds_of_100_us_are_never_zero_ns() {
    let dir = scratch("clock_ns");
    every_lane(
        &fixture("clock_ns.lu"),
        &dir,
        "os_clock_ns",
        &|lane, stdout| {
            assert_eq!(
                stdout, "clock_ns zeros=0 of 20 behind=0\n",
                "the {lane} machine's clock"
            );
        },
    );
}

// ------------------------------------------------------------ #797 --

/// #797 (`[os.fs.realpath]`): the canonical path of a file, a directory
/// with a trailing slash, a `..` after a directory and after a LINK (taken
/// after the link: `rp/l1/..` is `rp/a`), a dangling link and a missing
/// name (`not_found`), a loop and a name under a file (`io`). Every row is
/// relative to the canonical working directory, so the record is one on
/// every unix host. Red at s248's prediction commit: E0301.
#[test]
fn the_canonical_path_on_every_machine() {
    let dir = scratch("realpath");
    let want_unix = "file=rp/a/f\ndir_slash=rp/a\ndot_dot=rp/a/f\nmissing=not_found\n\
under_file=io\nempty=not_found\ndot=.\nlinks=made\nlink=rp/a/b\nlink_dot_dot=rp/a\n\
dangling=not_found\nloop=io\n";
    let got = every_lane(
        &fixture("realpath.lu"),
        &dir,
        "fs_realpath",
        &|lane, stdout| {
            if cfg!(unix) {
                assert_eq!(stdout, want_unix, "the {lane} machine's canonical paths");
            } else {
                assert!(
                    stdout.starts_with("file=rp\\a\\f\n") && stdout.contains("links=unsupported\n"),
                    "the {lane} machine's canonical paths (windows): {stdout}"
                );
            }
        },
    );
    report(&format!("REPORT realpath rows: {got:?}"));
}

// ------------------------------------------------------------ #796 --

/// #796 (`[os.fs.sync]`): `fs_sync` on a handle open for writing is ok,
/// a closed or forged handle and a pipe `io`; `fs_sync_dir` of the
/// working directory ok, of a missing name `not_found`, of a file
/// `invalid` (windows: `unsupported` by name for every argument); and the
/// exclusive create the save starts with answers `exists` for a taken
/// name. Red at s248's prediction commit: E0301.
#[test]
fn the_sync_rows_on_every_machine() {
    let dir = scratch("sync_rows");
    let want_unix = "written=ok\nexcl_again=exists\nclosed=io\nforged=io\nnegative=io\npipe=io\n\
dir_dot=ok\ndir_missing=not_found\ndir_file=invalid\ndir_empty=not_found\nagain=-1\n";
    let got = every_lane(
        &fixture("sync_rows.lu"),
        &dir,
        "fs_sync",
        &|lane, stdout| {
            if cfg!(unix) {
                assert_eq!(stdout, want_unix, "the {lane} machine's sync rows");
            } else {
                assert!(
                    stdout.starts_with("written=ok\nexcl_again=unsupported\nclosed=io\n")
                        && stdout.contains("dir_dot=unsupported\ndir_missing=unsupported\n"),
                    "the {lane} machine's sync rows (windows): {stdout}"
                );
            }
        },
    );
    report(&format!("REPORT sync rows: {got:?}"));
}
