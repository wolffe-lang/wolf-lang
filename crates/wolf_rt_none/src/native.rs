//! The region, the ambient slot and the allocator hook (kw12,
//! `[abi.target.none.alloc]`).
//!
//! # The hook contract
//!
//! The program supplies two functions under these names, in wolf
//! (`export fn`) or assembly, with the C convention:
//!
//! - `wolf_alloc(size: i64, align: i64) -> *u8` returns `size` writable
//!   bytes at an address that is a multiple of `align`. `align` is a
//!   power of two; this runtime always asks for 16 (every grant it
//!   makes is 16-aligned, the alignment both tiers assume of a runtime
//!   allocation), and `size` is a positive multiple of 16. The bytes
//!   need not be zeroed. The hook must not return null: a null is the
//!   hook breaking its contract, and the runtime answers it with
//!   `wolf_trap(alloc-contract)` before touching the memory. Running out
//!   is the program's policy, decided inside its hook (a kernel
//!   panics).
//! - `wolf_free(p: *u8, size: i64, align: i64)` returns a block. The
//!   runtime calls it exactly once per block it is done with, with the
//!   very `size` and `align` that block was allocated with, and never
//!   with null. It is called when a region is freed (each of the
//!   region's chunks, then its header) and when an interpolation
//!   finishes (the build buffer and its header). Root allocations — a
//!   container or string made outside every `region` — are never freed,
//!   which is the hosted meaning of the process root.
//!
//! Neither hook is ever called re-entrantly by the runtime, and nothing
//! here calls them from a context the program did not enter.
//!
//! # The region
//!
//! A region is a bump allocator over chunks taken from the hook on the
//! hosted ladder (1 KiB doubling to 1 MiB, larger for one larger ask):
//! each chunk starts with a 16-byte link (`next`, `size`) so the region
//! can hand every chunk back on free; the bump runs after the link. The
//! ledger (`region_bytes`, the creation-time cap and its
//! `alloc-contract` trap) is the hosted one, byte for byte: the aligned
//! sum of what was asked, monotone, high-water. `live_region_bytes` is
//! chunk capacity owned by live regions, as hosted (here the link words
//! are inside that capacity).

use core::ffi::c_void;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// Trap kind codes — `[conf.trap.set]`'s runtime numbering, the
/// hosted `wolf_rt::native::trap_code` values.
pub mod trap_code {
    pub const ALLOC_CONTRACT: i32 = 9;
    pub const UB: i32 = 11;
}

unsafe extern "C" {
    fn wolf_alloc(size: i64, align: i64) -> *mut u8;
    fn wolf_free(p: *mut u8, size: i64, align: i64);
    fn wolf_trap(kind: i32, file: *const u8, file_len: i64, line: i64, col: i64) -> !;
}

/// Every grant's alignment, and the hook's `align` argument.
const ALIGN: usize = 16;
/// The hosted chunk ladder (`wolf_rt::native`): first chunk, ceiling.
const CHUNK_MIN: usize = 1024;
const CHUNK_MAX: usize = 1024 * 1024;

/// A site-less trap through the program's hook.
pub(crate) fn trap(kind: i32) -> ! {
    // SAFETY: the hook takes no pointer it dereferences when the file
    // length is zero, and must not return.
    unsafe { wolf_trap(kind, core::ptr::null(), 0, 0, 0) }
}

/// `size` bytes (a multiple of 16) at a 16-aligned address from the
/// program's hook; a null answer is the hook breaking its contract.
pub(crate) fn hook_alloc(size: usize) -> *mut u8 {
    // SAFETY: the hook's contract (module docs); the arguments are a
    // positive multiple of 16 and 16.
    let p = unsafe { wolf_alloc(size as i64, ALIGN as i64) };
    if p.is_null() {
        trap(trap_code::ALLOC_CONTRACT);
    }
    p
}

