//! The s40 native `List` runtime — a region-backed growable buffer.
//!
//! The value form of a `List[T]` at the native tier is ONE pointer to
//! a 40-byte header `{data: *mut u8, len: i64, cap: i64, elem: i64,
//! region: *mut c_void}`
//! living in the ambient region (see [`crate::str`]'s design note for
//! the region story: header and buffer land per `[mem.region.create.3]`,
//! growth copies into a fresh buffer and abandons the old bytes to the
//! arena — regions reclaim wholesale). Elements are flat values of
//! `elem` bytes each, in the v0 packed spill layout lowering already
//! uses for `mut` arguments — an `int` element is 8 bytes, a `str`
//! element is its 16-byte `{ptr, len}` pair, a flat struct is its
//! packed fields.
//!
//! Every entry point takes the header pointer; element traffic goes
//! through caller-owned slots (`elem_ptr` in, `out` out), so compiled
//! code never addresses list memory directly — the WIR token story
//! stays entirely on the caller's own stack slots, and bounds checks
//! stay where the trap identity lives (lowering emits `trap(bounds)`
//! from the miss code; `pop`/`get` misses become `{none}` rows there
//! instead — same runtime entry, two spellings, the D25 posture).
//!
//! # Where a `List` allocates (s76, wolf-lang#81)
//!
//! Both the header and the element buffer land in the AMBIENT REGION at
//! the allocation site — the c08 strbuf rule applied to the container
//! every program reaches for (`[mem.region.create.3]`). The ambient
//! region is DYNAMIC (D12: a callee allocates into its CALLER's
//! region), and [`crate::native::ambient_region`] is the thread's
//! current answer; a null answer means the process root, which is
//! exactly `main`'s enclosing region when no `region` block is open.
//!
//! The header REMEMBERS the region it was born in, so `push`'s growth
//! reallocation lands in the same arena as the original no matter what
//! is ambient when the push happens. Growth abandons the old bytes to
//! the arena; the region reclaims wholesale (`[mem.region.intra.2]`) —
//! so a `region scratch { }` around container work now frees every byte
//! the container used, which is the bug #81 reports.
//!
//! Nothing here keeps a container ALIVE past its region: a `List` that
//! outlives the region it was allocated in is a use-after-free, and
//! rejecting it is `wolf_mem`'s escape analysis (E1010), not the
//! allocator's — see `corpus/memory/region_escape_container.lu`.

use core::ffi::c_void;

use crate::native::{__wolf_rt_region_alloc, ambient_region};
use crate::str::ambient_alloc;

#[repr(C)]
pub(crate) struct ListHdr {
    data: *mut u8,
    len: i64,
    cap: i64,
    elem: i64,
    /// The region this list was born in (null = the process root). Read
    /// only by [`push_raw`]'s growth path: [mem.region.intra.2] frees
    /// the arena as a unit, so a list's bytes must never be split
    /// across two regions with different lifetimes.
    region: *mut c_void,
}

/// Allocate `size` bytes in `region`, or in the process root when
/// `region` is null.
fn alloc_in(region: *mut c_void, size: usize) -> *mut u8 {
    if region.is_null() {
        ambient_alloc(size)
    } else {
        // SAFETY: a non-null slot is a live region handle — lowering
        // brackets `enter`/`leave` on the X4 cleanup chain, so the
        // ambient handle is live for as long as it is ambient, and a
        // header's remembered handle cannot outlive the header (the
        // header lives IN that region).
        unsafe { __wolf_rt_region_alloc(region, size as i64) }
    }
}

pub(crate) fn new_list(elem: usize) -> *mut ListHdr {
    let region = ambient_region();
    let hdr = alloc_in(region, core::mem::size_of::<ListHdr>()) as *mut ListHdr;
    unsafe {
        hdr.write(ListHdr {
            data: core::ptr::null_mut(),
            len: 0,
            cap: 0,
            elem: elem as i64,
            region,
        });
    }
    hdr
}

