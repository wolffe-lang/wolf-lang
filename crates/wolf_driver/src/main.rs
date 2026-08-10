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

use std::path::{Path, PathBuf};

use wolf_diag::{Diagnostic, HumanReporter, JsonReporter, RenderOptions, Reporter, Sources};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        // D38: the compiler is named wolfgang; the command stays `wolf`.
        Some("--version") => println!("wolf {} (wolfgang)", env!("CARGO_PKG_VERSION")),
        Some("--explain") => explain(&args[1..]),
        Some("build") => build(&args[1..]),
        Some("conform-run") => conform_run(&args[1..]),
        Some("interface") => interface(&args[1..]),
        Some("audit-surface") => audit_surface(&args[1..]),
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

/// Split the std-root flag out of a subcommand's arguments (F-0001):
/// `--std-root <dir>` or `--std-root=<dir>` names the directory whose
/// subdirectories back `use std.…`. Returns the remaining arguments
/// and the flag's value; a flag with no directory is an error.
fn take_std_root(args: &[String]) -> Result<(Vec<String>, Option<PathBuf>), String> {
    let mut rest = Vec::new();
    let mut root = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(v) = a.strip_prefix("--std-root=") {
            root = Some(PathBuf::from(v));
        } else if a == "--std-root" {
            i += 1;
            let Some(v) = args.get(i) else {
                return Err("--std-root needs a directory".to_string());
            };
            root = Some(PathBuf::from(v));
        } else {
            rest.push(a.clone());
        }
        i += 1;
    }
    Ok((rest, root))
}

/// The effective std root: the `--std-root` flag wins, the `WOLF_STD`
/// environment variable is the fallback, neither keeps the prelude
/// stub answering `use std.…`. A configured root that is not a
/// directory is an error, never a silent fall-through to the stub.
fn effective_std_root(flag: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
    let root = flag.or_else(|| {
        std::env::var_os("WOLF_STD")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    });
    match root {
        Some(r) if !r.is_dir() => Err(format!(
            "std root is not a directory: {} (from --std-root or WOLF_STD)",
            r.display()
        )),
        other => Ok(other),
    }
}

