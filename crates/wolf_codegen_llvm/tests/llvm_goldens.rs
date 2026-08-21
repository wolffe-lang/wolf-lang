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

/// s97 (#112): the indirect call — `func.addr` yields the defined
/// symbol as a ptr operand, and the call line goes through the VALUE
/// (`call i64 %0(...)`), not an `@name`. Provenance note (target 7b):
/// under PIC both tiers hand the same linker-resolved address around,
/// and the exec witness on the clif tier plus the differential on the
/// corpus witnesses hold the two backends to one behavior.
#[test]
fn llvm_call_ind() {
    if let Some(t) = ir_of_fixture("call_ind") {
        let call = t
            .lines()
            .find(|l| l.trim_start().starts_with('%') && l.contains(" = call i64 %"))
            .expect("the indirect call goes through a VALUE, not an @name");
        assert!(
            !call.contains('@'),
            "an indirect call must not name a symbol: {call}"
        );
        insta::assert_snapshot!("llvm_call_ind", t);
    }
}

/// s101: the buffer-pointer alignment fact and its drops. @sum's
/// Header-role ptr load feeds only element addresses, so its VALUE
/// carries `!align !{i64 16}` (the base fact the vectorizer derives
/// widened-access alignment from — per-access `align` stays natural).
/// @leaked (value escapes into a call) and @edge (value crosses a
/// branch edge) are the SAME load shape with provenance broken — the
/// claim must drop. The dyn_dispatch golden pins the third drop (a
/// call.ind callee) by staying byte-stable.
#[test]
fn llvm_align_facts() {
    if let Some(t) = ir_of_fixture("align_facts") {
        let claims = t.matches("!align").count();
        // @sum's buffer load and no other — @leaked's and @edge's
        // identical loads must not carry it.
        assert_eq!(
            claims, 1,
            "exactly one load claims the buffer alignment: {t}"
        );
        assert!(t.contains("!{i64 16}"), "the claim is 16: {t}");
        insta::assert_snapshot!("llvm_align_facts", t);
    }
}

/// s96 (`[abi.native.dyn]`): the dyn dispatch chain on the LLVM tier.
/// Same no-symbol assertion as `llvm_call_ind` — the dispatch call
/// goes through the slot-loaded VALUE; a `@name` here would mean the
/// emitter re-derived a static callee for an erased type.
#[test]
fn llvm_dyn_dispatch() {
    if let Some(t) = ir_of_fixture("dyn_dispatch") {
        let call = t
            .lines()
            .find(|l| l.trim_start().starts_with('%') && l.contains(" = call i64 %"))
            .expect("the dispatch call goes through a VALUE, not an @name");
        assert!(
            !call.contains('@'),
            "a dyn dispatch call must not name a symbol: {call}"
        );
        insta::assert_snapshot!("llvm_dyn_dispatch", t);
    }
}

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