/// Hand back a block [`hook_alloc`] returned for `size`.
///
/// # Safety
///
/// `p` came from `hook_alloc(size)` and is not used again.
pub(crate) unsafe fn hook_free(p: *mut u8, size: usize) {
    unsafe { wolf_free(p, size as i64, ALIGN as i64) }
}

/// `n` rounded up to the 16-byte grain, never below one grain (zero-size
/// asks get distinct pointers, as hosted).
pub(crate) fn grain(n: usize) -> usize {
    n.saturating_add(ALIGN - 1).max(ALIGN) & !(ALIGN - 1)
}

/// The `n`-th chunk's capacity when it must hold `need` bytes: the
/// hosted ladder, never below `need`.
fn chunk_size(nth: usize, need: usize) -> usize {
    let steps = (CHUNK_MAX.trailing_zeros() - CHUNK_MIN.trailing_zeros()) as usize;
    need.max(CHUNK_MIN << nth.min(steps))
}

/// The link at the head of every chunk.
#[repr(C, align(16))]
struct ChunkLink {
    next: *mut ChunkLink,
    size: usize,
}

const LINK: usize = core::mem::size_of::<ChunkLink>();

#[repr(C)]
struct Region {
    /// Bump cursor and end of the current chunk (null/null: none open).
    cur: *mut u8,
    end: *mut u8,
    /// The chunks, newest first.
    chunks: *mut ChunkLink,
    nchunks: usize,
    /// Sum of the chunks' capacities.
    owned: usize,
    /// The ledger: aligned bytes ever granted.
    bytes: usize,
    /// The creation-time budget (`usize::MAX` = uncapped).
    cap: usize,
}

const REGION_HDR: usize = (core::mem::size_of::<Region>() + ALIGN - 1) & !(ALIGN - 1);

static LIVE_REGION_BYTES: AtomicUsize = AtomicUsize::new(0);

/// The ambient region (`[mem.region.create.3]`); null = the process
/// root. One word for the thread of control running now: a program
/// that switches threads saves and restores it per thread through
/// [`wolf_rt_ambient_get`] / [`wolf_rt_ambient_set`]
/// (`[abi.target.none.ambient]`, wolf-lang#611).
static AMBIENT_REGION: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());

/// The ambient region, or null for the process root (read by the
/// shared list/map source).
pub(crate) fn ambient_region() -> *mut c_void {
    AMBIENT_REGION.load(Ordering::Relaxed)
}

/// `region.new` — a fresh, empty, uncapped region.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_region_new() -> *mut c_void {
    let r = hook_alloc(REGION_HDR).cast::<Region>();
    // SAFETY: a fresh block of REGION_HDR >= size_of::<Region>() bytes,
    // 16-aligned.
    unsafe {
        r.write(Region {
            cur: core::ptr::null_mut(),
            end: core::ptr::null_mut(),
            chunks: core::ptr::null_mut(),
            nchunks: 0,
            owned: 0,
            bytes: 0,
            cap: usize::MAX,
        });
    }
    r.cast()
}

/// `region_set_cap` — the creation-time budget (`[mem.region.cap.1]`);
/// negative is `alloc-contract` (`[mem.region.cap.2]`).
///
/// # Safety
///
/// `handle` is a live region from [`__wolf_rt_region_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_set_cap(handle: *mut c_void, cap: i64) {
    if cap < 0 {
        trap(trap_code::ALLOC_CONTRACT);
    }
    let r: &mut Region = unsafe { &mut *handle.cast() };
    r.cap = cap as usize;
}

/// `region.alloc` — `size` bytes, 16-aligned, charged to the ledger.
///
/// # Safety
///
/// `handle` is a live region from [`__wolf_rt_region_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_alloc(handle: *mut c_void, size: i64) -> *mut u8 {
    let r: &mut Region = unsafe { &mut *handle.cast() };
    if size < 0 {
        trap(trap_code::ALLOC_CONTRACT);
    }
    let size = grain(size as usize);
    if r.bytes.saturating_add(size) > r.cap {
        trap(trap_code::ALLOC_CONTRACT);
    }
    let cur = r.cur as usize;
    if !r.cur.is_null() && size <= r.end as usize - cur {
        r.cur = r.cur.wrapping_add(size);
        r.bytes += size;
        return cur as *mut u8;
    }
    region_alloc_slow(r, size)
}

