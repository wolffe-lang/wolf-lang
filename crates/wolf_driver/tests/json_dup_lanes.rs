//! s224 (wolf-lang#124, ruling B22, `[os.json.dup]`) — a repeated json
//! object name is last-wins, one member per name at its first position,
//! on every machine.
//!
//! The seven `corpus/json/dup_*.lu` rows, each held to its own `check:`
//! header on the checked, native and release machines and on lupin.
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every entry on the NATIVE lane only, and
//! the checked machine's kernel (`wolf_mem::json`) is a different copy
//! from the one native and release link (`wolf_rt::json`).
//!
//! Measured at trunk `ac0ac498` and `a0169704` (hasu): all four machines
//! answered every row FIRST-wins and counted every repeat, agreeing with
//! each other and parting from the ruling — so a gate that compared the
//! machines with each other would have been green. This one compares
//! each with the header. lupin 0.1.49 (`f516a5f`, the pairing) is pinned
//! by version and commit to the stdout it was measured giving; any other
//! lupin build owes the header (the wolf-interp s224 branch mirrors the
//! kernel).

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
    version: String,
    commit: String,
}

fn parse(bytes: &[u8]) -> Option<Obs> {
    let rec: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        commit: rec["commit"].as_str().unwrap_or("").to_string(),
    })
}

/// One wolfgang machine's record, or `None` for the s59 environment
/// skip (named on stderr). An ICE or a link error fails (`lane_exit`),
/// and an exit with no record is never a skip (#550).
fn wolfgang(dir: &Path, file: &str, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .current_dir(dir)
        .args(["conform-run", file, flag, "--json"])
        .output()
        .expect("wolf runs");
    if let Some(obs) = parse(&out.stdout) {
        return Some(obs);
    }
    if flag != "--checked" && lane_exit::environment_refusal(&out, &format!("wolf {flag} {file}")) {
        eprintln!(
            "SKIP {file} {flag}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    panic!(
        "conform-run {flag} {file} left no record (exit {:?}): {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The sibling lupin, found as `pairing.rs` finds it.
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

fn require_lupin() -> Option<PathBuf> {
    let l = sibling_lupin();
    if l.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of wolf-lang#124's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    l
}

/// lupin 0.1.49 (the pairing, release commit `f516a5f`) as measured on
/// each row: first-wins, every repeat counted.
const LUPIN_049: (&str, &str) = ("0.1.49", "f516a5f");

fn lupin_049_measured(row: &str) -> &'static str {
    match row {
        "dup_last_wins.lu" => "1 2\n",
        "dup_count_distinct.lu" => "1 3\n",
        "dup_nested.lu" => "first 2 1 2\n",
        "dup_thrice.lu" => "1 2 6\n",
        "dup_kind.lu" => "num str -1\n",
        "dup_render.lu" => "{\"a\":1,\"b\":2,\"a\":3}\n",
        "dup_path.lu" => "missing 1\n",
        other => panic!("{other}: a new dup row needs lupin 0.1.49's measured answer here"),
    }
}

/// A row's `check: run(exit=0, stdout="…")` stdout (a JSON string).
fn want_of(text: &str, path: &Path) -> String {
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix("//! check: "))
        .unwrap_or_else(|| panic!("{} has no check: header", path.display()));
    let lit = line
        .strip_prefix("run(exit=0, stdout=")
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or_else(|| panic!("{}: unread check header {line:?}", path.display()));
    serde_json::from_str(lit).expect("the stdout literal is a JSON string")
}

#[test]
fn repeated_json_names_are_last_wins_on_four_machines() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/json");
    let mut rows: Vec<String> = std::fs::read_dir(&dir)
        .expect("corpus/json")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("dup_") && n.ends_with(".lu"))
        .collect();
    rows.sort();
    assert_eq!(rows.len(), 7, "the seven s224 rows: {rows:?}");
    let lupin = require_lupin();
    let mut bad = Vec::new();
    let mut ran = 0;
    for row in &rows {
        let want = want_of(
            &std::fs::read_to_string(dir.join(row)).expect("row reads"),
            &dir.join(row),
        );
        for flag in ["--checked", "--native", "--release"] {
            if let Some(obs) = wolfgang(&dir, row, flag) {
                ran += 1;
                if obs.verdict != "exit(0)" || obs.stdout != want {
                    bad.push(format!(
                        "{row} {flag}: {} {:?}, want {want:?}",
                        obs.verdict, obs.stdout
                    ));
                }
            }
        }
        if let Some(l) = &lupin {
            let out = Command::new(l)
                .current_dir(&dir)
                .args(["conform-run", row, "--json"])
                .output()
                .expect("lupin runs");
            let obs = parse(&out.stdout).unwrap_or_else(|| {
                panic!(
                    "lupin left no record for {row}: {}",
                    String::from_utf8_lossy(&out.stderr)
                )
            });
            ran += 1;
            let owed = if (obs.version.as_str(), obs.commit.as_str()) == LUPIN_049 {
                lupin_049_measured(row).to_string()
            } else {
                want.clone()
            };
            if obs.verdict != "exit(0)" || obs.stdout != owed {
                bad.push(format!(
                    "{row} lupin {} {}: {} {:?}, want {owed:?}",
                    obs.version, obs.commit, obs.verdict, obs.stdout
                ));
            }
        }
    }
    eprintln!(
        "json_dup_lanes: {ran} program-machine runs, {} disagree",
        bad.len()
    );
    assert!(
        bad.is_empty(),
        "wolf-lang#124 (B22, last-wins):\n{}",
        bad.join("\n")
    );
}
