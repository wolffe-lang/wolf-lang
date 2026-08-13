//! The T1 micro-suite's pure layer (s44, the M2 gate).
//!
//! Everything in this module is a function of its inputs — no processes,
//! no clocks, no filesystem — so the gate's arithmetic is unit-testable
//! and the harness in `main.rs` is left with nothing but plumbing. That
//! split is deliberate: a benchmark gate whose decision procedure cannot
//! be tested is a benchmark gate nobody should believe.
//!
//! # What gates and what does not (D5, sharpened here)
//!
//! s31 learned this the hard way: the first `bench diff --gate` trip was
//! `max_rss +37.8%` on an xtask-only commit, next to wall times
//! "improving" 60%. The rule that came out of it — only DETERMINISTIC
//! metrics gate merges — still holds, and this sprint does not repeal it.
//! What it adds is the machinery that makes wall-derived numbers
//! *publishable*: a per-kernel layout-noise floor (report 10 delta 1),
//! full run distributions, and a tie rule that swallows any win the floor
//! could have manufactured.
//!
//! - **Gating (deterministic, same answer on any machine):** LLVM IR
//!   instruction counts and their ratio ([`ir_ratio`]), vectorization
//!   witness counts ([`count_vectorized_loops`]), correctness sinks.
//! - **Report-only (wall-derived):** every `ns_per_op`, every speedup,
//!   every geomean, the X3 overhead number, the sentinel delta, LLVM's
//!   share of build wall time. These are published with distributions and
//!   floors, and they decide M2 on the dedicated runner over three
//!   consecutive nightly runs — but they never fail a PR.

/// One kernel of the T1 suite, as declared in
/// `bench/kernels/manifest.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct Kernel {
    /// Directory name under `bench/kernels/` and the `bench` key in the
    /// report JSON.
    pub name: String,
    /// Family letter: A aliasing, B arena, C layout, D string, E arith.
    pub family: String,
    /// One line: the fact that is supposed to pay.
    pub thesis: String,
    /// Sweep count for the C/Rust lanes.
    pub ops: u64,
    /// Sweep count for the wolf lanes. Deliberately allowed to differ:
    /// the comparison metric is ns per unit of WORK, so each lane runs
    /// however many sweeps it needs to spend a comparable amount of time
    /// under the clock. Equal sweep counts across lanes that differ by
    /// 100x in speed would give one lane a 30 ms sample and the other a
    /// 3 s sample — that is a resolution artefact, not a measurement.
    pub ops_wolf: u64,
    /// A hand-tuned `expert.c` exists (families A and C).
    pub expert: bool,
    /// A `wrapping.lu` variant exists (family E, the X3 tracker).
    pub wrapping: bool,
    /// Vectorization witnesses are collected for this kernel (A/C/E).
    pub witness: bool,
    /// The correctness sink is a float, so it compares with a relative
    /// tolerance instead of exactly.
    pub sink_float: bool,
    /// Calls the naive-C baseline MUST still make after `clang -O3`, for
    /// the kernel to be measuring its thesis at all (s79). Empty for most
    /// kernels; `["malloc","free"]` for the two whose thesis is
    /// allocation discipline. Checked by `cargo xtask bench-gates`.
    pub baseline_calls: Vec<String>,
}

