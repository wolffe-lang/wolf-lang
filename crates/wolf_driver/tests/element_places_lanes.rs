//! eg00 (EGC, wolf-lang#446's campaign) — `[mem.model.place.elem]`'s
//! ten green witnesses, asserted on every machine; eg01 (EG1 and
//! wolf-lang#460) adds the literal-index rows, item 3's must-revival
//! rows and R3's failing twin; eg02 (EG2) adds element claims — each
//! exclusivity, loan and iteration leg with two distinct literals, and
//! its run-time-index twin that stays refused; eg02b (wolf-lang#472,
//! ruled A) adds item 1(c) under a claim — a member read beside an
//! element claim runs — and its whole-container twin that stays refused;
//! s185 (wolf-lang#474 and wolffe-lang/wolf-interp#149, ruled
//! 2026-09-30) adds item 1(c)'s header METHODS — `count`, `is_empty` and
//! a `Map`'s or `Pool`'s `len()` beside a moved or claimed element — and
//! the whole-read twins (`push`, `get`, `pop`, a slice, an impl method's
//! `self`) that stay refused; eg03 (EG3) adds item 4's R1 and R2 for one
//! call's claims — offsets of one unwritten local, a literal-range loop
//! index against a literal outside it — and the twins where the proof
//! fails or is not read.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs a `phase: run` entry on the NATIVE lane
//! only and checks a `fail(..)` entry's phase, so it cannot see the
//! checked machine's ANSWER, nor lupin's. Each case here asserts the
//! checked, native and release lanes give the row's verdict (and bytes)
//! and that lupin gives the clause's ruled verdict.
//!
//! The rows where lupin's answer is not the ruled one read a moved
//! element — `elem_move_same_const_read.lu`, the four
//! `elem_*_store_no_revive_*.lu` and `elem_sym_reassigned_no_revive.lu`:
//! lupin 0.1.40 marks a moved element but its index read never consults
//! the mark (wolffe-lang/wolf-interp#141). For the versions in
//! `PRE_MIRROR_LUPIN` those cases assert the MEASURED answer; any later
//! version must trap, so a pin bump that carries lupin forward without
//! the fix goes red here by name (s180's design).
//!
//! The one-place rows are the soundness bar the clause states: a
//! wolfgang lane that starts RUNNING one of them has made a run-time
//! index distinct without a proof rule — a regression, not progress.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// lupin releases that predate wolffe-lang/wolf-interp#141's fix: 0.1.40
/// alone. Emptied at the 0.1.42 pairing (r24): the pairing's sibling is
/// 0.1.42, which carries the fix and takes the ruled arm on every case.
const PRE_MIRROR_LUPIN: &[&str] = &[];

/// lupin releases whose `Map` read copies a non-`Copy` value out rather
/// than moving it, so a later read of that key sees the original
/// (wolffe-lang/wolf-interp#144). 0.1.41 is is56's head (`02433a0`),
/// measured by eg01b: it fixes #141 and still copies. Emptied at the
/// 0.1.42 pairing (r24): 0.1.42 carries is58's #144 mirror and takes the
/// ruled arm.
const PRE_MAP_MOVE_LUPIN: &[&str] = &[];

/// lupin releases that read a container MEMBER (`xs.len`) under an
/// element claim as a read of the whole container, and so trap
/// `bump(mut xs[0], xs.len)` where item 1(c) says the two are distinct
/// (wolffe-lang/wolf-lang#472, ruled A on 2026-09-29; lupin's mirror is
/// is59). Measured by eg02b on the published 0.1.40 and 0.1.41 archives.
/// Any later version must run these rows: a pin bump that carries lupin
/// forward without is59 goes red here by name. Emptied at the 0.1.42
/// pairing (r24): 0.1.42 carries is59 and runs these rows.
const PRE_MEMBER_LUPIN: &[&str] = &[];

/// lupin releases that read a header METHOD (`xs.count()`,
/// `xs.is_empty()`) under an element claim as a read of the whole
/// container, and so trap `bump(mut xs[0], xs.count())` where item 1(c)
/// says the header is not the element (wolf-lang#474, ruled 2026-09-30:
/// `len`, `count` and `is_empty` are header reads on every machine;
/// lupin's mirror is is60). Measured by s185 on the published 0.1.40 and
/// 0.1.41 archives. Any later version must run the row. Emptied at the
/// 0.1.42 pairing (r24): 0.1.42 carries is60 and runs it.
const PRE_HEADER_METHOD_LUPIN: &[&str] = &[];

