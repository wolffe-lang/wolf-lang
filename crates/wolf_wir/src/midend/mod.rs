//! The WIR mid-end (s42, c09): the optimizer that makes the facts pay
//! before LLVM ever sees the module — inlining (bottom-up, callee-
//! simplify-first), rule-table GVN + folding + DCE, region-aware
//! memory optimization, e-SSA/π-node value-range analysis with
//! checked-arithmetic elimination and loop-level check versioning,
//! region-bounded allocation sinking (stack promotion), bump-
//! allocation coalescing, and LICM. LLVM receives small, already-
//! optimized, fact-annotated IR (D1/D3); rustc's firehose posture is
//! the anti-goal, stated as budgets in the sprint contract.
//!
//! # The pass manager (target 1, superseding the s24 stub)
//!
//! Every pass runs under [`optimize_module`]'s manager:
//! - **In-place mutation, batch repair** (the egg discipline): passes
//!   mutate freely and may orphan instructions, values, and whole
//!   blocks; the pass boundary rebuilds the function via
//!   [`compact`](compact::compact) — reachable-RPO layout, dense
//!   renumbering, canonical block order. No incremental CFG/dominator
//!   maintenance exists anywhere in this module.
//! - **Fact custody (D2)**: the manager snapshots the fact table
//!   around every pass. A fact changed or dropped while its subject
//!   survives requires an explicit [`PassCtx`] invalidation from the
//!   pass; a fact whose operands died with removed code is dropped by
//!   compaction and audited as value-deleted. A `ValueDeleted` claim
//!   whose subject survived is rejected as a lie. This supersedes the
//!   arena-containment check of the s24 `run_pass` stub (arenas are
//!   append-only, so containment never fired); "the value no longer
//!   exists" now means "nothing in the reachable layout defines it".
//! - **Verify-between-passes**: with [`Options::verify_each`] (debug
//!   compilers, CI, `WOLF_VERIFY_EACH_PASS=1`) every pass boundary
//!   re-verifies the whole function; the pipeline always verifies the
//!   module once at the end regardless.
//!
//! # Conc conservatism (s73 seam)
//!
//! Concurrency lowers to `func.addr` + calls into `__wolf_rt_*` task
//! seams, and every runtime call is a potential schedule point
//! (spec/07). The passes are conservative by construction: calls are
//! opaque (nothing sinks, forwards, or coalesces across a call on the
//! same region chain; regions whose handles or pointers reach ANY call
//! never promote), and `func.addr` is a pure link-time constant that
//! no pass touches beyond GVN dedup. A spawn edge is a call — the
//! barrier falls out of the token discipline plus the escape checks.

pub(crate) mod analysis;
pub mod cluster;
mod coalesce;
mod compact;
pub mod dedup;
mod inline;
pub mod instrument;
mod licm;
mod memopt;
mod rangeopt;
mod simplify;
mod sink;
pub mod summary;
mod wp;

use crate::facts::FactData;
use crate::ir::{FuncId, Function, Module, SigData, SigId};
use crate::types::TypeInterner;
use crate::verify::{ErrClass, Invalidation, PassCtx, VerifyError, verify_function, verify_module};

pub use rangeopt::HOT_LOOP_NOTE;
pub use wp::{WholeProgram, WpStats, branch_weights, member_ids, optimize_whole_program, owner};

