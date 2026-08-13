//! s42 mid-end evidence: per-pass WIR snapshots on fixtures (the s01
//! review-artifact convention), the two holy-grail witnesses LOCKED as
//! snapshots (contract acceptance), the amendment passes' positive AND
//! negative litmuses, and the conc-conservatism structural proof.
//!
//! Every fixture runs with `verify_each` forced ON: the verifier and
//! the fact-custody audit run at every pass boundary, so a fixture
//! passing here is also a pass-manager regression test.

use wolf_wir::midend::{OptStats, Options, optimize_module, run_named_pass};
use wolf_wir::{parse_module, print_module, verify_module};

fn opts() -> Options {
    Options {
        verify_each: true,
        ..Options::default()
    }
}

/// Run one named pass over every function; return the canonical dump.
fn one_pass(src: &str, pass: &str) -> (String, OptStats) {
    let mut m = parse_module(src).expect("fixture parses");
    verify_module(&m).expect("fixture verifies before");
    let mut stats = OptStats::default();
    let o = opts();
    for fid in m.funcs.keys().collect::<Vec<_>>() {
        run_named_pass(&mut m, fid, pass, &o, &mut stats).expect("pass + verify green");
    }
    verify_module(&m).expect("fixture verifies after");
    (print_module(&m), stats)
}

fn pipeline(src: &str) -> (String, OptStats) {
    let mut m = parse_module(src).expect("fixture parses");
    verify_module(&m).expect("fixture verifies before");
    let stats = optimize_module(&mut m, &opts()).expect("pipeline green");
    (print_module(&m), stats)
}

// ------------------------------------------------------- simplify ----

/// Rule-table hits, GVN dedup, constant folding, and trap-free DCE in
/// one body — checked ops preserved except where the TABLE licenses
/// the rewrite (x+0, x-x: trap-free rows).
#[test]
fn simplify_rules_gvn_fold() {
    let src = "fn @f(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iconst.i64 0\n  \
               %2 = iadd.chk %0, %1\n  \
               %3 = iadd.chk %0, %0\n  \
               %4 = iadd.chk %0, %0\n  \
               %5 = isub.chk %2, %0\n  \
               %6 = iadd.chk %3, %5\n  \
               %7 = imul.chk %4, %6\n  \
               ret %7\n\
               }\n";
    let (out, stats) = one_pass(src, "simplify");
    assert!(stats.rule_hits >= 2, "x+0 and x-x rows fire: {stats}");
    assert!(stats.gvn_hits >= 1, "duplicate iadd.chk dedups: {stats}");
    insta::assert_snapshot!("simplify_rules_gvn_fold", out);
}

/// `br` on a constant folds to `jmp`; the untaken arm (with its
/// provably-overflowing check) vanishes at the pass boundary — and a
/// reachable provable overflow becomes `trap.overflow` (X3: the fold
/// IS the runtime outcome).
#[test]
fn simplify_branch_fold_and_trap() {
    let src = "fn @g() -> i64 {\n\
               b0:\n  \
               %0 = bconst true\n  \
               br %0, b1, b2\n\
               b1:\n  \
               %1 = iconst.i64 3\n  \
               ret %1\n\
               b2:\n  \
               %2 = iconst.i64 9223372036854775807\n  \
               %3 = iadd.chk %2, %2\n  \
               ret %3\n\
               }\n";
    let (out, stats) = one_pass(src, "simplify");
    assert!(stats.branch_folds >= 1);
    assert!(!out.contains("b2:"), "untaken arm compacted away:\n{out}");
    insta::assert_snapshot!("simplify_branch_fold", out);
}

// --------------------------------------------------------- inline ----

/// A small pure callee inlines; the caller then folds through it.
/// Error-union style multi-block callees join at a continuation.
#[test]
fn inline_scalar_callee() {
    let src = "fn @twice(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iadd.chk %0, %0\n  \
               ret %1\n\
               }\n\
               \n\
               fn @main() -> i64 {\n\
               b0:\n  \
               %0 = iconst.i64 21\n  \
               %1 = call @twice(%0)\n  \
               ret %1\n\
               }\n";
    let (out, stats) = pipeline(src);
    assert_eq!(stats.inlined_calls, 1, "{stats}");
    assert!(
        !out.contains("call @twice"),
        "call site gone after inline:\n{out}"
    );
    assert!(out.contains("iconst.i64 42"), "folded through:\n{out}");
    insta::assert_snapshot!("inline_scalar_callee", out);
}

