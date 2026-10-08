//! s213 (wolf-lang#572, #575, #577, #579): the four gaps PAX worked
//! around, on four machines — the checked machine, native, release and
//! lupin — over the corpus rows each clause names:
//!
//! - `[mem.static.4]` (#579): `memory/static_qualified*/` — a `pub`
//!   module item read (a `var` also written) through its module's name;
//! - `[mem.unsafe.raw.5]` (#577): `memory/raw_field_store*.lu` and
//!   `memory/raw_ub_misaligned_repr_c_field_store.lu` — a store to a
//!   field of a raw element, L4 asked of the element;
//! - `[type.int.not]` (#575, ruling #51 = A): `typecheck/int_not_*.lu` —
//!   `!` on an integer is its complement;
//! - `[type.fn.never]` (#572, ruling #50 = A): `typecheck/fn_never_*.lu` —
//!   a call to a `-> never` fn is bottom.
//!
//! Measured at trunk 294d626d = the v0.2.24 archive (kasumi,
//! `~/lanes/s213/evidence/probes-trunk-archive.log`,
//! `probes2-trunk-archive.log`): every qualified read was `unsupported`
//! on the three compiling machines ("a member access without a recorded
//! type", "module items in checked execution"); every field store was
//! `unsupported` ("assignment through this place shape"), outside
//! `unsafe` too; `!` on an integer was E0409; `-> never` named no type
//! (E0301) and the undeclared form was E0401.
//!
//! lupin 0.1.47 (the 0.2.24 pairing) and 0.1.48 (the 0.2.25 pairing,
//! measured answering alike, r31) run the `const` and `let` reads
//! and the bodyless `extern "c" fn` row, and answer E1301 for the
//! field store outside `unsafe`; they part on every other row, each
//! pinned below by version and release commit as pre-mirror (lupin's
//! half is wolf-interp#200). A newer lupin must answer what the clauses say.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const LUPIN_ISSUE: &str = "wolf-interp#200";

