//! kw12: the freestanding runtime against a recording allocator hook,
//! on the host (`[abi.target.none.alloc]`'s hook contract, checked
//! call by call). The test binary IS the program: it defines
//! `wolf_alloc`, `wolf_free` and `wolf_trap` the way a kernel would, and
//! records every call, so each row can say exactly which blocks the
//! runtime asked for and gave back.
//!
//! The end-to-end witness (a wolf kernel, both tiers, linked `-nostdlib`
//! against this crate built for x86_64-unknown-none) is
//! `crates/wolf_driver/tests/freestanding_alloc.rs`.

use std::alloc::Layout;
use std::collections::BTreeMap;
use std::sync::Mutex;

use wolf_rt_none::list::{
    __wolf_rt_list_copy, __wolf_rt_list_len, __wolf_rt_list_new, __wolf_rt_list_pop,
    __wolf_rt_list_push, __wolf_rt_list_read,
};
use wolf_rt_none::map::{__wolf_rt_map_get, __wolf_rt_map_new, __wolf_rt_map_set};
use wolf_rt_none::native::{
    __wolf_rt_closure_alloc, __wolf_rt_live_region_bytes, __wolf_rt_region_alloc,
    __wolf_rt_region_ambient_enter, __wolf_rt_region_ambient_leave, __wolf_rt_region_bytes,
    __wolf_rt_region_free, __wolf_rt_region_new, __wolf_rt_region_set_cap,
};
use wolf_rt_none::str::{
    __wolf_rt_strbuf_bool, __wolf_rt_strbuf_char, __wolf_rt_strbuf_finish, __wolf_rt_strbuf_i64,
    __wolf_rt_strbuf_new, __wolf_rt_strbuf_str,
};

/// Live blocks by address: (size, align). The runtime's state (the
/// ambient slot, the live counter) is process-wide, as on the target, so
/// every row runs under one lock.
static LIVE: Mutex<BTreeMap<usize, (i64, i64)>> = Mutex::new(BTreeMap::new());
static SERIAL: Mutex<()> = Mutex::new(());
static FREES: Mutex<Vec<(usize, i64, i64)>> = Mutex::new(Vec::new());

