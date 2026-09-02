//! The differential protocol (spec/06): observation-record validation and
//! comparison. `xtask differ` and the fixture tests are the consumers;
//! wolf-interp implements the same document independently.

use std::collections::{BTreeMap, BTreeSet};

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
    // `warnings` ([proto.record.warn], s67): optional and additive —
    // honest-absent when the implementation runs no warning analyses —
    // but well-shaped when present: `{code, span[2]}` entries.
    if let Some(w) = obj.get("warnings") {
        let ws = w
            .as_array()
            .ok_or("`warnings` must be an array when present")?;
        for entry in ws {
            let ok = entry.get("code").is_some_and(|c| c.is_string())
                && entry
                    .get("span")
                    .and_then(|s| s.as_array())
                    .is_some_and(|s| s.len() == 2);
            if !ok {
                return Err("warning entries need {code, span[2]}".into());
            }
        }
    }
    let verdict = obj
        .get("verdict")
        .and_then(|x| x.as_str())
        .ok_or("missing/invalid required field `verdict`")?;
    parse_verdict(verdict)
}

/// Divergence classes per `[proto.cmp.severity]`, descending.
///
/// `SpanWidth` (s134, D71 — wolf-lang#220) is a named SUB-class of
/// `Diag`: both sides reject with the same code at the same START
/// byte and disagree only on the span's width. is34's first full
/// three-lane diff-run found eight such rows and could only file them
/// as a waiver, because the report spelled them exactly like a
/// wrong-locus divergence. D71 ruled the width (the span is the
/// offending token), and the class exists so the NEXT width drift is
/// a row that names itself, not a waiver. It is still a divergence —
/// `[proto.cmp.phase]` compares spans byte-exact — and gates like one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Soundness,
    Verdict,
    Diag,
    SpanWidth,
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
            // Both detect UB and agree on the anchor: compare the row
            // where both carry it ([proto.record.ext] — an x- key
            // participates in equality only when both sides have it).
            // A row disagreement is where the two independent s04
            // implementations part on the closed enumeration — a
            // spec-clarification trigger ([proto.cmp.triage]), the
            // highest-severity class because the row names the
            // licensed optimization.
            fn row(r: &serde_json::Value) -> Option<&str> {
                r.get("x-ub-row").and_then(|v| v.as_str())
            }
            if let (Some(rx), Some(ry)) = (row(a), row(b))
                && rx != ry
            {
                return Some((Class::Soundness, format!("ub row mismatch: {rx} vs {ry}")));
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
            // Same code, same start byte, different width: the D71
            // sub-class (s134), named so a width drift reads as what
            // it is instead of as a wrong locus.
            let lo = |r: &serde_json::Value| {
                r["diagnostics"]
                    .as_array()
                    .and_then(|d| d.first())
                    .and_then(|d| d["span"].as_array())
                    .and_then(|s| s.first())
                    .and_then(|v| v.as_u64())
            };
            let code = |r: &serde_json::Value| first(r).map(|(c, _)| c);
            if code(a) == code(b) && lo(a).is_some() && lo(a) == lo(b) {
                return Some((
                    Class::SpanWidth,
                    format!(
                        "same code and start, widths differ: {:?} vs {:?}",
                        first(a),
                        first(b)
                    ),
                ));
            }
            return Some((Class::Diag, format!("{:?} vs {:?}", first(a), first(b))));
        }
    }
    if matches!(va, Verdict::Exit(_)) && !structural_only {
        let sha = |r: &serde_json::Value| r["stdout_sha256"].as_str().map(str::to_string);
        if sha(a) != sha(b) {
            return Some((Class::Stdout, "stdout hash mismatch".into()));
        }
    }
    // Warning parity ([proto.record.warn], s67): compared only when
    // BOTH records carry the array (the additive-key rule of
    // [proto.record.ext] applied to a named field) — the sorted
    // {code, span} sets must agree.
    if let (Some(wa), Some(wb)) = (a.get("warnings"), b.get("warnings")) {
        let set = |w: &serde_json::Value| -> Vec<String> {
            let mut v: Vec<String> = w
                .as_array()
                .map(|ws| {
                    ws.iter()
                        .map(|e| format!("{}@{}", e["code"].as_str().unwrap_or(""), e["span"]))
                        .collect()
                })
                .unwrap_or_default();
            v.sort();
            v
        };
        let (sa, sb) = (set(wa), set(wb));
        if sa != sb {
            return Some((
                Class::Diag,
                format!("warnings [{}] vs [{}]", sa.join(", "), sb.join(", ")),
            ));
        }
    }
    None
}

// ------------------------------------------------------- lane coverage --

