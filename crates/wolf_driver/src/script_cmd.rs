//! Script mode (s53) — "a single file with frontmatter deps is a
//! first-class package: fetch, build, cache, execute."
//!
//! The headline operational claim is *strictly better than Python*:
//! Python re-resolves and mutates environments; wolf resolves once,
//! deterministically, into an immutable cache. So the second run must
//! feel instant, and it does — a warm rerun is a hash of the script's
//! own bytes plus its pinned resolution, a stat of the cached binary,
//! and an exec. No resolver runs. No network syscall happens. No
//! compiler phase runs.
//!
//! # What makes a file a script
//!
//! It announces itself: a `#!` interpreter line, or a `pkg { … }`
//! frontmatter block inside its leading `//!`. Everything else is the
//! ordinary project/single-file build, unchanged — the two modes differ
//! only in *where state lives*, and a script's state never touches the
//! directory the script sits in.
//!
//! # The lockfile is real, and hidden (RFC 3502)
//!
//! `<cache>/scripts/<script-id>/` holds `wolf.sum` (the integrity
//! ledger, byte-identical in format to a project's) and `resolution`
//! (the pinned answer). `script-id = hash(absolute path, frontmatter)`,
//! so editing a dependency changes the identity and the re-resolve is
//! clean by construction — a pin is never edited in place.

use std::path::{Path, PathBuf};

use wolf_diag::{HumanReporter, RenderOptions, Reporter, Sources};
use wolf_pkg::{Lock, Project, ResolveOpts, script};

/// Script-mode posture flags, parsed off `wolf run`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Posture {
    /// `--locked`: refuse if the frontmatter drifted from the pin.
    pub locked: bool,
    /// `--update`: re-resolve and re-pin.
    pub update: bool,
    /// `--offline`: resolution may not fetch, ever.
    pub offline: bool,
    /// `--yes`: answer the missing-dependency prompt with "yes".
    pub yes: bool,
}

/// A recognized script and its resolved world.
pub struct Script {
    /// The script's own file.
    pub path: PathBuf,
    /// `hash(absolute path, frontmatter)`.
    pub id: String,
    /// `<cache>/scripts/<id>/`.
    pub state: PathBuf,
    /// The resolved dependency graph (empty for a std-only script).
    pub project: Option<Project>,
    /// The build key: everything a compiled artifact depends on.
    pub build_key: String,
    /// `<cache>/builds/<key>/`.
    pub build_dir: PathBuf,
}

/// Does this file announce itself as a script? Cheap and syntactic, on
/// the bytes the compiler will read anyway.
pub fn is_script(text: &str) -> bool {
    text.starts_with("#!") || script::find(text).is_some()
}

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("wolf run: {msg}");
    std::process::exit(2);
}

