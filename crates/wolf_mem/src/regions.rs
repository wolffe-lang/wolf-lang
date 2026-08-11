//! Region inference (s19 Targets 1–3, 5): allocation-site region
//! attribution with Cyclone-rule signature defaults, and the
//! escape-to-stack promotion facts c05/s26 consume.
//!
//! # The model
//!
//! Every heap allocation (struct literal, constructor call, non-`Copy`
//! call result — `[mem.model.alloc]`; there is no `new` keyword) is
//! attributed to the **ambient region** at its site: the innermost
//! enclosing `in r { }` / `region name { }` scope, else the function's
//! implicit caller-region parameter (`[mem.region.create.3]`, D12).
//! Inference is unification-based over the function's region
//! variables and **intraprocedural only** — call sites see the
//! callee's *default* Cyclone scheme (every heap-reaching parameter
//! fresh-and-generalized, results and the allocation effect in
//! `ρ_caller`), never its body.
//!
//! The variable lattice:
//!
//! - `Scope`/`Value` regions (syntactic introductions) are **rigid**:
//!   they unify only with themselves. Two that never unify denote
//!   provably distinct regions — the crate invariant s20's
//!   disjointness argument stands on (`[mem.region.create.2]`).
//! - `Caller` and `Static` are rigid classes; `Static` outlives
//!   everything (returning static data is always legal).
//! - `Param` regions are **flexible**: a parameter's data may turn
//!   out to live in the caller's region (the default effect covers
//!   it), but a *direct* demand that two parameters share a region is
//!   the one thing Cyclone's data says needs an annotation — no such
//!   surface exists yet, so it reports E1004 with a restructure hint.
//!
//! Values keep the region of their allocation through moves (the
//! conservative uniform model): embedding a value into an aggregate
//! demands co-location, which is where unification happens. `copy`
//! breaks the linkage — a copy is a fresh allocation in the ambient
//! region — and is therefore the diagnostic fix ladder's first rung.
//!
//! # Promotion (Target 5)
//!
//! A `Scope`/`Value` region freed in this frame (never moved away)
//! whose data never escapes is **promoted**: the create/free pair can
//! disappear and its arena become a stack allocation. Independently,
//! a literal allocation site whose value provably stays in-frame
//! (never returned, never passed `take`/`mut`, never stored to module
//! state, never caught by a region close) gets a per-site stack fact.
//! Both are facts only — no codegen until c05/s26 — and must be
//! semantically invisible (`[mem.region.promote.1]`).

use wolf_span::Span;

use crate::cfg::{AllocSite, Region, RegionId, RegionKind, SiteKind};

/// What a region variable resolved to (union-find root binding).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Binding {
    Caller,
    Static,
    /// A rigid local region (`Scope`/`Value` introduction).
    Local(RegionId),
}

/// One unification's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unify {
    Ok,
    /// Two *parameter* regions were demanded equal — the
    /// equality-annotation case (rare by Cyclone's measurement); no
    /// surface exists to state it, so the demand is an E1004.
    ParamsMerged,
    /// Two distinct rigid regions were demanded equal.
    Conflict(Binding, Binding),
}

/// The per-function region variable table: union-find plus rigid
/// bindings.
#[derive(Debug, Default)]
pub(crate) struct RegionTable {
    pub regions: Vec<Region>,
    parent: Vec<u32>,
    binding: Vec<Option<Binding>>,
}

impl RegionTable {
    pub fn new_region(&mut self, region: Region) -> RegionId {
        let id = RegionId(self.regions.len() as u32);
        let binding = match region.kind {
            RegionKind::Caller => Some(Binding::Caller),
            RegionKind::Static => Some(Binding::Static),
            RegionKind::Scope | RegionKind::Value => Some(Binding::Local(id)),
            RegionKind::Param => None,
        };
        self.regions.push(region);
        self.parent.push(id.0);
        self.binding.push(binding);
        id
    }

    fn find(&mut self, r: RegionId) -> u32 {
        let mut root = r.0;
        while self.parent[root as usize] != root {
            root = self.parent[root as usize];
        }
        // Path compression.
        let mut cur = r.0;
        while self.parent[cur as usize] != root {
            let next = self.parent[cur as usize];
            self.parent[cur as usize] = root;
            cur = next;
        }
        root
    }

