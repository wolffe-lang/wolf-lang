//! wolf-lang#471: exit 2 from `wolf` means two things, and a driver gate
//! must tell them apart before it skips.
//!
//! `wolf` exits 2 when the host cannot run a lane (no `cc`, no runtime
//! library, a tier that refuses the platform) — the environment refusal
//! every native and release gate here skips loudly (the s59 pattern).
//! It ALSO exits 2 on an internal compiler error: every ICE the driver
//! reports is a stderr line `wolf <command>: ICE: …` followed by
//! `std::process::exit(2)` (lowered WIR failing verification, the
//! mid-end breaking the module, a backend `Internal` error, a native
//! binary killed by a signal under `conform-run`). Read as a skip, an
//! ICE on the native or release lane was a green gate — eg02 measured
//! it on wolf-lang#470's release ICE (`element_places_lanes.rs`).
//!
//! [`environment_refusal`] is the one test every gate asks before it
//! skips: `true` only for an exit 2 that names no ICE; an exit 2 that
//! names one FAILS the test here, with the ICE in the message.
//!
//! Included by path (`mod lane_exit;`) in each gate file under
//! `crates/wolf_driver/tests/` that skips on exit 2; it is not a test
//! target of its own (`lane_exit_helper.rs` tests it).

use std::process::Output;

/// The marker every driver ICE carries on stderr: `wolf build: ICE: …`,
/// `wolf conform-run: ICE: …`, `wolf fix: ICE: …`.
const ICE_MARKER: &str = ": ICE:";

/// Did `wolf` exit 2 because the host cannot run the lane? `false` for
/// any other exit. An exit 2 whose stderr names an internal compiler
/// error is never an environment refusal: the calling test fails here,
/// naming `what` (the invocation, for the message) and the ICE.
pub fn environment_refusal(out: &Output, what: &str) -> bool {
    if out.status.code() != Some(2) {
        return false;
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if let Some(line) = stderr.lines().find(|l| l.contains(ICE_MARKER)) {
        panic!(
            "{what} hit an internal compiler error (exit 2), not an environment \
             refusal — an ICE fails the gate, never skips it (wolf-lang#471): {line}\n\
             full stderr:\n{}",
            stderr.trim()
        );
    }
    true
}