/// Parse `bench/kernels/manifest.json`. Hand-rolled against
/// `serde_json::Value` so the manifest stays the single source of truth
/// for the suite and adding a kernel never touches Rust.
pub fn parse_manifest(text: &str) -> Result<Vec<Kernel>, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("manifest is not JSON: {e}"))?;
    let arr = v["kernels"]
        .as_array()
        .ok_or_else(|| "manifest has no `kernels` array".to_string())?;
    let mut out = Vec::with_capacity(arr.len());
    for k in arr {
        let name = k["name"]
            .as_str()
            .ok_or_else(|| "kernel without a `name`".to_string())?
            .to_string();
        let need = |field: &str| -> Result<u64, String> {
            k[field]
                .as_u64()
                .ok_or_else(|| format!("{name}: `{field}` must be a positive integer"))
        };
        out.push(Kernel {
            family: k["family"]
                .as_str()
                .ok_or_else(|| format!("{name}: no `family`"))?
                .to_string(),
            thesis: k["thesis"].as_str().unwrap_or("").to_string(),
            ops: need("ops")?,
            ops_wolf: need("ops_wolf")?,
            expert: k["expert"].as_bool().unwrap_or(false),
            wrapping: k["wrapping"].as_bool().unwrap_or(false),
            witness: k["witness"].as_bool().unwrap_or(false),
            sink_float: k["sink"].as_str() == Some("float"),
            baseline_calls: k["baseline_calls"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            name,
        });
    }
    if out.is_empty() {
        return Err("manifest declares no kernels".to_string());
    }
    Ok(out)
}

/// The **layout-noise floor** (report 10 delta 1, GATE-RELEVANT).
///
/// Stabilizer showed that once code/stack/heap layout is randomized, -O3
/// vs -O2 on SPEC CPU2006 is statistically indistinguishable from noise;
/// Mytkowicz showed that environment-block size and link order alone can
/// flip a paper's conclusion. Both effects are the same magnitude as the
/// effect M2 claims, so a suite that does not measure them can manufacture
/// its own result — "refutable by construction", as the amendment puts it.
///
/// Given one median per layout configuration, the floor is the relative
/// spread those configurations produce on their own:
/// `(max - min) / min`. A speedup inside that band is a TIE regardless of
/// what MAD says, because layout alone could have produced it.
///
/// Fewer than two configurations means the axis was not exercised and
/// there is no floor to report — `None`, never a comforting zero.
pub fn layout_noise_floor(medians: &[f64]) -> Option<f64> {
    if medians.len() < 2 {
        return None;
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &m in medians {
        if !m.is_finite() || m <= 0.0 {
            return None;
        }
        lo = lo.min(m);
        hi = hi.max(m);
    }
    Some((hi - lo) / lo)
}

/// One kernel's verdict against one comparison lane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict {
    /// wolf is faster by this many percent.
    Win { pct: f64 },
    /// Inside the layout-noise floor: layout alone could explain it.
    Tie,
    /// wolf is slower by this many percent.
    Loss { pct: f64 },
}

impl Verdict {
    /// The word that goes in the report.
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Win { .. } => "WIN",
            Verdict::Tie => "TIE",
            Verdict::Loss { .. } => "LOSS",
        }
    }

    /// How far wolf is behind, in percent; 0 for ties and wins.
    pub fn loss_pct(self) -> f64 {
        match self {
            Verdict::Loss { pct } => pct,
            _ => 0.0,
        }
    }
}

/// Score wolf against a comparison lane, floor applied.
///
/// `speedup` is `other / wolf` on ns-per-work — greater than one means
/// wolf is faster. `floor` is the kernel's layout-noise floor; `None`
/// (the axis was not exercised) falls back to the 2% practical floor the
/// s01 significance model already uses, and says so by being explicit
/// here rather than silently trusting a raw ratio.
pub fn score(wolf: f64, other: f64, floor: Option<f64>) -> Verdict {
    let band = floor.unwrap_or(0.02).max(0.02);
    let speedup = other / wolf;
    if (speedup - 1.0).abs() <= band {
        return Verdict::Tie;
    }
    if speedup > 1.0 {
        Verdict::Win {
            pct: (speedup - 1.0) * 100.0,
        }
    } else {
        // How much slower wolf is, which is the number the >10% rule
        // in the contract's primary gate is stated against.
        Verdict::Loss {
            pct: (wolf / other - 1.0) * 100.0,
        }
    }
}

