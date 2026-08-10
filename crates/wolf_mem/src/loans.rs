//! Local loan checking (s18 Target 5): per-function loan-set dataflow
//! — *loans, not lifetimes* (`[mem.tier0.borrow]`).
//!
//! A loan L = (place, kind, borrower, origin) is created wherever a
//! borrow occurs. Kill rules: (a) the borrowed place's base is
//! overwritten or moved, (b) the borrowing local's last use passes
//! (standard backward liveness over the CFG — NLL-grade, not lexical),
//! (c) call-extent loans end with their call (those never enter this
//! engine: they live and die inside one `Call` statement and are
//! checked pairwise by [`crate::excl`]).
//!
//! Forward dataflow: `LiveLoans(entry) = ⋃ LiveLoans(preds)`, gen/kill
//! per statement, bitsets indexed by loan id, iterate to fixpoint. A
//! loan conflicts only while it is *needed*: gen-reached **and** its
//! borrower still live — that conjunction is what accepts the NLL
//! problem cases (and the Polonius case #3, which in wolf cannot even
//! be written across a function boundary: borrows are second-class at
//! signatures, so the region-inference failure it exercises has no
//! surface here — see the unit tests' verdicts).
//!
//! Today's typeable surface produces no first-class borrows (`&x`
//! refuses in sema until the region campaign wires the surface), so
//! this engine is exercised by the unit tests below; the lowerer will
//! start emitting `Borrow`/`Activate`/`UseBorrower` statements without
//! any change here.
//!
//! Error conditions at each statement, for every live loan L:
//! - a write (`Init`/`Mutate`) or move of a place overlapping L.place
//!   — for a *reserved* two-phase loan, writes conflict but reads do
//!   not until activation;
//! - a read of a place overlapping a live **unique, activated**
//!   L.place;
//! - a new borrow overlapping L, unless both are shared.

use wolf_diag::{Diagnostic, codes};
use wolf_span::Span;

use crate::cfg::{Cfg, LoanKind, Stmt};
use crate::place::{Base, PlaceId};

type Bits = u64;
const MAX_LOANS: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
struct Flow {
    live: Bits,
    active: Bits,
}

