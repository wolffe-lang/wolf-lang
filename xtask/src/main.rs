//! Repo automation (`cargo xtask <command>`). CI-shaped: pure, exit-code
//! driven — the s02 CI workflows are thin wrappers over these commands.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

use xtask::corpus::{self, Directives};
use xtask::stats;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("ci") => ci(),
        Some("deps-check") => deps_check(),
        Some("corpus") => corpus_cmd(),
        Some("abi-check") => abi_check(),
        Some("debug-check") => debug_check(),
        Some("bench") => bench_cmd(&args[1..]),
        Some("fuzz-smoke") => fuzz_smoke(),
        Some("dist") => dist(),
        Some("spec-extract") => spec_extract(args.iter().any(|a| a == "--check")),
        Some("conformance") => conformance_cmd(&args[1..]),
        Some("differ") => differ_cmd(&args[1..]),
        Some("print-gate") => print_gate(),
        Some("diag-catalog") => diag_catalog(args.iter().any(|a| a == "--check")),
        Some("fmt-lu") => fmt_lu(),
        Some("audit-surface") => audit_surface(),
        _ => {
            eprintln!(
                "usage: cargo xtask <ci|deps-check|corpus|bench|fuzz-smoke|dist|spec-extract|conformance|differ|print-gate|diag-catalog|fmt-lu|audit-surface>"
            );
            eprintln!("       cargo xtask bench --track=<runtime|compile> [--runs=N] [--out=FILE]");
            eprintln!("       cargo xtask bench diff <baseline.jsonl> <candidate.jsonl> [--gate]");
            ExitCode::from(2)
        }
    }
}

/// fmt-check + clippy (deny warnings) + tests + graph law + corpus.
fn ci() -> ExitCode {
    let steps: &[(&str, &[&str])] = &[
        ("fmt", &["fmt", "--all", "--check"]),
        (
            "clippy",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        ("test", &["test", "--workspace"]),
        ("deps-check", &["xtask", "deps-check"]),
        ("corpus", &["xtask", "corpus"]),
        ("abi-check", &["xtask", "abi-check"]),
        ("debug-check", &["xtask", "debug-check"]),
        ("spec-extract", &["xtask", "spec-extract", "--check"]),
        ("conformance", &["xtask", "conformance"]),
        ("print-gate", &["xtask", "print-gate"]),
        ("diag-catalog", &["xtask", "diag-catalog", "--check"]),
        ("fmt-lu", &["xtask", "fmt-lu"]),
        ("audit-surface", &["xtask", "audit-surface"]),
        ("differ-self", &["xtask", "differ", "--self"]),
    ];
    for (name, args) in steps {
        eprintln!("== xtask ci: {name}");
        let ok = Command::new("cargo")
            .args(*args)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("xtask ci: step `{name}` failed");
            return ExitCode::FAILURE;
        }
    }
    eprintln!("xtask ci: all steps green");
    ExitCode::SUCCESS
}

// ------------------------------------------------------------- abi-check --

/// `xtask abi-check` — the s29 differential ABI gate (s49's embryo):
/// the fixed signature table compiled by the wolf backend AND the host
/// C compiler, round-tripped and bit-asserted. A named PR-CI step so
/// an ABI regression fails loudly under its own name, not somewhere
/// inside the workspace test wall.
fn abi_check() -> ExitCode {
    let ok = run_ok(
        "cargo",
        &[
            "test",
            "-p",
            "wolf_codegen_clif",
            "--test",
            "abi_check",
            "--quiet",
        ],
    );
    if ok {
        eprintln!("abi-check: signature table green against host cc");
        ExitCode::SUCCESS
    } else {
        eprintln!("abi-check: ABI divergence against host cc");
        ExitCode::FAILURE
    }
}

// -------------------------------------------------------------- debug-check --

/// `xtask debug-check` — the s30/s31 debuggability gate as a NAMED
/// PR-CI step (same rationale as `abi-check`): wolf's own DWARF in the
/// binary, the gdb transcript over the stepping fixture AND
/// `corpus/hello.lu`, and the issue-#26 mangling end-to-end. The tests
/// skip loudly where gdb or a native toolchain is absent — the linux
/// CI lane installs both, so a silent skip there is a lane bug, not a
/// green.
fn debug_check() -> ExitCode {
    let ok = run_ok(
        "cargo",
        &[
            "test",
            "-p",
            "wolf_driver",
            "--test",
            "debug_native",
            "--quiet",
        ],
    );
    if ok {
        eprintln!("debug-check: debug sections + gdb transcripts green");
        ExitCode::SUCCESS
    } else {
        eprintln!("debug-check: debuggability regression");
        ExitCode::FAILURE
    }
}

// ---------------------------------------------------------------- corpus --

