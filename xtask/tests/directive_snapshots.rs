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