/// lupin releases whose method receiver on a container or struct
/// holding a moved part runs instead of trapping — `push`, `get`, `pop`,
/// a slice, an impl method's `self` (wolffe-lang/wolf-interp#149, ruled
/// 2026-09-30: every method but the header reads reads the whole
/// container; lupin's mirror is is60). Measured by s185 on the published
/// 0.1.40 and 0.1.41 archives. Any later version must trap. Emptied at
/// the 0.1.42 pairing (r24): 0.1.42 carries is60 and traps.
const PRE_WHOLE_RECEIVER_LUPIN: &[&str] = &[];

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    codes: Vec<String>,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        codes: rec["diagnostics"]
            .as_array()
            .map(|ds| {
                ds.iter()
                    .filter_map(|d| d["code"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// One wolfgang lane. `None` means the host cannot run a native lane
/// (the s59 skip pattern), never a silent pass.
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    // An internal compiler error also exits 2; it fails here, never
    // skips (eg02: wolf-lang#470's release ICE read as a skip before
    // this gate learned it; wolf-lang#471 moved the test to `lane_exit`).
    if flag != "--checked"
        && lane_exit::environment_refusal(
            &out,
            &format!("conform-run {flag} on {}", entry.display()),
        )
    {
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

/// lupin's version and observation, or `None` when this box has no
/// sibling. `WOLF_PAIRING_REQUIRE_SIBLING` is the linux CI job saying
/// "one was arranged here" (r10/#253): there an absent sibling is a
/// failed fetch, and a gate that answers broken plumbing with a green
/// skip is not a gate.
fn lupin_says(entry: &Path) -> Option<(String, Obs)> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of [mem.model.place.elem]'s gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still compared against the RULED answer here; only the oracle \
             leg is absent"
        );
        return None;
    };
    let v = Command::new(&lupin)
        .arg("--version")
        .output()
        .expect("lupin --version runs");
    let version = String::from_utf8_lossy(&v.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string();
    assert!(!version.is_empty(), "lupin --version printed no version");
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    Some((version, parse_obs(&out.stdout, "lupin's observation")))
}

/// The corpus row itself is the program: one truth per file.
fn corpus(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/memory")
        .join(name);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// What one machine must answer: a verdict, and for a run the bytes.
#[derive(Clone, Copy)]
struct Want<'a> {
    verdict: &'a str,
    stdout: Option<&'a str>,
}

const fn runs(stdout: &str) -> Want<'_> {
    Want {
        verdict: "exit(0)",
        stdout: Some(stdout),
    }
}

const fn verdict(v: &str) -> Want<'_> {
    Want {
        verdict: v,
        stdout: None,
    }
}

fn check(machine: &str, entry: &Path, obs: &Obs, want: Want<'_>) {
    assert_eq!(
        obs.verdict,
        want.verdict,
        "the {machine} verdict on {} (diagnostics {:?})",
        entry.display(),
        obs.codes
    );
    if let Some(bytes) = want.stdout {
        assert_eq!(
            obs.stdout,
            bytes,
            "the {machine} answer on {}",
            entry.display()
        );
    }
}

/// Every wolfgang lane answers `wolfgang`; lupin answers `pre` at a
/// `PRE_MIRROR_LUPIN` version and `ruled` at any other.
fn every_lane(name: &str, wolfgang: Want<'_>, pre: Want<'_>, ruled: Want<'_>) {
    every_lane_pinned(name, PRE_MIRROR_LUPIN, wolfgang, pre, ruled)
}

/// `every_lane` with the pre-mirror versions named by the caller.
fn every_lane_pinned(
    name: &str,
    pre_versions: &[&str],
    wolfgang: Want<'_>,
    pre: Want<'_>,
    ruled: Want<'_>,
) {
    let entry = corpus(name);
    let checked = lane(&entry, "--checked").expect("the checked lane always runs");
    check("checked", &entry, &checked, wolfgang);
    for flag in ["--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            check(flag, &entry, &obs, wolfgang);
        }
    }
    let Some((version, lupin)) = lupin_says(&entry) else {
        return;
    };
    if pre_versions.contains(&version.as_str()) {
        check(
            &format!("lupin {version} (pre-mirror)"),
            &entry,
            &lupin,
            pre,
        );
    } else {
        check(&format!("lupin {version}"), &entry, &lupin, ruled);
    }
}

