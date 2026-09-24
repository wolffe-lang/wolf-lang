//! s178 (wolf-lang#449) — a `mut` argument's claim is exclusive for
//! the call's ARGUMENT EVALUATION and for nothing else, and the
//! checked lane, the native lane and lupin all agree the programs
//! below run.
//!
//! `[mem.tier0.excl.1]` is stated **at every program point**: a claim
//! an `if` arm or a `match` arm made is not live at the join, so a
//! call after the join is the only live access path at its own point.
//! 0.2.16 shipped an extent check that refused these — E1002 on both
//! wolfgang lanes, in the mem tier — while every earlier release and
//! lupin 0.1.38 ran them. boreutils carried workarounds at 12 sites in
//! 7 of 15 utilities and lobo at 16 sites in 4 files rather than stay
//! off the pin.
//!
//! Why this needs a test of its own rather than the corpus alone
//! (s171's lesson, wave-45): `cargo xtask corpus` runs every
//! `phase: run` entry on the NATIVE lane, and `lane-coverage` counts
//! which entries the checked lane EXECUTES, not what it answers. The
//! four corpus files pin the shapes and feed the lupin pairing; THIS
//! file is what fails if one lane regresses alone.
//!
//! And the second half of the file is the part that must never be
//! deleted to make the first half pass. s168's refusals are REAL —
//! `f(mut xs[0], grow(mut xs))` hands the callee a raw address into a
//! buffer the nested call may free, and `take2(mut r.a, wipe(mut r))`
//! loses the callee's write to the writeback. A fix for #449 that
//! widens into those is a silent wrong answer, not an over-refusal, so
//! they are asserted here beside the programs that must run, in one
//! file, on the same lanes. `crates/wolf_driver/tests/
//! mut_element_place.rs` holds s168's full nine cases; these two are
//! the ones #449's fix could plausibly have reached.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    }
}

/// One wolfgang lane. `None` means the host cannot run the native lane
/// (the s59 skip pattern), never a silent pass.
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) && flag == "--native" {
        eprintln!(
            "SKIP: environment cannot run the native lane: {}",
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

/// The sibling lupin, found exactly as `pairing.rs` finds it: `LUPIN`
/// first, then a `wolf-interp` checkout beside an ancestor.
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

/// lupin's observation, or `None` when this box has no sibling.
///
/// `WOLF_PAIRING_REQUIRE_SIBLING` is the linux CI job saying "one was
/// arranged here" (r10/#253): there, an absent sibling is the fetch
/// having failed, and a gate that answers broken plumbing with a green
/// skip is not a gate.
fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of #449's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still compared against the EXPECTED verdict here, so this file \
             keeps its teeth; only the oracle leg is absent"
        );
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

fn program(case: &str, src: &str) -> PathBuf {
    // One program per directory: file boundaries create no scopes
    // (D32), so two `main`s in one directory are E0302.
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, src).expect("write");
    entry
}

/// Every lane runs it, exits 0, and prints `want`.
///
/// All three legs are asserted against the EXPECTED answer, not only
/// against each other: cross-lane equality alone passes when the lanes
/// are wrong together, and at 0.2.16 both wolfgang lanes were wrong
/// together on every program below.
fn every_lane_runs(entry: &Path, want: &str) {
    let checked = lane(entry, "--checked").expect("the checked lane always runs");
    assert_eq!(
        checked.verdict,
        "exit(0)",
        "the CHECKED lane refuses a legal program on {} (#449)",
        entry.display()
    );
    assert_eq!(
        checked.stdout,
        want,
        "checked stdout on {}",
        entry.display()
    );
    if let Some(native) = lane(entry, "--native") {
        assert_eq!(
            native.verdict,
            "exit(0)",
            "the NATIVE lane refuses a legal program on {} (#449)",
            entry.display()
        );
        assert_eq!(native.stdout, want, "native stdout on {}", entry.display());
        assert_eq!(
            checked.stdout,
            native.stdout,
            "the wolfgang lanes disagree on {}",
            entry.display()
        );
    }
    if let Some(lupin) = lupin_says(entry) {
        assert_eq!(
            lupin.verdict,
            "exit(0)",
            "lupin refuses a legal program on {}",
            entry.display()
        );
        assert_eq!(lupin.stdout, want, "lupin stdout on {}", entry.display());
    }
}