/// The ONE tunable table (target 2 + amendment 4): every heuristic
/// threshold the mid-end consults, in one place, adjusted by s44's
/// iteration loop with benchmark evidence — never scattered constants.
/// Budgets are ITERATION COUNTS, not wall clock (D4: reproducible).
#[derive(Clone, Debug)]
pub struct Thresholds {
    /// Callees at or under this WIR-instruction count always inline.
    pub inline_always: u32,
    /// Hard callee-size cap after bonuses.
    pub inline_max: u32,
    /// Size cap for a callee with exactly one call site in the module.
    pub inline_single_use: u32,
    /// Budget bonus per loop-depth level of the call site.
    pub inline_depth_bonus: u32,
    /// Budget bonus per constant or `read`-mode argument.
    pub inline_const_arg_bonus: u32,
    /// Fixed iteration budget for the simplify fixpoint loop.
    pub simplify_iters: u32,
    /// Whole-region stack-promotion cap, bytes (amendment 1).
    pub sink_max_bytes: u64,
    /// Coalesced-allocation cap, bytes (amendment 2).
    pub coalesce_max_bytes: u64,
    /// Loop-versioning: max instructions in a candidate loop.
    pub version_max_insts: u32,
    /// Loop-versioning: minimum remaining checks to pay for a guard.
    pub version_min_checks: u32,
    /// Whole-program (s43): target summed WIR size per codegen
    /// cluster — the cluster COUNT falls out of it, so this and
    /// `cluster_max` are the whole partition's only inputs besides the
    /// graph itself (D4: no core count, no wall clock).
    pub cluster_target_size: u32,
    /// Whole-program: hard cap on the cluster count.
    pub cluster_max: usize,
    /// Whole-program: largest callee eligible for cross-cluster
    /// import, in WIR instructions.
    pub import_max: u32,
    /// Whole-program: total imported WIR instructions per cluster.
    pub import_budget: u32,
    // ---- PGO (s45) -----------------------------------------------
    //
    // Three knobs, all in the ONE table with everything else, and all
    // **neutral by default**. That is an evidence-led setting, not
    // timidity, and the evidence is worth stating where the numbers
    // are:
    //
    // A profile record is keyed by the POST-mid-end content hash of
    // the body (D8/s43, locked). So any knob here that changes a body
    // destroys that body's own record: the body the profile described
    // no longer exists, and the block counts that would have become
    // LLVM branch weights go with it. Measured on the T1 suite at s45:
    // with `inline_hot_bonus = 64`, `word_count`'s hot 155-instruction
    // `count_words` inlined into `main` and BOTH of the program's
    // records went stale, so a profile that matched 2/2 before the
    // pass matched 0/2 after it and the `!prof` channel was silent on
    // exactly the code it was collected for. No kernel showed a
    // reproducible win to pay for that, and two read as losses outside
    // their floors.
    //
    // So at v1 PGO's live channel is the one that does NOT move the
    // bodies — measured branch weights through s41 — and these three
    // ship neutral. They are implemented, tested and tunable, because
    // the fix is a keying or edge-counter change rather than a
    // heuristic one, and the closeout should be able to revisit them
    // without rebuilding the machinery.
    /// The normalized `hot=` rank ([`summary::HOTNESS_SCALE`]) at or
    /// above which a callee counts as HOT. A rank, so it is
    /// workload-independent.
    pub hot_rank: u32,
    /// Extra callee-size budget, in WIR instructions, at a hot callee.
    /// **0 = neutral (default)**: the inliner takes the s42 decision
    /// verbatim whatever the profile says.
    pub inline_hot_bonus: u32,
    /// Budget REMOVED at a PROVEN-COLD callee (`hot=0`: a record
    /// exists and the body never ran). **0 = neutral (default).** A
    /// callee with no record is unknown, not cold, and this never
    /// applies to it.
    pub inline_cold_penalty: u32,
    /// Cluster-affinity multiplier for an edge whose callee is hot —
    /// the "hot caller/callee pairs weighted into the same cluster"
    /// knob. **1 = neutral (default).**
    pub cluster_hot_boost: u32,
}

impl Default for Thresholds {
    fn default() -> Thresholds {
        Thresholds {
            inline_always: 16,
            inline_max: 64,
            inline_single_use: 96,
            inline_depth_bonus: 16,
            inline_const_arg_bonus: 8,
            simplify_iters: 4,
            sink_max_bytes: 1024,
            coalesce_max_bytes: 4096,
            version_max_insts: 48,
            version_min_checks: 1,
            cluster_target_size: 512,
            cluster_max: 8,
            import_max: 96,
            import_budget: 512,
            hot_rank: 100,
            inline_hot_bonus: 0,
            inline_cold_penalty: 0,
            cluster_hot_boost: 1,
        }
    }
}

