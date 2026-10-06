//! s200 (wolf-lang#411, `[mem.list.bytes]`): the bulk byte scan.
//!
//! `bytes_find(xs, b, from) -> int ! {none}` and `bytes_count(xs, b) ->
//! int` over a `List[byte]`. Counting newlines is the archetypal bulk
//! byte operation (`wc -l`, a line splitter, a tokenizer's skip), and a
//! wolf loop over the elements cannot reach it: boreutils measured its
//! scalar `wc -l` at 0.13x GNU on x86-64 (#411), and nothing in the
//! language lets eight bytes enter one word. These two calls hand the
//! scan to the runtime, on every tier that links it (native and release
//! alike).
//!
//! The scans are written with the target's BASELINE vector instructions
//! — SSE2 on x86-64, NEON on aarch64, both present on every CPU of the
//! target, so nothing is detected at run time — and a scalar loop on
//! every other architecture:
//!
//! - **count** compares 16 bytes at a time and subtracts the all-ones
//!   lanes from a byte-wide accumulator, which cannot overflow in 255
//!   rounds; then it widens (`psadbw`; `uaddlv`) and starts again;
//! - **find** compares 16 bytes at a time and asks the compare's mask
//!   (`pmovmskb`; `umaxv`) whether any lane hit, and walks only the
//!   16 bytes that answered yes.
//!
//! Intrinsics, and not a loop left to the optimiser, because the loop was
//! tried first: written over 32-byte rows into a `[u8; 32]` accumulator,
//! LLVM vectorised it ACROSS rows, gathering one byte per row with scalar
//! loads (s200's first cut: 67.7 ms to count 256 MiB on kasumi, against
//! 93.7 ms for the plain wolf loop; `bench/byte-scan/`). The crate tests
//! hold every path to the scalar definition.

/// Rounds a byte-wide lane can count before it could wrap.
const ROUNDS: usize = 255;

/// How many bytes of `xs` equal `b`.
pub fn count(xs: &[u8], b: u8) -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        count_sse2(xs, b)
    }
    #[cfg(target_arch = "aarch64")]
    {
        count_neon(xs, b)
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        count_scalar(xs, b)
    }
}

/// The scalar definition: every architecture's fallback, and the tests'
/// reference.
pub fn count_scalar(xs: &[u8], b: u8) -> usize {
    xs.iter().filter(|&&x| x == b).count()
}

#[cfg(target_arch = "x86_64")]
fn count_sse2(xs: &[u8], b: u8) -> usize {
    use core::arch::x86_64::{
        _mm_cmpeq_epi8, _mm_cvtsi128_si64, _mm_loadu_si128, _mm_sad_epu8, _mm_set1_epi8,
        _mm_setzero_si128, _mm_sub_epi8, _mm_unpackhi_epi64,
    };
    let mut rows = xs.chunks_exact(16);
    let mut total = 0usize;
    // SAFETY: SSE2 is part of the x86-64 baseline, and every load reads
    // 16 bytes of a 16-byte chunk (unaligned loads).
    unsafe {
        let needle = _mm_set1_epi8(b as i8);
        let zero = _mm_setzero_si128();
        let mut acc = zero;
        let mut rounds = 0;
        for row in &mut rows {
            let v = _mm_loadu_si128(row.as_ptr().cast());
            acc = _mm_sub_epi8(acc, _mm_cmpeq_epi8(v, needle));
            rounds += 1;
            if rounds == ROUNDS {
                let sums = _mm_sad_epu8(acc, zero);
                total += _mm_cvtsi128_si64(sums) as usize
                    + _mm_cvtsi128_si64(_mm_unpackhi_epi64(sums, sums)) as usize;
                acc = zero;
                rounds = 0;
            }
        }
        let sums = _mm_sad_epu8(acc, zero);
        total += _mm_cvtsi128_si64(sums) as usize
            + _mm_cvtsi128_si64(_mm_unpackhi_epi64(sums, sums)) as usize;
    }
    total + count_scalar(rows.remainder(), b)
}

#[cfg(target_arch = "aarch64")]
fn count_neon(xs: &[u8], b: u8) -> usize {
    use core::arch::aarch64::{vaddlvq_u8, vceqq_u8, vdupq_n_u8, vld1q_u8, vsubq_u8};
    let mut rows = xs.chunks_exact(16);
    let mut total = 0usize;
    // SAFETY: NEON is part of the aarch64 baseline, and every load reads
    // 16 bytes of a 16-byte chunk.
    unsafe {
        let needle = vdupq_n_u8(b);
        let mut acc = vdupq_n_u8(0);
        let mut rounds = 0;
        for row in &mut rows {
            let v = vld1q_u8(row.as_ptr());
            acc = vsubq_u8(acc, vceqq_u8(v, needle));
            rounds += 1;
            if rounds == ROUNDS {
                total += usize::from(vaddlvq_u8(acc));
                acc = vdupq_n_u8(0);
                rounds = 0;
            }
        }
        total += usize::from(vaddlvq_u8(acc));
    }
    total + count_scalar(rows.remainder(), b)
}

/// The first index of `b` in `xs`, if any.
pub fn find(xs: &[u8], b: u8) -> Option<usize> {
    #[cfg(target_arch = "x86_64")]
    {
        find_sse2(xs, b)
    }
    #[cfg(target_arch = "aarch64")]
    {
        find_neon(xs, b)
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        find_scalar(xs, b)
    }
}

