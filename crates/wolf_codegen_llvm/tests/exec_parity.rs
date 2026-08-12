//! Release-tier machine-code execution (s41 acceptance): WIR → LLVM
//! IR → clang -O2 → a real process, asserted on exit codes and trap
//! identities — the SAME outcomes the debug tier and the reference
//! interpreter produce (D2: tiers behaviorally identical; X3: checked
//! arithmetic traps identically in release, only the speed changes).

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

mod common;

use common::{add_plain_entry_shim, fixture, module_ir, run_ir};
use wolf_codegen_llvm::EmitOptions;

fn run_fixture(name: &str, opt: &str) -> Option<(i32, String)> {
    let mut m = fixture(name);
    let shim = add_plain_entry_shim(&mut m);
    let ir = module_ir(&m, Some(shim), EmitOptions::default())?;
    let (code, _, stderr) = run_ir(&format!("exec_{name}_{}", &opt[1..]), &ir, opt);
    Some((code, stderr))
}

/// The contract's acceptance fixture: branches, calls, structs, and
/// checked arithmetic, through clang -O2, exiting correctly.
#[test]
fn tree_transform_runs_to_zero() {
    for opt in ["-O0", "-O2"] {
        let Some((code, _)) = run_fixture("region_infer_tree_transform", opt) else {
            return;
        };
        assert_eq!(code, 0, "at {opt}");
    }
}

/// X3 in release: the overflow fixture traps with the SAME identity
/// the debug tier reports (trap kind survives -O2).
#[test]
fn overflow_traps_identically() {
    for opt in ["-O0", "-O2"] {
        let Some((code, stderr)) = run_fixture("overflow", opt) else {
            return;
        };
        assert_eq!(code, 134, "trap exit code at {opt}");
        assert!(
            stderr.contains("wolf-trap: overflow"),
            "trap identity at {opt}: {stderr}"
        );
    }
}

#[test]
fn qmark_defer_runs() {
    let Some((code, _)) = run_fixture("qmark_defer", "-O2") else {
        return;
    };
    assert_eq!(code, 0);
}

#[test]
fn region_freeze_runs() {
    let Some((code, _)) = run_fixture("region_freeze_ok", "-O2") else {
        return;
    };
    assert_eq!(code, 0);
}

#[test]
fn exclusivity_fixture_runs() {
    let Some((code, _)) = run_fixture("exclusivity", "-O2") else {
        return;
    };
    assert_eq!(code, 0);
}

#[test]
fn intdot_range_runs() {
    let Some((code, _)) = run_fixture("intdot_range", "-O2") else {
        return;
    };
    assert_eq!(code, 0);
}

/// The finished-object path (Backend::finish → clang -c): relocatable
/// bytes come back non-empty and ELF-shaped.
#[test]
fn object_emission_produces_elf() {
    use wolf_backend::Backend;
    let mut m = fixture("region_infer_tree_transform");
    let shim = add_plain_entry_shim(&mut m);
    let mut backend = match wolf_codegen_llvm::LlvmBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIP: {e}");
            return;
        }
    };
    for (id, f) in m.funcs.iter() {
        let (symbol, linkage) = if id == shim {
            ("main".to_string(), wolf_backend::Linkage::Export)
        } else {
            (
                wolf_backend::mangle(&m, &f.name, f.sig),
                wolf_backend::Linkage::Local,
            )
        };
        backend
            .declare_function(&m, id, &f.name, &symbol, f.sig, linkage)
            .unwrap();
    }
    for (id, f) in m.funcs.iter() {
        backend
            .define_function(&m, id, f, &mut wolf_backend::NullDebugSink)
            .unwrap();
    }
    let product = Box::new(backend).finish().expect("object emits");
    assert!(product.bytes.len() > 64);
    assert_eq!(&product.bytes[..4], b"\x7fELF");
}
