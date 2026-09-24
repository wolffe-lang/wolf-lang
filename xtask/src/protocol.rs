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
    // The file index ([proto.record.diag], s181 — #437): optional and
    // additive, but all-or-nothing. With `files` present every
    // diagnostic carries an in-range integer `file`; without it none
    // does. `files[0]` is the entry, so the array is never empty.
    match obj.get("files") {
        None => {
            if diags.iter().any(|d| d.get("file").is_some()) {
                return Err("a diagnostic carries `file` but the record has no `files`".into());
            }
        }
        Some(f) => {
            let fs = f
                .as_array()
                .filter(|fs| !fs.is_empty() && fs.iter().all(|x| x.is_string()))
                .ok_or("`files` must be a non-empty array of paths when present")?;
            for d in diags {
                let Some(i) = d.get("file").and_then(|i| i.as_u64()) else {
                    return Err(
                        "with `files` present, every diagnostic carries an integer `file`".into(),
                    );
                };
                if i as usize >= fs.len() {
                    return Err(format!(
                        "diagnostic `file` {i} is past `files` ({})",
                        fs.len()
                    ));
                }
            }
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
    // `trap_message` ([proto.record.trap], s169): optional and additive
    // — the program's own words for its fault, honest-absent when the
    // implementation does not hold them — but well-shaped when present,
    // and only on a trap. It is NEVER compared (see `compare`).
    if let Some(m) = obj.get("trap_message")
        && !m.is_string()
    {
        return Err("`trap_message` must be a string when present".into());
    }
    let verdict = obj
        .get("verdict")
        .and_then(|x| x.as_str())
        .ok_or("missing/invalid required field `verdict`")?;
    let verdict = parse_verdict(verdict)?;
    if obj.contains_key("trap_message") && !matches!(verdict, Verdict::Trap(_)) {
        return Err("`trap_message` belongs only on a `trap(kind)` verdict".into());
    }
    Ok(verdict)
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
    // [proto.cmp.pass] (s169): a `pass` side stopped WITHOUT EXECUTING,
    // so against a dynamic verdict the two records are not two answers
    // to one question — there is nothing to compare. The carve-out is
    // exactly this wide: `pass` vs `fail(CODE)` falls through to the
    // verdict comparison below and IS a divergence, because there both
    // sides answered the static question and answered it differently.
    // That pair was INVISIBLE before s169 (wolfgang spelled its clean
    // stop `unsupported`, which the clause above excuses), so the
    // amendment opens a hole rather than widening one.
    let dynamic = |v: &Verdict| matches!(v, Verdict::Exit(_) | Verdict::Trap(_) | Verdict::Ub(_));
    if (va == Verdict::Pass && dynamic(&vb)) || (vb == Verdict::Pass && dynamic(&va)) {
        return None;
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
        // first diagnostic's code+span must agree [proto.cmp.phase],
        // and the file it names ([proto.record.diag]'s file index,
        // s181): the index resolves to its path, and a diagnostic
        // without `file` (or `file: 0`) names the entry.
        let first = |r: &serde_json::Value| {
            r["diagnostics"]
                .as_array()
                .and_then(|d| d.first())
                .map(|d| {
                    let file = match d.get("file").and_then(|i| i.as_u64()) {
                        None | Some(0) => "<entry>".to_string(),
                        Some(i) => r["files"][i as usize].as_str().unwrap_or("").to_string(),
                    };
                    (
                        d["code"].as_str().unwrap_or("").to_string(),
                        d["span"].to_string(),
                        file,
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
            let code = |r: &serde_json::Value| first(r).map(|(c, _, _)| c);
            let file = |r: &serde_json::Value| first(r).map(|(_, _, f)| f);
            if code(a) == code(b) && file(a) == file(b) && lo(a).is_some() && lo(a) == lo(b) {
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
    let sha = |r: &serde_json::Value| r["stdout_sha256"].as_str().map(str::to_string);
    if matches!(va, Verdict::Exit(_)) && !structural_only && sha(a) != sha(b) {
        return Some((Class::Stdout, "stdout hash mismatch".into()));
    }
    // A trap's output is compared when BOTH records carry it
    // ([proto.cmp.phase], s163 — wolf-lang#216, ruled B25): what a
    // program wrote before its fault is an observation exactly as an
    // exiting program's is. An absent digest on either side is the
    // honest-absent of [proto.record.fields], never a divergence; `ub`
    // output is never compared.
    if matches!(va, Verdict::Trap(_))
        && !structural_only
        && let (Some(x), Some(y)) = (sha(a), sha(b))
        && x != y
    {
        return Some((Class::Stdout, "trap stdout hash mismatch".into()));
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
/// - `pass` is the ladder's clean stop (`[proto.record.pass]`): an
///   explicit `--phase` request, or a full-ladder run that reached the
///   deepest rung its lane implements. Either way nothing executed, so
///   there is no program outcome in it. s169 made wolfgang's default
///   lane spell its clean stop `pass` instead of `unsupported`, which
///   moves entries between two EXCLUDED classes and cannot move this
///   number — the `default` lane's count stays 0, which is exactly the
///   evidence `RUN_LANES` carries it for.
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

    /// [proto.cmp.phase] (s163, wolf-lang#216): a trap compares kind AND
    /// the stdout digest when both records carry one; one side's absent
    /// digest is not a divergence; `ub` bytes are never compared.
    #[test]
    fn a_trap_compares_its_stdout_when_both_carry_it() {
        let with = |v: &str, sha: Option<&str>| {
            let mut r = record(v);
            r["stdout_sha256"] = sha.map_or(json!(null), |s| json!(s));
            r
        };
        let same = compare(
            &with("trap(assert)", Some("aa")),
            &with("trap(assert)", Some("aa")),
            false,
        );
        assert!(same.is_none());
        let (class, detail) = compare(
            &with("trap(assert)", Some("aa")),
            &with("trap(assert)", Some("bb")),
            false,
        )
        .unwrap();
        assert_eq!(class, Class::Stdout, "{detail}");
        assert!(
            compare(
                &with("trap(assert)", Some("aa")),
                &with("trap(assert)", None),
                false
            )
            .is_none(),
            "an absent digest is honest-absent, not a divergence"
        );
        assert!(
            compare(
                &with("trap(assert)", Some("aa")),
                &with("trap(assert)", Some("bb")),
                true
            )
            .is_none(),
            "structural comparison never reads bytes"
        );
        assert!(
            compare(
                &with("ub(mem.ub)", Some("aa")),
                &with("ub(mem.ub)", Some("bb")),
                false
            )
            .is_none(),
            "ub output is never compared"
        );
        // The kind still decides first.
        let (class, _) = compare(
            &with("trap(assert)", Some("aa")),
            &with("trap(bounds)", Some("aa")),
            false,
        )
        .unwrap();
        assert_eq!(class, Class::Verdict);
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

    /// `[proto.cmp.pass]`, in both directions and at both edges. The
    /// second half is the one that matters: a `pass` against a
    /// REJECTION must still be a divergence, or the amendment would
    /// have bought a wider excuse than the one it replaced.
    #[test]
    fn pass_against_a_run_is_not_divergence_but_against_a_rejection_it_is() {
        for dynamic in ["exit(0)", "exit(7)", "trap(overflow)", "ub(mem.ub)"] {
            let (p, d) = (record("pass"), record(dynamic));
            assert!(
                compare(&p, &d, false).is_none(),
                "pass vs {dynamic} compares nothing — the passing side did not execute"
            );
            assert!(
                compare(&d, &p, false).is_none(),
                "{dynamic} vs pass, the other way round"
            );
        }
        // The hole the amendment OPENS: one side completed the static
        // ladder clean, the other rejected the program. Two answers to
        // one question, and they differ.
        let (p, f) = (record("pass"), record("fail(E0401)"));
        assert_eq!(
            compare(&p, &f, false).map(|(c, _)| c),
            Some(Class::Verdict),
            "pass vs fail is a disagreement about the language"
        );
        assert_eq!(compare(&f, &p, false).map(|(c, _)| c), Some(Class::Verdict));
        // And `pass` vs `pass` agrees whatever rung each stopped at.
        let mut a = record("pass");
        a["phase_reached"] = json!("wir");
        let mut b = record("pass");
        b["phase_reached"] = json!("mem");
        assert!(compare(&a, &b, false).is_none());
    }

    /// `[proto.record.trap]`: additive, shape-checked, trap-only, and
    /// never compared.
    #[test]
    fn trap_message_is_additive_shaped_and_never_compared() {
        let mut a = record("trap(assert)");
        a["trap_message"] = json!("one is not two");
        let b = record("trap(assert)");
        assert!(validate_record(&a).is_ok());
        // Absent on one side, different on the other: neither diverges.
        assert!(compare(&a, &b, false).is_none());
        let mut b2 = record("trap(assert)");
        b2["trap_message"] = json!("ein ist nicht zwei");
        assert!(
            compare(&a, &b2, false).is_none(),
            "wording is never comparison surface ([proto.record.diag]'s rule)"
        );
        // Shape: a string, and only on a trap.
        let mut bad = record("trap(assert)");
        bad["trap_message"] = json!(["not", "a", "string"]);
        assert!(validate_record(&bad).is_err());
        let mut misplaced = record("exit(0)");
        misplaced["trap_message"] = json!("a program that exited has no fault to explain");
        assert!(validate_record(&misplaced).is_err());
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