/// s86: concurrency on the release tier.
///
/// `func.addr` is the compiled task entry's address, and until s86 it
/// was this backend's last standing refusal — it took every program
/// containing a `spawn` with it, so `conform-run --native --release`
/// recorded `unsupported` at `wir` for the whole conc corpus. It
/// lowers as a link-time constant, exactly like `data.addr`: the
/// symbol IS the `ptr` operand, so no instruction is emitted for it.
/// The fixture is the spawn-under-a-loop shape, so the golden also
/// shows the per-iteration capture record (`region.alloc` inside the
/// loop) reaching `__wolf_rt_scope_spawn`.
#[test]
fn llvm_task_env_loop() {
    if let Some(t) = ir_of_fixture("task_env_loop") {
        // The address handed to the runtime must be the very symbol
        // this module defines — a `declare`d stand-in would link, run,
        // and call the wrong body.
        let sym = t
            .lines()
            .find_map(|l| l.strip_prefix("define internal i64 @"))
            .and_then(|l| l.split_once('(').map(|(s, _)| s.trim().to_string()))
            .expect("the task entry is defined in this module");
        let spawn = t
            .lines()
            .find(|l| l.contains("@\"__wolf_rt_scope_spawn\"("))
            .expect("the spawn call is emitted");
        assert!(
            spawn.contains(&format!("ptr @{sym},")),
            "func.addr must pass the DEFINED symbol {sym}: {spawn}"
        );
        insta::assert_snapshot!("llvm_task_env_loop", t);
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

/// s78 acceptance (wolf-lang#82): EVERY load and store through a
/// region-tagged pointer carries scope metadata — including the ones
/// whose pointer was itself loaded out of region memory, which is what
/// every container access looks like after s75. Machine-checked as a
/// lower bound over every module the tier can emit: one annotated
/// memory line per region-tokened memory op (an aggregate op lowers to
/// several, hence `>=`), and nothing silently losing its scopes.
///
/// The region SCOPE is what the emitter can honestly claim here.
/// Per-container disjointness is NOT claimed and must not be: two
/// `List` values may share one buffer (`let b = a` copies a header
/// pointer), so the buffers of two distinct containers are not a
/// theorem anyone proved — see the `#82` note in `emit.rs`.
#[test]
fn every_region_access_carries_scopes() {
    let mut total = 0usize;
    for (name, m) in all_modules() {
        let Some(ir) = ir_of(&m) else { return };
        let mut expect = 0usize;
        for (_, f) in m.funcs.iter() {
            for b in wolf_wir::block_order(f) {
                for &i in &f.blocks[b].insts {
                    let tok = match f.insts[i].op {
                        Opcode::Load => f.vpool.get(f.insts[i].args).get(1).copied(),
                        Opcode::Store => f.vpool.get(f.insts[i].args).get(2).copied(),
                        _ => None,
                    };
                    if let Some(tok) = tok
                        && matches!(
                            m.types.get(f.value_ty(tok)),
                            wolf_wir::types::TypeData::Mem(_)
                        )
                    {
                        expect += 1;
                    }
                }
            }
        }
        let got = ir
            .lines()
            .filter(|l| {
                l.contains("!alias.scope") && (l.contains(" load ") || l.contains("  store "))
            })
            .count();
        assert!(
            got >= expect,
            "{name}: {expect} region-tokened memory op(s) but only {got} \
             annotated line(s):\n{ir}"
        );
        total += expect;
    }
    assert!(total > 0, "the guard must have something to guard");
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
        let Some(ir) = module_ir(
            &m,
            None,
            EmitOptions {
                strip_facts: true,
                ..EmitOptions::default()
            },
        ) else {
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

/// s83 acceptance: the call-site `!noalias` fact s78 declined, in the
/// half that has a theorem — emitted where the callee provably cannot
/// reach the region, and NOT emitted where it can.
///
/// Both directions in one test on purpose. A fact that is never emitted
/// passes any "is it sound?" check trivially, and a fact that is always
/// emitted passes any "is it useful?" check trivially; the pair is the
/// only thing that pins the boundary. The dynamic half is
/// `fact_fuzz.rs`'s `call_noalias_local` / `call_escaped_pointer`, on
/// both the strip axis and the optimized-vs-unoptimized axis.
#[test]
fn a_call_claims_only_the_regions_it_cannot_reach() {
    let claim = fuzzgen::shape_call_noalias_local();
    let Some(ir) = ir_of(&claim) else { return };
    let calls: Vec<&str> = ir
        .lines()
        .filter(|l| l.contains("call ") && l.contains("@\"") && !l.contains("declare"))
        .collect();
    assert!(
        calls.iter().any(|l| l.contains("!noalias")),
        "call_noalias_local: no call carries the fact — a region the \
         callee was never given must be claimed:\n{ir}"
    );

    // The guard: every region in the escaped shape is reachable through
    // the pointer that crossed, so NO call may claim anything.
    let guard = fuzzgen::shape_call_escaped_pointer();
    let Some(ir) = ir_of(&guard) else { return };
    for l in ir.lines() {
        if l.contains("call ") && l.contains("@\"writer\"") {
            assert!(
                !l.contains("!noalias"),
                "call_escaped_pointer: the call claims a region whose \
                 pointer it was HANDED — that is the #92 miscompile:\n{l}"
            );
        }
    }
}
