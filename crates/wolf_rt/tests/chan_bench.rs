//! Linux-only (the task layer's platform posture): the real body lives
//! in linux_only/chan_bench.rs — a subdirectory so cargo does not treat it
//! as its own target; off-linux this target is an empty main.
#[cfg(target_os = "linux")]
#[path = "linux_only/chan_bench.rs"]
mod imp;

#[cfg(target_os = "linux")]
fn main() {
    imp::main()
}

#[cfg(not(target_os = "linux"))]
fn main() {}
