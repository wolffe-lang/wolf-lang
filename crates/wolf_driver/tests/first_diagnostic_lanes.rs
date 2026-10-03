//! s203 (wolf-lang#377, ruling #28, `[proto.record.first]`) — which
//! code a refusal carries. The diagnostic at the earliest byte offset
//! wins; at the same offset the earlier phase wins (lex, then parse,
//! then resolve). Every wolfgang lane must carry the same code for the
//! same program: the checked, native and release records
//! (`conform-run`), AND the top line of `wolf run` / `wolf run
//! --release`, which is what a person and wl21's verifier read.
//!
//! Through s202 the lanes chose three ways. `conform-run` stopped at
//! lex when lex had any error, wherever it sat, and otherwise took the
//! parser's first error in emission order; `wolf run` sorted lex,
//! parse AND resolve diagnostics by (offset, span end, code). So a
//! program could be E0102 on its record and E0203 on its screen (the
//! 38-candidate class of #377), or E0207 and E0301.
//!
//! lupin answers with its first lex error, else its parser's one error.
//! Where it parts, the gate pins its answer by version (s180's design):
//! a newer lupin that still parts goes red by name. Every part is filed
//! as wolf-interp#175; this lane did not change lupin.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    phase: String,
    stdout: String,
    version: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        phase: rec["phase_reached"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
    }
}

/// One `conform-run` lane. `None` means the host cannot run the native
/// lane (the s59 skip pattern), never a silent pass.
fn record(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) && flag != "--checked" {
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

/// What `wolf run` (or `wolf run --release`) puts on top: the code of
/// the first `error[E…]` on stderr — wl21's verifier reads exactly
/// this — or `exit(N) <stdout>` when the program runs. `None` when the
/// host cannot link (a program that runs needs `cc`).
fn screen(entry: &Path, release: bool) -> Option<String> {
    let mut cmd = Command::new(wolf());
    cmd.arg("run");
    if release {
        cmd.arg("--release");
    }
    let out = cmd.arg(entry).output().expect("wolf runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    if let Some(at) = stderr.find("error[E") {
        let code = &stderr[at + "error[".len()..];
        let end = code.find(']').expect("a closed code bracket");
        return Some(format!("fail({})", &code[..end]));
    }
    if lane_exit::environment_refusal(&out, "wolf run") {
        eprintln!(
            "SKIP: environment cannot link for `wolf run`: {}",
            stderr.trim()
        );
        return None;
    }
    Some(format!(
        "exit({}) {}",
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout)
    ))
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

/// lupin's observation, or `None` when this box has no sibling (never
/// on the linux CI job, which sets `WOLF_PAIRING_REQUIRE_SIBLING`).
fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of #377's gate did not run",
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
    Some(parse_obs(&out.stdout, "lupin's observation"))
}

fn corpus(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/rows")
        .join(name);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// A driver fixture: a witness whose own recovery is no program, kept
/// out of the corpus's program-wide properties.
fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/first_diagnostic")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// lupin releases known to part on a row, with what they answer
/// (wolf-interp#175 mirrors every one).
type PreMirror<'a> = &'a [(&'a str, &'a str)];

/// Every wolfgang lane refuses `entry` with `want` (`fail(E…)`) at
/// `phase`, and `wolf run` / `wolf run --release` put `want`'s code on
/// top. lupin answers `want` too, or — for a version in `pre_mirror`
/// — the measured answer of a known lupin part.
fn every_lane_refuses(entry: &Path, want: &str, phase: &str, pre_mirror: PreMirror<'_>) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = record(entry, flag) else {
            continue;
        };
        assert_eq!(
            (obs.verdict.as_str(), obs.phase.as_str()),
            (want, phase),
            "the {flag} record on {} ([proto.record.first]: earliest offset, then earlier phase)",
            entry.display()
        );
    }
    for release in [false, true] {
        if let Some(top) = screen(entry, release) {
            assert_eq!(
                top,
                want,
                "the top line of `wolf run{}` on {}",
                if release { " --release" } else { "" },
                entry.display()
            );
        }
    }
    if let Some(lupin) = lupin_says(entry) {
        let expect = pre_mirror
            .iter()
            .find(|(v, _)| *v == lupin.version)
            .map(|(_, s)| *s)
            .unwrap_or(want);
        assert_eq!(
            lupin.verdict,
            expect,
            "lupin {}'s verdict on {}",
            lupin.version,
            entry.display()
        );
    }
}

