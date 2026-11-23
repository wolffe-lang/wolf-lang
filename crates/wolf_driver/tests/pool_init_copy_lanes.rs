//! wolf-lang#1183 — a pool's `init` copies a value handed to it plainly,
//! on every machine.
//!
//! `[mem.region.edge.elem]`: the builtin container stores that keep their
//! argument, `push`'s value parameter and `Pool.init`'s, copy a non-`Copy`
//! element passed plainly into the container, and move it under `take`
//! (wolf-lang#385, which made `push` do so). `init` stored the value as it
//! was: natively the pool and the caller held one value, and the checked
//! machine moved it and trapped at the caller's next read.
//!
//! Why a driver test beside the corpus row: `cargo xtask corpus` runs every
//! entry on the native lane only, and this gate asks the checked machine,
//! native, release and lupin, each against the row's header.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The corpus rows.
const ROWS: [&str; 1] = ["pool_init_copies_value"];

const MACHINES: [&str; 3] = ["--checked", "--native", "--release"];

struct Obs {
    verdict: String,
    stdout: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    }
}

/// One machine's observation of `entry`; `None` where the host cannot run
/// a compiled tier (a loud skip, refused under `WOLF_TEST_NATIVE_REQUIRE`).
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) && flag != "--checked" {
        assert!(
            std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
            "required tier {flag} unavailable: {}",
            String::from_utf8_lossy(&out.stderr)
        );
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

/// The sibling lupin, found as the other lane gates find it: `LUPIN`
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

fn lupin(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found"
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN); the machines still answer the header"
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

/// The `stdout="…"` of a row's `run(exit=0, …)` header, unescaped.
fn expected(src: &str) -> String {
    let line = src
        .lines()
        .find(|l| l.starts_with("//! check: run(exit=0, stdout=\""))
        .expect("a run(exit=0, stdout=…) header");
    let body = line
        .trim_start_matches("//! check: run(exit=0, stdout=\"")
        .trim_end_matches("\")");
    body.replace("\\n", "\n").replace("\\\"", "\"")
}

fn corpus(row: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/memory")
        .join(format!("{row}.lu"))
}

#[test]
fn every_row_answers_its_header_on_every_machine() {
    let mut wrong = Vec::new();
    for row in ROWS {
        let entry = corpus(row);
        let src = std::fs::read_to_string(&entry).expect("the row exists");
        let want = expected(&src);
        for m in MACHINES {
            let Some(o) = lane(&entry, m) else {
                continue;
            };
            if o.verdict != "exit(0)" || o.stdout != want {
                wrong.push(format!(
                    "{row} {m}: {} {:?}, the header {:?}",
                    o.verdict, o.stdout, want
                ));
            }
        }
        if let Some(o) = lupin(&entry)
            && (o.verdict != "exit(0)" || o.stdout != want)
        {
            wrong.push(format!(
                "{row} lupin: {} {:?}, the header {:?}",
                o.verdict, o.stdout, want
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
