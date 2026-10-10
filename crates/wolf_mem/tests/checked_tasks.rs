//! s226 (C1) — the checked machine's task tier, asked directly
//! (`[exec.checked.task]`, `[exec.checked.sched]`, `[exec.checked.race]`).
//! The rows held to lupin are the driver's `checked_tasks_lanes`; these
//! are the answers that are the machine's own: what ends a run from
//! inside a task, where a task allocates, and what a seed changes.

use wolf_mem::ubcheck::{self, Budget, RunOutcome, UbRow, Verdict};
use wolf_sema::{AliasTable, MemoryLoader, NotYet, resolve_package_with, typecheck_package_with};

fn run_with(src: &str, budget: Budget, seed: u64) -> Result<RunOutcome, NotYet> {
    let mut ml = MemoryLoader::new("tasks");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        !tc.has_errors(),
        "input typechecks clean: {:?}",
        tc.diagnostics
    );
    let mem = wolf_mem::check_package(&res.package, &tc);
    assert!(
        mem.diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "input is statically accepted: {:?}",
        mem.diagnostics
    );
    ubcheck::run_checked_seeded(&res.package, &tc, budget, "", "main", seed)
}

fn run(src: &str) -> RunOutcome {
    run_with(src, Budget::default(), 0).expect("the program is within the executable surface")
}

const UAF_PROC: &str = r#"import c "stdlib.h"

fn bad() -> int {
    // # Safety: DELIBERATELY UNSOUND — a read after free.
    unsafe {
        let m = c.malloc(8)
        let p = m as *u64
        p[0] = 1
        c.free(m)
        let x = p[0]
    }
    7
}

fn main() -> !int {
    let w = spawn proc bad()
    let v = w.join() else |e| {
        print("joined: {e}")
        return 0
    }
    print("value {v}")
    0
}
"#;

/// A proc contains a trap (`[conc.proc.exit]`'s `fault(kind)`); it does
/// not contain a UB finding. The run ends with the row, and the join's
/// handler never prints. (lupin 0.1.49 answers `joined: error`, exit 0 —
/// wolf-interp#228.)
#[test]
fn a_ub_finding_inside_a_proc_ends_the_run() {
    let out = run(UAF_PROC);
    match out.verdict {
        Verdict::Ub(f) => assert_eq!(f.row, UbRow::P1, "{}", f.message),
        other => panic!("expected ub(P1), got {other:?}"),
    }
    assert_eq!(out.stdout, "", "the join's handler does not run");
}

