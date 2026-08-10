//! Places and paths (`[mem.model.place]`, `[mem.model.path.disjoint]`).
//!
//! A place is a base plus a projection path: `a`, `a.x`, `a.x.y`. The
//! overlap relation is the spec's, verbatim: two paths conflict iff one
//! is a prefix of the other after identical projections; otherwise they
//! are disjoint. Index projections collapse to a single opaque "some
//! element" projection — element-granular disjointness is a non-target
//! (s18), so an opaque step conservatively matches any step.

use std::collections::HashMap;

/// A place's base. `Local` is a function-local binding (the tracked
/// case); `Global` is a module-level `let`/`var`/`const` item — mode
/// agreement and same-call exclusivity apply to globals, but move
/// tracking does not (module-level state is a later campaign's
/// granule).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Base {
    Local(u32),
    Global(u32, String),
}

/// One projection step.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Proj {
    /// A named field (tuple positions project as `"0"`, `"1"`, …).
    Field(String),
    /// Any index projection (`v[i]`): all elements are one place.
    Opaque,
}

/// A place: base + projection path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Place {
    pub base: Base,
    pub proj: Vec<Proj>,
}

/// Interned place id, per function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlaceId(pub u32);

/// The per-function place universe. Every place a body mentions is
/// interned here during lowering — and when a field projection on a
/// struct-typed place is interned, its *siblings* are interned too, so
/// the move analysis can expand a whole-value move into per-field
/// residue on partial re-initialization without consulting types.
#[derive(Debug, Default)]
pub struct PlaceTable {
    places: Vec<Place>,
    /// Whether the place's type is `Copy` (implicit copy on use).
    copy: Vec<bool>,
    index: HashMap<Place, PlaceId>,
}

impl PlaceTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, place: Place, is_copy: bool) -> PlaceId {
        if let Some(&id) = self.index.get(&place) {
            return id;
        }
        let id = PlaceId(self.places.len() as u32);
        self.places.push(place.clone());
        self.copy.push(is_copy);
        self.index.insert(place, id);
        id
    }

    pub fn get(&self, id: PlaceId) -> &Place {
        &self.places[id.0 as usize]
    }

    pub fn is_copy(&self, id: PlaceId) -> bool {
        self.copy[id.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.places.len()
    }

    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (PlaceId, &Place)> {
        self.places
            .iter()
            .enumerate()
            .map(|(i, p)| (PlaceId(i as u32), p))
    }

    /// `[mem.model.path.disjoint]`: do the two places conflict?
    pub fn overlap(&self, a: PlaceId, b: PlaceId) -> bool {
        overlap(self.get(a), self.get(b))
    }

    /// Is `a` a (non-strict) prefix of `b` — does using `a` use all of
    /// `b`?
    pub fn covers(&self, a: PlaceId, b: PlaceId) -> bool {
        covers(self.get(a), self.get(b))
    }
}

fn steps_match(a: &Proj, b: &Proj) -> bool {
    match (a, b) {
        (Proj::Field(x), Proj::Field(y)) => x == y,
        // Opaque conservatively matches anything: two index
        // projections into the same base overlap, and an index
        // projection overlaps any field path (a non-target refined
        // no further).
        (Proj::Opaque, _) | (_, Proj::Opaque) => true,
    }
}

/// Two paths conflict iff one is a prefix of the other after identical
/// projections.
pub fn overlap(a: &Place, b: &Place) -> bool {
    if a.base != b.base {
        return false;
    }
    a.proj
        .iter()
        .zip(b.proj.iter())
        .all(|(x, y)| steps_match(x, y))
}

/// Is `a` a (non-strict) prefix of `b`?
pub fn covers(a: &Place, b: &Place) -> bool {
    a.base == b.base
        && a.proj.len() <= b.proj.len()
        && a.proj
            .iter()
            .zip(b.proj.iter())
            .all(|(x, y)| steps_match(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(base: u32, fields: &[&str]) -> Place {
        Place {
            base: Base::Local(base),
            proj: fields.iter().map(|f| Proj::Field(f.to_string())).collect(),
        }
    }

    #[test]
    fn disjoint_fields_do_not_overlap() {
        assert!(!overlap(&p(0, &["x"]), &p(0, &["y"])));
        assert!(!overlap(&p(0, &["a", "n"]), &p(0, &["b", "n"])));
        assert!(!overlap(&p(0, &["x"]), &p(1, &["x"])));
    }

    #[test]
    fn prefixes_overlap_both_ways() {
        assert!(overlap(&p(0, &[]), &p(0, &["x"])));
        assert!(overlap(&p(0, &["x"]), &p(0, &[])));
        assert!(overlap(&p(0, &["a"]), &p(0, &["a", "n"])));
        assert!(overlap(&p(0, &["x"]), &p(0, &["x"])));
    }

    #[test]
    fn opaque_matches_everything_at_its_step() {
        let idx = Place {
            base: Base::Local(0),
            proj: vec![Proj::Opaque],
        };
        assert!(overlap(&idx, &p(0, &["x"])));
        assert!(overlap(&idx, &idx));
        assert!(!overlap(
            &idx,
            &Place {
                base: Base::Local(1),
                proj: vec![Proj::Opaque],
            }
        ));
    }

    #[test]
    fn covers_is_prefix_only() {
        assert!(covers(&p(0, &[]), &p(0, &["x"])));
        assert!(!covers(&p(0, &["x"]), &p(0, &[])));
        assert!(covers(&p(0, &["x"]), &p(0, &["x"])));
        assert!(!covers(&p(0, &["x"]), &p(0, &["y"])));
    }
}
