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

use wolf_diag::lint::{AllowRegion, Level, LintLevels, Selector};
use wolf_diag::{Diagnostic, HumanReporter, JsonReporter, RenderOptions, Reporter, Sources};

mod cimport_cmd;
mod doc_cmd;
mod doctest_cmd;
mod pkg_cmd;
mod profile_cmd;
mod script_cmd;
mod test_cmd;

/// The reference-interpreter pairing (r01 criteria row 7): the lupin
/// version this wolf release is differentially tested against.
const LUPIN_PIN_VERSION: &str = "0.1.8";
/// The wolf-interp commit sha of the pairing. Placeholder until the
/// integrator stamps it at tag time (tags are the integrator's act);
/// a release tag never ships with this value unstamped.
const LUPIN_PIN_SHA: &str = "7886559";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        // D38: the compiler is named wolfgang; the command stays `wolf`.
        // Second line: the release pairing (r01 criteria row 7) — lupin is
        // the reference interpreter this release was differentially tested
        // against. LUPIN_PIN_SHA is stamped by the integrator at tag time.
        Some("--version") => {
            println!("wolf {} (wolfgang)", env!("CARGO_PKG_VERSION"));
            println!(
                "paired with lupin {LUPIN_PIN_VERSION} (reference interpreter), pin {LUPIN_PIN_SHA}"
            );
        }
        Some("--explain") => explain(&args[1..]),
        Some("build") => build(&args[1..]),
        Some("run") => run(&args[1..]),
        Some("fix") => fix(&args[1..]),
        Some("test") => test_cmd::test_cmd(&args[1..]),
        Some("conform-run") => conform_run(&args[1..]),
        Some("interface") => interface(&args[1..]),
        Some("audit-surface") => audit_surface(&args[1..]),
        // c10: the C header importer (s46). Runs a worker process; the
        // compiler never links a C frontend.
        Some("c-import") => cimport_cmd::c_import(&args[1..]),
        Some("fmt") => fmt(&args[1..]),
        Some("lsp") => lsp(&args[1..]),
        // The s51 package verbs (D34: the names are forever).
        Some("add") => pkg_cmd::add(&args[1..]),
        Some("rm") => pkg_cmd::rm(&args[1..]),
        Some("update") => pkg_cmd::update(&args[1..]),
        Some("audit") => pkg_cmd::audit(&args[1..]),
        Some("tree") => pkg_cmd::tree(&args[1..]),
        Some("why") => pkg_cmd::why(&args[1..]),
        // s53: documentation and the script-mode cache.
        Some("doc") => doc_cmd::doc(&args[1..]),
        Some("cache") => script_cmd::cache(&args[1..]),
        Some("profile") => profile_cmd::profile(&args[1..]),
        Some("init") => pkg_cmd::init(&args[1..]),
        // D34: the single binary grows per-campaign; stubs are honest.
        Some("vendor") => pkg_cmd::vendor(&args[1..]),
        Some(cmd @ ("bench" | "dbg" | "publish")) => {
            eprintln!("wolf {cmd}: not yet (grows at its own campaign; D34's single binary)");
            std::process::exit(2);
        }
        _ => {
            eprintln!(
                "usage: wolf build|run|test|doc|fix|fmt|lsp|init|add|rm|update|audit|tree|why|vendor|cache|profile|interface|audit-surface|c-import|conform-run|--explain|--version"
            );
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
///
/// `project` is the s51 dependency resolution, when the entry's
/// directory holds a real `wolf.pkg`: dep aliases become loader roots,
/// and a `std` path-dependency roots `use std.…` when neither the
/// `--std-root` flag nor `WOLF_STD` said otherwise (explicit beats
/// manifest).
fn resolve_from_entry(
    entry: &Path,
    sm: &mut wolf_span::SourceMap,
    sources: &mut Sources,
    std_root: Option<&Path>,
    project: Option<&wolf_pkg::Project>,
) -> Result<wolf_sema::Resolution, String> {
    let std_root = std_root
        .map(Path::to_path_buf)
        .or_else(|| project.and_then(|p| p.std_root.clone()));
    let mut loader =
        wolf_sema::DiskLoader::from_entry(entry, sm, Box::new(|src: &[u8]| is_member_file(src)))
            .ok_or_else(|| format!("cannot open package around {}", entry.display()))?
            .with_std_root(std_root);
    if let Some(p) = project {
        loader = loader.with_dep_roots(p.dep_roots.clone());
    }
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

/// What `wolf build` emits ([--emit], s31; `llvm-ir` added s41).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Emit {
    /// Link an executable (the default).
    Bin,
    /// Stop at relocatable objects.
    Obj,
    /// Dump the canonical WIR text (s24 round-trip format).
    Wir,
    /// Dump the release tier's LLVM IR (s41; requires `--release`).
    LlvmIr,
}

/// Build options (s31 driver v0).
struct BuildOpts {
    /// Bypass `.lu-cache` reads AND writes (the determinism oracle:
    /// a `--no-cache` build must be bit-identical to a cached one).
    no_cache: bool,
    /// Cache-hit accounting on stderr (the D7 acceptance surface).
    verbose: bool,
    /// The `--checked` profile: quarantine allocator + checked-tier
    /// runtime hooks (plumbing only at v0; matures in s54). Folds into
    /// every rebuild key.
    checked: bool,
    /// The `--release` tier (s41/c09): the LLVM backend behind the
    /// same seam, checked semantics IDENTICAL (X3 is
    /// profile-independent — only the speed changes). Folds into
    /// every rebuild key.
    release: bool,
    emit: Emit,
    /// Where `.lu-cache/` lives; `None` disables persistence entirely
    /// (conform-run's native rung — corpus directories must not grow
    /// cache droppings).
    cache_root: Option<PathBuf>,
    /// CLI lint levels (s67): `--allow/--warn/--deny <sel>`,
    /// `--deny-warnings`. Layered over the manifest's `lints.*` rules;
    /// source `#[allow]` attributes are the most local authority.
    lints: LintLevels,
    /// Render surviving warnings on stderr after a clean gate (true
    /// for `wolf build`/`run`; false for conform-run's native rung,
    /// whose own reporter renders the diagnostic set — twice would be
    /// noise).
    report_warnings: bool,
    /// `--codegen-report` (s43): dump the frozen summary index and the
    /// cluster/import decisions to stderr. OUR diagnostic surface, not
    /// a tuning knob — cluster count and size stay compiler-chosen
    /// (s43 non-target: no user-facing knobs at v1).
    codegen_report: bool,
    /// `--profile-gen[=<dir>]` (s45): build an INSTRUMENTED binary that
    /// writes a `.wprof` when it exits. The value is the directory the
    /// profile lands in; `None` means "not instrumented", which is
    /// every build that does not ask.
    profile_gen: Option<PathBuf>,
    /// `--profile=<f.wprof>` (s45): consume a profile. `None` is the
    /// default and is a NORMAL build — no warning, no nag, no
    /// degradation (D4: PGO is integrated, optional, never required).
    profile_use: Option<PathBuf>,
    /// How many diagnostics a report may print before it stops and
    /// counts the rest; `0` prints everything (`--error-limit=N`).
    /// A wall of errors is not more information than a screenful: a
    /// few kilobytes of garbage answered with hundreds of diagnostics
    /// across thousands of lines, which scrolls the *first* and most
    /// fixable one off the top of the terminal.
    error_limit: usize,
}

impl BuildOpts {
    fn ephemeral() -> BuildOpts {
        BuildOpts {
            no_cache: true,
            verbose: false,
            checked: false,
            release: false,
            emit: Emit::Bin,
            cache_root: None,
            lints: LintLevels::new(),
            report_warnings: false,
            codegen_report: false,
            profile_gen: None,
            profile_use: None,
            // The differential/conformance rungs compare whole
            // diagnostic sets: never truncate what a machine reads.
            error_limit: 0,
        }
    }
}

/// The default report cap. Big enough that a real program's error list
/// arrives whole, small enough that a wrecked file does not scroll the
/// first diagnostic away.
const DEFAULT_ERROR_LIMIT: usize = 25;

/// Render at most `limit` diagnostics, then say how many were held
/// back and how to see them. `limit == 0` means no cap.
fn report_capped(
    sources: &Sources,
    diags: &[Diagnostic],
    limit: usize,
    opts: RenderOptions,
) -> String {
    let mut reporter = HumanReporter::new(sources, opts);
    let shown = if limit == 0 {
        diags.len()
    } else {
        limit.min(diags.len())
    };
    for d in &diags[..shown] {
        reporter.report(d);
    }
    let mut out = reporter.take_output();
    if shown < diags.len() {
        let held = diags.len() - shown;
        let errors = diags[shown..]
            .iter()
            .filter(|d| d.severity == wolf_diag::Severity::Error)
            .count();
        out.push_str(&format!(
            "... and {held} more diagnostic{} ({errors} error{}) not shown. \
             Fix the first one — later reports are often echoes of it; \
             `--error-limit=0` prints them all.\n",
            if held == 1 { "" } else { "s" },
            if errors == 1 { "" } else { "s" },
        ));
    }
    out
}

/// Parse a lint selector as given on a flag: shape via
/// [`Selector::parse`], and exact codes must be registered — a `--deny
/// W9999` is a user error at the CLI (the *attribute* form warns W0302
/// instead, because source outlives toolchains; a flag does not).
fn parse_lint_selector(cmd: &str, flag: &str, s: &str) -> Selector {
    let fail = |msg: &str| -> ! {
        eprintln!("wolf {cmd}: {flag}: {msg}");
        std::process::exit(2);
    };
    let sel = match Selector::parse(s) {
        Ok(sel) => sel,
        Err(e) => fail(&e),
    };
    if let Selector::Code(code) = &sel
        && wolf_diag::explain(code).is_none()
    {
        fail(&format!(
            "`{code}` is not a registered diagnostic code (see docs/diagnostics.md)"
        ));
    }
    sel
}

/// The effective lint levels for a package: the manifest's `lints.*`
/// stub entries (wolf.pkg next to the entry file), with CLI rules
/// layered after so flags win ties. A malformed manifest is a build
/// error, never silently ignored.
fn effective_lints(entry: &Path, cli: &LintLevels) -> Result<LintLevels, String> {
    let mut levels = LintLevels::new();
    let root = entry.parent().unwrap_or(Path::new("."));
    if let Ok(manifest) = std::fs::read_to_string(root.join("wolf.pkg")) {
        for (sel, lvl) in wolf_diag::lint::parse_manifest_lints(&manifest)? {
            levels.set(sel, lvl);
        }
    }
    levels.extend_from(cli);
    Ok(levels)
}

/// One acquired unit: its object bytes plus the `--verbose`
/// accounting line, deferred so parallel cluster codegen still reports
/// in unit order.
type UnitResult = Result<(Vec<u8>, Option<String>), BuildStop>;

/// One compilation unit: a sema MODULE's functions on the debug tier
/// (s31, the D7 spine), a codegen CLUSTER's functions on the release
/// tier (s43 target 5) — plus its rebuild key. One shape, because the
/// cache, the DWARF split, and the entry/trap-table rule are the same
/// question either way; only the partition differs.
struct ModUnit {
    /// Cache-friendly display name (`root` for the package root on the
    /// debug tier; `c00`, `c01`, … for release clusters).
    name: String,
    funcs: Vec<wolf_wir::FuncId>,
    /// Full rebuild key (hex); the first 16 chars name the object.
    key: String,
    /// Key components, for cache-miss reason reporting.
    comps: KeyComps,
    /// This unit carries the entry shim + trap table.
    is_entry: bool,
}

/// The rebuild key's components (each a sha256 hex): toolchain/ABI/
/// profile environment, module source, direct-dep interface hashes,
/// the printed-WIR codegen input, and — on the release tier — the
/// whole-program summary state this unit rests on.
struct KeyComps {
    env: String,
    src: String,
    deps: String,
    wir: String,
    /// s43: `summary-format <v> <cluster content key>` for a release
    /// cluster, `-` for a debug-tier module unit (whose objects must
    /// NOT be invalidated by a summary-format bump — the dev loop is
    /// Tier-F's, D4's honest-tiers tradeoff).
    sum: String,
}

/// Compile one entry file to a native executable at `out` (s31: the
/// `wolf build` pipeline — sema → mem → wir → per-module CLIF objects
/// through the `.lu-cache` interface-hash skeleton → lld/cc link).
/// Diagnostics are rendered to stderr by the caller's `sources`.
fn compile_native(
    file: &Path,
    std_root: Option<&Path>,
    out: &Path,
    sources: &mut Sources,
    opts: &BuildOpts,
    // `resolved`: a dependency graph the caller already resolved —
    // script mode (s53), whose manifest rode in the script's `//!` block
    // rather than in a `wolf.pkg` on disk. `None` means "look for a
    // manifest next to the entry", the s51 project path.
    resolved: Option<&wolf_pkg::Project>,
) -> Result<(), BuildStop> {
    let mut sm = wolf_span::SourceMap::new();
    // The s51 hookup: a real `wolf.pkg` next to the entry file makes
    // this a *project* build — the dependency graph resolves first
    // (path trees in place, git trees via the pinned store; reading it
    // executes nothing, D33), and its aliases become loader roots.
    let root = file.parent().unwrap_or(Path::new("."));
    let owned_project = match resolved {
        Some(_) => None,
        None => pkg_cmd::project_for_build(root, &mut sm),
    };
    let pkg_project: Option<&wolf_pkg::Project> = resolved.or(owned_project.as_ref());
    if let Some(project) = pkg_project {
        for m in &project.manifests {
            sources.add(m.file, m.display.clone(), m.text.as_bytes());
        }
        if project.has_errors() {
            let mut reporter = HumanReporter::new(sources, RenderOptions::default());
            for d in &project.diagnostics {
                reporter.report(d);
            }
            eprint!("{}", reporter.take_output());
            return Err(BuildStop::Errors);
        }
    }
    let res = resolve_from_entry(file, &mut sm, sources, std_root, pkg_project)
        .map_err(BuildStop::Environment)?;
    // The warning system (s67): source `#[allow]` regions + manifest
    // levels + CLI levels decide each warning's fate — dropped,
    // reported, or promoted to an error that stops the build.
    let scan = wolf_sema::scan_allows(&res.package);
    let levels = effective_lints(file, &opts.lints).map_err(BuildStop::Environment)?;
    let limit = opts.error_limit;
    let render = move |sources: &Sources, diags: &[Diagnostic]| {
        eprint!(
            "{}",
            report_capped(sources, diags, limit, RenderOptions::default())
        );
    };
    let has_errors =
        |ds: &[Diagnostic]| ds.iter().any(|d| d.severity == wolf_diag::Severity::Error);
    // Each phase's diagnostics pass through the level machinery, then
    // accumulate; an error (original or deny-promoted) renders
    // EVERYTHING pending and stops. `pending` carries surviving
    // warnings across phases so a clean build still reports them.
    let mut pending: Vec<Diagnostic> = Vec::new();
    let gate = |sources: &Sources,
                pending: &mut Vec<Diagnostic>,
                diags: Vec<Diagnostic>|
     -> Result<(), BuildStop> {
        pending.extend(wolf_diag::lint::apply(&levels, &scan.allows, diags));
        if has_errors(pending) {
            wolf_diag::sort_diagnostics(pending);
            render(sources, pending);
            return Err(BuildStop::Errors);
        }
        Ok(())
    };
    let mut resolve_diags = res.diagnostics.clone();
    resolve_diags.extend(scan.diagnostics.iter().cloned());
    gate(sources, &mut pending, resolve_diags)?;
    // I13's import-graph half (s51): a package whose modules import a
    // capability-carrying std facade module must declare the
    // capability in its manifest — E1504, an error, never a warning.
    if let Some(project) = pkg_project {
        gate(
            sources,
            &mut pending,
            pkg_cmd::capability_diagnostics(project, &res.package),
        )?;
    }
    let tc = wolf_sema::typecheck_package(&res);
    if let Some(nyc) = tc.not_yet.first() {
        return Err(BuildStop::Refused {
            phase: "resolve",
            reason: format!("{} @{}..{}", nyc.construct, nyc.span.lo, nyc.span.hi),
        });
    }
    gate(sources, &mut pending, tc.diagnostics.clone())?;
    let mem = wolf_mem::check_package(&res.package, &tc);
    if let Some(nyc) = mem.not_yet.first() {
        return Err(BuildStop::Refused {
            phase: "typecheck",
            reason: format!("{} @{}..{}", nyc.construct, nyc.span.lo, nyc.span.hi),
        });
    }
    gate(sources, &mut pending, mem.diagnostics.clone())?;
    // No later phase produces diagnostics: report the surviving
    // warnings now, whatever `--emit` does next.
    if opts.report_warnings && !pending.is_empty() {
        wolf_diag::sort_diagnostics(&mut pending);
        render(sources, &pending);
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
    // `--emit=wir`: the canonical textual dump (s24 round-trip format)
    // — every stage inspectable. Deliberately BEFORE the mid-end: the
    // dump shows lowering's honest output; `--release --emit=llvm-ir`
    // shows the optimized module.
    if opts.emit == Emit::Wir {
        let text = wolf_wir::print_module(&module);
        std::fs::write(out, text)
            .map_err(|e| BuildStop::Environment(format!("write {}: {e}", out.display())))?;
        return Ok(());
    }
    // The Tier-R mid-end (s42) and the whole-program phase (s43).
    //
    // `--release` compiles the module graph as ONE program (D4: LTO is
    // the model, not a flag): the s42 pipeline runs per module, bodies
    // dedup by content hash, summaries drive clustering and the import
    // decision, and cross-cluster inlining follows. The debug tier
    // keeps the plain s42 pipeline — its own dev loop, its own
    // granularity (D4's honest tiers). `WOLF_MIDEND=1` forces the s42
    // pipeline onto the debug tier (the conc-conservatism differential
    // litmus runs spawn corpus through Tier-F with and without it).
    //
    // `WOLF_MIDEND=0` is the reverse hatch, and it exists for exactly
    // one consumer: s44's IR-volume lane (#70). The contract states the
    // budget as "LLVM IR handed to LLVM <= 50% of the NAIVE s41
    // lowering", so the naive lowering has to be reachable — otherwise
    // the denominator is a guess. It is a MEASUREMENT mode, never a
    // supported build mode: the mid-end and the whole-program phase are
    // both skipped, so the build falls back to per-module units with no
    // clustering, no dedup, and no cross-module inlining.
    let ice = |e: wolf_wir::VerifyError| -> ! {
        eprintln!("wolf build: ICE: mid-end broke the module\n{e}");
        std::process::exit(2);
    };
    let mid_stats = std::env::var("WOLF_MIDEND_STATS").as_deref() == Ok("1");
    let midend_off = std::env::var("WOLF_MIDEND").as_deref() == Ok("0");
    let mut whole: Option<wolf_wir::midend::WholeProgram> = None;
    // s45: the profile, if one was given. Read BEFORE the mid-end runs
    // — a `.wprof` this compiler cannot read is a loud build error, not
    // a silently ignored file. (Not being given one is not an error and
    // never says anything at all.)
    let profile = match &opts.profile_use {
        None => None,
        Some(p) => Some(load_profile(p)?),
    };
    let mut branch_weights: Option<wolf_codegen_llvm::BranchWeights> = None;
    if opts.release && !midend_off {
        let homes = wolf_wir::midend::summary::Homes::from_package(&res.package, &module);
        let wp = wolf_wir::midend::optimize_whole_program(
            &mut module,
            &homes,
            &wolf_wir::midend::Options {
                profile: profile.clone(),
                ..wolf_wir::midend::Options::default()
            },
        )
        .unwrap_or_else(|e| ice(e));
        if mid_stats {
            eprintln!("{}", wp.stats);
        }
        if opts.codegen_report {
            eprint!("{}", codegen_report(&wp));
        }
        // Stale tolerance (s45): ONE summary line, never an error, and
        // only when something was actually stale. A fully stale profile
        // produces exactly the no-profile build — the line says so
        // rather than leaving the user to wonder why PGO did nothing.
        if let (Some(p), Some(cov)) = (&opts.profile_use, &wp.stats.profile)
            && cov.stale() > 0
        {
            eprintln!(
                "wolf build: {}: {} of {} profile record(s) no longer match this build and were \
                 ignored{} — `wolf profile show {}` explains which",
                p.display(),
                cov.stale(),
                cov.records,
                if cov.fully_stale() {
                    " (nothing applied: this build is identical to one with no profile)"
                } else {
                    ""
                },
                p.display()
            );
        }
        // The block-count half of the profile, matched against the
        // FINAL bodies, on its way to LLVM as `!prof` branch weights.
        if let Some(p) = &profile {
            let w = wolf_wir::midend::branch_weights(&module, p);
            if !w.is_empty() {
                branch_weights = Some(std::sync::Arc::new(w));
            }
        }
        // Instrumentation is the LAST thing that touches the module:
        // the record key is the hash of the optimized body, so the
        // counters go in after that body is final.
        if let Some(dir) = &opts.profile_gen {
            let out_path = if dir.as_os_str().is_empty() {
                wolf_wir::midend::instrument::DEFAULT_PROFILE_FILE.to_string()
            } else {
                dir.join(wolf_wir::midend::instrument::DEFAULT_PROFILE_FILE)
                    .to_string_lossy()
                    .into_owned()
            };
            let st = wolf_wir::midend::instrument::instrument(&mut module, &out_path);
            if !st.has_entry {
                eprintln!(
                    "wolf build: --profile-gen: this module has no entry point, so nothing will \
                     ever write a profile"
                );
            } else if opts.verbose {
                eprintln!(
                    "wolf build: instrumented {} function(s), {} counter(s) -> {out_path}",
                    st.funcs, st.counters
                );
            }
        }
        whole = Some(wp);
    } else if std::env::var("WOLF_MIDEND").as_deref() == Ok("1") {
        let stats =
            wolf_wir::midend::optimize_module(&mut module, &wolf_wir::midend::Options::default())
                .unwrap_or_else(|e| ice(e));
        if mid_stats {
            eprintln!("{stats}");
        }
    }
    if (opts.release || std::env::var("WOLF_MIDEND").as_deref() == Ok("1"))
        && std::env::var("WOLF_MIDEND_DUMP").as_deref() == Ok("1")
    {
        eprintln!("{}", wolf_wir::print_module(&module));
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

    // `--emit=llvm-ir` (s41): the release tier's whole-module IR, one
    // inspectable text — every stage inspectable, like `--emit=wir`.
    if opts.emit == Emit::LlvmIr {
        let mut backend =
            wolf_codegen_llvm::LlvmBackend::with_options(emit_opts(branch_weights.clone()))
                .map_err(|e| refuse("wir", e))?;
        let all: Vec<wolf_wir::FuncId> = module.funcs.keys().collect();
        wolf_codegen_clif::compile_selected(
            &mut backend,
            &module,
            &all,
            Some(shim),
            true,
            false,
            &mut wolf_backend::NullDebugSink,
        )
        .map_err(|e| refuse("wir", e))?;
        std::fs::write(out, backend.module_ir())
            .map_err(|e| BuildStop::Environment(format!("write {}: {e}", out.display())))?;
        return Ok(());
    }

    // ---- per-module units (s31: the D7 spine, coarse and batch) ----
    let pkg = &res.package;
    // FileId index → sema module.
    let mut file_mod: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for (mi, md) in pkg.modules.iter().enumerate() {
        for &fi in &md.files {
            file_mod.insert(pkg.files[fi].raw.file.index() as u32, mi);
        }
    }
    // Group WIR functions by defining module; synthetic functions (the
    // entry shim) ride the root module's object (index 0 by s12
    // construction).
    let mut by_module: std::collections::HashMap<usize, Vec<wolf_wir::FuncId>> =
        std::collections::HashMap::new();
    for (fid, f) in module.funcs.iter() {
        let mi = f
            .src_file
            .and_then(|fi| file_mod.get(&fi).copied())
            .unwrap_or(0);
        by_module.entry(mi).or_default().push(fid);
    }
    // s12 interfaces, topo-indexed → per-module (pkg_hash covers the
    // package-visible surface: what in-package dependents can see).
    let ifaces = wolf_sema::build_interfaces(pkg);
    let mut iface_of: std::collections::HashMap<usize, &wolf_sema::Interface> =
        std::collections::HashMap::new();
    for (i, &mi) in pkg.topo.iter().enumerate() {
        iface_of.insert(mi, &ifaces[i]);
    }
    // A dep's key contribution is its SURFACE hash — items, impls,
    // dyn records, trusted rows — deliberately NOT `pkg_hash`, whose
    // header chains dep hashes transitively: a pub edit in a leaf must
    // rebuild its direct dependents (their sema rests on the surface)
    // but not the whole downstream cone (D7's two granularities; the
    // wir component still catches genuine transitive codegen effects).
    let surface_hash = |i: &wolf_sema::Interface| -> String {
        let mut acc = String::new();
        for it in &i.items {
            acc.push_str(&format!(
                "item {:?} {:?} {} {}\n",
                it.kind, it.vis, it.name, it.sig
            ));
        }
        for im in &i.impls {
            acc.push_str(&format!(
                "impl {} [{}]\n",
                im.header,
                im.rewrites.join(", ")
            ));
        }
        for d in &i.dyns {
            acc.push_str(&format!(
                "dyn {} {} [{}]\n",
                d.name,
                d.dyn_safe,
                d.methods.join(", ")
            ));
        }
        for (m, o) in &i.trusted {
            acc.push_str(&format!("trusted {m} {o}\n"));
        }
        sha256_hex(acc.as_bytes())
    };
    // D7: a `.wprof` is a BUILD INPUT, so it keys the build like any
    // other input. The tag below is the profile file's own content
    // hash, which means:
    //   - the same source with a different profile is a different key,
    //     so a stale object is never reused across a profile change;
    //   - the same source with the same profile hits the cache, and the
    //     hit is byte-identical to a cold build (the s43 invariant,
    //     unchanged);
    //   - a build with NO profile keys as `-`, exactly as before s45,
    //     so nothing about the default path moved.
    // The `.wprof` VERSION rides too: a reader change must invalidate
    // objects even when the file's bytes did not move.
    let pgo_comp = match (&opts.profile_gen, &opts.profile_use, &profile) {
        (Some(_), _, _) => "gen".to_string(),
        (None, Some(_), Some(p)) => format!(
            "use/{}/{}",
            wolf_wir::profile::WPROF_VERSION,
            &sha256_hex(p.render().as_bytes())[..16]
        ),
        _ => "-".to_string(),
    };
    let env_comp = format!(
        "wolf {} commit {} abi {} profile {} pgo {}",
        env!("CARGO_PKG_VERSION"),
        option_env!("WOLF_COMMIT").unwrap_or("unknown"),
        wolf_backend::abi::CONVENTION_VERSION,
        match (opts.release, opts.checked) {
            (true, _) => "release",
            (false, true) => "checked",
            (false, false) => "debug",
        },
        pgo_comp,
    );
    let mut units: Vec<ModUnit> = Vec::new();
    // ---- release: cluster units (s43 target 5) -------------------------
    //
    // Release-mode incrementality is deliberately COARSE (D4): the
    // object cache is keyed per cluster on (member + import body
    // hashes, summary format, toolchain/ABI/profile). A clean hit
    // skips LLVM for that cluster; no finer promise is made, because
    // the dev loop is Tier-F's job.
    if let Some(wp) = &whole {
        // Functions minted AFTER clustering (the entry shim) belong to
        // no cluster; they ride the one that owns `main`.
        let extras: Vec<wolf_wir::FuncId> = module
            .funcs
            .iter()
            .filter(|(_, f)| wolf_wir::midend::owner(&wp.clusters, &f.name).is_none())
            .map(|(fid, _)| fid)
            .collect();
        let extra_owner = wp
            .clusters
            .iter()
            .position(|c| c.members.iter().any(|m| m == "main"))
            .unwrap_or(0);
        for (i, c) in wp.clusters.iter().enumerate() {
            let mut funcs = wolf_wir::midend::member_ids(&module, c);
            if i == extra_owner {
                funcs.extend(extras.iter().copied());
                funcs.sort();
                funcs.dedup();
            }
            if funcs.is_empty() {
                continue;
            }
            let wir_text = wolf_wir::print_selected(&module, &funcs);
            let comps = KeyComps {
                env: env_comp.clone(),
                src: "-".to_string(),
                deps: "-".to_string(),
                wir: sha256_hex(format!("{wir_text}\ntags:{}", module.tags.join(",")).as_bytes()),
                sum: format!("summary-format {} {}", wp.stats.summary_version, c.key),
            };
            let key = sha256_hex(
                format!(
                    "{}\n{}\n{}\n{}\n{}\n{}",
                    comps.env, c.name, comps.src, comps.deps, comps.wir, comps.sum
                )
                .as_bytes(),
            );
            units.push(ModUnit {
                name: c.name.clone(),
                is_entry: funcs.contains(&shim),
                funcs,
                key,
                comps,
            });
        }
        // The entry shim must land in exactly one object.
        debug_assert_eq!(
            units.iter().filter(|u| u.is_entry).count(),
            1,
            "exactly one cluster carries the entry shim"
        );
    }
    for &mi in &pkg.topo {
        if whole.is_some() {
            break; // clusters replaced the module partition
        }
        let Some(funcs) = by_module.get(&mi) else {
            continue; // type-only modules produce no object
        };
        let md = &pkg.modules[mi];
        let dotted = md.dotted();
        let name = if dotted.is_empty() {
            "root".to_string()
        } else {
            dotted
        };
        // (a) module source: (FileId index, display, bytes) — the file
        // indices anchor the DWARF file table, so they key too.
        let mut src_acc = String::new();
        for &fi in &md.files {
            let raw = &pkg.files[fi].raw;
            src_acc.push_str(&format!("{}#{}\n", raw.file.index(), raw.display));
            src_acc.push_str(&sha256_hex(&raw.src));
            src_acc.push('\n');
        }
        // (b) direct dep interface hashes (pkg_hash chains through dep
        // export hashes, so transitive interface changes flow).
        let mut deps_acc = String::new();
        for &d in &md.deps {
            let h = iface_of
                .get(&d)
                .map(|i| surface_hash(i))
                .unwrap_or_default();
            deps_acc.push_str(&format!("{}={h}\n", pkg.modules[d].dotted()));
        }
        // (c) the exact codegen input: the canonical printed WIR of
        // this module's functions (D8 — catches everything the source
        // ⊕ iface key cannot see: CTFE folds from dep bodies, the
        // package-global error-tag table, convention-visible types).
        let wir_text = wolf_wir::print_selected(&module, funcs);
        let comps = KeyComps {
            env: env_comp.clone(),
            src: sha256_hex(src_acc.as_bytes()),
            deps: sha256_hex(deps_acc.as_bytes()),
            wir: sha256_hex(format!("{wir_text}\ntags:{}", module.tags.join(",")).as_bytes()),
            sum: "-".to_string(),
        };
        let key = sha256_hex(
            format!(
                "{}\n{}\n{}\n{}\n{}\n{}",
                comps.env, name, comps.src, comps.deps, comps.wir, comps.sum
            )
            .as_bytes(),
        );
        let is_entry = funcs.contains(&shim);
        units.push(ModUnit {
            name,
            funcs: funcs.clone(),
            key,
            comps,
            is_entry,
        });
    }

    // The cache root (`.lu-cache/`, D7): `--no-cache` bypasses reads
    // AND writes — the determinism oracle builds fully fresh.
    //
    // An INSTRUMENTED build never touches the object cache at all
    // (s45): its objects are not release artifacts, they are a
    // measuring instrument, and the surest way to keep one out of a
    // release binary is for it never to be written down. The key would
    // have kept them apart on its own; not writing them means the
    // question cannot arise.
    let cache = if opts.no_cache || opts.profile_gen.is_some() {
        None
    } else {
        opts.cache_root.as_ref().map(|r| r.join(".lu-cache"))
    };
    // s12 interface-file emission: every module's `.wolfi` + content
    // hash persists next to the objects it keys.
    if let Some(cache) = &cache {
        let ifdir = cache.join("ifaces");
        let _ = std::fs::create_dir_all(&ifdir);
        for (i, &mi) in pkg.topo.iter().enumerate() {
            let dotted = pkg.modules[mi].dotted();
            let name = if dotted.is_empty() { "root" } else { &dotted };
            let _ = std::fs::write(
                ifdir.join(format!("{name}.wolfi")),
                wolf_sema::encode(&ifaces[i]),
            );
        }
    }

    // Acquire object bytes per unit: cache hit or compile. Clusters
    // lower through the backend INDEPENDENTLY (s43 target 4), so the
    // release tier fans them out over the machine's cores — one
    // backend instance per worker, results collected IN UNIT ORDER so
    // the link inputs (and the accounting lines) stay byte-stable
    // whatever the thread schedule did (D4).
    let acquire = |u: &ModUnit| -> UnitResult {
        let obj_file = cache
            .as_ref()
            .map(|c| c.join("obj").join(format!("{}-{}.o", u.name, &u.key[..16])));
        let manifest_file = cache
            .as_ref()
            .map(|c| c.join("modules").join(format!("{}.json", u.name)));
        if let Some(p) = &obj_file
            && p.is_file()
        {
            let bytes = std::fs::read(p)
                .map_err(|e| BuildStop::Environment(format!("read {}: {e}", p.display())))?;
            let log = opts.verbose.then(|| {
                format!(
                    "wolf build: {}: reused object (key {})",
                    u.name,
                    &u.key[..16]
                )
            });
            return Ok((bytes, log));
        }
        let log = opts.verbose.then(|| {
            let old = manifest_file
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
            let reason = match old {
                None => "new".to_string(),
                Some(old) => {
                    if old["env"].as_str() != Some(&u.comps.env) {
                        "toolchain/profile changed".to_string()
                    } else if old["src"].as_str() != Some(&u.comps.src) {
                        "source changed".to_string()
                    } else if old["deps"].as_str() != Some(&u.comps.deps) {
                        "dep interface changed".to_string()
                    } else if old["sum"].as_str() != Some(&u.comps.sum) {
                        "summary/cluster changed".to_string()
                    } else if old["wir"].as_str() != Some(&u.comps.wir) {
                        "codegen input changed".to_string()
                    } else {
                        "object missing".to_string()
                    }
                }
            };
            format!("wolf build: {}: compiled ({reason})", u.name)
        });
        let bytes = compile_unit(&module, u, shim, pkg, opts.release, branch_weights.clone())?;
        if let Some(p) = &obj_file {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
                // Prune stale keys of this unit (one live object per
                // unit; the cache never grows unboundedly).
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for e in entries.flatten() {
                        let fname = e.file_name().to_string_lossy().into_owned();
                        if fname.starts_with(&format!("{}-", u.name))
                            && fname.ends_with(".o")
                            && e.path() != *p
                        {
                            let _ = std::fs::remove_file(e.path());
                        }
                    }
                }
            }
            let _ = std::fs::write(p, &bytes);
        }
        if let Some(p) = &manifest_file {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let manifest = serde_json::json!({
                "key": u.key,
                "env": u.comps.env,
                "src": u.comps.src,
                "deps": u.comps.deps,
                "wir": u.comps.wir,
                "sum": u.comps.sum,
            });
            let _ = std::fs::write(p, manifest.to_string());
        }
        Ok((bytes, log))
    };
    let codegen_start = std::time::Instant::now();
    let acquired: Vec<UnitResult> = if whole.is_some() && units.len() > 1 {
        use rayon::prelude::*;
        units.par_iter().map(&acquire).collect()
    } else {
        units.iter().map(&acquire).collect()
    };
    let codegen_wall = codegen_start.elapsed();
    let mut objects: Vec<(String, Vec<u8>)> = Vec::new();
    for (u, got) in units.iter().zip(acquired) {
        let (bytes, log) = got?;
        if let Some(log) = log {
            eprintln!("{log}");
        }
        objects.push((u.name.clone(), bytes));
    }
    if opts.verbose {
        // Phase-resolved codegen wall time: the number the parallel
        // scaling lane scrapes (the frontend and mid-end ahead of it
        // are serial, so total wall would understate the phase). No
        // colon after `codegen` — the cache-accounting scrapers key on
        // `wolf build: <unit>: <verdict>`.
        eprintln!(
            "wolf build: codegen {} unit(s) in {:.3}s",
            units.len(),
            codegen_wall.as_secs_f64()
        );
    }

    // `--emit=obj`: write the relocatable objects and stop.
    if opts.emit == Emit::Obj {
        if let [(_, bytes)] = objects.as_slice() {
            std::fs::write(out, bytes)
                .map_err(|e| BuildStop::Environment(format!("write {}: {e}", out.display())))?;
        } else {
            for (name, bytes) in &objects {
                let p = out.with_extension(format!("{name}.o"));
                std::fs::write(&p, bytes)
                    .map_err(|e| BuildStop::Environment(format!("write {}: {e}", p.display())))?;
            }
        }
        return Ok(());
    }
    link_objects(&objects, out)
}

/// `--codegen-report` (s43): the whole-program phase's decisions, in
/// the frozen summary format plus one line per cluster. OUR diagnostic
/// surface — the partition itself stays compiler-chosen.
fn codegen_report(wp: &wolf_wir::midend::WholeProgram) -> String {
    let mut out = String::from("wolf codegen-report\n");
    out.push_str(&wp.summary.render());
    for c in &wp.clusters {
        out.push_str(&format!(
            "cluster {} size={} key={} members=[{}] imports=[{}]\n",
            c.name,
            c.size,
            &c.key[..16],
            c.members.join(","),
            c.imports.join(",")
        ));
    }
    out.push_str(&format!("{}\n", wp.stats));
    out
}

/// Release-tier emission options: `WOLF_STRIP_FACTS=1` lowers with
/// EVERY fact channel silenced — no `noalias`/`readonly`/`deref`, no
/// scoped-noalias, no `!range`/`!invariant.load`/`!prof`, alignment
/// claims dropped to 1.
///
/// This is s44's metadata-drop sentinel (D42 ruling 3), the permanent
/// nightly lane that PRICES the bonus channel: metadata is a bonus (D2)
/// — our own mid-end exploits the same facts in WIR, so dropping it
/// costs speed, never correctness. Compiling the A-family both ways per
/// commit is how channel decay (an LLVM bump quietly ignoring us) shows
/// up as a number instead of as a mystery. Never a supported build
/// mode; the flag is deliberately an env var, not a CLI switch.
/// `branch_weights` is s45's measured `!prof` channel; `None` (no
/// profile) leaves the IR byte-identical to a pre-s45 build, and
/// `WOLF_STRIP_FACTS=1` silences it with every other channel — a
/// measured weight is a fact like any other, and the sentinel prices
/// it beside them.
fn emit_opts(
    branch_weights: Option<wolf_codegen_llvm::BranchWeights>,
) -> wolf_codegen_llvm::EmitOptions {
    wolf_codegen_llvm::EmitOptions {
        strip_facts: std::env::var("WOLF_STRIP_FACTS").as_deref() == Ok("1"),
        branch_weights,
    }
}

/// Read a `.wprof` for `--profile=`. A file the compiler cannot read is
/// a LOUD build error: silently continuing without it would make "PGO
/// did nothing" indistinguishable from "the file was a typo", and the
/// format's own rule is work-or-refuse, never misread.
fn load_profile(path: &Path) -> Result<wolf_wir::profile::Profile, BuildStop> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| BuildStop::Environment(format!("--profile={}: {e}", path.display())))?;
    wolf_wir::profile::Profile::parse(&text)
        .map_err(|e| BuildStop::Environment(format!("--profile={}: {e}", path.display())))
}

