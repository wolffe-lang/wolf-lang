//! s186 (wolf-lang#476) — `[mem.tier0.excl.1]` inside a call's own
//! arguments: the arguments after a `mut` argument are evaluated inside
//! its claim (`[mem.model.order]`), so a read or a write of the claimed
//! place there — one call down, as an operand, in a block — is E1002 on
//! the checked, native and release lanes, exactly as the direct forms
//! (`both(mut xs[0], xs)`, `f(mut a, a.x)`) always were.
//!
//! Before s186 every refused row here ran on all three wolfgang lanes
//! (`4 3`, `1 4`, `3`, `4`, `2`, `3`, `2`) while lupin trapped
//! `exclusivity`, and the block-write row answered `6` on checked and
//! `2` on native and release. The nested-claim check (s168/s178) looked
//! only for a nested call's `mut`/`take` arguments. Each refused row
//! asserts E1002 is its ONLY diagnostic: the new scan must not echo the
//! direct checks.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` checks a `fail(..)` entry's phase and runs a
//! `phase: run` entry on the NATIVE lane only; it cannot see the checked
//! machine's answer, nor lupin's.
//!
//! lupin's column is wolf-interp's: 0.1.40, 0.1.41 and 0.1.42 trap every
//! refused shape (measured, kasumi `~/lanes/s186/evidence/` and s185's
//! probes), so no version is pinned pre-mirror.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    codes: Vec<String>,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        codes: rec["diagnostics"]
            .as_array()
            .map(|ds| {
                ds.iter()
                    .filter_map(|d| d["code"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// One wolfgang lane. `None` means the host cannot run a native lane
/// (the s59 skip pattern), never a silent pass.
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) && flag != "--checked" {
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

/// lupin's version and observation, or `None` when this box has no
/// sibling. `WOLF_PAIRING_REQUIRE_SIBLING` is the linux CI job saying
/// "one was arranged here" (r10/#253): there an absent sibling is a
/// failed fetch, and a gate that answers broken plumbing with a green
/// skip is not a gate.
fn lupin_says(entry: &Path) -> Option<(String, Obs)> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of this gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still compared against the RULED answer here; only the oracle \
             leg is absent"
        );
        return None;
    };
    let v = Command::new(&lupin)
        .arg("--version")
        .output()
        .expect("lupin --version runs");
    let version = String::from_utf8_lossy(&v.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string();
    assert!(!version.is_empty(), "lupin --version printed no version");
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    Some((version, parse_obs(&out.stdout, "lupin's observation")))
}

/// The corpus row itself is the program: one truth per file.
fn corpus(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/memory")
        .join(name);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// What one machine must answer: a verdict, and for a run the bytes.
#[derive(Clone, Copy)]
struct Want<'a> {
    verdict: &'a str,
    stdout: Option<&'a str>,
}

const fn runs(stdout: &str) -> Want<'_> {
    Want {
        verdict: "exit(0)",
        stdout: Some(stdout),
    }
}

const fn verdict(v: &str) -> Want<'_> {
    Want {
        verdict: v,
        stdout: None,
    }
}

fn check(machine: &str, entry: &Path, obs: &Obs, want: Want<'_>) {
    assert_eq!(
        obs.verdict,
        want.verdict,
        "the {machine} verdict on {} (diagnostics {:?})",
        entry.display(),
        obs.codes
    );
    if let Some(bytes) = want.stdout {
        assert_eq!(
            obs.stdout,
            bytes,
            "the {machine} answer on {}",
            entry.display()
        );
    }
}

/// Every wolfgang lane answers `wolfgang`; lupin answers `oracle`.
fn every_lane(name: &str, wolfgang: Want<'_>, oracle: Want<'_>) {
    let entry = corpus(name);
    let checked = lane(&entry, "--checked").expect("the checked lane always runs");
    check("checked", &entry, &checked, wolfgang);
    for flag in ["--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            check(flag, &entry, &obs, wolfgang);
        }
    }
    if let Some((version, lupin)) = lupin_says(&entry) {
        check(&format!("lupin {version}"), &entry, &lupin, oracle);
    }
}

/// A refused row: E1002 alone on every wolfgang lane, lupin's trap.
fn refused(name: &str) {
    every_lane(name, verdict("fail(E1002)"), verdict("trap(exclusivity)"));
    let entry = corpus(name);
    for flag in ["--checked", "--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            assert_eq!(
                obs.codes,
                vec!["E1002".to_string()],
                "the {flag} diagnostics on {}",
                entry.display()
            );
        }
    }
}

// ------------------------------------------------ #476's four shapes --

/// `bump(mut xs[0], total(xs))` — eg02b's find.
#[test]
fn a_whole_read_lend_one_call_down_under_an_element_claim_is_refused() {
    refused("mut_claim_nested_read_elem.lu");
}

/// `grow(mut xs, total(xs))`.
#[test]
fn a_whole_read_lend_one_call_down_under_a_whole_claim_is_refused() {
    refused("mut_claim_nested_read_whole.lu");
}

/// `bump(mut xs[0], xs.get(1) else 9)` — s185's first.
#[test]
fn a_whole_reading_receiver_under_an_element_claim_is_refused() {
    refused("mut_claim_nested_get.lu");
}

/// `grow(mut xs, xs.count())` — s185's second.
#[test]
fn a_header_method_under_a_whole_claim_is_refused() {
    refused("mut_claim_nested_header.lu");
}

// ----------------------------------------- the same rule, found here --

/// `bump(mut xs[0], id(xs[0]))`.
#[test]
fn a_copy_read_of_the_claimed_element_one_call_down_is_refused() {
    refused("mut_claim_nested_copy_read.lu");
}

/// `bump(mut a, a + 1)`.
#[test]
fn an_operand_read_of_the_claimed_place_is_refused() {
    refused("mut_claim_operand_read.lu");
}

/// `bump(mut a, { a = 5; 1 })` — the wrong answer (checked `6`,
/// native and release `2`).
#[test]
fn a_write_of_the_claimed_place_in_a_later_argument_is_refused() {
    refused("mut_claim_arg_block_write.lu");
}

/// `mput(mut m, msize(m))`.
#[test]
fn a_whole_map_read_one_call_down_under_its_claim_is_refused() {
    refused("mut_claim_nested_map.lu");
}

// ------------------------------------------------- what keeps running --

/// Disjoint element, header beside an element claim, a read before the
/// claim, a disjoint field, a header method in a branch.
#[test]
fn disjoint_and_earlier_reads_still_run() {
    let want = runs("11 3\n");
    every_lane("mut_claim_nested_disjoint_reads.lu", want, want);
}