/// Geometric mean — the only honest average for ratios (Heiser's
/// checklist calls arithmetic-mean-of-ratios out by name). `None` on an
/// empty sample or any non-positive entry.
pub fn geomean(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    let mut acc = 0.0f64;
    for &x in xs {
        // NaN fails `is_finite`, so this rejects it too — a geomean over a
        // sample with a hole in it must be absent, not zero.
        if !x.is_finite() || x <= 0.0 {
            return None;
        }
        acc += x.ln();
    }
    Some((acc / xs.len() as f64).exp())
}

/// The IR-volume ratio (#70 metric 1): instructions handed to LLVM with
/// the s42 mid-end on, over the same count with the mid-end off (the
/// naive s41 lowering). The contract's budget is `<= 0.50` geomean.
pub fn ir_ratio(with_midend: usize, naive: usize) -> Option<f64> {
    (naive > 0).then(|| with_midend as f64 / naive as f64)
}

/// Count the LLVM IR instructions in a textual module — the number the
/// #70 budget is stated against.
///
/// Deliberately a text count over the `--emit=llvm-ir` surface: that text
/// IS what the backend hands `clang -x ir`, so counting it needs no LLVM
/// bindings (D33) and the artefact stays reviewable. Counted: every line
/// inside a `define` body that is not a label, a brace, a comment or
/// blank. Not counted: globals, `declare`s, metadata definitions,
/// `attributes`, and the module header.
pub fn count_ir_instructions(module: &str) -> usize {
    let mut n = 0usize;
    let mut depth = 0usize;
    for raw in module.lines() {
        let line = raw.trim();
        if line.starts_with("define") {
            // `define ... {` opens on the same line in our emitter, but
            // tolerate a brace on the next line too.
            depth += usize::from(line.ends_with('{'));
            continue;
        }
        if depth == 0 {
            if line == "{" {
                depth = 1;
            }
            continue;
        }
        if line == "}" {
            depth -= 1;
            continue;
        }
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        // A basic-block label: `name:` with nothing after it.
        if line.ends_with(':') && !line.contains(' ') {
            continue;
        }
        n += 1;
    }
    n
}

/// Count vectorized loops in an LLVM optimization-remarks stream
/// (`-Rpass=loop-vectorize`) — the vectorization witness (report 10
/// delta 2). A kernel whose loop stops vectorizing fails visibly here
/// before wall time moves on the current machine, which is the point:
/// this number is deterministic, so it can gate.
pub fn count_vectorized_loops(remarks: &str) -> usize {
    remarks
        .lines()
        .filter(|l| l.contains("vectorized loop") || l.contains("interleaved loop"))
        .count()
}

/// One tick of wolf's millisecond clock as a fraction of a measured wall
/// time (#84). Every wolf number in the report is read THROUGH this: a
/// delta smaller than the quantum is one bucket, not a result.
///
/// s44 sized the suite so this was ≤ 0.7% and s75/s77 then made the wolf
/// lane up to 100x faster without anybody re-checking, so by s78 two
/// kernels were being read at ONE TICK — 100% quantisation — and a family
/// geomean was being quoted off it. Publishing the number per lane is how
/// that stops being invisible.
pub fn clock_quantum_frac(wall_ns: f64) -> Option<f64> {
    (wall_ns.is_finite() && wall_ns > 0.0).then(|| MS_IN_NS / wall_ns)
}

/// One millisecond, in nanoseconds: the resolution of `time_now_ms`, the
/// only monotonic clock the language exposes to a program.
pub const MS_IN_NS: f64 = 1.0e6;

/// Is a measured delta big enough to be a reading rather than a bucket?
/// Both arguments are fractions; `delta` may be signed.
pub fn delta_is_readable(delta: f64, quantum: f64) -> bool {
    delta.is_finite() && quantum.is_finite() && delta.abs() >= quantum
}

