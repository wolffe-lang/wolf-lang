//! The memory checker (s18–s23) — the language's soul.
//!
//! Contract: Tier-0 MVS exclusivity (loan sets over a checker-internal,
//! WIR-shaped check CFG — never exported; WIR builds from typed HIR),
//! region inference and the region checker (D10), Perceus-style `shared`
//! insertion and generational `handle` pools (X5), the unsafe tier with
//! Tree-Borrows-shaped provenance (D11), and the miri-lite UB checker
//! (s23). Checker facts flow to WIR as verified fact annotations.
//!
//! # Implemented today (s18 — Tier-0 exclusivity)
//!
//! - **The check CFG** ([`cfg`]): per-function basic blocks of effect
//!   statements over field-granular *places* ([`place`],
//!   `[mem.model.path.disjoint]`), explicit edges for control flow,
//!   `?` error edges (D30), and X3 trap edges. Checker-internal by
//!   contract; its textual dump is snapshot-pinned.
//! - **Moves** ([`moves`]): forward maybe-uninitialized dataflow —
//!   use-after-move / use-of-uninit (E1001) with move-site
//!   provenance, partial moves per field, re-initialization revives
//!   (whole or per-field residue).
//! - **Call-site mode agreement** (E1007, in [`lower`]): `f(mut x)` /
//!   `f(take x)` must match the declaration (X1) — the c03 handoff,
//!   decided to fire here with exclusivity, one diagnostic in one
//!   place. `mut` needs a place, not a temporary (E1009).
//! - **Exclusivity** ([`excl`], E1002): within one call, a `mut`
//!   place has no other live access path (`[mem.tier0.excl]`);
//!   disjoint fields go `mut` together; `Copy` reads complete at
//!   argument evaluation, which is what keeps the two-phase
//!   `xs.push(xs.len)` shape legal.
//! - **View sets** (E1008): `fn norm(mut self.{x, y})` narrows the
//!   receiver's exclusive footprint at call sites and pins the callee
//!   to the declared fields (`[mem.tier0.excl.3]`).
//! - **Local loan dataflow** ([`loans`]): NLL-grade per-function loan
//!   sets — gen at borrows, killed by base overwrite and by borrower
//!   last-use (backward liveness), two-phase reservation. The
//!   surface (`&x`) refuses in sema until the region campaign, so the
//!   engine is exercised by its unit tests (RFC 2094's problem cases,
//!   the Polonius case accepted).
//! - **Fact export stubs** ([`facts`]): `mut` → noalias +
//!   dereferenceable, `read` → frozen-for-call, whole-value move
//!   sites — the s26 schema, snapshot-tested now.
//! - **Region inference** ([`regions`], s19): every heap allocation
//!   (struct literal, constructor, non-`Copy` call result) is
//!   attributed to the ambient region (`[mem.region.create.3]`, D12)
//!   with Cyclone-rule signature defaults — fresh generalized
//!   parameter regions, results in `ρ_caller`, zero annotations in
//!   the common shape. Conflicting placement demands are E1004;
//!   region-local data outliving its region's wholesale free is
//!   E1010. Escape analysis records stack-promotion facts (regions
//!   whose create/free pair is frame-local; individual allocations
//!   that never escape) for c05/s26 — facts only, no codegen.
//! - **The region checker** ([`lower`]-integrated, s20): `freeze`
//!   consumes the affine value and promotes the owned subtree to
//!   `imm` — writes through any path into frozen data are E1012,
//!   frozen sites are exempt from co-location (`[mem.region.edge.imm]`)
//!   and outlive every frame; transfer/freeze of an open region is
//!   E1005 (`[mem.region.freeze.3]` — the open window pins the
//!   handle, `mut`-lending included); the multiopen open set is
//!   checked as an **antichain in the region forest**
//!   (`[mem.region.multiopen]`, E1011 — iso parent edges recorded at
//!   embedding stores, `in h.child { }` resolves one-step field
//!   paths); a call returning `region` mints a fresh identity (the
//!   scheme-carrying interface's shape — wolf_sema renders and hashes
//!   the derived Cyclone scheme into every heap-reaching fn sig).
//! - **The shared tier** ([`shared`]-module + [`lower`]-integrated,
//!   s21): strong `shared` edges must form a DAG at the type level —
//!   a strong cycle (through embeds, `List`/`Pool` elements, tuples,
//!   rows, or `shared` wrappers) is E1006 at the definition, with the
//!   `weak`/`handle` rewrite prescribed; `shared` cells are the
//!   sanctioned cross-region escape (exempt from co-location and
//!   region-close sweeps — RC frees them, never a region); the
//!   Perceus insertion *plan* is recorded as facts (dup at `clone()`
//!   fan-out, drop at scope exit LIFO with defers per
//!   `[mem.shared.drop.1]`, migratable to last use — c05 lowers it);
//!   every cell carries the **static atomicity bit** (non-atomic
//!   unless a freeze/transfer point reaches it — thread-exclusivity
//!   is structural); `pool[h]` accesses emit generational
//!   `handle-check` statements with trap edges (`stale-handle` is the
//!   dynamic half by contract, X5 — no static approximation exists,
//!   because handles are plain data whose validity is only decided at
//!   the slot). The prelude containers `List[T]`/`Pool[T]` type as
//!   sema builtins (enough for the Tier-2 corpus; std proper is s37).
//! - **The unsafe tier** ([`lower`]-integrated, s22): the floor,
//!   with rules *simpler* than the safe tier's (D11, the
//!   anti-Stacked-Borrows posture). Typing is permissive — raw
//!   pointers are inert data, deriving/holding/passing them is free
//!   (creation is not a use, Tree Borrows) — and each *operation*
//!   checks one local rule: raw reads/writes (`p[i]`), non-identity
//!   pointer casts ([`mem.prov.expose`] bridges), strict-provenance
//!   ops (`addr`/`with_addr`/`expose`/`with_exposed`), `assume
//!   noalias`, `borrow … from …`, and C calls all demand the
//!   `unsafe` ring (E1301); signatures never carry `*T` and exported
//!   types never mention it — module-private fields may (E1302,
//!   `[mem.unsafe.scope]`); `assume` operands must be raw pointers
//!   (E1304); door 1's operands must be `region` + `*T` (E1305, the
//!   claim itself is the dynamic P6 obligation). Every raw op is
//!   recorded as an **attribution fact** (see [`facts`]'s s22
//!   schema) for s23's miri-lite and the is04 oracle — no aliasing
//!   is assumed, no UB is approximated statically: a file this tier
//!   accepts may still be `ub(mem.ub)` at run time, by design. The
//!   `# Safety:` comment lint is W1301 (advisory). The `#[trusted]`
//!   manifest rule (E1303) lives in `wolf_sema::audit` with `wolf
//!   audit-surface`.
//! - **Mode enforcement** ([`lower`]-integrated, s72 — D39/D40, the
//!   last red row of the v0.1.0 audit): writes reaching a `read`
//!   parameter — projections and `mut`-lends included — are E1014
//!   (`[mem.tier0.mode.read]`, the callee-side half #27 found
//!   missing); arguments are two-phase — a later argument may read a
//!   place an earlier `mut` argument claims (`f(mut a, a.x)`, legal
//!   since s186's `[mem.tier0.excl.4]`) but not write or move it; and
//!   `for x in xs` holds a READ
//!   claim on the iterated place for the loop's extent
//!   (`[mem.iter.excl]`) — the container never moves (the #15
//!   reads-as-moves accident is dead), stays live after the loop, and
//!   mut uses inside the body (push/pop/clear, element writes,
//!   `mut`-lends, moves) are E1013 with the collect-then-apply /
//!   index-loop teaching.
//! - **Honest refusals**: inline C/asm (c10),
//!   closures/concurrency (c05), region identity through conflicting
//!   rebinds or multi-step paths (c05 backlog), `shared` acyclicity
//!   through opaque generic std types (s16) return [`NotYet`] — the
//!   conform-run `mem` rung completes only when every body was
//!   actually checked.