pub(crate) fn push_raw(hdr: *mut ListHdr, elem_ptr: *const u8) {
    unsafe {
        let h = &mut *hdr;
        if h.len == h.cap {
            let ncap = if h.cap == 0 { 8 } else { h.cap * 2 };
            // The list's OWN region, not whatever is ambient now: a
            // region frees as a unit, so the buffer must not migrate.
            let ndata = alloc_in(h.region, (ncap * h.elem) as usize);
            if h.len > 0 {
                core::ptr::copy_nonoverlapping(h.data, ndata, (h.len * h.elem) as usize);
            }
            h.data = ndata;
            h.cap = ncap;
        }
        core::ptr::copy_nonoverlapping(
            elem_ptr,
            h.data.add((h.len * h.elem) as usize),
            h.elem as usize,
        );
        h.len += 1;
    }
}

/// A `List[byte]` minted from a byte slice at EXACT capacity (s136,
/// wolf-lang#231): one 1-byte-element buffer of `bytes.len()`, one
/// `copy_nonoverlapping`, no growth history. Every byte PRODUCER in
/// the runtime — `str.bytes()`'s materializing fallback, the fs and
/// net byte reads — mints through here, so a 64 KiB read charges the
/// region one header plus 64 KiB and never the doubled buffer the
/// `push`-grown shape charged (`[mem.region.account.1]` keeps
/// abandoned buffers charged; there are none). The list is an
/// ordinary list afterwards: a `push` past `cap` grows it exactly as
/// [`push_raw`] grows any other.
pub(crate) fn from_bytes(bytes: &[u8]) -> *mut ListHdr {
    let hdr = new_list(1);
    if !bytes.is_empty() {
        unsafe {
            let h = &mut *hdr;
            let data = alloc_in(h.region, bytes.len());
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
            h.data = data;
            h.len = bytes.len() as i64;
            h.cap = bytes.len() as i64;
        }
    }
    hdr
}

/// Push one `int` element onto an 8-byte-element list.
pub(crate) fn push_int(hdr: *mut ListHdr, v: i64) {
    let cell = [v];
    push_raw(hdr, cell.as_ptr().cast());
}

/// Copy `s` into the ambient region and push it as a `{ptr, len}`
/// element of a 16-byte-element list — the `List[str]` builder every
/// list-returning builtin shares (`env_args`, `env_vars`,
/// `fs_read_dir`).
pub(crate) fn push_str(hdr: *mut ListHdr, s: &str) {
    let p = crate::str::ambient_copy(s.as_bytes());
    let pair = [p as i64, s.len() as i64];
    push_raw(hdr, pair.as_ptr().cast());
}

/// `List[T]()` — a fresh empty list of `elem_size`-byte elements.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_list_new(elem_size: i64) -> i64 {
    new_list(elem_size.max(1) as usize) as i64
}

/// `push` — append one element (copied from `elem_ptr`).
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_list_new`]; `elem_ptr` must address the
/// list's element size in readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_list_push(hdr: i64, elem_ptr: i64) {
    push_raw(hdr as *mut ListHdr, elem_ptr as *const u8);
}

/// `pop` — 1 with the last element through `out`, or 0 when empty.
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_list_new`]; `out` must address the list's
/// element size in writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_list_pop(hdr: i64, out: i64) -> i64 {
    unsafe {
        let h = &mut *(hdr as *mut ListHdr);
        if h.len == 0 {
            return 0;
        }
        h.len -= 1;
        core::ptr::copy_nonoverlapping(
            h.data.add((h.len * h.elem) as usize),
            out as *mut u8,
            h.elem as usize,
        );
    }
    1
}

