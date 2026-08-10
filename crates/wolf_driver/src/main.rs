//! `wolf` — the single toolchain binary (D34).
//!
//! Eventual surface: build run test bench fmt doc lsp dbg add vendor audit
//! publish fix toolchain. v0 grows at s31 (`wolf build|run`); today the
//! binary anchors the crate graph's top, serves the differential protocol
//! (`conform-run`, spec/06 [proto.invoke]) with the deepest implemented
//! phase (typecheck, s13), and fronts the s10 diagnostics engine:
//! `wolf --explain E####` renders the registry entry, and `conform-run`
//! reports diagnostics on stderr in the human CLI format (default) or
//! the diag-schema JSON line format (`--error-format=json`) while stdout
//! keeps the spec/06-minimal observation record. `wolf interface`
//! pretty-prints the s12 `wolfi` module interfaces of a package.

use std::path::Path;

use wolf_diag::{Diagnostic, HumanReporter, JsonReporter, RenderOptions, Reporter, Sources};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") => println!("wolf 0.0.1 (pre-alpha)"),
        Some("--explain") => explain(&args[1..]),
        Some("conform-run") => conform_run(&args[1..]),
        Some("interface") => interface(&args[1..]),
        Some("fmt") => fmt(&args[1..]),
        Some("lsp") => lsp(&args[1..]),
        _ => {
            eprintln!("wolf: pre-alpha scaffold; `wolf build|run` lands at sprint s31");
            std::process::exit(2);
        }
    }
}

/// `wolf lsp` — the compiler serving the Language Server Protocol over
/// stdio (s52 v0, D34: one process, one truth). `--stdio` is accepted
/// for clients that pass the conventional channel flag; sockets are
/// s57's attachment story.
fn lsp(args: &[String]) {
    for a in args {
        if a != "--stdio" {
            eprintln!("wolf lsp: unknown flag `{a}` (v0 serves stdio only)");
            std::process::exit(2);
        }
    }
    match wolf_lsp::run_stdio() {
        // The lifecycle's exit code: 0 only when `exit` followed
        // `shutdown`; a bare `exit` is 1 (LSP spec).
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("wolf lsp: {e}");
            std::process::exit(1);
        }
    }
}

/// `wolf fmt <paths…>` — the zero-option formatter (s11, D34).
///
/// Files and directories (recursing into `*.lu`) reformat in place;
/// `-` formats stdin to stdout; `--check` rewrites nothing and exits
/// nonzero listing files that are not canonical. There are no other
/// flags, deliberately: one canonical style, fixed by spec/01 §7 and
/// versioned per the D36 stability policy (`wolf_fmt::STYLE_VERSION`)
/// — requests to configure are answered with the D34 rationale, not an
/// option.
fn fmt(args: &[String]) {
    let mut check = false;
    let mut paths: Vec<String> = Vec::new();
    for a in args {
        match a.as_str() {
            "--check" => check = true,
            "-" => paths.push("-".to_string()),
            _ if a.starts_with("--") => {
                eprintln!(
                    "wolf fmt: unknown flag `{a}` — wolf fmt has no options (D34): \
                     one canonical style, so no codebase ever argues about it"
                );
                std::process::exit(2);
            }
            _ => paths.push(a.clone()),
        }
    }
    if paths.is_empty() {
        eprintln!("usage: wolf fmt [--check] <file.lu|dir|->...");
        std::process::exit(2);
    }

    let mut sm = wolf_span::SourceMap::new();
    let mut sources = Sources::new();
    let mut unformatted: Vec<String> = Vec::new();
    let mut partials: Vec<Diagnostic> = Vec::new();
    let mut failed = false;

    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let mut use_stdin = false;
    for p in &paths {
        if p == "-" {
            use_stdin = true;
            continue;
        }
        let path = std::path::PathBuf::from(p);
        if path.is_dir() {
            collect_lu(&path, &mut files);
        } else if path.is_file() {
            files.push(path);
        } else {
            eprintln!("wolf fmt: no such file or directory: {p}");
            std::process::exit(2);
        }
    }
    files.sort();

    if use_stdin {
        use std::io::Read;
        let mut src = Vec::new();
        if std::io::stdin().read_to_end(&mut src).is_err() {
            eprintln!("wolf fmt: failed reading stdin");
            std::process::exit(2);
        }
        let id = sm.intern(Path::new("<stdin>"));
        sources.add(id, "<stdin>".to_string(), &src);
        let out = wolf_fmt::format_source(id, &src);
        if check {
            if out.text != src || out.partial {
                unformatted.push("<stdin>".to_string());
            }
        } else {
            use std::io::Write;
            let _ = std::io::stdout().write_all(&out.text);
        }
        if out.partial {
            partials.push(partial_diag(id, &src));
        }
    }

    for f in &files {
        let display = f.display().to_string().replace('\\', "/");
        let src = match std::fs::read(f) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("wolf fmt: read {display}: {e}");
                failed = true;
                continue;
            }
        };
        let id = sm.intern(f);
        sources.add(id, display.clone(), &src);
        let out = wolf_fmt::format_source(id, &src);
        // `--check` asks one question — is the file canonical? — so a
        // file with syntax errors whose formattable parts are already
        // canonical passes (the corpus keeps its deliberate
        // counter-examples without tripping CI). Write mode reports
        // partial work per the s11 contract.
        if out.partial && !check {
            partials.push(partial_diag(id, &src));
        }
        if out.text != src {
            if check {
                unformatted.push(display.clone());
            } else if let Err(e) = std::fs::write(f, &out.text) {
                eprintln!("wolf fmt: write {display}: {e}");
                failed = true;
            }
        }
    }

    // Report partial formats through the diagnostics engine (s10).
    if !partials.is_empty() {
        let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
        for d in &partials {
            reporter.report(d);
        }
        let rendered = reporter.take_output();
        if !rendered.is_empty() {
            eprint!("{rendered}");
        }
    }
    if check && !unformatted.is_empty() {
        for f in &unformatted {
            eprintln!("wolf fmt --check: {f} is not canonically formatted");
        }
    }
    if failed || !partials.is_empty() || (check && !unformatted.is_empty()) {
        std::process::exit(1);
    }
}