/// Mid-end options.
#[derive(Clone, Debug)]
pub struct Options {
    pub thresholds: Thresholds,
    /// Verify (and fact-audit) after every pass, not just at the end.
    pub verify_each: bool,
    /// The profile to consume, if the build was given one (s45,
    /// `wolf build --release --profile=<f.wprof>`). **`None` is the
    /// default and the normal case** (D4: PGO is integrated, optional
    /// and NEVER required — a build without a profile is a build, not
    /// a degraded build). A profile that matches nothing produces a
    /// byte-identical result to `None`.
    pub profile: Option<crate::profile::Profile>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            thresholds: Thresholds::default(),
            verify_each: cfg!(debug_assertions)
                || std::env::var("WOLF_VERIFY_EACH_PASS").as_deref() == Ok("1"),
            profile: None,
        }
    }
}

/// Per-run counters — the evidence surface (`WOLF_MIDEND_STATS=1`
/// prints them; tests gate on them; s44 reads them per commit).
#[derive(Clone, Debug, Default)]
pub struct OptStats {
    pub funcs: usize,
    pub insts_before: usize,
    pub insts_after: usize,
    /// Rows in the declarative peephole/GVN rule table (amendment 4).
    pub rule_table_len: usize,
    pub folds: usize,
    pub rule_hits: usize,
    pub gvn_hits: usize,
    pub branch_folds: usize,
    pub trivial_params: usize,
    pub dce_removed: usize,
    pub inlined_calls: usize,
    /// Of those, call sites whose callee lives in ANOTHER source
    /// module (s43: the whole-program win, counted).
    pub cross_module_inlined: usize,
    /// Of those, callees imported across a cluster boundary by the
    /// summary-driven import decision (thin-LTO import semantics).
    pub cross_cluster_inlined: usize,
    pub loads_eliminated: usize,
    pub stores_forwarded: usize,
    pub dead_stores: usize,
    /// Checked ops rewritten to wrap forms (range-proven trap-free).
    pub checks_rewritten: usize,
    /// Overflow checks observed inside natural loops (the X3 metric).
    pub loop_checks_seen: usize,
    /// Of those, statically eliminated (directly or via versioning).
    pub loop_checks_eliminated: usize,
    pub loops_versioned: usize,
    /// Container bounds guards observed (s75): a `br` whose false arm
    /// is a bare `trap.bounds` block. Counted wherever they appear,
    /// not only in loops — a check outside a loop is still a check.
    pub bounds_checks_seen: usize,
    /// Of those, proven away (the dominating comparison decided them).
    pub bounds_checks_eliminated: usize,
    /// Whole regions promoted to the frame (amendment 1).
    pub regions_promoted: usize,
    /// `region.alloc`s replaced by frame offsets under promotion.
    pub allocs_promoted: usize,
    /// `region.alloc`s fused into a preceding one (amendment 2).
    pub allocs_coalesced: usize,
    pub loads_hoisted: usize,
    pub invariants_hoisted: usize,
    /// Uncalled, unexported function bodies dropped after inlining.
    pub funcs_removed: usize,
}

impl OptStats {
    /// The X3 elimination rate over hot loops (the ≥80% gate's metric).
    pub fn elimination_rate(&self) -> Option<f64> {
        if self.loop_checks_seen == 0 {
            return None;
        }
        Some(self.loop_checks_eliminated as f64 / self.loop_checks_seen as f64)
    }

    /// The bounds-check elimination rate (s75). Bounds checks STAY:
    /// this measures how many the analysis could discharge, never how
    /// many were deleted for being inconvenient.
    pub fn bounds_elimination_rate(&self) -> Option<f64> {
        if self.bounds_checks_seen == 0 {
            return None;
        }
        Some(self.bounds_checks_eliminated as f64 / self.bounds_checks_seen as f64)
    }
}

