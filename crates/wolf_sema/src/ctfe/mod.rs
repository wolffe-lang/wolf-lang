//! The comptime/CTFE engine (s16, D29/D33).
//!
//! Structure:
//! - [`value`] — the interned, content-hashable value arena (scalars,
//!   aggregates, comptime slices, functions, types);
//! - [`eval`] — the tree-walking machine over typed HIR: explicit
//!   value stack + environment frames, memoized per (function,
//!   argument hash), budgeted (fuel/heap/depth — never disableable);
//! - [`intrinsics`] — the explicit comptime allowlist and the D33
//!   sandbox tables (every ambient surface refused *with its reason*);
//! - [`norm`] — const-generic ring normalization (the `N+1 ≡ 1+N`
//!   fix, with the witness line drawn exactly where documented);
//! - [`derive`] — the reflection derive exemplar whose generated
//!   impls pass ordinary s14 checking, with staged diagnostics.
//!
//! This module also owns the package-level comptime pass
//! ([`run_package`]): after bodies check, every registered comptime
//! call site evaluates under its budget, and faults become catalog
//! diagnostics (E07xx) with comptime backtraces.

pub mod derive;
pub mod eval;
pub mod intrinsics;
pub mod norm;
pub mod value;

use std::collections::HashSet;

use wolf_ast::{AttrItem, Attribute, GreenNode, SyntaxKind, is_stmt_kind};
use wolf_diag::{Applicability, Diagnostic, Suggestion, codes};
use wolf_span::Span;

use crate::check::{BodyResult, NotYet};
use crate::graph::Package;
use crate::sig::SigTables;
use crate::typecheck::BodyOutcome;

pub use eval::{Budget, CtfeStats, Engine, Fault, FaultKind};
pub use intrinsics::SandboxCategory;
pub use value::{CtType, ValId, ValueArena, ValueKind};

/// The result of the package-level comptime pass.
#[derive(Debug, Default)]
pub struct CtfePass {
    pub diagnostics: Vec<Diagnostic>,
    /// Honest engine gaps: the construct is fine, the s16 evaluator
    /// subset does not cover it — the body's rung retreats, exactly
    /// like a checker refusal.
    pub not_yet: Vec<NotYet>,
    pub stats: CtfeStats,
}

/// Evaluate every registered comptime call site in the package.
/// Deterministic: outcomes arrive in body order, sites in visit order.
pub fn run_package(pkg: &Package, sigs: &SigTables, outcomes: &[BodyOutcome]) -> CtfePass {
    let mut pass = CtfePass::default();
    let mut engine = Engine::new(pkg, sigs, outcomes);
    let mut seen: HashSet<(u32, u32, u32)> = HashSet::new();
    for o in outcomes {
        let BodyResult::Checked(tb) = &o.result else {
            continue;
        };
        if tb.comptime_calls.is_empty() {
            continue;
        }
        let Some(outer) = pkg.files[o.body.file]
            .parse
            .root
            .nodes()
            .filter(|n| n.kind.is_item())
            .nth(o.body.decl)
        else {
            continue;
        };
        let node = match o.body.member {
            None => outer,
            Some(mi) => {
                let Some(m) = outer.nodes().filter(|n| n.kind.is_item()).nth(mi) else {
                    continue;
                };
                m
            }
        };
        for &site_span in &tb.comptime_calls {
            if !seen.insert((site_span.file.index() as u32, site_span.lo, site_span.hi)) {
                continue;
            }
            let Some(site) = find_call(node, site_span) else {
                continue;
            };
            let (budget, fix_at, mut attr_diags) = budget_for(pkg, o.body.file, node, site_span);
            pass.diagnostics.append(&mut attr_diags);
            match engine.evaluate_site(&o.body, tb, node, site, budget) {
                Ok(_) => {}
                Err(f) => match f.kind {
                    FaultKind::EngineGap { construct } => pass.not_yet.push(NotYet {
                        construct,
                        span: f.span,
                    }),
                    _ => pass.diagnostics.push(fault_to_diag(&f, budget, fix_at)),
                },
            }
        }
    }
    pass.stats = engine.stats;
    pass
}

/// Locate the `CallExpr` node with exactly `span` under `node`.
fn find_call(node: &GreenNode, span: Span) -> Option<&GreenNode> {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind == SyntaxKind::CallExpr && n.span == span {
            return Some(n);
        }
        for c in n.nodes() {
            if c.span.lo <= span.lo && span.hi <= c.span.hi {
                stack.push(c);
            }
        }
    }
    None
}

// ------------------------------------------------------------ budgets ---

