//! Physical function ordering is independent of optimization clusters.
//!
//! Keys use source ownership and stable names, never body hashes, arena ids,
//! cluster numbers or worker completion order. Hex encodes arbitrary source
//! names without letting them become linker-script syntax.

use wolf_wir::ir::Function;

/// Included in every driver object-cache key.
pub const VERSION: &str = "function-layout-v1";
pub const MACHO_COLD: &str = "__wolf_cold";

fn component(value: &str) -> String {
    use std::fmt::Write;
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        write!(encoded, "{byte:02x}").unwrap();
    }
    encoded
}

/// A separator sorts before hexadecimal digits, preserving prefix ordering.
pub fn key(home: &str, name: &str, symbol: &str) -> String {
    format!(
        "{}.{}.{}",
        component(home),
        component(name),
        component(symbol)
    )
}

/// ELF input sections. Explicit source placement always takes precedence.
/// Cold sections deliberately do not match the default `.text.*` wildcard;
/// the driver collects them after normal text with its supplemental script.
pub fn elf_section(function: &Function, symbol: &str) -> Option<String> {
    if let Some(section) = &function.section {
        return Some(section.clone());
    }
    function.code_home.as_ref().map(|home| {
        let prefix = if function.cold {
            ".wolf.cold."
        } else {
            ".text.wolf."
        };
        format!("{prefix}{}", key(home, &function.name, symbol))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_follow_home_then_name_then_abi_not_contents() {
        let mut actual = vec![
            ("b", "a", "a"),
            ("a", "b", "a"),
            ("a", "aa", "a"),
            ("a", "a", "b"),
            ("a", "a", "a"),
            ("a.a", "a", "a"),
        ];
        let mut expected = actual.clone();
        expected.sort();
        actual.sort_by_key(|(home, name, symbol)| key(home, name, symbol));
        assert_eq!(actual, expected);
    }

    #[test]
    fn names_cannot_inject_linker_syntax_or_collide_at_separators() {
        let encoded = key("a.*", "\"b\n", "$c");
        assert!(encoded.bytes().all(|b| b.is_ascii_hexdigit() || b == b'.'));
        assert_ne!(key("a", "b.c", "d"), key("a.b", "c", "d"));
        assert!(key("a", "b", "d") < key("aa", "b", "d"));
    }
}
