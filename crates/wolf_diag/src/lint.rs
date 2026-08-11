//! Warning levels and configuration (s67 — the warning system).
//!
//! Warnings are a registered, leveled, configurable tier: every warning
//! rides the same registry/renderer/fixture machinery as errors, and its
//! *level* — [`Level::Allow`] / [`Level::Warn`] / [`Level::Deny`] — is
//! decided here, from three declarative sources with fixed precedence:
//!
//! 1. `#[allow(w1301)]` attributes in the source (item-granular — part
//!    of the program, so every consumer honors them, including
//!    `conform-run`),
//! 2. CLI flags (`wolf build --deny W1301`, `--deny-warnings`),
//! 3. the manifest's `lints.*` entries in `wolf.pkg` (the s51 stub
//!    format; CLI flags layer *over* manifest rules).
//!
//! Selector precedence is specificity, not source: a per-code rule
//! beats a per-family rule beats the all-warnings rule; among rules of
//! equal specificity the *last* one set wins (CLI rules are appended
//! after manifest rules, so the command line has the final word).
//!
//! Levels apply to warning-severity diagnostics only — an error rejects
//! meaning and cannot be allowed, denied, or configured away (spec/01
//! §9's severity contract). A denied warning keeps its `W####` code and
//! reports at error severity, failing the build.

use crate::{Diagnostic, Severity};
use wolf_span::Span;

/// What to do with a warning: drop it, report it, or reject the build.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Allow,
    Warn,
    Deny,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Allow => "allow",
            Level::Warn => "warn",
            Level::Deny => "deny",
        }
    }
}

/// Which warnings a rule names: one code (`W1301`), one family
/// (`W13xx`), or every warning (`warnings`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Selector {
    /// Every warning-severity diagnostic (`--deny-warnings`, or the
    /// selector spelled `warnings`).
    All,
    /// A family: the two-digit prefix, stored as `W13` for `W13xx`.
    Family(String),
    /// One exact code, stored uppercase (`W1301`).
    Code(String),
}

impl Selector {
    /// Parse a selector as written on a flag, a manifest line, or an
    /// `#[allow(…)]` argument: `warnings`, `W1301`/`w1301`, or the
    /// family form `W13xx`/`w13xx`. Registration of exact codes is the
    /// caller's check (the CLI rejects unknown codes; the attribute
    /// scan warns W0302) — this parses the *shape* only.
    pub fn parse(s: &str) -> Result<Selector, String> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("warnings") {
            return Ok(Selector::All);
        }
        let up = s.to_ascii_uppercase();
        let bytes = up.as_bytes();
        let shape_err = || {
            format!(
                "`{s}` is not a lint selector — write a code (`W1301`), a family \
                 (`W13xx`), or `warnings`"
            )
        };
        if bytes.len() != 5 || !matches!(bytes[0], b'E' | b'W') {
            return Err(shape_err());
        }
        if bytes[1..].iter().all(u8::is_ascii_digit) {
            return Ok(Selector::Code(up));
        }
        if bytes[1..3].iter().all(u8::is_ascii_digit) && &up[3..] == "XX" {
            return Ok(Selector::Family(up[..3].to_string()));
        }
        Err(shape_err())
    }

    /// Does this selector name `code`?
    pub fn matches(&self, code: &str) -> bool {
        match self {
            Selector::All => true,
            Selector::Family(prefix) => code.starts_with(prefix.as_str()),
            Selector::Code(c) => code == c,
        }
    }

    /// More specific selectors win: code > family > all.
    fn specificity(&self) -> u8 {
        match self {
            Selector::All => 0,
            Selector::Family(_) => 1,
            Selector::Code(_) => 2,
        }
    }
}

/// The effective level configuration: an ordered rule list. Most
/// specific matching rule wins; among equals, the last one set.
#[derive(Clone, Default, Debug)]
pub struct LintLevels {
    rules: Vec<(Selector, Level)>,
}

