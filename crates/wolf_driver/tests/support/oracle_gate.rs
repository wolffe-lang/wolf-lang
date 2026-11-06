//! The DWARF suites' tool gate. A host that provides every debugger, dump tool
//! and pinned oracle these rows use sets `WOLF_DWARF_REQUIRE=1`: there a
//! missing, refused or unpinned tool fails its row by name. Elsewhere the row
//! prints a SKIP with its reason, which is never a pass. An unsupported OS or
//! ABI is platform scope, not a tool absence, and is not routed through here.

/// Report that `row` cannot be measured on this host because of `reason`.
/// `Ok` means the row was skipped; `Err` means the host required it.
pub fn absent(row: &str, reason: &str) -> Result<(), String> {
    let required = std::env::var_os("WOLF_DWARF_REQUIRE").is_some();
    match verdict(required, row, reason) {
        Ok(skip) => {
            eprintln!("{skip}");
            Ok(())
        }
        Err(error) => {
            eprintln!("FAIL {row}");
            Err(error)
        }
    }
}

/// The SKIP line to print, or the failure when the host requires the tool.
fn verdict(required: bool, row: &str, reason: &str) -> Result<String, String> {
    // The recorder reads one SKIP line per row; keep the reason on one line.
    let reason = reason.split_whitespace().collect::<Vec<_>>().join(" ");
    if required {
        return Err(format!("{row}: WOLF_DWARF_REQUIRE is set and {reason}"));
    }
    Ok(format!("SKIP {row} reason={reason}"))
}

#[cfg(test)]
mod tests {
    use super::verdict;

    #[test]
    fn a_required_tool_absence_fails_by_name() {
        let error = verdict(true, "a-row", "the tool is missing").unwrap_err();
        assert_eq!(
            error,
            "a-row: WOLF_DWARF_REQUIRE is set and the tool is missing"
        );
    }

    #[test]
    fn an_unrequired_absence_is_one_skip_line() {
        assert_eq!(
            verdict(false, "a-row", "two\nlines"),
            Ok("SKIP a-row reason=two lines".to_owned())
        );
    }
}
