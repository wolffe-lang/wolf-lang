//! LIFO delete (truncate-to-mark) across a whole function's arenas —
//! unused by s24 itself, load-bearing for s25's speculative
//! construction (Click §5), so it is designed and tested NOW (s24
//! acceptance): build N instructions, mark, build M more (plus a new
//! block, facts, and an import), roll back, and assert arena lengths
//! and table state are exactly restored.

use wolf_wir::types::{BOOL, I64, PTR};
use wolf_wir::{
    Aux, DerefSize, FactData, FactKind, Function, IntCc, Just, Module, Opcode, Param, Theorem,
    verify_module,
};

#[test]
fn truncate_to_mark_restores_everything_exactly() {
    let mut m = Module::new();
    let sig = m.make_sig(vec![Param::val(I64), Param::val(PTR)], vec![I64]);
    let mut f = Function::new("spec", sig);
    let entry = f.make_block(&[I64, PTR]);
    let params = f.block_params(entry);
    let (n, p) = (params[0], params[1]);
    // N = 3 instructions before the mark.
    let (_, c1) = f.append_inst(entry, Opcode::Iconst, &[], &[I64], Aux::Int(7));
    let (_, s) = f.append_inst(entry, Opcode::IaddChk, &[n, c1[0]], &[I64], Aux::None);
    let (_, _cmp) = f.append_inst(
        entry,
        Opcode::Icmp,
        &[s[0], n],
        &[BOOL],
        Aux::IntCc(IntCc::Slt),
    );
    f.add_fact(FactData::new(
        FactKind::Deref(p, DerefSize::Const(8)),
        Just::Theorem(Theorem::ExclMut),
    ));

    let lens_before = (
        f.blocks.len(),
        f.insts.len(),
        f.values.len(),
        f.facts.len(),
        f.ext_funcs.len(),
        f.vpool.len(),
        f.layout.len(),
    );
    let snapshot = format!("{f:?}");
    let mark = f.mark();

    // M = speculative work: a new block, instructions appended to BOTH
    // the old and the new block, a fact, and a callee import.
    let spec = f.make_block(&[I64]);
    let (_, c2) = f.append_inst(entry, Opcode::Iconst, &[], &[I64], Aux::Int(99));
    let edge = f.block_call(spec, &[c2[0]]);
    f.append_inst(entry, Opcode::Jmp, &[], &[], Aux::Jump(edge));
    let sp = f.block_params(spec)[0];
    f.append_inst(spec, Opcode::Ret, &[sp], &[], Aux::None);
    f.add_fact(FactData::new(FactKind::Range(c2[0], 99, 99), Just::DefOp));
    let helper_sig = m.make_sig(vec![], vec![]);
    f.import_func("helper", helper_sig);
    assert_ne!(
        snapshot,
        format!("{f:?}"),
        "speculation changed the function"
    );

    // Roll back: exact restoration, deep-compared.
    f.truncate_to_mark(mark);
    let lens_after = (
        f.blocks.len(),
        f.insts.len(),
        f.values.len(),
        f.facts.len(),
        f.ext_funcs.len(),
        f.vpool.len(),
        f.layout.len(),
    );
    assert_eq!(lens_before, lens_after, "arena lengths restored");
    assert_eq!(snapshot, format!("{f:?}"), "table state restored exactly");

    // The rolled-back function is still buildable: entity indices are
    // re-minted from the mark point, and the finished function
    // verifies and prints.
    let (_, z) = f.append_inst(entry, Opcode::Iconst, &[], &[I64], Aux::Int(0));
    f.append_inst(entry, Opcode::Ret, &[z[0]], &[], Aux::None);
    m.add_func(f);
    verify_module(&m).expect("post-rollback function verifies");
    let d1 = wolf_wir::print_module(&m);
    let re = wolf_wir::parse_module(&d1).expect("dump parses");
    assert_eq!(wolf_wir::print_module(&re), d1);
}
