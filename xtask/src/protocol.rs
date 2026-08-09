//! The differential protocol (spec/06): observation-record validation and
//! comparison. `xtask differ` and the fixture tests are the consumers;
//! wolf-interp implements the same document independently.

use crate::corpus::TRAP_KINDS;

/// Parsed verdict per `[proto.record.verdict]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail(String),
    Exit(i64),
    Trap(String),
    Ub(String),
    Unsupported,
}

pub fn parse_verdict(s: &str) -> Result<Verdict, String> {
    if s == "pass" {
        return Ok(Verdict::Pass);
    }
    if s == "unsupported" {
        return Ok(Verdict::Unsupported);
    }
    if let Some(code) = s.strip_prefix("fail(").and_then(|r| r.strip_suffix(')')) {
        return Ok(Verdict::Fail(code.to_string()));
    }
    if let Some(n) = s.strip_prefix("exit(").and_then(|r| r.strip_suffix(')')) {
        return n
            .parse()
            .map(Verdict::Exit)
            .map_err(|_| format!("bad exit status `{n}`"));
    }
    if let Some(k) = s.strip_prefix("trap(").and_then(|r| r.strip_suffix(')')) {
        if !TRAP_KINDS.contains(&k) {
            return Err(format!("unknown trap kind `{k}`"));
        }
        return Ok(Verdict::Trap(k.to_string()));
    }
    if let Some(a) = s.strip_prefix("ub(").and_then(|r| r.strip_suffix(')')) {
        return Ok(Verdict::Ub(a.to_string()));
    }
    Err(format!("unparseable verdict `{s}`"))
}

/// Validate an observation record against `[proto.record]`. Returns the
/// parsed verdict on success.
pub fn validate_record(v: &serde_json::Value) -> Result<Verdict, String> {
    let obj = v.as_object().ok_or("record is not an object")?;
    if obj.get("protocol").and_then(|p| p.as_i64()) != Some(1) {
        return Err("protocol version must be 1".into());
    }
    for key in ["impl", "impl_version", "commit", "file", "phase_reached"] {
        if !obj.get(key).is_some_and(|x| x.is_string()) {
            return Err(format!("missing/invalid required field `{key}`"));
        }
    }
    if !obj.get("seeded").is_some_and(|x| x.is_boolean()) {
        return Err("missing/invalid required field `seeded`".into());
    }
    let diags = obj
        .get("diagnostics")
        .and_then(|d| d.as_array())
        .ok_or("missing/invalid required field `diagnostics`")?;
    for d in diags {
        let ok = d.get("code").is_some_and(|c| c.is_string())
            && d.get("span")
                .and_then(|s| s.as_array())
                .is_some_and(|s| s.len() == 2)
            && d.get("severity").is_some_and(|s| s.is_string());
        if !ok {
            return Err("diagnostic entries need {code, span[2], severity}".into());
        }
    }
    let verdict = obj
        .get("verdict")
        .and_then(|x| x.as_str())
        .ok_or("missing/invalid required field `verdict`")?;
    parse_verdict(verdict)
}

/// Divergence classes per `[proto.cmp.severity]`, descending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Soundness,
    Verdict,
    Diag,
    Stdout,
}

/// Compare two validated records per `[proto.cmp]`. `structural_only`
/// applies the seeded-false concurrency relaxation. `None` = agree (or
/// `unsupported` on either side — the caller tracks the ledger).
pub fn compare(
    a: &serde_json::Value,
    b: &serde_json::Value,
    structural_only: bool,
) -> Option<(Class, String)> {
    let va = validate_record(a).ok()?;
    let vb = validate_record(b).ok()?;
    if va == Verdict::Unsupported || vb == Verdict::Unsupported {
        return None; // conservatism ledger, not divergence
    }
    match (&va, &vb) {
        (Verdict::Ub(x), Verdict::Ub(y)) => {
            if x != y {
                return Some((Class::Soundness, format!("ub({x}) vs ub({y})")));
            }
        }
        (Verdict::Ub(x), other) => {
            return Some((Class::Soundness, format!("ub({x}) vs {other:?}")));
        }
        (other, Verdict::Ub(y)) => {
            return Some((Class::Soundness, format!("{other:?} vs ub({y})")));
        }
        _ => {}
    }
    if va != vb {
        return Some((Class::Verdict, format!("{va:?} vs {vb:?}")));
    }
    if let Verdict::Fail(_) = va {
        // first diagnostic's code+span must agree [proto.cmp.phase]
        let first = |r: &serde_json::Value| {
            r["diagnostics"]
                .as_array()
                .and_then(|d| d.first())
                .map(|d| {
                    (
                        d["code"].as_str().unwrap_or("").to_string(),
                        d["span"].to_string(),
                    )
                })
        };
        if first(a) != first(b) {
            return Some((Class::Diag, format!("{:?} vs {:?}", first(a), first(b))));
        }
    }
    if matches!(va, Verdict::Exit(_)) && !structural_only {
        let sha = |r: &serde_json::Value| r["stdout_sha256"].as_str().map(str::to_string);
        if sha(a) != sha(b) {
            return Some((Class::Stdout, "stdout hash mismatch".into()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record(verdict: &str) -> serde_json::Value {
        json!({
            "protocol": 1, "impl": "t", "impl_version": "0", "commit": "c",
            "file": "f.lu", "phase_reached": "run", "seeded": false,
            "diagnostics": [], "verdict": verdict,
            "stdout_sha256": null, "stdout_inline": null
        })
    }

    #[test]
    fn verdict_grammar() {
        assert_eq!(parse_verdict("pass"), Ok(Verdict::Pass));
        assert_eq!(parse_verdict("exit(3)"), Ok(Verdict::Exit(3)));
        assert_eq!(
            parse_verdict("trap(stale-handle)"),
            Ok(Verdict::Trap("stale-handle".into()))
        );
        assert!(parse_verdict("trap(nonsense)").is_err());
        assert_eq!(
            parse_verdict("ub(mem.ub)"),
            Ok(Verdict::Ub("mem.ub".into()))
        );
    }

    #[test]
    fn wrong_version_rejected() {
        let mut r = record("pass");
        r["protocol"] = json!(2);
        assert!(validate_record(&r).is_err());
    }

    #[test]
    fn ub_vs_defined_is_soundness() {
        let (a, b) = (record("ub(mem.ub)"), record("exit(0)"));
        let (class, _) = compare(&a, &b, false).unwrap();
        assert_eq!(class, Class::Soundness);
    }

    #[test]
    fn unsupported_is_not_divergence() {
        let (a, b) = (record("unsupported"), record("exit(0)"));
        assert!(compare(&a, &b, false).is_none());
    }

    #[test]
    fn equal_records_agree() {
        let (a, b) = (record("trap(overflow)"), record("trap(overflow)"));
        assert!(compare(&a, &b, false).is_none());
    }
}
