//! The pairing line cannot rot again (#87).
//!
//! Three layers, weakest to strongest:
//! 1. `PAIRING` is well-formed — a semver-shaped version and a hex pin.
//! 2. `wolf --version` prints exactly what `PAIRING` declares — code
//!    and file cannot drift apart (structural: the binary includes the
//!    file, so this pins the include stays wired).
//! 3. When a sibling lupin binary exists (`LUPIN` env override first,
//!    then the conventional sibling-checkout locations), its
//!    self-reported version must MATCH `PAIRING` — a lupin release
//!    that bumped without this file being stamped fails the local
//!    gauntlet, which is where the differential ritual lives. On a
//!    box with no sibling (bare CI), this layer notes itself absent
//!    and layers 1–2 still hold; the local gauntlet is the gate the
//!    house trusts, and every box that runs differentials has the
//!    sibling by definition.

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
    // `lupin 0.1.13 (wolf-interp, reference interpreter at pin <sha>)`
    let sibling_version = first
        .strip_prefix("lupin ")
        .and_then(|r| r.split_whitespace().next())
        .unwrap_or_default()
        .to_string();
    let (version, _) = pairing_fields();
    assert_eq!(
        sibling_version,
        version,
        "the sibling lupin ({}) reports {sibling_version} but PAIRING says {version} — \
         a lupin release happened and crates/wolf_driver/PAIRING was not stamped; \
         update it with the differential ritual (#87)",
        lupin.display()
    );
}
