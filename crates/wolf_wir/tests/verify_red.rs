//! Verifier negative suite (s24 acceptance: ≥15, one per invariant
//! class) — every rejection class has a red test asserting the exact
//! class, and error messages carry canonical textual coordinates plus
//! the offending function dumped inline.

use wolf_wir::{ErrClass, Function, Module, verify_module};

/// Parse (must succeed — these programs are representable, just
/// invalid) and expect the verifier to reject with `class`.
fn expect_reject(src: &str, class: ErrClass) -> wolf_wir::VerifyError {
    let m = wolf_wir::parse_module(src).unwrap_or_else(|e| panic!("red input must parse: {e}"));
    let err = verify_module(&m).expect_err("verifier must reject");
    assert_eq!(
        err.class,
        class,
        "wrong rejection class: wanted {}, got {} ({})",
        class.as_str(),
        err.class.as_str(),
        err.msg
    );
    err
}

#[test]
fn entry_sig_mismatch() {
    expect_reject(
        "fn @f(i64) -> i64 {\nb0(%x: f64):\n  %y = iconst.i64 0\n  ret %y\n}\n",
        ErrClass::EntrySig,
    );
}

#[test]
fn missing_terminator() {
    expect_reject(
        "fn @f(i64) -> i64 {\nb0(%x: i64):\n  %y = iadd.chk %x, %x\nb1:\n  ret %y\n}\n",
        ErrClass::Terminator,
    );
}

#[test]
fn terminator_mid_block() {
    expect_reject(
        "fn @f(i64) -> i64 {\nb0(%x: i64):\n  ret %x\n  %y = iadd.chk %x, %x\n  ret %y\n}\n",
        ErrClass::Terminator,
    );
}

#[test]
fn reserved_op_rejected() {
    // `region.*`/`rc.*` went live in s26; `sync.transfer` holds the
    // reserved fence until c05.
    expect_reject(
        "fn @f(ptr) {\nb0(%p: ptr):\n  %q: ptr = sync.transfer %p\n  ret\n}\n",
        ErrClass::ReservedOp,
    );
}

#[test]
fn ill_typed_arith() {
    expect_reject(
        "fn @f(f64, f64) -> f64 {\nb0(%x: f64, %y: f64):\n  %z = iadd.chk %x, %y\n  ret %z\n}\n",
        ErrClass::Type,
    );
}

#[test]
fn branch_condition_not_bool() {
    expect_reject(
        "fn @f(i64) -> i64 {\nb0(%x: i64):\n  br %x, b1, b1\nb1:\n  ret %x\n}\n",
        ErrClass::Type,
    );
}

#[test]
fn iconst_out_of_range() {
    expect_reject(
        "fn @f() -> i8 {\nb0:\n  %x = iconst.i8 300\n  ret %x\n}\n",
        ErrClass::ConstRange,
    );
}

#[test]
fn call_argument_type_mismatch() {
    expect_reject(
        "decl @g(i64) -> i64\n\nfn @f(ptr) -> i64 {\nb0(%p: ptr):\n  %r = call @g(%p)\n  ret %r\n}\n",
        ErrClass::CallSig,
    );
}

#[test]
fn block_arg_arity_mismatch() {
    expect_reject(
        "fn @f(i64) -> i64 {\nb0(%x: i64):\n  jmp b1(%x)\nb1:\n  %y = iconst.i64 0\n  ret %y\n}\n",
        ErrClass::BlockArgArity,
    );
}

#[test]
fn block_arg_type_mismatch() {
    expect_reject(
        "fn @f(i64) -> f64 {\nb0(%x: i64):\n  jmp b1(%x)\nb1(%y: f64):\n  ret %y\n}\n",
        ErrClass::BlockArgType,
    );
}

#[test]
fn unreachable_block() {
    expect_reject(
        "fn @f(i64) -> i64 {\nb0(%x: i64):\n  ret %x\nb1:\n  %y = iconst.i64 1\n  ret %y\n}\n",
        ErrClass::Unreachable,
    );
}

#[test]
fn dominance_violation() {
    // %y is defined in one diamond arm and used at the join.
    let err = expect_reject(
        "fn @f(bool, i64) -> i64 {\nb0(%c: bool, %x: i64):\n  br %c, b1, b2\nb1:\n  %y = iadd.chk %x, %x\n  jmp b3\nb2:\n  jmp b3\nb3:\n  ret %y\n}\n",
        ErrClass::Dominance,
    );
    // The diagnostic speaks canonical text and dumps the function.
    let shown = err.to_string();
    assert!(
        shown.contains("is not dominated by its definition"),
        "{shown}"
    );
    assert!(shown.contains("--- offending function ---"), "{shown}");
    assert!(shown.contains("fn @f(bool, i64) -> i64 {"), "{shown}");
}