use wolf_ast::{GreenNode, SyntaxKind, is_expr_kind};
use wolf_diag::{Diagnostic, codes};
use wolf_sema::sig::{ItemSig, SigTables};
use wolf_sema::types::{TyId, TyKind, TypeTable};
use wolf_sema::{BodyRef, BodyResult, NotYet, Package, Typecheck, TypedBody};
use wolf_span::Span;

pub mod byteview;
pub mod cfg;
mod excl;
pub mod facts;
pub mod json;
pub mod loans;
mod lower;
mod moves;
pub mod place;
mod procjoin;
pub mod regions;
mod shared;
mod strres;
pub mod ubcheck;

pub use facts::FnFacts;
pub use lower::Lowered;
pub use regions::RegionSummary;

/// The package-level result of the s18–s19 memory checker.
#[derive(Debug, Default)]
pub struct MemCheck {
    /// E1xxx diagnostics, deterministically sorted. Meaningful for
    /// the rung verdict only when `not_yet` is empty (the same
    /// conservatism contract as the typecheck rung: never report a
    /// half-checked file).
    pub diagnostics: Vec<Diagnostic>,
    /// Every honest refusal, in body order.
    pub not_yet: Vec<NotYet>,
    /// Per-function fact summaries (s26's input schema).
    pub facts: Vec<FnFacts>,
    /// Per-function region inference records (s19): the reviewable
    /// dump surface behind `wolf conform-run --dump=regions`.
    pub regions: Vec<RegionSummary>,
}