/// A single-block callee with token params (the mut-param shape):
/// region identity rebinds formal r0 to the caller's actual region,
/// and the call's successor token maps to the inlined chain's final
/// version — fact custody under inlining, the correctness crux.
#[test]
fn inline_token_callee_rebinds_region() {
    let src = "fn @bump(mut ptr, mem.r0) -> i64 {\n  \
               fact deref %0 8 : excl.mut\n\
               b0(%0: ptr, %1: mem.r0):\n  \
               %2 = load.i64 %0, %1\n  \
               %3 = iconst.i64 1\n  \
               %4 = iadd.chk %2, %3\n  \
               %5 = store.i64 %4, %0, %1\n  \
               ret %4\n\
               }\n\
               \n\
               fn @main() -> i64 {\n\
               b0:\n  \
               %0: ptr, %1: mem.r0 = region.new\n  \
               %2 = iconst.i64 8\n  \
               %3: ptr, %4: mem.r0 = region.alloc %0, %2, %1\n  \
               %5 = iconst.i64 41\n  \
               %6 = store.i64 %5, %3, %4\n  \
               %7, %8 = call @bump(%3, %6)\n  \
               region.free %0, %8\n  \
               ret %7\n\
               }\n";
    let (out, stats) = pipeline(src);
    assert!(stats.inlined_calls >= 1, "{stats}");
    assert!(!out.contains("call @bump"), "inlined:\n{out}");
    insta::assert_snapshot!("inline_token_callee", out);
}

// --------------------------------------------------------- memopt ----

/// HOLY-GRAIL WITNESS 1 (contract acceptance, locked): a `read`-param
/// (frozen) load CSEs ACROSS AN OPAQUE CALL — the call does not
/// consume region r0's token, so it provably cannot write r0; the
/// second load is the first. No points-to analysis anywhere: the
/// token discipline is the oracle.
#[test]
fn witness_read_param_load_cse_across_opaque_call() {
    let src = "decl @opaque()\n\
               \n\
               fn @f(read ptr, mem.r0) -> i64 {\n  \
               fact frozen %0 : frozen.read\n\
               b0(%0: ptr, %1: mem.r0):\n  \
               %2 = load.i64 %0, %1\n  \
               call @opaque()\n  \
               %4 = load.i64 %0, %1\n  \
               %5 = iadd.chk %2, %4\n  \
               ret %5\n\
               }\n";
    let (out, stats) = one_pass(src, "memopt");
    assert_eq!(stats.loads_eliminated, 1, "{stats}");
    assert_eq!(out.matches("load.i64").count(), 1, "one load left:\n{out}");
    assert!(out.contains("fact frozen"), "fact preserved (D2):\n{out}");
    insta::assert_snapshot!("witness_read_param_cse", out);
}

/// Store-to-load forwarding through the token chain, dominator-scoped.
#[test]
fn memopt_store_to_load_forwarding() {
    let src = "fn @f() -> i64 {\n\
               b0:\n  \
               %0: ptr, %1: mem.r0 = region.new\n  \
               %2 = iconst.i64 8\n  \
               %3: ptr, %4: mem.r0 = region.alloc %0, %2, %1\n  \
               %5 = iconst.i64 7\n  \
               %6 = store.i64 %5, %3, %4\n  \
               %7 = load.i64 %3, %6\n  \
               region.free %0, %6\n  \
               ret %7\n\
               }\n";
    let (out, stats) = one_pass(src, "memopt");
    assert_eq!(stats.stores_forwarded, 1, "{stats}");
    assert!(!out.contains("load.i64"), "load forwarded:\n{out}");
    insta::assert_snapshot!("memopt_forwarding", out);
}

