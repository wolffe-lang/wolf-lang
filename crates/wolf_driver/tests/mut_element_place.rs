//! s168 — lending a container ELEMENT, on every lane.
//!
//! Why this needs a test of its own rather than corpus files: the
//! corpus gate (`cargo xtask corpus`) executes every `phase: run`
//! entry on the NATIVE lane, and `lane-coverage` counts which entries
//! the checked lane executes, not what it ANSWERS. So a lend that
//! reads the right cell natively and the wrong one in checked
//! execution would sail through both gates. That is the s171 lesson,
//! bought at the cost of a silent wrong answer.
//!
//! Every case asserts the two lanes agree AND that they agree on the
//! right answer — cross-lane equality alone passes when both lanes
//! are wrong together.
//!
//! The last two cases are the exclusivity story, and they are the
//! reason the rest is sound: an element lend hands the callee a raw
//! address into the container's buffer, so anything that could reach
//! the container while the callee holds it has to be rejected before
//! it runs. Both were seen red against the shipping fix removed.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

struct Obs {
    verdict: String,
    stdout: String,
}

/// Run one entry file on one lane. `None` means the host cannot run
/// the native lane (the s59 skip pattern), never a silent pass.
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
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    })
}

fn case(name: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, src).expect("write fixture");
    entry
}

/// Both lanes run it, and both print `want`.
fn both_lanes_say(entry: &Path, want: &str) {
    let checked = lane(entry, "--checked").expect("the checked lane always runs");
    assert_eq!(
        checked.verdict,
        "exit(0)",
        "checked verdict on {}",
        entry.display()
    );
    assert_eq!(
        checked.stdout,
        want,
        "the CHECKED lane wrote through the wrong place on {}",
        entry.display()
    );
    let Some(native) = lane(entry, "--native") else {
        return;
    };
    assert_eq!(native.verdict, "exit(0)", "native verdict");
    assert_eq!(
        native.stdout,
        want,
        "the NATIVE lane wrote through the wrong place on {}",
        entry.display()
    );
    assert_eq!(
        checked.stdout,
        native.stdout,
        "the lanes disagree on {}",
        entry.display()
    );
}

/// Both lanes REFUSE it, with the same diagnostic code.
fn both_lanes_reject(entry: &Path, code: &str) {
    for flag in ["--checked", "--native"] {
        let Some(obs) = lane(entry, flag) else { continue };
        assert_eq!(
            obs.verdict,
            format!("fail({code})"),
            "{flag} did not reject {} with {code}",
            entry.display()
        );
    }
}

/// The plain shape: a scalar element lent `mut`. A callee that writes
/// its parameter must move the element the index names and no other.
#[test]
fn a_scalar_element_lends_by_address() {
    let entry = case(
        "s168_scalar_element",
        r#"
fn bump(mut n: int, by: int) {
    n = n + by
}

fn main() -> !int {
    var xs = [1, 2, 3]
    var i = 1
    bump(mut xs[i], 10)
    bump(mut xs[0], 100)
    print("{xs[0]} {xs[1]} {xs[2]}")
    0
}
"#,
    );
    both_lanes_say(&entry, "101 12 3\n");
}

/// A struct element lent whole, and a FIELD of one. The sibling
/// element and the sibling field must both be untouched — a spilled
/// copy with a writeback would be indistinguishable here on one
/// element and wrong on two, which is why both are printed.
#[test]
fn a_struct_element_and_one_of_its_fields_lend_by_address() {
    let entry = case(
        "s168_struct_element",
        r#"
struct Cell { n: int, m: int }

fn advance(mut c: Cell) {
    c.n = c.n + 1
    c.m = c.m + 10
}

fn bump(mut n: int) {
    n = n + 100
}

fn main() -> !int {
    var cs = [Cell { n: 1, m: 2 }, Cell { n: 3, m: 4 }]
    advance(mut cs[0])
    bump(mut cs[1].m)
    print("{cs[0].n} {cs[0].m} {cs[1].n} {cs[1].m}")
    0
}
"#,
    );
    both_lanes_say(&entry, "2 12 3 104\n");
}

/// The shape that motivated the place model: a field of a `mut`
/// PARAMETER, indexed, lent onward to a recursive call. A spilled
/// copy here would write back over the recursion's own work.
#[test]
fn the_recursive_insert_lends_through_a_mut_parameters_field() {
    let entry = case(
        "s168_recursive_insert",
        r#"
struct Word { key: int, count: int }
struct Bucket { tag: int, words: List[Word] }

fn bump(mut w: Word) {
    w.count = w.count + 1
}

fn insert(mut b: Bucket, k: int) {
    var i = 0
    while i < b.words.len {
        if b.words[i].key == k {
            bump(mut b.words[i])
            return
        }
        i = i + 1
    }
    (mut b.words).push(Word { key: k, count: 1 })
}

fn main() -> !int {
    var b = Bucket { tag: 3, words: [] }
    insert(mut b, 7)
    insert(mut b, 9)
    insert(mut b, 7)
    insert(mut b, 7)
    print("{b.words.len} {b.words[0].key} {b.words[0].count} {b.words[1].key} {b.words[1].count}")
    0
}
"#,
    );
    both_lanes_say(&entry, "2 7 3 9 1\n");
}