/// Validate every corpus file's directive header. Phase *execution* is
/// stubbed until the compiler grows phases (s31); headers must parse and
/// use canonical phases today so the corpus stays a truthful ledger.
fn corpus_cmd() -> ExitCode {
    let mut files = Vec::new();
    collect_wolf_files(Path::new("corpus"), &mut files);
    files.sort();
    let mut bad = 0u32;
    let mut parsed: Vec<(PathBuf, Directives)> = Vec::new();
    for f in &files {
        let src = match std::fs::read_to_string(f) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("corpus: {}: unreadable: {e}", f.display());
                bad += 1;
                continue;
            }
        };
        match corpus::parse_directives(&src) {
            Ok(d) => {
                if d.member {
                    // compiled through its module's entry file (s12)
                } else if d.phase.is_none() {
                    eprintln!("corpus: {}: missing `//! phase:` directive", f.display());
                    bad += 1;
                } else {
                    parsed.push((f.clone(), d));
                }
            }
            Err(e) => {
                eprintln!("corpus: {}: {e}", f.display());
                bad += 1;
            }
        }
    }
    for (f, d) in &parsed {
        let phase = d.phase.as_deref().unwrap_or("?");
        eprintln!(
            "corpus: {} [phase: {phase}] {}",
            f.display(),
            if d.conforms.is_empty() {
                String::new()
            } else {
                format!("conforms: {}", d.conforms.join(", "))
            }
        );
    }
    // Phase execution + ledger enforcement (s01 rule, live since s07):
    // the declared phase must equal the deepest phase that succeeds today.
    // Runs through the differential protocol so xtask stays independent of
    // compiler crates. A stub driver (verdict `unsupported`) skips checks.
    let mut executed = false;
    if run_ok("cargo", &["build", "-p", "wolf_driver", "--quiet"]) {
        for (f, d) in &parsed {
            // `phase: run` files execute NATIVELY (s31: compile, link,
            // run — the M1 gate). WOLF_NATIVE=1 keeps the argv shape
            // protocol-stable.
            let native = d.phase.as_deref() == Some("run");
            let mut cmd = Command::new("target/debug/wolf");
            cmd.arg("conform-run").arg(f).arg("--json");
            if native {
                cmd.env("WOLF_NATIVE", "1");
            }
            let out = cmd.output();
            let Ok(out) = out else { continue };
            if !out.status.success() {
                eprintln!("corpus: {}: conform-run failed", f.display());
                eprintln!("  stderr: {}", String::from_utf8_lossy(&out.stderr).trim());
                bad += 1;
                continue;
            }
            let Ok(rec) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
                eprintln!("corpus: {}: bad observation record", f.display());
                bad += 1;
                continue;
            };
            let verdict = rec["verdict"].as_str().unwrap_or("");
            let reached = rec["phase_reached"].as_str().unwrap_or("none");
            if verdict == "unsupported" && reached == "none" {
                continue; // stub driver: no phase engine at all
            }
            // `unsupported` with a real phase_reached still carries phase
            // evidence: everything through phase_reached completed clean.
            executed = true;
            let reached_rank = corpus::phase_rank(reached).unwrap_or(0);
            // deepest phase that SUCCEEDS: reached on pass/run-verdicts,
            // one before reached on fail (reached = the phase that failed)
            let deepest_pass = if verdict.starts_with("fail(") {
                reached_rank.saturating_sub(1)
            } else {
                reached_rank
            };
            let declared = d.phase.as_deref().and_then(corpus::phase_rank).unwrap_or(0);
            if declared != deepest_pass {
                eprintln!(
                    "corpus: {}: header claims phase `{}` but deepest passing phase is `{}` — advance/retreat `//! phase:` deliberately",
                    f.display(),
                    d.phase.as_deref().unwrap_or("none"),
                    corpus::PHASES[deepest_pass],
                );
                bad += 1;
            }
            // Expected-code matching (s10): a `check: fail(CODE)` file whose
            // failure is observable today must fail with exactly that code.
            if let Some(corpus::Check::Fail(expected)) = &d.check
                && let Some(actual) = verdict
                    .strip_prefix("fail(")
                    .and_then(|v| v.strip_suffix(')'))
                && actual != expected
            {
                eprintln!(
                    "corpus: {}: expected fail({expected}) but got fail({actual})",
                    f.display()
                );
                bad += 1;
            }
            // `check: run(...)` assertion (s31 — the M1 gate): a file
            // that reaches the run rung must exit/trap exactly as the
            // directive says, and its stdout must match with the
            // trailing newline ignored (`print` appends one;
            // [conf.directive.check]).
            if let Some(corpus::Check::Run(exp)) = &d.check
                && (verdict.starts_with("exit(") || verdict.starts_with("trap("))
            {
                let exit_ok = match &exp.exit {
                    corpus::ExitExpect::Code(n) => verdict == format!("exit({n})"),
                    corpus::ExitExpect::Trap(None) => verdict.starts_with("trap("),
                    corpus::ExitExpect::Trap(Some(kind)) => verdict == format!("trap({kind})"),
                };
                if !exit_ok {
                    eprintln!(
                        "corpus: {}: expected run({:?}) but the program's verdict is {verdict}",
                        f.display(),
                        exp.exit,
                    );
                    bad += 1;
                }
                if let Some(want) = &exp.stdout {
                    let got = rec["stdout_inline"].as_str().unwrap_or("");
                    let trimmed = got.strip_suffix('\n').unwrap_or(got);
                    if trimmed != want && got != want {
                        eprintln!(
                            "corpus: {}: expected stdout {want:?} but got {got:?}",
                            f.display()
                        );
                        bad += 1;
                    }
                }
            }
        }
    }
    eprintln!(
        "corpus: {} file(s), {} bad{}",
        files.len(),
        bad,
        if executed {
            " — phase ledger enforced via conform-run"
        } else {
            " — phase execution pending a non-stub driver"
        }
    );
    if bad > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn collect_wolf_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_wolf_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "lu") {
            out.push(p);
        }
    }
}

// ----------------------------------------------------------------- bench --

/// Reference kernels: name -> self-timed ops per invocation. Values sized
/// for ~100ms per run. The wolf slot activates at M1 (s31/s44).
const KERNELS: &[(&str, u64)] = &[
    ("alias_daxpy", 20_000),
    ("list_alloc", 400),
    ("aos_dot", 1_500),
    ("word_count", 60),
];

fn bench_cmd(args: &[String]) -> ExitCode {
    if args.first().map(String::as_str) == Some("diff") {
        return bench_diff(&args[1..]);
    }
    let mut track = None;
    let mut runs: u32 = 10;
    let mut out_path: Option<PathBuf> = None;
    for a in args {
        if let Some(v) = a.strip_prefix("--track=") {
            track = Some(v.to_string());
        } else if let Some(v) = a.strip_prefix("--runs=") {
            runs = v.parse().expect("--runs=N");
        } else if let Some(v) = a.strip_prefix("--out=") {
            out_path = Some(PathBuf::from(v));
        } else {
            eprintln!("bench: unknown argument `{a}`");
            return ExitCode::from(2);
        }
    }
    let commit = git_short_sha();
    let records = match track.as_deref() {
        Some("runtime") => bench_runtime(runs, &commit),
        Some("compile") => bench_compile(runs.min(3), &commit),
        _ => {
            eprintln!("bench: --track=<runtime|compile> is required");
            return ExitCode::from(2);
        }
    };
    let Some(records) = records else {
        return ExitCode::FAILURE;
    };
    let out_path = out_path.unwrap_or_else(|| {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock before epoch")
            .as_secs();
        PathBuf::from(format!(
            "bench-results/{}-{commit}-{t}.jsonl",
            track.expect("track set above")
        ))
    });
    let mut body = String::new();
    for r in &records {
        body.push_str(&r.to_string());
        body.push('\n');
    }
    if let Some(parent) = out_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).expect("mkdir bench output dir");
    }
    std::fs::write(&out_path, body).expect("write bench results");
    eprintln!(
        "bench: {} record(s) -> {}",
        records.len(),
        out_path.display()
    );
    ExitCode::SUCCESS
}

fn bench_runtime(runs: u32, commit: &str) -> Option<Vec<serde_json::Value>> {
    let bin_dir = Path::new("target/bench-bin");
    std::fs::create_dir_all(bin_dir).expect("mkdir bench-bin");
    let perf = perf_available();
    if !perf {
        eprintln!("bench: perf(1) not available — wall-time only (instruction counts skipped)");
    }
    let mut records = Vec::new();
    for (kernel, ops) in KERNELS {
        let dir = Path::new("bench/kernels").join(kernel);
        for (lang, config) in [("c", "cc -O3"), ("rust", "rustc -O")] {
            let bin = bin_dir.join(format!("{kernel}_{lang}"));
            let built = match lang {
                "c" => run_ok(
                    "cc",
                    &[
                        "-O3",
                        "-o",
                        bin.to_str().expect("utf8 path"),
                        dir.join("ref.c").to_str().expect("utf8 path"),
                    ],
                ),
                _ => run_ok(
                    "rustc",
                    &[
                        "-O",
                        "--edition=2024",
                        "-o",
                        bin.to_str().expect("utf8 path"),
                        dir.join("ref.rs").to_str().expect("utf8 path"),
                    ],
                ),
            };
            if !built {
                eprintln!("bench: failed to build {kernel}/{lang}");
                return None;
            }
            for _ in 0..runs {
                let out = Command::new(&bin)
                    .arg(ops.to_string())
                    .output()
                    .expect("run kernel");
                if !out.status.success() {
                    eprintln!("bench: {kernel}/{lang} exited nonzero");
                    return None;
                }
                let v: serde_json::Value =
                    serde_json::from_slice(&out.stdout).expect("kernel self-report json");
                let ns = v["ns"].as_f64().expect("ns");
                let n = v["ops"].as_f64().expect("ops");
                records.push(record(
                    kernel,
                    "runtime",
                    lang,
                    "ns_per_op",
                    ns / n,
                    "ns/op",
                    commit,
                    config,
                ));
            }
            if perf && let Some(instr) = perf_instructions(&bin, &ops.to_string()) {
                records.push(record(
                    kernel,
                    "runtime",
                    lang,
                    "instructions",
                    instr,
                    "count",
                    commit,
                    config,
                ));
            }
        }
    }
    Some(records)
}

