//! Corpus header directives (s01/s02, extended by s06).
//!
//! A corpus file's leading `//!` block carries at most four directive keys:
//!
//! ```text
//! //! check: pass | fail(E0312) | run(exit=0) | run(exit=trap)
//! //! phase: none|lex|parse|resolve|typecheck|mem|wir|run
//! //! conforms: mem.region.freeze, err.row.union
//! //! warns: E0802, W1301
//! //! forward: borrow expressions are not implemented
//! ```
//!
//! Non-directive `//!` lines are prose and ignored. The directive language
//! stays tiny — extend only under review (s01; `warns:` added by s67,
//! `forward:` by s91).
//!
//! `warns:` is the warning ledger (s67): the exact set of warning codes
//! this file is expected to fire. A file without the directive must run
//! warning-clean — the repo's own `--deny-warnings` posture, enforced by
//! `cargo xtask corpus` — and a file *with* it must fire exactly the
//! declared codes (a reviewed-allow is a visible header, never a silent
//! suppression).
//!
//! `forward:` is the intention marker (s91). Its value names the
//! unimplemented construct the compiler stops on, and it declares that
//! this file's `check:` pins behaviour wolfgang does not implement yet:
//! the file is an *intention*, not an enforced rule, and every count
//! that separates the two reads this directive. It is load-bearing in
//! both directions, enforced by `cargo xtask corpus` — a `fail(CODE)`
//! file the compiler declines must carry it, and a file carrying it
//! that the compiler no longer declines is a stale marker over a pin
//! that has landed. `cargo xtask diag-catalog --check` reads it too:
//! it is the only way a corpus file may pin a code the catalog has
//! never heard of.

use std::collections::BTreeSet;

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
    /// race, ub, deadlock).
    Trap(Option<String>),
}

/// The closed trap-kind vocabulary (s06; spec 02 §7 assigns them;
/// `deadlock` added by the spec/03 amendment `[conc.deadlock.trap]` —
/// the deliberate `[conf.trap.set]` revision, s33 aligning the
/// harness with spec/05's 12-kind set).
pub const TRAP_KINDS: [&str; 12] = [
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
    "deadlock",
];

/// Parsed header block of one corpus file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Directives {
    pub check: Option<Check>,
    pub phase: Option<String>,
    pub conforms: Vec<String>,
    /// `warns: E0802, W1301` — the exact warning codes this file is
    /// expected to fire (sorted, deduped). Empty = must be
    /// warning-clean (the s67 `--deny-warnings` posture).
    pub warns: Vec<String>,
    /// `member: true` — this file belongs to a multi-file module case and
    /// is compiled through its entry file, never conform-run directly
    /// (s12: directory = module).
    pub member: bool,
    /// `forward: <reason>` — this file's `check:` pins behaviour that is
    /// not implemented yet, and the reason names the construct the
    /// compiler stops on (s91). An intention, not an enforced rule; the
    /// counts keep the two apart on the strength of this field.
    pub forward: Option<String>,
}

impl Directives {
    /// Every diagnostic code this header pins: the `check: fail(CODE)`
    /// rejection first, then each `warns:` code. What a reader of the
    /// corpus is entitled to look up in the catalog.
    pub fn pinned_codes(&self) -> Vec<&str> {
        let mut out = Vec::new();
        if let Some(Check::Fail(code)) = &self.check {
            out.push(code.as_str());
        }
        out.extend(self.warns.iter().map(String::as_str));
        out
    }
}

