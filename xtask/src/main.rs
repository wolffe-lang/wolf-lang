//! Repo automation (`cargo xtask <command>`). CI-shaped: pure, exit-code
//! driven — the s02 CI workflows are thin wrappers over these commands.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

use xtask::corpus::{self, Directives};
use xtask::stats;

mod bench_t1;
mod ritual;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("ci") => ci(),
        Some("deps-check") => deps_check(),
        Some("corpus") => corpus_cmd(),
        Some("abi-check") => abi_check(),
        Some("debug-check") => debug_check(),
        Some("midend-rate") => midend_rate(),
        Some("install") => {
            // `cargo xtask install [DIR]` — the two-artifact install: the
            // driver locates libwolf_rt.a NEXT TO the `wolf` binary (or via
            // WOLF_RT_LIB), so the binary never travels alone.
            let dest = args
                .get(1)
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    let home = std::env::var_os("HOME").expect("HOME unset");
                    std::path::Path::new(&home).join(".local/bin")
                });
            assert!(
                run_ok(
                    "cargo",
                    &[
                        "build",
                        "--release",
                        "-p",
                        "wolf_driver",
                        "-p",
                        "wolf_rt",
                        "-p",
                        "wolf_cimport"
                    ]
                ),
                "release build failed"
            );
            std::fs::create_dir_all(&dest).expect("create install dir");
            // The importer worker installs beside `wolf`, which is
            // exactly where `wolf` looks for it (s46).
            for f in ["wolf", "libwolf_rt.a", "wolf-cimport-worker"] {
                let from = std::path::Path::new("target/release").join(f);
                let to = dest.join(f);
                // unlink first: a running `wolf lsp` holds the old inode
                // (ETXTBSY on in-place copy); the unlinked file lives on
                // for its holders and the new one takes the name.
                let _ = std::fs::remove_file(&to);
                std::fs::copy(&from, &to)
                    .unwrap_or_else(|e| panic!("copy {f} -> {}: {e}", to.display()));
            }
            println!(
                "install: wolf + libwolf_rt.a + wolf-cimport-worker -> {}",
                dest.display()
            );
            ExitCode::SUCCESS
        }
        Some("bench") => bench_cmd(&args[1..]),
        Some("bench-gates") => bench_t1::bench_gates(),
        Some("fuzz-smoke") => fuzz_smoke(),
        Some("fmt-fuzz") => fmt_fuzz(&args[1..]),
        Some("dist") => dist(),
        Some("spec-extract") => spec_extract(args.iter().any(|a| a == "--check")),
        Some("conformance") => conformance_cmd(&args[1..]),
        Some("differ") => differ_cmd(&args[1..]),
        Some("lane-coverage") => lane_coverage_cmd(&args[1..]),
        Some("print-gate") => print_gate(),
        Some("diag-catalog") => diag_catalog(args.iter().any(|a| a == "--check")),
        Some("doc-catalog") => doc_catalog(args.iter().any(|a| a == "--check")),
        Some("fmt-lu") => fmt_lu(),
        Some("peel") => peel_cmd(&args[1..]),
        Some("audit-surface") => audit_surface(),
        _ => {
            eprintln!(
                "usage: cargo xtask <ci|deps-check|corpus|peel|bench|bench-gates|fuzz-smoke|fmt-fuzz|dist|spec-extract|conformance|differ|lane-coverage|print-gate|diag-catalog|doc-catalog|fmt-lu|audit-surface|midend-rate>"
            );
            eprintln!(
                "       cargo xtask lane-coverage [--json]   (the [proto.cmp.coverage] gate)"
            );
            eprintln!(
                "       cargo xtask peel [FILTER] [--all]    (reasons BEHIND the lowering ledger's\n                                             refusals — the contract-author's lens)"
            );
            eprintln!(
                "       cargo xtask bench --track=<runtime|compile|t1|irvolume> [--runs=N] [--out=FILE]"
            );
            eprintln!("                         [--kernels=a,b]   (t1 only)");
            eprintln!("       cargo xtask bench diff <baseline.jsonl> <candidate.jsonl> [--gate]");
            eprintln!("       cargo xtask bench gate <t1.jsonl>    (the M2 verdict, nightly)");
            eprintln!("       cargo xtask bench ritual [--out-dir=DIR] [--dry-run]");
            eprintln!(
                "                         (the s44 nightly, codified: quiet check, conditions,\n\
                 \x20                        run, gate, tick ledger, bench-data append)"
            );
            eprintln!(
                "       cargo xtask fmt-fuzz [--ci] [--seconds=N] [--seed=N] [--cases=N] [--out=DIR]"
            );
            eprintln!(
                "                            [--expect=N] [--allow-open] [--file=PATH] [--triage=DIR]"
            );
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
        // `fuzz/` is deliberately NOT a workspace member (it needs
        // nightly to *build*), so `--workspace` never type-checks it and
        // a field added to a shared struct stayed green here and went
        // red on CI. Checking costs seconds and needs only stable.
        (
            "fuzz-check",
            &["check", "--manifest-path", "fuzz/Cargo.toml"],
        ),
        ("deps-check", &["xtask", "deps-check"]),
        ("corpus", &["xtask", "corpus"]),
        ("abi-check", &["xtask", "abi-check"]),
        ("debug-check", &["xtask", "debug-check"]),
        ("midend-rate", &["xtask", "midend-rate"]),
        // s44: the DETERMINISTIC bench gates only (IR volume ratchet +
        // vectorization witnesses). Wall-derived M2 numbers are nightly
        // and report-only — D5, and this sprint is where that line got
        // drawn on purpose rather than by accident.
        ("bench-gates", &["xtask", "bench-gates"]),
        ("spec-extract", &["xtask", "spec-extract", "--check"]),
        ("conformance", &["xtask", "conformance"]),
        ("print-gate", &["xtask", "print-gate"]),
        ("diag-catalog", &["xtask", "diag-catalog", "--check"]),
        ("doc-catalog", &["xtask", "doc-catalog", "--check"]),
        ("fmt-lu", &["xtask", "fmt-lu"]),
        ("fmt-fuzz", &["xtask", "fmt-fuzz", "--ci"]),
        ("audit-surface", &["xtask", "audit-surface"]),
        // The release archive is public-facing product: staging it in
        // CI keeps a broken manifest from reaching a release page again
        // (it did once — the archive shipped without the runtime lib,
        // and a later license edit made staging panic outright).
        ("dist-smoke", &["xtask", "dist"]),
        ("differ-self", &["xtask", "differ", "--self"]),
        // s82: what the differential actually covers, gated. The
        // release-parity floor keeps two tiers compared on a file set
        // that may not shrink; this keeps the file set itself from
        // shrinking under all three lanes at once.
        ("lane-coverage", &["xtask", "lane-coverage"]),
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

// ------------------------------------------------------------ midend-rate --

/// `xtask midend-rate` — the s42 X3 claw-back acceptance as a NAMED
/// PR-CI step, and the per-commit REPORT of the number itself.
///
/// The contract gates ≥80% of the overflow checks inside hot loops on
/// the checked-arith kernel tier (`corpus/kernels/`) being statically
/// eliminated — directly by the π-range analysis or by amendment 3's
/// loop-versioning backstop. The gate lives in `wolf_wir`'s
/// `midend_corpus` test (that is where the compiler crates are); this
/// lane runs exactly that test with output uncaptured and echoes the
/// measured rate, so the number is visible per commit instead of being
/// swallowed by the test harness on success.
fn midend_rate() -> ExitCode {
    let out = Command::new("cargo")
        .args([
            "test",
            "-p",
            "wolf_wir",
            "--test",
            "midend_corpus",
            "kernel_tier_elimination_rate",
            "--",
            "--nocapture",
        ])
        .output();
    let Ok(out) = out else {
        eprintln!("midend-rate: cannot run cargo");
        return ExitCode::FAILURE;
    };
    // The measurement prints on the test's stderr; echo the per-kernel
    // lines and the tier total verbatim — this IS the report.
    let text = String::from_utf8_lossy(&out.stderr);
    let mut reported = false;
    for line in text.lines() {
        if line.starts_with("kernel ") {
            eprintln!("midend-rate: {line}");
            reported |= line.starts_with("kernel tier elimination rate:");
        }
    }
    if !out.status.success() {
        eprintln!("midend-rate: the ≥80% elimination-rate acceptance FAILED");
        eprint!("{text}");
        return ExitCode::FAILURE;
    }
    if !reported {
        eprintln!("midend-rate: the test passed but printed no rate — the lane is stale");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
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
        eprintln!("debug-check: debug sections + debugger transcripts green");
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
                if d.is_member() {
                    // compiled through its module's entry file
                    // (s12/D59: `member: true` or a plain file with no
                    // entry machinery at all)
                } else if d.phase.is_none() {
                    eprintln!("corpus: {}: missing `//! phase:` directive", f.display());
                    bad += 1;
                } else if d.check.is_none() {
                    eprintln!(
                        "corpus: {}: has `phase:` but no `check:` — an entry file carries \
                         both ([conf.directive.member])",
                        f.display()
                    );
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
    // The two halves of the rejection ledger, counted from what the
    // compiler DID rather than from what the headers claim (s91).
    let mut rules = 0u32;
    let mut forward_pins = 0u32;
    if run_ok(
        "cargo",
        &["build", "-p", "wolf_driver", "-p", "wolf_rt", "--quiet"],
    ) {
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
            // The forward-pin ledger (s91). A `check: fail(CODE)` file
            // is a claim that the compiler ENFORCES a rule. When the
            // compiler declines the file instead, the claim is an
            // intention, and it must say so — otherwise it is counted
            // as a rule by everything downstream, in the direction that
            // flatters us. Checked both ways so the marker cannot rot:
            // a pin that has since landed drops its `forward:` line in
            // the same commit that makes the rejection real.
            if matches!(d.check, Some(corpus::Check::Fail(_))) {
                if verdict == "unsupported" && d.forward.is_none() {
                    let why = String::from_utf8_lossy(&out.stderr)
                        .lines()
                        .find_map(|l| l.split_once("unsupported — ").map(|(_, w)| w.to_string()))
                        .unwrap_or_else(|| format!("declined at {reached}"));
                    eprintln!(
                        "corpus: {}: pins a rejection the compiler cannot make — it declines this \
                         file ({why}). That is an intention, not a rule: say so with \
                         `//! forward: <what is missing>`",
                        f.display(),
                    );
                    bad += 1;
                } else if verdict != "unsupported"
                    && let Some(reason) = &d.forward
                {
                    eprintln!(
                        "corpus: {}: `//! forward: {reason}` says this is not implemented, but the \
                         compiler answers `{verdict}` — the pin landed; drop the `forward:` line \
                         and let it count as a rule",
                        f.display(),
                    );
                    bad += 1;
                }
                if verdict.starts_with("fail(") {
                    rules += 1;
                } else if d.forward.is_some() {
                    forward_pins += 1;
                }
            } else if let Some(reason) = &d.forward
                && verdict != "unsupported"
            {
                eprintln!(
                    "corpus: {}: `//! forward: {reason}` says this is not implemented, but the \
                     compiler answers `{verdict}` — drop the stale marker",
                    f.display(),
                );
                bad += 1;
            }
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
            // The warning ledger (s67): the record's `warnings` array
            // must carry exactly the codes the `warns:` directive
            // declares — the repo's own `--deny-warnings` posture (an
            // undeclared warning fails CI; a declared one that stops
            // firing is a stale header, equally loud).
            let mut fired: Vec<String> = rec["warnings"]
                .as_array()
                .map(|ws| {
                    ws.iter()
                        .filter_map(|w| w["code"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            fired.sort();
            fired.dedup();
            if fired != d.warns {
                eprintln!(
                    "corpus: {}: warning ledger mismatch — fired [{}], header declares [{}] \
                     (declare with `//! warns:` or fix the warning; the corpus is \
                     --deny-warnings clean by decree)",
                    f.display(),
                    fired.join(", "),
                    d.warns.join(", "),
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
    // Both numbers, never one corrected number (s91). A forward pin is
    // not a subtraction to be quietly applied to the rule count: it is
    // its own quantity, and a reader who is told only "N rules" cannot
    // tell whether the corpus grew a rule or grew an intention.
    if executed {
        eprintln!(
            "corpus: fail-pin ledger: {rules} rule(s) the compiler enforces, \
             {forward_pins} forward pin(s) it does not implement yet"
        );
    }
    if bad > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Every corpus file that is a `phase: run` ENTRY (not a `member:`
/// module) — the set that compiles end to end through both native tiers.
/// s44's IR-volume lane sweeps it because #70 states its budget "geomean
/// across the corpus", and the 13-kernel suite is a deliberately hot
/// sample of it.
fn corpus_run_entries() -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_wolf_files(Path::new("corpus"), &mut files);
    files.sort();
    files.retain(|f| {
        std::fs::read_to_string(f)
            .ok()
            .and_then(|src| corpus::parse_directives(&src).ok())
            .is_some_and(|d| !d.is_member() && d.phase.as_deref() == Some("run"))
    });
    files
}

/// `cargo xtask peel [FILTER] [--all]` — the survey lens over the
/// corpus (the c19 closeout's lesson made a tool): for every file the
/// lowering ledger marks `refused`, print what is BEHIND the fail-fast
/// reasons. A `behind:` line was collected by skipping the refusing
/// statement and lowering on, so it may be follow-on noise — the
/// contract is "the first reason per statement is reliable, the rest
/// are leads". Quiet for files where the survey adds nothing (--all
/// prints those too); FILTER substring-matches the path. Never a gate:
/// output is unsnapshotted, and `ci` does not run it.
fn peel_cmd(args: &[String]) -> ExitCode {
    let all = args.iter().any(|a| a == "--all");
    if let Some(bad) = args.iter().find(|a| a.starts_with("--") && *a != "--all") {
        eprintln!("peel: unknown flag `{bad}` (flags: --all)");
        return ExitCode::from(2);
    }
    let filter = args.iter().find(|a| !a.starts_with("--"));
    if !run_ok(
        "cargo",
        &["build", "-p", "wolf_driver", "-p", "wolf_rt", "--quiet"],
    ) {
        eprintln!("peel: driver build failed");
        return ExitCode::FAILURE;
    }
    let mut files = Vec::new();
    collect_wolf_files(Path::new("corpus"), &mut files);
    files.sort();
    let mut shown = 0u32;
    let mut behind_total = 0u32;
    let mut ices = 0u32;
    for f in &files {
        let rel = f.display().to_string();
        if let Some(pat) = filter
            && !rel.contains(pat.as_str())
        {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(f) else {
            continue;
        };
        // Member files compile through their module's entry file (s12/D59).
        if corpus::parse_directives(&src).is_ok_and(|d| d.is_member()) {
            continue;
        }
        let out = Command::new("target/debug/wolf")
            .arg("conform-run")
            .arg(f)
            .arg("--json")
            .arg("--dump=peel")
            .output();
        let Ok(out) = out else { continue };
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !out.status.success() {
            // The lens must complete over the whole corpus; a panic
            // here is a survey bug, and hiding it would let the tool
            // rot exactly the way the ledger's masking did.
            eprintln!("peel: {rel}: ICE (exit {:?})", out.status.code());
            for l in stderr.lines().rev().take(3) {
                eprintln!("peel:   {l}");
            }
            ices += 1;
            continue;
        }
        let ledger: Vec<&str> = stderr
            .lines()
            .filter(|l| l.starts_with("peel: ledger: "))
            .collect();
        let behind: Vec<&str> = stderr
            .lines()
            .filter(|l| l.starts_with("peel: behind: "))
            .collect();
        behind_total += behind.len() as u32;
        if behind.is_empty() && !(all && !ledger.is_empty()) {
            continue;
        }
        shown += 1;
        eprintln!("peel: {rel}");
        for l in ledger.iter().chain(behind.iter()) {
            eprintln!("peel:   {}", l.trim_start_matches("peel: "));
        }
    }
    eprintln!(
        "peel: {shown} file(s) shown, {behind_total} reason(s) behind the ledger, {ices} ICE(s)"
    );
    if ices > 0 {
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

// --------------------------------------------------------- lane-coverage --

/// The lanes wolfgang can reach the `run` rung on, in report order.
/// `default` (no flag) is carried deliberately: its count is the
/// evidence that a plain `conform-run` compares nothing dynamically,
/// which is the shape of the bug lupin 0.1.9 was running into for four
/// pins before anyone noticed.
const RUN_LANES: &[(&str, &str)] = &[
    ("default", ""),
    ("checked", "--checked"),
    ("native", "--native"),
    ("release", "--release"),
];

/// The lanes the coverage figure is the union OF.
const COMPARED_LANES: &[&str] = &["checked", "native", "release"];

/// Coverage floors (`[proto.cmp.coverage]`, s82 — wolf-lang#90).
///
/// Written in the shape of the release-parity floor
/// (`crates/wolf_driver/tests/release_native.rs`) and for the same
/// reason: a gate whose measured set can shrink in silence is a gate
/// that stays green while it stops testing anything. These RATCHET —
/// when a lane learns to execute more of the corpus, raise them in the
/// same commit; they are never lowered to make a run pass. Lowering one
/// is a deliberate, reviewed statement that the differential now sees
/// less than it did, and it needs the reason written next to it.
///
/// s82 baseline, measured at that sprint's HEAD over 261 non-member
/// entries: checked 130, native 125, release 115, union 145, all-three
/// 110. The union is above the best single lane — that gap IS the
/// non-nesting wolf-lang#90 reported, and it is why no single-lane
/// count may stand in for the coverage of the differential.
///
/// Ratcheted once at the close of the s88/s89/s90 wave, over 271
/// entries, from integrated trunk rather than from any one lane: a
/// floor raised inside a branch breaks the next branch to merge for a
/// reason that is not its fault. The lanes are still not nested (union
/// 153, all three 118), which is the property the gate exists to keep
/// visible.
/// Ratcheted again by s86, over 272 entries: the release lane goes
/// 123 → 134 in one step, because its last conc refusal (`func.addr`,
/// the compiled task entry) was the only thing keeping eleven
/// spawn-bearing programs off that lane. native +1 is s86's own new
/// corpus entry. `all-three` does NOT move: the CHECKED lane still
/// refuses structured concurrency by name (C1, deferred), so the two
/// native lanes and the checked lane remain un-nested — which is
/// exactly the fact this gate exists to keep visible.
/// Ratcheted by s92, over 273 entries: every number moves by exactly
/// one, and `all-three` moves for the first time since it was set. One
/// file did it — `memory/byte_view_escape.lu` was `fail(E1015)`, a
/// refusal every lane agreed on, and is now a run every lane agrees on
/// (the lend degrades to a copy and warns W1004). A file that was
/// refused everywhere and now runs everywhere is the one shape that
/// raises the intersection, so +1 on `all-three` is not a floor
/// getting looser; it is a rule the compiler stopped enforcing because
/// the program had a meaning all along.
/// Ratcheted by s93, over 275 entries: every number moves by exactly
/// two, and all two are NEW files — `generics/first_of_list.lu` (the
/// #105 program) and `generics/two_instances.lu`, the first corpus
/// entries that call a generic free function with concrete arguments
/// and run. They run on every lane, so `all-three` moves with them; a
/// POPULATION change, not a lane learning something. No pre-existing
/// file moved lanes: the five that refused for monomorphization lost
/// that reason and kept their real next one (four are c06 — dispatch
/// tables, a fn value as an argument — and `comptime/norm_linear.lu`
/// is s94's unelaborated generic nominal), so native/release did not
/// gain a single old file. The s93 contract expected `rows/hof_tail`
/// to be the +1; it is not, and the ledger says why.
/// Ratcheted by s87, over 275 entries: native and release +1, union
/// +1, checked and `all-three` unchanged. One pre-existing file moved
/// — `conc/proc_spawn_loop.lu`, refused since s86 as "a proc spawned
/// in a loop (its env outlives every extent here — s87)", now runs on
/// both native tiers because the runtime copies a proc's argument
/// record before the spawn returns (`[abi.native.procenv]`). The
/// checked lane still refuses structured concurrency by name (C1,
/// deferred), so `all-three` does not move: the last "concurrency"
/// refusal on the native lanes is gone, and the lanes stay un-nested
/// for the reason this gate exists to show.
/// Ratcheted by s95 over 279 entries: native/release +8 — eight files
/// whose only native blocker was static trait dispatch (the c06
/// deferral) now run; checked +0 — the checked rung still refuses
/// impl-bearing modules by name (#12), which is why `all-three` moves
/// only +2 (derive-shaped movers with no impl in the entry surface).
/// The s95 contract guessed all-three would not move; two files
/// measured otherwise, and the union (+6) is the honest sum: the two
/// lanes that gained overlap the checked lane on two entries only.
/// (History: s94 145/142/142 — the four `corpus/generics/` witnesses,
/// +4 every lane.)
/// Ratcheted by s97 over 279 entries: native and release +2, union
/// +2 — `rows/hof_tail.lu` and `rows/inferred_private.lu`, refused
/// since always because WIR had no opcode to call through a fn value
/// (#112), run on both native tiers through `call.ind`. checked +0:
/// the checked rung still refuses the module-item read that produces
/// the value (#12), so `all-three` holds at 127 — the first ratchet
/// whose all-three prediction MEASURED true, stated for the record.
/// Ratcheted by s96 over 279 entries: native and release +1 —
/// `traits/dyn_ok.lu` runs, its dispatch lowered to the existing ops
/// (pair split, slot load, s97's `call.ind`; zero new opcodes).
/// checked +0 and union +0: the checked lane was ALREADY executing
/// the file (its refusal only ever lived in WIR), which is exactly
/// why `all-three` moves +1 — the native tiers joining a file the
/// checked lane held is the one shape where all-three moves without
/// union moving. Nothing constructs a dyn value yet (the filed
/// surface decision), so this is the dispatch chain compiling and
/// linking, with its execution pinned by the backends' hand-built
/// vtable fixture.
/// Adjusted by s98 over 280 entries — the one DELIBERATE lowering in
/// this table's history, with the reason the gate demands: checked
/// 145 → 144 and all-three 128 → 127, because `traits/dyn_ok.lu` now
/// CONSTRUCTS and CALLS through its pairs (D47, `[mem.dyn.unsize]`),
/// and the checked rung refuses "trait dispatch in checked execution"
/// by name — a pre-existing posture (#12's family) that never bit
/// while the s96 dispatch chain was dead code behind the missing
/// construction rule. The file did not regress; it gained the content
/// the checked executor never supported, and pretending otherwise
/// would need the pin softened. native holds 153; release holds 153
/// (the vtable's slot shims are call-graph edges and DFE roots now —
/// without that the release tier refused a table whose shims the
/// partitioner had separated or eliminated). union holds 170. The
/// new `traits/dyn_temp_refused.lu` is `rejected` (E0810 fail-pin,
/// lupin divergence recorded as wolf-interp#31) and sits outside
/// every run count.
/// Ratcheted by #12 over 280 entries: the checked executor learned
/// the dispatch family, and the s98 retreat's restoration
/// (144 → 145 predicted) MEASURED at 144 → **153** — nine files, not
/// one, because the retreat's file was only the youngest member of
/// the family the executor refused. Trait dispatch reads the s17
/// record (impl override, then trait default with `Self` carried
/// per-frame); qualified calls key the record's own method name (the
/// dotted callee was the miss); fn VALUES land (`Value::Fn`, the
/// s95/s97 twin — hof_tail and inferred_private execute); `dyn`
/// dispatch reads the concrete type the D47 cast stamped on the
/// value; generic bodies bind their rigids at the call site from the
/// caller's typed args (the machine's monomorphization). all-three
/// 127 → **136** — every mover already ran on both native tiers.
/// union holds 170: the checked lane joined files others held, the
/// mirror of s96's move. One divergence found and FIXED en route:
/// the trait-impl index was overwriting the inherent one, so
/// `d.speak()` answered the trait — ty.method.order's witness
/// (`method_inherent.lu`) now prints `woof` then `spoken 7`,
/// byte-identical to lupin.
/// Ratcheted by the front-end lane (#111/#116/#23/#119, 2026-08-21):
/// five witnesses enter at run — explicit generic application
/// (`generics/explicit_apply`, plus `grammar/brackets_generic_call`
/// waking from its resolve pin), a qualified module fn as a value
/// (`typecheck/fn_value_import`), a payload-free variant as a bare
/// value across a module boundary (`typecheck/variant_value`), and an
/// impl on a primitive (`traits/prim_impl`). Native/release take all
/// five (+5); the checked executor reaches three of them (+3);
/// all-three follows the checked lane (+3). Counts measured by this
/// gate, not predicted.
// s104 ratchet over 288 entries: `kernels/guarded_stencil.lu` (the
// overlap-guard witness) executes on native and release — 159 → 160
// each, union 176 → 177. checked holds 156 and all-three holds 139:
// like the other `kernels/` perf witnesses, it is outside the checked
// executor's run set, so the lanes' non-nesting is unchanged.
// s106 ratchet over 290 entries: net crosses (c26, #118). The two s39
// witnesses (`net/echo_roundtrip`, `net/refused_row`) plus the new
// `net/read_deadline` timeout witness execute on native and release
// for the first time, and `net/spawn_accept` (the blocking-honesty
// witness) joins them — 160 → 164 each. checked gains the timeout
// witness (156 → 157; spawn_accept stays outside its run set: C1
// defers structured concurrency there). union 177 → 179, all-three
// 139 → 142 (the three non-conc net files now run everywhere).
// s105 ratchet over 292 entries — the region VALUE tier (c25): four
// new witnesses. `memory/region_value_pass.lu` and
// `memory/region_value_return.lu` execute on all three lanes (native
// and release learned the handle-as-value lowering; the checked
// executor always ran regions) — +2 native, +2 release, +2 all-three.
// `memory/region_value_container.lu` and `memory/region_value_elem.lu`
// pin the refusals that REMAIN (a region behind an aggregate boundary
// / a container element) and execute on the checked lane only — with
// the two movers, checked +4 and union +4. Counts measured by this
// gate, not predicted.
// s105 ratchet #2 over 299 entries — the closure value tier (c25):
// native and release +3 each (165), union +3 (184). One PRE-EXISTING
// file moved — `memory/prov_holy_grail.lu`, refused since s73 as
// "closure lowering outside `spawn`", runs on both native tiers now
// that a capture-free closure lambda-lifts to an s95 fn value — plus
// the two new run witnesses (`closure_value_paths`,
// `closure_kill_list`). checked holds 160 and all-three holds 141:
// the checked executor still refuses closures by name ("closures in
// checked execution", the #12 family), so the native tiers pulled
// ahead — the non-nesting this gate exists to keep visible. The four
// new refusal witnesses (escape/write/mut/region-capture) execute on
// no lane and sit outside every run count.
// merge ratchet (s106 + s105, over 301 entries): the two lanes ratcheted
// off the same base over DISJOINT movers (net/* vs memory/*), so the
// merged floors are the sum of both deltas — measured by this gate on
// the merged tree, not predicted.
// s107 ratchet over 301 entries: json and process cross (c26's last
// arms, #118). `json/query.lu`, `json/rows.lu`, `os/spawn_rows.lu`
// execute on native and release for the first time — 169 → 172 each,
// all-three 144 → 147 (the checked lane always ran all three; the
// native tiers joined files it held, the s96 mirror shape). checked
// holds 161 and union holds 186 — no file entered or left the union,
// which is exactly what a crossing of checked-held files looks like.
// The last two `checked lane only` arms are gone from the lowering;
// the checked-native builtin split is CLOSED. Counts measured by this
// gate, not predicted.
// s108 ratchet over 302 entries: the raw-literal decode fix (#76).
// `strings/raw_fences.lu` — the `#`-fenced forms the off-by-one never
// met — executes on all three lanes: +1 everywhere (162/173/173,
// union 187, all-three 148). `lints/raw_interp_braces.lu` advanced
// `phase: wir` → `run` in the same commit but moves no count: every
// lane already EXECUTED it (with the opening quote in the value); the
// stale pin only stopped the harness from judging its output. Counts
// measured by this gate, not predicted.
// s108 ratchet #2 over 304 entries: the entry-signature rule (#106).
// `typecheck/main_unit_row.lu` — the run witness holding E0414's
// legal boundary (`fn main() -> !()`) — executes on all three lanes:
// +1 everywhere (163/174/174, union 188, all-three 149). Its fail
// twin `typecheck/main_returns_str.lu` executes on no lane (a static
// rejection, the residue class the divergence used to hide from) and
// moves no count. Counts measured by this gate, not predicted.
// s108 ratchet #3 over 306 entries: call-divergence in fallback
// position (#35, narrowed). `rows/handler_diverge_call.lu` — the
// generic handler whose tail is the literal `assert(false)`, hit path
// — executes on all three lanes (+1 each, +1 all-three). Its trap
// twin `rows/handler_diverge_trap.lu` executes on native and release
// (the miss path traps `assert` there) while the checked executor
// refuses the raising call in argument position by name — the s105
// non-nesting shape: native/release +2 to 176, checked +1 to 164,
// union +2 to 190, all-three +1 to 150. Counts measured by this gate,
// not predicted.
// s108 ratchet #4 over 310 entries: the callable tier's two gaps
// (#116). One PRE-EXISTING file moved — `typecheck/fn_value_import`,
// whose qualified cross-module fn value the checked executor refused
// while both compiled tiers ran it, now runs on all three lanes:
// checked +1 to 165, all-three +1 to 151. The new
// `typecheck/nested_fn_value.lu` — a nested named fn as a fn value —
// executes on native and release (+1 each to 177, union +1 to 191);
// the checked executor still refuses the closure family by name, the
// s105 split. Its capture twin `nested_fn_capture.lu` executes on no
// lane. Counts measured by this gate, not predicted.
// s108 ratchet #5 over 311 entries: #29's probe-close witness.
// `resolve/leaf_twins` — two same-leaf modules (`fmt.float`,
// `math.float`) coexisting, cross-importing, and disambiguated with
// `use … as` — executes on all three lanes: +1 everywhere
// (166/178/178, union 192, all-three 152). The issue's collapse no
// longer reproduces anywhere; this file keeps it that way. Counts
// measured by this gate, not predicted.
// s109 ratchet over 317 entries: the rulings land (D51/D52). Six new
// run-phase witnesses. D51's three — `rows/nested_row_return` and
// `nested_row_param` advancing resolve→run plus the new
// `nested_row_merge_payload` — execute on all three lanes (+3 each,
// +3 all-three). D52's three: `tag_let_position` and
// `tag_shadow_local` execute on all three (+2 each, +2 all-three;
// the checked lane's WRONG let value is #122, but coverage counts
// execution reach); `tag_arg_position` executes on native and release
// only (the checked executor refuses the raising call in argument
// position by name — the s105 non-nesting shape): native/release +1.
// Totals 171/184/184, union 198, all-three 157. Counts measured by
// this gate, not predicted.
// s110 ratchet over 319 entries: the header-promotion witnesses.
// `kernels/hot_header` (the b3 push-loop shape, promotion gated in
// midend_corpus.rs) and `kernels/hot_header_alias` (the aliasing
// complement, refusal gated) execute on native and release (+2 each
// to 186, union +2 to 200); the checked executor reports both
// unsupported ("this operator in checked execution" — the
// guarded_stencil verdict, same tier, same shape), so checked and
// all-three hold at 171/157. Counts measured by this gate, not
// predicted.
// s111 ratchet over 323 entries: the crypto probe pays back (c27,
// wave five). Four new run-phase witnesses, each executing on ALL
// three lanes, so every number moves by exactly four:
// `kernels/sha256_block` (#130 — the checked tier's new wrapping
// shift/bitwise arms carry a full FIPS 180-4 compression),
// `typecheck/wrap_narrow_cast` (#131 — itrunc to wrapping targets),
// `generics/list_wrapping_elem` (#132 — the SHA-512 K-table shape),
// and `memory/mut_param_field_lend` (#133 — mut field paths off mut
// parameters re-lend into the caller's slot). No pre-existing file
// moved lanes: the checked tier's non-wrapping bitwise refusal holds
// (`hot_header`/`guarded_stencil`/`walk_twice` keep their honest
// verdicts), and #122's fix changes tag_let_position's checked
// VERDICT (exit 1 -> exit 0), not its execution reach. Totals
// 175/190/190, union 204, all-three 161. Counts measured by this
// gate, not predicted.
// s112 ratchet over 333 entries: the constant-time tier lands (c28).
// Three new run-phase witnesses, each a #[consttime] fn executing on
// ALL three lanes (the verifier gates the shared ladder pre-fork and
// costs nothing off-attribute), so every number moves by exactly
// three: `kernels/ct_tag_compare` (the accumulate-then-single-check
// tag shape, scalar limbs), `kernels/ct_cswap` (the arithmetic
// conditional-select), and `ct/public_len` (the public(…) exemption
// driving a loop bound). The seven ct refusal witnesses land as
// static rejections (E1601-E1607, `rejected` class) and by design do
// not move coverage. No pre-existing file moved lanes — the
// off-by-default proof at the coverage level. Totals 178/193/193,
// union 207, all-three 164. Counts measured by this gate, not
// predicted.
// s113 ratchet over 346 entries: D54's numeric-literal system (the
// anchor) lands with the c2f witness and its litmus battery, and #138's
// int↔float casts close. Nine new run-phase witnesses execute on the
// lanes: six adopt/propagate/divide litmus files plus the c2f witness
// all run on ALL three lanes (checked/native/release each +9), and the
// three cast run-witnesses (`cast_int_to_float`, and the two
// float→int trap faults `cast_float_overflow_trap`/`cast_float_nan_trap`)
// reach native/release but not checked (native/release each +1 more).
// The static-refusal litmus files (numlit_value_refused,
// numlit_float_to_int_refused, numlit_ambiguity_named) land as
// `rejected` and by design do not move coverage. No pre-existing file
// moved lanes — Group A's zero-churn proof at the coverage level.
// Totals 187/203/203, union 217, all-three 173. Counts measured by this
// gate, not predicted.
//
// s114 ratchet over 348 entries: signal RECEPTION lands (c30, #126). Two
// new run-phase witnesses. `signal_loopback.lu` — the deterministic
// sequential loopback (listen→raise→wait) — runs on ALL three lanes:
// the checked machine models signals as a pure in-machine queue, the
// native/release lanes deliver the real SIGHUP through the reactor's
// task layer (checked/native/release each +1, all-three +1, union +1).
// `signal_supervisor.lu` — the wws shape, a parked supervisor woken by
// a sibling's raise — reaches native/release but the checked lane
// refuses it BY NAME (`spawn` is structured concurrency, C1-deferred),
// so native/release each +1 more (union +1). No pre-existing file moved
// lanes. Totals 188/205/205, union 219, all-three 174. Counts measured
// by this gate, not predicted.
//
// s115 ratchet over 353 entries: c27's fifth small-debts pass. Five new
// run-phase witnesses, each three-lane, so every lane and both
// intersections move +5. `net/byte_roundtrip.lu` — the #137 binary
// round-trip (0xFF + embedded NUL + a split codepoint) over loopback,
// byte-equal on all three lanes. `faults/wrap_top_bit_as_int.lu` and
// `faults/wrap_high_as_i32.lu` — the D56/#135 out-of-range wrapping→int
// TRAP at both widths, `typecheck/wrap_as_int_in_range.lu` — its
// in-range converting twin. `rows/iter_diverging_else_bound.lu` — the
// #139 witness: a List bound through a diverging `else` iterates, and
// the reject path propagates (the checked-tier `return <row>` fix). No
// pre-existing file moved lanes (full corpus 0-bad). Totals
// 193/210/210, union 224, all-three 179. Counts measured by this gate.
//
// s116 ratchet over 356 entries: c27's sixth small-debts pass. Three
// new run-phase witnesses, each three-lane, so every count moves +3.
// `generics/list_imported_elem/` — the #140 program:
// `List[geo.Point]()` with an IMPORTED element type, constructed,
// pushed, and read back across the module boundary (the refusal never
// lived in monomorphization — sema's bracket-arg type reading only
// knew a bare ident, so the `mod.Type` member shape fell through to
// the generic-data refusal). `generics/list_struct_elem.lu` — its
// LOCAL twin, the regression pin: it ran before the fix and the
// per-file sweep shows it verdict-stable. `net/line_reader_bytes.lu`
// — #46's buffered fill riding s115's byte path: a read boundary
// inside `é`, buffered as bytes, decoded at the protocol layer (zero
// compiler change; s115 made it free). The full per-file three-lane
// sweep shows exactly ONE pre-existing file moving: the #140 witness
// itself, refused@resolve → run on all three. Totals 196/213/213,
// union 227, all-three 182. Counts measured by this gate, not
// predicted.
//
// s117 ratchet over 358 entries: the shim travels with its spawner
// (#136, c31's first clustering-correctness sprint). Two new
// run-phase witnesses, both native+release only (the checked machine
// refuses spawn AND closures by name — C1-deferred / borrow-only
// closures, the s114 precedent): `conc/spawn_cluster_split.lu` — the
// wolf-wws parked-forwarder shape reduced, a spawner whose program
// partitions into two clusters; at base the release tier refused
// `func.addr of @main.task0.entry outside this object's subset`
// while native ran it, and the v3 summary's `refs=` edge (a
// `func.addr` reference is reachability the call graph cannot see)
// fuses the shim into its spawner's cluster cap-exempt.
// `memory/closure_cluster_split.lu` — the same class one constructor
// over (s105): `@main.cls0` split from `main` refused identically at
// base, covered by the same edge by construction, never by a spawn
// special case. Native/release each +2, union +2; checked and
// all-three unchanged — the deltas are exactly the two new files, no
// pre-existing file moved lanes. Totals 196/215/215, union 229,
// all-three 182. Counts measured by this gate, not predicted.
//
// s118 ratchet over 362 entries: the OS random source lands (c30's
// second rung, #143). Three new run-phase witnesses, each THREE-lane —
// every lane makes the real platform call (the checked machine is a
// host process; `[os.random.checked]`), so every count moves +3:
// `os/random_differs.lu` — two 32-byte draws differ (the weakest
// honest property a witness can pin without becoming a statistical
// instrument; the draws compare to each other, never to a pinned
// byte). `os/random_edges.lu` — length 0 is the empty list, 64 KiB
// comes back complete and in-range (the fill loop owns the short-read
// boundary). `os/random_negative_trap.lu` — n < 0 is trap(assert) on
// every lane ([os.random.fill]; the OS-failure trap of
// [os.random.trap] rides the same nonzero-rc branch). The comptime
// refusal witness (`comptime/sandbox_os_random.lu`, E0701) lands as
// `rejected` and by design does not move coverage. No pre-existing
// file moved lanes — the deltas are exactly the three new files.
// Totals 199/218/218, union 232, all-three 185. Counts measured by
// this gate, not predicted.
// s119 ratchet over 362 entries: the loop and the layout (c32's first
// codegen-debts pass, #142/#144 — both found by real TLS code). Four
// new run-phase witnesses, each three-lane, so every count moves +4.
// `memory/carried_quotient_pair.lu` / `carried_quotient_nested.lu` —
// the #142 shapes (a floordiv quotient carried across sequential and
// nested index-write loops, TweetNaCl `modL`): at base the release
// mid-end ICEd on them (versioning against a stale CFG missed the
// second loop's loop-closed live-out routing; the verifier's
// edge-located token rule is the other half).
// `memory/list_session_struct.lu` / `list_mixed_width_struct.lu` —
// the #144 shapes (a `bool`/`i32` beside wider fields in a `List`
// element): at base the native tier refused the non-tiling packed
// stride; the stride now rounds up to the element's alignment. The
// full per-file three-lane sweep diff is exactly the four new files —
// no pre-existing file moved lanes. Totals 200/219/219, union 233,
// all-three 186. Counts measured by this gate, not predicted.
// merge ratchet (s118 + s119, over 366 entries): the two lanes
// ratcheted off the same base over DISJOINT movers — s118's three
// os/random witnesses and s119's four codegen witnesses, all
// three-lane — so the merged floors are the sum of both deltas
// (+3 and +4 on every count). Measured by this gate on the merged
// tree, not predicted.
// s120 ratchet over 368 entries: the boundary primitive completes
// (c33-strings' first sprint, #17). Two new run-phase witnesses, each
// three-lane, so every count moves +2. `strings/chars_walk.lu` —
// `chars()` yields the Unicode scalars and the width walk
// reconstructs byte offsets that land on exactly the boundaries `get`
// accepts ([mem.str.chars]; lupin 0.1.13 refuses `chars()` by name,
// so the file is wolfc-lane evidence). `strings/boundary_battery.lu`
// — the 2-byte/3-byte/4-byte battery (é/中/🐺): lead vs continuation
// bytes through the byte view, whole-char `get` hits, and every
// mid-code-point slice a `{none}` refusal — this one runs under lupin
// too and agrees. No pre-existing file moved lanes. Totals
// 205/224/224, union 238, all-three 191. Counts measured by this
// gate, not predicted.
// s121 ratchet over 374 entries: the scalar gets a type (c33-strings'
// second sprint, D58). Six new run-phase witnesses, each three-lane,
// so every count moves +6; the migrated `strings/chars_walk.lu`
// (`chars()` now yields `List[char]`) stays three-lane, so it moves
// nothing. `strings/char_battery.lu` — literals at all four UTF-8
// widths, escape spellings, both casts, and the domain's legal edges
// (0, 0xD7FF, 0xE000, 0x10FFFF). `strings/char_order.lu` — scalar
// order and equality plus match-over-char dispatch.
// `strings/char_interp.lu` — `{c}` prints the character (stdout
// pinned; a spec takes the str surface). `faults/char_cast_
// surrogate_trap.lu` / `_range_trap.lu` / `_negative_trap.lu` — the
// three `int as char` refusals by name, trap(overflow) on every lane
// (the surrogate gap is the D24-critical one). No pre-existing file
// moved lanes — the deltas are exactly the six new files. Totals
// 211/230/230, union 244, all-three 197. Counts measured by this
// gate, not predicted.
// s123 ratchet over 377 entries: the compiler does not panic (#151).
// Three new run-phase witnesses, every one a shape that CRASHED the
// compiler before this sprint. `strings/match_str_const_scrutinee.lu`
// — `match` over str literals with an interned-literal scrutinee, the
// build-time-decided candidates (the Braun `use_var` panic and its
// unreachable-block siblings). `typecheck/match_guard_const.lu` — the
// guard that decides at build time; the checked executor still
// refuses match guards by name ("this expression shape in checked
// execution"), so checked moves +2 while native/release move +3.
// `typecheck/match_chain_reuse.lu` — a chain-block constant
// re-mentioned after the match (the [dominance] GVN-leak neighbour);
// three-lane. Totals 213/233/233, union 247, all-three 199. Counts
// measured by this gate, not predicted.
// s123 second ratchet over 383 entries: E0415 and the #152 pinning.
// Five more run-phase witnesses. `typecheck/numlit_extremes.lu` — the
// legal signed extremes, i64::MIN through the direct `-<literal>`
// spelling every tier now decodes in one step; three-lane.
// `typecheck/numlit_u64_edge.lu` — `u64::MAX` runs on the native
// tiers; the checked executor's i64 value model refuses the spelling
// by name, so checked moves +4 while native/release move +5.
// `typecheck/numlit_list_element_width.lu` and the two
// `faults/overflow_list_pop_*` twins — the #152 program pinned
// correct beside the traps at both widths; all three-lane
// (trap(overflow) counts as an executing verdict). Totals
// 217/238/238, union 252, all-three 203. Counts measured by this
// gate, not predicted.
// s124 ratchet over 380 entries: the module explains itself (D59).
// Four new run-phase witnesses, each three-lane, so every count moves
// +4. `resolve/bare_sibling/pair.lu` — a directiveless sibling is a
// member and its fn is in scope (#149 probe 1). `resolve/plain_subdir/
// main.lu` — a subdirectory module with no `member: true` resolves
// (#145 both ways; the marked spelling is `resolve/two_mod/`).
// `resolve/standalone_pair/left.lu`/`right.lu` — two standalone mains
// coexist in one directory (`member: false` and the entry pair). The
// two fail-phase witnesses (`dup_bare`, `broken_sibling`) join the
// rejection ledger instead. Totals 215/234/234, union 248, all-three
// 201. Counts measured by this gate, not predicted.
// merge (s123 + s124, 2026-08-28): disjoint witness sets (eight
// crash/literal witnesses; four module-formation witnesses), so the
// deltas compound. Counts measured by this gate on the merged tree,
// not predicted.
// Floors are PER-PLATFORM and MEASURED, never inherited (s59): the
// linux numbers are linux measurements and stay untouched; each newly
// ported host ratchets from its own first measurement.
#[cfg(not(target_os = "macos"))]
const LANE_FLOORS: &[(&str, usize)] = &[("checked", 221), ("native", 242), ("release", 242)];
#[cfg(not(target_os = "macos"))]
const UNION_FLOOR: usize = 256;
#[cfg(not(target_os = "macos"))]
const ALL_THREE_FLOOR: usize = 207;
// s59, measured on macOS/aarch64 the day the gate lifted: checked and
// native at FULL linux parity (221/242, union 256 — the port left no
// coverage behind), release honestly 0 (the s41 tier refused this
// host by name until its own c13 sprint).
// s127, measured the day the RELEASE gate lifted: release 242 and
// all-three 207 — full linux parity; the release tier executes every
// corpus entry here that it executes there. Counts measured by this
// gate on this host, not predicted and not inherited.
// s126 ratchet over 397 entries, RE-MEASURED on macOS/aarch64 on the
// tree that carries s127's release tier (the branch's own measurement
// predated it; a rebase re-measures, never inherits): the index
// chooses its origin (D61). Six new run-phase witnesses.
// `grammar/index_origin_file.lu` — the file-wide marker, every
// `[gram.expr.index.origin]` table row, stdout pinned; three-lane.
// `grammar/index_origin_scopes.lu` — the statement form, nested
// restore, innermost-wins, interpolation; three-lane.
// `grammar/index_origin_closure.lu` — the lexical-scope closure
// witness; the checked executor still refuses capturing closures by
// name, so checked moves +5 while native/release move +6.
// `faults/index_origin_zero.lu` — `xs[0]` under `index(1)` traps
// bounds BY THE SHIFT; three-lane. `faults/index_origin_min_
// overflow.lu` — the checked shift's int.min corner traps overflow;
// three-lane. `lints/index_origin_get.lu` — `.get` is origin-free,
// W0317 pinned; three-lane. The two fail-phase witnesses
// (`index_origin_misplaced` E0211, `index_origin_bad` E0813) join
// the rejection ledger (115 → 117). No pre-existing file moved
// lanes. Totals 226/248/248, union 262, all-three 212. Counts
// measured by this gate, not predicted.
// s128 ratchet over 401 entries: comma-grouped binders (D63). One new
// run-phase witness — `grammar/let_group.lu`, the D63 group with a
// later binder reading an earlier one — runs on every lane, so every
// count moves +1 (a POPULATION change; no pre-existing file moved
// lanes). The two refusal teach-notes (`let_group_one_init`,
// `let_group_bare_tuple`) join the rejection ledger, and
// `let_group_destructure` waits at `mem` for #173's landing. Totals
// 227/249/249, union 263, all-three 213. Counts measured by this
// gate, not predicted.
// s128 item 2 ratchet over 404 entries: destructuring bindings land
// (#173). Two new run witnesses (`memory/destructure_bind.lu`,
// `memory/destructure_partial_live.lu`) run three-lane, and
// `grammar/let_group_destructure.lu` — parked at `mem` by item 1 —
// now runs on every lane too, so every count moves +3 (one parked
// file moved lanes, two are population). The partial-move fault twin
// (`destructure_partial_move`, E1001) joins the rejection ledger.
// Totals 230/252/252, union 266, all-three 216. Counts measured by
// this gate, not predicted.
// s128 item 3 ratchet over 408 entries: D62 lands (#172) — `+`/`+=`
// on two strs is interpolation-append. One new run witness
// (`strings/concat_plus.lu`: the legal chain, `+=` in every spelling
// the program needs) runs three-lane, so every count moves +1; the
// three mix pins (`concat_mix_int`/`concat_mix_char`/`concat_int_str`,
// E0409) join the rejection ledger. Totals 231/253/253, union 267,
// all-three 217. Counts measured by this gate, not predicted.
// s128 item 4 ratchet over 412 entries: List slicing lands (#171,
// `[mem.list.slice]`). Four new run-phase witnesses, each three-lane
// — `memory/list_slice.lu` (open/`^n`/closed forms + `for` over a
// slice value), `memory/list_slice_edges.lu` (the empty edges and
// double-`^n`), and the two `faults/` bounds twins (oob, reversed —
// traps are runs) — so every count moves +4. Totals 235/257/257,
// union 271, all-three 221. Counts measured by this gate, not
// predicted.
#[cfg(target_os = "macos")]
const LANE_FLOORS: &[(&str, usize)] = &[("checked", 235), ("native", 257), ("release", 257)];
#[cfg(target_os = "macos")]
const UNION_FLOOR: usize = 271;
#[cfg(target_os = "macos")]
const ALL_THREE_FLOOR: usize = 221;

/// One lane's observation of one corpus entry.
struct LaneObs {
    verdict: String,
    phase: String,
    /// The lane's own words for a refusal, lifted off stderr. Derived
    /// every run from the compiler that produced it, so the residue's
    /// reasons cannot drift out of date the way a hand-kept list would.
    reason: Option<String>,
}

/// Why one lane observation did not produce a record.
enum LaneStop {
    /// Exit 2: the environment cannot drive this lane (no cc/clang,
    /// no libwolf_rt.a) — the caller skips LOUDLY rather than
    /// reporting a lane that never ran as a lane that covers nothing.
    Environment(String),
    /// Any other failure — a crash or a malformed record is a
    /// regression, never a skip. The exit-code asymmetry is the
    /// point (s59): exit 2 alone means "could not run"; everything
    /// else means "ran and broke", and a ported host must never be
    /// silently green through the wrong branch.
    Broken(String),
}

/// Run `conform-run` on one file in one lane.
fn lane_observe(wolf: &Path, file: &Path, flag: &str) -> Result<LaneObs, LaneStop> {
    let mut cmd = Command::new(wolf);
    cmd.arg("conform-run").arg(file).arg("--json");
    if !flag.is_empty() {
        cmd.arg(flag);
    }
    let out = cmd
        .output()
        .map_err(|e| LaneStop::Broken(format!("spawn wolf: {e}")))?;
    if out.status.code() == Some(2) {
        return Err(LaneStop::Environment(format!(
            "environment cannot drive `{flag}`: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    if !out.status.success() {
        return Err(LaneStop::Broken(format!(
            "conform-run {flag} failed on {}: {}",
            file.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| LaneStop::Broken(format!("bad record: {e}")))?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    let reason = stderr
        .lines()
        .find_map(|l| l.split_once("unsupported — "))
        .map(|(_, why)| why.split_whitespace().collect::<Vec<_>>().join(" "));
    Ok(LaneObs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        phase: rec["phase_reached"].as_str().unwrap_or("none").to_string(),
        reason,
    })
}

/// Why no lane executed an entry — the residue, classified. The classes
/// are computed from the records on every run, never declared in a list
/// somebody has to remember to update: that is the whole reason the
/// number cannot rot into folklore between differentials.
///
/// `forward` is the one class a header participates in (s91): a file
/// whose `check:` pins a rejection the compiler cannot make yet is
/// refused, not rejected, and its own header says which construct is
/// missing. The declaration cannot rot either — `xtask corpus` fails if
/// the marker outlives the refusal it describes — and without it the
/// entry sits in `refused@…` beside genuine scope gaps while every
/// count downstream reads its `fail(…)` pin as an enforced rule.
fn residue_class(obs: &BTreeMap<&str, LaneObs>, forward: bool) -> String {
    let lanes: Vec<&LaneObs> = COMPARED_LANES.iter().filter_map(|l| obs.get(l)).collect();
    if forward && lanes.iter().all(|o| o.verdict == "unsupported") {
        return "forward".to_string();
    }
    if lanes.iter().all(|o| o.verdict.starts_with("fail(")) {
        // Rejected by every lane: a NEGATIVE entry. It IS compared — at
        // its rejection rung, under [proto.cmp.rung] — so it is not a
        // hole in the differential, only outside the run rung. Counting
        // these as a coverage failure would be the same error in the
        // other direction: treating a working comparison as a gap.
        "rejected".to_string()
    } else if lanes.iter().all(|o| o.verdict == "unsupported") {
        // Declined by every lane: the real scope gap, and the number
        // this sprint exists to drive down. The rung it stopped at is
        // the coarse machine-readable reason, and it is always present
        // even where the lane printed no prose (the typecheck rung
        // declines silently today — the per-lane `reason` is null there
        // and the rung is what carries the meaning).
        let deepest = lanes
            .iter()
            .map(|o| o.phase.as_str())
            .max_by_key(|p| corpus::phase_rank(p).unwrap_or(0))
            .unwrap_or("none");
        format!("refused@{deepest}")
    } else {
        // Lanes disagreeing about whether the program is even admissible
        // — one rejects it, another declines it. Never silent.
        "mixed".to_string()
    }
}

/// `cargo xtask lane-coverage [--json]` — publish what the differential
/// actually covers (`[proto.cmp.coverage]`; wolf-lang#90).
///
/// lupin 0.1.11 measured this from OUTSIDE the runner, on the grounds
/// that auditing a lane with itself is circular, and found the three
/// run-reaching lanes are not nested: 56 of the entries it executes are
/// met by no wolfgang lane at all. This command is that audit brought
/// in-tree and made a gate — same measurement, same definition of
/// "executed" (`protocol::covered_at_run`), run on every commit so the
/// figure cannot decay into folklore between differentials.
fn lane_coverage_cmd(args: &[String]) -> ExitCode {
    let json = args.iter().any(|a| a == "--json");
    if !run_ok(
        "cargo",
        &["build", "-p", "wolf_driver", "-p", "wolf_rt", "--quiet"],
    ) {
        eprintln!("lane-coverage: failed to build wolf + libwolf_rt.a");
        return ExitCode::FAILURE;
    }
    let wolf = PathBuf::from("target/debug/wolf");
    let mut files = Vec::new();
    collect_wolf_files(Path::new("corpus"), &mut files);
    files.sort();
    // Members compile through their module's entry file (s12), so they
    // are not entries and must not sit in the denominator. The same
    // pass collects the forward pins (s91) — entries whose `check:`
    // records an intention rather than a rule the compiler enforces.
    let mut forward: BTreeSet<String> = BTreeSet::new();
    files.retain(|f| {
        let Some(d) = std::fs::read_to_string(f)
            .ok()
            .and_then(|s| corpus::parse_directives(&s).ok())
        else {
            return true;
        };
        if d.forward.is_some() {
            forward.insert(f.display().to_string());
        }
        !d.is_member()
    });

    let mut cov = xtask::protocol::Coverage::default();
    let mut per_file: BTreeMap<String, BTreeMap<&str, LaneObs>> = BTreeMap::new();
    for f in &files {
        let key = f.display().to_string();
        for (lane, flag) in RUN_LANES {
            match lane_observe(&wolf, f, flag) {
                Ok(obs) => {
                    let rec = serde_json::json!({
                        "phase_reached": obs.phase, "verdict": obs.verdict,
                    });
                    cov.observe(lane, &key, &rec);
                    per_file.entry(key.clone()).or_default().insert(lane, obs);
                }
                Err(LaneStop::Environment(e)) => {
                    // Loud skip, never a silent green: a lane that could
                    // not run is not a lane that covers nothing.
                    eprintln!("lane-coverage: SKIP — {e}");
                    return ExitCode::SUCCESS;
                }
                Err(LaneStop::Broken(e)) => {
                    // A lane that RAN and broke is a regression, not a
                    // skip (the s59 exit-code asymmetry).
                    eprintln!("lane-coverage: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
    }

    let union = cov.union(COMPARED_LANES);
    let all_three = cov.intersection(COMPARED_LANES);
    let uncovered = cov.uncovered(COMPARED_LANES);
    eprintln!(
        "lane-coverage: {} non-member corpus entries ([proto.cmp.coverage])",
        cov.entries()
    );
    for (lane, _) in RUN_LANES {
        let n = cov.lane(lane);
        let holes = cov.holes(lane, COMPARED_LANES).len();
        eprintln!(
            "lane-coverage:   {lane:<8} executes {n:>3} at run{}",
            if COMPARED_LANES.contains(lane) {
                format!("  ({holes} the other lanes reach and it does not)")
            } else {
                String::new()
            }
        );
    }
    eprintln!(
        "lane-coverage:   UNION {} of {} — all three {}, so the lanes are {}nested",
        union.len(),
        cov.entries(),
        all_three.len(),
        if union.len() == all_three.len() {
            ""
        } else {
            "NOT "
        }
    );

    // The residue, by class. `rejected` entries are compared at their
    // rejection rung and are NOT counted as a coverage failure; the
    // `refused` ones are the honest gap; `forward` entries pin a
    // rejection that does not exist yet and are neither.
    let mut by_class: BTreeMap<String, Vec<&String>> = BTreeMap::new();
    for f in &uncovered {
        let Some(obs) = per_file.get(f) else { continue };
        by_class
            .entry(residue_class(obs, forward.contains(f.as_str())))
            .or_default()
            .push(f);
    }
    for (class, fs) in &by_class {
        eprintln!("lane-coverage:   residue `{class}`: {}", fs.len());
        if class == "forward" {
            // Named, not just counted: the whole point of the class is
            // that a reader can see which file is an intention.
            for f in fs {
                eprintln!("lane-coverage:     forward pin: {f}");
            }
        }
    }
    // Both numbers, never one corrected number. `rejected` is what
    // every compared lane really refuses to compile — the rules; a
    // forward pin looks identical from outside the compiler (a
    // `fail(CODE)` header no lane executes) and is not one, so folding
    // the two together miscounts in the direction that flatters us.
    eprintln!(
        "lane-coverage:   static rejections: {} rule(s) every lane enforces, {} forward pin(s) \
         no lane can make yet",
        by_class.get("rejected").map_or(0, Vec::len),
        by_class.get("forward").map_or(0, Vec::len),
    );
    if json {
        for f in &uncovered {
            let Some(obs) = per_file.get(f) else { continue };
            let lanes: serde_json::Map<String, serde_json::Value> = COMPARED_LANES
                .iter()
                .filter_map(|l| {
                    obs.get(l).map(|o| {
                        (
                            (*l).to_string(),
                            serde_json::json!({
                                "verdict": o.verdict,
                                "phase_reached": o.phase,
                                "reason": o.reason,
                            }),
                        )
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::json!({
                    "file": f,
                    "class": residue_class(obs, forward.contains(f.as_str())),
                    "forward": forward.contains(f.as_str()),
                    "lanes": lanes,
                })
            );
        }
    }

    // The ratchet. Coverage may rise and may not fall.
    let mut fell = false;
    for (lane, floor) in LANE_FLOORS {
        let n = cov.lane(lane);
        if n < *floor {
            eprintln!(
                "lane-coverage: the `{lane}` lane executes {n} entries, below its floor of {floor} \
                 — a lane stopped running programs it used to run; fix the refusal or lower the \
                 floor deliberately, with the reason"
            );
            fell = true;
        }
    }
    if union.len() < UNION_FLOOR {
        eprintln!(
            "lane-coverage: union coverage {} is below the floor of {UNION_FLOOR} — the \
             differential sees less of the corpus than it did",
            union.len()
        );
        fell = true;
    }
    let all_three_holds = all_three.len() >= ALL_THREE_FLOOR;
    if !all_three_holds {
        eprintln!(
            "lane-coverage: all-three coverage {} is below the floor of {ALL_THREE_FLOOR} — the \
             lanes are diverging in scope, not converging",
            all_three.len()
        );
        fell = true;
    }
    if fell {
        return ExitCode::FAILURE;
    }
    eprintln!(
        "lane-coverage: floors held (checked/native/release/union/all-three \
         ≥ {}/{}/{}/{UNION_FLOOR}/{ALL_THREE_FLOOR})",
        LANE_FLOORS[0].1, LANE_FLOORS[1].1, LANE_FLOORS[2].1,
    );
    ExitCode::SUCCESS
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
    if args.first().map(String::as_str) == Some("ritual") {
        return ritual::ritual(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("gate") {
        let Some(path) = args.get(1) else {
            eprintln!("bench gate: need <t1.jsonl>");
            return ExitCode::from(2);
        };
        return bench_t1::gate(path);
    }
    let mut track = None;
    let mut runs: u32 = 10;
    let mut out_path: Option<PathBuf> = None;
    let mut kernels: Option<String> = None;
    for a in args {
        if let Some(v) = a.strip_prefix("--track=") {
            track = Some(v.to_string());
        } else if let Some(v) = a.strip_prefix("--runs=") {
            runs = v.parse().expect("--runs=N");
        } else if let Some(v) = a.strip_prefix("--out=") {
            out_path = Some(PathBuf::from(v));
        } else if let Some(v) = a.strip_prefix("--kernels=") {
            kernels = Some(v.to_string());
        } else {
            eprintln!("bench: unknown argument `{a}`");
            return ExitCode::from(2);
        }
    }
    let commit = git_short_sha();
    let records = match track.as_deref() {
        Some("runtime") => bench_runtime(runs, &commit),
        Some("compile") => bench_compile(runs.min(3), &commit),
        // s44: the T1 micro suite (the M2 gate) and issue #70's two
        // IR-volume metrics. Separate tracks, because they answer
        // separate questions and the nightly lane schedules them apart.
        Some("t1") => bench_t1::run(runs.max(3), kernels.as_deref(), &commit),
        Some("irvolume") => bench_t1::irvolume(&commit),
        _ => {
            eprintln!("bench: --track=<runtime|compile|t1|irvolume> is required");
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
/// The WIDE bench project (s43): 8 modules × 8 functions whose bodies
/// are deliberately past the inliner's largest budget, so the
/// whole-program phase produces MANY clusters and the parallel codegen
/// lane has something real to measure. The narrow `nmod` chain stays
/// exactly as it was (its baselines must remain comparable) — this is
/// a second project, not a redefinition of the first.
fn bench_wide_project(proj: &Path) {
    const MODS: usize = 8;
    const FNS: usize = 8;
    const STMTS: usize = 120; // > every inline budget in the table
    let _ = std::fs::remove_dir_all(proj);
    std::fs::create_dir_all(proj).expect("mkdir wide project");
    for k in 1..=MODS {
        let dir = proj.join(format!("w{k:02}"));
        std::fs::create_dir_all(&dir).expect("mkdir module");
        let mut src = String::from("//! member: true\n");
        for j in 1..=FNS {
            src.push_str(&format!(
                "\n/// Wide body {k}.{j}.\npub fn h{k:02}{j:02}(x: int) -> int {{\n    let v0 = x + {j}\n"
            ));
            for s in 1..=STMTS {
                let op = match s % 4 {
                    0 => "+",
                    1 => "*",
                    2 => "-",
                    _ => "+",
                };
                src.push_str(&format!("    let v{s} = v{} {op} {}\n", s - 1, (s % 7) + 1));
            }
            src.push_str(&format!("    v{STMTS}\n}}\n"));
        }
        // One aggregator per module: keeps every body reachable
        // (dead-function elimination would otherwise take them).
        src.push_str("\n/// Aggregate.\npub fn agg(x: int) -> int {\n    ");
        let calls: Vec<String> = (1..=FNS).map(|j| format!("h{k:02}{j:02}(x)")).collect();
        src.push_str(&calls.join(" + "));
        src.push_str("\n}\n");
        std::fs::write(dir.join(format!("w{k:02}.lu")), src).expect("write wide module");
    }
    let uses: String = (1..=MODS).map(|k| format!("use w{k:02}\n")).collect();
    let sum: Vec<String> = (1..=MODS).map(|k| format!("w{k:02}.agg(1)")).collect();
    std::fs::write(
        proj.join("main.lu"),
        format!(
            "{uses}\nfn main() -> !int {{\n    if {} > 0 {{ 0 }} else {{ 1 }}\n}}\n",
            sum.join(" + ")
        ),
    )
    .expect("write wide main");
}

/// Scrape the whole-program counters off one `--release
/// --codegen-report` build (s43): cross-module inlines, cross-cluster
/// inlines, imports, clusters, and the D8 dedup ratio. Deterministic
/// counts, not timings — they say what the optimizer DID, so a silent
/// loss of cross-module optimization shows up as a number, not as a
/// mystery in a wall clock.
fn whole_program_counts(
    wolf: &Path,
    entry: &Path,
    prog: &Path,
) -> Option<Vec<(&'static str, f64, &'static str)>> {
    let out = Command::new(wolf)
        .arg("build")
        .arg(entry)
        .arg("-o")
        .arg(prog)
        .arg("--release")
        .arg("--no-cache")
        .arg("--codegen-report")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stderr).into_owned();
    // "  whole-program: N module(s) -> N cluster(s), N import(s); N
    //   cross-module inline(s), N cross-cluster"
    let wp = text.lines().find(|l| l.contains("whole-program:"))?;
    let dd = text.lines().find(|l| l.contains("dedup:"))?;
    // Label-anchored, never positional: the number immediately before
    // the label word. A stats-line reword breaks the lane loudly (the
    // metric vanishes) instead of silently reporting the wrong field.
    let before = |line: &str, label: &str| -> Option<f64> {
        let idx = line.find(label)?;
        line[..idx].split_whitespace().last()?.parse().ok()
    };
    let mut v: Vec<(&'static str, f64, &'static str)> = Vec::new();
    for (metric, label, line) in [
        ("clusters", "cluster(s)", wp),
        ("cluster_imports", "import(s)", wp),
        ("cross_module_inlines", "cross-module inline(s)", wp),
        ("cross_cluster_inlines", "cross-cluster", wp),
        ("dedup_bodies_seen", "bodies ->", dd),
        ("dedup_bodies_unique", "unique", dd),
    ] {
        if let Some(value) = before(line, label) {
            v.push((metric, value, "count"));
        }
    }
    // The tracked D8 health metric: instantiations to unique bodies.
    if let (Some(seen), Some(unique)) = (before(dd, "bodies ->"), before(dd, "unique"))
        && unique > 0.0
    {
        v.push(("dedup_ratio", seen / unique, "ratio"));
    }
    (!v.is_empty()).then_some(v)
}

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
        // (b') the Tier-R compile lane (s41, REPORT-ONLY until s44
        // sets gates): clean release builds through the LLVM tier.
        // Skips loudly where the release toolchain (clang) is absent.
        if wolf_build(&["--release"]) {
            for _ in 0..runs {
                let _ = std::fs::remove_dir_all(proj.join(".lu-cache"));
                let t = Instant::now();
                if !wolf_build(&["--release"]) {
                    return None;
                }
                records.push(record(
                    "nmod",
                    "compile",
                    "wolf",
                    "release_clean_build_wall_s",
                    t.elapsed().as_secs_f64(),
                    "s",
                    commit,
                    config,
                ));
            }
            // (b'') the s43 whole-program lane. Two halves:
            //
            // - PARALLEL SCALING: the same clean release build at one
            //   worker and at eight. Clusters lower independently, so
            //   the ratio is the codegen phase's scaling (contract
            //   target 4). Report-only until s44's variance work; the
            //   BUILDS themselves stay byte-identical either way,
            //   which the driver's `release_builds_are_reproducible`
            //   test gates.
            // - WHOLE-PROGRAM COUNTS: cross-module inlines, imports,
            //   clusters, and the D8 instantiations-to-unique-bodies
            //   ratio — the health metrics the contract tracks forever.
            let release_1t = |threads: &str| -> Option<f64> {
                let _ = std::fs::remove_dir_all(proj.join(".lu-cache"));
                let t = Instant::now();
                let ok = Command::new(wolf)
                    .arg("build")
                    .arg(&entry)
                    .arg("-o")
                    .arg(&prog)
                    .arg("--release")
                    .env("RAYON_NUM_THREADS", threads)
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                ok.then(|| t.elapsed().as_secs_f64())
            };
            for _ in 0..runs {
                if let (Some(one), Some(eight)) = (release_1t("1"), release_1t("8")) {
                    records.push(record(
                        "nmod",
                        "compile",
                        "wolf",
                        "release_clean_build_1t_wall_s",
                        one,
                        "s",
                        commit,
                        config,
                    ));
                    records.push(record(
                        "nmod",
                        "compile",
                        "wolf",
                        "release_clean_build_8t_wall_s",
                        eight,
                        "s",
                        commit,
                        config,
                    ));
                }
            }
            if let Some(counts) = whole_program_counts(wolf, &entry, &prog) {
                for (metric, value, unit) in counts {
                    records.push(record(
                        "nmod", "compile", "wolf", metric, value, unit, commit, config,
                    ));
                }
            }
            // (b''') the WIDE project: enough WIR volume to partition,
            // so the parallel codegen lane measures clusters lowering
            // side by side rather than one cluster twice.
            let wide = Path::new("target/bench-wide");
            bench_wide_project(wide);
            let wentry = wide.join("main.lu");
            let wprog = wide.join("prog");
            // Returns (total wall, codegen-phase wall): the phase
            // number is what parallel scaling is ABOUT — the frontend
            // and mid-end ahead of it are serial, so a total-wall
            // ratio would understate the partition's effect.
            let wide_build = |threads: &str| -> Option<(f64, f64)> {
                let _ = std::fs::remove_dir_all(wide.join(".lu-cache"));
                let t = Instant::now();
                let out = Command::new(wolf)
                    .arg("build")
                    .arg(&wentry)
                    .arg("-o")
                    .arg(&wprog)
                    .arg("--release")
                    .arg("--verbose")
                    .env("RAYON_NUM_THREADS", threads)
                    .output()
                    .ok()?;
                if !out.status.success() {
                    return None;
                }
                let total = t.elapsed().as_secs_f64();
                let text = String::from_utf8_lossy(&out.stderr).into_owned();
                let phase = text
                    .lines()
                    .find_map(|l| l.split("unit(s) in ").nth(1))
                    .and_then(|s| s.trim_end_matches('s').parse::<f64>().ok())?;
                Some((total, phase))
            };
            if wide_build("8").is_some() {
                for _ in 0..runs {
                    if let (Some(one), Some(eight)) = (wide_build("1"), wide_build("8")) {
                        for (metric, value) in [
                            ("release_clean_build_1t_wall_s", one.0),
                            ("release_clean_build_8t_wall_s", eight.0),
                            ("release_codegen_1t_wall_s", one.1),
                            ("release_codegen_8t_wall_s", eight.1),
                        ] {
                            records.push(record(
                                "nmodwide", "compile", "wolf", metric, value, "s", commit, config,
                            ));
                        }
                    }
                }
                if let Some(counts) = whole_program_counts(wolf, &wentry, &wprog) {
                    for (metric, value, unit) in counts {
                        records.push(record(
                            "nmodwide", "compile", "wolf", metric, value, unit, commit, config,
                        ));
                    }
                }
            } else {
                eprintln!("bench: wide project did not build — parallel codegen lane skipped");
            }
        } else {
            eprintln!("bench: release tier unavailable (clang?) — Tier-R compile lane skipped");
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
    // (c-w) the s67 cost gate: one `conform-run` sweep of the corpus per
    // run — the clean-code diagnostic path, where the warning machinery
    // (allow scan + level application) rides every file. `bench diff
    // --gate` holds this within noise of its baseline per D5: warning
    // infrastructure must cost nothing when no warning fires.
    {
        let mut sweep_files = Vec::new();
        collect_wolf_files(Path::new("corpus"), &mut sweep_files);
        sweep_files.sort();
        sweep_files.retain(|f| {
            std::fs::read_to_string(f)
                .ok()
                .and_then(|src| corpus::parse_directives(&src).ok())
                .is_some_and(|d| !d.is_member())
        });
        for _ in 0..runs {
            let t = Instant::now();
            for f in &sweep_files {
                let _ = Command::new("target/debug/wolf")
                    .arg("conform-run")
                    .arg(f)
                    .arg("--json")
                    .output();
            }
            records.push(record(
                "corpus",
                "compile",
                "wolf",
                "corpus_conform_wall_s",
                t.elapsed().as_secs_f64(),
                "s",
                commit,
                config,
            ));
        }
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
                    // The D8 health metric (s93 + s94): instantiation
                    // requests, bodies lowered, and distinct content-
                    // hash classes post-mid-end — seen/unique is the
                    // dedup ratio. See bench/gates.json.
                    ("wir_instantiations_seen", "instantiations"),
                    ("wir_instantiations_lowered", "bodies"),
                    ("wir_instantiations_unique", "bodies"),
                    // s98: vtable demands vs distinct tables emitted —
                    // the same discipline applied to dyn dispatch data.
                    ("wir_vtables_demanded", "vtables"),
                    ("wir_vtables_unique", "vtables"),
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
                // Hardware-sensitive metrics (wall clock, RSS) swing
                // wildly across runner instances — the first gate trip
                // was max_rss +37.8% on an xtask-only commit, beside
                // wall times "improving" 60% (reports/10 §measurement:
                // no variance floor, no credible gate). They stay
                // REPORTED but do not gate until s44's methodology
                // lands; deterministic counters and rates keep gating.
                // Deterministic = same input, same number, any machine
                // (hit counts, memo rates). Everything wall-derived —
                // including per_sec throughput, which swung +78.7% then
                // -32.7% across two runs of sibling commits — is
                // hardware-sensitive and reports without gating.
                let deterministic = metric.ends_with("_hits") || metric.ends_with("hit_rate");
                let hw_sensitive = !deterministic;
                let dir = if regressed { "REGRESSED" } else { "improved" };
                let tag = if regressed && hw_sensitive {
                    " (report-only: hardware-sensitive, no variance floor yet)"
                } else {
                    ""
                };
                eprintln!("  {key}: {dir} {delta_pct:+.1}%{tag}");
                if regressed && !hw_sensitive {
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

// -------------------------------------------------------------- fmt-fuzz --

/// The formatter's idempotence hunt, stable-Rust and CI-runnable: a
/// corpus-seeded, SplitMix64-driven mutation loop over
/// `wolf_fmt`'s invariants (the loop itself is
/// `crates/wolf_fmt/examples/fmt_fuzz.rs` — xtask depends on no
/// workspace crate, so it drives the example as a subprocess).
///
/// Every fmt idempotence class found so far has been a comment-layout
/// class, and comments only appear in bulk in *real programs*: seeding
/// from the corpus rather than from bytes is why this loop out-finds
/// the nightly's libFuzzer lane by roughly an order of magnitude on the
/// same well.
///
/// `--ci` is the bounded, fixed-count slice `cargo xtask ci` runs; a
/// bare invocation defaults to a ten-second local sweep, and the
/// nightly hands it `--seconds=` in the hours. Every finding is
/// minimized and printed; a bare run exits nonzero on any of them,
/// while `--ci` fails only on the classes that must never bend (see
/// `--allow-open` in the loop's own docs).
fn fmt_fuzz(args: &[String]) -> ExitCode {
    if !run_ok(
        "cargo",
        &[
            "build",
            "--release",
            "-p",
            "wolf_fmt",
            "--example",
            "fmt_fuzz",
        ],
    ) {
        eprintln!("fmt-fuzz: failed to build the loop");
        return ExitCode::FAILURE;
    }
    let ci = args.iter().any(|a| a == "--ci");
    let mut pass: Vec<String> = args.iter().filter(|a| *a != "--ci").cloned().collect();
    if ci {
        // A fixed case count (not a clock) so the lane costs the same
        // on every machine, and `--allow-open` because the layout well
        // is not dry yet: CI holds the invariants that must never bend
        // — no panic, no lost comment, no changed tree, no fallback
        // that rewrote its input — and reports the convergence classes
        // still banked. The nightly runs the long sweep.
        pass.push("--cases=60000".to_string());
        pass.push("--seed=63".to_string());
        pass.push("--allow-open".to_string());
    }
    let ok = Command::new("target/release/examples/fmt_fuzz")
        .args(&pass)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        eprintln!("fmt-fuzz: invariants held");
        ExitCode::SUCCESS
    } else {
        eprintln!("fmt-fuzz: findings above — minimize, fix, and bank the fixture in");
        eprintln!(
            "          crates/wolf_fmt/tests/regressions/ (unfixed/ for a banked-but-open class)"
        );
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
        "08-package.md",
        "09-constant-time.md",
        "10-types.md",
        "11-os.md",
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

// ----------------------------------------------------------- doc-catalog --

/// The documentation generator's own catalog gate — `diag-catalog`'s
/// sibling (s53). `docs/api/` is the generated documentation of the s53
/// doc fixture, and it is a REVIEWED artifact: the format of a published
/// page, in the repository, diffable in a pull request. CI regenerates
/// it into memory and compares byte-for-byte, so a change to the
/// renderer that nobody reviewed cannot land.
///
/// `cargo xtask doc-catalog` rewrites it; `--check` verifies it.
fn doc_catalog(check: bool) -> ExitCode {
    let fixture = "crates/wolf_doc/fixtures/pkg";
    let out = "docs/api";
    if !Path::new(fixture).is_dir() {
        eprintln!("doc-catalog: the doc fixture is missing at {fixture}");
        return ExitCode::FAILURE;
    }
    let mut args = vec![
        "run",
        "-q",
        "-p",
        "wolf_driver",
        "--",
        "doc",
        fixture,
        "--out",
        out,
    ];
    if check {
        args.push("--check");
    }
    // The doc fixture's own coverage is deliberately incomplete (an
    // undocumented item and a doctest-less one), so the burn-down list
    // has something to print; `--require-docs` is therefore NOT passed
    // here. The gate this step enforces is byte-stability of the output.
    let status = Command::new("cargo").args(&args).status();
    match status {
        Ok(s) if s.success() => {
            eprintln!(
                "doc-catalog: docs/api is the generated documentation of {fixture}{}",
                if check {
                    " and is in sync"
                } else {
                    " (rewritten)"
                }
            );
            ExitCode::SUCCESS
        }
        Ok(_) => {
            eprintln!(
                "doc-catalog: docs/api OUT OF SYNC — run `cargo xtask doc-catalog` and \
                 review the diff (generated documentation is a reviewed artifact)"
            );
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("doc-catalog: cannot run the driver: {e}");
            ExitCode::FAILURE
        }
    }
}

// ---------------------------------------------------------- diag-catalog --

/// Generate docs/diagnostics.md from wolf_diag's registry and verify the
/// fixture rule: every registered code appears in ≥1 committed snapshot
/// (the s01 hook, armed for real in s10). `--check` verifies sync.
fn diag_catalog(check: bool) -> ExitCode {
    if !diag_voice() {
        return ExitCode::FAILURE;
    }
    let registry = std::fs::read_to_string("crates/wolf_diag/src/registry.rs")
        .expect("read wolf_diag registry");
    // Parse the rigid one-entry-per-code format:
    //   code!(E0101, "summary", r#" explanation "#);
    let mut entries: Vec<(String, String, String)> = Vec::new();
    let mut rest = registry.as_str();
    let mut consumed = 0usize;
    while let Some(pos) = rest.find("code!(") {
        let abs = consumed + pos;
        rest = &rest[pos + 6..];
        consumed = abs + 6;
        // skip the macro-definition template line (starts with `$`)
        if rest.trim_start().starts_with('$') {
            continue;
        }
        // A `code!(` inside a comment documents the format; it is not a
        // declaration. The module doc comment's example was harvested as
        // a real code and published `E0101 - one-line summary` into the
        // catalog, a page users read.
        let line_start = registry[..abs].rfind('\n').map(|i| i + 1).unwrap_or(0);
        if registry[line_start..abs].trim_start().starts_with("//") {
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
        consumed = registry.len() - after.len();
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
    // The corpus half of the gate (s91). Everything above runs one way
    // — registry to page — which is why a corpus file could pin a code
    // that existed nowhere and leave the gate green. A pinned code is a
    // dependency on the compiler's behaviour, so it must be a code the
    // compiler can emit, or a forward pin that admits it is not one.
    let documented: BTreeSet<String> = entries.iter().map(|(c, _, _)| c.clone()).collect();
    let mut corpus_files = Vec::new();
    collect_wolf_files(Path::new("corpus"), &mut corpus_files);
    corpus_files.sort();
    let mut pins: Vec<corpus::Pin> = Vec::new();
    for f in &corpus_files {
        let Ok(src) = std::fs::read_to_string(f) else {
            continue;
        };
        let Ok(d) = corpus::parse_directives(&src) else {
            continue; // `xtask corpus` owns the parse errors; do not double-report
        };
        for code in d.pinned_codes() {
            pins.push(corpus::Pin {
                code: code.to_string(),
                file: f.display().to_string(),
                forward: d.forward.clone(),
            });
        }
    }
    let (forward_pins, unbacked) = corpus::audit_pins(&pins, &documented);
    for pin in &unbacked {
        eprintln!(
            "diag-catalog: {}: pins `{}`, which no diagnostic emits and no catalog entry \
             describes — register the code, or mark the file `//! forward: <what is missing>` \
             if it pins behaviour that is not implemented yet",
            pin.file, pin.code,
        );
        bad += 1;
    }
    // Published, not merely tolerated: a code the corpus depends on and
    // the compiler cannot produce is something a reader of the catalog
    // is entitled to find there, under its own heading, saying so.
    if !forward_pins.is_empty() {
        body.push_str(
            "\n## Forward pins\n\nCodes the corpus pins that this compiler does not emit \
             yet. Each one is an intention recorded against a construct that is not \
             implemented — the behaviour we mean to have, not behaviour we enforce today. \
             They are not part of the count above.\n\n",
        );
        for pin in &forward_pins {
            body.push_str(&format!(
                "- `{}` — {} (not implemented: {})\n",
                pin.code,
                pin.file,
                pin.forward.as_deref().unwrap_or(""),
            ));
        }
    }
    if bad > 0 {
        return ExitCode::FAILURE;
    }
    // The philosophy page (c16): docs/warnings.md carries the severity
    // contract and escape-hatch etiquette as prose, then the full
    // W-catalog generated from the same registry — the audit hook the
    // v1 diagnostics-polish pass extends (its warning-posture review
    // walks exactly this page, and `--check` keeps it honest in CI).
    let mut warn_body = String::from(WARNINGS_PREAMBLE);
    let mut warn_count = 0usize;
    for (code, summary, explanation) in &entries {
        if !code.starts_with('W') {
            continue;
        }
        warn_count += 1;
        warn_body.push_str(&format!("\n## {code} — {summary}\n\n{explanation}\n"));
    }
    let out = Path::new("docs/diagnostics.md");
    let warn_out = Path::new("docs/warnings.md");
    if check {
        let on_disk = std::fs::read_to_string(out).unwrap_or_default();
        if on_disk != body {
            eprintln!(
                "diag-catalog: docs/diagnostics.md OUT OF SYNC — run `cargo xtask diag-catalog`"
            );
            return ExitCode::FAILURE;
        }
        let warn_disk = std::fs::read_to_string(warn_out).unwrap_or_default();
        if warn_disk != warn_body {
            eprintln!(
                "diag-catalog: docs/warnings.md OUT OF SYNC — run `cargo xtask diag-catalog`"
            );
            return ExitCode::FAILURE;
        }
        eprintln!(
            "diag-catalog: {} codes ({warn_count} warnings), all with fixtures, catalogs in sync; \
             the corpus pins {} of them, plus {} forward pin(s) on codes nothing emits yet",
            entries.len(),
            pins.iter()
                .map(|p| &p.code)
                .filter(|c| documented.contains(*c))
                .collect::<BTreeSet<_>>()
                .len(),
            forward_pins.len(),
        );
    } else {
        std::fs::create_dir_all("docs").expect("mkdir docs");
        std::fs::write(out, body).expect("write catalog");
        std::fs::write(warn_out, warn_body).expect("write warnings page");
        eprintln!(
            "diag-catalog: wrote docs/diagnostics.md ({} codes) and docs/warnings.md ({warn_count} warnings)",
            entries.len()
        );
    }
    ExitCode::SUCCESS
}

// ------------------------------------------------------------- voice ---

/// Call sites whose string arguments are read by a *user*: the
/// diagnostic engine's prose builders, and the honest-refusal templates
/// that print as "cannot compile this yet — {construct}".
const PROSE_SITES: &[&str] = &[
    ".with_note(",
    ".with_label(",
    ".with_help(",
    ".with_secondary(",
    ".with_suggestion(",
    "Diagnostic::error(",
    "Diagnostic::warning(",
    ".refuse(",
    "gap!(",
    "construct:",
];

/// Files whose every string literal ends up in a user's working tree:
/// the generated headers of files the tools write and the user commits.
const EMITTER_FILES: &[&str] = &["crates/wolf_pkg/src/lock.rs"];

/// `s68`, `c05`, `is04`: a lowercase marker (`s`, `c`, or `is`) followed
/// by exactly two digits, standing alone. Spec anchors
/// (`[mem.ub.defined]`) and D-numbered decisions are public vocabulary
/// and stay; so does anything with three digits or a letter attached.
fn internal_id(text: &str) -> Option<String> {
    let b = text.as_bytes();
    for i in 0..b.len() {
        let (mark, digits) = if b[i..].starts_with(b"is") {
            (2, &b[i + 2..])
        } else if b[i] == b's' || b[i] == b'c' {
            (1, &b[i + 1..])
        } else {
            continue;
        };
        let boundary_ok = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        if !boundary_ok || digits.len() < 2 {
            continue;
        }
        let two = digits[0].is_ascii_digit() && digits[1].is_ascii_digit();
        let closed = digits.len() == 2 || !(digits[2].is_ascii_alphanumeric() || digits[2] == b'_');
        if two && closed {
            return Some(text[i..i + mark + 2].to_string());
        }
    }
    None
}

/// Contents of the string literals starting at `i`, joined. `single`
/// takes the first literal only (for `construct:`-style fields);
/// otherwise literals are gathered until the call's parens rebalance,
/// which picks up `format!(…)` arguments and `\`-continued lines.
fn literal_run(b: &[u8], mut i: usize, single: bool) -> String {
    let mut depth = 1i32;
    let mut out: Vec<u8> = Vec::new();
    let limit = (i + 4096).min(b.len());
    while i < limit && depth > 0 {
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 2;
                    } else {
                        out.push(b[i]);
                        i += 1;
                    }
                }
                i += 1;
                if single {
                    break;
                }
                out.push(b' ');
                continue;
            }
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The voice gate: no internal identifier may reach a reader.
///
/// wolf_diag's own unit test covers `--explain` summaries and
/// explanations, but the leaks that actually shipped lived in the text
/// it cannot see — a note built in wolf_pkg, a refusal template in
/// wolf_sema, and the header the lockfile writer stamps into every
/// user's version control. This walks all three: the prose builders'
/// string arguments across every compiler crate, the generated catalog
/// pages, and every literal in the files that write user artifacts.
fn diag_voice() -> bool {
    let mut bad: Vec<String> = Vec::new();
    let mut checked = 0usize;

    let mut files = Vec::new();
    for krate in std::fs::read_dir("crates").expect("crates dir").flatten() {
        collect_rs_files(&krate.path().join("src"), &mut files);
    }
    files.sort();
    for f in &files {
        // The registry has its own test, and its prose is r#"…"# raw
        // strings this scanner deliberately does not read.
        if f.ends_with("registry.rs") {
            continue;
        }
        let body = std::fs::read_to_string(f).unwrap_or_default();
        // `#[cfg(test)]` prose is not read by users.
        let body = match body.find("\n#[cfg(test)]") {
            Some(i) => &body[..i],
            None => &body[..],
        };
        let emitter = EMITTER_FILES
            .iter()
            .any(|e| f.to_string_lossy().replace('\\', "/").ends_with(e));
        let b = body.as_bytes();
        for site in PROSE_SITES {
            let mut from = 0usize;
            while let Some(j) = body[from..].find(site) {
                let at = from + j;
                from = at + site.len();
                let text = literal_run(b, from, *site == "construct:");
                checked += 1;
                if let Some(id) = internal_id(&text) {
                    bad.push(format!(
                        "{}:{}: internal identifier `{id}` in reader-facing prose: {}",
                        f.display(),
                        body[..at].lines().count(),
                        text.trim().chars().take(90).collect::<String>()
                    ));
                }
            }
        }
        if emitter {
            // Every literal, not only the diagnostic ones: this file
            // writes a header into a file the user commits.
            let mut i = 0usize;
            while i < b.len() {
                if b[i] == b'"' {
                    let text = literal_run(b, i, true);
                    checked += 1;
                    if let Some(id) = internal_id(&text) {
                        bad.push(format!(
                            "{}:{}: internal identifier `{id}` in a generated header: {}",
                            f.display(),
                            body[..i].lines().count(),
                            text.trim().chars().take(90).collect::<String>()
                        ));
                    }
                    i += 1 + text.len() + 1;
                    continue;
                }
                i += 1;
            }
        }
    }

    // The generated pages are product too.
    for page in ["docs/diagnostics.md", "docs/warnings.md"] {
        let body = std::fs::read_to_string(page).unwrap_or_default();
        for (n, line) in body.lines().enumerate() {
            checked += 1;
            if let Some(id) = internal_id(line) {
                bad.push(format!(
                    "{page}:{}: internal identifier `{id}` on a catalog page: {}",
                    n + 1,
                    line.trim().chars().take(90).collect::<String>()
                ));
            }
        }
    }

    for b in &bad {
        eprintln!("diag-catalog: voice: {b}");
    }
    if bad.is_empty() {
        eprintln!("diag-catalog: voice: {checked} reader-facing strings carry no internal ids");
        true
    } else {
        eprintln!(
            "diag-catalog: voice: {} leak(s) — sprint and campaign ids are ours, not the reader's",
            bad.len()
        );
        false
    }
}

/// The hand-written half of docs/warnings.md (the generated W-catalog
/// follows it). Prose changes happen here, never in the output file.
const WARNINGS_PREAMBLE: &str = "\
# Wolf warnings — the philosophy, and the catalog

GENERATED by `cargo xtask diag-catalog` from
`crates/wolf_diag/src/registry.rs` — do not edit. The prose lives in
`xtask/src/main.rs`; the per-code entries are the registry's.

## The severity contract

An **error** rejects meaning: the program has none, so an error cannot
be allowed, denied, or configured away. A **warning** marks a program
that is legal and inadvisable — every W-code cites a concrete recorded
hazard or a house idiom rule, never \"might be slow someday\"
(spec/01 §9; D22). Warnings are mined from what actually bit people:
each entry's rationale names the wound, and a warning that stops
earning its keep is deleted rather than ignored. Idiom-arbiter codes
mechanize the written API conventions — \"idiomatic wolf\" is
checkable, not tribal.

## Levels, and who outranks whom

Every warning is leveled: `allow` drops it, `warn` reports it, `deny`
reports it at error severity and fails the build (the code stays
`W####`, and a note names the rule so the reader knows the rejection
is configuration, not semantics). Three sources set levels, most local
first:

1. **`#[allow(w1301)]` in the source** — item-granular, part of the
   program, honored by every consumer;
2. **CLI flags** — `--allow/--warn/--deny <sel>`, `--deny-warnings`;
3. **the manifest** — `lints.<level> = sel, …` in `wolf.pkg`.

Selectors are one code (`W1301`), a family (`W13xx`), or `warnings`.
Specificity wins (code over family over all); among equals the last
rule set wins, and CLI rules are appended after manifest rules on
purpose.

## Escape-hatch etiquette

Allow **locally** and **with a reason**: an `#[allow]` sits on the one
item that earns it, next to a comment saying why the shape is
deliberate. Package-wide allows in the manifest are for staged
adoption, not permanent silence. **CI posture is deny**: a tree that
is warning-clean stays warning-clean by decree (`--deny-warnings`),
and this repository's own corpus holds itself to exactly that bar. A
warning everyone silences is a bug in the catalog — file it; the
catalog answers by fixing the lint or retiring the number (retired
numbers are never reused).

## Fix-its, and what `wolf fix` will touch

Warnings carry mechanical fixes where one exists. Only
**machine-applicable** suggestions are ever applied by `wolf fix` —
a fix is machine-applicable only when the toolchain can prove every
affected site is rewritten (W1002 rewrites the declaration and every
call site together, and downgrades itself to a suggestion when a call
site is not provably safe to touch). Everything else renders as a
`help:` the author applies by hand.

## Cross-implementation posture

The differential protocol's record carries a `warnings` array
(`[proto.record.warn]`), and warning parity grows lint-by-lint:
syntax- and name-level lints are shared-analysis (the reference
interpreter can implement them), type/memory/concurrency lints are
compiler-only until it grows the analysis — absence is honest, never
a divergence. `#[allow]` is part of the program and suppresses
identically on both sides.

---

# The W-catalog
";

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
        // E1013 (s72, D40 [mem.iter.excl]): one rule, two enforcement
        // modes — wolfgang rejects the mutated-while-iterated shape
        // statically, lupin traps it. The lupin mirror lands in
        // v0.1.8; until that pin, an oracle running the shape clean
        // is the expected conservatism note, not a divergence. E1014
        // E1014 (D39 callee-side read-mode) mirrors as exclusivity,
        // pinned by the v0.1.8 write barrier.
        "E1002" | "E1013" | "E1014" => Some("exclusivity"),
        "E1004" | "E1005" | "E1010" | "E1011" | "E1012" => Some("region-fault"),
        // The conc family (spec 03): E1101's shape is the data race
        // an oracle's detector reports ([conc.mm.race.3]); E1103's is
        // the self-acquisition deadlock ([conc.deadlock.self]). E1102
        // has no dynamic counterpart — capture-by-copy machines run
        // the unsendable-payload shape with task-local effects, so an
        // oracle that runs it clean is a completeness note.
        "E1101" => Some("race"),
        "E1103" => Some("deadlock"),
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
    let mut forward_pins = 0u32;
    // Coverage of THIS lane pairing ([proto.cmp.coverage], s82): how
    // many entries each side executed at the run rung, and how many
    // both did — the only files this invocation could compare
    // dynamically. Published in the report because a divergence count
    // is meaningless without the size of the set it was drawn from,
    // and because wolf-lang#90 was a coverage collapse that no
    // divergence count could have shown.
    let mut entries = 0u32;
    let (mut a_run, mut b_run, mut both_run) = (0u32, 0u32, 0u32);
    for f in &files {
        let directives = std::fs::read_to_string(f)
            .ok()
            .and_then(|src| corpus::parse_directives(&src).ok());
        if directives.as_ref().is_some_and(|d| d.is_member()) {
            continue; // compiled through its module's entry file (s12)
        }
        // A forward pin (s91) enters the conservatism ledger honestly —
        // A declines a construct it has not implemented — but it is not
        // the same fact as a rule one side enforces and the other does
        // not, so the ledger publishes it as its own number.
        let forward = directives.is_some_and(|d| d.forward.is_some());
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
        entries += 1;
        let (ca, cb) = (
            xtask::protocol::covered_at_run(&ra),
            xtask::protocol::covered_at_run(&rb),
        );
        a_run += u32::from(ca);
        b_run += u32::from(cb);
        both_run += u32::from(ca && cb);
        let structural =
            !ra["seeded"].as_bool().unwrap_or(false) || !rb["seeded"].as_bool().unwrap_or(false);
        if ra["verdict"] == serde_json::json!("unsupported")
            || rb["verdict"] == serde_json::json!("unsupported")
        {
            unsupported += 1;
            forward_pins += u32::from(forward);
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
            "differ: {} file(s) — {} agreement(s), {} completeness note(s), {} SOUNDNESS finding(s), {} unsupported ({} forward pin(s)); {} hard divergence(s)",
            files.len(),
            agreements,
            completeness,
            soundness,
            unsupported,
            forward_pins,
            divergences,
        );
    } else {
        eprintln!(
            "differ: {} file(s), {} divergence(s), {} in conservatism ledger \
             (unsupported), {} of them forward pin(s)",
            files.len(),
            divergences,
            unsupported,
            forward_pins,
        );
    }
    eprintln!(
        "differ: run-rung coverage — A executed {a_run}, B executed {b_run}, \
         BOTH executed {both_run} of {entries} entries ([proto.cmp.coverage]; \
         `cargo xtask lane-coverage` is the gated union across A's lanes)"
    );
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
    // Three artifacts, always: `wolf` cannot link a program without
    // `libwolf_rt.a` beside it. Shipping the binary alone is the exact
    // failure `cargo xtask install` exists to prevent, and v0.1.0's
    // first tarballs shipped that way (#62). The third is the C
    // importer worker (s46): the compiler locates it *next to itself*,
    // so an archive without it is one where `import c` cannot work.
    if !run_ok(
        "cargo",
        &[
            "build",
            "--release",
            "-p",
            "wolf_driver",
            "-p",
            "wolf_rt",
            "-p",
            "wolf_cimport",
            "--quiet",
        ],
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
    let rt = Path::new("target/release").join("libwolf_rt.a");
    if !rt.exists() {
        eprintln!("dist: libwolf_rt.a missing — the archive would not link");
        return ExitCode::FAILURE;
    }
    std::fs::copy(&rt, stage.join("libwolf_rt.a")).expect("stage runtime lib");
    // The C importer worker rides along: `wolf` finds it beside itself,
    // and never links it (s46, D33).
    let worker_exe = if host.contains("windows") {
        "wolf-cimport-worker.exe"
    } else {
        "wolf-cimport-worker"
    };
    let worker = Path::new("target/release").join(worker_exe);
    if !worker.exists() {
        eprintln!("dist: {worker_exe} missing — `import c` would not work from this archive");
        return ExitCode::FAILURE;
    }
    std::fs::copy(&worker, stage.join(worker_exe)).expect("stage importer worker");
    // Flatten: the archive is a flat directory, so a nested source path
    // stages under its file name (copying to stage/crates/... panicked
    // on the missing parents — dist only runs on tags, so nothing caught
    // it until the release page did).
    for f in ["README.md", "LICENSE", "crates/wolf_rt/LICENSE-EXCEPTION"] {
        let dest = stage.join(Path::new(f).file_name().expect("named file"));
        std::fs::copy(f, dest).expect("stage metadata file");
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
        // wolf_rt for the same reason as the Cranelift tier: the shared
        // trap-kind table (single authority for codegen and runtime).
        (
            "wolf_codegen_llvm",
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
        // The documentation generator (s53): a CLIENT of the query
        // contract, exactly like the LSP shim. It renders and never
        // analyzes — it reaches sema only for the item-signature
        // pretty-printer and the visibility enum, so a doc page cannot
        // describe a type the compiler did not resolve.
        (
            "wolf_doc",
            Some(&["wolf_span", "wolf_diag", "wolf_sema", "wolf_query"][..]),
        ),
        // The C header importer INTERFACE (s46, c10): the artifact,
        // its serialization, the worker process contract and the
        // cache. It must NEVER link a C frontend — the worker is a
        // separate executable the compiler locates at run time (D17's
        // swappable-importer requirement, and the only way libclang can
        // exist in this story without a build script inside the
        // compiler, D33). Its whole third-party surface is blake3.
        ("wolf_cimport", Some(&["wolf_span", "wolf_diag"][..])),
        // The package manager (s51): formats + resolution only — it
        // lexes manifests with the compiler's own lexer (one grammar,
        // D33) and never sees sema; the driver wires resolution into
        // module loading, not this crate. It reaches wolf_cimport for
        // the `c: { }` recipe type: the manifest declares C
        // dependencies, and the importer owns what a declaration means.
        (
            "wolf_pkg",
            Some(&["wolf_span", "wolf_diag", "wolf_lex", "wolf_cimport"][..]),
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