/// Compile-track metrics over the REAL pipeline (s31, D5): `wolf
/// build` on a generated N-module project — clean-build wall time and
/// the **incremental single-function-edit rebuild latency** through
/// `.lu-cache` (the M3 headline metric's first honest numbers; they
/// are module-grain and deliberately ugly — c12's before/after story
/// baselines here). The sema/CTFE/WIR example micro-metrics ride
/// along unchanged.
fn bench_compile(runs: u32, commit: &str) -> Option<Vec<serde_json::Value>> {
    let mut records = Vec::new();
    let config = "wolf-batch-v0";
    if !run_ok(
        "cargo",
        &["build", "-p", "wolf_driver", "-p", "wolf_rt", "--quiet"],
    ) {
        return None;
    }
    // The generated project: a chain of NMOD modules (m01 → m02 → … →
    // leaf), each with one `pub fn`; main sums through the chain.
    const NMOD: usize = 12;
    let proj = Path::new("target/bench-nmod");
    let _ = std::fs::remove_dir_all(proj);
    std::fs::create_dir_all(proj).expect("mkdir bench project");
    let leaf = NMOD;
    for k in 1..=NMOD {
        let dir = proj.join(format!("m{k:02}"));
        std::fs::create_dir_all(&dir).expect("mkdir module");
        let body = if k == leaf {
            format!("//! member: true\n\npub fn f{k:02}(x: int) -> int {{\n    x + {k}\n}}\n")
        } else {
            let next = k + 1;
            format!(
                "//! member: true\nuse m{next:02}\n\npub fn f{k:02}(x: int) -> int {{\n    m{next:02}.f{next:02}(x) + {k}\n}}\n"
            )
        };
        std::fs::write(dir.join(format!("m{k:02}.lu")), body).expect("write module");
    }
    std::fs::write(
        proj.join("main.lu"),
        "use m01\n\nfn main() -> !int {\n    if m01.f01(0) > 0 { 0 } else { 1 }\n}\n",
    )
    .expect("write main");
    let wolf = Path::new("target/debug/wolf");
    let entry = proj.join("main.lu");
    let prog = proj.join("prog");
    let wolf_build = |extra: &[&str]| -> bool {
        let mut cmd = Command::new(wolf);
        cmd.arg("build").arg(&entry).arg("-o").arg(&prog);
        cmd.args(extra);
        cmd.status().map(|s| s.success()).unwrap_or(false)
    };
    // One probe: hosts that cannot link natively (non-linux-x86-64,
    // no cc) skip the wolf metrics but keep the micro-metrics.
    if wolf_build(&[]) {
        let mut flip = false;
        for _ in 0..runs {
            // (a) clean build: no cache at all.
            let _ = std::fs::remove_dir_all(proj.join(".lu-cache"));
            let t = Instant::now();
            if !wolf_build(&[]) {
                return None;
            }
            records.push(record(
                "nmod",
                "compile",
                "wolf",
                "clean_build_wall_s",
                t.elapsed().as_secs_f64(),
                "s",
                commit,
                config,
            ));
            // (b) the M3 headline: touch ONE function body in the
            // leaf module, rebuild through the cache.
            flip = !flip;
            let expr = if flip {
                format!("{leaf} + x")
            } else {
                format!("x + {leaf}")
            };
            std::fs::write(
                proj.join(format!("m{leaf:02}"))
                    .join(format!("m{leaf:02}.lu")),
                format!(
                    "//! member: true\n\npub fn f{leaf:02}(x: int) -> int {{\n    {expr}\n}}\n"
                ),
            )
            .expect("edit leaf");
            let t = Instant::now();
            if !wolf_build(&[]) {
                return None;
            }
            records.push(record(
                "nmod",
                "compile",
                "wolf",
                "incr_rebuild_wall_s",
                t.elapsed().as_secs_f64(),
                "s",
                commit,
                config,
            ));
        }
        // (c) max-RSS of one incremental rebuild, via /usr/bin/time -v.
        touch(&proj.join("m01").join("m01.lu"));
        match max_rss_kb(
            "target/debug/wolf",
            &[
                "build",
                entry.to_str().expect("utf8 path"),
                "-o",
                prog.to_str().expect("utf8 path"),
            ],
        ) {
            Some(kb) => records.push(record(
                "nmod",
                "compile",
                "wolf",
                "max_rss_kb",
                kb,
                "kB",
                commit,
                config,
            )),
            None => eprintln!("bench: /usr/bin/time unavailable — max_rss_kb skipped"),
        }
    } else {
        eprintln!("bench: native pipeline unavailable on this host — wolf compile metrics skipped");
    }
    // (c') checker throughput (s13, D5): bodies-checked-per-second over
    // the corpus, from wolf_sema's bench example. Skips gracefully until
    // the example exists.
    let bb = Command::new("cargo")
        .args([
            "run",
            "-p",
            "wolf_sema",
            "--example",
            "bodies_bench",
            "--quiet",
        ])
        .output();
    match bb {
        Ok(out) if out.status.success() => {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout)
                && let Some(bps) = v["bodies_per_sec"].as_f64()
            {
                records.push(record(
                    "sema",
                    "compile",
                    "rust",
                    "bodies_per_sec",
                    bps,
                    "bodies/s",
                    commit,
                    config,
                ));
            }
        }
        _ => eprintln!("bench: wolf_sema bodies_bench unavailable — bodies/sec skipped"),
    }
    // (c'') comptime engine metrics (s16, D5) — skips until the example exists.
    let cb = Command::new("cargo")
        .args([
            "run",
            "-p",
            "wolf_sema",
            "--example",
            "ctfe_bench",
            "--quiet",
        ])
        .output();
    match cb {
        Ok(out) if out.status.success() => {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout)
                && let Some(rate) = v["ctfe_memo_hit_rate"].as_f64()
            {
                records.push(record(
                    "sema",
                    "compile",
                    "rust",
                    "ctfe_memo_hit_rate",
                    rate,
                    "ratio",
                    commit,
                    config,
                ));
            }
        }
        _ => eprintln!("bench: wolf_sema ctfe_bench unavailable — memo rate skipped"),
    }
    // (c''') WIR construction metrics (s25, D5): instructions built
    // per second plus the peephole hit-rate counters (fold/identity/
    // gvn/forward) — the Click §5 claim, measured. Skips gracefully
    // until the example exists.
    let wb = Command::new("cargo")
        .args([
            "run",
            "-p",
            "wolf_wir",
            "--example",
            "wir_build_bench",
            "--quiet",
        ])
        .output();
    match wb {
        Ok(out) if out.status.success() => {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                for (metric, unit) in [
                    ("wir_insts_per_sec", "insts/s"),
                    ("wir_fold_hits", "hits"),
                    ("wir_identity_hits", "hits"),
                    ("wir_gvn_hits", "hits"),
                    ("wir_forward_hits", "hits"),
                ] {
                    if let Some(x) = v[metric].as_f64() {
                        records.push(record(
                            "wir", "compile", "rust", metric, x, unit, commit, config,
                        ));
                    }
                }
            }
        }
        _ => eprintln!("bench: wolf_wir wir_build_bench unavailable — wir-build skipped"),
    }
    Some(records)
}

