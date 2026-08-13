//! `wolf doc` — the documentation verb (s53, D34: the name is forever).
//!
//! ```text
//! wolf doc [--private] [--out DIR] [--check] [--open] [--json]
//!          [--coverage] [--require-docs] [--deny-warnings]
//!          [--std-root DIR] [<file.lu|dir>]
//! ```
//!
//! Zero-config: bare `wolf doc` documents the package around the
//! working directory together with its resolved dependency surface, and
//! writes `doc/` next to it. `--check` regenerates nothing to disk and
//! exits nonzero on any drift, which is the CI posture (and the same
//! posture `cargo xtask diag-catalog --check` takes for the diagnostics
//! catalog — generated documentation is checked, not trusted).
//!
//! Broken intra-doc links are W1501 warnings; `--deny-warnings`
//! promotes them, so a link that stopped resolving fails CI instead of
//! shipping as a dead reference.

use std::path::{Path, PathBuf};

use wolf_diag::{Diagnostic, HumanReporter, RenderOptions, Reporter, Sources};

use crate::{effective_std_root, take_std_root};

struct Cli {
    entry: Option<PathBuf>,
    out: PathBuf,
    private: bool,
    check: bool,
    open: bool,
    json_only: bool,
    coverage_only: bool,
    require_docs: bool,
    deny_warnings: bool,
    std_root: Option<PathBuf>,
    /// `--std-root` was given AND no entry: document the std tree
    /// itself, which is s53's proving corpus.
    doc_std: bool,
}

fn usage() -> ! {
    eprintln!(
        "usage: wolf doc [--private] [--out DIR] [--check] [--open] [--json] [--coverage]\n\
         \x20               [--require-docs] [--deny-warnings] [--std-root DIR] [<file.lu|dir>]"
    );
    std::process::exit(2);
}

fn parse_cli(args: &[String]) -> Cli {
    let (args, std_flag) = match take_std_root(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("wolf doc: {e}");
            std::process::exit(2);
        }
    };
    let std_root = match effective_std_root(std_flag.clone()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("wolf doc: {e}");
            std::process::exit(2);
        }
    };
    let mut cli = Cli {
        entry: None,
        out: PathBuf::from("doc"),
        private: false,
        check: false,
        open: false,
        json_only: false,
        coverage_only: false,
        require_docs: false,
        deny_warnings: false,
        std_root,
        doc_std: false,
    };
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--private" => cli.private = true,
            "--check" => cli.check = true,
            "--open" => cli.open = true,
            "--json" => cli.json_only = true,
            "--coverage" => cli.coverage_only = true,
            "--require-docs" => cli.require_docs = true,
            "--deny-warnings" => cli.deny_warnings = true,
            "--out" => {
                i += 1;
                match args.get(i) {
                    Some(v) => cli.out = PathBuf::from(v),
                    None => {
                        eprintln!("wolf doc: --out needs a directory");
                        std::process::exit(2);
                    }
                }
            }
            _ if a.starts_with("--out=") => cli.out = PathBuf::from(&a["--out=".len()..]),
            "-h" | "--help" => usage(),
            _ if a.starts_with('-') => {
                eprintln!("wolf doc: unknown flag `{a}`");
                usage();
            }
            _ if cli.entry.is_none() => cli.entry = Some(PathBuf::from(a)),
            _ => {
                eprintln!("wolf doc: one package at a time (got a second path `{a}`)");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    // `wolf doc --std-root <tree>` with no entry documents the std tree.
    if cli.entry.is_none() && std_flag.is_some() {
        cli.doc_std = true;
    }
    cli
}

