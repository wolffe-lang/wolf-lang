//! kw07 (K3 = B, STATUS #31): `[mem.unsafe.volatile]` on four machines —
//! the checked machine, native, release and lupin — over the corpus rows
//! `corpus/memory/volatile_*.lu`. The compiled tiers' one-access-per-call
//! promise is `volatile_disasm.rs`'s; these rows are the language's
//! answer: what the methods read and write on an allocation, where they
//! need the ring, which pointees they take, and the UB rows they reach.
//!
//! Measured at trunk 8e36bc1a (kasumi, `~/lanes/kw07/evidence/
//! rows-trunk.log`): every row was `unsupported` on the three wolfgang
//! machines ("this raw-pointer operation (the surface is is_null/addr/
//! with_addr/expose/with_exposed)") and on lupin 0.1.45 ("`*T` has no
//! method `write_volatile` in this machine's std subset").
//!
//! lupin 0.1.45 (the 0.2.22 pairing) has no volatile surface; each row's
//! parting is pinned below by that version as pre-mirror (lupin's half is
//! wolf-interp#185). A newer lupin must answer what the clause
//! says. 0.1.46 (the 0.2.23 pairing, r28) is still pre-mirror (#185
//! open) and answers the same, measured; its pins are carried. So is
//! 0.1.47 (the 0.2.24 pairing, r29), measured.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const LUPIN_ISSUE: &str = "wolf-interp#185";

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
             (LUPIN={}) — the oracle leg of kw07's gate did not run",
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
        .join("../../corpus/memory")
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

/// lupin's measured answer where it parts (pre-mirror).
struct Pin<'a> {
    version: &'a str,
    verdict: &'a str,
    named: &'a str,
}

/// lupin 0.1.45 has no volatile method: every row is `unsupported`,
/// by name (measured at trunk and head, `rows-*.log`). 0.1.46 (the
/// 0.2.23 pairing, r28) answers the same, measured; kept with its issue.
const PRE_MIRROR: &[Pin<'static>] = &[
    Pin {
        version: "0.1.45",
        verdict: "unsupported",
        named: "_volatile`",
    },
    Pin {
        version: "0.1.46",
        verdict: "unsupported",
        named: "_volatile`",
    },
    Pin {
        version: "0.1.47",
        verdict: "unsupported",
        named: "_volatile`",
    },
];

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

/// `[mem.unsafe.volatile]`, `.1`, `.3`: every admitted pointee written
/// and read back through one allocation, the widths overlapping
/// little-endian, and the volatile and ordinary spellings mixed.
#[test]
fn every_width_reads_back_what_it_wrote() {
    const OUT: &str = "255 65535 4294967295 9223372036854775807\n\
                       -128 -32768 -2147483648 -9223372036854775808\n\
                       200 4 772\n\
                       17367812 5\n";
    every_machine(
        "volatile_widths.lu",
        runs(OUT),
        Some(runs(OUT)),
        runs(OUT),
        PRE_MIRROR,
    );
}

/// `[mem.unsafe.volatile]`: an access outside the ring is E1301.
#[test]
fn a_volatile_read_needs_the_unsafe_ring() {
    let e = fails("fail(E1301)", &["E1301"]);
    every_machine("volatile_outside_unsafe.lu", e, Some(e), e, PRE_MIRROR);
}

/// `[mem.unsafe.volatile.1]`: `*bool` and `*int` are E1307.
#[test]
fn a_pointee_that_is_not_one_access_is_e1307() {
    let e = fails("fail(E1307)", &["E1307"]);
    for row in ["volatile_pointee_bool.lu", "volatile_pointee_int.lu"] {
        every_machine(row, e, Some(e), e, PRE_MIRROR);
    }
}

/// `[mem.unsafe.volatile.3]`: on an allocation the access is ordinary,
/// so a read after `c.free` is row P1 exactly as `p[0]` is.
#[test]
fn a_volatile_read_after_free_is_row_p1() {
    let p1 = ub("P1", "mem.prov.state");
    every_machine("volatile_ub_uaf.lu", p1, None, p1, PRE_MIRROR);
}

/// `[mem.unsafe.volatile.3]`: a misaligned address is row L3, clause
/// `mem.unsafe.volatile`. The compiled tiers compile and run it (O11:
/// one aligned access, no check — undefined); `with_addr` lowers there
/// since kw06 (F9).
#[test]
fn a_misaligned_volatile_read_is_row_l3() {
    let l3 = ub("L3", "mem.unsafe.volatile");
    // lupin 0.1.45 stops one call earlier, at `with_addr` (its half is
    // wolf-interp#184, kw06's); the mirror needs both issues. 0.1.46
    // carries #184's mirror and reaches the volatile call: still
    // `unsupported`, now naming `read_volatile` (#185; ruled at r28).
    every_machine(
        "volatile_ub_misaligned.lu",
        l3,
        None,
        l3,
        &[
            Pin {
                version: "0.1.45",
                verdict: "unsupported",
                named: "`with_addr`",
            },
            Pin {
                version: "0.1.46",
                verdict: "unsupported",
                named: "`read_volatile`",
            },
            Pin {
                version: "0.1.47",
                verdict: "unsupported",
                named: "`read_volatile`",
            },
        ],
    );
}
