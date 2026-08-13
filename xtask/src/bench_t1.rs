//! The T1 micro-suite harness (s44, the M2 gate) — the process-driving
//! half; the arithmetic lives in `xtask::t1` where it can be tested.
//!
//! Three commands sit here:
//!
//! - `bench --track=t1` runs the suite: build every lane, check the
//!   correctness sinks agree, measure with paired interleaved runs across
//!   several LAYOUT CONFIGURATIONS, compute each kernel's layout-noise
//!   floor, and emit every sample into the report JSON.
//! - `bench --track=irvolume` measures issue #70's two metrics: the LLVM
//!   IR instruction count handed to LLVM against the naive s41 lowering,
//!   and LLVM's share of Tier-R build wall time.
//! - `bench gate <t1.jsonl>` re-derives the M2 verdict from stored data,
//!   which is how the nightly lane declares (or refuses) M2 without
//!   re-running anything.
//!
//! # Why the layout configurations exist
//!
//! Stabilizer: under randomized layout, -O3 vs -O2 on SPEC CPU2006 is
//! indistinguishable from noise. Mytkowicz: environment-block size and
//! link order alone flip conclusions. Both effects are the size of the
//! effect M2 claims, so the harness perturbs what it can perturb and
//! reports the resulting spread as a floor. Axes exercised here:
//! environment-block size (three sizes) and ASLR on/off (via `setarch
//! -R`, skipped where unavailable).
//!
//! Link order is the third axis report 10 names, and it is **not**
//! exercised: a single-object kernel has no link order, and wolf's driver
//! links internally with no ordering knob. That is recorded as a known
//! limitation in `bench/protocol.md` rather than papered over — and the
//! kernel floor takes the MAXIMUM of the wolf and naive-C floors, which
//! is the conservative direction (a bigger floor swallows more claimed
//! wins).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

use xtask::stats;
use xtask::t1::{self, Kernel, Scored, Verdict};

/// One layout configuration. `pad` is the size of a junk environment
/// variable, which moves the initial stack and everything anchored to it;
/// `aslr` false runs under `setarch -R`.
struct Layout {
    name: &'static str,
    pad: usize,
    aslr: bool,
}

/// L0 is the reference configuration every reported median comes from;
/// L1..L3 exist to measure how much the layout alone can move it.
const LAYOUTS: &[Layout] = &[
    Layout {
        name: "L0",
        pad: 0,
        aslr: true,
    },
    Layout {
        name: "L1",
        pad: 4096,
        aslr: true,
    },
    Layout {
        name: "L2",
        pad: 65536,
        aslr: true,
    },
    Layout {
        name: "L3",
        pad: 0,
        aslr: false,
    },
];

/// The lanes whose layout floor is measured: the gated pair. Everything
/// else is measured at L0 only — the floor's job is to police the gated
/// comparison, and sweeping every lane over every layout would quadruple
/// a suite that already takes minutes.
const FLOOR_LANES: &[&str] = &["wolf", "c_naive"];

/// A built, runnable lane.
struct Built {
    lane: &'static str,
    bin: PathBuf,
    ops: u64,
    /// The toolchain/flags string that lands in the report JSON.
    config: String,
}

