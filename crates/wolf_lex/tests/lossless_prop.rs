//! The lossless round-trip property over arbitrary inputs (strings AND
//! raw bytes), plus protocol balance. Case count follows proptest's
//! `PROPTEST_CASES` env var (CI can crank it to 10^6 per the s07
//! acceptance bar).

mod util;

use proptest::prelude::*;

fn check(bytes: &[u8]) {
    // util::lex_bytes asserts reassembly and StrBegin/StrEnd +
    // InterpOpen/InterpClose balance and the Eof completion marker.
    let _ = util::lex_bytes(bytes);
}

proptest! {
    #[test]
    fn lossless_over_strings(s in any::<String>()) {
        check(s.as_bytes());
    }

    #[test]
    fn lossless_over_bytes(b in proptest::collection::vec(any::<u8>(), 0..512)) {
        check(&b);
    }

    /// Weighted toward the lexer's alphabet so string modes, dedent, and
    /// terminators actually get exercised (pure `any::<String>()` rarely
    /// produces a quote pair).
    #[test]
    fn lossless_over_wolfish(s in proptest::collection::vec(
        prop_oneof![
            Just("\""), Just("\"\"\""), Just("{"), Just("}"), Just("{{"),
            Just("\\n"), Just("\\"), Just("r\""), Just("r#\""), Just("re\""),
            Just("#["), Just("]"), Just("("), Just(")"), Just("\n"),
            Just("    "), Just("x"), Just("1.0"), Just("1"), Just("//c"),
            Just(":"), Just(";"), Just("let"), Just("?"), Just("^"),
        ],
        0..64,
    )) {
        let joined: String = s.concat();
        check(joined.as_bytes());
    }
}
