//! The peephole mechanism, unit-tested at each gauntlet stage (s25
//! acceptance): fold (including checked-overflow → trap), the identity
//! table, GVN hit-and-exact-rollback, GVN misses across non-dominating
//! scopes, `br const → jmp`, and the token-threaded wins — store→load
//! forwarding and redundant-load GVN. Plus Braun loop-carried tokens
//! as block params through the raw builder API.

use wolf_wir::entity::EntityRef;
use wolf_wir::ir::{Aux, Module, Param};
use wolf_wir::types::{BOOL, I32, I64, PTR, RegionId};
use wolf_wir::{FuncBuilder, InsOut, Opcode, verify_function};

fn arith_builder(m: &mut Module) -> FuncBuilder<'_> {
    let sig = m.make_sig(vec![Param::val(I64), Param::val(I64)], vec![I64]);
    FuncBuilder::new(m, "t", sig)
}

// ------------------------------------------------------------- fold ----

#[test]
fn all_const_operands_fold_and_roll_back() {
    let mut m = Module::new();
    let mut b = arith_builder(&mut m);
    let c2 = b.iconst(I64, 2);
    let c3 = b.iconst(I64, 3);
    let insts_before = b.func.insts.len();
    let vals_before = b.func.values.len();
    let out = b.ins(Opcode::IaddChk, &[c2, c3], &[I64], Aux::None).one();
    // The iadd was rolled back; only the new iconst 5 was appended.
    assert_eq!(b.func.insts.len(), insts_before + 1);
    assert_eq!(b.func.values.len(), vals_before + 1);
    assert_eq!(b.as_int_const(out), Some(5));
    assert_eq!(b.stats.fold, 1);
    b.ins_ret(&[out]);
    let f = b.finish();
    verify_function(&m, &f).unwrap();
}

#[test]
fn checked_overflow_folds_to_trap() {
    let mut m = Module::new();
    let sig = m.make_sig(vec![], vec![]);
    let mut b = FuncBuilder::new(&mut m, "t", sig);
    let max = b.iconst(I32, i32::MAX as i64);
    let one = b.iconst(I32, 1);
    let out = b.ins(Opcode::IaddChk, &[max, one], &[I32], Aux::None);
    assert!(matches!(out, InsOut::Trapped));
    assert!(b.is_filled(b.current_block()));
    let f = b.finish();
    verify_function(&m, &f).unwrap();
    m.add_func(f);
    // The block ends in `trap`, exactly the X3 runtime outcome.
    let dump = wolf_wir::print_module(&m);
    assert!(dump.contains("trap"), "{dump}");
}

#[test]
fn const_zero_divisor_folds_to_trap_even_with_unknown_lhs() {
    let mut m = Module::new();
    let mut b = arith_builder(&mut m);
    let x = b.block_params(b.current_block())[0];
    let zero = b.iconst(I64, 0);
    let out = b.ins(Opcode::IdivChk, &[x, zero], &[I64], Aux::None);
    assert!(matches!(out, InsOut::Trapped));
    let f = b.finish();
    verify_function(&m, &f).unwrap();
}

// --------------------------------------------------- identity table ----

#[test]
fn identity_add_zero_returns_operand_and_rolls_back() {
    let mut m = Module::new();
    let mut b = arith_builder(&mut m);
    let x = b.block_params(b.current_block())[0];
    let zero = b.iconst(I64, 0);
    let len = b.func.insts.len();
    let out = b.ins(Opcode::IaddChk, &[x, zero], &[I64], Aux::None).one();
    assert_eq!(out, x, "x + 0 → x");
    assert_eq!(b.func.insts.len(), len, "arena rolled back to the mark");
    assert_eq!(b.stats.identity, 1);
    // The commuted form too (0 + x → x).
    let out = b.ins(Opcode::IaddChk, &[zero, x], &[I64], Aux::None).one();
    assert_eq!(out, x);
    // x & x → x; x ^ x → 0; x - x → 0; x * 1 → x.
    let band = b.ins(Opcode::Band, &[x, x], &[I64], Aux::None).one();
    assert_eq!(band, x);
    let bxor = b.ins(Opcode::Bxor, &[x, x], &[I64], Aux::None).one();
    assert_eq!(b.as_int_const(bxor), Some(0));
    let sub = b.ins(Opcode::IsubChk, &[x, x], &[I64], Aux::None).one();
    assert_eq!(b.as_int_const(sub), Some(0));
    let one = b.iconst(I64, 1);
    let mul = b.ins(Opcode::ImulChk, &[x, one], &[I64], Aux::None).one();
    assert_eq!(mul, x);
    b.ins_ret(&[x]);
    let f = b.finish();
    verify_function(&m, &f).unwrap();
}

