//! Exemplar property tests (s01): the pattern every crate follows.
//! Case-count convention: PROPTEST_CASES is small in PR CI, large nightly.

use std::path::{Path, PathBuf};

use proptest::prelude::*;
use wolf_span::SourceMap;

proptest! {
    /// Interning is idempotent: the same path always yields the same id.
    #[test]
    fn intern_idempotent(paths in proptest::collection::vec("[a-z0-9_/]{1,24}", 1..64)) {
        let mut map = SourceMap::new();
        let first: Vec<_> = paths.iter().map(|p| map.intern(Path::new(p))).collect();
        let second: Vec<_> = paths.iter().map(|p| map.intern(Path::new(p))).collect();
        prop_assert_eq!(&first, &second);
    }

    /// Round-trip: every interned id looks up the path that produced it.
    #[test]
    fn intern_roundtrip(paths in proptest::collection::vec("[a-z0-9_/]{1,24}", 1..64)) {
        let mut map = SourceMap::new();
        for p in &paths {
            let id = map.intern(Path::new(p));
            prop_assert_eq!(map.path(id), Path::new(p));
        }
    }

    /// Distinct paths get distinct ids, and the map counts uniques exactly.
    #[test]
    fn intern_distinct(paths in proptest::collection::vec("[a-z0-9_/]{1,24}", 1..64)) {
        let mut map = SourceMap::new();
        let unique: std::collections::BTreeSet<PathBuf> =
            paths.iter().map(PathBuf::from).collect();
        for p in &paths {
            map.intern(Path::new(p));
        }
        prop_assert_eq!(map.len(), unique.len());
        let ids: std::collections::BTreeSet<_> =
            unique.iter().map(|p| map.intern(p)).collect();
        prop_assert_eq!(ids.len(), unique.len());
    }
}
