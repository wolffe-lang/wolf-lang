//! s237 (wolf-lang#736, `[type.int.shift]`) — a shift on a plain
//! (non-wrapping) integer means one thing on four machines.
//!
//! The rule, written here a second time and independently of the
//! compiler ([`rule`], over `i128`): `x << n` is `x * 2^n` and traps
//! `overflow` when that leaves the type; `x >> n` is the floor of
//! `x / 2^n`; a negative count traps `overflow`. A count at or past the
//! width is no case of its own: every bit has left, so `>>` answers 0
//! (-1 for a negative value) and `<<` answers 0 for 0 and traps for
//! anything else.
//!
//! Every integer type meets every edge of the table the lane measured
//! first (`edges`): a count equal to the width, one past it, 100; a
//! negative count; a set bit shifted out; a positive value shifted
//! into a signed type's top bit; `MIN` and -1 shifted either way; the
//! boundary counts that still fit. Operands reach the operator through
//! a function, so each machine computes at run time. The edges the rule
//! answers are one program per type; each edge it traps is a program of
//! its own. Machines: the checked machine, native and release through
//! `conform-run`, and the sibling lupin.
//!
//! Red at trunk `b407c73b` (wolf 0.2.29's archive measured the same,
//! `~/lanes/s237/evidence/shift-0.2.29-lupin-0.1.52.tsv` on kasumi):
//! the checked machine refused every program by name ("this operator in
//! checked execution"); native and release masked the count to the
//! width and dropped the bits (`255 << 1` at `u8` printed 254, `1 << 8`
//! printed 1, `255 >> 8` printed 255, `8 >> -1` printed 0) and
//! disagreed with lupin 0.1.52 on 155 of the table's 250 cells. lupin
//! 0.1.52 already computed the rule's answer on all 250.
//!
//! One corner lupin 0.1.52 does not answer: a count of 128 or more
//! leaves the 128-bit integer it computes in, and it traps where the
//! rule says 0 or -1 (`255 >> 200` at `u8`). That row is pinned by
//! version, as s180 designed: 0.1.52 exactly is asserted to trap, and
//! any other version is held to the rule.

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
             (LUPIN={}) — the oracle leg of s237's #736 gate did not run",
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

/// A fresh one-program module: `<tmp>/shift_lanes/<name>/main.lu`.
fn program(name: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("shift_lanes")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, src).expect("write the program");
    entry
}

#[derive(Clone, Copy)]
struct Ty {
    name: &'static str,
    bits: u32,
    signed: bool,
}

impl Ty {
    fn lo(self) -> i128 {
        if self.signed {
            -(1i128 << (self.bits - 1))
        } else {
            0
        }
    }

    fn hi(self) -> i128 {
        if self.signed {
            (1i128 << (self.bits - 1)) - 1
        } else {
            (1i128 << self.bits) - 1
        }
    }
}

/// `[type.int.shift]`, independently of the compiler: `Some(v)` is the
/// answer, `None` the `overflow` trap.
fn rule(shl: bool, x: i128, n: i128, t: Ty) -> Option<i128> {
    if n < 0 {
        return None;
    }
    let past = n >= i128::from(t.bits);
    if !shl {
        return Some(match (past, x < 0) {
            (true, true) => -1,
            (true, false) => 0,
            (false, _) => x >> n,
        });
    }
    if x == 0 {
        return Some(0);
    }
    if past {
        return None;
    }
    let v = x << n;
    (t.lo()..=t.hi()).contains(&v).then_some(v)
}

/// The table's edges for one type: `(is <<, value, count)`.
fn edges(t: Ty) -> Vec<(bool, i128, i128)> {
    let w = i128::from(t.bits);
    let (lo, hi) = (t.lo(), t.hi());
    let mut out = vec![
        (true, 1, 3),
        (false, 64, 2),
        (true, 1, w - 1),
        (true, 1, w - 2),
        (true, hi, 1),
        (true, 0, w),
        (true, 1, w),
        (false, hi, w),
        (true, 1, w + 1),
        (false, hi, w + 1),
        (true, 1, 100),
        (false, hi, 100),
        (true, 0, 100),
        (false, hi, w - 1),
        (true, 3, w - 1),
        (true, hi, 0),
        (false, hi, 0),
    ];
    if t.signed {
        out.extend([
            (true, 1, -1),
            (false, 8, -1),
            (true, 0, -1),
            (false, -8, 1),
            (false, -7, 1),
            (false, -1, w - 1),
            (false, lo, w - 1),
            (true, -1, 1),
            (true, -1, w - 1),
            (true, lo, 1),
            (true, -2, w - 1),
            (false, -8, w),
            (false, -8, 100),
            (false, lo, 0),
        ]);
    }
    out
}

fn fns(t: Ty) -> String {
    let n = t.name;
    format!(
        "fn shl(x: {n}, n: {n}) -> {n} {{\n    x << n\n}}\n\n\
         fn shr(x: {n}, n: {n}) -> {n} {{\n    x >> n\n}}\n\n"
    )
}

fn call(shl: bool, x: i128, n: i128) -> String {
    format!("{}({x}, {n})", if shl { "shl" } else { "shr" })
}

/// What each machine said about one program, the compiler's three
/// first. `want` is the verdict and the standard output.
fn every_machine(entry: &Path, what: &str, want: (&str, &str)) {
    let mut who = vec![("checked", lane(entry, "--checked"))];
    who.push(("native", lane(entry, "--native")));
    who.push(("release", lane(entry, "--release")));
    who.push(("lupin", lupin_says(entry)));
    for (name, obs) in who {
        let Some(obs) = obs else { continue };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            want,
            "{name} {} on {what} ([type.int.shift], wolf-lang#736); refusal {:?}",
            obs.version,
            obs.refusal
        );
    }
}

