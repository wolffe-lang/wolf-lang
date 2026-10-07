//! The package-manager verbs (s51, D34: `add rm update audit tree
//! why` — the names are forever).
//!
//! All verbs operate on a project directory (`--dir DIR`, default the
//! working directory) holding an s51 `wolf.pkg`. The division of
//! labor: `wolf_pkg` owns formats and resolution; this module owns the
//! CLI, the ledger writes, and honest exits — 0 clean, 1 refusal or
//! finding, 2 usage/environment.

use std::collections::BTreeMap;
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
        eprintln!("wolf {cmd}: write {}: {e}", show_path(&path));
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
                "wolf {cmd}: {} is not a wolf manifest (expected a `pkg {{ }}` block)",
                show_path(&path)
            );
            std::process::exit(2);
        }
        Err(_) => {
            eprintln!(
                "wolf {cmd}: no wolf.pkg in {} (run `wolf add` to create one, or --dir)",
                show_path(dir)
            );
            std::process::exit(2);
        }
    }
}

/// Print the capability deltas of a resolution against the recorded
/// ledger (I13: acquisition is surfaced BEFORE the lockfile changes).
/// Returns true when any capability was *acquired*.
/// The capability deltas against the lock, reported.
///
/// `to_stdout` decides the stream, and the rule is which verb's DATA
/// this is (s157, wolf-lang#157): under `wolf audit` the deltas ARE
/// the answer — the whole reason to run the verb — so they go to
/// stdout with the tree, and `wolf audit | tee report.txt` keeps the
/// finding. Under `add`/`update` they are a side note beside the
/// verb's real work, so they stay on stderr where the rest of that
/// verb's progress lives.
fn report_cap_deltas(project: &Project, lock: &Lock, to_stdout: bool) -> bool {
    let deltas = wolf_pkg::audit::diff_against_lock(project, lock);
    let mut acquired = false;
    let say = |line: String| {
        if to_stdout {
            println!("{line}");
        } else {
            eprintln!("{line}");
        }
    };
    for d in &deltas {
        for c in &d.added {
            say(format!(
                "wolf audit: `{}` ACQUIRES capability `{}` (was not in wolf.sum)",
                d.alias,
                c.as_str()
            ));
            acquired = true;
        }
        for c in &d.removed {
            say(format!(
                "wolf audit: `{}` drops capability `{}`",
                d.alias,
                c.as_str()
            ));
        }
    }
    acquired
}

// --------------------------------------------------------------- init ----

