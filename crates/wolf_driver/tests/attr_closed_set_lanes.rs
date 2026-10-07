//! kw01 (K13 and K7, STATUS #31 — wolf-lang#519, #524): the attribute
//! set is closed on every wolfgang lane, `#[cfg(target = …)]` is
//! evaluated, and `"c"` is the only ABI string.
//!
//! Measured at trunk 12a56b22 (kasumi, `~/lanes/kw01/evidence/
//! red-trunk-12a56b22-rows.log`): every refused row RAN on `--checked`,
//! `--native` and `--release` (`exit(0)`; `attr_thread_local` was
//! refused later, at `mem`, for the module `var`), the two arch-gated
//! definitions of `cfg_target_arch` collided (E0302), and the
//! freestanding-gated code of `cfg_target_freestanding` was resolved
//! (E0301). lupin 0.1.43 and 0.1.44 read no attribute either: they run every
//! refused row, and each parting is pinned below by version as
//! pre-mirror (the lupin half is wolf-interp#174); a newer lupin
//! must answer what the compiler answers. 0.1.45 (the 0.2.22 pairing,
//! r27) still reads none, measured, and is pinned with 0.1.44's answers
//! but one: is68's wide scalar pass (#138) types the freestanding-gated
//! code `cfg_target_freestanding.lu` keeps, so 0.1.45 answers
//! `fail(E0401)` there where 0.1.44 declined it `unsupported` — the same
//! unread `cfg`, reached one pass sooner. 0.1.46 (the 0.2.23 pairing,
//! r28) carries is70's mirror (#174): every refused row and both `cfg`
//! rows answer the compiler's column, so their pins are dropped; the
//! control `attr_implemented_set.lu` stays `unsupported` (lupin declines
//! its `comptime fn`) and is carried to 0.1.46, to 0.1.47 (the 0.2.24
//! pairing, r29) and to 0.1.48 (the 0.2.25 pairing, r30), measured.
//!
//! Why a driver gate beside the corpus rows: `cargo xtask corpus` reads
//! a row on its default lane only, and the rule is "on every machine".

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
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
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
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        codes,
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
    }
}

/// The native and release lanes need `libwolf_rt.a` beside `wolf`;
/// without it they SKIP and the gate measures one lane (kw01 met this
/// as a vacuous green at trunk). Build it first.
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

/// One wolfgang lane; `None` is the s59 environment skip (never an ICE).
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    ensure_rt_staticlib();
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
             (LUPIN={}) — the oracle leg of kw01's gate did not run",
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
    Some(parse_obs(&out.stdout, "lupin's observation"))
}

fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// What a lane must answer: a verdict, the sorted codes, the stdout.
struct Want<'a> {
    verdict: &'a str,
    codes: &'a [&'a str],
    stdout: &'a str,
}

/// lupin 0.1.43's measured answer where it parts (pre-mirror).
struct Pin<'a> {
    version: &'a str,
    verdict: &'a str,
    stdout: &'a str,
}

fn every_lane(row: &str, want: Want<'_>, lupin_pre_mirror: &[Pin<'_>]) {
    let entry = corpus(row);
    let codes: Vec<String> = want.codes.iter().map(|c| c.to_string()).collect();
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        let lane_name = if flag == "--checked" {
            "the CHECKED lane"
        } else {
            flag
        };
        assert_eq!(
            obs.verdict, want.verdict,
            "{lane_name} on {row} (K13/K7: read or refused by name, never ignored); codes \
             {:?}, stdout {:?}",
            obs.codes, obs.stdout
        );
        assert_eq!(obs.codes, codes, "{lane_name}'s diagnostics on {row}");
        assert_eq!(obs.stdout, want.stdout, "{lane_name}'s stdout on {row}");
    }
    let Some(lupin) = lupin_says(&entry) else {
        return;
    };
    match lupin_pre_mirror.iter().find(|p| p.version == lupin.version) {
        Some(pin) => {
            assert_eq!(
                lupin.verdict, pin.verdict,
                "lupin {} (pre-mirror) verdict on {row}",
                lupin.version
            );
            assert_eq!(
                lupin.stdout, pin.stdout,
                "lupin {} (pre-mirror) stdout on {row}",
                lupin.version
            );
        }
        None => {
            assert_eq!(
                lupin.verdict, want.verdict,
                "lupin {}'s answer on {row} (the mirror of K13/K7 is wolf-interp#174); codes {:?}",
                lupin.version, lupin.codes
            );
            assert_eq!(
                lupin.codes, codes,
                "lupin {}'s codes on {row}",
                lupin.version
            );
            assert_eq!(
                lupin.stdout, want.stdout,
                "lupin {}'s stdout on {row}",
                lupin.version
            );
        }
    }
}