/// Document a whole std tree: every subdirectory holding `.lu` files is
/// one std module, resolved as its own package with the tree as its std
/// root (so a std module importing another std module resolves).
///
/// This is not the single-package path with a different flag — the std
/// facade genuinely is a set of independently versioned packages (D31),
/// and documenting it as one package would require a root module that
/// imports every module and uses none, which resolution correctly
/// refuses (E0305). Modules are documented where they live.
fn doc_std_tree(
    root: &Path,
    private: bool,
    sources: &mut Sources,
) -> (wolf_query::DocPackage, Vec<Diagnostic>, usize) {
    let mut modules: Vec<wolf_query::DocModule> = Vec::new();
    let mut links: Vec<Diagnostic> = Vec::new();
    let mut refused = 0usize;
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let name = dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let has_lu = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| e.path().extension().is_some_and(|x| x == "lu"));
        if !has_lu {
            continue;
        }
        let mut sm = wolf_span::SourceMap::new();
        let mut loader =
            wolf_sema::DiskLoader::from_dir(&dir, &mut sm).with_std_root(Some(root.to_path_buf()));
        let Ok(res) = wolf_sema::resolve_package(&mut loader, &wolf_sema::AliasTable::default())
        else {
            refused += 1;
            continue;
        };
        for unit in &res.package.files {
            sources.add(unit.raw.file, unit.raw.display.clone(), &unit.raw.src);
        }
        if res
            .diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error)
        {
            // A std module the compiler cannot resolve is not documented
            // and is COUNTED — a silently missing page is the one
            // failure mode a documentation tool must not have. Its
            // diagnostics render, so the gap has a reason.
            refused += 1;
            let mut reporter = HumanReporter::new(sources, RenderOptions::default());
            let mut ds = res.diagnostics.clone();
            wolf_diag::sort_diagnostics(&mut ds);
            for d in &ds {
                reporter.report(d);
            }
            eprint!("{}", reporter.take_output());
            eprintln!("wolf doc: std module `{name}` does not resolve and gets no page");
            continue;
        }
        let docs = wolf_query::doc_package(&res, private);
        links.extend(wolf_query::resolve_links(&docs, &res));
        for mut m in docs.modules {
            // A std module that imports another std module pulls it into
            // its package under its REAL name (`std.cmp`). That module
            // is already canonically named; only the package's own root
            // takes the directory's name.
            m.path = match m.path.strip_prefix("std.") {
                Some(canonical) => canonical.to_string(),
                None if m.path.is_empty() => name.clone(),
                None => format!("{name}.{}", m.path),
            };
            modules.push(m);
        }
    }
    modules.sort_by(|a, b| a.path.cmp(&b.path));
    modules.dedup_by(|a, b| a.path == b.path);
    (
        wolf_query::DocPackage {
            package: "std".to_string(),
            modules,
            private,
        },
        links,
        refused,
    )
}

