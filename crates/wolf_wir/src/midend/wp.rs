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
//! # Where a profile enters (s45)
//!
//! At step 3 and again at step 6, and nowhere else. Both are the
//! points at which a summary index exists, and a summary index is the
//! only thing a `.wprof` can be matched against: records are keyed by
//! the D8 body hash, and the summary is where that hash lives.
//!
//! - **Step 3** fills the reserved `hot=` slot, and the clusterer, the
//!   import decision and the cross-cluster inline round read it.
//! - **Step 6** fills it again on the re-summarized index, so the
//!   PUBLISHED index — the one `--codegen-report` prints, the one c12
//!   reads, the one the cluster cache key folds — carries the hotness
//!   of the bodies the backend will actually see.
//!
//! The module phase (step 1) runs unprofiled by construction: its
//! inlining is what turns the pre-mid-end bodies into the ones the
//! profile names, so at step 1 there is nothing to match yet. Bodies
//! the cross-cluster round then changes get a fresh hash and lose
//! their step-3 record at step 6 — which is stale-record handling
//! working exactly as designed, not a gap: leaf bodies, the ones an
//! inliner most wants hotness for, are the ones the round does not
//! change.
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
    /// s94: distinct content-hash classes among `.mono.` instances at
    /// the dedup point (`None` when the program has no instances) —
    /// `instantiations_seen / lowered / unique` is the D8 ratio.
    pub instantiations_unique: Option<usize>,
    /// Distinct source modules seen in the home map.
    pub modules: usize,
    pub clusters: usize,
    /// Bodies imported across cluster boundaries (summary-driven).
    pub imports: usize,
    /// The frozen summary index's digest — what the cache keys fold.
    pub summary_digest: String,
    pub summary_version: u32,
    /// How much of the supplied `.wprof` applied to this build (s45).
    /// `None` when no profile was supplied — the normal case, and the
    /// one that must look and behave exactly like s43's.
    pub profile: Option<crate::profile::Coverage>,
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
        )?;
        if let Some(c) = &self.profile {
            write!(f, "\n  profile: {c}")?;
        }
        Ok(())
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
            hot: None, // step 1 runs unprofiled — see the module docs
        };
        optimize_one(m, fid, ve, th, &mut stats, scope)?;
    }

    // ---- 2. dedup (D8) -------------------------------------------------
    // s94: the health metric's third number, measured at the exact
    // point D8 names — post module-phase, pre merge — so it is the
    // count the dedup pass itself is about to fold to.
    let instantiations_unique = dedup::instantiations_unique(m);
    let dstats = dedup::dedup(m);

    // ---- 3./4. summaries, clusters, imports ----------------------------
    let mut summary = super::summary::summarize(m, homes);
    // The profile enters here, by content hash. With no profile this is
    // skipped entirely and every `hot=` stays `-`, so everything below
    // is the s43 pipeline unchanged.
    let early_coverage = opts
        .profile
        .as_ref()
        .map(|p| super::summary::apply_profile(&mut summary, p));
    let hot = hot_map(&summary);
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
                hot: hot.as_ref(),
            };
            optimize_one(m, fid, ve, th, &mut stats, scope)?;
        }
    }

    // ---- 6. DFE, re-summarize, re-key ----------------------------------
    dead_function_elim(m, &mut stats);
    stats.insts_after = count_insts(m);
    verify_module(m)?;
    let mut summary = super::summary::summarize(m, homes);
    // Re-match against the FINAL bodies: the published index's `hot=`
    // describes what the backend will see, and the coverage reported
    // here is the number `wolf profile show` and the driver's stale
    // warning are about.
    // The reported coverage is the UNION of the two matches, never the
    // second alone: a record that drove a decision at step 3 applied,
    // even if the decision it drove then changed the body out from
    // under it. Saying otherwise would let a build claim to be the
    // no-profile build when it is not.
    let coverage = opts.profile.as_ref().map(|p| {
        let late = super::summary::apply_profile(&mut summary, p);
        match early_coverage {
            Some(early) => early.union(late),
            None => late,
        }
    });
    let clusters = cluster::rekey(&clusters, &summary);
    let stats = WpStats {
        summary_digest: summary.digest(),
        summary_version: SUMMARY_FORMAT_VERSION,
        modules,
        clusters: clusters.len(),
        imports,
        dedup: dstats,
        instantiations_unique,
        opt: stats,
        profile: coverage,
    };
    Ok(WholeProgram {
        summary,
        clusters,
        stats,
    })
}

/// Name → hotness rank, for the bodies a profile matched. `None` when
/// nothing matched at all, which makes the inliner's profiled and
/// unprofiled paths the same code path rather than merely equivalent
/// ones.
fn hot_map(s: &ProgramSummary) -> Option<std::collections::BTreeMap<String, u32>> {
    let m: std::collections::BTreeMap<String, u32> = s
        .funcs
        .iter()
        .filter_map(|f| f.hotness.map(|h| (f.name.clone(), h)))
        .collect();
    (!m.is_empty()).then_some(m)
}

/// Per-function block counts for the bodies `profile` matches in `m`,
/// keyed by FUNCTION NAME and positional over
/// [`crate::print::block_order`] — the branch-weight channel s41 hands
/// to LLVM (`!prof`), and the only place the block-level half of a
/// profile is consumed.
///
/// Taken against the FINAL module, so the hash match is exact: a name
/// appears here only if this build's body for it hashes to a record's
/// key, in which case the record's counts describe this body's blocks
/// one for one.
pub fn branch_weights(
    m: &Module,
    profile: &crate::profile::Profile,
) -> std::collections::BTreeMap<String, Vec<u64>> {
    let mut out = std::collections::BTreeMap::new();
    for (_, f) in m.funcs.iter() {
        let Some(r) = profile.get(&dedup::body_hash(m, f)) else {
            continue;
        };
        if r.blocks.len() != crate::print::block_order(f).len() {
            // The hash fixes the block structure, so this cannot
            // happen against an honest file; refuse the record rather
            // than emit weights against a shape we cannot justify.
            continue;
        }
        out.insert(f.name.clone(), r.blocks.clone());
    }
    out
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
