//! Doctests (s53 §1, D36): "code fences in doc comments are doctests by
//! default: compiled and run under `wolf test` — the docs cannot rot,
//! same covenant as the book track."
//!
//! # What a doctest is
//!
//! A fence in a `///` or `//!` comment, in any wolf source file. It is a
//! wolf **program**: if it declares no `main`, the runner wraps it in
//! `fn main() -> !int { … 0 }`, so a two-line example stays two lines.
//! The runner imports the documented module for you — `use std.str` for
//! `std/str`, `use util` for a package's `util` module — because a doc
//! example that spends its first line on ceremony is a doc example
//! nobody reads.
//!
//! # The directives, all three
//!
//! - `no_run` — must compile; is not executed.
//! - `should_fail(E0402, …)` — must be REFUSED, with one of those codes.
//!   No codes means "any error", but naming one is the reviewed form.
//! - `ignore` — not a program at all (prose in a fence); never compiled.
//!
//! # Where they run
//!
//! Each doctest is staged as its own file in a temporary directory and
//! compiled through the ordinary ladder (resolve → typecheck → mem),
//! then executed on the checked machine — the same machine `wolf test`
//! runs `test_*` functions on, so a doctest's verdict means exactly what
//! a test's verdict means. Nothing is written next to the source.

use std::path::{Path, PathBuf};

use wolf_diag::{Diagnostic, HumanReporter, RenderOptions, Reporter, Sources};
use wolf_mem::ubcheck::{self, Budget, Verdict};

/// A doctest's outcome, in the runner's own vocabulary.
pub enum Outcome {
    Pass(String),
    Fail(String),
    /// The ladder refused a construct (the conservatism ledger).
    Unsupported(String),
}

impl Outcome {
    pub fn status(&self) -> &'static str {
        match self {
            Outcome::Pass(_) => "pass",
            Outcome::Fail(_) => "fail",
            Outcome::Unsupported(_) => "unsupported",
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            Outcome::Pass(d) | Outcome::Fail(d) | Outcome::Unsupported(d) => d,
        }
    }
}

/// The doctests of one source file's package, with the loader roots to
/// compile them against. `entry` is a `.lu` file; the package around it
/// is resolved once.
pub fn collect(
    entry: &Path,
    std_root: Option<&Path>,
) -> Result<crate::doc_cmd::DoctestSurface, String> {
    let mut sm = wolf_span::SourceMap::new();
    let mut sources = Sources::new();
    crate::doc_cmd::doctests_of(entry, std_root, &mut sm, &mut sources)
}

