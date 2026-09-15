//! The s152 native `Map` runtime — a keyed store over the `List`
//! buffer discipline (`[type.map]`, `[mem.map.absent]`).
//!
//! The value form of a `Map[K, V]` at the native tier is ONE pointer to
//! a header whose first five words ARE a [`crate::list::ListHdr`] —
//! `{data, len, cap, elem, region}` at the same offsets — so lowering
//! reads the entry count through the list header's `len` slot and the
//! region story is the list's word for word (`[mem.region.create.3]`:
//! header and buffer land in the ambient region at the constructor,
//! growth stays in the birth region). Four words follow: how keys
//! compare, the key's byte size, the value's offset inside an entry,
//! and the value's byte size.
//!
//! An ENTRY is the `(K, V)` tuple's flat layout — the key's bytes at
//! offset 0, the value's at `val_off`, `elem` bytes per entry (the
//! tuple's list stride) — so `pairs()` is a byte copy of the entry
//! buffer into a fresh list of that stride and compiled code loads
//! each element as the tuple it already knows how to load. Entries
//! keep INSERTION ORDER: the clause leaves iteration order
//! unspecified, the reference interpreter answers insertion order
//! (`Value::Map(Vec<(Value, Slot)>)`), and both machines agreeing is
//! worth more than a hash table's constant factor for the maps this
//! edition serves (four key types, the count exercise's dozen entries).
//! Lookup is a linear scan; a hashed store behind the same five entry
//! points is a runtime change and no clause moves.
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
}

const KEY_BYTES: i64 = 0;
const KEY_STR: i64 = 1;

/// Do the keys at `a` and `b` name the same key?
unsafe fn key_eq(h: &MapHdr, a: *const u8, b: *const u8) -> bool {
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

/// The entry whose key equals the key at `key_ptr`, by index.
unsafe fn find(h: &MapHdr, key_ptr: *const u8) -> Option<i64> {
    unsafe {
        for i in 0..h.len {
            let entry = h.data.add((i * h.elem) as usize);
            if key_eq(h, entry, key_ptr) {
                return Some(i);
            }
        }
        None
    }
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
        core::ptr::copy_nonoverlapping(kv, h.data.add((h.len * h.elem) as usize), h.elem as usize);
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
/// order the remaining keys were first bound in. O(n) in the entries
/// after the erased one; nothing is freed (the buffer's capacity and
/// its birth region are the map's, as for a list's `pop`).
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
        (*(hdr as *mut MapHdr)).len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::list::ListHdr;

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