/// Which of the calls a kernel's baseline is REQUIRED to make are missing
/// from the compiled binary's undefined-symbol list.
///
/// The s79 instrument for a fault the rig had no way to see: `b3_churn`'s
/// `ref.c` mallocs a buffer that provably does not escape, so clang
/// deleted the allocation and the "malloc/free per request" baseline
/// executed no allocation at all for five sprints. Nothing noticed,
/// because nothing was looking — a baseline that does not do its work
/// just looks fast, and a fast baseline only ever makes wolf look worse,
/// which is the direction nobody audits.
///
/// Symbol presence is a property of the binary, so this is deterministic
/// for a given toolchain and can gate. It is not stable ACROSS clang
/// versions — but a clang that starts deleting our baseline's workload is
/// precisely the event worth a red build.
pub fn missing_baseline_calls(nm_output: &str, required: &[String]) -> Vec<String> {
    required
        .iter()
        .filter(|want| {
            !nm_output.lines().any(|l| {
                l.split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .any(|tok| tok == want.as_str())
            })
        })
        .cloned()
        .collect()
}

/// Below this many nanoseconds per unit of work, a lane did not execute
/// the workload — it proved the answer and printed it.
///
/// One unit of work is at minimum a load, an arithmetic op and a compare.
/// Even a 16-wide vector unit at 5 GHz cannot retire that in under ~0.01 ns
/// per unit, so anything faster is a folded loop, and a folded loop must
/// contribute no number: comparing against it would be measuring the
/// constant folder. This caught `e3_index_arith`'s Rust lane, where rustc
/// solved the closed form of a loop over a compile-time-constant bound and
/// "beat" wolf by 200000x.
pub const FOLDED_NS_PER_OP: f64 = 0.01;

/// Did this lane actually run the workload? See [`FOLDED_NS_PER_OP`].
pub fn lane_ran_the_work(ns_per_op: f64) -> bool {
    ns_per_op.is_finite() && ns_per_op >= FOLDED_NS_PER_OP
}

/// The parsed exception ledger (`bench/bench-exceptions.md`).
///
/// Machine-readable by one marker line per entry — `- exception: <kernel>`
/// — so the ledger stays a document a human reads and the gate cannot
/// drift from it.
pub fn parse_exceptions(md: &str) -> Vec<String> {
    md.lines()
        .filter_map(|l| l.trim().strip_prefix("- exception:"))
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect()
}

/// One kernel's scored line in the gate report.
#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub kernel: String,
    pub family: String,
    /// `other / wolf` on ns-per-work: greater than one means wolf wins.
    pub speedup: f64,
    pub floor: Option<f64>,
    pub verdict: Verdict,
}

/// The M2 gate's decision, with every reason it reached it.
#[derive(Debug, Clone, PartialEq)]
pub struct GateOutcome {
    /// Geomean speedup vs naive `clang -O3` across the whole suite.
    pub geomean: Option<f64>,
    /// Per-family geomeans, sorted by family letter. Published because a
    /// suite-wide geomean that hides a family-shaped loss is exactly the
    /// dishonesty this sprint exists to prevent.
    pub by_family: Vec<(String, f64)>,
    /// Kernels losing by more than 10% with no documented exception.
    pub undocumented_losses: Vec<String>,
    /// Kernels losing by more than 10% that ARE documented.
    pub excepted_losses: Vec<String>,
    /// Exceptions declared in the ledger (cap: 3).
    pub exceptions_declared: usize,
    /// True only if every clause of the contract's primary gate holds.
    pub primary_holds: bool,
}