/// Parse the leading `//!` block of `src`. Errors are human-readable and
/// name the offending line (1-based).
pub fn parse_directives(src: &str) -> Result<Directives, String> {
    let mut d = Directives::default();
    for (i, line) in src.lines().enumerate() {
        let line = line.trim_start();
        // s53: an executable script opens with `#!`, which the lexer
        // takes as trivia at byte 0. The header block starts after it.
        if i == 0 && line.starts_with("#!") {
            continue;
        }
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
        } else if let Some(v) = rest.strip_prefix("warns:") {
            for code in v.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                let b = code.as_bytes();
                let shaped = b.len() == 5
                    && matches!(b[0], b'E' | b'W')
                    && b[1..].iter().all(u8::is_ascii_digit);
                if !shaped {
                    return Err(format!(
                        "line {lineno}: bad `warns:` code `{code}` (codes look like W1301)"
                    ));
                }
                d.warns.push(code.to_string());
            }
            d.warns.sort();
            d.warns.dedup();
        } else if let Some(v) = rest.strip_prefix("forward:") {
            let v = v.trim();
            if v.is_empty() {
                return Err(format!(
                    "line {lineno}: `forward:` needs a reason — name the construct that is not \
                     implemented yet"
                ));
            }
            if d.forward.is_some() {
                return Err(format!("line {lineno}: duplicate `forward:` directive"));
            }
            d.forward = Some(v.to_string());
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
            stdout = Some(unescape_stdout(v));
        } else if !part.is_empty() {
            return Err(format!("unknown run() argument `{part}`"));
        }
    }
    let exit = exit.ok_or("run() requires exit=")?;
    Ok(RunExpect { exit, stdout })
}

/// Decode the escape set of a `stdout="…"` directive value (s38 —
/// multi-print expectations need real newlines): `\n`, `\t`, `\\`,
/// `\"`. Anything else keeps the backslash literally (the directive
/// language stays small).
fn unescape_stdout(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
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

/// One diagnostic code a corpus file pins, carrying the forward-pin
/// marking of the file that pins it (s91).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub code: String,
    pub file: String,
    pub forward: Option<String>,
}

