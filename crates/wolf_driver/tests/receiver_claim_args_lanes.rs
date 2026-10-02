//! s192 (wolf-lang#487, found by is63). A `mut` receiver is the
//! call's first argument (`[mem.tier0.excl.4]`): its own arguments may
//! READ it and see the value from before the call, but a WRITE, MOVE,
//! re-claim (`mut`) or lend of it inside them is E1002 on the checked,
//! native and release lanes.
//!
//! Before s192 the receiver's claim never reached the argument scan
//! s186 wrote (`check_nested_claims_after_mut`), so the writes, moves
//! and nested re-claims ran with no diagnostic on every wolfgang lane,
//! trunk `57805e35` and the 0.2.19 archive alike: the issue's shape
//! printed `1 9` (the push lost), a view-set receiver's own field
//! printed `51` on checked and `2` on native and release, and a
//! re-claim through a call ran to `trap(bounds)`. The direct re-claim,
//! the `read` lend and the capturing closure were already refused by
//! the call surface's pairwise check; their rows record that. Each
//! refused row asserts E1002 is its ONLY diagnostic.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` checks a `fail(..)` entry's phase and runs a
//! `phase: run` entry on the NATIVE lane only; it cannot see the checked
//! machine's answer, nor lupin's.
//!
//! lupin's column is wolf-interp's. lupin 0.1.42 holds no receiver claim
//! while the arguments run: it answers `ub(mem.ub)` where its Tree
//! Borrows model catches the access, and RUNS the rest (measured, kasumi
//! `~/lanes/s192/evidence/probes-trunk-57805e35.log`). 0.1.42 is pinned
//! pre-mirror by version with those answers, row by row; any later
//! lupin must trap `exclusivity` (is62 moves lupin).

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

/// lupin releases measured before the mirror of wolf-lang#487: 0.1.42.
/// Emptied at the 0.1.43 pairing (r25): 0.1.43 carries is62's receiver
/// claim (wolffe-lang/wolf-interp#164) and traps `exclusivity` on every row.
const PRE_RECEIVER_LUPIN: &[&str] = &[];

/// A refused row: only E1002s on every wolfgang lane; lupin answers
/// `pre` at a pinned version and traps `exclusivity` at any other.
fn refused(name: &str, pre: Want<'_>) {
    let entry = corpus(name);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        check(flag, &entry, &obs, verdict("fail(E1002)"));
        assert!(
            !obs.codes.is_empty() && obs.codes.iter().all(|c| c == "E1002"),
            "the {flag} diagnostics on {}: {:?}",
            entry.display(),
            obs.codes
        );
    }
    let Some((version, lupin)) = lupin_says(&entry) else {
        return;
    };
    if PRE_RECEIVER_LUPIN.contains(&version.as_str()) {
        check(
            &format!("lupin {version} (pre-mirror)"),
            &entry,
            &lupin,
            pre,
        );
    } else {
        check(
            &format!("lupin {version}"),
            &entry,
            &lupin,
            verdict("trap(exclusivity)"),
        );
    }
}

const UB: Want<'static> = verdict("ub(mem.ub)");

// ----------------------------------------------- the issue's shapes --

/// `(mut xs).push({ xs = [9]; 5 })` — wolf-lang#487 itself (`1 9`).
#[test]
fn a_write_of_the_receiver_in_its_own_argument_is_refused() {
    refused("recv_claim_arg_write.lu", UB);
}

/// `(mut xs).push({ var t = move xs; xs = [9]; t.len })` (`1 9`).
#[test]
fn a_move_of_the_receiver_in_its_own_argument_is_refused() {
    refused("recv_claim_arg_move.lu", UB);
}

// --------------------------------------------------------- re-claims --

/// `(mut a).absorb(mut a)` — refused by the surface before s192.
#[test]
fn the_receiver_claimed_again_as_an_argument_is_refused() {
    refused("recv_claim_arg_reclaim.lu", runs("6\n"));
}

