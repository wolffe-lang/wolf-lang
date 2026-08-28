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
//!
//! # The byte view (s77, wolf-lang#80)
//!
//! `bytes()` used to MATERIALIZE — `__wolf_rt_str_bytes` built a
//! `List[int]`, eight heap bytes per input byte, so every string kernel
//! in the M2 suite measured an allocation (family D at 0.015x). It is
//! now a **view**, and the view is the receiver's own `{ptr, len}`
//! pair: the same two words a `str` is, and the same two words every
//! zero-copy result in this file already was (`trim`, `get`,
//! `strip_*`, the `split`/`words`/`lines` elements). One
//! representation, one lifetime story — a view borrows exactly the
//! storage a subslice borrows, so nothing new can dangle that a
//! subslice could not.
//!
//! Element access is compiled, not called: lowering emits `ptr.off` at
//! stride 1 plus `load.i8` plus `zext` (s75's gep+load at the stride
//! bytes actually have), with the bounds check in the caller where the
//! range analysis can see it. So `wolf_rt` owns no byte-view entry
//! point at all — which is the point.
//!
//! **What the view cannot do.** It cannot be written: lowering emits no
//! store path for it, the surface already refuses to name a temporary as
//! a mutable place (E0804/E1009 for `s.bytes().push(1)` and
//! `(mut s.bytes()).push(1)`), and lowering refuses the mutators on top
//! of that. It cannot become a `str` by CASTING: the two str-producing
//! entries here (`str_get`, and the `s[a..b]` lowering that mirrors it)
//! only ever narrow an ALREADY-valid `str` on code-point boundaries.
//!
//! # The byte source (s81, wolf-lang#58)
//!
//! s77 left the byte tier one-way on purpose — a view and no source —
//! and named the missing half: a `List[int] -> str ! {utf8}` primitive
//! that VALIDATES, because an unchecked one is the forging hole rather
//! than the fix. [`__wolf_rt_str_from_utf8`] is that half, and it is
//! the only operation in the language that builds a `str` out of
//! arbitrary numbers. Its failure is a ROW (`utf8`), not a trap and not
//! undefined behaviour: refusing bytes is an outcome a caller handles,
//! which is what lets wolf-std finally write `bytes.to_str`. So "every
//! `str` in a wolf program is valid UTF-8" stays a theorem: narrowing
//! preserves it, and construction checks it.
//!
//! Two entries here are kept deliberately even though the compiler no
//! longer calls them on the hot path:
//!
//! - [`__wolf_rt_str_bytes`] is the MATERIALIZING fallback. A view that
//!   has to become a first-class `List[int]` value (a binding, an
//!   argument, a return) still goes through it, bit-for-bit as before.
//! - [`__wolf_rt_str_get`] is the reference semantics for the inline
//!   domain test, and the C-ABI entry for an FFI caller. Its domain and
//!   the compiled test are pinned equal by
//!   `inline_domain_matches_the_shim` below — the `[mem.str.get]`
//!   "same domain" law, checked rather than asserted.
//!
//! # The separator set (s84, wolf-lang#95)
//!
//! `words`/`lines`/`split` used to be spelled as whatever Rust's
//! `std::str` did — `split_whitespace`, `lines`, `split` — which meant
//! the *definition* of a wolf program's word boundaries lived in this
//! crate's host library. `[mem.str.ws]` ends that: the separator set is
//! Unicode `White_Space`, twenty-five scalars, written down and FROZEN
//! at v1. [`ws_at`] is that clause as code, and it is the only place in
//! the runtime that decides what a separator is — `words` and the
//! `trim` family both go through it, and [`words_of`]/[`lines_of`] are
//! the `[mem.str.words]`/`[mem.str.lines]` walks spelled out rather
//! than delegated.
//!
//! The compiler does not CALL any of it on the hot path: `for w in
//! s.words()` lowers to the same decision tree inline (`wolf_wir`'s
//! `ws_at_inline`), and `ws_inline_matches_the_shim` below pins the two
//! equal over every scalar — the `inline_domain_matches_the_shim`
//! precedent, one clause with two spellings that are checked against
//! each other instead of trusted to match.

use std::sync::Mutex;

use crate::io::{
    f64_shortest, render_bool_packed, render_f64_packed, render_i64_packed, render_str_packed,
};

// ------------------------------------------------ the ambient region --

const CHUNK_MIN: usize = 64 * 1024;
const ALIGN: usize = 16;

struct Arena {
    /// Raw `MaybeUninit` capacity, same rule as a region's chunks
    /// (`crate::native::Chunk`): the Rust side never reads these bytes.
    chunks: Vec<crate::native::Chunk>,
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
        // #113: no zeroing (the control experiment convicted this path
        // — ambient chunks zeroed in full made the no-region variant
        // 1.65x slower than the region pair). Same debug/release split
        // as `native::new_chunk`, same E1001/L1 reasoning. No pooling:
        // the root arena never frees.
        a.chunks.push(crate::native::new_chunk(cap));
        a.used = 0;
    }
    let used = a.used;
    a.used += size;
    let chunk = a.chunks.last_mut().expect("chunk exists");
    unsafe { chunk.as_mut_ptr().cast::<u8>().add(used) }
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