    pub fn binding(&mut self, r: RegionId) -> Option<Binding> {
        let root = self.find(r);
        self.binding[root as usize]
    }

    pub fn same(&mut self, a: RegionId, b: RegionId) -> bool {
        self.find(a) == self.find(b)
    }

    /// Demand that `a` and `b` are one region. Never merges two
    /// distinct rigid classes; the caller turns the outcome into the
    /// right diagnostic.
    pub fn unify(&mut self, a: RegionId, b: RegionId) -> Unify {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return Unify::Ok;
        }
        match (self.binding[ra as usize], self.binding[rb as usize]) {
            (Some(x), Some(y)) if x == y => {
                self.parent[rb as usize] = ra;
                Unify::Ok
            }
            (Some(x), Some(y)) => Unify::Conflict(x, y),
            (Some(_), None) => {
                self.parent[rb as usize] = ra;
                Unify::Ok
            }
            (None, Some(_)) => {
                self.parent[ra as usize] = rb;
                Unify::Ok
            }
            (None, None) => {
                self.parent[rb as usize] = ra;
                Unify::ParamsMerged
            }
        }
    }
}

/// The per-function inference record: the reviewable dump surface and
/// the promotion facts (s26's input).
#[derive(Debug)]
pub struct RegionSummary {
    pub name: String,
    pub regions: Vec<Region>,
    /// Per region: the rendered resolution of its class (`caller`,
    /// `static`, `region tmp`, or `own` for a still-generalized
    /// parameter region).
    pub resolved: Vec<String>,
    pub sites: Vec<SiteSummary>,
    /// Regions whose create/free pair is frame-local and clean: the
    /// promotion fact.
    pub promoted: Vec<RegionId>,
    /// Promoted regions with **no allocation site at all** (s68,
    /// W1001): pure ceremony — nothing is ever built in them, and
    /// their handle never leaves the frame (promotion's own proof).
    pub never_allocates: Vec<RegionId>,
    /// Whether any placement demand could not be discharged (the
    /// function has E1004/E1010 diagnostics).
    pub conflicted: bool,
}

#[derive(Debug)]
pub struct SiteSummary {
    pub span: Span,
    pub ty: String,
    pub region: RegionId,
    pub kind: SiteKind,
    /// Why the value may leave the frame, when it may.
    pub escape: Option<&'static str>,
    /// The per-site stack-promotion fact: a literal allocation that
    /// provably never escapes.
    pub stacked: bool,
}

impl RegionSummary {
    /// Deterministic textual form (the `--dump=regions` and snapshot
    /// surface). Byte-offset spans, walk-order ids, no internal
    /// variable names — regions render by their introduction.
    pub fn render(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(out, "regions fn {}", self.name);
        for (i, r) in self.regions.iter().enumerate() {
            let strat = match &r.strategy {
                crate::cfg::Strategy::Arena => String::new(),
                crate::cfg::Strategy::Rc => " rc".to_string(),
                crate::cfg::Strategy::Pool(t) => format!(" pool({t})"),
            };
            let promoted = if self.promoted.contains(&RegionId(i as u32)) {
                " [promoted -> stack; create/free elided]"
            } else {
                ""
            };
            let line = match r.kind {
                RegionKind::Caller => "ambient caller (implicit parameter)".to_string(),
                RegionKind::Static => "ambient static (module state)".to_string(),
                RegionKind::Param => {
                    format!("param {} -> {}", r.name, self.resolved[i])
                }
                RegionKind::Scope => format!(
                    "region {}{strat} @{}..{} (scope){promoted}",
                    r.name, r.span.lo, r.span.hi
                ),
                RegionKind::Value => format!(
                    "region {}{strat} @{}..{} (value){promoted}",
                    r.name, r.span.lo, r.span.hi
                ),
            };
            let _ = writeln!(out, "  {line}");
        }
        for s in &self.sites {
            let region = &self.resolved[s.region.0 as usize];
            let mark = match (s.stacked, s.escape) {
                (true, _) => " [stack]".to_string(),
                (false, Some(why)) => format!(" (escapes: {why})"),
                (false, None) => String::new(),
            };
            let verb = match s.kind {
                SiteKind::Lit => "alloc",
                SiteKind::CallResult => "call-alloc",
                SiteKind::Param => "param-data",
            };
            let _ = writeln!(
                out,
                "  {verb} {} @{}..{} -> {}{}",
                s.ty, s.span.lo, s.span.hi, region, mark
            );
        }
        out
    }
}

