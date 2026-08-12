//! Golden LLVM-IR snapshots (s41 target 6): per-fact `insta`
//! snapshots so reviewers see exactly which attributes/metadata each
//! wolf construct produces — lowering changes are reviewed diffs, and
//! diffs on LLVM bumps are visible artifacts.
//!
//! Also holds the s41 acceptance invariants that are cheapest as text
//! assertions over EVERY emitted module:
//! - deterministic emission (byte-identical IR, twice);
//! - no `invoke`/`landingpad`/personality anywhere (D30 — the CI
//!   grep-grade check);
//! - every define/declare `nounwind`;
//! - the stripped lane really is stripped (the fuzz control is
//!   honest).

mod common;

use common::{fixture, module_ir};
use wolf_codegen_llvm::{EmitOptions, fuzzgen};
use wolf_wir::entity::EntityRef;
use wolf_wir::facts::{DerefSize, FactData, FactKind, Just, Theorem};
use wolf_wir::ir::{Aux, Function, Mode, Module, Param};
use wolf_wir::ops::Opcode;
use wolf_wir::types::{self, RegionId};

fn ir_of(m: &Module) -> Option<String> {
    module_ir(m, None, EmitOptions::default())
}

fn ir_of_fixture(name: &str) -> Option<String> {
    let m = fixture(name);
    ir_of(&m)
}

// ---- per-fixture goldens (mirror the debug tier's clif_goldens) -----------

#[test]
fn llvm_overflow() {
    if let Some(t) = ir_of_fixture("overflow") {
        insta::assert_snapshot!("llvm_overflow", t);
    }
}

#[test]
fn llvm_intdot_range() {
    if let Some(t) = ir_of_fixture("intdot_range") {
        insta::assert_snapshot!("llvm_intdot_range", t);
    }
}

#[test]
fn llvm_exclusivity() {
    if let Some(t) = ir_of_fixture("exclusivity") {
        insta::assert_snapshot!("llvm_exclusivity", t);
    }
}

#[test]
fn llvm_qmark_defer() {
    if let Some(t) = ir_of_fixture("qmark_defer") {
        insta::assert_snapshot!("llvm_qmark_defer", t);
    }
}

#[test]
fn llvm_region_freeze_ok() {
    if let Some(t) = ir_of_fixture("region_freeze_ok") {
        insta::assert_snapshot!("llvm_region_freeze_ok", t);
    }
}

#[test]
fn llvm_region_infer_tree_transform() {
    if let Some(t) = ir_of_fixture("region_infer_tree_transform") {
        insta::assert_snapshot!("llvm_region_infer_tree_transform", t);
    }
}

// ---- per-fact goldens ------------------------------------------------------

/// `mut`/`read` param modes + deref/noalias/frozen theorem facts →
/// `noalias`/`readonly`/`noundef`/`dereferenceable(n)` argument
/// attributes; the frozen pointer's load gets `!invariant.load`.
fn param_facts_module() -> Module {
    let mut m = Module::new();
    let tok = m.types.mem(RegionId::new(0));
    let sig = m.make_sig(
        vec![
            Param {
                ty: types::PTR,
                mode: Mode::Mut,
            },
            Param {
                ty: types::PTR,
                mode: Mode::Read,
            },
            Param::val(tok),
        ],
        vec![types::I64],
    );
    let mut f = Function::new("param_facts", sig);
    let b0 = f.make_block(&[types::PTR, types::PTR, tok]);
    let ps = f.block_params(b0);
    let (p, q, t) = (ps[0], ps[1], ps[2]);
    f.add_fact(FactData::new(
        FactKind::Deref(p, DerefSize::Const(8)),
        Just::Theorem(Theorem::ExclMut),
    ));
    f.add_fact(FactData::new(
        FactKind::Deref(q, DerefSize::Const(8)),
        Just::Theorem(Theorem::FrozenRead),
    ));
    f.add_fact(FactData::new(
        FactKind::Noalias(p, q),
        Just::Theorem(Theorem::ExclMut),
    ));
    f.add_fact(FactData::new(
        FactKind::Frozen(q),
        Just::Theorem(Theorem::FrozenRead),
    ));
    let (_, a) = f.append_inst(b0, Opcode::Load, &[p, t], &[types::I64], Aux::None);
    let (_, b) = f.append_inst(b0, Opcode::Load, &[q, t], &[types::I64], Aux::None);
    let (_, r) = f.append_inst(
        b0,
        Opcode::IaddWrap,
        &[a[0], b[0]],
        &[types::I64],
        Aux::None,
    );
    f.append_inst(b0, Opcode::Ret, &[r[0]], &[], Aux::None);
    m.add_func(f);
    m
}

#[test]
fn llvm_param_fact_attributes() {
    let m = param_facts_module();
    wolf_wir::verify_module(&m).expect("verifies");
    if let Some(t) = ir_of(&m) {
        assert!(t.contains("ptr noalias noundef dereferenceable(8) %p0"));
        assert!(t.contains("ptr noalias readonly noundef dereferenceable(8) %p1"));
        assert!(t.contains("!invariant.load"));
        insta::assert_snapshot!("llvm_param_fact_attributes", t);
    }
}

