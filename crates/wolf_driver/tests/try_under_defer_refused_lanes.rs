//! s196 (wolf-lang#498, ruling #19) — a `?` inside a `defer` or
//! `errdefer` expression is refused at compile time, E0611, on every
//! machine (`[type.row.defer]`). A deferred expression runs while the
//! function is already leaving, so a second error has nowhere to go.
//!
//! Before the ruling the machines parted three ways, measured at trunk
//! cdde128a (kasumi, `~/lanes/s196/evidence/red-trunk-cdde128a-rows.log`):
//! native and release OVERFLOWED THE COMPILER'S STACK (`wolf build` and
//! `conform-run` both aborted, rc=134, no record — the `?`'s error edge
//! re-lowered the defer that contains it, without end); the checked
//! machine ran the deferred `?` and dropped its row (`exit(0)`, the
//! function answered 1); lupin 0.1.43 made the row the function's
//! result (`... 9`).
//!
//! Why a driver test beside the corpus rows: `cargo xtask corpus` reads
//! the rows' `fail(E0611)` on the default lane only, and the crash is a
//! property of the native and release lanes. Every case here asserts
//! that each wolfgang lane ANSWERS (a dead compiler is a failed test,
//! never a skip — the s59/#471 pattern reads an abort as neither exit 2
//! nor success) and answers E0611 exactly once; lupin 0.1.43 is pinned
//! by version to its measured bytes, and any newer lupin must refuse
//! with the same code (s180's design; wolf-interp's mirror is is66).

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
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        codes: rec["diagnostics"]
            .as_array()
            .map(|ds| {
                ds.iter()
                    .filter_map(|d| d["code"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
    }
}

/// One wolfgang lane. `None` means the host cannot run the native lane
/// (the s59 skip pattern), never a silent pass — and never an abort:
/// a compiler that dies by signal has no exit code at all, which is
/// the first assertion below.
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    assert!(
        out.status.code().is_some(),
        "conform-run {flag} on {} DIED BY SIGNAL ({}): wolf-lang#498's stack overflow is \
         back — stderr:\n{}",
        entry.display(),
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
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

/// lupin's observation, or `None` when this box has no sibling
/// (`WOLF_PAIRING_REQUIRE_SIBLING` turns an absent sibling into a
/// failure, r10/#253).
fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of #498's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still compared against the EXPECTED answer here; only the oracle \
             leg is absent"
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
fn corpus(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/rows/negative")
        .join(name);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

const CODE: &str = "E0611";

/// Every wolfgang lane answers `fail(E0611)` with exactly one
/// diagnostic. lupin, when present, answers the same — or, for a
/// version named in `lupin_pre_mirror`, the measured bytes of the
/// known pre-mirror behaviour (it RUNS the shape), pinned by version
/// so a newer lupin that still runs it reds by name.
fn every_lane_refuses(entry: &Path, lupin_pre_mirror: &[(&str, &str)]) {
    let want = format!("fail({CODE})");
    let checked = lane(entry, "--checked").expect("the checked lane always runs");
    assert_eq!(
        checked.verdict,
        want,
        "the CHECKED lane on {} (wolf-lang#498: a `?` under a defer is refused, not run and \
         dropped); codes {:?}, stdout {:?}",
        entry.display(),
        checked.codes,
        checked.stdout
    );
    assert_eq!(
        checked.codes,
        vec![CODE.to_string()],
        "the CHECKED lane's diagnostics on {}",
        entry.display()
    );
    for flag in ["--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            continue;
        };
        assert_eq!(
            obs.verdict,
            want,
            "the {flag} lane on {} (wolf-lang#498); codes {:?}",
            entry.display(),
            obs.codes
        );
        assert_eq!(
            obs.codes,
            vec![CODE.to_string()],
            "the {flag} lane's diagnostics on {}",
            entry.display()
        );
    }
    if let Some(lupin) = lupin_says(entry) {
        match lupin_pre_mirror.iter().find(|(v, _)| *v == lupin.version) {
            Some((_, measured)) => {
                assert_eq!(
                    lupin.verdict,
                    "exit(0)",
                    "lupin {} (pre-mirror) verdict on {}",
                    lupin.version,
                    entry.display()
                );
                assert_eq!(
                    lupin.stdout,
                    *measured,
                    "lupin {} (pre-mirror) answer on {}",
                    lupin.version,
                    entry.display()
                );
            }
            None => assert_eq!(
                lupin.verdict,
                want,
                "lupin {}'s answer on {} (the mirror of ruling #19 is wolf-interp's is66); \
                 codes {:?}, stdout {:?}",
                lupin.version,
                entry.display(),
                lupin.codes,
                lupin.stdout
            ),
        }
    }
}

/// `wolf build` on the row: the compiler answers with the diagnostic
/// and the front door's rejection status — exit 2 (`[conf.exit.static]`:
/// a rejection is 2, never 1) with no ICE marker — never a signal. Red
/// at trunk cdde128a with `thread 'main' has overflowed its stack`,
/// rc=134 and no diagnostic.
fn build_refuses(entry: &Path) {
    let out_dir = std::env::temp_dir().join(format!(
        "s196-build-{}-{}",
        std::process::id(),
        entry.file_stem().unwrap().to_string_lossy()
    ));
    let out = Command::new(wolf())
        .arg("build")
        .arg(entry)
        .arg("-o")
        .arg(&out_dir)
        .output()
        .expect("wolf runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.code().is_some(),
        "wolf build on {} DIED BY SIGNAL ({}): wolf-lang#498's stack overflow is back — \
         stderr:\n{stderr}",
        entry.display(),
        out.status
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "wolf build on {} did not exit 2, the front door's rejection status \
         ([conf.exit.static]):\n{stderr}",
        entry.display()
    );
    assert!(
        !stderr.contains(": ICE:"),
        "wolf build on {} hit an internal compiler error, not a diagnostic:\n{stderr}",
        entry.display()
    );
    assert!(
        stderr.contains(&format!("error[{CODE}]")),
        "wolf build on {} refused without error[{CODE}]:\n{stderr}",
        entry.display()
    );
    let _ = std::fs::remove_file(&out_dir);
}

/// The issue's shape: `defer print("deferred {key(ok)?}")`.
#[test]
fn a_try_in_a_defer_is_refused_everywhere() {
    let row = corpus("try_in_defer.lu");
    every_lane_refuses(
        &row,
        &[("0.1.43", "body\nkey\ndeferred a\n1\nbody\nkey\n9\n")],
    );
    build_refuses(&row);
}

/// The issue's errdefer shape: `errdefer print("errdefer {look(m, key(ok)?) else 0}")`.
#[test]
fn a_try_in_an_errdefer_is_refused_everywhere() {
    let row = corpus("try_in_errdefer.lu");
    every_lane_refuses(&row, &[("0.1.43", "key\nlook a\n5\nkey\nkey\n9\n")]);
    build_refuses(&row);
}

/// A `?` in a binding inside a `defer { … }` block: the refusal reads
/// the whole deferred expression.
#[test]
fn a_try_in_a_defer_block_is_refused_everywhere() {
    let row = corpus("try_in_defer_block.lu");
    every_lane_refuses(
        &row,
        &[("0.1.43", "body\nkey\ndeferred a\n1\nbody\nkey\n9\n")],
    );
    build_refuses(&row);
}
