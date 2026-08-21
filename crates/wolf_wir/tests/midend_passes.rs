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
/// the induction lower bound.
///
/// The data accumulator is the s85 case. `acc = acc + i` cannot be
/// proven outright — `n` is opaque, so nothing bounds the sum — and
/// before s85 the check simply stayed. Now the reduction versions:
/// `n <= 2^31-1` is tested ONCE outside, and under it the trip-scaled
/// bound puts the accumulator inside `0..=n(n-1)`, which is inside
/// i64. The fast copy is not unchecked arithmetic; it is arithmetic
/// whose check was discharged by the guard above it, and the slow
/// copy — the one a larger `n` actually runs — keeps every check.
/// Both halves are in the snapshot for exactly that reason.
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
    assert_eq!(stats.loops_versioned, 1, "the reduction versions: {stats}");
    assert!(
        out.contains("iadd.chk %"),
        "the slow copy keeps its check (verdicts are sacred):\n{out}"
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

/// The UTF-8 continuation probe's interval shape (d2, the s99
/// follow-on diagnosis): `x & 192` with `x` proven non-negative and
/// bounded BELOW the probe constant decides `(x & 192) != 128` — AND
/// cannot increase a non-negative operand, so the result is bounded
/// by `min(hi(x), mask)`, and [0,127] is disjoint from 128 where the
/// mask-only bound [0,192] is not. The bounded source here is itself
/// a mask (`%p & 127`); a byte load with a minted range fact is the
/// same shape.
#[test]
fn rangeopt_band_carries_the_operand_bound() {
    let src = "fn @probe(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iconst.i64 127\n  \
               %2 = band %0, %1\n  \
               %3 = iconst.i64 192\n  \
               %4 = band %2, %3\n  \
               %5 = iconst.i64 128\n  \
               %6 = icmp.ne %4, %5\n  \
               br %6, b1, b2\n\
               b1:\n  \
               %7 = iconst.i64 1\n  \
               ret %7\n\
               b2:\n  \
               %8 = iconst.i64 0\n  \
               ret %8\n\
               }\n";
    let (out, _stats) = one_pass(src, "rangeopt");
    assert!(
        !out.contains("br "),
        "the probe is decided — [0,127] & 192 cannot be 128:\n{out}"
    );
}

/// The other half of the same rule, kept honest across #98: a bounds
/// test nothing relates to the guard is never folded IN PLACE. Here
/// the loop runs to `%0` and the index is tested against a DIFFERENT
/// bound `%1` — since #98 the versioner may ASK for the missing
/// relation at the loop's door (`0 <= %1` and `%0 <= %1` as real
/// guards) and fold the FAST copy's check under them; the slow copy
/// keeps the check and the trap, which is what "proves nothing"
/// still means: nothing is proven unguarded.
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
    // The versioner asked for the relation and got it — the fast twin
    // folds, and the metric counts the hot path's win exactly as the
    // check metric does.
    assert_eq!(
        stats.bounds_checks_eliminated, 1,
        "the guarded fast copy folds its check: {stats}"
    );
    // The teeth: the fold happened UNDER materialized guards, never in
    // place. Both synthesized comparisons are real branches, the slow
    // copy still tests the index against %1, and the trap is intact.
    assert!(
        out.contains("icmp.sle %0, %1"),
        "the relation guard is a real branch:\n{out}"
    );
    assert!(
        out.contains("icmp.ult"),
        "the slow copy keeps the bounds test:\n{out}"
    );
    assert!(
        out.contains("trap.bounds"),
        "the unproven path keeps its trap:\n{out}"
    );
}

