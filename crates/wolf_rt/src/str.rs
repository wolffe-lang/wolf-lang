//! The s40 native `str` runtime — region-backed strbuf materialization.
//!
//! # Design note: region-backed strbuf materialization (owed by c08)
//!
//! A materialized `str` is an allocation like any other, and it lands
//! in the **ambient region** per `[mem.region.create.3]` — I15's
//! `#[noalloc]` law makes the allocation story a decision, not a
//! shortcut, so this note records the decision:
//!
//! - **The value form is unchanged.** A `str` stays the two-word
//!   `{ptr, len}` pair (s31); materialization only decides where the
//!   BYTES live. Literals stay in rodata; zero-copy results (`trim`,
//!   `get`, `strip_*`, `split` elements) stay subslices of their
//!   receiver's bytes; only genuinely new byte sequences (interpolation,
//!   `upper`/`lower`, `repeat`, `replace`, `fs_read_text`) allocate.
//! - **The ambient region, realized for v0.** The checked lane tracks
//!   an ambient-region stack and charges every materialization against
//!   it. The native debug tier realizes the ambient region as the
//!   PROCESS-LIFETIME root region: one bump arena, created lazily,
//!   freed by process exit. `main`'s enclosing region is exactly the
//!   process, so for everything the native lane can express today
//!   (s26 `region` blocks own no str materializations yet — the
//!   region-inference seam that places a materialization into a NAMED
//!   region is c09's, with escape analysis) this is the semantically
//!   correct region, not an approximation. The one observable
//!   difference — bytes outliving an enclosing `region { }` block —
//!   cannot be observed without raw-pointer reads that the checked
//!   lane already rules UB.
//! - **Growth is arena-shaped.** A growable buffer (strbuf, List) that
//!   outgrows its chunk copies into a fresh chunk and ABANDONS the old
//!   bytes to the arena — regions reclaim wholesale, never per-object
//!   (`[mem.region.intra.2]`); the debug tier pays memory for
//!   simplicity, the same trade the s28 bump allocator made.
//! - **Interpolation materializes through a strbuf.** Lowering emits
//!   `strbuf_new` → per-segment appends (literal chunks from rodata,
//!   holes through the SAME packed-spec renderers the s38 print path
//!   uses — parity with the checked executor's `eval_string` is by
//!   construction, the fmt_parity precedent) → `strbuf_finish`, which
//!   copies the built bytes into the ambient region and returns the
//!   `{ptr, len}` pair through a caller stack slot.
//! - **Out-parameters, not multi-returns.** Every shim returning a
//!   `str` writes `{ptr: i64, len: i64}` through a 16-byte out slot
//!   the caller owns; the scalar return (where present) carries the
//!   hit/miss or error code. This keeps the whole family inside the
//!   frozen all-`i64` runtime-symbol shape (s28) — no ABI novelty at
//!   the debug tier.
//! - **Misses are codes, never traps.** `get`/`strip_*`/`find` report
//!   miss as a return code; LOWERING decides between building a
//!   `{none}` row (`s.get(..)`) and trapping `bounds` (`s[a..b]`) —
//!   the runtime has one entry for both, exactly the `[mem.str.get]`
//!   "same domain" law.
//! - **Semantics are the checked lane's, function by function.** Every
//!   algorithm below is the same Rust `std::str` call the checked
//!   executor makes (`ubcheck.rs`'s str method arm), so native/checked
//!   stdout parity is by construction. The two runtime-condition
//!   refusals the checked lane makes (`split`/`count`/`replace` of an
//!   EMPTY needle — refusals, because no ruling exists yet) cannot be
//!   refusals at native run time; they take the documented DETERMINISTIC
//!   placeholders (`count` = 0, `split` = the whole string as one
//!   element, `replace` = identity) pending the spec ruling. Corpus
//!   programs that exercise them are unsupported on the checked lane
//!   and outside the parity gate.
//!
//! `str` values handed to these shims are valid UTF-8 by construction
//! (the language admits no other `str`); the shims trust it.

use std::sync::Mutex;

