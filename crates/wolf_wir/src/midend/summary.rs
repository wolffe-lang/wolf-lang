//! Module summaries (s43 target 4) — the thin-LTO spine.
//!
//! A summary is everything the whole-program phase needs to reason
//! about a function WITHOUT loading its body: size, call edges, the
//! fact digest, and the body's content hash (D8). Summaries are cheap
//! to compute (one linear scan per function), cheap to hash, and
//! **frozen**: c12's resident compiler and the tooling track key on
//! this format, so the schema below changes only with a version bump.
//!
//! The format is documented for consumers in
//! `wolf_wir/summary-format.md` (as `text.md` documents the WIR
//! textual format); this module is its implementation.
//!
//! # The frozen schema (v2)
//!
//! The canonical serialization is line-oriented text — one function per
//! line, deterministic field order, no floats, no wall-clock, no
//! thread-order inputs. It is the hash input and the `--codegen-report`
//! surface at once (one format, one authority):
//!
//! ```text
//! summary-format 2
//! fn <name> home=<module> size=<insts> blocks=<n> flags=<EMTSRKA|-> \
//!    hash=<sha256> facts=<regions/noalias/ranges/deref/frozen> \
//!    ret=<lo..=hi|-> stores=[a<k>:<lo..=hi>,…] \
//!    hot=<-|u32> calls=[<callee>@<depth>/<sites>/<constargs>,…] impls=[]
//! ```
//!
//! v2 (s99) adds the interprocedural range half — `ret=` is the
//! function's provable return range (`-` = unbounded), `stores=` the
//! store-meet per visible local container allocation site (`a<k>` in
//! RPO discovery order; poisoned or store-free sites are absent).
//! Both are computed by `midend/interproc.rs` on the same bodies this
//! index describes. The bump invalidates every release cluster cache
//! key by construction (the version leads the key accumulator).
//!
//! Field vocabulary:
//! - `home` — the defining source module (`root` for the package root;
//!   `-` when the caller has no module map, e.g. a hand-built fixture).
//! - `size` — WIR instruction count over the reachable layout: the
//!   quantity every threshold in [`super::Thresholds`] is denominated
//!   in.
//! - `flags` — one letter per predicate, in this fixed order, `-` where
//!   absent: `E` exported (C membrane, D19), `M` the program entry,
//!   `T` has effect-token parameters, `S` single-block body, `R`
//!   recursive (in a call-graph SCC with itself), `K` contains a task
//!   seam (`__wolf_rt_scope_*`/`__wolf_rt_proc_*` — the conc
//!   conservatism marker), `A` address-taken (`func.addr`).
//! - `hash` — the D8 content hash of the NORMALIZED body (equivalence
//!   up to local names only; see [`super::dedup::body_hash`]).
//! - `facts` — the D2 fact digest: counts per fact kind, in the fixed
//!   order region/noalias/range/deref/frozen.
//! - `hot` — the hotness hint. **s43 reserved this slot and s45 fills
//!   it; the format did not change to accommodate it, which is the
//!   whole point of having reserved it.** `-` means *unknown* (no
//!   profile, or a profile with no record for this body) and is the
//!   default in every build. A number is a **normalized 0..=1000
//!   hotness rank**: `1000 × this body's peak block count ÷ the
//!   hottest peak block count in the program`, integer arithmetic, ties
//!   included. It is deliberately relative and bounded rather than a
//!   raw count: the summary index is folded into the release cluster
//!   cache key, and a raw count would make the key depend on how long
//!   the training run happened to be. `hot=0` is *proven cold* (a
//!   record exists and the body never executed) and is real
//!   information; `hot=-` is *unknown* and must never be treated as
//!   cold — that is what keeps a stale profile from degrading a build
//!   below the no-profile one. See [`apply_profile`].
//! - `calls` — outgoing edges to MODULE functions (external `decl`
//!   callees, including every `__wolf_rt_*` seam, are not edges; they
//!   are opaque by construction). The three fields after `@` are the
//!   deepest loop depth any site sits at, the site count, and the
//!   constant-argument count — the exact inputs [`super::inline`]'s budget consults, so
//!   the import decision can be made from summaries alone.
//! - `impls` — **RESERVED HEADROOM (D42)**. Per-trait
//!   possible-implementation sets, for the singleton-dyn
//!   devirtualization pass ruled POST-M2 backlog by report 10 (no T1
//!   kernel is dyn-bound). The field is emitted and parsed at v1 and is
//!   ALWAYS EMPTY; when the pass lands it fills in without a version
//!   bump, which is precisely why it is frozen now — c12/tooling can
//!   rely on the slot existing. Nothing in this sprint writes it, and
//!   no pass reads it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ir::{Aux, FuncId, Function, Module};
use crate::ops::Opcode;

