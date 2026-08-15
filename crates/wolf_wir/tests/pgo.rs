//! s45 in the mid-end: the reserved `hot=` slot, filled by content
//! hash, and what reading it changes.
//!
//! The properties under test are the ones the design commitments cash
//! out to:
//!
//! 1. **The format did not move.** s43 froze the summary schema with a
//!    `hot=` slot in it; s45 fills that slot and adds no field beside
//!    it, so the line shape, the field order and the version are all
//!    unchanged.
//! 2. **Content-hash keying.** A record applies iff its key is a
//!    body's D8 hash. Recompiling an untouched body keeps its record;
//!    changing a body loses exactly that one.
//! 3. **Unknown is not cold.** A body with no record keeps `hot=-`, so
//!    a partial profile never pessimizes the un-matched half.
//! 4. **A stale profile is inert.** Nothing it names exists, so the
//!    summary index — digest included — is bit-identical to the
//!    no-profile one, and so is everything keyed on it.

use std::collections::BTreeMap;

use wolf_wir::midend::summary::{Homes, apply_profile};
use wolf_wir::midend::{Options, Thresholds, optimize_whole_program};
use wolf_wir::parse_module;
use wolf_wir::profile::{Profile, Record};

/// Two RECURSIVE helpers plus `@main`. Recursion is what makes the
/// fixture have more than one body at all: the inliner refuses
/// recursive callees, so `@hot` and `@cold` survive the whole-program
/// phase instead of dissolving into `@main` and taking the whole
/// question of per-body records with them.
const PROGRAM: &str = "\
decl @hot(i64) -> i64
decl @cold(i64) -> i64
fn @hot(i64) -> i64 {
b0(%0: i64):
  %1 = iconst.i64 0
  %2 = icmp.sle %0, %1
  br %2, b1, b2
b1:
  ret %1
b2:
  %3 = iconst.i64 1
  %4 = isub.wrap %0, %3
  %5 = call @hot(%4)
  %6 = iadd.wrap %5, %3
  ret %6
}
fn @cold(i64) -> i64 {
b0(%0: i64):
  %1 = iconst.i64 0
  %2 = icmp.sle %0, %1
  br %2, b1, b2
b1:
  ret %1
b2:
  %3 = iconst.i64 2
  %4 = isub.wrap %0, %3
  %5 = call @cold(%4)
  %6 = iadd.wrap %5, %3
  ret %6
}
fn @main() -> i64 {
b0:
  %0 = iconst.i64 30
  %1 = call @hot(%0)
  %2 = call @cold(%0)
  %3 = iadd.wrap %1, %2
  ret %3
}
";

fn opts() -> Options {
    Options {
        verify_each: true,
        ..Options::default()
    }
}

/// The summary index for `PROGRAM` after the whole-program phase, and
/// the module it describes.
fn summarized() -> (wolf_wir::Module, wolf_wir::midend::summary::ProgramSummary) {
    let mut m = parse_module(PROGRAM).expect("fixture parses");
    let wp = optimize_whole_program(&mut m, &Homes::single(), &opts()).expect("green");
    (m, wp.summary)
}

/// A profile giving `hash` the counts in `blocks`.
fn profile_of(rows: &[(&str, Vec<u64>)]) -> Profile {
    let mut funcs: BTreeMap<String, Record> = BTreeMap::new();
    for (h, blocks) in rows {
        funcs.insert(
            (*h).to_string(),
            Record {
                blocks: blocks.clone(),
            },
        );
    }
    Profile { runs: 1, funcs }
}