/// Build the summary from the finished tables (called by the lowerer
/// once a body's walk completes).
pub(crate) fn summarize(
    name: &str,
    mut rt: RegionTable,
    sites: &[AllocSite],
    site_escape: &[Option<&'static str>],
    promoted: &[RegionId],
    conflicted: bool,
) -> RegionSummary {
    let n = rt.regions.len();
    let mut resolved = Vec::with_capacity(n);
    for i in 0..n {
        let id = RegionId(i as u32);
        let text = match rt.binding(id) {
            Some(Binding::Caller) => "caller".to_string(),
            Some(Binding::Static) => "static".to_string(),
            Some(Binding::Local(r)) => {
                let r = &rt.regions[r.0 as usize];
                format!("region {}", r.name)
            }
            None => "own (generalized)".to_string(),
        };
        resolved.push(text);
    }
    let site_summaries: Vec<SiteSummary> = sites
        .iter()
        .enumerate()
        .map(|(i, s)| SiteSummary {
            span: s.span,
            ty: s.ty.clone(),
            region: s.region,
            kind: s.kind,
            escape: site_escape[i],
            stacked: s.kind == SiteKind::Lit
                && site_escape[i].is_none()
                && !promoted.iter().any(|p| rt.same(*p, s.region)),
        })
        .collect();
    // W1001's fact: a promoted region no site ever resolved into.
    let never_allocates: Vec<RegionId> = promoted
        .iter()
        .copied()
        .filter(|&p| !sites.iter().any(|s| rt.same(s.region, p)))
        .collect();
    RegionSummary {
        name: name.to_string(),
        regions: rt.regions,
        resolved,
        sites: site_summaries,
        promoted: promoted.to_vec(),
        never_allocates,
        conflicted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::Strategy;

    fn region(kind: RegionKind) -> Region {
        let mut sm = wolf_span::SourceMap::new();
        let file = sm.intern(std::path::Path::new("test.lu"));
        Region {
            name: "r".to_string(),
            kind,
            strategy: Strategy::Arena,
            span: Span::new(file, 0, 0),
        }
    }

    #[test]
    fn params_absorb_rigids_but_rigids_never_merge() {
        let mut rt = RegionTable::default();
        let caller = rt.new_region(region(RegionKind::Caller));
        let p1 = rt.new_region(region(RegionKind::Param));
        let p2 = rt.new_region(region(RegionKind::Param));
        let scope = rt.new_region(region(RegionKind::Scope));

        assert_eq!(rt.unify(p1, caller), Unify::Ok);
        assert_eq!(rt.binding(p1), Some(Binding::Caller));
        // p2 joins p1 (now caller-bound): fine — the default effect.
        assert_eq!(rt.unify(p2, p1), Unify::Ok);
        // Rigid–rigid is a conflict, both ways.
        assert_eq!(
            rt.unify(caller, scope),
            Unify::Conflict(Binding::Caller, Binding::Local(scope))
        );
        assert_eq!(
            rt.unify(scope, caller),
            Unify::Conflict(Binding::Local(scope), Binding::Caller)
        );
    }

    #[test]
    fn direct_param_param_merge_is_the_annotation_case() {
        let mut rt = RegionTable::default();
        let p1 = rt.new_region(region(RegionKind::Param));
        let p2 = rt.new_region(region(RegionKind::Param));
        assert_eq!(rt.unify(p1, p2), Unify::ParamsMerged);
        // Idempotent afterwards.
        assert_eq!(rt.unify(p1, p2), Unify::Ok);
    }

    #[test]
    fn scope_regions_stay_distinct() {
        let mut rt = RegionTable::default();
        let a = rt.new_region(region(RegionKind::Scope));
        let b = rt.new_region(region(RegionKind::Scope));
        assert!(matches!(rt.unify(a, b), Unify::Conflict(..)));
        assert!(!rt.same(a, b));
    }
}