/// Check every fully-typed body of the package. `tc` must come from
/// [`wolf_sema::typecheck_package`] over the same `pkg`.
pub fn check_package(pkg: &Package, tc: &Typecheck) -> MemCheck {
    let mut out = MemCheck::default();
    // s216 (wolf-lang#618): the declared fns whose `str` result is only
    // ever static bytes — their calls mint no ambient site.
    let statics = strres::static_str_fns(pkg, tc);
    // s22: unsafety never crosses a signature ([mem.unsafe.scope],
    // E1302) — checked over the signature tables before any body runs.
    unsafe_sig_check(pkg, &tc.sigs, &mut out.diagnostics);
    // s21: strong `shared` edges must form a DAG at the type level
    // ([mem.shared.rc.2], E1006) — checked over the signature tables
    // before any body runs.
    shared::check(&tc.sigs, &mut out.diagnostics, &mut out.not_yet);
    // s89: the byte-view lend verdicts live in `byteview`, and
    // `wolf_wir::lower` computes them to decide which call sites may
    // pass a view instead of a copy. Since wolf-lang#387 retired W1004
    // that lowering is their ONLY consumer — this pass no longer needs
    // the table for a diagnostic of its own.
    // s233 (wolf-lang#678's helper half): every body is lowered before
    // any is judged, because the must-join check reads the whole
    // package — a call is refused by what its callee leaves running.
    // s255 (`[mem.region.outer-write]`): and in callee-first order,
    // because a call reads its callee's ambient-clean verdicts.
    let mut lowered_all: Vec<(&wolf_sema::BodyOutcome, Lowered)> = Vec::new();
    for (outcome, lowered) in lower_all(pkg, tc, &statics) {
        match lowered {
            Err(nyc) => out.not_yet.push(nyc),
            Ok(lowered) => lowered_all.push((outcome, lowered)),
        }
    }
    let must_join = procjoin::solve(
        &lowered_all
            .iter()
            .map(|(_, l)| (&l.cfg, &l.procs))
            .collect::<Vec<_>>(),
    );
    for ((outcome, lowered), joined) in lowered_all.into_iter().zip(must_join) {
        let _progress = wolf_span::progress::span(
            "unit",
            &pkg.modules[outcome.body.module].dotted(),
            "mem/check",
            &outcome.body.name,
        );
        {
            {
                out.diagnostics.extend(lowered.diags);
                out.diagnostics.extend(joined);
                moves::check(&lowered.cfg, &mut out.diagnostics);
                excl::check(&lowered.cfg, &mut out.diagnostics);
                loans::check(&lowered.cfg, &mut out.diagnostics);
                out.facts.push(facts::collect(
                    &lowered.cfg,
                    &lowered.regions,
                    &lowered.rc_cells,
                ));
                // W1001 (s68) — a region inference proves empty: no
                // allocation ever lands in it and its handle never
                // leaves the frame. A body that exposes a region's
                // base to the raw tier is exempt wholesale: an empty
                // arena handed across the C membrane is filled by
                // code this checker cannot see.
                let in_std = pkg.modules[outcome.body.module]
                    .path
                    .first()
                    .is_some_and(|s| s == "std");
                let exposes_region = lowered.cfg.blocks.iter().any(|b| {
                    b.stmts.iter().any(
                        |s| matches!(s, cfg::Stmt::Expose { dir, .. } if *dir == "region->ptr"),
                    )
                });
                for &rid in &lowered.regions.never_allocates {
                    if exposes_region || in_std {
                        break;
                    }
                    let r = &lowered.regions.regions[rid.0 as usize];
                    out.diagnostics.push(
                        Diagnostic::warning(
                            codes::W1001,
                            r.span,
                            format!("the region `{}` never allocates", r.name),
                        )
                        .with_label("nothing lives here")
                        .with_note(
                            "no allocation is attributed to this region and its \
                             handle stays in this frame; delete the region, or move \
                             the allocation it was written for inside it."
                                .to_string(),
                        ),
                    );
                }
                out.regions.push(lowered.regions);
            }
        }
    }
    wolf_diag::sort_diagnostics(&mut out.diagnostics);
    out
}