#[test]
fn icmp_same_operand_folds_by_reflexivity() {
    let mut m = Module::new();
    let mut b = arith_builder(&mut m);
    let x = b.block_params(b.current_block())[0];
    use wolf_wir::IntCc;
    let eq = b
        .ins(Opcode::Icmp, &[x, x], &[BOOL], Aux::IntCc(IntCc::Eq))
        .one();
    assert_eq!(b.as_bool_const(eq), Some(true));
    let lt = b
        .ins(Opcode::Icmp, &[x, x], &[BOOL], Aux::IntCc(IntCc::Slt))
        .one();
    assert_eq!(b.as_bool_const(lt), Some(false));
}

// -------------------------------------------------------------- GVN ----

#[test]
fn gvn_hit_returns_dominating_value_and_rolls_back_to_exact_mark() {
    let mut m = Module::new();
    let mut b = arith_builder(&mut m);
    let params = b.block_params(b.current_block());
    let (x, y) = (params[0], params[1]);
    let first = b.ins(Opcode::IaddChk, &[x, y], &[I64], Aux::None).one();
    let insts = b.func.insts.len();
    let values = b.func.values.len();
    let second = b.ins(Opcode::IaddChk, &[x, y], &[I64], Aux::None).one();
    assert_eq!(second, first, "GVN returns the dominating value");
    assert_eq!(b.func.insts.len(), insts, "exact arena length");
    assert_eq!(b.func.values.len(), values, "exact value count");
    assert_eq!(b.stats.gvn, 1);
    // Commutative canonicalization: y + x hits too.
    let third = b.ins(Opcode::IaddChk, &[y, x], &[I64], Aux::None).one();
    assert_eq!(third, first);
    // A different opcode misses.
    let sub = b.ins(Opcode::IsubChk, &[x, y], &[I64], Aux::None).one();
    assert_ne!(sub, first);
}

#[test]
fn gvn_misses_across_non_dominating_scopes() {
    let mut m = Module::new();
    let mut b = arith_builder(&mut m);
    let params = b.block_params(b.current_block());
    let (x, y) = (params[0], params[1]);
    b.gvn_push_scope();
    let inner = b.ins(Opcode::IaddChk, &[x, y], &[I64], Aux::None).one();
    b.gvn_pop_scope();
    // The scope popped: the same expression must MISS (a value from a
    // sibling arm does not dominate).
    let after = b.ins(Opcode::IaddChk, &[x, y], &[I64], Aux::None).one();
    assert_ne!(after, inner, "popped scopes must not leak GVN entries");
    // But an outer-scope entry hits inside a nested scope.
    b.gvn_push_scope();
    let hit = b.ins(Opcode::IaddChk, &[x, y], &[I64], Aux::None).one();
    assert_eq!(hit, after, "dominating entries stay visible");
    b.gvn_pop_scope();
}

// ------------------------------------------------- br const → jmp ----

#[test]
fn br_on_const_emits_the_taken_edge_only() {
    let mut m = Module::new();
    let sig = m.make_sig(vec![Param::val(I64)], vec![I64]);
    let mut b = FuncBuilder::new(&mut m, "t", sig);
    let x = b.block_params(b.current_block())[0];
    let t = b.bconst(true);
    let merge = b.create_block();
    let p = b.add_block_param(merge, I64);
    let zero = b.iconst(I64, 0);
    // br true, merge(x), merge(0) — must lower to jmp merge(x).
    b.ins_br(t, merge, &[x], merge, &[zero]);
    b.seal_block(merge);
    b.switch_to_block(merge);
    b.ins_ret(&[p]);
    let f = b.finish();
    verify_function(&m, &f).unwrap();
    m.add_func(f);
    let dump = wolf_wir::print_module(&m);
    assert!(!dump.contains("br "), "no br remains: {dump}");
    assert!(dump.contains("jmp"), "{dump}");
}

