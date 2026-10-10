//! s222 (wolf-lang#635): the read family's reusable buffer.
//!
//! Every read shim — `net_read`, `net_read_bytes`, `fs_read`,
//! `fs_read_chunk`, `fs_read_at` — used to allocate `vec![0u8; max]`
//! (clamped to 1 MiB), let the kernel overwrite the front of it, copy
//! the `n` bytes read into the ambient region, and free it: a zeroed
//! allocation and a free per call for bytes nobody reads. ws54
//! profiled it on a lobo hand (`alloc_zeroed` under `read_shim`;
//! s222 found `fs_read_chunk`, lobo's body read, paying the same).
//!
//! Here each thread keeps ONE buffer, zero-filled only when it grows
//! (so it is always initialized memory and `Read::read` may borrow it
//! as `&mut [u8]` soundly), never longer than the 1 MiB clamp. A call
//! takes the buffer out of the thread-local for its duration and puts
//! it back after, so a re-entrant use (none exists today) would get a
//! fresh buffer rather than an aliased one, and a park in the middle
//! of a read holds the buffer by value. The result is still COPIED
//! into the ambient region exactly as before: what a program gets is
//! unchanged, only the staging buffer stopped being per-call.
//!
//! Cost: at most 1 MiB per thread that has read with a `max` that
//! large; lobo's hands read with 64 KiB.

use std::cell::Cell;

/// The read family's clamp on `max` (the checked lane's, `[os.fs.read]`).
pub(crate) const READ_CLAMP: usize = 1 << 20;

/// A positive `max`, clamped.
pub(crate) fn clamp(max: i64) -> usize {
    (max.max(0) as u64).min(READ_CLAMP as u64) as usize
}

thread_local! {
    static READ_BUF: Cell<Vec<u8>> = const { Cell::new(Vec::new()) };
}

/// Run `f` over a `len`-byte buffer (`len <= READ_CLAMP`) whose
/// contents are unspecified initialized bytes — whatever an earlier
/// read left, or zeros. `f` must not read what it has not written.
pub(crate) fn with_read_buf<R>(len: usize, f: impl FnOnce(&mut [u8]) -> R) -> R {
    debug_assert!(len <= READ_CLAMP);
    let mut buf = READ_BUF.try_with(Cell::take).unwrap_or_default();
    if buf.len() < len {
        buf.reserve_exact(len - buf.len());
        buf.resize(len, 0);
    }
    let r = f(&mut buf[..len]);
    let _ = READ_BUF.try_with(|c| c.set(buf));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buffer_is_reused_and_grows_once() {
        let a = with_read_buf(64, |b| b.as_ptr() as usize);
        let b = with_read_buf(32, |b| b.as_ptr() as usize);
        assert_eq!(a, b, "a smaller read reuses the buffer");
        let c = with_read_buf(128, |b| {
            assert_eq!(b.len(), 128);
            b.as_ptr() as usize
        });
        let d = with_read_buf(128, |b| b.as_ptr() as usize);
        assert_eq!(c, d, "after growing, the grown buffer is reused");
    }

    #[test]
    fn a_nested_use_gets_its_own_buffer() {
        with_read_buf(16, |outer| {
            outer.fill(7);
            with_read_buf(16, |inner| inner.fill(9));
            assert!(
                outer.iter().all(|&x| x == 7),
                "the nested call did not alias"
            );
        });
    }
}
