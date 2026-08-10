//! Maybe-uninitialized dataflow (s18 Target 2): use-after-move and
//! use-of-uninit, field-granular, with move-site provenance
//! (`[mem.tier0.move]`, E1001).
//!
//! Forward may-analysis over the check CFG. The state is the set of
//! *maybe moved-out* places, each carrying the span that emptied it.
//! Joins union (a place moved on any path in is maybe-moved), the
//! transfer applies the statement's effect, and reporting runs as one
//! deterministic sweep after the fixpoint.
//!
//! Partial moves are the point: `let x = s.a` poisons `s.a` (and every
//! path under it, and any whole-`s` use) while `s.b` stays live —
//! `[mem.model.path.disjoint]` decides every conflict. Re-initializing
//! a place revives it (`[mem.tier0.move.4]`); re-initializing *part*
//! of a wholly-moved value expands the old move into per-field residue
//! over the interned place universe (the lowerer interned every
//! mentioned place's field siblings exactly for this).

use std::collections::{BTreeMap, HashSet};

use wolf_diag::{Applicability, Diagnostic, Suggestion, codes};
use wolf_span::Span;

use crate::cfg::{Cfg, Stmt};
use crate::place::PlaceId;

/// Why a place is empty: moved at the span, or declared without a
/// value there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Emptied {
    span: Span,
    uninit_decl: bool,
}

type State = BTreeMap<PlaceId, Emptied>;

fn join(into: &mut State, from: &State) -> bool {
    let mut changed = false;
    for (&place, &why) in from {
        match into.get(&place) {
            None => {
                into.insert(place, why);
                changed = true;
            }
            Some(&existing) => {
                // Deterministic provenance at merges: the earliest
                // span wins.
                if (why.span.lo, why.span.hi) < (existing.span.lo, existing.span.hi) {
                    into.insert(place, why);
                    changed = true;
                }
            }
        }
    }
    changed
}

pub fn check(cfg: &Cfg, diags: &mut Vec<Diagnostic>) {
    // -------------------------------------------------- fixpoint ----
    let n = cfg.blocks.len();
    let mut entry: Vec<Option<State>> = vec![None; n];
    entry[cfg.entry.0 as usize] = Some(State::new());
    let mut work = vec![cfg.entry];
    while let Some(b) = work.pop() {
        let mut state = entry[b.0 as usize].clone().expect("on worklist ⇒ visited");
        for stmt in &cfg.block(b).stmts {
            transfer(cfg, stmt, &mut state, None);
        }
        for &succ in &cfg.block(b).succs {
            let slot = &mut entry[succ.0 as usize];
            let changed = match slot {
                None => {
                    *slot = Some(state.clone());
                    true
                }
                Some(existing) => join(existing, &state),
            };
            if changed && !work.contains(&succ) {
                work.push(succ);
            }
        }
    }

    // ---------------------------------------------- report sweep ----
    let mut seen: HashSet<(Span, PlaceId, Span)> = HashSet::new();
    for (i, block) in cfg.blocks.iter().enumerate() {
        let Some(state) = &entry[i] else {
            continue; // unreachable: nothing to check
        };
        let mut state = state.clone();
        for stmt in &block.stmts {
            let mut sink = |use_span: Span, place: PlaceId, why: Emptied, moved: PlaceId| {
                if !seen.insert((use_span, place, why.span)) {
                    return;
                }
                diags.push(report(cfg, use_span, place, why, moved));
            };
            transfer(cfg, stmt, &mut state, Some(&mut sink));
        }
    }
}

