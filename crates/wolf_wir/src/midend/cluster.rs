//! Codegen clusters and the import decision (s43 target 4) — wolf's
//! codegen-units analog, chosen by the compiler and invisible to
//! users.
//!
//! # Clustering
//!
//! The deduped call graph is partitioned into clusters that lower and
//! link in parallel. Two forces, in this order:
//!
//! 1. **Affinity** — hot caller/callee pairs belong together, so no
//!    inlining win is lost to the partition in the first place.
//!    Agglomeration walks call edges by descending weight
//!    (`sites × (1 + loop depth)`) and fuses their groups while the
//!    fused group stays under the target size. This is the
//!    min-cut-flavored half: the heaviest edges are the ones the
//!    partition must not cut.
//! 2. **Balance** — groups are then bin-packed into the cluster count
//!    by descending size into the currently-lightest bin, so summed
//!    WIR size is even and the parallel codegen phase has no straggler.
//!
//! # Determinism (D4)
//!
//! Cluster assignment is a pure function of the deduped graph. Every
//! ordering is explicit and content-derived: edges by
//! `(−weight, caller, callee)`, groups by `(−size, content hash,
//! name)`, bins by `(load, index)`. No wall clock, no thread order, no
//! core count, no hash-map iteration order anywhere. Budgets are
//! iteration/count-fixed. Two clean builds of one commit therefore
//! produce identical clusters — and identical objects.
//!
//! # The import decision (thin-LTO import semantics)
//!
//! A call whose callee lives in another cluster cannot be inlined
//! without the body — so the body is IMPORTED into the caller's
//! cluster: made visible to [`super::inline`], which then applies the
//! ordinary s42 budget to it. Import candidacy is decided from
//! SUMMARIES ALONE (no bodies loaded), under a per-cluster
//! count-fixed budget.
//!
//! Refused by construction, whatever the budget says:
//! - **recursive** callees (the inliner refuses them anyway),
//! - **multi-block callees with effect-token parameters** (the s42 v0
//!   restriction: the final token version per chain must be
//!   syntactically evident),
//! - **task-seam carriers** (`__wolf_rt_scope_*`/`_proc_*`/`_task_*`/
//!   `_chan_*`/`_select_*`): the s42 conc conservatism restated at the
//!   whole-program layer. A spawn edge is a call and calls are opaque;
//!   refusing the IMPORT keeps that reasoning inside one cluster
//!   instead of asking it to hold across a partition it cannot see.
//!
//! Every knob here lives in the mid-end's ONE tunable table
//! ([`Thresholds`], s42 amendment 4) — `cluster_target_size`,
//! `cluster_max`, `import_max`, `import_budget` — never as scattered
//! constants.

use std::collections::{BTreeMap, BTreeSet};

use super::Thresholds;
use super::summary::{FuncSummary, ProgramSummary};

/// One codegen cluster: the functions that lower into one object,
/// plus the bodies imported into it for cross-cluster inlining.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cluster {
    /// `c00`, `c01`, … — the object's cache-friendly name.
    pub name: String,
    /// Member function names, sorted.
    pub members: Vec<String>,
    /// Imported (not owned) function names, sorted — inlining
    /// candidates from other clusters.
    pub imports: Vec<String>,
    /// Summed member WIR size.
    pub size: u32,
    /// Content key: the members' and imports' body hashes. This is
    /// what the driver's cluster-object cache keys on.
    pub key: String,
}

/// Partition `summary`'s call graph. See the module docs for the
/// algorithm and its determinism argument.
pub fn partition(summary: &ProgramSummary, th: &Thresholds) -> Vec<Cluster> {
    let n = summary.funcs.len();
    if n == 0 {
        return Vec::new();
    }
    let idx: BTreeMap<&str, usize> = summary
        .funcs
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();
    let size: Vec<u32> = summary.funcs.iter().map(|f| f.size).collect();
    let total: u64 = size.iter().map(|&s| s as u64).sum();
    let target = th.cluster_target_size.max(1) as u64;
    let want = total.div_ceil(target) as usize;
    let clusters = want.clamp(1, th.cluster_max.max(1)).min(n);
    if clusters == 1 {
        let members: Vec<String> = summary.funcs.iter().map(|f| f.name.clone()).collect();
        return vec![finish("c00", members, Vec::new(), summary)];
    }
    // ---- 1. affinity agglomeration ------------------------------------
    let mut uf: Vec<usize> = (0..n).collect();
    let mut group_size: Vec<u32> = size.clone();
    let mut edges: Vec<(u32, usize, usize)> = Vec::new();
    for (i, f) in summary.funcs.iter().enumerate() {
        for c in &f.calls {
            if let Some(&j) = idx.get(c.callee.as_str())
                && i != j
            {
                edges.push((c.sites * (1 + c.depth), i, j));
            }
        }
    }
    // Descending weight; ties by caller then callee NAME (the summary
    // is name-sorted, so index order is name order).
    edges.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    for (_, i, j) in edges {
        let (ri, rj) = (find(&mut uf, i), find(&mut uf, j));
        if ri == rj {
            continue;
        }
        if group_size[ri] + group_size[rj] > th.cluster_target_size {
            continue; // cutting here keeps the partition balanced
        }
        let (keep, gone) = if ri < rj { (ri, rj) } else { (rj, ri) };
        uf[gone] = keep;
        group_size[keep] += group_size[gone];
        group_size[gone] = 0;
    }
    // ---- 2. balanced bin packing --------------------------------------
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        let r = find(&mut uf, i);
        groups.entry(r).or_default().push(i);
    }
    // Descending group size; ties by the group's smallest body hash,
    // then its smallest name — content, never arena order.
    let mut order: Vec<(u32, &str, &str, Vec<usize>)> = groups
        .into_values()
        .map(|g| {
            let sz: u32 = g.iter().map(|&i| size[i]).sum();
            let hash = g
                .iter()
                .map(|&i| summary.funcs[i].body_hash.as_str())
                .min()
                .unwrap_or("");
            let name = g
                .iter()
                .map(|&i| summary.funcs[i].name.as_str())
                .min()
                .unwrap_or("");
            (sz, hash, name, g)
        })
        .collect();
    order.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)).then(a.2.cmp(b.2)));
    let mut bins: Vec<(u32, Vec<usize>)> = vec![(0, Vec::new()); clusters];
    for (sz, _, _, g) in order {
        let (bi, _) = bins
            .iter()
            .enumerate()
            .min_by_key(|(i, (load, _))| (*load, *i))
            .map(|(i, b)| (i, b.0))
            .expect("at least one bin");
        bins[bi].0 += sz;
        bins[bi].1.extend(g);
    }
    bins.into_iter()
        .enumerate()
        .map(|(i, (_, members))| {
            let mut names: Vec<String> = members
                .into_iter()
                .map(|j| summary.funcs[j].name.clone())
                .collect();
            names.sort();
            finish(&format!("c{i:02}"), names, Vec::new(), summary)
        })
        .filter(|c| !c.members.is_empty())
        .collect()
}