/// Both wolfgang lanes REFUSE it with `code`. lupin has no static
/// tier, so it is not consulted: this is the compile-time half of
/// `[conf.trap.map]`, and s168's own file already pins the dynamic
/// side.
fn both_wolfgang_lanes_reject(entry: &Path, code: &str) {
    let want = format!("fail({code})");
    let checked = lane(entry, "--checked").expect("the checked lane always runs");
    assert_eq!(
        checked.verdict,
        want,
        "the CHECKED lane no longer refuses {} — s168's real refusal was weakened",
        entry.display()
    );
    if let Some(native) = lane(entry, "--native") {
        assert_eq!(
            native.verdict,
            want,
            "the NATIVE lane no longer refuses {} — s168's real refusal was weakened",
            entry.display()
        );
    }
}

// ------------------------------------------------ what must RUN ----

/// ws37's witness, narrowed twice, moving lobo's pin to 0.2.16.
/// `corpus/memory/mut_claim_if_else.lu`.
#[test]
fn both_arms_of_an_if_else_may_claim_the_same_place() {
    let entry = program(
        "s449_if_else",
        "\
struct Log {
    lines: List[str],
}

fn note(mut l: Log, m: str) {
    (mut l.lines).push(m)
}

fn main() -> !int {
    var lg = Log { lines: List[str]() }
    if true {
        note(mut lg, \"a\")
    } else {
        note(mut lg, \"b\")
    }
    note(mut lg, \"c\")
    print(\"{lg.lines.len}\")
    0
}
",
    );
    every_lane_runs(&entry, "2\n");
}

/// The control one edit away, which 0.2.16 compiled while refusing its
/// sibling. It is here so a future regression cannot pass this file by
/// breaking both: if the `else` ever starts mattering again, exactly
/// one of these two tests goes red and names the difference.
/// `corpus/memory/mut_claim_if_no_else.lu`.
#[test]
fn the_same_if_without_an_else_still_runs() {
    let entry = program(
        "s449_if_no_else",
        "\
struct Log {
    lines: List[str],
}

fn note(mut l: Log, m: str) {
    (mut l.lines).push(m)
}

fn main() -> !int {
    var lg = Log { lines: List[str]() }
    if true {
        note(mut lg, \"a\")
    }
    note(mut lg, \"c\")
    print(\"{lg.lines.len}\")
    0
}
",
    );
    every_lane_runs(&entry, "2\n");
}

/// ws37's second witness: every arm of a `match` claims the place, and
/// a later call claims it again. 0.2.16 named the FIRST arm, which is
/// what pinned the cause to block-index order.
/// `corpus/memory/mut_claim_match_arms.lu`.
#[test]
fn every_arm_of_a_match_may_claim_the_same_place() {
    let entry = program(
        "s449_match_arms",
        "\
enum Kind { A, B, C }

struct Ev {
    tags: List[str],
}

fn note(mut e: Ev, m: str) {
    (mut e.tags).push(m)
}

fn run(k: Kind) -> int {
    var ev = Ev { tags: List[str]() }
    let code = match k {
        Kind.A => { note(mut ev, \"a\"); 1 },
        Kind.B => { note(mut ev, \"b\"); 2 },
        Kind.C => { note(mut ev, \"c\"); 3 },
    }
    note(mut ev, \"done\")
    code + ev.tags.len
}

fn main() -> !int {
    print(\"{run(Kind.B)}\")
    0
}
",
    );
    every_lane_runs(&entry, "4\n");
}

/// bu07's shape — the exit convention's most ordinary one, and the one
/// that cost boreutils 12 sites: write under a condition, then call
/// the shared writer. `corpus/memory/mut_claim_cond_write.lu`.
#[test]
fn a_conditional_write_then_the_shared_writer_runs() {
    let entry = program(
        "s449_cond_write",
        "\
fn push_one(mut out: List[byte], n: int) {
    (mut out).push(n as byte)
}

fn quoted(s: str) -> int {
    var out = List[byte]()
    if s.len > 0 {
        push_one(mut out, 120)
    } else {
        (mut out).push(64 as byte)
    }
    push_one(mut out, 64)
    out.len
}

fn main() -> !int {
    print(\"{quoted(\"ab\")}\")
    0
}
",
    );
    every_lane_runs(&entry, "2\n");
}

/// A claim inside a loop body, and the correction to this lane's own
/// prediction. I wrote this case expecting the mechanism to predict a
/// third leak; it does not, and the test passed at trunk with the bug
/// in place. `eval_while` mints `head`, then `body`, then `exit`, and
/// the code after the loop lowers in `exit` — the HIGHEST of the
/// three — so the body never outranked the cursor and the bad walk
/// could not reach it. The pin stays because it is the one loop-shaped
/// neighbour of #449 and it is now asserted rather than assumed, but
/// it is a non-regression pin, not a find: it was green before the
/// fix and is green after.
#[test]
fn a_claim_inside_a_loop_body_does_not_outlive_the_loop() {
    let entry = program(
        "s449_loop_body",
        "\
struct Log {
    lines: List[str],
}

fn note(mut l: Log, m: str) {
    (mut l.lines).push(m)
}

fn main() -> !int {
    var lg = Log { lines: List[str]() }
    var i = 0
    while i < 3 {
        note(mut lg, \"x\")
        i += 1
    }
    note(mut lg, \"done\")
    print(\"{lg.lines.len}\")
    0
}
",
    );
    every_lane_runs(&entry, "4\n");
}

// ------------------------- what must STILL be refused (s168) -------

/// s168's element case, unweakened: `grow(mut xs)` is spelled INSIDE
/// the argument list of a call already holding `mut xs[0]`, so it is
/// inside the extent by the rule's own terms. Measured before s168's
/// check existed, this printed `1` natively and `6` checked — a silent
/// wrong answer on both lanes. #449's fix must not reach it.
#[test]
fn a_nested_call_claiming_the_container_is_still_rejected() {
    let entry = program(
        "s449_guard_nested_container",
        "\
fn grow(mut xs: List[int]) -> int {
    var i = 0
    while i < 64 {
        (mut xs).push(i)
        i += 1
    }
    0
}

fn take2(mut a: int, b: int) {
    a = a + b
}

fn main() -> !int {
    var xs = List[int]()
    (mut xs).push(1)
    take2(mut xs[0], grow(mut xs))
    print(\"{xs[0]}\")
    0
}
",
    );
    both_wolfgang_lanes_reject(&entry, "E1002");
}

/// s168's spilled-prefix case, unweakened, and the one the contract
/// names by hand: `take2(mut r.a, wipe(mut r))` stays E1002 on both
/// lanes. Here the loss is quieter than a stale pointer — the callee's
/// write goes to the writeback and vanishes.
#[test]
fn a_nested_call_claiming_a_spilled_prefix_is_still_rejected() {
    let entry = program(
        "s449_guard_nested_prefix",
        "\
struct R {
    a: int,
    b: int,
}

fn wipe(mut r: R) -> int {
    r.a = 0
    r.b = 0
    0
}

fn take2(mut a: int, b: int) {
    a = a + b
}

fn main() -> !int {
    var r = R { a: 5, b: 6 }
    take2(mut r.a, wipe(mut r))
    print(\"{r.a}\")
    0
}
",
    );
    both_wolfgang_lanes_reject(&entry, "E1002");
}

/// The shape the fix's bound could most plausibly have let through: a
/// branch spelled INSIDE the argument list, whose arms claim the place
/// an earlier argument holds. Those blocks are allocated after the
/// mark, so they are genuinely inside the extent and stay refused —
/// this is the test that separates "bound the scan to the argument
/// evaluation" from "stop scanning other blocks".
#[test]
fn a_branch_inside_the_argument_list_claiming_the_place_is_rejected() {
    let entry = program(
        "s449_guard_branch_in_args",
        "\
struct R {
    a: int,
    b: int,
}

fn wipe(mut r: R) -> int {
    r.a = 0
    r.b = 0
    0
}

fn take2(mut a: int, b: int) {
    a = a + b
}

fn main() -> !int {
    var r = R { a: 5, b: 6 }
    take2(mut r.a, if r.b > 0 { wipe(mut r) } else { 0 })
    print(\"{r.a}\")
    0
}
",
    );
    both_wolfgang_lanes_reject(&entry, "E1002");
}