/// The budget of one call site: defaults, overridden by
/// `#[budget(fuel = …, heap = …, depth = …)]` on the innermost
/// enclosing statement (or the item), clamped to the hard ceilings —
/// never disableable (D33). Returns the budget, the fix-it insertion
/// point for raise-the-budget suggestions, and any E0709 diagnostics.
fn budget_for(
    pkg: &Package,
    file: usize,
    body_node: &GreenNode,
    site: Span,
) -> (Budget, Span, Vec<Diagnostic>) {
    // Innermost enclosing statement-or-item node.
    let mut host = body_node;
    let mut cursor = body_node;
    loop {
        let Some(inner) = cursor
            .nodes()
            .find(|c| c.span.lo <= site.lo && site.hi <= c.span.hi)
        else {
            break;
        };
        if is_stmt_kind(inner.kind) || inner.kind.is_item() {
            host = inner;
        }
        cursor = inner;
    }
    let fix_at = Span::new(site.file, host.span.lo, host.span.lo);
    let src = &pkg.files[file].raw.src;
    let text = |s: Span| -> String {
        String::from_utf8_lossy(&src[s.lo as usize..s.hi as usize]).into_owned()
    };
    let mut budget = Budget::default();
    let mut diags = Vec::new();
    // The host statement's attributes, else the item's.
    let attrs: Vec<Attribute<'_>> = host
        .nodes()
        .filter_map(Attribute::cast)
        .chain(body_node.nodes().filter_map(Attribute::cast))
        .collect();
    for attr in attrs {
        for item in attr.items() {
            let Some(path) = item.path() else { continue };
            let name = path
                .segments()
                .map(|t| text(t.span))
                .collect::<Vec<_>>()
                .join(".");
            if name != "budget" {
                continue;
            }
            let Some(input) = item.input() else { continue };
            for setting in input.nodes().filter_map(AttrItem::cast) {
                let Some(sp) = setting.path() else { continue };
                let key = sp
                    .segments()
                    .map(|t| text(t.span))
                    .collect::<Vec<_>>()
                    .join(".");
                let value = setting.input().and_then(|inp| {
                    inp.tokens()
                        .find(|t| t.kind == SyntaxKind::Int)
                        .and_then(|t| text(t.span).replace('_', "").parse::<u64>().ok())
                });
                let span = setting.syntax().span;
                let Some(v) = value else {
                    diags.push(
                        Diagnostic::error(
                            codes::E0709,
                            span,
                            format!("`{key}` needs an integer value, like `{key} = 1000000`"),
                        )
                        .with_label("no usable value here"),
                    );
                    continue;
                };
                if v == 0 {
                    diags.push(
                        Diagnostic::error(
                            codes::E0709,
                            span,
                            format!(
                                "a comptime budget cannot be turned off — `{key} = 0` would \
                                 disable the limit"
                            ),
                        )
                        .with_label("budgets are raised, never removed")
                        .with_note(
                            "the sandbox guarantee (D33) includes bounded evaluation: every \
                             budget has a default, a per-site override, and a hard ceiling — \
                             there is no spelling that removes one.",
                        ),
                    );
                    continue;
                }
                let (slot, max): (&mut u64, u64) = match key.as_str() {
                    "fuel" => (&mut budget.fuel, eval::MAX_FUEL),
                    "heap" => (&mut budget.heap, eval::MAX_HEAP),
                    "depth" => {
                        if v > u64::from(eval::MAX_DEPTH) {
                            diags.push(over_ceiling(span, &key, u64::from(eval::MAX_DEPTH)));
                            budget.depth = eval::MAX_DEPTH;
                        } else {
                            budget.depth = v as u32;
                        }
                        continue;
                    }
                    _ => {
                        diags.push(
                            Diagnostic::error(
                                codes::E0709,
                                span,
                                format!(
                                    "`{key}` is not a comptime budget — the budgets are \
                                     `fuel`, `heap`, and `depth`"
                                ),
                            )
                            .with_label("unknown budget"),
                        );
                        continue;
                    }
                };
                if v > max {
                    diags.push(over_ceiling(span, &key, max));
                    *slot = max;
                } else {
                    *slot = v;
                }
            }
        }
    }
    (budget, fix_at, diags)
}

fn over_ceiling(span: Span, key: &str, max: u64) -> Diagnostic {
    Diagnostic::error(
        codes::E0709,
        span,
        format!("`{key}` cannot exceed the hard ceiling of {max}"),
    )
    .with_label("above the ceiling")
    .with_note(
        "budgets bound comptime evaluation so a build can neither hang nor be \
         used as an attack surface; the ceiling itself is not negotiable (D33).",
    )
}

// -------------------------------------------------- fault → diagnostic --