impl std::fmt::Display for OptStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "midend: {} func(s), {} -> {} inst(s)",
            self.funcs, self.insts_before, self.insts_after
        )?;
        writeln!(
            f,
            "  simplify: {} fold(s), {} rule hit(s) (table of {}), {} gvn, {} br-fold, {} trivial param(s), {} dce",
            self.folds,
            self.rule_hits,
            self.rule_table_len,
            self.gvn_hits,
            self.branch_folds,
            self.trivial_params,
            self.dce_removed
        )?;
        writeln!(
            f,
            "  inline: {} call site(s) ({} cross-module, {} cross-cluster)",
            self.inlined_calls, self.cross_module_inlined, self.cross_cluster_inlined
        )?;
        writeln!(
            f,
            "  memopt: {} load(s) eliminated, {} store(s) forwarded, {} dead store(s)",
            self.loads_eliminated, self.stores_forwarded, self.dead_stores
        )?;
        writeln!(
            f,
            "  ranges: {} check(s) -> wrap; hot loops {}/{} eliminated{}, {} loop(s) versioned",
            self.checks_rewritten,
            self.loop_checks_eliminated,
            self.loop_checks_seen,
            match self.elimination_rate() {
                Some(r) => format!(" ({:.1}%)", r * 100.0),
                None => String::new(),
            },
            self.loops_versioned
        )?;
        writeln!(
            f,
            "  bounds: {}/{} guard(s) proven away{}",
            self.bounds_checks_eliminated,
            self.bounds_checks_seen,
            match self.bounds_elimination_rate() {
                Some(r) => format!(" ({:.1}%)", r * 100.0),
                None => String::new(),
            }
        )?;
        writeln!(
            f,
            "  regions: {} promoted ({} alloc(s)), {} alloc(s) coalesced",
            self.regions_promoted, self.allocs_promoted, self.allocs_coalesced
        )?;
        write!(
            f,
            "  licm: {} load(s), {} invariant op(s) hoisted; {} dead func(s) dropped",
            self.loads_hoisted, self.invariants_hoisted, self.funcs_removed
        )
    }
}

/// The regions of `f` whose effect token names EVERY writer of their
/// storage — the mid-end's own soundness predicate, published so the
/// BACKENDS can rest on exactly the same theorem the passes do (s83).
///
/// The LLVM tier's call-site `!noalias` fact is the first consumer:
/// a call that does not take an exhaustive region's token provably
/// cannot touch its memory, which is the claim s78 declined for want
/// of a theorem. Keeping one definition is the point — an emitter that
/// re-derived this would be free to drift from the passes, and an
/// unenforced asymmetry in the fact rig is exactly how s80's miscompile
/// happened.
pub fn exhaustive_regions(m: &Module, f: &crate::ir::Function) -> std::collections::HashSet<u32> {
    let view = ModView {
        types: &m.types,
        sigs: &m.sigs,
    };
    memopt::exhaustive_regions(f, &view)
}

/// Read-only module context passes see beside the function they own.
pub(crate) struct ModView<'a> {
    pub types: &'a TypeInterner,
    pub sigs: &'a crate::entity::PrimaryMap<SigId, SigData>,
}

