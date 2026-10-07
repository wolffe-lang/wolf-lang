//! Capability manifests + `wolf audit` (s51 Target 6, I13).
//!
//! Every package declares its ambient-authority footprint; the audit
//! surface renders the transitive tree, and upgrades that *acquire*
//! authority are surfaced before the ledger changes (CI-enforceable).
//! Enforcement's static half lives here too: a package whose code
//! reaches a capability it does not declare fails its build (E1504) —
//! the tree is only trustworthy if it cannot silently under-report.
//!
//! What a package REACHES (s217, wolf-lang#615) is three things, each a
//! [`CapUse`] the driver derives from the resolved code: an import of a
//! capability-carrying std facade module or `import c` ([`Reach::Import`]);
//! a prelude host builtin named anywhere in the package's own code — the
//! sandbox table decides its capability ([`Reach::Builtin`]); and a std
//! module the package imports whose own code reaches one
//! ([`Reach::Std`]). Until s217 only the first counted, so a `caps=[]`
//! dependency read the filesystem through `fs_read_text` with no
//! diagnostic and `wolf audit` said `effective: []`.

use std::collections::{BTreeMap, BTreeSet};

use wolf_diag::{Diagnostic, codes};
use wolf_span::Span;

use crate::lock::Lock;
use crate::manifest::Cap;
use crate::project::Project;

/// Which capability a std facade module carries.
pub fn std_module_cap(first_seg_after_std: &str) -> Option<Cap> {
    match first_seg_after_std {
        "net" => Some(Cap::Net),
        "fs" => Some(Cap::Fs),
        "env" => Some(Cap::Env),
        _ => None,
    }
}

/// The import target a `import c "header"` declaration contributes to
/// the capability graph (s46, c10).
pub const C_IMPORT_TARGET: &str = "import c";

/// Which capability an import target carries.
///
/// `std.net`/`std.fs`/`std.env` come from the facade. **`import c`
/// carries `ffi`**: imported C is the obvious hole in a capability
/// manifest — a header can declare anything, and calling into it leaves
/// wolf's world entirely — so opening one is a declared act, visible to
/// every consumer running `wolf audit`.
///
/// What this deliberately does *not* do is decide that an imported
/// declaration also carries `exec`, `net` or `fs`. Inferring that from
/// a C name (`system`, `socket`, `open`) would be a heuristic wearing a
/// guarantee's clothes, and the s46 contract does not say what a
/// declaration carries. `ffi` is the honest floor: it says "this
/// package left the sandbox", which is true, checkable, and diffable.
/// The finer question is recorded for the c10 closeout.
pub fn import_cap(target: &str) -> Option<Cap> {
    if target == C_IMPORT_TARGET {
        return Some(Cap::Ffi);
    }
    let rest = target.strip_prefix("std.")?;
    std_module_cap(rest.split('.').next().unwrap_or(""))
}

/// Render the capability tree, root-first, box-drawn, deterministic.
pub fn render_tree(project: &Project) -> String {
    let mut out = String::from("capability tree (I13)\n");
    if project.pkgs.is_empty() {
        return out;
    }
    render_node(project, 0, "", &mut out);
    let eff = effective(project);
    let eff: Vec<&str> = eff.iter().map(|c| c.as_str()).collect();
    out.push_str(&format!("effective: [{}]\n", eff.join(", ")));
    out
}

fn caps_str(caps: &[Cap]) -> String {
    let words: Vec<&str> = caps.iter().map(|c| c.as_str()).collect();
    format!("[{}]", words.join(", "))
}

fn render_node(project: &Project, i: usize, prefix: &str, out: &mut String) {
    let p = &project.pkgs[i];
    if i == 0 {
        out.push_str(&format!(
            "{} {} (root) caps={}\n",
            p.name,
            p.version,
            caps_str(&p.caps)
        ));
    }
    let deps = &p.deps;
    for (k, &d) in deps.iter().enumerate() {
        let last = k + 1 == deps.len();
        let tee = if last { "└── " } else { "├── " };
        let dp = &project.pkgs[d];
        if dp.is_std {
            out.push_str(&format!("{prefix}{tee}std (the std facade)\n"));
            continue;
        }
        out.push_str(&format!(
            "{prefix}{tee}{} {} caps={}\n",
            dp.alias,
            dp.version,
            caps_str(&dp.caps)
        ));
        let next = format!("{prefix}{}", if last { "    " } else { "│   " });
        render_node(project, d, &next, out);
    }
}

/// The DECLARED transitive capability set: the union over every
/// package's manifest. What the code reaches is [`effective_reached`].
pub fn effective(project: &Project) -> BTreeSet<Cap> {
    project
        .pkgs
        .iter()
        .flat_map(|p| p.caps.iter().copied())
        .collect()
}

/// One package's capability acquisition/loss against the ledger.
#[derive(Debug, PartialEq, Eq)]
pub struct CapDelta {
    pub alias: String,
    pub added: Vec<Cap>,
    pub removed: Vec<Cap>,
}

