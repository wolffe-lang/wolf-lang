//! The OS random source (s118, wolf-lang#143) — `os_random(n)`.
//!
//! One entry, one meaning: fill a buffer with bytes the OPERATING
//! SYSTEM asserts are unpredictable (its CSPRNG — the same pool that
//! keys the host's own TLS stacks). There is deliberately NO userspace
//! generator here, no seeding API, no "fast" variant, and no fallback:
//! this module either hands back OS entropy or reports failure, and
//! the lowering turns that failure into a TRAP (`[os.random.trap]`,
//! the X3 temperament). A cryptographic caller that receives
//! predictable bytes while believing them unpredictable is the worst
//! failure this language could ship — a degraded path would be a lie
//! with a keyboard.
//!
//! # Platform matrix (spec `[os.random.platform]`)
//!
//! - **Linux:** `getrandom(2)`, flags 0 — the modern call: no
//!   `/dev/urandom` file descriptor to exhaust, no chroot failure
//!   mode. It blocks only until the kernel pool is initialized (the
//!   call's own contract — the honest handling of the early-boot
//!   case), then never again. `EINTR` retries; short reads (possible
//!   for requests over 256 bytes) continue where they stopped.
//! - **macOS / FreeBSD:** `getentropy(3)` in 256-byte chunks (the
//!   call's own per-request cap; a larger request is `EIO` by
//!   contract, so the chunking is correctness, not tuning).
//! - **Windows (tier-1):** `BCryptGenRandom` with
//!   `BCRYPT_USE_SYSTEM_PREFERRED_RNG` — the documented modern call
//!   (`RtlGenRandom` is its deprecated alias). Declared directly
//!   against `bcrypt.dll`; no crate (D15).
//! - **Anything else:** `fill` answers `false` and the surface traps —
//!   a NAMED gap, never silence and never a PRNG.
//!
//! Unlike `signal`/`reactor`/`task` this module is NOT linux-gated:
//! the entropy call has no dependency on the native concurrency layer,
//! so the macOS/Windows arms are compiled and unit-tested by the
//! host-matrix CI today even though no codegen target reaches them
//! yet (compiled-program delivery follows the backend port — the
//! s114 discipline; the gap is the LANE, not this call).
//!
//! # Why failure is a trap and not a row
//!
//! Every other os-tier failure is a D30 row because the caller has a
//! legitimate different response to it (retry, report, fall back).
//! There is no legitimate response to "the OS cannot provide
//! entropy" that continues toward key generation — a row invites
//! exactly the `else`-arm fallback this surface exists to make
//! impossible. So the shim's nonzero code becomes `trap(assert)` in
//! lowering (ruled by `[os.random.trap]`, the `[mem.str.repeat]`
//! ruled-trap precedent), and `os_random` types as plain `List[int]`
//! with no error row at all.

use crate::fs::write_bytes_list;

/// Wire codes the WIR lowering keys on. ANY nonzero code is the
/// deterministic trap `assert` at the call site — there is no row to
/// map onto, by design (`[os.random.trap]`).
pub mod rand_code {
    /// The buffer is filled with OS entropy.
    pub const OK: i64 = 0;
    /// The OS did not provide bytes (or the platform has no backend,
    /// or the count was negative — the caller contract). TRAP.
    pub const FAIL: i64 = 1;
}

/// Fill `buf` entirely with OS-provided entropy. `true` iff every
/// byte was written by the platform call; a partial fill is never
/// exposed (the buffer must be treated as garbage on `false`).
pub fn fill(buf: &mut [u8]) -> bool {
    imp::fill(buf)
}

#[cfg(target_os = "linux")]
mod imp {
    /// `getrandom(2)`, flags 0: blocks only until the kernel pool
    /// initializes (the early-boot contract), never after. EINTR
    /// retries; a short read continues at the boundary.
    pub fn fill(buf: &mut [u8]) -> bool {
        let mut done = 0usize;
        while done < buf.len() {
            let rest = &mut buf[done..];
            // SAFETY: live buffer, correct length, flags 0.
            let n = unsafe { libc::getrandom(rest.as_mut_ptr().cast(), rest.len(), 0) };
            if n < 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                return false;
            }
            done += n as usize;
        }
        true
    }
}

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
mod imp {
    /// `getentropy(3)`: at most 256 bytes per call by contract (more
    /// is EIO), so the loop is the API's own shape.
    pub fn fill(buf: &mut [u8]) -> bool {
        for chunk in buf.chunks_mut(256) {
            // SAFETY: live chunk, length <= 256 by construction.
            if unsafe { libc::getentropy(chunk.as_mut_ptr().cast(), chunk.len()) } != 0 {
                return false;
            }
        }
        true
    }
}

#[cfg(windows)]
mod imp {
    #[link(name = "bcrypt")]
    unsafe extern "system" {
        /// The documented modern Windows CSPRNG entry (bcrypt.dll).
        fn BCryptGenRandom(
            halgorithm: *mut core::ffi::c_void,
            pbbuffer: *mut u8,
            cbbuffer: u32,
            dwflags: u32,
        ) -> i32;
    }