/// Item 1(d): `move t.0` leaves `t.1` readable — tuple positions are
/// field steps, distinct before the clause.
#[test]
fn a_tuple_position_moves_alone() {
    let want = runs("1 2\n");
    every_lane("elem_tuple_pos_move.lu", want, want, want);
}

/// Item 1(d), exclusivity: `bump2(mut t.0, mut t.1)` is two disjoint
/// claims.
#[test]
fn two_tuple_positions_go_mut_together() {
    let want = runs("2 12\n");
    every_lane("elem_tuple_pos_mut.lu", want, want, want);
}

/// Item 2: `move xs[i]` against a read of `xs[0]` — one place; lupin
/// sees `i = 1` and runs it (the conservatism direction).
#[test]
fn a_run_time_index_and_a_literal_are_one_place() {
    every_lane(
        "elem_dyn_move_const_read.lu",
        verdict("fail(E1001)"),
        runs("2 1\n"),
        runs("2 1\n"),
    );
}

/// Item 2: `add2(mut xs[i], mut xs[j])` — no rule proves `i != j`.
#[test]
fn two_run_time_indices_are_one_place() {
    every_lane(
        "elem_dyn_mut_pair.lu",
        verdict("fail(E1002)"),
        runs("2 12\n"),
        runs("2 12\n"),
    );
}

/// Item 2: the same literal twice is the same element on every
/// machine.
#[test]
fn the_same_literal_twice_is_one_element_everywhere() {
    let trap = verdict("trap(exclusivity)");
    every_lane("elem_same_const_mut.lu", verdict("fail(E1002)"), trap, trap);
}

/// Item 4, where R1 fails: `xs[i]` and `xs[k + 1]` over two locals —
/// and equal at run time, which lupin's trap shows.
#[test]
fn an_offset_through_another_local_stays_one_place() {
    let trap = verdict("trap(exclusivity)");
    every_lane(
        "elem_offset_other_local.lu",
        verdict("fail(E1002)"),
        trap,
        trap,
    );
}

/// Item 4, where R2 fails: a `0..3` loop index against `xs[0]`.
#[test]
fn a_loop_range_containing_the_literal_stays_one_place() {
    let trap = verdict("trap(exclusivity)");
    every_lane(
        "elem_loop_covers_const.lu",
        verdict("fail(E1002)"),
        trap,
        trap,
    );
}

/// EG3 (eg03), R1: `add2(mut xs[i], mut xs[i + 1])` — two offsets of
/// one local the call leaves unwritten are distinct (K13, unparked).
#[test]
fn two_offsets_of_one_unwritten_local_go_mut_together() {
    let want = runs("2 12\n");
    every_lane("elem_offset_mut_pair.lu", want, want, want);
}

/// EG3 (eg03), R2: `for i in 1..3 { add2(mut xs[0], mut xs[i]) }` — a
/// literal-range index the body never writes is never `0` (K14,
/// unparked).
#[test]
fn a_loop_index_outside_the_literal_goes_mut_beside_it() {
    let want = runs("3 12 13\n");
    every_lane("elem_loop_induction_mut.lu", want, want, want);
}

/// EG3 (eg03): R1 and R2 on every leg of the call-surface relation —
/// two nonzero offsets, the offset first, a nested path, a lent read,
/// `..=`, a literal above the range, a `Copy` read of the index between
/// the claims, and a `while` loop that writes the index between calls.
#[test]
fn offsets_and_loop_indices_are_distinct_on_every_leg() {
    let want = runs("4 14 | 11 3 | 4 15 | 2 2 | 3 12 13 | 2 3 23 | 2 12 | 2 13 13\n");
    every_lane("elem_dyn_proofs_legs.lu", want, want, want);
}

