//! s171 (wolf-lang#391) — the s160 `str` producers charge the ambient
//! region on the CHECKED lane too.
//!
//! `[mem.region.escape]`: `upper`, `lower`, `repeat`, `replace` and the
//! free producer `str_from_utf8` build fresh bytes in the ambient
//! region, "each a site exactly as `+` is", and
//! `[mem.region.account.1]` charges every materialization to that
//! region. Native and lupin did; the checked tier charged `+` and
//! interpolation and none of the five.
//!
//! Why a driver test and not only the corpus witness: `cargo xtask
//! corpus` executes every `phase: run` entry on the NATIVE lane, and
//! native was already right, so a corpus file alone cannot fail when
//! the checked tier regresses.
//!
//! The `cap: 0` verdict is the sharper half — a charge that takes a
//! region's ledger past its budget is `trap(alloc-contract)` at the
//! charging site (`[mem.region.cap.1]`), so a producer that charges
//! nothing RUNS where the other lanes trap. Verdicts are portable;
//! the ledger's UNITS are explicitly not comparison surface
//! (`[mem.region.account.1]`), so nothing here pins a byte count.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

struct Obs {
    verdict: String,
    stdout: String,
}

fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) && flag == "--native" {
        eprintln!(
            "SKIP: environment cannot run the native lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    })
}

fn fixture(case: &str, body: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, body).expect("write fixture");
    entry
}

/// The five producers of `[mem.region.escape]`, plus `+` as the
/// control that was already right on every lane.
const PRODUCERS: &[(&str, &str)] = &[
    ("plus", r#"base + base"#),
    ("upper", r#"base.upper()"#),
    ("lower", r#"base.lower()"#),
    ("repeat", r#"base.repeat(2)"#),
    ("replace", r#"base.replace("r", "x")"#),
    ("from_utf8", r#"str_from_utf8(base.bytes()) else "?""#),
];

/// Each producer moves the ambient region's ledger, on both lanes.
/// The boolean is the portable claim; the byte count is not.
#[test]
fn every_str_producer_charges_the_ambient_region_on_both_lanes() {
    for (name, expr) in PRODUCERS {
        let src = format!(
            r#"
fn main() -> !int {{
    let base = "0123456789abcdef"
    var charged = 0
    region pass {{
        let s = {expr}
        charged = region_bytes(pass)
        if s.len == 0 {{ return 1 }}
    }}
    print("charged {{charged > 0}}")
    0
}}
"#
        );
        let entry = fixture(&format!("s171_charge_{name}"), &src);
        let checked = lane(&entry, "--checked").expect("checked lane runs");
        assert_eq!(
            checked.stdout, "charged true\n",
            "the CHECKED lane charged nothing for `{name}` (#391)"
        );
        if let Some(native) = lane(&entry, "--native") {
            assert_eq!(
                native.stdout, "charged true\n",
                "the native lane charged nothing for `{name}`"
            );
        }
    }
}

/// The verdict form: under `cap: 0` a producer that charges the region
/// trips `[mem.region.cap.1]` at the charging site. A tier that charges
/// nothing prints and exits 0 instead, which is how #391 showed up as
/// a difference a user can see rather than a ledger detail.
#[test]
fn a_zero_cap_region_refuses_every_str_producer_on_both_lanes() {
    for (name, expr) in PRODUCERS {
        let src = format!(
            r#"
fn main() -> !int {{
    let base = "re"
    region idle(cap: 0) {{
        let s = {expr}
        print(s)
    }}
    0
}}
"#
        );
        let entry = fixture(&format!("s171_cap_{name}"), &src);
        let checked = lane(&entry, "--checked").expect("checked lane runs");
        assert_eq!(
            checked.verdict, "trap(alloc-contract)",
            "the CHECKED lane admitted `{name}` under cap: 0 (#391)"
        );
        if let Some(native) = lane(&entry, "--native") {
            assert_eq!(
                native.verdict, "trap(alloc-contract)",
                "the native lane admitted `{name}` under cap: 0"
            );
        }
    }
}
