//! `wolf test` — the built-in test framework, v0 (s39; D34's single
//! binary, one test story for every wolf codebase).
//!
//! # Discovery (the D34 shape, v0 slice)
//!
//! Black-box `*_test.lu` files: `wolf test [paths…]` walks each
//! directory for files ending `_test.lu` (a file argument is taken
//! as-is) — no registration, no life-before-main (D32). Inside a test
//! file, every top-level zero-parameter `fn test_*` is one test, run
//! in **declaration order**, each in a fresh checked machine
//! (per-test isolation: nothing survives between tests). A test file
//! with no `test_*` functions but a `main` runs black-box as one test
//! named `main`. In-language `test "name" { … }` blocks are the
//! contract's other half; they need grammar and land with the
//! colocated-test slice — recorded as an s39 delta, not silently
//! dropped.
//!
//! # Verdicts and exit discipline
//!
//! A test passes iff its checked execution exits 0 (a unit return is
//! exit 0; `assert` failure is `trap(assert)`; an error row escaping
//! is the documented D30 process outcome, exit 1). Anything the
//! checked machine refuses is reported **unsupported** — the
//! conservatism ledger; an unsupported test fails the run, because a
//! green run must mean every discovered test actually ran and passed.
//! Exit codes: 0 all pass, 1 any failure/unsupported/compile error,
//! 2 usage or environment.
//!
//! # Warnings (s67)
//!
//! Test files compile under the same warning machinery as `wolf
//! build`: source `#[allow]` regions, the manifest `lints` table, and
//! the CLI `--allow/--warn/--deny/--deny-warnings` flags. Surviving
//! warnings render on stderr even when every test passes; a
//! deny-promoted warning is a compile error and fails the run.
//!
//! # The determinism surface (X12, decided in s36 — spec/07 §5)
//!
//! - `--schedules=N` explores: each test runs N times under seeds
//!   split from a printed root seed; a verdict that varies across
//!   runs is a finding (FAILED, schedule divergence), and every
//!   failing run prints its seed and a copy-pasteable `--replay=`
//!   line.
//! - `--replay=<seed | w1-token | ev:stream>` is the one reproduction
//!   verb — it accepts every schedule spelling (`[sched.seed]`).
//!   Its SCOPE on the native lane (s74, #65): a decimal seed reaches
//!   `wolf_rt`'s scheduler PRNG — steal victims, `select` tie-breaks —
//!   and cross-task arrival order on the real worker pool is not
//!   derived from it. `[sched.stable]` states byte-identical replay
//!   over a recorded *decision stream*, which only the det engine's
//!   `w1-`/`ev:` forms carry, and the native lane does not emit one
//!   yet. So a native divergence's replay line is MEASURED before it
//!   is printed ([`native_replay_scope`]): the line either reports a
//!   confirmed reproduction or says what the seed does not pin. The
//!   flag never claims a reproduction it has not observed.
//! - `--chaos` is parked with its owner named (the c07 closeout; the
//!   injection engine lands on the s35 submit/deliver seams) and
//!   refuses loudly until then.
//!
//! Honesty note (s73 updates the s39 posture): under `--schedules`
//! and decimal `--replay`, MAIN-SHAPED tests compile natively and run
//! per seed with `WOLF_SCHED_SEED` set — the seed genuinely reaches
//! the runtime scheduler's PRNG (X12 end to end). `test_*` fns still
//! execute on the checked machine (serial, seed-blind) until the
//! native multi-test harness lands; packed `w1-`/`ev:` replays await
//! the det-engine runtime build (both recorded, tier named).
//!
//! # Machine output (`--json`, wolf-test/0)
//!
//! One JSON object per line on stdout (stderr keeps the rich human
//! diagnostics). Schema `wolf-test/0` — versioned from day one,
//! documented in `docs/test-json.md`, conformance-tested in
//! `tests/test_cmd.rs`; D36 stability fossilizes at s51, and any
//! change before that bumps the version. Events: `suite` (one per
//! test file), `test` (one per test), `summary` (exactly one, last).

use std::path::PathBuf;

use wolf_diag::lint::{Level, LintLevels};
use wolf_diag::{Diagnostic, HumanReporter, RenderOptions, Reporter, Sources};
use wolf_mem::ubcheck::{self, Budget, Verdict};