/// EG3's twins that lupin traps: each is one element at run time.
fn one_element_at_run_time(name: &str) {
    let trap = verdict("trap(exclusivity)");
    every_lane(name, verdict("fail(E1002)"), trap, trap);
}

/// R1 fails: a block argument writes `i` between the claims.
#[test]
fn an_offset_after_a_block_that_writes_the_index_stays_one_place() {
    one_element_at_run_time("elem_offset_rebound_mut.lu");
}

/// R1 fails: a nested `back(mut i)` writes `i` between the claims.
#[test]
fn an_offset_after_a_call_that_writes_the_index_stays_one_place() {
    one_element_at_run_time("elem_offset_rebound_call_mut.lu");
}

/// R2 fails: the body assigns the loop index (`i = 0`).
#[test]
fn a_loop_index_the_body_writes_stays_one_place() {
    one_element_at_run_time("elem_loop_induction_written_mut.lu");
}

/// R2 fails: `1..=2` holds the literal `2`.
#[test]
fn an_inclusive_range_holding_the_literal_stays_one_place() {
    one_element_at_run_time("elem_loop_inclusive_covers_const.lu");
}

/// R1's same half: `xs[i]` and `xs[i + 0]` are one element.
#[test]
fn a_zero_offset_is_the_same_element() {
    one_element_at_run_time("elem_offset_zero_mut.lu");
}

/// R1 relates offsets of one local, never an offset and a literal.
#[test]
fn an_offset_against_a_literal_stays_one_place() {
    one_element_at_run_time("elem_offset_vs_literal_mut.lu");
}

/// EG3's conservatism twins: shapes R1/R2 do not read stay one place;
/// lupin runs each with its values.
fn not_read_by_the_rules(name: &str, bytes: &str) {
    every_lane(name, verdict("fail(E1002)"), runs(bytes), runs(bytes));
}

/// A subtracted offset (`xs[i - 1]`) is not R1's spelling.
#[test]
fn a_subtracted_offset_stays_one_place() {
    not_read_by_the_rules("elem_offset_negative_mut.lu", "11 3\n");
}

/// A local offset (`xs[i + j]`) is not a literal.
#[test]
fn a_local_offset_stays_one_place() {
    not_read_by_the_rules("elem_offset_nonliteral_mut.lu", "2 12\n");
}

/// A range bound that is a local (`1..n`) is not a literal range.
#[test]
fn a_non_literal_range_stays_one_place() {
    not_read_by_the_rules("elem_loop_nonliteral_range_mut.lu", "3 12 13\n");
}

/// Item 2: `Pool` handles are run-time values; lupin declines `Pool`.
#[test]
fn two_pool_handles_are_one_place() {
    let declines = verdict("unsupported");
    every_lane(
        "elem_pool_handles_one_place.lu",
        verdict("fail(E1002)"),
        declines,
        declines,
    );
}

/// The moved element itself stays unreadable: E1001 on wolfgang, and
/// `[mem.tier0.move.2]`'s trap on lupin once wolf-interp#141 is fixed.
#[test]
fn the_moved_element_itself_stays_unreadable() {
    every_lane(
        "elem_move_same_const_read.lu",
        verdict("fail(E1001)"),
        runs("1 1\n"),
        verdict("trap(use-after-move)"),
    );
}

/// Item 4's R3: out through `xs[i]`, back through `xs[i]`, read.
#[test]
fn a_store_through_the_same_index_revives_the_element() {
    let want = runs("1 3\n");
    every_lane("elem_same_index_revive.lu", want, want, want);
}

// ------------------------------------------------ eg01: item 3 (#460) --
//
// A store revives a moved element only through an index that provably
// denotes it. Before eg01 every one of these compiled (wolf-lang#460):
// native and release printed the aliased `2 2 1`, checked trapped on
// the heap twins and printed `1 1 9` on the `Copy` ones (B126). lupin
// 0.1.40's index read ignores the moved mark (wolffe-lang/wolf-interp#141),
// so its measured answer is pinned for the pre-mirror versions and the
// trap is asserted for any later one.

/// Item 3: `xs[1] = [5]` after `move xs[0]` revives nothing — #460's row.
#[test]
fn a_store_at_another_literal_revives_nothing() {
    every_lane(
        "elem_const_store_no_revive_heap.lu",
        verdict("fail(E1001)"),
        runs("2 1 1\n"),
        verdict("trap(use-after-move)"),
    );
}

