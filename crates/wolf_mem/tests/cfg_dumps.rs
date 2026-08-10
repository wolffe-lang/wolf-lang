//! Check-CFG textual dumps over the Tier-0 memory corpus (the s01
//! IR-dump snapshot family): the CFG shape is a reviewed artifact and
//! must stay byte-stable — blocks, statements, edges, trap edges,
//! defer lowering, and view-set narrowing are all visible here.
//!
//! Also pins the fact-export stubs (s26's input schema) for the same
//! corpus files, and the determinism/robustness property: random
//! effect CFGs check to identical verdicts on repeated runs, without
//! panicking.

use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

fn corpus(path: &str) -> String {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/");
    std::fs::read_to_string(format!("{root}{path}")).expect("corpus file")
}

fn dump(src: &str) -> String {
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        tc.not_yet.is_empty(),
        "dump inputs typecheck: {:?}",
        tc.not_yet
    );
    wolf_mem::dump_package(&res.package, &tc)
}

fn facts(src: &str) -> String {
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let tc = typecheck_package_with(&res.package, true);
    let mem = wolf_mem::check_package(&res.package, &tc);
    let mut out = String::new();
    for f in &mem.facts {
        out.push_str(&f.render());
        out.push('\n');
    }
    out
}

#[test]
fn dump_exclusivity_acceptance_file() {
    insta::assert_snapshot!("dump_exclusivity", dump(&corpus("memory/exclusivity.lu")));
}

#[test]
fn dump_move_use_after() {
    insta::assert_snapshot!(
        "dump_move_use_after",
        dump(&corpus("memory/move_use_after.lu"))
    );
}

#[test]
fn dump_excl_overlap() {
    insta::assert_snapshot!("dump_excl_overlap", dump(&corpus("memory/excl_overlap.lu")));
}

#[test]
fn dump_view_set_norm() {
    insta::assert_snapshot!(
        "dump_view_set_norm",
        dump(&corpus("memory/view_set_norm.lu"))
    );
}

#[test]
fn dump_excl_disjoint_ok() {
    insta::assert_snapshot!(
        "dump_excl_disjoint_ok",
        dump(&corpus("memory/excl_disjoint_ok.lu"))
    );
}

#[test]
fn dump_defer_order() {
    insta::assert_snapshot!("dump_defer_order", dump(&corpus("memory/defer_order.lu")));
}

#[test]
fn dump_use_after_move_field() {
    insta::assert_snapshot!(
        "dump_use_after_move_field",
        dump(&corpus("faults/use_after_move_field.lu"))
    );
}

#[test]
fn dump_exclusivity_nested_path() {
    insta::assert_snapshot!(
        "dump_exclusivity_nested_path",
        dump(&corpus("faults/exclusivity_nested_path.lu"))
    );
}

#[test]
fn dump_is_deterministic() {
    let src = corpus("memory/exclusivity.lu");
    assert_eq!(dump(&src), dump(&src), "two lowerings are byte-identical");
}

#[test]
fn facts_exclusivity_acceptance_file() {
    insta::assert_snapshot!("facts_exclusivity", facts(&corpus("memory/exclusivity.lu")));
}

#[test]
fn facts_move_use_after() {
    insta::assert_snapshot!(
        "facts_move_use_after",
        facts(&corpus("memory/move_use_after.lu"))
    );
}

// ---------------------------------------------------------------------
// Fuzz-lite (the s18 acceptance's "checker never panics, verdicts
// deterministic" gate, at the CFG level): seeded random effect CFGs.
// ---------------------------------------------------------------------

