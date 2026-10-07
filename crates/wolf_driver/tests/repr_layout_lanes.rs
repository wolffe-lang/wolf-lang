//! kw08 (K4, STATUS #31): `[abi.layout.packed]`, `[abi.layout.align]`
//! and `[abi.layout.query]` on four machines — the checked machine,
//! native, release and lupin — over kw08's corpus rows. The C side of
//! every layout (gcc and clang, both directions, both compiling tiers)
//! is `repr_c_raw_layout.rs`'s; these rows are the language's answer on
//! each machine: the queries' numbers, the refusals by name, and the
//! raw-pointee bytes where a machine models them.
//!
//! Measured at trunk 9bf6a5d5 (kasumi, `~/lanes/kw08/evidence/
//! rows-trunk-*.log`): `packed` and `align(N)` were E0817 on the three
//! wolfgang machines, `align_of` and `offset_of` E0301, `size_of` of a
//! struct E0708.
//!
//! lupin 0.1.45 (the 0.2.22 pairing) reads no attribute
//! (wolf-interp#174) and has no layout query: each row's parting is
//! pinned below by that version as pre-mirror (lupin's half is
//! wolf-interp#188). A newer lupin must answer what the clauses say.
//! 0.1.46 (the 0.2.23 pairing, r28) is still pre-mirror (#188 open):
//! its pins are carried, six of the nine rows with the verdict changed
//! to E0817 (its closed attribute set refuses `packed` and `align` by
//! name; ruled at r28). 0.1.47 (the 0.2.24 pairing, r29) answered as
//! 0.1.46 did on all nine. 0.1.48 (the 0.2.25 pairing, r30) carries
//! is73's mirror (#188 closed) and answers all nine as the machines do,
//! measured unpinned; every pin dropped.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const LUPIN_ISSUE: &str = "wolf-interp#188";

#[derive(Debug)]
struct Obs {
    verdict: String,
    codes: Vec<String>,
    stdout: String,
    version: String,
    unsupported: String,
    ub_row: String,
    ub_clause: String,
}

fn parse_obs(bytes: &[u8], stderr: &[u8], what: &str) -> Obs {
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
    codes.dedup();
    let unsupported = rec["x-unsupported-construct"]
        .as_str()
        .or(rec["x-unsupported"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| String::from_utf8_lossy(stderr).into_owned());
    let s = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: s("verdict"),
        codes,
        stdout: s("stdout_inline"),
        version: s("impl_version"),
        unsupported,
        ub_row: s("x-ub-row"),
        ub_clause: s("x-ub-clause"),
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

/// One wolfgang lane; `None` is the s59 environment skip, named on
/// stderr (never an ICE, never the checked machine).
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
             (LUPIN={}) — the oracle leg of kw08's gate did not run",
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
    Some(parse_obs(&out.stdout, &out.stderr, "lupin's observation"))
}

fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// What a machine must answer. `named` is a substring of the refusal's
/// construct when the verdict is `unsupported` (refused BY NAME);
/// `ub` is the (row, clause) pair of a `ub(mem.ub)` verdict.
#[derive(Clone, Copy)]
struct Want<'a> {
    verdict: &'a str,
    codes: &'a [&'a str],
    stdout: &'a str,
    named: &'a str,
    ub: (&'a str, &'a str),
}

const fn runs(stdout: &str) -> Want<'_> {
    Want {
        verdict: "exit(0)",
        codes: &[],
        stdout,
        named: "",
        ub: ("", ""),
    }
}

const fn fails<'a>(verdict: &'a str, codes: &'a [&'a str]) -> Want<'a> {
    Want {
        verdict,
        codes,
        stdout: "",
        named: "",
        ub: ("", ""),
    }
}

/// lupin's measured answer where it parts (pre-mirror).
struct Pin<'a> {
    version: &'a str,
    verdict: &'a str,
    named: &'a str,
}

/// Every row's pin dropped at the 0.1.48 pairing (r30): lupin answers
/// as the machines do (is73, #188 closed). 0.1.45 resolved no query and
/// read no attribute; 0.1.46 and 0.1.47 refused `packed` and `align(N)`
/// E0817 (is70's closed set).
const NO_PIN: &[Pin<'static>] = &[];

const fn refused(named: &str) -> Want<'_> {
    Want {
        verdict: "unsupported",
        codes: &[],
        stdout: "",
        named,
        ub: ("", ""),
    }
}

fn assert_obs(who: &str, row: &str, obs: &Obs, want: Want<'_>) {
    assert_eq!(
        obs.verdict, want.verdict,
        "{who} on {row}; codes {:?}, stdout {:?}, refusal {:?}",
        obs.codes, obs.stdout, obs.unsupported
    );
    let codes: Vec<String> = want.codes.iter().map(|c| c.to_string()).collect();
    assert_eq!(obs.codes, codes, "{who}'s diagnostics on {row}");
    assert_eq!(obs.stdout, want.stdout, "{who}'s stdout on {row}");
    if want.verdict == "unsupported" {
        assert!(
            obs.unsupported.contains(want.named),
            "{who} on {row} refuses by name ({:?}), never a guess: {:?}",
            want.named,
            obs.unsupported
        );
    }
    if want.verdict == "ub(mem.ub)" {
        assert_eq!(
            (obs.ub_row.as_str(), obs.ub_clause.as_str()),
            want.ub,
            "{who}'s UB row and clause on {row}"
        );
    }
}

