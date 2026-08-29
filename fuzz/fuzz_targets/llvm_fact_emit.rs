//! s41: the fact-emission structural fuzz target. Seeds drive the
//! scoped-noalias program generator; every program must verify, emit
//! deterministically on both lanes, keep the stripped lane honest, and
//! never contain an unwind construct (D30). The full DIFFERENTIAL rig
//! (compile at -O0/-O2/-O3 and run) lives in
//! `wolf_codegen_llvm/tests/fact_fuzz.rs` — clang invocations do not
//! belong inside a libfuzzer loop; this target covers the emitter's
//! own invariants at fuzzing throughput.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut seed = [0u8; 8];
    for (i, b) in data.iter().take(8).enumerate() {
        seed[i] = *b;
    }
    let seed = u64::from_le_bytes(seed);
    let m = wolf_codegen_llvm::fuzzgen::random_program(seed);
    wolf_wir::verify_module(&m).expect("generated program verifies");
    let on = emit(&m, false);
    let off = emit(&m, true);
    assert_eq!(on, emit(&m, false), "emission must be deterministic");
    for bad in ["invoke ", "landingpad", "personality"] {
        assert!(!on.contains(bad), "unwind construct emitted");
    }
    for fact in ["!alias.scope", "!invariant.load", "!range", "!prof"] {
        assert!(!off.contains(fact), "stripped lane carries {fact}");
    }
});

fn emit(m: &wolf_wir::Module, strip_facts: bool) -> String {
    use wolf_backend::Backend;
    let mut backend =
        wolf_codegen_llvm::LlvmBackend::with_options(wolf_codegen_llvm::EmitOptions {
            strip_facts,
            // This target fuzzes the FACT channel; profile weights are a
            // separate channel with its own lane, and leaving them on
            // would make the strip differential compare two things.
            branch_weights: None,
            // Host target (the backend refuses unsupported hosts).
            target: None,
        })
        .expect("backend");
    for (id, f) in m.funcs.iter() {
        let symbol = wolf_backend::mangle(m, &f.name, f.sig);
        backend
            .declare_function(m, id, &f.name, &symbol, f.sig, wolf_backend::Linkage::Local)
            .expect("declare");
    }
    for (id, f) in m.funcs.iter() {
        backend
            .define_function(m, id, f, &mut wolf_backend::NullDebugSink)
            .expect("define");
    }
    backend.module_ir()
}
