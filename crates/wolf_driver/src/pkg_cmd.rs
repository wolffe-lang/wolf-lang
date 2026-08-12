//! The package-manager verbs (s51, D34: `add rm update audit tree
//! why` — the names are forever).
//!
//! All verbs operate on a project directory (`--dir DIR`, default the
//! working directory) holding an s51 `wolf.pkg`. The division of
//! labor: `wolf_pkg` owns formats and resolution; this module owns the
//! CLI, the ledger writes, and honest exits — 0 clean, 1 refusal or
//! finding, 2 usage/environment.

use std::path::{Path, PathBuf};

use wolf_diag::{Diagnostic, HumanReporter, RenderOptions, Reporter, Sources};
use wolf_pkg::manifest::{self, DepSource};
use wolf_pkg::{Lock, Project, ResolveOpts};

/// Split `--dir <d>`/`--dir=<d>` out of the args; default `.`.
fn take_dir(args: &[String], cmd: &str) -> (Vec<String>, PathBuf) {
    let mut rest = Vec::new();
    let mut dir = PathBuf::from(".");
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(v) = a.strip_prefix("--dir=") {
            dir = PathBuf::from(v);
        } else if a == "--dir" {
            i += 1;
            match args.get(i) {
                Some(v) => dir = PathBuf::from(v),
                None => {
                    eprintln!("wolf {cmd}: --dir needs a directory");
                    std::process::exit(2);
                }
            }
        } else {
            rest.push(a.clone());
        }
        i += 1;
    }
    (rest, dir)
}

/// Render a project's diagnostics (manifest-spanned) to stderr.
fn render_project(project: &Project) {
    let mut sources = Sources::new();
    for m in &project.manifests {
        sources.add(m.file, m.display.clone(), m.text.as_bytes());
    }
    let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
    for d in &project.diagnostics {
        reporter.report(d);
    }
    let out = reporter.take_output();
    if !out.is_empty() {
        eprint!("{out}");
    }
}

fn read_lock(dir: &Path, cmd: &str) -> Option<Lock> {
    let text = std::fs::read_to_string(dir.join("wolf.sum")).ok()?;
    match Lock::parse(&text) {
        Ok(l) => Some(l),
        Err(e) => {
            eprintln!("wolf {cmd}: {e}");
            std::process::exit(2);
        }
    }
}

fn write_lock(dir: &Path, project: &Project, cmd: &str) {
    let lock = project.to_lock();
    let path = dir.join("wolf.sum");
    if lock.entries.is_empty() {
        // No dependencies: no ledger file (a byte-stable nothing).
        let _ = std::fs::remove_file(&path);
        return;
    }
    if let Err(e) = std::fs::write(&path, lock.render()) {
        eprintln!("wolf {cmd}: write {}: {e}", path.display());
        std::process::exit(2);
    }
}

fn resolve_or_die(dir: &Path, opts: &ResolveOpts, cmd: &str) -> Project {
    let mut sm = wolf_span::SourceMap::new();
    let project = wolf_pkg::resolve_project(dir, &mut sm, opts);
    render_project(&project);
    if project.has_errors() {
        eprintln!("wolf {cmd}: the dependency graph does not resolve; fix the errors above");
        std::process::exit(1);
    }
    project
}

fn require_manifest(dir: &Path, cmd: &str) -> String {
    let path = dir.join("wolf.pkg");
    match std::fs::read_to_string(&path) {
        Ok(text) if wolf_pkg::is_manifest(&text) => text,
        Ok(_) => {
            eprintln!(
                "wolf {cmd}: {} is not an s51 manifest (expected a `pkg {{ }}` block)",
                path.display()
            );
            std::process::exit(2);
        }
        Err(_) => {
            eprintln!(
                "wolf {cmd}: no wolf.pkg in {} (run `wolf add` to create one, or --dir)",
                dir.display()
            );
            std::process::exit(2);
        }
    }
}

/// Print the capability deltas of a resolution against the recorded
/// ledger (I13: acquisition is surfaced BEFORE the lockfile changes).
/// Returns true when any capability was *acquired*.
fn report_cap_deltas(project: &Project, lock: &Lock) -> bool {
    let deltas = wolf_pkg::audit::diff_against_lock(project, lock);
    let mut acquired = false;
    for d in &deltas {
        for c in &d.added {
            eprintln!(
                "wolf audit: `{}` ACQUIRES capability `{}` (was not in wolf.sum)",
                d.alias,
                c.as_str()
            );
            acquired = true;
        }
        for c in &d.removed {
            eprintln!(
                "wolf audit: `{}` drops capability `{}`",
                d.alias,
                c.as_str()
            );
        }
    }
    acquired
}