#[derive(Debug)]
struct Obs {
    verdict: String,
    codes: Vec<String>,
    stdout: String,
    version: String,
    commit: String,
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
        commit: s("commit"),
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
             (LUPIN={}) — the oracle leg of s213's gate did not run",
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

const fn ub<'a>(row: &'a str, clause: &'a str) -> Want<'a> {
    Want {
        verdict: "ub(mem.ub)",
        codes: &["E1401"],
        stdout: "",
        named: "",
        ub: (row, clause),
    }
}

/// lupin's measured answer where it parts (pre-mirror), keyed by
/// version AND the commit its release archive reports, so a development
/// build of the mirror (which still calls itself the last released
/// version) is held to the ruled answers.
struct Pin<'a> {
    version: &'a str,
    commit: &'a str,
    verdict: &'a str,
    named: &'a str,
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
    match lupin_pre_mirror
        .iter()
        .find(|p| p.version == lupin.version && lupin.commit.starts_with(p.commit))
    {
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

const L4: Want<'static> = ub("L4", "mem.unsafe.raw.4");

/// lupin's L4: the same verdict, row and clause, and no diagnostic (a
/// `ub` record carries none on lupin; the checked machine adds E1401).
const L4_LUPIN: Want<'static> = Want { codes: &[], ..L4 };

/// A row refused at compile time with `code` on every compiling
/// machine.
const fn fails(verdict: &'static str, codes: &'static [&'static str]) -> Want<'static> {
    Want {
        verdict,
        codes,
        stdout: "",
        named: "",
        ub: ("", ""),
    }
}

const E0401: Want<'static> = fails("fail(E0401)", &["E0401"]);
const E0409: Want<'static> = fails("fail(E0409)", &["E0409"]);
const E1301: Want<'static> = fails("fail(E1301)", &["E1301"]);

/// The same answer on all four machines, with lupin 0.1.47's and
/// 0.1.48's parting (if any) pinned by version and release commit.
fn agree(row: &str, want: Want<'_>, pins: &[Pin<'_>]) {
    every_machine(row, want, Some(want), want, pins);
}

const fn pin<'a>(verdict: &'a str, named: &'a str) -> [Pin<'a>; 2] {
    [
        Pin {
            version: "0.1.47",
            commit: "b3228cb",
            verdict,
            named,
        },
        Pin {
            version: "0.1.48",
            commit: "531bf05",
            verdict,
            named,
        },
    ]
}

const NO_PLACE: &str = "does not denote a place";
const NEEDS_BOOL: &str = "`!` needs a bool";

/// `[mem.static.4]` (#579): `limits.WIDTH` is the `const`'s value and
/// `limits.WIDTH` of a `pub let` its data — lupin already ran both.
#[test]
fn a_pub_const_or_let_reads_through_its_module_name() {
    agree("memory/static_qualified/main.lu", runs("7 7\n"), &[]);
    agree(
        "memory/static_qualified_let/main.lu",
        runs("9 lim 18\n"),
        &[],
    );
}

/// `[mem.static.4]` (#579): a `pub var` read and written as
/// `counter.COUNT` inside `unsafe`; outside it, E1301.
#[test]
fn a_pub_var_through_its_module_name_is_raw_tier() {
    agree(
        "memory/static_qualified_var/main.lu",
        runs("40 43 45\n"),
        &pin("unsupported", "is not a local place"),
    );
    agree(
        "memory/static_qualified_var_outside_unsafe/main.lu",
        E1301,
        &pin("exit(0)", ""),
    );
}

/// `[mem.unsafe.raw.5]` (#577): a store to a field of a raw element —
/// plain, compound, nested and through `(*p)`, and a packed struct's
/// `u64` at offset 2 (defined: the struct's alignment is 1).
#[test]
fn a_field_of_a_raw_element_is_stored_in_place() {
    agree(
        "memory/raw_field_store.lu",
        runs("1 7 0 5\n"),
        &pin("unsupported", NO_PLACE),
    );
    agree(
        "memory/raw_field_store_compound.lu",
        runs("3 42 0 -9\n"),
        &pin("unsupported", NO_PLACE),
    );
    agree(
        "memory/raw_field_store_nested.lu",
        runs("5 0 9 11\n"),
        &pin("unsupported", NO_PLACE),
    );
    agree(
        "memory/raw_field_store_packed.lu",
        runs("65535 4096 1 512\n"),
        // 0.1.47 refused `packed` itself (E0817); 0.1.48 admits it (is73)
        // and declines the field store as it does the unpacked rows (r31,
        // measured).
        &[
            Pin {
                version: "0.1.47",
                commit: "b3228cb",
                verdict: "fail(E0817)",
                named: "",
            },
            Pin {
                version: "0.1.48",
                commit: "531bf05",
                verdict: "unsupported",
                named: NO_PLACE,
            },
        ],
    );
}

/// `[mem.unsafe.raw.5]` (#577): the store is a raw write — E1301
/// outside `unsafe` (lupin already said so).
#[test]
fn a_raw_field_store_outside_unsafe_is_e1301() {
    agree("memory/raw_field_store_outside_unsafe.lu", E1301, &[]);
}

/// `[mem.unsafe.raw.4]`/`.5` (#577, s209's question): the store asks
/// row L4 of the ELEMENT — a plain `#[repr(c)]` `{u32, u64}` 4 past an
/// 8-aligned address is L4 at `s[0].a`; the compiled tiers run it
/// undefined.
#[test]
fn a_misaligned_repr_c_element_store_is_row_l4() {
    every_machine(
        "memory/raw_ub_misaligned_repr_c_field_store.lu",
        L4,
        None,
        L4_LUPIN,
        &pin("unsupported", NO_PLACE),
    );
}

/// `[type.int.not]` (#575, ruling #51 = A): `!` on an integer is its
/// complement at the operand's width; a byte widens to `int`; a float
/// stays E0409.
#[test]
fn bang_on_an_integer_is_its_complement() {
    agree(
        "typecheck/int_not_mask.lu",
        runs("73728 77824 4294892730 4294967295 4096 57344\n"),
        &pin("unsupported", NEEDS_BOOL),
    );
    agree(
        "typecheck/int_not_signed.lu",
        runs("-6 127 0 -256 -1 -128\n"),
        &pin("unsupported", NEEDS_BOOL),
    );
    agree(
        "typecheck/int_not_byte.lu",
        runs("-16 240 255\n"),
        &pin("unsupported", NEEDS_BOOL),
    );
    agree(
        "typecheck/int_not_float.lu",
        E0409,
        &pin("unsupported", NEEDS_BOOL),
    );
}

/// `[type.fn.never]` (#572, ruling #50 = A): a call to a `-> never` fn is
/// bottom — the issue's witness runs, its trap twin stops at the
/// callee's `assert(false)`, a bodyless `extern "c" fn` fits the
/// handler; a reachable end or a `return` is E0401.
#[test]
fn a_call_to_a_never_fn_is_bottom() {
    agree(
        "typecheck/fn_never_handler_arm.lu",
        runs("1\n"),
        &pin("fail(E0301)", ""),
    );
    agree(
        "typecheck/fn_never_trap.lu",
        Want {
            verdict: "trap(assert)",
            codes: &[],
            stdout: "bad\n",
            named: "",
            ub: ("", ""),
        },
        &pin("fail(E0301)", ""),
    );
    agree("typecheck/fn_never_extern.lu", runs("2\n"), &[]);
    agree(
        "typecheck/fn_never_reaches_end.lu",
        E0401,
        &pin("fail(E0301)", ""),
    );
    agree(
        "typecheck/fn_never_return.lu",
        E0401,
        &pin("fail(E0301)", ""),
    );
}