/// Evaluate the contract's primary M2 gate over scored kernels.
///
/// Primary (contract §3): geomean speedup vs naive `clang -O3` >= 1.00,
/// no individual kernel losing by more than 10% unless covered by a
/// documented exception, exceptions capped at 3.
pub fn evaluate_gate(scored: &[Scored], exceptions: &[String]) -> GateOutcome {
    let all: Vec<f64> = scored.iter().map(|s| s.speedup).collect();
    let mut fams: std::collections::BTreeMap<String, Vec<f64>> = std::collections::BTreeMap::new();
    for s in scored {
        fams.entry(s.family.clone()).or_default().push(s.speedup);
    }
    let by_family: Vec<(String, f64)> = fams
        .into_iter()
        .filter_map(|(f, xs)| geomean(&xs).map(|g| (f, g)))
        .collect();
    let mut undocumented_losses = Vec::new();
    let mut excepted_losses = Vec::new();
    for s in scored {
        if s.verdict.loss_pct() > 10.0 {
            if exceptions.iter().any(|e| e == &s.kernel) {
                excepted_losses.push(s.kernel.clone());
            } else {
                undocumented_losses.push(s.kernel.clone());
            }
        }
    }
    let geomean_v = geomean(&all);
    let primary_holds = geomean_v.is_some_and(|g| g >= 1.00)
        && undocumented_losses.is_empty()
        && exceptions.len() <= 3;
    GateOutcome {
        geomean: geomean_v,
        by_family,
        undocumented_losses,
        excepted_losses,
        exceptions_declared: exceptions.len(),
        primary_holds,
    }
}