fn find(uf: &mut [usize], mut x: usize) -> usize {
    while uf[x] != x {
        uf[x] = uf[uf[x]];
        x = uf[x];
    }
    x
}

/// Fill in a cluster's derived fields (size and content key).
fn finish(name: &str, members: Vec<String>, imports: Vec<String>, s: &ProgramSummary) -> Cluster {
    let size = members
        .iter()
        .filter_map(|m| s.get(m))
        .map(|f| f.size)
        .sum();
    let mut acc = format!("summary-format {}\ncluster {name}\n", s.version);
    for m in &members {
        acc.push_str(&format!(
            "member {m} {}\n",
            s.get(m).map(|f| f.body_hash.as_str()).unwrap_or("?")
        ));
    }
    for i in &imports {
        acc.push_str(&format!(
            "import {i} {}\n",
            s.get(i).map(|f| f.body_hash.as_str()).unwrap_or("?")
        ));
    }
    Cluster {
        name: name.to_string(),
        members,
        imports,
        size,
        key: crate::hash::sha256_hex(acc.as_bytes()),
    }
}

/// Recompute every cluster's derived fields against a fresh summary
/// (bodies change under cross-cluster inlining; the cache key must
/// name what the backend will actually see).
pub fn rekey(clusters: &[Cluster], s: &ProgramSummary) -> Vec<Cluster> {
    clusters
        .iter()
        .map(|c| {
            let members: Vec<String> = c
                .members
                .iter()
                .filter(|m| s.get(m).is_some())
                .cloned()
                .collect();
            let imports: Vec<String> = c
                .imports
                .iter()
                .filter(|i| s.get(i).is_some())
                .cloned()
                .collect();
            finish(&c.name, members, imports, s)
        })
        .collect()
}

/// Decide the cross-cluster imports for every cluster, from summaries
/// alone. Returns the clusters with `imports` filled in.
pub fn decide_imports(clusters: &[Cluster], s: &ProgramSummary, th: &Thresholds) -> Vec<Cluster> {
    // name → owning cluster index.
    let mut owner: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, c) in clusters.iter().enumerate() {
        for m in &c.members {
            owner.insert(m.as_str(), i);
        }
    }
    clusters
        .iter()
        .enumerate()
        .map(|(ci, c)| {
            // Candidate imports: (benefit, callee) over every member's
            // cross-cluster call edge.
            let mut cand: BTreeMap<&str, u32> = BTreeMap::new();
            for m in &c.members {
                let Some(f) = s.get(m) else { continue };
                for e in &f.calls {
                    if owner.get(e.callee.as_str()).copied() == Some(ci) {
                        continue; // same cluster: ordinary inlining
                    }
                    let Some(callee) = s.get(&e.callee) else {
                        continue;
                    };
                    if !importable(callee, th) {
                        continue;
                    }
                    *cand.entry(callee.name.as_str()).or_insert(0) += e.sites * (1 + e.depth);
                }
            }
            // Highest benefit first; ties by name. Spend the budget.
            let mut ranked: Vec<(u32, &str)> = cand.into_iter().map(|(n, b)| (b, n)).collect();
            ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
            let mut spent = 0u32;
            let mut imports: Vec<String> = Vec::new();
            for (_, name) in ranked {
                let sz = s.get(name).map(|f| f.size).unwrap_or(0);
                if spent + sz > th.import_budget {
                    continue;
                }
                spent += sz;
                imports.push(name.to_string());
            }
            imports.sort();
            finish(&c.name, c.members.clone(), imports, s)
        })
        .collect()
}

/// Import candidacy, from the summary alone (see the module docs).
fn importable(f: &FuncSummary, th: &Thresholds) -> bool {
    if f.flags.recursive || f.flags.task_seam {
        return false;
    }
    if f.flags.token_params && !f.flags.single_block {
        return false;
    }
    // The inliner's own hard cap still applies at the site; importing a
    // body it could never accept is pure compile-time waste.
    f.size <= th.import_max.min(th.inline_single_use.max(th.inline_max))
}

/// The names visible to `c`'s inliner: its members plus its imports.
pub(crate) fn visible(c: &Cluster) -> BTreeSet<String> {
    c.members.iter().chain(c.imports.iter()).cloned().collect()
}
