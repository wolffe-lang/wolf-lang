//! s37 — container method depth under checked execution: the List
//! recoverable reads (`pop`/`get`/`first`/`last` miss as `{none}`
//! rows, never traps or sentinels), the emptiness probes, and the
//! Pool observability surface wolf-lang#11 named as missing (`len`,
//! `is_empty`, the non-trapping `alive` handle probe). Mutating
//! receivers spell their X1 mode (`(mut xs).pop()`).

use wolf_mem::ubcheck::{self, Budget, Verdict};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

fn assert_exit(src: &str, code: u8) {
    let mut ml = MemoryLoader::new("cont");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "input resolves clean: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        tc.not_yet.is_empty() && !tc.has_errors(),
        "input typechecks clean: {:?} {:?}",
        tc.not_yet,
        tc.diagnostics
    );
    let mem = wolf_mem::check_package(&res.package, &tc);
    assert!(
        mem.not_yet.is_empty(),
        "input stays inside the mem surface: {:?}",
        mem.not_yet
    );
    assert!(
        mem.diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "input is statically accepted: {:?}",
        mem.diagnostics
    );
    match ubcheck::run_checked(&res.package, &tc, Budget::default())
        .expect("within the executable surface")
        .verdict
    {
        Verdict::Exit(n) => assert_eq!(n, code, "exit code"),
        other => panic!("expected exit({code}), got {other:?}"),
    }
}

#[test]
fn list_recoverable_reads() {
    assert_exit(
        "fn main() -> !int {\n\
         var xs = List[int]()\n\
         let was_empty = xs.is_empty()\n\
         (mut xs).push(10)\n\
         (mut xs).push(20)\n\
         (mut xs).push(30)\n\
         let hit = xs.get(1) else 0 - 1\n\
         let miss = xs.get(9) else 0 - 1\n\
         let head = xs.first() else 0 - 1\n\
         let tail = xs.last() else 0 - 1\n\
         let popped = (mut xs).pop() else 0 - 1\n\
         let n = xs.count()\n\
         let ok_reads = hit == 20 && miss == 0 - 1 && head == 10 && tail == 30\n\
         let ok_drain = popped == 30 && n == 2 && was_empty && !xs.is_empty()\n\
         if ok_reads && ok_drain { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

#[test]
fn list_pop_to_empty_misses() {
    assert_exit(
        "fn main() -> !int {\n\
         var xs = List[int]()\n\
         (mut xs).push(1)\n\
         let a = (mut xs).pop() else 0 - 1\n\
         let b = (mut xs).pop() else 0 - 1\n\
         if a == 1 && b == 0 - 1 && xs.is_empty() { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

#[test]
fn list_clear() {
    assert_exit(
        "fn main() -> !int {\n\
         var xs = List[int]()\n\
         (mut xs).push(1)\n\
         (mut xs).push(2)\n\
         (mut xs).clear()\n\
         if xs.len == 0 && xs.is_empty() { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

#[test]
fn pool_observability() {
    // wolf-lang#11's gap 3, closed: length counts LIVE slots, and
    // `alive` answers the staleness question without the trap.
    assert_exit(
        "struct Node { v: int }\n\
         \n\
         fn main() -> !int {\n\
         var pool = Pool[Node]()\n\
         let was_empty = pool.is_empty()\n\
         let h = (mut pool).reserve()\n\
         (mut pool).init(h, Node { v: 7 })\n\
         let one = pool.len()\n\
         let live = pool.alive(h)\n\
         (mut pool).remove(h)\n\
         let gone = pool.alive(h)\n\
         let ok_probe = was_empty && one == 1 && live && !gone\n\
         if ok_probe && pool.is_empty() { 0 } else { 1 }\n\
         }\n",
        0,
    );
}
