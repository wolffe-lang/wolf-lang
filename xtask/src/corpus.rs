//! Corpus header directives (s01/s02, extended by s06).
//!
//! A corpus file's leading `//!` block carries at most three directive keys:
//!
//! ```text
//! //! check: pass | fail(E0312) | run(exit=0) | run(exit=trap)
//! //! phase: none|lex|parse|resolve|typecheck|mem|wir|run
//! //! conforms: mem.region.freeze, err.row.union
//! ```
//!
//! Non-directive `//!` lines are prose and ignored. The directive language
//! stays tiny — extend only under review (s01).

/// Canonical phase ladder (s06). Order matters: later phases include earlier.
pub const PHASES: [&str; 8] = [
    "none",
    "lex",
    "parse",
    "resolve",
    "typecheck",
    "mem",
    "wir",
    "run",
];

/// The `check:` expectation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// Compiles clean through its declared phase.
    Pass,
    /// Rejected with this diagnostic code (e.g. `E0312`).
    Fail(String),
    /// Runs to completion with this outcome.
    Run(RunExpect),
}

/// Expected run outcome for `check: run(...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunExpect {
    /// `exit=N` or `exit=trap` (deterministic fault per the s06 vocabulary).
    pub exit: ExitExpect,
    /// Optional `stdout="..."` exact match.
    pub stdout: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitExpect {
    Code(i32),
    /// `exit=trap` (any kind) or `exit=trap(kind)` with a kind from the
    /// closed s06 vocabulary (overflow, div-zero, bounds, use-after-move,
    /// exclusivity, region-fault, stale-handle, alloc-contract, assert,
    /// race, ub).
    Trap(Option<String>),
}

/// The closed trap-kind vocabulary (s06; spec 02 §7 assigns them).
pub const TRAP_KINDS: [&str; 11] = [
    "overflow",
    "div-zero",
    "bounds",
    "use-after-move",
    "exclusivity",
    "region-fault",
    "stale-handle",
    "alloc-contract",
    "assert",
    "race",
    "ub",
];

/// Parsed header block of one corpus file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Directives {
    pub check: Option<Check>,
    pub phase: Option<String>,
    pub conforms: Vec<String>,
    /// `member: true` — this file belongs to a multi-file module case and
    /// is compiled through its entry file, never conform-run directly
    /// (s12: directory = module).
    pub member: bool,
}

/// Parse the leading `//!` block of `src`. Errors are human-readable and
/// name the offending line (1-based).
pub fn parse_directives(src: &str) -> Result<Directives, String> {
    let mut d = Directives::default();
    for (i, line) in src.lines().enumerate() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("//!") else {
            break; // header block ends at the first non-`//!` line
        };
        let rest = rest.trim();
        let lineno = i + 1;
        if let Some(v) = rest.strip_prefix("check:") {
            if d.check.is_some() {
                return Err(format!("line {lineno}: duplicate `check:` directive"));
            }
            d.check = Some(parse_check(v.trim()).map_err(|e| format!("line {lineno}: {e}"))?);
        } else if let Some(v) = rest.strip_prefix("phase:") {
            let v = v.trim();
            if !PHASES.contains(&v) {
                return Err(format!(
                    "line {lineno}: unknown phase `{v}` (canonical: {})",
                    PHASES.join(", ")
                ));
            }
            if d.phase.is_some() {
                return Err(format!("line {lineno}: duplicate `phase:` directive"));
            }
            d.phase = Some(v.to_string());
        } else if let Some(v) = rest.strip_prefix("conforms:") {
            d.conforms.extend(
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            );
        } else if let Some(v) = rest.strip_prefix("member:") {
            match v.trim() {
                "true" => d.member = true,
                "false" => {}
                other => return Err(format!("line {lineno}: bad member value `{other}`")),
            }
        }
        // any other `//!` line is prose
    }
    Ok(d)
}

fn parse_check(v: &str) -> Result<Check, String> {
    if v == "pass" {
        return Ok(Check::Pass);
    }
    if let Some(code) = v.strip_prefix("fail(").and_then(|s| s.strip_suffix(')')) {
        let code = code.trim();
        if code.is_empty() {
            return Err("`fail(...)` needs a diagnostic code".into());
        }
        return Ok(Check::Fail(code.to_string()));
    }
    if let Some(args) = v.strip_prefix("run(").and_then(|s| s.strip_suffix(')')) {
        return parse_run(args).map(Check::Run);
    }
    Err(format!(
        "bad `check:` value `{v}` (expected pass | fail(CODE) | run(exit=..))"
    ))
}

