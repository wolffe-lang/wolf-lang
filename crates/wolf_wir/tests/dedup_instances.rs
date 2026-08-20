//! s94: the D8 dedup ratio's pin, over the corpus witness the s43
//! contract named — `corpus/generics/hundred_shapes.lu`, one generic
//! fn, one hundred call sites, one layout. The per-file lowering
//! ledger says `lowers`; the unique count is a whole-module fact a
//! per-file line cannot carry, so it is pinned HERE: seen 100 (every
//! site demands), lowered 10 (the worklist is key-idempotent — ten
//! distinct element spellings), unique 1 (every instance's body is
//! `ptr -> ptr` post-mid-end, so the content hash folds all ten to
//! one representative). A move in any of the three numbers is a
//! population change or a dedup regression, and this test makes it
//! loud either way.

use std::path::Path;
use wolf_sema::{AliasTable, DiskLoader, resolve_package_with, typecheck_package_with};

#[test]
fn hundred_shapes_is_one_body() {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/generics/hundred_shapes.lu");
    let mut sm = wolf_span::SourceMap::new();
    let mut loader =
        DiskLoader::from_entry(&f, &mut sm, Box::new(|_: &[u8]| false)).expect("loader");
    let res = resolve_package_with(&mut loader, &AliasTable::default(), true).expect("resolve");
    let tc = typecheck_package_with(&res.package, true);
    assert!(tc.not_yet.is_empty() && !tc.has_errors(), "mem-clean");
    let mut build = wolf_wir::lower_package(&res.package, &tc);
    assert_eq!(
        build.stats.instantiations_seen, 100,
        "every call site pushes a demand"
    );
    assert_eq!(
        build.stats.instantiations_lowered, 10,
        "ten distinct keys lower ten bodies"
    );
    let homes = wolf_wir::midend::summary::Homes::from_package(&res.package, &build.module);
    let wp = wolf_wir::midend::optimize_whole_program(
        &mut build.module,
        &homes,
        &wolf_wir::midend::Options::default(),
    )
    .expect("whole-program phase");
    assert_eq!(
        wp.stats.instantiations_unique,
        Some(1),
        "at the D8 dedup point, ten ptr->ptr bodies are ONE hash class"
    );
    assert!(
        wp.stats.dedup.bodies_merged >= 9,
        "nine instance duplicates merge (got {})",
        wp.stats.dedup.bodies_merged
    );
}