/// Wrap a fence body into a complete program. A fence that declares its
/// own `main` is taken verbatim; anything else becomes a `main` body.
fn program(module: &str, code: &str, std_backed: bool) -> String {
    // Imports are FILE-scoped (D32), so a `use` a fence wrote for
    // readability has to be hoisted out of the generated `main` — a
    // `use` inside a body does not bind anything.
    let mut imports: Vec<&str> = Vec::new();
    let mut body: Vec<&str> = Vec::new();
    for line in code.lines() {
        let t = line.trim();
        if t.starts_with("use ") || t.starts_with("import ") {
            imports.push(t);
        } else {
            body.push(line);
        }
    }
    let mut out = String::new();
    // The documented module's import, so the example reads like the call
    // a user would write, unless the fence already wrote it.
    if !module.is_empty() {
        let want = if std_backed {
            format!("use std.{module}")
        } else {
            format!("use {}", module.rsplit('.').next().unwrap_or(module))
        };
        if !imports.contains(&want.as_str()) {
            out.push_str(&want);
            out.push('\n');
        }
    }
    for i in &imports {
        out.push_str(i);
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    let joined = body.join("\n");
    if joined.contains("fn main") {
        out.push_str(&joined);
        if !joined.ends_with('\n') {
            out.push('\n');
        }
        return out;
    }
    out.push_str("fn main() -> !int {\n");
    for line in &body {
        if line.trim().is_empty() {
            out.push('\n');
            continue;
        }
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("    0\n}\n");
    out
}

/// Run one doctest against the surface its package resolved under.
/// `std_root` is the std facade when one is in play.
pub fn run(
    dt: &wolf_doc::Doctest,
    surface: &crate::doc_cmd::DoctestSurface,
    std_root: Option<&Path>,
    scratch: &Path,
) -> Outcome {
    // A `std`-backed run is one where the documented tree IS the std
    // facade: `--std-root <tree>` with the tree as the package. A `std`
    // path-dependency is an ordinary loader root, not this.
    let std_backed = std_root.is_some_and(|r| {
        let a = r.canonicalize().ok();
        let b = surface.pkg_root.canonicalize().ok();
        a.is_some() && a == b
    });
    let source = program(&dt.module, &dt.fence.code, std_backed);
    let dir = scratch.join(sanitize(&dt.name));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Outcome::Unsupported(format!("cannot stage the doctest: {e}"));
    }
    let entry = dir.join("doctest.lu");
    if let Err(e) = std::fs::write(&entry, &source) {
        return Outcome::Unsupported(format!("cannot stage the doctest: {e}"));
    }
    // Loader roots: `use std.…` through the std slot when the documented
    // tree IS the std facade; otherwise the documented package rides as
    // a dependency under its own module alias.
    let mut sm = wolf_span::SourceMap::new();
    let mut sources = Sources::new();
    let mut loader = match wolf_sema::DiskLoader::from_entry(
        &entry,
        &mut sm,
        Box::new(|src: &[u8]| crate::is_member_file(src)),
    ) {
        Some(l) => l,
        None => return Outcome::Unsupported("cannot open the staged doctest".to_string()),
    };
    if std_backed {
        loader = loader.with_std_root(std_root.map(Path::to_path_buf));
    } else {
        // The package's own loader roots, so a fence may `use` anything
        // the documented module could — plus the documented module
        // itself, under its last path segment, when it is a submodule
        // rather than a dependency alias.
        let mut roots = surface.dep_roots.clone();
        if !dt.module.is_empty() {
            let alias = dt.module.rsplit('.').next().unwrap_or(&dt.module);
            roots.entry(alias.to_string()).or_insert_with(|| {
                let mut at = surface.pkg_root.clone();
                for seg in dt.module.split('.') {
                    at = at.join(seg);
                }
                at
            });
        }
        loader = loader
            .with_dep_roots(roots)
            .with_std_root(surface.std_root.clone().or(std_root.map(Path::to_path_buf)));
    }
    let res = match wolf_sema::resolve_package(&mut loader, &wolf_sema::AliasTable::default()) {
        Ok(r) => r,
        Err(e) => return Outcome::Unsupported(format!("cannot resolve the doctest: {e}")),
    };
    for unit in &res.package.files {
        sources.add(unit.raw.file, unit.raw.display.clone(), &unit.raw.src);
    }
    let mut diags: Vec<Diagnostic> = res.diagnostics.clone();
    let mut refused: Option<String> = None;
    if !has_errors(&diags) {
        let tc = wolf_sema::typecheck_package(&res);
        if let Some(nyc) = tc.not_yet.first() {
            refused = Some(nyc.construct.to_string());
        }
        diags.extend(tc.diagnostics.iter().cloned());
        if refused.is_none() && !has_errors(&diags) {
            let mem = wolf_mem::check_package(&res.package, &tc);
            if let Some(nyc) = mem.not_yet.first() {
                refused = Some(nyc.construct.to_string());
            }
            diags.extend(mem.diagnostics.iter().cloned());
        }
        // Compiled clean, and it was supposed to fail?
        if let Some(want) = dt.fence.should_fail() {
            return should_fail_verdict(want, &diags);
        }
        if let Some(construct) = refused {
            return Outcome::Unsupported(construct);
        }
        if has_errors(&diags) {
            return compile_failure(&sources, &diags);
        }
        if dt.fence.no_run() {
            return Outcome::Pass("compiles (no_run)".to_string());
        }
        return match ubcheck::run_checked_fn(&res.package, &tc, Budget::default(), "", "main") {
            Ok(out) => match &out.verdict {
                Verdict::Exit(0) => Outcome::Pass("exit(0)".to_string()),
                Verdict::Exit(n) => Outcome::Fail(format!("exit({n})")),
                Verdict::Trap(t) => Outcome::Fail(format!("trap({})", t.kind)),
                Verdict::Ub(f) => {
                    let d = ubcheck::ub_diagnostic(f);
                    render(&sources, std::slice::from_ref(&d));
                    Outcome::Fail("ub(mem.ub)".to_string())
                }
            },
            Err(refusal) => Outcome::Unsupported(refusal.construct.to_string()),
        };
    }
    if let Some(want) = dt.fence.should_fail() {
        return should_fail_verdict(want, &diags);
    }
    compile_failure(&sources, &diags)
}

fn should_fail_verdict(want: &[String], diags: &[Diagnostic]) -> Outcome {
    let codes: Vec<String> = diags
        .iter()
        .filter(|d| d.severity == wolf_diag::Severity::Error)
        .map(|d| d.code.as_str().to_string())
        .collect();
    if codes.is_empty() {
        return Outcome::Fail(format!(
            "expected a refusal ({}), and it compiled",
            if want.is_empty() {
                "any error".to_string()
            } else {
                want.join(", ")
            }
        ));
    }
    if want.is_empty() {
        return Outcome::Pass(format!("refused: {}", codes.join(", ")));
    }
    if want.iter().any(|w| codes.iter().any(|c| c == w)) {
        return Outcome::Pass(format!("refused: {}", codes.join(", ")));
    }
    Outcome::Fail(format!(
        "expected {}, got {}",
        want.join(", "),
        codes.join(", ")
    ))
}

fn compile_failure(sources: &Sources, diags: &[Diagnostic]) -> Outcome {
    let mut ds = diags.to_vec();
    wolf_diag::sort_diagnostics(&mut ds);
    render(sources, &ds);
    let codes: Vec<String> = ds
        .iter()
        .filter(|d| d.severity == wolf_diag::Severity::Error)
        .map(|d| d.code.as_str().to_string())
        .collect();
    Outcome::Fail(if codes.is_empty() {
        "does not compile".to_string()
    } else {
        format!("does not compile: {}", codes.join(", "))
    })
}

fn render(sources: &Sources, diags: &[Diagnostic]) {
    let mut reporter = HumanReporter::new(sources, RenderOptions::default());
    for d in diags {
        reporter.report(d);
    }
    eprint!("{}", reporter.take_output());
}

fn has_errors(ds: &[Diagnostic]) -> bool {
    ds.iter().any(|d| d.severity == wolf_diag::Severity::Error)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// A scratch directory for a run's staged doctests, under the system
/// temp dir (never next to the source).
pub fn scratch_dir() -> PathBuf {
    std::env::temp_dir().join(format!("wolf-doctest-{}", std::process::id()))
}
