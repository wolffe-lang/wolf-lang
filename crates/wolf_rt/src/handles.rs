//! The table behind a family of `int` handles (s228, wolf-lang#661).
//!
//! `fs_open` and the net calls hand a program an `int`. Until s228 both
//! tables were a `Vec<Option<T>>` that only ever pushed: the handle was
//! the index, a close left a `None`, and the vector — 4 bytes a file,
//! 56 a socket — grew with every open for the life of the process.
//!
//! # The scheme: the number is never reused, the storage always is
//!
//! `[os.fs.std]` promises that later handles are larger and that a
//! number is never reused, and programs print handles. So the NUMBER
//! stays exactly what it was — a counter that only goes up — and what
//! changes is where the entry lives: in a map keyed by the live numbers.
//! A close removes the key and its storage; the next open takes a new
//! number and whatever storage the map has free.
//!
//! **Use after close.** A closed number is absent from the map and can
//! never come back, because the counter never hands a number out twice.
//! So a call on a closed handle misses, and the caller answers its `io`
//! row, as before; and it cannot reach some other open handle, because
//! no two handles ever share a number. This is the tombstone scheme
//! with the tombstones implied by the counter instead of stored: every
//! number below `next` that is not in the map is closed.
//!
//! **Bound.** Memory is proportional to the handles open at once, and
//! the map gives storage back after a burst ([`Handles::remove`]). The
//! counter is an `i64`: at a billion opens a second it lasts 292 years.
//!
//! **Cost.** A lookup is one multiply and one probe where it was one
//! index. The keys are the runtime's own sequential integers, never a
//! peer's choice, so the hash needs no secret.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// A Fibonacci multiply over a sequential key: the high bits the map's
/// control bytes read and the low bits its bucket index both vary from
/// one key to the next.
#[derive(Default)]
pub(crate) struct SeqHasher(u64);

impl Hasher for SeqHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
    fn write_i64(&mut self, n: i64) {
        self.write_u64(n as u64);
    }
    fn write_u32(&mut self, n: u32) {
        self.write_u64(u64::from(n));
    }
}

/// A map from sequential integer keys.
pub(crate) type SeqMap<K, V> = HashMap<K, V, BuildHasherDefault<SeqHasher>>;

/// Below this capacity a table is never shrunk: the storage is a few
/// hundred bytes and a server's steady set of handles lives here.
const SHRINK_FLOOR: usize = 64;

/// The live handles of one family, by number.
#[derive(Debug)]
pub(crate) struct Handles<T> {
    live: SeqMap<i64, T>,
    /// The number the next [`Handles::insert`] answers. Only ever
    /// increases.
    next: i64,
}

impl<T> Handles<T> {
    /// An empty table whose first handle is `first`. `const`, so a
    /// table can be a `static` behind a `Mutex` with no lazy init.
    pub(crate) const fn starting_at(first: i64) -> Handles<T> {
        Handles {
            live: HashMap::with_hasher(BuildHasherDefault::new()),
            next: first,
        }
    }

    /// Put `v` in the table and answer its handle: the smallest number
    /// this table has never answered.
    pub(crate) fn insert(&mut self, v: T) -> i64 {
        let h = self.next;
        // 2^63 opens: unreachable (the module doc), and wrapping into
        // the negative space the callers use for error codes would be
        // worse than stopping.
        self.next = h.checked_add(1).expect("wolf-rt: handle numbers exhausted");
        self.live.insert(h, v);
        h
    }

    /// The entry behind `h`, or `None` for a closed, forged or negative
    /// handle.
    pub(crate) fn get(&self, h: i64) -> Option<&T> {
        self.live.get(&h)
    }

    /// [`Handles::get`], mutably.
    pub(crate) fn get_mut(&mut self, h: i64) -> Option<&mut T> {
        self.live.get_mut(&h)
    }

    /// Close `h`: its entry, or `None` when it was not open. The
    /// storage goes back to the map, and after a burst (fewer than an
    /// eighth of the buckets in use) back to the host.
    pub(crate) fn remove(&mut self, h: i64) -> Option<T> {
        let gone = self.live.remove(&h)?;
        let cap = self.live.capacity();
        if cap > SHRINK_FLOOR && self.live.len() < cap / 8 {
            self.live.shrink_to((self.live.len() * 2).max(SHRINK_FLOOR));
        }
        Some(gone)
    }

    /// How many handles are open.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.live.len()
    }

    /// How many entries the table has storage for.
    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.live.capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The numbers are the ones the push-only vector answered: `first`,
    /// then one more each time, whatever was closed in between.
    #[test]
    fn numbers_only_go_up() {
        let mut t: Handles<&str> = Handles::starting_at(3);
        assert_eq!(t.insert("a"), 3);
        assert_eq!(t.insert("b"), 4);
        assert_eq!(t.remove(3), Some("a"));
        assert_eq!(t.insert("c"), 5, "a closed number is never answered again");
        assert_eq!(t.remove(4), Some("b"));
        assert_eq!(t.remove(5), Some("c"));
        assert_eq!(t.insert("d"), 6);
    }

    /// wolf-lang#661's witness at the table: a closed handle misses for
    /// ever — after its storage has been reused a million times over —
    /// and never answers the entry that reused it.
    #[test]
    fn a_closed_handle_never_reaches_a_later_one() {
        let mut t: Handles<u64> = Handles::starting_at(0);
        let stale = t.insert(u64::MAX);
        assert_eq!(t.remove(stale), Some(u64::MAX));
        assert_eq!(t.remove(stale), None, "double close misses");
        for i in 0..1_000_000u64 {
            let h = t.insert(i);
            assert!(
                t.get(stale).is_none(),
                "the closed handle reached entry {i}"
            );
            assert!(t.get_mut(stale).is_none());
            assert_eq!(t.get(h), Some(&i));
            assert_eq!(t.remove(h), Some(i));
        }
        assert!(t.get(-1).is_none() && t.get(i64::MAX).is_none() && t.get(i64::MIN).is_none());
    }

    /// The bound: a million open/close cycles with one handle open at
    /// a time leave the table no larger than it was after the first.
    #[test]
    fn storage_is_reused() {
        let mut t: Handles<[u8; 56]> = Handles::starting_at(0);
        let keep = t.insert([0; 56]);
        let h = t.insert([1; 56]);
        t.remove(h);
        let warm = t.capacity();
        for _ in 0..1_000_000 {
            let h = t.insert([1; 56]);
            t.remove(h);
        }
        assert_eq!(t.len(), 1);
        assert_eq!(t.capacity(), warm, "the table grew with closed handles");
        assert!(t.get(keep).is_some());
    }

    /// A burst gives its storage back: ten thousand at once, then two.
    #[test]
    fn a_burst_is_given_back() {
        let mut t: Handles<u64> = Handles::starting_at(0);
        let hs: Vec<i64> = (0..10_000).map(|i| t.insert(i)).collect();
        let peak = t.capacity();
        assert!(peak >= 10_000);
        for &h in &hs[2..] {
            t.remove(h);
        }
        assert_eq!(t.len(), 2);
        assert!(
            t.capacity() <= SHRINK_FLOOR * 2,
            "{} buckets kept for 2 handles (peak {peak})",
            t.capacity()
        );
        assert_eq!(t.get(hs[0]), Some(&0));
        assert_eq!(t.get(hs[1]), Some(&1));
    }
}
