//! s38 — the two format-spec renderers pinned against each other.
//!
//! The compiler parses and packs every spec (`wolf_sema::fmtspec`);
//! the checked executor renders through `fmtspec::apply`, and the
//! native runtime renders in `wolf_rt::io` from the packed i64 — a
//! hand-mirrored implementation (wolf_rt may depend on nothing in the
//! compiler, D15). A byte of divergence between them is a
//! cross-lane stdout divergence, exactly the class the differential
//! rig exists to catch — so this test walks both over the full spec
//! grammar and every value class the lanes can print.

use wolf_rt::io as rtio;
use wolf_sema::fmtspec::{self, FmtValue};

/// Every spec here must parse, validate against the class, and render
/// the SAME bytes through both implementations.
fn assert_parity(spec_text: &str, v: FmtValue<'_>) {
    let spec =
        fmtspec::parse(spec_text).unwrap_or_else(|e| panic!("spec `{spec_text}` parses: {e:?}"));
    let reference =
        fmtspec::apply(&spec, v).unwrap_or_else(|e| panic!("spec `{spec_text}` fits {v:?}: {e:?}"));
    let mut packed = fmtspec::pack(&spec);
    if let FmtValue::Int { unsigned: true, .. } = v {
        packed |= fmtspec::PACK_UNSIGNED;
    }
    let native = match v {
        FmtValue::Str(s) => rtio::render_str_packed(s, packed),
        FmtValue::Bool(b) => rtio::render_bool_packed(b, packed),
        FmtValue::Int { v, .. } => rtio::render_i64_packed(v, packed),
        FmtValue::F64(x) => rtio::render_f64_packed(x, packed),
    };
    assert_eq!(
        reference, native,
        "lanes diverge on `{{x:{spec_text}}}` for {v:?}"
    );
}

#[test]
fn str_specs_agree() {
    let values = ["", "hi", "wolves", "é", "aé", "🐺", "a b"];
    let specs = [
        "", "8", "<8", ">8", "^8", "*>8", "*<8", "-^9", ".2", ".1", ".3", ">6.2", "0>4", "2",
    ];
    for v in values {
        for s in specs {
            assert_parity(s, FmtValue::Str(v));
        }
    }
}

#[test]
fn bool_specs_agree() {
    for v in [true, false] {
        for s in ["", "8", ">8", "^7", "*<9"] {
            assert_parity(s, FmtValue::Bool(v));
        }
    }
}

#[test]
fn int_specs_agree() {
    let values = [0i64, 1, -1, 42, -42, 255, -255, i64::MAX, i64::MIN];
    let specs = [
        "", "6", ">6", "<6", "^6", "*>8", "+", "+06", "06", "08", "0", "x", "X", "b", "o", ">8x",
        "+x", "020b", "+020",
    ];
    for v in values {
        for s in specs {
            assert_parity(s, FmtValue::Int { v, unsigned: false });
            assert_parity(s, FmtValue::Int { v, unsigned: true });
        }
    }
}

#[test]
#[allow(clippy::approx_constant, clippy::excessive_precision)] // repro pins
fn float_specs_agree() {
    let values = [
        0.0,
        -0.0,
        0.5,
        1.5,
        2.5,
        -1.5,
        0.1,
        3.14159,
        1e-5,
        1e16,
        1e17,
        9.999999999999999e22,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    let specs = [
        "", "10", ">10", "<10", "^12", "*>12", "+", ".0", ".2", ".6", ".20", ">8.2", "+.2", "08.2",
        "f", ".2f", ".0f", "e", ".2e", ".0e", "E", ".2E", "012.3f", "+012.3e",
    ];
    for v in values {
        for s in specs {
            assert_parity(s, FmtValue::F64(v));
        }
    }
}

#[test]
fn default_renderings_agree() {
    // Spec-less floats flow through `f64_shortest` on both sides.
    for v in [0.3, -2.5e-12, 123456.789, 3.0, 1e23, 5e-324] {
        assert_eq!(
            fmtspec::f64_shortest(v),
            rtio::f64_shortest(v),
            "shortest diverges on {v}"
        );
    }
}