/// s78, the AFFINE half of the relational channel — `a2_stencil1d`'s
/// exact shape, which the same-pair rule could not touch: the guard is
/// `i < n - 1` and the three indices are `i - 1`, `i`, `i + 1`. Each
/// query decomposes to the same base pair as the guard, so the
/// difference interval decides all three, trap arms and all. Both
/// forms of the offset are here on purpose: `isub.wrap` (admitted
/// because the interval channel proves it cannot wrap — this is the
/// form the pass's own first round mints) and `iadd.chk` (admitted
/// because a trap already ruled the wrap out).
///
/// Note what carries the unsigned indices: nothing bounds the opaque
/// length `%0` below on its own, so crossing from the guard's signed
/// ordering to the checks' unsigned one comes from refining `n - 1`
/// BACKWARD onto `n` at π seeding time.
#[test]
fn rangeopt_affine_relation_proves_offset_indices() {
    let src = "fn @stencil(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iconst.i64 1\n  \
               %2 = isub.chk %0, %1\n  \
               jmp b1(%1, %1)\n\
               b1(%3: i64, %4: i64):\n  \
               %5 = icmp.slt %3, %2\n  \
               br %5, b2, b9\n\
               b2:\n  \
               %6 = isub.wrap %3, %1\n  \
               %7 = icmp.ult %6, %0\n  \
               br %7, b3, b6\n\
               b3:\n  \
               %8 = icmp.ult %3, %0\n  \
               br %8, b4, b7\n\
               b4:\n  \
               %9 = iadd.chk %3, %1\n  \
               %10 = icmp.ult %9, %0\n  \
               br %10, b5, b8\n\
               b5:\n  \
               jmp b1(%9, %4)\n\
               b6:\n  \
               trap.bounds\n\
               b7:\n  \
               trap.bounds\n\
               b8:\n  \
               trap.bounds\n\
               b9:\n  \
               ret %4\n\
               }\n";
    let (out, stats) = one_pass(src, "rangeopt");
    assert_eq!(stats.bounds_checks_seen, 3, "{stats}");
    assert_eq!(
        stats.bounds_checks_eliminated, 3,
        "all three affine offsets of the guarded pair: {stats}"
    );
    assert!(
        !out.contains("trap.bounds"),
        "the proven guards take their trap arms with them:\n{out}"
    );
}

/// The other half, and the one the sprint's risk posture cares about:
/// an offset whose arithmetic is NOT proven wrap-free is not an affine
/// offset, and the check stays. `%7 = iadd.wrap %4, 1` sits under a
/// guard that bounds `i` from BELOW only, so nothing rules out
/// `i == i64::MAX`, where `i + 1` is not `i + 1` — and a decomposition
/// that is not an identity would decide the comparison wrongly.
/// [`rangeopt_affine_wrap_admitted_when_bounded`] is the same body with
/// the check form, which the channel does prove.
#[test]
fn rangeopt_affine_refuses_an_unproven_wrap() {
    let (out, stats) = one_pass(&affine_wrap_src("iadd.wrap"), "rangeopt");
    assert_eq!(stats.bounds_checks_seen, 1, "{stats}");
    assert_eq!(
        stats.bounds_checks_eliminated, 0,
        "an unbounded wrap is not an affine offset: {stats}"
    );
    assert!(
        out.contains("trap.bounds"),
        "the unproven guard keeps its trap:\n{out}"
    );
}

#[test]
fn rangeopt_affine_wrap_admitted_when_bounded() {
    let (out, stats) = one_pass(&affine_wrap_src("iadd.chk"), "rangeopt");
    assert_eq!(stats.bounds_checks_seen, 1, "{stats}");
    assert_eq!(
        stats.bounds_checks_eliminated, 1,
        "a checked offset carries its own no-wrap proof: {stats}"
    );
    assert!(!out.contains("trap.bounds"), "{out}");
}