// ------------------------------------- tokens, forwarding, loads ----

/// (ptr, i64, mem.r0) -> i64 with a store/load playground.
fn mem_builder(m: &mut Module) -> FuncBuilder<'_> {
    let mem0 = m.types.mem(RegionId::new(0));
    let sig = m.make_sig(
        vec![Param::val(PTR), Param::val(I64), Param::val(mem0)],
        vec![I64],
    );
    let mut b = FuncBuilder::new(m, "t", sig);
    let tok = b.block_params(b.current_block())[2];
    b.def_mem(RegionId::new(0), tok);
    b
}

#[test]
fn store_to_load_forwarding_through_the_token_chain() {
    let mut m = Module::new();
    let mut b = mem_builder(&mut m);
    let params = b.block_params(b.current_block());
    let (p, v) = (params[0], params[1]);
    let r0 = RegionId::new(0);
    b.ins_store(v, p, r0);
    let loaded = b.ins_load(I64, p, r0);
    assert_eq!(loaded, v, "the load forwards the stored value");
    assert_eq!(b.stats.forward, 1);
    b.ins_ret(&[loaded]);
    let f = b.finish();
    verify_function(&m, &f).unwrap();
    m.add_func(f);
    let dump = wolf_wir::print_module(&m);
    assert!(!dump.contains("load"), "no load survives: {dump}");
    insta::assert_snapshot!("store_to_load_forwarding", dump);
}

#[test]
fn redundant_load_elimination_via_gvn_same_token() {
    let mut m = Module::new();
    let mut b = mem_builder(&mut m);
    let p = b.block_params(b.current_block())[0];
    let r0 = RegionId::new(0);
    let a = b.ins_load(I64, p, r0);
    let c = b.ins_load(I64, p, r0);
    assert_eq!(a, c, "same address, same token: one load");
    assert_eq!(b.stats.gvn, 1);
    // A store defs a NEW token: the next load must miss.
    let one = b.iconst(I64, 1);
    b.ins_store(one, p, r0);
    let d = b.ins_load(I64, p, r0);
    assert_eq!(d, one, "forwarded from the new store");
    b.ins_ret(&[a]);
    let f = b.finish();
    verify_function(&m, &f).unwrap();
    m.add_func(f);
    // The dump contains exactly ONE load where the source had two —
    // the redundant-load-elimination snapshot (s25 acceptance).
    let dump = wolf_wir::print_module(&m);
    assert_eq!(dump.matches("load").count(), 1, "{dump}");
    insta::assert_snapshot!("redundant_load_elimination", dump);
}

#[test]
fn loop_carried_token_becomes_a_block_param() {
    let mut m = Module::new();
    let mem0 = m.types.mem(RegionId::new(0));
    let sig = m.make_sig(
        vec![Param::val(PTR), Param::val(I64), Param::val(mem0)],
        vec![],
    );
    let mut b = FuncBuilder::new(&mut m, "t", sig);
    let r0 = RegionId::new(0);
    let params = b.block_params(b.current_block());
    let (p, n, tok) = (params[0], params[1], params[2]);
    b.def_mem(r0, tok);
    // Counter variable.
    let i = b.declare_var(I64);
    let zero = b.iconst(I64, 0);
    b.def_var(i, zero);
    let header = b.create_block();
    b.ins_jmp(header, &[]);
    b.switch_to_block(header);
    // Loop body: store i to p[i], i += 1, loop while i < n.
    let iv = b.use_var(i);
    let addr = b.ins_ptr_off(p, iv, 8);
    b.ins_store(iv, addr, r0);
    let one = b.iconst(I64, 1);
    let next = b.ins(Opcode::IaddChk, &[iv, one], &[I64], Aux::None).one();
    b.def_var(i, next);
    use wolf_wir::IntCc;
    let cont = b
        .ins(Opcode::Icmp, &[next, n], &[BOOL], Aux::IntCc(IntCc::Slt))
        .one();
    let exit = b.create_block();
    b.ins_br(cont, header, &[], exit, &[]);
    b.seal_block(header);
    b.seal_block(exit);
    b.switch_to_block(exit);
    b.ins_ret(&[]);
    let f = b.finish();
    verify_function(&m, &f).unwrap();
    m.add_func(f);
    let dump = wolf_wir::print_module(&m);
    // The header carries BOTH the counter and the mem token as params
    // (loop-carried token with zero extra machinery), and the store's
    // successor token feeds the back edge.
    assert!(
        dump.contains("mem.r0):") || dump.contains(": mem.r0"),
        "{dump}"
    );
    let header_line = dump
        .lines()
        .find(|l| l.starts_with("b1(") && l.contains("mem.r0"))
        .unwrap_or_else(|| panic!("loop header must carry the token param:\n{dump}"));
    assert!(header_line.contains("i64"), "counter param too: {dump}");
}

