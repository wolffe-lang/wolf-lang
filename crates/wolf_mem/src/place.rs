//! Places and paths (`[mem.model.place]`, `[mem.model.path.disjoint]`).
//!
//! A place is a base plus a projection path: `a`, `a.x`, `a.x.y`. The
//! overlap relation is the spec's, verbatim: two paths conflict iff one
//! is a prefix of the other after identical projections; otherwise they
//! are disjoint.
//!
//! Index projections (`[mem.model.place.elem]`, eg01) come in three
//! spellings: a literal index or key (`xs[0]`, `m["a"]`), a plain
//! local of a `Copy` type (`xs[i]`, `m[k]`, `p[h]` — eg01b widened it
//! from integer locals), and anything else. Two relations read them:
//!
//! - [`PlaceTable::overlap`] / [`PlaceTable::covers`] are the relation
//!   for CLAIMS (eg02, EGC's EG2): exclusivity (`[mem.tier0.excl]`,
//!   D39's read and s168's nested call inside a `mut` argument), loans
//!   (`[mem.tier0.borrow]`) and iteration (`[mem.iter.excl]`) ask these.
//!   Two different literals are distinct and the path rule holds (1(a),
//!   (b)); an index step never meets a member step (1(c): `xs.len` is
//!   the header, not an element — eg02b, on the maintainer's ruling A
//!   for wolf-lang#472); every other index pair is one place (item 2).
//!   Step for step this is the moves pass's *may* relation.
//! - The moves pass (`[mem.tier0.move]`) asks [`PlaceTable::overlap_elem`]
//!   (*may* the two share storage: two different literals never do,
//!   and an element is never its container's header) and
//!   [`PlaceTable::covers_must`] (does a store *surely* re-initialize
//!   the moved place: item 3's must-revival, with R3 for a local index
//!   the caller vouches is unwritten since the move).
//! - The pairwise check of ONE call's argument surface
//!   ([`crate::excl`]) asks [`PlaceTable::overlap_call`] (EG3, eg03):
//!   the claim relation plus R1 (`xs[i + a]` and `xs[i + b]`, `a ≠ b`)
//!   and R2 (a loop index over literal bounds against a literal outside
//!   them), each only for a local the caller vouches keeps its value
//!   through the call's evaluation ([`Proof`]). Moves, loans, iteration
//!   and s186's nested-access check do not read R1 or R2.

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

/// A literal index or key, by VALUE (`1`, `0x1` and `1_0`'s cousin
/// `01` are one key): `[mem.model.place.elem]` 1(a)/(b).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    Int(u128),
    /// A plain string literal's text (no escape, no interpolation).
    Str(String),
    Bool(bool),
    /// A `char` literal's text between the quotes (no escape).
    Char(String),
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Key::Int(n) => write!(f, "{n}"),
            Key::Str(s) => write!(f, "\"{s}\""),
            Key::Bool(b) => write!(f, "{b}"),
            Key::Char(c) => write!(f, "'{c}'"),
        }
    }
}

/// One projection step.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Proj {
    /// A named field (tuple positions project as `"0"`, `"1"`, …).
    Field(String),
    /// A literal index or key (`xs[0]`, `m["a"]`): the same element as
    /// another `Lit` step iff the keys are equal.
    Lit(Key),
    /// A plain `Copy` local as the index or key (`xs[i]`, `m[k]`,
    /// `p[h]`; the local's id):
    /// one place with every other index for *may*; the same element as
    /// another `Sym` of the same local only while that local is
    /// unwritten (R3 — the moves pass decides that, not this table).
    Sym(u32),
    /// A plain `int` local plus a positive integer literal (`xs[i + 1]`;
    /// the local's id and the offset), EG3's R1 spelling. One place with
    /// every index for *may* and never the same element for *must* (R3
    /// never revives through an offset); only the call-surface relation
    /// ([`PlaceTable::overlap_call`]) reads the offset.
    Off(u32, u128),
    /// Any other index projection (`v[f()]`, a pool handle): one place
    /// with every index.
    Opaque,
}