fn bench_diff(args: &[String]) -> ExitCode {
    let gate = args.iter().any(|a| a == "--gate");
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let [base, cand] = files.as_slice() else {
        eprintln!("bench diff: need <baseline.jsonl> <candidate.jsonl>");
        return ExitCode::from(2);
    };
    let group = |path: &str| -> BTreeMap<String, Vec<f64>> {
        let mut m: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let body = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("bench diff: read {path}: {e}"));
        for line in body.lines().filter(|l| !l.trim().is_empty()) {
            let v: serde_json::Value = serde_json::from_str(line).expect("jsonl record");
            let key = format!(
                "{}/{}/{}/{}",
                v["bench"].as_str().expect("bench"),
                v["track"].as_str().expect("track"),
                v["lang"].as_str().expect("lang"),
                v["metric"].as_str().expect("metric"),
            );
            m.entry(key)
                .or_default()
                .push(v["value"].as_f64().expect("value"));
        }
        m
    };
    let b = group(base);
    let c = group(cand);
    let mut regressions = 0u32;
    for (key, bvals) in &b {
        let Some(cvals) = c.get(key) else {
            eprintln!("  {key}: missing from candidate");
            continue;
        };
        match stats::compare(bvals, cvals) {
            Some(stats::Verdict::Unchanged) => eprintln!("  {key}: unchanged"),
            Some(stats::Verdict::Significant { delta_pct }) => {
                // Metric polarity (s31 fix): wall times, RSS, ns/op
                // and instruction counts are lower-is-better;
                // throughput (`*_per_sec`) and hit-rate metrics are
                // bigger-is-better — an improvement there must never
                // trip the gate.
                let metric = key.rsplit('/').next().unwrap_or("");
                let lower_is_better = !(metric.ends_with("per_sec")
                    || metric.ends_with("hit_rate")
                    || metric.ends_with("_hits"));
                let regressed = if lower_is_better {
                    delta_pct > 0.0
                } else {
                    delta_pct < 0.0
                };
                let dir = if regressed { "REGRESSED" } else { "improved" };
                eprintln!("  {key}: {dir} {delta_pct:+.1}%");
                if regressed {
                    regressions += 1;
                }
            }
            None => eprintln!("  {key}: empty sample"),
        }
    }
    if gate && regressions > 0 {
        eprintln!("bench diff: {regressions} significant regression(s) — gate failed");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[allow(clippy::too_many_arguments)]
fn record(
    bench: &str,
    track: &str,
    lang: &str,
    metric: &str,
    value: f64,
    unit: &str,
    commit: &str,
    config: &str,
) -> serde_json::Value {
    serde_json::json!({
        "bench": bench, "track": track, "lang": lang, "metric": metric,
        "value": value, "unit": unit, "commit": commit, "config": config,
        "style": style_version(),
    })
}

// ------------------------------------------------------------------ fuzz --

/// Build the fuzz scaffold if cargo-fuzz (nightly) is available; the
/// scaffold itself always exists so adding a target is a five-line diff.
fn fuzz_smoke() -> ExitCode {
    let available = Command::new("cargo")
        .args(["fuzz", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !available {
        eprintln!("fuzz-smoke: cargo-fuzz not installed — scaffold present, smoke skipped");
        eprintln!(
            "fuzz-smoke: CI runs this with nightly (s02); install with `cargo install cargo-fuzz`"
        );
        return ExitCode::SUCCESS;
    }
    if run_ok("cargo", &["fuzz", "build", "--fuzz-dir", "fuzz"]) {
        eprintln!("fuzz-smoke: all targets build");
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// ---------------------------------------------------------- spec-extract --

/// Extract the normative EBNF from spec/01-grammar.md into
/// spec/grammar.ebnf; `--check` verifies sync instead of writing (CI).
fn spec_extract(check: bool) -> ExitCode {
    // link pass first: dangling cross-references fail regardless of mode
    let names = [
        "01-grammar.md",
        "02-memory-model.md",
        "03-concurrency.md",
        "04-abi.md",
        "05-conformance.md",
        "06-differential-protocol.md",
    ];
    let bodies: Vec<(String, String)> = names
        .iter()
        .filter_map(|n| {
            std::fs::read_to_string(Path::new("spec").join(n))
                .ok()
                .map(|b| (n.to_string(), b))
        })
        .collect();
    let docs: Vec<(&str, &str)> = bodies
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_str()))
        .collect();
    let link_errors = xtask::spec::link_check(&docs);
    if !link_errors.is_empty() {
        for e in &link_errors {
            eprintln!("spec-extract: {e}");
        }
        return ExitCode::FAILURE;
    }
    // anchors.json — the machine-readable clause registry [conf.anchor.index]
    let index = xtask::spec::anchor_index(&docs);
    let anchors_json = serde_json::to_string_pretty(&serde_json::json!({
        "version": 1,
        "anchors": index,
    }))
    .expect("serialize anchors")
        + "\n";
    let anchors_path = Path::new("spec/anchors.json");
    if check {
        let on_disk = std::fs::read_to_string(anchors_path).unwrap_or_default();
        if on_disk != anchors_json {
            eprintln!(
                "spec-extract: spec/anchors.json OUT OF SYNC — run `cargo xtask spec-extract`"
            );
            return ExitCode::FAILURE;
        }
    } else {
        std::fs::write(anchors_path, &anchors_json).expect("write anchors.json");
        eprintln!(
            "spec-extract: wrote spec/anchors.json ({} anchors)",
            index.len()
        );
    }
    let md = std::fs::read_to_string("spec/01-grammar.md").expect("read spec/01-grammar.md");
    let extracted = xtask::spec::extract_ebnf(&md);
    let out = Path::new("spec/grammar.ebnf");
    if check {
        let on_disk = std::fs::read_to_string(out).unwrap_or_default();
        if on_disk == extracted {
            eprintln!("spec-extract: spec/grammar.ebnf is in sync");
            ExitCode::SUCCESS
        } else {
            eprintln!("spec-extract: OUT OF SYNC — run `cargo xtask spec-extract`");
            ExitCode::FAILURE
        }
    } else {
        std::fs::write(out, &extracted).expect("write spec/grammar.ebnf");
        eprintln!(
            "spec-extract: wrote spec/grammar.ebnf ({} bytes)",
            extracted.len()
        );
        ExitCode::SUCCESS
    }
}

// ---------------------------------------------------------- diag-catalog --

/// Generate docs/diagnostics.md from wolf_diag's registry and verify the
/// fixture rule: every registered code appears in ≥1 committed snapshot
/// (the s01 hook, armed for real in s10). `--check` verifies sync.
fn diag_catalog(check: bool) -> ExitCode {
    let registry = std::fs::read_to_string("crates/wolf_diag/src/registry.rs")
        .expect("read wolf_diag registry");
    // Parse the rigid one-entry-per-code format:
    //   code!(E0101, "summary", r#" explanation "#);
    let mut entries: Vec<(String, String, String)> = Vec::new();
    let mut rest = registry.as_str();
    while let Some(pos) = rest.find("code!(") {
        rest = &rest[pos + 6..];
        // skip the macro-definition template line (starts with `$`)
        if rest.trim_start().starts_with('$') {
            continue;
        }
        let Some((code, after)) = rest.split_once(',') else {
            continue;
        };
        let code = code.trim().to_string();
        if !code.starts_with('E') && !code.starts_with('W') {
            continue;
        }
        let Some(sq_start) = after.find('"') else {
            continue;
        };
        let Some(sq_len) = after[sq_start + 1..].find('"') else {
            continue;
        };
        let summary = after[sq_start + 1..sq_start + 1 + sq_len].to_string();
        let Some(ex_start) = after.find("r#\"") else {
            continue;
        };
        let Some(ex_len) = after[ex_start + 3..].find("\"#") else {
            continue;
        };
        let explanation = after[ex_start + 3..ex_start + 3 + ex_len]
            .trim()
            .to_string();
        entries.push((code, summary, explanation));
        rest = after;
    }
    if entries.is_empty() {
        eprintln!("diag-catalog: no code! entries found — registry format changed?");
        return ExitCode::FAILURE;
    }
    entries.sort();
    // Fixture rule: each code appears in at least one committed snapshot.
    let mut snaps = Vec::new();
    for krate in std::fs::read_dir("crates").expect("crates dir").flatten() {
        collect_snap_files(&krate.path().join("tests").join("snapshots"), &mut snaps);
    }
    snaps.sort(); // read_dir order is filesystem-dependent; the catalog is not
    let all_snaps: Vec<(PathBuf, String)> = snaps
        .into_iter()
        .map(|p| {
            let body = std::fs::read_to_string(&p).unwrap_or_default();
            (p, body)
        })
        .collect();
    let mut bad = 0u32;
    let mut body = String::from(
        "# Wolf diagnostics catalog\n\nGENERATED by `cargo xtask diag-catalog` \
         from `crates/wolf_diag/src/registry.rs` — do not edit.\nEvery code is a \
         reviewed artifact: it ships with an explanation (`wolf --explain`) and \
         at least one snapshot fixture (CI-enforced).\n",
    );
    for (code, summary, explanation) in &entries {
        let fixtures: Vec<String> = all_snaps
            .iter()
            .filter(|(_, s)| s.contains(code.as_str()))
            .map(|(p, _)| p.display().to_string())
            .collect();
        if fixtures.is_empty() {
            eprintln!(
                "diag-catalog: {code}: no snapshot fixture — every diagnostic is a reviewed artifact"
            );
            bad += 1;
        }
        body.push_str(&format!(
            "\n## {code} — {summary}\n\n{explanation}\n\nFixtures: {}\n",
            if fixtures.is_empty() {
                "NONE".to_string()
            } else {
                fixtures.join(", ")
            }
        ));
    }
    if bad > 0 {
        return ExitCode::FAILURE;
    }
    let out = Path::new("docs/diagnostics.md");
    if check {
        let on_disk = std::fs::read_to_string(out).unwrap_or_default();
        if on_disk != body {
            eprintln!(
                "diag-catalog: docs/diagnostics.md OUT OF SYNC — run `cargo xtask diag-catalog`"
            );
            return ExitCode::FAILURE;
        }
        eprintln!(
            "diag-catalog: {} codes, all with fixtures, catalog in sync",
            entries.len()
        );
    } else {
        std::fs::create_dir_all("docs").expect("mkdir docs");
        std::fs::write(out, body).expect("write catalog");
        eprintln!(
            "diag-catalog: wrote docs/diagnostics.md ({} codes)",
            entries.len()
        );
    }
    ExitCode::SUCCESS
}

fn collect_snap_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_snap_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "snap") {
            out.push(p);
        }
    }
}

