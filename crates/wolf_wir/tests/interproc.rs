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

#[test]
fn a_zero_index_element_access_is_still_an_element() {
    // The a5 shape (the s104 regression, ritual 1.011x → 0.603x): the
    // callee reads `src[0]` — a zero-INDEX element access, ptr.off
    // with const 0 at scale 8 — and stores to `scratch[0]`, the same
    // shape on the write side. c3b8b2a's identity rule keyed on the
    // offset alone, so both stopped classifying as elements: the load
    // lost its grant, the store went unattributable, and three checked
    // adds returned to the hot loop. The identity is scale-1 ONLY.
    let m = wolf_wir::parse_module(include_str!("fixtures/interproc_a5.wir")).expect("parse");
    let ipr = interproc::analyze(&m);
    let g = ipr.grants.get("probe").expect("probe granted");
    assert_eq!(g.get(&2), Some(&(0, 255)), "src param grant at index 0");
    let m2 = phase(include_str!("fixtures/interproc_a5.wir"));
    let s = fn_text(&m2, "probe");
    assert!(
        s.contains("summary."),
        "the channel should mint into probe:\n{s}"
    );
    assert!(
        !s.contains("iadd.chk"),
        "the element sum should discharge:\n{s}"
    );
}

/// wolf-lang#940's shape at the WIR level: `main` makes a list, passes
/// its slot as `mut` to `r<hops>`, each relay `r<k>` hands its own
/// `mut` parameter straight to `r<k-1>` and `r1` to `sink`; then `main`
/// stores 7 into element 0 itself and loads it back. `sink` is either
/// a module writer storing 6 into element 0 or an unknown external.
/// Nothing here is optimized: the analysis reads the bodies as built.
fn relay_chain(hops: usize, external_sink: bool) -> String {
    let mut t = String::new();
    t.push_str("decl @__wolf_rt_list_new(i64, mem.r0, mem.r1) -> ptr\n");
    t.push_str("decl @__wolf_rt_print_i64(i64)\n");
    if external_sink {
        t.push_str("decl @sink(ptr)\n");
    } else {
        t.push_str("decl @sink(mut ptr, mem.r0)\n");
    }
    for k in 1..=hops {
        t.push_str(&format!("decl @r{k}(mut ptr, mem.r0)\n"));
    }
    t.push('\n');
    if !external_sink {
        t.push_str(
            "fn @sink(mut ptr, mem.r0) {\n\
             b0(%0: ptr, %1: mem.r0):\n  \
             %2: mem.r2 = region.foreign 1\n  \
             %3: mem.r1 = region.foreign 0\n  \
             %4 = load.ptr %0, %1\n  \
             %5 = load.ptr %4, %3\n  \
             %6 = iconst.i64 0\n  \
             %7 = ptr.off %5, %6, 8\n  \
             %8 = iconst.i64 6\n  \
             %9 = store.i64 %8, %7, %2\n  \
             ret\n}\n\n",
        );
    }
    for k in 1..=hops {
        let callee = if k == 1 {
            "sink".to_string()
        } else {
            format!("r{}", k - 1)
        };
        let call = if k == 1 && external_sink {
            "  call @sink(%0)\n".to_string()
        } else {
            format!("  %2 = call @{callee}(%0, %1)\n")
        };
        t.push_str(&format!(
            "fn @r{k}(mut ptr, mem.r0) {{\nb0(%0: ptr, %1: mem.r0):\n{call}  ret\n}}\n\n"
        ));
    }
    t.push_str(&format!(
        "fn @main() {{\n\
         b0:\n  \
         %0: mem.r1 = region.foreign 1\n  \
         %1: mem.r0 = region.foreign 0\n  \
         %2 = iconst.i64 8\n  \
         %3, %4, %5 = call @__wolf_rt_list_new(%2, %1, %0)\n  \
         %6: ptr, %7: mem.r2 = stack.alloc %2\n  \
         %8 = store.ptr %3, %6, %7\n  \
         %9 = call @r{hops}(%6, %8)\n  \
         %10 = load.ptr %6, %9\n  \
         %11 = load.ptr %10, %4\n  \
         %12 = iconst.i64 0\n  \
         %13 = ptr.off %11, %12, 8\n  \
         %14 = iconst.i64 7\n  \
         %15 = store.i64 %14, %13, %5\n  \
         %16 = load.i64 %13, %15\n  \
         call @__wolf_rt_print_i64(%16)\n  \
         ret\n}}\n"
    ));
    t
}

fn main_elems(text: &str) -> Option<(i128, i128)> {
    let m = wolf_wir::parse_module(text).expect("parse");
    let ipr = interproc::analyze(&m);
    ipr.local_elems.get("main").and_then(|e| e.get(&0)).copied()
}

#[test]
fn a_relayed_mut_param_carries_the_callees_writes() {
    // One relay: before #940's fix the relay's `mut` parameter handed
    // on was opaque, the writer's 6 never reached `main`, and `main`'s
    // element load was proved to be 7.
    assert_eq!(main_elems(&relay_chain(1, false)), Some((6, 7)));
}

#[test]
fn a_mut_relay_chain_longer_than_the_widen_passes_still_closes() {
    // Five relays: one round of pass 2 carries an effect one hop, so a
    // chain past WIDEN_PASSES only reaches `main` through the widening.
    const { assert!(interproc::WIDEN_PASSES < 5) };
    let got = main_elems(&relay_chain(5, false));
    assert_ne!(got, Some((7, 7)), "the writer's 6 was lost on the way up");
    assert!(
        got.is_none_or(|(lo, hi)| lo <= 6 && hi >= 7),
        "main's elements must cover both stores: {got:?}"
    );
}

#[test]
fn a_mut_param_reaching_an_unknown_external_kills_the_callers_list() {
    // The relay hands its `mut` slot to a function the channel cannot
    // see into: whatever it does to the list, `main` may not claim 7.
    assert_eq!(main_elems(&relay_chain(1, true)), None);
}
