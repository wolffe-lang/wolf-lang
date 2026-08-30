//! The pairing line cannot rot again (#87) — either half (D57).
//!
//! Three layers, weakest to strongest:
//! 1. `PAIRING` is well-formed — a semver-shaped version and a hex pin.
//! 2. `wolf --version` prints exactly what `PAIRING` declares — code
//!    and file cannot drift apart (structural: the binary includes the
//!    file, so this pins the include stays wired).
//! 3. When a sibling lupin binary exists (`LUPIN` env override first,
//!    then the conventional sibling-checkout locations), its
//!    self-reported identity must MATCH `PAIRING`. Per D57 the sibling's
//!    version tells the truth about what it is: a release build prints
//!    the bare version, an off-tag build prints `version+dev.<commit>`.
//!    So the base version must always match (a lupin release that bumped
//!    without this file being stamped fails the local gauntlet), and for
//!    a RELEASE build the declared upstream pin must match too — the
//!    half the r02 audit found rotting: 0.1.13 shipped declaring one pin,
//!    trunk re-vendored eight times, and this test compared neither the
//!    suffix nor the pin, so the gauntlet stayed green while two
//!    different interpreters both answered `lupin 0.1.13`. A dev sibling
//!    makes no release claim, so its pin is advance notice, not rot; the
//!    skip says so out loud. On a box with no sibling (bare CI), layer 3
//!    notes itself absent and layers 1–2 still hold; the local gauntlet
//!    is the gate the house trusts, and every box that runs
//!    differentials has the sibling by definition. The verdict logic
//!    itself is pure and unit-tested below, so the gate's teeth are
//!    exercised on every run of this suite, sibling or not.

use std::path::PathBuf;
use std::process::Command;

const PAIRING: &str = include_str!("../PAIRING");

fn pairing_fields() -> (String, String) {
    let mut version = None;
    let mut pin = None;
    for line in PAIRING.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("lupin-version =") {
            version = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("lupin-pin =") {
            pin = Some(v.trim().to_string());
        }
    }
    (
        version.expect("PAIRING names lupin-version"),
        pin.expect("PAIRING names lupin-pin"),
    )
}

#[test]
fn the_pairing_file_is_well_formed() {
    let (version, pin) = pairing_fields();
    let semverish = {
        let parts: Vec<&str> = version.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    };
    assert!(semverish, "lupin-version `{version}` is not semver-shaped");
    assert!(
        (7..=40).contains(&pin.len()) && pin.chars().all(|c| c.is_ascii_hexdigit()),
        "lupin-pin `{pin}` is not a git sha"
    );
}

#[test]
fn version_output_prints_the_pairing_file() {
    let (version, pin) = pairing_fields();
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("--version")
        .output()
        .expect("wolf --version runs");
    let text = String::from_utf8_lossy(&out.stdout);
    let expect = format!("paired with lupin {version} (reference interpreter), pin {pin}");
    assert!(
        text.lines().any(|l| l == expect),
        "wolf --version does not print PAIRING's line.\n  wanted: {expect}\n  got:\n{text}"
    );
}

/// The conventional sibling locations, worktree-safe: walk up from the
/// crate looking for a `wolf-interp` directory beside an ancestor.
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

/// The D57 verdict over the sibling's first `--version` line.
///
/// `lupin <version>[+dev.<commit>] (wolf-interp, reference interpreter at
/// pin <sha>)` — the base version must equal PAIRING's; the pin must equal
/// PAIRING's when the sibling is a release build (no `+` suffix — only a
/// build made at its release tag prints the bare version). `Ok` carries
/// the note saying what was compared; `Err` is the rot, spelled out.
fn sibling_verdict(first_line: &str, version: &str, pin: &str) -> Result<String, String> {
    let token = first_line
        .strip_prefix("lupin ")
        .and_then(|r| r.split_whitespace().next())
        .unwrap_or_default();
    let (base, suffix) = match token.split_once('+') {
        Some((base, suffix)) => (base, Some(suffix)),
        None => (token, None),
    };
    if base != version {
        return Err(format!(
            "the sibling lupin reports {token} but PAIRING says {version} — \
             a lupin release happened and crates/wolf_driver/PAIRING was not \
             stamped; update it with the differential ritual (#87)"
        ));
    }
    let sibling_pin = first_line
        .split(" at pin ")
        .nth(1)
        .map(|r| r.trim_end_matches(')').trim())
        .unwrap_or_default();
    match suffix {
        None => {
            // A bare version is a release claim (D57): the pin is part of
            // what that release IS, so it is compared. Prefix-tolerant so a
            // full-sha PAIRING matches lupin's 7-char spelling.
            let pins_match = !sibling_pin.is_empty()
                && (sibling_pin.starts_with(pin) || pin.starts_with(sibling_pin));
            if pins_match {
                Ok(format!(
                    "release sibling {token}: version AND pin ({sibling_pin}) both \
                     compared against PAIRING and matched"
                ))
            } else {
                Err(format!(
                    "the sibling lupin is the RELEASE build of {token} declaring pin \
                     {sibling_pin}, but PAIRING says {pin} — the pin half of the \
                     pairing is stale (the rot #87 was created to kill, r02/D57); \
                     re-run the differential ritual and stamp PAIRING"
                ))
            }
        }
        Some(suffix) => Ok(format!(
            "dev sibling {token} (`+{suffix}`): base version compared and matched; \
             the pin ({sibling_pin}) is NOT compared — an off-tag build makes no \
             release claim (D57), so a moved pin there is advance notice, not rot"
        )),
    }
}

