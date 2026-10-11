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
//! changes is where the entry lives: in a table keyed by the live
//! numbers ([`SeqTable`]). A close removes the key and frees its slot;
//! the next open takes a new number and a free slot.
//!
//! **Use after close.** A closed number is absent from the table and
//! can never come back, because the counter never hands a number out
//! twice. So a call on a closed handle misses, and the caller answers
//! its `io` row, as before; and it cannot reach some other open handle,
//! because no two handles ever share a number — a slot holds its
//! entry's own number, and a lookup compares it. This is the tombstone
//! scheme with the tombstones implied by the counter instead of stored:
//! every number below `next` that is not in the table is closed.
//!
//! **Bound.** Memory is proportional to the handles open at once, and
//! the table gives storage back after a burst. The counter is an
//! `i64`: at a billion opens a second it lasts 292 years.
//!
//! **Cost.** The keys are the runtime's own sequential integers, so
//! there is nothing to hash: a key's home slot is its low bits, a
//! lookup is that index and one comparison (the vector's index and its
//! `None` check, plus the compare), and a close shifts at most the few
//! entries that had been pushed past their home. Measured (kasumi,
//! `perf stat -e instructions:u`, release tier): 72 more user
//! instructions per `fs_open` + `fs_close` than the push-only vector
//! (345 to 417) and 103 more per socket's life, with no change in
//! wall time — both cycles are their syscalls. The first cut of s228
//! used the standard hash map with a multiplicative hasher and cost
//! 140 and 171.

/// A table is never smaller than this once it holds anything, and
/// never shrunk below it: a server's steady set of handles lives here.
const MIN_SLOTS: usize = 16;

/// An open-addressed table from the runtime's own sequential keys —
/// handle numbers, proc indexes, monitor serials — to their entries.
///
/// Linear probing from `key & mask`, at most half full, so every probe
/// ends at an empty slot; removal shifts the run behind the hole back
/// (no tombstones, so a table that opens and closes for ever never
/// degrades or needs rebuilding).
#[derive(Debug)]
pub(crate) struct SeqTable<T> {
    /// Empty, or a power of two long.
    slots: Vec<Option<(u64, T)>>,
    live: usize,
}

impl<T> SeqTable<T> {
    /// `const`, so a table can be a `static` behind a `Mutex` with no
    /// lazy init.
    pub(crate) const fn new() -> SeqTable<T> {
        SeqTable {
            slots: Vec::new(),
            live: 0,
        }
    }

    /// The slot at `i & mask`.
    #[inline(always)]
    fn slot(&self, i: usize) -> &Option<(u64, T)> {
        debug_assert!(self.slots.len().is_power_of_two());
        // SAFETY: every caller has checked the table is not empty, its
        // length is a power of two (`rebuild` is the only place it is
        // set), and the index is masked by `len - 1`, so it is in
        // bounds.
        unsafe { self.slots.get_unchecked(i & (self.slots.len() - 1)) }
    }

    /// The slot holding `key`, if it is in the table.
    #[inline]
    fn find(&self, key: u64) -> Option<usize> {
        if self.slots.is_empty() {
            return None;
        }
        let mask = self.slots.len() - 1;
        let mut i = key as usize & mask;
        loop {
            match self.slot(i) {
                None => return None,
                Some((k, _)) if *k == key => return Some(i),
                Some(_) => i = (i + 1) & mask,
            }
        }
    }

    /// The entry for `key`.
    #[inline]
    pub(crate) fn get(&self, key: u64) -> Option<&T> {
        let i = self.find(key)?;
        self.slots[i].as_ref().map(|(_, v)| v)
    }

    /// [`SeqTable::get`], mutably.
    #[inline]
    pub(crate) fn get_mut(&mut self, key: u64) -> Option<&mut T> {
        let i = self.find(key)?;
        self.slots[i].as_mut().map(|(_, v)| v)
    }

    /// Put `v` under `key`, which must not be in the table (the
    /// callers' keys come from counters that never repeat).
    #[inline]
    pub(crate) fn insert(&mut self, key: u64, v: T) {
        debug_assert!(self.find(key).is_none(), "key {key} inserted twice");
        if (self.live + 1) * 2 > self.slots.len() {
            self.rebuild((self.slots.len() * 2).max(MIN_SLOTS));
        }
        self.place(key, v);
        self.live += 1;
    }

    /// The first empty slot at or after `key`'s home takes it.
    #[inline]
    fn place(&mut self, key: u64, v: T) {
        let mask = self.slots.len() - 1;
        let mut i = key as usize & mask;
        while self.slot(i).is_some() {
            i = (i + 1) & mask;
        }
        self.slots[i] = Some((key, v));
    }

    /// Take `key`'s entry out, or `None` when it is not in the table.
    /// The slot is free at once; after a burst (fewer than an eighth
    /// of the slots in use) the table gives storage back to the host.
    #[inline]
    pub(crate) fn remove(&mut self, key: u64) -> Option<T> {
        let mut hole = self.find(key)?;
        let (_, gone) = self.slots[hole].take().expect("found");
        self.live -= 1;
        // Close the hole: an entry further along the run moves back
        // into it unless its home lies after the hole (cyclically), in
        // which case a probe for it never passes the hole.
        let mask = self.slots.len() - 1;
        let mut j = hole;
        loop {
            j = (j + 1) & mask;
            let Some((k, _)) = self.slot(j) else { break };
            let home = *k as usize & mask;
            if (j.wrapping_sub(home) & mask) >= (j.wrapping_sub(hole) & mask) {
                self.slots[hole] = self.slots[j].take();
                hole = j;
            }
        }
        if self.slots.len() > MIN_SLOTS * 4 && self.live * 8 < self.slots.len() {
            self.rebuild((self.live * 4).next_power_of_two().max(MIN_SLOTS));
        }
        Some(gone)
    }