/// The scalar definition of [`find`].
pub fn find_scalar(xs: &[u8], b: u8) -> Option<usize> {
    xs.iter().position(|&x| x == b)
}

#[cfg(target_arch = "x86_64")]
fn find_sse2(xs: &[u8], b: u8) -> Option<usize> {
    use core::arch::x86_64::{_mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_set1_epi8};
    let mut rows = xs.chunks_exact(16);
    let mut at = 0usize;
    // SAFETY: as `count_sse2`.
    unsafe {
        let needle = _mm_set1_epi8(b as i8);
        for row in &mut rows {
            let v = _mm_loadu_si128(row.as_ptr().cast());
            let mask = _mm_movemask_epi8(_mm_cmpeq_epi8(v, needle));
            if mask != 0 {
                return Some(at + mask.trailing_zeros() as usize);
            }
            at += 16;
        }
    }
    find_scalar(rows.remainder(), b).map(|p| at + p)
}

#[cfg(target_arch = "aarch64")]
fn find_neon(xs: &[u8], b: u8) -> Option<usize> {
    use core::arch::aarch64::{vceqq_u8, vdupq_n_u8, vld1q_u8, vmaxvq_u8};
    let mut rows = xs.chunks_exact(16);
    let mut at = 0usize;
    // SAFETY: as `count_neon`.
    unsafe {
        let needle = vdupq_n_u8(b);
        for row in &mut rows {
            let v = vld1q_u8(row.as_ptr());
            if vmaxvq_u8(vceqq_u8(v, needle)) != 0 {
                return find_scalar(row, b).map(|p| at + p);
            }
            at += 16;
        }
    }
    find_scalar(rows.remainder(), b).map(|p| at + p)
}

/// `bytes_find(xs, b, from) -> int ! {none}`: the first index `i >=
/// from` with `xs[i] == b`, or -1 for the `none` row — an absent byte,
/// or a `from` outside `0..len` (`xs.get`'s posture: absence is a row,
/// never a trap). `b` arrives zero-extended.
///
/// # Safety
///
/// `hdr` must be a live `List[byte]` header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_bytes_find(hdr: i64, b: i64, from: i64) -> i64 {
    let Some(xs) = (unsafe { crate::list::u8_elems(hdr) }) else {
        return -1;
    };
    let Some(start) = usize::try_from(from).ok().filter(|&s| s < xs.len()) else {
        return -1;
    };
    find(&xs[start..], b as u8).map_or(-1, |i| (start + i) as i64)
}

/// `bytes_count(xs, b) -> int`: how many elements of `xs` equal `b`.
///
/// # Safety
///
/// `hdr` must be a live `List[byte]` header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_bytes_count(hdr: i64, b: i64) -> i64 {
    let Some(xs) = (unsafe { crate::list::u8_elems(hdr) }) else {
        return 0;
    };
    count(xs, b as u8) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic byte stream (xorshift), so the property runs the
    /// same on every host.
    fn bytes(seed: u64, n: usize) -> Vec<u8> {
        let mut x = seed | 1;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                // A small alphabet, so every target byte is common.
                (x % 7) as u8 + b'\n' - 3
            })
            .collect()
    }

    /// The scans against the scalar definitions, at every length around
    /// the vector edges (16, 255 * 16) and at several alignments.
    #[test]
    fn the_scans_agree_with_the_scalar_loops() {
        for n in [
            0, 1, 31, 32, 33, 63, 64, 8159, 8160, 8161, 16_320, 16_321, 70_000,
        ] {
            let xs = bytes(n as u64 + 7, n);
            for b in [b'\n', b'\n' - 3, b'\n' + 3, 0, 0xff] {
                let want = xs.iter().filter(|&&x| x == b).count();
                assert_eq!(count(&xs, b), want, "count n={n} b={b}");
                for off in [0, 1, 5, 31] {
                    let s = &xs[off.min(n)..];
                    assert_eq!(
                        find(s, b),
                        s.iter().position(|&x| x == b),
                        "find n={n} b={b}"
                    );
                }
            }
        }
    }

    /// A lane that sees the byte in every one of its 255 rounds reaches
    /// 255 and no further — the widening edge is where an off-by-one
    /// would wrap.
    #[test]
    fn a_block_of_nothing_but_the_byte_counts_every_one() {
        for n in [
            16 * ROUNDS - 1,
            16 * ROUNDS,
            16 * ROUNDS + 1,
            3 * 16 * ROUNDS + 17,
        ] {
            assert_eq!(count(&vec![b'\n'; n], b'\n'), n);
        }
    }

    /// The entries over a real list header: `from` outside `0..len` is
    /// the `none` row (-1), a hit is absolute, never relative to `from`.
    #[test]
    fn the_entries_read_a_list_header() {
        let hdr = crate::list::from_bytes(b"ab\ncd\n") as i64;
        unsafe {
            assert_eq!(__wolf_rt_bytes_count(hdr, i64::from(b'\n')), 2);
            assert_eq!(__wolf_rt_bytes_find(hdr, i64::from(b'\n'), 0), 2);
            assert_eq!(__wolf_rt_bytes_find(hdr, i64::from(b'\n'), 3), 5);
            assert_eq!(__wolf_rt_bytes_find(hdr, i64::from(b'z'), 0), -1);
            assert_eq!(__wolf_rt_bytes_find(hdr, i64::from(b'\n'), 6), -1);
            assert_eq!(__wolf_rt_bytes_find(hdr, i64::from(b'\n'), -1), -1);
        }
    }
}
