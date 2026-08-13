//! The s40 native `List` runtime — a region-backed growable buffer.
//!
//! The value form of a `List[T]` at the native tier is ONE pointer to
//! a 32-byte header `{data: *mut u8, len: i64, cap: i64, elem: i64}`
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

use crate::str::ambient_alloc;

#[repr(C)]
pub(crate) struct ListHdr {
    data: *mut u8,
    len: i64,
    cap: i64,
    elem: i64,
}

pub(crate) fn new_list(elem: usize) -> *mut ListHdr {
    let hdr = ambient_alloc(core::mem::size_of::<ListHdr>()) as *mut ListHdr;
    unsafe {
        hdr.write(ListHdr {
            data: core::ptr::null_mut(),
            len: 0,
            cap: 0,
            elem: elem as i64,
        });
    }
    hdr
}

pub(crate) fn push_raw(hdr: *mut ListHdr, elem_ptr: *const u8) {
    unsafe {
        let h = &mut *hdr;
        if h.len == h.cap {
            let ncap = if h.cap == 0 { 8 } else { h.cap * 2 };
            let ndata = ambient_alloc((ncap * h.elem) as usize);
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
        };
        let base = (&raw const hdr) as usize;
        assert_eq!((&raw const hdr.data) as usize - base, 0, "LIST_DATA_OFF");
        assert_eq!((&raw const hdr.len) as usize - base, 8, "LIST_LEN_OFF");
        assert_eq!(core::mem::size_of::<ListHdr>(), 32);
        assert_eq!(core::mem::align_of::<ListHdr>(), 8);
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