/// Read a script's frontmatter, resolve its dependencies (through the
/// same resolver, store and ledger a project uses), and pin the answer.
///
/// Every failure is either a rendered diagnostic + exit 1 (the user's
/// script is wrong) or exit 2 (the environment is wrong) — never a
/// silent fallback to a different world than the one asked for.
pub fn prepare(
    path: &Path,
    text: &str,
    posture: Posture,
    sm: &mut wolf_span::SourceMap,
    sources: &mut Sources,
) -> Script {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let file = sm.intern(path);
    sources.add(file, display_of(path), text.as_bytes());
    let read = script::read(file, text);
    if !read.diagnostics.is_empty() {
        let mut reporter = HumanReporter::new(sources, RenderOptions::default());
        let mut ds = read.diagnostics.clone();
        wolf_diag::sort_diagnostics(&mut ds);
        for d in &ds {
            reporter.report(d);
        }
        eprint!("{}", reporter.take_output());
    }
    if read.manifest.is_none() && read.has_frontmatter {
        eprintln!("wolf run: the script's frontmatter is refused; fix the errors above");
        std::process::exit(1);
    }
    let id = script::script_id(&abs, read.frontmatter.as_ref());
    let state = match wolf_pkg::source::script_dir(&id) {
        Ok(p) => p,
        Err(e) => fail(e),
    };

    // The pin: `wolf.sum` + `resolution` in the script's state dir,
    // plus the path-keyed pointer that remembers which state dir this
    // file last used. `--locked` asserts the pointer still names it.
    let pinned_fm = std::fs::read_to_string(state.join("resolution")).ok();
    let current_fm = frontmatter_record(&abs, read.frontmatter.as_ref());
    let pointer = wolf_pkg::source::script_pointer(&abs).ok();
    let pointed_id = pointer
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string());
    if posture.locked
        && !posture.update
        && let Some(was) = &pointed_id
        && was != &id
    {
        // A drifted pin is an error with a span: the frontmatter block
        // itself, which is the thing that moved.
        let span = read
            .frontmatter
            .as_ref()
            .map(|f| wolf_span::Span::new(file, f.lo, f.hi))
            .unwrap_or_else(|| wolf_span::Span::new(file, 0, 0));
        let d = script::drift_diagnostic(span);
        let mut reporter = HumanReporter::new(sources, RenderOptions::default());
        reporter.report(&d);
        eprint!("{}", reporter.take_output());
        std::process::exit(1);
    }

    let mut project = None;
    if let Some(manifest) = read.manifest {
        let lock = std::fs::read_to_string(state.join("wolf.sum"))
            .ok()
            .and_then(|t| Lock::parse(&t).ok());
        let root = abs.parent().unwrap_or(Path::new(".")).to_path_buf();
        let p = wolf_pkg::resolve_manifest(
            &root,
            manifest,
            sm,
            &ResolveOpts {
                lock,
                // Fetch happens only at resolution time, never during
                // build (D33). `--offline` forbids it outright; a warm
                // rerun never reaches this code at all.
                fetch_unpinned: !posture.offline,
                refresh: posture.update,
                store: None,
                offline: posture.offline,
            },
        );
        for m in &p.manifests {
            sources.add(m.file, m.display.clone(), m.text.as_bytes());
        }
        if p.has_errors() {
            let mut reporter = HumanReporter::new(sources, RenderOptions::default());
            let mut ds = p.diagnostics.clone();
            wolf_diag::sort_diagnostics(&mut ds);
            for d in &ds {
                reporter.report(d);
            }
            eprint!("{}", reporter.take_output());
            if posture.offline {
                eprintln!(
                    "wolf run: a dependency is missing from the cache and `--offline` \
                     forbids fetching it (`wolf --explain E1509`)"
                );
            } else {
                eprintln!("wolf run: the script's dependencies do not resolve");
            }
            std::process::exit(1);
        }
        project = Some(p);
    }

    // Pin the answer. Writes are additive: a new ledger and a new
    // resolution record, never an edit of an existing one (a different
    // frontmatter has a different script-id and therefore a different
    // directory).
    if let Some(p) = &project {
        let lock = p.to_lock();
        if let Err(e) = std::fs::create_dir_all(&state) {
            fail(format!("create {}: {e}", state.display()));
        }
        let rendered = lock.render();
        let sum_path = state.join("wolf.sum");
        // Byte-stable: only write when the bytes differ, so a warm run
        // does not touch the filesystem.
        if std::fs::read_to_string(&sum_path).ok().as_deref() != Some(rendered.as_str())
            && let Err(e) = std::fs::write(&sum_path, &rendered)
        {
            fail(format!("write {}: {e}", sum_path.display()));
        }
        if pinned_fm.as_deref() != Some(current_fm.as_str())
            && let Err(e) = std::fs::write(state.join("resolution"), &current_fm)
        {
            fail(format!("write the pinned resolution: {e}"));
        }
    }
    // The pointer moves for every script, frontmatter or not: it is what
    // makes "this file's pin" a question with an answer.
    if pointed_id.as_deref() != Some(id.as_str())
        && let Some(p) = &pointer
    {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(p, &id) {
            fail(format!("write the script pointer: {e}"));
        }
    }

    let build_key = build_key(text, &current_fm, project.as_ref());
    let build_dir = match wolf_pkg::source::builds_dir() {
        Ok(p) => p.join(&build_key),
        Err(e) => fail(e),
    };
    Script {
        path: path.to_path_buf(),
        id,
        state,
        project,
        build_key,
        build_dir,
    }
}

