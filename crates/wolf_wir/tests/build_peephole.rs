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