/// `wolf init --from-script <file.lu> [--dir DIR]` — promote a script
/// to a package (s53).
///
/// This is the verb E1507's fix-it names, so it has to exist and it has
/// to do exactly what the message promises: the frontmatter's dependency
/// entries, capabilities and edition move into a real `wolf.pkg`
/// **verbatim**, and the script's code moves into `main.lu` with its
/// frontmatter block removed and its prose kept. The original file is
/// never deleted — a promotion that destroys the input is a promotion
/// nobody tries twice.
pub fn init(args: &[String]) {
    let (args, dir) = take_dir(args, "init");
    let usage = || -> ! {
        crate::help::usage_exit("init");
    };
    let mut from: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--from-script" {
            i += 1;
            match args.get(i) {
                Some(v) => from = Some(PathBuf::from(v)),
                None => {
                    eprintln!("wolf init: --from-script needs a file");
                    std::process::exit(2);
                }
            }
        } else if let Some(v) = a.strip_prefix("--from-script=") {
            from = Some(PathBuf::from(v));
        } else {
            usage();
        }
        i += 1;
    }
    let Some(from) = from else { usage() };
    let text = match std::fs::read_to_string(&from) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("wolf init: cannot read {}: {e}", show_path(&from));
            std::process::exit(2);
        }
    };
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(&from);
    let read = wolf_pkg::script::read(file, &text);
    let stem = from
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".to_string());
    // The manifest: the frontmatter's own entries, plus the identity a
    // package needs and a script does not have.
    let mut manifest = format!(
        "pkg {{\n    name:    \"local/{stem}\",\n    version: \"0.1.0\",\n    edition: \"{}\",\n",
        read.manifest
            .as_ref()
            .map(|m| m.edition.as_str())
            .unwrap_or("1")
    );
    if let Some(m) = &read.manifest {
        if let Some(w) = &m.wolf_min {
            manifest.push_str(&format!("    wolf:    \"{w}\",\n"));
        }
        if !m.deps.is_empty() {
            manifest.push_str("\n    deps: {\n");
            for d in &m.deps {
                manifest.push_str(&format!(
                    "        {},\n",
                    manifest::render_dep(&d.alias, &d.source)
                ));
            }
            manifest.push_str("    },\n");
        }
        if !m.caps.is_empty() {
            manifest.push_str(&format!(
                "\n    capabilities: [{}],\n",
                m.caps
                    .iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    manifest.push_str("}\n");
    // The code: everything after the frontmatter's `//!` block, with the
    // shebang dropped (a package's module is not executable by itself)
    // and any surrounding prose kept as the module's doc comment.
    let code = strip_frontmatter(&text, read.frontmatter.as_ref());
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("wolf init: create {}: {e}", show_path(&dir));
        std::process::exit(2);
    }
    for (name, body) in [("wolf.pkg", &manifest), ("main.lu", &code)] {
        let at = dir.join(name);
        if at.exists() {
            eprintln!(
                "wolf init: {} already exists — refusing to overwrite",
                show_path(&at)
            );
            std::process::exit(1);
        }
        if let Err(e) = std::fs::write(&at, body) {
            eprintln!("wolf init: write {}: {e}", show_path(&at));
            std::process::exit(2);
        }
    }
    println!(
        "wolf init: wrote {} and {} from {} (the script is untouched)",
        show_path(&dir.join("wolf.pkg")),
        show_path(&dir.join("main.lu")),
        show_path(&from)
    );
}

/// The script's code with its shebang and its frontmatter LINES removed,
/// leaving the surrounding `//!` prose in place — the prose documents
/// the module, which is what it did before.
fn strip_frontmatter(text: &str, fm: Option<&wolf_pkg::Frontmatter>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0usize;
    for line in text.split_inclusive('\n') {
        let lo = at;
        let hi = at + line.trim_end_matches(['\n', '\r']).len();
        at += line.len();
        if lo == 0 && line.starts_with("#!") {
            continue;
        }
        if let Some(f) = fm
            && lo < f.hi as usize
            && hi > f.lo as usize
        {
            continue;
        }
        out.push_str(line);
    }
    // Collapse the blank `//!` lines the removal orphaned, at both ends
    // of the header block.
    while out.starts_with("//!\n") {
        out = out["//!\n".len()..].to_string();
    }
    let mut lines: Vec<&str> = out.lines().collect();
    let mut end = 0usize;
    while end < lines.len() && lines[end].trim_start().starts_with("//!") {
        end += 1;
    }
    while end > 0 && lines[end - 1].trim() == "//!" {
        lines.remove(end - 1);
        end -= 1;
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out.trim_start_matches('\n').to_string()
}

// ---------------------------------------------------------------- add ----

/// `wolf add <alias> (--path DIR | --git URL --tag TAG) [--dir DIR]`.
pub fn add(args: &[String]) {
    let (args, dir) = take_dir(args, "add");
    let usage = || -> ! {
        crate::help::usage_exit("add");
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
                "wolf add: `{name}` looks like a registry dependency, and no hosted \
                 registry exists — give a source: --path DIR, or --git URL --tag TAG"
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
                "wolf add: {} is not a wolf manifest (expected a `pkg {{ }}` block)",
                show_path(&manifest_path)
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
                show_path(&manifest_path)
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
            show_path(&manifest_path).to_string().replace('\\', "/"),
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
        eprintln!("wolf add: write {}: {e}", show_path(&manifest_path));
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
        offline: false,
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
    report_cap_deltas(&project, &old_lock, false);
    verify_log_or_die(&project, "add");
    write_lock(&dir, &project, "add");
    let added = project.pkgs.iter().find(|p| p.alias == alias);
    match added {
        Some(p) => {
            let caps: Vec<&str> = p.caps.iter().map(|c| c.as_str()).collect();
            // "capabilities: none" reads as a fact about the package;
            // "capabilities []" reads as a rendering accident of an
            // empty list (s157, wolf-lang#157).
            eprintln!(
                "wolf add: {alias} {} — capabilities: {}{}",
                p.version,
                if caps.is_empty() {
                    "none".to_string()
                } else {
                    caps.join(", ")
                },
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
        crate::help::usage_exit("rm");
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
        eprintln!("wolf rm: write {}: {e}", show_path(&manifest_path));
        std::process::exit(2);
    }
    let opts = ResolveOpts {
        lock: read_lock(&dir, "rm"),
        fetch_unpinned: false,
        refresh: false,
        store: None,
        offline: false,
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
        crate::help::usage_exit("update");
    }
    require_manifest(&dir, "update");
    let old_lock = read_lock(&dir, "update").unwrap_or_default();
    let opts = ResolveOpts {
        lock: read_lock(&dir, "update"),
        fetch_unpinned: true,
        refresh: true,
        store: None,
        offline: false,
    };
    let project = resolve_or_die(&dir, &opts, "update");
    report_cap_deltas(&project, &old_lock, false);
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
    verify_log_or_die(&project, "update");
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
                crate::help::usage_exit("audit");
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
        offline: false,
    };
    let project = resolve_or_die(&dir, &opts, "audit");
    // s217 (wolf-lang#615): the audit reads the CODE, not only the
    // manifests — the same derivation the build's E1504 reads, computed
    // here from source, so `--ci` needs no build to have run.
    let derived = derive_uses(&dir, &project);
    print!(
        "{}",
        wolf_pkg::audit::render_audit(&project, derived.as_deref().map_err(String::as_str))
    );
    let mut undeclared = false;
    match &derived {
        Ok(uses) => {
            for (u, _) in wolf_pkg::audit::undeclared(&project, uses) {
                let p = &project.pkgs[u.owner];
                let who = if u.owner == 0 { &p.name } else { &p.alias };
                println!(
                    "wolf audit: `{who}` reaches capability `{}` without declaring it ({} at {})",
                    u.cap.as_str(),
                    u.what(),
                    u.at
                );
                undeclared = true;
            }
        }
        Err(e) => println!("wolf audit: cannot derive capabilities from the code: {e}"),
    }
    let acquired = match &lock {
        Some(lock) => report_cap_deltas(&project, lock, true),
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
    if ci && undeclared {
        eprintln!("wolf audit: undeclared capability use — refusing (--ci)");
        std::process::exit(1);
    }
    if ci && derived.is_err() {
        eprintln!(
            "wolf audit: the code could not be read, so nothing is vouched for — refusing (--ci)"
        );
        std::process::exit(1);
    }
}

/// Load and resolve the package at `dir` the way its builds would —
/// `WOLF_STD` beats the manifest's `std` path dependency, the
/// dependency aliases are loader roots — and derive every capability
/// use from the code ([`cap_uses`]). Nothing is compiled.
///
/// A package root can hold standalone entries (D59: `//! member:
/// false`, pax's `kmain_*.lu`), each its own build that the directory
/// view leaves out; when the directory has any, every `.lu` file in it
/// is also resolved as an entry and the uses are united, so the audit
/// covers every program the manifest governs. `Err` only when no view
/// of the directory loads at all.
fn derive_uses(dir: &Path, project: &Project) -> Result<Vec<wolf_pkg::audit::CapUse>, String> {
    use wolf_pkg::audit::CapUse;
    let std_root = crate::effective_std_root(None)?.or_else(|| project.std_root.clone());
    // One view: the uses, and whether the root module left standalone
    // entries out.
    let resolve = |entry: Option<&Path>| -> Result<(Vec<CapUse>, bool), String> {
        let mut sm = wolf_span::SourceMap::new();
        let loader = match entry {
            Some(file) => wolf_sema::DiskLoader::from_entry(file, &mut sm)
                .ok_or_else(|| format!("cannot open package around {}", show_path(file)))?,
            None => wolf_sema::DiskLoader::from_dir(dir, &mut sm),
        };
        let mut loader = loader
            .with_std_root(std_root.clone())
            .with_dep_roots(project.dep_roots.clone());
        let res = wolf_sema::resolve_package(&mut loader, &wolf_sema::AliasTable::default())?;
        let standalone = !res.package.modules[0].excluded.is_empty();
        Ok((cap_uses(project, &res), standalone))
    };
    let whole = resolve(None);
    let (mut uses, need_entries) = match &whole {
        Ok((u, standalone)) => (u.clone(), *standalone),
        Err(_) => (Vec::new(), true),
    };
    if need_entries {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|e| format!("read {}: {e}", show_path(dir)))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "lu"))
            .collect();
        files.sort();
        let mut loaded = whole.is_ok();
        for f in &files {
            let Ok((more, _)) = resolve(Some(f)) else {
                continue;
            };
            loaded = true;
            for u in more {
                let dup = uses.iter().any(|v| {
                    v.owner == u.owner && v.cap == u.cap && v.reach == u.reach && v.at == u.at
                });
                if !dup {
                    uses.push(u);
                }
            }
        }
        if !loaded {
            return whole.map(|(u, _)| u);
        }
    }
    Ok(uses)
}

// --------------------------------------------------------- tree / why ----

/// `wolf tree [--dir DIR]` — the resolved dependency tree.
pub fn tree(args: &[String]) {
    let (args, dir) = take_dir(args, "tree");
    if !args.is_empty() {
        crate::help::usage_exit("tree");
    }
    require_manifest(&dir, "tree");
    let opts = ResolveOpts {
        lock: read_lock(&dir, "tree"),
        fetch_unpinned: false,
        refresh: false,
        store: None,
        offline: false,
    };
    let project = resolve_or_die(&dir, &opts, "tree");
    print!("{}", wolf_pkg::audit::render_tree(&project));
}

/// `wolf why <alias> [--dir DIR]` — the dependency chain that pulls
/// `alias` into the build (MVS terms: who states the minimum).
pub fn why(args: &[String]) {
    let (args, dir) = take_dir(args, "why");
    let [alias] = args.as_slice() else {
        crate::help::usage_exit("why");
    };
    require_manifest(&dir, "why");
    let opts = ResolveOpts {
        lock: read_lock(&dir, "why"),
        fetch_unpinned: false,
        refresh: false,
        store: None,
        offline: false,
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
            offline: false,
        },
    ))
}

/// The I13 check, driver-side: derive every capability use from the
/// resolved code ([`cap_uses`]) and ask wolf_pkg which packages reach
/// capabilities they never declared (E1504).
pub fn capability_diagnostics(project: &Project, res: &wolf_sema::Resolution) -> Vec<Diagnostic> {
    wolf_pkg::audit::capability_check_uses(project, &cap_uses(project, res))
}

/// The capability a sandbox category carries in a manifest (s217,
/// wolf-lang#615). The ONE table of host builtins is the D33 sandbox
/// table (`ctfe::intrinsics::host_stub`); this match only names each
/// category's manifest word, and it is exhaustive on purpose: a new
/// category cannot land without someone deciding what it costs.
/// `Io` (stdio: `print`, `eprint`, `read_line`), `Clock` and `Random`
/// have no capability name in the manifest grammar — charging them
/// would need a new name, which is a ruling, not a lane's call.
pub fn category_cap(c: wolf_sema::ctfe::SandboxCategory) -> Option<wolf_pkg::manifest::Cap> {
    use wolf_pkg::manifest::Cap;
    use wolf_sema::ctfe::SandboxCategory as S;
    match c {
        S::Fs => Some(Cap::Fs),
        S::Net => Some(Cap::Net),
        S::Env => Some(Cap::Env),
        S::Exec => Some(Cap::Exec),
        S::Ffi => Some(Cap::Ffi),
        S::Io | S::Clock | S::Random => None,
    }
}

/// The capability a prelude name reaches, if it is a host builtin in a
/// capability-carrying sandbox category.
pub fn builtin_cap(name: &str) -> Option<wolf_pkg::manifest::Cap> {
    // PLANT (s217, reverted next): the builtin half answers nothing —
    // trunk's import-only I13. cap_reach and builtin_cap_tests must go red.
    let _ = wolf_sema::ctfe::intrinsics::host_stub(name).and_then(category_cap);
    None
}

/// `display:line:col` for a span in one of the package's files.
fn site(pkg: &wolf_sema::Package, fi: usize, span: wolf_span::Span) -> String {
    let unit = &pkg.files[fi];
    let src = &unit.raw.src;
    let lo = (span.lo as usize).min(src.len());
    let before = &src[..lo];
    let line = before.iter().filter(|&&b| b == b'\n').count() + 1;
    let line_start = before
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    let col = String::from_utf8_lossy(&src[line_start..lo])
        .chars()
        .count()
        + 1;
    // Slashes on every host, as the diagnostic renderer prints paths.
    format!("{}:{line}:{col}", unit.raw.display.replace('\\', "/"))
}

/// A site in the package: (index into `Package::files`, span).
type Site = Option<(usize, wolf_span::Span)>;

/// One module's own reach: the capability, how, and where.
type OwnReach = (wolf_pkg::manifest::Cap, wolf_pkg::audit::Reach, Site);

/// Every place the resolved build's code reaches a capability (s217,
/// wolf-lang#615) — the derivation both the build's E1504 and
/// `wolf audit` read, so the two cannot disagree:
///
/// - **imports** the facade rule names (`use std.net`, `import c`),
///   charged to the importing module's package, as since s51;
/// - **host builtins**: every name the resolver bound to the prelude
///   (`RefTarget::Prelude`) that the sandbox table puts in a
///   capability-carrying category, anywhere in the package's own files
///   — so a builtin in a helper counts however the package reaches the
///   helper, and a fn the package declares under a builtin's name is the
///   package's own item, never a capability;
/// - **std modules**: a std module owns no capability of its own; what
///   its code reaches (its builtins and facade imports, closed over the
///   std modules it imports) is charged to the package whose module
///   imports it. Until s217 std modules fell through to the ROOT
///   package, so `use std.process` asked the root for `net` (its import)
///   and never for `exec` (its `os_spawn`).
pub fn cap_uses(project: &Project, res: &wolf_sema::Resolution) -> Vec<wolf_pkg::audit::CapUse> {
    use wolf_pkg::audit::{CapUse, Reach, import_cap, owner_of};
    use wolf_pkg::manifest::Cap;
    use wolf_sema::graph::BindTarget;
    let pkg = &res.package;
    let is_std =
        |m: usize| pkg.has_std_root && pkg.modules[m].path.first().is_some_and(|s| s == "std");
    // The first site in module `m` that names `dep` (its `use` line).
    let import_site = |m: usize, dep: usize| -> Site {
        let md = &pkg.modules[m];
        md.bindings
            .iter()
            .enumerate()
            .find_map(|(k, file_bindings)| {
                file_bindings.iter().find_map(|b| match &b.target {
                    BindTarget::PkgModule(d) | BindTarget::Item { module: d, .. } if *d == dep => {
                        Some((md.files[k], b.decl_span))
                    }
                    _ => None,
                })
            })
    };
    // One module's own reach: (cap, how, site) in source order.
    let own_of = |m: usize| -> Vec<OwnReach> {
        let md = &pkg.modules[m];
        let mut v = Vec::new();
        for &d in &md.deps {
            let target = pkg.modules[d].dotted();
            if let Some(cap) = import_cap(&target) {
                v.push((cap, Reach::Import { target }, import_site(m, d)));
            }
        }
        for (k, file_bindings) in md.bindings.iter().enumerate() {
            for b in file_bindings {
                if b.target == BindTarget::CNamespace {
                    let target = wolf_pkg::audit::C_IMPORT_TARGET.to_string();
                    v.push((
                        Cap::Ffi,
                        Reach::Import { target },
                        Some((md.files[k], b.decl_span)),
                    ));
                }
            }
        }
        for &fi in &md.files {
            for r in res.refs.get(fi).into_iter().flatten() {
                if let wolf_sema::RefTarget::Prelude(name) = &r.target
                    && let Some(cap) = builtin_cap(name)
                {
                    v.push((
                        cap,
                        Reach::Builtin { name: name.clone() },
                        Some((fi, r.span)),
                    ));
                }
            }
        }
        v
    };
    let own: Vec<Vec<OwnReach>> = (0..pkg.modules.len()).map(own_of).collect();
    // Each std module's reach, closed over its std imports: cap → the
    // first builtin (or facade import) that reaches it.
    let mut std_reach: Vec<Option<BTreeMap<Cap, String>>> = vec![None; pkg.modules.len()];
    fn close(
        m: usize,
        pkg: &wolf_sema::Package,
        is_std: &dyn Fn(usize) -> bool,
        own: &[Vec<OwnReach>],
        memo: &mut [Option<BTreeMap<Cap, String>>],
    ) -> BTreeMap<Cap, String> {
        if let Some(done) = &memo[m] {
            return done.clone();
        }
        // Cycle-closing edges are excluded from `deps`, so the std
        // graph is a DAG; the placeholder only guards a malformed one.
        memo[m] = Some(BTreeMap::new());
        let mut acc: BTreeMap<Cap, String> = BTreeMap::new();
        for (cap, how, _) in &own[m] {
            let via = match how {
                Reach::Import { target } => target,
                Reach::Builtin { name } => name,
                Reach::Std { via, .. } => via,
            };
            acc.entry(*cap).or_insert_with(|| via.clone());
        }
        for &d in &pkg.modules[m].deps {
            if is_std(d) {
                for (cap, via) in close(d, pkg, is_std, own, memo) {
                    acc.entry(cap).or_insert(via);
                }
            }
        }
        memo[m] = Some(acc.clone());
        acc
    }
    let mut out = Vec::new();
    for m in 0..pkg.modules.len() {
        let dotted = pkg.modules[m].dotted();
        let Some(owner) = owner_of(project, &dotted, is_std(m)) else {
            continue;
        };
        let at = |s: Site| s.map(|(fi, sp)| site(pkg, fi, sp));
        for (cap, reach, s) in own[m].iter().cloned() {
            out.push(CapUse {
                owner,
                cap,
                module: dotted.clone(),
                reach,
                span: s.map(|(_, sp)| sp),
                at: at(s).unwrap_or_else(|| "its imports".to_string()),
            });
        }
        for &d in &pkg.modules[m].deps {
            if !is_std(d) {
                continue;
            }
            let s = import_site(m, d);
            for (cap, via) in close(d, pkg, &is_std, &own, &mut std_reach) {
                out.push(CapUse {
                    owner,
                    cap,
                    module: dotted.clone(),
                    reach: Reach::Std {
                        module: pkg.modules[d].dotted(),
                        via,
                    },
                    span: s.map(|(_, sp)| sp),
                    // A home-module edge (`[type.method.home]`) has no
                    // `use` line: the method call loaded it.
                    at: at(s).unwrap_or_else(|| "a method call (its home module)".to_string()),
                });
            }
        }
    }
    out
}

/// The static transparency log, when the environment names one:
/// `WOLF_LOG` is a directory holding `log` + `log.head` (dumb files),
/// `WOLF_LOG_KEY` an optional 64-hex-char b3k key that must also
/// verify the head. Client-side verification is transport-agnostic by
/// construction (X7): c15 changes how these bytes arrive, not what
/// this function does with them.
fn log_env(cmd: &str) -> Option<(PathBuf, Option<[u8; 32]>)> {
    let dir = std::env::var_os("WOLF_LOG").map(PathBuf::from)?;
    let key = match std::env::var("WOLF_LOG_KEY") {
        Ok(hexkey) => match parse_key(&hexkey) {
            Ok(k) => Some(k),
            Err(e) => {
                eprintln!("wolf {cmd}: WOLF_LOG_KEY: {e}");
                std::process::exit(2);
            }
        },
        Err(_) => None,
    };
    Some((dir, key))
}

fn parse_key(hexkey: &str) -> Result<[u8; 32], String> {
    let hexkey = hexkey.trim();
    if hexkey.len() != 64 {
        return Err(format!("a b3k key is 64 hex chars, got {}", hexkey.len()));
    }
    let mut key = [0u8; 32];
    for i in 0..32 {
        key[i] = u8::from_str_radix(&hexkey[2 * i..2 * i + 2], 16)
            .map_err(|_| "a b3k key is hex".to_string())?;
    }
    Ok(key)
}

/// Verify against the environment's log, if any; supply-chain
/// mismatches are fatal BEFORE any ledger write.
fn verify_log_or_die(project: &Project, cmd: &str) {
    let Some((dir, key)) = log_env(cmd) else {
        return;
    };
    let errs = wolf_pkg::log::verify_project_against_log(project, &dir, key.as_ref());
    if !errs.is_empty() {
        for e in &errs {
            eprintln!("wolf {cmd}: {e}");
        }
        eprintln!("wolf {cmd}: transparency-log verification failed — nothing was written");
        std::process::exit(1);
    }
}

// ------------------------------------------------------------- vendor ----

/// Portable recursive copy (tier-1 includes windows: no symlink
/// tricks, no permission bits beyond what create/write give).
fn copy_tree_portable(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("create {}: {e}", show_path(to)))?;
    let rd = std::fs::read_dir(from).map_err(|e| format!("read {}: {e}", show_path(from)))?;
    for entry in rd {
        let entry = entry.map_err(|e| format!("read {}: {e}", show_path(from)))?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_tree_portable(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| format!("copy {}: {e}", show_path(&src)))?;
        }
    }
    Ok(())
}