/// Write one machine word through an out slot (the single-value
/// twin of [`write_pair`]: a list header, a size, a timestamp).
///
/// # Safety
///
/// `out` must address 8 writable bytes.
pub(crate) unsafe fn write_word(out: i64, v: i64) {
    unsafe { (out as *mut i64).write(v) };
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

// ------------------------------------------ the separator set (s84) --

/// `[mem.str.ws]` — is a `White_Space` scalar encoded at the front of
/// `b`, and how many bytes wide is it?
///
/// `Some(width)` when the scalar starting at `b[0]` is one of the
/// twenty-five, `None` otherwise. `None` covers continuation bytes by
/// construction (no continuation byte is a lead byte of anything), which
/// is what lets a caller step one byte at a time through a NON-separator
/// run without ever mistaking the tail of a code point for a boundary.
///
/// Shape, and why it is a decision tree rather than a table. The set is
/// six ASCII scalars plus nineteen that all live behind four lead bytes
/// (`C2`, `E1`, `E2`, `E3`), so the ASCII half is two integer compares —
/// `b == 0x20`, or `b - 9` unsigned-below-5 — and the rest is a branch
/// that ordinary text never takes. An A/B over the `word_count` buffer
/// measured the two compares at 0.491 ns/byte against 1.003 for a
/// 256-byte lookup table and 0.513 for a 64-bit shift-mask: the table
/// loses because it turns a pure-ALU predicate into a load per byte,
/// which also stops the scan vectorizing. Data beat the table, so data
/// it is.
pub(crate) fn ws_at(b: &[u8]) -> Option<usize> {
    let b0 = *b.first()?;
    if b0 < 0x80 {
        // U+0020, and U+0009..U+000D as one unsigned range.
        return (b0 == 0x20 || b0.wrapping_sub(0x09) <= 4).then_some(1);
    }
    let b1 = *b.get(1)?;
    match b0 {
        // U+0085 NEL, U+00A0 NO-BREAK SPACE.
        0xC2 => (b1 == 0x85 || b1 == 0xA0).then_some(2),
        0xE1..=0xE3 => {
            let b2 = *b.get(2)?;
            let hit = match b0 {
                // U+1680 OGHAM SPACE MARK.
                0xE1 => b1 == 0x9A && b2 == 0x80,
                0xE2 => {
                    // U+2000..U+200A, then U+2028/U+2029/U+202F, then
                    // U+205F behind the other continuation byte.
                    (b1 == 0x80
                        && (b2.wrapping_sub(0x80) <= 0x0A
                            || b2 == 0xA8
                            || b2 == 0xA9
                            || b2 == 0xAF))
                        || (b1 == 0x81 && b2 == 0x9F)
                }
                // U+3000 IDEOGRAPHIC SPACE.
                _ => b1 == 0x80 && b2 == 0x80,
            };
            hit.then_some(3)
        }
        _ => None,
    }
}

/// `[mem.str.words]` — the maximal non-empty runs of non-separator
/// scalars, as zero-copy subslices. Never yields an empty piece; a run
/// of separators is one boundary; leading and trailing separators yield
/// nothing. This is the exact walk `wolf_wir` emits inline.
pub(crate) fn words_of(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        // Skip separators by the SCALAR's width — stepping by one here
        // would land inside a multi-byte space and open a "word" at a
        // continuation byte.
        while i < n {
            match ws_at(&b[i..]) {
                Some(w) => i += w,
                None => break,
            }
        }
        if i >= n {
            return out;
        }
        let start = i;
        // Run to the next separator one byte at a time: `ws_at` answers
        // `None` for every continuation byte, so this can only stop on a
        // code-point boundary.
        while i < n && ws_at(&b[i..]).is_none() {
            i += 1;
        }
        out.push(&s[start..i]);
    }
}

/// `[mem.str.lines]` — split on LF, absorbing one CR that immediately
/// preceded it. A trailing LF opens no final empty line; a CR with no LF
/// after it stays in the line.
pub(crate) fn lines_of(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let start = i;
        let mut j = i;
        while j < n && b[j] != b'\n' {
            j += 1;
        }
        let mut end = j;
        // `j < n` is the "an LF terminated this line" test: only then is
        // a CR part of the terminator rather than part of the text.
        if j < n && end > start && b[end - 1] == b'\r' {
            end -= 1;
        }
        out.push(&s[start..end]);
        i = j + 1;
    }
    out
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

