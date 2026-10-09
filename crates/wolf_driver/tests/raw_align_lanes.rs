//! s209 (ruling #36 = A, wolf-lang#574): `[mem.unsafe.raw.4]` on four
//! machines — the checked machine, native, release and lupin — over the
//! corpus rows `corpus/memory/raw_ub_misaligned_*.lu`, their near-miss
//! twin `raw_aligned_control.lu` and the packed contrast
//! `packed_field_raw_read.lu`. An ordinary raw access at an address
//! that is not a multiple of its pointee's alignment is UB row L4; the
//! compiled tiers keep the natural alignment (O12, which
//! `raw_align_ir.rs` holds release's IR to) and run it undefined.
//!
//! Measured at trunk 8edac3ee = the v0.2.23 archive (kasumi,
//! `~/lanes/s209/evidence/rows-trunk-archive.log`): every misaligned
//! row was `exit(0)` on the checked machine, native, release and lupin
//! 0.1.46 alike — the checked machine defined the access the compiled
//! tiers assume aligned. A field read through a raw element (`g[0].f`)
//! was `unsupported` on the checked machine ("member access outside
//! the modelled surface").
//!
//! lupin 0.1.46 ran the misaligned rows as defined, each L4 row pinned
//! by that version as pre-mirror. 0.1.47 (the 0.2.24 pairing, r29)
//! carries the mirror (wolf-interp#192): every L4 row is `ub(mem.ub)`,
//! row L4, clause `mem.unsafe.raw.4`, measured. That pin is dropped, and
//! lupin is held to the clause like every other machine. Its record
//! carries no diagnostic on a `ub` verdict (`[proto.record.verdict]`:
//! only a `fail` carries one), so lupin is held to the verdict, the row
//! and the clause, not to the checked machine's E1401. Its packed and
//! repr(c) rows stop earlier, at wolf-interp#188 (no `packed`, no struct
//! pointee); 0.1.48 (the 0.2.25 pairing, r30) admits `packed` (is73) but
//! still read a struct element as bytes (wolf-interp#205). 0.1.49 (the
//! 0.2.26 pairing, r31) carries s213's struct pointee: both rows answer
//! the clause and every pin is dropped.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const LUPIN_ISSUE: &str = "wolf-interp#192";

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
             (LUPIN={}) — the oracle leg of s209's gate did not run",
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

const fn ub<'a>(row: &'a str, clause: &'a str) -> Want<'a> {
    Want {
        verdict: "ub(mem.ub)",
        codes: &["E1401"],
        stdout: "",
        named: "",
        ub: (row, clause),
    }
}

/// lupin's measured answer where it parts (pre-mirror), and the issue
/// that holds its mirror. Keyed by version AND the commit its release
/// archive reports, so a development build carrying the mirror (which
/// still calls itself the last released version) is held to the clause
/// (r31: s213's struct pointee answers both rows at a dev 0.1.48).
struct Pin<'a> {
    version: &'a str,
    commit: &'a str,
    verdict: &'a str,
    named: &'a str,
    issue: &'a str,
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
                "lupin {} (pre-mirror, {}) on {row}",
                lupin.version, pin.issue
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

/// `[mem.unsafe.raw.4]`: a `*u16`, `*u32` or `*u64` read or write one,
/// two or four bytes past an aligned base (through `with_addr`; `q[0]`
/// and `*q`) is row L4, clause `mem.unsafe.raw.4`. The compiled tiers
/// compile and run it (O12: the natural alignment, no check —
/// undefined).
#[test]
fn a_misaligned_ordinary_access_is_row_l4() {
    for row in [
        "raw_ub_misaligned_u16_read.lu",
        "raw_ub_misaligned_u16_write.lu",
        "raw_ub_misaligned_u32_read.lu",
        "raw_ub_misaligned_u32_write.lu",
        "raw_ub_misaligned_u64_read.lu",
        "raw_ub_misaligned_u64_write.lu",
    ] {
        every_machine(row, L4, None, L4_LUPIN, &[]);
    }
}

/// Every pin dropped at the 0.1.49 pairing (r31): s213's struct pointee
/// (wolf-interp#200) answers both rows wolf-interp#205 held, measured.
const NO_PIN: &[Pin<'static>] = &[];

/// `[mem.unsafe.raw.4]`: through a raw element it is the STRUCT's
/// alignment that is asked — a plain `#[repr(c)]` `{u32, u64}` (8)
/// four bytes past an aligned base is L4 at `s[0].b`. lupin 0.1.46 and
/// 0.1.47 have no struct pointee (a `*Pair` reads as bytes,
/// wolf-interp#188; measured on 0.1.47 at r29). 0.1.48 (the 0.2.25
/// pairing, r30) answers the same after is73 closed #188, measured; the
/// pin was re-keyed to wolf-interp#205; dropped at 0.1.49 (r31).
#[test]
fn a_misaligned_repr_c_element_is_row_l4() {
    every_machine(
        "raw_ub_misaligned_repr_c_field.lu",
        L4,
        None,
        // lupin's `ub` record carries no diagnostic, as on the other L4
        // rows (wolf-lang#606); first reached by a lupin with s213's struct
        // pointee (r31).
        L4_LUPIN,
        NO_PIN,
    );
}

/// The near-miss twin: every width at a multiple of its alignment, a
/// byte at an odd offset — defined, the same answer on every machine.
#[test]
fn an_aligned_access_of_every_width_is_defined() {
    const OUT: &str = "aligned 4096 513 77 200 -2\n";
    every_machine(
        "raw_aligned_control.lu",
        runs(OUT),
        Some(runs(OUT)),
        runs(OUT),
        &[],
    );
}

/// `[abi.layout.packed]`: a packed struct's alignment is 1, so its
/// `u64` field at offset 2 (and 12, in the second element) read
/// through `g[i].base` is defined on every machine — never L4. lupin
/// 0.1.46 and 0.1.47 refuse `packed` by name (E0817, wolf-interp#188).
/// 0.1.48 (the 0.2.25 pairing, r30) admits `packed` (is73, #188 closed)
/// and then reads the `*Grid` element as bytes: `unsupported`, "has no
/// member `limit`", measured. Kept with the verdict changed (ruled at
/// r30), keyed to wolf-interp#205; dropped at 0.1.49 (r31).
#[test]
fn a_packed_field_through_a_raw_element_is_defined() {
    const OUT: &str = "packed 255 4096 1 512\n";
    every_machine(
        "packed_field_raw_read.lu",
        runs(OUT),
        Some(runs(OUT)),
        runs(OUT),
        NO_PIN,
    );
}