/// HOLY-GRAIL WITNESS 2 (contract acceptance, locked): dead-store
/// elimination into a freed region. The region is function-local,
/// nothing loads it, no pointer escapes — `region.free` is
/// bulk-dead-store and the stores die.
#[test]
fn witness_dead_store_into_freed_region() {
    let src = "fn @g() -> i64 {\n\
               b0:\n  \
               %0: ptr, %1: mem.r0 = region.new\n  \
               %2 = iconst.i64 64\n  \
               %3: ptr, %4: mem.r0 = region.alloc %0, %2, %1\n  \
               %5 = iconst.i64 7\n  \
               %6 = store.i64 %5, %3, %4\n  \
               %7 = iconst.i64 8\n  \
               %8 = ptr.off %3, %7, 1\n  \
               %9 = store.i64 %7, %8, %6\n  \
               region.free %0, %9\n  \
               %10 = iconst.i64 0\n  \
               ret %10\n\
               }\n";
    let (out, stats) = one_pass(src, "memopt");
    assert_eq!(stats.dead_stores, 2, "{stats}");
    assert!(!out.contains("store.i64"), "stores dead:\n{out}");
    insta::assert_snapshot!("witness_dse_freed_region", out);
}

/// NEGATIVE: a pointer that escapes to a call keeps its stores —
/// tokenless runtime read seams could observe them.
#[test]
fn memopt_dse_respects_escape() {
    let src = "decl @sink(ptr)\n\
               \n\
               fn @g() -> i64 {\n\
               b0:\n  \
               %0: ptr, %1: mem.r0 = region.new\n  \
               %2 = iconst.i64 64\n  \
               %3: ptr, %4: mem.r0 = region.alloc %0, %2, %1\n  \
               %5 = iconst.i64 7\n  \
               %6 = store.i64 %5, %3, %4\n  \
               call @sink(%3)\n  \
               region.free %0, %6\n  \
               %8 = iconst.i64 0\n  \
               ret %8\n\
               }\n";
    let (out, stats) = one_pass(src, "memopt");
    assert_eq!(stats.dead_stores, 0, "escaped pointer: stores stay");
    assert!(out.contains("store.i64"), "{out}");
}

// ----------------------------------------------------------- sink ----

/// Amendment 1 (b3 request-churn shape): a small per-iteration region
/// promotes to the frame — `region.new`→`stack.alloc`, allocations
/// become offsets, the free disappears. The backend's entry-alloca
/// discipline reuses ONE slab per activation.
#[test]
fn sink_promotes_small_region_in_loop() {
    let src = "fn @churn(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iconst.i64 0\n  \
               jmp b1(%1)\n\
               b1(%2: i64):\n  \
               %3 = icmp.slt %2, %0\n  \
               br %3, b2, b3\n\
               b2:\n  \
               %4: ptr, %5: mem.r0 = region.new\n  \
               %6 = iconst.i64 32\n  \
               %7: ptr, %8: mem.r0 = region.alloc %4, %6, %5\n  \
               %9 = store.i64 %2, %7, %8\n  \
               region.free %4, %9\n  \
               %10 = iconst.i64 1\n  \
               %11 = iadd.chk %2, %10\n  \
               jmp b1(%11)\n\
               b3:\n  \
               ret %2\n\
               }\n";
    let (out, stats) = one_pass(src, "sink");
    assert_eq!(stats.regions_promoted, 1, "{stats}");
    assert_eq!(stats.allocs_promoted, 1, "{stats}");
    assert!(out.contains("stack.alloc"), "{out}");
    assert!(!out.contains("region.new"), "{out}");
    assert!(!out.contains("region.free"), "{out}");
    insta::assert_snapshot!("sink_b3_promotion", out);
}