impl Proj {
    /// An index step of any spelling.
    pub fn is_index(&self) -> bool {
        !matches!(self, Proj::Field(_))
    }
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

    /// `[mem.model.path.disjoint]` under `[mem.model.place.elem]`, for
    /// claims: may a claim on one place reach the other?
    pub fn overlap(&self, a: PlaceId, b: PlaceId) -> bool {
        overlap(self.get(a), self.get(b))
    }

    /// Is `a` a (non-strict) prefix of `b` under the claim relation —
    /// may using `a` use all of `b`?
    pub fn covers(&self, a: PlaceId, b: PlaceId) -> bool {
        covers(self.get(a), self.get(b))
    }

    /// `[mem.model.place.elem]` for moves: MAY the two places share
    /// storage? Two different literal steps never do (1(a)/(b)); an
    /// index step never meets a member step (1(c)); every other index
    /// pair may.
    pub fn overlap_elem(&self, a: PlaceId, b: PlaceId) -> bool {
        overlap_elem(self.get(a), self.get(b))
    }

    /// May `a` contain all of `b` (a prefix under the *may* relation)?
    pub fn covers_may(&self, a: PlaceId, b: PlaceId) -> bool {
        let (a, b) = (self.get(a), self.get(b));
        a.base == b.base
            && a.proj.len() <= b.proj.len()
            && a.proj
                .iter()
                .zip(b.proj.iter())
                .all(|(x, y)| may_elem(x, y))
    }

    /// `[mem.model.place.elem]` item 3: does `a` SURELY contain all of
    /// `b`? Fields and literals compare by equality; a `Sym` step equals
    /// another only for the same local and only when `sym_ok` vouches
    /// for it (R3: the local is unwritten between the two uses); an
    /// `Opaque` step equals nothing.
    pub fn covers_must(&self, a: PlaceId, b: PlaceId, sym_ok: &dyn Fn(u32) -> bool) -> bool {
        covers_must(self.get(a), self.get(b), sym_ok)
    }

    /// EG3 (eg03): the claim relation for the argument surface of ONE
    /// call, where R1 and R2 may prove two index steps distinct. See
    /// [`overlap_call`].
    pub fn overlap_call(&self, a: PlaceId, b: PlaceId, proof: &Proof<'_>) -> bool {
        overlap_call(self.get(a), self.get(b), proof)
    }

    /// Two paths that are a prefix pair AS SPELLED (every step of the
    /// shorter equal to the longer's): the wording of a conflict note,
    /// never a decision.
    pub fn spelled_prefix(&self, a: PlaceId, b: PlaceId) -> bool {
        let (a, b) = (self.get(a), self.get(b));
        let (short, long) = if a.proj.len() <= b.proj.len() {
            (a, b)
        } else {
            (b, a)
        };
        a.base == b.base && short.proj.iter().zip(long.proj.iter()).all(|(x, y)| x == y)
    }
}

/// `[mem.model.place.elem]`'s *may*, the relation claims and moves both
/// read: 1(a)/(b) literals by value, 1(c) an element against a member
/// step, item 2 every other index pair.
fn may_elem(a: &Proj, b: &Proj) -> bool {
    match (a, b) {
        (Proj::Field(x), Proj::Field(y)) => x == y,
        (Proj::Lit(x), Proj::Lit(y)) => x == y,
        (Proj::Field(_), _) | (_, Proj::Field(_)) => false,
        _ => true,
    }
}

/// Item 3's *must*: the two steps surely denote the same storage.
fn must_elem(a: &Proj, b: &Proj, sym_ok: &dyn Fn(u32) -> bool) -> bool {
    match (a, b) {
        (Proj::Field(x), Proj::Field(y)) => x == y,
        (Proj::Lit(x), Proj::Lit(y)) => x == y,
        (Proj::Sym(x), Proj::Sym(y)) => x == y && sym_ok(*x),
        _ => false,
    }
}

