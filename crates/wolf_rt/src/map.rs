//! The s152 native `Map` runtime — a keyed store over the `List`
//! buffer discipline (`[type.map]`, `[mem.map.absent]`).
//!
//! The value form of a `Map[K, V]` at the native tier is ONE pointer to
//! a header whose first five words ARE a [`crate::list::ListHdr`] —
//! `{data, len, cap, elem, region}` at the same offsets — so lowering
//! reads the entry count through the list header's `len` slot and the
//! region story is the list's word for word (`[mem.region.create.3]`:
//! header and buffer land in the ambient region at the constructor,
//! growth stays in the birth region). Four key/value layout words
//! follow, then a pointer to the hash index and its slot count.
//!
//! An ENTRY is the `(K, V)` tuple's flat layout — the key's bytes at
//! offset 0, the value's at `val_off`, `elem` bytes per entry (the
//! tuple's list stride) — so `pairs()` is a byte copy of the entry
//! buffer into a fresh list of that stride and compiled code loads
//! each element as the tuple it already knows how to load. Optimized
//! `pairs()` walks read this same dense buffer through the ListHdr prefix.
//! Entries keep INSERTION ORDER, agreeing with the checked machine and
//! reference interpreter. A separate deterministic open-addressing index
//! stores entry indices, never entry pointers, so entry-buffer growth
//! cannot invalidate it. Its power-of-two table is at most half full.
//! Lookup and insertion are expected constant time in the entry count
//! (plus hashing the key's bytes); adversarial collisions can still be
//! linear. This is not a randomized, collision-resistant hash.
//!
//! Index growth doubles, in the birth region, keeping abandoned tables
//! charged just like entry buffers. Eight bytes a slot cost 16–32 bytes
//! per peak entry beyond the initial small table, and less than 64 bytes
//! per peak entry including abandoned tables (plus the small-table
//! constant). Clear and remove reuse the index without allocating.
//!
//! Key comparison is the language's own equality (`[type.map.key]`):
//! `key_kind` 0 compares the key's bytes (`int`, `char`, `bool` — a
//! flat scalar each), `key_kind` 1 reads the key as a `str`'s
//! `{ptr, len}` pair and compares the bytes it names. No user code
//! ever decides what "the same key" means, which is why the key set
//! is closed.
//!
//! Every entry point takes the header pointer and ONE caller-owned
//! slot laid out as an entry: `get` reads the key from it and writes
//! the value into it on a hit; `set` reads both. So compiled code
//! never addresses map memory directly — the WIR token story stays on
//! the caller's stack slot, as for lists.

use core::ffi::c_void;

use crate::list::{alloc_in, list_from_bytes};
use crate::native::ambient_region;

#[repr(C)]
pub(crate) struct MapHdr {
    /// The [`ListHdr`] prefix: the entry buffer, live count, capacity
    /// (in entries), entry stride, birth region.
    data: *mut u8,
    len: i64,
    cap: i64,
    elem: i64,
    region: *mut c_void,
    /// 0 = the key's bytes compare; 1 = the key is a `str` pair.
    key_kind: i64,
    key_size: i64,
    val_off: i64,
    val_size: i64,
    /// Zero is vacant; otherwise a slot is the dense entry index + 1.
    index: *mut i64,
    index_cap: i64,
}

const KEY_BYTES: i64 = 0;
const KEY_STR: i64 = 1;

/// Do the keys at `a` and `b` name the same key?
unsafe fn key_eq(h: &MapHdr, a: *const u8, b: *const u8) -> bool {
    #[cfg(test)]
    tests::COMPARISONS.with(|n| n.set(n.get() + 1));
    unsafe {
        if h.key_kind == KEY_STR {
            let ap = (a as *const i64).read_unaligned() as *const u8;
            let al = (a as *const i64).add(1).read_unaligned() as usize;
            let bp = (b as *const i64).read_unaligned() as *const u8;
            let bl = (b as *const i64).add(1).read_unaligned() as usize;
            if al != bl {
                return false;
            }
            if al == 0 {
                return true;
            }
            core::slice::from_raw_parts(ap, al) == core::slice::from_raw_parts(bp, bl)
        } else {
            let n = h.key_size as usize;
            core::slice::from_raw_parts(a, n) == core::slice::from_raw_parts(b, n)
        }
    }
}

