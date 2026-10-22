//! s256: a read activation's move markers never empty the caller's arena.
use wolf_mem::ubcheck::{self, Budget, RunOutcome, Verdict};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

const SOURCE: &str = include_str!("../../../corpus/memory/read_param_indexed_caller.lu");

fn checked(src: &str) -> RunOutcome {
    let mut loader = MemoryLoader::new("read-views");
    loader.add_file(&[], "main.lu", src);
    let resolved = resolve_package_with(&mut loader, &AliasTable::default(), true).unwrap();
    let tc = typecheck_package_with(&resolved.package, true);
    // The oracle also tests dynamic use-after-move, deliberately bypassing
    // static move diagnostics for those rows (but never resolution/type gaps).
    assert!(tc.not_yet.is_empty(), "{:?}", tc.not_yet);
    ubcheck::run_checked(&resolved.package, &tc, Budget::default()).unwrap()
}

#[test]
fn indexed_read_caller_survives() {
    let out = checked(SOURCE);
    assert!(matches!(out.verdict, Verdict::Exit(0)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "1 1 1\n");
}

#[test]
fn indexed_read_rebound_root_survives() {
    let src = SOURCE.replace("let tb = ed.bufs[b].tb", "let view = ed\n    let tb = view.bufs[b].tb");
    let out = checked(&src);
    assert!(matches!(out.verdict, Verdict::Exit(0)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "1 1 1\n");
}

#[test]
fn indexed_read_still_moves_inside_its_activation() {
    let src = SOURCE.replace("len_of(tb)\n}", "len_of(tb) + ed.bufs[b].tb.bytes.len\n}");
    let out = checked(&src);
    assert!(matches!(out.verdict, Verdict::Trap(t) if t.kind == "use-after-move"));
}

#[test]
fn indexed_read_different_source_spans_name_the_same_place() {
    let src = SOURCE.replace("len_of(tb)\n}", "let again = ed.bufs[b].tb\n    len_of(tb) + len_of(again)\n}");
    let out = checked(&src);
    assert!(matches!(out.verdict, Verdict::Trap(t) if t.kind == "use-after-move"));
}

#[test]
fn copied_container_is_not_a_read_view() {
    let src = SOURCE.replace("let tb = ed.bufs[b].tb", "let view = copy ed\n    let tb = view.bufs[b].tb");
    let out = checked(&src);
    assert!(matches!(out.verdict, Verdict::Exit(0)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "1 1 1\n");
}
