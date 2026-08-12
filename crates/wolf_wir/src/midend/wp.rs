//! The whole-program phase (s43): the release build compiles the
//! module graph as ONE program.
//!
//! Shape — thin-LTO, with wolf's own twist that the duplicate dies
//! before the cache/backend boundary rather than after:
//!
//! 1. **Module phase.** Every function runs the s42 pipeline with the
//!    inliner's horizon set to its OWN source module. This is what
//!    makes the whole-program phase's wins measurable instead of
//!    accidental: whatever crosses a module boundary crosses it under
//!    a summary-driven decision, counted as such.
//! 2. **Dedup (D8).** Content-identical bodies collapse to one
//!    representative ([`super::dedup`]) — hashed POST-mid-end, so
//!    bodies converge only after their type-dependent differences were
//!    folded away.
//! 3. **Summaries.** [`super::summary::summarize`] over the deduped
//!    module: the frozen index the rest of the phase (and c12, and
//!    s45) reads.
//! 4. **Clusters + imports.** [`super::cluster`] partitions the
//!    deduped call graph and decides, from summaries alone, which
//!    cross-cluster bodies to import.
//! 5. **Cross-cluster inlining.** The s42 inliner again, per cluster,
//!    with members ∪ imports visible; every function it changes runs
//!    the rest of the s42 pipeline so the import is cleaned up in
//!    place (the imported body's constants fold into the caller).
//! 6. **DFE, re-summarize, re-key.** Callees left dead by inlining go;
//!    the surviving bodies are re-summarized and the clusters re-keyed,
//!    so a cluster's cache key names exactly the bodies the backend
//!    will see.
//!
//! Determinism (D4) end to end: no wall clock, no thread order, no
//! core count, no hash-map iteration order — see [`super::cluster`]'s
//! determinism note. The phase is a pure function of the lowered
//! module plus the home map.
//!
//! Conc conservatism across the new boundary: s42's passes are
//! conservative because calls are opaque (nothing sinks, forwards, or
//! coalesces across a call on the same region chain; a region whose
//! handle reaches ANY call never promotes), and a spawn edge IS a
//! call. Clustering does not weaken that — it only changes which
//! bodies are visible to the INLINER — and the import decision refuses
//! task-seam carriers outright, so no spawn edge is ever crossed by a
//! cross-cluster import.

use std::collections::BTreeSet;

use crate::ir::{FuncId, Module};
use crate::verify::{VerifyError, verify_module};

use super::cluster::{self, Cluster};
use super::dedup::{self, DedupStats};
use super::inline::Scope;
use super::summary::{Homes, ProgramSummary, SUMMARY_FORMAT_VERSION};
use super::{OptStats, Options, count_insts, dead_function_elim, optimize_one};

/// What the whole-program phase produced: the frozen summary index,
/// the codegen clusters, and the evidence counters.
#[derive(Clone, Debug)]
pub struct WholeProgram {
    pub summary: ProgramSummary,
    pub clusters: Vec<Cluster>,
    pub stats: WpStats,
}

/// Whole-program evidence (the s43 half of the mid-end's counters).
#[derive(Clone, Debug, Default)]
pub struct WpStats {
    /// The s42 counters, accumulated over every phase.
    pub opt: OptStats,
    pub dedup: DedupStats,
    /// Distinct source modules seen in the home map.
    pub modules: usize,
    pub clusters: usize,
    /// Bodies imported across cluster boundaries (summary-driven).
    pub imports: usize,
    /// The frozen summary index's digest — what the cache keys fold.
    pub summary_digest: String,
    pub summary_version: u32,
}

impl std::fmt::Display for WpStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.opt)?;
        writeln!(
            f,
            "  whole-program: {} module(s) -> {} cluster(s), {} import(s); \
             {} cross-module inline(s), {} cross-cluster",
            self.modules,
            self.clusters,
            self.imports,
            self.opt.cross_module_inlined,
            self.opt.cross_cluster_inlined
        )?;
        write!(
            f,
            "  dedup: {} bodies -> {} unique ({} merged, {} site(s) retargeted{}); \
             summary v{} {}",
            self.dedup.bodies_seen,
            self.dedup.bodies_unique,
            self.dedup.bodies_merged,
            self.dedup.sites_retargeted,
            match self.dedup.ratio() {
                Some(r) => format!(", {r:.2}x"),
                None => String::new(),
            },
            self.summary_version,
            &self.summary_digest[..16.min(self.summary_digest.len())]
        )
    }
}