pub fn doc(args: &[String]) {
    let cli = parse_cli(args);
    let mut sm = wolf_span::SourceMap::new();
    let mut sources = Sources::new();

    // `wolf doc --std-root <tree>` with no entry: the stdlib docs, s53's
    // proving corpus (D31 — stdlib docs are a first-class deliverable).
    if cli.doc_std {
        let root = cli.std_root.clone().expect("--std-root implies a root");
        let (docs, link_diags, refused) = doc_std_tree(&root, cli.private, &mut sources);
        finish(&cli, docs, link_diags, refused, &sources, Vec::new(), "std");
    }

    // What are we documenting? A file entry, a directory, the std tree,
    // or — zero-config — the package around the working directory.
    let (root, entry): (PathBuf, Option<PathBuf>) = match (&cli.entry, cli.doc_std) {
        (Some(p), _) if p.is_file() => (
            p.parent().unwrap_or(Path::new(".")).to_path_buf(),
            Some(p.clone()),
        ),
        (Some(p), _) if p.is_dir() => (p.clone(), None),
        (Some(p), _) => {
            eprintln!("wolf doc: no such file or directory: {}", p.display());
            std::process::exit(2);
        }
        (None, true) => (
            cli.std_root.clone().expect("--std-root implies a root"),
            None,
        ),
        (None, false) => (PathBuf::from("."), None),
    };

    // The dependency surface: a package's documentation is generated
    // against the dependency set that resolved (s51's answer), and the
    // index records which one that was.
    let project = if cli.doc_std {
        None
    } else {
        crate::pkg_cmd::project_for_build(&root, &mut sm)
    };
    if let Some(p) = &project {
        for m in &p.manifests {
            sources.add(m.file, m.display.clone(), m.text.as_bytes());
        }
        if p.has_errors() {
            let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
            for d in &p.diagnostics {
                reporter.report(d);
            }
            eprint!("{}", reporter.take_output());
            eprintln!("wolf doc: the dependency graph does not resolve; fix the errors above");
            std::process::exit(1);
        }
    }
    let std_root = if cli.doc_std {
        // The std tree documents itself: it is the package, so it is not
        // also its own std slot.
        None
    } else {
        cli.std_root.clone()
    };

    let res = match &entry {
        Some(file) => crate::resolve_from_entry(
            file,
            &mut sm,
            &mut sources,
            std_root.as_deref(),
            project.as_ref(),
        ),
        None => {
            let mut loader =
                wolf_sema::DiskLoader::from_dir(&root, &mut sm).with_std_root(std_root.clone());
            if let Some(p) = &project {
                loader = loader.with_dep_roots(p.dep_roots.clone());
            }
            wolf_sema::resolve_package(&mut loader, &wolf_sema::AliasTable::default()).inspect(
                |r| {
                    for unit in &r.package.files {
                        sources.add(unit.raw.file, unit.raw.display.clone(), &unit.raw.src);
                    }
                },
            )
        }
    };
    let res = match res {
        Ok(r) => r,
        Err(e) => {
            eprintln!("wolf doc: {e}");
            std::process::exit(2);
        }
    };
    // Resolution must be clean: a signature the compiler could not
    // resolve is a signature no page may print.
    let mut diags: Vec<Diagnostic> = res.diagnostics.clone();
    let fatal = diags
        .iter()
        .any(|d| d.severity == wolf_diag::Severity::Error);
    if fatal {
        let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
        wolf_diag::sort_diagnostics(&mut diags);
        for d in &diags {
            reporter.report(d);
        }
        eprint!("{}", reporter.take_output());
        eprintln!("wolf doc: the package does not resolve; fix the errors above");
        std::process::exit(1);
    }

    let mut docs = wolf_query::doc_package(&res, cli.private);
    // The manifest's `name` is the package's identity when there is one
    // — the loader only knows the directory it was handed.
    if let Some(p) = &project
        && let Some(rootpkg) = p.pkgs.first()
        && !rootpkg.name.is_empty()
    {
        docs.package = rootpkg.name.clone();
    }
    let link_diags = wolf_query::resolve_links(&docs, &res);
    let deps: Vec<(String, String, String)> = project
        .as_ref()
        .map(|p| {
            p.pkgs
                .iter()
                .skip(1)
                .map(|d| (d.alias.clone(), d.name.clone(), d.version.clone()))
                .collect()
        })
        .unwrap_or_default();
    let title = docs.package.clone();
    finish(&cli, docs, link_diags, 0, &sources, deps, &title);
}