/// FNV-1a over the equality bytes, with a final avalanche so nearby
/// scalars and common string prefixes spread across the low slot bits.
/// No address, ambient state, platform hasher or random seed participates.
unsafe fn key_hash(h: &MapHdr, key: *const u8) -> u64 {
    unsafe {
        let (bytes, len) = if h.key_kind == KEY_STR {
            (
                (key as *const i64).read_unaligned() as *const u8,
                (key as *const i64).add(1).read_unaligned() as usize,
            )
        } else {
            (key, h.key_size as usize)
        };
        let mut hash = 0xcbf29ce484222325u64;
        // An empty string may have a null pointer: never make a slice or
        // dereference it. Embedded NULs and non-ASCII bytes are ordinary bytes.
        for i in 0..len {
            hash = (hash ^ bytes.add(i).read() as u64).wrapping_mul(0x100000001b3);
        }
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xff51afd7ed558ccd);
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xc4ceb9fe1a85ec53);
        hash ^ (hash >> 33)
    }
}

/// The entry whose key equals the key at `key_ptr`, by index. Even a
/// synthetically full table terminates: a vacant slot is not the only exit.
unsafe fn find(h: &MapHdr, key_ptr: *const u8) -> Option<i64> {
    if h.len == 0 {
        return None;
    }
    unsafe {
        let mask = h.index_cap as u64 - 1;
        let mut slot = key_hash(h, key_ptr) & mask;
        for _ in 0..h.index_cap {
            let encoded = h.index.add(slot as usize).read();
            if encoded == 0 {
                return None;
            }
            let i = encoded - 1;
            if key_eq(h, h.data.add((i * h.elem) as usize), key_ptr) {
                return Some(i);
            }
            slot = (slot + 1) & mask;
        }
        None
    }
}

/// Place an already-written dense entry into a table with a vacant slot.
unsafe fn index_insert(h: &MapHdr, entry: i64) {
    unsafe {
        let mask = h.index_cap as u64 - 1;
        let mut slot = key_hash(h, h.data.add((entry * h.elem) as usize)) & mask;
        for _ in 0..h.index_cap {
            let p = h.index.add(slot as usize);
            if p.read() == 0 {
                p.write(entry + 1);
                return;
            }
            slot = (slot + 1) & mask;
        }
        unreachable!("map index must have a vacant slot");
    }
}

unsafe fn clear_index(h: &MapHdr) {
    if h.index_cap != 0 {
        unsafe { h.index.write_bytes(0, h.index_cap as usize) };
    }
}

unsafe fn rebuild_index(h: &MapHdr) {
    unsafe {
        clear_index(h);
        for i in 0..h.len {
            index_insert(h, i);
        }
    }
}

/// Ensure space for `entries` at load <= 1/2, retaining the old table's
/// region charge. Copy can request an arbitrary entry count, not a power of two.
unsafe fn reserve_index(h: &mut MapHdr, entries: i64) {
    if entries <= h.index_cap / 2 {
        return;
    }
    let mut cap = h.index_cap.max(16);
    while entries > cap / 2 {
        cap *= 2;
    }
    h.index = alloc_in(h.region, cap as usize * core::mem::size_of::<i64>()) as *mut i64;
    h.index_cap = cap;
    unsafe { rebuild_index(h) };
}

