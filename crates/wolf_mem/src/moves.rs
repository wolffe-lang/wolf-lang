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
//!
//! Elements (`[mem.model.place.elem]`, eg01): a use conflicts with a
//! moved place when the two MAY share storage (`overlap_elem`: two
//! different literal indices never do); a store revives a moved place
//! only when it SURELY re-initializes all of it (`covers_must`, item 3
//! — before eg01 any index store revived the collapsed element,
//! wolf-lang#460). R3: `xs[i]` surely denotes the element a `move
//! xs[i]` emptied while `i` is unwritten since the move; any write to
//! `i` blurs every moved place spelled through it, and a local that is
//! ever borrowed — or any raw-tier statement in the body — never
//! qualifies.
//!
//! Returns (`[mem.tier0.mode.mut]`, s184, wolf-lang#464): the state
//! reaching the exit is what every caller gets back, so a place under
//! a `mut` parameter still maybe-moved there is E1001 at its move — the
//! caller's place would otherwise name the moved value.

use std::collections::{BTreeMap, HashSet};

use wolf_diag::{Applicability, Diagnostic, Suggestion, codes};
use wolf_span::Span;

use crate::cfg::{Cfg, Stmt};
use crate::place::{Base, PlaceId, Proj};

/// Why a place is empty: moved at the span, or declared without a
/// value there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Emptied {
    span: Span,
    uninit_decl: bool,
    /// R3: a local this place is indexed by (`xs[i]`) was written since
    /// the place emptied, so its `Sym` steps no longer name the element
    /// that moved — the place can be revived only through a literal or
    /// a store to a prefix without them.
    blurred: bool,
}

impl Emptied {
    fn at(span: Span, uninit_decl: bool) -> Self {
        Emptied {
            span,
            uninit_decl,
            blurred: false,
        }
    }
}

/// Which locals can never carry R3's proof: every local some loan is
/// taken on (a write through the borrower is not a statement on the
/// local), and — when the body has any raw-tier statement — all of
/// them.
struct SymRule {
    loaned: HashSet<u32>,
    raw: bool,
}

impl SymRule {
    fn new(cfg: &Cfg) -> Self {
        let loaned = cfg
            .loans
            .iter()
            .filter_map(|l| match cfg.places.get(l.place).base {
                Base::Local(b) => Some(b),
                Base::Global(..) => None,
            })
            .collect();
        let raw = cfg.blocks.iter().flat_map(|b| b.stmts.iter()).any(|s| {
            matches!(
                s,
                Stmt::UnsafeEnter { .. }
                    | Stmt::RawRead { .. }
                    | Stmt::RawWrite { .. }
                    | Stmt::Assume { .. }
                    | Stmt::Expose { .. }
                    | Stmt::Door { .. }
                    | Stmt::ProvOp { .. }
            )
        });
        SymRule { loaned, raw }
    }

    /// May a `Sym` over `local` stand for the value it had when `why`
    /// emptied the place?
    fn holds(&self, local: u32, why: &Emptied) -> bool {
        !why.blurred && !self.raw && !self.loaned.contains(&local)
    }

    /// For two places both read NOW (a store's target and a place of
    /// the universe): only the standing exclusions apply.
    fn now(&self, local: u32) -> bool {
        !self.raw && !self.loaned.contains(&local)
    }
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
                // span wins. A blur on either path survives the merge
                // (R3 holds only if it holds on every path in).
                let mut merged = existing;
                if (why.span.lo, why.span.hi) < (existing.span.lo, existing.span.hi) {
                    merged.span = why.span;
                    merged.uninit_decl = why.uninit_decl;
                }
                merged.blurred |= why.blurred;
                if merged != existing {
                    into.insert(place, merged);
                    changed = true;
                }
            }
        }
    }
    changed
}

pub fn check(cfg: &Cfg, diags: &mut Vec<Diagnostic>) {
    let rule = SymRule::new(cfg);
    // -------------------------------------------------- fixpoint ----
    let n = cfg.blocks.len();
    let mut entry: Vec<Option<State>> = vec![None; n];
    entry[cfg.entry.0 as usize] = Some(State::new());
    let mut work = vec![cfg.entry];
    while let Some(b) = work.pop() {
        let mut state = entry[b.0 as usize].clone().expect("on worklist ⇒ visited");
        for stmt in &cfg.block(b).stmts {
            transfer(cfg, &rule, stmt, &mut state, None);
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
            transfer(cfg, &rule, stmt, &mut state, Some(&mut sink));
        }
    }

    // ------------------------------------ `mut` parameters at return ----
    if let Some(state) = &entry[cfg.exit.0 as usize] {
        let used: HashSet<Span> = seen.iter().map(|&(_, _, moved_at)| moved_at).collect();
        at_return(cfg, state, &used, diags);
    }
}

