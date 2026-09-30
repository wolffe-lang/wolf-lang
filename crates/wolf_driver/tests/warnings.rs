//! s67 acceptance — the warning system end-to-end through the `wolf`
//! binary: levels (`--deny-warnings`, `--allow`, per-family), the
//! `#[allow]` attribute, the `lints.*` manifest stub, the additive
//! `warnings` array in conform-run records, and `wolf fix`
//! (dry-run/apply/idempotent).
//!
//! Everything here stops before codegen (`--emit=wir`), so the tests
//! run on every host — no `cc`, no `libwolf_rt.a`.
//!
//! Exit statuses here follow `[conf.exit]` (s169): a static
//! rejection is **2**, not 1 — 1 is what a program that RAN and
//! returned an error out of `main` exits with, and the two were
//! indistinguishable at the process level until the clause ruled.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A body whose dead `match` arm fires the E0802 warning.
const WARNY: &str = "fn main() -> !int {\n    var x = 0\n    match x {\n        0 => { x = 1 }\n        _ => { x = 2 }\n        1 => { x = 3 }\n    }\n    if x > 0 { 0 } else { 1 }\n}\n";
/// The same body with the arm allowed at the item.
const ALLOWED: &str = "#[allow(e0802)]\nfn main() -> !int {\n    var x = 0\n    match x {\n        0 => { x = 1 }\n        _ => { x = 2 }\n        1 => { x = 3 }\n    }\n    if x > 0 { 0 } else { 1 }\n}\n";

fn fixture(case: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir fixture");
    std::fs::write(dir.join("main.lu"), src).expect("write main");
    dir
}