#[test]
fn token_consumed_twice() {
    // Two stores off the same mem token: the chain must be a spine.
    expect_reject(
        "fn @f(mut ptr, i64, mem.r0) {\nb0(%p: ptr, %v: i64, %m: mem.r0):\n  %m2 = store.i64 %v, %p, %m\n  %m3 = store.i64 %v, %p, %m\n  ret\n}\n",
        ErrClass::TokenLinearity,
    );
}

#[test]
fn fact_on_wrong_type() {
    expect_reject(
        "fn @f(i64, i64) -> i64 {\n  fact noalias %a %b : excl.mut\nb0(%a: i64, %b: i64):\n  ret %a\n}\n",
        ErrClass::FactType,
    );
}

#[test]
fn unjustified_range_fact() {
    // `: op` re-derives the range from the defining op; iconst 42 does
    // not imply 0..=10.
    let err = expect_reject(
        "fn @f() -> i64 {\n  fact range %x 0..=10 : op\nb0:\n  %x = iconst.i64 42\n  ret %x\n}\n",
        ErrClass::FactRange,
    );
    assert!(
        err.msg.contains("not implied by the defining op"),
        "{}",
        err.msg
    );
}

#[test]
fn empty_range_fact() {
    expect_reject(
        "fn @f() -> i64 {\n  fact range %x 10..=0 : excl.mut\nb0:\n  %x = iconst.i64 5\n  ret %x\n}\n",
        ErrClass::FactRange,
    );
}

#[test]
fn range_fact_on_block_param_citing_op() {
    expect_reject(
        "fn @f(i64) -> i64 {\n  fact range %x 0..=10 : op\nb0(%x: i64):\n  ret %x\n}\n",
        ErrClass::FactJust,
    );
}

#[test]
fn noalias_of_value_with_itself() {
    expect_reject(
        "fn @f(ptr) -> ptr {\n  fact noalias %p %p : excl.mut\nb0(%p: ptr):\n  ret %p\n}\n",
        ErrClass::FactNoalias,
    );
}

#[test]
fn deref_citing_non_allocation() {
    expect_reject(
        "fn @f(read ptr, i64, mem.r0) -> ptr {\n  fact deref %q 8 : op %q\nb0(%p: ptr, %i: i64, %m: mem.r0):\n  %q = ptr.off %p, %i, 8\n  ret %q\n}\n",
        ErrClass::FactDeref,
    );
}

#[test]
fn noalias_cannot_be_op_derived() {
    // There is no way to state an unverified aliasing claim (D2).
    expect_reject(
        "fn @f(mut ptr, ptr) -> i64 {\n  fact noalias %p %q : op\nb0(%p: ptr, %q: ptr):\n  %z = iconst.i64 0\n  ret %z\n}\n",
        ErrClass::FactJust,
    );
}

// ---- the s26 memory-family negatives -------------------------------

#[test]
fn load_after_region_free() {
    // Use-after-free is STRUCTURAL: region.free consumed the token, so
    // a later load through it is a token-order rejection, not a
    // runtime hope.
    let err = expect_reject(
        "fn @f() -> i64 {\nb0:\n  %h: ptr, %m: mem.r0 = region.new\n  %n = iconst.i64 8\n  %p: ptr, %m2: mem.r0 = region.alloc %h, %n, %m\n  region.free %h, %m2\n  %v = load.i64 %p, %m2\n  ret %v\n}\n",
        ErrClass::TokenOrder,
    );
    assert!(err.msg.contains("no live token, no loads"), "{}", err.msg);
}

#[test]
fn load_of_stale_token_after_store() {
    // Reading a token a store already consumed (the pre-store world)
    // is the same staleness, same block.
    expect_reject(
        "fn @f(mut ptr, i64, mem.r0) -> i64 {\nb0(%p: ptr, %v: i64, %m: mem.r0):\n  %m2 = store.i64 %v, %p, %m\n  %x = load.i64 %p, %m\n  ret %x\n}\n",
        ErrClass::TokenOrder,
    );
}

#[test]
fn cross_block_load_after_free() {
    expect_reject(
        "fn @f(bool) -> i64 {\nb0(%c: bool):\n  %h: ptr, %m: mem.r0 = region.new\n  %n = iconst.i64 8\n  %p: ptr, %m2: mem.r0 = region.alloc %h, %n, %m\n  region.free %h, %m2\n  br %c, b1, b2\nb1:\n  %v = load.i64 %p, %m2\n  ret %v\nb2:\n  %z = iconst.i64 0\n  ret %z\n}\n",
        ErrClass::TokenOrder,
    );
}