/// What EG3's proof rules may assume at one call (`[mem.model.place.elem]`
/// item 4). `stable(l)`: local `l` keeps its value through the call's
/// callee, receiver and argument evaluation — nothing there writes it,
/// no loan is ever taken on it, and the body has no raw-tier statement.
/// `range(l)`: `l` is the index of `for l in LO..HI` (or `LO..=HI`) with
/// integer-literal bounds and is never written in the loop's body; the
/// half-open `[lo, hi)` it ranges over.
pub struct Proof<'a> {
    pub stable: &'a dyn Fn(u32) -> bool,
    pub range: &'a dyn Fn(u32) -> Option<(u128, u128)>,
}

impl Proof<'_> {
    /// No proof at all: `overlap_call` is then exactly `overlap`.
    pub const NONE: Proof<'static> = Proof {
        stable: &|_| false,
        range: &|_| None,
    };
}

/// EG3: do R1 or R2 prove the two index steps never denote one element?
/// R1 — `i + a` and `i + b` over one stable local (a plain `Sym` is
/// offset 0), `a ≠ b`. R2 — a stable loop index against an integer
/// literal outside its literal range.
fn proved_distinct(a: &Proj, b: &Proj, proof: &Proof<'_>) -> bool {
    let off = |p: &Proj| match p {
        Proj::Sym(l) => Some((*l, 0u128)),
        Proj::Off(l, k) => Some((*l, *k)),
        _ => None,
    };
    if let (Some((l1, k1)), Some((l2, k2))) = (off(a), off(b))
        && l1 == l2
        && k1 != k2
    {
        return (proof.stable)(l1);
    }
    match (a, b) {
        (Proj::Sym(l), Proj::Lit(Key::Int(c))) | (Proj::Lit(Key::Int(c)), Proj::Sym(l)) => {
            match (proof.range)(*l) {
                Some((lo, hi)) => (*c < lo || *c >= hi) && (proof.stable)(*l),
                None => false,
            }
        }
        _ => false,
    }
}

/// The claim relation ([`overlap`]) with EG3's proof rules: a step pair
/// R1 or R2 proves distinct makes the two paths distinct, exactly as a
/// pair of different literals does (the path rule).
pub fn overlap_call(a: &Place, b: &Place, proof: &Proof<'_>) -> bool {
    a.base == b.base
        && a.proj
            .iter()
            .zip(b.proj.iter())
            .all(|(x, y)| may_elem(x, y) && !proved_distinct(x, y, proof))
}

/// See [`PlaceTable::overlap_elem`].
pub fn overlap_elem(a: &Place, b: &Place) -> bool {
    a.base == b.base
        && a.proj
            .iter()
            .zip(b.proj.iter())
            .all(|(x, y)| may_elem(x, y))
}

/// See [`PlaceTable::covers_must`].
pub fn covers_must(a: &Place, b: &Place, sym_ok: &dyn Fn(u32) -> bool) -> bool {
    a.base == b.base
        && a.proj.len() <= b.proj.len()
        && a.proj
            .iter()
            .zip(b.proj.iter())
            .all(|(x, y)| must_elem(x, y, sym_ok))
}

/// Two paths conflict iff one is a prefix of the other after identical
/// projections — identical under the claim relation.
pub fn overlap(a: &Place, b: &Place) -> bool {
    if a.base != b.base {
        return false;
    }
    a.proj
        .iter()
        .zip(b.proj.iter())
        .all(|(x, y)| may_elem(x, y))
}