fn parse_run(args: &str) -> Result<RunExpect, String> {
    let mut exit = None;
    let mut stdout = None;
    for part in split_args(args) {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("exit=") {
            exit = Some(if v == "trap" {
                ExitExpect::Trap(None)
            } else if let Some(kind) = v.strip_prefix("trap(").and_then(|s| s.strip_suffix(')')) {
                let kind = kind.trim();
                if !TRAP_KINDS.contains(&kind) {
                    return Err(format!(
                        "unknown trap kind `{kind}` (closed set: {})",
                        TRAP_KINDS.join(", ")
                    ));
                }
                ExitExpect::Trap(Some(kind.to_string()))
            } else {
                ExitExpect::Code(
                    v.parse::<i32>()
                        .map_err(|_| format!("bad exit value `{v}`"))?,
                )
            });
        } else if let Some(v) = part.strip_prefix("stdout=") {
            let v = v
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .ok_or_else(|| format!("stdout value must be quoted, got `{v}`"))?;
            stdout = Some(v.to_string());
        } else if !part.is_empty() {
            return Err(format!("unknown run() argument `{part}`"));
        }
    }
    let exit = exit.ok_or("run() requires exit=")?;
    Ok(RunExpect { exit, stdout })
}

/// Split run() args on commas that are outside double quotes.
fn split_args(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth_quote = false;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '"' => depth_quote = !depth_quote,
            ',' if !depth_quote => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

/// Position of a phase on the canonical ladder.
pub fn phase_rank(phase: &str) -> Option<usize> {
    PHASES.iter().position(|p| *p == phase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_header_parses() {
        let src = "//! check: run(exit=0, stdout=\"hi, wolf\")\n\
                   //! phase: run\n\
                   //! conforms: str.interp, mem.region.freeze\n\
                   //! prose lines are ignored\n\
                   fn main() {}\n";
        let d = parse_directives(src).unwrap();
        assert_eq!(
            d.check,
            Some(Check::Run(RunExpect {
                exit: ExitExpect::Code(0),
                stdout: Some("hi, wolf".into()),
            }))
        );
        assert_eq!(d.phase.as_deref(), Some("run"));
        assert_eq!(d.conforms, vec!["str.interp", "mem.region.freeze"]);
    }

    #[test]
    fn trap_and_fail_forms() {
        let d = parse_directives("//! check: run(exit=trap)\n").unwrap();
        assert_eq!(
            d.check,
            Some(Check::Run(RunExpect {
                exit: ExitExpect::Trap(None),
                stdout: None
            }))
        );
        let d = parse_directives("//! check: fail(E0312)\n").unwrap();
        assert_eq!(d.check, Some(Check::Fail("E0312".into())));
    }

    #[test]
    fn header_ends_at_first_code_line() {
        let d = parse_directives("//! phase: lex\nlet x = 1\n//! phase: run\n").unwrap();
        assert_eq!(d.phase.as_deref(), Some("lex"));
    }

    #[test]
    fn rejects_unknown_phase_and_duplicates() {
        assert!(parse_directives("//! phase: sema\n").is_err());
        assert!(parse_directives("//! phase: lex\n//! phase: run\n").is_err());
        assert!(parse_directives("//! check: pass\n//! check: pass\n").is_err());
        assert!(parse_directives("//! check: maybe\n").is_err());
    }

    #[test]
    fn quoted_comma_stays_in_stdout() {
        let d = parse_directives("//! check: run(exit=0, stdout=\"a, b\")\n").unwrap();
        let Some(Check::Run(r)) = d.check else {
            panic!("expected run")
        };
        assert_eq!(r.stdout.as_deref(), Some("a, b"));
    }

    #[test]
    fn phase_ladder_is_ordered() {
        assert!(phase_rank("none").unwrap() < phase_rank("lex").unwrap());
        assert!(phase_rank("wir").unwrap() < phase_rank("run").unwrap());
        assert_eq!(phase_rank("sema"), None);
    }
}