/// Diff the resolved graph's capability sets against the recorded
/// ledger — the I13 upgrade gate: acquisition is surfaced *before*
/// the lockfile changes. New packages report their whole set as added.
pub fn diff_against_lock(project: &Project, lock: &Lock) -> Vec<CapDelta> {
    let mut out = Vec::new();
    for p in &project.pkgs[1..] {
        if p.is_std {
            continue;
        }
        let old: BTreeSet<Cap> = lock
            .entries
            .get(&p.alias)
            .map(|e| e.caps.iter().copied().collect())
            .unwrap_or_default();
        let new: BTreeSet<Cap> = p.caps.iter().copied().collect();
        let added: Vec<Cap> = new.difference(&old).copied().collect();
        let removed: Vec<Cap> = old.difference(&new).copied().collect();
        if !added.is_empty() || !removed.is_empty() {
            out.push(CapDelta {
                alias: p.alias.clone(),
                added,
                removed,
            });
        }
    }
    out
}

/// How a package reaches a capability (s217).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// An import the facade rule names: `use std.net` / `std.fs` /
    /// `std.env`, or `import c` (→ `ffi`).
    Import { target: String },
    /// A prelude host builtin named in the package's own code; its
    /// capability is the sandbox table's category for the name.
    Builtin { name: String },
    /// A std module the package imports reaches the capability through
    /// its own code (`std.process` → `os_spawn`), `via` naming the first
    /// builtin or facade import that does.
    Std { module: String, via: String },
}

/// One site where a package's code reaches a capability.
#[derive(Debug, Clone)]
pub struct CapUse {
    /// The package charged: an index into [`Project::pkgs`].
    pub owner: usize,
    pub cap: Cap,
    /// The dotted module holding the site (`""` = the root module).
    pub module: String,
    pub reach: Reach,
    /// The site itself (the builtin's name token, the `use` line), when
    /// one is known.
    pub span: Option<Span>,
    /// The site as a reader finds it: `display:line:col`.
    pub at: String,
}

impl CapUse {
    /// "calls `fs_read_text`" — what the site does, for messages.
    pub fn what(&self) -> String {
        match &self.reach {
            Reach::Import { target } => format!("imports `{target}`"),
            Reach::Builtin { name } => format!("calls `{name}`"),
            Reach::Std { module, via } => format!("uses `{module}`, which reaches `{via}`"),
        }
    }

    fn rank(&self) -> u8 {
        match self.reach {
            Reach::Import { .. } => 0,
            Reach::Std { .. } => 1,
            Reach::Builtin { .. } => 2,
        }
    }
}

/// The package that owns a module, by the module's dotted path: the
/// resolved package whose alias is the first segment, the root package
/// otherwise. `None` for a std module (`std_module` — the caller knows
/// whether `std.…` is the facade): std is charged to whoever imports
/// it, never to itself or to the root.
pub fn owner_of(project: &Project, dotted: &str, std_module: bool) -> Option<usize> {
    if std_module {
        return None;
    }
    let first = dotted.split('.').next().unwrap_or("");
    Some(
        project
            .pkgs
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, p)| !p.is_std && p.alias == first)
            .map_or(0, |(i, _)| i),
    )
}

/// The import half alone, from a module→imports table: every import
/// the facade rule names, charged to the module's owner, with no site.
pub fn import_uses(project: &Project, module_imports: &[(String, Vec<String>)]) -> Vec<CapUse> {
    let mut out = Vec::new();
    for (module, imports) in module_imports {
        let std_module = module == "std" || module.starts_with("std.");
        let Some(owner) = owner_of(project, module, std_module) else {
            continue;
        };
        for target in imports {
            let Some(cap) = import_cap(target) else {
                continue;
            };
            out.push(CapUse {
                owner,
                cap,
                module: module.clone(),
                reach: Reach::Import {
                    target: target.clone(),
                },
                span: None,
                at: String::new(),
            });
        }
    }
    out
}

/// The import-graph half of I13 enforcement. `module_imports` is the
/// resolved build's module graph: (dotted module name, dotted names it
/// imports). An import of `std.net`/`std.fs`/`std.env` (or `import c`)
/// requires the owner to declare the capability. Undeclared use is
/// E1504 — an error, never a warning. The build calls
/// [`capability_check_uses`] with the whole derivation.
pub fn capability_check(
    project: &Project,
    module_imports: &[(String, Vec<String>)],
) -> Vec<Diagnostic> {
    capability_check_uses(project, &import_uses(project, module_imports))
}

/// Every (package, capability) its code reaches without declaring it:
/// the first use of each (imports first, then std modules, then
/// builtins; source order within a kind) and how many more there are.
pub fn undeclared<'a>(project: &Project, uses: &'a [CapUse]) -> Vec<(&'a CapUse, usize)> {
    let mut first: BTreeMap<(usize, Cap), (&CapUse, usize)> = BTreeMap::new();
    for u in uses {
        if project.pkgs[u.owner].caps.contains(&u.cap) {
            continue;
        }
        first
            .entry((u.owner, u.cap))
            .and_modify(|(best, n)| {
                *n += 1;
                if u.rank() < best.rank() {
                    *best = u;
                }
            })
            .or_insert((u, 0));
    }
    first.into_values().collect()
}