use crate::io::{
    f64_shortest, render_bool_packed, render_f64_packed, render_i64_packed, render_str_packed,
};

// ------------------------------------------------ the ambient region --

const CHUNK_MIN: usize = 64 * 1024;
const ALIGN: usize = 16;

struct Arena {
    chunks: Vec<Box<[u8]>>,
    used: usize,
}

static AMBIENT: Mutex<Arena> = Mutex::new(Arena {
    chunks: Vec::new(),
    used: 0,
});

/// Bump-allocate `size` bytes (16-aligned) in the process ambient
/// region. Never fails, never frees; zero-size asks get a distinct
/// aligned pointer.
pub(crate) fn ambient_alloc(size: usize) -> *mut u8 {
    let mut a = AMBIENT.lock().unwrap_or_else(|p| p.into_inner());
    let size = size.next_multiple_of(ALIGN).max(ALIGN);
    let need_new = match a.chunks.last() {
        Some(c) => a.used + size > c.len(),
        None => true,
    };
    if need_new {
        let cap = size.max(CHUNK_MIN);
        a.chunks.push(vec![0u8; cap].into_boxed_slice());
        a.used = 0;
    }
    let used = a.used;
    a.used += size;
    let chunk = a.chunks.last_mut().expect("chunk exists");
    unsafe { chunk.as_mut_ptr().add(used) }
}

/// Copy `bytes` into the ambient region, returning the stable pointer.
pub(crate) fn ambient_copy(bytes: &[u8]) -> *const u8 {
    let p = ambient_alloc(bytes.len());
    if !bytes.is_empty() {
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len()) };
    }
    p
}

// ----------------------------------------------------- str plumbing ---

/// Rebuild the `&str` behind a `{ptr, len}` pair.
///
/// # Safety
///
/// `ptr` must address `len` bytes of valid UTF-8 (every wolf `str` is).
pub(crate) unsafe fn view<'a>(ptr: i64, len: i64) -> &'a str {
    if ptr == 0 || len <= 0 {
        return "";
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    unsafe { core::str::from_utf8_unchecked(bytes) }
}

/// Write a `{ptr, len}` pair through an out slot.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
pub(crate) unsafe fn write_pair(out: i64, ptr: i64, len: i64) {
    let o = out as *mut i64;
    unsafe {
        o.write(ptr);
        o.add(1).write(len);
    }
}

/// Materialize `s` in the ambient region and write its pair.
unsafe fn write_owned(out: i64, s: &str) {
    let p = ambient_copy(s.as_bytes());
    unsafe { write_pair(out, p as i64, s.len() as i64) };
}

// -------------------------------------------------------- the strbuf --

/// `strbuf.new` — a fresh interpolation buffer (a Rust `String`;
/// `finish` moves the bytes into the ambient region and drops it).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_strbuf_new() -> i64 {
    Box::into_raw(Box::new(String::new())) as i64
}

unsafe fn buf<'a>(handle: i64) -> &'a mut String {
    unsafe { &mut *(handle as *mut String) }
}

/// Append a str segment under the packed `spec` (0 = raw bytes).
///
/// # Safety
///
/// `handle` from [`__wolf_rt_strbuf_new`], unfinished; `ptr`/`len` a
/// valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_strbuf_str(handle: i64, ptr: i64, len: i64, spec: i64) {
    let s = unsafe { view(ptr, len) };
    let b = unsafe { buf(handle) };
    if spec == 0 {
        b.push_str(s);
    } else {
        b.push_str(&render_str_packed(s, spec));
    }
}

/// Append a decimal (or spec-rendered) integer hole. Bit 14 of the
/// spec marks the value unsigned, exactly as in the s38 write shims.
///
/// # Safety
///
/// `handle` from [`__wolf_rt_strbuf_new`], unfinished.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_strbuf_i64(handle: i64, v: i64, spec: i64) {
    let b = unsafe { buf(handle) };
    if spec == 0 {
        b.push_str(&v.to_string());
    } else {
        b.push_str(&render_i64_packed(v, spec));
    }
}