/// `setarch -R` availability, probed once.
fn setarch_available() -> bool {
    Command::new("setarch")
        .arg("--help")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run one lane once under one layout, returning (ns per unit of work,
/// sink text). `None` on any failure — a lane that cannot run must not
/// silently contribute a number.
fn sample(b: &Built, l: &Layout, setarch: bool) -> Option<(f64, String)> {
    let mut cmd = if !l.aslr && setarch {
        let mut c = Command::new("setarch");
        c.arg("-R").arg(&b.bin);
        c
    } else if !l.aslr {
        return None; // the axis is unavailable; do not fake it
    } else {
        Command::new(&b.bin)
    };
    cmd.arg(b.ops.to_string());
    if l.pad > 0 {
        cmd.env("WOLF_BENCH_LAYOUT_PAD", "x".repeat(l.pad));
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let ns = v["ns"].as_f64()?;
    let ops = v["ops"].as_f64()?;
    if ops <= 0.0 || ns <= 0.0 {
        return None;
    }
    let sink = match &v["sink"] {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        _ => return None,
    };
    Some((ns / ops, sink))
}

/// One report-JSON record. The frozen s01 schema (`bench`, `track`,
/// `lang`, `metric`, `value`, `unit`, `commit`, `config`, `style`) plus
/// additive s44 keys — `bench diff` keys on the frozen five and ignores
/// the rest, so extending is safe and dropping one is not.
#[allow(clippy::too_many_arguments)]
fn rec(
    kernel: &Kernel,
    lane: &str,
    metric: &str,
    value: f64,
    unit: &str,
    commit: &str,
    config: &str,
    extra: &[(&str, serde_json::Value)],
) -> serde_json::Value {
    let mut v = serde_json::json!({
        "bench": kernel.name, "track": "t1", "lang": lane, "metric": metric,
        "value": value, "unit": unit, "commit": commit, "config": config,
        "style": crate::style_version(), "family": kernel.family,
    });
    for (k, val) in extra {
        v[*k] = val.clone();
    }
    v
}

/// Build every lane of one kernel. Missing toolchains skip their lane
/// loudly; a missing wolf or naive-C lane fails the kernel outright
/// because the gated comparison is exactly those two.
fn build_lanes(k: &Kernel, dir: &Path, bin_dir: &Path) -> Option<Vec<Built>> {
    let wolf = Path::new("target/debug/wolf");
    let mut out: Vec<Built> = Vec::new();
    let bin = |suffix: &str| bin_dir.join(format!("{}_{suffix}", k.name));
    let s = |p: &Path| p.to_str().expect("utf8 path").to_string();

    // --- wolf, the subject -------------------------------------------
    //
    // `--no-cache` on every wolf lane, for two reasons: a benchmark lane
    // must never measure a cache hit, and the package cache lands NEXT TO
    // the entry file, so caching would have the harness writing build
    // artefacts into the source tree it is benchmarking.
    let wbin = bin("wolf");
    let built = Command::new(wolf)
        .args(["build", &s(&dir.join("kernel.lu")), "-o", &s(&wbin)])
        .arg("--release")
        .arg("--no-cache")
        .output()
        .ok()?;
    if !built.status.success() {
        eprintln!(
            "t1: {}: wolf lane failed to build:\n{}",
            k.name,
            String::from_utf8_lossy(&built.stderr)
        );
        return None;
    }
    out.push(Built {
        lane: "wolf",
        bin: wbin,
        ops: k.ops_wolf,
        config: format!("wolf build --release (clang {})", clang_version()),
    });

    // --- the gated comparison: naive C -------------------------------
    let cbin = bin("c_naive");
    if !crate::run_ok("clang", &["-O3", "-o", &s(&cbin), &s(&dir.join("ref.c"))]) {
        eprintln!("t1: {}: naive C lane failed to build", k.name);
        return None;
    }
    out.push(Built {
        lane: "c_naive",
        bin: cbin,
        ops: k.ops,
        config: format!("clang -O3 ({})", clang_version()),
    });

    // --- secondary, report-only: expert C and Rust -------------------
    if k.expert {
        let b = bin("c_expert");
        if crate::run_ok("clang", &["-O3", "-o", &s(&b), &s(&dir.join("expert.c"))]) {
            out.push(Built {
                lane: "c_expert",
                bin: b,
                ops: k.ops,
                config: format!("clang -O3, hand-tuned source ({})", clang_version()),
            });
        }
    }
    let rbin = bin("rust");
    if crate::run_ok(
        "rustc",
        &[
            "-O",
            "--edition=2024",
            "-o",
            &s(&rbin),
            &s(&dir.join("ref.rs")),
        ],
    ) {
        out.push(Built {
            lane: "rust",
            bin: rbin,
            ops: k.ops,
            config: format!("rustc -O ({})", rustc_version()),
        });
    }

    // --- the X3 tracker: the same wolf kernel, unchecked -------------
    if k.wrapping {
        let b = bin("wolf_wrapping");
        if crate::run_ok(
            "target/debug/wolf",
            &[
                "build",
                &s(&dir.join("wrapping.lu")),
                "-o",
                &s(&b),
                "--release",
                "--no-cache",
            ],
        ) {
            out.push(Built {
                lane: "wolf_wrapping",
                bin: b,
                ops: k.ops_wolf,
                config: "wolf build --release, wrapping[int] accumulators".to_string(),
            });
        }
    }

    // --- the metadata-drop sentinel (report 10 delta 3) --------------
    //
    // PERMANENT lane, family A only (one extra compile+run, per the Q3
    // ruling). It prices the bonus channel: metadata is a bonus (D2), so
    // stripping it must cost speed and nothing else. A shrinking delta on
    // an LLVM bump is channel decay, visible as a number.
    if k.family == "A" {
        let b = bin("wolf_stripped");
        let ok = Command::new(wolf)
            .args(["build", &s(&dir.join("kernel.lu")), "-o", &s(&b)])
            .arg("--release")
            .arg("--no-cache")
            .env("WOLF_STRIP_FACTS", "1")
            .status()
            .map(|st| st.success())
            .unwrap_or(false);
        if ok {
            out.push(Built {
                lane: "wolf_stripped",
                bin: b,
                ops: k.ops_wolf,
                config: "wolf build --release, WOLF_STRIP_FACTS=1".to_string(),
            });
        }
    }

    // --- the scrutiny lane (report 10 delta 5, UNGATED) --------------
    //
    // Published beside the gated naive-C number so nobody has to
    // re-derive it to embarrass us: clang -O3 -march=native, and PGO'd
    // clang. These are not the gate — the gate is default-flags naive C,
    // which is what people ship — but they are what a sceptical reader
    // will run, so we run them first.
    let nbin = bin("c_native");
    if crate::run_ok(
        "clang",
        &[
            "-O3",
            "-march=native",
            "-o",
            &s(&nbin),
            &s(&dir.join("ref.c")),
        ],
    ) {
        out.push(Built {
            lane: "c_native",
            bin: nbin,
            ops: k.ops,
            config: format!("clang -O3 -march=native ({})", clang_version()),
        });
    }
    if let Some(p) = build_pgo(k, dir, bin_dir) {
        out.push(p);
    }
    Some(out)
}

/// The PGO'd clang lane: instrument, run once on the real workload, merge,
/// rebuild. Skips (returning `None`) when `llvm-profdata` is absent.
fn build_pgo(k: &Kernel, dir: &Path, bin_dir: &Path) -> Option<Built> {
    let s = |p: &Path| p.to_str().expect("utf8 path").to_string();
    let instr = bin_dir.join(format!("{}_pgo_gen", k.name));
    let use_bin = bin_dir.join(format!("{}_c_pgo", k.name));
    let raw = bin_dir.join(format!("{}.profraw", k.name));
    let prof = bin_dir.join(format!("{}.profdata", k.name));
    if !crate::run_ok(
        "clang",
        &[
            "-O3",
            "-fprofile-instr-generate",
            "-o",
            &s(&instr),
            &s(&dir.join("ref.c")),
        ],
    ) {
        return None;
    }
    // A short training run: enough to see the hot loop, not enough to
    // double the suite's wall time.
    let train_ops = (k.ops / 8).max(1);
    let ok = Command::new(&instr)
        .arg(train_ops.to_string())
        .env("LLVM_PROFILE_FILE", &raw)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        return None;
    }
    let Some(profdata) = profdata_binary() else {
        eprintln!("t1: llvm-profdata unavailable — the PGO scrutiny lane is skipped");
        return None;
    };
    if !crate::run_ok(&profdata, &["merge", "-output", &s(&prof), &s(&raw)]) {
        eprintln!(
            "t1: `{profdata} merge` failed (a clang/profdata version mismatch is the usual cause) \
             — the PGO scrutiny lane is skipped"
        );
        return None;
    }
    if !crate::run_ok(
        "clang",
        &[
            "-O3",
            &format!("-fprofile-instr-use={}", s(&prof)),
            "-o",
            &s(&use_bin),
            &s(&dir.join("ref.c")),
        ],
    ) {
        return None;
    }
    Some(Built {
        lane: "c_pgo",
        bin: use_bin,
        ops: k.ops,
        config: format!("clang -O3 + PGO ({})", clang_version()),
    })
}

/// Find a usable `llvm-profdata`: the plain name, then the version-suffixed
/// one matching the clang we build with (distros ship both shapes, and a
/// profdata from a different major refuses the profile).
fn profdata_binary() -> Option<String> {
    let major: String = clang_version()
        .split("clang version ")
        .nth(1)
        .unwrap_or("")
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let mut candidates = vec!["llvm-profdata".to_string()];
    if !major.is_empty() {
        candidates.push(format!("llvm-profdata-{major}"));
    }
    if let Some(v) = std::env::var_os("LLVM_PROFDATA") {
        candidates.insert(0, v.to_string_lossy().into_owned());
    }
    candidates.into_iter().find(|c| {
        Command::new(c)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

fn tool_version(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("{cmd} version unknown"))
}

fn clang_version() -> String {
    tool_version("clang", &["--version"])
}

fn rustc_version() -> String {
    tool_version("rustc", &["--version"])
}

/// Collect a vectorization witness for one source file: the LLVM
/// optimization remarks at the opt level that lane actually uses.
/// Deterministic, so it can gate.
fn vector_witness(args: &[&str]) -> Option<String> {
    let out = Command::new("clang").args(args).output().ok()?;
    Some(String::from_utf8_lossy(&out.stderr).into_owned())
}

/// `bench --track=t1`.
pub fn run(runs: u32, only: Option<&str>, commit: &str) -> Option<Vec<serde_json::Value>> {
    let manifest = std::fs::read_to_string("bench/kernels/manifest.json")
        .map_err(|e| eprintln!("t1: read bench/kernels/manifest.json: {e}"))
        .ok()?;
    let kernels = t1::parse_manifest(&manifest)
        .map_err(|e| eprintln!("t1: {e}"))
        .ok()?;
    if !crate::run_ok(
        "cargo",
        &["build", "-p", "wolf_driver", "-p", "wolf_rt", "--quiet"],
    ) {
        return None;
    }
    let bin_dir = Path::new("target/bench-t1");
    std::fs::create_dir_all(bin_dir).expect("mkdir bench-t1");
    let setarch = setarch_available();
    if !setarch {
        eprintln!("t1: setarch(1) unavailable — the ASLR layout axis is skipped (floor narrows)");
    }
    let perf = crate::perf_available();
    if !perf {
        eprintln!("t1: perf(1) unavailable — instruction counts skipped (wall time only)");
    }

    let mut records: Vec<serde_json::Value> = Vec::new();
    let mut scored: Vec<Scored> = Vec::new();
    let mut summary_lines: Vec<String> = Vec::new();

    for k in &kernels {
        if only.is_some_and(|f| !f.split(',').any(|n| n == k.name)) {
            continue;
        }
        let dir = Path::new("bench/kernels").join(&k.name);
        let Some(lanes) = build_lanes(k, &dir, bin_dir) else {
            eprintln!("t1: {}: SKIPPED (a required lane did not build)", k.name);
            continue;
        };

        // ---- correctness first: same inputs, same outputs -----------
        //
        // A performance number from a lane that computes something else
        // is worse than no number. This check is the reason every kernel
        // reports a `sink`.
        let mut sinks: Vec<(&str, String)> = Vec::new();
        for b in &lanes {
            match sample(b, &LAYOUTS[0], setarch) {
                Some((_, sink)) => sinks.push((b.lane, sink)),
                None => {
                    eprintln!("t1: {}: lane {} would not run", k.name, b.lane);
                    return None;
                }
            }
        }
        let (_, reference) = &sinks[0];
        let mut divergent = Vec::new();
        for (lane, sink) in &sinks[1..] {
            if !t1::sinks_agree(reference, sink, k.sink_float) {
                divergent.push(format!("{lane}={sink}"));
            }
        }
        if !divergent.is_empty() {
            eprintln!(
                "t1: {}: CORRECTNESS DIVERGENCE — wolf={reference}, {}",
                k.name,
                divergent.join(", ")
            );
            return None;
        }

        // ---- paired, interleaved measurement ------------------------
        //
        // A/B/A/B on the same core set, with the lane order ROTATED per
        // run so no lane systematically owns the warm cache or the drift
        // at the end of the sweep.
        let mut samples: BTreeMap<(&str, &str), Vec<f64>> = BTreeMap::new();
        for r in 0..runs {
            for li in 0..lanes.len() {
                let b = &lanes[(li + r as usize) % lanes.len()];
                for l in LAYOUTS {
                    if l.name != "L0" && !FLOOR_LANES.contains(&b.lane) {
                        continue;
                    }
                    if let Some((ns_per_op, _)) = sample(b, l, setarch) {
                        samples.entry((b.lane, l.name)).or_default().push(ns_per_op);
                    }
                }
            }
        }

        // Every sample lands in the report JSON: report 10 delta 5 asks
        // for full per-kernel distributions, not just median+MAD.
        for ((lane, layout), xs) in &samples {
            let config = lanes
                .iter()
                .find(|b| &b.lane == lane)
                .map(|b| b.config.clone())
                .unwrap_or_default();
            for x in xs {
                records.push(rec(
                    k,
                    lane,
                    "ns_per_op",
                    *x,
                    "ns/op",
                    commit,
                    &config,
                    &[("layout", serde_json::json!(layout))],
                ));
            }
        }

        // ---- layout-noise floor -------------------------------------
        let lane_floor = |lane: &str| -> Option<f64> {
            let medians: Vec<f64> = LAYOUTS
                .iter()
                .filter_map(|l| samples.get(&(lane, l.name)))
                .filter_map(|xs| stats::median(xs))
                .collect();
            t1::layout_noise_floor(&medians)
        };
        let mut floors = Vec::new();
        for lane in FLOOR_LANES {
            if let Some(f) = lane_floor(lane) {
                floors.push(f);
                records.push(rec(
                    k,
                    lane,
                    "layout_noise_floor",
                    f,
                    "ratio",
                    commit,
                    "env-size x ASLR perturbation",
                    &[],
                ));
            }
        }
        // The conservative choice: the widest floor either lane showed.
        let floor = floors
            .iter()
            .copied()
            .fold(None::<f64>, |acc, f| Some(acc.map_or(f, |a: f64| a.max(f))));
        if let Some(f) = floor {
            records.push(rec(
                k,
                "suite",
                "layout_noise_floor",
                f,
                "ratio",
                commit,
                "max(wolf, c_naive) — the conservative direction",
                &[],
            ));
        }

        // ---- reported medians, all from L0 --------------------------
        let med = |lane: &str| -> Option<f64> {
            samples.get(&(lane, "L0")).and_then(|xs| stats::median(xs))
        };
        let Some(wolf_med) = med("wolf") else {
            eprintln!("t1: {}: no wolf samples", k.name);
            continue;
        };
        for b in &lanes {
            if let (Some(mine), Some(mad)) = (
                med(b.lane),
                samples.get(&(b.lane, "L0")).and_then(|xs| stats::mad(xs)),
            ) {
                records.push(rec(
                    k,
                    b.lane,
                    "ns_per_op_median",
                    mine,
                    "ns/op",
                    commit,
                    &b.config,
                    &[("mad", serde_json::json!(mad))],
                ));
            }
        }

        // ---- speedups and verdicts ----------------------------------
        for lane in [
            "c_naive",
            "c_expert",
            "rust",
            "c_native",
            "c_pgo",
            "wolf_wrapping",
            "wolf_stripped",
        ] {
            let Some(other) = med(lane) else { continue };
            // A lane that proved the answer instead of computing it is not
            // a comparison, it is a measurement of the constant folder.
            if !t1::lane_ran_the_work(other) {
                eprintln!(
                    "t1: {}: lane {lane} folded the workload ({other:.3e} ns/op) — no comparison \
                     is reported against it",
                    k.name
                );
                records.push(rec(
                    k,
                    lane,
                    "folded_workload",
                    other,
                    "ns/op",
                    commit,
                    "below the plausibility floor: the loop was constant-folded",
                    &[],
                ));
                continue;
            }
            let speedup = other / wolf_med;
            let v = t1::score(wolf_med, other, floor);
            records.push(rec(
                k,
                "wolf",
                &format!("speedup_vs_{lane}"),
                speedup,
                "ratio",
                commit,
                "median(L0) other / median(L0) wolf",
                &[
                    ("verdict", serde_json::json!(v.label())),
                    ("floor", serde_json::json!(floor)),
                ],
            ));
            if lane == "c_naive" {
                scored.push(Scored {
                    kernel: k.name.clone(),
                    family: k.family.clone(),
                    speedup,
                    floor,
                    verdict: v,
                });
                summary_lines.push(format!(
                    "  {:<18} {} {:>7.3}x vs naive C  {:<4} (floor {:>5.2}%)  {}",
                    k.name,
                    k.family,
                    speedup,
                    v.label(),
                    floor.unwrap_or(0.0) * 100.0,
                    k.thesis
                ));
            }
        }
        // X3: the checked-vs-wrapping delta IS the overhead number.
        if let (Some(w), Some(p)) = (med("wolf"), med("wolf_wrapping")) {
            records.push(rec(
                k,
                "wolf",
                "x3_checked_overhead",
                w / p - 1.0,
                "ratio",
                commit,
                "checked / wrapping - 1",
                &[],
            ));
        }
        // The sentinel: what the bonus channel is worth on this kernel.
        if let (Some(w), Some(st)) = (med("wolf"), med("wolf_stripped")) {
            records.push(rec(
                k,
                "wolf",
                "metadata_bonus",
                st / w - 1.0,
                "ratio",
                commit,
                "stripped / annotated - 1",
                &[],
            ));
        }

        // ---- instruction counts (stability, not honesty) ------------
        if perf {
            for b in &lanes {
                if let Some(instr) = crate::perf_instructions(&b.bin, &b.ops.to_string()) {
                    records.push(rec(
                        k,
                        b.lane,
                        "instructions",
                        instr,
                        "count",
                        commit,
                        &b.config,
                        &[],
                    ));
                }
            }
        }

        // ---- vectorization witnesses (deterministic, gating) --------
        if k.witness {
            let dir_s = |f: &str| dir.join(f).to_str().expect("utf8 path").to_string();
            // Naive C at the level it is gated at.
            if let Some(r) = vector_witness(&[
                "-O3",
                "-Rpass=loop-vectorize",
                "-c",
                "-o",
                "/dev/null",
                &dir_s("ref.c"),
            ]) {
                records.push(rec(
                    k,
                    "c_naive",
                    "vectorized_loops",
                    t1::count_vectorized_loops(&r) as f64,
                    "count",
                    commit,
                    "clang -O3 -Rpass=loop-vectorize",
                    &[],
                ));
            }
            // wolf, on the IR the backend actually hands clang, at the
            // -O2 the backend actually uses.
            let ll = bin_dir.join(format!("{}.ll", k.name));
            let ok = Command::new("target/debug/wolf")
                .args([
                    "build",
                    &dir_s("kernel.lu"),
                    "-o",
                    ll.to_str().expect("utf8 path"),
                    "--release",
                    "--emit=llvm-ir",
                    "--no-cache",
                ])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok
                && let Some(r) = vector_witness(&[
                    "-x",
                    "ir",
                    ll.to_str().expect("utf8 path"),
                    "-O2",
                    "-Rpass=loop-vectorize",
                    "-Wno-override-module",
                    "-c",
                    "-o",
                    "/dev/null",
                ])
            {
                records.push(rec(
                    k,
                    "wolf",
                    "vectorized_loops",
                    t1::count_vectorized_loops(&r) as f64,
                    "count",
                    commit,
                    "clang -x ir -O2 -Rpass=loop-vectorize (the backend's own line)",
                    &[],
                ));
            }
        }
    }

    // ---- the gate, printed straight --------------------------------
    let exceptions = std::fs::read_to_string("bench/bench-exceptions.md")
        .map(|md| t1::parse_exceptions(&md))
        .unwrap_or_default();
    let outcome = t1::evaluate_gate(&scored, &exceptions);
    eprintln!("\n== T1 suite: wolf vs naive clang -O3 (layout floor applied)");
    for line in &summary_lines {
        eprintln!("{line}");
    }
    eprintln!("\n== per family (geomean speedup vs naive C; a family-shaped loss must not hide)");
    for (fam, g) in &outcome.by_family {
        eprintln!("  family {fam}: {g:.3}x");
    }
    match outcome.geomean {
        Some(g) => eprintln!("\n  SUITE geomean: {g:.3}x  (M2 primary needs >= 1.000)"),
        None => eprintln!("\n  SUITE geomean: unavailable"),
    }
    if !outcome.undocumented_losses.is_empty() {
        eprintln!(
            "  undocumented losses > 10%: {}",
            outcome.undocumented_losses.join(", ")
        );
    }
    eprintln!(
        "  M2 primary gate: {}",
        if outcome.primary_holds {
            "HOLDS"
        } else {
            "DOES NOT HOLD"
        }
    );

    // The verdict itself is data, so the nightly lane can read it back.
    let suite = serde_json::json!({
        "bench": "suite", "track": "t1", "lang": "wolf",
        "metric": "geomean_speedup_vs_c_naive",
        "value": outcome.geomean.unwrap_or(0.0), "unit": "ratio",
        "commit": commit, "config": "M2 primary", "style": crate::style_version(),
        "primary_holds": outcome.primary_holds,
        "undocumented_losses": outcome.undocumented_losses,
        "exceptions_declared": outcome.exceptions_declared,
    });
    records.push(suite);
    for (fam, g) in &outcome.by_family {
        records.push(serde_json::json!({
            "bench": "suite", "track": "t1", "lang": "wolf",
            "metric": format!("geomean_speedup_vs_c_naive_family_{fam}"),
            "value": g, "unit": "ratio", "commit": commit,
            "config": "M2 primary, per family", "style": crate::style_version(),
        }));
    }
    Some(records)
}

/// `bench --track=irvolume` — issue #70's two metrics, measured.
///
/// 1. LLVM IR instruction count handed to LLVM vs the naive s41 lowering
///    (`WOLF_MIDEND=0`), geomean across the corpus. Budget: <= 0.50.
///    DETERMINISTIC, so this one gates.
/// 2. LLVM's share of Tier-R build wall time. Budget: <= 0.50.
///    Wall-derived, so this one reports.
///
/// The share is measured without instrumenting the driver: `t_total` is a
/// real clean `--release` build, and `t_llvm` is `clang -x ir -c -O2
/// -fPIC` on the exact IR that build hands clang (same file, same flags,
/// same clang — see `wolf_codegen_llvm::LlvmBackend::finish`). For the
/// single-cluster kernels the two are the same invocation, so the ratio is
/// not an estimate.
pub fn irvolume(commit: &str) -> Option<Vec<serde_json::Value>> {
    let manifest = std::fs::read_to_string("bench/kernels/manifest.json").ok()?;
    let kernels = t1::parse_manifest(&manifest)
        .map_err(|e| eprintln!("irvolume: {e}"))
        .ok()?;
    if !crate::run_ok(
        "cargo",
        &["build", "-p", "wolf_driver", "-p", "wolf_rt", "--quiet"],
    ) {
        return None;
    }
    let dir = Path::new("target/bench-ir");
    std::fs::create_dir_all(dir).expect("mkdir bench-ir");
    let wolf = Path::new("target/debug/wolf");
    let mut records = Vec::new();
    let mut ratios: Vec<f64> = Vec::new();
    let mut shares: Vec<f64> = Vec::new();

    let emit = |src: &Path, out: &Path, midend: bool| -> Option<String> {
        let mut cmd = Command::new(wolf);
        cmd.args([
            "build",
            src.to_str().expect("utf8 path"),
            "-o",
            out.to_str().expect("utf8 path"),
        ])
        .arg("--release")
        .arg("--emit=llvm-ir")
        .arg("--no-cache");
        if !midend {
            cmd.env("WOLF_MIDEND", "0");
        }
        let st = cmd.status().ok()?;
        st.success().then(|| std::fs::read_to_string(out).ok())?
    };

    for k in &kernels {
        let src = Path::new("bench/kernels").join(&k.name).join("kernel.lu");
        let on = dir.join(format!("{}-on.ll", k.name));
        let off = dir.join(format!("{}-off.ll", k.name));
        let (Some(ir_on), Some(ir_off)) = (emit(&src, &on, true), emit(&src, &off, false)) else {
            eprintln!("irvolume: {}: could not emit both lowerings", k.name);
            continue;
        };
        let n_on = t1::count_ir_instructions(&ir_on);
        let n_off = t1::count_ir_instructions(&ir_off);
        let Some(ratio) = t1::ir_ratio(n_on, n_off) else {
            continue;
        };
        ratios.push(ratio);
        let mk = |metric: &str, value: f64, unit: &str, config: &str| {
            serde_json::json!({
                "bench": k.name, "track": "irvolume", "lang": "wolf",
                "metric": metric, "value": value, "unit": unit,
                "commit": commit, "config": config,
                "style": crate::style_version(), "family": k.family,
            })
        };
        records.push(mk("llvm_ir_insts", n_on as f64, "count", "s42 mid-end on"));
        records.push(mk(
            "llvm_ir_insts_naive",
            n_off as f64,
            "count",
            "WOLF_MIDEND=0, the naive s41 lowering",
        ));
        records.push(mk(
            "llvm_ir_ratio",
            ratio,
            "ratio",
            "mid-end / naive; #70 metric 1, budget <= 0.50",
        ));

        // LLVM's share of Tier-R build wall time.
        let prog = dir.join(format!("{}-prog", k.name));
        let t0 = Instant::now();
        let ok = Command::new(wolf)
            .args([
                "build",
                src.to_str().expect("utf8 path"),
                "-o",
                prog.to_str().expect("utf8 path"),
            ])
            .arg("--release")
            .arg("--no-cache")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let total = t0.elapsed().as_secs_f64();
        if !ok {
            continue;
        }
        let obj = dir.join(format!("{}-ir.o", k.name));
        let t1i = Instant::now();
        let clang_ok = crate::run_ok(
            "clang",
            &[
                "-x",
                "ir",
                on.to_str().expect("utf8 path"),
                "-c",
                "-O2",
                "-fPIC",
                "-Wno-override-module",
                "-o",
                obj.to_str().expect("utf8 path"),
            ],
        );
        let llvm = t1i.elapsed().as_secs_f64();
        if clang_ok && total > 0.0 {
            let share = llvm / total;
            shares.push(share);
            records.push(mk(
                "release_build_wall_s",
                total,
                "s",
                "clean --release build, no cache",
            ));
            records.push(mk(
                "llvm_wall_s",
                llvm,
                "s",
                "clang -x ir -c -O2 -fPIC on the emitted module",
            ));
            records.push(mk(
                "llvm_wall_share",
                share,
                "ratio",
                "#70 metric 2, budget <= 0.50 (report-only: wall-derived)",
            ));
        }
    }

    // ---- the same ratio over the CORPUS -----------------------------
    //
    // #70 states the budget "geomean across the corpus", and the kernel
    // suite is 13 files chosen for their hot loops — a biased sample of
    // exactly the shapes the mid-end is best at. The corpus sweep is the
    // number the issue actually asks for. Files the release tier refuses
    // (concurrency, `func.addr`) are skipped, not counted as 1.0.
    let mut corpus_ratios: Vec<f64> = Vec::new();
    let mut refused = 0usize;
    for f in crate::corpus_run_entries() {
        let on = dir.join("corpus-on.ll");
        let off = dir.join("corpus-off.ll");
        let (Some(a), Some(b)) = (emit(&f, &on, true), emit(&f, &off, false)) else {
            refused += 1;
            continue;
        };
        if let Some(r) = t1::ir_ratio(t1::count_ir_instructions(&a), t1::count_ir_instructions(&b))
        {
            corpus_ratios.push(r);
        }
    }
    let geo_corpus = t1::geomean(&corpus_ratios);
    if let Some(g) = geo_corpus {
        records.push(serde_json::json!({
            "bench": "corpus", "track": "irvolume", "lang": "wolf",
            "metric": "geomean_llvm_ir_ratio", "value": g, "unit": "ratio",
            "commit": commit,
            "config": format!(
                "#70 metric 1 over {} corpus run-entries ({refused} refused by Tier-R)",
                corpus_ratios.len()
            ),
            "style": crate::style_version(),
        }));
    }

    let geo_ratio = t1::geomean(&ratios);
    let geo_share = t1::geomean(&shares);
    eprintln!("\n== issue #70, measured");
    match geo_corpus {
        Some(g) => eprintln!(
            "  LLVM IR handed to LLVM, CORPUS: {:.1}% of the naive s41 lowering (geomean, n={}, \
             {refused} refused) — budget <= 50.0%: {}",
            g * 100.0,
            corpus_ratios.len(),
            if g <= 0.50 { "MET" } else { "NOT MET" }
        ),
        None => eprintln!("  LLVM IR ratio over the corpus: unavailable"),
    }
    match geo_ratio {
        Some(g) => eprintln!(
            "  LLVM IR handed to LLVM: {:.1}% of the naive s41 lowering (geomean, n={}) — budget <= 50.0%: {}",
            g * 100.0,
            ratios.len(),
            if g <= 0.50 { "MET" } else { "NOT MET" }
        ),
        None => eprintln!("  LLVM IR ratio: unavailable"),
    }
    match geo_share {
        Some(g) => eprintln!(
            "  LLVM's share of Tier-R build wall: {:.1}% (geomean, n={}) — budget <= 50.0%: {}",
            g * 100.0,
            shares.len(),
            if g <= 0.50 { "MET" } else { "NOT MET" }
        ),
        None => eprintln!("  LLVM wall share: unavailable"),
    }
    for (metric, value, config) in [
        (
            "geomean_llvm_ir_ratio",
            geo_ratio,
            "#70 metric 1, budget <= 0.50 (deterministic: GATES)",
        ),
        (
            "geomean_llvm_wall_share",
            geo_share,
            "#70 metric 2, budget <= 0.50 (wall-derived: report-only)",
        ),
    ] {
        if let Some(v) = value {
            records.push(serde_json::json!({
                "bench": "suite", "track": "irvolume", "lang": "wolf",
                "metric": metric, "value": v, "unit": "ratio",
                "commit": commit, "config": config,
                "style": crate::style_version(),
            }));
        }
    }
    Some(records)
}

/// `cargo xtask bench-gates` — the DETERMINISTIC half of s44, and the
/// only part of this sprint that is allowed to fail a PR (D5).
///
/// Two gates, both same-input-same-answer on any machine:
///
/// 1. **IR volume** (#70 metric 1): the geomean ratio of LLVM IR handed to
///    LLVM against the naive s41 lowering, over the kernel suite and over
///    the corpus, ratcheted against `bench/gates.json`. The contract's
///    0.50 target is NOT met today (57.7% corpus / 87.5% kernels), so the
///    gate holds the measured value instead of the aspiration and says so
///    out loud — a gate that is red from birth teaches nobody anything.
/// 2. **Vectorization witnesses** (report 10 delta 2): per-kernel counts of
///    LLVM's `loop-vectorize` remarks, floored per lane. A loop that stops
///    vectorizing fails here before any wall clock moves.
///
/// Skips loudly (exit 0) where the release toolchain is unavailable: a
/// missing clang must not turn into a false red.
pub fn bench_gates() -> ExitCode {
    let Ok(gates_text) = std::fs::read_to_string("bench/gates.json") else {
        eprintln!("bench-gates: bench/gates.json is missing");
        return ExitCode::FAILURE;
    };
    let gates: serde_json::Value = match serde_json::from_str(&gates_text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("bench-gates: bench/gates.json is not JSON: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Ok(manifest) = std::fs::read_to_string("bench/kernels/manifest.json") else {
        eprintln!("bench-gates: bench/kernels/manifest.json is missing");
        return ExitCode::FAILURE;
    };
    let kernels = match t1::parse_manifest(&manifest) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("bench-gates: {e}");
            return ExitCode::FAILURE;
        }
    };
    if !crate::run_ok(
        "cargo",
        &["build", "-p", "wolf_driver", "-p", "wolf_rt", "--quiet"],
    ) {
        return ExitCode::FAILURE;
    }
    if Command::new("clang").arg("--version").output().is_err() {
        eprintln!("bench-gates: clang unavailable — the release tier's gates are skipped");
        return ExitCode::SUCCESS;
    }
    let dir = Path::new("target/bench-gates");
    std::fs::create_dir_all(dir).expect("mkdir bench-gates");
    let wolf = Path::new("target/debug/wolf");
    let mut failures: Vec<String> = Vec::new();

    let emit = |src: &Path, out: &Path, midend: bool| -> Option<String> {
        let mut cmd = Command::new(wolf);
        cmd.args([
            "build",
            src.to_str().expect("utf8 path"),
            "-o",
            out.to_str().expect("utf8 path"),
        ])
        .arg("--release")
        .arg("--emit=llvm-ir")
        .arg("--no-cache");
        if !midend {
            cmd.env("WOLF_MIDEND", "0");
        }
        cmd.status()
            .ok()
            .filter(|s| s.success())
            .and_then(|_| std::fs::read_to_string(out).ok())
    };

    // ---- gate 1: IR volume ------------------------------------------
    let mut kernel_ratios = Vec::new();
    for k in &kernels {
        let src = Path::new("bench/kernels").join(&k.name).join("kernel.lu");
        let on = dir.join("on.ll");
        let off = dir.join("off.ll");
        let (Some(a), Some(b)) = (emit(&src, &on, true), emit(&src, &off, false)) else {
            failures.push(format!(
                "{}: could not emit both lowerings — the IR-volume gate cannot be evaluated",
                k.name
            ));
            continue;
        };
        if let Some(r) = t1::ir_ratio(t1::count_ir_instructions(&a), t1::count_ir_instructions(&b))
        {
            kernel_ratios.push(r);
        }
    }
    let mut corpus_ratios = Vec::new();
    for f in crate::corpus_run_entries() {
        let on = dir.join("c-on.ll");
        let off = dir.join("c-off.ll");
        let (Some(a), Some(b)) = (emit(&f, &on, true), emit(&f, &off, false)) else {
            continue; // Tier-R refusals are conservatism, not regressions
        };
        if let Some(r) = t1::ir_ratio(t1::count_ir_instructions(&a), t1::count_ir_instructions(&b))
        {
            corpus_ratios.push(r);
        }
    }
    let iv = &gates["ir_volume"];
    for (label, ratios, key) in [
        ("kernel suite", &kernel_ratios, "kernel_geomean_ceiling"),
        ("corpus", &corpus_ratios, "corpus_geomean_ceiling"),
    ] {
        let Some(g) = t1::geomean(ratios) else {
            failures.push(format!("ir-volume: no {label} ratios were measured"));
            continue;
        };
        let Some(ceiling) = iv[key].as_f64() else {
            failures.push(format!("bench/gates.json: ir_volume.{key} is missing"));
            continue;
        };
        let target = iv["contract_target"].as_f64().unwrap_or(0.50);
        eprintln!(
            "bench-gates: IR volume, {label}: {:.1}% of naive (n={}) — ratchet {:.1}%, \
             contract target {:.1}% ({})",
            g * 100.0,
            ratios.len(),
            ceiling * 100.0,
            target * 100.0,
            if g <= target {
                "MET"
            } else {
                "NOT MET, tracked"
            }
        );
        if g > ceiling {
            failures.push(format!(
                "ir-volume: {label} geomean {:.3} exceeds the ratchet {ceiling:.3} — the mid-end \
                 is handing LLVM MORE than it used to",
                g
            ));
        }
    }

    // ---- gate 2: vectorization witnesses ----------------------------
    let witnesses = &gates["vectorization_witnesses"]["kernels"];
    for k in kernels.iter().filter(|k| k.witness) {
        let kdir = Path::new("bench/kernels").join(&k.name);
        let expect = &witnesses[&k.name];
        if expect.is_null() {
            failures.push(format!(
                "{}: declares `witness` but bench/gates.json records no floor for it",
                k.name
            ));
            continue;
        }
        let cs = kdir.join("ref.c");
        if let Some(r) = vector_witness(&[
            "-O3",
            "-Rpass=loop-vectorize",
            "-c",
            "-o",
            "/dev/null",
            cs.to_str().expect("utf8 path"),
        ]) {
            let got = t1::count_vectorized_loops(&r);
            let min = expect["c_naive_min"].as_u64().unwrap_or(0) as usize;
            if got < min {
                failures.push(format!(
                    "{}: naive C vectorized {got} loop(s), floor is {min} — the comparison lane \
                     changed under us (a clang bump?), so the recorded floors are stale",
                    k.name
                ));
            }
        }
        let ll = dir.join(format!("{}.ll", k.name));
        if emit(&kdir.join("kernel.lu"), &ll, true).is_some()
            && let Some(r) = vector_witness(&[
                "-x",
                "ir",
                ll.to_str().expect("utf8 path"),
                "-O2",
                "-Rpass=loop-vectorize",
                "-Wno-override-module",
                "-c",
                "-o",
                "/dev/null",
            ])
        {
            let got = t1::count_vectorized_loops(&r);
            let min = expect["wolf_min"].as_u64().unwrap_or(0) as usize;
            eprintln!(
                "bench-gates: witness {:<18} wolf={got} (floor {min})",
                k.name
            );
            if got < min {
                failures.push(format!(
                    "{}: wolf vectorized {got} loop(s), floor is {min} — a loop that used to \
                     vectorize stopped",
                    k.name
                ));
            }
        }
    }

    if failures.is_empty() {
        eprintln!("bench-gates: deterministic bench gates green");
        ExitCode::SUCCESS
    } else {
        for f in &failures {
            eprintln!("bench-gates: {f}");
        }
        ExitCode::FAILURE
    }
}

/// `bench gate <t1.jsonl>` — re-derive the M2 verdict from stored data.
///
/// The nightly lane runs this against the JSONL it just committed, so the
/// declaration ("M2 holds on three consecutive nightly runs") is made from
/// the published numbers and not from a transient in-memory state. Exit 1
/// when the primary gate does not hold.
pub fn gate(path: &str) -> ExitCode {
    let Ok(body) = std::fs::read_to_string(path) else {
        eprintln!("bench gate: cannot read {path}");
        return ExitCode::from(2);
    };
    let mut scored: Vec<Scored> = Vec::new();
    let mut floors: BTreeMap<String, f64> = BTreeMap::new();
    let mut rows: Vec<(String, String, f64, String)> = Vec::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            eprintln!("bench gate: {path}: malformed JSONL record");
            return ExitCode::from(2);
        };
        let bench = v["bench"].as_str().unwrap_or("");
        let metric = v["metric"].as_str().unwrap_or("");
        if metric == "layout_noise_floor" && v["lang"].as_str() == Some("suite") {
            if let Some(f) = v["value"].as_f64() {
                floors.insert(bench.to_string(), f);
            }
            continue;
        }
        if metric == "speedup_vs_c_naive"
            && let Some(sp) = v["value"].as_f64()
        {
            rows.push((
                bench.to_string(),
                v["family"].as_str().unwrap_or("?").to_string(),
                sp,
                v["verdict"].as_str().unwrap_or("").to_string(),
            ));
        }
    }
    if rows.is_empty() {
        eprintln!("bench gate: {path} carries no speedup_vs_c_naive records");
        return ExitCode::from(2);
    }
    for (kernel, family, speedup, stored) in rows {
        let floor = floors.get(&kernel).copied();
        // Re-derive rather than trust the stored label: the gate must be
        // reproducible from the raw ratio and the raw floor.
        let verdict = t1::score(1.0, speedup, floor);
        if !stored.is_empty() && stored != verdict.label() {
            eprintln!(
                "bench gate: {kernel}: stored verdict {stored} disagrees with re-derived {} \
                 — the report JSON and the decision procedure have diverged",
                verdict.label()
            );
            return ExitCode::from(2);
        }
        scored.push(Scored {
            kernel,
            family,
            speedup,
            floor,
            verdict,
        });
    }
    let exceptions = std::fs::read_to_string("bench/bench-exceptions.md")
        .map(|md| t1::parse_exceptions(&md))
        .unwrap_or_default();
    let outcome = t1::evaluate_gate(&scored, &exceptions);
    eprintln!("== M2 gate over {path}");
    for s in &scored {
        eprintln!(
            "  {:<18} {} {:>7.3}x {:<4}{}",
            s.kernel,
            s.family,
            s.speedup,
            s.verdict.label(),
            match s.verdict {
                Verdict::Loss { pct } if pct > 10.0 => {
                    if outcome.excepted_losses.contains(&s.kernel) {
                        "  (documented exception)"
                    } else {
                        "  (UNDOCUMENTED >10% loss)"
                    }
                }
                _ => "",
            }
        );
    }
    for (fam, g) in &outcome.by_family {
        eprintln!("  family {fam}: {g:.3}x");
    }
    match outcome.geomean {
        Some(g) => eprintln!("  suite geomean: {g:.3}x (needs >= 1.000)"),
        None => eprintln!("  suite geomean: unavailable"),
    }
    eprintln!(
        "  exceptions declared: {} (cap 3)",
        outcome.exceptions_declared
    );
    if outcome.primary_holds {
        eprintln!("  M2 primary gate: HOLDS");
        ExitCode::SUCCESS
    } else {
        eprintln!("  M2 primary gate: DOES NOT HOLD — M2 is not declared");
        ExitCode::FAILURE
    }
}
