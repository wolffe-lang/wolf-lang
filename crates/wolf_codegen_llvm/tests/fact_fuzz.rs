//! The scoped-noalias differential fuzz rig (s41 target 5 — the Rust
//! lesson, mechanized). Every program has KNOWN-TRUE aliasing facts by
//! construction; it is lowered twice — metadata-on and
//! metadata-stripped — and both lowerings are compiled at -O0/-O2/-O3
//! and RUN. All six executions must agree on (exit code, stdout,
//! trap identity). Any divergence is an LLVM bug or a lowering bug;
//! either way it blocks emission of that pattern until triaged.
//!
//! The permanent seed corpus is the three historical Rust-miscompile
//! shapes + the CFG-duplication stressor (report 10 amendment 1),
//! rebuilt in WIR terms in `wolf_codegen_llvm::fuzzgen`. The seeded
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
fn differential(tag: &str, mut module: wolf_wir::Module) -> bool {
    wolf_wir::verify_module(&module).expect("generated module verifies");
    let shim = add_plain_entry_shim(&mut module);
    let Some(ir_on) = module_ir(&module, Some(shim), EmitOptions { strip_facts: false }) else {
        return false;
    };
    let ir_off = module_ir(&module, Some(shim), EmitOptions { strip_facts: true })
        .expect("strip lane emits");
    let mut outcomes: Vec<(String, (i32, String, String))> = Vec::new();
    for (lane, ir) in [("meta", &ir_on), ("strip", &ir_off)] {
        for opt in OPT_LEVELS {
            let got = run_ir(&format!("fuzz_{tag}_{lane}_{}", &opt[1..]), ir, opt);
            outcomes.push((format!("{lane}{opt}"), got));
        }
    }
    let (ref base_name, ref base) = outcomes[0];
    for (name, got) in &outcomes[1..] {
        assert_eq!(
            (got.0, &got.1, &got.2),
            (base.0, &base.1, &base.2),
            "divergence on `{tag}`: {name} disagrees with {base_name}\n\
             --- annotated IR ---\n{ir_on}"
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
