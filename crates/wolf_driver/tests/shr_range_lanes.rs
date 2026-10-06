//! s214 (wolf-lang#600, P0): a branch on the top half of a logical shift
//! is kept. rangeopt bounded `x >> s` by the width's SIGNED maximum
//! shifted, `(2^(bits-1) - 1) >> s`, where a logical shift reads its
//! operand as unsigned bits and reaches `(2^bits - 1) >> s`; release
//! deleted every branch whose condition needed the top half (`a >> 63
//! == 1`, `a >> 39 == 33554304`, a byte's `>> 4 == 15`). Native runs no
//! mid-end and was right; so were the checked machine and lupin.
//!
//! Rows, each seen red at 294d626d on kasumi (`~/lanes/s214/evidence/`):
//!
//! 1. `corpus/kernels/lshr_top_bits.lu` — `wrapping[u64]` and
//!    `wrapping[u8]`, four machines: release printed `... 0` for `31`;
//! 2. `corpus/memory/raw_lshr_top_bits.lu` — plain `u64`/`u8` out of a
//!    C allocation: release `0` for `31`; the checked machine refuses a
//!    shift on a non-wrapping integer by name;
//! 3. the issue's hosted witness (`fixtures/shr_range/`, an opaque value
//!    from listed assembly; x86-64 linux): release printed one line of
//!    three.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    codes: Vec<String>,
    stdout: String,
    version: String,
    refusal: String,
}

fn parse_obs(bytes: &[u8], stderr: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    let mut codes: Vec<String> = rec["diagnostics"]
        .as_array()
        .map(|ds| {
            ds.iter()
                .filter_map(|d| d["code"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    codes.sort();
    codes.dedup();
    let refusal = rec["x-unsupported-construct"]
        .as_str()
        .or(rec["x-unsupported"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| String::from_utf8_lossy(stderr).into_owned());
    let s = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: s("verdict"),
        codes,
        stdout: s("stdout_inline"),
        version: s("impl_version"),
        refusal,
    }
}

/// Build the runtime staticlib once (every lane needs it; its file
/// name is the host's: `libwolf_rt.a`, or `wolf_rt.lib` on windows).
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

/// The unix runtime archive, for a link this gate does itself.
#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(dead_code)
)]
fn rt_staticlib() -> PathBuf {
    ensure_rt_staticlib();
    let rt = Path::new(wolf())
        .parent()
        .expect("wolf has a directory")
        .join("libwolf_rt.a");
    assert!(rt.is_file(), "libwolf_rt.a beside wolf: {}", rt.display());
    rt
}

/// One wolfgang lane; `None` is the s59 environment skip, named on
/// stderr (never an ICE, never the checked machine).
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    ensure_rt_staticlib();
    let out = Command::new(wolf())
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
    Some(parse_obs(&out.stdout, &out.stderr, "the observation"))
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

fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of s214's #600 gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — only the oracle leg is absent");
        return None;
    };
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    assert!(
        !out.stdout.is_empty(),
        "lupin wrote no record for {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, &out.stderr, "lupin's observation"))
}

fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(dead_code)
)]
fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/shr_range")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(dead_code)
)]
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("shr_range")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// Every machine that runs the row prints `want`; the checked machine
/// may instead refuse by `refusal` (named, never a guess).
fn every_machine_prints(row: &str, want: &str, checked_refusal: Option<&str>) {
    let entry = corpus(row);
    let checked = lane(&entry, "--checked").expect("the checked machine always runs");
    match checked_refusal {
        Some(named) => assert!(
            checked.verdict == "unsupported" && checked.refusal.contains(named),
            "the checked machine refuses {row} by name ({named:?}): {checked:?}"
        ),
        None => assert_eq!(
            (checked.verdict.as_str(), checked.stdout.as_str()),
            ("exit(0)", want),
            "the checked machine on {row}: {checked:?}"
        ),
    }
    let mut who = vec![("native", lane(&entry, "--native"))];
    who.push(("release", lane(&entry, "--release")));
    who.push(("lupin", lupin_says(&entry)));
    for (name, obs) in who {
        let Some(obs) = obs else { continue };
        assert_eq!(
            (obs.verdict.as_str(), obs.codes.len(), obs.stdout.as_str()),
            ("exit(0)", 0, want),
            "{name} {} on {row}: a branch on the top half of `>>` is kept \
             (wolf-lang#600); refusal {:?}",
            obs.version,
            obs.refusal
        );
    }
}

/// Row 1: wrapping integers, all four machines.
#[test]
fn a_branch_on_the_top_half_of_a_wrapping_shift_is_kept() {
    every_machine_prints(
        "kernels/lshr_top_bits.lu",
        "33554304 1 131071 15 31\n",
        None,
    );
}

/// Row 2: plain `u64` and `u8`.
#[test]
fn a_branch_on_the_top_half_of_a_u64_shift_is_kept() {
    every_machine_prints(
        "memory/raw_lshr_top_bits.lu",
        "33554304 1 15 31\n",
        Some("this operator in checked execution"),
    );
}

/// Row 3: #600's own witness, the value behind listed assembly.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn the_issues_witness_takes_both_branches_on_both_tiers() {
    let dir = scratch("issue");
    for name in ["main.lu", "rd.S"] {
        std::fs::copy(fixture(name), dir.join(name)).expect("copy the witness");
    }
    std::fs::write(
        dir.join("wolf.pkg"),
        "pkg {\n    name: \"shr-range\"\n    version: \"0.0.1\"\n    asm: [\"rd.S\"]\n}\n",
    )
    .expect("write wolf.pkg");
    rt_staticlib();
    for tier in ["native", "release"] {
        let mut cmd = Command::new(wolf());
        // Flags before the file: what follows the file is the program's
        // own argv under `wolf run`.
        cmd.current_dir(&dir).arg("run");
        if tier == "release" {
            cmd.arg("--release");
        }
        cmd.arg("main.lu");
        let out = cmd.output().expect("wolf runs");
        assert_eq!(
            (
                out.status.code(),
                String::from_utf8_lossy(&out.stdout).as_ref()
            ),
            (
                Some(0),
                "a >> 39 is true\nbranch taken: a >> 39 == 33554304\n\
                 branch taken: a >> 63 == 1\n"
            ),
            "{tier}: both branches are taken (wolf-lang#600); stderr {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