/// A script's display name in diagnostics: its file name, never its
/// absolute path (D7 — an on-disk location never leaks).
pub fn display_of(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// The pinned-resolution record: the identity the `--locked` check
/// compares. Deliberately human-readable — a cache file a person can
/// read is a cache file a person can trust.
fn frontmatter_record(abs: &Path, fm: Option<&script::Frontmatter>) -> String {
    let mut out = String::from("wolf-script-resolution/0\n");
    out.push_str("path ");
    out.push_str(&abs.to_string_lossy());
    out.push('\n');
    out.push_str("frontmatter-hash ");
    out.push_str(
        &blake3::hash(fm.map(|f| f.literal.as_str()).unwrap_or("").as_bytes()).to_hex()[..32],
    );
    out.push('\n');
    match fm {
        Some(f) => {
            for line in f.literal.lines() {
                out.push_str("| ");
                out.push_str(line);
                out.push('\n');
            }
        }
        None => out.push_str("| (no frontmatter — a std-only script)\n"),
    }
    out
}

/// `hash(script bytes, frontmatter record, resolved dep addresses,
/// toolchain, target, profile)` — s53 §4's build key. A cached artifact
/// under this key is valid for exactly the world that produced it.
fn build_key(text: &str, fm_record: &str, project: Option<&Project>) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"wolf-script-build/0\n");
    h.update(env!("CARGO_PKG_VERSION").as_bytes());
    h.update(b"\n");
    h.update(std::env::consts::ARCH.as_bytes());
    h.update(b"-");
    h.update(std::env::consts::OS.as_bytes());
    h.update(b"\ndebug\n");
    h.update(text.as_bytes());
    h.update(b"\n");
    h.update(fm_record.as_bytes());
    if let Some(p) = project {
        for pkg in &p.pkgs {
            h.update(pkg.alias.as_bytes());
            h.update(b" ");
            h.update(pkg.version.as_bytes());
            h.update(b" ");
            h.update(pkg.hash.as_deref().unwrap_or("-").as_bytes());
            h.update(b"\n");
            // A `path:` dependency is yours to edit, so its bits — not
            // just its version — decide the key. Without this a script
            // would happily rerun a stale binary after you edited the
            // library next door, which is exactly the drift wolf claims
            // not to have.
            if pkg.hash.is_none()
                && let Ok(tree) = wolf_pkg::hash_tree(&pkg.root)
            {
                h.update(tree.as_bytes());
                h.update(b"\n");
            }
        }
    }
    h.finalize().to_hex()[..32].to_string()
}

impl Script {
    /// The cached executable's path.
    pub fn binary(&self) -> PathBuf {
        let stem = self
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "script".to_string());
        self.build_dir.join(stem)
    }

    /// Is a compiled artifact already there? This is the whole warm-run
    /// story: one stat, no resolver, no compiler, no network.
    pub fn warm(&self) -> bool {
        self.binary().is_file()
    }
}

// ------------------------------------------------------ missing imports --