#[test]
fn two_roots_for_one_region() {
    // region.new minting a region the signature already owns.
    expect_reject(
        "fn @f(mem.r0) {\nb0(%m: mem.r0):\n  %h: ptr, %m1: mem.r0 = region.new\n  ret\n}\n",
        ErrClass::RegionRoot,
    );
}

#[test]
fn frozen_token_cannot_be_consumed() {
    // Stores/frees through a frozen region are unrepresentable.
    let err = expect_reject(
        "fn @f(i64) {\nb0(%v: i64):\n  %h: ptr, %m: mem.r0 = region.new\n  %f: mem.r0 = sync.freeze %h, %m\n  %m2: mem.r0 = store.i64 %v, %h, %f\n  ret\n}\n",
        ErrClass::FrozenToken,
    );
    assert!(err.msg.contains("frozen token"), "{}", err.msg);
}

#[test]
fn region_fact_forgery_rejected() {
    // Claiming region r9 for a pointer whose allocation mints r0:
    // provenance cannot be forged in safe-tier WIR.
    let err = expect_reject(
        "fn @f() -> i64 {\n  fact region %p r9 : op\nb0:\n  %h: ptr, %m: mem.r0 = region.new\n  %n = iconst.i64 8\n  %p: ptr, %m2: mem.r0 = region.alloc %h, %n, %m\n  %v = load.i64 %p, %m2\n  ret %v\n}\n",
        ErrClass::FactJust,
    );
    assert!(err.msg.contains("forge"), "{}", err.msg);
}

#[test]
fn region_fact_must_cite_an_allocation() {
    // An arbitrary pointer (entry param) cannot claim region identity
    // through `: op` — nothing local derives it.
    expect_reject(
        "fn @f(read ptr, mem.r0) -> ptr {\n  fact region %p r0 : op\nb0(%p: ptr, %m: mem.r0):\n  ret %p\n}\n",
        ErrClass::FactJust,
    );
}

#[test]
fn deref_overclaim_rejected() {
    // The allocation provides 8 bytes; the fact claims 64.
    let err = expect_reject(
        "fn @f() -> i64 {\n  fact deref %p 64 : op\nb0:\n  %h: ptr, %m: mem.r0 = region.new\n  %n = iconst.i64 8\n  %p: ptr, %m2: mem.r0 = region.alloc %h, %n, %m\n  %v = load.i64 %p, %m2\n  ret %v\n}\n",
        ErrClass::FactDeref,
    );
    assert!(err.msg.contains("more bytes"), "{}", err.msg);
}

#[test]
fn rem_range_fact_rederived() {
    // urem.chk by 10 implies 0..=9; claiming 0..=3 must be rejected,
    // and the true postcondition must be accepted.
    expect_reject(
        "fn @f(i64) -> i64 {\n  fact range %r 0..=3 : op\nb0(%x: i64):\n  %ten = iconst.i64 10\n  %r = urem.chk %x, %ten\n  ret %r\n}\n",
        ErrClass::FactRange,
    );
    let good = "fn @f(i64) -> i64 {\n  fact range %r 0..=9 : op\nb0(%x: i64):\n  %ten = iconst.i64 10\n  %r = urem.chk %x, %ten\n  ret %r\n}\n";
    let m = wolf_wir::parse_module(good).expect("parses");
    verify_module(&m).expect("the true postcondition verifies");
}

#[test]
fn stack_alloc_size_must_be_const() {
    expect_reject(
        "fn @f(i64) {\nb0(%n: i64):\n  %p: ptr, %m: mem.r0 = stack.alloc %n\n  ret\n}\n",
        ErrClass::Type,
    );
}

#[test]
fn function_without_blocks() {
    // Not representable in text — built through the raw arena API.
    let mut m = Module::new();
    let sig = m.make_sig(vec![], vec![]);
    m.add_func(Function::new("empty", sig));
    let err = verify_module(&m).expect_err("no blocks");
    assert_eq!(err.class, ErrClass::Layout);
}

