//! `wolf` — the single toolchain binary (D34).
//!
//! Eventual surface: build run test bench fmt doc lsp dbg add vendor audit
//! publish fix toolchain. v0 grows at s31 (`wolf build|run`); today the
//! binary anchors the crate graph's top and serves the differential
//! protocol (`conform-run`, spec/06 [proto.invoke]) with the deepest
//! implemented phase: **lex** (s07).

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") => println!("wolf 0.0.1 (pre-alpha)"),
        Some("conform-run") => conform_run(&args[1..]),
        _ => {
            eprintln!("wolf: pre-alpha scaffold; `wolf build|run` lands at sprint s31");
            std::process::exit(2);
        }
    }
}

const PHASES: [&str; 8] = [
    "none",
    "lex",
    "parse",
    "resolve",
    "typecheck",
    "mem",
    "wir",
    "run",
];

/// Observation record ([proto.record]). The deepest implemented phase is
/// `lex`: `--phase=lex` verdicts `pass`/`fail(E01xx)` on the real token
/// stream; deeper (or absent) phase requests run the lexer too but stay
/// `unsupported` when it is clean — the conservatism ledger keeps showing
/// exactly what the compiler cannot do yet. `--phase=none` remains the
/// pre-s07 stub record.
fn conform_run(args: &[String]) {
    let mut file = None;
    let mut phase: Option<String> = None;
    for a in args {
        if a == "--json" || a.starts_with("--seed=") {
            continue; // accepted per [proto.invoke.cli]
        }
        if let Some(p) = a.strip_prefix("--phase=") {
            if !PHASES.contains(&p) {
                eprintln!(
                    "wolf conform-run: unknown phase `{p}` (canonical: {})",
                    PHASES.join(", ")
                );
                std::process::exit(2);
            }
            phase = Some(p.to_string());
            continue;
        }
        if a.starts_with("--") {
            eprintln!("wolf conform-run: unknown flag `{a}`");
            std::process::exit(2);
        }
        file = Some(a.clone());
    }
    let Some(file) = file else {
        eprintln!("usage: wolf conform-run <file.lu> [--phase=<p>] [--seed=N] [--json]");
        std::process::exit(2);
    };
    if !Path::new(&file).is_file() {
        eprintln!("wolf conform-run: no such file: {file}");
        std::process::exit(2);
    }

    let (phase_reached, verdict, diagnostics) = if phase.as_deref() == Some("none") {
        ("none", "unsupported".to_string(), Vec::new())
    } else {
        let bytes = match std::fs::read(&file) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("wolf conform-run: read {file}: {e}");
                std::process::exit(2);
            }
        };
        let mut sm = wolf_span::SourceMap::new();
        let id = sm.intern(Path::new(&file));
        let lexed = wolf_lex::lex(id, &bytes);
        let diags: Vec<serde_json::Value> = lexed
            .diagnostics
            .iter()
            .map(|d| {
                serde_json::json!({
                    "code": d.code,
                    "span": [d.span.lo, d.span.hi],
                    "severity": match d.severity {
                        wolf_diag::Severity::Error => "error",
                        wolf_diag::Severity::Warning => "warning",
                    },
                })
            })
            .collect();
        let first_error = lexed
            .diagnostics
            .iter()
            .find(|d| d.severity == wolf_diag::Severity::Error);
        let verdict = match (first_error, phase.as_deref()) {
            (Some(e), _) => format!("fail({})", e.code),
            // Stopped exactly at the requested phase, clean.
            (None, Some("lex")) => "pass".to_string(),
            // Asked to go deeper (or as deep as possible): lex is clean
            // but the rest is out of scope — conservatism ledger.
            (None, _) => "unsupported".to_string(),
        };
        ("lex", verdict, diags)
    };

    let record = serde_json::json!({
        "protocol": 1,
        "impl": "wolfc",
        "impl_version": env!("CARGO_PKG_VERSION"),
        "commit": option_env!("WOLF_COMMIT").unwrap_or("unknown"),
        "file": file.replace('\\', "/"),
        "phase_reached": phase_reached,
        "seeded": false,
        "diagnostics": diagnostics,
        "verdict": verdict,
        "stdout_sha256": serde_json::Value::Null,
        "stdout_inline": serde_json::Value::Null,
    });
    println!("{record}");
}