/// Element read: 1 with element `idx` through `out`, or 0 out of
/// bounds. The caller decides the miss spelling (trap vs `{none}`).
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_list_new`]; `out` must address the list's
/// element size in writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_list_read(hdr: i64, idx: i64, out: i64) -> i64 {
    unsafe {
        let h = &*(hdr as *const ListHdr);
        if idx < 0 || idx >= h.len {
            return 0;
        }
        core::ptr::copy_nonoverlapping(
            h.data.add((idx * h.elem) as usize),
            out as *mut u8,
            h.elem as usize,
        );
    }
    1
}

/// Element write: 1 after storing through `elem_ptr`, or 0 out of
/// bounds (the caller traps `bounds`).
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_list_new`]; `elem_ptr` must address the
/// list's element size in readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_list_write(hdr: i64, idx: i64, elem_ptr: i64) -> i64 {
    unsafe {
        let h = &mut *(hdr as *mut ListHdr);
        if idx < 0 || idx >= h.len {
            return 0;
        }
        core::ptr::copy_nonoverlapping(
            elem_ptr as *const u8,
            h.data.add((idx * h.elem) as usize),
            h.elem as usize,
        );
    }
    1
}

/// `len` — live element count.
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_list_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_list_len(hdr: i64) -> i64 {
    unsafe { (*(hdr as *const ListHdr)).len }
}

/// `clear` — drop every element (capacity kept).
///
/// # Safety
///
/// `hdr` from [`__wolf_rt_list_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_list_clear(hdr: i64) {
    unsafe { (*(hdr as *mut ListHdr)).len = 0 };
}

/// The elements of a `List[int]` as a plain slice, or `None` when the
/// header is not an 8-byte-element list (s81).
///
/// A shim that consumes a whole list at once — [`crate::str`]'s byte
/// source was the first, until s136 moved the byte tier to
/// [`u8_elems`] — should not pay a `copy_nonoverlapping` per element
/// through [`__wolf_rt_list_read`]'s caller slot. The width CHECK is
/// the point of the `Option`: compiled code cannot reach this with the
/// wrong list (sema types the argument), but a direct FFI caller could
/// hand over a `List[str]` whose 16-byte elements would overrun an
/// 8-byte read slot, and refusing is the only answer that is not
/// undefined behaviour. No shim consumes a whole `List[int]` today
/// (`os_random` only PRODUCES one), so this is the unit tests' reader.
///
/// # Safety
///
/// `hdr` must be a live header from [`__wolf_rt_list_new`], and the
/// returned slice borrows its buffer — it must not outlive a `push`
/// that reallocates.
#[cfg(test)]
pub(crate) unsafe fn i64_elems<'a>(hdr: i64) -> Option<&'a [i64]> {
    let h = unsafe { &*(hdr as *const ListHdr) };
    if h.elem != 8 || h.len < 0 {
        return None;
    }
    if h.len == 0 || h.data.is_null() {
        return Some(&[]);
    }
    Some(unsafe { core::slice::from_raw_parts(h.data.cast::<i64>(), h.len as usize) })
}

/// The elements of a `List[byte]` (s136, D72: 1-byte octet elements)
/// as a plain byte slice — [`i64_elems`]'s posture at the `byte`
/// width. Every byte CONSUMER in the runtime (`str_from_utf8`, the fs
/// and net byte writes) reads through here; a header of any other
/// element width is refused (`None`), which is the FFI caller's
/// `invalid` — compiled code cannot produce one, sema types the
/// argument `List[byte]`.
///
/// # Safety
///
/// `hdr` must be a live header from [`__wolf_rt_list_new`], and the
/// returned slice borrows its buffer — it must not outlive a `push`
/// that reallocates.
pub(crate) unsafe fn u8_elems<'a>(hdr: i64) -> Option<&'a [u8]> {
    let h = unsafe { &*(hdr as *const ListHdr) };
    if h.elem != 1 || h.len < 0 {
        return None;
    }
    if h.len == 0 || h.data.is_null() {
        return Some(&[]);
    }
    Some(unsafe { core::slice::from_raw_parts(h.data, h.len as usize) })
}

