//! s169 — the outcome surface (`wolf-lang#150`, `#343`, `#32`).
//!
//! The record is what every other tool in this organisation believes:
//! wolf-book's sample runner, wolf-std's rig, lobo's corpus driver and
//! boreutils' `difftest` all read `conform-run`'s verdict as the
//! compiler's answer about a program. Until s169 that verdict could
//! not say **"this program is fine"** — the default lane completed the
//! whole static ladder, lowered clean, and stamped `unsupported`,
//! which is a claim about the PROGRAM (`[proto.record.unsupported]`:
//! a construct outside this implementation's scope).
//!
//! These tests pin the three facts apart:
//!
//! - a clean program is `wir`/`pass` (`[proto.record.pass]`),
//! - a construct the lowering does not cover is `unsupported` with the
//!   construct NAMED (`x-unsupported-construct`), at a rung before
//!   `wir`,
//! - an illegal program is `fail(CODE)`.
//!
//! and they pin `[conf.exit]`'s front-door statuses, which existed as
//! two implementations' habits and no rule.
//!
//! Everything here stops before codegen, so it runs on every host — no
//! `cc`, no `libwolf_rt.a`. The native half lives in `trap_site.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A program with nothing wrong with it.
const FINE: &str = "fn main() -> !int {\n    print(\"hello\")\n    0\n}\n";

/// wolf-lang#343's own witness, verbatim. Filed as "the default lane
/// refuses every slice form", it is in fact a clean lowering — which
/// is the finding, not the fix.
const SLICE: &str =
    "fn main() -> !int {\n    let s = \"espresso\"\n    print(\"{s[..2]}\")\n    0\n}\n";

/// A construct the reference lowering genuinely does not cover:
/// const-generic values do not elaborate (the `corpus/comptime/
/// norm_linear.lu` refusal, reduced).
const REFUSED: &str = "struct Buf[N: type] {\n    len: int,\n}\n\nfn closed(b: Buf[2 + 2]) -> Buf[4] {\n    copy b\n}\n\nfn main() -> !int {\n    0\n}\n";

/// A program the language declines.
const ILLEGAL: &str = "fn main() -> !int {\n    let x: int = \"nope\"\n    0\n}\n";

/// A program that runs and returns an error out of `main` — the case
/// whose process status a static rejection used to be confusable with.
const ERRS: &str = "fn main() -> !int {\n    return NoComma\n}\n";

/// A failing two-argument `assert`: the message is the program's own
/// words for its fault, and `[conf.trap.assert]` evaluates it only on
/// the failing path.
const ASSERTS: &str = "fn main() -> !int {\n    assert(1 == 2, \"one is not two\")\n    0\n}\n";

fn fixture(case: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("s169-{case}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir fixture");
    std::fs::write(dir.join("main.lu"), src).expect("write main");
    dir
}

/// One `conform-run --json` observation: (exit code, record, stderr).
fn observe(dir: &Path, extra: &[&str]) -> (i32, serde_json::Value, String) {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(dir.join("main.lu"))
        .arg("--json")
        .args(extra)
        .output()
        .expect("run wolf conform-run");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let rec: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!("record is not JSON ({e}):\nstdout: {stdout}\nstderr: {stderr}")
    });
    (out.status.code().unwrap_or(-1), rec, stderr)
}

fn field<'a>(r: &'a serde_json::Value, k: &str) -> &'a str {
    r.get(k).and_then(|v| v.as_str()).unwrap_or("<absent>")
}

// ------------------------------------------------- the three facts --

/// The headline. A clean program reaches the default lane's deepest
/// rung and the record says so — `pass`, not `unsupported`.
#[test]
fn a_fine_program_passes_on_the_default_lane() {
    let dir = fixture("fine", FINE);
    let (code, rec, err) = observe(&dir, &[]);
    assert_eq!(code, 0, "[proto.invoke.exit]: a record means exit 0\n{err}");
    assert_eq!(field(&rec, "verdict"), "pass", "record: {rec}");
    assert_eq!(field(&rec, "phase_reached"), "wir", "record: {rec}");
    // A clean stop carries no refusal keys and no diagnostics, and it
    // says nothing on stderr — there is nothing to say.
    assert!(rec.get("x-unsupported-construct").is_none(), "{rec}");
    assert_eq!(
        rec["diagnostics"].as_array().map(Vec::len),
        Some(0),
        "{rec}"
    );
    assert!(err.is_empty(), "a clean program prints nothing:\n{err}");
    // `pass` carries no program outcome ([proto.record.pass]).
    assert!(rec["stdout_sha256"].is_null(), "{rec}");
}