/// `wolf vendor [--dir DIR]` — write the store-backed slice of the
/// resolution into `vendor/wolf/<multihash>/`, a store-layout mirror
/// the next build prefers automatically. Every guarantee carries
/// over because the vendor tree IS a store: hashes re-derive on use
/// (E1506), so a tampered vendor tree fails exactly like a tampered
/// store — mirrors are untrusted by construction.
pub fn vendor(args: &[String]) {
    let (args, dir) = take_dir(args, "vendor");
    if !args.is_empty() {
        crate::help::usage_exit("vendor");
    }
    require_manifest(&dir, "vendor");
    let opts = ResolveOpts {
        lock: read_lock(&dir, "vendor"),
        fetch_unpinned: false,
        refresh: false,
        store: None,
        offline: false,
    };
    let project = resolve_or_die(&dir, &opts, "vendor");
    let vend = wolf_pkg::project::vendor_dir(&dir);
    let mut wrote = 0usize;
    for p in &project.pkgs[1..] {
        let Some(hash) = &p.hash else { continue };
        let dst = wolf_pkg::source::store_path(&vend, hash);
        if dst.is_dir() {
            println!("wolf vendor: {} {} (already vendored)", p.alias, hash);
            continue;
        }
        if let Err(e) = copy_tree_portable(&p.root, &dst) {
            eprintln!("wolf vendor: {e}");
            std::process::exit(1);
        }
        println!("wolf vendor: {} {} -> {}", p.alias, hash, show_path(&dst));
        wrote += 1;
    }
    if wrote == 0 && !vend.is_dir() {
        println!(
            "wolf vendor: nothing store-backed to vendor (path dependencies travel with the tree)"
        );
    }
}

