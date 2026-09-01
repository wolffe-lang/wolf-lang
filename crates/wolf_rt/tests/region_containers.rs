//! s76 target 3 — the reclamation litmus (wolf-lang#81).
//!
//! `region scratch { … }` around container work must free every byte the
//! container used. Before s76 it freed NONE of them: `wolf_rt::list`
//! allocated its header and element buffer in the process-lifetime root
//! arena, so the region's wholesale free reclaimed nothing and family
//! B's `b3_churn` leaked 720 ns per request.
//!
//! # What this asserts, and why not RSS
//!
//! The witness is the runtime's OWN accounting:
//! `wolf_rt::native::live_region_bytes()` — the chunk capacity currently
//! owned by live regions. It moves for exactly two events (a region
//! takes a chunk from the system allocator; a region hands its chunks
//! back), so `before == after` across a create/fill/free cycle is an
//! exact, deterministic claim.
//!
//! RSS answers the same question badly: the system allocator is entitled
//! to retain freed pages, page-in is lazy, and the reading includes every
//! other allocation the process made. An RSS assertion tight enough to
//! catch the bug would flake, and one loose enough not to flake would not
//! catch it. So: the ledger, not RSS.
//!
//! # Why this is ONE test function
//!
//! `live_region_bytes` is process-wide, and cargo runs a test binary's
//! tests on parallel threads. Everything that must observe the global
//! ledger therefore lives in a single test in a file of its own, where no
//! sibling can create a region underneath the measurement. The
//! per-region assertions that DON'T need the global (placement,
//! growth-stays-put, the process-root fallback) are unit tests in
//! `wolf_rt::list`, where per-region attribution makes them immune to
//! test parallelism.

use wolf_rt::list::{__wolf_rt_list_len, __wolf_rt_list_new, __wolf_rt_list_push};
use wolf_rt::native::{
    __wolf_rt_live_region_bytes, __wolf_rt_region_ambient_enter, __wolf_rt_region_ambient_leave,
    __wolf_rt_region_bytes, __wolf_rt_region_free, __wolf_rt_region_new, live_region_bytes,
    region_bytes,
};

/// 1.6 MB of `int` elements — far above the 16 KB region chunk floor, so
/// the buffer is unmistakably region storage and not rounding.
const N: i64 = 200_000;

#[test]
fn region_free_reclaims_container_storage() {
    let base = live_region_bytes();

    // ---- a large List built inside a region --------------------------
    let r = __wolf_rt_region_new();
    // SAFETY: `r` is a fresh live handle; every list handle below comes
    // from `__wolf_rt_list_new`, and the element slots are stack i64s of
    // exactly the list's element size.
    let (grew, charged) = unsafe {
        let prev = __wolf_rt_region_ambient_enter(r);
        assert!(prev.is_null(), "the test thread starts at the process root");

        let h = __wolf_rt_list_new(8);
        for v in 0..N {
            __wolf_rt_list_push(h, (&raw const v) as i64);
        }
        assert_eq!(__wolf_rt_list_len(h), N, "the list holds every element");

        let grew = live_region_bytes() - base;
        let charged = region_bytes(r);
        // The s131 builtin shims (#187) answer with the same ledger:
        // one i64-shaped read each, nothing recomputed.
        assert_eq!(
            __wolf_rt_region_bytes(r),
            charged as i64,
            "the region_bytes builtin shim reads the same ledger"
        );
        assert_eq!(
            __wolf_rt_live_region_bytes(),
            live_region_bytes() as i64,
            "the live_region_bytes builtin shim reads the same counter"
        );
        __wolf_rt_region_ambient_leave(prev);
        (grew, charged)
    };

    // The header and the buffer are the region's storage. Both numbers
    // are ~0 under the #81 behavior.
    assert!(
        charged >= (N * 8) as usize,
        "the region's own ledger must carry the container: {charged} bytes for {N} elements"
    );
    assert!(
        grew >= (N * 8) as usize,
        "live region chunk capacity must cover the container: {grew} bytes for {N} elements"
    );

    // ---- the region exits -------------------------------------------
    // SAFETY: `r` is still live and no pointer into it is used again —
    // the escape checker (E1010) is what proves that for compiled code.
    unsafe { __wolf_rt_region_free(r) };

    assert_eq!(
        live_region_bytes(),
        base,
        "region free must return EVERY byte the container used \
         (this is the assertion wolf-lang#81 fails)"
    );
}