/// Run one function pass under the manager: snapshot facts, run,
/// audit invalidations, compact, optionally verify, swap. Returns
/// whether the pass changed anything.
pub(crate) fn run_managed<P>(
    m: &mut Module,
    func: FuncId,
    pass_name: &str,
    verify_each: bool,
    pass: P,
) -> Result<bool, VerifyError>
where
    P: FnOnce(&mut Function, &ModView, &mut PassCtx) -> bool,
{
    let before: Vec<(crate::facts::FactId, FactData)> = m.funcs[func]
        .facts
        .iter()
        .map(|(id, fd)| (id, *fd))
        .collect();
    let mut ctx = PassCtx::default();
    let changed = {
        let Module {
            ref types,
            ref sigs,
            ref mut funcs,
            ..
        } = *m;
        let view = ModView { types, sigs };
        pass(&mut funcs[func], &view, &mut ctx)
    };
    if !changed {
        return Ok(false);
    }
    let f = &m.funcs[func];
    // (1) Pre-compact audit: a pre-existing fact changed or removed by
    // the pass itself needs an explicit invalidation (D2).
    for &(id, ref old) in &before {
        let intact = f.facts.get(id).is_some_and(|now| now == old);
        if intact {
            continue;
        }
        if !ctx.invalidations().iter().any(|&(fid, _)| fid == id) {
            return Err(VerifyError {
                class: ErrClass::DroppedFact,
                func: f.name.clone(),
                msg: format!(
                    "pass `{pass_name}` dropped or changed fact {id} ({}) without a justified \
                     invalidation — facts are semantics, not metadata (D2)",
                    old.kind.keyword()
                ),
                dump: crate::print::print_function(m, f),
            });
        }
    }
    // (2) Compact: batch invariant repair. Facts whose operands died
    // are dropped here — the legitimate value-deleted loss.
    let out = compact::compact(f);
    // (3) Invalidation honesty: `ValueDeleted` claims must be true —
    // the old fact's subject must NOT have survived compaction.
    for &(id, why) in ctx.invalidations() {
        if why != Invalidation::ValueDeleted {
            continue;
        }
        let Some((_, old)) = before.iter().find(|&&(fid, _)| fid == id) else {
            continue; // invalidating a fact minted by this same pass
        };
        if out.vmap.contains_key(&old.kind.subject()) {
            return Err(VerifyError {
                class: ErrClass::DroppedFact,
                func: f.name.clone(),
                msg: format!(
                    "pass `{pass_name}` invalidated fact {id} as value-deleted, but its subject \
                     value still exists"
                ),
                dump: crate::print::print_function(m, f),
            });
        }
    }
    if verify_each {
        verify_function(m, &out.func)?;
    }
    m.funcs[func] = out.func;
    Ok(true)
}

/// The mid-end pipeline: bottom-up over the call graph (CGSCC order),
/// each function fully optimized before its callers consider inlining
/// it (amendment 4's callee-simplify-before-inline, by construction).
pub fn optimize_module(m: &mut Module, opts: &Options) -> Result<OptStats, VerifyError> {
    let mut stats = OptStats {
        rule_table_len: crate::peephole_rules::RULES.len(),
        funcs: m.funcs.len(),
        insts_before: count_insts(m),
        ..OptStats::default()
    };
    let th = &opts.thresholds;
    let ve = opts.verify_each;
    let order = inline::bottom_up_order(m);
    for fid in order {
        optimize_one(m, fid, ve, th, &mut stats, inline::Scope::all())?;
    }
    // Dead-function elimination: after inlining, callee bodies with no
    // remaining reference are ballast the backend would still compile.
    // Roots: exported functions and `@main` (the driver's entry shim
    // imports it after the mid-end). References: `Aux::Callee` edges —
    // both `call` and `func.addr` (task entries stay live). A module
    // with no root at all (a library shape) skips DFE wholesale.
    dead_function_elim(m, &mut stats);
    stats.insts_after = count_insts(m);
    // The pipeline's own gate: whatever the per-pass setting, the
    // optimized module verifies before any backend sees it.
    verify_module(m)?;
    Ok(stats)
}

/// One function's full pass sequence, in the fixed order — the unit
/// both [`optimize_module`] and the whole-program phase drive. The
/// only difference between them is the inliner's [`inline::Scope`].
///
/// Callee-simplify-before-inline holds by construction: the CGSCC
/// order means a function was already simplified in its own visit when
/// a caller reaches here, and the first visit simplifies before
/// anything else looks at it.
fn optimize_one(
    m: &mut Module,
    fid: FuncId,
    ve: bool,
    th: &Thresholds,
    stats: &mut OptStats,
    scope: inline::Scope<'_>,
) -> Result<(), VerifyError> {
    simplify::run(m, fid, ve, th, stats)?;
    inline::run(m, fid, ve, th, stats, scope)?;
    simplify::run(m, fid, ve, th, stats)?;
    memopt::run(m, fid, ve, stats)?;
    rangeopt::run(m, fid, ve, th, stats)?;
    sink::run(m, fid, ve, th, stats)?;
    coalesce::run(m, fid, ve, th, stats)?;
    licm::run(m, fid, ve, stats)?;
    simplify::run(m, fid, ve, th, stats)?;
    Ok(())
}