// ------------------------------------------------------------ publish ----

/// `wolf publish [--dir DIR] [--log DIR --key FILE]` — v1: verify the
/// package (manifest gates, full resolution, the E1504 capability
/// check), compute the three content addresses, and emit the signed
/// log record. With `--log`, this IS the static log's maintainer flow:
/// append + re-head (append-only — a published version is immutable,
/// even for its author). Without it, the record lands in
/// `.wolf-publish/` as the submission bundle for a maintainer holding
/// the key. Self-serve publishing (accounts, 2FA) is registry-launch
/// material (c15) and changes none of these bytes.
pub fn publish(args: &[String]) {
    let (args, dir) = take_dir(args, "publish");
    let mut log_dir: Option<PathBuf> = None;
    let mut key_file: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--log" => {
                i += 1;
                log_dir = args.get(i).map(PathBuf::from);
            }
            "--key" => {
                i += 1;
                key_file = args.get(i).map(PathBuf::from);
            }
            _ => {
                crate::help::usage_exit("publish");
            }
        }
        i += 1;
    }
    let manifest_text = require_manifest(&dir, "publish");
    let opts = ResolveOpts {
        lock: read_lock(&dir, "publish"),
        fetch_unpinned: false,
        refresh: false,
        store: None,
        offline: false,
    };
    let project = resolve_or_die(&dir, &opts, "publish");
    let root_pkg = &project.pkgs[0];
    if root_pkg.name.is_empty() || !root_pkg.name.contains('/') {
        eprintln!(
            "wolf publish: `{}` is not a scoped owner/pkg name — the log has no flat namespace",
            root_pkg.name
        );
        std::process::exit(1);
    }

    // Full resolution + the capability gate: publishing an
    // undeclared-capability package is refused exactly like building
    // one (I13 — the audit record other people rely on starts true).
    let mut sm = wolf_span::SourceMap::new();
    let mut sources = Sources::new();
    let owned = project_for_build(&dir, &mut sm);
    let project_ref = owned.as_ref().unwrap_or(&project);
    for m in &project_ref.manifests {
        sources.add(m.file, m.display.clone(), m.text.as_bytes());
    }
    let mut loader =
        wolf_sema::DiskLoader::from_dir(&dir, &mut sm).with_std_root(project_ref.std_root.clone());
    loader = loader.with_dep_roots(project_ref.dep_roots.clone());
    let res = match wolf_sema::resolve_package(&mut loader, &wolf_sema::AliasTable::default()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("wolf publish: {e}");
            std::process::exit(1);
        }
    };
    for unit in &res.package.files {
        sources.add(unit.raw.file, unit.raw.display.clone(), &unit.raw.src);
    }
    let mut diags = res.diagnostics.clone();
    diags.extend(capability_diagnostics(project_ref, &res));
    if diags
        .iter()
        .any(|d| d.severity == wolf_diag::Severity::Error)
    {
        let mut reporter = HumanReporter::new(&sources, RenderOptions::default());
        for d in &diags {
            reporter.report(d);
        }
        eprint!("{}", reporter.take_output());
        eprintln!("wolf publish: the package does not verify; nothing was published");
        std::process::exit(1);
    }

    // The three addresses that make the version immutable.
    let tree = match wolf_pkg::hash_tree(&dir) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("wolf publish: {e}");
            std::process::exit(1);
        }
    };
    // The address is over the interface's CONTENT — the toolchain
    // stamp stays out of it (#292), so a compiler release that touches
    // no `pub` surface leaves `interface=` where `tree=` and
    // `manifest=` already were.
    let ifaces = wolf_sema::build_interfaces(&res.package);
    let iface_text: String = ifaces.iter().map(wolf_sema::digest_text).collect();
    let record = wolf_pkg::log::LogRecord {
        key: format!("{}@{}", root_pkg.name, root_pkg.version),
        tree,
        manifest: wolf_pkg::log::hash_bytes(manifest_text.as_bytes()),
        interface: wolf_pkg::log::hash_bytes(iface_text.as_bytes()),
    };

    match log_dir {
        Some(logd) => {
            let Some(key_file) = key_file else {
                eprintln!("wolf publish: --log needs --key FILE (the b3k head key)");
                std::process::exit(2);
            };
            let key = match std::fs::read_to_string(&key_file)
                .map_err(|e| format!("read {}: {e}", show_path(&key_file)))
                .and_then(|t| parse_key(&t))
            {
                Ok(k) => k,
                Err(e) => {
                    eprintln!("wolf publish: --key: {e}");
                    std::process::exit(2);
                }
            };
            if let Err(e) = std::fs::create_dir_all(&logd) {
                eprintln!("wolf publish: create {}: {e}", show_path(&logd));
                std::process::exit(1);
            }
            let log_file = logd.join("log");
            if let Err(e) = wolf_pkg::log::append(&log_file, &record) {
                eprintln!("wolf publish: {e}");
                std::process::exit(1);
            }
            let lines = match wolf_pkg::log::read_records(&log_file) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("wolf publish: {e}");
                    std::process::exit(1);
                }
            };
            let head = wolf_pkg::log::TreeHead::over(&lines, &key);
            if let Err(e) = std::fs::write(wolf_pkg::log::head_path(&log_file), head.render()) {
                eprintln!("wolf publish: write head: {e}");
                std::process::exit(1);
            }
            println!(
                "wolf publish: {} appended (log size {}, head signed)",
                record.key, head.size
            );
        }
        None => {
            let bundle_dir = dir.join(".wolf-publish");
            if let Err(e) = std::fs::create_dir_all(&bundle_dir) {
                eprintln!("wolf publish: create {}: {e}", show_path(&bundle_dir));
                std::process::exit(1);
            }
            let fname = format!(
                "{}.record",
                record.key.replace('/', "-").replace('@', "-at-")
            );
            let path = bundle_dir.join(fname);
            if let Err(e) = std::fs::write(&path, record.render()) {
                eprintln!("wolf publish: write {}: {e}", show_path(&path));
                std::process::exit(1);
            }
            print!("{}", record.render());
            println!(
                "wolf publish: record written to {} — submit it to your log's maintainer",
                show_path(&path)
            );
        }
    }
}

