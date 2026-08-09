//! `wolf` — the single toolchain binary (D34).
//!
//! Eventual surface: build run test bench fmt doc lsp dbg add vendor audit
//! publish fix toolchain. v0 grows at s31 (`wolf build|run`); today the
//! binary anchors the crate graph's top, serves the differential protocol
//! (`conform-run`, spec/06 [proto.invoke]) with the deepest implemented
//! phase (parse, s09), and fronts the s10 diagnostics engine:
//! `wolf --explain E####` renders the registry entry, and `conform-run`
//! reports diagnostics on stderr in the human CLI format (default) or
//! the diag-schema JSON line format (`--error-format=json`) while stdout
//! keeps the spec/06-minimal observation record.

use std::path::Path;

use wolf_diag::{Diagnostic, HumanReporter, JsonReporter, RenderOptions, Reporter, Sources};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") => println!("wolf 0.0.1 (pre-alpha)"),
        Some("--explain") => explain(&args[1..]),
        Some("conform-run") => conform_run(&args[1..]),
        _ => {
            eprintln!("wolf: pre-alpha scaffold; `wolf build|run` lands at sprint s31");
            std::process::exit(2);
        }
    }
}

/// `wolf --explain E0101`: the registry entry's summary + extended
/// explanation. Unknown codes exit 2.
fn explain(args: &[String]) {
    let Some(code) = args.first() else {
        eprintln!("usage: wolf --explain E####");
        std::process::exit(2);
    };
    let Some(info) = wolf_diag::explain(code) else {
        eprintln!(
            "wolf --explain: `{code}` is not a registered diagnostic code \
             (codes look like E0101; see the diagnostic catalog)"
        );
        std::process::exit(2);
    };
    println!("{}: {}", info.code, info.summary);
    println!();
    println!("{}", info.explanation.trim());
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
/// `parse` (s09): `--phase=lex|parse` verdict `pass`/`fail(E….)` on the
/// real front end; deeper (or absent) phase requests run the front end
/// too but stay `unsupported` when it is clean — the conservatism ledger
/// keeps showing exactly what the compiler cannot do yet. `--phase=none`
/// remains the pre-s07 stub record.
///
/// Diagnostics ride stderr (s10): the human CLI format by default, one
/// diag-schema JSON object per line with `--error-format=json`. The
/// stdout record's `diagnostics` array stays spec/06-minimal
/// (`{code, span, severity}`) — the record schema is frozen by the
/// protocol, the stderr stream is the rich surface.
fn conform_run(args: &[String]) {
    let mut file = None;
    let mut phase: Option<String> = None;
    let mut error_format = "human".to_string();
    for a in args {
        if a == "--json" || a.starts_with("--seed=") {
            continue; // accepted per [proto.invoke.cli]
        }
        if let Some(f) = a.strip_prefix("--error-format=") {
            if f != "human" && f != "json" {
                eprintln!("wolf conform-run: unknown error format `{f}` (human, json)");
                std::process::exit(2);
            }
            error_format = f.to_string();
            continue;
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
        eprintln!(
            "usage: wolf conform-run <file.lu> [--phase=<p>] [--seed=N] [--json] \
             [--error-format=human|json]"
        );
        std::process::exit(2);
    };
    if !Path::new(&file).is_file() {
        eprintln!("wolf conform-run: no such file: {file}");
        std::process::exit(2);
    }

    let mut sources = Sources::new();
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
        sources.add(id, file.replace('\\', "/"), &bytes);
        let lexed = wolf_lex::lex(id, &bytes);
        let first_error = |ds: &[Diagnostic]| {
            ds.iter()
                .find(|d| d.severity == wolf_diag::Severity::Error)
                .map(|d| d.code)
        };
        // Phase ladder, deepest implemented: parse (s09). Each rung either
        // stops with fail(code) at that rung, passes at the requested rung,
        // or falls through deeper; past the last rung the verdict is
        // `unsupported` (conservatism ledger).
        if let Some(code) = first_error(&lexed.diagnostics) {
            ("lex", format!("fail({code})"), lexed.diagnostics)
        } else if phase.as_deref() == Some("lex") {
            ("lex", "pass".to_string(), lexed.diagnostics)
        } else {
            let parsed = wolf_parse::parse_tokens(&lexed, &bytes);
            let parse_error = first_error(&parsed.diagnostics);
            let mut all = lexed.diagnostics;
            all.extend(parsed.diagnostics);
            wolf_diag::sort_diagnostics(&mut all);
            if let Some(code) = parse_error {
                ("parse", format!("fail({code})"), all)
            } else if phase.as_deref() == Some("parse") {
                ("parse", "pass".to_string(), all)
            } else {
                ("parse", "unsupported".to_string(), all)
            }
        }
    };

    // The rich diagnostic stream (s10) — stderr only, stdout is the
    // protocol's.
    let mut reporter: Box<dyn Reporter> = if error_format == "json" {
        Box::new(JsonReporter::new())
    } else {
        Box::new(HumanReporter::new(&sources, RenderOptions::default()))
    };
    for d in &diagnostics {
        reporter.report(d);
    }
    let rendered = reporter.take_output();
    if !rendered.is_empty() {
        eprint!("{rendered}");
    }

    // The observation record — spec/06-minimal diagnostics, never more.
    let minimal: Vec<serde_json::Value> = diagnostics
        .iter()
        .map(|d| {
            serde_json::json!({
                "code": d.code.as_str(),
                "span": [d.span().lo, d.span().hi],
                "severity": d.severity.as_str(),
            })
        })
        .collect();
    let record = serde_json::json!({
        "protocol": 1,
        "impl": "wolfc",
        "impl_version": env!("CARGO_PKG_VERSION"),
        "commit": option_env!("WOLF_COMMIT").unwrap_or("unknown"),
        "file": file.replace('\\', "/"),
        "phase_reached": phase_reached,
        "seeded": false,
        "diagnostics": minimal,
        "verdict": verdict,
        "stdout_sha256": serde_json::Value::Null,
        "stdout_inline": serde_json::Value::Null,
    });
    println!("{record}");
}