/// Non-overlapping occurrence count. Empty needle: 0 — an empty
/// needle matches nothing, ruled by [mem.str.empty] (#56); every
/// lane answers the same.
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
/// The separator set is `[mem.str.ws]`'s, through [`ws_at`], and not the
/// host library's `char::is_whitespace`: the clause froze the set at
/// twenty-five scalars, and a delegated `str::trim` would quietly track
/// whatever the build's Rust believes instead.
///
/// # Safety
///
/// A valid str pair; `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_trim(sp: i64, sl: i64, mode: i64, out: i64) {
    let s = unsafe { view(sp, sl) };
    let b = s.as_bytes();
    let mut lo = 0usize;
    if mode != 2 {
        while let Some(w) = ws_at(&b[lo..]) {
            lo += w;
        }
    }
    let mut hi = b.len();
    if mode != 1 {
        // Backwards, and only over the trailing run: step off the
        // continuation bytes to reach the last scalar's lead byte, then
        // ask the same predicate. `str::trim_end` is O(trailing) and so
        // is this — a forward rescan would have made trimming a long
        // string of two spaces cost its whole length.
        while hi > lo {
            let mut st = hi - 1;
            while st > lo && (b[st] & 0xC0) == 0x80 {
                st -= 1;
            }
            if ws_at(&b[st..]).is_none() {
                break;
            }
            hi = st;
        }
    }
    let t = &s[lo..hi];
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
/// by the caller's contract: lowering traps `assert` first, ruled by
/// [mem.str.repeat] (#57). The clamp below stays regardless — it is
/// the only thing between a direct FFI caller and
/// `repeat(huge_negative as usize)`.
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

/// `replace(from, to)` — materialized. Empty `from`: identity — an
/// empty needle matches nothing, ruled by [mem.str.empty] (#56);
/// every lane answers the same.
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

/// The view family, MATERIALIZED as `List[str]` — the fallback, since
/// s84: `for w in s.words()` and its `lines`/`split` siblings walk the
/// receiver inline and never reach here (`[mem.str.view]`). This stays
/// for the positions that need a first-class `List[str]` value — a
/// binding, an argument, a return — exactly as `__wolf_rt_str_bytes`
/// stayed for the byte view.
///
/// `split(sep)` (mode 0, `[mem.str.split]`), `words` (mode 1,
/// `[mem.str.words]`), `lines` (mode 2, `[mem.str.lines]`). Elements are
/// zero-copy subslices of the receiver's bytes; the LIST is the only
/// allocation. Empty separator: the whole string as one element — an
/// empty separator splits nowhere, ruled by [mem.str.empty] (#56) and
/// restated as the `count(sep) + 1` identity in [mem.str.split].
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
        1 => words_of(s),
        _ => lines_of(s),
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

/// `bytes()` — the MATERIALIZING fallback (s77): a `List[int]` with
/// each byte as an i64 element, exactly the checked lane's value shape.
///
/// Compiled code no longer calls this to WALK bytes — `for b in
/// s.bytes()`, `s.bytes()[i]` and the `len`/`count`/`is_empty`/`get`/
/// `first`/`last` family read the receiver's `{ptr, len}` pair
/// directly (see the byte-view section of this module's docs). This
/// stays for the positions that need a first-class `List[int]` value:
/// a binding, a call argument, a return.
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

/// `chars()` — code-point iteration, materialized (s120, #17,
/// `[mem.str.chars]`): a `List[int]` with each Unicode scalar value
/// as an i64 element, in string order. The elements are scalars —
/// `0..=0x10FFFF` minus the surrogate gap, guaranteed by the `str`
/// invariant (every `str` is valid UTF-8, [mem.str.get]) — and a
/// scalar's UTF-8 byte extent is a function of its value
/// (`< 0x80` → 1, `< 0x800` → 2, `< 0x10000` → 3, else 4), so a
/// caller advances a byte cursor by real width without a `char`
/// type. Always materializes: a variable-width decode has no strided
/// walk, so `chars()` has no view tier yet (the pre-s89 `bytes()`
/// posture).
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_chars(sp: i64, sl: i64) -> i64 {
    let s = unsafe { view(sp, sl) };
    let hdr = crate::list::new_list(8);
    for c in s.chars() {
        let v = [i64::from(u32::from(c))];
        crate::list::push_raw(hdr, v.as_ptr().cast());
    }
    hdr as i64
}

