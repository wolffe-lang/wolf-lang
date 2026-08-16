//! Exemplar insta snapshot (s01): the pattern diagnostics will follow —
//! reviewed golden output, changed only deliberately via `cargo insta`.

#[test]
fn full_header_snapshot() {
    let src = "//! check: run(exit=0, stdout=\"hi, wolf\")\n\
               //! phase: run\n\
               //! conforms: str.interp, mem.region.freeze\n\
               //! A prose line describing intent.\n\
               fn main() {}\n";
    insta::assert_debug_snapshot!(xtask::corpus::parse_directives(src));
}

#[test]
fn error_snapshot() {
    insta::assert_debug_snapshot!(xtask::corpus::parse_directives("//! phase: sema\n"));
}

/// A forward pin, as `corpus/memory/borrow_escape.lu` now spells one:
/// the `check:` is a rejection the compiler cannot make yet, and the
/// header says which construct is missing. Reviewed as a golden so the
/// difference between a rule and an intention stays visible in the
/// parse, not only in the counts downstream.
#[test]
fn forward_pin_snapshot() {
    let src = "//! check: fail(E1003)\n\
               //! phase: resolve\n\
               //! conforms: mem.tier0.borrow.1\n\
               //! forward: borrow expressions\n\
               fn main() -> !int { 0 }\n";
    insta::assert_debug_snapshot!(xtask::corpus::parse_directives(src));
}
