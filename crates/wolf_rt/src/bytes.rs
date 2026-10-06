//! s200 (wolf-lang#411, `[mem.list.bytes]`): the bulk byte scan.
//!
//! `bytes_find(xs, b, from) -> int ! {none}` and `bytes_count(xs, b) ->
//! int` over a `List[byte]`. Counting newlines is the archetypal bulk
//! byte operation (`wc -l`, a line splitter, a tokenizer's skip), and a
//! wolf loop over the elements cannot reach it: boreutils measured its
//! scalar `wc -l` at 0.13x GNU on x86-64 (#411), and nothing in the
//! language lets eight bytes enter one word. These two calls hand the
//! scan to a loop written so the optimiser that builds this crate turns
//! it into vector compares — SSE2 on x86-64, NEON on aarch64, the
//! baseline of each target, so no runtime feature detection — on every
//! tier that links the runtime (native and release alike).
//!
//! The loops are shaped for the vectoriser, not for a reader:
//!
//! - **count** sums byte-wide lanes for at most 255 rows of 32 bytes
//!   before widening (a lane cannot overflow in 255 rows), so the hot
//!   loop is one compare and one subtract per 16 bytes;
//! - **find** asks "is it in this 32-byte block?" as an OR over the
//!   block's compares, and only the block that answers yes is walked a
//!   byte at a time.

/// Lanes per row in both scans.
const LANES: usize = 32;

/// Rows a byte-wide lane can count before it could wrap.
const ROWS: usize = 255;

/// How many bytes of `xs` equal `b`.
pub fn count(xs: &[u8], b: u8) -> usize {
    let mut total = 0usize;
    let mut blocks = xs.chunks_exact(LANES * ROWS);
    for block in &mut blocks {
        let mut acc = [0u8; LANES];
        for row in block.chunks_exact(LANES) {
            for (a, &x) in acc.iter_mut().zip(row) {
                *a += u8::from(x == b);
            }
        }
        total += acc.iter().map(|&a| usize::from(a)).sum::<usize>();
    }
    let rest = blocks.remainder();
    let mut rows = rest.chunks_exact(LANES);
    let mut acc = [0u8; LANES];
    for row in &mut rows {
        for (a, &x) in acc.iter_mut().zip(row) {
            *a += u8::from(x == b);
        }
    }
    total += acc.iter().map(|&a| usize::from(a)).sum::<usize>();
    total + rows.remainder().iter().filter(|&&x| x == b).count()
}

/// The first index of `b` in `xs`, if any.
pub fn find(xs: &[u8], b: u8) -> Option<usize> {
    let mut rows = xs.chunks_exact(LANES);
    let mut at = 0usize;
    for row in &mut rows {
        if row.iter().fold(false, |hit, &x| hit | (x == b)) {
            return row.iter().position(|&x| x == b).map(|p| at + p);
        }
        at += LANES;
    }
    rows.remainder()
        .iter()
        .position(|&x| x == b)
        .map(|p| at + p)
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

    /// The scans against the obvious scalar loops, at every length
    /// around the block edges (32, 255 * 32) and every alignment.
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

    /// A lane that sees the byte in every one of its 255 rows reaches
    /// 255 and no further — the block edge is where an off-by-one would
    /// wrap.
    #[test]
    fn a_block_of_nothing_but_the_byte_counts_every_one() {
        for n in [
            LANES * ROWS - 1,
            LANES * ROWS,
            LANES * ROWS + 1,
            3 * LANES * ROWS + 17,
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