/// Run every path-sensitive pass over one check CFG (the moves, call
/// exclusivity, and loan analyses) and return its diagnostics. The
/// fuzz/property surface: deterministic for a given CFG, never
/// panicking.
pub fn check_cfg(cfg: &cfg::Cfg) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    moves::check(cfg, &mut diags);
    excl::check(cfg, &mut diags);
    loans::check(cfg, &mut diags);
    diags
}

/// The check-CFG dumps of every lowerable body, in body order (the
/// s01 IR-dump snapshot family).
pub fn dump_package(pkg: &Package, tc: &Typecheck) -> String {
    let mut out = String::new();
    let statics = strres::static_str_fns(pkg, tc);
    // s255: the same callee-first lowering `check_package` runs, so a
    // dump shows the sites the check saw (a clean parameter's call
    // mints none); the output stays in body order.
    for (_, lowered) in lower_all(pkg, tc, &statics) {
        if let Ok(lowered) = lowered {
            out.push_str(&lowered.cfg.dump());
            out.push('\n');
        }
    }
    out
}

/// s255 (`[mem.region.outer-write]`): every `mut` parameter's
/// ambient-clean verdict, one line each, in body order — the
/// `--dump=mut-clean` debug surface a port reads to see at once which
/// calls a per-turn `region` block will refuse and why:
/// `fn <name> #<index> <param>: clean` or `… unclean at <file>:<line>:<col>`.
/// A body that did not lower prints `fn <name>: (not lowered)`.
pub fn dump_mut_clean_package(pkg: &Package, tc: &Typecheck) -> String {
    let mut out = String::new();
    let statics = strres::static_str_fns(pkg, tc);
    for (outcome, lowered) in lower_all(pkg, tc, &statics) {
        let name = &outcome.body.name;
        let Ok(lowered) = lowered else {
            out.push_str(&format!("fn {name}: (not lowered)\n"));
            continue;
        };
        let params = body_fn_node(pkg, &outcome.body)
            .and_then(|(_, outer)| fn_params(pkg, &tc.sigs, &outcome.body, outer))
            .unwrap_or(&[]);
        for (idx, verdict) in &lowered.mut_clean {
            let pname = params
                .get(*idx as usize)
                .map(|p| p.name.as_str())
                .unwrap_or("?");
            match verdict {
                Ok(()) => out.push_str(&format!("fn {name} #{idx} {pname}: clean\n")),
                Err(at) => {
                    // The store is in the body's own file (the verdict
                    // reads this body's holds), so the body's file index
                    // names it; a span's `FileId` is the loader's.
                    let Some(f) = pkg.files.get(outcome.body.file) else {
                        out.push_str(&format!(
                            "fn {name} #{idx} {pname}: unclean at file#{}\n",
                            outcome.body.file
                        ));
                        continue;
                    };
                    let src = &f.raw.src;
                    let lo = (at.lo as usize).min(src.len());
                    let hi = (at.hi as usize).min(src.len()).max(lo);
                    let line = 1 + src[..lo].iter().filter(|&&b| b == b'\n').count();
                    let col = 1 + lo
                        - src[..lo]
                            .iter()
                            .rposition(|&b| b == b'\n')
                            .map_or(0, |p| p + 1);
                    let snippet = String::from_utf8_lossy(&src[lo..hi])
                        .lines()
                        .next()
                        .unwrap_or("")
                        .to_string();
                    out.push_str(&format!(
                        "fn {name} #{idx} {pname}: unclean at {}:{line}:{col} `{snippet}`\n",
                        f.raw.display
                    ));
                }
            }
        }
    }
    out
}

/// The region-inference dumps of every lowerable body, in body order
/// (the `--dump=regions` debug surface, snapshot-pinned).
pub fn dump_regions_package(pkg: &Package, tc: &Typecheck) -> String {
    let mut out = String::new();
    let statics = strres::static_str_fns(pkg, tc);
    for (_, lowered) in lower_all(pkg, tc, &statics) {
        if let Ok(lowered) = lowered {
            out.push_str(&lowered.regions.render());
            out.push('\n');
        }
    }
    out
}