mod fuzz {
    use wolf_mem::cfg::{
        Block, BlockId, CallSurface, Cfg, Loan, LoanId, LoanKind, Local, LocalId, Stmt,
    };
    use wolf_mem::place::{Base, Place, PlaceTable, Proj};
    use wolf_span::Span;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, bound: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) % bound.max(1)
        }
    }

    fn random_cfg(seed: u64) -> Cfg {
        let mut r = Lcg(seed);
        let mut sm = wolf_span::SourceMap::new();
        let file = sm.intern(std::path::Path::new("fuzz.lu"));
        let span = |r: &mut Lcg| {
            let lo = r.next(1000) as u32;
            Span::new(file, lo, lo + 1)
        };
        let n_locals = 2 + r.next(4) as usize;
        let mut places = PlaceTable::new();
        let mut place_ids = Vec::new();
        for l in 0..n_locals {
            let base = Place {
                base: Base::Local(l as u32),
                proj: Vec::new(),
            };
            place_ids.push(places.intern(base.clone(), l % 2 == 0));
            for f in ["x", "y"] {
                let mut proj = base.proj.clone();
                proj.push(Proj::Field(f.to_string()));
                place_ids.push(places.intern(
                    Place {
                        base: base.base.clone(),
                        proj,
                    },
                    false,
                ));
            }
        }
        let n_blocks = 2 + r.next(6) as usize;
        let mut blocks: Vec<Block> = (0..n_blocks).map(|_| Block::default()).collect();
        let mut loans = Vec::new();
        // A couple of loans with borrowers drawn from the local pool.
        for _ in 0..r.next(3) {
            let place = place_ids[r.next(place_ids.len() as u64) as usize];
            loans.push(Loan {
                place,
                kind: if r.next(2) == 0 {
                    LoanKind::Shared
                } else {
                    LoanKind::Unique
                },
                borrower: LocalId(r.next(n_locals as u64) as u32),
                origin: span(&mut r),
                two_phase: r.next(4) == 0,
            });
        }
        #[allow(clippy::needless_range_loop)] // `blocks[b]` is indexed twice per iteration
        for b in 0..n_blocks {
            let n_stmts = r.next(6) as usize;
            for _ in 0..n_stmts {
                let place = place_ids[r.next(place_ids.len() as u64) as usize];
                let s = span(&mut r);
                let stmt = match r.next(9) {
                    0 => Stmt::Read { place, span: s },
                    1 => Stmt::Move { place, span: s },
                    2 => Stmt::Init { place, span: s },
                    3 => Stmt::Mutate { place, span: s },
                    4 => Stmt::Uninit { place, span: s },
                    5 if !loans.is_empty() => Stmt::Borrow {
                        loan: LoanId(r.next(loans.len() as u64) as u32),
                        span: s,
                    },
                    6 if !loans.is_empty() => Stmt::Activate {
                        loan: LoanId(r.next(loans.len() as u64) as u32),
                        span: s,
                    },
                    7 => Stmt::UseBorrower {
                        local: LocalId(r.next(n_locals as u64) as u32),
                        span: s,
                    },
                    _ => {
                        let second = place_ids[r.next(place_ids.len() as u64) as usize];
                        Stmt::Call(CallSurface {
                            callee: "f".to_string(),
                            span: s,
                            mut_args: vec![(place, s)],
                            read_args: vec![(second, s)],
                            take_args: Vec::new(),
                        })
                    }
                };
                blocks[b].stmts.push(stmt);
            }
            // 0–2 random forward-or-backward edges: cycles included.
            for _ in 0..r.next(3) {
                let to = BlockId(r.next(n_blocks as u64) as u32);
                if b as u32 != to.0 && !blocks[b].succs.contains(&to) {
                    blocks[b].succs.push(to);
                }
            }
        }
        // Keep every block reachable-ish: chain them.
        #[allow(clippy::needless_range_loop)]
        for b in 0..n_blocks - 1 {
            let to = BlockId(b as u32 + 1);
            if !blocks[b].succs.contains(&to) {
                blocks[b].succs.push(to);
            }
        }
        Cfg {
            name: format!("fuzz_{seed}"),
            blocks,
            locals: (0..n_locals)
                .map(|l| Local {
                    name: format!("l{l}"),
                    span: Span::new(file, 0, 0),
                    ty: "?".to_string(),
                    is_copy: l % 2 == 0,
                    param_mode: None,
                })
                .collect(),
            places,
            loans,
            entry: BlockId(0),
            exit: BlockId(n_blocks as u32 - 1),
        }
    }

    /// The acceptance gate, CFG-shaped: no panic, and byte-identical
    /// verdicts on repeated runs, across 200 seeded random CFGs with
    /// cycles, partial moves, loans, and call surfaces.
    #[test]
    fn checker_never_panics_and_is_deterministic() {
        for seed in 0..200u64 {
            let cfg = random_cfg(seed);
            let first: Vec<String> = wolf_mem::check_cfg(&cfg)
                .iter()
                .map(|d| format!("{:?}@{:?}", d.code, d.span()))
                .collect();
            let second: Vec<String> = wolf_mem::check_cfg(&cfg)
                .iter()
                .map(|d| format!("{:?}@{:?}", d.code, d.span()))
                .collect();
            assert_eq!(first, second, "seed {seed} diverged between runs");
        }
    }
}