fn refused(row: &str, codes: &[&str]) {
    let verdict = format!("fail({})", codes[0]);
    every_lane(
        row,
        Want {
            verdict: &verdict,
            codes,
            stdout: "",
        },
        &[],
    );
}

#[test]
fn an_unknown_attribute_is_refused() {
    refused("grammar/attr_unknown.lu", &["E0817"]);
}

#[test]
fn an_unimplemented_contract_is_refused() {
    refused("grammar/attr_contract_unimplemented.lu", &["E0817"]);
}

#[test]
fn an_unimplemented_repr_is_refused() {
    refused("grammar/attr_repr_unimplemented.lu", &["E0817"]);
}

/// lc00's line: three refusals, one per item.
#[test]
fn every_item_of_a_bogus_repr_line_is_refused() {
    refused("grammar/attr_repr_bogus.lu", &["E0817", "E0817", "E0817"]);
}

/// kw09 implemented `#[section]` (`[abi.link.section]`, K6); what stays
/// E0817 is the attribute on a `const` (no storage) and Rust's
/// `link_section` spelling.
#[test]
fn a_section_on_a_const_and_link_section_are_refused() {
    refused("grammar/attr_section.lu", &["E0817", "E0817"]);
}

#[test]
fn thread_local_is_refused_until_it_is_implemented() {
    refused("grammar/attr_thread_local.lu", &["E0817"]);
}

#[test]
fn an_attribute_where_nothing_reads_it_is_refused() {
    refused("grammar/attr_misplaced.lu", &["E0817", "E0817"]);
}

#[test]
fn a_cfg_naming_no_target_is_refused() {
    refused("grammar/cfg_target_unknown.lu", &["E0817"]);
}

#[test]
fn a_cfg_predicate_wolf_does_not_decide_is_refused() {
    refused("grammar/cfg_predicate_unknown.lu", &["E0817"]);
}

/// wolf-lang#524: kw00's interrupt handler.
#[test]
fn an_abi_string_other_than_c_is_refused() {
    refused("grammar/extern_abi_interrupt.lu", &["E0818"]);
}

/// Exactly one arch-gated definition survives on every tier-1 host.
#[test]
fn cfg_keeps_the_definition_for_this_target_only() {
    every_lane(
        "grammar/cfg_target_arch.lu",
        Want {
            verdict: "exit(0)",
            codes: &[],
            stdout: "64\n",
        },
        &[],
    );
}

/// What the freestanding target gates is never resolved or typed.
#[test]
fn cfg_drops_what_another_target_gates_before_resolution() {
    every_lane(
        "grammar/cfg_target_freestanding.lu",
        Want {
            verdict: "exit(0)",
            codes: &[],
            stdout: "hosted\n1\n",
        },
        &[],
    );
}

/// The control: the implemented set compiles and runs unchanged. lupin
/// declines its `comptime fn` (`unsupported`), 0.1.46 included.
#[test]
fn the_implemented_attributes_still_compile() {
    every_lane(
        "grammar/attr_implemented_set.lu",
        Want {
            verdict: "exit(0)",
            codes: &[],
            stdout: "3\n7\n",
        },
        &[
            Pin {
                version: "0.1.43",
                verdict: "unsupported",
                stdout: "",
            },
            Pin {
                version: "0.1.44",
                verdict: "unsupported",
                stdout: "",
            },
            Pin {
                version: "0.1.45",
                verdict: "unsupported",
                stdout: "",
            },
            Pin {
                version: "0.1.46",
                verdict: "unsupported",
                stdout: "",
            },
            Pin {
                version: "0.1.47",
                verdict: "unsupported",
                stdout: "",
            },
            Pin {
                version: "0.1.48",
                verdict: "unsupported",
                stdout: "",
            },
        ],
    );
}