use super::analysis;
use super::dedup::body_hash;

/// The frozen summary format version. Bump ONLY with a schema change;
/// it is folded into every release-tier cache key, so a bump
/// invalidates cluster objects and nothing else.
pub const SUMMARY_FORMAT_VERSION: u32 = 2;

/// The home-module map: WIR function name → defining source module.
/// The mid-end has no notion of source modules (one `Module` holds the
/// whole package), so the driver supplies this; fixtures use
/// [`Homes::single`].
#[derive(Clone, Debug, Default)]
pub struct Homes {
    map: BTreeMap<String, String>,
}

impl Homes {
    /// Every function in one nameless module (the fixture/library
    /// shape): the module phase then optimizes the program as one unit.
    pub fn single() -> Homes {
        Homes::default()
    }

    /// Record `func`'s defining module.
    pub fn set(&mut self, func: impl Into<String>, home: impl Into<String>) {
        self.map.insert(func.into(), home.into());
    }

    /// The map the driver uses: a WIR function's home is the sema
    /// module owning the source file its spans point into. Synthetic
    /// functions (entry shims, hand-built fixtures) have no file and
    /// ride the root module — the same rule the s31 per-module object
    /// split uses, so homes and objects never disagree.
    pub fn from_package(pkg: &wolf_sema::Package, m: &Module) -> Homes {
        let mut file_mod: HashMap<u32, usize> = HashMap::new();
        for (mi, md) in pkg.modules.iter().enumerate() {
            for &fi in &md.files {
                file_mod.insert(pkg.files[fi].raw.file.index() as u32, mi);
            }
        }
        let mut homes = Homes::default();
        for (_, f) in m.funcs.iter() {
            let mi = f
                .src_file
                .and_then(|fi| file_mod.get(&fi).copied())
                .unwrap_or(0);
            let dotted = pkg
                .modules
                .get(mi)
                .map(|md| md.dotted())
                .unwrap_or_default();
            homes.set(
                f.name.clone(),
                if dotted.is_empty() { "root" } else { &dotted },
            );
        }
        homes
    }

    /// The home of `func` — `"-"` when unmapped (synthetic functions
    /// and fixtures).
    pub fn home_of(&self, func: &str) -> &str {
        self.map.get(func).map(String::as_str).unwrap_or("-")
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// The D2 fact digest — counts per kind, never the facts themselves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FactDigest {
    pub region: u32,
    pub noalias: u32,
    pub range: u32,
    pub deref: u32,
    pub frozen: u32,
}

/// Per-function flags (the fixed `EMTSRKA` letter order).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    pub exported: bool,
    pub entry: bool,
    pub token_params: bool,
    pub single_block: bool,
    pub recursive: bool,
    pub task_seam: bool,
    pub address_taken: bool,
}

impl Flags {
    fn render(&self) -> String {
        let bits = [
            (self.exported, 'E'),
            (self.entry, 'M'),
            (self.token_params, 'T'),
            (self.single_block, 'S'),
            (self.recursive, 'R'),
            (self.task_seam, 'K'),
            (self.address_taken, 'A'),
        ];
        bits.iter()
            .map(|&(on, c)| if on { c } else { '-' })
            .collect()
    }
}

/// One outgoing call edge, aggregated over its sites.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallEdge {
    pub callee: String,
    /// Deepest loop depth any site of this edge sits at.
    pub depth: u32,
    pub sites: u32,
    /// Constant integer arguments at the deepest site (the inline
    /// budget's bonus input).
    pub const_args: u32,
}

/// RESERVED (D42): a trait and the implementations a call site could
/// dispatch to. Always empty at v1 — see the module docs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraitImpls {
    pub trait_name: String,
    pub impls: Vec<String>,
}