/// s160 (wolf-lang#355, `[mem.region.proc]`): is this body named by
/// some `spawn proc` in the package? Such a body lowers with the
/// PROC's own root region as its ambient, not its caller's — a proc
/// is a failure domain owning its regions (`[conc.proc.1]`) and
/// `[conc.proc.kill]` step 3 bulk-frees them at its exit, so
/// `[mem.region.create.3]`'s "the caller's current region" default has
/// no caller to name. The set is sema's (`Typecheck::proc_entries`) —
/// the one place a spawn's name is resolved; this must never
/// re-resolve it. A function used BOTH ways — spawned somewhere and
/// called directly elsewhere — is checked under the proc rule in
/// both: the conservative reading, and the one a reader can predict
/// from the source without tracing every call site.
fn is_proc_entry(tc: &Typecheck, body: &BodyRef) -> bool {
    body.member.is_none() && tc.proc_entries.contains(&(body.module, body.name.clone()))
}

/// Lower one checked body. `None`: the item has no lowerable body
/// (extern fn, broken tree).
/// s255 (wolf-lang#839, #856, `[mem.region.outer-write]`): lower every
/// checked body of the package, callee-first, filling the ambient-clean
/// table as it goes — the answer each `mut` parameter's own lowering
/// gives, read by every call that passes a `mut` argument.
///
/// The bodies form a call graph (a declared call's `decl_span` is its
/// callee's name span, the key `strres` uses too); its strongly
/// connected components are lowered in reverse topological order, so a
/// callee's verdicts are final before its callers read them. A
/// component with a cycle (a recursive walk like aspen's `walk_dir`, or
/// mutual recursion) starts from the optimistic answer — every `mut`
/// parameter clean — and is re-lowered until no verdict moves: the
/// verdicts only ever go from clean to unclean, so the fixpoint is
/// reached in at most one step per parameter that falls (bounded here,
/// with every member marked unclean past the bound). A component with
/// no cycle is lowered once. The result is each body's lowering (the
/// last one of its component), in the package's body order.
fn lower_all<'a>(
    pkg: &'a Package,
    tc: &'a Typecheck,
    statics: &std::collections::HashSet<wolf_span::Span>,
) -> Vec<(&'a wolf_sema::BodyOutcome, Result<Lowered, NotYet>)> {
    use std::collections::HashMap;
    // The lowerable bodies, each with its key (a fn's name span) and
    // its parameter modes (for the optimistic start).
    struct Node<'a> {
        outcome: &'a wolf_sema::BodyOutcome,
        tb: &'a TypedBody,
        key: Option<wolf_span::Span>,
        mut_params: Vec<u32>,
    }
    let mut nodes: Vec<Node<'a>> = Vec::new();
    for outcome in &tc.bodies {
        let BodyResult::Checked(tb) = &outcome.result else {
            continue;
        };
        let (key, mut_params) = match body_fn_node(pkg, &outcome.body) {
            Some((node, outer)) => {
                let key = wolf_ast::FnDecl::cast(node).and_then(|d| d.name().map(|t| t.span));
                let params = fn_params(pkg, &tc.sigs, &outcome.body, outer).unwrap_or_default();
                let muts = params
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.mode == Some(wolf_ast::ParamMode::Mut))
                    .map(|(i, _)| i as u32)
                    .collect();
                (key, muts)
            }
            None => (None, Vec::new()),
        };
        nodes.push(Node {
            outcome,
            tb,
            key,
            mut_params,
        });
    }
    let by_key: HashMap<wolf_span::Span, usize> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.key.map(|k| (k, i)))
        .collect();
    let edges: Vec<Vec<usize>> = nodes
        .iter()
        .map(|n| {
            let mut out: Vec<usize> =
                n.tb.calls
                    .iter()
                    .filter_map(|(_, c)| c.decl_span.and_then(|d| by_key.get(&d).copied()))
                    .collect();
            out.sort_unstable();
            out.dedup();
            out
        })
        .collect();
    // Tarjan's SCCs, emitted in reverse topological order (callees
    // before callers), iteratively (a deep call chain is not a deep
    // recursion here).
    let n = nodes.len();
    let mut index = vec![usize::MAX; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut sccs: Vec<Vec<usize>> = Vec::new();
    let mut next = 0usize;
    for root in 0..n {
        if index[root] != usize::MAX {
            continue;
        }
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        index[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (v, ref mut ei)) = work.last_mut() {
            if *ei < edges[v].len() {
                let w = edges[v][*ei];
                *ei += 1;
                if index[w] == usize::MAX {
                    index[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    work.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            } else {
                work.pop();
                if let Some(&(u, _)) = work.last() {
                    low[u] = low[u].min(low[v]);
                }
                if low[v] == index[v] {
                    let mut comp = Vec::new();
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        comp.push(w);
                        if w == v {
                            break;
                        }
                    }
                    comp.sort_unstable();
                    sccs.push(comp);
                }
            }
        }
    }
    let mut table: lower::MutClean = HashMap::new();
    let mut results: Vec<Option<Result<Lowered, NotYet>>> = (0..n).map(|_| None).collect();
    let lower_one = |i: usize, table: &lower::MutClean| -> Option<Result<Lowered, NotYet>> {
        let nd = &nodes[i];
        lower_body(
            pkg,
            &tc.sigs,
            nd.tb,
            &nd.outcome.body,
            is_proc_entry(tc, &nd.outcome.body),
            statics,
            table,
        )
    };
    for comp in &sccs {
        let cyclic = comp.len() > 1 || comp.iter().any(|&v| edges[v].contains(&v));
        // The optimistic start: every `mut` parameter of the component
        // clean, so a recursive call inside it reads "clean" first.
        if cyclic {
            for &i in comp {
                if let Some(k) = nodes[i].key {
                    for &pi in &nodes[i].mut_params {
                        table.insert((k, pi), Ok(()));
                    }
                }
            }
        }
        let mut rounds = 0usize;
        loop {
            rounds += 1;
            let mut moved = false;
            for &i in comp {
                let r = lower_one(i, &table);
                if let (Some(Ok(l)), Some(k)) = (&r, nodes[i].key) {
                    for &(pi, ref verdict) in &l.mut_clean {
                        let prev = table.get(&(k, pi)).cloned();
                        if prev.as_ref() != Some(verdict) {
                            moved = true;
                            table.insert((k, pi), *verdict);
                        }
                    }
                }
                results[i] = r;
            }
            if !cyclic || !moved {
                break;
            }
            if rounds >= 16 {
                // Past the bound: every member unclean, one last pass.
                for &i in comp {
                    if let Some(k) = nodes[i].key {
                        for &pi in &nodes[i].mut_params {
                            let at = k;
                            table.insert((k, pi), Err(at));
                        }
                    }
                }
                for &i in comp {
                    results[i] = lower_one(i, &table);
                }
                break;
            }
        }
    }
    nodes
        .into_iter()
        .zip(results)
        .filter_map(|(nd, r)| r.map(|r| (nd.outcome, r)))
        .collect()
}

/// The declaration node a body lowers from, with its enclosing item
/// for an impl or trait member — `None` when the body is not a fn.
fn body_fn_node<'a>(
    pkg: &'a Package,
    body: &BodyRef,
) -> Option<(&'a GreenNode, Option<&'a GreenNode>)> {
    let root = &pkg.files[body.file].parse.root;
    let node = root.nodes().filter(|n| n.kind.is_item()).nth(body.decl)?;
    let (node, outer) = match body.member {
        None => (node, None),
        Some(mi) => {
            let inner = node.nodes().filter(|n| n.kind.is_item()).nth(mi)?;
            (inner, Some(node))
        }
    };
    (node.kind == SyntaxKind::FnDecl).then_some((node, outer))
}