/// `(mut xs).push({ (mut xs).push(7); 5 })` (`4 7`).
#[test]
fn the_receiver_claimed_again_inside_its_argument_is_refused() {
    refused("recv_claim_arg_reclaim_nested.lu", UB);
}

/// `(mut xs).push(drain(mut xs))` (`trap(bounds)`).
#[test]
fn the_receiver_lent_mut_to_a_call_in_its_argument_is_refused() {
    refused("recv_claim_arg_reclaim_call.lu", UB);
}

// ------------------------------------------------------------- lends --

/// `(mut a).apply(fn() a.n)` — refused by the surface before s192.
#[test]
fn a_closure_argument_capturing_the_receiver_is_refused() {
    refused("recv_claim_arg_closure.lu", runs("12\n"));
}

/// `(mut a).absorb(a)`, `a` not `Copy` — refused by the surface before
/// s192.
#[test]
fn the_receiver_lent_read_into_its_own_call_is_refused() {
    refused("recv_claim_arg_lend.lu", runs("6\n"));
}

// ------------------------------------------------- element receivers --

/// `(mut xs[0]).push({ xs[0] = [7, 7]; 5 })` (`2 7`).
#[test]
fn a_write_of_an_element_receiver_in_its_argument_is_refused() {
    refused("recv_claim_arg_elem_write.lu", UB);
}

/// `(mut xs[0]).push({ var t = move xs[0]; xs[0] = [7]; t.len })`
/// (`1 7`).
#[test]
fn a_move_of_an_element_receiver_in_its_argument_is_refused() {
    refused("recv_claim_arg_elem_move.lu", UB);
}

/// `(mut xs[0]).push({ xs = [[7]]; 5 })` — the container replaced
/// (`1 1`).
#[test]
fn the_container_of_an_element_receiver_written_in_its_argument_is_refused() {
    refused("recv_claim_arg_elem_whole_write.lu", runs("1 2\n"));
}

// --------------------------------------------------- field receivers --

/// `(mut out.class_off).push({ out.class_off = [9]; 5 })` (`1 9`).
#[test]
fn a_write_of_a_field_receiver_in_its_argument_is_refused() {
    refused("recv_claim_arg_field_write.lu", UB);
}

/// The field moved out and stored back (`1 9`).
#[test]
fn a_move_of_a_field_receiver_in_its_argument_is_refused() {
    refused("recv_claim_arg_field_move.lu", UB);
}

/// `out = Out { … }` under the claim on `out.class_off` (`1 9`).
#[test]
fn the_struct_of_a_field_receiver_written_in_its_argument_is_refused() {
    refused("recv_claim_arg_field_prefix_write.lu", runs("2 9\n"));
}

/// `(mut p).set_x({ p.x = 50; 1 })` under `mut self.{x}` — checked
/// `51`, native and release `2`.
#[test]
fn a_write_of_a_view_set_receivers_field_in_its_argument_is_refused() {
    refused("recv_claim_arg_view_write.lu", runs("2\n"));
}

// ------------------------------------------------------ what still runs --

/// Header and whole reads, a block's own local, bu12's sibling-field
/// read, a sibling-field write, element-receiver reads, a different
/// literal element written, a write after the call, a view-set field
/// read. Every machine, every lupin.
#[test]
fn reads_and_disjoint_writes_beside_a_mut_receiver_run() {
    let entry = corpus("recv_claim_arg_reads.lu");
    let want = runs("5 2 5 5 2 3 3 5 3 3 2 10 5\n");
    let checked = lane(&entry, "--checked").expect("the checked lane always runs");
    check("checked", &entry, &checked, want);
    for flag in ["--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            check(flag, &entry, &obs, want);
        }
    }
    if let Some((version, lupin)) = lupin_says(&entry) {
        check(&format!("lupin {version}"), &entry, &lupin, want);
    }
}
