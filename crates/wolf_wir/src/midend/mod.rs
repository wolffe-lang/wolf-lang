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
mod coalesce;
mod compact;
mod inline;
mod licm;
mod memopt;
mod rangeopt;
mod simplify;
mod sink;

use crate::facts::FactData;
use crate::ir::{FuncId, Function, Module, SigData, SigId};
use crate::types::TypeInterner;
use crate::verify::{ErrClass, Invalidation, PassCtx, VerifyError, verify_function, verify_module};

pub use rangeopt::HOT_LOOP_NOTE;

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
        }
    }
}

/// Mid-end options.
#[derive(Clone, Debug)]
pub struct Options {
    pub thresholds: Thresholds,
    /// Verify (and fact-audit) after every pass, not just at the end.
    pub verify_each: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            thresholds: Thresholds::default(),
            verify_each: cfg!(debug_assertions)
                || std::env::var("WOLF_VERIFY_EACH_PASS").as_deref() == Ok("1"),
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
        writeln!(f, "  inline: {} call site(s)", self.inlined_calls)?;
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
        // Callee-simplify-before-inline: this function was already
        // simplified in its own visit when a caller reaches here; the
        // first visit simplifies before anything else looks at it.
        simplify::run(m, fid, ve, th, &mut stats)?;
        inline::run(m, fid, ve, th, &mut stats)?;
        simplify::run(m, fid, ve, th, &mut stats)?;
        memopt::run(m, fid, ve, &mut stats)?;
        rangeopt::run(m, fid, ve, th, &mut stats)?;
        sink::run(m, fid, ve, th, &mut stats)?;
        coalesce::run(m, fid, ve, th, &mut stats)?;
        licm::run(m, fid, ve, &mut stats)?;
        simplify::run(m, fid, ve, th, &mut stats)?;
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
        "inline" => inline::run(m, func, ve, th, stats),
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
