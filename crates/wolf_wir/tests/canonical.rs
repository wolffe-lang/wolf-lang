//! Canonical numbering (s24 acceptance): the printer numbers blocks in
//! RPO and values in definition order along it, so the SAME function
//! built in two different insertion orders dumps byte-identically —
//! dumps diff cleanly and D8 content-hashing has a stable input.

use wolf_wir::types::{BOOL, I64};
use wolf_wir::{Aux, Function, IntCc, Module, Opcode, Param, verify_module};

/// A diamond: entry branches, both arms jump to a join with an arg.
/// `order` flips the creation order of the non-entry blocks and the
/// order their bodies are filled in; the FUNCTION is the same.
fn build(order_flipped: bool) -> (Module, String) {
    let mut m = Module::new();
    let sig = m.make_sig(vec![Param::val(BOOL), Param::val(I64)], vec![I64]);
    let mut f = Function::new("diamond", sig);
    let entry = f.make_block(&[BOOL, I64]);
    let (bt, bf, bj);
    if order_flipped {
        bj = f.make_block(&[I64]);
        bf = f.make_block(&[]);
        bt = f.make_block(&[]);
    } else {
        bt = f.make_block(&[]);
        bf = f.make_block(&[]);
        bj = f.make_block(&[I64]);
    }
    let params = f.block_params(entry);
    let (c, x) = (params[0], params[1]);
    let jparam = f.block_params(bj)[0];

    let fill_bt = |f: &mut Function| {
        let (_, r) = f.append_inst(bt, Opcode::IaddChk, &[x, x], &[I64], Aux::None);
        let edge = f.block_call(bj, &[r[0]]);
        f.append_inst(bt, Opcode::Jmp, &[], &[], Aux::Jump(edge));
    };
    let fill_bf = |f: &mut Function| {
        let edge = f.block_call(bj, &[x]);
        f.append_inst(bf, Opcode::Jmp, &[], &[], Aux::Jump(edge));
    };
    let fill_entry = |f: &mut Function| {
        let t = f.block_call(bt, &[]);
        let e = f.block_call(bf, &[]);
        let (_, r) = f.append_inst(entry, Opcode::Icmp, &[x, x], &[BOOL], Aux::IntCc(IntCc::Eq));
        let (_, r2) = f.append_inst(entry, Opcode::Bconst, &[], &[BOOL], Aux::Bool(true));
        let _ = (r, r2, c);
        f.append_inst(entry, Opcode::Br, &[c], &[], Aux::Br(t, e));
    };
    let fill_bj = |f: &mut Function| {
        f.append_inst(bj, Opcode::Ret, &[jparam], &[], Aux::None);
    };
    if order_flipped {
        fill_bf(&mut f);
        fill_bj(&mut f);
        fill_bt(&mut f);
        fill_entry(&mut f);
    } else {
        fill_entry(&mut f);
        fill_bt(&mut f);
        fill_bf(&mut f);
        fill_bj(&mut f);
    }
    m.add_func(f);
    let text = wolf_wir::print_module(&m);
    (m, text)
}

#[test]
fn insertion_order_does_not_change_the_dump() {
    let (m1, d1) = build(false);
    let (m2, d2) = build(true);
    verify_module(&m1).expect("order A verifies");
    verify_module(&m2).expect("order B verifies");
    assert_eq!(d1, d2, "canonical numbering must erase insertion order");
    // And the canonical dump is itself a fixpoint.
    let re = wolf_wir::parse_module(&d1).expect("canonical dump parses");
    assert_eq!(wolf_wir::print_module(&re), d1);
}