#[cold]
#[inline(never)]
fn region_alloc_slow(r: &mut Region, size: usize) -> *mut u8 {
    let cap = chunk_size(r.nchunks, size.saturating_add(LINK));
    let base = hook_alloc(cap);
    let link = base.cast::<ChunkLink>();
    // SAFETY: a fresh 16-aligned block of `cap >= LINK + size` bytes.
    unsafe {
        link.write(ChunkLink {
            next: r.chunks,
            size: cap,
        });
        let start = base.add(LINK);
        r.cur = start.add(size);
        r.end = base.add(cap);
        r.chunks = link;
        r.nchunks += 1;
        r.owned += cap;
        r.bytes += size;
        LIVE_REGION_BYTES.fetch_add(cap, Ordering::Relaxed);
        start
    }
}

/// `region.free` — every chunk back to the hook, then the header.
///
/// # Safety
///
/// `handle` is a live region from [`__wolf_rt_region_new`]; it is dead
/// after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_free(handle: *mut c_void) {
    let r = handle.cast::<Region>();
    unsafe {
        let mut c = (*r).chunks;
        while !c.is_null() {
            let next = (*c).next;
            let size = (*c).size;
            hook_free(c.cast(), size);
            c = next;
        }
        LIVE_REGION_BYTES.fetch_sub((*r).owned, Ordering::Relaxed);
        hook_free(r.cast(), REGION_HDR);
    }
}

/// The `region_bytes(r)` builtin: the region's ledger weight.
///
/// # Safety
///
/// `handle` is a live region from [`__wolf_rt_region_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_bytes(handle: *mut c_void) -> i64 {
    (unsafe { &*handle.cast::<Region>() }).bytes as i64
}

/// The `live_region_bytes()` builtin: chunk capacity owned by live
/// regions.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_live_region_bytes() -> i64 {
    LIVE_REGION_BYTES.load(Ordering::Relaxed) as i64
}

/// Open `handle` as the ambient region; returns the previous one, which
/// the X4 cleanup chain hands to [`__wolf_rt_region_ambient_leave`].
///
/// # Safety
///
/// `handle` is a live region for as long as it stays ambient.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_ambient_enter(handle: *mut c_void) -> *mut c_void {
    AMBIENT_REGION.swap(handle, Ordering::Relaxed)
}

/// Restore the ambient region a matching enter returned.
///
/// # Safety
///
/// `prev` is what the matching enter returned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_ambient_leave(prev: *mut c_void) {
    AMBIENT_REGION.store(prev, Ordering::Relaxed);
}

/// `wolf_rt_ambient_get() -> *u8` (`[abi.target.none.ambient]`): the
/// running thread's ambient region, null for the process root. A
/// scheduler stores it in the outgoing thread's record at a switch.
#[unsafe(no_mangle)]
pub extern "C" fn wolf_rt_ambient_get() -> *mut c_void {
    AMBIENT_REGION.load(Ordering::Relaxed)
}

/// `wolf_rt_ambient_set(p: *u8)` (`[abi.target.none.ambient]`): make
/// `p` the running thread's ambient region. A scheduler passes the
/// incoming thread's saved value at a switch — null for a thread that
/// has not run yet (the process root).
///
/// # Safety
///
/// `p` is null or a value [`wolf_rt_ambient_get`] returned on the
/// thread now being resumed, whose region is still open there.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wolf_rt_ambient_set(p: *mut c_void) {
    AMBIENT_REGION.store(p, Ordering::Relaxed);
}

/// A capturing closure's record, in the ambient region
/// (`[abi.native.closure]`), as hosted.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_closure_alloc(size: i64) -> *mut u8 {
    if size < 0 {
        trap(trap_code::ALLOC_CONTRACT);
    }
    crate::list::alloc_in(ambient_region(), size as usize)
}