fn dead_function_elim(m: &mut Module, stats: &mut OptStats) {
    use std::collections::{HashMap, HashSet};
    let by_name: HashMap<String, FuncId> =
        m.funcs.iter().map(|(id, f)| (f.name.clone(), id)).collect();
    let mut live: HashSet<FuncId> = HashSet::new();
    let mut work: Vec<FuncId> = m
        .funcs
        .iter()
        .filter(|(_, f)| f.export || f.name == "main")
        .map(|(id, _)| id)
        .collect();
    if work.is_empty() {
        return; // no root: a library-shaped module — keep everything
    }
    while let Some(fid) = work.pop() {
        if !live.insert(fid) {
            continue;
        }
        let f = &m.funcs[fid];
        for &b in &f.layout {
            for &inst in &f.blocks[b].insts {
                if let crate::ir::Aux::Callee(ef) = f.insts[inst].aux
                    && let Some(&cid) = by_name.get(&f.ext_funcs[ef].name)
                {
                    work.push(cid);
                }
            }
        }
    }
    if live.len() == m.funcs.len() {
        return;
    }
    // FuncIds are positional: rebuild the function table with only the
    // live ones, in the original order (nothing else in a Module keys
    // on FuncId — callees resolve by NAME).
    let mut funcs = crate::entity::PrimaryMap::new();
    let old = std::mem::replace(&mut m.funcs, crate::entity::PrimaryMap::new());
    for (id, f) in old.iter() {
        if live.contains(&id) {
            funcs.push(f.clone());
        } else {
            stats.funcs_removed += 1;
        }
    }
    m.funcs = funcs;
}

/// Run ONE pass by name — the test/debug surface. Per-pass snapshots
/// are reviewed artifacts (s01 convention); the pipeline itself uses
/// the fixed order in [`optimize_module`].
pub fn run_named_pass(
    m: &mut Module,
    func: FuncId,
    name: &str,
    opts: &Options,
    stats: &mut OptStats,
) -> Result<bool, VerifyError> {
    let th = &opts.thresholds;
    let ve = opts.verify_each;
    match name {
        "simplify" => simplify::run(m, func, ve, th, stats),
        "inline" => inline::run(m, func, ve, th, stats, inline::Scope::all()),
        "memopt" => memopt::run(m, func, ve, stats),
        "rangeopt" => rangeopt::run(m, func, ve, th, stats),
        "sink" => sink::run(m, func, ve, th, stats),
        "coalesce" => coalesce::run(m, func, ve, th, stats),
        "licm" => licm::run(m, func, ve, stats),
        other => panic!("unknown mid-end pass `{other}`"),
    }
}

fn count_insts(m: &Module) -> usize {
    m.funcs
        .values()
        .map(|f| {
            f.layout
                .iter()
                .map(|&b| f.blocks[b].insts.len())
                .sum::<usize>()
        })
        .sum()
}

/// Facts referencing `v` (subject or operand) — passes deleting a
/// definition consult this to record honest invalidations.
pub(crate) fn facts_on(f: &Function, v: crate::ir::Value) -> Vec<crate::facts::FactId> {
    use crate::facts::{DerefSize, FactKind, Just};
    f.facts
        .iter()
        .filter(|(_, fd)| {
            let mut vals = vec![fd.kind.subject()];
            if let FactKind::Noalias(_, b) = fd.kind {
                vals.push(b);
            }
            if let FactKind::Deref(_, DerefSize::Scaled { count, .. }) = fd.kind {
                vals.push(count);
            }
            if let Just::Op(o) = fd.just {
                vals.push(o);
            }
            vals.contains(&v)
        })
        .map(|(id, _)| id)
        .collect()
}