// ---------------------------------------------------------------- fmt-lu --

/// Canonical-style gate (s11): every `.lu` file in the tree passes
/// `wolf fmt --check`. Runs through the driver so xtask stays independent
/// of compiler crates.
fn fmt_lu() -> ExitCode {
    if !run_ok("cargo", &["build", "-p", "wolf_driver", "--quiet"]) {
        eprintln!("fmt-lu: failed to build wolf");
        return ExitCode::FAILURE;
    }
    let ok = run_ok("target/debug/wolf", &["fmt", "--check", "corpus"]);
    if ok {
        eprintln!("fmt-lu: corpus is canonical");
        ExitCode::SUCCESS
    } else {
        eprintln!("fmt-lu: unformatted .lu files — run `wolf fmt corpus`");
        ExitCode::FAILURE
    }
}

// --------------------------------------------------------- audit-surface --

/// The D11 unsafety-inventory gate (s22, I13 precursor). Runs `wolf
/// audit-surface` through the driver (xtask stays independent of
/// compiler crates): the corpus's trusted litmus and the green fixture
/// must audit clean; the undeclared-trusted fixture must FAIL — the
/// ring-2 manifest rule (E1303) is a build error, red-tested here.
fn audit_surface() -> ExitCode {
    if !run_ok("cargo", &["build", "-p", "wolf_driver", "--quiet"]) {
        eprintln!("audit-surface: failed to build wolf");
        return ExitCode::FAILURE;
    }
    let wolf = "target/debug/wolf";
    // Green: the corpus memory package (its wolf.pkg declares `root`)
    // and the declared fixture.
    for target in ["corpus/memory/unsafe_trusted.lu", "xtask/fixtures/audit/ok"] {
        let out = Command::new(wolf)
            .args(["audit-surface", target])
            .output()
            .expect("run wolf audit-surface");
        if !out.status.success() {
            eprintln!(
                "audit-surface: `{target}` should audit clean but failed:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
            return ExitCode::FAILURE;
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !stdout.contains("trusted fn") {
            eprintln!(
                "audit-surface: `{target}` inventory is missing its trusted roster:\n{stdout}"
            );
            return ExitCode::FAILURE;
        }
        eprint!("{stdout}");
    }
    // Red: an undeclared trusted module fails the build (E1303).
    let out = Command::new(wolf)
        .args(["audit-surface", "xtask/fixtures/audit/undeclared"])
        .output()
        .expect("run wolf audit-surface");
    if out.status.success() {
        eprintln!(
            "audit-surface: the undeclared-trusted fixture must FAIL — E1303 is load-bearing"
        );
        return ExitCode::FAILURE;
    }
    if !String::from_utf8_lossy(&out.stderr).contains("E1303") {
        eprintln!("audit-surface: the red fixture failed without citing E1303");
        return ExitCode::FAILURE;
    }
    eprintln!("audit-surface: inventory green; undeclared-trusted red-test holds");
    ExitCode::SUCCESS
}

// ------------------------------------------------------------ print-gate --

/// Compiler-phase crates never print (s10): all reporting flows through
/// wolf_diag's structured values and the driver's reporters. wolf_diag
/// itself is exempt (renderers live there but return strings; the gate
/// keeps phases honest, not the engine).
fn print_gate() -> ExitCode {
    let gated = [
        "wolf_lex",
        "wolf_parse",
        "wolf_ast",
        "wolf_sema",
        "wolf_mem",
        "wolf_wir",
    ];
    let mut bad = 0u32;
    for krate in gated {
        let src = Path::new("crates").join(krate).join("src");
        let mut files = Vec::new();
        collect_rs_files(&src, &mut files);
        for f in files {
            let body = std::fs::read_to_string(&f).unwrap_or_default();
            for (i, line) in body.lines().enumerate() {
                let t = line.trim_start();
                if t.starts_with("//") {
                    continue;
                }
                if t.contains("println!") || t.contains("eprintln!") || t.contains("print!") {
                    eprintln!(
                        "print-gate: {}:{}: compiler phases do not print — emit a Diagnostic",
                        f.display(),
                        i + 1
                    );
                    bad += 1;
                }
            }
        }
    }
    if bad > 0 {
        ExitCode::FAILURE
    } else {
        eprintln!("print-gate: compiler phases are print-free");
        ExitCode::SUCCESS
    }
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_rs_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

// ----------------------------------------------------------- conformance --

/// Tag validity + coverage report ([conf.tag], [conf.cover]). Gates on
/// validity; coverage is informational (the debt list).
fn conformance_cmd(args: &[String]) -> ExitCode {
    let out_path = args
        .iter()
        .find_map(|a| a.strip_prefix("--out="))
        .map(PathBuf::from);
    let anchors: serde_json::Value = match std::fs::read_to_string("spec/anchors.json") {
        Ok(s) => serde_json::from_str(&s).expect("anchors.json parses"),
        Err(e) => {
            eprintln!("conformance: read spec/anchors.json: {e} (run spec-extract)");
            return ExitCode::FAILURE;
        }
    };
    let registry = anchors["anchors"].as_object().expect("anchors map");
    let mut files = Vec::new();
    collect_wolf_files(Path::new("corpus"), &mut files);
    files.sort();
    let mut bad = 0u32;
    let mut forward = 0u32;
    let mut untagged = Vec::new();
    let mut tests_per_clause: BTreeMap<String, u32> = BTreeMap::new();
    for f in &files {
        let src = std::fs::read_to_string(f).expect("read corpus file");
        let d = match corpus::parse_directives(&src) {
            Ok(d) => d,
            Err(_) => continue, // xtask corpus owns directive errors
        };
        let litmus = ["grammar", "memory", "conc"]
            .iter()
            .any(|t| f.starts_with(Path::new("corpus").join(t)));
        if d.conforms.is_empty() {
            if litmus {
                eprintln!(
                    "conformance: {}: litmus file missing `conforms:`",
                    f.display()
                );
                bad += 1;
            } else {
                untagged.push(f.clone());
            }
            continue;
        }
        for tag in &d.conforms {
            let ns = tag.split('.').next().unwrap_or("");
            if xtask::spec::REGISTERED_NS.contains(&ns) {
                if registry.contains_key(tag) {
                    *tests_per_clause.entry(tag.clone()).or_default() += 1;
                } else {
                    eprintln!(
                        "conformance: {}: unknown anchor `{tag}` (not in anchors.json)",
                        f.display()
                    );
                    bad += 1;
                }
            } else if xtask::spec::FORWARD_NS.contains(&ns) {
                forward += 1;
            } else {
                eprintln!(
                    "conformance: {}: tag `{tag}` in unregistered namespace `{ns}`",
                    f.display()
                );
                bad += 1;
            }
        }
    }
    let covered = tests_per_clause.len();
    let total = registry.len();
    eprintln!(
        "conformance: {} anchors, {} covered ({} debt), {} forward tags, {} untagged non-litmus files",
        total,
        covered,
        total - covered,
        forward,
        untagged.len()
    );
    if let Some(out) = out_path {
        let commit = git_short_sha();
        let mut body = String::new();
        for (clause, owner) in registry {
            let tests = tests_per_clause.get(clause.as_str()).copied().unwrap_or(0);
            let status = if tests > 0 { "covered" } else { "debt" };
            let _ = owner;
            body.push_str(
                &serde_json::json!({
                    "clause": clause, "tests": tests, "status": status, "commit": commit,
                    "style": style_version(),
                })
                .to_string(),
            );
            body.push('\n');
        }
        if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).expect("mkdir conformance out dir");
        }
        std::fs::write(&out, body).expect("write conformance report");
        eprintln!("conformance: report -> {}", out.display());
    }
    if bad > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

// ---------------------------------------------------------------- differ --

/// Reference differential harness ([proto.harness.differ]).
///
/// Flags: `--self` diffs wolfgang against itself (the CI protocol
/// self-check). `--checked` drives impl A (wolfgang) through the s23
/// miri-lite so the `run` rung is reached — the checker-vs-oracle
/// differential of s23 Target 4; without it wolfgang reports
/// `unsupported` past `mem` and only the static rungs compare.
/// `--corpus=<dir>` walks an alternate corpus root (e.g. the pinned
/// interpreter's vendored tree, so both sides see files their pins
/// share).
/// The `[conf.trap.map]` correspondence: the dynamic trap kind a
/// static memory-tier rejection code maps to. When wolfgang rejects a
/// file with `fail(CODE)` and the oracle traps with the paired kind,
/// the two implementations *agree* — the static tier caught at compile
/// time exactly the fault the dynamic tier would raise (the s23
/// static-vs-dynamic soundness relationship, consistent). A `None`
/// entry means the code has no dynamic counterpart in the closed
/// trap vocabulary (E1006 is a compile error by construction —
/// `[mem.ub.defined]`), so an oracle that runs it clean is a
/// completeness note, never a divergence.
fn static_code_to_trap(code: &str) -> Option<&'static str> {
    match code {
        "E1001" => Some("use-after-move"),
        "E1002" => Some("exclusivity"),
        "E1004" | "E1005" | "E1010" | "E1011" | "E1012" => Some("region-fault"),
        _ => None,
    }
}

fn differ_cmd(args: &[String]) -> ExitCode {
    // s23 triage: cross-implementation runs classify a wolfgang
    // `fail(CODE)` against the oracle's dynamic outcome by the
    // static-vs-dynamic contract, instead of calling every such pair
    // a raw verdict divergence (which is what `--self` needs but a
    // cross-impl run does not). Soundness-direction findings
    // (wolfgang-accepts + oracle-faults) stay hard failures; everything
    // static-stricter is a logged completeness note.
    let triage = args.iter().any(|a| a == "--triage");
    let checked = args.iter().any(|a| a == "--checked");
    // `--native` (s28): impl A compiles each file to MACHINE CODE and
    // reports the executed binary's verdict — the first
    // compiled-vs-interpreted differential. Carried by environment for
    // the same reason as `--checked`.
    let native = args.iter().any(|a| a == "--native");
    let corpus_root = args
        .iter()
        .find_map(|a| a.strip_prefix("--corpus="))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("corpus"));
    let (cmd_a, cmd_b): (String, String) = if args.iter().any(|a| a == "--self") {
        if !run_ok("cargo", &["build", "-p", "wolf_driver", "--quiet"]) {
            eprintln!("differ: failed to build wolf");
            return ExitCode::FAILURE;
        }
        ("target/debug/wolf".into(), "target/debug/wolf".into())
    } else {
        let free: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
        let [a, b] = free.as_slice() else {
            eprintln!(
                "differ: need <implA-cmd> <implB-cmd> (or --self) [--checked] [--native] [--corpus=DIR]"
            );
            return ExitCode::from(2);
        };
        ((*a).clone(), (*b).clone())
    };
    // Impl A (wolfgang) reaches the run rung under `--checked`; the flag
    // is carried via the environment so wolf-interp — which ignores
    // WOLF_CHECKED — is untouched, keeping the two implementations
    // independent (no shared code, s06).
    let conform_run = |cmd: &str, file: &Path, is_a: bool| -> Result<serde_json::Value, String> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let mut command = Command::new(parts[0]);
        command
            .args(&parts[1..])
            .args(["conform-run"])
            .arg(file)
            .arg("--json");
        if is_a && checked {
            command.env("WOLF_CHECKED", "1");
        }
        if is_a && native {
            command.env("WOLF_NATIVE", "1");
        }
        let out = command.output().map_err(|e| format!("spawn {cmd}: {e}"))?;
        if !out.status.success() {
            return Err(format!("{cmd}: conform-run exited nonzero"));
        }
        serde_json::from_slice(&out.stdout).map_err(|e| format!("{cmd}: bad record: {e}"))
    };
    let mut files = Vec::new();
    collect_wolf_files(&corpus_root, &mut files);
    files.sort();
    let mut divergences = 0u32;
    let mut soundness = 0u32;
    let mut completeness = 0u32;
    let mut agreements = 0u32;
    let mut unsupported = 0u32;
    for f in &files {
        let is_member = std::fs::read_to_string(f)
            .ok()
            .and_then(|src| corpus::parse_directives(&src).ok())
            .is_some_and(|d| d.member);
        if is_member {
            continue; // compiled through its module's entry file (s12)
        }
        let (ra, rb) = match (conform_run(&cmd_a, f, true), conform_run(&cmd_b, f, false)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                eprintln!("differ: {}: {e}", f.display());
                divergences += 1;
                continue;
            }
        };
        for (name, r) in [("A", &ra), ("B", &rb)] {
            if let Err(e) = xtask::protocol::validate_record(r) {
                eprintln!("differ: {}: impl {name} record invalid: {e}", f.display());
                divergences += 1;
            }
        }
        let structural =
            !ra["seeded"].as_bool().unwrap_or(false) || !rb["seeded"].as_bool().unwrap_or(false);
        if ra["verdict"] == serde_json::json!("unsupported")
            || rb["verdict"] == serde_json::json!("unsupported")
        {
            unsupported += 1;
        }
        let va = ra["verdict"].as_str().unwrap_or("");
        let vb = rb["verdict"].as_str().unwrap_or("");
        // s23 triage of the fail-vs-run pair: wolfgang (A) rejected
        // statically, the oracle (B) ran to a dynamic outcome.
        if triage && let Some(code) = va.strip_prefix("fail(").and_then(|s| s.strip_suffix(')')) {
            let classify = |note: &str| {
                println!(
                    "{}",
                    serde_json::json!({
                        "file": f.display().to_string(),
                        "class": note,
                        "a": va,
                        "b": vb,
                    })
                );
            };
            // Agreement: the static code maps to the oracle's trap.
            if let Some(k) = static_code_to_trap(code)
                && vb == format!("trap({k})")
            {
                agreements += 1;
                continue;
            }
            // Completeness note: static stricter, oracle runs clean
            // or the code has no dynamic counterpart. Logged, not a
            // divergence (the backlog that feeds rule refinement).
            classify("Completeness");
            completeness += 1;
            continue;
        }
        // Soundness direction: A accepted (ran) but B faulted/UB'd.
        if triage
            && (va.starts_with("exit(") || va == "pass")
            && (vb.starts_with("trap(") || vb.starts_with("ub("))
        {
            println!(
                "{}",
                serde_json::json!({
                    "file": f.display().to_string(),
                    "class": "SOUNDNESS",
                    "a": va,
                    "b": vb,
                })
            );
            soundness += 1;
            divergences += 1;
            continue;
        }
        if let Some((class, detail)) = xtask::protocol::compare(&ra, &rb, structural) {
            println!(
                "{}",
                serde_json::json!({
                    "file": f.display().to_string(),
                    "class": format!("{class:?}"),
                    "detail": detail,
                })
            );
            divergences += 1;
        } else if va == vb
            && (va.starts_with("exit(") || va.starts_with("trap(") || va.starts_with("ub("))
        {
            agreements += 1;
        }
    }
    if triage {
        eprintln!(
            "differ: {} file(s) — {} agreement(s), {} completeness note(s), {} SOUNDNESS finding(s), {} unsupported; {} hard divergence(s)",
            files.len(),
            agreements,
            completeness,
            soundness,
            unsupported,
            divergences,
        );
    } else {
        eprintln!(
            "differ: {} file(s), {} divergence(s), {} in conservatism ledger (unsupported)",
            files.len(),
            divergences,
            unsupported
        );
    }
    if divergences > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