/// One spelling for every path a package verb prints (wolf-lang#222):
/// forward slashes on every host — the rule `RawFile.display` already
/// applies to every diagnostic path (`--> app/main.lu:3:5` on windows
/// too), so `wolf add`/`publish`/`vendor`/`init` say `app/wolf.pkg`
/// where the diagnostics would, instead of `app\wolf.pkg` beside them.
pub(crate) fn show_path(p: &std::path::Path) -> String {
    p.display().to_string().replace('\\', "/")
}

#[cfg(test)]
mod show_path_tests {
    use super::show_path;
    use std::path::Path;

    /// Both spellings a host can hand the verbs: the native windows
    /// separator normalizes, the slash spelling is already canonical
    /// and passes through byte-identical.
    #[test]
    fn both_separator_spellings_print_with_slashes() {
        assert_eq!(show_path(Path::new("app\\wolf.pkg")), "app/wolf.pkg");
        assert_eq!(
            show_path(Path::new("app\\.wolf-publish\\x.record")),
            "app/.wolf-publish/x.record"
        );
        assert_eq!(show_path(Path::new("app/wolf.pkg")), "app/wolf.pkg");
        assert_eq!(
            show_path(Path::new("app/.wolf-publish/x.record")),
            "app/.wolf-publish/x.record"
        );
    }
}