/// Does this record show the lane **executing the program**?
/// (`[proto.cmp.coverage]`, s82 — the answer to wolf-lang#90.)
///
/// The run rung can only compare files a lane actually ran, so coverage
/// counts exactly the records carrying a dynamic observation:
/// `phase_reached` is `run` and the verdict is one the run rung can
/// answer — `exit`, `trap`, `ub`. Nothing else counts, and the
/// exclusions are the whole point of the number:
///
/// - `unsupported` is a **refusal** (`[proto.record.unsupported]`). It
///   belongs to the conservatism ledger, and two lanes declining the
///   same file is not agreement about that file. A coverage metric that
///   scored refusals would climb every time a lane got *worse* — worse
///   than publishing no number at all.
/// - `fail(CODE)` is a **rejection** observed at a static rung. It is
///   compared, at that rung, under `[proto.cmp.rung]` — but it is not
///   run-rung coverage, and folding it in here would let a lane that
///   stopped executing programs hide behind one that still rejects them.
/// - `pass` answers an explicit `--phase` request; there is no program
///   outcome in it.
pub fn covered_at_run(record: &serde_json::Value) -> bool {
    if record.get("phase_reached").and_then(|p| p.as_str()) != Some("run") {
        return false;
    }
    let Some(v) = record.get("verdict").and_then(|v| v.as_str()) else {
        return false;
    };
    matches!(
        parse_verdict(v),
        Ok(Verdict::Exit(_) | Verdict::Trap(_) | Verdict::Ub(_))
    )
}

/// Run-rung coverage of one corpus walk, per lane
/// (`[proto.cmp.coverage]`).
///
/// The three run-reaching lanes are **not nested** (wolf-lang#90:
/// `checked` reaches `run` on files `native` declines, and the other way
/// round), so no single lane's count is the coverage of the
/// differential. The union is the honest figure; the intersection is
/// what running one lane and calling it "the run tier" would have you
/// believe. Both are published, because the distance between them is
/// the non-nesting made visible.
#[derive(Debug, Default)]
pub struct Coverage {
    entries: BTreeSet<String>,
    lanes: BTreeMap<String, BTreeSet<String>>,
}

impl Coverage {
    /// Record one lane's observation of one corpus entry. Every entry
    /// is counted in the denominator whether or not any lane ran it —
    /// a file nobody executes is exactly what the number must expose.
    pub fn observe(&mut self, lane: &str, file: &str, record: &serde_json::Value) {
        self.entries.insert(file.to_string());
        let set = self.lanes.entry(lane.to_string()).or_default();
        if covered_at_run(record) {
            set.insert(file.to_string());
        }
    }

    /// Corpus entries walked (the denominator).
    pub fn entries(&self) -> usize {
        self.entries.len()
    }

    /// Entries this lane executed at `run`.
    pub fn lane(&self, lane: &str) -> usize {
        self.lanes.get(lane).map_or(0, BTreeSet::len)
    }

    /// Entries executed by **at least one** of `lanes` — honest coverage.
    pub fn union(&self, lanes: &[&str]) -> BTreeSet<String> {
        let mut u = BTreeSet::new();
        for l in lanes {
            if let Some(s) = self.lanes.get(*l) {
                u.extend(s.iter().cloned());
            }
        }
        u
    }

    /// Entries executed by **every** one of `lanes`.
    pub fn intersection(&self, lanes: &[&str]) -> BTreeSet<String> {
        let mut it = lanes.iter();
        let Some(first) = it.next().and_then(|l| self.lanes.get(*l)) else {
            return BTreeSet::new();
        };
        let mut acc = first.clone();
        for l in it {
            match self.lanes.get(*l) {
                Some(s) => acc.retain(|f| s.contains(f)),
                None => return BTreeSet::new(),
            }
        }
        acc
    }

    /// Entries **no** lane executed — the residue the differential
    /// cannot see at `run`, in walk order.
    pub fn uncovered(&self, lanes: &[&str]) -> Vec<String> {
        let u = self.union(lanes);
        self.entries
            .iter()
            .filter(|f| !u.contains(*f))
            .cloned()
            .collect()
    }

    /// Entries some other lane executed and `lane` did not — this
    /// lane's share of the non-nesting.
    pub fn holes(&self, lane: &str, lanes: &[&str]) -> Vec<String> {
        let mine = self.lanes.get(lane).cloned().unwrap_or_default();
        self.union(lanes)
            .into_iter()
            .filter(|f| !mine.contains(f))
            .collect()
    }
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

