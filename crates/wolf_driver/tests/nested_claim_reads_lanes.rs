//! s186 (wolf-lang#476, ruled 2026-09-30: arguments are two-phase,
//! `[mem.tier0.excl.4]`). Within one call the arguments evaluate left
//! to right and a `mut` argument's claim takes effect at call entry, so
//! a later argument may READ the claimed place — a `Copy` value, an
//! operand, a header or member, a whole read one call down — and sees
//! the value from before the call; it may not WRITE or MOVE it (E1002
//! on the checked, native and release lanes).
//!
//! Before s186 the reads one level down ran on every wolfgang lane
//! (the bytes asserted here), a direct `Copy` or member read was E1002
//! (D39), and the block write ran with a wrong answer: `6` on checked,
//! `2` on native and release. Each refused row asserts E1002 is its
//! ONLY diagnostic.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` checks a `fail(..)` entry's phase and runs a
//! `phase: run` entry on the NATIVE lane only; it cannot see the checked
//! machine's answer, nor lupin's.
//!
//! lupin's column is wolf-interp's. 0.1.40 through 0.1.42 trap every
//! read row `exclusivity` (measured, kasumi `~/lanes/s186/evidence/`);
//! those versions are pinned pre-mirror by name, and any later lupin
//! must print the ruled bytes (is63 moves lupin). The refused rows trap
//! on every version.

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

/// lupin releases measured before the mirror of `[mem.tier0.excl.4]`:
/// each traps a read of a claimed place in a later argument.
const PRE_TWO_PHASE_LUPIN: &[&str] = &["0.1.40", "0.1.41", "0.1.42"];

/// Every wolfgang lane answers `wolfgang`; lupin answers `pre` at one
/// of `pre_versions` and `ruled` at any other.
fn every_lane_pinned(
    name: &str,
    pre_versions: &[&str],
    wolfgang: Want<'_>,
    pre: Want<'_>,
    ruled: Want<'_>,
) {
    let entry = corpus(name);
    let checked = lane(&entry, "--checked").expect("the checked lane always runs");
    check("checked", &entry, &checked, wolfgang);
    for flag in ["--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            check(flag, &entry, &obs, wolfgang);
        }
    }
    let Some((version, lupin)) = lupin_says(&entry) else {
        return;
    };
    if pre_versions.contains(&version.as_str()) {
        check(
            &format!("lupin {version} (pre-mirror)"),
            &entry,
            &lupin,
            pre,
        );
    } else {
        check(&format!("lupin {version}"), &entry, &lupin, ruled);
    }
}

/// A read row: the reads-before-the-call bytes on every wolfgang lane
/// and on any lupin past the pinned ones.
fn reads_run(name: &str, stdout: &str) {
    let want = runs(stdout);
    every_lane_pinned(
        name,
        PRE_TWO_PHASE_LUPIN,
        want,
        verdict("trap(exclusivity)"),
        want,
    );
}

/// A refused row: only E1002s on every wolfgang lane, lupin's trap.
fn refused(name: &str) {
    let e = verdict("fail(E1002)");
    let trap = verdict("trap(exclusivity)");
    every_lane_pinned(name, &[], e, trap, trap);
    let entry = corpus(name);
    for flag in ["--checked", "--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            assert!(
                !obs.codes.is_empty() && obs.codes.iter().all(|c| c == "E1002"),
                "the {flag} diagnostics on {}: {:?}",
                entry.display(),
                obs.codes
            );
        }
    }
}

// ------------------------------------------------ #476's four shapes --

/// `bump(mut xs[0], total(xs))` — eg02b's find.
#[test]
fn a_whole_read_lend_one_call_down_under_an_element_claim_runs() {
    reads_run("mut_claim_nested_read_elem.lu", "4 3\n");
}

/// `grow(mut xs, total(xs))`.
#[test]
fn a_whole_read_lend_one_call_down_under_a_whole_claim_runs() {
    reads_run("mut_claim_nested_read_whole.lu", "1 4\n");
}

/// `bump(mut xs[0], xs.get(1) else 9)` — s185's first.
#[test]
fn a_whole_reading_receiver_under_an_element_claim_runs() {
    reads_run("mut_claim_nested_get.lu", "3\n");
}

/// `grow(mut xs, xs.count())` — s185's second.
#[test]
fn a_header_method_under_a_whole_claim_runs() {
    reads_run("mut_claim_nested_header.lu", "4\n");
}

// ----------------------------------------- more reads, found by s186 --

/// `bump(mut xs[0], id(xs[0]))`.
#[test]
fn a_copy_read_of_the_claimed_element_one_call_down_runs() {
    reads_run("mut_claim_nested_copy_read.lu", "2\n");
}

/// `bump(mut a, a + 1)`.
#[test]
fn an_operand_read_of_the_claimed_place_runs() {
    reads_run("mut_claim_operand_read.lu", "3\n");
}

/// `mput(mut m, msize(m))`.
#[test]
fn a_whole_map_read_one_call_down_under_its_claim_runs() {
    reads_run("mut_claim_nested_map.lu", "2\n");
}

/// The direct reads D39 refused (`bump(mut a, a)`, `grow(mut xs,
/// xs.len)`), the downstream shapes (interpolation, an operand, a
/// nested call over the struct) and a sibling-field receiver.
#[test]
fn direct_and_downstream_reads_run() {
    reads_run("mut_claim_two_phase_reads.lu", "2 4 23 1 3 5 2 3\n");
}

// ------------------------------------------ writes and moves refused --

/// `bump(mut a, { a = 5; 1 })` — the wrong answer (checked `6`,
/// native and release `2`).
#[test]
fn a_write_of_the_claimed_place_in_a_later_argument_is_refused() {
    refused("mut_claim_arg_block_write.lu");
}

/// `grow(mut xs, { var t = move xs; xs = [9]; t.len })`.
#[test]
fn a_move_of_the_claimed_place_in_a_later_argument_is_refused() {
    refused("mut_claim_arg_block_move.lu");
}

// ------------------------------------------------- what always ran --

/// Disjoint element, header beside an element claim, a read before the
/// claim, a disjoint field, a header method in a branch.
#[test]
fn disjoint_and_earlier_reads_still_run() {
    let want = runs("11 3\n");
    every_lane_pinned("mut_claim_nested_disjoint_reads.lu", &[], want, want, want);
}