/// A trap inside a proc is contained and read at the join.
#[test]
fn a_trap_inside_a_proc_is_a_fault_at_the_join() {
    let out = run("fn boom() -> int {\n    assert(1 == 2)\n    7\n}\n\n\
         fn main() -> !int {\n    let w = spawn proc boom()\n    \
         let v = w.join() else |e| {\n        print(\"joined: {e}\")\n        return 3\n    }\n    \
         print(\"value {v}\")\n    0\n}\n");
    assert!(matches!(out.verdict, Verdict::Exit(3)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "joined: fault\n");
}

/// A trap in a scope's task is `[conc.task.fail]`'s failure: the
/// sibling is cancelled or runs to its end, and the trap re-raises at
/// the scope's exit — after the join, so what the sibling printed
/// stands and the line after the scope does not.
#[test]
fn a_trap_in_a_task_reraises_at_the_scopes_exit() {
    let out = run("fn main() -> !int {\n    scope s {\n        \
         s.spawn(fn() { assert(1 == 2) })\n        \
         s.spawn(fn() { print(\"sibling\") })\n    }\n    \
         print(\"after\")\n    0\n}\n");
    match out.verdict {
        Verdict::Trap(t) => assert_eq!(t.kind, "assert"),
        other => panic!("expected trap(assert), got {other:?}"),
    }
    assert_eq!(out.stdout, "sibling\n");
}

/// The budgets are the run's: a task that exhausts the step budget
/// ends the run as `unsupported`, from wherever it was running.
#[test]
fn a_budget_exhausted_in_a_task_is_the_runs_refusal() {
    let src = "fn main() -> !int {\n    scope s {\n        \
               s.spawn(fn() {\n            var i = 0\n            \
               while i >= 0 { i = i + 1 }\n        })\n    }\n    0\n}\n";
    let tight = Budget {
        steps: 20_000,
        ..Budget::default()
    };
    let refusal = run_with(src, tight, 0).expect_err("the step budget runs out");
    assert_eq!(refusal.construct, "step budget exhausted");
}

/// `os_exit` on a task is the program's exit, at once: the owner parked
/// at the join never resumes.
#[test]
fn os_exit_on_a_task_exits_the_run() {
    let out = run("fn main() -> !int {\n    scope s {\n        \
         s.spawn(fn() {\n            print(\"leaving\")\n            os_exit(7)\n        })\n    }\n    \
         print(\"after\")\n    0\n}\n");
    assert!(matches!(out.verdict, Verdict::Exit(7)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "leaving\n");
}

/// Module state is one object for every task (`[mem.static]`): two
/// tasks writing it with nothing between them race, and the trap names
/// `[conc.mm.race.3]`. (lupin 0.1.49 gives each task a copy and prints
/// 0 — wolf-interp#229.)
#[test]
fn module_state_written_by_two_tasks_is_a_race() {
    let out = run("var hits: int = 0\n\nfn bump() {\n    \
         // # Safety: DELIBERATELY UNSOUND — module state written by two tasks.\n    \
         unsafe {\n        hits = hits + 1\n    }\n}\n\n\
         fn main() -> !int {\n    scope s {\n        s.spawn(fn() { bump() })\n        \
         s.spawn(fn() { bump() })\n    }\n    0\n}\n");
    match out.verdict {
        Verdict::Trap(t) => assert_eq!((t.kind, t.clause), ("race", "conc.mm.race.3")),
        other => panic!("expected trap(race), got {other:?}"),
    }
    assert!(
        out.stderr.contains("data race on module state"),
        "{}",
        out.stderr
    );
}

/// The same two writers, one after the other's scope has joined: the
/// join orders them (`[conc.mm.hb.spawn]`), and the count is 2.
#[test]
fn module_state_written_across_two_joined_scopes_is_ordered() {
    let out = run("var hits: int = 0\n\nfn bump() {\n    \
         // # Safety: each call runs in a scope that joined before the next opens.\n    \
         unsafe {\n        hits = hits + 1\n    }\n}\n\n\
         fn main() -> !int {\n    scope a {\n        a.spawn(fn() { bump() })\n    }\n    \
         scope b {\n        b.spawn(fn() { bump() })\n    }\n    var n = 0\n    \
         // # Safety: read after both joins.\n    \
         unsafe {\n        n = hits\n    }\n    print(\"{n}\")\n    0\n}\n");
    assert!(matches!(out.verdict, Verdict::Exit(0)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "2\n");
}

/// `[conc.task.par.cost]`: "tasks run with the process root as their
/// ambient region" — a scope task's list is not charged to the region
/// its spawner had open, as on the native tier. (lupin 0.1.49 charges
/// the spawner's region — wolf-interp#229.)
#[test]
fn a_scope_task_allocates_in_the_root_region() {
    let out = run(
        "fn main() -> !int {\n    var before = 0\n    var after = 0\n    \
         region r {\n        before = region_bytes(r)\n        scope s {\n            \
         s.spawn(fn() {\n                var xs = List[int]()\n                \
         for i in 0..100 { (mut xs).push(i) }\n                print(\"built {xs.len}\")\n            \
         })\n        }\n        after = region_bytes(r)\n    }\n    \
         let grew = after > before\n    print(\"grew {grew}\")\n    0\n}\n",
    );
    assert_eq!(out.stdout, "built 100\ngrew false\n");
}

/// Ruling #30: a `par` worker allocates in a region of its own, and the
/// join transfers that region — its charge included — to the region
/// the `par` was evaluated in.
#[test]
fn a_par_workers_region_is_transferred_to_the_pars_region() {
    let out = run("fn label(n: int) -> str { \"row-{n}-of-many\" }\n\n\
         fn main() -> !int {\n    var xs = List[int]()\n    \
         for i in 0..64 { (mut xs).push(i) }\n    var before = 0\n    var after = 0\n    \
         var last = 0\n    region r {\n        before = region_bytes(r)\n        \
         let ys = xs.par(label)\n        after = region_bytes(r)\n        \
         last = ys[63].len\n    }\n    let grew = after > before\n    \
         print(\"grew {grew} {last}\")\n    0\n}\n");
    assert!(matches!(out.verdict, Verdict::Exit(0)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "grew true 14\n");
}

/// `[exec.checked.sched]`: seed 0 runs tasks in spawn order; a packed
/// seed is its digits; equal seeds are equal runs.
#[test]
fn a_seed_selects_the_schedule() {
    let src = "fn main() -> !int {\n    scope s {\n        \
               s.spawn(fn() { print_raw(\"a\") })\n        \
               s.spawn(fn() { print_raw(\"b\") })\n        \
               s.spawn(fn() { print_raw(\"c\") })\n    }\n    0\n}\n";
    let at = |seed: u64| run_with(src, Budget::default(), seed).expect("runs").stdout;
    assert_eq!(at(0), "abc");
    // 5 = 2 + 1·3: the third of three ready tasks, then the second of two.
    assert_eq!(at((1 << 62) | 5), "cba");
    // 1 = 1 + 0·3: the second, then the first of the two left.
    assert_eq!(at((1 << 62) | 1), "bac");
    for seed in [1, 2, 3, 42, u64::MAX] {
        assert_eq!(at(seed), at(seed), "seed {seed} twice");
    }
}