/// `compiled` is `None` where the compiled tiers' behaviour is undefined
/// by the row itself: they must compile and run it (no refusal, no
/// diagnostic), and what it does is not asserted.
fn every_machine(
    row: &str,
    checked: Want<'_>,
    compiled: Option<Want<'_>>,
    lupin_want: Want<'_>,
    lupin_pre_mirror: &[Pin<'_>],
) {
    let entry = corpus(row);
    if let Some(obs) = lane(&entry, "--checked") {
        assert_obs("the CHECKED lane", row, &obs, checked);
    }
    for flag in ["--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        match compiled {
            Some(want) => assert_obs(flag, row, &obs, want),
            None => assert!(
                obs.verdict != "unsupported" && !obs.verdict.starts_with("fail("),
                "{flag} on {row} must compile and run the UB (its behaviour is the row's, \
                 undefined): {} {:?} {:?}",
                obs.verdict,
                obs.codes,
                obs.unsupported
            ),
        }
    }
    let Some(lupin) = lupin_says(&entry) else {
        return;
    };
    match lupin_pre_mirror.iter().find(|p| p.version == lupin.version) {
        Some(pin) => {
            assert_eq!(
                lupin.verdict, pin.verdict,
                "lupin {} (pre-mirror, {LUPIN_ISSUE}) on {row}",
                lupin.version
            );
            assert!(
                lupin.unsupported.contains(pin.named),
                "lupin {} on {row} refuses by name ({:?}): {:?}",
                lupin.version,
                pin.named,
                lupin.unsupported
            );
        }
        None => assert_obs(
            &format!("lupin {} (the mirror is {LUPIN_ISSUE})", lupin.version),
            row,
            &lupin,
            lupin_want,
        ),
    }
}

/// `[abi.layout.query]`: the three queries answer C's numbers for
/// plain, packed, aligned and nested `#[repr(c)]` structs and scalars,
/// as comptime folds every machine shares.
#[test]
fn the_queries_answer_the_c_layout_on_every_machine() {
    const OUT: &str = "C3 12 4 0 4 8\nP3 6 1 0 1 5\nGdtr 10 1 0 2\nA16 16 16 0\n\
                       Outer 48 16 0 16 32\nPNest 10 2 0 1 8\nPage 4096 4096\n\
                       scalars 1 2 8 8 1 8\n";
    every_machine(
        "comptime/layout_query_repr_c.lu",
        runs(OUT),
        Some(runs(OUT)),
        runs(OUT),
        NO_PIN,
    );
}

/// `[abi.layout.query]`: a native-layout struct has no comptime layout —
/// E0708 for each of the three queries.
#[test]
fn a_native_layout_struct_has_no_comptime_layout() {
    let e = fails("fail(E0708)", &["E0708"]);
    for row in [
        "comptime/size_of_layout.lu",
        "comptime/align_of_layout.lu",
        "comptime/offset_of_layout.lu",
    ] {
        every_machine(row, e, Some(e), e, NO_PIN);
    }
}

/// `[abi.layout.packed]` with `[abi.layout.query]`: a packed struct's
/// fields as scalar raw accesses at `offset_of` — the form every machine
/// models, the checked machine included.
#[test]
fn packed_fields_at_offset_of_run_on_every_machine() {
    const OUT: &str = "10 0 2\n255 0 0 16 0 0 0 0 0 0\n255 4096\n";
    every_machine(
        "memory/packed_fields_at_offset_of.lu",
        runs(OUT),
        Some(runs(OUT)),
        runs(OUT),
        NO_PIN,
    );
}

/// `[abi.layout.packed]`: a packed field is never lent (E0819).
#[test]
fn a_packed_field_is_never_lent() {
    let e = fails("fail(E0819)", &["E0819"]);
    every_machine("memory/packed_field_lend.lu", e, Some(e), e, NO_PIN);
}

/// `[abi.layout.align]`: a representation that cannot be laid out is
/// E0820 — one per struct, four structs.
#[test]
fn an_unlayable_representation_is_e0820() {
    let e = fails("fail(E0820)", &["E0820"]);
    every_machine("grammar/attr_repr_unlayable.lu", e, Some(e), e, NO_PIN);
}

/// `[abi.layout.packed]`, `[abi.layout.align]` through a raw pointer:
/// the compiling tiers store and load at the clause's layout; the
/// checked machine and lupin refuse the whole-aggregate store by name
/// (no byte-level aggregate), never guessing a layout.
#[test]
fn packed_and_aligned_pointees_at_the_clause_layout() {
    const PACKED: &str = "1 2 0 0 0 3 4 5 0 0 0 6\n17 3735928559 34\n\
                          255 0 0 16 0 0 0 0 0 0\n255 1048576\n";
    const ALIGNED: &str = "A16 0:1 16:2\nOuter 0:7 16:9 32:1 33:2 48:1 64:2 80:3\n\
                           PNest 0:5 1:6 2:2 3:1 6:7 8:4 9:3 10:1 11:2 12:3 16:4 18:5\n\
                           1 2 3\n1 2 3 4 5\n";
    for (row, out) in [
        ("memory/raw_repr_packed_layout.lu", PACKED),
        ("memory/raw_repr_align_layout.lu", ALIGNED),
    ] {
        every_machine(
            row,
            refused("raw write of a non-scalar"),
            Some(runs(out)),
            refused("a raw store writes an integer-shaped pointee"),
            NO_PIN,
        );
    }
}