/// I13 enforcement over the whole derivation (s217): one E1504 per
/// (package, capability) reached without a declaration — at the
/// manifest, where the fix goes, naming the site and what it reaches.
pub fn capability_check_uses(project: &Project, uses: &[CapUse]) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for (u, more) in undeclared(project, uses) {
        let p = &project.pkgs[u.owner];
        let where_ = if u.owner == 0 {
            "this package".to_string()
        } else {
            format!("dependency `{}`", p.alias)
        };
        let module = if u.module.is_empty() {
            "the root module".to_string()
        } else {
            format!("module `{}`", u.module)
        };
        let Some(span) = p.blame_span else { continue };
        let cap = u.cap.as_str();
        let mut d = match &u.reach {
            Reach::Import { target } => Diagnostic::error(
                codes::E1504,
                span,
                format!("{where_} uses `{target}` but does not declare the `{cap}` capability"),
            )
            .with_label(format!("declared capabilities: {}", caps_str(&p.caps)))
            .with_note(format!(
                "{module} imports `{target}`. Add `{cap}` to this package's \
                 `capabilities: [ … ]` (making the footprint visible to every \
                 consumer running `wolf audit`, I13), or drop the import."
            )),
            Reach::Builtin { name } => Diagnostic::error(
                codes::E1504,
                span,
                format!("{where_} calls `{name}` but does not declare the `{cap}` capability"),
            )
            .with_label(format!("declared capabilities: {}", caps_str(&p.caps)))
            .with_note(format!(
                "{module} calls the host builtin `{name}` at {}, which reaches the \
                 `{cap}` capability whether or not the package imports a std \
                 module. Add `{cap}` to this package's `capabilities: [ … ]` \
                 (making the footprint visible to every consumer running \
                 `wolf audit`, I13), or stop calling it.",
                u.at
            )),
            Reach::Std { module: m, via } => Diagnostic::error(
                codes::E1504,
                span,
                format!("{where_} uses `{m}` but does not declare the `{cap}` capability"),
            )
            .with_label(format!("declared capabilities: {}", caps_str(&p.caps)))
            .with_note(format!(
                "{module} imports `{m}` at {}, and `{m}` reaches `{cap}` through \
                 `{via}`. Add `{cap}` to this package's `capabilities: [ … ]` \
                 (making the footprint visible to every consumer running \
                 `wolf audit`, I13), or drop the import.",
                u.at
            )),
        };
        if let (Some(site), Reach::Builtin { name }) = (u.span, &u.reach) {
            d = d.with_secondary(
                site,
                format!("`{name}` reaches the `{cap}` capability here"),
            );
        }
        if more > 0 {
            d = d.with_note(format!(
                "{more} more site{} in the same package reach{} `{cap}`; `wolf audit` lists \
                 the capability with its reason.",
                if more == 1 { "" } else { "s" },
                if more == 1 { "es" } else { "" },
            ));
        }
        diags.push(d);
    }
    diags
}

/// What the code reaches, by capability: the union of every use.
pub fn effective_reached(uses: &[CapUse]) -> BTreeSet<Cap> {
    uses.iter().map(|u| u.cap).collect()
}

/// `wolf audit`'s report (s217): the tree, then `effective` — what the
/// manifests declare united with what the code reaches — then one
/// reason line per (capability, package). `derived` is `Err` when the
/// code could not be loaded: the declared set is all the audit can say,
/// and it says so.
pub fn render_audit(project: &Project, derived: Result<&[CapUse], &str>) -> String {
    let mut out = String::from("capability tree (I13)\n");
    if project.pkgs.is_empty() {
        return out;
    }
    render_node(project, 0, "", &mut out);
    let uses: &[CapUse] = derived.unwrap_or(&[]);
    let mut eff = effective(project);
    eff.extend(effective_reached(uses));
    let words: Vec<&str> = eff.iter().map(|c| c.as_str()).collect();
    out.push_str(&format!("effective: [{}]\n", words.join(", ")));
    for cap in &eff {
        for (i, p) in project.pkgs.iter().enumerate() {
            if p.is_std {
                continue;
            }
            let declared = p.caps.contains(cap);
            let mut sites = uses.iter().filter(|u| u.owner == i && u.cap == *cap);
            let first = sites.clone().min_by_key(|u| u.rank());
            let n = sites.by_ref().count();
            if !declared && first.is_none() {
                continue;
            }
            let who = if i == 0 {
                format!("{} (root)", p.name)
            } else {
                p.alias.clone()
            };
            let more = if n > 1 {
                format!(" (+{} more)", n - 1)
            } else {
                String::new()
            };
            let line = match (declared, first) {
                (true, Some(u)) => format!("declared; {} at {}{more}", u.what(), u.at),
                (true, None) if derived.is_ok() => {
                    "declared (nothing in its code reaches it)".to_string()
                }
                (true, None) => "declared".to_string(),
                (false, Some(u)) => format!("UNDECLARED: {} at {}{more}", u.what(), u.at),
                (false, None) => unreachable!("filtered above"),
            };
            out.push_str(&format!("  {}: {who} — {line}\n", cap.as_str()));
        }
    }
    out
}
