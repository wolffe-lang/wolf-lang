//! wolf-lang#755 (s237) — a construct that branches in the middle of an
//! expression, inside a function with no `if`, loop, call or index, on
//! every machine.
//!
//! Such a function is lowered as one block (the "straight-line"
//! pre-scan, `contains_control` in `lower.rs`), and the builder keeps its
//! locals for that block alone. A construct that ends the block inside
//! an expression left a local declared before it with no definition
//! after it: native and release panicked ("use of a variable with no
//! definition on some path"). The constructs, found by reading the
//! lowering for a branch made inside an expression and each a row of
//! `corpus/typecheck/straight_line_*.lu`:
//!
//! - a narrowing or sign-changing integer `as` (the range check) — at
//!   trunk `b407c73b` and wolf 0.2.29;
//! - `int as char` (the scalar range and the surrogate gap) — at trunk;
//! - a signed `wrapping` `/` or `%` whose divisor may be -1 — at trunk;
//! - `==`, `!=` and the orderings on two `str`s (compared inline) — at
//!   trunk;
//! - a plain `<<`, `>>`, `<<=` or `>>=` (s237's checks) — at s237's own
//!   head `822120f0`, where pax's kernels stopped building.
//!
//! Constructs that branch only in their own operation (a checked `+`, a
//! plain `/`, a float `as int`, unsigned negation) or that the pre-scan
//! already treats as control (`if`, `&&`, `!`, a call, an index) do not
//! split a straight-line body and are not rows. Every row is red at
//! trunk on native and release (the shift rows at s237's head), and the
//! checked machine and lupin 0.1.52 already printed each row's answer.

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
    refusal: String,
}

fn parse_obs(bytes: &[u8], stderr: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_else(|e| {
        panic!(
            "{what} record parses: {e}; stderr {}",
            String::from_utf8_lossy(stderr)
        )
    });
    let refusal = rec["x-unsupported-construct"]
        .as_str()
        .or(rec["x-unsupported"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| String::from_utf8_lossy(stderr).into_owned());
    let s = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: s("verdict"),
        stdout: s("stdout_inline"),
        version: s("impl_version"),
        refusal,
    }
}

/// Build the runtime staticlib once (the compiled lanes link it).
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

/// One wolfgang lane; `None` is the s59 environment skip, named on
/// stderr (never the checked machine, which needs no toolchain).
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
    Some(parse_obs(&out.stdout, &out.stderr, "the observation"))
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
             (LUPIN={}) — the oracle leg of the #755 gate did not run",
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
    assert!(
        !out.stdout.is_empty(),
        "lupin wrote no record for {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, &out.stderr, "lupin's observation"))
}

fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// The row's own claim: `//! check: run(exit=0, stdout="…")`.
fn header_stdout(entry: &Path) -> String {
    let text = std::fs::read_to_string(entry).expect("read the row");
    let line = text
        .lines()
        .find(|l| l.starts_with("//! check: run(exit=0, stdout=\""))
        .unwrap_or_else(|| panic!("{} has no run(exit=0, stdout=…) header", entry.display()));
    let rest = &line["//! check: run(exit=0, stdout=\"".len()..];
    let end = rest.rfind("\")").expect("the header closes");
    rest[..end].replace("\\n", "\n")
}

fn every_machine(row: &str) {
    let entry = corpus(&format!("typecheck/straight_line_{row}.lu"));
    let want = header_stdout(&entry);
    let mut who = vec![("checked", lane(&entry, "--checked"))];
    who.push(("native", lane(&entry, "--native")));
    who.push(("release", lane(&entry, "--release")));
    who.push(("lupin", lupin_says(&entry)));
    for (name, obs) in who {
        let Some(obs) = obs else { continue };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want.as_str()),
            "{name} {} on straight_line_{row} (wolf-lang#755); refusal {:?}",
            obs.version,
            obs.refusal
        );
    }
}

macro_rules! rows {
    ($($test:ident: $row:literal;)*) => {$(
        #[test]
        fn $test() {
            every_machine($row);
        }
    )*};
}

rows! {
    a_narrowing_as_keeps_the_locals_around_it: "as_narrow";
    a_sign_changing_as_keeps_the_locals_around_it: "as_sign";
    an_int_as_char_keeps_the_locals_around_it: "as_char";
    a_signed_wrapping_div_keeps_the_locals_around_it: "wrap_div";
    a_signed_wrapping_rem_keeps_the_locals_around_it: "wrap_rem";
    a_str_equality_keeps_the_locals_around_it: "str_eq";
    a_str_ordering_keeps_the_locals_around_it: "str_order";
    a_plain_shr_by_a_variable_keeps_the_locals_around_it: "shr_var";
    a_plain_shl_by_a_constant_keeps_the_locals_around_it: "shl_const";
    a_plain_shl_assign_keeps_the_locals_around_it: "shl_assign";
}
