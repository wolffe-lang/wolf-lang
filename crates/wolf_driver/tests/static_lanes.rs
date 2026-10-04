//! kw09 (K11 = A and K6, STATUS #31): `[mem.static]` and
//! `[abi.link.extern]`/`[abi.link.section]` on four machines — the
//! checked machine, native, release and lupin — over the corpus rows
//! `corpus/memory/static_*.lu`, `corpus/membrane/extern_let_*.lu` and
//! the hosted placement fixture. What lands in which object section is
//! `static_sections.rs`'s; these rows are the language's answer: module
//! state read and written as ordinary memory, the ring every `var`
//! access needs, the comptime-only initializers, and the refusals by
//! name.
//!
//! Measured at trunk 9bf6a5d5 (kasumi, `~/lanes/kw09/evidence/
//! probes-trunk.log`, rows-trunk.log): every module item was
//! `unsupported` on native and release (`item-initializer lowering
//! (globals)`) and on the checked machine (`module items in checked
//! execution` for a read, `assignment through this place shape` for a
//! write); a `var` written outside `unsafe` passed `mem`; `extern "c"
//! let` was E0201 at parse; `#[section]` was E0817.
//!
//! lupin 0.1.45 (the 0.2.22 pairing) agrees on the rows that run and
//! parts on every refusal; each parting is pinned below by that version
//! as pre-mirror (lupin's half is wolf-interp#190, which also records
//! 0.1.45's stack overflow on an initializer cycle). A newer lupin must
//! answer what the clauses say.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const LUPIN_ISSUE: &str = "wolf-interp#190";

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

fn lupin_version(lupin: &Path) -> String {
    let out = Command::new(lupin)
        .arg("--version")
        .output()
        .expect("lupin runs");
    // "lupin 0.1.45 (wolf-interp, …)"
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string()
}

fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of kw09's gate did not run",
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
    // A machine that dies writes no record: its answer is the crash,
    // named by its stderr, so a pin can say so and a fix shows up.
    if out.stdout.is_empty() {
        return Some(Obs {
            verdict: "crash".to_string(),
            codes: Vec::new(),
            stdout: String::new(),
            version: lupin_version(&lupin),
            unsupported: String::from_utf8_lossy(&out.stderr).into_owned(),
            ub_row: String::new(),
            ub_clause: String::new(),
        });
    }
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
            if matches!(pin.verdict, "unsupported" | "crash") {
                assert!(
                    lupin.unsupported.contains(pin.named),
                    "lupin {} on {row} names {:?}: {:?}",
                    lupin.version,
                    pin.named,
                    lupin.unsupported
                );
            } else {
                assert_eq!(
                    lupin.codes.join(","),
                    pin.named,
                    "lupin {} (pre-mirror, {LUPIN_ISSUE}) on {row}: its diagnostics",
                    lupin.version
                );
            }
        }
        None => assert_obs(
            &format!("lupin {} (the mirror is {LUPIN_ISSUE})", lupin.version),
            row,
            &lupin,
            lupin_want,
        ),
    }
}

/// lupin 0.1.45's measured answer where it parts (rows-*.log).
const fn pin(verdict: &'static str, named: &'static str) -> [Pin<'static>; 1] {
    [Pin {
        version: "0.1.45",
        verdict,
        named,
    }]
}