#[unsafe(no_mangle)]
pub extern "C" fn wolf_alloc(size: i64, align: i64) -> *mut u8 {
    assert!(size > 0 && size % 16 == 0, "size {size}: a positive multiple of 16");
    assert_eq!(align, 16, "the runtime asks for 16");
    let layout = Layout::from_size_align(size as usize, align as usize).unwrap();
    // SAFETY: non-zero size.
    let p = unsafe { std::alloc::alloc(layout) };
    assert!(!p.is_null());
    LIVE.lock().unwrap().insert(p as usize, (size, align));
    p
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wolf_free(p: *mut u8, size: i64, align: i64) {
    assert!(!p.is_null(), "wolf_free(null)");
    let got = LIVE.lock().unwrap().remove(&(p as usize));
    assert_eq!(
        got,
        Some((size, align)),
        "wolf_free({p:?}, {size}, {align}) must name a live block by its own size and align"
    );
    FREES.lock().unwrap().push((p as usize, size, align));
    let layout = Layout::from_size_align(size as usize, align as usize).unwrap();
    // SAFETY: allocated above with this layout.
    unsafe { std::alloc::dealloc(p, layout) };
}

#[unsafe(no_mangle)]
pub extern "C" fn wolf_trap(kind: i32, _file: *const u8, _len: i64, _line: i64, _col: i64) -> ! {
    // A trap ends the program; the rows below never reach one.
    eprintln!("wolf_trap({kind})");
    std::process::exit(134)
}

fn live() -> BTreeMap<usize, (i64, i64)> {
    LIVE.lock().unwrap().clone()
}

/// Build a str by interpolation: `pieces` appended in order.
fn interpolate(f: impl FnOnce(i64)) -> Vec<u8> {
    let h = __wolf_rt_strbuf_new();
    f(h);
    let mut out = [0i64; 2];
    unsafe { __wolf_rt_strbuf_finish(h, out.as_mut_ptr() as i64) };
    assert_eq!(out[0] % 16, 0, "a finished str is a 16-aligned grant");
    unsafe { std::slice::from_raw_parts(out[0] as *const u8, out[1] as usize) }.to_vec()
}

#[test]
fn a_region_hands_every_block_back_by_its_own_size() {
    let _g = SERIAL.lock().unwrap();
    let before = live();
    let live_before = __wolf_rt_live_region_bytes();
    let r = __wolf_rt_region_new();
    unsafe {
        __wolf_rt_region_set_cap(r, 1 << 40);
        let prev = __wolf_rt_region_ambient_enter(r);
        // A list grown in the region: header and every buffer land in it.
        let xs = __wolf_rt_list_new(8);
        for i in 0..5000i64 {
            __wolf_rt_list_push(xs, (&raw const i) as i64);
        }
        assert_eq!(__wolf_rt_list_len(xs), 5000);
        let mut v = 0i64;
        assert_eq!(__wolf_rt_list_read(xs, 4321, (&raw mut v) as i64), 1);
        assert_eq!(v, 4321);
        // A closure record and an interpolated str, ambient too.
        let rec = __wolf_rt_closure_alloc(24);
        assert_eq!(rec as usize % 16, 0);
        let s = interpolate(|h| __wolf_rt_strbuf_i64(h, 42, 0));
        assert_eq!(s, b"42");
        __wolf_rt_region_ambient_leave(prev);
        assert!(__wolf_rt_region_bytes(r) > 5000 * 8);
        assert!(__wolf_rt_live_region_bytes() > live_before);
        __wolf_rt_region_free(r);
    }
    assert_eq!(
        live(),
        before,
        "after region.free nothing the region took is still live"
    );
    assert_eq!(__wolf_rt_live_region_bytes(), live_before);
}

#[test]
fn the_ledger_and_the_cap_are_the_hosted_ones() {
    let _g = SERIAL.lock().unwrap();
    let r = __wolf_rt_region_new();
    unsafe {
        __wolf_rt_region_set_cap(r, 96);
        let a = __wolf_rt_region_alloc(r, 80);
        let b = __wolf_rt_region_alloc(r, 1);
        assert!(!a.is_null() && !b.is_null() && a != b);
        assert_eq!(a as usize % 16, 0);
        assert_eq!(b as usize % 16, 0);
        // 80 + 16 = 96: at the cap exactly is not a breach (hosted).
        assert_eq!(__wolf_rt_region_bytes(r), 96);
        // One allocation larger than the ladder's first rung gets a
        // chunk of its own.
        let r2 = __wolf_rt_region_new();
        let big = __wolf_rt_region_alloc(r2, 5000);
        assert_eq!(big as usize % 16, 0);
        std::ptr::write_bytes(big, 0xab, 5000);
        assert_eq!(__wolf_rt_region_bytes(r2), 5008);
        __wolf_rt_region_free(r2);
        __wolf_rt_region_free(r);
    }
}

#[test]
fn root_allocations_are_never_freed_and_a_finish_frees_its_buffer() {
    let _g = SERIAL.lock().unwrap();
    FREES.lock().unwrap().clear();
    let before = live();
    let s = interpolate(|h| unsafe {
        // 50 bytes fit the first 64-byte buffer; 150 more outgrow it.
        let a = "x".repeat(50);
        let b = "x".repeat(150);
        __wolf_rt_strbuf_str(h, a.as_ptr() as i64, a.len() as i64, 0);
        __wolf_rt_strbuf_str(h, b.as_ptr() as i64, b.len() as i64, 0);
        __wolf_rt_strbuf_i64(h, -7, 0);
    });
    assert_eq!(s.len(), 202);
    assert!(s.ends_with(b"x-7"));
    let after = live();
    // Only the root copy of the finished str is still live: the build
    // buffers (64 then 208 bytes) and the header went back.
    let new: Vec<_> = after
        .iter()
        .filter(|(k, _)| !before.contains_key(k))
        .collect();
    assert_eq!(new.len(), 1, "one live block, the str's root copy: {new:?}");
    assert_eq!(*new[0].1, (208, 16));
    let frees = FREES.lock().unwrap().clone();
    let sizes: Vec<i64> = frees.iter().map(|f| f.1).collect();
    assert_eq!(sizes, vec![64, 208, 32], "old buffer, final buffer, header");
}

#[test]
fn holes_render_as_the_hosted_runtime_renders_them() {
    let _g = SERIAL.lock().unwrap();
    // Packed specs, `wolf_rt::io::unpack`'s layout.
    let width = |w: i64| (1 << 12) | (w << 16);
    let right = 3 << 8;
    let center = 2 << 8;
    let zero = 1 << 11;
    let sign = 1 << 10;
    let unsigned = 1 << 14;
    let kind = |k: i64| k << 48;
    let prec = |p: i64| (1 << 13) | (p << 32);
    let cases: &[(&str, Vec<u8>)] = &[
        (
            "int default",
            interpolate(|h| unsafe { __wolf_rt_strbuf_i64(h, i64::MIN, 0) }),
        ),
        (
            "int width 8 (right by default)",
            interpolate(|h| unsafe { __wolf_rt_strbuf_i64(h, -42, width(8)) }),
        ),
        (
            "int zero-padded with sign",
            interpolate(|h| unsafe { __wolf_rt_strbuf_i64(h, 42, width(6) | zero | sign) }),
        ),
        (
            "hex, upper hex, octal, binary",
            interpolate(|h| unsafe {
                __wolf_rt_strbuf_i64(h, 255, kind(3));
                __wolf_rt_strbuf_i64(h, 255, kind(4));
                __wolf_rt_strbuf_i64(h, 8, kind(2));
                __wolf_rt_strbuf_i64(h, 5, kind(1));
            }),
        ),
        (
            "unsigned",
            interpolate(|h| unsafe { __wolf_rt_strbuf_i64(h, -1, unsigned) }),
        ),
        (
            "str centered with a fill",
            interpolate(|h| unsafe {
                let s = "ab";
                __wolf_rt_strbuf_str(h, s.as_ptr() as i64, 2, width(7) | center | b'*' as i64);
            }),
        ),
        (
            "str precision at a char boundary",
            interpolate(|h| unsafe {
                let s = "héllo";
                __wolf_rt_strbuf_str(h, s.as_ptr() as i64, s.len() as i64, prec(2));
            }),
        ),
        (
            "bool right-aligned, char",
            interpolate(|h| unsafe {
                __wolf_rt_strbuf_bool(h, 1, width(6) | right);
                __wolf_rt_strbuf_char(h, 'é' as i64, 0);
                __wolf_rt_strbuf_char(h, 0x11_0000, 0);
            }),
        ),
    ];
    let want: &[&str] = &[
        "-9223372036854775808",
        "     -42",
        "+00042",
        "ffFF10101",
        "18446744073709551615",
        "**ab***",
        "h",
        "  trueé\u{FFFD}",
    ];
    for ((name, got), want) in cases.iter().zip(want) {
        assert_eq!(
            std::str::from_utf8(got).unwrap(),
            *want,
            "{name}: the hosted renderer's bytes"
        );
    }
}

#[test]
fn lists_and_maps_are_the_hosted_families() {
    let _g = SERIAL.lock().unwrap();
    unsafe {
        let xs = __wolf_rt_list_new(8);
        for v in [3i64, 1, 4] {
            __wolf_rt_list_push(xs, (&raw const v) as i64);
        }
        let ys = __wolf_rt_list_copy(xs);
        let mut out = 0i64;
        assert_eq!(__wolf_rt_list_pop(xs, (&raw mut out) as i64), 1);
        assert_eq!(out, 4);
        assert_eq!(__wolf_rt_list_len(xs), 2);
        assert_eq!(__wolf_rt_list_len(ys), 3, "the copy is independent");
        // Map[int, int]: key bytes at 0, value at 8, stride 16.
        let m = __wolf_rt_map_new(0, 8, 8, 8, 16);
        for (k, v) in [(1i64, 10i64), (2, 20), (1, 11)] {
            let kv = [k, v];
            __wolf_rt_map_set(m, kv.as_ptr() as i64);
        }
        let mut kv = [1i64, 0];
        assert_eq!(__wolf_rt_map_get(m, kv.as_mut_ptr() as i64), 1);
        assert_eq!(kv[1], 11);
        let mut miss = [9i64, 0];
        assert_eq!(__wolf_rt_map_get(m, miss.as_mut_ptr() as i64), 0);
    }
}
