//! Fuzz the formatter (s11 Target 5): arbitrary bytes → parse → fmt →
//! invariants, no panic.
//!
//! Two tiers of invariant — the split `fmt_fuzz --allow-open` already
//! makes (crates/wolf_fmt/examples/fmt_fuzz.rs) — because they answer
//! different questions and one of them has a known-open bank.
//!
//! CORRUPTING — a crash on any input, always:
//! - formatting never panics;
//! - comment multiset preservation (no comment lost or duplicated);
//! - round-trip: the output reparses to the same normalized tree;
//! - a fallback outcome returns the input byte-identical.
//!
//! LAYOUT — idempotence, `fmt(fmt(s)) == fmt(s)`, byte-equal. The
//! well is not dry: `wolf_fmt/tests/regressions/unfixed/` banks the
//! open classes (#74) and `banked_classes_are_still_open` keeps the
//! bank honest in the other direction. Crashing this target on a
//! rediscovery made the nightly red every night for a fact the bank
//! already recorded (#109) — a red gate that named nothing new, and
//! trained everyone to stop reading it. So a non-idempotent input is
//! REPORTED, with the input, and the run continues. The corrupting
//! tier keeps the teeth; the sweep is where a human triages layout.
//!
//! Why not "crash only if not already banked": a class is structural,
//! not textual, and no cheap test on bytes recognises a libFuzzer
//! mutant of a banked case as the same class — measured at 0 of 11
//! under a one-item prefix splice. A classifier that agreed with the
//! human-named classes would be its own project; until then, pretend
//! otherwise and this target is red for the old reason with a longer
//! excuse attached.
//!
//! To hunt layout findings with a crash-on-find loop, run this target
//! with `WOLF_FMT_FUZZ_STRICT=1`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fn strict() -> bool {
    static S: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *S.get_or_init(|| std::env::var_os("WOLF_FMT_FUZZ_STRICT").is_some())
}

fuzz_target!(|data: &[u8]| {
    let out = wolf_fmt::format_text(data);
    if out.fell_back {
        assert_eq!(out.text, data, "fallback must be byte-identical");
        return;
    }
    // Comment multiset + normalized round-trip (format_text already
    // verified them internally — fell_back=false is the proof — but
    // assert the public invariant directly too). CORRUPTING.
    let mut sm = wolf_span::SourceMap::new();
    let a = sm.intern(std::path::Path::new("in.lu"));
    let b = sm.intern(std::path::Path::new("out.lu"));
    assert_eq!(
        wolf_fmt::comment_multiset(a, data),
        wolf_fmt::comment_multiset(b, &out.text),
        "comment multiset drift"
    );
    // Idempotence. LAYOUT: reported, not fatal, unless strict.
    let again = wolf_fmt::format_text(&out.text);
    if again.text != out.text {
        if strict() {
            panic!(
                "fmt not idempotent\n  input={:?}\n  pass1={:?}\n  pass2={:?}",
                data, out.text, again.text
            );
        }
        eprintln!(
            "fmt-fuzz: layout finding (not idempotent; not corrupting) input={:?}",
            data
        );
    }
});