// ---------------------------------------------------------------- add ----

/// `wolf add <alias> (--path DIR | --git URL --tag TAG) [--dir DIR]`.
pub fn add(args: &[String]) {
    let (args, dir) = take_dir(args, "add");
    let usage = || -> ! {
        eprintln!("usage: wolf add <alias> (--path DIR | --git URL --tag TAG) [--dir DIR]");
        std::process::exit(2);
    };
    let mut name: Option<String> = None;
    let mut path: Option<String> = None;
    let mut git: Option<String> = None;
    let mut tag: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let mut flag = |out: &mut Option<String>, name: &str| {
            i += 1;
            match args.get(i) {
                Some(v) => *out = Some(v.clone()),
                None => {
                    eprintln!("wolf add: {name} needs a value");
                    std::process::exit(2);
                }
            }
        };
        match a.as_str() {
            "--path" => flag(&mut path, "--path"),
            "--git" => flag(&mut git, "--git"),
            "--tag" => flag(&mut tag, "--tag"),
            _ if a.starts_with('-') => usage(),
            _ if name.is_none() => name = Some(a.clone()),
            _ => usage(),
        }
        i += 1;
    }
    let Some(name) = name else { usage() };
    // The import alias is one identifier: `wolf add acme/redis` takes
    // the package's last segment as the alias.
    let alias = name.rsplit('/').next().unwrap_or(&name).to_string();
    let source = match (path, git, tag) {
        (Some(p), None, None) => DepSource::Path { path: p },
        (None, Some(url), Some(tag)) => DepSource::Git { url, tag },
        (None, Some(_), None) => {
            eprintln!("wolf add: --git needs a --tag pin (MVS resolves points, not branches)");
            std::process::exit(2);
        }
        (None, None, None) => {
            eprintln!(
                "wolf add: `{name}` looks like a registry dependency, and the hosted \
                 registry arrives at c15 (X7) — give a v0 source: --path DIR, or --git URL --tag TAG"
            );
            std::process::exit(2);
        }
        _ => usage(),
    };

    // Ensure a manifest exists (a fresh project gets a minimal one).
    let manifest_path = dir.join("wolf.pkg");
    let original = match std::fs::read_to_string(&manifest_path) {
        Ok(text) if wolf_pkg::is_manifest(&text) => text,
        Ok(_) => {
            eprintln!(
                "wolf add: {} is not an s51 manifest (expected a `pkg {{ }}` block)",
                manifest_path.display()
            );
            std::process::exit(2);
        }
        Err(_) => {
            let dirname = dir
                .canonicalize()
                .ok()
                .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "app".to_string());
            let text = manifest::minimal_manifest(&format!("local/{dirname}"));
            eprintln!(
                "wolf add: created {} (minimal manifest)",
                manifest_path.display()
            );
            text
        }
    };
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(&manifest_path);
    let (m, diags) = manifest::parse(file, &original);
    let Some(m) = m else {
        let mut sources = Sources::new();
        sources.add(
            file,
            manifest_path.display().to_string().replace('\\', "/"),
            original.as_bytes(),
        );
        let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
        for d in &diags {
            reporter.report(d);
        }
        eprint!("{}", reporter.take_output());
        eprintln!("wolf add: the manifest does not parse; fix it first");
        std::process::exit(1);
    };
    if m.deps.iter().any(|d| d.alias == alias) {
        eprintln!(
            "wolf add: `{alias}` is already a dependency (use `wolf rm` first, or `wolf update`)"
        );
        std::process::exit(1);
    }
    let edited = manifest::insert_dep(&original, &m, &alias, &source);
    if let Err(e) = std::fs::write(&manifest_path, &edited) {
        eprintln!("wolf add: write {}: {e}", manifest_path.display());
        std::process::exit(2);
    }

    // Resolve with the new entry; on failure the edit is rolled back —
    // `wolf add` either lands whole (manifest + ledger) or not at all.
    let old_lock = read_lock(&dir, "add").unwrap_or_default();
    let opts = ResolveOpts {
        lock: read_lock(&dir, "add"),
        fetch_unpinned: true,
        refresh: false,
        store: None,
    };
    let mut sm2 = wolf_span::SourceMap::new();
    let project = wolf_pkg::resolve_project(&dir, &mut sm2, &opts);
    render_project(&project);
    if project.has_errors() {
        let restored = if manifest_path.is_file() && !original.is_empty() {
            std::fs::write(&manifest_path, &original).is_ok()
        } else {
            false
        };
        eprintln!(
            "wolf add: `{alias}` does not resolve; {}",
            if restored {
                "the manifest edit was rolled back"
            } else {
                "fix the manifest"
            }
        );
        std::process::exit(1);
    }
    report_cap_deltas(&project, &old_lock);
    write_lock(&dir, &project, "add");
    let added = project.pkgs.iter().find(|p| p.alias == alias);
    match added {
        Some(p) => {
            let caps: Vec<&str> = p.caps.iter().map(|c| c.as_str()).collect();
            eprintln!(
                "wolf add: {alias} {} — capabilities [{}]{}",
                p.version,
                caps.join(", "),
                p.hash
                    .as_deref()
                    .map(|h| format!(", pinned {}", &h[..16.min(h.len())]))
                    .unwrap_or_default()
            );
        }
        None => eprintln!("wolf add: {alias} added"),
    }
}