/// wolf-lang#343, answered. The issue's reading was that "the
/// reference lowering covers no slice at all"; measured, the lowering
/// covers it and the lane simply has no engine. The record now tells
/// the reader which of the two it is, and the checked lane — the same
/// program, a lane that DOES execute — proves the program runs.
#[test]
fn the_b11_slice_witness_is_a_clean_lowering_not_a_refusal() {
    let dir = fixture("slice", SLICE);
    let (_, rec, err) = observe(&dir, &[]);
    assert_eq!(field(&rec, "verdict"), "pass", "record: {rec}");
    assert_eq!(field(&rec, "phase_reached"), "wir", "record: {rec}");
    assert!(
        rec.get("x-unsupported-construct").is_none(),
        "no construct was refused — the lowering covers the slice:\n{rec}\n{err}"
    );
    let (_, checked, _) = observe(&dir, &["--checked"]);
    assert_eq!(field(&checked, "verdict"), "exit(0)", "{checked}");
    assert_eq!(field(&checked, "stdout_inline"), "es\n", "{checked}");
}

/// The other side of the distinction, and the one that makes it worth
/// anything: a construct the lowering really does not cover is still
/// `unsupported`, still stops BEFORE `wir`, and still names itself.
#[test]
fn a_refused_construct_is_still_unsupported_and_names_itself() {
    let dir = fixture("refused", REFUSED);
    let (_, rec, err) = observe(&dir, &[]);
    assert_eq!(field(&rec, "verdict"), "unsupported", "record: {rec}");
    assert_ne!(
        field(&rec, "phase_reached"),
        "wir",
        "a refusal reports the last COMPLETED rung ([proto.record.phase]): {rec}"
    );
    let construct = field(&rec, "x-unsupported-construct");
    assert!(
        construct.contains("const-generic"),
        "the record names the construct (s134, #219): {rec}"
    );
    assert!(
        rec.get("x-unsupported-span").is_some(),
        "and where it is: {rec}"
    );
    assert!(
        err.contains("unsupported — "),
        "the rich channel says why too:\n{err}"
    );
}

/// And the third fact, unchanged: the language declining a program.
#[test]
fn an_illegal_program_is_a_rejection_not_a_refusal() {
    let dir = fixture("illegal", ILLEGAL);
    let (code, rec, _) = observe(&dir, &[]);
    assert_eq!(code, 0, "[proto.invoke.exit]");
    assert_eq!(field(&rec, "verdict"), "fail(E0401)", "record: {rec}");
    assert!(rec.get("x-unsupported-construct").is_none(), "{rec}");
}

// ------------------------------------------------- the trap message --

/// `[proto.record.trap]`. The program's own words for its fault reach
/// the record. Before s169 `stdout_inline` — the field that carries a
/// program's OUTPUT — was the only channel a runner had for this, and
/// the checked machine evaluated the message and threw it away.
#[test]
fn a_failing_assert_puts_its_message_in_the_record() {
    let dir = fixture("assert-msg", ASSERTS);
    let (_, rec, _) = observe(&dir, &["--checked"]);
    assert_eq!(field(&rec, "verdict"), "trap(assert)", "record: {rec}");
    assert_eq!(field(&rec, "trap_message"), "one is not two", "{rec}");
    // The message is not the program's output, and never was.
    assert!(rec["stdout_inline"].is_null(), "{rec}");
    // The site, in both spellings: bytes for a machine
    // ([proto.record.ext]), line:col for a person ([conf.trap.render]
    // — one span spelling per tool, and the record is a tool).
    let span = rec["x-trap-span"].as_array().expect("byte span");
    assert_eq!(span.len(), 2, "{rec}");
    let pos = rec["x-trap-pos"].as_array().expect("line:col");
    assert_eq!(pos.len(), 2, "{rec}");
    assert_eq!(pos[0].as_u64(), Some(2), "the assert is on line 2: {rec}");
    assert_eq!(pos[1].as_u64(), Some(5), "at column 5: {rec}");
}

