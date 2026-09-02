//! Golden CLIF snapshots for corpus-derived WIR (s28 acceptance):
//! lowering changes are reviewed diffs, never surprises. Fixtures are
//! canonical `--dump=wir` output of corpus files checked in under
//! `tests/fixtures/` (regenerate with
//! `wolf conform-run corpus/<f>.lu --dump=wir`).

use wolf_backend::Backend;
use wolf_codegen_clif::ClifBackend;

/// Hosts the backend refuses skip LOUDLY (the runtime-SKIP pattern —
/// the test starts passing the moment the s59/c13 gate lifts for a
/// host). The golden content is host-independent EXCEPT the calling-
/// convention token cranelift prints in every signature (`system_v` on
/// linux/x86-64, `apple_aarch64` on macOS/aarch64) — normalized to
/// `host_cc` below so one pinned golden serves every supported host.
fn clif_of(fixture: &str) -> Option<String> {
    let path = format!(
        "{}/tests/fixtures/{fixture}.wir",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).expect("read fixture");
    let module = wolf_wir::parse_module(&text).expect("fixture parses");
    wolf_wir::verify_module(&module).expect("fixture verifies");
    let mut backend = match ClifBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIP: {e}");
            return None;
        }
    };
    wolf_codegen_clif::compile_module(
        &mut backend,
        &module,
        None,
        &mut wolf_backend::NullDebugSink,
    )
    .expect("compiles");
    let mut out = String::new();
    for (name, clif) in backend.clif_texts() {
        out.push_str(&format!(";; @{name}\n{clif}\n"));
    }
    // The one host-dependent token (see the doc above): the default
    // calling convention's printed name. Everything else in the CLIF
    // text is identical across the supported hosts.
    let out = out
        .replace("apple_aarch64", "host_cc")
        .replace("windows_fastcall", "host_cc")
        .replace("system_v", "host_cc");
    // The object must also finish cleanly (relocatable ELF bytes).
    let product = Box::new(backend).finish().expect("object emits");
    assert!(!product.bytes.is_empty());
    Some(out)
}

#[test]
fn clif_call_ind() {
    if let Some(t) = clif_of("call_ind") {
        insta::assert_snapshot!("clif_call_ind", t)
    }
}

/// s96 (`[abi.native.dyn]`): the dyn dispatch chain — pair split,
/// slot load, `call_indirect` — with the vtable hand-built in a
/// stack slot so the shape is executable without a construction rule.
#[test]
fn clif_dyn_dispatch() {
    if let Some(t) = clif_of("dyn_dispatch") {
        insta::assert_snapshot!("clif_dyn_dispatch", t)
    }
}

#[test]
fn clif_overflow() {
    if let Some(t) = clif_of("overflow") {
        insta::assert_snapshot!("clif_overflow", t)
    };
}

#[test]
fn clif_intdot_range() {
    if let Some(t) = clif_of("intdot_range") {
        insta::assert_snapshot!("clif_intdot_range", t)
    };
}

#[test]
fn clif_exclusivity() {
    if let Some(t) = clif_of("exclusivity") {
        insta::assert_snapshot!("clif_exclusivity", t)
    };
}

#[test]
fn clif_qmark_defer() {
    if let Some(t) = clif_of("qmark_defer") {
        insta::assert_snapshot!("clif_qmark_defer", t)
    };
}

#[test]
fn clif_region_freeze_ok() {
    if let Some(t) = clif_of("region_freeze_ok") {
        insta::assert_snapshot!("clif_region_freeze_ok", t)
    };
}

#[test]
fn clif_region_infer_tree_transform() {
    if let Some(t) = clif_of("region_infer_tree_transform") {
        insta::assert_snapshot!("clif_region_infer_tree_transform", t)
    };
}
