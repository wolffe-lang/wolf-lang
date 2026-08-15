//! The scoped-noalias differential fuzz rig (s41 target 5 — the Rust
//! lesson, mechanized). Every program has KNOWN-TRUE aliasing facts by
//! construction; it is lowered twice — metadata-on and
//! metadata-stripped — and both lowerings are compiled at -O0/-O2/-O3
//! and RUN. All six executions must agree on (exit code, stdout,
//! trap identity). Any divergence is an LLVM bug or a lowering bug;
//! either way it blocks emission of that pattern until triaged.
//!
//! The permanent seed corpus is the three historical Rust-miscompile
//! shapes + the CFG-duplication stressor (report 10 amendment 1), plus
//! the five shapes wolf found in ITS OWN fact rig — the loaded-pointer
//! case (s78), the two foreign-root cases (s80), and the two call-site
//! cases (s83) — rebuilt in WIR terms in
//! `wolf_codegen_llvm::fuzzgen`. The seeded
//! random lane runs a small budget in PR CI (part of the workspace
//! test wall); nightly/long runs raise `WOLF_FACT_FUZZ_N`. The
//! LLVM-version bump policy requires a clean long run first.
//!
//! This same stripped-lowering control is the s44 metadata-drop
//! sentinel's substrate (D42 ruling 3).

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

mod common;

use common::{add_plain_entry_shim, module_ir, run_ir};
use wolf_codegen_llvm::{EmitOptions, fuzzgen};

const OPT_LEVELS: [&str; 3] = ["-O0", "-O2", "-O3"];

/// Lower `module` both ways, compile each at every opt level, run all
/// six, and assert one behavior. Returns false when the host cannot
/// run the rig (skip loudly).
fn differential(tag: &str, module: wolf_wir::Module) -> bool {
    differential_lane(tag, module, false)
}

/// [`differential`] plus a MID-END lane: the same module optimized by
/// s42 is emitted and run alongside the unoptimized one, and all twelve
/// executions must agree (s80) — two phases × two metadata lanes ×
/// three opt levels.
///
/// The plain rig cannot witness an optimizer bug and it is worth being
/// exact about why: it compares metadata-on against metadata-stripped,
/// and a mid-end miscompile lands in BOTH lanes identically, so they
/// agree and the rig reports nothing. Optimized-against-unoptimized is
/// the axis that catches it — the same axis `WOLF_MIDEND=0` gives the
/// driver. The shim is added AFTER the mid-end runs: dead-function
/// elimination roots at `main` and exports, and the shim is neither.
fn differential_midend(tag: &str, module: wolf_wir::Module) -> bool {
    differential_lane(tag, module, true)
}

fn differential_lane(tag: &str, module: wolf_wir::Module, midend: bool) -> bool {
    wolf_wir::verify_module(&module).expect("generated module verifies");
    let mut lanes: Vec<(&str, wolf_wir::Module)> = vec![("raw", module.clone())];
    if midend {
        let mut opt = module;
        wolf_wir::midend::optimize_module(&mut opt, &wolf_wir::midend::Options::default())
            .expect("the mid-end optimizes the generated module");
        lanes.push(("mid", opt));
    }
    let mut outcomes: Vec<(String, (i32, String, String))> = Vec::new();
    let mut first_ir = String::new();
    for (phase, mut m) in lanes {
        let shim = add_plain_entry_shim(&mut m);
        let Some(ir_on) = module_ir(
            &m,
            Some(shim),
            EmitOptions {
                strip_facts: false,
                ..EmitOptions::default()
            },
        ) else {
            return false;
        };
        let ir_off = module_ir(
            &m,
            Some(shim),
            EmitOptions {
                strip_facts: true,
                ..EmitOptions::default()
            },
        )
        .expect("strip lane emits");
        if first_ir.is_empty() {
            first_ir = ir_on.clone();
        }
        for (lane, ir) in [("meta", &ir_on), ("strip", &ir_off)] {
            for opt in OPT_LEVELS {
                let got = run_ir(&format!("fuzz_{tag}_{phase}_{lane}_{}", &opt[1..]), ir, opt);
                outcomes.push((format!("{phase}/{lane}{opt}"), got));
            }
        }
    }
    let (ref base_name, ref base) = outcomes[0];
    for (name, got) in &outcomes[1..] {
        assert_eq!(
            (got.0, &got.1, &got.2),
            (base.0, &base.1, &base.2),
            "divergence on `{tag}`: {name} disagrees with {base_name}\n\
             --- annotated IR ---\n{first_ir}"
        );
    }
    true
}

/// The historical shapes are PERMANENT regression tests (s41
/// acceptance: both — now three + stressor — present and passing).
#[test]
fn historical_shape_inline_noalias() {
    differential("inline_noalias", fuzzgen::shape_inline_noalias());
}

#[test]
fn historical_shape_licm_scopes() {
    differential("licm_scopes", fuzzgen::shape_licm_scopes());
}

#[test]
fn historical_shape_unroll_scopes() {
    differential("unroll_scopes", fuzzgen::shape_unroll_scopes());
}

#[test]
fn cfg_duplication_stressor() {
    differential("cfg_duplication", fuzzgen::shape_cfg_duplication());
}

/// s78 (wolf-lang#82): the accessed pointer is READ OUT of another
/// region's memory — the container shape. The scopes on these loads and
/// stores are the new reach, so they get the same differential the
/// allocation-rooted ones have had since s41.
#[test]
fn loaded_pointer_scopes() {
    differential("loaded_pointer", fuzzgen::shape_loaded_pointer_scopes());
}

