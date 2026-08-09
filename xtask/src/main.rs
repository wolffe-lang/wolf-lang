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
        Some("bench") => bench_cmd(&args[1..]),
        Some("fuzz-smoke") => fuzz_smoke(),
        Some("dist") => dist(),
        _ => {
            eprintln!("usage: cargo xtask <ci|deps-check|corpus|bench|fuzz-smoke|dist>");
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
                if d.phase.is_none() {
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
    eprintln!(
        "corpus: {} file(s), {} bad — phase execution stubbed until s31",
        files.len(),
        bad
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

/// Compile-track metrics against the bootstrap toolchain. The incremental
/// metric is a stub-for-wolf until s31 wires `wolf build`; the schema and
/// gating machinery are the deliverable now (s01).
fn bench_compile(runs: u32, commit: &str) -> Option<Vec<serde_json::Value>> {
    let mut records = Vec::new();
    let config = "bootstrap-cargo-stub";
    for _ in 0..runs {
        // (a) clean rebuild of the driver crate
        if !run_ok("cargo", &["clean", "-p", "wolf_driver", "--quiet"]) {
            return None;
        }
        let t = Instant::now();
        if !run_ok("cargo", &["build", "-p", "wolf_driver", "--quiet"]) {
            return None;
        }
        records.push(record(
            "driver",
            "compile",
            "rust",
            "clean_build_wall_s",
            t.elapsed().as_secs_f64(),
            "s",
            commit,
            config,
        ));
        // (b) incremental rebuild after touching one file
        touch(Path::new("crates/wolf_driver/src/main.rs"));
        let t = Instant::now();
        if !run_ok("cargo", &["build", "-p", "wolf_driver", "--quiet"]) {
            return None;
        }
        records.push(record(
            "driver",
            "compile",
            "rust",
            "incr_rebuild_wall_s",
            t.elapsed().as_secs_f64(),
            "s",
            commit,
            config,
        ));
    }
    // (c) max-RSS of one incremental rebuild, via /usr/bin/time -v
    touch(Path::new("crates/wolf_driver/src/main.rs"));
    match max_rss_kb("cargo", &["build", "-p", "wolf_driver", "--quiet"]) {
        Some(kb) => records.push(record(
            "driver",
            "compile",
            "rust",
            "max_rss_kb",
            kb,
            "kB",
            commit,
            config,
        )),
        None => eprintln!("bench: /usr/bin/time unavailable — max_rss_kb skipped"),
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
                let dir = if delta_pct > 0.0 {
                    "REGRESSED"
                } else {
                    "improved"
                };
                eprintln!("  {key}: {dir} {delta_pct:+.1}%");
                if delta_pct > 0.0 {
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
            "wolf_codegen_clif",
            Some(&["wolf_span", "wolf_diag", "wolf_wir"][..]),
        ),
        (
            "wolf_codegen_llvm",
            Some(&["wolf_span", "wolf_diag", "wolf_wir"][..]),
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