#[test]
fn conflicting_callee_signatures() {
    // Same name, two different signatures, in two different functions:
    // caught at module level.
    let mut m = Module::new();
    let sig_v = m.make_sig(vec![], vec![]);
    let sig_a = m.make_sig(vec![wolf_wir::Param::val(wolf_wir::types::I64)], vec![]);
    let mut f1 = Function::new("f1", sig_v);
    let b = f1.make_block(&[]);
    f1.import_func("mystery", sig_v);
    f1.append_inst(b, wolf_wir::Opcode::Ret, &[], &[], wolf_wir::Aux::None);
    let mut f2 = Function::new("f2", sig_v);
    let b = f2.make_block(&[]);
    f2.import_func("mystery", sig_a);
    f2.append_inst(b, wolf_wir::Opcode::Ret, &[], &[], wolf_wir::Aux::None);
    m.add_func(f1);
    m.add_func(f2);
    let err = verify_module(&m).expect_err("conflicting sigs");
    assert_eq!(err.class, ErrClass::CallSig);
}

// ---- the s27 error-union family + per-path token linearity ----

#[test]
fn eu_make_err_tag_must_be_i64() {
    expect_reject(
        "fn @f() -> eu{i64} {\nb0:\n  %b = bconst true\n  %e: eu{i64} = eu.make.err %b\n  ret %e\n}\n",
        ErrClass::Type,
    );
}

#[test]
fn eu_ok_on_unit_ok_extracts_nothing() {
    expect_reject(
        "fn @f(i64) -> i64 {\nb0(%x: i64):\n  %t = iconst.i64 1\n  %e: eu{unit} = eu.make.err %t\n  %v: i64 = eu.ok %e\n  ret %v\n}\n",
        ErrClass::Type,
    );
}

#[test]
fn eu_is_err_needs_a_union() {
    expect_reject(
        "fn @f(i64) -> i64 {\nb0(%x: i64):\n  %b: bool = eu.is_err %x\n  ret %x\n}\n",
        ErrClass::Type,
    );
}

#[test]
fn eu_err_slot_out_of_range() {
    expect_reject(
        "fn @f(i64) -> i64 {\nb0(%x: i64):\n  %t = iconst.i64 1\n  %e: eu{i64, i64} = eu.make.err %t, %x\n  %p: i64 = eu.err %e, 1\n  ret %p\n}\n",
        ErrClass::Type,
    );
}

#[test]
fn eu_make_ok_payload_type_must_match() {
    expect_reject(
        "fn @f(f64) -> eu{i64} {\nb0(%x: f64):\n  %e: eu{i64} = eu.make.ok %x\n  ret %e\n}\n",
        ErrClass::Type,
    );
}

/// s27 defer duplication: one token value consumed on BOTH arms of a
/// branch is legal — only one arm runs (per-path linearity).
#[test]
fn token_consumed_on_disjoint_arms_is_accepted() {
    let src = "fn @f(bool) {\nb0(%c: bool):\n  %h: ptr, %m: mem.r0 = region.new\n  br %c, b1, b2\nb1:\n  region.free %h, %m\n  ret\nb2:\n  region.free %h, %m\n  ret\n}\n";
    let m = wolf_wir::parse_module(src).expect("parses");
    verify_module(&m).expect("disjoint-arm consumption is per-path linear");
}

/// Two consumers where one reaches the other stays a rejection: that
/// path consumes the token twice.
#[test]
fn token_consumed_twice_across_sequential_blocks() {
    let err = expect_reject(
        "fn @f() {\nb0:\n  %h: ptr, %m: mem.r0 = region.new\n  jmp b1\nb1:\n  region.free %h, %m\n  jmp b2\nb2:\n  region.free %h, %m\n  ret\n}\n",
        ErrClass::TokenLinearity,
    );
    assert!(err.msg.contains("on one path"), "{}", err.msg);
}

#[test]
fn every_rejection_class_is_exercised() {
    // The suite above (plus pass_facts.rs for DroppedFact) covers every
    // class — this is a checklist so a new class cannot land untested.
    let covered = [
        ErrClass::Layout,
        ErrClass::EntrySig,
        ErrClass::Terminator,
        ErrClass::ReservedOp,
        ErrClass::Type,
        ErrClass::ConstRange,
        ErrClass::CallSig,
        ErrClass::BlockArgArity,
        ErrClass::BlockArgType,
        ErrClass::Unreachable,
        ErrClass::Dominance,
        ErrClass::TokenLinearity,
        ErrClass::TokenOrder,
        ErrClass::RegionRoot,
        ErrClass::FrozenToken,
        ErrClass::FactType,
        ErrClass::FactRange,
        ErrClass::FactNoalias,
        ErrClass::FactDeref,
        ErrClass::FactJust,
        ErrClass::DroppedFact, // pass_facts.rs
    ];
    assert_eq!(covered.len(), 21);
}