/// NEGATIVE (the b1 guard): an unbounded region — non-constant
/// allocation size — never promotes; and a region whose pointer
/// reaches a call (every spawn seam is a call) never promotes.
#[test]
fn sink_rejects_unbounded_and_escaping() {
    let unbounded = "fn @b1(i64) -> i64 {\n\
                     b0(%0: i64):\n  \
                     %1: ptr, %2: mem.r0 = region.new\n  \
                     %3: ptr, %4: mem.r0 = region.alloc %1, %0, %2\n  \
                     %5 = store.i64 %0, %3, %4\n  \
                     region.free %1, %5\n  \
                     ret %0\n\
                     }\n";
    let (_, stats) = one_pass(unbounded, "sink");
    assert_eq!(stats.regions_promoted, 0, "unbounded region stays");
    let escaping = "decl @spawnish(ptr)\n\
                    \n\
                    fn @esc() -> i64 {\n\
                    b0:\n  \
                    %0: ptr, %1: mem.r0 = region.new\n  \
                    %2 = iconst.i64 32\n  \
                    %3: ptr, %4: mem.r0 = region.alloc %0, %2, %1\n  \
                    call @spawnish(%3)\n  \
                    region.free %0, %4\n  \
                    %6 = iconst.i64 0\n  \
                    ret %6\n\
                    }\n";
    let (out, stats) = one_pass(escaping, "sink");
    assert_eq!(
        stats.regions_promoted, 0,
        "escape to a call (spawn conservatism): no sinking\n{out}"
    );
}

// ------------------------------------------------------- coalesce ----

/// Amendment 2: consecutive same-region constant allocations fuse into
/// one bump; the follower becomes a 16-aligned offset.
#[test]
fn coalesce_fuses_adjacent_allocs() {
    let src = "fn @co() -> i64 {\n\
               b0:\n  \
               %0: ptr, %1: mem.r0 = region.new\n  \
               %2 = iconst.i64 16\n  \
               %3: ptr, %4: mem.r0 = region.alloc %0, %2, %1\n  \
               %5: ptr, %6: mem.r0 = region.alloc %0, %2, %4\n  \
               %7 = iconst.i64 1\n  \
               %8 = store.i64 %7, %3, %6\n  \
               %9 = store.i64 %7, %5, %8\n  \
               %10 = load.i64 %5, %9\n  \
               region.free %0, %9\n  \
               ret %10\n\
               }\n";
    let (out, stats) = one_pass(src, "coalesce");
    assert_eq!(stats.allocs_coalesced, 1, "{stats}");
    assert_eq!(out.matches("region.alloc").count(), 1, "one bump:\n{out}");
    assert!(out.contains("iconst.i64 32"), "grown leader:\n{out}");
    insta::assert_snapshot!("coalesce_two_allocs", out);
}

/// NEGATIVE (conc conservatism): a CALL between two allocations is a
/// schedule point (spec/07) — no coalescing across it, even though
/// the token chain would otherwise allow the fuse.
#[test]
fn coalesce_never_crosses_a_call() {
    let src = "decl @sched_point()\n\
               \n\
               fn @co() -> i64 {\n\
               b0:\n  \
               %0: ptr, %1: mem.r0 = region.new\n  \
               %2 = iconst.i64 16\n  \
               %3: ptr, %4: mem.r0 = region.alloc %0, %2, %1\n  \
               call @sched_point()\n  \
               %6: ptr, %7: mem.r0 = region.alloc %0, %2, %4\n  \
               %8 = store.i64 %2, %6, %7\n  \
               region.free %0, %8\n  \
               %9 = iconst.i64 0\n  \
               ret %9\n\
               }\n";
    let (out, stats) = one_pass(src, "coalesce");
    assert_eq!(stats.allocs_coalesced, 0, "schedule point is a barrier");
    assert_eq!(out.matches("region.alloc").count(), 2, "{out}");
}

// ------------------------------------------------------- rangeopt ----

