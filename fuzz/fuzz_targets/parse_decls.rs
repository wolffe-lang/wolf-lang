//! s08 parser fuzz target. Parsing is TOTAL: arbitrary bytes in, a
//! complete lossless tree + diagnostics out. Asserted invariants (beyond
//! "no panic"): the tree verifier passes (token tiling, span nesting)
//! and tree text reproduces the input byte-for-byte. Seeds live in
//! fuzz/corpus/parse_decls/ (the broken-input suite).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Spans are u32 byte offsets; the documented precondition.
    if data.len() > u32::MAX as usize {
        return;
    }
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(std::path::Path::new("fuzz.lu"));
    let parse = wolf_parse::parse_file(file, data);

    // The three tree invariants, the D22 bet made falsifiable.
    if let Err(e) = wolf_ast::verify(&parse.root, data) {
        panic!("tree verifier failed: {e}");
    }

    // Lossless: tree text == input, byte-for-byte.
    assert_eq!(parse.root.text(data), data, "lossless round-trip violated");
});
