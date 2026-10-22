//! s258 (o1, wolf-lang#879) — a member read and a method call on a `!T`
//! are E0409 on every wolfgang machine. `[type.row.operand]`: a `!T` is
//! two values, never one, and no operator reads it; "a member read
//! (`r.len`, `r.field`) and a method call (`r.f(…)`) on a `!T` are the
//! same refusal, E0409 at the base, with the same handle-first note".
//!
//! Measured at the control, wolf 0.2.34 (`9da7bdc`, kasumi,
//! `docs/s258-row-member.md`): `rows/negative/row_operand_member.lu`
//! was `unsupported` ("members on this type (the std surface)") and
//! `row_operand_method.lu` `unsupported` ("methods on builtin types
//! (the std surface)") on the checked machine, native and release
//! alike, while lupin 0.1.55 ran both (`n: 1`, `yes`; exit 0), reading
//! the payload through the row. The handled twin
//! `rows/row_member_handled.lu` ran on all four.
//!
//! lupin 0.1.55 (the 0.2.34 pairing, record commit `e642f26`) predates
//! the mirror: its run of the two witnesses is pinned by that version
//! AND commit (s180's design, raw_align_lanes' shape), so a newer lupin
//! that still runs them reds by name, and a development build carrying
//! the mirror (which still reports the last released version, at
//! another commit) is held to the clause. wolf-interp owes the mirror.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every entry on the NATIVE lane only; this
//! holds the checked machine (`conform-run --checked`) and release to
//! the same answer, and the three older `row_operand_*` rows to theirs.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const ISSUE: &str = "wolf-lang#879";

#[derive(Debug)]
struct Obs {
    verdict: String,
    codes: Vec<String>,
    stdout: String,
    version: String,
    commit: String,
    refused: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    let s = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: s("verdict"),
        codes: rec["diagnostics"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|d| d["code"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        stdout: s("stdout_inline"),
        version: s("impl_version"),
        commit: s("commit"),
        refused: s("x-unsupported-construct"),
    }
}

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

/// One wolfgang lane. `None` is the s59 environment skip, named on
/// stderr — never an ICE (lane_exit), never the checked machine.
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

/// lupin's observation, or `None` when this box has no sibling.
/// `WOLF_PAIRING_REQUIRE_SIBLING` is the linux CI job saying "one was
/// arranged here" (r10/#253): there an absent sibling is a failed
/// fetch, never a green skip.
fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of {ISSUE}'s gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still held to the clause here; only the oracle leg is absent"
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

/// The corpus row itself is the program: one truth per file.
fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// A lupin answer measured on a release that predates the mirror:
/// (version, record commit, verdict, stdout).
type Pin = (&'static str, &'static str, &'static str, &'static str);

/// lupin 0.1.55, the 0.2.34 pairing, runs each witness, reading the
/// payload through the row (measured on kasumi, s258): one pin per row.
const MEMBER_PRE_MIRROR: &[Pin] = &[("0.1.55", "e642f26", "exit(0)", "n: 1\n")];
const METHOD_PRE_MIRROR: &[Pin] = &[("0.1.55", "e642f26", "exit(0)", "yes\n")];

/// Every wolfgang lane answers `verdict` with exactly `codes` and
/// `stdout`; lupin answers the same, or what `pins` measured for its
/// version and commit.
fn every_lane_says(rel: &str, verdict: &str, codes: &[&str], stdout: &str, pins: &[Pin]) {
    let entry = corpus(rel);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            (verdict, stdout),
            "the {flag} answer on {rel} ({ISSUE}); refusal {:?}, codes {:?}",
            obs.refused,
            obs.codes
        );
        assert_eq!(obs.codes, codes, "the {flag} diagnostics on {rel}");
    }
    let Some(lupin) = lupin_says(&entry) else {
        return;
    };
    let got = (lupin.verdict.as_str(), lupin.stdout.as_str());
    if got == (verdict, stdout) && lupin.codes == codes {
        return;
    }
    assert!(
        pins.iter().any(|(v, c, pv, ps)| {
            *v == lupin.version && *c == lupin.commit && (*pv, *ps) == got
        }),
        "lupin {} ({})'s answer on {rel}: {got:?} codes {:?}, want {:?} (pre-mirror pins {pins:?})",
        lupin.version,
        lupin.commit,
        lupin.codes,
        (verdict, stdout),
    );
}

/// `rest.len` on a `str ! {none}`: E0409 at the base on every machine,
/// never the std-surface refusal 0.2.34 answered.
#[test]
fn a_member_read_on_a_row_is_e0409_on_every_lane() {
    every_lane_says(
        "rows/negative/row_operand_member.lu",
        "fail(E0409)",
        &["E0409"],
        "",
        MEMBER_PRE_MIRROR,
    );
}

/// `rest.starts_with("4")` on a `str ! {none}`: the same refusal at the
/// receiver, never "methods on builtin types".
#[test]
fn a_method_call_on_a_row_is_e0409_on_every_lane() {
    every_lane_says(
        "rows/negative/row_operand_method.lu",
        "fail(E0409)",
        &["E0409"],
        "",
        METHOD_PRE_MIRROR,
    );
}

/// The handled twin: `(rest else "").len`, a method on what `else`
/// yields, `strip(s)?.len` in a fallible fn — runs on all four machines,
/// lupin included (no pin).
#[test]
fn a_handled_row_reads_its_member_on_every_lane() {
    every_lane_says(
        "rows/row_member_handled.lu",
        "exit(0)",
        &[],
        "n: 1 four: true jobs: 2 none: 0\n",
        &[],
    );
}

/// The operator rows the clause already ruled keep their answer, lupin
/// 0.1.55's included: o1 touched the dot, not the operators.
#[test]
fn the_operator_rows_are_unchanged() {
    for rel in [
        "rows/negative/row_operand_add.lu",
        "rows/negative/row_operand_compare.lu",
        "rows/negative/row_operand_rhs.lu",
    ] {
        every_lane_says(rel, "fail(E0409)", &["E0409"], "", &[]);
    }
}