/// One function's summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncSummary {
    pub name: String,
    pub home: String,
    pub size: u32,
    pub blocks: u32,
    pub flags: Flags,
    pub body_hash: String,
    pub facts: FactDigest,
    /// v2 (s99): the provable return range; `None` = unbounded.
    pub ret: Option<(i128, i128)>,
    /// v2 (s99): store-meet per local container allocation site.
    pub stores: Vec<(u32, (i128, i128))>,
    /// The `hot=` slot: `None` = unknown (no profile / no record),
    /// `Some(0..=1000)` = normalized hotness rank. See the module docs
    /// and [`apply_profile`].
    pub hotness: Option<u32>,
    pub calls: Vec<CallEdge>,
    /// RESERVED (D42): always empty at v1.
    pub impls: Vec<TraitImpls>,
}

impl FuncSummary {
    /// This function's one canonical line.
    pub fn render(&self) -> String {
        let calls: Vec<String> = self
            .calls
            .iter()
            .map(|c| format!("{}@{}/{}/{}", c.callee, c.depth, c.sites, c.const_args))
            .collect();
        let impls: Vec<String> = self
            .impls
            .iter()
            .map(|t| format!("{}:{}", t.trait_name, t.impls.join("|")))
            .collect();
        let stores: Vec<String> = self
            .stores
            .iter()
            .map(|(k, (lo, hi))| format!("a{k}:{lo}..={hi}"))
            .collect();
        format!(
            "fn {} home={} size={} blocks={} flags={} hash={} facts={}/{}/{}/{}/{} ret={} \
             stores=[{}] hot={} calls=[{}] impls=[{}]",
            self.name,
            self.home,
            self.size,
            self.blocks,
            self.flags.render(),
            self.body_hash,
            self.facts.region,
            self.facts.noalias,
            self.facts.range,
            self.facts.deref,
            self.facts.frozen,
            match self.ret {
                Some((lo, hi)) => format!("{lo}..={hi}"),
                None => "-".to_string(),
            },
            stores.join(","),
            match self.hotness {
                Some(h) => h.to_string(),
                None => "-".to_string(),
            },
            calls.join(","),
            impls.join(",")
        )
    }
}

/// The whole program's summaries, in function-name order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramSummary {
    pub version: u32,
    pub funcs: Vec<FuncSummary>,
}

impl ProgramSummary {
    /// The canonical serialization (the frozen schema).
    pub fn render(&self) -> String {
        let mut out = format!("summary-format {}\n", self.version);
        for f in &self.funcs {
            out.push_str(&f.render());
            out.push('\n');
        }
        out
    }

    /// Content digest of the whole summary index.
    pub fn digest(&self) -> String {
        crate::hash::sha256_hex(self.render().as_bytes())
    }

    pub fn get(&self, name: &str) -> Option<&FuncSummary> {
        self.funcs
            .binary_search_by(|f| f.name.as_str().cmp(name))
            .ok()
            .map(|i| &self.funcs[i])
    }
}

/// The top of the normalized hotness scale (`hot=1000` is the hottest
/// body in the program). A rank, not a count — see the module docs.
pub const HOTNESS_SCALE: u32 = 1000;

/// Fill the reserved `hot=` slot from a `.wprof` (s45), and score how
/// much of the profile applied.
///
/// Matching is by **content hash and nothing else** — a record applies
/// to a body iff it names that body's D8 hash. Consequences, all of
/// them intended:
///
/// - a record for a body this build does not contain is STALE: it is
///   dropped, silently as far as the summary is concerned and with one
///   counted summary line as far as the driver is concerned. It can
///   never be applied to the wrong body, because the only thing that
///   could match it is a body with the same hash, which is the same
///   body;
/// - a body with no record keeps `hot=-` (unknown), NOT `hot=0`. A
///   half-stale profile therefore leaves the un-matched half of the
///   program exactly as the no-profile build would have it, instead of
///   marking it cold and pessimizing it;
/// - a fully stale profile changes no field of the index, so the
///   summary digest, the cluster keys, and the emitted binary are
///   byte-identical to a no-profile build.
///
/// The scale is taken over the peak BLOCK count, not the entry count:
/// a function called once around a million-iteration loop is hot, and
/// entry counts say it is not.
pub fn apply_profile(
    s: &mut ProgramSummary,
    profile: &crate::profile::Profile,
) -> crate::profile::Coverage {
    let cov = crate::profile::coverage(profile, s.funcs.iter().map(|f| f.body_hash.as_str()));
    let peak = |f: &FuncSummary| profile.get(&f.body_hash).map(|r| r.peak());
    let top = s.funcs.iter().filter_map(peak).max().unwrap_or(0);
    for f in &mut s.funcs {
        let Some(p) = profile.get(&f.body_hash).map(|r| r.peak()) else {
            continue; // unknown stays unknown
        };
        f.hotness = Some(if top == 0 {
            0
        } else {
            // Integer, deterministic, saturating: no floats anywhere
            // near a value that rides a cache key.
            ((u128::from(p) * u128::from(HOTNESS_SCALE)) / u128::from(top)) as u32
        });
    }
    cov
}