/// One test's outcome.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Pass,
    Fail,
    /// The checked machine refused (conservatism ledger) — counted
    /// against a green run, reported with the construct named.
    Unsupported,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::Unsupported => "unsupported",
        }
    }
}

struct Tally {
    passed: usize,
    failed: usize,
    unsupported: usize,
    filtered_out: usize,
}

pub fn test_cmd(args: &[String]) {
    let fail = |msg: &str| -> ! {
        eprintln!("wolf test: {msg}");
        std::process::exit(2);
    };
    let (args, std_root) = match crate::take_std_root(args).and_then(|(a, f)| {
        let root = crate::effective_std_root(f)?;
        Ok((a, root))
    }) {
        Ok(v) => v,
        Err(e) => fail(&e),
    };
    let mut filter: Option<String> = None;
    let mut fail_fast = false;
    let mut list = false;
    let mut json = false;
    // s53: doctests run by default; `--no-doc` skips them, `--doc` runs
    // only them.
    let mut no_doc = false;
    let mut only_doc = false;
    let mut schedules: Option<u32> = None;
    let mut replay: Option<String> = None;
    let mut lints = LintLevels::new();
    let mut paths: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(v) = a.strip_prefix("--filter=") {
            filter = Some(v.to_string());
        } else if a == "--filter" {
            i += 1;
            match args.get(i) {
                Some(v) => filter = Some(v.clone()),
                None => fail("--filter needs a substring"),
            }
        } else if a == "--fail-fast" {
            fail_fast = true;
        } else if a == "--list" {
            list = true;
        } else if a == "--json" {
            json = true;
        } else if a == "--no-doc" {
            no_doc = true;
        } else if a == "--doc" {
            only_doc = true;
        } else if a == "--deny-warnings" {
            lints.deny_warnings();
        } else if let Some((flag, level)) = match a.as_str() {
            "--allow" => Some(("--allow", Level::Allow)),
            "--warn" => Some(("--warn", Level::Warn)),
            "--deny" => Some(("--deny", Level::Deny)),
            _ => None,
        } {
            i += 1;
            match args.get(i) {
                Some(v) => lints.set(crate::parse_lint_selector("test", flag, v), level),
                None => fail(&format!(
                    "{flag} needs a lint selector (W####, W##xx, warnings)"
                )),
            }
        } else if let Some((flag, level, v)) = a
            .strip_prefix("--allow=")
            .map(|v| ("--allow", Level::Allow, v))
            .or_else(|| {
                a.strip_prefix("--warn=")
                    .map(|v| ("--warn", Level::Warn, v))
            })
            .or_else(|| {
                a.strip_prefix("--deny=")
                    .map(|v| ("--deny", Level::Deny, v))
            })
        {
            lints.set(crate::parse_lint_selector("test", flag, v), level);
        } else if let Some(v) = a.strip_prefix("--schedules=") {
            match v.parse::<u32>() {
                Ok(n) if n > 0 => schedules = Some(n),
                _ => fail("--schedules needs a positive count (--schedules=N)"),
            }
        } else if let Some(v) = a.strip_prefix("--replay=") {
            if parse_schedule_spec(v).is_none() {
                fail(&format!(
                    "`--replay={v}` is not a schedule: expected a decimal seed, \
                     a `w1-` schedule token, or an explicit `ev:c0,c1,…` \
                     decision stream (spec/07 [sched.seed])"
                ));
            }
            replay = Some(v.to_string());
        } else if a == "--chaos" {
            // Decided name, parked engine (spec/07 [sched.flags]): the
            // injection seams are named (io.arrive delay/reorder in the
            // simulated reactor, proc.kill injection, s35 short
            // reads/errors); the c07 closeout owns the handoff.
            eprintln!(
                "wolf test: `--chaos` is named and reserved (X12/D23, spec/07 \
                 §5) — seeded fault injection at the io.arrive/proc.kill/s35 \
                 submit-deliver seams. The injection engine is a c07-closeout \
                 handoff and has not landed; chaos runs will be replayable by \
                 the same `--replay=` mechanism when it does."
            );
            std::process::exit(2);
        } else if a.starts_with('-') {
            fail(&format!(
                "unknown flag `{a}` (usage: wolf test [<dir>|<file.lu>]… \
                 [--filter=SUBSTR] [--fail-fast] [--list] [--json] \
                 [--schedules=N] [--replay=<seed|w1-token|ev:stream>] [--chaos] \
                 [--allow|--warn|--deny <sel>] [--deny-warnings] [--std-root <dir>])"
            ));
        } else {
            paths.push(a.clone());
        }
        i += 1;
    }
    if paths.is_empty() {
        paths.push(".".to_string());
    }
    if schedules.is_some() && replay.is_some() {
        fail("--schedules and --replay are exclusive: explore first, then replay a finding");
    }
    // Root seed: `WOLF_SCHED_SEED` pins it (CI reproducibility); else
    // time-derived — every derived per-test seed is printed on
    // failure, so any finding is reproducible regardless.
    let root_seed: u64 = std::env::var("WOLF_SCHED_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x5EED)
        });
    if let Some(n) = schedules {
        eprintln!(
            "wolf test: exploring {n} schedule(s) per test (root seed {root_seed}; \
             reproduce any run with --replay=SEED)"
        );
    }
    if let Some(spec) = &replay {
        eprintln!("wolf test: replaying schedule `{spec}`");
        // The scope, said once and up front (s74, #65): a decimal seed
        // on a main-shaped test drives the REAL runtime, where it pins
        // the scheduler PRNG and not cross-task arrival order.
        if spec.parse::<u64>().is_ok() {
            eprintln!(
                "wolf test: a decimal seed pins the runtime's scheduler PRNG (steal \
                 victims, `select` tie-breaks); cross-task arrival order on the worker \
                 pool is not seed-derived, so a divergence that came from arrival order \
                 may not come back ([sched.seed])"
            );
        }
    }

    if no_doc && only_doc {
        fail("--doc and --no-doc are exclusive: one says only doctests, the other says none");
    }

    // ---- discovery: `*_test.lu` files, sorted (deterministic order) --
    let mut files: Vec<PathBuf> = Vec::new();
    for p in &paths {
        if only_doc {
            break;
        }
        let path = PathBuf::from(p);
        if path.is_dir() {
            let mut all = Vec::new();
            crate::collect_lu(&path, &mut all);
            files.extend(all.into_iter().filter(|f| {
                f.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with("_test.lu"))
            }));
        } else if path.is_file() {
            files.push(path);
        } else {
            fail(&format!("no such file or directory: {p}"));
        }
    }
    files.sort();
    files.dedup();

    let mut tally = Tally {
        passed: 0,
        failed: 0,
        unsupported: 0,
        filtered_out: 0,
    };
    let emit = |v: serde_json::Value| {
        println!("{v}");
    };
    let mut stopped_early = false;

    'files: for file in &files {
        let display = file.display().to_string().replace('\\', "/");
        let mut sm = wolf_span::SourceMap::new();
        let mut sources = Sources::new();
        let res =
            match crate::resolve_from_entry(file, &mut sm, &mut sources, std_root.as_deref(), None)
            {
                Ok(r) => r,
                Err(e) => fail(&e),
            };
        // The s67 gate, build's posture exactly: allow regions +
        // manifest + CLI levels; errors (original or deny-promoted)
        // render and fail the file.
        let scan = wolf_sema::scan_allows(&res.package);
        let levels = match crate::effective_lints(file, &lints) {
            Ok(l) => l,
            Err(e) => fail(&e),
        };
        let render = |sources: &Sources, diags: &[Diagnostic]| {
            let mut reporter = HumanReporter::new(sources, RenderOptions::default());
            for d in diags {
                reporter.report(d);
            }
            eprint!("{}", reporter.take_output());
        };
        let has_errors =
            |ds: &[Diagnostic]| ds.iter().any(|d| d.severity == wolf_diag::Severity::Error);
        let mut pending: Vec<Diagnostic> = Vec::new();
        let gate =
            |pending: &mut Vec<Diagnostic>, sources: &Sources, diags: Vec<Diagnostic>| -> bool {
                pending.extend(wolf_diag::lint::apply(&levels, &scan.allows, diags));
                if has_errors(pending) {
                    wolf_diag::sort_diagnostics(pending);
                    render(sources, pending);
                    true
                } else {
                    false
                }
            };
        let file_fail = |tally: &mut Tally, detail: &str| {
            tally.failed += 1;
            if json {
                emit(serde_json::json!({
                    "schema": "wolf-test/0", "event": "test", "file": display,
                    "name": "<file>", "status": "fail", "detail": detail,
                }));
            } else {
                println!("test {display} ... FAILED ({detail})");
            }
        };
        let mut resolve_diags = res.diagnostics.clone();
        resolve_diags.extend(scan.diagnostics.iter().cloned());
        if gate(&mut pending, &sources, resolve_diags) {
            file_fail(&mut tally, "does not compile");
            if fail_fast {
                stopped_early = true;
                break 'files;
            }
            continue;
        }
        let tc = wolf_sema::typecheck_package(&res);
        if let Some(nyc) = tc.not_yet.first() {
            report_file_unsupported(&mut tally, json, &display, nyc.construct);
            continue;
        }
        if gate(&mut pending, &sources, tc.diagnostics.clone()) {
            file_fail(&mut tally, "does not compile");
            if fail_fast {
                stopped_early = true;
                break 'files;
            }
            continue;
        }
        let mem = wolf_mem::check_package(&res.package, &tc);
        if let Some(nyc) = mem.not_yet.first() {
            report_file_unsupported(&mut tally, json, &display, nyc.construct);
            continue;
        }
        if gate(&mut pending, &sources, mem.diagnostics.clone()) {
            file_fail(&mut tally, "does not compile");
            if fail_fast {
                stopped_early = true;
                break 'files;
            }
            continue;
        }
        // Surviving warnings render even when everything passes —
        // `wolf test` surfaces warnings, never swallows them (s67).
        if !pending.is_empty() {
            wolf_diag::sort_diagnostics(&mut pending);
            render(&sources, &pending);
        }

        // ---- the file's tests: `test_*` fns, else black-box main --
        let discovered = ubcheck::discover_tests(&res.package);
        let tests: Vec<(String, usize)> = if discovered.is_empty() {
            if ubcheck::has_main(&res.package) {
                vec![("main".to_string(), 0)]
            } else {
                if !json {
                    eprintln!(
                        "wolf test: {display}: no `test_*` functions and no `main` — \
                         nothing to run"
                    );
                }
                Vec::new()
            }
        } else {
            discovered
        };
        if json && !tests.is_empty() {
            emit(serde_json::json!({
                "schema": "wolf-test/0", "event": "suite",
                "file": display, "tests": tests.len(),
            }));
        }
        for (name, arity) in &tests {
            let qualified = format!("{display}::{name}");
            if let Some(f) = &filter
                && !qualified.contains(f.as_str())
            {
                tally.filtered_out += 1;
                continue;
            }
            if list {
                println!("{qualified}");
                continue;
            }
            // Per-test isolation: a fresh machine per test — no state
            // survives, no ordering coupling. Parallel execution is
            // the s32-task story and lands with the native test
            // harness; v0 is serial and deterministic.
            let run_once =
                || match ubcheck::run_checked_fn(&res.package, &tc, Budget::default(), "", name) {
                    Ok(out) => {
                        let (status, detail) = match &out.verdict {
                            Verdict::Exit(0) => (Status::Pass, "exit(0)".to_string()),
                            Verdict::Exit(n) => (Status::Fail, format!("exit({n})")),
                            Verdict::Trap(t) => (Status::Fail, format!("trap({})", t.kind)),
                            Verdict::Ub(f) => {
                                // The D22 bar: a UB verdict renders its
                                // full E1401, not a one-liner.
                                let d = ubcheck::ub_diagnostic(f);
                                render(&sources, std::slice::from_ref(&d));
                                (Status::Fail, "ub(mem.ub)".to_string())
                            }
                        };
                        (status, detail, Some(out))
                    }
                    Err(nyc) => (Status::Unsupported, nyc.construct.to_string(), None),
                };
            let (status, detail, out) = if *arity != 0 {
                (
                    Status::Unsupported,
                    "a test fn with parameters (s39 runs zero-parameter tests)".to_string(),
                    None,
                )
            } else if name == "main"
                && (schedules.is_some()
                    || replay
                        .as_deref()
                        .and_then(|s| s.parse::<u64>().ok())
                        .is_some())
            {
                // s73 — the native schedule lane: a main-shaped test
                // explores/replays on the REAL runtime, each run's
                // seed in `WOLF_SCHED_SEED` (X12 end to end). The
                // packed `w1-`/`ev:` forms still need the det-engine
                // harness (they fall through to the checked machine's
                // honest note).
                let seeds: Vec<Option<u64>> = match (schedules, &replay) {
                    (Some(n), _) => (0..u64::from(n))
                        .map(|k| Some(derive_seed(root_seed, &qualified, k)))
                        .collect(),
                    (None, Some(spec)) => {
                        vec![Some(spec.parse::<u64>().expect("checked decimal"))]
                    }
                    _ => unreachable!("guard requires schedules or a decimal replay"),
                };
                match native_schedule_runs(file, std_root.as_deref(), &seeds) {
                    Err((s, d)) => (s, d, None),
                    Ok(runs) => {
                        let (seed0, s0, d0) =
                            (runs[0].0.unwrap_or(0), runs[0].1, runs[0].2.clone());
                        let divergence = runs
                            .iter()
                            .find(|(_, s, d, _, _)| *s != s0 || *d != d0)
                            .map(|(seed, _, d, _, _)| (seed.unwrap_or(0), d.clone()));
                        let mk_out = |r: &(Option<u64>, Status, String, String, String)| {
                            Some(ubcheck::RunOutcome {
                                verdict: Verdict::Exit(0),
                                stdout: r.3.clone(),
                                stderr: r.4.clone(),
                            })
                        };
                        match divergence {
                            Some((seed_div, d_div)) => (
                                Status::Fail,
                                format!(
                                    "schedule divergence: seed {seed0} → {d0}, seed \
                                     {seed_div} → {d_div} — replay: wolf test \
                                     --replay={seed_div} {display}{}",
                                    native_replay_scope(
                                        file,
                                        std_root.as_deref(),
                                        seed_div,
                                        &d_div,
                                    )
                                ),
                                runs.iter()
                                    .find(|(sd, ..)| sd.unwrap_or(0) == seed_div)
                                    .and_then(mk_out),
                            ),
                            None => {
                                let nn = runs.len();
                                let detail = if s0 == Status::Pass {
                                    format!("{d0} [native, {nn} schedule(s)]")
                                } else {
                                    format!(
                                        "{d0} [native, {nn} schedule(s); replay: wolf test \
                                         --replay={seed0} {display}]"
                                    )
                                };
                                (s0, detail, mk_out(&runs[0]))
                            }
                        }
                    }
                }
            } else if let Some(n) = schedules {
                // Exploration (spec/07 [sched.flags]): N runs under
                // derived seeds; the CI-checkable property is verdict
                // stability. A varying verdict is a finding, and every
                // finding carries its replay line.
                let mut runs: Vec<(u64, Status, String, Option<_>)> = Vec::new();
                for k in 0..u64::from(n) {
                    let seed = derive_seed(root_seed, &qualified, k);
                    let (s, d, o) = run_once();
                    runs.push((seed, s, d, o));
                }
                let (seed0, s0, d0) = (runs[0].0, runs[0].1, runs[0].2.clone());
                let divergence = runs
                    .iter()
                    .find(|(_, s, d, _)| *s != s0 || *d != d0)
                    .map(|(seed, _, d, _)| (*seed, d.clone()));
                match divergence {
                    Some((seed_div, d_div)) => (
                        Status::Fail,
                        format!(
                            "schedule divergence: seed {seed0} → {d0}, seed {seed_div} → \
                             {d_div} — replay: wolf test --replay={seed_div} {display}"
                        ),
                        runs.pop().and_then(|(_, _, _, o)| o),
                    ),
                    None => {
                        let detail = if s0 == Status::Pass {
                            format!("{d0} [{n} schedule(s)]")
                        } else {
                            format!(
                                "{d0} [{n} schedule(s); replay: wolf test \
                                 --replay={seed0} {display}]"
                            )
                        };
                        (s0, detail, runs.swap_remove(0).3)
                    }
                }
            } else {
                // s73: a main-shaped test the checked machine refuses
                // (concurrency, C1 deferred) falls back to one native
                // black-box run — the conservatism ledger stays for
                // `test_*` fns, but a runnable program runs.
                let r = run_once();
                if r.0 == Status::Unsupported && name == "main" {
                    match native_schedule_runs(file, std_root.as_deref(), &[None]) {
                        Ok(runs) => {
                            let (_, s, d, stdout, stderr) = runs.into_iter().next().expect("one");
                            (
                                s,
                                format!("{d} [native]"),
                                Some(ubcheck::RunOutcome {
                                    verdict: Verdict::Exit(0),
                                    stdout,
                                    stderr,
                                }),
                            )
                        }
                        Err((s, d)) => (s, d, None),
                    }
                } else {
                    r
                }
            };
            match status {
                Status::Pass => tally.passed += 1,
                Status::Fail => tally.failed += 1,
                Status::Unsupported => tally.unsupported += 1,
            }
            if json {
                let mut ev = serde_json::json!({
                    "schema": "wolf-test/0", "event": "test", "file": display,
                    "name": name, "status": status.as_str(), "detail": detail,
                });
                if status != Status::Pass
                    && let Some(out) = &out
                {
                    ev["stdout"] = serde_json::json!(out.stdout);
                    ev["stderr"] = serde_json::json!(out.stderr);
                }
                emit(ev);
            } else {
                match status {
                    Status::Pass => println!("test {qualified} ... ok"),
                    Status::Fail => println!("test {qualified} ... FAILED ({detail})"),
                    Status::Unsupported => {
                        println!("test {qualified} ... unsupported ({detail})")
                    }
                }
                if status != Status::Pass
                    && let Some(out) = &out
                {
                    for line in out.stdout.lines() {
                        println!("  stdout: {line}");
                    }
                    for line in out.stderr.lines() {
                        println!("  stderr: {line}");
                    }
                }
            }
            if status != Status::Pass && fail_fast {
                stopped_early = true;
                break 'files;
            }
        }
    }

    // ---- doctests (s53): fences in doc comments are tests, by default --
    //
    // A separate pass over EVERY `.lu` file the paths name (docs live in
    // source, not in `*_test.lu`), so a package's documentation is
    // verified by the same command that verifies its tests. `--no-doc`
    // opts out; `--doc` runs only this pass.
    if !no_doc {
        run_doctests(
            &paths,
            std_root.as_deref(),
            &filter,
            list,
            json,
            fail_fast,
            &mut tally,
            &mut stopped_early,
        );
    }

    if list {
        std::process::exit(0);
    }
    let ok = tally.failed == 0 && tally.unsupported == 0;
    if json {
        emit(serde_json::json!({
            "schema": "wolf-test/0", "event": "summary",
            "passed": tally.passed, "failed": tally.failed,
            "unsupported": tally.unsupported, "filtered_out": tally.filtered_out,
            "stopped_early": stopped_early,
        }));
    } else {
        println!(
            "wolf test: {} passed; {} failed; {} unsupported; {} filtered out{}",
            tally.passed,
            tally.failed,
            tally.unsupported,
            tally.filtered_out,
            if stopped_early {
                " (stopped early: --fail-fast)"
            } else {
                ""
            }
        );
    }
    std::process::exit(if ok { 0 } else { 1 });
}