/// Render a comptime fault as its catalog diagnostic, with the
/// comptime backtrace as ordered secondaries (innermost first) and a
/// raise-the-budget fix-it where one applies.
fn fault_to_diag(f: &Fault, _budget: Budget, fix_at: Span) -> Diagnostic {
    let mut d = match &f.kind {
        FaultKind::Sandbox { name, category } => Diagnostic::error(
            codes::E0701,
            f.span,
            format!(
                "`{name}` reaches {}, which comptime code can never touch",
                category.surface()
            ),
        )
        .with_label(match category {
            SandboxCategory::Ffi => "outside the sandbox",
            _ => "ambient IO at compile time",
        })
        .with_note(format!("why it is refused — {}.", category.reason()))
        .with_note(
            "the comptime sandbox is hermetic (D33): the intrinsics available at \
             compile time are an explicit allowlist, and nothing ambient is on it. \
             Compute this value at runtime instead; file contents will arrive later \
             as declared build inputs (s51), never as an evaluator capability.",
        ),
        FaultKind::Fuel { spent } => Diagnostic::error(
            codes::E0702,
            f.span,
            format!("comptime evaluation ran out of fuel after {spent} steps"),
        )
        .with_label("evaluation stopped here")
        .with_note(
            "fuel bounds how long the compiler will evaluate before concluding the \
             computation is runaway — a build can be slow, never hung (D33).",
        )
        .with_suggestion(raise_budget(fix_at, "fuel", spent.saturating_mul(2))),
        FaultKind::Heap { cells } => Diagnostic::error(
            codes::E0703,
            f.span,
            format!("comptime evaluation exceeded its heap budget of {cells} cells"),
        )
        .with_label("the allocation that went over happened here")
        .with_note(
            "the comptime heap is capped so evaluation cannot exhaust the machine \
             compiling the program (D33); most overruns are unbounded value growth \
             in a loop.",
        )
        .with_suggestion(raise_budget(fix_at, "heap", cells.saturating_mul(2))),
        FaultKind::Depth { frames } => Diagnostic::error(
            codes::E0704,
            f.span,
            format!("comptime evaluation recursed past {frames} call frames"),
        )
        .with_label("the call that went over the limit")
        .with_note(
            "call depth is a resource limit, not a host stack: deep recursion is \
             refused with this report instead of crashing the compiler (D33).",
        )
        .with_suggestion(raise_budget(fix_at, "depth", u64::from(*frames) * 2)),
        FaultKind::NotComptime { what } => Diagnostic::error(
            codes::E0705,
            f.span,
            format!("{what}, so this cannot evaluate at compile time"),
        )
        .with_label("must be comptime-known")
        .with_note(
            "a `comptime fn` runs during compilation: every argument must be a \
             literal, a `const`, a type, or the result of another comptime call.",
        ),
        FaultKind::Arith { op, ty, detail } => Diagnostic::error(
            codes::E0706,
            f.span,
            format!("this `{op}` on `{ty}` faults at compile time: {detail}"),
        )
        .with_label("checked arithmetic, comptime included")
        .with_note(
            "checked arithmetic has one semantics everywhere (X3): what would trap \
             at runtime is an error at comptime — intended wraparound is spelled \
             `wrapping[T]`, never a mode.",
        ),
        FaultKind::Layout { what } => Diagnostic::error(
            codes::E0708,
            f.span,
            format!("the size of {what} is not resolved until codegen lays it out"),
        )
        .with_label("unresolved until codegen (c05)")
        .with_note(
            "layout (sizes, offsets) is decided by the code generator, not the \
             type checker; comptime can answer for fixed-width primitives today \
             and for aggregates once c05 lands.",
        ),
        FaultKind::AssertFailed => {
            Diagnostic::error(codes::E0710, f.span, "this comptime assertion failed")
                .with_label("evaluated to `false` at compile time")
                .with_note(
                    "a failed comptime `assert` stops compilation — it is the \
                     witness mechanism for facts the checker cannot see on its own.",
                )
        }
        FaultKind::EngineGap { construct } => {
            // Callers route gaps to NotYet; this arm renders only if
            // one slips through (defensive, still honest).
            Diagnostic::error(
                codes::E0705,
                f.span,
                format!("{construct} is not comptime-evaluable yet"),
            )
        }
    };
    // The comptime backtrace: innermost frames first, consecutive
    // duplicates folded (recursion would otherwise print one line per
    // frame).
    let mut shown = 0usize;
    let mut i = 0usize;
    while i < f.frames.len() && shown < 8 {
        let fr = &f.frames[i];
        let mut dup = 0usize;
        while i + dup + 1 < f.frames.len() {
            let next = &f.frames[i + dup + 1];
            if next.name == fr.name && next.call_span == fr.call_span {
                dup += 1;
            } else {
                break;
            }
        }
        let label = if dup > 0 {
            format!(
                "while evaluating `{}` — {} recursive frames",
                fr.name,
                dup + 1
            )
        } else {
            format!("while evaluating `{}`, entered here", fr.name)
        };
        d = d.with_secondary(fr.call_span, label);
        i += dup + 1;
        shown += 1;
    }
    if i < f.frames.len() {
        d = d.with_note(format!(
            "…and {} more comptime frames below these.",
            f.frames.len() - i
        ));
    }
    d
}

fn raise_budget(fix_at: Span, key: &str, to: u64) -> Suggestion {
    Suggestion::new(
        format!("raise the budget here: `#[budget({key} = {to})]`"),
        vec![(fix_at, format!("#[budget({key} = {to})]\n"))],
        Applicability::Maybe,
    )
}