/// Summarize every function in `m` (target 4's module phase).
/// Deterministic: output is sorted by function name; every internal
/// traversal is over sorted or arena-index order.
pub fn summarize(m: &Module, homes: &Homes) -> ProgramSummary {
    // v2 (s99): the interprocedural range half rides the same scan.
    let ipr = super::interproc::analyze(m);
    let by_name: HashMap<&str, FuncId> = m
        .funcs
        .iter()
        .map(|(id, f)| (f.name.as_str(), id))
        .collect();
    // Address-taken is a module-wide property: any `func.addr` naming
    // the function anywhere (task entries are the live case).
    let mut address_taken: BTreeSet<&str> = BTreeSet::new();
    for (_, f) in m.funcs.iter() {
        for &b in &f.layout {
            for &inst in &f.blocks[b].insts {
                if f.insts[inst].op == Opcode::FuncAddr
                    && let Aux::Callee(ef) = f.insts[inst].aux
                {
                    address_taken.insert(f.ext_funcs[ef].name.as_str());
                }
            }
        }
    }
    // s98: a vtable slot is address-taken the same way — the shim's
    // address escapes into module data, so its identity is pinned
    // (dedup) and it never inlines away.
    for d in &m.data {
        for fname in &d.funcs {
            address_taken.insert(fname.as_str());
        }
    }
    let recursive = recursive_set(m, &by_name);
    let mut funcs: Vec<FuncSummary> = m
        .funcs
        .iter()
        .map(|(id, f)| one(m, id, f, homes, &by_name, &address_taken, &recursive, &ipr))
        .collect();
    funcs.sort_by(|a, b| a.name.cmp(&b.name));
    ProgramSummary {
        version: SUMMARY_FORMAT_VERSION,
        funcs,
    }
}