/// Compile one module unit to relocatable object bytes: its own
/// backend instance, its own DWARF builder over ONLY its files (the
/// object must not observe other modules' file tables — cache keys
/// don't fold them), trap table + entry shim only in the entry unit.
/// `release` selects the LLVM tier (s41) behind the SAME trait — the
/// driver branches on capabilities below, never on identity.
fn compile_unit(
    module: &wolf_wir::Module,
    u: &ModUnit,
    shim: wolf_wir::FuncId,
    pkg: &wolf_sema::Package,
    release: bool,
    branch_weights: Option<wolf_codegen_llvm::BranchWeights>,
) -> Result<Vec<u8>, BuildStop> {
    let refuse = |e: wolf_backend::BackendError| match e {
        wolf_backend::BackendError::Unsupported(reason) => BuildStop::Refused {
            phase: "wir",
            reason,
        },
        wolf_backend::BackendError::Internal(msg) => {
            eprintln!("wolf build: ICE: backend: {msg}");
            std::process::exit(2);
        }
    };
    let mut backend: Box<dyn wolf_backend::Backend> = if release {
        Box::new(
            wolf_codegen_llvm::LlvmBackend::with_options(emit_opts(branch_weights))
                .map_err(refuse)?,
        )
    } else {
        Box::new(wolf_codegen_clif::ClifBackend::new().map_err(refuse)?)
    };
    // DWARF v0 (s30): the debug tier always carries debug info — the
    // Cranelift backend IS the debug tier (release is the LLVM tier,
    // s41). The builder collects the DebugSink stream per object.
    let mut dwarf = if backend.capabilities().dwarf_fidelity != wolf_backend::DwarfFidelity::None {
        let mut files = std::collections::HashMap::new();
        let unit_files: std::collections::HashSet<u32> = u
            .funcs
            .iter()
            .filter_map(|&f| module.funcs[f].src_file)
            .collect();
        for unit in &pkg.files {
            let idx = unit.raw.file.index() as u32;
            if !unit_files.contains(&idx) {
                continue;
            }
            let mut line_starts = vec![0u32];
            for (i, &b) in unit.raw.src.iter().enumerate() {
                if b == b'\n' {
                    line_starts.push(i as u32 + 1);
                }
            }
            files.insert(
                idx,
                wolf_backend::dwarf::SourceFile {
                    path: unit.raw.display.clone(),
                    line_starts,
                },
            );
        }
        let comp_dir = std::env::current_dir()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|_| ".".to_string());
        Some(wolf_backend::dwarf::DwarfBuilder::new(comp_dir, files))
    } else {
        None
    };
    let mut null = wolf_backend::NullDebugSink;
    let sink: &mut dyn wolf_backend::DebugSink = match dwarf.as_mut() {
        Some(d) => d,
        None => &mut null,
    };
    wolf_codegen_clif::compile_selected(
        backend.as_mut(),
        module,
        &u.funcs,
        u.is_entry.then_some(shim),
        u.is_entry,
        true,
        sink,
    )
    .map_err(refuse)?;
    if let Some(dwarf) = &dwarf {
        let sections = dwarf.finish().unwrap_or_else(|e| {
            eprintln!("wolf build: ICE: DWARF emission failed: {e}");
            std::process::exit(2);
        });
        backend.add_debug_sections(sections).map_err(refuse)?;
    }
    let product = backend.finish().map_err(refuse)?;
    Ok(product.bytes)
}