#[test]
fn the_reserved_slot_fills_without_moving_the_format() {
    let (_, base) = summarized();
    let before = base.render();
    // Every function prints `hot=-` with no profile — s43's shape.
    for line in before.lines().filter(|l| l.starts_with("fn ")) {
        assert!(line.contains(" hot=- "), "unprofiled: {line}");
    }
    let mut s = base.clone();
    let first = &s.funcs[0];
    let p = profile_of(&[(first.body_hash.as_str(), vec![10; first.blocks as usize])]);
    apply_profile(&mut s, &p);
    let after = s.render();
    assert_eq!(
        before.lines().next(),
        after.lines().next(),
        "the version header is untouched"
    );
    assert_eq!(
        before.lines().count(),
        after.lines().count(),
        "no line was added or removed"
    );
    for (b, a) in before.lines().zip(after.lines()) {
        let (bf, af): (Vec<&str>, Vec<&str>) = (
            b.split_whitespace().collect(),
            a.split_whitespace().collect(),
        );
        assert_eq!(
            bf.len(),
            af.len(),
            "no field was added beside `hot=`\n{b}\n{a}"
        );
        for (x, y) in bf.iter().zip(&af) {
            if x.starts_with("hot=") {
                assert!(y.starts_with("hot="), "the slot stayed in place");
                continue;
            }
            assert_eq!(x, y, "only `hot=` moved:\n{b}\n{a}");
        }
    }
}

#[test]
fn hotness_is_a_rank_over_the_peak_block_count() {
    let (_, base) = summarized();
    assert!(base.funcs.len() >= 2, "the fixture keeps several bodies");
    let a = base.funcs[0].clone();
    let b = base.funcs[1].clone();
    let mut s = base.clone();
    let p = profile_of(&[
        (a.body_hash.as_str(), {
            let mut v = vec![1u64; a.blocks as usize];
            v[0] = 1000;
            v
        }),
        (b.body_hash.as_str(), {
            let mut v = vec![1u64; b.blocks as usize];
            v[0] = 250;
            v
        }),
    ]);
    let cov = apply_profile(&mut s, &p);
    assert_eq!(cov.matched, 2);
    assert_eq!(cov.stale(), 0);
    let hot = |name: &str| s.get(name).and_then(|f| f.hotness);
    assert_eq!(hot(&a.name), Some(1000), "the hottest body tops the scale");
    assert_eq!(hot(&b.name), Some(250), "and the rest are relative to it");
    // A rank, not a count: scaling the whole profile changes nothing,
    // which is what keeps the summary digest (and the cluster cache
    // key) independent of how long the training run was.
    let mut s2 = base.clone();
    let scaled = profile_of(&[
        (a.body_hash.as_str(), {
            let mut v = vec![7u64; a.blocks as usize];
            v[0] = 7000;
            v
        }),
        (b.body_hash.as_str(), {
            let mut v = vec![7u64; b.blocks as usize];
            v[0] = 1750;
            v
        }),
    ]);
    apply_profile(&mut s2, &scaled);
    assert_eq!(
        s.render(),
        s2.render(),
        "a longer training run of the same shape produces the same index"
    );
}

#[test]
fn a_body_without_a_record_stays_unknown_not_cold() {
    let (_, base) = summarized();
    let a = base.funcs[0].clone();
    let mut s = base.clone();
    let p = profile_of(&[(a.body_hash.as_str(), vec![5; a.blocks as usize])]);
    let cov = apply_profile(&mut s, &p);
    assert_eq!(cov.matched, 1);
    assert!(cov.unprofiled_bodies >= 1, "some body had no record");
    for f in &s.funcs {
        if f.name == a.name {
            assert!(f.hotness.is_some());
        } else {
            assert_eq!(
                f.hotness, None,
                "@{} has no record, so it is UNKNOWN — never cold",
                f.name
            );
        }
    }
}

#[test]
fn a_zeroed_record_is_proven_cold_and_says_so() {
    let (_, base) = summarized();
    let a = base.funcs[0].clone();
    let b = base.funcs[1].clone();
    let mut s = base.clone();
    let p = profile_of(&[
        (a.body_hash.as_str(), vec![100; a.blocks as usize]),
        (b.body_hash.as_str(), vec![0; b.blocks as usize]),
    ]);
    apply_profile(&mut s, &p);
    assert_eq!(
        s.get(&b.name).and_then(|f| f.hotness),
        Some(0),
        "a record of zeroes is real information: this body did not run"
    );
}