/// Apply one statement. When `report` is given, uses of maybe-moved
/// places are surfaced (the post-fixpoint sweep).
fn transfer(
    cfg: &Cfg,
    stmt: &Stmt,
    state: &mut State,
    mut report: Option<&mut dyn FnMut(Span, PlaceId, Emptied, PlaceId)>,
) {
    let mut check_use = |state: &State, place: PlaceId, span: Span| {
        if let Some(sink) = report.as_deref_mut() {
            for (&moved, &why) in state.iter() {
                if cfg.places.overlap(moved, place) {
                    sink(span, place, why, moved);
                    return; // one report per use
                }
            }
        }
    };
    match stmt {
        Stmt::Read { place, span } | Stmt::Mutate { place, span } => {
            check_use(state, *place, *span);
        }
        Stmt::Move { place, span } => {
            check_use(state, *place, *span);
            state.insert(
                *place,
                Emptied {
                    span: *span,
                    uninit_decl: false,
                },
            );
        }
        Stmt::Uninit { place, span } => {
            state.insert(
                *place,
                Emptied {
                    span: *span,
                    uninit_decl: true,
                },
            );
        }
        Stmt::Init { place, span: _ } => {
            let init = *place;
            // Revive everything the initialization covers…
            let revived: Vec<PlaceId> = state
                .keys()
                .copied()
                .filter(|&m| cfg.places.covers(init, m))
                .collect();
            for m in revived {
                state.remove(&m);
            }
            // …and expand any move that strictly contains it into the
            // untouched residue: `move p` then `p.a = …` leaves `p.b`
            // (and whole-`p` uses) moved.
            let containing: Vec<(PlaceId, Emptied)> = state
                .iter()
                .filter(|&(&m, _)| cfg.places.covers(m, init) && m != init)
                .map(|(&m, &w)| (m, w))
                .collect();
            for (m, why) in containing {
                state.remove(&m);
                let residue: Vec<PlaceId> = cfg
                    .places
                    .iter()
                    .map(|(id, _)| id)
                    .filter(|&q| cfg.places.covers(m, q) && !cfg.places.overlap(init, q))
                    .collect();
                for q in residue {
                    state.entry(q).or_insert(why);
                }
            }
        }
        Stmt::Call(c) => {
            for &(place, span) in c.mut_args.iter().chain(c.read_args.iter()) {
                check_use(state, place, span);
            }
            // `take` arguments were moved by their evaluation-order
            // `Move` statements.
        }
        Stmt::Borrow { loan, span } => {
            check_use(state, cfg.loans[loan.0 as usize].place, *span);
        }
        Stmt::UseBorrower { .. }
        | Stmt::Activate { .. }
        | Stmt::CheckedOp { .. }
        | Stmt::Alloc { .. }
        | Stmt::RegionOpen { .. }
        | Stmt::RegionClose { .. } => {}
    }
}

fn report(cfg: &Cfg, use_span: Span, place: PlaceId, why: Emptied, moved: PlaceId) -> Diagnostic {
    let shown = cfg.show_place(place);
    let shown_moved = cfg.show_place(moved);
    if why.uninit_decl {
        return Diagnostic::error(
            codes::E1001,
            use_span,
            format!("`{shown}` is used here but has no value yet"),
        )
        .with_label("used while uninitialized")
        .with_secondary(
            why.span,
            format!("`{shown_moved}` is declared without a value here"),
        )
        .with_note("assign to it first; a place becomes usable the moment it holds a value.");
    }
    let mut d = Diagnostic::error(
        codes::E1001,
        use_span,
        format!("`{shown}` is used here after its value moved away"),
    )
    .with_label("used after the move")
    .with_secondary(why.span, format!("`{shown_moved}` moved here"));
    if place != moved {
        let both = if cfg.places.covers(moved, place) {
            format!("`{shown}` is part of `{shown_moved}`")
        } else {
            format!("`{shown_moved}` is part of `{shown}`")
        };
        d = d.with_note(format!(
            "{both}; moving one empties the other. Disjoint fields stay usable."
        ));
    }
    d.with_suggestion(Suggestion::new(
        "to keep the original, copy it at the move".to_string(),
        vec![(
            Span::new(why.span.file, why.span.lo, why.span.lo),
            "copy ".to_string(),
        )],
        Applicability::Maybe,
    ))
    .with_note("re-initializing the place (assigning to it) also makes it usable again.")
}
