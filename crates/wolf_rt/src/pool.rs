//! The native `Pool[T]` runtime — a region-backed generational slot
//! arena (X5, `[mem.shared.handle.1]` / `[mem.shared.handle.2]`).
//!
//! The value form of a `Pool[T]` at the native tier is ONE pointer to
//! a header, exactly as a `List[T]` is (see [`crate::list`] for the
//! region story, which this module shares in full: header and slot
//! buffers land in the AMBIENT region at the allocation site, growth
//! copies into a fresh buffer in the pool's OWN region, and the
//! region reclaims wholesale).
//!
//! # The handle
//!
//! `handle T` is ONE 64-bit word: the slot index in the low 32 bits
//! and the slot's generation in the high 32. That is the whole value
//! — a handle is `Copy`, it stores in a struct field, it rides a
//! `List[handle T]`, and it carries no pointer, which is the property
//! the type exists for. It is NOT an address: a handle outlives the
//! byte it named and says so, where a pointer outlives it and lies.
//!
//! ```text
//!   63                     32 31                      0
//!  +-------------------------+-------------------------+
//!  |       generation        |          index          |
//!  +-------------------------+-------------------------+
//! ```
//!
//! # Why the generation is not the debug tag
//!
//! This generation is LANGUAGE-VISIBLE and its staleness is a
//! **defined trap** in every profile (`trap(stale-handle)`), in
//! release builds too. [`crate::quarantine`]'s tag is an invisible
//! checked-build artifact for the unsafe tier and reports `ub`. The
//! two layers are deliberately separate; the confusion is common
//! enough that both modules say so.
//!
//! # Two-phase creation, and why there is no null handle
//!
//! `reserve` hands back a handle to an UNINITIALIZED live slot;
//! `init` fills it. The pair exists so a cyclic structure can name
//! its own nodes before they exist — the doubly-linked ring in
//! `corpus/regions.lu` is the shape — without any `Option`, any null,
//! and any moment at which a handle does not denote a slot. A read of
//! a reserved-but-uninitialized slot is not detectable here and is
//! `wolf_mem`'s definite-initialization problem, exactly as it is for
//! a local.
//!
//! # Slot liveness
//!
//! `remove` bumps the slot's generation and marks it dead, so every
//! extant handle to it goes stale at once (X5). A dead slot is pushed
//! on a free list and the NEXT `reserve` takes it — at its new
//! generation, so a handle from the previous tenant still faults. The
//! generation is 32 bits and wraps; a slot would have to be recycled
//! 2^32 times for a stale handle to be mistaken for a live one, and
//! that is the documented bound, not an accident.
//!
//! Every entry point takes the header pointer and element traffic
//! goes through caller-owned slots, as the list's does — except
//! [`__wolf_rt_pool_addr`], which hands compiled code the ADDRESS of
//! a live slot's payload so a place write (`pool[h].next = k`,
//! wolf-lang#31) can store through it without a read-modify-write of
//! the whole element. A stale handle answers 0 there, and lowering
//! turns the zero into `trap(stale-handle)` — the miss code carries
//! the trap identity, the D25 posture the list already follows.

use core::ffi::c_void;

use crate::list::alloc_in;
use crate::native::ambient_region;

/// One slot's bookkeeping, kept out of the payload buffer so the
/// payload stays flat and an element address is the payload address.
#[repr(C)]
#[derive(Clone, Copy)]
struct Slot {
    generation: i64,
    /// 1 live, 0 dead. Wider than a bool so the struct has no padding
    /// story to get wrong across the FFI boundary.
    live: i64,
}

#[repr(C)]
pub(crate) struct PoolHdr {
    /// The payload buffer: `cap * elem` bytes.
    data: *mut u8,
    /// The slot table: `cap` entries.
    slots: *mut Slot,
    /// Slots ever reserved (live or dead) — the high-water mark, NOT
    /// the live count.
    used: i64,
    cap: i64,
    elem: i64,
    /// Live slots. `len()` answers this.
    live: i64,
    /// Head of the dead-slot free list, or -1. The list threads
    /// through the payload buffer's first 8 bytes of each dead slot,
    /// which nothing may read while the slot is dead.
    free_head: i64,
    /// The region this pool was born in (null = the process root),
    /// remembered for the same reason a list remembers it: growth
    /// must not split a pool's bytes across two lifetimes.
    region: *mut c_void,
}