/// The doctest pass (s53). Every `.lu` file under the given paths is
/// asked for its doc fences; each fence is one test, reported through
/// the same `wolf-test/0` records as any other test — a doctest that
/// fails fails the run, which is the whole covenant.
#[allow(clippy::too_many_arguments)]
fn run_doctests(
    paths: &[String],
    std_root: Option<&std::path::Path>,
    filter: &Option<String>,
    list: bool,
    json: bool,
    fail_fast: bool,
    tally: &mut Tally,
    stopped_early: &mut bool,
) {
    let mut files: Vec<PathBuf> = Vec::new();
    for p in paths {
        let path = PathBuf::from(p);
        if path.is_dir() {
            crate::collect_lu(&path, &mut files);
        } else if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    files.dedup();
    let scratch = crate::doctest_cmd::scratch_dir();
    let _ = std::fs::remove_dir_all(&scratch);
    for file in &files {
        let display = file.display().to_string().replace('\\', "/");
        // Cheap pre-filter: no fence, no work. Resolving a package per
        // source file is the expensive part, and most files have no
        // doc examples.
        let text = std::fs::read_to_string(file).unwrap_or_default();
        if !text.contains("```") {
            continue;
        }
        let surface = match crate::doctest_cmd::collect(file, std_root) {
            Ok(d) => d,
            Err(e) => {
                if !json {
                    eprintln!("wolf test: {display}: {e}");
                }
                continue;
            }
        };
        if surface.doctests.is_empty() {
            continue;
        }
        if json {
            emit_json(serde_json::json!({
                "schema": "wolf-test/0", "event": "suite",
                "file": display, "tests": surface.doctests.len(),
            }));
        }
        for dt in &surface.doctests {
            let qualified = format!("{display}::{} (doctest)", dt.name);
            if let Some(f) = filter
                && !qualified.contains(f.as_str())
            {
                tally.filtered_out += 1;
                continue;
            }
            if list {
                println!("{qualified}");
                continue;
            }
            let outcome = crate::doctest_cmd::run(dt, &surface, std_root, &scratch);
            match outcome.status() {
                "pass" => tally.passed += 1,
                "unsupported" => tally.unsupported += 1,
                _ => tally.failed += 1,
            }
            if json {
                emit_json(serde_json::json!({
                    "schema": "wolf-test/0", "event": "test", "file": display,
                    "name": format!("{} (doctest)", dt.name),
                    "status": outcome.status(), "detail": outcome.detail(),
                }));
            } else {
                let word = match outcome.status() {
                    "pass" => "ok",
                    "unsupported" => "unsupported",
                    _ => "FAILED",
                };
                println!("doctest {qualified} ... {word} ({})", outcome.detail());
            }
            if outcome.status() != "pass" && fail_fast {
                *stopped_early = true;
                let _ = std::fs::remove_dir_all(&scratch);
                return;
            }
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

fn emit_json(v: serde_json::Value) {
    println!("{v}");
}

/// Validate a `--replay` schedule spec (spec/07 `[sched.seed]`):
/// decimal seed, `w1-` schedule token, or explicit `ev:` decision
/// stream. Validation only — the checked machine is seed-blind; the
/// native runtime consumes the spec when the native harness lands.
fn parse_schedule_spec(s: &str) -> Option<()> {
    if let Some(list) = s.strip_prefix("ev:") {
        let items: Vec<&str> = list.split(',').filter(|x| !x.is_empty()).collect();
        if items.is_empty() || items.iter().any(|x| x.trim().parse::<u32>().is_err()) {
            return None;
        }
        return Some(());
    }
    if s.starts_with("w1-") {
        // Token decode lives in the task layer, which shares the s28
        // linux-only campaign posture; off-linux the replay flags refuse
        // before this validator matters, so an invalid verdict here stays
        // honest rather than silently accepting an uncheckable token.
        #[cfg(target_os = "linux")]
        return wolf_rt::task::seed_spec::parse_token(s).map(|_| ());
        #[cfg(not(target_os = "linux"))]
        return None;
    }
    s.parse::<u64>().ok().map(|_| ())
}

/// How many times a native divergence's seed is re-run to see whether
/// the printed `--replay=` line actually reproduces it (s74, #65).
const NATIVE_REPLAY_PROBE: usize = 3;

/// Say what the printed `--replay=SEED` line is worth on the NATIVE
/// lane, having MEASURED it (s74, #65).
///
/// A decimal seed reaches `wolf_rt`'s scheduler PRNG — steal victims,
/// `select` tie-breaks — and nothing else. Cross-task arrival order on
/// the real worker pool is not seed-derived, so a divergence that came
/// from arrival order is not reproducible from the seed, and the flag
/// used to print the line anyway. `[sched.stable]`'s byte-identical
/// guarantee is stated over a *decision stream*, which only the det
/// engine's `w1-`/`ev:` recordings carry; the native lane has no such
/// recording yet (owner: the s36/s73 family, det-engine runtime build).
///
/// So: re-run the seed a few times. If the verdict comes back, the line
/// is confirmed and says so. If it does not, the line says what the
/// seed does and does not pin, and points at what reproduces instead.
/// Nothing here promises what it has not seen.
fn native_replay_scope(
    file: &std::path::Path,
    std_root: Option<&std::path::Path>,
    seed: u64,
    want: &str,
) -> String {
    let probe = vec![Some(seed); NATIVE_REPLAY_PROBE];
    let Ok(runs) = native_schedule_runs(file, std_root, &probe) else {
        return String::new(); // the probe itself failed: claim nothing
    };
    let hits = runs.iter().filter(|(_, _, d, _, _)| d == want).count();
    if hits == runs.len() {
        return format!(" (replay confirmed: reproduced {hits}/{})", runs.len());
    }
    format!(
        " — NOTE: that replay reproduced this verdict {hits}/{} times. On the \
         native lane a decimal seed pins the runtime's scheduler PRNG (steal \
         victims, `select` tie-breaks), NOT cross-task arrival order on the \
         worker pool, so an arrival-order divergence is not reproducible from \
         the seed alone ([sched.seed]; byte-identical replay in [sched.stable] \
         is stated over a recorded decision stream, which the native lane does \
         not yet emit). Re-explore with --schedules=N, or make the property \
         schedule-independent — compare a sorted or hashed output rather than \
         an interleaving",
        runs.len()
    )
}

/// s73 — the native schedule lane (X12's promise, end to end): a
/// main-shaped test under `--schedules`/`--replay` compiles ONCE and
/// runs once per seed with `WOLF_SCHED_SEED` set, so every derived
/// seed genuinely reaches the runtime's scheduler PRNG (steal
/// victims, select tie-breaks — `wolf_rt::task::hooks::root_seed`).
/// Returns per-seed (seed, status, detail, stdout, stderr), or the
/// honest refusal.
#[allow(clippy::type_complexity)]
fn native_schedule_runs(
    file: &std::path::Path,
    std_root: Option<&std::path::Path>,
    seeds: &[Option<u64>],
) -> Result<Vec<(Option<u64>, Status, String, String, String)>, (Status, String)> {
    let dir = std::env::temp_dir().join(format!(
        "wolf-test-native-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    if std::fs::create_dir_all(&dir).is_err() {
        return Err((Status::Fail, "cannot create a scratch dir".to_string()));
    }
    let exe = dir.join(crate::exe_name("a.out"));
    let mut scratch = Sources::new();
    let compiled = crate::compile_native(
        file,
        std_root,
        &exe,
        &mut scratch,
        &crate::BuildOpts::ephemeral(),
        None,
    );
    let out = match compiled {
        Ok(()) => {
            let mut runs = Vec::with_capacity(seeds.len());
            for &seed in seeds {
                let mut cmd = std::process::Command::new(&exe);
                if let Some(s) = seed {
                    cmd.env("WOLF_SCHED_SEED", s.to_string());
                }
                let run = cmd.output();
                match run {
                    Err(e) => {
                        let _ = std::fs::remove_dir_all(&dir);
                        return Err((Status::Fail, format!("cannot run the test binary: {e}")));
                    }
                    Ok(o) => {
                        let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
                        let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
                        let trap_kind = stderr.lines().find_map(|l| {
                            l.trim()
                                .strip_prefix("wolf-trap:")
                                .map(|k| k.trim().to_string())
                        });
                        let (status, detail) = match (o.status.code(), trap_kind) {
                            (Some(code), Some(kind)) if code == wolf_rt::native::TRAP_EXIT_CODE => {
                                (Status::Fail, format!("trap({kind})"))
                            }
                            (Some(0), _) => (Status::Pass, "exit(0)".to_string()),
                            (Some(code), _) => (Status::Fail, format!("exit({code})")),
                            (None, _) => (Status::Fail, "died without an exit code".to_string()),
                        };
                        runs.push((seed, status, detail, stdout, stderr));
                    }
                }
            }
            Ok(runs)
        }
        Err(crate::BuildStop::Refused { reason, .. }) => Err((Status::Unsupported, reason)),
        Err(crate::BuildStop::Errors) => Err((Status::Fail, "does not compile".to_string())),
        Err(crate::BuildStop::Environment(msg)) => Err((Status::Fail, msg)),
    };
    let _ = std::fs::remove_dir_all(&dir);
    out
}

/// Split a per-run seed from the root (SplitMix64 over root, test
/// name, and run index — deterministic, collision-spread).
fn derive_seed(root: u64, qualified: &str, k: u64) -> u64 {
    let mut h: u64 = root ^ 0x9E37_79B9_7F4A_7C15u64.wrapping_mul(k.wrapping_add(1));
    for b in qualified.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01B3);
    }
    // SplitMix64 finalizer.
    h = h.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = h;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    // Stay in the simple-seed namespace (bit 62 clear, [sched.seed]).
    (z ^ (z >> 31)) & !(1u64 << 62)
}

/// A file whose ladder refused before tests could run: every test in
/// it is unknowable, so the file reports one unsupported entry — the
/// conservatism ledger, never a silent skip.
fn report_file_unsupported(tally: &mut Tally, json: bool, display: &str, construct: &str) {
    tally.unsupported += 1;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "schema": "wolf-test/0", "event": "test", "file": display,
                "name": "<file>", "status": "unsupported", "detail": construct,
            })
        );
    } else {
        println!("test {display} ... unsupported ({construct})");
    }
}