#[cfg(test)]
mod builtin_cap_tests {
    use super::builtin_cap;
    use wolf_pkg::manifest::Cap;

    /// The sandbox table decides; this only names the manifest word.
    /// Every capability-carrying family is charged, by its first and
    /// a later member (s217, wolf-lang#615).
    #[test]
    fn each_capability_family_is_charged_from_the_sandbox_table() {
        for (name, cap) in [
            ("read_text", Cap::Fs),
            ("fs_read_text", Cap::Fs),
            ("fs_read_at", Cap::Fs),
            ("net_fetch", Cap::Net),
            ("net_nodelay", Cap::Net),
            ("env_var", Cap::Env),
            ("os_cwd", Cap::Env),
            ("os_cpus", Cap::Env),
            ("os_spawn", Cap::Exec),
            ("os_exit", Cap::Exec),
            ("os_signal_listen", Cap::Exec),
        ] {
            assert_eq!(builtin_cap(name), Some(cap), "{name}");
        }
    }

    /// Stdio, the clock and randomness have sandbox categories (they are
    /// refused at comptime) but no capability name in the manifest; the
    /// pure builtins and every non-builtin have neither.
    #[test]
    fn stdio_clock_random_and_pure_names_carry_no_capability() {
        for name in [
            "print",
            "eprint",
            "read_line",
            "clock_ms",
            "time_sleep_ms",
            "random_seed",
            "os_random",
            "json_get",
            "str_from_utf8",
            "region_bytes",
            "left",
        ] {
            assert_eq!(builtin_cap(name), None, "{name}");
        }
    }
}
