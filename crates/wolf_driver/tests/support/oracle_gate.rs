//! The DWARF suites' tool gate. A host that provides every debugger, dump tool
//! and pinned oracle these rows use sets `WOLF_DWARF_REQUIRE=1`: there a
//! missing, refused or unpinned tool fails its row by name. Elsewhere the row
//! prints a SKIP with its reason, which is never a pass. An unsupported OS or
//! ABI is platform scope, not a tool absence, and is not routed through here.

/// Report that `row` cannot be measured on this host because of `reason`.
/// `Ok` means the row was skipped; `Err` means the host required it.
pub fn absent(row: &str, reason: &str) -> Result<(), String> {
    absent_when(
        std::env::var_os("WOLF_DWARF_REQUIRE").is_some(),
        row,
        reason,
    )
}

fn absent_when(required: bool, row: &str, reason: &str) -> Result<(), String> {
    // The recorder reads one SKIP line per row; keep the reason on one line.
    let reason = reason.split_whitespace().collect::<Vec<_>>().join(" ");
    if required {
        eprintln!("FAIL {row}");
        return Err(format!("{row}: WOLF_DWARF_REQUIRE is set and {reason}"));
    }
    eprintln!("SKIP {row} reason={reason}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::absent_when;

    #[test]
    fn a_required_tool_absence_fails_by_name() {
        let error = absent_when(true, "dwarf-v1-row", "the tool is missing").unwrap_err();
        assert!(error.starts_with("dwarf-v1-row: WOLF_DWARF_REQUIRE is set"));
    }

    #[test]
    fn an_unrequired_absence_skips() {
        assert_eq!(absent_when(false, "dwarf-v1-row", "two\nlines"), Ok(()));
    }
}
