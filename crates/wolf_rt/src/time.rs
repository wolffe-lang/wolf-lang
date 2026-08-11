//! The s40 native time runtime — ms integers, X12 posture.
//!
//! Semantics are `ubcheck.rs`'s `time_builtin`, entry for entry:
//! `time_now_ms` is MONOTONIC from an arbitrary process-local anchor
//! (values are durations to compare/subtract, never wall timestamps),
//! `time_unix_ms` is the wall clock since the Unix epoch,
//! `time_sleep_ms` blocks the calling thread.
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
/// durations return immediately.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_time_sleep_ms(ms: i64) {
    if ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(ms as u64));
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

    #[test]
    fn unix_is_after_2020() {
        // 2020-01-01 in ms — a loose sanity bound, not a wall assert.
        assert!(__wolf_rt_time_unix_ms() > 1_577_836_800_000);
    }
}
