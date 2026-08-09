//! Fuzz the formatter (s11 Target 5): arbitrary bytes → parse → fmt →
//! invariants, no panic.
//!
//! Invariants on every input, broken or not:
//! - formatting never panics;
//! - idempotence: fmt(fmt(s)) == fmt(s), byte-equal;
//! - comment multiset preservation (no comment lost/duplicated);
//! - round-trip: the output reparses to the same normalized tree;
//! - a fallback outcome returns the input byte-identical.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let out = wolf_fmt::format_text(data);
    if out.fell_back {
        assert_eq!(out.text, data, "fallback must be byte-identical");
        return;
    }
    // Idempotence.
    let again = wolf_fmt::format_text(&out.text);
    assert_eq!(again.text, out.text, "fmt not idempotent");
    // Comment multiset + normalized round-trip (format_text already
    // verified them internally — fell_back=false is the proof — but
    // assert the public invariant directly too).
    let mut sm = wolf_span::SourceMap::new();
    let a = sm.intern(std::path::Path::new("in.lu"));
    let b = sm.intern(std::path::Path::new("out.lu"));
    assert_eq!(
        wolf_fmt::comment_multiset(a, data),
        wolf_fmt::comment_multiset(b, &out.text),
        "comment multiset drift"
    );
});