/// The corpus half of the catalog gate (s91): a code the corpus pins
/// must be one the compiler can emit, or a declared forward pin.
///
/// The catalog gate used to run in one direction only — registry to
/// docs — so a corpus file could depend on a code that existed nowhere
/// and the gate stayed green. It did: from the day it was written,
/// `fail(E1003)` was pinned against a code no catalog entry described
/// and no code path produced, and every count that read the corpus
/// treated it as a rule the compiler enforces.
///
/// Returns `(forward, unbacked)`: `forward` is the pins on undocumented
/// codes whose file says, in its header, that this is behaviour we
/// intend rather than behaviour we enforce — publishable, and honest;
/// `unbacked` is the pins on undocumented codes that claim nothing, and
/// each one is a gate failure.
pub fn audit_pins<'a>(
    pins: &'a [Pin],
    documented: &BTreeSet<String>,
) -> (Vec<&'a Pin>, Vec<&'a Pin>) {
    let mut forward = Vec::new();
    let mut unbacked = Vec::new();
    for pin in pins {
        if documented.contains(&pin.code) {
            continue;
        }
        if pin.forward.is_some() {
            forward.push(pin);
        } else {
            unbacked.push(pin);
        }
    }
    (forward, unbacked)
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
    fn warns_directive_parses_sorted_and_rejects_shapes() {
        let d = parse_directives("//! phase: mem\n//! warns: W1301, E0802, W1301\n").unwrap();
        assert_eq!(d.warns, vec!["E0802", "W1301"]);
        assert!(parse_directives("//! warns: warning\n").is_err());
        assert!(parse_directives("//! warns: W13xx\n").is_err());
        let d = parse_directives("//! phase: mem\n").unwrap();
        assert!(d.warns.is_empty());
    }

    #[test]
    fn forward_directive_carries_a_reason() {
        let d = parse_directives(
            "//! check: fail(E1003)\n//! phase: resolve\n//! forward: borrow expressions\n",
        )
        .unwrap();
        assert_eq!(d.forward.as_deref(), Some("borrow expressions"));
        // A marker with nothing behind it teaches a reader nothing.
        assert!(parse_directives("//! forward:\n").is_err());
        assert!(parse_directives("//! forward: a\n//! forward: b\n").is_err());
        // Absence is the default: an ordinary rule is not an intention.
        let d = parse_directives("//! check: fail(E0401)\n//! phase: typecheck\n").unwrap();
        assert!(d.forward.is_none());
    }

    #[test]
    fn pinned_codes_are_the_check_then_the_warns() {
        let d = parse_directives("//! check: fail(E1003)\n//! warns: W1301, E0802\n").unwrap();
        assert_eq!(d.pinned_codes(), vec!["E1003", "E0802", "W1301"]);
        let d = parse_directives("//! check: pass\n//! phase: run\n").unwrap();
        assert!(d.pinned_codes().is_empty());
    }

    /// The gate should have failed the day `borrow_escape.lu` was
    /// written. This is that day, reconstructed: the file as it stood,
    /// against a catalog that has never heard of E1003.
    #[test]
    fn an_unmarked_pin_on_a_phantom_code_fails_the_gate() {
        let documented: BTreeSet<String> = ["E1001", "E1002", "E1004"]
            .into_iter()
            .map(String::from)
            .collect();
        let as_written = parse_directives(
            "//! check: fail(E1003)\n//! phase: resolve\n//! conforms: mem.tier0.borrow.1\n",
        )
        .unwrap();
        let pins: Vec<Pin> = as_written
            .pinned_codes()
            .into_iter()
            .map(|code| Pin {
                code: code.to_string(),
                file: "corpus/memory/borrow_escape.lu".into(),
                forward: as_written.forward.clone(),
            })
            .collect();
        let (forward, unbacked) = audit_pins(&pins, &documented);
        assert!(forward.is_empty());
        assert_eq!(unbacked.len(), 1, "E1003 is pinned and nothing emits it");
        assert_eq!(unbacked[0].code, "E1003");

        // Marked as the intention it is, the same pin passes — and is
        // published as a forward pin rather than counted as a rule.
        let marked = parse_directives(
            "//! check: fail(E1003)\n//! phase: resolve\n//! forward: borrow expressions\n",
        )
        .unwrap();
        let pins: Vec<Pin> = marked
            .pinned_codes()
            .into_iter()
            .map(|code| Pin {
                code: code.to_string(),
                file: "corpus/memory/borrow_escape.lu".into(),
                forward: marked.forward.clone(),
            })
            .collect();
        let (forward, unbacked) = audit_pins(&pins, &documented);
        assert!(unbacked.is_empty());
        assert_eq!(forward.len(), 1);
        assert_eq!(forward[0].forward.as_deref(), Some("borrow expressions"));
    }

    #[test]
    fn a_documented_code_needs_no_marking() {
        let documented: BTreeSet<String> =
            ["E1001", "W1301"].into_iter().map(String::from).collect();
        let pins = vec![
            Pin {
                code: "E1001".into(),
                file: "corpus/memory/use_after_move.lu".into(),
                forward: None,
            },
            Pin {
                code: "W1301".into(),
                file: "corpus/lints/unused.lu".into(),
                forward: None,
            },
        ];
        let (forward, unbacked) = audit_pins(&pins, &documented);
        assert!(forward.is_empty() && unbacked.is_empty());
    }

    #[test]
    fn phase_ladder_is_ordered() {
        assert!(phase_rank("none").unwrap() < phase_rank("lex").unwrap());
        assert!(phase_rank("wir").unwrap() < phase_rank("run").unwrap());
        assert_eq!(phase_rank("sema"), None);
    }

    #[test]
    fn stdout_directive_decodes_escapes() {
        // Multi-print expectations carry real newlines (s38).
        let d = parse_directives(
            "//! check: run(exit=0, stdout=\"a\\nb\\tc\\\\d\")\n//! phase: run\nfn main() {}\n",
        )
        .unwrap();
        let Some(Check::Run(r)) = d.check else {
            panic!("run check");
        };
        assert_eq!(r.stdout.as_deref(), Some("a\nb\tc\\d"));
    }
}