fn partial_diag(file: wolf_span::FileId, src: &[u8]) -> Diagnostic {
    let span = wolf_span::Span::new(file, 0, src.len().min(1) as u32);
    Diagnostic::warning(
        wolf_fmt::codes::PARTIAL_FORMAT,
        span,
        "this file has syntax errors, so it was only partially formatted",
    )
    .with_note(
        "regions with syntax errors (and one statement around them) were left \
         byte-for-byte untouched; fix the parse errors and run `wolf fmt` again"
            .to_string(),
    )
}

fn collect_lu(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    items.sort();
    for p in items {
        if p.is_dir() {
            collect_lu(&p, out);
        } else if p.extension().is_some_and(|e| e == "lu") {
            out.push(p);
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

/// Does this file's `//!` header carry `member: true` (s12: it belongs
/// to a multi-file module case and is compiled via its entry file)?
fn is_member_file(src: &[u8]) -> bool {
    let text = String::from_utf8_lossy(src);
    for line in text.lines() {
        let Some(rest) = line.trim_start().strip_prefix("//!") else {
            break;
        };
        if let Some(v) = rest.trim().strip_prefix("member:")
            && v.trim() == "true"
        {
            return true;
        }
    }
    false
}

/// Run s12 resolution from an entry file; registers every loaded file
/// with `sources` and returns the resolution (package + the full
/// parse/graph/resolution diagnostic set in deterministic order).
fn resolve_from_entry(
    entry: &Path,
    sm: &mut wolf_span::SourceMap,
    sources: &mut Sources,
) -> Result<wolf_sema::Resolution, String> {
    let mut loader =
        wolf_sema::DiskLoader::from_entry(entry, sm, Box::new(|src: &[u8]| is_member_file(src)))
            .ok_or_else(|| format!("cannot open package around {}", entry.display()))?;
    let res = wolf_sema::resolve_package(&mut loader, &wolf_sema::AliasTable::default())?;
    for unit in &res.package.files {
        sources.add(unit.raw.file, unit.raw.display.clone(), &unit.raw.src);
    }
    Ok(res)
}

/// `wolf interface <file-or-dir>` — resolve the package and pretty-print
/// every module's `wolfi` v0 interface, dependencies first (s12). A file
/// argument is an entry point (conform-run rules); a directory takes all
/// of its `.lu` files as the root module. Nothing is persisted — build
/// artifacts arrive with the incremental driver (s31).
fn interface(args: &[String]) {
    let Some(path) = args.first() else {
        eprintln!("usage: wolf interface <file.lu|dir>");
        std::process::exit(2);
    };
    let p = Path::new(path);
    let mut sm = wolf_span::SourceMap::new();
    let mut sources = Sources::new();
    let res = if p.is_file() {
        let mut loader = wolf_sema::DiskLoader::from_entry(
            p,
            &mut sm,
            Box::new(|src: &[u8]| is_member_file(src)),
        )
        .unwrap_or_else(|| {
            eprintln!("wolf interface: cannot open package around {path}");
            std::process::exit(2);
        });
        wolf_sema::resolve_package(&mut loader, &wolf_sema::AliasTable::default())
    } else if p.is_dir() {
        let mut loader = wolf_sema::DiskLoader::from_dir(p, &mut sm);
        wolf_sema::resolve_package(&mut loader, &wolf_sema::AliasTable::default())
    } else {
        eprintln!("wolf interface: no such file or directory: {path}");
        std::process::exit(2);
    };
    let res = match res {
        Ok(r) => r,
        Err(e) => {
            eprintln!("wolf interface: {e}");
            std::process::exit(2);
        }
    };
    for unit in &res.package.files {
        sources.add(unit.raw.file, unit.raw.display.clone(), &unit.raw.src);
    }
    let has_errors = res
        .diagnostics
        .iter()
        .any(|d| d.severity == wolf_diag::Severity::Error);
    if !res.diagnostics.is_empty() {
        let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
        for d in &res.diagnostics {
            reporter.report(d);
        }
        eprint!("{}", reporter.take_output());
    }
    if has_errors {
        eprintln!("wolf interface: the package does not resolve; fix the errors above");
        std::process::exit(1);
    }
    let ifaces = wolf_sema::build_interfaces(&res.package);
    // Sealed inferred rows (s15): module-private facts — shown for
    // humans, never serialized or hashed (private items are not
    // interface surface). `build_interfaces` iterates `pkg.topo`, so
    // ifaces[i] is module topo[i].
    let sigs = wolf_sema::build_sigs(&res.package);
    for (i, iface) in ifaces.iter().enumerate() {
        if i > 0 {
            println!();
        }
        print!("{}", wolf_sema::pretty(iface));
        let module = res.package.topo[i];
        let sealed: Vec<&(usize, String, String)> = sigs
            .sealed
            .iter()
            .filter(|(m, _, _)| *m == module)
            .collect();
        if !sealed.is_empty() {
            println!("  sealed rows (private, not hashed):");
            for (_, _, rendered) in sealed {
                println!("    {rendered}");
            }
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
/// `mem` (s18): each rung either stops with `fail(code)`, passes at
/// the requested rung, or falls through deeper; a file with any
/// NotYetCheckable body stops at the last completed rung +
/// `unsupported` — the conservatism ledger keeps showing exactly what
/// the compiler cannot do yet. `--phase=none` remains the pre-s07 stub
/// record.
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
    // The index→path table for diag-schema `files` (SourceMap intern
    // order): filled after the ladder runs, when every file the run
    // loaded has been interned.
    let mut files_table: Vec<String> = Vec::new();
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
        // Phase ladder, deepest implemented: mem (s18). Each rung either
        // stops with fail(code) at that rung, passes at the requested rung,
        // or falls through deeper; past the last rung the verdict is
        // `unsupported` (conservatism ledger).
        let result = if let Some(code) = first_error(&lexed.diagnostics) {
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
                // The resolve rung (s12): the module graph grows from
                // the entry file's directory; sibling files participate
                // when their header carries `member: true` (the corpus
                // multi-file contract), sibling directories are child
                // modules loaded on import.
                match resolve_from_entry(Path::new(&file), &mut sm, &mut sources) {
                    Err(e) => {
                        eprintln!("wolf conform-run: {e}");
                        std::process::exit(2);
                    }
                    Ok(res) => {
                        let all = res.diagnostics.clone();
                        if let Some(code) = first_error(&all) {
                            ("resolve", format!("fail({code})"), all)
                        } else if phase.as_deref() == Some("resolve") {
                            ("resolve", "pass".to_string(), all)
                        } else {
                            // The typecheck rung (s13). The ledger
                            // contract: every body Checked and clean ⇒
                            // the rung completes; ANY NotYetCheckable
                            // body ⇒ the rung was *not* completed —
                            // phase stays `resolve`, verdict
                            // `unsupported`, and partial type errors
                            // are withheld (conservatism, not silence:
                            // the file's verdict must not rest on a
                            // half-checked run). Type errors on a
                            // fully-checkable file fail here.
                            let tc = wolf_sema::typecheck_package(&res);
                            if !tc.not_yet.is_empty() {
                                ("resolve", "unsupported".to_string(), all)
                            } else {
                                let mut all = all;
                                all.extend(tc.diagnostics.iter().cloned());
                                wolf_diag::sort_diagnostics(&mut all);
                                if let Some(code) = first_error(&all) {
                                    ("typecheck", format!("fail({code})"), all)
                                } else if phase.as_deref() == Some("typecheck") {
                                    ("typecheck", "pass".to_string(), all)
                                } else {
                                    // The mem rung (s18): Tier-0
                                    // exclusivity over every typed
                                    // body. Same conservatism
                                    // contract as typecheck — ANY
                                    // NotYet (regions s19–s20,
                                    // shared/handle s21, unsafe s22,
                                    // closures c05) means the rung
                                    // was not completed and partial
                                    // memory errors are withheld.
                                    let mem = wolf_mem::check_package(&res.package, &tc);
                                    if !mem.not_yet.is_empty() {
                                        ("typecheck", "unsupported".to_string(), all)
                                    } else {
                                        let mut all = all;
                                        all.extend(mem.diagnostics.iter().cloned());
                                        wolf_diag::sort_diagnostics(&mut all);
                                        if let Some(code) = first_error(&all) {
                                            ("mem", format!("fail({code})"), all)
                                        } else if phase.as_deref() == Some("mem") {
                                            ("mem", "pass".to_string(), all)
                                        } else {
                                            ("mem", "unsupported".to_string(), all)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };
        files_table = sm
            .paths()
            .map(|p| p.display().to_string().replace('\\', "/"))
            .collect();
        result
    };

    // The rich diagnostic stream (s10) — stderr only, stdout is the
    // protocol's.
    let mut reporter: Box<dyn Reporter> = if error_format == "json" {
        // Every line carries the run's index→path table, so span file
        // indices are resolvable by consumers (the wolf-lsp harness's
        // secondary-file-table request; additive within schema v1).
        Box::new(JsonReporter::with_files(files_table))
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