fn lower_body(
    pkg: &Package,
    sigs: &SigTables,
    tb: &TypedBody,
    body: &BodyRef,
    proc_entry: bool,
    statics: &std::collections::HashSet<wolf_span::Span>,
    mut_clean: &lower::MutClean,
) -> Option<Result<Lowered, NotYet>> {
    let _progress = wolf_span::progress::span(
        "unit",
        &pkg.modules[body.module].dotted(),
        "mem/lower",
        &body.name,
    );
    let root = &pkg.files[body.file].parse.root;
    let node = root.nodes().filter(|n| n.kind.is_item()).nth(body.decl)?;
    let (node, outer) = match body.member {
        None => (node, None),
        Some(mi) => {
            let inner = node.nodes().filter(|n| n.kind.is_item()).nth(mi)?;
            (inner, Some(node))
        }
    };
    let lowerer = lower::Lowerer::new(pkg, sigs, tb, body.module, body.file)
        .with_static_strs(statics)
        .with_mut_clean(mut_clean);
    match node.kind {
        SyntaxKind::FnDecl => {
            let d = wolf_ast::FnDecl::cast(node)?;
            let block = d.body()?;
            let params = fn_params(pkg, sigs, body, outer)?;
            // s160: the proc's own region is introduced at the
            // function's NAME — where a reader sees which proc this
            // is, and where lupin's `region-fault` points too.
            let proc_intro = proc_entry.then(|| d.name().map_or(block.syntax().span, |t| t.span));
            let key = d.name().map(|t| t.span);
            Some(
                lowerer
                    .lower_fn(&body.name, params, block, proc_intro)
                    .map(|mut l| {
                        // s233: the must-join check's name for this body.
                        l.procs.key = key;
                        l.procs.trait_key = trait_method_key(sigs, body, outer);
                        l.procs.name = body.name.clone();
                        l
                    }),
            )
        }
        SyntaxKind::ConstDecl => {
            let init = node.nodes().find(|n| is_expr_kind(n.kind))?;
            Some(lowerer.lower_init(&body.name, init))
        }
        SyntaxKind::LetDecl | SyntaxKind::VarDecl => {
            // The binder that declares THIS name — a comma group
            // carries several, each its own body (D63).
            let src = &pkg.files[body.file].raw.src;
            let binders = wolf_ast::binding_binders(node);
            let owner = binders
                .iter()
                .find(|b| {
                    b.pattern.is_some_and(|p| {
                        p.tokens().any(|t| {
                            t.kind == SyntaxKind::Ident
                                && &src[t.span.lo as usize..t.span.hi as usize]
                                    == body.name.as_bytes()
                        })
                    })
                })
                .or(binders.first())?;
            let init = owner.init?;
            Some(lowerer.lower_init(&body.name, init))
        }
        _ => None,
    }
}