/// #377 class 1 (38 of wl21's 99), its tie: lex E0102 and a parse
/// report start at one byte; the earlier phase is first. (Most of the
/// class is the other shape — stray text before the quote, the parser's
/// E0203 the earlier offset — `parse_error_before_a_lex_error_is_first`.) Red at trunk 12a56b22 on
/// the screen: `wolf run` said E0203 for the top-level shape (wl21's)
/// and E0201 for the row's in-body shape.
#[test]
fn same_offset_lex_before_parse() {
    every_lane_refuses(
        &corpus("negative/first_same_offset_lex.lu"),
        "fail(E0102)",
        "lex",
        &[],
    );
}

/// #377 class 2 (22): a Markdown fence. E0107 everywhere on wolfgang;
/// lupin's E0101 is the wrong code for a stray character
/// (wolf-interp#175).
#[test]
fn stray_backtick_is_e0107() {
    every_lane_refuses(
        &corpus("negative/first_stray_backtick.lu"),
        "fail(E0107)",
        "lex",
        &[("0.1.43", "fail(E0101)"), ("0.1.44", "fail(E0101)")],
    );
}

/// #377 class 3 (10): `let mut f`. One offset, one phase: the ruling
/// does not order the pair; the registry's E0207 names the fact.
#[test]
fn keyword_where_a_pattern_starts_is_e0207() {
    every_lane_refuses(
        &corpus("negative/first_keyword_pattern.lu"),
        "fail(E0207)",
        "parse",
        &[("0.1.43", "fail(E0201)"), ("0.1.44", "fail(E0201)")],
    );
}

/// #377 class 4 (7): the error row's `{` is never closed. E0202 at the
/// `{` is earlier than lupin's E0008 at `var`.
#[test]
fn unclosed_row_brace_is_earlier_than_the_keyword() {
    every_lane_refuses(
        &fixture("row_brace_unclosed.lu"),
        "fail(E0202)",
        "parse",
        &[("0.1.43", "fail(E0008)"), ("0.1.44", "fail(E0008)")],
    );
}

/// kw00's sixth pair: top-level `union`. E0203 on wolfgang; lupin's
/// E0201 at the same span and phase.
#[test]
fn toplevel_union_is_e0203() {
    every_lane_refuses(
        &fixture("union_toplevel.lu"),
        "fail(E0203)",
        "parse",
        &[("0.1.43", "fail(E0201)"), ("0.1.44", "fail(E0201)")],
    );
}

/// A parse error before a lex error. Red at trunk 12a56b22 on all three
/// records: `conform-run` stopped at lex and said E0102.
#[test]
fn parse_error_before_a_lex_error_is_first() {
    every_lane_refuses(
        &corpus("negative/first_parse_before_lex.lu"),
        "fail(E0207)",
        "parse",
        &[("0.1.43", "fail(E0102)"), ("0.1.44", "fail(E0102)")],
    );
}

/// An unclosed `(` one byte before the unterminated string inside it.
/// Red at trunk on all three records (E0102); the screen already said
/// E0202.
#[test]
fn boundary_before_a_lex_error_is_first() {
    every_lane_refuses(
        &corpus("negative/first_boundary_before_lex.lu"),
        "fail(E0202)",
        "parse",
        &[("0.1.43", "fail(E0102)"), ("0.1.44", "fail(E0102)")],
    );
}

/// A resolve error above a parse error: the program does not parse, so
/// it is not resolved. Red at trunk on the screen: `wolf run` said
/// E0301.
#[test]
fn parse_error_stops_before_resolve() {
    every_lane_refuses(
        &corpus("negative/first_parse_before_resolve.lu"),
        "fail(E0207)",
        "parse",
        &[("0.1.43", "fail(E0201)"), ("0.1.44", "fail(E0201)")],
    );
}

/// #377 class 5, the two `exit(0)` against `fail(E0201)`: a list
/// literal lupin 0.1.36 could not parse. Every lane runs it now — no
/// pre-mirror pin: lupin has had list literals since 0.1.37.
#[test]
fn list_literal_program_runs_everywhere() {
    let entry = corpus("first_list_literal_sum.lu");
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = record(&entry, flag) else {
            continue;
        };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", "285\n"),
            "the {flag} record"
        );
    }
    for release in [false, true] {
        if let Some(top) = screen(&entry, release) {
            assert_eq!(top, "exit(0) 285\n", "`wolf run` (release: {release})");
        }
    }
    if let Some(lupin) = lupin_says(&entry) {
        assert_eq!(
            (lupin.verdict.as_str(), lupin.stdout.as_str()),
            ("exit(0)", "285\n"),
            "lupin {}",
            lupin.version
        );
    }
}
