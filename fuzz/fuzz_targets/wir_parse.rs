//! Fuzz the WIR textual parser (s24): arbitrary bytes in, a module or
//! a deterministic error out — never a panic.
//!
//! Invariants:
//! - parsing never panics; printing never panics; the verifier never
//!   panics (rejecting is fine — its verdict must simply be a value);
//! - for modules the VERIFIER accepts, print → parse → print is a
//!   byte-identical fixpoint. The canonical form (defs before uses,
//!   RPO blocks) is only defined for well-formed modules — a module
//!   that parses but fails dominance can genuinely use a value before
//!   any printable def, so no re-parse promise exists for it (the
//!   fuzzer found exactly such a module; the earlier unconditional
//!   assert over-stated the s24 contract).
//!
//! Seeds live in fuzz/corpus/wir_parse/ (the s24 fixture corpus).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(module) = wolf_wir::parse_module(src) else {
        return;
    };
    let p1 = wolf_wir::print_module(&module);
    if wolf_wir::verify_module(&module).is_ok() {
        let reparsed = wolf_wir::parse_module(&p1)
            .expect("canonical output of a verified module must re-parse");
        let p2 = wolf_wir::print_module(&reparsed);
        assert_eq!(p1, p2, "print→parse→print must reach a fixpoint");
    } else {
        // still exercise the printer+parser on the canonical text —
        // outcomes may differ, panics may not
        let _ = wolf_wir::parse_module(&p1);
    }
});