// ------------------------------- a call clobbers foreign storage ----
//
// s214 (wolf-lang#598): module state, `extern "c" let` storage and every
// raw pointer access ride the function's FOREIGN buffer region, whose
// token a callee never consumes — it writes the same bytes under a root
// of its own. So a store or load built before a call is never reused
// after it: the load stays, and reads what the callee wrote. At 0.2.24
// the builder forwarded the stored value across the call and hash-consed
// the second load into the first (native and release printed 0 for 99).

/// `(ptr) -> i64` with a foreign buffer root and a tokenless callee.
fn foreign_builder(m: &mut Module) -> (FuncBuilder<'_>, RegionId, wolf_wir::ir::ExtFunc) {
    let sig = m.make_sig(vec![Param::val(PTR)], vec![I64]);
    let unit = m.make_sig(vec![], vec![]);
    let mut b = FuncBuilder::new(m, "t", sig);
    let r = b.ins_region_foreign(wolf_wir::ops::ForeignRole::Buffer);
    let g = b.func.import_func("g", unit);
    (b, r, g)
}

fn verified_dump(m: &mut Module, f: wolf_wir::ir::Function) -> String {
    verify_function(m, &f).unwrap();
    m.add_func(f);
    wolf_wir::print_module(m)
}

#[test]
fn a_foreign_store_is_not_forwarded_across_a_call() {
    let mut m = Module::new();
    let (mut b, r, g) = foreign_builder(&mut m);
    let p = b.block_params(b.current_block())[0];
    let zero = b.iconst(I64, 0);
    b.ins_store(zero, p, r);
    b.ins_call(g, &[]);
    let after = b.ins_load(I64, p, r);
    assert_ne!(after, zero, "the callee may have written p: reload");
    assert_eq!(b.stats.forward, 0);
    b.ins_ret(&[after]);
    let f = b.finish();
    let dump = verified_dump(&mut m, f);
    let call = dump.find("call @g").expect("the call");
    let load = dump.find("load.i64").expect("a load survives");
    assert!(call < load, "the load follows the call: {dump}");
}

#[test]
fn a_foreign_load_is_not_reused_across_a_call() {
    let mut m = Module::new();
    let (mut b, r, g) = foreign_builder(&mut m);
    let p = b.block_params(b.current_block())[0];
    let before = b.ins_load(I64, p, r);
    b.ins_call(g, &[]);
    let after = b.ins_load(I64, p, r);
    assert_ne!(before, after, "two loads, one each side of the call");
    assert_eq!(b.stats.gvn, 0);
    let sum = b
        .ins(Opcode::IaddWrap, &[before, after], &[I64], Aux::None)
        .one();
    b.ins_ret(&[sum]);
    let f = b.finish();
    let dump = verified_dump(&mut m, f);
    assert_eq!(dump.matches("load.i64").count(), 2, "{dump}");
}

#[test]
fn a_call_through_a_fn_value_clobbers_too() {
    let mut m = Module::new();
    let (mut b, r, _) = foreign_builder(&mut m);
    let p = b.block_params(b.current_block())[0];
    let unit = b.module.make_sig(vec![], vec![]);
    let fp = b.ins_load(PTR, p, r);
    let one = b.iconst(I64, 1);
    b.ins_store(one, p, r);
    b.ins_call_ind(fp, unit, &[]);
    let after = b.ins_load(I64, p, r);
    assert_ne!(after, one, "call.ind may write p");
    b.ins_ret(&[after]);
    let f = b.finish();
    let dump = verified_dump(&mut m, f);
    assert_eq!(dump.matches("load.i64").count(), 1, "{dump}");
}

