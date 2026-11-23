//! wolf-lang#1179 (`[mem.region.escape]`, `[os.term.pty]`): a pty's slave
//! name asked inside a region and returned out of it is refused.
//!
//! `os_term_pty_name` builds its answer in the current region, as `os_cwd`
//! does, but the memory tier did not count it among the calls whose `str`
//! answer is an allocation there, so the escape compiled. The witness
//! (`fixtures/region_sites/escape.lu`) then allocates in a second region and
//! prints the name: the native and release binaries printed the second
//! region's `X` bytes in its place. The rows:
//!
//! - **Refused** (`escape.lu`, every host): `fail(E1010)` on the checked
//!   machine, and `wolf build` refuses it with `error[E1010]` on the native
//!   and the release tiers. Where a tier still builds it, the binary is run
//!   and what it printed is put in the failure, so a red run shows the bytes.
//! - **Kept** (`copy_out.lu`): the same name copied out of its region with
//!   `copy region` is the slave's device path on every machine, with none of
//!   the second region's bytes in it. Windows has no pty: `pty=unsupported`.
//!
//! **lupin**: the paired lupin refuses `os_term_open_pty` by name on every
//! host (it admits no `unsafe`, and its std has no pty call), so its answer
//! is pinned to that refusal, and an `exit(0)` that prints a name for the
//! witness fails here whatever the version. Every machine's answer is
//! printed, so a `--nocapture` log is the table.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A line a passing test still shows: written to stderr's handle directly,
/// past the harness's capture.
fn loud(line: &str) {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/region_sites")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("region_sites_lanes")
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
        reason: field("x-unsupported"),
        version: field("impl_version"),
    }
}

/// The checked machine's record of `entry`, run in `dir`.
fn checked(entry: &Path, dir: &Path) -> Obs {
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--checked")
        .arg("--json");
    let out = run_in(cmd, dir);
    let obs = parse_obs(&out.stdout, "the checked machine's record");
    loud(&format!(
        "== checked on {}: {}\n{}{}",
        entry.display(),
        obs.verdict,
        obs.stdout,
        String::from_utf8_lossy(&out.stderr)
    ));
    obs
}

/// What `wolf build` made of `entry` on one tier.
enum Built {
    /// The binary, at this path.
    Exe(PathBuf),
    /// The compiler refused it; its stderr.
    Refused(String),
    /// This host cannot build the tier (said loudly).
    Skipped,
}

fn build(entry: &Path, dir: &Path, release: bool) -> Built {
    let tier = if release { "release" } else { "native" };
    let exe = dir.join(format!("a-{tier}{}", std::env::consts::EXE_SUFFIX));
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
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if stderr.contains("error[") {
        return Built::Refused(stderr);
    }
    if lane_exit::environment_refusal(&out, "wolf build") {
        loud(&format!(
            "SKIP: {} --{tier}: this host cannot build the tier: {}",
            entry.display(),
            stderr.trim()
        ));
        return Built::Skipped;
    }
    assert!(out.status.success(), "wolf build --{tier} failed: {stderr}");
    Built::Exe(exe)
}

/// The sibling lupin, found as `pairing.rs` finds it, or a loud skip — a
/// failure under `WOLF_PAIRING_REQUIRE_SIBLING`.
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
             (LUPIN={}) — the oracle leg did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        loud("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    found
}

fn lupin(entry: &Path, dir: &Path) -> Option<Obs> {
    let l = lupin_or_skip()?;
    let mut cmd = Command::new(&l);
    cmd.arg("conform-run").arg(entry).arg("--json");
    let out = run_in(cmd, dir);
    let obs = parse_obs(&out.stdout, "lupin's record");
    loud(&format!(
        "== lupin {} on {}: {} {}\n{}",
        obs.version,
        entry.display(),
        obs.verdict,
        obs.reason,
        obs.stdout
    ));
    Some(obs)
}

/// lupin refuses the pty pair by name (no `unsafe`, no std call for it).
fn lupin_refuses_the_pty(obs: &Obs) -> bool {
    obs.verdict == "unsupported" && obs.reason.contains("`os_term_open_pty`")
}

/// #1179: the witness is refused on every machine wolf has. Red at trunk
/// (`1add9bb3`): the checked machine ran it to `exit(0)` and the native and
/// release binaries printed `name=XXXXXXXXXXXX` on macOS.
#[test]
fn a_pty_name_returned_out_of_its_region_is_refused() {
    let dir = scratch("escape");
    let entry = fixture("escape.lu");
    let mut wrong = Vec::new();

    let c = checked(&entry, &dir);
    if c.verdict != "fail(E1010)" {
        wrong.push(format!(
            "the checked machine answered {} (stdout {:?}), not fail(E1010)",
            c.verdict, c.stdout
        ));
    }
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        match build(&entry, &dir, release) {
            Built::Refused(stderr) => {
                loud(&format!("== {tier}: refused\n{}", stderr.trim()));
                if !stderr.contains("error[E1010]") {
                    wrong.push(format!(
                        "the {tier} build refused, but not with E1010:\n{stderr}"
                    ));
                }
            }
            Built::Exe(exe) => {
                let out = run_in(Command::new(&exe), &dir);
                let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
                loud(&format!(
                    "== {tier}: BUILT, exit {:?}\n{stdout}",
                    out.status.code()
                ));
                wrong.push(format!(
                    "the {tier} tier built the witness, and its binary printed {stdout:?}"
                ));
            }
            Built::Skipped => {}
        }
    }
    if let Some(l) = lupin(&entry, &dir) {
        let refused = lupin_refuses_the_pty(&l)
            || l.verdict == "fail(E1010)"
            || l.verdict == "trap(region-fault)";
        if !refused {
            wrong.push(format!(
                "lupin {} answered {} {:?}: neither its pty refusal nor a region refusal",
                l.version, l.verdict, l.stdout
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the escape of a pty name (wolf-lang#1179):\n- {}",
        wrong.join("\n- ")
    );
}

/// #1179's control: the name copied out of its region is the slave's
/// device path on every machine after a second region allocates. Green at
/// trunk too — `copy region` always copied — and green after: the site
/// refuses the escape, never the copy.
#[test]
fn a_pty_name_copied_out_of_its_region_is_kept() {
    let dir = scratch("copy_out");
    let entry = fixture("copy_out.lu");
    let want = if cfg!(unix) {
        "junk=64\ndevice=true\noverwritten=false\n"
    } else {
        "pty=unsupported\n"
    };

    let c = checked(&entry, &dir);
    assert_eq!(
        (c.verdict.as_str(), c.stdout.as_str()),
        ("exit(0)", want),
        "the CHECKED machine on copy_out.lu: {c:?}"
    );
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let exe = match build(&entry, &dir, release) {
            Built::Exe(exe) => exe,
            Built::Refused(stderr) => panic!("the {tier} build refused copy_out.lu:\n{stderr}"),
            Built::Skipped => continue,
        };
        let out = run_in(Command::new(&exe), &dir);
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        loud(&format!(
            "== {tier}: exit {:?}\n{stdout}",
            out.status.code()
        ));
        assert_eq!(
            out.status.code(),
            Some(0),
            "the {tier} binary on copy_out.lu"
        );
        assert_eq!(stdout, want, "the {tier} binary on copy_out.lu");
    }
    if let Some(l) = lupin(&entry, &dir) {
        assert!(
            lupin_refuses_the_pty(&l) || (l.verdict == "exit(0)" && l.stdout == want),
            "lupin {} on copy_out.lu: {l:?}",
            l.version
        );
    }
}