/// `[mem.tier0.mode.mut]` (s184, wolf-lang#464): a `mut` parameter is
/// initialized at every return of the callee. Every return — a
/// `return`, the `?` error edge, the fall-through — joins `cfg.exit`
/// after its defers, so the state there is what the caller gets back:
/// a place under a `mut` parameter (the parameter itself, a field, an
/// element, a map value) that is still maybe-moved there is refused at
/// the move that emptied it. One report per parameter and move site,
/// and none for a move whose use in the body already drew E1001 (one
/// root cause, one diagnostic).
fn at_return(cfg: &Cfg, state: &State, used: &HashSet<Span>, diags: &mut Vec<Diagnostic>) {
    let mut seen: HashSet<(u32, Span)> = HashSet::new();
    for (&moved, &why) in state {
        let Base::Local(l) = cfg.places.get(moved).base else {
            continue;
        };
        if cfg.locals[l as usize].param_mode != Some(Some(wolf_ast::ParamMode::Mut))
            || why.uninit_decl
            || used.contains(&why.span)
            || !seen.insert((l, why.span))
        {
            continue;
        }
        diags.push(report_at_return(cfg, l, moved, why));
    }
}

fn report_at_return(cfg: &Cfg, param: u32, moved: PlaceId, why: Emptied) -> Diagnostic {
    let local = &cfg.locals[param as usize];
    let name = local.name.clone();
    let shown = cfg.show_place(moved);
    let d = Diagnostic::error(
        codes::E1001,
        why.span,
        format!("`{shown}` may return to the caller with its value moved away"),
    )
    .with_label("moved here, and not stored back on every path to a return")
    .with_secondary(
        local.span,
        format!("`{name}` is `mut`: the caller reads it again after the call"),
    )
    .with_note(format!(
        "a `mut` parameter is initialized at every return of the function \
         [mem.tier0.mode.mut]: store a value back into `{shown}` before each return."
    ));
    // No `copy` fix-it: copying at the move leaves the parameter whole
    // but drops the callee's change on the floor, which is rarely what
    // a `mut` parameter was for. The fix is the store back.
    // #325: the parameter this refusal names — W1002's "never written"
    // for the same name stands down beside it (the move is the write
    // its syntactic scan cannot see).
    d.about(name, why.span)
}