/// X3's claw-back: the canonical counter's increment check is proven
/// by the loop guard's π refinement alone (i < n ⇒ i+1 ≤ MAX) plus
/// the induction lower bound; the data accumulator's check stays —
/// checked-in-release means data-dependent overflow still traps.
#[test]
fn rangeopt_counter_eliminates_accumulator_stays() {
    let src = "fn @sum(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iconst.i64 0\n  \
               jmp b1(%1, %1)\n\
               b1(%2: i64, %3: i64):\n  \
               %4 = icmp.slt %2, %0\n  \
               br %4, b2, b3\n\
               b2:\n  \
               %5 = iconst.i64 1\n  \
               %6 = iadd.chk %2, %5\n  \
               %7 = iadd.chk %3, %2\n  \
               jmp b1(%6, %7)\n\
               b3:\n  \
               ret %3\n\
               }\n";
    let (out, stats) = one_pass(src, "rangeopt");
    assert!(stats.checks_rewritten >= 1, "{stats}");
    assert!(out.contains("iadd.wrap"), "counter proven:\n{out}");
    assert!(
        out.contains("iadd.chk %"),
        "accumulator keeps its check (verdicts are sacred):\n{out}"
    );
    insta::assert_snapshot!("rangeopt_counter_loop", out);
}

/// Amendment 3, the backstop: `i * 8` against an OPAQUE bound is not
/// directly provable; the demand-driven versioner guards `n <= K`,
/// clones the loop, and the fast copy's checks die on the second
/// analysis round. Both versions verify; the slow copy keeps every
/// check. LOCKED as a snapshot — the versioned shape is the artifact.
#[test]
fn rangeopt_versioning_backstop() {
    let src = "fn @scale(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iconst.i64 0\n  \
               jmp b1(%1, %1)\n\
               b1(%2: i64, %3: i64):\n  \
               %4 = icmp.slt %2, %0\n  \
               br %4, b2, b3\n\
               b2:\n  \
               %5 = iconst.i64 8\n  \
               %6 = imul.chk %2, %5\n  \
               %7 = iconst.i64 1\n  \
               %8 = iadd.chk %2, %7\n  \
               jmp b1(%8, %6)\n\
               b3:\n  \
               ret %3\n\
               }\n";
    let (out, stats) = one_pass(src, "rangeopt");
    assert_eq!(stats.loops_versioned, 1, "{stats}");
    assert!(out.contains("imul.wrap"), "fast copy proven:\n{out}");
    assert!(
        out.contains("imul.chk"),
        "slow copy keeps the check:\n{out}"
    );
    assert_eq!(
        stats.loop_checks_eliminated, stats.loop_checks_seen,
        "the backstop closes the gap: {stats}"
    );
    insta::assert_snapshot!("rangeopt_versioned_loop", out);
}

/// s75, the relational channel: intervals cannot prove `i <u n` from
/// `i <s n` — the fact is a RELATION, and an interval domain forgets
/// relations by construction. The π seeding therefore keeps the
/// comparison itself, and the guard over the same operand pair
/// decides the bounds test, trap arm and all. Both operands are
/// provably non-negative at the query block (`i` is an induction
/// variable from 0; the guard's own refinement puts `n` above it), so
/// crossing from the signed to the unsigned ordering is sound.
#[test]
fn rangeopt_relation_proves_the_unsigned_bounds_test() {
    let src = "fn @walk(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iconst.i64 0\n  \
               jmp b1(%1, %1)\n\
               b1(%2: i64, %3: i64):\n  \
               %4 = icmp.slt %2, %0\n  \
               br %4, b2, b5\n\
               b2:\n  \
               %5 = icmp.ult %2, %0\n  \
               br %5, b3, b4\n\
               b3:\n  \
               %6 = iconst.i64 1\n  \
               %7 = iadd.chk %2, %6\n  \
               jmp b1(%7, %3)\n\
               b4:\n  \
               trap.bounds\n\
               b5:\n  \
               ret %3\n\
               }\n";
    let (out, stats) = one_pass(src, "rangeopt");
    assert_eq!(stats.bounds_checks_seen, 1, "{stats}");
    assert_eq!(stats.bounds_checks_eliminated, 1, "{stats}");
    assert!(
        !out.contains("trap.bounds"),
        "the proven guard takes its trap arm with it:\n{out}"
    );
}