// ----------------------------------------------------------------- rm ----

/// `wolf rm <alias> [--dir DIR]`.
pub fn rm(args: &[String]) {
    let (args, dir) = take_dir(args, "rm");
    let [alias] = args.as_slice() else {
        eprintln!("usage: wolf rm <alias> [--dir DIR]");
        std::process::exit(2);
    };
    let text = require_manifest(&dir, "rm");
    let manifest_path = dir.join("wolf.pkg");
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(&manifest_path);
    let (m, _) = manifest::parse(file, &text);
    let Some(m) = m else {
        eprintln!("wolf rm: the manifest does not parse; fix it first");
        std::process::exit(1);
    };
    let Some(edited) = manifest::remove_dep(&text, &m, alias) else {
        eprintln!("wolf rm: `{alias}` is not a dependency");
        std::process::exit(1);
    };
    if let Err(e) = std::fs::write(&manifest_path, &edited) {
        eprintln!("wolf rm: write {}: {e}", manifest_path.display());
        std::process::exit(2);
    }
    let opts = ResolveOpts {
        lock: read_lock(&dir, "rm"),
        fetch_unpinned: false,
        refresh: false,
        store: None,
    };
    let project = resolve_or_die(&dir, &opts, "rm");
    write_lock(&dir, &project, "rm");
    eprintln!("wolf rm: {alias} removed");
}

// ------------------------------------------------------------- update ----

/// `wolf update [--dir DIR]` — re-fetch pinned deps, refresh the
/// ledger. The capability diff is surfaced BEFORE the write (I13).
pub fn update(args: &[String]) {
    let (args, dir) = take_dir(args, "update");
    if !args.is_empty() {
        eprintln!("usage: wolf update [--dir DIR]");
        std::process::exit(2);
    }
    require_manifest(&dir, "update");
    let old_lock = read_lock(&dir, "update").unwrap_or_default();
    let opts = ResolveOpts {
        lock: read_lock(&dir, "update"),
        fetch_unpinned: true,
        refresh: true,
        store: None,
    };
    let project = resolve_or_die(&dir, &opts, "update");
    report_cap_deltas(&project, &old_lock);
    // Hash movements, named.
    let new_lock = project.to_lock();
    for (alias, e) in &new_lock.entries {
        let old = old_lock.entries.get(alias);
        match (old.and_then(|o| o.hash.as_deref()), e.hash.as_deref()) {
            (Some(a), Some(b)) if a != b => {
                eprintln!(
                    "wolf update: {alias} {} -> {}",
                    &a[..16.min(a.len())],
                    &b[..16.min(b.len())]
                );
            }
            _ => {}
        }
    }
    write_lock(&dir, &project, "update");
    eprintln!(
        "wolf update: wolf.sum refreshed ({} entr{})",
        new_lock.entries.len(),
        if new_lock.entries.len() == 1 {
            "y"
        } else {
            "ies"
        }
    );
}

// -------------------------------------------------------------- audit ----

/// `wolf audit [--ci] [--dir DIR]` — the I13 capability tree, plus the
/// acquisition diff against `wolf.sum`. `--ci` exits nonzero when any
/// package acquired a capability the ledger has not witnessed.
pub fn audit(args: &[String]) {
    let (args, dir) = take_dir(args, "audit");
    let mut ci = false;
    for a in &args {
        match a.as_str() {
            "--ci" => ci = true,
            _ => {
                eprintln!("usage: wolf audit [--ci] [--dir DIR]");
                std::process::exit(2);
            }
        }
    }
    require_manifest(&dir, "audit");
    let lock = read_lock(&dir, "audit");
    let opts = ResolveOpts {
        lock: lock.clone(),
        fetch_unpinned: false,
        refresh: false,
        store: None,
    };
    let project = resolve_or_die(&dir, &opts, "audit");
    print!("{}", wolf_pkg::audit::render_tree(&project));
    let acquired = match &lock {
        Some(lock) => report_cap_deltas(&project, lock),
        None => {
            eprintln!(
                "wolf audit: no wolf.sum yet — nothing to diff against (run a verb that writes it)"
            );
            false
        }
    };
    if ci && acquired {
        eprintln!("wolf audit: capability acquisition detected — refusing (--ci)");
        std::process::exit(1);
    }
}