// ------------------------------------------------------------------ dist --

/// Release-artifact mechanics (s02 stub): build the host-target `wolf`
/// binary and stage a versioned archive under target/dist/. The release
/// workflow runs this per-OS on the tier-1 matrix; c13 fills in substance
/// (real cross-compilation, signing).
fn dist() -> ExitCode {
    let host = rustc_host_triple();
    let version = env!("CARGO_PKG_VERSION");
    if !run_ok(
        "cargo",
        &["build", "--release", "-p", "wolf_driver", "--quiet"],
    ) {
        eprintln!("dist: release build failed");
        return ExitCode::FAILURE;
    }
    let exe = if host.contains("windows") {
        "wolf.exe"
    } else {
        "wolf"
    };
    let name = format!("wolf-{version}-{host}");
    let stage = Path::new("target/dist").join(&name);
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage).expect("mkdir dist stage");
    std::fs::copy(Path::new("target/release").join(exe), stage.join(exe))
        .expect("stage wolf binary");
    for f in ["README.md", "LICENSE-MIT", "LICENSE-APACHE"] {
        std::fs::copy(f, stage.join(f)).expect("stage metadata file");
    }
    let archive = format!("target/dist/{name}.tar.gz");
    if !run_ok("tar", &["-C", "target/dist", "-czf", &archive, &name]) {
        eprintln!("dist: tar failed");
        return ExitCode::FAILURE;
    }
    eprintln!("dist: {archive}");
    ExitCode::SUCCESS
}