/// `Map[K, V]()` — a fresh empty map: how keys compare, the key's
/// size, the value's offset and size inside an entry, and the entry
/// stride (the `(K, V)` tuple's list stride).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_map_new(
    key_kind: i64,
    key_size: i64,
    val_off: i64,
    val_size: i64,
    stride: i64,
) -> i64 {
    let region = ambient_region();
    let hdr = alloc_in(region, core::mem::size_of::<MapHdr>()) as *mut MapHdr;
    unsafe {
        hdr.write(MapHdr {
            data: core::ptr::null_mut(),
            len: 0,
            cap: 0,
            elem: stride.max(1),
            region,
            key_kind: if key_kind == KEY_STR {
                KEY_STR
            } else {
                KEY_BYTES
            },
            key_size: key_size.max(0),
            val_off: val_off.max(0),
            val_size: val_size.max(0),
            index: core::ptr::null_mut(),
            index_cap: 0,
        });
    }
    hdr as i64
}

/// `m[k]` — 1 with the bound value copied into the slot's value bytes
/// (`[mem.map.absent]`: lowering makes the hit the ok half and the
/// miss the `none` tag), or 0 when `k` is absent. The slot holds the
/// key at offset 0.
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_map_new`]; `kv` must address one entry's
/// worth of readable-and-writable bytes laid out as the map's entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_map_get(hdr: i64, kv: i64) -> i64 {
    unsafe {
        let h = &*(hdr as *const MapHdr);
        let kv = kv as *mut u8;
        match find(h, kv) {
            Some(i) => {
                let entry = h.data.add((i * h.elem) as usize);
                core::ptr::copy_nonoverlapping(
                    entry.add(h.val_off as usize),
                    kv.add(h.val_off as usize),
                    h.val_size as usize,
                );
                1
            }
            None => 0,
        }
    }
}

/// `m[k] = v` — replace the bound value, or append a fresh entry
/// (`[mem.map.absent]`: an assignment through an absent key inserts).
/// Growth doubles the entry buffer in the map's BIRTH region, exactly
/// as a list's push does.
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_map_new`]; `kv` must address one entry's
/// worth of readable bytes laid out as the map's entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_map_set(hdr: i64, kv: i64) {
    unsafe {
        let h = &mut *(hdr as *mut MapHdr);
        let kv = kv as *const u8;
        if let Some(i) = find(h, kv) {
            let entry = h.data.add((i * h.elem) as usize);
            core::ptr::copy_nonoverlapping(
                kv.add(h.val_off as usize),
                entry.add(h.val_off as usize),
                h.val_size as usize,
            );
            return;
        }
        if h.len == h.cap {
            let ncap = if h.cap == 0 { 8 } else { h.cap * 2 };
            let ndata = alloc_in(h.region, (ncap * h.elem) as usize);
            if h.len > 0 {
                core::ptr::copy_nonoverlapping(h.data, ndata, (h.len * h.elem) as usize);
            }
            h.data = ndata;
            h.cap = ncap;
        }
        reserve_index(h, h.len + 1);
        core::ptr::copy_nonoverlapping(kv, h.data.add((h.len * h.elem) as usize), h.elem as usize);
        index_insert(h, h.len);
        h.len += 1;
    }
}

/// `pairs()` — a fresh `List[(K, V)]` of every entry in insertion
/// order: the entry buffer copied at exact capacity into a list of
/// the entry stride, so compiled code loads each element as the tuple.
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_map_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_map_pairs(hdr: i64) -> i64 {
    unsafe {
        let h = &*(hdr as *const MapHdr);
        let n = (h.len * h.elem) as usize;
        let bytes: &[u8] = if n == 0 {
            &[]
        } else {
            core::slice::from_raw_parts(h.data, n)
        };
        list_from_bytes(h.elem as usize, bytes) as i64
    }
}