// --------------------------------------------------------- tree / why ----

/// `wolf tree [--dir DIR]` — the resolved dependency tree.
pub fn tree(args: &[String]) {
    let (args, dir) = take_dir(args, "tree");
    if !args.is_empty() {
        eprintln!("usage: wolf tree [--dir DIR]");
        std::process::exit(2);
    }
    require_manifest(&dir, "tree");
    let opts = ResolveOpts {
        lock: read_lock(&dir, "tree"),
        fetch_unpinned: false,
        refresh: false,
        store: None,
    };
    let project = resolve_or_die(&dir, &opts, "tree");
    print!("{}", wolf_pkg::audit::render_tree(&project));
}

/// `wolf why <alias> [--dir DIR]` — the dependency chain that pulls
/// `alias` into the build (MVS terms: who states the minimum).
pub fn why(args: &[String]) {
    let (args, dir) = take_dir(args, "why");
    let [alias] = args.as_slice() else {
        eprintln!("usage: wolf why <alias> [--dir DIR]");
        std::process::exit(2);
    };
    require_manifest(&dir, "why");
    let opts = ResolveOpts {
        lock: read_lock(&dir, "why"),
        fetch_unpinned: false,
        refresh: false,
        store: None,
    };
    let project = resolve_or_die(&dir, &opts, "why");
    let Some(target) = project.pkgs.iter().position(|p| p.alias == *alias) else {
        eprintln!("wolf why: `{alias}` is not in the resolved graph");
        std::process::exit(1);
    };
    // BFS from the root; parent pointers give the chain.
    let mut parent: Vec<Option<usize>> = vec![None; project.pkgs.len()];
    let mut queue = std::collections::VecDeque::from([0usize]);
    let mut seen = vec![false; project.pkgs.len()];
    seen[0] = true;
    while let Some(i) = queue.pop_front() {
        for &d in &project.pkgs[i].deps {
            if !seen[d] {
                seen[d] = true;
                parent[d] = Some(i);
                queue.push_back(d);
            }
        }
    }
    if !seen[target] {
        eprintln!("wolf why: `{alias}` resolves but nothing depends on it");
        std::process::exit(1);
    }
    let mut chain = vec![target];
    while let Some(p) = parent[*chain.last().unwrap()] {
        chain.push(p);
    }
    chain.reverse();
    let words: Vec<String> = chain
        .iter()
        .map(|&i| {
            let p = &project.pkgs[i];
            if i == 0 {
                format!("{} (root)", p.name)
            } else {
                format!("{} {}", p.alias, p.version)
            }
        })
        .collect();
    println!("{}", words.join(" -> "));
}

/// The build-time hookup (s51): when the entry file's directory holds
/// an s51 manifest, resolve the dependency graph and hand back the
/// loader roots. `None` = no manifest (single-package build, the
/// pre-s51 world, still first-class). Errors come back as diagnostics
/// with the manifest sources to register.
pub fn project_for_build(root: &Path, sm: &mut wolf_span::SourceMap) -> Option<Project> {
    let text = std::fs::read_to_string(root.join("wolf.pkg")).ok()?;
    if !wolf_pkg::is_manifest(&text) {
        return None;
    }
    let lock = std::fs::read_to_string(root.join("wolf.sum"))
        .ok()
        .and_then(|t| Lock::parse(&t).ok());
    Some(wolf_pkg::resolve_project(
        root,
        sm,
        &ResolveOpts {
            lock,
            fetch_unpinned: false,
            refresh: false,
            store: None,
        },
    ))
}

/// The import-graph half of the I13 check, driver-side: build the
/// module→imports table from the resolved package and ask wolf_pkg
/// which packages use capabilities they never declared (E1504).
pub fn capability_diagnostics(project: &Project, pkg: &wolf_sema::Package) -> Vec<Diagnostic> {
    let module_imports: Vec<(String, Vec<String>)> = pkg
        .modules
        .iter()
        .map(|m| {
            (
                m.dotted(),
                m.deps.iter().map(|&d| pkg.modules[d].dotted()).collect(),
            )
        })
        .collect();
    wolf_pkg::audit::capability_check(project, &module_imports)
}