/// Pack a handle. Kept in one place so the compiler's own packing
/// (`wolf_wir`'s `handle_pack`) has exactly one definition to agree
/// with.
#[inline]
fn pack(index: i64, generation: i64) -> i64 {
    ((generation & 0xFFFF_FFFF) << 32) | (index & 0xFFFF_FFFF)
}

#[inline]
fn unpack(h: i64) -> (i64, i64) {
    (h & 0xFFFF_FFFF, (h >> 32) & 0xFFFF_FFFF)
}

pub(crate) fn new_pool(elem: usize) -> *mut PoolHdr {
    let region = ambient_region();
    let hdr = alloc_in(region, core::mem::size_of::<PoolHdr>()) as *mut PoolHdr;
    // SAFETY: `alloc_in` answers a fresh region allocation of exactly
    // this size; nothing else addresses it yet.
    unsafe {
        hdr.write(PoolHdr {
            data: core::ptr::null_mut(),
            slots: core::ptr::null_mut(),
            used: 0,
            cap: 0,
            elem: elem as i64,
            live: 0,
            free_head: -1,
            region,
        });
    }
    hdr
}

/// Grow to at least `want` slots, copying both buffers into the
/// pool's own region. Abandons the old bytes to the arena, as the
/// list's growth does.
unsafe fn grow(h: &mut PoolHdr, want: i64) {
    if want <= h.cap {
        return;
    }
    let ncap = if h.cap == 0 {
        want.max(8)
    } else {
        let mut c = h.cap;
        while c < want {
            c *= 2;
        }
        c
    };
    unsafe {
        let ndata = alloc_in(h.region, (ncap * h.elem) as usize);
        let nslots = alloc_in(h.region, ncap as usize * core::mem::size_of::<Slot>()) as *mut Slot;
        if h.used > 0 {
            core::ptr::copy_nonoverlapping(h.data, ndata, (h.used * h.elem) as usize);
            core::ptr::copy_nonoverlapping(h.slots, nslots, h.used as usize);
        }
        // Fresh slots start dead at generation 0; `reserve` is what
        // makes one live, so a half-grown table never reads live.
        for i in h.used..ncap {
            nslots.offset(i as isize).write(Slot {
                generation: 0,
                live: 0,
            });
        }
        h.data = ndata;
        h.slots = nslots;
        h.cap = ncap;
    }
}

/// The address of slot `index`'s payload. The caller has already
/// established that `index` is in range.
#[inline]
unsafe fn payload(h: &PoolHdr, index: i64) -> *mut u8 {
    unsafe { h.data.add((index * h.elem) as usize) }
}

/// Is this handle live in this pool?
unsafe fn resolve(h: &PoolHdr, handle: i64) -> Option<i64> {
    let (index, generation) = unpack(handle);
    if index < 0 || index >= h.used {
        return None;
    }
    // SAFETY: `index` is below `used`, which is below `cap`, so the
    // slot table entry exists.
    let s = unsafe { *h.slots.offset(index as isize) };
    if s.live == 0 || s.generation != generation {
        return None;
    }
    Some(index)
}

/// `Pool[T]()` — a fresh empty pool of `elem_size`-byte payloads.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_pool_new(elem_size: i64) -> i64 {
    new_pool(elem_size.max(1) as usize) as i64
}

