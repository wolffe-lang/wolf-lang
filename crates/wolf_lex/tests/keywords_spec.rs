//! The keyword table can never drift from the spec: read the
//! `reserved_kw` production out of spec/grammar.ebnf at test time and
//! compare exactly ([gram.inv.kw] — the list is normative, the count is
//! a checksum).

use std::collections::BTreeSet;
use std::path::Path;

fn spec_keywords() -> BTreeSet<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/grammar.ebnf");
    let ebnf = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "read {}: {e} (run `cargo xtask spec-extract`)",
            path.display()
        )
    });
    let mut kws = BTreeSet::new();
    let mut in_prod = false;
    for line in ebnf.lines() {
        if line.starts_with("reserved_kw") && line.contains("::=") {
            in_prod = true;
        } else if in_prod && (line.trim().is_empty() || line.contains("::=")) {
            break;
        }
        if in_prod {
            let mut rest = line;
            while let Some(start) = rest.find('\'') {
                let Some(len) = rest[start + 1..].find('\'') else {
                    break;
                };
                kws.insert(rest[start + 1..start + 1 + len].to_string());
                rest = &rest[start + 1 + len + 1..];
            }
        }
    }
    kws
}

#[test]
fn keyword_table_matches_grammar_ebnf() {
    let spec = spec_keywords();
    let table: BTreeSet<String> = wolf_lex::KEYWORDS
        .iter()
        .map(|(s, _)| (*s).to_string())
        .collect();
    assert_eq!(spec.len(), 50, "[gram.inv.kw] count checksum");
    assert_eq!(
        table, spec,
        "wolf_lex::KEYWORDS disagrees with spec/grammar.ebnf reserved_kw"
    );
}