/// Append a `true`/`false` hole. `i8` — WIR bools cross the call
/// boundary as one byte, exactly as in the s38 write shims.
///
/// # Safety
///
/// `handle` from [`__wolf_rt_strbuf_new`], unfinished.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_strbuf_bool(handle: i64, v: i8, spec: i64) {
    let b = unsafe { buf(handle) };
    if spec == 0 {
        b.push_str(if v != 0 { "true" } else { "false" });
    } else {
        b.push_str(&render_bool_packed(v != 0, spec));
    }
}

/// Append an `f64` hole (shortest round-trip without a spec — the
/// wolf-std `decimal.to_str` layout, same as print).
///
/// # Safety
///
/// `handle` from [`__wolf_rt_strbuf_new`], unfinished.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_strbuf_f64(handle: i64, v: f64, spec: i64) {
    let b = unsafe { buf(handle) };
    if spec == 0 {
        b.push_str(&f64_shortest(v));
    } else {
        b.push_str(&render_f64_packed(v, spec));
    }
}

/// Move the built bytes into the ambient region; write the `{ptr,
/// len}` pair through `out`; drop the buffer.
///
/// # Safety
///
/// `handle` from [`__wolf_rt_strbuf_new`] — dead after this call.
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_strbuf_finish(handle: i64, out: i64) {
    let s = unsafe { Box::from_raw(handle as *mut String) };
    unsafe { write_owned(out, &s) };
}

// ----------------------------------------------- the s37 method set ---

/// Byte equality of two str pairs (1 = equal).
///
/// # Safety
///
/// Both pairs must be valid str pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_eq(ap: i64, al: i64, bp: i64, bl: i64) -> i64 {
    i64::from(unsafe { view(ap, al) == view(bp, bl) })
}

/// Byte-lexicographic order (`[mem.str.order]`): -1 / 0 / 1.
///
/// # Safety
///
/// Both pairs must be valid str pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_cmp(ap: i64, al: i64, bp: i64, bl: i64) -> i64 {
    match unsafe { view(ap, al).as_bytes().cmp(view(bp, bl).as_bytes()) } {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

/// The boundary primitive (`[mem.str.get]`): the slice `s[a..b]` when
/// defined (1, pair through `out`, zero-copy), the miss code 0 exactly
/// on the checked slice's fault domain — OOB, `b < a`, or an offset
/// splitting a code point.
///
/// # Safety
///
/// A valid str pair; `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_get(sp: i64, sl: i64, a: i64, b: i64, out: i64) -> i64 {
    let s = unsafe { view(sp, sl) };
    if a < 0 || b < a || b > s.len() as i64 {
        return 0;
    }
    let (a, b) = (a as usize, b as usize);
    if !s.is_char_boundary(a) || !s.is_char_boundary(b) {
        return 0;
    }
    unsafe { write_pair(out, sp + a as i64, (b - a) as i64) };
    1
}

/// `find` (rev = 0) / `rfind` (rev = 1): the byte offset, or -1 miss.
///
/// # Safety
///
/// Both pairs must be valid str pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_find(sp: i64, sl: i64, np: i64, nl: i64, rev: i64) -> i64 {
    let (s, n) = unsafe { (view(sp, sl), view(np, nl)) };
    let hit = if rev == 0 { s.find(n) } else { s.rfind(n) };
    hit.map_or(-1, |o| o as i64)
}

/// `starts_with` (0) / `ends_with` (1) / `contains` (2) — 1 = hit.
///
/// # Safety
///
/// Both pairs must be valid str pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_probe(sp: i64, sl: i64, np: i64, nl: i64, mode: i64) -> i64 {
    let (s, n) = unsafe { (view(sp, sl), view(np, nl)) };
    i64::from(match mode {
        0 => s.starts_with(n),
        1 => s.ends_with(n),
        _ => s.contains(n),
    })
}

