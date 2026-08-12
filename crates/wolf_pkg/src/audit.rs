//! Capability manifests + `wolf audit` (s51 Target 6, I13).
//!
//! Every package declares its ambient-authority footprint; the audit
//! surface renders the transitive tree, and upgrades that *acquire*
//! authority are surfaced before the ledger changes (CI-enforceable).
//! Enforcement's static half lives here too: a package whose modules
//! import a capability-carrying std facade module without declaring
//! the capability fails its build (E1504) — the tree is only
//! trustworthy if it cannot silently under-report.

use std::collections::{BTreeMap, BTreeSet};

use wolf_diag::{Diagnostic, codes};

use crate::lock::Lock;
use crate::manifest::Cap;
use crate::project::Project;

/// Which capability a std facade module carries (the import-graph half
/// of I13 enforcement; `ffi`/`unsafe`/`comptime` come from the
/// compiler's own analysis — the s22 audit surface — not from imports).
pub fn std_module_cap(first_seg_after_std: &str) -> Option<Cap> {
    match first_seg_after_std {
        "net" => Some(Cap::Net),
        "fs" => Some(Cap::Fs),
        "env" => Some(Cap::Env),
        _ => None,
    }
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

/// The effective transitive capability set: the union over every
/// package in the build.
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

/// The import-graph half of I13 enforcement. `module_imports` is the
/// resolved build's module graph: (dotted module name, dotted names it
/// imports). A module's owning package is the resolved package whose
/// alias is the module's first segment (the root package otherwise);
/// an import of `std.net`/`std.fs`/`std.env` requires the owner to
/// declare the capability. Undeclared use is E1504 — an error, never
/// a warning.
pub fn capability_check(
    project: &Project,
    module_imports: &[(String, Vec<String>)],
) -> Vec<Diagnostic> {
    let by_alias: BTreeMap<&str, usize> = project
        .pkgs
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, p)| !p.is_std)
        .map(|(i, p)| (p.alias.as_str(), i))
        .collect();
    // (owner package, capability) -> first importing (module, target).
    let mut missing: BTreeMap<(usize, Cap), (String, String)> = BTreeMap::new();
    for (module, imports) in module_imports {
        let first = module.split('.').next().unwrap_or("");
        let owner = by_alias.get(first).copied().unwrap_or(0);
        for target in imports {
            let Some(rest) = target.strip_prefix("std.") else {
                continue;
            };
            let seg = rest.split('.').next().unwrap_or("");
            let Some(cap) = std_module_cap(seg) else {
                continue;
            };
            if project.pkgs[owner].caps.contains(&cap) {
                continue;
            }
            missing
                .entry((owner, cap))
                .or_insert_with(|| (module.clone(), target.clone()));
        }
    }
    let mut diags = Vec::new();
    for ((owner, cap), (module, target)) in missing {
        let p = &project.pkgs[owner];
        let where_ = if owner == 0 {
            "this package".to_string()
        } else {
            format!("dependency `{}`", p.alias)
        };
        let module = if module.is_empty() {
            "the root module".to_string()
        } else {
            format!("module `{module}`")
        };
        let Some(span) = p.blame_span else { continue };
        diags.push(
            Diagnostic::error(
                codes::E1504,
                span,
                format!(
                    "{where_} uses `{target}` but does not declare the `{}` capability",
                    cap.as_str()
                ),
            )
            .with_label(format!("declared capabilities: {}", caps_str(&p.caps)))
            .with_note(format!(
                "{module} imports `{target}`. Add `{}` to this package's \
                 `capabilities: [ … ]` (making the footprint visible to every \
                 consumer running `wolf audit`, I13), or drop the import.",
                cap.as_str()
            )),
        );
    }
    diags
}
