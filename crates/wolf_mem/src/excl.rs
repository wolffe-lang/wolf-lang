//! Call-surface exclusivity (`[mem.tier0.excl]`, E1002): within one
//! call, a `mut` place must have no other live access path — and a
//! `take`d place may not be lent at all.
//!
//! Call-extent loans begin and end inside a single call statement, so
//! this check is pairwise over the [`crate::cfg::CallSurface`] — no
//! dataflow. Two-phase leniency is structural: `Copy` values passed
//! `read` were copied during argument evaluation and never appear in
//! the surface (the `xs.push(xs.len)` shape), so what remains here
//! really is live for the whole call.

use wolf_diag::{Diagnostic, codes};

use crate::cfg::{Cfg, Stmt};

pub fn check(cfg: &Cfg, diags: &mut Vec<Diagnostic>) {
    for block in &cfg.blocks {
        for stmt in &block.stmts {
            let Stmt::Call(c) = stmt else { continue };
            // mut vs mut / read / take.
            for (i, &(m, mspan)) in c.mut_args.iter().enumerate() {
                for &(m2, m2span) in c.mut_args.iter().skip(i + 1) {
                    if cfg.places.overlap(m, m2) {
                        let (a, b) = (cfg.show_place(m), cfg.show_place(m2));
                        diags.push(
                            Diagnostic::error(
                                codes::E1002,
                                m2span,
                                format!("`{b}` cannot go `mut` here: it overlaps `{a}`, already passed `mut` in this call"),
                            )
                            .with_label("second exclusive claim on the same place")
                            .with_secondary(mspan, format!("`{a}` is passed `mut` here"))
                            .with_note(overlap_note(cfg, m, m2)),
                        );
                    }
                }
                for &(r, rspan) in &c.read_args {
                    if cfg.places.overlap(m, r) {
                        let (a, b) = (cfg.show_place(m), cfg.show_place(r));
                        diags.push(
                            Diagnostic::error(
                                codes::E1002,
                                rspan,
                                format!("`{b}` is lent for this call while `{a}` goes `mut` in it"),
                            )
                            .with_label("read of a place being mutated")
                            .with_secondary(mspan, format!("`{a}` is passed `mut` here"))
                            .with_note(overlap_note(cfg, m, r)),
                        );
                    }
                }
                for &(t, tspan) in &c.take_args {
                    if cfg.places.overlap(m, t) {
                        let (a, b) = (cfg.show_place(m), cfg.show_place(t));
                        diags.push(
                            Diagnostic::error(
                                codes::E1002,
                                tspan,
                                format!(
                                    "`{b}` moves away in this call while `{a}` goes `mut` in it"
                                ),
                            )
                            .with_label("moved while being mutated")
                            .with_secondary(mspan, format!("`{a}` is passed `mut` here")),
                        );
                    }
                }
            }
            // read vs take: the lent place may not move away.
            for &(r, rspan) in &c.read_args {
                for &(t, tspan) in &c.take_args {
                    if cfg.places.overlap(r, t) {
                        let (a, b) = (cfg.show_place(r), cfg.show_place(t));
                        diags.push(
                            Diagnostic::error(
                                codes::E1002,
                                tspan,
                                format!("`{b}` moves away in this call while `{a}` is lent to it"),
                            )
                            .with_label("moved while lent")
                            .with_secondary(rspan, format!("`{a}` is read by this call")),
                        );
                    }
                }
            }
        }
    }
}

fn overlap_note(cfg: &Cfg, a: crate::place::PlaceId, b: crate::place::PlaceId) -> String {
    let (sa, sb) = (cfg.show_place(a), cfg.show_place(b));
    if a == b {
        "the same place twice is never disjoint.".to_string()
    } else if cfg.places.covers(a, b) {
        format!(
            "`{sb}` is inside `{sa}` — a path and its prefix conflict [mem.model.path.disjoint]. Disjoint fields (`x.a` with `x.b`) are fine together."
        )
    } else if cfg.places.covers(b, a) {
        format!(
            "`{sa}` is inside `{sb}` — a path and its prefix conflict [mem.model.path.disjoint]. Disjoint fields (`x.a` with `x.b`) are fine together."
        )
    } else {
        "the two paths can reach the same memory.".to_string()
    }
}