#[test]
fn without_a_call_foreign_forwarding_and_gvn_stay() {
    let mut m = Module::new();
    let (mut b, r, _) = foreign_builder(&mut m);
    let p = b.block_params(b.current_block())[0];
    let a = b.ins_load(I64, p, r);
    let c = b.ins_load(I64, p, r);
    assert_eq!(a, c, "no call between: one load");
    let seven = b.iconst(I64, 7);
    b.ins_store(seven, p, r);
    let d = b.ins_load(I64, p, r);
    assert_eq!(d, seven, "no call between: forwarded");
    let sum = b.ins(Opcode::IaddWrap, &[a, d], &[I64], Aux::None).one();
    b.ins_ret(&[sum]);
    let f = b.finish();
    let dump = verified_dump(&mut m, f);
    assert_eq!(dump.matches("load.i64").count(), 1, "{dump}");
}

/// A region the caller LENT (an entry `mem` param) is exhaustive: a call
/// that does not take its token cannot reach it, so forwarding across
/// the call stays — the token discipline's dividend is untouched.
#[test]
fn a_lent_region_still_forwards_across_a_call() {
    let mut m = Module::new();
    let unit = m.make_sig(vec![], vec![]);
    let mut b = mem_builder(&mut m);
    let g = b.func.import_func("g", unit);
    let params = b.block_params(b.current_block());
    let (p, v) = (params[0], params[1]);
    let r0 = RegionId::new(0);
    b.ins_store(v, p, r0);
    b.ins_call(g, &[]);
    let loaded = b.ins_load(I64, p, r0);
    assert_eq!(loaded, v, "the lent region forwards across the call");
    b.ins_ret(&[loaded]);
    let f = b.finish();
    let dump = verified_dump(&mut m, f);
    assert!(!dump.contains("load"), "{dump}");
}

/// A container LENT to the function is frozen for the call
/// (`[mem.tier0.mode.read]`, the SB holy grail): a load inside it — the
/// header's `len`, the data pointer, an element through that pointer —
/// keeps its reuse across a call. `corpus/memory/prov_holy_grail.lu` is
/// the source-level shape.
#[test]
fn a_load_inside_a_frozen_container_is_reused_across_a_call() {
    let mut m = Module::new();
    let sig = m.make_sig(vec![Param::val(PTR)], vec![I64]);
    let unit = m.make_sig(vec![], vec![]);
    let mut b = FuncBuilder::new(&mut m, "t", sig);
    let hdrs = b.ins_region_foreign(wolf_wir::ops::ForeignRole::Header);
    let bufs = b.ins_region_foreign(wolf_wir::ops::ForeignRole::Buffer);
    let g = b.func.import_func("g", unit);
    let xs = b.block_params(b.current_block())[0];
    b.freeze_root(xs);
    let eight = b.iconst(I64, 8);
    let len_at = b.ins_ptr_off(xs, eight, 1);
    let len = b.ins_load(I64, len_at, hdrs);
    let data = b.ins_load(PTR, xs, hdrs);
    let zero = b.iconst(I64, 0);
    let at = b.ins_ptr_off(data, zero, 8);
    let first = b.ins_load(I64, at, bufs);
    b.ins_call(g, &[]);
    assert_eq!(b.ins_load(I64, len_at, hdrs), len, "len, frozen");
    let data2 = b.ins_load(PTR, xs, hdrs);
    assert_eq!(data2, data, "the data pointer, frozen");
    let at2 = b.ins_ptr_off(data2, zero, 8);
    assert_eq!(b.ins_load(I64, at2, bufs), first, "the element, frozen");
    let sum = b
        .ins(Opcode::IaddWrap, &[len, first], &[I64], Aux::None)
        .one();
    b.ins_ret(&[sum]);
    let f = b.finish();
    let dump = verified_dump(&mut m, f);
    assert_eq!(dump.matches("= load").count(), 3, "{dump}");
}

