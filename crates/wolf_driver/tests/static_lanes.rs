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
//! answer what the clauses say. 0.1.46 (the 0.2.23 pairing, r28) is
//! still pre-mirror (#190 open) and answers the corpus rows as 0.1.45
//! did, measured; its closed attribute set (is70) refuses the hosted
//! `#[section]` program E0817 where 0.1.45 ran it (ruled at r28).

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

/// lupin 0.1.45's measured answer where it parts (rows-*.log); 0.1.46
/// (the 0.2.23 pairing, r28) answers every one of these rows the same,
/// measured.
const fn pin(verdict: &'static str, named: &'static str) -> [Pin<'static>; 2] {
    [
        Pin {
            version: "0.1.45",
            verdict,
            named,
        },
        Pin {
            version: "0.1.46",
            verdict,
            named,
        },
    ]
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

/// `[mem.static.3]` (wolf-lang#585, s210): a module `let`/`const`
/// initialized by a string literal holds the literal's value, the bytes
/// the same literal has inside a fn: the `"""` dedent, the raw fence,
/// code-point escapes and doubled braces. At 0.2.23 (8edac3ee) the
/// comptime engine cooked module initializers with its own decoder and
/// all three compiler machines printed the raw literal text (kasumi
/// `~/lanes/s210/probes/585-trunk.log`); lupin was right.
#[test]
fn a_module_string_literal_holds_its_value() {
    const OUT: &str = "two\n  lines\n\nusage: tool [-n]\n  -n  dry run\n\n\
                       true 12 31\na\\nb\n4 AB 2 {x} 3\n";
    every_machine(
        "memory/static_str_literals.lu",
        runs(OUT),
        Some(runs(OUT)),
        runs(OUT),
        NO_PIN,
    );
}

/// wolf-book ch06's capstone (`e7772338`, `book/ch06.md:808`) opens with
/// a module `let USAGE = """…"""` under a lupin-run fence (wolf-lang#585,
/// s210). Typed, it is #585's shape: 0.2.23 printed the raw literal on
/// all three compiler machines and lupin the dedented text (kasumi
/// `~/lanes/s210/evidence/585-book.log`). Untyped, as the book writes it,
/// the compiler machines still decline it by name, at 0.2.23 and here.
#[test]
fn the_books_usage_text_at_module_level() {
    const USAGE: &str = "\"\"\"\n    usage: wordcount TEXT\n    \
                         Count the words in TEXT and report the most frequent.\n    \"\"\"\n";
    const MAIN: &str = "\nfn main() -> !int {\n    print(USAGE)\n    0\n}\n";
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("static_lanes_book_usage");
    let _ = std::fs::remove_dir_all(&base);
    let typed = base.join("typed");
    let untyped = base.join("untyped");
    for (dir, decl) in [(&typed, "let USAGE: str = "), (&untyped, "let USAGE = ")] {
        std::fs::create_dir_all(dir).expect("mkdir");
        std::fs::write(dir.join("main.lu"), format!("{decl}{USAGE}{MAIN}")).expect("write");
    }
    let text =
        runs("usage: wordcount TEXT\nCount the words in TEXT and report the most frequent.\n\n");
    let typed = typed.join("main.lu");
    if let Some(obs) = lane(&typed, "--checked") {
        assert_obs("the CHECKED lane", "the book's typed USAGE", &obs, text);
    }
    for flag in ["--native", "--release"] {
        if let Some(obs) = lane(&typed, flag) {
            assert_obs(flag, "the book's typed USAGE", &obs, text);
        }
    }
    if let Some(lupin) = lupin_says(&typed) {
        assert_obs("lupin", "the book's typed USAGE", &lupin, text);
    }
    let untyped = untyped.join("main.lu");
    let declined = Want {
        verdict: "unsupported",
        codes: &[],
        stdout: "",
        named: "an item without a declared type",
        ub: ("", ""),
    };
    if let Some(obs) = lane(&untyped, "--checked") {
        assert_obs(
            "the CHECKED lane",
            "the book's untyped USAGE",
            &obs,
            declined,
        );
    }
    for flag in ["--native", "--release"] {
        if let Some(obs) = lane(&untyped, flag) {
            assert_obs(flag, "the book's untyped USAGE", &obs, declined);
        }
    }
    if let Some(lupin) = lupin_says(&untyped) {
        assert_obs("lupin", "the book's untyped USAGE", &lupin, text);
    }
}

/// The same decoder fed s71's fold table (wolf-lang#585, s210): a
/// comptime fn's `"""` result, folded into a run-time call site, printed
/// the raw literal on all three compiler machines in every release since
/// v0.1.0. lupin refuses a `comptime fn` by name (the engine is the
/// compiler's, s16), so the compiler machines answer alone here.
#[test]
fn a_comptime_fold_of_a_multiline_string_is_its_value() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("static_lanes_str_fold");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(
        &entry,
        "comptime fn banner() -> str {\n    \"\"\"\n    hi\n      there\n    \"\"\"\n}\n\
         fn main() -> !int {\n    let s = banner()\n    print(s)\n    0\n}\n",
    )
    .expect("write the fold program");
    let want = runs("hi\n  there\n\n");
    if let Some(obs) = lane(&entry, "--checked") {
        assert_obs("the CHECKED lane", "the str fold", &obs, want);
    }
    for flag in ["--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            assert_obs(flag, "the str fold", &obs, want);
        }
    }
    if let Some(lupin) = lupin_says(&entry) {
        assert_eq!(
            lupin.verdict, "unsupported",
            "lupin {} refuses a comptime fn by name",
            lupin.version
        );
        assert!(
            lupin.unsupported.contains("comptime fn"),
            "lupin {} names the comptime fn: {:?}",
            lupin.version,
            lupin.unsupported
        );
    }
}