/// Probe (once) for `ld.lld` on PATH; `Some("-fuse-ld=lld")` routes
/// the link through lld (D1: lld ships with the toolchain — packaging
/// is c13/s66's problem, the probe is v0's honest posture).
fn lld_fuse_flag() -> Option<&'static str> {
    use std::sync::OnceLock;
    static PROBE: OnceLock<bool> = OnceLock::new();
    let have = *PROBE.get_or_init(|| {
        std::process::Command::new("ld.lld")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
    });
    have.then_some("-fuse-ld=lld")
}

/// Link the module objects + `libwolf_rt.a` into an executable via the
/// system C driver, `-fuse-ld=lld` when the probe finds lld. Objects
/// are staged under deterministic temp names so cached and `--no-cache`
/// links see identical inputs (the CI determinism check's ground).
fn link_objects(objects: &[(String, Vec<u8>)], out: &Path) -> Result<(), BuildStop> {
    let rt = find_rt_lib().ok_or_else(|| {
        BuildStop::Environment(
            "libwolf_rt.a not found next to the `wolf` binary (build it with \
             `cargo build -p wolf_rt`, or point WOLF_RT_LIB at it)"
                .to_string(),
        )
    })?;
    let dir = std::env::temp_dir().join(format!(
        "wolf-link-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir)
        .map_err(|e| BuildStop::Environment(format!("create {}: {e}", dir.display())))?;
    let mut paths = Vec::new();
    for (i, (name, bytes)) in objects.iter().enumerate() {
        let p = dir.join(format!("{i:02}-{name}.o"));
        std::fs::write(&p, bytes)
            .map_err(|e| BuildStop::Environment(format!("write {}: {e}", p.display())))?;
        paths.push(p);
    }
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let mut cmd = std::process::Command::new(&cc);
    cmd.arg("-o").arg(out);
    for p in &paths {
        cmd.arg(p);
    }
    // What a Rust staticlib needs from the platform (linux x86-64,
    // the s28 target).
    cmd.arg(&rt).args(["-lpthread", "-ldl", "-lm"]);
    // s32 Target 1 ("a wolf binary that never spawns is a C binary"):
    // rustc emits function sections, so section GC drops every
    // wolf_rt entry point the program never calls — the no-spawn CI
    // test asserts no scheduler symbols survive in a no-spawn binary.
    // Debug info is unaffected (non-alloc sections are not collected).
    #[cfg(target_os = "macos")]
    cmd.arg("-Wl,-dead_strip");
    #[cfg(not(target_os = "macos"))]
    cmd.arg("-Wl,--gc-sections");
    if let Some(flag) = lld_fuse_flag() {
        cmd.arg(flag);
    }
    let status = cmd
        .status()
        .map_err(|e| BuildStop::Environment(format!("cannot run `{cc}`: {e}")));
    let _ = std::fs::remove_dir_all(&dir);
    let status = status?;
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

/// Parsed `wolf build`/`wolf run` command line.
struct BuildCli {
    file: String,
    out: Option<PathBuf>,
    std_root: Option<PathBuf>,
    opts: BuildOpts,
    /// Arguments after the file (`wolf run prog.lu -- args…` or just
    /// trailing words): the program's argv.
    prog_args: Vec<String>,
    /// s53 script-mode posture (`--locked`, `--update`, `--offline`,
    /// `--yes`). Ignored by `wolf build` of a non-script.
    script: script_cmd::Posture,
}

/// Parse the shared build/run flag surface (s31): `-o|--out`,
/// `--emit=wir|obj|bin|llvm-ir`, `--no-cache`, `--verbose`,
/// `--checked`, `--debug` (the default; accepted), `--release` (the
/// s41 LLVM tier), `--std-root`.
fn parse_build_cli(cmd: &str, args: &[String], run_mode: bool) -> BuildCli {
    let usage = || -> ! {
        eprintln!(
            "usage: wolf {cmd} <file.lu> [-o OUT] [--emit=wir|obj|bin|llvm-ir] [--no-cache] \
             [--verbose] [--checked] [--release] [--codegen-report] \
             [--profile-gen[=<dir>]] [--profile=<file.wprof>] \
             [--std-root <dir>] \
             [--allow|--warn|--deny <W####|W##xx|warnings>] [--deny-warnings] \
             [--error-limit=N]{}",
            if run_mode { " [prog args…]" } else { "" }
        );
        std::process::exit(2);
    };
    let fail = |msg: &str| -> ! {
        eprintln!("wolf {cmd}: {msg}");
        std::process::exit(2);
    };
    let (args, std_root) = match take_std_root(args).and_then(|(a, f)| {
        let root = effective_std_root(f)?;
        Ok((a, root))
    }) {
        Ok(v) => v,
        Err(e) => fail(&e),
    };
    let mut file: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut opts = BuildOpts {
        no_cache: false,
        verbose: false,
        checked: false,
        release: false,
        emit: Emit::Bin,
        cache_root: None,
        lints: LintLevels::new(),
        report_warnings: true,
        codegen_report: false,
        profile_gen: None,
        profile_use: None,
        error_limit: DEFAULT_ERROR_LIMIT,
    };
    let mut prog_args: Vec<String> = Vec::new();
    let mut script = script_cmd::Posture::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if run_mode && file.is_some() {
            // Everything after the file belongs to the program.
            prog_args.push(a.clone());
            i += 1;
            continue;
        }
        if a == "-o" || a == "--out" {
            i += 1;
            match args.get(i) {
                Some(v) => out = Some(PathBuf::from(v)),
                None => fail("-o/--out needs a path"),
            }
        } else if let Some(v) = a.strip_prefix("--out=") {
            out = Some(PathBuf::from(v));
        } else if let Some(v) = a.strip_prefix("-o") {
            out = Some(PathBuf::from(v));
        } else if let Some(e) = a.strip_prefix("--emit=") {
            opts.emit = match e {
                "wir" => Emit::Wir,
                "obj" => Emit::Obj,
                "bin" => Emit::Bin,
                "llvm-ir" => Emit::LlvmIr,
                other => fail(&format!("unknown emit `{other}` (wir, obj, bin, llvm-ir)")),
            };
        } else if let Some(v) = a.strip_prefix("--error-limit=") {
            match v.parse::<usize>() {
                Ok(n) => opts.error_limit = n,
                Err(_) => fail("--error-limit needs a count (0 for no limit)"),
            }
        } else if a == "--deny-warnings" {
            opts.lints.deny_warnings();
        } else if let Some((flag, level)) = match a.as_str() {
            "--allow" => Some(("--allow", Level::Allow)),
            "--warn" => Some(("--warn", Level::Warn)),
            "--deny" => Some(("--deny", Level::Deny)),
            _ => None,
        } {
            i += 1;
            match args.get(i) {
                Some(v) => opts.lints.set(parse_lint_selector(cmd, flag, v), level),
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
            opts.lints.set(parse_lint_selector(cmd, flag, v), level);
        } else if a == "--no-cache" {
            opts.no_cache = true;
        } else if a == "--verbose" {
            opts.verbose = true;
        } else if a == "--checked" {
            opts.checked = true;
        } else if a == "--debug" {
            // The default tier: DWARF on, Cranelift. Accepted for
            // symmetry.
        } else if a == "--release" {
            // The s41 LLVM tier. Checked semantics are IDENTICAL (X3
            // is profile-independent) — only the speed changes.
            opts.release = true;
        } else if a == "--codegen-report" {
            // s43: dump the summary index + cluster/import decisions.
            opts.codegen_report = true;
        } else if a == "--profile-gen" {
            // s45: instrument, dumping `default.wprof` in the CWD.
            opts.profile_gen = Some(PathBuf::new());
        } else if let Some(v) = a.strip_prefix("--profile-gen=") {
            if v.is_empty() {
                fail("--profile-gen=<dir> needs a directory (or pass bare --profile-gen)");
            }
            opts.profile_gen = Some(PathBuf::from(v));
        } else if let Some(v) = a.strip_prefix("--profile=") {
            if v.is_empty() {
                fail("--profile=<file.wprof> needs a path");
            }
            opts.profile_use = Some(PathBuf::from(v));
        } else if a == "--profile" {
            i += 1;
            match args.get(i) {
                Some(v) => opts.profile_use = Some(PathBuf::from(v)),
                None => fail("--profile needs a path to a .wprof"),
            }
        } else if a == "--locked" {
            script.locked = true;
        } else if a == "--update" {
            script.update = true;
        } else if a == "--offline" {
            script.offline = true;
        } else if a == "--yes" || a == "-y" {
            script.yes = true;
        } else if a.starts_with('-') {
            fail(&format!("unknown flag `{a}`"));
        } else {
            file = Some(a.clone());
        }
        i += 1;
    }
    let Some(file) = file else { usage() };
    if opts.emit == Emit::LlvmIr && !opts.release {
        fail("--emit=llvm-ir is the release tier's IR; pass --release");
    }
    // Both PGO flags are release-tier surface: the consumers are the
    // s42 inliner, the s43 clusterer and the LLVM tier's block
    // placement, none of which the debug tier runs. Saying so is
    // better than silently instrumenting a build that will never use
    // the result.
    if opts.profile_gen.is_some() && !opts.release {
        fail("--profile-gen is a release-tier build; pass --release");
    }
    if opts.profile_use.is_some() && !opts.release {
        fail("--profile= is consumed by the release tier; pass --release");
    }
    if opts.profile_gen.is_some() && opts.profile_use.is_some() {
        fail("--profile-gen and --profile are the two halves of PGO, not one build");
    }
    if !Path::new(&file).is_file() {
        fail(&format!("no such file: {file}"));
    }
    // A bare file name has an empty parent; anchor it so the package
    // root (= the entry's directory) is a readable path.
    let file = if Path::new(&file)
        .parent()
        .is_none_or(|p| p.as_os_str().is_empty())
    {
        format!("./{file}")
    } else {
        file
    };
    // The cache lives at the package root (`.lu-cache/` next to the
    // entry file).
    opts.cache_root = Path::new(&file).parent().map(Path::to_path_buf);
    BuildCli {
        file,
        out,
        std_root,
        opts,
        prog_args,
        script,
    }
}

fn report_build_stop(cmd: &str, stop: BuildStop) -> ! {
    match stop {
        BuildStop::Errors => {
            // Every code carries an explanation, and nothing in the
            // report said so: a reader who does not already know the
            // catalog exists never finds it.
            eprintln!(
                "wolf {cmd}: the package does not compile; fix the errors above \
                 (`wolf --explain E0201` explains any code by name)"
            );
            std::process::exit(1);
        }
        BuildStop::Refused { phase, reason } => {
            eprintln!(
                "wolf {cmd}: cannot compile this yet — {reason} (pipeline is honest \
                 through `{phase}`; the conservatism ledger, not a bug in your program)"
            );
            std::process::exit(1);
        }
        BuildStop::Environment(msg) => {
            eprintln!("wolf {cmd}: {msg}");
            std::process::exit(2);
        }
    }
}

/// `wolf build <file.lu> [-o OUT] [--emit=…] [--no-cache] [--verbose]
/// [--checked] [--std-root <dir>]` — an entry file becomes a native
/// executable through the `.lu-cache` rebuild skeleton (s31). Refusals
/// name the construct and the deepest completed phase (the
/// conservatism ledger extends to codegen; nothing is silently
/// interpreted instead).
fn build(args: &[String]) {
    let cli = parse_build_cli("build", args, false);
    if lld_fuse_flag().is_none() && cli.opts.emit == Emit::Bin {
        eprintln!(
            "wolf build: note: ld.lld not found on PATH — linking with the system \
             linker (lld ships with the toolchain; packaging is c13)"
        );
    }
    let path = Path::new(&cli.file);
    let out = cli.out.clone().unwrap_or_else(|| {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "a.out".to_string());
        match cli.opts.emit {
            Emit::Bin => PathBuf::from(stem),
            Emit::Obj => PathBuf::from(format!("{stem}.o")),
            Emit::Wir => PathBuf::from(format!("{stem}.wir")),
            Emit::LlvmIr => PathBuf::from(format!("{stem}.ll")),
        }
    });
    let mut sources = Sources::new();
    match compile_native(
        path,
        cli.std_root.as_deref(),
        &out,
        &mut sources,
        &cli.opts,
        None,
    ) {
        Ok(()) => {}
        Err(stop) => report_build_stop("build", stop),
    }
}

/// `wolf run <file.lu> [prog args…]` — build into the package cache
/// and exec, exit code propagated (the cargo-run sugar promised at
/// s12). All build flags apply; the binary lands in `.lu-cache/bin/`.
fn run(args: &[String]) {
    let mut cli = parse_build_cli("run", args, true);
    if cli.opts.emit != Emit::Bin {
        eprintln!("wolf run: --emit makes no sense here; use `wolf build`");
        std::process::exit(2);
    }
    // s53 script mode: a file that announces itself with `#!` or with a
    // `pkg { … }` frontmatter block is a first-class package whose state
    // lives in the cache, never next to the script.
    let text = std::fs::read_to_string(&cli.file).unwrap_or_default();
    if script_cmd::is_script(&text) {
        run_script(&mut cli, &text);
    }
    let path = Path::new(&cli.file);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "a.out".to_string());
    let out = match &cli.out {
        Some(o) => o.clone(),
        None => match (&cli.opts.cache_root, cli.opts.no_cache) {
            (Some(root), false) => {
                let bin = root.join(".lu-cache").join("bin");
                if let Err(e) = std::fs::create_dir_all(&bin) {
                    eprintln!("wolf run: create {}: {e}", bin.display());
                    std::process::exit(2);
                }
                bin.join(&stem)
            }
            _ => std::env::temp_dir().join(format!("wolf-run-{}-{stem}", std::process::id())),
        },
    };
    let mut sources = Sources::new();
    match compile_native(
        path,
        cli.std_root.as_deref(),
        &out,
        &mut sources,
        &cli.opts,
        None,
    ) {
        Ok(()) => {}
        Err(stop) => report_build_stop("run", stop),
    }
    let status = std::process::Command::new(&out)
        .args(&cli.prog_args)
        .status();
    match status {
        Ok(s) => match s.code() {
            Some(code) => std::process::exit(code),
            None => {
                eprintln!("wolf run: program terminated by a signal");
                std::process::exit(2);
            }
        },
        Err(e) => {
            eprintln!("wolf run: cannot run {}: {e}", out.display());
            std::process::exit(2);
        }
    }
}

/// The script-mode half of `wolf run` (s53). Never returns: a script
/// either execs its program or exits with a reason.
///
/// The warm path is the point. Second run: read the file, hash it with
/// its pinned resolution, stat one binary, exec. No resolver, no
/// network, no compiler phase — the "instant on the second run" claim,
/// as code rather than as an aspiration.
fn run_script(cli: &mut BuildCli, text: &str) -> ! {
    let path = PathBuf::from(&cli.file);
    let mut sm = wolf_span::SourceMap::new();
    let mut sources = Sources::new();
    let mut text = text.to_string();
    let mut script = script_cmd::prepare(&path, &text, cli.script, &mut sm, &mut sources);
    // The prompt (s53 §3): in SCRIPT mode a missing dependency is a
    // question — project mode keeps the s51 error-with-fix-it, and
    // nothing here changes that. Answered once, then re-resolved: the
    // accepted entry is real frontmatter, so the second pass runs the
    // ordinary path with nothing special about it.
    let missing = script_cmd::missing_imports(&path, &text, script.project.as_ref());
    if !missing.is_empty() {
        let mut edited = false;
        for alias in &missing {
            let Some(source) = script_cmd::prompt_add(&path, alias, cli.script) else {
                std::process::exit(1);
            };
            let Some(manifest) = script_cmd::manifest_of(&path, &text) else {
                eprintln!(
                    "wolf run: `{alias}` needs a frontmatter block to be added to; \
                     write `//! pkg {{ deps: {{ … }} }}` at the head of the script"
                );
                std::process::exit(1);
            };
            match script_cmd::insert_dep(&text, &manifest, alias, &source) {
                Some(next) => {
                    text = next;
                    edited = true;
                }
                None => {
                    eprintln!("wolf run: cannot place `{alias}` in the frontmatter");
                    std::process::exit(1);
                }
            }
        }
        if edited {
            if let Err(e) = std::fs::write(&path, &text) {
                eprintln!("wolf run: write {}: {e}", path.display());
                std::process::exit(2);
            }
            let mut sm = wolf_span::SourceMap::new();
            sources = Sources::new();
            script = script_cmd::prepare(&path, &text, cli.script, &mut sm, &mut sources);
        }
    }
    let out = cli.out.clone().unwrap_or_else(|| script.binary());
    let exec = |out: &Path| -> ! {
        let status = std::process::Command::new(out)
            .args(&cli.prog_args)
            .status();
        match status {
            Ok(s) => match s.code() {
                Some(code) => std::process::exit(code),
                None => {
                    eprintln!("wolf run: program terminated by a signal");
                    std::process::exit(2);
                }
            },
            Err(e) => {
                eprintln!("wolf run: cannot run {}: {e}", out.display());
                std::process::exit(2);
            }
        }
    };
    // Warm: the artifact under this build key already exists, and a
    // build key covers every input that could change the artifact.
    if cli.opts.verbose {
        eprintln!(
            "wolf run: script {} — pin {}, build {}",
            &script.id[..12],
            script.state.display(),
            &script.build_key[..12]
        );
    }
    if !cli.opts.no_cache && cli.out.is_none() && script.warm() {
        if cli.opts.verbose {
            eprintln!("wolf run: warm — no resolver, no compiler, no network");
        }
        exec(&out);
    }
    if let Err(e) = std::fs::create_dir_all(&script.build_dir) {
        eprintln!("wolf run: create {}: {e}", script.build_dir.display());
        std::process::exit(2);
    }
    // Artifacts live in the cache, so the script's own directory is
    // never written to (s53: "no cache writes beyond builds/").
    cli.opts.cache_root = Some(script.build_dir.clone());
    let mut sources = Sources::new();
    match compile_native(
        &path,
        cli.std_root.as_deref(),
        &out,
        &mut sources,
        &cli.opts,
        script.project.as_ref(),
    ) {
        Ok(()) => {}
        Err(stop) => report_build_stop("run", stop),
    }
    exec(&out);
}

/// `wolf fix <file.lu> [--apply] [--std-root <dir>]` — promote
/// machine-applicable suggestions to applied edits (s67; the s10
/// suggestion machinery, D34's promised subcommand).
///
/// Dry-run by default: lists every fix it would make and exits 1 so
/// scripts can gate on "fixes pending"; `--apply` writes the files.
/// Only `MachineApplicable` suggestions are ever applied (the
/// applicability contract), the first such suggestion per diagnostic;
/// suggestions whose edits overlap an already-accepted fix are skipped
/// with a note. Applying a fix removes the diagnostic that carried it,
/// so the command is idempotent: a second run finds nothing to do.
fn fix(args: &[String]) {
    let (args, std_root) = match take_std_root(args).and_then(|(a, f)| {
        let root = effective_std_root(f)?;
        Ok((a, root))
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("wolf fix: {e}");
            std::process::exit(2);
        }
    };
    let mut apply = false;
    let mut file: Option<String> = None;
    for a in &args {
        match a.as_str() {
            "--apply" => apply = true,
            _ if a.starts_with('-') => {
                eprintln!("wolf fix: unknown flag `{a}`");
                std::process::exit(2);
            }
            _ => file = Some(a.clone()),
        }
    }
    let Some(file) = file else {
        eprintln!("usage: wolf fix <file.lu> [--apply] [--std-root <dir>]");
        std::process::exit(2);
    };
    let path = Path::new(&file);
    if !path.is_file() {
        eprintln!("wolf fix: no such file: {file}");
        std::process::exit(2);
    }
    let mut sm = wolf_span::SourceMap::new();
    let mut sources = Sources::new();
    let res = match resolve_from_entry(path, &mut sm, &mut sources, std_root.as_deref(), None) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("wolf fix: {e}");
            std::process::exit(2);
        }
    };
    // The ladder, tolerant of errors (fixes often live ON errors):
    // each phase's diagnostics are collected as deep as the pipeline
    // honestly gets; allowed warnings never offer their fixes.
    let scan = wolf_sema::scan_allows(&res.package);
    let levels = match effective_lints(path, &LintLevels::new()) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("wolf fix: {e}");
            std::process::exit(2);
        }
    };
    let has_errors =
        |ds: &[Diagnostic]| ds.iter().any(|d| d.severity == wolf_diag::Severity::Error);
    let mut diags = res.diagnostics.clone();
    diags.extend(scan.diagnostics.iter().cloned());
    if !has_errors(&diags) {
        let tc = wolf_sema::typecheck_package(&res);
        if tc.not_yet.is_empty() {
            diags.extend(tc.diagnostics.iter().cloned());
            if !has_errors(&diags) {
                let mem = wolf_mem::check_package(&res.package, &tc);
                if mem.not_yet.is_empty() {
                    diags.extend(mem.diagnostics.iter().cloned());
                }
            }
        }
    }
    let mut diags = wolf_diag::lint::apply(&levels, &scan.allows, diags);
    wolf_diag::sort_diagnostics(&mut diags);

    // FileId index → (display path, source bytes).
    let mut by_file: std::collections::HashMap<u32, (String, Vec<u8>)> =
        std::collections::HashMap::new();
    for unit in &res.package.files {
        by_file.insert(
            unit.raw.file.index() as u32,
            (unit.raw.display.clone(), unit.raw.src.clone()),
        );
    }
    // Accept suggestions greedily in diagnostic order; per-file
    // interval sets refuse overlaps so fixes never fight.
    let mut accepted: std::collections::HashMap<u32, Vec<(u32, u32)>> =
        std::collections::HashMap::new();
    let mut edits_by_file: std::collections::HashMap<u32, Vec<(wolf_span::Span, String)>> =
        std::collections::HashMap::new();
    let mut planned: Vec<String> = Vec::new();
    let mut skipped = 0usize;
    for d in &diags {
        let Some(sug) = d
            .suggestions
            .iter()
            .find(|s| s.applicability == wolf_diag::Applicability::MachineApplicable)
        else {
            continue;
        };
        let conflicts = sug.edits.iter().any(|(sp, _)| {
            accepted
                .get(&(sp.file.index() as u32))
                .is_some_and(|ivs| ivs.iter().any(|&(lo, hi)| sp.lo < hi && lo < sp.hi))
        });
        if conflicts {
            skipped += 1;
            continue;
        }
        for (sp, rep) in &sug.edits {
            let fi = sp.file.index() as u32;
            accepted.entry(fi).or_default().push((sp.lo, sp.hi));
            edits_by_file
                .entry(fi)
                .or_default()
                .push((*sp, rep.clone()));
        }
        let loc = by_file
            .get(&(d.span().file.index() as u32))
            .map(|(p, src)| {
                let line = 1 + src[..(d.span().lo as usize).min(src.len())]
                    .iter()
                    .filter(|&&b| b == b'\n')
                    .count();
                format!("{p}:{line}")
            })
            .unwrap_or_else(|| "?".to_string());
        planned.push(format!("{loc}: {}: {}", d.code, sug.message));
    }
    if planned.is_empty() {
        eprintln!("wolf fix: nothing to fix");
        if skipped > 0 {
            eprintln!("wolf fix: {skipped} overlapping fix(es) skipped — rerun after applying");
        }
        std::process::exit(0);
    }
    for p in &planned {
        println!("{}{p}", if apply { "fixed " } else { "would fix " });
    }
    if !apply {
        eprintln!(
            "wolf fix: {} fix(es) pending — rerun with --apply to write them",
            planned.len()
        );
        std::process::exit(1);
    }
    let mut files_written = 0usize;
    for (fi, edits) in &edits_by_file {
        let Some((display, src)) = by_file.get(fi) else {
            continue;
        };
        let Some(out) = wolf_diag::suggest::apply_edits(src, edits) else {
            // Individually accepted edits cannot overlap; a refusal
            // here is a registry bug worth a loud exit.
            eprintln!("wolf fix: ICE: accepted edits failed to apply in {display}");
            std::process::exit(2);
        };
        if let Err(e) = std::fs::write(display, out) {
            eprintln!("wolf fix: write {display}: {e}");
            std::process::exit(2);
        }
        files_written += 1;
    }
    eprintln!(
        "wolf fix: applied {} fix(es) across {} file(s){}",
        planned.len(),
        files_written,
        if skipped > 0 {
            format!("; {skipped} overlapping fix(es) skipped — run `wolf fix` again")
        } else {
            String::new()
        }
    );
}