/// The other half of the same rule, and the one that matters more: a
/// bounds test nothing relates to the guard STAYS. Here the loop runs
/// to `%0` and the index is tested against a DIFFERENT bound `%1`.
#[test]
fn rangeopt_keeps_a_bounds_test_over_an_unrelated_bound() {
    let src = "fn @walk(i64, i64) -> i64 {\n\
               b0(%0: i64, %1: i64):\n  \
               %2 = iconst.i64 0\n  \
               jmp b1(%2, %2)\n\
               b1(%3: i64, %4: i64):\n  \
               %5 = icmp.slt %3, %0\n  \
               br %5, b2, b5\n\
               b2:\n  \
               %6 = icmp.ult %3, %1\n  \
               br %6, b3, b4\n\
               b3:\n  \
               %7 = iconst.i64 1\n  \
               %8 = iadd.chk %3, %7\n  \
               jmp b1(%8, %4)\n\
               b4:\n  \
               trap.bounds\n\
               b5:\n  \
               ret %4\n\
               }\n";
    let (out, stats) = one_pass(src, "rangeopt");
    assert_eq!(stats.bounds_checks_seen, 1, "{stats}");
    assert_eq!(
        stats.bounds_checks_eliminated, 0,
        "an unrelated bound proves nothing: {stats}"
    );
    assert!(
        out.contains("trap.bounds"),
        "the unproven guard keeps its trap:\n{out}"
    );
}

/// A loop containing a CALL never versions (schedule points, spec/07).
#[test]
fn rangeopt_versioning_skips_loops_with_calls() {
    let src = "decl @tick()\n\
               \n\
               fn @scale(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iconst.i64 0\n  \
               jmp b1(%1, %1)\n\
               b1(%2: i64, %3: i64):\n  \
               %4 = icmp.slt %2, %0\n  \
               br %4, b2, b3\n\
               b2:\n  \
               call @tick()\n  \
               %6 = iconst.i64 8\n  \
               %7 = imul.chk %2, %6\n  \
               %8 = iconst.i64 1\n  \
               %9 = iadd.chk %2, %8\n  \
               jmp b1(%9, %7)\n\
               b3:\n  \
               ret %3\n\
               }\n";
    let (_, stats) = one_pass(src, "rangeopt");
    assert_eq!(stats.loops_versioned, 0, "calls bar versioning: {stats}");
}

// ----------------------------------------------------------- licm ----

/// A frozen-region load (token defined outside the loop, never
/// invalidated) hoists to the preheader; safe-tier loads cannot trap,
/// so the speculation is free.
#[test]
fn licm_hoists_frozen_load() {
    let src = "fn @f(i64) -> i64 {\n  \
               fact frozen %1 : op %3\n\
               b0(%0: i64):\n  \
               %1: ptr, %2: mem.r0 = region.new\n  \
               %3: mem.r0 = sync.freeze %1, %2\n  \
               %4 = iconst.i64 0\n  \
               jmp b1(%4, %4)\n\
               b1(%5: i64, %6: i64):\n  \
               %7 = icmp.slt %5, %0\n  \
               br %7, b2, b3\n\
               b2:\n  \
               %8 = load.i64 %1, %3\n  \
               %9 = iadd.wrap %6, %8\n  \
               %10 = iconst.i64 1\n  \
               %11 = iadd.wrap %5, %10\n  \
               jmp b1(%11, %9)\n\
               b3:\n  \
               ret %6\n\
               }\n";
    let (out, stats) = one_pass(src, "licm");
    assert_eq!(stats.loads_hoisted, 1, "{stats}");
    insta::assert_snapshot!("licm_frozen_load", out);
}

// -------------------------------------------- fact custody (D2) ----

/// The pipeline preserves facts it has no licence to touch: entry
/// facts survive the full pipeline bit-for-bit (modulo renumbering).
#[test]
fn pipeline_preserves_entry_facts() {
    let src = "fn @f(mut ptr, read ptr) -> i64 {\n  \
               fact noalias %0 %1 : excl.mut\n  \
               fact frozen %1 : frozen.read\n\
               b0(%0: ptr, %1: ptr):\n  \
               %2 = iconst.i64 0\n  \
               ret %2\n\
               }\n";
    let (out, _) = pipeline(src);
    assert!(out.contains("fact noalias"), "{out}");
    assert!(out.contains("fact frozen"), "{out}");
}