/// Item 3, the `Copy` twin (B126's shape on the checked machine).
#[test]
fn a_copy_element_is_not_revived_by_a_sibling_store() {
    every_lane(
        "elem_const_store_no_revive_int.lu",
        verdict("fail(E1001)"),
        runs("1 1 9\n"),
        verdict("trap(use-after-move)"),
    );
}

/// Item 3 through a run-time index: `xs[i] = [5]` is not provably `xs[0]`.
#[test]
fn a_store_through_a_run_time_index_revives_nothing() {
    every_lane(
        "elem_dyn_store_no_revive_heap.lu",
        verdict("fail(E1001)"),
        runs("2 1 1\n"),
        verdict("trap(use-after-move)"),
    );
}

/// Item 3 through a run-time index, the `Copy` twin.
#[test]
fn a_copy_element_is_not_revived_through_a_run_time_index() {
    every_lane(
        "elem_dyn_store_no_revive_int.lu",
        verdict("fail(E1001)"),
        runs("1 1 9\n"),
        verdict("trap(use-after-move)"),
    );
}

// ------------------------------------------ eg01: EG1 and item 3's + --

/// Item 1(a) on a non-`Copy` element: `move xs[0]`, read `xs[1].len`.
#[test]
fn a_literal_element_moves_alone() {
    let want = runs("1 2\n");
    every_lane("elem_const_move_heap.lu", want, want, want);
}

/// Item 1(a), the path rule: `move xs[0].tags`, read `xs[1].tags.len`.
#[test]
fn paths_through_distinct_literals_are_distinct() {
    let want = runs("1 2\n");
    every_lane("elem_const_field_of_elem.lu", want, want, want);
}

/// Item 1(b): the value at `m["a"]` read out, `m["b"]` still readable.
#[test]
fn distinct_literal_keys_are_distinct_places() {
    let want = runs("1 2\n");
    every_lane("elem_key_move_map.lu", want, want, want);
}

/// Item 1(c): `move xs[0]`, then `xs.len` — the header is no element.
#[test]
fn the_header_is_not_an_element() {
    let want = runs("1 2\n");
    every_lane("elem_len_after_move.lu", want, want, want);
}

/// Item 3, positive: a store through the same literal revives it.
#[test]
fn a_store_through_the_same_literal_revives_the_element() {
    let want = runs("2 1 7\n");
    every_lane("elem_const_store_revives.lu", want, want, want);
}

/// R3 fails once the local is written between the move and the store.
#[test]
fn a_store_through_a_reassigned_local_revives_nothing() {
    every_lane(
        "elem_sym_reassigned_no_revive.lu",
        verdict("fail(E1001)"),
        runs("1 1 2\n"),
        verdict("trap(use-after-move)"),
    );
}

// ------------------------------------- eg01b: R3 over any `Copy` local --
//
// The maintainer's ruling (2026-09-26): R1 and R3 are stated over any
// `Copy`-typed local, not only an integer one. eg01 had narrowed
// revival to integer locals, so each positive row below was E1001 at
// `7841f7dc` (and compiled on 0.2.17 through wolf-lang#460's any-store
// revival); the blur row stays E1001, and 0.2.17 printed a moved
// buffer for it on every wolfgang lane.

/// R3 through a `str` key, as a parameter and as a local.
#[test]
fn a_store_through_the_same_str_key_revives_the_value() {
    let want = runs("3 7 2\n");
    every_lane("elem_str_key_revive.lu", want, want, want);
}

/// R3 through `char` and `bool` key locals.
#[test]
fn a_store_through_the_same_char_or_bool_key_revives_the_value() {
    let want = runs("2 2 2 4\n");
    every_lane("elem_char_bool_key_revive.lu", want, want, want);
}

/// R3 fails once the `str` key local is written between: the store
/// names `m["b"]`, and `m["a"]` stays read out. lupin's map read copies
/// (wolffe-lang/wolf-interp#144), so it prints the original `1` at the
/// `PRE_MAP_MOVE_LUPIN` versions; the ruled answer is the trap.
#[test]
fn a_store_through_a_reassigned_key_revives_nothing() {
    every_lane_pinned(
        "elem_key_reassigned_no_revive.lu",
        PRE_MAP_MOVE_LUPIN,
        verdict("fail(E1001)"),
        runs("1\n"),
        verdict("trap(use-after-move)"),
    );
}

