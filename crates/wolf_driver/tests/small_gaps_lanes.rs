//! s252 — the small gaps (wolf-lang#800, #813, #814, #816, #822, #801),
//! every row on every machine, with lupin's column beside it.
//!
//! `cargo xtask corpus` runs a `phase: run` row natively and a refusal
//! row on the default lane; this gate runs each of s252's rows on the
//! checked machine, native and release, and asserts one answer. lupin
//! (the PAIRING sibling) is asserted at what 0.1.54 answered and 0.1.55,
//! the pairing since 0.2.34, answers alike on every row: where it
//! agrees, the same answer; where it owes a clause (`[mem.static.3]`'s
//! `wrapping[T]`, `[mem.region.scalar]`) or diverges
//! (`numlit_wrapping_countdown.lu`: it accepts an `int` count on a
//! wrapping shift), its own answer, so a lupin that follows — or moves
//! the other way — is loud here and its column gets rewritten with it.

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
    warnings: Vec<String>,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        warnings: rec["warnings"]
            .as_array()
            .map(|ws| {
                ws.iter()
                    .filter_map(|w| w["code"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

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
             (LUPIN={}) — the lupin column of s252's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the three wolfgang \
             lanes are still asserted; only lupin's column is absent"
        );
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

/// What lupin (0.1.54 and 0.1.55) answers, when it is not the three lanes'.
enum Lupin {
    Same,
    Verdict(&'static str),
    Run(&'static str),
}

/// A run row: exit(0) and `want` on the checked machine, native and
/// release; `warns` the warning codes each lane reports.
fn run_row(rel: &str, want: &str, warns: &[&str], lupin: Lupin, why: &str) {
    let entry = corpus(rel);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        assert_eq!(obs.verdict, "exit(0)", "{rel} on {flag}: {why}");
        assert_eq!(obs.stdout, want, "{rel} on {flag}: the answer");
        assert_eq!(obs.warnings, warns, "{rel} on {flag}: the warnings");
    }
    if let Some(l) = lupin_says(&entry) {
        match lupin {
            Lupin::Same => {
                assert_eq!(l.verdict, "exit(0)", "{rel}: lupin's verdict");
                assert_eq!(l.stdout, want, "{rel}: lupin's answer");
            }
            Lupin::Verdict(v) => assert_eq!(
                l.verdict, v,
                "{rel}: lupin's column moved — rewrite it here, in the row's \
                 header and in the clause's lupin sentence"
            ),
            Lupin::Run(out) => {
                assert_eq!(l.verdict, "exit(0)", "{rel}: lupin's verdict");
                assert_eq!(
                    l.stdout, out,
                    "{rel}: lupin's divergence moved — rewrite its column"
                );
            }
        }
    }
}

/// A refusal row: `fail(code)` on the three lanes.
fn fail_row(rel: &str, code: &str, why: &str) {
    let entry = corpus(rel);
    let want = format!("fail({code})");
    for flag in ["--checked", "--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            assert_eq!(obs.verdict, want, "{rel} on {flag}: {why}");
        }
    }
}

/// #813: an adapter of a `Copy` scalar is `Copy`
/// (`[mem.tier0.move.3]`); an adapter of a `List` still moves.
#[test]
fn an_adapter_of_a_copy_scalar_is_copy() {
    run_row(
        "memory/distinct_copy_scalar.lu",
        "6 6 14 49 49\n",
        &[],
        Lupin::Same,
        "a nominal adapter of int, u32 or wrapping[u64] read twice — before s252 E1001",
    );
    fail_row(
        "memory/distinct_container_moves.lu",
        "E1001",
        "an adapter of a List moves as the List does",
    );
}

/// #814: module state of `wrapping[T]` is static data
/// (`[mem.static.3]`). lupin 0.1.55 refuses it as unsupported, as 0.1.54 did.
#[test]
fn wrapping_module_state_is_static_data() {
    run_row(
        "memory/static_wrapping.lu",
        "63 0 44 127 1\n",
        &[],
        Lupin::Verdict("unsupported"),
        "a const, let and var of wrapping types — before s252 unsupported, not static data",
    );
}

/// #816, #801, #822 (1): a value whose type reaches no storage names
/// no region (`[mem.region.scalar]`), and `copy` of a call result is a
/// fresh ambient value.
#[test]
fn a_scalar_only_value_names_no_region() {
    run_row(
        "memory/region_scalar_value.lu",
        "7 1 2 3 4 5 9\n",
        &[],
        Lupin::Verdict("trap(region-fault)"),
        "scalar-only literals, bindings, an enum payload, a for binding and a call result \
         across a region — before s252 E1010",
    );
    run_row(
        "memory/region_scalar_mut.lu",
        "3 2 1\n",
        &[],
        Lupin::Same,
        "a scalar-only mut argument inside a region — before s252 E1010 (#801)",
    );
    run_row(
        "memory/region_scalar_store.lu",
        "1 1 2 1\n",
        &[],
        Lupin::Verdict("trap(region-fault)"),
        "a scalar-only store into another parameter's region, and copy of a call result — \
         before s252 E1004 (#816)",
    );
    run_row(
        "memory/region_scalar_freeze.lu",
        "7\n",
        &[],
        Lupin::Verdict("trap(region-fault)"),
        "a scalar-only value read out of a frozen region is the binding's own words — \
         before s252 E1012",
    );
}

/// What the narrowing leaves refused: storage, and only storage.
#[test]
fn storage_keeps_every_region_rule() {
    fail_row(
        "memory/region_scalar_list_store.lu",
        "E1004",
        "a List-holding call result stored into another parameter's region, no copy",
    );
    fail_row(
        "memory/region_scalar_str_escape.lu",
        "E1010",
        "a struct with a built str field held past its region",
    );
    fail_row(
        "memory/region_scalar_list_mut.lu",
        "E1010",
        "a List-holding mut argument inside a region",
    );
    fail_row(
        "memory/region_conflict_params.lu",
        "E1004",
        "one parameter's List-holding data stored into another's region",
    );
    fail_row(
        "memory/region_escape_local.lu",
        "E1010",
        "a List-holding struct built in a region and held outside",
    );
    fail_row(
        "memory/region_freeze_write.lu",
        "E1012",
        "a write into frozen List-holding data",
    );
}

/// #822 (2)–(5) and #800: by design, each with its row.
#[test]
fn the_by_design_rows_answer_as_the_spec_says() {
    fail_row(
        "memory/read_param_field_rebound.lu",
        "E1001",
        "a binding moves out of a read parameter in this frame ([mem.tier0.mode.read])",
    );
    fail_row(
        "grammar/if_brace_next_line.lu",
        "E0201",
        "a header's { on the next line ([gram.lex.newline])",
    );
    fail_row(
        "typecheck/int_not_tilde.lu",
        "E0107",
        "~ is no wolf syntax; ! is the bitwise not ([type.int.not])",
    );
    run_row(
        "lints/shadow_prelude_local.lu",
        "5 7\n",
        &["W0304"],
        Lupin::Same,
        "a local named clock_ms shadows the prelude's ([conf.resolve.ambient])",
    );
    run_row(
        "typecheck/numlit_wrapping_countdown.lu",
        "turns=40 acc=3\nturns=16 acc=13\n",
        &[],
        Lupin::Run("turns=16 acc=13\nturns=16 acc=13\n"),
        "an unannotated counter used as a wrapping shift count is wrapping[u64] \
         ([type.numlit.value])",
    );
}