    /// Move every entry into a table of `len` slots (a power of two,
    /// at least twice the live count).
    #[cold]
    #[inline(never)]
    fn rebuild(&mut self, len: usize) {
        debug_assert!(len.is_power_of_two() && len >= self.live * 2);
        let old = std::mem::take(&mut self.slots);
        self.slots.resize_with(len, || None);
        for (k, v) in old.into_iter().flatten() {
            self.place(k, v);
        }
    }

    /// Every entry, in no particular order.
    pub(crate) fn values(&self) -> impl Iterator<Item = &T> {
        self.slots.iter().flatten().map(|(_, v)| v)
    }

    /// How many entries the table holds.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.live
    }

    /// How many slots the table has.
    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.slots.len()
    }
}

/// The live handles of one family, by number.
#[derive(Debug)]
pub(crate) struct Handles<T> {
    live: SeqTable<T>,
    /// The number the next [`Handles::insert`] answers. Only ever
    /// increases.
    next: i64,
}

impl<T> Handles<T> {
    /// An empty table whose first handle is `first` (not negative).
    /// `const`, so a table can be a `static` behind a `Mutex` with no
    /// lazy init.
    pub(crate) const fn starting_at(first: i64) -> Handles<T> {
        Handles {
            live: SeqTable::new(),
            next: first,
        }
    }

    /// Put `v` in the table and answer its handle: the smallest number
    /// this table has never answered.
    #[inline]
    pub(crate) fn insert(&mut self, v: T) -> i64 {
        let h = self.next;
        // 2^63 opens: unreachable (the module doc), and wrapping into
        // the negative space the callers use for error codes would be
        // worse than stopping.
        self.next = h.checked_add(1).expect("wolf-rt: handle numbers exhausted");
        self.live.insert(h as u64, v);
        h
    }

    /// The entry behind `h`, or `None` for a closed, forged or negative
    /// handle (a negative number is no key: as a `u64` it is larger
    /// than any the counter can reach).
    #[inline]
    pub(crate) fn get(&self, h: i64) -> Option<&T> {
        self.live.get(h as u64)
    }

    /// [`Handles::get`], mutably.
    #[inline]
    pub(crate) fn get_mut(&mut self, h: i64) -> Option<&mut T> {
        self.live.get_mut(h as u64)
    }

    /// Close `h`: its entry, or `None` when it was not open.
    #[inline]
    pub(crate) fn remove(&mut self, h: i64) -> Option<T> {
        self.live.remove(h as u64)
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
        assert!(peak >= 20_000);
        for &h in &hs[2..] {
            t.remove(h);
        }
        assert_eq!(t.len(), 2);
        assert!(
            t.capacity() <= MIN_SLOTS * 4,
            "{} slots kept for 2 handles (peak {peak})",
            t.capacity()
        );
        assert_eq!(t.get(hs[0]), Some(&0));
        assert_eq!(t.get(hs[1]), Some(&1));
    }

    /// The table against a model, through the shapes that stress
    /// probing and the backward shift: keys that collide on their home
    /// slot (a long-lived handle and the ones a table-length later),
    /// removals from the middle of a run, runs that wrap the end of
    /// the table, and growth and shrinking with entries in place.
    #[test]
    fn the_table_agrees_with_a_model() {
        use std::collections::BTreeMap;
        let mut t: SeqTable<u64> = SeqTable::new();
        let mut model: BTreeMap<u64, u64> = BTreeMap::new();
        // A deterministic generator (the runtime carries no rand).
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let mut key = 0u64;
        for step in 0..200_000u32 {
            let r = next();
            // Mostly open, then mostly close, in waves: the table
            // grows, shrinks and regrows with entries in place.
            let open = match (step / 20_000) % 2 {
                0 => r % 3 != 0,
                _ => r % 3 == 0,
            };
            if open || model.is_empty() {
                // Keys only go up, in strides that land on occupied
                // homes (16 and 32 are table lengths the test visits).
                key += [1, 1, 1, 16, 32][(r >> 8) as usize % 5];
                t.insert(key, key ^ 0xABCD);
                model.insert(key, key ^ 0xABCD);
            } else {
                // Close one of the oldest 64 — but keep the first few
                // keys for the whole run (the long-lived listener).
                let nth = (r >> 16) as usize % model.len().min(64);
                let victim = *model.keys().nth(nth).unwrap();
                if victim > 8 {
                    assert_eq!(t.remove(victim), model.remove(&victim));
                } else {
                    let newest = *model.keys().next_back().unwrap();
                    assert_eq!(t.remove(newest), model.remove(&newest));
                }
            }
            if step % 997 == 0 {
                assert_eq!(t.len(), model.len());
                for (k, v) in &model {
                    assert_eq!(t.get(*k), Some(v), "step {step}: key {k} lost");
                }
                assert!(t.get(key + 1).is_none());
                assert!(t.capacity() >= t.len() * 2);
            }
        }
        let mut seen: Vec<u64> = t.values().copied().collect();
        seen.sort_unstable();
        let mut want: Vec<u64> = model.values().copied().collect();
        want.sort_unstable();
        assert_eq!(seen, want);
        for k in model.keys().copied().collect::<Vec<_>>() {
            assert_eq!(t.remove(k), model.remove(&k));
            assert_eq!(t.remove(k), None);
        }
        assert_eq!(t.len(), 0);
    }
}