/// s80 (wolf-lang#83): two `region.foreign` roots of one role over one
/// piece of storage — the state inlining a container-touching callee
/// produces. Under one-scope-per-region this MISCOMPILED: the roots
/// were declared `!noalias` and LLVM forwarded a load across a store
/// into the same bytes. Both lanes, because the mid-end had the same
/// hole (`memopt`'s token versions do not cross chains).
#[test]
fn foreign_dup_roots_do_not_claim_disjoint() {
    differential("foreign_dup_roots", fuzzgen::shape_foreign_dup_roots());
}

/// The half that indicts the METADATA: two addresses LLVM cannot prove
/// equal, so it has to consult the scopes it was given.
#[test]
fn foreign_dup_roots_under_opaque_indices() {
    differential(
        "foreign_dup_roots_opaque",
        fuzzgen::shape_foreign_dup_roots_opaque_index(),
    );
}

#[test]
fn foreign_dup_roots_survive_the_midend() {
    differential_midend("foreign_dup_roots_mid", fuzzgen::shape_foreign_dup_roots());
}

/// s80: the NON-INLINING witness the audit owed. An opaque callee
/// writes the caller's foreign storage while consuming none of its
/// tokens; the caller's load around the call must not forward. The
/// mid-end lane is the one that matters — this is `memopt`'s and
/// `licm`'s claim, not the emitter's.
#[test]
fn foreign_storage_survives_a_non_inlining_call() {
    differential("foreign_cross_call", fuzzgen::shape_foreign_cross_call());
}

#[test]
fn foreign_storage_survives_a_non_inlining_call_under_the_midend() {
    differential_midend(
        "foreign_cross_call_mid",
        fuzzgen::shape_foreign_cross_call(),
    );
}

/// s80: `licm`'s half. The loop's foreign token is defined outside the
/// loop and consumed nowhere inside it, which is the pass's entire test
/// for hoisting a load — and the call in the body writes the bytes the
/// load reads. Loop-invariant TOKEN is not loop-invariant MEMORY.
#[test]
fn a_foreign_load_does_not_hoist_over_a_call() {
    differential_midend("foreign_licm_call", fuzzgen::shape_foreign_licm_call());
}

/// s83 (#92): the call-site `!noalias` fact's guard. The caller hands a
/// RAW POINTER into a local region across an opaque call and the callee
/// writes through it — no token changes hands, so both `memopt`'s "no
/// token ⇒ no effect" and the emitter's new call-site fact would license
/// forwarding the pre-call load. Neither may.
#[test]
fn an_escaped_pointer_defeats_the_call_site_fact() {
    differential(
        "call_escaped_pointer",
        fuzzgen::shape_call_escaped_pointer(),
    );
}

#[test]
fn an_escaped_pointer_defeats_the_call_site_fact_under_the_midend() {
    differential_midend(
        "call_escaped_pointer_mid",
        fuzzgen::shape_call_escaped_pointer(),
    );
}

/// s83: the same fact where it is TRUE. A second region that never
/// leaves the frame may be forwarded across the very call that writes
/// the first. Both lanes must agree — this is the shape that goes quiet
/// if the fact is dropped and LOUD if it is widened by one region.
#[test]
fn a_call_cannot_reach_a_region_it_was_never_given() {
    differential("call_noalias_local", fuzzgen::shape_call_noalias_local());
}

#[test]
fn a_call_cannot_reach_a_region_it_was_never_given_under_the_midend() {
    differential_midend(
        "call_noalias_local_mid",
        fuzzgen::shape_call_noalias_local(),
    );
}

/// The call-site witnesses only witness if the call SURVIVES, for the
/// same reason the s80 one does.
#[test]
fn the_call_site_witnesses_really_do_not_inline() {
    for (tag, mut m) in [
        (
            "call_escaped_pointer",
            fuzzgen::shape_call_escaped_pointer(),
        ),
        ("call_noalias_local", fuzzgen::shape_call_noalias_local()),
    ] {
        let stats =
            wolf_wir::midend::optimize_module(&mut m, &wolf_wir::midend::Options::default())
                .unwrap();
        assert_eq!(
            stats.inlined_calls, 0,
            "{tag} inlined, so it no longer witnesses anything — grow the padding"
        );
    }
}

/// The witness only witnesses if the call SURVIVES: an inlined callee
/// hides the very hazard under audit, which is how #83 went unfiled for
/// two sprints. Pin it.
#[test]
fn the_cross_call_witness_really_does_not_inline() {
    let mut m = fuzzgen::shape_foreign_cross_call();
    let stats =
        wolf_wir::midend::optimize_module(&mut m, &wolf_wir::midend::Options::default()).unwrap();
    assert_eq!(
        stats.inlined_calls, 0,
        "the s80 witness inlined, so it no longer witnesses anything — grow the padding"
    );
    let main = m
        .funcs
        .values()
        .find(|f| f.name == "main")
        .expect("witness has a main");
    let calls = main
        .layout
        .iter()
        .flat_map(|&b| main.blocks[b].insts.iter())
        .filter(|&&i| main.insts[i].op == wolf_wir::ops::Opcode::Call)
        .count();
    assert_eq!(calls, 1, "the call to @writer must survive into codegen");
}

/// The seeded random lane. PR CI runs a small budget; raise
/// `WOLF_FACT_FUZZ_N` for the nightly/bump-policy long run.
#[test]
fn random_differential_lane() {
    let n: u64 = std::env::var("WOLF_FACT_FUZZ_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6);
    for seed in 1..=n {
        if !differential(&format!("seed{seed}"), fuzzgen::random_program(seed)) {
            return; // host cannot run the rig; skipped loudly already
        }
    }
}