/// Render, then report — shared by the single-package path and the std
/// tree. Never returns: `wolf doc` always exits with a verdict.
///
/// `link_diags` are the broken intra-doc links; `refused` counts modules
/// that did not resolve at all and therefore have no page.
#[allow(clippy::too_many_arguments)]
fn finish(
    cli: &Cli,
    docs: wolf_query::DocPackage,
    link_diags: Vec<Diagnostic>,
    refused: usize,
    sources: &Sources,
    deps: Vec<(String, String, String)>,
    title: &str,
) -> ! {
    let broken_links = link_diags.len();
    if !link_diags.is_empty() {
        let mut reporter = HumanReporter::new(sources, RenderOptions::default());
        let mut ds = link_diags;
        wolf_diag::sort_diagnostics(&mut ds);
        for d in &ds {
            reporter.report(d);
        }
        eprint!("{}", reporter.take_output());
    }
    let title = title.to_string();
    let opts = wolf_doc::Options {
        private: cli.private,
        deps,
        title: title.clone(),
    };
    let cov = wolf_doc::coverage(&docs);
    let site = wolf_doc::render(&docs, &opts);

    // --coverage: the burn-down list, nothing written.
    if cli.coverage_only {
        println!("{}", cov.summary());
        for name in &cov.undocumented {
            println!("  no doc comment: {name}");
        }
        for name in &cov.no_doctest {
            println!("  no doctest:     {name}");
        }
        let gate_failed = cli.require_docs && !cov.complete();
        if gate_failed {
            eprintln!(
                "wolf doc: {} item(s) without a doc comment, {} without a doctest",
                cov.undocumented.len(),
                cov.no_doctest.len()
            );
        }
        std::process::exit(if gate_failed { 1 } else { 0 });
    }

    if cli.json_only {
        let json = site
            .files
            .get("index.json")
            .expect("the index is always generated");
        print!("{}", String::from_utf8_lossy(json));
        std::process::exit(0);
    }

    if cli.check {
        let drift = site.check(&cli.out);
        if drift.in_sync() {
            eprintln!(
                "wolf doc: {} file(s) in sync under {} — {}",
                site.files.len(),
                cli.out.display(),
                cov.summary()
            );
            std::process::exit(0);
        }
        for f in &drift.missing {
            eprintln!("wolf doc: MISSING {f}");
        }
        for f in &drift.changed {
            eprintln!("wolf doc: CHANGED {f}");
        }
        for f in &drift.stale {
            eprintln!("wolf doc: STALE   {f}");
        }
        eprintln!(
            "wolf doc: {} out of sync — run `wolf doc --out {}` to regenerate",
            cli.out.display(),
            cli.out.display()
        );
        std::process::exit(1);
    }

    if let Err(e) = site.write(&cli.out) {
        eprintln!("wolf doc: {e}");
        std::process::exit(2);
    }
    if refused > 0 {
        // A module the compiler could not resolve gets no page, and the
        // count is printed rather than swallowed: a documentation set
        // that is quietly incomplete is worse than one that says so.
        println!(
            "wolf doc: {refused} module(s) did not resolve and are NOT documented \
             (their diagnostics are above)"
        );
    }
    println!(
        "wolf doc: wrote {} file(s) to {} ({})",
        site.files.len(),
        cli.out.display(),
        cov.summary()
    );
    for name in &cov.undocumented {
        println!("  no doc comment: {name}");
    }
    for name in &cov.no_doctest {
        println!("  no doctest:     {name}");
    }
    if cli.open {
        // No browser is launched: the index path is printed as a
        // `file://` URL, because a documentation tool that spawns a
        // process is a documentation tool that fails on a headless box.
        // The output is readable with JavaScript off, so the URL is the
        // whole feature.
        let abs = cli
            .out
            .join("index.html")
            .canonicalize()
            .unwrap_or_else(|_| cli.out.join("index.html"));
        println!("file://{}", abs.display().to_string().replace('\\', "/"));
    }
    let gate_failed = cli.require_docs && !cov.complete();
    let links_failed = cli.deny_warnings && broken_links > 0;
    if links_failed {
        eprintln!("wolf doc: {broken_links} broken intra-doc link(s), and warnings are denied");
    }
    if gate_failed {
        eprintln!(
            "wolf doc: {} item(s) without a doc comment, {} without a doctest",
            cov.undocumented.len(),
            cov.no_doctest.len()
        );
    }
    std::process::exit(if gate_failed || links_failed { 1 } else { 0 });
}

/// A package's doctests, with everything needed to compile one: the
/// loader roots the package itself resolves under.
pub struct DoctestSurface {
    pub doctests: Vec<wolf_doc::Doctest>,
    /// The package root (`entry`'s directory) — a module's own directory
    /// hangs off this.
    pub pkg_root: PathBuf,
    /// alias → root, from the resolved dependency graph.
    pub dep_roots: std::collections::BTreeMap<String, PathBuf>,
    /// A `std` path-dependency, when the manifest declared one.
    pub std_root: Option<PathBuf>,
}

/// The doctest surface, for `wolf test` (s53 §1: fences are doctests by
/// default, and `wolf test` owns running them). Returns the extracted
/// doctests of the package around `entry`, or a human error.
pub fn doctests_of(
    entry: &Path,
    std_root: Option<&Path>,
    sm: &mut wolf_span::SourceMap,
    sources: &mut Sources,
) -> Result<DoctestSurface, String> {
    let root = entry.parent().unwrap_or(Path::new("."));
    let project = crate::pkg_cmd::project_for_build(root, sm);
    let res = crate::resolve_from_entry(entry, sm, sources, std_root, project.as_ref())?;
    let docs = wolf_query::doc_package(&res, false);
    Ok(DoctestSurface {
        doctests: wolf_doc::doctests(&docs),
        pkg_root: root.to_path_buf(),
        dep_roots: project
            .as_ref()
            .map(|p| p.dep_roots.clone())
            .unwrap_or_default(),
        std_root: project.and_then(|p| p.std_root),
    })
}