#[test]
fn the_pairing_matches_the_sibling_lupin_when_present() {
    let Some(lupin) = sibling_lupin() else {
        eprintln!(
            "pairing: no sibling lupin on this box (set LUPIN to point at one) — \
             the rot check ran elsewhere; layers 1-2 still hold here"
        );
        return;
    };
    let out = Command::new(&lupin)
        .arg("--version")
        .output()
        .expect("sibling lupin runs");
    let first = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    let (version, pin) = pairing_fields();
    match sibling_verdict(&first, &version, &pin) {
        Ok(note) => eprintln!("pairing: {} — {note}", lupin.display()),
        Err(rot) => panic!("pairing: {} — {rot}", lupin.display()),
    }
}

/// The verdict logic runs on every suite run, sibling binary or not — a
/// gate whose teeth have never been felt is not a gate. Each direction
/// below is one the live test can take.
#[test]
fn a_release_sibling_matching_both_halves_passes() {
    let verdict = sibling_verdict(
        "lupin 0.1.14 (wolf-interp, reference interpreter at pin 90c90df)",
        "0.1.14",
        "90c90df",
    );
    assert!(verdict.is_ok(), "{verdict:?}");
}

#[test]
fn a_version_mismatch_fails_for_release_and_dev_builds_alike() {
    for line in [
        "lupin 0.1.13 (wolf-interp, reference interpreter at pin 90c90df)",
        "lupin 0.1.13+dev.a3591de (wolf-interp, reference interpreter at pin 90c90df)",
    ] {
        let verdict = sibling_verdict(line, "0.1.14", "90c90df");
        let err = verdict.expect_err("a stale version is rot whatever the build");
        assert!(err.contains("PAIRING was not stamped"), "{err}");
    }
}

#[test]
fn a_release_sibling_declaring_a_different_pin_fails() {
    // The r02 finding, verbatim: PAIRING said c9da6d9 while the sibling
    // release declared another pin — and nothing compared them.
    let verdict = sibling_verdict(
        "lupin 0.1.13 (wolf-interp, reference interpreter at pin 90c90df)",
        "0.1.13",
        "c9da6d9",
    );
    let err = verdict.expect_err("a release build's pin is identity");
    assert!(err.contains("pin half of the pairing is stale"), "{err}");
}

#[test]
fn a_dev_sibling_pin_is_not_compared_and_the_skip_says_why() {
    let verdict = sibling_verdict(
        "lupin 0.1.14+dev.a3591de (wolf-interp, reference interpreter at pin 1234567)",
        "0.1.14",
        "90c90df",
    );
    let note = verdict.expect("a dev sibling makes no release claim");
    assert!(note.contains("NOT compared"), "{note}");
    assert!(note.contains("no release claim"), "{note}");
}

#[test]
fn a_full_sha_pairing_pin_matches_the_short_spelling() {
    let verdict = sibling_verdict(
        "lupin 0.1.14 (wolf-interp, reference interpreter at pin 90c90df)",
        "0.1.14",
        "90c90df91dc314f9d0ae322d387dd10be046c828",
    );
    assert!(verdict.is_ok(), "{verdict:?}");
}

/// Layer 4 (r03, D57): the FIRST line tells the build's own truth.
/// `wolf <identity> (wolfgang[, pin <commit>])` — a bare identity is a
/// release claim and must carry its pin clause; a `+dev.<commit>`
/// identity is any off-tag or unstamped build. Shape-checked in both
/// directions so the suite passes on a dev tree and on the tag build
/// alike, while a malformed line fails everywhere.
#[test]
fn the_first_version_line_tells_the_build_truth() {
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("--version")
        .output()
        .expect("wolf --version runs");
    let text = String::from_utf8_lossy(&out.stdout);
    let first = text.lines().next().expect("a first line");
    let rest = first
        .strip_prefix(concat!("wolf ", env!("CARGO_PKG_VERSION")))
        .unwrap_or_else(|| panic!("line 1 opens with the crate version: {first}"));
    let (suffix, tail) = rest
        .split_once(" (wolfgang")
        .unwrap_or_else(|| panic!("line 1 names wolfgang (D38): {first}"));
    let pin = tail
        .strip_suffix(')')
        .unwrap_or_else(|| panic!("line 1 closes its parenthesis: {first}"))
        .strip_prefix(", pin ");
    let hexish = |s: &str| s.len() == 7 && s.chars().all(|c| c.is_ascii_hexdigit());
    if suffix.is_empty() {
        // The bare version is a release claim (D57): only a stamped
        // build makes it, and a stamped build knows its commit.
        let pin = pin.unwrap_or_else(|| panic!("a release build names its pin: {first}"));
        assert!(hexish(pin), "{first}");
    } else {
        let commit = suffix
            .strip_prefix("+dev.")
            .unwrap_or_else(|| panic!("an off-tag build spells `+dev.<commit>`: {first}"));
        match pin {
            // A stamped dev build: the pin clause repeats the commit.
            Some(p) => {
                assert!(hexish(commit) && p == commit, "{first}");
            }
            // Unstamped: no commit to claim, and none claimed.
            None => assert_eq!(commit, "unknown", "{first}"),
        }
    }
}
