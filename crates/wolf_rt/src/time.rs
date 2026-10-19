//! The s40 native time runtime — ms integers, X12 posture.
//!
//! Semantics are `ubcheck.rs`'s `time_builtin`, entry for entry:
//! `time_now_ms` is MONOTONIC from an arbitrary process-local anchor
//! (values are durations to compare/subtract, never wall timestamps),
//! `time_unix_ms` is the wall clock since the Unix epoch,
//! `time_sleep_ms` blocks the calling thread. s248 (wolf-lang#798,
//! `[os.clock.ns]`): `os_clock_ns` is the same monotonic clock, from the
//! same anchor, in nanoseconds — so `os_clock_ns() / 1_000_000` and
//! `time_now_ms()` read the same clock.
//!
//! # The X12 seam
//!
//! [`now_anchor`] is the ONE monotonic clock read in this module —
//! the site the s36 deterministic scheduler virtualizes when its
//! clock-hook seam widens from timers to clock READS (today the det
//! engine virtualizes timer fire order; a virtual `now` is the
//! tracked campaign-closeout item). Nothing else in `wolf_rt` may
//! read the clock for program-visible values; the CI grep-gate over
//! the intrinsics allowlist (s37) starts from this inventory.

use std::sync::OnceLock;
use std::time::Instant;

/// The process-local monotonic anchor — the one program-visible clock
/// read (see the module doc). Fixed at first use.
fn now_anchor() -> Instant {
    static T0: OnceLock<Instant> = OnceLock::new();
    *T0.get_or_init(Instant::now)
}

/// `time_now_ms() -> int` — monotonic ms since the anchor.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_time_now_ms() -> i64 {
    now_anchor().elapsed().as_millis().min(i64::MAX as u128) as i64
}

/// `os_clock_ns() -> int` (s248, wolf-lang#798, `[os.clock.ns]`) — the
/// monotonic clock in nanoseconds since the anchor [`__wolf_rt_time_now_ms`]
/// reads: the host's finest monotonic clock (`clock_gettime(CLOCK_MONOTONIC)`
/// on linux, the mach clock on macOS, `QueryPerformanceCounter` on
/// windows, as `std::time::Instant` reads them). An i64 of nanoseconds
/// lasts 292 years from the anchor; it saturates there.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_clock_ns() -> i64 {
    now_anchor().elapsed().as_nanos().min(i64::MAX as u128) as i64
}

/// `time_unix_ms() -> int` — wall-clock ms since the Unix epoch (0 if
/// the host clock sits before it).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_time_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

/// `time_sleep_ms(ms)` — blocks the calling thread; non-positive
/// durations return immediately. A task's sleep holds its worker, so
/// the pool compensates as for any wait (s231: until then a sleeping
/// task's worker was simply gone for the duration — two sleeping tasks
/// on a one-cpu host left no worker for anything else).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_time_sleep_ms(ms: i64) {
    if ms > 0 {
        crate::host_wait(|| {
            std::thread::sleep(std::time::Duration::from_millis(ms as u64));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_and_sleep() {
        let a = __wolf_rt_time_now_ms();
        __wolf_rt_time_sleep_ms(2);
        let b = __wolf_rt_time_now_ms();
        assert!(a >= 0);
        assert!(b > a, "sleep advanced the monotonic read");
        __wolf_rt_time_sleep_ms(0); // immediate
        __wolf_rt_time_sleep_ms(-5); // immediate, never a panic
    }

    /// s248 (#798): nanoseconds from the anchor the ms clock reads — a
    /// ~100 us piece of work is never 0 ns, and the two clocks agree to
    /// the millisecond.
    #[test]
    fn ns_resolves_what_ms_cannot() {
        let mut zeros = 0;
        for _ in 0..20 {
            let a = __wolf_rt_os_clock_ns();
            let mut acc = 0u64;
            for i in 0..20_000u64 {
                acc = acc.wrapping_add(std::hint::black_box(i % 7));
            }
            std::hint::black_box(acc);
            let b = __wolf_rt_os_clock_ns();
            assert!(b >= a, "monotonic");
            if b == a {
                zeros += 1;
            }
        }
        assert_eq!(zeros, 0, "a ~100 us round measured as 0 ns");
        let ms = __wolf_rt_time_now_ms();
        let ns = __wolf_rt_os_clock_ns();
        assert!(ns / 1_000_000 >= ms, "the same anchor: {ns} ns, {ms} ms");
        assert!(ns / 1_000_000 - ms <= 1_000, "the same anchor: {ns} ns, {ms} ms");
    }

    #[test]
    fn unix_is_after_2020() {
        // 2020-01-01 in ms — a loose sanity bound, not a wall assert.
        assert!(__wolf_rt_time_unix_ms() > 1_577_836_800_000);
    }
}