/// Optimize `m` as one program. `homes` maps each function to its
/// defining source module (see [`Homes`]); an empty map means "one
/// module", which degenerates to the s42 pipeline plus dedup and a
/// single cluster.
pub fn optimize_whole_program(
    m: &mut Module,
    homes: &Homes,
    opts: &Options,
) -> Result<WholeProgram, VerifyError> {
    let th = &opts.thresholds;
    let ve = opts.verify_each;
    let mut stats = OptStats {
        rule_table_len: crate::peephole_rules::RULES.len(),
        funcs: m.funcs.len(),
        insts_before: count_insts(m),
        ..OptStats::default()
    };

    // ---- 1. module phase: same-module horizon --------------------------
    let mut by_home: std::collections::BTreeMap<String, BTreeSet<String>> = Default::default();
    for (_, f) in m.funcs.iter() {
        by_home
            .entry(homes.home_of(&f.name).to_string())
            .or_default()
            .insert(f.name.clone());
    }
    let modules = by_home.len();
    for fid in super::inline::bottom_up_order(m) {
        let home = homes.home_of(&m.funcs[fid].name).to_string();
        let peers = by_home.get(&home).cloned().unwrap_or_default();
        let scope = Scope {
            allow: Some(&peers),
            imported: None,
            homes: Some(homes),
        };
        optimize_one(m, fid, ve, th, &mut stats, scope)?;
    }

    // ---- 2. dedup (D8) -------------------------------------------------
    let dstats = dedup::dedup(m);

    // ---- 3./4. summaries, clusters, imports ----------------------------
    let summary = super::summary::summarize(m, homes);
    let clusters = cluster::partition(&summary, th);
    let clusters = cluster::decide_imports(&clusters, &summary, th);
    let imports: usize = clusters.iter().map(|c| c.imports.len()).sum();

    // ---- 5. cross-cluster inlining -------------------------------------
    // A single cluster with nothing imported still runs this round:
    // members from different source modules were invisible to each
    // other in phase 1, and inside one cluster they are not.
    for c in &clusters {
        let visible = cluster::visible(c);
        let imported: BTreeSet<String> = c.imports.iter().cloned().collect();
        let members: BTreeSet<&str> = c.members.iter().map(String::as_str).collect();
        for fid in super::inline::bottom_up_order(m) {
            if !members.contains(m.funcs[fid].name.as_str()) {
                continue;
            }
            let scope = Scope {
                allow: Some(&visible),
                imported: Some(&imported),
                homes: Some(homes),
            };
            optimize_one(m, fid, ve, th, &mut stats, scope)?;
        }
    }

    // ---- 6. DFE, re-summarize, re-key ----------------------------------
    dead_function_elim(m, &mut stats);
    stats.insts_after = count_insts(m);
    verify_module(m)?;
    let summary = super::summary::summarize(m, homes);
    let clusters = cluster::rekey(&clusters, &summary);
    let stats = WpStats {
        summary_digest: summary.digest(),
        summary_version: SUMMARY_FORMAT_VERSION,
        modules,
        clusters: clusters.len(),
        imports,
        dedup: dstats,
        opt: stats,
    };
    Ok(WholeProgram {
        summary,
        clusters,
        stats,
    })
}

/// The cluster a function belongs to, by name — the driver's lookup
/// when it maps clusters back onto `FuncId`s for codegen.
pub fn owner<'a>(clusters: &'a [Cluster], func: &str) -> Option<&'a Cluster> {
    clusters
        .iter()
        .find(|c| c.members.iter().any(|m| m == func))
}

/// Resolve a cluster's members to ids in `m`, in FuncId order.
pub fn member_ids(m: &Module, c: &Cluster) -> Vec<FuncId> {
    let want: BTreeSet<&str> = c.members.iter().map(String::as_str).collect();
    m.funcs
        .iter()
        .filter(|(_, f)| want.contains(f.name.as_str()))
        .map(|(id, _)| id)
        .collect()
}