/// Is `a` a (non-strict) prefix of `b`?
pub fn covers(a: &Place, b: &Place) -> bool {
    a.base == b.base
        && a.proj.len() <= b.proj.len()
        && a.proj
            .iter()
            .zip(b.proj.iter())
            .all(|(x, y)| may_elem(x, y))
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

    /// An `Opaque` index matches every index at its step, itself
    /// included — and no member step (1(c), eg02b).
    #[test]
    fn opaque_matches_every_index_at_its_step() {
        let idx = Place {
            base: Base::Local(0),
            proj: vec![Proj::Opaque],
        };
        assert!(!overlap(&idx, &p(0, &["len"])));
        assert!(overlap(&idx, &idx));
        assert!(overlap(&idx, &ix(0, vec![lit(0)])));
        assert!(!overlap(
            &idx,
            &Place {
                base: Base::Local(1),
                proj: vec![Proj::Opaque],
            }
        ));
    }

    fn ix(base: u32, steps: Vec<Proj>) -> Place {
        Place {
            base: Base::Local(base),
            proj: steps,
        }
    }

    fn lit(n: u128) -> Proj {
        Proj::Lit(Key::Int(n))
    }

    const YES: &dyn Fn(u32) -> bool = &|_| true;
    const NO: &dyn Fn(u32) -> bool = &|_| false;

    /// EG2, the claim relation: two different literals are distinct,
    /// the path rule holds, and every other index pair is one place.
    #[test]
    fn claims_separate_literals_and_nothing_else() {
        assert!(!overlap(&ix(0, vec![lit(0)]), &ix(0, vec![lit(1)])));
        assert!(!covers(&ix(0, vec![lit(0)]), &ix(0, vec![lit(1)])));
        assert!(overlap(&ix(0, vec![lit(1)]), &ix(0, vec![lit(1)])));
        let s = |t: &str| Proj::Lit(Key::Str(t.into()));
        assert!(!overlap(&ix(0, vec![s("a")]), &ix(0, vec![s("b")])));
        // The path rule: `g[0][1]` and `g[1][0]`; `cs[0].n` and `cs[1].n`.
        assert!(!overlap(
            &ix(0, vec![lit(0), lit(1)]),
            &ix(0, vec![lit(1), lit(0)])
        ));
        let n = Proj::Field("n".into());
        assert!(!overlap(
            &ix(0, vec![lit(0), n.clone()]),
            &ix(0, vec![lit(1), n.clone()])
        ));
        // A prefix still conflicts: `g[0]` and `g[0][1]`, `xs` and `xs[0]`.
        assert!(overlap(&ix(0, vec![lit(0)]), &ix(0, vec![lit(0), lit(1)])));
        assert!(covers(&ix(0, vec![lit(0)]), &ix(0, vec![lit(0), lit(1)])));
        assert!(overlap(&ix(0, vec![]), &ix(0, vec![lit(0)])));
        // Item 2: any index that is not a literal is one place with
        // every index, the same local included.
        for a in [lit(0), Proj::Sym(3), Proj::Opaque] {
            for b in [Proj::Sym(3), Proj::Sym(4), Proj::Opaque] {
                assert!(overlap(&ix(0, vec![a.clone()]), &ix(0, vec![b.clone()])));
                assert!(overlap(&ix(0, vec![b.clone()]), &ix(0, vec![a.clone()])));
                assert!(covers(&ix(0, vec![a.clone()]), &ix(0, vec![b.clone()])));
            }
        }
    }

    /// 1(c) under a claim (eg02b, wolf-lang#472 ruled A): an element,
    /// whatever its index, never meets a member of its container —
    /// `f(mut xs[0], xs.len)`, `f(mut xs[i], xs.len)`, `f(mut g[0][1],
    /// g[0].len)` — and the container itself still covers both.
    #[test]
    fn a_member_never_meets_an_element_under_a_claim() {
        let len = Proj::Field("len".into());
        for a in [lit(0), Proj::Sym(1), Proj::Opaque] {
            let (e, m) = (ix(0, vec![a.clone()]), ix(0, vec![len.clone()]));
            assert!(!overlap(&e, &m) && !overlap(&m, &e));
            assert!(!covers(&e, &m) && !covers(&m, &e));
            assert!(!overlap_elem(&e, &m));
            let inner = ix(0, vec![lit(0), a]);
            assert!(!overlap(&inner, &ix(0, vec![lit(0), len.clone()])));
            assert!(!overlap(&inner, &ix(0, vec![len.clone()])));
        }
        let whole = ix(0, vec![]);
        assert!(covers(&whole, &ix(0, vec![lit(0)])));
        assert!(covers(&whole, &ix(0, vec![len.clone()])));
    }

    /// 1(a)/(b): two different literals never share storage; the same
    /// literal always does; any other index pair may.
    #[test]
    fn literals_are_distinct_by_value_for_moves() {
        assert!(!overlap_elem(&ix(0, vec![lit(0)]), &ix(0, vec![lit(1)])));
        assert!(overlap_elem(&ix(0, vec![lit(1)]), &ix(0, vec![lit(1)])));
        let s = |t: &str| Proj::Lit(Key::Str(t.into()));
        assert!(!overlap_elem(&ix(0, vec![s("a")]), &ix(0, vec![s("b")])));
        assert!(overlap_elem(
            &ix(0, vec![lit(0)]),
            &ix(0, vec![Proj::Sym(2)])
        ));
        assert!(overlap_elem(
            &ix(0, vec![lit(0)]),
            &ix(0, vec![Proj::Opaque])
        ));
        assert!(overlap_elem(
            &ix(0, vec![Proj::Sym(2)]),
            &ix(0, vec![Proj::Sym(2)])
        ));
        assert!(overlap_elem(
            &ix(0, vec![Proj::Sym(2)]),
            &ix(0, vec![Proj::Sym(5)])
        ));
        // The path rule: the first differing step decides.
        assert!(!overlap_elem(
            &ix(0, vec![lit(0), lit(1)]),
            &ix(0, vec![lit(1), lit(0)])
        ));
        // A prefix still conflicts.
        assert!(overlap_elem(&ix(0, vec![]), &ix(0, vec![lit(0)])));
    }

    /// 1(c): an element is never its container's header member.
    #[test]
    fn an_element_is_not_a_member() {
        let len = Proj::Field("len".into());
        for a in [lit(0), Proj::Sym(1), Proj::Opaque] {
            assert!(!overlap_elem(&ix(0, vec![a]), &ix(0, vec![len.clone()])));
        }
    }

    /// Item 3: must-revival. A literal revives the same literal; a
    /// `Sym` the same local only when vouched for; `Opaque` nothing.
    #[test]
    fn covers_must_is_item_three() {
        assert!(covers_must(&ix(0, vec![lit(1)]), &ix(0, vec![lit(1)]), YES));
        assert!(!covers_must(
            &ix(0, vec![lit(1)]),
            &ix(0, vec![lit(0)]),
            YES
        ));
        assert!(covers_must(
            &ix(0, vec![Proj::Sym(2)]),
            &ix(0, vec![Proj::Sym(2)]),
            YES
        ));
        assert!(!covers_must(
            &ix(0, vec![Proj::Sym(2)]),
            &ix(0, vec![Proj::Sym(2)]),
            NO
        ));
        assert!(!covers_must(
            &ix(0, vec![Proj::Sym(2)]),
            &ix(0, vec![Proj::Sym(3)]),
            YES
        ));
        assert!(!covers_must(
            &ix(0, vec![Proj::Sym(2)]),
            &ix(0, vec![lit(0)]),
            YES
        ));
        assert!(!covers_must(
            &ix(0, vec![Proj::Opaque]),
            &ix(0, vec![Proj::Opaque]),
            YES
        ));
        // The whole container revives every element; a member path
        // covers its own subpaths only.
        assert!(covers_must(&ix(0, vec![]), &ix(0, vec![Proj::Opaque]), NO));
        assert!(covers_must(
            &ix(0, vec![lit(0)]),
            &ix(0, vec![lit(0), Proj::Field("tags".into())]),
            NO
        ));
        assert!(!covers_must(&ix(0, vec![lit(0)]), &ix(0, vec![]), YES));
    }

    /// EG3 (eg03), R1: two offsets of one stable local are distinct
    /// when they differ; equal offsets, another local, an unstable
    /// local, a literal and an `Opaque` index stay one place.
    #[test]
    fn r1_separates_offsets_of_one_stable_local() {
        let stable = |l: u32| l != 9;
        let none = |_: u32| None;
        let pf = Proof {
            stable: &stable,
            range: &none,
        };
        let one = |a: Proj, b: Proj| overlap_call(&ix(0, vec![a]), &ix(0, vec![b]), &pf);
        assert!(!one(Proj::Sym(1), Proj::Off(1, 1)));
        assert!(!one(Proj::Off(1, 1), Proj::Sym(1)));
        assert!(!one(Proj::Off(1, 1), Proj::Off(1, 2)));
        assert!(one(Proj::Off(1, 1), Proj::Off(1, 1)));
        assert!(one(Proj::Sym(1), Proj::Sym(1)));
        assert!(one(Proj::Sym(1), Proj::Off(2, 1)));
        assert!(one(Proj::Sym(9), Proj::Off(9, 1)));
        assert!(one(lit(1), Proj::Off(1, 1)));
        assert!(one(Proj::Opaque, Proj::Off(1, 1)));
        // The path rule: `g[i][0]` and `g[i + 1][0]`.
        assert!(!overlap_call(
            &ix(0, vec![Proj::Sym(1), lit(0)]),
            &ix(0, vec![Proj::Off(1, 1), lit(0)]),
            &pf
        ));
        // A prefix still conflicts; another base never does.
        assert!(overlap_call(
            &ix(0, vec![]),
            &ix(0, vec![Proj::Off(1, 1)]),
            &pf
        ));
        // Without a proof the relation is the claim relation.
        assert!(overlap_call(
            &ix(0, vec![Proj::Sym(1)]),
            &ix(0, vec![Proj::Off(1, 1)]),
            &Proof::NONE
        ));
        // Moves and claims outside one call never read the offset.
        assert!(overlap(
            &ix(0, vec![Proj::Sym(1)]),
            &ix(0, vec![Proj::Off(1, 1)])
        ));
        assert!(overlap_elem(
            &ix(0, vec![Proj::Sym(1)]),
            &ix(0, vec![Proj::Off(1, 1)])
        ));
        assert!(!covers_must(
            &ix(0, vec![Proj::Off(1, 1)]),
            &ix(0, vec![Proj::Off(1, 1)]),
            YES
        ));
    }

    /// EG3 (eg03), R2: a stable loop index over `[lo, hi)` is distinct
    /// from a literal outside it, and one place with a literal inside.
    #[test]
    fn r2_separates_a_loop_index_from_a_literal_outside_its_range() {
        let stable = |l: u32| l != 9;
        let range = |l: u32| (l == 1 || l == 9).then_some((1u128, 3u128));
        let pf = Proof {
            stable: &stable,
            range: &range,
        };
        let one = |a: Proj, b: Proj| overlap_call(&ix(0, vec![a]), &ix(0, vec![b]), &pf);
        assert!(!one(lit(0), Proj::Sym(1)));
        assert!(!one(Proj::Sym(1), lit(3)));
        assert!(!one(Proj::Sym(1), lit(7)));
        assert!(one(Proj::Sym(1), lit(1)));
        assert!(one(Proj::Sym(1), lit(2)));
        assert!(one(Proj::Sym(2), lit(0)), "no range, no proof");
        assert!(
            one(Proj::Sym(9), lit(0)),
            "an unstable index proves nothing"
        );
        assert!(one(Proj::Off(1, 5), lit(0)), "R2 reads a plain index only");
        let s = Proj::Lit(Key::Str("a".into()));
        assert!(one(Proj::Sym(1), s), "R2 reads an integer literal only");
    }

    #[test]
    fn keys_display_by_value() {
        assert_eq!(Key::Int(16).to_string(), "16");
        assert_eq!(Key::Str("a".into()).to_string(), "\"a\"");
        assert_eq!(Key::Bool(true).to_string(), "true");
        assert_eq!(Key::Char("x".into()).to_string(), "'x'");
    }

    #[test]
    fn covers_is_prefix_only() {
        assert!(covers(&p(0, &[]), &p(0, &["x"])));
        assert!(!covers(&p(0, &["x"]), &p(0, &[])));
        assert!(covers(&p(0, &["x"]), &p(0, &["x"])));
        assert!(!covers(&p(0, &["x"]), &p(0, &["y"])));
    }
}
