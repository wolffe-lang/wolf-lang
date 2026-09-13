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
use wolf_rt::str::__wolf_rt_str_repeat;

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

    // ---- s160 (wolf-lang#191): the same litmus, for `str` -----------
    //
    // The #81 twin, string edition. Until s160 `str` materialization
    // allocated in `wolf_rt::str`'s own process-lifetime arena rather
    // than the ambient region, so this second half read ~0 charged and
    // the free reclaimed nothing — a `region scratch { }` reclaimed a
    // List's storage and not one byte of the string work beside it.
    // Same instrument and same reasoning as above: the ledger, never
    // RSS. It lives in this function, not a sibling, because
    // `live_region_bytes` is process-wide (see the module header).
    let r2 = __wolf_rt_region_new();
    // SAFETY: `r2` is a fresh live handle; `out` is a 16-byte slot the
    // caller owns, which is exactly what the str shims write through.
    let (grew2, charged2, total_len) = unsafe {
        let prev = __wolf_rt_region_ambient_enter(r2);
        assert!(
            prev.is_null(),
            "the str phase also starts at the process root"
        );

        // 2,000 materializations of 512 bytes each — ~1 MB, far above
        // the chunk floor, so nothing here is rounding.
        let src = "0123456789abcdef";
        let mut out = [0i64; 2];
        let mut total_len = 0i64;
        for _ in 0..2_000 {
            __wolf_rt_str_repeat(
                src.as_ptr() as i64,
                src.len() as i64,
                32,
                (&raw mut out) as i64,
            );
            total_len += out[1];
        }

        let grew2 = live_region_bytes() - base;
        let charged2 = region_bytes(r2);
        __wolf_rt_region_ambient_leave(prev);
        (grew2, charged2, total_len)
    };

    assert_eq!(
        total_len,
        2_000 * 512,
        "every repeat produced its 512 bytes"
    );
    assert!(
        charged2 >= 2_000 * 512,
        "the region's ledger must carry the STRING bytes too: {charged2} bytes \
         (this is the assertion wolf-lang#191 fails — it read ~0)"
    );
    assert!(
        grew2 >= 2_000 * 512,
        "live region chunk capacity must cover the string bytes: {grew2}"
    );

    // SAFETY: `r2` is live and no `{ptr, len}` pair built above is read
    // again — E1010 is what proves that for compiled code, and s160
    // widened it to proc frames (#355) and projected `str` reads (#321)
    // precisely because this free makes the leak a dangle.
    unsafe { __wolf_rt_region_free(r2) };

    assert_eq!(
        live_region_bytes(),
        base,
        "region free must return EVERY byte the string work used \
         (this is the assertion wolf-lang#191 fails)"
    );
}