/// Compare two correctness sinks across lanes. Integers must agree
/// exactly; floats agree to a relative tolerance, because the lanes are
/// separate compilers over IEEE arithmetic, not one program twice.
pub fn sinks_agree(a: &str, b: &str, float: bool) -> bool {
    if a == b {
        return true;
    }
    if !float {
        return false;
    }
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => {
            let scale = x.abs().max(y.abs()).max(1.0);
            (x - y).abs() / scale <= 1e-12
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trip() {
        let ks = parse_manifest(
            r#"{"kernels":[
                {"name":"e1","family":"E","thesis":"t","ops":10,"ops_wolf":3,
                 "wrapping":true,"witness":true},
                {"name":"a1","family":"A","thesis":"u","ops":20,"ops_wolf":1,
                 "expert":true,"sink":"float","baseline_calls":["malloc","free"]}]}"#,
        )
        .expect("parses");
        assert_eq!(ks.len(), 2);
        assert_eq!(ks[0].name, "e1");
        assert!(ks[0].wrapping && ks[0].witness && !ks[0].expert && !ks[0].sink_float);
        assert!(ks[0].baseline_calls.is_empty());
        assert!(ks[1].expert && ks[1].sink_float);
        assert_eq!(ks[1].ops_wolf, 1);
        assert_eq!(ks[1].baseline_calls, vec!["malloc", "free"]);
    }

    #[test]
    fn manifest_rejects_junk() {
        assert!(parse_manifest("{}").is_err());
        assert!(parse_manifest(r#"{"kernels":[]}"#).is_err());
        assert!(parse_manifest(r#"{"kernels":[{"family":"A","ops":1,"ops_wolf":1}]}"#).is_err());
        // A missing ops field must fail loudly, not default to zero.
        assert!(parse_manifest(r#"{"kernels":[{"name":"k","family":"A","ops":1}]}"#).is_err());
    }

    #[test]
    fn floor_is_the_relative_spread() {
        let f = layout_noise_floor(&[100.0, 104.0, 102.0]).expect("floor");
        assert!((f - 0.04).abs() < 1e-12, "got {f}");
        // One configuration exercises no axis: no floor, not zero.
        assert_eq!(layout_noise_floor(&[100.0]), None);
        assert_eq!(layout_noise_floor(&[]), None);
        assert_eq!(layout_noise_floor(&[100.0, 0.0]), None);
    }

    #[test]
    fn a_win_below_the_floor_is_a_tie() {
        // wolf 100, C 103 -> 3% "win", but layout alone moves 5%.
        assert_eq!(score(100.0, 103.0, Some(0.05)), Verdict::Tie);
        // Same numbers, a quiet machine: now it is a win.
        match score(100.0, 103.0, Some(0.005)) {
            Verdict::Win { pct } => assert!((pct - 3.0).abs() < 1e-9, "got {pct}"),
            v => panic!("expected win, got {v:?}"),
        }
    }

    #[test]
    fn a_loss_below_the_floor_is_also_a_tie() {
        assert_eq!(score(103.0, 100.0, Some(0.05)), Verdict::Tie);
    }

    #[test]
    fn loss_percent_is_how_much_slower_wolf_is() {
        // wolf takes 200 where C takes 100: wolf is 100% slower.
        match score(200.0, 100.0, Some(0.02)) {
            Verdict::Loss { pct } => assert!((pct - 100.0).abs() < 1e-9, "got {pct}"),
            v => panic!("expected loss, got {v:?}"),
        }
    }

    #[test]
    fn floor_never_drops_below_the_practical_two_percent() {
        // A suspiciously quiet floor cannot license a 1% "win".
        assert_eq!(score(100.0, 101.0, Some(0.0)), Verdict::Tie);
        assert_eq!(score(100.0, 101.0, None), Verdict::Tie);
    }

    #[test]
    fn geomean_of_ratios() {
        let g = geomean(&[0.5, 2.0]).expect("geomean");
        assert!((g - 1.0).abs() < 1e-12, "got {g}");
        assert_eq!(geomean(&[]), None);
        assert_eq!(geomean(&[1.0, 0.0]), None);
    }

    #[test]
    fn ir_instruction_counting_ignores_scaffolding() {
        let ir = concat!(
            "; wolf release tier\n",
            "source_filename = \"wolf\"\n",
            "@\"_W.str.0\" = private constant [2 x i8] c\"a\\00\"\n",
            "\n",
            "define internal i64 @\"f\"() nounwind {\n",
            "b0:\n",
            "  %t1 = add i64 1, 2\n",
            "  ; a comment\n",
            "  ret i64 %t1\n",
            "}\n",
            "declare void @\"__wolf_rt_trap\"(i32) nounwind\n",
            "!0 = !{!\"branch_weights\", i32 1, i32 2000}\n",
        );
        assert_eq!(count_ir_instructions(ir), 2);
    }

    #[test]
    fn ir_ratio_needs_a_denominator() {
        assert_eq!(ir_ratio(50, 100), Some(0.5));
        assert_eq!(ir_ratio(1, 0), None);
    }

    #[test]
    fn vectorization_witness_counts_remarks() {
        let remarks = concat!(
            "ref.c:13:5: remark: vectorized loop (vectorization width: 4, ",
            "interleaved count: 2) [-Rpass=loop-vectorize]\n",
            "ref.c:20:5: remark: loop not vectorized [-Rpass-missed=loop-vectorize]\n",
            "ref.c:31:5: remark: interleaved loop (interleaved count: 4)\n",
        );
        assert_eq!(count_vectorized_loops(remarks), 2);
    }

    #[test]
    fn exceptions_are_read_from_marker_lines() {
        let md = "# ledger\n\nprose\n- exception: a1_triad\n  - exception: b2_json\n- other: x\n";
        assert_eq!(parse_exceptions(md), vec!["a1_triad", "b2_json"]);
    }

    #[test]
    fn gate_fails_on_an_undocumented_big_loss() {
        let scored = vec![
            Scored {
                kernel: "e1".into(),
                family: "E".into(),
                speedup: 2.0,
                floor: Some(0.01),
                verdict: score(100.0, 200.0, Some(0.01)),
            },
            Scored {
                kernel: "a1".into(),
                family: "A".into(),
                speedup: 0.5,
                floor: Some(0.01),
                verdict: score(200.0, 100.0, Some(0.01)),
            },
        ];
        let out = evaluate_gate(&scored, &[]);
        // geomean(2.0, 0.5) == 1.0, which clears the geomean clause —
        // and the gate still fails, because a2 loses by 100% with no
        // exception. This is the clause that stops a geomean from
        // laundering a family-shaped loss.
        assert!((out.geomean.expect("geomean") - 1.0).abs() < 1e-12);
        assert_eq!(out.undocumented_losses, vec!["a1"]);
        assert!(!out.primary_holds);
        // Document it and the same numbers pass.
        let out = evaluate_gate(&scored, &["a1".to_string()]);
        assert_eq!(out.excepted_losses, vec!["a1"]);
        assert!(out.primary_holds);
    }

    #[test]
    fn gate_fails_when_too_many_exceptions_are_declared() {
        let scored = vec![Scored {
            kernel: "e1".into(),
            family: "E".into(),
            speedup: 1.5,
            floor: Some(0.01),
            verdict: score(100.0, 150.0, Some(0.01)),
        }];
        let four = [
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        assert!(!evaluate_gate(&scored, &four).primary_holds);
        assert!(evaluate_gate(&scored, &four[..3]).primary_holds);
    }

    #[test]
    fn gate_reports_families_separately() {
        let mk = |k: &str, f: &str, s: f64| Scored {
            kernel: k.into(),
            family: f.into(),
            speedup: s,
            floor: Some(0.01),
            verdict: Verdict::Tie,
        };
        let out = evaluate_gate(
            &[mk("e1", "E", 4.0), mk("e2", "E", 1.0), mk("a1", "A", 0.25)],
            &[],
        );
        let fams: Vec<&str> = out.by_family.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(fams, vec!["A", "E"]);
        assert!((out.by_family[0].1 - 0.25).abs() < 1e-12);
        assert!((out.by_family[1].1 - 2.0).abs() < 1e-12);
    }

    #[test]
    fn the_clock_quantum_is_one_tick_over_the_wall() {
        // 250 ms under a 1 ms clock: 0.4% per bucket.
        let q = clock_quantum_frac(250.0e6).expect("quantum");
        assert!((q - 0.004).abs() < 1e-12, "got {q}");
        // The s78 state of family A: a 1 ms reading is ONE bucket.
        assert_eq!(clock_quantum_frac(1.0e6), Some(1.0));
        assert_eq!(clock_quantum_frac(0.0), None);
        assert_eq!(clock_quantum_frac(f64::NAN), None);
    }

    #[test]
    fn a_delta_inside_one_bucket_is_not_a_reading() {
        // The sentinel's s75/s78 reading: +0.0% against a 17-50% bucket.
        assert!(!delta_is_readable(0.0, 0.17));
        // s79's family A: a few percent against a 0.5% bucket.
        assert!(delta_is_readable(0.0385, 0.0077));
        // Sign does not matter — a negative bonus is still a reading.
        assert!(delta_is_readable(-0.0872, 0.005));
        assert!(!delta_is_readable(f64::NAN, 0.005));
    }

    #[test]
    fn a_baseline_that_lost_its_allocation_is_named() {
        // What `nm -u` looks like on the fixed b3_churn baseline.
        let good = "                 U malloc@GLIBC_2.2.5\n                 U free@GLIBC_2.2.5\n";
        let want = vec!["malloc".to_string(), "free".to_string()];
        assert!(missing_baseline_calls(good, &want).is_empty());
        // And on the one clang optimized the allocation out of.
        let elided = "                 U printf@GLIBC_2.2.5\n";
        assert_eq!(missing_baseline_calls(elided, &want), want);
        // A substring must not count: `mallocate` is not `malloc`.
        assert_eq!(
            missing_baseline_calls("  U mallocate\n", &["malloc".to_string()]),
            vec!["malloc"]
        );
        // No requirement, no finding.
        assert!(missing_baseline_calls("", &[]).is_empty());
    }

    #[test]
    fn a_folded_lane_contributes_no_number() {
        assert!(!lane_ran_the_work(5.077e-06));
        assert!(!lane_ran_the_work(f64::NAN));
        assert!(lane_ran_the_work(0.15));
        assert!(lane_ran_the_work(FOLDED_NS_PER_OP));
    }

    #[test]
    fn sink_comparison_is_exact_for_integers() {
        assert!(sinks_agree("42", "42", false));
        assert!(!sinks_agree("42", "43", false));
        // Floats from three different compilers agree to a tolerance.
        assert!(sinks_agree("4096.0020485000005", "4096.002048500001", true));
        assert!(!sinks_agree("4096.0", "4097.0", true));
        assert!(!sinks_agree("nonsense", "4096.0", true));
    }
}