/// Run s12 resolution from an entry file; registers every loaded file
/// with `sources` and returns the resolution (package + the full
/// parse/graph/resolution diagnostic set in deterministic order).
fn resolve_from_entry(
    entry: &Path,
    sm: &mut wolf_span::SourceMap,
    sources: &mut Sources,
    std_root: Option<&Path>,
) -> Result<wolf_sema::Resolution, String> {
    let mut loader =
        wolf_sema::DiskLoader::from_entry(entry, sm, Box::new(|src: &[u8]| is_member_file(src)))
            .ok_or_else(|| format!("cannot open package around {}", entry.display()))?
            .with_std_root(std_root.map(Path::to_path_buf));
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
    let (args, std_root) = match take_std_root(args).and_then(|(a, f)| {
        let root = effective_std_root(f)?;
        Ok((a, root))
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("wolf interface: {e}");
            std::process::exit(2);
        }
    };
    let Some(path) = args.first() else {
        eprintln!("usage: wolf interface [--std-root <dir>] <file.lu|dir>");
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
        })
        .with_std_root(std_root.clone());
        wolf_sema::resolve_package(&mut loader, &wolf_sema::AliasTable::default())
    } else if p.is_dir() {
        let mut loader =
            wolf_sema::DiskLoader::from_dir(p, &mut sm).with_std_root(std_root.clone());
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

/// `wolf audit-surface <file-or-dir>` — the I13 precursor (s22): the
/// package's complete unsafety inventory, one block per module —
/// trusted fns with their obligations, `unsafe`-block counts, `assume`
/// sites, re-entry doors, C imports, inline C/asm. Greppable,
/// diffable, CI-logged. Enforces the D11 ring-2 rule: a module with
/// `#[trusted]` code must be declared in `wolf.pkg`'s `trusted` entry
/// (E1303) — the mismatch is a build error (exit 1).
fn audit_surface(args: &[String]) {
    let (args, std_root) = match take_std_root(args).and_then(|(a, f)| {
        let root = effective_std_root(f)?;
        Ok((a, root))
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("wolf audit-surface: {e}");
            std::process::exit(2);
        }
    };
    let Some(path) = args.first() else {
        eprintln!("usage: wolf audit-surface [--std-root <dir>] <file.lu|dir>");
        std::process::exit(2);
    };
    let p = Path::new(path);
    let mut sm = wolf_span::SourceMap::new();
    let mut sources = Sources::new();
    let (res, root) = if p.is_file() {
        let mut loader = wolf_sema::DiskLoader::from_entry(
            p,
            &mut sm,
            Box::new(|src: &[u8]| is_member_file(src)),
        )
        .unwrap_or_else(|| {
            eprintln!("wolf audit-surface: cannot open package around {path}");
            std::process::exit(2);
        })
        .with_std_root(std_root.clone());
        (
            wolf_sema::resolve_package(&mut loader, &wolf_sema::AliasTable::default()),
            p.parent().unwrap_or(Path::new(".")).to_path_buf(),
        )
    } else if p.is_dir() {
        let mut loader =
            wolf_sema::DiskLoader::from_dir(p, &mut sm).with_std_root(std_root.clone());
        (
            wolf_sema::resolve_package(&mut loader, &wolf_sema::AliasTable::default()),
            p.to_path_buf(),
        )
    } else {
        eprintln!("wolf audit-surface: no such file or directory: {path}");
        std::process::exit(2);
    };
    let res = match res {
        Ok(r) => r,
        Err(e) => {
            eprintln!("wolf audit-surface: {e}");
            std::process::exit(2);
        }
    };
    for unit in &res.package.files {
        sources.add(unit.raw.file, unit.raw.display.clone(), &unit.raw.src);
    }
    if res
        .diagnostics
        .iter()
        .any(|d| d.severity == wolf_diag::Severity::Error)
    {
        eprintln!("wolf audit-surface: the package does not resolve; fix its errors first");
        std::process::exit(1);
    }
    let surfaces = wolf_sema::audit::surface(&res.package);
    print!("{}", wolf_sema::audit::render(&surfaces));
    // The ring-2 manifest rule (E1303): `wolf.pkg` next to the
    // package root, s51-stub format (`trusted = mod_a, mod_b`).
    let manifest = std::fs::read_to_string(root.join("wolf.pkg")).ok();
    let diags = wolf_sema::audit::manifest_check(&res.package, manifest.as_deref());
    if !diags.is_empty() {
        let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
        for d in &diags {
            reporter.report(d);
        }
        eprint!("{}", reporter.take_output());
        eprintln!(
            "wolf audit-surface: undeclared `#[trusted]` module(s) — the manifest is the deal"
        );
        std::process::exit(1);
    }
}

/// Why a native compile did not produce an executable — every case an
/// HONEST refusal or a user error, never a silent fallback.
enum BuildStop {
    /// Diagnostics were reported; the package does not compile.
    Errors,
    /// A construct the pipeline cannot handle yet (conservatism
    /// ledger): the deepest phase that DID complete + the reason.
    Refused { phase: &'static str, reason: String },
    /// Environment problem (no cc, missing libwolf_rt.a) — exit 2.
    Environment(String),
}

/// Compile one entry file to a native executable at `out` (s28: the
/// v0 `wolf build` pipeline — sema → mem → wir → CLIF → object → cc).
/// Diagnostics are rendered to stderr by the caller's `sources`.
fn compile_native(
    file: &Path,
    std_root: Option<&Path>,
    out: &Path,
    sources: &mut Sources,
) -> Result<(), BuildStop> {
    let mut sm = wolf_span::SourceMap::new();
    let res =
        resolve_from_entry(file, &mut sm, sources, std_root).map_err(BuildStop::Environment)?;
    let render = |sources: &Sources, diags: &[Diagnostic]| {
        let mut reporter = HumanReporter::new(sources, RenderOptions::default());
        for d in diags {
            reporter.report(d);
        }
        eprint!("{}", reporter.take_output());
    };
    let has_errors =
        |ds: &[Diagnostic]| ds.iter().any(|d| d.severity == wolf_diag::Severity::Error);
    if has_errors(&res.diagnostics) {
        render(sources, &res.diagnostics);
        return Err(BuildStop::Errors);
    }
    let tc = wolf_sema::typecheck_package(&res);
    if let Some(nyc) = tc.not_yet.first() {
        return Err(BuildStop::Refused {
            phase: "resolve",
            reason: format!("{} @{}..{}", nyc.construct, nyc.span.lo, nyc.span.hi),
        });
    }
    if has_errors(&tc.diagnostics) {
        render(sources, &tc.diagnostics);
        return Err(BuildStop::Errors);
    }
    let mem = wolf_mem::check_package(&res.package, &tc);
    if let Some(nyc) = mem.not_yet.first() {
        return Err(BuildStop::Refused {
            phase: "typecheck",
            reason: format!("{} @{}..{}", nyc.construct, nyc.span.lo, nyc.span.hi),
        });
    }
    if has_errors(&mem.diagnostics) {
        render(sources, &mem.diagnostics);
        return Err(BuildStop::Errors);
    }
    let build = wolf_wir::lower_package(&res.package, &tc);
    if let Some(nyc) = build.not_yet.first() {
        return Err(BuildStop::Refused {
            phase: "mem",
            reason: format!("{} @{}..{}", nyc.construct, nyc.span.lo, nyc.span.hi),
        });
    }
    let mut module = build.module;
    if let Err(e) = wolf_wir::verify_module(&module) {
        eprintln!("wolf build: ICE: lowered WIR failed verification\n{e}");
        std::process::exit(2);
    }
    // Backend: the driver drives the TRAIT; ClifBackend is the s28
    // implementation behind it (capabilities, never identity).
    let refuse = |phase: &'static str, e: wolf_backend::BackendError| match e {
        wolf_backend::BackendError::Unsupported(reason) => BuildStop::Refused { phase, reason },
        wolf_backend::BackendError::Internal(msg) => {
            eprintln!("wolf build: ICE: backend: {msg}");
            std::process::exit(2);
        }
    };
    let shim = wolf_codegen_clif::add_entry_shim(&mut module).map_err(|e| refuse("wir", e))?;
    let mut backend: Box<dyn wolf_backend::Backend> =
        Box::new(wolf_codegen_clif::ClifBackend::new().map_err(|e| refuse("wir", e))?);
    wolf_codegen_clif::compile_module(backend.as_mut(), &module, Some(shim))
        .map_err(|e| refuse("wir", e))?;
    let product = backend.finish().map_err(|e| refuse("wir", e))?;
    link_object(&product.bytes, out)
}

/// Link one relocatable object + `libwolf_rt.a` into an executable via
/// the system C compiler (the v0 link step; s31 owns the real driver
/// integration).
fn link_object(object: &[u8], out: &Path) -> Result<(), BuildStop> {
    let rt = find_rt_lib().ok_or_else(|| {
        BuildStop::Environment(
            "libwolf_rt.a not found next to the `wolf` binary (build it with \
             `cargo build -p wolf_rt`, or point WOLF_RT_LIB at it)"
                .to_string(),
        )
    })?;
    let obj_path = out.with_extension("o");
    std::fs::write(&obj_path, object)
        .map_err(|e| BuildStop::Environment(format!("write {}: {e}", obj_path.display())))?;
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let status = std::process::Command::new(&cc)
        .arg("-o")
        .arg(out)
        .arg(&obj_path)
        .arg(&rt)
        // What a Rust staticlib needs from the platform (linux x86-64,
        // the s28 target).
        .args(["-lpthread", "-ldl", "-lm"])
        .status()
        .map_err(|e| BuildStop::Environment(format!("cannot run `{cc}`: {e}")))?;
    let _ = std::fs::remove_file(&obj_path);
    if !status.success() {
        return Err(BuildStop::Environment(format!(
            "`{cc}` failed linking {}",
            out.display()
        )));
    }
    Ok(())
}

/// Locate `libwolf_rt.a`: `WOLF_RT_LIB` wins; otherwise next to the
/// running `wolf` binary (cargo puts both in target/<profile>/).
fn find_rt_lib() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("WOLF_RT_LIB").filter(|v| !v.is_empty()) {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.join("libwolf_rt.a");
    p.is_file().then_some(p)
}

/// `wolf build <file.lu> [-o OUT] [--std-root <dir>]` — the s28 v0
/// build: an entry file becomes a native executable. Refusals name the
/// construct and the deepest completed phase (the conservatism ledger
/// extends to codegen; nothing is silently interpreted instead).
fn build(args: &[String]) {
    let (args, std_root) = match take_std_root(args).and_then(|(a, f)| {
        let root = effective_std_root(f)?;
        Ok((a, root))
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("wolf build: {e}");
            std::process::exit(2);
        }
    };
    let mut file: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-o" {
            i += 1;
            match args.get(i) {
                Some(v) => out = Some(PathBuf::from(v)),
                None => {
                    eprintln!("wolf build: -o needs a path");
                    std::process::exit(2);
                }
            }
        } else if let Some(v) = a.strip_prefix("-o") {
            out = Some(PathBuf::from(v));
        } else if a.starts_with('-') {
            eprintln!("wolf build: unknown flag `{a}`");
            std::process::exit(2);
        } else {
            file = Some(a.clone());
        }
        i += 1;
    }
    let Some(file) = file else {
        eprintln!("usage: wolf build <file.lu> [-o OUT] [--std-root <dir>]");
        std::process::exit(2);
    };
    let path = Path::new(&file);
    if !path.is_file() {
        eprintln!("wolf build: no such file: {file}");
        std::process::exit(2);
    }
    let out = out.unwrap_or_else(|| {
        PathBuf::from(
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "a.out".to_string()),
        )
    });
    let mut sources = Sources::new();
    match compile_native(path, std_root.as_deref(), &out, &mut sources) {
        Ok(()) => {}
        Err(BuildStop::Errors) => {
            eprintln!("wolf build: the package does not compile; fix the errors above");
            std::process::exit(1);
        }
        Err(BuildStop::Refused { phase, reason }) => {
            eprintln!(
                "wolf build: cannot compile this yet — {reason} (pipeline is honest \
                 through `{phase}`; the conservatism ledger, not a bug in your program)"
            );
            std::process::exit(1);
        }
        Err(BuildStop::Environment(msg)) => {
            eprintln!("wolf build: {msg}");
            std::process::exit(2);
        }
    }
}

