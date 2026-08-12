//! Hand-rolled entity arenas — u32 newtype indices, flat side tables,
//! packed operand lists. Modeled on `cranelift-entity` (refs/repos/
//! cranelift) but owned: no cranelift dependency in `wolf_wir` (s24).
//!
//! Everything is index-based: no pointer graphs, no `Rc`, no interior
//! mutability. All maps support **LIFO delete** (mark, then truncate
//! back to the mark) — unused by s24 itself but load-bearing for s25's
//! speculative construction (Click §5), so it is designed and tested
//! now.

use std::fmt;
use std::marker::PhantomData;

/// A u32-backed entity index. Implemented by the `entity_id!` newtypes.
pub trait EntityRef: Copy + Eq + Ord + fmt::Debug {
    fn new(index: u32) -> Self;
    fn as_u32(self) -> u32;
    fn index(self) -> usize {
        self.as_u32() as usize
    }
}

/// Declare a u32 newtype entity index with a display prefix.
macro_rules! entity_id {
    ($(#[$doc:meta])* $vis:vis struct $name:ident, $prefix:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        $vis struct $name(u32);

        impl $crate::entity::EntityRef for $name {
            fn new(index: u32) -> Self {
                $name(index)
            }
            fn as_u32(self) -> u32 {
                self.0
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }
    };
}
pub(crate) use entity_id;

/// A rollback point for a [`PrimaryMap`] or [`ListPool`] (LIFO delete).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Mark(pub(crate) u32);

/// The arena that OWNS entities of kind `K`: pushing mints the next
/// index; data lives in one flat `Vec`.
#[derive(Clone, Debug, Default)]
pub struct PrimaryMap<K: EntityRef, V> {
    elems: Vec<V>,
    _k: PhantomData<K>,
}

impl<K: EntityRef, V> PrimaryMap<K, V> {
    pub fn new() -> Self {
        PrimaryMap {
            elems: Vec::new(),
            _k: PhantomData,
        }
    }

    /// Append, minting the next entity index.
    pub fn push(&mut self, v: V) -> K {
        let k = K::new(u32::try_from(self.elems.len()).expect("entity index overflow"));
        self.elems.push(v);
        k
    }

    /// The index the next `push` will mint.
    pub fn next_key(&self) -> K {
        K::new(self.elems.len() as u32)
    }

    pub fn len(&self) -> usize {
        self.elems.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elems.is_empty()
    }

    pub fn contains(&self, k: K) -> bool {
        k.index() < self.elems.len()
    }

    pub fn get(&self, k: K) -> Option<&V> {
        self.elems.get(k.index())
    }

    pub fn keys(&self) -> impl DoubleEndedIterator<Item = K> + use<K, V> {
        (0..self.elems.len() as u32).map(K::new)
    }

    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> {
        self.elems
            .iter()
            .enumerate()
            .map(|(i, v)| (K::new(i as u32), v))
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.elems.iter()
    }

    /// LIFO delete, half 1: remember the current length.
    pub fn mark(&self) -> Mark {
        Mark(self.elems.len() as u32)
    }

    /// LIFO delete, half 2: drop every entity minted since `mark`.
    pub fn truncate_to(&mut self, mark: Mark) {
        debug_assert!(mark.0 as usize <= self.elems.len(), "mark from the future");
        self.elems.truncate(mark.0 as usize);
    }
}

impl<K: EntityRef, V> std::ops::Index<K> for PrimaryMap<K, V> {
    type Output = V;
    fn index(&self, k: K) -> &V {
        &self.elems[k.index()]
    }
}

impl<K: EntityRef, V> std::ops::IndexMut<K> for PrimaryMap<K, V> {
    fn index_mut(&mut self, k: K) -> &mut V {
        &mut self.elems[k.index()]
    }
}

/// A side table over entities owned elsewhere. Reads of unset keys
/// yield the default; writes grow the table.
#[derive(Clone, Debug)]
pub struct SecondaryMap<K: EntityRef, V: Clone> {
    elems: Vec<V>,
    default: V,
    _k: PhantomData<K>,
}

impl<K: EntityRef, V: Clone + Default> Default for SecondaryMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: EntityRef, V: Clone + Default> SecondaryMap<K, V> {
    pub fn new() -> Self {
        SecondaryMap {
            elems: Vec::new(),
            default: V::default(),
            _k: PhantomData,
        }
    }
}

impl<K: EntityRef, V: Clone> SecondaryMap<K, V> {
    pub fn get(&self, k: K) -> &V {
        self.elems.get(k.index()).unwrap_or(&self.default)
    }

    pub fn set(&mut self, k: K, v: V) {
        if k.index() >= self.elems.len() {
            self.elems.resize(k.index() + 1, self.default.clone());
        }
        self.elems[k.index()] = v;
    }
}

/// A packed pool of entity lists: each [`EntityList`] is an
/// (offset, len) window into one shared `Vec<u32>`. Lists are built
/// once and never resized — s25's builder constructs operand lists
/// before minting the instruction that owns them.
#[derive(Clone, Debug, Default)]
pub struct ListPool<T: EntityRef> {
    data: Vec<u32>,
    _t: PhantomData<T>,
}

/// A handle to a list in a [`ListPool`]. Copyable, 8 bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct EntityList<T: EntityRef> {
    off: u32,
    len: u32,
    _t: PhantomData<T>,
}

impl<T: EntityRef> Default for EntityList<T> {
    fn default() -> Self {
        EntityList {
            off: 0,
            len: 0,
            _t: PhantomData,
        }
    }
}

impl<T: EntityRef> ListPool<T> {
    pub fn new() -> Self {
        ListPool {
            data: Vec::new(),
            _t: PhantomData,
        }
    }

    /// Pack a slice into the pool.
    pub fn intern(&mut self, elems: &[T]) -> EntityList<T> {
        let off = u32::try_from(self.data.len()).expect("list pool overflow");
        self.data.extend(elems.iter().map(|e| e.as_u32()));
        EntityList {
            off,
            len: elems.len() as u32,
            _t: PhantomData,
        }
    }

    /// Read a list back. O(len) — copies indices out of the packed pool.
    pub fn get(&self, list: EntityList<T>) -> Vec<T> {
        self.data[list.off as usize..(list.off + list.len) as usize]
            .iter()
            .map(|&raw| T::new(raw))
            .collect()
    }

    /// Overwrite element `i` of a list in place (s42 pass infrastructure:
    /// operand rewriting). Lists are interned per instruction/edge and
    /// never shared (only the empty list aliases, and it has no elements
    /// to write), so in-place element writes cannot alias another list.
    pub fn set(&mut self, list: EntityList<T>, i: usize, v: T) {
        debug_assert!(i < list.len as usize, "list element out of range");
        self.data[list.off as usize + i] = v.as_u32();
    }

    /// Total packed length (for arena-layout tests).
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn mark(&self) -> Mark {
        Mark(self.data.len() as u32)
    }

    pub fn truncate_to(&mut self, mark: Mark) {
        debug_assert!(mark.0 as usize <= self.data.len(), "mark from the future");
        self.data.truncate(mark.0 as usize);
    }
}

impl<T: EntityRef> EntityList<T> {
    pub fn len(self) -> usize {
        self.len as usize
    }

    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    entity_id!(struct T, "t");

    #[test]
    fn primary_map_push_and_index() {
        let mut m: PrimaryMap<T, &str> = PrimaryMap::new();
        let a = m.push("a");
        let b = m.push("b");
        assert_eq!(a, T::new(0));
        assert_eq!(b, T::new(1));
        assert_eq!(m[a], "a");
        assert_eq!(m[b], "b");
        assert_eq!(m.len(), 2);
        assert_eq!(m.keys().collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(format!("{a}"), "t0");
    }

    #[test]
    fn primary_map_lifo_truncate() {
        let mut m: PrimaryMap<T, u64> = PrimaryMap::new();
        for i in 0..5 {
            m.push(i);
        }
        let mark = m.mark();
        for i in 5..9 {
            m.push(i);
        }
        assert_eq!(m.len(), 9);
        m.truncate_to(mark);
        assert_eq!(m.len(), 5);
        // Re-pushing mints the same indices again — LIFO delete is exact.
        let k = m.push(99);
        assert_eq!(k, T::new(5));
    }

    #[test]
    fn secondary_map_defaults_and_growth() {
        let mut s: SecondaryMap<T, u32> = SecondaryMap::new();
        assert_eq!(*s.get(T::new(7)), 0);
        s.set(T::new(3), 42);
        assert_eq!(*s.get(T::new(3)), 42);
        assert_eq!(*s.get(T::new(2)), 0);
    }

    #[test]
    fn list_pool_intern_get_truncate() {
        let mut pool: ListPool<T> = ListPool::new();
        let l1 = pool.intern(&[T::new(1), T::new(2)]);
        let mark = pool.mark();
        let l2 = pool.intern(&[T::new(3)]);
        assert_eq!(pool.get(l1), vec![T::new(1), T::new(2)]);
        assert_eq!(pool.get(l2), vec![T::new(3)]);
        assert_eq!(pool.len(), 3);
        pool.truncate_to(mark);
        assert_eq!(pool.len(), 2);
        assert_eq!(pool.get(l1), vec![T::new(1), T::new(2)]);
        let empty = pool.intern(&[]);
        assert!(empty.is_empty());
    }
}