/// `i > n - 1` guards the loop (a LOWER bound on `i`, so `i + 1` is
/// only an affine offset when the op itself rules the wrap out), and
/// `i + 1 > n` follows from it.
fn affine_wrap_src(op: &str) -> String {
    format!(
        "fn @above(i64) -> i64 {{\n\
         b0(%0: i64):\n  \
         %1 = iconst.i64 1\n  \
         %2 = isub.chk %0, %1\n  \
         %3 = iconst.i64 0\n  \
         jmp b1(%3, %3)\n\
         b1(%4: i64, %5: i64):\n  \
         %6 = icmp.sgt %4, %2\n  \
         br %6, b2, b5\n\
         b2:\n  \
         %7 = {op} %4, %1\n  \
         %8 = icmp.sgt %7, %0\n  \
         br %8, b3, b4\n\
         b3:\n  \
         %9 = iadd.chk %4, %1\n  \
         jmp b1(%9, %5)\n\
         b4:\n  \
         trap.bounds\n\
         b5:\n  \
         ret %5\n\
         }}\n"
    )
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

/// s85, the trip-scaled accumulator, on the shape D44 named: `acc =
/// acc + (i & 1023)` over a loop whose bound is a CONSTANT. Nothing
/// about the accumulator is constant — it is not an induction variable
/// with a fixed step, which is why every earlier round left its check
/// alone — but the increment is bounded by the mask and the iteration
/// count is bounded by the guard, and the product of the two is a
/// bound on the sum. No guard, no clone, no second copy of the loop:
/// the ranges simply close.
#[test]
fn rangeopt_trip_scaled_accumulator_needs_no_guard() {
    let src = "fn @sum() -> i64 {\n\
               b0:\n  \
               %0 = iconst.i64 0\n  \
               jmp b1(%0, %0)\n\
               b1(%1: i64, %2: i64):\n  \
               %3 = iconst.i64 100000\n  \
               %4 = icmp.slt %1, %3\n  \
               br %4, b2, b3\n\
               b2:\n  \
               %5 = iconst.i64 1023\n  \
               %6 = band %1, %5\n  \
               %7 = iadd.chk %2, %6\n  \
               %8 = iconst.i64 1\n  \
               %9 = iadd.chk %1, %8\n  \
               jmp b1(%9, %7)\n\
               b3:\n  \
               ret %2\n\
               }\n";
    let (out, stats) = one_pass(src, "rangeopt");
    assert_eq!(stats.loops_versioned, 0, "no guard is needed: {stats}");
    assert!(
        !out.contains("iadd.chk"),
        "100000 iterations of at most 1023 each is 102_300_000, and that fits:\n{out}"
    );
    assert_eq!(
        stats.loop_checks_eliminated, stats.loop_checks_seen,
        "{stats}"
    );
}

/// The same reduction with an OPAQUE bound. Nothing bounds the trip
/// count, so nothing bounds the sum, so the direct round proves
/// nothing — and this is where the versioning client earns its place:
/// it hoists `n <= K` above the loop and the fast copy's checks fall
/// to the same trip-scaled rule under the guard. The slow copy still
/// carries every check, which is the distinction that matters: the
/// fast body is not unchecked arithmetic, it is arithmetic whose check
/// was discharged once, outside.
#[test]
fn rangeopt_versions_a_reduction_against_an_opaque_bound() {
    let src = "fn @sum(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iconst.i64 0\n  \
               jmp b1(%1, %1)\n\
               b1(%2: i64, %3: i64):\n  \
               %4 = icmp.slt %2, %0\n  \
               br %4, b2, b3\n\
               b2:\n  \
               %5 = iconst.i64 1023\n  \
               %6 = band %2, %5\n  \
               %7 = iadd.chk %3, %6\n  \
               %8 = iconst.i64 1\n  \
               %9 = iadd.chk %2, %8\n  \
               jmp b1(%9, %7)\n\
               b3:\n  \
               ret %3\n\
               }\n";
    let (out, stats) = one_pass(src, "rangeopt");
    assert_eq!(stats.loops_versioned, 1, "{stats}");
    assert!(out.contains("iadd.wrap"), "fast copy discharged:\n{out}");
    assert!(
        out.contains("iadd.chk"),
        "slow copy keeps every check:\n{out}"
    );
}

/// The monotonicity rule is CHECKED-only, and this is the litmus.
/// `%6 = iadd.wrap %2, 1` is user `wrapping[T]` arithmetic as far as
/// this pass can tell — it carries no trap, so "the counter never
/// descends below its entry value" is not a theorem about it. Here the
/// loop exits on `!=`, which pins no interval at all, so nothing rules
/// out the counter reaching `i64::MAX` and wrapping to `i64::MIN` on
/// the next step; the trip-scaled rule refuses for exactly that reason
/// (its wrap admission needs the step proven representable) and the
/// `i < 0` guard keeps its trap.
///
/// This is a soundness regression test, not a precision one: before
/// s85 the wrap form was accepted as monotone on the stated grounds
/// that this pass only mints wrap forms it has already proven — true
/// of the ones it mints, false of the ones a `wrapping[T]` program
/// hands it, and the difference is a trap that would go missing.
#[test]
fn rangeopt_refuses_monotonicity_from_a_wrapping_counter() {
    let src = "fn @f(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iconst.i64 0\n  \
               jmp b1(%1)\n\
               b1(%2: i64):\n  \
               %3 = icmp.ne %2, %0\n  \
               br %3, b2, b5\n\
               b2:\n  \
               %4 = iconst.i64 0\n  \
               %5 = icmp.slt %2, %4\n  \
               br %5, b3, b4\n\
               b3:\n  \
               trap.bounds\n\
               b4:\n  \
               %7 = iconst.i64 1\n  \
               %6 = iadd.wrap %2, %7\n  \
               jmp b1(%6)\n\
               b5:\n  \
               ret %2\n\
               }\n";
    let (out, stats) = one_pass(src, "rangeopt");
    assert_eq!(
        stats.bounds_checks_eliminated, 0,
        "a wrapping counter with no upper guard is not monotone: {stats}"
    );
    assert!(out.contains("trap.bounds"), "the guard stays:\n{out}");
}

/// The other side of the same rule, so the litmus above is not just
/// the analysis being blind: give the SAME wrapping counter an upper
/// guard and the step is provably representable (`i < n` ⇒ `i + 1 ≤
/// n ≤ MAX`), the trip bound closes, and the counter's floor comes
/// back — this time as a proof rather than as a promise.
#[test]
fn rangeopt_wrapping_counter_under_a_guard_is_bounded() {
    let src = "fn @f(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iconst.i64 0\n  \
               jmp b1(%1)\n\
               b1(%2: i64):\n  \
               %3 = icmp.slt %2, %0\n  \
               br %3, b2, b5\n\
               b2:\n  \
               %4 = iconst.i64 0\n  \
               %5 = icmp.slt %2, %4\n  \
               br %5, b3, b4\n\
               b3:\n  \
               trap.bounds\n\
               b4:\n  \
               %7 = iconst.i64 1\n  \
               %6 = iadd.wrap %2, %7\n  \
               jmp b1(%6)\n\
               b5:\n  \
               ret %2\n\
               }\n";
    let (out, _) = one_pass(src, "rangeopt");
    assert!(
        !out.contains("trap.bounds"),
        "the guarded counter is provably non-negative:\n{out}"
    );
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

// ------------------------------------------------ licm licensor (s102) --

/// The a5 shape, reduced: a loop whose CALL stores only through its
/// `excl.mut` parameter's object graph, while the loop's loads chain
/// to a DIFFERENT entry parameter — the c04 disjointness theorem (G7:
/// "a checker theorem nobody spends") licenses the hoist past both
/// conservatism rules.
fn licensor_src(callee_arg: &str, fact_line: &str) -> String {
    format!(
        "fn @sink(mut ptr, mem.r0, i64) -> i64 {{\n  \
         fact deref %0 8 : excl.mut\n\
         b0(%0: ptr, %1: mem.r0, %2: i64):\n  \
         %3: mem.r2 = region.foreign 1\n  \
         %4: mem.r1 = region.foreign 0\n  \
         %5 = load.ptr %0, %1\n  \
         %6 = load.ptr %5, %4\n  \
         %7 = iconst.i64 0\n  \
         %8 = ptr.off %6, %7, 8\n  \
         %9 = store.i64 %2, %8, %3\n  \
         ret %2\n\
         }}\n\
         fn @spin(ptr, mut ptr, mem.r0, i64) -> i64 {{\n  \
         {fact_line}\n\
         b0(%0: ptr, %1: ptr, %2: mem.r0, %3: i64):\n  \
         %4: mem.r2 = region.foreign 1\n  \
         %5: mem.r1 = region.foreign 0\n  \
         %6 = iconst.i64 0\n  \
         jmp b1(%6, %2, %6)\n\
         b1(%7: i64, %8: mem.r0, %9: i64):\n  \
         %10 = icmp.slt %7, %3\n  \
         br %10, b2, b3\n\
         b2:\n  \
         %11 = load.ptr %0, %5\n  \
         %12 = ptr.off %11, %6, 8\n  \
         %13 = load.i64 %12, %4\n  \
         %14, %15 = call @sink({callee_arg}, %8, %13)\n  \
         %16 = iadd.wrap %9, %13\n  \
         %17 = iconst.i64 1\n  \
         %18 = iadd.wrap %7, %17\n  \
         jmp b1(%18, %15, %16)\n\
         b3:\n  \
         ret %9\n\
         }}\n"
    )
}

#[test]
fn licm_licenses_a_load_past_a_disjoint_writing_callee() {
    let src = licensor_src("%1", "fact deref %1 8 : excl.mut");
    let (out, stats) = one_pass(&src, "licm");
    assert!(
        stats.loads_hoisted >= 2,
        "the data-ptr and element loads hoist past the call: {stats}\n{out}"
    );
    insta::assert_snapshot!("licm_licensed_disjoint_callee", out);
}

/// The negative the theorem demands: the callee writes through the
/// SAME chain the load reads (no exclusivity proof separates them) —
/// nothing hoists, however invariant the load looks.
#[test]
fn licm_refuses_the_license_on_a_shared_root() {
    let src = licensor_src("%0", "fact deref %1 8 : excl.mut");
    let (_, stats) = one_pass(&src, "licm");
    assert_eq!(
        stats.loads_hoisted, 0,
        "a writer through the load's own root licenses nothing: {stats}"
    );
}

/// No exclusivity fact, no license: the same disjoint-looking shape
/// WITHOUT `excl.mut` on the written param keeps blocking (the
/// D44-addendum rule — no fact, no proof, no hoist).
#[test]
fn licm_refuses_the_license_without_the_theorem() {
    let src = licensor_src("%1", "fact deref %1 8 : frozen.read");
    let (_, stats) = one_pass(&src, "licm");
    assert_eq!(
        stats.loads_hoisted, 0,
        "no excl.mut fact on the written param, no license: {stats}"
    );
}

/// An EXTERNAL callee has no write set — Unknown blocks everything,
/// exactly as before s102.
#[test]
fn licm_refuses_the_license_for_an_external_callee() {
    let src = "decl @mystery(mut ptr, mem.r0, i64) -> i64\n\
               fn @spin(ptr, mut ptr, mem.r0, i64) -> i64 {\n  \
               fact deref %1 8 : excl.mut\n\
               b0(%0: ptr, %1: ptr, %2: mem.r0, %3: i64):\n  \
               %4: mem.r2 = region.foreign 1\n  \
               %5: mem.r1 = region.foreign 0\n  \
               %6 = iconst.i64 0\n  \
               jmp b1(%6, %2, %6)\n\
               b1(%7: i64, %8: mem.r0, %9: i64):\n  \
               %10 = icmp.slt %7, %3\n  \
               br %10, b2, b3\n\
               b2:\n  \
               %11 = load.ptr %0, %5\n  \
               %12 = ptr.off %11, %6, 8\n  \
               %13 = load.i64 %12, %4\n  \
               %14, %15 = call @mystery(%1, %8, %13)\n  \
               %16 = iadd.wrap %9, %13\n  \
               %17 = iconst.i64 1\n  \
               %18 = iadd.wrap %7, %17\n  \
               jmp b1(%18, %15, %16)\n\
               b3:\n  \
               ret %9\n\
               }\n";
    let (_, stats) = one_pass(src, "licm");
    assert_eq!(
        stats.loads_hoisted, 0,
        "an external callee's writes are unbounded: {stats}"
    );
}

// ---------------------------------------------- loop-region CSE (s102) --

/// Two identical sequential pure loops: the second merges onto the
/// first (e3's mechanism at WIR level).
fn twin_loops_src(second_entry: &str, second_body_op: &str) -> String {
    format!(
        "fn @twice(i64) -> i64 {{\n\
         b0(%0: i64):\n  \
         %1 = iconst.i64 0\n  \
         %2 = iconst.i64 8\n  \
         %3 = iconst.i64 1\n  \
         jmp b1(%1, %1)\n\
         b1(%4: i64, %5: i64):\n  \
         %6 = icmp.slt %4, %0\n  \
         br %6, b2, b3\n\
         b2:\n  \
         %7 = imul.wrap %4, %2\n  \
         %8 = iadd.wrap %5, %7\n  \
         %9 = iadd.wrap %4, %3\n  \
         jmp b1(%9, %8)\n\
         b3:\n  \
         jmp b4({second_entry})\n\
         b4(%10: i64, %11: i64):\n  \
         %12 = icmp.slt %10, %0\n  \
         br %12, b5, b6\n\
         b5:\n  \
         %13 = {second_body_op} %10, %2\n  \
         %14 = iadd.wrap %11, %13\n  \
         %15 = iadd.wrap %10, %3\n  \
         jmp b4(%15, %14)\n\
         b6:\n  \
         %16 = iadd.wrap %5, %11\n  \
         ret %16\n\
         }}\n"
    )
}

#[test]
fn loopcse_merges_identical_sequential_loops() {
    let src = twin_loops_src("%1, %1", "imul.wrap");
    let (out, stats) = one_pass(&src, "loopcse");
    assert_eq!(stats.loops_cse, 1, "the twin merges: {stats}\n{out}");
    insta::assert_snapshot!("loopcse_merged_twins", out);
}

/// Different body op — no merge, however alike the rest looks.
#[test]
fn loopcse_refuses_a_differing_body() {
    let src = twin_loops_src("%1, %1", "iadd.wrap");
    let (_, stats) = one_pass(&src, "loopcse");
    assert_eq!(stats.loops_cse, 0, "different ops never merge: {stats}");
}

/// Different entry values — no merge (the exit values would differ).
#[test]
fn loopcse_refuses_differing_entry_args() {
    let src = twin_loops_src("%3, %1", "imul.wrap");
    let (_, stats) = one_pass(&src, "loopcse");
    assert_eq!(stats.loops_cse, 0, "different entries never merge: {stats}");
}

/// Branch-arm ALTERNATIVES (the versioner's fast/slow twins) never
/// merge: neither loop's exit dominates the other's preheader.
#[test]
fn loopcse_refuses_versioned_alternatives() {
    let src = "fn @alt(i64, i64) -> i64 {\n\
               b0(%0: i64, %1: i64):\n  \
               %2 = iconst.i64 0\n  \
               %3 = iconst.i64 8\n  \
               %4 = iconst.i64 1\n  \
               %5 = icmp.slt %2, %1\n  \
               br %5, b1, b4\n\
               b1:\n  \
               jmp b2(%2, %2)\n\
               b2(%6: i64, %7: i64):\n  \
               %8 = icmp.slt %6, %0\n  \
               br %8, b3, b7\n\
               b3:\n  \
               %9 = imul.wrap %6, %3\n  \
               %10 = iadd.wrap %7, %9\n  \
               %11 = iadd.wrap %6, %4\n  \
               jmp b2(%11, %10)\n\
               b4:\n  \
               jmp b5(%2, %2)\n\
               b5(%12: i64, %13: i64):\n  \
               %14 = icmp.slt %12, %0\n  \
               br %14, b6, b8\n\
               b6:\n  \
               %15 = imul.wrap %12, %3\n  \
               %16 = iadd.wrap %13, %15\n  \
               %17 = iadd.wrap %12, %4\n  \
               jmp b5(%17, %16)\n\
               b7:\n  \
               ret %7\n\
               b8:\n  \
               ret %13\n\
               }\n";
    let (_, stats) = one_pass(src, "loopcse");
    assert_eq!(
        stats.loops_cse, 0,
        "alternatives in sibling arms never merge: {stats}"
    );
}

/// A loop that READS memory never merges: the loads could observe
/// different bytes on the two executions.
#[test]
fn loopcse_refuses_a_loop_that_loads() {
    let src = "fn @rd(ptr, mem.r0, i64) -> i64 {\n\
               b0(%0: ptr, %1: mem.r0, %2: i64):\n  \
               %3 = iconst.i64 0\n  \
               %4 = iconst.i64 1\n  \
               jmp b1(%3, %3)\n\
               b1(%5: i64, %6: i64):\n  \
               %7 = icmp.slt %5, %2\n  \
               br %7, b2, b3\n\
               b2:\n  \
               %8 = load.i64 %0, %1\n  \
               %9 = iadd.wrap %6, %8\n  \
               %10 = iadd.wrap %5, %4\n  \
               jmp b1(%10, %9)\n\
               b3:\n  \
               jmp b4(%3, %3)\n\
               b4(%11: i64, %12: i64):\n  \
               %13 = icmp.slt %11, %2\n  \
               br %13, b5, b6\n\
               b5:\n  \
               %14 = load.i64 %0, %1\n  \
               %15 = iadd.wrap %12, %14\n  \
               %16 = iadd.wrap %11, %4\n  \
               jmp b4(%16, %15)\n\
               b6:\n  \
               %17 = iadd.wrap %6, %12\n  \
               ret %17\n\
               }\n";
    let (_, stats) = one_pass(src, "loopcse");
    assert_eq!(stats.loops_cse, 0, "a loading loop never merges: {stats}");
}

// ---------------------------------- rewrite relations (s102, d2) --

/// A just-proven `iadd.chk x, 5` IS the relation `sum >= x` — the
/// branch folder spends it on d2's skeleton shape `x <= x+5`, which
/// intervals alone cannot decide when the loop bound is unknown.
/// The guard chain gives x a range ([0, %0] via the entry guard), so
/// the chk proves; the ule then folds off the recorded relation.
fn rewrite_relation_src(add_op: &str) -> String {
    format!(
        "fn @skel(i64, i64) -> i64 {{\n\
         b0(%0: i64, %1: i64):\n  \
         %2 = iconst.i64 0\n  \
         %3 = iconst.i64 5\n  \
         %4 = iconst.i64 100000\n  \
         %5 = icmp.sle %0, %4\n  \
         br %5, b1, b6\n\
         b1:\n  \
         jmp b2(%2, %2)\n\
         b2(%6: i64, %7: i64):\n  \
         %8 = icmp.slt %6, %0\n  \
         br %8, b3, b5\n\
         b3:\n  \
         %9 = {add_op} %6, %3\n  \
         %10 = icmp.ule %6, %9\n  \
         br %10, b4, b7\n\
         b4:\n  \
         %11 = iadd.wrap %7, %9\n  \
         %12 = iconst.i64 1\n  \
         %13 = iadd.wrap %6, %12\n  \
         jmp b2(%13, %11)\n\
         b5:\n  \
         ret %7\n\
         b6:\n  \
         ret %2\n\
         b7:\n  \
         trap.bounds\n\
         }}\n"
    )
}

#[test]
fn rangeopt_spends_a_proven_check_as_a_relation() {
    let (out, stats) = one_pass(&rewrite_relation_src("iadd.chk"), "rangeopt");
    assert!(stats.checks_rewritten >= 1, "the chk proves: {stats}");
    assert!(
        !out.contains("br %10") && !out.contains("trap.bounds"),
        "the skeleton BRANCH folds off the recorded relation (the dead \
         icmp is simplify's DCE, not rangeopt's):\n{out}"
    );
}

/// The D44-addendum negative: a USER wrap op minted the same opcode
/// and proves nothing — the comparison must NOT fold from a wrap form
/// no check-elimination established.
#[test]
fn rangeopt_never_spends_a_user_wrap_as_a_relation() {
    let (out, _stats) = one_pass(&rewrite_relation_src("iadd.wrap"), "rangeopt");
    assert!(
        out.contains("br %10") && out.contains("trap.bounds"),
        "a user wrap op establishes no order — the branch stays:\n{out}"
    );
}