/// A trap with no message says nothing rather than inventing one
/// (`[proto.record.trap]`'s honest-absent).
#[test]
fn a_trap_with_no_message_carries_no_field() {
    let dir = fixture(
        "assert-bare",
        "fn main() -> !int {\n    assert(1 == 2)\n    0\n}\n",
    );
    let (_, rec, _) = observe(&dir, &["--checked"]);
    assert_eq!(field(&rec, "verdict"), "trap(assert)", "record: {rec}");
    assert!(
        rec.get("trap_message").is_none(),
        "no message was written, so none is reported: {rec}"
    );
    assert!(rec.get("x-trap-pos").is_some(), "the site is still known");
}

// ------------------------------------------------------- [conf.exit] --

/// `wolf run FILE`, exit status only.
fn run_status(dir: &Path) -> i32 {
    Command::new(wolf())
        .arg("run")
        .arg(dir.join("main.lu"))
        .output()
        .expect("run wolf run")
        .status
        .code()
        .unwrap_or(-1)
}

fn build_status(dir: &Path) -> (i32, String) {
    let out = Command::new(wolf())
        .arg("build")
        .arg(dir.join("main.lu"))
        .arg("-o")
        .arg(dir.join("out.wir"))
        .arg("--emit=wir")
        .arg("--no-cache")
        .output()
        .expect("run wolf build");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `[conf.exit.static]`. A rejection is 2, and the reason it is not 1
/// is the whole clause: 1 is what a program that RAN and returned an
/// error exits with, and the two were indistinguishable at the process
/// level. wolf-book's runner carried four pin bumps of false-green
/// `run(exit=1)` fences on exactly this confusion (bs46).
#[test]
fn a_static_rejection_exits_two_and_never_collides_with_a_run() {
    let bad = fixture("exit-illegal", ILLEGAL);
    let (code, err) = build_status(&bad);
    assert_eq!(code, 2, "a rejection is 2 ([conf.exit.static]):\n{err}");
    assert!(err.contains("the package does not compile"), "{err}");
    assert_eq!(run_status(&bad), 2, "`run` rules the same as `build`");
}

/// `[conf.exit.refused]`. "Your program is illegal" and "I cannot
/// compile this yet" are different facts with different owners, and
/// wolf spelled both `1`.
#[test]
fn a_refusal_exits_four_and_is_not_a_rejection() {
    let dir = fixture("exit-refused", REFUSED);
    let (code, err) = build_status(&dir);
    assert_eq!(code, 4, "a refusal is 4 ([conf.exit.refused]):\n{err}");
    assert!(err.contains("cannot compile this yet"), "{err}");
}

/// `[conf.exit.collide]`, the guarantee in its useful direction: a
/// rejection and a refusal never take 0 or 1, so the statuses a
/// passing and an error-returning program produce are clear of them.
///
/// The error-return half needs codegen, so it is asserted where the
/// native tier already runs; here the invariant is checked on the two
/// statuses this test file can produce.
#[test]
fn neither_a_rejection_nor_a_refusal_can_be_mistaken_for_a_run() {
    for (case, src) in [("collide-illegal", ILLEGAL), ("collide-refused", REFUSED)] {
        let dir = fixture(case, src);
        let (code, err) = build_status(&dir);
        assert!(
            code != 0 && code != 1,
            "{case}: a program that never ran must not take a status a run takes \
             (got {code}) ([conf.exit.collide]):\n{err}"
        );
    }
    // And the shape the clause exists to keep apart is still reachable:
    // ERRS compiles, so it is not a rejection.
    let errs = fixture("collide-errs", ERRS);
    let (code, err) = build_status(&errs);
    assert_eq!(code, 0, "an error-returning program COMPILES:\n{err}");
}