/// The same chain from a parameter NOT frozen (a raw pointer, or a
/// container taken or lent `mut`) reloads after the call.
#[test]
fn the_same_chain_from_an_unfrozen_parameter_reloads() {
    let mut m = Module::new();
    let sig = m.make_sig(vec![Param::val(PTR)], vec![I64]);
    let unit = m.make_sig(vec![], vec![]);
    let mut b = FuncBuilder::new(&mut m, "t", sig);
    let hdrs = b.ins_region_foreign(wolf_wir::ops::ForeignRole::Header);
    let g = b.func.import_func("g", unit);
    let xs = b.block_params(b.current_block())[0];
    let eight = b.iconst(I64, 8);
    let len_at = b.ins_ptr_off(xs, eight, 1);
    let len = b.ins_load(I64, len_at, hdrs);
    b.ins_call(g, &[]);
    let again = b.ins_load(I64, len_at, hdrs);
    assert_ne!(again, len, "not frozen: the callee may have pushed");
    let sum = b
        .ins(Opcode::IaddWrap, &[len, again], &[I64], Aux::None)
        .one();
    b.ins_ret(&[sum]);
    let f = b.finish();
    let dump = verified_dump(&mut m, f);
    assert_eq!(dump.matches("= load").count(), 2, "{dump}");
}

/// An INERT call (the print shims: they write no memory a program can
/// name) keeps forwarding and load reuse; the next ordinary call does not.
#[test]
fn an_inert_call_keeps_forwarding_and_the_next_call_does_not() {
    let mut m = Module::new();
    let (mut b, r, g) = foreign_builder(&mut m);
    let p = b.block_params(b.current_block())[0];
    let unit = b.module.make_sig(vec![], vec![]);
    let print = b.func.import_func("__wolf_rt_print_begin", unit);
    let seven = b.iconst(I64, 7);
    b.ins_store(seven, p, r);
    b.ins_call_inert(print, &[]);
    assert_eq!(
        b.ins_load(I64, p, r),
        seven,
        "forwarded across the inert call"
    );
    b.ins_call(g, &[]);
    let after = b.ins_load(I64, p, r);
    assert_ne!(after, seven, "not across the ordinary one");
    b.ins_ret(&[after]);
    let f = b.finish();
    let dump = verified_dump(&mut m, f);
    assert_eq!(dump.matches("= load").count(), 1, "{dump}");
}

/// The loop shape: a load before the loop, a call in its body, the same
/// load after it. The body's call runs between them on the path through
/// the loop, though no call sits between them in the preheader.
#[test]
fn a_call_in_a_loop_body_clobbers_the_load_after_the_loop() {
    let mut m = Module::new();
    let (mut b, r, g) = foreign_builder(&mut m);
    let p = b.block_params(b.current_block())[0];
    let before = b.ins_load(I64, p, r);
    let header = b.create_block();
    b.ins_jmp(header, &[]);
    b.switch_to_block(header);
    b.ins_call(g, &[]);
    let flag = b.ins_load(BOOL, p, r);
    let exit = b.create_block();
    b.ins_br(flag, header, &[], exit, &[]);
    b.seal_block(header);
    b.seal_block(exit);
    b.switch_to_block(exit);
    let after = b.ins_load(I64, p, r);
    assert_ne!(before, after, "the body's call ran between them");
    let sum = b
        .ins(Opcode::IaddWrap, &[before, after], &[I64], Aux::None)
        .one();
    b.ins_ret(&[sum]);
    let f = b.finish();
    let dump = verified_dump(&mut m, f);
    assert_eq!(dump.matches("load.i64").count(), 2, "{dump}");
}

// ------------------------------------------- the s99-era identities ----