/// `reserve` — a live, uninitialized slot and the handle naming it.
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_pool_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_pool_reserve(hdr: i64) -> i64 {
    unsafe {
        let h = &mut *(hdr as *mut PoolHdr);
        // A dead slot first: reuse is what keeps a long-lived pool
        // bounded by its live set rather than by its history.
        if h.free_head >= 0 {
            let index = h.free_head;
            let next = *(payload(h, index) as *const i64);
            h.free_head = next;
            let s = &mut *h.slots.offset(index as isize);
            s.live = 1;
            h.live += 1;
            return pack(index, s.generation);
        }
        if h.used == h.cap {
            grow(h, h.used + 1);
        }
        let index = h.used;
        h.used += 1;
        let s = &mut *h.slots.offset(index as isize);
        s.generation = 0;
        s.live = 1;
        h.live += 1;
        pack(index, 0)
    }
}

/// The ADDRESS of the live slot `handle` names, or 0 when the handle
/// is stale. Lowering turns the zero into `trap(stale-handle)`.
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_pool_new`]. The address is valid until the
/// pool grows or the slot is removed; compiled code uses it within
/// one expression, exactly as it uses a list element address.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_pool_addr(hdr: i64, handle: i64) -> i64 {
    unsafe {
        let h = &*(hdr as *const PoolHdr);
        match resolve(h, handle) {
            Some(index) => payload(h, index) as i64,
            None => 0,
        }
    }
}

/// `init` / a whole-element write through a handle: 1 after storing
/// `elem_ptr`'s bytes into the slot, 0 when the handle is stale.
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_pool_new`]; `elem_ptr` must address the
/// pool's element size in readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_pool_write(hdr: i64, handle: i64, elem_ptr: i64) -> i64 {
    unsafe {
        let h = &mut *(hdr as *mut PoolHdr);
        let Some(index) = resolve(h, handle) else {
            return 0;
        };
        core::ptr::copy_nonoverlapping(elem_ptr as *const u8, payload(h, index), h.elem as usize);
    }
    1
}