const NO_PIN: &[Pin<'static>] = &[];

/// `[mem.static.1]`, `[mem.static.3]` (wolf-lang#560): `const` and `let`
/// read in safe code, initializers naming each other in either order.
#[test]
fn const_and_let_are_immutable_data_read_in_safe_code() {
    const OUT: &str = "5 11 4095 true 2.5 kw09\n-7 255 4294967296\n";
    every_machine(
        "memory/static_const_let.lu",
        runs(OUT),
        Some(runs(OUT)),
        runs(OUT),
        NO_PIN,
    );
}

/// `[mem.static.2]`: a `var` is memory every later read sees — across
/// calls, and in a loop whose condition reads what a callee writes.
#[test]
fn a_var_is_written_across_calls_inside_unsafe() {
    const OUT: &str = "4 14 3 1 40 203 false\n";
    every_machine(
        "memory/static_var.lu",
        runs(OUT),
        Some(runs(OUT)),
        runs(OUT),
        NO_PIN,
    );
}

/// `[mem.static.2]` (K11 = A): every access to a `var` outside the ring
/// is E1301, one per site.
#[test]
fn a_var_outside_unsafe_is_e1301_at_every_site() {
    let e = fails("fail(E1301)", &["E1301"]);
    every_machine(
        "memory/static_var_outside_unsafe.lu",
        e,
        Some(e),
        e,
        &pin("exit(1)", ""),
    );
}

/// `[mem.static.3]`: an initializer that reads a `var` is E0705.
#[test]
fn a_run_time_initializer_is_e0705() {
    let e = fails("fail(E0705)", &["E0705"]);
    every_machine(
        "memory/static_init_not_comptime.lu",
        e,
        Some(e),
        e,
        &pin("exit(0)", ""),
    );
}

/// `[mem.static.3]`: two initializers that need each other are each
/// E0705 (lupin 0.1.45 overflows its stack, wolf-interp#190).
#[test]
fn an_initializer_cycle_is_e0705() {
    let e = fails("fail(E0705)", &["E0705"]);
    every_machine(
        "memory/static_init_cycle.lu",
        e,
        Some(e),
        e,
        &pin("crash", "overflowed its stack"),
    );
}

/// `[mem.static.3]`: a `List` is not static data yet — refused by name.
#[test]
fn a_list_in_module_state_is_refused_by_name() {
    let u = Want {
        verdict: "unsupported",
        codes: &[],
        stdout: "",
        named: "module state of a type that is not static data",
        ub: ("", ""),
    };
    every_machine(
        "memory/static_list_let.lu",
        u,
        Some(u),
        u,
        &pin("exit(0)", ""),
    );
}

/// `[abi.link.extern]`: the image's own header read through each host's
/// link-time symbol; the checked machine models no link.
#[test]
fn an_extern_let_is_the_symbols_address() {
    let refused = Want {
        verdict: "unsupported",
        codes: &[],
        stdout: "",
        named: "a link-time symbol",
        ub: ("", ""),
    };
    every_machine(
        "membrane/extern_let_image.lu",
        refused,
        Some(runs("header ok\n")),
        refused,
        &pin("fail(E0201)", "E0201"),
    );
}

/// `[abi.link.extern]`: a non-pointer type is E0819.
#[test]
fn an_extern_let_of_a_non_pointer_is_e0819() {
    let e = fails("fail(E0819)", &["E0819"]);
    every_machine(
        "membrane/extern_let_not_ptr.lu",
        e,
        Some(e),
        e,
        &pin("fail(E0201)", "E0201"),
    );
}

/// `[abi.link.section]`: the checked machine has no image, so a program
/// that places a section is refused by name (the compiled tiers' answer
/// is `static_sections.rs`'s, by host).
#[test]
fn the_checked_machine_refuses_section_placement_by_name() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("static_lanes_hosted_sections");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("hosted_sections.lu");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/freestanding_static/hosted_sections.lu"),
        &entry,
    )
    .expect("copy fixture");
    let obs = lane(&entry, "--checked").expect("the checked machine always runs");
    assert_obs(
        "the CHECKED lane",
        "hosted_sections.lu",
        &obs,
        Want {
            verdict: "unsupported",
            codes: &[],
            stdout: "",
            named: "section placement",
            ub: ("", ""),
        },
    );
    if let Some(lupin) = lupin_says(&entry) {
        if lupin.version == "0.1.45" {
            assert_eq!(
                (lupin.verdict.as_str(), lupin.stdout.as_str()),
                ("exit(0)", "24 100\n"),
                "lupin 0.1.45 (pre-mirror, {LUPIN_ISSUE}) runs the placed program"
            );
        } else {
            assert_eq!(
                lupin.verdict, "unsupported",
                "lupin {} (the mirror is {LUPIN_ISSUE}) refuses section placement",
                lupin.version
            );
            assert!(lupin.unsupported.contains("section placement"));
        }
    }
}