/// R3 through a `Pool` handle local; lupin declines `Pool` by name.
#[test]
fn a_store_through_the_same_pool_handle_revives_the_element() {
    every_lane(
        "elem_pool_handle_revive.lu",
        runs("2\n"),
        verdict("unsupported"),
        verdict("unsupported"),
    );
}

// ---------------------------------------------- eg02: element claims ----
//
// EG2: every rule that asks whether two paths conflict under a claim —
// call exclusivity (`[mem.tier0.excl]`, E1002, with D39's read and
// s168's nested call inside a `mut` argument), borrows
// (`[mem.tier0.borrow]`) and iteration (`[mem.iter.excl]`, E1013) —
// reads the element relation eg01 gave the moves pass. Each distinct
// row below was E1002/E1013 on every wolfgang lane through 0.2.18;
// each has a run-time-index twin that must stay refused.

/// Item 1(a), exclusivity: `add2(mut xs[0], mut xs[1])` (eg00's parked
/// witness, ruled `2 12` on every machine).
#[test]
fn two_literal_elements_go_mut_together() {
    let want = runs("2 12\n");
    every_lane("elem_const_mut_pair.lu", want, want, want);
}

/// The path rule under claims: `add2(mut g[0][1], mut g[1][0])`.
#[test]
fn paths_through_distinct_literals_go_mut_together() {
    let want = runs("3 13\n");
    every_lane("elem_const_nested_mut.lu", want, want, want);
}

/// The headline shape: `swap(mut xs[0], mut xs[1])`.
#[test]
fn two_literal_elements_swap() {
    let want = runs("2 1 3\n");
    every_lane("elem_const_swap.lu", want, want, want);
}

/// Whole struct elements owning heap lists, both pushed through.
#[test]
fn two_heap_elements_swap_and_grow() {
    let want = runs("2 1 1 2\n");
    every_lane("elem_const_swap_heap.lu", want, want, want);
}

/// The same field of two distinct elements.
#[test]
fn one_field_of_two_literal_elements_goes_mut_together() {
    let want = runs("2 12\n");
    every_lane("elem_const_field_mut_pair.lu", want, want, want);
}

/// D39 per element: a `Copy` read of `xs[1]` inside the claim on `xs[0]`.
#[test]
fn a_literal_element_is_read_inside_another_ones_claim() {
    let want = runs("3 2\n");
    every_lane("elem_const_read_after_mut.lu", want, want, want);
}

/// s168's nested-call leg per element: `put(mut xs[0], grow(mut xs[1]))`.
#[test]
fn a_nested_call_may_claim_another_literal_element() {
    let want = runs("2 5 65\n");
    every_lane("elem_const_nested_call.lu", want, want, want);
}

/// `mut` against `take`: `put(mut xs[0], take xs[1])`.
#[test]
fn a_literal_element_moves_into_a_call_another_one_mutates_in() {
    let want = runs("2 2\n");
    every_lane("elem_const_take_while_mut.lu", want, want, want);
}

/// `read` against `take`: `keep(xs[0], take xs[1])`.
#[test]
fn a_literal_element_moves_into_a_call_another_one_is_lent_to() {
    let want = runs("3 1\n");
    every_lane("elem_const_read_while_take.lu", want, want, want);
}

/// `[mem.iter.excl]` per element: iterate `xs[0]`, push into `xs[1]`.
#[test]
fn another_literal_element_changes_under_the_loop() {
    let want = runs("2 3 2\n");
    every_lane("elem_const_iter_mut.lu", want, want, want);
}

/// `[mem.tier0.borrow]` per element: a dyn loan of `xs[0]`, a store to
/// `xs[1]` while it is live.
#[test]
fn another_literal_element_is_stored_under_a_loan() {
    let want = runs("7 9\n");
    every_lane("elem_const_dyn_loan.lu", want, want, want);
}