/// The s28 native rung for `conform-run --native`: compile the
/// mem-clean file to a real executable, run it, and report the run
/// verdict — `exit(N)`, or `trap(kind)` recovered from the
/// `wolf-trap:` stderr contract ([`wolf_rt::native`]). Refusals keep
/// the honest phase: `mem` when lowering refused, `wir` when the
/// backend did. `release` routes codegen through the s41 LLVM tier —
/// the differential lane that holds the two tiers to ONE behavior.
fn native_run(
    file: &Path,
    std_root: Option<&Path>,
    release: bool,
    all: Vec<Diagnostic>,
    run_stdout: &mut Option<String>,
    seed: Option<u64>,
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
    // Fully ephemeral: the native rung must not grow `.lu-cache`
    // droppings in corpus directories.
    let opts = BuildOpts {
        release,
        ..BuildOpts::ephemeral()
    };
    let result = compile_native(file, std_root, &exe, &mut scratch_sources, &opts, None);
    let outcome = match result {
        Ok(()) => {
            let mut cmd = std::process::Command::new(&exe);
            if let Some(s) = seed {
                // The one seed the runtime consumes (hooks.rs's
                // `root_seed`): steal victims, select tie-breaks.
                cmd.env("WOLF_SCHED_SEED", s.to_string());
            }
            let run = cmd.output();
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

/// Say WHY a rung declined, on stderr — the rich channel the record
/// deliberately keeps empty.
///
/// The `unsupported` verdict is the conservatism ledger: the file's
/// verdict must not rest on a half-checked run, so the record carries
/// no partial diagnostics. That is a rule about the RECORD; it was
/// never a reason for the compiler to say nothing at all. Three
/// findings asked for this half in a row (wolf-std F-0060, F-0069 and
/// F-0077 / wolf-lang#101, #47) because an unattributable
/// `unsupported` costs a reader a bisect to learn which subset they
/// left. The `--checked` and `--native` rungs already speak here; the
/// static rungs now speak in the same words, and `xtask lane-coverage`
/// reads them off stderr for the residue report.
fn report_refusal(nyc: Option<&wolf_sema::check::NotYet>) {
    if let Some(nyc) = nyc {
        eprintln!(
            "wolf conform-run: unsupported — {} @{}..{}",
            nyc.construct, nyc.span.lo, nyc.span.hi
        );
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
            // The program's `eprint` channel reaches the real stderr
            // (parity with the native lane); the record never hashes
            // it — stdout is the compared channel (spec/06).
            eprint!("{}", outcome.stderr);
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
        report_refusal(build.not_yet.first());
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
            "wir-build: insts={} fold={} identity={} gvn={} forward={} \
             instantiations_seen={} instantiations_lowered={}",
            s.insts,
            s.fold,
            s.identity,
            s.gvn,
            s.forward,
            s.instantiations_seen,
            s.instantiations_lowered
        );
    }
    if phase == Some("wir") {
        ("wir", "pass".to_string(), all)
    } else {
        ("wir", "unsupported".to_string(), all)
    }
}

/// SHA-256 (FIPS 180-4) for the observation record's `stdout_sha256`
/// and every `.lu-cache` rebuild key. The implementation lives in
/// `wolf_wir::hash` — ONE hash for the toolchain, shared with the D8
/// content addressing the whole-program phase does over bodies (s43
/// moved it there; wolf stays build-script- and heavy-dependency-free,
/// D33/D15).
fn sha256_hex(data: &[u8]) -> String {
    wolf_wir::sha256_hex(data)
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
    // `--release` (or WOLF_RELEASE=1) routes the native rung through
    // the s41 LLVM tier — the corpus/differ release lane.
    let mut release = std::env::var("WOLF_RELEASE")
        .map(|v| v == "1")
        .unwrap_or(false);
    // `--zstats` (s25): peephole hit-rate counters from the wir rung's
    // builder, dumped on stderr — the Click claim, measured.
    let mut zstats = false;
    // s73: `--seed=N` reaches the native runtime (`WOLF_SCHED_SEED`
    // drives the s32/s36 scheduler PRNG) — `[proto.seed.flag]` made
    // real for the native rung; the record reports `seeded` honestly.
    let mut seed: Option<u64> = None;
    for a in args {
        if a == "--json" {
            continue; // accepted per [proto.invoke.cli]
        }
        if let Some(v) = a.strip_prefix("--seed=") {
            match v.parse::<u64>() {
                Ok(n) => seed = Some(n),
                Err(_) => {
                    eprintln!("wolf conform-run: --seed needs a decimal u64");
                    std::process::exit(2);
                }
            }
            continue;
        }
        if a == "--checked" {
            checked = true;
            continue;
        }
        if a == "--native" {
            native = true;
            continue;
        }
        if a == "--release" {
            release = true;
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
            if d != "regions" && d != "cfg" && d != "wir" && d != "peel" {
                eprintln!("wolf conform-run: unknown dump `{d}` (regions, cfg, wir, peel)");
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
             [--checked] [--native] [--release] [--std-root <dir>]"
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
    // Source `#[allow]` regions (s67): part of the program, so the
    // conformance surface honors them — collected at the resolve rung,
    // applied to the final diagnostic set.
    let mut allow_regions: Vec<AllowRegion> = Vec::new();
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
                    None,
                ) {
                    Err(e) => {
                        eprintln!("wolf conform-run: {e}");
                        std::process::exit(2);
                    }
                    Ok(res) => {
                        // The s67 allow scan: regions for the final
                        // filter, W030x lints into the resolve rung.
                        let scan = wolf_sema::scan_allows(&res.package);
                        allow_regions = scan.allows;
                        let mut all = res.diagnostics.clone();
                        all.extend(scan.diagnostics);
                        wolf_diag::sort_diagnostics(&mut all);
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
                                report_refusal(tc.not_yet.first());
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
                                        report_refusal(mem.not_yet.first());
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
                                        } else if native || release {
                                            // The s28 native rung:
                                            // machine code, executed
                                            // (s41: --release selects
                                            // the LLVM tier).
                                            native_run(
                                                Path::new(&file),
                                                std_root.as_deref(),
                                                release,
                                                all,
                                                &mut run_stdout,
                                                seed,
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
            None,
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
                // The survey lens (`cargo xtask peel`): the ledger's
                // fail-fast reasons plus what they mask. `ledger`
                // lines are exactly the wir rung's refusals; `behind`
                // lines were collected by skipping the failed
                // statement and lowering on, so they may be follow-on
                // noise — leads for a contract author, never a gate.
                // Not snapshotted anywhere, by design.
                "peel" => {
                    let (build, reasons) = wolf_wir::lower_package_survey(&res.package, &tc);
                    let mut text = String::new();
                    for r in &reasons {
                        if r.follow_on {
                            text.push_str(&format!(
                                "peel: behind: {} @{}..{} (fn {}, follow-on possible)\n",
                                r.construct, r.span.lo, r.span.hi, r.fn_name
                            ));
                        } else {
                            text.push_str(&format!(
                                "peel: ledger: {} @{}..{} (fn {})\n",
                                r.construct, r.span.lo, r.span.hi, r.fn_name
                            ));
                        }
                    }
                    if build.not_yet.is_empty() {
                        text.push_str("peel: clean\n");
                    }
                    text
                }
                _ => wolf_mem::dump_package(&res.package, &tc),
            };
            eprint!("{text}");
        }
    }

    // Source-level `#[allow]` suppression (s67): warnings inside an
    // allowed region drop before reporting and before the record —
    // levels stay default (conform-run takes no lint flags; the
    // attribute is the program's own, so every consumer honors it).
    let diagnostics = wolf_diag::lint::apply(&LintLevels::new(), &allow_regions, diagnostics);

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
    // The warnings array (s67, [proto.record.warn]): every
    // warning-severity observation as `{code, span}` — additive within
    // protocol 1, present whenever the implementation runs its warning
    // analyses (wolfgang always does), honest-absent otherwise.
    let warnings: Vec<serde_json::Value> = diagnostics
        .iter()
        .filter(|d| d.severity == wolf_diag::Severity::Warning)
        .map(|d| {
            serde_json::json!({
                "code": d.code.as_str(),
                "span": [d.span().lo, d.span().hi],
            })
        })
        .collect();
    let mut record = serde_json::json!({
        "protocol": 1,
        "impl": "wolfgang",
        "impl_version": env!("CARGO_PKG_VERSION"),
        "commit": option_env!("WOLF_COMMIT").unwrap_or("unknown"),
        "file": file.replace('\\', "/"),
        "phase_reached": phase_reached,
        // s73: true exactly when a --seed reached a native execution
        // ([proto.seed.flag] — the checked machine stays seed-blind).
        "seeded": seed.is_some() && native,
        "diagnostics": minimal,
        "warnings": warnings,
        "verdict": verdict,
        "stdout_sha256": stdout_sha,
        "stdout_inline": stdout_inline,
    });
    for (k, v) in x_ext {
        record[k] = v;
    }
    println!("{record}");
}