/// The elements of a `List[char]` (s121, D58: 4-byte scalar elements,
/// native-endian) — [`i64_elems`]'s posture at the `char` width. The
/// buffer's alignment is the allocator's (≥ 8), so the `u32` cast is
/// always aligned.
///
/// # Safety
///
/// `hdr` must be a live header from [`__wolf_rt_list_new`], and the
/// returned slice borrows its buffer — it must not outlive a `push`
/// that reallocates.
#[cfg(test)]
pub(crate) unsafe fn u32_elems<'a>(hdr: i64) -> Option<&'a [u32]> {
    let h = unsafe { &*(hdr as *const ListHdr) };
    if h.elem != 4 || h.len < 0 {
        return None;
    }
    if h.len == 0 || h.data.is_null() {
        return Some(&[]);
    }
    Some(unsafe { core::slice::from_raw_parts(h.data.cast::<u32>(), h.len as usize) })
}

/// The elements of a `List[str]` as `{ptr, len}` pairs, or `None` when
/// the header is not a 16-byte-element list — [`i64_elems`]'s posture
/// (s81) for the second whole-list consumer, `os_spawn`'s argv (s107):
/// the width CHECK keeps a direct FFI caller's 8-byte elements from
/// being misread as pointer halves.
///
/// # Safety
///
/// `hdr` must be a live header from [`__wolf_rt_list_new`], and the
/// returned slice borrows its buffer — it must not outlive a `push`
/// that reallocates.
pub(crate) unsafe fn str_pair_elems<'a>(hdr: i64) -> Option<&'a [[i64; 2]]> {
    let h = unsafe { &*(hdr as *const ListHdr) };
    if h.elem != 16 || h.len < 0 {
        return None;
    }
    if h.len == 0 || h.data.is_null() {
        return Some(&[]);
    }
    Some(unsafe { core::slice::from_raw_parts(h.data.cast::<[i64; 2]>(), h.len as usize) })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// s75: compiled code addresses `data` and `len` DIRECTLY —
    /// element access is `ptr.off` + `load`, not a shim call — so
    /// these two offsets are part of the runtime ABI, not a private
    /// layout. `wolf_wir::lower`'s `LIST_DATA_OFF` / `LIST_LEN_OFF`
    /// are the compiler's copy; move a field here and this test is
    /// what tells you the lowering moved with it.
    #[test]
    fn header_offsets_are_the_lowering_abi() {
        let hdr = ListHdr {
            data: core::ptr::null_mut(),
            len: 0,
            cap: 0,
            elem: 0,
            region: core::ptr::null_mut(),
        };
        let base = (&raw const hdr) as usize;
        assert_eq!((&raw const hdr.data) as usize - base, 0, "LIST_DATA_OFF");
        assert_eq!((&raw const hdr.len) as usize - base, 8, "LIST_LEN_OFF");
        // s76 appended `region` — the two lowering-visible offsets are
        // unchanged, which is the whole point of appending.
        assert_eq!(core::mem::size_of::<ListHdr>(), 40);
        assert_eq!(core::mem::align_of::<ListHdr>(), 8);
    }

    // ------------------------------- s76: where a List allocates ----

    /// Target 1: header AND element buffer land in the region that was
    /// ambient at the allocation site. Asserted with the runtime's own
    /// per-region ledger (`region_bytes`), which moves for exactly one
    /// reason — no RSS, no noise.
    #[test]
    fn header_and_buffer_land_in_the_ambient_region() {
        let r = crate::native::__wolf_rt_region_new();
        unsafe {
            let prev = crate::native::__wolf_rt_region_ambient_enter(r);
            assert!(prev.is_null(), "a test thread starts at the process root");
            assert_eq!(crate::native::region_bytes(r), 0);
            let h = __wolf_rt_list_new(8);
            let after_hdr = crate::native::region_bytes(r);
            assert!(after_hdr >= 40, "the header is region storage: {after_hdr}");
            for v in 0..4096i64 {
                __wolf_rt_list_push(h, (&raw const v) as i64);
            }
            let after_buf = crate::native::region_bytes(r);
            assert!(
                after_buf >= after_hdr + 4096 * 8,
                "the element buffer is region storage too: {after_hdr} -> {after_buf}"
            );
            crate::native::__wolf_rt_region_ambient_leave(prev);
            crate::native::__wolf_rt_region_free(r);
        }
    }

    /// No ambient region ⇒ the process root, exactly as before s76: a
    /// program with no `region` block is unchanged.
    #[test]
    fn no_ambient_region_means_the_process_root() {
        let r = crate::native::__wolf_rt_region_new();
        unsafe {
            assert!(crate::native::ambient_region().is_null());
            let h = __wolf_rt_list_new(8);
            for v in 0..1024i64 {
                __wolf_rt_list_push(h, (&raw const v) as i64);
            }
            assert_eq!(
                crate::native::region_bytes(r),
                0,
                "nothing charged to a region that was never ambient"
            );
            assert_eq!(__wolf_rt_list_len(h), 1024);
            crate::native::__wolf_rt_region_free(r);
        }
    }

    /// Target 2: growth stays in the region the list was BORN in, even
    /// when a different region is ambient at the `push`. A region frees
    /// as a unit ([mem.region.intra.2]), so a list whose header and
    /// buffer straddled two regions would be a dangling read waiting to
    /// happen.
    #[test]
    fn growth_stays_in_the_birth_region() {
        let a = crate::native::__wolf_rt_region_new();
        let b = crate::native::__wolf_rt_region_new();
        unsafe {
            let outer = crate::native::__wolf_rt_region_ambient_enter(a);
            let h = __wolf_rt_list_new(8);
            let v = 1i64;
            __wolf_rt_list_push(h, (&raw const v) as i64);
            let a_before = crate::native::region_bytes(a);
            crate::native::__wolf_rt_region_ambient_leave(outer);

            let outer_b = crate::native::__wolf_rt_region_ambient_enter(b);
            for v in 0..4096i64 {
                __wolf_rt_list_push(h, (&raw const v) as i64);
            }
            crate::native::__wolf_rt_region_ambient_leave(outer_b);

            assert_eq!(
                crate::native::region_bytes(b),
                0,
                "growth must not migrate into whatever is ambient at the push"
            );
            assert!(
                crate::native::region_bytes(a) > a_before + 4096 * 8,
                "growth belongs to the birth region"
            );
            // The elements survived the reallocation chain.
            let mut out = [0i64; 1];
            assert_eq!(__wolf_rt_list_len(h), 4097);
            assert_eq!(__wolf_rt_list_read(h, 4096, out.as_mut_ptr() as i64), 1);
            assert_eq!(out[0], 4095);
            crate::native::__wolf_rt_region_free(b);
            crate::native::__wolf_rt_region_free(a);
        }
    }

    /// Target 4, the `move` half: a region holding a container crosses
    /// a spawn boundary (s34's transfer/adopt ledger seam) and stays
    /// usable. Both seams are identity on the handle, which is exactly
    /// why a header's remembered region survives the move — assert it
    /// rather than argue it.
    #[test]
    // The transfer/adopt pair lives in the task layer (linux at s28,
    // macOS since s59); the rest of this file is portable everywhere.
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn a_moved_region_keeps_its_container() {
        let a = crate::native::__wolf_rt_region_new();
        unsafe {
            let prev = crate::native::__wolf_rt_region_ambient_enter(a);
            let h = __wolf_rt_list_new(8);
            for v in 0..64i64 {
                __wolf_rt_list_push(h, (&raw const v) as i64);
            }
            crate::native::__wolf_rt_region_ambient_leave(prev);

            let moved = crate::task::__wolf_rt_region_transfer(a);
            let adopted = crate::task::__wolf_rt_region_adopt(moved);
            assert_eq!(adopted, a, "transfer/adopt are identity on the handle");

            // Growth after the move still lands in the same arena.
            let before = crate::native::region_bytes(adopted);
            for v in 64..4096i64 {
                __wolf_rt_list_push(h, (&raw const v) as i64);
            }
            assert!(crate::native::region_bytes(adopted) > before);
            let mut out = [0i64; 1];
            assert_eq!(__wolf_rt_list_len(h), 4096);
            assert_eq!(__wolf_rt_list_read(h, 4095, out.as_mut_ptr() as i64), 1);
            assert_eq!(out[0], 4095);
            crate::native::__wolf_rt_region_free(adopted);
        }
    }

    /// Nested regions restore the enclosing one on the way out — the
    /// save/restore discipline lowering emits on the X4 cleanup chain.
    #[test]
    fn nested_regions_restore_the_enclosing_ambient() {
        let outer = crate::native::__wolf_rt_region_new();
        let inner = crate::native::__wolf_rt_region_new();
        unsafe {
            let p0 = crate::native::__wolf_rt_region_ambient_enter(outer);
            let a = __wolf_rt_list_new(8);
            let p1 = crate::native::__wolf_rt_region_ambient_enter(inner);
            let b = __wolf_rt_list_new(8);
            let v = 7i64;
            __wolf_rt_list_push(b, (&raw const v) as i64);
            crate::native::__wolf_rt_region_ambient_leave(p1);
            assert_eq!(crate::native::ambient_region(), outer);
            // The inner region's list dies with it; the outer list does
            // not (this is the shape the checker enforces statically).
            let inner_bytes = crate::native::region_bytes(inner);
            crate::native::__wolf_rt_region_free(inner);
            assert!(inner_bytes >= 40);
            __wolf_rt_list_push(a, (&raw const v) as i64);
            assert_eq!(__wolf_rt_list_len(a), 1);
            crate::native::__wolf_rt_region_ambient_leave(p0);
            assert!(crate::native::ambient_region().is_null());
            crate::native::__wolf_rt_region_free(outer);
        }
    }

    #[test]
    fn push_pop_read_write_len() {
        let h = __wolf_rt_list_new(8);
        let mut out = [0i64; 1];
        unsafe {
            assert_eq!(__wolf_rt_list_len(h), 0);
            assert_eq!(__wolf_rt_list_pop(h, out.as_mut_ptr() as i64), 0);
            for v in [10i64, 20, 30] {
                __wolf_rt_list_push(h, (&raw const v) as i64);
            }
            assert_eq!(__wolf_rt_list_len(h), 3);
            assert_eq!(__wolf_rt_list_read(h, 1, out.as_mut_ptr() as i64), 1);
            assert_eq!(out[0], 20);
            assert_eq!(__wolf_rt_list_read(h, 3, out.as_mut_ptr() as i64), 0);
            let nv = 99i64;
            assert_eq!(__wolf_rt_list_write(h, 0, (&raw const nv) as i64), 1);
            assert_eq!(__wolf_rt_list_pop(h, out.as_mut_ptr() as i64), 1);
            assert_eq!(out[0], 30);
            __wolf_rt_list_clear(h);
            assert_eq!(__wolf_rt_list_len(h), 0);
        }
    }

    #[test]
    fn growth_preserves_elements() {
        let h = __wolf_rt_list_new(8);
        let mut out = [0i64; 1];
        unsafe {
            for v in 0..100i64 {
                __wolf_rt_list_push(h, (&raw const v) as i64);
            }
            assert_eq!(__wolf_rt_list_len(h), 100);
            for i in 0..100i64 {
                assert_eq!(__wolf_rt_list_read(h, i, out.as_mut_ptr() as i64), 1);
                assert_eq!(out[0], i);
            }
        }
    }
}