/// A branch-refined `range` fact on a load → `!range` metadata
/// (half-open per LangRef).
fn range_fact_module() -> Module {
    let mut m = Module::new();
    let tok = m.types.mem(RegionId::new(0));
    let sig = m.make_sig(
        vec![Param::val(types::PTR), Param::val(tok)],
        vec![types::I64],
    );
    let mut f = Function::new("ranged", sig);
    let b0 = f.make_block(&[types::PTR, tok]);
    let ps = f.block_params(b0);
    let (_, v) = f.append_inst(b0, Opcode::Load, &[ps[0], ps[1]], &[types::I64], Aux::None);
    f.add_fact(FactData::new(
        FactKind::Range(v[0], 0, 9),
        Just::Theorem(Theorem::BoundsBr),
    ));
    f.append_inst(b0, Opcode::Ret, &[v[0]], &[], Aux::None);
    m.add_func(f);
    m
}

#[test]
fn llvm_range_fact_metadata() {
    let m = range_fact_module();
    wolf_wir::verify_module(&m).expect("verifies");
    if let Some(t) = ir_of(&m) {
        assert!(t.contains("!range"), "range fact must reach LLVM:\n{t}");
        assert!(t.contains("!{i64 0, i64 10}"), "half-open bounds:\n{t}");
        insta::assert_snapshot!("llvm_range_fact_metadata", t);
    }
}

/// Region scopes + invariant loads + the cold error/trap arcs, on the
/// fuzz rig's permanent shapes (the densest fact patterns we emit).
#[test]
fn llvm_shape_licm_scopes() {
    let m = fuzzgen::shape_licm_scopes();
    if let Some(t) = ir_of(&m) {
        assert!(t.contains("!alias.scope"));
        assert!(t.contains("!noalias"));
        assert!(t.contains("!invariant.load"));
        assert!(t.contains("align 16 dereferenceable(8)") || t.contains("noalias align 16"));
        insta::assert_snapshot!("llvm_shape_licm_scopes", t);
    }
}

#[test]
fn llvm_shape_unroll_scopes() {
    let m = fuzzgen::shape_unroll_scopes();
    if let Some(t) = ir_of(&m) {
        insta::assert_snapshot!("llvm_shape_unroll_scopes", t);
    }
}

// ---- acceptance invariants over every module we can emit ------------------

fn all_modules() -> Vec<(String, Module)> {
    let mut out: Vec<(String, Module)> = [
        "overflow",
        "intdot_range",
        "exclusivity",
        "qmark_defer",
        "region_freeze_ok",
        "region_infer_tree_transform",
    ]
    .iter()
    .map(|n| (n.to_string(), fixture(n)))
    .collect();
    for (n, m) in fuzzgen::historical_shapes() {
        out.push((n.to_string(), m));
    }
    out.push(("param_facts".into(), param_facts_module()));
    out.push(("range_fact".into(), range_fact_module()));
    for seed in 1..=4 {
        out.push((format!("seed{seed}"), fuzzgen::random_program(seed)));
    }
    out
}

/// Byte-identical IR for identical input, twice (s41 acceptance:
/// domain assignment and everything else is deterministic).
#[test]
fn emission_is_deterministic() {
    for (name, m) in all_modules() {
        let Some(a) = ir_of(&m) else { return };
        let b = ir_of(&m).unwrap();
        assert_eq!(a, b, "{name}: emission must be deterministic");
    }
}

/// D30: no `invoke`, no `landingpad`, no personality, anywhere, ever —
/// and every define/declare carries `nounwind`. (The CI grep-grade
/// check from s41 target 2.)
#[test]
fn no_unwinding_constructs_ever() {
    for (name, m) in all_modules() {
        let Some(ir) = ir_of(&m) else { return };
        for bad in ["invoke ", "landingpad", "personality", "resume "] {
            assert!(!ir.contains(bad), "{name}: emitted `{bad}`:\n{ir}");
        }
        for line in ir.lines() {
            if line.starts_with("define") || line.starts_with("declare") {
                // Intrinsic declares carry no attrs (LLVM knows them);
                // everything else must be nounwind.
                if line.contains("@llvm.") {
                    continue;
                }
                assert!(line.contains("nounwind"), "{name}: not nounwind: {line}");
            }
        }
    }
}

/// The metadata-stripped control lane is honest: no fact channel
/// survives it (ABI attributes like sret/byval may).
#[test]
fn strip_lane_is_stripped() {
    for (name, m) in all_modules() {
        let Some(ir) = module_ir(&m, None, EmitOptions { strip_facts: true }) else {
            return;
        };
        for fact in [
            "!alias.scope",
            "!noalias",
            "!invariant.load",
            "!range",
            "!prof",
            "noalias",
            "readonly",
            "dereferenceable",
            "noundef",
            "align 16 ptr", // the region.alloc return-attr form
        ] {
            assert!(
                !ir.contains(fact),
                "{name}: stripped lane still carries `{fact}`:\n{ir}"
            );
        }
    }
}