    /// D71 (s134, wolf-lang#220): the eight DIV-2026-020 rows were
    /// same-code, same-start, different-width — a class of their own
    /// now, so the next drift is a named row. A different start byte
    /// stays plain `Diag`.
    #[test]
    fn a_width_only_span_drift_is_its_own_class() {
        let mut a = record("fail(E0201)");
        a["diagnostics"] = json!([{"code": "E0201", "span": [550, 550], "severity": "error"}]);
        let mut b = record("fail(E0201)");
        b["diagnostics"] = json!([{"code": "E0201", "span": [550, 551], "severity": "error"}]);
        let (class, detail) = compare(&a, &b, false).unwrap();
        assert_eq!(class, Class::SpanWidth, "{detail}");
        // A different start is a locus divergence, not a width one.
        let mut c = record("fail(E0201)");
        c["diagnostics"] = json!([{"code": "E0201", "span": [364, 365], "severity": "error"}]);
        let (class, _) = compare(&a, &c, false).unwrap();
        assert_eq!(class, Class::Diag);
        // A different code is never width.
        let mut d = record("fail(E0202)");
        d["diagnostics"] = json!([{"code": "E0202", "span": [550, 551], "severity": "error"}]);
        assert_ne!(compare(&a, &d, false).unwrap().0, Class::SpanWidth);
        // Equal spans agree.
        let b2 = b.clone();
        assert!(compare(&b, &b2, false).is_none());
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

    #[test]
    fn warnings_are_additive_and_compared_when_shared() {
        // Absent on one side: never a divergence (honest-absent).
        let mut a = record("pass");
        a["warnings"] = json!([{"code": "W1301", "span": [10, 16]}]);
        let b = record("pass");
        assert!(compare(&a, &b, false).is_none());
        // Present on both and equal: agree.
        let mut b2 = record("pass");
        b2["warnings"] = json!([{"code": "W1301", "span": [10, 16]}]);
        assert!(compare(&a, &b2, false).is_none());
        // Present on both and different: a Diag-class divergence.
        let mut b3 = record("pass");
        b3["warnings"] = json!([]);
        let (class, _) = compare(&a, &b3, false).unwrap();
        assert_eq!(class, Class::Diag);
        // Malformed entries reject at validation.
        let mut bad = record("pass");
        bad["warnings"] = json!([{"code": 7}]);
        assert!(validate_record(&bad).is_err());
    }

    /// A record at a shallower rung: what a refusal actually looks like.
    fn at(verdict: &str, phase: &str) -> serde_json::Value {
        let mut r = record(verdict);
        r["phase_reached"] = json!(phase);
        r
    }

    #[test]
    fn only_dynamic_observations_are_coverage() {
        // The three run-rung outcomes count.
        assert!(covered_at_run(&record("exit(0)")));
        assert!(covered_at_run(&record("trap(overflow)")));
        assert!(covered_at_run(&record("ub(mem.ub)")));
        // A refusal never counts, at any rung — including one that
        // somehow names `run`.
        assert!(!covered_at_run(&at("unsupported", "mem")));
        assert!(!covered_at_run(&at("unsupported", "run")));
        // A rejection is compared at its own rung, never here.
        assert!(!covered_at_run(&at("fail(E1002)", "mem")));
        // `pass` answers a --phase request; no program outcome in it.
        assert!(!covered_at_run(&at("pass", "run")));
        // A dynamic verdict that never reached `run` is not coverage.
        assert!(!covered_at_run(&at("exit(0)", "wir")));
    }

    #[test]
    fn two_refusals_are_conservatism_not_coverage() {
        // The failure mode the metric exists to make impossible: a file
        // both lanes decline must not lift the number.
        let mut c = Coverage::default();
        c.observe("checked", "a.lu", &at("unsupported", "mem"));
        c.observe("native", "a.lu", &at("unsupported", "wir"));
        assert_eq!(c.entries(), 1);
        assert_eq!(c.lane("checked"), 0);
        assert_eq!(c.union(&["checked", "native"]).len(), 0);
        assert_eq!(c.uncovered(&["checked", "native"]), vec!["a.lu"]);
    }

    #[test]
    fn the_lanes_need_not_nest() {
        // wolf-lang#90 in miniature: each lane runs what the other
        // declines, so neither count is the coverage and the union is.
        let mut c = Coverage::default();
        c.observe("checked", "x.lu", &record("exit(0)"));
        c.observe("native", "x.lu", &at("unsupported", "mem"));
        c.observe("checked", "y.lu", &at("unsupported", "mem"));
        c.observe("native", "y.lu", &record("exit(0)"));
        c.observe("checked", "z.lu", &record("exit(0)"));
        c.observe("native", "z.lu", &record("exit(0)"));
        let lanes = ["checked", "native"];
        assert_eq!(c.lane("checked"), 2);
        assert_eq!(c.lane("native"), 2);
        assert_eq!(c.union(&lanes).len(), 3);
        assert_eq!(c.intersection(&lanes).len(), 1);
        assert_eq!(c.holes("checked", &lanes), vec!["y.lu"]);
        assert_eq!(c.holes("native", &lanes), vec!["x.lu"]);
        assert!(c.uncovered(&lanes).is_empty());
    }

    #[test]
    fn a_lane_that_never_ran_contributes_nothing() {
        let mut c = Coverage::default();
        c.observe("checked", "a.lu", &record("exit(0)"));
        assert_eq!(c.lane("release"), 0);
        assert!(c.intersection(&["checked", "release"]).is_empty());
        assert_eq!(c.holes("release", &["checked"]), vec!["a.lu"]);
    }
}