/// `wolf build --emit=wir` with extra flags: (exit code, stderr).
fn build_wir(dir: &Path, extra: &[&str]) -> (i32, String) {
    let out = Command::new(wolf())
        .arg("build")
        .arg(dir.join("main.lu"))
        .arg("-o")
        .arg(dir.join("out.wir"))
        .arg("--emit=wir")
        .arg("--no-cache")
        .args(extra)
        .output()
        .expect("run wolf build");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn warnings_report_but_do_not_fail_the_build() {
    let dir = fixture("warn-default", WARNY);
    let (code, err) = build_wir(&dir, &[]);
    assert_eq!(code, 0, "warnings never fail a default build:\n{err}");
    assert!(err.contains("warning[E0802]"), "warning renders:\n{err}");
}

#[test]
fn deny_warnings_promotes_and_fails() {
    let dir = fixture("warn-deny", WARNY);
    let (code, err) = build_wir(&dir, &["--deny-warnings"]);
    assert_eq!(code, 2, "denied warning fails the build:\n{err}");
    assert!(err.contains("error[E0802]"), "promoted to error:\n{err}");
    assert!(
        err.contains("promoted by the lint configuration"),
        "the note names the rule:\n{err}"
    );
}

#[test]
fn per_code_and_per_family_levels_layer_by_specificity() {
    let dir = fixture("warn-levels", WARNY);
    // allow silences.
    let (code, err) = build_wir(&dir, &["--allow", "E0802"]);
    assert_eq!(code, 0);
    assert!(!err.contains("E0802"), "allowed warning is silent:\n{err}");
    // family allow under --deny-warnings: specificity wins.
    let (code, err) = build_wir(&dir, &["--deny-warnings", "--allow", "E08xx"]);
    assert_eq!(code, 0, "family allow beats blanket deny:\n{err}");
    // unknown code on the flag is a CLI error.
    let (code, _) = build_wir(&dir, &["--deny", "W9999"]);
    assert_eq!(code, 2, "unregistered code on a flag exits 2");
}

#[test]
fn allow_attribute_is_item_granular_and_beats_deny() {
    let dir = fixture("warn-attr", ALLOWED);
    let (code, err) = build_wir(&dir, &["--deny-warnings"]);
    assert_eq!(code, 0, "source allow wins over CLI deny:\n{err}");
    assert!(!err.contains("E0802"), "suppressed entirely:\n{err}");
}

#[test]
fn manifest_lints_apply_and_cli_overrides() {
    let dir = fixture("warn-manifest", WARNY);
    std::fs::write(dir.join("wolf.pkg"), "lints.allow = E0802\n").expect("write manifest");
    let (code, err) = build_wir(&dir, &[]);
    assert_eq!(code, 0);
    assert!(!err.contains("E0802"), "manifest allow silences:\n{err}");
    let (code, err) = build_wir(&dir, &["--warn", "E0802"]);
    assert_eq!(code, 0);
    assert!(
        err.contains("warning[E0802]"),
        "CLI overrides manifest:\n{err}"
    );
    // A malformed manifest is an environment error, never ignored.
    std::fs::write(dir.join("wolf.pkg"), "lints.forbid = E0802\n").expect("rewrite manifest");
    let (code, _) = build_wir(&dir, &[]);
    assert_eq!(code, 2, "malformed lints table refuses loudly");
}

#[test]
fn conform_run_record_carries_the_warnings_array() {
    let dir = fixture("warn-record", WARNY);
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(dir.join("main.lu"))
        .arg("--json")
        .output()
        .expect("run conform-run");
    assert!(out.status.success());
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    let warns = rec["warnings"].as_array().expect("warnings array present");
    assert_eq!(warns.len(), 1);
    assert_eq!(warns[0]["code"], "E0802");
    assert!(warns[0]["span"].as_array().is_some_and(|s| s.len() == 2));

    // The allowed variant records an EMPTY array — the attribute is
    // part of the program, honored by the conformance surface.
    let dir = fixture("warn-record-allowed", ALLOWED);
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(dir.join("main.lu"))
        .arg("--json")
        .output()
        .expect("run conform-run");
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    assert_eq!(rec["warnings"].as_array().map(Vec::len), Some(0));
    assert_eq!(rec["diagnostics"].as_array().map(Vec::len), Some(0));
}

/// s169 (#49, wolf-std F-0046/F-0053). `--deny-warnings` is on
/// `build`, `test` and `doc`, and wolf-std consumes the toolchain
/// through `conform-run` and the record ALONE — so the gate existed
/// and the one repo that wanted it could not reach it.
///
/// The promotion is a REJECTION, and it lands at the rung whose
/// analysis produced the warning: `fail(CODE)` at `typecheck` for a
/// typed-wave lint, not a note bolted onto a `pass` at the end.
#[test]
fn conform_run_honours_deny_warnings_and_rejects_at_the_warning_rung() {
    let dir = fixture("warn-deny-record", WARNY);
    let record = |extra: &[&str]| -> serde_json::Value {
        let out = Command::new(wolf())
            .arg("conform-run")
            .arg(dir.join("main.lu"))
            .arg("--json")
            .args(extra)
            .output()
            .expect("run conform-run");
        assert!(
            out.status.success(),
            "[proto.invoke.exit]: a record means exit 0, stderr:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("observation record parses")
    };

    // Unflagged: a warning, and the program is otherwise fine.
    let plain = record(&[]);
    assert_eq!(plain["verdict"], "pass", "{plain}");
    assert_eq!(
        plain["warnings"].as_array().map(Vec::len),
        Some(1),
        "{plain}"
    );

    // Denied: the same observation is now an error, the verdict is a
    // rejection, and it reports the rung that found it.
    let denied = record(&["--deny-warnings"]);
    assert_eq!(denied["verdict"], "fail(E0802)", "{denied}");
    assert_eq!(denied["phase_reached"], "typecheck", "{denied}");
    assert_eq!(
        denied["diagnostics"][0]["severity"], "error",
        "the promotion is visible in the record, not only in the verdict: {denied}"
    );
    assert_eq!(
        denied["warnings"].as_array().map(Vec::len),
        Some(0),
        "a promoted warning is no longer a warning observation ([proto.record.warn]): {denied}"
    );

    // The source attribute still wins: it is part of the program, and
    // the flag is a consumer's configuration.
    let allowed = fixture("warn-deny-allowed", ALLOWED);
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(allowed.join("main.lu"))
        .arg("--json")
        .arg("--deny-warnings")
        .output()
        .expect("run conform-run");
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("record parses");
    assert_eq!(
        rec["verdict"], "pass",
        "`#[allow]` is the program's own and survives --deny-warnings: {rec}"
    );
}

#[test]
fn fix_applies_machine_applicable_edits_idempotently() {
    let dir = fixture(
        "fix-cycle",
        "use nowhere_needed\n\nfn main() -> !int {\n    let s = \"abc\"\n    let b = s[-1]\n    0\n}\n",
    );
    std::fs::create_dir_all(dir.join("nowhere_needed")).expect("mkdir module");
    std::fs::write(
        dir.join("nowhere_needed/m.lu"),
        "//! member: true\n\npub fn f() -> int {\n    1\n}\n",
    )
    .expect("write module");
    let run_fix = |extra: &[&str]| {
        let out = Command::new(wolf())
            .arg("fix")
            .arg(dir.join("main.lu"))
            .args(extra)
            .output()
            .expect("run wolf fix");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    // Dry-run: reports pending fixes, exits 1, writes nothing.
    let before = std::fs::read_to_string(dir.join("main.lu")).expect("read");
    let (code, out, _) = run_fix(&[]);
    assert_eq!(code, 1, "dry-run signals pending fixes");
    assert!(out.contains("E0305"), "unused-import fix planned:\n{out}");
    assert!(out.contains("E0209"), "negative-index fix planned:\n{out}");
    assert_eq!(
        before,
        std::fs::read_to_string(dir.join("main.lu")).expect("read"),
        "dry-run writes nothing"
    );
    // Apply: both fixes land.
    let (code, _, err) = run_fix(&["--apply"]);
    assert_eq!(code, 0, "apply succeeds:\n{err}");
    let after = std::fs::read_to_string(dir.join("main.lu")).expect("read");
    assert!(!after.contains("use nowhere_needed"), "import deleted");
    assert!(after.contains("s[^1]"), "index rewritten to `^`:\n{after}");
    // Idempotent: nothing left to do.
    let (code, _, err) = run_fix(&[]);
    assert_eq!(code, 0, "second run is clean");
    assert!(err.contains("nothing to fix"), "{err}");
}

/// The idiom arbiter's W1002 fix through the real subcommand: the
/// drop-the-mut edit rewrites the declaration AND the call sites in
/// one machine-applicable suggestion, and the result is warning-clean
/// (idempotence: a second run finds nothing).
#[test]
fn fix_drops_dead_mut_at_declaration_and_call_sites() {
    let dir = fixture(
        "fix-dead-mut",
        "fn offset(mut base: int, delta: int) -> int {\n    base + delta\n}\n\n\
         fn main() -> !int {\n    var b = 1\n    offset(mut b, 2) + offset(mut b, 3) - 7\n}\n",
    );
    let run_fix = |extra: &[&str]| {
        let out = Command::new(wolf())
            .arg("fix")
            .arg(dir.join("main.lu"))
            .args(extra)
            .output()
            .expect("run wolf fix");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    };
    let (code, out) = run_fix(&[]);
    assert_eq!(code, 1, "dry-run signals the pending fix");
    assert!(out.contains("W1002"), "dead-mut fix planned:\n{out}");
    let (code, _) = run_fix(&["--apply"]);
    assert_eq!(code, 0, "apply succeeds");
    let after = std::fs::read_to_string(dir.join("main.lu")).expect("read");
    assert!(
        after.contains("fn offset(base: int"),
        "declaration rewritten:\n{after}"
    );
    assert!(
        after.contains("offset(b, 2) + offset(b, 3)"),
        "both call sites rewritten:\n{after}"
    );
    let (code, _) = run_fix(&[]);
    assert_eq!(code, 0, "nothing left to fix");
}

/// wolf-lang#325 (s154): W1002 stands down where a mode error names
/// the same parameter. The body writes `xs` — it misspelled the write,
/// which is the E0804 — and the lint's flat scan could not see it, so
/// the two diagnostics disagreed about whether a line writes and the
/// lint's machine-applicable fix pointed away from E0804's. The
/// build's stderr carries one diagnostic now, and `wolf fix` offers no
/// edit at all.
#[test]
fn mode_error_retires_the_mut_parameter_lint() {
    let dir = fixture(
        "mut-lint-vs-mode-error",
        "fn take_last[T](mut xs: List[T]) -> T ! {none} {\n    xs.pop()\n}\n\n\
         fn main() -> !int {\n    var xs = List[int]()\n    (mut xs).push(1)\n    \
         take_last(mut xs) else 0\n}\n",
    );
    let (code, err) = build_wir(&dir, &[]);
    assert_eq!(code, 2, "the mode error still stops the build:\n{err}");
    assert!(
        err.contains("error[E0804]"),
        "the mode error renders:\n{err}"
    );
    assert!(
        !err.contains("W1002"),
        "the lint that contradicts it is retired:\n{err}"
    );
    let out = Command::new(wolf())
        .arg("fix")
        .arg(dir.join("main.lu"))
        .output()
        .expect("run wolf fix");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        !text.contains("W1002"),
        "`wolf fix` offers no drop-the-`mut` edit here:\n{text}"
    );
}

/// wolf-lang#464 (s184): a `mut` parameter moved out and never stored
/// back is E1001 at the move (`[mem.tier0.mode.mut]`), and W1002's
/// "never written" beside it was false — the move is the write the
/// lint's flat scan cannot see, and its drop-the-`mut` fix would have
/// walked away from the store back the refusal asks for. The build's
/// stderr carries the refusal alone, and `wolf fix` offers no W1002
/// edit.
#[test]
fn a_mut_parameter_left_moved_out_retires_the_lint() {
    let dir = fixture(
        "mut-lint-vs-moveout",
        "fn f(mut xs: List[int]) {\n    var t = move xs\n    (mut t).push(9)\n}\n\n\
         fn main() -> !int {\n    var xs = [1]\n    f(mut xs)\n    xs.len\n}\n",
    );
    let (code, err) = build_wir(&dir, &[]);
    assert_eq!(code, 2, "the refusal stops the build:\n{err}");
    assert!(err.contains("error[E1001]"), "the refusal renders:\n{err}");
    assert!(
        !err.contains("W1002"),
        "the lint that contradicts it is retired:\n{err}"
    );
    let out = Command::new(wolf())
        .arg("fix")
        .arg(dir.join("main.lu"))
        .output()
        .expect("run wolf fix");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        !text.contains("W1002"),
        "`wolf fix` offers no drop-the-`mut` edit here:\n{text}"
    );
}

/// The control for the case above: moving out of a `copy` leaves the
/// parameter whole and unwritten, so the lint still fires — and
/// nothing is refused.
#[test]
fn a_mut_parameter_copied_from_still_warns() {
    let dir = fixture(
        "mut-lint-copy-control",
        "fn f(mut xs: List[int]) {\n    var t = copy xs\n    (mut t).push(9)\n}\n\n\
         fn main() -> !int {\n    var xs = [1]\n    f(mut xs)\n    xs.len - 1\n}\n",
    );
    let (code, err) = build_wir(&dir, &[]);
    assert_eq!(code, 0, "a warning never fails a default build:\n{err}");
    assert!(err.contains("warning[W1002]"), "the lint fires:\n{err}");
    assert!(!err.contains("E1001"), "nothing is refused:\n{err}");
}

/// The control for the case above: with no mode error in sight the
/// lint is unchanged — a `mut` parameter nothing writes still warns.
#[test]
fn dead_mut_parameter_still_warns_without_a_mode_error() {
    let dir = fixture(
        "mut-lint-control",
        "fn offset(mut base: int, delta: int) -> int {\n    base + delta\n}\n\n\
         fn main() -> !int {\n    var b = 1\n    offset(mut b, 2) - 3\n}\n",
    );
    let (code, err) = build_wir(&dir, &[]);
    assert_eq!(code, 0, "a warning never fails a default build:\n{err}");
    assert!(err.contains("warning[W1002]"), "the lint fires:\n{err}");
}

// ------------------------------------ s185: wolf-lang#469, deny-warnings --
//
// Under `--deny-warnings` W1002 is an error, and every door stopped on it
// at the RESOLVE rung — before the mem rung whose E1001 retires it
// (#464) or the typecheck rung whose E0804 does (#325). The build said
// "never written" and "drop the `mut`", the retired wrong diagnosis,
// exactly where a project denies warnings (bu10 found it in boreutils).
// A promoted W1002 now waits for the mem rung; one nothing retired
// still rejects the program, at `resolve`.

/// #464's shape, the refusal alone.
const MOVED_OUT: &str = "fn f(mut xs: List[int]) {\n    var t = move xs\n    (mut t).push(9)\n}\n\n\
     fn main() -> !int {\n    var xs = [1]\n    f(mut xs)\n    xs.len\n}\n";

/// A `mut` parameter nothing writes: W1002 stands.
const DEAD_MUT: &str = "fn offset(mut base: int, delta: int) -> int {\n    base + delta\n}\n\n\
     fn main() -> !int {\n    var b = 1\n    offset(mut b, 2) - 3\n}\n";

/// `conform-run <main.lu> --json <extra>`'s record.
fn conform_record(dir: &Path, extra: &[&str]) -> serde_json::Value {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(dir.join("main.lu"))
        .arg("--json")
        .args(extra)
        .output()
        .expect("run conform-run");
    assert!(
        out.status.success(),
        "[proto.invoke.exit]: a record means exit 0, stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("observation record parses")
}

/// The codes a record's diagnostics carry, in order.
fn record_codes(rec: &serde_json::Value) -> Vec<String> {
    rec["diagnostics"]
        .as_array()
        .map(|ds| {
            ds.iter()
                .filter_map(|d| d["code"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// `wolf test <main.lu> <extra>`: (exit code, stdout, stderr).
fn wolf_test(dir: &Path, extra: &[&str]) -> (i32, String, String) {
    let out = Command::new(wolf())
        .arg("test")
        .arg(dir.join("main.lu"))
        .args(extra)
        .output()
        .expect("run wolf test");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `wolf build --deny-warnings` on #464's shape: E1001, and no W1002.
#[test]
fn a_mut_parameter_left_moved_out_is_e1001_under_deny_warnings() {
    let dir = fixture("deny-moveout-build", MOVED_OUT);
    let (code, err) = build_wir(&dir, &["--deny-warnings"]);
    assert_eq!(code, 2, "the refusal stops the build:\n{err}");
    assert!(err.contains("error[E1001]"), "the refusal renders:\n{err}");
    assert!(
        !err.contains("W1002"),
        "the retired lint does not come back under --deny-warnings:\n{err}"
    );
    assert!(
        !err.contains("drop the `mut`"),
        "no drop-the-`mut` help beside the store-back refusal:\n{err}"
    );
}

/// `conform-run --deny-warnings` on #464's shape: `fail(E1001)` at
/// `mem`, the rung that found it, and no W1002 in the record.
#[test]
fn conform_run_reports_e1001_not_w1002_under_deny_warnings() {
    let dir = fixture("deny-moveout-record", MOVED_OUT);
    let plain = conform_record(&dir, &[]);
    let denied = conform_record(&dir, &["--deny-warnings"]);
    for rec in [&plain, &denied] {
        assert_eq!(rec["verdict"], "fail(E1001)", "{rec}");
        assert_eq!(rec["phase_reached"], "mem", "{rec}");
        assert_eq!(record_codes(rec), vec!["E1001".to_string()], "{rec}");
    }
}

/// `wolf test --deny-warnings` on #464's shape: rejected on E1001.
#[test]
fn wolf_test_reports_e1001_not_w1002_under_deny_warnings() {
    let dir = fixture("deny-moveout-test", MOVED_OUT);
    let (code, out, err) = wolf_test(&dir, &["--deny-warnings"]);
    assert_eq!(code, 1, "a rejected file fails the run:\n{out}\n{err}");
    assert!(out.contains("REJECTED"), "the file is rejected:\n{out}");
    assert!(err.contains("error[E1001]"), "the refusal renders:\n{err}");
    assert!(!err.contains("W1002"), "no retired lint:\n{err}");
}

/// wolf-lang#325's shape under `--deny-warnings`: E0804 at typecheck
/// retires W1002 there too, on the build and in the record.
#[test]
fn a_mode_error_retires_the_lint_under_deny_warnings() {
    let dir = fixture(
        "deny-mode-error",
        "fn take_last[T](mut xs: List[T]) -> T ! {none} {\n    xs.pop()\n}\n\n\
         fn main() -> !int {\n    var xs = List[int]()\n    (mut xs).push(1)\n    \
         take_last(mut xs) else 0\n}\n",
    );
    let (code, err) = build_wir(&dir, &["--deny-warnings"]);
    assert_eq!(code, 2, "the mode error stops the build:\n{err}");
    assert!(err.contains("error[E0804]"), "the mode error renders:\n{err}");
    assert!(!err.contains("W1002"), "no retired lint:\n{err}");
    let rec = conform_record(&dir, &["--deny-warnings"]);
    assert_eq!(rec["verdict"], "fail(E0804)", "{rec}");
    assert_eq!(rec["phase_reached"], "typecheck", "{rec}");
    assert!(
        !record_codes(&rec).iter().any(|c| c == "W1002"),
        "no retired lint in the record: {rec}"
    );
}

/// The control: a `mut` parameter nothing writes is still refused under
/// `--deny-warnings` on every door, as W1002, and the record still
/// names `resolve`, the rung whose analysis found it.
#[test]
fn a_dead_mut_parameter_still_fails_under_deny_warnings() {
    let dir = fixture("deny-dead-mut", DEAD_MUT);
    let (code, err) = build_wir(&dir, &["--deny-warnings"]);
    assert_eq!(code, 2, "the promoted lint fails the build:\n{err}");
    assert!(err.contains("error[W1002]"), "promoted to error:\n{err}");
    let rec = conform_record(&dir, &["--deny-warnings"]);
    assert_eq!(rec["verdict"], "fail(W1002)", "{rec}");
    assert_eq!(rec["phase_reached"], "resolve", "{rec}");
    let rec = conform_record(&dir, &["--deny-warnings", "--phase=resolve"]);
    assert_eq!(rec["verdict"], "fail(W1002)", "{rec}");
    let (code, out, err) = wolf_test(&dir, &["--deny-warnings"]);
    assert_eq!(code, 1, "rejected:\n{out}\n{err}");
    assert!(out.contains("REJECTED"), "the file is rejected:\n{out}");
    assert!(err.contains("error[W1002]"), "promoted to error:\n{err}");
}

/// The second control: a W1002 nothing retires, beside a later rung's
/// own refusal, is still the verdict at `resolve` — the earliest rung
/// that would have stopped before #469's deferral stops the record now.
#[test]
fn a_standing_w1002_beside_a_later_refusal_keeps_the_resolve_verdict() {
    let dir = fixture(
        "deny-dead-mut-and-moveout",
        "fn offset(mut base: int, delta: int) -> int {\n    base + delta\n}\n\n\
         fn main() -> !int {\n    var b = 1\n    var xs = [[1]]\n    \
         let a = move xs[0]\n    let c = xs\n    offset(mut b, 2) - 3 + a.len - c.len\n}\n",
    );
    let rec = conform_record(&dir, &["--deny-warnings"]);
    assert_eq!(rec["verdict"], "fail(W1002)", "{rec}");
    assert_eq!(rec["phase_reached"], "resolve", "{rec}");
    let (code, err) = build_wir(&dir, &["--deny-warnings"]);
    assert_eq!(code, 2, "refused:\n{err}");
    assert!(err.contains("error[W1002]"), "the lint stands:\n{err}");
}
