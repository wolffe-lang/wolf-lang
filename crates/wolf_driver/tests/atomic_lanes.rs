//! kw11 (K5 = A, STATUS #31): `[conc.mm.atomic.order]`,
//! `[conc.mm.atomic.raw]` and `[conc.mm.fence]` on four machines — the
//! checked machine, native, release and lupin — over the corpus rows
//! `corpus/conc/atomic_*.lu`. What each operation compiles to is
//! `atomic_disasm.rs`'s; the counts under contention on every cpu set
//! are `atomic_witness.rs`'s. These rows are the language's answer: what
//! each operation reads, writes and yields on an allocation, which
//! orders and pointees it takes, where it needs the ring, and the UB
//! rows it reaches.
//!
//! Measured at trunk a3465f87 (kasumi, `~/lanes/kw11/evidence/
//! rows-trunk-a3465f87.log` 07e83269…): on the three wolfgang machines
//! every row that spells `fence` was fail(E0301) ("nothing named `fence`
//! is in scope") and every other row `unsupported` ("this raw-pointer
//! operation", the resolver deferring `Order` as a candidate tag) — except
//! the racy counter, which has no atomic and ran (105556 natively);
//! lupin 0.1.46 answered `unsupported` ("`Order.seq_cst` does not
//! resolve") and trap(race) on the racy counter.
//!
//! lupin 0.1.46 (the 0.2.23 pairing) has no atomic surface; each row's
//! parting is pinned below by that version as pre-mirror (lupin's half
//! is wolf-interp#194). A newer lupin must answer what the clause says.
//! 0.1.47 (r29) and 0.1.48 (the 0.2.25 pairing, r30; #194 still open)
//! answered every row as 0.1.46 did, measured. 0.1.49 (the 0.2.26
//! pairing, r31) carries is74's mirror: the pins are dropped, and on the
//! UB rows lupin is held to the verdict, row and clause, its `ub` record
//! carrying no diagnostic (wolf-lang#606). It still traps `race` on the
//! plain counter, measured, so that check names 0.1.49 too.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const LUPIN_ISSUE: &str = "wolf-interp#194";

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
             (LUPIN={}) — the oracle leg of kw11's gate did not run",
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
        .join("../../corpus/conc")
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

const fn ub<'a>(row: &'a str, clause: &'a str) -> Want<'a> {
    Want {
        verdict: "ub(mem.ub)",
        codes: &["E1401"],
        stdout: "",
        named: "",
        ub: (row, clause),
    }
}

/// lupin's answer on a UB row: the same verdict, row and clause, and no
/// diagnostic — a `ub` record carries none (wolf-lang#606).
const fn lupin_ub(want: Want<'_>) -> Want<'_> {
    Want { codes: &[], ..want }
}

/// lupin's measured answer where it parts (pre-mirror).
struct Pin<'a> {
    version: &'a str,
    verdict: &'a str,
    named: &'a str,
}