/// `isub.wrap(iadd.chk(x, k), x)` IS `k` — exact because a live
/// checked-add result holds the mathematical sum (it would have
/// trapped otherwise). The shape is a slice width: `s[i..i + K]`
/// lowers this exact pair, and the width folding to `K` is what lets
/// `str_eq_inline` see a constant operand length (d2's hot loop).
#[test]
fn isub_wrap_of_iadd_chk_is_the_addend() {
    let mut m = Module::new();
    let mut b = arith_builder(&mut m);
    let params = b.block_params(b.current_block());
    let (x, k) = (params[0], params[1]);
    let end = b.ins(Opcode::IaddChk, &[x, k], &[I64], Aux::None).one();
    let idents_before = b.stats.identity;
    // Both operand orders of the sub's match against the add.
    let w1 = b.ins(Opcode::IsubWrap, &[end, x], &[I64], Aux::None).one();
    assert_eq!(w1, k, "isub.wrap(x + k, x) is k, the existing value");
    let w2 = b.ins(Opcode::IsubWrap, &[end, k], &[I64], Aux::None).one();
    assert_eq!(w2, x, "isub.wrap(x + k, k) is x, the existing value");
    assert_eq!(b.stats.identity, idents_before + 2);
    b.ins_ret(&[w1]);
    let f = b.finish();
    verify_function(&m, &f).expect("verifies");
}

/// The same identity over `iadd.wrap`: ((x + k) mod 2^b) - x is k mod
/// 2^b, which IS k for same-width operands — all `.wrap` promises.
#[test]
fn isub_wrap_of_iadd_wrap_is_the_addend() {
    let mut m = Module::new();
    let mut b = arith_builder(&mut m);
    let params = b.block_params(b.current_block());
    let (x, k) = (params[0], params[1]);
    let end = b.ins(Opcode::IaddWrap, &[x, k], &[I64], Aux::None).one();
    let w = b.ins(Opcode::IsubWrap, &[end, x], &[I64], Aux::None).one();
    assert_eq!(w, k);
    b.ins_ret(&[w]);
    let f = b.finish();
    verify_function(&m, &f).expect("verifies");
}

/// `agg.get(agg.make(v0, v1), k)` IS `vk` — a pair built and
/// immediately projected carries nothing the ingredient did not. The
/// lowering needs this at BUILD time: a slice's `{ptr, len}` is an
/// `agg.make`, and `str_eq_inline` asks `as_int_const` of the length
/// one projection deep.
#[test]
fn agg_get_of_agg_make_is_the_ingredient() {
    let mut m = Module::new();
    let sig = m.make_sig(vec![Param::val(PTR), Param::val(I64)], vec![I64]);
    let mut b = FuncBuilder::new(&mut m, "t", sig);
    let params = b.block_params(b.current_block());
    let (p, l) = (params[0], params[1]);
    let agg_ty = b
        .module
        .types
        .intern(wolf_wir::types::TypeData::Agg(vec![PTR, I64]));
    let pair = b.ins(Opcode::AggMake, &[p, l], &[agg_ty], Aux::None).one();
    let got_p = b.ins(Opcode::AggGet, &[pair], &[PTR], Aux::Int(0)).one();
    let got_l = b.ins(Opcode::AggGet, &[pair], &[I64], Aux::Int(1)).one();
    assert_eq!(got_p, p, "projection 0 is the ingredient");
    assert_eq!(got_l, l, "projection 1 is the ingredient");
    // A constant ingredient is therefore VISIBLE to as_int_const
    // through the pair — the property d2's lowering leans on.
    let five = b.iconst(I64, 5);
    let cpair = b
        .ins(Opcode::AggMake, &[p, five], &[agg_ty], Aux::None)
        .one();
    let clen = b.ins(Opcode::AggGet, &[cpair], &[I64], Aux::Int(1)).one();
    assert_eq!(b.as_int_const(clen), Some(5));
    b.ins_ret(&[got_l]);
    let f = b.finish();
    verify_function(&m, &f).expect("verifies");
}

// ----------------------------------------------- determinism gate ----

#[test]
fn identical_builder_runs_print_identically() {
    let build = |seed: i64| {
        let mut m = Module::new();
        let mut b = arith_builder(&mut m);
        let params = b.block_params(b.current_block());
        let (x, y) = (params[0], params[1]);
        let c = b.iconst(I64, seed);
        let s = b.ins(Opcode::IaddChk, &[x, c], &[I64], Aux::None).one();
        let t = b.ins(Opcode::ImulChk, &[s, y], &[I64], Aux::None).one();
        b.ins_ret(&[t]);
        let f = b.finish();
        m.add_func(f);
        wolf_wir::print_module(&m)
    };
    assert_eq!(build(7), build(7));
}