/// The write half of the same walk: `a.b.c = v` and `xs[i].f = v`,
/// both refused by name on the native pipe before s168 ("assignment
/// through nested places").
#[test]
fn nested_and_element_field_assignment_agree() {
    let entry = case(
        "s168_nested_assign",
        r#"
struct Inner { n: int }
struct Outer { a: Inner, b: int }
struct Cell { n: int, m: int }

fn main() -> !int {
    var o = Outer { a: Inner { n: 1 }, b: 2 }
    o.a.n = 41
    o.a.n += 1
    var cs = [Cell { n: 1, m: 2 }, Cell { n: 3, m: 4 }]
    cs[1].n = 30
    cs[0].m += 20
    print("{o.a.n} {o.b} {cs[0].n} {cs[0].m} {cs[1].n} {cs[1].m}")
    0
}
"#,
    );
    both_lanes_say(&entry, "42 2 1 22 30 4\n");
}

/// An out-of-range lend traps rather than lending an address past the
/// end — the element place carries the ordinary bounds check, once.
#[test]
fn an_out_of_range_element_lend_traps_on_both_lanes() {
    let entry = case(
        "s168_oob_lend",
        r#"
fn bump(mut n: int) {
    n = n + 1
}

fn main() -> !int {
    var xs = [1, 2]
    bump(mut xs[5])
    print("{xs[0]}")
    0
}
"#,
    );
    let checked = lane(&entry, "--checked").expect("checked runs");
    assert_eq!(checked.verdict, "trap(bounds)", "checked verdict");
    if let Some(native) = lane(&entry, "--native") {
        assert_eq!(native.verdict, "trap(bounds)", "native verdict");
    }
}

/// Exclusivity, leg one: every element of a container is ONE place
/// ([mem.model.place]), so a second element claim in the same call is
/// E1002. Without this a callee holds two addresses into one buffer.
#[test]
fn two_element_lends_in_one_call_are_rejected() {
    let entry = case(
        "s168_two_elements",
        r#"
fn add2(mut a: int, mut b: int) {
    a = a + 1
    b = b + 1
}

fn main() -> !int {
    var xs = [1, 2, 3]
    add2(mut xs[0], mut xs[1])
    print("{xs[0]}")
    0
}
"#,
    );
    both_lanes_reject(&entry, "E1002");
}

/// Exclusivity, leg two, and the leg that was missing. The arguments
/// spelled after a `mut` one are evaluated INSIDE its claim, so a
/// nested call that grows the container would free the buffer the
/// lend points into. Measured before the check existed: native
/// printed `1 66`, checked printed `6 66`, and nothing was reported.
#[test]
fn a_nested_call_claiming_the_container_is_rejected() {
    let entry = case(
        "s168_nested_claim",
        r#"
fn grow(mut xs: List[int]) -> int {
    var i = 0
    while i < 64 {
        (mut xs).push(90 + i)
        i = i + 1
    }
    5
}

fn take2(mut a: int, b: int) {
    a = a + b
}

fn main() -> !int {
    var xs = [1, 2]
    take2(mut xs[0], grow(mut xs))
    print("{xs[0]} {xs.len}")
    0
}
"#,
    );
    both_lanes_reject(&entry, "E1002");
}

/// The same leg over a SPILLED field rather than an element — the
/// shape that predates s168 and was wrong the other way: the
/// writeback restored the stale copy over the nested call's write.
#[test]
fn a_nested_call_claiming_a_spilled_bases_prefix_is_rejected() {
    let entry = case(
        "s168_nested_claim_field",
        r#"
struct R { a: int, b: int }

fn wipe(mut r: R) -> int {
    r.a = 1000
    r.b = 2000
    7
}

fn take2(mut a: int, b: int) {
    a = a + b
}

fn main() -> !int {
    var r = R { a: 1, b: 2 }
    take2(mut r.a, wipe(mut r))
    print("{r.a} {r.b}")
    0
}
"#,
    );
    both_lanes_reject(&entry, "E1002");
}

/// The guard on the exclusivity fix: disjoint bases and disjoint
/// FIELDS still pass. A rule that rejected these would be a new way
/// to refuse correct programs.
#[test]
fn disjoint_claims_are_still_accepted() {
    let entry = case(
        "s168_disjoint_ok",
        r#"
struct R { a: int, b: int }

fn add(mut n: int, k: int) {
    n = n + k
}

fn side(mut m: int) -> int {
    m = m + 1
    5
}

fn main() -> !int {
    var xs = [1, 2]
    var ys = [10, 20]
    add(mut xs[0], side(mut ys[1]))
    var r = R { a: 1, b: 2 }
    add(mut r.a, side(mut r.b))
    print("{xs[0]} {ys[1]} {r.a} {r.b}")
    0
}
"#,
    );
    both_lanes_say(&entry, "6 21 6 3\n");
}
