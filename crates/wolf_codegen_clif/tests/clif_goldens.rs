//! Golden CLIF snapshots for corpus-derived WIR (s28 acceptance):
//! lowering changes are reviewed diffs, never surprises. Fixtures are
//! canonical `--dump=wir` output of corpus files checked in under
//! `tests/fixtures/` (regenerate with
//! `wolf conform-run corpus/<f>.lu --dump=wir`).

use wolf_backend::Backend;
use wolf_codegen_clif::ClifBackend;

/// s28 targets linux/x86-64 only (the M1 platform); other hosts skip
/// LOUDLY — the golden content is host-independent and stays pinned by
/// the linux CI lane.
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
    // The object must also finish cleanly (relocatable ELF bytes).
    let product = Box::new(backend).finish().expect("object emits");
    assert!(!product.bytes.is_empty());
    Some(out)
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