/// Item 2 under a claim, read: `bump(mut xs[0], xs[i])`. E1002 under
/// D39 through 0.2.19; a read of a claimed place in a later argument is
/// legal since s186 (`[mem.tier0.excl.4]`), and every lane prints
/// lupin's bytes.
#[test]
fn a_run_time_index_may_be_read_beside_a_claim() {
    let want = runs("3 2\n");
    every_lane("elem_dyn_read_after_mut.lu", want, want, want);
}

/// Item 2 under the nested-call leg: `put(mut xs[0], grow(mut xs[i]))`.
#[test]
fn a_nested_call_may_not_claim_a_run_time_index() {
    let lupin = runs("2 65\n");
    every_lane(
        "elem_dyn_nested_call.lu",
        verdict("fail(E1002)"),
        lupin,
        lupin,
    );
}

/// Item 2 under `mut` against `take`: `put(mut xs[0], take xs[i])` —
/// E1001 (the claim's use may reach the moved element) and E1002 (the
/// element moves while the claim holds), on every wolfgang lane.
#[test]
fn a_run_time_index_does_not_move_into_a_claiming_call() {
    let lupin = runs("2\n");
    every_lane(
        "elem_dyn_take_while_mut.lu",
        verdict("fail(E1001)"),
        lupin,
        lupin,
    );
    let entry = corpus("elem_dyn_take_while_mut.lu");
    for flag in ["--checked", "--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            assert!(
                obs.codes.iter().any(|c| c == "E1002"),
                "{flag}: the exclusivity half is reported too ({:?})",
                obs.codes
            );
        }
    }
}

/// Item 2 under iteration: iterate `xs[0]`, push into `xs[i]`.
#[test]
fn a_run_time_index_does_not_change_under_the_loop() {
    let lupin = runs("2 3\n");
    every_lane("elem_dyn_iter_mut.lu", verdict("fail(E1013)"), lupin, lupin);
}

/// Item 2 under a loan: a store through `xs[i]` while `xs[0]` is lent.
#[test]
fn a_run_time_index_is_not_stored_under_a_loan() {
    let lupin = runs("7 9\n");
    every_lane("elem_dyn_dyn_loan.lu", verdict("fail(E1002)"), lupin, lupin);
}

/// A path and its prefix: `add2(mut g[0], mut g[0][1])`.
#[test]
fn an_element_and_a_path_inside_it_stay_one_claim() {
    let trap = verdict("trap(exclusivity)");
    every_lane(
        "elem_prefix_mut_pair.lu",
        verdict("fail(E1002)"),
        trap,
        trap,
    );
}

/// Item 1(c) under a claim (eg02b, wolf-lang#472 ruled A):
/// `bump(mut xs[0], xs.len)` — the member is the header, never the
/// element. eg02 kept it E1002 because lupin traps it; lupin through
/// `PRE_MEMBER_LUPIN` still does.
#[test]
fn a_member_read_under_an_element_claim_runs() {
    let want = runs("4 3\n");
    every_lane_pinned(
        "elem_member_read_after_mut.lu",
        PRE_MEMBER_LUPIN,
        want,
        verdict("trap(exclusivity)"),
        want,
    );
}

/// Item 1(c) under a claim, every leg: a member read before the claim,
/// through a run-time index, one level down, the outer container's
/// member, and inside a nested call.
#[test]
fn a_member_read_beside_an_element_claim_runs_on_every_leg() {
    let want = runs("4 8 6 4 5\n");
    every_lane_pinned(
        "elem_member_read_under_claims.lu",
        PRE_MEMBER_LUPIN,
        want,
        verdict("trap(exclusivity)"),
        want,
    );
}

/// The one-place twin: the whole container, not a member, read under an
/// element claim — a prefix, refused as it always was.
#[test]
fn the_whole_container_read_under_an_element_claim_stays_refused() {
    let trap = verdict("trap(exclusivity)");
    every_lane(
        "elem_claim_whole_read.lu",
        verdict("fail(E1002)"),
        trap,
        trap,
    );
}

/// wolf-lang#470 (found by eg02): two `mut` claims in ONE caller region
/// — here two fields off a `mut` parameter, the pre-EG2 shape of it —
/// ICEd the release tier's inliner through 0.2.18. Every element-pair
/// case above asserts `--release` too, which is the other half.
#[test]
fn two_claims_in_one_caller_region_survive_the_release_inliner() {
    let want = runs("2 12\n");
    every_lane("mut_two_fields_one_region.lu", want, want, want);
}