/// lupin 0.1.46 (the 0.2.23 pairing) has no atomic surface: every row
/// is `unsupported`, naming the first spelling it cannot resolve — an
/// `Order` mark or `fence` (measured at kw11's head, `rows-*.log`).
/// 0.1.47 (the 0.2.24 pairing, r29) and 0.1.48 (the 0.2.25 pairing,
/// r30) answered the same, measured row by row. 0.1.49 (the 0.2.26
/// pairing, r31) carries is74's mirror (#194): every pin is dropped.
const PRE_MIRROR: &[Pin<'static>] = &[];

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

/// `[conc.mm.atomic.raw]`, `.1`: every operation on every admitted
/// pointee at `seq_cst`, the read-modify-writes yielding the old value
/// and wrapping at the width.
#[test]
fn every_operation_on_every_width() {
    const OUT: &str = "u8 250 4 240 0 48 33 7 true 9 false 9\n\
                       u16 65530 4 65520 0 48 33 7 true 9 false 9\n\
                       u32 4294967290 4 4294967280 0 48 33 7 true 9 false 9\n\
                       u64 4611686018427387904 4611686018427387914 4611686018427387894 6 54 39 7 \
                       true 9 false 9\n\
                       i8 122 -124 112 0 48 33 7 true 9 false 9\n\
                       i16 32762 -32764 32752 0 48 33 7 true 9 false 9\n\
                       i32 2147483642 -2147483644 2147483632 0 48 33 7 true 9 false 9\n\
                       i64 9223372036854775802 -9223372036854775804 9223372036854775792 0 48 33 7 \
                       true 9 false 9\n";
    every_machine(
        "atomic_widths.lu",
        runs(OUT),
        Some(runs(OUT)),
        runs(OUT),
        PRE_MIRROR,
    );
}

/// `[conc.mm.atomic.raw.2]`, `.5`: every admitted order of every
/// operation, and the nine admitted CAS pairs, answer what `seq_cst`
/// answers on one task.
#[test]
fn every_admitted_order_answers_on_one_task() {
    const OUT: &str = "load 5 5 5\nstore 1 2 3\nswap 3 4 5 6 7\nadd 8 9 10 11 12\n\
                       sub 13 12 11 10 9\nand 8 8 8 8 8\nor 8 9 9 9 9\nxor 9 8 9 8 9\n\
                       cas 8 true 9 true 10 true 11 true 12 true 13 true 14 true 15 true 16 true\n\
                       cas 17 false 17 false\n";
    every_machine(
        "atomic_orders.lu",
        runs(OUT),
        Some(runs(OUT)),
        runs(OUT),
        PRE_MIRROR,
    );
}

/// `[conc.mm.fence]`: the four admitted fences, `seq_cst` outside the
/// ring too.
#[test]
fn every_admitted_fence_runs() {
    every_machine(
        "atomic_fence.lu",
        runs("42 7\n"),
        Some(runs("42 7\n")),
        runs("42 7\n"),
        PRE_MIRROR,
    );
}

/// `[conc.mm.atomic.raw]`, `[conc.mm.fence]`: an atomic operation and a
/// fence weaker than `seq_cst` outside the ring are E1301.
#[test]
fn atomics_and_weak_fences_need_the_unsafe_ring() {
    let e = fails("fail(E1301)", &["E1301"]);
    every_machine("atomic_outside_unsafe.lu", e, Some(e), e, PRE_MIRROR);
}

/// `[conc.mm.atomic.raw.1]`: `*int`, `*uint`, `*byte`, `*bool` are E1308.
#[test]
fn a_pointee_that_is_not_a_fixed_width_integer_is_e1308() {
    let e = fails("fail(E1308)", &["E1308"]);
    every_machine("atomic_pointee.lu", e, Some(e), e, PRE_MIRROR);
}

/// `[conc.mm.atomic.order]`, `.raw.2`, `[conc.mm.fence]`: every order an
/// operation does not admit, an operand that is not a mark, and `Order`
/// as a value are E1309.
#[test]
fn an_order_the_operation_does_not_admit_is_e1309() {
    let e = fails("fail(E1309)", &["E1309"]);
    every_machine("atomic_order_misuse.lu", e, Some(e), e, PRE_MIRROR);
}

/// `[conc.mm.atomic.raw.4]`: a misaligned atomic address is row L4
/// (clause `mem.unsafe.raw.4`, s209's row). The compiled tiers compile
/// and run it (one aligned instruction, no check — undefined).
#[test]
fn a_misaligned_atomic_is_row_l4() {
    let l4 = ub("L4", "mem.unsafe.raw.4");
    every_machine(
        "atomic_ub_misaligned.lu",
        l4,
        None,
        lupin_ub(l4),
        PRE_MIRROR,
    );
}

/// `[conc.mm.atomic.raw.5]`: on an allocation an atomic operation is an
/// access, so a load after `c.free` is row P1 as `p[0]` is.
#[test]
fn an_atomic_load_after_free_is_row_p1() {
    let p1 = ub("P1", "mem.prov.state");
    every_machine("atomic_ub_uaf.lu", p1, None, lupin_ub(p1), PRE_MIRROR);
}

/// `[conc.mm.atomic.raw]` under contention: four tasks' `atomic_add`s
/// and four tasks' CAS loops give exact counts on native and release
/// (every cpu set is `atomic_witness.rs`'s). The checked machine runs no
/// task (C1 deferred) and refuses the program by name.
#[test]
fn the_counter_is_exact_on_the_compiled_tiers() {
    const OUT: &str = "400000 100000\n";
    let refused = Want {
        verdict: "unsupported",
        codes: &[],
        stdout: "",
        named: "structured concurrency",
        ub: ("", ""),
    };
    every_machine(
        "atomic_counter.lu",
        refused,
        Some(runs(OUT)),
        runs(OUT),
        PRE_MIRROR,
    );
}

/// `[conc.mm.race.1]`: the plain-increment twin is a data race. The
/// compiled tiers compile and run it and nothing is asserted about the
/// count; lupin may detect the race (`[conc.mm.race.3]`), and 0.1.46,
/// 0.1.47 and 0.1.48 do (measured at r30; lupin files the row's `pass`
/// as DIV-2026-028, wolf-lang#603).
#[test]
fn the_plain_counter_is_a_race_and_asserts_nothing() {
    let entry = corpus("atomic_race_plain.lu");
    if let Some(obs) = lane(&entry, "--checked") {
        assert_eq!(obs.verdict, "unsupported", "checked on the racy counter");
        assert!(obs.unsupported.contains("structured concurrency"));
    }
    for flag in ["--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        assert!(
            obs.verdict == "exit(0)" && obs.codes.is_empty(),
            "{flag} compiles and runs the racy counter: {} {:?} {:?}",
            obs.verdict,
            obs.codes,
            obs.unsupported
        );
        eprintln!(
            "REPORT (not asserted): {flag} racy count {:?}",
            obs.stdout.trim()
        );
    }
    let Some(lupin) = lupin_says(&entry) else {
        return;
    };
    assert!(
        matches!(lupin.verdict.as_str(), "trap(race)" | "exit(0)"),
        "lupin {} on the racy counter detects the race or runs it: {}",
        lupin.version,
        lupin.verdict
    );
    if matches!(
        lupin.version.as_str(),
        "0.1.46" | "0.1.47" | "0.1.48" | "0.1.49"
    ) {
        assert_eq!(
            lupin.verdict, "trap(race)",
            "lupin {}, measured",
            lupin.version
        );
    }
}