/// The declared parameter list (modes and view sets) of a fn body:
/// module item, impl method, or trait default body.
fn fn_params<'a>(
    pkg: &'a Package,
    sigs: &'a SigTables,
    body: &BodyRef,
    outer: Option<&GreenNode>,
) -> Option<&'a [wolf_sema::sig::ParamSig]> {
    match outer {
        None => match sigs.get(body.module, &body.name)? {
            ItemSig::Fn(f) => Some(&f.params),
            _ => None,
        },
        Some(o) if o.kind == SyntaxKind::ImplDecl => {
            let imp = sigs
                .impls
                .iter()
                .find(|i| i.file == body.file && i.decl == body.decl)?;
            imp.methods
                .iter()
                .find(|m| m.name == body.name)
                .map(|m| m.sig.params.as_slice())
        }
        Some(o) if o.kind == SyntaxKind::TraitDecl => {
            let name_span = wolf_ast::TraitDecl::cast(o)?.name()?.span;
            let tname = text(pkg, body.file, name_span);
            let tr = wolf_sema::traits::TraitRef {
                module: body.module,
                name: tname,
            };
            sigs.traits
                .get(&tr)?
                .methods
                .iter()
                .find(|m| m.name == body.name)
                .map(|m| m.sig.params.as_slice())
        }
        _ => None,
    }
}

/// s233: for a method of a trait impl, the trait method's NAME span —
/// what a call dispatched through a bound names as its declaration.
fn trait_method_key(sigs: &SigTables, body: &BodyRef, outer: Option<&GreenNode>) -> Option<Span> {
    let o = outer?;
    if o.kind != SyntaxKind::ImplDecl {
        return None;
    }
    let imp = sigs
        .impls
        .iter()
        .find(|i| i.file == body.file && i.decl == body.decl)?;
    let tr = imp.trait_ref.as_ref()?;
    sigs.traits
        .get(tr)?
        .methods
        .iter()
        .find(|m| m.name == body.name)
        .map(|m| m.name_span)
}

fn text(pkg: &Package, file: usize, span: Span) -> String {
    let src = &pkg.files[file].raw.src;
    String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned()
}

// ------------------------------------- the tier boundary (s22, E1302) ----