// ------------------------------- s185: the header methods (#474, #149) --
//
// The maintainer's ruling of 2026-09-30: `len`, `count` and `is_empty`
// read only a container's header, so they are allowed beside a claimed
// or moved element on every machine; every other method reads the whole
// container. Through 0.2.18 every wolfgang lane refused a header method
// on a container with a moved element (E1001) — the receiver was read
// as the whole container — and already ran one beside an element claim.

/// Item 1(c) beside a moved element: `count()` and `is_empty()` on the
/// container and on every container above the moved element.
#[test]
fn header_methods_run_beside_a_moved_element() {
    let want = runs("1 2 false 20 1 2 2 false false\n");
    every_lane("elem_header_methods_after_move.lu", want, want, want);
}

/// The `Map` header: `count()`, `is_empty()`, `len()` after a value is
/// read out.
#[test]
fn map_header_methods_run_beside_a_read_out_value() {
    let want = runs("1 2 false 2\n");
    every_lane("elem_header_methods_after_move_map.lu", want, want, want);
}

/// The `Pool` header: `len()` and `is_empty()` after `move p[h]`; lupin
/// declines `Pool` by name.
#[test]
fn pool_header_methods_run_beside_a_moved_element() {
    let declines = verdict("unsupported");
    every_lane(
        "elem_header_methods_after_move_pool.lu",
        runs("1 1 false\n"),
        declines,
        declines,
    );
}

/// Item 1(c) under a claim, for the methods: `bump(mut xs[0],
/// xs.count())`, `flag(mut xs[1], xs.is_empty())` — every wolfgang lane
/// ran this through 0.2.18 and the ruling keeps it (wolf-lang#474);
/// lupin through `PRE_HEADER_METHOD_LUPIN` traps.
#[test]
fn header_methods_run_under_an_element_claim() {
    let want = runs("4 3\n");
    every_lane_pinned(
        "elem_header_methods_under_claim.lu",
        PRE_HEADER_METHOD_LUPIN,
        want,
        verdict("trap(exclusivity)"),
        want,
    );
}

/// A whole-read method on a container (or struct) holding a moved part:
/// E1001 on every wolfgang lane; lupin through `PRE_WHOLE_RECEIVER_LUPIN`
/// runs it and prints `pre`, any later lupin traps.
fn a_whole_read_stays_refused(name: &str, pre: &str) {
    every_lane_pinned(
        name,
        PRE_WHOLE_RECEIVER_LUPIN,
        verdict("fail(E1001)"),
        runs(pre),
        verdict("trap(use-after-move)"),
    );
}

/// `(mut xs).push([3])` after `move xs[0]`.
#[test]
fn push_after_a_moved_element_stays_refused() {
    a_whole_read_stays_refused("elem_whole_read_push_after_move.lu", "1 3\n");
}

/// `xs.get(1)` after `move xs[0]` — a live index, through the method.
#[test]
fn get_after_a_moved_element_stays_refused() {
    a_whole_read_stays_refused("elem_whole_read_get_after_move.lu", "1 2\n");
}

/// `(mut xs).pop()` after `move xs[0]`, the last element live.
#[test]
fn pop_after_a_moved_element_stays_refused() {
    a_whole_read_stays_refused("elem_whole_read_pop_after_move.lu", "1 2 1\n");
}

/// `xs[1..3]` beside the moved `xs[0]`.
#[test]
fn a_slice_beside_a_moved_element_stays_refused() {
    a_whole_read_stays_refused("elem_whole_read_slice_after_move.lu", "1 2\n");
}

/// An impl method's `self` after `move p.x`.
#[test]
fn an_impl_self_after_a_moved_field_stays_refused() {
    a_whole_read_stays_refused("elem_whole_read_self_after_move.lu", "1 2\n");
}

/// A header read is a builtin container's, not a method name: an impl
/// method spelled `count`/`is_empty` takes the whole `self`.
#[test]
fn a_method_named_count_is_not_a_header_read() {
    a_whole_read_stays_refused("elem_whole_read_named_count_after_move.lu", "1 2 false\n");
}