/// Apply one statement. When `report` is given, uses of maybe-moved
/// places are surfaced (the post-fixpoint sweep).
fn transfer(
    cfg: &Cfg,
    rule: &SymRule,
    stmt: &Stmt,
    state: &mut State,
    mut report: Option<&mut dyn FnMut(Span, PlaceId, Emptied, PlaceId)>,
) {
    let mut check_use = |state: &State, place: PlaceId, span: Span| {
        if let Some(sink) = report.as_deref_mut() {
            for (&moved, &why) in state.iter() {
                if cfg.places.overlap_elem(moved, place) {
                    sink(span, place, why, moved);
                    return; // one report per use
                }
            }
        }
    };
    match stmt {
        Stmt::Read { place, span } => {
            check_use(state, *place, *span);
        }
        Stmt::Move { place, span } => {
            check_use(state, *place, *span);
            state.insert(*place, Emptied::at(*span, false));
            blur(cfg, state, *place);
        }
        Stmt::Uninit { place, span } => {
            state.insert(*place, Emptied::at(*span, true));
            blur(cfg, state, *place);
        }
        Stmt::Init { place, span: _ } => {
            let init = *place;
            // Revive everything the initialization SURELY covers
            // (item 3: `xs[1] = v` revives a moved `xs[1]`, never a
            // moved `xs[0]`, and `xs[i] = v` a moved `xs[i]` only
            // while `i` is unwritten since the move — R3)…
            let revived: Vec<PlaceId> = state
                .iter()
                .filter(|&(&m, why)| cfg.places.covers_must(init, m, &|l| rule.holds(l, why)))
                .map(|(&m, _)| m)
                .collect();
            for m in revived {
                state.remove(&m);
            }
            // …and expand any move that surely contains it into the
            // untouched residue: `move p` then `p.a = …` leaves `p.b`
            // (and whole-`p` uses) moved. The residue is every place
            // that MAY lie inside the move, less what the store surely
            // re-initialized and the store's own ancestors (partly
            // live now; their other leaves are residue themselves). A
            // move that only MAY contain the store stays whole.
            let containing: Vec<(PlaceId, Emptied)> = state
                .iter()
                .filter(|&(&m, why)| {
                    m != init && cfg.places.covers_must(m, init, &|l| rule.holds(l, why))
                })
                .map(|(&m, &w)| (m, w))
                .collect();
            for (m, why) in containing {
                state.remove(&m);
                let now = |l: u32| rule.now(l);
                let residue: Vec<PlaceId> = cfg
                    .places
                    .iter()
                    .map(|(id, _)| id)
                    .filter(|&q| {
                        cfg.places.covers_may(m, q)
                            && !cfg.places.covers_must(init, q, &now)
                            && !cfg.places.covers_must(q, init, &now)
                    })
                    .collect();
                for q in residue {
                    state.entry(q).or_insert(why);
                }
            }
            blur(cfg, state, init);
        }
        Stmt::Mutate { place, span } => {
            check_use(state, *place, *span);
            blur(cfg, state, *place);
        }
        Stmt::Call(c) => {
            for &(place, span) in c.mut_args.iter().chain(c.read_args.iter()) {
                check_use(state, place, span);
            }
            // `take` arguments were moved by their evaluation-order
            // `Move` statements. A `mut` argument may write its place.
            for &(place, _) in &c.mut_args {
                blur(cfg, state, place);
            }
        }
        Stmt::Borrow { loan, span } => {
            let place = cfg.loans[loan.0 as usize].place;
            check_use(state, place, *span);
            blur(cfg, state, place);
        }
        // s21: `Dup` rides its `clone()` call's own receiver read;
        // `Drop` is conditional (drop-if-live — a moved-away local
        // must not re-report); `HandleCheck` guards the access that
        // carries its own `Read`. None are uses here.
        Stmt::UseBorrower { .. }
        | Stmt::Activate { .. }
        | Stmt::CheckedOp { .. }
        | Stmt::Alloc { .. }
        | Stmt::RegionOpen { .. }
        | Stmt::RegionClose { .. }
        | Stmt::Dup { .. }
        | Stmt::Drop { .. }
        | Stmt::HandleCheck { .. } => {}
        // s22 — raw-tier statements are attribution facts, not place
        // effects: raw memory has no move discipline (the pointer
        // value's own reads are separate `Read` statements).
        Stmt::UnsafeEnter { .. }
        | Stmt::UnsafeExit { .. }
        | Stmt::RawRead { .. }
        | Stmt::RawWrite { .. }
        | Stmt::Assume { .. }
        | Stmt::Expose { .. }
        | Stmt::Door { .. }
        | Stmt::ProvOp { .. } => {}
    }
}

/// R3's "not assigned between": a statement that may write `written`'s
/// base local blurs every maybe-moved place indexed by that local.
fn blur(cfg: &Cfg, state: &mut State, written: PlaceId) {
    let Base::Local(l) = cfg.places.get(written).base else {
        return;
    };
    for (&m, why) in state.iter_mut() {
        if !why.blurred && cfg.places.get(m).proj.contains(&Proj::Sym(l)) {
            why.blurred = true;
        }
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
        let note = if cfg.places.covers_must(moved, place, &|_| true) {
            format!(
                "`{shown}` is part of `{shown_moved}`; moving one empties the other. Disjoint \
                 fields stay usable."
            )
        } else if cfg.places.covers_must(place, moved, &|_| true) {
            format!(
                "`{shown_moved}` is part of `{shown}`; moving one empties the other. Disjoint \
                 fields stay usable."
            )
        } else {
            format!(
                "`{shown}` may reach the moved `{shown_moved}`: an index that is not a literal \
                 is one place with every other index of its container \
                 [mem.model.place.elem]. Distinct literal indices stay usable."
            )
        };
        d = d.with_note(note);
    }
    // A destructure's element move happens at a binding PATTERN, where
    // `copy` is not grammar (s128 #173) — the fix-it would not parse.
    if !cfg.pattern_moves.contains(&why.span) {
        d = d.with_suggestion(Suggestion::new(
            "to keep the original, copy it at the move".to_string(),
            vec![(
                Span::new(why.span.file, why.span.lo, why.span.lo),
                "copy ".to_string(),
            )],
            Applicability::Maybe,
        ));
    }
    d.with_note("re-initializing the place (assigning to it) also makes it usable again.")
}
