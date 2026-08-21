//! s99: the interprocedural range channel, pinned at the WIR level.
//!
//! The positive fixture is the real a2/stencil program (a caller
//! pushing `i & 255` into src, a callee whose element-sum `iadd.chk`s
//! want that proof and whose `mut out` token parameter keeps it out
//! of line forever — the s42 v0 rule).
//!
//! The alias fixture is the SAME program with one edit: the loop
//! carries src's header as the `mut out` argument, so the callee
//! writes the container it reads. Wolf source cannot express this —
//! the mem tier rejects it as E1002 (the c04 exclusivity theorem:
//! "the same place twice is never disjoint"), measured while writing
//! the driver witnesses — so the channel's frame check is DEFENSE IN
//! DEPTH, and a hand-built WIR module is the only place it can be
//! witnessed. That is exactly why this test exists.

use wolf_wir::midend::{self, Options, interproc};

fn phase(text: &str) -> wolf_wir::Module {
    let mut m = wolf_wir::parse_module(text).expect("parse");
    let homes = midend::summary::Homes::single();
    midend::optimize_whole_program(&mut m, &homes, &Options::default()).expect("wp");
    m
}

fn fn_text(m: &wolf_wir::Module, name: &str) -> String {
    let ids: Vec<_> = m
        .funcs
        .iter()
        .filter(|(_, f)| f.name == name)
        .map(|(id, _)| id)
        .collect();
    wolf_wir::print_selected(m, &ids)
}

#[test]
fn the_stencil_shape_discharges_its_element_sums() {
    let m = phase(include_str!("fixtures/interproc_a2.wir"));
    let s = fn_text(&m, "stencil");
    assert!(
        s.contains("summary."),
        "the channel should mint summary facts into stencil:\n{s}"
    );
    // The two element-sum adds discharge; the `len - 1` isub stays
    // (len is unbounded — the honest cold residue, same as the real
    // kernel).
    assert!(
        !s.contains("iadd.chk"),
        "the element sums should discharge to wrap form:\n{s}"
    );
    assert!(
        s.contains("iadd.wrap"),
        "the discharged adds should still exist as wrap ops:\n{s}"
    );
}

#[test]
fn the_aliased_variant_refuses_the_grant() {
    let m = phase(include_str!("fixtures/interproc_a2_alias.wir"));
    let s = fn_text(&m, "stencil");
    assert!(
        s.contains("iadd.chk"),
        "a callee writing the container it reads must keep its checks:\n{s}"
    );
    assert!(
        !s.contains("summary."),
        "no summary fact may mint into an aliased callee:\n{s}"
    );
}

#[test]
fn analyze_grants_match_the_phase() {
    // The analysis result is inspectable on its own: the positive
    // fixture grants stencil's src param [0,255]; the alias fixture
    // grants nothing.
    let m = wolf_wir::parse_module(include_str!("fixtures/interproc_a2.wir")).expect("parse");
    let ipr = interproc::analyze(&m);
    let g = ipr.grants.get("stencil").expect("stencil granted");
    assert_eq!(g.get(&2), Some(&(0, 255)), "src param grant");
    let m2 =
        wolf_wir::parse_module(include_str!("fixtures/interproc_a2_alias.wir")).expect("parse");
    let ipr2 = interproc::analyze(&m2);
    assert!(
        !ipr2.grants.contains_key("stencil"),
        "no grant for the aliased stencil: {:?}",
        ipr2.grants
    );
}

#[test]
fn reverify_rejects_a_tampered_fact() {
    use wolf_wir::{FactData, FactKind, Just};
    let mut m = wolf_wir::parse_module(include_str!("fixtures/interproc_a2.wir")).expect("parse");
    // Mint honestly first, then tamper: widen... a NARROWER fact than
    // the proof is fine; a fact with NO proof must be rejected. Claim
    // a range on a value nothing proves (the len load in stencil).
    let ipr = interproc::analyze(&m);
    let _ = interproc::mint(&mut m, &ipr, 0xfeed);
    assert!(interproc::reverify(&m).is_ok(), "honest facts re-derive");
    let fid = m
        .funcs
        .iter()
        .find(|(_, f)| f.name == "stencil")
        .map(|(id, _)| id)
        .expect("stencil");
    // Forge on the first LOAD result of stencil's entry block — the
    // src length, a value neither the interprocedural proof map nor
    // the local evaluator can bound (a loaded i64 is the type's whole
    // range). Found structurally, not by a hard-coded id.
    let forged = {
        let f = &m.funcs[fid];
        let e = f.entry().expect("entry");
        f.blocks[e]
            .insts
            .iter()
            .find_map(|&ii| {
                (f.insts[ii].op == wolf_wir::Opcode::Load)
                    .then(|| f.vpool.get(f.insts[ii].results).first().copied())
                    .flatten()
                    .filter(|&r| m.types.int_bounds(f.value_ty(r)).is_some())
            })
            .expect("an int load in stencil's entry")
    };
    let f = &mut m.funcs[fid];
    f.add_fact(FactData::new(
        FactKind::Range(forged, 0, 10),
        Just::Summary(0xfeed),
    ));
    assert!(
        interproc::reverify(&m).is_err(),
        "a summary fact without a proof must be rejected"
    );
}
