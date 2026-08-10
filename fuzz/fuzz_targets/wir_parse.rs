//! Fuzz the WIR textual parser (s24): arbitrary bytes in, a module or
//! a deterministic error out — never a panic.
//!
//! Invariants on every input that parses:
//! - printing never panics;
//! - print → parse → print is a byte-identical fixpoint (the round-trip
//!   property the s24 canonical form promises for EVERY parseable
//!   module, verifier-green or not);
//! - the verifier never panics (rejecting is fine — its verdict must
//!   simply be a value).
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
    let reparsed = wolf_wir::parse_module(&p1)
        .expect("canonical output of a parsed module must re-parse");
    let p2 = wolf_wir::print_module(&reparsed);
    assert_eq!(p1, p2, "print→parse→print must reach a fixpoint");
    let _ = wolf_wir::verify_module(&module);
});