impl LintLevels {
    pub fn new() -> LintLevels {
        LintLevels::default()
    }

    /// Append a rule (later rules win over earlier ones of the same
    /// specificity — callers add manifest rules first, CLI rules after).
    pub fn set(&mut self, selector: Selector, level: Level) {
        self.rules.push((selector, level));
    }

    /// `--deny-warnings`: every warning becomes an error unless a more
    /// specific rule (or a source `#[allow]`) says otherwise.
    pub fn deny_warnings(&mut self) {
        self.set(Selector::All, Level::Deny);
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Append every rule of `other` after this set's own (the layering
    /// call: manifest levels first, then `extend_from(cli)` so flags
    /// win ties).
    pub fn extend_from(&mut self, other: &LintLevels) {
        self.rules.extend(other.rules.iter().cloned());
    }

    /// The configured level for `code` (default: [`Level::Warn`]).
    pub fn level_for(&self, code: &str) -> Level {
        let mut best: Option<(u8, Level)> = None;
        for (sel, lvl) in &self.rules {
            if !sel.matches(code) {
                continue;
            }
            let sp = sel.specificity();
            // `>=` : later rules of equal specificity overwrite.
            if best.is_none_or(|(bsp, _)| sp >= bsp) {
                best = Some((sp, *lvl));
            }
        }
        best.map(|(_, l)| l).unwrap_or(Level::Warn)
    }
}

/// One source-level `#[allow(…)]`: the selector it names and the item
/// region it governs. A warning whose primary span falls inside the
/// region is dropped before any configured level applies — the source
/// is the most local authority.
#[derive(Clone, Debug)]
pub struct AllowRegion {
    pub selector: Selector,
    pub span: Span,
}

impl AllowRegion {
    fn suppresses(&self, d: &Diagnostic) -> bool {
        let p = d.primary.span;
        self.selector.matches(d.code.as_str())
            && p.file == self.span.file
            && p.lo >= self.span.lo
            && p.hi <= self.span.hi
    }
}

/// Apply the level configuration and source allows to a diagnostic set.
/// Errors pass through untouched; warnings are dropped (`allow`), kept
/// (`warn`), or promoted to errors (`deny` — the code stays `W####`,
/// the severity becomes `error`, and a note names the rule so the
/// reader knows the rejection is configuration, not semantics).
pub fn apply(
    levels: &LintLevels,
    allows: &[AllowRegion],
    diags: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    diags
        .into_iter()
        .filter_map(|mut d| {
            if d.severity != Severity::Warning {
                return Some(d);
            }
            if allows.iter().any(|a| a.suppresses(&d)) {
                return None;
            }
            match levels.level_for(d.code.as_str()) {
                Level::Allow => None,
                Level::Warn => Some(d),
                Level::Deny => {
                    d.severity = Severity::Error;
                    d.notes.push(format!(
                        "this is a warning promoted by the lint configuration \
                         (`deny {}`); the program's meaning is unaffected — \
                         allow it, fix it, or drop the deny.",
                        d.code
                    ));
                    Some(d)
                }
            }
        })
        .collect()
}

/// Parse the `[lints]` entries of a `wolf.pkg` manifest (the s51 stub
/// format, D33: declarative only — one `lints.<level> = sel, sel` line
/// per level, repeatable):
///
/// ```text
/// lints.deny = W1301, W13xx
/// lints.allow = W0302
/// lints.warn = warnings
/// ```
///
/// Rules are returned in file order; the caller appends CLI rules
/// after, so flags override the manifest. Errors name the line.
pub fn parse_manifest_lints(manifest: &str) -> Result<Vec<(Selector, Level)>, String> {
    let mut out = Vec::new();
    for (i, line) in manifest.lines().enumerate() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("lints.") else {
            continue;
        };
        let Some((key, list)) = rest.split_once('=') else {
            return Err(format!(
                "wolf.pkg line {}: `lints.<level>` needs `= selector, …`",
                i + 1
            ));
        };
        let level = match key.trim() {
            "allow" => Level::Allow,
            "warn" => Level::Warn,
            "deny" => Level::Deny,
            other => {
                return Err(format!(
                    "wolf.pkg line {}: unknown lint level `{other}` (allow, warn, deny)",
                    i + 1
                ));
            }
        };
        for sel in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let sel = sel.trim_matches('"');
            let selector =
                Selector::parse(sel).map_err(|e| format!("wolf.pkg line {}: {e}", i + 1))?;
            out.push((selector, level));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes;
    use std::path::Path;
    use wolf_span::SourceMap;

    fn file() -> wolf_span::FileId {
        let mut sm = SourceMap::new();
        sm.intern(Path::new("t.lu"))
    }

    #[test]
    fn selector_shapes() {
        assert_eq!(Selector::parse("warnings"), Ok(Selector::All));
        assert_eq!(Selector::parse("w1301"), Ok(Selector::Code("W1301".into())));
        assert_eq!(Selector::parse("W13xx"), Ok(Selector::Family("W13".into())));
        assert!(Selector::parse("w13").is_err());
        assert!(Selector::parse("13xx").is_err());
        assert!(Selector::parse("Wxxxx").is_err());
    }

    #[test]
    fn specificity_beats_order_and_order_breaks_ties() {
        let mut l = LintLevels::new();
        l.deny_warnings();
        l.set(Selector::Family("W13".into()), Level::Warn);
        l.set(Selector::Code("W1301".into()), Level::Allow);
        assert_eq!(l.level_for("W1301"), Level::Allow); // code > family
        assert_eq!(l.level_for("W1302"), Level::Warn); // family > all
        assert_eq!(l.level_for("W0301"), Level::Deny); // all
        // Ties: last wins.
        l.set(Selector::Code("W1301".into()), Level::Deny);
        assert_eq!(l.level_for("W1301"), Level::Deny);
        // Unconfigured default.
        assert_eq!(LintLevels::new().level_for("W9999"), Level::Warn);
    }

    #[test]
    fn apply_drops_promotes_and_respects_allows() {
        let f = file();
        let w = |lo: u32| Diagnostic::warning(codes::W1301, Span::new(f, lo, lo + 1), "w");
        let e = Diagnostic::error(codes::E0201, Span::new(f, 0, 1), "e");
        let mut levels = LintLevels::new();
        levels.deny_warnings();
        let allows = [AllowRegion {
            selector: Selector::Code("W1301".into()),
            span: Span::new(f, 10, 20),
        }];
        let out = apply(&levels, &allows, vec![e.clone(), w(12), w(30)]);
        // The allowed one (span 12 inside 10..20) is gone; the other is
        // promoted; the error is untouched.
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].code, codes::E0201);
        assert_eq!(out[1].code, codes::W1301);
        assert_eq!(out[1].severity, Severity::Error);
        assert!(out[1].notes.iter().any(|n| n.contains("deny W1301")));
    }

    #[test]
    fn allow_regions_are_item_granular() {
        let f = file();
        let allows = [AllowRegion {
            selector: Selector::Family("W13".into()),
            span: Span::new(f, 0, 10),
        }];
        let inside = Diagnostic::warning(codes::W1301, Span::new(f, 2, 4), "in");
        let outside = Diagnostic::warning(codes::W1301, Span::new(f, 8, 12), "out");
        let out = apply(&LintLevels::new(), &allows, vec![inside, outside]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].message, "out");
    }

    #[test]
    fn manifest_stub_parses_and_rejects() {
        let rules = parse_manifest_lints(
            "trusted = mod_a\nlints.deny = W1301, W13xx\nlints.allow = warnings\n",
        )
        .expect("parses");
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0], (Selector::Code("W1301".into()), Level::Deny));
        assert_eq!(rules[2], (Selector::All, Level::Allow));
        assert!(parse_manifest_lints("lints.forbid = W1301\n").is_err());
        assert!(parse_manifest_lints("lints.deny = bogus\n").is_err());
    }
}