/// Non-overlapping occurrence count. Empty needle: 0 (the documented
/// deterministic placeholder — the checked lane refuses, see the
/// design note).
///
/// # Safety
///
/// Both pairs must be valid str pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_count(sp: i64, sl: i64, np: i64, nl: i64) -> i64 {
    let (s, n) = unsafe { (view(sp, sl), view(np, nl)) };
    if n.is_empty() {
        return 0;
    }
    s.matches(n).count() as i64
}

/// `trim` (0) / `trim_start` (1) / `trim_end` (2) — zero-copy: the
/// pair through `out` is a subslice of the receiver's bytes.
///
/// # Safety
///
/// A valid str pair; `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_trim(sp: i64, sl: i64, mode: i64, out: i64) {
    let s = unsafe { view(sp, sl) };
    let t = match mode {
        0 => s.trim(),
        1 => s.trim_start(),
        _ => s.trim_end(),
    };
    let off = t.as_ptr() as i64 - s.as_ptr() as i64;
    let ptr = if sl == 0 { sp } else { sp + off };
    unsafe { write_pair(out, ptr, t.len() as i64) };
}

/// `lower` (0) / `upper` (1) — Unicode case mapping, materialized in
/// the ambient region.
///
/// # Safety
///
/// A valid str pair; `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_case(sp: i64, sl: i64, upper: i64, out: i64) {
    let s = unsafe { view(sp, sl) };
    let t = if upper == 0 {
        s.to_lowercase()
    } else {
        s.to_uppercase()
    };
    unsafe { write_owned(out, &t) };
}

/// `strip_prefix` (suffix = 0) / `strip_suffix` (suffix = 1): 1 = hit
/// with the zero-copy rest through `out`, 0 = miss.
///
/// # Safety
///
/// Both pairs valid; `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_strip(
    sp: i64,
    sl: i64,
    np: i64,
    nl: i64,
    suffix: i64,
    out: i64,
) -> i64 {
    let (s, n) = unsafe { (view(sp, sl), view(np, nl)) };
    let hit = if suffix == 0 {
        s.strip_prefix(n).map(|r| (nl, r.len()))
    } else {
        s.strip_suffix(n).map(|r| (0, r.len()))
    };
    match hit {
        Some((off, len)) => {
            unsafe { write_pair(out, sp + off, len as i64) };
            1
        }
        None => 0,
    }
}

/// `repeat` — `count` copies, materialized. The count is non-negative
/// by the caller's contract (lowering traps `bounds` first, matching
/// the checked lane).
///
/// # Safety
///
/// A valid str pair; `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_repeat(sp: i64, sl: i64, count: i64, out: i64) {
    let s = unsafe { view(sp, sl) };
    let t = s.repeat(count.max(0) as usize);
    unsafe { write_owned(out, &t) };
}

/// `replace(from, to)` — materialized. Empty `from`: identity (the
/// documented deterministic placeholder; the checked lane refuses).
///
/// # Safety
///
/// All three pairs valid; `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_replace(
    sp: i64,
    sl: i64,
    fp: i64,
    fl: i64,
    tp: i64,
    tl: i64,
    out: i64,
) {
    let (s, from, to) = unsafe { (view(sp, sl), view(fp, fl), view(tp, tl)) };
    if from.is_empty() {
        unsafe { write_pair(out, sp, sl) };
        return;
    }
    let t = s.replace(from, to);
    unsafe { write_owned(out, &t) };
}

/// The view family, materialized as `List[str]` (D25/s37 v0):
/// `split(sep)` (mode 0), `words` (mode 1 — Unicode `White_Space`),
/// `lines` (mode 2). Elements are zero-copy subslices of the
/// receiver's bytes. Empty separator: the whole string as one element
/// (deterministic placeholder; the checked lane refuses).
///
/// # Safety
///
/// Both pairs must be valid str pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_split(sp: i64, sl: i64, np: i64, nl: i64, mode: i64) -> i64 {
    let (s, n) = unsafe { (view(sp, sl), view(np, nl)) };
    let parts: Vec<&str> = match mode {
        0 if n.is_empty() => vec![s],
        0 => s.split(n).collect(),
        1 => s.split_whitespace().collect(),
        _ => s.lines().collect(),
    };
    let hdr = crate::list::new_list(16);
    for p in parts {
        let off = if sl == 0 {
            0
        } else {
            p.as_ptr() as i64 - s.as_ptr() as i64
        };
        let pair = [sp + off, p.len() as i64];
        crate::list::push_raw(hdr, pair.as_ptr().cast());
    }
    hdr as i64
}