/// A whole-element read through a handle: 1 with the payload through
/// `out`, 0 when the handle is stale.
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_pool_new`]; `out` must address the pool's
/// element size in writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_pool_read(hdr: i64, handle: i64, out: i64) -> i64 {
    unsafe {
        let h = &*(hdr as *const PoolHdr);
        let Some(index) = resolve(h, handle) else {
            return 0;
        };
        core::ptr::copy_nonoverlapping(payload(h, index), out as *mut u8, h.elem as usize);
    }
    1
}

/// `remove` — free the slot and bump its generation so every extant
/// handle to it goes stale. 1 on success, 0 when the handle is
/// already stale (lowering traps on the zero).
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_pool_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_pool_remove(hdr: i64, handle: i64) -> i64 {
    unsafe {
        let h = &mut *(hdr as *mut PoolHdr);
        let Some(index) = resolve(h, handle) else {
            return 0;
        };
        {
            let s = &mut *h.slots.offset(index as isize);
            s.live = 0;
            // Wrapping is the documented bound, not an accident: see
            // the module header.
            s.generation = s.generation.wrapping_add(1) & 0xFFFF_FFFF;
        }
        h.live -= 1;
        // Thread the free list through the dead slot's payload. Only
        // a dead slot's bytes are used this way, and nothing may read
        // a dead slot.
        if h.elem >= 8 {
            *(payload(h, index) as *mut i64) = h.free_head;
            h.free_head = index;
        }
    }
    1
}

/// `alive` — the non-trapping liveness probe (s37, wolf-lang#11).
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_pool_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_pool_alive(hdr: i64, handle: i64) -> i64 {
    unsafe {
        let h = &*(hdr as *const PoolHdr);
        i64::from(resolve(h, handle).is_some())
    }
}

/// `len` — the LIVE slot count, which is what the checked lane
/// counts. Not `used`: a pool that reserved a thousand slots and
/// removed them all is empty.
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_pool_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_pool_len(hdr: i64) -> i64 {
    unsafe { (*(hdr as *const PoolHdr)).live }
}

/// `capacity` — slots the pool can hold before it grows again (D50's
/// accessor set).
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_pool_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_pool_capacity(hdr: i64) -> i64 {
    unsafe { (*(hdr as *const PoolHdr)).cap }
}

/// `clear` — remove every live slot, bumping each generation, so
/// every handle the pool ever issued is stale afterwards. Capacity is
/// kept.
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_pool_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_pool_clear(hdr: i64) {
    unsafe {
        let h = &mut *(hdr as *mut PoolHdr);
        for index in 0..h.used {
            let s = &mut *h.slots.offset(index as isize);
            if s.live == 0 {
                continue;
            }
            s.live = 0;
            s.generation = s.generation.wrapping_add(1) & 0xFFFF_FFFF;
            if h.elem >= 8 {
                *(payload(h, index) as *mut i64) = h.free_head;
                h.free_head = index;
            }
        }
        h.live = 0;
    }
}

/// The handle naming the live slot at or after `from` in reservation
/// order, or 0 when there is none. Iteration is a scan over the slot
/// table rather than a protocol object, so `for` over a pool costs
/// one word of state and no allocation.
///
/// 0 is unambiguous as "none" here ONLY because it would otherwise
/// mean index 0 at generation 0, and this entry point answers the
/// handle SHIFTED: the caller sees `pack(index, gen) | LIVE_BIT`.
/// See [`POOL_ITER_LIVE`].
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_pool_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_pool_next(hdr: i64, from: i64) -> i64 {
    unsafe {
        let h = &*(hdr as *const PoolHdr);
        let mut index = from.max(0);
        while index < h.used {
            let s = *h.slots.offset(index as isize);
            if s.live != 0 {
                return pack(index, s.generation) | POOL_ITER_LIVE;
            }
            index += 1;
        }
        0
    }
}

/// The bit [`__wolf_rt_pool_next`] sets so that "no more slots" (0)
/// is distinguishable from the perfectly ordinary handle
/// `{index: 0, generation: 0}`. Lowering masks it off.
pub const POOL_ITER_LIVE: i64 = 1 << 62;

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(elem: usize) -> i64 {
        __wolf_rt_pool_new(elem as i64)
    }

    #[test]
    fn a_handle_packs_index_and_generation() {
        assert_eq!(unpack(pack(3, 7)), (3, 7));
        assert_eq!(unpack(pack(0, 0)), (0, 0));
        assert_eq!(
            unpack(pack(0xFFFF_FFFF, 0xFFFF_FFFF)),
            (0xFFFF_FFFF, 0xFFFF_FFFF)
        );
    }

    #[test]
    fn reserve_init_read_round_trips() {
        unsafe {
            let p = pool(8);
            let h = __wolf_rt_pool_reserve(p);
            let v: i64 = 42;
            assert_eq!(__wolf_rt_pool_write(p, h, &v as *const i64 as i64), 1);
            let mut out: i64 = 0;
            assert_eq!(__wolf_rt_pool_read(p, h, &mut out as *mut i64 as i64), 1);
            assert_eq!(out, 42);
            assert_eq!(__wolf_rt_pool_len(p), 1);
        }
    }

    #[test]
    fn a_removed_handle_is_stale_everywhere() {
        unsafe {
            let p = pool(8);
            let h = __wolf_rt_pool_reserve(p);
            let v: i64 = 1;
            __wolf_rt_pool_write(p, h, &v as *const i64 as i64);
            assert_eq!(__wolf_rt_pool_remove(p, h), 1);
            assert_eq!(__wolf_rt_pool_alive(p, h), 0);
            assert_eq!(__wolf_rt_pool_addr(p, h), 0);
            let mut out: i64 = 0;
            assert_eq!(__wolf_rt_pool_read(p, h, &mut out as *mut i64 as i64), 0);
            assert_eq!(__wolf_rt_pool_write(p, h, &v as *const i64 as i64), 0);
            assert_eq!(__wolf_rt_pool_remove(p, h), 0, "a second remove is stale");
            assert_eq!(__wolf_rt_pool_len(p), 0);
        }
    }

    #[test]
    fn a_reused_slot_does_not_answer_the_old_handle() {
        unsafe {
            let p = pool(8);
            let first = __wolf_rt_pool_reserve(p);
            __wolf_rt_pool_remove(p, first);
            let second = __wolf_rt_pool_reserve(p);
            assert_eq!(unpack(first).0, unpack(second).0, "the slot is reused");
            assert_ne!(first, second, "at a new generation");
            assert_eq!(__wolf_rt_pool_alive(p, first), 0);
            assert_eq!(__wolf_rt_pool_alive(p, second), 1);
        }
    }

    #[test]
    fn the_address_is_the_payload_and_a_place_write_lands_in_it() {
        unsafe {
            let p = pool(16);
            let h = __wolf_rt_pool_reserve(p);
            let payload = [7i64, 9i64];
            __wolf_rt_pool_write(p, h, payload.as_ptr() as i64);
            let addr = __wolf_rt_pool_addr(p, h);
            assert_ne!(addr, 0);
            // The second field, written in place — `pool[h].next = k`.
            *((addr + 8) as *mut i64) = 11;
            let mut out = [0i64, 0i64];
            __wolf_rt_pool_read(p, h, out.as_mut_ptr() as i64);
            assert_eq!(out, [7, 11]);
        }
    }

    #[test]
    fn growth_keeps_every_live_handle_valid() {
        unsafe {
            let p = pool(8);
            let mut hs = Vec::new();
            for i in 0..100i64 {
                let h = __wolf_rt_pool_reserve(p);
                __wolf_rt_pool_write(p, h, &i as *const i64 as i64);
                hs.push(h);
            }
            assert!(__wolf_rt_pool_capacity(p) >= 100);
            assert_eq!(__wolf_rt_pool_len(p), 100);
            for (i, h) in hs.iter().enumerate() {
                let mut out: i64 = -1;
                assert_eq!(__wolf_rt_pool_read(p, *h, &mut out as *mut i64 as i64), 1);
                assert_eq!(out, i as i64);
            }
        }
    }

    #[test]
    fn clear_stales_every_handle_and_keeps_capacity() {
        unsafe {
            let p = pool(8);
            let a = __wolf_rt_pool_reserve(p);
            let b = __wolf_rt_pool_reserve(p);
            let cap = __wolf_rt_pool_capacity(p);
            __wolf_rt_pool_clear(p);
            assert_eq!(__wolf_rt_pool_len(p), 0);
            assert_eq!(__wolf_rt_pool_alive(p, a), 0);
            assert_eq!(__wolf_rt_pool_alive(p, b), 0);
            assert_eq!(__wolf_rt_pool_capacity(p), cap);
        }
    }

    #[test]
    fn iteration_walks_the_live_slots_in_reservation_order() {
        unsafe {
            let p = pool(8);
            let a = __wolf_rt_pool_reserve(p);
            let b = __wolf_rt_pool_reserve(p);
            let c = __wolf_rt_pool_reserve(p);
            __wolf_rt_pool_remove(p, b);
            let mut seen = Vec::new();
            let mut cur = __wolf_rt_pool_next(p, 0);
            while cur != 0 {
                let h = cur & !POOL_ITER_LIVE;
                seen.push(h);
                cur = __wolf_rt_pool_next(p, unpack(h).0 + 1);
            }
            assert_eq!(seen, vec![a, c]);
        }
    }

    #[test]
    fn an_empty_pool_iterates_zero_times() {
        unsafe {
            let p = pool(8);
            assert_eq!(__wolf_rt_pool_next(p, 0), 0);
            assert_eq!(__wolf_rt_pool_len(p), 0);
        }
    }

    #[test]
    fn slot_zero_at_generation_zero_is_not_the_end_of_iteration() {
        // The whole reason `next` sets a bit: `pack(0, 0)` is 0.
        unsafe {
            let p = pool(8);
            let a = __wolf_rt_pool_reserve(p);
            assert_eq!(a, 0, "the first handle really is the all-zero word");
            let first = __wolf_rt_pool_next(p, 0);
            assert_ne!(first, 0, "and iteration must not read it as the end");
            assert_eq!(first & !POOL_ITER_LIVE, a);
        }
    }
}