#[test]
fn a_fully_stale_profile_leaves_the_index_bit_identical() {
    let (_, base) = summarized();
    let mut s = base.clone();
    let p = profile_of(&[(&"e".repeat(64), vec![1, 2, 3])]);
    let cov = apply_profile(&mut s, &p);
    assert!(cov.fully_stale());
    assert_eq!(
        base.render(),
        s.render(),
        "nothing it names exists, so nothing about the build moves"
    );
    assert_eq!(base.digest(), s.digest(), "the digest the cache keys fold");
}

#[test]
fn recompiling_an_untouched_program_keeps_its_records() {
    let (_, first) = summarized();
    let (_, again) = summarized();
    let hashes_a: Vec<&str> = first.funcs.iter().map(|f| f.body_hash.as_str()).collect();
    let hashes_b: Vec<&str> = again.funcs.iter().map(|f| f.body_hash.as_str()).collect();
    assert_eq!(
        hashes_a, hashes_b,
        "the hash is over content, so recompiling is not an invalidation"
    );
}

#[test]
fn a_changed_body_loses_exactly_its_own_record() {
    let (_, base) = summarized();
    let edited_src = PROGRAM.replace("%3 = iconst.i64 2", "%3 = iconst.i64 5");
    let mut m2 = parse_module(&edited_src).expect("parses");
    let wp2 = optimize_whole_program(&mut m2, &Homes::single(), &opts()).expect("green");
    let old: std::collections::BTreeSet<&str> =
        base.funcs.iter().map(|f| f.body_hash.as_str()).collect();
    let new: std::collections::BTreeSet<&str> = wp2
        .summary
        .funcs
        .iter()
        .map(|f| f.body_hash.as_str())
        .collect();
    assert!(
        !old.is_disjoint(&new),
        "the bodies the edit did not touch keep their hashes"
    );
    assert_ne!(old, new, "and the one it did touch does not");
}

// ---- what reading the slot changes ----------------------------------

/// **The default thresholds are neutral**: consuming a profile does
/// not change a single body, so the bodies a profile describes are
/// still there afterwards and its block counts still reach LLVM.
///
/// This is the s45 finding, pinned. Records are keyed by the
/// post-mid-end body hash, so a mid-end knob that acts on hotness
/// destroys the record of every body it changes — measured on
/// `word_count`, where a hot 155-instruction callee inlined and took
/// both of the program's records with it. The knobs stay, at neutral;
/// what ships live is the channel that does not move the bodies.
#[test]
fn the_default_thresholds_do_not_move_a_single_body() {
    let mut plain = parse_module(PROGRAM).expect("parses");
    let wp_plain = optimize_whole_program(&mut plain, &Homes::single(), &opts()).expect("green");
    // A profile that says everything is as hot as it gets.
    let p = profile_of(
        &wp_plain
            .summary
            .funcs
            .iter()
            .map(|f| (f.body_hash.as_str(), vec![10_000; f.blocks as usize]))
            .collect::<Vec<_>>(),
    );
    let mut profiled = parse_module(PROGRAM).expect("parses");
    let wp_prof = optimize_whole_program(
        &mut profiled,
        &Homes::single(),
        &Options {
            profile: Some(p),
            ..opts()
        },
    )
    .expect("green");
    assert_eq!(
        wolf_wir::print_module(&plain),
        wolf_wir::print_module(&profiled),
        "at the default thresholds a profile changes no body at all"
    );
    let cov = wp_prof.stats.profile.expect("coverage is reported");
    assert_eq!(
        cov.matched, cov.records,
        "and so a fresh profile still applies in full: {cov}"
    );
    assert_eq!(cov.stale(), 0);
}