#[allow(clippy::too_many_arguments)]
fn one(
    m: &Module,
    id: FuncId,
    f: &Function,
    homes: &Homes,
    by_name: &HashMap<&str, FuncId>,
    address_taken: &BTreeSet<&str>,
    recursive: &BTreeSet<FuncId>,
    ipr: &super::interproc::Ipr,
) -> FuncSummary {
    let cfg = analysis::cfg(f);
    let doms = analysis::dominators(&cfg);
    let loops = analysis::loops(f, &cfg, &doms);
    let mut size = 0u32;
    let mut facts = FactDigest::default();
    let mut task_seam = false;
    // Edges keyed by callee name; `depth` takes the max, `const_args`
    // the count at the deepest site (ties: the first in RPO).
    let mut edges: BTreeMap<String, CallEdge> = BTreeMap::new();
    for &b in &cfg.rpo {
        let depth = loops.depth.get(&b).copied().unwrap_or(0);
        for &inst in &f.blocks[b].insts {
            size += 1;
            // s98: a `data.addr` of a vtable is an edge to every slot
            // shim — the table is emitted in the object that uses it,
            // and its slots relocate against functions that must be
            // co-resident (`func.addr`'s own subset rule). Without
            // these edges the partitioner separates shim from table
            // and the release tier refuses a program the debug tier
            // runs.
            if f.insts[inst].op == Opcode::DataAddr
                && let Aux::Data(di) = f.insts[inst].aux
            {
                if let Some(d) = m.data.get(di as usize) {
                    for fname in &d.funcs {
                        if !by_name.contains_key(fname.as_str()) {
                            continue;
                        }
                        let e = edges.entry(fname.clone()).or_insert_with(|| CallEdge {
                            callee: fname.clone(),
                            depth: 0,
                            sites: 0,
                            const_args: 0,
                        });
                        e.sites += 1;
                        if depth >= e.depth {
                            e.depth = depth;
                        }
                    }
                }
                continue;
            }
            if f.insts[inst].op != Opcode::Call {
                continue;
            }
            let Aux::Callee(ef) = f.insts[inst].aux else {
                continue;
            };
            let name = &f.ext_funcs[ef].name;
            if is_task_seam(name) {
                task_seam = true;
            }
            if !by_name.contains_key(name.as_str()) {
                continue; // external decl: opaque, never an edge
            }
            let const_args = f
                .vpool
                .get(f.insts[inst].args)
                .into_iter()
                .filter(|&a| analysis::const_int(f, a).is_some())
                .count() as u32;
            let e = edges.entry(name.clone()).or_insert_with(|| CallEdge {
                callee: name.clone(),
                depth: 0,
                sites: 0,
                const_args: 0,
            });
            e.sites += 1;
            if depth >= e.depth {
                e.depth = depth;
                e.const_args = const_args;
            }
        }
    }
    for fd in f.facts.values() {
        use crate::facts::FactKind;
        match fd.kind {
            FactKind::Region(..) => facts.region += 1,
            FactKind::Noalias(..) => facts.noalias += 1,
            FactKind::Range(..) => facts.range += 1,
            FactKind::Deref(..) => facts.deref += 1,
            FactKind::Frozen(..) => facts.frozen += 1,
        }
    }
    let token_params = f
        .entry()
        .map(|e| {
            f.block_params(e)
                .iter()
                .any(|&p| m.types.is_token(f.value_ty(p)))
        })
        .unwrap_or(false);
    FuncSummary {
        name: f.name.clone(),
        home: homes.home_of(&f.name).to_string(),
        size,
        blocks: cfg.rpo.len() as u32,
        flags: Flags {
            exported: f.export,
            entry: f.name == "main",
            token_params,
            single_block: f.layout.len() == 1,
            recursive: recursive.contains(&id),
            task_seam,
            address_taken: address_taken.contains(f.name.as_str()),
        },
        body_hash: body_hash(m, f),
        facts,
        ret: ipr.rets.get(&f.name).copied().flatten(),
        stores: ipr
            .stores
            .get(&f.name)
            .map(|st| st.iter().map(|(&k, &r)| (k, r)).collect())
            .unwrap_or_default(),
        hotness: None,
        calls: edges.into_values().collect(),
        impls: Vec::new(),
    }
}

/// A conc seam: the runtime entry points that create, spawn into, or
/// join a structured scope or proc. A callee carrying one is never
/// imported across a cluster boundary (the s42 conc conservatism,
/// restated at the whole-program layer).
pub(crate) fn is_task_seam(name: &str) -> bool {
    name.starts_with("__wolf_rt_scope_")
        || name.starts_with("__wolf_rt_proc_")
        || name.starts_with("__wolf_rt_task_")
        || name.starts_with("__wolf_rt_chan_")
        || name.starts_with("__wolf_rt_select_")
}

/// Functions that can reach themselves through module call edges —
/// direct recursion and every SCC member alike (the inliner refuses
/// them, so the import decision must not offer them either).
///
/// Reachability by DFS per node: quadratic in the function count, and
/// the function count is what it is (a package, not a universe); the
/// clarity is worth more here than an asymptote.
fn recursive_set(m: &Module, by_name: &HashMap<&str, FuncId>) -> BTreeSet<FuncId> {
    let succ: HashMap<FuncId, Vec<FuncId>> = m
        .funcs
        .iter()
        .map(|(id, f)| {
            let mut v: Vec<FuncId> = Vec::new();
            for &b in &f.layout {
                for &inst in &f.blocks[b].insts {
                    if let Aux::Callee(ef) = f.insts[inst].aux
                        && let Some(&cid) = by_name.get(f.ext_funcs[ef].name.as_str())
                        && !v.contains(&cid)
                    {
                        v.push(cid);
                    }
                }
            }
            (id, v)
        })
        .collect();
    let mut out: BTreeSet<FuncId> = BTreeSet::new();
    for root in m.funcs.keys() {
        let mut seen: BTreeSet<FuncId> = BTreeSet::new();
        let mut work: Vec<FuncId> = succ[&root].clone();
        while let Some(v) = work.pop() {
            if v == root {
                out.insert(root);
                break;
            }
            if seen.insert(v) {
                work.extend(succ[&v].iter().copied());
            }
        }
    }
    out
}