/// `bytes()` — the byte view, materialized as `List[int]` (each byte
/// an i64 element, exactly the checked lane's value shape).
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_bytes(sp: i64, sl: i64) -> i64 {
    let s = unsafe { view(sp, sl) };
    let hdr = crate::list::new_list(8);
    for b in s.bytes() {
        let v = [i64::from(b)];
        crate::list::push_raw(hdr, v.as_ptr().cast());
    }
    hdr as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair_of(s: &str) -> (i64, i64) {
        (s.as_ptr() as i64, s.len() as i64)
    }

    fn read_pair(out: &[i64; 2]) -> String {
        unsafe { view(out[0], out[1]).to_string() }
    }

    #[test]
    fn strbuf_builds_interpolations() {
        let b = __wolf_rt_strbuf_new();
        let (hp, hl) = pair_of("hello, ");
        let (np, nl) = pair_of("wolf");
        let mut out = [0i64; 2];
        unsafe {
            __wolf_rt_strbuf_str(b, hp, hl, 0);
            __wolf_rt_strbuf_str(b, np, nl, 0);
            __wolf_rt_strbuf_i64(b, 3, 0);
            __wolf_rt_strbuf_bool(b, 1, 0);
            __wolf_rt_strbuf_f64(b, 0.5, 0);
            __wolf_rt_strbuf_finish(b, out.as_mut_ptr() as i64);
        }
        assert_eq!(read_pair(&out), "hello, wolf3true0.5");
    }

    #[test]
    fn get_matches_the_checked_domain() {
        let (sp, sl) = pair_of("é wolf");
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        unsafe {
            assert_eq!(__wolf_rt_str_get(sp, sl, 0, 2, o), 1);
            assert_eq!(read_pair(&out), "é");
            assert_eq!(__wolf_rt_str_get(sp, sl, 0, 1, o), 0); // split code point
            assert_eq!(__wolf_rt_str_get(sp, sl, 3, 2, o), 0); // b < a
            assert_eq!(__wolf_rt_str_get(sp, sl, 0, 99, o), 0); // oob
        }
    }

    #[test]
    fn the_method_set_matches_std() {
        let (sp, sl) = pair_of("  the wolf runs  ");
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        unsafe {
            __wolf_rt_str_trim(sp, sl, 0, o);
            assert_eq!(read_pair(&out), "the wolf runs");
            let (np, nl) = pair_of("wolf");
            assert_eq!(__wolf_rt_str_find(sp, sl, np, nl, 0), 6);
            assert_eq!(__wolf_rt_str_probe(sp, sl, np, nl, 2), 1);
            let (ep, el) = pair_of("é");
            __wolf_rt_str_case(ep, el, 1, o);
            assert_eq!(read_pair(&out), "É");
            let (ap, al) = pair_of("ab");
            __wolf_rt_str_repeat(ap, al, 2, o);
            assert_eq!(read_pair(&out), "abab");
            let (wp, wl) = pair_of("a b  c");
            let hdr = __wolf_rt_str_split(wp, wl, 0, 0, 1);
            assert_eq!(crate::list::__wolf_rt_list_len(hdr), 3);
        }
    }

    #[test]
    fn ordering_is_byte_lexicographic() {
        let (ap, al) = pair_of("wolf");
        let (bp, bl) = pair_of("wolves");
        let (zp, zl) = pair_of("z");
        let (ep, el) = pair_of("é");
        unsafe {
            assert_eq!(__wolf_rt_str_cmp(ap, al, bp, bl), -1);
            assert_eq!(__wolf_rt_str_cmp(zp, zl, ep, el), -1); // "z" < "é"
            assert_eq!(__wolf_rt_str_eq(ap, al, ap, al), 1);
        }
    }
}