/// The s28 native rung for `conform-run --native`: compile the
/// mem-clean file to a real executable, run it, and report the run
/// verdict — `exit(N)`, or `trap(kind)` recovered from the
/// `wolf-trap:` stderr contract ([`wolf_rt::native`]). Refusals keep
/// the honest phase: `mem` when lowering refused, `wir` when the
/// backend did.
fn native_run(
    file: &Path,
    std_root: Option<&Path>,
    all: Vec<Diagnostic>,
    run_stdout: &mut Option<String>,
) -> (&'static str, String, Vec<Diagnostic>) {
    let dir = std::env::temp_dir().join(format!(
        "wolf-native-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("wolf conform-run: cannot create {}: {e}", dir.display());
        std::process::exit(2);
    }
    let exe = dir.join("a.out");
    let mut scratch_sources = Sources::new();
    let result = compile_native(file, std_root, &exe, &mut scratch_sources);
    let outcome = match result {
        Ok(()) => {
            let run = std::process::Command::new(&exe).output();
            match run {
                Err(e) => {
                    eprintln!("wolf conform-run: cannot run {}: {e}", exe.display());
                    std::process::exit(2);
                }
                Ok(o) => {
                    *run_stdout = Some(String::from_utf8_lossy(&o.stdout).into_owned());
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let trap_kind = stderr.lines().find_map(|l| {
                        l.trim()
                            .strip_prefix("wolf-trap:")
                            .map(|k| k.trim().to_string())
                    });
                    match (o.status.code(), trap_kind) {
                        (Some(code), Some(kind)) if code == wolf_rt::native::TRAP_EXIT_CODE => {
                            ("run", format!("trap({kind})"), all)
                        }
                        (Some(code), _) => ("run", format!("exit({code})"), all),
                        (None, _) => {
                            // Killed by a signal: not a defined wolf
                            // outcome — a compiler/runtime bug, never a
                            // verdict.
                            eprintln!(
                                "wolf conform-run: ICE: native binary died without an exit code"
                            );
                            std::process::exit(2);
                        }
                    }
                }
            }
        }
        Err(BuildStop::Errors) => {
            // The static ladder already ran clean before this rung; an
            // error here is a pipeline inconsistency.
            eprintln!("wolf conform-run: ICE: native rung found errors after a clean ladder");
            std::process::exit(2);
        }
        Err(BuildStop::Refused { phase, reason }) => {
            eprintln!("wolf conform-run --native: unsupported — {reason}");
            (phase, "unsupported".to_string(), all)
        }
        Err(BuildStop::Environment(msg)) => {
            eprintln!("wolf conform-run: {msg}");
            std::process::exit(2);
        }
    };
    let _ = std::fs::remove_dir_all(&dir);
    outcome
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

/// Execute a mem-clean package under the s23 miri-lite UB machine and
/// shape the run-rung observation: verdicts per `[proto.record.verdict]`
/// (`exit`/`trap`/`ub(mem.ub)`), UB details as `x-ub-*` extension keys
/// (row, clause, spans — the is04-compatible surface), and the E1401
/// diagnostic naming the row, the responsible operation, and the
/// licensed optimization (the D2 pairing, executable). An honest
/// refusal keeps the ladder at `mem`/`unsupported` — the conservatism
/// ledger, never a guess.
fn checked_run(
    pkg: &wolf_sema::Package,
    tc: &wolf_sema::Typecheck,
    mem: &wolf_mem::MemCheck,
    mut all: Vec<Diagnostic>,
    run_stdout: &mut Option<String>,
    x_ext: &mut Vec<(&'static str, serde_json::Value)>,
) -> (&'static str, String, Vec<Diagnostic>) {
    use wolf_mem::ubcheck::{self, Budget, Verdict};
    match ubcheck::run_checked(pkg, tc, Budget::default()) {
        Err(nyc) => {
            // Surface the refusal on stderr (the rich channel); the
            // record stays `unsupported` — the conservatism ledger.
            eprintln!(
                "wolf conform-run --checked: unsupported — {} @{}..{}",
                nyc.construct, nyc.span.lo, nyc.span.hi
            );
            ("mem", "unsupported".to_string(), all)
        }
        Ok(outcome) => {
            *run_stdout = Some(outcome.stdout);
            match outcome.verdict {
                Verdict::Exit(code) => ("run", format!("exit({code})"), all),
                Verdict::Trap(t) => {
                    x_ext.push(("x-trap-clause", serde_json::json!(t.clause)));
                    x_ext.push(("x-trap-span", serde_json::json!([t.span.lo, t.span.hi])));
                    ("run", format!("trap({})", t.kind), all)
                }
                Verdict::Ub(f) => {
                    x_ext.push(("x-ub-row", serde_json::json!(f.row.as_str())));
                    x_ext.push(("x-ub-clause", serde_json::json!(f.row.clause())));
                    x_ext.push(("x-ub-span", serde_json::json!([f.span.lo, f.span.hi])));
                    x_ext.push((
                        "x-ub-tag-span",
                        serde_json::json!([f.tag_span.lo, f.tag_span.hi]),
                    ));
                    // The s22 attribution fact the verdict traces to.
                    if let Some((op, span)) = ubcheck::attribute(&f, &mem.facts) {
                        x_ext.push(("x-ub-op", serde_json::json!(op)));
                        x_ext.push(("x-ub-op-span", serde_json::json!([span.lo, span.hi])));
                    }
                    all.push(ubcheck::ub_diagnostic(&f));
                    wolf_diag::sort_diagnostics(&mut all);
                    ("run", "ub(mem.ub)".to_string(), all)
                }
            }
        }
    }
}

/// The s25 `wir` rung: Braun-construct WIR from the typed HIR of a
/// mem-clean package. Any honest refusal keeps the ladder at
/// `mem`/`unsupported` (the conservatism ledger). A lowered module
/// that fails the verifier or the print→parse→print fixpoint is a
/// compiler bug — a deterministic ICE (exit 2), never a verdict.
fn wir_rung(
    pkg: &wolf_sema::Package,
    tc: &wolf_sema::Typecheck,
    phase: Option<&str>,
    zstats: bool,
    all: Vec<Diagnostic>,
) -> (&'static str, String, Vec<Diagnostic>) {
    let build = wolf_wir::lower_package(pkg, tc);
    if !build.not_yet.is_empty() {
        return ("mem", "unsupported".to_string(), all);
    }
    if let Err(e) = wolf_wir::verify_module(&build.module) {
        eprintln!("wolf conform-run: ICE: lowered WIR failed verification\n{e}");
        std::process::exit(2);
    }
    let printed = wolf_wir::print_module(&build.module);
    match wolf_wir::parse_module(&printed) {
        Ok(reparsed) if wolf_wir::print_module(&reparsed) == printed => {}
        Ok(_) => {
            eprintln!("wolf conform-run: ICE: WIR dump is not a print→parse→print fixpoint");
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("wolf conform-run: ICE: canonical WIR dump does not reparse: {e}");
            std::process::exit(2);
        }
    }
    if zstats {
        let s = build.stats;
        eprintln!(
            "wir-build: insts={} fold={} identity={} gvn={} forward={}",
            s.insts, s.fold, s.identity, s.gvn, s.forward
        );
    }
    if phase == Some("wir") {
        ("wir", "pass".to_string(), all)
    } else {
        ("wir", "unsupported".to_string(), all)
    }
}

/// A minimal, dependency-free SHA-256 (FIPS 180-4) for the observation
/// record's `stdout_sha256` — wolf stays build-script- and
/// heavy-dependency-free (D33/D15), and the protocol needs one hash.
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bitlen = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

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
    // The std root (F-0001): `--std-root <dir>` wins, `WOLF_STD` is
    // the fallback, neither keeps the prelude-stub `std`.
    let (args, std_root) = match take_std_root(args).and_then(|(a, f)| {
        let root = effective_std_root(f)?;
        Ok((a, root))
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("wolf conform-run: {e}");
            std::process::exit(2);
        }
    };
    let args = &args[..];
    let mut file = None;
    let mut phase: Option<String> = None;
    let mut error_format = "human".to_string();
    let mut dump: Option<String> = None;
    // The s23 miri-lite routing stub: `--checked` (or WOLF_CHECKED=1,
    // for harnesses that cannot add flags) executes a mem-clean file
    // under the UB machine and reports the run rung. Full `wolf run
    // --checked` integration lands s31.
    let mut checked = std::env::var("WOLF_CHECKED")
        .map(|v| v == "1")
        .unwrap_or(false);
    // The s28 native rung: `--native` (or WOLF_NATIVE=1) compiles a
    // mem-clean file through wir → Cranelift → cc and reports the RUN
    // verdict of the real binary — M1's first light. Refusals keep the
    // honest phase, exactly like `--checked`.
    let mut native = std::env::var("WOLF_NATIVE")
        .map(|v| v == "1")
        .unwrap_or(false);
    // `--zstats` (s25): peephole hit-rate counters from the wir rung's
    // builder, dumped on stderr — the Click claim, measured.
    let mut zstats = false;
    for a in args {
        if a == "--json" || a.starts_with("--seed=") {
            continue; // accepted per [proto.invoke.cli]
        }
        if a == "--checked" {
            checked = true;
            continue;
        }
        if a == "--native" {
            native = true;
            continue;
        }
        if let Some(f) = a.strip_prefix("--error-format=") {
            if f != "human" && f != "json" {
                eprintln!("wolf conform-run: unknown error format `{f}` (human, json)");
                std::process::exit(2);
            }
            error_format = f.to_string();
            continue;
        }
        if a == "--zstats" {
            zstats = true;
            continue;
        }
        if let Some(d) = a.strip_prefix("--dump=") {
            if d != "regions" && d != "cfg" && d != "wir" {
                eprintln!("wolf conform-run: unknown dump `{d}` (regions, cfg, wir)");
                std::process::exit(2);
            }
            dump = Some(d.to_string());
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
             [--error-format=human|json] [--dump=regions|cfg|wir] [--zstats] \
             [--checked] [--native] [--std-root <dir>]"
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
    // `--checked` run-rung observations (s23): the program's stdout
    // and the UB/trap extension keys ([proto.record.ext]).
    let mut run_stdout: Option<String> = None;
    let mut x_ext: Vec<(&'static str, serde_json::Value)> = Vec::new();
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
                match resolve_from_entry(
                    Path::new(&file),
                    &mut sm,
                    &mut sources,
                    std_root.as_deref(),
                ) {
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
                                        } else if checked {
                                            // The s23 miri-lite: a
                                            // mem-clean file executes
                                            // under the UB machine;
                                            // refusals stay in the
                                            // conservatism ledger.
                                            // (It interprets the typed
                                            // HIR directly; the wir
                                            // rung below serves the
                                            // default ladder.)
                                            checked_run(
                                                &res.package,
                                                &tc,
                                                &mem,
                                                all,
                                                &mut run_stdout,
                                                &mut x_ext,
                                            )
                                        } else if native {
                                            // The s28 native rung:
                                            // machine code, executed.
                                            native_run(
                                                Path::new(&file),
                                                std_root.as_deref(),
                                                all,
                                                &mut run_stdout,
                                            )
                                        } else {
                                            // The wir rung (s25):
                                            // lower what typechecked
                                            // and mem-passed; any
                                            // refusal keeps the
                                            // ladder at mem.
                                            wir_rung(
                                                &res.package,
                                                &tc,
                                                phase.as_deref(),
                                                zstats,
                                                all,
                                            )
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

    // The debug dump surface (s19): every lowerable body's region
    // inference (or check CFG) — stderr only, snapshot-reviewable;
    // stdout stays the protocol's.
    if let Some(kind) = &dump {
        let mut dump_sm = wolf_span::SourceMap::new();
        let mut dump_sources = Sources::new();
        if let Ok(res) = resolve_from_entry(
            Path::new(&file),
            &mut dump_sm,
            &mut dump_sources,
            std_root.as_deref(),
        ) {
            let tc = wolf_sema::typecheck_package(&res);
            let text = match kind.as_str() {
                "regions" => wolf_mem::dump_regions_package(&res.package, &tc),
                // The lowered WIR of every lowerable body (s25);
                // refusals are listed so the dump is never silently
                // partial.
                "wir" => {
                    let build = wolf_wir::lower_package(&res.package, &tc);
                    let mut text = wolf_wir::print_module(&build.module);
                    for nyc in &build.not_yet {
                        text.push_str(&format!(
                            "; not lowered: {} @{}..{}\n",
                            nyc.construct, nyc.span.lo, nyc.span.hi
                        ));
                    }
                    text
                }
                _ => wolf_mem::dump_package(&res.package, &tc),
            };
            eprint!("{text}");
        }
    }

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
    // `--checked` run observations: stdout hash/inline (required for
    // `exit` verdicts with output) and the x- extension keys.
    let (stdout_sha, stdout_inline) = match &run_stdout {
        Some(s) if !s.is_empty() => {
            let inline: String = s.chars().take(4096).collect();
            (
                serde_json::json!(sha256_hex(s.as_bytes())),
                serde_json::json!(inline),
            )
        }
        _ => (serde_json::Value::Null, serde_json::Value::Null),
    };
    let mut record = serde_json::json!({
        "protocol": 1,
        "impl": "wolfgang",
        "impl_version": env!("CARGO_PKG_VERSION"),
        "commit": option_env!("WOLF_COMMIT").unwrap_or("unknown"),
        "file": file.replace('\\', "/"),
        "phase_reached": phase_reached,
        "seeded": false,
        "diagnostics": minimal,
        "verdict": verdict,
        "stdout_sha256": stdout_sha,
        "stdout_inline": stdout_inline,
    });
    for (k, v) in x_ext {
        record[k] = v;
    }
    println!("{record}");
}