    /// Use the system-preferred RNG (null algorithm handle).
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;

    /// `BCryptGenRandom`: NTSTATUS 0 is success. Chunked so a length
    /// can never overflow the u32 the API takes.
    pub fn fill(buf: &mut [u8]) -> bool {
        for chunk in buf.chunks_mut(1 << 30) {
            // SAFETY: live chunk; length fits u32 by construction.
            let status = unsafe {
                BCryptGenRandom(
                    std::ptr::null_mut(),
                    chunk.as_mut_ptr(),
                    chunk.len() as u32,
                    BCRYPT_USE_SYSTEM_PREFERRED_RNG,
                )
            };
            if status != 0 {
                return false;
            }
        }
        true
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    windows
)))]
mod imp {
    /// The NAMED gap: no backend on this platform yet, so the surface
    /// TRAPS (`[os.random.platform]`). Never a PRNG, never silence.
    pub fn fill(_buf: &mut [u8]) -> bool {
        false
    }
}

// ---- the C entry surface -------------------------------------------------

/// `os_random(n: int) -> List[int]` — mint a fresh `n`-byte `List[int]`
/// of OS entropy through the out slot. Codes: 0 ok; ANY nonzero code is
/// the trap `assert` at the call site (`[os.random.trap]`) — a negative
/// `n` (the caller contract, the `[mem.str.repeat]` posture) and an OS
/// that cannot provide bytes both land there, because neither has a
/// legitimate continue. `n == 0` is the empty list — a valid request
/// for no entropy, not an error.
///
/// # Safety
///
/// `out` must address 8 writable bytes (the list header word).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_random(n: i64, out: i64) -> i64 {
    let Ok(len) = usize::try_from(n) else {
        return rand_code::FAIL; // n < 0: caller contract — trap
    };
    if len == 0 {
        unsafe { write_bytes_list(out, b"") };
        return rand_code::OK;
    }
    let mut buf = vec![0u8; len];
    if !fill(&mut buf) {
        return rand_code::FAIL; // no entropy — trap, never a fallback
    }
    unsafe { write_bytes_list(out, &buf) };
    rand_code::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list_i64(hdr: i64) -> Vec<i64> {
        unsafe { crate::list::i64_elems(hdr) }
            .expect("a valid List[int] header")
            .to_vec()
    }

    /// Two draws differ. This is the WEAKEST honest property a unit
    /// test can assert about an entropy source without becoming a
    /// statistical instrument (which a unit test must not be — a flaky
    /// distribution assertion is worse than none): equal 32-byte draws
    /// from a working CSPRNG have probability 2^-256, so equality here
    /// means the source is broken, not unlucky.
    #[test]
    fn two_draws_differ() {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        assert!(fill(&mut a), "OS entropy must be available on CI hosts");
        assert!(fill(&mut b), "OS entropy must be available on CI hosts");
        assert_ne!(a, b, "two 32-byte OS draws returned identical bytes");
    }

    /// The zero-length request: valid, fills nothing, succeeds.
    #[test]
    fn zero_length_fill_succeeds() {
        assert!(fill(&mut []));
    }

    /// A large request (past every per-call platform cap: getrandom's
    /// 256-byte no-interrupt window, getentropy's 256-byte limit) is
    /// filled completely — the loop owns the boundary, the caller
    /// never sees a short fill.
    #[test]
    fn large_fill_is_complete_and_nonzero() {
        let mut buf = vec![0u8; 65_536];
        assert!(fill(&mut buf));
        // 64 KiB of CSPRNG output is all-zero with probability
        // 2^-524288: a zero buffer means "nothing was written", not
        // an unlucky draw.
        assert!(buf.iter().any(|&b| b != 0), "large fill wrote nothing");
    }

    /// The shim: a fresh List[int] of exactly n in-range elements;
    /// two shim draws differ (the same weakest-honest property at the
    /// ABI boundary); n == 0 is the empty list; n < 0 is the FAIL
    /// code lowering turns into trap(assert) — witnessed here as a
    /// code because the trap spelling lives in lowering, not in this
    /// shim (`[os.random.trap]`).
    #[test]
    fn shim_mints_lists_and_refuses_negative() {
        let mut out = [0i64; 1];
        let o = out.as_mut_ptr() as i64;
        assert_eq!(unsafe { __wolf_rt_os_random(16, o) }, rand_code::OK);
        let a = list_i64(out[0]);
        assert_eq!(a.len(), 16);
        assert!(a.iter().all(|&v| (0..=255).contains(&v)));
        assert_eq!(unsafe { __wolf_rt_os_random(16, o) }, rand_code::OK);
        let b = list_i64(out[0]);
        assert_ne!(a, b, "two 16-byte shim draws returned identical bytes");
        assert_eq!(unsafe { __wolf_rt_os_random(0, o) }, rand_code::OK);
        assert_eq!(list_i64(out[0]), Vec::<i64>::new());
        assert_eq!(unsafe { __wolf_rt_os_random(-1, o) }, rand_code::FAIL);
    }
}