/// A hot callee earns the extra inline budget WHEN THE KNOB IS TURNED
/// UP; the same callee without a record does not. The unprofiled path
/// is the s42 decision verbatim.
///
/// The fixture puts `@big` and `@main` in DIFFERENT source modules,
/// which is what defers the decision to the cross-cluster round — the
/// round that has a summary index, hence hashes, hence hotness. In one
/// module the module phase would have decided before a profile could
/// be matched at all (see the whole-program module docs).
#[test]
fn hotness_buys_inline_budget_and_only_for_hot_callees() {
    // A callee just over the default hard cap (`inline_max` 64),
    // called once from a loop-free site: nothing but the hot bonus can
    // carry it.
    // A DEPENDENT chain, not a repeated expression: GVN would hash
    // eighty copies of `iadd.wrap %0, %1` down to one and the fixture
    // would measure nothing.
    let n = 80;
    let mut body = String::from("fn @big(i64) -> i64 {\nb0(%0: i64):\n  %1 = iconst.i64 1\n");
    for i in 0..n {
        body.push_str(&format!(
            "  %{} = iadd.wrap %{}, %1\n",
            i + 2,
            if i == 0 { 0 } else { i + 1 }
        ));
    }
    body.push_str(&format!(
        "  ret %{}\n}}\nfn @main() -> i64 {{\nb0:\n  %0 = iconst.i64 3\n  \
         %1 = call @big(%0)\n  ret %1\n}}\n",
        n + 1
    ));
    let mut homes = Homes::single();
    homes.set("big", "a");
    homes.set("main", "b");

    let base_opts = Options {
        verify_each: true,
        thresholds: Thresholds {
            // Single-use would carry it on its own; take that route
            // away so the hot bonus is the only variable.
            inline_single_use: 1,
            // The knob under test, off by default (see above).
            inline_hot_bonus: 64,
            ..Thresholds::default()
        },
        ..Options::default()
    };
    let mut plain = parse_module(&body).expect("parses");
    let wp_plain = optimize_whole_program(&mut plain, &homes, &base_opts).expect("green");
    assert_eq!(
        wp_plain.stats.opt.inlined_calls, 0,
        "unprofiled, the callee is over budget"
    );
    let big = wp_plain
        .summary
        .get("big")
        .expect("@big survives when it is not inlined");
    let p = profile_of(&[(big.body_hash.as_str(), vec![9_000; big.blocks as usize])]);

    let mut hot = parse_module(&body).expect("parses");
    let wp_hot = optimize_whole_program(
        &mut hot,
        &homes,
        &Options {
            profile: Some(p),
            ..base_opts.clone()
        },
    )
    .expect("green");
    assert_eq!(
        wp_hot.stats.opt.inlined_calls, 1,
        "a hot callee earns `inline_hot_bonus` and fits"
    );
    assert!(
        wp_hot.stats.opt.cross_cluster_inlined + wp_hot.stats.opt.cross_module_inlined > 0,
        "and it is the cross-module round that took it"
    );
}

/// `branch_weights` hands out counts only for bodies whose hash the
/// profile names, and the counts line up with the canonical order.
#[test]
fn branch_weights_are_offered_only_for_matched_bodies() {
    let (m, s) = summarized();
    let a = s.funcs[0].clone();
    let p = profile_of(&[(a.body_hash.as_str(), vec![42; a.blocks as usize])]);
    let w = wolf_wir::midend::branch_weights(&m, &p);
    assert_eq!(w.len(), 1, "one record, one body: {w:?}");
    let counts = w.get(&a.name).expect("the matched body is named");
    assert_eq!(counts.len(), a.blocks as usize, "one count per block");

    // A record whose block count disagrees with the body is refused
    // outright rather than shifted into place.
    let bad = profile_of(&[(a.body_hash.as_str(), vec![42; a.blocks as usize + 3])]);
    assert!(
        wolf_wir::midend::branch_weights(&m, &bad).is_empty(),
        "a length mismatch is a corrupt record, not a stale one"
    );
}