/// Does the type mention a raw pointer anywhere (through wrappers,
/// aggregates, rows, fn types)?
fn contains_ptr(table: &TypeTable, id: TyId, depth: u32) -> bool {
    if depth > 32 {
        return false;
    }
    match table.kind(id) {
        TyKind::Ptr(_) => true,
        TyKind::Wrapping(t)
        | TyKind::Distinct(t)
        | TyKind::Range(t)
        | TyKind::Shared(t)
        | TyKind::Weak(t)
        | TyKind::Handle(t)
        | TyKind::List(t)
        | TyKind::Pool(t) => contains_ptr(table, *t, depth + 1),
        TyKind::Map(k, v) => {
            contains_ptr(table, *k, depth + 1) || contains_ptr(table, *v, depth + 1)
        }
        TyKind::Tuple(elems) => elems.iter().any(|&t| contains_ptr(table, t, depth + 1)),
        TyKind::Fn(params, ret) => params
            .iter()
            .chain(std::iter::once(ret))
            .any(|&t| contains_ptr(table, t, depth + 1)),
        TyKind::ErrUnion(ok, row) => {
            contains_ptr(table, *ok, depth + 1) || contains_ptr(table, *row, depth + 1)
        }
        TyKind::Row { tags, .. } => tags
            .iter()
            .flat_map(|(_, payload)| payload.iter())
            .any(|&t| contains_ptr(table, t, depth + 1)),
        _ => false,
    }
}

fn e1302(span: Span, what: &str) -> Diagnostic {
    Diagnostic::error(
        codes::E1302,
        span,
        format!("{what} carries a raw pointer, but this boundary stays fully safe"),
    )
    .with_label("`*T` crosses the boundary here")
    .with_note(
        "unsafety never appears in types crossing a module's boundary — there are \
         no `unsafe fn`s; the proof lives at the `unsafe` block and the module is \
         the audit granule. A `*T` may stand in a module-private fn's signature or \
         at the C membrane (`export`/`extern \"c\"`); elsewhere pass a `handle` \
         (revalidated per access) or a region value, or keep the `*T` in a \
         module-private field.",
    )
}

/// The s22 boundary rule ([mem.unsafe.scope]), as K9(b) = B narrowed
/// it (`[mem.unsafe.sig]`; STATUS #31 and #32 R1, kw02 — wolf-lang#514):
/// a `*T` may appear in the signature of a **module-private** fn item
/// (every caller is inside the module, the audit granule) and of a
/// fn at the **C membrane** (`export fn`, `extern "c" fn`, either
/// side — there the pointer is C's anyway). Everywhere else — a `pub`
/// or `pub(pkg)` fn, every impl method and trait method — the
/// signature is fully safe (E1302). No *exported* type (struct field,
/// enum payload, global) mentions `*T`. Module-private data may hold
/// raw pointers: the module is the audit granule, and allocator
/// internals need a home.
fn unsafe_sig_check(pkg: &Package, sigs: &SigTables, diags: &mut Vec<Diagnostic>) {
    let ptr = |id: TyId| contains_ptr(&sigs.table, id, 0);
    let check_fn = |name: &str, f: &wolf_sema::sig::FnSig, diags: &mut Vec<Diagnostic>| {
        for p in &f.params {
            if ptr(p.ty) {
                diags.push(e1302(p.span, &format!("`{name}`'s parameter `{}`", p.name)));
            }
        }
        if ptr(f.ret) {
            diags.push(e1302(
                f.ret_span.unwrap_or(f.name_span),
                &format!("`{name}`'s return type"),
            ));
        }
    };
    for (m, module) in sigs.modules.iter().enumerate() {
        for (name, sig) in module {
            let exported = pkg
                .tables
                .get(m)
                .and_then(|t| t.get(name))
                .is_some_and(|i| i.vis != wolf_sema::Vis::Private);
            match sig {
                // `[mem.unsafe.sig]`: the two places a `*T` may stand.
                ItemSig::Fn(f) if f.membrane.is_some() || !exported => {}
                ItemSig::Fn(f) => check_fn(name, f, diags),
                ItemSig::Struct(ss) if exported => {
                    for f in &ss.fields {
                        if ptr(f.ty) {
                            diags.push(e1302(
                                f.span,
                                &format!("the exported type `{name}`'s field `{}`", f.name),
                            ));
                        }
                    }
                }
                ItemSig::Enum { variants, .. } if exported => {
                    for v in variants {
                        if v.payload.iter().any(|&t| ptr(t)) {
                            diags.push(e1302(
                                v.span,
                                &format!("the exported enum `{name}`'s variant `{}`", v.name),
                            ));
                        }
                    }
                }
                ItemSig::Global(g) if exported => {
                    if let Some(t) = g.ty
                        && ptr(t)
                    {
                        diags.push(e1302(g.name_span, &format!("the exported item `{name}`")));
                    }
                }
                _ => {}
            }
        }
    }
    for imp in &sigs.impls {
        for m in &imp.methods {
            check_fn(&m.name, &m.sig, diags);
        }
    }
    for td in sigs.traits.values() {
        for m in &td.methods {
            check_fn(&m.name, &m.sig, diags);
        }
    }
}