pub fn check(cfg: &Cfg, diags: &mut Vec<Diagnostic>) {
    if cfg.loans.is_empty() {
        return;
    }
    assert!(
        cfg.loans.len() <= MAX_LOANS,
        "loan bitset width exceeded — widen to a bitvec"
    );

    // ---------------------- borrower liveness (backward, per block) --
    // A borrower is "used" by UseBorrower and by any Read/Move of a
    // place based on it.
    let n = cfg.blocks.len();
    let borrower_of_stmt = |stmt: &Stmt| -> Option<u32> {
        match stmt {
            Stmt::UseBorrower { local, .. } => Some(local.0),
            Stmt::Read { place, .. } | Stmt::Move { place, .. } => {
                match cfg.places.get(*place).base {
                    Base::Local(l) => Some(l),
                    Base::Global(..) => None,
                }
            }
            _ => None,
        }
    };
    let mut live_out: Vec<Bits> = vec![0; n]; // bit i = borrower of loan i live
    let borrower_bit = |local: u32| -> Bits {
        let mut bits = 0;
        for (i, loan) in cfg.loans.iter().enumerate() {
            if loan.borrower.0 == local {
                bits |= 1 << i;
            }
        }
        bits
    };
    loop {
        let mut changed = false;
        for b in (0..n).rev() {
            let mut out: Bits = 0;
            for &succ in &cfg.blocks[b].succs {
                out |= block_live_in(
                    cfg,
                    succ.0 as usize,
                    live_out[succ.0 as usize],
                    &borrower_bit,
                    &borrower_of_stmt,
                );
            }
            if out != live_out[b] {
                live_out[b] = out;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // ------------------------------------ loan dataflow (forward) ----
    let mut entry: Vec<Option<Flow>> = vec![None; n];
    entry[cfg.entry.0 as usize] = Some(Flow::default());
    let mut work = vec![cfg.entry.0 as usize];
    while let Some(b) = work.pop() {
        let mut flow = entry[b].expect("visited");
        for stmt in &cfg.blocks[b].stmts {
            transfer(cfg, stmt, &mut flow, None, 0);
        }
        for &succ in &cfg.blocks[b].succs {
            let s = succ.0 as usize;
            let joined = match entry[s] {
                None => flow,
                Some(prev) => Flow {
                    live: prev.live | flow.live,
                    active: prev.active | flow.active,
                },
            };
            if entry[s] != Some(joined) {
                entry[s] = Some(joined);
                if !work.contains(&s) {
                    work.push(s);
                }
            }
        }
    }

    // ----------------------------------------------- report sweep ----
    for b in 0..n {
        let Some(mut flow) = entry[b] else { continue };
        // Per-statement borrower-liveness-after, computed backward.
        let stmts = &cfg.blocks[b].stmts;
        let mut live_after: Vec<Bits> = vec![0; stmts.len()];
        let mut live = live_out[b];
        for (i, stmt) in stmts.iter().enumerate().rev() {
            live_after[i] = live;
            if let Some(local) = borrower_of_stmt(stmt) {
                live |= borrower_bit(local);
            }
        }
        for (i, stmt) in stmts.iter().enumerate() {
            let mut sink = |span: Span, msg: String, loan: usize| {
                let origin = cfg.loans[loan].origin;
                diags.push(
                    Diagnostic::error(codes::E1002, span, msg)
                        .with_label("conflicting access")
                        .with_secondary(origin, "the value is lent here")
                        .with_note(
                            "the loan is still needed by a later use of the borrowing \
                             name; once its last use passes, the place is free again.",
                        ),
                );
            };
            transfer(cfg, stmt, &mut flow, Some(&mut sink), live_after[i]);
        }
    }
}

fn block_live_in(
    cfg: &Cfg,
    b: usize,
    live_out: Bits,
    borrower_bit: &dyn Fn(u32) -> Bits,
    borrower_of_stmt: &dyn Fn(&Stmt) -> Option<u32>,
) -> Bits {
    let mut live = live_out;
    for stmt in cfg.blocks[b].stmts.iter().rev() {
        if let Some(local) = borrower_of_stmt(stmt) {
            live |= borrower_bit(local);
        }
    }
    live
}

/// One statement's gen/kill (+ optional conflict reporting). A loan
/// participates in conflicts only when gen-reached, its borrower is
/// live after the statement (`borrower_live`), and — for reads — it
/// is activated.
fn transfer(
    cfg: &Cfg,
    stmt: &Stmt,
    flow: &mut Flow,
    mut report: Option<&mut dyn FnMut(Span, String, usize)>,
    borrower_live: Bits,
) {
    let needed = flow.live & borrower_live;
    let conflicts = |place: PlaceId,
                     span: Span,
                     write: bool,
                     report: &mut Option<&mut dyn FnMut(Span, String, usize)>| {
        let Some(sink) = report.as_deref_mut() else {
            return;
        };
        for (i, loan) in cfg.loans.iter().enumerate() {
            if needed & (1 << i) == 0 {
                continue;
            }
            if !cfg.places.overlap(loan.place, place) {
                continue;
            }
            let activated = flow.active & (1 << i) != 0;
            let unique = loan.kind == LoanKind::Unique;
            let hit = if write {
                // Writes conflict with every loan — including a
                // reserved (two-phase) unique loan.
                true
            } else {
                // Reads conflict only with an *activated* unique
                // loan (the two-phase window keeps reads legal).
                unique && activated
            };
            if hit {
                let what = cfg.show_place(place);
                let lent = cfg.show_place(loan.place);
                let verb = if write { "changed" } else { "read" };
                sink(
                    span,
                    format!("`{what}` cannot be {verb} while `{lent}` is lent out"),
                    i,
                );
            }
        }
    };
    match stmt {
        Stmt::Read { place, span } => conflicts(*place, *span, false, &mut report),
        Stmt::Mutate { place, span } => {
            conflicts(*place, *span, true, &mut report);
            kill_overlapping(cfg, flow, *place);
        }
        Stmt::Move { place, span } | Stmt::Init { place, span } => {
            conflicts(*place, *span, true, &mut report);
            kill_overlapping(cfg, flow, *place);
        }
        Stmt::Call(c) => {
            for &(p, span) in &c.mut_args {
                conflicts(p, span, true, &mut report);
            }
            for &(p, span) in &c.read_args {
                conflicts(p, span, false, &mut report);
            }
            for &(p, span) in &c.take_args {
                conflicts(p, span, true, &mut report);
            }
        }
        Stmt::Borrow { loan, span } => {
            let new = &cfg.loans[loan.0 as usize];
            if let Some(sink) = &mut report {
                for (i, old) in cfg.loans.iter().enumerate() {
                    if needed & (1 << i) == 0 || i == loan.0 as usize {
                        continue;
                    }
                    if !cfg.places.overlap(old.place, new.place) {
                        continue;
                    }
                    let both_shared = old.kind == LoanKind::Shared && new.kind == LoanKind::Shared;
                    let old_reserved = old.two_phase && flow.active & (1 << i) == 0;
                    if !both_shared && !(old_reserved && new.kind == LoanKind::Shared) {
                        let lent = cfg.show_place(old.place);
                        sink(
                            *span,
                            format!(
                                "`{}` is already lent out via `{lent}`",
                                cfg.show_place(new.place)
                            ),
                            i,
                        );
                    }
                }
            }
            flow.live |= 1 << loan.0;
            if !new.two_phase {
                flow.active |= 1 << loan.0;
            }
        }
        Stmt::Activate { loan, .. } => {
            flow.active |= 1 << loan.0;
        }
        // s21 RC/handle bookkeeping statements are not accesses the
        // loan sets constrain (their guarded access carries its own
        // Read/Mutate).
        Stmt::Dup { .. }
        | Stmt::Drop { .. }
        | Stmt::HandleCheck { .. }
        | Stmt::Uninit { .. }
        | Stmt::UseBorrower { .. }
        | Stmt::CheckedOp { .. }
        | Stmt::Alloc { .. }
        | Stmt::RegionOpen { .. }
        | Stmt::RegionClose { .. } => {}
    }
}

/// Kill rule (a): overwriting or moving a place kills the loans it
/// overlaps.
fn kill_overlapping(cfg: &Cfg, flow: &mut Flow, place: PlaceId) {
    for (i, loan) in cfg.loans.iter().enumerate() {
        if cfg.places.overlap(loan.place, place) {
            flow.live &= !(1 << i);
            flow.active &= !(1 << i);
        }
    }
}

// ---------------------------------------------------------------------
// The dataflow engine's own tests: hand-built CFGs standing in for the
// borrow surface until it types. RFC 2094's named problem cases are
// encoded directly; the corpus takes over when `&` lands.
// ---------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{Block, BlockId, Cfg, Loan, Local, LocalId, Stmt};
    use crate::place::{Base, Place, PlaceTable};

    fn span(lo: u32, hi: u32) -> Span {
        let mut sm = wolf_span::SourceMap::new();
        let file = sm.intern(std::path::Path::new("loans-test.lu"));
        Span::new(file, lo, hi)
    }

    struct Builder {
        cfg: Cfg,
    }

    impl Builder {
        fn new(locals: &[&str]) -> Builder {
            let mut places = PlaceTable::new();
            for (i, _) in locals.iter().enumerate() {
                places.intern(
                    Place {
                        base: Base::Local(i as u32),
                        proj: Vec::new(),
                    },
                    false,
                );
            }
            Builder {
                cfg: Cfg {
                    name: "test".to_string(),
                    blocks: vec![Block::default()],
                    locals: locals
                        .iter()
                        .map(|n| Local {
                            name: n.to_string(),
                            span: span(0, 0),
                            ty: "?".to_string(),
                            is_copy: false,
                            param_mode: None,
                        })
                        .collect(),
                    places,
                    loans: Vec::new(),
                    regions: Vec::new(),
                    sites: Vec::new(),
                    entry: BlockId(0),
                    exit: BlockId(0),
                },
            }
        }

        fn place(&mut self, local: u32) -> PlaceId {
            self.cfg.places.intern(
                Place {
                    base: Base::Local(local),
                    proj: Vec::new(),
                },
                false,
            )
        }

        fn block(&mut self) -> BlockId {
            self.cfg.blocks.push(Block::default());
            BlockId(self.cfg.blocks.len() as u32 - 1)
        }

        fn edge(&mut self, from: BlockId, to: BlockId) {
            self.cfg.blocks[from.0 as usize].succs.push(to);
        }

        fn stmt(&mut self, b: BlockId, s: Stmt) {
            self.cfg.blocks[b.0 as usize].stmts.push(s);
        }

        fn loan(&mut self, place: PlaceId, kind: LoanKind, borrower: u32, two_phase: bool) -> u32 {
            self.cfg.loans.push(Loan {
                place,
                kind,
                borrower: LocalId(borrower),
                origin: span(1, 2),
                two_phase,
            });
            self.cfg.loans.len() as u32 - 1
        }

        fn run(&self) -> Vec<Diagnostic> {
            let mut diags = Vec::new();
            check(&self.cfg, &mut diags);
            diags
        }
    }

    /// NLL problem case #1 (RFC 2094): the borrow's last use passes
    /// before the mutation — loans die at last use, not scope exit.
    #[test]
    fn nll_case_1_last_use_before_mutation() {
        // let p = &v; use(p); v = …;   — legal
        let mut b = Builder::new(&["v", "p"]);
        let v = b.place(0);
        let l = b.loan(v, LoanKind::Unique, 1, false);
        let b0 = BlockId(0);
        b.stmt(
            b0,
            Stmt::Borrow {
                loan: crate::cfg::LoanId(l),
                span: span(1, 2),
            },
        );
        b.stmt(
            b0,
            Stmt::UseBorrower {
                local: LocalId(1),
                span: span(3, 4),
            },
        );
        b.stmt(
            b0,
            Stmt::Init {
                place: v,
                span: span(5, 6),
            },
        );
        assert!(b.run().is_empty(), "loan dead after last use");
    }

    /// The rejecting counterpart: the borrower is used *after* the
    /// mutation, so the write conflicts.
    #[test]
    fn write_while_loan_needed_rejected() {
        let mut b = Builder::new(&["v", "p"]);
        let v = b.place(0);
        let l = b.loan(v, LoanKind::Unique, 1, false);
        let b0 = BlockId(0);
        b.stmt(
            b0,
            Stmt::Borrow {
                loan: crate::cfg::LoanId(l),
                span: span(1, 2),
            },
        );
        b.stmt(
            b0,
            Stmt::Init {
                place: v,
                span: span(3, 4),
            },
        );
        b.stmt(
            b0,
            Stmt::UseBorrower {
                local: LocalId(1),
                span: span(5, 6),
            },
        );
        let diags = b.run();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, codes::E1002);
    }

    /// NLL problem case #2 (RFC 2094): a loan created and used in one
    /// branch does not poison the other branch.
    #[test]
    fn nll_case_2_branch_local_loan() {
        // if c { let p = &v; use(p); } else { v = …; }
        let mut b = Builder::new(&["v", "p"]);
        let v = b.place(0);
        let l = b.loan(v, LoanKind::Unique, 1, false);
        let b0 = BlockId(0);
        let then_b = b.block();
        let else_b = b.block();
        let join = b.block();
        b.edge(b0, then_b);
        b.edge(b0, else_b);
        b.edge(then_b, join);
        b.edge(else_b, join);
        b.stmt(
            then_b,
            Stmt::Borrow {
                loan: crate::cfg::LoanId(l),
                span: span(1, 2),
            },
        );
        b.stmt(
            then_b,
            Stmt::UseBorrower {
                local: LocalId(1),
                span: span(3, 4),
            },
        );
        b.stmt(
            else_b,
            Stmt::Init {
                place: v,
                span: span(5, 6),
            },
        );
        assert!(b.run().is_empty(), "the else branch never sees the loan");
    }

    /// NLL problem case #3 (RFC 2094's appendix; the Polonius case):
    /// the loan escapes only on the early-return path, so the mutation
    /// on the fall-through path is legal. Wolf's verdict: **accepted**
    /// — we compute loans, not outlives-constraints, so the loan is
    /// simply not live where the mutation happens. (The full
    /// interprocedural shape — returning the borrow — cannot exist in
    /// wolf at all: no reference crosses a signature, E1003's clause.)
    #[test]
    fn nll_case_3_polonius_conditional_escape_accepted() {
        // if c { use(p); return } v = …;
        let mut b = Builder::new(&["v", "p"]);
        let v = b.place(0);
        let l = b.loan(v, LoanKind::Unique, 1, false);
        let b0 = BlockId(0);
        let escape = b.block();
        let fall = b.block();
        b.edge(b0, escape);
        b.edge(b0, fall);
        b.stmt(
            b0,
            Stmt::Borrow {
                loan: crate::cfg::LoanId(l),
                span: span(1, 2),
            },
        );
        b.stmt(
            escape,
            Stmt::UseBorrower {
                local: LocalId(1),
                span: span(3, 4),
            },
        );
        b.stmt(
            fall,
            Stmt::Init {
                place: v,
                span: span(5, 6),
            },
        );
        assert!(
            b.run().is_empty(),
            "loans-not-lifetimes accepts the Polonius case"
        );
    }

    /// Two-phase: a reserved unique loan tolerates reads until its
    /// activation, and conflicts with them after.
    #[test]
    fn two_phase_reserved_reads_ok_active_reads_conflict() {
        let mut b = Builder::new(&["v", "p"]);
        let v = b.place(0);
        let l = b.loan(v, LoanKind::Unique, 1, true);
        let b0 = BlockId(0);
        b.stmt(
            b0,
            Stmt::Borrow {
                loan: crate::cfg::LoanId(l),
                span: span(1, 2),
            },
        );
        b.stmt(
            b0,
            Stmt::Read {
                place: v,
                span: span(3, 4),
            },
        ); // reserved: ok
        b.stmt(
            b0,
            Stmt::Activate {
                loan: crate::cfg::LoanId(l),
                span: span(5, 6),
            },
        );
        b.stmt(
            b0,
            Stmt::Read {
                place: v,
                span: span(7, 8),
            },
        ); // active: conflict
        b.stmt(
            b0,
            Stmt::UseBorrower {
                local: LocalId(1),
                span: span(9, 10),
            },
        );
        let diags = b.run();
        assert_eq!(diags.len(), 1, "only the post-activation read conflicts");
        assert_eq!(diags[0].span().lo, 7);
    }

    /// Kill rule (a): overwriting the base ends the loan.
    #[test]
    fn overwrite_kills_loan() {
        let mut b = Builder::new(&["v", "p"]);
        let v = b.place(0);
        let l = b.loan(v, LoanKind::Unique, 1, false);
        let b0 = BlockId(0);
        b.stmt(
            b0,
            Stmt::Borrow {
                loan: crate::cfg::LoanId(l),
                span: span(1, 2),
            },
        );
        // The overwrite itself conflicts (borrower still live)…
        b.stmt(
            b0,
            Stmt::Init {
                place: v,
                span: span(3, 4),
            },
        );
        // …but afterwards the loan is dead: this read is fine.
        b.stmt(
            b0,
            Stmt::Read {
                place: v,
                span: span(5, 6),
            },
        );
        b.stmt(
            b0,
            Stmt::UseBorrower {
                local: LocalId(1),
                span: span(7, 8),
            },
        );
        let diags = b.run();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span().lo, 3);
    }

    /// Shared loans coexist; a unique borrow over a live shared loan
    /// does not.
    #[test]
    fn shared_shared_ok_unique_over_shared_rejected() {
        let mut b = Builder::new(&["v", "p", "q"]);
        let v = b.place(0);
        let l1 = b.loan(v, LoanKind::Shared, 1, false);
        let l2 = b.loan(v, LoanKind::Shared, 2, false);
        let b0 = BlockId(0);
        b.stmt(
            b0,
            Stmt::Borrow {
                loan: crate::cfg::LoanId(l1),
                span: span(1, 2),
            },
        );
        b.stmt(
            b0,
            Stmt::Borrow {
                loan: crate::cfg::LoanId(l2),
                span: span(3, 4),
            },
        );
        b.stmt(
            b0,
            Stmt::UseBorrower {
                local: LocalId(1),
                span: span(5, 6),
            },
        );
        b.stmt(
            b0,
            Stmt::UseBorrower {
                local: LocalId(2),
                span: span(7, 8),
            },
        );
        assert!(b.run().is_empty(), "shared loans coexist");

        let mut b = Builder::new(&["v", "p", "q"]);
        let v = b.place(0);
        let l1 = b.loan(v, LoanKind::Shared, 1, false);
        let l2 = b.loan(v, LoanKind::Unique, 2, false);
        let b0 = BlockId(0);
        b.stmt(
            b0,
            Stmt::Borrow {
                loan: crate::cfg::LoanId(l1),
                span: span(1, 2),
            },
        );
        b.stmt(
            b0,
            Stmt::Borrow {
                loan: crate::cfg::LoanId(l2),
                span: span(3, 4),
            },
        );
        b.stmt(
            b0,
            Stmt::UseBorrower {
                local: LocalId(2),
                span: span(5, 6),
            },
        );
        b.stmt(
            b0,
            Stmt::UseBorrower {
                local: LocalId(1),
                span: span(7, 8),
            },
        );
        let diags = b.run();
        assert_eq!(diags.len(), 1, "unique over live shared conflicts");
    }

    /// Loops reach a fixpoint and stay deterministic: the same CFG
    /// checks to the same verdict twice.
    #[test]
    fn loop_fixpoint_deterministic() {
        let mut b = Builder::new(&["v", "p"]);
        let v = b.place(0);
        let l = b.loan(v, LoanKind::Unique, 1, false);
        let b0 = BlockId(0);
        let head = b.block();
        let body = b.block();
        let exit = b.block();
        b.edge(b0, head);
        b.edge(head, body);
        b.edge(head, exit);
        b.edge(body, head);
        b.stmt(
            b0,
            Stmt::Borrow {
                loan: crate::cfg::LoanId(l),
                span: span(1, 2),
            },
        );
        b.stmt(
            body,
            Stmt::Init {
                place: v,
                span: span(3, 4),
            },
        );
        b.stmt(
            exit,
            Stmt::UseBorrower {
                local: LocalId(1),
                span: span(5, 6),
            },
        );
        let first = b.run();
        let second = b.run();
        assert_eq!(first.len(), second.len());
        assert_eq!(first.len(), 1, "loop-carried conflict found once");
    }
}