/// `m.remove(k)` — erase the key's entry (wolf-lang#344, D50): 1 with
/// the erased value written into the slot's value half, 0 on a miss
/// (the map unchanged). The entries after it shift down one stride, so
/// insertion order survives the erase — `pairs()` still reports the
/// order the remaining keys were first bound in. The tail shifts and
/// the retained index is rebuilt (expected O(slots + key bytes)); no
/// storage is allocated or freed. The capacities and birth region stay.
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_map_new`]; `kv` must address one entry's
/// worth of readable-and-writable bytes laid out as the map's entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_map_remove(hdr: i64, kv: i64) -> i64 {
    unsafe {
        let h = &mut *(hdr as *mut MapHdr);
        let kv = kv as *mut u8;
        let Some(i) = find(h, kv) else {
            return 0;
        };
        let stride = h.elem as usize;
        let entry = h.data.add(i as usize * stride);
        core::ptr::copy_nonoverlapping(
            entry.add(h.val_off as usize),
            kv.add(h.val_off as usize),
            h.val_size as usize,
        );
        let after = (h.len - i - 1) as usize * stride;
        if after > 0 {
            core::ptr::copy(entry.add(stride), entry, after);
        }
        h.len -= 1;
        1
    }
}

/// `copy m` (wolf-lang#384, `[mem.tier0.move.3]`): a fresh map in the
/// AMBIENT region with the same key protocol and a byte copy of every
/// entry, in the same order. Keys are `str`/`int`/`char`/`bool`
/// (`[type.map.key]`), so a key's bytes are the whole key (a `str` key's
/// bytes are immutable and shared, as every `str` copy's are); a value
/// that reaches heap storage is compiled code's to replace.
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_map_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_map_copy(hdr: i64) -> i64 {
    unsafe {
        let h = &*(hdr as *const MapHdr);
        let out = __wolf_rt_map_new(h.key_kind, h.key_size, h.val_off, h.val_size, h.elem);
        if h.len > 0 {
            let o = &mut *(out as *mut MapHdr);
            let n = (h.len * h.elem) as usize;
            let data = alloc_in(o.region, n);
            core::ptr::copy_nonoverlapping(h.data, data, n);
            o.data = data;
            o.len = h.len;
            o.cap = h.len;
            reserve_index(o, o.len);
        }
        out
    }
}

/// `clear` — drop every entry (capacity kept).
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_map_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_map_clear(hdr: i64) {
    unsafe {
        let h = &mut *(hdr as *mut MapHdr);
        h.len = 0;
        clear_index(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::list::ListHdr;

    std::thread_local! {
        pub(super) static COMPARISONS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    }

    fn reset_comparisons() {
        COMPARISONS.with(|n| n.set(0));
    }

    fn bounded_comparisons(n: usize) {
        let comparisons = COMPARISONS.with(|n| n.get());
        assert!(
            comparisons <= 16 * n,
            "{comparisons} key comparisons for {n} probes; bound {}",
            16 * n
        );
    }

    unsafe fn put(m: i64, k: i64, v: i64) {
        let kv = [k, v];
        unsafe { __wolf_rt_map_set(m, kv.as_ptr() as i64) };
    }

    unsafe fn get(m: i64, k: i64) -> Option<i64> {
        let mut kv = [k, -1];
        (unsafe { __wolf_rt_map_get(m, kv.as_mut_ptr() as i64) } != 0).then_some(kv[1])
    }

    unsafe fn ordered_keys(m: i64) -> Vec<i64> {
        unsafe {
            let h = &*(m as *const MapHdr);
            assert_eq!(crate::list::__wolf_rt_list_len(m), h.len, "ListHdr prefix");
            let pairs = __wolf_rt_map_pairs(m);
            (0..h.len)
                .map(|i| {
                    let mut kv = [0i64; 2];
                    assert_eq!(
                        crate::list::__wolf_rt_list_read(pairs, i, kv.as_mut_ptr() as i64),
                        1
                    );
                    let direct = (h.data.add((i * h.elem) as usize) as *const i64).read_unaligned();
                    assert_eq!(kv[0], direct, "pairs snapshot and in-place walk agree");
                    kv[0]
                })
                .collect()
        }
    }

    #[test]
    fn indexed_integer_collisions_survive_rehash() {
        let m = __wolf_rt_map_new(KEY_BYTES, 8, 8, 8, 16);
        unsafe {
            // 2 and 6 collide in the initial 16-slot deterministic index.
            put(m, 2, 20);
            put(m, 6, 60);
            reset_comparisons();
            for k in 0..256 {
                put(m, k, k * 10);
            }
            bounded_comparisons(256);
            reset_comparisons();
            for k in 0..256 {
                assert_eq!(get(m, k), Some(k * 10));
            }
            assert_eq!(get(m, 256), None);
            bounded_comparisons(257);
            let mut order = vec![2, 6];
            order.extend((0..256).filter(|k| *k != 2 && *k != 6));
            assert_eq!(ordered_keys(m), order);
        }
    }

    #[test]
    fn indexed_string_collisions_compare_contents_after_growth() {
        let keys: Vec<Vec<u8>> = (0..256).map(|i| format!("k{i}").into_bytes()).collect();
        let m = __wolf_rt_map_new(KEY_STR, 16, 16, 8, 24);
        unsafe {
            // k2 and k10 collide in the initial 16-slot index.
            for i in [2, 10].into_iter().chain(0..256) {
                let kv = [keys[i].as_ptr() as i64, keys[i].len() as i64, i as i64];
                __wolf_rt_map_set(m, kv.as_ptr() as i64);
            }
            reset_comparisons();
            for (i, key) in keys.iter().enumerate() {
                let other = key.clone();
                let mut kv = [other.as_ptr() as i64, other.len() as i64, -1];
                assert_eq!(__wolf_rt_map_get(m, kv.as_mut_ptr() as i64), 1);
                assert_eq!(kv[2], i as i64);
            }
            bounded_comparisons(256);
        }
    }

    #[test]
    fn indexed_remove_patches_survivors_without_reordering() {
        let m = __wolf_rt_map_new(KEY_BYTES, 8, 8, 8, 16);
        unsafe {
            for k in 0..256 {
                put(m, k, k * 10);
            }
            for k in [0i64, 127, 255] {
                let mut kv = [k, -1];
                assert_eq!(__wolf_rt_map_remove(m, kv.as_mut_ptr() as i64), 1);
                assert_eq!(kv[1], k * 10);
                assert_eq!(__wolf_rt_map_remove(m, kv.as_mut_ptr() as i64), 0);
            }
            let order: Vec<i64> = (0..256).filter(|k| ![0, 127, 255].contains(k)).collect();
            assert_eq!(ordered_keys(m), order);
            reset_comparisons();
            for k in 0..256 {
                assert_eq!(
                    get(m, k),
                    if [0, 127, 255].contains(&k) {
                        None
                    } else {
                        Some(k * 10)
                    }
                );
            }
            bounded_comparisons(256);
            put(m, 127, 99);
            let mut order = order;
            order.push(127);
            assert_eq!(ordered_keys(m), order);
            assert_eq!(get(m, 127), Some(99));
        }
    }

    #[test]
    fn indexed_clear_reuses_storage_and_forgets_old_keys() {
        let m = __wolf_rt_map_new(KEY_BYTES, 8, 8, 8, 16);
        unsafe {
            for k in 0..256 {
                put(m, k, k);
            }
            let data = (*(m as *const MapHdr)).data;
            __wolf_rt_map_clear(m);
            __wolf_rt_map_clear(m);
            for k in 0..256 {
                assert_eq!(get(m, k), None);
            }
            for k in (256..512).rev() {
                put(m, k, k);
            }
            assert_eq!((*(m as *const MapHdr)).data, data);
            assert_eq!(ordered_keys(m), (256..512).rev().collect::<Vec<_>>());
            reset_comparisons();
            for k in 0..512 {
                assert_eq!(get(m, k), (k >= 256).then_some(k));
            }
            bounded_comparisons(512);
        }
    }

    #[test]
    fn indexed_copy_has_independent_storage_and_grows_from_exact_capacity() {
        let m = __wolf_rt_map_new(KEY_BYTES, 8, 8, 8, 16);
        unsafe {
            for k in 0..257 {
                put(m, k, k);
            }
            let c = __wolf_rt_map_copy(m);
            assert_eq!(ordered_keys(c), ordered_keys(m));
            put(c, 257, 257);
            put(c, 0, 999);
            assert_eq!(get(m, 0), Some(0));
            assert_eq!(get(m, 257), None);
            __wolf_rt_map_clear(m);
            reset_comparisons();
            for k in 0..258 {
                assert_eq!(get(c, k), Some(if k == 0 { 999 } else { k }));
            }
            bounded_comparisons(258);
        }
    }

    #[test]
    fn indexed_growth_charges_abandoned_tables_to_birth_region() {
        use crate::native::{
            __wolf_rt_region_ambient_enter as enter, __wolf_rt_region_ambient_leave as leave,
            __wolf_rt_region_free as free, __wolf_rt_region_new as new, region_bytes,
        };
        let birth = new();
        let child = new();
        unsafe {
            let prev = enter(birth);
            let m = __wolf_rt_map_new(KEY_BYTES, 8, 8, 8, 16);
            let saved = enter(child);
            for k in 0..257 {
                put(m, k, k);
            }
            let child_after_growth = region_bytes(child);
            let charged = region_bytes(birth);
            __wolf_rt_map_clear(m);
            for k in 0..257 {
                put(m, k, k);
            }
            let mut kv = [128i64, 0];
            let removed = __wolf_rt_map_remove(m, kv.as_mut_ptr() as i64);
            put(m, 128, 128);
            let refilled = region_bytes(birth);
            let child_after_refill = region_bytes(child);
            leave(saved);
            leave(prev);
            free(child);
            free(birth);
            assert_eq!(removed, 1);
            assert_eq!(
                child_after_growth, 0,
                "growth never charges the ambient child"
            );
            assert_eq!(child_after_refill, 0);
            // 96-byte rounded header, entry capacities 8+...+512,
            // index capacities 16+...+1024, each index slot eight bytes.
            assert_eq!(charged, 96 + 1016 * 16 + 2032 * 8);
            assert_eq!(refilled, charged, "clear, erase and refill charge nothing");
        }
    }

    #[test]
    fn full_index_miss_is_bounded_and_next_insert_grows() {
        let m = __wolf_rt_map_new(KEY_BYTES, 8, 8, 8, 16);
        unsafe {
            for k in 0..16 {
                put(m, k, k);
            }
            let h = &mut *(m as *mut MapHdr);
            // Public mutations keep load <= 1/2. Deliberately use only
            // half the allocated slots to exercise the full-table exit.
            h.index_cap = 16;
            rebuild_index(h);
            reset_comparisons();
            assert_eq!(get(m, 99), None);
            assert_eq!(COMPARISONS.with(|n| n.get()), 16);
            for k in 0..16 {
                assert_eq!(get(m, k), Some(k));
            }
            put(m, 99, 999);
            assert_eq!(get(m, 99), Some(999));
            assert_eq!((*(m as *const MapHdr)).index_cap, 64);
            for k in 0..16 {
                assert_eq!(get(m, k), Some(k));
            }
        }
    }

    #[test]
    fn deterministic_hash_vectors_and_empty_binary_strings() {
        let ints = __wolf_rt_map_new(KEY_BYTES, 8, 8, 8, 16);
        let strings = __wolf_rt_map_new(KEY_STR, 16, 16, 8, 24);
        unsafe {
            let ih = &*(ints as *const MapHdr);
            let sh = &*(strings as *const MapHdr);
            let two = [2i64, 0];
            let six = [6i64, 0];
            assert_eq!(
                key_hash(ih, two.as_ptr().cast()) & 15,
                key_hash(ih, six.as_ptr().cast()) & 15
            );
            let mut buckets = Vec::new();
            for text in [b"k2".as_slice(), b"k10".as_slice()] {
                let kv = [text.as_ptr() as i64, text.len() as i64, 0];
                buckets.push(key_hash(sh, kv.as_ptr().cast()) & 15);
            }
            assert_eq!(buckets[0], buckets[1]);
            // A str and an int also reach the same initial slot, in
            // their own typed maps: the language never mixes key types.
            let k4 = [b"k4".as_ptr() as i64, 2, 0];
            assert_eq!(
                key_hash(ih, two.as_ptr().cast()) & 15,
                key_hash(sh, k4.as_ptr().cast()) & 15
            );
            for (i, text) in [b"".as_slice(), b"a\0b", b"a", "λ".as_bytes()]
                .into_iter()
                .enumerate()
            {
                let kv = [text.as_ptr() as i64, text.len() as i64, i as i64];
                __wolf_rt_map_set(strings, kv.as_ptr() as i64);
                let other = text.to_vec();
                let mut probe = [other.as_ptr() as i64, other.len() as i64, -1];
                assert_eq!(__wolf_rt_map_get(strings, probe.as_mut_ptr() as i64), 1);
                assert_eq!(probe[2], i as i64);
            }
            let mut null_empty = [0i64, 0, -1];
            assert_eq!(
                __wolf_rt_map_get(strings, null_empty.as_mut_ptr() as i64),
                1
            );
            assert_eq!(null_empty[2], 0);
        }
    }

    #[test]
    fn scalar_hash_ignores_entry_padding_and_value_bytes() {
        for size in [1, 4] {
            let m = __wolf_rt_map_new(KEY_BYTES, size, 8, 8, 16);
            let mut kv = [0u8; 16];
            kv[0] = 1;
            kv[8..].copy_from_slice(&42i64.to_ne_bytes());
            unsafe {
                __wolf_rt_map_set(m, kv.as_ptr() as i64);
                kv[size as usize..8].fill(0xff);
                kv[8..].fill(0);
                assert_eq!(__wolf_rt_map_get(m, kv.as_mut_ptr() as i64), 1);
                assert_eq!(i64::from_ne_bytes(kv[8..].try_into().unwrap()), 42);
            }
        }
    }

    /// `copy` (#384): an independent map — same entries, same order,
    /// and a write to either leaves the other alone.
    #[test]
    fn copy_is_independent() {
        let m = __wolf_rt_map_new(KEY_BYTES, 8, 8, 8, 16);
        let entry = |k: i64, v: i64| {
            let mut b = [0u8; 16];
            b[..8].copy_from_slice(&k.to_ne_bytes());
            b[8..].copy_from_slice(&v.to_ne_bytes());
            b
        };
        unsafe {
            let empty = __wolf_rt_map_copy(m);
            assert_eq!((*(empty as *const MapHdr)).len, 0);
            for (k, v) in [(1, 10), (2, 20)] {
                let mut e = entry(k, v);
                __wolf_rt_map_set(m, e.as_mut_ptr() as i64);
            }
            let c = __wolf_rt_map_copy(m);
            let mut e = entry(3, 30);
            __wolf_rt_map_set(c, e.as_mut_ptr() as i64);
            let mut e = entry(1, 11);
            __wolf_rt_map_set(c, e.as_mut_ptr() as i64);
            assert_eq!((*(m as *const MapHdr)).len, 2);
            assert_eq!((*(c as *const MapHdr)).len, 3);
            let mut probe = entry(1, 0);
            assert_eq!(__wolf_rt_map_get(m, probe.as_mut_ptr() as i64), 1);
            assert_eq!(i64::from_ne_bytes(probe[8..].try_into().unwrap()), 10);
        }
    }

    /// `remove` (#344): the hit writes the erased value back, a second
    /// remove misses, and the survivors keep their insertion order.
    #[test]
    fn remove_erases_and_keeps_insertion_order() {
        let m = __wolf_rt_map_new(KEY_BYTES, 8, 8, 8, 16);
        let entry = |k: i64, v: i64| {
            let mut b = [0u8; 16];
            b[..8].copy_from_slice(&k.to_ne_bytes());
            b[8..].copy_from_slice(&v.to_ne_bytes());
            b
        };
        unsafe {
            for (k, v) in [(1, 10), (2, 20), (3, 30)] {
                let mut e = entry(k, v);
                __wolf_rt_map_set(m, e.as_mut_ptr() as i64);
            }
            let mut probe = entry(2, 0);
            assert_eq!(__wolf_rt_map_remove(m, probe.as_mut_ptr() as i64), 1);
            assert_eq!(i64::from_ne_bytes(probe[8..].try_into().unwrap()), 20);
            let mut again = entry(2, 0);
            assert_eq!(__wolf_rt_map_remove(m, again.as_mut_ptr() as i64), 0);
            let h = &*(m as *const MapHdr);
            assert_eq!(h.len, 2);
            let keys: Vec<i64> = (0..h.len)
                .map(|i| (h.data.add(i as usize * 16) as *const i64).read_unaligned())
                .collect();
            assert_eq!(keys, [1, 3], "the survivors keep their insertion order");
            let mut last = entry(3, 0);
            assert_eq!(__wolf_rt_map_remove(m, last.as_mut_ptr() as i64), 1);
            let mut first = entry(1, 0);
            assert_eq!(__wolf_rt_map_remove(m, first.as_mut_ptr() as i64), 1);
            assert_eq!((*(m as *const MapHdr)).len, 0);
        }
    }

    /// One `(int, int)` map: insert, replace, miss, pairs, clear —
    /// the entry layout is two 8-byte words.
    #[test]
    fn int_keys_round_trip() {
        let m = __wolf_rt_map_new(KEY_BYTES, 8, 8, 8, 16);
        unsafe {
            let mut kv: [i64; 2] = [7, 100];
            __wolf_rt_map_set(m, kv.as_mut_ptr() as i64);
            kv = [9, 200];
            __wolf_rt_map_set(m, kv.as_mut_ptr() as i64);
            kv = [7, 300];
            __wolf_rt_map_set(m, kv.as_mut_ptr() as i64);
            assert_eq!((*(m as *const MapHdr)).len, 2);
            kv = [7, 0];
            assert_eq!(__wolf_rt_map_get(m, kv.as_mut_ptr() as i64), 1);
            assert_eq!(kv[1], 300);
            kv = [8, 0];
            assert_eq!(__wolf_rt_map_get(m, kv.as_mut_ptr() as i64), 0);
            let pairs = __wolf_rt_map_pairs(m) as *const ListHdr;
            assert_eq!(crate::list::__wolf_rt_list_len(pairs as i64), 2);
            let mut out = [0i64; 2];
            assert_eq!(
                crate::list::__wolf_rt_list_read(pairs as i64, 1, out.as_mut_ptr() as i64),
                1
            );
            assert_eq!(out, [9, 200]);
            __wolf_rt_map_clear(m);
            assert_eq!((*(m as *const MapHdr)).len, 0);
            kv = [7, 0];
            assert_eq!(__wolf_rt_map_get(m, kv.as_mut_ptr() as i64), 0);
        }
    }

    /// `str` keys compare by the bytes they name, never by pointer:
    /// two copies of `"a"` are one key.
    #[test]
    fn str_keys_compare_by_bytes() {
        let a1 = *b"a";
        let a2 = *b"a";
        let m = __wolf_rt_map_new(KEY_STR, 16, 16, 8, 24);
        unsafe {
            let mut kv: [i64; 3] = [a1.as_ptr() as i64, 1, 5];
            __wolf_rt_map_set(m, kv.as_mut_ptr() as i64);
            kv = [a2.as_ptr() as i64, 1, 0];
            assert_eq!(__wolf_rt_map_get(m, kv.as_mut_ptr() as i64), 1);
            assert_eq!(kv[2], 5);
            let b = *b"b";
            kv = [b.as_ptr() as i64, 1, 0];
            assert_eq!(__wolf_rt_map_get(m, kv.as_mut_ptr() as i64), 0);
        }
    }
}