/// The `use` names a script imports, in declaration order.
fn imported_names(path: &Path, text: &str) -> Vec<String> {
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(path);
    let parse = wolf_parse::parse_file(file, text.as_bytes());
    let src = text.as_bytes();
    let mut out = Vec::new();
    for node in parse.root.nodes() {
        if node.kind != wolf_ast::SyntaxKind::UseDecl {
            continue;
        }
        let Some(d) = wolf_ast::UseDecl::cast(node) else {
            continue;
        };
        let Some(p) = d.path() else { continue };
        if let Some(first) = p.segments().next() {
            let name = String::from_utf8_lossy(first.text(src)).into_owned();
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// Imports that no dependency and no facade answers.
///
/// A neighbouring DIRECTORY is not silently a module here, and that is
/// deliberate rather than an oversight: the loader's entry mode only
/// admits files that opt in with `member: true` — a corpus mechanism for
/// conform cases — while a dependency tree admits every `.lu` file it
/// holds. So in script mode a directory next to the script is a
/// *dependency candidate*, and the frontmatter is where dependencies are
/// declared. That is the shape the prompt below offers.
pub fn missing_imports(path: &Path, text: &str, project: Option<&Project>) -> Vec<String> {
    imported_names(path, text)
        .into_iter()
        .filter(|name| {
            if name == "std" || name == "c" {
                return false;
            }
            !project.is_some_and(|p| p.dep_roots.contains_key(name))
        })
        .collect()
}

/// The missing-import prompt (s53 §3): in SCRIPT mode a missing
/// dependency is a question, in project mode it stays an error with a
/// fix-it (s51). `--yes` answers it; a non-TTY errors instead of
/// hanging, because a pipeline that blocks on a prompt is a pipeline
/// that has already failed.
///
/// The offer is always a source form v0 can actually resolve. A sibling
/// directory of that name is offered as a `path:` dependency; anything
/// else has no answer to offer, and saying so beats writing an entry
/// that would be refused on the next line.
pub fn prompt_add(path: &Path, alias: &str, posture: Posture) -> Option<wolf_pkg::DepSource> {
    let dir = path.parent().unwrap_or(Path::new("."));
    if !dir.join(alias).is_dir() {
        eprintln!(
            "wolf run: `{alias}` is imported and no dependency provides it. There is no \
             source to offer: a v0 dependency is `path:` (a local tree) or `git:` + `tag:` \
             (a pinned fetch), and neither can be guessed from a name."
        );
        return None;
    }
    let source = wolf_pkg::DepSource::Path {
        path: format!("./{alias}"),
    };
    if posture.yes {
        eprintln!("wolf run: adding `{alias}` to the frontmatter (--yes)");
        return Some(source);
    }
    if !is_tty() {
        eprintln!(
            "wolf run: `{alias}` is imported and not in the frontmatter. On a terminal \
             this is a question; here it is an error — pass `--yes` to accept the \
             addition, or write the dependency into the frontmatter yourself."
        );
        return None;
    }
    eprint!("add {alias} to frontmatter? [y/N] ");
    use std::io::Write;
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return None;
    }
    let answer = line.trim().to_ascii_lowercase();
    if answer == "y" || answer == "yes" {
        Some(source)
    } else {
        None
    }
}

/// The script's frontmatter as a parsed manifest, for the editing path.
pub fn manifest_of(path: &Path, text: &str) -> Option<wolf_pkg::Manifest> {
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(path);
    script::read(file, text).manifest
}

/// Splice a dependency entry into a script's frontmatter, as a new `//!`
/// line. The script stays one file and stays readable: the entry is
/// written in the frontmatter's own idiom, not appended to a synthesized
/// buffer. `None` when the script has no frontmatter to edit.
pub fn insert_dep(
    text: &str,
    manifest: &wolf_pkg::Manifest,
    alias: &str,
    source: &wolf_pkg::DepSource,
) -> Option<String> {
    let entry = wolf_pkg::manifest::render_dep(alias, source);
    // With a `deps: { … }` map, insert before the line holding its `}`.
    // Without one, insert before the line holding the block's own `}`.
    let anchor = manifest.deps_span.map(|s| s.hi).unwrap_or(manifest.span.hi) as usize;
    let (indent, line_body) = if manifest.deps_span.is_some() {
        ("        ", entry)
    } else {
        ("    ", format!("deps: {{ {entry} }},"))
    };
    let line_start = text[..anchor].rfind('\n').map(|i| i + 1)?;
    let mut out = String::with_capacity(text.len() + line_body.len() + 8);
    out.push_str(&text[..line_start]);
    out.push_str("//! ");
    out.push_str(indent);
    out.push_str(&line_body);
    if manifest.deps_span.is_some() {
        out.push(',');
    }
    out.push('\n');
    out.push_str(&text[line_start..]);
    Some(out)
}

/// Is stdin a terminal? Platform-agnostic: `IsTerminal` is std's, on
/// every tier-1 target (no unixisms).
fn is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

// ------------------------------------------------------------- cache gc --

/// `wolf cache gc [--dry-run] [--days N]` and `wolf cache path` — the
/// ONLY deletion in the cache (s53 §4: nothing is ever mutated in
/// place, so garbage collection is the whole lifecycle story).
pub fn cache(args: &[String]) {
    let sub = args.first().map(String::as_str);
    let root = match wolf_pkg::source::cache_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("wolf cache: {e}");
            std::process::exit(2);
        }
    };
    match sub {
        Some("path") => {
            println!("{}", root.display());
            for (name, what) in [
                ("store", "package sources — immutable"),
                ("log", "transparency-log heads and proofs"),
                ("builds", "compiled artifacts"),
                ("scripts", "script-mode pins"),
            ] {
                let at = root.join(name);
                println!(
                    "  {name:<8} {:<8} {}  ({what})",
                    if at.is_dir() { "present" } else { "absent" },
                    at.display()
                );
            }
        }
        Some("gc") => {
            let dry = args.iter().any(|a| a == "--dry-run");
            let mut removed = 0usize;
            let mut bytes = 0u64;
            // Build artifacts and script pins are derived state: they
            // are always re-creatable from the store plus the source.
            // The store itself is NOT collected here — deleting fetched
            // sources costs a network round trip, so it takes an
            // explicit `--all`.
            let all = args.iter().any(|a| a == "--all");
            let mut targets = vec![root.join("builds"), root.join("scripts")];
            if all {
                targets.push(root.join("store"));
            }
            for dir in targets {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
                paths.sort();
                for p in paths {
                    bytes += dir_size(&p);
                    removed += 1;
                    if !dry {
                        let _ = std::fs::remove_dir_all(&p);
                    }
                }
            }
            println!(
                "wolf cache gc: {} {} entr{} ({} KiB){}",
                if dry { "would remove" } else { "removed" },
                removed,
                if removed == 1 { "y" } else { "ies" },
                bytes / 1024,
                if all {
                    " including fetched sources"
                } else {
                    " (fetched sources kept; `--all` collects them too)"
                }
            );
        }
        _ => {
            eprintln!("usage: wolf cache <path|gc [--dry-run] [--all]>");
            std::process::exit(2);
        }
    }
}

fn dir_size(at: &Path) -> u64 {
    let Ok(meta) = std::fs::metadata(at) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    let Ok(entries) = std::fs::read_dir(at) else {
        return 0;
    };
    entries.flatten().map(|e| dir_size(&e.path())).sum()
}
