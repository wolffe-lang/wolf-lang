//! Fact export stubs (s18 Target 7, s19 Target 5): per-function
//! summary records for the optimizer pipeline. Dead weight until s26
//! lowers them — the schema exists now and is snapshot-tested.
//!
//! - `mut` parameter → `noalias` + `dereferenceable` for the call
//!   (§7/O1);
//! - `read` parameter → immutable-for-the-call, Frozen (§7/O2 — the
//!   SB "holy grail");
//! - whole-value move sites → memcpy-and-forget (§7/O7);
//! - allocation sites → their inferred regions (s26 alias scopes);
//! - promoted regions / non-escaping allocations → stack placement
//!   (c05; `[mem.region.promote.1]` — must stay unobservable);
//! - s21 Tier-2 plan facts: recorded RC dups (genuine fan-out) and
//!   drops (scope exit, LIFO with defers, migratable to last use —
//!   Perceus's drop-at-last-use is the c05 refinement), `shared`
//!   cells with the **static atomicity bit** (non-atomic unless a
//!   freeze/transfer point reaches the cell — thread-exclusivity is
//!   structural, never guessed), and generational handle checks as
//!   checked-op facts (s42 hoists/dedups them within provably-unfreed
//!   windows).

use wolf_span::Span;

use crate::cfg::{Cfg, Stmt};
use crate::regions::RegionSummary;

#[derive(Debug, Clone)]
pub struct FnFacts {
    pub name: String,
    /// `mut` parameters: exclusive for the call ⇒ noalias +
    /// dereferenceable.
    pub mut_params: Vec<String>,
    /// `read` parameters: immutable for the call ⇒ Frozen.
    pub read_params: Vec<String>,
    /// `take` parameters: owned on entry.
    pub take_params: Vec<String>,
    /// Whole-value move sites (place, span): memcpy-and-forget.
    pub move_sites: Vec<(String, Span)>,
    /// Allocations attributed to regions: (type, region, span).
    pub alloc_sites: Vec<(String, String, Span)>,
    /// Frame-local regions: the create/free pair disappears; the
    /// arena becomes one stack allocation.
    pub promoted_regions: Vec<(String, Span)>,
    /// Individually stackable allocations: (type, span).
    pub stack_allocs: Vec<(String, Span)>,
    /// s21 — recorded RC increments (place, span): explicit `clone()`
    /// fan-out; everything else is statically elided (single owner).
    pub dups: Vec<(String, Span)>,
    /// s21 — recorded RC decrements (place, decl span): scope exit,
    /// LIFO with defers ([mem.shared.drop.1]); payloads carry no
    /// destructors yet, so every one is migratable to last use.
    pub drops: Vec<(String, Span)>,
    /// s21 — `shared`/`weak` cell allocations with the static
    /// atomicity bit (true = the atomic form is required: the cell is
    /// reachable from a freeze/transfer point).
    pub cells: Vec<(String, Span, bool)>,
    /// s21 — generational checks at pool slot accesses.
    pub handle_checks: Vec<(String, Span)>,
}

pub fn collect(
    cfg: &Cfg,
    regions: &RegionSummary,
    cells: &[(crate::cfg::SiteId, bool)],
) -> FnFacts {
    let mut facts = FnFacts {
        name: cfg.name.clone(),
        mut_params: Vec::new(),
        read_params: Vec::new(),
        take_params: Vec::new(),
        move_sites: Vec::new(),
        alloc_sites: Vec::new(),
        promoted_regions: Vec::new(),
        stack_allocs: Vec::new(),
        dups: Vec::new(),
        drops: Vec::new(),
        cells: Vec::new(),
        handle_checks: Vec::new(),
    };
    for &(site, atomic) in cells {
        let s = &cfg.sites[site.0 as usize];
        facts.cells.push((s.ty.clone(), s.span, atomic));
    }
    for s in &regions.sites {
        // Parameter pseudo-sites are the *caller's* allocations; the
        // param mode lines above already carry their facts.
        if s.kind == crate::cfg::SiteKind::Param {
            continue;
        }
        facts.alloc_sites.push((
            s.ty.clone(),
            regions.resolved[s.region.0 as usize].clone(),
            s.span,
        ));
        if s.stacked {
            facts.stack_allocs.push((s.ty.clone(), s.span));
        }
    }
    for &rid in &regions.promoted {
        let r = &regions.regions[rid.0 as usize];
        facts.promoted_regions.push((r.name.clone(), r.span));
    }
    for local in &cfg.locals {
        match local.param_mode {
            Some(None) => facts.read_params.push(local.name.clone()),
            Some(Some(wolf_ast::ParamMode::Mut)) => facts.mut_params.push(local.name.clone()),
            Some(Some(wolf_ast::ParamMode::Take)) => facts.take_params.push(local.name.clone()),
            None => {}
        }
    }
    for block in &cfg.blocks {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Move { place, span } if cfg.places.get(*place).proj.is_empty() => {
                    facts.move_sites.push((cfg.show_place(*place), *span));
                }
                Stmt::Dup { place, span } => {
                    facts.dups.push((cfg.show_place(*place), *span));
                }
                Stmt::Drop { place, span } => {
                    facts.drops.push((cfg.show_place(*place), *span));
                }
                Stmt::HandleCheck { place, span } => {
                    facts.handle_checks.push((cfg.show_place(*place), *span));
                }
                _ => {}
            }
        }
    }
    facts
}

impl FnFacts {
    /// Deterministic textual form (the snapshot surface, and s26's
    /// starting contract).
    pub fn render(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(out, "facts fn {}", self.name);
        for p in &self.mut_params {
            let _ = writeln!(out, "  mut {p} -> noalias, dereferenceable");
        }
        for p in &self.read_params {
            let _ = writeln!(out, "  read {p} -> immutable-for-call (frozen)");
        }
        for p in &self.take_params {
            let _ = writeln!(out, "  take {p} -> owned");
        }
        for (place, span) in &self.move_sites {
            let _ = writeln!(
                out,
                "  move {place} @{}..{} -> memcpy-and-forget",
                span.lo, span.hi
            );
        }
        for (ty, region, span) in &self.alloc_sites {
            let _ = writeln!(out, "  alloc {ty} @{}..{} -> {region}", span.lo, span.hi);
        }
        for (name, span) in &self.promoted_regions {
            let _ = writeln!(
                out,
                "  promote region {name} @{}..{} -> stack (create/free elided)",
                span.lo, span.hi
            );
        }
        for (ty, span) in &self.stack_allocs {
            let _ = writeln!(
                out,
                "  stack-alloc {ty} @{}..{} (never escapes)",
                span.lo, span.hi
            );
        }
        for (place, span) in &self.dups {
            let _ = writeln!(
                out,
                "  dup {place} @{}..{} -> rc+1 (fan-out)",
                span.lo, span.hi
            );
        }
        for (place, span) in &self.drops {
            let _ = writeln!(
                out,
                "  drop {place} @{}..{} -> scope exit, LIFO with defers; migratable to last use",
                span.lo, span.hi
            );
        }
        for (ty, span, atomic) in &self.cells {
            let bit = if *atomic {
                "atomic (reaches a freeze/transfer point)"
            } else {
                "non-atomic (thread-exclusive by region structure)"
            };
            let _ = writeln!(out, "  rc-cell {ty} @{}..{} -> {bit}", span.lo, span.hi);
        }
        for (place, span) in &self.handle_checks {
            let _ = writeln!(
                out,
                "  handle-check {place} @{}..{} -> generation check (hoistable in unfreed window)",
                span.lo, span.hi
            );
        }
        out
    }
}