fn the_table_holds_at(t: Ty) {
    let mut body = String::new();
    let mut want = String::new();
    let mut traps = Vec::new();
    for (shl, x, n) in edges(t) {
        match rule(shl, x, n, t) {
            Some(v) => {
                body.push_str(&format!("    print(\"{{{}}}\")\n", call(shl, x, n)));
                want.push_str(&format!("{v}\n"));
            }
            None => traps.push((shl, x, n)),
        }
    }
    assert!(
        !want.is_empty() && !traps.is_empty(),
        "{}: the table has both answers and traps",
        t.name
    );
    let answers = program(
        &format!("{}_answers", t.name),
        &format!("{}fn main() {{\n{body}}}\n", fns(t)),
    );
    every_machine(
        &answers,
        &format!("{}: the edges the rule answers", t.name),
        ("exit(0)", &want),
    );
    for (i, (shl, x, n)) in traps.into_iter().enumerate() {
        let c = call(shl, x, n);
        let entry = program(
            &format!("{}_trap_{i}", t.name),
            &format!(
                "{}fn main() {{\n    let r = {c}\n    print(\"{{r}}\")\n}}\n",
                fns(t)
            ),
        );
        every_machine(
            &entry,
            &format!("{}: {c} leaves the type or has a negative count", t.name),
            ("trap(overflow)", ""),
        );
    }
}

macro_rules! table {
    ($($test:ident: $name:literal, $bits:literal, $signed:literal;)*) => {$(
        #[test]
        fn $test() {
            the_table_holds_at(Ty { name: $name, bits: $bits, signed: $signed });
        }
    )*};
}

table! {
    the_shift_table_holds_at_i8: "i8", 8, true;
    the_shift_table_holds_at_i16: "i16", 16, true;
    the_shift_table_holds_at_i32: "i32", 32, true;
    the_shift_table_holds_at_i64: "i64", 64, true;
    the_shift_table_holds_at_int: "int", 64, true;
    the_shift_table_holds_at_u8: "u8", 8, false;
    the_shift_table_holds_at_u16: "u16", 16, false;
    the_shift_table_holds_at_u32: "u32", 32, false;
    the_shift_table_holds_at_u64: "u64", 64, false;
    the_shift_table_holds_at_uint: "uint", 64, false;
}

/// The reference above is the rule and not a second copy of a bug: its
/// answers at the points the spec names, spelled out by hand.
#[test]
fn the_rule_is_the_clauses() {
    let u8t = Ty {
        name: "u8",
        bits: 8,
        signed: false,
    };
    let i8t = Ty {
        name: "i8",
        bits: 8,
        signed: true,
    };
    let int = Ty {
        name: "int",
        bits: 64,
        signed: true,
    };
    assert_eq!(rule(true, 1, 7, u8t), Some(128));
    assert_eq!(rule(true, 255, 1, u8t), None);
    assert_eq!(rule(true, 1, 8, u8t), None);
    assert_eq!(rule(true, 0, 8, u8t), Some(0));
    assert_eq!(rule(false, 255, 8, u8t), Some(0));
    assert_eq!(rule(false, 255, 7, u8t), Some(1));
    assert_eq!(rule(true, 1, 7, i8t), None);
    assert_eq!(rule(true, -1, 7, i8t), Some(-128));
    assert_eq!(rule(true, -128, 1, i8t), None);
    assert_eq!(rule(false, -7, 1, i8t), Some(-4));
    assert_eq!(rule(false, -8, 8, i8t), Some(-1));
    assert_eq!(rule(false, 8, -1, i8t), None);
    assert_eq!(rule(true, 0, -1, i8t), None);
    assert_eq!(rule(true, 1, 63, int), None);
    assert_eq!(rule(true, 1, 62, int), Some(1 << 62));
    assert_eq!(rule(false, i128::from(i64::MIN), 63, int), Some(-1));
}

/// lupin releases measured to trap on a count of 128 or more where the
/// rule answers (the count leaves its 128-bit integer), by version.
/// Measured on the 0.1.52 archive (sha256 f5194b74…) on kasumi. Any
/// other version is held to the rule.
const LUPIN_TRAPS_PAST_127: &[&str] = &["0.1.52"];

/// A count past 127: the rule has no ceiling (`255 >> 200` at `u8` is
/// 0, `0 << 200` is 0, `-5 >> 130` at `i16` is -1).
#[test]
fn a_count_past_127_is_still_the_value() {
    let entry = program(
        "past_127",
        "fn r8(x: u8, n: u8) -> u8 {\n    x >> n\n}\n\n\
         fn l8(x: u8, n: u8) -> u8 {\n    x << n\n}\n\n\
         fn r16(x: i16, n: i16) -> i16 {\n    x >> n\n}\n\n\
         fn main() {\n    print(\"{r8(255, 200)} {l8(0, 200)} {r16(-5, 130)}\")\n}\n",
    );
    let want = ("exit(0)", "0 0 -1\n");
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            want,
            "{flag} on a count past 127; refusal {:?}",
            obs.refusal
        );
    }
    let Some(obs) = lupin_says(&entry) else {
        return;
    };
    if LUPIN_TRAPS_PAST_127.contains(&obs.version.as_str()) {
        assert_eq!(
            obs.verdict, "trap(overflow)",
            "lupin {} is pinned as trapping on a count past 127: {obs:?}",
            obs.version
        );
        eprintln!(
            "PINNED lupin {}: a count past 127 traps (its 128-bit shift), the rule answers",
            obs.version
        );
    } else {
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            want,
            "lupin {} on a count past 127: only {LUPIN_TRAPS_PAST_127:?} are pinned as \
             trapping; a newer lupin answers the rule",
            obs.version
        );
    }
}