/// `str_from_utf8(b: List[int]) -> str ! {utf8}` — the s81 border post
/// (wolf-lang#58), and the ONLY operation in the language that builds a
/// `str` out of arbitrary numbers.
///
/// 0 = accepted, with the materialized pair through `out`; 1 = the
/// `utf8` row. It VALIDATES, and that is the whole reason it exists:
/// s77 declined to add an unchecked bytes-to-str path because an
/// unchecked one is the forging hole — every other str-producing entry
/// here only ever narrows an ALREADY-valid `str` on code-point
/// boundaries, and a cast would have made "every `str` is valid UTF-8"
/// a hope instead of an invariant.
///
/// What "validates" means, precisely. Elements outside `0..=255` are
/// not bytes at all and are rejected before anything else. The byte
/// sequence then goes through `core::str::from_utf8`, which is the
/// same Rust `std::str` reference the whole module uses, so the
/// rejected set is exactly UTF-8's: a lone continuation byte, a
/// truncated multi-byte sequence, an overlong encoding, a surrogate
/// (U+D800..U+DFFF), and a scalar past U+10FFFF. A NUL byte is VALID
/// text and is accepted — wolf `str`s carry their length, so there is
/// no terminator to confuse.
///
/// Accepted bytes are copied into the ambient region, exactly like any
/// other materialization (see this module's design note): the result is
/// an ordinary owned `str`, borrowing nothing from the caller's list.
///
/// # Safety
///
/// `hdr` must be a live `List[int]` header from
/// [`crate::list::__wolf_rt_list_new`]; `out` must address 16 writable
/// bytes. A header of the wrong ELEMENT WIDTH is refused rather than
/// misread — compiled code cannot produce one (sema types the argument
/// `List[int]`), and a direct FFI caller deserves an answer instead of
/// undefined behaviour.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_str_from_utf8(hdr: i64, out: i64) -> i64 {
    let Some(elems) = (unsafe { crate::list::i64_elems(hdr) }) else {
        return 1;
    };
    let mut bytes: Vec<u8> = Vec::with_capacity(elems.len());
    for &v in elems {
        // A byte is 0..=255. Anything else is not a byte, so it cannot
        // be part of any UTF-8 encoding — the same `utf8` answer, made
        // before the decoder ever sees it.
        let Ok(b) = u8::try_from(v) else {
            return 1;
        };
        bytes.push(b);
    }
    match core::str::from_utf8(&bytes) {
        Ok(s) => {
            unsafe { write_owned(out, s) };
            0
        }
        Err(_) => 1,
    }
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

    /// s77: the byte view IS the receiver's `{ptr, len}` pair, and the
    /// i-th byte is `*(ptr + i)` — that is the whole ABI compiled code
    /// relies on (`ptr.off` at stride 1 + `load.i8`), so it is pinned
    /// here the way `list.rs` pins the header offsets. Move the pair
    /// layout and this test is what tells you the lowering moved with
    /// it.
    #[test]
    fn byte_view_is_the_receiver_pair() {
        let s = "wolf é";
        let (sp, sl) = pair_of(s);
        assert_eq!(sl as usize, s.len(), "the view's length is the byte len");
        for i in 0..s.len() {
            let byte = unsafe { *((sp as *const u8).add(i)) };
            assert_eq!(byte, s.as_bytes()[i], "byte {i} through the view");
        }
        // The pair layout the out-slot ABI writes: ptr at +0, len at +8.
        let mut out = [0i64; 2];
        unsafe { write_pair(out.as_mut_ptr() as i64, sp, sl) };
        assert_eq!((out[0], out[1]), (sp, sl));
    }

    /// s77 target 2: the whole zero-copy family hands back subslices of
    /// the RECEIVER's storage — one representation shared with the byte
    /// view, not a second one. Every pair below must point inside the
    /// receiver's bytes.
    #[test]
    fn zero_copy_results_share_the_receiver_storage() {
        let s = "  the wolf runs  ";
        let (sp, sl) = pair_of(s);
        let inside = |p: i64, l: i64| p >= sp && p + l <= sp + sl;
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        unsafe {
            __wolf_rt_str_trim(sp, sl, 0, o);
            assert!(inside(out[0], out[1]), "trim is a subslice");
            assert_eq!(__wolf_rt_str_get(sp, sl, 2, 5, o), 1);
            assert!(inside(out[0], out[1]), "get is a subslice");
            let (np, nl) = pair_of("  ");
            assert_eq!(__wolf_rt_str_strip(sp, sl, np, nl, 0, o), 1);
            assert!(inside(out[0], out[1]), "strip_prefix is a subslice");
            let hdr = __wolf_rt_str_split(sp, sl, 0, 0, 1);
            assert_eq!(crate::list::__wolf_rt_list_len(hdr), 3);
            let mut elem = [0i64; 2];
            for i in 0..3 {
                assert_eq!(
                    crate::list::__wolf_rt_list_read(hdr, i, elem.as_mut_ptr() as i64),
                    1
                );
                assert!(inside(elem[0], elem[1]), "split element {i} is a subslice");
            }
        }
    }

    /// s77 target 4: the code-point test the COMPILER inlines is the
    /// one this runtime makes. `wolf_wir::lower`'s `str_boundary_ok`
    /// spells `off == len || (byte[off] & 0xC0) != 0x80`; that must be
    /// `str::is_char_boundary`, offset for offset, or `s[a..b]` and
    /// `s.get(a..b)` would stop trapping where they trap today.
    #[test]
    fn char_boundary_rule_is_the_two_bit_test() {
        for s in ["", "wolf", "héllo", "é€🐺x", "🐺"] {
            let b = s.as_bytes();
            for off in 0..=b.len() {
                // The compiled spelling: past-the-end is a boundary and
                // reads nothing; otherwise the two top bits decide.
                let inlined = b.get(off).is_none_or(|byte| (byte & 0xC0) != 0x80);
                assert_eq!(
                    inlined,
                    s.is_char_boundary(off),
                    "boundary rule at {off} of {s:?}"
                );
            }
        }
    }

    /// s120 (#17, `[mem.str.chars]`): `chars()` yields the Unicode
    /// scalar values in string order — pinned against Rust's own
    /// decoder — and each scalar's UTF-8 width, a pure function of
    /// its value, advances a byte cursor over exactly the offsets
    /// `is_char_boundary` accepts, summing to the byte length. That
    /// walk is the whole reason the primitive exists.
    #[test]
    fn chars_are_the_scalars_in_order() {
        for s in ["", "wolf", "héllo", "é€🐺x", "a中🐺"] {
            let (sp, sl) = pair_of(s);
            let hdr = unsafe { __wolf_rt_str_chars(sp, sl) };
            let elems = unsafe { crate::list::i64_elems(hdr) }.expect("an int list");
            let expect: Vec<i64> = s.chars().map(|c| i64::from(u32::from(c))).collect();
            assert_eq!(elems, expect.as_slice(), "scalars of {s:?}");
            let mut off = 0usize;
            for &c in elems {
                off += match c {
                    0..=0x7F => 1,
                    0x80..=0x7FF => 2,
                    0x800..=0xFFFF => 3,
                    _ => 4,
                };
                assert!(
                    s.is_char_boundary(off),
                    "the width walk lands on a boundary at {off} of {s:?}"
                );
            }
            assert_eq!(off, s.len(), "the widths sum to the byte length");
        }
    }

    /// s77 target 4, the other half: the inlined DOMAIN (`lo <=u hi
    /// <=u len`, then both endpoints on boundaries) accepts exactly
    /// what `__wolf_rt_str_get` accepts — `[mem.str.get]`'s "same
    /// domain" law, checked over every endpoint pair of a mixed-width
    /// string including the out-of-range and inverted ones.
    #[test]
    fn inline_domain_matches_the_shim() {
        let s = "é€🐺x";
        let (sp, sl) = pair_of(s);
        let bytes = s.as_bytes();
        let boundary = |off: i64| -> bool { off == sl || (bytes[off as usize] & 0xC0) != 0x80 };
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        for lo in -2i64..=sl + 2 {
            for hi in -2i64..=sl + 2 {
                // The compiled test, verbatim: two unsigned compares
                // (a negative endpoint is a very large unsigned one),
                // then the boundary probes — which are only reached
                // because the range half already holds.
                let in_range = (lo as u64) <= (hi as u64) && (hi as u64) <= (sl as u64);
                let inlined = in_range && boundary(lo) && boundary(hi);
                let shim = unsafe { __wolf_rt_str_get(sp, sl, lo, hi, o) } == 1;
                assert_eq!(inlined, shim, "domain at {lo}..{hi} of {s:?}");
                if shim {
                    // And the pair is the same arithmetic the lowering
                    // does: `ptr + lo`, `hi - lo`.
                    assert_eq!((out[0], out[1]), (sp + lo, hi - lo));
                }
            }
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

    /// Build a `List[int]` of the given element values (they are `int`s,
    /// not bytes — the out-of-range ones are the point).
    fn int_list(values: &[i64]) -> i64 {
        let hdr = crate::list::new_list(8);
        for &v in values {
            let cell = [v];
            crate::list::push_raw(hdr, cell.as_ptr().cast());
        }
        hdr as i64
    }

    fn from_utf8(values: &[i64]) -> Result<String, ()> {
        let hdr = int_list(values);
        let mut out = [0i64; 2];
        let rc = unsafe { __wolf_rt_str_from_utf8(hdr, out.as_mut_ptr() as i64) };
        if rc == 0 {
            Ok(read_pair(&out))
        } else {
            Err(())
        }
    }

    /// s81 target 2 (#58): the border post ACCEPTS text and REFUSES
    /// everything UTF-8 refuses — and refuses it as a return code the
    /// caller turns into the `utf8` row, never as a trap. These are the
    /// ugly inputs by name, because "it validates" is a claim and this
    /// is the evidence.
    #[test]
    fn from_utf8_accepts_text_and_refuses_the_ugly_inputs() {
        // Accepted: ASCII, multi-byte, astral, empty, and — deliberately
        // — a string carrying an interior NUL. A wolf `str` is
        // {ptr, len}: there is no terminator for a NUL to end.
        assert_eq!(from_utf8(&[]), Ok(String::new()));
        assert_eq!(from_utf8(&[119, 111, 108, 102]), Ok("wolf".to_string()));
        assert_eq!(from_utf8(&[0xC3, 0xA9]), Ok("é".to_string())); // U+00E9
        assert_eq!(from_utf8(&[0xE2, 0x82, 0xAC]), Ok("€".to_string())); // U+20AC
        assert_eq!(
            from_utf8(&[0xF0, 0x9F, 0x90, 0xBA]),
            Ok("🐺".to_string()) // U+1F43A, four bytes
        );
        let nul = from_utf8(&[119, 0, 102]).expect("a NUL is valid text");
        assert_eq!(nul.len(), 3);
        assert_eq!(nul.as_bytes()[1], 0);

        // Refused, one named failure mode at a time.
        assert!(from_utf8(&[0x80]).is_err(), "a lone continuation byte");
        assert!(from_utf8(&[0xBF, 0xBF]).is_err(), "continuations only");
        assert!(from_utf8(&[0xE2, 0x82]).is_err(), "a truncated sequence");
        assert!(
            from_utf8(&[0xF0, 0x9F, 0x90]).is_err(),
            "a truncated 4-byte sequence"
        );
        assert!(
            from_utf8(&[0xC0, 0xAF]).is_err(),
            "an overlong encoding of '/'"
        );
        assert!(
            from_utf8(&[0xE0, 0x80, 0xAF]).is_err(),
            "a 3-byte overlong form"
        );
        assert!(
            from_utf8(&[0xED, 0xA0, 0x80]).is_err(),
            "a surrogate (U+D800)"
        );
        assert!(
            from_utf8(&[0xF4, 0x90, 0x80, 0x80]).is_err(),
            "a scalar past U+10FFFF"
        );
        assert!(from_utf8(&[0xFE]).is_err(), "a byte UTF-8 never uses");

        // And the elements that are not bytes at all. `List[int]` holds
        // `int`s, so a caller can hand over 256 or -1; neither is a
        // byte, so neither can be part of any encoding.
        assert!(from_utf8(&[256]).is_err(), "an element above 255");
        assert!(from_utf8(&[-1]).is_err(), "a negative element");
        assert!(
            from_utf8(&[119, 111, 108, 300]).is_err(),
            "one bad element poisons the whole sequence"
        );
    }

    // ------------------------------- the separator set (s84, #95) ---

    /// `[mem.str.ws]`'s twenty-five scalars, written out. This list is
    /// the CLAUSE, not a copy of one: the spec freezes the set at v1,
    /// so it is spelled here as data and every other spelling in the
    /// tree is checked against it.
    const WHITE_SPACE: [char; 25] = [
        '\u{0009}', '\u{000A}', '\u{000B}', '\u{000C}', '\u{000D}', '\u{0020}', '\u{0085}',
        '\u{00A0}', '\u{1680}', '\u{2000}', '\u{2001}', '\u{2002}', '\u{2003}', '\u{2004}',
        '\u{2005}', '\u{2006}', '\u{2007}', '\u{2008}', '\u{2009}', '\u{200A}', '\u{2028}',
        '\u{2029}', '\u{202F}', '\u{205F}', '\u{3000}',
    ];

    /// `ws_at` decides EXACTLY `[mem.str.ws]`'s set, over every scalar
    /// there is — not a sample, because a decision tree over lead bytes
    /// is precisely the shape whose bugs hide in one unvisited branch.
    /// The width it reports is the scalar's UTF-8 width, which is what
    /// keeps the skip loop landing on code-point boundaries.
    #[test]
    fn ws_at_is_the_twenty_five_scalars() {
        let mut seen = Vec::new();
        for c in 0u32..=0x10FFFF {
            let Some(ch) = char::from_u32(c) else {
                continue;
            };
            let mut buf = [0u8; 4];
            let want = WHITE_SPACE.contains(&ch);
            let wide = ch.encode_utf8(&mut buf).len();
            let got = ws_at(&buf[..wide]);
            assert_eq!(got.is_some(), want, "ws_at on U+{c:04X}");
            if want {
                assert_eq!(got, Some(wide), "width of U+{c:04X}");
                seen.push(ch);
            }
        }
        assert_eq!(seen, WHITE_SPACE, "the set, in order");
    }

    /// No continuation byte is ever a separator START. This is the
    /// invariant the word walk leans on when it steps one byte at a
    /// time through a word: if a `10xxxxxx` byte could answer "yes",
    /// the walk would open a field in the middle of a code point and
    /// hand back a `str` `[mem.str.get]` would have refused.
    #[test]
    fn no_continuation_byte_starts_a_separator() {
        for b in 0x80u8..=0xBF {
            assert_eq!(ws_at(&[b, 0x80, 0x80]), None, "continuation {b:#04x}");
        }
    }

    /// The compiled spelling, transcribed. `wolf_wir::lower`'s
    /// `ws_at_inline` emits this decision tree as WIR — every byte
    /// through `zext` into an `i64`, so the compares below are on
    /// `i64`s in 0..=255 and the unsigned ones are `icmp.ule` — and it
    /// is a SECOND implementation of `[mem.str.ws]`, which means the
    /// clause has two spellings that can disagree. The next test is
    /// what stops them, exactly as `inline_domain_matches_the_shim`
    /// stops `[mem.str.get]`'s two spellings.
    fn ws_inline(b: &[u8]) -> Option<usize> {
        // U+2000..U+200A (bits 0..10), U+2028, U+2029 (40, 41), U+202F
        // (47), as one mask over `cp - 0x2000` — the `iconst.i64
        // 144036023240703` in the dump.
        const E2_MASK: i64 = (1 << 47) | (1 << 41) | (1 << 40) | 0x7FF;
        let b0 = i64::from(*b.first()?);
        if b0 < 0x80 {
            // `icmp.eq b0, 32`, then `isub.wrap b0, 9` + `icmp.ule 4`.
            let ws = b0 == 0x20 || (b0.wrapping_sub(9) as u64) <= 4;
            return ws.then_some(1);
        }
        if b0 == 0xC2 {
            let b1 = i64::from(*b.get(1)?);
            return (b1 == 0x85 || b1 == 0xA0).then_some(2);
        }
        // `isub.wrap b0, 0xE1` + `icmp.ule 2` — the only other lead
        // bytes that can begin a separator.
        if (b0.wrapping_sub(0xE1) as u64) > 2 {
            return None;
        }
        let b1 = i64::from(*b.get(1)?);
        let b2 = i64::from(*b.get(2)?);
        let cp = ((b0 & 0x0F) << 12) | ((b1 & 0x3F) << 6) | (b2 & 0x3F);
        let off = cp.wrapping_sub(0x2000);
        let hit = if (off as u64) <= 0x2F {
            (E2_MASK >> off) & 1 != 0
        } else {
            cp == 0x1680 || cp == 0x3000 || cp == 0x205F
        };
        hit.then_some(3)
    }

    /// The COMPILED predicate and the runtime one answer the same
    /// question, over every scalar there is. A decision tree over lead
    /// bytes is precisely the shape whose bugs hide in one unvisited
    /// branch, and the two trees are written differently on purpose —
    /// the runtime matches on bytes, the compiled one decodes to the
    /// scalar — so agreeing here is evidence rather than tautology.
    #[test]
    fn ws_inline_matches_the_shim() {
        for c in 0u32..=0x10FFFF {
            let Some(ch) = char::from_u32(c) else {
                continue;
            };
            let mut buf = [0u8; 4];
            let wide = ch.encode_utf8(&mut buf).len();
            assert_eq!(
                ws_inline(&buf[..wide]),
                ws_at(&buf[..wide]),
                "the two spellings of [mem.str.ws] at U+{c:04X}"
            );
        }
        // And over EVERY first byte — including the continuation bytes
        // and the lead bytes UTF-8 never uses — with the trailing two
        // ranging over the continuation range. That range is the whole
        // domain, and stating it is the point: the compiled tree reads
        // `base[i+1]`/`base[i+2]` only after the lead byte has promised
        // they exist, and it MASKS them with 0x3F to decode, so
        // `E1 9A 00` (which the runtime's byte match rejects and the
        // decode accepts as U+1680) is a real disagreement on an input
        // no `str` can hold. Widening this loop past 0x80..=0xBF would
        // be asserting something neither spelling promises.
        for b0 in 0u8..=255 {
            for b1 in 0x80u8..=0xBF {
                for b2 in [0x80u8, 0x8A, 0x8B, 0x9F, 0xA8, 0xA9, 0xAF, 0xB0, 0xBF] {
                    let raw = [b0, b1, b2];
                    assert_eq!(
                        ws_inline(&raw),
                        ws_at(&raw),
                        "the two spellings on {raw:02X?}"
                    );
                }
            }
        }
    }

    /// The set is FROZEN at v1, and the host library is not the
    /// authority — but a drift between them is a REVIEW EVENT, so it is
    /// checked. If this fails, Unicode (or Rust's table) changed: decide
    /// deliberately whether `[mem.str.ws]` moves, and change the clause
    /// first if it does.
    #[test]
    fn the_frozen_set_still_matches_the_host_table() {
        let host: Vec<char> = (0u32..=0x10FFFF)
            .filter_map(char::from_u32)
            .filter(|c| c.is_whitespace())
            .collect();
        assert_eq!(
            host, WHITE_SPACE,
            "the host's White_Space table drifted from [mem.str.ws]"
        );
    }

    /// `[mem.str.words]`, `[mem.str.lines]`, `[mem.str.split]` on the
    /// inputs that decide them. These are the ugly ones by name — the
    /// happy path was never in doubt and never told anyone anything.
    #[test]
    fn the_walks_answer_the_clause_on_the_ugly_inputs() {
        let w = |s: &str| words_of(s).join("|");
        // Empty, and only-separators: no words at all.
        assert_eq!(w(""), "");
        assert_eq!(w("   "), "");
        assert_eq!(w("\t\r\n"), "");
        assert_eq!(w("\u{a0}\u{3000}"), "");
        // Leading, trailing, and RUNS collapse to one boundary each.
        assert_eq!(w(" a "), "a");
        assert_eq!(w("a  b"), "a|b");
        assert_eq!(w("\t\ta\r\n\r\nb\t"), "a|b");
        // Every non-ASCII separator, and the width handling behind it.
        assert_eq!(w("a\u{a0}b"), "a|b");
        assert_eq!(w("a\u{85}b"), "a|b");
        assert_eq!(w("a\u{1680}b"), "a|b");
        assert_eq!(w("a\u{2028}b\u{2029}c"), "a|b|c");
        assert_eq!(w("a\u{202f}b\u{205f}c"), "a|b|c");
        assert_eq!(w("a\u{3000}b"), "a|b");
        assert_eq!(w("  \u{2000}\u{2001}x  "), "x");
        // A non-separator multi-byte scalar stays INSIDE its word — the
        // one-byte step through a word must not split it.
        assert_eq!(w(" é€🐺 "), "é€🐺");
        assert_eq!(w("é\u{a0}🐺"), "é|🐺");
        // A word is never empty, which is the whole point of the ruling.
        for s in ["", "  ", " a ", "a  b", "\u{3000}a\u{3000}"] {
            assert!(words_of(s).iter().all(|p| !p.is_empty()), "words of {s:?}");
        }

        let l = |s: &str| lines_of(s).join("|");
        assert_eq!(l(""), "");
        assert_eq!(
            lines_of(""),
            Vec::<&str>::new(),
            "an empty str has no lines"
        );
        assert_eq!(l("a"), "a");
        assert_eq!(l("a\n"), "a");
        assert_eq!(lines_of("a\n").len(), 1, "a trailing LF opens no line");
        assert_eq!(l("\n"), "");
        assert_eq!(lines_of("\n").len(), 1, "one LF is one empty line");
        assert_eq!(l("a\n\nb"), "a||b");
        assert_eq!(l("a\r\nb\r\n"), "a|b");
        assert_eq!(l("\r\n"), "");
        // A CR with no LF after it is TEXT, not a terminator.
        assert_eq!(l("a\r"), "a\r");
        assert_eq!(l("a\rb"), "a\rb");
        // The Unicode line separators are separators for `words` and
        // NOT terminators here — the two clauses disagree on purpose.
        assert_eq!(lines_of("a\u{2028}b").len(), 1);
        assert_eq!(lines_of("a\u{85}b").len(), 1);
    }

    /// `[mem.str.split]`'s identity: `split(sep)` yields exactly
    /// `count(sep) + 1` fields, for EVERY receiver and separator —
    /// including the empty one, where `[mem.str.empty]`'s 0 and the one
    /// whole-string field are the same rule rather than an exception.
    #[test]
    fn split_yields_count_plus_one_fields() {
        let cases = [
            ("", ""),
            ("", ","),
            (",", ","),
            (",,", ","),
            (",a,,b,", ","),
            ("abab", "ab"),
            ("aaa", "aa"),
            ("xabax", "ab"),
            ("wolf", ","),
            ("a\r\nb", "\r\n"),
            ("é€🐺", "€"),
            ("hello", ""),
        ];
        for (s, sep) in cases {
            let (sp, sl) = pair_of(s);
            let (np, nl) = pair_of(sep);
            unsafe {
                let hdr = __wolf_rt_str_split(sp, sl, np, nl, 0);
                let fields = crate::list::__wolf_rt_list_len(hdr);
                let count = __wolf_rt_str_count(sp, sl, np, nl);
                assert_eq!(fields, count + 1, "split({s:?}, {sep:?}) field count");
            }
        }
    }

    /// The materializing shim answers the same walks the clause defines
    /// — the fallback and the hot path are one definition.
    #[test]
    fn the_split_shim_rides_the_clause_walks() {
        let s = "  the\u{a0}quick\r\nbrown  ";
        let (sp, sl) = pair_of(s);
        let z = 0i64;
        for (mode, want) in [(1i64, words_of(s)), (2, lines_of(s))] {
            let hdr = unsafe { __wolf_rt_str_split(sp, sl, sp, z, mode) };
            assert_eq!(
                unsafe { crate::list::__wolf_rt_list_len(hdr) },
                want.len() as i64,
                "mode {mode} element count"
            );
            let mut elem = [0i64; 2];
            for (i, w) in want.iter().enumerate() {
                unsafe {
                    crate::list::__wolf_rt_list_read(hdr, i as i64, elem.as_mut_ptr() as i64);
                    assert_eq!(&view(elem[0], elem[1]), w, "mode {mode} element {i}");
                }
                // And it is a VIEW: inside the receiver's own storage.
                assert!(elem[0] >= sp && elem[0] + elem[1] <= sp + sl);
            }
        }
    }

    /// `trim` moved off `str::trim` onto [`ws_at`] so one frozen set
    /// serves the whole family; the answers must not have moved with it.
    #[test]
    fn trim_uses_the_frozen_set() {
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        for s in [
            "",
            "   ",
            "  the wolf  ",
            "\u{a0}\u{2000}x\u{3000}\u{202f}",
            "\u{85}",
            "é",
            " é ",
            "\u{a0}",
            "no-space",
            "\t\r\n mixed \u{205f}",
        ] {
            let (sp, sl) = pair_of(s);
            for (mode, want) in [(0i64, s.trim()), (1, s.trim_start()), (2, s.trim_end())] {
                unsafe { __wolf_rt_str_trim(sp, sl, mode, o) };
                assert_eq!(read_pair(&out), want, "trim mode {mode} of {s:?}");
                if !s.is_empty() {
                    assert!(out[0] >= sp && out[0] + out[1] <= sp + sl, "a subslice");
                }
            }
        }
    }

    /// The accepted bytes are OWNED: they land in the ambient region,
    /// not in the caller's list buffer. A `str` that borrowed a mutable
    /// `List` would be exactly the forging hole this primitive exists to
    /// close — the list could be written after the check.
    #[test]
    fn from_utf8_materializes_into_the_ambient_region() {
        let hdr = int_list(&[119, 111, 108, 102]);
        let mut out = [0i64; 2];
        let rc = unsafe { __wolf_rt_str_from_utf8(hdr, out.as_mut_ptr() as i64) };
        assert_eq!(rc, 0);
        assert_eq!(read_pair(&out), "wolf");
        // Overwrite the source list; the str must not move with it.
        let poison = [0x80i64];
        assert_eq!(
            unsafe { crate::list::__wolf_rt_list_write(hdr, 0, poison.as_ptr() as i64) },
            1
        );
        assert_eq!(read_pair(&out), "wolf", "the str owns its bytes");
    }
}
