//! First fuzz target: the corpus directive parser must never panic on any
//! input — it either parses or returns a readable error. The s07 lexer is
//! the next registrant.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = xtask::corpus::parse_directives(s);
    }
});
