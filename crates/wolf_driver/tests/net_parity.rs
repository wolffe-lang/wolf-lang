//! s39 — the net row-tag mapping pinned across the seam: wolf_rt and
//! wolf_mem may not see each other (the locked graph; D15), so each
//! carries the `io::ErrorKind` → tag table by hand, and THIS test is
//! the single place that proves they never drift — the `fmt_parity`
//! precedent, applied to sockets.

use std::io::ErrorKind;

/// Every kind the mapping distinguishes plus a sample of the coarsened
/// remainder. (ErrorKind is non-exhaustive; the mapping's default arm
/// makes new kinds `io` on both sides by construction.)
const KINDS: &[ErrorKind] = &[
    ErrorKind::ConnectionRefused,
    ErrorKind::TimedOut,
    ErrorKind::WouldBlock,
    ErrorKind::ConnectionReset,
    ErrorKind::ConnectionAborted,
    ErrorKind::BrokenPipe,
    ErrorKind::NotConnected,
    ErrorKind::NotFound,
    ErrorKind::PermissionDenied,
    ErrorKind::AddrInUse,
    ErrorKind::AddrNotAvailable,
    ErrorKind::InvalidInput,
    ErrorKind::UnexpectedEof,
    ErrorKind::Interrupted,
    ErrorKind::OutOfMemory,
];

#[test]
fn runtime_and_checked_lane_agree_on_every_tag() {
    for &k in KINDS {
        assert_eq!(
            wolf_rt::net::err_tag(k),
            wolf_mem::ubcheck::net_err_tag(k),
            "net row tag diverged for {k:?}",
        );
    }
}

/// The vocabulary itself is closed: v0 tags are exactly
/// {refused, timeout, closed, io} — the contract's row classes.
#[test]
fn the_tag_vocabulary_is_closed() {
    let mut seen: Vec<&str> = KINDS.iter().map(|&k| wolf_rt::net::err_tag(k)).collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen, ["closed", "io", "refused", "timeout"]);
}