fn rustc_host_triple() -> String {
    let out = Command::new("rustc")
        .args(["-vV"])
        .output()
        .expect("rustc -vV");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: ").map(str::to_string))
        .expect("host triple in rustc -vV")
}

// --------------------------------------------------------------- helpers --

fn run_ok(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn touch(path: &Path) {
    let now = std::fs::read(path).expect("touch: read");
    std::fs::write(path, now).expect("touch: write");
}

/// The canonical-style version (s11 stability tiers), read textually from
/// wolf_fmt so xtask stays independent of compiler crates. Stamped into
/// JSONL report metadata so style churn never silently invalidates data.
fn style_version() -> String {
    std::fs::read_to_string("crates/wolf_fmt/src/lib.rs")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.contains("pub const STYLE_VERSION"))
                .and_then(|l| l.split('"').nth(1).map(str::to_string))
        })
        .unwrap_or_else(|| "unknown".into())
}

fn git_short_sha() -> String {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn perf_available() -> bool {
    Command::new("perf")
        .args(["stat", "-e", "instructions:u", "-x,", "true"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn perf_instructions(bin: &Path, arg: &str) -> Option<f64> {
    let out = Command::new("perf")
        .args(["stat", "-e", "instructions:u", "-x,"])
        .arg(bin)
        .arg(arg)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    // -x, puts the counter in field 1 of stderr
    let err = String::from_utf8_lossy(&out.stderr);
    err.lines()
        .find(|l| l.contains("instructions"))
        .and_then(|l| l.split(',').next())
        .and_then(|n| n.trim().parse::<f64>().ok())
}

fn max_rss_kb(cmd: &str, args: &[&str]) -> Option<f64> {
    let time = Path::new("/usr/bin/time");
    if !time.exists() {
        return None;
    }
    let out = Command::new(time)
        .arg("-v")
        .arg(cmd)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let err = String::from_utf8_lossy(&out.stderr);
    err.lines()
        .find(|l| l.contains("Maximum resident set size"))
        .and_then(|l| l.rsplit(':').next())
        .and_then(|n| n.trim().parse::<f64>().ok())
}

// ------------------------------------------------------------ deps-check --

/// Enforce the locked crate dependency direction (s00): each workspace
/// crate may depend only on the workspace crates in its allowlist.
fn deps_check() -> ExitCode {
    // crate -> workspace crates it MAY depend on. wolf_driver is the top and
    // unrestricted; xtask may not depend on workspace crates at all.
    let allowed: BTreeMap<&str, Option<&[&str]>> = BTreeMap::from([
        ("wolf_span", Some(&[][..])),
        ("wolf_diag", Some(&["wolf_span"][..])),
        ("wolf_lex", Some(&["wolf_span", "wolf_diag"][..])),
        ("wolf_ast", Some(&["wolf_span"][..])),
        (
            "wolf_parse",
            Some(&["wolf_span", "wolf_diag", "wolf_lex", "wolf_ast"][..]),
        ),
        (
            "wolf_sema",
            Some(&["wolf_span", "wolf_diag", "wolf_ast", "wolf_parse"][..]),
        ),
        (
            "wolf_mem",
            Some(&["wolf_span", "wolf_diag", "wolf_ast", "wolf_sema"][..]),
        ),
        (
            "wolf_wir",
            Some(
                &[
                    "wolf_span",
                    "wolf_diag",
                    "wolf_ast",
                    "wolf_sema",
                    "wolf_mem",
                ][..],
            ),
        ),
        (
            "wolf_fmt",
            Some(
                &[
                    "wolf_span",
                    "wolf_diag",
                    "wolf_lex",
                    "wolf_ast",
                    "wolf_parse",
                ][..],
            ),
        ),
        // The backend interface (s28): the PERMANENT seam of the
        // codegen story — `wir ← backend ← codegen_clif ← driver`.
        // It must NEVER depend on Cranelift; the red-test for a
        // deliberate wolf_backend → cranelift edge lives in the s28
        // acceptance (external crates are caught by the crate's own
        // dependency review, workspace direction by this list).
        (
            "wolf_backend",
            Some(&["wolf_span", "wolf_diag", "wolf_wir"][..]),
        ),
        // wolf_rt appears here for the shared trap-kind/symbol tables
        // (single authority for codegen and the runtime); wolf_rt
        // itself stays dependency-thin (D15) and never sees the
        // compiler.
        (
            "wolf_codegen_clif",
            Some(
                &[
                    "wolf_span",
                    "wolf_diag",
                    "wolf_wir",
                    "wolf_backend",
                    "wolf_rt",
                ][..],
            ),
        ),
        (
            "wolf_codegen_llvm",
            Some(&["wolf_span", "wolf_diag", "wolf_wir", "wolf_backend"][..]),
        ),
        // The editor stack (s52): wolf_query is the compiler-side query
        // contract over the analysis pipeline; wolf_lsp is the
        // transport-only shim and sees no compiler internals beyond it.
        (
            "wolf_query",
            Some(
                &[
                    "wolf_span",
                    "wolf_diag",
                    "wolf_lex",
                    "wolf_ast",
                    "wolf_parse",
                    "wolf_sema",
                    "wolf_fmt",
                ][..],
            ),
        ),
        (
            "wolf_lsp",
            Some(&["wolf_span", "wolf_diag", "wolf_query"][..]),
        ),
        // wolf_rt links into user programs: dependency-thin by law (D15).
        ("wolf_rt", Some(&["wolf_span"][..])),
        ("wolf_driver", None), // top of the graph: unrestricted
        ("xtask", Some(&[][..])),
    ]);

    let out = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .expect("cargo metadata failed to run");
    if !out.status.success() {
        eprintln!("deps-check: cargo metadata failed");
        return ExitCode::FAILURE;
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("cargo metadata: invalid json");

    let mut violations = 0u32;
    for pkg in meta["packages"].as_array().expect("packages") {
        let name = pkg["name"].as_str().expect("name");
        let Some(entry) = allowed.get(name) else {
            eprintln!("deps-check: crate `{name}` has no allowlist entry — add it to xtask");
            violations += 1;
            continue;
        };
        let Some(allow) = entry else { continue };
        for dep in pkg["dependencies"].as_array().expect("deps") {
            let dep_name = dep["name"].as_str().expect("dep name");
            if allowed.contains_key(dep_name) && !allow.contains(&dep_name) {
                eprintln!("deps-check: ILLEGAL EDGE {name} -> {dep_name}");
                violations += 1;
            }
            // The s28 backend seam: the interface crate must NEVER see
            // Cranelift (any dependency kind — dev/build included).
            // c12 replaces the Cranelift crate wholesale; an edge here
            // would make the permanent artifact depend on the
            // disposable one.
            if name == "wolf_backend" && dep_name.starts_with("cranelift") {
                eprintln!(
                    "deps-check: ILLEGAL EDGE {name} -> {dep_name} \
                     (the backend interface must never depend on Cranelift, s28)"
                );
                violations += 1;
            }
        }
    }
    if violations > 0 {
        eprintln!(
            "deps-check: {violations} violation(s) — the crate graph direction is locked (s00)"
        );
        ExitCode::FAILURE
    } else {
        eprintln!("deps-check: crate graph direction ok");
        ExitCode::SUCCESS
    }
}