/// `[mem.static.3]` (wolf-lang#584, s210): a failed comptime `assert`
/// under a module `const` is ONE fault and one record. 0.2.23 reported
/// it twice: once from the call-site pass, again from kw09's
/// module-state evaluation of the same initializer (kasumi
/// `~/lanes/s210/evidence/584-gate-trunk.log`). The program is
/// wolf-book's ch18 s2. lupin refuses a `comptime fn` by name.
#[test]
fn a_failed_comptime_assert_under_a_module_const_is_one_record() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("static_lanes_assert_once");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(
        &entry,
        "comptime fn shard_mask(shards: int) -> int {\n    var mask = 1\n    \
         while mask < shards {\n        mask = mask * 2\n    }\n    mask - 1\n}\n\n\
         comptime fn expect_mask(shards: int, want: int) -> bool {\n    \
         assert(shard_mask(shards) == want)\n    true\n}\n\n\
         const SIXTEEN_SHARDS: bool = expect_mask(16, 14)\n\n\
         fn main() -> !int {\n    0\n}\n",
    )
    .expect("write the assert program");
    ensure_rt_staticlib();
    for flag in ["--checked", "--native", "--release"] {
        let out = Command::new(wolf())
            .arg("conform-run")
            .arg(&entry)
            .arg(flag)
            .arg("--json")
            .output()
            .expect("wolf runs");
        let rec: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("the record parses");
        let codes: Vec<&str> = rec["diagnostics"]
            .as_array()
            .map(|ds| ds.iter().filter_map(|d| d["code"].as_str()).collect())
            .unwrap_or_default();
        assert_eq!(
            (rec["verdict"].as_str().unwrap_or(""), codes),
            ("fail(E0710)", vec!["E0710"]),
            "{flag}: one fault, one record"
        );
    }
    if let Some(lupin) = lupin_says(&entry) {
        assert_eq!(
            lupin.verdict, "unsupported",
            "lupin {} refuses a comptime fn by name",
            lupin.version
        );
        assert!(lupin.unsupported.contains("comptime fn"));
    }
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

/// `[abi.link.extern]`: a non-pointer type is E0821.
#[test]
fn an_extern_let_of_a_non_pointer_is_e0819() {
    let e = fails("fail(E0821)", &["E0821"]);
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
        } else if lupin.version == "0.1.46" {
            // is70's closed attribute set refuses each `#[section]` by
            // name, E0817 — the same missing mirror, a stricter symptom
            // (measured; ruled at r28).
            assert_eq!(
                (lupin.verdict.as_str(), lupin.codes.join(",").as_str()),
                ("fail(E0817)", "E0817"),
                "lupin 0.1.46 (pre-mirror, {LUPIN_ISSUE}) refuses the placed program E0817"
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
